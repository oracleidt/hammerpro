use glam::{DVec3, Vec3A};
use hp_raytrace::bvh::Bvh;
use rayon::prelude::*;

use crate::emit::lump;
use crate::materials::surf;
use crate::props::ModelSource;
use crate::rad::data::{MapData, i32_at};
use crate::rad::lights::Kind;
use crate::rad::surface::{self, LitFace};

pub(crate) const KIND_WORLD: u8 = 0;
pub(crate) const KIND_PROP: u8 = 1;
pub(crate) const KIND_SKY: u8 = 2;
pub(crate) const KIND_LIGHT: u8 = 3;
pub(crate) const NONE: u32 = u32::MAX;
const TEXTURE_MAX: u32 = 256;
const LIGHT_RADIUS: f32 = 3.0;
const LIGHT_REFERENCE: f32 = 128.0;
const LIGHT_PEAK: f32 = 255.0 * 64.0;
const SWITCHED_STYLES: u8 = 32;

#[derive(Clone, Copy, Debug)]
pub(crate) struct Tri {
    pub kind: u8,
    pub material: u32,
    pub face: u32,
    pub uv: [[f32; 2]; 3],
    pub lm: [[f32; 2]; 3],
}

#[derive(Clone, Debug, Default)]
pub(crate) struct Texture {
    pub w: u32,
    pub h: u32,
    pub px: Vec<[u8; 4]>,
}

#[derive(Clone, Debug)]
pub(crate) struct Material {
    pub texture: u32,
    pub tint: Vec3A,
    pub alpha_test: Option<f32>,
    pub translucent: bool,
    pub additive: bool,
    pub alpha: f32,
    pub unlit: bool,
    pub water: Option<Vec3A>,
    pub selfillum: bool,
    pub hidden: bool,
    pub two_sided: bool,
    pub average: Vec3A,
}

impl Default for Material {
    fn default() -> Self {
        Material {
            texture: NONE,
            tint: Vec3A::ONE,
            alpha_test: None,
            translucent: false,
            additive: false,
            alpha: 1.0,
            unlit: false,
            water: None,
            selfillum: false,
            hidden: false,
            two_sided: false,
            average: Vec3A::splat(0.5),
        }
    }
}

#[derive(Clone, Debug, Default)]
pub(crate) struct FaceLight {
    pub w: u32,
    pub h: u32,
    pub slots: [Vec<u32>; 2],
}

#[derive(Clone, Debug)]
pub(crate) struct Sphere {
    pub center: Vec3A,
    pub radius: f32,
    pub radiance: [Vec3A; 2],
    pub light: hp_raytrace::Light,
}

#[derive(Clone, Debug)]
pub(crate) struct SkyFace {
    pub w: u32,
    pub h: u32,
    pub texels: Vec<Vec3A>,
}

pub(crate) struct Scene {
    pub bvh: Bvh,
    pub tris: Vec<Tri>,
    pub normals: Vec<Vec3A>,
    pub prop_normals: Vec<[Vec3A; 3]>,
    pub materials: Vec<Material>,
    pub textures: Vec<Texture>,
    pub faces: Vec<FaceLight>,
    pub lighting: [Vec<u8>; 2],
    pub passes: [bool; 2],
    pub lights: Vec<hp_raytrace::Light>,
    pub ambient: [Vec3A; 2],
    pub spheres: Vec<Sphere>,
    pub sky: [Option<Vec<SkyFace>>; 2],
    pub camera: Option<(Vec3A, f32)>,
    pub lines: Vec<String>,
}

pub(crate) static SRGB: std::sync::LazyLock<[f32; 256]> =
    std::sync::LazyLock::new(|| std::array::from_fn(|i| (i as f32 / 255.0).powf(2.2)));

