use egui::{Align2, CursorIcon, Pos2, Ui};
use glam::{DVec2, DVec3};
use hp_app::i18n::TrTable;
use hp_app::state::AppState;
use hp_app::{Action, Severity, Tool, ToolFlow, ToolId, ToolMenuEntry, ViewInfo, ViewInput, ViewKind, t, tf};
use hp_geom::Aabb;
use hp_render::Overlay;

use crate::common::{
    BoxHandle, Mouse, Ov, Snap, box_screen_rect, clean, consume_plain_key, depth_axis, edit_box, fmt_len, fmt_size,
    footprint_box, fresh, hit_handle, paint_handle, paint_label, rgba,
};

const COLOR_TARGET: egui::Color32 = egui::Color32::from_rgb(255, 72, 56);
const COLOR_OTHER: egui::Color32 = egui::Color32::from_rgb(255, 150, 130);
const MAX_DRAWN: usize = 256;
const DEFAULT_HALF: f64 = 1024.0;

const MENU_NEW: u32 = 1;
const MENU_DELETE: u32 = 2;
const MENU_TOGGLE: u32 = 3;

#[derive(Clone, Copy, Debug, PartialEq)]
enum Drag {
    New { view: usize, kind: ViewKind, start: DVec2, depth: (f64, f64) },
    Edit { view: usize, kind: ViewKind, handle: BoxHandle, start_box: Aabb, from: DVec2 },
}

impl Drag {
    fn view(&self) -> usize {
        match *self {
            Drag::New { view, .. } | Drag::Edit { view, .. } => view,
        }
    }
}

#[derive(Debug, Default)]
pub struct CordonTool {
    drag: Option<Drag>,
    preview: Option<Aabb>,
    hover: Option<(usize, BoxHandle, u64)>,
    new_next: bool,
}

const MAX_SHOWN_NAME: usize = 96;

fn shown_name(name: &str) -> &str {
    match name.char_indices().nth(MAX_SHOWN_NAME) {
        Some((cut, _)) => &name[..cut],
        None => name,
    }
}

fn cordon_box(c: &hp_vmf::Cordon) -> Option<Aabb> {
    let b = Aabb::new(c.mins, c.maxs);
    (c.mins.is_finite() && c.maxs.is_finite() && b.size().min_element() > 1e-3).then_some(b)
}

pub fn target_cordon(state: &AppState) -> Option<usize> {
    let n = state.doc.scene().cordons().cordons.len();
    match state.runtime.visibility.edit_cordon {
        Some(i) if i < n => Some(i),
        _ => (n > 0).then_some(0),
    }
}

impl CordonTool {
    pub fn new() -> Self {
        Self::default()
    }

    pub fn creates_new(&self) -> bool {
        self.new_next
    }

    pub fn set_creates_new(&mut self, on: bool) {
        self.new_next = on;
    }

    fn current_box(&self, state: &AppState) -> Option<Aabb> {
        if self.drag.is_some() {
            return self.preview;
        }
        if self.new_next {
            return None;
        }
        let i = target_cordon(state)?;
        state.doc.scene().cordons().cordons.get(i).and_then(cordon_box)
    }

    fn depth_range(&self, state: &AppState, d: usize, snap: &Snap) -> (f64, f64) {
        if !self.new_next
            && let Some(b) =
                target_cordon(state).and_then(|i| state.doc.scene().cordons().cordons.get(i)).and_then(cordon_box)
        {
            return (b.min[d], b.max[d]);
        }
        let bounds = state.doc.selection_bounds(&|e| state.entity_bounds(e)).or_else(|| {
            let mut b = Aabb::EMPTY;
            for n in state.doc.scene().solids().values() {
                if !n.bounds.is_empty() && n.bounds.min.is_finite() && n.bounds.max.is_finite() {
                    b = b.union(&n.bounds);
                }
            }
            (!b.is_empty()).then_some(b)
        });
        match bounds.filter(|b| b.size()[d] > 1e-3 && b.min[d].is_finite() && b.max[d].is_finite()) {
            Some(b) => {
                let step = snap.step_along(d);
                ((snap.v(b.min[d], d) - step).min(b.min[d]), (snap.v(b.max[d], d) + step).max(b.max[d]))
            }
            None => (-DEFAULT_HALF, DEFAULT_HALF),
        }
    }

