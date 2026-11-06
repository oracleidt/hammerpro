use super::disp_live::{max_crack, quad_tiles};
use super::*;
use crate::disp::tags::{TagKind, TagValue};
use crate::disp::{BoundsLimit, navigating_frame};
use hp_geom::{DISP_TAG_FORCE_REMOVE_BIT, disp_tri_states};

fn above(x: f64, y: f64) -> ViewInfo {
    view3d(DVec3::new(x, y, 600.0), 0.0, -89.0)
}

fn retained(tool: &DisplacementTool, state: &AppState, view: &ViewInfo) -> hp_render::Overlay {
    let mut out = hp_render::Overlay::default();
    tool.retained_overlay(state, view, &mut out);
    out
}

fn disp_info(state: &AppState, f: FaceRef) -> hp_vmf::DispInfo {
    (**state.doc.face_side(f).and_then(|s| s.disp.as_ref()).expect("disp")).clone()
}

#[test]
fn every_selected_displacement_shows_its_tint_and_grid() {
    let mut state = state_with(&[]);
    let mut tool = DisplacementTool::new();
    let faces = quad_tiles(&mut state, &mut tool, [2, 2, 2, 2]);
    state.doc.select_faces(&faces[..3], SelectOp::Replace);
    let view = above(0.0, 0.0);
    let out = retained(&tool, &state, &view);
    assert_eq!(out.triangles.len(), 3 * 32, "a tint for every selected displacement");
    assert_eq!(out.lines.len(), 3 * 56, "a grid for every selected displacement");
    let v1 = tool.retained_overlay_version(&state, &view);
    state.doc.select_faces(&faces, SelectOp::Replace);
    assert_ne!(tool.retained_overlay_version(&state, &view), v1, "the selection changes the highlight");
    assert_eq!(retained(&tool, &state, &view).triangles.len(), 4 * 32);
    let top = view2d(ViewKind::Top, DVec2::ZERO, 1.0);
    let out = retained(&tool, &state, &top);
    assert!(out.triangles.is_empty());
    assert_eq!(out.lines_on_top.len(), 4 * 56);
    state.settings.render.displacements_2d = hp_app::settings::Disp2dSetting::Outline;
    assert!(retained(&tool, &state, &top).lines_on_top.is_empty(), "outline mode: no grid");
    tool.mask_grid = true;
    let out = retained(&tool, &state, &view);
    assert!(out.lines.is_empty() && out.triangles.len() == 4 * 32);
    tool.mask_select = true;
    assert!(retained(&tool, &state, &view).is_empty());
}

#[test]
fn selection_opacity_refreshes_the_mesh_mask_without_editing_the_surface() {
    let (mut state, tool, face) = floor_with_disp(3);
    state.doc.select_faces(&[face], SelectOp::Replace);
    let view = above(0.0, 0.0);
    let initial = disp_info(&state, face);
    let revision = state.doc.revision();
    let mut previous_version = None;
    for opacity in [0.0, 0.237, 1.0] {
        state.settings.mesh.disp_selection_opacity = opacity;
        let version = tool.retained_overlay_version(&state, &view);
        assert_ne!(version, previous_version);
        previous_version = version;
        let out = retained(&tool, &state, &view);
        assert!(!out.lines.is_empty(), "topology stays visible");
        if opacity == 0.0 {
            assert!(out.triangles.is_empty());
        } else {
            assert_eq!(out.triangles.len(), 128);
            assert!(out.triangles.iter().all(|t| t.color[3] == opacity));
        }
        assert_eq!(state.doc.revision(), revision);
        assert_eq!(disp_info(&state, face), initial);
    }
}

