use egui::{RichText, Ui};
use hp_app::grid::{ActiveGrid, GridSource};
use hp_app::i18n::TrTable;
use hp_app::settings::{GRID_COORD_LIMIT, GRID_MAX, GRID_MIN, GridMode, GridSpec, MAX_MAJOR_EVERY};
use hp_app::{Action, AppState, Severity, ViewKind, t, tf};

use crate::common::{EscapeGuard, action_check, action_enabled, fmt_grid, window_layer};
use crate::menu::menu_entry;

const MODE_ACTIONS: [Action; 4] =
    [Action::GridModeGlobal, Action::GridModePerView, Action::GridModeCustom, Action::GridModeSelection];
const COMMAND_ACTIONS: [Action; 7] = [
    Action::GridFromSelection,
    Action::GridFromFace,
    Action::AttachGrid,
    Action::DetachGrid,
    Action::GridManager,
    Action::RotateGrid45,
    Action::AlignGridToSelection,
];

pub fn view_name(kind: ViewKind) -> &'static str {
    match kind {
        ViewKind::Perspective => t!("view.3d"),
        ViewKind::Top => t!("shell.grid.view_top"),
        ViewKind::Front => t!("shell.grid.view_front"),
        ViewKind::Side => t!("shell.grid.view_side"),
    }
}

pub fn steps_text(spec_steps: [f64; 3]) -> String {
    let [x, y, z] = spec_steps;
    if (x - y).abs() < 1e-9 && (x - z).abs() < 1e-9 {
        fmt_grid(x)
    } else {
        format!("{} × {} × {}", fmt_grid(x), fmt_grid(y), fmt_grid(z))
    }
}

pub fn source_label(target: &ActiveGrid) -> String {
    match &target.source {
        GridSource::Global => t!("grid.label").to_owned(),
        GridSource::View(kind) => tf!("shell.grid.view_label", view_name(*kind)),
        GridSource::Map(name) | GridSource::Preset(name) => short_name(name),
    }
}

const TOOLBAR_NAME_CHARS: usize = 14;

fn short_name(name: &str) -> String {
    if name.chars().count() <= TOOLBAR_NAME_CHARS {
        name.to_owned()
    } else {
        let cut: String = name.chars().take(TOOLBAR_NAME_CHARS - 2).collect();
        format!("{}...", cut.trim_end())
    }
}

pub fn source_tip(state: &AppState, target: &ActiveGrid) -> String {
    let f = &target.frame;
    let what = match &target.source {
        GridSource::Global => t!("shell.grid.tip_global").to_owned(),
        GridSource::View(kind) => tf!("shell.grid.tip_view", view_name(*kind)),
        GridSource::Map(name) => tf!("shell.grid.tip_map", name),
        GridSource::Preset(name) => tf!("shell.grid.tip_preset", name),
    };
    let mut tip = format!("{what}\n{}", tf!("shell.grid.tip_steps", steps_text(f.step.to_array())));
    if state.settings.grid.mode == GridMode::Selection && target.source == GridSource::Global {
        tip.push('\n');
        tip.push_str(t!("shell.grid.tip_no_selection_grid"));
    }
    tip.push('\n');
    tip.push_str(t!("shell.grid.tip_click"));
    tip
}

pub fn grid_menu(ui: &mut Ui, state: &mut AppState) {
    ui.set_min_width(240.0);
    for action in MODE_ACTIONS {
        action_entry(ui, state, action);
    }
    let grids = state.all_custom_grids();
    if !grids.is_empty() {
        ui.separator();
        ui.label(RichText::new(t!("shell.grid.custom_grids")).weak().small());
        let mode = state.settings.grid.mode;
        for (spec, source) in grids {
            let active = mode == GridMode::Custom && state.settings.grid.custom.eq_ignore_ascii_case(&spec.name);
            let label = match source {
                GridSource::Preset(_) => tf!("shell.grid.preset_entry", &spec.name, steps_text(spec.step)),
                _ => tf!("shell.grid.map_entry", &spec.name, steps_text(spec.step)),
            };
            if menu_entry(ui, &label, None, true, hp_app::actions::Check::Radio(active)).clicked()
                && let Err(e) = state.use_custom_grid(&spec.name)
            {
                state.notify(Severity::Warning, e);
            }
        }
    }
    ui.separator();
    crate::grid_rotate::rotate_row(ui, state);
    for action in COMMAND_ACTIONS {
        action_entry(ui, state, action);
    }
    ui.separator();
    let mut on = state.settings.grid.object_grids;
    if ui.checkbox(&mut on, t!("shell.grid.object_grids")).on_hover_text(t!("shell.grid.object_grids_tip")).changed() {
        state.settings.grid.object_grids = on;
        state.settings_dirty = true;
    }
}