    fn commit(&mut self, state: &mut AppState, b: Aabb, new: bool) {
        let b = Aabb::new(b.min.map(clean), b.max.map(clean));
        let target = target_cordon(state);
        let result = match (new, target) {
            (false, Some(i)) => state.doc.set_cordon_box(i, b),
            _ => {
                let name = state.doc.unique_cordon_name(t!("create.cordon.new_name"));
                state.doc.add_cordon(&name, b).map(Some)
            }
        };
        match result {
            Ok(index) => {
                state.runtime.visibility.edit_cordon = index;
                self.new_next = false;
            }
            Err(e) => state.notify(Severity::Warning, tf!("create.cordon.failed", e)),
        }
    }

    fn pick_cordon(state: &mut AppState, view: &ViewInfo, pos: Pos2) -> bool {
        let mut best: Option<(usize, f32)> = None;
        for (i, c) in state.doc.scene().cordons().cordons.iter().enumerate().take(MAX_DRAWN) {
            let Some(rect) = cordon_box(c).and_then(|b| box_screen_rect(view, &b)) else { continue };
            if rect.contains(pos) && best.is_none_or(|(_, area)| rect.area() < area) {
                best = Some((i, rect.area()));
            }
        }
        let Some((i, _)) = best else { return false };
        state.runtime.visibility.edit_cordon = Some(i);
        true
    }

    pub(crate) fn handle_mouse(&mut self, state: &mut AppState, view: &ViewInfo, m: &Mouse) -> ToolFlow {
        if !view.is_2d() {
            return ToolFlow::Ignored;
        }
        let Some(pos) = m.pos else { return ToolFlow::Ignored };
        let snap = Snap::new(state, view, m.modifiers);
        if m.hovering() {
            self.hover = self
                .current_box(state)
                .and_then(|b| hit_handle(view, &b, pos))
                .map(|h| (view.index, h, state.frame.index));
            return ToolFlow::Ignored;
        }
        if m.clicked && self.drag.is_none() {
            Self::pick_cordon(state, view, pos);
            return ToolFlow::Handled;
        }
        if m.drag_started {
            let origin = m.origin().unwrap_or(pos);
            let existing = self.current_box(state);
            self.drag = match existing.and_then(|b| hit_handle(view, &b, origin).map(|h| (b, h))) {
                Some((b, handle)) => Some(Drag::Edit {
                    view: view.index,
                    kind: view.kind,
                    handle,
                    start_box: b,
                    from: view.screen_to_2d(origin),
                }),
                None => {
                    let depth = self.depth_range(state, depth_axis(view.kind), &snap);
                    Some(Drag::New {
                        view: view.index,
                        kind: view.kind,
                        start: snap.v2(view.screen_to_2d(origin)),
                        depth,
                    })
                }
            };
            self.preview = existing;
        }
        if m.dragging || m.drag_stopped {
            match self.drag {
                Some(Drag::New { view: vi, kind, start, depth }) if vi == view.index => {
                    self.preview = footprint_box(kind, start, snap.v2(view.screen_to_2d(pos)), depth);
                }
                Some(Drag::Edit { view: vi, kind, handle, start_box, from }) if vi == view.index => {
                    self.preview = Some(edit_box(&start_box, kind, handle, from, view.screen_to_2d(pos), &snap));
                    self.hover = Some((view.index, handle, state.frame.index));
                }
                _ => {}
            }
        }
        if m.drag_stopped && self.drag.is_some_and(|d| d.view() == view.index) {
            let drag = self.drag.take();
            let new = self.new_next;
            if let Some(b) = self.preview.take()
                && (matches!(drag, Some(Drag::Edit { start_box, .. }) if start_box != b)
                    || matches!(drag, Some(Drag::New { .. })))
            {
                self.commit(state, b, new);
            }
        }
        ToolFlow::Handled
    }

