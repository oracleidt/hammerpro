use super::disp_live::quad_tiles;
use super::*;
use crate::disp::{adopt_object_targets, object_targets_win, with_groups};

fn grouped_tiles() -> (AppState, DisplacementTool, Vec<FaceRef>) {
    let mut state = state_with(&[]);
    let mut tool = DisplacementTool::new();
    let faces = quad_tiles(&mut state, &mut tool, [3, 3, 3, 3]);
    let objs: Vec<ObjRef> = faces.iter().map(|f| ObjRef::Solid(f.solid)).collect();
    state.doc.select_faces(&faces[..1], SelectOp::Replace);
    state.doc.select(&objs, SelectOp::Replace);
    state.doc.group(&objs).expect("group");
    (state, tool, faces)
}

fn top(state: &AppState, f: FaceRef) -> f64 {
    mesh(state, f).positions.iter().map(|p| p.z).fold(f64::MIN, f64::max)
}

fn sorted(mut v: Vec<FaceRef>) -> Vec<FaceRef> {
    v.sort_unstable_by_key(|f| (f.solid, f.side));
    v
}

#[test]
fn a_selected_group_is_the_target_even_after_an_older_face_selection() {
    let (state, _, faces) = grouped_tiles();
    assert_eq!(state.doc.selected_faces(), vec![faces[0]], "the face selection of the creation is still there");
    assert_eq!(sorted(crate::disp::commands::targets(&state.doc)), sorted(faces.clone()));
    assert!(object_targets_win(&state.doc));
}

#[test]
fn raising_a_selected_group_raises_every_member_and_crosses_seams() {
    let (mut state, mut tool, faces) = grouped_tiles();
    tool.interaction = Interaction::Geometry;
    tool.brush.geo_effect = GeoEffect::Raise;
    tool.brush.selected_only = true;
    tool.brush.falloff = Falloff::Constant;
    tool.brush.radius = 200.0;
    tool.brush.strength = 200.0;
    state.frame.dt = 0.1;
    tool.begin_stroke(&mut state, 0);
    for x in [-120.0, -40.0, 40.0, 120.0] {
        tool.paint_at(&mut state, DVec3::new(x, x * 0.5, 0.0), false);
    }
    tool.end_stroke(&mut state);
    for f in &faces {
        assert!(top(&state, *f) > 1.0, "{f:?} was not raised");
    }
}

#[test]
fn commands_work_on_every_grouped_displacement() {
    let (mut state, mut tool, faces) = grouped_tiles();
    assert_eq!(tool.set_elevation(&mut state, 8.0), 4);
    for f in &faces {
        let d = state.doc.face_side(*f).and_then(|s| s.disp.as_ref()).expect("disp");
        assert_eq!(d.elevation, 8.0);
    }
    let before: Vec<Vec<f64>> = faces.iter().map(|f| distances(&state, *f)).collect();
    assert!(crate::disp::commands::add_noise(&mut state) > 0);
    let changed = faces.iter().zip(&before).filter(|(f, b)| distances(&state, **f) != **b).count();
    assert_eq!(changed, 4, "noise reached every member of the group");
}

#[test]
fn clicking_a_grouped_displacement_takes_its_whole_group() {
    let (mut state, _, faces) = grouped_tiles();
    state.settings.edit.select_mode = hp_app::settings::SelectModeSetting::Groups;
    assert_eq!(sorted(with_groups(&state, vec![faces[2]])), sorted(faces.clone()));
    state.settings.edit.select_mode = hp_app::settings::SelectModeSetting::Objects;
    assert_eq!(with_groups(&state, vec![faces[2]]), vec![faces[2]]);
}

#[test]
fn toggling_a_face_keeps_the_rest_of_the_group_and_escape_clears_it() {
    let (mut state, mut tool, faces) = grouped_tiles();
    adopt_object_targets(&mut state);
    assert_eq!(sorted(state.doc.selected_faces()), sorted(faces.clone()));
    state.doc.select_faces(&faces[..1], SelectOp::Toggle);
    assert_eq!(sorted(crate::disp::commands::targets(&state.doc)), sorted(faces[1..].to_vec()));
    let (mut fresh, _, _) = grouped_tiles();
    assert!(Tool::cancel(&mut tool, &mut fresh));
    assert!(crate::disp::commands::targets(&fresh.doc).is_empty());
    assert!(!object_targets_win(&fresh.doc));
}
