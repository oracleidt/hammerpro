use std::hash::{Hash, Hasher};
use std::sync::Arc;
use std::sync::mpsc::{Receiver, TryRecvError};

use glam::DVec3;
use hp_app::i18n::TrTable;
use hp_app::{Action, AppState, Severity, t, tf};
use hp_editor::mesh_disp::{
    self, AxisChoice, ConformOptions, Fill, MeshDispError, MeshDispOptions, Origin, Pick, PowerChoice, TilePlan,
    TileSelectionOptions, Tri,
};
use hp_editor::{EditError, FaceRef};

use super::Package;
use crate::app::HammerProApp;

mod preview;
pub(crate) mod source;

use source::{SourceMesh, UpChoice};

pub(crate) struct MeshDisp;

#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
enum Which {
    #[default]
    None,
    Mesh,
    Conform,
    Tile,
}

type Loading = Receiver<Result<SourceMesh, String>>;

#[derive(Default)]
pub(crate) struct State {
    which: Which,
    source: Option<SourceMesh>,
    revision: u64,
    loading: Option<(String, Loading)>,
    error: Option<String>,
    up: UpChoice,
    convert: Option<bool>,
    model_path: String,
    mesh: Option<MeshDispOptions>,
    conform: Option<ConformOptions>,
    tile: Option<TileSelectionOptions>,
    tris: Option<(u64, Arc<Vec<Tri>>)>,
    preview_key: u64,
    plan: Option<Result<TilePlan, MeshDispError>>,
    pending_since: Option<f64>,
}

impl State {
    fn convert(&self) -> bool {
        self.convert.unwrap_or(true)
    }

    fn mesh_opts(&mut self) -> &mut MeshDispOptions {
        self.mesh.get_or_insert_with(MeshDispOptions::default)
    }

    fn conform_opts(&mut self) -> &mut ConformOptions {
        self.conform.get_or_insert_with(ConformOptions::default)
    }

    fn tile_opts(&mut self) -> &mut TileSelectionOptions {
        self.tile.get_or_insert_with(TileSelectionOptions::default)
    }

    fn tris(&mut self) -> Option<Arc<Vec<Tri>>> {
        let source = self.source.as_ref()?;
        let key = self.revision.wrapping_mul(31).wrapping_add(self.up as u64 * 2 + u64::from(self.convert()));
        if let Some((k, t)) = &self.tris
            && *k == key
        {
            return Some(t.clone());
        }
        let tris = Arc::new(source.tris(self.up, self.convert()));
        self.tris = Some((key, tris.clone()));
        Some(tris)
    }

    fn set_source(&mut self, source: SourceMesh) {
        let file = source.is_file();
        self.source = Some(source);
        self.revision += 1;
        self.tris = None;
        self.plan = None;
        self.preview_key = 0;
        self.error = None;
        self.mesh_opts().origin = if file { Origin::Center } else { Origin::Keep };
        self.conform_opts().origin = Origin::Keep;
    }
}

const HANDLED: &[Action] = &[Action::MeshToDisp, Action::DispConformMesh, Action::DispTileSelection];

impl Package for MeshDisp {
    fn handled(&self) -> &'static [Action] {
        HANDLED
    }

    fn handle(&self, app: &mut HammerProApp, _ctx: &egui::Context, action: Action) -> bool {
        match action {
            Action::MeshToDisp => app.open_mesh_window(Which::Mesh),
            Action::DispConformMesh => app.open_mesh_window(Which::Conform),
            Action::DispTileSelection => {
                app.pkg.mesh_disp.which = Which::Tile;
                app.state.runtime.mesh_disp.preview = None;
            }
            _ => return false,
        }
        true
    }

    fn tick(&self, app: &mut HammerProApp, ctx: &egui::Context) {
        let ui = &mut app.pkg.mesh_disp;
        let received = match &ui.loading {
            None => return,
            Some((_, rx)) => rx.try_recv(),
        };
        match received {
            Ok(Ok(source)) => {
                ui.loading = None;
                ui.set_source(source);
            }
            Ok(Err(e)) => {
                let name = ui.loading.take().map_or_else(String::new, |(n, _)| n);
                ui.error = Some(e.clone());
                app.state.notify(Severity::Warning, tf!("mesh_disp.read_failed", name, e));
            }
            Err(TryRecvError::Empty) => ctx.request_repaint_after(std::time::Duration::from_millis(100)),
            Err(TryRecvError::Disconnected) => ui.loading = None,
        }
    }

    fn dialogs(&self, app: &mut HammerProApp, ctx: &egui::Context) {
        match app.pkg.mesh_disp.which {
            Which::None => {
                if app.state.runtime.mesh_disp.preview.is_some() {
                    app.state.runtime.mesh_disp.preview = None;
                }
            }
            Which::Mesh => mesh_window(app, ctx),
            Which::Conform => conform_window(app, ctx),
            Which::Tile => tile_window(app, ctx),
        }
    }

    fn translations(&self) -> TrTable {
        TR
    }
}

fn pick_mesh_file(state: &AppState) -> Option<std::path::PathBuf> {
    let mut dialog = rfd::FileDialog::new().add_filter(t!("mesh_disp.filter"), &source::MESH_EXTENSIONS);
    if let Some(dir) = state
        .doc
        .path()
        .or_else(|| state.doc.opened_from())
        .and_then(|p| p.parent().map(std::path::Path::to_path_buf))
        .or_else(|| state.settings.game.map_dir.clone())
    {
        dialog = dialog.set_directory(dir);
    }
    dialog.pick_file()
}

fn start_loading(ui: &mut State, path: std::path::PathBuf) {
    let name = path.file_name().map_or_else(String::new, |n| n.to_string_lossy().into_owned());
    let (tx, rx) = std::sync::mpsc::channel();
    let spawned = std::thread::Builder::new().name("hp-mesh-disp-read".to_owned()).spawn(move || {
        let r = std::panic::catch_unwind(std::panic::AssertUnwindSafe(|| source::read_file(&path)))
            .unwrap_or_else(|_| Err("the file reader stopped unexpectedly".to_owned()));
        let _ = tx.send(r);
    });
    if spawned.is_ok() {
        ui.loading = Some((name, rx));
        ui.error = None;
    }
}

