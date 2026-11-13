#![allow(clippy::unwrap_used, clippy::expect_used, clippy::panic)]

use std::collections::HashMap;

use egui::Pos2;
use glam::{DVec2, DVec3};
use hp_app::{Action, Tool, ToolId, ViewKind};
use hp_editor::{Document, ObjRef};
use hp_render::Overlay;

use super::build::{GenEnv, model_names, plan, profile_parts};
use super::curve::{Path, sag_depth};
use super::model::*;
use super::store;
use super::{SplineTool, apply};
use crate::common::Mouse;
use crate::tests_support::{info_2d, settle};

fn pts(list: &[(f64, f64, f64)]) -> Vec<SplinePoint> {
    list.iter().map(|(x, y, z)| SplinePoint::new(DVec3::new(*x, *y, *z))).collect()
}

fn meta(mode: Mode, list: &[(f64, f64, f64)]) -> SplineMeta {
    SplineMeta { mode, points: pts(list), ..SplineMeta::default() }
}

fn env_with(models: &[&str]) -> GenEnv<'static> {
    let mut bounds = HashMap::new();
    for m in models {
        bounds.insert((*m).to_owned(), (DVec3::new(-32.0, -2.0, 0.0), DVec3::new(32.0, 2.0, 48.0)));
    }
    GenEnv { ground: None, bounds }
}

#[test]
fn every_option_round_trips_through_the_text_form() {
    let mut m = SplineMeta {
        points: vec![
            SplinePoint { roll: 15.0, width: 1.5, ..SplinePoint::new(DVec3::new(1.5, -2.25, 64.0)) },
            SplinePoint {
                handle_in: Some(DVec3::new(-10.0, 0.0, 0.0)),
                handle_out: None,
                ..SplinePoint::new(DVec3::new(300.0, 40.0, 70.0))
            },
        ],
        ..SplineMeta::default()
    };
    for (k, v) in [
        ("mode", "props"),
        ("interp", "bezier"),
        ("closed", "on"),
        ("follow_terrain", "true"),
        ("terrain_offset", "4"),
        ("material", "metal/metalpipe001a"),
        ("profile", "custom"),
        ("custom", "0:0 16:0 8:12"),
        ("models", "models/a.mdl, models/b.mdl"),
        ("side", "both"),
        ("jitter", "12.5"),
        ("scale_min", "0.8"),
        ("scale_max", "1.25"),
        ("cable", "brush"),
        ("fence", "rails"),
        ("post_model", "models/post.mdl"),
        ("rope_every", "96"),
        ("seed", "77"),
    ] {
        m.set_option(k, v).unwrap();
    }
    let m = m.clamped();
    let text = m.encode();
    assert!(SplineMeta::is_spline_data(&text));
    assert!(ascii_safe(&text));
    assert_eq!(SplineMeta::decode(&text), Some(m.clone()));
    for o in OPTIONS {
        let v = m.get_option(o.key).unwrap();
        let mut copy = m.clone();
        copy.set_option(o.key, &v).unwrap_or_else(|e| panic!("{}: {e}", o.key));
        assert_eq!(copy.get_option(o.key).unwrap(), v, "{}", o.key);
    }
}

#[test]
fn bad_values_are_refused() {
    let mut m = SplineMeta::default();
    assert!(m.set_option("mode", "spiral").is_err());
    assert!(m.set_option("sides", "2").is_err());
    assert!(m.set_option("sides", "4.5").is_err());
    assert!(m.set_option("radius", "nan").is_err());
    assert!(m.set_option("models", " , ").is_err());
    assert!(m.set_option("custom", "1:2 x:3").is_err());
    assert!(m.set_option("nope", "1").is_err());
    assert!(m.set_option("material", "").is_err());
    assert_eq!(SplineMeta::decode(""), None);
    assert_eq!(SplineMeta::decode("sp2|pts=0,0,0,0,1"), None);
    assert_eq!(SplineMeta::decode("sp1|mode=path"), None);
    assert_eq!(SplineMeta::decode("sp1|pts=0,0,nan,0,1"), None);
    assert_eq!(SplineMeta::decode("sp1|pts=0,0,0,0"), None);
    assert!(SplineMeta::decode("sp1|future=7|pts=0,0,0,0,1;10,0,0,0,1").is_some());
}

