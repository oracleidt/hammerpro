use glam::DVec3;

use crate::brush::MapBrush;
use crate::emit::lump;
use crate::materials::{contents, surf};
use hp_bsp::limits as lim;

pub const GEOMETRY_HEADER: &str = "---- Map statistics ----";
pub const DETAILS_HEADER: &str = "---- Geometry details ----";
pub const VIS_HEADER: &str = "---- Visibility statistics ----";
pub const LIGHTING_HEADER: &str = "---- Lighting statistics ----";
pub const TABLE_HEADER: &str = "Object names            Objects/Maxobjs       Memory / Maxmem   Fullness";

const TOP: usize = 15;
const WARN_PERCENT: f64 = 90.0;
pub const MAX_LIGHTING: u64 = hp_bsp::limits::LIGHTING.sdk;
const WORLDLIGHT_SIZE: usize = 88;

#[derive(Clone, Debug, PartialEq)]
pub struct LimitRow {
    pub name: &'static str,
    pub valve: &'static str,
    pub used: Option<u64>,
    pub max: u64,
    pub bytes: u64,
    pub max_bytes: u64,
}

impl LimitRow {
    pub fn percent(&self) -> f64 {
        match self.used {
            Some(u) if self.max > 0 => u as f64 * 100.0 / self.max as f64,
            _ if self.max_bytes > 0 => self.bytes as f64 * 100.0 / self.max_bytes as f64,
            _ => 0.0,
        }
    }

    pub fn line(&self) -> String {
        let objects = match self.used {
            Some(u) => format!("{u}/{}", self.max),
            None => "[variable]".to_owned(),
        };
        format!(
            "{:<22} {:>17} {:>21}  ({:5.1}%)",
            self.name,
            objects,
            format!("{}/{}", self.bytes, self.max_bytes),
            self.percent()
        )
    }
}

fn u16_at(d: &[u8], o: usize) -> u16 {
    d.get(o..o + 2).map_or(0, |b| u16::from_le_bytes([b[0], b[1]]))
}
fn i16_at(d: &[u8], o: usize) -> i16 {
    u16_at(d, o) as i16
}
fn i32_at(d: &[u8], o: usize) -> i32 {
    d.get(o..o + 4).map_or(0, |b| i32::from_le_bytes([b[0], b[1], b[2], b[3]]))
}
fn f32_at(d: &[u8], o: usize) -> f32 {
    f32::from_bits(i32_at(d, o) as u32)
}

#[derive(Clone, Copy, Debug, Default)]
struct Leaf {
    contents: i32,
    cluster: i16,
    area: u16,
    mins: [i16; 3],
    maxs: [i16; 3],
    first_face: usize,
    faces: usize,
}

impl Leaf {
    fn center(&self) -> DVec3 {
        DVec3::new(
            (f64::from(self.mins[0]) + f64::from(self.maxs[0])) * 0.5,
            (f64::from(self.mins[1]) + f64::from(self.maxs[1])) * 0.5,
            (f64::from(self.mins[2]) + f64::from(self.maxs[2])) * 0.5,
        )
    }

    fn volume(&self) -> f64 {
        (0..3).map(|k| (f64::from(self.maxs[k]) - f64::from(self.mins[k])).max(0.0)).product()
    }
}

#[derive(Clone, Debug, Default)]
struct Face {
    texinfo: i32,
    disp: i16,
    edges: usize,
    first_edge: usize,
    styles: usize,
    light_offset: i32,
    luxels: [i32; 2],
}

struct View<'a> {
    bsp: &'a hp_bsp::Bsp,
    faces: Vec<Face>,
    texinfo: Vec<(i32, i32, f64)>,
    materials: Vec<String>,
    verts: Vec<DVec3>,
    edges: Vec<[u16; 2]>,
    surfedges: Vec<i32>,
    leaves: Vec<Leaf>,
    leaf_faces: Vec<u16>,
    face_ids: Vec<u16>,
    limits: hp_bsp::limits::LimitSet,
}

