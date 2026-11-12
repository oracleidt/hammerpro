use egui::{Event, Modifiers, PointerButton, Pos2, RawInput, vec2};
use glam::DVec2;
use hp_app::testing::test_state;

use super::*;

#[derive(Default)]
struct DragTool {
    start: Option<DVec2>,
    end: Option<DVec2>,
    frames: usize,
    seen: Vec<(usize, DVec2)>,
}

impl Tool for DragTool {
    fn id(&self) -> ToolId {
        ToolId::Block
    }

    fn view_input(&mut self, _state: &mut AppState, input: &ViewInput<'_>) -> ToolFlow {
        let r = input.response;
        let Some(pos) = input.pointer else { return ToolFlow::Ignored };
        if r.drag_started_by(PointerButton::Primary) {
            let origin = r.ctx.input(|i| i.pointer.press_origin()).unwrap_or(pos);
            self.start = Some(snap16(input.view.screen_to_2d(origin)));
        }
        if r.dragged_by(PointerButton::Primary) || r.drag_stopped_by(PointerButton::Primary) {
            let end = snap16(input.view.screen_to_2d(pos));
            self.end = Some(end);
            self.frames += 1;
            self.seen.push((input.view.index, input.view.cam2d.center));
        }
        ToolFlow::Handled
    }
}

fn snap16(p: DVec2) -> DVec2 {
    (p / 16.0).round() * 16.0
}

struct H {
    ctx: egui::Context,
    vp: Viewports,
    state: AppState,
    tool: DragTool,
    time: f64,
}

impl H {
    fn new(autopan: bool) -> Self {
        register_translations();
        let mut state = test_state();
        state.settings.workflow.drag_autopan = autopan;
        let mut h =
            Self { ctx: egui::Context::default(), vp: Viewports::new(), state, tool: DragTool::default(), time: 0.0 };
        h.frame(vec![]);
        h.frame(vec![]);
        h
    }

    fn frame(&mut self, events: Vec<Event>) {
        self.time += 1.0 / 60.0;
        self.state.frame.dt = 1.0 / 60.0;
        let input = RawInput {
            screen_rect: Some(egui::Rect::from_min_size(Pos2::ZERO, vec2(1600.0, 900.0))),
            time: Some(self.time),
            events,
            focused: true,
            ..Default::default()
        };
        let mut out = self.ctx.run_ui(input, |ui| {
            egui::CentralPanel::default().show(ui, |ui| self.vp.ui(ui, &mut self.state, &mut self.tool));
        });
        out.textures_delta.clear();
    }

    fn view_2d(&self, kind: ViewKind) -> (usize, Rect) {
        self.vp
            .view_rects()
            .iter()
            .enumerate()
            .find_map(|(i, v)| v.filter(|(k, _)| *k == kind).map(|(_, r)| (i, r)))
            .expect("2D view visible")
    }

    fn info(&self, i: usize) -> ViewInfo {
        let (kind, rect) = self.vp.view_rects()[i].expect("visible");
        view_info(i, kind, rect, &self.state.cameras, &RenderConfig::from_state(&self.state), 1.0)
    }

    fn press(&mut self, p: Pos2) {
        self.frame(vec![Event::PointerMoved(p)]);
        self.frame(vec![Event::PointerButton {
            pos: p,
            button: PointerButton::Primary,
            pressed: true,
            modifiers: Modifiers::NONE,
        }]);
    }

    fn move_to(&mut self, p: Pos2) {
        self.frame(vec![Event::PointerMoved(p)]);
    }

    fn release(&mut self, p: Pos2) {
        self.frame(vec![Event::PointerButton {
            pos: p,
            button: PointerButton::Primary,
            pressed: false,
            modifiers: Modifiers::NONE,
        }]);
    }
}

fn drag_past_the_edge(kind: ViewKind, autopan: bool) -> (H, usize, Pos2) {
    let mut h = H::new(autopan);
    let (i, rect) = h.view_2d(kind);
    let start = rect.center();
    h.press(start);
    for k in 1..=6 {
        h.move_to(start + vec2(20.0 * k as f32, 0.0));
    }
    let outside = Pos2::new(rect.right() + 60.0, rect.bottom() + 30.0);
    h.move_to(outside);
    (h, i, outside)
}

