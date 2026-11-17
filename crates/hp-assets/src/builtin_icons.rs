use hp_render::{TexFormat, TextureUpload};

use crate::texture::PreparedTexture;

pub(crate) const PREFIX: &str = "hammerpro/icons/";
const SIZE: u32 = 64;
const SAMPLES: u32 = 4;

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(crate) enum Glyph {
    Bulb,
    Spot,
    Sun,
    Cubemap,
    Speaker,
    Gear,
    Target,
    Fog,
    Camera,
    Teleport,
    Text,
    Particles,
    Fire,
    Filter,
    Clock,
    Counter,
    Script,
    Player,
    Generic,
}

const ALL: [Glyph; 19] = [
    Glyph::Bulb,
    Glyph::Spot,
    Glyph::Sun,
    Glyph::Cubemap,
    Glyph::Speaker,
    Glyph::Gear,
    Glyph::Target,
    Glyph::Fog,
    Glyph::Camera,
    Glyph::Teleport,
    Glyph::Text,
    Glyph::Particles,
    Glyph::Fire,
    Glyph::Filter,
    Glyph::Clock,
    Glyph::Counter,
    Glyph::Script,
    Glyph::Player,
    Glyph::Generic,
];

impl Glyph {
    fn name(self) -> &'static str {
        match self {
            Glyph::Bulb => "bulb",
            Glyph::Spot => "spot",
            Glyph::Sun => "sun",
            Glyph::Cubemap => "cubemap",
            Glyph::Speaker => "speaker",
            Glyph::Gear => "gear",
            Glyph::Target => "target",
            Glyph::Fog => "fog",
            Glyph::Camera => "camera",
            Glyph::Teleport => "teleport",
            Glyph::Text => "text",
            Glyph::Particles => "particles",
            Glyph::Fire => "fire",
            Glyph::Filter => "filter",
            Glyph::Clock => "clock",
            Glyph::Counter => "counter",
            Glyph::Script => "script",
            Glyph::Player => "player",
            Glyph::Generic => "generic",
        }
    }

    fn from_name(name: &str) -> Option<Glyph> {
        ALL.into_iter().find(|g| g.name() == name)
    }

    fn color(self) -> [f32; 3] {
        match self {
            Glyph::Bulb | Glyph::Spot => [1.0, 0.86, 0.35],
            Glyph::Sun => [1.0, 0.66, 0.16],
            Glyph::Cubemap => [0.35, 0.67, 1.0],
            Glyph::Speaker => [0.47, 0.86, 0.47],
            Glyph::Gear | Glyph::Filter | Glyph::Clock | Glyph::Counter => [0.75, 0.55, 1.0],
            Glyph::Target => [1.0, 0.36, 0.36],
            Glyph::Fog => [0.8, 0.84, 0.88],
            Glyph::Camera => [0.4, 0.86, 0.9],
            Glyph::Teleport => [0.3, 0.85, 0.75],
            Glyph::Text => [0.95, 0.95, 0.95],
            Glyph::Particles => [1.0, 0.5, 0.8],
            Glyph::Fire => [1.0, 0.42, 0.15],
            Glyph::Script => [0.55, 0.6, 1.0],
            Glyph::Player => [0.45, 0.9, 0.4],
            Glyph::Generic => [0.62, 0.72, 0.85],
        }
    }
}

