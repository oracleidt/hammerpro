use ahash::{HashMap, HashMapExt};
use bytemuck::{Pod, Zeroable};
use glam::Vec3;

use crate::camera::Camera;
use crate::gpu::srgb_to_linear;
use crate::pipelines::{self, Blend, Desc};
use crate::{DrawCtx, FogVolume, FrameCounter, RenderMode, Renderer, ViewOptions, ViewParams};

pub(crate) const FOG_VOLUME_WGSL: &str =
    concat!(include_str!("shaders/common.wgsl"), "\n", include_str!("shaders/fog_volume.wgsl"));

pub(crate) const MAX_DRAWN: usize = 256;
const NOISE_SCALE: f32 = 1.0 / 48.0;

#[repr(C)]
#[derive(Clone, Copy, Debug, Default, PartialEq, Pod, Zeroable)]
pub(crate) struct FogVolumeUniform {
    pub fog_a: [f32; 4],
    pub fog_b: [f32; 4],
    pub fog_c: [f32; 4],
    pub lo: [f32; 4],
    pub hi: [f32; 4],
    pub depth: [f32; 4],
}

fn finite(v: f32, fallback: f32) -> f32 {
    if v.is_finite() { v } else { fallback }
}

pub(crate) fn volume_uniform(v: &FogVolume, depth: [f32; 2]) -> Option<FogVolumeUniform> {
    let f = v.preview;
    if !v.min.iter().chain(&v.max).all(|c| c.is_finite()) || (0..3).any(|i| v.max[i] <= v.min[i]) {
        return None;
    }
    let rgb = |c: [f32; 3]| c.map(|x| srgb_to_linear(finite(x, 1.0).clamp(0.0, 1.0)));
    let [r, g, b] = rgb(f.color);
    let [r2, g2, b2] = rgb(f.color2);
    let start = finite(f.start, 0.0).max(0.0);
    let end = finite(f.end, start + 1.0).max(start + 1.0);
    let density = finite(f.max_density, 1.0).clamp(0.0, 1.0);
    if density <= 0.0 {
        return None;
    }
    let dir = Vec3::from(f.dir).normalize_or(Vec3::X);
    let dir = if dir.is_finite() { dir } else { Vec3::X };
    Some(FogVolumeUniform {
        fog_a: [r, g, b, density],
        fog_b: [r2, g2, b2, start],
        fog_c: [dir.x, dir.y, dir.z, end],
        lo: [v.min[0], v.min[1], v.min[2], if f.blend { 1.0 } else { 0.0 }],
        hi: [v.max[0], v.max[1], v.max[2], 0.0],
        depth: [depth[0], depth[1], NOISE_SCALE, 0.0],
    })
}

pub(crate) fn inside_volume(volumes: &[FogVolume], eye: [f32; 3]) -> Option<&FogVolume> {
    volumes.iter().find(|v| v.active && v.contains(eye))
}

pub(crate) fn applies(params: &ViewParams, opts: &ViewOptions, is_ortho: bool) -> bool {
    !is_ortho && opts.fog_volumes && params.mode != RenderMode::Wireframe
}

fn box_distance(eye: Vec3, min: Vec3, max: Vec3) -> f32 {
    eye.clamp(min, max).distance(eye)
}

pub(crate) fn draw_order(volumes: &[FogVolume], eye: Vec3, visible: impl Fn(Vec3, Vec3) -> bool) -> Vec<usize> {
    let mut order: Vec<(f32, usize)> = volumes
        .iter()
        .enumerate()
        .filter(|(_, v)| visible(Vec3::from(v.min), Vec3::from(v.max)))
        .map(|(i, v)| (box_distance(eye, Vec3::from(v.min), Vec3::from(v.max)), i))
        .collect();
    order.sort_by(|a, b| b.0.total_cmp(&a.0).then(a.1.cmp(&b.1)));
    order.into_iter().map(|(_, i)| i).take(MAX_DRAWN).collect()
}

pub(crate) struct FogFrame {
    pub count: u32,
}

struct FogViewGpu {
    generation: u64,
    buffer: wgpu::Buffer,
    bind: wgpu::BindGroup,
}

