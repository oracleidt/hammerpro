use std::fmt::Write as _;
use std::sync::atomic::AtomicBool;

use hp_editor::Document;
use hp_editor::limits::checks::MaterialFacts;
use hp_editor::limits::leak::{find_leaks, flood_map, leak_input_from};
use hp_editor::limits::snapshot::MapSnapshot;

struct ByName;

impl hp_bspc::MaterialSource for ByName {
    fn facts(&self, name: &str) -> hp_bspc::MaterialFacts {
        hp_bspc::materials::facts_by_name(name)
    }
}

type P = [f64; 3];

#[derive(Default)]
struct Vmf {
    world: String,
    entities: String,
    next: u32,
}

impl Vmf {
    fn id(&mut self) -> u32 {
        self.next += 1;
        self.next
    }

    fn solid(&mut self, faces: &[[P; 3]], material: &str) -> String {
        let mut s = format!("solid {{ \"id\" \"{}\"\n", self.id());
        for f in faces {
            let pts: Vec<String> = f.iter().map(|p| format!("({} {} {})", p[0], p[1], p[2])).collect();
            let _ = writeln!(
                s,
                "side {{ \"id\" \"{}\" \"plane\" \"{}\" \"material\" \"{material}\" \"uaxis\" \"[1 0 0 0] 0.25\" \"vaxis\" \"[0 -1 0 0] 0.25\" \"rotation\" \"0\" \"lightmapscale\" \"16\" \"smoothing_groups\" \"0\" }}",
                self.id(),
                pts.join(" ")
            );
        }
        s.push_str("}\n");
        s
    }

    fn box_faces(lo: P, hi: P) -> Vec<[P; 3]> {
        let [x0, y0, z0] = lo;
        let [x1, y1, z1] = hi;
        vec![
            [[x0, y1, z1], [x1, y1, z1], [x1, y0, z1]],
            [[x0, y0, z0], [x1, y0, z0], [x1, y1, z0]],
            [[x0, y1, z1], [x0, y0, z1], [x0, y0, z0]],
            [[x1, y1, z0], [x1, y0, z0], [x1, y0, z1]],
            [[x1, y1, z1], [x0, y1, z1], [x0, y1, z0]],
            [[x1, y0, z0], [x0, y0, z0], [x0, y0, z1]],
        ]
    }

    fn world_box(&mut self, lo: P, hi: P, material: &str) {
        let s = self.solid(&Self::box_faces(lo, hi), material);
        self.world.push_str(&s);
    }

    fn world_faces(&mut self, faces: &[[P; 3]], material: &str) {
        let s = self.solid(faces, material);
        self.world.push_str(&s);
    }

    fn prism(&mut self, poly: &[[f64; 2]], z0: f64, z1: f64, material: &str) {
        let (a, b, c) = (poly[0], poly[1], poly[2]);
        let mut faces = vec![
            [[a[0], a[1], z1], [c[0], c[1], z1], [b[0], b[1], z1]],
            [[a[0], a[1], z0], [b[0], b[1], z0], [c[0], c[1], z0]],
        ];
        for i in 0..poly.len() {
            let (p, q) = (poly[i], poly[(i + 1) % poly.len()]);
            faces.push([[p[0], p[1], z0], [p[0], p[1], z1], [q[0], q[1], z1]]);
        }
        self.world_faces(&faces, material);
    }

    fn detail_box(&mut self, lo: P, hi: P, material: &str) {
        let id = self.id();
        let s = self.solid(&Self::box_faces(lo, hi), material);
        let _ = writeln!(self.entities, "entity {{ \"id\" \"{id}\" \"classname\" \"func_detail\"\n{s}}}");
    }

    fn point(&mut self, class: &str, o: P) -> u32 {
        let id = self.id();
        let _ = writeln!(
            self.entities,
            "entity {{ \"id\" \"{id}\" \"classname\" \"{class}\" \"origin\" \"{} {} {}\" }}",
            o[0], o[1], o[2]
        );
        id
    }