pub(crate) fn glyph_for_class(classname: &str) -> Option<Glyph> {
    let c = classname.trim().to_ascii_lowercase();
    let c = c.as_str();
    let starts = |p: &str| c.starts_with(p);
    Some(match c {
        "light" | "light_dynamic" | "point_light" | "light_glspot" => Glyph::Bulb,
        "light_spot" | "point_spotlight" | "beam_spotlight" | "env_projectedtexture" => Glyph::Spot,
        "light_environment" | "light_directional" | "env_sun" | "env_skypaint" => Glyph::Sun,
        "env_cubemap" | "env_cubemap_box" => Glyph::Cubemap,
        "ambient_generic" | "ambient_music" | "env_microphone" => Glyph::Speaker,
        "env_fog_controller" | "env_wind" => Glyph::Fog,
        "point_camera" | "point_viewcontrol" | "info_camera_link" => Glyph::Camera,
        "point_teleport" | "info_teleport_destination" => Glyph::Teleport,
        "game_text" | "point_worldtext" | "env_message" | "env_hudhint" | "game_text_tf" => Glyph::Text,
        "info_particle_system"
        | "env_smokestack"
        | "env_steam"
        | "env_spark"
        | "env_sparks"
        | "env_dustpuff"
        | "env_splash"
        | "env_particles"
        | "env_smoketrail" => Glyph::Particles,
        "env_fire" | "env_firesource" | "env_explosion" | "env_physexplosion" | "env_fire_trail" => Glyph::Fire,
        "logic_timer" => Glyph::Clock,
        "lua_run" | "logic_script" => Glyph::Script,
        _ if starts("env_soundscape") || starts("snd_") => Glyph::Speaker,
        _ if starts("filter_") => Glyph::Filter,
        _ if starts("math_") => Glyph::Counter,
        _ if starts("point_script") => Glyph::Script,
        _ if starts("logic_") || starts("game_") || starts("ai_") || (starts("npc_") && c.ends_with("_maker")) => {
            Glyph::Gear
        }
        _ if starts("info_player_") || c.contains("spawnpoint") => Glyph::Player,
        _ if starts("info_") || starts("point_") || starts("scripted_") => Glyph::Target,
        _ if starts("env_") || starts("phys_") || starts("shadow_") || starts("color_correction") => Glyph::Generic,
        _ => return None,
    })
}

pub(crate) fn material_for_class(classname: &str) -> Option<String> {
    glyph_for_class(classname).map(|g| format!("{PREFIX}{}", g.name()))
}

pub(crate) fn is_builtin(name: &str) -> bool {
    name.starts_with(PREFIX)
}

pub(crate) fn material(name: &str) -> Option<hp_vmt::Material> {
    let glyph = Glyph::from_name(name.strip_prefix(PREFIX)?)?;
    let text = format!(
        "\"Sprite\" {{ \"$spriteorientation\" \"vp_parallel\" \"$spriteorigin\" \"[ 0.50 0.50 ]\" \"$basetexture\" \"{PREFIX}{}\" \"$no_fullbright\" 1 }}",
        glyph.name()
    );
    hp_vmt::parse(&text).ok()
}

pub(crate) fn size(name: &str) -> Option<(u32, u32)> {
    name.strip_prefix(PREFIX).and_then(Glyph::from_name).map(|_| (SIZE, SIZE))
}

pub(crate) fn prepare(key: &str) -> Option<PreparedTexture> {
    let rest = key.strip_prefix(PREFIX)?;
    let (name, linear) = match rest.strip_suffix(crate::material::LINEAR_SUFFIX) {
        Some(n) => (n, true),
        None => (rest, false),
    };
    let glyph = Glyph::from_name(name)?;
    let levels = crate::texture::build_mips(SIZE, SIZE, pixels(glyph))?;
    let format = if linear { TexFormat::Rgba8Linear } else { TexFormat::Rgba8Srgb };
    Some(PreparedTexture { upload: TextureUpload { width: SIZE, height: SIZE, format, levels }, size: [SIZE, SIZE] })
}

fn circle(x: f32, y: f32, cx: f32, cy: f32, r: f32) -> bool {
    (x - cx).powi(2) + (y - cy).powi(2) <= r * r
}

fn ring(x: f32, y: f32, cx: f32, cy: f32, r: f32, w: f32) -> bool {
    let d = ((x - cx).powi(2) + (y - cy).powi(2)).sqrt();
    (d - r).abs() <= w * 0.5
}

fn rect(x: f32, y: f32, x0: f32, y0: f32, x1: f32, y1: f32) -> bool {
    x >= x0 && x <= x1 && y >= y0 && y <= y1
}

fn seg(x: f32, y: f32, a: (f32, f32), b: (f32, f32), w: f32) -> bool {
    let (dx, dy) = (b.0 - a.0, b.1 - a.1);
    let len = dx * dx + dy * dy;
    let t = if len > 0.0 { (((x - a.0) * dx + (y - a.1) * dy) / len).clamp(0.0, 1.0) } else { 0.0 };
    let (px, py) = (a.0 + dx * t - x, a.1 + dy * t - y);
    px * px + py * py <= w * w * 0.25
}

