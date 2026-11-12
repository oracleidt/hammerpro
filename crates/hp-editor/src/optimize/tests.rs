use std::sync::atomic::AtomicBool;

use glam::DVec3;
use hp_geom::Aabb;

use super::areaportals;
use super::detail::{self, DetailKind, classify};
use super::hints::{HINT, HintCandidate, HintReason};
use super::lights::{self, LightKnowledge, lamp_model, light_material_by_name};
use super::sample::two_rooms;
use super::{Finding, Impact, Topic, sort_findings};
use crate::selection::ObjRef;

fn no_model(_: &hp_vmf::Entity) -> Option<Aabb> {
    None
}

fn no_texlight(_: &str) -> bool {
    false
}

fn know() -> LightKnowledge<'static> {
    LightKnowledge {
        model_bounds: &no_model,
        light_material: &light_material_by_name,
        texlight: &no_texlight,
        max_switchable: 32,
    }
}

#[test]
fn classifies_detail_shapes() {
    assert_eq!(classify(DVec3::new(32.0, 32.0, 32.0), 6, true), Some(DetailKind::Small));
    assert_eq!(classify(DVec3::new(24.0, 112.0, 8.0), 6, true), Some(DetailKind::Step));
    assert_eq!(classify(DVec3::new(8.0, 300.0, 8.0), 6, true), Some(DetailKind::Trim));
    assert_eq!(classify(DVec3::new(32.0, 32.0, 256.0), 6, true), Some(DetailKind::Pillar));
    assert_eq!(classify(DVec3::new(200.0, 100.0, 100.0), 10, false), Some(DetailKind::Complex));
    assert_eq!(classify(DVec3::new(512.0, 512.0, 16.0), 6, true), None);
    assert_eq!(classify(DVec3::new(f64::NAN, 1.0, 1.0), 6, true), None);
}

#[test]
fn suggests_verified_func_detail_and_converts() {
    let mut s = two_rooms();
    let cancel = AtomicBool::new(false);
    let sug = detail::suggest(&s.doc, None, &cancel);
    assert!(sug.verified && !sug.map_leaks, "{sug:?}");
    let ids = sug.ids();
    for c in &s.crates {
        assert!(ids.contains(c), "crate {c} missing from {ids:?}");
    }
    assert!(
        sug.count(DetailKind::Pillar) >= 1 && sug.count(DetailKind::Step) >= 1 && sug.count(DetailKind::Small) >= 4
    );
    let f = sug.finding().expect("finding");
    assert_eq!(f.code, "detail_candidates");
    assert_eq!(f.fix, Some("func_detail"));
    let objs: Vec<ObjRef> = ids.iter().map(|i| ObjRef::Solid(*i)).collect();
    let e = s.doc.tie_to_entity(&objs, "func_detail", &[]).expect("tie");
    assert_eq!(s.doc.scene().entity(e).map(|n| n.entity.classname.as_str()), Some("func_detail"));
    let again = detail::suggest(&s.doc, None, &cancel);
    assert!(again.candidates.is_empty(), "{again:?}");
}

#[test]
fn detail_suggestion_never_offers_a_sealing_brush() {
    let mut s = two_rooms();
    let cancel = AtomicBool::new(false);
    let v = DVec3::new;
    let wall = s
        .doc
        .scene()
        .solids()
        .values()
        .find(|n| n.geom.aabb.min.x == 1152.0 && n.geom.aabb.size().y > 400.0)
        .map(|n| n.solid.id)
        .expect("east wall");
    s.doc.delete(&[ObjRef::Solid(wall)]).expect("delete");
    let w = super::sample::WALL;
    let a = super::sample::block(&mut s.doc, v(1152.0, 0.0, 0.0), v(1168.0, 256.0, 256.0), w);
    let b = super::sample::block(&mut s.doc, v(1152.0, 320.0, 0.0), v(1168.0, 512.0, 256.0), w);
    let c = super::sample::block(&mut s.doc, v(1152.0, 256.0, 64.0), v(1168.0, 320.0, 256.0), w);
    let plug = super::sample::block(&mut s.doc, v(1152.0, 256.0, 0.0), v(1168.0, 320.0, 64.0), w);
    let sug = detail::suggest(&s.doc, None, &cancel);
    assert!(sug.verified, "{sug:?}");
    for id in [a, b, c, plug] {
        assert!(!sug.ids().contains(&id), "sealing brush {id} offered: {sug:?}");
    }
}

