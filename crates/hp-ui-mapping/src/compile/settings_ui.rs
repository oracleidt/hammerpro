use std::path::PathBuf;
use std::sync::mpsc::{self, Receiver, TryRecvError};
use std::time::{Duration, Instant};

use egui::{RichText, Ui};
use hp_app::settings::{
    BspModeSetting, CompilePresetSetting, CompileSettings, LightingSetting, PackPolicy, RadModeSetting, Settings,
    VisModeSetting,
};
use hp_app::{AppState, t, tf, theme};
use hp_compile::flags::flag_info;
use hp_compile::{BspMode, StepKind};

use super::background::{ContentKey, snapshot_is_cheap};
use super::plan::{self, PlanExtras, effective_modes};
use super::snapshot::{self, CompileSidecar, SnapshotInput, SnapshotOutput};
use super::{CompileDialog, ToolProbe, game_root, history_file, is_iteration_preset};
use crate::util::{Form, lazy_hover_text, wrap_group};

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(crate) enum BrowseTarget {
    GameExe,
    MapsDir,
}

impl BrowseTarget {
    pub(crate) fn title(self) -> String {
        match self {
            BrowseTarget::GameExe => t!("compile.browse_game_exe").to_owned(),
            BrowseTarget::MapsDir => t!("compile.browse_maps_dir").to_owned(),
        }
    }

    pub(crate) fn is_folder(self) -> bool {
        self == BrowseTarget::MapsDir
    }

    pub(crate) fn apply(self, s: &mut CompileSettings, p: PathBuf) {
        match self {
            BrowseTarget::GameExe => s.game_exe_path = Some(p),
            BrowseTarget::MapsDir => s.maps_dir_override = Some(p),
        }
    }
}

pub(crate) fn smart_applies(s: &CompileSettings) -> bool {
    s.smart_recompile
        && !s.expert
        && s.uses_builtin_compiler()
        && is_iteration_preset(s.preset)
        && effective_modes(s).bsp == BspMode::Full
}

#[derive(Clone, Debug)]
pub(crate) struct PreviewLine {
    pub kind: StepKind,
    pub line: String,
    pub flags: Vec<(String, String)>,
}

#[derive(Default)]
pub(crate) struct PreviewCache {
    key: Option<PreviewKey>,
    pub lines: Vec<PreviewLine>,
    auto_key: Option<AutoKey>,
    auto_since: Option<Instant>,
    auto_rx: Option<(Receiver<SnapshotOutput>, AutoKey)>,
    pub compiles: u64,
    auto_cost_ms: Option<u64>,
}

type AutoKey = (ContentKey, CompileSettings, u64);
type PreviewKey = (CompileSettings, u64, Option<PathBuf>, bool, hp_app::i18n::Lang, Option<u64>);

pub(crate) fn flag_text(step: StepKind, flag: &str) -> Option<String> {
    let info = flag_info(step, flag)?;
    let key = format!("compile.flag.{}", info.key());
    Some(if hp_app::i18n::has_key(&key) { hp_app::i18n::tr(&key).to_owned() } else { info.english.to_owned() })
}

impl PreviewCache {
    pub(crate) fn update(&mut self, ctx: &egui::Context, state: &mut AppState, probe: &ToolProbe, busy: bool) {
        let s = &state.settings.compile;
        let cordoned = s.compile_cordon_only && state.doc.cordons_in_use();
        let mounted = state.assets.fs().map(hp_fs::GameFs::generation);
        let path = state.doc.path().map(PathBuf::from);
        let key = (s.clone(), probe.serial, path, cordoned, hp_app::i18n::lang(), mounted);
        if self.key.as_ref() != Some(&key) {
            self.lines = Self::compute_lines(state, probe, cordoned);
            self.key = Some(key);
        }
        self.update_auto(ctx, state, busy);
    }

