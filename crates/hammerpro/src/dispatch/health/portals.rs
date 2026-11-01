use std::path::{Path, PathBuf};
use std::time::SystemTime;

use crossbeam_channel::{Receiver, TryRecvError};
use hp_app::{AppState, Severity, t, tf};
use hp_compile::{BspMode, CompileEvent, CompileHandle, CompilePlan, PlanOptions, Preset, RadMode, StepModes, VisMode};
use hp_editor::compile_export::CompileExport;
use hp_editor::limits::cost::{CostLumps, RenderCosts};
use hp_editor::limits::portals::{BspVis, MAX_PRT_BYTES, PortalFile, lump, parse_prt};
use hp_editor::limits::world::{self, BspWorld, WorldLumps};
use hp_editor::limits::world_light::{self, LightMode, PropLighting, PropLumps, StaticPropLight};

const MAX_BSP_BYTES: u64 = 2 * 1024 * 1024 * 1024;
const BUILD_DIR: &str = ".hammerpro_build";
const PORTAL_DIR: &str = "portals";

struct BspLoad {
    vis: Option<BspVis>,
    costs: Option<RenderCosts>,
    world: Option<BspWorld>,
    counts: Vec<(&'static str, u64)>,
    areas: usize,
    clusters: u32,
    modified: u64,
}

enum Build {
    Writing { rx: Receiver<Result<PathBuf, String>>, workdir: PathBuf },
    Running { handle: CompileHandle, prt: PathBuf, last_error: Option<String> },
}

#[derive(Default)]
pub(super) struct Portals {
    pick: Option<Receiver<Option<PathBuf>>>,
    prt: Option<Receiver<(PathBuf, Result<PortalFile, String>)>>,
    bsp: Option<Receiver<(PathBuf, Result<BspLoad, String>)>>,
    vis_wanted: bool,
    build: Option<Build>,
}

fn modified(path: &Path) -> Option<SystemTime> {
    std::fs::metadata(path).ok()?.modified().ok()
}

fn unix_secs(t: Option<SystemTime>) -> u64 {
    t.and_then(|t| t.duration_since(std::time::UNIX_EPOCH).ok()).map_or(0, |d| d.as_secs())
}

fn newest(paths: Vec<PathBuf>) -> Option<PathBuf> {
    paths.into_iter().filter_map(|p| Some((modified(&p)?, p))).max_by_key(|(t, _)| *t).map(|(_, p)| p)
}

fn compiled_files(state: &AppState, ext: &str) -> Vec<PathBuf> {
    let Some(map) = state.doc.path() else { return Vec::new() };
    let (Some(dir), Some(stem)) = (map.parent(), map.file_stem()) else { return Vec::new() };
    let name = format!("{}.{ext}", stem.to_string_lossy());
    let mut out = vec![dir.join(BUILD_DIR).join(&name), dir.join(&name)];
    if ext == "prt" {
        out.push(dir.join(BUILD_DIR).join(PORTAL_DIR).join(&name));
    }
    if ext == "bsp"
        && let Some(maps) = state
            .settings
            .compile
            .maps_dir_override
            .clone()
            .or_else(|| state.assets.fs().and_then(|fs| fs.root()).map(|r| r.join("garrysmod").join("maps")))
    {
        out.push(maps.join(&name));
    }
    out
}

fn read_prt(path: &Path) -> Result<PortalFile, String> {
    let len = std::fs::metadata(path).map_err(|e| e.to_string())?.len();
    if len > MAX_PRT_BYTES {
        return Err(format!("{len} bytes"));
    }
    let bytes = std::fs::read(path).map_err(|e| e.to_string())?;
    parse_prt(&String::from_utf8_lossy(&bytes)).map_err(|e| e.to_string())
}

const AREAS_LUMP: usize = 20;
const AREA_PORTALS_LUMP: usize = 21;
const CLIP_PORTAL_VERTS_LUMP: usize = 41;
const MAP_FLAGS_LUMP: usize = 59;

fn read_bsp(path: &Path) -> Result<BspLoad, String> {
    let meta = std::fs::metadata(path).map_err(|e| e.to_string())?;
    if meta.len() > MAX_BSP_BYTES {
        return Err(format!("{} bytes", meta.len()));
    }
    let bsp = hp_bsp::Bsp::open(path).map_err(|e| e.to_string())?;
    let counts: Vec<(&'static str, u64)> = bsp.limit_usage().iter().map(|u| (u.name, u.used)).collect();
    let lump = |i: usize| bsp.lump(i).map(|c| c.into_owned()).map_err(|e| e.to_string());
    let leaf_version = bsp.lump_info(lump::LEAFS).map_err(|e| e.to_string())?.version;
    let mut vis = BspVis::from_lumps(
        &lump(lump::PLANES)?,
        &lump(lump::NODES)?,
        &lump(lump::LEAFS)?,
        leaf_version,
        &lump(lump::MODELS)?,
        &lump(lump::VISIBILITY)?,
    )
    .ok();
    if let Some(v) = vis.as_mut() {
        let entities: Vec<hp_editor::limits::portals::PortalEntity> = bsp
            .entities()
            .unwrap_or_default()
            .iter()
            .filter_map(|e| {
                let class = e.get("classname")?.to_ascii_lowercase();
                let window = class == "func_areaportalwindow";
                if class != "func_areaportal" && !window {
                    return None;
                }
                Some(hp_editor::limits::portals::PortalEntity {
                    number: e.get("portalnumber")?.trim().parse().ok()?,
                    door: e.get("target").unwrap_or_default().trim().to_owned(),
                    name: e.get("targetname").unwrap_or_default().trim().to_owned(),
                    start_open: e.get("StartOpen").is_none_or(|v| v.trim() != "0"),
                    window,
                    fade_dist: e
                        .get("FadeDist")
                        .and_then(|v| v.trim().parse::<f64>().ok())
                        .filter(|v| v.is_finite() && *v > 0.0)
                        .map_or(0, |v| v.min(1.0e6) as u32),
                })
            })
            .collect();
        v.set_area_portals(
            &lump(AREAS_LUMP).unwrap_or_default(),
            &lump(AREA_PORTALS_LUMP).unwrap_or_default(),
            &lump(CLIP_PORTAL_VERTS_LUMP).unwrap_or_default(),
            entities,
        );
    }
    let areas = vis.as_ref().map_or(0, BspVis::area_count);
    let clusters = vis.as_ref().map_or(0, BspVis::cluster_count);
    let costs = match &vis {
        Some(v) if v.has_visibility() => {
            let props = bsp
                .game_lumps()
                .ok()
                .and_then(|g| {
                    g.get(hp_bsp::gamelump::id::STATIC_PROPS).and_then(|p| p.data().ok().map(|d| d.into_owned()))
                })
                .unwrap_or_default();
            let leaf_faces = lump(lump::LEAF_FACES).unwrap_or_default();
            let faces = lump(lump::FACES).unwrap_or_default();
            let texinfo = lump(lump::TEXINFO).unwrap_or_default();
            let dispinfo = lump(lump::DISPINFO).unwrap_or_default();
            let overlays = lump(lump::OVERLAYS).unwrap_or_default();
            let leafs = lump(lump::LEAFS)?;
            let l = CostLumps {
                leafs: &leafs,
                leaf_version,
                leaf_faces: &leaf_faces,
                faces: &faces,
                texinfo: &texinfo,
                dispinfo: &dispinfo,
                overlays: &overlays,
                static_props: &props,
            };
            Some(RenderCosts::from_lumps(&l, v))
        }
        _ => None,
    };
    let world = read_world(&bsp, vis.as_ref(), leaf_version).map(|mut w| {
        w.cubemaps = read_cubemaps(&bsp, path);
        w
    });
    Ok(BspLoad { vis, costs, world, counts, areas, clusters, modified: unix_secs(meta.modified().ok()) })
}

pub(crate) fn bake_prop_lighting(bsp: &hp_bsp::Bsp) -> Option<PropLighting> {
    let lump = |i: usize| bsp.lump(i).map(|c| c.into_owned()).unwrap_or_default();
    let leaf_version = bsp.lump_info(lump::LEAFS).ok()?.version;
    let vis = BspVis::from_lumps(
        &lump(lump::PLANES),
        &lump(lump::NODES),
        &lump(lump::LEAFS),
        leaf_version,
        &lump(lump::MODELS),
        &lump(lump::VISIBILITY),
    )
    .ok();
    read_world(bsp, vis.as_ref(), leaf_version).map(|w| w.light.props)
}

fn read_cubemaps(bsp: &hp_bsp::Bsp, path: &Path) -> Vec<world::WorldCubemap> {
    let samples = bsp.cubemaps().unwrap_or_default();
    if samples.is_empty() {
        return Vec::new();
    }
    let map = path.file_stem().map(|s| s.to_string_lossy().into_owned()).unwrap_or_default();
    let origins: Vec<[i32; 3]> = samples.iter().map(|(o, _)| *o).collect();
    let (built, boxes) = match bsp.pakfile() {
        Ok(pak) => (hp_bsp::cubemaps::read_built(&pak, &map, &origins), hp_assets::parallax::boxes_of_pakfile(&pak)),
        Err(_) => (Vec::new(), Default::default()),
    };
    samples
        .iter()
        .enumerate()
        .map(|(i, (origin, size))| {
            let [ldr, hdr] = built.get(i).cloned().unwrap_or_default();
            world::WorldCubemap {
                origin: *origin,
                size: *size,
                ldr: ldr.map(std::sync::Arc::new),
                hdr: hdr.map(std::sync::Arc::new),
                parallax: boxes.get(origin).map(|b| (b.rows, b.origin)),
            }
        })
        .collect()
}

const LIGHT_CLASSES: [&str; 4] = ["light", "light_spot", "light_glspot", "light_environment"];

fn read_world(bsp: &hp_bsp::Bsp, vis: Option<&BspVis>, leaf_version: u32) -> Option<BspWorld> {
    let lump = |i: usize| bsp.lump(i).map(|c| c.into_owned()).unwrap_or_default();
    let mut faces = lump(lump::FACES);
    if faces.is_empty() {
        faces = lump(world::lump::FACES_HDR);
    }
    let (planes, vertexes, edges, surfedges) =
        (lump(lump::PLANES), lump(world::lump::VERTEXES), lump(world::lump::EDGES), lump(world::lump::SURFEDGES));
    let (texinfo, texdata) = (lump(lump::TEXINFO), lump(world::lump::TEXDATA));
    let (string_table, string_data) = (lump(world::lump::TEXDATA_STRING_TABLE), lump(world::lump::TEXDATA_STRING_DATA));
    let (models, leafs, leaf_faces) = (lump(lump::MODELS), lump(lump::LEAFS), lump(lump::LEAF_FACES));
    let (dispinfo, disp_verts) = (lump(lump::DISPINFO), lump(world::lump::DISP_VERTS));
    let disp_multiblend = lump(world::lump::DISP_MULTIBLEND);
    let (ldr, hdr) = (lump(world_light::lump::LIGHTING), lump(world_light::lump::LIGHTING_HDR));
    let hdr_faces = lump(world_light::lump::FACES_HDR);
    let (mode, lighting, light_faces) = if !hdr.is_empty() && hdr_faces.len() == faces.len() {
        (LightMode::Hdr, hdr, hdr_faces)
    } else if !ldr.is_empty() {
        (LightMode::Ldr, ldr, Vec::new())
    } else {
        (LightMode::Fullbright, Vec::new(), Vec::new())
    };
    let entities = bsp.entities().unwrap_or_default();
    let int = |e: &hp_bsp::entities::Entity, k: &str| e.get(k).and_then(|v| v.trim().parse::<i64>().ok()).unwrap_or(0);
    let styles = world_light::StyleTable::from_entities(
        entities
            .iter()
            .filter(|e| {
                e.get("classname").is_some_and(|c| LIGHT_CLASSES.iter().any(|k| c.trim().eq_ignore_ascii_case(k)))
            })
            .map(|e| world_light::LightEntity {
                name: e.get("targetname").unwrap_or_default(),
                style: int(e, "style").clamp(-1, 1 << 16) as i32,
                default_style: int(e, "defaultstyle").clamp(-1, 1 << 16) as i32,
                start_dark: int(e, "spawnflags") & 1 != 0,
                pattern: e.get("pattern").unwrap_or_default(),
            }),
    );
    let scales = styles.values(0.0, &styles.initial());
    let vec3 = |e: &hp_bsp::entities::Entity, k: &str| -> [f32; 3] {
        let v: Vec<f32> = e.get(k).unwrap_or_default().split_whitespace().filter_map(|x| x.parse().ok()).collect();
        if v.len() == 3 && v.iter().all(|x| x.is_finite()) { [v[0], v[1], v[2]] } else { [0.0; 3] }
    };
    let brush_entities: Vec<world::BrushEntity> = entities
        .iter()
        .filter_map(|e| {
            let model = e.get("model")?.trim().strip_prefix('*')?.parse::<u32>().ok()?;
            let id = e.get("hammerid")?.trim().parse::<u32>().ok()?;
            Some(world::BrushEntity { model, id, origin: vec3(e, "origin"), angles: vec3(e, "angles") })
        })
        .collect();
    let l = WorldLumps {
        planes: &planes,
        vertexes: &vertexes,
        edges: &edges,
        surfedges: &surfedges,
        faces: &faces,
        texinfo: &texinfo,
        texdata: &texdata,
        string_table: &string_table,
        string_data: &string_data,
        models: &models,
        leafs: &leafs,
        leaf_version,
        leaf_faces: &leaf_faces,
        dispinfo: &dispinfo,
        disp_verts: &disp_verts,
        disp_multiblend: &disp_multiblend,
        lighting: &lighting,
        light_faces: &light_faces,
        light_mode: mode,
        brush_entities: &brush_entities,
    };
    let mut w = BspWorld::from_lumps(&l, vis);
    if w.faces.is_empty() {
        return None;
    }
    w.light.styles = styles;
    add_details(bsp, &mut w, &leafs, leaf_version, mode, &scales);
    add_static_props(bsp, &mut w, &leafs, leaf_version);
    let mut ids: Vec<u32> =
        entities.iter().filter_map(|e| e.get("hammerid").and_then(|v| v.trim().parse::<u32>().ok())).collect();
    ids.sort_unstable();
    ids.dedup();
    w.entity_ids = ids;
    w.rope_entities = rope_entities(&entities);
    w.particle_entities = particle_entities(&entities);
    w.light.props = read_prop_lighting(bsp, &w, vis, mode, &scales, &planes, &leafs, leaf_version, &models);
    Some(w)
}

fn rope_entities(entities: &[hp_bsp::entities::Entity]) -> Vec<hp_vmf::Entity> {
    let is_rope = |c: &str| {
        let c = c.trim();
        c.eq_ignore_ascii_case("move_rope") || c.eq_ignore_ascii_case("keyframe_rope")
    };
    if !entities.iter().any(|e| is_rope(e.classname())) {
        return Vec::new();
    }
    entities
        .iter()
        .filter(|e| is_rope(e.classname()) || e.get("targetname").is_some_and(|n| !n.trim().is_empty()))
        .map(|e| hp_vmf::Entity {
            classname: e.classname().to_owned(),
            props: e.pairs.iter().filter(|(k, _)| !k.eq_ignore_ascii_case("classname")).cloned().collect(),
            ..hp_vmf::Entity::default()
        })
        .collect()
}

fn particle_entities(entities: &[hp_bsp::entities::Entity]) -> Vec<hp_vmf::Entity> {
    let is_system = |e: &hp_bsp::entities::Entity| e.classname().trim().eq_ignore_ascii_case("info_particle_system");
    let mut targets: std::collections::HashSet<String> = std::collections::HashSet::new();
    for e in entities.iter().filter(|e| is_system(e)) {
        for (k, v) in &e.pairs {
            if k.to_ascii_lowercase().starts_with("cpoint") && !v.trim().is_empty() {
                targets.insert(v.trim().to_ascii_lowercase());
            }
        }
    }
    entities
        .iter()
        .filter(|e| {
            is_system(e) || e.get("targetname").is_some_and(|n| targets.contains(&n.trim().to_ascii_lowercase()))
        })
        .map(|e| hp_vmf::Entity {
            classname: e.classname().to_owned(),
            props: e.pairs.iter().filter(|(k, _)| !k.eq_ignore_ascii_case("classname")).cloned().collect(),
            ..hp_vmf::Entity::default()
        })
        .collect()
}

#[allow(clippy::too_many_arguments)]
fn read_prop_lighting(
    bsp: &hp_bsp::Bsp,
    w: &BspWorld,
    vis: Option<&BspVis>,
    mode: LightMode,
    scales: &[f32; world_light::MAX_STYLES],
    planes: &[u8],
    leafs: &[u8],
    leaf_version: u32,
    models: &[u8],
) -> PropLighting {
    use world_light::lump as wl;
    let lump = |i: usize| bsp.lump(i).map(|c| c.into_owned()).unwrap_or_default();
    let hdr = mode == LightMode::Hdr;
    let (index, ambient, lights) = if hdr {
        (lump(wl::LEAF_AMBIENT_INDEX_HDR), lump(wl::LEAF_AMBIENT_LIGHTING_HDR), lump(wl::WORLDLIGHTS_HDR))
    } else {
        (lump(wl::LEAF_AMBIENT_INDEX), lump(wl::LEAF_AMBIENT_LIGHTING), lump(wl::WORLDLIGHTS))
    };
    let lights_version =
        bsp.lump_info(if hdr { wl::WORLDLIGHTS_HDR } else { wl::WORLDLIGHTS }).map_or(0, |l| l.version);
    let lights = hp_bsp::format::worldlights_as_v0(&lights, lights_version).into_owned();
    let nodes = lump(lump::NODES);
    let l = PropLumps {
        planes,
        nodes: &nodes,
        leafs,
        leaf_version,
        models,
        ambient_index: &index,
        ambient: &ambient,
        worldlights: &lights,
    };
    let mut p = PropLighting::from_lumps(&l, scales);
    p.set_sky_faces(w.faces.iter().filter(|f| f.sky).map(|f| w.bounds_of(f)).collect());
    p.set_visibility(vis.cloned().map(std::sync::Arc::new));
    if mode == LightMode::Fullbright {
        return p;
    }
    let (sprp, sprp_version) = bsp
        .game_lumps()
        .ok()
        .and_then(|g| {
            g.get(hp_bsp::gamelump::id::STATIC_PROPS).and_then(|p| p.data().ok().map(|d| (d.into_owned(), p.version)))
        })
        .unwrap_or_default();
    let flags = lump(MAP_FLAGS_LUMP).get(0..4).map_or(0, |b| u32::from_le_bytes([b[0], b[1], b[2], b[3]]));
    let baked = flags & if hdr { 2 } else { 1 } != 0;
    let pak = baked.then(|| bsp.pakfile().ok()).flatten();
    for (i, (model, origin, angles, prop_flags, lighting_origin)) in
        world_light::read_static_props_of(&sprp, sprp_version).into_iter().enumerate()
    {
        let colors = if prop_flags & 0x40 != 0 {
            None
        } else {
            let name = if hdr { format!("sp_hdr_{i}.vhv") } else { format!("sp_{i}.vhv") };
            pak.as_ref()
                .and_then(|pak| pak.read_name(&name).ok().flatten())
                .and_then(|d| world_light::read_vhv(&d))
                .map(std::sync::Arc::new)
        };
        p.static_props.push(StaticPropLight { model, origin, angles, lighting_origin, colors });
    }
    p
}

fn add_details(
    bsp: &hp_bsp::Bsp,
    w: &mut BspWorld,
    leafs: &[u8],
    leaf_version: u32,
    mode: LightMode,
    scales: &[f32; world_light::MAX_STYLES],
) {
    use hp_bspc::detail_props::{DPLH, DPLT, DPRP, DetailLump, kind, parse_styles};
    use hp_editor::limits::world::{DetailModelPlacement, DetailOrientation, DetailSprite};
    let games = bsp.game_lumps().ok();
    let Some(data) = games.as_ref().and_then(|g| g.get(DPRP).and_then(|l| l.data().ok().map(|d| d.into_owned())))
    else {
        return;
    };
    let style_lump = if mode == LightMode::Hdr { DPLH } else { DPLT };
    let records = games
        .as_ref()
        .and_then(|g| g.get(style_lump).and_then(|l| l.data().ok()))
        .and_then(|d| parse_styles(&d).map_err(|e| log::warn!("player view: {e}")).ok())
        .unwrap_or_default();
    let lump = match DetailLump::parse(&data) {
        Ok(l) => l,
        Err(e) => {
            log::warn!("player view: {e}");
            return;
        }
    };
    let leaf_size = if leaf_version == 0 { 56 } else { 32 };
    let cluster = |leaf: u16| -> Option<u32> {
        let o = usize::from(leaf) * leaf_size + 4;
        let c = leafs.get(o..o + 2).map(|b| i16::from_le_bytes([b[0], b[1]]))?;
        u32::try_from(c).ok()
    };
    let mut sprites = Vec::new();
    let mut models = Vec::new();
    for (i, o) in lump.objects.iter().enumerate() {
        let origin = o.origin.to_array();
        let angles = o.angles.to_array();
        if !origin.iter().chain(&angles).all(|v| v.is_finite()) {
            continue;
        }
        if o.kind == kind::MODEL {
            if let Some(model) = lump.models.get(usize::from(o.dict)) {
                let light = detail_light(o, &records, scales);
                let index = (w.detail_models.len() + models.len()) as u32;
                w.light.detail_styles.extend(detail_style_light(o, &records, true, index));
                models.push((
                    DetailModelPlacement { model: model.to_ascii_lowercase(), origin, angles, light, group: 0 },
                    cluster(o.leaf),
                ));
            }
            continue;
        }
        let Some(d) = lump.sprites.get(usize::from(o.dict)) else { continue };
        let scale = if o.scale.is_finite() { o.scale } else { 1.0 };
        let (right, up) = right_up(angles);
        let flipped = i % 2 == 1;
        let (u0, u1) = if flipped { (d[4], d[6]) } else { (d[6], d[4]) };
        let index = (w.details.len() + sprites.len()) as u32;
        w.light.detail_styles.extend(detail_style_light(o, &records, false, index));
        sprites.push((
            DetailSprite {
                origin,
                right,
                up,
                ul: [d[0] * scale, d[1] * scale],
                lr: [d[2] * scale, d[3] * scale],
                tex: [u0, d[5], u1, d[7]],
                orientation: match o.orientation {
                    1 => DetailOrientation::ScreenAligned,
                    2 => DetailOrientation::Vertical,
                    _ => DetailOrientation::Normal,
                },
                color: detail_color(detail_light(o, &records, scales)),
                group: 0,
            },
            cluster(o.leaf),
        ));
    }
    let entities = bsp.entities().unwrap_or_default();
    let find = |class: &str| {
        entities.iter().find(|e| e.get("classname").is_some_and(|c| c.trim().eq_ignore_ascii_case(class)))
    };
    w.detail_material =
        find("worldspawn").and_then(|e| e.get("detailmaterial").map(|m| m.trim().to_owned())).unwrap_or_default();
    let float = |e: &hp_bsp::entities::Entity, k: &str, default: f32| {
        e.get(k).and_then(|v| v.trim().parse::<f32>().ok()).filter(|v| v.is_finite()).unwrap_or(default)
    };
    w.detail_fade = world::detail_fade(
        find("env_detail_controller").map(|e| (float(e, "fademindist", 400.0), float(e, "fademaxdist", 1200.0))),
    );
    w.add_details(sprites, models);
}

fn add_static_props(bsp: &hp_bsp::Bsp, w: &mut BspWorld, leafs: &[u8], leaf_version: u32) {
    use hp_editor::limits::world::StaticPropPlacement;
    let Some((data, version)) = bsp.game_lumps().ok().and_then(|g| {
        g.get(hp_bsp::gamelump::id::STATIC_PROPS).and_then(|p| p.data().ok().map(|d| (d.into_owned(), p.version)))
    }) else {
        return;
    };
    let leaf_size = if leaf_version == 0 { 56 } else { 32 };
    let cluster = |leaf: u16| -> Option<u32> {
        let o = usize::from(leaf) * leaf_size + 4;
        let c = leafs.get(o..o + 2).map(|b| i16::from_le_bytes([b[0], b[1]]))?;
        u32::try_from(c).ok()
    };
    let props = world_light::read_static_prop_entries_of(&data, version)
        .into_iter()
        .map(|e| {
            let [min, max] = e.fade;
            let fade = if max > 0.0 { [if (0.0..=max).contains(&min) { min } else { max }, max] } else { [0.0; 2] };
            let clusters = e.leaves.iter().filter_map(|l| cluster(*l)).collect();
            (
                StaticPropPlacement {
                    model: e.model,
                    origin: e.origin,
                    angles: e.angles,
                    skin: e.skin,
                    fade,
                    group: 0,
                },
                clusters,
            )
        })
        .collect();
    w.add_static_props(props);
}

fn detail_light(
    o: &hp_bspc::detail_props::DetailObject,
    records: &[([u8; 4], u8)],
    scales: &[f32; world_light::MAX_STYLES],
) -> [f32; 3] {
    let scale = |s: u8| scales.get(usize::from(s)).copied().unwrap_or(0.0);
    let mut out = world_light::rgbe_to_linear(o.lighting).map(|c| c * scale(0));
    let first = usize::try_from(o.light_styles).unwrap_or(usize::MAX);
    let styles = records.get(first..).unwrap_or_default().iter().take(usize::from(o.light_style_count));
    for (c, style) in styles {
        let v = scale(*style);
        if v != 0.0 {
            let l = world_light::rgbe_to_linear(*c);
            for k in 0..3 {
                out[k] += v * l[k];
            }
        }
    }
    out
}

fn detail_style_light(
    o: &hp_bspc::detail_props::DetailObject,
    records: &[([u8; 4], u8)],
    model: bool,
    index: u32,
) -> Option<world_light::DetailStyleLight> {
    let first = usize::try_from(o.light_styles).unwrap_or(usize::MAX);
    let styles: Vec<(u8, [f32; 3])> = records
        .get(first..)
        .unwrap_or_default()
        .iter()
        .take(usize::from(o.light_style_count))
        .filter(|(_, s)| usize::from(*s) < world_light::MAX_STYLES)
        .map(|(c, s)| (*s, world_light::rgbe_to_linear(*c)))
        .collect();
    styles.iter().any(|(s, _)| *s != 0).then(|| world_light::DetailStyleLight {
        model,
        index,
        base: world_light::rgbe_to_linear(o.lighting),
        styles,
    })
}

fn detail_color(linear: [f32; 3]) -> [u8; 4] {
    let gamma = |v: f32| (if v.is_finite() { v } else { 0.0 }.clamp(0.0, 1.0).powf(1.0 / 2.2) * 255.0).round() as u8;
    [gamma(linear[0]), gamma(linear[1]), gamma(linear[2]), 255]
}

fn right_up(a: [f32; 3]) -> ([f32; 3], [f32; 3]) {
    let (sp, cp) = a[0].to_radians().sin_cos();
    let (sy, cy) = a[1].to_radians().sin_cos();
    let (sr, cr) = a[2].to_radians().sin_cos();
    let right = [-sr * sp * cy + cr * sy, -sr * sp * sy - cr * cy, -sr * cp];
    let up = [cr * sp * cy + sr * sy, cr * sp * sy - sr * cy, cr * cp];
    (right, up)
}

fn spawn<T: Send + 'static>(name: &str, f: impl FnOnce() -> T + Send + 'static) -> Option<Receiver<T>> {
    let (tx, rx) = crossbeam_channel::bounded(1);
    std::thread::Builder::new()
        .name(name.to_owned())
        .spawn(move || {
            let _ = tx.send(f());
        })
        .ok()?;
    Some(rx)
}

impl Portals {
    pub(super) fn map_loaded(&mut self) {
        if let Some(Build::Running { handle, .. }) = &self.build {
            handle.cancel();
        }
        *self = Self::default();
    }

