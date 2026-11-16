use hp_app::settings::{
    CompileSettings, RayConfigSetting, RayDeviceSetting, RayQualitySetting, RayShaftSourceSetting, RayTracingSettings,
};
use serde_json::{Value, json};

use super::tools::{Args, R};

const AUTO: &str = " null or \"auto\": automatic, the value of the quality level.";

enum Kind {
    Int(u32, u32),
    Real(f32, f32),
}

const NUMBERS: &[(&str, Kind, &str)] = &[
    ("rt_samples", Kind::Int(1, 65536), "Ray tracing: smallest number of samples per luxel (-rtsamples)."),
    (
        "rt_max_samples",
        Kind::Int(1, 65536),
        "Ray tracing: largest number of samples per luxel where the noise stays high (-rtmaxsamples).",
    ),
    ("rt_bounces", Kind::Int(0, 64), "Ray tracing: most light bounces (-rtbounces)."),
    (
        "rt_threshold",
        Kind::Real(0.0, 1.0),
        "Ray tracing: adaptive sampling noise threshold, lower is cleaner and slower (-rtthreshold).",
    ),
    (
        "rt_light_size",
        Kind::Real(0.0, 64.0),
        "Ray tracing: scale of the light sizes, bigger gives softer shadows, 0 sharp ones (-rtlightsize).",
    ),
    (
        "rt_sun_angle",
        Kind::Real(0.0, 45.0),
        "Ray tracing: angular size of the sun in degrees for soft sun shadows (-rtsunangle; automatic: the light_environment).",
    ),
    ("rt_sky_samples", Kind::Int(1, 64), "Ray tracing: sky dome samples per bounce (-rtskysamples)."),
    ("rt_denoise", Kind::Real(0.0, 4.0), "Ray tracing: edge aware denoiser strength, 0 (off) to 4 (-rtdenoise)."),
    ("rt_supersample", Kind::Int(1, 8), "Ray tracing: sample points per luxel side, 1 to 8 (-rtsupersample)."),
    (
        "rt_prop_samples",
        Kind::Int(1, 4096),
        "Ray tracing: rays per vertex for the static prop vertex lighting (-rtpropsamples).",
    ),
    (
        "rt_chunk",
        Kind::Int(0, 16_777_216),
        "Ray tracing: luxels per GPU batch, 0 automatic (sized so the driver never times out) (-rtchunk).",
    ),
];

pub(super) const SWITCHES: &[(&str, &str)] = &[
    (
        "rt_ao",
        "Ray tracing: baked ambient occlusion, contact shadows in corners, under and at the base of static props, also in the prop vertex lighting (default true; false is -rtnoao).",
    ),
    (
        "rt_shafts",
        "Ray tracing: light shafts, additive volumetric looking beams baked where the sun and strong spot cones pass through windows, doors and holes; no collision, no shadow, they fade with distance and view angle (default true; false is -rtnoshafts).",
    ),
    (
        "rt_shaft_light_color",
        "Ray tracing: shafts take the color of their light (default true; false is -rtshaftwhite).",
    ),
    ("rt_shaft_dust", "Ray tracing: dust motes drifting in the light shafts (default false, -rtshaftdust)."),
    ("rt_glow", "Ray tracing: glow halos around strong lights (default false, -rtglow)."),
    (
        "rt_cubemaps",
        "Ray tracing: automatic cubemaps (default true; false is -rtnocubemaps). When the map has no env_cubemap of its own, the geometry step places env_cubemap entities where reflective surfaces need them, with the sizes of suggest_cubemaps, in the compiled map only (the VMF is not changed), and sizes the cubemaps from what they reflect (-autocubemaps); a map with its own cubemaps keeps them. buildcubemaps in game still renders them.",
    ),
    (
        "rt_tonemap",
        "Ray tracing: an env_tonemap_controller with sensible bloom and exposure for HDR, only when the map has none (default true; false is -rtnotonemap).",
    ),
    ("rt_color_correction", "Ray tracing: subtle automatic color correction (default false, -rtcolorcorrection)."),
];

