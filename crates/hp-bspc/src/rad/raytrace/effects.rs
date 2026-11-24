use glam::Vec3A;
use hp_raytrace::kernel::Ctx as Tracer;
use hp_raytrace::scene::{LightKind, Scene as RtScene};
use rayon::prelude::*;

use super::shafts::{SHAFT_MATERIAL, Shaft, ShaftMesh, ShaftVertex, ShaftVolume};
use super::volume::{VolumeRules, build_volume};
use super::{Effects, ShaftSource};

const MAX_SUN_SHAFTS: usize = 32;
const MAX_SPOT_SHAFTS: usize = 24;
const MAX_GLOWS: usize = 32;
const MAX_VOLUMES: usize = 16;
const MAX_VOLUME_TRIANGLES: usize = 240_000;
const VOLUME_GAIN: f32 = 0.7;
const VOLUME_DIR: &str = "hammerpro/raytrace";
const FRAMED_SHARE: f32 = 0.5;
const MAX_CANDIDATES: usize = 160_000;
const CELL: f32 = 32.0;
const OPEN_DIRS: usize = 24;
const INDOOR_LIMIT: f32 = 0.12;
const OUTDOOR_LIMIT: f32 = 0.45;
const INDOOR_VOLUME: f32 = 0.3;
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
            let longest = lengths[lengths.len() - 1];
            let pad = CELL * 0.5 * (1.0 + fx.shaft_softness);
            let w1 = (hi1 - lo1) * 0.5 + pad;
            let w2 = (hi2 - lo2) * 0.5 + pad;
            if w1.min(w2) < 6.0 || length < 64.0 || length < w1.max(w2) * 0.6 {
                return None;
            }
            let mid = center + a1 * (lo1 + hi1) * 0.5 + a2 * (lo2 + hi2) * 0.5;
            let shaft = Shaft {
                origin: mid.to_array(),
                direction: (-l).to_array(),
                length: (longest + 16.0).min(max_len),
                width: w1 * 2.0,
                side: a1.to_array(),
                depth: w2 * 2.0,
                color,
                spot: false,
            };
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
            if hit.min(1024.0 * fx.shaft_length) < 48.0 {
                return None;
            }
            let length = (hit + 16.0).min(reach).min(1024.0 * fx.shaft_length);
            let half = stopdot2.clamp(-1.0, 1.0).acos().min(1.2);
            let width = 2.0 * length * half.tan() * 0.85;
            Some((
                peak,
                Shaft {
                    origin: o.to_array(),
                    direction: d.to_array(),
                    length,
                    width: width.max(8.0),
                    side: hp_raytrace::sampling::frame(d).0.to_array(),
                    depth: width.max(8.0),
                    color: color_of(light, pass, fx.shaft_light_color),
                    spot: true,
                },
            ))
        })
        .collect();
    list.sort_by(|a, b| b.0.total_cmp(&a.0));
    list.into_iter().take(MAX_SPOT_SHAFTS).map(|s| s.1).collect()
}

pub(crate) type Lit<'a> = dyn Fn(Vec3A) -> bool + Sync + 'a;

pub(crate) struct Rules<'a> {
    pub lit: &'a Lit<'a>,
    pub inside: &'a Lit<'a>,
}

pub(crate) fn framed(t: &Tracer<'_>, s: &Shaft, rules: &Rules<'_>) -> bool {
    let d = Vec3A::from_array(s.direction).normalize_or(Vec3A::NEG_Z);
    let (a1, a2) = hp_raytrace::sampling::frame(-d);
    let top = Vec3A::from_array(s.origin);
    let offsets = [
        (0.0f32, 0.0f32),
        (1.0, 0.0),
        (-1.0, 0.0),
        (0.0, 1.0),
        (0.0, -1.0),
        (2.0, 0.0),
        (-2.0, 0.0),
        (0.0, 2.0),
        (0.0, -2.0),
    ];
    let Some(p) = [16.0f32, 32.0, 48.0]
        .iter()
        .flat_map(|t| offsets.iter().map(move |(x, y)| top + d * *t + a1 * (x * 16.0) + a2 * (y * 16.0)))
        .find(|q| (rules.lit)(*q) && (rules.inside)(*q))
    else {
        return false;
    };
    let reach = 1024.0f32;
    let lit_extent = |dir: Vec3A| {
        let mut x = 0.0f32;
        while x < reach {
            let q = p + dir * (x + 8.0);
            if !(rules.lit)(q) {
                break;
            }
            x += 8.0;
        }
        x
    };
    let room = |dir: Vec3A| {
        t.closest(p, dir, reach).filter(|h| (rules.inside)(p + dir * (h.t - 4.0).max(0.0))).map_or(reach, |h| h.t)
    };
    let dirs = [a1, -a1, a2, -a2];
    let e: Vec<f32> = dirs.iter().map(|v| lit_extent(*v)).collect();
    let r: Vec<f32> = dirs.iter().map(|v| room(*v).max(8.0)).collect();
    let share = |i: usize| {
        let (plus, minus) = (r[i] < reach, r[i + 1] < reach);
        match (plus, minus) {
            (true, true) => (e[i] + e[i + 1] + 8.0) / (r[i] + r[i + 1]),
            (true, false) => (2.0 * e[i] + 8.0) / (2.0 * r[i]),
            (false, true) => (2.0 * e[i + 1] + 8.0) / (2.0 * r[i + 1]),
            (false, false) => 1.0,
        }
        .min(1.0)
    };
    r.iter().filter(|x| **x >= reach).count() <= 1 && share(0) * share(2) < FRAMED_SHARE
}