    pub(super) fn cancel(&mut self, state: &mut AppState) {
        if let Some(Build::Running { handle, .. }) = &self.build {
            handle.cancel();
        }
        self.build = None;
        self.prt = None;
        state.runtime.health.portal_build_running = false;
        state.runtime.health.portal_build_request = false;
        state.runtime.health.portal_load_request = None;
        state.runtime.health.portal_reload_request = false;
        state.runtime.health.portal_cancel_request = false;
    }

    pub(super) fn load(&mut self, _state: &mut AppState, path: PathBuf) {
        let p = path.clone();
        self.prt = spawn("hp-health-prt", move || (p.clone(), read_prt(&p)));
    }

    pub(super) fn load_or_pick(&mut self, state: &mut AppState) {
        if let Some(path) = newest(compiled_files(state, "prt")) {
            self.load(state, path);
            return;
        }
        if self.pick.is_some() {
            return;
        }
        let mut dialog = rfd::FileDialog::new().add_filter(t!("app.health.portal_filter"), &["prt"]);
        if let Some(dir) = state.doc.path().and_then(Path::parent) {
            dialog = dialog.set_directory(dir);
        }
        let (tx, rx) = crossbeam_channel::bounded(1);
        if cfg!(target_os = "macos") {
            let _ = tx.send(dialog.pick_file());
        } else {
            std::thread::spawn(move || {
                let _ = tx.send(dialog.pick_file());
            });
        }
        self.pick = Some(rx);
    }

