use std::cell::Cell;

use hp_editor::Id;

use crate::settings::{DispWalkMode, PropShadowSetting, RenderModeSetting, Settings, StaticPropShadows};

#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash)]
pub struct RenderDisplay {
    pub smoothing_groups: bool,
    pub render_effects: bool,
    pub highlight_invalid: bool,
    pub visgroup_colors: bool,
    pub object_colors_3d: bool,
    pub displacements: bool,
    pub displacements_in_2d: bool,
    pub displacements_only: bool,
    pub disp_walkable: bool,
    pub disp_walk_mode: DispWalkMode,
    pub disp_walk_limit: u16,
    pub sky: bool,
    pub sky_3d: bool,
    pub fog: Option<Id>,
    pub fog_volumes: bool,
    pub fog_volume_texture: bool,
    pub fish: bool,
    pub fish_swim: bool,
    pub sky_camera: Option<Id>,
    pub lods: bool,
    pub collision: bool,
    pub collision_only: bool,
    pub hidden_tools: u8,
    pub normal_maps: bool,
    pub cubemaps: bool,
    pub cubemaps_auto: bool,
    pub light_hdr: bool,
    pub projected_textures: bool,
    pub sky_occlusion: bool,
    pub sun_glow: bool,
    pub prop_shadows: StaticPropShadows,
    pub texture_streaming: bool,
}

pub const HIDE_NODRAW: u8 = 1;
pub const HIDE_CLIPS: u8 = 2;
pub const HIDE_TRIGGERS: u8 = 4;
pub const HIDE_HINTS: u8 = 8;

impl Default for RenderDisplay {
    fn default() -> Self {
        Self {
            smoothing_groups: false,
            render_effects: true,
            highlight_invalid: true,
            visgroup_colors: false,
            object_colors_3d: false,
            displacements: true,
            displacements_in_2d: true,
            displacements_only: false,
            disp_walkable: false,
            disp_walk_mode: DispWalkMode::Source,
            disp_walk_limit: 4557,
            sky: true,
            sky_3d: true,
            fog: None,
            fog_volumes: true,
            fog_volume_texture: true,
            fish: true,
            fish_swim: true,
            sky_camera: None,
            lods: false,
            collision: false,
            collision_only: false,
            hidden_tools: 0,
            normal_maps: true,
            cubemaps: true,
            cubemaps_auto: true,
            light_hdr: true,
            projected_textures: true,
            sky_occlusion: true,
            sun_glow: true,
            prop_shadows: StaticPropShadows::Collision,
            texture_streaming: true,
        }
    }
}

impl RenderDisplay {
    pub fn from_settings(s: &Settings, rt: &RenderRuntime) -> Self {
        let r = &s.render;
        let plan = rt.perf.plan(r.performance_mode);
        Self {
            smoothing_groups: s.view.render_mode == RenderModeSetting::SmoothingGroups,
            render_effects: r.render_effects,
            highlight_invalid: r.highlight_invalid,
            visgroup_colors: r.use_visgroup_colors,
            object_colors_3d: r.object_colors_3d,
            displacements: r.show_displacements,
            displacements_in_2d: r.displacements_in_2d,
            displacements_only: r.displacements_only,
            disp_walkable: r.disp_walkable,
            disp_walk_mode: r.disp_walk_mode,
            disp_walk_limit: walk_limit(r.disp_walk_angle),
            sky: r.sky_preview,
            sky_3d: r.skybox_3d,
            fog: r.fog_preview.then_some(rt.fog_controller.unwrap_or(0)),
            fog_volumes: r.fog_volume_preview,
            fog_volume_texture: r.fog_volume_texture,
            fish: r.fish_preview,
            fish_swim: r.fish_swim,
            sky_camera: rt.sky_camera,
            lods: r.lod_distances.iter().any(|d| d.is_finite() && *d > 0.0),
            collision: r.collision_models,
            collision_only: r.collision_only,
            hidden_tools: [
                (r.hide_nodraw, HIDE_NODRAW),
                (r.hide_clips, HIDE_CLIPS),
                (r.hide_triggers, HIDE_TRIGGERS),
                (r.hide_hints, HIDE_HINTS),
            ]
            .iter()
            .filter(|(on, _)| *on)
            .fold(0, |m, (_, bit)| m | bit),
            normal_maps: r.normal_maps && plan.normal_maps,
            cubemaps: r.cubemaps && plan.cubemaps,
            cubemaps_auto: r.cubemaps_auto,
            light_hdr: r.lighting_hdr(s.view.render_mode, s.compile.lighting),
            projected_textures: r.projected_textures,
            sky_occlusion: r.sky_occlusion && plan.sky_occlusion,
            sun_glow: r.sun_glow && plan.sun_glow,
            prop_shadows: prop_shadows(s),
            texture_streaming: r.texture_streaming,
        }
    }
}