fn volume_gain(fx: &Effects, vol: &ShaftVolume) -> f32 {
    let base = if vol.spot { VOLUME_GAIN * 1.3 } else { VOLUME_GAIN };
    base * fx.shaft_intensity / (2.0 * super::volume::weight_sum())
}

pub(crate) fn shaft_mesh(t: &Tracer<'_>, scene: &RtScene, fx: &Effects, pass: usize) -> ShaftMesh {
    let fx = fx.sanitized();
    if !fx.shafts || fx.shaft_intensity <= 0.0 {
        return ShaftMesh { material: SHAFT_MATERIAL.to_owned(), ..ShaftMesh::default() };
    }
    let mut built: Vec<(Shaft, ShaftVolume)> = Vec::new();
    let mut triangles = 0usize;
    let mut take = |s: Shaft, v: Option<ShaftVolume>, built: &mut Vec<(Shaft, ShaftVolume)>| {
        if let Some(v) = v {
            let n = v.indices.len() / 3;
            if built.len() < MAX_VOLUMES && triangles + n <= MAX_VOLUME_TRIANGLES {
                triangles += n;
                built.push((s, v));
            }
        }
    };
    if fx.shaft_source == ShaftSource::SunAndSpots {
        for s in spot_shafts(t, scene, &fx, pass) {
            let o = Vec3A::from_array(s.origin);
            let lit = move |p: Vec3A| {
                let v = o - p;
                let dist = v.length();
                dist < 6.0 || t.visible(p, v / dist, dist - 4.0).max_element() > 0.0
            };
            let hit = move |o: Vec3A, d: Vec3A, max: f32| t.closest(o, d, max).map_or(max, |h| h.t);
            let rules = VolumeRules { lit: &lit, inside: &|_| true, hit: &hit, softness: fx.shaft_softness };
            let v = build_volume(&s, &rules);
            take(s, v, &mut built);
        }
    }
    if let Some(sun) = scene.lights.iter().find(|l| l.kind == LightKind::Sun) {
        let l = -Vec3A::from_array(sun.direction).normalize_or(Vec3A::NEG_Z);
        let lit = move |p: Vec3A| t.sky_visible(p, l, false).max_element() > 0.0;
        let dirs = hemisphere_dirs(OPEN_DIRS);
        let inside = |p: Vec3A| openness(t, p, &dirs[..8]) < OUTDOOR_LIMIT;
        let rules = Rules { lit: &lit, inside: &inside };
        let indoor = |p: Vec3A| openness(t, p, &dirs) < INDOOR_VOLUME;
        let hit = move |o: Vec3A, d: Vec3A, max: f32| t.closest(o, d, max).map_or(max, |h| h.t);
        let vrules = VolumeRules { lit: &lit, inside: &indoor, hit: &hit, softness: fx.shaft_softness };
        for s in sun_shafts(t, scene, sun, &fx, pass) {
            let fr = framed(t, &s, &rules);
            if fr {
                let v = build_volume(&s, &vrules);
                take(s, v, &mut built);
            }
        }
    }
    let mut vertices = Vec::new();
    let mut indices = Vec::new();
    for (_, vol) in &built {
        let k = volume_gain(&fx, vol);
        let base = vertices.len() as u32;
        let color = [vol.color[0], vol.color[1], vol.color[2], k];
        vertices.extend(vol.vertices.iter().zip(&vol.weights).zip(&vol.layers).map(|((p, w), l)| ShaftVertex {
            pos: *p,
            uv: [0.5, 0.5],
            color: [color[0], color[1], color[2], color[3] * super::volume::vertex_alpha(vol.spot, *w, *l)],
        }));
        indices.extend(vol.indices.iter().map(|i| i + base));
    }
    let (shafts, volumes) = built.into_iter().unzip();
    ShaftMesh { shafts, volumes, vertices, indices, material: SHAFT_MATERIAL.to_owned() }
}

