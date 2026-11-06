use std::collections::HashMap;

use super::*;

struct Target {
    generation: u64,
    copy: wgpu::Texture,
    bind: wgpu::BindGroup,
}

pub(super) struct Refraction {
    layouts: [wgpu::BindGroupLayout; 2],
    pipes: [wgpu::RenderPipeline; 2],
    targets: HashMap<u32, Target>,
}

fn source(msaa: bool) -> String {
    let depth = if msaa {
        "@group(2) @binding(1) var heat_depth: texture_depth_multisampled_2d;\n\
         fn heat_z(p: vec2<i32>) -> f32 {\n\
           var z = 0.0;\n\
           for (var i = 0; i < 4; i += 1) { z = max(z, textureLoad(heat_depth, p, i)); }\n\
           return z;\n\
         }"
    } else {
        "@group(2) @binding(1) var heat_depth: texture_depth_2d;\n\
         fn heat_z(p: vec2<i32>) -> f32 { return textureLoad(heat_depth, p, 0); }"
    };
    format!("{PARTICLE_WGSL}\n{depth}\n{}", include_str!("../shaders/particle_refract.wgsl"))
}

impl Refraction {
    fn new(device: &wgpu::Device, shared: &Shared) -> Self {
        let layouts = std::array::from_fn(|i| {
            device.create_bind_group_layout(&wgpu::BindGroupLayoutDescriptor {
                label: Some("hp-render particle refraction"),
                entries: &[
                    wgpu::BindGroupLayoutEntry {
                        binding: 0,
                        visibility: wgpu::ShaderStages::FRAGMENT,
                        ty: wgpu::BindingType::Texture {
                            sample_type: wgpu::TextureSampleType::Float { filterable: true },
                            view_dimension: wgpu::TextureViewDimension::D2,
                            multisampled: false,
                        },
                        count: None,
                    },
                    wgpu::BindGroupLayoutEntry {
                        binding: 1,
                        visibility: wgpu::ShaderStages::FRAGMENT,
                        ty: wgpu::BindingType::Texture {
                            sample_type: wgpu::TextureSampleType::Depth,
                            view_dimension: wgpu::TextureViewDimension::D2,
                            multisampled: i > 0,
                        },
                        count: None,
                    },
                ],
            })
        });
        let pipes = std::array::from_fn(|i| {
            let module = device.create_shader_module(wgpu::ShaderModuleDescriptor {
                label: Some("hp-render particle refraction"),
                source: wgpu::ShaderSource::Wgsl(source(i > 0).into()),
            });
            let layout = device.create_pipeline_layout(&wgpu::PipelineLayoutDescriptor {
                label: Some("hp-render particle refraction"),
                bind_group_layouts: &[Some(&shared.view_bgl), Some(&shared.material_bgl), Some(&layouts[i])],
                immediate_size: 0,
            });
            pipelines::build_color_only(
                device,
                Desc {
                    label: "hp-render particle refraction",
                    module: &module,
                    vs: "vs_particle",
                    fs: "fs_particle_refract",
                    layout: &layout,
                    buffers: &[Some(vertex_layout(size_of::<ParticleGpu>(), wgpu::VertexStepMode::Instance, &ATTRS))],
                    topology: wgpu::PrimitiveTopology::TriangleList,
                    cull: None,
                    depth_test: false,
                    depth_write: false,
                    blend: Blend::Alpha,
                    constants: &[],
                },
            )
        });
        Self { layouts, pipes, targets: HashMap::new() }
    }
}

impl Renderer {
    pub(crate) fn prepare_particle_refraction(&mut self, view_id: u32) -> bool {
        if !self.particles.runs.iter().any(|r| r.blend == ParticleBlend::Refract) {
            return false;
        }
        let Some(view) = self.views.get(&view_id) else { return false };
        let state = self.particles.refraction.get_or_insert_with(|| Refraction::new(&self.device, &self.shared));
        if state.targets.get(&view_id).is_none_or(|t| t.generation != view.generation) {
            let copy = self.device.create_texture(&wgpu::TextureDescriptor {
                label: Some("hp-render particle scene copy"),
                size: wgpu::Extent3d { width: view.size[0], height: view.size[1], depth_or_array_layers: 1 },
                mip_level_count: 1,
                sample_count: 1,
                dimension: wgpu::TextureDimension::D2,
                format: pipelines::COLOR_FORMAT,
                usage: wgpu::TextureUsages::COPY_DST | wgpu::TextureUsages::TEXTURE_BINDING,
                view_formats: &[],
            });
            let copy_view = copy.create_view(&wgpu::TextureViewDescriptor::default());
            let bind = self.device.create_bind_group(&wgpu::BindGroupDescriptor {
                label: Some("hp-render particle scene copy"),
                layout: &state.layouts[usize::from(view.samples > 1)],
                entries: &[
                    wgpu::BindGroupEntry { binding: 0, resource: wgpu::BindingResource::TextureView(&copy_view) },
                    wgpu::BindGroupEntry { binding: 1, resource: wgpu::BindingResource::TextureView(&view.depth_view) },
                ],
            });
            state.targets.insert(view_id, Target { generation: view.generation, copy, bind });
        }
        true
    }

    pub(crate) fn record_particle_refraction(
        &self,
        enc: &mut wgpu::CommandEncoder,
        view_id: u32,
        n: &mut FrameCounter,
    ) {
        let (Some(state), Some(buffer), Some(view)) =
            (&self.particles.refraction, &self.particles.buffer, self.views.get(&view_id))
        else {
            return;
        };
        let (Some(target), Some(common)) = (state.targets.get(&view_id), &view.bind_group) else { return };
        enc.copy_texture_to_texture(
            view.color.as_image_copy(),
            target.copy.as_image_copy(),
            wgpu::Extent3d { width: view.size[0], height: view.size[1], depth_or_array_layers: 1 },
        );
        let mut pass = enc.begin_render_pass(&wgpu::RenderPassDescriptor {
            label: Some("hp-render particle refraction"),
            color_attachments: &[Some(wgpu::RenderPassColorAttachment {
                view: &view.color_view,
                depth_slice: None,
                resolve_target: None,
                ops: wgpu::Operations { load: wgpu::LoadOp::Load, store: wgpu::StoreOp::Store },
            })],
            depth_stencil_attachment: None,
            timestamp_writes: None,
            occlusion_query_set: None,
            multiview_mask: None,
        });
        pass.set_pipeline(&state.pipes[usize::from(view.samples > 1)]);
        pass.set_bind_group(0, common, &[]);
        pass.set_bind_group(2, &target.bind, &[]);
        pass.set_vertex_buffer(0, buffer.buffer.slice(..));
        for run in self.particles.runs.iter().filter(|r| r.blend == ParticleBlend::Refract) {
            let Some(mat) = self.scene.find_material(run.material).and_then(|s| self.mat_gpu.get(s as usize)) else {
                continue;
            };
            pass.set_bind_group(1, &mat.bind_group, &[]);
            pass.draw(0..6, run.start..run.start + run.count);
            n.add(u64::from(run.count) * 2);
            n.counts.sprites += run.count;
        }
    }

    pub(crate) fn remove_particle_refraction_view(&mut self, view_id: u32) {
        if let Some(state) = self.particles.refraction.as_mut() {
            state.targets.remove(&view_id);
        }
    }
}
