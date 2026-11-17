use ahash::{AHashMap, AHashSet};
use glam::{DVec2, DVec3};
use hp_app::runtime::helpers::scene_display;
use hp_editor::side_index::overlay::{
    self, OverlayFragment, OverlayShape, TransitionShape, is_overlay_class, is_transition_class,
};
use hp_editor::{Document, FaceRef, Id, IdSet};
use hp_fgd::ResolvedClass;
use hp_geom::Aabb;
use hp_render::{MaterialId, MeshBatch, ObjectFlags, ObjectMesh, Vertex};
use hp_vmf::Entity;
use rayon::prelude::*;

use crate::entities::{class_of, point_color};
use crate::material::MatKind;
use crate::producer::{DerivedBuild, DerivedObject, DerivedProducer, ProducerChanges, ProducerCx};
use crate::util::material_key;

const SURFACE_OFFSET: f64 = 0.2;
const ORDER_STEP: f64 = 0.1;
const MISSING_SIZE: DVec2 = DVec2::new(64.0, 64.0);
const PARALLEL_OVERLAYS: usize = 16;

pub(crate) enum OverlayKind {
    Quad(OverlayShape),
    Transition(TransitionShape),
}

impl OverlayKind {
    fn sides(&self) -> Vec<Id> {
        match self {
            OverlayKind::Quad(s) => s.sides.clone(),
            OverlayKind::Transition(t) => t.all_sides().collect(),
        }
    }

    fn material(&self) -> &str {
        match self {
            OverlayKind::Quad(s) => &s.material,
            OverlayKind::Transition(t) => &t.material,
        }
    }

    fn render_order(&self) -> u8 {
        match self {
            OverlayKind::Quad(s) => s.render_order,
            OverlayKind::Transition(_) => 0,
        }
    }
}

pub(crate) fn overlay_kind(e: &Entity, class: Option<&ResolvedClass>) -> Option<OverlayKind> {
    let transition = is_transition_class(&e.classname) || class.is_some_and(|c| c.has_helper("overlay_transition"));
    if transition {
        return TransitionShape::from_entity(e).map(OverlayKind::Transition);
    }
    let quad = is_overlay_class(&e.classname) || class.is_some_and(|c| c.has_helper("overlay"));
    quad.then(|| OverlayShape::from_entity(e)).flatten().map(OverlayKind::Quad)
}

pub(crate) fn fragments_mesh(
    frags: &[OverlayFragment],
    render_order: u8,
    tex: DVec2,
    material: MaterialId,
    wire: [u8; 3],
) -> ObjectMesh {
    let offset = SURFACE_OFFSET + ORDER_STEP * f64::from(render_order.min(3));
    let mut vertices = Vec::new();
    let mut indices = Vec::new();
    let mut bounds = Aabb::empty();
    for f in frags {
        let base = u32::try_from(vertices.len()).unwrap_or(u32::MAX);
        let normal = f.normal.as_vec3().to_array();
        for (p, t) in f.points.iter().zip(&f.texcoords) {
            let p = *p + f.normal * offset;
            bounds.extend(p);
            let uv = (*t * tex).as_vec2().to_array();
            vertices.push(Vertex { pos: p.as_vec3().to_array(), normal, uv, alpha: 0.0 });
        }
        hp_geom::triangulate_fan_into(base, f.points.len(), &mut indices);
    }
    let lift = |p: DVec3, n: DVec3| (p + n * offset).as_vec3().to_array();
    let mut edges = Vec::new();
    let normal_of = |p: DVec3| {
        frags.iter().find(|f| f.points.iter().any(|q| (*q - p).length_squared() < 1e-6)).map_or(DVec3::Z, |f| f.normal)
    };
    for (a, b) in overlay::outline(frags) {
        let n = normal_of(a);
        edges.push(lift(a, n));
        edges.push(lift(b, n));
    }
    let (aabb_min, aabb_max) = if bounds.is_empty() {
        ([0.0; 3], [0.0; 3])
    } else {
        (bounds.min.as_vec3().to_array(), bounds.max.as_vec3().to_array())
    };
    ObjectMesh {
        batches: vec![MeshBatch { material, vertices, indices, blend: Vec::new(), cubemaps: Vec::new() }],
        edges,
        aabb_min,
        aabb_max,
        wire_color: wire,
        ..Default::default()
    }
}

struct OverlayState {
    sides: Vec<Id>,
    pending: bool,
}

#[derive(Default)]
pub(crate) struct OverlayProducer {
    overlays: AHashMap<Id, OverlayState>,
    dirty: AHashSet<Id>,
    wanted: AHashMap<Id, Vec<Id>>,
    faces: AHashMap<Id, FaceRef>,
    solid_sides: AHashMap<Id, Vec<Id>>,
    missing: AHashSet<Id>,
    shown: Option<bool>,
    clear: bool,
}