    pub(super) fn ensure_vis(&mut self, state: &mut AppState, quiet: bool) {
        self.vis_wanted = !quiet;
        let Some(path) = newest(compiled_files(state, "bsp")) else {
            if !quiet {
                state.notify(Severity::Info, t!("app.health.no_bsp"));
                state.runtime.health.pvs = false;
            }
            self.vis_wanted = false;
            return;
        };
        let fresh = state.runtime.health.vis_path.as_ref() == Some(&path)
            && state.runtime.health.exact.as_ref().is_some_and(|e| e.modified == unix_secs(modified(&path)));
        if fresh && state.runtime.health.vis.is_some() {
            self.vis_wanted = false;
            return;
        }
        self.read_bsp(path);
    }

    fn read_bsp(&mut self, path: PathBuf) {
        let p = path.clone();
        self.bsp = spawn("hp-health-bsp", move || (p.clone(), read_bsp(&p)));
    }

    pub(super) fn after_compile(&mut self, state: &mut AppState, since: Option<SystemTime>) {
        let new = |p: &PathBuf| since.is_none_or(|s| modified(p).is_some_and(|m| m >= s));
        if let Some(prt) = newest(compiled_files(state, "prt")).filter(new) {
            if state.runtime.health.portals.is_some() {
                self.load(state, prt);
            } else if state.settings.health.offer_portals {
                state.runtime.health.portal_offer = Some(prt);
            }
        }
        if let Some(bsp) = newest(compiled_files(state, "bsp")).filter(new) {
            self.read_bsp(bsp);
        }
    }

