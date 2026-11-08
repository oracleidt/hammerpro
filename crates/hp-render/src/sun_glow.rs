use ahash::{HashMap, HashMapExt};
use glam::{Vec3, Vec4};

use crate::camera::Camera;
use crate::pipelines::{self, Blend, Desc};
use crate::types::{RenderMode, SunGlow, ViewOptions, ViewParams, WorldSprite};
use crate::{DrawCtx, FrameCounter, Renderer};

pub(crate) const RANGE: f32 = 0.766_044_4;
const OVERLAY_DISTANCE: f32 = 100.0;
const SIZE_AT_RANGE: f32 = 150.0;
const SIZE_AT_ONE: f32 = 70.0;
const PROXY_SCREEN: f32 = 0.025;

#[repr(C)]
#[derive(Clone, Copy, Debug, Default, PartialEq, bytemuck::Pod, bytemuck::Zeroable)]
pub(crate) struct SunGlowUniform {
    base: [f32; 4],
    right: [f32; 4],
    up: [f32; 4],
    color: [f32; 4],
    proxy: [f32; 4],
    params: [f32; 4],
}

pub(crate) fn applies(params: &ViewParams, opts: &ViewOptions, is_ortho: bool) -> bool {
    !is_ortho && opts.sun_glow && !matches!(params.mode, RenderMode::Wireframe | RenderMode::Flat)
}

pub(crate) fn fade(dot: f32) -> f32 {
    ((dot - RANGE) / (1.0 - RANGE)).clamp(0.0, 1.0)
}

pub(crate) fn sprite_size(dot: f32, size: f32) -> f32 {
    let t = fade(dot);
    (SIZE_AT_RANGE + (SIZE_AT_ONE - SIZE_AT_RANGE) * t) * size
}

pub(crate) fn basis(dir: Vec3, horz: f32, vert: f32) -> (Vec3, Vec3) {
    let mut right = dir.cross(Vec3::Z).normalize_or_zero();
    if right == Vec3::ZERO {
        right = dir.cross(Vec3::Y).normalize_or_zero();
    }
    let up = right.cross(dir).normalize_or_zero();
    (right * horz, up * vert)
}

fn uniforms(glow: &SunGlow, cam: &Camera) -> Vec<(u32, SunGlowUniform)> {
    let dir = Vec3::from(glow.direction).normalize_or_zero();
    if dir == Vec3::ZERO {
        return Vec::new();
    }
    let dot = dir.dot(cam.forward);
    let opacity = fade(dot);
    if dot <= 0.0 {
        return Vec::new();
    }
    let base = cam.eye + dir * OVERLAY_DISTANCE;
    let far = cam.eye + dir * cam.far.max(cam.near * 2.0) * 0.5;
    let clip = cam.view_proj * Vec4::new(far.x, far.y, far.z, 1.0);
    let threshold = if clip.w > 1e-6 { clip.z / clip.w } else { 0.0 };
    let proxy = screen_of(cam, base);
    let radius = PROXY_SCREEN * cam.height;
    let mut out = Vec::new();
    for sprite in [glow.core, glow.overlay] {
        if sprite.size <= 0.0 {
            continue;
        }
        let half = if sprite.modulate { sprite_size(dot, sprite.size) } else { sprite.size };
        let (right, up) = basis(dir, half, half);
        let fade = if sprite.modulate { opacity } else { 1.0 };
        let c = sprite.color.map(|v| v * fade * glow.hdr_scale);
        out.push((
            sprite.material,
            SunGlowUniform {
                base: [base.x, base.y, base.z, threshold],
                right: [right.x, right.y, right.z, 0.0],
                up: [up.x, up.y, up.z, 0.0],
                color: [c[0], c[1], c[2], radius],
                proxy: [proxy[0], proxy[1], 0.0, 0.0],
                params: [0.0; 4],
            },
        ));
    }
    out
}

