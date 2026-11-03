use glam::{Mat4, Vec4};

use crate::camera::Camera;
use crate::types::{MaterialFlags, Overlay, ViewKind, ViewOptions, ViewParams};
use crate::{ObjectFlags, Renderer};

pub(crate) const REFLECT_VIEW: u32 = 0xffff_fb00;
const MAX_SIZE: u32 = 1024;
const MIN_HEIGHT: f32 = 0.5;

#[derive(Clone, Copy, Debug)]
pub(crate) struct Reflecting {
    pub plane: Vec4,
    pub world_only: bool,
    pub sky_only: bool,
    pub tone: f32,
    pub main_cam: Camera,
}

#[derive(Clone, Copy, Debug, PartialEq)]
pub(crate) struct WaterPlane {
    pub height: f32,
    pub entities: bool,
    pub skybox_only: bool,
    pub sky_3d: bool,
}

pub(crate) fn mirror(v: [f32; 3], height: Option<f32>) -> [f32; 3] {
    match height {
        Some(h) => [v[0], v[1], 2.0 * h - v[2]],
        None => [v[0], v[1], -v[2]],
    }
}

pub(crate) fn reflected_params(params: &ViewParams, plane: &WaterPlane) -> Option<ViewParams> {
    let ViewKind::Perspective { eye, forward, up, fov_y_deg, near, far } = params.kind else { return None };
    if eye[2] - plane.height < MIN_HEIGHT {
        return None;
    }
    let [w, h] = params.size;
    let k = (MAX_SIZE as f32 / w.max(h).max(1) as f32).min(1.0);
    let size = [((w as f32 * k) as u32).max(16), ((h as f32 * k) as u32).max(16)];
    Some(ViewParams {
        size,
        kind: ViewKind::Perspective {
            eye: mirror(eye, Some(plane.height)),
            forward: mirror(forward, None),
            up: mirror(up, None),
            fov_y_deg,
            near,
            far,
        },
        mode: params.mode,
        grid: None,
        background: params.background,
        selection_color: [0.0; 4],
        show_sprites: params.show_sprites && plane.entities && !plane.skybox_only,
        show_models: params.show_models && plane.entities && !plane.skybox_only,
        msaa: false,
        game_pixel_grid: params.game_pixel_grid,
    })
}

pub(crate) fn reflected_options(o: &ViewOptions, plane: &WaterPlane) -> ViewOptions {
    ViewOptions {
        grid_3d: None,
        occlusion_culling: false,
        selection_outline: false,
        game_wireframe: 0,
        collision_models: false,
        collision_only: false,
        walkable: false,
        walk_marks: false,
        show_sky_3d: o.show_sky_3d && plane.sky_3d,
        ..*o
    }
}

pub(crate) fn water_plane(height: f32) -> Vec4 {
    Vec4::new(0.0, 0.0, 1.0, -height)
}

impl Renderer {
    pub(crate) fn find_water_plane(&self, cam: &Camera, opts: &ViewOptions) -> Option<WaterPlane> {
        if opts.water_detail == crate::types::WaterDetail::Simple || cam.is_ortho {
            return None;
        }
        let radius = if opts.cull_radius.is_finite() { opts.cull_radius.max(0.0) } else { 0.0 };
        let mut best: Option<(f32, WaterPlane)> = None;
        for (mi, m) in self.scene.materials.iter().enumerate() {
            let Some((desc, w)) = m.desc.as_ref().and_then(|d| d.water.map(|w| (d, w))) else { continue };
            if !w.reflect || w.cheap || !w.above_water || desc.flags.contains(MaterialFlags::NODRAW_IN_3D) {
                continue;
            }
            for &c in &m.chunks {
                let Some(Some(ch)) = self.scene.chunks.get(c as usize) else { continue };
                if ch.used_i == 0 || ch.all_hidden || !cam.aabb_visible(ch.aabb_min, ch.aabb_max) {
                    continue;
                }
                if radius > 0.0 && cam.eye.clamp(ch.aabb_min, ch.aabb_max).distance_squared(cam.eye) > radius * radius {
                    continue;
                }
                for id in ch.members.keys() {
                    let Some(obj) = self.scene.objects.get(id) else { continue };
                    let Some(mesh) = obj.stored.as_ref() else { continue };
                    if self.scene.flags(*id).is_some_and(|f| f.contains(ObjectFlags::HIDDEN)) {
                        continue;
                    }
                    let Some((lo, hi)) = crate::water::surface_bounds(mesh, mi as u32) else { continue };
                    if !cam.aabb_visible(lo, hi) || cam.eye.z - hi.z < MIN_HEIGHT {
                        continue;
                    }
                    let d = cam.eye.clamp(lo, hi).distance_squared(cam.eye);
                    if best.is_none_or(|(b, _)| d < b) {
                        best = Some((
                            d,
                            WaterPlane {
                                height: hi.z,
                                entities: w.reflect_entities
                                    || opts.water_detail == crate::types::WaterDetail::ReflectAll,
                                skybox_only: w.reflect_skybox_only,
                                sky_3d: w.reflect_3d_sky,
                            },
                        ));
                    }
                }
            }
        }
        best.map(|(_, p)| p)
    }

