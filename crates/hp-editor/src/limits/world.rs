use std::collections::HashMap;

use glam::DVec3;

use super::portals::BspVis;
use super::world_light::{self, LightMode, WorldLight};

pub mod lump {
    pub const TEXDATA: usize = 2;
    pub const VERTEXES: usize = 3;
    pub const EDGES: usize = 12;
    pub const SURFEDGES: usize = 13;
    pub const DISP_VERTS: usize = 33;
    pub const DISP_MULTIBLEND: usize = 63;
    pub const TEXDATA_STRING_TABLE: usize = 44;
    pub const TEXDATA_STRING_DATA: usize = 43;
    pub const FACES_HDR: usize = 58;
}

#[derive(Clone, Copy, Debug, Default)]
pub struct WorldLumps<'a> {
    pub planes: &'a [u8],
    pub vertexes: &'a [u8],
    pub edges: &'a [u8],
    pub surfedges: &'a [u8],
    pub faces: &'a [u8],
    pub texinfo: &'a [u8],
    pub texdata: &'a [u8],
    pub string_table: &'a [u8],
    pub string_data: &'a [u8],
    pub models: &'a [u8],
    pub leafs: &'a [u8],
    pub leaf_version: u32,
    pub leaf_faces: &'a [u8],
    pub dispinfo: &'a [u8],
    pub disp_verts: &'a [u8],
    pub disp_multiblend: &'a [u8],
    pub lighting: &'a [u8],
    pub light_faces: &'a [u8],
    pub light_mode: LightMode,
    pub brush_entities: &'a [BrushEntity],
}

#[derive(Clone, Copy, Debug, Default, PartialEq)]
pub struct BrushEntity {
    pub model: u32,
    pub id: u32,
    pub origin: [f32; 3],
    pub angles: [f32; 3],
}

#[derive(Clone, Debug, Default, PartialEq)]
pub struct EntityModel {
    pub id: u32,
    pub faces: std::ops::Range<u32>,
}

#[derive(Clone, Copy, Debug, PartialEq)]
pub struct WorldFace {
    pub material: u32,
    pub first: u32,
    pub count: u32,
    pub disp_power: u8,
    pub sky: bool,
    pub normal: [f32; 3],
    pub dist: f32,
    pub light: u32,
}

pub const NO_LIGHTMAP: u32 = u32::MAX;

impl WorldFace {
    pub fn triangles(&self) -> u32 {
        if self.disp_power > 0 {
            let cells = (1u32 << self.disp_power).pow(2);
            cells * 2
        } else {
            self.count.saturating_sub(2)
        }
    }

    pub fn is_disp(&self) -> bool {
        self.disp_power > 0
    }
}

#[derive(Clone, Copy, Debug, PartialEq)]
pub struct WorldVertex {
    pub pos: [f32; 3],
    pub uv: [f32; 2],
    pub alpha: f32,
    pub lm: [f32; 2],
    pub multiblend: Option<[f32; 4]>,
}

#[derive(Clone, Debug, Default, PartialEq)]
pub struct FaceGroup {
    pub clusters: Vec<u32>,
    pub faces: Vec<u32>,
    pub min: [f32; 3],
    pub max: [f32; 3],
}

#[derive(Clone, Debug, Default)]
pub struct BspWorld {
    pub materials: Vec<String>,
    pub vertices: Vec<WorldVertex>,
    pub faces: Vec<WorldFace>,
    pub groups: Vec<FaceGroup>,
    pub details: Vec<DetailSprite>,
    pub detail_models: Vec<DetailModelPlacement>,
    pub detail_material: String,
    pub detail_fade: [f32; 2],
    pub static_props: Vec<StaticPropPlacement>,
    pub entity_ids: Vec<u32>,
    pub rope_entities: Vec<hp_vmf::Entity>,
    pub particle_entities: Vec<hp_vmf::Entity>,
    pub light: WorldLight,
    pub entity_faces: Vec<WorldFace>,
    pub entity_models: Vec<EntityModel>,
    pub cubemaps: Vec<WorldCubemap>,
    pub material_cubemaps: Vec<Option<[i32; 3]>>,
}

#[derive(Clone, Debug, Default, PartialEq)]
pub struct WorldCubemap {
    pub origin: [i32; 3],
    pub size: i32,
    pub ldr: Option<std::sync::Arc<Vec<u8>>>,
    pub hdr: Option<std::sync::Arc<Vec<u8>>>,
    pub parallax: Option<([[f32; 4]; 3], [f32; 3])>,
}

impl WorldCubemap {
    pub fn built(&self) -> bool {
        self.ldr.is_some() || self.hdr.is_some()
    }
}

#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub enum DetailOrientation {
    #[default]
    Normal,
    ScreenAligned,
    Vertical,
}

#[derive(Clone, Copy, Debug, Default, PartialEq)]
pub struct DetailSprite {
    pub origin: [f32; 3],
    pub right: [f32; 3],
    pub up: [f32; 3],
    pub ul: [f32; 2],
    pub lr: [f32; 2],
    pub tex: [f32; 4],
    pub orientation: DetailOrientation,
    pub color: [u8; 4],
    pub group: u32,
}

#[derive(Clone, Debug, PartialEq)]
pub struct DetailModelPlacement {
    pub model: String,
    pub origin: [f32; 3],
    pub angles: [f32; 3],
    pub light: [f32; 3],
    pub group: u32,
}

impl Default for DetailModelPlacement {
    fn default() -> Self {
        Self { model: String::new(), origin: [0.0; 3], angles: [0.0; 3], light: [1.0; 3], group: 0 }
    }
}

#[derive(Clone, Debug, Default, PartialEq)]
pub struct StaticPropPlacement {
    pub model: String,
    pub origin: [f32; 3],
    pub angles: [f32; 3],
    pub skin: u32,
    pub fade: [f32; 2],
    pub group: u32,
}

const STATIC_PROP_RADIUS: f32 = 256.0;

pub const DETAIL_FADE_DEFAULT: [f32; 2] = [1200.0, 400.0];

pub fn detail_fade(controller: Option<(f32, f32)>) -> [f32; 2] {
    let [dist, fade] = DETAIL_FADE_DEFAULT;
    match controller {
        Some((start, end)) if start.is_finite() && end.is_finite() => {
            [dist.min(end).max(0.0), fade.min(start).max(0.0)]
        }
        _ => DETAIL_FADE_DEFAULT,
    }
}

