use egui::{Event, Key, Modifiers, Pos2, RawInput, vec2};
use hp_app::testing::test_state;
use hp_app::{ToolFlow, ToolId};

use super::*;

struct QuietTool;

impl Tool for QuietTool {
    fn id(&self) -> ToolId {
        ToolId::Select
    }

    fn view_input(&mut self, _state: &mut AppState, _input: &ViewInput<'_>) -> ToolFlow {
        ToolFlow::Handled
    }
}

fn frame(ctx: &egui::Context, vp: &mut Viewports, state: &mut AppState, time: &mut f64, events: Vec<Event>) {
    *time += 1.0 / 60.0;
    state.frame.dt = 1.0 / 60.0;
    let input = RawInput {
        screen_rect: Some(egui::Rect::from_min_size(Pos2::ZERO, vec2(1600.0, 900.0))),
        time: Some(*time),
        events,
        focused: true,
        ..Default::default()
    };
    let mut out = ctx.run_ui(input, |ui| {
        egui::CentralPanel::default().show(ui, |ui| vp.ui(ui, state, &mut QuietTool));
    });
    out.textures_delta.clear();
}

fn z_press() -> Event {
    Event::Key { key: Key::Z, physical_key: Some(Key::Z), pressed: true, repeat: false, modifiers: Modifiers::NONE }
}

fn z_release() -> Event {
    Event::Key { key: Key::Z, physical_key: Some(Key::Z), pressed: false, repeat: false, modifiers: Modifiers::NONE }
}

#[test]
fn z_is_left_to_a_modal_transform() {
    register_translations();
    let ctx = egui::Context::default();
    let mut vp = Viewports::new();
    let mut state = test_state();
    let mut time = 0.0;
    frame(&ctx, &mut vp, &mut state, &mut time, vec![]);
    frame(&ctx, &mut vp, &mut state, &mut time, vec![]);
    let Some((kind, rect)) = vp.view_rects()[0] else { panic!("the 3D view is visible") };
    assert_eq!(kind, ViewKind::Perspective);
    let p = rect.center() + vec2(20.0, 20.0);
    frame(&ctx, &mut vp, &mut state, &mut time, vec![Event::PointerMoved(p)]);

    state.runtime.selection.modal_transform = true;
    frame(&ctx, &mut vp, &mut state, &mut time, vec![z_press()]);
    assert!(!vp.is_mouselook(), "Z belongs to the modal transform");
    let kept = ctx.input(|i| i.events.iter().any(|e| matches!(e, Event::Key { key: Key::Z, pressed: true, .. })));
    assert!(kept, "the Z press is still there for the tool");
    frame(&ctx, &mut vp, &mut state, &mut time, vec![z_release()]);

    state.runtime.selection.modal_transform = false;
    frame(&ctx, &mut vp, &mut state, &mut time, vec![z_press()]);
    assert!(vp.is_mouselook());
}

#[test]
fn z_is_left_to_a_block_draw_axis_lock() {
    register_translations();
    let ctx = egui::Context::default();
    let mut vp = Viewports::new();
    let mut state = test_state();
    let mut time = 0.0;
    frame(&ctx, &mut vp, &mut state, &mut time, vec![]);
    frame(&ctx, &mut vp, &mut state, &mut time, vec![]);
    let Some((_, rect)) = vp.view_rects()[0] else { panic!("the 3D view is visible") };
    frame(&ctx, &mut vp, &mut state, &mut time, vec![Event::PointerMoved(rect.center())]);
    state.runtime.brush.block_axis_keys = true;
    frame(&ctx, &mut vp, &mut state, &mut time, vec![z_press()]);
    assert!(!vp.is_mouselook(), "Z locks the block draw to Z");
    frame(&ctx, &mut vp, &mut state, &mut time, vec![z_release()]);
    state.runtime.brush.block_axis_keys = false;
    frame(&ctx, &mut vp, &mut state, &mut time, vec![z_press()]);
    assert!(vp.is_mouselook());
}

