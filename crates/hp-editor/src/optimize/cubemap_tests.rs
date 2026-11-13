use glam::DVec3;

use super::*;
use crate::optimize::sample::{self, Hole, block, shell};

const TILE: &str = "TILE/TILEFLOOR001A";
const GLASS: &str = "GLASS/GLASSWINDOW001A";

fn know() -> CubemapKnowledge<'static> {
    CubemapKnowledge { material: &class_by_name, strength: &full_strength }
}

fn kv(k: &str, v: &str) -> (String, String) {
    (k.to_owned(), v.to_owned())
}

fn retexture_floor(doc: &mut Document, min: DVec3, max: DVec3, material: &str) -> Id {
    block(doc, min, max, material)
}

fn two_rooms(door: bool) -> Document {
    let mut doc = Document::new_empty();
    let v = DVec3::new;
    let hole = |side: f64| Hole { x_side: side, y0: 192.0, y1: 320.0, z1: 128.0 };
    let a: Vec<Hole> = if door { vec![hole(1.0)] } else { Vec::new() };
    let b: Vec<Hole> = if door { vec![hole(-1.0)] } else { Vec::new() };
    shell(&mut doc, v(0.0, 0.0, 0.0), v(512.0, 512.0, 256.0), 16.0, &a);
    shell(&mut doc, v(640.0, 0.0, 0.0), v(1152.0, 512.0, 256.0), 16.0, &b);
    if door {
        block(&mut doc, v(528.0, 176.0, -16.0), v(624.0, 336.0, 0.0), sample::WALL);
        block(&mut doc, v(528.0, 176.0, 128.0), v(624.0, 336.0, 144.0), sample::WALL);
        block(&mut doc, v(528.0, 176.0, 0.0), v(624.0, 192.0, 128.0), sample::WALL);
        block(&mut doc, v(528.0, 320.0, 0.0), v(624.0, 336.0, 128.0), sample::WALL);
    }
    for i in 0..4 {
        let x = 32.0 + f64::from(i) * 112.0;
        retexture_floor(&mut doc, v(x, 32.0, 0.0), v(x + 96.0, 480.0, 2.0), TILE);
    }
    retexture_floor(&mut doc, v(700.0, 64.0, 0.0), v(1100.0, 448.0, 2.0), TILE);
    block(&mut doc, v(1100.0, 200.0, 64.0), v(1150.0, 300.0, 192.0), GLASS);
    doc
}

fn codes(r: &CubemapReport) -> Vec<&'static str> {
    r.findings.iter().map(|f| f.code).collect()
}

#[test]
fn sizes_follow_the_fgd_choices() {
    assert_eq!(size_pixels(0), 128);
    assert_eq!(size_pixels(1), 1);
    assert_eq!(size_pixels(6), 32);
    assert_eq!(size_pixels(9), 256);
    assert_eq!(size_bytes(9), 6 * 256 * 256 * 4 * 4 / 3);
    assert_eq!(parse_sides(" 12 x 7 -3 0 9"), vec![12, 7, 9]);
    assert_eq!(class_by_name("tools/toolsnodraw"), None);
    assert_eq!(class_by_name("nature/water_canals03"), Some(MaterialClass::Water));
    assert_eq!(class_by_name("glass/glasswindow001a"), Some(MaterialClass::Glass));
    assert_eq!(class_by_name("metal/metalfloor001a"), Some(MaterialClass::Metal));
    assert_eq!(class_by_name("brick/brickwall001a"), None);
    assert_eq!(class_from_material("x/y", true, false), MaterialClass::Water);
    assert_eq!(class_from_material("x/y", false, true), MaterialClass::Glass);
    assert_eq!(class_from_material("x/y", false, false), MaterialClass::Other);
}

#[test]
fn faces_take_the_nearest_cubemap_in_front_like_vbsp() {
    let origins = [DVec3::new(0.0, 0.0, -10.0), DVec3::new(0.0, 0.0, 100.0)];
    assert_eq!(nearest_in_front(&origins, DVec3::ZERO, DVec3::Z), Some(1));
    assert_eq!(nearest_in_front(&origins, DVec3::ZERO, DVec3::NEG_Z), Some(0));
    let behind = [DVec3::new(0.0, 0.0, -10.0), DVec3::new(0.0, 0.0, -100.0)];
    assert_eq!(nearest_in_front(&behind, DVec3::ZERO, DVec3::Z), Some(0));
    assert_eq!(nearest_in_front(&[], DVec3::ZERO, DVec3::Z), None);
}

