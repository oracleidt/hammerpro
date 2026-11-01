use std::path::{Path, PathBuf};

use egui::{RichText, Ui};
use hp_app::settings::CustomDirSetting;
use hp_app::{AppState, Severity, t, tf};
use hp_fs::{CustomLayout, CustomMode};

use crate::jobs::{GameScan, Job, PickRequest, PickTarget, Picker, Poll, scan_game_folder};

#[derive(Debug, Default)]
pub struct GameJobs {
    picker: Option<Picker>,
    detect: Option<Job<Option<PathBuf>>>,
    scan: Option<(PathBuf, Job<GameScan>)>,
    pub last_scan: Option<GameScan>,
    pub auto_detect_done: bool,
}

impl GameJobs {
    pub fn poll(&mut self, state: &mut AppState) {
        if let Some(picker) = &self.picker
            && let Some(paths) = picker.poll()
        {
            let target = picker.target;
            self.picker = None;
            apply_pick(state, target, paths);
        }
        if let Some(job) = &self.detect {
            match job.poll() {
                Poll::Pending => {}
                Poll::Ready(found) => {
                    self.detect = None;
                    match found {
                        Some(root) => {
                            state.log(Severity::Success, tf!("shell.game.detected", root.display()));
                            state.settings.game.gmod_root = Some(root);
                            state.settings_dirty = true;
                        }
                        None => state.notify(Severity::Warning, t!("shell.game.not_found")),
                    }
                }
                Poll::Failed => self.detect = None,
            }
        }
        if let Some((_, job)) = &self.scan {
            match job.poll() {
                Poll::Pending => {}
                Poll::Ready(scan) => {
                    self.last_scan = Some(scan);
                    self.scan = None;
                }
                Poll::Failed => self.scan = None,
            }
        }
    }

    pub fn picking(&self) -> bool {
        self.picker.is_some()
    }

    pub fn detecting(&self) -> bool {
        self.detect.is_some()
    }

    pub fn pick(&mut self, ctx: &egui::Context, req: PickRequest) {
        if self.picker.is_none() {
            self.picker = Some(Picker::open(ctx, req));
        }
    }

    pub fn start_detect(&mut self, ctx: &egui::Context) {
        if self.detect.is_none() {
            self.detect = Some(Job::spawn(ctx, "detect-gmod", hp_fs::detect_gmod_root));
        }
    }

    pub fn ensure_scan(&mut self, ctx: &egui::Context, root: Option<&Path>) {
        let Some(root) = root.filter(|r| !r.as_os_str().is_empty()) else {
            self.last_scan = None;
            return;
        };
        let scanning_this = self.scan.as_ref().is_some_and(|(p, _)| p == root);
        let scanned_this =
            self.last_scan.as_ref().is_some_and(|s| s.root == root || s.root == root.parent().unwrap_or(root));
        if scanning_this || (scanned_this && self.scan.is_none()) {
            return;
        }
        let path = root.to_path_buf();
        let job_path = path.clone();
        self.scan = Some((path, Job::spawn(ctx, "scan-game", move || scan_game_folder(&job_path))));
    }

    pub fn rescan(&mut self) {
        self.last_scan = None;
    }

    pub fn scanning(&self) -> bool {
        self.scan.is_some()
    }
}

pub fn apply_pick(state: &mut AppState, target: PickTarget, paths: Vec<PathBuf>) {
    if paths.is_empty() {
        return;
    }
    let game = &mut state.settings.game;
    let first = paths.first().cloned();
    match target {
        PickTarget::GmodRoot => game.gmod_root = first,
        PickTarget::MapDir => game.map_dir = first,
        PickTarget::GameDir => {
            if let Some(dir) = first {
                let id = game.game.clone();
                let dir = hp_fs::games::game_folder_of(&dir, &id);
                game.game_dirs.insert(id, dir);
            }
        }
        PickTarget::FgdOverride => game.fgd_override = first,
        PickTarget::ExtraGameDir => {
            for p in paths {
                if !game.extra_game_dirs.contains(&p) {
                    game.extra_game_dirs.push(p);
                }
            }
        }
        PickTarget::ExtraFgds => {
            for p in paths {
                if !game.extra_fgds.contains(&p) {
                    game.extra_fgds.push(p);
                }
            }
        }
        PickTarget::CustomTextureDirs => {
            for p in paths {
                if !game.custom_texture_dirs.iter().any(|d| d.path == p) {
                    let prefix = hp_fs::suggest_prefix(&p);
                    game.custom_texture_dirs.push(CustomDirSetting { path: p, prefix, enabled: true });
                }
            }
        }
    }
    state.settings_dirty = true;
}

