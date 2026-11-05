use bytemuck::{Pod, Zeroable};

use crate::arena::GrowBuffer;
use crate::camera::Camera;
use crate::pipelines::{self, Blend, Desc, Shared, vertex_layout};
use crate::types::MaterialId;
use crate::{FrameCounter, Renderer, ViewParams};

pub(crate) const PARTICLE_WGSL: &str = concat!(
    include_str!("shaders/common.wgsl"),
    "\n",
    include_str!("shaders/material.wgsl"),
    "\n",
    include_str!("shaders/parallax.wgsl"),
    "\n",
    include_str!("shaders/particle.wgsl")
);

#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub enum ParticleOrient {
    #[default]
    Screen,
    Upright,
    Flat,
    Stretched,
}

#[derive(Clone, Copy, Debug, Default, PartialEq, Eq, PartialOrd, Ord, Hash)]
pub enum ParticleBlend {
    #[default]
    Alpha,
    Additive,
    AddSelf,
}

#[derive(Clone, Copy, Debug, PartialEq)]
pub struct ParticleInstance {
    pub pos: [f32; 3],
    pub size: f32,
    pub axis: [f32; 3],
    pub roll: f32,
    pub uv: [f32; 4],
    pub color: [f32; 4],
    pub bright: f32,
    pub add_self: f32,
    pub screen_fade: [f32; 2],
    pub orient: ParticleOrient,
    pub material: MaterialId,
    pub blend: ParticleBlend,
}

#[repr(C)]
#[derive(Clone, Copy, Debug, Default, PartialEq, Pod, Zeroable)]
pub(crate) struct ParticleGpu {
    pos: [f32; 3],
    size: f32,
    axis: [f32; 3],
    roll: f32,
    rect: [f32; 4],
    color: u32,
    mode: u32,
    bright: f32,
    fade: u32,
}

const ATTRS: [wgpu::VertexAttribute; 9] = [
    wgpu::VertexAttribute { format: wgpu::VertexFormat::Float32x3, offset: 0, shader_location: 0 },
    wgpu::VertexAttribute { format: wgpu::VertexFormat::Float32, offset: 12, shader_location: 1 },
    wgpu::VertexAttribute { format: wgpu::VertexFormat::Float32x3, offset: 16, shader_location: 2 },
    wgpu::VertexAttribute { format: wgpu::VertexFormat::Float32, offset: 28, shader_location: 3 },
    wgpu::VertexAttribute { format: wgpu::VertexFormat::Float32x4, offset: 32, shader_location: 4 },
    wgpu::VertexAttribute { format: wgpu::VertexFormat::Unorm8x4, offset: 48, shader_location: 5 },
    wgpu::VertexAttribute { format: wgpu::VertexFormat::Uint32, offset: 52, shader_location: 6 },
    wgpu::VertexAttribute { format: wgpu::VertexFormat::Float32, offset: 56, shader_location: 7 },
    wgpu::VertexAttribute { format: wgpu::VertexFormat::Unorm16x2, offset: 60, shader_location: 8 },
];

const BLENDS: [ParticleBlend; 3] = [ParticleBlend::Alpha, ParticleBlend::AddSelf, ParticleBlend::Additive];

fn blend_index(b: ParticleBlend) -> usize {
    match b {
        ParticleBlend::Alpha => 0,
        ParticleBlend::AddSelf => 1,
        ParticleBlend::Additive => 2,
    }
}

#[derive(Clone, Copy, Debug, PartialEq)]
struct Run {
    material: MaterialId,
    blend: ParticleBlend,
    start: u32,
    count: u32,
}

struct Pipes {
    samples: u32,
    pipes: [wgpu::RenderPipeline; 3],
}

#[derive(Default)]
pub(crate) struct Particles {
    buffer: Option<GrowBuffer>,
    runs: Vec<Run>,
    count: u32,
    module: Option<wgpu::ShaderModule>,
    pipes: Vec<Pipes>,
}

