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

#[derive(Debug, Default)]
pub struct SelectionRuntime {
    pub modal_transform: bool,
    pub edges: Vec<EdgeRef>,
    pub vertices: Vec<(Id, DVec3)>,
    pub element_revision: u64,
    pub snapshots: Vec<PickSnapshot>,
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
}
