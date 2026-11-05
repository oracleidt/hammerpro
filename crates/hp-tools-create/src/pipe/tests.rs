#![allow(clippy::unwrap_used, clippy::expect_used, clippy::panic)]

use glam::DVec3;
use hp_app::grid::GridFrame;
use hp_app::{Tool, ViewKind};
use hp_editor::Document;
use hp_render::Overlay;

use super::*;
use crate::common::Mouse;
use crate::tests_support::{info_2d, settle};

fn flat() -> GridFrame {
    GridFrame::uniform(16.0)
}

fn spec(path: &str) -> PipeSpec {
    PipeSpec { moves: parse_moves(path).unwrap(), ..PipeSpec::default() }
}

fn close(a: DVec3, b: DVec3) -> bool {
    a.distance(b) < 1e-6
}

#[test]
fn moves_parse_and_print() {
    let m = parse_moves("256 L 128, up 64; r d").unwrap();
    assert_eq!(
        m,
        vec![
            PipeMove::Forward(256.0),
            PipeMove::Turn(Turn::Left),
            PipeMove::Forward(128.0),
            PipeMove::Turn(Turn::Up),
            PipeMove::Forward(64.0),
            PipeMove::Turn(Turn::Right),
            PipeMove::Turn(Turn::Down),
        ]
    );
    assert_eq!(format_moves(&m), "256 L 128 U 64 R D");
    assert_eq!(parse_moves("f128 l90").unwrap(), vec![PipeMove::Forward(128.0), PipeMove::Turn(Turn::Left)]);
    assert!(parse_moves("128 sideways").is_err());
    assert!(parse_moves("-5").is_err());
    assert!(parse_moves("0").is_err());
    assert!(parse_moves("").unwrap().is_empty());
}

#[test]
fn straight_hollow_square_pipe() {
    let p = plan(&spec("256"), &flat()).unwrap();
    assert_eq!(p.pieces.len(), 4 * 4, "4 brushes along, 4 walls each");
    assert_eq!(p.bends, 0);
    assert!(close(p.end, DVec3::new(256.0, 0.0, 0.0)));
    assert!(close(p.end_direction, DVec3::X));
    assert!((p.length - 256.0).abs() < 1e-9);
    assert_eq!(p.bounds.min, DVec3::new(0.0, -32.0, -32.0));
    assert_eq!(p.bounds.max, DVec3::new(256.0, 32.0, 32.0));
}

#[test]
fn solid_round_pipe_uses_one_brush_per_segment() {
    let s = PipeSpec { hollow: false, shape: Shape::Round, sides: 8, segments: 3, ..spec("192") };
    let p = plan(&s, &flat()).unwrap();
    assert_eq!(p.pieces.len(), 3);
    assert!(p.pieces.iter().all(|x| x.points.len() == 16));
}

#[test]
fn bends_take_radius_off_both_runs() {
    let p = plan(&spec("256 L 256"), &flat()).unwrap();
    assert_eq!(p.bends, 1);
    assert_eq!(p.runs, 2);
    assert_eq!(p.count(PieceKind::Bend), 4 * 4);
    assert_eq!(p.count(PieceKind::Straight), 2 * 4 * 4);
    assert!(close(p.end, DVec3::new(256.0, 256.0, 0.0)));
    assert!(close(p.end_direction, DVec3::Y), "left of +x is +y");
    let expected = 192.0 * 2.0 + 64.0 * std::f64::consts::FRAC_PI_2;
    assert!((p.length - expected).abs() < 1e-9);
    let err = plan(&spec("40 L 200"), &flat()).unwrap_err();
    assert!(matches!(err, PipeError::TooShort { leg: 1, .. }), "{err}");
    assert!(err.to_string().contains("run 1"));
}

#[test]
fn turns_follow_the_up_axis() {
    let p = plan(&spec("256 U 256 R 128"), &flat()).unwrap();
    assert_eq!(p.bends, 2);
    assert!(close(p.end, DVec3::new(256.0, -128.0, 256.0)), "{:?}", p.end);
    assert!(close(p.end_direction, DVec3::NEG_Y));
    let down = plan(&spec("256 D 256"), &flat()).unwrap();
    assert!(close(down.end, DVec3::new(256.0, 0.0, -256.0)));
    let vertical = PipeSpec { direction: DVec3::Z, ..spec("256 L 256") };
    let v = plan(&vertical, &flat()).unwrap();
    assert!(close(v.end, DVec3::new(0.0, -256.0, 256.0)), "{:?}", v.end);
}