#[test]
fn options_are_saved_and_restored() {
    use hp_app::Tool as _;
    let mut state = state_with(&[FLOOR]);
    let mut tool = DisplacementTool::new();
    assert!(tool.set_option(&mut state, "radius", "300"));
    assert!(tool.set_option(&mut state, "mode", "tag"));
    assert!(tool.set_option(&mut state, "tag_kind", "buildable"));
    assert!(tool.set_option(&mut state, "mask_grid", "on"));
    assert!(state.settings_dirty);
    assert_eq!(state.settings.mesh.disp_tool.get("radius").map(String::as_str), Some("300"));
    let mut fresh = DisplacementTool::new();
    fresh.activate(&mut state);
    assert_eq!(fresh.brush.radius, 300.0);
    assert_eq!(fresh.interaction, Interaction::Tag);
    assert_eq!(fresh.brush.tag.kind, TagKind::Buildable);
    assert!(fresh.mask_grid);
    state.settings.mesh.disp_tool.insert("no_such_option".to_owned(), "1".to_owned());
    let mut again = DisplacementTool::new();
    again.activate(&mut state);
    assert_eq!(again.brush.radius, 300.0);
}

#[test]
fn selection_helpers() {
    let mut state = state_with(&[]);
    let mut tool = DisplacementTool::new();
    let faces = quad_tiles(&mut state, &mut tool, [3, 3, 3, 3]);
    state.doc.select_faces(&faces[..1], SelectOp::Replace);
    assert!(tool.select_adjacent(&mut state) >= 2);
    let sel = state.doc.selected_faces();
    assert!(sel.contains(&faces[1]) && sel.contains(&faces[2]), "{sel:?}");
    state.doc.clear_face_selection();
    state.doc.select(&[ObjRef::Solid(faces[3].solid)], SelectOp::Replace);
    assert_eq!(tool.select_of_solids(&mut state), 1);
    assert_eq!(state.doc.selected_faces(), vec![faces[3]]);
    let id = block(&mut state, DVec3::new(600.0, -64.0, -64.0), DVec3::new(728.0, 64.0, 0.0));
    let two = [side_facing(&state, id, DVec3::Z), side_facing(&state, id, DVec3::X)];
    make_disps(&mut state, &mut tool, &two, 2);
    state.doc.clear_face_selection();
    let view = above(664.0, 0.0);
    let c = view.rect.center();
    tool.interaction = Interaction::Select;
    tool.click_select(&mut state, &view, c, false, true);
    let sel = state.doc.selected_faces();
    assert!(sel.contains(&two[0]) && sel.contains(&two[1]), "{sel:?}");
    tool.click_select(&mut state, &view, c, false, true);
    assert!(state.doc.selected_faces().is_empty(), "Shift click toggles them off");
    use hp_app::Tool as _;
    assert!(tool.set_option(&mut state, "select", "all"));
    assert_eq!(state.doc.selected_faces().len(), 6);
    assert!(tool.set_option(&mut state, "lightmap_scale", "32"));
    assert!(state.doc.selected_faces().iter().all(|f| state.doc.face_side(*f).is_some_and(|s| s.lightmap_scale == 32)));
    assert!(tool.set_option(&mut state, "select", "none"));
    assert!(state.doc.selected_faces().is_empty());
    assert!(!tool.set_option(&mut state, "select", "everything"));
    assert!(!tool.set_option(&mut state, "lightmap_scale", "0"));
    assert!(
        !tool.option_values().iter().any(|o| ["select", "lightmap_scale", "cast_shadows"].contains(&o.key)),
        "commands are not saved"
    );
}

#[test]
fn lightmap_scale_of_the_selection() {
    let (mut state, mut tool, face) = floor_with_disp(3);
    let steps = undo_steps(&state);
    assert_eq!(tool.set_lightmap_scale(&mut state, 8), 1);
    assert_eq!(tool.set_lightmap_scale(&mut state, 4), 1);
    assert_eq!(state.doc.face_side(face).expect("side").lightmap_scale, 4);
    assert_eq!(undo_steps(&state), steps + 1);
    assert_eq!(tool.set_lightmap_scale(&mut state, 4), 0, "no change");
}