#[test]
fn catmull_rom_passes_through_every_point_and_loops_close() {
    let m = meta(Mode::Path, &[(0.0, 0.0, 0.0), (200.0, 100.0, 0.0), (400.0, 0.0, 32.0), (600.0, 50.0, 32.0)]);
    let path = Path::build(&m, None).unwrap();
    for (k, p) in m.points.iter().enumerate() {
        let f = path.at(path.control_s[k]);
        assert!(f.pos.distance(p.pos) < 1e-6, "{k}: {:?}", f.pos);
    }
    assert!(path.length() > 600.0);
    let mut closed = m.clone();
    closed.closed = true;
    let loop_path = Path::build(&closed, None).unwrap();
    assert!(loop_path.length() > path.length());
    assert!(loop_path.samples.last().unwrap().pos.distance(m.points[0].pos) < 1e-6);
    assert!(loop_path.at(loop_path.length() + 10.0).pos.distance(loop_path.at(10.0).pos) < 1e-6);
    let two = meta(Mode::Path, &[(0.0, 0.0, 0.0), (0.0, 0.0, 0.0)]);
    assert!(Path::build(&two, None).is_err());
}

#[test]
fn bezier_handles_shape_the_curve() {
    let mut m = meta(Mode::Path, &[(0.0, 0.0, 0.0), (256.0, 0.0, 0.0)]);
    m.interp = Interp::Bezier;
    let flat = Path::build(&m, None).unwrap();
    assert!(flat.samples.iter().all(|s| s.pos.y.abs() < 1e-6));
    m.points[0].handle_out = Some(DVec3::new(64.0, 128.0, 0.0));
    let bent = Path::build(&m, None).unwrap();
    assert!(bent.samples.iter().any(|s| s.pos.y > 40.0));
    assert!(bent.at(0.0).tangent.y > 0.5);
}

#[test]
fn frames_are_orthonormal_and_roll_banks_them() {
    let mut m = meta(Mode::Path, &[(0.0, 0.0, 0.0), (256.0, 0.0, 0.0)]);
    let f = Path::build(&m, None).unwrap().at(64.0);
    assert!(f.right.distance(DVec3::NEG_Y) < 1e-9 && f.up.distance(DVec3::Z) < 1e-9);
    m.points[0].roll = 30.0;
    m.points[1].roll = 30.0;
    let r = Path::build(&m, None).unwrap().at(64.0);
    assert!((r.up.dot(DVec3::Z) - 30f64.to_radians().cos()).abs() < 1e-9);
    assert!(r.right.dot(r.up).abs() < 1e-9 && r.tangent.dot(r.up).abs() < 1e-9);
}

#[test]
fn follow_terrain_puts_the_path_on_the_ground() {
    let mut m = meta(Mode::Path, &[(0.0, 0.0, 500.0), (512.0, 0.0, 500.0)]);
    m.follow_terrain = true;
    m.terrain_offset = 2.0;
    let ground = |p: DVec2, _z: f64| Some(p.x * 0.25);
    let path = Path::build(&m, Some(&ground)).unwrap();
    for s in &path.samples {
        assert!((s.pos.z - (s.pos.x * 0.25 + 2.0)).abs() < 1e-9);
    }
    m.follow_terrain = false;
    let free = Path::build(&m, Some(&ground)).unwrap();
    assert!(free.samples.iter().all(|s| (s.pos.z - 500.0).abs() < 1e-9));
}

