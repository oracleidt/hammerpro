#![allow(clippy::unwrap_used, clippy::expect_used, clippy::panic)]

use egui::CursorIcon;
use glam::{DVec2, DVec3};
use hp_app::state::AppState;
use hp_app::{Tool, ToolFlow, ToolId, ViewInfo, ViewKind};
use hp_geom::{Aabb, validate_solid};
use hp_render::Overlay;

use super::geom::*;
use super::tool::{CurvePhase, CurveTool};
use super::tr::CURVE_TR;
use crate::block::CreateAs;
use crate::common::Mouse;
use crate::tests_support::{fgd_state, info_2d, info_3d, settle};

fn v(x: f64, y: f64) -> DVec2 {
    DVec2::new(x, y)
}

fn arc(radius: f64, degrees: f64, n: usize) -> Vec<DVec2> {
    (0..=n)
        .map(|k| DVec2::from_angle((degrees * k as f64 / n as f64).to_radians()) * radius)
        .collect()
}

fn spec<'a>(points: &'a [DVec2], segments: u32, thickness: f64) -> CurveSpec<'a> {
    CurveSpec {
        kind: ViewKind::Top,
        points,
        segments,
        thickness,
        depth: (0.0, 64.0),
        material: "dev/dev_measuregeneric01b",
        tex_scale: 0.25,
        lightmap_scale: 16,
    }
}

fn build(s: &CurveSpec<'_>) -> Result<Vec<Vec<hp_vmf::Side>>, BuildError> {
    let mut next = 1u32;
    build_brushes(s, &mut || {
        next += 1;
        next
    })
}

#[test]
fn clamping_of_the_parameters() {
    assert_eq!(clamp_subdivisions(-5), 2);
    assert_eq!(clamp_subdivisions(0), 2);
    assert_eq!(clamp_subdivisions(1), 2);
    assert_eq!(clamp_subdivisions(2), 2);
    assert_eq!(clamp_subdivisions(3), 3);
    assert_eq!(clamp_subdivisions(4096), 4096);
    assert_eq!(clamp_subdivisions(4097), 4096);
    assert_eq!(clamp_subdivisions(i64::MAX), 4096);
    assert_eq!(clamp_thickness(0.0), 1.0);
    assert_eq!(clamp_thickness(-3.0), 1.0);
    assert_eq!(clamp_thickness(f64::NAN), 1.0);
    assert_eq!(clamp_thickness(12.5), 12.5);
    assert_eq!(clamp_thickness(1024.0), 1024.0);
    assert_eq!(clamp_thickness(5000.0), 1024.0);
    assert_eq!(clamp_height(0.0), MIN_HEIGHT);
    assert_eq!(clamp_height(1e9), MAX_HEIGHT);
    let l = layout(&[v(0.0, 0.0), v(4096.0, 0.0)], 1, 0.0).unwrap();
    assert_eq!(l.quads.len(), 2, "1 subdivision is raised to 2");
    let l = layout(&[v(-8000.0, 0.0), v(8000.0, 0.0)], 100_000, 16.0).unwrap();
    assert_eq!(l.quads.len(), 4096, "the count is capped at 4096");
}

#[test]
fn grid_snap_of_the_points() {
    assert_eq!(snap_point(v(3.0, 13.0), 8.0), v(0.0, 16.0));
    assert_eq!(snap_point(v(-5.0, 4.1), 8.0), v(-8.0, 8.0));
    assert_eq!(snap_point(v(-3.9, -0.1), 8.0), v(0.0, 0.0));
    assert!(snap_point(v(-3.9, -0.1), 8.0).x.is_sign_positive(), "no negative zero");
    assert_eq!(snap_point(v(17.0, 31.0), 16.0), v(16.0, 32.0));
    assert_eq!(snap_point(v(1.3, 2.7), 0.0), v(1.3, 2.7), "no grid, no snap");
}

