use std::sync::mpsc::Receiver;
use std::time::{Duration, Instant};

use glam::DVec3;
use hp_app::extend::{StatusContext, StatusFit, StatusSegment, StatusSide, status_order, status_priority};
use hp_app::i18n::TrTable;
use hp_app::runtime::RealDims;
use hp_app::view::ViewInfo;
use hp_app::{Action, AppState, Severity, ViewKind, t, tf};
use hp_editor::ObjRef;
use hp_render::Overlay;
use hp_ui_viewports::layers::OverlayLayer;
use hp_ui_viewports::overlay::{cross_marker, thick_line_on_top};

use crate::app::HammerProApp;
use crate::dispatch::mcp::{self, PlaceReport, UiPlace};

pub(crate) const HANDLED: &[Action] = &[
    Action::PlaceOnTop,
    Action::PlaceAgainstWall,
    Action::PlaceAgainstWallAway,
    Action::PlaceNextTo,
    Action::SettleFloating,
    Action::FitToGround,
    Action::ShowShapePreview,
    Action::ToggleRealDimensions,
];

const DIMS_DEBOUNCE: Duration = Duration::from_millis(120);
const SHEET_W: u32 = 960;
const SHEET_H: u32 = 720;
const SETTLE_LABELS: &[&str] = &["Create entity"];

#[derive(Clone, Debug, PartialEq, Eq)]
enum Source {
    Selection(u64),
    Model(String, u64),
}

#[derive(Default)]
pub(crate) struct State {
    dims_seen: Option<(u64, u64)>,
    dims_changed: Option<Instant>,
    settle_serial: Option<u64>,
    sheet_source: Option<Source>,
    sheet_job: Option<(Source, Receiver<Result<mcp::Sheet, String>>)>,
    sheet_texture: Option<egui::TextureHandle>,
    sheet_caption: String,
    sheet_error: Option<String>,
    use_model: bool,
}

pub(crate) fn handle(app: &mut HammerProApp, action: Action) -> bool {
    match action {
        Action::PlaceOnTop => place_on_target(app, UiPlace::OnTop, "Place on top", action),
        Action::PlaceNextTo => place_on_target(app, UiPlace::NextTo, "Place next to", action),
        Action::PlaceAgainstWall | Action::PlaceAgainstWallAway => against(app, action),
        Action::SettleFloating => settle(app),
        Action::FitToGround => fit(app),
        Action::ShowShapePreview => {
            let rt = &mut app.state.runtime.placement;
            rt.shape_preview_open = !rt.shape_preview_open;
            if rt.shape_preview_open {
                app.pkg.placement.ops.use_model = std::mem::take(&mut rt.shape_preview_focus_model);
                app.pkg.placement.ops.sheet_source = None;
            }
        }
        Action::ToggleRealDimensions => {
            let s = &mut app.state.settings.placement;
            s.show_real_dimensions = !s.show_real_dimensions;
            app.state.settings_dirty = true;
            if !s.show_real_dimensions {
                app.state.runtime.placement.real_dims = None;
            }
            app.pkg.placement.ops.dims_seen = None;
        }
        _ => return false,
    }
    true
}

fn plural(n: usize, one: &'static str, other: &'static str) -> &'static str {
    let singular = match hp_app::i18n::lang() {
        hp_app::i18n::Lang::Fr => n <= 1,
        hp_app::i18n::Lang::En => n == 1,
        hp_app::i18n::Lang::Ru => n % 10 == 1 && n % 100 != 11,
    };
    if singular { one } else { other }
}

fn fmt(x: f64) -> String {
    let r = (x * 100.0).round() / 100.0;
    let r = if r == 0.0 { 0.0 } else { r };
    if r == r.trunc() { format!("{}", r as i64) } else { format!("{r}") }
}

