use std::sync::{Arc, OnceLock};

use bytemuck::{Pod, Zeroable};
use wgpu::util::DeviceExt;

use crate::hooks::{HookContext, PassHook};
use crate::lightmap::LightmapDisplay;
use crate::pipelines::{COLOR_FORMAT, DEPTH_FORMAT};

#[repr(C)]
#[derive(Clone, Copy, Debug, Default, PartialEq, Pod, Zeroable)]
pub struct ShaftVertex {
    pub pos: [f32; 3],
    pub uv: [f32; 2],
    pub color: [f32; 4],
}

#[repr(C)]
#[derive(Clone, Copy, Debug, Default, PartialEq, Pod, Zeroable)]
pub struct GlowHalo {
    pub center: [f32; 3],
    pub size: f32,
    pub color: [f32; 3],
    pub pad: f32,
}

#[derive(Clone, Debug, Default, PartialEq)]
pub struct LightShafts {
    pub vertices: Vec<ShaftVertex>,
    pub indices: Vec<u32>,
    pub glows: Vec<GlowHalo>,
}

impl LightShafts {
    pub fn is_empty(&self) -> bool {
        (self.indices.len() < 3 || self.vertices.is_empty()) && self.glows.is_empty()
    }

    pub fn valid(&self) -> bool {
        let n = self.vertices.len() as u32;
        self.indices.len().is_multiple_of(3)
            && self.indices.iter().all(|i| *i < n)
            && self.glows.iter().all(|g| g.size.is_finite() && g.size > 0.0 && g.center.iter().all(|c| c.is_finite()))
    }

    pub fn gpu_bytes(&self) -> u64 {
        (self.vertices.len() * size_of::<ShaftVertex>()
            + self.indices.len() * 4
            + self.glows.len() * size_of::<GlowHalo>()) as u64
    }
}

#[derive(Default)]
pub(crate) struct ShaftState {
    pub(crate) mesh: Option<Arc<LightShafts>>,
    pub(crate) generation: u64,
    pub(crate) player: Option<Arc<LightShafts>>,
    pub(crate) player_generation: u64,
}

const PLAYER_GENERATION: u64 = 1 << 62;

fn active(r: &crate::Renderer) -> Option<(&Arc<LightShafts>, u64)> {
    let state = &r.light_shafts;
    if r.compiled.mode().is_some() {
        return state
            .player
            .as_ref()
            .filter(|m| !m.is_empty())
            .map(|m| (m, PLAYER_GENERATION | state.player_generation));
    }
    if r.lightmap.display() != LightmapDisplay::Baked {
        return None;
    }
    state.mesh.as_ref().filter(|m| !m.is_empty()).map(|m| (m, state.generation))
}

#[repr(C)]
#[derive(Clone, Copy, Debug, Default, Pod, Zeroable)]
struct ShaftParams {
    exposure: f32,
    ldr: u32,
    pad: [u32; 2],
}

struct Gpu {
    generation: u64,
    shafts: Option<(wgpu::Buffer, wgpu::Buffer, u32)>,
    glows: Option<(wgpu::Buffer, u32)>,
}

#[derive(Default)]
pub(crate) struct ShaftPass {
    gpu: Option<Gpu>,
    params: Option<(wgpu::Buffer, wgpu::BindGroup)>,
    layout: OnceLock<(wgpu::BindGroupLayout, wgpu::PipelineLayout, wgpu::ShaderModule)>,
    pipelines: [OnceLock<wgpu::RenderPipeline>; 2],
    glow_pipelines: [OnceLock<wgpu::RenderPipeline>; 2],
}

const SHADER: &str = concat!(include_str!("shaders/common.wgsl"), "\n", include_str!("shaders/light_shafts.wgsl"));

const GLOW_ATTRS: [wgpu::VertexAttribute; 3] = [
    wgpu::VertexAttribute { format: wgpu::VertexFormat::Float32x3, offset: 0, shader_location: 0 },
    wgpu::VertexAttribute { format: wgpu::VertexFormat::Float32, offset: 12, shader_location: 1 },
    wgpu::VertexAttribute { format: wgpu::VertexFormat::Float32x3, offset: 16, shader_location: 2 },
];

const ATTRS: [wgpu::VertexAttribute; 3] = [
    wgpu::VertexAttribute { format: wgpu::VertexFormat::Float32x3, offset: 0, shader_location: 0 },
    wgpu::VertexAttribute { format: wgpu::VertexFormat::Float32x2, offset: 12, shader_location: 1 },
    wgpu::VertexAttribute { format: wgpu::VertexFormat::Float32x4, offset: 20, shader_location: 2 },
];

