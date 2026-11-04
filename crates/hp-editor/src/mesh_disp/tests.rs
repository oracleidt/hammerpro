use glam::DVec3;

use super::*;
use crate::{Document, FaceRef, ObjRef, Owner, SelectOp};

fn up_tri(a: DVec3, b: DVec3, c: DVec3) -> Tri {
    if (b - a).cross(c - a).z >= 0.0 { [a, b, c] } else { [a, c, b] }
}

fn down_tri(a: DVec3, b: DVec3, c: DVec3) -> Tri {
    if (b - a).cross(c - a).z <= 0.0 { [a, b, c] } else { [a, c, b] }
}

fn quad(x: f64, y: f64, w: f64, d: f64, z: f64) -> Vec<Tri> {
    let p = |u: f64, v: f64| DVec3::new(x + u, y + v, z);
    vec![up_tri(p(0.0, 0.0), p(w, 0.0), p(w, d)), up_tri(p(0.0, 0.0), p(w, d), p(0.0, d))]
}

fn pyramid(size: f64, height: f64) -> Vec<Tri> {
    let apex = DVec3::new(size * 0.5, size * 0.5, height);
    let c = [DVec3::ZERO, DVec3::new(size, 0.0, 0.0), DVec3::new(size, size, 0.0), DVec3::new(0.0, size, 0.0)];
    let mut tris = Vec::new();
    for k in 0..4 {
        tris.push(up_tri(c[k], c[(k + 1) % 4], apex));
    }
    tris.push(down_tri(c[0], c[1], c[2]));
    tris.push(down_tri(c[0], c[2], c[3]));
    tris
}

fn keep() -> MeshDispOptions {
    MeshDispOptions { origin: Origin::Keep, power: PowerChoice::Fixed(2), ..MeshDispOptions::default() }
}

fn pyramid_height(p: DVec3, size: f64, height: f64) -> f64 {
    let half = size * 0.5;
    let d = (p.x - half).abs().max((p.y - half).abs());
    height * (1.0 - d / half)
}

fn disp_positions(doc: &Document, f: FaceRef) -> Vec<DVec3> {
    doc.scene().solid(f.solid).and_then(|n| n.disp_mesh(f.side)).map(|m| m.positions.clone()).unwrap_or_default()
}

#[test]
fn a_pyramid_gives_the_right_heights() {
    let tris = pyramid(256.0, 100.0);
    let opts = keep();
    let plan = plan(&tris, &opts).expect("plan");
    assert_eq!((plan.tile_count(), plan.power), (1, 2));
    assert_eq!(plan.plane, 0.0);
    assert_eq!(plan.warnings.overhang_nodes, 0, "a closed mesh has no overhang");
    let mut doc = Document::new_empty();
    let r = doc.create_mesh_displacements(&plan, &opts).expect("create");
    assert_eq!((r.tiles, r.faces.len()), (1, 1));
    let positions = disp_positions(&doc, r.faces[0]);
    assert_eq!(positions.len(), 25);
    for p in &positions {
        assert!((p.z - pyramid_height(*p, 256.0, 100.0)).abs() < 1e-6, "{p}");
    }
    assert!(positions.iter().any(|p| (p.z - 100.0).abs() < 1e-6));
    let step = doc.undo_label().map(str::to_owned);
    assert_eq!(step.as_deref(), Some("Mesh to displacement"));
    assert!(doc.undo());
    assert!(doc.disp_faces().is_empty());
    assert!(doc.redo());
    assert_eq!(doc.disp_faces().len(), 1);
    assert!(disp_positions(&doc, r.faces[0]).iter().any(|p| (p.z - 100.0).abs() < 1e-6));
}

#[test]
fn a_flat_quad_gives_flat_tiles() {
    let tris = quad(0.0, 0.0, 512.0, 512.0, 40.0);
    let opts = keep();
    let plan = plan(&tris, &opts).expect("plan");
    assert_eq!((plan.nx, plan.ny, plan.tile_count(), plan.skipped), (2, 2, 4, 0));
    assert_eq!(plan.plane, 40.0);
    let mut doc = Document::new_empty();
    let r = doc.create_mesh_displacements(&plan, &opts).expect("create");
    assert_eq!(r.faces.len(), 4);
    for f in &r.faces {
        let d = doc.face_side(*f).and_then(|s| s.disp.as_deref()).expect("disp");
        assert!(d.distances.iter().all(|x| x.abs() < 1e-9));
        assert!(disp_positions(&doc, *f).iter().all(|p| (p.z - 40.0).abs() < 1e-9));
    }
}

