use glam::DVec3;
use hp_app::i18n::TrTable;
use hp_app::{Action, Severity, ViewKind, t, tf};
use hp_editor::{AlignMode, Id, ObjRef};

use super::Package;
use crate::app::HammerProApp;

pub(crate) struct Selection;

#[derive(Default)]
pub(crate) struct State {}

const HANDLED: &[Action] = &[
    Action::PasteSpecial,
    Action::ArrayDuplicate,
    Action::SnapToGrid,
    Action::TransformDialog,
    Action::FlipX,
    Action::FlipY,
    Action::FlipZ,
    Action::AlignLeft,
    Action::AlignRight,
    Action::AlignTop,
    Action::AlignBottom,
    Action::MoveToFloor,
    Action::MoveToCeiling,
    Action::MoveToFloorAligned,
    Action::MoveToCeilingAligned,
    Action::SnapToGridIndividually,
    Action::CenterOrigins,
    Action::TogglePivot,
    Action::FlipHorizontal,
    Action::FlipVertical,
    Action::AlignCenterHorizontal,
    Action::AlignCenterVertical,
    Action::ToggleGizmoLocal,
    Action::RandomRotate,
    Action::RandomRotateDialog,
];

impl Package for Selection {
    fn handled(&self) -> &'static [Action] {
        HANDLED
    }

    fn handle(&self, app: &mut HammerProApp, _ctx: &egui::Context, action: Action) -> bool {
        match action {
            Action::PasteSpecial => app.state.ui.show_paste_special = true,
            Action::ArrayDuplicate => app.state.ui.show_array_duplicate = true,
            Action::SnapToGrid | Action::SnapToGridIndividually => {
                if app.require_selection() {
                    let objs = app.selected();
                    let grid = app.state.grid_size();
                    let each = action == Action::SnapToGridIndividually;
                    let state = &mut app.state;
                    let assets = &state.assets;
                    let result = state.doc.snap_to_grid(&objs, grid, each, &|e| assets.entity_bounds(e));
                    app.report(action, result);
                }
            }
            Action::TransformDialog => app.state.ui.show_transform = true,
            Action::RandomRotate => app.random_rotate(),
            Action::RandomRotateDialog => {
                app.state.ui.show_transform = true;
                app.state.ui.transform_random = true;
            }
            Action::FlipX => app.flip(0, action),
            Action::FlipY => app.flip(1, action),
            Action::FlipZ => app.flip(2, action),
            Action::FlipHorizontal | Action::FlipVertical => {
                let axis = app.view_flip_axis(action == Action::FlipVertical);
                app.flip(axis, action);
            }
            Action::AlignLeft | Action::AlignRight | Action::AlignTop | Action::AlignBottom => app.align(action),
            Action::AlignCenterHorizontal | Action::AlignCenterVertical => app.align(action),
            Action::MoveToFloor | Action::MoveToCeiling | Action::MoveToFloorAligned | Action::MoveToCeilingAligned => {
                app.drop_to_surface(action);
            }
            Action::CenterOrigins => app.center_origins(),
            Action::TogglePivot => app.toggle_pivot(),
            Action::ToggleGizmoLocal => {
                let s = &mut app.state.settings.selection;
                s.gizmo_local = !s.gizmo_local;
                app.state.settings_dirty = true;
            }
            _ => return false,
        }
        true
    }

    fn translations(&self) -> TrTable {
        TR
    }

    fn tick(&self, app: &mut HammerProApp, _ctx: &egui::Context) {
        hp_tools_select::sync_options(&app.state);
    }
}

const TR: TrTable = &[
    ("selpkg.nothing_below", "There is no surface below the selection", "Aucune surface sous la sélection"),
    ("selpkg.nothing_above", "There is no surface above the selection", "Aucune surface au-dessus de la sélection"),
    (
        "selpkg.random_rotated",
        "Random Z rotation: {0} turned, {1} skipped (brushes, brush entities or no angles key)",
        "Rotation Z aléatoire : {0} tournés, {1} ignorés (brosses, entités de brosses ou sans clé angles)",
    ),
    (
        "selpkg.random_none",
        "Random Z rotation: nothing selected has an angles key ({0} skipped)",
        "Rotation Z aléatoire : rien de sélectionné n'a de clé angles ({0} ignorés)",
    ),
    (
        "selpkg.no_origin",
        "No selected brush entity has an origin to center",
        "Aucune entité de brosses sélectionnée n'a d'origine à centrer",
    ),
];

impl HammerProApp {
    fn command_view(&self) -> Option<ViewKind> {
        let index = self.state.ui.hovered_view.or(self.state.ui.focused_view)?;
        hp_app::QuadrantView::from_index(self.state.settings.ui.quadrants.get(index).copied().unwrap_or(1)).kind()
    }

