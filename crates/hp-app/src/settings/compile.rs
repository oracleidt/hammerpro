use std::collections::BTreeMap;
use std::path::PathBuf;

use serde::{Deserialize, Serialize};

#[derive(Clone, Copy, Debug, Default, PartialEq, Eq, Serialize, Deserialize)]
pub enum CompilePresetSetting {
    #[serde(alias = "Fast")]
    Draft,
    #[default]
    Normal,
    Final,
    Custom,
}

#[derive(Clone, Copy, Debug, Default, PartialEq, Eq, Serialize, Deserialize)]
pub enum LightingSetting {
    #[default]
    Ldr,
    Hdr,
    Both,
}

#[derive(Clone, Copy, Debug, Default, PartialEq, Eq, Serialize, Deserialize)]
pub enum BspModeSetting {
    #[default]
    Normal,
    OnlyEnts,
    OnlyProps,
}

#[derive(Clone, Copy, Debug, Default, PartialEq, Eq, Serialize, Deserialize)]
pub enum VisModeSetting {
    Fast,
    #[default]
    Normal,
}

#[derive(Clone, Copy, Debug, Default, PartialEq, Eq, Serialize, Deserialize)]
pub enum RadModeSetting {
    Draft,
    #[default]
    Normal,
    Final,
}

#[derive(Clone, Copy, Debug, Default, PartialEq, Eq, Serialize, Deserialize)]
pub enum PackPolicy {
    #[default]
    ReleaseOnly,
    Always,
}

#[derive(Clone, Copy, Debug, Default, PartialEq, Eq, Hash, Serialize, Deserialize)]
pub enum StaticPropShadows {
    None,
    #[default]
    Collision,
    Mesh,
}

#[derive(Clone, Copy, Debug, Default, PartialEq, Eq, Hash, Serialize, Deserialize)]
pub enum RayQualitySetting {
    Draft,
    #[default]
    Normal,
    Final,
    Optimal,
}

impl RayQualitySetting {
    pub const ALL: [RayQualitySetting; 4] =
        [RayQualitySetting::Draft, RayQualitySetting::Normal, RayQualitySetting::Final, RayQualitySetting::Optimal];

    pub fn name(self) -> &'static str {
        match self {
            RayQualitySetting::Draft => "draft",
            RayQualitySetting::Normal => "normal",
            RayQualitySetting::Final => "final",
            RayQualitySetting::Optimal => "optimal",
        }
    }

    pub fn parse(text: &str) -> Option<RayQualitySetting> {
        let t = text.trim();
        RayQualitySetting::ALL.into_iter().find(|q| q.name().eq_ignore_ascii_case(t))
    }
}

#[derive(Clone, Copy, Debug, Default, PartialEq, Eq, Hash, Serialize, Deserialize)]
pub enum RayDeviceSetting {
    #[default]
    Auto,
    Gpu,
    Cpu,
}

impl RayDeviceSetting {
    pub const ALL: [RayDeviceSetting; 3] = [RayDeviceSetting::Auto, RayDeviceSetting::Gpu, RayDeviceSetting::Cpu];

    pub fn name(self) -> &'static str {
        match self {
            RayDeviceSetting::Auto => "auto",
            RayDeviceSetting::Gpu => "gpu",
            RayDeviceSetting::Cpu => "cpu",
        }
    }

    pub fn parse(text: &str) -> Option<RayDeviceSetting> {
        let t = text.trim();
        RayDeviceSetting::ALL.into_iter().find(|d| d.name().eq_ignore_ascii_case(t))
    }
}

#[derive(Clone, Copy, Debug, Default, PartialEq, Eq, Hash, Serialize, Deserialize)]
pub enum RayConfigSetting {
    #[default]
    Default,
    Optimal,
    Custom,
}

impl RayConfigSetting {
    pub const ALL: [RayConfigSetting; 3] =
        [RayConfigSetting::Default, RayConfigSetting::Optimal, RayConfigSetting::Custom];

    pub fn name(self) -> &'static str {
        match self {
            RayConfigSetting::Default => "default",
            RayConfigSetting::Optimal => "optimal",
            RayConfigSetting::Custom => "custom",
        }
    }

    pub fn parse(text: &str) -> Option<RayConfigSetting> {
        let t = text.trim();
        RayConfigSetting::ALL.into_iter().find(|c| c.name().eq_ignore_ascii_case(t))
    }
}

