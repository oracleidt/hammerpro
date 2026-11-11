use std::collections::HashMap;
use std::hash::{Hash, Hasher};
use std::sync::Arc;
use std::time::{Duration, Instant};

use glam::DVec3;
use hp_geom::{Aabb, CLIP_EPS, NORMAL_EPS, Plane, SolidGeom, texture_axes_valid, validate_solid};
use hp_vmf::{Entity, Side, TexAxis};
use rayon::prelude::*;

use crate::glob::normalize_material;
use crate::ids::{IdSet, id_set};
use crate::scene::{EntityNode, Owner, Renumbered, SolidNode};
use crate::{Document, Id, IdMap, ObjRef};

const CHUNK: usize = 2048;
const MAX_LIGHTMAP_DIM: f64 = 32.0;

#[derive(Default)]
pub(crate) struct FastHasher(u64);

impl FastHasher {
    #[inline]
    fn add(&mut self, w: u64) {
        self.0 = (self.0.rotate_left(5) ^ w).wrapping_mul(0x51_7c_c1_b7_27_22_0a_95);
    }
}

impl Hasher for FastHasher {
    fn write(&mut self, bytes: &[u8]) {
        let mut chunks = bytes.chunks_exact(8);
        for c in &mut chunks {
            self.add(u64::from_le_bytes([c[0], c[1], c[2], c[3], c[4], c[5], c[6], c[7]]));
        }
        let rest = chunks.remainder();
        if !rest.is_empty() {
            let mut b = [0u8; 8];
            b[..rest.len()].copy_from_slice(rest);
            self.add(u64::from_le_bytes(b) ^ ((rest.len() as u64) << 56));
        }
    }
    fn write_u8(&mut self, i: u8) {
        self.add(u64::from(i));
    }
    fn write_u32(&mut self, i: u32) {
        self.add(u64::from(i));
    }
    fn write_u64(&mut self, i: u64) {
        self.add(i);
    }
    fn write_usize(&mut self, i: usize) {
        self.add(i as u64);
    }
    fn finish(&self) -> u64 {
        self.0
    }
}

fn hash_f64(h: &mut FastHasher, v: f64) {
    h.add((v + 0.0).to_bits());
}

fn hash_vec(h: &mut FastHasher, v: DVec3) {
    hash_f64(h, v.x);
    hash_f64(h, v.y);
    hash_f64(h, v.z);
}

fn hash_axis(h: &mut FastHasher, a: &TexAxis) {
    hash_vec(h, a.axis);
    hash_f64(h, a.shift);
    hash_f64(h, a.scale);
}

#[derive(Clone, Debug, PartialEq)]
pub struct SideRec {
    pub id: Id,
    pub material: u32,
    pub disp: bool,
    pub texinfo: u64,
    pub plane: u64,
    pub luxels: u32,
}

#[derive(Clone, Debug)]
pub struct SolidRec {
    pub id: Id,
    pub owner: Owner,
    pub geom: Arc<SolidGeom>,
    pub bounds: Aabb,
    pub sides: Box<[SideRec]>,
    pub group: Option<Id>,
    pub visgroups: Box<[Id]>,
    pub invalid_reason: Option<String>,
    pub dup_planes: Box<[u16]>,
    pub bad_axes: Box<[u16]>,
    pub disp_bad: Box<[u16]>,
    pub volume: f64,
    pub bevels: u8,
    pub axial: bool,
    pub shape: u64,
}

impl SolidRec {
    pub fn has_disp(&self) -> bool {
        self.sides.iter().any(|s| s.disp)
    }
}