    fn start_build(&mut self, state: &mut AppState) {
        if self.build.is_some() {
            return;
        }
        let (workdir, name) = match state.doc.path() {
            Some(p) => (
                p.parent().map_or_else(|| PathBuf::from("."), Path::to_path_buf).join(BUILD_DIR).join(PORTAL_DIR),
                p.file_stem().map_or_else(|| "untitled".to_owned(), |s| s.to_string_lossy().into_owned()),
            ),
            None => (hp_app::settings::Settings::cache_dir().join(PORTAL_DIR), "untitled".to_owned()),
        };
        let snapshot = state.doc.compile_snapshot(&CompileExport::default());
        let dir = workdir.clone();
        let Some(rx) = spawn("hp-health-vmf", move || snapshot.write(&dir, &name).map_err(|e| e.to_string())) else {
            return;
        };
        state.runtime.health.portal_build_running = true;
        state.notify(Severity::Info, t!("app.health.vbsp_started"));
        self.build = Some(Build::Writing { rx, workdir });
    }

    fn tools(state: &AppState) -> Option<hp_compile::ToolPaths> {
        let root = state
            .assets
            .fs()
            .and_then(|fs| fs.root().map(Path::to_path_buf))
            .or_else(|| state.settings.game.gmod_root.clone())?;
        hp_compile::ToolPaths::detect(&root)
    }