#[test]
fn sweep_profiles_build_convex_brushes_along_the_path() {
    let mut m = meta(Mode::Sweep, &[(0.0, 0.0, 0.0), (256.0, 0.0, 0.0), (512.0, 128.0, 0.0)]);
    m.segment = 32.0;
    for profile in Profile::ALL {
        m.profile = *profile;
        if *profile == Profile::Custom {
            m.custom = vec![DVec2::new(-8.0, 0.0), DVec2::new(8.0, 0.0), DVec2::new(0.0, 12.0)];
        }
        let parts = profile_parts(&m).unwrap();
        let p = plan(&m, &GenEnv::default()).unwrap();
        let stations = (p.length / 32.0).round() as usize;
        assert_eq!(p.solids.len(), stations * parts.len(), "{profile:?}");
        assert!(p.entities.is_empty());
        for s in &p.solids {
            assert_eq!(s.rings[0].len(), s.rings[1].len());
            assert!(s.uv.is_some());
        }
        let mut doc = Document::new_empty();
        let solids = store::build_solids(&p, m.tex_scale, 16, doc.peek_next_side_id()).unwrap();
        doc.create_solids(solids, hp_editor::Owner::World).unwrap();
    }
    m.profile = Profile::Custom;
    m.custom = vec![
        DVec2::new(0.0, 0.0),
        DVec2::new(10.0, 0.0),
        DVec2::new(5.0, 2.0),
        DVec2::new(10.0, 10.0),
        DVec2::new(0.0, 10.0),
    ];
    assert!(plan(&m, &GenEnv::default()).is_err(), "a concave profile is refused");
}

#[test]
fn texture_runs_on_along_the_path() {
    let mut m = meta(Mode::Sweep, &[(0.0, 0.0, 0.0), (512.0, 0.0, 0.0)]);
    m.profile = Profile::Curb;
    m.segment = 128.0;
    let p = plan(&m, &GenEnv::default()).unwrap();
    let solids = store::build_solids(&p, 0.25, 16, 1).unwrap();
    let u_at = |sides: &Vec<hp_vmf::Side>, x: f64| -> Option<f64> {
        sides
            .iter()
            .find(|s| s.normal().z > 0.9)
            .map(|s| (DVec3::new(x, 0.0, 8.0).dot(s.uaxis.axis)) / s.uaxis.scale + s.uaxis.shift)
    };
    let end_of_first = u_at(&solids[0], 128.0).unwrap();
    let start_of_second = u_at(&solids[1], 128.0).unwrap();
    assert!((end_of_first - start_of_second).abs() < 1e-2, "{end_of_first} {start_of_second}");
}

#[test]
fn fences_repeat_models_with_posts_and_caps() {
    let mut m = meta(Mode::Fence, &[(0.0, 0.0, 0.0), (256.0, 0.0, 0.0), (512.0, 0.0, 64.0)]);
    m.model = "models/fence.mdl".into();
    m.post_model = "models/post.mdl".into();
    m.post_every_segment = true;
    m.start_cap = "models/cap.mdl".into();
    m.end_cap = "models/cap.mdl".into();
    let env = env_with(&["models/fence.mdl", "models/post.mdl", "models/cap.mdl"]);
    assert_eq!(model_names(&m).len(), 3);
    let p = plan(&m, &env).unwrap();
    let segs = p.count(super::EntityRole::Segment);
    assert_eq!(segs, (p.length / 64.0).round() as usize, "the model length is the spacing");
    assert_eq!(p.count(super::EntityRole::Post), segs + 1);
    assert_eq!(p.count(super::EntityRole::Cap), 2);
    let sloped = p.entities.iter().rfind(|e| e.role == super::EntityRole::Segment).unwrap();
    assert!(sloped.angles.x < -1.0, "segments tilt with the slope: {:?}", sloped.angles);
    m.align_slope = false;
    let flat = plan(&m, &env).unwrap();
    assert!(flat.entities.iter().all(|e| e.angles.x == 0.0));
    m.fence = FenceStyle::Rails;
    let rails = plan(&m, &env).unwrap();
    assert!(rails.entities.is_empty());
    let posts = (rails.length / m.post_spacing).round() as usize + 1;
    let rail_brushes = (rails.length / m.segment).round() as usize * m.rail_count as usize;
    assert_eq!(rails.solids.len(), posts + rail_brushes);
}