    fn map(&self) -> hp_vmf::Map {
        let text = format!(
            "world {{ \"id\" \"1\" \"classname\" \"worldspawn\" \"skyname\" \"sky_day01_01\"\n{}}}\n{}",
            self.world, self.entities
        );
        hp_vmf::parse(&text).expect("generated map")
    }
}

const WALL: &str = "BRICK/BRICKWALL042G";
const FLOOR: &str = "BRICK/BRICKFLOOR001A";

fn mitred_basement(open_corner: bool) -> hp_vmf::Map {
    let mut v = Vmf { next: 100, ..Vmf::default() };
    v.world_box([-1608.0, -172.0, -280.0], [-1116.0, 156.0, -276.0], FLOOR);
    v.world_box([-1608.0, -172.0, -388.0], [-1604.0, 156.0, -276.0], WALL);
    v.world_box([-1120.0, -172.0, -388.0], [-1116.0, 156.0, -276.0], WALL);
    v.world_box([-1604.0, 152.0, -388.0], [-1120.0, 156.0, -280.0], WALL);
    v.world_box([-1604.0, -172.0, -388.0], [-1120.0, -168.0, -280.0], WALL);
    v.world_box([-1588.0, -168.0, -388.0], [-1120.0, 152.0, -384.0], FLOOR);
    v.world_box([-1604.0, -168.0, -416.0], [-1440.0, 152.0, -412.0], WALL);
    let miter = if open_corner { 150.0 } else { 151.951 };
    v.world_faces(
        &[
            [[-1436.0, 156.0, -388.0], [-1436.0, 156.0, -416.0], [-1436.0, -172.0, -416.0]],
            [[-1436.0, -172.0, -388.0], [-1436.0, -172.0, -416.0], [-1440.0, -172.0, -416.0]],
            [[-1436.0, 156.0, -388.0], [-1436.0, -172.0, -388.0], [-1440.0, -172.0, -388.0]],
            [[-1440.0, -172.0, -416.0], [-1436.0, -172.0, -416.0], [-1436.0, 156.0, -416.0]],
            [[-1440.0, -172.0, -388.0], [-1440.0, -172.0, -416.0], [-1440.0, miter, -416.0]],
            [[-1440.0, miter, -416.0], [-1436.0, 156.0, -416.0], [-1436.0, 156.0, -388.0]],
        ],
        WALL,
    );
    v.world_faces(
        &[
            [[-1608.0, 156.0, -388.0], [-1436.0, 156.0, -388.0], [-1440.0, 152.0, -388.0]],
            [[-1604.0, 152.0, -416.0], [-1440.0, 152.0, -416.0], [-1436.0, 156.0, -416.0]],
            [[-1608.0, 156.0, -416.0], [-1436.0, 156.0, -416.0], [-1436.0, 156.0, -388.0]],
            [[-1440.0, 152.0, -416.0], [-1604.0, 152.0, -416.0], [-1604.0, 152.0, -388.0]],
            [[-1604.0, 152.0, -416.0], [-1608.0, 156.0, -416.0], [-1608.0, 156.0, -388.0]],
            [[-1436.0, 156.0, -416.0], [-1440.0, 152.0, -416.0], [-1440.0, 152.0, -388.0]],
        ],
        WALL,
    );
    v.world_box([-1608.0, -172.0, -416.0], [-1604.0, 156.0, -388.0], WALL);
    v.world_box([-1604.0, -172.0, -416.0], [-1440.0, -168.0, -388.0], WALL);
    for y in [-168.0, -88.0, -8.0, 72.0] {
        v.detail_box([-1604.0, y, -388.0], [-1588.0, y + 80.0, -384.0], "METAL/METALGRATE011A");
    }
    v.point("info_player_start", [-1505.0, 1.0, -384.0]);
    v.point("light", [-1382.0, 83.0, -331.0]);
    v.point("light", [-1520.0, 16.0, -400.0]);
    v.map()
}