    fn paint_2d(&self, state: &AppState, view: &ViewInfo, painter: &egui::Painter) {
        let Some(b) = self.current_box(state) else { return };
        let Some(rect) = box_screen_rect(view, &b) else { return };
        let Some((r, u, _)) = view.kind.axes_2d() else { return };
        let size = b.size();
        paint_label(
            painter,
            Pos2::new(rect.center().x, rect.top() - 4.0),
            Align2::CENTER_BOTTOM,
            &fmt_len(size[r]),
            COLOR_TARGET,
        );
        paint_label(
            painter,
            Pos2::new(rect.left() - 6.0, rect.center().y),
            Align2::RIGHT_CENTER,
            &fmt_len(size[u]),
            COLOR_TARGET,
        );
        if self.drag.is_none()
            && let Some(c) = target_cordon(state).and_then(|i| state.doc.scene().cordons().cordons.get(i))
        {
            let label = if c.exclude {
                format!("{} ({})", shown_name(&c.name), t!("create.cordon.exclude"))
            } else {
                shown_name(&c.name).to_owned()
            };
            paint_label(painter, rect.left_bottom() + egui::vec2(0.0, 4.0), Align2::LEFT_TOP, &label, COLOR_TARGET);
        }
        let hovered = self.hover.filter(|(vi, _, frame)| *vi == view.index && fresh(state, *frame)).map(|(_, h, _)| h);
        for h in BoxHandle::RESIZE {
            paint_handle(painter, h.screen_pos(rect), hovered == Some(h));
        }
    }
}

impl Tool for CordonTool {
    fn id(&self) -> ToolId {
        ToolId::Cordon
    }

    fn deactivate(&mut self, state: &mut AppState) {
        self.cancel(state);
        self.new_next = false;
    }

    fn view_input(&mut self, state: &mut AppState, input: &ViewInput<'_>) -> ToolFlow {
        if input.navigating {
            return ToolFlow::Ignored;
        }
        let m = Mouse::from_input(input);
        self.handle_mouse(state, input.view, &m)
    }

    fn key_input(&mut self, state: &mut AppState, ctx: &egui::Context) -> bool {
        if (self.drag.is_some() || self.new_next) && consume_plain_key(ctx, egui::Key::Escape) {
            return self.cancel(state);
        }
        false
    }

    fn overlay(&self, state: &AppState, view: &ViewInfo, out: &mut Overlay) {
        let mut ov = Ov::new(out, view);
        let target = if self.new_next { None } else { target_cordon(state) };
        let cordons = state.doc.scene().cordons();
        for (i, c) in cordons.cordons.iter().enumerate().take(MAX_DRAWN) {
            if Some(i) == target {
                continue;
            }
            if let Some(b) = cordon_box(c) {
                ov.box_edges(&b, rgba(COLOR_OTHER, if c.active { 0.55 } else { 0.3 }), false);
            }
        }
        for extra in cordons.extra_boxes.iter().take(MAX_DRAWN) {
            let b = Aabb::new(extra.mins, extra.maxs);
            if extra.mins.is_finite() && extra.maxs.is_finite() && !b.is_empty() {
                ov.box_edges(&b, rgba(COLOR_OTHER, 0.3), false);
            }
        }
        let Some(b) = self.current_box(state) else { return };
        if !view.is_2d() {
            ov.box_faces(&b, rgba(COLOR_TARGET, 0.06));
        }
        ov.box_edges(&b, rgba(COLOR_TARGET, 1.0), true);
    }

    fn paint(&self, state: &AppState, view: &ViewInfo, painter: &egui::Painter) {
        if !view.is_2d() {
            return;
        }
        let painter = painter.with_clip_rect(view.rect.intersect(painter.clip_rect()));
        self.paint_2d(state, view, &painter);
    }

