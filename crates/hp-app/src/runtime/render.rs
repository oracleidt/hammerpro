use std::cell::Cell;

use hp_editor::Id;

use crate::settings::{PropShadowSetting, RenderModeSetting, Settings, StaticPropShadows};

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
        }
    }
}

impl RenderDisplay {
    pub fn from_settings(s: &Settings, rt: &RenderRuntime) -> Self {
        let r = &s.render;
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
            normal_maps: r.normal_maps,
            cubemaps: r.cubemaps,
            cubemaps_auto: r.cubemaps_auto,
            light_hdr: r.light_preview_hdr,
            projected_textures: r.projected_textures,
            sky_occlusion: r.sky_occlusion,
            sun_glow: r.sun_glow,
            prop_shadows: prop_shadows(s),
        }
    }
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

#[derive(Debug, Default)]
pub struct RenderRuntime {
    pub fog_controller: Option<Id>,
    pub sky_camera: Option<Id>,
    pub views_rendered: u64,
    pub views_reused: u64,
}

#[cfg(test)]
mod tests {
    use super::*;

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