impl Particles {
    fn pipes(&mut self, device: &wgpu::Device, shared: &Shared, samples: u32) {
        if self.pipes.iter().any(|p| p.samples == samples) {
            return;
        }
        let module = self.module.get_or_insert_with(|| {
            device.create_shader_module(wgpu::ShaderModuleDescriptor {
                label: Some("hp-render particles"),
                source: wgpu::ShaderSource::Wgsl(PARTICLE_WGSL.into()),
            })
        });
        let vb = [Some(vertex_layout(size_of::<ParticleGpu>(), wgpu::VertexStepMode::Instance, &ATTRS))];
        let make = |b: ParticleBlend| {
            let (label, blend) = match b {
                ParticleBlend::Alpha => ("hp-render particles", Blend::Alpha),
                ParticleBlend::AddSelf => ("hp-render particles add self", Blend::Premultiplied),
                ParticleBlend::Additive => ("hp-render particles additive", Blend::Additive),
            };
            pipelines::build_with(
                device,
                samples,
                Desc {
                    label,
                    module,
                    vs: "vs_particle",
                    fs: "fs_particle",
                    layout: &shared.layout_material,
                    buffers: &vb,
                    topology: wgpu::PrimitiveTopology::TriangleList,
                    cull: None,
                    depth_test: true,
                    depth_write: false,
                    blend,
                    constants: &[("BLEND_MODE", blend_index(b) as f64)],
                },
                wgpu::PolygonMode::Fill,
            )
        };
        self.pipes.push(Pipes { samples, pipes: BLENDS.map(make) });
    }
}

fn finite3(v: [f32; 3]) -> [f32; 3] {
    if v.iter().all(|c| c.is_finite()) { v } else { [0.0; 3] }
}

fn finite(v: f32, default: f32) -> f32 {
    if v.is_finite() { v } else { default }
}

fn pack_fade(f: [f32; 2]) -> u32 {
    let q = |v: f32| (finite(v, 0.0).clamp(0.0, 4.0) / 4.0 * 65535.0).round() as u32;
    q(f[0]) | (q(f[1]) << 16)
}

fn pack(particles: &[ParticleInstance]) -> (Vec<ParticleGpu>, Vec<Run>) {
    let mut keyed: Vec<(usize, usize)> = particles
        .iter()
        .enumerate()
        .filter(|(_, p)| p.pos.iter().all(|c| c.is_finite()))
        .map(|(i, p)| (blend_index(p.blend), i))
        .collect();
    keyed.sort_unstable();
    let mut data = Vec::with_capacity(keyed.len());
    let mut runs: Vec<Run> = Vec::new();
    for (_, i) in keyed {
        let p = &particles[i];
        let rect = if p.uv.iter().all(|v| v.is_finite()) { p.uv } else { [0.0, 0.0, 1.0, 1.0] };
        let color = p.color.map(|c| finite(c, 0.0).clamp(0.0, 1.0));
        data.push(ParticleGpu {
            pos: p.pos,
            size: finite(p.size, 0.0).max(0.0),
            axis: finite3(p.axis),
            roll: finite(p.roll, 0.0),
            rect,
            color: crate::pack_rgba(color),
            mode: (finite(p.add_self, 0.0).clamp(0.0, 16.0) * 256.0).round() as u32 * 256
                + match p.orient {
                    ParticleOrient::Screen => 0,
                    ParticleOrient::Upright => 1,
                    ParticleOrient::Flat => 2,
                    ParticleOrient::Stretched => 3,
                },
            bright: finite(p.bright, 1.0).clamp(0.0, 16.0),
            fade: pack_fade(p.screen_fade),
        });
        let n = data.len() as u32;
        match runs.last_mut() {
            Some(r) if r.material == p.material && r.blend == p.blend => r.count += 1,
            _ => runs.push(Run { material: p.material, blend: p.blend, start: n - 1, count: 1 }),
        }
    }
    (data, runs)
}

impl Renderer {
    pub fn set_particles(&mut self, particles: &[ParticleInstance]) {
        if self.upload_particles(false, particles) {
            self.content_gen += 1;
        }
    }

    fn upload_particles(&mut self, preview: bool, particles: &[ParticleInstance]) -> bool {
        let limit = (self.max_buffer / size_of::<ParticleGpu>() as u64) as usize;
        let particles = &particles[..particles.len().min(limit)];
        let mut set = std::mem::take(if preview { &mut self.preview_particles } else { &mut self.particles });
        let label = if preview { "hp-render preview particles" } else { "hp-render particles" };
        let changed = !(particles.is_empty() && set.count == 0);
        if changed {
            let (data, runs) = pack(particles);
            for r in &runs {
                let _ = self.scene.material_slot(r.material);
            }
            if !runs.is_empty() {
                self.sync_materials();
            }
            set.count = data.len() as u32;
            set.runs = runs;
            if !data.is_empty() {
                let bytes: &[u8] = bytemuck::cast_slice(&data);
                let buffer = set
                    .buffer
                    .get_or_insert_with(|| GrowBuffer::new(&self.device, label, wgpu::BufferUsages::VERTEX, 1 << 14));
                let _ = buffer.ensure(&self.device, &self.queue, bytes.len() as u64, self.max_buffer, false);
                if bytes.len() as u64 <= buffer.size {
                    self.queue.write_buffer(&buffer.buffer, 0, bytes);
                } else {
                    set.count = 0;
                    set.runs.clear();
                }
            }
        }
        *(if preview { &mut self.preview_particles } else { &mut self.particles }) = set;
        changed
    }

