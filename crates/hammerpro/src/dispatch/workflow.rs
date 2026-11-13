use std::path::{Path, PathBuf};

use crossbeam_channel::{Receiver, TryRecvError};
use glam::DVec3;
use hp_app::actions::{Category, KeyBindings};
use hp_app::extend::MenuExtension;
use hp_app::i18n::TrTable;
use hp_app::runtime::{WorkflowMapMemory, WorkflowRuntime};
use hp_app::settings::{MAX_LAYOUT_NAME, MAX_LAYOUTS, RenderModeSetting, SavedLayout, SelectModeSetting, clamp_grid};
use hp_app::state::{Pointfile, RightTab};
use hp_app::view::{QuadrantView, ViewKind};
use hp_app::{Action, AppState, Severity, t, tf};
use hp_ui_shell::common::Check;
use hp_ui_shell::menu::{menu_entry, submenu};

use super::{DockTab, Package};
use crate::app::HammerProApp;

pub(crate) struct Workflow;

type PointfileRead = (PathBuf, Result<Vec<[f64; 3]>, String>);

#[derive(Default)]
pub(crate) struct State {
    synced_grid: Option<f64>,
    pointfile: Option<Receiver<PointfileRead>>,
    layout_name: String,
    focus_name: bool,
}

const HISTORY_TAB: &str = "history";

const HANDLED: &[Action] = &[
    Action::ToggleIgnoreGroups,
    Action::Zoom2dIn,
    Action::Zoom2dOut,
    Action::ResetViewSplits,
    Action::ViewTop,
    Action::ViewSide,
    Action::ViewFront,
    Action::View3dTextured,
    Action::View3dFlat,
    Action::CycleView2d,
    Action::ViewLogical,
    Action::ToggleViewInfo,
    Action::ToggleBoundsOverlay,
    Action::ShowHistory,
    Action::ShowToolbar,
    Action::ShowToolPalette,
    Action::ShowStatusBar,
    Action::ToolPaletteTwoColumns,
    Action::ToolbarTwoRows,
    Action::ResetToolbar,
    Action::SaveLayout,
    Action::ResetLayout,
    Action::CloseMap,
    Action::CloseOtherMaps,
    Action::NextMap,
    Action::PreviousMap,
    Action::GridModeGlobal,
    Action::GridModePerView,
    Action::GridModeCustom,
    Action::GridModeSelection,
    Action::GridFromSelection,
    Action::GridFromFace,
    Action::AttachGrid,
    Action::DetachGrid,
    Action::GridManager,
    Action::RotateGrid45,
    Action::AlignGridToSelection,
    Action::ToggleCameraSmoothing,
];

