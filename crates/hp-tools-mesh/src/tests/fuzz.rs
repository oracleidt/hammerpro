use hp_render::Overlay;

use super::*;
use crate::disp::PointerFrame;

struct Rng(u64);

impl Rng {
    fn next(&mut self) -> u64 {
        let mut x = self.0;
        x ^= x << 13;
        x ^= x >> 7;
        x ^= x << 17;
        self.0 = x;
        x
    }

    fn below(&mut self, n: usize) -> usize {
        (self.next() % n.max(1) as u64) as usize
    }

    fn unit(&mut self) -> f64 {
        (self.next() >> 11) as f64 / (1u64 << 53) as f64
    }

    fn range(&mut self, lo: f64, hi: f64) -> f64 {
        lo + (hi - lo) * self.unit()
    }

    fn chance(&mut self, p: f64) -> bool {
        self.unit() < p
    }
}

const VALUES: &[&str] = &[
    "0", "1", "-1", "2", "3", "4", "5", "8", "16", "64", "100", "256", "0.001", "0.5", "45", "90", "180", "1e12",
    "-1e12", "NaN", "inf", "on", "off", "auto", "x", "y", "z", "", "1 2 3", "0 0 0",
];

fn random_view(rng: &mut Rng) -> ViewInfo {
    match rng.below(5) {
        0 => view2d(ViewKind::Top, DVec2::new(rng.range(-300.0, 300.0), rng.range(-300.0, 300.0)), rng.range(0.1, 4.0)),
        1 => {
            view2d(ViewKind::Front, DVec2::new(rng.range(-300.0, 300.0), rng.range(-100.0, 200.0)), rng.range(0.1, 4.0))
        }
        2 => {
            view2d(ViewKind::Side, DVec2::new(rng.range(-300.0, 300.0), rng.range(-100.0, 200.0)), rng.range(0.1, 4.0))
        }
        _ => view3d(
            DVec3::new(rng.range(-500.0, 500.0), rng.range(-500.0, 500.0), rng.range(20.0, 500.0)),
            rng.range(-180.0, 180.0),
            rng.range(-89.0, -5.0),
        ),
    }
}

fn surface_pos(state: &AppState, view: &ViewInfo, rng: &mut Rng) -> Pos2 {
    let solids: Vec<_> = state.doc.scene().solids().values().collect();
    if rng.chance(0.8)
        && let Some(n) = solids.get(rng.below(solids.len()))
        && let Some(f) = n.geom.faces.get(rng.below(n.geom.faces.len()))
        && !f.vertices.is_empty()
    {
        let a = f.vertices[rng.below(f.vertices.len())];
        let p = if rng.chance(0.5) { a } else { a.lerp(f.center(), rng.unit()) };
        if let Some(s) = view.world_to_screen(p) {
            return s + vec2(rng.range(-3.0, 3.0) as f32, rng.range(-3.0, 3.0) as f32);
        }
    }
    Pos2::new(rng.range(0.0, 800.0) as f32, rng.range(0.0, 600.0) as f32)
}

fn mods(rng: &mut Rng) -> Modifiers {
    Modifiers {
        shift: rng.chance(0.2),
        ctrl: rng.chance(0.15),
        command: false,
        alt: rng.chance(0.1),
        ..Modifiers::NONE
    }
}

fn bad_solids(state: &AppState) -> Vec<u32> {
    state
        .doc
        .scene()
        .solids()
        .values()
        .filter(|n| n.solid.sides.iter().all(|s| s.disp.is_none()))
        .filter(|n| !n.geom.valid || !(n.geom.aabb.size().min_element() > 0.05 || n.geom.volume() > 0.01))
        .map(|n| n.solid.id)
        .collect()
}

fn finite_overlay(tool: &dyn Tool, state: &AppState, view: &ViewInfo) -> bool {
    let mut out = Overlay::default();
    tool.overlay(state, view, &mut out);
    let finite = |p: [f32; 3]| p.iter().all(|c| c.is_finite());
    out.lines.iter().chain(&out.lines_on_top).all(|l| finite(l.a) && finite(l.b))
        && out.triangles.iter().chain(&out.triangles_on_top).all(|t| t.p.iter().all(|p| finite(*p)))
}

