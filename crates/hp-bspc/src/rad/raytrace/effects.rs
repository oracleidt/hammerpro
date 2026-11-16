use glam::Vec3A;
use hp_raytrace::kernel::Ctx as Tracer;
use hp_raytrace::scene::{LightKind, Scene as RtScene};
use rayon::prelude::*;

use super::shafts::{SHAFT_MATERIAL, Shaft, ShaftMesh, ShaftVertex};
use super::{Effects, ShaftSource};

const MAX_SUN_SHAFTS: usize = 32;
const MAX_SPOT_SHAFTS: usize = 24;
const MAX_GLOWS: usize = 32;
const MAX_SPRITES: usize = 96;
const MAX_CANDIDATES: usize = 160_000;
const CELL: f32 = 32.0;
const OPEN_DIRS: usize = 24;
const INDOOR_LIMIT: f32 = 0.12;
const OUTDOOR_LIMIT: f32 = 0.45;
const SHAFT_TEXTURES: [(u32, u32, &str); 4] =
    [(128, 128, "shaft_1"), (64, 128, "shaft_2"), (64, 256, "shaft_4"), (32, 256, "shaft_8")];
const SPOT_TEXTURE: (u32, u32, &str) = (128, 256, "spot_cone");
const DUST_TEXTURE: (u32, u32, &str) = (128, 256, "shaft_dust");
pub(crate) const GLOW_TEXTURE: (u32, u32, &str) = (64, 64, "glow");
const LUT: &str = "materials/hammerpro/raytrace/rt_grade.raw";

#[derive(Default)]
pub(crate) struct EffectOutput {
    pub entities: Vec<hp_bsp::Entity>,
    pub files: Vec<(String, Vec<u8>)>,
    pub shafts: ShaftMesh,
    pub lines: Vec<String>,
}

fn hemisphere_dirs(n: usize) -> Vec<Vec3A> {
    (0..n)
        .map(|i| {
            let (u, v) = hp_raytrace::sampling::r2(i as u32, 11, 7, 5);
            hp_raytrace::sampling::cosine(Vec3A::Z, u * 0.9 + 0.05, v)
        })
        .collect()
}

fn openness(t: &Tracer<'_>, p: Vec3A, dirs: &[Vec3A]) -> f32 {
    let open = dirs.iter().filter(|d| t.sky_visible(p, **d, true).max_element() > 0.0).count();
    open as f32 / dirs.len().max(1) as f32
}

fn color_of(light: &hp_raytrace::Light, pass: usize, use_color: bool) -> [f32; 3] {
    if !use_color {
        return [1.0; 3];
    }
    let c = Vec3A::from_array(light.intensity[pass.min(1)]);
    let m = c.max_element();
    if m > 0.0 { (c / m).to_array() } else { [1.0; 3] }
}

struct Beam {
    top: Vec3A,
    length: f32,
}

