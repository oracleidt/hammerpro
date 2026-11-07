//! Transient model-particle buffers, separate from selectable scene models.

use crate::arena::GrowBuffer;
use crate::gpu::InstanceGpu;
use crate::pipelines::{MeshPass, PipelineSet};
use crate::{FrameCounter, ModelInstance, Renderer};

#[derive(Default)]
pub(crate) struct ModelParticles {
    buffer: Option<GrowBuffer>,
    runs: Vec<(u32, u32, u32, bool)>,
    count: u32,
}

impl Renderer {
    pub fn set_particle_models(&mut self, models: &[ModelInstance]) {
        self.upload_particle_models(false, models);
        self.content_gen += 1;
    }

    pub fn particle_model_count(&self) -> u32 {
        self.particle_models.count
    }

    pub(crate) fn upload_particle_models(&mut self, preview: bool, models: &[ModelInstance]) {
        let mut set =
            std::mem::take(if preview { &mut self.preview_particle_models } else { &mut self.particle_models });
        set.runs.clear();
        let limit = (self.max_buffer / size_of::<InstanceGpu>() as u64) as usize;
        let points = self.model_cube_points();
        let mut data = Vec::with_capacity(models.len().min(limit));
        for instance in models.iter().take(limit) {
            let Some(model) = self.models.get(&instance.model.0) else { continue };
            if !instance.transform.iter().flatten().chain(&instance.tint).all(|v| v.is_finite())
                || instance.tint[3] <= 0.0
            {
                continue;
            }
            let [m0, m1, m2, m3] = instance.transform;
            let start = data.len() as u32;
            data.push(InstanceGpu {
                m0,
                m1,
                m2,
                m3,
                tint: instance.tint,
                bmin: model.aabb_min.to_array(),
                bmax: model.aabb_max.to_array(),
                slot: u32::MAX,
                fade: [0.0; 2],
                light: u32::MAX,
                cube: u32::from(self.cubes.model_cubemap(&points, glam::Vec3::from_slice(&m3[..3]))),
            });
            let faded = instance.tint[3] < 1.0;
            match set.runs.last_mut() {
                Some((id, _, count, alpha)) if *id == instance.model.0 && *alpha == faded => *count += 1,
                _ => set.runs.push((instance.model.0, start, 1, faded)),
            }
        }
        set.count = data.len() as u32;
        if !data.is_empty() {
            let bytes = bytemuck::cast_slice(&data);
            let buffer = set.buffer.get_or_insert_with(|| {
                GrowBuffer::new(&self.device, "hp-render model particles", wgpu::BufferUsages::VERTEX, 4096)
            });
            let _ = buffer.ensure(&self.device, &self.queue, bytes.len() as u64, self.max_buffer, false);
            if bytes.len() as u64 <= buffer.size {
                self.queue.write_buffer(&buffer.buffer, 0, bytes);
            } else {
                set.count = 0;
                set.runs.clear();
            }
        }
        *(if preview { &mut self.preview_particle_models } else { &mut self.particle_models }) = set;
    }

    pub(crate) fn draw_particle_models(
        &self,
        set: &ModelParticles,
        pass: &mut wgpu::RenderPass<'_>,
        pipes: &PipelineSet,
        n: &mut FrameCounter,
    ) {
        self.draw_particle_models_range(set, pass, pipes, 0..u32::MAX, n);
    }

    pub(crate) fn draw_particle_models_range(
        &self,
        set: &ModelParticles,
        pass: &mut wgpu::RenderPass<'_>,
        pipes: &PipelineSet,
        range: std::ops::Range<u32>,
        n: &mut FrameCounter,
    ) {
        let Some(buffer) = set.buffer.as_ref().filter(|_| set.count > 0) else { return };
        pass.set_vertex_buffer(1, buffer.buffer.slice(..));
        // Keep authored order. Only adjacent instances of the same model merge.
        for &(id, first, count, faded) in &set.runs {
            let end = (first + count).min(range.end);
            let first = first.max(range.start);
            if end <= first {
                continue;
            }
            let count = end - first;
            let Some(model) = self.models.get(&id) else { continue };
            pass.set_vertex_buffer(0, model.vbuf.slice(..));
            pass.set_vertex_buffer(2, model.tbuf.slice(..));
            pass.set_index_buffer(model.ibuf.slice(..), wgpu::IndexFormat::Uint32);
            for part in &model.parts {
                let desc = self.scene.materials.get(part.material as usize).and_then(|m| m.desc);
                let Some((kind, two)) = crate::classify_with(desc.as_ref(), false, false) else { continue };
                let pipe = if faded && matches!(kind, MeshPass::Opaque | MeshPass::AlphaTest) {
                    &pipes.model_faded[usize::from(kind == MeshPass::AlphaTest)][usize::from(two)]
                } else {
                    &pipes.model[kind as usize][usize::from(two)]
                };
                pass.set_pipeline(pipe);
                if let Some(material) = self.mat_gpu.get(part.material as usize) {
                    pass.set_bind_group(1, &material.bind_group, &[]);
                    n.material();
                }
                pass.draw_indexed(part.first..part.first + part.count, 0, first..first + count);
                n.model(u64::from(part.count / 3) * u64::from(count), count);
            }
        }
    }
}
