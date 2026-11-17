use glam::Vec3;

use crate::gpu::{self, GpuVertex};
use crate::{ModelId, Renderer, Vertex, tangents};

pub(crate) fn skinned_vertices(vertices: &[Vertex], range: (f32, f32)) -> Vec<GpuVertex> {
    let (slot, blend) = (range.0.to_bits(), range.1.to_bits());
    vertices
        .iter()
        .map(|v| {
            let p = Vec3::from(v.pos);
            GpuVertex {
                pos: if p.is_finite() { v.pos } else { [0.0; 3] },
                uv: if v.uv[0].is_finite() && v.uv[1].is_finite() { v.uv } else { [0.0; 2] },
                nrm_alpha: gpu::pack_normal_alpha(v.normal, v.alpha),
                slot,
                blend,
                cube: u32::from(crate::NO_CUBEMAP),
            }
        })
        .collect()
}

impl Renderer {
    pub fn model_vertex_count(&self, id: ModelId) -> usize {
        self.models.get(&id.0).map_or(0, |m| (m.vbuf.size() as usize) / size_of::<GpuVertex>())
    }

    pub fn update_model_vertices(&mut self, id: ModelId, vertices: &[Vertex], tangents: &[[f32; 4]]) -> bool {
        let Some(model) = self.models.get(&id.0) else { return false };
        if model.parts.is_empty() || vertices.is_empty() {
            return false;
        }
        let count = (model.vbuf.size() as usize) / size_of::<GpuVertex>();
        if vertices.len() != count {
            return false;
        }
        let range = model.lod_ranges.first().copied().unwrap_or((0.0, 0.0));
        let packed = skinned_vertices(vertices, range);
        let vdata: &[u8] = bytemuck::cast_slice(&packed);
        self.queue.write_buffer(&model.vbuf, 0, vdata);
        let mut bytes = vdata.len() as u64;
        if tangents.len() == vertices.len() && model.tbuf.size() as usize >= tangents.len() * 4 {
            let packed: Vec<u32> = tangents.iter().map(|t| tangents::pack_tangent(*t)).collect();
            let tdata: &[u8] = bytemuck::cast_slice(&packed);
            self.queue.write_buffer(&model.tbuf, 0, tdata);
            bytes += tdata.len() as u64;
        }
        self.staging.wrote(&self.device, &self.queue, bytes);
        true
    }

    pub fn set_model_bounds(&mut self, id: ModelId, min: [f32; 3], max: [f32; 3]) {
        let (lo, hi) = (Vec3::from(min), Vec3::from(max));
        if !(lo.is_finite() && hi.is_finite() && lo.cmple(hi).all()) {
            return;
        }
        if let Some(model) = self.models.get_mut(&id.0)
            && (model.aabb_min != lo || model.aabb_max != hi)
        {
            model.aabb_min = lo;
            model.aabb_max = hi;
            self.content_gen += 1;
        }
    }

    pub fn model_animation_generation(&self) -> u64 {
        self.model_anim_gen
    }

    pub fn note_animated_models(&mut self, shadows: bool) {
        self.model_anim_gen += 1;
        if shadows {
            self.shadow_epoch += 1;
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn skinned_vertices_keep_the_lod_range_and_drop_bad_values() {
        let v = [
            Vertex { pos: [1.0, 2.0, 3.0], normal: [0.0, 0.0, 1.0], uv: [0.5, 0.25], alpha: 1.0 },
            Vertex { pos: [f32::NAN, 0.0, 0.0], normal: [0.0, 0.0, 1.0], uv: [f32::INFINITY, 0.0], alpha: 1.0 },
        ];
        let out = skinned_vertices(&v, (0.0, 512.0));
        assert_eq!(out.len(), 2);
        assert_eq!(out[0].pos, [1.0, 2.0, 3.0]);
        assert_eq!(out[0].slot, 0.0f32.to_bits());
        assert_eq!(out[0].blend, 512.0f32.to_bits());
        assert_eq!(out[1].pos, [0.0; 3]);
        assert_eq!(out[1].uv, [0.0; 2]);
        assert_eq!(out[1].cube, u32::from(crate::NO_CUBEMAP));
    }
}
