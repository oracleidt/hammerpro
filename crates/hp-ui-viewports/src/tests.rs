use std::cell::Cell;

use egui::{Event, Key, Modifiers, PointerButton, Pos2, RawInput, vec2};
use hp_app::testing::{run_ui, test_state};
use hp_app::view::{Camera2d, Camera3d};
use hp_app::{ToolFlow, ToolId};

use super::*;

#[derive(Default)]
struct RecordingTool {
    id: ToolId,
    inputs: Vec<(usize, bool, bool)>,
    secondary_clicks: Vec<(usize, Modifiers)>,
    keys: usize,
    overlays: Cell<usize>,
    paints: Cell<usize>,
    cursors: Cell<usize>,
    right_drag: bool,
    fly_clicks: Vec<(usize, Pos2, Modifiers)>,
}

impl Tool for RecordingTool {
    fn id(&self) -> ToolId {
        self.id
    }

    fn view_input(&mut self, _state: &mut AppState, input: &ViewInput<'_>) -> ToolFlow {
        self.inputs.push((input.view.index, input.navigating, input.pointer.is_some()));
        if input.response.secondary_clicked() {
            self.secondary_clicks.push((input.view.index, input.modifiers));
        }
        ToolFlow::Handled
    }

    fn click_while_flying(&mut self, _state: &mut AppState, view: &ViewInfo, pos: Pos2, mods: Modifiers) -> ToolFlow {
        self.fly_clicks.push((view.index, pos, mods));
        ToolFlow::Handled
    }

    fn key_input(&mut self, _state: &mut AppState, _ctx: &egui::Context) -> bool {
        self.keys += 1;
        false
    }

    fn overlay(&self, _state: &AppState, _view: &ViewInfo, out: &mut Overlay) {
        self.overlays.set(self.overlays.get() + 1);
        out.points.push(hp_render::OverlayPoint { pos: [0.0; 3], color: [1.0; 4], size_px: 4.0 });
    }

    fn paint(&self, _state: &AppState, _view: &ViewInfo, painter: &egui::Painter) {
        self.paints.set(self.paints.get() + 1);
        painter.circle_filled(painter.clip_rect().center(), 2.0, Color32::WHITE);
    }

    fn cursor(&self, _state: &AppState, _view: &ViewInfo, _pos: Pos2) -> Option<CursorIcon> {
        self.cursors.set(self.cursors.get() + 1);
        Some(CursorIcon::Crosshair)
    }

    fn takes_right_drag(&self, kind: ViewKind, _modifiers: Modifiers) -> bool {
        self.right_drag && kind == ViewKind::Perspective
    }
}

struct Harness {
    ctx: egui::Context,
    vp: Viewports,
    state: AppState,
    tool: RecordingTool,
    time: f64,
    modifiers: Modifiers,
    last_output: Option<egui::PlatformOutput>,
}

impl Harness {
    fn new() -> Self {
        register_translations();
        let mut h = Self {
            ctx: egui::Context::default(),
            vp: Viewports::new(),
            state: test_state(),
            tool: RecordingTool::default(),
            time: 0.0,
            modifiers: Modifiers::NONE,
            last_output: None,
        };
        h.frame(vec![]);
        h.frame(vec![]);
        h
    }

    fn frame(&mut self, mut events: Vec<Event>) {
        events.insert(0, Event::ModifiersChanged(self.modifiers));
        self.time += 1.0 / 60.0;
        self.state.frame.dt = 1.0 / 60.0;
        self.state.frame.fps = 60.0;
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
        self.last_output = Some(out.platform_output);
    }

    fn rect(&self, i: usize) -> Rect {
        match self.vp.view_rects()[i] {
            Some((_, r)) => r,
            None => panic!("view {i} is not visible"),
        }
    }

    fn move_to(&mut self, p: Pos2) {
        self.frame(vec![Event::PointerMoved(p)]);
    }

    fn button(&mut self, p: Pos2, button: PointerButton, pressed: bool) {
        self.frame(vec![
            Event::PointerMoved(p),
            Event::PointerButton { pos: p, button, pressed, modifiers: self.modifiers },
        ]);
    }