    fn compute_lines(state: &AppState, probe: &ToolProbe, cordoned: bool) -> Vec<PreviewLine> {
        let Some(res) = probe.result.as_ref().and_then(|r| r.resolved.as_ref()) else { return Vec::new() };
        let s = &state.settings.compile;
        let Ok(target) = snapshot::target_for(
            state.doc.path(),
            cordoned,
            &Settings::cache_dir().join("untitled"),
            state.settings.compile.bsp_beside_vmf,
        ) else {
            return Vec::new();
        };
        let external = match super::compilers::choice(s, &res.tools) {
            super::compilers::Choice::External(p) => Some(p),
            _ => None,
        };
        let expert = super::expert::active_profile(s).filter(|_| external.is_none());
        let built = match expert {
            Some(profile) => super::expert::plan_of(profile, &target, &res.tools),
            None => {
                let content_args = plan::content_args(state.assets.fs());
                let extras = PlanExtras { content_args, ..PlanExtras::default() };
                plan::build_plan_with(s, &res.tools, &target.vmf(), Vec::new(), None, extras)
                    .map_err(|e| super::notices::plan_error_text(&e))
                    .and_then(|p| match &external {
                        Some(profile) => {
                            super::compilers::external_plan(p, profile, s, &res.tools, plan::effective_lighting(s))
                        }
                        None => Ok(p),
                    })
            }
        };
        let p = match built {
            Ok(p) => p,
            Err(e) => return vec![PreviewLine { kind: StepKind::Custom, line: e, flags: Vec::new() }],
        };
        if expert.is_some() || external.is_some() {
            return p
                .steps
                .iter()
                .filter(|st| st.enabled)
                .map(|st| PreviewLine { kind: st.kind, line: st.command_line(), flags: Vec::new() })
                .collect();
        }
        p.steps
            .iter()
            .filter(|st| matches!(st.kind, StepKind::Bsp | StepKind::Rad))
            .map(|st| {
                let flags = st
                    .args
                    .iter()
                    .filter(|a| a.starts_with('-'))
                    .filter_map(|a| flag_text(st.kind, a).map(|t| (a.clone(), t)))
                    .collect();
                PreviewLine { kind: st.kind, line: st.command_line(), flags }
            })
            .collect()
    }

    fn update_auto(&mut self, ctx: &egui::Context, state: &mut AppState, busy: bool) {
        let s = &state.settings.compile;
        if !smart_applies(s) {
            state.runtime.compile.auto_label = None;
            return;
        }
        let key = (ContentKey::of(state), s.clone(), self.compiles);
        if let Some((rx, k)) = &self.auto_rx {
            match rx.try_recv() {
                Ok(out) => {
                    if *k == key {
                        let after = state.settings.compile.copy_to_maps || state.settings.compile.launch_game;
                        state.runtime.compile.auto_label =
                            out.dry.and_then(|d| d.choice.map(|c| snapshot::auto_text(c.kind, d.estimate_ms, after)));
                        self.auto_key = Some(key.clone());
                    }
                    self.auto_rx = None;
                }
                Err(TryRecvError::Disconnected) => self.auto_rx = None,
                Err(TryRecvError::Empty) => return,
            }
        }
        if self.auto_key.as_ref() == Some(&key) || busy {
            return;
        }
        state.runtime.compile.auto_label = None;
        let since = *self.auto_since.get_or_insert_with(Instant::now);
        if since.elapsed() < Duration::from_millis(400) {
            ctx.request_repaint_after(Duration::from_millis(450));
            return;
        }
        self.auto_since = None;
        if !snapshot_is_cheap(&state.doc, self.auto_cost_ms) {
            self.auto_key = Some(key);
            return;
        }
        let export = CompileDialog::export_of(s, &state.settings.visibility);
        let t0 = Instant::now();
        let snap = state.doc.compile_snapshot(&export);
        self.auto_cost_ms = Some(u64::try_from(t0.elapsed().as_millis()).unwrap_or(u64::MAX));
        let Ok(target) = snapshot::target_for(
            state.doc.path(),
            snap.is_cordoned(),
            &Settings::cache_dir().join("untitled"),
            state.settings.compile.bsp_beside_vmf,
        ) else {
            self.auto_key = Some(key);
            return;
        };
        let previous = state.sidecar.get::<CompileSidecar>().record(&target.name);
        let map_key = hp_app::sidecar::map_key(target.source.as_deref());
        let input = SnapshotInput {
            history_file: s.history.then(|| history_file(&map_key)),
            snapshot: snap,
            target,
            settings: s.clone(),
            gmod_root: game_root(state),
            fs: state.assets.fs().cloned(),
            mounting: false,
            previous,
            cache_dir: Settings::cache_dir().join("pack"),
            content_dir: Settings::cache_dir().join("compile_content"),
            builds_dir: None,
            release: false,
            background: false,
            dry_run: true,
            snapshot_ms: 0,
            launch_flags: Vec::new(),
        };
        let (tx, rx) = mpsc::channel();
        let repaint = ctx.clone();
        let spawned = std::thread::Builder::new().name("hp-compile-auto".into()).spawn(move || {
            let _ = tx.send(snapshot::prepare_snapshot(input));
            repaint.request_repaint();
        });
        if spawned.is_ok() {
            self.auto_rx = Some((rx, key));
        }
    }
}