fn finite_disps(state: &AppState) -> bool {
    state.doc.disp_faces().iter().all(|f| {
        state.doc.face_side(*f).and_then(|s| s.disp.as_ref()).is_none_or(|d| {
            d.distances.iter().chain(&d.alphas).all(|v| v.is_finite())
                && d.normals.iter().chain(&d.offsets).all(|v| v.is_finite())
        })
    })
}

fn set_random_option(tool: &mut dyn Tool, state: &mut AppState, rng: &mut Rng, name: &str) {
    let options = tool.option_values();
    let Some(o) = options.get(rng.below(options.len())) else { return };
    let v = match rng.below(3) {
        0 => o.value.clone(),
        1 => {
            let words: Vec<&str> = o
                .takes
                .split(|c: char| !(c.is_ascii_alphanumeric() || c == '_' || c == '.' || c == '-'))
                .filter(|w| (1..=24).contains(&w.len()))
                .collect();
            words.get(rng.below(words.len())).map_or_else(String::new, |w| (*w).to_owned())
        }
        _ => VALUES[rng.below(VALUES.len())].to_owned(),
    };
    let r = std::panic::catch_unwind(std::panic::AssertUnwindSafe(|| tool.set_option(state, o.key, &v)));
    assert!(r.is_ok(), "{name}: set_option {}={v:?} panicked", o.key);
}

fn press(tool: &mut dyn Tool, state: &mut AppState, rng: &mut Rng) {
    let keys = [
        egui::Key::Enter,
        egui::Key::Escape,
        egui::Key::Backspace,
        egui::Key::Delete,
        egui::Key::Tab,
        egui::Key::F,
        egui::Key::E,
    ];
    let key = keys[rng.below(keys.len())];
    let m = if rng.chance(0.3) { ctrl() } else { Modifiers::NONE };
    run_keys(state, tool, &[(key, m)]);
}

fn check(state: &AppState, baseline: &[u32], what: &str) {
    let bad: Vec<u32> = bad_solids(state).into_iter().filter(|id| !baseline.contains(id)).collect();
    assert!(bad.is_empty(), "{what}: invalid or sliver solids {bad:?}");
    assert!(finite_disps(state), "{what}: non finite displacement data");
}

#[test]
fn fuzz_the_vertex_tool() {
    let mut edits = 0;
    for seed in 1..=16u64 {
        let mut state = state_with(&[WALL]);
        let baseline = bad_solids(&state);
        let mut tool = VertexTool::new();
        tool.activate(&mut state);
        let mut rng = Rng(seed.wrapping_mul(0x9E37_79B9_7F4A_7C15) | 1);
        for step in 0..800 {
            state.frame.index += 1;
            let view = random_view(&mut rng);
            let what = format!("vertex seed {seed} step {step}");
            match rng.below(12) {
                0 => {
                    let ids: Vec<u32> = state.doc.scene().solids().keys().copied().collect();
                    let pick: Vec<ObjRef> = (0..1 + rng.below(3))
                        .filter_map(|_| ids.get(rng.below(ids.len())).map(|id| ObjRef::Solid(*id)))
                        .collect();
                    state.doc.select(&pick, SelectOp::Replace);
                }
                1 => press(&mut tool, &mut state, &mut rng),
                2 => set_random_option(&mut tool, &mut state, &mut rng, &what),
                3 => {
                    if rng.chance(0.5) {
                        state.doc.undo();
                    } else {
                        state.doc.redo();
                    }
                }
                4..=6 => {
                    let p = surface_pos(&state, &view, &mut rng);
                    let m = mods(&mut rng);
                    tool.handle(&mut state, &view, Gesture::Hover(p), m);
                    tool.handle(&mut state, &view, Gesture::Click(p), m);
                }
                7 | 8 => {
                    let a = surface_pos(&state, &view, &mut rng);
                    let m = mods(&mut rng);
                    tool.handle(&mut state, &view, Gesture::DragStart { origin: a, pos: a }, m);
                    let mut b = a;
                    for _ in 0..rng.below(4) {
                        b = surface_pos(&state, &view, &mut rng);
                        tool.handle(&mut state, &view, Gesture::Drag(b), m);
                    }
                    tool.handle(&mut state, &view, Gesture::DragEnd(b), m);
                }
                9 => {
                    let actions = [
                        Action::VertexSplitFace,
                        Action::VertexWeld,
                        Action::VertexWeldDistance,
                        Action::VertexCleanup,
                        Action::SnapToGrid,
                        Action::Delete,
                        Action::SelectAll,
                    ];
                    tool.action(&mut state, actions[rng.below(actions.len())]);
                }
                10 => {
                    tool.wheel(&mut state, &view, [1.0, -1.0, 5.0][rng.below(3)], mods(&mut rng));
                }
                _ => {
                    assert!(finite_overlay(&tool, &state, &view), "{what}: overlay");
                    let _ = tool.status_hint(&state);
                }
            }
            if step % 50 == 0 {
                run_ui(&mut state, |ui, state| {
                    tool.options_ui(ui, state);
                    tool.paint(state, &view, ui.painter());
                });
            }
            if step % 10 == 0 {
                check(&state, &baseline, &what);
            }
        }
        edits += undo_steps(&state);
        while state.doc.undo() {}
        check(&state, &baseline, "vertex undo all");
        while state.doc.redo() {}
        check(&state, &baseline, "vertex redo all");
    }
    assert!(edits > 10, "the fuzzing made real edits: {edits}");
}