#[test]
fn cables_chain_ropes_or_sweep_hanging_brushes() {
    let mut m = meta(Mode::Cable, &[(0.0, 0.0, 256.0), (512.0, 0.0, 256.0), (1024.0, 128.0, 256.0)]);
    let p = plan(&m, &GenEnv::default()).unwrap();
    assert_eq!(p.entities.len(), 3);
    assert_eq!(p.entities[0].class, "move_rope");
    assert_eq!(p.entities[1].class, "keyframe_rope");
    assert_eq!(p.entities[0].next, Some(1));
    assert_eq!(p.entities[2].next, None);
    assert_eq!(p.wires.len(), 2);
    let low = p.wires[0].iter().map(|v| v.z).fold(f64::INFINITY, f64::min);
    assert!((256.0 - low - sag_depth(512.0, m.slack)).abs() < 1.0);
    m.closed = true;
    let looped = plan(&m, &GenEnv::default()).unwrap();
    assert_eq!(looped.entities[2].next, Some(0));
    m.closed = false;
    m.rope_every = 128.0;
    assert!(plan(&m, &GenEnv::default()).unwrap().entities.len() > 5);
    m.rope_every = 0.0;
    m.cable = CableStyle::Brush;
    let brush = plan(&m, &GenEnv::default()).unwrap();
    assert!(brush.entities.is_empty() && !brush.solids.is_empty());
}

#[test]
fn prop_lines_are_seeded_spaced_and_avoid_each_other() {
    let mut m = meta(Mode::Props, &[(0.0, 0.0, 0.0), (1024.0, 0.0, 0.0)]);
    m.models = vec!["models/a.mdl".into(), "models/b.mdl".into()];
    m.spacing = 128.0;
    m.jitter = 16.0;
    m.yaw_jitter = 30.0;
    m.scale_min = 0.5;
    m.scale_max = 1.5;
    let env = env_with(&["models/a.mdl", "models/b.mdl"]);
    let a = plan(&m, &env).unwrap();
    assert_eq!(a, plan(&m, &env).unwrap(), "same seed, same props");
    assert!(a.entities.iter().any(|e| e.scale != 1.0));
    m.seed = 2;
    assert_ne!(a, plan(&m, &env).unwrap());
    m.jitter = 0.0;
    m.yaw_jitter = 0.0;
    m.scale_min = 1.0;
    m.scale_max = 1.0;
    m.sides_mode = Sides::Both;
    m.offset_x = 64.0;
    let both = plan(&m, &env).unwrap();
    assert_eq!(both.entities.len(), 2 * 9);
    assert!(both.entities.iter().any(|e| e.origin.y > 60.0) && both.entities.iter().any(|e| e.origin.y < -60.0));
    m.sides_mode = Sides::Center;
    m.spacing = 16.0;
    let crowded = plan(&m, &env).unwrap();
    assert!(crowded.skipped > 0, "64 units wide props 16 units apart overlap");
    m.avoid_overlap = false;
    assert_eq!(plan(&m, &env).unwrap().skipped, 0);
    m.drop = true;
    let ground = |_: DVec2, _: f64| Some(-40.0);
    let dropped = plan(&m, &GenEnv { ground: Some(&ground), bounds: env.bounds.clone() }).unwrap();
    assert!(dropped.entities.iter().all(|e| e.origin.z == -40.0));
}

fn create(state: &mut hp_app::AppState, m: &SplineMeta) -> apply::Generated {
    apply::generate(state, None, m).unwrap()
}

