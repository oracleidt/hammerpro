use std::cell::Cell;

use glam::DVec2;
use hp_app::settings::SelectElement;
use hp_app::tool::ToolOption;
use hp_app::{AppState, ViewKind};

use super::SelectTool;
use crate::gizmo::GizmoMode;
use crate::math::HandleMode;

thread_local! {
    static SETTINGS_SNAPSHOT: Cell<(bool, bool, SelectElement, bool)> =
        const { Cell::new((true, false, SelectElement::Object, true)) };
}

pub fn sync_options(state: &AppState) {
    let s = &state.settings.selection;
    SETTINGS_SNAPSHOT.with(|c| c.set((s.stretch_handles, s.gizmo_local, s.element, s.box_confirm)));
}

fn on_off(b: bool) -> &'static str {
    if b { "on" } else { "off" }
}

fn flag(v: &str) -> Option<bool> {
    match v.to_ascii_lowercase().as_str() {
        "on" | "true" | "1" | "yes" => Some(true),
        "off" | "false" | "0" | "no" => Some(false),
        _ => None,
    }
}

fn kind_name(k: ViewKind) -> &'static str {
    match k {
        ViewKind::Top => "top",
        ViewKind::Front => "front",
        ViewKind::Side => "side",
        ViewKind::Perspective => "3d",
    }
}

fn handle_mode_name(m: HandleMode) -> &'static str {
    match m {
        HandleMode::Scale => "scale",
        HandleMode::Rotate => "rotate",
        HandleMode::Skew => "skew",
    }
}

fn gizmo_mode_name(m: GizmoMode) -> &'static str {
    match m {
        GizmoMode::Move => "move",
        GizmoMode::Rotate => "rotate",
        GizmoMode::Scale => "scale",
    }
}

impl SelectTool {
    pub(super) fn set_named_option(&mut self, state: &mut AppState, key: &str, value: &str) -> bool {
        let v = value.trim().to_ascii_lowercase();
        match key {
            "handles" => {
                let Some(m) = HandleMode::ALL.into_iter().find(|m| handle_mode_name(*m) == v) else { return false };
                self.handle_mode = m;
            }
            "box" => match v.as_str() {
                "touching" | "inside" => {
                    self.box_inside = v == "inside";
                    self.centers_only = false;
                }
                "centers" => self.centers_only = true,
                _ => return false,
            },
            "element" => {
                let Some(e) = SelectElement::from_id(&v) else { return false };
                state.set_pick_element(e);
            }
            "gizmo" => match flag(&v) {
                Some(b) => self.gizmo_visible = b,
                None => return false,
            },
            "gizmo_mode" => {
                let Some(m) = GizmoMode::ALL.into_iter().find(|m| gizmo_mode_name(*m) == v) else { return false };
                self.gizmo_mode = m;
            }
            "gizmo_local" | "stretch_handles" | "box_confirm" => {
                let Some(b) = flag(&v) else { return false };
                let s = &mut state.settings.selection;
                match key {
                    "gizmo_local" => s.gizmo_local = b,
                    "stretch_handles" => s.stretch_handles = b,
                    _ => {
                        s.box_confirm = b;
                        if !b {
                            self.pending = None;
                        }
                    }
                }
                state.settings_dirty = true;
            }
            "pending_box" => match v.as_str() {
                "confirm" | "add" => {
                    if !self.confirm_pending_box(state, v == "add") {
                        return false;
                    }
                }
                "cancel" | "none" => {
                    self.pending = None;
                }
                _ => {
                    let nums: Vec<f64> = v
                        .split(|c: char| c == ',' || c.is_whitespace())
                        .filter(|t| !t.is_empty())
                        .filter_map(|t| t.parse().ok())
                        .collect();
                    let [ax, ay, bx, by] = nums[..] else { return false };
                    let Some((view, kind)) = self.box_view(state) else { return false };
                    if !self.set_pending_box(view, kind, DVec2::new(ax, ay), DVec2::new(bx, by)) {
                        return false;
                    }
                }
            },
            _ => return false,
        }
        self.hover = super::Hover::default();
        sync_options(state);
        true
    }

    pub(super) fn named_option_values(&self) -> Vec<ToolOption> {
        let (stretch, local, element, confirm) = SETTINGS_SNAPSHOT.with(Cell::get);
        let pending = match self.pending_box() {
            Some((kind, a, b)) => format!("{} {} {} {} {}", kind_name(kind), a.x, a.y, b.x, b.y),
            None => "none".to_owned(),
        };
        let boxed = if self.centers_only {
            "centers"
        } else if self.box_inside {
            "inside"
        } else {
            "touching"
        };
        vec![
            ToolOption::new(
                "element",
                element.id(),
                "object, face, edge or vertex: what a click in a 3D view selects. face fills the face selection \
                 (the inset and extrude keys use it), edge and vertex pick the edge or corner of a brush under \
                 the cursor (the bevel and vertex tools take them over). Shortcuts 4, 3, 2, 1.",
            ),
            ToolOption::new(
                "handles",
                handle_mode_name(self.handle_mode),
                "scale, rotate or skew: what the handles of the selection box do in the 2D views.",
            ),
            ToolOption::new(
                "box",
                boxed,
                "touching, inside or centers: which objects a box selection in a 2D view takes.",
            ),
            ToolOption::new(
                "box_confirm",
                on_off(confirm),
                "on/off: like Hammer++, a box drawn in a 2D view stays after release with handles to resize it \
                 and a drag inside to move it, Enter selects (Ctrl+Enter adds), Esc cancels; off selects on release \
                 (saved with the settings).",
            ),
            ToolOption::new(
                "pending_box",
                pending,
                "the box waiting for Enter: none, or its view (top, front, side) and corners on the two axes of \
                 that view. Set it to four numbers \"x1 y1 x2 y2\" to draw a box in the 2D view under the pointer \
                 (or the first 2D view), confirm to select it, add to add it to the selection, cancel to drop it.",
            ),
            ToolOption::new("gizmo", on_off(self.gizmo_visible), "on/off: show the transform gizmo in the 3D view."),
            ToolOption::new(
                "gizmo_mode",
                gizmo_mode_name(self.gizmo_mode),
                "move, rotate or scale: what the gizmo does.",
            ),
            ToolOption::new(
                "gizmo_local",
                on_off(local),
                "on/off: the gizmo of a single point entity follows its angles (saved with the settings).",
            ),
            ToolOption::new(
                "stretch_handles",
                on_off(stretch),
                "on/off: an arrow on each face of the selected brushes in the 3D view stretches the selection \
                 along its axis, the opposite face staying (saved with the settings).",
            ),
        ]
    }
}
