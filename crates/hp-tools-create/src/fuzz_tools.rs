#![allow(clippy::unwrap_used, clippy::expect_used, clippy::panic)]

use egui::{Event, Key, Modifiers};
use hp_app::state::AppState;
use hp_app::{Tool, ViewInfo, ViewKind};
use hp_editor::ObjRef;
use hp_render::Overlay;

use crate::common::Mouse;
use crate::fuzz3d::{Rng, bad_solids, random_mods, random_pos, random_view, varied_state};
use crate::tests_support::info_2d;
use crate::{
    BevelTool, BlockTool, CordonTool, CurveTool, DecalTool, EntityTool, OverlayTool, PathTool, PipeTool, PolygonTool,
    RoadTool, SprinkleTool,
};

trait Driven: Tool {
    fn mouse(&mut self, state: &mut AppState, view: &ViewInfo, m: &Mouse, secondary: bool);
}

macro_rules! driven {
    ($($t:ty),*) => {
        $(impl Driven for $t {
            fn mouse(&mut self, state: &mut AppState, view: &ViewInfo, m: &Mouse, _secondary: bool) {
                self.handle_mouse(state, view, m);
            }
        })*
    };
}

driven!(
    BlockTool,
    PolygonTool,
    CurveTool,
    EntityTool,
    DecalTool,
    OverlayTool,
    PathTool,
    RoadTool,
    PipeTool,
    CordonTool,
    SprinkleTool
);

impl Driven for BevelTool {
    fn mouse(&mut self, state: &mut AppState, view: &ViewInfo, m: &Mouse, secondary: bool) {
        self.handle_mouse(state, view, m, secondary);
    }
}

const VALUES: &[&str] = &[
    "0",
    "1",
    "-1",
    "2",
    "3",
    "5",
    "8",
    "13",
    "64",
    "128",
    "129",
    "256",
    "0.001",
    "0.5",
    "45",
    "90",
    "180",
    "360",
    "720",
    "3600",
    "16384",
    "65536",
    "1e12",
    "-1e12",
    "NaN",
    "inf",
    "on",
    "off",
    "auto",
    "world",
    "func_detail",
    "x",
    "y",
    "z",
    "+x",
    "-y",
    "longest",
    "",
    "1 2 3",
    "128 L 64 U 32 R D",
];

fn option_value(rng: &mut Rng, takes: &str, current: &str) -> String {
    match rng.below(4) {
        0 => current.to_owned(),
        1 => {
            let words: Vec<&str> = takes
                .split(|c: char| !(c.is_ascii_alphanumeric() || c == '_' || c == '.' || c == '-' || c == '+'))
                .filter(|w| (1..=24).contains(&w.len()))
                .collect();
            words.get(rng.below(words.len())).map_or_else(String::new, |w| (*w).to_owned())
        }
        _ => VALUES[rng.below(VALUES.len())].to_owned(),
    }
}

fn press(tool: &mut dyn Tool, state: &mut AppState, ctx: &egui::Context, key: Key, modifiers: Modifiers) {
    let input = egui::RawInput {
        events: vec![Event::Key { key, physical_key: None, pressed: true, repeat: false, modifiers }],
        ..Default::default()
    };
    let mut out = ctx.run_ui(input, |ui| {
        tool.key_input(state, ui.ctx());
    });
    out.textures_delta.clear();
}

fn finite_overlay(tool: &dyn Tool, state: &AppState, view: &ViewInfo) -> bool {
    let mut out = Overlay::default();
    tool.overlay(state, view, &mut out);
    let finite = |p: [f32; 3]| p.iter().all(|c| c.is_finite());
    out.lines.iter().chain(&out.lines_on_top).all(|l| finite(l.a) && finite(l.b))
        && out.triangles.iter().chain(&out.triangles_on_top).all(|t| t.p.iter().all(|p| finite(*p)))
}

fn new_bad(state: &AppState, baseline: &[u32]) -> Vec<u32> {
    bad_solids(state).into_iter().filter(|id| !baseline.contains(id)).collect()
}