impl Texture {
    pub fn sample(&self, uv: [f32; 2]) -> [u8; 4] {
        if self.w == 0 || self.h == 0 {
            return [255; 4];
        }
        let x = ((uv[0] * self.w as f32).floor() as i64).rem_euclid(i64::from(self.w)) as usize;
        let y = ((uv[1] * self.h as f32).floor() as i64).rem_euclid(i64::from(self.h)) as usize;
        self.px.get(y * self.w as usize + x).copied().unwrap_or([255; 4])
    }
}

impl SkyFace {
    pub fn sample(&self, u: f32, v: f32) -> Vec3A {
        if self.w == 0 || self.h == 0 {
            return Vec3A::ZERO;
        }
        let fx = (u.clamp(0.0, 1.0) * self.w as f32 - 0.5).clamp(0.0, (self.w - 1) as f32);
        let fy = (v.clamp(0.0, 1.0) * self.h as f32 - 0.5).clamp(0.0, (self.h - 1) as f32);
        let (x0, y0) = (fx.floor() as usize, fy.floor() as usize);
        let (x1, y1) = ((x0 + 1).min(self.w as usize - 1), (y0 + 1).min(self.h as usize - 1));
        let (ax, ay) = (fx - x0 as f32, fy - y0 as f32);
        let w = self.w as usize;
        let at = |x: usize, y: usize| self.texels[y * w + x];
        let top = at(x0, y0) * (1.0 - ax) + at(x1, y0) * ax;
        let bottom = at(x0, y1) * (1.0 - ax) + at(x1, y1) * ax;
        top * (1.0 - ay) + bottom * ay
    }
}

fn text_of(bytes: &[u8]) -> String {
    hp_kv::decode(bytes).0.into_owned()
}

pub(crate) fn read_material(read: &dyn Fn(&str) -> Option<Vec<u8>>, path: &str) -> Option<hp_vmt::Material> {
    let data = read(path)?;
    let mut m = hp_vmt::parse(&text_of(&data)).ok()?;
    if m.is_patch() {
        let load = |p: &str| read(p).map(|b| text_of(&b));
        m = hp_vmt::resolve_patch(&m, &load).ok()?;
    }
    Some(m.resolve_fallbacks(hp_vmt::DEFAULT_DX_LEVEL, true))
}

fn load_texture(read: &dyn Fn(&str) -> Option<Vec<u8>>, name: &str) -> Option<Texture> {
    let vtf = read(&format!("materials/{name}.vtf"))?;
    let info = hp_vtf::read_info(&vtf).ok()?;
    let img = hp_vtf::raw_level(&vtf, info.mip_for_max_dimension(TEXTURE_MAX), 0, 0).ok()?.to_rgba8().ok()?;
    let px = img.pixels.chunks_exact(4).map(|p| [p[0], p[1], p[2], p[3]]).collect();
    Some(Texture { w: img.width, h: img.height, px })
}

fn average(t: &Texture) -> Vec3A {
    if t.px.is_empty() {
        return Vec3A::splat(0.5);
    }
    let mut sum = Vec3A::ZERO;
    for p in &t.px {
        sum += Vec3A::new(SRGB[p[0] as usize], SRGB[p[1] as usize], SRGB[p[2] as usize]);
    }
    sum / t.px.len() as f32
}

fn linear_color(c: [f32; 3]) -> Vec3A {
    Vec3A::from_array(c).max(Vec3A::ZERO)
}