#[test]
fn counts_and_even_spacing() {
    for n in [2u32, 3, 4, 7, 8, 16, 100, 512, 4096] {
        let pts = arc(2048.0, 180.0, 6);
        let l = layout(&pts, n, 16.0).unwrap();
        assert_eq!(l.quads.len(), n as usize, "{n}");
        assert_eq!(l.centers.len(), n as usize + 1);
        assert_eq!(l.centers[0], pts[0], "starts on the first point");
        assert!(l.centers[n as usize].distance(pts[6]) < 1e-9, "ends on the last point");
        let lens: Vec<f64> = l.centers.windows(2).map(|w| w[0].distance(w[1])).collect();
        let mean = l.length / f64::from(n);
        assert!(lens.iter().all(|s| (s - mean).abs() < mean * 0.02), "{n}: even arc length");
        let true_len = std::f64::consts::PI * 2048.0;
        if n >= 64 {
            assert!((l.length - true_len).abs() < true_len * 0.01, "{n}: {} vs {true_len}", l.length);
        }
        assert!(l.length <= true_len * 1.0001, "{n}: the chords are never longer than the arc");
    }
    let straight = layout(&[v(0.0, 0.0), v(256.0, 0.0)], 4, 32.0).unwrap();
    assert_eq!(straight.centers, vec![v(0.0, 0.0), v(64.0, 0.0), v(128.0, 0.0), v(192.0, 0.0), v(256.0, 0.0)]);
    assert_eq!(straight.quads[0], [v(0.0, -16.0), v(64.0, -16.0), v(64.0, 16.0), v(0.0, 16.0)]);
}

#[test]
fn the_spline_goes_through_every_point() {
    let pts = [v(0.0, 0.0), v(128.0, 64.0), v(256.0, -64.0), v(384.0, 0.0)];
    let dense = dense_spline(&pts, 64);
    for p in pts {
        assert!(dense.iter().any(|q| q.distance(p) < 1e-9), "{p}");
    }
    let dup = [v(0.0, 0.0), v(0.0, 0.0), v(64.0, 0.0), v(64.0, 0.0), v(64.0, 64.0)];
    assert_eq!(clean_points(&dup), vec![v(0.0, 0.0), v(64.0, 0.0), v(64.0, 64.0)]);
    assert!(dense_spline(&dup, 8).iter().all(|p| p.is_finite()));
}

#[test]
fn adjacent_brushes_share_their_joint_and_stay_convex() {
    let pts = [v(0.0, 0.0), v(256.0, 128.0), v(512.0, 0.0), v(768.0, 256.0), v(512.0, 640.0)];
    for (n, t) in [(2u32, 8.0), (8, 32.0), (32, 16.0), (64, 64.0), (500, 4.0)] {
        let l = layout(&pts, n, t).unwrap();
        assert!(l.folded.is_empty(), "{n} {t}: {:?}", l.folded);
        for (i, q) in l.quads.iter().enumerate() {
            assert!(is_convex_quad(q), "{n} {t}: quad {i} {q:?}");
            let width = (q[3] - q[0]).length().min((q[2] - q[1]).length());
            assert!(width >= t - 1e-6, "{n} {t}: quad {i} is at least as wide as the thickness");
        }
        for w in l.quads.windows(2) {
            assert_eq!(w[0][1], w[1][0], "{n} {t}: right corners meet");
            assert_eq!(w[0][2], w[1][3], "{n} {t}: left corners meet");
        }
        let area: f64 = l.quads.iter().map(quad_area).sum();
        let expected = l.length * t;
        assert!((area - expected).abs() < expected * 0.01, "{n} {t}: no gap, no overlap: {area} vs {expected}");
    }
}

fn quad_area(q: &[DVec2; 4]) -> f64 {
    0.5 * (0..4).map(|i| q[i].perp_dot(q[(i + 1) % 4])).sum::<f64>().abs()
}