#[test]
fn digits_are_left_to_a_typed_rotation_in_2d() {
    register_translations();
    let ctx = egui::Context::default();
    let mut vp = Viewports::new();
    let mut state = test_state();
    let mut time = 0.0;
    frame(&ctx, &mut vp, &mut state, &mut time, vec![]);
    frame(&ctx, &mut vp, &mut state, &mut time, vec![]);
    let Some((kind, rect)) = vp.view_rects().iter().flatten().copied().find(|(k, _)| k.is_2d()) else {
        panic!("a 2D view is visible")
    };
    let p = rect.center() + vec2(10.0, 10.0);
    frame(&ctx, &mut vp, &mut state, &mut time, vec![Event::PointerMoved(p)]);
    let nine = |pressed| Event::Key {
        key: Key::Num9,
        physical_key: Some(Key::Num9),
        pressed,
        repeat: false,
        modifiers: Modifiers::NONE,
    };
    let before = state.cameras.cam2d(kind).zoom;
    state.runtime.selection.typing_angle = true;
    frame(&ctx, &mut vp, &mut state, &mut time, vec![nine(true)]);
    assert_eq!(state.cameras.cam2d(kind).zoom, before, "the digit belongs to the typed angle");
    let kept = ctx.input(|i| i.events.iter().any(|e| matches!(e, Event::Key { key: Key::Num9, pressed: true, .. })));
    assert!(kept, "the digit is still there for the tool");
    frame(&ctx, &mut vp, &mut state, &mut time, vec![nine(false)]);
    state.runtime.selection.typing_angle = false;
    frame(&ctx, &mut vp, &mut state, &mut time, vec![nine(true)]);
    assert_ne!(state.cameras.cam2d(kind).zoom, before, "without a rotation the digit zooms like Hammer");
}

#[test]
fn a_press_in_a_view_takes_the_keyboard_from_a_field() {
    register_translations();
    let ctx = egui::Context::default();
    let mut vp = Viewports::new();
    let mut state = test_state();
    let mut time = 0.0;
    let mut text = String::from("value");
    let mut focus = true;
    let mut run = |vp: &mut Viewports, state: &mut AppState, time: &mut f64, events: Vec<Event>| {
        *time += 1.0 / 60.0;
        let input = RawInput {
            screen_rect: Some(egui::Rect::from_min_size(Pos2::ZERO, vec2(1600.0, 900.0))),
            time: Some(*time),
            events,
            focused: true,
            ..Default::default()
        };
        let mut out = ctx.run_ui(input, |ui| {
            egui::Panel::right("panel").show(ui, |ui| {
                let r = ui.text_edit_singleline(&mut text);
                if std::mem::take(&mut focus) {
                    r.request_focus();
                }
            });
            egui::CentralPanel::default().show(ui, |ui| vp.ui(ui, state, &mut QuietTool));
        });
        out.textures_delta.clear();
    };
    run(&mut vp, &mut state, &mut time, vec![]);
    run(&mut vp, &mut state, &mut time, vec![]);
    assert!(ctx.text_edit_focused());
    let Some((_, rect)) = vp.view_rects()[1] else { panic!("the Top view is visible") };
    let (a, b) = (rect.center(), rect.center() + vec2(40.0, 0.0));
    let press = |pos: Pos2, pressed: bool| Event::PointerButton {
        pos,
        button: egui::PointerButton::Primary,
        pressed,
        modifiers: Modifiers::NONE,
    };
    run(&mut vp, &mut state, &mut time, vec![Event::PointerMoved(a)]);
    assert!(ctx.text_edit_focused());
    run(&mut vp, &mut state, &mut time, vec![press(a, true)]);
    run(&mut vp, &mut state, &mut time, vec![Event::PointerMoved(b)]);
    run(&mut vp, &mut state, &mut time, vec![press(b, false)]);
    assert!(!ctx.text_edit_focused(), "a drag in a view takes the keyboard");
    assert!(ctx.memory(|m| m.focused()).is_none());
}

#[derive(Default)]
struct FlyClicks {
    clicks: Vec<Pos2>,
}

impl Tool for FlyClicks {
    fn id(&self) -> ToolId {
        ToolId::Select
    }

    fn view_input(&mut self, _state: &mut AppState, _input: &ViewInput<'_>) -> ToolFlow {
        ToolFlow::Handled
    }

