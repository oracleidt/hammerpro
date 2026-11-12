use glam::DVec3;
use hp_app::AppState;
use hp_editor::{ObjRef, Problem};
use hp_shape::{ModelShape, TriMesh};
use serde_json::json;

use super::place_ui::{self, UiPlace};
use super::place_world::pin_shape;
use super::semantic_tests::{call, ok};

const CRATE: &str = "models/hp_test/ui_crate.mdl";
const TABLE: &str = "models/hp_test/ui_table.mdl";

fn pin() {
    let (lo, hi) = (DVec3::new(-10.0, -10.0, -3.0), DVec3::new(10.0, 10.0, 17.0));
    pin_shape(
        CRATE,
        0,
        ModelShape { path: CRATE.to_owned(), render: TriMesh::convex_box(lo, hi, 0), ..ModelShape::default() },
    );
    let mut m = TriMesh::new();
    for (x, y) in [(-20.0, -20.0), (20.0, -20.0), (-20.0, 20.0), (20.0, 20.0)] {
        m.append(&TriMesh::convex_box(DVec3::new(x - 1.0, y - 1.0, 0.0), DVec3::new(x + 1.0, y + 1.0, 10.0), 0));
    }
    m.append(&TriMesh::convex_box(DVec3::new(-22.0, -22.0, 10.0), DVec3::new(22.0, 22.0, 12.0), 0));
    pin_shape(TABLE, 0, ModelShape { path: TABLE.to_owned(), render: m, ..ModelShape::default() });
}

fn sides(first: u32, lo: [f64; 3], hi: [f64; 3]) -> String {
    let [x0, y0, z0] = lo;
    let [x1, y1, z1] = hi;
    let planes = [
        format!("({x0} {y1} {z1}) ({x1} {y1} {z1}) ({x1} {y0} {z1})"),
        format!("({x0} {y0} {z0}) ({x1} {y0} {z0}) ({x1} {y1} {z0})"),
        format!("({x0} {y1} {z1}) ({x0} {y0} {z1}) ({x0} {y0} {z0})"),
        format!("({x1} {y1} {z0}) ({x1} {y0} {z0}) ({x1} {y0} {z1})"),
        format!("({x1} {y1} {z1}) ({x0} {y1} {z1}) ({x0} {y1} {z0})"),
        format!("({x1} {y0} {z0}) ({x0} {y0} {z0}) ({x0} {y0} {z1})"),
    ];
    planes
        .iter()
        .enumerate()
        .map(|(k, p)| {
            format!(
                "side {{ \"id\" \"{}\" \"plane\" \"{p}\" \"material\" \"DEV/DEV_MEASUREGENERIC01B\" \"uaxis\" \"[1 0 0 0] 0.25\" \"vaxis\" \"[0 -1 0 0] 0.25\" \"rotation\" \"0\" \"lightmapscale\" \"16\" \"smoothing_groups\" \"0\" }}",
                first + k as u32
            )
        })
        .collect::<Vec<_>>()
        .join(" ")
}

fn room() -> AppState {
    pin();
    let mut s = hp_app::testing::test_state();
    let text = format!(
        "world {{ \"id\" \"1\" \"classname\" \"worldspawn\" solid {{ \"id\" \"2\" {} }} solid {{ \"id\" \"3\" {} }} }}",
        sides(10, [-512.0, -512.0, -16.0], [512.0, 512.0, 0.0]),
        sides(20, [300.0, -512.0, 0.0], [316.0, 512.0, 256.0]),
    );
    s.doc = hp_editor::Document::from_map(hp_vmf::parse(&text).expect("vmf"));
    s
}

fn prop(s: &mut AppState, model: &str, at: [f64; 3]) -> u32 {
    let kv = [("model".to_owned(), model.to_owned())];
    s.doc.create_point_entity("prop_static", DVec3::from_array(at), &kv).expect("prop")
}

fn origin(s: &AppState, id: u32) -> DVec3 {
    s.doc.scene().entity(id).and_then(|e| e.entity.origin()).expect("origin")
}

fn near(a: f64, b: f64) -> bool {
    (a - b).abs() <= 0.02
}