fn sun_shafts(t: &Tracer<'_>, scene: &RtScene, sun: &hp_raytrace::Light, fx: &Effects, pass: usize) -> Vec<Shaft> {
    let l = -Vec3A::from_array(sun.direction).normalize_or(Vec3A::NEG_Z);
    if l.z < 0.05 {
        return Vec::new();
    }
    let has_cameras = !scene.sky_cameras.is_empty();
    let dirs = hemisphere_dirs(OPEN_DIRS);
    let total: usize = scene.luxels.len();
    let stride = total.div_ceil(MAX_CANDIDATES).max(1);
    let candidates: Vec<(Vec3A, Vec3A)> = scene
        .surfaces
        .iter()
        .filter(|s| !(s.in_skybox && has_cameras))
        .flat_map(|s| (0..s.luxels()).map(move |i| s.first_luxel as usize + i))
        .step_by(stride)
        .filter_map(|li| {
            let lx = scene.luxels[li];
            (lx.points > 0).then(|| {
                let p = scene.points[lx.first_point as usize];
                (Vec3A::from_array(p.pos), Vec3A::from_array(p.normals[0]))
            })
        })
        .collect();
    let max_len = 2048.0 * fx.shaft_length;
    let beams: Vec<Beam> = candidates
        .par_iter()
        .filter_map(|&(p, n)| {
            if n.dot(l) <= 0.05 {
                return None;
            }
            if t.sky_visible(p, l, false).max_element() <= 0.5 {
                return None;
            }
            if openness(t, p, &dirs) > INDOOR_LIMIT {
                return None;
            }
            let sky_t = t.closest(p, l, hp_raytrace::kernel::MAX_TRACE).map_or(max_len, |h| h.t).min(max_len);
            let mut opening = sky_t;
            let mut d = 24.0f32;
            while d < sky_t {
                if openness(t, p + l * d, &dirs[..8]) >= OUTDOOR_LIMIT {
                    opening = d;
                    break;
                }
                d += 24.0;
            }
            (opening >= 48.0).then(|| Beam { top: p + l * opening, length: opening })
        })
        .collect();
    if beams.is_empty() {
        return Vec::new();
    }
    let (a1, a2) = hp_raytrace::sampling::frame(l);
    let key = |b: &Beam| ((b.top.dot(a1) / CELL).floor() as i32, (b.top.dot(a2) / CELL).floor() as i32);
    let mut cells: std::collections::HashMap<(i32, i32), Vec<usize>> = std::collections::HashMap::new();
    for (i, b) in beams.iter().enumerate() {
        cells.entry(key(b)).or_default().push(i);
    }
    let mut seen: std::collections::HashSet<(i32, i32)> = std::collections::HashSet::new();
    let mut groups: Vec<Vec<usize>> = Vec::new();
    let mut order: Vec<(i32, i32)> = cells.keys().copied().collect();
    order.sort_unstable();
    for start in order {
        if !seen.insert(start) {
            continue;
        }
        let mut stack = vec![start];
        let mut members = Vec::new();
        while let Some(c) = stack.pop() {
            if let Some(list) = cells.get(&c) {
                members.extend_from_slice(list);
            }
            for (dx, dy) in [(1, 0), (-1, 0), (0, 1), (0, -1)] {
                let n = (c.0 + dx, c.1 + dy);
                if cells.contains_key(&n) && seen.insert(n) {
                    stack.push(n);
                }
            }
        }
        groups.push(members);
    }
    let color = color_of(sun, pass, fx.shaft_light_color);
    let mut shafts: Vec<(f32, Shaft)> = groups
        .into_iter()
        .filter_map(|g| {
            if g.len() < 3 {
                return None;
            }
            let n = g.len() as f32;
            let center = g.iter().map(|&i| beams[i].top).sum::<Vec3A>() / n;
            let (mut lo1, mut hi1, mut lo2, mut hi2) = (f32::MAX, f32::MIN, f32::MAX, f32::MIN);
            let mut lengths: Vec<f32> = Vec::with_capacity(g.len());
            for &i in &g {
                let q = beams[i].top - center;
                lo1 = lo1.min(q.dot(a1));
                hi1 = hi1.max(q.dot(a1));
                lo2 = lo2.min(q.dot(a2));
                hi2 = hi2.max(q.dot(a2));
                lengths.push(beams[i].length);
            }
            lengths.sort_by(f32::total_cmp);
            let length = lengths[lengths.len() / 2];
            let pad = CELL * 0.5 * (1.0 + fx.shaft_softness);
            let w1 = (hi1 - lo1) * 0.5 + pad;
            let w2 = (hi2 - lo2) * 0.5 + pad;
            if w1.min(w2) < 6.0 || length < 64.0 || length < w1.max(w2) * 0.6 {
                return None;
            }
            let mid = center + a1 * (lo1 + hi1) * 0.5 + a2 * (lo2 + hi2) * 0.5;
            let width = w1.max(w2) * 2.0;
            let shaft = Shaft { origin: mid.to_array(), direction: (-l).to_array(), length, width, color, spot: false };
            Some((w1 * w2 * n.sqrt(), shaft))
        })
        .collect();
    shafts.sort_by(|a, b| b.0.total_cmp(&a.0));
    shafts.into_iter().take(MAX_SUN_SHAFTS).map(|s| s.1).collect()
}