#[derive(Clone, Debug, PartialEq)]
pub struct EntityRec {
    pub entity: Entity,
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct MaterialName {
    pub key: String,
    pub display: String,
}

#[derive(Clone, Debug, Default)]
pub struct MapSnapshot {
    pub content_serial: u64,
    pub world: Arc<Entity>,
    pub solids: Vec<Arc<SolidRec>>,
    pub entities: Vec<Arc<EntityRec>>,
    pub entity_solids: Vec<u32>,
    pub hidden_solids: IdSet,
    pub hidden_entities: IdSet,
    pub groups: IdSet,
    pub visgroups: IdSet,
    pub renumbered: (Vec<ObjRef>, usize),
    pub materials: Arc<Vec<MaterialName>>,
}

impl MapSnapshot {
    pub fn capture(doc: &Document) -> MapSnapshot {
        let mut b = SnapshotBuilder::default();
        loop {
            if let Some(s) = b.step(doc, Duration::MAX) {
                return s;
            }
        }
    }

    pub fn material_key(&self, id: u32) -> &str {
        self.materials.get(id as usize).map_or("", |m| m.key.as_str())
    }

    pub fn material_display(&self, id: u32) -> &str {
        self.materials.get(id as usize).map_or("", |m| m.display.as_str())
    }

    pub fn is_visible(&self, obj: ObjRef) -> bool {
        match obj {
            ObjRef::Solid(id) => !self.hidden_solids.contains(&id),
            ObjRef::Entity(id) => !self.hidden_entities.contains(&id),
        }
    }

    pub fn is_empty(&self) -> bool {
        self.solids.is_empty() && self.entities.is_empty()
    }
}

fn solid_fingerprint(n: &SolidNode) -> u64 {
    let mut h = FastHasher::default();
    h.write_u32(n.solid.id);
    match n.owner {
        Owner::World => h.write_u32(0),
        Owner::Entity(e) => {
            h.write_u32(1);
            h.write_u32(e);
        }
    }
    h.write_u32(n.solid.editor.group_id.unwrap_or(0));
    for v in &n.solid.editor.visgroup_ids {
        h.write_u32(*v);
    }
    h.write_usize(n.solid.sides.len());
    for s in &n.solid.sides {
        h.write_u32(s.id);
        for p in s.plane {
            hash_vec(&mut h, p);
        }
        s.material.hash(&mut h);
        hash_axis(&mut h, &s.uaxis);
        hash_axis(&mut h, &s.vaxis);
        h.write_u32(s.lightmap_scale as u32);
        match &s.disp {
            Some(d) => h.write_u32(1 + d.power),
            None => h.write_u32(0),
        }
    }
    h.write_usize(Arc::as_ptr(&n.geom) as usize);
    for d in n.disp.iter().flatten() {
        h.write_usize(Arc::as_ptr(d) as usize);
    }
    h.finish()
}

fn entity_fingerprint(n: &EntityNode) -> u64 {
    let mut h = FastHasher::default();
    let e = &n.entity;
    h.write_u32(e.id);
    e.classname.hash(&mut h);
    for (k, v) in &e.props {
        k.hash(&mut h);
        v.hash(&mut h);
    }
    for c in &e.connections {
        c.output.hash(&mut h);
        c.target.hash(&mut h);
        c.input.hash(&mut h);
        c.param.hash(&mut h);
        h.write_u32(c.delay.to_bits());
        h.write_u32(c.times as u32);
    }
    h.write_u32(e.editor.group_id.unwrap_or(0));
    for v in &e.editor.visgroup_ids {
        h.write_u32(*v);
    }
    h.write_u8(u8::from(e.hidden));
    h.finish()
}

fn plane_key(p: &Plane) -> u64 {
    let n = p.normal;
    let positive = n.x > 1e-9 || (n.x.abs() <= 1e-9 && (n.y > 1e-9 || (n.y.abs() <= 1e-9 && n.z > 0.0)));
    let (n, d) = if positive { (n, p.dist) } else { (-n, -p.dist) };
    let mut h = FastHasher::default();
    for c in [n.x, n.y, n.z] {
        h.add((c * 1e5).round() as i64 as u64);
    }
    h.add((d * 100.0).round() as i64 as u64);
    h.finish() | 1
}

fn face_luxels(vertices: &[DVec3], side: &Side) -> u32 {
    let scale = if side.lightmap_scale > 0 { f64::from(side.lightmap_scale) } else { 16.0 };
    let (Some(u), Some(v)) = (side.uaxis.axis.try_normalize(), side.vaxis.axis.try_normalize()) else {
        return 0;
    };
    let (mut s0, mut s1, mut t0, mut t1) = (f64::MAX, f64::MIN, f64::MAX, f64::MIN);
    for p in vertices {
        let (s, t) = (p.dot(u) / scale, p.dot(v) / scale);
        s0 = s0.min(s);
        s1 = s1.max(s);
        t0 = t0.min(t);
        t1 = t1.max(t);
    }
    if !(s1 >= s0 && t1 >= t0) {
        return 0;
    }
    let (w, h) = ((s1.ceil() - s0.floor()).max(1.0), (t1.ceil() - t0.floor()).max(1.0));
    let pieces = |d: f64| (d / MAX_LIGHTMAP_DIM).ceil().max(1.0);
    let luxels = (w + pieces(w)) * (h + pieces(h));
    if luxels.is_finite() { luxels.min(f64::from(u32::MAX)) as u32 } else { 0 }
}

type EntityStep = (Id, u64, Option<Arc<EntityRec>>, u32, bool);

struct Draft<'a> {
    rec: SolidRec,
    materials: Vec<std::borrow::Cow<'a, str>>,
}

