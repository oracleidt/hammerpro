use bytemuck::{Pod, Zeroable};
use glam::{Mat4, Vec3, Vec4};

use crate::Renderer;
use crate::camera::frustum_planes;
use crate::gpu::ViewUniform;
use crate::local_shadow::map_proj;
use crate::pipelines::{DEPTH_FORMAT, ShadowPipelines};

pub const MAX_PROJECTORS: usize = 16;
pub const PROJECTOR_TEXTURE_SIZE: u32 = 256;
pub(crate) const MAX_SHADOWED: usize = 4;
pub(crate) const SHADOW_SIZE: u32 = 1024;

#[derive(Clone, Debug, PartialEq)]
pub struct ProjectorImage {
    pub width: u32,
    pub height: u32,
    pub rgba: Vec<u8>,
}

#[derive(Clone, Debug, PartialEq)]
pub struct Projector {
    pub pos: [f32; 3],
    pub forward: [f32; 3],
    pub right: [f32; 3],
    pub up: [f32; 3],
    pub fov_deg: f32,
    pub near_z: f32,
    pub far_z: f32,
    pub color: [f32; 3],
    pub attenuation: [f32; 3],
    pub shadows: bool,
    pub image: Option<ProjectorImage>,
}

impl Default for Projector {
    fn default() -> Self {
        Self {
            pos: [0.0; 3],
            forward: [1.0, 0.0, 0.0],
            right: [0.0, -1.0, 0.0],
            up: [0.0, 0.0, 1.0],
            fov_deg: 45.0,
            near_z: 4.0,
            far_z: 750.0,
            color: [1.0; 3],
            attenuation: [1.0, 0.0, 0.0],
            shadows: false,
            image: None,
        }
    }
}

#[repr(C)]
#[derive(Clone, Copy, Debug, Default, PartialEq, Pod, Zeroable)]
pub(crate) struct ProjectorGpu {
    pos_near: [f32; 4],
    fwd_far: [f32; 4],
    right_tan: [f32; 4],
    up_layer: [f32; 4],
    color_const: [f32; 4],
    lin_quad: [f32; 4],
    shadow_vp: [[f32; 4]; 4],
}

struct ShadowMaps {
    view: wgpu::TextureView,
    layers: Vec<wgpu::TextureView>,
    uniforms: Vec<wgpu::Buffer>,
    binds: Vec<Option<(u64, wgpu::BindGroup)>>,
}

pub(crate) struct Projectors {
    pub(crate) buffer: wgpu::Buffer,
    pub(crate) view: wgpu::TextureView,
    texture: wgpu::Texture,
    list: Vec<Projector>,
    layers_with_image: [bool; MAX_PROJECTORS],
    shadow: Option<ShadowMaps>,
    pipelines: Option<ShadowPipelines>,
    drawn: [u64; MAX_SHADOWED],
}

fn finite3(v: [f32; 3], fallback: [f32; 3]) -> [f32; 3] {
    if v.iter().all(|c| c.is_finite()) { v } else { fallback }
}

fn unit(v: [f32; 3], fallback: [f32; 3]) -> [f32; 3] {
    let v = finite3(v, fallback);
    let len = (v[0] * v[0] + v[1] * v[1] + v[2] * v[2]).sqrt();
    if len > 1e-6 { [v[0] / len, v[1] / len, v[2] / len] } else { fallback }
}

pub(crate) fn sanitize(p: &Projector) -> Projector {
    let fin = |v: f32, d: f32| if v.is_finite() { v } else { d };
    let near = fin(p.near_z, 4.0).clamp(0.01, 1.0e5);
    Projector {
        pos: finite3(p.pos, [0.0; 3]),
        forward: unit(p.forward, [1.0, 0.0, 0.0]),
        right: unit(p.right, [0.0, -1.0, 0.0]),
        up: unit(p.up, [0.0, 0.0, 1.0]),
        fov_deg: fin(p.fov_deg, 45.0).clamp(1.0, 170.0),
        near_z: near,
        far_z: fin(p.far_z, 750.0).clamp(near + 0.01, 1.0e6),
        color: finite3(p.color, [1.0; 3]).map(|c| c.clamp(0.0, 1.0e3)),
        attenuation: finite3(p.attenuation, [1.0, 0.0, 0.0]).map(|c| c.clamp(0.0, 1.0e6)),
        shadows: p.shadows,
        image: p.image.clone().filter(|i| {
            i.width > 0 && i.height > 0 && i.rgba.len() as u64 == u64::from(i.width) * u64::from(i.height) * 4
        }),
    }
}