#[test]
fn lines_and_rays_hit_brushes() {
    let doc = two_rooms(false);
    let idx = SolidIndex::new(&doc, false);
    let a = DVec3::new(300.0, 256.0, 64.0);
    assert!(idx.clear_line(a, DVec3::new(100.0, 100.0, 100.0)));
    assert!(!idx.clear_line(a, DVec3::new(896.0, 256.0, 64.0)), "the walls between the rooms block");
    let down = idx.ray_hit(a, DVec3::NEG_Z, 4096.0).expect("floor");
    assert!((down - 62.0).abs() < 0.5, "{down}");
    assert!(idx.ray_hit(a, DVec3::Z, 4096.0).is_some_and(|t| (t - 192.0).abs() < 0.5));
}

#[test]
fn no_cubemap_at_all_suggests_one_per_room_at_eye_height() {
    let mut doc = two_rooms(true);
    let r = analyze(&doc, &know(), None);
    assert!(r.reflective >= 5, "{}", r.reflective);
    assert_eq!(
        r.rooms.len(),
        2,
        "a doorway separates the rooms: {:?}",
        r.rooms.iter().map(|x| x.center).collect::<Vec<_>>()
    );
    assert!(codes(&r).contains(&"cubemap_none"));
    let news: Vec<&Suggestion> = r.suggestions.iter().filter(|s| s.is_new()).collect();
    assert_eq!(news.len(), 2);
    let idx = SolidIndex::new(&doc, false);
    for s in &news {
        assert!(idx.contains(s.origin).is_none());
        let gap = idx.ray_hit(s.origin, DVec3::NEG_Z, 4096.0).expect("floor");
        assert!((60.0..=72.0).contains(&gap), "eye height, got {gap}");
        assert_eq!(s.origin, s.origin.round());
    }
    assert!(news.iter().any(|s| s.origin.x < 512.0) && news.iter().any(|s| s.origin.x > 640.0));
    let before = doc.scene.entities.len();
    let list: Vec<Suggestion> = r.suggestions.clone();
    let ids = doc.place_cubemaps(&list).expect("place");
    assert_eq!(ids.len(), 2);
    assert_eq!(doc.scene.entities.len(), before + 2);
    let again = analyze(&doc, &know(), None);
    assert!(!codes(&again).contains(&"cubemap_none"));
    assert!(!codes(&again).contains(&"cubemap_room_missing"), "{:?}", codes(&again));
    assert!(again.suggestions.iter().all(|s| !s.is_new()));
    assert!(doc.undo());
    assert_eq!(doc.scene.entities.len(), before, "accepting is one undo step");
}

#[test]
fn a_room_without_its_own_cubemap_is_found() {
    let mut doc = two_rooms(true);
    doc.create_point_entity("env_cubemap", DVec3::new(256.0, 256.0, 64.0), &[]).expect("probe");
    let r = analyze(&doc, &know(), None);
    assert!(codes(&r).contains(&"cubemap_room_missing"), "{:?}", codes(&r));
    let s = r.suggestions.iter().find(|s| s.is_new()).expect("a suggestion for the second room");
    assert!(s.origin.x > 640.0);
    assert_eq!(r.probes[0].room.map(|i| r.rooms[i].center.x < 512.0), Some(true));
}

#[test]
fn faces_behind_a_wall_get_sides_on_their_own_cubemap() {
    let mut doc = two_rooms(false);
    block(&mut doc, DVec3::new(648.0, 200.0, 0.0), DVec3::new(696.0, 300.0, 2.0), TILE);
    let own = doc.create_point_entity("env_cubemap", DVec3::new(1080.0, 100.0, 64.0), &[]).expect("own");
    doc.create_point_entity("env_cubemap", DVec3::new(480.0, 256.0, 64.0), &[]).expect("other");
    let r = analyze(&doc, &know(), None);
    assert!(r.blocked > 0, "the tile by the wall is nearer the cubemap of the other room");
    assert!(codes(&r).contains(&"cubemap_through_wall"), "{:?}", codes(&r));
    let amend = r.suggestions.iter().find(|s| s.probe == Some(own)).expect("sides for the own cubemap");
    assert!(!amend.sides.is_empty());
    doc.place_cubemaps(&r.suggestions).expect("apply");
    let sides = doc.scene.entity(own).and_then(|n| n.entity.get("sides").map(str::to_owned)).unwrap_or_default();
    assert_eq!(parse_sides(&sides), amend.sides);
    let again = analyze(&doc, &know(), None);
    assert!(!codes(&again).contains(&"cubemap_through_wall"), "{:?}", codes(&again));
}