impl HammerProApp {
    fn open_mesh_window(&mut self, which: Which) {
        if which == Which::Conform && hp_tools_mesh::disp::commands::targets(&self.state.doc).is_empty() {
            self.state.notify(Severity::Info, t!("disp.nothing_selected"));
            return;
        }
        let ui = &self.pkg.mesh_disp;
        if ui.source.is_none() && ui.loading.is_none() {
            if let Some(path) = pick_mesh_file(&self.state) {
                start_loading(&mut self.pkg.mesh_disp, path);
            } else if let Some(id) = source::selected_prop(&self.state) {
                self.load_prop(id);
            } else {
                return;
            }
        }
        self.pkg.mesh_disp.which = which;
    }

    fn load_prop(&mut self, id: u32) {
        match source::from_prop(&self.state, id) {
            Ok(s) => self.pkg.mesh_disp.set_source(s),
            Err(e) => {
                self.pkg.mesh_disp.error = Some(e.clone());
                self.state.notify(Severity::Warning, tf!("mesh_disp.read_failed", id, e));
            }
        }
    }

    fn load_model(&mut self) {
        let path = self.pkg.mesh_disp.model_path.clone();
        match source::from_model(&self.state, &path) {
            Ok(s) => self.pkg.mesh_disp.set_source(s),
            Err(e) => {
                self.pkg.mesh_disp.error = Some(e.clone());
                self.state.notify(Severity::Warning, tf!("mesh_disp.read_failed", path, e));
            }
        }
    }
}

fn combo<T: Copy + PartialEq>(ui: &mut egui::Ui, id: &str, value: &mut T, items: &[(T, String)]) -> bool {
    let current = items.iter().find(|(v, _)| v == value).map_or_else(String::new, |(_, l)| l.clone());
    let mut changed = false;
    egui::ComboBox::from_id_salt(id).selected_text(current).show_ui(ui, |ui| {
        for (v, label) in items {
            if ui.selectable_value(value, *v, label).changed() {
                changed = true;
            }
        }
    });
    changed
}

fn axis_items() -> Vec<(AxisChoice, String)> {
    vec![
        (AxisChoice::Z, t!("mesh_disp.axis_z").to_owned()),
        (AxisChoice::X, t!("mesh_disp.axis_x").to_owned()),
        (AxisChoice::Y, t!("mesh_disp.axis_y").to_owned()),
        (AxisChoice::Best, t!("mesh_disp.axis_best").to_owned()),
    ]
}

fn pick_items() -> Vec<(Pick, String)> {
    vec![(Pick::Top, t!("mesh_disp.pick_top").to_owned()), (Pick::First, t!("mesh_disp.pick_first").to_owned())]
}

fn origin_items() -> Vec<(Origin, String)> {
    vec![
        (Origin::Center, t!("mesh_disp.origin_center").to_owned()),
        (Origin::MinCorner, t!("mesh_disp.origin_min").to_owned()),
        (Origin::Keep, t!("mesh_disp.origin_keep").to_owned()),
    ]
}

fn power_items() -> Vec<(PowerChoice, String)> {
    vec![
        (PowerChoice::Auto, t!("mesh_disp.power_auto").to_owned()),
        (PowerChoice::Fixed(2), "2".to_owned()),
        (PowerChoice::Fixed(3), "3".to_owned()),
        (PowerChoice::Fixed(4), "4".to_owned()),
    ]
}

fn drag(ui: &mut egui::Ui, v: &mut f64, speed: f64, range: std::ops::RangeInclusive<f64>) -> bool {
    ui.add(egui::DragValue::new(v).speed(speed).range(range).max_decimals(3)).changed()
}

fn drag_count(ui: &mut egui::Ui, v: &mut u32, max: u32) -> bool {
    ui.add(egui::DragValue::new(v).speed(0.1).range(0..=max)).changed()
}

fn position_ui(ui: &mut egui::Ui, p: &mut DVec3) -> bool {
    let mut changed = false;
    ui.horizontal(|ui| {
        for v in [&mut p.x, &mut p.y, &mut p.z] {
            changed |= drag(ui, v, 1.0, -131_072.0..=131_072.0);
        }
    });
    changed
}

fn material_ui(ui: &mut egui::Ui, material: &mut String, current: &str) -> bool {
    let mut changed = false;
    ui.horizontal(|ui| {
        changed |= ui.add(egui::TextEdit::singleline(material).desired_width(190.0)).changed();
        if ui.button(t!("mesh_disp.current_texture")).on_hover_text(t!("mesh_disp.current_texture_tip")).clicked()
            && !current.is_empty()
        {
            *material = current.to_owned();
            changed = true;
        }
    });
    changed
}

enum SourceAction {
    None,
    OpenFile,
    Prop(u32),
    Model,
}

fn source_ui(ui: &mut egui::Ui, st: &mut State, prop: Option<u32>, size_known: bool) -> (SourceAction, bool) {
    let mut act = SourceAction::None;
    let mut changed = false;
    ui.group(|ui| {
        ui.label(egui::RichText::new(t!("mesh_disp.source")).strong());
        if let Some((name, _)) = &st.loading {
            ui.label(tf!("mesh_disp.reading", name));
        } else if let Some(s) = &st.source {
            ui.label(tf!("mesh_disp.loaded", s.label, s.triangles));
            if size_known {
                let size = s.size(st.up, st.convert());
                ui.label(
                    egui::RichText::new(tf!("mesh_disp.size", size.x.round(), size.y.round(), size.z.round())).weak(),
                );
            }
        } else {
            ui.label(egui::RichText::new(t!("mesh_disp.no_source")).weak());
        }
        if let Some(e) = &st.error {
            ui.colored_label(hp_app::theme::error(), e);
        }
        ui.horizontal_wrapped(|ui| {
            if ui.button(t!("mesh_disp.open_file")).on_hover_text(t!("mesh_disp.open_file_tip")).clicked() {
                act = SourceAction::OpenFile;
            }
            if ui
                .add_enabled(prop.is_some(), egui::Button::new(t!("mesh_disp.use_prop")))
                .on_hover_text(t!("mesh_disp.use_prop_tip"))
                .clicked()
                && let Some(id) = prop
            {
                act = SourceAction::Prop(id);
            }
        });
        ui.horizontal(|ui| {
            ui.add(
                egui::TextEdit::singleline(&mut st.model_path)
                    .hint_text("models/props_c17/oildrum001.mdl")
                    .desired_width(230.0),
            );
            if ui
                .add_enabled(!st.model_path.trim().is_empty(), egui::Button::new(t!("mesh_disp.load_model")))
                .on_hover_text(t!("mesh_disp.load_model_tip"))
                .clicked()
            {
                act = SourceAction::Model;
            }
        });
        if st.source.as_ref().is_some_and(SourceMesh::is_file) {
            ui.horizontal(|ui| {
                ui.label(t!("mesh_disp.up"));
                let items = [
                    (UpChoice::File, t!("mesh_disp.up_file").to_owned()),
                    (UpChoice::Y, "Y".to_owned()),
                    (UpChoice::Z, "Z".to_owned()),
                ];
                changed |= combo(ui, "hp_mesh_disp_up", &mut st.up, &items);
                let mut convert = st.convert();
                if ui
                    .checkbox(&mut convert, t!("mesh_disp.convert"))
                    .on_hover_text(t!("mesh_disp.convert_tip"))
                    .changed()
                {
                    st.convert = Some(convert);
                    changed = true;
                }
            });
        }
    });
    (act, changed)
}