fn bsp_label(run: bool, m: BspModeSetting) -> &'static str {
    match (run, m) {
        (false, _) => t!("compile.mode.no"),
        (true, BspModeSetting::Normal) => t!("compile.mode.normal"),
        (true, BspModeSetting::OnlyEnts) => t!("compile.mode.only_ents"),
        (true, BspModeSetting::OnlyProps) => t!("compile.mode.only_props"),
    }
}

fn vis_label(run: bool, m: VisModeSetting) -> &'static str {
    match (run, m) {
        (false, _) => t!("compile.mode.no"),
        (true, VisModeSetting::Fast) => t!("compile.mode.fast"),
        (true, VisModeSetting::Normal) => t!("compile.mode.normal"),
    }
}

fn rad_label(run: bool, m: RadModeSetting) -> &'static str {
    match (run, m) {
        (false, _) => t!("compile.mode.no"),
        (true, RadModeSetting::Draft) => t!("compile.mode.draft"),
        (true, RadModeSetting::Normal) => t!("compile.mode.normal"),
        (true, RadModeSetting::Final) => t!("compile.mode.final"),
    }
}

pub(crate) fn normalize_steps(s: &mut CompileSettings) {
    if s.preset == CompilePresetSetting::Custom {
        return;
    }
    let target = plan::preset_of(s.preset).modes();
    if plan::custom_modes(s) == target && s.run_bsp && s.run_vis && s.run_rad {
        return;
    }
    let eff = effective_modes(s);
    plan::set_custom_modes(s, target);
    s.run_bsp = eff.bsp != BspMode::Off;
    s.run_vis = eff.vis != hp_compile::VisMode::Off;
    s.run_rad = eff.rad != hp_compile::RadMode::Off;
    settle_preset(s);
}

pub(crate) fn settle_preset(s: &mut CompileSettings) {
    let custom = CompileSettings { preset: CompilePresetSetting::Custom, ..s.clone() };
    let modes = effective_modes(&custom);
    s.preset = match hp_compile::Preset::of_modes(modes) {
        hp_compile::Preset::Draft => CompilePresetSetting::Draft,
        hp_compile::Preset::Normal => CompilePresetSetting::Normal,
        hp_compile::Preset::Final => CompilePresetSetting::Final,
        hp_compile::Preset::Custom => CompilePresetSetting::Custom,
    };
}

impl CompileDialog {
    pub(super) fn settings_ui(&mut self, ui: &mut Ui, state: &mut AppState) {
        let running = self.is_running() || self.pending.is_some();
        let mut header = egui::CollapsingHeader::new(RichText::new(t!("compile.settings")).strong())
            .id_salt("hp_compile_settings")
            .default_open(true);
        if std::mem::take(&mut self.collapse_settings) {
            header = header.open(Some(false));
        }
        let ctx = ui.ctx().clone();
        let mut browse: Option<BrowseTarget> = None;
        let cordons = state.doc.cordon_box_count();
        let max_height = (ui.available_height() * 0.6).max(120.0);
        header.show(ui, |ui| {
            egui::ScrollArea::vertical().id_salt("hp_compile_settings_scroll").max_height(max_height).show(ui, |ui| {
                let before = state.settings.compile.clone();
                ui.add_enabled_ui(!running, |ui| {
                    self.main_grid(ui, state, cordons);
                    if self.external_name.is_some() {
                        super::compilers_ui::active_options_ui(ui, state);
                    } else {
                        ui.add_enabled_ui(!state.settings.compile.expert, |ui| {
                            let s = &mut state.settings.compile;
                            super::options_ui::step_menu(ui, s, StepKind::Bsp, "hp_compile_opts");
                            super::options_ui::step_menu(ui, s, StepKind::Rad, "hp_compile_opts");
                        });
                    }
                    egui::CollapsingHeader::new(t!("compile.advanced")).id_salt("hp_compile_advanced").show(ui, |ui| {
                        browse = Self::advanced_ui(ui, state, &self.probe);
                    });
                });
                if state.settings.compile != before {
                    state.settings_dirty = true;
                }
                super::launch::options_ui(ui, state, !running);
                self.tools_status_ui(ui);
            });
        });
        if let Some(target) = browse {
            self.browse(&ctx, target);
        }
    }