fn material_of(
    name: &str,
    m: Option<hp_vmt::Material>,
    read: &dyn Fn(&str) -> Option<Vec<u8>>,
) -> (Material, Option<Texture>) {
    let lower = name.replace('\\', "/").to_ascii_lowercase();
    let Some(m) = m else {
        let hidden = lower.starts_with("tools/") && lower != "tools/toolsblack";
        return (Material { hidden, ..Material::default() }, None);
    };
    let tool = lower.starts_with("tools/") || m.is_tool();
    if tool && lower != "tools/toolsblack" {
        return (Material { hidden: true, ..Material::default() }, None);
    }
    let texture = m.base_texture().and_then(|t| load_texture(read, &t));
    let tint = linear_color(m.color());
    let avg = texture.as_ref().map_or(Vec3A::splat(0.5), average) * tint;
    let water = m.is_water().then(|| {
        let fog = m.get("$fogcolor").and_then(hp_vmt::parse_vec3).unwrap_or([0.1, 0.12, 0.12]);
        let fog = if fog.iter().any(|c| *c > 1.0) { fog.map(|c| c / 255.0) } else { fog };
        Vec3A::from_array(fog.map(|c| c.clamp(0.0, 1.0).powf(2.2)))
    });
    let mat = Material {
        texture: NONE,
        tint,
        alpha_test: m.alpha_test().then(|| m.alpha_test_reference()),
        translucent: m.translucent() || m.alpha() < 0.999,
        additive: m.additive(),
        alpha: m.alpha(),
        unlit: m.is_unlit() || lower == "tools/toolsblack",
        water,
        selfillum: m.self_illum(),
        hidden: false,
        two_sided: m.no_cull(),
        average: avg,
    };
    let mat =
        if lower == "tools/toolsblack" { Material { tint: Vec3A::ZERO, average: Vec3A::ZERO, ..mat } } else { mat };
    (mat, texture)
}

fn push_material(scene: &mut Scene, (mut m, t): (Material, Option<Texture>)) -> u32 {
    if let Some(t) = t {
        scene.textures.push(t);
        m.texture = (scene.textures.len() - 1) as u32;
    }
    scene.materials.push(m);
    (scene.materials.len() - 1) as u32
}

fn hdr_texels(level: &hp_vtf::RawLevel<'_>, compressed: bool) -> Option<Vec<Vec3A>> {
    use hp_vtf::ImageFormat as F;
    let half = |p: &[u8], i: usize| super::vtf::f16_to_f32(u16::from_le_bytes([p[2 * i], p[2 * i + 1]]));
    let texels: Vec<Vec3A> = match level.format {
        F::Rgba16161616F if !compressed => {
            level.data.chunks_exact(8).map(|p| Vec3A::new(half(p, 0), half(p, 1), half(p, 2))).collect()
        }
        F::Rgba16161616 if !compressed => level
            .data
            .chunks_exact(8)
            .map(|p| {
                let c = |i: usize| f32::from(u16::from_le_bytes([p[2 * i], p[2 * i + 1]])) / 65535.0 * 16.0;
                Vec3A::new(c(0), c(1), c(2))
            })
            .collect(),
        _ => {
            let img = level.to_rgba8().ok()?;
            img.pixels
                .chunks_exact(4)
                .map(|p| {
                    let c = Vec3A::new(f32::from(p[0]), f32::from(p[1]), f32::from(p[2])) / 255.0;
                    if compressed { c * (f32::from(p[3]) / 255.0 * 8.0) } else { c }
                })
                .collect()
        }
    };
    (texels.len() == level.width as usize * level.height as usize).then_some(texels)
}

const SKY_SIDES: [&str; 6] = ["rt", "lf", "bk", "ft", "up", "dn"];

fn sky_face(read: &dyn Fn(&str) -> Option<Vec<u8>>, sky: &str, side: &str, hdr: bool) -> Option<SkyFace> {
    let names = if hdr {
        vec![format!("skybox/{sky}_hdr{side}"), format!("skybox/{sky}{side}")]
    } else {
        vec![format!("skybox/{sky}{side}")]
    };
    names.iter().find_map(|name| {
        let m = read_material(read, &format!("materials/{name}.vmt"))?;
        let tint = linear_color(m.color());
        let keys: &[(&str, bool)] = if hdr {
            &[("$hdrcompressedtexture", true), ("$hdrbasetexture", false), ("$basetexture", false)]
        } else {
            &[("$basetexture", false)]
        };
        keys.iter().find_map(|(key, compressed)| {
            let tex = m.texture(key)?;
            let data = read(&format!("materials/{tex}.vtf"))?;
            let info = hp_vtf::read_info(&data).ok()?;
            let level = hp_vtf::raw_level(&data, info.mip_for_max_dimension(512), 0, 0).ok()?;
            let float = matches!(level.format, hp_vtf::ImageFormat::Rgba16161616F | hp_vtf::ImageFormat::Rgba16161616);
            let texels = if hdr && (*compressed || float) {
                hdr_texels(&level, *compressed)?
            } else {
                let img = level.to_rgba8().ok()?;
                img.pixels
                    .chunks_exact(4)
                    .map(|p| Vec3A::new(SRGB[p[0] as usize], SRGB[p[1] as usize], SRGB[p[2] as usize]))
                    .collect()
            };
            Some(SkyFace {
                w: level.width,
                h: level.height,
                texels: texels.into_iter().map(|c| c * tint * 255.0).collect(),
            })
        })
    })
}