pub(super) const EFFECT_NUMBERS: &[(&str, f64, f64, &str)] = &[
    ("rt_ao_strength", 0.0, 1.0, "Ray tracing: ambient occlusion strength (default 0.5, -rtaostrength)."),
    ("rt_ao_radius", 4.0, 1024.0, "Ray tracing: ambient occlusion radius in units (default 48, -rtaoradius)."),
    ("rt_shaft_intensity", 0.0, 8.0, "Ray tracing: light shaft brightness (default 1, -rtshaftintensity)."),
    ("rt_shaft_length", 0.1, 8.0, "Ray tracing: light shaft length multiplier (default 1, -rtshaftlength)."),
    ("rt_shaft_softness", 0.0, 4.0, "Ray tracing: light shaft edge softness (default 1, -rtshaftsoftness)."),
    ("rt_glow_intensity", 0.0, 8.0, "Ray tracing: glow halo brightness (default 1, -rtglowintensity)."),
];

fn switch_slot<'a>(rt: &'a mut RayTracingSettings, key: &str) -> &'a mut bool {
    match key {
        "rt_ao" => &mut rt.ao,
        "rt_shafts" => &mut rt.shafts,
        "rt_shaft_light_color" => &mut rt.shaft_light_color,
        "rt_shaft_dust" => &mut rt.shaft_dust,
        "rt_glow" => &mut rt.glow,
        "rt_cubemaps" => &mut rt.auto_cubemaps,
        "rt_tonemap" => &mut rt.tonemap,
        _ => &mut rt.color_correction,
    }
}

fn effect_slot<'a>(rt: &'a mut RayTracingSettings, key: &str) -> &'a mut f32 {
    match key {
        "rt_ao_strength" => &mut rt.ao_strength,
        "rt_ao_radius" => &mut rt.ao_radius,
        "rt_shaft_intensity" => &mut rt.shaft_intensity,
        "rt_shaft_length" => &mut rt.shaft_length,
        "rt_shaft_softness" => &mut rt.shaft_softness,
        _ => &mut rt.glow_intensity,
    }
}

pub(super) fn schema(key: &str) -> Value {
    if let Some((_, text)) = SWITCHES.iter().find(|(k, _)| *k == key) {
        return json!({ "type": "boolean", "description": text });
    }
    if let Some((_, min, max, text)) = EFFECT_NUMBERS.iter().find(|(k, _, _, _)| *k == key) {
        return json!({ "type": "number", "minimum": min, "maximum": max, "description": text });
    }
    match key {
        "rt_gpu_share" => {
            json!({ "minimum": 0.1, "maximum": 1.0, "description": "Number or \"auto\". Share of the graphics card the ray traced editor preview keeps while you edit: \"auto\" (default, 0.6, keeps the editor at 60 fps or more) or 0.1 to 1 (higher converges faster). Preview only: compiles always use the whole graphics card (-rtgpushare on the preview only). Saved as compile.raytracing.preview_gpu_share (0: auto)." })
        }
        "rt_config" => {
            json!({ "type": "string", "enum": ["default", "optimal", "custom"], "description": "Ray tracing configuration: default (quality normal, every value automatic, the default effects: ambient occlusion, light shafts, automatic cubemaps and tonemap), optimal (like raytracing_optimal) or custom (your values; any other rt_ change switches to it). Turns ray tracing on." })
        }
        "rt_reset" => {
            json!({ "type": "boolean", "description": "true: back to the default ray tracing configuration (Reset to default of the compile window)." })
        }
        "rt_shaft_source" => {
            json!({ "type": "string", "enum": ["sun", "sun_and_spots"], "description": "Ray tracing: lights that make shafts, the sun only or the sun and strong spot lights (default) (-rtshaftsource)." })
        }
        _ => schema_base(key),
    }
}

