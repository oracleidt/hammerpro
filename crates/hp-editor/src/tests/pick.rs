use std::time::Instant;

use glam::{DVec2, DVec3};
use hp_geom::{Aabb, Ray, ViewAxis};
use hp_vmf::{Entity, Map, Solid};

use super::{box_sides, cube, no_bounds};
use crate::{Document, ObjRef, Owner, PickOptions, RectMode, SelectOp};

fn row() -> (Document, Vec<crate::Id>, crate::Id) {
    let mut doc = Document::new_empty();
    let mut ids = Vec::new();
    for i in 0..5 {
        ids.push(doc.create_solid(cube(DVec3::new(f64::from(i) * 100.0, 0.0, 0.0), 64.0), Owner::World).expect("s"));
    }
    let e = doc.create_point_entity("info_target", DVec3::new(32.0, 32.0, 200.0), &[]).expect("e");
    doc.clear_selection();
    (doc, ids, e)
}

#[test]
fn ray_picks_closest() {
    let (doc, ids, e) = row();
    let opts = PickOptions::default();
    let ray = Ray::new(DVec3::new(-100.0, 32.0, 32.0), DVec3::X);
    let hit = doc.pick_ray(&ray, &opts, &no_bounds).expect("hit");
    assert_eq!(hit.obj, ObjRef::Solid(ids[0]));
    assert!((hit.t - 100.0).abs() < 1e-9);
    assert_eq!(hit.normal, DVec3::NEG_X);
    assert_eq!(hit.face, Some(5), "the -X side");
    assert_eq!(hit.point, DVec3::new(0.0, 32.0, 32.0));
    let all = doc.pick_ray_all(&ray, &opts, &no_bounds);
    assert_eq!(all.len(), 5);
    assert!(all.windows(2).all(|w| w[0].t <= w[1].t));
    let down = Ray::new(DVec3::new(32.0, 32.0, 1000.0), DVec3::NEG_Z);
    let hit = doc.pick_ray(&down, &opts, &no_bounds).expect("entity hit");
    assert_eq!(hit.obj, ObjRef::Entity(e));
    assert_eq!(hit.point.z, 208.0);
    assert_eq!(hit.normal, DVec3::Z);
    let no_ents = PickOptions { include_entities: false, ..PickOptions::default() };
    assert_eq!(doc.pick_ray(&down, &no_ents, &no_bounds).expect("solid").obj, ObjRef::Solid(ids[0]));
    let big = |_: &Entity| Some(Aabb::new(DVec3::new(-1000.0, 30.0, 150.0), DVec3::new(1000.0, 34.0, 160.0)));
    assert_eq!(doc.pick_ray(&down, &opts, &big).expect("big").point.z, 160.0);
    let skip_first = |o: ObjRef| o != ObjRef::Solid(ids[0]);
    let filtered = PickOptions { filter: Some(&skip_first), ..PickOptions::default() };
    assert_eq!(doc.pick_ray(&ray, &filtered, &no_bounds).expect("hit").obj, ObjRef::Solid(ids[1]));
    let short = PickOptions { max_distance: 50.0, ..PickOptions::default() };
    assert!(doc.pick_ray(&ray, &short, &no_bounds).is_none());
    assert!(format!("{short:?}").contains("max_distance"));
    assert!(doc.pick_ray(&Ray::new(DVec3::ZERO, DVec3::ZERO), &opts, &no_bounds).is_none());
    assert!(doc.pick_ray(&Ray::new(DVec3::NAN, DVec3::X), &opts, &no_bounds).is_none());
}

