use glam::DVec3;

use crate::brush::{BrushSide, MapBrush};
use crate::emit::{Lumps, ModelOut};
use crate::faces::Faces;
use crate::materials::{MaterialSource, contents, surf};
use crate::planes::PlaneSet;
use crate::portals::Portals;
use crate::texinfo::{SideMapping, TexInfos};
use crate::tree::Tree;

#[derive(Clone, Debug)]
pub struct CompileOptions {
    pub leak_test: bool,
    pub vis: VisLevel,
    pub map_name: Option<String>,
    pub sky_vis: bool,
    pub no_detail: bool,
    pub no_water: bool,
    pub full_detail: bool,
    pub micro_volume: f64,
    pub threads: Option<usize>,
    pub radius_override: Option<f64>,
    pub vis_sort: bool,
    pub vis_trace: Option<(u32, u32)>,
    pub no_virtual_mesh: bool,
    pub no_merge: bool,
    pub no_subdiv: bool,
    pub no_tjunc: bool,
    pub no_prune: bool,
    pub default_luxel_size: f64,
    pub luxel_scale: f64,
    pub min_luxel_scale: f64,
    pub max_lightmap_dim: f64,
    pub relaxed_limits: bool,
    pub block_size: u32,
    pub prop_debug: bool,
    pub format: hp_bsp::BspFormat,
}

pub const RELAXED_NOTE: &str = "the game would refuse this map; this compile goes on";

fn relaxable_limit(e: &str) -> bool {
    e.starts_with("Bad surface extents")
        || ["MAX_MAP_TEXDATA", "MAX_MAP_EDGES", "MAX_MAP_SURFEDGES", "MAX_MAP_BRUSHSIDES"].iter().any(|l| e.contains(l))
}

impl Default for CompileOptions {
    fn default() -> Self {
        CompileOptions {
            leak_test: false,
            vis: VisLevel::Off,
            map_name: None,
            sky_vis: false,
            no_detail: false,
            no_water: false,
            full_detail: false,
            micro_volume: 1.0,
            threads: None,
            radius_override: None,
            vis_sort: true,
            vis_trace: None,
            no_virtual_mesh: false,
            no_merge: false,
            no_subdiv: false,
            no_tjunc: false,
            no_prune: false,
            default_luxel_size: 16.0,
            luxel_scale: 1.0,
            min_luxel_scale: 1.0,
            max_lightmap_dim: crate::faces::MAX_LIGHTMAP_DIM,
            relaxed_limits: false,
            block_size: crate::tree::BLOCK_SIZE as u32,
            prop_debug: false,
            format: hp_bsp::BspFormat::default(),
        }
    }
}

pub const MAX_LIGHTMAP_DIM_LIMIT: f64 = 125.0;

pub const MIN_BLOCK_SIZE: u32 = 128;
pub const MAX_BLOCK_SIZE: u32 = 32768;

pub fn block_size_of(v: f64) -> Option<u32> {
    if !v.is_finite() || v < 0.0 {
        return None;
    }
    let n = v.floor();
    if n == 0.0 {
        return Some(0);
    }
    (f64::from(MIN_BLOCK_SIZE)..=f64::from(MAX_BLOCK_SIZE)).contains(&n).then_some(n as u32)
}

pub const MAX_SWITCHED_LIGHTS: usize = hp_bsp::limits::SWITCHED_LIGHTS.max_usize();

pub const MAX_MAP_OVERLAYS: usize = hp_bsp::limits::OVERLAYS.max_usize();

pub const OVERLAY_BSP_FACE_COUNT: usize = hp_bsp::limits::OVERLAY_FACES.max_usize();

pub const MAX_MAP_MODELS: usize = hp_bsp::limits::MODELS.max_usize();

#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub enum VisLevel {
    #[default]
    Off,
    Fast,
    Full,
}

#[derive(Clone, Debug, Default)]
pub struct CompileOutput {
    pub bsp: Vec<u8>,
    pub leak: Option<Vec<DVec3>>,
    pub clusters: i32,
    pub faces: usize,
    pub brushes: usize,
    pub vis_portals: Vec<ClusterPortal>,
    pub prt: String,
    pub trace: Option<String>,
}

#[derive(Clone, Debug, Default)]
pub struct ClusterPortal {
    pub points: Vec<DVec3>,
    pub clusters: [u32; 2],
}

#[derive(Debug, thiserror::Error)]
pub enum CompileError {
    #[error("the map leaked")]
    Leaked { path: Vec<DVec3> },
    #[error("{0}")]
    Failed(String),
}

struct ModelSrc {
    entity: usize,
    brushes: Vec<usize>,
    origin: DVec3,
}

fn key<'a>(e: &'a hp_vmf::Entity, k: &str) -> Option<&'a str> {
    e.get(k).map(str::trim)
}

fn parse_vec(s: &str) -> Option<DVec3> {
    let v: Vec<f64> = s.split_whitespace().filter_map(|x| x.parse().ok()).collect();
    (v.len() == 3 && v.iter().all(|x| x.is_finite())).then(|| DVec3::new(v[0], v[1], v[2]))
}

const PHYS_MASK: u32 = contents::SOLID | contents::WINDOW | contents::GRATE | contents::MOVEABLE | 0x0200_0000;
const MODEL_PHYS_MASK: u32 = PHYS_MASK | contents::PLAYERCLIP | contents::MONSTERCLIP | contents::MASK_WATER;

fn merges_into_world(class: &str) -> bool {
    class.eq_ignore_ascii_case("func_detail") || class.eq_ignore_ascii_case("func_ladder")
}

fn removed_class(class: &str) -> bool {
    let c = class.to_ascii_lowercase();
    matches!(
        c.as_str(),
        "func_detail"
            | "func_viscluster"
            | "info_overlay"
            | "info_overlay_transition"
            | "env_cubemap"
            | "prop_static"
            | "static_prop"
            | "info_lighting"
            | "func_instance"
            | "func_instance_parms"
            | "info_no_dynamic_shadow"
            | "detail_prop"
            | "prop_detail"
            | "prop_detail_sprite"
            | crate::cubemap::PARALLAX_OBB_CLASS
    )
}

fn parallax_obbs(entities: &[&hp_vmf::Entity], materials: &dyn MaterialSource) -> Vec<(String, String)> {
    parallax_obb_windings(entities, materials)
        .into_iter()
        .map(|(name, w)| (name, crate::cubemap::parallax_rows(&crate::cubemap::parallax_obb_matrix(&w))))
        .collect()
}

pub(crate) fn parallax_obb_windings(
    entities: &[&hp_vmf::Entity],
    materials: &dyn MaterialSource,
) -> Vec<(String, Vec<Option<Vec<glam::Vec3>>>)> {
    let mut out = Vec::new();
    for (ei, e) in entities
        .iter()
        .enumerate()
        .filter(|(_, e)| e.classname.eq_ignore_ascii_case(crate::cubemap::PARALLAX_OBB_CLASS))
    {
        let Some(name) = key(e, "targetname").map(str::trim).filter(|n| !n.is_empty()) else { continue };
        let mut planes = PlaneSet::new();
        let mut tex = TexInfos::new();
        let mut facts = |n: &str| materials.facts(n);
        let windings: Vec<Option<Vec<glam::Vec3>>> = e
            .solids
            .first()
            .and_then(|s| {
                build_brush(
                    s,
                    ei,
                    DVec3::ZERO,
                    &BrushLoad::plain(0, false),
                    &mut planes,
                    &mut tex,
                    &mut facts,
                    &mut Vec::new(),
                    &ahash::AHashMap::new(),
                    &mut |_| {},
                )
                .ok()
                .flatten()
            })
            .map(|mut b| {
                b.add_bevels(&mut planes);
                b.sides.iter().map(|s| s.fwinding.clone()).collect()
            })
            .unwrap_or_default();
        out.push((name.to_owned(), windings));
    }
    out
}

fn origin_brush_center(lo: DVec3, hi: DVec3) -> DVec3 {
    let c = (lo.as_vec3() + hi.as_vec3()) * 0.5;
    DVec3::new(f64::from(c.x.trunc()), f64::from(c.y.trunc()), f64::from(c.z.trunc()))
}

const PATH_TRAINS: [&str; 3] = ["func_tracktrain", "func_tanktrain", "func_train"];

fn train_origin<'a>(ent: &hp_vmf::Entity, solids: impl Iterator<Item = &'a hp_vmf::Solid>) -> Option<DVec3> {
    let class = ent.classname.trim();
    if !PATH_TRAINS.iter().any(|c| c.eq_ignore_ascii_case(class)) || key(ent, "origin").and_then(parse_vec).is_some() {
        return None;
    }
    let (mut lo, mut hi) = (DVec3::splat(f64::MAX), DVec3::splat(f64::MIN));
    for solid in solids {
        for s in &solid.sides {
            for p in s.plane {
                lo = lo.min(p);
                hi = hi.max(p);
            }
        }
    }
    (lo.cmple(hi).all() && lo.is_finite() && hi.is_finite()).then(|| origin_brush_center(lo, hi))
}

fn detail_blockers(
    planes: &mut PlaneSet,
    brushes: &[MapBrush],
    ids: &[usize],
) -> Vec<crate::detail_props::DetailBlocker> {
    use crate::brushbsp::{DetailScreen, MAX_COORD, Visibility, make_brush_list};
    let mut out = Vec::new();
    for &i in ids {
        let lo = glam::Vec3::splat(-MAX_COORD);
        let hi = glam::Vec3::splat(MAX_COORD);
        let list = make_brush_list(planes, brushes, &[i], lo, hi, DetailScreen::NoDetail, Visibility::Windings);
        if let Some(b) = list.first() {
            out.push(crate::detail_props::DetailBlocker {
                mins: b.mins,
                maxs: b.maxs,
                planes: b.sides.iter().map(|s| planes.getf(s.plane)).collect(),
            });
        }
    }
    out
}

fn ladder_bounds<'a>(brushes: impl Iterator<Item = &'a MapBrush>) -> (DVec3, DVec3) {
    let mut lo = glam::Vec3::splat(99999.0);
    let mut hi = glam::Vec3::splat(-99999.0);
    for b in brushes {
        let (bl, bh) = crate::brushbsp::map_brush_bounds(b);
        crate::brush::add_point_to_bounds(bl, &mut lo, &mut hi);
        crate::brush::add_point_to_bounds(bh, &mut lo, &mut hi);
    }
    (lo.as_dvec3(), hi.as_dvec3())
}

fn force_areaportal_window_contents(entities: &[&hp_vmf::Entity], brushes: &mut [MapBrush]) {
    for e in entities {
        if !crate::areas::is_areaportal_class(&e.classname) || e.classname.eq_ignore_ascii_case("func_areaportal") {
            continue;
        }
        for k in ["target", "BackgroundBModel"] {
            let Some(name) = key(e, k).filter(|n| !n.is_empty()) else { continue };
            let Some(target) =
                entities.iter().position(|t| key(t, "targetname").is_some_and(|t| t.eq_ignore_ascii_case(name)))
            else {
                continue;
            };
            for b in brushes.iter_mut().filter(|b| b.entity == target) {
                b.contents = (b.contents & !contents::SOLID) | contents::TRANSLUCENT | contents::WINDOW;
            }
        }
    }
}

fn without_hidden(map: &hp_vmf::Map) -> Option<hp_vmf::Map> {
    let any = map.entities.iter().any(|e| e.hidden)
        || std::iter::once(&map.world).chain(map.entities.iter()).any(|e| e.solids.iter().any(|s| s.hidden));
    if !any {
        return None;
    }
    let mut m = map.clone();
    m.entities.retain(|e| !e.hidden);
    for e in std::iter::once(&mut m.world).chain(m.entities.iter_mut()) {
        e.solids.retain(|s| !s.hidden);
    }
    Some(m)
}

