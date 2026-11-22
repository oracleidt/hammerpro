use serde::{Deserialize, Serialize};

pub const RAYTRACING_FLAG: &str = "-raytracing";
pub const LANG_FLAG: &str = "-rtlang";
pub const QUALITIES: [&str; 4] = ["draft", "normal", "final", "optimal"];
pub const DEVICES: [&str; 3] = ["auto", "gpu", "cpu"];

pub const RT_FLAGS: &[(&str, bool)] = &[
    (RAYTRACING_FLAG, false),
    ("-rtquality", true),
    ("-rtdevice", true),
    ("-rtseed", true),
    ("-rtsamples", true),
    ("-rtmaxsamples", true),
    ("-rtbounces", true),
    ("-rtthreshold", true),
    ("-rtlightsize", true),
    ("-rtsunangle", true),
    ("-rtskysamples", true),
    ("-rtdenoise", true),
    ("-rtsupersample", true),
    ("-rtnobump", false),
    ("-rtpropsamples", true),
    ("-rtchunk", true),
    ("-rtluxels", true),
    ("-rtlook", true),
    (LANG_FLAG, true),
    ("-rtao", false),
    ("-rtnoao", false),
    ("-rtaostrength", true),
    ("-rtaoradius", true),
    ("-rtshafts", false),
    ("-rtnoshafts", false),
    ("-rtshaftintensity", true),
    ("-rtshaftlength", true),
    ("-rtshaftsoftness", true),
    ("-rtshaftwhite", false),
    ("-rtshaftsource", true),
    ("-rtshaftdust", false),
    ("-rtglow", false),
    ("-rtglowintensity", true),
    ("-rtnocubemaps", false),
    ("-rtreflections", false),
    ("-rtnoreflections", false),
    ("-rtnotonemap", false),
    ("-rtcolorcorrection", false),
    (GPU_SHARE_FLAG, true),
];

pub const GPU_SHARE_FLAG: &str = "-rtgpushare";

pub fn gpu_share_args(share: Option<f32>) -> Vec<String> {
    share
        .filter(|v| v.is_finite() && *v > 0.0)
        .map(|v| vec![GPU_SHARE_FLAG.to_owned(), number(v.clamp(0.1, 1.0))])
        .unwrap_or_default()
}

pub const SHAFT_SOURCES: [&str; 2] = ["sun", "sun_and_spots"];

pub const REFLECTION_MODES: [&str; 3] = ["auto", "on", "off"];

pub const BSP_AUTO_CUBEMAPS: &str = "-rtautocubemaps";

pub fn bsp_args(rt: Option<&RayTracingOptions>) -> Vec<String> {
    rt.filter(|r| r.effects.auto_cubemaps).map(|_| vec![BSP_AUTO_CUBEMAPS.to_owned()]).unwrap_or_default()
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(default)]
pub struct RayEffectsOptions {
    pub ao: bool,
    pub ao_strength: f32,
    pub ao_radius: f32,
    #[serde(rename = "light_shafts")]
    pub shafts: bool,
    pub shaft_intensity: f32,
    pub shaft_length: f32,
    pub shaft_softness: f32,
    pub shaft_light_color: bool,
    pub shaft_source: String,
    pub shaft_dust: bool,
    pub glow: bool,
    pub glow_intensity: f32,
    pub auto_cubemaps: bool,
    pub reflections: String,
    pub tonemap: bool,
    pub color_correction: bool,
}

impl Default for RayEffectsOptions {
    fn default() -> Self {
        RayEffectsOptions {
            ao: true,
            ao_strength: 0.5,
            ao_radius: 48.0,
            shafts: false,
            shaft_intensity: 1.0,
            shaft_length: 1.0,
            shaft_softness: 1.0,
            shaft_light_color: true,
            shaft_source: "sun_and_spots".to_owned(),
            shaft_dust: false,
            glow: false,
            glow_intensity: 1.0,
            auto_cubemaps: true,
            reflections: "auto".to_owned(),
            tonemap: true,
            color_correction: false,
        }
    }
}

