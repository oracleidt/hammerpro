use super::*;
use crate::disp::BrushSettings;
use crate::disp::sculpt::SculptOp;

fn oblique_view() -> ViewInfo {
    view3d(DVec3::new(-200.0, 0.0, 200.0), 0.0, -45.0)
}

fn moved_along(before: &hp_geom::DispMesh, after: &hp_geom::DispMesh, dir: DVec3) -> usize {
    let mut moved = 0;
    for (p, q) in after.positions.iter().zip(&before.positions) {
        let d = *p - *q;
        if d.length() < 1e-9 {
            continue;
        }
        moved += 1;
        let along = d.dot(dir);
        assert!(along > 0.0, "{q} moved {d}, not toward {dir}");
        assert!((d - dir * along).length() < 1e-6, "{q} moved {d}, not along {dir}");
    }
    moved
}

#[test]
fn view_axis_resolves_to_the_camera_direction() {
    let s = BrushSettings { axis: PaintAxis::View, ..BrushSettings::default() };
    assert_eq!(s.axis_vector(), None, "unresolved");
    let r = s.resolved(Some(DVec3::new(0.0, 0.0, 2.0)), None);
    assert_eq!(r.axis, PaintAxis::Custom);
    assert_eq!(r.axis_vector(), Some(DVec3::Z), "normalized");
    assert_eq!(s.resolved(None, None).axis, PaintAxis::FaceNormal);
    assert_eq!(s.resolved(Some(DVec3::ZERO), None).axis, PaintAxis::FaceNormal);
    assert_eq!(s.resolved(Some(DVec3::NAN), None).axis, PaintAxis::FaceNormal);
    let z = BrushSettings { axis: PaintAxis::Z, ..BrushSettings::default() };
    assert_eq!(z.resolved(Some(DVec3::X), None), z);
    let sample = build_sample(&r, Interaction::Geometry, DVec3::ZERO, false, 0.1).expect("sample");
    assert!(
        matches!(sample, Sample::Local(LocalBrush { op: LocalOp::Move { dir: PaintDir::Fixed(d), .. }, .. }) if d == DVec3::Z)
    );
}

#[test]
fn view_axis_moves_toward_the_camera() {
    let view = oblique_view();
    let toward = -view.cam3d.forward();
    assert!(toward.z > 0.5 && toward.x < -0.5, "{toward}");
    for (interaction, op, invert) in [
        (Interaction::Geometry, SculptOp::Push, false),
        (Interaction::Geometry, SculptOp::Push, true),
        (Interaction::Sculpt, SculptOp::Push, false),
        (Interaction::Sculpt, SculptOp::Push, true),
        (Interaction::Sculpt, SculptOp::Carve, false),
    ] {
        let (mut state, mut tool, face) = floor_with_disp(3);
        state.doc.clear_face_selection();
        tool.interaction = interaction;
        tool.brush.axis = PaintAxis::View;
        tool.brush.sculpt.op = op;
        tool.brush.radius = 96.0;
        state.frame.dt = 0.1;
        let before = mesh(&state, face);
        let c = view.rect.center();
        let (o, d) = view.ray(c);
        assert_eq!(pick_disp(&state.doc, &Ray::new(o, d)).expect("hit").face, face);
        let mods = match (interaction, invert) {
            (_, false) => Modifiers::NONE,
            (Interaction::Sculpt, true) => ctrl(),
            (_, true) => Modifiers { shift: true, ..Modifiers::default() },
        };
        let press = PointerFrame { pos: Some(c), pressed: true, down: true, mods, ..PointerFrame::default() };
        let steps = undo_steps(&state);
        tool.handle_pointer(&mut state, &view, press);
        for i in 1..=6 {
            state.frame.index += 1;
            state.frame.time += 0.05;
            let at = c + vec2(i as f32 * 3.0, 0.0);
            tool.handle_pointer(&mut state, &view, PointerFrame { pos: Some(at), pressed: false, ..press });
        }
        tool.handle_pointer(&mut state, &view, PointerFrame { pos: Some(c), pressed: false, down: false, ..press });
        assert_eq!(undo_steps(&state), steps + 1, "{interaction:?} {op:?}: one undo step");
        let away = invert != (op == SculptOp::Carve && interaction == Interaction::Sculpt);
        let dir = if away { -toward } else { toward };
        let moved = moved_along(&before, &mesh(&state, face), dir);
        assert!(moved > 0, "{interaction:?} {op:?} invert {invert}: something moved");
    }
}