pub fn compile(
    map: &hp_vmf::Map,
    materials: &dyn MaterialSource,
    model_files: &dyn crate::props::ModelSource,
    opts: &CompileOptions,
    log: &mut dyn FnMut(&str),
) -> Result<CompileOutput, CompileError> {
    let visible = without_hidden(map);
    let map = visible.as_ref().unwrap_or(map);
    let start = std::time::Instant::now();
    let timing = std::env::var_os("HP_BSPC_TIMES").is_some();
    let mut last_stage = start;
    let mut stages: Vec<(String, f64)> = Vec::new();
    let mut stage = |name: &str| {
        let now = std::time::Instant::now();
        let secs = (now - last_stage).as_secs_f64();
        if timing {
            let _ = std::io::Write::write_fmt(&mut std::io::stderr(), format_args!("[time] {name}: {secs:.3} s\n"));
        }
        stages.push((name.to_owned(), secs));
        last_stage = now;
    };
    let mut planes = PlaneSet::new();
    let mut tex = TexInfos::new();
    let mut brushes: Vec<MapBrush> = Vec::new();
    let mut entities: Vec<&hp_vmf::Entity> = vec![&map.world];
    entities.extend(map.entities.iter());

    let mut facts_cache: ahash::AHashMap<String, crate::materials::MaterialFacts> = ahash::AHashMap::new();
    let mut prefetched: ahash::AHashMap<String, crate::materials::MaterialFacts> = {
        use rayon::prelude::*;
        let mut seen = ahash::AHashSet::new();
        let names: Vec<&str> = entities
            .iter()
            .flat_map(|e| e.solids.iter())
            .flat_map(|s| s.sides.iter())
            .map(|s| s.material.as_str())
            .filter(|m| seen.insert(m.to_ascii_lowercase()))
            .collect();
        names.par_iter().map(|m| (m.to_ascii_lowercase(), materials.facts(m))).collect::<Vec<_>>().into_iter().collect()
    };
    let mut missing: Vec<String> = Vec::new();
    let mut facts_of = |name: &str| -> crate::materials::MaterialFacts {
        let k = name.to_ascii_lowercase();
        if let Some(f) = facts_cache.get(&k) {
            return f.clone();
        }
        let f = prefetched.remove(&k).unwrap_or_else(|| materials.facts(name));
        if !f.found {
            missing.push(k.clone());
        }
        facts_cache.insert(k, f.clone());
        f
    };

    let mut cubemaps = crate::cubemap::Cubemaps::new(&entities, opts.map_name.as_deref(), materials);
    cubemaps.resolve_parallax(&parallax_obbs(&entities, materials));
    let mut world_brushes: Vec<usize> = Vec::new();
    let mut disps: Vec<crate::disp::DispFace> = Vec::new();
    let mut models: Vec<ModelSrc> = Vec::new();
    let mut origins_of: Vec<Option<DVec3>> = vec![None; entities.len()];
    let mut origin_brush_of: Vec<Option<DVec3>> = vec![None; entities.len()];
    let mut removed: Vec<bool> = vec![false; entities.len()];
    let mut ladders: Vec<Option<(DVec3, DVec3)>> = vec![None; entities.len()];
    let mut blockers: Vec<crate::detail_props::DetailBlocker> = Vec::new();
    let mut occluder_srcs: Vec<(usize, Vec<usize>)> = Vec::new();
    let mut viscluster_srcs: Vec<Vec<usize>> = Vec::new();
    let mut side_texes: Vec<SideTex> = Vec::new();
    let mut overlay_ids: ahash::AHashMap<u32, Vec<u32>> = ahash::AHashMap::new();
    for (k, e) in entities.iter().filter(|e| e.classname.eq_ignore_ascii_case("info_overlay")).enumerate() {
        for id in key(e, "sides").unwrap_or("").split_whitespace().filter_map(|x| x.parse::<u32>().ok()) {
            overlay_ids.entry(id).or_default().push(k as u32);
        }
    }

    let load = BrushLoad::of(opts);
    for (ei, ent) in entities.iter().enumerate() {
        let class = ent.classname.clone();
        let into_world = ei == 0 || merges_into_world(&class);
        let areaportal = ei != 0 && crate::areas::is_areaportal_class(&class);
        let occluder = ei != 0 && crate::occluder::is_occluder_class(&class);
        let viscluster = class.eq_ignore_ascii_case("func_viscluster");
        if removed_class(&class) {
            removed[ei] = true;
        }
        if class.eq_ignore_ascii_case(crate::cubemap::PARALLAX_OBB_CLASS) {
            continue;
        }
        let key_origin = key(ent, "origin").and_then(parse_vec);
        origins_of[ei] = key_origin;
        if areaportal && ent.solids.len() != 1 {
            return Err(CompileError::Failed(format!("Entity {ei}: func_areaportal can only be a single brush")));
        }
        if ent.solids.is_empty() {
            continue;
        }
        let world_coords = into_world || areaportal || occluder || viscluster;
        let mut origin = if world_coords { DVec3::ZERO } else { key_origin.unwrap_or(DVec3::ZERO) };
        let mut list = Vec::new();
        let mut origin_brush = None;
        for solid in &ent.solids {
            let first_material = solid.sides.first().map(|s| s.material.as_str()).unwrap_or("");
            if !into_world && facts_of(first_material).contents & contents::ORIGIN != 0 {
                let (mut lo, mut hi) = (DVec3::splat(f64::MAX), DVec3::splat(f64::MIN));
                for s in &solid.sides {
                    for p in s.plane {
                        lo = lo.min(p);
                        hi = hi.max(p);
                    }
                }
                origin_brush = Some(origin_brush_center(lo, hi));
                origin_brush_of[ei] = origin_brush;
                list.push((solid, true));
                continue;
            }
            list.push((solid, false));
        }
        if let Some(o) = origin_brush.filter(|_| !world_coords) {
            origin = o;
        } else if !world_coords && let Some(o) = train_origin(ent, list.iter().filter(|(_, o)| !o).map(|(s, _)| *s)) {
            log(&format!(
                "Warning: entity {ei} ({class}) has no origin key nor origin brush: its origin is the center of its \
                 brushes ({} {} {}), where the train meets its path",
                o.x, o.y, o.z
            ));
            origin = o;
            origin_brush_of[ei] = Some(o);
        }
        if !world_coords {
            origins_of[ei] = Some(origin);
        }
        let mut ids = Vec::new();
        let mut brush_facts = |name: &str| {
            let mut f = facts_of(name);
            if areaportal {
                f.contents = contents::AREAPORTAL;
                f.raw_contents = contents::AREAPORTAL;
            } else if (occluder || viscluster) && f.contents == contents::EMPTY {
                f.contents = contents::SOLID;
                f.raw_contents = contents::SOLID;
            }
            f
        };
        let base = if class.eq_ignore_ascii_case("func_detail") {
            contents::DETAIL
        } else if class.eq_ignore_ascii_case("func_ladder") {
            contents::LADDER
        } else if class.eq_ignore_ascii_case("func_water") {
            contents::WATER
        } else {
            0
        };
        let strip_detail = ei != 0 && !into_world && !areaportal && !viscluster;
        let entity_sides = side_texes.len();
        for (brush_num, (solid, is_origin)) in list.into_iter().enumerate() {
            if ei != 0 && solid.sides.iter().any(|s| s.disp.is_some()) {
                return Err(CompileError::Failed(format!(
                    "Error: displacement found on a(n) {class} entity - not supported (entity {ei}, brush {brush_num})"
                )));
            }
            if is_origin {
                for s in &solid.sides {
                    let [a, b, c] = s.plane;
                    planes.from_points(a, b, c);
                    let facts = brush_facts(&s.material);
                    let mapping = SideMapping {
                        u_axis: s.uaxis.axis,
                        u_shift: s.uaxis.shift,
                        u_scale: s.uaxis.scale,
                        v_axis: s.vaxis.axis,
                        v_shift: s.vaxis.shift,
                        v_scale: s.vaxis.scale,
                        lightmap_scale: load.luxel_size(s.lightmap_scale),
                    };
                    let texdata = tex.texdata(&s.material, &facts);
                    tex.for_side(&mapping, DVec3::ZERO, facts.surf, texdata);
                }
                continue;
            }
            let first_side = side_texes.len();
            match build_brush(
                solid,
                ei,
                DVec3::ZERO,
                &BrushLoad { base, strip_detail, ..load },
                &mut planes,
                &mut tex,
                &mut brush_facts,
                &mut side_texes,
                &overlay_ids,
                log,
            ) {
                Ok(Some(b)) if into_world && solid.sides.iter().any(|s| s.disp.is_some()) => {
                    for s in &solid.sides {
                        let Some(info) = &s.disp else { continue };
                        let Some(bs) = b.sides.iter().find(|x| x.side_id == s.id) else { continue };
                        let order: Vec<([f32; 3], f32)> = solid
                            .sides
                            .iter()
                            .filter_map(|x| b.sides.iter().find(|y| y.side_id == x.id))
                            .map(|y| {
                                let (n, d) = planes.getf(y.plane);
                                ([n.x, n.y, n.z], d)
                            })
                            .collect();
                        let at = solid
                            .sides
                            .iter()
                            .filter(|x| b.sides.iter().any(|y| y.side_id == x.id))
                            .position(|x| x.id == s.id);
                        let vbsp_winding = at.and_then(|k| crate::disp::vbsp::side_winding(&order, k));
                        let Some(mut w) = vbsp_winding
                            .map(|p| {
                                crate::winding::Winding::new(
                                    p.iter()
                                        .map(|q| DVec3::new(f64::from(q[0]), f64::from(q[1]), f64::from(q[2])))
                                        .collect(),
                                )
                            })
                            .or_else(|| side_float_winding(bs))
                        else {
                            continue;
                        };
                        w.remove_colinear();
                        if w.points.len() != 4 {
                            log(&format!("Brush {}: non-quad displacement face ({} sides)", b.id, w.points.len()));
                            continue;
                        }
                        let facts = brush_facts(&s.material);
                        if let Some(t) = side_texes[first_side..].iter_mut().find(|t| t.side_id == s.id) {
                            t.slot = SideSlot::Disp(disps.len());
                        }
                        disps.push(crate::disp::DispFace {
                            winding: w,
                            plane: bs.plane,
                            texinfo: bs.texinfo,
                            info: (**info).clone(),
                            contents: b.contents & !contents::DETAIL,
                            ids: (b.id, s.id),
                            surfaceprop: if facts.has_surfaceprop && facts.surfaceprop.is_empty() {
                                "default".to_owned()
                            } else {
                                facts.surfaceprop.to_ascii_lowercase()
                            },
                            four_way: facts.shader.contains("lightmapped_4wayblend"),
                            surfaceprop2: facts.surfaceprop2.map(|s| s.to_ascii_lowercase()),
                        });
                    }
                }
                Ok(Some(mut b)) => {
                    if areaportal {
                        b.contents = contents::AREAPORTAL;
                    }
                    if ei == 0 && b.contents & (contents::PLAYERCLIP | contents::MONSTERCLIP) != 0 {
                        for s in &mut b.sides {
                            s.no_split = true;
                        }
                    }
                    if into_world && !areaportal && b.contents & contents::AREAPORTAL != 0 {
                        log(&format!("Brush {}: areaportal brushes must be in a func_areaportal, removed", b.id));
                        continue;
                    }
                    for t in side_texes[first_side..].iter_mut() {
                        t.slot = SideSlot::Brush(brushes.len());
                    }
                    b.add_bevels(&mut planes);
                    for t in side_texes[first_side..].iter_mut() {
                        if let Some(side) = b.sides.iter().find(|x| !x.bevel && x.side_id == t.side_id) {
                            t.winding = side_float_winding(side);
                        }
                    }
                    if b.contents != 0 && brush_volume(&b) < opts.micro_volume {
                        log(&format!("Brush {}: WARNING, microbrush", b.id));
                    }
                    ids.push(brushes.len());
                    brushes.push(b);
                }
                Ok(None) => {}
                Err(msg) => log(&msg),
            }
        }
        if origin != DVec3::ZERO {
            for &i in &ids {
                brushes[i].apply_origin(&mut planes, origin);
            }
            for t in &mut side_texes[entity_sides..] {
                t.offset = origin;
                if let SideSlot::Brush(i) = t.slot
                    && let Some(side) = brushes[i].sides.iter().find(|x| x.side_id == t.side_id)
                {
                    t.winding = side_float_winding(side);
                }
            }
            let mut order: Vec<usize> = Vec::new();
            let mut taken = vec![false; side_texes.len() - entity_sides];
            for &i in &ids {
                for side in brushes[i].sides.iter().filter(|x| !x.bevel) {
                    let found = side_texes[entity_sides..]
                        .iter()
                        .enumerate()
                        .position(|(k, t)| !taken[k] && t.slot == SideSlot::Brush(i) && t.side_id == side.side_id);
                    if let Some(k) = found {
                        taken[k] = true;
                        order.push(k);
                    }
                }
            }
            order.extend((0..taken.len()).filter(|&k| !taken[k]));
            for &i in &ids {
                for side in brushes[i].sides.iter_mut().filter(|x| x.bevel) {
                    side.texinfo = 0;
                }
            }
            for k in order {
                let t = &mut side_texes[entity_sides + k];
                let texdata = tex.infos.get(t.texinfo as usize).map_or(-1, |i| i.texdata);
                t.texinfo = tex.for_side(&t.mapping, origin, t.facts.surf, texdata);
                set_side_texinfo(t, &mut brushes, &mut disps);
            }
        }
        if class.eq_ignore_ascii_case("func_ladder") {
            ladders[ei] = Some(ladder_bounds(ids.iter().map(|&i| &brushes[i])));
        }
        if class.eq_ignore_ascii_case("func_detail_blocker") {
            blockers.extend(detail_blockers(&mut planes, &brushes, &ids));
            removed[ei] = true;
        }
        if into_world || areaportal {
            world_brushes.extend(ids);
        } else if occluder {
            occluder_srcs.push((ei, ids));
        } else if viscluster {
            viscluster_srcs.push(ids);
        } else if !ids.is_empty() && !removed[ei] {
            models.push(ModelSrc { entity: ei, brushes: ids, origin });
        }
    }
    {
        let mut order: Vec<usize> = world_brushes.clone();
        let mut in_world = vec![false; brushes.len()];
        for &i in &world_brushes {
            in_world[i] = true;
        }
        order.extend((0..brushes.len()).filter(|&i| !in_world[i]));
        let mut new_of = vec![0usize; brushes.len()];
        for (n, &o) in order.iter().enumerate() {
            new_of[o] = n;
        }
        let mut old: Vec<Option<MapBrush>> = std::mem::take(&mut brushes).into_iter().map(Some).collect();
        brushes = order.iter().map(|&o| old[o].take().expect("each brush once")).collect();
        let remap = |v: &mut Vec<usize>| v.iter_mut().for_each(|i| *i = new_of[*i]);
        remap(&mut world_brushes);
        for m in &mut models {
            remap(&mut m.brushes);
        }
        for (_, ids) in &mut occluder_srcs {
            remap(ids);
        }
        for ids in &mut viscluster_srcs {
            remap(ids);
        }
        for t in &mut side_texes {
            if let SideSlot::Brush(i) = t.slot {
                t.slot = SideSlot::Brush(new_of[i]);
            }
        }
    }
    log(&format!(
        "{} brushes, {} brush sides, {} planes",
        brushes.len(),
        brushes.iter().map(|b| b.sides.len()).sum::<usize>(),
        planes.len()
    ));
    stage("read brushes");
    texture_passes(&mut side_texes, &mut tex, &mut cubemaps, &mut brushes, &mut disps, &mut facts_of);
    let patches = format!("maps/{}/", cubemaps.map_name().to_ascii_lowercase());
    for m in missing.iter().filter(|m| !m.starts_with(&patches)) {
        log(&format!("material \"{m}\" not found."));
    }
    stage("texture passes");
    crate::disp::setup_allowed_verts(&mut disps);
    stage("displacement allowed vertices");

    force_areaportal_window_contents(&entities, &mut brushes);
    let contents_list: Vec<u32> = brushes.iter().map(|b| b.contents).collect();
    let bounds = |ids: &[usize], brushes: &[MapBrush]| {
        let mut lo = DVec3::splat(f64::MAX);
        let mut hi = DVec3::splat(f64::MIN);
        for &i in ids {
            lo = lo.min(brushes[i].mins);
            hi = hi.max(brushes[i].maxs);
        }
        (lo, hi)
    };
    let (wmins, wmaxs) = bounds(&world_brushes, &brushes);

    let flood_origins: Vec<(usize, DVec3)> = (1..entities.len())
        .filter_map(|i| {
            let o = origins_of[i]?;
            (o != DVec3::ZERO).then_some((i, o))
        })
        .collect();

    log("Processing world model...");
    if opts.block_size == 0 {
        log("The world is built in one block (-blocksize 0)");
    } else if opts.block_size != crate::tree::BLOCK_SIZE as u32 {
        log(&format!("The world is built in blocks of {} units (-blocksize)", opts.block_size));
    }
    let mut world_contents = contents_list.clone();
    let mut tree1 = crate::brushbsp::world_tree(
        &mut planes,
        &brushes,
        &mut world_contents,
        &world_brushes,
        crate::brushbsp::Visibility::Windings,
        opts.block_size,
    );
    for (b, c) in brushes.iter_mut().zip(&world_contents) {
        b.contents = *c;
    }
    let contents_list = world_contents;
    stage("first tree");
    let mut portals1 = Portals::make(&mut tree1, &planes);
    stage("first portals");
    if let Ok(v) = std::env::var("HP_BSPC_DEBUG_POINT") {
        let n: Vec<f64> = v.split(',').filter_map(|x| x.trim().parse().ok()).collect();
        if n.len() == 3 {
            let leaf = tree1.leaf_at(&planes, DVec3::new(n[0], n[1], n[2]));
            let node = &tree1.nodes[leaf];
            log(&format!(
                "debug: leaf {leaf} contents {:#x} brushes {:?} bounds {:?} {:?} portals {}",
                node.contents,
                node.brushes.iter().map(|b| brushes[b.original].id).collect::<Vec<_>>(),
                node.volume.as_ref().map(|v| v.mins),
                node.volume.as_ref().map(|v| v.maxs),
                node.portals.len()
            ));
        }
    }
    let (placed, leaked) = portals1.flood(&mut tree1, &planes, &flood_origins);
    let mut leak_path = None;
    if placed.is_empty() {
        log("**** No entity in the open: the map has no inside (place an info_player_start) ****");
    }
    if leaked {
        let (mut path, end) = portals1.leak_path(&tree1);
        let occupant = tree1.nodes[end].occupant;
        if let Some(e) = occupant.and_then(|i| entities.get(i)) {
            let o = key(e, "origin").and_then(parse_vec).unwrap_or(DVec3::ZERO);
            path.push(o);
            log("**** leaked ****");
            log(&format!("Entity {} ({:.2} {:.2} {:.2}) leaked!", e.classname, o.x, o.y, o.z));
        }
        if std::env::var("HP_BSPC_DEBUG_POINT").is_ok() {
            for q in &path {
                log(&format!("debug leak point {:.2} {:.2} {:.2} leaf {}", q.x, q.y, q.z, tree1.leaf_at(&planes, *q)));
            }
        }
        if let Some(first) = path.first() {
            log(&format!(
                "Leak path: {} points, it leaves the map near ({:.0} {:.0} {:.0})",
                path.len(),
                first.x,
                first.y,
                first.z
            ));
        }
        if opts.leak_test {
            return Err(CompileError::Leaked { path });
        }
        leak_path = Some(path);
    }
    portals1.mark_visible_sides(&tree1, &planes, &mut brushes);
    stage("flood, visible sides");
    let (mut tree, portals) = if leaked {
        (tree1, portals1)
    } else {
        drop(tree1);
        drop(portals1);
        let mut tree = crate::brushbsp::world_tree(
            &mut planes,
            &brushes,
            &mut contents_list.clone(),
            &world_brushes,
            crate::brushbsp::Visibility::Marked,
            opts.block_size,
        );
        stage("final tree");
        let portals = Portals::make(&mut tree, &planes);
        stage("final portals");
        if !placed.is_empty() {
            portals.flood(&mut tree, &planes, &flood_origins);
            Portals::fill_outside(&mut tree);
        }
        (tree, portals)
    };
    let mut portals = portals;
    log("Processing areas...");
    let areas = crate::areas::flood(&mut tree, &portals, &brushes, crate::areas::areaportal_entities(&entities), log);
    let area_lumps = crate::areas::lumps(&tree, &portals, &planes, &areas, log).map_err(CompileError::Failed)?;
    let max_areas = hp_bsp::limits::AREAS.max_in(opts.format.limits);
    if i64::from(areas.count) > max_areas as i64 {
        return Err(CompileError::Failed(format!(
            "Map is split into too many unique areas (max = {max_areas})
Probably too many areaportals"
        )));
    }
    if leak_path.is_none() {
        leak_path = areas.leak.clone();
    }
    crate::areas::remove_areaportal_brushes(&mut tree, &brushes);
    stage("areas");
    let max_lightmap_dim = opts.max_lightmap_dim.min(f64::from(opts.format.max_lightmap_dim));
    if max_lightmap_dim < opts.max_lightmap_dim {
        log(&format!(
            "Warning: -maxlightmapdim {} is more than {} takes: {max_lightmap_dim} used",
            opts.max_lightmap_dim,
            opts.format.branch.name()
        ));
    }
    let max_extent = hp_bsp::limits::LIGHTMAP_EXTENT.max_in(opts.format.limits) as i32;
    let face_options = crate::faces::FaceOptions {
        no_merge: opts.no_merge,
        no_subdiv: opts.no_subdiv,
        no_tjunc: opts.no_tjunc,
        max_lightmap_dim: (max_lightmap_dim != crate::faces::MAX_LIGHTMAP_DIM).then_some(max_lightmap_dim as f32),
        max_extent: (opts.format.limits != hp_bsp::limits::LimitSet::Gmod).then_some(max_extent),
    };
    let could_overflow = !opts.no_subdiv && max_lightmap_dim + 3.0 > f64::from(max_extent);
    let pristine = could_overflow.then(|| (tree.clone(), portals.clone()));
    let mut tight = ahash::AHashMap::new();
    let map_bounds = crate::brushbsp::map_bounds(&brushes, &world_brushes);
    let (mut faces, live, holder) = loop {
        let mut faces = Faces::make_with(&mut tree, &mut portals, &planes, &brushes, &tex, true, true, face_options);
        faces.tight = tight.clone();
        faces.merge(&mut tree, &planes, &brushes);
        faces.subdivide(&mut tree, &planes, &tex);
        stage("faces");
        let holder = tree.nodes.len();
        tree.nodes.push(crate::tree::Node { cluster: -1, ..crate::tree::Node::default() });
        crate::detail::merge_detail_tree(
            &mut tree,
            holder,
            &mut planes,
            &brushes,
            &world_brushes,
            map_bounds,
            &tex,
            &mut faces,
        );
        let live: Vec<usize> =
            tree.nodes.iter().flat_map(|n| n.faces.iter().copied()).filter(|f| faces.list[*f].live()).collect();
        stage("detail");
        faces.fix_tjuncs(&mut tree, true);
        stage("tjuncs");
        let bad = if could_overflow { faces.oversized(&tex) } else { Vec::new() };
        match &pristine {
            Some((t0, p0)) if !bad.is_empty() && faces.tighten(&bad, &mut tight) => {
                log(&format!("{} faces too big for their lightmap once welded: their sides are cut again", bad.len()));
                tree = t0.clone();
                portals = p0.clone();
            }
            _ => break (faces, live, holder),
        }
    };
    drop(pristine);
    let occluders = crate::occluder::build(
        &occluder_srcs,
        &entities,
        &mut planes,
        &contents_list,
        &brushes,
        &tex,
        &tree,
        &portals,
        log,
    );
    let sky_areas = crate::areas::sky_areas(&tree, &planes, &entities).map_err(CompileError::Failed)?;
    if !opts.no_prune {
        tree.prune();
    }
    let forced = crate::areas::forced_clusters(&tree, &planes, &brushes, &viscluster_srcs, &sky_areas, opts.sky_vis);
    let mut clusters = tree.number_clusters_with(&forced);
    if leaked {
        for n in &mut tree.nodes {
            n.cluster = 0;
        }
        clusters = 0;
    }
    let mut water = crate::water::WaterData::default();
    water.add_model(0, &mut tree, &portals, &planes, &brushes);
    let mut prt_tree = Tree {
        nodes: tree
            .nodes
            .iter()
            .map(|n| crate::tree::Node {
                plane: n.plane,
                children: n.children,
                parent: n.parent,
                contents: n.contents,
                cluster: n.cluster,
                ..Default::default()
            })
            .collect(),
        head: tree.head,
        outside: tree.outside,
        mins: tree.mins,
        maxs: tree.maxs,
    };
    let prt_portals = Portals::make(&mut prt_tree, &planes);
    let mut vis_portals = Vec::new();
    let mut stack = vec![prt_tree.head];
    while let Some(n) = stack.pop() {
        if !prt_tree.nodes[n].is_leaf() {
            let [f, b] = prt_tree.nodes[n].children;
            stack.push(b);
            stack.push(f);
            continue;
        }
        for &pid in &prt_tree.nodes[n].portals {
            let p = &prt_portals.list[pid];
            if p.nodes[0] != n || p.on_node.is_none() || p.winding.points.len() < 3 {
                continue;
            }
            let [a, b] = p.nodes;
            let (ca, cb) = (prt_tree.nodes[a].cluster, prt_tree.nodes[b].cluster);
            if ca < 0 || cb < 0 || ca == cb || !prt_tree.nodes[a].is_leaf() || !prt_tree.nodes[b].is_leaf() {
                continue;
            }
            let clusters = if winding_faces_away(&p.winding.points, p.plane.normal) {
                [cb as u32, ca as u32]
            } else {
                [ca as u32, cb as u32]
            };
            vis_portals.push(ClusterPortal { points: p.winding.points.clone(), clusters });
        }
    }
    drop((prt_tree, prt_portals));
    let prt = prt_text(clusters, &vis_portals);
    stage("occluders, clusters, water");
    if leaked {
        log(&format!(
            "world: {} nodes, {} faces; leaked: every leaf in cluster 0, no portal file, no visibility (like VBSP)",
            tree.nodes.len(),
            live.len()
        ));
    } else {
        log(&format!("world: {} nodes, {} faces, {} clusters", tree.nodes.len(), live.len(), clusters));
    }

    let model_planes = (!models.is_empty()).then(|| crate::brushbsp::model_planes(&mut planes));
    let built: Vec<(Tree, Portals, Faces)> = {
        use rayon::prelude::*;
        let planes = &planes;
        models
            .par_iter()
            .map(|m| {
                let mp = model_planes.expect("planes found when there are brush entities");
                let mut t0 = crate::brushbsp::model_tree_with(planes, mp, &brushes, &contents_list, &m.brushes);
                let mut p0 = Portals::make(&mut t0, planes);
                let mut tight = ahash::AHashMap::new();
                loop {
                    let (mut t, mut p) = if could_overflow {
                        (t0.clone(), p0.clone())
                    } else {
                        (std::mem::take(&mut t0), std::mem::take(&mut p0))
                    };
                    let mut f = Faces::make_with(&mut t, &mut p, planes, &brushes, &tex, false, true, face_options);
                    f.tight = tight.clone();
                    f.merge(&mut t, planes, &brushes);
                    f.subdivide(&mut t, planes, &tex);
                    f.fix_tjuncs(&mut t, false);
                    let bad = if could_overflow { f.oversized(&tex) } else { Vec::new() };
                    if bad.is_empty() || !f.tighten(&bad, &mut tight) {
                        break (t, p, f);
                    }
                }
            })
            .collect()
    };
    let mut model_trees: Vec<(Tree, Portals, Faces, DVec3, usize)> = Vec::with_capacity(models.len());
    for (m, (mut t, p, f)) in models.iter().zip(built) {
        water.add_model(model_trees.len() + 1, &mut t, &p, &planes, &brushes);
        model_trees.push((t, p, f, m.origin, m.entity));
    }
    stage("brush entities");
    let map_name = cubemaps.map_name().to_owned();
    if !map_name.is_empty() {
        let mut made: Vec<String> = Vec::new();
        for wm in &water.models {
            let Some(name) = usize::try_from(wm.texinfo)
                .ok()
                .and_then(|t| tex.infos.get(t))
                .and_then(|i| usize::try_from(i.texdata).ok())
                .and_then(|d| tex.data.get(d))
                .map(|d| d.name.clone())
            else {
                continue;
            };
            let (patched, original) = crate::water::depth_material(&map_name, &name, wm.depth());
            if made.contains(&patched) {
                continue;
            }
            cubemaps.insert_patch(&original, &patched, "$waterdepth", &wm.depth().to_string());
            made.push(patched);
        }
    }

    let max_models = hp_bsp::limits::MODELS.max_usize_in(opts.format.limits);
    if models.len() + 1 > max_models {
        return Err(CompileError::Failed(format!("Too many brush models in map, max = {max_models}")));
    }
    let mut lumps = Lumps::new();
    lumps.format = opts.format;
    let mut occluders = occluders;
    if let Some(o) = occluders.as_mut() {
        o.register(&mut lumps);
    }
    lumps.model(
        ModelOut {
            tree: &tree,
            portals: &portals,
            faces: &mut faces,
            origin: DVec3::ZERO,
            detail_holder: Some(holder),
            disps: &disps,
            bounds: crate::brushbsp::brushes_bounds(&brushes, &world_brushes),
        },
        &planes,
        &tex,
        &brushes,
    );
    let world_leaves = std::mem::take(&mut lumps.model_leaves);
    let mut model_number = vec![0usize; entities.len()];
    for (k, (t, p, f, origin, ent)) in model_trees.iter_mut().enumerate() {
        lumps.model(
            ModelOut {
                tree: t,
                portals: p,
                faces: f,
                origin: *origin,
                detail_holder: None,
                disps: &[],
                bounds: crate::brushbsp::brushes_bounds(&brushes, &models[k].brushes),
            },
            &planes,
            &tex,
            &brushes,
        );
        model_number[*ent] = k + 1;
    }
    stage("emit models");
    lumps.brushes(&brushes, &mut planes);
    let mut phys_models = Vec::new();
    let leaf_brushes = |t: &Tree| -> Vec<usize> {
        let mut out = Vec::new();
        let mut stack = vec![t.head];
        while let Some(n) = stack.pop() {
            let node = &t.nodes[n];
            if node.is_leaf() {
                out.extend(node.brushes.iter().map(|b| b.original).chain(node.extra_brushes.iter().copied()));
            } else {
                stack.extend(node.children);
            }
        }
        out.retain(|&b| b < brushes.len());
        out
    };
    let mut groups: Vec<(i32, Vec<usize>)> = vec![(0, leaf_brushes(&tree))];
    for (k, (t, _, _, _, _)) in model_trees.iter().enumerate() {
        groups.push(((k + 1) as i32, leaf_brushes(t)));
    }
    let lump_of = |l: usize| lumps.data.get(&l).map_or(Vec::new(), |(d, _)| d.clone());
    let (brush_lump, side_lump) = (lump_of(crate::emit::lump::BRUSHES), lump_of(crate::emit::lump::BRUSHSIDES));
    let texinfo_prop = |t: i32| -> Option<String> {
        let info = tex.infos.get(usize::try_from(t).ok()?)?;
        Some(usize::try_from(info.texdata).ok().and_then(|d| tex.data.get(d)).map_or_else(String::new, |d| {
            if d.has_surfaceprop && d.surfaceprop.is_empty() { "default".to_owned() } else { d.surfaceprop.clone() }
        }))
    };
    let mut entity_visible: ahash::AHashSet<(usize, usize)> = ahash::AHashSet::new();
    for (t, p, _, _, _) in model_trees.iter_mut() {
        for pid in 0..p.list.len() {
            let [a, b] = p.list[pid].nodes;
            let filled = |n: usize| t.nodes[n].is_leaf() && t.nodes[n].contents != 0;
            if p.list[pid].on_node.is_none() || !(filled(a) || filled(b)) {
                continue;
            }
            if p.list[pid].side.is_none() {
                p.find_side(t, &planes, &brushes, pid);
            }
            if let Some(side) = p.list[pid].side {
                entity_visible.insert(side);
            }
        }
    }
    let phys_solid = |i: usize| -> crate::phys::ConvexSolid {
        let b = &brushes[i];
        let world = b.entity == 0;
        let sides = crate::phys::lump_brush_sides(&brush_lump, &side_lump, i)
            .into_iter()
            .enumerate()
            .map(|(k, (plane, texinfo, bevel))| {
                let p = planes.get(plane);
                crate::phys::BrushSide {
                    plane: [p.normal.x as f32, p.normal.y as f32, p.normal.z as f32, p.dist as f32],
                    bevel,
                    visible: if k >= b.sides.len() {
                        true
                    } else if world {
                        b.sides[k].visible
                    } else {
                        entity_visible.contains(&(i, k))
                    },
                    surfaceprop: texinfo_prop(texinfo),
                }
            })
            .collect();
        crate::phys::ConvexSolid { sides, brush: i as i32 }
    };
    let convexes = |ids: &[usize], mask: u32| -> Vec<crate::phys::ConvexSolid> {
        let mut ids: Vec<usize> = ids.iter().copied().filter(|&i| brushes[i].contents & mask != 0).collect();
        ids.sort_unstable();
        ids.dedup();
        ids.into_iter().map(phys_solid).collect()
    };
    let mut extras = crate::phys::WorldExtras {
        surface_props: crate::phys::SurfaceProps::load(&|p| materials.read_file(p)),
        ..Default::default()
    };
    for (index, ids) in groups {
        let mask = if index == 0 { PHYS_MASK & !contents::GRATE } else { MODEL_PHYS_MASK };
        let solids = convexes(&ids, mask);
        let mut model = crate::phys::PhysModel { model_index: index, solids, ..Default::default() };
        if index == 0 {
            for extra in [contents::GRATE, contents::PLAYERCLIP, contents::MONSTERCLIP] {
                let more = convexes(&ids, extra);
                if !more.is_empty() {
                    extras.statics.push((extra, more));
                }
            }
            if !opts.no_virtual_mesh && disps.iter().all(|d| d.info.power <= 3) {
                extras.virtual_terrain = true;
                lumps.data.insert(crate::emit::lump::PHYSDISP, (crate::disp::physdisp_lump(&disps, PHYS_MASK), 0));
            } else {
                extras.terrain = terrain_soups(&disps);
            }
        } else if let Some((_, _, f, _, _)) = model_trees.get(index as usize - 1) {
            let (lo, hi) = crate::brushbsp::brushes_bounds(&brushes, &models[index as usize - 1].brushes);
            model.size = [hi.x as f32 - lo.x as f32, hi.y as f32 - lo.y as f32, hi.z as f32 - lo.z as f32];
            let mut faces: Vec<(usize, String, f32)> = f
                .list
                .iter()
                .filter_map(|fc| {
                    let area = if fc.piece {
                        0.0
                    } else {
                        crate::brushbsp::area(&fc.winding.points.iter().map(|p| p.as_vec3()).collect())
                    };
                    fc.output.map(|o| (o, texinfo_prop(fc.texinfo).unwrap_or_default(), area))
                })
                .collect();
            faces.sort_by_key(|x| x.0);
            model.faces = faces.into_iter().map(|(_, p, a)| (p, a)).collect();
        }
        if !model.solids.is_empty() || index == 0 {
            phys_models.push(model);
        }
    }
    let fluids: Vec<crate::phys::Fluid> = water
        .models
        .iter()
        .filter(|wm| wm.model == 0 && !wm.brushes.is_empty())
        .map(|wm| crate::phys::Fluid {
            convexes: convexes(&wm.brushes, u32::MAX),
            surfaceprop: "water".to_owned(),
            damping: 0.01,
            contents: wm.contents,
            surface: wm.has_surface.then_some([
                wm.normal.x as f32,
                wm.normal.y as f32,
                wm.normal.z as f32,
                wm.dist as f32,
            ]),
        })
        .collect();
    extras.fluids = fluids;
    lumps.data.insert(crate::emit::lump::PHYSCOLLIDE, (crate::phys::physcollide_lump_world(&phys_models, &extras), 0));
    stage("physics");
    lumps.data.insert(crate::emit::lump::LEAFWATERDATA, (water.lump(), 0));
    let max_overlays = hp_bsp::limits::OVERLAYS.max_usize_in(opts.format.limits);
    let overlays = overlay_lumps(&entities, &lumps.side_faces, &mut tex, materials, max_overlays, log)?;
    if opts.format.overlay_system_levels {
        lumps.data.insert(crate::emit::lump::OVERLAY_SYSTEM_LEVELS, (vec![0; overlays.0.len() / 352 * 4], 0));
    }
    lumps.data.insert(crate::emit::lump::OVERLAYS, (overlays.0, 0));
    lumps.data.insert(crate::emit::lump::OVERLAY_FADES, (overlays.1, 0));
    if opts.format.worldlights_version != 0 {
        for l in [crate::emit::lump::WORLDLIGHTS, crate::emit::lump::WORLDLIGHTS_HDR] {
            lumps.data.insert(l, (Vec::new(), opts.format.worldlights_version));
        }
    }
    let (area_lump, areaportal_lump, clip_lump) = area_lumps;
    lumps.data.insert(crate::emit::lump::AREAS, (area_lump, 0));
    lumps.data.insert(crate::emit::lump::AREAPORTALS, (areaportal_lump, 0));
    lumps.data.insert(crate::emit::lump::CLIPPORTALVERTS, (clip_lump, 0));
    if let Some(o) = &occluders {
        let data = o.lump(&mut lumps);
        lumps.data.insert(crate::emit::lump::OCCLUSION, (data, 2));
    }
    lumps.finish(&planes, &tex);
    if lumps.dropped_vertices > 0 {
        log(&format!("{} vertices no face uses dropped (MAX_MAP_VERTS)", lumps.dropped_vertices));
    }
    if lumps.prims_over_limit() {
        let (prims, indices) = lumps.prims_wanted;
        log(&format!(
            "Warning: Exceeded max t-junctions fixup limit! ({prims}/{} primitives, {indices}/{} indices)",
            crate::emit::MAX_MAP_PRIMITIVES,
            crate::emit::MAX_MAP_PRIMINDICES
        ));
    }
    {
        use crate::emit::lump;
        let get = |i: usize| lumps.data.get(&i).map_or(&[][..], |(d, _)| d.as_slice());
        let (normals, indices) = crate::emit::flat_vertex_normals(get(lump::PLANES), get(lump::FACES));
        lumps.data.insert(lump::VERTNORMALS, (normals, 0));
        lumps.data.insert(lump::VERTNORMALINDICES, (indices, 0));
        for l in [lump::LIGHTING_HDR, lump::LEAF_AMBIENT_LIGHTING_HDR, lump::LEAF_AMBIENT_LIGHTING] {
            lumps.data.entry(l).or_insert((Vec::new(), 1));
        }
        lumps.data.insert(lump::MAP_FLAGS, (vec![0; 4], 0));
        let macro_of = |name: &str| -> Option<String> {
            let key = crate::detail_props::original_material(name).to_ascii_lowercase();
            facts_cache
                .get(&key)
                .or_else(|| facts_cache.get(&name.to_ascii_lowercase()))
                .and_then(|f| f.macro_texture.clone())
        };
        let macros = crate::emit::face_macro_textures(&mut lumps.data, &macro_of);
        lumps.data.insert(lump::FACE_MACRO_TEXTURE_INFO, (macros, 0));
    }
    stage("overlays, finish");
    let mut computed = false;
    let mut pvs_rows: Option<Vec<Vec<u8>>> = None;
    let mut trace_text: Option<String> = None;
    let mut vis_run = crate::stats::VisRun {
        mode: match opts.vis {
            VisLevel::Off => "off",
            VisLevel::Fast => "fast",
            VisLevel::Full => "full",
        }
        .to_owned(),
        portals: vis_portals.len(),
        ..Default::default()
    };
    if opts.vis != VisLevel::Off && leak_path.is_some() {
        vis_run.mode = "skipped (leak)".to_owned();
    }
    if opts.vis != VisLevel::Off && leak_path.is_none() && clusters > 0 {
        let start_vis = std::time::Instant::now();
        let portals: Vec<crate::vis::VisPortal> = vis_portals
            .iter()
            .map(|p| crate::vis::VisPortal { points: p.points.clone(), clusters: p.clusters })
            .collect();
        let radius = entities
            .iter()
            .find(|e| e.classname.eq_ignore_ascii_case("env_fog_controller"))
            .map(|e| key(e, "farz").and_then(|v| v.trim().parse::<f64>().ok()).unwrap_or(0.0))
            .filter(|v| *v > 0.0);
        if let Some(r) = opts.radius_override {
            log(&format!("Vis Radius = {r:4.2} (-radius_override)"));
            vis_run.radius = Some((r, "-radius_override".to_owned()));
        } else if let Some(r) = radius {
            log(&format!("Using visradius of {r:.0} (env_fog_controller farz)"));
            vis_run.radius = Some((r, "env_fog_controller farz".to_owned()));
        }
        let vopts = crate::vis::VisOptions {
            fast: opts.vis == VisLevel::Fast,
            threads: opts.threads,
            sort: opts.vis_sort,
            radius_override: opts.radius_override,
            ..Default::default()
        };
        if let Some(t) = opts.vis_trace {
            let topts = crate::vis::VisOptions { trace: Some(t), ..vopts };
            match crate::vis::compute_trace(clusters as usize, &portals, &topts, radius) {
                Ok(Some(trace)) => {
                    log(&format!("Trace from cluster {} to cluster {}: {} points", t.0, t.1, trace.points.len()));
                    trace_text = Some(trace.text());
                }
                Ok(None) => log(&format!("Trace: cluster {} does not see cluster {}", t.0, t.1)),
                Err(e) => log(&e),
            }
        }
        let share = if vopts.fast { 0.9 } else { 0.05 };
        let t_pvs = std::time::Instant::now();
        let mut base_secs: Option<f64> = None;
        let lump_of = |i: usize| lumps.data.get(&i).map_or(&[][..], |(d, _)| d.as_slice());
        let sky = crate::vis::sky_clusters(
            lump_of(crate::emit::lump::LEAFS),
            lump_of(crate::emit::lump::LEAFFACES),
            lump_of(crate::emit::lump::FACES),
            lump_of(crate::emit::lump::TEXINFO),
            clusters as usize,
        );
        let pvs = crate::vis::compute_pvs_for_sky(clusters as usize, &portals, &vopts, radius, &sky, &mut |f| {
            if base_secs.is_none() && f >= share {
                base_secs = Some(t_pvs.elapsed().as_secs_f64());
            }
        });
        let pvs_secs = t_pvs.elapsed().as_secs_f64();
        if pvs.is_empty() {
            log("Warning: too many clusters for the visibility, everything is drawn");
            vis_run.mode = "off".to_owned();
        } else {
            let t_pas = std::time::Instant::now();
            let pas = crate::vis::compute_pas_with(&pvs, &vopts);
            let base = base_secs.unwrap_or(pvs_secs).min(pvs_secs);
            vis_run.times = vec![
                ("base".to_owned(), base),
                ((if vopts.fast { "cluster merge" } else { "flow" }).to_owned(), pvs_secs - base),
                ("pas".to_owned(), t_pas.elapsed().as_secs_f64()),
            ];
            let seen: usize = pvs.iter().map(|r| r.iter().map(|b| b.count_ones() as usize).sum::<usize>()).sum();
            log(&format!(
                "{} visibility: {} clusters, {} portals, average {:.1} visible clusters, {:.2} s",
                if vopts.fast { "Fast" } else { "Full" },
                clusters,
                portals.len(),
                seen as f64 / clusters as f64,
                start_vis.elapsed().as_secs_f64()
            ));
            lumps.data.insert(crate::emit::lump::VISIBILITY, (crate::vis::vis_lump(&pvs, &pas), 0));
            if (radius.is_some() || opts.radius_override.is_some())
                && let Some((leafs, _)) = lumps.data.get_mut(&crate::emit::lump::LEAFS)
            {
                for leaf in leafs.chunks_exact_mut(32) {
                    let v = u16::from_le_bytes([leaf[6], leaf[7]]) | (crate::emit::LEAF_FLAGS_RADIAL << 9);
                    leaf[6..8].copy_from_slice(&v.to_le_bytes());
                }
            }
            pvs_rows = Some(pvs);
            computed = true;
        }
    } else if opts.vis != VisLevel::Off && leak_path.is_some() {
        log("Visibility skipped: the map leaks (everything is drawn)");
    }
    stage("visibility");
    let has_water = brushes.iter().any(|b| b.contents & contents::MASK_WATER != 0);
    if computed {
        let marked = crate::water::fog_volumes(&mut lumps.data, pvs_rows.as_deref());
        if has_water {
            log(&format!("{marked} leaves see water"));
        }
    } else {
        lumps.data.insert(crate::emit::lump::LEAFMINDISTTOWATER, (vec![0; lumps.leaf_count() * 2], 0));
    }
    if !lumps.errors.is_empty() {
        let fatal = |e: &str| {
            (e.contains("exceeded") || e.starts_with("Bad surface extents"))
                && !(opts.relaxed_limits && relaxable_limit(e))
        };
        for e in &lumps.errors {
            if fatal(e) || !opts.relaxed_limits {
                log(e);
            } else {
                log(&format!("Warning: {e} ({RELAXED_NOTE})"));
            }
        }
        if lumps.errors.iter().any(|e| fatal(e)) {
            if let Ok(file) = hp_bsp::Bsp::from_vec(lumps.file(map.version_info.map_version)) {
                for l in crate::stats::limit_lines_in(&file, opts.format.limits) {
                    log(&l);
                }
            }
            return Err(CompileError::Failed(lumps.errors.join("\n")));
        }
    }

    let world_box = bounds_no_skybox(&lumps.data, &disps, &sky_areas).unwrap_or((wmins, wmaxs));
    let mut out_ents = entity_list(
        &entities,
        &EntityKeys {
            removed: &removed,
            model_number: &model_number,
            origin_brush: &origin_brush_of,
            ladders: &ladders,
            world_box: Some(world_box),
            map_version: map.version_info.map_version.to_string(),
            format: opts.format,
        },
    )?;
    if has_water && crate::water::ensure_lod_control(&mut out_ents) {
        log("Water found with no water_lod_control entity, creating a default one.");
    }
    lumps.data.insert(crate::emit::lump::ENTITIES, (hp_bsp::write_entities(&out_ents), 0));
    stage("water fog, entities");
    let lighting = |name: &str| {
        entities
            .iter()
            .rev()
            .find(|e| {
                e.classname.eq_ignore_ascii_case("info_lighting")
                    && key(e, "targetname").is_some_and(|t| t.eq_ignore_ascii_case(name))
            })
            .and_then(|e| key(e, "origin").and_then(parse_vec))
    };
    let (props, prop_ids): (Vec<crate::props::StaticProp>, Vec<u32>) = entities
        .iter()
        .filter(|e| e.classname.eq_ignore_ascii_case("prop_static"))
        .filter_map(|e| crate::props::StaticProp::from_entity(e, &lighting).map(|p| (p, e.id)))
        .unzip();
    let leaf_numbers: ahash::AHashMap<usize, u16> = world_leaves.iter().copied().collect();
    let mut warnings = Vec::new();
    let (sprp, outside) = crate::props::sprp_lump(
        &props,
        model_files,
        &tree,
        &planes,
        opts.format,
        &|n| leaf_numbers.get(&n).copied(),
        &mut |w| warnings.push(w),
    );
    for w in warnings {
        log(&w);
    }
    if opts.prop_debug {
        let outside: Vec<u32> = outside.iter().map(|&k| prop_ids[k]).collect();
        for l in crate::prop_debug::report(&entities, model_files, &outside) {
            log(&l);
        }
    }
    lumps.game_lumps.push((u32::from_be_bytes(*b"sprp"), opts.format.static_props.version(), sprp));
    log(&format!("{} static props", props.len()));
    let get_lump = |i: usize| lumps.data.get(&i).map(|(d, _)| d.clone()).unwrap_or_default();
    let dprp = crate::detail_props::compile_lump(&entities, &get_lump, materials, model_files, &blockers, log);
    lumps.game_lumps.push((crate::detail_props::DPRP, opts.format.detail_props_version, dprp));
    stage("props");

    crate::cubemap::emit(&mut cubemaps, &mut lumps, key(&map.world, "skyname"), log);
    let faces_written = lumps.data.get(&crate::emit::lump::FACES).map_or(0, |(d, _)| d.len() / 56);
    stage("cubemaps");
    let bsp = lumps.file(map.version_info.map_version);
    let mut problems = crate::validate::validate(&bsp);
    if opts.relaxed_limits {
        let relaxed = ["too many edges", "too many surface edges", "too many brush sides"];
        problems.retain(|p| !relaxed.iter().any(|r| p.starts_with(r)));
    }
    if !problems.is_empty() {
        for p in &problems {
            log(&format!("Error! {p}"));
        }
        return Err(CompileError::Failed(format!("the BSP failed its checks ({} problems)", problems.len())));
    }
    stage("write, validate");
    log("BSP checked: every index and range is valid");
    if let Ok(file) = hp_bsp::Bsp::from_vec(bsp.clone()) {
        let classes: Vec<&str> = entities.iter().map(|e| e.classname.as_str()).collect();
        let input = crate::stats::GeometryInput {
            map,
            brushes: &brushes,
            classes: &classes,
            stages: &stages,
            limits: opts.format.limits,
        };
        for l in crate::stats::geometry_lines(&file, &input) {
            log(&l);
        }
        for l in crate::stats::vis_lines(&file, &vis_run) {
            log(&l);
        }
    }
    log(&format!(
        "{} faces, {} leaves, {:.1} MB, {:.2} s elapsed",
        faces_written,
        lumps.leaf_count(),
        bsp.len() as f64 / 1_048_576.0,
        start.elapsed().as_secs_f64()
    ));
    Ok(CompileOutput {
        bsp,
        leak: leak_path,
        clusters,
        faces: faces_written,
        brushes: brushes.len(),
        vis_portals,
        prt,
        trace: trace_text,
    })
}

