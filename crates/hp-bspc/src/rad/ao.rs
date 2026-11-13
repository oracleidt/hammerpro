use ahash::AHashMap as HashMap;
use glam::Vec3A;
use rayon::prelude::*;

use super::RadQuality;
use super::bake::{Ctx, FaceLight, basis, cosine_dir_in, hash};
use super::data::MapData;
use super::props::{CLASS_PROP, CLASS_WORLD, NO_PROP};
use super::surface::LitFace;

pub const DEFAULT_RADIUS: f32 = 32.0;
pub const DEFAULT_SCALE: f32 = 0.5;
pub const MAX_SAMPLES: u32 = 1024;
pub const DEBUG_WHITE: f32 = 128.0;
const DEBUG_GAMMA: f32 = 2.2;
const PUSH: f32 = 1.0;
const CELL: f32 = 32.0;
const BLUR_LUXELS: f32 = 1.5;
const INWARD: f64 = 0.125;
const BLUR_NORMAL_DOT: f32 = 0.8;

#[derive(Clone, Debug, PartialEq)]
pub struct AoOptions {
    pub radius: f32,
    pub scale: f32,
    pub face_samples: Option<u32>,
    pub prop_samples: Option<u32>,
    pub debug: bool,
}

impl Default for AoOptions {
    fn default() -> Self {
        AoOptions { radius: DEFAULT_RADIUS, scale: DEFAULT_SCALE, face_samples: None, prop_samples: None, debug: false }
    }
}

pub fn face_samples(q: RadQuality) -> usize {
    match q {
        RadQuality::Draft => 16,
        RadQuality::Normal => 32,
        RadQuality::Final => 64,
    }
}

pub fn prop_samples(q: RadQuality) -> usize {
    match q {
        RadQuality::Draft => 16,
        RadQuality::Normal => 32,
        RadQuality::Final => 64,
    }
}

pub fn factor(term: f32, scale: f32) -> f32 {
    (1.0 - scale.clamp(0.0, 1.0) * (1.0 - term.clamp(0.0, 1.0))).clamp(0.0, 1.0)
}

pub fn debug_value(term: f32) -> f32 {
    term.clamp(0.0, 1.0).powf(DEBUG_GAMMA) * DEBUG_WHITE
}

pub fn falloff(t: f32, radius: f32) -> f32 {
    if radius <= 0.0 {
        return 0.0;
    }
    let x = (t / radius).clamp(0.0, 1.0);
    1.0 - x * x
}

#[derive(Clone, Debug, PartialEq)]
pub(crate) struct Ao {
    pub radius: f32,
    pub scale: f32,
    pub face_samples: usize,
    pub prop_samples: usize,
    pub debug: bool,
    sky: Vec<f32>,
}

impl Ao {
    pub fn new(o: &AoOptions, q: RadQuality, map: &MapData) -> Ao {
        let count = |n: Option<u32>, auto: usize| n.map_or(auto, |n| n.clamp(1, MAX_SAMPLES) as usize);
        Ao {
            radius: if o.radius.is_finite() && o.radius > 0.0 { o.radius } else { DEFAULT_RADIUS },
            scale: if o.scale.is_finite() { o.scale.clamp(0.0, 1.0) } else { DEFAULT_SCALE },
            face_samples: count(o.face_samples, face_samples(q)),
            prop_samples: count(o.prop_samples, prop_samples(q)),
            debug: o.debug,
            sky: sky_scales(map),
        }
    }

    pub fn sky_areas(&self) -> usize {
        self.sky.iter().filter(|s| **s != 1.0).count()
    }

    pub fn radius_at(&self, map: &MapData, p: Vec3A) -> f32 {
        let scale = map.area_at(p.as_dvec3()).and_then(|a| self.sky.get(a)).copied().unwrap_or(1.0);
        self.radius * scale
    }

    pub fn factor(&self, term: f32) -> f32 {
        factor(term, self.scale)
    }