fn tri(x: f32, y: f32, a: (f32, f32), b: (f32, f32), c: (f32, f32)) -> bool {
    let s = |p: (f32, f32), q: (f32, f32)| (q.0 - p.0) * (y - p.1) - (q.1 - p.1) * (x - p.0);
    let (d1, d2, d3) = (s(a, b), s(b, c), s(c, a));
    let neg = d1 < 0.0 || d2 < 0.0 || d3 < 0.0;
    let pos = d1 > 0.0 || d2 > 0.0 || d3 > 0.0;
    !(neg && pos)
}

fn spokes(x: f32, y: f32, count: u32, r0: f32, r1: f32, w: f32, phase: f32) -> bool {
    (0..count).any(|i| {
        let a = phase + i as f32 * std::f32::consts::TAU / count as f32;
        let (s, c) = a.sin_cos();
        seg(x, y, (c * r0, s * r0), (c * r1, s * r1), w)
    })
}

#[derive(Clone, Copy, PartialEq)]
enum Ink {
    None,
    Main,
    Light,
    Dark,
    Hole,
}

fn ink(glyph: Glyph, x: f32, y: f32) -> Ink {
    use Ink::*;
    let on = |b: bool, i: Ink| if b { i } else { None };
    match glyph {
        Glyph::Bulb => {
            if circle(x, y, -0.12, 0.3, 0.12) {
                Light
            } else if circle(x, y, 0.0, 0.18, 0.4) || tri(x, y, (-0.3, -0.05), (0.3, -0.05), (0.0, -0.32)) {
                Main
            } else {
                on(rect(x, y, -0.18, -0.58, 0.18, -0.24), Dark)
            }
        }
        Glyph::Spot => {
            if rect(x, y, -0.18, 0.38, 0.18, 0.66) {
                Dark
            } else {
                on(tri(x, y, (0.0, 0.5), (-0.55, -0.55), (0.55, -0.55)), Main)
            }
        }
        Glyph::Sun => {
            if circle(x, y, -0.08, 0.08, 0.1) {
                Light
            } else {
                on(circle(x, y, 0.0, 0.0, 0.3) || spokes(x, y, 8, 0.42, 0.66, 0.11, 0.0), Main)
            }
        }
        Glyph::Cubemap => {
            if circle(x, y, -0.2, 0.2, 0.14) {
                Light
            } else if circle(x, y, 0.0, 0.0, 0.56) {
                if (x + 0.2).powi(2) + (y - 0.2).powi(2) > 0.36 { Dark } else { Main }
            } else {
                None
            }
        }
        Glyph::Speaker => {
            let arc = |r: f32| x > 0.12 && y.abs() < (x - 0.0) * 1.1 && ring(x, y, 0.0, 0.0, r, 0.09);
            on(
                rect(x, y, -0.6, -0.2, -0.3, 0.2)
                    || ((-0.3..=0.08).contains(&x) && y.abs() <= 0.2 + (x + 0.3) * 0.85)
                    || arc(0.36)
                    || arc(0.58),
                Main,
            )
        }
        Glyph::Gear => {
            if circle(x, y, 0.0, 0.0, 0.17) {
                Hole
            } else {
                on(circle(x, y, 0.0, 0.0, 0.42) || spokes(x, y, 8, 0.3, 0.62, 0.17, 0.2), Main)
            }
        }
        Glyph::Target => on(
            ring(x, y, 0.0, 0.0, 0.5, 0.1)
                || circle(x, y, 0.0, 0.0, 0.12)
                || seg(x, y, (-0.7, 0.0), (-0.28, 0.0), 0.09)
                || seg(x, y, (0.28, 0.0), (0.7, 0.0), 0.09)
                || seg(x, y, (0.0, -0.7), (0.0, -0.28), 0.09)
                || seg(x, y, (0.0, 0.28), (0.0, 0.7), 0.09),
            Main,
        ),
        Glyph::Fog => {
            let wave = |cy: f32| (y - cy - 0.06 * (x * 7.0).sin()).abs() <= 0.06 && x.abs() <= 0.62;
            on(wave(0.34) || wave(0.0) || wave(-0.34), Main)
        }
        Glyph::Camera => {
            if circle(x, y, -0.16, 0.0, 0.17) {
                Hole
            } else {
                on(rect(x, y, -0.62, -0.32, 0.26, 0.32) || tri(x, y, (0.22, 0.0), (0.64, 0.3), (0.64, -0.3)), Main)
            }
        }
        Glyph::Teleport => on(
            seg(x, y, (-0.6, 0.26), (0.3, 0.26), 0.12)
                || tri(x, y, (0.62, 0.26), (0.26, 0.5), (0.26, 0.02))
                || seg(x, y, (0.6, -0.26), (-0.3, -0.26), 0.12)
                || tri(x, y, (-0.62, -0.26), (-0.26, -0.02), (-0.26, -0.5)),
            Main,
        ),
        Glyph::Text => on(rect(x, y, -0.5, 0.38, 0.5, 0.6) || rect(x, y, -0.11, -0.6, 0.11, 0.42), Main),
        Glyph::Particles => {
            let dots =
                [(-0.35, 0.3, 0.14), (0.25, 0.4, 0.1), (0.0, -0.05, 0.17), (-0.4, -0.38, 0.09), (0.4, -0.3, 0.13)];
            on(dots.iter().any(|&(cx, cy, r)| circle(x, y, cx, cy, r)), Main)
        }
        Glyph::Fire => {
            if circle(x, y, 0.0, -0.3, 0.2) || tri(x, y, (0.0, 0.15), (-0.18, -0.25), (0.18, -0.25)) {
                Light
            } else {
                on(circle(x, y, 0.0, -0.22, 0.38) || tri(x, y, (0.05, 0.68), (-0.37, -0.12), (0.37, -0.12)), Main)
            }
        }
        Glyph::Filter => on(tri(x, y, (-0.6, 0.5), (0.6, 0.5), (0.0, -0.08)) || rect(x, y, -0.1, -0.6, 0.1, 0.0), Main),
        Glyph::Clock => on(
            ring(x, y, 0.0, 0.0, 0.52, 0.11)
                || seg(x, y, (0.0, 0.0), (0.0, 0.36), 0.1)
                || seg(x, y, (0.0, 0.0), (0.26, -0.1), 0.1),
            Main,
        ),
        Glyph::Counter => on(
            seg(x, y, (-0.2, -0.58), (-0.12, 0.58), 0.12)
                || seg(x, y, (0.14, -0.58), (0.22, 0.58), 0.12)
                || seg(x, y, (-0.55, 0.2), (0.55, 0.2), 0.12)
                || seg(x, y, (-0.55, -0.2), (0.55, -0.2), 0.12),
            Main,
        ),
        Glyph::Script => on(
            seg(x, y, (-0.22, 0.42), (-0.58, 0.0), 0.12)
                || seg(x, y, (-0.58, 0.0), (-0.22, -0.42), 0.12)
                || seg(x, y, (0.22, 0.42), (0.58, 0.0), 0.12)
                || seg(x, y, (0.58, 0.0), (0.22, -0.42), 0.12)
                || seg(x, y, (0.1, 0.5), (-0.1, -0.5), 0.1),
            Main,
        ),
        Glyph::Player => {
            on(circle(x, y, 0.0, 0.36, 0.2) || ((-0.6..=0.1).contains(&y) && circle(x, y, 0.0, -0.6, 0.62)), Main)
        }
        Glyph::Generic => {
            let d = x.abs() + y.abs();
            if d <= 0.18 { Light } else { on((d - 0.46).abs() <= 0.08, Main) }
        }
    }
}