#[test]
fn sewn_seams_are_equal() {
    let tris = pyramid(512.0, 200.0);
    let opts = MeshDispOptions { smooth: 2, ..keep() };
    let plan = plan(&tris, &opts).expect("plan");
    let mut doc = Document::new_empty();
    let r = doc.create_mesh_displacements(&plan, &opts).expect("create");
    assert_eq!(r.faces.len(), 4);
    let all: Vec<Vec<DVec3>> = r.faces.iter().map(|f| disp_positions(&doc, *f)).collect();
    let mut shared = 0;
    for a in 0..all.len() {
        for b in a + 1..all.len() {
            for p in &all[a] {
                for q in &all[b] {
                    if (p.x - q.x).abs() < 1e-6 && (p.y - q.y).abs() < 1e-6 {
                        assert!((p.z - q.z).abs() < 1e-6, "{p} {q}");
                        shared += 1;
                    }
                }
            }
        }
    }
    assert!(shared >= 4 * 5 - 4, "{shared}");
}

#[test]
fn misses_are_skipped_and_filled() {
    let mut tris = quad(0.0, 0.0, 256.0, 256.0, 10.0);
    tris.extend(quad(512.0, 512.0, 256.0, 256.0, 30.0));
    let plan1 = plan(&tris, &keep()).expect("plan");
    assert_eq!(plan1.tile_count(), 2);
    assert!(plan1.warnings.filled_nodes == 0);
    let wide = MeshDispOptions { margin: 1, ..keep() };
    let plan2 = plan(&tris, &wide).expect("margin");
    assert!(plan2.tile_count() > 2);
    assert!(plan2.warnings.filled_nodes > 0);
    let base = MeshDispOptions { margin: 1, fill: Fill::Base, ..keep() };
    let plan3 = plan(&tris, &base).expect("base");
    assert_eq!(plan3.plane, 10.0);
    assert!(plan3.heights.iter().filter(|h| h.is_finite()).all(|h| (10.0..=30.0).contains(h)));
}

#[test]
fn overhangs_are_reported_and_the_top_is_kept() {
    let mut tris = quad(0.0, 0.0, 256.0, 256.0, 0.0);
    tris.extend(quad(0.0, 0.0, 256.0, 256.0, 300.0));
    let p = plan(&tris, &keep()).expect("plan");
    assert!(p.warnings.overhang_nodes > 0);
    assert!(p.heights.iter().filter(|h| h.is_finite()).all(|h| (*h - 300.0).abs() < 1e-9));
    let tris = pyramid(256.0, 50.0);
    assert_eq!(plan(&tris, &keep()).expect("closed").warnings.overhang_nodes, 0);
}

#[test]
fn first_hit_ignores_back_faces() {
    let mut tris = quad(0.0, 0.0, 256.0, 256.0, 0.0);
    tris.extend(quad(0.0, 0.0, 256.0, 256.0, 90.0).into_iter().map(|t| [t[0], t[2], t[1]]));
    let top = plan(&tris, &keep()).expect("top");
    assert!(top.heights.iter().filter(|h| h.is_finite()).all(|h| (*h - 90.0).abs() < 1e-9));
    let first = plan(&tris, &MeshDispOptions { pick: Pick::First, ..keep() }).expect("first");
    assert!(first.heights.iter().filter(|h| h.is_finite()).all(|h| h.abs() < 1e-9));
}

#[test]
fn auto_power_follows_the_mesh_detail() {
    assert_eq!(auto_power(256.0, 1000.0), 2);
    assert_eq!(auto_power(256.0, 40.0), 3);
    assert_eq!(auto_power(256.0, 10.0), 4);
    let tris = quad(0.0, 0.0, 256.0, 256.0, 0.0);
    let auto = MeshDispOptions { power: PowerChoice::Auto, ..keep() };
    assert_eq!(plan(&tris, &auto).expect("auto").power, 2);
}

