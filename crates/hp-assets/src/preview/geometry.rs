use std::sync::Arc;

use ahash::AHashMap;
use hp_app::state::{Gfx, PropPreviewRequest};
use hp_editor::prop_export::PropGeometry;
use hp_render::{MeshBatch, ObjectFlags, ObjectMesh, RenderMode, Vertex, ViewKind};

use super::{AssetManager, Preview, WIRE_OBJECT};

const VIEW: u32 = 1;
const OBJECT: u32 = 3;
type Stamp = (u64, ViewKind, [u32; 2], RenderMode, [f32; 3]);

#[derive(Default)]
pub(super) struct GeometryPreview {
    data: Option<Arc<PropGeometry>>,
    last: Option<Stamp>,
    texture: Option<(egui::TextureId, u64)>,
}

impl GeometryPreview {
    pub(super) fn redraw(&mut self) {
        self.last = None;
    }

    pub(super) fn invalidate(&mut self) {
        self.data = None;
        self.redraw();
    }
}

impl AssetManager {
    pub(crate) fn prop_preview_impl(
        &mut self,
        gfx: &mut Gfx,
        request: &PropPreviewRequest<'_>,
    ) -> Option<egui::TextureId> {
        if request.geometry.tris.is_empty() {
            return None;
        }
        self.note_gpu(&gfx.renderer);
        self.poll_results();
        if self.preview.is_none() {
            self.preview = Some(Preview::new(gfx));
        }
        let rebuild =
            self.preview.as_ref()?.geometry.data.as_ref().is_none_or(|old| !Arc::ptr_eq(old, request.geometry));
        if rebuild {
            let mut batches: Vec<MeshBatch> = Vec::new();
            let mut materials = AHashMap::new();
            let mut bounds = hp_geom::Aabb::EMPTY;
            let mut edges = Vec::new();
            for triangle in &request.geometry.tris {
                let index = *materials.entry(&triangle.material).or_insert_with(|| {
                    let index = batches.len();
                    batches
                        .push(MeshBatch { material: self.preview_material(&triangle.material), ..Default::default() });
                    index
                });
                let batch = &mut batches[index];
                let first = batch.vertices.len() as u32;
                for corner in 0..3 {
                    let position = triangle.pos[corner];
                    bounds.extend(position);
                    batch.vertices.push(Vertex {
                        pos: position.as_vec3().to_array(),
                        normal: triangle.normal[corner].as_vec3().to_array(),
                        uv: triangle.uv[corner].map(|v| v as f32),
                        alpha: triangle.disp.map_or(0.0, |disp| disp.alpha[corner] as f32),
                    });
                    edges.extend([position.as_vec3().to_array(), triangle.pos[(corner + 1) % 3].as_vec3().to_array()]);
                }
                batch.indices.extend([first, first + 1, first + 2]);
            }
            let preview = self.preview.as_mut()?;
            preview.renderer.upsert_object(
                OBJECT,
                ObjectMesh {
                    batches,
                    edges,
                    aabb_min: bounds.min.as_vec3().to_array(),
                    aabb_max: bounds.max.as_vec3().to_array(),
                    ..Default::default()
                },
            );
            preview.renderer.set_object_flags(OBJECT, ObjectFlags::HIDDEN);
            preview.geometry.data = Some(Arc::clone(request.geometry));
            preview.geometry.last = None;
        }
        self.preview_sync_normal_maps();
        self.preview_apply_dirty();
        let preview = self.preview.as_mut()?;
        let stamp = (
            request.revision,
            request.params.kind,
            request.params.size,
            request.params.mode,
            request.params.background,
        );
        if preview.needs_render || preview.geometry.last != Some(stamp) {
            let wire = preview.renderer.object_flags(WIRE_OBJECT);
            preview.renderer.set_object_flags(WIRE_OBJECT, ObjectFlags::HIDDEN);
            preview.renderer.set_model_instances(&[]);
            preview.renderer.set_object_flags(OBJECT, ObjectFlags::NO_2D | ObjectFlags::NO_SHADOW);
            preview.renderer.render_view(VIEW, request.params, request.overlay);
            // Restore the shared model-browser scene immediately. Each preview has
            // its own render target, so opening both cannot replace either image.
            preview.renderer.set_object_flags(OBJECT, ObjectFlags::HIDDEN);
            if let Some(flags) = wire {
                preview.renderer.set_object_flags(WIRE_OBJECT, flags);
            }
            preview.restore_after_thumbnail();
            preview.geometry.last = Some(stamp);
            preview.needs_render = false;
            preview.last = None;
        }
        let generation = preview.renderer.view_texture_generation(VIEW);
        let view = preview.renderer.view_texture(VIEW)?;
        match preview.geometry.texture {
            Some((id, old)) if old == generation => Some(id),
            Some((id, _)) => {
                gfx.update_texture(id, view);
                preview.geometry.texture = Some((id, generation));
                Some(id)
            }
            None => {
                let id = gfx.register_texture(view);
                preview.geometry.texture = Some((id, generation));
                Some(id)
            }
        }
    }
}
