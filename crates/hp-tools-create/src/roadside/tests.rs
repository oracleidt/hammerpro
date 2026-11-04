#![allow(clippy::unwrap_used, clippy::expect_used, clippy::panic)]

use glam::DVec2;
use hp_app::ViewKind;
use hp_app::state::AppState;
use hp_editor::{ObjRef, SelectOp};
use hp_geom::roadside::{GutterProfile, GutterShape, Sides};
use hp_geom::validate_solid;

use super::*;
use crate::curve::apply::{Target, create_curve};
use crate::curve::{CurveMeta, Section};
use crate::tests_support::settle;

fn curve(points: &[(f64, f64)]) -> CurveMeta {
    CurveMeta {
        kind: ViewKind::Top,
        points: points.iter().map(|p| DVec2::new(p.0, p.1)).collect(),
        depth: (0.0, 8.0),
        segments: 8,
        thickness: 64.0,
        tex_scale: 0.25,
        section: Section::default(),
    }
}

fn make_road(state: &mut AppState, points: &[(f64, f64)]) -> CurveMeta {
    let meta = curve(points);
    create_curve(&mut state.doc, &meta, "dev/road", 16, Target::World).unwrap();
    meta
}

fn meta_for(sources: Vec<Source>) -> RoadsideMeta {
    let mut m = RoadsideMeta { sources, ..Default::default() };
    m.materials.top = "dev/walk".to_owned();
    m
}

#[test]
fn a_sidewalk_is_valid_tagged_selected_and_one_undo_step() {
    let mut state = settle();
    let road = make_road(&mut state, &[(0.0, 0.0), (400.0, 0.0)]);
    let before = state.doc.scene().solids().len();
    let meta = meta_for(vec![Source::Curve(road)]);
    let made = create(&mut state.doc, &meta, 16, Target::World).unwrap();
    assert_eq!(made.ids.len(), 16);
    for id in &made.ids {
        let node = state.doc.scene().solid(*id).unwrap();
        assert!(validate_solid(&node.solid.sides).is_ok());
        assert!(state.doc.roadside_tag(*id).is_some());
        assert!(node.solid.sides.iter().all(|s| s.material == "dev/walk"));
    }
    assert!(state.doc.selected_roadside_piece().is_some());
    let (record, back) = read(&state.doc, made.ids[0]).unwrap();
    assert_eq!(record.ids.len(), 16);
    assert_eq!(back, meta);
    assert!(state.doc.undo());
    assert_eq!(state.doc.scene().solids().len(), before);
    assert!(state.doc.redo());
    assert_eq!(state.doc.scene().solids().len(), before + 16);
}

#[test]
fn editing_replaces_the_set_in_one_step_and_keeps_the_new_numbers() {
    let mut state = settle();
    let road = make_road(&mut state, &[(0.0, 0.0), (400.0, 0.0)]);
    let meta = meta_for(vec![Source::Curve(road)]);
    let made = create(&mut state.doc, &meta, 16, Target::World).unwrap();
    let (record, mut edited) = read(&state.doc, made.ids[0]).unwrap();
    edited.sides = Sides::Left;
    edited.sidewalk.width = 40.0;
    let total = state.doc.scene().solids().len();
    let again = replace(&mut state.doc, &record, &edited, 16).unwrap();
    assert_eq!(again.ids.len(), 8);
    assert_eq!(state.doc.scene().solids().len(), total - 8);
    let (_, stored) = read(&state.doc, again.ids[0]).unwrap();
    assert_eq!(stored.sidewalk.width, 40.0);
    assert!(state.doc.undo());
    assert_eq!(state.doc.scene().solids().len(), total);
}

#[test]
fn curb_and_gutter_stack_outward_with_their_own_materials() {
    let mut state = settle();
    let road = make_road(&mut state, &[(0.0, 0.0), (400.0, 0.0)]);
    let mut meta = meta_for(vec![Source::Curve(road)]);
    meta.kind = Kind::Curb;
    meta.sides = Sides::Left;
    meta.use_gutter = true;
    meta.gutter = GutterProfile { shape: GutterShape::U, slot: true, slot_every: 4, ..Default::default() };
    meta.materials.curb = "dev/curb".to_owned();
    meta.materials.gutter = "dev/gutter".to_owned();
    let made = create(&mut state.doc, &meta, 16, Target::World).unwrap();
    let mut curb_min_y = f64::MAX;
    let mut gutter_max_y = f64::MIN;
    for id in &made.ids {
        let node = state.doc.scene().solid(*id).unwrap();
        assert!(validate_solid(&node.solid.sides).is_ok());
        let m = &node.solid.sides[0].material;
        let (lo, hi) = (node.bounds.min.y, node.bounds.max.y);
        if m == "dev/curb" {
            curb_min_y = curb_min_y.min(lo);
        } else {
            assert_eq!(m, "dev/gutter");
            gutter_max_y = gutter_max_y.max(hi);
        }
    }
    assert!((gutter_max_y - 48.0).abs() < 1e-6, "{gutter_max_y}");
    assert!((curb_min_y - 48.0).abs() < 1e-6, "{curb_min_y}");
}

#[test]
fn a_curb_set_needs_something_to_build() {
    let mut state = settle();
    let road = make_road(&mut state, &[(0.0, 0.0), (400.0, 0.0)]);
    let mut meta = meta_for(vec![Source::Curve(road)]);
    meta.kind = Kind::Curb;
    meta.use_curb = false;
    meta.use_gutter = false;
    assert!(create(&mut state.doc, &meta, 16, Target::World).is_err());
}

