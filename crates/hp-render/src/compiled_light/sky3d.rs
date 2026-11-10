use std::sync::OnceLock;

use glam::{Mat4, Vec3};

use super::{BlockGpu, TRANSFORM_ATTRS, VERTEX_ATTRS, VertexGpu, box_visible, shader_and_layout, textured_mode};
use crate::pipelines::{COLOR_FORMAT, DEPTH_FORMAT, MeshPass};
use crate::{ObjectFlags, ObjectId, Renderer, classify_with};

struct SkyPipelines {
    opaque: [[wgpu::RenderPipeline; 2]; 2],
    blended: [[wgpu::RenderPipeline; 2]; 2],
    faded: [[wgpu::RenderPipeline; 2]; 2],
}

#[derive(Default)]
pub(crate) struct CompiledSky {
    pub(crate) objects: ahash::AHashSet<ObjectId>,
    pipelines: [OnceLock<SkyPipelines>; 2],
}

impl Renderer {
    pub fn set_compiled_sky(&mut self, objects: &[ObjectId]) {
        let set: ahash::AHashSet<ObjectId> = objects.iter().copied().collect();
        if set != self.compiled_sky.objects {
            self.compiled_sky.objects = set;
            self.content_gen += 1;
        }
    }

    pub fn compiled_sky(&self) -> Vec<ObjectId> {
        let mut out: Vec<ObjectId> = self.compiled_sky.objects.iter().copied().collect();
        out.sort_unstable();
        out
    }
}

#[allow(clippy::too_many_arguments)]
fn sky_pipeline(
    device: &wgpu::Device,
    module: &wgpu::ShaderModule,
    layout: &wgpu::PipelineLayout,
    samples: u32,
    alpha_test: bool,
    two_sided: bool,
    blend: Option<bool>,
    tint_opacity: bool,
) -> wgpu::RenderPipeline {
    let mode = match blend {
        None => 0.0,
        Some(false) => 1.0,
        Some(true) => 2.0,
    };
    let constants = [
        ("ALPHA_TEST", if alpha_test { 1.0 } else { 0.0 }),
        ("BLEND_MODE", mode),
        ("TINT_OPACITY", if tint_opacity { 1.0 } else { 0.0 }),
        ("SKY_3D", 1.0),
    ];
    let options = wgpu::PipelineCompilationOptions { constants: &constants, zero_initialize_workgroup_memory: false };
    let buffers = [
        Some(wgpu::VertexBufferLayout {
            array_stride: size_of::<VertexGpu>() as u64,
            step_mode: wgpu::VertexStepMode::Vertex,
            attributes: &VERTEX_ATTRS,
        }),
        Some(wgpu::VertexBufferLayout {
            array_stride: 64,
            step_mode: wgpu::VertexStepMode::Instance,
            attributes: &TRANSFORM_ATTRS,
        }),
    ];
    let component =
        |src, dst| wgpu::BlendComponent { src_factor: src, dst_factor: dst, operation: wgpu::BlendOperation::Add };
    use wgpu::BlendFactor as F;
    let blend_state = match blend {
        None => None,
        Some(false) => Some(wgpu::BlendState {
            color: component(F::SrcAlpha, F::OneMinusSrcAlpha),
            alpha: component(F::One, F::OneMinusSrcAlpha),
        }),
        Some(true) => Some(wgpu::BlendState { color: component(F::One, F::One), alpha: component(F::Zero, F::One) }),
    };
    let opaque = blend.is_none();
    let bias = if opaque {
        wgpu::DepthBiasState { constant: 4, slope_scale: 0.5, clamp: 0.0 }
    } else {
        wgpu::DepthBiasState::default()
    };
    device.create_render_pipeline(&wgpu::RenderPipelineDescriptor {
        label: Some("hp-render compiled skybox"),
        layout: Some(layout),
        vertex: wgpu::VertexState {
            module,
            entry_point: Some("vs_compiled"),
            compilation_options: options.clone(),
            buffers: &buffers,
        },
        primitive: wgpu::PrimitiveState {
            topology: wgpu::PrimitiveTopology::TriangleList,
            strip_index_format: None,
            front_face: wgpu::FrontFace::Ccw,
            cull_mode: if two_sided { None } else { Some(wgpu::Face::Back) },
            unclipped_depth: false,
            polygon_mode: wgpu::PolygonMode::Fill,
            conservative: false,
        },
        depth_stencil: Some(wgpu::DepthStencilState {
            format: DEPTH_FORMAT,
            depth_write_enabled: Some(opaque),
            depth_compare: Some(wgpu::CompareFunction::GreaterEqual),
            stencil: wgpu::StencilState::default(),
            bias,
        }),
        multisample: wgpu::MultisampleState { count: samples, mask: !0, alpha_to_coverage_enabled: false },
        fragment: Some(wgpu::FragmentState {
            module,
            entry_point: Some("fs_compiled"),
            compilation_options: options,
            targets: &[Some(wgpu::ColorTargetState {
                format: COLOR_FORMAT,
                blend: blend_state,
                write_mask: wgpu::ColorWrites::ALL,
            })],
        }),
        multiview_mask: None,
        cache: None,
    })
}

