use std::path::{Path, PathBuf};

use glam::DVec3;
use hp_geom::Aabb;
use hp_vmf::{Entity, Map, SaveOptions, Solid};

use crate::scene::Owner;
use crate::{Document, EditError, EditResult, Id};

pub const CORDON_SEAL_THICKNESS: f64 = 16.0;

pub const DEFAULT_SEAL_MATERIAL: &str = "TOOLS/TOOLSSKYBOX";

#[derive(Clone, Copy, Debug, Default, PartialEq, Eq, Hash)]
pub enum VisibilityExport {
    #[default]
    HammerCompatible,
    Everything,
    AsSaved,
}

#[derive(Clone, Debug, PartialEq)]
pub struct CompileExport {
    pub visibility: VisibilityExport,
    pub cordons: bool,
    pub seal_material: String,
}

impl Default for CompileExport {
    fn default() -> Self {
        Self { visibility: VisibilityExport::default(), cordons: true, seal_material: DEFAULT_SEAL_MATERIAL.to_owned() }
    }
}

#[derive(Clone, Debug, PartialEq)]
pub struct CompileSnapshot {
    pub map: Map,
    pub cordon_boxes: Vec<Aabb>,
    pub cordoned_out: usize,
    pub hidden: usize,
    pub hidden_lights: usize,
    pub added_player_start: bool,
}

impl CompileSnapshot {
    pub fn is_cordoned(&self) -> bool {
        !self.cordon_boxes.is_empty()
    }

    pub fn write(&self, dir: &Path, name: &str) -> EditResult<PathBuf> {
        check_file_name(name)?;
        std::fs::create_dir_all(dir).map_err(hp_vmf::VmfError::from)?;
        let path = dir.join(format!("{name}.vmf"));
        hp_vmf::save_with(&self.map, &path, &SaveOptions { backup: false, sync: false })?;
        Ok(path)
    }
}

fn check_file_name(name: &str) -> EditResult<()> {
    let bad = || EditError::Invalid(format!("not a valid map file name: {name:?}"));
    if name.is_empty()
        || name.len() > 200
        || name == "."
        || name == ".."
        || name.chars().any(|c| matches!(c, '/' | '\\' | ':' | '*' | '?' | '"' | '<' | '>' | '|') || c.is_control())
    {
        return Err(bad());
    }
    let stem = name.split('.').next().unwrap_or(name).trim_end().to_ascii_uppercase();
    let device = matches!(stem.as_str(), "CON" | "PRN" | "AUX" | "NUL" | "CONIN$" | "CONOUT$")
        || (stem.len() == 4
            && (stem.starts_with("COM") || stem.starts_with("LPT"))
            && stem.as_bytes().get(3).is_some_and(u8::is_ascii_digit));
    if device {
        return Err(bad());
    }
    Ok(())
}

pub fn seal_boxes(b: &Aabb, thickness: f64) -> [Aabb; 6] {
    let (lo, hi, t) = (b.min, b.max, thickness);
    let v = DVec3::new;
    [
        Aabb::new(v(lo.x - t, lo.y - t, lo.z - t), v(lo.x, hi.y + t, hi.z + t)),
        Aabb::new(v(hi.x, lo.y - t, lo.z - t), v(hi.x + t, hi.y + t, hi.z + t)),
        Aabb::new(v(lo.x, lo.y - t, lo.z - t), v(hi.x, lo.y, hi.z + t)),
        Aabb::new(v(lo.x, hi.y, lo.z - t), v(hi.x, hi.y + t, hi.z + t)),
        Aabb::new(v(lo.x, lo.y, lo.z - t), v(hi.x, hi.y, lo.z)),
        Aabb::new(v(lo.x, lo.y, hi.z), v(hi.x, hi.y, hi.z + t)),
    ]
}

fn inside_any(boxes: &[Aabb], p: DVec3) -> bool {
    boxes.iter().any(|b| b.contains_point(p))
}