#[test]
fn a_spline_is_kept_in_the_map_and_regenerates_after_reopening() {
    let mut state = settle();
    let mut m = meta(Mode::Sweep, &[(0.0, 0.0, 0.0), (256.0, 64.0, 0.0), (512.0, 0.0, 32.0)]);
    m.segment = 64.0;
    let before = state.doc.undo_labels().len();
    let done = create(&mut state, &m);
    assert_eq!(state.doc.undo_labels().len(), before + 1, "one undo step");
    let anchor = done.applied.anchor;
    assert_eq!(state.doc.scene().entity(anchor).unwrap().entity.classname, store::ANCHOR_CLASS);
    assert!(done.applied.detail.is_some(), "func_detail by default");
    let group = done.applied.group.unwrap();
    let members = state.doc.scene().group_members(group);
    assert!(members.contains(&ObjRef::Entity(anchor)));
    assert!(done.applied.outputs.iter().all(|o| members.contains(o)));
    let entry = store::read(&state.doc, anchor).unwrap();
    assert_eq!(entry.meta.points, m.clone().clamped().points);
    assert_eq!(entry.outputs, done.applied.outputs);
    for o in &done.applied.outputs {
        assert_eq!(store::anchor_of(&state.doc, *o), Some(anchor));
    }
    let detail_solid = state.doc.scene().entity_solid_ids(done.applied.detail.unwrap())[0];
    assert_eq!(store::anchor_of(&state.doc, ObjRef::Solid(detail_solid)), Some(anchor));

    let text = state.doc.to_vmf_string();
    let reopened = Document::from_map(hp_vmf::parse(&text).unwrap());
    let back = store::read(&reopened, store::anchors(&reopened)[0]).unwrap();
    assert_eq!(back.meta, entry.meta);
    assert_eq!(back.outputs.len(), entry.outputs.len());

    let moved: Vec<ObjRef> = members.clone();
    state.doc.translate(&moved, DVec3::new(100.0, 0.0, 0.0)).unwrap();
    let after = store::read(&state.doc, anchor).unwrap();
    assert!((after.meta.points[1].pos - DVec3::new(356.0, 64.0, 0.0)).length() < 1e-6, "the points follow the group");

    let mut edit = after.meta.clone();
    edit.points.push(SplinePoint::new(DVec3::new(900.0, 0.0, 32.0)));
    let old_outputs = after.outputs.clone();
    let again = apply::generate(&mut state, Some(anchor), &edit).unwrap();
    assert_eq!(again.applied.anchor, anchor);
    assert!(old_outputs.iter().all(|o| !state.doc.scene().contains(*o)), "the old objects are replaced");
    assert_eq!(store::read(&state.doc, anchor).unwrap().meta.points.len(), 4);
    assert_eq!(store::list(&state.doc).len(), 1);
    assert_eq!(state.doc.undo_label(), Some("Edit spline"));
    state.doc.undo();
    assert_eq!(store::read(&state.doc, anchor).unwrap().meta.points.len(), 3);
    assert!(old_outputs.iter().all(|o| state.doc.scene().contains(*o)));
}

#[test]
fn baking_leaves_plain_objects() {
    let mut state = settle();
    let m = meta(Mode::Cable, &[(0.0, 0.0, 128.0), (256.0, 0.0, 128.0), (512.0, 0.0, 128.0)]);
    let done = create(&mut state, &m);
    let ropes: Vec<_> = done.applied.outputs.clone();
    assert_eq!(ropes.len(), 3);
    let names: Vec<String> = ropes
        .iter()
        .map(|o| {
            let ObjRef::Entity(id) = o else { panic!() };
            state.doc.scene().entity(*id).unwrap().entity.get("targetname").unwrap().to_owned()
        })
        .collect();
    let first = match ropes[0] {
        ObjRef::Entity(id) => state.doc.scene().entity(id).unwrap().entity.clone(),
        ObjRef::Solid(_) => panic!(),
    };
    assert_eq!(first.classname, "move_rope");
    assert_eq!(first.get("NextKey"), Some(names[1].as_str()));
    let kept = store::bake(&mut state.doc, done.applied.anchor, true).unwrap();
    assert_eq!(kept.len(), 3);
    assert!(state.doc.scene().entity(done.applied.anchor).is_none());
    assert!(store::anchors(&state.doc).is_empty());
    assert!(kept.iter().all(|o| store::owner_uid(&state.doc, *o).is_none()));
    assert!(kept.iter().all(|o| store::anchor_of(&state.doc, *o).is_none()));
    state.doc.undo();
    assert_eq!(store::anchors(&state.doc).len(), 1);
}