#[test]
fn axes_x_y_and_brush_center() {
    let mut s = crate::BrushSettings { axis: PaintAxis::X, ..Default::default() };
    assert_eq!(s.axis_vector(), Some(DVec3::X));
    s.axis = PaintAxis::Y;
    assert_eq!(s.axis_vector(), Some(DVec3::Y));
    let Some(Sample::Local(b)) = build_sample(&s, Interaction::Geometry, DVec3::ZERO, false, 0.1) else { panic!() };
    assert!(matches!(b.op, LocalOp::Move { dir: PaintDir::Fixed(d), .. } if d == DVec3::Y));
    s.axis = PaintAxis::BrushCenter;
    assert_eq!(s.axis_vector(), None);
    let r = s.resolved(None, Some(DVec3::new(0.0, 3.0, 4.0)));
    assert_eq!(r.axis, PaintAxis::Custom);
    assert!((r.custom_axis - DVec3::new(0.0, 0.6, 0.8)).length() < 1e-12);
    assert_eq!(s.resolved(Some(DVec3::X), None).axis, PaintAxis::FaceNormal, "no surface: the face normal");
    let (mut state, mut tool, face) = floor_with_disp(3);
    state.doc.clear_face_selection();
    tool.brush.axis = PaintAxis::BrushCenter;
    tool.brush.radius = 64.0;
    state.frame.dt = 0.1;
    tool.begin_stroke(&mut state, 0);
    assert!(tool.paint_at(&mut state, DVec3::ZERO, false) > 0);
    tool.end_stroke(&mut state);
    let m = mesh(&state, face);
    assert!(
        m.positions.iter().zip(&m.base_positions).all(|(p, b)| (p.x - b.x).abs() < 1e-9 && (p.y - b.y).abs() < 1e-9)
    );
    assert!(m.positions.iter().any(|p| p.z > 1.0), "raised along the flat floor's normal");
}

#[test]
fn selected_only_off_paints_everything_under_the_brush() {
    let mut state = state_with(&[]);
    let mut tool = DisplacementTool::new();
    let a = block(&mut state, DVec3::new(-256.0, -128.0, -16.0), DVec3::new(0.0, 128.0, 0.0));
    let b = block(&mut state, DVec3::new(0.0, -128.0, -16.0), DVec3::new(256.0, 128.0, 0.0));
    let (fa, fb) = (side_facing(&state, a, DVec3::Z), side_facing(&state, b, DVec3::Z));
    make_disps(&mut state, &mut tool, &[fa, fb], 3);
    state.doc.select_faces(&[fa], SelectOp::Replace);
    state.frame.dt = 0.1;
    tool.begin_stroke(&mut state, 0);
    tool.paint_at(&mut state, DVec3::new(128.0, 0.0, 0.0), false);
    tool.end_stroke(&mut state);
    assert!(untouched(&state, fb), "selected only (Hammer's default)");
    tool.brush.selected_only = false;
    tool.begin_stroke(&mut state, 0);
    assert!(tool.paint_at(&mut state, DVec3::new(128.0, 0.0, 0.0), false) > 0);
    tool.end_stroke(&mut state);
    assert!(!untouched(&state, fb));
}

#[test]
fn continuous_painting_option() {
    for continuous in [true, false] {
        let (mut state, mut tool, face) = floor_with_disp(3);
        state.doc.clear_face_selection();
        tool.brush.continuous = continuous;
        let view = above(0.0, 0.0);
        let c = view.rect.center();
        state.frame.dt = 0.05;
        let press = PointerFrame { pos: Some(c), pressed: true, down: true, ..PointerFrame::default() };
        let hold = PointerFrame { pressed: false, ..press };
        tool.handle_pointer(&mut state, &view, press);
        state.frame.time += 0.5;
        let mut heights = Vec::new();
        for _ in 0..4 {
            state.frame.index += 1;
            tool.handle_pointer(&mut state, &view, hold);
            heights.push(distances(&state, face).iter().copied().fold(0.0f64, f64::max));
        }
        tool.handle_pointer(&mut state, &view, PointerFrame { down: false, ..hold });
        let grew = heights.windows(2).filter(|w| w[1] > w[0] + 1e-9).count();
        if continuous {
            assert_eq!(grew, 3, "{heights:?}");
        } else {
            assert_eq!(grew, 0, "a still brush paints once: {heights:?}");
        }
    }
}