pub(crate) struct FogVolumeState {
    bgl: [wgpu::BindGroupLayout; 2],
    layouts: [wgpu::PipelineLayout; 2],
    modules: [wgpu::ShaderModule; 2],
    pipes: [Option<wgpu::RenderPipeline>; 2],
    views: HashMap<u32, FogViewGpu>,
    stride: u64,
}

pub(crate) fn fog_volume_source(multisampled: bool) -> String {
    if multisampled {
        FOG_VOLUME_WGSL.replace("var scene_depth: texture_depth_2d;", "var scene_depth: texture_depth_multisampled_2d;")
    } else {
        FOG_VOLUME_WGSL.to_owned()
    }
}

impl FogVolumeState {
    fn new(device: &wgpu::Device, view_bgl: &wgpu::BindGroupLayout) -> Self {
        let fragment = wgpu::ShaderStages::FRAGMENT;
        let bgl = [false, true].map(|multisampled| {
            device.create_bind_group_layout(&wgpu::BindGroupLayoutDescriptor {
                label: Some("hp-render fog volume"),
                entries: &[
                    wgpu::BindGroupLayoutEntry {
                        binding: 0,
                        visibility: fragment,
                        ty: wgpu::BindingType::Buffer {
                            ty: wgpu::BufferBindingType::Uniform,
                            has_dynamic_offset: true,
                            min_binding_size: wgpu::BufferSize::new(size_of::<FogVolumeUniform>() as u64),
                        },
                        count: None,
                    },
                    wgpu::BindGroupLayoutEntry {
                        binding: 1,
                        visibility: fragment,
                        ty: wgpu::BindingType::Texture {
                            sample_type: wgpu::TextureSampleType::Depth,
                            view_dimension: wgpu::TextureViewDimension::D2,
                            multisampled,
                        },
                        count: None,
                    },
                ],
            })
        });
        let layouts = [0, 1].map(|i| {
            device.create_pipeline_layout(&wgpu::PipelineLayoutDescriptor {
                label: Some("hp-render fog volume layout"),
                bind_group_layouts: &[Some(view_bgl), Some(&bgl[i])],
                immediate_size: 0,
            })
        });
        let modules = [false, true].map(|multisampled| {
            device.create_shader_module(wgpu::ShaderModuleDescriptor {
                label: Some("hp-render fog volume"),
                source: wgpu::ShaderSource::Wgsl(fog_volume_source(multisampled).into()),
            })
        });
        let align = u64::from(device.limits().min_uniform_buffer_offset_alignment.max(1));
        let stride = (size_of::<FogVolumeUniform>() as u64).div_ceil(align) * align;
        Self { bgl, layouts, modules, pipes: [None, None], views: HashMap::new(), stride }
    }

    fn ensure_pipe(&mut self, device: &wgpu::Device, samples: u32) {
        let i = usize::from(samples > 1);
        if self.pipes[i].is_some() {
            return;
        }
        self.pipes[i] = Some(pipelines::build(
            device,
            samples,
            Desc {
                label: "hp-render fog volume",
                module: &self.modules[i],
                vs: "vs_fog_volume",
                fs: "fs_fog_volume",
                layout: &self.layouts[i],
                buffers: &[],
                topology: wgpu::PrimitiveTopology::TriangleList,
                cull: None,
                depth_test: false,
                depth_write: false,
                blend: Blend::Alpha,
                constants: &[],
            },
        ));
    }
}

impl Renderer {
    pub fn set_fog_volumes(&mut self, volumes: &[FogVolume]) {
        if self.fog_volumes != volumes {
            self.fog_volumes = volumes.to_vec();
            self.content_gen += 1;
        }
    }

    pub fn fog_volumes(&self) -> &[FogVolume] {
        &self.fog_volumes
    }