    fn poll_build(&mut self, state: &mut AppState) {
        let Some(build) = self.build.take() else { return };
        match build {
            Build::Writing { rx, workdir } => match rx.try_recv() {
                Ok(Ok(vmf)) => {
                    let Some(tools) = Self::tools(state) else {
                        state.notify(Severity::Warning, t!("app.health.no_compiler"));
                        state.runtime.health.portal_build_running = false;
                        return;
                    };
                    let opts = PlanOptions {
                        modes: Some(StepModes::new(BspMode::Full, VisMode::Off, RadMode::Off)),
                        copy_to_maps: false,
                        launch_game: false,
                        content_args: hp_ui_mapping::compile_content_args(state.assets.fs()),
                        ..PlanOptions::default()
                    };
                    match CompilePlan::preset(Preset::Draft, &tools, &vmf, &opts) {
                        Ok(plan) => {
                            let prt = vmf.with_extension("prt");
                            let handle = hp_compile::start(plan, &workdir);
                            self.build = Some(Build::Running { handle, prt, last_error: None });
                        }
                        Err(e) => {
                            state.notify(Severity::Warning, tf!("app.health.vbsp_failed", e));
                            state.runtime.health.portal_build_running = false;
                        }
                    }
                }
                Ok(Err(e)) => {
                    state.notify(Severity::Warning, tf!("app.health.vbsp_failed", e));
                    state.runtime.health.portal_build_running = false;
                }
                Err(TryRecvError::Empty) => self.build = Some(Build::Writing { rx, workdir }),
                Err(TryRecvError::Disconnected) => state.runtime.health.portal_build_running = false,
            },
            Build::Running { handle, prt, mut last_error } => {
                loop {
                    match handle.events.try_recv() {
                        Ok(CompileEvent::Log { line, severity, .. }) => {
                            if severity == hp_compile::Severity::Error {
                                last_error = Some(line);
                            }
                        }
                        Ok(CompileEvent::Finished { success, leaked, .. }) => {
                            state.runtime.health.portal_build_running = false;
                            if leaked {
                                state.notify(Severity::Warning, t!("app.health.vbsp_leak"));
                            } else if success && prt.is_file() {
                                let bsp = prt.with_extension("bsp");
                                self.load(state, prt);
                                if bsp.is_file() {
                                    self.read_bsp(bsp);
                                }
                            } else {
                                let why = last_error.unwrap_or_else(|| "hammerpro --bspc".to_owned());
                                state.notify(Severity::Warning, tf!("app.health.vbsp_failed", why));
                            }
                            return;
                        }
                        Ok(_) => {}
                        Err(TryRecvError::Empty) => break,
                        Err(TryRecvError::Disconnected) => {
                            state.runtime.health.portal_build_running = false;
                            return;
                        }
                    }
                }
                self.build = Some(Build::Running { handle, prt, last_error });
            }
        }
    }