pub fn detail_alpha(dist2: f32, fade: [f32; 2]) -> f32 {
    let max2 = fade[0] * fade[0];
    let start = fade[0] - fade[1];
    let fade2 = if start > 0.0 { start * start } else { 0.0 }.min(max2 - 1.0);
    if dist2 >= max2 {
        return 0.0;
    }
    if dist2 > fade2 { ((max2 - dist2) / (max2 - fade2)).clamp(0.0, 1.0) } else { 1.0 }
}

#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub struct WorldDraw {
    pub faces: u32,
    pub displacements: u32,
    pub triangles: u32,
    pub materials: u32,
    pub backfaces: u32,
    pub displacement_triangles: u32,
}

impl WorldDraw {
    pub fn without_displacements(self) -> WorldDraw {
        WorldDraw {
            displacements: 0,
            triangles: self.triangles.saturating_sub(self.displacement_triangles),
            displacement_triangles: 0,
            ..self
        }
    }
}

const FACE_SIZE: usize = 56;
const TEXINFO_SIZE: usize = 72;
const TEXDATA_SIZE: usize = 32;
const DISPINFO_SIZE: usize = 176;
const DISP_VERT_SIZE: usize = 20;
const SKIPPED: i32 = 0x80 | 0x100 | 0x200;
const SKY: i32 = 0x2 | 0x4;
const NOLIGHT: i32 = 0x400;
const BUMPLIGHT: i32 = 0x800;
const MAX_LUXELS: i32 = 1024;
const MAX_FACES: usize = 1 << 20;
const MAX_FACE_EDGES: usize = 512;
const MAX_POWER: i32 = 4;
const MAX_COORD: f32 = 1.0e6;
const BACKFACE_EPSILON: f32 = 0.01;

fn i32_at(d: &[u8], o: usize) -> Option<i32> {
    d.get(o..o + 4).map(|b| i32::from_le_bytes([b[0], b[1], b[2], b[3]]))
}

fn u16_at(d: &[u8], o: usize) -> Option<u16> {
    d.get(o..o + 2).map(|b| u16::from_le_bytes([b[0], b[1]]))
}

fn i16_at(d: &[u8], o: usize) -> Option<i16> {
    d.get(o..o + 2).map(|b| i16::from_le_bytes([b[0], b[1]]))
}

fn f32_at(d: &[u8], o: usize) -> Option<f32> {
    d.get(o..o + 4).map(|b| f32::from_le_bytes([b[0], b[1], b[2], b[3]]))
}

fn vec_at(d: &[u8], o: usize) -> Option<[f32; 3]> {
    let v = [f32_at(d, o)?, f32_at(d, o + 4)?, f32_at(d, o + 8)?];
    v.iter().all(|x| x.is_finite() && x.abs() <= MAX_COORD).then_some(v)
}

fn sub(a: [f32; 3], b: [f32; 3]) -> [f32; 3] {
    [a[0] - b[0], a[1] - b[1], a[2] - b[2]]
}

fn lerp(a: [f32; 3], b: [f32; 3], t: f32) -> [f32; 3] {
    [a[0] + (b[0] - a[0]) * t, a[1] + (b[1] - a[1]) * t, a[2] + (b[2] - a[2]) * t]
}

fn dot(a: [f32; 3], b: [f32; 3]) -> f32 {
    a[0] * b[0] + a[1] * b[1] + a[2] * b[2]
}

#[derive(Clone, Copy)]
struct TexProj {
    s: [f32; 4],
    t: [f32; 4],
}

impl TexProj {
    fn uv(&self, p: [f32; 3]) -> [f32; 2] {
        [
            p[0] * self.s[0] + p[1] * self.s[1] + p[2] * self.s[2] + self.s[3],
            p[0] * self.t[0] + p[1] * self.t[1] + p[2] * self.t[2] + self.t[3],
        ]
    }
}

fn texdata_name(l: &WorldLumps<'_>, i: usize) -> Option<String> {
    let id = usize::try_from(i32_at(l.texdata, i * TEXDATA_SIZE + 12)?).ok()?;
    let ofs = usize::try_from(i32_at(l.string_table, id * 4)?).ok()?;
    let rest = l.string_data.get(ofs..)?;
    let end = rest.iter().position(|b| *b == 0).unwrap_or(rest.len()).min(256);
    Some(String::from_utf8_lossy(&rest[..end]).into_owned())
}

pub fn patched_cubemap_origin(name: &str) -> Option<[i32; 3]> {
    let lower = name.replace('\\', "/").to_ascii_lowercase();
    let rest = lower.strip_prefix("maps/")?;
    let (_, mat) = rest.split_once('/')?;
    let parts: Vec<&str> = mat.rsplitn(4, '_').collect();
    if parts.len() != 4 || parts[3].is_empty() {
        return None;
    }
    let int = |p: &str| p.parse::<i32>().ok();
    Some([int(parts[2])?, int(parts[1])?, int(parts[0])?])
}

pub fn unpatched_material(name: &str) -> String {
    let lower = name.replace('\\', "/").to_ascii_lowercase();
    let Some(rest) = lower.strip_prefix("maps/") else { return name.to_owned() };
    let Some((_, mut mat)) = rest.split_once('/') else { return name.to_owned() };
    if let Some(m) = mat.strip_suffix("_wvt_patch") {
        mat = m;
    }
    let int = |p: &str| {
        let digits = p.strip_prefix('-').unwrap_or(p);
        !digits.is_empty() && digits.bytes().all(|b| b.is_ascii_digit())
    };
    let parts: Vec<&str> = mat.rsplitn(4, '_').collect();
    if parts.len() == 4 && parts[..3].iter().all(|p| int(p)) {
        mat = parts[3];
    }
    if let Some((m, depth)) = mat.rsplit_once("_depth_")
        && !depth.is_empty()
        && depth.bytes().all(|b| b.is_ascii_digit())
    {
        mat = m;
    }
    mat.to_owned()
}