#[test]
fn dragging_past_the_edge_scrolls_every_2d_view_and_the_drag_follows() {
    for kind in [ViewKind::Top, ViewKind::Front, ViewKind::Side] {
        let (mut h, i, outside) = drag_past_the_edge(kind, true);
        let initial = h.state.cameras;
        let before = h.state.cameras.cam2d(kind).center;
        let start = h.tool.start.expect("drag started");
        for _ in 0..30 {
            h.frame(vec![]);
        }
        let after = h.state.cameras.cam2d(kind).center;
        assert!(after.x > before.x + 1.0, "{kind:?}: scrolled right {before:?} -> {after:?}");
        assert!(after.y < before.y - 1.0, "{kind:?}: scrolled down {before:?} -> {after:?}");
        let view = h.info(i);
        let under = snap16(view.screen_to_2d(outside));
        assert_eq!(h.tool.end, Some(under), "{kind:?}: the dragged point stays under the cursor");
        assert_eq!(h.tool.start, Some(start), "{kind:?}: the start keeps its world position");
        assert!(h.tool.seen.iter().all(|(v, _)| *v == i), "{kind:?}: only the dragged view");
        let last = h.tool.seen.last().map(|(_, c)| *c);
        assert_eq!(last, Some(after), "{kind:?}: the tool saw the scrolled camera");
        h.release(outside);
        let stopped = h.state.cameras.cam2d(kind).center;
        for _ in 0..10 {
            h.frame(vec![]);
        }
        assert_eq!(h.state.cameras.cam2d(kind).center, stopped, "{kind:?}: stops with the drag");
        for other in [ViewKind::Top, ViewKind::Front, ViewKind::Side] {
            if other != kind {
                assert_eq!(
                    h.state.cameras.cam2d(other).center,
                    initial.cam2d(other).center,
                    "{kind:?} moved {other:?}"
                );
            }
        }
    }
}

#[test]
fn scrolling_speeds_up_with_the_distance() {
    let mut speeds = Vec::new();
    for past in [5.0, 80.0, 300.0] {
        let mut h = H::new(true);
        let (_, rect) = h.view_2d(ViewKind::Top);
        let start = rect.center();
        h.press(start);
        for k in 1..=6 {
            h.move_to(start + vec2(20.0 * k as f32, 0.0));
        }
        h.move_to(Pos2::new(rect.right() + past, start.y));
        let before = h.state.cameras.top.center;
        for _ in 0..20 {
            h.frame(vec![]);
        }
        let moved = h.state.cameras.top.center - before;
        assert!(moved.y.abs() < 1e-9, "{moved:?}");
        speeds.push(moved.x);
    }
    assert!(speeds[0] > 0.0 && speeds[0] < speeds[1] && speeds[1] < speeds[2], "{speeds:?}");
}

#[test]
fn off_by_default_and_quiet_inside_the_view() {
    assert!(!hp_app::settings::WorkflowSettings::default().drag_autopan);
    let (mut h, _, _) = drag_past_the_edge(ViewKind::Top, false);
    let before = h.state.cameras.top.center;
    for _ in 0..20 {
        h.frame(vec![]);
    }
    assert_eq!(h.state.cameras.top.center, before, "the setting is off");

    let mut h = H::new(true);
    let (_, rect) = h.view_2d(ViewKind::Top);
    let start = rect.center();
    h.press(start);
    for k in 1..=6 {
        h.move_to(start + vec2(20.0 * k as f32, 10.0 * k as f32));
    }
    let before = h.state.cameras.top.center;
    for _ in 0..20 {
        h.frame(vec![]);
    }
    assert_eq!(h.state.cameras.top.center, before, "no scrolling away from the edges");

    let mut h = H::new(true);
    let (_, rect) = h.view_2d(ViewKind::Top);
    let before = h.state.cameras.top.center;
    h.move_to(Pos2::new(rect.right() - 2.0, rect.center().y));
    for _ in 0..20 {
        h.frame(vec![]);
    }
    assert_eq!(h.state.cameras.top.center, before, "hovering the edge without a drag does nothing");
}

#[test]
fn the_3d_view_never_scrolls() {
    let mut h = H::new(true);
    let (_, rect) = h.view_2d(ViewKind::Perspective);
    let cams = h.state.cameras;
    let start = rect.center();
    h.press(start);
    for k in 1..=6 {
        h.move_to(start + vec2(20.0 * k as f32, 0.0));
    }
    h.move_to(Pos2::new(rect.right() + 50.0, start.y));
    for _ in 0..20 {
        h.frame(vec![]);
    }
    assert_eq!(h.state.cameras.cam3d.pos, cams.cam3d.pos);
    for kind in [ViewKind::Top, ViewKind::Front, ViewKind::Side] {
        assert_eq!(h.state.cameras.cam2d(kind).center, cams.cam2d(kind).center);
    }
}