impl ShaftPass {
    fn layout(&self, cx: &HookContext<'_>) -> &(wgpu::BindGroupLayout, wgpu::PipelineLayout, wgpu::ShaderModule) {
        self.layout.get_or_init(|| {
            let device = cx.device();
            let bgl = device.create_bind_group_layout(&wgpu::BindGroupLayoutDescriptor {
                label: Some("hp-render light shafts"),
                entries: &[wgpu::BindGroupLayoutEntry {
                    binding: 0,
                    visibility: wgpu::ShaderStages::FRAGMENT,
                    ty: wgpu::BindingType::Buffer {
                        ty: wgpu::BufferBindingType::Uniform,
                        has_dynamic_offset: false,
                        min_binding_size: None,
                    },
                    count: None,
                }],
            });
            let layout = device.create_pipeline_layout(&wgpu::PipelineLayoutDescriptor {
                label: Some("hp-render light shafts"),
                bind_group_layouts: &[Some(&cx.renderer.shared.view_bgl), Some(&bgl)],
                immediate_size: 0,
            });
            let module = device.create_shader_module(wgpu::ShaderModuleDescriptor {
                label: Some("hp-render light shafts"),
                source: wgpu::ShaderSource::Wgsl(SHADER.into()),
            });
            (bgl, layout, module)
        })
    }

    fn pipeline(&self, cx: &HookContext<'_>, glow: bool) -> &wgpu::RenderPipeline {
        let (_, layout, module) = self.layout(cx);
        let samples = cx.samples;
        let slot = if glow { &self.glow_pipelines } else { &self.pipelines };
        slot[usize::from(samples > 1)].get_or_init(|| {
            let device = cx.device();
            let buffers = [Some(if glow {
                wgpu::VertexBufferLayout {
                    array_stride: size_of::<GlowHalo>() as u64,
                    step_mode: wgpu::VertexStepMode::Instance,
                    attributes: &GLOW_ATTRS,
                }
            } else {
                wgpu::VertexBufferLayout {
                    array_stride: size_of::<ShaftVertex>() as u64,
                    step_mode: wgpu::VertexStepMode::Vertex,
                    attributes: &ATTRS,
                }
            })];
            let (vs, fs) = if glow { ("vs_glow", "fs_glow") } else { ("vs_shaft", "fs_shaft") };
            let additive = wgpu::BlendComponent {
                src_factor: wgpu::BlendFactor::One,
                dst_factor: wgpu::BlendFactor::One,
                operation: wgpu::BlendOperation::Add,
            };
            crate::pipeline_cache::with(device, |cache| {
                device.create_render_pipeline(&wgpu::RenderPipelineDescriptor {
                    label: Some("hp-render light shafts"),
                    layout: Some(layout),
                    vertex: wgpu::VertexState {
                        module,
                        entry_point: Some(vs),
                        compilation_options: wgpu::PipelineCompilationOptions::default(),
                        buffers: &buffers,
                    },
                    primitive: wgpu::PrimitiveState {
                        topology: wgpu::PrimitiveTopology::TriangleList,
                        cull_mode: None,
                        ..Default::default()
                    },
                    depth_stencil: Some(wgpu::DepthStencilState {
                        format: DEPTH_FORMAT,
                        depth_write_enabled: Some(false),
                        depth_compare: Some(wgpu::CompareFunction::GreaterEqual),
                        stencil: wgpu::StencilState::default(),
                        bias: wgpu::DepthBiasState::default(),
                    }),
                    multisample: wgpu::MultisampleState { count: samples, mask: !0, alpha_to_coverage_enabled: false },
                    fragment: Some(wgpu::FragmentState {
                        module,
                        entry_point: Some(fs),
                        compilation_options: wgpu::PipelineCompilationOptions::default(),
                        targets: &[Some(wgpu::ColorTargetState {
                            format: COLOR_FORMAT,
                            blend: Some(wgpu::BlendState { color: additive, alpha: additive }),
                            write_mask: wgpu::ColorWrites::COLOR,
                        })],
                    }),
                    multiview_mask: None,
                    cache,
                })
            })
        })
    }
}