pub fn walk_limit(angle: f32) -> u16 {
    let a = if angle.is_finite() { angle.clamp(1.0, 89.0) } else { 45.573 };
    (a * 100.0).round() as u16
}

pub fn prop_shadows(s: &Settings) -> StaticPropShadows {
    match s.render.prop_shadows {
        PropShadowSetting::Auto => s.compile.static_prop_shadows(),
        PropShadowSetting::Mesh => StaticPropShadows::Mesh,
        PropShadowSetting::Collision => StaticPropShadows::Collision,
    }
}

thread_local! {
    static DISPLAY: Cell<RenderDisplay> = Cell::new(RenderDisplay::default());
}

pub fn publish_render_display(display: RenderDisplay) {
    DISPLAY.with(|d| d.set(display));
}

pub fn render_display() -> RenderDisplay {
    DISPLAY.with(Cell::get)
}

pub fn previewed_sky_camera(state: &crate::AppState) -> Option<Id> {
    let id = state.runtime.render.sky_camera?;
    let node = state.doc.scene().entity(id)?;
    hp_editor::sky::is_sky_camera(&node.entity).then_some(id)
}

#[derive(Clone, Copy, Debug, PartialEq)]
pub struct DetailPreviewRequest {
    pub enabled: bool,
    pub eye: [f32; 3],
}

#[derive(Clone, Debug, Default, PartialEq)]
pub struct DetailTypeReport {
    pub material: String,
    pub detail_type: String,
    pub surfaces: usize,
    pub known: bool,
}

#[derive(Clone, Debug, Default, PartialEq)]
pub struct DetailPreviewReport {
    pub enabled: bool,
    pub drawn: bool,
    pub dict_file: Option<String>,
    pub dict_types: usize,
    pub dict_error: Option<String>,
    pub sprite_material: String,
    pub surfaces: usize,
    pub displacements: usize,
    pub brush_faces: usize,
    pub surfaces_in_range: usize,
    pub sprites: usize,
    pub models: usize,
    pub capped: bool,
    pub view_distance: f32,
    pub fade_distance: f32,
    pub types: Vec<DetailTypeReport>,
}

#[derive(Debug, Default)]
pub struct RenderRuntime {
    pub detail_preview: DetailPreviewReport,
    pub fog_controller: Option<Id>,
    pub sky_camera: Option<Id>,
    pub views_rendered: u64,
    pub views_reused: u64,
    pub perf: super::perf::PerfRuntime,
    pub animation: Option<std::time::Duration>,
    pub focused: bool,
    pub last_input: f64,
}

static ANIMATIONS_RUNNING: std::sync::atomic::AtomicBool = std::sync::atomic::AtomicBool::new(true);

pub fn animations_running() -> bool {
    ANIMATIONS_RUNNING.load(std::sync::atomic::Ordering::Relaxed)
}

