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
pub enum DispWalkMode {
    #[default]
    Source,
    Limit,
    Gradient,
}

impl DispWalkMode {
    pub const ALL: [DispWalkMode; 3] = [DispWalkMode::Source, DispWalkMode::Limit, DispWalkMode::Gradient];

    pub const fn name(self) -> &'static str {
        match self {
            DispWalkMode::Source => "source",
            DispWalkMode::Limit => "limit",
            DispWalkMode::Gradient => "gradient",
        }
    }
}

pub const DEFAULT_WALK_ANGLE: f32 = 45.573;

#[derive(Clone, Copy, Debug, Default, PartialEq, Eq, Hash, Serialize, Deserialize)]
pub enum Disp2dViewSetting {
    #[default]
    Global,
    Own(Disp2dSetting),
}

#[derive(Clone, Copy, Debug, Default, PartialEq, Eq, Hash, Serialize, Deserialize)]
pub enum PropShadowSetting {
    #[default]
    Auto,
    Mesh,
    Collision,
}

impl PropShadowSetting {
    pub const ALL: [PropShadowSetting; 3] =
        [PropShadowSetting::Auto, PropShadowSetting::Mesh, PropShadowSetting::Collision];
}

#[derive(Clone, Copy, Debug, Default, PartialEq, Eq, Hash, Serialize, Deserialize)]
pub enum ToneModeSetting {
    #[default]
    Auto,
    Fixed,
    Off,
}

impl ToneModeSetting {
    pub const ALL: [ToneModeSetting; 3] = [ToneModeSetting::Auto, ToneModeSetting::Fixed, ToneModeSetting::Off];

    pub fn mode(self) -> hp_render::ToneMode {
        match self {
            ToneModeSetting::Auto => hp_render::ToneMode::Auto,
            ToneModeSetting::Fixed => hp_render::ToneMode::Fixed,
            ToneModeSetting::Off => hp_render::ToneMode::Off,
        }
    }
}

#[derive(Clone, Copy, Debug, Default, PartialEq, Eq, Hash, Serialize, Deserialize)]
pub enum WaterDetailSetting {
    Simple,
    ReflectWorld,
    #[default]
    ReflectAll,
}

impl WaterDetailSetting {
    pub const ALL: [WaterDetailSetting; 3] =
        [WaterDetailSetting::Simple, WaterDetailSetting::ReflectWorld, WaterDetailSetting::ReflectAll];

    pub fn detail(self) -> hp_render::WaterDetail {
        match self {
            WaterDetailSetting::Simple => hp_render::WaterDetail::Simple,
            WaterDetailSetting::ReflectWorld => hp_render::WaterDetail::ReflectWorld,
            WaterDetailSetting::ReflectAll => hp_render::WaterDetail::ReflectAll,
        }
    }
}

#[derive(Clone, Copy, Debug, Default, PartialEq, Eq, Hash, Serialize, Deserialize)]
pub enum PerformanceMode {
    #[default]
    Auto,
    Off,
    Low,
    VeryLow,
}

impl PerformanceMode {
    pub const ALL: [PerformanceMode; 4] =
        [PerformanceMode::Auto, PerformanceMode::Off, PerformanceMode::Low, PerformanceMode::VeryLow];

    pub const fn id(self) -> &'static str {
        match self {
            PerformanceMode::Auto => "Auto",
            PerformanceMode::Off => "Off",
            PerformanceMode::Low => "Low",
            PerformanceMode::VeryLow => "VeryLow",
        }
    }
}

#[derive(Clone, Copy, Debug, Default, PartialEq, Eq, Hash, Serialize, Deserialize)]
pub enum GraphicsMemory {
    #[default]
    Auto,
    Low,
    Normal,
    High,
}

impl GraphicsMemory {
    pub const ALL: [GraphicsMemory; 4] =
        [GraphicsMemory::Auto, GraphicsMemory::Low, GraphicsMemory::Normal, GraphicsMemory::High];

    pub const fn id(self) -> &'static str {
        match self {
            GraphicsMemory::Auto => "Auto",
            GraphicsMemory::Low => "Low",
            GraphicsMemory::Normal => "Normal",
            GraphicsMemory::High => "High",
        }
    }
}

