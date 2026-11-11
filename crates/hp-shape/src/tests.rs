use glam::{DMat3, DMat4, DQuat, DVec2, DVec3};

use crate::*;

fn cube(min: [f64; 3], max: [f64; 3]) -> TriMesh {
    TriMesh::convex_box(DVec3::from(min), DVec3::from(max), 0)
}

fn prism_ridge_x(z0: f64, z1: f64) -> TriMesh {
    let mut m = TriMesh::new();
    let a = [DVec3::new(-10.0, -10.0, z0), DVec3::new(-10.0, 10.0, z0), DVec3::new(-10.0, 0.0, z1)];
    let b = a.map(|p| p + DVec3::new(20.0, 0.0, 0.0));
    m.add_polygon_facing(&a, DVec3::NEG_X, 0);
    m.add_polygon_facing(&b, DVec3::X, 0);
    for k in 0..3 {
        let q = [a[k], a[(k + 1) % 3], b[(k + 1) % 3], b[k]];
        let c = (q[0] + q[1] + q[2] + q[3]) / 4.0 - DVec3::new(0.0, 0.0, (z0 + z1) * 0.5);
        m.add_polygon_facing(&q, DVec3::new(0.0, c.y, c.z), 0);
    }
    m.closed = true;
    m
}

#[test]
fn box_drops_exactly_on_floor() {
    let floor = cube([-100.0, -100.0, -16.0], [100.0, 100.0, 0.0]);
    let b = cube([-8.0, -8.0, 37.5], [8.0, 8.0, 53.5]);
    let c = sweep(&b, DVec3::ZERO, &[&floor], DVec3::NEG_Z, 0.0, 1000.0).expect("contact");
    assert!((c.t - 37.5).abs() < 1e-9, "{c:?}");
    assert!(c.normal.z > 0.99);
    assert!(sweep(&b, DVec3::ZERO, &[&floor], DVec3::Z, 0.0, 1000.0).is_none());
    let side = sweep(&b, DVec3::new(-150.0, 0.0, -40.0), &[&floor], DVec3::X, 0.0, 1000.0).expect("side");
    assert!((side.t - 42.0).abs() < 1e-9, "{side:?}");
}

#[test]
fn edge_on_edge_contact_is_exact() {
    let below = prism_ridge_x(0.0, 10.0);
    let turn = DMat4::from_quat(DQuat::from_rotation_z(std::f64::consts::FRAC_PI_2));
    let flip = DMat4::from_scale(DVec3::new(1.0, 1.0, -1.0));
    let top = prism_ridge_x(0.0, 10.0).transformed(&(flip * turn)).translated(DVec3::new(0.0, 0.0, 40.0));
    assert!((top.bounds().min.z - 30.0).abs() < 1e-9);
    let c = sweep(&top, DVec3::ZERO, &[&below], DVec3::NEG_Z, 0.0, 1000.0).expect("contact");
    assert!((c.t - 20.0).abs() < 1e-6, "{c:?}");
    assert_eq!(c.kind, ContactKind::EdgeEdge);
}

#[test]
fn sunk_box_is_lifted_back() {
    let floor = cube([-100.0, -100.0, -16.0], [100.0, 100.0, 0.0]);
    let b = cube([-8.0, -8.0, -5.0], [8.0, 8.0, 11.0]);
    let (d, k) = exit_depth(&b, DVec3::ZERO, &[&floor], DVec3::NEG_Z, 16.0).expect("depth");
    assert_eq!(k, 0);
    assert!((d - 5.0).abs() < 1e-9, "{d}");
    let up = DVec3::Z * d;
    let c = sweep(&b, up, &[&floor], DVec3::NEG_Z, 0.0, 100.0).expect("contact");
    assert!(c.t.abs() < 1e-9);
    let free = cube([-8.0, -8.0, 1.0], [8.0, 8.0, 17.0]);
    assert!(exit_depth(&free, DVec3::ZERO, &[&floor], DVec3::NEG_Z, 16.0).is_none());
    let spike = cube([-1.0, -1.0, 0.0], [1.0, 1.0, 20.0]);
    let couch = cube([-30.0, -30.0, 15.0], [30.0, 30.0, 40.0]);
    let (d, _) = exit_depth(&couch, DVec3::ZERO, &[&spike], DVec3::NEG_Z, 30.0).expect("spike");
    assert!((d - 5.0).abs() < 1e-9, "{d}");
}

#[test]
fn barrel_under_table_is_not_lifted_onto_it() {
    let floor = cube([-100.0, -100.0, -16.0], [100.0, 100.0, 0.0]);
    let table = cube([-40.0, -40.0, 30.0], [40.0, 40.0, 32.0]);
    let barrel = cube([-10.0, -10.0, 2.0], [10.0, 10.0, 28.0]);
    assert!(exit_depth(&barrel, DVec3::ZERO, &[&floor, &table], DVec3::NEG_Z, 26.0).is_none());
    let c = sweep(&barrel, DVec3::ZERO, &[&floor, &table], DVec3::NEG_Z, 0.0, 100.0).expect("contact");
    assert_eq!(c.env, 0);
    assert!((c.t - 2.0).abs() < 1e-9);
}

