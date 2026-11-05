use super::LightmapDisplay;
use crate::Renderer;
use crate::hooks::{HookContext, PassHook};
use crate::pipelines::COLOR_FORMAT;

const LUT_SIZE: u32 = 32;

const SHADER: &str = r#"
@group(0) @binding(0) var src: texture_2d<f32>;
@group(0) @binding(1) var lut: texture_3d<f32>;
@group(0) @binding(2) var lut_samp: sampler;
@group(0) @binding(3) var zone_lut: texture_3d<f32>;
@group(0) @binding(4) var<uniform> flags: vec4<f32>;

@vertex
fn vs_full(@builtin(vertex_index) i: u32) -> @builtin(position) vec4<f32> {
    let x = f32(i32(i & 1u) * 4 - 1);
    let y = f32(i32(i >> 1u) * 4 - 1);
    return vec4<f32>(x, y, 0.0, 1.0);
}

fn to_srgb(c: vec3<f32>) -> vec3<f32> {
    let lo = c * 12.92;
    let hi = 1.055 * pow(max(c, vec3<f32>(0.0)), vec3<f32>(1.0 / 2.4)) - 0.055;
    return select(hi, lo, c <= vec3<f32>(0.0031308));
}

fn to_linear(c: vec3<f32>) -> vec3<f32> {
    let lo = c / 12.92;
    let hi = pow((c + vec3<f32>(0.055)) / 1.055, vec3<f32>(2.4));
    return select(hi, lo, c <= vec3<f32>(0.04045));
}

fn grid_coord(s: vec3<f32>) -> vec3<f32> {
    return s * (31.0 / 32.0) + vec3<f32>(0.5 / 32.0);
}

@fragment
fn fs_lut(@builtin(position) p: vec4<f32>) -> @location(0) vec4<f32> {
    let c = textureLoad(src, vec2<i32>(p.xy), 0);
    var s = clamp(to_srgb(c.rgb), vec3<f32>(0.0), vec3<f32>(1.0));
    if flags.x > 0.5 {
        s = clamp(textureSampleLevel(lut, lut_samp, grid_coord(s), 0.0).rgb, vec3<f32>(0.0), vec3<f32>(1.0));
    }
    if flags.y > 0.5 {
        s = clamp(textureSampleLevel(zone_lut, lut_samp, grid_coord(s), 0.0).rgb, vec3<f32>(0.0), vec3<f32>(1.0));
    }
    return vec4<f32>(to_linear(s), c.a);
}
"#;

#[derive(Default)]
pub(crate) struct ColorLut {
    view: Option<wgpu::TextureView>,
    zone: Option<wgpu::TextureView>,
    generation: u64,
}

fn upload_lut(device: &wgpu::Device, queue: &wgpu::Queue, data: &[u8]) -> wgpu::TextureView {
    let extent = wgpu::Extent3d { width: LUT_SIZE, height: LUT_SIZE, depth_or_array_layers: LUT_SIZE };
    let texture = device.create_texture(&wgpu::TextureDescriptor {
        label: Some("hp-render color correction"),
        size: extent,
        mip_level_count: 1,
        sample_count: 1,
        dimension: wgpu::TextureDimension::D3,
        format: wgpu::TextureFormat::Rgba8Unorm,
        usage: wgpu::TextureUsages::TEXTURE_BINDING | wgpu::TextureUsages::COPY_DST,
        view_formats: &[],
    });
    queue.write_texture(
        texture.as_image_copy(),
        data,
        wgpu::TexelCopyBufferLayout { offset: 0, bytes_per_row: Some(LUT_SIZE * 4), rows_per_image: Some(LUT_SIZE) },
        extent,
    );
    texture.create_view(&wgpu::TextureViewDescriptor::default())
}

fn identity_lut() -> Vec<u8> {
    let step = 255.0 / (LUT_SIZE - 1) as f32;
    let n = LUT_SIZE * LUT_SIZE * LUT_SIZE;
    let mut out = Vec::with_capacity(n as usize * 4);
    for i in 0..n {
        let (r, g, b) = (i % LUT_SIZE, (i / LUT_SIZE) % LUT_SIZE, i / (LUT_SIZE * LUT_SIZE));
        out.extend([r, g, b].map(|c| (c as f32 * step).round() as u8));
        out.push(255);
    }
    out
}

impl Renderer {
    pub fn set_color_correction(&mut self, lut: Option<&[u8]>) {
        let size = (LUT_SIZE * LUT_SIZE * LUT_SIZE * 4) as usize;
        let lut = lut.filter(|d| d.len() == size);
        let cc = &mut self.lightmap.color;
        cc.generation += 1;
        cc.view = lut.map(|data| upload_lut(&self.device, &self.queue, data));
        self.content_gen += 1;
    }

    pub fn has_color_correction(&self) -> bool {
        self.lightmap.color.view.is_some()
    }