#[test]
fn the_axis_can_be_chosen_or_found() {
    let wall: Vec<Tri> =
        quad(0.0, 0.0, 256.0, 256.0, 0.0).into_iter().map(|t| t.map(|p| DVec3::new(p.z + 64.0, p.x, p.y))).collect();
    let best = MeshDispOptions { axis: AxisChoice::Best, ..keep() };
    let p = plan(&wall, &best).expect("best");
    assert_eq!(p.frame.axis(), 0);
    let mut doc = Document::new_empty();
    let r = doc.create_mesh_displacements(&p, &best).expect("create");
    let positions = disp_positions(&doc, r.faces[0]);
    assert!(positions.iter().all(|q| (q.x - 64.0).abs() < 1e-6), "{:?}", positions.first());
    let z = MeshDispOptions { axis: AxisChoice::Z, ..keep() };
    assert!(plan(&wall, &z).is_err() || plan(&wall, &z).is_ok_and(|p| p.tile_count() <= 1));
}

#[test]
fn origin_modes_place_the_mesh() {
    let tris = quad(1000.0, 1000.0, 256.0, 256.0, 500.0);
    let center = MeshDispOptions { origin: Origin::Center, position: DVec3::new(0.0, 0.0, 8.0), ..keep() };
    let p = plan(&tris, &center).expect("center");
    assert_eq!((p.lo_u, p.lo_v, p.plane), (-256.0, -256.0, 8.0));
    assert_eq!(p.tile_count(), 4);
    let corner = MeshDispOptions { origin: Origin::MinCorner, position: DVec3::new(100.0, 0.0, 0.0), ..keep() };
    let p = plan(&tris, &corner).expect("corner");
    assert_eq!((p.lo_u, p.plane), (0.0, 0.0));
    let scaled = MeshDispOptions { scale: 2.0, ..keep() };
    let p = plan(&tris, &scaled).expect("scaled");
    assert_eq!(p.plane, 1000.0);
}

#[test]
fn skirts_and_a_bottom_close_the_shape() {
    let tris = pyramid(256.0, 100.0);
    let opts = MeshDispOptions { skirt: true, bottom: true, group: false, thickness: 32.0, ..keep() };
    let p = plan(&tris, &opts).expect("plan");
    assert_eq!((p.plane, p.base), (0.0, -32.0));
    let mut doc = Document::new_empty();
    let r = doc.create_mesh_displacements(&p, &opts).expect("create");
    assert_eq!((r.tiles, r.skirts, r.bottoms, r.solids.len(), r.faces.len()), (1, 4, 1, 6, 5));
    let tile = disp_positions(&doc, r.faces[0]);
    for f in &r.faces[1..] {
        let skirt = disp_positions(&doc, *f);
        let low = skirt.iter().map(|q| q.z).fold(f64::INFINITY, f64::min);
        assert!((low + 32.0).abs() < 1e-6, "{low}");
    }
    let edge: Vec<&DVec3> =
        tile.iter().filter(|q| q.x < 1e-6 || q.y < 1e-6 || q.x > 256.0 - 1e-6 || q.y > 256.0 - 1e-6).collect();
    assert_eq!(edge.len(), 16);
    let skirt_points: Vec<DVec3> = r.faces[1..].iter().flat_map(|f| disp_positions(&doc, *f)).collect();
    for q in edge {
        assert!(skirt_points.iter().any(|s| s.distance(*q) < 1e-6), "{q} is not on a skirt");
    }
    assert!(doc.undo());
    assert!(doc.disp_faces().is_empty());
}