#[test]
fn alpha_smoothing_across_seams() {
    let mut state = state_with(&[]);
    let mut tool = DisplacementTool::new();
    let a = block(&mut state, DVec3::new(-256.0, -256.0, -16.0), DVec3::new(0.0, 256.0, 0.0));
    let b = block(&mut state, DVec3::new(0.0, -256.0, -16.0), DVec3::new(256.0, 256.0, 0.0));
    let faces = [a, b].map(|id| side_facing(&state, id, DVec3::Z));
    make_disps(&mut state, &mut tool, &faces, 3);
    state.doc.clear_face_selection();
    tool.interaction = Interaction::Alpha;
    tool.brush.radius = 40.0;
    state.frame.dt = 0.1;
    tool.begin_stroke(&mut state, 0);
    for _ in 0..20 {
        tool.paint_at(&mut state, DVec3::new(-32.0, 0.0, 0.0), false);
    }
    tool.end_stroke(&mut state);
    let peak = |state: &AppState| faces.iter().flat_map(|f| alphas(state, *f)).fold(0.0f64, f64::max);
    let before = peak(&state);
    assert!(before > 100.0);
    tool.brush.alpha_effect = AlphaEffect::Smooth;
    tool.brush.smooth = 1.0;
    tool.brush.radius = 200.0;
    let steps = undo_steps(&state);
    tool.begin_stroke(&mut state, 0);
    for _ in 0..10 {
        assert!(tool.paint_at(&mut state, DVec3::new(0.0, 0.0, 0.0), false) > 0);
    }
    tool.end_stroke(&mut state);
    assert_eq!(undo_steps(&state), steps + 1);
    assert!(peak(&state) < before - 10.0, "smoothed: {} from {before}", peak(&state));
    let (ma, mb) = (mesh(&state, faces[0]), mesh(&state, faces[1]));
    let (aa, ab) = (alphas(&state, faces[0]), alphas(&state, faces[1]));
    let mut shared = 0;
    for (k, p) in ma.base_positions.iter().enumerate() {
        if let Some(j) = mb.base_positions.iter().position(|q| q.distance(*p) < 1e-6) {
            assert!((aa[k] - ab[j]).abs() < 1e-6, "{p}: {} vs {}", aa[k], ab[j]);
            shared += 1;
        }
    }
    assert_eq!(shared, 9);
}

#[test]
fn tag_painting() {
    use hp_app::Tool as _;
    let (mut state, mut tool, face) = floor_with_disp(3);
    state.doc.clear_face_selection();
    tool.interaction = Interaction::Tag;
    tool.brush.tag.kind = TagKind::Walkable;
    tool.brush.tag.value = TagValue::Off;
    tool.brush.radius = 40.0;
    let steps = undo_steps(&state);
    tool.begin_stroke(&mut state, 0);
    assert!(tool.paint_at(&mut state, DVec3::ZERO, false) > 0);
    tool.end_stroke(&mut state);
    assert_eq!(undo_steps(&state), steps + 1);
    assert_eq!(state.doc.undo_label(), Some("Tag displacement"));
    let states = |state: &AppState| disp_tri_states(&disp_info(state, face), &mesh(state, face));
    let walk = states(&state);
    let off = walk.iter().filter(|(w, _)| !w).count();
    assert!(off > 0 && off < walk.len(), "only under the brush: {off} of {}", walk.len());
    assert!(walk.iter().all(|(_, b)| *b), "buildable still follows the flat slope");
    tool.begin_stroke(&mut state, 0);
    tool.paint_at(&mut state, DVec3::ZERO, true);
    tool.end_stroke(&mut state);
    assert!(states(&state).iter().all(|(w, _)| *w));
    state.doc.select_faces(&[face], SelectOp::Replace);
    let n = tool.tag_selection(&mut state, TagKind::Remove, TagValue::On);
    assert_eq!(n, 128);
    assert!(disp_info(&state, face).triangle_tags.iter().all(|t| t & DISP_TAG_FORCE_REMOVE_BIT != 0));
    tool.mask_select = true;
    tool.mask_grid = true;
    let out = retained(&tool, &state, &above(0.0, 0.0));
    assert_eq!(out.triangles.len(), 128, "removed triangles are tinted");
    assert!(out.triangles.iter().all(|t| t.color == crate::disp::highlight::REMOVED));
    tool.begin_stroke(&mut state, 0);
    tool.paint_at(&mut state, DVec3::ZERO, false);
    tool.end_stroke(&mut state);
    assert!(disp_info(&state, face).triangle_tags.iter().all(|t| t & DISP_TAG_FORCE_REMOVE_BIT != 0));
    assert!(tool.set_option(&mut state, "tag_value", "auto"));
    assert_eq!(tool.brush.tag.value, TagValue::Auto);
}

