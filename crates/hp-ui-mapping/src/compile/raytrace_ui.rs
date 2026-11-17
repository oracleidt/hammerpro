use egui::{RichText, Ui};
use hp_app::settings::{
    CompileSettings, FlagChoiceSetting, RayConfigSetting, RayDeviceSetting, RayQualitySetting, RayShaftSourceSetting,
};
use hp_app::{t, tf, theme};
use hp_compile::RayTracingOptions;
use hp_compile::raytracing::RayEffectsOptions;
use hp_raytrace::device::{NO_GPU_MESSAGE_FR, NO_GPU_MESSAGE_RU};
use hp_raytrace::{NO_GPU_MESSAGE, Params, Quality};

use super::run::RunState;

pub(crate) const AUTO_LIGHTMAP_SCALE: &str = "-autolightmapscale";

pub(crate) fn engine_quality(q: RayQualitySetting) -> Quality {
    match q {
        RayQualitySetting::Draft => Quality::Draft,
        RayQualitySetting::Normal => Quality::Normal,
        RayQualitySetting::Final => Quality::Final,
        RayQualitySetting::Optimal => Quality::Optimal,
    }
}

pub(crate) fn defaults(q: RayQualitySetting) -> Params {
    Params::of(engine_quality(q))
}

pub(crate) fn plan_options(s: &CompileSettings) -> Option<RayTracingOptions> {
    let rt = &s.raytracing;
    rt.on.then(|| RayTracingOptions {
        quality: rt.quality.name().to_owned(),
        device: rt.device.name().to_owned(),
        seed: rt.seed,
        min_samples: rt.min_samples,
        max_samples: rt.max_samples,
        bounces: rt.bounces,
        threshold: rt.threshold,
        light_size: rt.light_size,
        sun_angle: rt.sun_angle,
        sky_samples: rt.sky_samples,
        denoise: rt.denoise,
        supersample: rt.supersample,
        bump: rt.bump,
        prop_samples: rt.prop_samples,
        chunk: rt.chunk,
        luxel_detail: rt.luxel_detail,
        look: rt.look.clone(),
        lang: hp_app::i18n::lang().code().to_owned(),
        effects: RayEffectsOptions {
            ao: rt.ao,
            ao_strength: rt.ao_strength,
            ao_radius: rt.ao_radius,
            shafts: rt.shafts,
            shaft_intensity: rt.shaft_intensity,
            shaft_length: rt.shaft_length,
            shaft_softness: rt.shaft_softness,
            shaft_light_color: rt.shaft_light_color,
            shaft_source: rt.shaft_source.name().to_owned(),
            shaft_dust: rt.shaft_dust,
            glow: rt.glow,
            glow_intensity: rt.glow_intensity,
            auto_cubemaps: rt.auto_cubemaps,
            reflections: "auto".to_owned(),
            tonemap: rt.tonemap,
            color_correction: rt.color_correction,
        },
    })
}

pub(crate) fn apply_config(s: &mut CompileSettings, c: RayConfigSetting) {
    match c {
        RayConfigSetting::Default => s.raytracing.reset_to_default(),
        RayConfigSetting::Optimal => apply_optimal(s),
        RayConfigSetting::Custom => s.raytracing.config = RayConfigSetting::Custom,
    }
}

pub(crate) fn apply_optimal(s: &mut CompileSettings) {
    s.raytracing.make_optimal();
    super::options_ui::set_choice(
        &mut s.bsp_flag_choices,
        AUTO_LIGHTMAP_SCALE,
        Some(FlagChoiceSetting { on: true, value: String::new() }),
    );
}

pub(crate) fn is_no_gpu_line(text: &str) -> bool {
    [NO_GPU_MESSAGE, NO_GPU_MESSAGE_FR, NO_GPU_MESSAGE_RU].iter().any(|m| text.contains(m))
}

pub(crate) fn no_gpu_banner(ui: &mut Ui, run: &RunState) {
    if !run.no_gpu {
        return;
    }
    let color = theme::warning();
    ui.add_space(4.0);
    egui::Frame::new()
        .fill(color.gamma_multiply(0.15))
        .stroke(egui::Stroke::new(1.0, color.gamma_multiply(0.6)))
        .corner_radius(4.0)
        .inner_margin(egui::Margin::symmetric(10, 6))
        .show(ui, |ui| {
            ui.horizontal_wrapped(|ui| {
                ui.label(RichText::new("⚠").color(color).size(16.0));
                ui.label(RichText::new(t!("raytrace.no_gpu")).color(color).strong())
                    .on_hover_text(t!("raytrace.no_gpu_tip"));
            });
        });
}