fn placement(s: &AppState) -> Vec<Problem> {
    s.runtime.health.problems.iter().filter(|p| hp_editor::placement_fix::is_placement_code(p.code)).cloned().collect()
}

#[test]
fn the_check_lists_floating_sunk_and_overlapping_props_and_fix_all_is_one_step() {
    let mut s = room();
    let floating = prop(&mut s, CRATE, [0.0, 0.0, 60.0]);
    let sunk = prop(&mut s, CRATE, [100.0, 0.0, 0.0]);
    let c = prop(&mut s, CRATE, [-100.0, 0.0, 3.0]);
    let d = prop(&mut s, CRATE, [-95.0, 0.0, 3.0]);
    let fine = prop(&mut s, CRATE, [0.0, 150.0, 3.0]);
    super::place_scan::scan_now(&mut s);
    assert!(!placement(&s).iter().any(|p| p.code == "overlapping_objects"), "overlaps are opt in");
    s.settings.placement.overlap_check = true;
    super::place_scan::scan_now(&mut s);
    let found = placement(&s);
    let codes: Vec<(&str, u32)> = found.iter().map(|p| (p.code, p.objects[0].id())).collect();
    assert!(codes.contains(&("floating_object", floating)), "{codes:?}");
    assert!(codes.contains(&("sunk_object", sunk)), "{codes:?}");
    assert_eq!(found.iter().filter(|p| p.code == "overlapping_objects").count(), 1, "one problem per pair: {codes:?}");
    assert!(codes.contains(&("overlapping_objects", c)), "{codes:?}");
    assert!(!codes.iter().any(|(_, id)| *id == fine), "{codes:?}");
    let f = found.iter().find(|p| p.code == "floating_object").expect("floating");
    assert_eq!(f.severity, hp_editor::Severity::Warning);
    assert_eq!(f.arg(1), "57");
    assert!(f.message.contains("floats 57 units above worldspawn #2"), "{}", f.message);
    let r = ok(&mut s, "check_problems", json!({ "limit": 500 }));
    assert!(r["problems"].as_array().unwrap().iter().any(|p| p["code"] == "floating_object"), "{r}");
    let steps = s.doc.undo_labels().len();
    let fixable: Vec<Problem> = found.iter().filter(|p| p.fixable).cloned().collect();
    assert_eq!(s.doc.fix_problems(&fixable).expect("fix all"), 3);
    assert_eq!(s.doc.undo_labels().len(), steps + 1);
    assert!(near(origin(&s, floating).z, 3.0), "{}", origin(&s, floating));
    assert!(near(origin(&s, sunk).z, 3.0), "{}", origin(&s, sunk));
    assert!((origin(&s, c).x - -100.0).abs() > 14.9 || (origin(&s, c).y).abs() > 14.9, "{}", origin(&s, c));
    assert_eq!(origin(&s, d), DVec3::new(-95.0, 0.0, 3.0));
    super::place_scan::scan_now(&mut s);
    assert!(placement(&s).is_empty(), "{:?}", placement(&s));
    assert!(s.doc.undo());
    assert_eq!(origin(&s, floating).z, 60.0);
}

#[test]
fn stacked_props_fall_together_with_fix_all_and_alone_without_their_support() {
    let mut s = room();
    let low = prop(&mut s, CRATE, [0.0, 0.0, 40.0]);
    let high = prop(&mut s, CRATE, [0.0, 0.0, 80.0]);
    super::place_scan::scan_now(&mut s);
    let found = placement(&s);
    let hp = found.iter().find(|p| p.objects[0] == ObjRef::Entity(high)).cloned().expect("high");
    assert_eq!(hp.arg(1), "20");
    s.doc.fix_problems(&found).expect("all");
    assert!(near(origin(&s, low).z, 3.0) && near(origin(&s, high).z, 23.0), "{} {}", origin(&s, low), origin(&s, high));
    assert!(s.doc.undo());
    s.doc.fix_problems(&[hp]).expect("alone");
    assert!(near(origin(&s, high).z, 60.0), "{}", origin(&s, high));
}