fn spot_shafts(t: &Tracer<'_>, scene: &RtScene, fx: &Effects, pass: usize) -> Vec<Shaft> {
    let mut list: Vec<(f32, Shaft)> = scene
        .lights
        .iter()
        .filter_map(|light| {
            let LightKind::Spot { stopdot2, .. } = light.kind else { return None };
            if light.style != 0 || stopdot2 < 0.3 {
                return None;
            }
            if openness(t, Vec3A::from_array(light.position), &hemisphere_dirs(OPEN_DIRS)) > INDOOR_LIMIT * 2.0 {
                return None;
            }
            let peak = Vec3A::from_array(light.intensity[pass.min(1)]).max_element();
            let reach = light.reach.min(4096.0);
            if peak <= 0.0 || reach < 96.0 {
                return None;
            }
            let o = Vec3A::from_array(light.position);
            let d = Vec3A::from_array(light.direction).normalize_or(Vec3A::NEG_Z);
            let hit = t.closest(o, d, reach).map_or(reach, |h| h.t);
            let length = (hit * 0.92).min(1024.0 * fx.shaft_length);
            if length < 48.0 {
                return None;
            }
            let half = stopdot2.clamp(-1.0, 1.0).acos().min(1.2);
            let width = 2.0 * length * half.tan() * 0.85;
            Some((
                peak,
                Shaft {
                    origin: o.to_array(),
                    direction: d.to_array(),
                    length,
                    width: width.max(8.0),
                    color: color_of(light, pass, fx.shaft_light_color),
                    spot: true,
                },
            ))
        })
        .collect();
    list.sort_by(|a, b| b.0.total_cmp(&a.0));
    list.into_iter().take(MAX_SPOT_SHAFTS).map(|s| s.1).collect()
}

fn quads(shafts: &[Shaft], intensity: f32) -> (Vec<ShaftVertex>, Vec<u32>) {
    let mut v = Vec::new();
    let mut idx = Vec::new();
    for s in shafts {
        let d = Vec3A::from_array(s.direction).normalize_or(Vec3A::NEG_Z);
        let top = Vec3A::from_array(s.origin);
        let bottom = top + d * s.length;
        let (a, b) = hp_raytrace::sampling::frame(d);
        let alpha = (0.35 * intensity).min(1.0);
        for axis in [a, b] {
            let base = v.len() as u32;
            let (wt, wb) = if s.spot { (s.width * 0.05, s.width * 0.5) } else { (s.width * 0.5, s.width * 0.5) };
            let c = [s.color[0], s.color[1], s.color[2], alpha];
            let cb = [s.color[0], s.color[1], s.color[2], 0.0];
            v.push(ShaftVertex { pos: (top - axis * wt).to_array(), uv: [0.0, 0.0], color: c });
            v.push(ShaftVertex { pos: (top + axis * wt).to_array(), uv: [1.0, 0.0], color: c });
            v.push(ShaftVertex { pos: (bottom + axis * wb).to_array(), uv: [1.0, 1.0], color: cb });
            v.push(ShaftVertex { pos: (bottom - axis * wb).to_array(), uv: [0.0, 1.0], color: cb });
            idx.extend_from_slice(&[base, base + 1, base + 2, base, base + 2, base + 3]);
        }
    }
    (v, idx)
}

pub(crate) fn shaft_mesh(t: &Tracer<'_>, scene: &RtScene, fx: &Effects, pass: usize) -> ShaftMesh {
    let fx = fx.sanitized();
    if !fx.shafts || fx.shaft_intensity <= 0.0 {
        return ShaftMesh { material: SHAFT_MATERIAL.to_owned(), ..ShaftMesh::default() };
    }
    let mut shafts = Vec::new();
    if let Some(sun) = scene.lights.iter().find(|l| l.kind == LightKind::Sun) {
        shafts.extend(sun_shafts(t, scene, sun, &fx, pass));
    }
    if fx.shaft_source == ShaftSource::SunAndSpots {
        shafts.extend(spot_shafts(t, scene, &fx, pass));
    }
    let (vertices, indices) = quads(&shafts, fx.shaft_intensity);
    ShaftMesh { shafts, vertices, indices, material: SHAFT_MATERIAL.to_owned() }
}

fn angles(forward: Vec3A, up: Vec3A) -> [f32; 3] {
    let f = forward.normalize_or(Vec3A::X);
    let pitch = -f.z.clamp(-1.0, 1.0).asin();
    let yaw = f.y.atan2(f.x);
    let (sp, cp) = pitch.sin_cos();
    let (sy, cy) = yaw.sin_cos();
    let up0 = Vec3A::new(sp * cy, sp * sy, cp);
    let right0 = Vec3A::new(sy, -cy, 0.0);
    let roll = up.dot(right0).atan2(up.dot(up0));
    [pitch.to_degrees(), yaw.to_degrees(), roll.to_degrees()]
}