fn run_source_action(app: &mut HammerProApp, act: SourceAction) {
    match act {
        SourceAction::None => {}
        SourceAction::OpenFile => {
            if let Some(path) = pick_mesh_file(&app.state) {
                start_loading(&mut app.pkg.mesh_disp, path);
            }
        }
        SourceAction::Prop(id) => app.load_prop(id),
        SourceAction::Model => app.load_model(),
    }
}

fn options_hash(o: &MeshDispOptions, st: &State) -> u64 {
    let mut h = std::collections::hash_map::DefaultHasher::new();
    format!("{o:?}").hash(&mut h);
    (st.revision, st.up as u8, st.convert()).hash(&mut h);
    h.finish()
}

fn window_frame(ctx: &egui::Context, title: &str, id: &str, add: impl FnOnce(&mut egui::Ui)) -> bool {
    let mut open = true;
    hp_app::chrome::Window::new(title)
        .id(egui::Id::unique(id))
        .open(&mut open)
        .collapsible(false)
        .resizable(false)
        .vscroll(true)
        .default_height(ctx.viewport_rect().height() - 90.0)
        .max_height(ctx.viewport_rect().height() - 90.0)
        .anchor(egui::Align2::RIGHT_TOP, [-12.0, 60.0])
        .show(ctx, |ui| {
            ui.set_max_width(420.0);
            add(ui);
        });
    open
}

fn mesh_window(app: &mut HammerProApp, ctx: &egui::Context) {
    let prop = source::selected_prop(&app.state);
    let center = app.state.doc.selection_center(&app.entity_bounds_fn());
    let current = app.state.current_material.clone();
    let mut st = std::mem::take(&mut app.pkg.mesh_disp);
    let mut act = SourceAction::None;
    let mut create = false;
    let mut close = false;
    let mut changed = false;
    let mut opts = st.mesh_opts().clone();
    let open = window_frame(ctx, t!("mesh_disp.title"), "hp_mesh_disp_window", |ui| {
        let (a, c) = source_ui(ui, &mut st, prop, true);
        act = a;
        changed |= c;
        ui.add_space(4.0);
        egui::Grid::new("hp_mesh_disp_grid").num_columns(2).spacing([8.0, 4.0]).show(ui, |ui| {
            ui.label(t!("mesh_disp.axis")).on_hover_text(t!("mesh_disp.axis_tip"));
            ui.horizontal(|ui| {
                changed |= combo(ui, "hp_mesh_disp_axis", &mut opts.axis, &axis_items());
                changed |=
                    ui.checkbox(&mut opts.flip, t!("mesh_disp.flip")).on_hover_text(t!("mesh_disp.flip_tip")).changed();
            });
            ui.end_row();
            ui.label(t!("mesh_disp.tile")).on_hover_text(t!("mesh_disp.tile_tip"));
            changed |= drag(ui, &mut opts.tile_size, 8.0, mesh_disp::MIN_TILE..=mesh_disp::MAX_TILE);
            ui.end_row();
            ui.label(t!("mesh_disp.power")).on_hover_text(t!("mesh_disp.power_tip"));
            changed |= combo(ui, "hp_mesh_disp_power", &mut opts.power, &power_items());
            ui.end_row();
            ui.label(t!("mesh_disp.pick")).on_hover_text(t!("mesh_disp.pick_tip"));
            changed |= combo(ui, "hp_mesh_disp_pick", &mut opts.pick, &pick_items());
            ui.end_row();
            ui.label(t!("mesh_disp.fill")).on_hover_text(t!("mesh_disp.fill_tip"));
            let fills = [
                (Fill::Nearest, t!("mesh_disp.fill_nearest").to_owned()),
                (Fill::Base, t!("mesh_disp.fill_base").to_owned()),
            ];
            changed |= combo(ui, "hp_mesh_disp_fill", &mut opts.fill, &fills);
            ui.end_row();
            ui.label(t!("mesh_disp.margin")).on_hover_text(t!("mesh_disp.margin_tip"));
            changed |= drag_count(ui, &mut opts.margin, mesh_disp::MAX_MARGIN);
            ui.end_row();
            ui.label(t!("mesh_disp.smooth")).on_hover_text(t!("mesh_disp.smooth_tip"));
            changed |= drag_count(ui, &mut opts.smooth, mesh_disp::MAX_SMOOTH);
            ui.end_row();
            ui.label(t!("mesh_disp.scale")).on_hover_text(t!("mesh_disp.scale_tip"));
            changed |= drag(ui, &mut opts.scale, 0.01, 0.001..=1000.0);
            ui.end_row();
            ui.label(t!("mesh_disp.origin")).on_hover_text(t!("mesh_disp.origin_tip"));
            changed |= combo(ui, "hp_mesh_disp_origin", &mut opts.origin, &origin_items());
            ui.end_row();
            ui.label(t!("mesh_disp.position"));
            ui.horizontal(|ui| {
                changed |= position_ui(ui, &mut opts.position);
                if ui.add_enabled(center.is_some(), egui::Button::new(t!("mesh_disp.at_selection"))).clicked()
                    && let Some(c) = center
                {
                    opts.position = c;
                    changed = true;
                }
            });
            ui.end_row();
            ui.label(t!("mesh_disp.material"));
            changed |= material_ui(ui, &mut opts.material, &current);
            ui.end_row();
            ui.label(t!("mesh_disp.uv_scale")).on_hover_text(t!("mesh_disp.uv_scale_tip"));
            changed |= drag(ui, &mut opts.uv_scale, 0.01, 0.001..=64.0);
            ui.end_row();
            ui.label(t!("mesh_disp.thickness")).on_hover_text(t!("mesh_disp.thickness_tip"));
            changed |= drag(ui, &mut opts.thickness, 1.0, 1.0..=4096.0);
            ui.end_row();
        });
        changed |=
            ui.checkbox(&mut opts.skirt, t!("mesh_disp.skirt")).on_hover_text(t!("mesh_disp.skirt_tip")).changed();
        changed |=
            ui.checkbox(&mut opts.bottom, t!("mesh_disp.bottom")).on_hover_text(t!("mesh_disp.bottom_tip")).changed();
        if opts.skirt || opts.bottom {
            ui.horizontal(|ui| {
                let mut custom = opts.base_height.is_some();
                if ui.checkbox(&mut custom, t!("mesh_disp.base")).on_hover_text(t!("mesh_disp.base_tip")).changed() {
                    opts.base_height = custom.then_some(0.0);
                    changed = true;
                }
                if let Some(b) = opts.base_height.as_mut() {
                    changed |= drag(ui, b, 1.0, -131_072.0..=131_072.0);
                }
            });
        }
        changed |=
            ui.checkbox(&mut opts.group, t!("mesh_disp.group")).on_hover_text(t!("mesh_disp.group_tip")).changed();
        ui.separator();
        match &st.plan {
            Some(Ok(plan)) => summary_ui(ui, plan, &opts),
            Some(Err(e)) => {
                ui.colored_label(hp_app::theme::error(), error_text(e));
            }
            None => {
                ui.label(egui::RichText::new(t!("mesh_disp.no_source")).weak());
            }
        }
        ui.horizontal(|ui| {
            let ready = matches!(st.plan, Some(Ok(_)));
            create = ui.add_enabled(ready, egui::Button::new(t!("mesh_disp.create"))).clicked();
            close = ui.button(t!("common.close")).clicked();
        });
    });
    *st.mesh_opts() = opts.clone();
    app.pkg.mesh_disp = st;
    run_source_action(app, act);
    let now = ctx.input(|i| i.time);
    refresh_plan(app, &opts, changed, now, ctx);
    if create {
        create_from_window(app, &opts);
    }
    if !open || close {
        app.pkg.mesh_disp.which = Which::None;
        app.state.runtime.mesh_disp.preview = None;
    }
}