fn report_text(r: &PlaceReport) -> String {
    let mut text = match r.gap {
        Some(g) => tf!(plural(r.moved, "placeops.moved_gap_one", "placeops.moved_gap"), r.moved, fmt(g.max(0.0))),
        None => tf!(plural(r.moved, "placeops.moved_one", "placeops.moved"), r.moved),
    };
    if r.overlaps > 0 {
        text.push_str(&tf!("placeops.still_overlapping", r.overlaps));
    }
    if r.other_landing {
        text.push_str(t!("placeops.other_landing"));
    }
    if !r.stayed.is_empty() {
        text.push_str(&tf!("placeops.stayed", r.stayed.len()));
    }
    text
}

fn notify_report(app: &mut HammerProApp, action: Action, result: Result<PlaceReport, String>) {
    match result {
        Ok(r) if r.moved == 0 => {
            let mut text = t!("placeops.nothing_moved").to_owned();
            if let Some(reason) = r.stayed.first() {
                text.push_str(&format!(" ({reason})"));
            }
            app.state.notify(Severity::Info, text);
        }
        Ok(r) => {
            let sev = if r.overlaps > 0 || r.other_landing { Severity::Warning } else { Severity::Success };
            let text = report_text(&r);
            app.state.notify(sev, text);
        }
        Err(e) => app.state.notify(Severity::Warning, tf!("app.op_failed", action.label(), e)),
    }
}

fn place_on_target(app: &mut HammerProApp, how: UiPlace, label: &str, action: Action) {
    let sel = app.selected();
    if sel.len() < 2 {
        app.state.notify(Severity::Info, t!("placeops.need_target"));
        return;
    }
    let target = sel.last().copied();
    let movers = &sel[..sel.len() - 1];
    let result = mcp::place_objects(&mut app.state, movers, target, how, label);
    notify_report(app, action, result);
}

fn view_kind(app: &HammerProApp) -> Option<ViewKind> {
    let index = app.state.ui.hovered_view.or(app.state.ui.focused_view)?;
    hp_app::QuadrantView::from_index(app.state.settings.ui.quadrants.get(index).copied().unwrap_or(1)).kind()
}

fn face_normal(app: &HammerProApp) -> Option<DVec3> {
    let face = app.state.doc.selection().faces.first().copied()?;
    let poly = app.state.doc.face_polygon(face)?;
    if poly.len() < 3 {
        return None;
    }
    let mut n = DVec3::ZERO;
    for i in 0..poly.len() {
        n += poly[i].cross(poly[(i + 1) % poly.len()]);
    }
    n.try_normalize()
}

pub(crate) fn wall_direction(app: &mut HammerProApp, movers: &[ObjRef]) -> Option<DVec3> {
    if let Some((_, n)) = app.state.runtime.placement.menu_hit.take()
        && let Some(d) = (-n).try_normalize()
    {
        return Some(d);
    }
    if let Some(n) = face_normal(app) {
        return Some(-n);
    }
    let flat = |d: DVec3| DVec3::new(d.x, d.y, 0.0).try_normalize();
    match view_kind(app) {
        Some(ViewKind::Front) => Some(DVec3::NEG_X),
        Some(ViewKind::Side) => Some(DVec3::Y),
        Some(ViewKind::Top) | None => mcp::nearest_wall(&app.state, movers),
        Some(_) => flat(app.state.cameras.cam3d.forward()).or_else(|| mcp::nearest_wall(&app.state, movers)),
    }
}

fn against(app: &mut HammerProApp, action: Action) {
    if !app.require_selection() {
        return;
    }
    let movers = app.selected();
    let Some(dir) = wall_direction(app, &movers) else {
        app.state.notify(Severity::Info, t!("placeops.no_wall"));
        return;
    };
    let away = action == Action::PlaceAgainstWallAway;
    let result =
        mcp::place_objects(&mut app.state, &movers, None, UiPlace::Against { dir, away }, "Place against wall");
    notify_report(app, action, result);
}

fn fit(app: &mut HammerProApp) {
    if !app.require_selection() {
        return;
    }
    let movers = app.selected();
    let result = mcp::place_objects(&mut app.state, &movers, None, UiPlace::Fit, "Fit to ground");
    notify_report(app, Action::FitToGround, result);
}