#[derive(Clone, Copy, Debug, Default, PartialEq, Eq, Hash, Serialize, Deserialize)]
pub enum GraphicsBackend {
    #[default]
    Auto,
    Vulkan,
    Dx12,
}

impl GraphicsBackend {
    pub const ALL: [GraphicsBackend; 3] = [GraphicsBackend::Auto, GraphicsBackend::Vulkan, GraphicsBackend::Dx12];

    pub const fn id(self) -> &'static str {
        match self {
            GraphicsBackend::Auto => "Auto",
            GraphicsBackend::Vulkan => "Vulkan",
            GraphicsBackend::Dx12 => "Dx12",
        }
    }

    pub fn parse(text: &str) -> Option<GraphicsBackend> {
        match text.trim().to_ascii_lowercase().as_str() {
            "auto" | "" => Some(GraphicsBackend::Auto),
            "vulkan" | "vk" => Some(GraphicsBackend::Vulkan),
            "dx12" | "d3d12" | "directx12" | "directx" => Some(GraphicsBackend::Dx12),
            _ => None,
        }
    }
}

#[derive(Clone, Copy, Debug, Default, PartialEq, Eq, Hash, Serialize, Deserialize)]
pub enum Dx12Compiler {
    #[default]
    Fxc,
    Dxc,
}

impl Dx12Compiler {
    pub const ALL: [Dx12Compiler; 2] = [Dx12Compiler::Fxc, Dx12Compiler::Dxc];

    pub const fn id(self) -> &'static str {
        match self {
            Dx12Compiler::Fxc => "Fxc",
            Dx12Compiler::Dxc => "Dxc",
        }
    }
}

#[derive(Clone, Copy, Debug, Default, PartialEq, Eq, Hash, Serialize, Deserialize)]
pub enum LitQuality {
    High,
    Balanced,
    #[default]
    Auto,
}

impl LitQuality {
    pub const ALL: [LitQuality; 3] = [LitQuality::Auto, LitQuality::High, LitQuality::Balanced];

    pub const fn id(self) -> &'static str {
        match self {
            LitQuality::High => "High",
            LitQuality::Balanced => "Balanced",
            LitQuality::Auto => "Auto",
        }
    }
}

pub const BALANCED_LIGHTS: u32 = 32;

pub const MIN_RENDER_SCALE: f32 = 0.25;

pub const MAX_ANIMATION_FPS: u32 = 240;

pub const MIN_VRAM_BUDGET_MB: u32 = 256;