    fn key(&mut self, key: Key, physical: Option<Key>, pressed: bool) {
        self.frame(vec![Event::Key { key, physical_key: physical, pressed, repeat: false, modifiers: self.modifiers }]);
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

    fn cursor_icon(&self) -> Option<CursorIcon> {
        self.last_output.as_ref().map(|o| o.cursor_icon)
    }
}

#[test]
fn smoke_with_testing_helpers() {
    register_translations();
    let mut state = test_state();
    let mut vp = Viewports::new();
    let mut tool = RecordingTool::default();
    run_ui(&mut state, |ui, state| vp.ui(ui, state, &mut tool));
    assert!(vp.view_rects().iter().all(Option::is_some));
    assert_eq!(tool.paints.get(), 3 * 4, "every visible view is painted once per frame");
    assert_eq!(tool.overlays.get(), 3 * 4);
    assert!(tool.inputs.is_empty(), "no pointer, no tool input");
    state.ui.maximized_view = Some(2);
    run_ui(&mut state, |ui, state| vp.ui(ui, state, &mut tool));
    let rects = vp.view_rects();
    assert_eq!(rects.iter().filter(|r| r.is_some()).count(), 1);
    assert!(rects[2].is_some());
    state.ui.maximized_view = Some(9);
    run_ui(&mut state, |ui, state| vp.ui(ui, state, &mut tool));
    assert_eq!(state.ui.maximized_view, None);
    state.pointfile = Some(hp_app::state::Pointfile {
        path: PathBuf::from("leak.lin"),
        points: vec![DVec3::ZERO, DVec3::new(64.0, 0.0, 0.0), DVec3::new(64.0, 64.0, 32.0)],
        cursor: 0,
    });
    state.settings.view.show_3d_grid = true;
    state.settings.view.show_fps = true;
    for mode in [RenderModeSetting::Flat, RenderModeSetting::Wireframe, RenderModeSetting::Lit] {
        state.settings.view.render_mode = mode;
        run_ui(&mut state, |ui, state| vp.ui(ui, state, &mut tool));
    }
}

#[test]
fn layout_follows_settings() {
    let mut h = Harness::new();
    let r0 = h.rect(0);
    let r1 = h.rect(1);
    let r3 = h.rect(3);
    assert!(r0.right() < r1.left() && r0.bottom() < r3.top());
    assert_eq!(h.vp.view_rects()[0].map(|v| v.0), Some(ViewKind::Perspective));
    assert_eq!(h.vp.view_rects()[3].map(|v| v.0), Some(ViewKind::Side));
    for i in 1..4 {
        let s = layout::pixel_size(h.rect(i), 1.0);
        assert_eq!((s[0] % 2, s[1] % 2), (1, 1), "view {i}: {s:?}");
    }
    h.state.settings.ui.quadrants[0] = 1;
    h.frame(vec![]);
    assert_eq!(h.vp.view_rects()[0].map(|v| v.0), Some(ViewKind::Top));
    assert_eq!(h.vp.view_size(ViewKind::Top), Some(h.rect(0).size()));
    h.state.settings.ui.split_x = 0.25;
    h.frame(vec![]);
    assert!(h.rect(0).width() < h.rect(1).width());
}

#[test]
fn hover_routes_to_tool_and_sets_world_position() {
    let mut h = Harness::new();
    let c = h.rect(1).center() + vec2(13.0, 7.0);
    h.move_to(c);
    h.frame(vec![]);
    assert_eq!(h.state.ui.hovered_view, Some(1));
    let Some(w) = h.state.hover_world else { panic!("hover position must be set in 2D") };
    assert_eq!(w.z, 0.0, "depth comes from the selection center or 0");
    assert_eq!(w.x % 16.0, 0.0, "snapped: {w:?}");
    assert_eq!(w.y % 16.0, 0.0, "snapped: {w:?}");
    let last = h.tool.inputs.last().copied();
    assert_eq!(last, Some((1, false, true)));
    assert!(h.tool.inputs.iter().all(|(v, _, _)| *v == 1), "only the hovered view gets input");
    assert_eq!(h.tool.keys, 0, "the binary calls key_input after the UI pass, never the viewports");
    assert!(h.tool.cursors.get() > 0);
    assert_eq!(h.cursor_icon(), Some(CursorIcon::Crosshair));
    let (view, pos) = h.vp.hovered_view(&h.state, &h.ctx).expect("the hovered view and pointer");
    assert_eq!((view.index, view.kind, view.rect), (1, h.vp.view_rects()[1].expect("view").0, h.rect(1)));
    assert_eq!(pos, c);
    h.move_to(Pos2::new(1599.0, 899.0));
    h.frame(vec![]);
    assert_eq!(h.state.ui.hovered_view, None);
    assert_eq!(h.state.hover_world, None);
    assert!(h.vp.hovered_view(&h.state, &h.ctx).is_none());
}

#[test]
fn hover_world_uses_selection_depth_in_2d() {
    let mut h = Harness::new();
    h.state.doc.select(&[hp_editor::ObjRef::Entity(5)], hp_editor::SelectOp::Replace);
    let c = h.rect(1).center();
    h.move_to(c);
    h.frame(vec![]);
    let Some(w) = h.state.hover_world else { panic!("hover position must be set") };
    assert!((w.z - 96.0).abs() < 1e-9, "{w:?}");
}

#[test]
fn hover_world_picks_surface_in_3d() {
    let mut h = Harness::new();
    h.state.cameras.cam3d =
        Camera3d { pos: DVec3::new(-128.0, -128.0, 300.0), yaw_deg: 0.0, pitch_deg: -89.0, ..Camera3d::default() };
    let c = h.rect(0).center();
    h.move_to(c);
    h.frame(vec![]);
    let Some(w) = h.state.hover_world else { panic!("the floor is under the cursor") };
    assert!(w.z.abs() < 1e-6, "{w:?}");
}

#[test]
fn wheel_zooms_2d_toward_cursor() {
    let mut h = Harness::new();
    let rect = h.rect(1);
    let p = rect.center() + vec2(100.0, -50.0);
    h.move_to(p);
    let before_zoom = h.state.cameras.top.zoom;
    let view = view_info(1, ViewKind::Top, rect, &h.state.cameras, &RenderConfig::from_state(&h.state), 1.0);
    let anchor = view.screen_to_2d(p);
    h.wheel(p, 1.0);
    assert!((h.state.cameras.top.zoom - before_zoom * 1.2).abs() < 1e-9);
    let view = view_info(1, ViewKind::Top, rect, &h.state.cameras, &RenderConfig::from_state(&h.state), 1.0);
    assert!((view.screen_to_2d(p) - anchor).length() < 1e-6, "the point under the cursor stays put");
    h.modifiers = Modifiers::CTRL;
    let zoom = h.state.cameras.top.zoom;
    h.wheel(p, 1.0);
    assert_eq!(h.state.cameras.top.zoom, zoom);
    assert!(h.state.requested_actions.contains(&Action::GridSmaller));
    h.wheel(p, -1.0);
    assert!(h.state.requested_actions.contains(&Action::GridLarger));
}

#[test]
fn wheel_moves_3d_camera() {
    let mut h = Harness::new();
    let p = h.rect(0).center();
    h.move_to(p);
    let before = h.state.cameras.cam3d.pos;
    h.wheel(p, 2.0);
    let moved = h.state.cameras.cam3d.pos - before;
    assert!((moved.length() - 2.0 * camera::dolly_step(1000.0, false, 3.0)).abs() < 1e-6);
    assert!(moved.dot(h.state.cameras.cam3d.forward()) > 0.0, "wheel up moves forward");
}

#[test]
fn middle_drag_pans_2d_without_tool_input() {
    let mut h = Harness::new();
    let p = h.rect(2).center();
    h.move_to(p);
    let inputs = h.tool.inputs.len();
    let center = h.state.cameras.front.center;
    let zoom = h.state.cameras.front.zoom;
    h.button(p, PointerButton::Middle, true);
    h.move_to(p + vec2(40.0, 20.0));
    assert!(h.vp.is_navigating());
    let moved = h.state.cameras.front.center - center;
    assert!((moved.x + 40.0 / zoom).abs() < 1e-6 && (moved.y - 20.0 / zoom).abs() < 1e-6, "{moved:?}");
    h.button(p + vec2(40.0, 20.0), PointerButton::Middle, false);
    assert_eq!(h.tool.inputs.len(), inputs);
    h.frame(vec![]);
    assert!(!h.vp.is_navigating());
}

#[test]
fn space_drag_pans_2d() {
    let mut h = Harness::new();
    let p = h.rect(1).center();
    h.move_to(p);
    h.key(Key::Space, None, true);
    let center = h.state.cameras.top.center;
    h.button(p, PointerButton::Primary, true);
    h.move_to(p + vec2(-30.0, 0.0));
    assert!(h.state.cameras.top.center.x > center.x, "dragging left moves the view right");
    h.button(p + vec2(-30.0, 0.0), PointerButton::Primary, false);
    h.key(Key::Space, None, false);
    assert!(h.tool.inputs.iter().all(|(_, nav, _)| !nav));
}

#[test]
fn right_drag_looks_and_flies() {
    let mut h = Harness::new();
    let p = h.rect(0).center();
    h.move_to(p);
    let yaw = h.state.cameras.cam3d.yaw_deg;
    h.button(p, PointerButton::Secondary, true);
    assert!(h.vp.captures_keyboard());
    h.move_to(p + vec2(50.0, 0.0));
    let sens = f64::from(h.state.settings.view.mouse_sensitivity);
    assert!((camera::wrap_degrees(h.state.cameras.cam3d.yaw_deg - yaw) + 50.0 * sens).abs() < 1e-6);
    assert_eq!(h.cursor_icon(), Some(CursorIcon::None), "cursor hidden while looking");
    let pos = h.state.cameras.cam3d.pos;
    h.key(Key::Z, Some(Key::W), true);
    for _ in 0..10 {
        h.frame(vec![]);
    }
    let moved = h.state.cameras.cam3d.pos - pos;
    assert!(moved.dot(h.state.cameras.cam3d.forward()) > 10.0, "{moved:?}");
    assert!(!h.vp.is_mouselook(), "AZERTY Z is the forward key, not the mouselook toggle");
    h.key(Key::Z, Some(Key::W), false);
    h.button(p + vec2(50.0, 0.0), PointerButton::Secondary, false);
    assert!(!h.vp.is_context_menu_open(), "a drag does not open the menu");
    assert!(!h.vp.captures_keyboard());
    for _ in 0..60 {
        h.frame(vec![]);
    }
    assert!(!h.vp.is_navigating());
}

#[test]
fn tools_taking_right_drags_see_the_whole_drag() {
    let mut h = Harness::new();
    h.tool.right_drag = true;
    let p = h.rect(0).center();
    h.move_to(p);
    let yaw = h.state.cameras.cam3d.yaw_deg;
    h.tool.inputs.clear();
    h.button(p, PointerButton::Secondary, true);
    for i in 1..=6 {
        h.move_to(p + vec2(i as f32 * 20.0, 0.0));
    }
    let out = h.rect(1).center();
    h.move_to(out);
    let during: Vec<(usize, bool, bool)> = h.tool.inputs.clone();
    h.button(out, PointerButton::Secondary, false);
    h.frame(vec![]);
    assert_eq!(h.state.cameras.cam3d.yaw_deg, yaw, "no camera look");
    assert!(!h.vp.is_context_menu_open(), "a drag does not open the menu");
    assert!(during.len() >= 8, "every frame of the drag reaches the tool: {during:?}");
    assert!(during.iter().all(|(v, nav, pointer)| *v == 0 && !nav && *pointer), "{during:?}");
}

#[test]
fn right_click_opens_context_menu() {
    let mut h = Harness::new();
    let p = h.rect(1).center();
    h.move_to(p);
    h.button(p, PointerButton::Secondary, true);
    h.button(p, PointerButton::Secondary, false);
    h.frame(vec![]);
    assert!(h.vp.is_context_menu_open());
    let Some(menu) = h.vp.menu else { panic!("menu must be open") };
    assert_eq!(menu.view.index, 1);
    assert!(menu.point.is_some());
    assert!(h.tool.secondary_clicks.is_empty(), "the selection tool does not take right clicks");
    h.key(Key::Escape, None, true);
    h.frame(vec![]);
    assert!(!h.vp.is_context_menu_open());
}

fn right_click(h: &mut Harness, p: Pos2, modifiers: Modifiers) {
    h.modifiers = modifiers;
    h.move_to(p);
    h.button(p, PointerButton::Secondary, true);
    h.button(p, PointerButton::Secondary, false);
    h.frame(vec![]);
    h.modifiers = Modifiers::NONE;
}

#[test]
fn texture_tool_gets_right_clicks_in_3d() {
    let mut h = Harness::new();
    h.tool.id = ToolId::Texture;
    let p = h.rect(0).center();
    right_click(&mut h, p, Modifiers::NONE);
    assert!(!h.vp.is_context_menu_open(), "no context menu over the 3D view");
    assert_eq!(h.tool.secondary_clicks, vec![(0, Modifiers::NONE)]);
    assert!(h.tool.inputs.iter().any(|(v, nav, _)| *v == 0 && !nav));
    right_click(&mut h, p + vec2(5.0, 0.0), Modifiers::ALT);
    assert!(!h.vp.is_context_menu_open());
    assert_eq!(h.tool.secondary_clicks.len(), 2);
    assert!(h.tool.secondary_clicks[1].1.alt);
    right_click(&mut h, p + vec2(10.0, 0.0), Modifiers::SHIFT);
    assert!(h.vp.is_context_menu_open());
    assert_eq!(h.tool.secondary_clicks.len(), 2, "the menu click does not reach the tool");
    h.key(Key::Escape, None, true);
    h.frame(vec![]);
    assert!(!h.vp.is_context_menu_open());
    h.move_to(p);
    h.button(p, PointerButton::Secondary, true);
    h.move_to(p + vec2(40.0, 0.0));
    h.button(p + vec2(40.0, 0.0), PointerButton::Secondary, false);
    h.frame(vec![]);
    assert!(!h.vp.is_context_menu_open());
    assert_eq!(h.tool.secondary_clicks.len(), 2);
    let q = h.rect(1).center();
    right_click(&mut h, q, Modifiers::NONE);
    assert!(h.vp.is_context_menu_open());
    assert_eq!(h.vp.menu.map(|m| m.view.index), Some(1));
    assert!(!tool_takes_right_click(ToolId::Select, ViewKind::Perspective, Modifiers::NONE));
    assert!(tool_takes_right_click(ToolId::Texture, ViewKind::Perspective, Modifiers::ALT));
    assert!(!tool_takes_right_click(ToolId::Texture, ViewKind::Top, Modifiers::NONE));
}

#[test]
fn zoom_percent_is_readable_at_every_zoom() {
    assert_eq!(format_zoom_percent(Camera2d::MIN_ZOOM), "0.2");
    assert_eq!(format_zoom_percent(0.0029), "0.29");
    assert_eq!(format_zoom_percent(0.045), "4.5");
    assert_eq!(format_zoom_percent(0.05), "5");
    assert_eq!(format_zoom_percent(0.0999), "10");
    assert_eq!(format_zoom_percent(0.25), "25");
    assert_eq!(format_zoom_percent(Camera2d::MAX_ZOOM), "6400");
    assert_eq!(format_zoom_percent(f64::NAN), "?");
    let mut z = Camera2d::MIN_ZOOM;
    while z <= Camera2d::MAX_ZOOM {
        let text = format_zoom_percent(z);
        assert!(text.parse::<f64>().is_ok_and(|v| v > 0.0), "{z}: {text}");
        z *= 1.2;
    }
}

#[test]
fn context_menu_choices() {
    let mut h = Harness::new();
    let view = view_info(1, ViewKind::Top, h.rect(1), &h.state.cameras, &RenderConfig::from_state(&h.state), 1.0);
    let menu = OpenMenu {
        view,
        pos: view.rect.center(),
        point: Some(DVec3::new(128.0, 64.0, 0.0)),
        normal: None,
        opened_frame: 1,
    };
    h.state.cameras.front.center = DVec2::new(0.0, 40.0);
    h.vp.run_menu_choice(&mut h.state, &menu, MenuChoice::CenterHere);
    assert_eq!(h.state.cameras.top.center, DVec2::new(128.0, 64.0));
    assert_eq!(h.state.cameras.front.center, DVec2::new(64.0, 40.0), "front keeps its height");
    h.vp.run_menu_choice(&mut h.state, &menu, MenuChoice::Action(Action::Copy));
    assert!(h.state.requested_actions.contains(&Action::Copy));
    let before = h.state.doc.scene().entities().len();
    h.state.current_entity_class = "info_target".to_owned();
    h.vp.run_menu_choice(&mut h.state, &menu, MenuChoice::CreateHere);
    assert_eq!(h.state.doc.scene().entities().len(), before + 1);
    let sel = h.state.doc.selected_entity_ids();
    let Some(node) = sel.first().and_then(|id| h.state.doc.scene().entity(*id)) else { panic!("new entity selected") };
    assert_eq!(node.entity.classname, "info_target");
    assert_eq!(node.entity.origin(), Some(DVec3::new(128.0, 64.0, 0.0)));
}

#[test]
fn mouselook_toggle() {
    let mut h = Harness::new();
    let p = h.rect(0).center() + vec2(30.0, 30.0);
    h.move_to(p);
    h.key(Key::Z, Some(Key::Z), true);
    assert!(h.vp.is_mouselook());
    assert!(h.vp.captures_keyboard());
    assert_eq!(h.state.ui.focused_view, Some(0));
    let yaw = h.state.cameras.cam3d.yaw_deg;
    h.move_to(p + vec2(-20.0, 0.0));
    assert!(camera::wrap_degrees(h.state.cameras.cam3d.yaw_deg - yaw) > 0.0, "looks without a button");
    let inputs = h.tool.inputs.len();
    h.frame(vec![]);
    assert_eq!(h.tool.inputs.len(), inputs, "tools are not called during mouselook");
    h.key(Key::Z, Some(Key::Z), false);
    h.key(Key::Escape, None, true);
    assert!(!h.vp.is_mouselook());
    h.move_to(p);
    h.key(Key::Z, Some(Key::W), true);
    assert!(h.vp.is_mouselook());
    h.key(Key::Z, Some(Key::W), false);
    let pos = h.state.cameras.cam3d.pos;
    h.key(Key::Z, Some(Key::W), true);
    assert!(h.vp.is_mouselook(), "the forward key does not leave mouselook");
    for _ in 0..5 {
        h.frame(vec![]);
    }
    assert!((h.state.cameras.cam3d.pos - pos).dot(h.state.cameras.cam3d.forward()) > 1.0, "flies forward");
    h.key(Key::Z, Some(Key::W), false);
    h.key(Key::W, Some(Key::Z), true);
    assert!(!h.vp.is_mouselook());
    h.key(Key::W, Some(Key::Z), false);
    let q = h.rect(1).center();
    h.move_to(q);
    h.key(Key::Z, Some(Key::Z), true);
    assert!(!h.vp.is_mouselook());
}

#[test]
fn left_click_while_flying_reaches_the_tool_at_the_crosshair() {
    let mut h = Harness::new();
    let center = h.rect(0).center();
    let p = center + vec2(10.0, 5.0);
    h.move_to(p);
    h.key(Key::Z, Some(Key::Z), true);
    h.key(Key::Z, Some(Key::Z), false);
    assert!(h.vp.is_mouselook());
    h.button(p, PointerButton::Primary, true);
    h.button(p, PointerButton::Primary, false);
    assert_eq!(h.tool.fly_clicks, vec![(0, center, Modifiers::NONE)], "one click, at the crosshair");
    assert!(h.vp.is_mouselook(), "the click does not leave mouselook");
    h.modifiers = Modifiers::CTRL;
    let pos = h.state.cameras.cam3d.pos;
    for _ in 0..5 {
        h.frame(vec![]);
    }
    assert_eq!(h.state.cameras.cam3d.pos, pos, "Ctrl does not fly down");
    h.button(p, PointerButton::Primary, true);
    h.button(p, PointerButton::Primary, false);
    assert_eq!(h.tool.fly_clicks.len(), 2);
    assert!(h.tool.fly_clicks[1].2.ctrl);
    h.modifiers = Modifiers::NONE;
    h.key(Key::Q, Some(Key::Q), true);
    for _ in 0..5 {
        h.frame(vec![]);
    }
    h.key(Key::Q, Some(Key::Q), false);
    assert!(h.state.cameras.cam3d.pos.z < pos.z - 1.0, "Q flies down");
    h.key(Key::Escape, None, true);
    assert!(!h.vp.is_mouselook());
    h.move_to(p);
    h.button(p, PointerButton::Primary, true);
    h.button(p, PointerButton::Primary, false);
    assert_eq!(h.tool.fly_clicks.len(), 2, "no fly click outside mouselook");
    h.tool.id = ToolId::Displacement;
    h.key(Key::Z, Some(Key::Z), true);
    h.key(Key::Z, Some(Key::Z), false);
    assert!(h.vp.is_mouselook());
    h.button(p, PointerButton::Primary, true);
    h.button(p, PointerButton::Primary, false);
    assert_eq!(h.tool.fly_clicks.len(), 2, "the displacement tool keeps its view input");
}

#[test]
fn splitter_drag_and_reset() {
    let mut h = Harness::new();
    let x = (h.rect(0).right() + h.rect(1).left()) * 0.5;
    let y = h.rect(0).top() + 40.0;
    let p = Pos2::new(x, y);
    h.move_to(p);
    h.button(p, PointerButton::Primary, true);
    for k in 1..=5 {
        h.move_to(p + vec2(20.0 * k as f32, 0.0));
    }
    let split = h.state.settings.ui.split_x;
    assert!(split > 0.52, "{split}");
    assert!(!h.state.settings_dirty, "saved when the drag ends");
    h.button(p + vec2(100.0, 0.0), PointerButton::Primary, false);
    assert!(h.state.settings_dirty);
    assert!(h.tool.inputs.is_empty() || h.tool.inputs.iter().all(|(v, _, _)| *v == 0 || *v == 1));
    h.state.settings_dirty = false;
    h.frame(vec![]);
    let x = (h.rect(0).right() + h.rect(1).left()) * 0.5;
    let p = Pos2::new(x, y);
    h.move_to(p);
    for _ in 0..2 {
        h.button(p, PointerButton::Primary, true);
        h.button(p, PointerButton::Primary, false);
    }
    assert_eq!(h.state.settings.ui.split_x, 0.5);
    assert!(h.state.settings_dirty);
}

#[test]
fn maximize_toggles() {
    let mut state = test_state();
    toggle_maximize(&mut state, 3);
    assert_eq!(state.ui.maximized_view, Some(3));
    toggle_maximize(&mut state, 1);
    assert_eq!(state.ui.maximized_view, Some(1));
    toggle_maximize(&mut state, 1);
    assert_eq!(state.ui.maximized_view, None);
}

fn drop_payload(h: &mut Harness, view: usize, at: Pos2, payload: DragPayload) {
    h.move_to(at);
    egui::DragAndDrop::set_payload(&h.ctx, payload);
    h.move_to(at + vec2(1.0, 0.0));
    h.button(at, PointerButton::Primary, false);
    assert_eq!(h.state.ui.focused_view, Some(view));
}

#[test]
fn drop_entity_and_model_in_2d() {
    let mut h = Harness::new();
    let at = h.rect(1).center() + vec2(37.0, -21.0);
    let before = h.state.doc.scene().entities().len();
    drop_payload(&mut h, 1, at, DragPayload::EntityClass("info_target".to_owned()));
    assert_eq!(h.state.doc.scene().entities().len(), before + 1);
    assert_eq!(h.state.current_entity_class, "info_target");
    let sel = h.state.doc.selected_entity_ids();
    let Some(e) = sel.first().and_then(|id| h.state.doc.scene().entity(*id)) else { panic!("dropped entity selected") };
    let Some(o) = e.entity.origin() else { panic!("origin") };
    assert_eq!((o.x % 16.0, o.y % 16.0, o.z), (0.0, 0.0, 0.0), "{o:?}");

    let path = "models/props_c17/oildrum001.mdl";
    drop_payload(&mut h, 1, at, DragPayload::Model { path: path.to_owned(), skin: 0 });
    let sel = h.state.doc.selected_entity_ids();
    let Some(e) = sel.first().and_then(|id| h.state.doc.scene().entity(*id)) else { panic!("dropped prop selected") };
    assert_eq!(e.entity.classname, "prop_static");
    assert_eq!(e.entity.get("model"), Some(path));
    assert_eq!(h.state.current_model.as_deref(), Some(path));
    assert!(!egui::DragAndDrop::has_any_payload(&h.ctx));
}

#[test]
fn drop_material_on_3d_face_and_2d_selection() {
    let mut h = Harness::new();
    h.state.cameras.cam3d =
        Camera3d { pos: DVec3::new(-128.0, -128.0, 300.0), yaw_deg: 0.0, pitch_deg: -89.0, ..Camera3d::default() };
    let at = h.rect(0).center();
    let mat = "concrete/concretefloor001a";
    h.move_to(at);
    egui::DragAndDrop::set_payload(&h.ctx, DragPayload::Material(mat.to_owned()));
    h.move_to(at + vec2(1.0, 0.0));
    assert!(!h.vp.preview.polygon.is_empty(), "the floor face is highlighted");
    h.button(at, PointerButton::Primary, false);
    let Some(floor) = h.state.doc.scene().solid(2) else { panic!("floor solid") };
    assert!(floor.solid.sides.iter().any(|s| s.material == mat), "material applied to the face under the cursor");
    assert_eq!(floor.solid.sides.iter().filter(|s| s.material == mat).count(), 1, "only that face");
    assert_eq!(h.state.current_material, mat);

    let wall = "dev/dev_measuregeneric01b";
    h.state.doc.select(&[hp_editor::ObjRef::Solid(3)], hp_editor::SelectOp::Replace);
    let at2 = h.rect(1).center();
    drop_payload(&mut h, 1, at2, DragPayload::Material(wall.to_owned()));
    let Some(s3) = h.state.doc.scene().solid(3) else { panic!("wall solid") };
    assert!(s3.solid.sides.iter().all(|s| s.material == wall));
}

#[test]
fn drop_prefab_pastes_in_background() {
    let dir = std::path::Path::new(env!("CARGO_MANIFEST_DIR")).join("../../target/hp-ui-viewports/scratch");
    if std::fs::create_dir_all(&dir).is_err() {
        return;
    }
    let file = dir.join("prefab_test.vmf");
    if std::fs::write(&file, hp_app::testing::TEST_VMF).is_err() {
        return;
    }
    let mut h = Harness::new();
    let solids = h.state.doc.scene().solids().len();
    let at = h.rect(1).center() + vec2(200.0, 0.0);
    drop_payload(&mut h, 1, at, DragPayload::Prefab(file.clone()));
    for _ in 0..200 {
        if !h.vp.is_loading_prefab() {
            break;
        }
        std::thread::sleep(std::time::Duration::from_millis(10));
        h.frame(vec![]);
    }
    assert!(!h.vp.is_loading_prefab());
    assert!(h.state.doc.scene().solids().len() > solids, "prefab solids pasted");
    assert_eq!(h.state.doc.undo_label(), Some("Paste prefab"), "one undo step");
    drop_payload(&mut h, 1, at, DragPayload::Prefab(dir.join("does_not_exist.vmf")));
    for _ in 0..200 {
        if !h.vp.is_loading_prefab() {
            break;
        }
        std::thread::sleep(std::time::Duration::from_millis(10));
        h.frame(vec![]);
    }
    assert!(h.state.console.lines().any(|l| l.severity == Severity::Error));
}

#[test]
fn header_dropdown_opens_and_closes() {
    let mut h = Harness::new();
    let header = h.rect(3).min + vec2(14.0, 14.0);
    h.move_to(header);
    h.button(header, PointerButton::Primary, true);
    h.button(header, PointerButton::Primary, false);
    h.frame(vec![]);
    assert!(egui::Popup::is_any_open(&h.ctx), "header dropdown open");
    assert!(h.tool.inputs.iter().all(|(v, _, _)| *v != 3));
    assert_ne!(h.state.ui.hovered_view, Some(3), "the header is not part of the view for hovering");
    h.key(Key::Escape, None, true);
    h.frame(vec![]);
    assert!(!egui::Popup::is_any_open(&h.ctx));
}

struct FgdAssets {
    inner: hp_app::testing::MockAssets,
    fgd: hp_fgd::ResolvedFgd,
}

const TEST_FGD: &str = r#"
@PointClass size(-16 -16 0, 16 16 72) color(0 255 0) = info_player_start : "Player start"
[
    angles(angle) : "Pitch Yaw Roll (Y Z X)" : "0 0 0"
]
@PointClass studioprop() = prop_static : "Static prop"
[
    model(studio) : "World Model" : ""
    skin(integer) : "Skin" : 0
    solid(choices) : "Collisions" : 6 =
    [
        0 : "Not Solid"
        6 : "Use VPhysics"
    ]
]
@SolidClass = func_detail : "Detail brush" []
@PointClass studio("models/editor/fixed.mdl") = prop_fixed : "Fixed model" []
@PointClass studioprop() = prop_default : "Default model" [ model(studio) : "Model" : "models/default.mdl" ]
@PointClass studio("models/editor/playerstart.mdl") = info_player_deathmatch : "Spawn point, GMod style (model, no size)"
[
    angles(angle) : "Pitch Yaw Roll (Y Z X)" : "0 0 0"
]
"#;

const PLAYERSTART_BOUNDS: (DVec3, DVec3) = (DVec3::new(-6.8, -21.0, 0.113), DVec3::new(6.8, 5.25, 71.27));
const PLAYERSTART_HULL: (DVec3, DVec3) = (DVec3::new(-16.0, -16.0, 0.0), DVec3::new(16.0, 16.0, 72.0));

impl FgdAssets {
    fn new() -> Self {
        Self { inner: hp_app::testing::MockAssets::default(), fgd: hp_fgd::Fgd::parse(TEST_FGD, None).resolve() }
    }
}

impl hp_app::AssetsApi for FgdAssets {
    fn status(&self) -> hp_app::state::AssetStatus {
        self.inner.status()
    }
    fn fs(&self) -> Option<&hp_fs::GameFs> {
        None
    }
    fn fgd(&self) -> Option<&hp_fgd::ResolvedFgd> {
        Some(&self.fgd)
    }
    fn generation(&self) -> u64 {
        self.inner.generation()
    }
    fn remount(&mut self, settings: &hp_app::settings::Settings) {
        self.inner.remount(settings);
    }
    fn reload_fgd(&mut self, settings: &hp_app::settings::Settings) {
        self.inner.reload_fgd(settings);
    }
    fn materials(&self) -> std::sync::Arc<Vec<hp_fs::AssetEntry>> {
        self.inner.materials()
    }
    fn models(&self) -> std::sync::Arc<Vec<hp_fs::AssetEntry>> {
        self.inner.models()
    }
    fn sounds(&self) -> std::sync::Arc<Vec<hp_fs::AssetEntry>> {
        self.inner.sounds()
    }
    fn source_label(&self, source: u32) -> String {
        self.inner.source_label(source)
    }
    fn material_info(&mut self, name: &str) -> Option<std::sync::Arc<hp_app::state::MaterialInfo>> {
        self.inner.material_info(name)
    }
    fn thumbnail(&mut self, ctx: &egui::Context, name: &str) -> hp_app::state::ThumbState {
        self.inner.thumbnail(ctx, name)
    }
    fn model_info(&mut self, path: &str) -> hp_app::state::ModelState {
        if path == "models/editor/playerstart.mdl" {
            return hp_app::state::ModelState::Ready(std::sync::Arc::new(hp_app::state::ModelInfo {
                path: path.to_owned(),
                bounds_min: PLAYERSTART_BOUNDS.0,
                bounds_max: PLAYERSTART_BOUNDS.1,
                hull_min: Some(PLAYERSTART_HULL.0),
                hull_max: Some(PLAYERSTART_HULL.1),
                is_static_prop: false,
                ..Default::default()
            }));
        }
        if path.contains("dynamic") {
            return hp_app::state::ModelState::Ready(std::sync::Arc::new(hp_app::state::ModelInfo {
                path: path.to_owned(),
                bounds_min: DVec3::new(-10.0, -10.0, -4.0),
                bounds_max: DVec3::new(10.0, 10.0, 30.0),
                is_static_prop: false,
                ..Default::default()
            }));
        }
        if path.contains("broken") {
            return hp_app::state::ModelState::Failed("bad header".to_owned());
        }
        self.inner.model_info(path)
    }
    fn model_preview(
        &mut self,
        gfx: &mut hp_app::Gfx,
        path: &str,
        camera: &hp_app::state::PreviewCamera,
        size_px: [u32; 2],
    ) -> Option<egui::TextureId> {
        self.inner.model_preview(gfx, path, camera, size_px)
    }
    fn entity_bounds(&self, entity: &hp_vmf::Entity) -> Option<hp_geom::Aabb> {
        let o = entity.origin()?;
        let class = self.fgd.class(&entity.classname);
        if class.and_then(|c| c.studio()).and_then(|s| s.model).as_deref() == Some("models/editor/playerstart.mdl") {
            return Some(hp_geom::Aabb::new(o + PLAYERSTART_BOUNDS.0, o + PLAYERSTART_BOUNDS.1));
        }
        if let Some((a, b)) = class.and_then(|c| c.size()) {
            return Some(hp_geom::Aabb::new(o + DVec3::from_array(a), o + DVec3::from_array(b)));
        }
        self.inner.entity_bounds(entity)
    }
    fn invalidate_material(&mut self, name: &str) {
        self.inner.invalidate_material(name);
    }
    fn import_images(&mut self, files: &[PathBuf], opts: &hp_app::state::ImportOptions) -> Vec<Result<String, String>> {
        self.inner.import_images(files, opts)
    }
    fn begin_frame(&mut self, ctx: &egui::Context, gfx: Option<&mut hp_app::Gfx>) {
        self.inner.begin_frame(ctx, gfx);
    }
    fn sync_scene(&mut self, doc: &mut hp_editor::Document, gfx: &mut hp_app::Gfx, opts: &hp_app::state::SceneOptions) {
        self.inner.sync_scene(doc, gfx, opts);
    }
    fn render_material_id(&mut self, name: &str) -> hp_render::MaterialId {
        self.inner.render_material_id(name)
    }
    fn status_line(&self) -> Option<String> {
        self.inner.status_line()
    }
}

fn fgd_harness() -> Harness {
    let mut h = Harness::new();
    let doc = std::mem::replace(&mut h.state.doc, hp_editor::Document::new_empty());
    let mut state = AppState::new(hp_app::settings::Settings::default(), Box::new(FgdAssets::new()));
    state.doc = doc;
    h.state = state;
    h.frame(vec![]);
    h
}

#[test]
fn drops_use_fgd_defaults_and_rest_on_surfaces() {
    let mut h = fgd_harness();
    h.state.cameras.cam3d =
        Camera3d { pos: DVec3::new(-128.0, -128.0, 300.0), yaw_deg: 0.0, pitch_deg: -89.0, ..Camera3d::default() };
    let at = h.rect(0).center();
    drop_payload(&mut h, 0, at, DragPayload::EntityClass("info_player_start".to_owned()));
    let sel = h.state.doc.selected_entity_ids();
    let Some(e) = sel.first().and_then(|id| h.state.doc.scene().entity(*id)) else { panic!("player start created") };
    assert_eq!(e.entity.classname, "info_player_start");
    assert_eq!(e.entity.get("angles"), Some("0 0 0"), "FGD defaults applied");
    let Some(o) = e.entity.origin() else { panic!("origin") };
    assert!(o.z.abs() < 1e-6, "feet on the floor (size mins z = 0): {o:?}");
    assert_eq!((o.x % 16.0, o.y % 16.0), (0.0, 0.0), "snapped in the floor plane: {o:?}");

    drop_payload(
        &mut h,
        0,
        at + vec2(120.0, 0.0),
        DragPayload::Model { path: "models/props_c17/oildrum001.mdl".to_owned(), skin: 0 },
    );
    let sel = h.state.doc.selected_entity_ids();
    let Some(e) = sel.first().and_then(|id| h.state.doc.scene().entity(*id)) else { panic!("prop created") };
    assert_eq!(e.entity.classname, "prop_static");
    assert_eq!(e.entity.get("skin"), Some("0"));
    assert_eq!(e.entity.get("solid"), Some("6"));
    let Some(o) = e.entity.origin() else { panic!("origin") };
    assert!((o.z - 16.0).abs() < 1e-6, "mock model bounds start at z = -16: {o:?}");

    drop_payload(
        &mut h,
        0,
        at + vec2(-120.0, 0.0),
        DragPayload::Model { path: "models/dynamic_thing.mdl".to_owned(), skin: 0 },
    );
    let sel = h.state.doc.selected_entity_ids();
    let Some(e) = sel.first().and_then(|id| h.state.doc.scene().entity(*id)) else { panic!("prop created") };
    assert_eq!(e.entity.classname, "prop_dynamic");
    assert_eq!(e.entity.origin().map(|o| o.z), Some(4.0));

    drop_payload(
        &mut h,
        0,
        at + vec2(0.0, 100.0),
        DragPayload::Model { path: "models/broken.mdl".to_owned(), skin: 0 },
    );
    assert!(h.state.console.lines().any(|l| l.severity == Severity::Warning));

    let count = h.state.doc.scene().entities().len();
    drop_payload(&mut h, 0, at, DragPayload::EntityClass("func_detail".to_owned()));
    assert_eq!(h.state.doc.scene().entities().len(), count);
}

fn create_here(h: &mut Harness, class: &str, offset: egui::Vec2) -> hp_vmf::Entity {
    h.state.current_entity_class = class.to_owned();
    let p = h.rect(0).center() + offset;
    right_click(h, p, Modifiers::NONE);
    let Some(menu) = h.vp.menu else { panic!("menu open") };
    assert!(menu.point.is_some(), "{class}: a surface under the click");
    let count = h.state.doc.scene().entities().len();
    h.vp.run_menu_choice(&mut h.state, &menu, MenuChoice::CreateHere);
    h.vp.menu = None;
    assert_eq!(h.state.doc.scene().entities().len(), count + 1, "{class} created at {:?}", menu.point);
    let sel = h.state.doc.selected_entity_ids();
    let Some(e) = sel.first().and_then(|id| h.state.doc.scene().entity(*id)) else { panic!("{class} created") };
    assert_eq!(e.entity.classname, class);
    e.entity.clone()
}

#[test]
fn create_here_places_like_the_entity_tool() {
    let mut h = fgd_harness();
    h.state.cameras.cam3d =
        Camera3d { pos: DVec3::new(-100.0, -60.0, 300.0), yaw_deg: 0.0, pitch_deg: -89.0, ..Camera3d::default() };
    let e = create_here(&mut h, "info_player_start", vec2(-100.0, 0.0));
    let Some(o) = e.origin() else { panic!("origin") };
    assert_eq!(o.z, 0.0, "{o:?}");
    assert_eq!((o.x % 16.0, o.y % 16.0), (0.0, 0.0), "{o:?}");
    let text = e.get("origin").unwrap_or_default().to_owned();
    assert!(!text.contains('.'), "no long decimals: {text}");
    let e = create_here(&mut h, "info_player_deathmatch", vec2(0.0, 0.0));
    assert_eq!(e.origin().map(|o| o.z), Some(0.0));
    h.state.current_model = Some("models/dynamic_thing.mdl".to_owned());
    h.state.current_model_skin = 3;
    let e = create_here(&mut h, "prop_static", vec2(100.0, 0.0));
    assert_eq!(e.get("model"), Some("models/dynamic_thing.mdl"));
    assert_eq!(e.get("skin"), Some("3"));
    assert_eq!(e.origin().map(|o| o.z), Some(4.0), "mock model bounds start at z = -4");

    h.state.cameras.cam3d =
        Camera3d { pos: DVec3::new(100.0, -200.0, 64.0), yaw_deg: 90.0, pitch_deg: 0.0, ..Camera3d::default() };
    for (class, dx) in [("info_player_start", -150.0), ("info_player_deathmatch", 50.0)] {
        let e = create_here(&mut h, class, vec2(dx, 0.0));
        let Some(o) = e.origin() else { panic!("origin") };
        assert_eq!((o.y, o.z, o.x % 16.0), (240.0, 64.0, 0.0), "{class}: {o:?}");
        let text = e.get("origin").unwrap_or_default().to_owned();
        assert!(text.ends_with(" 240 64") && !text.contains('.'), "{class}: {text}");
    }
    assert_eq!(placement::placement_bounds(&mut h.state, "info_player_deathmatch", None), PLAYERSTART_HULL);
    assert_eq!(placement::placement_bounds(&mut h.state, "info_player_start", None), PLAYERSTART_HULL);
}

#[test]
fn create_here_preserves_predefined_models_instead_of_the_browser_model() {
    let mut h = fgd_harness();
    h.state.cameras.cam3d =
        Camera3d { pos: DVec3::new(-100.0, -60.0, 300.0), yaw_deg: 0.0, pitch_deg: -89.0, ..Camera3d::default() };
    h.state.current_model = Some("models/browser.mdl".to_owned());
    h.state.current_model_skin = 4;
    for (class, expected, x) in [("prop_fixed", None, -100.0), ("prop_default", Some("models/default.mdl"), 100.0)] {
        let e = create_here(&mut h, class, vec2(x, 0.0));
        assert_eq!(e.get("model"), expected);
        assert_eq!(e.get("skin"), None);
    }
}

#[test]
fn dropped_models_keep_the_chosen_skin() {
    let mut h = fgd_harness();
    h.state.cameras.cam3d =
        Camera3d { pos: DVec3::new(-128.0, -128.0, 300.0), yaw_deg: 0.0, pitch_deg: -89.0, ..Camera3d::default() };
    let at = h.rect(0).center();
    let drum = "models/props_c17/oildrum001.mdl";
    h.state.current_model = Some(drum.to_owned());
    h.state.current_model_skin = 2;
    drop_payload(&mut h, 0, at, DragPayload::Model { path: drum.to_owned(), skin: 0 });
    let sel = h.state.doc.selected_entity_ids();
    let Some(e) = sel.first().and_then(|id| h.state.doc.scene().entity(*id)) else { panic!("prop created") };
    assert_eq!(e.entity.get("model"), Some(drum));
    assert_eq!(e.entity.get("skin"), Some("2"));
    assert_eq!(h.state.current_model_skin, 2, "the drop keeps the chosen skin");
    drop_payload(
        &mut h,
        0,
        at + vec2(120.0, 0.0),
        DragPayload::Model { path: "models/dynamic_thing.mdl".to_owned(), skin: 0 },
    );
    let sel = h.state.doc.selected_entity_ids();
    let Some(e) = sel.first().and_then(|id| h.state.doc.scene().entity(*id)) else { panic!("prop created") };
    assert!(matches!(e.entity.get("skin"), None | Some("0")), "default skin");
    assert_eq!(h.state.current_model.as_deref(), Some("models/dynamic_thing.mdl"));
    assert_eq!(h.state.current_model_skin, 0);
}

#[test]
fn context_menu_object_entries_need_selected_objects() {
    let mut h = Harness::new();
    let p = h.rect(1).center();
    right_click(&mut h, p, Modifiers::NONE);
    let Some(menu) = h.vp.menu else { panic!("menu open") };
    h.state.doc.select_faces(&[hp_editor::FaceRef::new(2, 1)], hp_editor::SelectOp::Replace);
    assert!(!h.state.doc.selection().is_empty());
    let cx = menu_context(&h.state, &menu);
    let state = &h.state;
    for action in [
        Action::Cut,
        Action::Copy,
        Action::Delete,
        Action::Duplicate,
        Action::Group,
        Action::HideSelected,
        Action::Carve,
        Action::MakeHollow,
        Action::SnapToGrid,
        Action::TransformDialog,
    ] {
        let on = entry_enabled(MenuEntry::Action(action), cx, |a| a.enabled(state));
        assert!(!on, "{action:?} enabled with only faces selected");
    }
    assert!(!entry_enabled(MenuEntry::Submenu(hp_app::actions::MenuGroup::ToolsAlign), cx, |a| a.enabled(state)));
    assert!(entry_enabled(MenuEntry::Action(Action::Paste), cx, |a| a.enabled(state)));
    assert!(!entry_enabled(MenuEntry::Action(Action::Undo), cx, |a| a.enabled(state)), "nothing to undo");
    h.state.doc.select(&[hp_editor::ObjRef::Solid(2)], hp_editor::SelectOp::Replace);
    let cx = menu_context(&h.state, &menu);
    let state = &h.state;
    assert!(entry_enabled(MenuEntry::Action(Action::Delete), cx, |a| a.enabled(state)));
    assert!(entry_enabled(MenuEntry::Submenu(hp_app::actions::MenuGroup::ToolsFlip), cx, |a| a.enabled(state)));
}

#[derive(Default)]
struct RetainedTool {
    id: ToolId,
    version: Cell<Option<u64>>,
    fills: Cell<usize>,
    size: usize,
}

impl Tool for RetainedTool {
    fn id(&self) -> ToolId {
        self.id
    }