#[derive(Clone, Copy, Debug, Default, PartialEq, Eq, Hash, Serialize, Deserialize)]
pub enum RayShaftSourceSetting {
    Sun,
    #[default]
    SunAndSpots,
}

impl RayShaftSourceSetting {
    pub const ALL: [RayShaftSourceSetting; 2] = [RayShaftSourceSetting::Sun, RayShaftSourceSetting::SunAndSpots];

    pub fn name(self) -> &'static str {
        match self {
            RayShaftSourceSetting::Sun => "sun",
            RayShaftSourceSetting::SunAndSpots => "sun_and_spots",
        }
    }

    pub fn parse(text: &str) -> Option<RayShaftSourceSetting> {
        let t = text.trim();
        RayShaftSourceSetting::ALL.into_iter().find(|c| c.name().eq_ignore_ascii_case(t))
    }
}

#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
#[serde(default)]
pub struct RayTracingSettings {
    pub on: bool,
    pub config: RayConfigSetting,
    pub quality: RayQualitySetting,
    pub device: RayDeviceSetting,
    pub seed: u32,
    pub min_samples: Option<u32>,
    pub max_samples: Option<u32>,
    pub bounces: Option<u32>,
    pub threshold: Option<f32>,
    pub light_size: Option<f32>,
    pub sun_angle: Option<f32>,
    pub sky_samples: Option<u32>,
    pub denoise: Option<f32>,
    pub supersample: Option<u32>,
    pub bump: bool,
    pub prop_samples: Option<u32>,
    pub chunk: Option<u32>,
    pub ao: bool,
    pub ao_strength: f32,
    pub ao_radius: f32,
    pub shafts: bool,
    pub shaft_intensity: f32,
    pub shaft_length: f32,
    pub shaft_softness: f32,
    pub shaft_light_color: bool,
    pub shaft_source: RayShaftSourceSetting,
    pub shaft_dust: bool,
    pub glow: bool,
    pub glow_intensity: f32,
    pub auto_cubemaps: bool,
    pub tonemap: bool,
    pub color_correction: bool,
    pub preview_gpu_share: f32,
}

impl Default for RayTracingSettings {
    fn default() -> Self {
        Self {
            on: false,
            config: RayConfigSetting::Default,
            quality: RayQualitySetting::Normal,
            device: RayDeviceSetting::Auto,
            seed: 0,
            min_samples: None,
            max_samples: None,
            bounces: None,
            threshold: None,
            light_size: None,
            sun_angle: None,
            sky_samples: None,
            denoise: None,
            supersample: None,
            bump: true,
            prop_samples: None,
            chunk: None,
            ao: true,
            ao_strength: 0.5,
            ao_radius: 48.0,
            shafts: true,
            shaft_intensity: 1.0,
            shaft_length: 1.0,
            shaft_softness: 1.0,
            shaft_light_color: true,
            shaft_source: RayShaftSourceSetting::SunAndSpots,
            shaft_dust: false,
            glow: false,
            glow_intensity: 1.0,
            auto_cubemaps: true,
            tonemap: true,
            color_correction: false,
            preview_gpu_share: 0.0,
        }
    }
}

impl RayTracingSettings {
    pub fn reset_to_default(&mut self) {
        *self = Self { on: self.on, preview_gpu_share: self.preview_gpu_share, ..Self::default() };
    }

    pub fn preview_share(&self) -> Option<f32> {
        let v = self.preview_gpu_share;
        (v.is_finite() && v > 0.0).then(|| v.clamp(0.1, 1.0))
    }

    pub fn make_optimal(&mut self) {
        *self = Self {
            on: true,
            config: RayConfigSetting::Optimal,
            quality: RayQualitySetting::Optimal,
            device: self.device,
            seed: self.seed,
            preview_gpu_share: self.preview_gpu_share,
            ..Self::default()
        };
    }

    pub fn is_default_state(&self) -> bool {
        *self == Self { on: self.on, config: self.config, preview_gpu_share: self.preview_gpu_share, ..Self::default() }
    }