fn quality_label(q: RayQualitySetting) -> &'static str {
    match q {
        RayQualitySetting::Draft => t!("raytrace.quality.draft"),
        RayQualitySetting::Normal => t!("raytrace.quality.normal"),
        RayQualitySetting::Final => t!("raytrace.quality.final"),
        RayQualitySetting::Optimal => t!("raytrace.quality.optimal"),
    }
}

fn quality_tip(q: RayQualitySetting) -> &'static str {
    match q {
        RayQualitySetting::Draft => t!("raytrace.quality.draft_tip"),
        RayQualitySetting::Normal => t!("raytrace.quality.normal_tip"),
        RayQualitySetting::Final => t!("raytrace.quality.final_tip"),
        RayQualitySetting::Optimal => t!("raytrace.quality.optimal_tip"),
    }
}

fn config_label(c: RayConfigSetting) -> &'static str {
    match c {
        RayConfigSetting::Default => t!("raytrace.config.default"),
        RayConfigSetting::Optimal => t!("raytrace.config.optimal"),
        RayConfigSetting::Custom => t!("raytrace.config.custom"),
    }
}

fn config_tip(c: RayConfigSetting) -> &'static str {
    match c {
        RayConfigSetting::Default => t!("raytrace.config.default_tip"),
        RayConfigSetting::Optimal => t!("raytrace.optimal_tip"),
        RayConfigSetting::Custom => t!("raytrace.config.custom_tip"),
    }
}

fn shaft_source_label(v: RayShaftSourceSetting) -> &'static str {
    match v {
        RayShaftSourceSetting::Sun => t!("raytrace.shaft_source.sun"),
        RayShaftSourceSetting::SunAndSpots => t!("raytrace.shaft_source.sun_and_spots"),
    }
}

fn device_label(d: RayDeviceSetting) -> &'static str {
    match d {
        RayDeviceSetting::Auto => t!("raytrace.device.auto"),
        RayDeviceSetting::Gpu => t!("raytrace.device.gpu"),
        RayDeviceSetting::Cpu => t!("raytrace.device.cpu"),
    }
}

fn quality_combo(ui: &mut Ui, q: &mut RayQualitySetting, salt: &str) {
    egui::ComboBox::from_id_salt((salt, "rt_quality"))
        .selected_text(quality_label(*q))
        .show_ui(ui, |ui| {
            for v in RayQualitySetting::ALL {
                ui.selectable_value(q, v, quality_label(v)).on_hover_text(quality_tip(v));
            }
        })
        .response
        .on_hover_text(format!("{}\n\n{}", t!("raytrace.quality_tip"), quality_tip(*q)));
}

pub(crate) fn toggle_row(ui: &mut Ui, s: &mut CompileSettings) {
    let before = s.raytracing.clone();
    ui.horizontal(|ui| {
        ui.checkbox(&mut s.raytracing.on, t!("raytrace.row")).on_hover_text(t!("raytrace.enable_tip"));
        ui.add_enabled_ui(s.raytracing.on, |ui| {
            quality_combo(ui, &mut s.raytracing.quality, "hp_compile_rt_row");
        });
    });
    s.raytracing.settle_after_change(&before);
}

fn real(ui: &mut Ui, value: &mut f32, range: std::ops::RangeInclusive<f32>, speed: f64, tip: &str) {
    ui.add(egui::DragValue::new(value).range(range).speed(speed).max_decimals(2)).on_hover_text(tip);
}