fn settle(app: &mut HammerProApp) {
    let sel = app.selected();
    let whole = sel.is_empty();
    let fresh = app.state.runtime.health.placement_serial == Some(app.state.doc.content_serial());
    if whole && fresh {
        let rt = &app.state.runtime.health;
        let problems: Vec<hp_editor::Problem> = rt
            .problems
            .iter()
            .filter(|p| p.fixable && matches!(p.code, "floating_object" | "sunk_object"))
            .cloned()
            .collect();
        let floating = problems.iter().filter(|p| p.code == "floating_object").count();
        let sunk = problems.len() - floating;
        let result = app.state.doc.transaction("Settle floating objects", |doc| doc.fix_problems(&problems));
        match result {
            Ok(0) => app.state.notify(Severity::Info, t!("placeops.nothing_floating")),
            Ok(_) => app.state.notify(Severity::Success, tf!("placeops.settled", floating, sunk)),
            Err(e) => app.state.notify(Severity::Warning, tf!("app.op_failed", Action::SettleFloating.label(), e)),
        }
        app.state.runtime.health.recheck = true;
        return;
    }
    let objs = if whole { None } else { Some(sel.as_slice()) };
    match mcp::settle_objects(&mut app.state, objs, "Settle floating objects") {
        Ok(r) if r.moved() == 0 => app.state.notify(Severity::Info, t!("placeops.nothing_floating")),
        Ok(r) => app.state.notify(Severity::Success, tf!("placeops.settled", r.floating, r.sunk)),
        Err(e) => app.state.notify(Severity::Warning, tf!("app.op_failed", Action::SettleFloating.label(), e)),
    }
    app.state.runtime.health.recheck = true;
}

fn selection_key(app: &HammerProApp) -> (u64, u64) {
    use std::hash::{Hash, Hasher};
    let mut h = std::collections::hash_map::DefaultHasher::new();
    for o in app.state.doc.selection().objects.iter().take(256) {
        o.hash(&mut h);
    }
    app.state.doc.selection().objects.len().hash(&mut h);
    app.state.assets.fs().map_or(0, |f| f.generation()).hash(&mut h);
    (app.state.doc.revision(), h.finish())
}

fn refresh_dims(app: &mut HammerProApp, ctx: &egui::Context) {
    if !app.state.settings.placement.show_real_dimensions {
        app.state.runtime.placement.real_dims = None;
        return;
    }
    let key = selection_key(app);
    if app.state.runtime.placement.real_dims.as_ref().is_some_and(|d| d.key == key) {
        return;
    }
    if app.state.doc.selection().objects.is_empty() {
        app.state.runtime.placement.real_dims = None;
        app.pkg.placement.ops.dims_seen = Some(key);
        return;
    }
    let ops = &mut app.pkg.placement.ops;
    if ops.dims_seen != Some(key) {
        ops.dims_seen = Some(key);
        ops.dims_changed = Some(Instant::now());
    }
    let waiting = ops.dims_changed.is_some_and(|t| t.elapsed() < DIMS_DEBOUNCE);
    if waiting || app.state.doc.in_transaction() {
        ctx.request_repaint_after(DIMS_DEBOUNCE);
        return;
    }
    let objs = app.selected();
    let mut dims = mcp::real_dims(&app.state, &objs);
    dims.key = key;
    app.state.runtime.placement.real_dims = Some(dims);
}

fn auto_settle(app: &mut HammerProApp) {
    let Some(serial) = app.state.doc.last_step_serial() else { return };
    let seen = app.pkg.placement.ops.settle_serial.replace(serial);
    if seen.is_none_or(|s| s >= serial) || !app.state.settings.placement.auto_settle_on_create {
        return;
    }
    if app.state.doc.in_transaction() || !app.state.doc.last_step_label().is_some_and(|l| SETTLE_LABELS.contains(&l)) {
        return;
    }
    let Some((_, created)) = app.state.doc.last_step_created() else { return };
    if created.is_empty() || created.len() > 64 {
        return;
    }
    if mcp::auto_settle_created(&mut app.state, &created, true) > 0 {
        app.pkg.placement.ops.settle_serial = app.state.doc.last_step_serial();
    }
}