#[test]
fn view_axis_is_fixed_for_the_stroke() {
    let (mut state, mut tool, face) = floor_with_disp(3);
    state.doc.clear_face_selection();
    tool.interaction = Interaction::Sculpt;
    tool.brush.axis = PaintAxis::View;
    tool.brush.radius = 96.0;
    let first = DVec3::new(-1.0, 0.0, 1.0).normalize();
    tool.toward_eye = Some(first);
    let before = mesh(&state, face);
    tool.begin_stroke(&mut state, 0);
    assert!(tool.paint_at(&mut state, DVec3::new(0.0, 0.0, 0.0), false) > 0);
    tool.toward_eye = Some(DVec3::new(0.0, 1.0, 1.0).normalize());
    assert!(tool.paint_at(&mut state, DVec3::new(40.0, 0.0, 0.0), false) > 0);
    tool.end_stroke(&mut state);
    assert!(moved_along(&before, &mesh(&state, face), first) > 0);
    let before = mesh(&state, face);
    let second = tool.toward_eye.expect("camera");
    tool.begin_stroke(&mut state, 0);
    assert!(tool.paint_at(&mut state, DVec3::new(-120.0, 60.0, 0.0), false) > 0);
    tool.end_stroke(&mut state);
    assert!(moved_along(&before, &mesh(&state, face), second) > 0);
}

#[test]
fn horizontal_view_axis_keeps_only_x_and_y() {
    use crate::disp::paint::{flat_direction, flat_toward_camera};
    let s = BrushSettings { axis: PaintAxis::ViewFlat, ..BrushSettings::default() };
    assert_eq!(s.axis_vector(), None, "unresolved");
    let r = s.resolved(Some(DVec3::new(-3.0, 4.0, 10.0)), None);
    assert_eq!(r.axis, PaintAxis::Custom);
    let v = r.axis_vector().expect("axis");
    assert!((v - DVec3::new(-0.6, 0.8, 0.0)).length() < 1e-12, "{v}");
    assert_eq!(s.resolved(Some(DVec3::Z), None).axis, PaintAxis::FaceNormal, "no horizontal part and no camera");
    let r = s.resolved_view(Some(DVec3::Z), Some(DVec3::new(0.0, -1.0, 0.0)), None);
    assert_eq!(r.axis_vector(), Some(DVec3::NEG_Y), "the camera fallback wins");
    assert_eq!(flat_direction(DVec3::new(0.0, 0.0, -1.0)), None);

    let cam = |yaw: f64, pitch: f64| hp_app::view::Camera3d { yaw_deg: yaw, pitch_deg: pitch, ..Default::default() };
    let d = flat_toward_camera(&cam(0.0, -30.0)).expect("flat");
    assert!((d - DVec3::NEG_X).length() < 1e-9, "{d}");
    let d = flat_toward_camera(&cam(90.0, -90.0)).expect("straight down");
    assert!((d - DVec3::NEG_Y).length() < 1e-6, "the bottom of the view: {d}");
    let d = flat_toward_camera(&cam(90.0, -89.0)).expect("nearly down");
    assert!((d - DVec3::NEG_Y).length() < 1e-6, "same as straight down: {d}");
    let up = cam(90.0, 90.0);
    let d = flat_toward_camera(&up).expect("straight up");
    assert!(d.z == 0.0 && (d.length() - 1.0).abs() < 1e-9);

    let view = oblique_view();
    let flat = DVec3::new(-1.0, 0.0, 0.0);
    for (interaction, invert) in
        [(Interaction::Geometry, false), (Interaction::Geometry, true), (Interaction::Sculpt, false)]
    {
        let (mut state, mut tool, face) = floor_with_disp(3);
        state.doc.clear_face_selection();
        tool.interaction = interaction;
        tool.brush.axis = PaintAxis::ViewFlat;
        tool.brush.radius = 96.0;
        state.frame.dt = 0.1;
        let before = mesh(&state, face);
        let c = view.rect.center();
        let mods = if invert { Modifiers { shift: true, ..Modifiers::default() } } else { Modifiers::NONE };
        let press = PointerFrame { pos: Some(c), pressed: true, down: true, mods, ..PointerFrame::default() };
        tool.handle_pointer(&mut state, &view, press);
        for i in 1..=4 {
            state.frame.index += 1;
            state.frame.time += 0.05;
            let at = c + vec2(i as f32 * 3.0, 0.0);
            tool.handle_pointer(&mut state, &view, PointerFrame { pos: Some(at), pressed: false, ..press });
        }
        tool.handle_pointer(&mut state, &view, PointerFrame { pos: Some(c), pressed: false, down: false, ..press });
        let dir = if invert { -flat } else { flat };
        assert!(moved_along(&before, &mesh(&state, face), dir) > 0, "{interaction:?} invert {invert}");
    }
}