fn terrain_soups(disps: &[crate::disp::DispFace]) -> Vec<Vec<crate::phys::SoupTriangle>> {
    const MAX_COORD: f64 = 16384.0;
    let mut cells: Vec<(i64, Vec<usize>)> = Vec::new();
    let mut tris: Vec<Vec<([DVec3; 3], f64)>> = Vec::with_capacity(disps.len());
    for (i, d) in disps.iter().enumerate() {
        let t = crate::disp::collision_triangles(d);
        let solid = d.contents & PHYS_MASK != 0 && d.info.flags & hp_geom::DISP_FLAG_NO_PHYSICS == 0;
        if solid && !t.is_empty() {
            let (mut lo, mut hi) = (DVec3::splat(f64::MAX), DVec3::splat(f64::MIN));
            for p in t.iter().flat_map(|(p, _)| p.iter()) {
                lo = lo.min(*p);
                hi = hi.max(*p);
            }
            let c = (lo + hi) * 0.5 + DVec3::splat(MAX_COORD);
            let cell = |v: f64, size: f64| ((v / size) as i64) & 0xff;
            let id = cell(c.x, 4096.0) | (cell(c.y, 4096.0) << 8) | (cell(c.z, 8192.0) << 16);
            match cells.iter_mut().find(|(k, _)| *k == id) {
                Some((_, list)) => list.push(i),
                None => cells.push((id, vec![i])),
            }
        }
        tris.push(t);
    }
    let mut soups = Vec::new();
    for (_, list) in cells {
        let mut soup: Vec<crate::phys::SoupTriangle> = Vec::new();
        let mut count = 0usize;
        for i in list {
            let d = &disps[i];
            count += tris[i].len();
            if count >= 65536 {
                soups.push(std::mem::take(&mut soup));
                count = tris[i].len();
            }
            for (points, alpha) in &tris[i] {
                let prop = match &d.surfaceprop2 {
                    Some(p2) if *alpha > crate::disp::ALPHA_PROP_DELTA => p2.as_str(),
                    _ => d.surfaceprop.as_str(),
                };
                soup.push(crate::phys::SoupTriangle { points: *points, surfaceprop: prop.to_owned() });
            }
        }
        if !soup.is_empty() {
            soups.push(soup);
        }
    }
    soups
}