#[test]
fn brushes_are_valid_six_sided_solids() {
    let pts = arc(512.0, 270.0, 5);
    let brushes = build(&spec(&pts, 24, 32.0)).unwrap();
    assert_eq!(brushes.len(), 24);
    let mut seen = std::collections::HashSet::new();
    for sides in &brushes {
        assert_eq!(sides.len(), 6, "box like brush");
        assert!(sides.iter().all(|s| s.material == "dev/dev_measuregeneric01b"));
        assert!(sides.iter().all(|s| seen.insert(s.id)), "unique side ids");
        let g = validate_solid(sides).unwrap();
        assert!(g.valid);
        assert_eq!(g.faces.len(), 6);
        assert!(g.faces.iter().all(|f| f.vertices.len() == 4), "every face is a quad");
        let b = Aabb::from_points(g.vertices());
        assert!((b.min.z - 0.0).abs() < 1e-6 && (b.max.z - 64.0).abs() < 1e-6, "{b:?}");
    }
    for kind in [ViewKind::Front, ViewKind::Side] {
        let s = CurveSpec { kind, depth: (-32.0, 0.0), ..spec(&pts, 8, 16.0) };
        let brushes = build(&s).unwrap();
        let d = kind.axes_2d().unwrap().2;
        for sides in &brushes {
            let g = validate_solid(sides).unwrap();
            let b = Aabb::from_points(g.vertices());
            assert!((b.min[d] + 32.0).abs() < 1e-6 && b.max[d].abs() < 1e-6, "{kind:?} {b:?}");
        }
    }
}

#[test]
fn tight_bends_and_bad_input_are_refused() {
    let hairpin = [v(0.0, 0.0), v(256.0, 0.0), v(0.0, 8.0)];
    let l = layout(&hairpin, 4, 64.0).unwrap();
    assert!(!l.folded.is_empty());
    assert!(matches!(build(&spec(&hairpin, 4, 64.0)), Err(BuildError::Curve(CurveError::Folded(_)))));
    let small = arc(16.0, 180.0, 4);
    assert!(matches!(build(&spec(&small, 8, 64.0)), Err(BuildError::Curve(CurveError::Folded(_)))));
    assert!(build(&spec(&small, 8, 4.0)).is_ok(), "a thinner curve fits in the bend");
    assert!(matches!(layout(&[v(1.0, 1.0)], 8, 16.0), Err(CurveError::TooFewPoints)));
    assert!(matches!(layout(&[v(1.0, 1.0), v(1.0, 1.0)], 8, 16.0), Err(CurveError::TooFewPoints)));
    assert!(matches!(layout(&[v(0.0, 0.0), v(64.0, 0.0)], 4096, 16.0), Err(CurveError::TooShort { .. })));
    assert!(matches!(layout(&[v(0.0, 0.0), v(20000.0, 0.0)], 8, 16.0), Err(CurveError::OutOfRange)));
    let mut flat = spec(&hairpin, 4, 4.0);
    flat.depth = (0.0, 0.5);
    assert!(matches!(build(&flat), Err(BuildError::Height)));
    for e in [
        CurveError::TooFewPoints,
        CurveError::TooShort { segment: 0.5 },
        CurveError::Folded(vec![1, 2]),
        CurveError::OutOfRange,
    ] {
        assert!(!e.to_string().is_empty());
        assert!(!e.to_string().contains('\u{2014}'));
    }
}

fn click(tool: &mut CurveTool, state: &mut AppState, view: &ViewInfo, p: DVec3) {
    let s = view.world_to_screen(p).expect("screen");
    tool.handle_mouse(state, view, &Mouse::click(s));
}