fn entity(pairs: &[(&str, String)]) -> hp_bsp::Entity {
    hp_bsp::Entity { pairs: pairs.iter().map(|(k, v)| ((*k).to_owned(), v.clone())).collect() }
}

fn fmt3(v: Vec3A) -> String {
    format!("{:.1} {:.1} {:.1}", v.x, v.y, v.z)
}

fn rgb(c: [f32; 3]) -> String {
    format!("{} {} {}", (c[0] * 255.0) as u8, (c[1] * 255.0) as u8, (c[2] * 255.0) as u8)
}

fn sprite_entities(mesh: &ShaftMesh, fx: &Effects) -> Vec<hp_bsp::Entity> {
    let mut out = Vec::new();
    for s in &mesh.shafts {
        if out.len() + 8 > MAX_SPRITES {
            break;
        }
        let d = Vec3A::from_array(s.direction).normalize_or(Vec3A::NEG_Z);
        let top = Vec3A::from_array(s.origin);
        let center = top + d * (s.length * 0.5);
        let (a, b) = hp_raytrace::sampling::frame(d);
        let up = -d;
        let alpha = ((if s.spot { 70.0 } else { 90.0 }) * fx.shaft_intensity).clamp(4.0, 255.0) as u8;
        let (tex, scale, copies) = if s.spot {
            (SPOT_TEXTURE, s.length / SPOT_TEXTURE.1 as f32, 1usize)
        } else {
            let aspect = s.width / s.length;
            let best = SHAFT_TEXTURES
                .iter()
                .min_by(|x, y| {
                    let ax = (x.0 as f32 / x.1 as f32 - aspect).abs();
                    let ay = (y.0 as f32 / y.1 as f32 - aspect).abs();
                    ax.total_cmp(&ay)
                })
                .copied()
                .unwrap_or(SHAFT_TEXTURES[2]);
            let scale = s.length / best.1 as f32;
            let sprite_w = best.0 as f32 * scale;
            (best, scale, ((s.width / sprite_w).round() as usize).clamp(1, 3))
        };
        for axis in [a, b] {
            let forward = axis.cross(up).normalize_or(Vec3A::X);
            let ang = angles(forward, up);
            let sprite_w = tex.0 as f32 * scale;
            for k in 0..copies {
                let off = (k as f32 - (copies as f32 - 1.0) * 0.5) * sprite_w * 0.8;
                let pos = center + axis * off;
                out.push(entity(&[
                    ("classname", "env_sprite_oriented".to_owned()),
                    ("origin", fmt3(pos)),
                    ("angles", format!("{:.2} {:.2} {:.2}", ang[0], ang[1], ang[2])),
                    ("model", format!("materials/hammerpro/raytrace/{}.vmt", tex.2)),
                    ("scale", format!("{scale:.3}")),
                    ("rendermode", "5".to_owned()),
                    ("renderamt", alpha.to_string()),
                    ("rendercolor", rgb(s.color)),
                    ("spawnflags", "1".to_owned()),
                    ("framerate", "10".to_owned()),
                    ("HDRColorScale", "0.7".to_owned()),
                    ("disablereceiveshadows", "1".to_owned()),
                ]));
            }
            if fx.shaft_dust && !s.spot {
                out.push(entity(&[
                    ("classname", "env_sprite_oriented".to_owned()),
                    ("origin", fmt3(center)),
                    ("angles", format!("{:.2} {:.2} {:.2}", ang[0], ang[1], ang[2])),
                    ("model", format!("materials/hammerpro/raytrace/{}.vmt", DUST_TEXTURE.2)),
                    ("scale", format!("{:.3}", s.length / DUST_TEXTURE.1 as f32)),
                    ("rendermode", "5".to_owned()),
                    ("renderamt", ((alpha as f32 * 1.4).min(255.0) as u8).to_string()),
                    ("rendercolor", rgb(s.color)),
                    ("spawnflags", "1".to_owned()),
                    ("HDRColorScale", "0.7".to_owned()),
                ]));
            }
        }
    }
    out
}