#[test]
fn areaportal_suggestion_check_and_fixes() {
    let mut s = two_rooms();
    let sug = areaportals::suggest(&s.doc, None, &no_model, true);
    let door = sug.iter().find(|p| p.door == Some(s.door)).expect("door suggestion");
    assert!(door.sealed, "{door:?}");
    let f = door.finding();
    assert_eq!(f.code, "areaportal_door");
    assert_eq!(f.fix, Some("place_areaportal"));
    let portal = s.doc.place_areaportal(door.bounds, door.door).expect("place");
    let issues = areaportals::check(&s.doc, None, &no_model);
    assert!(!issues.iter().any(|i| i.code == "areaportal_not_sealed"), "{issues:#?}");
    assert!(areaportals::suggest(&s.doc, None, &no_model, true).iter().all(|p| p.door != Some(s.door)));
    let bad = s
        .doc
        .place_areaportal(Aabb::new(DVec3::new(200.0, 200.0, 50.0), DVec3::new(202.0, 260.0, 100.0)), None)
        .expect("bad portal");
    let issues = areaportals::check(&s.doc, None, &no_model);
    assert!(issues.iter().any(|i| i.code == "areaportal_not_sealed" && i.objects == vec![ObjRef::Entity(bad)]));
    assert!(!issues.iter().any(|i| i.objects == vec![ObjRef::Entity(portal)] && i.code == "areaportal_not_sealed"));
    s.doc.update_entity(portal, |e| e.set("StartOpen", "1"));
    let issues = areaportals::check(&s.doc, None, &no_model);
    assert!(issues.iter().any(|i| i.code == "areaportal_door_state"));
    assert!(s.doc.sync_areaportal_state(portal).expect("sync"));
    assert_eq!(s.doc.scene().entity(portal).and_then(|n| n.entity.get("StartOpen")), Some("0"));
    let report = super::vis::VisReport { clusters: 10, areas: 1, ..Default::default() };
    let issues = areaportals::check(&s.doc, Some(&report), &no_model);
    assert!(issues.iter().any(|i| i.code == "areaportals_no_areas"));
}

#[test]
fn audits_lights_and_applies_fixes() {
    let mut s = two_rooms();
    let a = lights::audit(&s.doc, &know());
    let codes: Vec<&str> = a.findings.iter().map(|f| f.code).collect();
    for c in ["light_blocked", "light_no_fixture", "fixture_no_light", "light_redundant"] {
        assert!(codes.contains(&c), "{c} missing from {codes:?}");
    }
    assert!(
        a.findings.iter().any(|f| f.code == "light_no_fixture" && f.objects == vec![ObjRef::Entity(s.floating_light)])
    );
    let lamp = a.findings.iter().find(|f| f.code == "fixture_no_light").expect("lamp");
    assert_eq!(lamp.objects, vec![ObjRef::Entity(s.lamp)]);
    assert!(a.findings.windows(2).all(|w| w[0].rank() >= w[1].rank()));
    let fixes = a.fixes_for(&["add_light", "merge", "unblock"]);
    assert!(fixes.len() >= 3, "{fixes:?}");
    let before = s.doc.scene().entities().len();
    let n = s.doc.apply_light_fixes(&fixes).expect("fix");
    assert!(n >= 3);
    let after = lights::audit(&s.doc, &know());
    let codes: Vec<&str> = after.findings.iter().map(|f| f.code).collect();
    for c in ["light_blocked", "light_redundant", "fixture_no_light"] {
        assert!(!codes.contains(&c), "{c} still in {codes:?}");
    }
    assert!(s.doc.undo());
    assert_eq!(s.doc.scene().entities().len(), before);
}

