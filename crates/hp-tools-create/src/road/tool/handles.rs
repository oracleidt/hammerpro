use egui::{Modifiers, Pos2};
use glam::DVec3;
use hp_app::ViewInfo;
use hp_tools_select::gizmo::{GizmoLayout, GizmoMode, GizmoPart, WORLD_AXES};
use hp_tools_select::math::{closest_param_on_line, ray_plane};

use super::{Drag, MAX_POINTS, RoadPoint, RoadTool, Snap};

#[derive(Clone, Copy, Debug, PartialEq)]
pub(super) struct PointDrag {
    pub index: usize,
    pub part: GizmoPart,
    origin: DVec3,
    anchor: DVec3,
}

fn hit(view: &ViewInfo, pos: Pos2, origin: DVec3, part: GizmoPart) -> Option<DVec3> {
    let (o, d) = view.ray(pos);
    match part {
        GizmoPart::Axis(axis) => {
            let a = WORLD_AXES[axis % 3];
            closest_param_on_line(o, d, origin, a).map(|t| origin + a * t)
        }
        GizmoPart::Plane(axis) => ray_plane(o, d, origin, WORLD_AXES[axis % 3]),
        _ => None,
    }
}

impl RoadTool {
    pub(super) fn gizmo_layout(&self, view: &ViewInfo) -> Option<GizmoLayout> {
        let point = self.points.get(self.selected?)?;
        GizmoLayout::new(view, point.xyz())
    }

    pub(super) fn gizmo_at(&self, view: &ViewInfo, pos: Pos2) -> Option<GizmoPart> {
        if !view.rect.contains(pos) || self.point_at(view, pos).is_some() {
            return None;
        }
        self.gizmo_layout(view)?.hit(view, pos, GizmoMode::Move)
    }

    fn extrude_endpoint(&mut self, index: usize, modifiers: Modifiers) -> usize {
        if self.options.meta.layout != crate::road::RoadLayout::Path
            || !modifiers.shift
            || self.points.len() >= MAX_POINTS
            || self.points.len() < 2
        {
            return index;
        }
        if index == 0 {
            self.points.insert(0, self.points[0]);
        } else if index + 1 == self.points.len() {
            self.points.push(self.points[index]);
        } else {
            return index;
        }
        let index = if index == 0 { 0 } else { index + 1 };
        self.bump();
        index
    }

    pub(super) fn begin_point_drag(&mut self, view: &ViewInfo, pos: Pos2, modifiers: Modifiers) {
        let part = self.gizmo_at(view, pos);
        let index = if part.is_some() { self.selected } else { self.point_at(view, pos) };
        let Some(index) = index else {
            self.drag = Some(Drag::Swallow);
            return;
        };
        let part = part.or_else(|| (modifiers.ctrl && !view.is_2d()).then_some(GizmoPart::Axis(2)));
        let origin = self.points[index].xyz();
        let anchor = match part {
            Some(part) => match hit(view, pos, origin, part) {
                Some(anchor) => Some(anchor),
                None => {
                    self.drag = Some(Drag::Swallow);
                    return;
                }
            },
            None => None,
        };
        let index = self.extrude_endpoint(index, modifiers);
        self.selected = Some(index);
        self.hover = None;
        self.drag = Some(match (part, anchor) {
            (Some(part), Some(anchor)) => Drag::Gizmo(PointDrag { index, part, origin, anchor }),
            _ => Drag::Point(index),
        });
    }

    pub(super) fn drag_gizmo(&mut self, view: &ViewInfo, pos: Pos2, snap: &Snap, drag: PointDrag) {
        let Some(current) = hit(view, pos, drag.origin, drag.part) else { return };
        let mut point = drag.origin + current - drag.anchor;
        let mut moves = [true; 3];
        match drag.part {
            GizmoPart::Axis(axis) => {
                moves = [false; 3];
                moves[axis % 3] = true;
            }
            GizmoPart::Plane(axis) => moves[axis % 3] = false,
            _ => return,
        }
        for axis in 0..3 {
            point[axis] = if moves[axis] { snap.v(point[axis], axis) } else { drag.origin[axis] };
        }
        let p = RoadPoint::new(point.x, point.y, point.z);
        if !p.is_valid() || self.points.get(drag.index).is_none_or(|old| *old == p) {
            return;
        }
        if (point.z - drag.origin.z).abs() > 1e-6 {
            self.options.meta.follow_terrain = false;
        }
        self.points[drag.index] = p;
        self.bump();
    }
}

#[cfg(test)]
mod tests {
    #![allow(clippy::unwrap_used, clippy::expect_used)]

    use super::*;
    use crate::common::Mouse;
    use crate::tests_support::{info_2d, info_3d, settle};
    use hp_app::{Tool, ViewKind};

    fn tool() -> RoadTool {
        let mut tool = RoadTool::new();
        tool.add_point(RoadPoint::new(0.0, 0.0, 0.0));
        tool.add_point(RoadPoint::new(800.0, 0.0, 0.0));
        tool.selected = Some(0);
        tool
    }

    fn perspective() -> ViewInfo {
        info_3d(DVec3::new(600.0, -900.0, 600.0), 123.6900675, -29.0171406)
    }