fn touches_any(boxes: &[Aabb], b: &Aabb) -> bool {
    !b.is_empty() && boxes.iter().any(|c| c.intersects(b))
}

impl Document {
    pub fn content_serial(&self) -> u64 {
        self.history.state_serial()
    }

    pub fn compile_snapshot(&self, export: &CompileExport) -> CompileSnapshot {
        let boxes = if export.cordons { self.cordon_boxes() } else { Vec::new() };
        self.snapshot_in(export, boxes)
    }

    pub fn compile_snapshot_in(&self, export: &CompileExport, boxes: &[Aabb]) -> CompileSnapshot {
        self.snapshot_in(export, boxes.to_vec())
    }

    fn snapshot_in(&self, export: &CompileExport, boxes: Vec<Aabb>) -> CompileSnapshot {
        let mut map = self.to_map();
        let scene = self.scene();
        let mut hidden = 0usize;
        match export.visibility {
            VisibilityExport::AsSaved => {}
            VisibilityExport::Everything => {
                for e in &mut map.entities {
                    e.hidden = false;
                    e.solids.iter_mut().for_each(|s| s.hidden = false);
                }
                map.world.solids.iter_mut().for_each(|s| s.hidden = false);
            }
            VisibilityExport::HammerCompatible => {
                let hide_solid = |s: &mut Solid| {
                    let hide = !s.hidden && !self.is_solid_visible(s.id);
                    s.hidden |= hide;
                    usize::from(hide)
                };
                hidden += map.world.solids.iter_mut().map(hide_solid).sum::<usize>();
                for e in &mut map.entities {
                    if !e.hidden && !self.is_entity_visible(e.id) {
                        e.hidden = true;
                        hidden += 1;
                    }
                    if !e.hidden {
                        hidden += e.solids.iter_mut().map(hide_solid).sum::<usize>();
                    }
                }
            }
        }

        let mut cordoned_out = 0usize;
        let mut added_player_start = false;
        if !boxes.is_empty() {
            let solid_touches = |id: Id| scene.solid(id).is_some_and(|n| touches_any(&boxes, &n.bounds));
            let before = map.world.solids.len();
            map.world.solids.retain(|s| solid_touches(s.id));
            cordoned_out += before - map.world.solids.len();
            let before = map.entities.len();
            map.entities.retain(|e| {
                if !e.solids.is_empty() {
                    return e.solids.iter().any(|s| solid_touches(s.id));
                }
                e.classname.eq_ignore_ascii_case("light_environment")
                    || e.origin().is_none_or(|o| inside_any(&boxes, o))
            });
            cordoned_out += before - map.entities.len();
            if let Some(first) = boxes.first() {
                let inside = DVec3::new(first.center().x, first.center().y, first.max.z - 8.0);
                for e in &mut map.entities {
                    if e.classname.eq_ignore_ascii_case("light_environment")
                        && e.origin().is_none_or(|o| !inside_any(&boxes, o))
                    {
                        e.set("origin", format!("{} {} {}", inside.x, inside.y, inside.z));
                    }
                }
            }
            let next_id = self.seal_cordons(&mut map, &boxes, &export.seal_material);
            added_player_start = self.add_player_start(&mut map, &boxes, next_id);
            map.cordons.active = false;
        }
        crate::sky::bake_switches(&mut map);
        let hidden_lights = if export.visibility == VisibilityExport::Everything {
            0
        } else {
            map.entities.iter().filter(|e| e.hidden && e.classname.to_ascii_lowercase().starts_with("light")).count()
        };
        CompileSnapshot { map, cordon_boxes: boxes, cordoned_out, hidden, hidden_lights, added_player_start }
    }

    pub fn write_compile_snapshot(&self, dir: &Path, name: &str, export: &CompileExport) -> EditResult<PathBuf> {
        self.compile_snapshot(export).write(dir, name)
    }