impl BspWorld {
    pub fn add_details(
        &mut self,
        sprites: Vec<(DetailSprite, Option<u32>)>,
        models: Vec<(DetailModelPlacement, Option<u32>)>,
    ) {
        let mut index: HashMap<Vec<u32>, usize> = HashMap::new();
        for (i, g) in self.groups.iter().enumerate() {
            index.entry(g.clusters.clone()).or_insert(i);
        }
        let mut group_of = |groups: &mut Vec<FaceGroup>, cluster: Option<u32>, origin: [f32; 3], radius: f32| {
            let key: Vec<u32> = cluster.into_iter().collect();
            let g = *index.entry(key.clone()).or_insert_with(|| {
                groups.push(FaceGroup { clusters: key, faces: Vec::new(), min: origin, max: origin });
                groups.len() - 1
            });
            let group = &mut groups[g];
            for k in 0..3 {
                group.min[k] = group.min[k].min(origin[k] - radius);
                group.max[k] = group.max[k].max(origin[k] + radius);
            }
            g as u32
        };
        for (mut s, cluster) in sprites {
            let radius = s.ul.iter().chain(&s.lr).fold(0.0f32, |m, v| m.max(v.abs()));
            s.group = group_of(&mut self.groups, cluster, s.origin, radius);
            self.details.push(s);
        }
        for (mut m, cluster) in models {
            m.group = group_of(&mut self.groups, cluster, m.origin, 64.0);
            self.detail_models.push(m);
        }
    }

    pub fn add_static_props(&mut self, props: Vec<(StaticPropPlacement, Vec<u32>)>) {
        let mut index: HashMap<Vec<u32>, usize> = HashMap::new();
        for (i, g) in self.groups.iter().enumerate() {
            index.entry(g.clusters.clone()).or_insert(i);
        }
        for (mut p, mut clusters) in props {
            clusters.sort_unstable();
            clusters.dedup();
            let o = p.origin;
            let groups = &mut self.groups;
            let g = *index.entry(clusters.clone()).or_insert_with(|| {
                groups.push(FaceGroup { clusters, faces: Vec::new(), min: o, max: o });
                groups.len() - 1
            });
            let group = &mut self.groups[g];
            group.min = std::array::from_fn(|k| group.min[k].min(o[k] - STATIC_PROP_RADIUS));
            group.max = std::array::from_fn(|k| group.max[k].max(o[k] + STATIC_PROP_RADIUS));
            p.group = g as u32;
            self.static_props.push(p);
        }
    }

    pub fn from_lumps(l: &WorldLumps<'_>, vis: Option<&BspVis>) -> BspWorld {
        let first_face = i32_at(l.models, 40).and_then(|v| usize::try_from(v).ok()).unwrap_or(0);
        let face_count = i32_at(l.models, 44).and_then(|v| usize::try_from(v).ok()).unwrap_or(0);
        let total = (l.faces.len() / FACE_SIZE).min(MAX_FACES);
        let range = first_face.min(total)..(first_face.saturating_add(face_count)).min(total);

        let mut world = BspWorld::default();
        world.light.mode = l.light_mode;
        let mut material_index: HashMap<i32, u32> = HashMap::new();
        let mut kept: HashMap<usize, u32> = HashMap::new();
        for bsp_face in range {
            if let Some(face) = world.read_face(l, bsp_face, &mut material_index) {
                kept.insert(bsp_face, world.faces.len() as u32);
                world.faces.push(face);
            }
        }
        for e in l.brush_entities {
            let at = e.model as usize * 48;
            let (Some(first), Some(count)) = (i32_at(l.models, at + 40), i32_at(l.models, at + 44)) else { continue };
            let (Ok(first), Ok(count)) = (usize::try_from(first), usize::try_from(count)) else { continue };
            if e.model == 0 {
                continue;
            }
            let rot = glam::Mat3::from_euler(
                glam::EulerRot::ZYX,
                e.angles[1].to_radians(),
                e.angles[0].to_radians(),
                e.angles[2].to_radians(),
            );
            let origin = glam::Vec3::from(e.origin);
            let start = world.entity_faces.len() as u32;
            for bsp_face in first.min(total)..first.saturating_add(count).min(total) {
                let Some(mut face) = world.read_face(l, bsp_face, &mut material_index) else { continue };
                let range = face.first as usize..(face.first + face.count) as usize;
                for v in world.vertices.get_mut(range).unwrap_or_default() {
                    v.pos = (rot * glam::Vec3::from(v.pos) + origin).to_array();
                }
                let n = rot * glam::Vec3::from(face.normal);
                face.dist += n.dot(origin);
                face.normal = n.to_array();
                world.entity_faces.push(face);
            }
            let end = world.entity_faces.len() as u32;
            if end > start {
                world.entity_models.push(EntityModel { id: e.id, faces: start..end });
            }
        }
        world.group(l, &kept, vis);
        world
    }

