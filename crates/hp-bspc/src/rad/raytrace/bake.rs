use glam::{DVec2, DVec3, Vec3A};
use hp_light::bvh::Triangle as LightTri;
use hp_raytrace::scene::{self as rts, MAX_SLOTS, NONE};
use rayon::prelude::*;

use super::super::bake::{Ctx, FaceLight};
use super::super::data::MapData;
use super::super::lights::Kind;
use super::super::props::Occluders;
use super::super::surface::{self, LitFace, inside};
use super::{Quality, RayTracing};
use crate::materials::surf;

pub(crate) const DEFAULT_LIGHT_SIZE: f32 = 2.0;
const PROP_ALBEDO: f32 = 0.35;
const BOUNCE_STYLE_REACH: f32 = 1.5;

pub(crate) struct Built {
    pub scene: rts::Scene,
    pub face_normals: Vec<usize>,
    pub exposure: f32,
}

pub(crate) struct MaterialInfo {
    pub alpha: Option<rts::AlphaTexture>,
    pub translucent: bool,
    pub selfillum: Option<[f32; 3]>,
}

pub(crate) fn world_materials(src: &hp_bsp::Bsp, read: &(dyn Fn(&str) -> Option<Vec<u8>> + Sync)) -> Vec<MaterialInfo> {
    let names = super::super::texlights::texdata(src);
    let Some((first, rest)) = names.split_first() else { return Vec::new() };
    let mut out = vec![material_info(&first.0, read)];
    out.par_extend(rest.par_iter().map(|(name, _, _)| material_info(name, read)));
    out
}

fn material_info(name: &str, read: &dyn Fn(&str) -> Option<Vec<u8>>) -> MaterialInfo {
    let none = MaterialInfo { alpha: None, translucent: false, selfillum: None };
    let mut path = format!("materials/{}.vmt", name.replace('\\', "/").to_ascii_lowercase());
    for _ in 0..3 {
        let Some(bytes) = read(&path) else { return none };
        let Ok(root) = hp_kv::parse_bytes(&bytes) else { return none };
        let Some(vmt) = root.children.first() else { return none };
        if vmt.name.eq_ignore_ascii_case("patch") {
            match vmt.get("include") {
                Some(inc) => {
                    path = inc.replace('\\', "/").to_ascii_lowercase();
                    continue;
                }
                None => return none,
            }
        }
        let flag = |k: &str| {
            vmt.get(k).or_else(|| vmt.children.iter().find_map(|c| c.get(k))).is_some_and(|v| v.trim() != "0")
        };
        let alphatest = flag("$alphatest");
        let translucent = flag("$translucent");
        let selfillum = flag("$selfillum");
        if !(alphatest || translucent || selfillum) {
            return none;
        }
        let image = vmt.get("$basetexture").and_then(|base| {
            let vtf = read(&format!("materials/{}.vtf", base.replace('\\', "/").to_ascii_lowercase()))?;
            hp_vtf::raw_level(&vtf, 0, 0, 0).ok()?.to_rgba8().ok()
        });
        let glow = selfillum.then(|| {
            let tint = vmt
                .get("$selfillumtint")
                .map(|t| {
                    let v: Vec<f32> = t
                        .trim_matches(|c| c == '[' || c == ']' || c == '{' || c == '}')
                        .split_whitespace()
                        .filter_map(|x| x.parse().ok())
                        .collect();
                    if v.len() >= 3 { [v[0], v[1], v[2]] } else { [1.0; 3] }
                })
                .unwrap_or([1.0; 3]);
            let avg = image.as_ref().map_or([0.5; 3], |img| {
                let mut sum = [0.0f64; 3];
                let n = (img.pixels.len() / 4).max(1) as f64;
                for p in img.pixels.chunks_exact(4) {
                    let a = f64::from(p[3]) / 255.0;
                    for k in 0..3 {
                        sum[k] += (f64::from(p[k]) / 255.0).powf(2.2) * a;
                    }
                }
                sum.map(|v| (v / n) as f32)
            });
            [avg[0] * tint[0], avg[1] * tint[1], avg[2] * tint[2]]
        });
        let alpha = if alphatest || translucent {
            image.as_ref().map(|img| rts::AlphaTexture {
                width: img.width,
                height: img.height,
                alpha: img.pixels.chunks_exact(4).map(|p| p[3]).collect(),
            })
        } else {
            None
        };
        return MaterialInfo { alpha, translucent, selfillum: glow.filter(|g| g.iter().any(|c| *c > 0.004)) };
    }
    none
}

fn tri(p: [DVec3; 3]) -> [[f32; 3]; 3] {
    p.map(|v| v.as_vec3a().to_array())
}