pub fn request(target: PickTarget, start_dir: Option<PathBuf>) -> PickRequest {
    let (title, filter) = match target {
        PickTarget::GmodRoot => (t!("shell.game.pick_root"), None),
        PickTarget::ExtraGameDir => (t!("shell.game.pick_extra_dir"), None),
        PickTarget::MapDir => (t!("shell.game.pick_map_dir"), None),
        PickTarget::GameDir => (t!("shell.game.pick_game_dir"), None),
        PickTarget::CustomTextureDirs => (t!("shell.game.pick_custom_dir"), None),
        PickTarget::FgdOverride | PickTarget::ExtraFgds => {
            (t!("shell.game.pick_fgd"), Some((t!("shell.game.fgd_filter").to_owned(), vec!["fgd"])))
        }
    };
    PickRequest { target, title: title.to_owned(), start_dir, filter }
}

pub fn optional_path_edit(ui: &mut Ui, value: &mut Option<PathBuf>, hint: &str, width: f32) -> bool {
    let mut text = value.as_ref().map(|p| p.display().to_string()).unwrap_or_default();
    let r = ui.add(egui::TextEdit::singleline(&mut text).hint_text(hint).desired_width(width));
    if r.changed() {
        let trimmed = text.trim();
        *value = if trimmed.is_empty() { None } else { Some(PathBuf::from(trimmed)) };
        return true;
    }
    false
}

#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub struct PathRowOutput {
    pub changed: bool,
    pub browse: bool,
}

pub fn path_row(
    ui: &mut Ui,
    label: &str,
    label_width: Option<f32>,
    value: &mut Option<PathBuf>,
    hint: &str,
    browse_enabled: bool,
) -> (PathRowOutput, egui::Response) {
    let mut out = PathRowOutput::default();
    let row_height = ui.spacing().interact_size.y;
    let inner = ui.horizontal(|ui| {
        let label_response = match label_width {
            Some(w) => crate::common::fixed_label(ui, w, row_height, label),
            None => ui.label(label),
        };
        ui.allocate_ui_with_layout(
            egui::vec2(ui.available_width(), row_height),
            egui::Layout::right_to_left(egui::Align::Center),
            |ui| {
                if value.is_some() && ui.small_button("✖").on_hover_text(t!("common.clear")).clicked() {
                    *value = None;
                    out.changed = true;
                }
                if ui.add_enabled(browse_enabled, egui::Button::new(t!("common.browse"))).clicked() {
                    out.browse = true;
                }
                let width = ui.available_width().max(120.0);
                out.changed |= optional_path_edit(ui, value, hint, width);
            },
        );
        label_response
    });
    (out, inner.inner)
}

fn found_label(ui: &mut Ui, ok: bool, text: &str) {
    let severity = if ok { hp_app::Severity::Success } else { hp_app::Severity::Error };
    let (icon, color) = (severity.icon(), crate::common::severity_color(ui.visuals(), severity));
    ui.label(RichText::new(icon).color(color));
    ui.label(text);
}

pub fn default_is_gmod(state: &AppState) -> bool {
    let g = state.settings.game.game.trim();
    g.is_empty() || g.eq_ignore_ascii_case(hp_fs::GMOD_ID)
}