impl OverlayProducer {
    fn reset(&mut self) {
        self.overlays.clear();
        self.dirty.clear();
        self.wanted.clear();
        self.faces.clear();
        self.solid_sides.clear();
        self.missing.clear();
    }

    fn update_faces(&mut self, doc: &Document, changed: &IdSet, removed: &IdSet) {
        if self.wanted.is_empty() {
            return;
        }
        let mut touched: Vec<Id> = Vec::new();
        for solid in changed.iter().chain(removed) {
            if let Some(old) = self.solid_sides.remove(solid) {
                for s in old {
                    if self.faces.get(&s).is_some_and(|f| f.solid == *solid) {
                        self.faces.remove(&s);
                    }
                    touched.push(s);
                }
            }
        }
        let scene = doc.scene();
        for solid in changed {
            let Some(node) = scene.solid(*solid) else { continue };
            for (i, side) in node.solid.sides.iter().enumerate() {
                if self.wanted.contains_key(&side.id) {
                    self.faces.insert(side.id, FaceRef::new(*solid, i));
                    self.missing.remove(&side.id);
                    self.solid_sides.entry(*solid).or_default().push(side.id);
                    touched.push(side.id);
                }
            }
        }
        for s in touched {
            if let Some(list) = self.wanted.get(&s) {
                self.dirty.extend(list.iter().copied());
            }
        }
    }

    fn unlink(&mut self, id: Id, sides: &[Id]) {
        for s in sides {
            if let Some(list) = self.wanted.get_mut(s) {
                list.retain(|o| *o != id);
                if list.is_empty() {
                    self.wanted.remove(s);
                    self.faces.remove(s);
                    self.missing.remove(s);
                }
            }
        }
    }
}