fn oriented(
    mut p: [DVec3; 3],
    mut uv: [[f32; 2]; 3],
    mut lm: [[f32; 2]; 3],
    n: DVec3,
) -> ([DVec3; 3], [[f32; 2]; 3], [[f32; 2]; 3]) {
    if (p[1] - p[0]).cross(p[2] - p[0]).dot(n) < 0.0 {
        p.swap(1, 2);
        uv.swap(1, 2);
        lm.swap(1, 2);
    }
    (p, uv, lm)
}

fn tex_uv(map: &MapData, texinfo: i32, p: DVec3, size: (i32, i32)) -> [f32; 2] {
    let Some(t) = usize::try_from(texinfo).ok().and_then(|i| map.texinfo.get(i)) else { return [0.0; 2] };
    let c = |k: usize| {
        let v = t.tex[k];
        (p.x * f64::from(v[0]) + p.y * f64::from(v[1]) + p.z * f64::from(v[2]) + f64::from(v[3])) as f32
    };
    [c(0) / size.0.max(1) as f32, c(1) / size.1.max(1) as f32]
}

#[allow(clippy::too_many_arguments)]
pub(crate) fn build(
    ctx: &Ctx<'_>,
    faces: &[LitFace],
    prop_tris: &[LightTri],
    occluders: &Occluders,
    materials: &[MaterialInfo],
    texdata_sizes: &[(i32, i32)],
    rt: &RayTracing,
    style_bounce: bool,
    passes: usize,
) -> Built {
    let map = ctx.map;
    let params = rt.params();
    let mut scene = rts::Scene { passes: passes as u32, ..rts::Scene::default() };
    let mut mat_index: std::collections::HashMap<(u32, i32), u32> = std::collections::HashMap::new();
    let push_mat = |scene: &mut rts::Scene, m: rts::Material| -> u32 {
        scene.materials.push(m);
        (scene.materials.len() - 1) as u32
    };
    let blocker = push_mat(
        &mut scene,
        rts::Material { kind: rts::MaterialKind::Blocker, albedo: [0.0; 3], ..Default::default() },
    );
    let sky =
        push_mat(&mut scene, rts::Material { kind: rts::MaterialKind::Sky, albedo: [0.0; 3], ..Default::default() });
    let prop = push_mat(&mut scene, rts::Material { albedo: [PROP_ALBEDO; 3], two_sided: true, ..Default::default() });
    let mut alpha_of_tex: Vec<Option<u32>> = vec![None; materials.len()];
    for (i, m) in materials.iter().enumerate() {
        if let Some(a) = &m.alpha {
            scene.alpha_textures.push(a.clone());
            alpha_of_tex[i] = Some((scene.alpha_textures.len() - 1) as u32);
        }
    }
    let add = |scene: &mut rts::Scene,
               p: [[f32; 3]; 3],
               uv: [[f32; 2]; 3],
               lm: [[f32; 2]; 3],
               material: u32,
               surface: u32| {
        scene.triangles.push(rts::Triangle { p, uv, lm, material, surface });
    };
    let mut blockers = Vec::new();
    surface::world_brush_triangles(map, false, &mut blockers);
    surface::entity_brush_triangles(map, &mut blockers);
    for t in &blockers {
        add(&mut scene, t.p.map(|v| v.to_array()), [[0.0; 2]; 3], [[0.0; 2]; 3], blocker, NONE);
    }
    let lit_index = super::super::bake::lit_index(map.faces.len(), faces.iter());
    let clear = surface::see_through_faces(map);
    if let Some(m) = map.models.first() {
        for fi in m.first_face..m.first_face.saturating_add(m.num_faces).min(map.faces.len()) {
            let face = &map.faces[fi];
            let flags = map.face_flags(face);
            if flags & surf::NODRAW != 0 {
                continue;
            }
            let pts = map.face_points(face);
            if flags & surf::SKY != 0 {
                for k in 2..pts.len() {
                    add(&mut scene, tri([pts[0], pts[k - 1], pts[k]]), [[0.0; 2]; 3], [[0.0; 2]; 3], sky, NONE);
                }
                continue;
            }
            if flags & surf::WARP != 0 {
                continue;
            }
            let td = usize::try_from(face.texinfo)
                .ok()
                .and_then(|t| map.texinfo.get(t))
                .and_then(|t| usize::try_from(t.texdata).ok());
            let li = lit_index.get(fi).copied().flatten();
            let lf = li.and_then(|i| faces.get(i as usize));
            let refl = match lf {
                Some(f) => f.reflectivity.to_array(),
                None => td.and_then(|t| map.reflectivity.get(t)).copied().unwrap_or([0.0; 3]),
            };
            let refl = if params.cinematic { richer_bounce(refl) } else { refl };
            let see_through = clear.get(fi).copied().unwrap_or(false);
            let minfo = td.and_then(|t| materials.get(t));
            let translucent = see_through || minfo.is_some_and(|m| m.translucent && m.alpha.is_none());
            let key =
                (td.map_or(u32::MAX, |t| t as u32), if translucent { 1 } else { 0 } + if lf.is_some() { 2 } else { 0 });
            let material = *mat_index.entry(key).or_insert_with(|| {
                let alpha = td.and_then(|t| alpha_of_tex.get(t).copied().flatten()).unwrap_or(NONE);
                let peak = refl[0].max(refl[1]).max(refl[2]).max(1e-3);
                let m = if translucent {
                    rts::Material {
                        albedo: refl,
                        kind: rts::MaterialKind::Translucent,
                        alpha: if minfo.is_some_and(|m| m.translucent) { alpha } else { NONE },
                        transmit: refl.map(|c| 0.35 + 0.65 * (c / peak).clamp(0.0, 1.0)),
                        ..Default::default()
                    }
                } else {
                    rts::Material { albedo: refl, alpha, ..Default::default() }
                };
                scene.materials.push(m);
                (scene.materials.len() - 1) as u32
            });
            let size = td.and_then(|t| texdata_sizes.get(t)).copied().unwrap_or((256, 256));
            match lf {
                Some(f) if f.disp.is_some() => {
                    let d = f.disp.as_ref().expect("disp");
                    let up = f.normal;
                    for (t, uv) in d.triangles() {
                        let lm = uv.map(|c| [c.x as f32 * (f.w as f32 - 1.0), c.y as f32 * (f.h as f32 - 1.0)]);
                        let tuv = t.map(|p| tex_uv(map, face.texinfo, p, size));
                        let (p, tuv, lm) = oriented(t, tuv, lm, up);
                        add(&mut scene, tri(p), tuv, lm, material, li.unwrap_or(NONE));
                    }
                }
                _ => {
                    let n = map.face_plane(face).map_or(DVec3::Z, |p| p.normal);
                    for k in 2..pts.len() {
                        let t = [pts[0], pts[k - 1], pts[k]];
                        let lm = match lf {
                            Some(f) => t.map(|p| {
                                let c = f.luxel_coords(p);
                                [c.x as f32, c.y as f32]
                            }),
                            None => [[0.0; 2]; 3],
                        };
                        let tuv = t.map(|p| tex_uv(map, face.texinfo, p, size));
                        let (p, tuv, lm) = oriented(t, tuv, lm, n);
                        add(&mut scene, tri(p), tuv, lm, material, li.unwrap_or(NONE));
                    }
                }
            }
        }
    }
    let mut prop_alpha: std::collections::HashMap<usize, u32> = std::collections::HashMap::new();
    for t in prop_tris {
        if t.payload & surface::SKY_BIT != 0 {
            continue;
        }
        let (material, uv) = match occluders.alpha_of(t.payload) {
            Some((tex, uv)) => {
                let m = *prop_alpha.entry(tex).or_insert_with(|| {
                    let Some(a) = occluders.alpha_texture(tex) else { return prop };
                    scene.alpha_textures.push(a);
                    scene.materials.push(rts::Material {
                        albedo: [PROP_ALBEDO; 3],
                        alpha: (scene.alpha_textures.len() - 1) as u32,
                        two_sided: true,
                        ..Default::default()
                    });
                    (scene.materials.len() - 1) as u32
                });
                (m, uv)
            }
            None => (prop, [[0.0; 2]; 3]),
        };
        add(&mut scene, t.p.map(|v| v.to_array()), uv, [[0.0; 2]; 3], material, NONE);
    }
    let pairs: Vec<(rts::Light, rts::Light)> = ctx
        .lights
        .list
        .par_iter()
        .map(|l| {
            let mut e = engine_light(l, ctx.lights.sun_extent, &params);
            let original = e;
            let fixture = params.cinematic && cinematic_light(&mut e, ctx.shadow);
            fit_light_size(&mut e, ctx.shadow, params.light_size_scale, fixture);
            (original, e)
        })
        .collect();
    let (originals, lights): (Vec<rts::Light>, Vec<rts::Light>) = pairs.into_iter().unzip();
    scene.lights = lights;
    scene.sky_cameras =
        ctx.cameras.iter().map(|(o, inv)| rts::SkyCamera { origin: o.to_array(), inv_scale: *inv }).collect();
    let total_luxels: usize = faces.iter().map(LitFace::luxels).sum();
    let asked = if rt.quality == Quality::Optimal && rt.supersample.is_none() {
        optimal_supersample(total_luxels)
    } else {
        params.supersample
    };
    let supersample = supersample_for(asked, total_luxels);
    let per_face: Vec<FaceJob> =
        faces.par_iter().map(|f| face_job(ctx, f, supersample, rt.bump, style_bounce)).collect();
    let mut face_normals = Vec::with_capacity(faces.len());
    for job in per_face {
        let first_luxel = scene.luxels.len() as u32;
        let first_light = scene.light_lists.len() as u32;
        scene.light_lists.extend_from_slice(&job.lights);
        for (start, count) in job.luxels {
            scene.luxels.push(rts::Luxel { first_point: scene.points.len() as u32 + start, points: count });
        }
        scene.points.extend(job.points);
        face_normals.push(job.out_normals);
        scene.surfaces.push(rts::Surface {
            width: job.w,
            height: job.h,
            normals: job.normals,
            styles: job.styles,
            first_luxel,
            lights: (first_light, job.lights.len() as u32),
            in_skybox: job.in_skybox,
            minlight: job.minlight,
        });
    }
    let exposure = if params.cinematic {
        let first = exposure_match(&mut scene, &originals);
        knee_lights(&mut scene.lights, &originals);
        let second = exposure_match(&mut scene, &originals);
        knee_lights(&mut scene.lights, &originals);
        first * second
    } else {
        1.0
    };
    Built { scene, face_normals, exposure }
}