impl<'a> View<'a> {
    fn read(bsp: &'a hp_bsp::Bsp) -> View<'a> {
        let raw = |i: usize| bsp.lump(i).map(std::borrow::Cow::into_owned).unwrap_or_default();
        let faces = raw(lump::FACES)
            .chunks_exact(56)
            .map(|f| Face {
                first_edge: i32_at(f, 4).max(0) as usize,
                edges: i16_at(f, 8).max(0) as usize,
                texinfo: i32::from(i16_at(f, 10)),
                disp: i16_at(f, 12),
                styles: f[16..20].iter().filter(|&&s| s != 255).count(),
                light_offset: i32_at(f, 20),
                luxels: [i32_at(f, 36), i32_at(f, 40)],
            })
            .collect();
        let texinfo = raw(lump::TEXINFO)
            .chunks_exact(72)
            .map(|t| {
                let v = DVec3::new(f64::from(f32_at(t, 32)), f64::from(f32_at(t, 36)), f64::from(f32_at(t, 40)));
                (i32_at(t, 64), i32_at(t, 68), v.length())
            })
            .collect();
        let names = bsp.texture_names().unwrap_or_default();
        let materials = raw(lump::TEXDATA)
            .chunks_exact(32)
            .map(|d| {
                let name = usize::try_from(i32_at(d, 12)).ok().and_then(|i| names.get(i)).cloned().unwrap_or_default();
                crate::detail_props::original_material(&name).to_ascii_lowercase()
            })
            .collect();
        let verts = raw(lump::VERTEXES)
            .chunks_exact(12)
            .map(|v| DVec3::new(f64::from(f32_at(v, 0)), f64::from(f32_at(v, 4)), f64::from(f32_at(v, 8))))
            .collect();
        let edges = raw(lump::EDGES).chunks_exact(4).map(|e| [u16_at(e, 0), u16_at(e, 2)]).collect();
        let surfedges = raw(lump::SURFEDGES).chunks_exact(4).map(|e| i32_at(e, 0)).collect();
        let leaf_size = if bsp.lump_info(lump::LEAFS).map_or(1, |l| l.version) == 0 { 56 } else { 32 };
        let leaves = raw(lump::LEAFS)
            .chunks_exact(leaf_size)
            .map(|l| Leaf {
                contents: i32_at(l, 0),
                cluster: i16_at(l, 4),
                area: u16_at(l, 6) & 0x1ff,
                mins: [i16_at(l, 8), i16_at(l, 10), i16_at(l, 12)],
                maxs: [i16_at(l, 14), i16_at(l, 16), i16_at(l, 18)],
                first_face: usize::from(u16_at(l, 20)),
                faces: usize::from(u16_at(l, 22)),
            })
            .collect();
        let leaf_faces = raw(lump::LEAFFACES).chunks_exact(2).map(|b| u16_at(b, 0)).collect();
        let face_ids = raw(lump::FACEIDS).chunks_exact(2).map(|b| u16_at(b, 0)).collect();
        View {
            bsp,
            faces,
            texinfo,
            materials,
            verts,
            edges,
            surfedges,
            leaves,
            leaf_faces,
            face_ids,
            limits: hp_bsp::limits::LimitSet::Gmod,
        }
    }

    fn len(&self, i: usize) -> u64 {
        self.bsp.lump_info(i).map_or(0, |l| u64::from(l.uncompressed_len().unwrap_or(l.len)))
    }

    fn material(&self, f: &Face) -> &str {
        let t = usize::try_from(f.texinfo).ok().and_then(|t| self.texinfo.get(t));
        t.and_then(|t| usize::try_from(t.1).ok()).and_then(|d| self.materials.get(d)).map_or("", String::as_str)
    }

    fn points(&self, f: &Face) -> Vec<DVec3> {
        (f.first_edge..f.first_edge + f.edges)
            .filter_map(|e| {
                let se = *self.surfedges.get(e)?;
                let edge = self.edges.get(se.unsigned_abs() as usize)?;
                self.verts.get(usize::from(if se >= 0 { edge[0] } else { edge[1] })).copied()
            })
            .collect()
    }

    fn lightmap(&self, f: &Face) -> Option<(u64, u64)> {
        let (flags, _, _) = *usize::try_from(f.texinfo).ok().and_then(|t| self.texinfo.get(t))?;
        if flags as u32 & (surf::NOLIGHT | surf::SKY | surf::SKY2D) != 0 {
            return None;
        }
        let luxels = u64::try_from((f.luxels[0] + 1).max(0)).ok()? * u64::try_from((f.luxels[1] + 1).max(0)).ok()?;
        let bump = if flags as u32 & surf::BUMPLIGHT != 0 { 4 } else { 1 };
        let styles = if f.light_offset < 0 { 1 } else { f.styles.max(1) as u64 };
        Some((luxels, (luxels * bump + 1) * styles * 4))
    }

    fn game_lump(&self, id: &[u8; 4]) -> Option<Vec<u8>> {
        let games = self.bsp.game_lumps().ok()?;
        let g = games.get(u32::from_be_bytes(*id))?;
        g.data().ok().map(std::borrow::Cow::into_owned)
    }

    fn static_props(&self) -> Option<(u64, u64)> {
        let d = self.game_lump(b"sprp")?;
        let dict = usize::try_from(i32_at(&d, 0)).ok()?;
        let at = 4 + dict * 128;
        let leaves = usize::try_from(i32_at(&d, at)).ok()?;
        let at = at + 4 + leaves * 2;
        Some((u64::try_from(i32_at(&d, at)).ok()?, d.len() as u64))
    }

    fn detail_props(&self) -> Option<(u64, u64)> {
        let d = self.game_lump(b"dprp")?;
        let dict = usize::try_from(i32_at(&d, 0)).ok()?;
        let at = 4 + dict * 128;
        let sprites = usize::try_from(i32_at(&d, at)).ok()?;
        let at = at + 4 + sprites * crate::detail_props::SPRITE_SIZE;
        Some((u64::try_from(i32_at(&d, at)).ok()?, d.len() as u64))
    }

    fn geometry_rows(&self) -> Vec<LimitRow> {
        let row = |name, valve, l: usize, record: u64, max: u64| {
            let bytes = self.len(l);
            LimitRow { name, valve, used: Some(bytes / record), max, bytes, max_bytes: max * record }
        };
        let leaf_size = if self.bsp.lump_info(lump::LEAFS).map_or(1, |l| l.version) == 0 { 56 } else { 32 };
        let mut rows = vec![
            row("models", "MAX_MAP_MODELS", lump::MODELS, 48, lim::MODELS.max_in(self.limits)),
            row("brushes", "MAX_MAP_BRUSHES", lump::BRUSHES, 12, lim::BRUSHES.max_in(self.limits)),
            row("brushsides", "MAX_MAP_BRUSHSIDES", lump::BRUSHSIDES, 8, lim::BRUSHSIDES.max_in(self.limits)),
            row("planes", "MAX_MAP_PLANES", lump::PLANES, 20, lim::PLANES.max_in(self.limits)),
            row("vertexes", "MAX_MAP_VERTS", lump::VERTEXES, 12, lim::VERTEXES.max_in(self.limits)),
            row("nodes", "MAX_MAP_NODES", lump::NODES, 32, lim::NODES.max_in(self.limits)),
            row("texinfo", "MAX_MAP_TEXINFO", lump::TEXINFO, 72, lim::TEXINFO.max_in(self.limits)),
            row("texdata", "MAX_MAP_TEXDATA", lump::TEXDATA, 32, lim::TEXDATA.max_in(self.limits)),
            row("dispinfo", "MAX_MAP_DISPINFO", lump::DISPINFO, 176, lim::DISPINFO.max_in(self.limits)),
            row("disp_verts", "MAX_MAP_DISP_VERTS", lump::DISP_VERTS, 20, lim::DISP_VERTS.max_in(self.limits)),
            row("disp_tris", "MAX_MAP_DISP_TRIS", lump::DISP_TRIS, 2, lim::DISP_TRIS.max_in(self.limits)),
            row("faces", "MAX_MAP_FACES", lump::FACES, 56, lim::FACES.max_in(self.limits)),
            row("origfaces", "MAX_MAP_ORIGFACES", lump::ORIGINALFACES, 56, lim::ORIGFACES.max_in(self.limits)),
            row("leaves", "MAX_MAP_LEAFS", lump::LEAFS, leaf_size, lim::LEAFS.max_in(self.limits)),
            row("leaffaces", "MAX_MAP_LEAFFACES", lump::LEAFFACES, 2, lim::LEAFFACES.max_in(self.limits)),
            row("leafbrushes", "MAX_MAP_LEAFBRUSHES", lump::LEAFBRUSHES, 2, lim::LEAFBRUSHES.max_in(self.limits)),
            row("areas", "MAX_MAP_AREAS", lump::AREAS, 8, lim::AREAS.max_in(self.limits)),
            row("areaportals", "MAX_MAP_AREAPORTALS", lump::AREAPORTALS, 12, lim::AREAPORTALS.max_in(self.limits)),
            row("surfedges", "MAX_MAP_SURFEDGES", lump::SURFEDGES, 4, lim::SURFEDGES.max_in(self.limits)),
            row("edges", "MAX_MAP_EDGES", lump::EDGES, 4, lim::EDGES.max_in(self.limits)),
            row(
                "leafwaterdata",
                "MAX_MAP_LEAFWATERDATA",
                lump::LEAFWATERDATA,
                12,
                lim::LEAFWATERDATA.max_in(self.limits),
            ),
            row(
                "cubemapsamples",
                "MAX_MAP_CUBEMAPSAMPLES",
                lump::CUBEMAPS,
                16,
                lim::CUBEMAPSAMPLES.max_in(self.limits),
            ),
            row("overlays", "MAX_MAP_OVERLAYS", lump::OVERLAYS, 352, lim::OVERLAYS.max_in(self.limits)),
        ];
        let occluders = self.bsp.lump(lump::OCCLUSION).map_or(0, |d| i32_at(&d, 0).max(0) as u64);
        rows.push(LimitRow {
            name: "occluders",
            valve: "MAX_MAP_OCCLUDERS",
            used: Some(occluders),
            max: 0,
            bytes: self.len(lump::OCCLUSION),
            max_bytes: 0,
        });
        let entities = self.bsp.entities().map_or(0, |e| e.len() as u64);
        rows.push(LimitRow {
            name: "entities",
            valve: "MAX_MAP_ENTITIES",
            used: Some(entities),
            max: lim::ENTITIES.max_in(self.limits),
            bytes: self.len(lump::ENTITIES),
            max_bytes: 0,
        });
        let variable = |name, valve, l: usize, max_bytes: u64| LimitRow {
            name,
            valve,
            used: None,
            max: 0,
            bytes: self.len(l),
            max_bytes,
        };
        rows.push(variable("visdata", "MAX_MAP_VISIBILITY", lump::VISIBILITY, lim::VISIBILITY.max_in(self.limits)));
        rows.push(variable("entdata", "MAX_MAP_ENTSTRING", lump::ENTITIES, lim::ENTSTRING.max_in(self.limits)));
        let (props, prop_bytes) = self.static_props().unwrap_or((0, 0));
        rows.push(LimitRow {
            name: "static props",
            valve: "MAX_STATIC_PROPS",
            used: Some(props),
            max: lim::STATIC_PROPS.max_in(self.limits),
            bytes: prop_bytes,
            max_bytes: 0,
        });
        let (details, detail_bytes) = self.detail_props().unwrap_or((0, 0));
        rows.push(LimitRow {
            name: "detail props",
            valve: "MAX_DETAIL_PROPS",
            used: Some(details),
            max: 0,
            bytes: detail_bytes,
            max_bytes: 0,
        });
        let files = self.bsp.pakfile().map_or(0, |p| p.entries().len() as u64);
        rows.push(LimitRow {
            name: "pakfile",
            valve: "PAKFILE",
            used: Some(files),
            max: 0,
            bytes: self.len(lump::PAKFILE),
            max_bytes: 0,
        });
        rows.push(variable("physics", "MAX_MAP_PHYSCOLLIDE", lump::PHYSCOLLIDE, lim::PHYSCOLLIDE.max_in(self.limits)));
        rows.push(variable("physics terrain", "MAX_MAP_PHYSDISP", lump::PHYSDISP, lim::PHYSDISP.max_in(self.limits)));
        rows
    }

    fn lighting_rows(&self) -> Vec<LimitRow> {
        let size = |l: usize| hp_bsp::format::worldlight_size(self.bsp.lump_info(l).map_or(0, |i| i.version)) as u64;
        let lights = |name, l: usize| LimitRow {
            name,
            valve: "MAX_MAP_WORLDLIGHTS",
            used: Some(self.len(l) / size(l)),
            max: lim::WORLDLIGHTS.max_in(self.limits),
            bytes: self.len(l),
            max_bytes: lim::WORLDLIGHTS.max_in(self.limits) * size(l),
        };
        let data = |name, l: usize| LimitRow {
            name,
            valve: "dface_t lightofs",
            used: None,
            max: 0,
            bytes: self.len(l),
            max_bytes: lim::LIGHTING.max_in(self.limits),
        };
        let ambient = |name, l: usize| LimitRow {
            name,
            valve: "MAX_MAP_LEAFS",
            used: Some(self.len(l) / 28),
            max: lim::LEAFS.max_in(self.limits),
            bytes: self.len(l),
            max_bytes: lim::LEAFS.max_in(self.limits) * 28,
        };
        vec![
            lights("LDR worldlights", 15),
            lights("HDR worldlights", 54),
            data("LDR lightdata", lump::LIGHTING),
            data("HDR lightdata", lump::LIGHTING_HDR),
            ambient("LDR leaf ambient", lump::LEAF_AMBIENT_LIGHTING),
            ambient("HDR leaf ambient", lump::LEAF_AMBIENT_LIGHTING_HDR),
        ]
    }
}

fn table_lines(rows: &[LimitRow]) -> Vec<String> {
    let mut out = vec![TABLE_HEADER.to_owned()];
    out.extend(rows.iter().map(LimitRow::line));
    for r in rows {
        let p = r.percent();
        let max = if r.used.is_some() && r.max > 0 { r.max } else { r.max_bytes };
        if p > 100.0 {
            out.push(format!("{}: {:.1}% of the engine limit, over it ({} = {max})", r.name, p, r.valve));
        } else if p >= WARN_PERCENT {
            out.push(format!("Warning: {} uses {:.1}% of the engine limit ({} = {max})", r.name, p, r.valve));
        }
    }
    out
}

fn at(p: DVec3) -> String {
    format!("at ({:.0} {:.0} {:.0})", p.x, p.y, p.z)
}

fn size(lo: DVec3, hi: DVec3) -> String {
    let d = (hi - lo).max(DVec3::ZERO);
    format!("{:.0} x {:.0} x {:.0} units", d.x, d.y, d.z)
}

fn ranked(counts: ahash::AHashMap<String, (usize, usize)>) -> Vec<(String, usize, usize)> {
    let mut v: Vec<(String, usize, usize)> = counts.into_iter().map(|(k, (a, b))| (k, a, b)).collect();
    v.sort_by(|a, b| b.1.cmp(&a.1).then_with(|| a.0.cmp(&b.0)));
    v
}

fn stage_lines(title: &str, stages: &[(String, f64)]) -> Vec<String> {
    if stages.is_empty() {
        return Vec::new();
    }
    let mut out = vec![format!("Stage times ({title}):")];
    out.extend(stages.iter().enumerate().map(|(k, (name, s))| format!("#{}  {s:.3} s  {name}", k + 1)));
    let total: f64 = stages.iter().map(|(_, s)| s).sum();
    out.push(format!("Total  {total:.3} s"));
    out
}

pub struct GeometryInput<'a> {
    pub map: &'a hp_vmf::Map,
    pub brushes: &'a [MapBrush],
    pub classes: &'a [&'a str],
    pub stages: &'a [(String, f64)],
    pub limits: hp_bsp::limits::LimitSet,
}