impl Package for Workflow {
    fn handled(&self) -> &'static [Action] {
        HANDLED
    }

    fn handle(&self, app: &mut HammerProApp, _ctx: &egui::Context, action: Action) -> bool {
        match action {
            Action::CloseMap => {
                let id = app.tabs.active_id();
                app.close_tab(id);
                return true;
            }
            Action::CloseOtherMaps => {
                let id = app.tabs.active_id();
                app.close_other_tabs(id);
                return true;
            }
            Action::NextMap => {
                app.cycle_tabs(1);
                return true;
            }
            Action::PreviousMap => {
                app.cycle_tabs(-1);
                return true;
            }
            _ => {}
        }
        let state = &mut app.state;
        if hp_ui_shell::grids::run_grid_action(state, action) {
            return true;
        }
        match action {
            Action::ToggleIgnoreGroups => toggle_ignore_groups(state),
            Action::Zoom2dIn => hp_ui_viewports::zoom_all_2d(state, hp_ui_viewports::camera::ZOOM_STEP),
            Action::Zoom2dOut => hp_ui_viewports::zoom_all_2d(state, 1.0 / hp_ui_viewports::camera::ZOOM_STEP),
            Action::ResetViewSplits => {
                state.settings.ui.split_x = 0.5;
                state.settings.ui.split_y = 0.5;
                state.settings_dirty = true;
            }
            Action::ViewTop => set_view_kind(state, ViewKind::Top, None),
            Action::ViewSide => set_view_kind(state, ViewKind::Side, None),
            Action::ViewFront => set_view_kind(state, ViewKind::Front, None),
            Action::View3dTextured => set_view_kind(state, ViewKind::Perspective, Some(Action::RenderTextured)),
            Action::View3dFlat => set_view_kind(state, ViewKind::Perspective, Some(Action::RenderFlat)),
            Action::CycleView2d => cycle_view_2d(state),
            Action::ViewLogical => set_view_type(state, QuadrantView::Logical),
            Action::ToggleViewInfo => {
                let w = &mut state.settings.workflow;
                w.show_view_info = !w.show_view_info;
                state.settings_dirty = true;
            }
            Action::ToggleBoundsOverlay => {
                let w = &mut state.settings.workflow;
                w.show_bounds = !w.show_bounds;
                state.settings_dirty = true;
            }
            Action::ToggleCameraSmoothing => toggle_camera_smoothing(state),
            Action::ShowHistory => {
                let w = &mut state.settings.workflow;
                w.show_history = !w.show_history;
                if w.show_history {
                    state.ui.right_tab = RightTab::Package(HISTORY_TAB);
                }
                state.settings_dirty = true;
            }
            Action::ShowToolbar => {
                let w = &mut state.settings.workflow;
                w.show_toolbar = !w.show_toolbar;
                state.settings_dirty = true;
            }
            Action::ShowToolPalette => {
                let w = &mut state.settings.workflow;
                w.show_tool_palette = !w.show_tool_palette;
                state.settings_dirty = true;
            }
            Action::ShowStatusBar => {
                let w = &mut state.settings.workflow;
                w.show_status_bar = !w.show_status_bar;
                state.settings_dirty = true;
            }
            Action::ToolPaletteTwoColumns | Action::ToolbarTwoRows | Action::ResetToolbar => {
                toolbar_layout_action(state, action);
            }
            Action::SaveLayout => {
                state.runtime.workflow.save_layout_open = true;
                let pkg = &mut app.pkg.workflow;
                pkg.layout_name = next_layout_name(&app.state.settings.workflow.layouts);
                pkg.focus_name = true;
            }
            Action::ResetLayout => reset_layout(state),
            _ => return false,
        }
        true
    }

    fn init(&self, app: &mut HammerProApp) {
        let s = &app.state.settings;
        app.state.keys = KeyBindings::from_preset(s.workflow.keymap_preset, &s.keybindings);
        app.state.runtime.workflow.build_info = Some(build_info());
        app.shell
            .add_menu_extension(MenuExtension::new(Category::Window, 10, 1, layouts_menu).in_submenu("menu.layout"));
        app.shell.add_menu_extension(MenuExtension::new(Category::Window, 30, 25, open_maps_menu));
    }

    fn tick(&self, app: &mut HammerProApp, ctx: &egui::Context) {
        sync_grid_memory(app);
        route_dropped_files(app, ctx);
        poll_pointfile(app);
        let rt = &mut app.state.runtime.workflow;
        if let Some(name) = rt.apply_layout.take()
            && let Some(layout) = app.state.settings.workflow.layouts.get(&name).cloned()
        {
            apply_layout(&mut app.state, &layout);
        }
        if let Some(name) = app.state.runtime.workflow.delete_layout.take()
            && app.state.settings.workflow.layouts.remove(&name).is_some()
        {
            app.state.settings_dirty = true;
        }
    }

    fn map_loaded(&self, app: &mut HammerProApp) {
        let state = &mut app.state;
        if state.doc.path().is_some() || state.doc.opened_from().is_some() {
            restore_map_grid(state);
        }
        app.pkg.workflow.synced_grid = Some(app.state.settings.edit.grid_size);
    }

    fn dialogs(&self, app: &mut HammerProApp, ctx: &egui::Context) {
        if app.state.runtime.workflow.save_layout_open {
            save_layout_window(app, ctx);
        }
    }

    fn dock_tabs(&self, app: &HammerProApp, tabs: &mut Vec<DockTab>) {
        if app.state.settings.workflow.show_history {
            tabs.push(DockTab { id: HISTORY_TAB, label: t!("workflow.history").to_owned() });
        }
    }

    fn dock_ui(&self, app: &mut HammerProApp, ui: &mut egui::Ui, id: &'static str) -> bool {
        if id != HISTORY_TAB {
            return false;
        }
        history_ui(ui, &mut app.state);
        true
    }

    fn translations(&self) -> TrTable {
        TR
    }
}

fn toggle_ignore_groups(state: &mut AppState) {
    let mode = &mut state.settings.edit.select_mode;
    if *mode == SelectModeSetting::Solids {
        *mode = state.runtime.workflow.previous_select_mode.take().unwrap_or(SelectModeSetting::Groups);
    } else {
        state.runtime.workflow.previous_select_mode = Some(*mode);
        *mode = SelectModeSetting::Solids;
    }
    state.settings_dirty = true;
}

fn set_view_type(state: &mut AppState, view: QuadrantView) {
    let i = WorkflowRuntime::target_view(state);
    if let Some(q) = state.settings.ui.quadrants.get_mut(i) {
        *q = view.index();
        state.settings_dirty = true;
    }
}

fn set_view_kind(state: &mut AppState, kind: ViewKind, render: Option<Action>) {
    set_view_type(state, QuadrantView::View(kind));
    if let Some(action) = render {
        let wanted = if action == Action::RenderFlat { RenderModeSetting::Flat } else { RenderModeSetting::Textured };
        if state.settings.view.render_mode != wanted {
            state.request(action);
        }
    }
}