    pub(crate) fn prepare_fog_volumes(
        &mut self,
        view_id: u32,
        cam: &Camera,
        params: &ViewParams,
        opts: &ViewOptions,
    ) -> Option<FogFrame> {
        if !applies(params, opts, cam.is_ortho) || self.fog_volumes.is_empty() {
            return None;
        }
        let order = draw_order(&self.fog_volumes, cam.eye, |min, max| cam.aabb_visible(min, max));
        let (n, f) = (f64::from(cam.near.max(1e-3)), f64::from(cam.far.max(cam.near + 1.0)));
        let depth = [(n / (f - n)) as f32, (n * f / (f - n)) as f32];
        let uniforms: Vec<FogVolumeUniform> =
            order.iter().filter_map(|&i| volume_uniform(&self.fog_volumes[i], depth)).collect();
        if uniforms.is_empty() {
            return None;
        }
        let (generation, samples) = {
            let v = self.views.get(&view_id)?;
            (v.generation, v.samples)
        };
        if self.fog_volume_gpu.is_none() {
            self.fog_volume_gpu = Some(FogVolumeState::new(&self.device, &self.shared.view_bgl));
        }
        let state = self.fog_volume_gpu.as_mut()?;
        state.ensure_pipe(&self.device, samples);
        let stride = state.stride;
        if state.views.get(&view_id).is_none_or(|v| v.generation != generation) {
            let view = self.views.get(&view_id)?;
            let buffer = self.device.create_buffer(&wgpu::BufferDescriptor {
                label: Some("hp-render fog volume uniforms"),
                size: stride * MAX_DRAWN as u64,
                usage: wgpu::BufferUsages::UNIFORM | wgpu::BufferUsages::COPY_DST,
                mapped_at_creation: false,
            });
            let bind = self.device.create_bind_group(&wgpu::BindGroupDescriptor {
                label: Some("hp-render fog volume"),
                layout: &state.bgl[usize::from(samples > 1)],
                entries: &[
                    wgpu::BindGroupEntry {
                        binding: 0,
                        resource: wgpu::BindingResource::Buffer(wgpu::BufferBinding {
                            buffer: &buffer,
                            offset: 0,
                            size: wgpu::BufferSize::new(size_of::<FogVolumeUniform>() as u64),
                        }),
                    },
                    wgpu::BindGroupEntry { binding: 1, resource: wgpu::BindingResource::TextureView(&view.depth_view) },
                ],
            });
            state.views.insert(view_id, FogViewGpu { generation, buffer, bind });
        }
        let gpu = state.views.get(&view_id)?;
        let mut bytes = vec![0u8; stride as usize * uniforms.len()];
        for (k, u) in uniforms.iter().enumerate() {
            let at = k * stride as usize;
            bytes[at..at + size_of::<FogVolumeUniform>()].copy_from_slice(bytemuck::bytes_of(u));
        }
        self.queue.write_buffer(&gpu.buffer, 0, &bytes);
        Some(FogFrame { count: uniforms.len() as u32 })
    }

    pub(crate) fn forget_fog_volume_view(&mut self, view_id: u32) {
        if let Some(s) = self.fog_volume_gpu.as_mut() {
            s.views.remove(&view_id);
        }
    }
}