    fn form(&mut self, ui: &Ui) -> Form {
        let key = (hp_app::i18n::lang(), ui.ctx().pixels_per_point().to_bits());
        if let Some((k, form)) = self.form
            && k == key
        {
            return form;
        }
        let labels = [
            t!("compilers.label"),
            t!("compile.preset"),
            t!("compile.steps"),
            t!("compile.cordon"),
            t!("compile.static_props"),
            t!("compile.before"),
            t!("compile.content"),
            t!("compile.after"),
            t!("compile.game_args"),
            t!("compile.threads"),
        ];
        let form = Form::new(ui, &labels, 90.0, 190.0);
        self.form = Some((key, form));
        form
    }

    fn main_grid(&mut self, ui: &mut Ui, state: &mut AppState, cordons: usize) {
        let form = self.form(ui);
        let (_, detected) = super::compilers_ui::detection(ui.ctx(), state);
        let mut open_compilers = false;
        ui.spacing_mut().item_spacing.y = 6.0;
        form.row(ui, egui::WidgetText::from(t!("compilers.label")), |ui| {
            super::compilers_ui::picker(ui, state, &detected, "dialog");
            if ui.button(t!("compilers.edit")).on_hover_text(t!("compilers.label_tip")).clicked() {
                open_compilers = true;
            }
        });
        if open_compilers {
            self.tab = super::Tab::Compilers;
            self.collapse_settings = true;
        }
        let active = super::compilers_ui::active_of(state, &detected);
        self.external_name = (!active.is_builtin()).then(|| super::compilers::display_name(&active));
        if let Some(name) = &self.external_name {
            ui.label(RichText::new(tf!("compilers.external_note", name)).color(theme::warning()).small());
        }
        let external = self.external_name.is_some();
        let s = &mut state.settings.compile;
        normalize_steps(s);
        form.row(ui, t!("compile.preset"), |ui| {
            for (p, key, tip) in [
                (CompilePresetSetting::Draft, "compile.preset.draft", "compile.preset.draft_tip"),
                (CompilePresetSetting::Normal, "compile.preset.normal", "compile.preset.normal_tip"),
                (CompilePresetSetting::Final, "compile.preset.final", "compile.preset.final_tip"),
                (CompilePresetSetting::Custom, "compile.preset.custom", "compile.preset.custom_tip"),
            ] {
                if ui.add(egui::Button::selectable(s.preset == p, t!(key))).on_hover_text(t!(tip)).clicked() {
                    plan::apply_preset(s, p);
                }
            }
            ui.separator();
            ui.add_enabled(
                is_iteration_preset(s.preset) && !external,
                egui::Checkbox::new(&mut s.smart_recompile, t!("compile.smart")),
            )
            .on_hover_text(t!("compile.smart_tip"));
        });

        form.row(ui, t!("compile.steps"), |ui| {
            ui.add_enabled_ui(!s.expert || external, |ui| {
                let mut changed = false;
                wrap_group(ui, "bsp", |ui| {
                    ui.label(t!("compile.step.bsp"));
                    egui::ComboBox::from_id_salt("hp_compile_bsp")
                        .selected_text(bsp_label(s.run_bsp, s.bsp_mode))
                        .show_ui(ui, |ui| {
                            let cur = s.bsp_mode;
                            let mut pick = |ui: &mut Ui, run: bool, m: BspModeSetting| {
                                if ui
                                    .selectable_label(s.run_bsp == run && (!run || s.bsp_mode == m), bsp_label(run, m))
                                    .clicked()
                                {
                                    s.run_bsp = run;
                                    if run {
                                        s.bsp_mode = m;
                                    }
                                    changed = true;
                                }
                            };
                            pick(ui, false, cur);
                            pick(ui, true, BspModeSetting::Normal);
                            pick(ui, true, BspModeSetting::OnlyEnts);
                            pick(ui, true, BspModeSetting::OnlyProps);
                        })
                        .response
                        .on_hover_text(t!("compile.bsp_tip"));
                });
                wrap_group(ui, "vis", |ui| {
                    ui.label(t!("compile.visibility"));
                    egui::ComboBox::from_id_salt("hp_compile_vis")
                        .selected_text(vis_label(s.run_vis, s.vis_mode))
                        .show_ui(ui, |ui| {
                            let cur = s.vis_mode;
                            let mut pick = |ui: &mut Ui, run: bool, m: VisModeSetting| {
                                if ui
                                    .selectable_label(s.run_vis == run && (!run || s.vis_mode == m), vis_label(run, m))
                                    .clicked()
                                {
                                    s.run_vis = run;
                                    if run {
                                        s.vis_mode = m;
                                    }
                                    changed = true;
                                }
                            };
                            pick(ui, false, cur);
                            pick(ui, true, VisModeSetting::Fast);
                            pick(ui, true, VisModeSetting::Normal);
                        })
                        .response
                        .on_hover_text(t!("compile.vis_tip"));
                });
                wrap_group(ui, "rad", |ui| {
                    ui.label(t!("compile.step.rad"));
                    egui::ComboBox::from_id_salt("hp_compile_rad")
                        .selected_text(rad_label(s.run_rad, s.rad_mode))
                        .show_ui(ui, |ui| {
                            let cur = s.rad_mode;
                            let mut pick = |ui: &mut Ui, run: bool, m: RadModeSetting| {
                                if ui
                                    .selectable_label(s.run_rad == run && (!run || s.rad_mode == m), rad_label(run, m))
                                    .clicked()
                                {
                                    s.run_rad = run;
                                    if run {
                                        s.rad_mode = m;
                                    }
                                    changed = true;
                                }
                            };
                            pick(ui, false, cur);
                            pick(ui, true, RadModeSetting::Draft);
                            pick(ui, true, RadModeSetting::Normal);
                            pick(ui, true, RadModeSetting::Final);
                        });
                });
                wrap_group(ui, "lighting", |ui| {
                    ui.label(t!("compile.lighting"));
                    egui::ComboBox::from_id_salt("hp_compile_lighting")
                        .selected_text(match s.lighting {
                            LightingSetting::Ldr => t!("compile.lighting.ldr"),
                            LightingSetting::Hdr => t!("compile.lighting.hdr"),
                            LightingSetting::Both => t!("compile.lighting.both"),
                        })
                        .show_ui(ui, |ui| {
                            for (l, key) in [
                                (LightingSetting::Ldr, "compile.lighting.ldr"),
                                (LightingSetting::Hdr, "compile.lighting.hdr"),
                                (LightingSetting::Both, "compile.lighting.both"),
                            ] {
                                ui.selectable_value(&mut s.lighting, l, t!(key));
                            }
                        })
                        .response
                        .on_hover_text(t!("compile.lighting_tip"));
                });
                if changed {
                    settle_preset(s);
                }
            });
        });
        if s.expert && self.external_name.is_none() {
            form.row(ui, "", |ui| {
                ui.label(RichText::new(t!("compile.expert.active")).color(theme::warning()).small());
            });
        }

        let lines = &self.preview.lines;
        ui.horizontal_top(|ui| {
            ui.add_space(form.label_width() + ui.spacing().item_spacing.x);
            ui.vertical(|ui| {
                for line in lines {
                    let text = RichText::new(&line.line).monospace().small().weak();
                    let resp = lazy_hover_text(ui.add(egui::Label::new(text).truncate()), || {
                        let mut tip = super::step_name(line.kind).to_owned();
                        for (flag, explanation) in &line.flags {
                            tip.push_str(&format!("\n{flag}: {explanation}"));
                        }
                        tip
                    });
                    resp.context_menu(|ui| {
                        if ui.button(t!("compile.copy_command")).clicked() {
                            ui.ctx().copy_text(line.line.clone());
                            ui.close();
                        }
                    });
                }
            });
        });

        if cordons > 0 {
            form.row(ui, RichText::new(format!("⚠ {}", t!("compile.cordon"))).color(theme::warning()), |ui| {
                ui.checkbox(&mut s.compile_cordon_only, tf!("compile.cordon_only", cordons))
                    .on_hover_text(t!("compile.cordon_only_tip"));
            });
        }

        if !external {
            form.row(ui, t!("compile.static_props"), |ui| {
                ui.add_enabled_ui(!s.expert, |ui| super::options_ui::prop_toggles(ui, s));
            });
        }

        form.row(ui, t!("compile.before"), |ui| {
            ui.checkbox(&mut s.save_before_compile, t!("compile.save_before"))
                .on_hover_text(t!("compile.save_before_tip"));
            ui.checkbox(&mut s.compile_hidden, t!("compile.compile_hidden"))
                .on_hover_text(t!("compile.compile_hidden_tip"));
            let mut anyway = !s.stop_on_leak;
            if ui
                .checkbox(&mut anyway, t!("compile.leaking_anyway"))
                .on_hover_text(t!("compile.leaking_anyway_tip"))
                .changed()
            {
                s.stop_on_leak = !anyway;
            }
        });

        form.row(ui, t!("compile.content"), |ui| {
            ui.checkbox(&mut s.pack_custom_content, t!("compile.pack_content"))
                .on_hover_text(t!("compile.pack_content_tip"));
            wrap_group(ui, "pack_policy", |ui| {
                ui.add_enabled_ui(s.pack_custom_content, |ui| {
                    egui::ComboBox::from_id_salt("hp_compile_pack_policy")
                        .selected_text(match s.pack_policy {
                            PackPolicy::ReleaseOnly => t!("compile.pack_release_only"),
                            PackPolicy::Always => t!("compile.pack_always"),
                        })
                        .show_ui(ui, |ui| {
                            ui.selectable_value(
                                &mut s.pack_policy,
                                PackPolicy::ReleaseOnly,
                                t!("compile.pack_release_only"),
                            );
                            ui.selectable_value(&mut s.pack_policy, PackPolicy::Always, t!("compile.pack_always"));
                        });
                });
            });
            ui.checkbox(&mut s.dev_addon, t!("compile.dev_addon_option")).on_hover_text(t!("compile.dev_addon_tip"));
            ui.checkbox(&mut s.export_custom_materials, t!("compile.export_materials"))
                .on_hover_text(t!("compile.export_materials_tip"));
        });

        form.row(ui, t!("compile.after"), |ui| {
            ui.checkbox(&mut s.copy_to_maps, t!("compile.copy_to_maps")).on_hover_text(t!("compile.copy_to_maps_tip"));
            ui.checkbox(&mut s.bsp_beside_vmf, t!("compile.bsp_beside_vmf"))
                .on_hover_text(t!("compile.bsp_beside_vmf_tip"));
            wrap_group(ui, "launch", |ui| {
                ui.checkbox(&mut s.launch_game, t!("compile.launch_game")).on_hover_text(t!("compile.launch_game_tip"));
                ui.add_enabled(s.launch_game, egui::Checkbox::new(&mut s.launch_via_steam, t!("compile.via_steam")))
                    .on_hover_text(t!("compile.via_steam_tip"));
            });
            ui.checkbox(&mut s.load_pointfile_on_leak, t!("compile.load_pointfile"))
                .on_hover_text(t!("compile.load_pointfile_tip"));
        });

        form.row(ui, t!("compile.game_args"), |ui| {
            ui.add_enabled(
                s.launch_game && !s.launch_via_steam,
                egui::TextEdit::singleline(&mut s.game_args).desired_width(ui.available_width().max(120.0)),
            )
            .on_hover_text(t!("compile.game_args_tip"));
        });

        form.row(ui, t!("compile.threads"), |ui| {
            let mut auto = s.threads.is_none();
            let auto_n =
                hp_compile::flags::auto_threads(s.low_priority).unwrap_or_else(hp_compile::flags::logical_cores);
            wrap_group(ui, "threads", |ui| {
                if ui
                    .checkbox(&mut auto, tf!("compile.threads_auto_n", auto_n))
                    .on_hover_text(t!("compile.threads_tip"))
                    .changed()
                {
                    s.threads = if auto { None } else { Some(auto_n) };
                }
                if let Some(t) = &mut s.threads {
                    ui.add(egui::DragValue::new(t).range(1..=256).speed(0.2));
                }
            });
            ui.checkbox(&mut s.low_priority, t!("compile.low_priority")).on_hover_text(t!("compile.low_priority_tip"));
        });
    }