#[test]
fn picking_follows_edits_and_visibility() {
    let (mut doc, ids, _) = row();
    let ray = Ray::new(DVec3::new(-100.0, 32.0, 32.0), DVec3::X);
    let opts = PickOptions::default();
    assert_eq!(doc.pick_ray(&ray, &opts, &no_bounds).expect("hit").obj, ObjRef::Solid(ids[0]));
    doc.translate(&[ObjRef::Solid(ids[0])], DVec3::new(0.0, 0.0, 500.0)).expect("move");
    assert_eq!(doc.pick_ray(&ray, &opts, &no_bounds).expect("hit").obj, ObjRef::Solid(ids[1]));
    assert!(doc.undo());
    assert_eq!(doc.pick_ray(&ray, &opts, &no_bounds).expect("hit").obj, ObjRef::Solid(ids[0]));
    doc.set_hidden(&[ObjRef::Solid(ids[0])], true);
    assert_eq!(doc.pick_ray(&ray, &opts, &no_bounds).expect("hit").obj, ObjRef::Solid(ids[1]));
    let all = PickOptions { ignore_hidden: false, ..PickOptions::default() };
    assert_eq!(doc.pick_ray(&ray, &all, &no_bounds).expect("hit").obj, ObjRef::Solid(ids[0]));
    doc.delete(&[ObjRef::Solid(ids[1])]).expect("delete");
    assert_eq!(doc.pick_ray(&ray, &opts, &no_bounds).expect("hit").obj, ObjRef::Solid(ids[2]));
    let n = doc.create_solid(cube(DVec3::new(-50.0, 0.0, 0.0), 8.0), Owner::World).expect("new");
    let ray2 = Ray::new(DVec3::new(-100.0, 4.0, 4.0), DVec3::X);
    assert_eq!(doc.pick_ray(&ray2, &opts, &no_bounds).expect("hit").obj, ObjRef::Solid(n));
    for _ in 0..300 {
        doc.create_solid(cube(DVec3::new(0.0, 1000.0, 0.0), 8.0), Owner::World).expect("bulk");
    }
    assert_eq!(doc.pick_ray(&ray2, &opts, &no_bounds).expect("hit").obj, ObjRef::Solid(n));
    assert_eq!(doc.solids_in_box(&Aabb::new(DVec3::new(-60.0, 0.0, 0.0), DVec3::new(-40.0, 8.0, 8.0))), vec![n]);
}

#[test]
fn ray_starting_inside_solid_sees_the_next_one() {
    let (doc, ids, _) = row();
    let ray = Ray::new(DVec3::new(32.0, 32.0, 32.0), DVec3::X);
    let hit = doc.pick_ray(&ray, &PickOptions::default(), &no_bounds).expect("hit");
    assert_eq!(hit.obj, ObjRef::Solid(ids[1]));
}

#[test]
fn pick_2d_edges_and_handles() {
    let (doc, ids, e) = row();
    let objs = doc.pick_2d(ViewAxis::Top, DVec2::new(1.0, 30.0), 2.0, &no_bounds);
    assert_eq!(objs, vec![ObjRef::Solid(ids[0])]);
    let objs = doc.pick_2d(ViewAxis::Top, DVec2::new(132.0, 32.0), 2.0, &no_bounds);
    assert_eq!(objs, vec![ObjRef::Solid(ids[1])]);
    assert!(doc.pick_2d(ViewAxis::Top, DVec2::new(116.0, 16.0), 2.0, &no_bounds).is_empty());
    let objs = doc.pick_2d(ViewAxis::Front, DVec2::new(30.0, 205.0), 1.0, &no_bounds);
    assert_eq!(objs, vec![ObjRef::Entity(e)]);
    let objs = doc.pick_2d(ViewAxis::Front, DVec2::new(1.0, 32.0), 2.0, &no_bounds);
    assert_eq!(objs.len(), 5, "all cubes share the Y = 0 edge in the front view: {objs:?}");
    assert!(doc.pick_2d(ViewAxis::Top, DVec2::NAN, 2.0, &no_bounds).is_empty());
}