#[test]
fn chained_bends_make_an_s_and_a_u() {
    let s = plan(&spec("256 L 256 R 256"), &flat()).unwrap();
    assert!(close(s.end_direction, DVec3::X));
    assert_eq!(s.bends, 2);
    let u = plan(&spec("256 L 256 L 256"), &flat()).unwrap();
    assert!(close(u.end_direction, DVec3::NEG_X));
    assert!(close(u.end, DVec3::new(0.0, 256.0, 0.0)));
    let two = plan(&spec("256 L R 256"), &flat());
    assert!(two.is_err(), "two turns in a row are refused");
}

#[test]
fn trailing_turns_are_dropped() {
    let p = plan(&spec("256 L"), &flat()).unwrap();
    assert_eq!(p.bends, 0);
    assert!(close(p.end_direction, DVec3::X));
    let lead = plan(&spec("L 256"), &flat()).unwrap();
    assert!(close(lead.end_direction, DVec3::Y));
    assert!(close(lead.end, DVec3::new(0.0, 256.0, 0.0)));
}

#[test]
fn sizes_snap_to_the_grid() {
    let s = PipeSpec { radius: 20.0, wall: 5.0, bend_radius: 70.0, ..spec("250 L 250") };
    let p = plan(&s, &flat()).unwrap();
    assert_eq!(p.spec.radius, 16.0);
    assert_eq!(p.spec.wall, 8.0, "walls snap to half a grid step");
    assert_eq!(p.spec.bend_radius, 64.0);
    assert!(close(p.end, DVec3::new(256.0, 256.0, 0.0)), "{:?}", p.end);
    let free = PipeSpec { snap: false, radius: 20.0, wall: 5.0, bend_radius: 70.0, ..spec("250 L 250") };
    let f = plan(&free, &flat()).unwrap();
    assert_eq!(f.spec.radius, 20.0);
    assert!(close(f.end, DVec3::new(250.0, 250.0, 0.0)));
}

#[test]
fn thick_wall_is_refused() {
    let s = PipeSpec { radius: 16.0, wall: 16.0, ..spec("128") };
    assert!(plan(&s, &flat()).is_err());
    let solid = PipeSpec { hollow: false, ..s };
    assert!(plan(&solid, &flat()).is_ok(), "the wall does not matter for a solid pipe");
}

#[test]
fn rotated_and_custom_grids_are_followed() {
    let frame = GridFrame::uniform(16.0).rotated_in_view(ViewKind::Top, 30.0);
    let s = PipeSpec { start: DVec3::new(3.0, 5.0, 0.0), ..spec("256 L 128") };
    let p = plan(&s, &frame).unwrap();
    assert!(close(p.spec.start, frame.snap_point(DVec3::new(3.0, 5.0, 0.0))));
    assert!(close(p.end, frame.snap_point(p.end)), "the end stays on the rotated lattice");
    let axis = frame.axes.x_axis;
    assert!(close(p.spec.direction, axis));
    assert!(close(p.end_direction, frame.axes.y_axis));
    let steps = GridFrame { step: DVec3::new(32.0, 8.0, 16.0), ..GridFrame::uniform(16.0) };
    let q = plan(&spec("100 L 100"), &steps).unwrap();
    assert!(close(q.end, DVec3::new(96.0, 104.0, 0.0)), "x runs in steps of 32, y in steps of 8: {:?}", q.end);
}