    #[test]
    fn blue_arrow_raises_a_point_in_3d_without_moving_xy_or_changing_the_map() {
        let mut state = settle();
        let mut tool = tool();
        tool.options.meta.follow_terrain = true;
        let view = perspective();
        let layout = tool.gizmo_layout(&view).unwrap();
        let anchor = DVec3::Z * layout.size * 0.75;
        let from = view.world_to_screen(anchor).unwrap();
        let to = view.world_to_screen(anchor + DVec3::Z * 128.0).unwrap();
        let revision = state.doc.revision();
        assert_eq!(tool.gizmo_at(&view, from), Some(GizmoPart::Axis(2)));
        tool.handle_mouse(&mut state, &view, &Mouse::drag_start(from, to));
        tool.handle_mouse(&mut state, &view, &Mouse::drag_end(from, to));
        assert_eq!(tool.points[0], RoadPoint::new(0.0, 0.0, 128.0));
        assert_eq!(tool.points[1], RoadPoint::new(800.0, 0.0, 0.0));
        assert!(!tool.options.meta.follow_terrain);
        assert!(tool.preview(&state).is_ok());
        assert_eq!(state.doc.revision(), revision);
    }

    #[test]
    fn shift_drag_extends_either_end_without_moving_the_existing_points() {
        for index in [0, 1] {
            let mut state = settle();
            let mut tool = tool();
            let view = info_2d(ViewKind::Top, 0.5);
            let origin = tool.points[index].xyz();
            let from = view.world_to_screen(origin).unwrap();
            let target = origin + DVec3::X * if index == 0 { -256.0 } else { 256.0 };
            let to = view.world_to_screen(target).unwrap();
            tool.handle_mouse(&mut state, &view, &Mouse::drag_start(from, to).with_modifiers(Modifiers::SHIFT));
            tool.handle_mouse(&mut state, &view, &Mouse::drag_end(from, to).with_modifiers(Modifiers::SHIFT));
            assert_eq!(tool.points.len(), 3);
            let added = if index == 0 { 0 } else { 2 };
            assert_eq!(tool.points[added].xyz(), target);
            assert!(tool.points.iter().any(|p| p.xyz() == origin));
            assert!(tool.preview(&state).is_ok());
        }
    }

    #[test]
    fn xy_plane_handle_preserves_elevation_and_alt_disables_snapping() {
        let mut state = settle();
        let mut tool = tool();
        tool.points[0].z = 35.0;
        let view = perspective();
        let origin = tool.points[0].xyz();
        let part = GizmoPart::Plane(2);
        let anchor = origin + DVec3::new(20.0, 20.0, 0.0);
        let target = anchor + DVec3::new(18.25, -9.5, 0.0);
        let pos = view.world_to_screen(target).unwrap();
        let drag = PointDrag { index: 0, part, origin, anchor };
        let snap = Snap::new(&state, &view, Modifiers::ALT);
        tool.drag_gizmo(&view, pos, &snap, drag);
        assert!((tool.points[0].x - 18.25).abs() < 0.001);
        assert!((tool.points[0].y + 9.5).abs() < 0.001);
        assert_eq!(tool.points[0].z, 35.0);
        tool.cancel(&mut state);
        assert!(tool.points.is_empty());
    }

    #[test]
    fn activating_road_on_a_selected_road_reopens_it_and_cancel_keeps_the_original() {
        let mut state = settle();
        let mut original = tool();
        let made = original.create(&mut state).unwrap();
        let revision = state.doc.revision();
        let mut editing = RoadTool::new();
        editing.activate(&mut state);
        assert!(editing.is_editing());
        assert_eq!(editing.points.len(), 2);
        editing.set_point_height(1, 256.0);
        editing.sync_preview(&mut state);
        editing.cancel(&mut state);
        assert_eq!(state.doc.revision(), revision);
        let (_, meta) = crate::road::apply::read_road(&state.doc, made.ids[0]).unwrap();
        assert_eq!(meta.points[1].z, 0.0);
    }

    #[test]
    fn height_drag_through_a_real_response_is_applied_on_release() {
        use egui::{Event, PointerButton, RawInput, Sense};
        let ctx = egui::Context::default();
        let mut state = settle();
        let mut tool = tool();
        let mut view = perspective();
        let origin = DVec3::ZERO;
        let anchor = DVec3::Z * tool.gizmo_layout(&view).unwrap().size * 0.75;
        let from = view.world_to_screen(anchor).unwrap();
        let to = view.world_to_screen(anchor + DVec3::Z * 160.0).unwrap();
        let pointer = |pos, pressed| Event::PointerButton {
            pos,
            button: PointerButton::Primary,
            pressed,
            modifiers: Modifiers::NONE,
        };
        for events in [
            vec![Event::PointerMoved(from)],
            vec![pointer(from, true)],
            vec![Event::PointerMoved(to)],
            vec![pointer(to, false)],
        ] {
            let _ = ctx.run_ui(RawInput { screen_rect: Some(view.rect), events, ..Default::default() }, |ui| {
                let response = ui.allocate_rect(view.rect, Sense::click_and_drag());
                view.rect = response.rect;
                let input = hp_app::ViewInput {
                    view: &view,
                    response: &response,
                    pointer: ctx.pointer_hover_pos(),
                    modifiers: Modifiers::NONE,
                    navigating: false,
                };
                tool.view_input(&mut state, &input);
            });
        }
        assert_eq!(tool.points[0].xyz(), origin + DVec3::Z * 160.0);
    }
}