fn sky_faces(read: &dyn Fn(&str) -> Option<Vec<u8>>, sky: &str, hdr: bool) -> Option<Vec<SkyFace>> {
    if sky.trim().is_empty() {
        return None;
    }
    SKY_SIDES.iter().map(|s| sky_face(read, sky.trim(), s, hdr)).collect()
}

fn off_styles(entities: &[hp_bsp::Entity]) -> Vec<u8> {
    let mut off = Vec::new();
    for e in entities {
        if !e.classname().to_ascii_lowercase().starts_with("light") {
            continue;
        }
        let style = e.get("style").and_then(|s| s.trim().parse::<i32>().ok()).unwrap_or(0);
        let flags = e.get("spawnflags").and_then(|s| s.trim().parse::<i64>().ok()).unwrap_or(0);
        if style >= i32::from(SWITCHED_STYLES) && style < 255 && flags & 1 != 0 {
            off.push(style as u8);
        }
    }
    off
}

fn face_light(raw: &[u8], fi: usize, lf: &LitFace, off: &[u8]) -> Vec<u32> {
    let o = fi * 56;
    let Some(rec) = raw.get(o..o + 56) else { return Vec::new() };
    let lightofs = i32_at(rec, 20);
    if lightofs < 0 {
        return Vec::new();
    }
    let normals = if lf.bump { 4 } else { 1 };
    let lux = lf.luxels();
    (0..4)
        .take_while(|&k| rec[16 + k] != 255)
        .filter(|&k| !off.contains(&rec[16 + k]))
        .map(|k| lightofs as u32 + (k * normals * lux * 4) as u32)
        .collect()
}

fn octahedron(c: Vec3A, r: f32) -> [[Vec3A; 3]; 8] {
    let p = [Vec3A::X, Vec3A::NEG_X, Vec3A::Y, Vec3A::NEG_Y, Vec3A::Z, Vec3A::NEG_Z].map(|d| c + d * r);
    [
        [p[0], p[2], p[4]],
        [p[2], p[1], p[4]],
        [p[1], p[3], p[4]],
        [p[3], p[0], p[4]],
        [p[2], p[0], p[5]],
        [p[1], p[2], p[5]],
        [p[3], p[1], p[5]],
        [p[0], p[3], p[5]],
    ]
}

fn sphere_of(l: &hp_raytrace::Light, passes: [bool; 2]) -> Sphere {
    let d = LIGHT_REFERENCE;
    let mut probe = *l;
    probe.kind = hp_raytrace::LightKind::Point;
    let f = probe.falloff(Vec3A::NEG_Z, d) * d * d / (LIGHT_RADIUS * LIGHT_RADIUS);
    let radiance = [0, 1].map(|p| {
        if !passes[p] {
            return Vec3A::ZERO;
        }
        let c = Vec3A::from_array(l.intensity[p]) * f;
        let peak = c.max_element();
        if peak > LIGHT_PEAK { c * (LIGHT_PEAK / peak) } else { c }
    });
    Sphere { center: Vec3A::from_array(l.position), radius: LIGHT_RADIUS, radiance, light: *l }
}