    fn read_face(
        &mut self,
        l: &WorldLumps<'_>,
        bsp_face: usize,
        material_index: &mut HashMap<i32, u32>,
    ) -> Option<WorldFace> {
        let f = bsp_face * FACE_SIZE;
        let texinfo = i16_at(l.faces, f + 10).and_then(|t| usize::try_from(t).ok())?;
        let ti = texinfo * TEXINFO_SIZE;
        let flags = i32_at(l.texinfo, ti + 64)?;
        if flags & SKIPPED != 0 {
            return None;
        }
        let mut proj = TexProj { s: [0.0; 4], t: [0.0; 4] };
        for k in 0..4 {
            proj.s[k] = f32_at(l.texinfo, ti + k * 4).filter(|v| v.is_finite()).unwrap_or(0.0);
            proj.t[k] = f32_at(l.texinfo, ti + 16 + k * 4).filter(|v| v.is_finite()).unwrap_or(0.0);
        }
        let mut lm_proj = TexProj { s: [0.0; 4], t: [0.0; 4] };
        for k in 0..4 {
            lm_proj.s[k] = f32_at(l.texinfo, ti + 32 + k * 4).filter(|v| v.is_finite()).unwrap_or(0.0);
            lm_proj.t[k] = f32_at(l.texinfo, ti + 48 + k * 4).filter(|v| v.is_finite()).unwrap_or(0.0);
        }
        let texdata = i32_at(l.texinfo, ti + 68).unwrap_or(-1);
        let material = match material_index.get(&texdata) {
            Some(m) => *m,
            None => {
                let raw = usize::try_from(texdata).ok().and_then(|i| texdata_name(l, i));
                let name = raw.as_deref().map_or_else(|| "tools/toolsnodraw".to_owned(), unpatched_material);
                let m = self.materials.len() as u32;
                self.materials.push(name);
                self.material_cubemaps.push(raw.as_deref().and_then(patched_cubemap_origin));
                material_index.insert(texdata, m);
                m
            }
        };
        let first_edge = i32_at(l.faces, f + 4).and_then(|v| usize::try_from(v).ok()).unwrap_or(0);
        let num_edges = i16_at(l.faces, f + 8).and_then(|v| usize::try_from(v).ok()).unwrap_or(0);
        if !(3..=MAX_FACE_EDGES).contains(&num_edges) {
            return None;
        }
        let mut poly: Vec<[f32; 3]> = Vec::with_capacity(num_edges);
        for e in 0..num_edges {
            let Some(se) = i32_at(l.surfedges, (first_edge + e) * 4) else { break };
            let (edge, back) = (se.unsigned_abs() as usize, se < 0);
            let Some(v) = u16_at(l.edges, edge * 4 + if back { 2 } else { 0 }) else { break };
            let Some(p) = vec_at(l.vertexes, usize::from(v) * 12) else { break };
            poly.push(p);
        }
        if poly.len() != num_edges {
            return None;
        }
        let plane = u16_at(l.faces, f).map_or(0, usize::from);
        let normal = vec_at(l.planes, plane * 20).unwrap_or([0.0, 0.0, 1.0]);
        let dist = f32_at(l.planes, plane * 20 + 12).filter(|v| v.is_finite()).unwrap_or(0.0);
        let sky = flags & SKY != 0;
        let disp = i16_at(l.faces, f + 12).unwrap_or(-1);
        let luxel = |o: usize| i32_at(l.faces, f + o).unwrap_or(0);
        let mins = [luxel(28) as f32, luxel(32) as f32];
        let size = [luxel(36).clamp(0, MAX_LUXELS), luxel(40).clamp(0, MAX_LUXELS)];
        let first = self.vertices.len() as u32;
        let mut face = if disp >= 0 && poly.len() == 4 {
            let lm_size = [size[0] as f32, size[1] as f32];
            match build_disp(l, usize::try_from(disp).unwrap_or(0), &poly, &proj, lm_size, &mut self.vertices) {
                Some((count, power)) => {
                    WorldFace { material, first, count, disp_power: power, sky, normal, dist, light: NO_LIGHTMAP }
                }
                None => return None,
            }
        } else {
            self.vertices.extend(poly.iter().map(|p| {
                let c = lm_proj.uv(*p);
                WorldVertex {
                    pos: *p,
                    uv: proj.uv(*p),
                    alpha: 0.0,
                    lm: [c[0] - mins[0] + 0.5, c[1] - mins[1] + 0.5],
                    multiblend: None,
                }
            }));
            WorldFace {
                material,
                first,
                count: poly.len() as u32,
                disp_power: 0,
                sky,
                normal,
                dist,
                light: NO_LIGHTMAP,
            }
        };
        if l.light_mode != LightMode::Fullbright && flags & NOLIGHT == 0 {
            let lf = if l.light_faces.len() == l.faces.len() { l.light_faces } else { l.faces };
            let lightofs = i32_at(lf, f + 20).unwrap_or(-1);
            let styles = lf.get(f + 16..f + 20).map_or([255; 4], |b| [b[0], b[1], b[2], b[3]]);
            let (w, h) = (size[0] as usize + 1, size[1] as usize + 1);
            let bumped = flags & BUMPLIGHT != 0;
            let image = world_light::face_light_image(l.lighting, lightofs, styles, w, h, bumped, l.light_mode);
            if let Some(image) = image {
                face.light = self.light.images.len() as u32;
                self.light.images.push(image);
            }
        }
        Some(face)
    }

    fn group(&mut self, l: &WorldLumps<'_>, kept: &HashMap<usize, u32>, vis: Option<&BspVis>) {
        let mut clusters_of: Vec<Vec<u32>> = vec![Vec::new(); self.faces.len()];
        let leaf_size = if l.leaf_version == 0 { 56 } else { 32 };
        for leaf in l.leafs.chunks_exact(leaf_size) {
            let Some(cluster) = i16_at(leaf, 4).and_then(|c| u32::try_from(c).ok()) else { continue };
            let first = usize::from(u16_at(leaf, 20).unwrap_or(0));
            let count = usize::from(u16_at(leaf, 22).unwrap_or(0));
            for i in first..first + count {
                let Some(bsp_face) = u16_at(l.leaf_faces, i * 2) else { break };
                if let Some(&f) = kept.get(&usize::from(bsp_face)) {
                    let list = &mut clusters_of[f as usize];
                    if !list.contains(&cluster) {
                        list.push(cluster);
                    }
                }
            }
        }
        if let Some(vis) = vis {
            for (i, face) in self.faces.iter().enumerate() {
                if face.is_disp() {
                    let (min, max) = self.bounds_of(face);
                    clusters_of[i] = vis.clusters_in_box(
                        DVec3::new(f64::from(min[0]), f64::from(min[1]), f64::from(min[2])),
                        DVec3::new(f64::from(max[0]), f64::from(max[1]), f64::from(max[2])),
                    );
                }
            }
        }
        let mut index: HashMap<Vec<u32>, usize> = HashMap::new();
        for (i, mut clusters) in clusters_of.into_iter().enumerate() {
            clusters.sort_unstable();
            let (min, max) = self.bounds_of(&self.faces[i]);
            let g = *index.entry(clusters.clone()).or_insert_with(|| {
                self.groups.push(FaceGroup { clusters, faces: Vec::new(), min, max });
                self.groups.len() - 1
            });
            let group = &mut self.groups[g];
            group.faces.push(i as u32);
            for k in 0..3 {
                group.min[k] = group.min[k].min(min[k]);
                group.max[k] = group.max[k].max(max[k]);
            }
        }
    }

    pub fn face_vertices(&self, face: &WorldFace) -> &[WorldVertex] {
        let a = face.first as usize;
        self.vertices.get(a..a + face.count as usize).unwrap_or(&[])
    }