const MATCH_LUXELS: usize = 16;

pub(crate) fn exposure_match(scene: &mut rts::Scene, originals: &[rts::Light]) -> f32 {
    let changed: Vec<bool> = scene
        .lights
        .iter()
        .zip(originals)
        .map(|(a, b)| a.constant != b.constant || a.quadratic != b.quadratic || a.kind != b.kind)
        .collect();
    if !changed.iter().any(|c| *c) {
        return 1.0;
    }
    let lum = |i: [f32; 3]| 0.2126 * i[0] + 0.7152 * i[1] + 0.0722 * i[2];
    let reach = |l: &rts::Light, pos: Vec3A, n: Vec3A| -> f32 {
        let to = Vec3A::from_array(l.position) - pos;
        let dist = to.length();
        if dist.is_nan() || dist <= 1e-3 || dist > l.reach {
            return 0.0;
        }
        let dir = to / dist;
        let c = dir.dot(n);
        if c <= 0.0 { 0.0 } else { lum(l.intensity[0]) * l.falloff(dir, dist) * c }
    };
    let scene_ref = &*scene;
    let mut ratios: Vec<f32> = scene_ref
        .surfaces
        .par_iter()
        .filter_map(|s| {
            let lux = s.luxels();
            if lux == 0 || s.lights.1 == 0 {
                return None;
            }
            let step = lux.div_ceil(MATCH_LUXELS).max(1);
            let (mut now, mut before) = (0.0f64, 0.0f64);
            for i in (0..lux).step_by(step) {
                let l = &scene_ref.luxels[s.first_luxel as usize + i];
                if l.points == 0 {
                    continue;
                }
                let p = &scene_ref.points[l.first_point as usize];
                let (pos, n) = (Vec3A::from_array(p.pos), Vec3A::from_array(p.normals[0]));
                for j in 0..s.lights.1 {
                    let li = scene_ref.light_lists[(s.lights.0 + j) as usize] as usize;
                    if !changed[li] {
                        continue;
                    }
                    now += f64::from(reach(&scene_ref.lights[li], pos, n));
                    before += f64::from(reach(&originals[li], pos, n));
                }
            }
            (before > 1e-3).then(|| (now / before) as f32)
        })
        .collect();
    if ratios.is_empty() {
        return 1.0;
    }
    let mid = ratios.len() / 2;
    ratios.select_nth_unstable_by(mid, f32::total_cmp);
    let k = (1.0 / ratios[mid].max(1e-3)).clamp(0.67, 1.35);
    for (l, c) in scene.lights.iter_mut().zip(&changed) {
        if *c {
            for i in &mut l.intensity {
                for v in i.iter_mut() {
                    *v *= k;
                }
            }
        }
    }
    k
}

