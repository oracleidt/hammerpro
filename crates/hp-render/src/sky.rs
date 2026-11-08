use glam::{Mat4, Vec3};

use crate::camera::Camera;
use crate::{CompiledLightMode, Renderer, Sky3d, TexId, ViewOptions};

#[derive(Clone, Copy, Debug)]
pub struct HdrSkyFace<'a> {
    pub width: u32,
    pub height: u32,
    pub texels: &'a [[f32; 3]],
}

pub(crate) struct HdrSky {
    _textures: Vec<wgpu::Texture>,
    bind: wgpu::BindGroup,
}

impl Renderer {
    pub fn set_hdr_sky(&mut self, faces: Option<&[HdrSkyFace<'_>; 6]>) {
        let had = self.hdr_sky.take().is_some();
        let Some(faces) = faces else {
            if had {
                self.content_gen += 1;
            }
            return;
        };
        let ok = |f: &HdrSkyFace<'_>| {
            f.width > 0
                && f.height > 0
                && f.width <= self.max_tex_dim
                && f.height <= self.max_tex_dim
                && (f.width as usize) * (f.height as usize) == f.texels.len()
        };
        if !faces.iter().all(ok) {
            if had {
                self.content_gen += 1;
            }
            return;
        }
        let textures: Vec<wgpu::Texture> = faces
            .iter()
            .map(|f| {
                let size = wgpu::Extent3d { width: f.width, height: f.height, depth_or_array_layers: 1 };
                let texture = self.device.create_texture(&wgpu::TextureDescriptor {
                    label: Some("hp-render HDR sky face"),
                    size,
                    mip_level_count: 1,
                    sample_count: 1,
                    dimension: wgpu::TextureDimension::D2,
                    format: wgpu::TextureFormat::Rgba16Float,
                    usage: wgpu::TextureUsages::TEXTURE_BINDING | wgpu::TextureUsages::COPY_DST,
                    view_formats: &[],
                });
                let clean = |v: f32| if v.is_finite() { v.clamp(0.0, 65504.0) } else { 0.0 };
                let half = crate::compiled_light::f16_bits;
                let data: Vec<u16> = f
                    .texels
                    .iter()
                    .flat_map(|c| [half(clean(c[0])), half(clean(c[1])), half(clean(c[2])), half(1.0)])
                    .collect();
                self.queue.write_texture(
                    texture.as_image_copy(),
                    bytemuck::cast_slice(&data),
                    wgpu::TexelCopyBufferLayout { offset: 0, bytes_per_row: Some(f.width * 8), rows_per_image: None },
                    size,
                );
                texture
            })
            .collect();
        let views: Vec<wgpu::TextureView> =
            textures.iter().map(|t| t.create_view(&wgpu::TextureViewDescriptor::default())).collect();
        let entries: Vec<wgpu::BindGroupEntry<'_>> = views
            .iter()
            .enumerate()
            .map(|(i, v)| wgpu::BindGroupEntry { binding: i as u32, resource: wgpu::BindingResource::TextureView(v) })
            .collect();
        let bind = self.device.create_bind_group(&wgpu::BindGroupDescriptor {
            label: Some("hp-render HDR sky faces"),
            layout: &self.shared.sky_bgl,
            entries: &entries,
        });
        self.hdr_sky = Some(HdrSky { _textures: textures, bind });
        self.content_gen += 1;
    }

    pub fn has_hdr_sky(&self) -> bool {
        self.hdr_sky.is_some()
    }

    pub(crate) fn hdr_sky_bind(
        &self,
        opts: &ViewOptions,
        mode: crate::RenderMode,
        ortho: bool,
    ) -> Option<&wgpu::BindGroup> {
        let compiled = opts.compiled_lighting
            && crate::compiled_light::textured_mode(mode)
            && self.compiled.mode() == Some(CompiledLightMode::Hdr);
        let baked = self.lightmap_display() == crate::lightmap::LightmapDisplay::Baked && !self.lightmap_ldr();
        let hdr = !ortho && (compiled || baked || self.hdr_lit(opts, mode, ortho));
        self.hdr_sky.as_ref().filter(|_| hdr).map(|s| &s.bind)
    }