const THIN: f64 = 32.0;
const SMALL: f64 = 64.0;

pub fn geometry_lines(bsp: &hp_bsp::Bsp, input: &GeometryInput<'_>) -> Vec<String> {
    let mut v = View::read(bsp);
    v.limits = input.limits;
    let mut out = vec![GEOMETRY_HEADER.to_owned()];
    out.extend(table_lines(&v.geometry_rows()));
    out.push(DETAILS_HEADER.to_owned());

    let (mut structural, mut detail, mut water, mut tool, mut entity) = (0usize, 0usize, 0usize, 0usize, 0usize);
    let mut small: Vec<&MapBrush> = Vec::new();
    for b in input.brushes {
        let class = input.classes.get(b.entity).copied().unwrap_or("");
        if b.entity == 0 || class.eq_ignore_ascii_case("func_detail") {
            if b.contents & contents::DETAIL != 0 {
                detail += 1;
            } else if b.contents & contents::SOLID != 0 {
                structural += 1;
                let mut d = [b.maxs.x - b.mins.x, b.maxs.y - b.mins.y, b.maxs.z - b.mins.z];
                d.sort_by(f64::total_cmp);
                if b.entity == 0 && (d[1] <= THIN || d[2] <= SMALL) {
                    small.push(b);
                }
            } else if b.contents & contents::MASK_WATER != 0 {
                water += 1;
            } else {
                tool += 1;
            }
        } else {
            entity += 1;
        }
    }
    out.push(format!(
        "Brushes: {} total, {structural} structural, {detail} detail, {water} water, {tool} tool, {entity} in brush entities",
        input.brushes.len()
    ));

    let mut classes: ahash::AHashMap<String, (usize, usize)> = ahash::AHashMap::new();
    let mut brush_classes: ahash::AHashMap<String, (usize, usize)> = ahash::AHashMap::new();
    let mut fadeless: ahash::AHashMap<String, (usize, usize)> = ahash::AHashMap::new();
    let mut fadeless_at: ahash::AHashMap<String, DVec3> = ahash::AHashMap::new();
    for e in &input.map.entities {
        let class = e.classname.to_ascii_lowercase();
        classes.entry(class.clone()).or_default().0 += 1;
        if !e.solids.is_empty() {
            let c = brush_classes.entry(class.clone()).or_default();
            c.0 += 1;
            c.1 += e.solids.len();
        }
        if class == "prop_static" {
            let fade = e.get("fademaxdist").and_then(|v| v.trim().parse::<f64>().ok()).unwrap_or(0.0);
            if fade <= 0.0 {
                let model = e.get("model").unwrap_or("").trim().replace('\\', "/").to_ascii_lowercase();
                fadeless.entry(model.clone()).or_default().0 += 1;
                if let Some(o) = e.origin() {
                    fadeless_at.entry(model).or_insert(o);
                }
            }
        }
    }
    let in_lump = bsp.entities().map_or(0, |e| e.len());
    let brush_entities: usize = brush_classes.values().map(|c| c.0).sum();
    out.push(format!(
        "Entities: {} in the map, {in_lump} in the entity lump, {brush_entities} brush entities, {} point entities",
        input.map.entities.len(),
        input.map.entities.len() - brush_entities
    ));

    let models = bsp.lump(lump::MODELS).map(std::borrow::Cow::into_owned).unwrap_or_default();
    let world_faces = usize::try_from(i32_at(&models, 44)).unwrap_or(0).min(v.faces.len());
    let disp = bsp.lump(lump::DISPINFO).map(std::borrow::Cow::into_owned).unwrap_or_default();
    let mut powers = [0usize; 5];
    let mut disp_tris = 0usize;
    for d in disp.chunks_exact(176) {
        let p = usize::try_from(i32_at(d, 20)).unwrap_or(0).min(4);
        powers[p] += 1;
        disp_tris += 2 << (2 * p);
    }
    let disp_faces = v.faces.iter().filter(|f| f.disp >= 0).count();
    let tris: usize =
        v.faces.iter().filter(|f| f.disp < 0).map(|f| f.edges.saturating_sub(2)).sum::<usize>() + disp_tris;
    out.push(format!(
        "Faces: {world_faces} world, {} in brush entities, {disp_faces} displacements, {tris} triangles",
        v.faces.len() - world_faces
    ));
    let disps: usize = powers.iter().sum();
    out.push(format!(
        "Displacements: {disps} (power 2: {}, power 3: {}, power 4: {})",
        powers[2], powers[3], powers[4]
    ));
    if models.len() >= 24 {
        let p = |o: usize| {
            DVec3::new(
                f64::from(f32_at(&models, o)),
                f64::from(f32_at(&models, o + 4)),
                f64::from(f32_at(&models, o + 8)),
            )
        };
        out.push(format!("Map size: {}", size(p(0), p(12))));
    }
    out.push(format!("BSP file: {} bytes", bsp.len()));
    if v.len(lump::LIGHTING) == 0 && v.len(lump::LIGHTING_HDR) == 0 {
        let (luxels, bytes) =
            v.faces.iter().filter_map(|f| v.lightmap(f)).fold((0u64, 0u64), |a, b| (a.0 + b.0, a.1 + b.1));
        out.push(format!(
            "Lightmaps (estimate): {luxels} luxels, {bytes} bytes per pass of {MAX_LIGHTING} ({:.1}%)",
            bytes as f64 * 100.0 / MAX_LIGHTING as f64
        ));
    }

    let mut by_material: ahash::AHashMap<String, (usize, usize)> = ahash::AHashMap::new();
    for f in &v.faces {
        by_material.entry(v.material(f).to_owned()).or_default().0 += 1;
    }
    let materials = ranked(by_material);
    out.push(format!("Materials by faces (top {} of {}):", materials.len().min(TOP), materials.len()));
    for (k, (name, n, _)) in materials.iter().take(TOP).enumerate() {
        out.push(format!("#{}  {n} faces  {:.1}%  {name}", k + 1, *n as f64 * 100.0 / v.faces.len().max(1) as f64));
    }
    let classes = ranked(classes);
    out.push(format!("Entities by class (top {} of {}):", classes.len().min(TOP), classes.len()));
    for (k, (name, n, _)) in classes.iter().take(TOP).enumerate() {
        out.push(format!("#{}  {n}  {name}", k + 1));
    }
    let brush_classes = ranked(brush_classes);
    out.push(format!("Brush entities by class (top {} of {}):", brush_classes.len().min(TOP), brush_classes.len()));
    for (k, (name, n, brushes)) in brush_classes.iter().take(TOP).enumerate() {
        out.push(format!("#{}  {n}  {name}  {brushes} brushes", k + 1));
    }

    struct Area {
        leaves: usize,
        clusters: ahash::AHashSet<i16>,
        faces: ahash::AHashSet<u16>,
        largest: (f64, DVec3),
    }
    let mut areas: std::collections::BTreeMap<u16, Area> = std::collections::BTreeMap::new();
    for l in v.leaves.iter().filter(|l| l.contents & contents::SOLID as i32 == 0 && l.area > 0) {
        let a = areas.entry(l.area).or_insert_with(|| Area {
            leaves: 0,
            clusters: Default::default(),
            faces: Default::default(),
            largest: (-1.0, DVec3::ZERO),
        });
        a.leaves += 1;
        if l.cluster >= 0 {
            a.clusters.insert(l.cluster);
        }
        a.faces.extend(v.leaf_faces.iter().skip(l.first_face).take(l.faces).copied());
        if l.volume() > a.largest.0 {
            a.largest = (l.volume(), l.center());
        }
    }
    let mut areas: Vec<(u16, Area)> = areas.into_iter().collect();
    areas.sort_by(|a, b| b.1.faces.len().cmp(&a.1.faces.len()).then(a.0.cmp(&b.0)));
    out.push(format!("Areas by faces (top {} of {}):", areas.len().min(TOP), areas.len()));
    for (k, (n, a)) in areas.iter().take(TOP).enumerate() {
        out.push(format!(
            "#{}  area {n}  {} leaves  {} clusters  {} faces  {}",
            k + 1,
            a.leaves,
            a.clusters.len(),
            a.faces.len(),
            at(a.largest.1)
        ));
    }

    small.sort_by(|a, b| {
        let vol = |b: &MapBrush| (b.maxs - b.mins).max(DVec3::ZERO).element_product();
        vol(a).total_cmp(&vol(b)).then(a.id.cmp(&b.id))
    });
    out.push(format!("Small structural brushes (top {} of {}):", small.len().min(TOP), small.len()));
    for (k, b) in small.iter().take(TOP).enumerate() {
        out.push(format!("#{}  brush {}  {}  {}", k + 1, b.id, size(b.mins, b.maxs), at((b.mins + b.maxs) * 0.5)));
    }
    let fadeless_total: usize = fadeless.values().map(|c| c.0).sum();
    let fadeless = ranked(fadeless);
    out.push(format!(
        "Static props without fade distance (top {} of {} props):",
        fadeless.len().min(TOP),
        fadeless_total
    ));
    for (k, (model, n, _)) in fadeless.iter().take(TOP).enumerate() {
        let place = fadeless_at.get(model).map(|p| format!("  {}", at(*p))).unwrap_or_default();
        out.push(format!("#{}  {n} props  {model}{place}", k + 1));
    }
    out.extend(stage_lines("geometry", input.stages));
    if let Some(&(_, vis)) = input.stages.iter().find(|(name, _)| name == "visibility")
        && vis > 0.0
    {
        let total: f64 = input.stages.iter().map(|(_, s)| s).sum();
        out.push(format!("Geometry {:.3} s, visibility {vis:.3} s", total - vis));
    }
    out
}