pub(crate) const MAX_SAMPLE_POINTS: usize = 24_000_000;

pub(crate) fn optimal_supersample(luxels: usize) -> u32 {
    if luxels <= 1_500_000 {
        3
    } else if luxels <= 5_000_000 {
        2
    } else {
        1
    }
}

pub(crate) fn supersample_for(asked: u32, luxels: usize) -> usize {
    let mut ss = asked.max(1) as usize;
    while ss > 1 && luxels.saturating_mul(ss * ss) > MAX_SAMPLE_POINTS {
        ss -= 1;
    }
    ss
}

pub(crate) fn engine_light(
    l: &super::super::lights::Light,
    sun_extent: f32,
    params: &hp_raytrace::Params,
) -> rts::Light {
    let scale = params.light_size_scale;
    let mut intensity = [[0.0f32; 3]; 2];
    for (p, i) in l.intensity.iter().enumerate().take(2) {
        intensity[p] = i.to_array();
    }
    let (kind, size) = match l.kind {
        Kind::Point => (rts::LightKind::Point, params.light_radius * scale),
        Kind::Spot { stopdot, stopdot2, exponent } => {
            (rts::LightKind::Spot { stopdot, stopdot2, exponent }, params.light_radius * scale)
        }
        Kind::Surface => (rts::LightKind::Surface, 0.0),
        Kind::Sun => match params.sun_angle {
            Some(deg) => (rts::LightKind::Sun, deg.to_radians().sin().max(0.0)),
            None => {
                let soft = params.sun_soft_angle.to_radians().sin().max(0.0) * scale.max(0.0);
                (rts::LightKind::Sun, (sun_extent.max(0.0) * scale.max(0.0)).max(soft).min(0.7))
            }
        },
        Kind::SkyAmbient => (rts::LightKind::SkyAmbient, 0.0),
    };
    rts::Light {
        kind,
        position: l.origin.to_array(),
        direction: l.normal.to_array(),
        intensity,
        style: l.style,
        constant: l.constant,
        linear: l.linear,
        quadratic: l.quadratic,
        cap: l.cap,
        fade: l.fade,
        reach: l.reach,
        size,
    }
}