    fn options_ui(&mut self, ui: &mut Ui, state: &mut AppState) {
        ui.horizontal_wrapped(|ui| {
            let target = target_cordon(state);
            ui.label(t!("create.cordon.target"));
            let cordons = &state.doc.scene().cordons().cordons;
            let selected = match (self.new_next, target.and_then(|i| cordons.get(i))) {
                (true, _) => t!("create.cordon.new_pending").to_owned(),
                (false, Some(c)) => shown_name(&c.name).to_owned(),
                (false, None) => t!("create.cordon.none").to_owned(),
            };
            let mut chosen = None;
            egui::ComboBox::from_id_salt("hp_cordon_target").selected_text(selected).width(160.0).show_ui(ui, |ui| {
                for (i, c) in cordons.iter().enumerate().take(MAX_DRAWN) {
                    if ui.selectable_label(!self.new_next && target == Some(i), shown_name(&c.name)).clicked() {
                        chosen = Some(i);
                    }
                }
            });
            if let Some(i) = chosen {
                state.runtime.visibility.edit_cordon = Some(i);
                self.new_next = false;
            }
            if hp_app::widgets::icon_toggle(ui, "➕", self.new_next, t!("create.cordon.new_tip")).clicked() {
                self.new_next = !self.new_next;
            }
            if let Some(i) = target.filter(|_| !self.new_next)
                && let Some(c) = state.doc.scene().cordons().cordons.get(i).cloned()
            {
                let mut active = c.active;
                if ui
                    .checkbox(&mut active, t!("create.cordon.active"))
                    .on_hover_text(t!("create.cordon.active_tip"))
                    .changed()
                    && let Err(e) = state.doc.set_cordon_active(i, active)
                {
                    state.notify(Severity::Warning, tf!("create.cordon.failed", e));
                }
                let mut exclude = c.exclude;
                let include_resp = ui
                    .selectable_value(&mut exclude, false, t!("create.cordon.include"))
                    .on_hover_text(t!("create.cordon.include_tip"));
                let exclude_resp = ui
                    .selectable_value(&mut exclude, true, t!("create.cordon.exclude"))
                    .on_hover_text(t!("create.cordon.exclude_tip"));
                if (include_resp.changed() || exclude_resp.changed())
                    && let Err(e) = state.doc.set_cordon_exclude(i, exclude)
                {
                    state.notify(Severity::Warning, tf!("create.cordon.failed", e));
                }
                if let Some(b) = cordon_box(&c) {
                    ui.label(egui::RichText::new(fmt_size(&b)).weak());
                }
                if ui.small_button("✖").on_hover_text(t!("create.cordon.delete")).clicked() {
                    if let Err(e) = state.doc.remove_cordon(i) {
                        state.notify(Severity::Warning, tf!("create.cordon.failed", e));
                    }
                    state.runtime.visibility.edit_cordon = None;
                }
            }
            ui.separator();
            let mut on = state.doc.scene().cordons().active;
            if ui.checkbox(&mut on, Action::ToggleCordon.label()).on_hover_text(t!("create.cordon.use_tip")).changed() {
                state.request(Action::ToggleCordon);
            }
        });
    }

    fn cursor(&self, state: &AppState, view: &ViewInfo, pos: Pos2) -> Option<CursorIcon> {
        if !view.is_2d() {
            return None;
        }
        if let Some(Drag::Edit { handle, .. }) = self.drag {
            return Some(handle.cursor());
        }
        if self.drag.is_none()
            && let Some(h) = self.current_box(state).and_then(|b| hit_handle(view, &b, pos))
        {
            return Some(h.cursor());
        }
        Some(CursorIcon::Crosshair)
    }

    fn status_hint(&self, state: &AppState) -> Option<String> {
        let mut hint = if self.new_next {
            t!("create.cordon.hint_new").to_owned()
        } else {
            match target_cordon(state).and_then(|i| state.doc.scene().cordons().cordons.get(i)) {
                Some(c) => tf!("create.cordon.hint_edit", shown_name(&c.name)),
                None => t!("create.cordon.hint_new").to_owned(),
            }
        };
        if !state.doc.cordons_in_use() {
            hint.push(' ');
            hint.push_str(t!("create.cordon.hint_off"));
        }
        Some(hint)
    }

    fn cancel(&mut self, _state: &mut AppState) -> bool {
        self.hover = None;
        if self.drag.take().is_some() {
            self.preview = None;
            return true;
        }
        std::mem::take(&mut self.new_next)
    }

    fn context_entries(&self, state: &AppState) -> Vec<ToolMenuEntry> {
        let on = state.doc.scene().cordons().active;
        vec![
            ToolMenuEntry {
                label: t!("create.cordon.menu_new").to_owned(),
                id: MENU_NEW,
                enabled: !state.doc.selection().objects.is_empty(),
            },
            ToolMenuEntry {
                label: t!("create.cordon.delete").to_owned(),
                id: MENU_DELETE,
                enabled: target_cordon(state).is_some(),
            },
            ToolMenuEntry {
                label: if on { t!("create.cordon.menu_off") } else { Action::ToggleCordon.label() }.to_owned(),
                id: MENU_TOGGLE,
                enabled: true,
            },
        ]
    }