    pub fn particle_count(&self) -> u32 {
        self.particles.count
    }

    pub fn preview_particle_count(&self) -> u32 {
        self.preview_particles.count
    }

    pub(crate) fn ensure_particle_pipes(&mut self, samples: u32) {
        if self.particles.count > 0 {
            self.particles.pipes(&self.device, &self.shared, samples);
        }
    }

    pub(crate) fn draw_particles(&self, pass: &mut wgpu::RenderPass<'_>, samples: u32, n: &mut FrameCounter) {
        self.draw_particle_set(&self.particles, pass, samples, n);
    }

    pub fn render_particle_preview(
        &mut self,
        view_id: u32,
        params: &ViewParams,
        particles: &[ParticleInstance],
    ) -> &wgpu::TextureView {
        self.flush();
        let mut params = *params;
        params.size = [params.size[0].clamp(1, self.max_tex_dim), params.size[1].clamp(1, self.max_tex_dim)];
        let mut opts = self.view_options(view_id);
        opts.show_fog = false;
        opts.fog_volume_inside = false;
        opts.show_sky = false;
        opts.show_sky_3d = false;
        opts.compiled_lighting = false;
        opts.occlusion_culling = false;
        let cam = Camera::new(&params);
        let samples = if params.msaa { 4 } else { 1 };
        self.upload_particles(true, particles);
        self.ensure_view(view_id, params.size, samples);
        self.prepare_tone(view_id, params.size, params.mode, false);
        if self.preview_particles.count > 0 {
            self.preview_particles.pipes(&self.device, &self.shared, samples);
        }
        let uniform = self.view_uniform(&cam, &params, &opts, None, None, false, 1.0);
        self.ensure_view_bind_group(view_id);
        let mut counter = FrameCounter::default();
        let mut enc = self
            .device
            .create_command_encoder(&wgpu::CommandEncoderDescriptor { label: Some("hp-render particle preview") });
        if let Some(view) = self.views.get(&view_id)
            && let Some(bind_group) = view.bind_group.as_ref()
        {
            self.queue.write_buffer(&view.uniform, 0, bytemuck::bytes_of(&uniform));
            let bg = params.background.map(|c| f64::from(crate::gpu::srgb_to_linear(c)));
            let clear = wgpu::Color { r: bg[0], g: bg[1], b: bg[2], a: 1.0 };
            let (target, resolve) = match &view.msaa_view {
                Some(m) => (m, Some(&view.color_view)),
                None => (&view.color_view, None),
            };
            let mut pass = enc.begin_render_pass(&wgpu::RenderPassDescriptor {
                label: Some("hp-render particle preview pass"),
                color_attachments: &[Some(wgpu::RenderPassColorAttachment {
                    view: target,
                    depth_slice: None,
                    resolve_target: resolve,
                    ops: wgpu::Operations {
                        load: wgpu::LoadOp::Clear(clear),
                        store: if resolve.is_some() { wgpu::StoreOp::Discard } else { wgpu::StoreOp::Store },
                    },
                })],
                depth_stencil_attachment: Some(wgpu::RenderPassDepthStencilAttachment {
                    view: &view.depth_view,
                    depth_ops: Some(wgpu::Operations { load: wgpu::LoadOp::Clear(0.0), store: wgpu::StoreOp::Discard }),
                    stencil_ops: None,
                }),
                timestamp_writes: None,
                occlusion_query_set: None,
                multiview_mask: None,
            });
            pass.set_bind_group(0, bind_group, &[]);
            self.draw_particle_set(&self.preview_particles, &mut pass, samples, &mut counter);
        }
        let present = self.views.get(&view_id).map(|v| (v.color.clone(), v.color_view.clone(), v.display_srgb.clone()));
        if let Some((color, color_view, display)) = present {
            let inputs = crate::hdr::FrameInputs {
                color: &color,
                color_view: &color_view,
                display: &display,
                size: params.size,
                chain: false,
            };
            self.record_present(&mut enc, view_id, &inputs);
        }
        self.queue.submit([enc.finish()]);
        match self.views.get(&view_id) {
            Some(view) => &view.display_view,
            None => &self.fallback_view,
        }
    }