#[test]
fn rect_selection_2d() {
    let (mut doc, ids, e) = row();
    let touching = doc.objects_in_rect_2d(
        ViewAxis::Top,
        DVec2::new(50.0, -10.0),
        DVec2::new(150.0, 10.0),
        RectMode::Touching,
        &no_bounds,
    );
    assert_eq!(touching, vec![ObjRef::Solid(ids[0]), ObjRef::Solid(ids[1])]);
    let inside = doc.objects_in_rect_2d(
        ViewAxis::Top,
        DVec2::new(-10.0, -10.0),
        DVec2::new(170.0, 70.0),
        RectMode::Inside,
        &no_bounds,
    );
    assert_eq!(inside, vec![ObjRef::Solid(ids[0]), ObjRef::Solid(ids[1]), ObjRef::Entity(e)]);
    let swapped = doc.objects_in_rect_2d(
        ViewAxis::Top,
        DVec2::new(170.0, 70.0),
        DVec2::new(-10.0, -10.0),
        RectMode::Inside,
        &no_bounds,
    );
    assert_eq!(swapped, inside);
    doc.rotate_axis(&[ObjRef::Solid(ids[4])], DVec3::new(432.0, 32.0, 0.0), DVec3::Z, 45.0).expect("rotate");
    let b = doc.scene().solid(ids[4]).expect("s").bounds;
    let corner = doc.objects_in_rect_2d(
        ViewAxis::Top,
        DVec2::new(b.min.x, b.min.y),
        DVec2::new(b.min.x + 4.0, b.min.y + 4.0),
        RectMode::Touching,
        &no_bounds,
    );
    assert!(corner.is_empty(), "{corner:?}");
    doc.select(&inside, SelectOp::Replace);
    assert_eq!(doc.selection().objects.len(), 3);
}

fn grid_map(n: usize) -> Map {
    let mut map = Map::new_empty();
    let side = (n as f64).cbrt().ceil() as usize;
    let mut next_side = 1u32;
    for i in 0..n {
        let (x, y, z) = (i % side, (i / side) % side, i / (side * side));
        let min = DVec3::new(x as f64 * 48.0, y as f64 * 48.0, z as f64 * 48.0) - DVec3::splat(2400.0);
        let mut sides = box_sides(min, min + DVec3::splat(32.0), "DEV/DEV_MEASUREGENERIC01");
        for s in &mut sides {
            s.id = next_side;
            next_side += 1;
        }
        map.world.solids.push(Solid { id: i as u32 + 2, sides, ..Solid::default() });
    }
    map
}

#[test]
fn grid_map_picks_correctly() {
    let doc = Document::from_map(grid_map(2000));
    let opts = PickOptions::default();
    let ray = Ray::new(DVec3::new(-5000.0, -2384.0, -2384.0), DVec3::X);
    let hit = doc.pick_ray(&ray, &opts, &no_bounds).expect("hit");
    assert_eq!(hit.obj, ObjRef::Solid(2));
    let mut seed = 0x1234_5678_u64;
    let mut rnd = || {
        seed ^= seed << 13;
        seed ^= seed >> 7;
        seed ^= seed << 17;
        (seed >> 11) as f64 / (1u64 << 53) as f64
    };
    for _ in 0..200 {
        let origin = DVec3::new(rnd() * 6000.0 - 3000.0, rnd() * 6000.0 - 3000.0, 4000.0);
        let target = DVec3::new(rnd() * 600.0 - 2400.0, rnd() * 600.0 - 2400.0, rnd() * 600.0 - 2400.0);
        let ray = Ray::new(origin, target - origin);
        let fast = doc.pick_ray(&ray, &opts, &no_bounds).map(|h| (h.obj, h.t));
        let mut best: Option<(ObjRef, f64)> = None;
        for n in doc.scene().solids().values() {
            if let Some((t, _)) = hp_geom::ray_solid(&ray, &n.geom)
                && best.is_none_or(|b| t < b.1)
            {
                best = Some((ObjRef::Solid(n.solid.id), t));
            }
        }
        match (fast, best) {
            (Some(a), Some(b)) => assert!((a.1 - b.1).abs() < 1e-9, "{a:?} vs {b:?}"),
            (None, None) => {}
            other => panic!("mismatch {other:?}"),
        }
    }
}