fn schema_base(key: &str) -> Value {
    match key {
        "raytracing" => {
            json!({ "type": "boolean", "description": "Bake the lighting with Hammer Pro's physically based path tracer (ray tracing, on the GPU with a CPU fallback) instead of the classic radiosity (-raytracing). Off by default; only Hammer Pro's own compiler has it, Valve's VRAD never gets these options." })
        }
        "raytracing_quality" => {
            json!({ "type": "string", "enum": ["draft", "normal", "final", "optimal"], "description": "Ray tracing quality level (-rtquality): draft, normal (default), final, or optimal (the compiler picks the best balance for the map and this computer and logs why)." })
        }
        "raytracing_optimal" => {
            json!({ "type": "boolean", "description": "true does what the Optimal button of the compile window does: ray tracing on, quality optimal, every rt_ parameter back to automatic, and the bsp option autolightmapscale on (finer lightmaps near walkable areas and lights, coarser elsewhere, never above the engine limits)." })
        }
        "rt_bump" => {
            json!({ "type": "boolean", "description": "Ray tracing: bumped lightmaps on normal mapped surfaces (default true; false is -rtnobump)." })
        }
        "rt_device" => {
            json!({ "type": "string", "enum": ["auto", "gpu", "cpu"], "description": "Ray tracing device (-rtdevice): auto (default, the GPU when there is one), gpu or cpu." })
        }
        "rt_seed" => {
            json!({ "type": "integer", "minimum": 0, "maximum": u32::MAX, "description": "Ray tracing random seed (-rtseed, default 0): the same seed and settings give the same lighting." })
        }
        _ => {
            let Some((_, kind, text)) = NUMBERS.iter().find(|(k, _, _)| *k == key) else { return Value::Null };
            let (ty, min, max) = match kind {
                Kind::Int(a, b) => ("integer", f64::from(*a), f64::from(*b)),
                Kind::Real(a, b) => ("number", f64::from(*a), f64::from(*b)),
            };
            json!({ "minimum": min, "maximum": max, "description": format!("{text} Type: {ty}.{AUTO}") })
        }
    }
}

fn auto_requested(a: &Args<'_>, key: &str) -> bool {
    match a.map.get(key) {
        Some(Value::Null) => true,
        Some(Value::String(s)) => s.trim().eq_ignore_ascii_case("auto"),
        _ => false,
    }
}