#[test]
fn face_material_goes_on_the_road_facing_side_only() {
    let mut state = settle();
    let road = make_road(&mut state, &[(0.0, 0.0), (400.0, 0.0)]);
    let mut meta = meta_for(vec![Source::Curve(road)]);
    meta.sides = Sides::Left;
    meta.materials.face = "dev/face".to_owned();
    let made = create(&mut state.doc, &meta, 16, Target::World).unwrap();
    let node = state.doc.scene().solid(made.ids[3]).unwrap();
    let faces: Vec<_> = node.solid.sides.iter().filter(|s| s.material == "dev/face").collect();
    assert_eq!(faces.len(), 1);
    assert!(faces[0].plane.iter().all(|p| (p.y - 32.0).abs() < 1e-6));
}

#[test]
fn two_curves_meeting_make_a_rounded_junction() {
    let mut state = settle();
    let main = make_road(&mut state, &[(-300.0, 0.0), (300.0, 0.0)]);
    let side = make_road(&mut state, &[(0.0, 400.0), (0.0, 0.0)]);
    let mut meta = meta_for(vec![Source::Curve(main), Source::Curve(side)]);
    meta.radius = 24.0;
    let made = create(&mut state.doc, &meta, 16, Target::World).unwrap();
    for id in &made.ids {
        let node = state.doc.scene().solid(*id).unwrap();
        assert!(validate_solid(&node.solid.sides).is_ok());
        let inside_mouth = node.bounds.min.x > -30.0 && node.bounds.max.x < 30.0 && node.bounds.min.y < 90.0;
        assert!(!inside_mouth, "a piece blocks the mouth of the side road");
    }
    assert!(made.ids.len() > 30);
}

#[test]
fn sources_come_from_the_selected_curves_once() {
    let mut state = settle();
    make_road(&mut state, &[(0.0, 0.0), (400.0, 0.0)]);
    make_road(&mut state, &[(0.0, 300.0), (400.0, 300.0)]);
    let all: Vec<ObjRef> = state.doc.scene().solids().keys().map(|id| ObjRef::Solid(*id)).collect();
    state.doc.select(&all, SelectOp::Replace);
    let sources = sources_of_selection(&state.doc);
    assert_eq!(sources.len(), 2);
    state.doc.clear_selection();
    assert!(sources_of_selection(&state.doc).is_empty());
}

#[test]
fn bad_numbers_are_refused_before_anything_is_created() {
    let mut state = settle();
    let road = make_road(&mut state, &[(0.0, 0.0), (400.0, 0.0)]);
    let total = state.doc.scene().solids().len();
    let mut meta = meta_for(vec![Source::Curve(road)]);
    meta.sidewalk.width = 0.0;
    assert!(create(&mut state.doc, &meta, 16, Target::World).is_err());
    meta.sidewalk.width = 64.0;
    meta.sources.clear();
    assert!(create(&mut state.doc, &meta, 16, Target::World).is_err());
    assert_eq!(state.doc.scene().solids().len(), total);
}

#[test]
fn two_crossing_curves_get_an_open_intersection() {
    let mut state = settle();
    let a = make_road(&mut state, &[(-300.0, 0.0), (300.0, 0.0)]);
    let b = make_road(&mut state, &[(0.0, -300.0), (0.0, 300.0)]);
    let meta = meta_for(vec![Source::Curve(a), Source::Curve(b)]);
    let made = create(&mut state.doc, &meta, 16, Target::World).unwrap();
    for id in &made.ids {
        let node = state.doc.scene().solid(*id).unwrap();
        assert!(validate_solid(&node.solid.sides).is_ok());
        let b = node.bounds;
        let in_x = b.min.x > -30.0 && b.max.x < 30.0 && b.min.y.abs() < 100.0 && b.max.y.abs() < 100.0;
        assert!(!in_x, "a piece blocks the crossing");
    }
    assert!(made.ids.len() > 32);
}

#[test]
fn a_front_view_curve_gets_details_in_its_own_plane() {
    let mut state = settle();
    let mut front = curve(&[(0.0, 0.0), (400.0, 0.0)]);
    front.kind = ViewKind::Front;
    front.depth = (-16.0, 16.0);
    create_curve(&mut state.doc, &front, "dev/road", 16, Target::World).unwrap();
    let meta = meta_for(vec![Source::Curve(front)]);
    let made = create(&mut state.doc, &meta, 16, Target::World).unwrap();
    assert_eq!(made.ids.len(), 16);
    for id in &made.ids {
        let node = state.doc.scene().solid(*id).unwrap();
        assert!(validate_solid(&node.solid.sides).is_ok());
        assert!(node.bounds.max.x <= 16.0 + 6.0 + 1e-6 && node.bounds.min.x >= -16.0 - 16.0 - 1e-6);
        assert!(node.bounds.max.z.abs() >= 32.0 || node.bounds.min.z.abs() >= 32.0 - 1e-6);
    }
}

#[test]
fn terrain_refuses_a_curve_of_another_view_with_a_message() {
    let state = settle();
    let mut side = curve(&[(0.0, 0.0), (400.0, 0.0)]);
    side.kind = ViewKind::Side;
    let doc = &state.doc;
    let err = resolve_all(doc, &[Source::Curve(side.clone())]).unwrap_err();
    assert!(err.to_string().contains("from above"));
    let mixed = resolve_frame(doc, &[Source::Curve(side), Source::Curve(curve(&[(0.0, 0.0), (400.0, 0.0)]))]);
    assert!(mixed.is_err());
}