    fn click_while_flying(
        &mut self,
        _state: &mut AppState,
        _view: &hp_app::view::ViewInfo,
        pos: Pos2,
        _modifiers: Modifiers,
    ) -> ToolFlow {
        self.clicks.push(pos);
        ToolFlow::Handled
    }
}

fn tool_frame(
    ctx: &egui::Context,
    vp: &mut Viewports,
    state: &mut AppState,
    tool: &mut dyn Tool,
    time: &mut f64,
    events: Vec<Event>,
) {
    *time += 1.0 / 60.0;
    state.frame.dt = 1.0 / 60.0;
    let input = RawInput {
        screen_rect: Some(egui::Rect::from_min_size(Pos2::ZERO, vec2(1600.0, 900.0))),
        time: Some(*time),
        events,
        focused: true,
        ..Default::default()
    };
    let mut out = ctx.run_ui(input, |ui| {
        egui::CentralPanel::default().show(ui, |ui| vp.ui(ui, state, tool));
    });
    out.textures_delta.clear();
}

#[test]
fn left_click_while_flying_reaches_the_tool() {
    register_translations();
    let ctx = egui::Context::default();
    let mut vp = Viewports::new();
    let mut state = test_state();
    let mut tool = FlyClicks::default();
    let mut time = 0.0;
    let mut run = |vp: &mut Viewports, state: &mut AppState, tool: &mut FlyClicks, events: Vec<Event>| {
        tool_frame(&ctx, vp, state, tool, &mut time, events);
    };
    run(&mut vp, &mut state, &mut tool, vec![]);
    run(&mut vp, &mut state, &mut tool, vec![]);
    let Some((_, rect)) = vp.view_rects()[0] else { panic!("the 3D view is visible") };
    let anchor = rect.center();
    run(&mut vp, &mut state, &mut tool, vec![Event::PointerMoved(anchor + vec2(30.0, 10.0))]);
    run(&mut vp, &mut state, &mut tool, vec![z_press()]);
    assert!(vp.is_mouselook());
    run(&mut vp, &mut state, &mut tool, vec![z_release(), Event::PointerMoved(anchor)]);
    run(&mut vp, &mut state, &mut tool, vec![Event::MouseMoved(vec2(4.0, 0.0)), Event::PointerMoved(anchor)]);
    run(&mut vp, &mut state, &mut tool, vec![]);
    let button = |pressed| Event::PointerButton {
        pos: anchor,
        button: egui::PointerButton::Primary,
        pressed,
        modifiers: Modifiers::NONE,
    };
    run(&mut vp, &mut state, &mut tool, vec![button(true)]);
    run(&mut vp, &mut state, &mut tool, vec![button(false)]);
    run(&mut vp, &mut state, &mut tool, vec![]);
    assert!(vp.is_mouselook(), "the click keeps flying");
    assert_eq!(tool.clicks, [anchor], "one click at the crosshair");

    let Some((_, other)) = vp.view_rects()[3] else { panic!("a 2D view is visible") };
    let away = other.center();
    run(&mut vp, &mut state, &mut tool, vec![Event::PointerMoved(away)]);
    run(&mut vp, &mut state, &mut tool, vec![]);
    let at = |p: Pos2, pressed| Event::PointerButton {
        pos: p,
        button: egui::PointerButton::Primary,
        pressed,
        modifiers: Modifiers::NONE,
    };
    run(&mut vp, &mut state, &mut tool, vec![at(away, true)]);
    run(&mut vp, &mut state, &mut tool, vec![at(away, false)]);
    run(&mut vp, &mut state, &mut tool, vec![]);
    assert!(vp.is_mouselook());
    assert_eq!(tool.clicks, [anchor, anchor], "the hidden cursor off the 3D view still clicks at the crosshair");
}

#[test]
fn fly_hover_follows_the_locked_view() {
    assert_eq!(fly_hover(Some(3), Some(0), true), Some(0));
    assert_eq!(fly_hover(None, Some(0), true), Some(0));
    assert_eq!(fly_hover(Some(3), Some(0), false), Some(3));
    assert_eq!(fly_hover(Some(3), None, true), Some(3));
}