    pub fn settle_after_change(&mut self, before: &RayTracingSettings) {
        let same = |a: &RayTracingSettings| RayTracingSettings {
            on: false,
            config: RayConfigSetting::Default,
            preview_gpu_share: 0.0,
            ..a.clone()
        };
        if same(self) != same(before) {
            self.config = if self.is_default_state() { RayConfigSetting::Default } else { RayConfigSetting::Custom };
        }
    }
}

fn is_flag(token: &str, name: &str) -> bool {
    token.trim().trim_start_matches('-').eq_ignore_ascii_case(name)
}

#[derive(Clone, Debug, Default, PartialEq, Eq, Serialize, Deserialize)]
#[serde(default)]
pub struct FlagChoiceSetting {
    pub on: bool,
    pub value: String,
}

#[derive(Clone, Debug, Default, PartialEq, Eq, Serialize, Deserialize)]
#[serde(default)]
pub struct ProfileStep {
    pub enabled: bool,
    pub action: String,
    pub program: String,
    pub args: String,
    pub continue_on_error: bool,
}

#[derive(Clone, Debug, Default, PartialEq, Eq, Serialize, Deserialize)]
#[serde(default)]
pub struct CompileProfile {
    pub name: String,
    pub steps: Vec<ProfileStep>,
}

#[derive(Clone, Debug, Default, PartialEq, Eq, Serialize, Deserialize)]
#[serde(default)]
pub struct WindowPlacement {
    pub x: i32,
    pub y: i32,
    pub width: i32,
    pub height: i32,
    pub monitor: String,
    pub monitor_x: i32,
    pub monitor_y: i32,
}

#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
#[serde(default)]
pub struct CompileSettings {
    pub preset: CompilePresetSetting,
    pub lighting: LightingSetting,
    pub run_bsp: bool,
    pub run_vis: bool,
    pub run_rad: bool,
    pub pack_custom_content: bool,
    pub export_custom_materials: bool,
    pub copy_to_maps: bool,
    pub bsp_beside_vmf: bool,
    pub launch_game: bool,
    pub launch_via_steam: bool,
    pub game_args: String,
    pub threads: Option<u32>,
    pub extra_bsp_args: String,
    pub extra_rad_args: String,
    pub save_before_compile: bool,
    pub load_pointfile_on_leak: bool,
    pub bsp_mode: BspModeSetting,
    pub vis_mode: VisModeSetting,
    pub rad_mode: RadModeSetting,
    pub low_priority: bool,
    pub stop_on_leak: bool,
    pub compile_hidden: bool,
    pub smart_recompile: bool,
    pub background_precompile: bool,
    pub pack_policy: PackPolicy,
    pub dev_addon: bool,
    pub game_exe_path: Option<PathBuf>,
    pub model_compiler: Option<PathBuf>,
    pub model_compiler_args: String,
    pub maps_dir_override: Option<PathBuf>,
    pub compile_cordon_only: bool,
    pub cordon_material: String,
    pub progressive: bool,
    pub progressive_preset: CompilePresetSetting,
    pub history: bool,
    pub static_prop_lighting: bool,
    pub static_prop_shadows: bool,
    pub raytracing: RayTracingSettings,
    pub bsp_flag_choices: BTreeMap<String, FlagChoiceSetting>,
    pub rad_flag_choices: BTreeMap<String, FlagChoiceSetting>,
    pub expert: bool,
    pub profiles: Vec<CompileProfile>,
    pub active_profile: usize,
    pub separate_window: bool,
    pub window: Option<WindowPlacement>,
    pub compilers: Vec<super::compilers::CompilerProfile>,
    pub compiler_by_game: BTreeMap<String, String>,
    #[serde(skip)]
    pub target: Option<String>,
    #[serde(skip)]
    pub target_dir: Option<PathBuf>,
}

pub fn background_precompile_default() -> bool {
    false
}

pub fn background_precompile_supported() -> bool {
    std::thread::available_parallelism().is_ok_and(|n| n.get() >= 8)
}