    pub fn shade(&self, term: f32, c: &mut [Vec3A]) {
        if self.debug {
            c.fill(Vec3A::splat(debug_value(term)));
        } else {
            let k = self.factor(term);
            for v in c {
                *v *= k;
            }
        }
    }
}

pub(crate) fn sky_scales(map: &MapData) -> Vec<f32> {
    let mut out: Vec<f32> = Vec::new();
    for e in &map.entities {
        if !e.classname().eq_ignore_ascii_case("sky_camera") {
            continue;
        }
        let Some(o) = e.origin() else { continue };
        let scale = e.get("scale").and_then(|s| s.trim().parse::<f32>().ok()).unwrap_or(0.0);
        if !(scale > 0.0 && scale.is_finite()) {
            continue;
        }
        let Some(a) = map.area_at(glam::DVec3::from_array(o)) else { continue };
        if out.len() <= a {
            out.resize(a + 1, 1.0);
        }
        out[a] = 1.0 / scale;
    }
    out
}

fn seed_of(pos: Vec3A, normal: Vec3A) -> u32 {
    let q = (pos * 4.0).round();
    let n = (normal * 64.0).round();
    hash(q.x as i32 as u32, q.y as i32 as u32, q.z as i32 as u32)
        ^ hash(n.x as i32 as u32, n.y as i32 as u32, n.z as i32 as u32).rotate_left(13)
}

impl Ctx<'_> {
    pub(crate) fn ao_term(&self, pos: Vec3A, normal: Vec3A, radius: f32, samples: usize, skip: u32) -> f32 {
        let n = normal.normalize_or(Vec3A::Z);
        if samples == 0 || radius <= 0.0 {
            return 1.0;
        }
        let (t, b) = basis(n);
        let frame = [t, b, n];
        let seed = seed_of(pos, n);
        let keep = |p: u32| skip == NO_PROP || self.props.prop_of(p) != skip;
        let mut occluded = 0.0;
        for k in 0..samples {
            let d = cosine_dir_in(&frame, k, samples, seed);
            if let Some(h) = self.shadow.closest_in(CLASS_WORLD | CLASS_PROP, pos, d, radius, keep) {
                occluded += falloff(h.t, radius);
            }
        }
        (1.0 - occluded / samples as f32).clamp(0.0, 1.0)
    }

    pub(crate) fn ao_vertex(&self, pos: Vec3A, normal: Vec3A, radius: f32, skip: u32) -> Option<f32> {
        let ao = self.ao.as_ref()?;
        let n = normal.normalize_or(Vec3A::Z);
        Some(self.ao_term(pos + n * PUSH, n, radius, ao.prop_samples, skip))
    }
}

struct Point {
    pos: Vec3A,
    normal: Vec3A,
    raw: f32,
    reach: f32,
}

fn luxel_size(f: &LitFace) -> f32 {
    let v = glam::DVec3::new(f.lm[0][0], f.lm[0][1], f.lm[0][2]).length();
    if v > 0.0 { (1.0 / v) as f32 } else { 16.0 }
}

pub(crate) fn face_terms(ctx: &Ctx<'_>, ao: &Ao, faces: &[LitFace]) -> Vec<Vec<f32>> {
    let raw: Vec<Vec<Option<Point>>> = faces
        .par_iter()
        .map(|f| {
            let samples = ctx.luxel_samples(f).samples;
            let size = luxel_size(f);
            let reach = (size * BLUR_LUXELS).clamp(1.0, 4.0 * CELL);
            let center = samples.iter().flatten().next().map(|s| s.pos);
            let radius = center.map_or(ao.radius, |c| ao.radius_at(ctx.map, c));
            let flat = f.normal.as_vec3a();
            let middle = f.poly.iter().copied().sum::<glam::DVec3>() / f.poly.len().max(1) as f64 + f.normal;
            let out = samples
                .iter()
                .map(|s| {
                    let s = s.as_ref()?;
                    let (normal, from) = if f.disp.is_some() {
                        (s.normals[0], s.pos)
                    } else {
                        let p = s.pos.as_dvec3();
                        let inward = (middle - p).reject_from(f.normal).normalize_or_zero();
                        (flat, (p + inward * INWARD).as_vec3a())
                    };
                    let raw = ctx.ao_term(from, normal, radius, ao.face_samples, NO_PROP);
                    Some(Point { pos: s.pos, normal: normal.normalize_or(flat), raw, reach })
                })
                .collect();
            ctx.work.add(f.luxels() as u64);
            out
        })
        .collect();
    smooth(&raw)
}