    fn view_input(&mut self, _state: &mut AppState, _input: &ViewInput<'_>) -> ToolFlow {
        ToolFlow::Ignored
    }

    fn retained_overlay_version(&self, _state: &AppState, _view: &ViewInfo) -> Option<u64> {
        self.version.get()
    }

    fn retained_overlay(&self, _state: &AppState, _view: &ViewInfo, out: &mut Overlay) {
        assert!(out.is_empty(), "the retained overlay arrives empty");
        self.fills.set(self.fills.get() + 1);
        let line = hp_render::OverlayLine { a: [0.0; 3], b: [1.0; 3], color: [1.0; 4] };
        out.lines.extend(std::iter::repeat_n(line, self.size.max(1)));
    }
}

#[test]
fn retained_overlays_refill_only_when_the_version_changes() {
    let mut h = Harness::new();
    let cfg = RenderConfig::from_state(&h.state);
    let v3d = view_info(0, ViewKind::Perspective, h.rect(0), &h.state.cameras, &cfg, 1.0);
    let top = view_info(0, ViewKind::Top, h.rect(0), &h.state.cameras, &cfg, 1.0);
    let other = view_info(1, ViewKind::Top, h.rect(1), &h.state.cameras, &cfg, 1.0);
    let mut tool = RetainedTool { id: ToolId::Texture, ..Default::default() };
    assert!(!h.vp.refill_retained(&h.state, &tool, &v3d));
    tool.version.set(Some(7));
    assert!(h.vp.refill_retained(&h.state, &tool, &v3d), "first version uploads");
    assert_eq!(tool.fills.get(), 1);
    assert_eq!(h.vp.retained_overlay.lines.len(), 1);
    for _ in 0..5 {
        assert!(!h.vp.refill_retained(&h.state, &tool, &v3d));
    }
    assert_eq!(tool.fills.get(), 1);
    assert!(h.vp.refill_retained(&h.state, &tool, &other));
    assert_eq!(tool.fills.get(), 2);
    tool.version.set(Some(8));
    assert!(h.vp.refill_retained(&h.state, &tool, &v3d));
    assert!(h.vp.refill_retained(&h.state, &tool, &top));
    assert!(!h.vp.refill_retained(&h.state, &tool, &top));
    tool.id = ToolId::Vertex;
    assert!(h.vp.refill_retained(&h.state, &tool, &top));
    assert_eq!(tool.fills.get(), 5);
    tool.version.set(None);
    assert!(h.vp.refill_retained(&h.state, &tool, &top));
    assert!(h.vp.retained_overlay.is_empty());
    assert!(!h.vp.refill_retained(&h.state, &tool, &top));
    assert_eq!(tool.fills.get(), 5);
    tool.size = 100_000;
    tool.version.set(Some(9));
    assert!(h.vp.refill_retained(&h.state, &tool, &top));
    assert_eq!(h.vp.retained_overlay.lines.len(), 100_000);
    h.vp.trim_retained_scratch();
    assert_eq!(h.vp.retained_overlay.lines.capacity(), 0);
    let fills = tool.fills.get();
    tool.version.set(Some(10));
    let input = RawInput {
        screen_rect: Some(egui::Rect::from_min_size(Pos2::ZERO, vec2(1600.0, 900.0))),
        ..Default::default()
    };
    let mut out = h.ctx.run_ui(input, |ui| {
        egui::CentralPanel::default().show(ui, |ui| h.vp.ui(ui, &mut h.state, &mut tool));
    });
    out.textures_delta.clear();
    assert_eq!(tool.fills.get(), fills);
}

#[test]
fn real_maps_navigation_and_picking() {
    let mut paths = vec![PathBuf::from(r"C:\Users\oracle\Desktop\hammermcptest\test.vmf")];
    let ttt = PathBuf::from(r"D:\SteamLibrary\steamapps\common\GarrysMod\garrysmod\gamemodes\terrortown\mapexamples");
    if let Ok(dir) = std::fs::read_dir(&ttt) {
        paths.extend(
            dir.flatten().map(|e| e.path()).filter(|p| p.extension().is_some_and(|x| x.eq_ignore_ascii_case("vmf"))),
        );
    }
    for path in paths.into_iter().filter(|p| p.is_file()) {
        let Ok(doc) = hp_editor::Document::open(&path) else { continue };
        let mut h = Harness::new();
        h.state.doc = doc;
        let bounds = h.state.doc.scene().solids().values().fold(hp_geom::Aabb::EMPTY, |b, n| b.union(&n.bounds));
        if bounds.is_empty() {
            continue;
        }
        h.state.cameras.frame_2d(bounds.min, bounds.max, h.rect(1).size());
        h.state.cameras.cam3d.look_at(bounds.center());
        h.state.cameras.cam3d.frame_box(bounds.min, bounds.max);
        let start = std::time::Instant::now();
        let mut hits = 0;
        for i in 0..4 {
            let r = h.rect(i);
            for k in 0..8 {
                let p = r.min + vec2(r.width() * (0.1 + 0.1 * k as f32), r.height() * (0.15 + 0.09 * k as f32));
                h.move_to(p);
                hits += usize::from(h.state.hover_world.is_some());
            }
        }
        assert!(hits >= 24, "{}: 2D views always have a hover position ({hits})", path.display());
        let c = h.rect(0).center();
        h.move_to(c);
        h.button(c, PointerButton::Secondary, true);
        h.button(c, PointerButton::Secondary, false);
        h.key(Key::Escape, None, true);
        let ms = start.elapsed().as_secs_f64() * 1000.0;
        eprintln!("{}: 36 frames with picking in {ms:.1} ms", path.display());
    }
}

#[test]
fn views_and_headers_never_take_the_keyboard_focus() {
    let mut h = Harness::new();
    for _ in 0..12 {
        h.key(Key::Tab, None, true);
        h.key(Key::Tab, None, false);
        assert_eq!(h.ctx.memory(|m| m.focused()), None, "Tab focused a viewport widget");
    }
    let c = h.rect(0).center();
    h.button(c, PointerButton::Primary, true);
    h.button(c, PointerButton::Primary, false);
    let header = h.rect(3).min + vec2(14.0, 14.0);
    h.button(header, PointerButton::Primary, true);
    h.button(header, PointerButton::Primary, false);
    h.key(Key::Escape, None, true);
    h.frame(vec![]);
    assert_eq!(h.ctx.memory(|m| m.focused()), None);
}

#[test]
fn the_render_mode_menu_lists_every_mode_once() {
    use hp_app::settings::RenderModeSetting as M;
    let all = [
        M::Textured,
        M::Flat,
        M::Wireframe,
        M::TexturedWire,
        M::Lit,
        M::Unshaded,
        M::SmoothingGroups,
        M::Baked,
        M::LightmapGrid,
        M::ColorCorrected,
    ];
    for m in all {
        match m {
            M::Textured
            | M::Flat
            | M::Wireframe
            | M::TexturedWire
            | M::Lit
            | M::Unshaded
            | M::SmoothingGroups
            | M::Baked
            | M::LightmapGrid
            | M::ColorCorrected => {}
        }
        assert_eq!(crate::RENDER_MODES.iter().filter(|(x, _)| *x == m).count(), 1, "{m:?}");
    }
    assert_eq!(crate::RENDER_MODES.len(), all.len());
    assert!(crate::RENDER_MODES.contains(&(M::Baked, hp_app::actions::Action::RenderBaked)));
    assert!(crate::RENDER_MODES.contains(&(M::LightmapGrid, hp_app::actions::Action::RenderLightmapGrid)));
}

fn texts(h: &Harness) -> Vec<String> {
    let Some(update) = h.last_output.as_ref().and_then(|o| o.accesskit_update.clone()) else { return Vec::new() };
    let mut seen: Vec<(String, [i32; 2])> = Vec::new();
    for (_, n) in &update.nodes {
        let text = [n.label(), n.value()].into_iter().flatten().collect::<Vec<_>>().join(" ");
        let at = n.bounds().map_or([0, 0], |b| [((b.x0 + b.x1) * 0.5) as i32, ((b.y0 + b.y1) * 0.5) as i32]);
        if !text.is_empty() && !seen.iter().any(|(t, p)| *t == text && *p == at) {
            seen.push((text, at));
        }
    }
    seen.into_iter().map(|(t, _)| t).collect()
}

#[test]
fn headers_show_the_grid_only_when_each_view_has_its_own() {
    let _g = hp_app::testing::lang_lock();
    hp_app::i18n::set_lang(hp_app::i18n::Lang::En);
    let mut h = Harness::new();
    h.ctx.enable_accesskit();
    h.frame(vec![]);
    h.frame(vec![]);
    let all = texts(&h);
    assert!(!all.iter().any(|t| t.starts_with("Grid:")), "{all:?}");
    assert_eq!(all.iter().filter(|t| t.ends_with('%')).count(), 3, "the zoom of each 2D view: {all:?}");
    h.state.set_grid_mode(hp_app::settings::GridMode::PerView);
    h.state.settings.grid.per_view.front.size = 12.0;
    h.frame(vec![]);
    h.frame(vec![]);
    let all = texts(&h);
    let grids: Vec<&String> = all.iter().filter(|t| t.starts_with("Grid:")).collect();
    assert_eq!(grids.len(), 4, "one per view: {all:?}");
    assert!(all.iter().any(|t| t.starts_with("Grid: 12")), "{all:?}");
}

#[test]
fn custom_grids_are_drawn_as_lines() {
    let spec = hp_app::settings::GridSpec {
        name: "R".into(),
        step: [16.0, 8.0, 16.0],
        angles: [0.0, 30.0, 0.0],
        bounds: Some([[-128.0, -128.0, -64.0], [128.0, 128.0, 64.0]]),
        ..Default::default()
    };
    let frame = hp_app::grid::GridFrame::from_spec(&spec, 16.0);
    let mut state = test_state();
    let cfg = RenderConfig::from_state(&state);
    let top = params::view_info(
        1,
        ViewKind::Top,
        Rect::from_min_size(Pos2::ZERO, vec2(400.0, 400.0)),
        &state.cameras,
        &cfg,
        1.0,
    );
    let mut out = hp_render::Overlay::default();
    overlay::add_lattice_2d(&mut out, &top, &frame, 8, 4.0);
    assert!(!out.lines.is_empty());
    for l in &out.lines {
        for p in [l.a, l.b] {
            assert!(p[0].abs() <= 128.5 && p[1].abs() <= 128.5, "inside the limits: {p:?}");
        }
        let d = glam::DVec3::new(f64::from(l.b[0] - l.a[0]), f64::from(l.b[1] - l.a[1]), 0.0).normalize();
        assert!(d.dot(frame.axes.x_axis).abs() > 0.999 || d.dot(frame.axes.y_axis).abs() > 0.999, "{d}");
    }
    let mut out = hp_render::Overlay::default();
    let persp = params::view_info(
        0,
        ViewKind::Perspective,
        Rect::from_min_size(Pos2::ZERO, vec2(400.0, 400.0)),
        &state.cameras,
        &cfg,
        1.0,
    );
    overlay::add_grid_plane_3d(&mut out, &persp, &frame, 8);
    assert!(!out.lines.is_empty());
    state.put_custom_grid("", spec, true, false).expect("grid");
    state.use_custom_grid("R").expect("active");
    let cfg = RenderConfig::from_state(&state);
    let top = params::view_info(
        1,
        ViewKind::Top,
        Rect::from_min_size(Pos2::ZERO, vec2(400.0, 400.0)),
        &state.cameras,
        &cfg,
        1.0,
    );
    let shader = params::view_params(&top, &cfg).grid.expect("the plain grid outside the limits");
    assert_eq!(shader.size, 16.0);
    assert_eq!(top.grid, 8.0, "tools get the smallest step");
    let unlimited = hp_app::settings::GridSpec { bounds: None, ..state.map_grids()[0].clone() };
    state.put_custom_grid("R", unlimited, true, false).expect("edit");
    let cfg = RenderConfig::from_state(&state);
    let bare = params::view_params(&top, &cfg).grid.expect("axes and 1024 lines of a rotated grid");
    assert!(!bare.cells, "the lattice is drawn by the overlay");
    state.settings.grid.mode = hp_app::settings::GridMode::Global;
    let cfg = RenderConfig::from_state(&state);
    assert!(params::view_params(&top, &cfg).grid.is_some(), "Hammer's grid: the shader");
}
