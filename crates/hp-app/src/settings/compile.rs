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
