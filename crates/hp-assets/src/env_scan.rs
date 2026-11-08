use hp_editor::Document;
use hp_render::{SkyPaint, ToneEnv};
use hp_vmf::Entity;

pub(crate) const PAINTED_SKY: &str = "painted";

#[derive(Clone, Debug, PartialEq)]
pub(crate) struct SkyPaintSpec {
    pub paint: SkyPaint,
    pub star_texture: String,
    pub sun_from_env_sun: bool,
}

#[derive(Clone, Debug, PartialEq)]
pub(crate) struct SunGlowSpec {
    pub direction: [f32; 3],
    pub size: f32,
    pub overlay_size: f32,
    pub color: [f32; 3],
    pub overlay_color: [f32; 3],
    pub material: String,
    pub overlay_material: String,
    pub hdr_scale: f32,
    pub on: bool,
}

pub(crate) const GLOW_MATERIAL: &str = "sprites/light_glow02_add_noz";

#[derive(Clone, Debug, Default, PartialEq)]
pub(crate) struct EnvScan {
    pub sky_paint: Option<SkyPaintSpec>,
    pub env_sun: Option<[f32; 3]>,
    pub sun_glow: Option<SunGlowSpec>,
    pub tone: ToneEnv,
}

fn glow_direction(e: &Entity) -> [f32; 3] {
    let d = direction_of(e);
    [-d[0], -d[1], d[2]]
}

fn normalized_hue(c: [f32; 3]) -> [f32; 3] {
    let m = c[0].max(c[1]).max(c[2]);
    if m <= 0.0 { [1.0, 1.0, 1.0] } else { c.map(|v| v / m) }
}

fn glow_from_keys(e: &Entity) -> SunGlowSpec {
    let name = |key: &str| {
        e.get(key)
            .map(str::trim)
            .filter(|v| !v.is_empty())
            .map(str::to_owned)
            .unwrap_or_else(|| GLOW_MATERIAL.to_owned())
    };
    let render = e.get("rendercolor").and_then(vec3).unwrap_or([255.0, 255.0, 255.0]);
    let overlay = e.get("overlaycolor").and_then(vec3).unwrap_or([0.0, 0.0, 0.0]);
    let main = normalized_hue(render);
    let size = e.get("size").and_then(scalar).unwrap_or(16.0);
    let overlay_size = e.get("overlaysize").and_then(scalar).unwrap_or(-1.0);
    let disabled = e.get("StartDisabled").or_else(|| e.get("startdisabled")).and_then(scalar).unwrap_or(0.0);
    SunGlowSpec {
        direction: glow_direction(e),
        size,
        overlay_size,
        color: main,
        overlay_color: if overlay == [0.0, 0.0, 0.0] { main } else { overlay.map(|v| v / 255.0) },
        material: name("material"),
        overlay_material: name("overlaymaterial"),
        hdr_scale: e.get("HDRColorScale").or_else(|| e.get("hdrcolorscale")).and_then(scalar).unwrap_or(1.0),
        on: disabled == 0.0,
    }
}

fn floats(s: &str) -> Vec<f32> {
    s.split_ascii_whitespace().filter_map(|t| t.parse::<f32>().ok()).filter(|v| v.is_finite()).collect()
}

fn vec3(s: &str) -> Option<[f32; 3]> {
    let v = floats(s);
    (v.len() >= 3).then(|| [v[0], v[1], v[2]])
}

fn scalar(s: &str) -> Option<f32> {
    floats(s).first().copied()
}

fn color255(s: &str) -> Option<[f32; 3]> {
    vec3(s).map(|c| c.map(|v| v / 255.0))
}

pub(crate) fn is_painted(skyname: &str) -> bool {
    skyname.trim().eq_ignore_ascii_case(PAINTED_SKY)
}

fn paint_from_keys(e: &Entity) -> SkyPaintSpec {
    let mut spec = SkyPaintSpec {
        paint: SkyPaint::default(),
        star_texture: "skybox/starfield".to_owned(),
        sun_from_env_sun: false,
    };
    for (k, v) in &e.props {
        apply_paint_key(&mut spec, &k.to_ascii_lowercase(), v, false);
    }
    spec
}

