#![allow(clippy::unwrap_used, clippy::expect_used, clippy::panic)]

use egui::{Event, Key, Modifiers, Pos2};
use glam::DVec3;
use hp_app::state::AppState;
use hp_app::{Tool, ViewInfo, ViewKind};
use hp_editor::{FaceRef, ObjRef, Owner};
use hp_render::Overlay;

use crate::common::Mouse;
use crate::tests_support::{info_2d, info_3d, settle};
use crate::{ClipTool, ExtrudeTool};

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

fn hull(state: &mut AppState, points: &[DVec3], owner: Owner) {
    let mut next = 50_000u32;
    let mut gen_id = || {
        next += 1;
        next
    };
    let sides = hp_geom::hull_solid(points, "DEV/DEV_MEASUREGENERIC01B", 0.25, 16, &mut gen_id).expect("hull");
    state.doc.create_solid(sides, owner).expect("solid");
}

fn rotated_box(c: DVec3, half: DVec3, yaw: f64, pitch: f64) -> Vec<DVec3> {
    let r = glam::DQuat::from_rotation_z(yaw.to_radians()) * glam::DQuat::from_rotation_x(pitch.to_radians());
    let mut out = Vec::new();
    for i in 0..8 {
        let s = DVec3::new(
            if i & 1 == 0 { -1.0 } else { 1.0 },
            if i & 2 == 0 { -1.0 } else { 1.0 },
            if i & 4 == 0 { -1.0 } else { 1.0 },
        );
        out.push(c + r * (half * s));
    }
    out
}

pub(crate) fn varied_state() -> AppState {
    let mut state = settle();
    hull(
        &mut state,
        &rotated_box(DVec3::new(-160.0, 120.0, 64.0), DVec3::new(48.0, 6.0, 64.0), 33.0, 0.0),
        Owner::World,
    );
    hull(
        &mut state,
        &rotated_box(DVec3::new(150.0, -150.0, 40.0), DVec3::new(40.0, 30.0, 20.0), 17.0, 28.0),
        Owner::World,
    );
    hull(&mut state, &rotated_box(DVec3::new(0.0, -60.0, 0.5), DVec3::splat(0.5), 0.0, 0.0), Owner::World);
    hull(
        &mut state,
        &[
            DVec3::new(-200.0, -200.0, 0.0),
            DVec3::new(-100.0, -200.0, 0.0),
            DVec3::new(-200.0, -100.0, 0.0),
            DVec3::new(-100.0, -100.0, 0.0),
            DVec3::new(-200.0, -200.0, 96.0),
            DVec3::new(-200.0, -100.0, 96.0),
        ],
        Owner::World,
    );
    let ring: Vec<DVec3> = (0..24)
        .flat_map(|i| {
            let a = f64::from(i) * std::f64::consts::TAU / 24.0;
            let p = DVec3::new(220.0 + 40.0 * a.cos(), 120.0 + 40.0 * a.sin(), 0.0);
            [p, p + DVec3::Z * 180.0]
        })
        .collect();
    hull(&mut state, &ring, Owner::Entity(6));
    hull(
        &mut state,
        &rotated_box(DVec3::new(0.0, 900.0, 0.0), DVec3::new(15000.0, 8.0, 15000.0), 0.0, 0.0),
        Owner::World,
    );
    hull(&mut state, &rotated_box(DVec3::new(-300.0, 300.0, 3.0), DVec3::new(64.0, 64.0, 3.0), 0.0, 0.0), Owner::World);
    let disp_solid = state.doc.scene().solids().keys().copied().max().expect("solid");
    let top = state
        .doc
        .scene()
        .solid(disp_solid)
        .and_then(|n| n.geom.faces.iter().find(|f| f.plane.normal.z > 0.9).map(|f| f.side_index))
        .expect("top");
    state.doc.create_displacements(&[FaceRef::new(disp_solid, top)], 3).expect("disp");
    state
}