pub(crate) fn shadow_matrix(p: &Projector) -> Mat4 {
    let tan_half = (p.fov_deg.to_radians() * 0.5).tan();
    map_proj(Vec3::from(p.pos), Vec3::from(p.forward), Vec3::from(p.up), tan_half, p.far_z)
}

pub(crate) fn gpu_record(p: &Projector, layer: Option<usize>, shadow: Option<usize>) -> ProjectorGpu {
    let tan_half = (p.fov_deg.to_radians() * 0.5).tan();
    ProjectorGpu {
        pos_near: [p.pos[0], p.pos[1], p.pos[2], p.near_z],
        fwd_far: [p.forward[0], p.forward[1], p.forward[2], p.far_z],
        right_tan: [p.right[0], p.right[1], p.right[2], tan_half],
        up_layer: [p.up[0], p.up[1], p.up[2], layer.map_or(-1.0, |l| l as f32)],
        color_const: [p.color[0], p.color[1], p.color[2], p.attenuation[0]],
        lin_quad: [p.attenuation[1], p.attenuation[2], 1.0, shadow.map_or(-1.0, |l| l as f32)],
        shadow_vp: shadow.map_or([[0.0; 4]; 4], |_| shadow_matrix(p).to_cols_array_2d()),
    }
}

pub(crate) fn resample(image: &ProjectorImage, size: u32) -> Vec<u8> {
    let (w, h) = (image.width as usize, image.height as usize);
    let mut out = vec![0u8; (size * size * 4) as usize];
    for y in 0..size as usize {
        let sy = ((y as f32 + 0.5) * h as f32 / size as f32 - 0.5).clamp(0.0, (h - 1) as f32);
        let (y0, fy) = (sy.floor() as usize, sy - sy.floor());
        let (y0, y1) = (y0.min(h - 1), (y0 + 1).min(h - 1));
        for x in 0..size as usize {
            let sx = ((x as f32 + 0.5) * w as f32 / size as f32 - 0.5).clamp(0.0, (w - 1) as f32);
            let (x0, fx) = (sx.floor() as usize, sx - sx.floor());
            let (x0, x1) = (x0.min(w - 1), (x0 + 1).min(w - 1));
            for c in 0..4 {
                let px = |xx: usize, yy: usize| f32::from(image.rgba[(yy * w + xx) * 4 + c]);
                let top = px(x0, y0) * (1.0 - fx) + px(x1, y0) * fx;
                let bottom = px(x0, y1) * (1.0 - fx) + px(x1, y1) * fx;
                out[(y * size as usize + x) * 4 + c] = (top * (1.0 - fy) + bottom * fy).round().clamp(0.0, 255.0) as u8;
            }
        }
    }
    out
}

impl ShadowMaps {
    fn new(device: &wgpu::Device) -> Self {
        let texture = device.create_texture(&wgpu::TextureDescriptor {
            label: Some("hp-render projector shadows"),
            size: wgpu::Extent3d {
                width: SHADOW_SIZE,
                height: SHADOW_SIZE,
                depth_or_array_layers: MAX_SHADOWED as u32,
            },
            mip_level_count: 1,
            sample_count: 1,
            dimension: wgpu::TextureDimension::D2,
            format: DEPTH_FORMAT,
            usage: wgpu::TextureUsages::RENDER_ATTACHMENT | wgpu::TextureUsages::TEXTURE_BINDING,
            view_formats: &[],
        });
        let view = texture.create_view(&wgpu::TextureViewDescriptor {
            dimension: Some(wgpu::TextureViewDimension::D2Array),
            ..Default::default()
        });
        let layers = (0..MAX_SHADOWED as u32)
            .map(|l| {
                texture.create_view(&wgpu::TextureViewDescriptor {
                    dimension: Some(wgpu::TextureViewDimension::D2),
                    base_array_layer: l,
                    array_layer_count: Some(1),
                    ..Default::default()
                })
            })
            .collect();
        let uniforms = (0..MAX_SHADOWED)
            .map(|_| {
                device.create_buffer(&wgpu::BufferDescriptor {
                    label: Some("hp-render projector shadow uniform"),
                    size: size_of::<ViewUniform>() as u64,
                    usage: wgpu::BufferUsages::UNIFORM | wgpu::BufferUsages::COPY_DST,
                    mapped_at_creation: false,
                })
            })
            .collect();
        Self { view, layers, uniforms, binds: (0..MAX_SHADOWED).map(|_| None).collect() }
    }
}