#[derive(Clone, Debug, Default)]
pub struct VisRun {
    pub mode: String,
    pub portals: usize,
    pub radius: Option<(f64, String)>,
    pub times: Vec<(String, f64)>,
}

fn pvs_rows(vis: &[u8]) -> Vec<Vec<u8>> {
    let n = usize::try_from(i32_at(vis, 0)).unwrap_or(0);
    if vis.len() < 4 + n * 8 {
        return Vec::new();
    }
    let bytes = n.div_ceil(8);
    (0..n)
        .map(|c| {
            let mut row = Vec::with_capacity(bytes);
            let mut at = usize::try_from(i32_at(vis, 4 + c * 8)).unwrap_or(usize::MAX);
            while row.len() < bytes {
                let Some(&b) = vis.get(at) else { break };
                if b == 0 {
                    let run = usize::from(vis.get(at + 1).copied().unwrap_or(0));
                    row.extend(std::iter::repeat_n(0, run.min(bytes - row.len())));
                    at += 2;
                } else {
                    row.push(b);
                    at += 1;
                }
            }
            row.resize(bytes, 0);
            row
        })
        .collect()
}

pub fn vis_lines(bsp: &hp_bsp::Bsp, run: &VisRun) -> Vec<String> {
    let v = View::read(bsp);
    let vis = bsp.lump(lump::VISIBILITY).map(std::borrow::Cow::into_owned).unwrap_or_default();
    let rows = pvs_rows(&vis);
    let clusters = if rows.is_empty() {
        v.leaves.iter().map(|l| i32::from(l.cluster) + 1).max().unwrap_or(0).max(0) as usize
    } else {
        rows.len()
    };
    let mut out =
        vec![VIS_HEADER.to_owned(), format!("{clusters:>6} portalclusters"), format!("{:>6} numportals", run.portals)];
    let empty = v.leaves.iter().filter(|l| l.contents & contents::SOLID as i32 == 0).count();
    out.push(format!("Clusters: {clusters}, portals: {}, leaves: {} ({empty} empty)", run.portals, v.leaves.len()));
    out.push(format!("Vis mode: {}", if rows.is_empty() && run.mode == "full" { "off" } else { run.mode.as_str() }));
    match &run.radius {
        Some((r, from)) => out.push(format!("Vis radius: {r:.0} ({from})")),
        None => out.push("Vis radius: none".to_owned()),
    }
    if !run.times.is_empty() {
        let total: f64 = run.times.iter().map(|t| t.1).sum();
        let parts: Vec<String> = run.times.iter().map(|(n, s)| format!("{n} {s:.3} s")).collect();
        out.push(format!("Vis times: {}, total {total:.3} s", parts.join(", ")));
    }
    if rows.is_empty() {
        return out;
    }
    let seen: Vec<usize> = rows.iter().map(|r| r.iter().map(|b| b.count_ones() as usize).sum()).collect();
    let total: usize = seen.iter().sum();
    let n = clusters.max(1) as f64;
    let avg = total as f64 / n;
    let (most, worst) =
        seen.iter().enumerate().max_by_key(|(c, s)| (**s, std::cmp::Reverse(*c))).map_or((0, 0), |(c, s)| (*s, c));
    out.push(format!(
        "Visible clusters: average {avg:.1} ({:.1}%), max {most} ({:.1}%) in cluster {worst}",
        avg * 100.0 / n,
        most as f64 * 100.0 / n
    ));
    let mut leaves_of: Vec<Vec<usize>> = vec![Vec::new(); clusters];
    for (i, l) in v.leaves.iter().enumerate() {
        if let Some(list) = usize::try_from(l.cluster).ok().and_then(|c| leaves_of.get_mut(c)) {
            list.push(i);
        }
    }
    let place = |c: usize| {
        leaves_of[c]
            .iter()
            .map(|&i| &v.leaves[i])
            .max_by(|a, b| a.volume().total_cmp(&b.volume()))
            .map_or(DVec3::ZERO, Leaf::center)
    };
    let mut order: Vec<usize> = (0..clusters).collect();
    order.sort_by(|a, b| seen[*b].cmp(&seen[*a]).then(a.cmp(b)));
    out.push(format!("Clusters that see the most (top {} of {clusters}):", clusters.min(TOP)));
    for (k, &c) in order.iter().take(TOP).enumerate() {
        let mut faces = ahash::AHashSet::new();
        for (o, bits) in rows[c].iter().enumerate() {
            for bit in 0..8 {
                if bits & (1 << bit) != 0
                    && let Some(list) = leaves_of.get(o * 8 + bit)
                {
                    for &l in list {
                        let leaf = &v.leaves[l];
                        faces.extend(v.leaf_faces.iter().skip(leaf.first_face).take(leaf.faces).copied());
                    }
                }
            }
        }
        out.push(format!(
            "#{}  cluster {c}  sees {} clusters ({:.1}%)  {} faces  {}",
            k + 1,
            seen[c],
            seen[c] as f64 * 100.0 / n,
            faces.len(),
            at(place(c))
        ));
    }
    out
}