impl DerivedProducer for OverlayProducer {
    fn name(&self) -> &'static str {
        "overlays"
    }

    fn changed(&mut self, changes: &ProducerChanges<'_>, cx: &ProducerCx<'_>) -> bool {
        let shown = scene_display().overlays;
        if changes.full {
            self.reset();
        }
        let mut all = changes.full || changes.entities;
        if self.shown != Some(shown) {
            self.shown = Some(shown);
            if !shown {
                self.reset();
                self.clear = true;
                return true;
            }
            all = true;
        }
        if !shown {
            return self.clear;
        }
        if all {
            self.dirty.extend(cx.doc.scene().entities().keys().copied());
            self.dirty.extend(self.overlays.keys().copied());
        } else {
            let doc = changes.doc;
            self.dirty.extend(doc.entities_changed.iter().copied());
            self.dirty.extend(doc.entities_removed.iter().copied().filter(|id| self.overlays.contains_key(id)));
            if !doc.solids_changed.is_empty() || !doc.solids_removed.is_empty() {
                self.update_faces(cx.doc, &doc.solids_changed, &doc.solids_removed);
            }
        }
        self.dirty.extend(self.overlays.iter().filter(|(_, s)| s.pending).map(|(id, _)| *id));
        !self.dirty.is_empty() || self.clear
    }

    fn build(&mut self, cx: &mut ProducerCx<'_>) -> DerivedBuild {
        let mut out = DerivedBuild::default();
        if std::mem::take(&mut self.clear) {
            out.clear = true;
            if self.shown == Some(false) {
                return out;
            }
        }
        let mut ids: Vec<Id> = self.dirty.drain().collect();
        ids.sort_unstable();
        let fgd = cx.assets.fgd.clone();
        let scene = cx.doc.scene();
        let mut kinds: Vec<(Id, Option<OverlayKind>, [u8; 3])> = Vec::with_capacity(ids.len());
        let mut resolve: IdSet = IdSet::default();
        for id in ids {
            let found = scene.entity(id).and_then(|n| {
                let class = class_of(fgd.as_deref(), &n.entity);
                overlay_kind(&n.entity, class).map(|k| (k, point_color(&n.entity, class)))
            });
            if let Some(old) = self.overlays.remove(&id) {
                self.unlink(id, &old.sides);
            }
            let Some((kind, color)) = found else {
                out.remove.push(u64::from(id));
                continue;
            };
            let sides = kind.sides();
            for s in &sides {
                self.wanted.entry(*s).or_default().push(id);
                if !self.faces.contains_key(s) && !self.missing.contains(s) {
                    resolve.insert(*s);
                }
            }
            self.overlays.insert(id, OverlayState { sides, pending: false });
            kinds.push((id, Some(kind), color));
        }
        if !resolve.is_empty() {
            let found = cx.doc.find_sides(&resolve);
            for s in &resolve {
                match found.get(s) {
                    Some(face) => {
                        self.faces.insert(*s, *face);
                        self.solid_sides.entry(face.solid).or_default().push(*s);
                    }
                    None => {
                        self.missing.insert(*s);
                    }
                }
            }
        }
        let mut jobs: Vec<(Id, OverlayKind, [u8; 3], DVec2, MaterialId)> = Vec::with_capacity(kinds.len());
        for (id, kind, color) in kinds {
            let Some(kind) = kind else { continue };
            let key = material_key(kind.material());
            let tex = match cx.assets.material_info_impl(&key) {
                None => {
                    if let Some(s) = self.overlays.get_mut(&id) {
                        s.pending = true;
                    }
                    continue;
                }
                Some(info) if info.missing || info.width == 0 || info.height == 0 => MISSING_SIZE,
                Some(info) => DVec2::new(f64::from(info.width), f64::from(info.height)),
            };
            let material = cx.assets.asset_material(&key, MatKind::World);
            jobs.push((id, kind, color, tex, material));
        }
        let doc = cx.doc;
        let faces = &self.faces;
        let mesh_of = |(id, kind, color, tex, material): &(Id, OverlayKind, [u8; 3], DVec2, MaterialId)| {
            let lookup = |s: Id| faces.get(&s).copied();
            let frags = match kind {
                OverlayKind::Quad(s) => overlay::fragments(doc, s, &lookup),
                OverlayKind::Transition(t) => overlay::transition_fragments(doc, t, &lookup),
            };
            let mesh =
                (!frags.is_empty()).then(|| fragments_mesh(&frags, kind.render_order(), *tex, *material, *color));
            (*id, mesh)
        };
        let meshes: Vec<(Id, Option<ObjectMesh>)> = if jobs.len() >= PARALLEL_OVERLAYS {
            jobs.par_iter().map(mesh_of).collect()
        } else {
            jobs.iter().map(mesh_of).collect()
        };
        for (id, mesh) in meshes {
            match mesh {
                Some(mesh) => out.upsert.push(DerivedObject {
                    key: u64::from(id),
                    mesh,
                    owner: Some(id),
                    flags: ObjectFlags::NO_GAME_WIRE | ObjectFlags::SURFACE_DECAL | ObjectFlags::NO_SHADOW,
                }),
                None => out.remove.push(u64::from(id)),
            }
        }
        out
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn doc_with_overlay() -> Document {
        let mut state = hp_app::testing::test_state();
        state
            .doc
            .create_point_entity(
                "info_overlay",
                DVec3::new(240.0, 0.0, 0.0),
                &[
                    ("material".to_owned(), "overlays/test".to_owned()),
                    ("sides".to_owned(), "1".to_owned()),
                    ("BasisOrigin".to_owned(), "240 0 0".to_owned()),
                    ("BasisU".to_owned(), "1 0 0".to_owned()),
                    ("BasisV".to_owned(), "0 1 0".to_owned()),
                    ("BasisNormal".to_owned(), "0 0 1".to_owned()),
                    ("uv0".to_owned(), "-32 -16 0".to_owned()),
                    ("uv1".to_owned(), "-32 16 0".to_owned()),
                    ("uv2".to_owned(), "32 16 0".to_owned()),
                    ("uv3".to_owned(), "32 -16 0".to_owned()),
                    ("RenderOrder".to_owned(), "2".to_owned()),
                ],
            )
            .expect("overlay");
        state.doc
    }

    #[test]
    fn kinds_and_mesh() {
        let doc = doc_with_overlay();
        let id = doc.selected_entity_ids()[0];
        let e = &doc.scene().entity(id).expect("entity").entity;
        let Some(OverlayKind::Quad(shape)) = overlay_kind(e, None) else { panic!("overlay") };
        assert_eq!(shape.sides, vec![1]);
        let mut light = e.clone();
        light.classname = "light".to_owned();
        assert!(overlay_kind(&light, None).is_none());
        let found = doc.find_sides(&[1].into_iter().collect());
        let frags = overlay::fragments(&doc, &shape, &|s| found.get(&s).copied());
        let mesh = fragments_mesh(&frags, shape.render_order, DVec2::new(128.0, 64.0), 9, [1, 2, 3]);
        assert_eq!(mesh.batches.len(), 1);
        let b = &mesh.batches[0];
        assert_eq!((b.material, b.vertices.len(), b.indices.len()), (9, 4, 6));
        assert!(b.vertices.iter().all(|v| (v.pos[2] - 0.4).abs() < 1e-5));
        let max_u = b.vertices.iter().map(|v| v.uv[0]).fold(f32::MIN, f32::max);
        assert!((max_u - 96.0).abs() < 1e-3, "{max_u}");
        assert_eq!(mesh.edges.len(), 8, "four outline edges");
        assert_eq!(mesh.wire_color, [1, 2, 3]);
    }
}
