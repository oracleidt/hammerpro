use hp_app::settings::MergePrompt;

use super::*;

const TOP_INNER: DVec3 = DVec3::new(256.0, 256.0, 128.0);
const TOP_OUTER: DVec3 = DVec3::new(256.0, 272.0, 128.0);

fn wall_tool(prompt: MergePrompt) -> (AppState, VertexTool) {
    let mut state = state_with(&[WALL]);
    state.settings.vertex.merge_prompt = prompt;
    let mut tool = VertexTool::new();
    tool.activate(&mut state);
    (state, tool)
}

fn stack_one(state: &mut AppState, tool: &mut VertexTool) -> bool {
    assert_eq!(tool.select_at(state, &[TOP_OUTER]), 1);
    tool.move_selection(state, TOP_INNER - TOP_OUTER, "Move vertices", None)
}

fn wall_points(state: &AppState) -> usize {
    EditSolid::from_doc(&state.doc, WALL).expect("wall").points.len()
}

fn frame_with_keys(state: &mut AppState, tool: &mut VertexTool, keys: &[egui::Key]) {
    let ctx = egui::Context::default();
    for round in 0..2 {
        let events = if round == 1 {
            keys.iter()
                .map(|key| egui::Event::Key {
                    key: *key,
                    physical_key: None,
                    pressed: true,
                    repeat: false,
                    modifiers: Modifiers::NONE,
                })
                .collect()
        } else {
            Vec::new()
        };
        let input = egui::RawInput { screen_rect: Some(rect()), events, ..Default::default() };
        let mut out = ctx.run_ui(input, |ui| tool.update(state, ui.ctx()));
        out.textures_delta.clear();
    }
}

#[test]
fn stacked_vertices_ask_and_yes_merges_in_one_step() {
    let (mut state, mut tool) = wall_tool(MergePrompt::Ask);
    let before = vmf(&state);
    assert!(!stack_one(&mut state, &mut tool));
    assert!(tool.merge_prompt_open(), "the question is asked");
    assert_eq!(vmf(&state), before, "nothing changes while the question is open");
    assert_eq!(undo_steps(&state), 0);
    let mut out = hp_render::Overlay::default();
    tool.overlay(&state, &view2d(ViewKind::Top, DVec2::ZERO, 0.5), &mut out);
    assert!(!out.lines_on_top.is_empty() && !out.points.is_empty(), "the merged shape is previewed");
    assert!(tool.action(&mut state, Action::SnapToGrid), "the tool is blocked meanwhile");
    assert!(tool.answer_merge_prompt(&mut state, true, false));
    assert!(!tool.merge_prompt_open());
    assert_eq!(wall_points(&state), 7, "the two vertices became one");
    assert_eq!(undo_steps(&state), 1, "the move and the merge are one undo step");
    assert_eq!(state.settings.vertex.merge_prompt, MergePrompt::Ask);
    state.doc.undo();
    assert_eq!(vmf(&state), before);
}

#[test]
fn no_puts_the_vertices_back() {
    let (mut state, mut tool) = wall_tool(MergePrompt::Ask);
    let before = vmf(&state);
    stack_one(&mut state, &mut tool);
    assert!(tool.answer_merge_prompt(&mut state, false, false));
    assert!(!tool.merge_prompt_open());
    assert_eq!(vmf(&state), before, "the move is reverted");
    assert_eq!(undo_steps(&state), 0);
    assert!(!state.doc.can_redo(), "nothing left to redo");
    stack_one(&mut state, &mut tool);
    assert!(tool.cancel(&mut state), "escape answers no");
    assert_eq!(vmf(&state), before);
}

#[test]
fn enter_answers_yes_escape_answers_no_and_the_checkbox_stops_asking() {
    let (mut state, mut tool) = wall_tool(MergePrompt::Ask);
    let before = vmf(&state);
    stack_one(&mut state, &mut tool);
    frame_with_keys(&mut state, &mut tool, &[]);
    assert!(tool.merge_prompt_open(), "the dialog stays until it is answered");
    frame_with_keys(&mut state, &mut tool, &[egui::Key::Escape]);
    assert!(!tool.merge_prompt_open());
    assert_eq!(vmf(&state), before);
    stack_one(&mut state, &mut tool);
    frame_with_keys(&mut state, &mut tool, &[egui::Key::Enter]);
    assert!(!tool.merge_prompt_open());
    assert_eq!(wall_points(&state), 7);
    state.doc.undo();
    stack_one(&mut state, &mut tool);
    assert!(tool.answer_merge_prompt(&mut state, true, true));
    assert_eq!(state.settings.vertex.merge_prompt, MergePrompt::Always, "do not ask again");
    assert!(state.settings_dirty);
}

