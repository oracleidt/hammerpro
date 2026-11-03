use crate::gpu::GpuVertex;
use crate::pipelines::{CHUNK_ATTRS, LDR_FORMAT, Shared, model_mesh_buffers, vertex_layout};

const OUTLINE_WGSL: &str = concat!(include_str!("shaders/common.wgsl"), "\n", include_str!("shaders/outline.wgsl"));
const MASK_FORMAT: wgpu::TextureFormat = wgpu::TextureFormat::R8Unorm;

pub(crate) struct OutlinePipelines {
    pub mask_chunk: wgpu::RenderPipeline,
    pub mask_model: wgpu::RenderPipeline,
    pub outline: wgpu::RenderPipeline,
    bgl: wgpu::BindGroupLayout,
}

pub(crate) struct OutlineTarget {
    pub generation: u64,
    _texture: wgpu::Texture,
    pub view: wgpu::TextureView,
    pub bind: wgpu::BindGroup,
}

impl OutlinePipelines {
    pub fn new(device: &wgpu::Device, shared: &Shared) -> Self {
        let bgl = device.create_bind_group_layout(&wgpu::BindGroupLayoutDescriptor {
            label: Some("hp-render outline mask"),
            entries: &[wgpu::BindGroupLayoutEntry {
                binding: 0,
                visibility: wgpu::ShaderStages::FRAGMENT,
                ty: wgpu::BindingType::Texture {
                    sample_type: wgpu::TextureSampleType::Float { filterable: false },
                    view_dimension: wgpu::TextureViewDimension::D2,
                    multisampled: false,
                },
                count: None,
            }],
        });
        let outline_layout = device.create_pipeline_layout(&wgpu::PipelineLayoutDescriptor {
            label: Some("hp-render outline"),
            bind_group_layouts: &[Some(&shared.view_bgl), Some(&bgl)],
            immediate_size: 0,
        });
        let module = device.create_shader_module(wgpu::ShaderModuleDescriptor {
            label: Some("hp-render outline"),
            source: wgpu::ShaderSource::Wgsl(OUTLINE_WGSL.into()),
        });
        let chunk_vb = [Some(vertex_layout(size_of::<GpuVertex>(), wgpu::VertexStepMode::Vertex, &CHUNK_ATTRS))];
        let model_vb = model_mesh_buffers();
        let constants: &[(&str, f64)] = &[("SELECTED_ONLY", 1.0)];
        let mask = |vs: &str, buffers: &[Option<wgpu::VertexBufferLayout<'_>>]| {
            device.create_render_pipeline(&wgpu::RenderPipelineDescriptor {
                label: Some("hp-render outline mask"),
                layout: Some(&shared.layout_material),
                vertex: wgpu::VertexState {
                    module: &shared.mesh,
                    entry_point: Some(vs),
                    compilation_options: wgpu::PipelineCompilationOptions {
                        constants,
                        zero_initialize_workgroup_memory: false,
                    },
                    buffers,
                },
                primitive: wgpu::PrimitiveState::default(),
                depth_stencil: None,
                multisample: wgpu::MultisampleState::default(),
                fragment: Some(wgpu::FragmentState {
                    module: &shared.mesh,
                    entry_point: Some("fs_mask"),
                    compilation_options: wgpu::PipelineCompilationOptions {
                        constants,
                        zero_initialize_workgroup_memory: false,
                    },
                    targets: &[Some(wgpu::ColorTargetState {
                        format: MASK_FORMAT,
                        blend: None,
                        write_mask: wgpu::ColorWrites::ALL,
                    })],
                }),
                multiview_mask: None,
                cache: None,
            })
        };
        let outline = device.create_render_pipeline(&wgpu::RenderPipelineDescriptor {
            label: Some("hp-render outline"),
            layout: Some(&outline_layout),
            vertex: wgpu::VertexState {
                module: &module,
                entry_point: Some("vs_outline"),
                compilation_options: wgpu::PipelineCompilationOptions::default(),
                buffers: &[],
            },
            primitive: wgpu::PrimitiveState::default(),
            depth_stencil: None,
            multisample: wgpu::MultisampleState::default(),
            fragment: Some(wgpu::FragmentState {
                module: &module,
                entry_point: Some("fs_outline"),
                compilation_options: wgpu::PipelineCompilationOptions::default(),
                targets: &[Some(wgpu::ColorTargetState {
                    format: LDR_FORMAT,
                    blend: None,
                    write_mask: wgpu::ColorWrites::ALL,
                })],
            }),
            multiview_mask: None,
            cache: None,
        });
        Self { mask_chunk: mask("vs_chunk", &chunk_vb), mask_model: mask("vs_model", &model_vb), outline, bgl }
    }

    pub fn target(&self, device: &wgpu::Device, size: [u32; 2], generation: u64) -> OutlineTarget {
        let texture = device.create_texture(&wgpu::TextureDescriptor {
            label: Some("hp-render outline mask"),
            size: wgpu::Extent3d { width: size[0].max(1), height: size[1].max(1), depth_or_array_layers: 1 },
            mip_level_count: 1,
            sample_count: 1,
            dimension: wgpu::TextureDimension::D2,
            format: MASK_FORMAT,
            usage: wgpu::TextureUsages::RENDER_ATTACHMENT | wgpu::TextureUsages::TEXTURE_BINDING,
            view_formats: &[],
        });
        let view = texture.create_view(&wgpu::TextureViewDescriptor::default());
        let bind = device.create_bind_group(&wgpu::BindGroupDescriptor {
            label: Some("hp-render outline mask"),
            layout: &self.bgl,
            entries: &[wgpu::BindGroupEntry { binding: 0, resource: wgpu::BindingResource::TextureView(&view) }],
        });
        OutlineTarget { generation, _texture: texture, view, bind }
    }
}

#[cfg(test)]
mod tests {
    #[test]
    fn shader_is_clean() {
        assert!(super::OUTLINE_WGSL.contains("fn fs_outline"));
        assert!(!super::OUTLINE_WGSL.contains('\u{2014}') && !super::OUTLINE_WGSL.contains('\u{2013}'));
    }
}
