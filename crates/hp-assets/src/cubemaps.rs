use std::sync::Arc;

use ahash::{AHashMap, AHashSet};
use glam::{DVec3, Vec3};
use hp_editor::{Document, Id};
use hp_render::{CubemapId, CubemapProbe, NO_CUBEMAP, Renderer};

#[derive(Clone, Debug, Default, PartialEq)]
pub(crate) struct CubeAssign {
    points: Vec<(Vec3, u16)>,
    manual: AHashMap<u32, u16>,
}

impl CubeAssign {
    pub(crate) fn face(&self, side_id: u32, center: DVec3, normal: DVec3) -> u16 {
        if let Some(&c) = self.manual.get(&side_id) {
            return c;
        }
        nearest_in_front(&self.points, center.as_vec3(), normal.as_vec3()).unwrap_or(NO_CUBEMAP)
    }

    pub(crate) fn of_face(&self, side: &hp_vmf::Side, face: &hp_geom::FaceGeom) -> u16 {
        self.face(side.id, face.center(), face.normal())
    }

    pub(crate) fn solid_faces(&self, node: &hp_editor::SolidNode) -> Vec<u16> {
        node.geom
            .faces
            .iter()
            .map(|f| node.solid.sides.get(f.side_index).map_or(NO_CUBEMAP, |s| self.of_face(s, f)))
            .collect()
    }
}

pub(crate) fn nearest_in_front(points: &[(Vec3, u16)], center: Vec3, normal: Vec3) -> Option<u16> {
    let mut best: Option<(f32, u16)> = None;
    for &(o, id) in points {
        let d = o - center;
        let dist = d.length();
        let dir = if dist > 1e-10 { d / dist } else { Vec3::ZERO };
        if dir.dot(normal) >= 0.0 && best.is_none_or(|b| dist < b.0) {
            best = Some((dist, id));
        }
    }
    if best.is_some() {
        return best.map(|b| b.1);
    }
    for &(o, id) in points {
        let dist = o.distance(center);
        if best.is_none_or(|b| dist < b.0) {
            best = Some((dist, id));
        }
    }
    best.map(|b| b.1)
}

#[derive(Clone, Debug, PartialEq)]
pub(crate) struct ProbeDef {
    pub origin: [i32; 3],
    pub size: u32,
    pub sides: Vec<u32>,
}

fn atoi(s: &str) -> i64 {
    let s = s.trim_start();
    let (neg, digits) = match s.strip_prefix('-') {
        Some(r) => (true, r),
        None => (false, s.strip_prefix('+').unwrap_or(s)),
    };
    let mut v: i64 = 0;
    for c in digits.chars() {
        let Some(d) = c.to_digit(10) else { break };
        v = (v * 10 + i64::from(d)).min(1 << 40);
    }
    if neg { -v } else { v }
}

pub(crate) fn probe_def(e: &hp_vmf::Entity) -> Option<ProbeDef> {
    if !e.classname.trim().eq_ignore_ascii_case("env_cubemap") {
        return None;
    }
    let mut origin = [0i32; 3];
    for (k, part) in e.get("origin").unwrap_or("").split_whitespace().take(3).enumerate() {
        let v = part.parse::<f64>().unwrap_or(0.0);
        origin[k] = if v.is_finite() { v.clamp(-1.0e9, 1.0e9) as i32 } else { 0 };
    }
    let n = atoi(e.get("cubemapsize").unwrap_or("0"));
    let size = if (1..=16).contains(&n) { 1u32 << (n - 1) } else { 0 };
    let sides = e
        .get("sides")
        .unwrap_or("")
        .split_whitespace()
        .filter_map(|s| s.parse::<i64>().ok())
        .map(|v| v as u32)
        .collect();
    Some(ProbeDef { origin, size, sides })
}

struct Probe {
    id: CubemapId,
    def: ProbeDef,
}