fn effects_grid(ui: &mut Ui, s: &mut CompileSettings, salt: &str) {
    let rt = &mut s.raytracing;
    ui.add_space(4.0);
    ui.label(RichText::new(t!("raytrace.effects")).strong()).on_hover_text(t!("raytrace.effects_tip"));
    egui::Grid::new((salt, "rt_effects")).num_columns(2).spacing([12.0, 4.0]).striped(true).show(ui, |ui| {
        let tip = t!("raytrace.ao_tip");
        label(ui, "raytrace.ao", tip);
        ui.checkbox(&mut rt.ao, t!("raytrace.on")).on_hover_text(tip);
        ui.end_row();
        let on = rt.ao;
        let tip = t!("raytrace.ao_strength_tip");
        label(ui, "raytrace.ao_strength", tip);
        ui.add_enabled_ui(on, |ui| real(ui, &mut rt.ao_strength, 0.0..=1.0, 0.01, tip));
        ui.end_row();
        let tip = t!("raytrace.ao_radius_tip");
        label(ui, "raytrace.ao_radius", tip);
        ui.add_enabled_ui(on, |ui| real(ui, &mut rt.ao_radius, 4.0..=1024.0, 1.0, tip));
        ui.end_row();
        let tip = t!("raytrace.shafts_tip");
        label(ui, "raytrace.shafts", tip);
        ui.checkbox(&mut rt.shafts, t!("raytrace.on")).on_hover_text(tip);
        ui.end_row();
        let on = rt.shafts;
        let tip = t!("raytrace.shaft_intensity_tip");
        label(ui, "raytrace.shaft_intensity", tip);
        ui.add_enabled_ui(on, |ui| real(ui, &mut rt.shaft_intensity, 0.0..=8.0, 0.02, tip));
        ui.end_row();
        let tip = t!("raytrace.shaft_length_tip");
        label(ui, "raytrace.shaft_length", tip);
        ui.add_enabled_ui(on, |ui| real(ui, &mut rt.shaft_length, 0.1..=8.0, 0.02, tip));
        ui.end_row();
        let tip = t!("raytrace.shaft_softness_tip");
        label(ui, "raytrace.shaft_softness", tip);
        ui.add_enabled_ui(on, |ui| real(ui, &mut rt.shaft_softness, 0.0..=4.0, 0.02, tip));
        ui.end_row();
        let tip = t!("raytrace.shaft_light_color_tip");
        label(ui, "raytrace.shaft_light_color", tip);
        ui.add_enabled_ui(on, |ui| ui.checkbox(&mut rt.shaft_light_color, t!("raytrace.on")).on_hover_text(tip));
        ui.end_row();
        let tip = t!("raytrace.shaft_source_tip");
        label(ui, "raytrace.shaft_source", tip);
        ui.add_enabled_ui(on, |ui| {
            egui::ComboBox::from_id_salt((salt, "rt_shaft_source"))
                .selected_text(shaft_source_label(rt.shaft_source))
                .show_ui(ui, |ui| {
                    for v in RayShaftSourceSetting::ALL {
                        ui.selectable_value(&mut rt.shaft_source, v, shaft_source_label(v));
                    }
                })
                .response
                .on_hover_text(tip);
        });
        ui.end_row();
        let tip = t!("raytrace.shaft_dust_tip");
        label(ui, "raytrace.shaft_dust", tip);
        ui.add_enabled_ui(on, |ui| ui.checkbox(&mut rt.shaft_dust, t!("raytrace.on")).on_hover_text(tip));
        ui.end_row();
        let tip = t!("raytrace.glow_tip");
        label(ui, "raytrace.glow", tip);
        ui.checkbox(&mut rt.glow, t!("raytrace.on")).on_hover_text(tip);
        ui.end_row();
        let on = rt.glow;
        let tip = t!("raytrace.glow_intensity_tip");
        label(ui, "raytrace.glow_intensity", tip);
        ui.add_enabled_ui(on, |ui| real(ui, &mut rt.glow_intensity, 0.0..=8.0, 0.02, tip));
        ui.end_row();
        let tip = t!("raytrace.cubemaps_tip");
        label(ui, "raytrace.cubemaps", tip);
        ui.checkbox(&mut rt.auto_cubemaps, t!("raytrace.on")).on_hover_text(tip);
        ui.end_row();
        let tip = t!("raytrace.tonemap_tip");
        label(ui, "raytrace.tonemap", tip);
        ui.checkbox(&mut rt.tonemap, t!("raytrace.on")).on_hover_text(tip);
        ui.end_row();
        let tip = t!("raytrace.color_correction_tip");
        label(ui, "raytrace.color_correction", tip);
        ui.checkbox(&mut rt.color_correction, t!("raytrace.on")).on_hover_text(tip);
        ui.end_row();
    });
}

fn default_text(value: String) -> String {
    tf!("raytrace.default", value)
}

