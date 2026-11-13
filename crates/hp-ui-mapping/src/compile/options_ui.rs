use std::collections::BTreeMap;

use egui::{RichText, Ui};
use hp_app::settings::{CompileSettings, FlagChoiceSetting};
use hp_app::{t, tf, theme};
use hp_compile::flags::{self, FlagChoice, FlagInfo, ManagedBy};
use hp_compile::{BspMode, PlanOptions, RadMode, StepKind, split_args};

use super::plan::{effective_modes, lighting_of, threads_of};
use super::settings_ui::flag_text;

pub(crate) fn flag_choices(map: &BTreeMap<String, FlagChoiceSetting>) -> Vec<FlagChoice> {
    map.iter().map(|(flag, c)| FlagChoice { flag: flag.clone(), on: c.on, value: c.value.clone() }).collect()
}

fn choices_mut(s: &mut CompileSettings, step: StepKind) -> &mut BTreeMap<String, FlagChoiceSetting> {
    if step == StepKind::Bsp { &mut s.bsp_flag_choices } else { &mut s.rad_flag_choices }
}

fn choices(s: &CompileSettings, step: StepKind) -> &BTreeMap<String, FlagChoiceSetting> {
    if step == StepKind::Bsp { &s.bsp_flag_choices } else { &s.rad_flag_choices }
}

pub(crate) fn choice_of<'a>(map: &'a BTreeMap<String, FlagChoiceSetting>, flag: &str) -> Option<&'a FlagChoiceSetting> {
    map.get(flag).or_else(|| map.iter().find(|(k, _)| k.eq_ignore_ascii_case(flag)).map(|(_, v)| v))
}

pub(crate) fn set_choice(map: &mut BTreeMap<String, FlagChoiceSetting>, flag: &str, choice: Option<FlagChoiceSetting>) {
    map.retain(|k, _| !k.eq_ignore_ascii_case(flag));
    if let Some(c) = choice {
        map.insert(flag.to_owned(), c);
    }
}

pub(crate) struct StepFlags {
    pub preset: Vec<Vec<String>>,
    pub extra: Vec<Vec<String>>,
    pub merged: Vec<Vec<String>>,
}

pub(crate) fn step_flags(s: &CompileSettings, step: StepKind) -> StepFlags {
    let modes = effective_modes(s);
    let opts = PlanOptions {
        static_prop_lighting: s.static_prop_lighting,
        static_prop_shadows: s.static_prop_shadows,
        ..PlanOptions::default()
    };
    let (preset, merged, extra) = if step == StepKind::Bsp {
        let base = flags::bsp_flags(
            if modes.bsp == BspMode::Off { BspMode::Full } else { modes.bsp },
            modes.vis,
            s.stop_on_leak,
        );
        let extra = split_args(&s.extra_bsp_args);
        let (preset, _) = flags::merge_flags(step, &base, &[], &[]);
        let (mut merged, rest) = flags::merge_flags(step, &base, &flag_choices(&s.bsp_flag_choices), &extra);
        merged.extend(rest);
        (preset, merged, extra)
    } else {
        let rad = if modes.rad == RadMode::Off { RadMode::Normal } else { modes.rad };
        let (preset, _) = hp_compile::rad_step_flags(rad, &opts);
        let full = PlanOptions {
            rad_flags: flag_choices(&s.rad_flag_choices),
            extra_rad_args: s.extra_rad_args.clone(),
            ..opts
        };
        let (mut merged, rest) = hp_compile::rad_step_flags(rad, &full);
        merged.extend(rest);
        (preset, merged, split_args(&s.extra_rad_args))
    };
    StepFlags {
        preset: flags::arg_groups(&preset, step),
        extra: flags::arg_groups(&extra, step),
        merged: flags::arg_groups(&merged, step),
    }
}

pub fn effective_step_flags(s: &CompileSettings, step: StepKind) -> (Vec<String>, Vec<String>) {
    let sf = step_flags(s, step);
    (sf.preset.concat(), sf.merged.concat())
}

fn group_of<'a>(groups: &'a [Vec<String>], flag: &str) -> Option<&'a Vec<String>> {
    groups.iter().find(|g| g.first().is_some_and(|a| a.eq_ignore_ascii_case(flag)))
}

fn values_text(g: &[String]) -> String {
    g.get(1..).map(|v| v.join(" ")).unwrap_or_default()
}

#[derive(Clone, Debug, PartialEq)]
pub(crate) enum Origin {
    Managed(ManagedBy),
    PropLightingOff,
    Forced,
    Extra,
    Preset,
    Default,
}