    fn context_command(&mut self, state: &mut AppState, id: u32) {
        match id {
            MENU_NEW => {
                let Some(b) = state.doc.selection_bounds(&|e| state.entity_bounds(e)) else { return };
                let grow = DVec3::from_array(
                    [0, 1, 2].map(|i| if b.size()[i] < 1.0 { state.grid_size().max(1.0) } else { 0.0 }),
                );
                self.commit(state, Aabb::new(b.min - grow, b.max + grow), true);
            }
            MENU_DELETE => {
                if let Some(i) = target_cordon(state) {
                    if let Err(e) = state.doc.remove_cordon(i) {
                        state.notify(Severity::Warning, tf!("create.cordon.failed", e));
                    }
                    state.runtime.visibility.edit_cordon = None;
                }
            }
            MENU_TOGGLE => state.request(Action::ToggleCordon),
            _ => {}
        }
    }
}

pub const CORDON_TR: TrTable = &[
    ("create.cordon.new_name", "Cordon", "Cordon"),
    ("create.cordon.failed", "Cordon not changed: {0}", "Cordon non modifié : {0}"),
    ("create.cordon.target", "Cordon:", "Cordon :"),
    ("create.cordon.none", "(none)", "(aucun)"),
    ("create.cordon.new_pending", "New cordon", "Nouveau cordon"),
    (
        "create.cordon.new_tip",
        "New cordon: the next box you draw becomes a new cordon",
        "Nouveau cordon : la prochaine boîte dessinée devient un nouveau cordon",
    ),
    ("create.cordon.active", "Active", "Actif"),
    (
        "create.cordon.active_tip",
        "Include this cordon in the cordoned area",
        "Inclure ce cordon dans la zone cordonnée",
    ),
    ("create.cordon.delete", "Delete this cordon", "Supprimer ce cordon"),
    ("create.cordon.include", "Include", "Inclure"),
    (
        "create.cordon.include_tip",
        "Include cordon: only what touches the include cordons is kept",
        "Cordon d'inclusion : seul ce qui touche les cordons d'inclusion est gardé",
    ),
    ("create.cordon.exclude", "Exclude", "Exclure"),
    (
        "create.cordon.exclude_tip",
        "Exclude cordon: what lies fully inside it is dimmed and left out of cordon compiles, everything else is kept",
        "Cordon d'exclusion : ce qui est entièrement dedans est estompé et retiré des compilations du cordon, tout le reste est gardé",
    ),
    (
        "create.cordon.use_tip",
        "While cordons are used, objects outside every active cordon are dimmed, cannot be picked and are left out of cordon compiles",
        "Quand les cordons sont utilisés, les objets en dehors de tous les cordons actifs sont estompés, ne peuvent pas être sélectionnés et sont exclus des compilations du cordon",
    ),
    (
        "create.cordon.hint_new",
        "Drag in a 2D view to draw a new cordon box.",
        "Glissez dans une vue 2D pour dessiner une nouvelle boîte de cordon.",
    ),
    (
        "create.cordon.hint_edit",
        "Cordon \"{0}\": drag its handles to resize it, inside to move it, elsewhere to redraw it. Click inside another cordon to edit it.",
        "Cordon « {0} » : glissez ses poignées pour le redimensionner, l'intérieur pour le déplacer, ailleurs pour le redessiner. Cliquez dans un autre cordon pour le modifier.",
    ),
    ("create.cordon.hint_off", "(Cordons are not in use.)", "(Les cordons ne sont pas utilisés.)"),
    ("create.cordon.menu_new", "New cordon around the selection", "Nouveau cordon autour de la sélection"),
    ("create.cordon.menu_off", "Stop using cordons", "Ne plus utiliser les cordons"),
];

#[cfg(test)]
mod tests {
    #![allow(clippy::unwrap_used, clippy::expect_used, clippy::panic)]

    use egui::Pos2;
    use glam::DVec3;
    use hp_app::testing::run_ui;
    use hp_app::{Tool, ViewKind};
    use hp_editor::ObjRef;
    use hp_render::Overlay;