pub(crate) fn tick(app: &mut HammerProApp, ctx: &egui::Context) {
    auto_settle(app);
    refresh_dims(app, ctx);
    poll_sheet(app, ctx);
}

pub(crate) fn map_loaded(app: &mut HammerProApp) {
    let ops = &mut app.pkg.placement.ops;
    ops.dims_seen = None;
    ops.settle_serial = app.state.doc.last_step_serial();
    ops.sheet_source = None;
    app.state.runtime.placement.real_dims = None;
}

fn wanted_source(app: &HammerProApp) -> Option<Source> {
    let rt = &app.state.runtime.placement;
    let fs = app.state.assets.fs().map_or(0, |f| f.generation());
    if app.pkg.placement.ops.use_model
        && let Some(m) = rt.shape_preview_model.clone()
    {
        return Some(Source::Model(m, fs));
    }
    if app.state.doc.selection().objects.is_empty() {
        return rt.shape_preview_model.clone().map(|m| Source::Model(m, fs));
    }
    Some(Source::Selection(selection_key(app).0 ^ fs))
}

fn poll_sheet(app: &mut HammerProApp, ctx: &egui::Context) {
    if !app.state.runtime.placement.shape_preview_open {
        app.pkg.placement.ops.sheet_job = None;
        return;
    }
    if std::mem::take(&mut app.state.runtime.placement.shape_preview_focus_model) {
        app.pkg.placement.ops.use_model = true;
        app.pkg.placement.ops.sheet_source = None;
    }
    if let Some((src, rx)) = &app.pkg.placement.ops.sheet_job {
        match rx.try_recv() {
            Ok(result) => {
                let src = src.clone();
                let ops = &mut app.pkg.placement.ops;
                ops.sheet_job = None;
                match result {
                    Ok(sheet) => {
                        let img = egui::ColorImage::from_rgba_unmultiplied(
                            [sheet.image.width as usize, sheet.image.height as usize],
                            &sheet.image.rgba,
                        );
                        ops.sheet_texture =
                            Some(ctx.load_texture("hp_shape_preview", img, egui::TextureOptions::LINEAR));
                        ops.sheet_caption = sheet.caption;
                        ops.sheet_error = None;
                    }
                    Err(e) => {
                        ops.sheet_texture = None;
                        ops.sheet_error = Some(e);
                    }
                }
                ops.sheet_source = Some(src);
            }
            Err(std::sync::mpsc::TryRecvError::Empty) => {
                ctx.request_repaint_after(Duration::from_millis(60));
                return;
            }
            Err(std::sync::mpsc::TryRecvError::Disconnected) => app.pkg.placement.ops.sheet_job = None,
        }
    }
    let Some(want) = wanted_source(app) else {
        let ops = &mut app.pkg.placement.ops;
        ops.sheet_texture = None;
        ops.sheet_error = None;
        ops.sheet_source = None;
        return;
    };
    if app.pkg.placement.ops.sheet_source.as_ref() == Some(&want) || app.state.doc.in_transaction() {
        return;
    }
    let job = match &want {
        Source::Model(m, _) => Ok(mcp::sheet_job_model(&app.state, m, SHEET_W, SHEET_H)),
        Source::Selection(_) => mcp::sheet_job_selection(&app.state, SHEET_W, SHEET_H),
    };
    match job {
        Ok(job) => {
            let (tx, rx) = std::sync::mpsc::channel();
            let waker = ctx.clone();
            let spawned = std::thread::Builder::new().name("hp-shape-preview".into()).spawn(move || {
                let r = std::panic::catch_unwind(std::panic::AssertUnwindSafe(|| job.run()))
                    .unwrap_or_else(|_| Err("internal error".to_owned()));
                let _ = tx.send(r);
                waker.request_repaint();
            });
            if spawned.is_ok() {
                app.pkg.placement.ops.sheet_job = Some((want, rx));
            }
        }
        Err(e) => {
            let ops = &mut app.pkg.placement.ops;
            ops.sheet_texture = None;
            ops.sheet_error = Some(e);
            ops.sheet_source = Some(want);
        }
    }
}