    pub fn set_color_zone(&mut self, lut: Option<&[u8]>) {
        let size = (LUT_SIZE * LUT_SIZE * LUT_SIZE * 4) as usize;
        let lut = lut.filter(|d| d.len() == size);
        let cc = &mut self.lightmap.color;
        if lut.is_none() && cc.zone.is_none() {
            return;
        }
        cc.generation += 1;
        cc.zone = lut.map(|data| upload_lut(&self.device, &self.queue, data));
        self.content_gen += 1;
    }

    pub fn has_color_zone(&self) -> bool {
        self.lightmap.color.zone.is_some()
    }
}

struct Source {
    texture: wgpu::Texture,
    size: [u32; 2],
    bind: Option<(wgpu::BindGroup, u64)>,
}

#[derive(Default)]
pub(crate) struct PostPass {
    pipeline: Option<(wgpu::RenderPipeline, wgpu::BindGroupLayout, wgpu::Sampler)>,
    sources: std::collections::HashMap<u32, Source>,
    identity: Option<wgpu::TextureView>,
    flags: Option<wgpu::Buffer>,
}

impl PostPass {
    fn pipeline(&mut self, device: &wgpu::Device) -> &(wgpu::RenderPipeline, wgpu::BindGroupLayout, wgpu::Sampler) {
        self.pipeline.get_or_insert_with(|| {
            let layout = device.create_bind_group_layout(&wgpu::BindGroupLayoutDescriptor {
                label: Some("hp-render color correction"),
                entries: &[
                    wgpu::BindGroupLayoutEntry {
                        binding: 0,
                        visibility: wgpu::ShaderStages::FRAGMENT,
                        ty: wgpu::BindingType::Texture {
                            sample_type: wgpu::TextureSampleType::Float { filterable: false },
                            view_dimension: wgpu::TextureViewDimension::D2,
                            multisampled: false,
                        },
                        count: None,
                    },
                    wgpu::BindGroupLayoutEntry {
                        binding: 1,
                        visibility: wgpu::ShaderStages::FRAGMENT,
                        ty: wgpu::BindingType::Texture {
                            sample_type: wgpu::TextureSampleType::Float { filterable: true },
                            view_dimension: wgpu::TextureViewDimension::D3,
                            multisampled: false,
                        },
                        count: None,
                    },
                    wgpu::BindGroupLayoutEntry {
                        binding: 2,
                        visibility: wgpu::ShaderStages::FRAGMENT,
                        ty: wgpu::BindingType::Sampler(wgpu::SamplerBindingType::Filtering),
                        count: None,
                    },
                    wgpu::BindGroupLayoutEntry {
                        binding: 3,
                        visibility: wgpu::ShaderStages::FRAGMENT,
                        ty: wgpu::BindingType::Texture {
                            sample_type: wgpu::TextureSampleType::Float { filterable: true },
                            view_dimension: wgpu::TextureViewDimension::D3,
                            multisampled: false,
                        },
                        count: None,
                    },
                    wgpu::BindGroupLayoutEntry {
                        binding: 4,
                        visibility: wgpu::ShaderStages::FRAGMENT,
                        ty: wgpu::BindingType::Buffer {
                            ty: wgpu::BufferBindingType::Uniform,
                            has_dynamic_offset: false,
                            min_binding_size: None,
                        },
                        count: None,
                    },
                ],
            });
            let pl = device.create_pipeline_layout(&wgpu::PipelineLayoutDescriptor {
                label: Some("hp-render color correction"),
                bind_group_layouts: &[Some(&layout)],
                immediate_size: 0,
            });
            let module = device.create_shader_module(wgpu::ShaderModuleDescriptor {
                label: Some("hp-render color correction"),
                source: wgpu::ShaderSource::Wgsl(SHADER.into()),
            });
            let pipeline = device.create_render_pipeline(&wgpu::RenderPipelineDescriptor {
                label: Some("hp-render color correction"),
                layout: Some(&pl),
                vertex: wgpu::VertexState {
                    module: &module,
                    entry_point: Some("vs_full"),
                    compilation_options: Default::default(),
                    buffers: &[],
                },
                primitive: wgpu::PrimitiveState::default(),
                depth_stencil: None,
                multisample: wgpu::MultisampleState::default(),
                fragment: Some(wgpu::FragmentState {
                    module: &module,
                    entry_point: Some("fs_lut"),
                    compilation_options: Default::default(),
                    targets: &[Some(wgpu::ColorTargetState {
                        format: COLOR_FORMAT,
                        blend: None,
                        write_mask: wgpu::ColorWrites::ALL,
                    })],
                }),
                multiview_mask: None,
                cache: None,
            });
            let sampler = device.create_sampler(&wgpu::SamplerDescriptor {
                label: Some("hp-render color correction"),
                address_mode_u: wgpu::AddressMode::ClampToEdge,
                address_mode_v: wgpu::AddressMode::ClampToEdge,
                address_mode_w: wgpu::AddressMode::ClampToEdge,
                mag_filter: wgpu::FilterMode::Linear,
                min_filter: wgpu::FilterMode::Linear,
                ..Default::default()
            });
            (pipeline, layout, sampler)
        })
    }
}