#[derive(Clone, Debug, PartialEq)]
pub(crate) struct FlagRow {
    pub on: bool,
    pub value: String,
    pub origin: Origin,
}

pub(crate) fn flag_row(s: &CompileSettings, info: &FlagInfo, sf: &StepFlags) -> FlagRow {
    let step = info.step;
    let modes = effective_modes(s);
    if let Some(by) = info.managed_by() {
        let lower = info.flag.to_ascii_lowercase();
        let (on, value) = match by {
            ManagedBy::Threads => match threads_of(s) {
                Some(n) => (true, n.to_string()),
                None => (false, String::new()),
            },
            ManagedBy::Lighting => {
                let rad = if modes.rad == RadMode::Off { RadMode::Normal } else { modes.rad };
                (flags::rad_lighting(rad, lighting_of(s.lighting)).flag() == lower, String::new())
            }
            ManagedBy::Game => (true, String::new()),
            ManagedBy::StaticPropShadows => (!s.static_prop_shadows, String::new()),
            _ => match group_of(&sf.merged, info.flag) {
                Some(g) => (true, values_text(g)),
                None => (false, String::new()),
            },
        };
        return FlagRow { on, value, origin: Origin::Managed(by) };
    }
    let merged = group_of(&sf.merged, info.flag);
    let on = merged.is_some();
    let choice = choice_of(choices(s, step), info.flag);
    let value = match (merged, choice) {
        (Some(g), _) if g.len() > 1 => values_text(g),
        (_, Some(c)) => c.value.clone(),
        _ => String::new(),
    };
    let origin = if step == StepKind::Rad
        && !s.static_prop_lighting
        && info.flag.eq_ignore_ascii_case(flags::STATIC_PROP_LIGHTING)
    {
        Origin::PropLightingOff
    } else if choice.is_some() {
        Origin::Forced
    } else if group_of(&sf.extra, info.flag).is_some() {
        Origin::Extra
    } else if group_of(&sf.preset, info.flag).is_some() {
        Origin::Preset
    } else {
        Origin::Default
    };
    FlagRow { on, value, origin }
}

fn forced_count(s: &CompileSettings, step: StepKind) -> usize {
    choices(s, step).keys().filter(|k| flags::flag_info(step, k).is_some_and(|f| f.managed_by().is_none())).count()
}

pub(crate) fn prop_toggles(ui: &mut Ui, s: &mut CompileSettings) {
    ui.checkbox(&mut s.static_prop_lighting, t!("compile.prop_lighting"))
        .on_hover_text(t!("compile.prop_lighting_tip"));
    ui.checkbox(&mut s.static_prop_shadows, t!("compile.prop_shadows")).on_hover_text(t!("compile.prop_shadows_tip"));
}

pub fn compile_options_ui(ui: &mut Ui, s: &mut CompileSettings) {
    ui.horizontal_wrapped(|ui| {
        ui.label(t!("compile.static_props"));
        prop_toggles(ui, s);
    });
    step_menu(ui, s, StepKind::Bsp, "hp_settings_opts");
    step_menu(ui, s, StepKind::Rad, "hp_settings_opts");
}

pub(crate) fn step_menu(ui: &mut Ui, s: &mut CompileSettings, step: StepKind, salt: &str) {
    let forced = forced_count(s, step);
    let title = match (step, forced) {
        (StepKind::Bsp, 0) => t!("compile.opt.menu_bsp").to_owned(),
        (StepKind::Bsp, n) => tf!("compile.opt.menu_bsp_n", n),
        (_, 0) => t!("compile.opt.menu_rad").to_owned(),
        (_, n) => tf!("compile.opt.menu_rad_n", n),
    };
    egui::CollapsingHeader::new(title).id_salt((salt, "menu", step == StepKind::Bsp)).default_open(false).show(
        ui,
        |ui| {
            ui.label(RichText::new(t!("compile.opt.menu_tip")).small().weak());
            if forced > 0 && ui.small_button(t!("compile.opt.reset_all")).clicked() {
                choices_mut(s, step).clear();
            }
            let sf = step_flags(s, step);
            for (group, list) in flags::menu_flags(step) {
                ui.add_space(4.0);
                ui.label(RichText::new(t!(&format!("compile.flag_group.{}", group.key()))).strong());
                egui::Grid::new((salt, "grid", step == StepKind::Bsp, group.key()))
                    .num_columns(3)
                    .spacing([10.0, 3.0])
                    .show(ui, |ui| {
                        for info in list {
                            flag_row_ui(ui, s, info, &sf);
                            ui.end_row();
                        }
                    });
            }
        },
    );
}