#[test]
fn bad_options_and_empty_meshes_are_refused() {
    let tris = quad(0.0, 0.0, 256.0, 256.0, 0.0);
    assert!(matches!(plan(&[], &keep()), Err(MeshDispError::Empty)));
    assert!(plan(&tris, &MeshDispOptions { tile_size: 4.0, ..keep() }).is_err());
    assert!(plan(&tris, &MeshDispOptions { scale: 0.0, ..keep() }).is_err());
    assert!(plan(&tris, &MeshDispOptions { smooth: 1000, ..keep() }).is_err());
    assert!(plan(&tris, &MeshDispOptions { material: String::new(), ..keep() }).is_err());
    assert!(matches!(
        plan(&tris, &MeshDispOptions { base_height: Some(5.0), ..keep() }),
        Err(MeshDispError::BadBase(..))
    ));
    let huge = quad(0.0, 0.0, 100_000.0, 100_000.0, 0.0);
    assert!(matches!(
        plan(&huge, &MeshDispOptions { tile_size: 16.0, ..keep() }),
        Err(MeshDispError::TooManyTiles(..))
    ));
    assert_eq!(AxisChoice::parse("BEST"), Some(AxisChoice::Best));
    assert_eq!(PowerChoice::parse("auto"), Some(PowerChoice::Auto));
    assert_eq!(PowerChoice::parse("5"), None);
    assert_eq!(Origin::parse("min_corner"), Some(Origin::MinCorner));
}

fn flat_floor(doc: &mut Document, size: f64, z: f64) -> Vec<FaceRef> {
    let bounds = hp_geom::Aabb::new(DVec3::new(0.0, 0.0, z - 16.0), DVec3::new(size, size, z));
    let params = hp_geom::PrimitiveParams { bounds, ..Default::default() };
    let ids = doc.create_primitive(&params, Owner::World).expect("floor");
    let n = doc.scene().solid(ids[0]).expect("solid");
    let top =
        n.geom.faces.iter().find(|f| f.plane.normal.z > 0.99).map(|f| FaceRef::new(ids[0], f.side_index)).expect("top");
    doc.create_displacements(&[top], 3).expect("disp");
    vec![top]
}

#[test]
fn conforming_projects_existing_displacements() {
    let mut doc = Document::new_empty();
    let faces = flat_floor(&mut doc, 256.0, 0.0);
    let tris = pyramid(256.0, 100.0);
    let opts = ConformOptions::default();
    let r = doc.conform_displacements(&faces, &tris, &opts).expect("conform");
    assert_eq!((r.faces, r.vertices, r.missed), (1, 81, 0));
    for p in disp_positions(&doc, faces[0]) {
        assert!((p.z - pyramid_height(p, 256.0, 100.0)).abs() < 1e-6, "{p}");
    }
    assert_eq!(doc.undo_label(), Some("Conform displacements to mesh"));
    assert!(doc.undo());
    assert!(disp_positions(&doc, faces[0]).iter().all(|p| p.z.abs() < 1e-9));
    let half = ConformOptions { strength: 0.5, offset: 10.0, ..opts.clone() };
    doc.conform_displacements(&faces, &tris, &half).expect("half");
    let apex = disp_positions(&doc, faces[0]).iter().map(|p| p.z).fold(f64::MIN, f64::max);
    assert!((apex - 55.0).abs() < 1e-6, "{apex}");
    let off = [DVec3::new(5000.0, 5000.0, 0.0)];
    let far: Vec<Tri> = vec![[off[0], off[0] + DVec3::X * 10.0, off[0] + DVec3::Y * 10.0]];
    assert!(doc.conform_displacements(&faces, &far, &opts).is_err());
    assert!(doc.conform_displacements(&faces, &tris, &ConformOptions { strength: 2.0, ..opts.clone() }).is_err());
    assert!(doc.conform_displacements(&[], &tris, &opts).is_err());
}

#[test]
fn conforming_a_vertical_wall_uses_the_wall_axis() {
    let mut doc = Document::new_empty();
    let bounds = hp_geom::Aabb::new(DVec3::new(0.0, 0.0, 0.0), DVec3::new(16.0, 256.0, 256.0));
    let params = hp_geom::PrimitiveParams { bounds, ..Default::default() };
    let ids = doc.create_primitive(&params, Owner::World).expect("wall");
    let n = doc.scene().solid(ids[0]).expect("solid");
    let side = n.geom.faces.iter().find(|f| f.plane.normal.x > 0.99).map(|f| f.side_index).expect("side");
    let face = FaceRef::new(ids[0], side);
    doc.create_displacements(&[face], 3).expect("disp");
    let wall: Vec<Tri> =
        pyramid(256.0, 80.0).into_iter().map(|t| t.map(|p| DVec3::new(p.z + 16.0, p.x, p.y))).collect();
    let opts = ConformOptions { axis: AxisChoice::Best, ..ConformOptions::default() };
    let r = doc.conform_displacements(&[face], &wall, &opts).expect("conform");
    assert_eq!(r.missed, 0);
    let apex = disp_positions(&doc, face).iter().map(|p| p.x).fold(f64::MIN, f64::max);
    assert!((apex - 96.0).abs() < 1e-6, "{apex}");
}