pub(super) fn max_crack(state: &AppState, faces: &[FaceRef]) -> f64 {
    let meshes: Vec<hp_geom::DispMesh> = faces.iter().map(|f| mesh(state, *f)).collect();
    let mut worst = 0.0f64;
    for (i, b) in meshes.iter().enumerate() {
        let nb = b.row_len;
        for (j, a) in meshes.iter().enumerate() {
            if i == j {
                continue;
            }
            let na = a.row_len;
            let borders: [Vec<usize>; 4] = [
                (0..na).collect(),
                (0..na).map(|r| r * na + na - 1).collect(),
                (0..na).map(|c| (na - 1) * na + c).collect(),
                (0..na).map(|r| r * na).collect(),
            ];
            for k in 0..nb * nb {
                let (row, col) = (k / nb, k % nb);
                if row != 0 && row != nb - 1 && col != 0 && col != nb - 1 {
                    continue;
                }
                let p = b.base_positions[k];
                for border in &borders {
                    let (s0, s1) = (a.base_positions[border[0]], a.base_positions[border[na - 1]]);
                    let len = s0.distance(s1);
                    let u = (s1 - s0) / len;
                    let t = (p - s0).dot(u);
                    if t < -1e-6 || t > len + 1e-6 || ((p - s0) - u * t).length() > 1e-6 {
                        continue;
                    }
                    let step = len / (na - 1) as f64;
                    let idx = ((t / step).floor().max(0.0) as usize).min(na - 2);
                    let s = t / step - idx as f64;
                    let expected = a.positions[border[idx]].lerp(a.positions[border[idx + 1]], s);
                    worst = worst.max(expected.distance(b.positions[k]));
                }
            }
        }
    }
    worst
}

pub(super) fn quad_tiles(state: &mut AppState, tool: &mut DisplacementTool, powers: [u32; 4]) -> Vec<FaceRef> {
    let ids = [
        block(state, DVec3::new(-256.0, -256.0, -16.0), DVec3::new(0.0, 0.0, 0.0)),
        block(state, DVec3::new(0.0, -256.0, -16.0), DVec3::new(256.0, 0.0, 0.0)),
        block(state, DVec3::new(-256.0, 0.0, -16.0), DVec3::new(0.0, 256.0, 0.0)),
        block(state, DVec3::new(0.0, 0.0, -16.0), DVec3::new(256.0, 256.0, 0.0)),
    ];
    let faces: Vec<FaceRef> = ids.iter().map(|id| side_facing(state, *id, DVec3::Z)).collect();
    for (f, power) in faces.iter().zip(powers) {
        make_disps(state, tool, &[*f], power);
    }
    faces
}

const STROKE: [(f64, f64); 9] = [
    (-90.0, -40.0),
    (-50.0, -20.0),
    (-10.0, -5.0),
    (0.0, 0.0),
    (20.0, 10.0),
    (30.0, 40.0),
    (10.0, 80.0),
    (-30.0, 60.0),
    (-60.0, 20.0),
];