fn action_entry(ui: &mut Ui, state: &mut AppState, action: Action) {
    let enabled = action_enabled(state, action);
    let check = action_check(state, action);
    if menu_entry(ui, action.label(), state.keys.shortcut(action), enabled, check).clicked() {
        state.request(action);
        ui.close();
    }
}

pub fn run_grid_action(state: &mut AppState, action: Action) -> bool {
    match action {
        Action::GridModeGlobal => state.set_grid_mode(GridMode::Global),
        Action::GridModePerView => state.set_grid_mode(GridMode::PerView),
        Action::GridModeSelection => state.set_grid_mode(GridMode::Selection),
        Action::GridModeCustom => {
            let name = state.settings.grid.custom.clone();
            let first = state.all_custom_grids().into_iter().next().map(|(g, _)| g.name);
            match state.find_grid(&name).map(|(g, _)| g.name).or(first) {
                Some(n) => {
                    if let Err(e) = state.use_custom_grid(&n) {
                        state.notify(Severity::Warning, e);
                    }
                }
                None => state.notify(Severity::Info, t!("shell.grid.no_custom")),
            }
        }
        Action::GridFromSelection | Action::GridFromFace => {
            let face = action == Action::GridFromFace;
            let name =
                state.unique_grid_name(t!(if face { "shell.grid.face_name" } else { "shell.grid.selection_name" }));
            let spec = if face { state.grid_from_face(&name) } else { state.grid_from_selection(&name) };
            match spec {
                Some(spec) => match state.put_custom_grid("", spec, true, false) {
                    Ok(()) => {
                        let _ = state.use_custom_grid(&name);
                        state.notify(Severity::Info, tf!("shell.grid.created", name));
                    }
                    Err(e) => state.notify(Severity::Warning, e),
                },
                None => state.notify(
                    Severity::Info,
                    t!(if face { "shell.grid.need_face" } else { "shell.grid.need_selection" }),
                ),
            }
        }
        Action::AttachGrid => attach_active_grid(state),
        Action::DetachGrid => {
            let objs = state.doc.selected_objects();
            match state.doc.set_object_grid(&objs, &[], None) {
                Ok(n) => state.notify(Severity::Info, tf!("shell.grid.detached", n)),
                Err(e) => state.notify(Severity::Warning, e.to_string()),
            }
        }
        Action::GridManager => state.ui.show_grid_manager = !state.ui.show_grid_manager,
        Action::RotateGrid45 | Action::AlignGridToSelection => crate::grid_rotate::run(state, action),
        _ => return false,
    }
    true
}

fn attach_active_grid(state: &mut AppState) {
    let name = state.settings.grid.custom.clone();
    let Some((spec, source)) = state.find_grid(&name) else {
        state.notify(Severity::Info, t!("shell.grid.no_custom"));
        return;
    };
    let objs = state.doc.selected_objects();
    if objs.is_empty() {
        state.notify(Severity::Info, t!("shell.grid.need_selection"));
        return;
    }
    state.doc.begin("Attach grid");
    let copied = match source {
        GridSource::Preset(_) => state.put_custom_grid("", spec.clone(), true, false),
        _ => Ok(()),
    };
    let result =
        copied.and_then(|()| state.doc.set_object_grid(&objs, &[], Some(&spec.name)).map_err(|e| e.to_string()));
    match result {
        Ok(n) => {
            state.doc.commit();
            state.notify(Severity::Info, tf!("shell.grid.attached", &spec.name, n));
        }
        Err(e) => {
            state.doc.cancel();
            state.notify(Severity::Warning, e);
        }
    }
}