const CLEARANCE_SHARE: f32 = 0.75;

pub(crate) fn clearance(shadow: &hp_light::bvh::Bvh, at: Vec3A, reach: f32, below_only: bool) -> f32 {
    let mut best = reach;
    for x in -1i32..=1 {
        for y in -1i32..=1 {
            for z in -1i32..=1 {
                if (x == 0 && y == 0 && z == 0) || (below_only && z > 0) {
                    continue;
                }
                let d = Vec3A::new(x as f32, y as f32, z as f32).normalize();
                if let Some(h) = shadow.closest(at, d, best, |_| true) {
                    best = best.min(h.t);
                }
            }
        }
    }
    best
}

pub(crate) fn fit_light_size(light: &mut rts::Light, shadow: &hp_light::bvh::Bvh, scale: f32, fixture: bool) {
    if !matches!(light.kind, rts::LightKind::Point | rts::LightKind::Spot { .. }) {
        return;
    }
    if fixture {
        light.size *= FIXTURE_SIZE;
    }
    let floor = (DEFAULT_LIGHT_SIZE * scale.max(0.0)).min(light.size);
    if light.size <= floor {
        return;
    }
    let room = clearance(shadow, Vec3A::from_array(light.position), light.size / CLEARANCE_SHARE, fixture);
    light.size = light.size.min(room * CLEARANCE_SHARE).max(floor);
}

pub(crate) const FIXTURE_SIZE: f32 = 1.5;
pub(crate) const CEILING_REACH: f32 = 32.0;
pub(crate) const FIXTURE_GAIN: f32 = 2.0;
const POOL_SOFTNESS: f32 = 0.5;
const POOL_GAIN: f32 = 1.3;
const POOL_FADE: f32 = 0.75;
const BLEED: f32 = 1.35;

fn base_falloff(l: &rts::Light, d: f32) -> f32 {
    let e = d.max(1.0).min(l.cap);
    let denom = l.constant + e * (l.linear + e * l.quadratic);
    if denom > 0.0 { 1.0 / denom } else { 0.0 }
}

pub(crate) fn reference_distance(l: &rts::Light) -> f32 {
    let half = base_falloff(l, 1.0) * 0.5;
    let (mut lo, mut hi) = (1.0f32, 8192.0f32);
    if base_falloff(l, hi) > half {
        return hi;
    }
    for _ in 0..40 {
        let mid = 0.5 * (lo + hi);
        if base_falloff(l, mid) > half {
            lo = mid;
        } else {
            hi = mid;
        }
    }
    hi
}

pub(crate) fn cinematic_light(light: &mut rts::Light, shadow: &hp_light::bvh::Bvh) -> bool {
    if !matches!(light.kind, rts::LightKind::Point | rts::LightKind::Spot { .. }) {
        return false;
    }
    if light.quadratic <= 0.0 && light.linear <= 0.0 {
        return false;
    }
    let dref = reference_distance(light).clamp(64.0, 512.0);
    let soft = (POOL_SOFTNESS * dref).max(light.size);
    let gain = base_falloff(light, dref) * (dref * dref + soft * soft) * POOL_GAIN;
    if !(gain.is_finite() && gain > 0.0) {
        return false;
    }
    let reach = light.reach.min(1.0e6);
    let (start, end) = light.fade;
    let end = if end > start && end < reach { end } else { reach };
    let end = end.min(tail_distance(light, dref, end));
    light.constant = soft * soft;
    light.linear = 0.0;
    light.quadratic = 1.0;
    light.cap = 1.0e22;
    light.fade = (POOL_FADE * end, end);
    for i in &mut light.intensity {
        for c in i.iter_mut() {
            *c *= gain;
        }
    }
    let at = Vec3A::from_array(light.position);
    let ceiling =
        light.kind == rts::LightKind::Point && shadow.closest(at, Vec3A::Z, CEILING_REACH, |_| true).is_some();
    if ceiling {
        light.kind = rts::LightKind::Spot { stopdot: 0.0, stopdot2: 0.0, exponent: 1.0 };
        light.direction = [0.0, 0.0, -1.0];
        for i in &mut light.intensity {
            for c in i.iter_mut() {
                *c *= FIXTURE_GAIN;
            }
        }
    }
    ceiling
}