#[test]
fn live_sew_keeps_seams_closed_at_every_sample() {
    #[derive(Clone, Copy, Debug)]
    enum Effect {
        Raise,
        Lower,
        Smooth,
        Push,
        Carve,
    }
    let cases = [
        ([3, 3, 3, 3], Effect::Raise, false, false, PaintAxis::FaceNormal),
        ([3, 3, 3, 3], Effect::Raise, false, true, PaintAxis::FaceNormal),
        ([2, 3, 3, 4], Effect::Raise, false, false, PaintAxis::FaceNormal),
        ([2, 3, 3, 4], Effect::Lower, true, false, PaintAxis::SubdivNormal),
        ([2, 3, 4, 2], Effect::Raise, false, false, PaintAxis::View),
        ([2, 3, 3, 4], Effect::Push, false, false, PaintAxis::FaceNormal),
        ([4, 2, 3, 3], Effect::Push, false, true, PaintAxis::View),
        ([2, 3, 3, 4], Effect::Carve, false, false, PaintAxis::Z),
        ([2, 3, 3, 4], Effect::Smooth, false, false, PaintAxis::FaceNormal),
    ];
    for (powers, effect, spatial, selected_only, axis) in cases {
        let label = format!("{powers:?} {effect:?} spatial {spatial} selected {selected_only} {axis:?}");
        let mut state = state_with(&[]);
        let mut tool = DisplacementTool::new();
        let faces = quad_tiles(&mut state, &mut tool, powers);
        if matches!(effect, Effect::Smooth) {
            for (i, f) in faces.iter().enumerate() {
                let n = mesh(&state, *f).row_len;
                state
                    .doc
                    .edit_solid(f.solid, |s| {
                        let d = s.sides[f.side].disp.as_mut().expect("disp");
                        for k in 0..n * n {
                            d.normals[k] = DVec3::Z;
                            d.distances[k] = 4.0 + ((k * 7 + i * 3) % 5) as f64 * 6.0;
                        }
                    })
                    .expect("edit");
            }
            state.doc.select_faces(&faces, SelectOp::Replace);
            assert!(tool.sew(&mut state) > 0);
        }
        assert!(max_crack(&state, &faces) < 1e-6, "{label}: sewn before the stroke");
        if selected_only {
            state.doc.select_faces(&faces[..1], SelectOp::Replace);
        } else {
            state.doc.clear_face_selection();
        }
        tool.brush.autosew = true;
        tool.brush.spatial = spatial;
        tool.brush.axis = axis;
        tool.brush.radius = 100.0;
        tool.brush.smooth = 1.0;
        tool.toward_eye = Some(DVec3::new(-1.0, -0.5, 2.0).normalize());
        let invert = matches!(effect, Effect::Lower);
        match effect {
            Effect::Raise | Effect::Lower => tool.interaction = Interaction::Geometry,
            Effect::Smooth => {
                tool.interaction = Interaction::Geometry;
                tool.brush.geo_effect = GeoEffect::Smooth;
            }
            Effect::Push | Effect::Carve => {
                tool.interaction = Interaction::Sculpt;
                tool.brush.sculpt.op = if matches!(effect, Effect::Push) { SculptOp::Push } else { SculptOp::Carve };
            }
        }
        state.frame.dt = 0.1;
        let before: Vec<hp_geom::DispMesh> = faces.iter().map(|f| mesh(&state, *f)).collect();
        let steps = undo_steps(&state);
        tool.begin_stroke(&mut state, 0);
        let mut total = 0;
        for (x, y) in STROKE {
            let p = DVec3::new(x, y, 0.0);
            total += tool.paint_at(&mut state, p, invert);
            let crack = max_crack(&state, &faces);
            assert!(crack < 1e-6, "{label}: crack {crack} after the sample at {p}");
        }
        tool.end_stroke(&mut state);
        assert!(total > 0, "{label}: the stroke painted");
        assert!(max_crack(&state, &faces) < 1e-6, "{label}: after the stroke");
        assert_eq!(undo_steps(&state), steps + 1, "{label}: one undo step");
        for (i, (f, old)) in faces.iter().zip(&before).enumerate() {
            if selected_only && i > 0 {
                assert!(untouched(&state, *f), "{label}: unselected {f:?} untouched");
                continue;
            }
            let m = mesh(&state, *f);
            let n = m.row_len;
            let inner = (0..n * n).filter(|k| {
                let (r, c) = (k / n, k % n);
                r > 0 && r < n - 1 && c > 0 && c < n - 1 && m.positions[*k].distance(old.positions[*k]) > 1e-6
            });
            assert!(inner.count() > 0, "{label}: {f:?} painted inside");
            assert_tags_current(&state, *f);
        }
        state.doc.undo();
        for (f, old) in faces.iter().zip(&before) {
            assert_eq!(mesh(&state, *f).positions, old.positions, "{label}: undo restores {f:?}");
        }
    }
}