pub(crate) fn glow_entities(
    scene: &RtScene,
    entities: &[hp_bsp::Entity],
    fx: &Effects,
    pass: usize,
) -> Vec<hp_bsp::Entity> {
    let mut list: Vec<(f32, &hp_raytrace::Light)> = scene
        .lights
        .iter()
        .filter(|l| matches!(l.kind, LightKind::Point | LightKind::Spot { .. }) && l.style == 0)
        .map(|l| (Vec3A::from_array(l.intensity[pass.min(1)]).max_element(), l))
        .filter(|(p, _)| *p > 0.0)
        .collect();
    list.sort_by(|a, b| b.0.total_cmp(&a.0));
    let named_at: Vec<Vec3A> = entities
        .iter()
        .filter(|e| e.classname().starts_with("light") && e.get("targetname").is_some_and(|n| !n.is_empty()))
        .filter_map(|e| e.origin().map(|o| glam::DVec3::from_array(o).as_vec3a()))
        .collect();
    list.into_iter()
        .filter(|(_, l)| !named_at.iter().any(|p| p.distance(Vec3A::from_array(l.position)) < 1.0))
        .take(MAX_GLOWS)
        .map(|(peak, l)| {
            let scale = ((peak.sqrt() / 220.0).clamp(0.15, 1.2) * fx.glow_intensity).clamp(0.05, 4.0);
            entity(&[
                ("classname", "env_sprite".to_owned()),
                ("origin", fmt3(Vec3A::from_array(l.position))),
                ("model", format!("materials/hammerpro/raytrace/{}.vmt", GLOW_TEXTURE.2)),
                ("scale", format!("{scale:.3}")),
                ("rendermode", "9".to_owned()),
                ("renderamt", "200".to_owned()),
                ("rendercolor", rgb(color_of(l, pass, true))),
                ("spawnflags", "1".to_owned()),
                ("GlowProxySize", "4".to_owned()),
                ("HDRColorScale", "0.6".to_owned()),
            ])
        })
        .collect()
}

fn image(w: u32, h: u32, f: impl Fn(f32, f32) -> f32) -> hp_vtf::RgbaImage {
    let mut pixels = Vec::with_capacity((w * h * 4) as usize);
    for y in 0..h {
        for x in 0..w {
            let u = (x as f32 + 0.5) / w as f32;
            let v = (y as f32 + 0.5) / h as f32;
            let a = f(u, v).clamp(0.0, 1.0);
            let c = (a * 255.0).round() as u8;
            pixels.extend_from_slice(&[c, c, c, 255]);
        }
    }
    hp_vtf::RgbaImage { width: w, height: h, pixels }
}

fn vtf(img: &hp_vtf::RgbaImage) -> Option<Vec<u8>> {
    let opts = hp_vtf::EncodeOptions {
        format: hp_vtf::EncodeFormat::Bgr888,
        mipmaps: true,
        flags: hp_vtf::flags::CLAMPS | hp_vtf::flags::CLAMPT,
        ..hp_vtf::EncodeOptions::default()
    };
    hp_vtf::encode(img, &opts).ok()
}

fn smooth(x: f32) -> f32 {
    let x = x.clamp(0.0, 1.0);
    x * x * (3.0 - 2.0 * x)
}

fn beam_profile(u: f32, v: f32, softness: f32) -> f32 {
    let edge = (0.5 - (u - 0.5).abs()) * 2.0;
    let side = smooth(edge / (0.35 + 0.4 * softness.min(1.5)));
    let start = smooth(v / 0.08);
    let end = smooth((1.0 - v) / 0.45);
    side * start * end * (1.0 - 0.35 * v)
}

fn sprite_vmt(name: &str, orientation: &str) -> Vec<u8> {
    format!(
        "\"Sprite\"\r\n{{\r\n\t\"$spriteorientation\" \"{orientation}\"\r\n\t\"$spriteorigin\" \"[ 0.50 0.50 ]\"\r\n\t\"$basetexture\" \"hammerpro/raytrace/{name}\"\r\n\t\"$spriterendermode\" \"5\"\r\n}}\r\n"
    )
    .into_bytes()
}