pub(crate) fn dialogs(app: &mut HammerProApp, ctx: &egui::Context) {
    if !app.state.runtime.placement.shape_preview_open {
        return;
    }
    let mut open = true;
    let model = app.state.runtime.placement.shape_preview_model.clone();
    let has_sel = !app.state.doc.selection().objects.is_empty();
    let dims = app.state.runtime.placement.real_dims.clone();
    let busy = app.pkg.placement.ops.sheet_job.is_some();
    let mut refresh = false;
    let mut use_model = app.pkg.placement.ops.use_model;
    let ops = &app.pkg.placement.ops;
    let (texture, caption, error) = (ops.sheet_texture.clone(), ops.sheet_caption.clone(), ops.sheet_error.clone());
    hp_app::chrome::Window::new(t!("placeops.preview_title"))
        .id(egui::Id::unique("hp_shape_preview"))
        .open(&mut open)
        .default_size([640.0, 560.0])
        .min_size([360.0, 300.0])
        .resizable(true)
        .collapsible(true)
        .show(ctx, |ui| {
            ui.horizontal(|ui| {
                ui.add_enabled_ui(has_sel || !use_model, |ui| {
                    if ui.selectable_label(!use_model, t!("placeops.preview_selection")).clicked() {
                        use_model = false;
                    }
                });
                if let Some(m) = &model
                    && ui.selectable_label(use_model, tf!("placeops.preview_model", m)).clicked()
                {
                    use_model = true;
                }
                ui.with_layout(egui::Layout::right_to_left(egui::Align::Center), |ui| {
                    if ui.button(t!("placeops.preview_refresh")).clicked() {
                        refresh = true;
                    }
                    if busy {
                        ui.spinner();
                    }
                });
            });
            if !use_model && let Some(d) = &dims {
                ui.label(egui::RichText::new(d.summary()).strong());
            }
            ui.separator();
            if let Some(e) = &error {
                ui.label(egui::RichText::new(tf!("placeops.preview_error", e)).color(hp_app::theme::warning()));
            } else if let Some(tex) = &texture {
                let size = tex.size_vec2();
                let avail = ui.available_size() - egui::vec2(0.0, 40.0);
                let scale = (avail.x / size.x).min(avail.y.max(120.0) / size.y).clamp(0.1, 2.0);
                ui.add(egui::Image::new((tex.id(), size * scale)));
                ui.label(egui::RichText::new(caption.as_str()).small().weak());
            } else if !has_sel && model.is_none() {
                ui.label(egui::RichText::new(t!("placeops.preview_empty")).weak());
            } else {
                ui.spinner();
            }
        });
    let ops = &mut app.pkg.placement.ops;
    if use_model != ops.use_model || refresh {
        ops.use_model = use_model;
        ops.sheet_source = None;
    }
    if !open {
        app.state.runtime.placement.shape_preview_open = false;
    }
}

pub(crate) struct RealDimsLayer;

const FOOT: [f32; 4] = [0.35, 0.9, 1.0, 1.0];
const RECT: [f32; 4] = [0.35, 0.9, 1.0, 0.45];
const CONTACT: [f32; 4] = [0.4, 1.0, 0.45, 1.0];
const FLOAT: [f32; 4] = [1.0, 0.65, 0.15, 1.0];
const SUNK: [f32; 4] = [1.0, 0.3, 0.3, 1.0];