pub(super) fn apply(s: &mut CompileSettings, a: &Args<'_>) -> R<()> {
    let config = a.opt_enum("rt_config", &["default", "optimal", "custom"])?.and_then(|c| RayConfigSetting::parse(&c));
    if a.opt_bool("rt_reset")? == Some(true) || config == Some(RayConfigSetting::Default) {
        s.raytracing.reset_to_default();
        s.raytracing.on = true;
    }
    if a.opt_bool("raytracing_optimal")? == Some(true) || config == Some(RayConfigSetting::Optimal) {
        hp_ui_mapping::CompileDialog::mcp_raytracing_optimal(s);
    }
    if config == Some(RayConfigSetting::Custom) {
        s.raytracing.config = RayConfigSetting::Custom;
        s.raytracing.on = true;
    }
    match a.map.get("rt_gpu_share") {
        Some(Value::String(t)) if t.trim().eq_ignore_ascii_case("auto") => s.raytracing.preview_gpu_share = 0.0,
        Some(Value::Null) => s.raytracing.preview_gpu_share = 0.0,
        Some(_) => {
            if let Some(v) = a.opt_f64("rt_gpu_share", 0.1, 1.0)? {
                s.raytracing.preview_gpu_share = v as f32;
            }
        }
        None => {}
    }
    let before = s.raytracing.clone();
    let rt = &mut s.raytracing;
    for (key, _) in SWITCHES {
        if let Some(v) = a.opt_bool(key)? {
            *switch_slot(rt, key) = v;
        }
    }
    for (key, min, max, _) in EFFECT_NUMBERS {
        if let Some(v) = a.opt_f64(key, *min, *max)? {
            *effect_slot(rt, key) = v as f32;
        }
    }
    if let Some(v) = a.opt_enum("rt_shaft_source", &["sun", "sun_and_spots"])? {
        rt.shaft_source = RayShaftSourceSetting::parse(&v).unwrap_or_default();
    }
    if let Some(v) = a.opt_bool("raytracing")? {
        rt.on = v;
    }
    if let Some(q) = a.opt_enum("raytracing_quality", &["draft", "normal", "final", "optimal"])? {
        rt.quality = RayQualitySetting::parse(&q).unwrap_or_default();
    }
    if let Some(d) = a.opt_enum("rt_device", &["auto", "gpu", "cpu"])? {
        rt.device = RayDeviceSetting::parse(&d).unwrap_or_default();
    }
    if let Some(v) = a.opt_bool("rt_bump")? {
        rt.bump = v;
    }
    if let Some(v) = a.opt_u64("rt_seed", 0, u64::from(u32::MAX))? {
        rt.seed = v as u32;
    }
    for (key, kind, _) in NUMBERS {
        let auto = auto_requested(a, key);
        match kind {
            Kind::Int(min, max) => {
                let slot = int_slot(rt, key);
                if auto {
                    *slot = None;
                } else if let Some(v) = a.opt_u64(key, u64::from(*min), u64::from(*max))? {
                    *slot = (!(*key == "rt_chunk" && v == 0)).then_some(v as u32);
                }
            }
            Kind::Real(min, max) => {
                let slot = real_slot(rt, key);
                if auto {
                    *slot = None;
                } else if let Some(v) = a.opt_f64(key, f64::from(*min), f64::from(*max))? {
                    *slot = Some(v as f32);
                }
            }
        }
    }
    let touched = NUMBERS
        .iter()
        .map(|(k, _, _)| *k)
        .chain(SWITCHES.iter().map(|(k, _)| *k))
        .chain(EFFECT_NUMBERS.iter().map(|(k, _, _, _)| *k))
        .chain(["raytracing_quality", "rt_device", "rt_bump", "rt_seed", "rt_shaft_source"]);
    if a.get("raytracing").is_none() && touched.clone().any(|k| a.get(k).is_some() && !auto_requested(a, k)) {
        rt.on = true;
    }
    rt.settle_after_change(&before);
    Ok(())
}

fn int_slot<'a>(rt: &'a mut RayTracingSettings, key: &str) -> &'a mut Option<u32> {
    match key {
        "rt_samples" => &mut rt.min_samples,
        "rt_max_samples" => &mut rt.max_samples,
        "rt_bounces" => &mut rt.bounces,
        "rt_sky_samples" => &mut rt.sky_samples,
        "rt_supersample" => &mut rt.supersample,
        "rt_prop_samples" => &mut rt.prop_samples,
        _ => &mut rt.chunk,
    }
}

fn real_slot<'a>(rt: &'a mut RayTracingSettings, key: &str) -> &'a mut Option<f32> {
    match key {
        "rt_threshold" => &mut rt.threshold,
        "rt_light_size" => &mut rt.light_size,
        "rt_sun_angle" => &mut rt.sun_angle,
        _ => &mut rt.denoise,
    }
}