#[derive(Debug, Default)]
pub struct GridManager {
    escape: EscapeGuard,
    selected: Option<(String, bool)>,
    name_edit: Option<String>,
    error: Option<String>,
}

impl GridManager {
    pub fn show(&mut self, ctx: &egui::Context, state: &mut AppState) {
        if !state.ui.show_grid_manager {
            self.escape.reset();
            return;
        }
        let id = egui::Id::unique("hp_grid_manager");
        if self.escape.should_close(ctx, window_layer(id)) {
            state.ui.show_grid_manager = false;
            return;
        }
        let mut open = true;
        hp_app::chrome::Window::new(t!("shell.grid.manager_title")).id(id).open(&mut open).auto_sized().show(
            ctx,
            |ui| {
                ui.set_width(460.0);
                let tall = (ui.ctx().content_rect().height() - 140.0).max(240.0);
                egui::ScrollArea::vertical().max_height(tall).show(ui, |ui| self.ui(ui, state));
            },
        );
        self.escape.finish(ctx, window_layer(id));
        if !open {
            state.ui.show_grid_manager = false;
        }
    }

    fn ui(&mut self, ui: &mut Ui, state: &mut AppState) {
        ui.horizontal_wrapped(|ui| {
            ui.label(t!("shell.grid.mode"));
            for (mode, action) in GridMode::ALL.into_iter().zip(MODE_ACTIONS) {
                let enabled = action_enabled(state, action);
                let on = state.settings.grid.mode == mode;
                if ui.add_enabled(enabled, egui::Button::selectable(on, action.label())).clicked() {
                    run_grid_action(state, action);
                }
            }
        });
        crate::grid_rotate::rotate_row(ui, state);
        let mut object_grids = state.settings.grid.object_grids;
        if ui
            .checkbox(&mut object_grids, t!("shell.grid.object_grids"))
            .on_hover_text(t!("shell.grid.object_grids_tip"))
            .changed()
        {
            state.settings.grid.object_grids = object_grids;
            state.settings_dirty = true;
        }
        let mut plane = state.settings.grid.plane_3d;
        if ui.checkbox(&mut plane, t!("shell.grid.plane_3d")).changed() {
            state.settings.grid.plane_3d = plane;
            state.settings_dirty = true;
        }
        hp_app::widgets::section(ui, t!("shell.grid.views_title"));
        ui.weak(t!("shell.grid.views_help"));
        self.views_ui(ui, state);
        hp_app::widgets::section(ui, t!("shell.grid.custom_grids"));
        self.custom_ui(ui, state);
    }

    fn views_ui(&mut self, ui: &mut Ui, state: &mut AppState) {
        egui::Grid::new("hp_grid_views").num_columns(4).spacing([8.0, 4.0]).show(ui, |ui| {
            ui.weak(t!("shell.grid.col_view"));
            ui.weak(t!("shell.grid.col_size"));
            ui.weak(t!("shell.grid.col_origin"));
            ui.weak(t!("shell.grid.col_major"));
            ui.end_row();
            for kind in [ViewKind::Top, ViewKind::Front, ViewKind::Side, ViewKind::Perspective] {
                let mut g = state.settings.grid.per_view.get(kind).clone();
                let before = g.clone();
                ui.label(view_name(kind));
                ui.add(egui::DragValue::new(&mut g.size).range(GRID_MIN..=GRID_MAX).speed(0.25).max_decimals(4));
                ui.horizontal(|ui| {
                    for v in &mut g.origin {
                        ui.add(coord_value(v));
                    }
                });
                ui.add(egui::DragValue::new(&mut g.major_every).range(0..=MAX_MAJOR_EVERY));
                ui.end_row();
                if g != before {
                    *state.settings.grid.per_view.get_mut(kind) = g.sanitized();
                    state.settings_dirty = true;
                }
            }
        });
    }