#[derive(Clone, Debug, Default)]
pub struct RadReport {
    pub stages: Vec<(String, f64)>,
    pub bounces: u32,
    pub passes: Vec<String>,
    pub props_vertex_lit: usize,
    pub detail_lit: usize,
    pub texture_light_faces: usize,
    pub limits: hp_bsp::limits::LimitSet,
}

pub fn lighting_lines(bsp: &hp_bsp::Bsp, report: &RadReport) -> Vec<String> {
    let v = View::read(bsp);
    let mut out = vec![LIGHTING_HEADER.to_owned()];
    out.extend(table_lines(&v.lighting_rows()));
    let lit: Vec<(usize, u64, u64)> =
        v.faces.iter().enumerate().filter_map(|(i, f)| v.lightmap(f).map(|(l, b)| (i, l, b))).collect();
    let luxels: u64 = lit.iter().map(|x| x.1).sum();
    out.push(format!("Faces: {} faces, {} with lightmaps, {luxels} luxels", v.faces.len(), lit.len()));
    for (name, l) in [("LDR", lump::LIGHTING), ("HDR", lump::LIGHTING_HDR)] {
        let bytes = v.len(l);
        if bytes > 0 {
            out.push(format!(
                "Lightmap bytes: {name} {bytes} of {MAX_LIGHTING} ({:.1}%)",
                bytes as f64 * 100.0 / MAX_LIGHTING as f64
            ));
        }
    }
    let wl = [15usize, 54]
        .iter()
        .map(|&l| {
            let data = bsp.lump(l).map(std::borrow::Cow::into_owned).unwrap_or_default();
            hp_bsp::format::worldlights_as_v0(&data, bsp.lump_info(l).map_or(0, |i| i.version)).into_owned()
        })
        .find(|d| !d.is_empty())
        .unwrap_or_default();
    let mut kinds = [0usize; 6];
    let mut styles = ahash::AHashSet::new();
    for l in wl.chunks_exact(WORLDLIGHT_SIZE) {
        if let Some(k) = usize::try_from(i32_at(l, 40)).ok().and_then(|k| kinds.get_mut(k)) {
            *k += 1;
        }
        styles.insert(i32_at(l, 44));
    }
    let faces_raw = bsp.lump(lump::FACES).map(std::borrow::Cow::into_owned).unwrap_or_default();
    for f in faces_raw.chunks_exact(56) {
        styles.extend(f[16..20].iter().filter(|&&s| s != 255).map(|&s| i32::from(s)));
    }
    out.push(format!(
        "Lights: {} total, {} point, {} spot, {} sun, {} sky ambient, {} surface",
        kinds.iter().sum::<usize>(),
        kinds[1],
        kinds[2],
        kinds[3],
        kinds[5],
        kinds[0]
    ));
    let switchable = styles.iter().filter(|&&s| s >= 32).count();
    out.push(format!(
        "Light styles: {switchable} switchable of {}, {} styles used",
        hp_bsp::limits::SWITCHED_LIGHTS.max_in(report.limits),
        styles.len()
    ));
    if report.texture_light_faces > 0 {
        out.push(format!("Texture lights: {} faces, {} surface lights", report.texture_light_faces, kinds[0]));
    }
    let (props, _) = v.static_props().unwrap_or((0, 0));
    out.push(format!("Static props: {props}, {} with vertex lighting", report.props_vertex_lit));
    let (details, _) = v.detail_props().unwrap_or((0, 0));
    out.push(format!("Detail props: {details}, {} lit", report.detail_lit));
    out.push(format!("Bounces: {}", report.bounces));
    if !report.passes.is_empty() {
        out.push(format!("Passes: {}", report.passes.join(", ")));
    }
    let index = bsp.lump(lump::LEAF_AMBIENT_INDEX).map(std::borrow::Cow::into_owned).unwrap_or_default();
    let ambient_leaves = index.chunks_exact(4).filter(|c| u16_at(c, 0) > 0).count();
    out.push(format!("Leaf ambient: {} samples in {ambient_leaves} leaves", v.len(lump::LEAF_AMBIENT_LIGHTING) / 28));
    out.extend(stage_lines("lighting", &report.stages));

    struct Side {
        luxels: u64,
        scale: f64,
        material: String,
        lo: DVec3,
        hi: DVec3,
        side: u16,
    }
    let mut sides: ahash::AHashMap<(u16, i32, usize), Side> = ahash::AHashMap::new();
    let mut by_material: ahash::AHashMap<String, (usize, usize)> = ahash::AHashMap::new();
    for &(i, luxels, _) in &lit {
        let f = &v.faces[i];
        let side = v.face_ids.get(i).copied().unwrap_or(0);
        let key = (side, f.texinfo, if side == 0 { i } else { 0 });
        let per_unit = usize::try_from(f.texinfo).ok().and_then(|t| v.texinfo.get(t)).map_or(0.0, |t| t.2);
        let s = sides.entry(key).or_insert_with(|| Side {
            luxels: 0,
            scale: if per_unit > 0.0 { 1.0 / per_unit } else { 0.0 },
            material: v.material(f).to_owned(),
            lo: DVec3::splat(f64::MAX),
            hi: DVec3::splat(f64::MIN),
            side,
        });
        s.luxels += luxels;
        for p in v.points(f) {
            s.lo = s.lo.min(p);
            s.hi = s.hi.max(p);
        }
        let m = by_material.entry(v.material(f).to_owned()).or_default();
        m.0 += luxels as usize;
        m.1 += 1;
    }
    let mut sides: Vec<Side> = sides.into_values().filter(|s| s.lo.x <= s.hi.x).collect();
    sides.sort_by(|a, b| b.luxels.cmp(&a.luxels).then(a.side.cmp(&b.side)));
    out.push(format!("Faces with the most luxels (top {} of {}):", sides.len().min(TOP), sides.len()));
    for (k, s) in sides.iter().take(TOP).enumerate() {
        out.push(format!(
            "#{}  {} luxels  scale {}  side {}  {}  {}  {}",
            k + 1,
            s.luxels,
            (s.scale * 100.0).round() / 100.0,
            s.side,
            size(s.lo, s.hi),
            s.material,
            at((s.lo + s.hi) * 0.5)
        ));
    }
    let materials = ranked(by_material);
    out.push(format!("Materials with the most luxels (top {} of {}):", materials.len().min(TOP), materials.len()));
    for (k, (name, n, faces)) in materials.iter().take(TOP).enumerate() {
        out.push(format!(
            "#{}  {n} luxels  {:.1}%  {faces} faces  {name}",
            k + 1,
            *n as f64 * 100.0 / luxels.max(1) as f64
        ));
    }
    out
}