fn texture_files(fx: &Effects, shafts: bool, spots: bool, dust: bool, glow: bool) -> Vec<(String, Vec<u8>)> {
    let mut files = Vec::new();
    let mut add = |name: &str, img: hp_vtf::RgbaImage, orientation: &str| {
        if let Some(data) = vtf(&img) {
            files.push((format!("materials/hammerpro/raytrace/{name}.vtf"), data));
            files.push((format!("materials/hammerpro/raytrace/{name}.vmt"), sprite_vmt(name, orientation)));
        }
    };
    let soft = fx.shaft_softness;
    if shafts {
        for (w, h, name) in SHAFT_TEXTURES {
            add(name, image(w, h, |u, v| beam_profile(u, v, soft)), "oriented");
        }
    }
    if spots {
        let (w, h, name) = SPOT_TEXTURE;
        add(
            name,
            image(w, h, |u, v| {
                let half = 0.04 + 0.46 * v;
                let edge = (half - (u - 0.5).abs()) / half.max(1e-3);
                smooth(edge / (0.5 + 0.3 * soft.min(1.5)))
                    * smooth(v / 0.05)
                    * smooth((1.0 - v) / 0.5)
                    * (1.0 - 0.5 * v)
            }),
            "oriented",
        );
    }
    if dust {
        let (w, h, name) = DUST_TEXTURE;
        add(
            name,
            image(w, h, |u, v| {
                let x = (u * w as f32) as u32;
                let y = (v * h as f32) as u32;
                let r = hp_raytrace::sampling::hash3(x, y, 77);
                let speck = if r % 997 < 6 { 1.0 } else { 0.0 };
                speck * beam_profile(u, v, soft) * 1.6
            }),
            "oriented",
        );
    }
    if glow {
        let (w, h, name) = GLOW_TEXTURE;
        add(
            name,
            image(w, h, |u, v| {
                let r = ((u - 0.5).powi(2) + (v - 0.5).powi(2)).sqrt() * 2.0;
                (-(r * r) * 5.0).exp() * smooth((1.0 - r) / 0.2)
            }),
            "vp_parallel",
        );
    }
    files
}

fn has_class(entities: &[hp_bsp::Entity], class: &str) -> bool {
    entities.iter().any(|e| e.classname().eq_ignore_ascii_case(class))
}

fn tonemap() -> Vec<hp_bsp::Entity> {
    let controller = entity(&[
        ("classname", "env_tonemap_controller".to_owned()),
        ("targetname", "hp_rt_tonemap".to_owned()),
        ("origin", "0 0 0".to_owned()),
    ]);
    let mut auto = entity(&[
        ("classname", "logic_auto".to_owned()),
        ("origin", "0 0 0".to_owned()),
        ("spawnflags", "1".to_owned()),
    ]);
    for (input, value) in [
        ("SetBloomScale", "0.35"),
        ("SetAutoExposureMin", "0.7"),
        ("SetAutoExposureMax", "1.6"),
        ("SetTonemapRate", "0.9"),
    ] {
        auto.pairs.push(("OnMapSpawn".to_owned(), format!("hp_rt_tonemap,{input},{value},0,-1")));
    }
    vec![controller, auto]
}

fn color_lut() -> Vec<u8> {
    let n = 32usize;
    let mut out = Vec::with_capacity(n * n * n * 3);
    let curve = |x: f32| {
        let s = x * x * (3.0 - 2.0 * x);
        x + (s - x) * 0.28
    };
    for b in 0..n {
        for g in 0..n {
            for r in 0..n {
                let c = Vec3A::new(r as f32, g as f32, b as f32) / (n - 1) as f32;
                let l = c.dot(Vec3A::new(0.2126, 0.7152, 0.0722));
                let warm = Vec3A::new(1.03, 1.0, 0.96);
                let cool = Vec3A::new(0.98, 1.0, 1.03);
                let tint = cool.lerp(warm, smooth(l));
                let sat = Vec3A::splat(l).lerp(c, 1.06);
                let v = Vec3A::new(curve(sat.x), curve(sat.y), curve(sat.z)) * tint;
                out.extend_from_slice(&[
                    (v.x.clamp(0.0, 1.0) * 255.0).round() as u8,
                    (v.y.clamp(0.0, 1.0) * 255.0).round() as u8,
                    (v.z.clamp(0.0, 1.0) * 255.0).round() as u8,
                ]);
            }
        }
    }
    out
}