#[test]
fn collars_ring_the_pipe() {
    let s = PipeSpec { segments: 4, collar_every: 2, collar_size: 8.0, collar_length: 16.0, ..spec("256") };
    let p = plan(&s, &flat()).unwrap();
    assert_eq!(p.collars, 1);
    assert_eq!(p.count(PieceKind::Collar), 4);
    let ends = PipeSpec { collar_every: 0, collar_ends: true, ..s.clone() };
    assert_eq!(plan(&ends, &flat()).unwrap().collars, 2);
    let bends = PipeSpec { collar_every: 0, collar_bends: true, ..spec("256 L 256") };
    let b = plan(&PipeSpec { collar_size: 8.0, ..bends }, &flat()).unwrap();
    assert_eq!(b.collars, 2);
    let outer = p.pieces.iter().filter(|x| x.kind == PieceKind::Collar).flat_map(|x| x.points.iter());
    let reach = outer.map(|v| v.y.abs().max(v.z.abs())).fold(0.0, f64::max);
    assert!((reach - 40.0).abs() < 1e-9, "radius 32 plus a collar of 8: {reach}");
}

#[test]
fn too_many_brushes_are_refused() {
    let s = PipeSpec { segments: 256, shape: Shape::Round, sides: 48, ..spec("65000 L 65000 R 65000 L 65000") };
    assert!(matches!(plan(&s, &flat()).unwrap_err(), PipeError::TooMany(_)));
}

#[test]
fn points_become_moves() {
    let pts =
        [DVec3::ZERO, DVec3::new(256.0, 0.0, 0.0), DVec3::new(256.0, 256.0, 0.0), DVec3::new(256.0, 256.0, 128.0)];
    let (dir, moves) = moves_from_points(&pts, &flat()).unwrap();
    assert_eq!(dir, DVec3::X);
    assert_eq!(
        moves,
        vec![
            PipeMove::Forward(256.0),
            PipeMove::Turn(Turn::Left),
            PipeMove::Forward(256.0),
            PipeMove::Turn(Turn::Up),
            PipeMove::Forward(128.0)
        ]
    );
    let back = [DVec3::ZERO, DVec3::new(64.0, 0.0, 0.0), DVec3::ZERO];
    assert!(moves_from_points(&back, &flat()).is_err());
    let straight = [DVec3::ZERO, DVec3::new(64.0, 0.0, 0.0), DVec3::new(128.0, 0.0, 0.0)];
    assert_eq!(moves_from_points(&straight, &flat()).unwrap().1, vec![PipeMove::Forward(128.0)]);
}

#[test]
fn creates_valid_grouped_brushes_in_one_undo_step() {
    let mut doc = Document::new_empty();
    let p = plan(&PipeSpec { collar_every: 2, ..spec("256 L 256 U 256") }, &flat()).unwrap();
    let (undo, _) = doc.history_len();
    let made = apply::create(&mut doc, &p, 16, true).unwrap();
    assert_eq!(made.ids.len(), p.pieces.len());
    assert!(made.group.is_some());
    assert_eq!(doc.history_len().0, undo + 1);
    for id in &made.ids {
        let node = doc.scene().solid(*id).unwrap();
        assert!(node.geom.valid, "solid {id}");
    }
    assert_eq!(doc.selected_solid_ids().len(), made.ids.len());
    assert!(doc.undo());
    assert!(doc.scene().solids().is_empty());
    let round = PipeSpec { shape: Shape::Round, sides: 10, ..spec("256 L 256") };
    let r = plan(&round, &flat()).unwrap();
    let made = apply::create(&mut doc, &r, 16, false).unwrap();
    assert!(made.group.is_none());
    assert!(made.ids.iter().all(|id| doc.scene().solid(*id).is_some_and(|n| n.geom.valid)));
}

#[test]
fn translations_are_clean() {
    let problems = hp_app::i18n::lint_table(PIPE_TR);
    assert!(problems.is_empty(), "{problems:#?}");
    for (k, en, fr) in PIPE_TR {
        assert!(k.starts_with("pipe."), "{k}");
        for i in 0..4 {
            let p = format!("{{{i}}}");
            assert_eq!(en.contains(&p), fr.contains(&p), "{k}: placeholder {p}");
        }
    }
    let sources = [
        include_str!("tool.rs"),
        include_str!("build.rs"),
        include_str!("model.rs"),
        include_str!("apply.rs"),
        include_str!("mod.rs"),
        include_str!("tr.rs"),
        include_str!("tests.rs"),
    ];
    let known = |k: &str| {
        PIPE_TR.iter().any(|(c, _, _)| *c == k)
            || crate::TR.iter().any(|(c, _, _)| *c == k)
            || hp_app::strings::TR.iter().any(|(c, _, _)| *c == k)
    };
    for src in sources {
        assert!(!src.contains('\u{2014}') && !src.contains('\u{2013}'));
        for prefix in ["t!(\"", "tf!(\""] {
            let mut rest = src;
            while let Some(i) = rest.find(prefix) {
                let tail = &rest[i + prefix.len()..];
                let end = tail.find('"').unwrap_or(0);
                let key = &tail[..end];
                rest = &tail[end..];
                if key.contains('.') && !key.contains(' ') {
                    assert!(known(key), "missing translation key {key}");
                }
            }
        }
    }
    let used = include_str!("tool.rs");
    for (k, _, _) in PIPE_TR {
        assert!(used.contains(&format!("\"{k}\"")), "unused key {k}");
    }
}