fn solid_draft(n: &SolidNode) -> Draft<'_> {
    let geom = &n.geom;
    let sides = &n.solid.sides;
    let planes: Vec<Option<Plane>> = sides.iter().map(Plane::from_side).collect();
    let mut dup_planes = Vec::new();
    let mut bad_axes = Vec::new();
    let mut disp_bad = Vec::new();
    let mut recs = Vec::with_capacity(sides.len());
    let mut materials = Vec::with_capacity(sides.len());
    for (i, (s, p)) in sides.iter().zip(&planes).enumerate() {
        if let Some(p) = p {
            if planes.iter().take(i).flatten().any(|e| p.approx_eq(e, NORMAL_EPS, CLIP_EPS)) {
                dup_planes.push(i as u16);
            }
            if !texture_axes_valid(s, p.normal) {
                bad_axes.push(i as u16);
            }
        }
        let face = geom.face_for_side(i);
        let bad_power = s.disp.as_ref().is_some_and(|d| !(1..=4).contains(&d.power));
        if (s.disp.is_some() && (bad_power || face.is_none_or(|f| f.vertices.len() != 4)))
            || crate::ops::disp::unreadable_disp(s)
        {
            disp_bad.push(i as u16);
        }
        let name = normalize_material(&s.material);
        let mut th = FastHasher::default();
        th.write(name.to_ascii_lowercase().as_bytes());
        hash_axis(&mut th, &s.uaxis);
        hash_axis(&mut th, &s.vaxis);
        th.write_u32(s.lightmap_scale as u32);
        recs.push(SideRec {
            id: s.id,
            material: 0,
            disp: s.disp.is_some(),
            texinfo: th.finish(),
            plane: p.as_ref().map_or(0, plane_key),
            luxels: face.map_or(0, |f| face_luxels(&f.vertices, s)),
        });
        materials.push(name);
    }
    let mut axial_dirs = [false; 6];
    let mut axial = true;
    for f in &geom.faces {
        let nrm = f.plane.normal;
        let mut hit = false;
        for k in 0..3 {
            if (nrm[k].abs() - 1.0).abs() < 1e-9 {
                axial_dirs[k * 2 + usize::from(nrm[k] > 0.0)] = true;
                hit = true;
            }
        }
        axial &= hit;
    }
    let bevels = axial_dirs.iter().filter(|d| !**d).count() as u8;
    let invalid_reason = (!geom.valid)
        .then(|| validate_solid(sides).err().map_or_else(|| "invalid geometry".to_owned(), |e| e.to_string()));
    let rec = SolidRec {
        id: n.solid.id,
        owner: n.owner,
        geom: Arc::clone(geom),
        bounds: n.bounds,
        sides: recs.into_boxed_slice(),
        group: n.solid.editor.group_id,
        visgroups: n.solid.editor.visgroup_ids.clone().into_boxed_slice(),
        invalid_reason,
        dup_planes: dup_planes.into_boxed_slice(),
        bad_axes: bad_axes.into_boxed_slice(),
        disp_bad: disp_bad.into_boxed_slice(),
        volume: if geom.valid { geom.volume() } else { 0.0 },
        bevels: if geom.valid { bevels } else { 0 },
        axial: geom.valid && axial,
        shape: crate::duplicates::node_shape(n),
    };
    Draft { rec, materials }
}