pub fn limit_lines(bsp: &hp_bsp::Bsp) -> Vec<String> {
    limit_lines_in(bsp, hp_bsp::limits::LimitSet::Gmod)
}

pub fn limit_lines_in(bsp: &hp_bsp::Bsp, limits: hp_bsp::limits::LimitSet) -> Vec<String> {
    let mut out = vec![GEOMETRY_HEADER.to_owned()];
    let mut v = View::read(bsp);
    v.limits = limits;
    out.extend(table_lines(&v.geometry_rows()));
    out
}

pub fn limit_rows(bsp: &hp_bsp::Bsp) -> Vec<LimitRow> {
    let v = View::read(bsp);
    let mut rows = v.geometry_rows();
    rows.extend(v.lighting_rows());
    rows
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::materials::{MaterialFacts, MaterialSource, facts_by_name};

    struct Mats;
    impl MaterialSource for Mats {
        fn facts(&self, name: &str) -> MaterialFacts {
            facts_by_name(name)
        }
    }

    fn solid(id: &mut u32, lo: [f64; 3], hi: [f64; 3], mat: &str) -> String {
        let (a, b) = (lo, hi);
        let p = |x: f64, y: f64, z: f64| format!("({x} {y} {z})");
        let planes = [
            format!("{} {} {}", p(a[0], b[1], b[2]), p(b[0], b[1], b[2]), p(b[0], a[1], b[2])),
            format!("{} {} {}", p(a[0], a[1], a[2]), p(b[0], a[1], a[2]), p(b[0], b[1], a[2])),
            format!("{} {} {}", p(a[0], b[1], b[2]), p(a[0], a[1], b[2]), p(a[0], a[1], a[2])),
            format!("{} {} {}", p(b[0], b[1], a[2]), p(b[0], a[1], a[2]), p(b[0], a[1], b[2])),
            format!("{} {} {}", p(b[0], b[1], b[2]), p(a[0], b[1], b[2]), p(a[0], b[1], a[2])),
            format!("{} {} {}", p(b[0], a[1], a[2]), p(a[0], a[1], a[2]), p(a[0], a[1], b[2])),
        ];
        *id += 1;
        let mut s = format!("solid\n{{\n\"id\" \"{id}\"\n");
        for (k, plane) in planes.iter().enumerate() {
            *id += 1;
            let (u, v) =
                if k < 2 { ("[1 0 0 0] 0.25", "[0 -1 0 0] 0.25") } else { ("[1 0 0 0] 0.25", "[0 0 -1 0] 0.25") };
            let (u, v) = if k == 2 || k == 3 { ("[0 1 0 0] 0.25", v) } else { (u, v) };
            s += &format!(
                "side\n{{\n\"id\" \"{id}\"\n\"plane\" \"{plane}\"\n\"material\" \"{mat}\"\n\"uaxis\" \"{u}\"\n\"vaxis\" \"{v}\"\n\"lightmapscale\" \"16\"\n}}\n"
            );
        }
        s + "}\n"
    }

    fn room() -> String {
        let w = "dev/dev_measuregeneric01b";
        let mut id = 1;
        let mut world = String::new();
        for (lo, hi) in [
            ([-16.0, -16.0, -16.0], [528.0, 272.0, 0.0]),
            ([-16.0, -16.0, 256.0], [528.0, 272.0, 272.0]),
            ([-16.0, -16.0, 0.0], [0.0, 272.0, 256.0]),
            ([512.0, -16.0, 0.0], [528.0, 272.0, 256.0]),
            ([0.0, -16.0, 0.0], [512.0, 0.0, 256.0]),
            ([0.0, 256.0, 0.0], [512.0, 272.0, 256.0]),
            ([240.0, 112.0, 0.0], [272.0, 144.0, 192.0]),
        ] {
            world += &solid(&mut id, lo, hi, w);
        }
        let detail = solid(&mut id, [64.0, 64.0, 0.0], [96.0, 96.0, 32.0], w);
        let door = solid(&mut id, [400.0, 64.0, 0.0], [416.0, 192.0, 128.0], w);
        format!(
            "versioninfo\n{{\n\"mapversion\" \"1\"\n}}\nworld\n{{\n\"id\" \"1\"\n\"classname\" \"worldspawn\"\n{world}}}\n\
             entity\n{{\n\"id\" \"900\"\n\"classname\" \"func_detail\"\n{detail}}}\n\
             entity\n{{\n\"id\" \"901\"\n\"classname\" \"func_door\"\n{door}}}\n\
             entity\n{{\n\"id\" \"902\"\n\"classname\" \"info_player_start\"\n\"origin\" \"32 32 16\"\n}}\n\
             entity\n{{\n\"id\" \"903\"\n\"classname\" \"prop_static\"\n\"model\" \"models/a.mdl\"\n\"origin\" \"100 100 0\"\n}}\n\
             entity\n{{\n\"id\" \"904\"\n\"classname\" \"prop_static\"\n\"model\" \"models/a.mdl\"\n\"origin\" \"150 100 0\"\n\"fademaxdist\" \"0\"\n}}\n\
             entity\n{{\n\"id\" \"905\"\n\"classname\" \"light\"\n\"origin\" \"128 128 200\"\n\"_light\" \"255 255 255 200\"\n}}\n"
        )
    }

    fn compile_room(vis: crate::VisLevel) -> (Vec<u8>, Vec<String>) {
        let map = hp_vmf::parse(&room()).expect("vmf");
        let mut lines = Vec::new();
        let opts = crate::CompileOptions { vis, ..Default::default() };
        let out = crate::compile(&map, &Mats, &crate::props::NoModels, &opts, &mut |l| lines.push(l.to_owned()))
            .expect("compile");
        (out.bsp, lines)
    }

    fn line<'a>(lines: &'a [String], start: &str) -> &'a str {
        lines
            .iter()
            .find(|l| l.starts_with(start))
            .unwrap_or_else(|| panic!("no line {start:?} in\n{}", lines.join("\n")))
    }

    #[test]
    fn the_compile_logs_the_geometry_and_the_visibility() {
        let (bsp, lines) = compile_room(crate::VisLevel::Full);
        for header in [GEOMETRY_HEADER, DETAILS_HEADER, VIS_HEADER, TABLE_HEADER] {
            assert!(lines.iter().any(|l| l == header), "{header}");
        }
        let file = hp_bsp::Bsp::from_vec(bsp).unwrap();
        let rows = limit_rows(&file);
        let models = rows.iter().find(|r| r.name == "models").unwrap();
        assert_eq!(models.used, Some(2), "the world and the door");
        assert!(line(&lines, "models ").contains("2/4096"), "GMod: 4096 brush models");
        let planes = rows.iter().find(|r| r.name == "planes").unwrap();
        assert_eq!(planes.used, Some(file.lump(lump::PLANES).unwrap().len() as u64 / 20));
        assert!(line(&lines, "Brushes: ").contains("7 structural, 1 detail"), "{}", line(&lines, "Brushes: "));
        assert!(line(&lines, "Brushes: ").contains("1 in brush entities"));
        assert!(line(&lines, "Entities: ").starts_with("Entities: 6 in the map"));
        assert!(line(&lines, "Entities: ").contains("2 brush entities"));
        let small = lines.iter().position(|l| l.starts_with("Small structural brushes")).unwrap();
        assert!(
            lines[small + 1].starts_with("#1  brush ") && lines[small + 1].contains("32 x 32 x 192 units"),
            "{}",
            lines[small + 1]
        );
        assert!(lines[small + 1].ends_with("at (256 128 96)"));
        let fade = lines.iter().position(|l| l.starts_with("Static props without fade distance")).unwrap();
        assert_eq!(lines[fade], "Static props without fade distance (top 1 of 2 props):");
        assert_eq!(lines[fade + 1], "#1  2 props  models/a.mdl  at (100 100 0)");
        assert!(line(&lines, "Lightmaps (estimate): ").contains("bytes per pass of 2147483647"));
        assert!(line(&lines, "Materials by faces").starts_with("Materials by faces (top 1 of 1)"));
        assert!(lines.iter().any(|l| l.starts_with("#1  ") && l.ends_with("100.0%  dev/dev_measuregeneric01b")));
        assert!(lines.iter().any(|l| l == "Stage times (geometry):"));
        assert!(lines.iter().any(|l| l.starts_with("#1  ") && l.ends_with(" s  read brushes")));
        assert!(line(&lines, "Total  ").ends_with(" s"));
        assert!(line(&lines, "Geometry ").contains(" s, visibility "));
        assert!(line(&lines, "Vis mode: ").ends_with("full"));
        assert!(line(&lines, "Visible clusters: average ").contains("max"));
        assert!(line(&lines, "Vis times: ").contains("base"));
        let worst = lines.iter().position(|l| l.starts_with("Clusters that see the most")).unwrap();
        assert!(
            lines[worst + 1].starts_with("#1  cluster ") && lines[worst + 1].contains(" faces  at ("),
            "{}",
            lines[worst + 1]
        );
        let first = lines.iter().position(|l| l == GEOMETRY_HEADER).unwrap();
        for l in &lines[first..] {
            let lower = l.to_ascii_lowercase();
            assert!(
                !["error", "warning", "leak", "fail", "cannot", "too many"].iter().any(|w| lower.contains(w)),
                "{l}"
            );
        }
    }

    #[test]
    fn no_visibility_and_the_lighting() {
        let (bsp, lines) = compile_room(crate::VisLevel::Off);
        assert_eq!(line(&lines, "Vis mode: "), "Vis mode: off");
        assert!(!lines.iter().any(|l| l.starts_with("Clusters that see the most")));
        let opts = crate::rad::RadOptions {
            ldr: true,
            hdr: false,
            bounces: 1,
            quality: crate::rad::RadQuality::Draft,
            threads: Some(2),
            ..Default::default()
        };
        let mut log = Vec::new();
        let (lit, report) =
            crate::rad::light_bsp_report(&bsp, &opts, &crate::props::NoModels, &mut |l| log.push(l.to_owned()))
                .expect("lit");
        assert!(report.stages.iter().any(|s| s.0 == "direct light"));
        assert_eq!(report.passes, ["LDR"]);
        let file = hp_bsp::Bsp::from_vec(lit).unwrap();
        let lines = lighting_lines(&file, &report);
        assert_eq!(lines[0], LIGHTING_HEADER);
        let bytes = file.lump(lump::LIGHTING).unwrap().len();
        assert!(
            line(&lines, "Lightmap bytes: LDR ").starts_with(&format!("Lightmap bytes: LDR {bytes} of 2147483647"))
        );
        assert!(line(&lines, "Lights: ").starts_with("Lights: 1 total, 1 point"), "{}", line(&lines, "Lights: "));
        assert!(line(&lines, "LDR lightdata ").contains("[variable]"));
        let top = lines.iter().position(|l| l.starts_with("Faces with the most luxels")).unwrap();
        let first = &lines[top + 1];
        assert!(
            first.starts_with("#1  ")
                && first.contains(" luxels  scale 16  side ")
                && first.contains("dev/dev_measuregeneric01b  at ("),
            "{first}"
        );
        assert!(first.contains("512 x 256 x 0 units"), "{first}");
        assert!(
            line(&lines, "Materials with the most luxels").starts_with("Materials with the most luxels (top 1 of 1)")
        );
        assert!(lines.iter().any(|l| l.starts_with("#1  ") && l.contains("100.0%")));
    }

    #[test]
    fn rows_and_warnings() {
        let r = LimitRow {
            name: "brushsides",
            valve: "MAX_MAP_BRUSHSIDES",
            used: Some(60_000),
            max: 65_536,
            bytes: 480_000,
            max_bytes: 524_288,
        };
        assert!((r.percent() - 91.55).abs() < 0.01);
        assert_eq!(
            r.line().split_whitespace().collect::<Vec<_>>(),
            ["brushsides", "60000/65536", "480000/524288", "(", "91.6%)"]
        );
        let lines = table_lines(std::slice::from_ref(&r));
        assert_eq!(lines[2], "Warning: brushsides uses 91.6% of the engine limit (MAX_MAP_BRUSHSIDES = 65536)");
        let v = LimitRow {
            name: "visdata",
            valve: "MAX_MAP_VISIBILITY",
            used: None,
            max: 0,
            bytes: 0x0200_0000,
            max_bytes: 0x0100_0000,
        };
        assert!(v.line().contains("[variable]"));
        assert_eq!(
            table_lines(&[v])[2],
            "visdata: 200.0% of the engine limit, over it (MAX_MAP_VISIBILITY = 16777216)"
        );
        let mut vis = Vec::new();
        vis.extend_from_slice(&2i32.to_le_bytes());
        vis.extend_from_slice(&20i32.to_le_bytes());
        vis.extend_from_slice(&20i32.to_le_bytes());
        vis.extend_from_slice(&22i32.to_le_bytes());
        vis.extend_from_slice(&22i32.to_le_bytes());
        vis.extend_from_slice(&[0, 1, 0b11]);
        assert_eq!(pvs_rows(&vis), [vec![0], vec![3]]);
    }
}