pub fn game_section(ui: &mut Ui, state: &mut AppState, jobs: &mut GameJobs) {
    let ctx = ui.ctx().clone();
    let list = hp_app::games::catalogue(Some(&ctx));
    ui.horizontal(|ui| {
        match &list {
            Some(list) => {
                if let Some(id) = hp_app::games::game_combo(ui, "hp_shell_game", &state.settings.game.game, list) {
                    state.settings.game.game = id;
                    state.settings_dirty = true;
                }
            }
            None => {
                ui.spinner();
                ui.label(t!("shell.game.searching_games"));
            }
        }
        if ui
            .add_enabled(list.is_some(), egui::Button::new(t!("shell.game.search_again")))
            .on_hover_text(t!("shell.game.search_again_tip"))
            .clicked()
        {
            hp_app::games::refresh_catalogue();
        }
    });
    ui.label(RichText::new(t!("shell.game.game_note")).weak().small());
    if default_is_gmod(state) {
        return;
    }
    let id = state.settings.game.game.clone();
    let mut dir = state.settings.game.game_dirs.get(&id).cloned();
    let (out, _) =
        path_row(ui, t!("shell.game.game_dir"), None, &mut dir, t!("shell.game.game_dir_hint"), !jobs.picking());
    if out.changed {
        match dir {
            Some(d) => state.settings.game.game_dirs.insert(id.clone(), d),
            None => state.settings.game.game_dirs.remove(&id),
        };
        state.settings_dirty = true;
    }
    if out.browse {
        jobs.pick(&ctx, request(PickTarget::GameDir, state.settings.game.game_dirs.get(&id).cloned()));
    }
    let Some(list) = list else { return };
    let chosen = state.settings.game.game_dir_of(&id).map(Path::to_path_buf);
    let profile = match chosen {
        Some(d) => hp_fs::GameProfile::from_game_dir(&hp_fs::games::game_folder_of(&d, &id), &[]),
        None => list.iter().find(|p| p.id.eq_ignore_ascii_case(&id)).cloned(),
    };
    let Some(p) = profile else {
        found_label(ui, false, t!("shell.game.game_not_found"));
        return;
    };
    if let Some(reason) = &p.unsupported {
        found_label(ui, false, &tf!("shell.game.game_unsupported", reason));
        return;
    }
    if p.installed {
        found_label(ui, true, &tf!("shell.game.game_found", p.game_dir.display()));
    } else {
        found_label(ui, false, t!("shell.game.game_missing"));
    }
    let f = p.format();
    let limits = if f.limits == hp_fs::games::LimitSet::Gmod {
        t!("shell.game.limits_gmod")
    } else {
        t!("shell.game.limits_valve")
    };
    ui.label(tf!("shell.game.game_format", f.branch.name(), f.version, f.static_props.name(), limits));
    let fgds: Vec<String> = p.fgds.iter().map(|f| f.display().to_string()).collect();
    ui.label(if fgds.is_empty() {
        t!("shell.game.game_no_fgd").to_owned()
    } else {
        tf!("shell.game.game_fgds", fgds.join(", "))
    });
    if let Some(exe) = &p.exe {
        ui.label(tf!("shell.game.game_exe", exe.display()));
    }
    if p.mapbase {
        ui.label(RichText::new(t!("shell.game.game_mapbase")).weak());
    }
    ui.label(RichText::new(t!("shell.game.gmod_only_note")).weak().small());
}