    pub(super) fn tick(&mut self, state: &mut AppState) -> bool {
        if std::mem::take(&mut state.runtime.health.portal_cancel_request) {
            self.cancel(state);
        }
        if let Some(path) = state.runtime.health.portal_load_request.take() {
            self.load(state, path);
        }
        if std::mem::take(&mut state.runtime.health.portal_reload_request) {
            let mut candidates = compiled_files(state, "prt");
            candidates.extend(state.runtime.health.portal_path.clone());
            if let Some(path) = newest(candidates) {
                self.load(state, path);
            }
        }
        if std::mem::take(&mut state.runtime.health.portal_build_request) {
            self.start_build(state);
        }
        if std::mem::take(&mut state.runtime.health.bsp_reload_request)
            && let Some(path) = state.runtime.health.exact.as_ref().map(|e| e.path.clone())
        {
            self.read_bsp(path);
        }
        if let Some(rx) = &self.pick {
            match rx.try_recv() {
                Ok(picked) => {
                    self.pick = None;
                    if let Some(path) = picked {
                        self.load(state, path);
                    }
                }
                Err(TryRecvError::Empty) => {}
                Err(TryRecvError::Disconnected) => self.pick = None,
            }
        }
        if let Some(rx) = &self.prt {
            match rx.try_recv() {
                Ok((path, result)) => {
                    self.prt = None;
                    match result {
                        Ok(file) => {
                            let n = file.portals.len();
                            let one = match hp_app::i18n::lang() {
                                hp_app::i18n::Lang::Fr => n <= 1,
                                hp_app::i18n::Lang::En => n == 1,
                            };
                            let key = if one { "app.health.portals_loaded_one" } else { "app.health.portals_loaded" };
                            state.notify(Severity::Success, tf!(key, n, file.clusters));
                            let revision = state.doc.revision();
                            let rt = &mut state.runtime.health;
                            rt.set_portals(path, file);
                            rt.portal_doc_revision = Some(revision);
                            rt.show_portal_window = true;
                        }
                        Err(e) => state.notify(Severity::Warning, tf!("app.health.portals_failed", e)),
                    }
                }
                Err(TryRecvError::Empty) => {}
                Err(TryRecvError::Disconnected) => self.prt = None,
            }
        }
        if let Some(rx) = &self.bsp {
            match rx.try_recv() {
                Ok((path, result)) => {
                    self.bsp = None;
                    match result {
                        Ok(load) => {
                            let rt = &mut state.runtime.health;
                            rt.set_exact(path.clone(), load.modified, &load.counts, load.areas, load.clusters);
                            rt.world = load.world.map(std::sync::Arc::new);
                            match load.vis {
                                Some(vis) if vis.has_visibility() => {
                                    rt.set_vis(path, vis);
                                    if let Some(costs) = load.costs {
                                        rt.set_costs(costs);
                                    }
                                }
                                _ => {
                                    if self.vis_wanted {
                                        state.notify(Severity::Info, t!("app.health.no_vis"));
                                        state.runtime.health.pvs = false;
                                    }
                                }
                            }
                        }
                        Err(e) => {
                            if self.vis_wanted {
                                state.runtime.health.pvs = false;
                            }
                            state.log(Severity::Warning, tf!("app.health.bsp_failed", e));
                        }
                    }
                    self.vis_wanted = false;
                }
                Err(TryRecvError::Empty) => {}
                Err(TryRecvError::Disconnected) => self.bsp = None,
            }
        }
        self.poll_build(state);
        self.pick.is_some() || self.prt.is_some() || self.bsp.is_some() || self.build.is_some()
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn detail_sprite_axes_follow_angle_vectors() {
        let close = |a: [f32; 3], b: [f32; 3]| a.iter().zip(&b).all(|(x, y)| (x - y).abs() < 1e-5);
        let (r, u) = right_up([0.0, 0.0, 0.0]);
        assert!(close(r, [0.0, -1.0, 0.0]) && close(u, [0.0, 0.0, 1.0]), "{r:?} {u:?}");
        let (r, u) = right_up([0.0, 90.0, 0.0]);
        assert!(close(r, [1.0, 0.0, 0.0]) && close(u, [0.0, 0.0, 1.0]), "{r:?} {u:?}");
    }

    #[test]
    fn detail_sprites_take_their_baked_lighting() {
        let color = |c: [u8; 4]| detail_color(world_light::rgbe_to_linear(c));
        assert_eq!(color([255, 255, 255, 0]), [255; 4]);
        assert_eq!(color([128, 128, 128, (-1i8) as u8]), [136, 136, 136, 255]);
        assert_eq!(color([200, 0, 10, 2]), [255, 0, 110, 255]);
        assert_eq!(detail_color([f32::NAN, -1.0, 0.0]), [0, 0, 0, 255]);
    }

    #[test]
    fn detail_objects_add_their_light_styles() {
        use hp_bspc::detail_props::DetailObject;
        let records = vec![([64u8, 64, 64, 0], 7u8), ([128, 128, 128, (-1i8) as u8], 32), ([255, 255, 255, 0], 33)];
        let o = DetailObject {
            origin: glam::Vec3::ZERO,
            angles: glam::Vec3::ZERO,
            dict: 0,
            leaf: 0,
            lighting: [128, 128, 128, (-1i8) as u8],
            light_styles: 1,
            light_style_count: 2,
            sway: 0,
            shape_angle: 0,
            shape_size: 0,
            orientation: 0,
            kind: hp_bspc::detail_props::kind::MODEL,
            scale: 1.0,
        };
        let scales = world_light::style_scales([(32, false, None), (33, true, None)]);
        let l = detail_light(&o, &records, &scales);
        let want = 128.0 / 255.0;
        assert!(l.iter().all(|c| (c - want).abs() < 1e-4), "{l:?}");
        let l0 = detail_light(&o, &[], &scales);
        assert!(l0.iter().all(|c| (c - want * 0.5).abs() < 1e-4), "{l0:?}");
        let past = DetailObject { light_styles: 99, ..o };
        assert_eq!(detail_light(&past, &records, &scales), l0);
    }

    #[test]
    fn compiled_files_of_a_map() {
        let mut state = hp_app::testing::test_state();
        assert!(compiled_files(&state, "prt").is_empty(), "an unsaved map has none");
        state.doc.set_path(Some(PathBuf::from("C:/maps/gm_x.vmf")));
        let prt = compiled_files(&state, "prt");
        assert_eq!(prt[0], PathBuf::from("C:/maps/.hammerpro_build/gm_x.prt"));
        assert!(prt.iter().any(|p| p.ends_with("portals/gm_x.prt")));
        assert!(compiled_files(&state, "bsp").iter().all(|p| p.extension().is_some_and(|e| e == "bsp")));
        assert_eq!(newest(vec![PathBuf::from("Z:/nowhere/x.prt")]), None);
    }

    #[test]
    fn bounded_reads_of_hostile_files() {
        let dir = std::env::temp_dir().join(format!("hp_health_prt_{}", std::process::id()));
        std::fs::create_dir_all(&dir).expect("dir");
        let bad = dir.join("bad.prt");
        std::fs::write(&bad, b"PRT1\n2\n1\n3 0 9 (0 0 0) (1 0 0) (0 1 0)\n").expect("write");
        assert!(read_prt(&bad).is_err());
        let good = dir.join("good.prt");
        std::fs::write(&good, b"PRT1\n2\n1\n3 0 1 (0 0 0) (1 0 0) (0 1 0)\n").expect("write");
        assert_eq!(read_prt(&good).map(|f| f.portals.len()), Ok(1));
        let junk = dir.join("junk.bsp");
        std::fs::write(&junk, b"not a bsp at all").expect("write");
        assert!(read_bsp(&junk).is_err());
        let bsp = dir.join("empty.bsp");
        std::fs::write(&bsp, hp_bsp::synthetic_bsp(20, &[(1, vec![0; 40])])).expect("write");
        let load = read_bsp(&bsp).expect("bsp");
        assert!(load.vis.is_none());
        assert!(load.counts.iter().any(|(n, used)| *n == "planes" && *used == 2));
        let _ = std::fs::remove_dir_all(&dir);
    }
}