fn refresh_plan(app: &mut HammerProApp, opts: &MeshDispOptions, changed: bool, now: f64, ctx: &egui::Context) {
    let st = &mut app.pkg.mesh_disp;
    let key = options_hash(opts, st);
    if st.source.is_none() {
        st.plan = None;
        app.state.runtime.mesh_disp.preview = None;
        return;
    }
    if key == st.preview_key && st.plan.is_some() {
        return;
    }
    if changed {
        st.pending_since = Some(now);
    }
    let waiting = st.pending_since.is_some_and(|t| now - t < 0.12) && st.plan.is_some();
    if waiting {
        ctx.request_repaint_after(std::time::Duration::from_millis(60));
        return;
    }
    st.pending_since = None;
    let Some(tris) = st.tris() else { return };
    st.preview_key = key;
    let plan = mesh_disp::plan(&tris, opts);
    app.state.runtime.mesh_disp.preview = match &plan {
        Ok(p) => Some(Arc::new(preview::plan_lines(p, opts.skirt, opts.bottom))),
        Err(_) => None,
    };
    app.pkg.mesh_disp.plan = Some(plan);
}

fn error_text(e: &MeshDispError) -> String {
    let args = e.args();
    let shown: Vec<&dyn std::fmt::Display> = args.iter().map(|a| a as &dyn std::fmt::Display).collect();
    hp_app::i18n::tf(e.key(), &shown)
}

fn summary_ui(ui: &mut egui::Ui, plan: &TilePlan, opts: &MeshDispOptions) {
    let tiles = plan.tile_count();
    let extra = if opts.skirt { plan.edge_count() } else { 0 };
    ui.label(tf!("mesh_disp.summary", tiles, plan.power, plan.spacing, plan.plane));
    if extra > 0 || opts.bottom {
        ui.label(tf!("mesh_disp.summary_extra", extra, plan.base));
    }
    let w = plan.warnings;
    let warn = |ui: &mut egui::Ui, text: String| {
        ui.colored_label(hp_app::theme::warning(), text);
    };
    if plan.skipped > 0 {
        ui.label(egui::RichText::new(tf!("mesh_disp.skipped", plan.skipped)).weak());
    }
    if w.overhang_nodes > 0 {
        warn(ui, tf!("mesh_disp.warn_overhang", w.overhang_nodes));
    }
    if w.filled_nodes > 0 {
        ui.label(egui::RichText::new(tf!("mesh_disp.filled", w.filled_nodes)).weak());
    }
    if w.over_limit {
        warn(ui, tf!("mesh_disp.warn_limit", mesh_disp::DISP_LIMIT));
    }
}

pub(crate) struct Created {
    pub plan: TilePlan,
    pub result: mesh_disp::MeshDispResult,
}

pub(crate) fn create(state: &mut AppState, tris: &[Tri], opts: &MeshDispOptions) -> Result<Created, String> {
    let plan = mesh_disp::plan(tris, opts).map_err(|e| e.to_string())?;
    let result = state.doc.create_mesh_displacements(&plan, opts).map_err(|e| e.to_string())?;
    Ok(Created { plan, result })
}