fn cell_of(p: Vec3A) -> (i32, i32, i32) {
    let c = (p / CELL).floor();
    (c.x as i32, c.y as i32, c.z as i32)
}

fn smooth(raw: &[Vec<Option<Point>>]) -> Vec<Vec<f32>> {
    let mut grid: HashMap<(i32, i32, i32), Vec<(u32, u32)>> = HashMap::new();
    for (fi, pts) in raw.iter().enumerate() {
        for (li, p) in pts.iter().enumerate() {
            if let Some(p) = p {
                grid.entry(cell_of(p.pos)).or_default().push((fi as u32, li as u32));
            }
        }
    }
    raw.par_iter()
        .map(|pts| {
            pts.iter()
                .map(|p| {
                    let Some(p) = p else { return 1.0 };
                    let (lo, hi) = (cell_of(p.pos - Vec3A::splat(p.reach)), cell_of(p.pos + Vec3A::splat(p.reach)));
                    let (mut sum, mut weight) = (0.0f32, 0.0f32);
                    for x in lo.0..=hi.0 {
                        for y in lo.1..=hi.1 {
                            for z in lo.2..=hi.2 {
                                let Some(list) = grid.get(&(x, y, z)) else { continue };
                                for &(fi, li) in list {
                                    let Some(q) = &raw[fi as usize][li as usize] else { continue };
                                    if q.normal.dot(p.normal) < BLUR_NORMAL_DOT {
                                        continue;
                                    }
                                    let w = 1.0 - q.pos.distance(p.pos) / p.reach;
                                    if w > 0.0 {
                                        sum += q.raw * w;
                                        weight += w;
                                    }
                                }
                            }
                        }
                    }
                    if weight > 0.0 { sum / weight } else { p.raw }
                })
                .collect()
        })
        .collect()
}

pub(crate) fn apply(faces: &[LitFace], light: &mut [FaceLight], terms: &[Vec<f32>], ao: &Ao) {
    light.par_iter_mut().zip(faces).zip(terms).for_each(|((l, f), term)| {
        let lux = f.luxels();
        if term.len() != lux || lux == 0 {
            return;
        }
        let planes = l.values.len() / lux;
        let per_style = l.normals.max(1);
        let styles = l.styles.len().max(1);
        for plane in 0..planes {
            let slot = (plane / per_style) % styles;
            let values = &mut l.values[plane * lux..(plane + 1) * lux];
            for (v, t) in values.iter_mut().zip(term) {
                *v = if ao.debug {
                    if slot == 0 { glam::Vec3::splat(debug_value(*t)) } else { glam::Vec3::ZERO }
                } else {
                    *v * ao.factor(*t)
                };
            }
        }
    });
}

pub(crate) fn summary(ao: &Ao, ms: u128) -> String {
    let sky = match ao.sky_areas() {
        0 => String::new(),
        n => format!(", radius scaled in {n} 3D sky area(s)"),
    };
    format!(
        "ambient occlusion: radius {} units, scale {}, {} rays per luxel, {} rays per prop vertex{sky}{} in {ms} ms",
        ao.radius,
        ao.scale,
        ao.face_samples,
        ao.prop_samples,
        if ao.debug { ", debug: lightmaps show only the occlusion (-aodebug)" } else { "" }
    )
}

#[cfg(test)]
mod tests {
    use glam::DVec3;

    use super::super::tests::{compile_vmf, light_entity, quick, room, sealed_rooms, style_luxel_at};
    use super::super::{RadOptions, light_bsp};
    use super::*;

    fn floor(bsp: &[u8], x: f64, y: f64) -> f32 {
        style_luxel_at(bsp, DVec3::new(x, y, 0.0), DVec3::Z, 0).expect("floor luxel").max_element()
    }

