use glam::Vec3A;

use super::scene::{SkyFace, Texture};

pub(crate) const PAINTED: &str = "painted";
const FACE: u32 = 128;
const STAR_GAIN: f32 = 1.29;
const STAR_LAYER_GAIN: f32 = 1.5;

#[derive(Clone, Debug, PartialEq)]
pub(crate) struct Paint {
    pub top: Vec3A,
    pub bottom: Vec3A,
    pub fade_bias: f32,
    pub hdr_scale: f32,
    pub sun_size: f32,
    pub sun_color: Vec3A,
    pub sun_dir: Vec3A,
    pub dusk_scale: f32,
    pub dusk_intensity: f32,
    pub dusk_color: Vec3A,
    pub stars: bool,
    pub star_texture: String,
    pub star_layers: u32,
    pub star_scale: f32,
    pub star_fade: f32,
}

impl Default for Paint {
    fn default() -> Self {
        Paint {
            top: Vec3A::new(0.2, 0.5, 1.0),
            bottom: Vec3A::new(0.8, 1.0, 1.0),
            fade_bias: 1.0,
            hdr_scale: 0.66,
            sun_size: 2.0,
            sun_color: Vec3A::new(0.2, 0.1, 0.0),
            sun_dir: Vec3A::new(0.4, 0.0, 0.01),
            dusk_scale: 1.0,
            dusk_intensity: 1.0,
            dusk_color: Vec3A::new(1.0, 0.2, 0.0),
            stars: false,
            star_texture: "skybox/starfield".to_owned(),
            star_layers: 1,
            star_scale: 0.5,
            star_fade: 1.0,
        }
    }
}

fn num(v: &str) -> Option<f32> {
    v.split_ascii_whitespace().next()?.parse::<f32>().ok().filter(|x| x.is_finite())
}

fn vec3(v: &str) -> Option<Vec3A> {
    let p: Vec<f32> = v.split_ascii_whitespace().filter_map(|x| x.parse().ok()).collect();
    (p.len() >= 3 && p.iter().all(|x| x.is_finite())).then(|| Vec3A::new(p[0], p[1], p[2]))
}

fn sun_direction(e: &hp_bsp::Entity) -> Vec3A {
    let angles = vec3(e.get("angles").unwrap_or("")).unwrap_or(Vec3A::ZERO);
    let pitch = e.get("pitch").and_then(num).filter(|p| *p != 0.0).unwrap_or(angles.x);
    let (p, y) = (pitch.to_radians(), angles.y.to_radians());
    Vec3A::new(p.cos() * y.cos(), p.cos() * y.sin(), -p.sin())
}

pub(crate) fn paint_of(entities: &[hp_bsp::Entity]) -> Paint {
    let mut p = Paint::default();
    let Some(e) = entities.iter().find(|e| e.classname().eq_ignore_ascii_case("env_skypaint")) else { return p };
    let mut from_sun = false;
    for (k, v) in &e.pairs {
        let v = v.as_str();
        match k.to_ascii_lowercase().as_str() {
            "topcolor" => p.top = vec3(v).unwrap_or(p.top),
            "bottomcolor" => p.bottom = vec3(v).unwrap_or(p.bottom),
            "fadebias" => p.fade_bias = num(v).unwrap_or(p.fade_bias),
            "hdrscale" => p.hdr_scale = num(v).unwrap_or(p.hdr_scale),
            "sunsize" => p.sun_size = num(v).unwrap_or(p.sun_size),
            "suncolor" => p.sun_color = vec3(v).unwrap_or(p.sun_color),
            "sunnormal" => p.sun_dir = vec3(v).unwrap_or(p.sun_dir),
            "sunposmethod" => from_sun = num(v).is_some_and(|x| x as i32 == 1),
            "duskscale" => p.dusk_scale = num(v).unwrap_or(p.dusk_scale),
            "duskintensity" => p.dusk_intensity = num(v).unwrap_or(p.dusk_intensity),
            "duskcolor" => p.dusk_color = vec3(v).unwrap_or(p.dusk_color),
            "drawstars" => p.stars = num(v).is_some_and(|x| x != 0.0),
            "starlayers" => p.star_layers = num(v).map_or(p.star_layers, |x| x.clamp(1.0, 3.0) as u32),
            "starscale" => p.star_scale = num(v).unwrap_or(p.star_scale),
            "starfade" => p.star_fade = num(v).unwrap_or(p.star_fade),
            "startexture" if !v.trim().is_empty() => p.star_texture = v.trim().to_owned(),
            _ => {}
        }
    }
    if from_sun && let Some(sun) = entities.iter().find(|e| e.classname().eq_ignore_ascii_case("env_sun")) {
        p.sun_dir = sun_direction(sun);
    }
    p
}