    fn draw_particle_set(&self, d: &Particles, pass: &mut wgpu::RenderPass<'_>, samples: u32, n: &mut FrameCounter) {
        let (Some(buffer), Some(p)) = (&d.buffer, d.pipes.iter().find(|p| p.samples == samples)) else { return };
        if d.count == 0 {
            return;
        }
        pass.set_vertex_buffer(0, buffer.buffer.slice(..));
        let mut bound = None;
        for r in &d.runs {
            let Some(g) = self.scene.find_material(r.material).and_then(|s| self.mat_gpu.get(s as usize)) else {
                continue;
            };
            if bound != Some(r.blend) {
                pass.set_pipeline(&p.pipes[blend_index(r.blend)]);
                bound = Some(r.blend);
            }
            pass.set_bind_group(1, &g.bind_group, &[]);
            pass.draw(0..6, r.start..r.start + r.count);
            n.add(u64::from(r.count) * 2);
            n.counts.sprites += r.count;
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn p(material: MaterialId, blend: ParticleBlend, x: f32) -> ParticleInstance {
        ParticleInstance {
            pos: [x, 0.0, 0.0],
            size: 4.0,
            axis: [0.0; 3],
            roll: 0.0,
            uv: [0.0, 0.0, 1.0, 1.0],
            color: [1.0; 4],
            bright: 1.0,
            add_self: 0.0,
            screen_fade: [0.0; 2],
            orient: ParticleOrient::Screen,
            material,
            blend,
        }
    }

    #[test]
    fn runs_stay_back_to_front_and_put_additive_last() {
        use ParticleBlend::{AddSelf, Additive, Alpha};
        let list = [
            p(1, Alpha, 0.0),
            p(2, Additive, 1.0),
            p(3, Alpha, 2.0),
            p(4, AddSelf, 2.5),
            p(1, Alpha, 3.0),
            p(2, Additive, f32::NAN),
        ];
        let (data, runs) = pack(&list);
        assert_eq!(data.len(), 5, "non finite positions are skipped");
        let keys: Vec<(MaterialId, ParticleBlend, u32, u32)> =
            runs.iter().map(|r| (r.material, r.blend, r.start, r.count)).collect();
        assert_eq!(
            keys,
            vec![(1, Alpha, 0, 1), (3, Alpha, 1, 1), (1, Alpha, 2, 1), (4, AddSelf, 3, 1), (2, Additive, 4, 1)]
        );
        assert_eq!(data[2].pos[0], 3.0, "the order of the list is kept inside a blend");
    }

    #[test]
    fn hostile_values_are_cleaned() {
        let mut bad = p(1, ParticleBlend::Alpha, 0.0);
        bad.size = f32::NAN;
        bad.uv = [f32::INFINITY; 4];
        bad.bright = 1.0e9;
        bad.axis = [f32::NAN; 3];
        bad.screen_fade = [f32::NAN, 9.0];
        bad.orient = ParticleOrient::Stretched;
        let (data, _) = pack(&[bad]);
        assert_eq!(data[0].size, 0.0);
        assert_eq!(data[0].rect, [0.0, 0.0, 1.0, 1.0]);
        assert_eq!(data[0].bright, 16.0);
        assert_eq!(data[0].axis, [0.0; 3]);
        assert_eq!(data[0].mode, 3);
        assert_eq!(data[0].fade, 0xFFFF_0000, "start 0, end clamped to 4");
        assert_eq!(pack_fade([1.0, 2.0]), (16384 | (32768 << 16)));
        assert_eq!(size_of::<ParticleGpu>(), 64);
    }

    #[test]
    fn the_shader_has_the_common_block() {
        assert!(PARTICLE_WGSL.contains("struct View"));
        assert!(PARTICLE_WGSL.contains("fn vs_particle"));
        assert!(!PARTICLE_WGSL.contains('\u{2014}') && !PARTICLE_WGSL.contains('\u{2013}'));
    }
}
