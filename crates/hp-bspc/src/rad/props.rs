mod alpha;
#[cfg(test)]
mod box_tests;
mod model;

use ahash::AHashMap as HashMap;

use glam::Vec3A;
use hp_light::bvh::{Blocker, Bvh, Hit, Triangle};
use rayon::prelude::*;

use super::bake::{Ctx, FaceLight, MAX_TRACE, Part, SurfHit};
use super::lights::Kind;
use super::output::{encode, finish};
use super::surface::SKY_BIT;
use crate::props::ModelSource;

pub(crate) const PROP_BIT: u32 = 1 << 30;
const ALPHA_BIT: u32 = 1 << 29;
const INDEX_MASK: u32 = ALPHA_BIT - 1;
pub(crate) const NO_PROP: u32 = u32::MAX;

pub(crate) const CLASS_WORLD: u8 = 1;
pub(crate) const CLASS_SKY: u8 = 2;
pub(crate) const CLASS_PROP: u8 = 4;
pub(crate) const CLASS_ALPHA: u8 = 8;
const CLASS_OPAQUE: u8 = CLASS_WORLD | CLASS_SKY | CLASS_PROP;

pub(crate) const SHADOW_GROUPS: &[u8] = &[CLASS_WORLD | CLASS_PROP, CLASS_SKY];

const HINT_SLOTS: usize = 4096;

thread_local! {
    static HINTS: std::cell::RefCell<Vec<(u64, Option<Blocker>)>> = std::cell::RefCell::new(vec![(u64::MAX, None); HINT_SLOTS]);
}

fn blocker_key(end: Vec3A, kind: u64) -> u64 {
    let q = (end * 8.0).round();
    let h = (q.x as i64 as u64).wrapping_mul(0x9E37_79B9_7F4A_7C15)
        ^ (q.y as i64 as u64).wrapping_mul(0xC2B2_AE3D_27D4_EB4F)
        ^ (q.z as i64 as u64).wrapping_mul(0x1656_67B1_9E37_79F9)
        ^ kind.wrapping_mul(0x27D4_EB2F_1656_67C5);
    h ^ (h >> 29)
}

pub(crate) fn shadow_class(p: u32) -> u8 {
    if p & SKY_BIT != 0 {
        CLASS_SKY
    } else if p & PROP_BIT == 0 {
        CLASS_WORLD
    } else if p & ALPHA_BIT != 0 {
        CLASS_ALPHA
    } else {
        CLASS_PROP
    }
}

const SPRP: u32 = u32::from_be_bytes(*b"sprp");
const USE_LIGHTING_ORIGIN: u32 = 0x2;
const IGNORE_NORMALS: u32 = 0x8;
const NO_SHADOW: u32 = 0x10;
const NO_PER_VERTEX_LIGHTING: u32 = 0x40;
const NO_SELF_SHADOWING: u32 = 0x80;
pub(crate) const LVLFLAGS_BAKED_LDR: u32 = 0x1;
pub(crate) const LVLFLAGS_BAKED_HDR: u32 = 0x2;
#[allow(clippy::approx_constant)]
const INDIRECT_CONSTANT_DOT: f32 = 0.7071 / 2.0;
const CONSTANT_DOT: f32 = 0.7 / 2.0;
const EQUAL_EPSILON: f32 = 0.001;
const SUN_SAMPLES: usize = 30 / 4;
const FAST_SAMPLES: usize = 162 / 4;
const VHV_VERSION: u32 = 2;
const VERTEX_COLOR: u32 = 0x4;
const DIST_EPSILON: f32 = 0.031_25;
const SOFT_CELL: f32 = 8.0;
const NORMAL_BINS: f32 = 2.0;

#[derive(Clone, Debug, PartialEq)]
pub struct PropOptions {
    pub lighting: bool,
    pub polys: bool,
    pub texture_shadows: bool,
    pub no_self_shadowing: bool,
    pub shadows: bool,
    pub show_normals: bool,
    pub fast: bool,
    pub sample_scale: f32,
    pub sky_sample_scale: f32,
}

impl Default for PropOptions {
    fn default() -> Self {
        PropOptions {
            lighting: false,
            polys: false,
            texture_shadows: false,
            no_self_shadowing: false,
            shadows: true,
            show_normals: false,
            fast: false,
            sample_scale: 1.0,
            sky_sample_scale: 1.0,
        }
    }
}

struct Sampler {
    seed: u32,
}

impl Sampler {
    fn new() -> Sampler {
        Sampler { seed: 1 }
    }

    fn halton(mut seed: u32, base: u32) -> f32 {
        let step = 1.0 / base as f32;
        let mut ret = 0.0f32;
        let mut inv = step;
        while seed != 0 {
            ret += (seed % base) as f32 * inv;
            inv *= step;
            seed /= base;
        }
        ret
    }

    fn next(&mut self) -> Vec3A {
        self.seed += 1;
        let z = 2.0 * Sampler::halton(self.seed, 2) - 1.0;
        let phi = z.acos();
        let theta = 2.0 * std::f32::consts::PI * Sampler::halton(self.seed, 3);
        let sp = phi.sin();
        Vec3A::new(theta.cos() * sp, theta.sin() * sp, z)
    }
}

fn angle_matrix(a: [f32; 3]) -> glam::Mat3A {
    let (sp, cp) = a[0].to_radians().sin_cos();
    let (sy, cy) = a[1].to_radians().sin_cos();
    let (sr, cr) = a[2].to_radians().sin_cos();
    glam::Mat3A::from_cols(
        Vec3A::new(cp * cy, cp * sy, -sp),
        Vec3A::new(sr * sp * cy - cr * sy, sr * sp * sy + cr * cy, sr * cp),
        Vec3A::new(cr * sp * cy + sr * sy, cr * sp * sy - sr * cy, cr * cp),
    )
}

#[derive(Clone, Debug)]
struct Prop {
    origin: Vec3A,
    angles: [f32; 3],
    model: usize,
    flags: u32,
    lighting_origin: Vec3A,
}