#[test]
fn placement_tools_work_on_the_selection() {
    let mut s = room();
    let target = prop(&mut s, CRATE, [0.0, 0.0, 3.0]);
    let mover = prop(&mut s, CRATE, [100.0, 40.0, 3.0]);
    let r =
        place_ui::place(&mut s, &[ObjRef::Entity(mover)], Some(ObjRef::Entity(target)), UiPlace::OnTop, "Place on top")
            .expect("on top");
    assert_eq!(r.moved, 1);
    assert_eq!(r.gap.map(|g| g.abs() < 0.02), Some(true), "{r:?}");
    let o = origin(&s, mover);
    assert!(near(o.x, 0.0) && near(o.y, 0.0) && near(o.z, 23.0), "{o}");
    assert_eq!(s.doc.undo_label(), Some("Place on top"));
    assert!(s.doc.undo());

    let r = place_ui::place(
        &mut s,
        &[ObjRef::Entity(mover)],
        Some(ObjRef::Entity(target)),
        UiPlace::NextTo,
        "Place next to",
    )
    .expect("next to");
    assert_eq!(r.moved, 1);
    let o = origin(&s, mover);
    assert!(near(o.x, 20.0) && near(o.z, 3.0), "{o}");
    assert_eq!(r.overlaps, 0);

    let dir = DVec3::X;
    let r = place_ui::place(
        &mut s,
        &[ObjRef::Entity(mover)],
        None,
        UiPlace::Against { dir, away: true },
        "Place against wall",
    )
    .expect("against");
    assert_eq!(r.moved, 1);
    let o = origin(&s, mover);
    assert!(near(o.x, 290.0), "{o}");
    let yaw = s.doc.scene().entity(mover).and_then(|e| e.entity.angles()).map_or(0.0, |a| a.y);
    assert!(near(yaw.abs(), 180.0), "{yaw}");
    assert_eq!(place_ui::nearest_wall(&s, &[ObjRef::Entity(target)]), Some(DVec3::X));

    let up = prop(&mut s, CRATE, [-150.0, 0.0, 70.0]);
    let r = place_ui::settle(&mut s, Some(&[ObjRef::Entity(up)]), "Settle floating objects").expect("settle");
    assert_eq!((r.floating, r.sunk), (1, 0));
    assert!(near(origin(&s, up).z, 3.0));
    let again = place_ui::settle(&mut s, None, "Settle floating objects").expect("map");
    assert_eq!(again.moved(), 0);

    let table = prop(&mut s, TABLE, [-150.0, 200.0, 40.0]);
    let r = place_ui::place(&mut s, &[ObjRef::Entity(table)], None, UiPlace::Fit, "Fit to ground").expect("fit");
    assert_eq!(r.moved, 1);
    assert!(near(origin(&s, table).z, 0.0), "{}", origin(&s, table));
}

#[test]
fn real_dimensions_measure_size_gap_and_overlap() {
    let mut s = room();
    let a = prop(&mut s, CRATE, [0.0, 0.0, 10.0]);
    let b = prop(&mut s, TABLE, [100.0, 0.0, -2.0]);
    let d = place_ui::real_dims(&s, &[ObjRef::Entity(a)]);
    let u = d.single().expect("one unit");
    assert!(near(u.size.x, 20.0) && near(u.size.y, 20.0) && near(u.size.z, 20.0), "{:?}", u.size);
    assert_eq!(u.gap, Some(7.0));
    assert_eq!(u.footprint.len(), 4);
    assert_eq!(u.rect.len(), 4);
    assert!(!u.contacts.is_empty());
    assert!(d.summary().contains("20 × 20 × 20"), "{}", d.summary());
    let d = place_ui::real_dims(&s, &[ObjRef::Entity(b)]);
    let u = d.single().expect("table");
    assert!(near(u.size.x, 44.0) && near(u.size.z, 12.0), "{:?}", u.size);
    assert_eq!(u.gap, Some(-2.0));
    assert!(u.overlap >= 1.99, "{u:?}");
    let both = place_ui::real_dims(&s, &[ObjRef::Entity(a), ObjRef::Entity(b)]);
    assert_eq!(both.objects, 2);
    assert_eq!(both.units.len(), 2);
}

