use std::cell::Cell;
use std::rc::Rc;

use egui::{Event, Key, Modifiers, PointerButton, Pos2, RawInput, vec2};
use hp_app::state::{PickKind, PickRequest, PickResult};
use hp_app::testing::test_state;
use hp_app::view::Camera3d;
use hp_app::{ToolFlow, ToolMenuEntry};
use hp_editor::FaceRef;

use super::*;

#[derive(Default)]
struct HookTool {
    inputs: usize,
    middle_clicks: Vec<usize>,
    wheel_handled: bool,
    middle_drag: bool,
    wheels: Vec<(usize, f32, Modifiers)>,
    entries: Vec<ToolMenuEntry>,
    commands: Vec<u32>,
    cursors: Cell<usize>,
}

impl Tool for HookTool {
    fn id(&self) -> ToolId {
        ToolId::Block
    }

    fn view_input(&mut self, _state: &mut AppState, input: &ViewInput<'_>) -> ToolFlow {
        self.inputs += 1;
        if input.response.middle_clicked() {
            self.middle_clicks.push(input.view.index);
        }
        ToolFlow::Handled
    }

    fn wheel(&mut self, _state: &mut AppState, view: &ViewInfo, notches: f32, modifiers: Modifiers) -> ToolFlow {
        self.wheels.push((view.index, notches, modifiers));
        if self.wheel_handled { ToolFlow::Handled } else { ToolFlow::Ignored }
    }

    fn takes_middle_drag(&self, kind: ViewKind, _modifiers: Modifiers) -> bool {
        self.middle_drag && kind == ViewKind::Perspective
    }

    fn context_entries(&self, _state: &AppState) -> Vec<ToolMenuEntry> {
        self.entries.clone()
    }

    fn context_command(&mut self, _state: &mut AppState, id: u32) {
        self.commands.push(id);
    }

    fn cursor(&self, _state: &AppState, _view: &ViewInfo, _pos: Pos2) -> Option<CursorIcon> {
        self.cursors.set(self.cursors.get() + 1);
        Some(CursorIcon::Default)
    }
}

struct H {
    ctx: egui::Context,
    vp: Viewports,
    state: AppState,
    tool: HookTool,
    time: f64,
    modifiers: Modifiers,
    cursor: Option<CursorIcon>,
}

impl H {
    fn new() -> Self {
        register_translations();
        let mut h = Self {
            ctx: egui::Context::default(),
            vp: Viewports::new(),
            state: test_state(),
            tool: HookTool::default(),
            time: 0.0,
            modifiers: Modifiers::NONE,
            cursor: None,
        };
        h.frame(vec![]);
        h.frame(vec![]);
        h
    }

    fn frame(&mut self, mut events: Vec<Event>) {
        events.insert(0, Event::ModifiersChanged(self.modifiers));
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
        self.cursor = Some(out.platform_output.cursor_icon);
    }

    fn rect(&self, i: usize) -> Rect {
        match self.vp.view_rects()[i] {
            Some((_, r)) => r,
            None => panic!("view {i} is not visible"),
        }
    }

    fn view(&self, i: usize) -> ViewInfo {
        let kind = self.vp.view_rects()[i].map_or(ViewKind::Perspective, |v| v.0);
        view_info(i, kind, self.rect(i), &self.state.cameras, &RenderConfig::from_state(&self.state), 1.0)
    }

    fn button(&mut self, p: Pos2, button: PointerButton, pressed: bool) {
        self.frame(vec![
            Event::PointerMoved(p),
            Event::PointerButton { pos: p, button, pressed, modifiers: self.modifiers },
        ]);
    }

    fn click(&mut self, p: Pos2, button: PointerButton) {
        self.frame(vec![Event::PointerMoved(p)]);
        self.button(p, button, true);
        self.button(p, button, false);
    }