impl OverlayLayer for RealDimsLayer {
    fn name(&self) -> &'static str {
        "real_dimensions"
    }

    fn add(&self, state: &AppState, view: &ViewInfo, out: &mut Overlay) {
        if !state.settings.placement.show_real_dimensions {
            return;
        }
        let Some(d) = state.runtime.placement.real_dims.as_ref() else { return };
        for u in &d.units {
            let z = u.min.z;
            let ring = |pts: &[glam::DVec2], color: [f32; 4], out: &mut Overlay| {
                for i in 0..pts.len() {
                    let a = pts[i];
                    let b = pts[(i + 1) % pts.len()];
                    thick_line_on_top(out, view, DVec3::new(a.x, a.y, z), DVec3::new(b.x, b.y, z), color);
                }
            };
            ring(&u.footprint, FOOT, out);
            if !view.is_2d() || view.kind == ViewKind::Top {
                ring(&u.rect, RECT, out);
            }
            let low = u.contacts.iter().copied().fold(None::<DVec3>, |a, p| Some(a.map_or(p, |q| q + p)));
            let low = low.map(|s| s / u.contacts.len().max(1) as f64);
            for c in u.contacts.iter().take(64) {
                cross_marker(out, view, *c, 3.0, CONTACT);
            }
            if let (Some(g), Some(p)) = (u.gap, low)
                && g.abs() > 0.05
            {
                let color = if g > 0.0 { FLOAT } else { SUNK };
                thick_line_on_top(out, view, p, p - DVec3::Z * g, color);
                cross_marker(out, view, p - DVec3::Z * g, 5.0, color);
            }
        }
    }

    fn paint(&self, state: &AppState, view: &ViewInfo, painter: &egui::Painter) {
        if !state.settings.placement.show_real_dimensions {
            return;
        }
        let Some(d) = state.runtime.placement.real_dims.as_ref() else { return };
        for u in d.units.iter().take(8) {
            let c = (u.min + u.max) * 0.5;
            let top = DVec3::new(c.x, c.y, u.max.z);
            let Some(pos) = view.world_to_screen(top) else { continue };
            let color = match u.gap {
                Some(g) if g > 0.5 => hp_app::theme::warning(),
                Some(g) if g < -0.5 => hp_app::theme::error(),
                _ if u.overlap > 0.5 => hp_app::theme::error(),
                _ => hp_ui_viewports::paint::TEXT,
            };
            let text = hp_app::runtime::placement_unit_text(u);
            hp_ui_viewports::paint::text_pill(
                painter,
                pos - egui::vec2(0.0, 6.0),
                egui::Align2::CENTER_BOTTOM,
                text,
                color,
            );
        }
    }
}

pub(crate) struct RealDimsStatus;

impl StatusSegment for RealDimsStatus {
    fn id(&self) -> &'static str {
        "placement_real_dims"
    }

    fn side(&self) -> StatusSide {
        StatusSide::Left
    }

    fn order(&self) -> i32 {
        status_order::SELECTION + 20
    }

    fn fit(&mut self, ui: &egui::Ui, state: &AppState) -> Option<StatusFit> {
        let (text, _) = real_dims_text(state)?;
        let width = egui::WidgetText::from(egui::RichText::new(text).small())
            .into_galley(ui, Some(egui::TextWrapMode::Extend), f32::INFINITY, egui::TextStyle::Small)
            .size()
            .x
            .ceil()
            + 2.0 * ui.spacing().button_padding.x;
        Some(StatusFit::shrinkable(width.min(REAL_DIMS_MIN_WIDTH), width).with_priority(status_priority::REAL_DIMS))
    }

    fn ui(&mut self, ui: &mut egui::Ui, state: &mut AppState, _cx: &mut StatusContext) {
        let Some((text, warn)) = real_dims_text(state) else { return };
        let Some(d) = state.runtime.placement.real_dims.as_ref() else { return };
        let rich = egui::RichText::new(text).small();
        let rich = if warn { rich.color(hp_app::theme::warning()) } else { rich };
        let details = format!(
            "{}
{}",
            d.summary(),
            details_text(d)
        );
        let r = ui.add(egui::Button::new(rich).frame(false).truncate()).on_hover_text(details);
        if r.clicked() {
            state.request(Action::ShowShapePreview);
        }
    }
}

