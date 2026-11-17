use glam::Vec3A;
use rayon::prelude::*;

use super::shafts::{Shaft, ShaftVolume};

pub(crate) const LAYERS: usize = 4;
const CELL: f32 = 4.0;
const SPOT_GRID: usize = 96;
const ALONG: usize = 8;
const LAYER_WEIGHTS: [f32; LAYERS] = [0.0, 0.35, 0.75, 1.0];
const START_SEARCH: f32 = 72.0;

pub(crate) struct VolumeRules<'a> {
    pub lit: &'a (dyn Fn(Vec3A) -> bool + Sync + 'a),
    pub inside: &'a (dyn Fn(Vec3A) -> bool + Sync + 'a),
    pub hit: &'a (dyn Fn(Vec3A, Vec3A, f32) -> f32 + Sync + 'a),
    pub softness: f32,
}

fn smooth(x: f32) -> f32 {
    let x = x.clamp(0.0, 1.0);
    x * x * (3.0 - 2.0 * x)
}

pub(crate) fn layer_levels() -> [f32; LAYERS] {
    std::array::from_fn(|k| 0.12 + 0.76 * k as f32 / (LAYERS - 1) as f32)
}

pub(crate) fn layer_weight(layer: u8) -> f32 {
    LAYER_WEIGHTS[(layer as usize).min(LAYERS - 1)]
}

pub(crate) fn weight_sum() -> f32 {
    LAYER_WEIGHTS.iter().sum()
}

pub(crate) fn vertex_alpha(spot: bool, v: f32, layer: u8) -> f32 {
    layer_weight(layer) * profile(spot, v)
}

pub(crate) fn profile(spot: bool, v: f32) -> f32 {
    let v = v.clamp(0.0, 1.0);
    if spot {
        smooth(v / 0.04) * (1.0 - v).powf(1.6)
    } else {
        smooth(v / 0.12) * (1.0 - 0.45 * v) * smooth((1.0 - v) / 0.3)
    }
}

fn frame(s: &Shaft) -> (Vec3A, Vec3A, Vec3A) {
    let d = Vec3A::from_array(s.direction).normalize_or(Vec3A::NEG_Z);
    let (f1, f2) = hp_raytrace::sampling::frame(d);
    let side = Vec3A::from_array(s.side);
    let side = (side - d * side.dot(d)).normalize_or(f1);
    let across = d.cross(side).normalize_or(f2);
    (d, side, across)
}

struct Field {
    n: [usize; 2],
    lo: [f32; 2],
    step: f32,
    v: Vec<f32>,
}

impl Field {
    fn at(&self, i: usize, j: usize) -> f32 {
        self.v[j * self.n[0] + i]
    }
    fn pos(&self, i: f32, j: f32) -> [f32; 2] {
        [self.lo[0] + i * self.step, self.lo[1] + j * self.step]
    }
}

fn blur2(f: &mut Field, passes: usize) {
    for _ in 0..passes {
        for axis in 0..2 {
            let src = f.v.clone();
            for j in 0..f.n[1] {
                for i in 0..f.n[0] {
                    let mut sum = src[j * f.n[0] + i] * 2.0;
                    let mut w = 2.0;
                    for step in [-1i64, 1] {
                        let (a, b) = if axis == 0 { (i as i64 + step, j as i64) } else { (i as i64, j as i64 + step) };
                        if a < 0 || b < 0 || a >= f.n[0] as i64 || b >= f.n[1] as i64 {
                            continue;
                        }
                        sum += src[b as usize * f.n[0] + a as usize];
                        w += 1.0;
                    }
                    f.v[j * f.n[0] + i] = sum / w;
                }
            }
        }
    }
}