    fn key(&mut self, key: Key) {
        self.frame(vec![Event::Key {
            key,
            physical_key: None,
            pressed: true,
            repeat: false,
            modifiers: self.modifiers,
        }]);
        self.frame(vec![Event::Key {
            key,
            physical_key: None,
            pressed: false,
            repeat: false,
            modifiers: self.modifiers,
        }]);
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

    fn request(&mut self, kind: PickKind) {
        self.state.pick = Some(PickRequest { kind, requester: "test:key".to_owned(), result: None });
        self.frame(vec![]);
    }

    fn result(&self) -> Option<PickResult> {
        self.state.pick.as_ref().and_then(|p| p.result.clone())
    }

    fn look_down_at(&mut self, x: f64, y: f64) {
        self.state.cameras.cam3d =
            Camera3d { pos: DVec3::new(x, y, 300.0), yaw_deg: 0.0, pitch_deg: -89.0, ..Camera3d::default() };
    }
}

#[test]
fn the_wheel_goes_to_the_tool_first() {
    let mut h = H::new();
    let p = h.rect(1).center();
    let zoom = h.state.cameras.top.zoom;
    h.wheel(p, 1.0);
    assert_eq!(h.tool.wheels, vec![(1, 1.0, Modifiers::NONE)]);
    assert!(h.state.cameras.top.zoom > zoom);
    h.tool.wheel_handled = true;
    h.modifiers = Modifiers::ALT;
    let zoom = h.state.cameras.top.zoom;
    h.wheel(p, -2.0);
    assert_eq!(h.tool.wheels.last(), Some(&(1, -2.0, Modifiers::ALT)));
    assert_eq!(h.state.cameras.top.zoom, zoom);
    h.modifiers = Modifiers::NONE;
    let q = h.rect(0).center();
    h.frame(vec![Event::PointerMoved(q)]);
    h.button(q, PointerButton::Secondary, true);
    let speed = h.state.settings.view.camera_speed;
    let asked = h.tool.wheels.len();
    h.wheel(q, 1.0);
    assert_eq!(h.tool.wheels.len(), asked);
    assert!(h.state.settings.view.camera_speed > speed, "the wheel sped the camera up");
    h.wheel(q, -2.0);
    assert!(h.state.settings.view.camera_speed < speed, "and back slows it down");
    h.button(q, PointerButton::Secondary, false);
}

#[test]
fn a_tool_can_take_middle_mouse_without_panning_or_looking() {
    for scheme in hp_app::settings::NavScheme::ALL {
        let mut h = H::new();
        h.state.settings.workflow.nav_scheme = scheme;
        h.tool.middle_drag = true;
        let p = h.rect(0).center();
        let before = h.state.cameras.cam3d;
        h.button(p, PointerButton::Middle, true);
        h.frame(vec![Event::PointerMoved(p + vec2(3.0, 2.0))]);
        h.button(p + vec2(3.0, 2.0), PointerButton::Middle, false);
        assert_eq!(h.state.cameras.cam3d, before, "{scheme:?}");
        assert_eq!(h.tool.middle_clicks, vec![0], "{scheme:?}");
        assert!(!h.vp.is_context_menu_open());
    }
}

#[test]
fn tool_entries_come_first_in_the_context_menu() {
    let mut h = H::new();
    h.tool.entries = vec![
        ToolMenuEntry { label: "Create object".to_owned(), id: 7, enabled: true },
        ToolMenuEntry { label: "Create as cylinder".to_owned(), id: 8, enabled: false },
    ];
    let p = h.rect(1).center_top() + vec2(0.0, 60.0);
    h.click(p, PointerButton::Secondary);
    h.frame(vec![]);
    let Some(menu) = h.vp.menu else { panic!("the menu is open") };
    let first = menu.pos + vec2(40.0, 14.0);
    h.click(first, PointerButton::Primary);
    h.frame(vec![]);
    assert_eq!(h.tool.commands, vec![7]);
    assert!(!h.vp.is_context_menu_open(), "the menu closes after a choice");
    assert!(h.state.requested_actions.is_empty(), "no application action ran");
    h.vp.run_menu_choice(&mut h.state, &menu, MenuChoice::Tool(8));
    assert!(h.state.requested_actions.is_empty());
}

#[test]
fn a_middle_click_reaches_the_tool_a_middle_drag_pans() {
    let mut h = H::new();
    let p = h.rect(2).center();
    let center = h.state.cameras.front.center;
    h.click(p, PointerButton::Middle);
    assert_eq!(h.tool.middle_clicks, vec![2]);
    assert_eq!(h.state.cameras.front.center, center);
    assert!(!h.vp.is_navigating());
    h.frame(vec![Event::PointerMoved(p)]);
    h.button(p, PointerButton::Middle, true);
    h.frame(vec![Event::PointerMoved(p + vec2(30.0, 0.0))]);
    h.button(p + vec2(30.0, 0.0), PointerButton::Middle, false);
    assert_eq!(h.tool.middle_clicks, vec![2]);
    assert_ne!(h.state.cameras.front.center, center);
}

#[test]
fn pick_entities_in_3d_and_2d() {
    let mut h = H::new();
    h.request(PickKind::Entity);
    let inputs = h.tool.inputs;
    let cursors = h.tool.cursors.get();
    h.look_down_at(0.0, 0.0);
    let c = h.rect(0).center();
    h.click(c, PointerButton::Primary);
    assert_eq!(h.result(), Some(PickResult::Entity(5)));
    assert_eq!(h.tool.inputs, inputs, "the tool gets nothing while a pick is pending");
    assert_eq!(h.tool.cursors.get(), cursors, "the picker sets the cursor");
    assert_eq!(h.cursor, Some(CursorIcon::Crosshair));
    h.frame(vec![Event::PointerMoved(c + vec2(1.0, 0.0))]);
    assert!(h.tool.inputs > inputs);

    h.request(PickKind::Entity);
    h.look_down_at(-128.0, -200.0);
    h.click(c, PointerButton::Primary);
    assert_eq!(h.result(), None);
    assert!(pick::hint(&h.state, 0).is_some_and(|t| t.contains("Esc")));
    assert_eq!(h.vp.pick_hint(&h.state), pick::hint(&h.state, 0));

    let top = h.view(1);
    let Some(start) = top.world_to_screen(DVec3::new(0.0, -128.0, 0.0)) else { panic!("visible") };
    h.click(start, PointerButton::Primary);
    assert_eq!(h.result(), Some(PickResult::Entity(4)));
    h.request(PickKind::Entity);
    let Some(edge) = top.world_to_screen(DVec3::new(64.0, 32.0, 0.0)) else { panic!("visible") };
    h.click(edge, PointerButton::Primary);
    assert_eq!(h.result(), Some(PickResult::Entity(6)));

    h.request(PickKind::Entity);
    h.key(Key::Escape);
    assert_eq!(h.result(), Some(PickResult::Cancelled));
    assert_eq!(h.vp.pick_hint(&h.state), None, "no hint once answered");
}

#[test]
fn pick_faces_with_ctrl_and_enter() {
    let mut h = H::new();
    h.look_down_at(-128.0, -200.0);
    let c = h.rect(0).center();
    let floor = FaceRef::new(2, 0);
    h.request(PickKind::Faces);
    h.modifiers = Modifiers::CTRL;
    h.click(c, PointerButton::Primary);
    h.modifiers = Modifiers::NONE;
    assert_eq!(h.result(), None);
    assert_eq!(h.vp.picker.faces(), &[floor]);
    assert!(pick::hint(&h.state, 1).is_some_and(|t| t.contains('1')));
    h.key(Key::Enter);
    assert_eq!(h.result(), Some(PickResult::Faces(vec![(2, 0)])));
    assert!(h.vp.picker.faces().is_empty());
    h.request(PickKind::Faces);
    h.click(c, PointerButton::Primary);
    assert_eq!(h.result(), Some(PickResult::Faces(vec![(2, 0)])));
    h.request(PickKind::Faces);
    h.modifiers = Modifiers::CTRL;
    h.click(c, PointerButton::Primary);
    h.click(c, PointerButton::Primary);
    h.modifiers = Modifiers::NONE;
    assert!(h.vp.picker.faces().is_empty());
    h.key(Key::Enter);
    assert_eq!(h.result(), None);
    let p = h.rect(1).center();
    h.click(p, PointerButton::Primary);
    assert_eq!(h.result(), None);
    h.modifiers = Modifiers::CTRL;
    h.click(c, PointerButton::Primary);
    h.modifiers = Modifiers::NONE;
    h.state.pick = Some(PickRequest { kind: PickKind::Faces, requester: "other".to_owned(), result: None });
    h.frame(vec![]);
    assert!(h.vp.picker.faces().is_empty());
    h.key(Key::Escape);
    assert_eq!(h.result(), Some(PickResult::Cancelled));
}

#[test]
fn pick_points_and_angles() {
    let mut h = H::new();
    h.request(PickKind::Point);
    let top = h.view(1);
    let Some(p) = top.world_to_screen(DVec3::new(32.0, 48.0, 0.0)) else { panic!("visible") };
    h.click(p, PointerButton::Primary);
    assert_eq!(h.result(), Some(PickResult::Point(DVec3::new(32.0, 48.0, 0.0))));
    h.request(PickKind::Angles);
    h.look_down_at(-128.0, -200.0);
    let c = h.rect(0).center();
    h.click(c, PointerButton::Primary);
    let Some(PickResult::Point(q)) = h.result() else { panic!("a point") };
    assert!(q.z.abs() < 1e-6, "{q:?}");
}

#[derive(Default)]
struct CountingLayer {
    adds: Rc<Cell<usize>>,
    paints: Rc<Cell<usize>>,
    version: Rc<Cell<Option<u64>>>,
    fills: Rc<Cell<usize>>,
}

impl OverlayLayer for CountingLayer {
    fn name(&self) -> &'static str {
        "counting"
    }