fn create_from_window(app: &mut HammerProApp, opts: &MeshDispOptions) {
    let Some(tris) = app.pkg.mesh_disp.tris() else { return };
    match create(&mut app.state, &tris, opts) {
        Ok(c) => {
            app.state.notify(
                Severity::Success,
                tf!("mesh_disp.created", c.result.tiles, c.result.skirts, c.result.bottoms, c.plan.power),
            );
            if c.plan.warnings.overhang_nodes > 0 {
                app.state.notify(Severity::Warning, tf!("mesh_disp.warn_overhang", c.plan.warnings.overhang_nodes));
            }
            app.pkg.mesh_disp.which = Which::None;
            app.state.runtime.mesh_disp.preview = None;
        }
        Err(e) => app.state.notify(Severity::Warning, tf!("mesh_disp.failed", e)),
    }
}

fn conform_window(app: &mut HammerProApp, ctx: &egui::Context) {
    let prop = source::selected_prop(&app.state);
    let center = app.state.doc.selection_center(&app.entity_bounds_fn());
    let count = hp_tools_mesh::disp::commands::targets(&app.state.doc).len();
    let mut st = std::mem::take(&mut app.pkg.mesh_disp);
    let mut act = SourceAction::None;
    let mut apply = false;
    let mut close = false;
    let mut opts = st.conform_opts().clone();
    let open = window_frame(ctx, t!("mesh_disp.conform_title"), "hp_mesh_disp_conform", |ui| {
        let (a, _) = source_ui(ui, &mut st, prop, true);
        act = a;
        ui.label(egui::RichText::new(tf!("mesh_disp.conform_targets", count)).weak());
        egui::Grid::new("hp_mesh_disp_conform_grid").num_columns(2).spacing([8.0, 4.0]).show(ui, |ui| {
            ui.label(t!("mesh_disp.axis")).on_hover_text(t!("mesh_disp.axis_tip_conform"));
            ui.horizontal(|ui| {
                combo(ui, "hp_mesh_conform_axis", &mut opts.axis, &axis_items());
                ui.checkbox(&mut opts.flip, t!("mesh_disp.flip")).on_hover_text(t!("mesh_disp.flip_tip"));
            });
            ui.end_row();
            ui.label(t!("mesh_disp.pick")).on_hover_text(t!("mesh_disp.pick_tip"));
            combo(ui, "hp_mesh_conform_pick", &mut opts.pick, &pick_items());
            ui.end_row();
            ui.label(t!("mesh_disp.scale")).on_hover_text(t!("mesh_disp.scale_tip"));
            drag(ui, &mut opts.scale, 0.01, 0.001..=1000.0);
            ui.end_row();
            ui.label(t!("mesh_disp.origin")).on_hover_text(t!("mesh_disp.origin_tip"));
            combo(ui, "hp_mesh_conform_origin", &mut opts.origin, &origin_items());
            ui.end_row();
            ui.label(t!("mesh_disp.position"));
            ui.horizontal(|ui| {
                position_ui(ui, &mut opts.position);
                if ui.add_enabled(center.is_some(), egui::Button::new(t!("mesh_disp.at_selection"))).clicked()
                    && let Some(c) = center
                {
                    opts.position = c;
                }
            });
            ui.end_row();
            ui.label(t!("mesh_disp.strength")).on_hover_text(t!("mesh_disp.strength_tip"));
            drag(ui, &mut opts.strength, 0.01, 0.0..=1.0);
            ui.end_row();
            ui.label(t!("mesh_disp.offset")).on_hover_text(t!("mesh_disp.offset_tip"));
            drag(ui, &mut opts.offset, 1.0, -16384.0..=16384.0);
            ui.end_row();
        });
        ui.horizontal(|ui| {
            let ready = st.source.is_some() && count > 0;
            apply = ui.add_enabled(ready, egui::Button::new(t!("common.apply"))).clicked();
            close = ui.button(t!("common.close")).clicked();
        });
    });
    *st.conform_opts() = opts.clone();
    app.pkg.mesh_disp = st;
    run_source_action(app, act);
    update_bounds_preview(app, &opts);
    if apply {
        let faces = hp_tools_mesh::disp::commands::targets(&app.state.doc);
        if let Some(tris) = app.pkg.mesh_disp.tris() {
            match conform(&mut app.state, &faces, &tris, &opts) {
                Ok(r) => app.state.notify(Severity::Success, tf!("mesh_disp.conformed", r.faces, r.vertices, r.missed)),
                Err(e) => app.state.notify(Severity::Warning, tf!("mesh_disp.failed", e)),
            }
        }
    }
    if !open || close {
        app.pkg.mesh_disp.which = Which::None;
        app.state.runtime.mesh_disp.preview = None;
    }
}

pub(crate) fn conform(
    state: &mut AppState,
    faces: &[FaceRef],
    tris: &[Tri],
    opts: &ConformOptions,
) -> Result<hp_editor::mesh_disp::ConformResult, String> {
    state.doc.conform_displacements(faces, tris, opts).map_err(|e| match e {
        EditError::Invalid(s) => s,
        other => other.to_string(),
    })
}

fn update_bounds_preview(app: &mut HammerProApp, opts: &ConformOptions) {
    let st = &mut app.pkg.mesh_disp;
    if st.source.is_none() {
        app.state.runtime.mesh_disp.preview = None;
        return;
    }
    let mut h = std::collections::hash_map::DefaultHasher::new();
    format!("{opts:?}").hash(&mut h);
    (st.revision, st.up as u8, st.convert(), 7u8).hash(&mut h);
    let key = h.finish();
    if key == st.preview_key && app.state.runtime.mesh_disp.preview.is_some() {
        return;
    }
    let Some(tris) = st.tris() else { return };
    st.preview_key = key;
    let mo = MeshDispOptions {
        axis: opts.axis,
        flip: opts.flip,
        scale: opts.scale,
        origin: opts.origin,
        position: opts.position,
        ..MeshDispOptions::default()
    };
    app.state.runtime.mesh_disp.preview = mesh_disp::prepare(&tris, &mo, None).ok().map(|p| {
        let a = p.frame.world(p.lo);
        let b = p.frame.world(p.hi);
        Arc::new(preview::box_lines(a.min(b), a.max(b)))
    });
}