fn flag_row_ui(ui: &mut Ui, s: &mut CompileSettings, info: &FlagInfo, sf: &StepFlags) {
    let row = flag_row(s, info, sf);
    let tip = flag_text(info.step, info.flag).unwrap_or_else(|| info.english.to_owned());
    let locked = matches!(row.origin, Origin::Managed(_) | Origin::PropLightingOff);
    let mut on = row.on;
    let label = RichText::new(info.flag).monospace();
    let resp = ui.add_enabled(!locked, egui::Checkbox::new(&mut on, label)).on_hover_text(&tip);
    let resp = if locked { resp.on_disabled_hover_text(&tip) } else { resp };
    if resp.changed() {
        let value = if row.value.is_empty() { default_value(info) } else { row.value.clone() };
        let default_on = group_of(&sf.extra, info.flag).or_else(|| group_of(&sf.preset, info.flag)).is_some();
        let plain = on == default_on;
        set_choice(choices_mut(s, info.step), info.flag, (!plain).then_some(FlagChoiceSetting { on, value }));
    }
    if info.value_count() > 0 {
        let mut text = row.value.clone();
        let width = if matches!(info.flag.to_ascii_lowercase().as_str(), "-lights" | "-embed") { 200.0 } else { 70.0 };
        let edit = egui::TextEdit::singleline(&mut text)
            .desired_width(width)
            .hint_text(RichText::new(default_value(info)).weak().italics());
        let resp = ui.add_enabled(!locked, edit).on_hover_text(&tip);
        if resp.changed() {
            set_choice(choices_mut(s, info.step), info.flag, Some(FlagChoiceSetting { on: true, value: text }));
        }
    } else {
        ui.label("");
    }
    ui.horizontal(|ui| {
        let (text, color) = match &row.origin {
            Origin::Managed(by) => (t!(&format!("compile.managed.{}", by.key())).to_owned(), theme::text_weak()),
            Origin::PropLightingOff => (t!("compile.opt.prop_lighting_off").to_owned(), theme::warning()),
            Origin::Forced if row.on => (t!("compile.opt.forced_on").to_owned(), theme::accent()),
            Origin::Forced => {
                let missing = choice_of(choices(s, info.step), info.flag).is_some_and(|c| c.on);
                let key = if missing { "compile.opt.needs_value" } else { "compile.opt.forced_off" };
                (t!(key).to_owned(), if missing { theme::warning() } else { theme::accent() })
            }
            Origin::Extra => (t!("compile.opt.from_extra").to_owned(), theme::text_weak()),
            Origin::Preset => (t!("compile.opt.from_preset").to_owned(), theme::text_weak()),
            Origin::Default => (String::new(), theme::text_weak()),
        };
        if !text.is_empty() {
            ui.label(RichText::new(text).small().color(color));
        }
        if row.origin == Origin::Forced && ui.small_button("↺").on_hover_text(t!("compile.opt.reset")).clicked() {
            set_choice(choices_mut(s, info.step), info.flag, None);
        }
    });
}

fn default_value(info: &FlagInfo) -> String {
    match info.flag.to_ascii_lowercase().as_str() {
        "-bounce" => "100",
        "-smooth" => "45",
        "-softsun" | "-luxeldensity" => "0.5",
        "-extrasky" | "-staticpropsamplescale" => "2",
        "-chop" | "-maxchop" => "4",
        "-dispchop" => "8",
        "-micro" | "-minluxelscale" => "1",
        "-luxelscale" => "2",
        "-defaultluxelsize" => "16",
        "-maxlightmapdim" => "32",
        "-radius_override" => "4096",
        "-dxlevel" => "90",
        "-trace" => "0 1",
        "-aoradius" => "32",
        "-aoscale" => "0.5",
        "-aofacesamples" | "-aopropsamples" => "32",
        _ => "",
    }
    .to_owned()
}

#[cfg(test)]
mod tests {
    use super::*;

    fn row(s: &CompileSettings, step: StepKind, flag: &str) -> FlagRow {
        let info = flags::flag_info(step, flag).expect("flag");
        flag_row(s, info, &step_flags(s, step))
    }