fn box_triangles(lo: Vec3A, hi: Vec3A) -> Vec<[Vec3A; 3]> {
    let c = |i: usize| {
        Vec3A::new(
            if i & 1 == 0 { lo.x } else { hi.x },
            if i & 2 == 0 { lo.y } else { hi.y },
            if i & 4 == 0 { lo.z } else { hi.z },
        )
    };
    let quads = [[0, 2, 3, 1], [4, 5, 7, 6], [0, 1, 5, 4], [2, 6, 7, 3], [0, 4, 6, 2], [1, 3, 7, 5]];
    quads.iter().flat_map(|q| [[c(q[0]), c(q[1]), c(q[2])], [c(q[0]), c(q[2]), c(q[3])]]).collect()
}

fn read_sprp(src: &hp_bsp::Bsp) -> Result<(Vec<String>, Vec<Prop>), String> {
    let Ok(games) = src.game_lumps() else { return Ok((Vec::new(), Vec::new())) };
    let Some(g) = games.get(SPRP) else { return Ok((Vec::new(), Vec::new())) };
    let data = g.data().map_err(|e| e.to_string())?;
    let (lump, _) = hp_bsp::staticprops::StaticPropLump::parse(&data, g.version, None).map_err(|e| e.to_string())?;
    let props = lump
        .props
        .iter()
        .map(|r| Prop {
            origin: Vec3A::from_array(r.origin),
            angles: r.angles,
            model: usize::from(r.prop_type),
            flags: r.flags,
            lighting_origin: Vec3A::from_array(r.lighting_origin),
        })
        .collect();
    Ok((lump.names, props))
}

#[derive(Default)]
struct Dict {
    studio: Option<model::Studio>,
    vtx: Option<model::Vtx>,
    collide: Vec<[[f32; 3]; 3]>,
    alpha_of_texture: Option<Vec<Option<usize>>>,
}

#[derive(Clone, Copy, Debug)]
struct AlphaTri {
    prop: u32,
    texture: usize,
    uv: [[f32; 2]; 3],
}

#[derive(Default)]
pub(crate) struct Occluders {
    alpha: Vec<AlphaTri>,
    textures: alpha::Textures,
}

impl Occluders {
    pub(crate) fn prop_of(&self, p: u32) -> u32 {
        if p & SKY_BIT != 0 || p & PROP_BIT == 0 {
            return NO_PROP;
        }
        if p & ALPHA_BIT != 0 {
            return self.alpha.get((p & INDEX_MASK) as usize).map_or(NO_PROP, |a| a.prop);
        }
        p & INDEX_MASK
    }

    fn coverage(&self, h: &Hit) -> f32 {
        let Some(a) = self.alpha.get((h.payload & INDEX_MASK) as usize) else { return 0.0 };
        let b = [1.0 - h.u - h.v, h.u, h.v];
        f32::from(self.textures.sample(a.texture, &a.uv, b)) * (1.0 / 255.0)
    }

    pub(crate) fn visible(&self, shadow: &Bvh, from: Vec3A, dir: Vec3A, dist: f32, skip: u32) -> f32 {
        if self.opaque_hit(shadow, CLASS_OPAQUE, from, dir, dist, skip, blocker_key(from + dir * dist, 1)) {
            return 0.0;
        }
        if self.alpha.is_empty() {
            return 1.0;
        }
        1.0 - self.alpha_coverage(shadow, from, dir, dist, skip)
    }

    #[allow(clippy::too_many_arguments)]
    fn opaque_hit(&self, shadow: &Bvh, mask: u8, from: Vec3A, dir: Vec3A, dist: f32, skip: u32, key: u64) -> bool {
        HINTS.with(|h| {
            let mut h = h.borrow_mut();
            let slot = &mut h[(key % HINT_SLOTS as u64) as usize];
            if slot.0 != key {
                *slot = (key, None);
            }
            if skip == NO_PROP {
                shadow.occluded_hint(mask, from, dir, dist, |_| true, &mut slot.1)
            } else {
                shadow.occluded_hint(mask, from, dir, dist, |p| self.prop_of(p) != skip, &mut slot.1)
            }
        })
    }

    #[allow(clippy::too_many_arguments)]
    fn hint_distance(&self, shadow: &Bvh, mask: u8, from: Vec3A, dir: Vec3A, skip: u32, key: u64) -> Option<f32> {
        HINTS.with(|h| {
            let h = h.borrow();
            let slot = &h[(key % HINT_SLOTS as u64) as usize];
            if slot.0 != key {
                return None;
            }
            if skip == NO_PROP {
                shadow.hint_distance(slot.1, mask, from, dir, MAX_TRACE, |_| true)
            } else {
                shadow.hint_distance(slot.1, mask, from, dir, MAX_TRACE, |p| self.prop_of(p) != skip)
            }
        })
    }

    fn alpha_coverage(&self, shadow: &Bvh, from: Vec3A, dir: Vec3A, dist: f32, skip: u32) -> f32 {
        let mut cov = 0.0f32;
        shadow.each_hit_in(CLASS_ALPHA, from, dir, dist, |h| {
            if skip == NO_PROP || self.prop_of(h.payload) != skip {
                cov = (cov + self.coverage(h)).min(1.0);
            }
            cov < 1.0
        });
        cov
    }

    pub(crate) fn sky_visible(&self, shadow: &Bvh, from: Vec3A, dir: Vec3A, skip: u32) -> f32 {
        let key = blocker_key(dir * 1024.0, 2);
        let near = self.hint_distance(shadow, CLASS_WORLD | CLASS_PROP, from, dir, skip, key);
        let bound = near.map_or(MAX_TRACE, |t| f32::from_bits(t.to_bits() + 1));
        let Some(sky) = shadow.closest_in(CLASS_SKY, from, dir, bound, |_| true).map(|h| h.t) else { return 0.0 };
        if self.opaque_hit(shadow, CLASS_WORLD | CLASS_PROP, from, dir, sky, skip, key) {
            return 0.0;
        }
        if self.alpha.is_empty() {
            return 1.0;
        }
        1.0 - self.alpha_coverage(shadow, from, dir, sky, skip)
    }