pub(crate) const KNEE: f32 = 1.6;
pub(crate) const FIXTURE_KNEE: f32 = 1.4;
pub(crate) const MIN_POOL_SOFTNESS: f32 = 14.0;
const KNEE_SAMPLES: usize = 64;
const SOFT_STEPS: usize = 24;
const TAIL: f32 = 0.01;
const KNEE_FLOOR: f32 = 0.25;

fn tail_distance(l: &rts::Light, dref: f32, limit: f32) -> f32 {
    let floor = base_falloff(l, dref) * TAIL;
    let (mut lo, mut hi) = (dref, limit.max(dref));
    if base_falloff(l, hi) > floor {
        return hi;
    }
    for _ in 0..40 {
        let mid = 0.5 * (lo + hi);
        if base_falloff(l, mid) > floor {
            lo = mid;
        } else {
            hi = mid;
        }
    }
    hi
}

fn luminance3(c: [f32; 3]) -> f32 {
    0.2126 * c[0] + 0.7152 * c[1] + 0.0722 * c[2]
}

pub(crate) fn pool_peak(light: &rts::Light, original: &rts::Light, dref: f32) -> f32 {
    let axis = -Vec3A::from_array(light.direction).normalize_or(Vec3A::NEG_Z);
    let axis_before = -Vec3A::from_array(original.direction).normalize_or(Vec3A::NEG_Z);
    let (i_now, i_before) = (luminance3(light.intensity[0]), luminance3(original.intensity[0]));
    let floor = original.falloff(axis_before, dref) * i_before * KNEE_FLOOR;
    let end = light.fade.1.max(dref);
    let mut peak = 0.0f32;
    for k in 0..KNEE_SAMPLES {
        let d = end.powf(k as f32 / (KNEE_SAMPLES - 1) as f32).max(1.0);
        let before = (original.falloff(axis_before, d) * i_before).max(floor);
        if before <= 0.0 {
            continue;
        }
        peak = peak.max(light.falloff(axis, d) * i_now / before);
    }
    peak
}

pub(crate) fn knee_lights(lights: &mut [rts::Light], originals: &[rts::Light]) {
    lights.par_iter_mut().zip(originals.par_iter()).for_each(|(light, original)| {
        if light.quadratic != 1.0 || light.linear != 0.0 || original.constant == light.constant {
            return;
        }
        let dref = reference_distance(original).clamp(64.0, 512.0);
        let reshape = |light: &mut rts::Light, soft: f32| {
            let now = light.constant.max(0.0);
            let scale = (dref * dref + soft * soft) / (dref * dref + now);
            light.constant = soft * soft;
            for i in &mut light.intensity {
                for c in i.iter_mut() {
                    *c *= scale;
                }
            }
        };
        let lo = light.constant.max(0.0).sqrt().max(MIN_POOL_SOFTNESS);
        let hi = (4.0 * dref).max(lo);
        let mut best = (f32::INFINITY, lo);
        for k in 0..SOFT_STEPS {
            let soft = lo * (hi / lo).powf(k as f32 / (SOFT_STEPS - 1) as f32);
            reshape(light, soft);
            let peak = pool_peak(light, original, dref);
            if peak < best.0 {
                best = (peak, soft);
            }
        }
        reshape(light, best.1);
        let peak = best.0;
        let fixture = original.kind == rts::LightKind::Point && light.kind != rts::LightKind::Point;
        let limit = if fixture { KNEE * FIXTURE_KNEE } else { KNEE };
        let scale = if peak > limit { limit / peak } else { 1.0 };
        for i in &mut light.intensity {
            for c in i.iter_mut() {
                *c *= scale;
            }
        }
    });
}

pub(crate) fn richer_bounce(refl: [f32; 3]) -> [f32; 3] {
    let lum = 0.2126 * refl[0] + 0.7152 * refl[1] + 0.0722 * refl[2];
    let mut out = refl.map(|c| (lum + (c - lum) * BLEED).max(0.0));
    let now = 0.2126 * out[0] + 0.7152 * out[1] + 0.0722 * out[2];
    if now > 1e-4 {
        out = out.map(|c| c * lum / now);
    }
    out.map(|c| c.min(0.9))
}

struct FaceJob {
    w: u32,
    h: u32,
    normals: u32,
    out_normals: usize,
    styles: Vec<u8>,
    lights: Vec<u32>,
    luxels: Vec<(u32, u32)>,
    points: Vec<rts::Point>,
    in_skybox: bool,
    minlight: f32,
}