pub fn gmod_root_section(ui: &mut Ui, state: &mut AppState, jobs: &mut GameJobs) {
    let ctx = ui.ctx().clone();
    let row_height = ui.spacing().interact_size.y;
    ui.allocate_ui_with_layout(
        egui::vec2(ui.available_width(), row_height),
        egui::Layout::right_to_left(egui::Align::Center),
        |ui| {
            if jobs.detecting() {
                ui.label(RichText::new(t!("shell.game.detecting")).small());
                ui.add(egui::Spinner::new().size(14.0));
            } else if ui
                .button(format!("🔍 {}", t!("shell.game.detect")))
                .on_hover_text(t!("shell.game.detect_tip"))
                .clicked()
            {
                jobs.start_detect(&ctx);
            }
            let browse = ui.add_enabled(!jobs.picking(), egui::Button::new(t!("common.browse")));
            if browse.clicked() {
                let start = state.settings.game.gmod_root.clone();
                jobs.pick(&ctx, request(PickTarget::GmodRoot, start));
            }
            let width = ui.available_width().max(120.0);
            if optional_path_edit(ui, &mut state.settings.game.gmod_root, t!("shell.game.root_hint"), width) {
                state.settings_dirty = true;
            }
        },
    );
    jobs.ensure_scan(&ctx, state.settings.game.gmod_root.as_deref());
    ui.add_space(2.0);
    if state.settings.game.gmod_root.is_none() {
        ui.label(RichText::new(t!("shell.game.auto_detect_note")).small().color(ui.visuals().weak_text_color()));
        return;
    }
    if jobs.scanning() {
        ui.horizontal(|ui| {
            ui.add(egui::Spinner::new().size(12.0));
            ui.label(RichText::new(t!("shell.game.scanning")).small());
        });
        return;
    }
    let Some(scan) = jobs.last_scan.clone() else { return };
    ui.horizontal_wrapped(|ui| {
        ui.spacing_mut().item_spacing.x = 4.0;
        found_label(ui, scan.gameinfo, "gameinfo.txt");
        ui.add_space(8.0);
        found_label(ui, scan.fgd.is_some(), "garrysmod.fgd");
        ui.add_space(8.0);
        let weak = ui.visuals().weak_text_color();
        ui.label(RichText::new("·").color(weak));
        ui.label(RichText::new(tf!("shell.game.vpk_count", scan.vpk_count)).color(weak));
        ui.label(RichText::new("·").color(weak));
        ui.label(RichText::new(tf!("shell.game.addon_count", scan.addon_count)).color(weak));
        if ui.small_button("⟳").on_hover_text(t!("shell.game.rescan")).clicked() {
            jobs.rescan();
        }
    });
    if !scan.is_valid() {
        let color = crate::common::severity_color(ui.visuals(), hp_app::Severity::Warning);
        ui.label(RichText::new(t!("shell.game.invalid_root")).color(color));
    }
}