    fn add(&self, _state: &AppState, _view: &ViewInfo, out: &mut Overlay) {
        self.adds.set(self.adds.get() + 1);
        out.points.push(hp_render::OverlayPoint { pos: [0.0; 3], color: [1.0; 4], size_px: 3.0 });
    }

    fn paint(&self, _state: &AppState, _view: &ViewInfo, _painter: &egui::Painter) {
        self.paints.set(self.paints.get() + 1);
    }

    fn retained_version(&self, _state: &AppState, _view: &ViewInfo) -> Option<u64> {
        self.version.get()
    }

    fn retained(&self, _state: &AppState, _view: &ViewInfo, out: &mut Overlay) {
        self.fills.set(self.fills.get() + 1);
        out.lines.push(hp_render::OverlayLine { a: [0.0; 3], b: [1.0; 3], color: [1.0; 4] });
    }
}

#[test]
fn layers_draw_in_every_view_and_retain_with_the_tool() {
    let mut h = H::new();
    let layer = CountingLayer::default();
    let (adds, paints, version, fills) =
        (layer.adds.clone(), layer.paints.clone(), layer.version.clone(), layer.fills.clone());
    h.vp.add_layer(Box::new(layer));
    let names = h.vp.layer_names();
    assert_eq!(names.first(), Some(&"pointfile"));
    assert_eq!(names.last(), Some(&"counting"));
    h.frame(vec![]);
    assert_eq!(adds.get(), 4, "once per visible view");
    assert_eq!(paints.get(), 4);
    let view = h.view(0);
    assert!(!h.vp.refill_retained(&h.state, &h.tool, &view), "no retained geometry yet");
    version.set(Some(1));
    assert!(h.vp.refill_retained(&h.state, &h.tool, &view));
    assert_eq!((fills.get(), h.vp.retained_overlay.lines.len()), (1, 1));
    assert!(!h.vp.refill_retained(&h.state, &h.tool, &view));
    version.set(Some(2));
    assert!(h.vp.refill_retained(&h.state, &h.tool, &view));
    assert_eq!(fills.get(), 2);
    version.set(None);
    assert!(h.vp.refill_retained(&h.state, &h.tool, &view), "cleared once");
    assert!(h.vp.retained_overlay.is_empty());
    assert!(!h.vp.refill_retained(&h.state, &h.tool, &view));
}