fn number(v: f32) -> String {
    let text = format!("{v:.3}");
    text.trim_end_matches('0').trim_end_matches('.').to_owned()
}

fn auto_box(ui: &mut Ui, auto: &mut bool) -> bool {
    ui.checkbox(auto, t!("raytrace.auto")).on_hover_text(t!("raytrace.auto_tip")).changed()
}

fn opt_u32(
    ui: &mut Ui,
    value: &mut Option<u32>,
    fallback: u32,
    range: std::ops::RangeInclusive<u32>,
    tip: &str,
    default: &str,
) {
    ui.horizontal(|ui| {
        let mut auto = value.is_none();
        if auto_box(ui, &mut auto) {
            *value = (!auto).then_some(fallback.clamp(*range.start(), *range.end()));
        }
        match value {
            Some(v) => {
                ui.add(egui::DragValue::new(v).range(range).speed(1.0)).on_hover_text(tip);
            }
            None => {
                ui.label(RichText::new(default).weak().italics()).on_hover_text(tip);
            }
        }
    });
}

fn opt_f32(
    ui: &mut Ui,
    value: &mut Option<f32>,
    fallback: f32,
    range: std::ops::RangeInclusive<f32>,
    speed: f64,
    tip: &str,
    default: &str,
) {
    ui.horizontal(|ui| {
        let mut auto = value.is_none();
        if auto_box(ui, &mut auto) {
            *value = (!auto).then_some(fallback.clamp(*range.start(), *range.end()));
        }
        match value {
            Some(v) => {
                ui.add(egui::DragValue::new(v).range(range).speed(speed).max_decimals(3)).on_hover_text(tip);
            }
            None => {
                ui.label(RichText::new(default).weak().italics()).on_hover_text(tip);
            }
        }
    });
}

fn label(ui: &mut Ui, key: &str, tip: &str) {
    ui.label(t!(key)).on_hover_text(tip);
}

pub fn preview_args(s: &CompileSettings) -> Vec<String> {
    let mut on = s.clone();
    on.raytracing.on = true;
    let mut args = plan_options(&on).map(|o| o.args()).unwrap_or_default();
    args.extend(hp_compile::raytracing::gpu_share_args(s.raytracing.preview_share()));
    args
}

fn gpu_share_row(ui: &mut Ui, share: &mut f32) {
    let tip = t!("raytrace.gpu_share_tip");
    ui.horizontal(|ui| {
        let mut auto = !(share.is_finite() && *share > 0.0);
        if auto_box(ui, &mut auto) {
            *share = if auto { 0.0 } else { AUTO_GPU_SHARE };
        }
        if auto {
            ui.label(RichText::new(default_text(format!("{:.0}%", AUTO_GPU_SHARE * 100.0))).weak().italics())
                .on_hover_text(tip);
        } else {
            *share = share.clamp(0.1, 1.0);
            ui.add(
                egui::Slider::new(share, 0.1..=1.0)
                    .custom_formatter(|v, _| format!("{:.0}%", v * 100.0))
                    .custom_parser(|t| t.trim().trim_end_matches('%').trim().parse::<f64>().ok().map(|v| v / 100.0)),
            )
            .on_hover_text(tip);
        }
    });
}

pub(crate) const AUTO_GPU_SHARE: f32 = 0.6;

pub(crate) fn preview_section(ui: &mut Ui, s: &mut CompileSettings, salt: &str) {
    body(ui, s, salt, false);
}

pub(crate) fn section(ui: &mut Ui, s: &mut CompileSettings, salt: &str) {
    if !s.raytracing.on {
        return;
    }
    body(ui, s, salt, true);
}