fn apply_paint_key(spec: &mut SkyPaintSpec, key: &str, value: &str, color_is_255: bool) {
    let p = &mut spec.paint;
    let color = |v: &str| if color_is_255 { color255(v) } else { vec3(v) };
    match key {
        "topcolor" => {
            if let Some(c) = color(value) {
                p.top = c;
            }
        }
        "bottomcolor" => {
            if let Some(c) = color(value) {
                p.bottom = c;
            }
        }
        "fadebias" => p.fade_bias = scalar(value).unwrap_or(p.fade_bias),
        "hdrscale" => p.hdr_scale = scalar(value).unwrap_or(p.hdr_scale),
        "sunsize" => p.sun_size = scalar(value).unwrap_or(p.sun_size),
        "suncolor" => {
            if let Some(c) = color(value) {
                p.sun_color = c;
            }
        }
        "sunnormal" => {
            if let Some(c) = vec3(value) {
                p.sun_dir = c;
            }
        }
        "sunposmethod" => spec.sun_from_env_sun = scalar(value).is_some_and(|v| v as i32 == 1),
        "duskscale" => p.dusk_scale = scalar(value).unwrap_or(p.dusk_scale),
        "duskintensity" => p.dusk_intensity = scalar(value).unwrap_or(p.dusk_intensity),
        "duskcolor" => {
            if let Some(c) = color(value) {
                p.dusk_color = c;
            }
        }
        "drawstars" => p.draw_stars = scalar(value).is_some_and(|v| v != 0.0),
        "starlayers" => p.star_layers = scalar(value).map_or(p.star_layers, |v| v.clamp(1.0, 3.0) as u32),
        "starscale" => p.star_scale = scalar(value).unwrap_or(p.star_scale),
        "starfade" => p.star_fade = scalar(value).unwrap_or(p.star_fade),
        "starspeed" => p.star_speed = scalar(value).unwrap_or(p.star_speed),
        "startexture" if !value.trim().is_empty() => spec.star_texture = value.trim().to_owned(),
        _ => {}
    }
}

fn paint_input(spec: &mut SkyPaintSpec, input: &str, param: &str) {
    let key = match input.to_ascii_lowercase().as_str() {
        "settopcolor" => "topcolor",
        "setbottomcolor" => "bottomcolor",
        "setfadebias" => "fadebias",
        "setsunsize" => "sunsize",
        "setsunnormal" => "sunnormal",
        "setsuncolor" => "suncolor",
        "setduskscale" => "duskscale",
        "setduskintensity" => "duskintensity",
        "setduskcolor" => "duskcolor",
        "startexture" => "startexture",
        "starscale" => "starscale",
        "starfade" => "starfade",
        "starspeed" => "starspeed",
        "starlayers" => "starlayers",
        "hdrscale" => "hdrscale",
        _ => return,
    };
    let is_color = matches!(key, "topcolor" | "bottomcolor" | "suncolor" | "duskcolor");
    if key == "sunnormal" {
        spec.sun_from_env_sun = false;
    }
    apply_paint_key(spec, key, param, is_color);
}

fn tone_input(tone: &mut ToneEnv, input: &str, param: &str) {
    match input.to_ascii_lowercase().as_str() {
        "setautoexposuremin" => tone.exposure_min = scalar(param).filter(|v| *v >= 0.0),
        "setautoexposuremax" => tone.exposure_max = scalar(param).filter(|v| *v >= 0.0),
        "usedefaultautoexposure" => {
            tone.exposure_min = None;
            tone.exposure_max = None;
        }
        "setbloomscale" => tone.bloom_scale = scalar(param).filter(|v| *v >= 0.0),
        "usedefaultbloomscale" => tone.bloom_scale = None,
        _ => {}
    }
}

fn direction_of(e: &Entity) -> [f32; 3] {
    let angles = e.angles().filter(|a| a.is_finite()).unwrap_or_default();
    let pitch = e.get("pitch").and_then(scalar).filter(|p| *p != 0.0).unwrap_or(angles.x as f32);
    let (p, y) = (pitch.to_radians(), (angles.y as f32).to_radians());
    [p.cos() * y.cos(), p.cos() * y.sin(), -p.sin()]
}