#[derive(Debug, Default)]
struct MaterialTable {
    ids: HashMap<String, u32>,
    names: Arc<Vec<MaterialName>>,
}

impl MaterialTable {
    fn intern(&mut self, name: &str) -> u32 {
        let key = name.to_ascii_lowercase();
        if let Some(&id) = self.ids.get(&key) {
            return id;
        }
        let id = self.names.len() as u32;
        Arc::make_mut(&mut self.names).push(MaterialName { key: key.clone(), display: name.to_owned() });
        self.ids.insert(key, id);
        id
    }
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
struct PassKey {
    content: u64,
    visibility: u64,
    solids: usize,
    entities: usize,
}

fn pass_key(doc: &Document) -> PassKey {
    let mut h = FastHasher::default();
    for v in doc.scene().hidden_visgroups() {
        h.write_u32(*v);
    }
    PassKey {
        content: doc.content_serial(),
        visibility: h.finish(),
        solids: doc.scene().solids().len(),
        entities: doc.scene().entities().len(),
    }
}

#[derive(Debug)]
struct Pass {
    key: PassKey,
    solid_pos: usize,
    entity_pos: usize,
    solids: Vec<Arc<SolidRec>>,
    entities: Vec<Arc<EntityRec>>,
    entity_solids: Vec<u32>,
    hidden_solids: IdSet,
    hidden_entities: IdSet,
}

#[derive(Debug, Default)]
pub struct SnapshotBuilder {
    solids: IdMap<(u64, Arc<SolidRec>)>,
    entities: IdMap<(u64, Arc<EntityRec>)>,
    materials: MaterialTable,
    pass: Option<Pass>,
    rebuilt: usize,
}

impl SnapshotBuilder {
    pub fn new() -> Self {
        Self::default()
    }

    pub fn last_rebuilt(&self) -> usize {
        self.rebuilt
    }

    pub fn in_progress(&self) -> bool {
        self.pass.is_some()
    }

    pub fn restart(&mut self) {
        self.pass = None;
    }