fn fuzz<T: Driven>(make: fn() -> T, name: &str, seeds: u64, steps: usize) -> usize {
    let mut changes = 0;
    for seed in 1..=seeds {
        let mut tool = make();
        let mut state = varied_state();
        let baseline = bad_solids(&state);
        let (undo_start, _) = state.doc.history_len();
        let mut rng = Rng(seed.wrapping_mul(0x9E37_79B9_7F4A_7C15) ^ 0xA5A5);
        let ctx = egui::Context::default();
        tool.activate(&mut state);
        for step in 0..steps {
            state.frame.index += 1;
            let view = random_view(&state, &mut rng);
            match rng.below(14) {
                0 => {
                    let ids: Vec<u32> = state.doc.scene().solids().keys().copied().collect();
                    let pick: Vec<ObjRef> = (0..rng.below(4))
                        .filter_map(|_| ids.get(rng.below(ids.len())).map(|id| ObjRef::Solid(*id)))
                        .collect();
                    state.doc.set_selection(&pick);
                }
                1 => {
                    let keys = [Key::Enter, Key::Escape, Key::Backspace, Key::Tab, Key::Delete, Key::Enter];
                    let key = keys[rng.below(keys.len())];
                    let m = if rng.chance(0.2) { Modifiers::SHIFT } else { Modifiers::NONE };
                    press(&mut tool, &mut state, &ctx, key, m);
                }
                2 | 3 => {
                    let options = tool.option_values();
                    if let Some(o) = options.get(rng.below(options.len())) {
                        let v = option_value(&mut rng, o.takes, &o.value);
                        let r = std::panic::catch_unwind(std::panic::AssertUnwindSafe(|| {
                            tool.set_option(&mut state, o.key, &v)
                        }));
                        assert!(r.is_ok(), "{name} seed {seed} step {step}: set_option {}={v:?} panicked", o.key);
                    }
                }
                4 => {
                    if rng.chance(0.5) {
                        state.doc.undo();
                    } else {
                        state.doc.redo();
                    }
                }
                5..=7 => {
                    let p = random_pos(&state, &view, &mut rng);
                    let m = random_mods(&mut rng);
                    tool.mouse(&mut state, &view, &Mouse::hover(p).with_modifiers(m), false);
                    let click = if rng.chance(0.15) { Mouse::double_click(p) } else { Mouse::click(p) };
                    tool.mouse(&mut state, &view, &click.with_modifiers(m), rng.chance(0.2));
                }
                8 | 9 => {
                    let a = random_pos(&state, &view, &mut rng);
                    let m = random_mods(&mut rng);
                    tool.mouse(&mut state, &view, &Mouse::hover(a).with_modifiers(m), false);
                    tool.mouse(&mut state, &view, &Mouse::drag_start(a, a).with_modifiers(m), false);
                    let mut b = a;
                    for _ in 0..rng.below(4) {
                        b = random_pos(&state, &view, &mut rng);
                        tool.mouse(&mut state, &view, &Mouse::drag(a, b).with_modifiers(m), false);
                    }
                    tool.mouse(&mut state, &view, &Mouse::drag_end(a, b).with_modifiers(m), false);
                }
                10 => {
                    let notches = [1.0, -1.0, 3.0, -0.5, 40.0, -40.0][rng.below(6)];
                    tool.wheel(&mut state, &view, notches, random_mods(&mut rng));
                }
                11 => {
                    if rng.chance(0.3) {
                        tool.cancel(&mut state);
                    } else {
                        tool.reactivate(&mut state);
                    }
                }
                _ => {
                    let state_ref = &state;
                    let ok = std::panic::catch_unwind(std::panic::AssertUnwindSafe(|| {
                        let finite = finite_overlay(&tool, state_ref, &view);
                        let _ = tool.status_hint(state_ref);
                        let _ = tool.cursor(state_ref, &view, view.rect.center());
                        finite
                    }));
                    assert!(matches!(ok, Ok(true)), "{name} seed {seed} step {step}: overlay panicked or not finite");
                }
            }
            if step % 50 == 0 {
                let views = [view, info_2d(ViewKind::Top, 0.5)];
                hp_app::testing::run_ui(&mut state, |ui, state| {
                    tool.options_ui(ui, state);
                    for v in &views {
                        tool.paint(state, v, ui.painter());
                    }
                });
            }
            if step % 20 == 0 {
                let bad = new_bad(&state, &baseline);
                assert!(bad.is_empty(), "{name} seed {seed} step {step}: invalid or sliver solids {bad:?}");
            }
        }
        let (undo_end, _) = state.doc.history_len();
        changes += undo_end.saturating_sub(undo_start);
        while state.doc.undo() {}
        assert!(new_bad(&state, &baseline).is_empty(), "{name} seed {seed}: undo all");
        while state.doc.redo() {}
        assert!(new_bad(&state, &baseline).is_empty(), "{name} seed {seed}: redo all");
        tool.deactivate(&mut state);
    }
    changes
}

#[test]
fn fuzz_the_block_tool() {
    assert!(fuzz(BlockTool::new, "block", 12, 900) > 10);
}

#[test]
fn fuzz_the_polygon_tool() {
    fuzz(PolygonTool::new, "polygon", 10, 900);
}

#[test]
fn fuzz_the_curve_tool() {
    fuzz(CurveTool::new, "curve", 10, 900);
}

#[test]
fn fuzz_the_entity_tool() {
    fuzz(EntityTool::new, "entity", 8, 700);
}

#[test]
fn fuzz_the_decal_and_overlay_tools() {
    fuzz(DecalTool::new, "decal", 6, 600);
    fuzz(OverlayTool::new, "overlay", 8, 700);
}

#[test]
fn fuzz_the_path_tool() {
    fuzz(PathTool::new, "path", 8, 700);
}

#[test]
fn fuzz_the_road_tool() {
    fuzz(RoadTool::new, "road", 10, 900);
}

#[test]
fn fuzz_the_pipe_tool() {
    fuzz(PipeTool::new, "pipe", 10, 900);
}

#[test]
fn fuzz_the_cordon_tool() {
    fuzz(CordonTool::new, "cordon", 6, 600);
}

#[test]
fn fuzz_the_bevel_tool() {
    fuzz(BevelTool::new, "bevel", 10, 900);
}

#[test]
fn fuzz_the_sprinkle_tool() {
    fuzz(SprinkleTool::new, "sprinkle", 6, 600);
}