#[test]
fn pasted_copies_get_their_own_identity() {
    let mut state = settle();
    let m = meta(Mode::Sweep, &[(0.0, 0.0, 0.0), (256.0, 0.0, 0.0)]);
    let done = create(&mut state, &m);
    let mut all = vec![ObjRef::Entity(done.applied.anchor)];
    all.extend(done.applied.outputs.iter().copied());
    let copies = state.doc.duplicate(&all, DVec3::new(0.0, 512.0, 0.0)).unwrap();
    let anchors = store::anchors(&state.doc);
    assert_eq!(anchors.len(), 2, "{copies:?}");
    let copy = *anchors.iter().find(|a| **a != done.applied.anchor).unwrap();
    let a = store::read(&state.doc, done.applied.anchor).unwrap();
    let b = store::read(&state.doc, copy).unwrap();
    assert_eq!(a.outputs.len(), b.outputs.len());
    assert!(a.outputs.iter().all(|o| !b.outputs.contains(o)));
    let mut edit = b.meta.clone();
    edit.points[1].pos.x = 400.0;
    apply::generate(&mut state, Some(copy), &edit).unwrap();
    assert_eq!(store::read(&state.doc, done.applied.anchor).unwrap().outputs, a.outputs, "the original is untouched");
    assert_ne!(store::anchor_uid(&state.doc, copy), store::anchor_uid(&state.doc, done.applied.anchor));
}

#[test]
fn every_mode_generates_in_one_step() {
    for mode in Mode::ALL {
        let mut state = settle();
        let mut m = meta(*mode, &[(0.0, 0.0, 0.0), (256.0, 0.0, 0.0), (512.0, 128.0, 0.0)]);
        m.post_model = "models/post.mdl".into();
        m.posts_at_points = true;
        let before = state.doc.undo_labels().len();
        let done = create(&mut state, &m);
        assert_eq!(state.doc.undo_labels().len(), before + 1, "{mode:?}");
        assert_eq!(*mode == Mode::Path, done.applied.outputs.is_empty(), "{mode:?}");
        assert!(store::read(&state.doc, done.applied.anchor).is_some());
    }
}

fn click(
    tool: &mut SplineTool,
    state: &mut hp_app::AppState,
    view: &hp_app::ViewInfo,
    p: DVec3,
    mods: egui::Modifiers,
) {
    let s = view.world_to_screen(p).unwrap();
    tool.handle_mouse(state, view, &Mouse::click(s).with_modifiers(mods));
}