fn world_uniform(sprite: &WorldSprite, cam: &Camera, hdr: bool) -> Option<(u32, SunGlowUniform)> {
    let pos = Vec3::from(sprite.pos);
    if !pos.is_finite()
        || matches!(sprite.mode, 6 | 10)
        || !sprite.size.iter().all(|v| v.is_finite() && *v > 0.0)
        || !sprite.origin.iter().all(|v| v.is_finite())
    {
        return None;
    }
    let clip = cam.view_proj * pos.extend(1.0);
    if clip.w <= 0.0 {
        return None;
    }
    let distance = (pos - cam.eye).length().max(1e-6);
    let glow = matches!(sprite.mode, 3 | 9);
    let scale = if glow && sprite.mode != 9 && sprite.render_fx != 14 { distance / 200.0 } else { 1.0 };
    let fade = if !glow {
        1.0
    } else if sprite.render_fx == 14 {
        sprite.color[3]
    } else {
        (1200.0 / distance).powi(2).min(1.0)
    };
    let (mut right, mut up);
    let [pitch, yaw, roll] = sprite.angles.map(f32::to_radians);
    match sprite.orientation {
        0 | 1 => {
            let dir = if sprite.orientation == 1 { (pos - cam.eye).normalize() } else { cam.forward };
            right = dir.cross(Vec3::Z).try_normalize()?;
            up = Vec3::Z;
        }
        3 => {
            let (sp, cp) = pitch.sin_cos();
            let (sy, cy) = yaw.sin_cos();
            let (sr, cr) = roll.sin_cos();
            right = Vec3::new(-sr * sp * cy + cr * sy, -sr * sp * sy - cr * cy, -sr * cp);
            up = Vec3::new(cr * sp * cy + sr * sy, cr * sp * sy - sr * cy, cr * cp);
        }
        _ => {
            let (s, c) = roll.sin_cos();
            right = cam.right * c + cam.up * s;
            up = cam.up * c - cam.right * s;
        }
    }
    right *= sprite.size[0] * 0.5 * scale;
    up *= sprite.size[1] * 0.5 * scale;
    let radius = sprite.glow_proxy * cam.height / (2.0 * distance * cam.tan_half_fov);
    let exposure = if hdr { sprite.hdr_scale } else { 1.0 };
    let color = sprite.color.map(|v| v.clamp(0.0, 1.0));
    let rgb = [color[0], color[1], color[2]].map(|v| v * fade);
    let center = screen_of(cam, pos);
    let anchor = pos + right * (1.0 - 2.0 * sprite.origin[0]) + up * (2.0 * sprite.origin[1] - 1.0);
    Some((
        sprite.material,
        SunGlowUniform {
            base: [anchor.x, anchor.y, anchor.z, clip.z / clip.w],
            right: [right.x, right.y, right.z, color[3]],
            up: [up.x, up.y, up.z, f32::from(sprite.srgb_color)],
            color: [rgb[0], rgb[1], rgb[2], radius],
            proxy: [center[0], center[1], 1.0, sprite.mode as f32],
            params: [exposure, 0.0, 0.0, 0.0],
        },
    ))
}

fn screen_of(cam: &Camera, p: Vec3) -> [f32; 2] {
    let clip = cam.view_proj * Vec4::new(p.x, p.y, p.z, 1.0);
    if clip.w <= 1e-6 {
        return [cam.width * 0.5, cam.height * 0.5];
    }
    let ndc = clip.truncate() / clip.w;
    [(ndc.x + 1.0) * 0.5 * cam.width, (1.0 - ndc.y) * 0.5 * cam.height]
}

pub(crate) struct SunGlowFrame {
    pub materials: Vec<u32>,
    pub blends: Vec<usize>,
}

struct SunViewGpu {
    generation: u64,
    capacity: usize,
    buffer: wgpu::Buffer,
    bind: wgpu::BindGroup,
}

pub(crate) struct SunGlowState {
    bgl: [wgpu::BindGroupLayout; 2],
    layouts: [wgpu::PipelineLayout; 2],
    modules: [wgpu::ShaderModule; 2],
    pipes: [[Option<wgpu::RenderPipeline>; 4]; 2],
    views: HashMap<u32, SunViewGpu>,
    stride: u64,
}

pub(crate) fn sun_glow_source(multisampled: bool) -> String {
    if multisampled {
        pipelines::SUN_GLOW_WGSL
            .replace("var scene_depth: texture_depth_2d;", "var scene_depth: texture_depth_multisampled_2d;")
    } else {
        pipelines::SUN_GLOW_WGSL.to_owned()
    }
}