pub const CUBEMAPS_REVISION: u32 = 1;
pub const MAX_VRAM_BUDGET_MB: u32 = 262_144;

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
    pub object_colors_2d: bool,
    pub object_colors_3d: bool,
    pub highlight_invalid: bool,
    pub render_effects: bool,
    pub sky_preview: bool,
    pub skybox_3d: bool,
    pub sky_see_through: bool,
    pub water_preview: bool,
    pub detail_preview: bool,
    pub water_detail: WaterDetailSetting,
    pub fog_preview: bool,
    pub fog_volume_preview: bool,
    pub fog_volume_texture: bool,
    pub fish_preview: bool,
    pub fish_swim: bool,
    pub normal_maps: bool,
    pub player_compiled_lighting: bool,
    pub cubemaps: bool,
    #[serde(default)]
    pub cubemaps_revision: u32,
    pub cubemaps_auto: bool,
    pub light_shadows: u32,
    pub light_preview_hdr: bool,
    pub light_preview_match_compile: bool,
    pub projected_textures: bool,
    pub lit_indirect: bool,
    pub model_lods: bool,
    pub game_pixel_grid: bool,
    pub sun_glow: bool,
    pub tone_mapping: ToneModeSetting,
    pub exposure: f32,
    pub exposure_min: f32,
    pub exposure_max: f32,
    pub bloom: f32,
    pub map_tonemap: bool,
    pub sky_occlusion: bool,
    pub prop_shadows: PropShadowSetting,
    pub animate_materials: bool,
    pub animation_fps: u32,
    pub animate_in_background: bool,
    pub animation_idle_pause_s: u32,
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
    pub disp_walk_mode: DispWalkMode,
    pub disp_walk_angle: f32,
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
    pub vram_budget_mb: u32,
    pub graphics_memory: GraphicsMemory,
    pub keep_background_maps_on_gpu: bool,
    pub texture_streaming: bool,
    pub performance_mode: PerformanceMode,
    pub lit_quality: LitQuality,
    pub render_scale: f32,
    pub perf_hud: bool,
    pub primitive_stats: bool,
    pub graphics_backend: GraphicsBackend,
    pub dx12_compiler: Dx12Compiler,
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
            object_colors_2d: true,
            object_colors_3d: false,
            highlight_invalid: true,
            render_effects: true,
            sky_preview: true,
            skybox_3d: true,
            sky_see_through: false,
            water_preview: true,
            detail_preview: false,
            water_detail: WaterDetailSetting::ReflectAll,
            fog_preview: true,
            fog_volume_preview: true,
            fog_volume_texture: true,
            fish_preview: true,
            fish_swim: true,
            normal_maps: true,
            player_compiled_lighting: true,
            cubemaps: false,
            cubemaps_revision: CUBEMAPS_REVISION,
            cubemaps_auto: true,
            light_shadows: 16,
            light_preview_hdr: true,
            light_preview_match_compile: true,
            projected_textures: true,
            lit_indirect: true,
            model_lods: true,
            game_pixel_grid: true,
            sun_glow: true,
            tone_mapping: ToneModeSetting::Auto,
            exposure: 1.0,
            exposure_min: 0.5,
            exposure_max: 2.0,
            bloom: 1.0,
            map_tonemap: true,
            sky_occlusion: true,
            prop_shadows: PropShadowSetting::Auto,
            animate_materials: true,
            animation_fps: 30,
            animate_in_background: false,
            animation_idle_pause_s: 120,
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
            disp_walk_mode: DispWalkMode::Source,
            disp_walk_angle: DEFAULT_WALK_ANGLE,
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
            vram_budget_mb: 0,
            graphics_memory: GraphicsMemory::Auto,
            keep_background_maps_on_gpu: true,
            texture_streaming: true,
            performance_mode: PerformanceMode::Auto,
            lit_quality: LitQuality::Auto,
            render_scale: 1.0,
            perf_hud: false,
            primitive_stats: false,
            graphics_backend: GraphicsBackend::Auto,
            dx12_compiler: Dx12Compiler::Fxc,
        }
    }
}

impl RenderSettings {
    pub fn migrate(&mut self) {
        if self.cubemaps_revision < CUBEMAPS_REVISION {
            self.cubemaps = false;
            self.cubemaps_revision = CUBEMAPS_REVISION;
        }
    }

    pub fn animation_interval(&self, focused: bool, idle_secs: f64) -> Option<std::time::Duration> {
        if !focused && !self.animate_in_background {
            return None;
        }
        if self.animation_idle_pause_s > 0 && idle_secs >= f64::from(self.animation_idle_pause_s) {
            return None;
        }
        match self.animation_fps {
            0 => Some(std::time::Duration::ZERO),
            fps => Some(std::time::Duration::from_secs_f64(1.0 / f64::from(fps.min(MAX_ANIMATION_FPS)))),
        }
    }

    pub fn lighting_hdr(&self, mode: super::RenderModeSetting, lighting: super::LightingSetting) -> bool {
        if matches!(mode, super::RenderModeSetting::Baked | super::RenderModeSetting::LightmapGrid) {
            matches!(lighting, super::LightingSetting::Hdr | super::LightingSetting::Both)
        } else {
            self.preview_hdr(lighting)
        }
    }

    pub fn preview_hdr(&self, lighting: super::LightingSetting) -> bool {
        if self.light_preview_match_compile {
            matches!(lighting, super::LightingSetting::Hdr | super::LightingSetting::Both)
        } else {
            self.light_preview_hdr
        }
    }

    pub fn tone(&self) -> hp_render::ToneSettings {
        hp_render::ToneSettings {
            mode: self.tone_mapping.mode(),
            exposure: self.exposure,
            exposure_min: self.exposure_min,
            exposure_max: self.exposure_max,
            bloom: self.bloom,
            map_overrides: self.map_tonemap,
        }
    }