#[test]
fn the_tool_draws_clicks_and_creates() {
    let mut state = settle();
    state.doc.clear_selection();
    let mut tool = PipeTool::new();
    let top = info_2d(ViewKind::Top, 1.0);
    let at = |x: f64, y: f64| top.world_to_screen(DVec3::new(x, y, 0.0)).expect("screen");
    tool.handle_mouse(&mut state, &top, &Mouse::click(at(0.0, 0.0)));
    assert_eq!(tool.points().len(), 1);
    assert!(!tool.is_ready(&state));
    tool.handle_mouse(&mut state, &top, &Mouse::click(at(256.0, 10.0)));
    assert_eq!(tool.points().len(), 2, "the second point is held on an axis");
    assert_eq!(tool.points()[1].y, tool.points()[0].y);
    assert!(tool.is_ready(&state));
    tool.handle_mouse(&mut state, &top, &Mouse::hover(at(256.0, 200.0)));
    let mut out = Overlay::default();
    tool.overlay(&state, &top, &mut out);
    assert!(!out.lines_on_top.is_empty());
    tool.handle_mouse(&mut state, &top, &Mouse::click(at(250.0, 256.0)));
    assert_eq!(tool.points().len(), 3);
    let before = state.doc.stats().solids;
    let (undo, _) = state.doc.history_len();
    tool.handle_mouse(&mut state, &top, &Mouse::double_click(at(250.0, 256.0)));
    assert!(state.doc.stats().solids > before);
    assert_eq!(state.doc.history_len().0, undo + 1, "one undo step");
    assert!(tool.points().is_empty());
    assert!(tool.set_option(&mut state, "path", "128 L 128"));
    assert!(!tool.set_option(&mut state, "path", "bogus"));
    assert!(tool.set_option(&mut state, "radius", "16"));
    assert!(!tool.set_option(&mut state, "radius", "0"));
    assert!(tool.option_values().iter().any(|o| o.key == "radius" && o.value == "16"));
    tool.handle_mouse(&mut state, &top, &Mouse::click(at(-256.0, -256.0)));
    assert!(tool.is_ready(&state), "one point and a typed route");
    assert!(tool.cancel(&mut state));
    assert!(!tool.cancel(&mut state));
}

#[test]
fn the_tool_draws_in_every_view() {
    let mut state = settle();
    let mut tool = PipeTool::new();
    let top = info_2d(ViewKind::Top, 1.0);
    let at = |x: f64, y: f64| top.world_to_screen(DVec3::new(x, y, 0.0)).expect("screen");
    tool.handle_mouse(&mut state, &top, &Mouse::click(at(0.0, 0.0)));
    tool.handle_mouse(&mut state, &top, &Mouse::click(at(256.0, 0.0)));
    for view in [
        top,
        info_2d(ViewKind::Front, 1.0),
        info_2d(ViewKind::Side, 1.0),
        crate::tests_support::info_3d(DVec3::new(-300.0, -300.0, 200.0), 45.0, -20.0),
    ] {
        let mut out = Overlay::default();
        tool.overlay(&state, &view, &mut out);
        assert!(!out.is_empty(), "{:?}", view.kind);
    }
    hp_app::testing::run_ctx(&mut state, |ctx, state| {
        tool.paint(state, &top, &ctx.layer_painter(egui::LayerId::background()));
    });
    hp_app::testing::run_ui(&mut state, |ui, state| tool.options_ui(ui, state));
    assert!(tool.status_hint(&state).is_some_and(|h| !h.is_empty()));
}
