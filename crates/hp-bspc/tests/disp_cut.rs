mod common;

use common::lumps::Lumps;
use glam::DVec3;
use hp_editor::{ClipKeep, Document, Id};
use hp_geom::Plane;

fn box_sides(lo: DVec3, hi: DVec3) -> Vec<hp_vmf::Side> {
    common::block(lo, hi, "dev/dev_measuregeneric01b")
        .sides
        .iter()
        .map(|s| hp_vmf::Side {
            plane: s.points,
            material: s.material.clone(),
            uaxis: hp_vmf::TexAxis { axis: DVec3::X, shift: 0.0, scale: 0.25 },
            vaxis: hp_vmf::TexAxis { axis: DVec3::NEG_Y, shift: 0.0, scale: 0.25 },
            ..Default::default()
        })
        .collect()
}

fn disp_solids(doc: &Document) -> Vec<Id> {
    let mut ids: Vec<Id> = doc
        .scene()
        .solids()
        .iter()
        .filter(|(_, n)| n.solid.sides.iter().any(|s| s.disp.is_some()))
        .map(|(id, _)| *id)
        .collect();
    ids.sort_unstable();
    ids
}

fn compiled_disps(doc: &Document) -> usize {
    let map = doc.to_map();
    let mut lines = Vec::new();
    let out = hp_bspc::compile(&map, &common::GameMats, &hp_bspc::props::NoModels, &Default::default(), &mut |l| {
        lines.push(l.to_owned())
    })
    .expect("the cut map compiles");
    assert!(out.leak.is_none(), "the room stays sealed");
    let bad: Vec<&String> = lines
        .iter()
        .filter(|l| {
            let l = l.to_ascii_lowercase();
            l.contains("error") || l.contains("invalid") || (l.contains("disp") && l.contains("warning"))
        })
        .collect();
    assert!(bad.is_empty(), "{bad:?}");
    let mut infos: Vec<i16> =
        Lumps::from_bytes(&out.bsp).faces().iter().map(|f| f.dispinfo).filter(|d| *d >= 0).collect();
    infos.sort_unstable();
    infos.dedup();
    infos.len()
}

fn start() -> (Document, Id) {
    let map = hp_vmf::parse(&common::maps::displacement().text()).expect("the VMF parses");
    let doc = Document::from_map(map);
    let ids = disp_solids(&doc);
    assert_eq!(ids.len(), 1);
    assert_eq!(compiled_disps(&doc), 1);
    (doc, ids[0])
}

#[test]
fn a_power_3_displacement_cut_along_a_grid_line_compiles_in_two_pieces() {
    let (mut doc, id) = start();
    let plane = Plane::new(DVec3::X, -64.0).unwrap();
    doc.clip_with(&[id], &plane, ClipKeep::Both, None).expect("clip");
    let pieces = disp_solids(&doc);
    assert_eq!(pieces.len(), 2);
    for p in &pieces {
        let disp = doc.scene().solid(*p).unwrap().solid.sides.iter().find_map(|s| s.disp.clone()).unwrap();
        assert_eq!(disp.power, 3);
    }
    assert_eq!(compiled_disps(&doc), 2);
}

#[test]
fn a_power_3_displacement_cut_across_grid_lines_compiles() {
    let (mut doc, id) = start();
    let plane = Plane::new(DVec3::Y, -100.0).unwrap();
    doc.clip_with(&[id], &plane, ClipKeep::Both, None).expect("clip");
    assert_eq!(disp_solids(&doc).len(), 2);
    assert_eq!(compiled_disps(&doc), 2);
    let n = DVec3::new(1.0, 1.0, 0.0).normalize();
    let ids = disp_solids(&doc);
    doc.clip_with(&ids, &Plane::new(n, n.dot(DVec3::new(-150.0, 0.0, 0.0))).unwrap(), ClipKeep::Both, None)
        .expect("diagonal clip");
    let pieces = disp_solids(&doc).len();
    assert!(pieces >= 4, "{pieces}");
    assert_eq!(compiled_disps(&doc), pieces);
}

#[test]
fn slice_and_carve_keep_displacements_and_compile() {
    let (mut doc, id) = start();
    let planes = [Plane::new(DVec3::X, -128.0).unwrap(), Plane::new(DVec3::X, 0.0).unwrap()];
    doc.slice_solids(&[id], &planes, Some("tools/toolsnodraw")).expect("slice");
    assert_eq!(disp_solids(&doc).len(), 3);
    assert_eq!(compiled_disps(&doc), 3);
    let carver = doc
        .create_solid(
            box_sides(DVec3::new(-170.0, -170.0, -8.0), DVec3::new(-110.0, -110.0, 40.0)),
            hp_editor::Owner::World,
        )
        .expect("carver");
    doc.carve(&[carver]).expect("carve");
    doc.delete(&[hp_editor::ObjRef::Solid(carver)]).expect("delete carver");
    let pieces = disp_solids(&doc).len();
    assert!(pieces > 3, "{pieces}");
    assert_eq!(compiled_disps(&doc), pieces);
}