#[test]
fn draw_validate_and_create_in_the_top_view() {
    let mut state = settle();
    state.doc.clear_selection();
    let mut tool = CurveTool::new();
    let top = info_2d(ViewKind::Top, 1.0);
    for p in [(1.0, -2.0), (127.0, 63.0), (257.0, 1.0)] {
        click(&mut tool, &mut state, &top, DVec3::new(p.0, p.1, 0.0));
    }
    assert_eq!(tool.points(), [v(0.0, 0.0), v(128.0, 64.0), v(256.0, 0.0)], "snapped to the grid");
    assert_eq!(tool.depth(), Some((0.0, 64.0)), "block tool default depth");
    assert_eq!(tool.phase(), CurvePhase::Drawing);
    tool.set_subdivisions(12);
    tool.set_thickness(5000.0);
    assert_eq!(tool.options.thickness, 1024.0);
    tool.set_thickness(16.0);
    let before = state.doc.stats().solids;
    let (undo, _) = state.doc.history_len();
    let last = top.world_to_screen(DVec3::new(256.0, 0.0, 0.0)).unwrap();
    tool.handle_mouse(&mut state, &top, &Mouse::double_click(last));
    assert_eq!(tool.phase(), CurvePhase::Adjusting, "a double click validates");
    assert_eq!(tool.points().len(), 3);
    click(&mut tool, &mut state, &top, DVec3::new(400.0, 0.0, 0.0));
    assert_eq!(tool.points().len(), 3, "no new point while adjusting");
    assert_eq!(tool.create(&mut state), Some(12));
    assert_eq!(state.doc.stats().solids, before + 12);
    assert_eq!(state.doc.history_len().0, undo + 1, "one undo step");
    let ids = state.doc.selected_solid_ids();
    assert_eq!(ids.len(), 12, "the new brushes are selected");
    for id in ids {
        let node = state.doc.scene().solid(id).unwrap();
        assert!(node.geom.valid);
        assert_eq!(node.geom.faces.len(), 6);
        assert!(node.solid.sides.iter().all(|s| s.material == "dev/dev_measuregeneric01b"));
    }
    assert!(tool.points().is_empty());
    assert_eq!(tool.phase(), CurvePhase::Drawing);
    assert!(state.runtime.brush.last_created.is_some());
    state.doc.undo();
    assert_eq!(state.doc.stats().solids, before);
}

#[test]
fn freehand_drag_point_drag_and_depth_handles() {
    let mut state = settle();
    state.doc.clear_selection();
    let mut tool = CurveTool::new();
    let front = info_2d(ViewKind::Front, 1.0);
    let a = front.world_to_screen(DVec3::new(0.0, 0.0, 0.0)).unwrap();
    tool.handle_mouse(&mut state, &front, &Mouse::drag_start(a, a));
    for k in 1..=20 {
        let p = front.world_to_screen(DVec3::new(0.0, f64::from(k) * 10.0, f64::from(k * k) * 0.5)).unwrap();
        tool.handle_mouse(&mut state, &front, &Mouse::drag(a, p));
    }
    let end = front.world_to_screen(DVec3::new(0.0, 200.0, 200.0)).unwrap();
    tool.handle_mouse(&mut state, &front, &Mouse::drag_end(a, end));
    let n = tool.points().len();
    assert!((4..=20).contains(&n), "spaced freehand points: {n}");
    assert!(tool.points().iter().all(|p| p.x % 16.0 == 0.0 && p.y % 16.0 == 0.0), "on the grid");
    assert_eq!(tool.view_kind(), Some(ViewKind::Front));
    let first = front.world_to_screen(DVec3::new(0.0, 0.0, 0.0)).unwrap();
    let to = front.world_to_screen(DVec3::new(0.0, -32.0, 0.0)).unwrap();
    assert_eq!(tool.cursor(&state, &front, first), Some(CursorIcon::Grab));
    tool.handle_mouse(&mut state, &front, &Mouse::drag_start(first, first));
    tool.handle_mouse(&mut state, &front, &Mouse::drag_end(first, to));
    assert_eq!(tool.points()[0], v(-32.0, 0.0), "the point moved");
    assert_eq!(tool.points().len(), n, "moving a point adds none");
    let top = info_2d(ViewKind::Top, 1.0);
    let b = tool.depth().unwrap();
    let from = top.world_to_screen(DVec3::new(b.1, 64.0, 0.0)).unwrap();
    let to = top.world_to_screen(DVec3::new(b.1 + 64.0, 64.0, 0.0)).unwrap();
    tool.handle_mouse(&mut state, &top, &Mouse::hover(from));
    assert_eq!(tool.cursor(&state, &top, from), Some(CursorIcon::ResizeHorizontal));
    tool.handle_mouse(&mut state, &top, &Mouse::drag_start(from, from));
    tool.handle_mouse(&mut state, &top, &Mouse::drag_end(from, to));
    assert_eq!(tool.depth(), Some((b.0, b.1 + 64.0)));
    assert_eq!(tool.points().len(), n, "a drag in another view adds no point");
    tool.set_height(32.0);
    assert_eq!(tool.depth(), Some((b.0, b.0 + 32.0)));
    tool.options.subdivisions = 6;
    assert!(tool.validate());
    let made = tool.create(&mut state).unwrap();
    assert_eq!(made, 6);
    for id in state.doc.selected_solid_ids() {
        let bounds = state.doc.scene().solid(id).unwrap().bounds;
        assert!((bounds.min.x - b.0).abs() < 1e-6 && (bounds.max.x - b.0 - 32.0).abs() < 1e-6, "{bounds:?}");
    }
}