#[test]
fn shape_sheets_draw_the_selection_and_models() {
    let mut s = room();
    let a = prop(&mut s, CRATE, [0.0, 0.0, 3.0]);
    s.doc.set_selection(&[ObjRef::Entity(a)]);
    let sheet = place_ui::sheet_job_selection(&s, 480, 360).expect("job").run().expect("sheet");
    assert_eq!((sheet.image.width, sheet.image.height), (480, 360));
    assert!(sheet.caption.contains("size 20 x 20 x 20"), "{}", sheet.caption);
    let sheet = place_ui::sheet_job_model(&s, TABLE, 400, 300).run().expect("model");
    assert!(sheet.caption.contains("44 x 44 x 12"), "{}", sheet.caption);
    s.doc.set_selection(&[]);
    assert!(place_ui::sheet_job_selection(&s, 400, 300).is_err());
}

#[test]
fn mcp_edits_report_bad_placement_and_can_settle_new_props() {
    let mut s = room();
    let r = ok(
        &mut s,
        "create_entity",
        json!({ "classname": "prop_static", "origin": [0, 0, 60], "keyvalues": { "model": CRATE } }),
    );
    let note = r["placement_note"].as_str().expect("note");
    assert!(note.contains("floats 57 units") && note.contains("drop_to_surface"), "{note}");
    let r = ok(
        &mut s,
        "create_entity",
        json!({ "classname": "prop_static", "origin": [100, 0, 3], "keyvalues": { "model": CRATE } }),
    );
    assert!(r.get("placement_note").is_none(), "{r}");
    let resting = r["id"].as_u64().unwrap();
    prop(&mut s, CRATE, [130.0, 0.0, 3.0]);
    let r = ok(&mut s, "move_objects", json!({ "ids": [resting], "offset": [15, 0, 0] }));
    assert!(r["placement_note"].as_str().is_some_and(|n| n.contains("overlaps")), "{r}");
    let r = ok(&mut s, "create_entity", json!({ "classname": "info_target", "origin": [0, 300, 90] }));
    assert!(r.get("placement_note").is_none(), "point entities without a model are not checked: {r}");
    s.settings.placement.auto_settle_on_create = true;
    let steps = s.doc.undo_labels().len();
    let r = ok(
        &mut s,
        "create_entity",
        json!({ "classname": "prop_static", "origin": [0, -200, 90], "keyvalues": { "model": CRATE } }),
    );
    assert_eq!(r["auto_settled"], 1, "{r}");
    assert!(r.get("placement_note").is_none(), "{r}");
    let id = r["id"].as_u64().unwrap() as u32;
    assert!(near(origin(&s, id).z, 3.0), "{}", origin(&s, id));
    assert_eq!(s.doc.undo_labels().len(), steps + 1);
    let undo = call(&mut s, "undo", json!({}));
    assert!(!undo.is_error);
    assert!(s.doc.scene().entity(id).is_none(), "one undo removes the prop and its drop");
}

#[test]
fn ui_created_props_settle_in_the_same_undo_step() {
    let mut s = room();
    s.settings.placement.auto_settle_on_create = true;
    let id = prop(&mut s, CRATE, [0.0, 0.0, 90.0]);
    let created = s.doc.last_step_created().map(|(_, c)| c).unwrap_or_default();
    assert_eq!(place_ui::auto_settle_created(&mut s, &created, true), 1);
    assert!(near(origin(&s, id).z, 3.0));
    assert_eq!(s.doc.undo_label(), Some("Create entity"));
    assert!(s.doc.undo());
    assert!(s.doc.scene().entity(id).is_none());
}

#[test]
#[ignore]
fn timing_of_the_map_scan_preparation() {
    let Ok(path) = std::env::var("HP_PLACEMENT_MAP") else { return };
    let text = std::fs::read_to_string(&path).expect("map");
    let mut s = hp_app::testing::test_state();
    s.doc = hp_editor::Document::from_map(hp_vmf::parse(&text).expect("vmf"));
    for _ in 0..3 {
        let t = std::time::Instant::now();
        let job = super::place_scan::prepare_map_scan(&s);
        eprintln!("prepare {} ms, {} candidates", t.elapsed().as_millis(), job.candidates());
    }
    let t = std::time::Instant::now();
    let env = super::place_world::environment(
        &s,
        &Default::default(),
        &Default::default(),
        &super::place_world::EnvOptions { include_tools: false, include_points: false, region: None },
    );
    eprintln!("environment {} ms, {} objects", t.elapsed().as_millis(), env.len());
}