fn tile_window(app: &mut HammerProApp, ctx: &egui::Context) {
    let current = app.state.current_material.clone();
    let count = tile_source_count(&app.state);
    let mut opts = app.pkg.mesh_disp.tile_opts().clone();
    let mut apply = false;
    let mut close = false;
    let open = window_frame(ctx, t!("mesh_disp.tile_title"), "hp_mesh_disp_tile", |ui| {
        ui.label(egui::RichText::new(tf!("mesh_disp.tile_targets", count)).weak());
        egui::Grid::new("hp_mesh_disp_tile_grid").num_columns(2).spacing([8.0, 4.0]).show(ui, |ui| {
            ui.label(t!("mesh_disp.axis")).on_hover_text(t!("mesh_disp.axis_tip_tile"));
            ui.horizontal(|ui| {
                combo(ui, "hp_mesh_tile_axis", &mut opts.axis, &axis_items());
                ui.checkbox(&mut opts.flip, t!("mesh_disp.flip")).on_hover_text(t!("mesh_disp.flip_tip"));
            });
            ui.end_row();
            ui.label(t!("mesh_disp.tile")).on_hover_text(t!("mesh_disp.tile_tip_selection"));
            drag(ui, &mut opts.tile_size, 8.0, mesh_disp::MIN_TILE..=mesh_disp::MAX_TILE);
            ui.end_row();
            ui.label(t!("mesh_disp.power")).on_hover_text(t!("mesh_disp.power_tip_selection"));
            combo(ui, "hp_mesh_tile_power", &mut opts.power, &power_items());
            ui.end_row();
            ui.label(t!("mesh_disp.material"));
            material_ui(ui, &mut opts.material, &current);
            ui.end_row();
            ui.label(t!("mesh_disp.uv_scale")).on_hover_text(t!("mesh_disp.uv_scale_tip"));
            drag(ui, &mut opts.uv_scale, 0.01, 0.001..=64.0);
            ui.end_row();
            ui.label(t!("mesh_disp.thickness")).on_hover_text(t!("mesh_disp.thickness_tip"));
            drag(ui, &mut opts.thickness, 1.0, 1.0..=4096.0);
            ui.end_row();
        });
        ui.checkbox(&mut opts.delete_source, t!("mesh_disp.delete_source"))
            .on_hover_text(t!("mesh_disp.delete_source_tip"));
        ui.checkbox(&mut opts.group, t!("mesh_disp.group")).on_hover_text(t!("mesh_disp.group_tip"));
        ui.label(egui::RichText::new(t!("mesh_disp.tile_hint")).weak());
        ui.horizontal(|ui| {
            apply = ui.add_enabled(count > 0, egui::Button::new(t!("common.apply"))).clicked();
            close = ui.button(t!("common.close")).clicked();
        });
    });
    *app.pkg.mesh_disp.tile_opts() = opts.clone();
    if apply {
        match tile_selection(&mut app.state, &opts) {
            Ok(r) => {
                app.state.notify(Severity::Success, tf!("mesh_disp.tiled", r.tiles, r.sources));
                if r.skipped_faces > 0 {
                    app.state.notify(Severity::Info, tf!("mesh_disp.tile_skipped", r.skipped_faces));
                }
            }
            Err(e) => app.state.notify(Severity::Warning, tf!("mesh_disp.failed", e)),
        }
    }
    if !open || close {
        app.pkg.mesh_disp.which = Which::None;
    }
}

fn tile_source_count(state: &AppState) -> usize {
    let faces = state.doc.selected_faces().len();
    if faces > 0 { faces } else { state.doc.selected_objects().len() }
}

pub(crate) fn tile_selection(
    state: &mut AppState,
    opts: &TileSelectionOptions,
) -> Result<hp_editor::mesh_disp::TileSelectionResult, String> {
    state.doc.tile_selection(opts).map_err(|e| e.to_string())
}