#[test]
fn keys_cancel_entity_and_errors() {
    let mut state = fgd_state();
    let mut tool = CurveTool::new();
    tool.options.create_as = CreateAs::Entity("func_detail".to_owned());
    tool.options.keep_active = false;
    let side = info_2d(ViewKind::Side, 1.0);
    assert!(tool.status_hint(&state).is_some());
    click(&mut tool, &mut state, &side, DVec3::new(0.0, 0.0, 0.0));
    assert!(!tool.is_ready());
    assert!(!tool.validate());
    click(&mut tool, &mut state, &side, DVec3::new(256.0, 0.0, 128.0));
    assert!(tool.is_ready());
    click(&mut tool, &mut state, &info_2d(ViewKind::Top, 1.0), DVec3::new(500.0, 500.0, 0.0));
    assert_eq!(tool.points().len(), 2, "other views do not add points");
    let view3d = info_3d(DVec3::new(0.0, -300.0, 64.0), 90.0, 0.0);
    assert_eq!(tool.handle_mouse(&mut state, &view3d, &Mouse::click(view3d.rect.center())), ToolFlow::Ignored);
    tool.options.subdivisions = 4096;
    assert!(tool.validate());
    assert!(tool.create(&mut state).is_none(), "segments too short");
    assert!(tool.back_to_drawing());
    tool.options.subdivisions = 4;
    let entities = state.doc.stats().entities;
    assert_eq!(tool.create(&mut state), Some(4));
    assert_eq!(state.doc.stats().entities, entities + 1);
    let e = state.doc.selected_entity_ids()[0];
    assert_eq!(state.doc.scene().entity(e).unwrap().entity.classname, "func_detail");
    assert_eq!(state.doc.scene().entity_solid_ids(e).len(), 4);
    assert_eq!(state.requested_tool, Some(ToolId::Select));
    click(&mut tool, &mut state, &side, DVec3::new(0.0, 0.0, 0.0));
    assert!(tool.remove_last_point());
    assert!(tool.points().is_empty());
    click(&mut tool, &mut state, &side, DVec3::new(0.0, 0.0, 0.0));
    assert!(tool.cancel(&mut state));
    assert!(!tool.cancel(&mut state));
    assert!(tool.create(&mut state).is_none());
}

#[test]
fn options_round_trip() {
    let mut state = settle();
    let mut tool = CurveTool::new();
    for o in tool.option_values() {
        assert!(tool.set_option(&mut state, o.key, &o.value), "{} {}", o.key, o.value);
    }
    assert!(tool.set_option(&mut state, "subdivisions", "4096"));
    assert!(tool.set_option(&mut state, "subdivisions", "2"));
    assert!(!tool.set_option(&mut state, "subdivisions", "1"));
    assert!(!tool.set_option(&mut state, "subdivisions", "4097"));
    assert!(!tool.set_option(&mut state, "subdivisions", "8.5"));
    assert!(tool.set_option(&mut state, "thickness", "1"));
    assert!(tool.set_option(&mut state, "thickness", "1024"));
    assert!(!tool.set_option(&mut state, "thickness", "0.5"));
    assert!(!tool.set_option(&mut state, "thickness", "1025"));
    assert!(tool.set_option(&mut state, "height", "48"));
    assert_eq!(tool.options.height, Some(48.0));
    assert!(tool.set_option(&mut state, "height", "auto"));
    assert_eq!(tool.options.height, None);
    assert!(!tool.set_option(&mut state, "height", "0"));
    assert!(tool.set_option(&mut state, "create_as", "func_detail"));
    assert!(!tool.set_option(&mut state, "create_as", "a b"));
    assert!(!tool.set_option(&mut state, "nope", "1"));
    assert!(tool.set_option(&mut state, "height", "40"));
    click(&mut tool, &mut state, &info_2d(ViewKind::Top, 1.0), DVec3::ZERO);
    let (lo, hi) = tool.depth().unwrap();
    assert_eq!(hi - lo, 40.0, "the height option is used for new curves");
}