    use super::*;
    use crate::common::Mouse;
    use crate::tests_support::{info_2d, info_3d, settle};

    fn state() -> AppState {
        let state = settle();
        hp_app::i18n::register(CORDON_TR);
        state
    }

    fn screen(view: &ViewInfo, p: DVec3) -> Pos2 {
        view.world_to_screen(p).expect("on screen")
    }

    fn drag(tool: &mut CordonTool, state: &mut AppState, view: &ViewInfo, a: Pos2, b: Pos2) {
        assert_eq!(tool.handle_mouse(state, view, &Mouse::drag_start(a, a)), ToolFlow::Handled);
        let mid = a + (b - a) * 0.5;
        tool.handle_mouse(state, view, &Mouse::drag(a, mid));
        tool.handle_mouse(state, view, &Mouse::drag(a, b));
        tool.handle_mouse(state, view, &Mouse::drag_end(a, b));
    }

    fn cordon(state: &AppState, i: usize) -> hp_vmf::Cordon {
        state.doc.scene().cordons().cordons[i].clone()
    }

    #[test]
    fn handles_resize_the_target_in_one_undo_step() {
        let mut state = state();
        let mut tool = CordonTool::new();
        let view = info_2d(ViewKind::Top, 0.25);
        assert_eq!(target_cordon(&state), Some(0));
        let corner = screen(&view, DVec3::new(1024.0, 1024.0, 0.0));
        let to = screen(&view, DVec3::new(500.0, 700.0, 0.0));
        let before = state.doc.history_len().0;
        drag(&mut tool, &mut state, &view, corner, to);
        let c = cordon(&state, 0);
        assert_eq!(c.maxs, DVec3::new(496.0, 704.0, 1024.0), "snapped to the 16 grid, depth kept");
        assert_eq!(c.mins, DVec3::splat(-1024.0));
        assert_eq!(state.doc.history_len().0, before + 1, "one undo step on release");
        assert!(state.doc.undo());
        assert_eq!(cordon(&state, 0).maxs, DVec3::splat(1024.0));
    }

    #[test]
    fn drawing_elsewhere_redraws_and_new_cordons_get_unique_names() {
        let mut state = state();
        let mut tool = CordonTool::new();
        let view = info_2d(ViewKind::Front, 0.25);
        let a = screen(&view, DVec3::new(0.0, 1100.0, 0.0));
        let b = screen(&view, DVec3::new(0.0, 1300.0, 256.0));
        drag(&mut tool, &mut state, &view, a, b);
        let c = cordon(&state, 0);
        assert_eq!((c.mins, c.maxs), (DVec3::new(-1024.0, 1104.0, 0.0), DVec3::new(1024.0, 1296.0, 256.0)));
        tool.set_creates_new(true);
        let top = info_2d(ViewKind::Top, 0.25);
        drag(
            &mut tool,
            &mut state,
            &top,
            screen(&top, DVec3::new(-200.0, -200.0, 0.0)),
            screen(&top, DVec3::new(-40.0, -60.0, 0.0)),
        );
        assert_eq!(state.doc.scene().cordons().cordons.len(), 2);
        assert_eq!(target_cordon(&state), Some(1));
        assert!(!tool.creates_new());
        let n = cordon(&state, 1);
        assert_eq!(n.name, "Cordon 2");
        assert!(n.active);
        assert_eq!((n.mins.z, n.maxs.z), (-32.0, 144.0));
        tool.set_creates_new(true);
        drag(
            &mut tool,
            &mut state,
            &top,
            screen(&top, DVec3::new(300.0, 300.0, 0.0)),
            screen(&top, DVec3::new(400.0, 400.0, 0.0)),
        );
        assert_eq!(cordon(&state, 2).name, "Cordon 3");
    }

