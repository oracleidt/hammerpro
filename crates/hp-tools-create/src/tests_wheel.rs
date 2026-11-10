#![allow(clippy::unwrap_used, clippy::expect_used, clippy::panic)]

use egui::{Event, Modifiers, PointerButton, Pos2, RawInput, Rect, vec2};
use glam::DVec3;
use hp_app::state::AppState;
use hp_app::{Tool, ViewKind};
use hp_ui_viewports::Viewports;

use crate::block::{BlockTool, Primitive};
use crate::pipe::PipeTool;

struct Harness<T: Tool> {
    ctx: egui::Context,
    vp: Viewports,
    state: AppState,
    tool: T,
    time: f64,
    modifiers: Modifiers,
}

impl<T: Tool> Harness<T> {
    fn new(tool: T) -> Self {
        hp_ui_viewports::register_translations();
        let mut h = Self {
            ctx: egui::Context::default(),
            vp: Viewports::new(),
            state: crate::tests_support::settle(),
            tool,
            time: 0.0,
            modifiers: Modifiers::NONE,
        };
        h.state.doc.clear_selection();
        h.frame(vec![]);
        h.frame(vec![]);
        h
    }

    fn frame(&mut self, mut events: Vec<Event>) {
        events.insert(0, Event::ModifiersChanged(self.modifiers));
        self.time += 1.0 / 60.0;
        self.state.frame.dt = 1.0 / 60.0;
        self.state.frame.index += 1;
        let input = RawInput {
            screen_rect: Some(Rect::from_min_size(Pos2::ZERO, vec2(1600.0, 900.0))),
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

    fn rect(&self, i: usize) -> Rect {
        match self.vp.view_rects()[i] {
            Some((_, r)) => r,
            None => panic!("view {i} is not visible"),
        }
    }

    fn top_point(&self, x: f64, y: f64) -> Pos2 {
        let rect = self.rect(1);
        let cam = self.state.cameras.top;
        let view = hp_app::ViewInfo {
            index: 1,
            kind: ViewKind::Top,
            rect,
            cam3d: self.state.cameras.cam3d,
            cam2d: cam,
            pixels_per_point: 1.0,
            grid: 16.0,
        };
        view.world_to_screen(DVec3::new(x, y, 0.0)).expect("on screen")
    }

    fn button(&mut self, p: Pos2, pressed: bool) {
        self.frame(vec![
            Event::PointerMoved(p),
            Event::PointerButton { pos: p, button: PointerButton::Primary, pressed, modifiers: self.modifiers },
        ]);
    }

    fn move_to(&mut self, p: Pos2) {
        self.frame(vec![Event::PointerMoved(p)]);
    }

    fn wheel(&mut self, p: Pos2, notches: f32) {
        self.frame(vec![
            Event::PointerMoved(p),
            Event::MouseWheel {
                unit: egui::MouseWheelUnit::Line,
                delta: vec2(0.0, notches),
                phase: egui::TouchPhase::Move,
                source: egui::MouseWheelSource::Wheel,
                modifiers: self.modifiers,
            },
        ]);
    }
}

#[test]
fn the_block_tool_takes_the_wheel_from_the_zoom_only_while_a_round_shape_is_drawn() {
    let mut tool = BlockTool::new();
    tool.options.kind = Primitive::Cylinder;
    let mut h = Harness::new(tool);
    let a = h.top_point(0.0, 0.0);
    let c = h.top_point(96.0, 64.0);
    let zoom = h.state.cameras.top.zoom;
    h.wheel(a, 1.0);
    assert!(h.state.cameras.top.zoom > zoom, "nothing drawn: the wheel zooms");
    assert_eq!(h.tool.options.sides, 8);
    let a = h.top_point(0.0, 0.0);
    let c2 = h.top_point(96.0, 64.0);
    assert_ne!(c, c2, "the zoom moved the points");
    h.move_to(a);
    h.button(a, true);
    for k in 1..=6 {
        h.move_to(a + (c2 - a) * (k as f32 / 6.0));
    }
    let zoom = h.state.cameras.top.zoom;
    h.wheel(c2, 2.0);
    assert_eq!(h.tool.options.sides, 10, "the wheel adds sides during the drag");
    assert_eq!(h.state.cameras.top.zoom, zoom, "and the view does not zoom");
    h.button(c2, false);
    assert!(h.tool.pending_box().is_some());
    h.modifiers = Modifiers::SHIFT;
    h.wheel(c2, -1.0);
    assert_eq!(h.tool.options.sides, 10 - crate::block::WHEEL_FAST_STEP, "Shift steps faster");
    assert_eq!(h.state.cameras.top.zoom, zoom);
    h.modifiers = Modifiers::NONE;
    let persp = h.rect(0).center();
    let cam = h.state.cameras.cam3d;
    h.wheel(persp, 1.0);
    assert_eq!(h.tool.options.sides, 7, "the 3D view changes the count too");
    assert_eq!(h.state.cameras.cam3d.pos, cam.pos, "without moving the camera");
    h.tool.options.kind = Primitive::Block;
    h.wheel(c2, 1.0);
    assert!(h.state.cameras.top.zoom > zoom, "a block has no count: the wheel zooms");
    h.tool.options.kind = Primitive::Cylinder;
    let tool = &mut h.tool;
    assert!(tool.cancel(&mut h.state));
    let zoom = h.state.cameras.top.zoom;
    h.wheel(c2, 1.0);
    assert!(h.state.cameras.top.zoom > zoom, "cancelled: the wheel zooms again");
    assert_eq!(h.tool.options.sides, 7, "the count is kept for the next shape");
}

#[test]
fn the_pipe_tool_takes_shift_wheel_for_its_sides_while_a_path_is_drawn() {
    let _lang = hp_app::testing::lang_lock();
    let mut pipe = PipeTool::new();
    pipe.options.spec.shape = crate::pipe::Shape::Round;
    let mut h = Harness::new(pipe);
    let zoom = h.state.cameras.top.zoom;
    let a = h.top_point(0.0, 0.0);
    h.wheel(a, 1.0);
    assert!(h.state.cameras.top.zoom > zoom, "no path: the wheel zooms");
    let sides = h.tool.spec().sides;
    let a = h.top_point(0.0, 0.0);
    h.move_to(a);
    h.button(a, true);
    h.button(a, false);
    assert_eq!(h.tool.points().len(), 1);
    let zoom = h.state.cameras.top.zoom;
    h.wheel(a, 1.0);
    assert!(h.state.cameras.top.zoom > zoom, "a path is placed by clicks: the plain wheel still zooms");
    assert_eq!(h.tool.spec().sides, sides);
    h.modifiers = Modifiers::SHIFT;
    let zoom = h.state.cameras.top.zoom;
    h.wheel(a, 1.0);
    assert_eq!(h.tool.spec().sides, sides + 1);
    assert_eq!(h.state.cameras.top.zoom, zoom, "Shift+wheel changes the sides instead of zooming");
    h.wheel(a, -100.0);
    assert_eq!(h.tool.spec().sides, crate::pipe::MIN_SIDES);
    h.tool.options.spec.shape = crate::pipe::Shape::Square;
    let slices = h.tool.spec().bend_slices;
    h.wheel(a, 2.0);
    assert_eq!(h.tool.spec().bend_slices, slices + 2, "a square pipe has bend slices");
    assert!(h.tool.status_hint(&h.state).is_some_and(|t| t.contains("Shift")));
}