fn face_job(ctx: &Ctx<'_>, f: &LitFace, ss: usize, bump: bool, style_bounce: bool) -> FaceJob {
    let samples = ctx.luxel_samples(f);
    let mut clusters: Vec<i32> = samples.samples.iter().flatten().map(|sp| sp.cluster).collect();
    clusters.extend(f.poly.iter().map(|v| ctx.map.vis_cluster_at(*v + f.normal)));
    clusters.sort_unstable();
    clusters.dedup();
    clusters.retain(|c| *c >= 0);
    let (cand, cand_styles) = ctx.candidates(f, &clusters);
    let mut styles: Vec<u8> = vec![0];
    for st in &cand_styles {
        if styles.len() < MAX_SLOTS && !styles.contains(st) {
            styles.push(*st);
        }
    }
    if style_bounce && styles.len() < MAX_SLOTS {
        let (lo, hi) = (f.bounds.0.as_vec3a(), f.bounds.1.as_vec3a());
        for (i, l) in ctx.lights.list.iter().enumerate() {
            if styles.len() >= MAX_SLOTS {
                break;
            }
            if l.style == 0 || styles.contains(&l.style) || !matches!(l.kind, Kind::Point | Kind::Spot { .. }) {
                continue;
            }
            if l.origin.clamp(lo, hi).distance(l.origin) > l.reach * BOUNCE_STYLE_REACH {
                continue;
            }
            let row = ctx.light_rows.get(i).and_then(Option::as_ref);
            if clusters.is_empty() || clusters.iter().any(|c| super::super::bake::row_sees(row, *c)) {
                styles.push(l.style);
            }
        }
    }
    let lights: Vec<u32> =
        cand.iter().filter(|(li, _)| styles.contains(&ctx.lights.list[*li].style)).map(|(li, _)| *li as u32).collect();
    let out_normals = if f.bump { 4 } else { 1 };
    let normals = if f.bump && bump { 4 } else { 1 };
    let mut luxels = Vec::with_capacity(f.luxels());
    let mut points = Vec::new();
    let all_dark = samples.dark.iter().all(|d| *d);
    let push_point = |points: &mut Vec<rts::Point>, sp: &super::super::bake::Sample, lift: f64| {
        let ns: [[f32; 3]; 4] = std::array::from_fn(|k| sp.normals[if normals == 4 { k } else { 0 }].to_array());
        let pos = sp.pos.as_dvec3() + f.normal * lift;
        points.push(rts::Point { pos: pos.as_vec3a().to_array(), normals: ns });
    };
    for i in 0..f.luxels() {
        let start = points.len() as u32;
        let (s, t) = (i % f.w, i / f.w);
        let center = samples.centers[i];
        if samples.dark[i] {
            if all_dark {
                let sp = ctx.sample(f, center.x, center.y);
                push_point(&mut points, &sp, 0.0);
            }
            luxels.push((start, points.len() as u32 - start));
            continue;
        }
        let mut coords: Vec<DVec2> = Vec::new();
        if ss > 1 {
            for gy in 0..ss {
                for gx in 0..ss {
                    let q = DVec2::new(
                        s as f64 - 0.5 + (gx as f64 + 0.5) / ss as f64,
                        t as f64 - 0.5 + (gy as f64 + 0.5) / ss as f64,
                    );
                    if f.disp.is_some() {
                        let q = q.clamp(DVec2::ZERO, DVec2::new((f.w.max(1) - 1) as f64, (f.h.max(1) - 1) as f64));
                        coords.push(q);
                        continue;
                    }
                    let piece = &samples.pieces[i];
                    if piece.len() >= 3 && !inside(piece, q) {
                        continue;
                    }
                    if piece.len() < 3 && f.poly2.len() >= 3 && !inside(&f.poly2, q) {
                        continue;
                    }
                    if f.frame.is_some()
                        && (ctx.in_brush(f.plane_point(q.x, q.y) + f.normal) || !ctx.reaches(f, center, q))
                    {
                        continue;
                    }
                    coords.push(q);
                }
            }
        }
        if coords.is_empty() {
            coords.push(center);
        }
        for q in coords {
            let sp = ctx.sample(f, q.x, q.y);
            push_point(&mut points, &sp, 0.0);
        }
        luxels.push((start, points.len() as u32 - start));
    }
    let center = (f.bounds.0 + f.bounds.1) * 0.5;
    let in_skybox = ctx.cameras.is_empty()
        || ctx.map.area_at(center + f.normal).is_some_and(|a| ctx.camera_areas.get(a).copied().unwrap_or(false));
    FaceJob {
        w: f.w as u32,
        h: f.h as u32,
        normals,
        out_normals,
        styles,
        lights,
        luxels,
        points,
        in_skybox,
        minlight: f.minlight,
    }
}