    pub(crate) fn inside_prop(&self, shadow: &Bvh, pos: Vec3A, normal: Vec3A) -> bool {
        let mut crossings: Vec<(u32, u32)> = Vec::new();
        shadow.each_hit_in(CLASS_PROP | CLASS_ALPHA, pos, normal, MAX_TRACE, |h| {
            let p = self.prop_of(h.payload);
            match crossings.iter_mut().find(|c| c.0 == p) {
                Some(c) => c.1 += 1,
                None => crossings.push((p, 1)),
            }
            true
        });
        crossings.iter().any(|c| c.1 % 2 == 1)
    }
}

#[derive(Default)]
pub(crate) struct StaticProps {
    names: Vec<String>,
    props: Vec<Prop>,
    dicts: Vec<Dict>,
}

pub(crate) struct ShadowRules<'a> {
    pub no_shadow: &'a (dyn Fn(&str) -> bool + Sync),
    pub forced: &'a (dyn Fn(&str) -> bool + Sync),
}

#[derive(Clone, Copy, Debug)]
pub(crate) struct ReadWhat {
    pub collision: bool,
    pub texture_shadows: bool,
    pub lighting: bool,
}

fn models_needed(names: usize, props: &[Prop], what: ReadWhat) -> Vec<(bool, bool)> {
    let mut needed = vec![(false, false); names];
    for p in props {
        if let Some(slot) = needed.get_mut(p.model) {
            slot.0 |= what.collision && p.flags & NO_SHADOW == 0;
            slot.1 |= what.lighting && p.flags & NO_PER_VERTEX_LIGHTING == 0;
        }
    }
    needed
}

fn read_model(
    name: &str,
    models: &dyn ModelSource,
    what: ReadWhat,
    rules: &ShadowRules<'_>,
    log: &mut Vec<String>,
) -> Dict {
    let path = name.replace('\\', "/");
    let Some(mdl) = models.read_file(&path) else {
        log.push(format!("Error! Unable to load model \"{name}\""));
        return Dict::default();
    };
    let Some(mut studio) = model::studio_header(&mdl) else {
        log.push(format!("Error! Invalid model file \"{name}\""));
        return Dict::default();
    };
    let stem =
        |p: &str| p.rsplit_once('.').filter(|(_, ext)| !ext.contains('/')).map_or(p.to_owned(), |(s, _)| s.to_owned());
    let own = format!("models/{}", studio.name.replace('\\', "/"));
    let vvd =
        models.read_file(&format!("{}.vvd", stem(&own))).or_else(|| models.read_file(&format!("{}.vvd", stem(&path))));
    let verts = vvd.as_deref().and_then(|v| model::vvd_vertices(v, studio.checksum));
    match verts {
        Some(v) => {
            if !model::load_vertices(&mut studio, &mdl, &v) {
                log.push(format!("Warning: model \"{name}\": its meshes name vertices its VVD lacks"));
            }
        }
        None => log.push(format!("Error! Unable to load vertex data \"{}.vvd\"", stem(&own))),
    }
    let phy = if what.collision { models.read_file(&format!("{}.phy", stem(&path))) } else { None };
    let collide = phy.as_deref().and_then(model::phy_first_solid).map(|c| {
        std::panic::catch_unwind(std::panic::AssertUnwindSafe(|| crate::phys::surface::collide_triangles(c)))
            .unwrap_or_default()
    });
    let collide = match collide {
        Some(t) => t,
        None if !what.collision => Vec::new(),
        None => studio
            .models
            .iter()
            .flat_map(|m| &m.meshes)
            .flat_map(|mesh| {
                let pts: Vec<[f32; 3]> = mesh.vertices.iter().map(|v| v.pos).collect();
                std::panic::catch_unwind(|| crate::phys::surface::convex_from_verts_full(&pts))
                    .ok()
                    .flatten()
                    .map(|l| crate::phys::surface::ledge_triangles(&l))
                    .unwrap_or_default()
            })
            .collect(),
    };
    let vtx_path = format!("{}.dx90.vtx", stem(&path));
    let vtx = match models.read_file(&vtx_path) {
        Some(b) => {
            let v = model::vtx(&b, studio.checksum, &studio);
            if v.is_none() {
                log.push(format!("Error! Invalid VTX file \"{vtx_path}\""));
            }
            v
        }
        None => {
            log.push(format!("Error! Unable to load file \"{vtx_path}\""));
            None
        }
    };
    let casts_texture = what.collision
        && what.texture_shadows
        && (studio.flags & model::FLAG_CAST_TEXTURE_SHADOWS != 0 || (rules.forced)(name));
    Dict { studio: Some(studio), vtx, collide, alpha_of_texture: casts_texture.then(Vec::new) }
}

impl StaticProps {
    pub(crate) fn read(
        src: &hp_bsp::Bsp,
        models: &dyn ModelSource,
        what: ReadWhat,
        rules: &ShadowRules<'_>,
    ) -> Result<(StaticProps, Vec<String>), String> {
        let (names, props) = read_sprp(src)?;
        let needed = models_needed(names.len(), &props, what);
        if let Some(n) = names.iter().zip(&needed).find(|(_, w)| w.0 || w.1).map(|(n, _)| n) {
            let _ = models.read_file(&n.replace('\\', "/"));
        }
        let mut lines: Vec<Vec<String>> = vec![Vec::new(); names.len()];
        let dicts: Vec<Dict> = names
            .par_iter()
            .zip(&needed)
            .zip(lines.par_iter_mut())
            .map(|((n, &(shadow, lit)), l)| {
                if !(shadow || lit) {
                    return Dict::default();
                }
                read_model(n, models, ReadWhat { collision: shadow, ..what }, rules, l)
            })
            .collect();
        Ok((StaticProps { names, props, dicts }, lines.into_iter().flatten().collect()))
    }

    pub(crate) fn len(&self) -> usize {
        self.props.len()
    }