impl SunGlowState {
    fn new(device: &wgpu::Device, view_bgl: &wgpu::BindGroupLayout, mat_bgl: &wgpu::BindGroupLayout) -> Self {
        let fragment = wgpu::ShaderStages::FRAGMENT;
        let both = wgpu::ShaderStages::VERTEX_FRAGMENT;
        let bgl = [false, true].map(|multisampled| {
            device.create_bind_group_layout(&wgpu::BindGroupLayoutDescriptor {
                label: Some("hp-render sun glow"),
                entries: &[
                    wgpu::BindGroupLayoutEntry {
                        binding: 0,
                        visibility: both,
                        ty: wgpu::BindingType::Buffer {
                            ty: wgpu::BufferBindingType::Uniform,
                            has_dynamic_offset: true,
                            min_binding_size: wgpu::BufferSize::new(size_of::<SunGlowUniform>() as u64),
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
                label: Some("hp-render sun glow layout"),
                bind_group_layouts: &[Some(view_bgl), Some(mat_bgl), Some(&bgl[i])],
                immediate_size: 0,
            })
        });
        let modules = [false, true].map(|multisampled| {
            device.create_shader_module(wgpu::ShaderModuleDescriptor {
                label: Some("hp-render sun glow"),
                source: wgpu::ShaderSource::Wgsl(sun_glow_source(multisampled).into()),
            })
        });
        let align = u64::from(device.limits().min_uniform_buffer_offset_alignment.max(1));
        let stride = (size_of::<SunGlowUniform>() as u64).div_ceil(align) * align;
        Self {
            bgl,
            layouts,
            modules,
            pipes: std::array::from_fn(|_| Default::default()),
            views: HashMap::new(),
            stride,
        }
    }

    fn ensure_pipe(&mut self, device: &wgpu::Device, samples: u32) {
        let i = usize::from(samples > 1);
        for (k, blend) in [Blend::Additive, Blend::Alpha, Blend::Replace, Blend::Premultiplied].into_iter().enumerate()
        {
            if self.pipes[i][k].is_some() {
                continue;
            }
            self.pipes[i][k] = Some(pipelines::build(
                device,
                samples,
                Desc {
                    label: "hp-render sprite glow",
                    module: &self.modules[i],
                    vs: "vs_sun_glow",
                    fs: "fs_sun_glow",
                    layout: &self.layouts[i],
                    buffers: &[],
                    topology: wgpu::PrimitiveTopology::TriangleList,
                    cull: None,
                    depth_test: false,
                    depth_write: false,
                    blend,
                    constants: &[],
                },
            ));
        }
    }
}

impl Renderer {
    pub fn set_world_sprites(&mut self, sprites: &[WorldSprite]) {
        if self.world_sprites != sprites {
            self.world_sprites.clear();
            self.world_sprites.extend_from_slice(sprites);
            self.content_gen += 1;
        }
    }

    pub fn set_sun_glow(&mut self, glow: Option<SunGlow>) {
        if self.sun_glow != glow {
            self.sun_glow = glow;
            self.content_gen += 1;
        }
    }

    pub fn sun_glow(&self) -> Option<SunGlow> {
        self.sun_glow
    }

    pub(crate) fn prepare_sun_glow(
        &mut self,
        view_id: u32,
        cam: &Camera,
        params: &ViewParams,
        opts: &ViewOptions,
    ) -> Option<SunGlowFrame> {
        let mut list = if applies(params, opts, cam.is_ortho) {
            self.sun_glow.map_or_else(Vec::new, |glow| uniforms(&glow, cam))
        } else {
            Vec::new()
        };
        if !cam.is_ortho && params.show_sprites && !matches!(params.mode, RenderMode::Wireframe | RenderMode::Flat) {
            let hdr = self.hdr_lit(opts, params.mode, false);
            let mut visible: Vec<_> = self
                .world_sprites
                .iter()
                .filter(|sprite| {
                    !self
                        .scene
                        .flags(sprite.object)
                        .unwrap_or_default()
                        .intersects(crate::ObjectFlags::HIDDEN | crate::ObjectFlags::NO_3D)
                })
                .collect();
            visible.sort_by(|a, b| {
                let distance = |sprite: &WorldSprite| (Vec3::from(sprite.pos) - cam.eye).length_squared();
                distance(b).total_cmp(&distance(a)).then_with(|| a.object.cmp(&b.object))
            });
            list.extend(visible.into_iter().filter_map(|sprite| world_uniform(sprite, cam, hdr)));
        }
        let align = u64::from(self.device.limits().min_uniform_buffer_offset_alignment.max(1));
        let stride = (size_of::<SunGlowUniform>() as u64).div_ceil(align) * align;
        let capacity = (self.max_buffer / stride).min(u64::from(u32::MAX) / stride) as usize;
        list.truncate(capacity);
        if list.is_empty() {
            return None;
        }
        for (material, _) in &mut list {
            *material = self.scene.material_slot(*material);
        }
        self.sync_materials();
        let (generation, samples) = {
            let v = self.views.get(&view_id)?;
            (v.generation, v.samples)
        };
        if self.sun_glow_gpu.is_none() {
            self.sun_glow_gpu = Some(SunGlowState::new(&self.device, &self.shared.view_bgl, &self.shared.material_bgl));
        }
        let state = self.sun_glow_gpu.as_mut()?;
        state.ensure_pipe(&self.device, samples);
        let stride = state.stride;
        if state.views.get(&view_id).is_none_or(|v| v.generation != generation || v.capacity < list.len()) {
            let view = self.views.get(&view_id)?;
            let buffer = self.device.create_buffer(&wgpu::BufferDescriptor {
                label: Some("hp-render sun glow uniforms"),
                size: stride * list.len() as u64,
                usage: wgpu::BufferUsages::UNIFORM | wgpu::BufferUsages::COPY_DST,
                mapped_at_creation: false,
            });
            let bind = self.device.create_bind_group(&wgpu::BindGroupDescriptor {
                label: Some("hp-render sun glow"),
                layout: &state.bgl[usize::from(samples > 1)],
                entries: &[
                    wgpu::BindGroupEntry {
                        binding: 0,
                        resource: wgpu::BindingResource::Buffer(wgpu::BufferBinding {
                            buffer: &buffer,
                            offset: 0,
                            size: wgpu::BufferSize::new(size_of::<SunGlowUniform>() as u64),
                        }),
                    },
                    wgpu::BindGroupEntry { binding: 1, resource: wgpu::BindingResource::TextureView(&view.depth_view) },
                ],
            });
            state.views.insert(view_id, SunViewGpu { generation, capacity: list.len(), buffer, bind });
        }
        let gpu = state.views.get(&view_id)?;
        let mut bytes = vec![0u8; stride as usize * list.len()];
        for (k, (_, u)) in list.iter().enumerate() {
            let at = k * stride as usize;
            bytes[at..at + size_of::<SunGlowUniform>()].copy_from_slice(bytemuck::bytes_of(u));
        }
        self.queue.write_buffer(&gpu.buffer, 0, &bytes);
        let blends = list
            .iter()
            .map(|(_, u)| {
                if u.proxy[2] < 0.5 {
                    0
                } else {
                    match u.proxy[3] as u32 {
                        0 => 2,
                        1 | 2 | 4 => 1,
                        8 => 3,
                        _ => 0,
                    }
                }
            })
            .collect();
        Some(SunGlowFrame { materials: list.into_iter().map(|(m, _)| m).collect(), blends })
    }

    pub(crate) fn forget_sun_glow_view(&mut self, view_id: u32) {
        if let Some(s) = self.sun_glow_gpu.as_mut() {
            s.views.remove(&view_id);
        }
    }
}

impl DrawCtx<'_> {
    pub(crate) fn record_sun_glow(
        &self,
        enc: &mut wgpu::CommandEncoder,
        frame: &SunGlowFrame,
        view_id: u32,
        n: &mut FrameCounter,
    ) {
        let view = self.view;
        let Some(state) = self.r.sun_glow_gpu.as_ref() else { return };
        let Some(gpu) = state.views.get(&view_id) else { return };
        let (target, resolve) = match &view.msaa_view {
            Some(m) => (m, Some(&view.color_view)),
            None => (&view.color_view, None),
        };
        let mut pass = enc.begin_render_pass(&wgpu::RenderPassDescriptor {
            label: Some("hp-render sun glow pass"),
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
        pass.set_bind_group(0, self.bind_group, &[]);
        for (k, material) in frame.materials.iter().enumerate() {
            let Some(mg) = self.r.mat_gpu.get(*material as usize) else { continue };
            let Some(pipe) = state.pipes[usize::from(view.samples > 1)][frame.blends[k]].as_ref() else { continue };
            pass.set_pipeline(pipe);
            pass.set_bind_group(1, &mg.bind_group, &[]);
            pass.set_bind_group(2, &gpu.bind, &[(k as u64 * state.stride) as u32]);
            pass.draw(0..6, 0..1);
            n.add(2);
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn the_glow_fades_out_at_forty_degrees_from_the_view() {
        assert_eq!(fade(RANGE - 0.01), 0.0);
        assert!((fade(1.0) - 1.0).abs() < 1e-6);
        assert!((fade((RANGE + 1.0) * 0.5) - 0.5).abs() < 1e-3);
    }

    #[test]
    fn the_sprite_shrinks_as_the_sun_comes_to_the_middle_of_the_view() {
        assert!((sprite_size(1.0, 16.0) - 1120.0).abs() < 1e-3);
        assert!((sprite_size(RANGE, 16.0) - 2400.0).abs() < 1e-3);
    }

    #[test]
    fn the_basis_is_square_to_the_sun_and_level_with_the_world() {
        let dir = Vec3::new(0.3, -0.4, 0.866).normalize();
        let (right, up) = basis(dir, 10.0, 20.0);
        assert!(right.dot(dir).abs() < 1e-4 && up.dot(dir).abs() < 1e-4);
        assert!(right.z.abs() < 1e-4);
        assert!((right.length() - 10.0).abs() < 1e-3 && (up.length() - 20.0).abs() < 1e-3);
    }
}