    pub fn bounds_of(&self, face: &WorldFace) -> ([f32; 3], [f32; 3]) {
        let mut min = [f32::MAX; 3];
        let mut max = [f32::MIN; 3];
        for v in self.face_vertices(face) {
            for k in 0..3 {
                min[k] = min[k].min(v.pos[k]);
                max[k] = max[k].max(v.pos[k]);
            }
        }
        (min, max)
    }

    pub fn face_triangles(face: &WorldFace) -> Vec<u32> {
        if face.is_disp() {
            let n = (1u32 << face.disp_power) + 1;
            let mut out = Vec::with_capacity(((n - 1) * (n - 1) * 6) as usize);
            for i in 0..n - 1 {
                for j in 0..n - 1 {
                    let a = i * n + j;
                    let (b, c, d) = (a + 1, a + n, a + n + 1);
                    if (i + j) % 2 == 0 {
                        out.extend_from_slice(&[a, c, d, a, d, b]);
                    } else {
                        out.extend_from_slice(&[a, c, b, b, c, d]);
                    }
                }
            }
            out
        } else {
            (1..face.count.saturating_sub(1)).flat_map(|i| [0, i, i + 1]).collect()
        }
    }

    pub fn draw_stats(&self, eye: [f32; 3], drawn: impl Fn(usize) -> bool) -> WorldDraw {
        let mut out = WorldDraw::default();
        let mut materials: Vec<bool> = vec![false; self.materials.len()];
        for (g, group) in self.groups.iter().enumerate() {
            if !drawn(g) {
                continue;
            }
            for &f in &group.faces {
                let Some(face) = self.faces.get(f as usize) else { continue };
                if face.sky {
                    continue;
                }
                if !face.is_disp() && dot(face.normal, eye) - face.dist < -BACKFACE_EPSILON {
                    out.backfaces += 1;
                    continue;
                }
                if face.is_disp() {
                    out.displacements += 1;
                    out.displacement_triangles += face.triangles();
                } else {
                    out.faces += 1;
                }
                out.triangles += face.triangles();
                if let Some(m) = materials.get_mut(face.material as usize) {
                    *m = true;
                }
            }
        }
        out.materials = materials.iter().filter(|m| **m).count() as u32;
        out
    }
}

const MULTIBLEND_SIZE: usize = 80;
const DISP_HAS_MULTIBLEND: i32 = 0x4000_0000;

fn multiblend_start(l: &WorldLumps<'_>, index: usize) -> Option<usize> {
    if l.disp_multiblend.is_empty() {
        return None;
    }
    let flagged = |k: usize| {
        let tess = i32_at(l.dispinfo, k * DISPINFO_SIZE + 24)?;
        let power = i32_at(l.dispinfo, k * DISPINFO_SIZE + 20)?;
        (tess < 0 && tess & DISP_HAS_MULTIBLEND != 0 && (1..=MAX_POWER).contains(&power))
            .then(|| ((1usize << power) + 1).pow(2))
    };
    flagged(index)?;
    let start: usize = (0..index).filter_map(flagged).sum();
    let end = (start + flagged(index)?) * MULTIBLEND_SIZE;
    (end <= l.disp_multiblend.len()).then_some(start)
}

fn build_disp(
    l: &WorldLumps<'_>,
    index: usize,
    poly: &[[f32; 3]],
    proj: &TexProj,
    lm_size: [f32; 2],
    out: &mut Vec<WorldVertex>,
) -> Option<(u32, u8)> {
    let d = index * DISPINFO_SIZE;
    let start = vec_at(l.dispinfo, d)?;
    let first_vert = usize::try_from(i32_at(l.dispinfo, d + 12)?).ok()?;
    let power = i32_at(l.dispinfo, d + 20)?;
    if !(1..=MAX_POWER).contains(&power) {
        return None;
    }
    let n = (1usize << power) + 1;
    let blend = multiblend_start(l, index).map(|s| s * MULTIBLEND_SIZE);
    let closest = (0..4).min_by(|a, b| {
        let da = sub(poly[*a], start);
        let db = sub(poly[*b], start);
        dot(da, da).total_cmp(&dot(db, db))
    })?;
    let c: Vec<[f32; 3]> = (0..4).map(|k| poly[(closest + k) % 4]).collect();
    let uv: Vec<[f32; 2]> = c.iter().map(|p| proj.uv(*p)).collect();
    let step = 1.0 / (n - 1) as f32;
    for i in 0..n {
        let ti = i as f32 * step;
        let left = lerp(c[0], c[1], ti);
        let right = lerp(c[3], c[2], ti);
        let uv_left = [uv[0][0] + (uv[1][0] - uv[0][0]) * ti, uv[0][1] + (uv[1][1] - uv[0][1]) * ti];
        let uv_right = [uv[3][0] + (uv[2][0] - uv[3][0]) * ti, uv[3][1] + (uv[2][1] - uv[3][1]) * ti];
        for j in 0..n {
            let tj = j as f32 * step;
            let base = lerp(left, right, tj);
            let v = (first_vert + i * n + j) * DISP_VERT_SIZE;
            let dir = vec_at(l.disp_verts, v).unwrap_or([0.0; 3]);
            let dist = f32_at(l.disp_verts, v + 12).filter(|x| x.is_finite() && x.abs() <= MAX_COORD).unwrap_or(0.0);
            let alpha = f32_at(l.disp_verts, v + 16).filter(|x| x.is_finite()).unwrap_or(0.0);
            let pos = [base[0] + dir[0] * dist, base[1] + dir[1] * dist, base[2] + dir[2] * dist];
            let uvp = [uv_left[0] + (uv_right[0] - uv_left[0]) * tj, uv_left[1] + (uv_right[1] - uv_left[1]) * tj];
            let lm = [tj * lm_size[0] + 0.5, ti * lm_size[1] + 0.5];
            let multiblend = blend.and_then(|s| {
                let at = s + (i * n + j) * MULTIBLEND_SIZE;
                let w: [f32; 4] = std::array::from_fn(|k| f32_at(l.disp_multiblend, at + k * 4).unwrap_or(f32::NAN));
                w.iter().all(|v| v.is_finite()).then_some(w)
            });
            out.push(WorldVertex { pos, uv: uvp, alpha: (alpha / 255.0).clamp(0.0, 1.0), lm, multiblend });
        }
    }
    Some(((n * n) as u32, power as u8))
}