pub(super) fn json(s: &CompileSettings) -> Value {
    let rt = &s.raytracing;
    let quality = hp_bspc::rad::RtQuality::parse(rt.quality.name()).unwrap_or_default();
    let d = hp_bspc::rad::raytrace::Params::of(quality);
    let real = |v: f32| (f64::from(v) * 10000.0).round() / 10000.0;
    json!({
        "raytracing": rt.on,
        "rt_config": rt.config.name(),
        "rt_gpu_share": rt.preview_share().map_or(json!("auto"), |v| json!(real(v))),
        "rt_gpu_share_auto": real(hp_bspc::rad::raytrace::PREVIEW_AUTO_SHARE),
        "raytracing_quality": rt.quality.name(),
        "rt_ao": rt.ao,
        "rt_ao_strength": real(rt.ao_strength),
        "rt_ao_radius": real(rt.ao_radius),
        "rt_shafts": rt.shafts,
        "rt_shaft_intensity": real(rt.shaft_intensity),
        "rt_shaft_length": real(rt.shaft_length),
        "rt_shaft_softness": real(rt.shaft_softness),
        "rt_shaft_light_color": rt.shaft_light_color,
        "rt_shaft_source": rt.shaft_source.name(),
        "rt_shaft_dust": rt.shaft_dust,
        "rt_glow": rt.glow,
        "rt_glow_intensity": real(rt.glow_intensity),
        "rt_cubemaps": rt.auto_cubemaps,
        "rt_tonemap": rt.tonemap,
        "rt_color_correction": rt.color_correction,
        "raytracing_qualities": ["draft", "normal", "final", "optimal"],
        "rt_device": rt.device.name(),
        "rt_seed": rt.seed,
        "rt_bump": rt.bump,
        "rt_samples": rt.min_samples,
        "rt_max_samples": rt.max_samples,
        "rt_bounces": rt.bounces,
        "rt_threshold": rt.threshold.map(real),
        "rt_light_size": rt.light_size.map(real),
        "rt_sun_angle": rt.sun_angle.map(real),
        "rt_sky_samples": rt.sky_samples,
        "rt_denoise": rt.denoise.map(real),
        "rt_supersample": rt.supersample,
        "rt_prop_samples": rt.prop_samples,
        "rt_chunk": rt.chunk,
        "rt_defaults": {
            "note": "values used when an rt_ option is null (automatic), for the chosen quality",
            "rt_samples": d.min_samples,
            "rt_max_samples": d.max_samples,
            "rt_bounces": d.bounces,
            "rt_threshold": real(d.adaptive_threshold),
            "rt_light_size": real(d.light_size_scale),
            "rt_sun_angle": d.sun_angle.map(real),
            "rt_sky_samples": d.sky_samples,
            "rt_denoise": real(d.denoise),
            "rt_supersample": d.supersample,
            "rt_chunk": d.chunk_luxels,
        },
    })
}

#[cfg(test)]
mod tests {
    use serde_json::json;

    use super::super::semantic_tests::{err, ok};

