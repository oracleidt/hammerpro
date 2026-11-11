use glam::DVec3;
use hp_app::AppState;
use hp_shape::{ModelShape, TriMesh};
use serde_json::{Value, json};

use super::place_world::pin_shape;
use super::semantic_tests::{call, err, ok};

fn crate_shape(path: &str) -> ModelShape {
    let (lo, hi) = (DVec3::new(-10.0, -10.0, -3.0), DVec3::new(10.0, 10.0, 17.0));
    ModelShape {
        path: path.to_owned(),
        render: TriMesh::convex_box(lo, hi, 0),
        collision: vec![TriMesh::convex_box(lo + DVec3::splat(1.0), hi - DVec3::splat(1.0), 0)],
        ..ModelShape::default()
    }
}

fn wedge_shape(path: &str) -> ModelShape {
    let mut m = TriMesh::new();
    let p = |x: f64, y: f64, z: f64| DVec3::new(x, y, z);
    let a = [p(-16.0, -16.0, 0.0), p(16.0, -16.0, 0.0), p(16.0, -16.0, 32.0)];
    let b = a.map(|v| v + DVec3::new(0.0, 32.0, 0.0));
    m.add_polygon_facing(&a, DVec3::NEG_Y, 0);
    m.add_polygon_facing(&b, DVec3::Y, 0);
    m.add_polygon_facing(&[a[0], a[1], b[1], b[0]], DVec3::NEG_Z, 0);
    m.add_polygon_facing(&[a[1], a[2], b[2], b[1]], DVec3::X, 0);
    m.add_polygon_facing(&[a[0], a[2], b[2], b[0]], DVec3::new(-1.0, 0.0, 1.0), 0);
    m.closed = true;
    ModelShape { path: path.to_owned(), render: m, ..ModelShape::default() }
}

fn lamp_shape(path: &str) -> ModelShape {
    let mut m = TriMesh::convex_box(DVec3::new(-2.0, -6.0, -8.0), DVec3::new(0.0, 6.0, 8.0), 0);
    m.append(&TriMesh::convex_box(DVec3::new(0.0, -2.0, -2.0), DVec3::new(12.0, 2.0, 2.0), 0));
    ModelShape { path: path.to_owned(), render: m, ..ModelShape::default() }
}

fn prop(s: &mut AppState, model: &str, origin: [f64; 3]) -> u64 {
    let r = ok(
        s,
        "create_entity",
        json!({ "classname": "prop_static", "origin": origin, "keyvalues": { "model": model } }),
    );
    r["id"].as_u64().unwrap()
}

fn origin(s: &AppState, id: u64) -> DVec3 {
    s.doc.scene().entity(id as u32).and_then(|e| e.entity.origin()).unwrap()
}

fn near(a: f64, b: f64) -> bool {
    (a - b).abs() <= 0.011
}

fn disp_side(id: u32, lo: [f64; 3], hi: [f64; 3], hill: f64) -> String {
    let rows: Vec<String> = (0..5).map(|r| format!("\"row{r}\" \"{}\"", "0 0 1 ".repeat(5).trim())).collect();
    let dists: Vec<String> = (0..5)
        .map(|r| {
            let v: Vec<String> =
                (0..5).map(|c| if r == 2 && c == 2 { format!("{hill}") } else { "0".to_owned() }).collect();
            format!("\"row{r}\" \"{}\"", v.join(" "))
        })
        .collect();
    format!(
        "side {{ \"id\" \"{id}\" \"plane\" \"({x0} {y1} {z1}) ({x1} {y1} {z1}) ({x1} {y0} {z1})\" \"material\" \"NATURE/BLENDGRASSGRAVEL001A\" \"uaxis\" \"[1 0 0 0] 0.25\" \"vaxis\" \"[0 -1 0 0] 0.25\" \"rotation\" \"0\" \"lightmapscale\" \"16\" \"smoothing_groups\" \"0\" dispinfo {{ \"power\" \"2\" \"startposition\" \"[{x0} {y0} {z1}]\" \"elevation\" \"0\" \"subdiv\" \"0\" normals {{ {} }} distances {{ {} }} }} }}",
        rows.join(" "),
        dists.join(" "),
        x0 = lo[0],
        y0 = lo[1],
        x1 = hi[0],
        y1 = hi[1],
        z1 = hi[2],
    )
}