fn volume_tag(volumes: &[ShaftVolume]) -> String {
    use std::hash::{Hash, Hasher};
    let mut h = std::collections::hash_map::DefaultHasher::new();
    for v in volumes {
        for p in &v.vertices {
            for c in p {
                c.to_bits().hash(&mut h);
            }
        }
        v.indices.len().hash(&mut h);
    }
    format!("{:08x}", h.finish() as u32)
}

pub(crate) struct VolumeModel {
    pub files: Vec<(String, Vec<u8>)>,
    pub entity: hp_bsp::Entity,
}

pub(crate) fn volume_model(vol: &ShaftVolume, name: &str, gain: f32) -> Option<VolumeModel> {
    let center = Vec3A::from_array(vol.center);
    let local = |i: u32| {
        let p = Vec3A::from_array(vol.vertices[i as usize]) - center;
        glam::DVec3::new(f64::from(p.x), f64::from(p.y), f64::from(p.z))
    };
    let triangles: Vec<hp_meshio::Triangle> = vol
        .indices
        .chunks_exact(3)
        .map(|t| {
            let pos = [local(t[0]), local(t[1]), local(t[2])];
            let n = (pos[1] - pos[0]).cross(pos[2] - pos[0]).normalize_or(glam::DVec3::Z);
            let uv = |i: u32| {
                let layer = f64::from(vol.layers[i as usize]);
                glam::DVec2::new((layer * 2.0 + 1.0) / 8.0, f64::from(vol.weights[i as usize]))
            };
            hp_meshio::Triangle { pos, normal: [n; 3], uv: [uv(t[0]), uv(t[1]), uv(t[2])], material: 0 }
        })
        .collect();
    let material = format!("{name}_mat");
    let model = hp_meshio::Model {
        triangles,
        materials: vec![hp_meshio::Material { name: material.clone(), ..hp_meshio::Material::default() }],
        up: hp_meshio::UpAxis::Z,
        meters_per_unit: 1.0,
        objects: Vec::new(),
    };
    let path = format!("{VOLUME_DIR}/{name}");
    let opts = hp_meshio::studio::StudioOptions {
        model: path.clone(),
        cdmaterials: format!("{VOLUME_DIR}/"),
        static_prop: false,
        surface_prop: "default".to_owned(),
        mass: 1.0,
        collision: hp_meshio::smd::Collision::None,
        limits: hp_meshio::collision::CollisionLimits::default(),
        own_collision: false,
        lod: hp_meshio::lod::LodOptions { enabled: false, ..hp_meshio::lod::LodOptions::default() },
    };
    let studio = hp_meshio::studio::compile(&model, std::slice::from_ref(&material), &opts).ok()?;
    let c = (Vec3A::from_array(vol.color) * gain * 255.0).clamp(Vec3A::ZERO, Vec3A::splat(255.0));
    let tex = format!("{name}_tex");
    let (w, h) = (8u32, 256u32);
    let mut pixels: Vec<u8> = Vec::with_capacity((w * h * 4) as usize);
    for y in 0..h {
        let v = (y as f32 + 0.5) / h as f32;
        for x in 0..w {
            let p = super::volume::vertex_alpha(vol.spot, v, (x / 2) as u8);
            let n = (hp_raytrace::sampling::hash3(x, y, 91) % 1000) as f32 / 1000.0 - 0.5;
            let q = |ch: f32| ((ch * p + n).round().clamp(0.0, 255.0)) as u8;
            pixels.extend_from_slice(&[q(c.x), q(c.y), q(c.z), 255]);
        }
    }
    let texture = vtf(&hp_vtf::RgbaImage { width: w, height: h, pixels })?;
    let vmt = format!(
        "\"UnlitGeneric\"\r\n{{\r\n\t\"$basetexture\" \"{VOLUME_DIR}/{tex}\"\r\n\t\"$additive\" \"1\"\r\n\t\"$nocull\" \"1\"\r\n\t\"$model\" \"1\"\r\n\t\"$nodecal\" \"1\"\r\n}}\r\n"
    );
    let files = vec![
        (format!("models/{path}.mdl"), studio.mdl),
        (format!("models/{path}.vvd"), studio.vvd),
        (format!("models/{path}.dx90.vtx"), studio.vtx),
        (format!("materials/{VOLUME_DIR}/{material}.vmt"), vmt.into_bytes()),
        (format!("materials/{VOLUME_DIR}/{tex}.vtf"), texture),
        (
            format!("materials/{VOLUME_DIR}/{name}.hpvol"),
            super::shafts::encode_volume(
                &vol.vertices,
                &vol.weights
                    .iter()
                    .zip(&vol.layers)
                    .map(|(w, l)| gain * super::volume::vertex_alpha(vol.spot, *w, *l))
                    .collect::<Vec<f32>>(),
                &vol.indices,
                [vol.color[0], vol.color[1], vol.color[2], gain],
            ),
        ),
    ];
    let entity = entity(&[
        ("classname", "prop_dynamic".to_owned()),
        ("origin", fmt3(center)),
        ("angles", "0 0 0".to_owned()),
        ("model", format!("models/{path}.mdl")),
        ("solid", "0".to_owned()),
        ("disableshadows", "1".to_owned()),
        ("disablereceiveshadows", "1".to_owned()),
        ("DisableBoneFollowers", "1".to_owned()),
        ("fademindist", "-1".to_owned()),
        ("fademaxdist", "0".to_owned()),
        ("rendermode", "0".to_owned()),
        ("renderamt", "255".to_owned()),
        ("rendercolor", "255 255 255".to_owned()),
        ("spawnflags", "0".to_owned()),
        ("PerformanceMode", "0".to_owned()),
    ]);
    Some(VolumeModel { files, entity })
}