impl PassHook for ShaftPass {
    fn name(&self) -> &'static str {
        "light_shafts"
    }

    fn prepare(&mut self, cx: &HookContext<'_>, _encoder: &mut wgpu::CommandEncoder) {
        let r = cx.renderer;
        let Some((mesh, generation)) = active(r) else {
            self.gpu = None;
            return;
        };
        if self.gpu.as_ref().is_none_or(|g| g.generation != generation) {
            let device = cx.device();
            let shafts = (mesh.indices.len() >= 3 && !mesh.vertices.is_empty()).then(|| {
                let vbuf = device.create_buffer_init(&wgpu::util::BufferInitDescriptor {
                    label: Some("hp-render light shafts vertices"),
                    contents: bytemuck::cast_slice(&mesh.vertices),
                    usage: wgpu::BufferUsages::VERTEX,
                });
                let ibuf = device.create_buffer_init(&wgpu::util::BufferInitDescriptor {
                    label: Some("hp-render light shafts indices"),
                    contents: bytemuck::cast_slice(&mesh.indices),
                    usage: wgpu::BufferUsages::INDEX,
                });
                (vbuf, ibuf, mesh.indices.len() as u32)
            });
            let glows = (!mesh.glows.is_empty()).then(|| {
                let buf = device.create_buffer_init(&wgpu::util::BufferInitDescriptor {
                    label: Some("hp-render glow halos"),
                    contents: bytemuck::cast_slice(&mesh.glows),
                    usage: wgpu::BufferUsages::VERTEX,
                });
                (buf, mesh.glows.len() as u32)
            });
            self.gpu = Some(Gpu { generation, shafts, glows });
        }
        let params = ShaftParams { exposure: r.lightmap.exposure(), ldr: u32::from(r.lightmap.ldr()), pad: [0; 2] };
        if self.params.is_none() {
            let (bgl, _, _) = self.layout(cx);
            let buf = cx.device().create_buffer(&wgpu::BufferDescriptor {
                label: Some("hp-render light shafts params"),
                size: size_of::<ShaftParams>() as u64,
                usage: wgpu::BufferUsages::UNIFORM | wgpu::BufferUsages::COPY_DST,
                mapped_at_creation: false,
            });
            let bind = cx.device().create_bind_group(&wgpu::BindGroupDescriptor {
                label: Some("hp-render light shafts params"),
                layout: bgl,
                entries: &[wgpu::BindGroupEntry { binding: 0, resource: buf.as_entire_binding() }],
            });
            self.params = Some((buf, bind));
        }
        if let Some((buf, _)) = &self.params {
            cx.queue().write_buffer(buf, 0, bytemuck::bytes_of(&params));
        }
    }

    fn draw_translucent(&self, cx: &HookContext<'_>, pass: &mut wgpu::RenderPass<'_>) {
        let r = cx.renderer;
        if cx.is_ortho {
            return;
        }
        let Some((_, generation)) = active(r) else { return };
        let (Some(gpu), Some((_, bind))) = (&self.gpu, &self.params) else { return };
        if gpu.generation != generation {
            return;
        }
        if let Some((vbuf, ibuf, count)) = &gpu.shafts {
            pass.set_pipeline(self.pipeline(cx, false));
            pass.set_bind_group(0, cx.view_bind_group, &[]);
            pass.set_bind_group(1, bind, &[]);
            pass.set_vertex_buffer(0, vbuf.slice(..));
            pass.set_index_buffer(ibuf.slice(..), wgpu::IndexFormat::Uint32);
            pass.draw_indexed(0..*count, 0, 0..1);
        }
        if let Some((buf, count)) = &gpu.glows {
            pass.set_pipeline(self.pipeline(cx, true));
            pass.set_bind_group(0, cx.view_bind_group, &[]);
            pass.set_bind_group(1, bind, &[]);
            pass.set_vertex_buffer(0, buf.slice(..));
            pass.draw(0..6, 0..*count);
        }
    }
}

impl crate::Renderer {
    pub fn set_light_shafts(&mut self, shafts: Option<LightShafts>) {
        let shafts = shafts.filter(|s| !s.is_empty() && s.valid());
        let same = match (&self.light_shafts.mesh, &shafts) {
            (Some(a), Some(b)) => **a == *b,
            (None, None) => true,
            _ => false,
        };
        if same {
            return;
        }
        self.light_shafts.mesh = shafts.map(Arc::new);
        self.light_shafts.generation += 1;
    }

    pub fn set_player_light_shafts(&mut self, shafts: Option<LightShafts>) {
        let shafts = shafts.filter(|s| !s.is_empty() && s.valid());
        self.light_shafts.player = shafts.map(Arc::new);
        self.light_shafts.player_generation += 1;
    }

    pub fn light_shafts(&self) -> Option<&LightShafts> {
        self.light_shafts.mesh.as_deref()
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn the_vertex_layout_matches_the_shader() {
        assert_eq!(size_of::<ShaftVertex>(), 36);
        assert_eq!(ATTRS[2].offset, 20);
    }

    #[test]
    fn broken_meshes_are_refused() {
        let v = ShaftVertex::default();
        let good = LightShafts { vertices: vec![v; 3], indices: vec![0, 1, 2], glows: Vec::new() };
        assert!(good.valid() && !good.is_empty());
        assert_eq!(good.gpu_bytes(), 3 * 36 + 12);
        let bad = LightShafts { vertices: vec![v; 3], indices: vec![0, 1, 5], glows: Vec::new() };
        assert!(!bad.valid());
        assert!(LightShafts::default().is_empty());
        let halo = GlowHalo { center: [0.0; 3], size: 32.0, color: [1.0; 3], pad: 0.0 };
        let glows = LightShafts { glows: vec![halo], ..Default::default() };
        assert!(!glows.is_empty() && glows.valid(), "halos alone are drawn");
        let bad_halo = LightShafts { glows: vec![GlowHalo { size: f32::NAN, ..halo }], ..Default::default() };
        assert!(!bad_halo.valid());
        assert_eq!(size_of::<GlowHalo>(), 32);
    }

    #[test]
    fn the_shader_parses() {
        let module = wgpu::naga::front::wgsl::parse_str(SHADER).expect("light shaft shader");
        assert!(module.entry_points.iter().any(|e| e.name == "fs_shaft"));
        assert!(module.entry_points.iter().any(|e| e.name == "fs_glow"));
    }
}