    pub(crate) fn render_reflection(
        &mut self,
        params: &ViewParams,
        opts: &ViewOptions,
        cam: &Camera,
        tone: f32,
    ) -> Option<(Mat4, wgpu::TextureView)> {
        let plane = self.find_water_plane(cam, opts)?;
        let rp = reflected_params(params, &plane)?;
        let options = reflected_options(opts, &plane);
        self.view_options.insert(REFLECT_VIEW, options);
        let clip = water_plane(plane.height);
        self.reflecting = Some(Reflecting {
            plane: clip,
            world_only: !plane.entities,
            sky_only: plane.skybox_only,
            tone,
            main_cam: *cam,
        });
        let content = self.content_gen;
        self.render_view(REFLECT_VIEW, &rp, &Overlay::default());
        self.content_gen = content;
        self.reflecting = None;
        let view_proj = Camera::new(&rp).clipped_by(clip).view_proj;
        let view = self.views.get(&REFLECT_VIEW)?;
        Some((view_proj, view.color_view.clone()))
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::types::RenderMode;
    use glam::Vec3;

    fn params(eye: [f32; 3], forward: [f32; 3]) -> ViewParams {
        ViewParams {
            game_pixel_grid: false,
            size: [1600, 900],
            kind: ViewKind::Perspective { eye, forward, up: [0.0, 0.0, 1.0], fov_y_deg: 70.0, near: 1.0, far: 8192.0 },
            mode: RenderMode::Textured,
            grid: None,
            background: [0.0; 3],
            selection_color: [1.0; 4],
            show_sprites: true,
            show_models: true,
            msaa: true,
        }
    }

    fn plane(height: f32, entities: bool) -> WaterPlane {
        WaterPlane { height, entities, skybox_only: false, sky_3d: true }
    }

    #[test]
    fn the_reflection_camera_mirrors_the_eye_about_the_water() {
        let p = params([10.0, 20.0, 100.0], [0.6, 0.0, -0.8]);
        let r = reflected_params(&p, &plane(-28.0, false)).expect("above water");
        let ViewKind::Perspective { eye, forward, up, .. } = r.kind else { panic!("perspective") };
        assert_eq!(eye, [10.0, 20.0, -156.0]);
        assert_eq!(forward, [0.6, 0.0, 0.8]);
        assert_eq!(up, [0.0, 0.0, -1.0]);
        assert_eq!(r.size, [1024, 576]);
        assert!(!r.msaa && !r.show_models && !r.show_sprites, "world only by default like $reflectentities 0");
        let with = reflected_params(&p, &plane(-28.0, true)).expect("entities");
        assert!(with.show_models && with.show_sprites);
        assert!(reflected_params(&p, &plane(100.0, false)).is_none(), "no reflection from under the surface");
    }

    #[test]
    fn the_clip_plane_keeps_only_what_is_above_the_water() {
        let p = params([0.0, 0.0, 64.0], [1.0, 0.0, -0.3]);
        let rp = reflected_params(&p, &plane(0.0, true)).expect("reflection");
        let cam = Camera::new(&rp).clipped_by(water_plane(0.0));
        let depth = |x: Vec3| {
            let c = cam.view_proj * x.extend(1.0);
            (c.z / c.w, c.w)
        };
        let (above, w) = depth(Vec3::new(400.0, 0.0, 40.0));
        assert!(w > 0.0 && (0.0..=1.0).contains(&above), "{above}");
        let (below, _) = depth(Vec3::new(400.0, 0.0, -40.0));
        assert!(below > 1.0, "geometry under the water is clipped: {below}");
        let eye = Vec3::new(0.0, 0.0, -64.0);
        let ray = Vec3::new(1.0, 0.0, 0.3).normalize();
        let (near, _) = depth(eye + ray * 400.0);
        let (far, _) = depth(eye + ray * 4000.0);
        assert!(0.0 <= far && far < near && near <= 1.0, "farther along a ray is deeper in reversed Z: {far} {near}");
        assert!(!cam.aabb_visible(Vec3::new(390.0, -10.0, -60.0), Vec3::new(410.0, 10.0, -20.0)));
        assert!(cam.aabb_visible(Vec3::new(390.0, -10.0, 20.0), Vec3::new(410.0, 10.0, 60.0)));
    }

    #[test]
    fn reflected_views_leave_editor_aids_out() {
        let o =
            ViewOptions { occlusion_culling: true, selection_outline: true, game_wireframe: 2, ..Default::default() };
        let r = reflected_options(&o, &WaterPlane { height: 0.0, entities: false, skybox_only: false, sky_3d: false });
        assert!(!r.occlusion_culling && !r.selection_outline && r.game_wireframe == 0);
        assert!(!r.show_sky_3d);
    }
}