    fn custom_ui(&mut self, ui: &mut Ui, state: &mut AppState) {
        let grids: Vec<(GridSpec, GridSource)> = state
            .map_grids()
            .into_iter()
            .map(|g| {
                let n = g.name.clone();
                (g, GridSource::Map(n))
            })
            .chain(state.settings.grid.presets.iter().map(|g| (g.clone(), GridSource::Preset(g.name.clone()))))
            .collect();
        ui.horizontal_wrapped(|ui| {
            if ui.button(t!("shell.grid.new")).clicked() {
                let name = state.unique_grid_name(t!("shell.grid.new_name"));
                let spec = GridSpec::uniform(&name, state.settings.edit.grid_size);
                match state.put_custom_grid("", spec, true, false) {
                    Ok(()) => self.select(name, true),
                    Err(e) => self.error = Some(e),
                }
            }
            for action in [Action::GridFromSelection, Action::GridFromFace] {
                let enabled = action_enabled(state, action);
                if ui.add_enabled(enabled, egui::Button::new(action.label())).clicked() {
                    run_grid_action(state, action);
                    let name = state.settings.grid.custom.clone();
                    self.select(name, true);
                }
            }
        });
        if grids.is_empty() {
            ui.weak(t!("shell.grid.no_custom"));
            return;
        }
        ui.horizontal_wrapped(|ui| {
            for (spec, source) in &grids {
                let in_map = matches!(source, GridSource::Map(_));
                let on =
                    self.selected.as_ref().is_some_and(|(n, m)| n.eq_ignore_ascii_case(&spec.name) && *m == in_map);
                let text = if in_map { spec.name.clone() } else { tf!("shell.grid.preset_tag", &spec.name) };
                if ui.add(egui::Button::selectable(on, text)).clicked() {
                    self.select(spec.name.clone(), in_map);
                }
            }
        });
        let Some((name, in_map)) = self.selected.clone() else { return };
        let Some(spec) = grids
            .iter()
            .find(|(g, s)| g.name.eq_ignore_ascii_case(&name) && matches!(s, GridSource::Map(_)) == in_map)
            .map(|(g, _)| g.clone())
        else {
            self.selected = None;
            return;
        };
        ui.separator();
        self.editor_ui(ui, state, spec, in_map);
        if let Some(e) = &self.error {
            ui.colored_label(crate::common::severity_color(ui.visuals(), Severity::Warning), e);
        }
    }

    fn select(&mut self, name: String, in_map: bool) {
        self.selected = Some((name, in_map));
        self.name_edit = None;
        self.error = None;
    }