fn bounds_no_skybox(
    data: &ahash::AHashMap<usize, (Vec<u8>, u32)>,
    disps: &[crate::disp::DispFace],
    sky_areas: &[i32],
) -> Option<(DVec3, DVec3)> {
    use crate::emit::lump;
    let get = |i: usize| data.get(&i).map_or(&[][..], |(d, _)| d.as_slice());
    let (planes, nodes, leafs, models) = (get(lump::PLANES), get(lump::NODES), get(lump::LEAFS), get(lump::MODELS));
    let (faces, texinfo, leaffaces) = (get(lump::FACES), get(lump::TEXINFO), get(lump::LEAFFACES));
    let (surfedges, edges, verts) = (get(lump::SURFEDGES), get(lump::EDGES), get(lump::VERTEXES));
    let i32_at = |d: &[u8], o: usize| d.get(o..o + 4).map_or(0, |b| i32::from_le_bytes([b[0], b[1], b[2], b[3]]));
    let f32_at = |d: &[u8], o: usize| d.get(o..o + 4).map_or(0.0, |b| f32::from_le_bytes([b[0], b[1], b[2], b[3]]));
    let u16_at = |d: &[u8], o: usize| d.get(o..o + 2).map_or(0, |b| u16::from_le_bytes([b[0], b[1]]));
    let vertex = |v: usize| {
        DVec3::new(
            f64::from(f32_at(verts, v * 12)),
            f64::from(f32_at(verts, v * 12 + 4)),
            f64::from(f32_at(verts, v * 12 + 8)),
        )
    };
    let counts = |leaf: usize| {
        let o = leaf * 32;
        let area = i32::from(u16_at(leafs, o + 6) & 0x1ff);
        i32_at(leafs, o) & contents::SOLID as i32 == 0 && !sky_areas.contains(&area)
    };
    let head = i32_at(models, 36);
    let (mut lo, mut hi) = (DVec3::splat(f64::MAX), DVec3::splat(f64::MIN));
    let mut stack = vec![head];
    while let Some(n) = stack.pop() {
        if n >= 0 {
            let o = n as usize * 32;
            if o + 32 > nodes.len() {
                continue;
            }
            stack.push(i32_at(nodes, o + 4));
            stack.push(i32_at(nodes, o + 8));
            continue;
        }
        let leaf = (-1 - n) as usize;
        if !counts(leaf) {
            continue;
        }
        let o = leaf * 32;
        let (first, count) = (usize::from(u16_at(leafs, o + 20)), usize::from(u16_at(leafs, o + 22)));
        for k in first..first + count {
            let f = usize::from(u16_at(leaffaces, k * 2)) * 56;
            let ti = u16_at(faces, f + 10) as i16;
            if ti < 0 || i32_at(texinfo, ti as usize * 72 + 64) as u32 & (surf::SKY | surf::NODRAW) != 0 {
                continue;
            }
            let (fe, ne) = (i32_at(faces, f + 4), u16_at(faces, f + 8) as i16);
            for e in fe..fe + i32::from(ne) {
                let se = i32_at(surfedges, e as usize * 4);
                let v = usize::from(u16_at(edges, se.unsigned_abs() as usize * 4 + if se < 0 { 2 } else { 0 }));
                let p = vertex(v);
                lo = lo.min(p);
                hi = hi.max(p);
            }
        }
    }
    fn box_inside_world_r(
        node: i32,
        lo: glam::Vec3,
        hi: glam::Vec3,
        side_of: &dyn Fn(i32, glam::Vec3, glam::Vec3) -> (u8, [i32; 2]),
        counts: &dyn Fn(usize) -> bool,
        node_count: usize,
    ) -> bool {
        let mut node = node;
        loop {
            if node < 0 {
                return counts((-1 - node) as usize);
            }
            if node as usize >= node_count {
                return false;
            }
            let (side, [front, back]) = side_of(node, lo, hi);
            match side {
                1 => node = front,
                2 => node = back,
                _ => {
                    if box_inside_world_r(front, lo, hi, side_of, counts, node_count) {
                        return true;
                    }
                    node = back;
                }
            }
        }
    }
    let side_of = |node: i32, lo: glam::Vec3, hi: glam::Vec3| -> (u8, [i32; 2]) {
        const EPS: f32 = 0.001;
        let o = node as usize * 32;
        let pl = i32_at(nodes, o) as usize * 20;
        let n = glam::Vec3::new(f32_at(planes, pl), f32_at(planes, pl + 4), f32_at(planes, pl + 8));
        let dist = f32_at(planes, pl + 12);
        let kind = i32_at(planes, pl + 16);
        let children = [i32_at(nodes, o + 4), i32_at(nodes, o + 8)];
        let mut side = 0u8;
        if (0..3).contains(&kind) {
            let t = kind as usize;
            if hi[t] > dist + EPS {
                side |= 1;
            }
            if lo[t] < dist - EPS {
                side |= 2;
            }
            return (side, children);
        }
        let (mut c0, mut c1) = (glam::Vec3::ZERO, glam::Vec3::ZERO);
        for i in 0..3 {
            if n[i] < 0.0 {
                c0[i] = lo[i];
                c1[i] = hi[i];
            } else {
                c1[i] = lo[i];
                c0[i] = hi[i];
            }
        }
        let d1 = n.dot(c0) - dist;
        let d2 = n.dot(c1) - dist;
        if d1 >= EPS {
            side = 1;
        }
        if d2 < EPS {
            side |= 2;
        }
        (side, children)
    };
    let box_inside_world = |head: i32, lo: glam::Vec3, hi: glam::Vec3| {
        box_inside_world_r(head, lo, hi, &side_of, &counts, nodes.len() / 32)
    };
    for d in disps {
        let (mut flo, mut fhi) = (glam::Vec3::splat(1e24), glam::Vec3::splat(-1e24));
        for p in d.winding.points.iter().take(4) {
            flo = flo.min(p.as_vec3());
            fhi = fhi.max(p.as_vec3());
        }
        let puff = glam::Vec3::splat(0.1);
        let (dlo, dhi) = ((flo - puff).as_dvec3(), (fhi + puff).as_dvec3());
        let inside = box_inside_world(head, dlo.as_vec3(), dhi.as_vec3());
        if inside {
            lo = lo.min(dlo);
            hi = hi.max(dhi);
        }
    }
    (lo.cmple(hi).all()).then(|| (lo.trunc(), hi.trunc()))
}