#[test]
fn preview_in_every_view_and_panels() {
    let mut state = settle();
    let mut tool = CurveTool::new();
    let top = info_2d(ViewKind::Top, 1.0);
    for p in [(0.0, 0.0), (128.0, 128.0), (256.0, 0.0)] {
        click(&mut tool, &mut state, &top, DVec3::new(p.0, p.1, 0.0));
    }
    let hover = top.world_to_screen(DVec3::new(320.0, 64.0, 0.0)).unwrap();
    tool.handle_mouse(&mut state, &top, &Mouse::hover(hover));
    let views = [
        top,
        info_2d(ViewKind::Front, 1.0),
        info_2d(ViewKind::Side, 1.0),
        info_3d(DVec3::new(0.0, -300.0, 64.0), 90.0, 0.0),
    ];
    tool.options.subdivisions = 8;
    for view in &views {
        let mut out = Overlay::default();
        tool.overlay(&state, view, &mut out);
        assert!(!out.lines_on_top.is_empty() || !out.lines.is_empty(), "{:?}", view.kind);
    }
    let mut out = Overlay::default();
    tool.overlay(&state, &views[3], &mut out);
    assert_eq!(out.lines.len(), 8 * 12, "every brush edge in the 3D view");
    assert!(!out.triangles_on_top.is_empty(), "filled faces in the 3D view");
    tool.options.subdivisions = 16;
    let mut out = Overlay::default();
    tool.overlay(&state, &views[3], &mut out);
    assert_eq!(out.lines.len(), 16 * 12, "the preview follows the subdivisions");
    tool.options.thickness = 400.0;
    let mut out = Overlay::default();
    tool.overlay(&state, &views[3], &mut out);
    let red = crate::common::rgba(crate::common::COLOR_REMOVED, 1.0);
    assert!(out.lines.iter().any(|l| l.color == red), "folded segments are red");
    tool.options.thickness = 16.0;
    assert!(tool.validate());
    hp_app::testing::run_ctx(&mut state, |ctx, state| {
        for view in &views {
            tool.paint(state, view, &ctx.layer_painter(egui::LayerId::background()));
        }
    });
    hp_app::testing::run_ui(&mut state, |ui, state| tool.options_ui(ui, state));
    assert!(tool.status_hint(&state).is_some());
}

#[test]
fn strings_are_translated_and_used() {
    assert!(hp_app::i18n::lint_table(CURVE_TR).is_empty());
    let sources = [
        include_str!("tool.rs"),
        include_str!("geom.rs"),
        include_str!("mod.rs"),
        include_str!("tr.rs"),
        include_str!("tests.rs"),
    ];
    for src in sources {
        assert!(!src.contains('\u{2014}') && !src.contains('\u{2013}'), "no em or en dash");
        assert!(!src.lines().any(|l| l.trim_start().starts_with("//")), "no comments");
    }
    let tool = sources[0];
    for (k, en, fr) in CURVE_TR {
        assert!(k.starts_with("curve."), "{k}");
        assert!(tool.contains(&format!("\"{k}\"")), "unused key {k}");
        let count = |s: &str| (0..4).filter(|i| s.contains(&format!("{{{i}}}"))).count();
        assert_eq!(count(en), count(fr), "{k}: placeholders differ");
    }
    let known = |k: &str| CURVE_TR.iter().any(|(c, _, _)| *c == k) || crate::TR.iter().any(|(c, _, _)| *c == k);
    let mut checked = 0;
    for prefix in ["t!(\"", "tf!(\""] {
        let mut rest = tool;
        while let Some(i) = rest.find(prefix) {
            let boundary = rest[..i].chars().next_back().is_none_or(|c| !c.is_ascii_alphanumeric() && c != '_');
            let tail = &rest[i + prefix.len()..];
            let end = tail.find('"').unwrap_or(0);
            let key = &tail[..end];
            rest = &tail[end..];
            if !boundary {
                continue;
            }
            assert!(known(key), "missing translation {key}");
            checked += 1;
        }
    }
    assert!(checked > 20, "{checked}");
}