    fn editor_ui(&mut self, ui: &mut Ui, state: &mut AppState, spec: GridSpec, in_map: bool) {
        let mut edit = spec.clone();
        ui.horizontal(|ui| {
            ui.label(t!("shell.grid.name"));
            let text = self.name_edit.get_or_insert_with(|| spec.name.clone());
            let r = ui.add(egui::TextEdit::singleline(text).desired_width(180.0));
            if r.lost_focus() && !ui.input(|i| i.key_pressed(egui::Key::Escape)) {
                edit.name = text.trim().to_owned();
            }
            ui.weak(t!(if in_map { "shell.grid.in_map" } else { "shell.grid.in_settings" }));
        });
        egui::Grid::new("hp_grid_edit").num_columns(2).spacing([8.0, 4.0]).show(ui, |ui| {
            ui.label(t!("shell.grid.steps")).on_hover_text(t!("shell.grid.steps_tip"));
            ui.horizontal(|ui| {
                let uniform = edit.is_uniform();
                for (i, axis) in ["X", "Y", "Z"].iter().enumerate() {
                    ui.weak(*axis);
                    let before = edit.step[i];
                    ui.add(
                        egui::DragValue::new(&mut edit.step[i]).range(GRID_MIN..=GRID_MAX).speed(0.25).max_decimals(4),
                    );
                    if uniform && edit.step[i] != before && ui.input(|inp| !inp.modifiers.shift) {
                        edit.step = [edit.step[i]; 3];
                    }
                }
            });
            ui.end_row();
            ui.label(t!("shell.grid.origin"));
            ui.horizontal(|ui| {
                for v in &mut edit.origin {
                    ui.add(coord_value(v));
                }
            });
            ui.end_row();
            ui.label(t!("shell.grid.angles")).on_hover_text(t!("shell.grid.angles_tip"));
            ui.horizontal(|ui| {
                for v in &mut edit.angles {
                    ui.add(egui::DragValue::new(v).range(-360.0..=360.0).speed(0.5).max_decimals(3).suffix("°"));
                }
            });
            ui.end_row();
            ui.label(t!("shell.grid.major"));
            ui.add(egui::DragValue::new(&mut edit.major_every).range(0..=MAX_MAJOR_EVERY))
                .on_hover_text(t!("shell.grid.major_tip"));
            ui.end_row();
            let mut limited = edit.bounds.is_some();
            ui.checkbox(&mut limited, t!("shell.grid.limits")).on_hover_text(t!("shell.grid.limits_tip"));
            ui.vertical(|ui| {
                if limited && edit.bounds.is_none() {
                    let o = edit.origin;
                    edit.bounds = Some([o, [o[0] + 512.0, o[1] + 512.0, o[2] + 512.0]]);
                }
                if !limited {
                    edit.bounds = None;
                }
                if let Some(b) = edit.bounds.as_mut() {
                    for (row, key) in b.iter_mut().zip(["shell.grid.min", "shell.grid.max"]) {
                        ui.horizontal(|ui| {
                            ui.weak(t!(key));
                            for v in row.iter_mut() {
                                ui.add(coord_value(v));
                            }
                        });
                    }
                    let enabled = !state.doc.selection().objects.is_empty();
                    if ui.add_enabled(enabled, egui::Button::new(t!("shell.grid.fit_selection")).small()).clicked()
                        && let Some(sb) = state.doc.selection_bounds(&|e| state.entity_bounds(e))
                    {
                        *b = [sb.min.to_array(), sb.max.to_array()];
                    }
                }
            });
            ui.end_row();
            let mut colored = edit.color.is_some();
            ui.checkbox(&mut colored, t!("shell.grid.color"));
            if colored {
                let mut c = edit.color.unwrap_or([255, 160, 40]);
                ui.color_edit_button_srgb(&mut c);
                edit.color = Some(c);
            } else {
                edit.color = None;
            }
            ui.end_row();
        });
        if edit != spec {
            match state.put_custom_grid(&spec.name, edit.clone(), in_map, true) {
                Ok(()) => {
                    self.error = None;
                    if !edit.name.eq_ignore_ascii_case(&spec.name) {
                        self.select(edit.sanitized().name, in_map);
                    }
                }
                Err(e) => {
                    self.error = Some(e);
                    self.name_edit = None;
                }
            }
        }
        ui.horizontal_wrapped(|ui| {
            let active = state.settings.grid.mode == GridMode::Custom
                && state.settings.grid.custom.eq_ignore_ascii_case(&spec.name);
            if ui.add_enabled(!active, egui::Button::new(t!("shell.grid.use"))).clicked()
                && let Err(e) = state.use_custom_grid(&spec.name)
            {
                self.error = Some(e);
            }
            let can_attach = !state.doc.selection().objects.is_empty();
            if ui.add_enabled(can_attach, egui::Button::new(t!("shell.grid.attach"))).clicked() {
                state.settings.grid.custom = spec.name.clone();
                state.settings_dirty = true;
                attach_active_grid(state);
            }
            let (copy_key, to_map) =
                if in_map { ("shell.grid.save_preset", false) } else { ("shell.grid.copy_to_map", true) };
            if ui.button(t!(copy_key)).clicked() {
                match state.put_custom_grid("", spec.clone(), to_map, false) {
                    Ok(()) => self.select(spec.name.clone(), to_map),
                    Err(e) => self.error = Some(e),
                }
            }
            if ui.button(t!("shell.grid.delete")).clicked() {
                match state.delete_custom_grid(&spec.name, in_map) {
                    Ok(_) => self.selected = None,
                    Err(e) => self.error = Some(e),
                }
            }
        });
        if in_map {
            let users = state.doc.grid_user_count(&spec.name);
            ui.weak(tf!("shell.grid.users", users));
        }
    }
}