pub(crate) struct EntityKeys<'a> {
    pub(crate) removed: &'a [bool],
    pub(crate) model_number: &'a [usize],
    pub(crate) origin_brush: &'a [Option<DVec3>],
    pub(crate) ladders: &'a [Option<(DVec3, DVec3)>],
    pub(crate) world_box: Option<(DVec3, DVec3)>,
    pub(crate) map_version: String,
    pub(crate) format: hp_bsp::BspFormat,
}

const VBSP_VERSION: &str = "Oct 22 2026";

pub(crate) fn entity_list(
    entities: &[&hp_vmf::Entity],
    keys: &EntityKeys<'_>,
) -> Result<Vec<hp_bsp::Entity>, CompileError> {
    fn set(list: &mut Vec<(String, String)>, key: &str, value: String) {
        match list.iter_mut().find(|(k, _)| k.eq_ignore_ascii_case(key)) {
            Some((_, v)) => *v = value,
            None => list.insert(0, (key.to_owned(), value)),
        }
    }
    let get = |list: &[(String, String)], key: &str| {
        list.iter().find(|(k, _)| k.eq_ignore_ascii_case(key)).map(|(_, v)| v.clone())
    };
    let portal_numbers: ahash::AHashMap<usize, u16> =
        crate::areas::areaportal_entities(entities).iter().map(|ap| (ap.entity, ap.number)).collect();
    let mut removed = keys.removed.to_vec();
    let mut lists: Vec<Vec<(String, String)>> = Vec::with_capacity(entities.len());
    let mut overlay = 0usize;
    for (ei, ent) in entities.iter().enumerate() {
        let mut l: Vec<(String, String)> = Vec::new();
        set(&mut l, "hammerid", ent.id.to_string());
        if ei == 0 {
            set(&mut l, "mapversion", keys.map_version.clone());
        }
        set(&mut l, "classname", ent.classname.clone());
        for (k, v) in &ent.props {
            if k.eq_ignore_ascii_case("id") {
                continue;
            }
            set(&mut l, k, v.clone());
        }
        for c in &ent.connections {
            l.push((c.output.clone(), c.to_value_string()));
        }
        if let Some(o) = keys.origin_brush.get(ei).copied().flatten() {
            set(&mut l, "origin", format!("{} {} {}", o.x as i32, o.y as i32, o.z as i32));
        }
        let class = ent.classname.to_ascii_lowercase();
        if class == "func_ladder"
            && let Some((lo, hi)) = keys.ladders.get(ei).copied().flatten()
        {
            let f = |v: f64| format!("{:.2}", v as f32);
            for (k, v) in [
                ("mins.x", lo.x),
                ("mins.y", lo.y),
                ("mins.z", lo.z),
                ("maxs.x", hi.x),
                ("maxs.y", hi.y),
                ("maxs.z", hi.z),
            ] {
                set(&mut l, k, f(v));
            }
            set(&mut l, "classname", "info_ladder".to_owned());
        }
        if class == "info_overlay" {
            if get(&l, "targetname").is_some_and(|t| !t.is_empty()) {
                set(&mut l, "classname", "info_overlay_accessor".to_owned());
                set(&mut l, "OverlayID", overlay.to_string());
                removed[ei] = false;
            } else {
                removed[ei] = true;
            }
            overlay += 1;
        }
        if let Some(n) = portal_numbers.get(&ei) {
            set(&mut l, "portalnumber", n.to_string());
        }
        lists.push(l);
    }
    let mut node_ids: Vec<(i32, i32)> = Vec::new();
    for (ei, l) in lists.iter_mut().enumerate() {
        if keys.removed.get(ei).copied().unwrap_or(false) {
            continue;
        }
        let class = get(l, "classname").unwrap_or_default().to_ascii_lowercase();
        let node_keys: &[&str] = match class.as_str() {
            "info_node_hint" | "info_node_air_hint" | "info_hint" | "info_node_climb" => &["TargetNode"],
            "info_node_link" => &["StartNode", "EndNode"],
            _ => &[],
        };
        for k in node_keys {
            let value = crate::cubemap::atoi(&get(l, k).unwrap_or_default());
            let id = match node_ids.iter().find(|(v, _)| *v == value) {
                Some(&(_, id)) => id,
                None => {
                    let id = node_ids.len() as i32 + 1;
                    node_ids.push((value, id));
                    id
                }
            };
            set(l, k, id.to_string());
        }
    }
    let occluders: Vec<(usize, usize)> = crate::occluder::occluder_numbers(entities);
    for (ei, l) in lists.iter_mut().enumerate().skip(1) {
        if keys.model_number.get(ei).copied().unwrap_or(0) > 0 {
            set(l, "model", format!("*{}", keys.model_number[ei]));
        } else if occluders.iter().any(|(e, _)| *e == ei) && !entities[ei].solids.is_empty() {
            set(l, "model", String::new());
        }
    }
    let mut names: Vec<String> = Vec::new();
    for l in lists.iter_mut().skip(1) {
        let class = get(l, "classname").unwrap_or_default();
        if !class.get(..5).is_some_and(|p| p.eq_ignore_ascii_case("light"))
            || class.eq_ignore_ascii_case("light_dynamic")
        {
            continue;
        }
        let Some(t) = get(l, "targetname").filter(|t| !t.is_empty()) else { continue };
        let max_switched = hp_bsp::limits::SWITCHED_LIGHTS.max_usize_in(keys.format.limits);
        let j = match names.iter().position(|n| *n == t) {
            Some(j) => j,
            None if names.len() >= max_switched => {
                return Err(CompileError::Failed(format!(
                    "Too many switched lights (error at light {t}), max = {max_switched}"
                )));
            }
            None => {
                names.push(t);
                names.len() - 1
            }
        };
        let current = get(l, "style").unwrap_or_default();
        if crate::cubemap::atoi(&current) != 0 {
            set(l, "defaultstyle", current);
        }
        set(l, "style", (32 + j).to_string());
    }
    for (ei, k) in occluders {
        set(&mut lists[ei], "occludernumber", k.to_string());
    }
    if let Some(world) = lists.first_mut() {
        if keys.format.branch == hp_bsp::Branch::GarrysMod {
            set(world, "vbsp_exe", "gmod".to_owned());
            set(world, "vbsp_version", VBSP_VERSION.to_owned());
        }
        if let Some((lo, hi)) = keys.world_box {
            set(world, "world_mins", format!("{} {} {}", lo.x as i32, lo.y as i32, lo.z as i32));
            set(world, "world_maxs", format!("{} {} {}", hi.x as i32, hi.y as i32, hi.z as i32));
        }
    }
    let strip = |s: &str| s.trim_end_matches(|c: char| (c as u32) <= 32).to_owned();
    Ok(lists
        .into_iter()
        .zip(removed)
        .filter(|(_, r)| !r)
        .map(|(l, _)| hp_bsp::Entity { pairs: l.into_iter().map(|(k, v)| (strip(&k), strip(&v))).collect() })
        .collect())
}

