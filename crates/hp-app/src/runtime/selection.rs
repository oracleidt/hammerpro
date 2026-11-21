use std::hash::{Hash, Hasher};

use glam::DVec3;
use hp_editor::{EdgeRef, Id};

const MAX_SNAPSHOTS: usize = 64;

#[derive(Clone, Debug, PartialEq)]
pub struct PickSnapshot {
    pub solid: Id,
    pub key: u64,
    pub edges: Vec<EdgeRef>,
    pub vertices: Vec<DVec3>,
}

#[derive(Clone, Debug, PartialEq)]
pub struct DragTransform {
    pub objs: std::sync::Arc<[hp_editor::ObjRef]>,
    pub matrix: glam::DMat4,
    pub applied: bool,
}

#[derive(Debug, Default)]
pub struct SelectionRuntime {
    pub modal_transform: bool,
    pub typing_angle: bool,
    pub drag: Option<DragTransform>,
    pub edges: Vec<EdgeRef>,
    pub vertices: Vec<(Id, DVec3)>,
    pub element_revision: u64,
    pub snapshots: Vec<PickSnapshot>,
    pub lifted_face: Option<LiftedFace>,
    pub filter: hp_editor::SelectFilter,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct LiftedFace {
    pub face: hp_editor::FaceRef,
    pub side_id: Id,
}

impl SelectionRuntime {
    pub fn clear_elements(&mut self) {
        self.edges.clear();
        self.vertices.clear();
        self.snapshots.clear();
    }

    pub fn fresh_edges(&self, revision: u64) -> &[EdgeRef] {
        if self.element_revision == revision { &self.edges } else { &[] }
    }

    pub fn fresh_vertices(&self, revision: u64) -> &[(Id, DVec3)] {
        if self.element_revision == revision { &self.vertices } else { &[] }
    }

    fn solids(&self) -> Vec<Id> {
        let mut ids: Vec<Id> = self.edges.iter().map(|e| e.solid).chain(self.vertices.iter().map(|v| v.0)).collect();
        ids.sort_unstable();
        ids.dedup();
        ids
    }

    fn remember(&mut self, solid: Id, key: u64) {
        let edges: Vec<EdgeRef> = self.edges.iter().filter(|e| e.solid == solid).copied().collect();
        let vertices: Vec<DVec3> = self.vertices.iter().filter(|v| v.0 == solid).map(|v| v.1).collect();
        self.snapshots.retain(|s| !(s.solid == solid && s.key == key));
        if edges.is_empty() && vertices.is_empty() {
            return;
        }
        self.snapshots.push(PickSnapshot { solid, key, edges, vertices });
        if self.snapshots.len() > MAX_SNAPSHOTS {
            self.snapshots.remove(0);
        }
    }
}

fn geometry_key(doc: &hp_editor::Document, solid: Id) -> Option<u64> {
    let (points, _) = doc.solid_vertices(solid)?;
    let mut q: Vec<[i64; 3]> = points
        .iter()
        .map(|p| [(p.x * 1000.0).round() as i64, (p.y * 1000.0).round() as i64, (p.z * 1000.0).round() as i64])
        .collect();
    q.sort_unstable();
    let mut h = std::collections::hash_map::DefaultHasher::new();
    q.hash(&mut h);
    Some(h.finish())
}

impl crate::AppState {
    pub fn set_pick_element(&mut self, element: crate::settings::SelectElement) {
        use crate::settings::SelectElement;
        self.settings.selection.element = element;
        self.runtime.selection.clear_elements();
        if matches!(element, SelectElement::Edge | SelectElement::Vertex) {
            self.doc.clear_face_selection();
        }
        self.settings_dirty = true;
    }

    pub fn commit_picks(&mut self) {
        let revision = self.doc.revision();
        self.runtime.selection.element_revision = revision;
        for solid in self.runtime.selection.solids() {
            if let Some(key) = geometry_key(&self.doc, solid) {
                self.runtime.selection.remember(solid, key);
            }
        }
    }

    pub fn sync_pick_elements(&mut self) {
        let revision = self.doc.revision();
        let sel = &self.runtime.selection;
        if sel.element_revision == revision || (sel.edges.is_empty() && sel.vertices.is_empty()) {
            return;
        }
        let mut edges: Vec<EdgeRef> = Vec::new();
        let mut vertices: Vec<(Id, DVec3)> = Vec::new();
        for solid in sel.solids() {
            let Some(key) = geometry_key(&self.doc, solid) else { continue };
            if let Some(s) = sel.snapshots.iter().find(|s| s.solid == solid && s.key == key) {
                edges.extend(s.edges.iter().copied());
                vertices.extend(s.vertices.iter().map(|p| (solid, *p)));
            }
        }
        let sel = &mut self.runtime.selection;
        if edges.is_empty() && vertices.is_empty() {
            return;
        }
        sel.edges = edges;
        sel.vertices = vertices;
        sel.element_revision = revision;
    }

    pub fn lift_face(&mut self, face: hp_editor::FaceRef) {
        self.runtime.selection.lifted_face = self.doc.face_side(face).map(|side| LiftedFace { face, side_id: side.id });
    }

    pub fn clear_lifted_face(&mut self) {
        self.runtime.selection.lifted_face = None;
    }

    pub fn lifted_face(&self) -> Option<hp_editor::FaceRef> {
        let lifted = self.runtime.selection.lifted_face?;
        resolve_lifted(&self.doc, lifted)
    }

    pub fn sync_lifted_face(&mut self) {
        let Some(lifted) = self.runtime.selection.lifted_face else { return };
        self.runtime.selection.lifted_face =
            resolve_lifted(&self.doc, lifted).map(|face| LiftedFace { face, side_id: lifted.side_id });
    }
}

fn resolve_lifted(doc: &hp_editor::Document, lifted: LiftedFace) -> Option<hp_editor::FaceRef> {
    if doc.face_side(lifted.face).is_some_and(|s| s.id == lifted.side_id) {
        return Some(lifted.face);
    }
    let sides = &doc.scene().solid(lifted.face.solid)?.solid.sides;
    let index = sides.iter().position(|s| s.id == lifted.side_id)?;
    Some(hp_editor::FaceRef::new(lifted.face.solid, index))
}

#[cfg(test)]
mod tests {
    use hp_editor::FaceRef;

    use super::LiftedFace;

    #[test]
    fn lifted_face_follows_its_side_id() {
        let mut state = crate::testing::test_state();
        let face = FaceRef::new(3, 5);
        state.lift_face(face);
        assert_eq!(state.lifted_face(), Some(face));
        state.runtime.selection.lifted_face = Some(LiftedFace { face: FaceRef::new(3, 0), side_id: 12 });
        assert_eq!(state.lifted_face(), Some(face));
        state.sync_lifted_face();
        assert_eq!(state.runtime.selection.lifted_face, Some(LiftedFace { face, side_id: 12 }));
        state.runtime.selection.lifted_face = Some(LiftedFace { face, side_id: 999 });
        assert_eq!(state.lifted_face(), None);
        state.sync_lifted_face();
        assert_eq!(state.runtime.selection.lifted_face, None);
    }

    #[test]
    fn lifted_face_is_dropped_with_its_document() {
        let mut state = crate::testing::test_state();
        state.lift_face(FaceRef::new(3, 1));
        state.doc = hp_editor::Document::new_empty();
        state.sync_lifted_face();
        assert_eq!(state.lifted_face(), None);
        state.lift_face(FaceRef::new(3, 1));
        assert_eq!(state.runtime.selection.lifted_face, None);
    }
}