pub fn mount_toggles(ui: &mut Ui, state: &mut AppState) -> bool {
    let g = &mut state.settings.game;
    let mut changed = false;
    changed |= ui
        .checkbox(&mut g.mount_addons, t!("shell.game.mount_addons"))
        .on_hover_text(t!("shell.game.mount_addons_tip"))
        .changed();
    changed |= ui
        .checkbox(&mut g.mount_workshop, t!("shell.game.mount_workshop"))
        .on_hover_text(t!("shell.game.mount_workshop_tip"))
        .changed();
    changed |= ui
        .checkbox(&mut g.mount_cfg, t!("shell.game.mount_cfg"))
        .on_hover_text(t!("shell.game.mount_cfg_tip"))
        .changed();
    if changed {
        state.settings_dirty = true;
    }
    changed
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub enum PrefixCell {
    Fixed { shown: String, mode: CustomMode },
    Edit { text: String, detected: bool },
}

pub fn prefix_cell(dir: &CustomDirSetting, layout: Option<&CustomLayout>) -> PrefixCell {
    match layout {
        Some(l) if matches!(l.mode, CustomMode::GameFolder | CustomMode::InsideMaterials) => {
            let shown = if l.prefix.is_empty() { "materials/".to_owned() } else { l.prefix.clone() };
            PrefixCell::Fixed { shown, mode: l.mode }
        }
        Some(l) if l.mode == CustomMode::Detected && is_auto_prefix(dir) => {
            PrefixCell::Edit { text: l.prefix.clone(), detected: true }
        }
        _ => PrefixCell::Edit { text: dir.prefix.clone(), detected: false },
    }
}

fn is_auto_prefix(dir: &CustomDirSetting) -> bool {
    hp_fs::is_auto_prefix(&hp_fs::CustomDir {
        path: dir.path.clone(),
        prefix: dir.prefix.clone(),
        enabled: dir.enabled,
    })
}

pub fn edited_prefix(original: &str, shown: &str, text: &str) -> String {
    if text == shown { original.to_owned() } else { text.to_owned() }
}

#[derive(Clone, Debug, Default)]
struct PrefixEdit {
    original: String,
    shown: String,
    text: String,
}

const PREFIX_WIDTH: f32 = 190.0;

fn prefix_cell_ui(ui: &mut Ui, dir: &mut CustomDirSetting, cell: &PrefixCell) -> bool {
    let size = egui::vec2(PREFIX_WIDTH, 20.0);
    let (text, detected) = match cell {
        PrefixCell::Fixed { shown, mode } => {
            let tip = match mode {
                CustomMode::GameFolder => t!("shell.game.prefix_mode_game"),
                _ => t!("shell.game.prefix_mode_inside"),
            };
            let label = egui::Label::new(RichText::new(shown).monospace().small()).truncate();
            ui.allocate_ui_with_layout(size, egui::Layout::left_to_right(egui::Align::Center), |ui| {
                ui.add(label).on_hover_text(tip);
            });
            return false;
        }
        PrefixCell::Edit { text, detected } => (text, *detected),
    };
    let mut changed = false;
    ui.allocate_ui_with_layout(size, egui::Layout::right_to_left(egui::Align::Center), |ui| {
        ui.spacing_mut().item_spacing.x = 4.0;
        if detected {
            ui.label(RichText::new(t!("shell.game.prefix_detected")).small().color(hp_app::theme::accent()))
                .on_hover_text(t!("shell.game.prefix_detected_tip"));
        }
        let id = ui.make_persistent_id(("hp_shell_custom_prefix", &dir.path));
        let focused = ui.memory(|m| m.has_focus(id));
        let mut edit = focused
            .then(|| ui.data(|d| d.get_temp::<PrefixEdit>(id)))
            .flatten()
            .unwrap_or_else(|| PrefixEdit { original: dir.prefix.clone(), shown: text.clone(), text: text.clone() });
        let hint = RichText::new(hp_fs::suggest_prefix(&dir.path)).monospace();
        let r = ui.add_sized(
            [ui.available_width().max(60.0), 20.0],
            egui::TextEdit::singleline(&mut edit.text).id(id).hint_text(hint),
        );
        if r.changed() {
            let prefix = edited_prefix(&edit.original, &edit.shown, &edit.text);
            if prefix != dir.prefix {
                dir.prefix = prefix;
                changed = true;
            }
        }
        if r.has_focus() {
            ui.data_mut(|d| d.insert_temp(id, edit));
        } else {
            ui.data_mut(|d| d.remove::<PrefixEdit>(id));
        }
    });
    changed
}

pub fn custom_dirs_list(ui: &mut Ui, state: &mut AppState, jobs: &mut GameJobs) {
    let ctx = ui.ctx().clone();
    let mut remove = None;
    let mut changed = false;
    let fs = state.assets.fs();
    let dirs = &mut state.settings.game.custom_texture_dirs;
    if dirs.is_empty() {
        ui.label(RichText::new(t!("shell.game.custom_empty")).small().color(ui.visuals().weak_text_color()));
    } else {
        let cells: Vec<PrefixCell> =
            dirs.iter().map(|d| prefix_cell(d, fs.and_then(|fs| fs.custom_layout_of_path(&d.path)).as_ref())).collect();
        egui::Grid::new("hp_shell_custom_dirs").num_columns(4).striped(true).spacing([8.0, 4.0]).show(ui, |ui| {
            ui.label(RichText::new(t!("common.enabled")).small().strong());
            ui.label(RichText::new(t!("common.path")).small().strong());
            ui.label(RichText::new(t!("shell.game.prefix")).small().strong())
                .on_hover_text(t!("shell.game.prefix_tip"));
            ui.label("");
            ui.end_row();
            for (i, (dir, cell)) in dirs.iter_mut().zip(&cells).enumerate() {
                changed |= ui.checkbox(&mut dir.enabled, "").changed();
                let text = dir.path.display().to_string();
                crate::common::fixed_label(ui, 260.0, 18.0, RichText::new(&text).monospace().small())
                    .on_hover_text(&text);
                changed |= prefix_cell_ui(ui, dir, cell);
                if ui.small_button("✖").on_hover_text(t!("common.remove")).clicked() {
                    remove = Some(i);
                }
                ui.end_row();
            }
        });
    }
    if let Some(i) = remove {
        dirs.remove(i);
        changed = true;
    }
    if changed {
        state.settings_dirty = true;
    }
    ui.horizontal(|ui| {
        if ui.add_enabled(!jobs.picking(), egui::Button::new(format!("➕ {}", t!("shell.game.add_custom")))).clicked()
        {
            jobs.pick(&ctx, request(PickTarget::CustomTextureDirs, None));
        }
        ui.label(RichText::new(t!("shell.game.custom_tip")).small().color(ui.visuals().weak_text_color()));
    });
}

pub fn parse_app_ids(text: &str) -> Result<Vec<u32>, String> {
    let mut out = Vec::new();
    for part in text.split([',', ';', ' ']).map(str::trim).filter(|p| !p.is_empty()) {
        match part.parse::<u32>() {
            Ok(id) if id > 0 && id <= hp_fs::MAX_APP_ID => out.push(id),
            _ => return Err(part.to_owned()),
        }
    }
    Ok(out)
}

pub fn add_app_ids(list: &mut Vec<u32>, ids: &[u32]) -> usize {
    let mut added = 0;
    for id in ids {
        if *id > 0 && !list.contains(id) {
            list.push(*id);
            added += 1;
        }
    }
    added
}

pub fn move_app_id(list: &mut [u32], index: usize, up: bool) -> bool {
    let other = if up { index.checked_sub(1) } else { index.checked_add(1) };
    match other.filter(|o| *o < list.len()) {
        Some(o) if index < list.len() => {
            list.swap(index, o);
            true
        }
        _ => false,
    }
}

pub fn app_mounts_section(ui: &mut Ui, state: &mut AppState) {
    let ids = state.settings.game.mount_app_ids.clone();
    let mut changed = false;
    if ids.is_empty() {
        ui.label(RichText::new(t!("shell.game.apps_empty")).small().color(ui.visuals().weak_text_color()));
    } else {
        let mut action: Option<(usize, i8)> = None;
        egui::Grid::new("hp_shell_app_mounts").num_columns(4).striped(true).spacing([8.0, 2.0]).show(ui, |ui| {
            for (i, id) in ids.iter().enumerate() {
                ui.label(RichText::new(id.to_string()).monospace());
                match hp_fs::find_steam_app(*id) {
                    Some(app) => {
                        let tip = app.game_dirs.iter().map(|d| d.display().to_string()).collect::<Vec<_>>().join("\n");
                        let tip = if tip.is_empty() { tf!("shell.game.apps_no_game", app.root.display()) } else { tip };
                        ui.label(&app.name).on_hover_text(tip);
                    }
                    None => {
                        let name = hp_fs::steam_app_name(*id).unwrap_or_default();
                        ui.label(
                            RichText::new(tf!("shell.game.apps_missing", name).trim().to_owned())
                                .color(ui.visuals().warn_fg_color),
                        );
                    }
                }
                ui.horizontal(|ui| {
                    if ui
                        .add_enabled(i > 0, egui::Button::new("⏶").small())
                        .on_hover_text(t!("shell.game.apps_up"))
                        .clicked()
                    {
                        action = Some((i, -1));
                    }
                    if ui
                        .add_enabled(i + 1 < ids.len(), egui::Button::new("⏷").small())
                        .on_hover_text(t!("shell.game.apps_down"))
                        .clicked()
                    {
                        action = Some((i, 1));
                    }
                });
                if ui.small_button("✖").on_hover_text(t!("common.remove")).clicked() {
                    action = Some((i, 0));
                }
                ui.end_row();
            }
        });
        let list = &mut state.settings.game.mount_app_ids;
        match action {
            Some((i, 0)) if i < list.len() => {
                list.remove(i);
                changed = true;
            }
            Some((i, d)) => changed |= move_app_id(list, i, d < 0),
            None => {}
        }
    }
    let input_id = ui.make_persistent_id("hp_shell_app_input");
    let mut text: String = ui.data(|d| d.get_temp(input_id)).unwrap_or_default();
    let mut error: Option<String> = None;
    ui.horizontal(|ui| {
        ui.label(t!("shell.game.apps_add"));
        let edit = ui.add(egui::TextEdit::singleline(&mut text).desired_width(120.0).hint_text("240, 440"));
        let enter = edit.lost_focus() && ui.input(|i| i.key_pressed(egui::Key::Enter));
        if ui.button(format!("➕ {}", t!("common.add"))).clicked() || enter {
            match parse_app_ids(&text) {
                Ok(ids) if !ids.is_empty() => {
                    changed |= add_app_ids(&mut state.settings.game.mount_app_ids, &ids) > 0;
                    text.clear();
                }
                Ok(_) => {}
                Err(bad) => error = Some(tf!("shell.game.apps_bad", bad)),
            }
        }
        ui.menu_button(t!("shell.game.apps_installed"), |ui| {
            let apps = hp_fs::installed_source_apps();
            if apps.is_empty() {
                ui.label(t!("shell.game.apps_none_installed"));
            }
            egui::ScrollArea::vertical().max_height(320.0).show(ui, |ui| {
                for app in apps {
                    let have = state.settings.game.mount_app_ids.contains(&app.app_id);
                    let label = format!("{}  {}", app.app_id, app.name);
                    if ui.add_enabled(!have, egui::Button::new(label)).clicked() {
                        changed |= add_app_ids(&mut state.settings.game.mount_app_ids, &[app.app_id]) > 0;
                        ui.close();
                    }
                }
            });
        });
        if ui.small_button("⟳").on_hover_text(t!("shell.game.apps_refresh")).clicked() {
            hp_fs::refresh_steam_apps();
        }
    });
    ui.data_mut(|d| d.insert_temp(input_id, text));
    if let Some(e) = error {
        ui.label(RichText::new(e).color(ui.visuals().warn_fg_color));
    }
    ui.label(RichText::new(t!("shell.game.apps_tip")).small().color(ui.visuals().weak_text_color()));
    if changed {
        state.settings_dirty = true;
    }
}

pub fn path_list(ui: &mut Ui, id: &str, paths: &mut Vec<PathBuf>, empty_text: &str) -> (bool, bool) {
    let mut remove = None;
    if paths.is_empty() {
        ui.label(RichText::new(empty_text).small().color(ui.visuals().weak_text_color()));
    } else {
        egui::Grid::new(id).num_columns(2).striped(true).spacing([8.0, 2.0]).show(ui, |ui| {
            for (i, p) in paths.iter().enumerate() {
                let text = p.display().to_string();
                crate::common::fixed_label(ui, 420.0, 18.0, RichText::new(&text).monospace().small())
                    .on_hover_text(&text);
                if ui.small_button("✖").on_hover_text(t!("common.remove")).clicked() {
                    remove = Some(i);
                }
                ui.end_row();
            }
        });
    }
    let removed = remove.is_some();
    if let Some(i) = remove {
        paths.remove(i);
    }
    let add = ui.button(format!("➕ {}", t!("common.add"))).clicked();
    (removed, add)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn picks_update_settings() {
        let mut state = hp_app::testing::test_state();
        apply_pick(&mut state, PickTarget::GmodRoot, vec![PathBuf::from("D:/Games/GarrysMod")]);
        assert_eq!(state.settings.game.gmod_root, Some(PathBuf::from("D:/Games/GarrysMod")));
        assert!(state.settings_dirty);

        apply_pick(
            &mut state,
            PickTarget::CustomTextureDirs,
            vec![PathBuf::from("C:/Art/My Bricks"), PathBuf::from("C:/Art/My Bricks")],
        );
        assert_eq!(state.settings.game.custom_texture_dirs.len(), 1);
        assert_eq!(state.settings.game.custom_texture_dirs[0].prefix, "custom/my_bricks");
        assert!(state.settings.game.custom_texture_dirs[0].enabled);

        apply_pick(
            &mut state,
            PickTarget::ExtraFgds,
            vec![PathBuf::from("a.fgd"), PathBuf::from("b.fgd"), PathBuf::from("a.fgd")],
        );
        assert_eq!(state.settings.game.extra_fgds.len(), 2);
        state.settings_dirty = false;
        apply_pick(&mut state, PickTarget::MapDir, Vec::new());
        assert!(!state.settings_dirty);
        assert!(state.settings.game.map_dir.is_none());
    }

    #[test]
    fn prefix_cell_shows_the_effective_layout() {
        let dir =
            |prefix: &str| CustomDirSetting { path: "C:/Art/My Pack".into(), prefix: prefix.into(), enabled: true };
        let layout = |mode, prefix: &str| CustomLayout { mode, prefix: prefix.into() };
        let detected = layout(CustomMode::Detected, "mypack");
        let edit = |text: &str, detected| PrefixCell::Edit { text: text.into(), detected };
        assert_eq!(prefix_cell(&dir("custom/my_pack"), Some(&detected)), edit("mypack", true));
        assert_eq!(prefix_cell(&dir(""), Some(&detected)), edit("mypack", true));
        assert_eq!(prefix_cell(&dir("art/bricks"), Some(&detected)), edit("art/bricks", false));
        assert_eq!(prefix_cell(&dir("custom/my_pack"), None), edit("custom/my_pack", false));
        let prefixed = layout(CustomMode::Prefixed, "custom/my_pack");
        assert_eq!(prefix_cell(&dir("custom/my_pack"), Some(&prefixed)), edit("custom/my_pack", false));
        let game = layout(CustomMode::GameFolder, "");
        assert_eq!(
            prefix_cell(&dir("custom/my_pack"), Some(&game)),
            PrefixCell::Fixed { shown: "materials/".into(), mode: CustomMode::GameFolder }
        );
        let inside = layout(CustomMode::InsideMaterials, "custom/bricks");
        assert_eq!(
            prefix_cell(&dir("anything"), Some(&inside)),
            PrefixCell::Fixed { shown: "custom/bricks".into(), mode: CustomMode::InsideMaterials }
        );
    }

    #[test]
    fn editing_back_to_the_shown_prefix_keeps_it_automatic() {
        assert_eq!(edited_prefix("custom/my_pack", "mypack", "mypack2"), "mypack2");
        assert_eq!(edited_prefix("custom/my_pack", "mypack", "mypack"), "custom/my_pack");
        assert_eq!(edited_prefix("custom/my_pack", "mypack", ""), "");
        assert_eq!(edited_prefix("art", "art", "art/x"), "art/x");
    }

    #[test]
    fn app_id_list_edits() {
        assert_eq!(parse_app_ids(" 240, 440;4000 "), Ok(vec![240, 440, 4000]));
        assert_eq!(parse_app_ids(""), Ok(Vec::new()));
        assert_eq!(parse_app_ids("240, tf2"), Err("tf2".to_owned()));
        assert_eq!(parse_app_ids("0"), Err("0".to_owned()));
        assert_eq!(parse_app_ids("999999999"), Err("999999999".to_owned()));
        let mut list = vec![220];
        assert_eq!(add_app_ids(&mut list, &[240, 220, 440, 240]), 2);
        assert_eq!(list, [220, 240, 440]);
        assert!(move_app_id(&mut list, 2, true));
        assert_eq!(list, [220, 440, 240]);
        assert!(move_app_id(&mut list, 0, false));
        assert_eq!(list, [440, 220, 240]);
        assert!(!move_app_id(&mut list, 0, true));
        assert!(!move_app_id(&mut list, 2, false));
        assert!(!move_app_id(&mut list, 7, true));
    }

    #[test]
    fn requests_have_titles_and_filters() {
        crate::register_translations();
        let r = request(PickTarget::ExtraFgds, None);
        assert!(!r.title.is_empty());
        assert!(r.filter.is_some_and(|(_, e)| e == ["fgd"]));
        assert!(request(PickTarget::GmodRoot, None).filter.is_none());
    }
}