#[derive(Default)]
pub(crate) struct EditorCubes {
    probes: AHashMap<Id, Probe>,
    assign: Arc<CubeAssign>,
    revision: u64,
}

impl EditorCubes {
    pub(crate) fn revision(&self) -> u64 {
        self.revision
    }

    pub(crate) fn probe_views(&self) -> Vec<([f32; 3], u32)> {
        self.probes
            .values()
            .map(|p| {
                let size = if p.def.size == 0 { hp_render::DEFAULT_PROBE_SIZE } else { p.def.size };
                (p.def.origin.map(|v| v as f32), size.clamp(1, hp_render::MAX_CUBEMAP_RESOLUTION))
            })
            .collect()
    }

    pub(crate) fn assign(&self) -> Arc<CubeAssign> {
        Arc::clone(&self.assign)
    }

    pub(crate) fn probe_at(&self, origin: [i32; 3]) -> Option<CubemapId> {
        self.probes.values().filter(|p| p.def.origin == origin).map(|p| p.id).min()
    }

    pub(crate) fn sync(&mut self, doc: &Document, renderer: &mut Renderer, entities: Option<&[Id]>) -> bool {
        let scene = doc.scene();
        let mut wanted: AHashMap<Id, ProbeDef> = AHashMap::new();
        let look = |id: Id, wanted: &mut AHashMap<Id, ProbeDef>| {
            if let Some(n) = scene.entity(id)
                && doc.is_entity_visible(id)
                && let Some(def) = probe_def(&n.entity)
            {
                wanted.insert(id, def);
            }
        };
        let mut gone: Vec<Id> = Vec::new();
        match entities {
            None => {
                for id in scene.entities().keys() {
                    look(*id, &mut wanted);
                }
                gone.extend(self.probes.keys().filter(|id| !wanted.contains_key(id)).copied());
            }
            Some(list) => {
                for &id in list {
                    look(id, &mut wanted);
                    if !wanted.contains_key(&id) && self.probes.contains_key(&id) {
                        gone.push(id);
                    }
                }
            }
        }
        let mut changed = false;
        for id in gone {
            if let Some(p) = self.probes.remove(&id) {
                renderer.destroy_cubemap(p.id);
                changed = true;
            }
        }
        let mut need_sides = false;
        for (id, def) in wanted {
            if self.probes.get(&id).is_some_and(|p| p.def == def) {
                continue;
            }
            need_sides |= !def.sides.is_empty();
            match self.probes.get_mut(&id) {
                Some(p) => {
                    p.def = def;
                    changed = true;
                }
                None => {
                    let Some(cid) = renderer.create_cubemap_probe(&CubemapProbe {
                        origin: def.origin.map(|v| v as f32),
                        size: def.size,
                        sides: Vec::new(),
                    }) else {
                        log::warn!("env_cubemap {id}: too many cubemaps, left out");
                        continue;
                    };
                    self.probes.insert(id, Probe { id: cid, def });
                    changed = true;
                }
            }
        }
        if !changed {
            return false;
        }
        self.revision = self.revision.wrapping_add(1);
        let sides = if need_sides || self.probes.values().any(|p| !p.def.sides.is_empty()) {
            side_planes(doc, self.probes.values().flat_map(|p| p.def.sides.iter().copied()).collect())
        } else {
            AHashMap::new()
        };
        for p in self.probes.values() {
            let probe = CubemapProbe {
                origin: p.def.origin.map(|v| v as f32),
                size: p.def.size,
                sides: p.def.sides.iter().filter_map(|s| sides.get(s).copied()).collect(),
            };
            renderer.update_cubemap_probe(p.id, &probe);
        }
        let assign = self.build_assign();
        if *self.assign != assign {
            self.assign = Arc::new(assign);
        }
        true
    }