    fn seal_cordons(&self, map: &mut Map, boxes: &[Aabb], material: &str) -> Id {
        let material = if material.trim().is_empty() { DEFAULT_SEAL_MATERIAL } else { material.trim() };
        let mut next_id = self.peek_next_object_id();
        let mut next_side = self.peek_next_side_id();
        for b in boxes {
            for wall in seal_boxes(b, CORDON_SEAL_THICKNESS) {
                let mut side_id = || {
                    let id = next_side;
                    next_side = next_side.saturating_add(1);
                    id
                };
                let Ok(sides) = hp_geom::hull_solid(&wall.corners(), material, 0.25, 16, &mut side_id) else {
                    continue;
                };
                map.world.solids.push(Solid { id: next_id, sides, ..Solid::default() });
                next_id = next_id.saturating_add(1);
            }
        }
        next_id
    }

    fn add_player_start(&self, map: &mut Map, boxes: &[Aabb], id: Id) -> bool {
        let has_start = map.entities.iter().any(|e| {
            !e.hidden
                && e.classname.to_ascii_lowercase().starts_with("info_player_")
                && e.origin().is_some_and(|o| inside_any(boxes, o))
        });
        let Some(first) = boxes.first() else { return false };
        if has_start {
            return false;
        }
        let c = first.center();
        let mut z = first.min.z + 1.0;
        let scene = self.scene();
        for _ in 0..64 {
            let top = scene
                .solids()
                .values()
                .filter(|n| n.owner == Owner::World && !n.solid.hidden)
                .filter(|n| {
                    let b = &n.bounds;
                    b.min.x <= c.x && c.x <= b.max.x && b.min.y <= c.y && c.y <= b.max.y
                })
                .filter(|n| n.bounds.max.z > z && n.bounds.min.z < z + 72.0)
                .map(|n| n.bounds.max.z)
                .fold(f64::NEG_INFINITY, f64::max);
            if !top.is_finite() {
                break;
            }
            z = top + 1.0;
        }
        let z = z.min(first.max.z - 73.0).max(first.min.z + 1.0);
        let mut e = Entity { id, ..Entity::default() };
        e.classname = "info_player_start".to_owned();
        e.set("origin", hp_vmf::fmt_vec3(DVec3::new(c.x.round(), c.y.round(), z.round())));
        e.set("angles", "0 0 0");
        map.entities.push(e);
        true
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::ObjRef;
    use hp_vmf::{Cordon, CordonBox, Cordons};

    fn doc() -> Document {
        let text = r#"
world { "id" "1" "classname" "worldspawn"
  solid { "id" "2" side { "id" "1" "plane" "(0 64 64) (64 64 64) (64 0 64)" "material" "A" } side { "id" "2" "plane" "(0 0 0) (64 0 0) (64 64 0)" "material" "A" } side { "id" "3" "plane" "(0 64 64) (0 0 64) (0 0 0)" "material" "A" } side { "id" "4" "plane" "(64 64 0) (64 0 0) (64 0 64)" "material" "A" } side { "id" "5" "plane" "(64 64 64) (0 64 64) (0 64 0)" "material" "A" } side { "id" "6" "plane" "(64 0 0) (0 0 0) (0 0 64)" "material" "A" } }
  solid { "id" "3" side { "id" "7" "plane" "(1000 1064 64) (1064 1064 64) (1064 1000 64)" "material" "A" } side { "id" "8" "plane" "(1000 1000 0) (1064 1000 0) (1064 1064 0)" "material" "A" } side { "id" "9" "plane" "(1000 1064 64) (1000 1000 64) (1000 1000 0)" "material" "A" } side { "id" "10" "plane" "(1064 1064 0) (1064 1000 0) (1064 1000 64)" "material" "A" } side { "id" "11" "plane" "(1064 1064 64) (1000 1064 64) (1000 1064 0)" "material" "A" } side { "id" "12" "plane" "(1064 1000 0) (1000 1000 0) (1000 1000 64)" "material" "A" } }
}
entity { "id" "10" "classname" "light" "origin" "32 32 200" }
entity { "id" "11" "classname" "logic_auto" }
entity { "id" "12" "classname" "info_player_start" "origin" "1032 1032 100" }
"#;
        Document::from_map(hp_vmf::parse(text).expect("parse"))
    }

    fn written(snap: &CompileSnapshot) -> String {
        hp_vmf::write(&snap.map)
    }

    #[test]
    fn hidden_objects_follow_hammer() {
        let mut d = doc();
        d.set_hidden(&[ObjRef::Solid(2)], true);
        let vg = d.create_visgroup("Lights", None, &[ObjRef::Entity(10)]).expect("visgroup");
        d.set_visgroup_visible(vg, false).expect("hide visgroup");
        let snap = d.compile_snapshot(&CompileExport::default());
        assert_eq!(snap.hidden, 1, "the visgroup hidden light (the solid was already hidden)");
        let light = snap.map.entities.iter().find(|e| e.id == 10).expect("light kept");
        assert!(light.hidden, "wrapped in a hidden block");
        assert!(snap.map.world.solids.iter().find(|s| s.id == 2).is_some_and(|s| s.hidden));
        assert!(written(&snap).contains("hidden\n"));
        assert!(!d.scene().entity(10).expect("light").entity.hidden);

        let everything =
            d.compile_snapshot(&CompileExport { visibility: VisibilityExport::Everything, ..CompileExport::default() });
        assert!(everything.map.world.solids.iter().all(|s| !s.hidden));
        assert!(everything.map.entities.iter().all(|e| !e.hidden));
        let saved =
            d.compile_snapshot(&CompileExport { visibility: VisibilityExport::AsSaved, ..CompileExport::default() });
        assert!(!saved.map.entities.iter().any(|e| e.hidden), "a visgroup hide is not a Hide");
        assert!(saved.map.world.solids.iter().any(|s| s.hidden));
        assert!(!snap.is_cordoned());
    }

    #[test]
    fn instances_are_left_to_the_compiler() {
        let text = "world\n{\n\"id\" \"1\"\n\"classname\" \"worldspawn\"\n}\n\
                    entity\n{\n\"id\" \"7\"\n\"classname\" \"func_instance\"\n\"file\" \"instances/a.vmf\"\n\"origin\" \"64 0 0\"\n\"fixup_style\" \"0\"\n}\n";
        let d = Document::from_map(hp_vmf::parse(text).expect("parse"));
        let snap = d.compile_snapshot(&CompileExport::default());
        let inst = snap.map.entities.iter().find(|e| e.id == 7).expect("kept");
        assert_eq!(inst.classname, "func_instance");
        assert_eq!(inst.get("file"), Some("instances/a.vmf"));
        assert!(!inst.hidden);
    }

    #[test]
    fn cordon_keeps_the_box_and_seals_it() {
        let mut d = doc();
        let cordons = Cordons {
            active: true,
            cordons: vec![Cordon {
                name: "room".into(),
                active: true,
                mins: DVec3::new(-128.0, -128.0, 0.0),
                maxs: DVec3::splat(256.0),
            }],
            ..Cordons::default()
        };
        d.set_cordons(cordons).expect("cordons");
        let snap = d.compile_snapshot(&CompileExport::default());
        assert!(snap.is_cordoned());
        let ids: Vec<Id> = snap.map.world.solids.iter().map(|s| s.id).collect();
        assert!(ids.contains(&2) && !ids.contains(&3), "{ids:?}");
        assert_eq!(snap.map.world.solids.len(), 1 + 6, "six sealing brushes");
        let seal = snap.map.world.solids.last().expect("seal");
        assert!(seal.sides.iter().all(|s| s.material == DEFAULT_SEAL_MATERIAL));
        for s in &snap.map.world.solids[1..] {
            assert!(hp_geom::validate_solid(&s.sides).is_ok());
            assert!(s.id >= d.peek_next_object_id());
        }
        let classes: Vec<&str> = snap.map.entities.iter().map(|e| e.classname.as_str()).collect();
        assert_eq!(classes, ["light", "logic_auto", "info_player_start"]);
        assert!(snap.added_player_start);
        let start = snap.map.entities.last().and_then(Entity::origin).expect("origin");
        assert!(snap.cordon_boxes[0].contains_point(start));
        assert_eq!(start.z, 65.0, "on top of the brush under the box center");
        assert_eq!(snap.cordoned_out, 2);
        assert!(!snap.map.cordons.active, "the compilers never cordon again");
        assert!(d.scene().cordons().active);

        let mut c = d.scene().cordons().clone();
        c.extra_boxes.push(CordonBox { cordon: 0, mins: DVec3::new(900.0, 900.0, -100.0), maxs: DVec3::splat(1100.0) });
        c.cordons.push(Cordon { name: "off".into(), active: false, mins: DVec3::ZERO, maxs: DVec3::ONE });
        c.cordons.push(Cordon {
            name: "flat".into(),
            active: true,
            mins: DVec3::ZERO,
            maxs: DVec3::new(10.0, 10.0, 0.0),
        });
        d.set_cordons(c).expect("cordons");
        assert_eq!(d.cordon_boxes().len(), 2);
        let snap = d.compile_snapshot(&CompileExport::default());
        assert!(snap.map.world.solids.iter().any(|s| s.id == 3));
        assert!(!snap.added_player_start, "the player start is inside the second box");
        let all = d.compile_snapshot(&CompileExport { cordons: false, ..CompileExport::default() });
        assert_eq!(all.map.world.solids.len(), 2);
    }

    #[test]
    fn snapshot_in_given_boxes() {
        let d = doc();
        let around = Aabb::new(DVec3::new(900.0, 900.0, -64.0), DVec3::new(1200.0, 1200.0, 256.0));
        let snap = d.compile_snapshot_in(&CompileExport::default(), &[around]);
        let ids: Vec<Id> = snap.map.world.solids.iter().map(|s| s.id).collect();
        assert!(ids.contains(&3) && !ids.contains(&2), "{ids:?}");
        assert_eq!(snap.map.world.solids.len(), 1 + 6, "sealed");
        assert!(!d.scene().cordons().active, "the document has no cordon");
        let all = d.compile_snapshot_in(&CompileExport::default(), &[]);
        assert_eq!(all.map.world.solids.len(), 2);
        assert!(!all.is_cordoned());
    }

    #[test]
    fn the_sun_stays_in_a_cut_out_box() {
        let mut d = doc();
        let far = DVec3::new(-5000.0, -5000.0, 0.0);
        d.create_point_entity("light_environment", far, &[("_light".to_owned(), "255 255 255 200".to_owned())])
            .expect("sun");
        d.create_point_entity("light", far, &[]).expect("light");
        let around = Aabb::new(DVec3::new(900.0, 900.0, -64.0), DVec3::new(1200.0, 1200.0, 256.0));
        let snap = d.compile_snapshot_in(&CompileExport::default(), &[around]);
        let classes: Vec<&str> = snap.map.entities.iter().map(|e| e.classname.as_str()).collect();
        assert!(classes.contains(&"light_environment"), "{classes:?}");
        let sun = snap.map.entities.iter().find(|e| e.classname == "light_environment").expect("sun");
        assert!(sun.origin().is_some_and(|o| around.contains_point(o)), "moved inside the box: {:?}", sun.origin());
        assert!(!classes.contains(&"light"), "{classes:?}");
    }

    #[test]
    fn content_serial_follows_edits_not_selection() {
        let mut d = doc();
        let a = d.content_serial();
        d.set_selection(&[ObjRef::Solid(2)]);
        assert_eq!(d.content_serial(), a, "selection is not content");
        d.set_entity_key(&[10], "_light", "1 2 3 4").expect("edit");
        let b = d.content_serial();
        assert_ne!(b, a);
        assert!(d.undo());
        assert_eq!(d.content_serial(), a);
        assert!(d.redo());
        assert_eq!(d.content_serial(), b);
    }

    #[test]
    fn seal_walls_do_not_overlap() {
        let b = Aabb::new(DVec3::ZERO, DVec3::new(100.0, 200.0, 300.0));
        let walls = seal_boxes(&b, 16.0);
        for (i, a) in walls.iter().enumerate() {
            assert!(!a.intersection(&b).size().cmpgt(DVec3::ZERO).all(), "wall {i} enters the box");
            for (j, c) in walls.iter().enumerate().skip(i + 1) {
                let o = a.intersection(c).size();
                assert!(!(o.x > 0.0 && o.y > 0.0 && o.z > 0.0), "walls {i} and {j} overlap");
            }
        }
    }

    #[test]
    fn snapshot_file_names_are_checked() {
        let dir = std::path::Path::new(env!("CARGO_MANIFEST_DIR")).join("../../target/hp-editor/scratch/snapshot");
        let _ = std::fs::remove_dir_all(&dir);
        let d = doc();
        let path = d.write_compile_snapshot(&dir, "gm_room", &CompileExport::default()).expect("written");
        assert_eq!(path, dir.join("gm_room.vmf"));
        let back = hp_vmf::load(&path).expect("reload");
        assert_eq!(back.world.solids.len(), 2);
        assert!(!dir.join("gm_room.vmx").exists(), "no backup");
        for bad in ["", "..", "a/b", "a\\b", "C:x", "con", "CON.vmf", "lpt1", "com9", "nul ", "a?b"] {
            assert!(d.write_compile_snapshot(&dir, bad, &CompileExport::default()).is_err(), "{bad:?}");
        }
        assert!(check_file_name("console").is_ok() && check_file_name("com10").is_ok());
    }

    #[test]
    #[ignore = "benchmark"]
    fn bench_snapshot() {
        use std::fmt::Write as _;
        let mut s = String::from(
            "world { \"id\" \"1\" \"classname\" \"worldspawn\"
",
        );
        for i in 0..50_000u32 {
            let (x, y) = (f64::from(i % 250) * 64.0, f64::from(i / 250) * 64.0);
            let (x1, y1) = (x + 32.0, y + 32.0);
            let _ = write!(s, "solid {{ \"id\" \"{}\"", i + 2);
            for (k, p) in [
                format!("({x} {y1} 32) ({x1} {y1} 32) ({x1} {y} 32)"),
                format!("({x} {y} 0) ({x1} {y} 0) ({x1} {y1} 0)"),
                format!("({x} {y1} 32) ({x} {y} 32) ({x} {y} 0)"),
                format!("({x1} {y1} 0) ({x1} {y} 0) ({x1} {y} 32)"),
                format!("({x1} {y1} 32) ({x} {y1} 32) ({x} {y1} 0)"),
                format!("({x1} {y} 0) ({x} {y} 0) ({x} {y} 32)"),
            ]
            .iter()
            .enumerate()
            {
                let _ = write!(
                    s,
                    " side {{ \"id\" \"{}\" \"plane\" \"{p}\" \"material\" \"BRICK/A\" }}",
                    i * 6 + k as u32 + 1
                );
            }
            s.push_str(
                " }
",
            );
        }
        s.push_str(
            "}
",
        );
        let d = Document::from_map(hp_vmf::parse(&s).expect("parse"));
        for _ in 0..3 {
            let t = std::time::Instant::now();
            let snap = d.compile_snapshot(&CompileExport::default());
            let t1 = t.elapsed();
            let fp = crate::fingerprint::fingerprint_map(&snap.map);
            eprintln!("snapshot of 50000 solids: {t1:?}, fingerprint {:?} ({})", t.elapsed() - t1, fp.detail);
        }
    }
}