pub(crate) fn scan_entities<'a>(entities: impl Iterator<Item = &'a Entity> + Clone) -> EnvScan {
    let mut out = EnvScan::default();
    let mut paint: Option<(&Entity, SkyPaintSpec)> = None;
    let mut tonemap: Option<&Entity> = None;
    for e in entities.clone() {
        let class = e.classname.trim();
        if class.eq_ignore_ascii_case("env_skypaint") && paint.is_none() {
            paint = Some((e, paint_from_keys(e)));
        } else if class.eq_ignore_ascii_case("env_tonemap_controller") && tonemap.is_none() {
            tonemap = Some(e);
        } else if class.eq_ignore_ascii_case("env_sun") && out.env_sun.is_none() {
            out.env_sun = Some(direction_of(e));
            out.sun_glow = Some(glow_from_keys(e));
        }
    }
    let paint_name = paint.as_ref().and_then(|(e, _)| e.get("targetname")).map(str::trim).filter(|n| !n.is_empty());
    let tone_name = tonemap.and_then(|e| e.get("targetname")).map(str::trim).filter(|n| !n.is_empty());
    for e in entities {
        for c in &e.connections {
            if !c.output.trim().eq_ignore_ascii_case("OnMapSpawn") {
                continue;
            }
            let target = c.target.trim();
            if let (Some(name), Some((_, spec))) = (paint_name, paint.as_mut())
                && target.eq_ignore_ascii_case(name)
            {
                paint_input(spec, &c.input, &c.param);
            }
            if let Some(name) = tone_name
                && target.eq_ignore_ascii_case(name)
            {
                tone_input(&mut out.tone, &c.input, &c.param);
            }
        }
    }
    if let Some((_, mut spec)) = paint {
        if spec.sun_from_env_sun
            && let Some(dir) = out.env_sun
        {
            spec.paint.sun_dir = dir;
        }
        out.sky_paint = Some(spec);
    }
    out
}

pub(crate) fn scan(doc: &Document) -> EnvScan {
    let options = doc.light_plus();
    scan_entities(doc.scene().entities().values().filter(|node| options.uses_entity(node.entity.id)).map(|n| &n.entity))
}

#[cfg(test)]
mod tests {
    use super::*;

    fn ent(class: &str, kv: &[(&str, &str)], out: &[(&str, &str, &str, &str)]) -> Entity {
        let mut e = Entity { classname: class.to_owned(), ..Entity::default() };
        for (k, v) in kv {
            e.set(k, *v);
        }
        for (o, t, i, p) in out {
            e.connections.push(hp_vmf::Connection {
                output: (*o).to_owned(),
                target: (*t).to_owned(),
                input: (*i).to_owned(),
                param: (*p).to_owned(),
                ..hp_vmf::Connection::default()
            });
        }
        e
    }

    #[test]
    fn the_sky_paint_defaults_are_the_entity_defaults() {
        let e = [ent("env_skypaint", &[], &[])];
        let s = scan_entities(e.iter()).sky_paint.expect("paint");
        assert_eq!(s.paint.top, [0.2, 0.5, 1.0]);
        assert_eq!(s.paint.bottom, [0.8, 1.0, 1.0]);
        assert_eq!((s.paint.fade_bias, s.paint.hdr_scale, s.paint.dusk_intensity), (1.0, 0.66, 1.0));
        assert_eq!(s.star_texture, "skybox/starfield");
    }

    #[test]
    fn the_env_sun_glow_follows_the_entity_keys_like_the_game() {
        let sun = ent(
            "env_sun",
            &[
                ("pitch", "-50"),
                ("angles", "0 306 0"),
                ("size", "16"),
                ("overlaysize", "-1"),
                ("rendercolor", "241 240 199"),
                ("overlaycolor", "244 242 236"),
                ("HDRColorScale", "1.0"),
            ],
            &[],
        );
        let list = [sun];
        let g = scan_entities(list.iter()).sun_glow.expect("glow");
        assert!((g.direction[0] + 0.3778).abs() < 1e-3, "{:?}", g.direction);
        assert!((g.direction[1] - 0.5200).abs() < 1e-3, "{:?}", g.direction);
        assert!((g.direction[2] - 0.7660).abs() < 1e-3, "{:?}", g.direction);
        assert_eq!((g.size, g.overlay_size), (16.0, -1.0));
        assert!((g.color[0] - 1.0).abs() < 1e-6 && (g.color[2] - 199.0 / 241.0).abs() < 1e-6, "{:?}", g.color);
        assert!((g.overlay_color[0] - 244.0 / 255.0).abs() < 1e-6, "{:?}", g.overlay_color);
        assert_eq!((g.material.as_str(), g.hdr_scale, g.on), (GLOW_MATERIAL, 1.0, true));
    }