pub fn entities_only(
    map: &hp_vmf::Map,
    materials: &dyn MaterialSource,
    existing: &[u8],
) -> Result<Vec<u8>, CompileError> {
    entities_only_for(map, materials, existing, hp_bsp::BspFormat::default())
}

pub fn entities_only_for(
    map: &hp_vmf::Map,
    materials: &dyn MaterialSource,
    existing: &[u8],
    format: hp_bsp::BspFormat,
) -> Result<Vec<u8>, CompileError> {
    let visible = without_hidden(map);
    let map = visible.as_ref().unwrap_or(map);
    let bsp = hp_bsp::Bsp::from_vec(existing.to_vec()).map_err(|e| CompileError::Failed(e.to_string()))?;
    let old = bsp.entities().unwrap_or_default();
    let mut entities: Vec<&hp_vmf::Entity> = vec![&map.world];
    entities.extend(map.entities.iter());
    let mut removed = vec![false; entities.len()];
    let mut model_number = vec![0usize; entities.len()];
    let mut origins_of: Vec<Option<DVec3>> = vec![None; entities.len()];
    let is_origin = |s: &hp_vmf::Solid| {
        materials.facts(s.sides.first().map(|x| x.material.as_str()).unwrap_or("")).contents & contents::ORIGIN != 0
    };
    let mut ladders: Vec<Option<(DVec3, DVec3)>> = vec![None; entities.len()];
    let mut origin_brush_of: Vec<Option<DVec3>> = vec![None; entities.len()];
    let mut next = 1;
    for (ei, ent) in entities.iter().enumerate() {
        removed[ei] = removed_class(&ent.classname);
        if ent.classname.eq_ignore_ascii_case("func_ladder") {
            let mut planes = PlaneSet::new();
            let mut tex = TexInfos::new();
            let mut facts = |n: &str| materials.facts(n);
            let built: Vec<MapBrush> = ent
                .solids
                .iter()
                .filter_map(|s| {
                    let mut b = build_brush(
                        s,
                        ei,
                        DVec3::ZERO,
                        &BrushLoad::plain(contents::LADDER, false),
                        &mut planes,
                        &mut tex,
                        &mut facts,
                        &mut Vec::new(),
                        &ahash::AHashMap::new(),
                        &mut |_| {},
                    )
                    .ok()
                    .flatten()?;
                    b.add_bevels(&mut planes);
                    Some(b)
                })
                .collect();
            ladders[ei] = Some(ladder_bounds(built.iter()));
        }
        origins_of[ei] = key(ent, "origin").and_then(parse_vec);
        let special =
            crate::areas::is_areaportal_class(&ent.classname) || crate::occluder::is_occluder_class(&ent.classname);
        if ei == 0 || ent.solids.is_empty() || merges_into_world(&ent.classname) || removed[ei] || special {
            continue;
        }
        for solid in ent.solids.iter().filter(|s| is_origin(s)) {
            let (mut lo, mut hi) = (DVec3::splat(f64::MAX), DVec3::splat(f64::MIN));
            for s in &solid.sides {
                for p in s.plane {
                    lo = lo.min(p);
                    hi = hi.max(p);
                }
            }
            origins_of[ei] = Some(origin_brush_center(lo, hi));
            origin_brush_of[ei] = origins_of[ei];
        }
        if origin_brush_of[ei].is_none()
            && let Some(o) = train_origin(ent, ent.solids.iter().filter(|s| !is_origin(s)))
        {
            origins_of[ei] = Some(o);
            origin_brush_of[ei] = Some(o);
        }
        let mut planes = PlaneSet::new();
        let mut tex = TexInfos::new();
        let mut facts = |n: &str| materials.facts(n);
        let origin = origins_of[ei].unwrap_or(DVec3::ZERO);
        let builds = ent.solids.iter().filter(|s| !is_origin(s)).any(|s| {
            matches!(
                build_brush(
                    s,
                    ei,
                    origin,
                    &BrushLoad::plain(0, false),
                    &mut planes,
                    &mut tex,
                    &mut facts,
                    &mut Vec::new(),
                    &ahash::AHashMap::new(),
                    &mut |_| {}
                ),
                Ok(Some(_))
            )
        });
        if builds {
            model_number[ei] = next;
            next += 1;
        }
    }
    let models = bsp.lump(crate::emit::lump::MODELS).map(|d| d.len() / 48).unwrap_or(0);
    if next != models {
        return Err(CompileError::Failed(format!(
            "the brush entities changed since the last compile ({models} models, {next} now): compile the whole map"
        )));
    }
    let world_box = old.first().and_then(|w| {
        let lo = w.get("world_mins").and_then(parse_vec)?;
        let hi = w.get("world_maxs").and_then(parse_vec)?;
        Some((lo, hi))
    });
    let ents = entity_list(
        &entities,
        &EntityKeys {
            removed: &removed,
            model_number: &model_number,
            origin_brush: &origin_brush_of,
            ladders: &ladders,
            world_box,
            map_version: map.version_info.map_version.to_string(),
            format,
        },
    )?;
    let mut edit = bsp.edit();
    edit.set_entities(&ents).map_err(|e| CompileError::Failed(e.to_string()))?;
    edit.to_vec().map_err(|e| CompileError::Failed(e.to_string()))
}