#[cfg(test)]
mod tests {
    use super::*;

    fn put_f32(out: &mut Vec<u8>, v: &[f32]) {
        for x in v {
            out.extend_from_slice(&x.to_le_bytes());
        }
    }

    fn quad_lumps() -> (Vec<Vec<u8>>, u32) {
        let mut planes = Vec::new();
        put_f32(&mut planes, &[0.0, 0.0, 1.0, 0.0]);
        planes.extend_from_slice(&2i32.to_le_bytes());
        let mut vertexes = Vec::new();
        for p in [[0.0, 0.0, 0.0], [64.0, 0.0, 0.0], [64.0, 64.0, 0.0], [0.0, 64.0, 0.0]] {
            put_f32(&mut vertexes, &p);
        }
        let mut edges = Vec::new();
        for (a, b) in [(0u16, 0u16), (0, 1), (1, 2), (2, 3), (3, 0)] {
            edges.extend_from_slice(&a.to_le_bytes());
            edges.extend_from_slice(&b.to_le_bytes());
        }
        let mut surfedges = Vec::new();
        for e in [1i32, 2, 3, 4] {
            surfedges.extend_from_slice(&e.to_le_bytes());
        }
        let mut face = vec![0u8; FACE_SIZE];
        face[4..8].copy_from_slice(&0i32.to_le_bytes());
        face[8..10].copy_from_slice(&4i16.to_le_bytes());
        face[10..12].copy_from_slice(&0i16.to_le_bytes());
        face[12..14].copy_from_slice(&(-1i16).to_le_bytes());
        let mut texinfo = Vec::new();
        put_f32(&mut texinfo, &[1.0, 0.0, 0.0, 0.0, 0.0, -1.0, 0.0, 0.0]);
        put_f32(&mut texinfo, &[0.0; 8]);
        texinfo.extend_from_slice(&0i32.to_le_bytes());
        texinfo.extend_from_slice(&0i32.to_le_bytes());
        let mut texdata = vec![0u8; TEXDATA_SIZE];
        texdata[12..16].copy_from_slice(&0i32.to_le_bytes());
        let string_table = 0i32.to_le_bytes().to_vec();
        let string_data = b"DEV/DEV_FLOOR\0".to_vec();
        let mut models = vec![0u8; 48];
        models[44..48].copy_from_slice(&1i32.to_le_bytes());
        let mut leaf = vec![0u8; 32];
        leaf[4..6].copy_from_slice(&3i16.to_le_bytes());
        leaf[22..24].copy_from_slice(&1u16.to_le_bytes());
        let leaf_faces = 0u16.to_le_bytes().to_vec();
        (
            vec![
                planes,
                vertexes,
                edges,
                surfedges,
                face,
                texinfo,
                texdata,
                string_table,
                string_data,
                models,
                leaf,
                leaf_faces,
            ],
            1,
        )
    }

    #[test]
    fn patched_materials() {
        assert_eq!(patched_cubemap_origin("maps/gm_test/dev/dev_floor_128_-64_32"), Some([128, -64, 32]));
        assert_eq!(patched_cubemap_origin("MAPS\\gm_test\\metal\\a_b_1_2_3"), Some([1, 2, 3]));
        assert_eq!(patched_cubemap_origin("dev/dev_floor_128_-64_32"), None, "not a patch");
        assert_eq!(patched_cubemap_origin("maps/gm_test/nature/water_canals_depth_256"), None);
        assert_eq!(patched_cubemap_origin("maps/gm_test/brick/wall_a_b_c"), None);
        assert_eq!(unpatched_material("maps/gm_test/dev/dev_floor_128_-64_32"), "dev/dev_floor");
        assert_eq!(unpatched_material("MAPS/gm_test/nature/blend_wvt_patch"), "nature/blend");
        assert_eq!(unpatched_material("maps/gm_test/nature/water_canals_depth_256"), "nature/water_canals");
        assert_eq!(unpatched_material("concrete/wall_1_2"), "concrete/wall_1_2");
        assert_eq!(unpatched_material("maps/gm_test/brick/wall_a_b_c"), "brick/wall_a_b_c");
    }

    #[test]
    fn detail_objects_fade_like_the_client() {
        assert_eq!(detail_fade(None), [1200.0, 400.0]);
        let f = detail_fade(None);
        assert_eq!(detail_alpha(500.0 * 500.0, f), 1.0);
        assert_eq!(detail_alpha(800.0 * 800.0, f), 1.0);
        let mid = detail_alpha(1000.0 * 1000.0, f);
        assert!((mid - (1200.0f32 * 1200.0 - 1.0e6) / (1200.0 * 1200.0 - 640_000.0)).abs() < 1e-5, "{mid}");
        assert_eq!(detail_alpha(1200.0 * 1200.0, f), 0.0);
        assert_eq!(detail_fade(Some((300.0, 900.0))), [900.0, 300.0]);
        assert_eq!(detail_fade(Some((800.0, 3000.0))), [1200.0, 400.0]);
        assert_eq!(detail_fade(Some((f32::NAN, 900.0))), [1200.0, 400.0]);
        let all = [500.0, 800.0];
        assert!(detail_alpha(100.0, all) < 1.0 && detail_alpha(0.0, all) == 1.0);
        assert_eq!(DetailModelPlacement::default().light, [1.0; 3], "white when unknown");
    }

    #[test]
    fn static_props_join_the_group_of_their_clusters() {
        let mut w = BspWorld::default();
        w.groups.push(FaceGroup { clusters: vec![2, 4], ..Default::default() });
        let prop =
            |x: f32| StaticPropPlacement { model: "models/a.mdl".into(), origin: [x, 0.0, 0.0], ..Default::default() };
        w.add_static_props(vec![(prop(10.0), vec![4, 2, 4]), (prop(20.0), vec![7]), (prop(30.0), Vec::new())]);
        assert_eq!(w.static_props.len(), 3);
        assert_eq!(w.static_props[0].group, 0, "clusters 2 and 4 have a group");
        assert_eq!(w.groups[w.static_props[1].group as usize].clusters, [7], "a new group");
        assert!(w.groups[w.static_props[2].group as usize].clusters.is_empty(), "drawn from everywhere");
        let g = &w.groups[0];
        assert!(g.min[0] <= 10.0 - 256.0 && g.max[0] >= 10.0 + 256.0, "bounds grow around the prop");
    }