fn sky_state() -> AppState {
    pin();
    let lamp = "models/hp_test/hanging_lamp.mdl";
    let lo = DVec3::new(-6.0, -6.0, -20.0);
    let hi = DVec3::new(6.0, 6.0, 0.0);
    pin_shape(
        lamp,
        0,
        ModelShape { path: lamp.to_owned(), render: TriMesh::convex_box(lo, hi, 0), ..ModelShape::default() },
    );
    let hook = "models/hp_test/hook_box.mdl";
    let shape = ModelShape {
        path: hook.to_owned(),
        render: TriMesh::convex_box(DVec3::splat(-8.0), DVec3::splat(8.0), 0),
        attachments: vec![hp_shape::Attachment {
            name: "hook".to_owned(),
            pos: DVec3::new(0.0, 0.0, 8.0),
            forward: DVec3::X,
            up: DVec3::Z,
        }],
        ..ModelShape::default()
    };
    pin_shape(hook, 0, shape);
    let mut s = hp_app::testing::test_state();
    let room = [
        ([2000.0, -200.0, -16.0], [2400.0, 200.0, 0.0]),
        ([2000.0, -200.0, 400.0], [2400.0, 200.0, 416.0]),
        ([1984.0, -200.0, -16.0], [2000.0, 200.0, 416.0]),
        ([2400.0, -200.0, -16.0], [2416.0, 200.0, 416.0]),
        ([2000.0, -216.0, -16.0], [2400.0, -200.0, 416.0]),
        ([2000.0, 200.0, -16.0], [2400.0, 216.0, 416.0]),
    ];
    let mut solids =
        vec![format!("solid {{ \"id\" \"2\" {} }}", sides(10, [-512.0, -512.0, -16.0], [512.0, 512.0, 0.0]))];
    solids.push(format!("solid {{ \"id\" \"3\" {} }}", sides(20, [-512.0, -512.0, 300.0], [512.0, 512.0, 316.0])));
    for (k, (a, b)) in room.iter().enumerate() {
        solids.push(format!("solid {{ \"id\" \"{}\" {} }}", 100 + k, sides(200 + 10 * k as u32, *a, *b)));
    }
    let text = format!("world {{ \"id\" \"1\" \"classname\" \"worldspawn\" {} }}", solids.join(" "));
    s.doc = hp_editor::Document::from_map(hp_vmf::parse(&text).expect("vmf"));
    s.doc.create_point_entity("sky_camera", DVec3::new(2200.0, 0.0, 200.0), &[]).expect("sky camera");
    s
}

#[test]
fn skybox_props_and_hanging_props_are_left_to_mcp() {
    let mut s = sky_state();
    let sky = prop(&mut s, CRATE, [2200.0, 50.0, 100.0]);
    let floating = prop(&mut s, CRATE, [0.0, 0.0, 100.0]);
    let lamp = prop(&mut s, "models/hp_test/hanging_lamp.mdl", [100.0, 0.0, 200.0]);
    let hook = prop(&mut s, "models/hp_test/hook_box.mdl", [-100.0, 0.0, 200.0]);
    let near_ceiling = prop(&mut s, CRATE, [0.0, 100.0, 282.2]);
    super::place_scan::scan_now(&mut s);
    let ids: Vec<u32> = placement(&s).iter().map(|p| p.objects[0].id()).collect();
    assert_eq!(ids, vec![floating], "only the plain floating crate in the map is listed");
    let r = ok(&mut s, "find_floating_objects", json!({}));
    let listed: Vec<u64> = r["floating"].as_array().unwrap().iter().map(|v| v["id"].as_u64().unwrap()).collect();
    for id in [floating, lamp, hook, near_ceiling] {
        assert!(listed.contains(&u64::from(id)), "MCP still reports {id}: {r}");
    }
    let rep = place_ui::settle(&mut s, None, "Settle floating objects").expect("settle");
    assert_eq!(rep.floating, 1);
    assert!(near(origin(&s, floating).z, 3.0));
    assert_eq!(origin(&s, sky).z, 100.0, "skybox props stay");
    assert_eq!(origin(&s, lamp).z, 200.0, "hanging props stay");
}