    pub(crate) fn occluders(
        &mut self,
        polys: bool,
        rules: &ShadowRules<'_>,
        models: &dyn ModelSource,
        log: &mut Vec<String>,
    ) -> (Vec<Triangle>, Occluders) {
        let mut tris = Vec::new();
        let mut occ = Occluders::default();
        for d in &mut self.dicts {
            if let (Some(list), Some(s)) = (d.alpha_of_texture.as_mut(), d.studio.as_ref()) {
                *list = occ.textures.load_for_model(s, &|p| models.read_file(p), &mut |l| log.push(l.to_owned()));
            }
        }
        for (pi, prop) in self.props.iter().enumerate() {
            if prop.flags & NO_SHADOW != 0 {
                continue;
            }
            let Some(dict) = self.dicts.get(prop.model) else { continue };
            let m = angle_matrix(prop.angles);
            let xf = |p: [f32; 3]| m * Vec3A::from_array(p) + prop.origin;
            let id = PROP_BIT | (pi as u32 & INDEX_MASK);
            if !polys {
                for t in &dict.collide {
                    tris.push(Triangle { p: t.map(xf), payload: id });
                }
                if dict.collide.is_empty()
                    && let Some(s) = &dict.studio
                {
                    let (lo, hi) =
                        (Vec3A::from_array(s.hull_min) + prop.origin, Vec3A::from_array(s.hull_max) + prop.origin);
                    tris.extend(box_triangles(lo, hi).into_iter().map(|p| Triangle { p, payload: id }));
                }
                continue;
            }
            let (Some(s), Some(vtx)) = (&dict.studio, &dict.vtx) else {
                log.push(format!(
                    "Warning: static prop {pi} ({}) has no VTX: it and the props after it cast no shadow (VRAD)",
                    self.names.get(prop.model).map_or("", String::as_str)
                ));
                break;
            };
            for (mi, sub) in s.models.iter().enumerate() {
                let Some(lod0) = vtx.models.get(mi).and_then(|l| l.first()) else { continue };
                for (k, mesh) in sub.meshes.iter().enumerate() {
                    let tex_name =
                        usize::try_from(mesh.material).ok().and_then(|t| s.textures.get(t)).map_or("", String::as_str);
                    if (rules.no_shadow)(tex_name) {
                        continue;
                    }
                    let alpha_tex = dict
                        .alpha_of_texture
                        .as_ref()
                        .and_then(|l| usize::try_from(mesh.material).ok().and_then(|t| l.get(t).copied()).flatten());
                    for g in lod0.get(k).map_or(&[][..], Vec::as_slice) {
                        if g.has_strips {
                            log.push("unexpected strips found".to_owned());
                            return (tris, occ);
                        }
                        for t in &g.tris {
                            let v = t.map(|i| {
                                g.orig
                                    .get(usize::from(i))
                                    .and_then(|o| mesh.vertices.get(usize::from(*o)))
                                    .copied()
                                    .unwrap_or_default()
                            });
                            let p = [xf(v[0].pos), xf(v[2].pos), xf(v[1].pos)];
                            let mut payload = id;
                            if let Some(tex) = alpha_tex {
                                let uv = [v[0].uv, v[2].uv, v[1].uv];
                                if occ.textures.coverage_for_triangle(tex, &uv) < 1.0 {
                                    payload = PROP_BIT | ALPHA_BIT | (occ.alpha.len() as u32 & INDEX_MASK);
                                    occ.alpha.push(AlphaTri { prop: pi as u32, texture: tex, uv });
                                }
                            }
                            tris.push(Triangle { p, payload });
                        }
                    }
                }
            }
        }
        (tris, occ)
    }
}

pub(crate) fn hull_center(mdl: &[u8]) -> Option<glam::DVec3> {
    let s = model::studio_header(mdl)?;
    let c = (Vec3A::from_array(s.hull_min) + Vec3A::from_array(s.hull_max)) * 0.5;
    c.is_finite().then(|| c.as_dvec3())
}

pub(crate) struct MapPak<'a> {
    pub pak: Option<hp_bsp::PakArchive<'a>>,
    pub inner: &'a dyn ModelSource,
}

impl MapPak<'_> {
    fn packed(&self, path: &str) -> Option<Vec<u8>> {
        self.pak.as_ref()?.read_name(path).ok().flatten().map(|d| d.into_owned())
    }
}

impl ModelSource for MapPak<'_> {
    fn bounds(&self, path: &str) -> Option<(glam::DVec3, glam::DVec3)> {
        self.inner.bounds(path)
    }

    fn hull_center(&self, path: &str) -> Option<glam::DVec3> {
        match self.packed(path) {
            Some(mdl) => hull_center(&mdl),
            None => self.inner.hull_center(path),
        }
    }

    fn read_file(&self, path: &str) -> Option<Vec<u8>> {
        self.packed(path).or_else(|| self.inner.read_file(path))
    }
}

fn vertex_bytes(v: Vec3A) -> [u8; 4] {
    let c = encode(v);
    let scale = 2f32.powi(i32::from(c[3] as i8)) / 255.0;
    let ch = |m: u8| -> f32 {
        let linear = f32::from(m) * scale;
        let i = ((linear * 1024.0) as i32).clamp(0, 4095);
        (((i as f64) / 1024.0).powf(1.0 / 2.2) * 0.5).min(1.0) as f32
    };
    let mut rgb = [ch(c[0]), ch(c[1]), ch(c[2])];
    let max = rgb[0].max(rgb[1]).max(rgb[2]);
    if max > 1.0 {
        for x in &mut rgb {
            *x /= max;
        }
    }
    let byte = |x: f32| (x.max(0.0) * 255.0).round_ties_even().clamp(0.0, 255.0) as u8;
    [byte(rgb[2]), byte(rgb[1]), byte(rgb[0]), 255]
}

fn vhv_file(checksum: i32, meshes: &[(u32, Vec<Vec3A>)]) -> Vec<u8> {
    let align = |x: usize| (x + 511) & !511;
    let total: usize = meshes.iter().map(|m| m.1.len()).sum();
    let start = align(40 + meshes.len() * 28);
    let mut out = vec![0u8; align(start + total * 4)];
    let put = |out: &mut Vec<u8>, o: usize, v: u32| out[o..o + 4].copy_from_slice(&v.to_le_bytes());
    put(&mut out, 0, VHV_VERSION);
    put(&mut out, 4, checksum as u32);
    put(&mut out, 8, VERTEX_COLOR);
    put(&mut out, 12, 4);
    put(&mut out, 16, total as u32);
    put(&mut out, 20, meshes.len() as u32);
    let mut at = start;
    for (n, (lod, colors)) in meshes.iter().enumerate() {
        let h = 40 + n * 28;
        put(&mut out, h, *lod);
        put(&mut out, h + 4, colors.len() as u32);
        put(&mut out, h + 8, at as u32);
        for c in colors {
            out[at..at + 4].copy_from_slice(&vertex_bytes(*c));
            at += 4;
        }
    }
    out
}