    #[test]
    fn details_join_the_group_of_their_cluster() {
        let mut w = BspWorld {
            groups: vec![FaceGroup { clusters: vec![3], faces: vec![0], min: [0.0; 3], max: [10.0; 3] }],
            ..BspWorld::default()
        };
        let sprite =
            |x: f32| DetailSprite { origin: [x, 0.0, 0.0], ul: [-8.0, 16.0], lr: [8.0, 0.0], ..Default::default() };
        let model =
            DetailModelPlacement { model: "models/a.mdl".into(), origin: [0.0, 0.0, 50.0], ..Default::default() };
        w.add_details(
            vec![(sprite(20.0), Some(3)), (sprite(-5.0), Some(7)), (sprite(1.0), None)],
            vec![(model, Some(7))],
        );
        assert_eq!(w.details.len(), 3);
        assert_eq!(w.details[0].group, 0, "cluster 3 has a group");
        assert_eq!(w.groups[0].max[0], 36.0, "the group grows by the sprite");
        assert_eq!(w.groups[w.details[1].group as usize].clusters, [7], "a new group for cluster 7");
        assert!(w.groups[w.details[2].group as usize].clusters.is_empty(), "no cluster: drawn from everywhere");
        assert_eq!(w.detail_models[0].group, w.details[1].group, "same cluster, same group");
        assert_eq!(w.groups.len(), 3);
    }

    #[test]
    fn faces_materials_groups_and_stats() {
        let (b, leaf_version) = quad_lumps();
        let l = WorldLumps {
            planes: &b[0],
            vertexes: &b[1],
            edges: &b[2],
            surfedges: &b[3],
            faces: &b[4],
            texinfo: &b[5],
            texdata: &b[6],
            string_table: &b[7],
            string_data: &b[8],
            models: &b[9],
            leafs: &b[10],
            leaf_version,
            leaf_faces: &b[11],
            dispinfo: &[],
            disp_verts: &[],
            disp_multiblend: &[],
            lighting: &[],
            light_faces: &[],
            light_mode: LightMode::Fullbright,
            brush_entities: &[],
        };
        let w = BspWorld::from_lumps(&l, None);
        assert_eq!(w.materials, ["DEV/DEV_FLOOR"]);
        assert_eq!(w.faces.len(), 1);
        assert_eq!(w.faces[0].triangles(), 2);
        assert_eq!(w.groups.len(), 1);
        assert_eq!(w.groups[0].clusters, [3]);
        assert_eq!(w.face_vertices(&w.faces[0])[2].uv, [64.0, -64.0]);
        assert_eq!(BspWorld::face_triangles(&w.faces[0]), [0, 1, 2, 0, 2, 3]);
        let above = w.draw_stats([10.0, 10.0, 100.0], |_| true);
        assert_eq!((above.faces, above.triangles, above.materials, above.backfaces), (1, 2, 1, 0));
        let below = w.draw_stats([10.0, 10.0, -100.0], |_| true);
        assert_eq!((below.faces, below.backfaces), (0, 1));
        assert_eq!(w.draw_stats([0.0; 3], |_| false), WorldDraw::default());
        let d = WorldDraw {
            faces: 3,
            displacements: 2,
            triangles: 300,
            displacement_triangles: 256,
            ..WorldDraw::default()
        };
        let wire = d.without_displacements();
        assert_eq!((wire.faces, wire.displacements, wire.triangles), (3, 0, 44));
        let junk = WorldLumps { faces: &[1, 2, 3], models: &[9; 48], ..l };
        assert!(BspWorld::from_lumps(&junk, None).faces.is_empty());
    }