#[test]
fn badly_placed_cubemaps_are_reported() {
    let mut doc = two_rooms(false);
    let v = DVec3::new;
    let inside = doc.create_point_entity("env_cubemap", v(256.0, 256.0, -8.0), &[]).expect("inside");
    let low = doc.create_point_entity("env_cubemap", v(100.0, 400.0, 10.0), &[kv("cubemapsize", "9")]).expect("low");
    let near = doc.create_point_entity("env_cubemap", v(150.0, 400.0, 70.0), &[kv("sides", "999999")]).expect("near");
    let small = doc.create_point_entity("env_cubemap", v(900.0, 256.0, 64.0), &[kv("cubemapsize", "3")]).expect("s");
    let r = analyze(&doc, &know(), None);
    let has = |code: &str, id: Id| r.findings.iter().any(|f| f.code == code && f.objects.contains(&ObjRef::Entity(id)));
    assert!(has("cubemap_inside", inside));
    assert!(has("cubemap_floor", low));
    assert!(has("cubemap_large_size", low));
    assert!(has("cubemap_close", near));
    assert!(has("cubemap_deleted_side", near));
    assert!(has("cubemap_small_size", small));
    assert!(!has("cubemap_floor", near));
    assert_eq!(r.memory_bytes, size_bytes(0) * 2 + size_bytes(9) + size_bytes(3));
    assert!(r.findings.iter().all(|f| f.topic == Topic::Cubemaps));
}

#[test]
fn a_long_room_gets_several_cubemaps() {
    let mut doc = Document::new_empty();
    let v = DVec3::new;
    shell(&mut doc, v(0.0, 0.0, 0.0), v(3072.0, 512.0, 256.0), 16.0, &[]);
    for i in 0..12 {
        let x = 16.0 + f64::from(i) * 254.0;
        block(&mut doc, v(x, 32.0, 0.0), v(x + 240.0, 480.0, 2.0), TILE);
    }
    let r = analyze(&doc, &know(), None);
    assert_eq!(r.rooms.len(), 1);
    let news: Vec<&Suggestion> = r.suggestions.iter().filter(|s| s.is_new()).collect();
    assert!(news.len() >= 3, "{}", news.len());
    assert!(news.iter().any(|s| s.reason == "large_room"));
    for (i, a) in news.iter().enumerate() {
        for b in news.iter().skip(i + 1) {
            assert!(a.origin.distance(b.origin) >= TOO_CLOSE);
        }
    }
}

#[test]
fn too_many_cubemaps_cost_memory() {
    let mut doc = two_rooms(false);
    for i in 0..(MANY + 2) {
        let x = 40.0 + (i % 20) as f64 * 20.0;
        let y = 40.0 + (i / 20) as f64 * 40.0;
        doc.create_point_entity("env_cubemap", DVec3::new(x, y, 100.0), &[]).expect("probe");
    }
    let r = analyze(&doc, &know(), None);
    assert!(codes(&r).contains(&"cubemap_many"));
    assert!(doc.place_cubemaps(&[]).is_err());
}

#[test]
fn a_floor_whose_centre_is_covered_still_reflects() {
    let mut doc = Document::new_empty();
    let v = DVec3::new;
    shell(&mut doc, v(0.0, 0.0, 0.0), v(1024.0, 1024.0, 256.0), 16.0, &[]);
    block(&mut doc, v(0.0, 0.0, 0.0), v(1024.0, 1024.0, 2.0), "nature/water_canals_water2");
    block(&mut doc, v(384.0, 384.0, 2.0), v(640.0, 640.0, 256.0), sample::WALL);
    let r = analyze(&doc, &know(), None);
    let water: Vec<&FaceUse> = r.faces.iter().filter(|f| f.surface == Surface::Water && f.normal.z > 0.5).collect();
    assert_eq!(water.len(), 1, "the water top is visible around the pillar");
    let idx = SolidIndex::new(&doc, false);
    assert!(idx.contains(water[0].spot + water[0].normal).is_none());
    let s = r.suggestions.iter().find(|s| s.is_new()).expect("a cubemap for the pool");
    assert!(idx.contains(s.origin).is_none());
    assert!((60.0..=72.0).contains(&(s.origin.z - 2.0)), "eye height above the water, got {}", s.origin.z);
}

#[test]
fn tints_parse_like_the_engine() {
    assert_eq!(parse_tint("[ .5 .5 .5 ]").map(|v| (v * 100.0).round()), Some(50.0));
    assert_eq!(parse_tint("{255 255 255}").map(|v| (v * 100.0).round()), Some(100.0));
    assert_eq!(parse_tint(".25"), Some(0.25));
    assert_eq!(parse_tint("x"), None);
}