fn contours(f: &Field, level: f32) -> Vec<([f32; 2], [f32; 2])> {
    let mut out = Vec::new();
    let lerp = |a: (f32, f32, f32), b: (f32, f32, f32)| {
        let t = ((level - a.2) / (b.2 - a.2)).clamp(0.0, 1.0);
        f.pos(a.0 + (b.0 - a.0) * t, a.1 + (b.1 - a.1) * t)
    };
    for j in 0..f.n[1] - 1 {
        for i in 0..f.n[0] - 1 {
            let c = [
                (i as f32, j as f32, f.at(i, j)),
                (i as f32 + 1.0, j as f32, f.at(i + 1, j)),
                (i as f32 + 1.0, j as f32 + 1.0, f.at(i + 1, j + 1)),
                (i as f32, j as f32 + 1.0, f.at(i, j + 1)),
            ];
            let mut pts = Vec::with_capacity(4);
            for e in 0..4 {
                let (a, b) = (c[e], c[(e + 1) % 4]);
                if (a.2 >= level) != (b.2 >= level) {
                    pts.push(lerp(a, b));
                }
            }
            if pts.len() == 2 {
                out.push((pts[0], pts[1]));
            } else if pts.len() == 4 {
                out.push((pts[0], pts[1]));
                out.push((pts[2], pts[3]));
            }
        }
    }
    out
}

struct Ray {
    start: Vec3A,
    dir: Vec3A,
    length: f32,
}

fn sun_ray(s: &Shaft, p: [f32; 2], rules: &VolumeRules<'_>) -> Option<Ray> {
    let (d, side, across) = frame(s);
    let base = Vec3A::from_array(s.origin) + side * p[0] + across * p[1];
    let mut t = 0.0f32;
    while !((rules.lit)(base + d * t) && (rules.inside)(base + d * t)) {
        t += 4.0;
        if t > START_SEARCH {
            return None;
        }
    }
    let start = base + d * t;
    let length = ((rules.hit)(start, d, s.length.max(64.0)) - 2.0).max(0.0);
    (length > 16.0).then_some(Ray { start, dir: d, length })
}

fn spot_ray(s: &Shaft, p: [f32; 2], rules: &VolumeRules<'_>) -> Option<Ray> {
    let (d, side, across) = frame(s);
    let dir = (d + side * p[0] + across * p[1]).normalize_or(d);
    let o = Vec3A::from_array(s.origin) + dir * 6.0;
    let length = ((rules.hit)(o, dir, s.length.max(64.0)) - 2.0).max(0.0);
    (length > 16.0).then_some(Ray { start: o, dir, length })
}

fn sun_field(s: &Shaft, rules: &VolumeRules<'_>) -> Field {
    let (d, side, across) = frame(s);
    let (hu, hv) = (s.width * 0.5 + 24.0, s.depth * 0.5 + 24.0);
    let n = [((2.0 * hu / CELL) as usize + 1).clamp(4, 160), ((2.0 * hv / CELL) as usize + 1).clamp(4, 160)];
    let top = Vec3A::from_array(s.origin);
    let mut v = vec![0.0f32; n[0] * n[1]];
    v.par_chunks_mut(n[0]).enumerate().for_each(|(j, row)| {
        for (i, cell) in row.iter_mut().enumerate() {
            let base = top + side * (-hu + i as f32 * CELL) + across * (-hv + j as f32 * CELL);
            let mut t = 0.0f32;
            while t <= START_SEARCH {
                let p = base + d * t;
                if (rules.lit)(p) && (rules.inside)(p) {
                    *cell = 1.0;
                    break;
                }
                t += 8.0;
            }
        }
    });
    let mut f = Field { n, lo: [-hu, -hv], step: CELL, v };
    let passes = (2.0 + 1.5 * rules.softness).round().clamp(2.0, 8.0) as usize;
    blur2(&mut f, passes);
    f
}

fn spot_field(s: &Shaft, softness: f32) -> Field {
    let half = (s.width * 0.5 / s.length.max(1.0)).atan().min(1.2);
    let r = half.tan() * 1.15;
    let n = [SPOT_GRID, SPOT_GRID];
    let step = 2.0 * r / (SPOT_GRID - 1) as f32;
    let mut v = vec![0.0f32; n[0] * n[1]];
    let band = (0.45 + 0.1 * softness).clamp(0.3, 0.9);
    for j in 0..n[1] {
        for i in 0..n[0] {
            let x = -r + i as f32 * step;
            let y = -r + j as f32 * step;
            let q = (x * x + y * y).sqrt() / r;
            v[j * n[0] + i] = smooth((1.0 - q) / band);
        }
    }
    Field { n, lo: [-r, -r], step, v }
}

fn key(p: [f32; 2]) -> (i64, i64) {
    ((p[0] * 4096.0).round() as i64, (p[1] * 4096.0).round() as i64)
}