#[test]
fn cast_shadows_option() {
    let (mut state, _tool, face) = floor_with_disp(2);
    assert_eq!(crate::disp::commands::cast_shadows_state(&state.doc, &[face]), Some(true));
    assert!(!vmf(&state).contains("hp_cast_shadows"));
    assert_eq!(crate::disp::commands::set_cast_shadows(&mut state, false), 1);
    assert_eq!(state.doc.undo_label(), Some("Displacement shadows"));
    let text = vmf(&state);
    assert!(text.contains("\"hp_cast_shadows\" \"0\""), "written to the dispinfo");
    let map = hp_vmf::parse(&text).expect("read");
    let d = map.world.solids.iter().flat_map(|s| s.sides.iter()).find_map(|s| s.disp.as_ref()).expect("displacement");
    assert!(!d.casts_shadows(), "read back");
    assert_eq!(crate::disp::commands::cast_shadows_state(&state.doc, &[face]), Some(false));
    assert_eq!(crate::disp::commands::set_cast_shadows(&mut state, true), 1);
    assert!(!vmf(&state).contains("hp_cast_shadows"), "on: the key is gone");
}

#[test]
fn sculpt_bounds_limit() {
    let mut peaks = Vec::new();
    for bounds in [BoundsLimit::Attenuated, BoundsLimit::Additive] {
        let (mut state, mut tool, face) = floor_with_disp(3);
        state.doc.clear_face_selection();
        tool.interaction = Interaction::Sculpt;
        tool.brush.sculpt.bounds = bounds;
        tool.brush.sculpt.distance = 16.0;
        tool.brush.radius = 64.0;
        state.frame.dt = 0.1;
        tool.begin_stroke(&mut state, 0);
        for _ in 0..10 {
            tool.paint_at(&mut state, DVec3::ZERO, false);
        }
        tool.end_stroke(&mut state);
        peaks.push(distances(&state, face).iter().copied().fold(0.0f64, f64::max));
    }
    assert!((peaks[0] - 16.0).abs() < 1e-6, "attenuated: the offset once ({})", peaks[0]);
    assert!(peaks[1] > 40.0, "additive: more and more ({})", peaks[1]);
}