fn random_view(state: &AppState, rng: &mut Rng) -> ViewInfo {
    match rng.below(6) {
        0 => info_2d(ViewKind::Top, rng.range(0.05, 4.0)),
        1 => info_2d(ViewKind::Front, rng.range(0.05, 4.0)),
        2 => info_2d(ViewKind::Side, rng.range(0.05, 4.0)),
        3 => {
            let pos = DVec3::new(rng.range(-500.0, 500.0), rng.range(-500.0, 500.0), rng.range(-40.0, 400.0));
            info_3d(pos, rng.range(-180.0, 180.0), rng.range(-89.0, 89.0))
        }
        _ => {
            let target = surface_point(state, rng).unwrap_or(DVec3::ZERO);
            let dir =
                DVec3::new(rng.range(-1.0, 1.0), rng.range(-1.0, 1.0), rng.range(-1.0, 1.0)).normalize_or(DVec3::X);
            let pos = target + dir * rng.range(8.0, 700.0);
            let look = target - pos;
            let yaw = look.y.atan2(look.x).to_degrees();
            let pitch = (look.z / look.length()).asin().to_degrees().clamp(-89.0, 89.0);
            info_3d(pos, yaw, pitch)
        }
    }
}

fn surface_point(state: &AppState, rng: &mut Rng) -> Option<DVec3> {
    let solids: Vec<_> = state.doc.scene().solids().values().collect();
    let node = solids.get(rng.below(solids.len()))?;
    let face = node.geom.faces.get(rng.below(node.geom.faces.len()))?;
    let n = face.vertices.len();
    if n == 0 {
        return None;
    }
    let a = face.vertices[rng.below(n)];
    let b = face.vertices[rng.below(n)];
    Some(match rng.below(4) {
        0 => a,
        1 => (a + b) * 0.5,
        2 => a.lerp(face.center(), rng.range(0.0, 0.05)),
        _ => a.lerp(b, rng.unit()).lerp(face.center(), rng.unit()),
    })
}

fn random_pos(state: &AppState, view: &ViewInfo, rng: &mut Rng) -> Pos2 {
    if rng.chance(0.75)
        && let Some(p) = surface_point(state, rng)
        && let Some(s) = view.world_to_screen(p)
    {
        let j = if rng.chance(0.5) { 0.0 } else { rng.range(-6.0, 6.0) as f32 };
        return Pos2::new(s.x + j, s.y - j);
    }
    let r = view.rect;
    Pos2::new(
        rng.range(f64::from(r.min.x) - 20.0, f64::from(r.max.x) + 20.0) as f32,
        rng.range(f64::from(r.min.y), f64::from(r.max.y)) as f32,
    )
}

fn random_mods(rng: &mut Rng) -> Modifiers {
    Modifiers { shift: rng.chance(0.2), ctrl: rng.chance(0.15), alt: rng.chance(0.1), ..Modifiers::NONE }
}

fn keys(tool: &mut dyn Tool, state: &mut AppState, ctx: &egui::Context, rng: &mut Rng) {
    let all = [Key::Enter, Key::Escape, Key::Backspace, Key::Tab, Key::C, Key::X, Key::O, Key::Enter, Key::Enter];
    let key = all[rng.below(all.len())];
    let modifiers = if rng.chance(0.2) { Modifiers::SHIFT } else { Modifiers::NONE };
    let input = egui::RawInput {
        events: vec![Event::Key { key, physical_key: None, pressed: true, repeat: false, modifiers }],
        ..Default::default()
    };
    let mut out = ctx.run_ui(input, |ui| {
        tool.key_input(state, ui.ctx());
    });
    out.textures_delta.clear();
}