impl PassHook for PostPass {
    fn name(&self) -> &'static str {
        "color_correction"
    }

    fn finish(&mut self, cx: &HookContext<'_>, encoder: &mut wgpu::CommandEncoder) {
        let lm = &cx.renderer.lightmap;
        let map_on = lm.color.view.is_some() && lm.display() == LightmapDisplay::Baked;
        let zone_on = lm.color.zone.is_some();
        if !map_on && !zone_on {
            self.sources.clear();
            return;
        }
        if cx.is_ortho {
            return;
        }
        let device = cx.device();
        let generation = lm.color.generation;
        let [w, h] = cx.size;
        if w == 0 || h == 0 {
            return;
        }
        self.pipeline(device);
        if self.identity.is_none() {
            self.identity = Some(upload_lut(device, cx.queue(), &identity_lut()));
        }
        let flags = self.flags.get_or_insert_with(|| {
            device.create_buffer(&wgpu::BufferDescriptor {
                label: Some("hp-render color correction flags"),
                size: 16,
                usage: wgpu::BufferUsages::UNIFORM | wgpu::BufferUsages::COPY_DST,
                mapped_at_creation: false,
            })
        });
        let values: [f32; 4] = [f32::from(u8::from(map_on)), f32::from(u8::from(zone_on)), 0.0, 0.0];
        cx.queue().write_buffer(flags, 0, bytemuck::cast_slice(&values));
        let Some(identity) = &self.identity else { return };
        let lut = lm.color.view.as_ref().unwrap_or(identity);
        let zone = lm.color.zone.as_ref().unwrap_or(identity);
        let Some((pipeline, layout, sampler)) = &self.pipeline else { return };
        let src = self.sources.entry(cx.view_id).or_insert_with(|| Source {
            texture: device.create_texture(&wgpu::TextureDescriptor {
                label: Some("hp-render color correction source"),
                size: wgpu::Extent3d { width: w, height: h, depth_or_array_layers: 1 },
                mip_level_count: 1,
                sample_count: 1,
                dimension: wgpu::TextureDimension::D2,
                format: COLOR_FORMAT,
                usage: wgpu::TextureUsages::TEXTURE_BINDING | wgpu::TextureUsages::COPY_DST,
                view_formats: &[],
            }),
            size: [w, h],
            bind: None,
        });
        if src.size != [w, h] {
            src.texture = device.create_texture(&wgpu::TextureDescriptor {
                label: Some("hp-render color correction source"),
                size: wgpu::Extent3d { width: w, height: h, depth_or_array_layers: 1 },
                mip_level_count: 1,
                sample_count: 1,
                dimension: wgpu::TextureDimension::D2,
                format: COLOR_FORMAT,
                usage: wgpu::TextureUsages::TEXTURE_BINDING | wgpu::TextureUsages::COPY_DST,
                view_formats: &[],
            });
            src.size = [w, h];
            src.bind = None;
        }
        if src.bind.as_ref().is_none_or(|(_, g)| *g != generation) {
            let view = src.texture.create_view(&wgpu::TextureViewDescriptor::default());
            let bg = device.create_bind_group(&wgpu::BindGroupDescriptor {
                label: Some("hp-render color correction"),
                layout,
                entries: &[
                    wgpu::BindGroupEntry { binding: 0, resource: wgpu::BindingResource::TextureView(&view) },
                    wgpu::BindGroupEntry { binding: 1, resource: wgpu::BindingResource::TextureView(lut) },
                    wgpu::BindGroupEntry { binding: 2, resource: wgpu::BindingResource::Sampler(sampler) },
                    wgpu::BindGroupEntry { binding: 3, resource: wgpu::BindingResource::TextureView(zone) },
                    wgpu::BindGroupEntry { binding: 4, resource: flags.as_entire_binding() },
                ],
            });
            src.bind = Some((bg, generation));
        }
        let Some((bind, _)) = &src.bind else { return };
        encoder.copy_texture_to_texture(
            cx.color_texture.as_image_copy(),
            src.texture.as_image_copy(),
            wgpu::Extent3d { width: w, height: h, depth_or_array_layers: 1 },
        );
        let mut pass = encoder.begin_render_pass(&wgpu::RenderPassDescriptor {
            label: Some("hp-render color correction"),
            color_attachments: &[Some(wgpu::RenderPassColorAttachment {
                view: cx.color,
                depth_slice: None,
                resolve_target: None,
                ops: wgpu::Operations { load: wgpu::LoadOp::Load, store: wgpu::StoreOp::Store },
            })],
            depth_stencil_attachment: None,
            timestamp_writes: None,
            occlusion_query_set: None,
            multiview_mask: None,
        });
        pass.set_pipeline(pipeline);
        pass.set_bind_group(0, bind, &[]);
        pass.draw(0..3, 0..1);
    }
}