fn row_sees(row: Option<&Vec<u8>>, cluster: i32) -> bool {
    match row {
        Some(r) if cluster >= 0 => r.get(cluster as usize >> 3).is_none_or(|b| b & (1 << (cluster & 7)) != 0),
        _ => true,
    }
}

fn samples(scale: f32) -> usize {
    usize::try_from((162.0 * scale) as i32).unwrap_or(0)
}

fn sphere_table(opts: &PropOptions) -> Vec<Vec3A> {
    let n = [FAST_SAMPLES, SUN_SAMPLES, samples(opts.sky_sample_scale), samples(opts.sample_scale)]
        .into_iter()
        .max()
        .unwrap_or(0);
    let mut sampler = Sampler::new();
    (0..n).map(|_| sampler.next()).collect()
}

#[allow(clippy::too_many_arguments)]
fn direct_at(
    ctx: &Ctx<'_>,
    (opts, sphere): (&PropOptions, &[Vec3A]),
    cand: &[usize],
    pos: Vec3A,
    normal: Vec3A,
    skip: u32,
    (ignore_normals, part): (bool, Part),
    out: &mut [Vec3A],
) {
    let cluster = ctx.map.vis_cluster_at(pos.as_dvec3());
    for &li in cand {
        let Some(l) = ctx.lights.list.get(li) else { continue };
        let ambient = l.kind == Kind::SkyAmbient;
        if (part == Part::Direct && ambient) || (part == Part::Ambient && !ambient) {
            continue;
        }
        if !row_sees(ctx.light_rows.get(li).and_then(Option::as_ref), cluster) {
            continue;
        }
        let amount = match l.kind {
            Kind::Point | Kind::Spot { .. } | Kind::Surface => {
                let to = l.origin - pos;
                if to.length() > l.reach + 4.0 {
                    continue;
                }
                let pos = pos + to.normalize_or_zero() * 4.0;
                let delta = l.origin - pos;
                let dist = delta.length();
                if dist.is_nan() || dist <= 1e-4 {
                    continue;
                }
                let dir = delta / dist;
                let dot = if ignore_normals { CONSTANT_DOT } else { dir.dot(normal).max(0.0) };
                let fall = l.falloff(dir, dist);
                if dot <= 0.0 || fall <= 0.0 {
                    continue;
                }
                let end = if l.kind == Kind::Surface { l.origin + l.normal * DIST_EPSILON } else { l.origin };
                let ray = end - pos;
                let len = ray.length();
                let vis = if len > 1e-4 { ctx.props.visible(ctx.shadow, pos, ray / len, len, skip) } else { 1.0 };
                dot * fall * vis
            }
            Kind::Sun => {
                let pos = pos - l.normal * 4.0;
                let dot = if ignore_normals { CONSTANT_DOT } else { (-normal.dot(l.normal)).max(0.0) };
                if dot <= 0.0 {
                    continue;
                }
                let n = match (ctx.lights.sun_extent > 0.0, opts.fast) {
                    (false, _) => 1,
                    (true, false) => SUN_SAMPLES,
                    (true, true) => SUN_SAMPLES / 4,
                };
                let mut seen = 0.0f32;
                for d in 0..n {
                    let mut dir = -l.normal;
                    if d > 0 {
                        dir = (dir + sphere[d - 1] * ctx.lights.sun_extent).normalize_or(dir);
                    }
                    seen += ctx.sees_sky_skip(pos, dir, skip);
                }
                dot * seen / n as f32
            }
            Kind::SkyAmbient => {
                let pos = pos + normal * 4.0;
                let n = if opts.fast { FAST_SAMPLES } else { samples(opts.sky_sample_scale) };
                let (mut sum, mut sumdot) = (0.0f32, 0.0f32);
                for &a in &sphere[..n] {
                    let dot = if ignore_normals { CONSTANT_DOT } else { -normal.dot(a) };
                    if dot <= EQUAL_EPSILON {
                        continue;
                    }
                    sumdot += dot;
                    sum += dot * ctx.sees_sky_skip(pos, -a, skip);
                }
                if sumdot <= 0.0 {
                    continue;
                }
                sum / sumdot
            }
        };
        if amount > 0.0 {
            for (p, o) in out.iter_mut().enumerate() {
                *o += l.intensity.get(p).copied().unwrap_or(Vec3A::ZERO) * amount;
            }
        }
    }
}

fn candidate_lights(ctx: &Ctx<'_>, bounds: Option<(Vec3A, Vec3A)>) -> Vec<usize> {
    let list = &ctx.lights.list;
    (0..list.len())
        .filter(|&i| {
            let l = &list[i];
            l.style == 0
                && match (l.kind, bounds) {
                    (Kind::Point | Kind::Spot { .. } | Kind::Surface, Some((lo, hi))) => {
                        l.origin.clamp(lo, hi).distance(l.origin) <= l.reach + 5.0
                    }
                    _ => true,
                }
        })
        .collect()
}