const REAL_DIMS_MIN_WIDTH: f32 = 90.0;

fn real_dims_text(state: &AppState) -> Option<(String, bool)> {
    if !state.settings.placement.show_real_dimensions {
        return None;
    }
    let d = state.runtime.placement.real_dims.as_ref()?;
    let n = hp_app::runtime::real_num;
    let s = d.single().map_or(d.size, |u| u.size);
    let mut text = format!("📏 {} × {} × {}", n(s.x), n(s.y), n(s.z));
    let mut warn = false;
    if let Some(u) = d.single() {
        match u.gap {
            Some(g) if g.abs() > 0.05 => {
                text.push_str(&format!("  ⬇ {}", n(g)));
                warn = g.abs() > 0.5;
            }
            _ => {}
        }
        if u.overlap > 0.05 {
            text.push_str(&format!("  ⚠ {}", n(u.overlap)));
            warn |= u.overlap > 0.5;
        }
    }
    Some((text, warn))
}

fn details_text(d: &RealDims) -> String {
    let mut lines: Vec<String> =
        d.units.iter().map(|u| format!("{}: {}", u.label, hp_app::runtime::placement_unit_text(u))).collect();
    if d.objects > d.units.len() {
        lines.push(tf!("placeops.more_objects", d.objects - d.units.len()));
    }
    lines.push(t!("placeops.status_tip").to_owned());
    lines.join("\n")
}

pub(crate) const TR: TrTable = &[
    ("placeops.moved_one", "Moved {0} object", "{0} objet déplacé"),
    ("placeops.moved", "Moved {0} objects", "{0} objets déplacés"),
    ("placeops.moved_gap_one", "Moved {0} object, gap {1}", "{0} objet déplacé, écart {1}"),
    ("placeops.moved_gap", "Moved {0} objects, gap {1}", "{0} objets déplacés, écart {1}"),
    ("placeops.still_overlapping", ", still overlapping {0} objects", ", chevauche encore {0} objets"),
    (
        "placeops.other_landing",
        ", it landed on another object than the target",
        ", il s'est posé sur un autre objet que la cible",
    ),
    ("placeops.stayed", ", {0} could not move", ", {0} n'ont pas pu bouger"),
    ("placeops.nothing_moved", "Nothing moved", "Rien n'a bougé"),
    (
        "placeops.need_target",
        "Select the objects to move, then the target last (Ctrl+click)",
        "Sélectionnez les objets à déplacer, puis la cible en dernier (Ctrl+clic)",
    ),
    (
        "placeops.no_wall",
        "No wall found: select a face of the wall or use the right click menu on it",
        "Aucun mur trouvé : sélectionnez une face du mur ou utilisez le menu du clic droit dessus",
    ),
    ("placeops.nothing_floating", "No floating or sunk object to settle", "Aucun objet flottant ou enfoncé à reposer"),
    (
        "placeops.settled",
        "Settled objects: {0} floating dropped, {1} sunk lifted",
        "Objets reposés : {0} flottants descendus, {1} enfoncés remontés",
    ),
    ("placeops.preview_title", "Shape preview", "Aperçu de la forme"),
    ("placeops.preview_selection", "Selection", "Sélection"),
    ("placeops.preview_model", "Model: {0}", "Modèle : {0}"),
    ("placeops.preview_refresh", "Refresh", "Actualiser"),
    ("placeops.preview_error", "Cannot draw the shape: {0}", "Impossible de dessiner la forme : {0}"),
    (
        "placeops.preview_empty",
        "Select an object or pick a model in the model browser (Shape preview button)",
        "Sélectionnez un objet ou choisissez un modèle dans le navigateur de modèles (bouton Aperçu de la forme)",
    ),
    ("placeops.more_objects", "and {0} more objects", "et {0} autres objets"),
    (
        "placeops.status_tip",
        "Real size from the mesh (View > Real dimensions). Click: shape preview",
        "Taille réelle mesurée sur le maillage (Affichage > Dimensions réelles). Clic : aperçu de la forme",
    ),
];