fn draw(tool: &dyn Tool, state: &AppState, view: &ViewInfo) {
    let mut out = Overlay::default();
    tool.overlay(state, view, &mut out);
    let finite = |p: [f32; 3]| p.iter().all(|c| c.is_finite());
    assert!(out.lines.iter().chain(&out.lines_on_top).all(|l| finite(l.a) && finite(l.b)), "finite overlay lines");
    assert!(out.triangles.iter().chain(&out.triangles_on_top).all(|t| t.p.iter().all(|p| finite(*p))));
    let _ = tool.status_hint(state);
    let _ = tool.cursor(state, view, view.rect.center());
}

trait FuzzTool: Tool {
    fn mouse(&mut self, state: &mut AppState, view: &ViewInfo, m: &Mouse);
}

impl FuzzTool for ExtrudeTool {
    fn mouse(&mut self, state: &mut AppState, view: &ViewInfo, m: &Mouse) {
        self.handle_mouse(state, view, m);
    }
}

impl FuzzTool for ClipTool {
    fn mouse(&mut self, state: &mut AppState, view: &ViewInfo, m: &Mouse) {
        self.handle_mouse(state, view, m);
    }
}

fn bad_solids(state: &AppState) -> Vec<u32> {
    let mut bad = Vec::new();
    for n in state.doc.scene().solids().values() {
        if n.solid.sides.iter().any(|s| s.disp.is_some()) {
            continue;
        }
        let size = n.geom.aabb.size();
        if !n.geom.valid || !(size.min_element() > 0.05 || n.geom.volume() > 0.01) {
            bad.push(n.solid.id);
        }
    }
    bad
}

fn check_scene(state: &AppState, baseline: &[u32]) {
    let new: Vec<u32> = bad_solids(state).into_iter().filter(|id| !baseline.contains(id)).collect();
    assert!(new.is_empty(), "invalid or sliver solids made: {new:?}");
}

fn run<T: FuzzTool>(tool: T, seed: u64, steps: usize) -> usize {
    run_on(tool, varied_state(), seed, steps)
}

fn run_on<T: FuzzTool>(mut tool: T, mut state: AppState, seed: u64, steps: usize) -> usize {
    let mut edits = 0;
    let mut rng = Rng(seed);
    let baseline = bad_solids(&state);
    let ctx = egui::Context::default();
    for step in 0..steps {
        state.frame.index += 1;
        let view = random_view(&state, &mut rng);
        match rng.below(12) {
            0 => {
                let ids: Vec<u32> = state.doc.scene().solids().keys().copied().collect();
                let mut pick: Vec<ObjRef> = Vec::new();
                for _ in 0..rng.below(4) {
                    if let Some(id) = ids.get(rng.below(ids.len())) {
                        pick.push(ObjRef::Solid(*id));
                    }
                }
                if rng.chance(0.2) {
                    pick.push(ObjRef::Entity(6));
                }
                state.doc.set_selection(&pick);
            }
            1 => keys(&mut tool, &mut state, &ctx, &mut rng),
            2 => {
                let opts: &[(&str, &[&str])] = &[
                    ("mode", &["carve", "extrude"]),
                    ("depth", &["0.125", "1", "16", "300", "16384"]),
                    ("through", &["on", "off"]),
                    ("keep", &["front", "back", "both"]),
                    ("axis", &["auto", "x", "y", "z"]),
                    ("three_point", &["on", "off"]),
                ];
                let (k, vals) = opts[rng.below(opts.len())];
                tool.set_option(&mut state, k, vals[rng.below(vals.len())]);
            }
            3 => {
                if rng.chance(0.5) {
                    state.doc.undo();
                } else {
                    state.doc.redo();
                }
            }
            4..=6 => {
                let p = random_pos(&state, &view, &mut rng);
                let m = random_mods(&mut rng);
                tool.mouse(&mut state, &view, &Mouse::hover(p).with_modifiers(m));
                let click = if rng.chance(0.1) { Mouse::double_click(p) } else { Mouse::click(p) };
                tool.mouse(&mut state, &view, &click.with_modifiers(m));
            }
            7 | 8 => {
                let a = random_pos(&state, &view, &mut rng);
                let m = random_mods(&mut rng);
                tool.mouse(&mut state, &view, &Mouse::hover(a).with_modifiers(m));
                tool.mouse(&mut state, &view, &Mouse::drag_start(a, a).with_modifiers(m));
                let mut b = a;
                for _ in 0..rng.below(4) {
                    b = random_pos(&state, &view, &mut rng);
                    tool.mouse(&mut state, &view, &Mouse::drag(a, b).with_modifiers(m));
                }
                tool.mouse(&mut state, &view, &Mouse::drag_end(a, b).with_modifiers(m));
            }
            9 => {
                let p = random_pos(&state, &view, &mut rng);
                tool.mouse(&mut state, &view, &Mouse::hover(p).with_modifiers(random_mods(&mut rng)));
            }
            10 => {
                if rng.chance(0.3) {
                    tool.cancel(&mut state);
                } else {
                    tool.reactivate(&mut state);
                }
            }
            _ => {
                let state_ref = &state;
                let r = std::panic::catch_unwind(std::panic::AssertUnwindSafe(|| draw(&tool, state_ref, &view)));
                assert!(r.is_ok(), "seed {seed} step {step}: drawing panicked");
            }
        }
        let made = state
            .doc
            .undo_labels()
            .iter()
            .filter(|l| ["Clip", "Carve", "Extrude"].iter().any(|k| l.contains(k)))
            .count();
        edits = edits.max(made);
        if step % 60 == 0 {
            let views = [view, info_2d(ViewKind::Top, 0.5)];
            hp_app::testing::run_ui(&mut state, |ui, state| {
                tool.options_ui(ui, state);
                for v in &views {
                    tool.paint(state, v, ui.painter());
                }
            });
        }
        if step % 25 == 0 {
            draw(&tool, &state, &view);
            check_scene(&state, &baseline);
        }
    }
    check_scene(&state, &baseline);
    edits
}