fn model_offsets(map: &MapData) -> Vec<Option<Vec3A>> {
    let mut out: Vec<Option<Vec3A>> = vec![None; map.models.len()];
    if let Some(first) = out.first_mut() {
        *first = Some(Vec3A::ZERO);
    }
    for e in &map.entities {
        let Some(k) = e.get("model").and_then(|m| m.strip_prefix('*')).and_then(|k| k.parse::<usize>().ok()) else {
            continue;
        };
        let class = e.classname().to_ascii_lowercase();
        let hidden = class.starts_with("trigger_")
            || class == "func_areaportal"
            || class == "func_areaportalwindow"
            || class == "func_occluder"
            || class == "func_viscluster"
            || e.get("rendermode").is_some_and(|m| m.trim() == "10")
            || e.get("renderamt").is_some_and(|a| a.trim() == "0");
        if hidden || k == 0 {
            continue;
        }
        let o = e.origin().map_or(Vec3A::ZERO, |o| DVec3::from_array(o).as_vec3a());
        if let Some(slot) = out.get_mut(k) {
            *slot = Some(o);
        }
    }
    out
}

pub(crate) fn sky_camera(map: &MapData) -> Option<(Vec3A, f32)> {
    map.entities.iter().filter(|e| e.classname().eq_ignore_ascii_case("sky_camera")).find_map(|e| {
        let o = e.origin()?;
        let scale = e.get("scale").and_then(|s| s.trim().parse::<f32>().ok()).filter(|s| *s > 0.0 && s.is_finite())?;
        Some((DVec3::from_array(o).as_vec3a(), scale))
    })
}