impl Projectors {
    pub(crate) fn new(device: &wgpu::Device, queue: &wgpu::Queue) -> Self {
        let buffer = device.create_buffer(&wgpu::BufferDescriptor {
            label: Some("hp-render projectors"),
            size: (std::mem::size_of::<ProjectorGpu>() * MAX_PROJECTORS) as u64,
            usage: wgpu::BufferUsages::STORAGE | wgpu::BufferUsages::COPY_DST,
            mapped_at_creation: false,
        });
        queue.write_buffer(&buffer, 0, bytemuck::cast_slice(&[ProjectorGpu::default(); MAX_PROJECTORS]));
        let texture = device.create_texture(&wgpu::TextureDescriptor {
            label: Some("hp-render projector textures"),
            size: wgpu::Extent3d {
                width: PROJECTOR_TEXTURE_SIZE,
                height: PROJECTOR_TEXTURE_SIZE,
                depth_or_array_layers: MAX_PROJECTORS as u32,
            },
            mip_level_count: 1,
            sample_count: 1,
            dimension: wgpu::TextureDimension::D2,
            format: wgpu::TextureFormat::Rgba8UnormSrgb,
            usage: wgpu::TextureUsages::TEXTURE_BINDING | wgpu::TextureUsages::COPY_DST,
            view_formats: &[],
        });
        let view = texture.create_view(&wgpu::TextureViewDescriptor {
            dimension: Some(wgpu::TextureViewDimension::D2Array),
            ..Default::default()
        });
        Self {
            buffer,
            view,
            texture,
            list: Vec::new(),
            layers_with_image: [false; MAX_PROJECTORS],
            shadow: None,
            pipelines: None,
            drawn: [0; MAX_SHADOWED],
        }
    }

    fn write_layer(&self, queue: &wgpu::Queue, layer: usize, rgba: &[u8]) {
        queue.write_texture(
            wgpu::TexelCopyTextureInfo {
                texture: &self.texture,
                mip_level: 0,
                origin: wgpu::Origin3d { x: 0, y: 0, z: layer as u32 },
                aspect: wgpu::TextureAspect::All,
            },
            rgba,
            wgpu::TexelCopyBufferLayout {
                offset: 0,
                bytes_per_row: Some(PROJECTOR_TEXTURE_SIZE * 4),
                rows_per_image: Some(PROJECTOR_TEXTURE_SIZE),
            },
            wgpu::Extent3d { width: PROJECTOR_TEXTURE_SIZE, height: PROJECTOR_TEXTURE_SIZE, depth_or_array_layers: 1 },
        );
    }

    pub(crate) fn set(&mut self, device: &wgpu::Device, queue: &wgpu::Queue, list: &[Projector]) -> bool {
        let clean: Vec<Projector> = list.iter().take(MAX_PROJECTORS).map(sanitize).collect();
        if clean == self.list {
            return false;
        }
        let mut records = [ProjectorGpu::default(); MAX_PROJECTORS];
        let mut next_shadow = 0usize;
        for (i, p) in clean.iter().enumerate() {
            let same_image = self.list.get(i).is_some_and(|old| old.image == p.image);
            if !same_image || (p.image.is_none() && self.layers_with_image[i]) {
                if let Some(image) = &p.image {
                    self.write_layer(queue, i, &resample(image, PROJECTOR_TEXTURE_SIZE));
                    self.layers_with_image[i] = true;
                } else if self.layers_with_image[i] {
                    let white = vec![255u8; (PROJECTOR_TEXTURE_SIZE * PROJECTOR_TEXTURE_SIZE * 4) as usize];
                    self.write_layer(queue, i, &white);
                    self.layers_with_image[i] = false;
                }
            }
            let shadow = (p.shadows && next_shadow < MAX_SHADOWED).then(|| {
                next_shadow += 1;
                next_shadow - 1
            });
            records[i] = gpu_record(p, p.image.as_ref().map(|_| i), shadow);
        }
        if next_shadow > 0 && self.shadow.is_none() {
            self.shadow = Some(ShadowMaps::new(device));
        }
        queue.write_buffer(&self.buffer, 0, bytemuck::cast_slice(&records));
        self.list = clean;
        self.drawn = [0; MAX_SHADOWED];
        true
    }