    fn lumps_of(b: &[Vec<u8>]) -> WorldLumps<'_> {
        WorldLumps {
            planes: &b[0],
            vertexes: &b[1],
            edges: &b[2],
            surfedges: &b[3],
            faces: &b[4],
            texinfo: &b[5],
            texdata: &b[6],
            string_table: &b[7],
            string_data: &b[8],
            models: &b[9],
            leafs: &b[10],
            leaf_version: 1,
            leaf_faces: &b[11],
            ..WorldLumps::default()
        }
    }

    fn lit_quad() -> Vec<Vec<u8>> {
        let (mut b, _) = quad_lumps();
        let face = &mut b[4];
        face[16..20].copy_from_slice(&[0, 255, 255, 255]);
        face[20..24].copy_from_slice(&0i32.to_le_bytes());
        face[28..32].copy_from_slice(&0i32.to_le_bytes());
        face[32..36].copy_from_slice(&(-4i32).to_le_bytes());
        face[36..40].copy_from_slice(&4i32.to_le_bytes());
        face[40..44].copy_from_slice(&4i32.to_le_bytes());
        let texinfo = &mut b[5];
        let lm: [f32; 8] = [1.0 / 16.0, 0.0, 0.0, 0.0, 0.0, -1.0 / 16.0, 0.0, 0.0];
        for (k, v) in lm.iter().enumerate() {
            texinfo[32 + k * 4..36 + k * 4].copy_from_slice(&v.to_le_bytes());
        }
        let lighting: Vec<u8> = (0..25u8).flat_map(|k| [k, 0, 0, 0]).collect();
        b.push(lighting);
        b
    }

    #[test]
    fn lightmap_coordinates_follow_the_engine() {
        let b = lit_quad();
        let l = WorldLumps { lighting: &b[12], light_mode: LightMode::Hdr, ..lumps_of(&b) };
        let w = BspWorld::from_lumps(&l, None);
        let face = w.faces[0];
        assert_eq!(face.light, 0);
        let img = &w.light.images[0];
        assert_eq!((img.width, img.height), (5, 5));
        assert!((img.texels[7][0] - 7.0 / 255.0).abs() < 1e-6);
        let v = w.face_vertices(&face);
        assert_eq!(v[0].pos, [0.0, 0.0, 0.0]);
        assert_eq!(v[0].lm, [0.5, 4.5]);
        assert_eq!(v[2].pos, [64.0, 64.0, 0.0]);
        assert_eq!(v[2].lm, [4.5, 0.5]);
        let ldr = BspWorld::from_lumps(&WorldLumps { light_mode: LightMode::Ldr, ..l }, None);
        assert_eq!(ldr.light.mode, LightMode::Ldr);
        assert_eq!(ldr.light.images[0].texels[7], world_light::ldr_lightmap([7.0 / 255.0, 0.0, 0.0]));
        let none = BspWorld::from_lumps(&lumps_of(&b), None);
        assert_eq!(none.light.mode, LightMode::Fullbright);
        assert!(none.light.images.is_empty() && none.faces[0].light == NO_LIGHTMAP);
        let mut hdr_faces = b[4].clone();
        hdr_faces[20..24].copy_from_slice(&1000i32.to_le_bytes());
        let off = BspWorld::from_lumps(&WorldLumps { light_faces: &hdr_faces, ..l }, None);
        assert_eq!(off.faces[0].light, NO_LIGHTMAP, "missing data: white");
    }

    #[test]
    fn displacement_lightmaps_span_the_grid() {
        let mut b = lit_quad();
        b[4][12..14].copy_from_slice(&0i16.to_le_bytes());
        b[4][28..36].copy_from_slice(&[0; 8]);
        b[4][36..40].copy_from_slice(&8i32.to_le_bytes());
        b[4][40..44].copy_from_slice(&4i32.to_le_bytes());
        let mut dispinfo = vec![0u8; DISPINFO_SIZE];
        dispinfo[20..24].copy_from_slice(&1i32.to_le_bytes());
        let disp_verts = vec![0u8; DISP_VERT_SIZE * 9];
        let lighting: Vec<u8> = (0..45u8).flat_map(|k| [k, 0, 0, 0]).collect();
        let l = WorldLumps {
            dispinfo: &dispinfo,
            disp_verts: &disp_verts,
            lighting: &lighting,
            light_mode: LightMode::Hdr,
            ..lumps_of(&b)
        };
        let w = BspWorld::from_lumps(&l, None);
        let face = w.faces[0];
        assert_eq!((face.disp_power, face.light), (1, 0));
        assert_eq!((w.light.images[0].width, w.light.images[0].height), (9, 5));
        let v = w.face_vertices(&face);
        assert_eq!((v[0].pos, v[0].lm), ([0.0, 0.0, 0.0], [0.5, 0.5]));
        assert_eq!((v[6].pos, v[6].lm), ([64.0, 0.0, 0.0], [0.5, 4.5]));
        assert_eq!((v[2].pos, v[2].lm), ([0.0, 64.0, 0.0], [8.5, 0.5]));
        assert_eq!((v[8].pos, v[8].lm), ([64.0, 64.0, 0.0], [8.5, 4.5]));
        assert_eq!(v[4].lm, [4.5, 2.5]);
    }

    #[test]
    fn displacement_multiblend_is_read_per_vertex() {
        let mut b = lit_quad();
        b[4][12..14].copy_from_slice(&1i16.to_le_bytes());
        let mut dispinfo = vec![0u8; DISPINFO_SIZE * 2];
        for k in 0..2 {
            dispinfo[k * DISPINFO_SIZE + 20..k * DISPINFO_SIZE + 24].copy_from_slice(&1i32.to_le_bytes());
        }
        let tess = i32::MIN | DISP_HAS_MULTIBLEND;
        dispinfo[DISPINFO_SIZE + 24..DISPINFO_SIZE + 28].copy_from_slice(&tess.to_le_bytes());
        let disp_verts = vec![0u8; DISP_VERT_SIZE * 18];
        let mut blend = Vec::new();
        for k in 0..9 {
            let mut e = [0.0f32; 20];
            e[k % 4] = 1.0;
            put_f32(&mut blend, &e);
        }
        let l = WorldLumps { dispinfo: &dispinfo, disp_verts: &disp_verts, disp_multiblend: &blend, ..lumps_of(&b) };
        let w = BspWorld::from_lumps(&l, None);
        let v = w.face_vertices(&w.faces[0]);
        assert_eq!(v[0].multiblend, Some([1.0, 0.0, 0.0, 0.0]));
        assert_eq!(v[3].multiblend, Some([0.0, 0.0, 0.0, 1.0]));
        let short = WorldLumps { disp_multiblend: &blend[..80 * 8], ..l };
        let w = BspWorld::from_lumps(&short, None);
        assert!(w.face_vertices(&w.faces[0]).iter().all(|v| v.multiblend.is_none()), "a short lump is ignored");
        let mut plain = dispinfo.clone();
        plain[DISPINFO_SIZE + 24..DISPINFO_SIZE + 28].copy_from_slice(&i32::MIN.to_le_bytes());
        let w = BspWorld::from_lumps(&WorldLumps { dispinfo: &plain, ..l }, None);
        assert!(w.face_vertices(&w.faces[0]).iter().all(|v| v.multiblend.is_none()), "no flag, no blend");
    }

    #[test]
    fn brush_entities_are_placed_with_their_lightmaps() {
        let mut b = lit_quad();
        let mut models = vec![0u8; 96];
        models[88..92].copy_from_slice(&0i32.to_le_bytes());
        models[92..96].copy_from_slice(&1i32.to_le_bytes());
        b[9] = models;
        let ents = [BrushEntity { model: 1, id: 42, origin: [100.0, 0.0, 10.0], angles: [0.0, 90.0, 0.0] }];
        let l = WorldLumps { lighting: &b[12], light_mode: LightMode::Hdr, brush_entities: &ents, ..lumps_of(&b) };
        let w = BspWorld::from_lumps(&l, None);
        assert!(w.faces.is_empty(), "nothing in the world");
        assert_eq!(w.entity_models, [EntityModel { id: 42, faces: 0..1 }]);
        let face = w.entity_faces[0];
        assert_eq!(face.light, 0, "lit like the world");
        let v = w.face_vertices(&face);
        let p = v[1].pos;
        assert!((p[0] - 100.0).abs() < 1e-4 && (p[1] - 64.0).abs() < 1e-4 && (p[2] - 10.0).abs() < 1e-4, "{p:?}");
        assert_eq!(v[1].lm, [4.5, 4.5], "the lightmap does not move");
        assert!((face.dist - 10.0).abs() < 1e-4 && (face.normal[2] - 1.0).abs() < 1e-6);
    }
}