fn cycle_view_2d(state: &mut AppState) {
    let next = match WorkflowRuntime::target_view_type(state).kind() {
        Some(ViewKind::Top) => ViewKind::Front,
        Some(ViewKind::Front) => ViewKind::Side,
        Some(ViewKind::Side) => ViewKind::Top,
        Some(ViewKind::Perspective) | None => return,
    };
    set_view_kind(state, next, None);
}

fn restore_map_grid(state: &mut AppState) {
    let view = state.doc.scene().view_settings();
    match view.get("bSnapToGrid").map(str::trim) {
        Some("0") => state.settings.edit.snap_to_grid = false,
        Some("1") => state.settings.edit.snap_to_grid = true,
        _ => {}
    }
    let saved = view.get("nGridSpacing").and_then(|v| v.trim().parse::<f64>().ok());
    let memory: WorkflowMapMemory = state.sidecar.get();
    if let Some(grid) = memory.grid.filter(|g| g.is_finite() && *g > 0.0) {
        let grid = clamp_grid(grid, state.settings.edit.grid_size);
        let written = grid.round().clamp(1.0, 1024.0);
        if saved.is_none_or(|s| (s - written).abs() < 1e-9) {
            state.settings.edit.grid_size = grid;
        }
    }
}

fn toggle_camera_smoothing(state: &mut AppState) {
    let w = &mut state.settings.workflow;
    w.camera_smoothing = !w.camera_smoothing;
    state.settings_dirty = true;
}