fn box_sides(first: u32, lo: [f64; 3], hi: [f64; 3], skip_top: bool) -> String {
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
        .skip(usize::from(skip_top))
        .map(|(k, p)| {
            format!(
                "side {{ \"id\" \"{}\" \"plane\" \"{p}\" \"material\" \"DEV/DEV_MEASUREGENERIC01B\" \"uaxis\" \"[1 0 0 0] 0.25\" \"vaxis\" \"[0 -1 0 0] 0.25\" \"rotation\" \"0\" \"lightmapscale\" \"16\" \"smoothing_groups\" \"0\" }}",
                first + k as u32
            )
        })
        .collect::<Vec<_>>()
        .join(" ")
}

fn with_displacements() -> AppState {
    let mut s = hp_app::testing::test_state();
    let text = format!(
        "world {{ \"id\" \"1\" \"classname\" \"worldspawn\" solid {{ \"id\" \"2\" {} }} solid {{ \"id\" \"20\" {} {} }} solid {{ \"id\" \"21\" {} {} }} }}",
        box_sides(1, [-256.0, -256.0, -16.0], [256.0, 256.0, 0.0], false),
        disp_side(100, [536.0, -64.0, -16.0], [664.0, 64.0, 0.0], 16.0),
        box_sides(101, [536.0, -64.0, -16.0], [664.0, 64.0, 0.0], true),
        disp_side(200, [-200.0, -64.0, 84.0], [-72.0, 64.0, 100.0], 8.0),
        box_sides(201, [-200.0, -64.0, 84.0], [-72.0, 64.0, 100.0], true),
    );
    s.doc = hp_editor::Document::from_map(hp_vmf::parse(&text).expect("vmf"));
    s
}

#[test]
fn model_shapes_are_measured_on_the_mesh() {
    pin_shape("models/hp_test/shape_crate.mdl", 0, crate_shape("models/hp_test/shape_crate.mdl"));
    let mut s = hp_app::testing::test_state();
    let r = ok(&mut s, "get_object_shape", json!({ "model": "models/hp_test/shape_crate.mdl", "detail": "full" }));
    let o = &r["objects"][0];
    assert_eq!(o["bounds"]["size"], json!([20.0, 20.0, 20.0]));
    assert_eq!(o["pivot"]["origin_above_bottom"], json!(3.0));
    assert_eq!(o["footprint"]["area"], json!(400.0));
    assert_eq!(o["collision"]["bounds"]["size"], json!([18.0, 18.0, 18.0]));
    assert_eq!(o["slices"].as_array().unwrap().len(), 8);
    assert!(o["height_map"]["top"].as_array().unwrap().len() >= 2);
    let r = ok(&mut s, "get_object_shape", json!({ "solid_ids": [2] }));
    let o = &r["objects"][0];
    assert_eq!(o["type"], "solid");
    assert_eq!(o["bounds"]["size"], json!([512.0, 512.0, 16.0]));
    assert_eq!(o["vertices"].as_array().unwrap().len(), 8);
    assert_eq!(o["faces"], json!(6));
    let r = ok(&mut s, "get_object_shape", json!({ "ids": [6], "include_tools": true }));
    assert_eq!(r["objects"][0]["type"], "brush_entity");
    let r = ok(&mut s, "get_object_shape", json!({ "ids": [5] }));
    assert_eq!(r["objects"][0]["type"], "point_entity");
    err(&mut s, "get_object_shape", json!({ "model": "models/hp_test/missing.mdl" }));
    err(&mut s, "get_object_shape", json!({ "model": "../x.mdl" }));
    err(&mut s, "get_object_shape", json!({ "solid_ids": [2], "origin": [0, 0, 0] }));
}