pub(crate) fn color(p: &Paint, d: Vec3A, hdr: bool, stars: Option<&Texture>) -> Vec3A {
    let clamp3 = |c: Vec3A| c.clamp(Vec3A::ZERO, Vec3A::splat(64.0));
    let (top, bottom) = (clamp3(p.top), clamp3(p.bottom));
    let z = d.z.clamp(0.0, 1.0);
    let mut c = bottom.lerp(top, z.powf(p.fade_bias.clamp(0.001, 1024.0)));
    let sun = p.sun_dir.normalize_or(Vec3A::X);
    if p.sun_size > 0.0 {
        let e = (20.0 / p.sun_size).clamp(0.0, 4096.0);
        c += clamp3(p.sun_color) * d.dot(sun).clamp(0.0, 1.0).powf(e);
    }
    if let (true, Some(t), true) = (p.stars, stars, d.z > 0.0) {
        let sz = d.z.max(1e-3);
        let scale = 1.0 / p.star_scale.clamp(0.01, 1024.0);
        let uv = [d.x / sz * scale, d.y / sz * scale];
        let px = t.sample(uv);
        let raw = Vec3A::new(f32::from(px[0]), f32::from(px[1]), f32::from(px[2])) / 255.0;
        let gain = STAR_GAIN * STAR_LAYER_GAIN.powi(p.star_layers.clamp(1, 3) as i32 - 1);
        c += raw * (sz.powf(1.0 / p.star_fade.clamp(0.01, 1024.0)) * gain);
    }
    let dusk = 0.5 * p.dusk_intensity.clamp(0.0, 64.0);
    if dusk > 0.0 {
        let hxy = (d.x * d.x + d.y * d.y).sqrt();
        let dn = if hxy > 1e-4 { (d.x * sun.x + d.y * sun.y) / hxy } else { 0.0 };
        let h = ((1.0 - z).powf(4.5) * (0.582 + 0.381 * dn)).max(1e-6);
        let w = dusk * h.powf(2.0 - p.dusk_scale.clamp(0.0, 16.0));
        c += (clamp3(p.dusk_color) * 2.0 - c) * w;
    }
    let scale = if hdr { p.hdr_scale.clamp(0.0, 64.0) } else { 1.0 };
    (c * scale).max(Vec3A::ZERO)
}

pub(crate) fn painted_faces(p: &Paint, hdr: bool, stars: Option<&Texture>) -> Vec<SkyFace> {
    (0..6)
        .map(|face| {
            let mut texels = Vec::with_capacity((FACE * FACE) as usize);
            for y in 0..FACE {
                for x in 0..FACE {
                    let s = 2.0 * (x as f32 + 0.5) / FACE as f32 - 1.0;
                    let t = 2.0 * (y as f32 + 0.5) / FACE as f32 - 1.0;
                    let d = sky_face_dir(face, s, t).normalize();
                    texels.push(color(p, d, hdr, stars) * 255.0);
                }
            }
            SkyFace { w: FACE, h: FACE, texels }
        })
        .collect()
}

pub(crate) fn sky_face_dir(face: usize, s: f32, t: f32) -> Vec3A {
    let up = -t;
    match face {
        0 => Vec3A::new(1.0, -s, up),
        1 => Vec3A::new(-1.0, s, up),
        2 => Vec3A::new(s, 1.0, up),
        3 => Vec3A::new(-s, -1.0, up),
        4 => Vec3A::new(-up, -s, 1.0),
        _ => Vec3A::new(up, -s, -1.0),
    }
}