pub(crate) fn build(
    tracer: &Tracer<'_>,
    scene: &RtScene,
    entities: &[hp_bsp::Entity],
    fx: &Effects,
    hdr: bool,
    pass: usize,
) -> EffectOutput {
    let fx = fx.sanitized();
    let mut out = EffectOutput::default();
    let t = std::time::Instant::now();
    out.shafts = shaft_mesh(tracer, scene, &fx, pass);
    let spots = out.shafts.shafts.iter().any(|s| s.spot);
    let suns = out.shafts.shafts.iter().any(|s| !s.spot);
    if !out.shafts.shafts.is_empty() {
        out.entities.extend(sprite_entities(&out.shafts, &fx));
        out.lines.push(format!(
            "ray tracing effects: {} light shafts ({} from the sun, {} from spots) as {} oriented sprites in {} ms",
            out.shafts.shafts.len(),
            out.shafts.shafts.iter().filter(|s| !s.spot).count(),
            out.shafts.shafts.iter().filter(|s| s.spot).count(),
            out.entities.len(),
            t.elapsed().as_millis()
        ));
        let places: Vec<String> = out
            .shafts
            .shafts
            .iter()
            .take(4)
            .map(|s| {
                let mid = Vec3A::from_array(s.origin) + Vec3A::from_array(s.direction) * (s.length * 0.5);
                format!("({:.0} {:.0} {:.0})", mid.x, mid.y, mid.z)
            })
            .collect();
        out.lines.push(format!("ray tracing effects: largest light shafts around {}", places.join(", ")));
    } else if fx.shafts {
        out.lines.push("ray tracing effects: no opening lets a light shaft through".to_owned());
    }
    let glows = if fx.glow { glow_entities(scene, entities, &fx, pass) } else { Vec::new() };
    if !glows.is_empty() {
        out.lines.push(format!("ray tracing effects: {} glow halos around the strongest lights", glows.len()));
    }
    let glow = !glows.is_empty();
    out.entities.extend(glows);
    if fx.tonemap && hdr {
        if has_class(entities, "env_tonemap_controller") {
            out.lines.push(
                "ray tracing effects: the map has its own env_tonemap_controller, tonemap hints skipped".to_owned(),
            );
        } else {
            out.entities.extend(tonemap());
            out.lines
                .push("ray tracing effects: env_tonemap_controller added (bloom 0.35, exposure 0.7 to 1.6)".to_owned());
        }
    }
    if fx.color_correction {
        if has_class(entities, "color_correction") {
            out.lines.push("ray tracing effects: the map has its own color_correction, kept".to_owned());
        } else {
            out.entities.push(entity(&[
                ("classname", "color_correction".to_owned()),
                ("origin", "0 0 0".to_owned()),
                ("minfalloff", "-1".to_owned()),
                ("maxfalloff", "-1".to_owned()),
                ("maxweight", "1.0".to_owned()),
                ("filename", LUT.to_owned()),
                ("spawnflags", "1".to_owned()),
                ("fadeInDuration", "0".to_owned()),
                ("fadeOutDuration", "0".to_owned()),
            ]));
            out.files.push((LUT.to_owned(), color_lut()));
            out.lines.push("ray tracing effects: subtle color correction added".to_owned());
        }
    }
    out.files.extend(texture_files(&fx, suns, spots, suns && fx.shaft_dust, glow));
    out
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn sprite_angles_put_the_texture_up_along_the_beam() {
        let up = Vec3A::new(0.3, -0.2, 0.93).normalize();
        let (a, _) = hp_raytrace::sampling::frame(-up);
        let forward = a.cross(up).normalize();
        let [p, y, r] = angles(forward, up);
        let (sp, cp) = p.to_radians().sin_cos();
        let (sy, cy) = y.to_radians().sin_cos();
        let (sr, cr) = r.to_radians().sin_cos();
        let f = Vec3A::new(cp * cy, cp * sy, -sp);
        let u = Vec3A::new(cr * sp * cy + sr * sy, cr * sp * sy - sr * cy, cr * cp);
        assert!(f.distance(forward) < 1e-4, "{f} {forward}");
        assert!(u.distance(up) < 1e-4, "{u} {up}");
    }

    #[test]
    fn lut_is_the_right_size_and_keeps_black_and_white() {
        let l = color_lut();
        assert_eq!(l.len(), 32 * 32 * 32 * 3);
        assert!(l[0] < 8 && l[1] < 8 && l[2] < 8);
        let w = l.len() - 3;
        assert!(l[w] > 240 && l[w + 1] > 240 && l[w + 2] > 230);
    }
}
