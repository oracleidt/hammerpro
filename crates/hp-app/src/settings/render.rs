use serde::{Deserialize, Serialize};

#[derive(Clone, Copy, Debug, Default, PartialEq, Eq, Hash, Serialize, Deserialize)]
pub enum Models2dSetting {
    Boxes,
    Wireframe,
    #[default]
    SelectedOnly,
}

#[derive(Clone, Copy, Debug, Default, PartialEq, Eq, Hash, Serialize, Deserialize)]
pub enum Disp2dSetting {
    #[default]
    Mesh,
    SelectedMesh,
    Outline,
}

impl Disp2dSetting {
    pub const ALL: [Disp2dSetting; 3] = [Disp2dSetting::Mesh, Disp2dSetting::SelectedMesh, Disp2dSetting::Outline];
}

#[derive(Clone, Copy, Debug, Default, PartialEq, Eq, Hash, Serialize, Deserialize)]
pub enum Disp2dViewSetting {
    #[default]
    Global,
    Own(Disp2dSetting),
}

#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
#[serde(default)]
pub struct RenderSettings {
    pub grid_3d: bool,
    pub grid_3d_opacity: f32,
    pub grid_3d_fade_cells: f32,
    pub near_plane: f32,
    pub model_distance: f32,
    pub lod_distances: [f32; 2],
    pub models_in_2d: Models2dSetting,
    pub collision_models: bool,
    pub collision_only: bool,
    pub fill_selected_2d: bool,
    pub fill_selected_2d_opacity: f32,
    pub camera_in_2d: bool,
    pub vertex_dots_2d: bool,
    pub icons_in_2d: bool,
    pub icons_in_2d_px: f32,
    pub use_visgroup_colors: bool,
    pub highlight_invalid: bool,
    pub render_effects: bool,
    pub sky_preview: bool,
    pub skybox_3d: bool,
    pub sky_see_through: bool,
    pub water_preview: bool,
    pub fog_preview: bool,
    pub fog_volume_preview: bool,
    pub fog_volume_texture: bool,
    pub fish_preview: bool,
    pub fish_swim: bool,
    pub normal_maps: bool,
    pub player_compiled_lighting: bool,
    pub cubemaps: bool,
    pub cubemaps_auto: bool,
    pub light_shadows: u32,
    pub animate_materials: bool,
    pub model_fade: bool,
    pub force_opaque: bool,
    pub walkable: bool,
    pub radius_culling: bool,
    pub cull_radius: f32,
    pub show_displacements: bool,
    pub displacements_in_2d: bool,
    pub displacements_only: bool,
    pub displacements_2d: Disp2dSetting,
    pub displacements_2d_views: [Disp2dViewSetting; 4],
    pub disp_source_2d: bool,
    pub disp_walkable: bool,
    pub occlusion_culling: bool,
    pub selection_outline: bool,
    pub grid_dotted: bool,
    pub grid_highlight_64: bool,
    pub grid_min_px: f32,
    pub hide_nodraw: bool,
    pub hide_clips: bool,
    pub hide_triggers: bool,
    pub hide_hints: bool,
    pub max_fps: u32,
    pub unfocused_fps: u32,
    pub vsync: bool,
}

impl Default for RenderSettings {
    fn default() -> Self {
        Self {
            grid_3d: false,
            grid_3d_opacity: 0.45,
            grid_3d_fade_cells: 64.0,
            near_plane: 1.0,
            model_distance: 0.0,
            lod_distances: [0.0, 0.0],
            models_in_2d: Models2dSetting::SelectedOnly,
            collision_models: false,
            collision_only: false,
            fill_selected_2d: true,
            fill_selected_2d_opacity: 0.2,
            camera_in_2d: true,
            vertex_dots_2d: true,
            icons_in_2d: true,
            icons_in_2d_px: 16.0,
            use_visgroup_colors: false,
            highlight_invalid: true,
            render_effects: true,
            sky_preview: true,
            skybox_3d: true,
            sky_see_through: false,
            water_preview: true,
            fog_preview: false,
            fog_volume_preview: true,
            fog_volume_texture: true,
            fish_preview: true,
            fish_swim: true,
            normal_maps: true,
            player_compiled_lighting: true,
            cubemaps: true,
            cubemaps_auto: true,
            light_shadows: 4,
            animate_materials: true,
            model_fade: false,
            force_opaque: false,
            walkable: false,
            radius_culling: false,
            cull_radius: 4096.0,
            show_displacements: true,
            displacements_in_2d: true,
            displacements_only: false,
            displacements_2d: Disp2dSetting::Mesh,
            displacements_2d_views: [Disp2dViewSetting::Global; 4],
            disp_source_2d: true,
            disp_walkable: false,
            occlusion_culling: true,
            selection_outline: false,
            grid_dotted: false,
            grid_highlight_64: false,
            grid_min_px: 4.0,
            hide_nodraw: false,
            hide_clips: false,
            hide_triggers: false,
            hide_hints: false,
            max_fps: 0,
            unfocused_fps: 30,
            vsync: true,
        }
    }
}

impl RenderSettings {
    pub fn displacements_2d_of(&self, index: usize) -> Disp2dSetting {
        match self.displacements_2d_views.get(index) {
            Some(Disp2dViewSetting::Own(m)) => *m,
            _ => self.displacements_2d,
        }
    }

    pub fn near(&self) -> f32 {
        if self.near_plane.is_finite() { self.near_plane.clamp(0.5, 16.0) } else { 1.0 }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn old_files_load_with_defaults() {
        let s: RenderSettings = toml::from_str("grid_3d = true\n").expect("parse");
        assert!(s.grid_3d);
        assert_eq!(s.near_plane, 1.0);
        assert_eq!(s.models_in_2d, Models2dSetting::SelectedOnly);
        assert!(s.vsync && s.occlusion_culling);
        let bad = RenderSettings { near_plane: f32::NAN, ..Default::default() };
        assert_eq!(bad.near(), 1.0);
        assert_eq!(RenderSettings { near_plane: 100.0, ..Default::default() }.near(), 16.0);
        assert_eq!(s.displacements_2d, Disp2dSetting::Mesh);
        assert!(!s.disp_walkable);
        assert!(s.normal_maps, "normal maps are on by default");
        assert!(s.cubemaps && s.cubemaps_auto, "reflections are on by default");
        assert!(s.fog_volume_preview && s.fog_volume_texture, "fog volumes are previewed by default");
        assert!(s.fish_preview && s.fish_swim, "fish are previewed by default");
        let off: RenderSettings = toml::from_str(
            "normal_maps = false
",
        )
        .expect("parse");
        assert!(!off.normal_maps);
    }

    #[test]
    fn displacement_view_overrides_round_trip() {
        let mut s = RenderSettings { displacements_2d: Disp2dSetting::Outline, ..Default::default() };
        s.displacements_2d_views[2] = Disp2dViewSetting::Own(Disp2dSetting::SelectedMesh);
        assert_eq!(s.displacements_2d_of(0), Disp2dSetting::Outline);
        assert_eq!(s.displacements_2d_of(2), Disp2dSetting::SelectedMesh);
        assert_eq!(s.displacements_2d_of(9), Disp2dSetting::Outline);
        let text = toml::to_string(&s).expect("write");
        let back: RenderSettings = toml::from_str(&text).expect("read");
        assert_eq!(back, s);
    }
}