pub(crate) fn build_volume(s: &Shaft, rules: &VolumeRules<'_>) -> Option<ShaftVolume> {
    let field = if s.spot { spot_field(s, rules.softness) } else { sun_field(s, rules) };
    let peak = field.v.iter().copied().fold(0.0f32, f32::max);
    if peak < 0.5 {
        return None;
    }
    let mut vertices: Vec<[f32; 3]> = Vec::new();
    let mut weights: Vec<f32> = Vec::new();
    let mut layer_of: Vec<u8> = Vec::new();
    let mut indices: Vec<u32> = Vec::new();
    let mut layers = 0usize;
    for (layer, level) in layer_levels().into_iter().enumerate() {
        if layer_weight(layer as u8) <= 0.0 {
            continue;
        }
        let segs = contours(&field, level * peak);
        if segs.is_empty() {
            continue;
        }
        let mut points: Vec<[f32; 2]> = Vec::new();
        let mut index: std::collections::HashMap<(i64, i64), usize> = std::collections::HashMap::new();
        let mut pairs: Vec<(usize, usize)> = Vec::new();
        for (a, b) in segs {
            let mut id = |p: [f32; 2]| {
                *index.entry(key(p)).or_insert_with(|| {
                    points.push(p);
                    points.len() - 1
                })
            };
            let (ia, ib) = (id(a), id(b));
            if ia != ib {
                pairs.push((ia, ib));
            }
        }
        let rays: Vec<Option<Ray>> =
            points.par_iter().map(|p| if s.spot { spot_ray(s, *p, rules) } else { sun_ray(s, *p, rules) }).collect();
        let mut emitted = false;
        for (a, b) in pairs {
            let (Some(ra), Some(rb)) = (&rays[a], &rays[b]) else { continue };
            for step in 0..ALONG {
                let (v0, v1) = (step as f32 / ALONG as f32, (step + 1) as f32 / ALONG as f32);
                let base = vertices.len() as u32;
                for (r, v) in [(ra, v0), (rb, v0), (rb, v1), (ra, v1)] {
                    vertices.push((r.start + r.dir * (r.length * v)).to_array());
                    weights.push(v);
                    layer_of.push(layer as u8);
                }
                indices.extend_from_slice(&[base, base + 1, base + 2, base, base + 2, base + 3]);
            }
            emitted = true;
        }
        if emitted {
            layers += 1;
        }
    }
    if indices.is_empty() {
        return None;
    }
    let lo = vertices.iter().fold(Vec3A::splat(f32::MAX), |a, b| a.min(Vec3A::from_array(*b)));
    let hi = vertices.iter().fold(Vec3A::splat(f32::MIN), |a, b| a.max(Vec3A::from_array(*b)));
    Some(ShaftVolume {
        center: ((lo + hi) * 0.5).to_array(),
        vertices,
        weights,
        layers: layer_of,
        indices,
        shells: layers.max(1),
        color: s.color,
        spot: s.spot,
    })
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::rad::tests::{block, compile_vmf};
    use glam::DVec3;

    struct Box3 {
        lo: Vec3A,
        hi: Vec3A,
    }

    fn ray_hits(boxes: &[Box3], o: Vec3A, d: Vec3A) -> bool {
        boxes.iter().any(|b| {
            let inv = d.recip();
            let t1 = (b.lo - o) * inv;
            let t2 = (b.hi - o) * inv;
            let near = t1.min(t2).max_element();
            let far = t1.max(t2).min_element();
            far >= near.max(0.0)
        })
    }

    fn inside_any(boxes: &[Box3], p: Vec3A) -> bool {
        boxes.iter().any(|b| p.cmpgt(b.lo).all() && p.cmplt(b.hi).all())
    }

    fn sky_shell(id: &mut u32) -> String {
        let v = DVec3::new;
        let sky = "TOOLS/TOOLSSKYBOX";
        let mut world = String::new();
        for (lo, hi) in [
            (v(-512.0, -512.0, -32.0), v(1024.0, 1024.0, -16.0)),
            (v(-512.0, -512.0, 1024.0), v(1024.0, 1024.0, 1040.0)),
            (v(-528.0, -512.0, -32.0), v(-512.0, 1024.0, 1040.0)),
            (v(1024.0, -512.0, -32.0), v(1040.0, 1024.0, 1040.0)),
            (v(-528.0, -528.0, -32.0), v(1040.0, -512.0, 1040.0)),
            (v(-528.0, 1024.0, -32.0), v(1040.0, 1040.0, 1040.0)),
        ] {
            world.push_str(&block(id, lo, hi, sky, sky));
        }
        world
    }

    fn map(world: &str, pitch: f32, yaw: f32) -> String {
        format!(
            "versioninfo\n{{\n\t\"editorversion\" \"400\"\n\t\"mapversion\" \"1\"\n}}\nworld\n{{\n\t\"id\" \"1\"\n\t\"mapversion\" \"1\"\n\t\"classname\" \"worldspawn\"\n\t\"skyname\" \"sky_day01_01\"\n{world}}}\n\
             entity\n{{\n\t\"id\" \"9000\"\n\t\"classname\" \"info_player_start\"\n\t\"origin\" \"100 100 16\"\n}}\n\
             entity\n{{\n\t\"id\" \"9001\"\n\t\"classname\" \"light_environment\"\n\t\"origin\" \"100 100 128\"\n\t\"_light\" \"255 240 220 400\"\n\t\"_ambient\" \"130 150 180 60\"\n\t\"pitch\" \"{pitch}\"\n\t\"angles\" \"0 {yaw} 0\"\n\t\"SunSpreadAngle\" \"2\"\n}}\n"
        )
    }

    fn two_floors() -> (String, Vec<Box3>) {
        let v = DVec3::new;
        let w = "DEV/DEV_MEASUREGENERIC01B";
        let solids: Vec<(DVec3, DVec3)> = vec![
            (v(-512.0, -512.0, -16.0), v(1024.0, 1024.0, 0.0)),
            (v(0.0, 0.0, 0.0), v(16.0, 512.0, 544.0)),
            (v(16.0, 0.0, 0.0), v(496.0, 16.0, 544.0)),
            (v(16.0, 496.0, 0.0), v(496.0, 512.0, 544.0)),
            (v(496.0, 0.0, 0.0), v(512.0, 512.0, 352.0)),
            (v(496.0, 0.0, 352.0), v(512.0, 176.0, 528.0)),
            (v(496.0, 336.0, 352.0), v(512.0, 512.0, 528.0)),
            (v(496.0, 0.0, 528.0), v(512.0, 512.0, 544.0)),
            (v(0.0, 0.0, 544.0), v(512.0, 512.0, 560.0)),
            (v(352.0, 16.0, 256.0), v(496.0, 496.0, 272.0)),
            (v(496.0, 248.0, 352.0), v(512.0, 264.0, 528.0)),
        ];
        let mut id = 1u32;
        let mut world = sky_shell(&mut id);
        for (lo, hi) in &solids {
            world.push_str(&block(&mut id, *lo, *hi, w, w));
        }
        let boxes = solids.iter().map(|(lo, hi)| Box3 { lo: lo.as_vec3a() + 0.5, hi: hi.as_vec3a() - 0.5 }).collect();
        (map(&world, -55.0, 200.0), boxes)
    }

    fn cpu_shafts(bsp: &[u8]) -> super::super::ShaftMesh {
        let rt = super::super::RayTracing {
            device: super::super::DevicePreference::Cpu,
            min_samples: Some(1),
            max_samples: Some(1),
            ..super::super::RayTracing::with_quality(super::super::Quality::Draft)
        };
        let opts = crate::rad::RadOptions { raytracing: Some(rt), ..crate::rad::RadOptions::default() };
        let ctl = crate::rad::session::Control { cancelled: &|| false, progress: &|_, _| {}, partial: None };
        let p = super::super::Progressive::new(bsp.to_vec(), &opts, &crate::props::NoModels, &mut |_| {}, &ctl)
            .expect("progressive");
        p.light_shafts()
    }

    fn sun_dir(pitch: f32, yaw: f32) -> Vec3A {
        let (sp, cp) = pitch.to_radians().sin_cos();
        let (sy, cy) = yaw.to_radians().sin_cos();
        -Vec3A::new(cp * cy, cp * sy, -sp)
    }

    #[test]
    fn no_shell_vertex_lies_inside_solid_or_outside_the_lit_volume() {
        let (text, boxes) = two_floors();
        let bsp = compile_vmf(&text);
        let m = cpu_shafts(&bsp);
        let suns: Vec<&ShaftVolume> = m.volumes.iter().filter(|v| !v.spot).collect();
        assert!(!suns.is_empty(), "the framed upper window makes a light beam");
        let l = sun_dir(55.0, 200.0);
        let mut checked = 0;
        for vol in &suns {
            assert!(vol.shells >= 1 && !vol.indices.is_empty());
            for p in &vol.vertices {
                let q = Vec3A::from_array(*p);
                checked += 1;
                assert!(!inside_any(&boxes, q), "shell vertex inside solid at {q}");
                assert!(!ray_hits(&boxes, q + l * 0.25, l), "shell vertex out of the sunlight at {q}");
                assert!(q.x < 520.0, "shell vertex outdoors at {q}");
            }
        }
        assert!(checked > 100, "{checked}");
        let below: usize = suns.iter().flat_map(|v| v.vertices.iter()).filter(|p| p[2] < 256.0 && p[0] > 352.0).count();
        assert_eq!(below, 0, "no beam under the mezzanine slab");
    }

    #[test]
    fn a_room_with_an_open_front_gets_no_sun_volume() {
        let v = DVec3::new;
        let w = "DEV/DEV_MEASUREGENERIC01B";
        let mut id = 1u32;
        let mut world = sky_shell(&mut id);
        for (lo, hi) in [
            (v(-512.0, -512.0, -16.0), v(1024.0, 1024.0, 0.0)),
            (v(0.0, 0.0, 0.0), v(16.0, 512.0, 256.0)),
            (v(16.0, 0.0, 0.0), v(512.0, 16.0, 256.0)),
            (v(16.0, 496.0, 0.0), v(512.0, 512.0, 256.0)),
            (v(0.0, 0.0, 256.0), v(512.0, 512.0, 272.0)),
        ] {
            world.push_str(&block(&mut id, lo, hi, w, w));
        }
        let bsp = compile_vmf(&map(&world, -40.0, 190.0));
        let m = cpu_shafts(&bsp);
        assert!(m.volumes.iter().all(|v| v.spot), "the cutaway room draws no sun beam");
    }

    #[test]
    fn a_volume_exports_as_a_readable_model() {
        let (text, _) = two_floors();
        let bsp = compile_vmf(&text);
        let m = cpu_shafts(&bsp);
        let vol = m.volumes.first().expect("a volume");
        let out = super::super::effects::volume_model(vol, "vol_test_0", 0.02).expect("model");
        let get = |suffix: &str| out.files.iter().find(|(n, _)| n.ends_with(suffix)).map(|(_, d)| d.clone()).unwrap();
        let meshes = hp_mdl::raw_mesh_positions(&get(".mdl"), &get(".vvd")).expect("readable");
        let n: usize = meshes.iter().map(Vec::len).sum();
        assert!(n > 0);
        assert_eq!(out.entity.get("classname"), Some("prop_dynamic"));
        assert!(out.files.iter().any(|(n, _)| n.ends_with(".dx90.vtx")));
        assert!(
            out.files.iter().any(|(n, d)| n.ends_with("_mat.vmt") && String::from_utf8_lossy(d).contains("$additive"))
        );
    }

    #[test]
    fn player_view_data_round_trips() {
        let v = [[1.0f32, 2.0, 3.0], [4.0, 5.0, 6.0], [7.0, 8.0, 9.0]];
        let data = super::super::shafts::encode_volume(&v, &[0.0, 0.5, 1.0], &[0, 1, 2], [1.0, 0.8, 0.6, 0.02]);
        let (vv, a, i, c) = super::super::shafts::decode_volume(&data).expect("decoded");
        assert_eq!(
            (vv.as_slice(), a.as_slice(), i.as_slice(), c),
            (&v[..], &[0.0f32, 0.5, 1.0][..], &[0u32, 1, 2][..], [1.0, 0.8, 0.6, 0.02])
        );
        assert!(super::super::shafts::decode_volume(&data[..data.len() - 2]).is_none());
        assert_eq!(layer_weight(0), 0.0);
    }

    #[test]
    fn levels_grow_and_profiles_fade() {
        let l = layer_levels();
        assert!(l.windows(2).all(|w| w[1] > w[0]) && l[0] > 0.0 && l[LAYERS - 1] < 1.0);
        for spot in [false, true] {
            assert!(profile(spot, 0.0) < 0.01 && profile(spot, 1.0) < 0.01 && profile(spot, 0.3) > 0.3);
        }
    }
}