#[test]
fn props_drop_onto_brushes_props_and_displacements() {
    let m = "models/hp_test/drop_crate.mdl";
    pin_shape(m, 0, crate_shape(m));
    let mut s = with_displacements();
    let a = prop(&mut s, m, [0.0, 0.0, 40.0]);
    let r = ok(&mut s, "drop_to_surface", json!({ "ids": [a] }));
    assert_eq!(r["moved"], 1, "{r}");
    assert!(near(origin(&s, a).z, 3.0), "{}", origin(&s, a));
    let b = prop(&mut s, m, [4.0, 2.0, 90.0]);
    ok(&mut s, "drop_to_surface", json!({ "ids": [b] }));
    assert!(near(origin(&s, b).z, 23.0), "{}", origin(&s, b));
    let c = prop(&mut s, m, [600.0, 0.0, 70.0]);
    let r = ok(&mut s, "drop_to_surface", json!({ "ids": [c] }));
    assert_eq!(r["results"][0]["drop_contact"]["type"], "displacement", "{r}");
    assert!(near(origin(&s, c).z, 19.0), "{}", origin(&s, c));
    let sunk = prop(&mut s, m, [-100.0, -150.0, -2.0]);
    ok(&mut s, "drop_to_surface", json!({ "ids": [sunk] }));
    assert!(near(origin(&s, sunk).z, 3.0), "{}", origin(&s, sunk));
    let steps = s.doc.history_len();
    let far = prop(&mut s, m, [5000.0, 5000.0, 10.0]);
    let r = ok(&mut s, "drop_to_surface", json!({ "ids": [far] }));
    assert_eq!(r["moved"], 0);
    assert_eq!(r["stayed"].as_array().unwrap().len(), 1);
    assert_eq!(s.doc.history_len().0, steps.0 + 1);
    let check = ok(&mut s, "check_placement", json!({ "ids": [a, b, c] }));
    for o in check["objects"].as_array().unwrap() {
        assert_eq!(o["status"], "resting", "{o}");
        assert!(o["overlaps"].as_array().unwrap().is_empty(), "{o}");
    }
}

#[test]
fn brushes_and_displacements_move_too() {
    let mut s = with_displacements();
    let r = ok(
        &mut s,
        "create_brush",
        json!({ "min": [-32, -32, 40], "max": [32, 32, 56], "material": "dev/dev_measuregeneric01b" }),
    );
    let id = r["solid_ids"][0].as_u64().unwrap() as u32;
    let r = ok(&mut s, "drop_to_surface", json!({ "solid_ids": [id] }));
    assert_eq!(r["results"][0]["drop_contact"]["id"], 2, "{r}");
    let b = s.doc.scene().solid(id).unwrap().bounds;
    assert!(near(b.min.z, 0.0), "{b:?}");
    ok(&mut s, "drop_to_surface", json!({ "solid_ids": [21] }));
    let d = super::place_world::solid_obj(&s, 21, false).unwrap();
    let low = d.meshes.iter().map(|m| m.bounds().min.z).fold(f64::INFINITY, f64::min);
    assert!(near(low, 0.0), "{low}");
    assert_eq!(d.kind, "displacement");
    let r = ok(&mut s, "check_placement", json!({ "solid_ids": [21] }));
    assert_eq!(r["objects"][0]["status"], "resting", "{r}");
    let r = ok(&mut s, "place_object", json!({ "solid_ids": [id], "mode": "next_to", "target": 21, "side": "+y" }));
    let b = s.doc.scene().solid(id).unwrap().bounds;
    assert!(near(b.min.y, 64.0), "{b:?} {r}");
    assert!(near(b.min.z, 0.0), "{b:?}");
}