impl RenderRuntime {
    pub fn update_animation(
        &mut self,
        settings: &crate::settings::RenderSettings,
        focused: bool,
        input: bool,
        now: f64,
    ) {
        if input {
            self.last_input = now;
        }
        self.focused = focused;
        self.animation = settings.animation_interval(focused, (now - self.last_input).max(0.0));
        ANIMATIONS_RUNNING.store(self.animation.is_some(), std::sync::atomic::Ordering::Relaxed);
    }

    pub fn request_animation_frame(&self, ctx: &egui::Context) -> bool {
        match self.animation {
            Some(wait) if wait.is_zero() => {
                ctx.request_repaint();
                true
            }
            Some(wait) => {
                ctx.request_repaint_after(wait);
                true
            }
            None => false,
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn animation_clock_follows_focus_and_input() {
        let settings = crate::settings::RenderSettings::default();
        let mut rt = RenderRuntime::default();
        rt.update_animation(&settings, true, true, 10.0);
        assert!(rt.animation.is_some() && rt.focused);
        rt.update_animation(&settings, true, false, 129.0);
        assert!(rt.animation.is_some(), "119 s without input");
        rt.update_animation(&settings, true, false, 131.0);
        assert!(rt.animation.is_none(), "idle for more than two minutes");
        rt.update_animation(&settings, true, true, 132.0);
        assert!(rt.animation.is_some(), "any input wakes it");
        rt.update_animation(&settings, false, true, 133.0);
        assert!(rt.animation.is_none() && !rt.focused, "background");
        let ctx = egui::Context::default();
        assert!(!rt.request_animation_frame(&ctx));
        rt.update_animation(&settings, true, true, 134.0);
        assert!(rt.request_animation_frame(&ctx));
    }

    #[test]
    fn display_follows_settings_and_is_per_thread() {
        let mut s = Settings::default();
        s.view.render_mode = RenderModeSetting::SmoothingGroups;
        s.render.fog_preview = true;
        let rt = RenderRuntime { fog_controller: Some(42), ..Default::default() };
        let d = RenderDisplay::from_settings(&s, &rt);
        assert!(d.smoothing_groups);
        assert_eq!(d.fog, Some(42));
        assert!(d.normal_maps);
        s.render.normal_maps = false;
        assert!(!RenderDisplay::from_settings(&s, &rt).normal_maps);
        assert!(d.cubemaps && d.cubemaps_auto);
        s.render.cubemaps = false;
        assert!(!RenderDisplay::from_settings(&s, &rt).cubemaps);
        assert!(d.fog_volumes && d.fog_volume_texture && d.fish && d.fish_swim);
        s.render.fog_volume_texture = false;
        s.render.fish_swim = false;
        let off = RenderDisplay::from_settings(&s, &rt);
        assert!(!off.fog_volume_texture && !off.fish_swim && off.fog_volumes && off.fish);
        assert_eq!(d.prop_shadows, StaticPropShadows::Collision);
        s.compile.extra_rad_args = "-StaticPropPolys".into();
        assert_eq!(RenderDisplay::from_settings(&s, &rt).prop_shadows, StaticPropShadows::Mesh);
        s.render.prop_shadows = PropShadowSetting::Collision;
        assert_eq!(RenderDisplay::from_settings(&s, &rt).prop_shadows, StaticPropShadows::Collision);
        s.compile.static_prop_shadows = false;
        assert_eq!(RenderDisplay::from_settings(&s, &rt).prop_shadows, StaticPropShadows::Collision);
        s.render.prop_shadows = PropShadowSetting::Auto;
        assert_eq!(RenderDisplay::from_settings(&s, &rt).prop_shadows, StaticPropShadows::None);
        s.render.prop_shadows = PropShadowSetting::Mesh;
        assert_eq!(RenderDisplay::from_settings(&s, &rt).prop_shadows, StaticPropShadows::Mesh);
        publish_render_display(d);
        assert_eq!(render_display(), d);
        let other = std::thread::spawn(render_display).join().expect("thread");
        assert_eq!(other, RenderDisplay::default());
        publish_render_display(RenderDisplay::default());
    }
}