fn curved_road(missing_piece: bool, segments: usize, decimals: i32) -> (hp_vmf::Map, u32) {
    let mut v = Vmf { next: 100, ..Vmf::default() };
    let (cx, cy) = (13.37, -21.73);
    let radii = [160.0, 384.123, 640.877, 704.5];
    let start = 3.7f64.to_radians();
    let scale = 10f64.powi(decimals);
    let at = |r: f64, i: usize| {
        let a = start + std::f64::consts::TAU * i as f64 / segments as f64;
        [((cx + r * a.cos()) * scale).round() / scale, ((cy + r * a.sin()) * scale).round() / scale]
    };
    for i in 0..segments {
        let j = i + 1;
        if !(missing_piece && i == segments / 3) {
            v.prism(
                &[at(radii[1], i), at(radii[2], i), at(radii[2], j), at(radii[1], j)],
                -16.0,
                0.0,
                "NATURE/ASPHALT001A",
            );
        }
        v.prism(
            &[at(radii[0], i), at(radii[1], i), at(radii[1], j), at(radii[0], j)],
            -16.0,
            6.0,
            "CONCRETE/CONCRETEFLOOR001A",
        );
        v.prism(
            &[at(radii[2], i), at(radii[3], i), at(radii[3], j), at(radii[2], j)],
            -16.0,
            6.0,
            "CONCRETE/CONCRETEFLOOR001A",
        );
        v.prism(
            &[at(radii[3], i), at(radii[3] + 16.0, i), at(radii[3] + 16.0, j), at(radii[3], j)],
            -16.0,
            256.0,
            WALL,
        );
        v.prism(&[[cx, cy], at(radii[0], i), at(radii[0], j)], -16.0, 6.0, "CONCRETE/CONCRETEFLOOR001A");
    }
    v.world_box([-800.0, -800.0, 256.0], [800.0, 800.0, 272.0], "TOOLS/TOOLSSKYBOX");
    v.point("info_player_start", [cx + 512.0 * 1f64.cos(), cy + 512.0 * 1f64.sin(), 1.0]);
    v.point("light", [cx, cy, 200.0]);
    let prop = v.point("prop_static", [cx + 200.0, cy, 6.0]);
    (v.map(), prop)
}

fn instant(map: &hp_vmf::Map) -> hp_editor::limits::leak::LeakReport {
    let doc = Document::from_map(map.clone());
    let r = find_leaks(&doc.leak_input(&MaterialFacts::guess), &AtomicBool::new(false)).expect("not cancelled");
    let snap = MapSnapshot::capture(&doc);
    let from_snapshot = find_leaks(
        &leak_input_from(&snap, &mut |m| MaterialFacts::guess(snap.material_key(m))),
        &AtomicBool::new(false),
    )
    .expect("not cancelled");
    assert_eq!(r.leaks(), from_snapshot.leaks());
    r
}

fn compiler(map: &hp_vmf::Map) -> Option<hp_bspc::FloodLeak> {
    hp_bspc::flood_check(map, &ByName, None, &mut |_| {}).expect("flood")
}

fn compiler_from_snapshot(map: &hp_vmf::Map) -> Option<hp_bspc::FloodLeak> {
    let doc = Document::from_map(map.clone());
    let snap = MapSnapshot::capture(&doc);
    hp_bspc::flood_check(&flood_map(&snap, &[]), &ByName, None, &mut |_| {}).expect("flood")
}

#[test]
fn a_mitred_corner_off_the_grid_seals_like_the_compiler() {
    let sealed = mitred_basement(false);
    let r = instant(&sealed);
    assert!(!r.leaks(), "a 0.05 unit crack at a mitred corner is no leak: {:?}", r.leak);
    assert_eq!(compiler(&sealed), None);
    assert_eq!(compiler_from_snapshot(&sealed), None);
    let open = mitred_basement(true);
    let r = instant(&open);
    assert!(r.leaks(), "a 2 unit crack leaks");
    let c = compiler(&open).expect("the compiler sees the crack too");
    assert!(c.path.len() >= 2);
    assert!(compiler_from_snapshot(&open).is_some());
}