    pub(crate) fn shadow_view(&self) -> Option<&wgpu::TextureView> {
        self.shadow.as_ref().map(|s| &s.view)
    }

    pub(crate) fn has_shadow_maps(&self) -> bool {
        self.shadow.is_some()
    }

    pub(crate) fn count(&self) -> usize {
        self.list.len()
    }
}

impl Renderer {
    pub fn set_projectors(&mut self, list: &[Projector]) {
        let had_maps = self.projectors.has_shadow_maps();
        if self.projectors.set(&self.device, &self.queue, list) {
            self.content_gen += 1;
            self.shadow_epoch += 1;
            if had_maps != self.projectors.has_shadow_maps() {
                self.scene_gen += 1;
            }
        }
    }

    pub fn projector_count(&self) -> usize {
        self.projectors.count()
    }

    pub(crate) fn projector_shadow_view(&self) -> Option<&wgpu::TextureView> {
        self.projectors.shadow_view()
    }

    pub(crate) fn record_projector_shadows(&mut self, enc: &mut wgpu::CommandEncoder) -> (u32, u64) {
        let epoch = self.shadow_epoch;
        let scene_gen = self.scene_gen;
        if self.projectors.shadow.is_none() {
            return (0, 0);
        }
        if self.projectors.pipelines.is_none() {
            self.projectors.pipelines = Some(ShadowPipelines::new(&self.device, &self.shared));
        }
        let mut todo: Vec<(usize, Mat4)> = Vec::new();
        let mut slot = 0usize;
        for p in &self.projectors.list {
            if !p.shadows || slot >= MAX_SHADOWED {
                continue;
            }
            if self.projectors.drawn[slot] != epoch {
                todo.push((slot, shadow_matrix(p)));
            }
            slot += 1;
        }
        let mut binds: Vec<(usize, wgpu::BindGroup)> = Vec::new();
        for (slot, m) in &todo {
            let u = ViewUniform {
                view_proj: m.to_cols_array_2d(),
                eye: [0.0; 4],
                viewport: [1.0; 4],
                opts: [crate::OPT_SHADOW_PASS, 0, 0, 0],
                ..Default::default()
            };
            let Some(maps) = self.projectors.shadow.as_ref() else { continue };
            self.queue.write_buffer(&maps.uniforms[*slot], 0, bytemuck::bytes_of(&u));
            if maps.binds[*slot].as_ref().is_none_or(|(g, _)| *g != scene_gen) {
                let bg = self.shadow_pass_bind_group(&maps.uniforms[*slot], "hp-render projector shadow pass");
                binds.push((*slot, bg));
            }
        }
        if let Some(maps) = self.projectors.shadow.as_mut() {
            for (slot, bg) in binds {
                maps.binds[slot] = Some((scene_gen, bg));
            }
        }
        let (mut draws, mut tris) = (0u32, 0u64);
        let (Some(maps), Some(pipelines)) = (self.projectors.shadow.as_ref(), self.projectors.pipelines.as_ref())
        else {
            return (0, 0);
        };
        for (slot, m) in &todo {
            let Some((_, bg)) = maps.binds[*slot].as_ref() else { continue };
            let mut pass = enc.begin_render_pass(&wgpu::RenderPassDescriptor {
                label: Some("hp-render projector shadow pass"),
                color_attachments: &[],
                depth_stencil_attachment: Some(wgpu::RenderPassDepthStencilAttachment {
                    view: &maps.layers[*slot],
                    depth_ops: Some(wgpu::Operations { load: wgpu::LoadOp::Clear(1.0), store: wgpu::StoreOp::Store }),
                    stencil_ops: None,
                }),
                timestamp_writes: None,
                occlusion_query_set: None,
                multiview_mask: None,
            });
            pass.set_bind_group(0, bg, &[]);
            let planes: [Vec4; 6] = frustum_planes(m);
            let packed = self.pack_visible(&planes);
            let (d, t) = self.draw_shadow_casters(&mut pass, pipelines, &planes, None, packed.as_ref(), false);
            draws += d;
            tris += t;
        }
        for (slot, _) in &todo {
            self.projectors.drawn[*slot] = epoch;
        }
        (draws, tris)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn records_carry_the_frustum() {
        let p = Projector { fov_deg: 90.0, near_z: 4.0, far_z: 300.0, ..Default::default() };
        let g = gpu_record(&sanitize(&p), Some(2), Some(1));
        assert!((g.right_tan[3] - 1.0).abs() < 1e-5);
        assert_eq!((g.pos_near[3], g.fwd_far[3], g.up_layer[3]), (4.0, 300.0, 2.0));
        assert_eq!((g.lin_quad[2], g.lin_quad[3]), (1.0, 1.0));
        assert_eq!(gpu_record(&p, None, None).up_layer[3], -1.0);
        assert_eq!(gpu_record(&p, None, None).lin_quad[3], -1.0);
    }

    #[test]
    fn the_shadow_matrix_maps_the_frustum_axis_to_the_middle() {
        let p = Projector {
            pos: [0.0; 3],
            forward: [0.0, 1.0, 0.0],
            right: [1.0, 0.0, 0.0],
            up: [0.0, 0.0, 1.0],
            ..Default::default()
        };
        let m = shadow_matrix(&sanitize(&p));
        let c = m * Vec4::new(0.0, 100.0, 0.0, 1.0);
        let ndc = c.truncate() / c.w;
        assert!(ndc.x.abs() < 1e-5 && ndc.y.abs() < 1e-5 && ndc.z > 0.0 && ndc.z < 1.0);
        let side = m * Vec4::new(100.0 * 22.5f32.to_radians().tan(), 100.0, 0.0, 1.0);
        assert!((side.x / side.w).abs() < 1.0, "inside the frustum width");
    }

    #[test]
    fn sanitize_rejects_bad_numbers() {
        let p = Projector {
            pos: [f32::NAN, 0.0, 0.0],
            forward: [0.0; 3],
            fov_deg: 400.0,
            near_z: -5.0,
            far_z: 0.0,
            color: [f32::INFINITY, 2.0, -1.0],
            image: Some(ProjectorImage { width: 2, height: 2, rgba: vec![0; 3] }),
            ..Default::default()
        };
        let s = sanitize(&p);
        assert_eq!(s.pos, [0.0; 3]);
        assert_eq!(s.forward, [1.0, 0.0, 0.0]);
        assert_eq!(s.fov_deg, 170.0);
        assert!(s.near_z > 0.0 && s.far_z > s.near_z);
        assert_eq!(s.color, [1.0; 3]);
        let q = sanitize(&Projector { color: [0.5, 2.0, -1.0], ..Default::default() });
        assert_eq!(q.color, [0.5, 2.0, -1.0].map(|c: f32| c.clamp(0.0, 1.0e3)));
        assert!(s.image.is_none());
    }

    #[test]
    fn resample_keeps_flat_colors_and_orientation() {
        let image = ProjectorImage {
            width: 2,
            height: 2,
            rgba: vec![255, 0, 0, 255, 0, 255, 0, 255, 0, 0, 255, 255, 255, 255, 255, 255],
        };
        let out = resample(&image, 4);
        assert_eq!(&out[0..4], &[255, 0, 0, 255]);
        let last = out.len() - 4;
        assert_eq!(&out[last..], &[255, 255, 255, 255]);
        let flat = ProjectorImage { width: 3, height: 5, rgba: [10, 20, 30, 255].repeat(15) };
        assert!(resample(&flat, 8).chunks(4).all(|c| c == [10, 20, 30, 255]));
    }
}