type Key = (usize, usize, usize);

impl crate::DrawCtx<'_> {
    pub(crate) fn record_compiled_sky(
        &self,
        pass: &mut wgpu::RenderPass<'_>,
        translucent: bool,
        n: &mut crate::FrameCounter,
    ) {
        let r = self.r;
        let cl = &r.compiled;
        let sky = &r.compiled_sky;
        if self.cam.is_ortho
            || !self.opts.compiled_lighting
            || !cl.present
            || !textured_mode(self.params.mode)
            || sky.objects.is_empty()
        {
            return;
        }
        let (Some(bind_group), Some(transforms)) = (&cl.bind_group, &cl.transform_buf) else { return };
        let device = &r.device;
        let samples = self.hook_cx.samples;
        let (module, layout) = shader_and_layout(r, device);
        let pipes = sky.pipelines[usize::from(samples > 1)].get_or_init(|| {
            let make = |alpha, two, blend, tint| sky_pipeline(device, module, layout, samples, alpha, two, blend, tint);
            SkyPipelines {
                opaque: [false, true].map(|alpha| [false, true].map(|two| make(alpha, two, None, false))),
                blended: [false, true].map(|add| [false, true].map(|two| make(false, two, Some(add), false))),
                faded: [false, true].map(|alpha| [false, true].map(|two| make(alpha, two, Some(false), true))),
            }
        });
        let set = |key: Key| match key.0 {
            0 => &pipes.opaque[key.1][key.2],
            1 => &pipes.blended[key.1][key.2],
            _ => &pipes.faded[key.1][key.2],
        };
        let view_proj: Mat4 = self.cam.view_proj;
        let eye: Vec3 = self.cam.eye;
        let shown = |b: &BlockGpu| {
            sky.objects.contains(&b.object)
                || r.object_flags(b.object).is_some_and(|f| {
                    f.contains(ObjectFlags::SKY_3D)
                        && !f.contains(ObjectFlags::HIDDEN)
                        && !f.contains(ObjectFlags::NO_3D)
                })
        };
        let order: Vec<(u32, &BlockGpu)> = cl
            .blocks
            .iter()
            .enumerate()
            .filter(|(_, b)| shown(b) && (b.moved || box_visible(&view_proj, b.lo, b.hi)))
            .map(|(i, b)| (i as u32, b))
            .collect();
        if order.is_empty() {
            return;
        }
        pass.set_bind_group(2, bind_group, &[]);
        pass.set_vertex_buffer(1, transforms.slice(..));
        let mut draws: Vec<(f32, u32, &BlockGpu, u32, u32, u32, Key)> = Vec::new();
        for &(index, block) in &order {
            let alpha = r.object_tint(block.object).map_or(1.0, |t| t[3]);
            if alpha <= 0.0 {
                continue;
            }
            let faded = alpha < 0.999;
            let ours = r.object_flags(block.object).is_some_and(|f| f.contains(ObjectFlags::COMPILED_LIGHT));
            let m = cl.transforms.get(index as usize).map_or(Mat4::IDENTITY, Mat4::from_cols_array_2d);
            for d in &block.draws {
                let desc = r.scene.materials.get(d.material as usize).and_then(|m| m.desc);
                let Some((kind, two_sided)) = classify_with(desc.as_ref(), self.opts.force_opaque, false) else {
                    continue;
                };
                let two = usize::from(two_sided);
                let key = match (kind, translucent) {
                    (MeshPass::Opaque, false) if !faded => (0, 0, two),
                    (MeshPass::AlphaTest, false) if !faded => (0, 1, two),
                    (MeshPass::Translucent, true) if ours => (1, 0, two),
                    (MeshPass::Additive, true) if ours => (1, 1, two),
                    (MeshPass::Opaque, true) if ours && faded => (2, 0, two),
                    (MeshPass::AlphaTest, true) if ours && faded => (2, 1, two),
                    _ => continue,
                };
                if translucent {
                    for &(first, count, center) in &d.faces {
                        let dist = (m.transform_point3(center) - eye).length_squared();
                        draws.push((dist, index, block, d.material, first, count, key));
                    }
                } else {
                    draws.push((0.0, index, block, d.material, d.first, d.count, key));
                }
            }
        }
        if translucent {
            draws.sort_by(|a, b| b.0.total_cmp(&a.0));
        }
        let mut current: Option<Key> = None;
        let mut material = u32::MAX;
        let mut bound: Option<u32> = None;
        for (_, index, block, mat, first, count, key) in draws {
            let Some(mg) = r.mat_gpu.get(mat as usize) else { continue };
            if current != Some(key) {
                pass.set_pipeline(set(key));
                current = Some(key);
            }
            if material != mat {
                pass.set_bind_group(1, &mg.bind_group, &[]);
                material = mat;
            }
            if bound != Some(block.segment) {
                cl.bind_block(pass, block);
                bound = Some(block.segment);
            }
            pass.draw_indexed(first..first + count, block.base_vertex, index..index + 1);
            n.add(u64::from(count / 3));
        }
    }
}