    fn view_flip_axis(&self, vertical: bool) -> usize {
        match self.command_view() {
            Some(kind) if kind.is_2d() => match kind.axes_2d() {
                Some((r, u, _)) => {
                    if vertical {
                        u
                    } else {
                        r
                    }
                }
                None => 0,
            },
            Some(_) if !vertical => {
                let right = self.state.cameras.cam3d.right();
                if right.x.abs() >= right.y.abs() { 0 } else { 1 }
            }
            Some(_) => 2,
            None => {
                if vertical {
                    1
                } else {
                    0
                }
            }
        }
    }

    fn flip(&mut self, axis: usize, action: Action) {
        if !self.require_selection() {
            return;
        }
        let objs = self.selected();
        let center = self
            .state
            .pivot
            .filter(|p| p.is_finite())
            .or_else(|| self.state.doc.selection_center(&self.entity_bounds_fn()))
            .unwrap_or(DVec3::ZERO);
        let result = self.state.doc.flip(&objs, axis, center);
        self.report(action, result);
    }

    fn align(&mut self, action: Action) {
        let objs = self.selected();
        if objs.len() < 2 {
            self.state.notify(Severity::Info, t!("app.need_two"));
            return;
        }
        let view = self.command_view().filter(|k| k.is_2d()).unwrap_or(ViewKind::Top);
        let Some((right, up, _)) = view.axes_2d() else { return };
        let (axis, mode) = match action {
            Action::AlignLeft => (right, AlignMode::Min),
            Action::AlignRight => (right, AlignMode::Max),
            Action::AlignTop => (up, AlignMode::Max),
            Action::AlignCenterHorizontal => (right, AlignMode::Center),
            Action::AlignCenterVertical => (up, AlignMode::Center),
            _ => (up, AlignMode::Min),
        };
        let reference_obj = objs[objs.len() - 1];
        let reference = self.state.doc.object_bounds(reference_obj, &self.entity_bounds_fn());
        let Some(reference) = reference else { return };
        let others: Vec<ObjRef> = objs[..objs.len() - 1].to_vec();
        let state = &mut self.state;
        let assets = &state.assets;
        let result = state.doc.align_objects(&others, &reference, axis, mode, false, &|e| assets.entity_bounds(e));
        self.report(action, result);
    }

    pub(crate) fn random_rotate(&mut self) {
        if !self.require_selection() {
            return;
        }
        let objs = self.selected();
        let opts = self.state.settings.selection.random_rotate(hp_app::settings::SelectionSettings::random_seed());
        let result = self.state.doc.random_rotate(&objs, &opts);
        if let Some(r) = self.report(Action::RandomRotate, result) {
            let text = if r.rotated == 0 {
                tf!("selpkg.random_none", r.skipped)
            } else {
                tf!("selpkg.random_rotated", r.rotated, r.skipped)
            };
            self.state.notify(Severity::Info, text);
        }
    }

    fn drop_to_surface(&mut self, action: Action) {
        if !self.require_selection() {
            return;
        }
        let up = matches!(action, Action::MoveToCeiling | Action::MoveToCeilingAligned);
        let align = matches!(action, Action::MoveToFloorAligned | Action::MoveToCeilingAligned);
        let objs = self.selected();
        let state = &mut self.state;
        let assets = &state.assets;
        let dir = if up { DVec3::Z } else { DVec3::NEG_Z };
        let result = state.doc.drop_to_surface(&objs, dir, align, &|e| assets.entity_bounds(e));
        if let Some(0) = self.report(action, result) {
            let text = if up { t!("selpkg.nothing_above") } else { t!("selpkg.nothing_below") };
            self.state.notify(Severity::Info, text);
        }
    }

    fn center_origins(&mut self) {
        let scene = self.state.doc.scene();
        let mut ids: Vec<Id> = Vec::new();
        for o in self.selected() {
            let id = match o {
                ObjRef::Entity(id) => id,
                ObjRef::Solid(s) => match scene.object_of_solid(s) {
                    ObjRef::Entity(id) => id,
                    ObjRef::Solid(_) => continue,
                },
            };
            if !ids.contains(&id) {
                ids.push(id);
            }
        }
        let result = self.state.doc.center_origins(&ids);
        if let Some(0) = self.report(Action::CenterOrigins, result) {
            self.state.notify(Severity::Info, t!("selpkg.no_origin"));
        }
    }

    fn toggle_pivot(&mut self) {
        if self.state.pivot.take().is_some() {
            return;
        }
        let objs = self.selected();
        let scene = self.state.doc.scene();
        let single_origin = match objs.as_slice() {
            [ObjRef::Entity(id)] if !scene.is_brush_entity(*id) => {
                scene.entity(*id).and_then(|n| n.entity.origin()).filter(|o| o.is_finite())
            }
            _ => None,
        };
        self.state.pivot = single_origin.or_else(|| self.state.doc.selection_center(&self.entity_bounds_fn()));
    }
}