#[test]
fn tiling_a_selection_makes_flat_tiles_and_removes_the_source() {
    let mut doc = Document::new_empty();
    let bounds = hp_geom::Aabb::new(DVec3::new(0.0, 0.0, -16.0), DVec3::new(512.0, 300.0, 0.0));
    let params = hp_geom::PrimitiveParams { bounds, ..Default::default() };
    let ids = doc.create_primitive(&params, Owner::World).expect("slab");
    doc.select(&[ObjRef::Solid(ids[0])], SelectOp::Replace);
    let before = doc.history_len();
    let r = doc.tile_selection(&TileSelectionOptions::default()).expect("tile");
    assert_eq!((r.tiles, r.sources, r.removed, r.faces.len()), (4, 1, 1, 4));
    assert!(doc.scene().solid(ids[0]).is_none());
    assert_eq!(doc.history_len().0, before.0 + 1);
    for f in &r.faces {
        let d = doc.face_side(*f).and_then(|s| s.disp.as_deref()).expect("disp");
        assert_eq!(d.power, 3);
        assert!(disp_positions(&doc, *f).iter().all(|p| p.z.abs() < 1e-9));
    }
    assert_eq!(doc.undo_label(), Some("Tile displacements from selection"));
    assert!(doc.undo());
    assert!(doc.scene().solid(ids[0]).is_some());
    assert!(doc.disp_faces().is_empty());
    doc.select(&[ObjRef::Solid(ids[0])], SelectOp::Replace);
    let keep_source = TileSelectionOptions { delete_source: false, axis: AxisChoice::X, ..Default::default() };
    let r = doc.tile_selection(&keep_source).expect("walls");
    assert_eq!((r.tiles, r.removed), (2, 0));
    assert!(doc.tile_selection(&TileSelectionOptions { tile_size: 1.0, ..Default::default() }).is_err());
}

fn hill(n: usize, size: f64) -> Vec<Tri> {
    let h = |x: f64, y: f64| {
        let (dx, dy) = ((x - size / 2.0) / (size / 2.0), (y - size / 2.0) / (size / 2.0));
        220.0 * (1.0 - dx.hypot(dy)).max(0.0).powf(1.5)
    };
    let p = |i: usize, j: usize| {
        let (x, y) = (i as f64 * size / n as f64, j as f64 * size / n as f64);
        DVec3::new(x, y, h(x, y))
    };
    let mut out = Vec::new();
    for j in 0..n {
        for i in 0..n {
            out.push([p(i, j), p(i + 1, j), p(i + 1, j + 1)]);
            out.push([p(i, j), p(i + 1, j + 1), p(i, j + 1)]);
        }
    }
    out
}

#[test]
fn a_gridded_hill_keeps_every_tile_it_covers() {
    let tris = hill(24, 1024.0);
    let opts = MeshDispOptions { power: PowerChoice::Auto, ..keep() };
    let p = plan(&tris, &opts).expect("plan");
    assert_eq!((p.nx, p.ny, p.tile_count(), p.power), (4, 4, 16, 3));
    let centered = plan(&tris, &MeshDispOptions { origin: Origin::Center, ..opts }).expect("centered");
    assert_eq!((centered.nx, centered.ny, centered.tile_count()), (4, 4, 16));
    assert_eq!(centered.warnings.filled_nodes, 0);
}

#[test]
fn float_noise_on_a_tile_border_adds_no_tile() {
    let tris = hill(24, 1024.0 + 3e-11);
    let p = plan(&tris, &MeshDispOptions { origin: Origin::Center, ..keep() }).expect("plan");
    assert_eq!((p.nx, p.ny, p.tile_count(), p.skipped), (4, 4, 16, 0));
}
