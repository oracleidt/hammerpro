use glam::Vec3;

use crate::camera::{Camera, frustum_planes};
use crate::gpu::ViewUniform;
use crate::pipelines::{DEPTH_FORMAT, ShadowPipelines};
use crate::shadow::{MOTION_GROW, ShadowFit, fit, fit_grown};
use crate::{Renderer, ShadowControl};

pub(crate) struct ControlShadow {
    pub view: wgpu::TextureView,
    target: wgpu::TextureView,
    uniform: wgpu::Buffer,
    bind: Option<(u64, wgpu::BindGroup)>,
    pipelines: ShadowPipelines,
    key: Option<([f32; 16], u64)>,
    kept: Option<ShadowFit>,
    cams: ahash::AHashMap<u32, (glam::Mat4, u64)>,
}

impl Renderer {
    pub(crate) fn prepare_control_shadow(
        &mut self,
        view_id: u32,
        camera: &Camera,
        config: ShadowControl,
    ) -> Option<(ShadowFit, bool)> {
        if self.instance_count == 0 || !config.distance.is_finite() || config.distance <= 0.0 {
            return None;
        }
        let direction = Vec3::from_array(config.direction).try_normalize()?;
        let (min, max) = self.scene_bounds();
        let fresh = fit(camera, direction, min, max, 2048)?;
        if self.control_shadow.is_none() {
            let texture = self.device.create_texture(&wgpu::TextureDescriptor {
                label: Some("hp-render shadow control"),
                size: wgpu::Extent3d { width: 2048, height: 2048, depth_or_array_layers: 1 },
                mip_level_count: 1,
                sample_count: 1,
                dimension: wgpu::TextureDimension::D2,
                format: DEPTH_FORMAT,
                usage: wgpu::TextureUsages::TEXTURE_BINDING | wgpu::TextureUsages::RENDER_ATTACHMENT,
                view_formats: &[],
            });
            self.control_shadow = Some(ControlShadow {
                view: texture.create_view(&wgpu::TextureViewDescriptor {
                    dimension: Some(wgpu::TextureViewDimension::D2Array),
                    ..Default::default()
                }),
                target: texture.create_view(&wgpu::TextureViewDescriptor::default()),
                uniform: self.device.create_buffer(&wgpu::BufferDescriptor {
                    label: Some("hp-render shadow control uniform"),
                    size: size_of::<ViewUniform>() as u64,
                    usage: wgpu::BufferUsages::UNIFORM | wgpu::BufferUsages::COPY_DST,
                    mapped_at_creation: false,
                }),
                bind: None,
                pipelines: ShadowPipelines::shared(&self.device, &self.shared),
                key: None,
                kept: None,
                cams: ahash::AHashMap::default(),
            });
            self.scene_gen += 1;
        }
        let serial = self.render_serial;
        let control = self.control_shadow.as_mut()?;
        let last = control.cams.get(&view_id).copied();
        let moving = last.is_some_and(|(vp, _)| vp != camera.view_proj);
        let resting = last.is_some_and(|(vp, first)| vp == camera.view_proj && first != serial);
        if last.is_none_or(|(vp, _)| vp != camera.view_proj) {
            control.cams.insert(view_id, (camera.view_proj, serial));
        }
        let stable = !resting && !crate::profile::baseline("shadowfit");
        let fitted = match control.kept {
            Some(kept) if moving && stable && kept.covers(&fresh) => kept,
            Some(_) if moving && stable => fit_grown(camera, direction, min, max, 2048, MOTION_GROW)
                .filter(|wide| wide.covers(&fresh))
                .unwrap_or(fresh),
            _ => fresh,
        };
        control.kept = Some(fitted);
        if fitted != fresh {
            self.redraw_view(view_id);
        }
        let control = self.control_shadow.as_ref()?;
        let key = (fitted.view_proj.to_cols_array(), self.shadow_epoch);
        let dirty = control.key != Some(key);
        if dirty {
            let uniform = ViewUniform {
                view_proj: fitted.view_proj.to_cols_array_2d(),
                opts: [crate::OPT_SHADOW_PASS, 0, 0, 0],
                ..Default::default()
            };
            self.queue.write_buffer(&control.uniform, 0, bytemuck::bytes_of(&uniform));
        }
        let bind = control
            .bind
            .as_ref()
            .is_none_or(|(generation, _)| *generation != self.scene_gen)
            .then(|| self.shadow_pass_bind_group(&control.uniform, "hp-render shadow control"));
        let control = self.control_shadow.as_mut()?;
        if let Some(bind) = bind {
            control.bind = Some((self.scene_gen, bind));
        }
        control.key = Some(key);
        Some((fitted, dirty))
    }

    pub(crate) fn record_control_shadow(&self, encoder: &mut wgpu::CommandEncoder, fitted: &ShadowFit) -> (u32, u64) {
        let Some(control) = self.control_shadow.as_ref() else { return (0, 0) };
        let Some((_, bind)) = control.bind.as_ref() else { return (0, 0) };
        let planes = frustum_planes(&fitted.view_proj);
        let mut pass = encoder.begin_render_pass(&wgpu::RenderPassDescriptor {
            label: Some("hp-render shadow control"),
            color_attachments: &[],
            depth_stencil_attachment: Some(wgpu::RenderPassDepthStencilAttachment {
                view: &control.target,
                depth_ops: Some(wgpu::Operations { load: wgpu::LoadOp::Clear(1.0), store: wgpu::StoreOp::Store }),
                stencil_ops: None,
            }),
            timestamp_writes: None,
            occlusion_query_set: None,
            multiview_mask: None,
        });
        pass.set_bind_group(0, bind, &[]);
        self.draw_shadow_casters(&mut pass, &control.pipelines, &planes, None, None, true)
    }
}