impl RayEffectsOptions {
    pub fn reflections_mode(&self) -> &'static str {
        match self.reflections.trim().to_ascii_lowercase().as_str() {
            "on" | "always" | "true" => "on",
            "off" | "never" | "false" => "off",
            _ => "auto",
        }
    }

    fn push_args(&self, out: &mut Vec<String>) {
        let d = RayEffectsOptions::default();
        let mut flag = |on: bool, name: &str| {
            if on {
                out.push(name.to_owned());
            }
        };
        flag(!self.ao, "-rtnoao");
        flag(self.shafts, "-rtshafts");
        flag(!self.shaft_light_color, "-rtshaftwhite");
        flag(self.shaft_dust, "-rtshaftdust");
        flag(self.glow, "-rtglow");
        flag(!self.auto_cubemaps, "-rtnocubemaps");
        flag(self.reflections_mode() == "on", "-rtreflections");
        flag(self.reflections_mode() == "off", "-rtnoreflections");
        flag(!self.tonemap, "-rtnotonemap");
        flag(self.color_correction, "-rtcolorcorrection");
        let reals = [
            ("-rtaostrength", self.ao_strength, d.ao_strength),
            ("-rtaoradius", self.ao_radius, d.ao_radius),
            ("-rtshaftintensity", self.shaft_intensity, d.shaft_intensity),
            ("-rtshaftlength", self.shaft_length, d.shaft_length),
            ("-rtshaftsoftness", self.shaft_softness, d.shaft_softness),
            ("-rtglowintensity", self.glow_intensity, d.glow_intensity),
        ];
        for (name, v, default) in reals {
            if v.is_finite() && (v - default).abs() > 1e-6 {
                out.push(name.to_owned());
                out.push(number(v));
            }
        }
        let source = self.shaft_source.trim().to_ascii_lowercase();
        if !source.is_empty() && source != d.shaft_source {
            out.push("-rtshaftsource".to_owned());
            out.push(source);
        }
    }
}

pub fn is_raytracing_flag(token: &str) -> bool {
    RT_FLAGS.iter().any(|(f, _)| f.eq_ignore_ascii_case(token.trim()))
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(default)]
pub struct RayTracingOptions {
    pub quality: String,
    pub device: String,
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
    pub luxel_detail: Option<f32>,
    pub look: String,
    pub lang: String,
    pub effects: RayEffectsOptions,
}

impl Default for RayTracingOptions {
    fn default() -> Self {
        RayTracingOptions {
            quality: "normal".to_owned(),
            device: "auto".to_owned(),
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
            luxel_detail: None,
            look: String::new(),
            lang: String::new(),
            effects: RayEffectsOptions::default(),
        }
    }
}

fn number(v: f32) -> String {
    let text = format!("{v:.4}");
    let text = text.trim_end_matches('0').trim_end_matches('.');
    if text.is_empty() || text == "-" { "0".to_owned() } else { text.to_owned() }
}

impl RayTracingOptions {
    pub fn args(&self) -> Vec<String> {
        let mut out = vec![RAYTRACING_FLAG.to_owned()];
        let mut push = |flag: &str, value: String| {
            out.push(flag.to_owned());
            out.push(value);
        };
        let quality = self.quality.trim().to_ascii_lowercase();
        if !quality.is_empty() && quality != "normal" {
            push("-rtquality", quality);
        }
        let device = self.device.trim().to_ascii_lowercase();
        if !device.is_empty() && device != "auto" {
            push("-rtdevice", device);
        }
        if self.seed != 0 {
            push("-rtseed", self.seed.to_string());
        }
        let ints = [
            ("-rtsamples", self.min_samples),
            ("-rtmaxsamples", self.max_samples),
            ("-rtbounces", self.bounces),
            ("-rtskysamples", self.sky_samples),
            ("-rtsupersample", self.supersample),
            ("-rtpropsamples", self.prop_samples),
            ("-rtchunk", self.chunk),
        ];
        let floats = [
            ("-rtthreshold", self.threshold),
            ("-rtlightsize", self.light_size),
            ("-rtsunangle", self.sun_angle),
            ("-rtdenoise", self.denoise),
            ("-rtluxels", self.luxel_detail),
        ];
        for (flag, v) in ints {
            if let Some(v) = v {
                push(flag, v.to_string());
            }
        }
        for (flag, v) in floats {
            if let Some(v) = v.filter(|v| v.is_finite()) {
                push(flag, number(v));
            }
        }
        let look = self.look.trim().to_ascii_lowercase();
        if look == "classic" || look == "cinematic" {
            out.push("-rtlook".to_owned());
            out.push(look);
        }
        if !self.bump {
            out.push("-rtnobump".to_owned());
        }
        self.effects.push_args(&mut out);
        let lang = self.lang.trim();
        if !lang.is_empty() {
            out.push(LANG_FLAG.to_owned());
            out.push(lang.to_owned());
        }
        out
    }
}