pub fn props_only(
    map: &hp_vmf::Map,
    materials: &dyn MaterialSource,
    model_files: &dyn crate::props::ModelSource,
    existing: &[u8],
    prop_debug: bool,
    log: &mut dyn FnMut(&str),
) -> Result<Vec<u8>, CompileError> {
    props_only_for(map, materials, model_files, existing, prop_debug, hp_bsp::BspFormat::default(), log)
}

pub fn props_only_for(
    map: &hp_vmf::Map,
    materials: &dyn MaterialSource,
    model_files: &dyn crate::props::ModelSource,
    existing: &[u8],
    prop_debug: bool,
    format: hp_bsp::BspFormat,
    log: &mut dyn FnMut(&str),
) -> Result<Vec<u8>, CompileError> {
    let visible = without_hidden(map);
    let map = visible.as_ref().unwrap_or(map);
    let bsp = hp_bsp::Bsp::from_vec(existing.to_vec()).map_err(|e| CompileError::Failed(e.to_string()))?;
    let get = |i: usize| bsp.lump(i).map(|c| c.into_owned()).unwrap_or_default();
    let (planes, nodes, leafs) =
        (get(crate::emit::lump::PLANES), get(crate::emit::lump::NODES), get(crate::emit::lump::LEAFS));
    let f32_at = |d: &[u8], o: usize| d.get(o..o + 4).map_or(0.0, |b| f32::from_le_bytes([b[0], b[1], b[2], b[3]]));
    let i32_at = |d: &[u8], o: usize| d.get(o..o + 4).map_or(0, |b| i32::from_le_bytes([b[0], b[1], b[2], b[3]]));
    let head = i32_at(&get(crate::emit::lump::MODELS), 36);
    let mut entities: Vec<&hp_vmf::Entity> = vec![&map.world];
    entities.extend(map.entities.iter());
    let lighting = |name: &str| {
        entities
            .iter()
            .rev()
            .find(|e| {
                e.classname.eq_ignore_ascii_case("info_lighting")
                    && key(e, "targetname").is_some_and(|t| t.eq_ignore_ascii_case(name))
            })
            .and_then(|e| key(e, "origin").and_then(parse_vec))
    };
    let (props, prop_ids): (Vec<crate::props::StaticProp>, Vec<u32>) = entities
        .iter()
        .filter(|e| e.classname.eq_ignore_ascii_case("prop_static"))
        .filter_map(|e| crate::props::StaticProp::from_entity(e, &lighting).map(|p| (p, e.id)))
        .unzip();
    let step = |n: usize| -> crate::props::TreeStep {
        let count = nodes.len() / 32;
        if n >= count {
            let leaf = n - count;
            return crate::props::TreeStep::Leaf { open: i32_at(&leafs, leaf * 32) & contents::SOLID as i32 == 0 };
        }
        let o = n * 32;
        let p = i32_at(&nodes, o).max(0) as usize * 20;
        let normal = DVec3::new(
            f64::from(f32_at(&planes, p)),
            f64::from(f32_at(&planes, p + 4)),
            f64::from(f32_at(&planes, p + 8)),
        );
        let child = |c: i32| if c >= 0 { c as usize } else { count + (-1 - c) as usize };
        crate::props::TreeStep::Node {
            normal,
            dist: f64::from(f32_at(&planes, p + 12)),
            children: [child(i32_at(&nodes, o + 4)), child(i32_at(&nodes, o + 8))],
        }
    };
    let count = nodes.len() / 32;
    let (sprp, outside) = crate::props::sprp_lump_tree(
        &props,
        model_files,
        head.max(0) as usize,
        &step,
        format,
        &|n| n.checked_sub(count).map(|l| l as u16),
        &mut |w| log(&w),
    );
    if prop_debug {
        let outside: Vec<u32> = outside.iter().map(|&k| prop_ids[k]).collect();
        for l in crate::prop_debug::report(&entities, model_files, &outside) {
            log(&l);
        }
    }
    let mut games = bsp.game_lumps().map_err(|e| CompileError::Failed(e.to_string()))?;
    games
        .set(u32::from_be_bytes(*b"sprp"), format.static_props.version(), sprp)
        .map_err(|e| CompileError::Failed(e.to_string()))?;
    let mut blockers = Vec::new();
    {
        let mut planes = PlaneSet::new();
        let mut tex = TexInfos::new();
        let mut facts = |n: &str| materials.facts(n);
        let mut built: Vec<MapBrush> = Vec::new();
        for (ei, e) in
            entities.iter().enumerate().filter(|(_, e)| e.classname.eq_ignore_ascii_case("func_detail_blocker"))
        {
            let origin = key(e, "origin").and_then(parse_vec).unwrap_or(DVec3::ZERO);
            let mut ids = Vec::new();
            for solid in &e.solids {
                if let Ok(Some(mut b)) = build_brush(
                    solid,
                    ei,
                    DVec3::ZERO,
                    &BrushLoad::plain(0, true),
                    &mut planes,
                    &mut tex,
                    &mut facts,
                    &mut Vec::new(),
                    &ahash::AHashMap::new(),
                    &mut |_| {},
                ) {
                    b.add_bevels(&mut planes);
                    if origin != DVec3::ZERO {
                        b.apply_origin(&mut planes, origin);
                    }
                    ids.push(built.len());
                    built.push(b);
                }
            }
            blockers.extend(detail_blockers(&mut planes, &built, &ids));
        }
    }
    let dprp = crate::detail_props::bsp_lump(&entities, &bsp, materials, model_files, &blockers, log);
    games
        .set(crate::detail_props::DPRP, crate::detail_props::DPRP_VERSION, dprp)
        .map_err(|e| CompileError::Failed(e.to_string()))?;
    let mut edit = bsp.edit();
    edit.set_game_lumps(games);
    log(&format!("{} static props", props.len()));
    edit.to_vec().map_err(|e| CompileError::Failed(e.to_string()))
}

fn winding_faces_away(points: &[DVec3], normal: DVec3) -> bool {
    let p: Vec<glam::Vec3> = points.iter().map(|q| q.as_vec3()).collect();
    let (v1, v2) = (p[1] - p[0], p[2] - p[0]);
    let (n, _) = crate::planes::normalize_f32(v2.cross(v1));
    let n = n.as_dvec3();
    let pn = normal.as_vec3().as_dvec3();
    (pn.x * n.x + pn.y * n.y + pn.z * n.z) < 0.99
}

fn prt_float(v: f64) -> String {
    let r = (v + 0.5).floor();
    if (v - r).abs() < 0.001 { format!("{} ", r as i64) } else { format!("{v:.6} ") }
}

fn prt_text(clusters: i32, portals: &[ClusterPortal]) -> String {
    use std::fmt::Write as _;
    let mut s = String::new();
    let _ = writeln!(s, "PRT1");
    let _ = writeln!(s, "{}", clusters.max(0));
    let _ = writeln!(s, "{}", portals.len());
    for p in portals {
        let _ = write!(s, "{} {} {} ", p.points.len(), p.clusters[0], p.clusters[1]);
        for q in &p.points {
            let _ = write!(s, "({}{}{}) ", prt_float(q.x), prt_float(q.y), prt_float(q.z));
        }
        let _ = writeln!(s);
    }
    s
}

fn overlay_lumps(
    entities: &[&hp_vmf::Entity],
    side_faces: &ahash::AHashMap<u32, Vec<u32>>,
    tex: &mut TexInfos,
    materials: &dyn MaterialSource,
    max_overlays: usize,
    log: &mut dyn FnMut(&str),
) -> Result<(Vec<u8>, Vec<u8>), CompileError> {
    let mut out = crate::emit::Buf::default();
    let mut fades = crate::emit::Buf::default();
    for (id, e) in (0i32..).zip(entities.iter().filter(|e| e.classname.eq_ignore_ascii_case("info_overlay"))) {
        if id as usize >= max_overlays {
            return Err(CompileError::Failed(format!(
                "Too Many Overlays!
MAX_MAP_OVERLAYS = {max_overlays}"
            )));
        }
        let num = |k: &str| key(e, k).and_then(|v| v.parse::<f64>().ok()).filter(|v| v.is_finite()).unwrap_or(0.0);
        let vector = |k: &str| key(e, k).and_then(parse_vec).unwrap_or(DVec3::ZERO);
        let material = key(e, "material").unwrap_or("").to_owned();
        let mut faces: Vec<u32> = Vec::new();
        for s in key(e, "sides").unwrap_or("").split_whitespace() {
            if let Ok(sid) = s.parse::<u32>()
                && let Some(list) = side_faces.get(&sid)
            {
                faces.extend(list.iter().copied());
            }
        }
        faces.sort_unstable();
        faces.dedup();
        if faces.is_empty() && !key(e, "sides").unwrap_or("").trim().is_empty() {
            let o = vector("origin");
            log(&format!(
                "Warning: overlay on no face (info_overlay at {:.0} {:.0} {:.0}): its sides are gone",
                o.x, o.y, o.z
            ));
        }
        if faces.len() >= OVERLAY_BSP_FACE_COUNT {
            let o = vector("origin");
            return Err(CompileError::Failed(format!(
                "Overlay touching too many faces (touching {}, max {OVERLAY_BSP_FACE_COUNT})
Overlay {material} at {:.1} {:.1} {:.1}",
                faces.len(),
                o.x,
                o.y,
                o.z
            )));
        }
        let facts = materials.facts(&material);
        let texdata = tex.texdata(&material, &facts);
        let none = [0.0, 0.0, 0.0, -99999.0];
        let texinfo = tex.add(crate::texinfo::TexInfo { texture: [none; 2], lightmap: [none; 2], flags: 0, texdata });
        let order = (num("RenderOrder").clamp(0.0, 3.0) as u16) << 14;
        out.i32(id);
        out.i16(texinfo as i16);
        out.u16(faces.len() as u16 | order);
        for k in 0..64 {
            out.i32(faces.get(k).map_or(0, |f| *f as i32));
        }
        out.f32(num("StartU") as f32);
        out.f32(num("EndU") as f32);
        out.f32(num("StartV") as f32);
        out.f32(num("EndV") as f32);
        let (bu, bv, bn) = (vector("BasisU"), vector("BasisV"), vector("BasisNormal"));
        let flip = if bn.as_vec3().cross(bu.as_vec3()).dot(bv.as_vec3()) < 0.0 { 1.0 } else { 0.0 };
        let zs = [bu.x, bu.y, bu.z, flip];
        for (k, z) in zs.iter().enumerate() {
            let uv = vector(&format!("uv{k}"));
            out.f32(uv.x as f32);
            out.f32(uv.y as f32);
            out.f32(*z as f32);
        }
        out.vec3(vector("BasisOrigin"));
        out.vec3(bn);
        let squared = |v: f64| {
            let v = v as f32;
            if v > 0.0 { v * v } else { v }
        };
        fades.f32(squared(num("fademindist")));
        fades.f32(squared(num("fademaxdist")));
    }
    Ok((out.0, fades.0))
}

#[derive(Clone, Copy, Debug)]
struct BrushLoad {
    base: u32,
    strip_detail: bool,
    full_detail: bool,
    no_detail: bool,
    no_water: bool,
    luxel: [f64; 3],
}

impl BrushLoad {
    fn plain(base: u32, strip_detail: bool) -> BrushLoad {
        BrushLoad { base, strip_detail, full_detail: false, no_detail: false, no_water: false, luxel: [16.0, 1.0, 1.0] }
    }

    fn of(opts: &CompileOptions) -> BrushLoad {
        BrushLoad {
            full_detail: opts.full_detail,
            no_detail: opts.no_detail,
            no_water: opts.no_water,
            luxel: [opts.default_luxel_size, opts.luxel_scale, opts.min_luxel_scale],
            ..BrushLoad::plain(0, false)
        }
    }