fn body(ui: &mut Ui, s: &mut CompileSettings, salt: &str, compile: bool) {
    egui::CollapsingHeader::new(t!("raytrace.section")).id_salt((salt, "rt_section")).default_open(!compile).show(
        ui,
        |ui| {
            let before = s.raytracing.clone();
            let mut chosen: Option<RayConfigSetting> = None;
            ui.label(RichText::new(t!("raytrace.section_tip")).small().weak());
            ui.horizontal_wrapped(|ui| {
                if compile {
                    ui.checkbox(&mut s.raytracing.on, t!("raytrace.enable")).on_hover_text(t!("raytrace.enable_tip"));
                }
                ui.label(t!("raytrace.config")).on_hover_text(t!("raytrace.config_tip"));
                let current = s.raytracing.config;
                egui::ComboBox::from_id_salt((salt, "rt_config"))
                    .selected_text(config_label(current))
                    .show_ui(ui, |ui| {
                        for c in RayConfigSetting::ALL {
                            if ui.selectable_label(current == c, config_label(c)).on_hover_text(config_tip(c)).clicked()
                            {
                                chosen = Some(c);
                            }
                        }
                    })
                    .response
                    .on_hover_text(format!("{}\n\n{}", t!("raytrace.config_tip"), config_tip(current)));
                ui.label(t!("raytrace.quality")).on_hover_text(t!("raytrace.quality_tip"));
                quality_combo(ui, &mut s.raytracing.quality, salt);
                if ui.button(t!("raytrace.optimal")).on_hover_text(t!("raytrace.optimal_tip")).clicked() {
                    chosen = Some(RayConfigSetting::Optimal);
                }
                if s.raytracing.config != RayConfigSetting::Default
                    && ui.button(t!("raytrace.reset")).on_hover_text(t!("raytrace.reset_tip")).clicked()
                {
                    chosen = Some(RayConfigSetting::Default);
                }
            });
            let rt = &mut s.raytracing;
            let d = defaults(rt.quality);
            ui.add_enabled_ui(rt.on || !compile, |ui| {
                egui::Grid::new((salt, "rt_grid")).num_columns(2).spacing([12.0, 4.0]).striped(true).show(ui, |ui| {
                    let tip = t!("raytrace.min_samples_tip");
                    label(ui, "raytrace.min_samples", tip);
                    opt_u32(
                        ui,
                        &mut rt.min_samples,
                        d.min_samples,
                        1..=65536,
                        tip,
                        &default_text(d.min_samples.to_string()),
                    );
                    ui.end_row();
                    let tip = t!("raytrace.max_samples_tip");
                    label(ui, "raytrace.max_samples", tip);
                    opt_u32(
                        ui,
                        &mut rt.max_samples,
                        d.max_samples,
                        1..=65536,
                        tip,
                        &default_text(d.max_samples.to_string()),
                    );
                    ui.end_row();
                    let tip = t!("raytrace.bounces_tip");
                    label(ui, "raytrace.bounces", tip);
                    opt_u32(ui, &mut rt.bounces, d.bounces, 0..=64, tip, &default_text(d.bounces.to_string()));
                    ui.end_row();
                    let tip = t!("raytrace.threshold_tip");
                    label(ui, "raytrace.threshold", tip);
                    let text = default_text(number(d.adaptive_threshold));
                    opt_f32(ui, &mut rt.threshold, d.adaptive_threshold, 0.0..=1.0, 0.001, tip, &text);
                    ui.end_row();
                    let tip = t!("raytrace.light_size_tip");
                    label(ui, "raytrace.light_size", tip);
                    let text = default_text(number(d.light_size_scale));
                    opt_f32(ui, &mut rt.light_size, d.light_size_scale, 0.0..=64.0, 0.05, tip, &text);
                    ui.end_row();
                    let tip = t!("raytrace.sun_angle_tip");
                    label(ui, "raytrace.sun_angle", tip);
                    let text = match d.sun_angle {
                        Some(a) => default_text(format!("{}°", number(a))),
                        None => t!("raytrace.default_sun").to_owned(),
                    };
                    opt_f32(ui, &mut rt.sun_angle, d.sun_angle.unwrap_or(0.5), 0.0..=45.0, 0.05, tip, &text);
                    ui.end_row();
                    let tip = t!("raytrace.sky_samples_tip");
                    label(ui, "raytrace.sky_samples", tip);
                    opt_u32(
                        ui,
                        &mut rt.sky_samples,
                        d.sky_samples,
                        1..=64,
                        tip,
                        &default_text(d.sky_samples.to_string()),
                    );
                    ui.end_row();
                    let tip = t!("raytrace.denoise_tip");
                    label(ui, "raytrace.denoise", tip);
                    opt_f32(ui, &mut rt.denoise, d.denoise, 0.0..=4.0, 0.02, tip, &default_text(number(d.denoise)));
                    ui.end_row();
                    let tip = t!("raytrace.supersample_tip");
                    label(ui, "raytrace.supersample", tip);
                    opt_u32(
                        ui,
                        &mut rt.supersample,
                        d.supersample,
                        1..=8,
                        tip,
                        &default_text(d.supersample.to_string()),
                    );
                    ui.end_row();
                    let tip = t!("raytrace.look_tip");
                    label(ui, "raytrace.look", tip);
                    let auto_look =
                        if d.cinematic { t!("raytrace.look.cinematic") } else { t!("raytrace.look.classic") };
                    let look_text = |v: &str| match v {
                        "classic" => t!("raytrace.look.classic").to_owned(),
                        "cinematic" => t!("raytrace.look.cinematic").to_owned(),
                        _ => default_text(auto_look.to_owned()),
                    };
                    egui::ComboBox::from_id_salt((salt, "rt_look"))
                        .selected_text(look_text(rt.look.as_str()))
                        .show_ui(ui, |ui| {
                            for v in ["", "classic", "cinematic"] {
                                ui.selectable_value(&mut rt.look, v.to_owned(), look_text(v));
                            }
                        })
                        .response
                        .on_hover_text(tip);
                    ui.end_row();
                    let tip = t!("raytrace.luxel_detail_tip");
                    label(ui, "raytrace.luxel_detail", tip);
                    let text = default_text(format!("x{}", number(d.luxel_detail)));
                    opt_f32(ui, &mut rt.luxel_detail, d.luxel_detail.max(2.0), 1.0..=4.0, 0.05, tip, &text);
                    ui.end_row();
                    let tip = t!("raytrace.bump_tip");
                    label(ui, "raytrace.bump", tip);
                    ui.checkbox(&mut rt.bump, t!("raytrace.bump_on")).on_hover_text(tip);
                    ui.end_row();
                    let tip = t!("raytrace.prop_samples_tip");
                    label(ui, "raytrace.prop_samples", tip);
                    opt_u32(ui, &mut rt.prop_samples, 64, 1..=4096, tip, t!("raytrace.default_auto"));
                    ui.end_row();
                    let tip = t!("raytrace.device_tip");
                    label(ui, "raytrace.device", tip);
                    egui::ComboBox::from_id_salt((salt, "rt_device"))
                        .selected_text(device_label(rt.device))
                        .show_ui(ui, |ui| {
                            for v in RayDeviceSetting::ALL {
                                ui.selectable_value(&mut rt.device, v, device_label(v));
                            }
                        })
                        .response
                        .on_hover_text(tip);
                    ui.end_row();
                    let tip = t!("raytrace.chunk_tip");
                    label(ui, "raytrace.chunk", tip);
                    let text = if d.chunk_luxels == 0 {
                        t!("raytrace.default_auto").to_owned()
                    } else {
                        default_text(d.chunk_luxels.to_string())
                    };
                    opt_u32(ui, &mut rt.chunk, 65536, 1024..=16_777_216, tip, &text);
                    ui.end_row();
                    let tip = t!("raytrace.seed_tip");
                    label(ui, "raytrace.seed", tip);
                    ui.add(egui::DragValue::new(&mut rt.seed).speed(1.0)).on_hover_text(tip);
                    ui.end_row();
                    let tip = t!("raytrace.gpu_share_tip");
                    label(ui, "raytrace.gpu_share", tip);
                    gpu_share_row(ui, &mut rt.preview_gpu_share);
                    ui.end_row();
                });
            });
            effects_grid(ui, s, salt);
            ui.label(RichText::new(t!("raytrace.related")).small().weak());
            match chosen {
                Some(c) => apply_config(s, c),
                None => s.raytracing.settle_after_change(&before),
            }
        },
    );
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn plan_options_follow_the_settings() {
        let mut s = CompileSettings::default();
        assert!(plan_options(&s).is_none(), "off by default");
        s.raytracing.on = true;
        s.raytracing.bounces = Some(5);
        s.raytracing.bump = false;
        let o = plan_options(&s).expect("on");
        assert_eq!((o.quality.as_str(), o.bounces, o.bump), ("normal", Some(5), false));
        assert!(!o.lang.is_empty());
        let (_, merged) = super::super::options_ui::effective_step_flags(&s, hp_compile::StepKind::Rad);
        for w in ["-raytracing", "-rtbounces", "5", "-rtnobump", "-rtlang"] {
            assert!(merged.iter().any(|a| a == w), "{w} in {merged:?}");
        }
        let (_, bsp) = super::super::options_ui::effective_step_flags(&s, hp_compile::StepKind::Bsp);
        assert!(!bsp.iter().any(|a| hp_compile::raytracing::is_raytracing_flag(a)));
        assert!(bsp.iter().any(|a| a == hp_compile::raytracing::BSP_AUTO_CUBEMAPS), "{bsp:?}");
        s.raytracing.auto_cubemaps = false;
        let (_, bsp) = super::super::options_ui::effective_step_flags(&s, hp_compile::StepKind::Bsp);
        assert!(!bsp.iter().any(|a| a == hp_compile::raytracing::BSP_AUTO_CUBEMAPS));
    }

    #[test]
    fn optimal_clears_the_overrides_and_turns_on_the_automatic_lightmap_scale() {
        let mut s = CompileSettings::default();
        s.raytracing.min_samples = Some(8);
        s.raytracing.denoise = Some(0.0);
        s.raytracing.seed = 4;
        apply_optimal(&mut s);
        let rt = &s.raytracing;
        assert!(rt.on && rt.quality == RayQualitySetting::Optimal && rt.min_samples.is_none() && rt.seed == 4);
        assert_eq!(rt.config, RayConfigSetting::Optimal);
        apply_config(&mut s, RayConfigSetting::Default);
        assert!(s.raytracing.on && s.raytracing.is_default_state());
        s.raytracing.glow = true;
        let o = plan_options(&s).expect("on");
        assert!(o.effects.glow && o.effects.ao && o.args().contains(&"-rtglow".to_owned()));
        let choice = super::super::options_ui::choice_of(&s.bsp_flag_choices, AUTO_LIGHTMAP_SCALE).expect("choice");
        assert!(choice.on);
        let (_, bsp) = super::super::options_ui::effective_step_flags(&s, hp_compile::StepKind::Bsp);
        assert!(bsp.iter().any(|a| a == AUTO_LIGHTMAP_SCALE), "{bsp:?}");
    }

    #[test]
    fn the_no_gpu_line_raises_a_warning_in_every_language() {
        for m in [NO_GPU_MESSAGE, NO_GPU_MESSAGE_FR, NO_GPU_MESSAGE_RU] {
            let mut run = RunState::preparing("gm_rt");
            run.push_log(Some(1), hp_compile::Severity::Info, format!("Ray tracing: {m}"), None);
            assert!(run.no_gpu, "{m}");
            assert_eq!(run.warnings, 1);
            assert_eq!(run.log[0].severity, hp_compile::Severity::Warning);
        }
        let mut run = RunState::preparing("gm_rt");
        run.push_log(Some(1), hp_compile::Severity::Info, "Ray tracing on the GPU".to_owned(), None);
        assert!(!run.no_gpu && run.warnings == 0);
    }

    #[test]
    fn the_section_renders_in_every_language() {
        let _lang = crate::lang_lock();
        crate::register_translations();
        for lang in hp_app::i18n::Lang::ALL {
            hp_app::i18n::set_lang(lang);
            assert_eq!(t!("raytrace.no_gpu"), hp_raytrace::no_gpu_message(lang.code()));
            for q in RayQualitySetting::ALL {
                assert!(!quality_label(q).starts_with("compile.") && !quality_tip(q).starts_with("compile."));
            }
            for d in RayDeviceSetting::ALL {
                assert!(!device_label(d).starts_with("compile."));
            }
            let mut s = CompileSettings::default();
            s.raytracing.on = true;
            s.raytracing.bounces = Some(3);
            let mut run = RunState::preparing("gm_rt");
            run.push_log(None, hp_compile::Severity::Info, NO_GPU_MESSAGE.to_owned(), None);
            let ctx = egui::Context::default();
            for _ in 0..2 {
                let mut out = ctx.run_ui(egui::RawInput::default(), |ui| {
                    egui::CollapsingHeader::new("x").default_open(true).show(ui, |ui| {
                        toggle_row(ui, &mut s);
                        section(ui, &mut s, "test");
                        effects_grid(ui, &mut s, "test");
                        no_gpu_banner(ui, &run);
                    });
                });
                out.textures_delta.clear();
            }
            assert_eq!(s.raytracing.bounces, Some(3));
        }
        hp_app::i18n::set_lang(hp_app::i18n::Lang::En);
    }
}