pub fn add_lang(args: &mut Vec<String>, lang: &str) {
    let lang = lang.trim();
    if lang.is_empty()
        || !args.iter().any(|a| is_raytracing_flag(a))
        || args.iter().any(|a| a.eq_ignore_ascii_case(LANG_FLAG))
    {
        return;
    }
    let at = 1.min(args.len());
    args.splice(at..at, [LANG_FLAG.to_owned(), lang.to_owned()]);
}

#[cfg(test)]
mod tests {
    use super::*;

    fn words(s: &str) -> Vec<String> {
        crate::split_args(s)
    }

    #[test]
    fn plain_options_only_turn_it_on() {
        assert_eq!(RayTracingOptions::default().args(), [RAYTRACING_FLAG]);
        let fr = RayTracingOptions { lang: "fr".into(), ..RayTracingOptions::default() };
        assert_eq!(fr.args(), words("-raytracing -rtlang fr"));
    }

    #[test]
    fn every_parameter_is_written() {
        let o = RayTracingOptions {
            quality: "Final".into(),
            device: "cpu".into(),
            seed: 7,
            min_samples: Some(32),
            max_samples: Some(512),
            bounces: Some(8),
            threshold: Some(0.025),
            light_size: Some(1.5),
            sun_angle: Some(0.5),
            sky_samples: Some(3),
            denoise: Some(0.0),
            supersample: Some(4),
            bump: false,
            prop_samples: Some(64),
            chunk: Some(65536),
            luxel_detail: Some(1.5),
            look: "Cinematic".into(),
            lang: "ru".into(),
            effects: RayEffectsOptions::default(),
        };
        assert_eq!(
            o.args(),
            words(
                "-raytracing -rtquality final -rtdevice cpu -rtseed 7 -rtsamples 32 -rtmaxsamples 512 -rtbounces 8 \
                 -rtskysamples 3 -rtsupersample 4 -rtpropsamples 64 -rtchunk 65536 -rtthreshold 0.025 -rtlightsize 1.5 \
                 -rtsunangle 0.5 -rtdenoise 0 -rtluxels 1.5 -rtlook cinematic -rtnobump -rtlang ru"
            )
        );
        for a in o.args().iter().filter(|a| a.starts_with('-')) {
            assert!(is_raytracing_flag(a), "{a}");
        }
        let text = serde_json::to_string(&o).expect("write");
        assert_eq!(serde_json::from_str::<RayTracingOptions>(&text).expect("read"), o);
        let old: RayTracingOptions = serde_json::from_str("{}").expect("old");
        assert_eq!(old, RayTracingOptions::default());
    }