    #[test]
    fn ray_tracing_options_through_mcp() {
        let mut s = hp_app::testing::test_state();
        let o = ok(&mut s, "get_compile_options", json!({}));
        assert_eq!((o["raytracing"].as_bool(), o["raytracing_quality"].as_str()), (Some(false), Some("normal")));
        assert_eq!(o["rt_defaults"]["rt_bounces"].as_u64(), Some(6));
        assert!(o["rt_bounces"].is_null());
        let o = ok(&mut s, "set_compile_options", json!({ "rt_bounces": 4, "rt_denoise": 0.5, "rt_device": "cpu" }));
        assert_eq!(o["raytracing"].as_bool(), Some(true), "any rt_ value turns it on");
        assert_eq!(
            (o["rt_bounces"].as_u64(), o["rt_denoise"].as_f64(), o["rt_device"].as_str()),
            (Some(4), Some(0.5), Some("cpu"))
        );
        let flags = o["rad_options"]["command_flags"].as_str().unwrap_or_default().to_owned();
        assert!(
            flags.contains("-raytracing") && flags.contains("-rtbounces 4") && flags.contains("-rtdevice cpu"),
            "{flags}"
        );
        let bsp = o["bsp_options"]["command_flags"].as_str().unwrap_or_default().to_owned();
        assert!(bsp.contains("-rtautocubemaps"), "automatic cubemaps by default: {bsp}");
        assert_eq!(bsp.matches("-rt").count(), 1, "{bsp}");
        let o = ok(
            &mut s,
            "set_compile_options",
            json!({ "rt_bounces": null, "rt_denoise": "auto", "raytracing_quality": "final" }),
        );
        assert!(o["rt_bounces"].is_null() && o["rt_denoise"].is_null());
        assert_eq!(o["rt_defaults"]["rt_bounces"].as_u64(), Some(12));
        s.settings.compile.raytracing.seed = 3;
        s.settings.compile.raytracing.supersample = Some(6);
        let o = ok(&mut s, "set_compile_options", json!({ "raytracing_optimal": true }));
        assert_eq!((o["raytracing_quality"].as_str(), o["rt_seed"].as_u64()), (Some("optimal"), Some(3)));
        assert!(o["rt_supersample"].is_null());
        assert!(s.settings.compile.bsp_flag_choices.get("-autolightmapscale").is_some_and(|c| c.on));
        let o = ok(&mut s, "set_compile_options", json!({ "raytracing": false }));
        assert_eq!(o["raytracing"].as_bool(), Some(false));
        assert!(!o["rad_options"]["command_flags"].as_str().unwrap_or_default().contains("-raytracing"));
        err(&mut s, "set_compile_options", json!({ "rt_supersample": 9 }));
        err(&mut s, "set_compile_options", json!({ "raytracing_quality": "best" }));
        err(&mut s, "set_compile_options", json!({ "rad_options": { "rtbounces": 3 } }));
        let o = ok(&mut s, "set_compile_options", json!({ "rt_reset": true }));
        assert_eq!((o["rt_config"].as_str(), o["raytracing"].as_bool()), (Some("default"), Some(true)));
        assert_eq!(
            (o["rt_ao"].as_bool(), o["rt_shafts"].as_bool(), o["rt_glow"].as_bool()),
            (Some(true), Some(true), Some(false))
        );
        let o = ok(
            &mut s,
            "set_compile_options",
            json!({ "rt_glow": true, "rt_glow_intensity": 2.5, "rt_shaft_source": "sun", "rt_ao": false, "rt_ao_radius": 64 }),
        );
        assert_eq!(o["rt_config"].as_str(), Some("custom"));
        let flags = o["rad_options"]["command_flags"].as_str().unwrap_or_default().to_owned();
        for w in ["-rtglow", "-rtglowintensity 2.5", "-rtshaftsource sun", "-rtnoao", "-rtaoradius 64"] {
            assert!(flags.contains(w), "{w} in {flags}");
        }
        let o = ok(&mut s, "set_compile_options", json!({ "rt_config": "optimal" }));
        assert_eq!((o["rt_config"].as_str(), o["raytracing_quality"].as_str()), (Some("optimal"), Some("optimal")));
        assert_eq!(o["rt_glow"].as_bool(), Some(false));
        let o = ok(&mut s, "set_compile_options", json!({ "rt_config": "default" }));
        assert_eq!((o["rt_config"].as_str(), o["raytracing_quality"].as_str()), (Some("default"), Some("normal")));
        let o = ok(&mut s, "set_compile_options", json!({ "rt_gpu_share": 0.4 }));
        assert_eq!((o["rt_gpu_share"].as_f64(), o["rt_config"].as_str()), (Some(0.4), Some("default")));
        assert!(
            !o["rad_options"]["command_flags"].as_str().unwrap_or_default().contains("-rtgpushare"),
            "never in the compile"
        );
        let preview = hp_ui_mapping::raytracing_preview_args(&s.settings.compile);
        assert!(preview.windows(2).any(|w| w[0] == "-rtgpushare" && w[1] == "0.4"), "{preview:?}");
        let o = ok(&mut s, "set_compile_options", json!({ "rt_gpu_share": "auto" }));
        assert_eq!(o["rt_gpu_share"].as_str(), Some("auto"));
        assert!(!hp_ui_mapping::raytracing_preview_args(&s.settings.compile).contains(&"-rtgpushare".to_owned()));
        err(&mut s, "set_compile_options", json!({ "rt_gpu_share": 0.05 }));
        assert_eq!(o["rt_gpu_share_auto"].as_f64(), Some(0.6), "the editor shows the engine's automatic share");
        err(&mut s, "set_compile_options", json!({ "rt_shaft_length": 9 }));
        err(&mut s, "set_compile_options", json!({ "rt_config": "best" }));
        for key in [
            "raytracing",
            "rt_samples",
            "rt_seed",
            "rt_device",
            "raytracing_optimal",
            "rt_config",
            "rt_reset",
            "rt_glow",
            "rt_ao_radius",
            "rt_shaft_source",
        ] {
            assert!(!super::schema(key).is_null(), "{key}");
        }
    }
}