    #[test]
    fn clicks_pick_a_cordon_and_escape_or_tiny_drags_change_nothing() {
        let mut state = state();
        state.doc.add_cordon("small", Aabb::new(DVec3::splat(100.0), DVec3::splat(200.0))).expect("add");
        state.runtime.visibility.edit_cordon = Some(0);
        let mut tool = CordonTool::new();
        let view = info_2d(ViewKind::Top, 0.25);
        tool.handle_mouse(&mut state, &view, &Mouse::click(screen(&view, DVec3::splat(150.0))));
        assert_eq!(target_cordon(&state), Some(1));
        let p = screen(&view, DVec3::new(-600.0, 900.0, 0.0));
        let revision = state.doc.revision();
        drag(&mut tool, &mut state, &view, p, p);
        assert_eq!(state.doc.revision(), revision);
        tool.handle_mouse(&mut state, &view, &Mouse::drag_start(p, p));
        tool.handle_mouse(&mut state, &view, &Mouse::drag(p, p + egui::vec2(80.0, 80.0)));
        assert!(tool.cancel(&mut state));
        tool.handle_mouse(&mut state, &view, &Mouse::drag_end(p, p + egui::vec2(80.0, 80.0)));
        assert_eq!(state.doc.revision(), revision);
        let v3 = info_3d(DVec3::new(-300.0, -300.0, 200.0), 45.0, -20.0);
        assert_eq!(tool.handle_mouse(&mut state, &v3, &Mouse::click(Pos2::new(400.0, 300.0))), ToolFlow::Ignored);
    }

    #[test]
    fn context_menu_and_drawing_without_cordons() {
        let mut state = state();
        let mut tool = CordonTool::new();
        state.doc.remove_cordon(0).expect("remove");
        assert_eq!(target_cordon(&state), None);
        let entries = tool.context_entries(&state);
        assert!(!entries[0].enabled && !entries[1].enabled);
        state.doc.set_selection(&[ObjRef::Solid(3)]);
        tool.context_command(&mut state, MENU_NEW);
        let c = cordon(&state, 0);
        assert_eq!((c.mins, c.maxs), (DVec3::new(-256.0, 256.0, 0.0), DVec3::new(256.0, 272.0, 128.0)));
        tool.context_command(&mut state, MENU_TOGGLE);
        assert_eq!(state.requested_actions, vec![Action::ToggleCordon]);
        tool.context_command(&mut state, MENU_DELETE);
        assert!(state.doc.scene().cordons().cordons.is_empty());
    }

    #[test]
    fn draws_in_every_view() {
        let mut state = state();
        state.doc.add_cordon("second", Aabb::new(DVec3::splat(100.0), DVec3::splat(200.0))).expect("add");
        state.doc.set_cordons_enabled(true).expect("on");
        let mut tool = CordonTool::new();
        let views = [
            info_3d(DVec3::new(-300.0, -300.0, 200.0), 45.0, -20.0),
            info_2d(ViewKind::Top, 0.25),
            info_2d(ViewKind::Front, 0.25),
            info_2d(ViewKind::Side, 0.25),
        ];
        for new in [false, true] {
            tool.set_creates_new(new);
            for v in &views {
                let mut out = Overlay::default();
                tool.overlay(&state, v, &mut out);
                assert!(!out.lines.is_empty() || !out.lines_on_top.is_empty());
                let _ = tool.cursor(&state, v, v.rect.center());
            }
            assert!(tool.status_hint(&state).is_some_and(|h| !h.is_empty()));
            run_ui(&mut state, |ui, state| {
                tool.options_ui(ui, state);
                for v in &views {
                    tool.paint(state, v, ui.painter());
                }
            });
        }
        for (k, _, _) in CORDON_TR {
            assert!(!hp_app::i18n::tr_in(k, hp_app::i18n::Lang::Fr).is_empty());
        }
    }

    #[test]
    fn translations_are_clean_and_used() {
        assert!(hp_app::i18n::lint_table(CORDON_TR).is_empty(), "{:#?}", hp_app::i18n::lint_table(CORDON_TR));
        let src = include_str!("cordon.rs");
        let code = src.split("pub const CORDON_TR").next().unwrap_or_default();
        for (k, en, fr) in CORDON_TR {
            assert!(k.starts_with("create.cordon."), "{k}");
            assert!(code.contains(&format!("\"{k}\"")), "unused key {k}");
            assert!(!crate::TR.iter().any(|(c, _, _)| c == k), "{k} is also in the crate table");
            for i in 0..4 {
                let p = format!("{{{i}}}");
                assert_eq!(en.contains(&p), fr.contains(&p), "{k}: placeholder {p}");
            }
        }
        assert!(!src.contains('\u{2014}') && !src.contains('\u{2013}'));
    }
}