#[test]
fn fuzz_the_displacement_tool() {
    for seed in 1..=10u64 {
        let (mut state, mut tool, _) = floor_with_disp(2 + (seed % 3) as u32);
        let baseline = bad_solids(&state);
        tool.activate(&mut state);
        let mut rng = Rng(seed.wrapping_mul(0xD1B5_4A32_D192_ED03) | 1);
        for step in 0..600 {
            state.frame.index += 1;
            state.frame.dt = rng.range(0.0, 0.2) as f32;
            let view = random_view(&mut rng);
            let what = format!("displacement seed {seed} step {step}");
            match rng.below(10) {
                0 => set_random_option(&mut tool, &mut state, &mut rng, &what),
                1 => press(&mut tool, &mut state, &mut rng),
                2 => {
                    if rng.chance(0.5) {
                        state.doc.undo();
                    } else {
                        state.doc.redo();
                    }
                }
                3 => {
                    let n = rng.below(6);
                    match n {
                        0 => {
                            tool.create(&mut state, 1 + rng.below(4) as u32);
                        }
                        1 => {
                            tool.sew(&mut state);
                        }
                        2 => {
                            tool.flatten(&mut state);
                        }
                        3 => {
                            tool.set_power(&mut state, rng.below(6) as u32);
                        }
                        4 => {
                            tool.set_elevation(&mut state, rng.range(-1e6, 1e6));
                        }
                        _ => {
                            tool.select_all(&mut state);
                        }
                    }
                }
                4..=7 => {
                    let start = surface_pos(&state, &view, &mut rng);
                    let m = mods(&mut rng);
                    let frame = |pos, pressed, down| PointerFrame {
                        pos: Some(pos),
                        pressed,
                        down,
                        mods: m,
                        right_pressed: false,
                        right_down: false,
                    };
                    tool.handle_pointer(&mut state, &view, frame(start, true, true));
                    let mut p = start;
                    for _ in 0..rng.below(5) {
                        p += vec2(rng.range(-40.0, 40.0) as f32, rng.range(-40.0, 40.0) as f32);
                        tool.handle_pointer(&mut state, &view, frame(p, false, true));
                    }
                    tool.handle_pointer(&mut state, &view, frame(p, false, false));
                }
                8 => {
                    tool.wheel(&mut state, &view, [1.0, -1.0, 4.0][rng.below(3)], mods(&mut rng));
                }
                _ => {
                    assert!(finite_overlay(&tool, &state, &view), "{what}: overlay");
                }
            }
            if step % 50 == 0 {
                run_ui(&mut state, |ui, state| {
                    tool.options_ui(ui, state);
                    tool.paint(state, &view, ui.painter());
                });
            }
            if step % 10 == 0 {
                check(&state, &baseline, &what);
            }
        }
        while state.doc.undo() {}
        check(&state, &baseline, "displacement undo all");
        while state.doc.redo() {}
        check(&state, &baseline, "displacement redo all");
    }
}