    fn luxel_size(&self, lightmap_scale: i32) -> f64 {
        let [default, scale, min] = self.luxel;
        let mut v = f64::from(lightmap_scale);
        if v == 0.0 {
            v = default;
        }
        v *= scale;
        if v < min {
            v = min;
        }
        v
    }
}

#[allow(clippy::too_many_arguments)]
fn build_brush(
    solid: &hp_vmf::Solid,
    entity: usize,
    offset: DVec3,
    load: &BrushLoad,
    planes: &mut PlaneSet,
    tex: &mut TexInfos,
    facts_of: &mut dyn FnMut(&str) -> crate::materials::MaterialFacts,
    side_texes: &mut Vec<SideTex>,
    overlay_ids: &ahash::AHashMap<u32, Vec<u32>>,
    log: &mut dyn FnMut(&str),
) -> Result<Option<MapBrush>, String> {
    let (base, strip_detail) = (load.base, load.strip_detail);
    let id = solid.id;
    let mut sides: Vec<BrushSide> = Vec::with_capacity(solid.sides.len());
    let mut mappings: Vec<SideTex> = Vec::with_capacity(solid.sides.len());
    let mut side_contents = Vec::new();
    let mut surfaceprop = String::new();
    for (k, s) in solid.sides.iter().enumerate() {
        let [a, b, c] = s.plane;
        let Some(p) = planes.from_points(a - offset, b - offset, c - offset) else {
            continue;
        };
        if sides.iter().any(|x| x.plane == p) {
            log(&format!("Brush {id}, Side {k}: duplicate plane"));
            continue;
        }
        if sides.iter().any(|x| x.plane == (p ^ 1)) {
            return Err(format!("Brush {id}: mirrored plane"));
        }
        let facts = facts_of(&s.material);
        if surfaceprop.is_empty() {
            surfaceprop = facts.surfaceprop.clone();
        }
        let mapping = SideMapping {
            u_axis: s.uaxis.axis,
            u_shift: s.uaxis.shift,
            u_scale: s.uaxis.scale,
            v_axis: s.vaxis.axis,
            v_shift: s.vaxis.shift,
            v_scale: s.vaxis.scale,
            lightmap_scale: load.luxel_size(s.lightmap_scale),
        };
        let mut side_c = crate::materials::side_contents(facts.raw_contents | base, facts.surf);
        if load.full_detail {
            side_c &= !contents::DETAIL;
        }
        side_contents.push(side_c);
        let texdata = tex.texdata(&s.material, &facts);
        let texinfo = tex.for_side(&mapping, DVec3::ZERO, facts.surf, texdata);
        sides.push(BrushSide {
            plane: p,
            texinfo,
            surf: facts.surf,
            winding: None,
            bevel: false,
            visible: false,
            no_split: false,
            side_id: s.id,
            disp: None,
            contents: if strip_detail { side_c & !contents::DETAIL } else { side_c },
            fwinding: None,
            smoothing: s.smoothing_groups,
            overlays: overlay_ids.get(&{ s.id }).cloned().unwrap_or_default(),
        });
        mappings.push(SideTex {
            slot: SideSlot::Gone,
            mapping,
            material: s.material.clone(),
            facts,
            side_id: s.id,
            winding: None,
            normal: DVec3::ZERO,
            offset,
            texinfo,
            disp: s.disp.is_some(),
        });
    }
    let (c, _) = MapBrush::contents_from_sides(&side_contents);
    let disp = solid.sides.iter().any(|s| s.disp.is_some());
    if (load.no_detail && c & contents::DETAIL != 0 && !disp) || (load.no_water && c & contents::MASK_WATER != 0) {
        return Ok(None);
    }
    let mut b = MapBrush { id, entity, contents: c, sides, mins: DVec3::ZERO, maxs: DVec3::ZERO, surfaceprop };
    if !b.make_windings(planes) {
        return Err(format!("Brush {id}: bad geometry, removed"));
    }
    for (i, &bit) in [b.mins, b.maxs].iter().enumerate() {
        if bit.abs().max_element() > hp_limit() {
            return Err(format!("Brush {id}: bounds out of range ({})", if i == 0 { "min" } else { "max" }));
        }
    }
    for (side, mut t) in b.sides.iter().zip(mappings) {
        t.winding = side_float_winding(side);
        t.normal = planes.getf(side.plane).0.as_dvec3();
        side_texes.push(t);
    }
    Ok(Some(b))
}

fn side_float_winding(side: &BrushSide) -> Option<crate::winding::Winding> {
    side.fwinding
        .as_ref()
        .map(|w| crate::winding::Winding::new(w.iter().map(|p| p.as_dvec3()).collect()))
        .or_else(|| side.winding.clone())
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
enum SideSlot {
    Brush(usize),
    Disp(usize),
    Gone,
}

struct SideTex {
    slot: SideSlot,
    mapping: SideMapping,
    material: String,
    facts: crate::materials::MaterialFacts,
    side_id: u32,
    winding: Option<crate::winding::Winding>,
    normal: DVec3,
    offset: DVec3,
    texinfo: i32,
    disp: bool,
}

fn set_side_texinfo(t: &SideTex, brushes: &mut [MapBrush], disps: &mut [crate::disp::DispFace]) {
    match t.slot {
        SideSlot::Brush(b) => {
            if let Some(side) =
                brushes.get_mut(b).and_then(|b| b.sides.iter_mut().find(|s| !s.bevel && s.side_id == t.side_id))
            {
                side.texinfo = t.texinfo;
            }
        }
        SideSlot::Disp(d) => {
            if let Some(d) = disps.get_mut(d) {
                d.texinfo = t.texinfo;
            }
        }
        SideSlot::Gone => {}
    }
}

#[derive(Clone, Copy, Debug)]
enum SideVisit {
    Side(usize),
    Bevel(usize, usize),
}

fn side_visits(sides: &[SideTex], brushes: &[MapBrush]) -> Vec<SideVisit> {
    let mut out = Vec::with_capacity(sides.len());
    let mut i = 0;
    while i < sides.len() {
        let SideSlot::Brush(b) = sides[i].slot else {
            out.push(SideVisit::Side(i));
            i += 1;
            continue;
        };
        let end = (i..sides.len()).find(|&j| sides[j].slot != SideSlot::Brush(b)).unwrap_or(sides.len());
        let mut used = vec![false; end - i];
        for (k, side) in brushes[b].sides.iter().enumerate() {
            if side.bevel {
                if side.texinfo >= 0 {
                    out.push(SideVisit::Bevel(b, k));
                }
            } else if let Some(p) = (i..end).find(|&p| !used[p - i] && sides[p].side_id == side.side_id) {
                used[p - i] = true;
                out.push(SideVisit::Side(p));
            }
        }
        out.extend((i..end).filter(|&p| !used[p - i]).map(SideVisit::Side));
        i = end;
    }
    out
}

fn texture_passes(
    sides: &mut [SideTex],
    tex: &mut TexInfos,
    cubemaps: &mut crate::cubemap::Cubemaps<'_>,
    brushes: &mut [MapBrush],
    disps: &mut [crate::disp::DispFace],
    facts_of: &mut dyn FnMut(&str) -> crate::materials::MaterialFacts,
) {
    let name_of = |tex: &TexInfos, ti: i32| -> String {
        usize::try_from(ti)
            .ok()
            .and_then(|t| tex.infos.get(t))
            .and_then(|i| usize::try_from(i.texdata).ok())
            .and_then(|d| tex.data.get(d))
            .map_or(String::new(), |d| d.name.clone())
    };
    let retexture = |tex: &mut TexInfos, ti: i32, texdata: i32| -> i32 {
        match usize::try_from(ti).ok().and_then(|t| tex.infos.get(t)).copied() {
            Some(info) => tex.add(crate::texinfo::TexInfo { texdata, ..info }),
            None => ti,
        }
    };
    let map = cubemaps.map_name().to_owned();
    let visits = side_visits(sides, brushes);
    if !map.is_empty() {
        let wvt = |ti: i32, tex: &mut TexInfos, cubemaps: &mut crate::cubemap::Cubemaps<'_>| -> i32 {
            let name = name_of(tex, ti);
            if name.to_ascii_lowercase().contains("_wvt_patch") {
                return ti;
            }
            let patched =
                format!("maps/{map}/{}_wvt_patch", crate::materials::material_path(&name)).to_ascii_lowercase();
            if !tex.has_texdata(&patched) {
                cubemaps.wvt_patch(&name, &patched);
            }
            let td = tex.clone_texdata(ti, &patched);
            retexture(tex, ti, td)
        };
        for &v in &visits {
            match v {
                SideVisit::Side(i) => {
                    let t = &mut sides[i];
                    if !t.disp && t.texinfo >= 0 && t.facts.shader.contains("worldvertextransition") {
                        t.texinfo = wvt(t.texinfo, tex, cubemaps);
                        set_side_texinfo(t, brushes, disps);
                    }
                }
                SideVisit::Bevel(b, k) => {
                    let ti = brushes[b].sides[k].texinfo;
                    if facts_of(&name_of(tex, ti)).shader.contains("worldvertextransition") {
                        brushes[b].sides[k].texinfo = wvt(ti, tex, cubemaps);
                    }
                }
            }
        }
    }
    let mut cubemap_data: Vec<i32> = Vec::new();
    let mut patch = |t: &mut SideTex, patched: Option<String>, tex: &mut TexInfos, cubemap_data: &mut Vec<i32>| {
        let Some(patched) = patched else { return };
        let td = tex.clone_texdata(t.texinfo, &patched);
        if !cubemap_data.contains(&td) {
            cubemap_data.push(td);
        }
        t.texinfo = retexture(tex, t.texinfo, td);
        set_side_texinfo(t, brushes, disps);
    };
    let is_cubemap = |tex: &TexInfos, ti: i32, list: &[i32]| {
        usize::try_from(ti).ok().and_then(|t| tex.infos.get(t)).is_some_and(|i| list.contains(&i.texdata))
    };
    for (sample, side_id) in cubemaps.manual_sides() {
        let Some(t) = sides.iter_mut().find(|t| t.side_id == side_id) else { continue };
        let name = name_of(tex, t.texinfo);
        if is_cubemap(tex, t.texinfo, &cubemap_data) {
            cubemaps.messages.push(format!("Multiple references for cubemap on texture {name}!!!"));
            continue;
        }
        let patched = cubemaps.manual_material(&name, side_id, sample);
        patch(t, patched, tex, &mut cubemap_data);
    }
    let keep = |td: i32, cubemap_data: &mut Vec<i32>| {
        if !cubemap_data.contains(&td) {
            cubemap_data.push(td);
        }
    };
    for &v in &visits {
        match v {
            SideVisit::Side(i) => {
                let t = &mut sides[i];
                if t.texinfo >= 0 && !cubemaps.is_manual(t.side_id) && !is_cubemap(tex, t.texinfo, &cubemap_data) {
                    let name = name_of(tex, t.texinfo);
                    if let Some(p) =
                        cubemaps.side_material(&name, &t.facts, t.side_id, t.winding.as_ref(), t.offset, t.normal)
                    {
                        let td = tex.clone_texdata(t.texinfo, &p);
                        keep(td, &mut cubemap_data);
                        t.texinfo = retexture(tex, t.texinfo, td);
                        set_side_texinfo(t, brushes, disps);
                    }
                }
            }
            SideVisit::Bevel(b, k) => {
                let ti = brushes[b].sides[k].texinfo;
                if is_cubemap(tex, ti, &cubemap_data) {
                    continue;
                }
                let name = name_of(tex, ti);
                let facts = facts_of(&name);
                if let Some(p) = cubemaps.side_material(&name, &facts, u32::MAX, None, DVec3::ZERO, DVec3::ZERO) {
                    let td = tex.clone_texdata(ti, &p);
                    keep(td, &mut cubemap_data);
                    brushes[b].sides[k].texinfo = retexture(tex, ti, td);
                }
            }
        }
    }
    for t in sides.iter() {
        if t.facts.contents & contents::WATER == 0 || t.texinfo < 0 {
            continue;
        }
        let Some(bottom) = t.facts.bottom_material.clone() else { continue };
        let name = name_of(tex, t.texinfo);
        let bottom_name = if name.eq_ignore_ascii_case(&t.material) {
            bottom.clone()
        } else {
            cubemaps.dependent_name(&name, &bottom)
        };
        let bottom_facts = facts_of(&bottom);
        let td = tex.texdata(&bottom_name, &bottom_facts);
        tex.set_bottom(t.texinfo, td);
    }
}

fn brush_volume(b: &MapBrush) -> f64 {
    let Some(corner) =
        b.sides.iter().filter(|s| !s.bevel).find_map(|s| s.winding.as_ref().and_then(|w| w.points.first().copied()))
    else {
        return 0.0;
    };
    let mut volume = 0.0;
    for s in b.sides.iter().filter(|s| !s.bevel) {
        let Some(w) = &s.winding else { continue };
        if w.points.len() < 3 {
            continue;
        }
        let mut area = DVec3::ZERO;
        for i in 1..w.points.len() - 1 {
            area += (w.points[i] - w.points[0]).cross(w.points[i + 1] - w.points[0]);
        }
        let n = area.normalize_or_zero();
        let d = (w.points[0] - corner).dot(n);
        volume += d * area.length() * 0.5;
    }
    volume.abs() / 3.0
}

fn hp_limit() -> f64 {
    16384.0
}