#[test]
fn curved_road_wedges_seal_like_the_compiler() {
    for (segments, decimals) in [(40, 2), (96, 3)] {
        let (sealed, _) = curved_road(false, segments, decimals);
        let r = instant(&sealed);
        assert!(!r.leaks(), "{segments} wedges: {:?}", r.leak);
        assert_eq!(compiler(&sealed), None, "{segments} wedges");
    }
    let (open, prop) = curved_road(true, 40, 2);
    let doc = Document::from_map(open.clone());
    let leaking = find_leaks(&doc.leak_input(&MaterialFacts::guess), &AtomicBool::new(false)).expect("report").leaking;
    assert!(leaking.contains(&prop), "{leaking:?}");
    let r = instant(&open);
    assert!(r.leaks(), "a missing road piece leaks");
    let c = compiler(&open).expect("the compiler leaks too");
    assert!(c.entity.is_some_and(|e| leaking.contains(&e)), "the compiler names an entity that leaks: {:?}", c.entity);
    let snap = compiler_from_snapshot(&open).expect("the snapshot map leaks too");
    assert_eq!(snap.entity, c.entity);
    assert_eq!(snap.path.len(), c.path.len());
}

#[test]
fn a_cancelled_flood_stops_without_an_answer() {
    let (map, _) = curved_road(true, 40, 2);
    let stop = Some(std::sync::Arc::new(AtomicBool::new(true)));
    let r = hp_bspc::flood_check(&map, &ByName, stop, &mut |_| {});
    assert!(matches!(r, Err(hp_bspc::CompileError::Cancelled)), "{r:?}");
}

struct WithGrate;

impl hp_bspc::MaterialSource for WithGrate {
    fn facts(&self, name: &str) -> hp_bspc::MaterialFacts {
        if name.eq_ignore_ascii_case("METAL/METALLADDER001A") {
            let key = |k: &str| (k == "$alphatest").then(|| "1".to_owned());
            return hp_bspc::materials::facts_from_vmt("LightmappedGeneric", &key, Some((128, 512)), None);
        }
        hp_bspc::materials::facts_by_name(name)
    }
}

#[test]
fn an_alpha_tested_wall_leaks_in_both_checks() {
    let mut v = Vmf { next: 100, ..Vmf::default() };
    v.world_box([-80.0, -80.0, -80.0], [80.0, 80.0, -64.0], FLOOR);
    v.world_box([-80.0, -80.0, 64.0], [80.0, 80.0, 80.0], FLOOR);
    v.world_box([-80.0, -80.0, -64.0], [-64.0, 80.0, 64.0], WALL);
    v.world_box([-64.0, -80.0, -64.0], [64.0, -64.0, 64.0], WALL);
    v.world_box([-64.0, 64.0, -64.0], [64.0, 80.0, 64.0], WALL);
    v.world_box([64.0, -80.0, -64.0], [80.0, 80.0, 64.0], "METAL/METALLADDER001A");
    v.point("info_player_start", [0.0, 0.0, -63.0]);
    let map = v.map();
    let doc = Document::from_map(map.clone());
    let facts = |m: &str| {
        let mut f = MaterialFacts::guess(m);
        f.translucent = m.eq_ignore_ascii_case("METAL/METALLADDER001A");
        f
    };
    let r = find_leaks(&doc.leak_input(&facts), &AtomicBool::new(false)).expect("report");
    assert!(r.leaks(), "an alpha tested brush does not seal the instant check");
    let c = hp_bspc::flood_check(&map, &WithGrate, None, &mut |_| {}).expect("flood");
    assert!(c.is_some(), "nor the compiler");
    assert_eq!(hp_bspc::flood_check(&map, &ByName, None, &mut |_| {}).expect("flood"), None, "solid when opaque");
}
