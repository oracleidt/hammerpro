use serde::{Deserialize, Serialize};

#[derive(Clone, Copy, Debug, Default, PartialEq, Eq, Serialize, Deserialize)]
pub enum ThemeChoice {
    #[default]
    Dark,
    Light,
}

#[derive(Clone, Copy, Debug, Default, PartialEq, Eq, Serialize, Deserialize)]
pub enum RenderModeSetting {
    #[default]
    Textured,
    Flat,
    Wireframe,
    TexturedWire,
    Lit,
    Unshaded,
    SmoothingGroups,
    Baked,
    LightmapGrid,
    ColorCorrected,
}

#[derive(Clone, Copy, Debug, Default, PartialEq, Eq, Serialize, Deserialize)]
pub enum SelectModeSetting {
    #[default]
    Groups,
    Objects,
    Solids,
}

#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
#[serde(default)]
pub struct ViewSettings {
    pub render_mode: RenderModeSetting,
    pub fov_deg: f32,
    pub camera_speed: f32,
    pub camera_fast_multiplier: f32,
    pub mouse_sensitivity: f32,
    pub invert_mouse_y: bool,
    pub far_plane: f32,
    pub msaa: bool,
    pub show_models: bool,
    pub show_helpers: bool,
    pub show_tool_textures: bool,
    pub show_fps: bool,
    pub show_3d_grid: bool,
    pub grid_highlight_every: u32,
    pub show_1024_grid: bool,
    pub background_2d: [u8; 3],
    pub background_3d: [u8; 3],
    pub selection_color: [u8; 3],
    pub zoom_to_cursor: bool,
    pub hover_outline: bool,
    pub keyboard_layout: KeyboardLayout,
}

#[derive(Clone, Copy, Debug, Default, PartialEq, Eq, Serialize, Deserialize)]
pub enum KeyboardLayout {
    #[default]
    Auto,
    Azerty,
    Qwerty,
}

impl Default for ViewSettings {
    fn default() -> Self {
        Self {
            render_mode: RenderModeSetting::Textured,
            fov_deg: 75.0,
            camera_speed: 1000.0,
            camera_fast_multiplier: 3.0,
            mouse_sensitivity: 0.15,
            invert_mouse_y: false,
            far_plane: 32768.0,
            msaa: true,
            show_models: true,
            show_helpers: true,
            show_tool_textures: true,
            show_fps: false,
            show_3d_grid: false,
            grid_highlight_every: 8,
            show_1024_grid: true,
            background_2d: [20, 20, 22],
            background_3d: [36, 38, 44],
            selection_color: [255, 70, 70],
            zoom_to_cursor: true,
            hover_outline: false,
            keyboard_layout: KeyboardLayout::Auto,
        }
    }
}

#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
#[serde(default)]
pub struct EditSettings {
    pub grid_size: f64,
    pub snap_to_grid: bool,
    pub show_grid: bool,
    pub texture_lock: bool,
    pub texture_scale_lock: bool,
    pub select_mode: SelectModeSetting,
    pub default_material: String,
    pub default_texture_scale: f64,
    pub default_lightmap_scale: i32,
    pub default_point_entity: String,
    pub default_brush_entity: String,
    pub undo_levels: u32,
    pub autosave_minutes: u32,
    pub autosave_keep: u32,
    pub arrow_key_nudge: bool,
    pub paste_offset: bool,
    pub undo_restores_selection: bool,
    pub new_brush_color: Option<[u8; 3]>,
    pub keep_displacements_on_cut: bool,
}

impl Default for EditSettings {
    fn default() -> Self {
        Self {
            grid_size: 16.0,
            snap_to_grid: true,
            show_grid: true,
            texture_lock: true,
            texture_scale_lock: false,
            select_mode: SelectModeSetting::Groups,
            default_material: "dev/dev_measuregeneric01b".to_owned(),
            default_texture_scale: 0.25,
            default_lightmap_scale: 16,
            default_point_entity: "info_player_start".to_owned(),
            default_brush_entity: "func_detail".to_owned(),
            undo_levels: 500,
            autosave_minutes: 5,
            autosave_keep: 10,
            arrow_key_nudge: true,
            paste_offset: true,
            undo_restores_selection: true,
            new_brush_color: None,
            keep_displacements_on_cut: true,
        }
    }
}

#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
#[serde(default)]
pub struct UiSettings {
    pub theme: ThemeChoice,
    pub ui_scale: f32,
    pub accent_color: Option<[u8; 3]>,
    pub show_texture_browser: bool,
    pub show_entity_properties: bool,
    pub show_visgroups: bool,
    pub show_outliner: bool,
    pub show_model_browser: bool,
    pub show_console: bool,
    pub thumbnail_size: f32,
    pub split_x: f32,
    pub split_y: f32,
    pub quadrants: [u8; 4],
    pub background_image: String,
    pub background_opacity: f32,
    pub background_offset: [f32; 2],
    pub background_zoom: f32,
    pub background_hide_3d: bool,
    pub native_frames: bool,
    pub start_fullscreen: bool,
    pub uv_window: Option<super::WindowPlacement>,
    pub portals_window: Option<super::WindowPlacement>,
    pub floating_panels: std::collections::BTreeSet<String>,
}

impl Default for UiSettings {
    fn default() -> Self {
        Self {
            theme: ThemeChoice::Dark,
            ui_scale: 1.0,
            accent_color: None,
            show_texture_browser: true,
            show_entity_properties: true,
            show_visgroups: true,
            show_outliner: false,
            show_model_browser: false,
            show_console: true,
            thumbnail_size: 96.0,
            split_x: 0.5,
            split_y: 0.5,
            quadrants: [0, 1, 2, 3],
            background_image: String::new(),
            background_opacity: 0.15,
            background_offset: [0.0, 0.0],
            background_zoom: 1.0,
            background_hide_3d: true,
            native_frames: false,
            start_fullscreen: true,
            uv_window: None,
            portals_window: None,
            floating_panels: std::collections::BTreeSet::new(),
        }
    }
}