#[allow(clippy::too_many_arguments)]
fn indirect_at(
    ctx: &Ctx<'_>,
    light: &[FaceLight],
    (opts, sphere): (&PropOptions, &[Vec3A]),
    pos: Vec3A,
    normal: Vec3A,
    ignore_normals: bool,
    force_fast: bool,
    out: &mut [Vec3A],
) {
    let n = if opts.fast || force_fast { FAST_SAMPLES } else { samples(opts.sample_scale) };
    let mut total_dot = 0.0f32;
    let mut sum = vec![Vec3A::ZERO; out.len()];
    for &dir in &sphere[..n] {
        let dot = if ignore_normals { INDIRECT_CONSTANT_DOT } else { normal.dot(dir) };
        if dot <= EQUAL_EPSILON {
            continue;
        }
        total_dot += dot;
        let start = if force_fast { pos } else { pos - dir * 0.001 };
        let blocked = |t: f32| {
            t > 1.0
                && ctx.blockers.occluded(start, dir, t - 0.5, |_| true)
                && ctx.blockers.closest(start, dir, t - 0.5, |_| true).is_some_and(|h| !h.front)
        };
        let SurfHit::Face(fi, s, t, _) = ctx.trace_surface_if(start, dir, &blocked) else { continue };
        let (Some(f), Some(l)) = (ctx.faces.get(fi as usize), light.get(fi as usize)) else { continue };
        if l.styles.is_empty() {
            continue;
        }
        let x = (s as i32).clamp(0, f.w as i32 - 1) as usize;
        let y = (t as i32).clamp(0, f.h as i32 - 1) as usize;
        let lux = f.luxels();
        let weight = if force_fast { 1.0 } else { dot };
        for (p, o) in sum.iter_mut().enumerate() {
            let c = encode(finish(Vec3A::from(l.values[l.index(p, 0, 0, lux) + y * f.w + x]), f.minlight));
            let scale = super::output::pow2(i32::from(c[3] as i8));
            let v = Vec3A::new(f32::from(c[0]), f32::from(c[1]), f32::from(c[2])) * scale;
            *o += v * f.reflectivity * weight;
        }
    }
    if total_dot > 0.0 {
        for (o, s) in out.iter_mut().zip(sum) {
            *o += s / total_dot;
        }
    }
}

fn light_prop(
    ctx: &Ctx<'_>,
    light: &[FaceLight],
    sp: &StaticProps,
    pi: usize,
    (opts, sphere): (&PropOptions, &[Vec3A]),
    bounces: bool,
) -> Option<Vec<Vec<u8>>> {
    let prop = &sp.props[pi];
    if prop.flags & NO_PER_VERTEX_LIGHTING != 0 {
        return None;
    }
    let dict = sp.dicts.get(prop.model)?;
    let studio = dict.studio.as_ref()?;
    let np = ctx.passes;
    let mut files: Vec<Vec<(u32, Vec<Vec3A>)>> = vec![Vec::new(); np];
    if let Some(vtx) = &dict.vtx {
        let skip = if opts.no_self_shadowing || prop.flags & NO_SELF_SHADOWING != 0 { pi as u32 } else { NO_PROP };
        let ignore = prop.flags & IGNORE_NORMALS != 0;
        let m = angle_matrix(prop.angles);
        let head = ctx.map.models.first().map_or(-1, |m| m.head);
        let in_solid = |p: Vec3A| {
            ctx.map
                .point_leaf(head, p.as_dvec3())
                .and_then(|l| ctx.map.leafs.get(l))
                .is_some_and(|l| l.contents as u32 & crate::materials::contents::SOLID != 0)
        };
        let (mut lo, mut hi) = (Vec3A::splat(f32::INFINITY), Vec3A::splat(f32::NEG_INFINITY));
        for v in studio.models.iter().flat_map(|sub| &sub.meshes).flat_map(|mesh| &mesh.vertices) {
            let pos = m * Vec3A::from_array(v.pos) + prop.origin;
            (lo, hi) = (lo.min(pos), hi.max(pos));
        }
        let near = candidate_lights(ctx, (lo.is_finite() && hi.is_finite()).then_some((lo, hi)));
        let all = std::sync::OnceLock::new();
        for (mi, sub) in studio.models.iter().enumerate() {
            let mut verts: Vec<(Vec3A, Vec3A)> = Vec::with_capacity(sub.num_vertices.max(0) as usize);
            verts.extend(
                sub.meshes
                    .iter()
                    .flat_map(|mesh| &mesh.vertices)
                    .map(|v| (m * Vec3A::from_array(v.pos) + prop.origin, m * Vec3A::from_array(v.normal))),
            );
            let key = |(p, n): &(Vec3A, Vec3A)| [p.x, p.y, p.z, n.x, n.y, n.z].map(f32::to_bits);
            let mut first: HashMap<[u32; 6], usize> = HashMap::with_capacity(verts.len());
            let unique: Vec<usize> = verts.iter().enumerate().map(|(i, v)| *first.entry(key(v)).or_insert(i)).collect();
            let firsts: Vec<usize> = (0..verts.len()).filter(|&i| unique[i] == i).collect();
            let solid: Vec<bool> = firsts.par_iter().with_min_len(64).map(|&i| in_solid(verts[i].0)).collect();
            let mut cells: HashMap<[i32; 6], usize> = HashMap::new();
            let mut reps: Vec<usize> = Vec::new();
            let group: Vec<usize> = firsts
                .iter()
                .zip(&solid)
                .map(|(&i, &bad)| {
                    if bad {
                        return usize::MAX;
                    }
                    let (p, n) = verts[i];
                    let c = (p / SOFT_CELL).floor();
                    let q = (n * NORMAL_BINS).round();
                    let key = [c.x, c.y, c.z, q.x, q.y, q.z].map(|v| v as i32);
                    *cells.entry(key).or_insert_with(|| {
                        reps.push(i);
                        reps.len() - 1
                    })
                })
                .collect();
            let soft: Vec<(Vec<Vec3A>, Vec<Vec3A>)> = if opts.show_normals {
                Vec::new()
            } else {
                reps.par_iter()
                    .with_min_len(8)
                    .map(|&i| {
                        let (pos, normal) = verts[i];
                        let mut sky = vec![Vec3A::ZERO; np];
                        direct_at(ctx, (opts, sphere), &near, pos, normal, skip, (ignore, Part::Ambient), &mut sky);
                        let mut ind = vec![Vec3A::ZERO; np];
                        if bounces {
                            indirect_at(ctx, light, (opts, sphere), pos, normal, ignore, opts.fast, &mut ind);
                        }
                        (sky, ind)
                    })
                    .collect()
            };
            ctx.work.add(firsts.len() as u64);
            let lit_one = |(&i, &g): (&usize, &usize)| -> Option<Vec<Vec3A>> {
                let (pos, normal) = verts[i];
                if g == usize::MAX {
                    return None;
                }
                let mut c = vec![Vec3A::ZERO; np];
                if opts.show_normals {
                    c.fill((normal + Vec3A::ONE) * 50.0);
                } else {
                    direct_at(ctx, (opts, sphere), &near, pos, normal, skip, (ignore, Part::Direct), &mut c);
                    let (sky, ind) = &soft[g];
                    for ((o, s), v) in c.iter_mut().zip(sky).zip(ind) {
                        *o += *s + *v;
                    }
                }
                Some(c)
            };
            let done: Vec<Option<Vec<Vec3A>>> =
                firsts.par_iter().zip(&group).with_min_len(32).map(lit_one).inspect(|_| ctx.work.add(1)).collect();
            ctx.work.add(2 * (verts.len() - firsts.len()) as u64);
            let mut slot = vec![0usize; verts.len()];
            for (k, &i) in firsts.iter().enumerate() {
                slot[i] = k;
            }
            let mut colors: Vec<(Vec<Vec3A>, Vec3A, bool)> = Vec::with_capacity(verts.len());
            let mut bad: Vec<(usize, Vec3A, Vec3A)> = Vec::new();
            for (i, &(pos, normal)) in verts.iter().enumerate() {
                match &done[slot[unique[i]]] {
                    Some(c) => colors.push((c.clone(), pos, true)),
                    None => {
                        bad.push((colors.len(), pos, normal));
                        colors.push((vec![Vec3A::ZERO; np], pos, false));
                    }
                }
            }
            let lit = colors.len() - bad.len();
            if !bad.is_empty() && (prop.flags & USE_LIGHTING_ORIGIN != 0 || lit > 0) {
                for (k, bpos, bnormal) in &bad {
                    let mut best = if prop.flags & USE_LIGHTING_ORIGIN != 0 {
                        prop.lighting_origin
                    } else {
                        let mut found = 0;
                        let mut closest = f32::MAX;
                        for (i, c) in colors.iter().enumerate() {
                            if !c.2 {
                                continue;
                            }
                            let d = (c.1 - *bpos).length();
                            if d < closest {
                                closest = d;
                                found = i;
                            }
                        }
                        colors[found].1
                    };
                    for _ in 1..20 {
                        let mid = (best + *bpos) * 0.5;
                        if in_solid(mid) {
                            break;
                        }
                        best = mid;
                    }
                    let mut c = vec![Vec3A::ZERO; np];
                    direct_at(
                        ctx,
                        (opts, sphere),
                        all.get_or_init(|| candidate_lights(ctx, None)),
                        best,
                        *bnormal,
                        NO_PROP,
                        (false, Part::All),
                        &mut c,
                    );
                    indirect_at(ctx, light, (opts, sphere), best, *bnormal, false, true, &mut c);
                    colors[*k].0 = c;
                    colors[*k].1 = best;
                }
            }
            let lods = vtx.models.get(mi);
            for lod in 0..vtx.num_lods {
                for (k, mesh) in sub.meshes.iter().enumerate() {
                    let groups = lods.and_then(|l| l.get(lod)).and_then(|l| l.get(k)).map_or(&[][..], Vec::as_slice);
                    for g in groups {
                        for (p, file) in files.iter_mut().enumerate() {
                            let cols = g
                                .orig
                                .iter()
                                .map(|o| {
                                    usize::try_from(i64::from(mesh.vertex_offset) + i64::from(*o))
                                        .ok()
                                        .and_then(|i| colors.get(i))
                                        .map_or(Vec3A::ZERO, |c| c.0[p])
                                })
                                .collect();
                            file.push((lod as u32, cols));
                        }
                    }
                }
            }
        }
    } else {
        ctx.work.add(2 * model_vertices(studio));
    }
    Some(files.iter().map(|f| vhv_file(studio.checksum, f)).collect())
}