#[test]
fn place_modes_touch_exactly() {
    let m = "models/hp_test/place_crate.mdl";
    let lamp = "models/hp_test/place_lamp.mdl";
    pin_shape(m, 0, crate_shape(m));
    pin_shape(lamp, 0, lamp_shape(lamp));
    let mut s = hp_app::testing::test_state();
    let r = ok(&mut s, "place_object", json!({ "model": m, "position": [0, 0, 64] }));
    assert_eq!(r["created"], true, "{r}");
    let a = r["id"].as_u64().unwrap();
    assert!(near(origin(&s, a).z, 3.0));
    assert_eq!(r["check"]["ok"], true, "{r}");
    let r = ok(
        &mut s,
        "place_object",
        json!({ "model": m, "mode": "next_to", "target": a, "side": "-x", "position": [0, 0, 0] }),
    );
    let b = r["id"].as_u64().unwrap();
    assert!(near(origin(&s, b).x, -20.0) && near(origin(&s, b).z, 3.0), "{} {r}", origin(&s, b));
    let r = ok(&mut s, "place_object", json!({ "model": m, "mode": "on", "target": a, "position": [0, 0, 0] }));
    let c = r["id"].as_u64().unwrap();
    assert!(near(origin(&s, c).z, 23.0), "{}", origin(&s, c));
    let r = ok(
        &mut s,
        "place_object",
        json!({ "model": lamp, "mode": "against", "position": [0, 200, 64], "direction": [0, 1, 0], "facing": "away" }),
    );
    let l = r["id"].as_u64().unwrap();
    let max_y = r["bounds"]["max"][1].as_f64().unwrap();
    assert!(near(max_y, 256.0), "{r}");
    assert!(near(origin(&s, l).y, 254.0), "{}", origin(&s, l));
    let angles = s.doc.scene().entity(l as u32).unwrap().entity.angles().unwrap();
    assert!(near(angles.y.rem_euclid(360.0), 270.0), "{angles}");
    let r = ok(&mut s, "place_object", json!({ "ids": [a], "mode": "at", "position": [100, 0, 50], "dry_run": true }));
    assert_eq!(r["dry_run"], true);
    assert!(near(origin(&s, a).x, 0.0));
    let r = ok(&mut s, "place_object", json!({ "ids": [a], "position": [100, 0, 50], "gap": 2 }));
    assert!(near(origin(&s, a).z, 5.0), "{r}");
    err(&mut s, "place_object", json!({ "ids": [a], "mode": "on" }));
    err(&mut s, "place_object", json!({ "ids": [a], "mode": "against" }));
    err(&mut s, "place_object", json!({ "model": m }));
    err(&mut s, "place_object", json!({ "model": m, "ids": [a], "position": [0, 0, 0] }));
    err(&mut s, "place_object", json!({ "ids": [a], "mode": "on", "target": a }));
}

#[test]
fn overlaps_floating_and_sunk_are_found_and_fixed() {
    let m = "models/hp_test/float_crate.mdl";
    pin_shape(m, 0, crate_shape(m));
    let mut s = hp_app::testing::test_state();
    let a = prop(&mut s, m, [0.0, 0.0, 3.0]);
    let b = prop(&mut s, m, [15.0, 0.0, 3.0]);
    let r = ok(&mut s, "check_placement", json!({ "ids": [a] }));
    let o = &r["objects"][0];
    assert_eq!(o["status"], "resting", "{o}");
    let ov = o["overlaps"].as_array().unwrap();
    assert_eq!(ov.len(), 1, "{o}");
    assert_eq!(ov[0]["id"], json!(b));
    assert_eq!(ov[0]["depth"], json!(5.0));
    let f = prop(&mut s, m, [-100.0, 0.0, 30.0]);
    let k = prop(&mut s, m, [100.0, 100.0, 0.0]);
    let hang = prop(&mut s, m, [0.0, 246.0, 60.0]);
    let r = ok(&mut s, "find_floating_objects", json!({}));
    assert_eq!(r["floating_total"], 1, "{r}");
    assert_eq!(r["floating"][0]["id"], json!(f));
    assert_eq!(r["floating"][0]["gap"], json!(27.0));
    assert_eq!(r["sunk_total"], 1, "{r}");
    assert_eq!(r["sunk"][0]["depth"], json!(3.0));
    assert_eq!(r["attached"], 1, "{r}");
    let before = s.doc.history_len();
    let r = ok(&mut s, "find_floating_objects", json!({ "fix": true, "fix_sunk": true }));
    assert_eq!(r["fixed_total"], 2, "{r}");
    assert_eq!(s.doc.history_len().0, before.0 + 1);
    assert!(near(origin(&s, f).z, 3.0) && near(origin(&s, k).z, 3.0));
    assert!(near(origin(&s, hang).z, 60.0));
    let r = ok(&mut s, "find_floating_objects", json!({ "min": [-150, -50, -50], "max": [-50, 50, 50] }));
    assert_eq!(r["checked"], 1, "{r}");
}