    #[test]
    fn effects_write_only_what_differs_from_the_defaults() {
        let mut o = RayTracingOptions::default();
        assert_eq!(o.args(), [RAYTRACING_FLAG]);
        o.effects = RayEffectsOptions {
            ao: false,
            ao_strength: 0.75,
            ao_radius: 64.0,
            shafts: true,
            shaft_intensity: 2.0,
            shaft_length: 1.5,
            shaft_softness: 0.5,
            shaft_light_color: false,
            shaft_source: "sun".into(),
            shaft_dust: true,
            glow: true,
            glow_intensity: 3.0,
            auto_cubemaps: false,
            reflections: "off".into(),
            tonemap: false,
            color_correction: true,
        };
        assert_eq!(
            o.args(),
            words(
                "-raytracing -rtnoao -rtshafts -rtshaftwhite -rtshaftdust -rtglow -rtnocubemaps -rtnoreflections -rtnotonemap \
                 -rtcolorcorrection -rtaostrength 0.75 -rtaoradius 64 -rtshaftintensity 2 -rtshaftlength 1.5 \
                 -rtshaftsoftness 0.5 -rtglowintensity 3 -rtshaftsource sun"
            )
        );
        for a in o.args().iter().filter(|a| a.starts_with('-')) {
            assert!(is_raytracing_flag(a), "{a}");
        }
        let old: RayTracingOptions = serde_json::from_str(r#"{"quality":"final"}"#).expect("old");
        assert_eq!(old.effects, RayEffectsOptions::default());
        o.effects = RayEffectsOptions { reflections: "On".into(), ..RayEffectsOptions::default() };
        assert_eq!(o.args(), words("-raytracing -rtreflections"));
        o.effects.reflections = "auto".into();
        assert_eq!(o.args(), [RAYTRACING_FLAG]);
        let saved: RayEffectsOptions = serde_json::from_str(r#"{"shafts":true}"#).expect("saved");
        assert!(!saved.shafts, "volumetric beams saved on by older versions start off");
        let on: RayEffectsOptions = serde_json::from_str(r#"{"light_shafts":true}"#).expect("on");
        assert!(on.shafts);
    }

    #[test]
    fn the_preview_gpu_share_is_written_only_when_set() {
        assert!(gpu_share_args(None).is_empty());
        assert!(gpu_share_args(Some(0.0)).is_empty());
        assert_eq!(gpu_share_args(Some(0.35)), words("-rtgpushare 0.35"));
        assert_eq!(gpu_share_args(Some(3.0)), words("-rtgpushare 1"));
        assert!(!RayTracingOptions::default().args().contains(&GPU_SHARE_FLAG.to_owned()));
    }

    #[test]
    fn the_language_is_added_once_when_ray_tracing_is_on() {
        let mut plain = words("--rad -ldr map");
        add_lang(&mut plain, "fr");
        assert_eq!(plain, words("--rad -ldr map"));
        let mut on = words("--rad -RayTracing map");
        add_lang(&mut on, "fr");
        assert_eq!(on, words("--rad -rtlang fr -RayTracing map"));
        add_lang(&mut on, "ru");
        assert_eq!(on, words("--rad -rtlang fr -RayTracing map"));
        let mut only = words("--rad -rtbounces 3 map");
        add_lang(&mut only, "en");
        assert!(only.contains(&"-rtlang".to_owned()));
        let mut none = Vec::new();
        add_lang(&mut none, "en");
        assert!(none.is_empty());
    }

    #[test]
    fn only_the_lighting_step_of_the_own_compiler_gets_the_flags() {
        use crate::{CompilePlan, PlanOptions, Preset, StepKind, ToolPaths};
        let tools = ToolPaths {
            compiler: "hammerpro.exe".into(),
            game_exe: None,
            game_dir: "garrysmod".into(),
            maps_dir: "garrysmod/maps".into(),
        };
        let vmf = std::env::temp_dir().join("gm_rt_plan.vmf");
        let args = |opts: &PlanOptions, kind: StepKind| {
            let plan = CompilePlan::preset(Preset::Normal, &tools, &vmf, opts).expect("plan");
            plan.steps.iter().find(|s| s.kind == kind).map(|s| s.args.clone()).unwrap_or_default()
        };
        let plain = PlanOptions::default();
        assert!(!args(&plain, StepKind::Rad).iter().any(|a| is_raytracing_flag(a)), "off by default");
        let rt =
            RayTracingOptions { quality: "draft".into(), bounces: Some(2), lang: "fr".into(), ..Default::default() };
        let on = PlanOptions { raytracing: Some(rt), ..PlanOptions::default() };
        let rad = args(&on, StepKind::Rad);
        assert_eq!(rad.first().map(String::as_str), Some(crate::RAD_ARG));
        for w in words("-raytracing -rtquality draft -rtbounces 2 -rtlang fr") {
            assert!(rad.contains(&w), "{w} in {rad:?}");
        }
        assert!(!args(&on, StepKind::Bsp).iter().any(|a| is_raytracing_flag(a)));
        let bsp = args(&on, StepKind::Bsp);
        assert!(bsp.contains(&BSP_AUTO_CUBEMAPS.to_owned()), "automatic cubemaps by default: {bsp:?}");
        assert!(!args(&plain, StepKind::Bsp).contains(&BSP_AUTO_CUBEMAPS.to_owned()));
        let mut no_cube = on.clone();
        if let Some(rt) = no_cube.raytracing.as_mut() {
            rt.effects.auto_cubemaps = false;
        }
        assert!(!args(&no_cube, StepKind::Bsp).contains(&BSP_AUTO_CUBEMAPS.to_owned()));
        let (flags, _) = crate::rad_step_flags(crate::RadMode::Final, &on);
        assert!(flags.contains(&"-final".to_owned()) && flags.contains(&RAYTRACING_FLAG.to_owned()));
    }
}
