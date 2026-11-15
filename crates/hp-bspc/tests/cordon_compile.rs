mod common;

use glam::{DVec2, DVec3};
use hp_editor::compile_export::CompileExport;
use hp_editor::terrain_ops::{TerrainLayout, TerrainSurface};
use hp_editor::{Document, Owner};
use hp_geom::Aabb;

fn terrain_doc() -> Document {
    let mut doc = Document::new_empty();
    let layout = TerrainLayout {
        min: DVec2::new(-512.0, -128.0),
        size: DVec2::new(1024.0, 256.0),
        tiles: (4, 1),
        power: 3,
        plane_z: 0.0,
        thickness: 16.0,
        material: "dev/dev_measuregeneric01b".to_owned(),
        lightmap_scale: 16,
    };
    let height = |x: f64, y: f64| 8.0 + (x * 0.01).sin() * 6.0 + (y * 0.013).cos() * 4.0;
    let surface = TerrainSurface { height: &height, weights: None };
    doc.create_terrain(&layout, &surface, "Import heightmap").expect("terrain");
    doc
}

fn add_box(doc: &mut Document, min: DVec3, max: DVec3) -> hp_editor::Id {
    let mut next = 1u32;
    let mut ids = || {
        next += 1;
        next
    };
    let sides = hp_geom::hull_solid(&Aabb::new(min, max).corners(), "dev/dev_measuregeneric01b", 0.25, 16, &mut ids)
        .expect("hull");
    doc.create_solid(sides, Owner::World).expect("solid")
}

fn room_doc() -> (Document, hp_editor::Id) {
    let mut doc = Document::new_empty();
    let v = DVec3::new;
    for (a, b) in [
        (v(-272.0, -272.0, -16.0), v(272.0, 272.0, 0.0)),
        (v(-272.0, -272.0, 256.0), v(272.0, 272.0, 272.0)),
        (v(-272.0, -272.0, 0.0), v(-256.0, 272.0, 256.0)),
        (v(256.0, -272.0, 0.0), v(272.0, 272.0, 256.0)),
        (v(-256.0, -272.0, 0.0), v(256.0, -256.0, 256.0)),
        (v(-256.0, 256.0, 0.0), v(256.0, 272.0, 256.0)),
    ] {
        add_box(&mut doc, a, b);
    }
    let pillar = add_box(&mut doc, v(-32.0, -32.0, 0.0), v(32.0, 32.0, 128.0));
    doc.create_point_entity("info_player_start", v(128.0, 128.0, 8.0), &[]).expect("start");
    (doc, pillar)
}

fn set_cordons(doc: &mut Document, list: &[(Aabb, bool)]) {
    let mut c = doc.scene().cordons().clone();
    c.cordons.clear();
    c.active = true;
    for (k, (b, exclude)) in list.iter().enumerate() {
        c.cordons.push(hp_vmf::Cordon {
            name: format!("c{k}"),
            active: true,
            mins: b.min,
            maxs: b.max,
            exclude: *exclude,
        });
    }
    doc.set_cordons(c).expect("cordons");
}

struct Built {
    planes: usize,
    brushes: usize,
    displacements: usize,
    leaked: bool,
}

fn build(doc: &Document) -> (hp_editor::compile_export::CompileSnapshot, Built) {
    let snap = doc.compile_snapshot(&CompileExport::default());
    let mut log = Vec::new();
    let out = hp_bspc::compile(
        &snap.map,
        &common::GameMats,
        &hp_bspc::props::NoModels,
        &hp_bspc::CompileOptions::default(),
        &mut |l| log.push(l.to_owned()),
    )
    .unwrap_or_else(|e| panic!("compiles: {e}: {log:?}"));
    assert!(hp_bspc::validate::validate(&out.bsp).is_empty(), "{:?}", hp_bspc::validate::validate(&out.bsp));
    let leaked = out.leak.is_some();
    let bsp = hp_bsp::Bsp::from_vec(out.bsp).expect("bsp");
    let len = |l: usize| bsp.lump(l).map_or(0, |p| p.len());
    let built = Built {
        planes: len(hp_bspc::emit::lump::PLANES) / 20,
        brushes: len(hp_bspc::emit::lump::BRUSHES) / 12,
        displacements: len(hp_bspc::emit::lump::DISPINFO) / 176,
        leaked,
    };
    (snap, built)
}

#[test]
fn cordoned_compiles_are_sealed_and_keep_what_the_cordons_ask() {
    let v = DVec3::new;
    for b in [
        Aabb::new(v(-600.0, -200.0, -64.0), v(600.0, 200.0, 256.0)),
        Aabb::new(v(-520.0, -136.0, -24.0), v(520.0, 136.0, 24.0)),
    ] {
        let mut doc = terrain_doc();
        set_cordons(&mut doc, &[(b, false)]);
        let (snap, built) = build(&doc);
        assert!(snap.added_player_start && !snap.cordon_left_nothing());
        let back = hp_vmf::parse(&hp_vmf::write(&snap.map)).expect("valid vmf");
        assert!(!back.cordons.active && back.cordons.cordons.is_empty(), "Valve vbsp gets no cordon to apply again");
        assert!(built.planes > 0 && built.brushes >= 6, "sealed by the cordon walls");
        assert_eq!(built.displacements, 4, "the displacement strip is kept");
        assert!(!built.leaked);
    }

    let (mut doc, pillar) = room_doc();
    set_cordons(&mut doc, &[(Aabb::new(v(-400.0, -400.0, -64.0), v(400.0, 400.0, 400.0)), false)]);
    let (_, room) = build(&doc);
    assert!(!room.leaked && room.planes > 0 && room.brushes >= 13);

    set_cordons(&mut doc, &[(Aabb::new(v(-64.0, -64.0, -1.0), v(64.0, 64.0, 200.0)), true)]);
    let (snap, cut) = build(&doc);
    assert!(snap.is_cordoned() && snap.cordon_boxes.is_empty() && snap.exclude_boxes.len() == 1);
    assert!(!snap.map.world.solids.iter().any(|s| s.id == pillar), "the pillar inside the exclude cordon is cut");
    assert_eq!(snap.map.world.solids.len(), 6, "the room walls cross nothing and stay");
    assert!(!cut.leaked && cut.brushes == 6);

    set_cordons(
        &mut doc,
        &[
            (Aabb::new(v(-400.0, -400.0, -64.0), v(400.0, 400.0, 400.0)), false),
            (Aabb::new(v(-64.0, -64.0, -1.0), v(64.0, 64.0, 200.0)), true),
        ],
    );
    let (snap, mixed) = build(&doc);
    assert!(!snap.map.world.solids.iter().any(|s| s.id == pillar));
    assert!(!mixed.leaked && mixed.brushes == 12, "six room brushes and six cordon walls");

    set_cordons(&mut doc, &[(Aabb::new(v(5000.0, 5000.0, 5000.0), v(5200.0, 5200.0, 5200.0)), false)]);
    let (snap, empty) = build(&doc);
    assert!(snap.cordon_left_nothing(), "the compile warns that nothing remains");
    assert!(empty.planes > 0 && empty.brushes == 6 && !empty.leaked, "still a valid sealed map");
}