#[test]
fn the_tool_draws_edits_and_generates_a_spline() {
    let mut state = settle();
    let view = info_2d(ViewKind::Top, 1.0);
    let mut tool = SplineTool::new();
    tool.activate(&mut state);
    for p in [DVec3::new(-128.0, 0.0, 0.0), DVec3::new(0.0, 64.0, 0.0), DVec3::new(128.0, 0.0, 0.0)] {
        click(&mut tool, &mut state, &view, p, egui::Modifiers::NONE);
    }
    assert_eq!(tool.meta.points.len(), 3);
    assert_eq!(tool.meta.points[1].pos.truncate(), DVec2::new(0.0, 64.0));
    let mut ov = Overlay::default();
    tool.overlay(&state, &view, &mut ov);
    assert!(!ov.lines.is_empty() || !ov.lines_on_top.is_empty());

    let a = view.world_to_screen(DVec3::new(128.0, 0.0, 0.0)).unwrap();
    let b = view.world_to_screen(DVec3::new(160.0, -32.0, 0.0)).unwrap();
    tool.handle_mouse(&mut state, &view, &Mouse::drag_start(a, a));
    tool.handle_mouse(&mut state, &view, &Mouse::drag(a, b));
    tool.handle_mouse(&mut state, &view, &Mouse::drag_end(a, b));
    assert_eq!(tool.meta.points[2].pos.truncate(), DVec2::new(160.0, -32.0));

    let path = Path::build(&tool.meta, None).unwrap();
    let on = path.at(path.length() * 0.25).pos;
    let near = Pos2::new(view.world_to_screen(on).unwrap().x, view.world_to_screen(on).unwrap().y);
    tool.handle_mouse(&mut state, &view, &Mouse::click(near).with_modifiers(egui::Modifiers::CTRL));
    assert_eq!(tool.meta.points.len(), 4, "ctrl+click on the curve inserts a point");
    assert_eq!(tool.selected, Some(1));

    assert!(tool.set_option(&mut state, "mode", "sweep"));
    assert!(tool.set_option(&mut state, "selected_point", "2"));
    assert!(tool.set_option(&mut state, "point_roll", "20"));
    assert!(tool.set_option(&mut state, "point_width", "2"));
    assert!(!tool.set_option(&mut state, "point_width", "200"));
    assert!(!tool.set_option(&mut state, "mode", "nope"));
    assert_eq!(tool.meta.points[1].roll, 20.0);
    let opts = tool.option_values();
    assert!(opts.iter().any(|o| o.key == "profile" && o.value == "pipe"));
    assert!(opts.iter().any(|o| o.key == "point_width" && o.value == "2"));

    assert!(tool.generate(&mut state));
    let anchor = tool.editing.unwrap();
    assert!(store::read(&state.doc, anchor).is_some());
    assert!(!tool.is_dirty());

    assert!(tool.set_option(&mut state, "selected_point", "1"));
    tool.context_command(&mut state, 2);
    assert!(tool.is_dirty());
    state.frame.time += 1.0;
    tool.update(&mut state, &egui::Context::default());
    assert_eq!(store::read(&state.doc, anchor).unwrap().meta.points.len(), 3, "an edited spline regenerates by itself");
    assert_eq!(state.doc.undo_label(), Some("Edit spline"));

    state.doc.undo();
    tool.update(&mut state, &egui::Context::default());
    assert_eq!(tool.meta.points.len(), 4, "undo brings the tool back to the stored spline");

    hp_app::testing::run_ui(&mut state, |ui, state| tool.options_ui(ui, state));
    assert!(tool.status_hint(&state).is_some());
    assert!(!tool.context_entries(&state).is_empty());

    let mut other = SplineTool::new();
    state.doc.select(&[ObjRef::Entity(anchor)], hp_editor::SelectOp::Replace);
    other.activate(&mut state);
    assert_eq!(other.editing, Some(anchor), "opening the tool on a spline edits it");
    assert!(other.action(&mut state, Action::SplineBake));
    assert!(store::anchors(&state.doc).is_empty());
}

#[test]
fn keys_and_cancel_follow_the_other_tools() {
    let mut state = settle();
    let view = info_2d(ViewKind::Front, 1.0);
    let mut tool = SplineTool::new();
    click(&mut tool, &mut state, &view, DVec3::new(0.0, 0.0, 0.0), egui::Modifiers::NONE);
    click(&mut tool, &mut state, &view, DVec3::new(64.0, 0.0, 64.0), egui::Modifiers::NONE);
    assert_eq!(tool.meta.points.len(), 2);
    assert!(tool.cancel(&mut state), "first cancel drops the selection");
    assert!(tool.cancel(&mut state), "then the points");
    assert!(tool.meta.points.is_empty());
    assert!(!tool.cancel(&mut state));
    assert_eq!(tool.id(), ToolId::Spline);
}

#[test]
fn the_tool_icon_and_texts_exist() {
    let _state = settle();
    for (key, en, fr) in super::SPLINE_TR {
        assert!(!en.is_empty() && !fr.is_empty(), "{key}");
        assert!(!en.contains('\u{2014}') && !fr.contains('\u{2014}'));
    }
    assert!(hp_app::i18n::lint_table(super::SPLINE_TR).is_empty());
    let font = egui::FontId::proportional(14.0);
    let ctx = egui::Context::default();
    let mut out = ctx.run_ui(egui::RawInput::default(), |_| {});
    out.textures_delta.clear();
    assert!(ctx.fonts_mut(|f| f.has_glyphs(&font, ToolId::Spline.icon())), "the tool icon has a glyph");
}
