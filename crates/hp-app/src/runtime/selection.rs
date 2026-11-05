use glam::DVec3;
use hp_editor::{EdgeRef, Id};

#[derive(Debug, Default)]
pub struct SelectionRuntime {
    pub modal_transform: bool,
    pub edges: Vec<EdgeRef>,
    pub vertices: Vec<(Id, DVec3)>,
    pub element_revision: u64,
}

impl SelectionRuntime {
    pub fn clear_elements(&mut self) {
        self.edges.clear();
        self.vertices.clear();
    }

    pub fn fresh_edges(&self, revision: u64) -> &[EdgeRef] {
        if self.element_revision == revision { &self.edges } else { &[] }
    }

    pub fn fresh_vertices(&self, revision: u64) -> &[(Id, DVec3)] {
        if self.element_revision == revision { &self.vertices } else { &[] }
    }
}