#[test]
#[ignore = "benchmark"]
fn bench_100k_load_and_pick() {
    let map = grid_map(100_000);
    let text = hp_vmf::write(&map);
    let t = Instant::now();
    let parsed = hp_vmf::parse(&text).expect("parse");
    let parse_time = t.elapsed();
    let t = Instant::now();
    let doc = Document::from_map(parsed);
    let build_time = t.elapsed();
    let opts = PickOptions::default();
    let t = Instant::now();
    let first = doc.pick_ray(&Ray::new(DVec3::new(-5000.0, -2384.0, -2384.0), DVec3::X), &opts, &no_bounds);
    let bvh_time = t.elapsed();
    assert!(first.is_some());
    let mut seed = 0x9E37_79B9_u64;
    let mut rnd = || {
        seed ^= seed << 13;
        seed ^= seed >> 7;
        seed ^= seed << 17;
        (seed >> 11) as f64 / (1u64 << 53) as f64
    };
    let rays: Vec<Ray> = (0..1000)
        .map(|_| {
            let origin = DVec3::new(rnd() * 8000.0 - 4000.0, rnd() * 8000.0 - 4000.0, rnd() * 8000.0 - 4000.0);
            let target = DVec3::new(rnd() * 2400.0 - 2400.0, rnd() * 2400.0 - 2400.0, rnd() * 2400.0 - 2400.0);
            Ray::new(origin, target - origin)
        })
        .collect();
    let t = Instant::now();
    let mut hits = 0;
    let mut worst = std::time::Duration::ZERO;
    for r in &rays {
        let t1 = Instant::now();
        if doc.pick_ray(r, &opts, &no_bounds).is_some() {
            hits += 1;
        }
        worst = worst.max(t1.elapsed());
    }
    let pick_time = t.elapsed();
    let t = Instant::now();
    let map_back = doc.to_map();
    let to_map_time = t.elapsed();
    let t = Instant::now();
    let out = hp_vmf::write(&map_back);
    let write_time = t.elapsed();
    let t = Instant::now();
    let mut doc = doc;
    let all: Vec<ObjRef> = doc.scene().units().collect();
    doc.translate(&all, DVec3::X * 16.0).expect("move");
    let move_time = t.elapsed();
    let t = Instant::now();
    assert!(doc.undo());
    let undo_time = t.elapsed();
    let t = Instant::now();
    doc.delete(&all[..50_000]).expect("delete");
    let delete_time = t.elapsed();
    let t = Instant::now();
    assert!(doc.undo());
    let undo_delete_time = t.elapsed();
    let t = Instant::now();
    let in_rect = doc.objects_in_rect_2d(
        ViewAxis::Top,
        DVec2::splat(-2400.0),
        DVec2::splat(-1400.0),
        RectMode::Touching,
        &no_bounds,
    );
    let rect_time = t.elapsed();
    eprintln!(
        "100k solids ({} MB VMF): parse {parse_time:?}, Document::from_map {build_time:?}, first pick (BVH build) {bvh_time:?}, \
         1000 picks {pick_time:?} ({hits} hits, worst {worst:?}), to_map {to_map_time:?}, write {write_time:?}, \
         move all {move_time:?}, undo move {undo_time:?}, delete 50k {delete_time:?}, undo delete {undo_delete_time:?}, \
         rect query {rect_time:?} ({} objects)",
        out.len() / 1_000_000,
        in_rect.len()
    );
    assert!(pick_time.as_secs_f64() / 1000.0 < 0.002, "average pick must stay under 2 ms");
}