fn model_vertices(s: &model::Studio) -> u64 {
    s.models.iter().flat_map(|m| &m.meshes).map(|m| m.vertices.len() as u64).sum()
}

fn prop_vertices(sp: &StaticProps, pi: usize) -> u64 {
    sp.props
        .get(pi)
        .and_then(|p| sp.dicts.get(p.model))
        .and_then(|d| d.studio.as_ref())
        .map_or(1, |s| 2 * model_vertices(s).max(1))
}

pub(crate) fn light(
    ctx: &Ctx<'_>,
    light: &[FaceLight],
    sp: &StaticProps,
    opts: &PropOptions,
    bounces: bool,
    passes: &[bool],
) -> (Vec<(String, Vec<u8>)>, usize, usize) {
    let weights: Vec<u64> = (0..sp.props.len()).map(|i| prop_vertices(sp, i)).collect();
    let sphere = sphere_table(opts);
    ctx.work.begin(super::progress::Stage::StaticProps, weights.iter().sum());
    let per_prop: Vec<Option<Vec<Vec<u8>>>> = (0..sp.props.len())
        .into_par_iter()
        .map(|i| {
            let r = light_prop(ctx, light, sp, i, (opts, &sphere), bounces);
            if r.is_none() {
                ctx.work.add(weights[i]);
            }
            r
        })
        .collect();
    let mut out = Vec::new();
    for (p, &hdr) in passes.iter().enumerate() {
        for (i, files) in per_prop.iter().enumerate() {
            if let Some(f) = files.as_ref().and_then(|f| f.get(p)) {
                let name = if hdr { format!("sp_hdr_{i}.vhv") } else { format!("sp_{i}.vhv") };
                out.push((name, f.clone()));
            }
        }
    }
    let lit: Vec<usize> = per_prop.iter().enumerate().filter(|(_, f)| f.is_some()).map(|(i, _)| i).collect();
    let vertices = lit
        .iter()
        .filter_map(|&i| sp.dicts.get(sp.props[i].model).and_then(|d| d.studio.as_ref()))
        .map(|s| model_vertices(s) as usize)
        .sum();
    (out, lit.len(), vertices)
}

pub(crate) fn add_to_pak(src: &hp_bsp::Bsp, files: Vec<(String, Vec<u8>)>) -> Result<Vec<u8>, String> {
    let mut w = match src.pakfile() {
        Ok(p) => p.to_writer().map_err(|e| e.to_string())?,
        Err(_) => hp_bsp::PakWriter::new(),
    };
    for (name, data) in files {
        w.add(&name, data).map_err(|e| e.to_string())?;
    }
    w.finish_vbsp().map_err(|e| e.to_string())
}