    pub(crate) fn ensure_sky_bind(&mut self, faces: [Option<TexId>; 6]) {
        if self.sky_bind.as_ref().is_some_and(|(f, _)| *f == faces) {
            return;
        }
        let views: Vec<&wgpu::TextureView> = faces
            .iter()
            .filter_map(|f| {
                let id = f.filter(|t| self.textures.contains_key(&t.0)).unwrap_or(TexId::LOADING);
                self.texture_view_or_missing(id)
            })
            .collect();
        if views.len() != 6 {
            self.sky_bind = None;
            return;
        }
        let entries: Vec<wgpu::BindGroupEntry<'_>> = views
            .iter()
            .enumerate()
            .map(|(i, v)| wgpu::BindGroupEntry { binding: i as u32, resource: wgpu::BindingResource::TextureView(v) })
            .collect();
        let bg = self.device.create_bind_group(&wgpu::BindGroupDescriptor {
            label: Some("hp-render sky faces"),
            layout: &self.shared.sky_bgl,
            entries: &entries,
        });
        self.sky_bind = Some((faces, bg));
    }
}

pub(crate) fn sky_camera(cam: &Camera, sky: &Sky3d) -> Option<Camera> {
    if cam.is_ortho || !(sky.scale.is_finite() && sky.scale >= 1.0) {
        return None;
    }
    let origin = Vec3::from(sky.origin);
    if !origin.is_finite() {
        return None;
    }
    let inv = 1.0 / sky.scale;
    let eye = origin + cam.eye * inv;
    let near = (cam.near * inv).max(1e-3);
    let far = (cam.far * inv).max(near * 2.0);
    let mut c = *cam;
    c.eye = eye;
    c.near = near;
    c.far = far;
    c.view_proj = perspective(eye, cam.right, cam.up, cam.forward, cam.tan_half_fov, cam.width / cam.height, near, far);
    c.planes = crate::camera::frustum_planes(&c.view_proj);
    Some(c)
}

#[allow(clippy::too_many_arguments)]
fn perspective(eye: Vec3, right: Vec3, up: Vec3, fwd: Vec3, tan_half: f32, aspect: f32, near: f32, far: f32) -> Mat4 {
    crate::camera::perspective_view_proj(eye, right, up, fwd, tan_half, aspect, near, far, [0.0, 0.0])
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::{RenderMode, ViewKind, ViewParams};

    fn cam() -> Camera {
        Camera::new(&ViewParams {
            game_pixel_grid: false,
            size: [200, 100],
            kind: ViewKind::Perspective {
                eye: [1600.0, -320.0, 64.0],
                forward: [1.0, 0.0, 0.0],
                up: [0.0, 0.0, 1.0],
                fov_y_deg: 70.0,
                near: 4.0,
                far: 32768.0,
            },
            mode: RenderMode::Textured,
            grid: None,
            background: [0.0; 3],
            selection_color: [1.0; 4],
            show_sprites: true,
            show_models: true,
            msaa: false,
        })
    }

    #[test]
    fn sky_camera_scales_around_the_sky_origin() {
        let c = cam();
        let sky = Sky3d { origin: [0.0, 0.0, -2000.0], scale: 16.0 };
        let Some(s) = sky_camera(&c, &sky) else { panic!("perspective camera") };
        assert!((s.eye - Vec3::new(100.0, -20.0, -1996.0)).length() < 1e-3, "{:?}", s.eye);
        assert_eq!(s.forward, c.forward);
        assert!((s.near - 0.25).abs() < 1e-6 && (s.far - 2048.0).abs() < 1e-3);
        let world = Vec3::new(3200.0, 160.0, 400.0);
        let in_sky = Vec3::from(sky.origin) + world / 16.0;
        let (Some(a), Some(b)) = (c.world_to_screen(world), s.world_to_screen(in_sky)) else { panic!("in front") };
        assert!((a[0] - b[0]).abs() < 1e-2 && (a[1] - b[1]).abs() < 1e-2, "{a:?} {b:?}");
        assert!(sky_camera(&c, &Sky3d { scale: 0.0, ..sky }).is_none());
        assert!(sky_camera(&c, &Sky3d { origin: [f32::NAN; 3], ..sky }).is_none());
    }
}