pub(crate) fn to_face_lights(
    out: &hp_raytrace::Output,
    built: &Built,
    passes: usize,
    ao_strength: (f32, f32),
) -> Vec<FaceLight> {
    out.surfaces.par_iter().enumerate().map(|(si, so)| face_light(so, built, si, passes, ao_strength)).collect()
}

pub(crate) fn contact_exposure(out: &hp_raytrace::Output, built: &Built, strength: f32) -> f32 {
    if strength <= 0.0 {
        return 1.0;
    }
    let mut factors: Vec<f32> = Vec::new();
    for (si, so) in out.surfaces.iter().enumerate() {
        let s = &built.scene.surfaces[si];
        let (mut sum, mut n) = (0.0f32, 0u32);
        for (i, ao) in so.ao.iter().enumerate().take(s.luxels()) {
            if built.scene.luxels[s.first_luxel as usize + i].points > 0 {
                sum += (1.0 - strength * (1.0 - ao)).max(0.0);
                n += 1;
            }
        }
        if n > 0 {
            factors.push(sum / n as f32);
        }
    }
    if factors.is_empty() {
        return 1.0;
    }
    let mid = factors.len() / 2;
    factors.select_nth_unstable_by(mid, f32::total_cmp);
    let m = factors[mid];
    if m > 0.05 { (1.0 / m).min(1.5) } else { 1.0 }
}

pub(crate) fn face_light(
    so: &hp_raytrace::SurfaceOut,
    built: &Built,
    si: usize,
    passes: usize,
    ao_strength: (f32, f32),
) -> FaceLight {
    let s = &built.scene.surfaces[si];
    let onn = built.face_normals[si];
    let lux = s.luxels();
    let nn = s.normals as usize;
    let ns = s.styles.len();
    let dark: Vec<bool> = (0..lux).map(|i| built.scene.luxels[s.first_luxel as usize + i].points == 0).collect();
    let value = |pass: usize, slot: usize, n: usize, i: usize| -> Vec3A {
        let n = n.min(nn - 1);
        let k = ((pass * ns + slot) * nn + n) * lux + i;
        let d = Vec3A::from_array(so.direct[k]);
        let ind = Vec3A::from_array(so.indirect[k]);
        let ao = so.ao.get(i).copied().unwrap_or(1.0);
        d * (1.0 - ao_strength.0 * (1.0 - ao)).max(0.0) + ind * (1.0 - ao_strength.1 * (1.0 - ao)).max(0.0)
    };
    let keep: Vec<usize> = (0..ns)
        .filter(|&slot| {
            slot == 0
                || (0..lux).any(|i| (0..passes).any(|p| (0..nn).any(|n| value(p, slot, n, i).max_element() > 0.5)))
        })
        .collect();
    let kept = keep.len();
    let mut values = vec![glam::Vec3::ZERO; passes * kept * onn * lux];
    for p in 0..passes {
        for (ks, &slot) in keep.iter().enumerate() {
            for n in 0..onn {
                let dst = ((p * kept + ks) * onn + n) * lux;
                for i in 0..lux {
                    values[dst + i] = value(p, slot, n, i).into();
                }
            }
        }
    }
    FaceLight { styles: keep.iter().map(|&k| s.styles[k]).collect(), normals: onn, values, dark }
}

pub(crate) fn optimal_params(rt: &RayTracing, luxels: usize, gpu: bool) -> (hp_raytrace::Params, String) {
    let mut p = rt.params();
    if rt.quality != Quality::Optimal {
        return (p, String::new());
    }
    let ss = optimal_supersample(luxels);
    let (min, max, bounces, why) = if gpu {
        if luxels <= 1_500_000 {
            (128, 512, 8, "GPU and a small or medium map: final quality sampling")
        } else if luxels <= 5_000_000 {
            (64, 256, 6, "GPU and a large map: normal sampling")
        } else {
            (32, 128, 5, "GPU and a very large map: lighter sampling to keep the compile in minutes")
        }
    } else if luxels <= 300_000 {
        (48, 192, 5, "no GPU, small map: normal sampling")
    } else {
        (16, 64, 3, "no GPU, large map: draft sampling so the CPU compile stays reasonable")
    };
    if rt.min_samples.is_none() {
        p.min_samples = min;
    }
    if rt.max_samples.is_none() {
        p.max_samples = max.max(p.min_samples);
    }
    if rt.bounces.is_none() {
        p.bounces = bounces;
    }
    if rt.supersample.is_none() {
        p.supersample = ss;
    }
    let p = p.sanitized();
    (
        p,
        format!(
            "Optimal ray tracing for {luxels} luxels: {} to {} samples per luxel, {} bounces, {}x{} sample points per luxel ({why})",
            p.min_samples, p.max_samples, p.bounces, p.supersample, p.supersample
        ),
    )
}