impl Default for CompileSettings {
    fn default() -> Self {
        Self {
            preset: CompilePresetSetting::Normal,
            lighting: LightingSetting::Ldr,
            run_bsp: true,
            run_vis: true,
            run_rad: true,
            pack_custom_content: true,
            export_custom_materials: false,
            copy_to_maps: true,
            bsp_beside_vmf: false,
            launch_game: true,
            launch_via_steam: false,
            game_args: "-console -dev".to_owned(),
            threads: None,
            extra_bsp_args: String::new(),
            extra_rad_args: String::new(),
            save_before_compile: false,
            load_pointfile_on_leak: true,
            bsp_mode: BspModeSetting::Normal,
            vis_mode: VisModeSetting::Normal,
            rad_mode: RadModeSetting::Normal,
            low_priority: true,
            stop_on_leak: true,
            compile_hidden: false,
            smart_recompile: true,
            background_precompile: background_precompile_default(),
            pack_policy: PackPolicy::ReleaseOnly,
            dev_addon: true,
            game_exe_path: None,
            model_compiler: None,
            model_compiler_args: String::new(),
            maps_dir_override: None,
            target: None,
            target_dir: None,
            compile_cordon_only: true,
            cordon_material: "TOOLS/TOOLSSKYBOX".to_owned(),
            progressive: false,
            progressive_preset: CompilePresetSetting::Normal,
            history: true,
            static_prop_lighting: true,
            static_prop_shadows: true,
            raytracing: RayTracingSettings::default(),
            bsp_flag_choices: BTreeMap::new(),
            rad_flag_choices: BTreeMap::new(),
            expert: false,
            profiles: Vec::new(),
            active_profile: 0,
            separate_window: true,
            window: None,
            compilers: Vec::new(),
            compiler_by_game: BTreeMap::new(),
        }
    }
}

impl CompileSettings {
    pub fn game_key(&self) -> String {
        self.target.as_deref().map(str::trim).filter(|t| !t.is_empty()).unwrap_or(hp_fs::GMOD_ID).to_ascii_lowercase()
    }

    pub fn active_compiler_id(&self) -> &str {
        self.compiler_by_game
            .get(&self.game_key())
            .map(String::as_str)
            .filter(|id| !super::compilers::is_builtin_id(id))
            .unwrap_or(super::compilers::BUILTIN_COMPILER)
    }

    pub fn uses_builtin_compiler(&self) -> bool {
        super::compilers::is_builtin_id(self.active_compiler_id())
    }

    pub fn set_active_compiler(&mut self, id: &str) {
        let key = self.game_key();
        if super::compilers::is_builtin_id(id) {
            self.compiler_by_game.remove(&key);
        } else {
            self.compiler_by_game.insert(key, id.trim().to_owned());
        }
    }

    pub fn remove_compiler(&mut self, id: &str) -> bool {
        let before = self.compilers.len();
        self.compilers.retain(|p| !p.id.eq_ignore_ascii_case(id));
        self.compiler_by_game.retain(|_, v| !v.eq_ignore_ascii_case(id));
        self.compilers.len() != before
    }

    pub fn reset_compilers(&mut self) {
        self.compilers.clear();
        self.compiler_by_game.clear();
    }