fn sample(glyph: Glyph, x: f32, y: f32) -> [f32; 4] {
    const BG: [f32; 3] = [0.11, 0.12, 0.15];
    let half = 0.86;
    let corner = 0.24;
    let (qx, qy) = ((x.abs() - (half - corner)).max(0.0), (y.abs() - (half - corner)).max(0.0));
    let outside = (qx * qx + qy * qy).sqrt() - corner;
    if outside > 0.0 {
        return [0.0; 4];
    }
    let main = glyph.color();
    let rgb = if outside > -0.09 {
        main.map(|c| c * 0.85)
    } else {
        match ink(glyph, x * 1.12, y * 1.12) {
            Ink::Main => main,
            Ink::Light => main.map(|c| c + (1.0 - c) * 0.7),
            Ink::Dark => main.map(|c| c * 0.55),
            Ink::None | Ink::Hole => BG,
        }
    };
    [rgb[0], rgb[1], rgb[2], 0.92]
}

pub(crate) fn pixels(glyph: Glyph) -> Vec<u8> {
    let mut out = Vec::with_capacity((SIZE * SIZE * 4) as usize);
    let n = (SAMPLES * SAMPLES) as f32;
    for py in 0..SIZE {
        for px in 0..SIZE {
            let mut acc = [0.0f32; 4];
            for sy in 0..SAMPLES {
                for sx in 0..SAMPLES {
                    let fx = (px as f32 + (sx as f32 + 0.5) / SAMPLES as f32) / SIZE as f32 * 2.0 - 1.0;
                    let fy = 1.0 - (py as f32 + (sy as f32 + 0.5) / SAMPLES as f32) / SIZE as f32 * 2.0;
                    let s = sample(glyph, fx, fy);
                    for k in 0..3 {
                        acc[k] += s[k] * s[3];
                    }
                    acc[3] += s[3];
                }
            }
            let a = acc[3] / n;
            let unpremul = |v: f32| if acc[3] > 0.0 { v / acc[3] } else { 0.0 };
            let to8 = |v: f32| (v.clamp(0.0, 1.0) * 255.0 + 0.5) as u8;
            out.extend_from_slice(&[to8(unpremul(acc[0])), to8(unpremul(acc[1])), to8(unpremul(acc[2])), to8(a)]);
        }
    }
    out
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn known_classes_get_builtin_icons() {
        for (class, glyph) in [
            ("env_cubemap", Glyph::Cubemap),
            ("light", Glyph::Bulb),
            ("light_spot", Glyph::Spot),
            ("light_environment", Glyph::Sun),
            ("env_sun", Glyph::Sun),
            ("ambient_generic", Glyph::Speaker),
            ("env_soundscape_triggerable", Glyph::Speaker),
            ("logic_auto", Glyph::Gear),
            ("logic_relay", Glyph::Gear),
            ("logic_timer", Glyph::Clock),
            ("math_counter", Glyph::Counter),
            ("info_target", Glyph::Target),
            ("info_player_start", Glyph::Player),
            ("point_template", Glyph::Target),
            ("point_teleport", Glyph::Teleport),
            ("filter_activator_name", Glyph::Filter),
            ("LUA_RUN", Glyph::Script),
            ("env_shake", Glyph::Generic),
        ] {
            assert_eq!(glyph_for_class(class), Some(glyph), "{class}");
        }
        for class in ["prop_static", "func_door", "worldspawn", "npc_citizen", "weapon_crowbar", ""] {
            assert_eq!(glyph_for_class(class), None, "{class}");
        }
        assert_eq!(material_for_class("env_cubemap").as_deref(), Some("hammerpro/icons/cubemap"));
    }

    #[test]
    fn every_icon_is_a_sprite_with_a_drawn_texture() {
        for g in ALL {
            let name = format!("{PREFIX}{}", g.name());
            let mat = material(&name).expect("material");
            assert_eq!(mat.shader_kind(), hp_vmt::ShaderKind::Sprite);
            let a = crate::material::analyze(&name, &mat, &|_| 0);
            assert_eq!(a.texture.as_deref(), Some(name.as_str()));
            let p = prepare(&name).expect("texture");
            assert_eq!(p.size, [SIZE, SIZE]);
            assert_eq!(p.upload.format, TexFormat::Rgba8Srgb);
            let px = &p.upload.levels[0];
            assert_eq!(px.len(), (SIZE * SIZE * 4) as usize);
            let opaque = px.chunks_exact(4).filter(|c| c[3] > 200).count();
            let clear = px.chunks_exact(4).filter(|c| c[3] == 0).count();
            assert!(opaque > 1500 && clear > 100, "{name}: {opaque} opaque, {clear} clear");
            let main = g.color().map(|c| (c * 255.0) as i32);
            let inked = px
                .chunks_exact(4)
                .filter(|c| (0..3).all(|k| (i32::from(c[k]) - main[k]).abs() <= 3) && c[3] > 200)
                .count();
            assert!(inked > 120, "{name}: glyph drawn with {inked} pixels");
            let linear = prepare(&format!("{name}{}", crate::material::LINEAR_SUFFIX)).expect("linear");
            assert_eq!(linear.upload.format, TexFormat::Rgba8Linear);
        }
        assert!(prepare("hammerpro/icons/nope").is_none());
        assert!(material("editor/light").is_none());
    }
}