#[test]
fn rays_boxes_and_drawings_use_real_meshes() {
    let w = "models/hp_test/ray_wedge.mdl";
    pin_shape(w, 0, wedge_shape(w));
    let mut s = hp_app::testing::test_state();
    let id = prop(&mut s, w, [0.0, 0.0, 0.0]);
    let r = ok(&mut s, "trace_ray", json!({ "origin": [-12, 0, 200], "direction": [0, 0, -1] }));
    assert_eq!(r["object"]["id"], json!(id), "{r}");
    assert!((r["point"][2].as_f64().unwrap() - 4.0).abs() < 0.01, "{r}");
    let corner = json!({ "min": [-16, -4, 24], "max": [-8, 4, 31] });
    let mut args = corner.clone();
    args["exact"] = json!(true);
    let r = ok(&mut s, "query_box", args);
    assert!(!r["entities"].as_array().unwrap().iter().any(|e| e["id"] == json!(id)), "{r}");
    let r = ok(&mut s, "get_object_shape", json!({ "ids": [id] }));
    assert_eq!(r["objects"][0]["bounds"]["size"], json!([32.0, 32.0, 32.0]));
    let png = call(&mut s, "render_shape", json!({ "ids": [id], "width": 640, "height": 480 }));
    assert!(!png.is_error, "{}", png.first_text());
    assert!(matches!(png.content[0], hp_mcp::protocol::Content::Image { .. }));
    let png = call(
        &mut s,
        "render_shape",
        json!({ "min": [-300, -300, -20], "max": [300, 300, 140], "views": ["top", "iso"] }),
    );
    assert!(!png.is_error, "{}", png.first_text());
    let png = call(&mut s, "render_shape", json!({ "model": w, "mesh": "both" }));
    assert!(!png.is_error, "{}", png.first_text());
    err(&mut s, "render_shape", json!({ "ids": [id], "views": ["sideways"] }));
    err(&mut s, "render_shape", json!({ "min": [0, 0, 0] }));
}

#[test]
fn groups_move_as_one_unit() {
    let m = "models/hp_test/group_crate.mdl";
    pin_shape(m, 0, crate_shape(m));
    let mut s = hp_app::testing::test_state();
    let a = prop(&mut s, m, [0.0, 0.0, 50.0]);
    let b = prop(&mut s, m, [0.0, 0.0, 70.0]);
    let r = ok(&mut s, "group_objects", json!({ "ids": [a, b] }));
    let g = r["group"].as_u64().expect("group id");
    let r = ok(&mut s, "get_object_shape", json!({ "group_ids": [g] }));
    assert_eq!(r["objects"][0]["type"], "group");
    assert_eq!(r["objects"][0]["bounds"]["size"], json!([20.0, 20.0, 40.0]));
    ok(&mut s, "drop_to_surface", json!({ "ids": [b] }));
    assert!(near(origin(&s, a).z, 3.0) && near(origin(&s, b).z, 23.0), "{} {}", origin(&s, a), origin(&s, b));
    let v: Value = ok(&mut s, "check_placement", json!({ "group_ids": [g] }));
    assert_eq!(v["objects"][0]["status"], "resting", "{v}");
}

fn table_shape(path: &str) -> ModelShape {
    let mut m = TriMesh::new();
    for (x, y) in [(-20.0, -20.0), (20.0, -20.0), (-20.0, 20.0), (20.0, 20.0)] {
        m.append(&TriMesh::convex_box(DVec3::new(x - 1.0, y - 1.0, 0.0), DVec3::new(x + 1.0, y + 1.0, 10.0), 0));
    }
    m.append(&TriMesh::convex_box(DVec3::new(-22.0, -22.0, 10.0), DVec3::new(22.0, 22.0, 12.0), 0));
    ModelShape { path: path.to_owned(), render: m, ..ModelShape::default() }
}