pub(crate) fn build(src: &hp_bsp::Bsp, models: &dyn ModelSource) -> Result<Scene, String> {
    let map = MapData::read(src)?;
    let read = |p: &str| models.read_file(p);
    let raw = |i: usize| src.lump(i).map(|c| c.into_owned()).unwrap_or_default();
    let lighting = [raw(lump::LIGHTING), raw(lump::LIGHTING_HDR)];
    let passes = [!lighting[0].is_empty(), !lighting[1].is_empty()];
    let faces_raw = [raw(lump::FACES), raw(lump::FACES_HDR)];
    let lit = surface::lit_faces(&map, 45.0);
    let mut scene = Scene {
        bvh: Bvh::default(),
        tris: Vec::new(),
        normals: Vec::new(),
        prop_normals: Vec::new(),
        materials: Vec::new(),
        textures: Vec::new(),
        faces: Vec::new(),
        lighting,
        passes,
        lights: Vec::new(),
        ambient: [Vec3A::ZERO; 2],
        spheres: Vec::new(),
        sky: [None, None],
        camera: sky_camera(&map),
        lines: Vec::new(),
    };
    let off = off_styles(&map.entities);
    let mut face_index = vec![NONE; map.faces.len()];
    for lf in lit.iter().flatten() {
        let slots = [0, 1].map(|p| {
            let table = if faces_raw[p].is_empty() { &faces_raw[0] } else { &faces_raw[p] };
            if passes[p] { face_light(table, lf.face, lf, &off) } else { Vec::new() }
        });
        face_index[lf.face] = scene.faces.len() as u32;
        scene.faces.push(FaceLight { w: lf.w as u32, h: lf.h as u32, slots });
    }
    let names = crate::rad::texlights::texdata(src);
    let loaded: Vec<(Material, Option<Texture>)> = names
        .par_iter()
        .map(|(name, _, _)| {
            let path = format!("materials/{}.vmt", name.replace('\\', "/").to_ascii_lowercase());
            material_of(name, read_material(&read, &path), &read)
        })
        .collect();
    for m in loaded {
        push_material(&mut scene, m);
    }
    let offsets = model_offsets(&map);
    let mut geometry: Vec<[Vec3A; 3]> = Vec::new();
    let mut add = |scene: &mut Scene, p: [Vec3A; 3], tri: Tri| {
        geometry.push(p);
        scene.tris.push(tri);
    };
    for (mi, m) in map.models.iter().enumerate() {
        let Some(offset) = offsets.get(mi).copied().flatten() else { continue };
        for fi in m.first_face..m.first_face.saturating_add(m.num_faces).min(map.faces.len()) {
            let face = &map.faces[fi];
            let Some(tex) = usize::try_from(face.texinfo).ok().and_then(|t| map.texinfo.get(t)) else { continue };
            if tex.flags & surf::NODRAW != 0 {
                continue;
            }
            let rel = map.face_points(face);
            if rel.len() < 3 {
                continue;
            }
            if tex.flags & surf::SKY != 0 {
                for k in 2..rel.len() {
                    let p = [rel[0], rel[k - 1], rel[k]].map(|v| v.as_vec3a() + offset);
                    add(
                        &mut scene,
                        p,
                        Tri { kind: KIND_SKY, material: NONE, face: NONE, uv: [[0.0; 2]; 3], lm: [[0.0; 2]; 3] },
                    );
                }
                continue;
            }
            let td = usize::try_from(tex.texdata).ok().filter(|t| *t < scene.materials.len());
            let Some(td) = td else { continue };
            if scene.materials[td].hidden {
                continue;
            }
            let (tw, th) = names.get(td).map_or((256, 256), |n| (n.1.max(1), n.2.max(1)));
            let uv_of = |p: DVec3| {
                let c = |k: usize| {
                    let v = tex.tex[k];
                    (p.x * f64::from(v[0]) + p.y * f64::from(v[1]) + p.z * f64::from(v[2]) + f64::from(v[3])) as f32
                };
                [c(0) / tw as f32, c(1) / th as f32]
            };
            let li = face_index[fi];
            let lf = lit[fi].as_ref().filter(|_| li != NONE);
            let normal = map.face_plane(face).map_or(DVec3::Z, |p| p.normal);
            match lf.and_then(|f| f.disp.as_ref().map(|d| (f, d))) {
                Some((f, d)) => {
                    for (t, uv) in d.triangles() {
                        let lm = uv.map(|c| [c.x as f32 * (f.w as f32 - 1.0), c.y as f32 * (f.h as f32 - 1.0)]);
                        let mut p = t.map(|v| v.as_vec3a());
                        let mut tuv = t.map(uv_of);
                        let mut lm = lm;
                        if (p[1] - p[0]).cross(p[2] - p[0]).dot(normal.as_vec3a()) < 0.0 {
                            p.swap(1, 2);
                            tuv.swap(1, 2);
                            lm.swap(1, 2);
                        }
                        add(&mut scene, p, Tri { kind: KIND_WORLD, material: td as u32, face: li, uv: tuv, lm });
                    }
                }
                None => {
                    for k in 2..rel.len() {
                        let t = [rel[0], rel[k - 1], rel[k]];
                        let world = t.map(|v| v + offset.as_dvec3());
                        let lm = match lf {
                            Some(f) => world.map(|p| {
                                let c = f.luxel_coords(p);
                                [c.x as f32, c.y as f32]
                            }),
                            None => [[0.0; 2]; 3],
                        };
                        let mut p = world.map(|v| v.as_vec3a());
                        let mut tuv = t.map(uv_of);
                        let mut lm = lm;
                        if (p[1] - p[0]).cross(p[2] - p[0]).dot(normal.as_vec3a()) < 0.0 {
                            p.swap(1, 2);
                            tuv.swap(1, 2);
                            lm.swap(1, 2);
                        }
                        add(
                            &mut scene,
                            p,
                            Tri {
                                kind: KIND_WORLD,
                                material: td as u32,
                                face: if lf.is_some() { li } else { NONE },
                                uv: tuv,
                                lm,
                            },
                        );
                    }
                }
            }
        }
    }
    let (props, prop_lines) = crate::rad::props::StaticProps::read_render(src, models)?;
    let (prop_tris, prop_materials) = props.render_triangles();
    drop(props);
    let first = scene.materials.len() as u32;
    let loaded: Vec<(Material, Option<Texture>)> = prop_materials
        .par_iter()
        .map(|paths| {
            paths
                .iter()
                .find_map(|p| read_material(&read, p).map(|m| (p, m)))
                .map(|(p, m)| {
                    let name = p.trim_start_matches("materials/").trim_end_matches(".vmt");
                    let (mut mat, t) = material_of(name, Some(m), &read);
                    mat.unlit = false;
                    mat.water = None;
                    (mat, t)
                })
                .unwrap_or_else(|| (Material { average: Vec3A::splat(0.35), ..Material::default() }, None))
        })
        .collect();
    for m in loaded {
        push_material(&mut scene, m);
    }
    for t in prop_tris {
        let face = scene.prop_normals.len() as u32;
        scene.prop_normals.push(t.n);
        add(&mut scene, t.p, Tri { kind: KIND_PROP, material: first + t.material, face, uv: t.uv, lm: [[0.0; 2]; 3] });
    }
    let parsed = crate::rad::lights::parse(&map.entities, &[false, true], &map);
    let params = hp_raytrace::Params::of(hp_raytrace::Quality::Normal);
    for l in &parsed.list {
        if l.style != 0 && off.contains(&l.style) {
            continue;
        }
        let mut e = super::super::bake::engine_light(l, parsed.sun_extent, &params);
        if !passes[0] {
            e.intensity[0] = [0.0; 3];
        }
        if !passes[1] {
            e.intensity[1] = [0.0; 3];
        }
        match l.kind {
            Kind::SkyAmbient => {
                scene.ambient = [0, 1].map(|p| Vec3A::from_array(e.intensity[p]));
            }
            Kind::Surface => {}
            Kind::Point | Kind::Spot { .. } => {
                let s = sphere_of(&e, passes);
                for p in octahedron(s.center, s.radius) {
                    let face = scene.spheres.len() as u32;
                    add(
                        &mut scene,
                        p,
                        Tri { kind: KIND_LIGHT, material: NONE, face, uv: [[0.0; 2]; 3], lm: [[0.0; 2]; 3] },
                    );
                }
                scene.spheres.push(s);
                scene.lights.push(e);
            }
            Kind::Sun => scene.lights.push(e),
        }
    }
    let sky = map.entities.first().and_then(|w| w.get("skyname")).unwrap_or("").to_owned();
    if sky.trim().eq_ignore_ascii_case(super::sky::PAINTED) {
        let paint = super::sky::paint_of(&map.entities);
        let stars = paint.stars.then(|| load_texture(&read, &paint.star_texture)).flatten();
        scene.sky = [0, 1].map(|p| passes[p].then(|| super::sky::painted_faces(&paint, p == 1, stars.as_ref())));
    } else {
        scene.sky = [0, 1].map(|p| if passes[p] { sky_faces(&read, &sky, p == 1) } else { None });
        if scene.sky[1].is_none() && passes[1] {
            scene.sky[1] = sky_faces(&read, &sky, false);
        }
    }
    let mut lines: Vec<String> = prop_lines.into_iter().filter(|l| l.starts_with("Error")).take(4).collect();
    if !sky.trim().is_empty() && passes.iter().zip(&scene.sky).any(|(p, s)| *p && s.is_none()) {
        lines.push(format!(
            "Warning: cubemaps: the sky textures skybox/{sky} cannot be read, the sky shows black in the cubemaps"
        ));
    }
    scene.lines = lines;
    scene.normals = geometry.iter().map(|p| (p[1] - p[0]).cross(p[2] - p[0]).normalize_or_zero()).collect();
    let ids: Vec<u32> = (0..geometry.len() as u32).collect();
    let arrays: Vec<[[f32; 3]; 3]> = geometry.iter().map(|t| t.map(|v| v.to_array())).collect();
    scene.bvh = if arrays.is_empty() { Bvh::default() } else { Bvh::build(&arrays, &ids) };
    Ok(scene)
}