    fn lit_room(ao: Option<AoOptions>, origin: &str) -> Vec<u8> {
        let bsp = compile_vmf(&room("DEV/DEV_MEASUREGENERIC01B", &light_entity(500, origin, "")));
        let opts = RadOptions { hdr: false, ldr: true, ao, ..quick() };
        light_bsp(&bsp, &opts, &mut |_| {}).expect("light")
    }

    #[test]
    fn the_formula_blends_toward_one_with_the_scale() {
        assert_eq!(factor(1.0, 0.5), 1.0);
        assert_eq!(factor(0.0, 0.5), 0.5);
        assert_eq!(factor(0.0, 1.0), 0.0);
        assert_eq!(factor(0.4, 0.0), 1.0);
        assert!((factor(0.6, 0.5) - 0.8).abs() < 1e-6);
        assert_eq!(factor(0.0, 3.0), 0.0);
        assert_eq!(debug_value(1.0), DEBUG_WHITE);
        assert!((debug_value(0.5) - DEBUG_WHITE * 0.5f32.powf(2.2)).abs() < 1e-3);
        assert_eq!(debug_value(-1.0), 0.0);
        assert_eq!(falloff(0.0, 32.0), 1.0);
        assert_eq!(falloff(16.0, 32.0), 0.75);
        assert_eq!(falloff(40.0, 32.0), 0.0);
        let o = AoOptions::default();
        assert_eq!((o.radius, o.scale, o.debug), (32.0, 0.5, false));
        assert!(RadOptions::default().ao.is_none(), "ambient occlusion is opt in");
    }

    #[test]
    fn sample_counts_follow_fast_and_final_unless_given() {
        let map = MapData::default();
        let of = |o: &AoOptions, q| {
            let a = Ao::new(o, q, &map);
            (a.face_samples, a.prop_samples)
        };
        let auto = AoOptions::default();
        assert_eq!(of(&auto, RadQuality::Draft), (16, 16));
        assert_eq!(of(&auto, RadQuality::Normal), (32, 32));
        assert_eq!(of(&auto, RadQuality::Final), (64, 64));
        let given = AoOptions { face_samples: Some(20), prop_samples: Some(5000), ..auto.clone() };
        assert_eq!(of(&given, RadQuality::Draft), (20, MAX_SAMPLES as usize));
        assert_eq!(of(&given, RadQuality::Final), (20, MAX_SAMPLES as usize));
        let bad = AoOptions { radius: -1.0, scale: f32::NAN, ..auto };
        let a = Ao::new(&bad, RadQuality::Normal, &map);
        assert_eq!((a.radius, a.scale), (DEFAULT_RADIUS, DEFAULT_SCALE));
    }

    #[test]
    fn corners_get_darker_than_the_open_floor() {
        let plain = lit_room(None, "64 128 128");
        let with = lit_room(Some(AoOptions::default()), "64 128 128");
        let (corner_plain, corner_ao) = (floor(&plain, 2.0, 128.0), floor(&with, 2.0, 128.0));
        let (open_plain, open_ao) = (floor(&plain, 128.0, 128.0), floor(&with, 128.0, 128.0));
        assert!(corner_plain > 0.0 && open_plain > 0.0);
        let corner = corner_ao / corner_plain;
        let open = open_ao / open_plain;
        assert!((0.5..0.93).contains(&corner), "the corner gets darker: {corner}");
        assert!((open - 1.0).abs() < 0.02, "the open floor keeps its light: {open}");
        let pillar = floor(&with, 236.0, 128.0) / floor(&plain, 236.0, 128.0);
        assert!(pillar < 0.93, "the foot of the pillar gets darker: {pillar}");
    }