    pub fn displacements_2d_of(&self, index: usize) -> Disp2dSetting {
        match self.displacements_2d_views.get(index) {
            Some(Disp2dViewSetting::Own(m)) => *m,
            _ => self.displacements_2d,
        }
    }

    pub fn vram_budget(&self) -> Option<u64> {
        (self.vram_budget_mb > 0)
            .then(|| u64::from(self.vram_budget_mb.clamp(MIN_VRAM_BUDGET_MB, MAX_VRAM_BUDGET_MB)) << 20)
    }

    pub fn scale(&self) -> f32 {
        if self.render_scale.is_finite() { self.render_scale.clamp(MIN_RENDER_SCALE, 1.0) } else { 1.0 }
    }

    pub fn near(&self) -> f32 {
        if self.near_plane.is_finite() { self.near_plane.clamp(0.5, 16.0) } else { 1.0 }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn graphics_backend_parses_and_defaults_to_auto() {
        assert_eq!(RenderSettings::default().graphics_backend, GraphicsBackend::Auto);
        let old: RenderSettings = toml::from_str("vsync = true").expect("parse");
        assert_eq!(old.graphics_backend, GraphicsBackend::Auto);
        assert_eq!(old.dx12_compiler, Dx12Compiler::Fxc, "DXC is only used when turned on");
        let dxc: RenderSettings = toml::from_str("dx12_compiler = \"Dxc\"").expect("parse");
        assert_eq!(dxc.dx12_compiler, Dx12Compiler::Dxc);
        assert_eq!(Dx12Compiler::ALL.map(Dx12Compiler::id), ["Fxc", "Dxc"]);
        let set: RenderSettings = toml::from_str("graphics_backend = \"Dx12\"").expect("parse");
        assert_eq!(set.graphics_backend, GraphicsBackend::Dx12);
        assert_eq!(GraphicsBackend::parse("Vulkan"), Some(GraphicsBackend::Vulkan));
        assert_eq!(GraphicsBackend::parse("dx12"), Some(GraphicsBackend::Dx12));
        assert_eq!(GraphicsBackend::parse(" AUTO "), Some(GraphicsBackend::Auto));
        assert_eq!(GraphicsBackend::parse("gl"), None);
        for b in GraphicsBackend::ALL {
            assert_eq!(GraphicsBackend::parse(b.id()), Some(b));
        }
    }

    #[test]
    fn animations_pace_and_pause() {
        use std::time::Duration;
        let s = RenderSettings::default();
        assert_eq!(s.animation_interval(true, 0.0), Some(Duration::from_secs_f64(1.0 / 30.0)));
        assert_eq!(s.animation_interval(false, 0.0), None, "paused in the background");
        assert_eq!(s.animation_interval(true, 119.0), Some(Duration::from_secs_f64(1.0 / 30.0)));
        assert_eq!(s.animation_interval(true, 120.0), None, "paused after two minutes without input");
        let every = RenderSettings { animation_fps: 0, animate_in_background: true, animation_idle_pause_s: 0, ..s };
        assert_eq!(every.animation_interval(false, 1e9), Some(Duration::ZERO));
        let fast = RenderSettings { animation_fps: 100_000, ..RenderSettings::default() };
        assert_eq!(fast.animation_interval(true, 0.0), Some(Duration::from_secs_f64(1.0 / 240.0)));
        let old: RenderSettings = toml::from_str("animate_materials = true").expect("parse");
        assert_eq!((old.animation_fps, old.animate_in_background, old.animation_idle_pause_s), (30, false, 120));
    }

    #[test]
    fn lighting_range_follows_compilation_and_preserves_manual_comparisons() {
        use crate::settings::LightingSetting;
        let mut settings: RenderSettings = toml::from_str("light_preview_hdr = true").unwrap();
        assert!(!settings.preview_hdr(LightingSetting::Ldr));
        assert!(settings.preview_hdr(LightingSetting::Hdr));
        assert!(settings.preview_hdr(LightingSetting::Both));
        settings.light_preview_match_compile = false;
        assert!(settings.preview_hdr(LightingSetting::Ldr));
        assert!(!settings.lighting_hdr(crate::settings::RenderModeSetting::Baked, LightingSetting::Ldr));
        assert!(settings.lighting_hdr(crate::settings::RenderModeSetting::Lit, LightingSetting::Ldr));
        settings.light_preview_hdr = false;
        let restored: RenderSettings = toml::from_str(&toml::to_string(&settings).unwrap()).unwrap();
        assert!(!restored.preview_hdr(LightingSetting::Hdr));
        assert!(restored.lighting_hdr(crate::settings::RenderModeSetting::Baked, LightingSetting::Hdr));
        assert!(!restored.light_preview_match_compile);
    }

    #[test]
    fn cubemap_reflections_turn_off_once_for_older_settings() {
        let mut old: RenderSettings = toml::from_str("cubemaps = true\ncubemaps_auto = false\n").expect("parse");
        assert_eq!(old.cubemaps_revision, 0);
        old.migrate();
        assert!(!old.cubemaps, "older files get the new default");
        assert!(!old.cubemaps_auto, "other choices are kept");
        assert_eq!(old.cubemaps_revision, CUBEMAPS_REVISION);
        old.cubemaps = true;
        let mut saved: RenderSettings = toml::from_str(&toml::to_string(&old).expect("write")).expect("read");
        saved.migrate();
        assert!(saved.cubemaps, "turning them on again is kept after the migration");
        let mut fresh = RenderSettings::default();
        fresh.migrate();
        assert!(!fresh.cubemaps);
        let mut missing: RenderSettings = toml::from_str("grid_3d = true\n").expect("parse");
        missing.migrate();
        assert!(!missing.cubemaps);
    }

    #[test]
    fn graphics_memory_defaults_to_auto_and_round_trips() {
        let old: RenderSettings = toml::from_str(
            "vram_budget_mb = 0
",
        )
        .expect("parse");
        assert_eq!(old.graphics_memory, GraphicsMemory::Auto, "older files get Auto");
        for m in GraphicsMemory::ALL {
            let r: RenderSettings = toml::from_str(&format!("graphics_memory = \"{}\"", m.id())).expect("parse");
            assert_eq!(r.graphics_memory, m);
        }
    }

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
        assert_eq!(s.disp_walk_mode, DispWalkMode::Source);
        assert_eq!(s.disp_walk_angle, DEFAULT_WALK_ANGLE);
        assert!(s.normal_maps, "normal maps are on by default");
        assert!(!s.cubemaps && s.cubemaps_auto, "reflections are off by default");
        assert!(s.fog_preview && s.fog_volume_preview && s.fog_volume_texture, "fog is previewed by default");
        assert!(s.fish_preview && s.fish_swim, "fish are previewed by default");
        let off: RenderSettings = toml::from_str(
            "normal_maps = false
",
        )
        .expect("parse");
        assert!(!off.normal_maps);
        assert_eq!(s.prop_shadows, PropShadowSetting::Auto);
        let mesh: RenderSettings = toml::from_str("prop_shadows = \"Collision\"").expect("parse");
        assert_eq!(mesh.prop_shadows, PropShadowSetting::Collision);
    }

    #[test]
    fn performance_settings_default_to_full_quality_and_load_by_name() {
        let s: RenderSettings = toml::from_str(
            "grid_3d = true
",
        )
        .expect("parse");
        assert_eq!(s.performance_mode, PerformanceMode::Auto);
        assert_eq!((s.render_scale, s.scale(), s.perf_hud), (1.0, 1.0, false));
        for m in PerformanceMode::ALL {
            let r: RenderSettings = toml::from_str(&format!("performance_mode = \"{}\"", m.id())).expect("parse");
            assert_eq!(r.performance_mode, m);
        }
        let odd = RenderSettings { render_scale: 0.01, ..Default::default() };
        assert_eq!(odd.scale(), MIN_RENDER_SCALE);
        assert_eq!(RenderSettings { render_scale: 7.0, ..Default::default() }.scale(), 1.0);
        assert_eq!(RenderSettings { render_scale: f32::NAN, ..Default::default() }.scale(), 1.0);
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