fn sync_grid_memory(app: &mut HammerProApp) {
    let grid = app.state.settings.edit.grid_size;
    if app.pkg.workflow.synced_grid == Some(grid) || !grid.is_finite() {
        return;
    }
    app.pkg.workflow.synced_grid = Some(grid);
    app.state.sidecar.set(&WorkflowMapMemory { grid: Some(grid) });
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
enum DropKind {
    Map,
    Pointfile,
    Portals,
    Pack,
    Image,
    Model,
    Unknown,
}

fn drop_kind(path: &Path) -> DropKind {
    let ext = path.extension().and_then(|e| e.to_str()).unwrap_or_default().to_ascii_lowercase();
    match ext.as_str() {
        "vmf" | "vmx" | "vmm" => DropKind::Map,
        "lin" | "pts" => DropKind::Pointfile,
        "prt" => DropKind::Portals,
        "bsp" | "gma" | "vpk" => DropKind::Pack,
        "png" | "jpg" | "jpeg" | "tga" | "bmp" => DropKind::Image,
        "fbx" | "obj" | "gltf" | "glb" => DropKind::Model,
        _ => DropKind::Unknown,
    }
}

const MAX_DROP: usize = 256;

fn route_dropped_files(app: &mut HammerProApp, ctx: &egui::Context) {
    let dropped: Vec<PathBuf> = ctx.input(|i| {
        i.raw
            .dropped_files
            .iter()
            .take(MAX_DROP)
            .map(|f| f.path().to_path_buf())
            .filter(|p| !p.as_os_str().is_empty())
            .collect()
    });
    if dropped.is_empty() {
        return;
    }
    let mut images = Vec::new();
    let mut models = Vec::new();
    for path in dropped {
        let name = path.file_name().map(|n| n.to_string_lossy().into_owned()).unwrap_or_default();
        match drop_kind(&path) {
            DropKind::Map => {}
            DropKind::Pointfile => start_pointfile_read(app, ctx, path),
            DropKind::Portals if Action::LoadPortalFile.is_ready() => app.state.runtime.workflow.push_dropped(path),
            DropKind::Pack if Action::OpenArchive.is_ready() => app.state.runtime.workflow.push_dropped(path),
            DropKind::Portals | DropKind::Pack => {
                app.state.notify(Severity::Info, tf!("workflow.drop_not_yet", name));
            }
            DropKind::Image => images.push(path),
            DropKind::Model => models.push(path),
            DropKind::Unknown => app.state.notify(Severity::Info, tf!("workflow.drop_unsupported", name)),
        }
    }
    match models.len() {
        0 => {}
        1 if app.pkg.lab.model_import.busy() => app.state.notify(Severity::Info, t!("workflow.drop_model_busy")),
        1 => app.pkg.lab.model_import.open_file(ctx, models.remove(0)),
        _ => app.pkg.blender.add_batch_inputs(models),
    }
    if !images.is_empty() {
        let import = app.texture_dialogs.import_dialog();
        if import.is_running() {
            app.state.notify(Severity::Info, t!("workflow.drop_import_busy"));
        } else {
            import.open(images);
        }
    }
}

fn start_pointfile_read(app: &mut HammerProApp, ctx: &egui::Context, path: PathBuf) {
    let (tx, rx) = crossbeam_channel::bounded(1);
    let repaint = ctx.clone();
    let spawned = std::thread::Builder::new().name("hp-pointfile-drop".to_owned()).spawn(move || {
        let result = hp_compile::read_pointfile(&path).map_err(|e| e.to_string());
        let _ = tx.send((path, result));
        repaint.request_repaint();
    });
    match spawned {
        Ok(_) => app.pkg.workflow.pointfile = Some(rx),
        Err(e) => app.state.notify(Severity::Error, tf!("app.pointfile_failed", e)),
    }
}

fn poll_pointfile(app: &mut HammerProApp) {
    let Some(rx) = app.pkg.workflow.pointfile.as_ref() else { return };
    let (path, result) = match rx.try_recv() {
        Ok(read) => read,
        Err(TryRecvError::Empty) => return,
        Err(TryRecvError::Disconnected) => {
            app.pkg.workflow.pointfile = None;
            return;
        }
    };
    app.pkg.workflow.pointfile = None;
    match result {
        Ok(points) => {
            let points: Vec<DVec3> = points.into_iter().map(DVec3::from_array).collect();
            app.state.notify(Severity::Success, tf!("app.pointfile_loaded", points.len()));
            app.state.pointfile = Some(Pointfile { path, points, cursor: 0 });
            hp_ui_mapping::go_to_leak_start(&mut app.state);
        }
        Err(e) => app.state.notify(Severity::Error, tf!("app.pointfile_failed", e)),
    }
}

fn current_layout(state: &AppState) -> SavedLayout {
    let ui = &state.settings.ui;
    SavedLayout {
        quadrants: ui.quadrants,
        split_x: ui.split_x,
        split_y: ui.split_y,
        maximized: state.ui.maximized_view,
        panels: [
            ui.show_texture_browser,
            ui.show_entity_properties,
            ui.show_visgroups,
            ui.show_outliner,
            ui.show_model_browser,
            ui.show_console,
        ],
    }
}

pub(crate) fn apply_layout(state: &mut AppState, layout: &SavedLayout) {
    let ui = &mut state.settings.ui;
    ui.quadrants = layout.quadrants.map(|q| if q > QuadrantView::LOGICAL_INDEX { 3 } else { q });
    ui.split_x = hp_ui_viewports::layout::clamp_split(layout.split_x);
    ui.split_y = hp_ui_viewports::layout::clamp_split(layout.split_y);
    let [textures, entity, visgroups, outliner, models, console] = layout.panels;
    ui.show_texture_browser = textures;
    ui.show_entity_properties = entity;
    ui.show_visgroups = visgroups;
    ui.show_outliner = outliner;
    ui.show_model_browser = models;
    ui.show_console = console;
    state.ui.maximized_view = layout.maximized.filter(|&i| i < 4);
    state.settings_dirty = true;
}

fn reset_layout(state: &mut AppState) {
    apply_layout(state, &SavedLayout::default());
    let defaults = hp_app::settings::UiSettings::default();
    state.settings.ui.dock_width = defaults.dock_width;
    state.settings.ui.console_height = defaults.console_height;
}

fn toolbar_layout_action(state: &mut AppState, action: Action) {
    let w = &mut state.settings.workflow;
    match action {
        Action::ToolPaletteTwoColumns => {
            w.tool_palette_columns = if w.tool_palette_two_columns() { 1 } else { 2 };
            w.show_tool_palette = true;
        }
        Action::ToolbarTwoRows => {
            w.toolbar_rows = if w.toolbar_two_rows() { 1 } else { 2 };
            w.show_toolbar = true;
        }
        Action::ResetToolbar => {
            w.reset_toolbar();
            w.show_toolbar = true;
        }
        _ => return,
    }
    state.settings_dirty = true;
}

#[cfg(test)]
fn default_layout() -> SavedLayout {
    let d = hp_app::settings::UiSettings::default();
    SavedLayout {
        panels: [
            d.show_texture_browser,
            d.show_entity_properties,
            d.show_visgroups,
            d.show_outliner,
            d.show_model_browser,
            d.show_console,
        ],
        ..SavedLayout::default()
    }
}

fn next_layout_name(layouts: &std::collections::BTreeMap<String, SavedLayout>) -> String {
    (1..=layouts.len() + 1)
        .map(|i| tf!("workflow.layout_default_name", i))
        .find(|n| !layouts.contains_key(n))
        .unwrap_or_default()
}

fn clean_layout_name(text: &str) -> Option<String> {
    let name: String =
        text.chars().filter(|c| !c.is_control()).collect::<String>().trim().chars().take(MAX_LAYOUT_NAME).collect();
    let name = name.trim().to_owned();
    (!name.is_empty()).then_some(name)
}

fn save_layout(state: &mut AppState, name: String) -> bool {
    let layouts = &mut state.settings.workflow.layouts;
    if !layouts.contains_key(&name) && layouts.len() >= MAX_LAYOUTS {
        return false;
    }
    let layout = current_layout(state);
    state.settings.workflow.layouts.insert(name, layout);
    state.settings_dirty = true;
    true
}

fn open_maps_menu(ui: &mut egui::Ui, state: &mut AppState) {
    submenu(ui, t!("workflow.open_maps"), |ui| {
        ui.set_min_width(220.0);
        let active = state.runtime.workflow.active_tab;
        let mut chosen = None;
        for tab in &state.runtime.workflow.tabs {
            let label = if tab.dirty { format!("{}*", tab.title) } else { tab.title.clone() };
            if menu_entry(ui, &label, None, true, Check::Radio(tab.id == active)).clicked() {
                chosen = Some(tab.id);
            }
        }
        if let Some(id) = chosen {
            state.runtime.workflow.tab_request = Some(hp_app::runtime::TabRequest::Switch(id));
        }
    });
}

fn layouts_menu(ui: &mut egui::Ui, state: &mut AppState) {
    submenu(ui, t!("workflow.saved_layouts"), |ui| {
        ui.set_min_width(200.0);
        let names: Vec<String> = state.settings.workflow.layouts.keys().cloned().collect();
        if names.is_empty() {
            menu_entry(ui, t!("workflow.no_layouts"), None, false, Check::None);
            return;
        }
        for name in &names {
            if menu_entry(ui, name, None, true, Check::None).clicked() {
                state.runtime.workflow.apply_layout = Some(name.clone());
            }
        }
        ui.separator();
        submenu(ui, t!("workflow.delete_layout"), |ui| {
            for name in &names {
                if menu_entry(ui, name, None, true, Check::None).clicked() {
                    state.runtime.workflow.delete_layout = Some(name.clone());
                }
            }
        });
    });
}

fn save_layout_window(app: &mut HammerProApp, ctx: &egui::Context) {
    let mut open = true;
    let mut done = false;
    let pkg = &mut app.pkg.workflow;
    let state = &mut app.state;
    if ctx.input_mut(|i| i.consume_key(egui::Modifiers::NONE, egui::Key::Escape)) {
        state.runtime.workflow.save_layout_open = false;
        return;
    }
    hp_app::chrome::Window::new(t!("action.save_layout"))
        .id(egui::Id::unique("hp_workflow_save_layout"))
        .open(&mut open)
        .collapsible(false)
        .resizable(false)
        .anchor(egui::Align2::CENTER_CENTER, egui::Vec2::ZERO)
        .show(ctx, |ui| {
            ui.label(t!("workflow.layout_name"));
            let field = ui.add(egui::TextEdit::singleline(&mut pkg.layout_name).desired_width(260.0).char_limit(64));
            if std::mem::take(&mut pkg.focus_name) {
                field.request_focus();
            }
            let name = clean_layout_name(&pkg.layout_name);
            if let Some(n) = &name
                && state.settings.workflow.layouts.contains_key(n)
            {
                ui.label(egui::RichText::new(t!("workflow.layout_replaces")).small().color(ui.visuals().warn_fg_color));
            }
            let enter = field.lost_focus() && ui.input(|i| i.key_pressed(egui::Key::Enter));
            ui.horizontal(|ui| {
                let save = ui.add_enabled(name.is_some(), egui::Button::new(t!("common.save"))).clicked();
                if (save || enter)
                    && let Some(n) = name.clone()
                {
                    if save_layout(state, n.clone()) {
                        state.notify(Severity::Success, tf!("workflow.layout_saved", n));
                    } else {
                        state.notify(Severity::Warning, tf!("workflow.layouts_full", MAX_LAYOUTS));
                    }
                    done = true;
                }
                if ui.button(t!("common.cancel")).clicked() {
                    done = true;
                }
            });
        });
    if !open || done {
        state.runtime.workflow.save_layout_open = false;
    }
}

fn history_ui(ui: &mut egui::Ui, state: &mut AppState) {
    let undo: Vec<String> = state.doc.undo_labels().into_iter().map(hp_ui_shell::menu::undo_step_text).collect();
    let redo: Vec<String> = state.doc.redo_labels().into_iter().map(hp_ui_shell::menu::undo_step_text).collect();
    ui.label(egui::RichText::new(t!("workflow.history_hint")).small().color(ui.visuals().weak_text_color()));
    ui.separator();
    let rows = 1 + undo.len() + redo.len();
    let row_height = ui.spacing().interact_size.y;
    let mut target: Option<HistoryTarget> = None;
    egui::ScrollArea::vertical().auto_shrink([false, false]).stick_to_bottom(true).show_rows(
        ui,
        row_height,
        rows,
        |ui, range| {
            for row in range {
                let (text, current, done, goal) = if row == 0 {
                    (t!("workflow.history_start").to_owned(), undo.is_empty(), true, HistoryTarget::Undo(undo.len()))
                } else if row <= undo.len() {
                    let k = row - 1;
                    (undo[k].clone(), k + 1 == undo.len(), true, HistoryTarget::Undo(undo.len() - 1 - k))
                } else {
                    let j = row - 1 - undo.len();
                    (redo[j].clone(), false, false, HistoryTarget::Redo(j + 1))
                };
                let mut rich = egui::RichText::new(text);
                if !done {
                    rich = rich.color(ui.visuals().weak_text_color()).italics();
                }
                if ui
                    .add(egui::Button::selectable(current, rich).min_size(egui::vec2(ui.available_width(), 0.0)))
                    .clicked()
                {
                    target = Some(goal);
                }
            }
        },
    );
    if let Some(target) = target {
        go_to_step(state, target);
    }
}

fn go_to_step(state: &mut AppState, target: HistoryTarget) {
    let (n, undo) = match target {
        HistoryTarget::Undo(n) => (n, true),
        HistoryTarget::Redo(n) => (n, false),
    };
    for _ in 0..n {
        let moved = if undo { state.doc.undo() } else { state.doc.redo() };
        if !moved {
            break;
        }
    }
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
enum HistoryTarget {
    Undo(usize),
    Redo(usize),
}

fn build_info() -> String {
    let date = env!("HP_BUILD_DATE");
    match option_env!("HP_BUILD_COMMIT") {
        Some(commit) => format!("{date}, {commit}"),
        None => date.to_owned(),
    }
}

const TR: TrTable = &[
    ("workflow.history", "History", "Historique"),
    ("workflow.open_maps", "Open maps", "Maps ouvertes"),
    (
        "workflow.history_hint",
        "Click a step to go back or forward to it.",
        "Cliquez sur une étape pour y revenir ou y avancer.",
    ),
    ("workflow.history_start", "Map opened", "Ouverture de la map"),
    ("workflow.saved_layouts", "Saved layouts", "Dispositions enregistrées"),
    ("workflow.no_layouts", "No saved layout", "Aucune disposition enregistrée"),
    ("workflow.delete_layout", "Delete", "Supprimer"),
    ("workflow.layout_name", "Name of the layout:", "Nom de la disposition :"),
    ("workflow.layout_default_name", "Layout {0}", "Disposition {0}"),
    (
        "workflow.layout_replaces",
        "A layout with this name exists: it will be replaced.",
        "Une disposition porte déjà ce nom : elle sera remplacée.",
    ),
    ("workflow.layout_saved", "Layout saved: {0}", "Disposition enregistrée : {0}"),
    (
        "workflow.layouts_full",
        "At most {0} layouts can be saved: delete one first.",
        "Au plus {0} dispositions peuvent être enregistrées : supprimez-en une d'abord.",
    ),
    (
        "workflow.drop_unsupported",
        "{0}: this kind of file can not be opened here.",
        "{0} : ce type de fichier ne peut pas être ouvert ici.",
    ),
    (
        "workflow.drop_not_yet",
        "{0}: opening this kind of file is not available yet.",
        "{0} : l'ouverture de ce type de fichier n'est pas encore disponible.",
    ),
    (
        "workflow.drop_model_busy",
        "A model import is running: drop the model again when it is done.",
        "Une importation de modèle est en cours : déposez de nouveau le modèle quand elle sera terminée.",
    ),
    (
        "workflow.drop_import_busy",
        "An image import is running: drop the images again when it is done.",
        "Une importation d'images est en cours : déposez de nouveau les images quand elle sera terminée.",
    ),
];

#[cfg(test)]
mod tests {
    use super::*;

    fn state() -> AppState {
        hp_app::i18n::register(TR);
        hp_app::testing::test_state()
    }

    #[test]
    fn table_is_clean() {
        assert!(hp_app::i18n::lint_table(TR).is_empty(), "{:#?}", hp_app::i18n::lint_table(TR));
    }

    #[test]
    fn toolbar_and_palette_layout_actions() {
        let mut s = state();
        s.settings.workflow.show_tool_palette = false;
        toolbar_layout_action(&mut s, Action::ToolPaletteTwoColumns);
        assert!(s.settings.workflow.tool_palette_two_columns() && s.settings.workflow.show_tool_palette);
        assert_eq!(Action::ToolPaletteTwoColumns.check(&s), hp_app::actions::Check::Toggle(true));
        toolbar_layout_action(&mut s, Action::ToolPaletteTwoColumns);
        assert_eq!(s.settings.workflow.tool_palette_columns, 1);
        toolbar_layout_action(&mut s, Action::ToolbarTwoRows);
        assert!(s.settings.workflow.toolbar_two_rows());
        s.settings.workflow.toolbar_hidden.push("cordon".into());
        s.settings.workflow.toolbar_order.push("render".into());
        s.settings_dirty = false;
        toolbar_layout_action(&mut s, Action::ResetToolbar);
        let w = &s.settings.workflow;
        assert!(!w.toolbar_two_rows() && w.toolbar_hidden.is_empty() && w.toolbar_order.is_empty());
        assert!(s.settings_dirty);
        for a in [Action::ToolPaletteTwoColumns, Action::ToolbarTwoRows, Action::ResetToolbar] {
            assert!(HANDLED.contains(&a), "{a:?}");
        }
    }

    #[test]
    fn camera_smoothing_toggles_and_keeps_the_acceleration_time() {
        let mut s = state();
        s.settings.workflow.fly_response = 0.3;
        assert!(s.settings.workflow.camera_smoothing, "on by default");
        assert_eq!(Action::ToggleCameraSmoothing.check(&s), hp_app::actions::Check::Toggle(true));
        assert_eq!(Action::from_id("toggle_camera_smoothing"), Some(Action::ToggleCameraSmoothing));
        assert!(HANDLED.contains(&Action::ToggleCameraSmoothing));
        toggle_camera_smoothing(&mut s);
        assert!(!s.settings.workflow.camera_smoothing && s.settings_dirty);
        assert_eq!(Action::ToggleCameraSmoothing.check(&s), hp_app::actions::Check::Toggle(false));
        assert_eq!(s.settings.workflow.fly_response_secs(), 0.0);
        toggle_camera_smoothing(&mut s);
        assert!(s.settings.workflow.camera_smoothing);
        assert!((s.settings.workflow.fly_response_secs() - 0.3).abs() < 1e-6);
    }

    #[test]
    fn ignore_groups_toggles_solids_and_back() {
        let mut s = state();
        s.settings.edit.select_mode = SelectModeSetting::Objects;
        toggle_ignore_groups(&mut s);
        assert_eq!(s.settings.edit.select_mode, SelectModeSetting::Solids);
        toggle_ignore_groups(&mut s);
        assert_eq!(s.settings.edit.select_mode, SelectModeSetting::Objects, "back to the previous mode");
        s.settings.edit.select_mode = SelectModeSetting::Solids;
        s.runtime.workflow.previous_select_mode = None;
        toggle_ignore_groups(&mut s);
        assert_eq!(s.settings.edit.select_mode, SelectModeSetting::Groups);
        assert!(s.settings_dirty);
    }

    #[test]
    fn view_keys_change_the_view_under_the_mouse() {
        let mut s = state();
        s.ui.hovered_view = Some(3);
        set_view_kind(&mut s, ViewKind::Top, None);
        assert_eq!(s.settings.ui.quadrants[3], 1);
        cycle_view_2d(&mut s);
        assert_eq!(ViewKind::from_index(s.settings.ui.quadrants[3]), ViewKind::Front);
        cycle_view_2d(&mut s);
        assert_eq!(ViewKind::from_index(s.settings.ui.quadrants[3]), ViewKind::Side);
        cycle_view_2d(&mut s);
        assert_eq!(ViewKind::from_index(s.settings.ui.quadrants[3]), ViewKind::Top);
        s.ui.hovered_view = None;
        s.ui.focused_view = Some(1);
        set_view_kind(&mut s, ViewKind::Perspective, Some(Action::RenderFlat));
        assert_eq!(s.settings.ui.quadrants[1], 0);
        assert!(s.requested_actions.contains(&Action::RenderFlat));
        let before = s.settings.ui.quadrants;
        cycle_view_2d(&mut s);
        assert_eq!(s.settings.ui.quadrants, before);
        set_view_type(&mut s, QuadrantView::Logical);
        assert_eq!(QuadrantView::from_index(s.settings.ui.quadrants[1]), QuadrantView::Logical);
        let before = s.settings.ui.quadrants;
        cycle_view_2d(&mut s);
        assert_eq!(s.settings.ui.quadrants, before);
        set_view_kind(&mut s, ViewKind::Top, None);
        assert_eq!(QuadrantView::from_index(s.settings.ui.quadrants[1]), QuadrantView::View(ViewKind::Top));
    }

    #[test]
    fn grid_and_snap_follow_the_map() {
        let mut s = state();
        s.doc.set_view_setting("nGridSpacing", "3");
        s.doc.set_view_setting("bSnapToGrid", "0");
        s.sidecar.set(&WorkflowMapMemory { grid: Some(2.5) });
        restore_map_grid(&mut s);
        assert!(!s.settings.edit.snap_to_grid);
        assert_eq!(s.settings.edit.grid_size, 2.5, "2.5 is written as 3 for Hammer");
        s.settings.edit.grid_size = 16.0;
        s.doc.set_view_setting("nGridSpacing", "32");
        restore_map_grid(&mut s);
        assert_eq!(s.settings.edit.grid_size, 16.0);
        s.sidecar.set(&WorkflowMapMemory { grid: Some(f64::NAN) });
        restore_map_grid(&mut s);
        assert_eq!(s.settings.edit.grid_size, 16.0);
    }

    #[test]
    fn reset_layout_restores_the_panel_sizes() {
        let mut s = state();
        s.settings.ui.dock_width = 231.0;
        s.settings.ui.console_height = 400.0;
        s.settings.ui.split_x = 0.3;
        reset_layout(&mut s);
        let defaults = hp_app::settings::UiSettings::default();
        assert_eq!(s.settings.ui.dock_width, defaults.dock_width);
        assert_eq!(s.settings.ui.console_height, defaults.console_height);
        assert_eq!(s.settings.ui.split_x, 0.5);
        assert!(s.settings_dirty);
    }

    #[test]
    fn layouts_save_apply_and_stay_bounded() {
        let mut s = state();
        s.settings.ui.split_x = 0.3;
        s.settings.ui.quadrants = [1, 0, 2, 3];
        s.settings.ui.show_console = false;
        assert!(save_layout(&mut s, "Wide".into()));
        s.settings.ui.split_x = 0.8;
        s.settings.ui.quadrants = [0, 1, 2, 3];
        let saved = s.settings.workflow.layouts["Wide"].clone();
        apply_layout(&mut s, &saved);
        assert_eq!(s.settings.ui.split_x, 0.3);
        assert_eq!(s.settings.ui.quadrants, [1, 0, 2, 3]);
        assert!(!s.settings.ui.show_console);
        apply_layout(&mut s, &SavedLayout { quadrants: [9, 9, 9, 9], split_x: f32::NAN, maximized: Some(7), ..saved });
        assert_eq!(s.settings.ui.quadrants, [3, 3, 3, 3]);
        let logical = QuadrantView::LOGICAL_INDEX;
        apply_layout(&mut s, &SavedLayout { quadrants: [0, logical, 2, 3], ..default_layout() });
        assert_eq!(s.settings.ui.quadrants, [0, logical, 2, 3]);
        assert_eq!(s.settings.ui.split_x, 0.5);
        assert_eq!(s.ui.maximized_view, None);
        apply_layout(&mut s, &default_layout());
        assert_eq!(s.settings.ui.quadrants, [0, 1, 2, 3]);
        for i in 0..MAX_LAYOUTS + 3 {
            let _ = save_layout(&mut s, format!("L{i}"));
        }
        assert_eq!(s.settings.workflow.layouts.len(), MAX_LAYOUTS);
        assert!(save_layout(&mut s, "Wide".into()), "replacing an existing layout always works");
        assert_eq!(clean_layout_name("  My\tlayout \n"), Some("Mylayout".into()));
        assert_eq!(clean_layout_name("   "), None);
        assert_eq!(clean_layout_name(&"x".repeat(200)).map(|n| n.chars().count()), Some(MAX_LAYOUT_NAME));
        let names = std::collections::BTreeMap::from([("Layout 1".to_owned(), SavedLayout::default())]);
        assert_eq!(next_layout_name(&names), "Layout 2");
    }

    #[test]
    fn dropped_file_kinds() {
        assert_eq!(drop_kind(Path::new("C:/maps/a.VMF")), DropKind::Map);
        assert_eq!(drop_kind(Path::new("a.lin")), DropKind::Pointfile);
        assert_eq!(drop_kind(Path::new("a.pts")), DropKind::Pointfile);
        assert_eq!(drop_kind(Path::new("a.prt")), DropKind::Portals);
        assert_eq!(drop_kind(Path::new("C:/models/test.FBX")), DropKind::Model);
        assert_eq!(drop_kind(Path::new("rock.obj")), DropKind::Model);
        assert_eq!(drop_kind(Path::new("tree.glb")), DropKind::Model);
        assert_eq!(drop_kind(Path::new("a.bsp")), DropKind::Pack);
        assert_eq!(drop_kind(Path::new("a.gma")), DropKind::Pack);
        assert_eq!(drop_kind(Path::new("brick.PNG")), DropKind::Image);
        assert_eq!(drop_kind(Path::new("notes.txt")), DropKind::Unknown);
        assert_eq!(drop_kind(Path::new("no_extension")), DropKind::Unknown);
    }

    #[test]
    fn history_panel_moves_through_the_steps() {
        let mut s = state();
        hp_ui_shell::register_translations();
        s.doc.select_all();
        let objs = s.doc.selected_objects();
        s.doc.translate(&objs, DVec3::X).expect("move");
        s.doc.translate(&objs, DVec3::Y).expect("move");
        assert_eq!(s.doc.history_len(), (2, 0));
        hp_app::testing::run_ui(&mut s, history_ui);
        assert_eq!(s.doc.history_len(), (2, 0), "drawing changes nothing");
        go_to_step(&mut s, HistoryTarget::Undo(1));
        assert_eq!(s.doc.history_len(), (1, 1));
        go_to_step(&mut s, HistoryTarget::Redo(1));
        assert_eq!(s.doc.history_len(), (2, 0));
        go_to_step(&mut s, HistoryTarget::Undo(2));
        assert_eq!(s.doc.history_len(), (0, 2));
        go_to_step(&mut s, HistoryTarget::Redo(9));
        assert_eq!(s.doc.history_len(), (2, 0));
        hp_app::testing::run_ui(&mut s, history_ui);
        assert!(build_info().starts_with("20"), "{}", build_info());
    }
}