fn volume_outputs(mesh: &ShaftMesh, fx: &Effects) -> (Vec<(String, Vec<u8>)>, Vec<hp_bsp::Entity>) {
    let tag = volume_tag(&mesh.volumes);
    let mut files = Vec::new();
    let mut ents = Vec::new();
    for (i, vol) in mesh.volumes.iter().enumerate() {
        if let Some(m) = volume_model(vol, &format!("vol_{tag}_{i}"), volume_gain(fx, vol)) {
            files.extend(m.files);
            ents.push(m.entity);
        }
    }
    (files, ents)
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

fn sprite_vmt(name: &str, orientation: &str) -> Vec<u8> {
    format!(
        "\"Sprite\"\r\n{{\r\n\t\"$spriteorientation\" \"{orientation}\"\r\n\t\"$spriteorigin\" \"[ 0.50 0.50 ]\"\r\n\t\"$basetexture\" \"hammerpro/raytrace/{name}\"\r\n\t\"$spriterendermode\" \"5\"\r\n}}\r\n"
    )
    .into_bytes()
}

fn texture_files(glow: bool) -> Vec<(String, Vec<u8>)> {
    let mut files = Vec::new();
    if glow {
        let (w, h, name) = GLOW_TEXTURE;
        let img = image(w, h, |u, v| {
            let r = ((u - 0.5).powi(2) + (v - 0.5).powi(2)).sqrt() * 2.0;
            (-(r * r) * 5.0).exp() * smooth((1.0 - r) / 0.2)
        });
        if let Some(data) = vtf(&img) {
            files.push((format!("materials/hammerpro/raytrace/{name}.vtf"), data));
            files.push((format!("materials/hammerpro/raytrace/{name}.vmt"), sprite_vmt(name, "vp_parallel")));
        }
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
        ("SetAutoExposureMin", "1.0"),
        ("SetAutoExposureMax", "2.0"),
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
    if !out.shafts.volumes.is_empty() {
        let (files, ents) = volume_outputs(&out.shafts, &fx);
        let triangles: usize = out.shafts.volumes.iter().map(|v| v.indices.len() / 3).sum();
        out.lines.push(format!(
            "ray tracing effects: {} volumetric light beams ({} from the sun, {} from spots), {} triangles in {} models, in {} ms",
            out.shafts.volumes.len(),
            out.shafts.volumes.iter().filter(|v| !v.spot).count(),
            out.shafts.volumes.iter().filter(|v| v.spot).count(),
            triangles,
            ents.len(),
            t.elapsed().as_millis()
        ));
        let places: Vec<String> = out
            .shafts
            .volumes
            .iter()
            .take(4)
            .map(|v| format!("({:.0} {:.0} {:.0})", v.center[0], v.center[1], v.center[2]))
            .collect();
        out.lines.push(format!("ray tracing effects: light beams around {}", places.join(", ")));
        out.files.extend(files);
        out.entities.extend(ents);
    } else if fx.shafts {
        out.lines.push("ray tracing effects: no framed opening lets a light beam through".to_owned());
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
                .push("ray tracing effects: env_tonemap_controller added (bloom 0.35, exposure 1.0 to 2.0)".to_owned());
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
    out.files.extend(texture_files(glow));
    out
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn lut_is_the_right_size_and_keeps_black_and_white() {
        let l = color_lut();
        assert_eq!(l.len(), 32 * 32 * 32 * 3);
        assert!(l[0] < 8 && l[1] < 8 && l[2] < 8);
        let w = l.len() - 3;
        assert!(l[w] > 240 && l[w + 1] > 240 && l[w + 2] > 230);
    }
}