    pub(crate) fn sync_parallax(&mut self, doc: &Document, renderer: &mut Renderer) {
        let boxes: AHashMap<Id, hp_render::parallax::ParallaxBox> =
            crate::parallax::boxes_of_document(doc).into_iter().collect();
        for (id, p) in &self.probes {
            renderer.set_cubemap_parallax(p.id, boxes.get(id).copied());
        }
    }

    fn build_assign(&self) -> CubeAssign {
        let mut probes: Vec<(&Id, &Probe)> = self.probes.iter().collect();
        probes.sort_by_key(|(id, _)| **id);
        let mut a = CubeAssign::default();
        for (_, p) in &probes {
            a.points
                .push((Vec3::new(p.def.origin[0] as f32, p.def.origin[1] as f32, p.def.origin[2] as f32), p.id.raw()));
            for s in &p.def.sides {
                a.manual.entry(*s).or_insert(p.id.raw());
            }
        }
        a
    }
}

fn side_planes(doc: &Document, ids: AHashSet<u32>) -> AHashMap<u32, (u32, [f32; 4])> {
    let mut out = AHashMap::new();
    if ids.is_empty() {
        return out;
    }
    for (sid, n) in doc.scene().solids() {
        for f in &n.geom.faces {
            let Some(side) = n.solid.sides.get(f.side_index) else { continue };
            if ids.contains(&side.id) {
                let normal = f.normal();
                let d = normal.dot(f.center());
                out.insert(side.id, (*sid, [normal.x as f32, normal.y as f32, normal.z as f32, d as f32]));
            }
        }
    }
    out
}

#[cfg(test)]
impl CubeAssign {
    pub(crate) fn for_tests(points: &[([f32; 3], u16)]) -> Self {
        CubeAssign { points: points.iter().map(|(o, c)| (Vec3::from(*o), *c)).collect(), manual: AHashMap::new() }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn faces_take_the_nearest_probe_in_front_like_vbsp() {
        let a = CubeAssign {
            points: vec![
                (Vec3::new(0.0, 0.0, 100.0), 1),
                (Vec3::new(0.0, 0.0, -20.0), 2),
                (Vec3::new(500.0, 0.0, 10.0), 3),
            ],
            manual: AHashMap::from_iter([(77, 3)]),
        };
        assert_eq!(a.face(1, DVec3::ZERO, DVec3::Z), 1);
        assert_eq!(a.face(1, DVec3::ZERO, -DVec3::Z), 2);
        assert_eq!(a.face(1, DVec3::new(400.0, 0.0, 0.0), DVec3::X), 3);
        assert_eq!(a.face(77, DVec3::ZERO, DVec3::Z), 3);
        let b = CubeAssign {
            points: vec![(Vec3::new(-50.0, 0.0, 0.0), 7), (Vec3::new(-10.0, 0.0, 0.0), 8)],
            ..Default::default()
        };
        assert_eq!(b.face(1, DVec3::ZERO, DVec3::X), 8);
        assert_eq!(CubeAssign::default().face(1, DVec3::ZERO, DVec3::Z), NO_CUBEMAP);
    }

    #[test]
    fn env_cubemap_keys_read_like_the_compiler() {
        let mut e = hp_vmf::Entity { classname: "env_cubemap".into(), ..Default::default() };
        e.set("origin", "10.9 -3.7 64");
        e.set("cubemapsize", "6");
        e.set("sides", "12 x 40 -2");
        let d = probe_def(&e).unwrap();
        assert_eq!(d.origin, [10, -3, 64], "truncated toward zero");
        assert_eq!(d.size, 32);
        assert_eq!(d.sides, vec![12, 40, u32::MAX - 1]);
        e.set("cubemapsize", "0");
        assert_eq!(probe_def(&e).unwrap().size, 0, "the default size");
        e.set("cubemapsize", "garbage");
        assert_eq!(probe_def(&e).unwrap().size, 0);
        let light = hp_vmf::Entity { classname: "light".into(), ..Default::default() };
        assert!(probe_def(&light).is_none());
    }
}