#[test]
fn smoothing_opens_no_crack_along_fixed_neighbours() {
    for powers in [[3, 3, 3, 3], [2, 3, 4, 3]] {
        for selected in [true, false] {
            let mut state = state_with(&[]);
            let mut tool = DisplacementTool::new();
            let faces = quad_tiles(&mut state, &mut tool, powers);
            state.doc.clear_face_selection();
            tool.brush.autosew = true;
            tool.brush.radius = 120.0;
            state.frame.dt = 0.1;
            tool.begin_stroke(&mut state, 0);
            for (x, y) in [(-60.0, -40.0), (-20.0, -10.0), (10.0, 5.0), (40.0, 30.0)] {
                tool.paint_at(&mut state, DVec3::new(x, y, 0.0), false);
            }
            tool.end_stroke(&mut state);
            assert!(max_crack(&state, &faces) < 1e-6, "{powers:?}: sewn relief");
            let before: Vec<Vec<f64>> = faces.iter().map(|f| distances(&state, *f)).collect();
            tool.brush.autosew = false;
            tool.brush.geo_effect = GeoEffect::Smooth;
            tool.brush.smooth = 1.0;
            tool.brush.radius = 100.0;
            if selected {
                state.doc.select_faces(&faces[..1], SelectOp::Replace);
            } else {
                state.doc.clear_face_selection();
            }
            tool.begin_stroke(&mut state, 0);
            for _ in 0..6 {
                tool.paint_at(&mut state, DVec3::new(-10.0, -10.0, 0.0), false);
            }
            tool.end_stroke(&mut state);
            let crack = max_crack(&state, &faces);
            assert!(crack < 1e-6, "{powers:?} selected {selected}: crack {crack}");
            assert_ne!(distances(&state, faces[0]), before[0], "{powers:?} selected {selected}: smoothed");
            if selected {
                for (f, old) in faces.iter().zip(&before).skip(1) {
                    assert_eq!(&distances(&state, *f), old, "{powers:?}: unselected neighbours stay");
                }
            }
        }
    }
}

#[test]
fn alt_right_click_picks_the_axis() {
    let (mut state, mut tool, _) = floor_with_disp(3);
    let view = above(0.0, 0.0);
    let c = view.rect.center();
    let mods = Modifiers { alt: true, ..Modifiers::default() };
    let f = PointerFrame { pos: Some(c), right_pressed: true, right_down: true, mods, ..PointerFrame::default() };
    assert_eq!(tool.handle_pointer(&mut state, &view, f), ToolFlow::Handled);
    assert_eq!(tool.brush.axis, PaintAxis::Custom);
    assert!((tool.brush.custom_axis - DVec3::Z).length() < 1e-6, "{}", tool.brush.custom_axis);
    assert!(!tool.is_painting());
}

#[test]
fn painting_goes_on_while_the_camera_moves() {
    let (mut state, mut tool, face) = floor_with_disp(4);
    state.doc.clear_face_selection();
    tool.brush.radius = 24.0;
    state.frame.dt = 0.05;
    let steps = undo_steps(&state);
    let first = above(-96.0, 0.0);
    let c = first.rect.center();
    let press = PointerFrame { pos: Some(c), pressed: true, down: true, ..PointerFrame::default() };
    tool.handle_pointer(&mut state, &first, press);
    state.frame.time += 0.5;
    let mut offset0: Option<f64> = None;
    for i in 0..=8 {
        state.frame.index += 1;
        let view = above(-96.0 + f64::from(i) * 24.0, 0.0);
        let held = PointerFrame {
            pos: Some(c),
            pressed: false,
            down: true,
            right_pressed: i == 0,
            right_down: true,
            mods: Modifiers { shift: true, alt: true, ..Modifiers::default() },
        };
        tool.handle_pointer(&mut state, &view, navigating_frame(held));
        assert!(tool.is_painting(), "frame {i}");
        let hover = state.hover_world.expect("brush on the surface");
        let offset = hover.x - view.cam3d.pos.x;
        let first_offset = *offset0.get_or_insert(offset);
        assert!((offset - first_offset).abs() < 1.0, "the brush follows the camera: {hover}");
    }
    tool.handle_pointer(&mut state, &above(96.0, 0.0), PointerFrame { pos: Some(c), ..PointerFrame::default() });
    assert!(!tool.is_painting());
    assert_eq!(undo_steps(&state), steps + 1, "one stroke");
    assert_eq!(tool.brush.axis, PaintAxis::FaceNormal, "no axis pick");
    assert_eq!(tool.brush.radius, 24.0, "no radius drag");
    let m = mesh(&state, face);
    for x in [-96.0, 0.0, 96.0] {
        let k = m.base_positions.iter().position(|b| (b.x - x).abs() < 1e-6 && b.y.abs() < 1e-6).expect("vertex");
        assert!(m.positions[k].z > 0.5, "raised at x = {x} (raise, not lower: Shift was for the camera)");
    }
}