#[test]
fn varied_state_is_valid() {
    let state = varied_state();
    assert!(bad_solids(&state).is_empty());
    assert!(state.doc.scene().solids().len() >= 10);
}

#[test]
fn fuzz_the_extrude_tool() {
    let mut edits = 0;
    for seed in 1..=24u64 {
        edits += run(ExtrudeTool::new(), seed.wrapping_mul(0x9E37_79B9_7F4A_7C15) | 1, 1500);
    }
    assert!(edits > 50, "the fuzzing made real edits: {edits}");
}

#[test]
fn fuzz_the_clip_tool() {
    let mut edits = 0;
    for seed in 1..=24u64 {
        edits += run(ClipTool::new(), seed.wrapping_mul(0xD1B5_4A32_D192_ED03) | 1, 1500);
    }
    assert!(edits > 50, "the fuzzing made real edits: {edits}");
}

#[test]
#[ignore = "needs HP_FUZZ_VMF pointing to a real map"]
fn fuzz_on_a_real_map() {
    let path = std::env::var("HP_FUZZ_VMF").expect("HP_FUZZ_VMF");
    let text = std::fs::read_to_string(path).expect("read");
    let load = || {
        let mut state = settle();
        state.doc = hp_editor::Document::from_map(hp_vmf::parse(&text).expect("parse"));
        state
    };
    let steps: usize = std::env::var("HP_FUZZ_STEPS").ok().and_then(|v| v.parse().ok()).unwrap_or(1500);
    let (mut carved, mut clipped) = (0, 0);
    for seed in 1..=6u64 {
        carved += run_on(ExtrudeTool::new(), load(), seed.wrapping_mul(0x9E37_79B9_7F4A_7C15) | 1, steps);
        clipped += run_on(ClipTool::new(), load(), seed.wrapping_mul(0xD1B5_4A32_D192_ED03) | 1, steps);
    }
    println!("real map fuzz: {carved} carve or extrude edits, {clipped} clips");
    assert!(carved > 0 && clipped > 0);
}
