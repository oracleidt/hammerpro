use std::sync::Arc;

use hp_editor::Id;
use hp_render::{ObjectFlags, ObjectMesh};

use crate::producer::{DerivedBuild, DerivedObject, DerivedProducer, ProducerChanges, ProducerCx};

#[derive(Default)]
pub(crate) struct ToolPreview {
    mesh: Option<Arc<ObjectMesh>>,
    revision: u64,
    pub hidden: Vec<Id>,
    pub restore: Vec<Id>,
}

impl ToolPreview {
    pub fn set(&mut self, mesh: Option<Arc<ObjectMesh>>, hidden: &[Id]) {
        let same = match (&self.mesh, &mesh) {
            (Some(a), Some(b)) => Arc::ptr_eq(a, b),
            (None, None) => true,
            _ => false,
        };
        if same && self.hidden == hidden {
            return;
        }
        let hidden = if mesh.is_some() { hidden } else { &[] };
        if self.hidden != hidden {
            self.restore.extend(self.hidden.iter().copied());
            self.hidden = hidden.to_vec();
        }
        self.mesh = mesh;
        self.revision = self.revision.wrapping_add(1);
    }
}

#[derive(Default)]
pub(crate) struct PreviewProducer {
    revision: Option<u64>,
}

impl DerivedProducer for PreviewProducer {
    fn name(&self) -> &'static str {
        "tool_preview"
    }

    fn changed(&mut self, changes: &ProducerChanges<'_>, cx: &ProducerCx<'_>) -> bool {
        let revision = cx.assets.scene.tool_preview.revision;
        let changed = self.revision.replace(revision) != Some(revision);
        changes.full || changed
    }

    fn build(&mut self, cx: &mut ProducerCx<'_>) -> DerivedBuild {
        let mut result = DerivedBuild { clear: true, ..Default::default() };
        if let Some(mesh) = cx.assets.scene.tool_preview.mesh.clone() {
            for batch in &mesh.batches {
                cx.assets.touch_material(batch.material);
            }
            result.upsert.push(DerivedObject {
                key: 0,
                mesh: (*mesh).clone(),
                owner: None,
                flags: ObjectFlags::NO_2D | ObjectFlags::NO_SHADOW,
            });
        }
        result
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn clearing_a_preview_restores_hidden_brushes_and_identical_frames_do_not_rebuild() {
        let mut preview = ToolPreview::default();
        let mesh = Arc::new(ObjectMesh::default());
        preview.set(Some(mesh.clone()), &[3, 4]);
        let revision = preview.revision;
        preview.set(Some(mesh), &[3, 4]);
        assert_eq!(preview.revision, revision);
        preview.set(None, &[]);
        assert!(preview.mesh.is_none());
        assert!(preview.hidden.is_empty());
        assert_eq!(preview.restore, [3, 4]);
        assert_ne!(preview.revision, revision);
    }
}