#[test]
fn live_sew_moves_the_shared_edge_of_walls() {
    let mut state = state_with(&[]);
    let mut tool = DisplacementTool::new();
    let floor = block(&mut state, DVec3::new(-256.0, -256.0, 0.0), DVec3::new(256.0, 256.0, 64.0));
    let edge_wall = block(&mut state, DVec3::new(-272.0, -256.0, 64.0), DVec3::new(-256.0, 256.0, 256.0));
    let far_wall = block(&mut state, DVec3::new(256.0, -256.0, 64.0), DVec3::new(272.0, 256.0, 256.0));
    let ff = side_facing(&state, floor, DVec3::Z);
    let ef = side_facing(&state, edge_wall, DVec3::X);
    let wf = side_facing(&state, far_wall, DVec3::NEG_X);
    make_disps(&mut state, &mut tool, &[ff, ef, wf], 3);
    state.doc.clear_face_selection();
    tool.brush.autosew = true;
    tool.brush.radius = 128.0;
    state.frame.dt = 0.1;
    tool.begin_stroke(&mut state, 0);
    for y in [-60.0, -20.0, 20.0, 60.0] {
        assert!(tool.paint_at(&mut state, DVec3::new(-240.0, y, 64.0), false) > 0);
        assert!(max_crack(&state, &[ff, ef]) < 1e-6, "crack at y {y}");
    }
    tool.end_stroke(&mut state);
    let (floor_mesh, wall_mesh) = (mesh(&state, ff), mesh(&state, ef));
    let edge = |b: &DVec3| (b.x + 256.0).abs() < 1e-6 && (b.z - 64.0).abs() < 1e-6;
    let raised: Vec<(DVec3, DVec3)> = floor_mesh
        .base_positions
        .iter()
        .zip(&floor_mesh.positions)
        .filter(|(b, p)| edge(b) && p.z - b.z > 1.0)
        .map(|(b, p)| (*b, *p))
        .collect();
    assert!(!raised.is_empty(), "the floor edge is raised");
    for (b, p) in &raised {
        let k = wall_mesh.base_positions.iter().position(|q| q.distance(*b) < 1e-6).expect("partner");
        assert!(wall_mesh.positions[k].distance(*p) < 1e-6, "the wall follows the floor at {b}");
    }
    for (b, p) in wall_mesh.base_positions.iter().zip(&wall_mesh.positions) {
        if !edge(b) {
            assert!(b.distance(*p) < 1e-9, "wall vertex {b} moved to {p}");
        }
    }
    assert!(untouched(&state, wf), "the far wall");
}

#[test]
fn without_autosew_unselected_neighbours_stay() {
    let mut state = state_with(&[]);
    let mut tool = DisplacementTool::new();
    let faces = quad_tiles(&mut state, &mut tool, [3, 3, 3, 3]);
    state.doc.select_faces(&faces[..1], SelectOp::Replace);
    tool.brush.radius = 80.0;
    state.frame.dt = 0.1;
    tool.begin_stroke(&mut state, 0);
    assert!(tool.paint_at(&mut state, DVec3::new(-10.0, -10.0, 0.0), false) > 0);
    tool.end_stroke(&mut state);
    for f in &faces[1..] {
        assert!(untouched(&state, *f));
    }
    assert!(max_crack(&state, &faces) > 1.0);
}