#[test]
fn masked_out_materials_and_tiny_areas_get_no_suggestion() {
    let mut doc = Document::new_empty();
    let v = DVec3::new;
    shell(&mut doc, v(0.0, 0.0, 0.0), v(512.0, 512.0, 256.0), 16.0, &[]);
    block(&mut doc, v(32.0, 32.0, 0.0), v(480.0, 480.0, 2.0), TILE);
    let weak = |m: &str| if m.to_ascii_lowercase().contains("tile") { 0.01 } else { 1.0 };
    let k = CubemapKnowledge { material: &class_by_name, strength: &weak };
    let r = analyze(&doc, &k, None);
    assert_eq!(r.reflective, 0, "an envmap masked to almost nothing does not count");
    assert!(r.suggestions.is_empty());

    let mut doc = Document::new_empty();
    shell(&mut doc, v(0.0, 0.0, 0.0), v(512.0, 512.0, 256.0), 16.0, &[]);
    block(&mut doc, v(32.0, 32.0, 0.0), v(96.0, 96.0, 2.0), TILE);
    let r = analyze(&doc, &know(), None);
    assert!(r.reflective > 0);
    assert!(r.suggestions.is_empty(), "a small shiny patch is not worth a cubemap");
    assert!(r.findings.iter().all(|f| f.code != "cubemap_room_missing" || f.impact != Impact::High));
}

#[test]
fn small_reflective_bits_join_the_room_they_belong_to() {
    let mut doc = two_rooms(true);
    block(&mut doc, DVec3::new(560.0, 240.0, 0.0), DVec3::new(592.0, 272.0, 1.0), TILE);
    let r = analyze(&doc, &know(), None);
    assert_eq!(r.rooms.len(), 2, "the patch in the doorway merges into a neighbour");
}

#[test]
fn a_huge_room_with_one_cubemap_gets_more_where_faces_are_far() {
    let mut doc = Document::new_empty();
    let v = DVec3::new;
    shell(&mut doc, v(0.0, 0.0, 0.0), v(6144.0, 1024.0, 512.0), 16.0, &[]);
    for i in 0..12 {
        let x = 16.0 + f64::from(i) * 510.0;
        block(&mut doc, v(x, 32.0, 0.0), v(x + 500.0, 992.0, 2.0), TILE);
    }
    doc.create_point_entity("env_cubemap", v(256.0, 512.0, 64.0), &[]).expect("probe");
    let r = analyze(&doc, &know(), None);
    assert!(r.far > 0);
    let far: Vec<&Suggestion> = r.suggestions.iter().filter(|s| s.reason == "far_from_cubemap").collect();
    assert!(!far.is_empty(), "{:?}", r.suggestions);
    assert!(far.iter().all(|s| s.origin.x > 256.0 + FAR * 0.5));
}

#[test]
fn new_cubemaps_take_the_usual_size_of_the_map() {
    let mut doc = two_rooms(true);
    for (x, size) in [(100.0, "6"), (300.0, "6"), (400.0, "8")] {
        doc.create_point_entity("env_cubemap", DVec3::new(x, 256.0, 64.0), &[kv("cubemapsize", size)]).expect("probe");
    }
    let r = analyze(&doc, &know(), None);
    assert_eq!(usual_size(&r.probes), 6);
    let s = r.suggestions.iter().find(|s| s.is_new()).expect("the second room needs one");
    assert_eq!(s.size, 6);
    assert_eq!(usual_size(&[]), 0);
}

#[test]
fn a_facade_hidden_from_every_cubemap_gets_one_in_front_of_it() {
    let mut doc = Document::new_empty();
    let v = DVec3::new;
    shell(&mut doc, v(0.0, 0.0, 0.0), v(4096.0, 4096.0, 1024.0), 16.0, &[]);
    block(&mut doc, v(1024.0, 1024.0, 0.0), v(2048.0, 2048.0, 768.0), "METAL/METALWALL001A");
    block(&mut doc, v(2600.0, 500.0, 0.0), v(2664.0, 2300.0, 900.0), sample::WALL);
    doc.create_point_entity("env_cubemap", v(3200.0, 400.0, 64.0), &[]).expect("probe");
    let r = analyze(&doc, &know(), None);
    assert!(r.blocked > 0, "the east facade sees its cubemap only through the screen wall");
    let facade = r.suggestions.iter().find(|s| s.reason == "facade").expect("a facade suggestion");
    assert!(facade.origin.x > 2048.0 + 128.0, "in front of the east facade: {:?}", facade.origin);
    let idx = SolidIndex::new(&doc, false);
    assert!(idx.contains(facade.origin).is_none());
    let before = r.blocked;
    doc.place_cubemaps(&r.suggestions).expect("apply");
    let again = analyze(&doc, &know(), None);
    assert!(again.blocked < before, "{} -> {}", before, again.blocked);
    assert!(again.suggestions.iter().all(|s| s.reason != "facade"), "{:?}", again.suggestions);
    assert_eq!(again.unfixable, 0);
}