#[test]
fn the_nearest_visible_surface_wins() {
    let mut doc = Document::new_empty();
    let wall = doc
        .create_solid(
            box_sides(DVec3::new(0.0, -128.0, 0.0), DVec3::new(16.0, 128.0, 128.0), "brick/wall"),
            Owner::World,
        )
        .expect("wall");
    let behind = doc
        .create_solid(
            box_sides(DVec3::new(200.0, -128.0, 0.0), DVec3::new(264.0, 128.0, 128.0), "brick/back"),
            Owner::World,
        )
        .expect("behind");
    let clip = doc
        .create_solid(
            box_sides(DVec3::new(-64.0, -128.0, 0.0), DVec3::new(-48.0, 128.0, 128.0), "tools/toolsplayerclip"),
            Owner::World,
        )
        .expect("clip");
    let light = doc.create_point_entity("light", DVec3::new(100.0, 0.0, 64.0), &[]).expect("light");
    let prop = doc.create_point_entity("prop_static", DVec3::new(-200.0, 0.0, 64.0), &[]).expect("prop");
    doc.clear_selection();
    let ray = Ray::new(DVec3::new(-500.0, 0.0, 64.0), DVec3::X);
    let opts = PickOptions::default();
    let order: Vec<ObjRef> = doc.pick_ray_all(&ray, &opts, &no_bounds).iter().map(|h| h.obj).collect();
    assert_eq!(
        order,
        vec![
            ObjRef::Entity(prop),
            ObjRef::Solid(clip),
            ObjRef::Solid(wall),
            ObjRef::Entity(light),
            ObjRef::Solid(behind)
        ]
    );

    let tools_hidden = |m: &str| m.starts_with("tools/");
    let looks = PickOptions { face_hidden: Some(&tools_hidden), ..PickOptions::default() };
    let no_prop = |o: ObjRef| o != ObjRef::Entity(prop);
    let looks_no_prop = PickOptions { filter: Some(&no_prop), ..looks };
    assert_eq!(doc.pick_ray(&ray, &looks_no_prop, &no_bounds).map(|h| h.obj), Some(ObjRef::Solid(wall)), "hidden clip");
    assert_eq!(doc.pick_ray(&ray, &opts, &no_bounds).map(|h| h.obj), Some(ObjRef::Entity(prop)));

    let thin = |e: &Entity, r: &Ray| {
        if e.classname != "prop_static" {
            return crate::EntityRay::Bounds;
        }
        let o = e.origin().unwrap_or_default();
        let t = (o.x - r.origin.x) / r.dir.x;
        let p = r.at(t);
        if t >= 0.0 && (p.y - o.y).abs() < 1.0 {
            crate::EntityRay::Hit { t, normal: DVec3::X }
        } else {
            crate::EntityRay::Miss
        }
    };
    let models = PickOptions { entity_ray: Some(&thin), face_hidden: Some(&tools_hidden), ..PickOptions::default() };
    let hit = doc.pick_ray(&ray, &models, &no_bounds).expect("model hit");
    assert_eq!(hit.obj, ObjRef::Entity(prop));
    assert!((hit.t - 300.0).abs() < 1e-9);
    assert_eq!(hit.normal, DVec3::NEG_X, "normals face the camera");
    let beside = Ray::new(DVec3::new(-500.0, 4.0, 64.0), DVec3::X);
    assert_eq!(
        doc.pick_ray(&beside, &models, &no_bounds).map(|h| h.obj),
        Some(ObjRef::Solid(wall)),
        "a model box is not the model: the gap shows the wall"
    );
    let inside = Ray::new(DVec3::new(-205.0, 0.0, 64.0), DVec3::X);
    assert_eq!(doc.pick_ray(&inside, &models, &no_bounds).map(|h| h.obj), Some(ObjRef::Entity(prop)), "inside its box");
    assert_eq!(doc.pick_ray(&inside, &opts, &no_bounds).map(|h| h.obj), Some(ObjRef::Solid(clip)));

    let back = Ray::new(DVec3::new(-500.0, 0.0, 64.0), DVec3::NEG_X);
    assert!(doc.pick_ray(&back, &opts, &no_bounds).is_none(), "nothing behind the camera");
    doc.set_hidden(&[ObjRef::Solid(wall)], true);
    assert_eq!(doc.pick_ray(&ray, &looks_no_prop, &no_bounds).map(|h| h.obj), Some(ObjRef::Entity(light)));
}

#[test]
fn a_hidden_entry_face_shows_what_is_behind() {
    let mut doc = Document::new_empty();
    let mut sides = box_sides(DVec3::new(0.0, 0.0, 0.0), DVec3::new(128.0, 128.0, 16.0), "nature/dirt");
    for s in &mut sides {
        if s.plane[0].z == 16.0 && s.plane[1].z == 16.0 && s.plane[2].z == 16.0 {
            s.material = "tools/toolsnodraw".to_owned();
        }
    }
    let floor = doc.create_solid(sides, Owner::World).expect("floor");
    let under = doc.create_solid(cube(DVec3::new(0.0, 0.0, -100.0), 64.0), Owner::World).expect("under");
    let down = Ray::new(DVec3::new(32.0, 32.0, 500.0), DVec3::NEG_Z);
    let opts = PickOptions::default();
    assert_eq!(doc.pick_ray(&down, &opts, &no_bounds).map(|h| h.obj), Some(ObjRef::Solid(floor)));
    let nodraw = |m: &str| m == "tools/toolsnodraw";
    let looks = PickOptions { face_hidden: Some(&nodraw), ..PickOptions::default() };
    assert_eq!(doc.pick_ray(&down, &looks, &no_bounds).map(|h| h.obj), Some(ObjRef::Solid(under)));
}