#[test]
fn always_and_never_skip_the_question() {
    let (mut state, mut tool) = wall_tool(MergePrompt::Always);
    assert!(stack_one(&mut state, &mut tool));
    assert!(!tool.merge_prompt_open());
    assert_eq!(wall_points(&state), 7);
    assert_eq!(undo_steps(&state), 1);

    let (mut state, mut tool) = wall_tool(MergePrompt::Never);
    let before = vmf(&state);
    assert!(!stack_one(&mut state, &mut tool));
    assert!(!tool.merge_prompt_open());
    assert_eq!(vmf(&state), before, "never merges: the move is undone");
    assert!(has_toast(&mut state));
}

#[test]
fn moves_that_stack_nothing_are_not_asked() {
    let (mut state, mut tool) = wall_tool(MergePrompt::Ask);
    assert_eq!(tool.select_at(&state, &[TOP_OUTER]), 1);
    assert!(tool.move_selection(&mut state, DVec3::new(0.0, 0.0, 16.0), "Move vertices", None));
    assert!(!tool.merge_prompt_open());
    assert_eq!(wall_points(&state), 8);
}

#[test]
fn a_live_drag_onto_other_vertices_asks_in_2d_and_3d() {
    for live in [true, false] {
        let (mut state, mut tool) = wall_tool(MergePrompt::Ask);
        state.settings.mesh.live_preview = live;
        let before = vmf(&state);
        let front = view2d(ViewKind::Front, DVec2::new(264.0, 64.0), 2.0);
        let grab = front.world_to_screen(TOP_OUTER).expect("screen");
        let to = front.world_to_screen(TOP_INNER).expect("screen");
        tool.handle(&mut state, &front, Gesture::DragStart { origin: grab, pos: grab }, Modifiers::NONE);
        tool.handle(&mut state, &front, Gesture::Drag(to), Modifiers::NONE);
        tool.handle(&mut state, &front, Gesture::DragEnd(to), Modifiers::NONE);
        assert!(tool.merge_prompt_open(), "live {live}");
        assert!(!state.doc.in_transaction());
        assert_eq!(vmf(&state), before, "the live preview is rolled back while asking (live {live})");
        assert!(tool.answer_merge_prompt(&mut state, true, false));
        assert_eq!(wall_points(&state), 6, "both corners under the cursor merged (live {live})");
        assert_eq!(undo_steps(&state), 1);
        assert!(state.doc.scene().solid(WALL).expect("wall").geom.valid);
    }
    let (mut state, mut tool) = wall_tool(MergePrompt::Ask);
    let v = view3d(DVec3::new(200.0, 264.0, 700.0), 90.0, -89.0);
    let grab = v.world_to_screen(TOP_OUTER).expect("screen");
    let to = v.world_to_screen(TOP_INNER).expect("screen");
    tool.handle(&mut state, &v, Gesture::DragStart { origin: grab, pos: grab }, Modifiers::NONE);
    tool.handle(&mut state, &v, Gesture::Drag(to), Modifiers::NONE);
    tool.handle(&mut state, &v, Gesture::DragEnd(to), Modifiers::NONE);
    assert!(tool.merge_prompt_open(), "the 3D view asks too");
    assert!(tool.answer_merge_prompt(&mut state, false, false));
    assert_eq!(wall_points(&state), 8);
}

#[test]
fn off_is_the_default_and_keeps_the_old_silent_merge() {
    let mut state = state_with(&[WALL]);
    assert_eq!(state.settings.vertex.merge_prompt, MergePrompt::Off);
    let mut tool = VertexTool::new();
    tool.activate(&mut state);
    assert!(stack_one(&mut state, &mut tool));
    assert!(!tool.merge_prompt_open(), "no question when the option is off");
    assert_eq!(wall_points(&state), 7, "stacked vertices merge like before");
    assert_eq!(undo_steps(&state), 1);

    let (mut state, mut tool) = wall_tool(MergePrompt::Off);
    let front = view2d(ViewKind::Front, DVec2::new(264.0, 64.0), 2.0);
    let grab = front.world_to_screen(TOP_OUTER).expect("screen");
    let to = front.world_to_screen(TOP_INNER).expect("screen");
    tool.handle(&mut state, &front, Gesture::DragStart { origin: grab, pos: grab }, Modifiers::NONE);
    tool.handle(&mut state, &front, Gesture::Drag(to), Modifiers::NONE);
    tool.handle(&mut state, &front, Gesture::DragEnd(to), Modifiers::NONE);
    assert!(!tool.merge_prompt_open());
    assert_eq!(wall_points(&state), 6);
    assert_eq!(undo_steps(&state), 1);
}