pub(crate) fn map_flags(old: &[u8], lighting: bool, passes: &[bool]) -> Vec<u8> {
    let mut flags = old.get(0..4).map_or(0, |b| u32::from_le_bytes([b[0], b[1], b[2], b[3]]));
    if lighting {
        for &hdr in passes {
            flags |= if hdr { LVLFLAGS_BAKED_HDR } else { LVLFLAGS_BAKED_LDR };
        }
    } else {
        flags &= !(LVLFLAGS_BAKED_HDR | LVLFLAGS_BAKED_LDR);
    }
    flags.to_le_bytes().to_vec()
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn sky_only_through_sky_faces() {
        let quad = |z: f32, payload: u32| {
            let (a, b, c, d) = (
                Vec3A::new(-100.0, -100.0, z),
                Vec3A::new(100.0, -100.0, z),
                Vec3A::new(100.0, 100.0, z),
                Vec3A::new(-100.0, 100.0, z),
            );
            [Triangle { p: [a, b, c], payload }, Triangle { p: [a, c, d], payload }]
        };
        let sky_only: Vec<Triangle> = quad(500.0, SKY_BIT).to_vec();
        let bvh = Bvh::with_groups(&sky_only, shadow_class, SHADOW_GROUPS);
        let occ = Occluders::default();
        assert_eq!(occ.sky_visible(&bvh, Vec3A::ZERO, Vec3A::Z, NO_PROP), 1.0, "the sky face is seen");
        assert_eq!(occ.sky_visible(&bvh, Vec3A::ZERO, Vec3A::X, NO_PROP), 0.0, "a ray that escapes sees no sky");
        assert_eq!(occ.sky_visible(&bvh, Vec3A::ZERO, Vec3A::NEG_Z, NO_PROP), 0.0);
        let mut roofed = sky_only.clone();
        roofed.extend(quad(200.0, 0));
        let bvh = Bvh::with_groups(&roofed, shadow_class, SHADOW_GROUPS);
        assert_eq!(occ.sky_visible(&bvh, Vec3A::ZERO, Vec3A::Z, NO_PROP), 0.0, "a roof under the sky blocks it");
        assert_eq!(occ.sky_visible(&bvh, Vec3A::new(0.0, 0.0, 300.0), Vec3A::Z, NO_PROP), 1.0);
    }

    #[test]
    fn the_direction_table_repeats_the_sampler() {
        for opts in [
            PropOptions::default(),
            PropOptions { fast: true, ..PropOptions::default() },
            PropOptions { sample_scale: 4.0, sky_sample_scale: 2.0, ..PropOptions::default() },
        ] {
            let table = sphere_table(&opts);
            assert_eq!(table.len(), samples(opts.sample_scale).max(samples(opts.sky_sample_scale)).max(FAST_SAMPLES));
            let mut s = Sampler::new();
            for d in &table {
                assert_eq!(d.to_array().map(f32::to_bits), s.next().to_array().map(f32::to_bits));
            }
        }
    }

    #[test]
    fn only_models_that_shadow_or_get_lit_are_read() {
        let prop = |model: usize, flags: u32| Prop {
            origin: Vec3A::ZERO,
            angles: [0.0; 3],
            model,
            flags,
            lighting_origin: Vec3A::ZERO,
        };
        let props = [
            prop(0, NO_SHADOW),
            prop(1, 0),
            prop(2, NO_SHADOW | NO_PER_VERTEX_LIGHTING),
            prop(0, NO_SHADOW),
            prop(9, 0),
        ];
        let what = |collision: bool, lighting: bool| ReadWhat { collision, texture_shadows: false, lighting };
        assert_eq!(
            models_needed(4, &props, what(true, false)),
            vec![(false, false), (true, false), (false, false), (false, false)]
        );
        assert_eq!(
            models_needed(4, &props, what(true, true)),
            vec![(false, true), (true, true), (false, false), (false, false)]
        );
        assert_eq!(models_needed(3, &props, what(false, false)), vec![(false, false); 3]);
    }

    #[test]
    fn halton_sphere_like_vrad() {
        let mut s = Sampler::new();
        let v = s.next();
        assert!((v.z + 0.5).abs() < 1e-6);
        let r = (1.0f32 - 0.25).sqrt();
        assert!((v.x - (2.0 * std::f32::consts::PI * 2.0 / 3.0).cos() * r).abs() < 1e-5);
        let v = s.next();
        let theta = 2.0 * std::f32::consts::PI / 9.0;
        assert!((v.z - 0.5).abs() < 1e-6);
        assert!((v.y - theta.sin() * r).abs() < 1e-5 && (v.x - theta.cos() * r).abs() < 1e-5);
    }

    #[test]
    fn vertex_gamma() {
        assert_eq!(vertex_bytes(Vec3A::splat(255.0)), [128, 128, 128, 255]);
        assert_eq!(vertex_bytes(Vec3A::ZERO), [0, 0, 0, 255]);
        let b = vertex_bytes(Vec3A::new(255.0, 0.0, 0.0));
        assert_eq!((b[0], b[2]), (0, 128));
    }

    #[test]
    fn vhv_layout() {
        let f = vhv_file(7, &[(0, vec![Vec3A::ZERO; 3]), (1, vec![Vec3A::ZERO; 2])]);
        assert_eq!(f.len(), 1024);
        let u = |o: usize| u32::from_le_bytes(f[o..o + 4].try_into().unwrap());
        assert_eq!((u(0), u(4), u(8), u(12), u(16), u(20)), (2, 7, 4, 4, 5, 2));
        assert_eq!((u(40), u(44), u(48)), (0, 3, 512));
        assert_eq!((u(68), u(72), u(76)), (1, 2, 524));
    }

    #[test]
    fn flags() {
        assert_eq!(map_flags(&[4, 0, 0, 0], true, &[false, true]), vec![7, 0, 0, 0]);
        assert_eq!(map_flags(&[7, 0, 0, 0], false, &[false]), vec![4, 0, 0, 0]);
        assert_eq!(map_flags(&[], true, &[false]), vec![1, 0, 0, 0]);
    }
}