impl DrawCtx<'_> {
    pub(crate) fn record_fog_volumes(
        &self,
        enc: &mut wgpu::CommandEncoder,
        frame: &FogFrame,
        view_id: u32,
        n: &mut FrameCounter,
    ) {
        let view = self.view;
        let Some(state) = self.r.fog_volume_gpu.as_ref() else { return };
        let (Some(pipe), Some(gpu)) = (state.pipes[usize::from(view.samples > 1)].as_ref(), state.views.get(&view_id))
        else {
            return;
        };
        let (target, resolve) = match &view.msaa_view {
            Some(m) => (m, Some(&view.color_view)),
            None => (&view.color_view, None),
        };
        let mut pass = enc.begin_render_pass(&wgpu::RenderPassDescriptor {
            label: Some("hp-render fog volume pass"),
            color_attachments: &[Some(wgpu::RenderPassColorAttachment {
                view: target,
                depth_slice: None,
                resolve_target: resolve,
                ops: wgpu::Operations {
                    load: wgpu::LoadOp::Load,
                    store: if resolve.is_some() { wgpu::StoreOp::Discard } else { wgpu::StoreOp::Store },
                },
            })],
            depth_stencil_attachment: Some(wgpu::RenderPassDepthStencilAttachment {
                view: &view.depth_view,
                depth_ops: None,
                stencil_ops: None,
            }),
            timestamp_writes: None,
            occlusion_query_set: None,
            multiview_mask: None,
        });
        pass.set_pipeline(pipe);
        pass.set_bind_group(0, self.bind_group, &[]);
        for k in 0..frame.count {
            let offset = u32::try_from(u64::from(k) * state.stride).unwrap_or(0);
            pass.set_bind_group(1, &gpu.bind, &[offset]);
            pass.draw(0..3, 0..1);
            n.add(1);
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::Fog;

    fn fog() -> Fog {
        Fog {
            color: [1.0, 0.5, 0.0],
            color2: [0.0, 0.0, 1.0],
            dir: [0.0, 0.0, 0.0],
            blend: true,
            start: 10.0,
            end: 5.0,
            max_density: 3.0,
            far_z: 0.0,
        }
    }

    fn vol(min: [f32; 3], max: [f32; 3], active: bool) -> FogVolume {
        FogVolume { min, max, fog: Some(fog()), preview: fog(), active }
    }

    #[test]
    fn shader_variants() {
        for ms in [false, true] {
            let src = fog_volume_source(ms);
            assert!(!src.contains('\u{2014}') && !src.contains('\u{2013}'), "dash character");
            assert_eq!(src.contains("texture_depth_multisampled_2d"), ms);
            assert!(src.contains("struct View") && src.contains("fn fs_fog_volume"));
        }
        assert_eq!(size_of::<FogVolumeUniform>(), 96);
    }

    #[test]
    fn uniform_is_sanitized() {
        let u = volume_uniform(&vol([0.0; 3], [64.0; 3], true), [0.1, 2.0]).expect("drawn");
        assert_eq!(u.fog_a[3], 1.0, "density clamps to 1");
        assert_eq!(u.fog_b[3], 10.0);
        assert_eq!(u.fog_c[3], 11.0, "end stays past start");
        assert_eq!([u.fog_c[0], u.fog_c[1], u.fog_c[2]], [1.0, 0.0, 0.0], "a zero direction falls back to +X");
        assert_eq!(u.lo[3], 1.0, "blend flag");
        assert!(u.fog_a[1] < 0.5, "colors are linear");
        assert!(volume_uniform(&vol([0.0; 3], [0.0, 64.0, 64.0], true), [0.1, 2.0]).is_none(), "flat box");
        let off = FogVolume { fog: None, ..vol([0.0; 3], [64.0; 3], true) };
        assert!(volume_uniform(&off, [0.1, 2.0]).is_some(), "the preview stays when the game fog is off");
        let mut thin = vol([0.0; 3], [64.0; 3], true);
        thin.preview.max_density = 0.0;
        assert!(volume_uniform(&thin, [0.1, 2.0]).is_none());
    }

    #[test]
    fn eye_inside_picks_the_first_active_volume() {
        let list =
            [vol([0.0; 3], [100.0; 3], false), vol([50.0; 3], [200.0; 3], true), vol([0.0; 3], [300.0; 3], true)];
        assert!(inside_volume(&list, [10.0; 3]).is_some_and(|v| v.max == [300.0; 3]), "disabled ones are skipped");
        assert!(inside_volume(&list, [60.0; 3]).is_some_and(|v| v.max == [200.0; 3]));
        assert!(inside_volume(&list, [-1.0; 3]).is_none());
    }

    #[test]
    fn drawn_back_to_front() {
        let list = [
            vol([0.0; 3], [10.0; 3], true),
            vol([1000.0; 3], [1010.0; 3], true),
            FogVolume { fog: None, ..vol([500.0; 3], [510.0; 3], true) },
            vol([-5000.0; 3], [-4990.0; 3], true),
        ];
        let order = draw_order(&list, Vec3::splat(-1.0), |_, _| true);
        assert_eq!(order, vec![3, 1, 2, 0]);
        let culled = draw_order(&list, Vec3::splat(-1.0), |min, _| min.x >= 0.0);
        assert_eq!(culled, vec![1, 2, 0]);
    }
}