    #[test]
    fn rows_show_the_preset_the_choices_and_the_managed_flags() {
        let mut s = CompileSettings::default();
        super::super::plan::apply_preset(&mut s, hp_app::settings::CompilePresetSetting::Final);
        let r = row(&s, StepKind::Rad, "-final");
        assert_eq!((r.on, r.origin), (true, Origin::Preset));
        assert_eq!(row(&s, StepKind::Rad, "-StaticPropLighting").origin, Origin::Preset);
        assert_eq!(row(&s, StepKind::Rad, "-noextra").origin, Origin::Default);
        assert!(!row(&s, StepKind::Rad, "-noextra").on);
        assert_eq!(
            row(&s, StepKind::Rad, "-both"),
            FlagRow { on: true, value: String::new(), origin: Origin::Managed(ManagedBy::Lighting) }
        );
        assert!(!row(&s, StepKind::Rad, "-ldr").on);
        assert!(row(&s, StepKind::Bsp, "-vis").on && !row(&s, StepKind::Bsp, "-fastvis").on);
        assert!(!row(&s, StepKind::Rad, "-nostaticpropshadows").on);
        s.static_prop_shadows = false;
        assert!(row(&s, StepKind::Rad, "-nostaticpropshadows").on);
        s.static_prop_lighting = false;
        let r = row(&s, StepKind::Rad, "-StaticPropLighting");
        assert_eq!((r.on, r.origin), (false, Origin::PropLightingOff));
        s.extra_rad_args = "-bounce 4".to_owned();
        let r = row(&s, StepKind::Rad, "-bounce");
        assert_eq!((r.on, r.value.as_str(), r.origin), (true, "4", Origin::Extra));
        set_choice(&mut s.rad_flag_choices, "-bounce", Some(FlagChoiceSetting { on: true, value: "8".into() }));
        set_choice(&mut s.rad_flag_choices, "-final", Some(FlagChoiceSetting { on: false, value: String::new() }));
        let r = row(&s, StepKind::Rad, "-bounce");
        assert_eq!((r.on, r.value.as_str(), r.origin), (true, "8", Origin::Forced));
        assert_eq!(
            row(&s, StepKind::Rad, "-final"),
            FlagRow { on: false, value: String::new(), origin: Origin::Forced }
        );
        assert_eq!(forced_count(&s, StepKind::Rad), 2);
        set_choice(&mut s.rad_flag_choices, "-BOUNCE", None);
        assert!(choice_of(&s.rad_flag_choices, "-bounce").is_none());
        s.threads = Some(3);
        assert_eq!(row(&s, StepKind::Bsp, "-threads").value, "3");
    }

    #[test]
    fn menus_render_in_both_languages() {
        let _lang = crate::lang_lock();
        crate::register_translations();
        for lang in hp_app::i18n::Lang::ALL {
            hp_app::i18n::set_lang(lang);
            for g in flags::FlagGroup::ALL {
                assert!(hp_app::i18n::has_key(&format!("compile.flag_group.{}", g.key())), "{g:?}");
            }
            for by in [
                ManagedBy::VisMode,
                ManagedBy::BspMode,
                ManagedBy::StopOnLeak,
                ManagedBy::Threads,
                ManagedBy::Lighting,
                ManagedBy::Game,
                ManagedBy::StaticPropShadows,
            ] {
                assert!(hp_app::i18n::has_key(&format!("compile.managed.{}", by.key())), "{by:?}");
            }
            let mut s = CompileSettings::default();
            s.rad_flag_choices.insert("-bounce".into(), FlagChoiceSetting { on: true, value: String::new() });
            s.bsp_flag_choices.insert("-nodetail".into(), FlagChoiceSetting { on: true, value: String::new() });
            let ctx = egui::Context::default();
            let mut out = ctx.run_ui(egui::RawInput::default(), |ui| {
                compile_options_ui(ui, &mut s);
                for step in [StepKind::Bsp, StepKind::Rad] {
                    let sf = step_flags(&s, step);
                    for (_, list) in flags::menu_flags(step) {
                        egui::Grid::new(("rows", step == StepKind::Bsp, list[0].flag)).num_columns(3).show(ui, |ui| {
                            for info in list {
                                flag_row_ui(ui, &mut s, info, &sf);
                                ui.end_row();
                            }
                        });
                    }
                }
            });
            out.textures_delta.clear();
            assert_eq!(forced_count(&s, StepKind::Rad), 1);
            assert_eq!(forced_count(&s, StepKind::Bsp), 1);
        }
        hp_app::i18n::set_lang(hp_app::i18n::Lang::En);
    }
}