fn coord_value(v: &mut f64) -> egui::DragValue<'_> {
    egui::DragValue::new(v).range(-GRID_COORD_LIMIT..=GRID_COORD_LIMIT).speed(1.0).max_decimals(4)
}

pub(crate) const TR: TrTable = &[
    ("shell.grid.view_top", "Top", "Dessus"),
    ("shell.grid.view_front", "Front", "Face"),
    ("shell.grid.view_side", "Side", "Côté"),
    ("shell.grid.view_label", "Grid ({0})", "Grille ({0})"),
    (
        "shell.grid.tip_global",
        "Global grid: one size for every view (Hammer)",
        "Grille globale : une taille pour toutes les vues (Hammer)",
    ),
    (
        "shell.grid.tip_view",
        "Grid of the {0} view (one grid per view): the size controls change the grid of the view under the mouse, or of the last view used",
        "Grille de la vue {0} (une grille par vue) : les commandes de taille changent la grille de la vue sous la souris, ou de la dernière vue utilisée",
    ),
    (
        "shell.grid.tip_map",
        "Custom grid \"{0}\", saved with the map",
        "Grille personnalisée « {0} », enregistrée avec la map",
    ),
    (
        "shell.grid.tip_preset",
        "Custom grid \"{0}\", saved in the settings",
        "Grille personnalisée « {0} », enregistrée dans les paramètres",
    ),
    ("shell.grid.tip_steps", "Step: {0}", "Pas : {0}"),
    (
        "shell.grid.tip_no_selection_grid",
        "The selection has no grid: the global grid applies",
        "La sélection n'a pas de grille : la grille globale s'applique",
    ),
    (
        "shell.grid.tip_click",
        "Click to choose the grid: global, one per view, a custom grid or the grid of the selection",
        "Cliquez pour choisir la grille : globale, une par vue, personnalisée ou celle de la sélection",
    ),
    ("shell.grid.custom_grids", "Custom grids", "Grilles personnalisées"),
    ("shell.grid.map_entry", "{0}  ({1})", "{0}  ({1})"),
    ("shell.grid.preset_entry", "{0}  ({1}, preset)", "{0}  ({1}, préréglage)"),
    ("shell.grid.preset_tag", "{0} (preset)", "{0} (préréglage)"),
    ("shell.grid.object_grids", "Objects use their grid", "Les objets utilisent leur grille"),
    (
        "shell.grid.object_grids_tip",
        "Editing an object that carries a grid (moving, scaling, stretching, vertices, clipping) or drawing on it snaps to its grid, whatever the grid mode",
        "Modifier un objet qui porte une grille (déplacer, redimensionner, étirer, sommets, découpe) ou dessiner dessus s'aligne sur sa grille, quel que soit le mode",
    ),
    ("shell.grid.plane_3d", "Draw the custom grid in the 3D view", "Afficher la grille personnalisée dans la vue 3D"),
    (
        "shell.grid.no_custom",
        "No custom grid yet: create one in the grid manager or from the selection.",
        "Aucune grille personnalisée : créez-en une dans le gestionnaire de grilles ou depuis la sélection.",
    ),
    ("shell.grid.selection_name", "Selection grid", "Grille de sélection"),
    ("shell.grid.face_name", "Face grid", "Grille de face"),
    ("shell.grid.new_name", "Grid", "Grille"),
    ("shell.grid.created", "Grid \"{0}\" created and active", "Grille « {0} » créée et active"),
    ("shell.grid.need_selection", "Select an object first", "Sélectionnez d'abord un objet"),
    ("shell.grid.need_face", "Select a face first (texture tool)", "Sélectionnez d'abord une face (outil texture)"),
    ("shell.grid.attached", "Grid \"{0}\" attached to {1} object(s)", "Grille « {0} » attachée à {1} objet(s)"),
    ("shell.grid.detached", "Grid detached from {0} object(s)", "Grille détachée de {0} objet(s)"),
    ("shell.grid.manager_title", "Grids", "Grilles"),
    ("shell.grid.mode", "Active grid:", "Grille active :"),
    ("shell.grid.views_title", "Grids of the views", "Grilles des vues"),
    (
        "shell.grid.views_help",
        "Used in the \"One grid per view\" mode: a precise grid in one view, a coarse one in another.",
        "Utilisées dans le mode « Une grille par vue » : une grille précise dans une vue, une grille large dans une autre.",
    ),
    ("shell.grid.col_view", "View", "Vue"),
    ("shell.grid.col_size", "Size", "Taille"),
    ("shell.grid.col_origin", "Origin X Y Z", "Origine X Y Z"),
    ("shell.grid.col_major", "Major lines", "Lignes principales"),
    ("shell.grid.new", "New grid", "Nouvelle grille"),
    ("shell.grid.name", "Name", "Nom"),
    ("shell.grid.in_map", "saved with the map", "enregistrée avec la map"),
    (
        "shell.grid.in_settings",
        "saved in the settings (every map)",
        "enregistrée dans les paramètres (toutes les maps)",
    ),
    ("shell.grid.steps", "Step X Y Z", "Pas X Y Z"),
    (
        "shell.grid.steps_tip",
        "Any size, decimals allowed (12, 24, 1.5...). Equal steps change together; hold Shift to change one",
        "N'importe quelle taille, décimales comprises (12, 24, 1,5...). Des pas égaux changent ensemble ; maintenez Maj pour n'en changer qu'un",
    ),
    ("shell.grid.origin", "Origin", "Origine"),
    ("shell.grid.angles", "Pitch Yaw Roll", "Tangage Lacet Roulis"),
    (
        "shell.grid.angles_tip",
        "Orientation of the grid, like entity angles: follow a rotated building (yaw) or a slope (pitch, roll)",
        "Orientation de la grille, comme les angles d'une entité : suivre un bâtiment tourné (lacet) ou une pente (tangage, roulis)",
    ),
    ("shell.grid.major", "Major line every", "Ligne principale toutes les"),
    ("shell.grid.major_tip", "0: the setting of the views", "0 : le réglage des vues"),
    ("shell.grid.limits", "Limits", "Limites"),
    (
        "shell.grid.limits_tip",
        "The grid applies inside this box only; outside, the global grid applies",
        "La grille ne s'applique que dans cette boîte ; en dehors, la grille globale s'applique",
    ),
    ("shell.grid.min", "Min", "Min"),
    ("shell.grid.max", "Max", "Max"),
    ("shell.grid.fit_selection", "Fit to the selection", "Ajuster à la sélection"),
    ("shell.grid.color", "Colour", "Couleur"),
    ("shell.grid.use", "Use", "Utiliser"),
    ("shell.grid.attach", "Attach to the selection", "Attacher à la sélection"),
    ("shell.grid.save_preset", "Save as preset", "Enregistrer comme préréglage"),
    ("shell.grid.copy_to_map", "Copy to the map", "Copier dans la map"),
    ("shell.grid.delete", "Delete", "Supprimer"),
    ("shell.grid.users", "Attached to {0} object(s) or group(s)", "Attachée à {0} objet(s) ou groupe(s)"),
];

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn steps_and_names_for_the_toolbar() {
        assert_eq!(steps_text([16.0; 3]), "16");
        assert_eq!(steps_text([12.0, 12.0, 8.0]), "12 × 12 × 8");
        assert_eq!(steps_text([1.5, 1.5, 1.5]), "1.5");
        assert_eq!(short_name("Stairs"), "Stairs");
        assert_eq!(short_name("A very long grid name"), "A very long...");
        assert!(short_name("A very long grid name").chars().count() <= TOOLBAR_NAME_CHARS + 1);
        assert!(hp_app::i18n::lint_table(TR).is_empty(), "{:#?}", hp_app::i18n::lint_table(TR));
    }
}