    fn advanced_ui(ui: &mut Ui, state: &mut AppState, probe: &ToolProbe) -> Option<BrowseTarget> {
        let mut browse = None;
        let s = &mut state.settings.compile;
        let seal = &mut state.settings.visibility.cordon_material;
        let mut seal_changed = false;
        egui::Grid::new("hp_compile_adv_grid").num_columns(2).spacing([12.0, 6.0]).show(ui, |ui| {
            for (label, tip, value) in [
                ("compile.extra_bsp", "compile.extra_bsp_tip", &mut s.extra_bsp_args),
                ("compile.extra_rad", "compile.extra_rad_tip", &mut s.extra_rad_args),
            ] {
                ui.label(t!(label));
                ui.add(egui::TextEdit::singleline(value).desired_width(f32::INFINITY).hint_text("-option value"))
                    .on_hover_text(t!(tip));
                ui.end_row();
            }

            ui.label(t!("compile.background"));
            ui.horizontal_wrapped(|ui| {
                ui.add_enabled(
                    s.low_priority,
                    egui::Checkbox::new(&mut s.background_precompile, t!("compile.background_option")),
                )
                .on_hover_text(t!("compile.background_tip"));
                ui.checkbox(&mut s.progressive, t!("compile.progressive")).on_hover_text(t!("compile.progressive_tip"));
                ui.checkbox(&mut s.history, t!("compile.history_option"));
            });
            ui.end_row();

            ui.label(t!("compile.cordon_material"));
            seal_changed = ui
                .add(
                    egui::TextEdit::singleline(seal)
                        .desired_width(240.0)
                        .hint_text(hp_app::settings::VisibilitySettings::default().cordon_material),
                )
                .changed();
            ui.end_row();

            ui.label(t!("compile.game_exe"));
            ui.horizontal(|ui| {
                let detected =
                    probe.result.as_ref().and_then(|r| r.resolved.as_ref()).and_then(|r| r.tools.game_exe.clone());
                let text = match (&s.game_exe_path, detected) {
                    (Some(p), _) if !p.as_os_str().is_empty() => p.display().to_string(),
                    (_, Some(u)) => tf!("compile.automatic_path", u.display()),
                    _ => t!("compile.game_exe_none").to_owned(),
                };
                if ui.button(t!("common.browse")).clicked() {
                    browse = Some(BrowseTarget::GameExe);
                }
                if s.game_exe_path.is_some() && ui.small_button("✖").on_hover_text(t!("common.reset")).clicked() {
                    s.game_exe_path = None;
                }
                ui.add(egui::Label::new(RichText::new(text).weak()).truncate());
            });
            ui.end_row();

            ui.label(t!("compile.maps_dir"));
            ui.horizontal(|ui| {
                if ui.button(t!("common.browse")).clicked() {
                    browse = Some(BrowseTarget::MapsDir);
                }
                if s.maps_dir_override.is_some() && ui.small_button("✖").on_hover_text(t!("common.reset")).clicked() {
                    s.maps_dir_override = None;
                }
                let text = s
                    .maps_dir_override
                    .as_ref()
                    .map_or_else(|| t!("compile.maps_dir_default").to_owned(), |p| p.display().to_string());
                ui.add(egui::Label::new(RichText::new(text).weak()).truncate());
            });
            ui.end_row();
        });
        if seal_changed {
            state.settings_dirty = true;
        }
        browse
    }