#[test]
fn overlaps_and_touching() {
    let a = cube([0.0, 0.0, 0.0], [10.0, 10.0, 10.0]);
    let flush = cube([10.0, 0.0, 0.0], [20.0, 10.0, 10.0]);
    assert!(overlap(&a, &flush, 0.01).is_none());
    let inter = cube([7.0, 2.0, 2.0], [17.0, 8.0, 8.0]);
    let o = overlap(&a, &inter, 0.01).expect("overlap");
    assert!(o.crossings > 0 && !o.contained);
    let inner = cube([2.0, 2.0, 2.0], [4.0, 4.0, 4.0]);
    let o = overlap(&inner, &a, 0.01).expect("contained");
    assert!(o.contained);
    assert!((separation(&inter, &a, DVec3::X) - 3.0).abs() < 1e-9);
    assert!((separation(&inter, &a, DVec3::NEG_X) - 17.0).abs() < 1e-9);
    assert!((separation(&inter, &a, DVec3::Z) - 8.0).abs() < 1e-9);
    assert_eq!(separation(&flush, &a, DVec3::X), 0.0);
    assert!(contains_point(&a, DVec3::splat(5.0)));
    assert!(!contains_point(&a, DVec3::splat(15.0)));
}

#[test]
fn hulls_rects_and_profiles() {
    let pts =
        [DVec2::new(0.0, 0.0), DVec2::new(4.0, 0.0), DVec2::new(4.0, 2.0), DVec2::new(0.0, 2.0), DVec2::new(2.0, 1.0)];
    let h = hull_2d(&pts);
    assert_eq!(h.len(), 4);
    assert!((polygon_area(&h) - 8.0).abs() < 1e-12);
    let rot = DMat4::from_quat(DQuat::from_rotation_z(30f64.to_radians()));
    let b = cube([-20.0, -5.0, 0.0], [20.0, 5.0, 10.0]).transformed(&rot);
    let r = min_area_rect(&footprint(&b)).expect("rect");
    assert!((r.size.x - 40.0).abs() < 1e-6 && (r.size.y - 10.0).abs() < 1e-6, "{r:?}");
    assert!((r.angle - 30.0).abs() < 1e-6, "{r:?}");
    let ob = oriented_box(&b, DMat3::from_quat(DQuat::from_rotation_z(30f64.to_radians()))).expect("obb");
    assert!((ob.size - DVec3::new(40.0, 10.0, 10.0)).abs().max_element() < 1e-6);
    let p = contact_patch(&b, 0.1).expect("patch");
    assert_eq!(p.z, 0.0);
    assert!((p.area - 400.0).abs() < 1e-6);
    let mut pyramid = TriMesh::new();
    let base = [
        DVec3::new(-10.0, -10.0, 0.0),
        DVec3::new(10.0, -10.0, 0.0),
        DVec3::new(10.0, 10.0, 0.0),
        DVec3::new(-10.0, 10.0, 0.0),
    ];
    let apex = DVec3::new(0.0, 0.0, 20.0);
    pyramid.add_polygon_facing(&base, DVec3::NEG_Z, 0);
    for k in 0..4 {
        let tri = [base[k], base[(k + 1) % 4], apex];
        let c = (tri[0] + tri[1]) * 0.5;
        pyramid.add_polygon_facing(&tri, DVec3::new(c.x, c.y, 5.0), 0);
    }
    let s = slices(&pyramid, 4);
    assert_eq!(s.len(), 4);
    assert!(s[0].area > s[3].area);
    assert!((s[0].max.x - s[0].min.x - 17.5).abs() < 1e-6, "{:?}", s[0]);
    let hm = height_map(&pyramid, 4).expect("hm");
    assert_eq!((hm.nx, hm.ny), (4, 4));
    let top = hm.top[hm.nx + 1].expect("center cell");
    assert!((top - 15.0).abs() < 1e-6, "{top}");
    assert_eq!(hm.bottom[hm.nx + 1], Some(0.0));
}

#[test]
fn ray_hits_nearest() {
    let a = cube([0.0, 0.0, 0.0], [10.0, 10.0, 10.0]);
    let b = cube([0.0, 0.0, 20.0], [10.0, 10.0, 30.0]);
    let h = ray_cast(&[&b, &a], DVec3::new(5.0, 5.0, 50.0), DVec3::NEG_Z, 1000.0).expect("hit");
    assert_eq!(h.env, 0);
    assert!((h.t - 20.0).abs() < 1e-9);
    assert!(h.normal.z > 0.99);
}