    #[test]
    fn debug_lightmaps_hold_only_the_occlusion_term() {
        let debug = AoOptions { debug: true, ..AoOptions::default() };
        let dim = lit_room(Some(debug.clone()), "64 128 128");
        let bright = lit_room(Some(debug), "400 128 200");
        let open = floor(&dim, 128.0, 128.0);
        assert!((open - DEBUG_WHITE).abs() < DEBUG_WHITE * 0.02, "open floor is white: {open}");
        let corner = floor(&dim, 2.0, 128.0);
        assert!(corner < open * 0.9 && corner > 0.0, "the corner is darker: {corner}");
        for (x, y) in [(128.0, 128.0), (2.0, 128.0), (236.0, 100.0), (400.0, 30.0)] {
            assert_eq!(floor(&dim, x, y), floor(&bright, x, y), "the light plays no part at {x} {y}");
        }
    }

    #[test]
    fn the_radius_is_scaled_in_the_3d_skybox() {
        let camera = "entity\n{\n\t\"id\" \"700\"\n\t\"classname\" \"sky_camera\"\n\t\"origin\" \"728 128 128\"\n\t\"scale\" \"16\"\n}\n";
        let text = sealed_rooms(&[("", ""), ("", "")]) + camera;
        let bsp = compile_vmf(&text);
        let src = hp_bsp::Bsp::from_vec(bsp.clone()).expect("bsp");
        let map = MapData::read(&src).expect("map");
        let ao = Ao::new(&AoOptions::default(), RadQuality::Normal, &map);
        assert_eq!(ao.sky_areas(), 1);
        assert_eq!(ao.radius_at(&map, Vec3A::new(128.0, 128.0, 64.0)), 32.0);
        assert_eq!(ao.radius_at(&map, Vec3A::new(728.0, 128.0, 64.0)), 2.0);
        let debug = RadOptions {
            hdr: false,
            ldr: true,
            ao: Some(AoOptions { debug: true, ..AoOptions::default() }),
            ..quick()
        };
        let mut lines = Vec::new();
        let lit = light_bsp(&bsp, &debug, &mut |l| lines.push(l.to_owned())).expect("light");
        assert!(lines.iter().any(|l| l.contains("radius scaled in 1 3D sky area")), "{lines:?}");
        let main = floor(&lit, 16.0, 128.0);
        let sky = floor(&lit, 616.0, 128.0);
        assert!(main < DEBUG_WHITE * 0.95, "16 units from a wall of the map is occluded: {main}");
        assert!(sky > DEBUG_WHITE * 0.99, "16 skybox units are 256 map units, out of reach: {sky}");
    }

    #[test]
    fn the_stage_shows_in_the_plan_and_the_log() {
        let bsp = compile_vmf(&room("DEV/DEV_MEASUREGENERIC01B", &light_entity(500, "64 128 128", "")));
        let mut lines = Vec::new();
        let opts = RadOptions { ao: Some(AoOptions::default()), ..quick() };
        light_bsp(&bsp, &opts, &mut |l| lines.push(l.to_owned())).expect("light");
        let plan = lines.iter().find(|l| l.starts_with("Lighting stages: ")).expect("plan");
        assert_eq!(
            plan,
            "Lighting stages: direct lighting, bounce rays, bounced light, ambient occlusion, leaf ambient, writing"
        );
        assert!(
            lines.iter().any(|l| l.starts_with("ambient occlusion: radius 32 units, scale 0.5, 16 rays per luxel"))
        );
        let phases = lines.iter().find(|l| l.starts_with("Light phase times: ")).expect("phases");
        assert!(phases.contains("ambient occlusion "), "{phases}");
    }

    #[test]
    fn faces_split_by_a_pillar_keep_one_value_on_shared_luxels() {
        super::super::tests::split_floor_without_seams(Some(AoOptions::default()));
    }

    #[test]
    fn results_do_not_depend_on_the_thread_count() {
        let bsp = compile_vmf(&room("DEV/DEV_MEASUREGENERIC01B", &light_entity(500, "64 128 128", "")));
        let run = |threads| {
            let opts = RadOptions { threads: Some(threads), ao: Some(AoOptions::default()), ..quick() };
            light_bsp(&bsp, &opts, &mut |_| {}).expect("light")
        };
        assert!(run(1) == run(3));
    }
}