    fn tools_status_ui(&self, ui: &mut Ui) {
        match &self.probe.result {
            None => {
                ui.horizontal(|ui| {
                    ui.spinner();
                    ui.label(RichText::new(t!("compile.detecting")).weak());
                });
            }
            Some(r) => match (&r.resolved, &r.error) {
                (Some(res), _) if r.missing.is_empty() => {
                    ui.label(
                        RichText::new(format!("✔ {}", super::tools::tools_text(&res.tools)))
                            .color(theme::success())
                            .small(),
                    );
                }
                (Some(_), _) => {
                    let list: Vec<String> = r.missing.iter().map(|p| p.display().to_string()).collect();
                    ui.label(
                        RichText::new(format!("⚠ {}", tf!("compile.err_missing_files", list.join(", "))))
                            .color(theme::warning())
                            .small(),
                    );
                }
                (None, Some(e)) => {
                    ui.label(RichText::new(format!("⚠ {e}")).color(theme::warning()).small());
                }
                (None, None) => {
                    let text = super::tools::no_game_text(r.root.as_deref());
                    ui.label(RichText::new(format!("⚠ {text}")).color(theme::warning()).small());
                }
            },
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn step_choices_settle_the_preset() {
        let mut s = CompileSettings::default();
        plan::apply_preset(&mut s, CompilePresetSetting::Draft);
        s.vis_mode = VisModeSetting::Normal;
        settle_preset(&mut s);
        assert_eq!(s.preset, CompilePresetSetting::Custom);
        s.vis_mode = VisModeSetting::Fast;
        settle_preset(&mut s);
        assert_eq!(s.preset, CompilePresetSetting::Draft, "back to the named preset");
        s.run_rad = false;
        settle_preset(&mut s);
        assert_eq!(s.preset, CompilePresetSetting::Custom);
        assert!(!smart_applies(&s));
        let mut old =
            CompileSettings { preset: CompilePresetSetting::Draft, run_vis: false, ..CompileSettings::default() };
        normalize_steps(&mut old);
        assert_eq!(old.preset, CompilePresetSetting::Custom);
        assert_eq!(
            effective_modes(&old),
            hp_compile::StepModes::new(BspMode::Full, hp_compile::VisMode::Off, hp_compile::RadMode::Draft)
        );
        let mut fresh = CompileSettings::default();
        normalize_steps(&mut fresh);
        assert_eq!(fresh, CompileSettings::default(), "nothing to do");
        let mut n = CompileSettings::default();
        assert!(smart_applies(&n));
        n.smart_recompile = false;
        assert!(!smart_applies(&n));
    }

    #[test]
    fn settings_rows_wrap_instead_of_overflowing() {
        let _lang = crate::lang_lock();
        crate::register_translations();
        let mut state = hp_app::testing::test_state();
        let mut d = CompileDialog::new();
        for lang in hp_app::i18n::Lang::ALL {
            hp_app::i18n::set_lang(lang);
            for width in [460.0f32, 640.0, 900.0] {
                let ctx = egui::Context::default();
                let mut used = egui::Rect::NOTHING;
                for _ in 0..3 {
                    let input = egui::RawInput {
                        screen_rect: Some(egui::Rect::from_min_size(egui::Pos2::ZERO, egui::vec2(1600.0, 1200.0))),
                        ..Default::default()
                    };
                    let mut out = ctx.run_ui(input, |ui| {
                        let r = ui.allocate_ui(egui::vec2(width, 1100.0), |ui| {
                            ui.set_max_width(width);
                            d.main_grid(ui, &mut state, 1);
                        });
                        used = r.response.rect;
                    });
                    out.textures_delta.clear();
                }
                if used.width() > width + 1.0 {
                    hp_app::i18n::set_lang(hp_app::i18n::Lang::En);
                    panic!("{lang:?} {width}: the compile settings use {} points", used.width());
                }
                let form = d.form.map(|(_, f)| f.label_width()).unwrap_or_default();
                assert!((90.0..=190.0).contains(&form), "{lang:?}: label column {form}");
            }
        }
        hp_app::i18n::set_lang(hp_app::i18n::Lang::En);
    }

    #[test]
    fn flags_are_explained_in_both_languages() {
        let _lang = crate::lang_lock();
        crate::register_translations();
        for lang in hp_app::i18n::Lang::ALL {
            hp_app::i18n::set_lang(lang);
            for f in hp_compile::flags::FLAGS {
                let key = format!("compile.flag.{}", f.key());
                assert!(hp_app::i18n::has_key(&key), "{key}");
                assert!(flag_text(f.step, f.flag).is_some_and(|t| !t.starts_with("compile.")));
            }
        }
        hp_app::i18n::set_lang(hp_app::i18n::Lang::En);
        assert!(flag_text(StepKind::Rad, "-unknown").is_none());
    }
}