fn slope_shape(path: &str) -> ModelShape {
    let mut m = TriMesh::new();
    let p = |x: f64, y: f64, z: f64| DVec3::new(x, y, z);
    let top = |x: f64| 20.0 + 0.2 * x;
    let a =
        [p(-100.0, -100.0, 0.0), p(100.0, -100.0, 0.0), p(100.0, -100.0, top(100.0)), p(-100.0, -100.0, top(-100.0))];
    let b = a.map(|v| v + DVec3::new(0.0, 200.0, 0.0));
    m.add_polygon_facing(&a, DVec3::NEG_Y, 0);
    m.add_polygon_facing(&b, DVec3::Y, 0);
    m.add_polygon_facing(&[a[0], a[1], b[1], b[0]], DVec3::NEG_Z, 0);
    m.add_polygon_facing(&[a[3], a[2], b[2], b[3]], DVec3::new(-0.2, 0.0, 1.0), 0);
    m.add_polygon_facing(&[a[1], a[2], b[2], b[1]], DVec3::X, 0);
    m.add_polygon_facing(&[a[0], a[3], b[3], b[0]], DVec3::NEG_X, 0);
    m.closed = true;
    ModelShape { path: path.to_owned(), render: m, ..ModelShape::default() }
}

#[test]
fn editor_move_to_floor_uses_real_meshes() {
    let m = "models/hp_test/editor_crate.mdl";
    pin_shape(m, 0, crate_shape(m));
    let mut s = with_displacements();
    let a = prop(&mut s, m, [0.0, 0.0, 60.0]);
    let c = prop(&mut s, m, [600.0, 0.0, 70.0]);
    let objs = [hp_editor::ObjRef::Entity(a as u32), hp_editor::ObjRef::Entity(c as u32)];
    let n = super::mesh_drop(&mut s, &objs, DVec3::NEG_Z, false, "Move to floor").expect("drop");
    assert_eq!(n, 2);
    assert!(near(origin(&s, a).z, 3.0) && near(origin(&s, c).z, 19.0), "{} {}", origin(&s, a), origin(&s, c));
    assert_eq!(s.doc.undo_label(), Some("Move to floor"));
    assert_eq!(s.doc.scene().entity(a as u32).unwrap().entity.get("origin"), Some("0 0 3"));
    let up = super::mesh_drop(&mut s, &[hp_editor::ObjRef::Solid(21)], DVec3::NEG_Z, false, "Physics placement")
        .expect("disp");
    assert_eq!(up, 1);
    let low = super::place_world::solid_obj(&s, 21, false)
        .unwrap()
        .meshes
        .iter()
        .map(|x| x.bounds().min.z)
        .fold(f64::INFINITY, f64::min);
    assert!(near(low, 0.0), "{low}");
    assert_eq!(
        super::mesh_drop(&mut s, &[hp_editor::ObjRef::Entity(a as u32)], DVec3::NEG_Z, false, "Move to floor")
            .expect("again"),
        0
    );
}

#[test]
fn fit_supports_puts_every_leg_down_or_says_why() {
    let t = "models/hp_test/fit_table.mdl";
    let g = "models/hp_test/fit_slope.mdl";
    let k = "models/hp_test/fit_block.mdl";
    pin_shape(t, 0, table_shape(t));
    pin_shape(g, 0, slope_shape(g));
    let block = ModelShape {
        path: k.to_owned(),
        render: TriMesh::convex_box(DVec3::new(-3.0, -3.0, 0.0), DVec3::splat(3.0) + DVec3::new(0.0, 0.0, 9.0), 0),
        ..ModelShape::default()
    };
    pin_shape(k, 0, block);
    let mut s = hp_app::testing::test_state();
    prop(&mut s, g, [0.0, -150.0, 0.0]);
    let r = ok(&mut s, "place_object", json!({ "model": t, "position": [0, -150, 80], "fit": "supports" }));
    let gaps = r["support_gaps"].as_array().expect("gaps");
    assert_eq!(gaps.len(), 4, "{r}");
    assert!(gaps.iter().all(|x| x.as_f64().unwrap() <= 0.1), "{r}");
    assert!(r.get("fit_note").is_none(), "{r}");
    let plain = ok(&mut s, "place_object", json!({ "model": t, "position": [40, -110, 80], "dry_run": true }));
    assert!(plain.get("support_gaps").is_none());
    prop(&mut s, k, [0.0, 100.0, 0.0]);
    let r = ok(&mut s, "place_object", json!({ "model": t, "position": [0, 100, 60], "fit": "supports" }));
    assert!(r["fit"]["max_gap"].as_f64().unwrap() > 1.5, "{r}");
    assert!(r["fit_note"].as_str().unwrap().contains("cannot all touch"), "{r}");
}