#[test]
fn light_inconsistency_and_sun() {
    let mut s = two_rooms();
    let kv = |k: &str, v: &str| (k.to_owned(), v.to_owned());
    for (i, value) in ["255 220 180 200", "255 220 180 210", "255 220 180 190", "80 120 255 900"].iter().enumerate() {
        let x = 700.0 + i as f64 * 100.0;
        s.doc
            .create_point_entity("prop_static", DVec3::new(x, 450.0, 240.0), &[kv("model", "models/props/lamp01.mdl")])
            .expect("lamp");
        s.doc.create_point_entity("light", DVec3::new(x, 450.0, 220.0), &[kv("_light", value)]).expect("light");
    }
    super::sample::block(&mut s.doc, DVec3::new(0.0, 0.0, 300.0), DVec3::new(64.0, 64.0, 316.0), "TOOLS/TOOLSSKYBOX");
    let a = lights::audit(&s.doc, &know());
    let odd = a.findings.iter().filter(|f| f.code == "light_inconsistent").count();
    assert_eq!(odd, 1, "{:#?}", a.findings);
    assert!(a.findings.iter().any(|f| f.code == "no_light_environment"));
    let n = s.doc.apply_light_fixes(&a.fixes_for(&["harmonize", "add_sun"])).expect("fix");
    assert_eq!(n, 2);
    let b = lights::audit(&s.doc, &know());
    assert!(!b.findings.iter().any(|f| f.code == "light_inconsistent" || f.code == "no_light_environment"));
}

#[test]
fn names_of_fixtures() {
    assert!(lamp_model("models/props_c17/light_cagelight02_on.mdl"));
    assert!(lamp_model("models/props_interiors/Furniture_Lamp01a.mdl"));
    assert!(!lamp_model("models/props_c17/oildrum001.mdl"));
    assert!(light_material_by_name("lights/white001"));
    assert!(!light_material_by_name("tools/toolslight"));
    assert!(!light_material_by_name("brick/brickwall001a"));
}

#[test]
fn hint_brush_has_one_hint_face() {
    let c = HintCandidate {
        axis: 2,
        at: 128.0,
        rect: Aabb::new(DVec3::new(0.0, 0.0, 128.0), DVec3::new(256.0, 128.0, 128.0)),
        cluster: 0,
        reason: HintReason::DoorwayTop,
        weight: 1.0,
        gain: Some(12),
    };
    let sides = c.sides().expect("sides");
    assert_eq!(sides.len(), 6);
    let hints: Vec<_> = sides.iter().filter(|s| s.material == HINT).collect();
    assert_eq!(hints.len(), 1);
    assert!(hints[0].normal().z > 0.99);
    let f = c.finding();
    assert_eq!(f.fix, Some("hint"));
    let mut s = two_rooms();
    let ids = s.doc.add_hint_brushes(&[c]).expect("add");
    assert_eq!(ids.len(), 1);
    assert!(s.doc.add_hint_brushes(&[]).is_err());
}

#[test]
fn findings_sort_by_impact_then_score() {
    let mut list = vec![
        Finding::new(Topic::Lights, "a", Impact::Low, String::new()).score(1000.0),
        Finding::new(Topic::Lights, "b", Impact::High, String::new()).score(1.0),
        Finding::new(Topic::Lights, "c", Impact::High, String::new()).score(5.0),
    ];
    sort_findings(&mut list);
    let codes: Vec<&str> = list.iter().map(|f| f.code).collect();
    assert_eq!(codes, ["c", "b", "a"]);
    assert!(list[0].focus().is_empty());
}