pub(crate) const TR: TrTable = &[
    ("mesh_disp.filter", "3D models", "Modèles 3D"),
    ("mesh_disp.title", "Mesh to displacement", "Maillage en displacement"),
    ("mesh_disp.conform_title", "Conform displacements to mesh", "Plaquer les displacements sur un maillage"),
    ("mesh_disp.tile_title", "Tile selection with displacements", "Daller la sélection avec des displacements"),
    ("mesh_disp.source", "Mesh", "Maillage"),
    (
        "mesh_disp.no_source",
        "No mesh yet: open a file, use a selected prop or give a model path.",
        "Aucun maillage : ouvrez un fichier, utilisez un prop sélectionné ou donnez un chemin de modèle.",
    ),
    ("mesh_disp.reading", "Reading {0}...", "Lecture de {0}..."),
    ("mesh_disp.loaded", "{0}: {1} triangles", "{0} : {1} triangles"),
    ("mesh_disp.size", "Size: {0} x {1} x {2} units", "Taille : {0} x {1} x {2} unités"),
    ("mesh_disp.open_file", "Open file...", "Ouvrir un fichier..."),
    ("mesh_disp.open_file_tip", "OBJ, FBX, glTF or GLB file.", "Fichier OBJ, FBX, glTF ou GLB."),
    ("mesh_disp.use_prop", "Use the selected prop", "Utiliser le prop sélectionné"),
    (
        "mesh_disp.use_prop_tip",
        "Reads the model of the selected prop_static or prop_physics, with its position, angles and scale.",
        "Lit le modèle du prop_static ou prop_physics sélectionné, avec sa position, ses angles et son échelle.",
    ),
    ("mesh_disp.load_model", "Load .mdl", "Charger le .mdl"),
    (
        "mesh_disp.load_model_tip",
        "Reads a model of the game content, placed at the origin.",
        "Lit un modèle du contenu du jeu, placé à l'origine.",
    ),
    ("mesh_disp.up", "Up axis of the file", "Axe haut du fichier"),
    ("mesh_disp.up_file", "From the file", "Selon le fichier"),
    ("mesh_disp.convert", "Convert the units", "Convertir les unités"),
    (
        "mesh_disp.convert_tip",
        "Meters of the file become Source units: 1 m = 39.37 units.",
        "Les mètres du fichier deviennent des unités Source : 1 m = 39,37 unités.",
    ),
    ("mesh_disp.axis", "Projection", "Projection"),
    (
        "mesh_disp.axis_tip",
        "Direction of the rays that measure the height of the mesh. Z is top down; Best fit picks the axis the mesh faces most.",
        "Direction des rayons qui mesurent la hauteur du maillage. Z est vu de dessus ; Meilleur choix prend l'axe vers lequel le maillage est le plus tourné.",
    ),
    (
        "mesh_disp.axis_tip_conform",
        "Direction in which each displacement vertex is moved onto the mesh. Best fit follows the normal of the displacements.",
        "Direction dans laquelle chaque sommet du displacement est ramené sur le maillage. Meilleur choix suit la normale des displacements.",
    ),
    (
        "mesh_disp.axis_tip_tile",
        "Axis the tiles face. Best fit picks the axis of the largest selected faces.",
        "Axe vers lequel les dalles sont tournées. Meilleur choix prend l'axe des plus grandes faces sélectionnées.",
    ),
    ("mesh_disp.axis_z", "Z (top down)", "Z (vue de dessus)"),
    ("mesh_disp.axis_x", "X", "X"),
    ("mesh_disp.axis_y", "Y", "Y"),
    ("mesh_disp.axis_best", "Best fit", "Meilleur choix"),
    ("mesh_disp.flip", "From the other side", "Depuis l'autre côté"),
    (
        "mesh_disp.flip_tip",
        "Projects against the axis, for a ceiling or a wall seen from the back.",
        "Projette contre l'axe, pour un plafond ou un mur vu de l'arrière.",
    ),
    ("mesh_disp.tile", "Tile size", "Taille des dalles"),
    (
        "mesh_disp.tile_tip",
        "Width of each displacement tile in units (16 to 4096).",
        "Largeur de chaque dalle de displacement en unités (16 à 4096).",
    ),
    (
        "mesh_disp.tile_tip_selection",
        "Largest size of a tile: each selected face is cut into equal tiles of at most this size.",
        "Taille maximale d'une dalle : chaque face sélectionnée est découpée en dalles égales d'au plus cette taille.",
    ),
    ("mesh_disp.power", "Power", "Puissance"),
    (
        "mesh_disp.power_tip",
        "Auto picks 2, 3 or 4 from the detail of the mesh so that a vertex is about as close as its triangle edges.",
        "Auto prend 2, 3 ou 4 selon le détail du maillage pour qu'un sommet soit à peu près aussi proche que les arêtes de ses triangles.",
    ),
    (
        "mesh_disp.power_tip_selection",
        "Power of the new tiles (auto is 3).",
        "Puissance des nouvelles dalles (auto vaut 3).",
    ),
    ("mesh_disp.power_auto", "Auto", "Auto"),
    ("mesh_disp.pick", "Surface", "Surface"),
    (
        "mesh_disp.pick_tip",
        "Top: the highest surface under the ray. First hit: the highest surface that faces the ray, which ignores the back of the mesh.",
        "Dessus : la surface la plus haute sous le rayon. Premier impact : la surface la plus haute tournée vers le rayon, ce qui ignore l'arrière du maillage.",
    ),
    ("mesh_disp.pick_top", "Top", "Dessus"),
    ("mesh_disp.pick_first", "First hit", "Premier impact"),
    ("mesh_disp.fill", "Holes", "Trous"),
    (
        "mesh_disp.fill_tip",
        "Vertices of a kept tile that the mesh does not cover: copy the nearest height, or drop to the lowest height.",
        "Sommets d'une dalle conservée que le maillage ne couvre pas : copier la hauteur la plus proche, ou descendre à la hauteur la plus basse.",
    ),
    ("mesh_disp.fill_nearest", "Nearest height", "Hauteur la plus proche"),
    ("mesh_disp.fill_base", "Lowest height", "Hauteur la plus basse"),
    ("mesh_disp.margin", "Margin tiles", "Dalles de marge"),
    (
        "mesh_disp.margin_tip",
        "Extra tiles kept around the tiles that touch the mesh (0 to 8).",
        "Dalles gardées en plus autour de celles qui touchent le maillage (0 à 8).",
    ),
    ("mesh_disp.smooth", "Smoothing", "Lissage"),
    (
        "mesh_disp.smooth_tip",
        "Iterations that average each height with its neighbours (0 to 64).",
        "Itérations qui moyennent chaque hauteur avec ses voisines (0 à 64).",
    ),
    ("mesh_disp.scale", "Scale", "Échelle"),
    (
        "mesh_disp.scale_tip",
        "Scale of the mesh after the unit conversion (0.001 to 1000).",
        "Échelle du maillage après la conversion des unités (0,001 à 1000).",
    ),
    ("mesh_disp.origin", "Placement", "Placement"),
    (
        "mesh_disp.origin_tip",
        "Center: the middle of the mesh and its lowest point go to the position. Min corner: the lowest corner goes to the position. Keep: the mesh stays where the file puts it.",
        "Centre : le milieu du maillage et son point le plus bas vont à la position. Coin min : le coin le plus bas va à la position. Garder : le maillage reste où le fichier le met.",
    ),
    ("mesh_disp.origin_center", "Center", "Centre"),
    ("mesh_disp.origin_min", "Min corner", "Coin min"),
    ("mesh_disp.origin_keep", "Keep position", "Garder la position"),
    ("mesh_disp.position", "Position", "Position"),
    ("mesh_disp.at_selection", "At the selection", "Sur la sélection"),
    ("mesh_disp.material", "Texture", "Texture"),
    ("mesh_disp.current_texture", "Current", "Actuelle"),
    (
        "mesh_disp.current_texture_tip",
        "Uses the texture picked in the texture browser.",
        "Utilise la texture choisie dans le navigateur de textures.",
    ),
    ("mesh_disp.uv_scale", "Texture scale", "Échelle de texture"),
    (
        "mesh_disp.uv_scale_tip",
        "Texture scale of the tiles: the texture is continuous from one tile to the next.",
        "Échelle de texture des dalles : la texture se prolonge d'une dalle à l'autre.",
    ),
    ("mesh_disp.thickness", "Brush thickness", "Épaisseur des brushes"),
    (
        "mesh_disp.thickness_tip",
        "Thickness of the brush under each displacement.",
        "Épaisseur du brush sous chaque displacement.",
    ),
    ("mesh_disp.skirt", "Skirts on the border", "Jupes sur le bord"),
    (
        "mesh_disp.skirt_tip",
        "Closes the border of the tiles with walls that go down to the base height.",
        "Ferme le bord des dalles avec des murs qui descendent jusqu'à la hauteur de base.",
    ),
    ("mesh_disp.bottom", "Flat bottom brush", "Brush plat en dessous"),
    (
        "mesh_disp.bottom_tip",
        "A flat brush under the tiles, at the base height.",
        "Un brush plat sous les dalles, à la hauteur de base.",
    ),
    ("mesh_disp.base", "Base height", "Hauteur de base"),
    (
        "mesh_disp.base_tip",
        "Height of the skirt bottoms and of the flat brush. Off: one brush thickness under the lowest point.",
        "Hauteur du bas des jupes et du brush plat. Désactivé : une épaisseur de brush sous le point le plus bas.",
    ),
    ("mesh_disp.group", "Group the new brushes", "Grouper les nouveaux brushes"),
    ("mesh_disp.group_tip", "Puts every new brush in one group.", "Met tous les nouveaux brushes dans un seul groupe."),
    (
        "mesh_disp.summary",
        "{0} tiles of power {1}, one vertex every {2} units, base plane at {3}.",
        "{0} dalles de puissance {1}, un sommet toutes les {2} unités, plan de base à {3}.",
    ),
    ("mesh_disp.summary_extra", "{0} skirt tiles, base height {1}.", "{0} dalles de jupe, hauteur de base {1}."),
    (
        "mesh_disp.skipped",
        "{0} tiles skipped because the mesh barely touches them.",
        "{0} dalles ignorées car le maillage les touche à peine.",
    ),
    (
        "mesh_disp.filled",
        "{0} vertices are not covered by the mesh and were filled.",
        "{0} sommets ne sont pas couverts par le maillage et ont été remplis.",
    ),
    (
        "mesh_disp.warn_overhang",
        "{0} vertices have an overhang: only the top surface is kept.",
        "{0} sommets ont un surplomb : seule la surface du dessus est gardée.",
    ),
    (
        "mesh_disp.warn_limit",
        "More than {0} displacements: the game engine accepts about that many in one map.",
        "Plus de {0} displacements : le moteur du jeu en accepte à peu près autant dans une map.",
    ),
    ("mesh_disp.create", "Create the tiles", "Créer les dalles"),
    (
        "mesh_disp.created",
        "Mesh converted: {0} tiles, {1} skirts, {2} bottom brushes, power {3}.",
        "Maillage converti : {0} dalles, {1} jupes, {2} brushes de fond, puissance {3}.",
    ),
    ("mesh_disp.failed", "Mesh to displacement failed: {0}", "Maillage en displacement impossible : {0}"),
    ("mesh_disp.read_failed", "Cannot read {0}: {1}", "Lecture impossible de {0} : {1}"),
    (
        "mesh_disp.conform_targets",
        "{0} displacements will be moved onto the mesh.",
        "{0} displacements seront ramenés sur le maillage.",
    ),
    ("mesh_disp.strength", "Strength", "Force"),
    (
        "mesh_disp.strength_tip",
        "0 keeps the displacement, 1 puts it exactly on the mesh.",
        "0 garde le displacement, 1 le met exactement sur le maillage.",
    ),
    ("mesh_disp.offset", "Height offset", "Décalage de hauteur"),
    (
        "mesh_disp.offset_tip",
        "Units added to the mesh height along the axis.",
        "Unités ajoutées à la hauteur du maillage le long de l'axe.",
    ),
    (
        "mesh_disp.conformed",
        "{0} displacements conformed: {1} vertices moved, {2} outside the mesh.",
        "{0} displacements plaqués : {1} sommets déplacés, {2} hors du maillage.",
    ),
    (
        "mesh_disp.tile_targets",
        "{0} selected items will become flat displacement tiles.",
        "{0} éléments sélectionnés deviendront des dalles de displacement plates.",
    ),
    ("mesh_disp.delete_source", "Delete the source brushes", "Supprimer les brushes d'origine"),
    (
        "mesh_disp.delete_source_tip",
        "Removes the brushes whose faces were tiled. Undo brings them back.",
        "Supprime les brushes dont les faces ont été dallées. Annuler les ramène.",
    ),
    (
        "mesh_disp.tile_hint",
        "Then use Conform displacements to mesh to shape them.",
        "Utilisez ensuite Plaquer les displacements sur un maillage pour les modeler.",
    ),
    ("mesh_disp.tiled", "{0} flat tiles made from {1} brushes.", "{0} dalles plates créées à partir de {1} brushes."),
    (
        "mesh_disp.tile_skipped",
        "{0} faces were skipped: they are not flat and square to the axis.",
        "{0} faces ignorées : elles ne sont pas plates et perpendiculaires à l'axe.",
    ),
    ("mesh_disp.err_empty", "The mesh has no triangle.", "Le maillage n'a aucun triangle."),
    (
        "mesh_disp.err_no_hit",
        "No point of the mesh lies under or over the tiles along this axis.",
        "Aucun point du maillage ne se trouve sous ou sur les dalles le long de cet axe.",
    ),
    ("mesh_disp.err_option", "{0}", "{0}"),
    (
        "mesh_disp.err_tiles",
        "{0} tiles are too many (at most {1}): use bigger tiles or a smaller mesh.",
        "{0} dalles, c'est trop (au plus {1}) : utilisez de plus grandes dalles ou un maillage plus petit.",
    ),
    (
        "mesh_disp.err_base",
        "The base height {0} must be below the lowest point of the surface ({1}).",
        "La hauteur de base {0} doit être sous le point le plus bas de la surface ({1}).",
    ),
];

pub(crate) fn script_open(app: &mut HammerProApp, path: std::path::PathBuf, confirm: bool) {
    match source::read_file(&path) {
        Ok(s) => app.pkg.mesh_disp.set_source(s),
        Err(e) => {
            app.state.notify(Severity::Warning, tf!("mesh_disp.read_failed", path.display(), e));
            return;
        }
    }
    app.pkg.mesh_disp.which = Which::Mesh;
    if confirm {
        let opts = app.pkg.mesh_disp.mesh_opts().clone();
        create_from_window(app, &opts);
    }
}