    #[test]
    fn an_env_sun_without_keys_takes_the_game_defaults() {
        let list = [ent("env_sun", &[("pitch", "-90")], &[])];
        let g = scan_entities(list.iter()).sun_glow.expect("glow");
        assert_eq!((g.size, g.color, g.overlay_color), (16.0, [1.0; 3], [1.0; 3]));
        assert_eq!((g.material.as_str(), g.overlay_material.as_str()), (GLOW_MATERIAL, GLOW_MATERIAL));
        assert!(g.direction[2] > 0.999, "{:?}", g.direction);
    }

    #[test]
    fn keys_set_the_paint_and_map_spawn_inputs_follow() {
        let sky = ent(
            "env_skypaint",
            &[
                ("targetname", "sky"),
                ("topcolor", "0.22 0.51 1.00"),
                ("hdrscale", "0.56"),
                ("fadebias", "0.1"),
                ("sunposmethod", "1"),
                ("drawstars", "1"),
                ("starlayers", "2"),
                ("startexture", "skybox/clouds"),
            ],
            &[],
        );
        let sun = ent("env_sun", &[("pitch", "-50"), ("angles", "0 90 0")], &[]);
        let auto = ent(
            "logic_auto",
            &[],
            &[("OnMapSpawn", "sky", "SetBottomColor", "255 0 51"), ("OnMapSpawn", "sky", "SetDuskScale", "0.5")],
        );
        let list = [sky, sun, auto];
        let scan = scan_entities(list.iter());
        let s = scan.sky_paint.expect("paint");
        assert_eq!(s.paint.top, [0.22, 0.51, 1.0]);
        assert_eq!((s.paint.hdr_scale, s.paint.fade_bias, s.paint.dusk_scale), (0.56, 0.1, 0.5));
        assert_eq!(s.paint.bottom, [1.0, 0.0, 0.2]);
        assert_eq!((s.paint.draw_stars, s.paint.star_layers, s.star_texture.as_str()), (true, 2, "skybox/clouds"));
        let d = s.paint.sun_dir;
        assert!(d[2] > 0.76 && d[2] < 0.77 && d[0].abs() < 0.43 && d[1] > 0.4, "{d:?}");
    }

    #[test]
    fn a_custom_sun_normal_beats_the_env_sun_after_the_input() {
        let sky = ent("env_skypaint", &[("targetname", "s"), ("sunposmethod", "1")], &[]);
        let sun = ent("env_sun", &[("pitch", "-10")], &[]);
        let auto = ent("logic_auto", &[], &[("OnMapSpawn", "s", "SetSunNormal", "0 1 0")]);
        let list = [sky, sun, auto];
        let s = scan_entities(list.iter()).sky_paint.expect("paint");
        assert_eq!(s.paint.sun_dir, [0.0, 1.0, 0.0]);
    }

    #[test]
    fn the_tonemap_controller_inputs_set_the_exposure_range_and_the_bloom() {
        let tone = ent("env_tonemap_controller", &[("targetname", "TonemapController")], &[]);
        let auto = ent(
            "logic_auto",
            &[],
            &[
                ("OnMapSpawn", "TonemapController", "SetAutoExposureMax", "1.8"),
                ("OnMapSpawn", "tonemapcontroller", "SetAutoExposureMin", "0.7"),
                ("OnMapSpawn", "TonemapController", "SetBloomScale", "0.3"),
                ("OnMapSpawn", "TonemapController", "SetTonemapRate", "0.2"),
                ("OnTrigger", "TonemapController", "SetBloomScale", "9"),
            ],
        );
        let list = [tone, auto];
        let t = scan_entities(list.iter()).tone;
        assert_eq!((t.exposure_min, t.exposure_max, t.bloom_scale), (Some(0.7), Some(1.8), Some(0.3)));
        let reset = ent("logic_auto", &[], &[("OnMapSpawn", "TonemapController", "UseDefaultBloomScale", "")]);
        let list = [list[0].clone(), list[1].clone(), reset];
        assert_eq!(scan_entities(list.iter()).tone.bloom_scale, None);
    }

    #[test]
    fn maps_without_those_entities_have_nothing() {
        let list = [ent("light", &[], &[])];
        assert_eq!(scan_entities(list.iter()), EnvScan::default());
        assert!(is_painted(" Painted ") && !is_painted("sky_day01_01"));
    }
}