    pub fn step(&mut self, doc: &Document, budget: Duration) -> Option<MapSnapshot> {
        let start = Instant::now();
        let key = pass_key(doc);
        if self.pass.as_ref().is_none_or(|p| p.key != key) {
            self.pass = Some(Pass {
                key,
                solid_pos: 0,
                entity_pos: 0,
                solids: Vec::with_capacity(key.solids),
                entities: Vec::with_capacity(key.entities),
                entity_solids: Vec::with_capacity(key.entities),
                hidden_solids: id_set(0),
                hidden_entities: id_set(0),
            });
            self.rebuilt = 0;
        }
        let scene = doc.scene();
        loop {
            let pass = self.pass.as_mut()?;
            if pass.solid_pos < key.solids {
                let end = (pass.solid_pos + CHUNK).min(key.solids);
                let cache = &self.solids;
                let results: Vec<(Id, u64, Option<Draft<'_>>, bool)> = (pass.solid_pos..end)
                    .into_par_iter()
                    .filter_map(|i| {
                        let (&id, n) = scene.solids.get_index(i)?;
                        let fp = solid_fingerprint(n);
                        let fresh = cache.get(&id).is_some_and(|(f, _)| *f == fp);
                        let draft = (!fresh).then(|| solid_draft(n));
                        Some((id, fp, draft, doc.is_solid_visible(id)))
                    })
                    .collect();
                for (id, fp, draft, visible) in results {
                    let rec = match draft {
                        Some(Draft { mut rec, materials }) => {
                            for (s, m) in rec.sides.iter_mut().zip(&materials) {
                                s.material = self.materials.intern(m);
                            }
                            let rec = Arc::new(rec);
                            self.solids.insert(id, (fp, Arc::clone(&rec)));
                            self.rebuilt += 1;
                            rec
                        }
                        None => match self.solids.get(&id) {
                            Some((_, r)) => Arc::clone(r),
                            None => continue,
                        },
                    };
                    pass.solids.push(rec);
                    if !visible {
                        pass.hidden_solids.insert(id);
                    }
                }
                pass.solid_pos = end;
            } else if pass.entity_pos < key.entities {
                let end = (pass.entity_pos + CHUNK).min(key.entities);
                let cache = &self.entities;
                let results: Vec<EntityStep> = (pass.entity_pos..end)
                    .into_par_iter()
                    .filter_map(|i| {
                        let (&id, n) = scene.entities.get_index(i)?;
                        let fp = entity_fingerprint(n);
                        let fresh = cache.get(&id).is_some_and(|(f, _)| *f == fp);
                        let rec = (!fresh).then(|| Arc::new(EntityRec { entity: n.entity.clone() }));
                        let solids = scene.entity_solid_count(id) as u32;
                        Some((id, fp, rec, solids, doc.is_entity_visible(id)))
                    })
                    .collect();
                for (id, fp, rec, solids, visible) in results {
                    let rec = match rec {
                        Some(r) => {
                            self.entities.insert(id, (fp, Arc::clone(&r)));
                            self.rebuilt += 1;
                            r
                        }
                        None => match self.entities.get(&id) {
                            Some((_, r)) => Arc::clone(r),
                            None => continue,
                        },
                    };
                    pass.entities.push(rec);
                    pass.entity_solids.push(solids);
                    if !visible {
                        pass.hidden_entities.insert(id);
                    }
                }
                pass.entity_pos = end;
            } else {
                break;
            }
            if start.elapsed() >= budget {
                return None;
            }
        }
        let pass = self.pass.take()?;
        if self.solids.len() != pass.solids.len() {
            let keep: IdSet = pass.solids.iter().map(|r| r.id).collect();
            self.solids.retain(|id, _| keep.contains(id));
        }
        if self.entities.len() != pass.entities.len() {
            let keep: IdSet = pass.entities.iter().map(|r| r.entity.id).collect();
            self.entities.retain(|id, _| keep.contains(id));
        }
        let mut groups = id_set(scene.groups().len());
        groups.extend(scene.groups().keys().copied());
        let mut visgroups = id_set(0);
        let mut stack: Vec<&hp_vmf::VisGroup> = scene.visgroups().iter().collect();
        while let Some(v) = stack.pop() {
            visgroups.insert(v.id);
            stack.extend(v.children.iter());
        }
        let renumbered: Vec<ObjRef> = doc
            .renumbered
            .iter()
            .filter_map(|r| match *r {
                Renumbered::Solid { new, .. } => Some(ObjRef::Solid(new)),
                Renumbered::Entity { new, .. } => Some(ObjRef::Entity(new)),
                Renumbered::Group { .. } => None,
            })
            .filter(|o| scene.contains(*o))
            .collect();
        let mut world = scene.world().clone();
        world.solids.clear();
        world.groups.clear();
        Some(MapSnapshot {
            content_serial: key.content,
            world: Arc::new(world),
            solids: pass.solids,
            entities: pass.entities,
            entity_solids: pass.entity_solids,
            hidden_solids: pass.hidden_solids,
            hidden_entities: pass.hidden_entities,
            groups,
            visgroups,
            renumbered: (renumbered, doc.renumbered.len()),
            materials: Arc::clone(&self.materials.names),
        })
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::tests::box_sides;

    fn doc() -> Document {
        let mut doc = Document::new_empty();
        for i in 0..10 {
            let o = DVec3::new(f64::from(i) * 100.0, 0.0, 0.0);
            doc.create_solid(box_sides(o, o + DVec3::splat(64.0), "BRICK/WALL"), Owner::World).expect("solid");
        }
        doc.create_point_entity("info_player_start", DVec3::new(32.0, 32.0, 100.0), &[]).expect("entity");
        doc
    }

    #[test]
    fn records_are_reused_until_an_object_changes() {
        let mut doc = doc();
        let mut b = SnapshotBuilder::new();
        let s1 = b.step(&doc, Duration::MAX).expect("snapshot");
        assert_eq!(s1.solids.len(), 10);
        assert_eq!(s1.entities.len(), 1);
        assert_eq!(b.last_rebuilt(), 11);
        assert_eq!(s1.materials.len(), 1);
        assert_eq!(s1.material_display(s1.solids[0].sides[0].material), "BRICK/WALL");
        assert_eq!(s1.material_key(s1.solids[0].sides[0].material), "brick/wall");
        let s2 = b.step(&doc, Duration::MAX).expect("snapshot");
        assert_eq!(b.last_rebuilt(), 0);
        assert!(Arc::ptr_eq(&s1.solids[3], &s2.solids[3]));
        let id = s1.solids[3].id;
        doc.apply_material(&[crate::FaceRef::new(id, 0)], "CONCRETE/FLOOR").expect("material");
        let s3 = b.step(&doc, Duration::MAX).expect("snapshot");
        assert_eq!(b.last_rebuilt(), 1);
        assert!(!Arc::ptr_eq(&s1.solids[3], &s3.solids[3]));
        assert_eq!(s3.material_key(s3.solids[3].sides[0].material), "concrete/floor");
        doc.delete(&[ObjRef::Solid(id)]).expect("delete");
        let other = s1.solids[0].id;
        doc.set_hidden(&[ObjRef::Solid(other)], true);
        let s4 = b.step(&doc, Duration::MAX).expect("snapshot");
        assert_eq!(s4.solids.len(), 9);
        assert!(!s4.is_visible(ObjRef::Solid(other)));
        assert!(s4.is_visible(ObjRef::Solid(s1.solids[1].id)));
    }

    #[test]
    fn a_tiny_budget_takes_several_steps_and_restarts_on_edits() {
        let mut doc = doc();
        let mut b = SnapshotBuilder::new();
        let mut steps = 0;
        let snap = loop {
            steps += 1;
            if let Some(s) = b.step(&doc, Duration::ZERO) {
                break s;
            }
            assert!(steps < 100);
        };
        assert_eq!(snap.solids.len(), 10);
        assert!(b.step(&doc, Duration::ZERO).is_none());
        assert!(b.in_progress());
        doc.create_point_entity("light", DVec3::new(0.0, 0.0, 10.0), &[]).expect("entity");
        let snap = loop {
            if let Some(s) = b.step(&doc, Duration::ZERO) {
                break s;
            }
        };
        assert_eq!(snap.entities.len(), 2);
        assert_eq!(snap.content_serial, doc.content_serial());
    }

    #[test]
    fn side_facts() {
        let doc = doc();
        let snap = MapSnapshot::capture(&doc);
        let s = &snap.solids[0];
        assert!(s.axial);
        assert_eq!(s.bevels, 0);
        assert!((s.volume - 64.0 * 64.0 * 64.0).abs() < 1e-6);
        let planes: std::collections::HashSet<u64> = s.sides.iter().map(|x| x.plane).collect();
        assert_eq!(planes.len(), 6);
        assert_eq!(s.sides[0].luxels, (4 + 1) * (4 + 1));
        assert!(s.dup_planes.is_empty() && s.bad_axes.is_empty() && s.disp_bad.is_empty());
        assert_eq!(snap.world.classname, "worldspawn");
    }

    #[test]
    fn plane_keys_ignore_orientation() {
        let a = Plane::new(DVec3::X, 10.0).expect("plane");
        let b = Plane { normal: -a.normal, dist: -a.dist };
        assert_eq!(plane_key(&a), plane_key(&b));
        let c = Plane::new(DVec3::X, 10.5).expect("plane");
        assert_ne!(plane_key(&a), plane_key(&c));
    }
}