    pub fn static_prop_shadows(&self) -> StaticPropShadows {
        const POLYS: &str = "StaticPropPolys";
        const NONE: &str = "nostaticpropshadows";
        let shape = |polys: bool| if polys { StaticPropShadows::Mesh } else { StaticPropShadows::Collision };
        if self.uses_builtin_compiler() {
            let extra: Vec<&str> = self.extra_rad_args.split_whitespace().collect();
            if !self.static_prop_shadows || extra.iter().any(|t| is_flag(t, NONE)) {
                return StaticPropShadows::None;
            }
            let chosen = self.rad_flag_choices.iter().find(|(k, _)| is_flag(k, POLYS)).map(|(_, c)| c.on);
            return shape(chosen.unwrap_or_else(|| extra.iter().any(|t| is_flag(t, POLYS))));
        }
        let id = self.active_compiler_id();
        let Some(profile) = self.compilers.iter().find(|p| p.id.eq_ignore_ascii_case(id)) else {
            return StaticPropShadows::Collision;
        };
        let polys = profile.steps.iter().filter(|st| st.enabled && st.kind == super::CompilerStepKind::Rad).any(|st| {
            st.options.keys().any(|k| is_flag(k, POLYS))
                || st.args.split_whitespace().chain(st.extra.split_whitespace()).any(|t| is_flag(t, POLYS))
        });
        shape(polys)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn static_prop_shadows_follow_the_compile() {
        let mut s = CompileSettings::default();
        assert_eq!(s.static_prop_shadows(), StaticPropShadows::Collision, "plain settings: collision models");
        s.extra_rad_args = "-bounce 2 -staticproppolys".into();
        assert_eq!(s.static_prop_shadows(), StaticPropShadows::Mesh);
        s.rad_flag_choices.insert("-StaticPropPolys".into(), FlagChoiceSetting { on: false, value: String::new() });
        assert_eq!(s.static_prop_shadows(), StaticPropShadows::Collision, "a choice turned off wins over the extra");
        s.extra_rad_args.clear();
        s.rad_flag_choices.insert("-StaticPropPolys".into(), FlagChoiceSetting { on: true, value: String::new() });
        assert_eq!(s.static_prop_shadows(), StaticPropShadows::Mesh);
        s.static_prop_shadows = false;
        assert_eq!(s.static_prop_shadows(), StaticPropShadows::None);
        s.static_prop_shadows = true;
        s.extra_rad_args = "-NoStaticPropShadows".into();
        assert_eq!(s.static_prop_shadows(), StaticPropShadows::None);
        s.extra_rad_args.clear();
        let mut vrad = super::super::CompilerProfile::blank("VRAD");
        vrad.id = "user:1".into();
        s.compilers.push(vrad.clone());
        s.set_active_compiler("user:1");
        assert_eq!(s.static_prop_shadows(), StaticPropShadows::Collision, "Valve's VRAD without the flag");
        if let Some(st) = s.compilers[0].steps.iter_mut().find(|st| st.kind == super::super::CompilerStepKind::Rad) {
            st.options.insert("StaticPropPolys".into(), String::new());
        }
        assert_eq!(s.static_prop_shadows(), StaticPropShadows::Mesh);
        if let Some(st) = s.compilers[0].steps.iter_mut().find(|st| st.kind == super::super::CompilerStepKind::Rad) {
            st.options.clear();
            st.extra = "-final -StaticPropPolys".into();
        }
        assert_eq!(s.static_prop_shadows(), StaticPropShadows::Mesh);
        if let Some(st) = s.compilers[0].steps.iter_mut().find(|st| st.kind == super::super::CompilerStepKind::Rad) {
            st.enabled = false;
        }
        assert_eq!(s.static_prop_shadows(), StaticPropShadows::Collision);
        s.set_active_compiler("detected:vrad");
        assert_eq!(s.static_prop_shadows(), StaticPropShadows::Collision, "an unedited detected compiler");
    }

    #[test]
    fn old_files_load() {
        let s: CompileSettings = toml::from_str("preset = \"Fast\"\nrun_vis = false\n").expect("parse");
        assert_eq!(s.preset, CompilePresetSetting::Draft);
        assert!(!s.run_vis);
        assert!(s.low_priority && s.stop_on_leak && s.smart_recompile);
        assert_eq!(s.cordon_material, "TOOLS/TOOLSSKYBOX");
        let round: CompileSettings = toml::from_str(&toml::to_string(&s).expect("write")).expect("read");
        assert_eq!(round, s);
    }

    #[test]
    fn prop_options_and_flag_choices() {
        let old: CompileSettings = toml::from_str(
            "preset = \"Final\"
extra_rad_args = \"-bounce 4\"
",
        )
        .expect("old file");
        assert!(old.static_prop_lighting && old.static_prop_shadows);
        assert!(old.bsp_flag_choices.is_empty() && old.rad_flag_choices.is_empty());
        let all: crate::settings::Settings = toml::from_str(
            "[compile]
preset = \"Draft\"
",
        )
        .expect("settings file");
        assert!(all.compile.static_prop_lighting && all.compile.static_prop_shadows);

        let mut s = CompileSettings { static_prop_lighting: false, static_prop_shadows: false, ..old };
        s.rad_flag_choices.insert("-bounce".into(), FlagChoiceSetting { on: true, value: "8".into() });
        s.rad_flag_choices.insert("-fast".into(), FlagChoiceSetting { on: false, value: String::new() });
        s.bsp_flag_choices.insert("-trace".into(), FlagChoiceSetting { on: true, value: "3 7".into() });
        let text = toml::to_string(&s).expect("write");
        let back: CompileSettings = toml::from_str(&text).expect("read");
        assert_eq!(back, s);
        assert!(!back.static_prop_lighting && !back.static_prop_shadows);
        let file = crate::settings::Settings { compile: s.clone(), ..Default::default() };
        let back: crate::settings::Settings = toml::from_str(&toml::to_string(&file).expect("write")).expect("read");
        assert_eq!(back.compile, s);
    }

    #[test]
    fn ray_tracing_is_off_by_default_and_saved() {
        let old: crate::settings::Settings = toml::from_str(
            "[compile]
preset = \"Final\"
",
        )
        .expect("old file");
        assert_eq!(old.compile.raytracing, RayTracingSettings::default());
        assert!(!old.compile.raytracing.on && old.compile.raytracing.bump);
        let mut s = crate::settings::Settings::default();
        s.compile.raytracing = RayTracingSettings {
            on: true,
            quality: RayQualitySetting::Optimal,
            device: RayDeviceSetting::Cpu,
            seed: 9,
            bounces: Some(4),
            threshold: Some(0.03),
            bump: false,
            ..RayTracingSettings::default()
        };
        assert!(!s.compile.raytracing.is_default_state());
        let back: crate::settings::Settings = toml::from_str(&toml::to_string(&s).expect("write")).expect("read");
        assert_eq!(back.compile.raytracing, s.compile.raytracing);
        let part: CompileSettings = toml::from_str(
            "[raytracing]
on = true
",
        )
        .expect("partial");
        assert!(part.raytracing.on && part.raytracing.bump && part.raytracing.quality == RayQualitySetting::Normal);
        let mut rt = s.compile.raytracing.clone();
        rt.make_optimal();
        assert!(rt.on && rt.seed == 9 && rt.bump && rt.bounces.is_none() && rt.config == RayConfigSetting::Optimal);
        assert_eq!(rt.quality, RayQualitySetting::Optimal);
        let before = rt.clone();
        rt.glow = true;
        rt.settle_after_change(&before);
        assert_eq!(rt.config, RayConfigSetting::Custom);
        rt.reset_to_default();
        assert!(rt.on && rt.is_default_state() && rt.config == RayConfigSetting::Default && rt.ao && rt.shafts);
        let before = rt.clone();
        rt.on = false;
        rt.settle_after_change(&before);
        assert_eq!(rt.config, RayConfigSetting::Default, "the checkbox is not a change of configuration");
        let before = rt.clone();
        rt.quality = RayQualitySetting::Final;
        rt.settle_after_change(&before);
        assert_eq!(rt.config, RayConfigSetting::Custom);
        let before = rt.clone();
        rt.quality = RayQualitySetting::Normal;
        rt.settle_after_change(&before);
        assert_eq!(rt.config, RayConfigSetting::Default, "back to the default values");
        assert_eq!(RayConfigSetting::parse("Optimal"), Some(RayConfigSetting::Optimal));
        assert_eq!(RayShaftSourceSetting::parse("sun_and_spots"), Some(RayShaftSourceSetting::SunAndSpots));
        assert_eq!(RayQualitySetting::parse("FINAL"), Some(RayQualitySetting::Final));
        assert_eq!(RayDeviceSetting::parse("gpu"), Some(RayDeviceSetting::Gpu));
        assert!(RayQualitySetting::parse("best").is_none());
    }

    #[test]
    fn preview_gpu_share_is_apart_from_the_configuration() {
        let old: CompileSettings = toml::from_str("[raytracing]\non = true\n").expect("old");
        assert_eq!((old.raytracing.preview_gpu_share, old.raytracing.preview_share()), (0.0, None), "auto");
        let mut rt = old.raytracing.clone();
        let before = rt.clone();
        rt.preview_gpu_share = 0.35;
        rt.settle_after_change(&before);
        assert_eq!(rt.config, RayConfigSetting::Default, "a preview setting is not a configuration change");
        assert!(rt.is_default_state());
        assert_eq!(rt.preview_share(), Some(0.35));
        rt.reset_to_default();
        rt.make_optimal();
        assert_eq!(rt.preview_gpu_share, 0.35);
        rt.preview_gpu_share = 5.0;
        assert_eq!(rt.preview_share(), Some(1.0));
    }

    #[test]
    fn compile_window_place_is_kept() {
        let old: crate::settings::Settings = toml::from_str("[compile]\npreset = \"Draft\"\n").expect("old file");
        assert!(old.compile.separate_window && old.compile.window.is_none());
        let mut s = crate::settings::Settings::default();
        assert!(s.compile.separate_window);
        s.compile.separate_window = false;
        s.compile.window = Some(WindowPlacement {
            x: -1700,
            y: 40,
            width: 900,
            height: 700,
            monitor: "\\\\.\\DISPLAY2".into(),
            monitor_x: -1920,
            monitor_y: 0,
        });
        let text = toml::to_string(&s).expect("write");
        let back: crate::settings::Settings = toml::from_str(&text).expect("read");
        assert_eq!(back.compile, s.compile);
        let part: CompileSettings = toml::from_str("[window]\nx = 5\ny = 6\n").expect("partial");
        assert_eq!(part.window.map(|w| (w.x, w.y, w.width)), Some((5, 6, 0)));
    }

    #[test]
    fn compiler_profiles_and_choices_persist_per_game() {
        use crate::settings::compilers::{CompilerProfile, CompilerStep, CompilerStepKind};
        let old: crate::settings::Settings = toml::from_str("[compile]\npreset = \"Draft\"\n").expect("old file");
        assert!(old.compile.compilers.is_empty() && old.compile.uses_builtin_compiler());
        let mut s = crate::settings::Settings::default();
        let mut p = CompilerProfile::blank("Tools with spaces");
        p.id = "user:1".into();
        p.steps[0].exe = r"C:\Program Files\Valve tools\vbsp.exe".into();
        p.steps.push(CompilerStep::of_kind(CompilerStepKind::Game));
        s.compile.compilers.push(p);
        s.compile.set_active_compiler("user:1");
        assert_eq!(s.compile.active_compiler_id(), "user:1");
        s.compile.target = Some("cstrike".into());
        assert!(s.compile.uses_builtin_compiler(), "another game keeps its own choice");
        s.compile.set_active_compiler("detected:valve:bin");
        s.compile.target = None;
        let text = toml::to_string(&s).expect("write");
        let mut back: crate::settings::Settings = toml::from_str(&text).expect("read");
        assert_eq!(back.compile, s.compile);
        assert_eq!(back.compile.active_compiler_id(), "user:1");
        back.compile.target = Some("CStrike".into());
        assert_eq!(back.compile.active_compiler_id(), "detected:valve:bin");
        back.compile.set_active_compiler("builtin");
        assert!(back.compile.uses_builtin_compiler() && !back.compile.compiler_by_game.contains_key("cstrike"));
        back.compile.target = None;
        assert!(back.compile.remove_compiler("USER:1"));
        assert!(back.compile.compilers.is_empty() && back.compile.uses_builtin_compiler());
        assert!(!back.compile.remove_compiler("user:1"));
        back.compile.set_active_compiler("detected:valve:bin");
        back.compile.reset_compilers();
        assert!(back.compile.uses_builtin_compiler());
    }

    #[test]
    fn compiler_choice_keys_are_ignored() {
        let old = r#"
bsp_compiler = "External"
compiler_policy = "GameOnly"
compiler_dirs = ["D:/ToolsPP/bin"]
vbsp_path = "D:/custom/vbsp.exe"
vvis_path = "D:/custom/vvis.exe"
vrad_path = "D:/custom/vrad.exe"
bspzip_path = "D:/custom/bspzip.exe"
extra_vis_args = "-fast"
vis_radius = 4096
extra_bsp_args = "-leaktest"
preset = "Final"
low_priority = false
game_exe_path = "D:/game/gmod.exe"
"#;
        let s: CompileSettings = toml::from_str(old).expect("old settings load");
        assert_eq!(s.preset, CompilePresetSetting::Final);
        assert_eq!(s.extra_bsp_args, "-leaktest");
        assert!(!s.low_priority);
        assert_eq!(s.game_exe_path, Some(PathBuf::from("D:/game/gmod.exe")));
        let written = toml::to_string(&s).expect("write");
        for gone in ["bsp_compiler", "compiler_policy", "compiler_dirs", "vbsp_path", "bspzip_path", "vis_radius"] {
            assert!(!written.contains(gone), "{gone} is not written any more");
        }
        let file = format!("[compile]\n{old}");
        let all: crate::settings::Settings = toml::from_str(&file).expect("settings file loads");
        assert_eq!(all.compile.preset, CompilePresetSetting::Final);
    }
}