#[test]
fn sheet_draws_something() {
    let layers = vec![Layer { mesh: cube([0.0, 0.0, 0.0], [64.0, 32.0, 16.0]), color: [200, 120, 80], wire: false }];
    let markers = vec![Marker { pos: DVec3::ZERO, color: [255, 0, 0], label: "PIVOT".to_owned() }];
    let views = vec![ViewKind::Top, ViewKind::Front, ViewKind::Side, ViewKind::Iso];
    let img = render_sheet(
        &layers,
        &markers,
        &SheetOptions {
            width: 800,
            height: 600,
            views,
            dimensions: true,
            grid: true,
            title: "TEST 64X32X16".to_owned(),
            ground: Some(0.0),
        },
    );
    assert_eq!(img.rgba.len(), 800 * 600 * 4);
    let colored = img.rgba.chunks_exact(4).filter(|p| p[0] > 150 && p[1] > 80 && p[1] < 130 && p[2] < 90).count();
    assert!(colored > 5000, "{colored}");
    let dim = img.rgba.chunks_exact(4).filter(|p| p[0] == 255 && p[1] == 196 && p[2] == 64).count();
    assert!(dim > 100);
}

type Reader = Box<dyn FnMut(&str) -> Option<Vec<u8>>>;

fn read_gmod(path: &str) -> Option<Reader> {
    let root = std::path::Path::new(r"D:\SteamLibrary\steamapps\common\GarrysMod\sourceengine");
    let vpks: Vec<hp_vpk::VpkArchive> = ["hl2_misc_dir.vpk", "content_hl2_dir.vpk"]
        .iter()
        .filter_map(|n| hp_vpk::VpkArchive::open(root.join(n)).ok())
        .collect();
    if vpks.is_empty() || !vpks.iter().any(|v| v.contains(path)) {
        return None;
    }
    Some(Box::new(move |p: &str| vpks.iter().find_map(|v| v.read(p).ok().flatten())))
}

#[test]
fn real_couch_and_barrel_shapes() {
    let couch = "models/props_c17/furniturecouch001a.mdl";
    let Some(mut read) = read_gmod(couch) else {
        eprintln!("skipping: Garry's Mod content not found");
        return;
    };
    let shape = load_shape(couch, &mut read, 0).expect("couch");
    let b = shape.render.bounds();
    let size = b.size();
    assert!(size.x > 30.0 && size.y > 70.0 && size.z > 25.0, "{size:?}");
    assert!(!shape.collision.is_empty());
    let cb = shape.collision_mesh().bounds();
    assert!((cb.size() - size).abs().max_element() < 12.0, "{cb:?} {b:?}");
    let floor = cube([-200.0, -200.0, -16.0], [200.0, 200.0, 0.0]);
    let placed = shape.render.translated(DVec3::new(0.0, 0.0, 100.0));
    let c = sweep(&placed, DVec3::ZERO, &[&floor], DVec3::NEG_Z, 0.0, 1000.0).expect("contact");
    assert!((c.t - (100.0 + b.min.z)).abs() < 1e-6, "{} {}", c.t, b.min.z);
    let barrel = "models/props_c17/oildrum001.mdl";
    let shape = load_shape(barrel, &mut read, 0).expect("barrel");
    let s = shape.render.bounds().size();
    assert!((s.z - 45.0).abs() < 4.0 && (s.x - 29.0).abs() < 1.0, "{s:?}");
    let r = min_area_rect(&footprint(&shape.render)).expect("rect");
    assert!(r.size.x < 30.0 && r.size.y > 27.0, "{r:?}");
}

#[test]
fn box_queries_use_triangles() {
    let mut slab = TriMesh::new();
    slab.add_polygon_facing(
        &[
            DVec3::new(0.0, 0.0, 0.0),
            DVec3::new(100.0, 0.0, 100.0),
            DVec3::new(100.0, 10.0, 100.0),
            DVec3::new(0.0, 10.0, 0.0),
        ],
        DVec3::new(-1.0, 0.0, 1.0),
        0,
    );
    let near_corner = hp_geom::Aabb::new(DVec3::new(80.0, 0.0, 0.0), DVec3::new(100.0, 10.0, 20.0));
    assert!(!touches_box(&slab, &near_corner));
    let on_it = hp_geom::Aabb::new(DVec3::new(45.0, 2.0, 45.0), DVec3::new(55.0, 8.0, 55.0));
    assert!(touches_box(&slab, &on_it));
    let solid = cube([0.0, 0.0, 0.0], [100.0, 100.0, 100.0]);
    assert!(touches_box(&solid, &hp_geom::Aabb::new(DVec3::splat(40.0), DVec3::splat(60.0))));
    assert!(!touches_box(&solid, &hp_geom::Aabb::new(DVec3::splat(140.0), DVec3::splat(160.0))));
}
