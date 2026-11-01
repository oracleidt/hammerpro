use serde::{Deserialize, Serialize};

#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum ValveTool {
    Vbsp,
    Vvis,
    Vrad,
}

impl ValveTool {
    pub const ALL: [ValveTool; 3] = [ValveTool::Vbsp, ValveTool::Vvis, ValveTool::Vrad];

    pub fn key(self) -> &'static str {
        match self {
            ValveTool::Vbsp => "vbsp",
            ValveTool::Vvis => "vvis",
            ValveTool::Vrad => "vrad",
        }
    }
}

#[derive(Clone, Copy, Debug, PartialEq)]
pub enum ValveValue {
    Switch,
    Int { min: i64, max: i64 },
    Float { min: f64, max: f64 },
    Text,
    File,
    Folder,
    Numbers(usize),
}

impl ValveValue {
    pub fn count(self) -> usize {
        match self {
            ValveValue::Switch => 0,
            ValveValue::Numbers(n) => n,
            _ => 1,
        }
    }

    pub fn key(self) -> &'static str {
        match self {
            ValveValue::Switch => "switch",
            ValveValue::Int { .. } => "integer",
            ValveValue::Float { .. } => "number",
            ValveValue::Text => "text",
            ValveValue::File => "file",
            ValveValue::Folder => "folder",
            ValveValue::Numbers(_) => "numbers",
        }
    }
}

#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash)]
pub enum ValveGroup {
    Quality,
    Speed,
    Geometry,
    Visibility,
    Lightmaps,
    StaticProps,
    Content,
    Debug,
    OtherBranches,
}

impl ValveGroup {
    pub const ALL: [ValveGroup; 9] = [
        ValveGroup::Quality,
        ValveGroup::Speed,
        ValveGroup::Geometry,
        ValveGroup::Visibility,
        ValveGroup::Lightmaps,
        ValveGroup::StaticProps,
        ValveGroup::Content,
        ValveGroup::Debug,
        ValveGroup::OtherBranches,
    ];

    pub fn key(self) -> &'static str {
        match self {
            ValveGroup::Quality => "quality",
            ValveGroup::Speed => "speed",
            ValveGroup::Geometry => "geometry",
            ValveGroup::Visibility => "visibility",
            ValveGroup::Lightmaps => "lightmaps",
            ValveGroup::StaticProps => "static_props",
            ValveGroup::Content => "content",
            ValveGroup::Debug => "debug",
            ValveGroup::OtherBranches => "other_branches",
        }
    }
}

#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash)]
pub enum ValveManaged {
    BspMode,
    VisMode,
    RadQuality,
    Lighting,
    Threads,
}

impl ValveManaged {
    pub fn key(self) -> &'static str {
        match self {
            ValveManaged::BspMode => "bsp_mode",
            ValveManaged::VisMode => "vis_mode",
            ValveManaged::RadQuality => "rad_quality",
            ValveManaged::Lighting => "lighting",
            ValveManaged::Threads => "threads",
        }
    }
}

#[derive(Clone, Copy, Debug, PartialEq)]
pub struct ValveFlag {
    pub tool: ValveTool,
    pub flag: &'static str,
    pub value: ValveValue,
    pub group: ValveGroup,
    pub default: &'static str,
    pub managed: Option<ValveManaged>,
}

impl ValveFlag {
    pub fn key(&self) -> String {
        let name: String =
            self.flag.trim_start_matches('-').chars().map(|c| c.to_ascii_lowercase()).filter(|c| *c != '_').collect();
        format!("{}_{name}", self.tool.key())
    }

    pub fn name(&self) -> &'static str {
        self.flag.trim_start_matches('-')
    }
}

const fn f(tool: ValveTool, flag: &'static str, value: ValveValue, group: ValveGroup) -> ValveFlag {
    ValveFlag { tool, flag, value, group, default: "", managed: None }
}

const fn d(
    tool: ValveTool,
    flag: &'static str,
    value: ValveValue,
    group: ValveGroup,
    default: &'static str,
) -> ValveFlag {
    ValveFlag { tool, flag, value, group, default, managed: None }
}

const fn m(tool: ValveTool, flag: &'static str, value: ValveValue, group: ValveGroup, by: ValveManaged) -> ValveFlag {
    ValveFlag { tool, flag, value, group, default: "", managed: Some(by) }
}

use ValveGroup as G;
use ValveTool::{Vbsp as B, Vrad as R, Vvis as V};
use ValveValue::Switch as S;

const THREADS: ValveValue = ValveValue::Int { min: 1, max: 256 };

pub const VALVE_FLAGS: &[ValveFlag] = &[
    m(B, "-onlyents", S, G::Quality, ValveManaged::BspMode),
    m(B, "-onlyprops", S, G::Quality, ValveManaged::BspMode),
    m(B, "-threads", THREADS, G::Speed, ValveManaged::Threads),
    f(B, "-low", S, G::Speed),
    f(B, "-nodetail", S, G::Geometry),
    f(B, "-fulldetail", S, G::Geometry),
    f(B, "-nowater", S, G::Geometry),
    f(B, "-noweld", S, G::Geometry),
    f(B, "-nocsg", S, G::Geometry),
    f(B, "-noshare", S, G::Geometry),
    f(B, "-notjunc", S, G::Geometry),
    f(B, "-allowdetailcracks", S, G::Geometry),
    f(B, "-noopt", S, G::Geometry),
    f(B, "-noprune", S, G::Geometry),
    f(B, "-nomerge", S, G::Geometry),
    f(B, "-nomergewater", S, G::Geometry),
    f(B, "-nosubdiv", S, G::Geometry),
    f(B, "-snapaxial", S, G::Geometry),
    d(B, "-micro", ValveValue::Float { min: 0.0, max: 1_000_000.0 }, G::Geometry, "1"),
    d(B, "-block", ValveValue::Numbers(2), G::Geometry, "0 0"),
    d(B, "-blocks", ValveValue::Numbers(4), G::Geometry, "0 0 0 0"),
    f(B, "-leaktest", S, G::Visibility),
    f(B, "-forceskyvis", S, G::Visibility),
    d(B, "-luxelscale", ValveValue::Float { min: 0.01, max: 1000.0 }, G::Lightmaps, "1"),
    d(B, "-minluxelscale", ValveValue::Float { min: 0.01, max: 1000.0 }, G::Lightmaps, "1"),
    f(B, "-lightifmissing", S, G::Lightmaps),
    f(B, "-bumpall", S, G::Lightmaps),
    f(B, "-allowdynamicpropsasstatic", S, G::StaticProps),
    f(B, "-virtualdispphysics", S, G::StaticProps),
    f(B, "-dumpstaticprops", S, G::StaticProps),
    f(B, "-embed", ValveValue::Folder, G::Content),
    f(B, "-keepstalezip", S, G::Content),
    f(B, "-replacematerials", S, G::Content),
    f(B, "-verbose", S, G::Debug),
    f(B, "-verboseentities", S, G::Debug),
    f(B, "-glview", S, G::Debug),
    f(B, "-dumpcollide", S, G::Debug),
    f(B, "-novconfig", S, G::Debug),
    f(B, "-FullMinidumps", S, G::Debug),
    f(B, "-StaticPropCombine", S, G::OtherBranches),
    f(B, "-KeepSources", S, G::OtherBranches),
    m(V, "-fast", S, G::Quality, ValveManaged::VisMode),
    d(V, "-radius_override", ValveValue::Float { min: 0.0, max: 1_000_000.0 }, G::Quality, "4096"),
    m(V, "-threads", THREADS, G::Speed, ValveManaged::Threads),
    f(V, "-low", S, G::Speed),
    f(V, "-mpi", S, G::Speed),
    f(V, "-mpi_pw", ValveValue::Text, G::Speed),
    f(V, "-nosort", S, G::Visibility),
    f(V, "-tmpin", S, G::Visibility),
    f(V, "-tmpout", S, G::Visibility),
    d(V, "-trace", ValveValue::Numbers(2), G::Debug, "0 1"),
    f(V, "-verbose", S, G::Debug),
    f(V, "-novconfig", S, G::Debug),
    f(V, "-FullMinidumps", S, G::Debug),
    m(R, "-fast", S, G::Quality, ValveManaged::RadQuality),
    m(R, "-final", S, G::Quality, ValveManaged::RadQuality),
    m(R, "-ldr", S, G::Quality, ValveManaged::Lighting),
    m(R, "-hdr", S, G::Quality, ValveManaged::Lighting),
    m(R, "-both", S, G::Quality, ValveManaged::Lighting),
    d(R, "-extrasky", ValveValue::Int { min: 1, max: 1024 }, G::Quality, "16"),
    d(R, "-bounce", ValveValue::Int { min: 0, max: 1000 }, G::Quality, "100"),
    f(R, "-noextra", S, G::Quality),
    f(R, "-fastambient", S, G::Quality),
    d(R, "-softsun", ValveValue::Float { min: 0.0, max: 90.0 }, G::Quality, "0"),
    m(R, "-threads", THREADS, G::Speed, ValveManaged::Threads),
    f(R, "-low", S, G::Speed),
    f(R, "-mpi", S, G::Speed),
    f(R, "-mpi_pw", ValveValue::Text, G::Speed),
    d(R, "-smooth", ValveValue::Float { min: 0.0, max: 180.0 }, G::Lightmaps, "45"),
    d(R, "-luxeldensity", ValveValue::Float { min: 0.0, max: 1.0 }, G::Lightmaps, "1"),
    d(R, "-chop", ValveValue::Float { min: 0.1, max: 1024.0 }, G::Lightmaps, "4"),
    d(R, "-maxchop", ValveValue::Float { min: 0.1, max: 1024.0 }, G::Lightmaps, "4"),
    d(R, "-dispchop", ValveValue::Float { min: 0.1, max: 1024.0 }, G::Lightmaps, "8"),
    d(R, "-maxdispsamplesize", ValveValue::Int { min: 1, max: 65536 }, G::Lightmaps, "512"),
    f(R, "-LargeDispSampleRadius", S, G::Lightmaps),
    f(R, "-centersamples", S, G::Lightmaps),
    f(R, "-dlightmap", S, G::Lightmaps),
    f(R, "-noskyboxrecurse", S, G::Lightmaps),
    f(R, "-StaticPropLighting", S, G::StaticProps),
    f(R, "-StaticPropPolys", S, G::StaticProps),
    f(R, "-TextureShadows", S, G::StaticProps),
    f(R, "-nossprops", S, G::StaticProps),
    f(R, "-nodetaillight", S, G::StaticProps),
    f(R, "-onlydetail", S, G::StaticProps),
    f(R, "-lights", ValveValue::File, G::Content),
    f(R, "-verbose", S, G::Debug),
    f(R, "-dump", S, G::Debug),
    f(R, "-dumpnormals", S, G::Debug),
    f(R, "-dumptrace", S, G::Debug),
    f(R, "-debugextra", S, G::Debug),
    f(R, "-loghash", S, G::Debug),
    f(R, "-rederror", S, G::Debug),
    f(R, "-OnlyStaticProps", S, G::Debug),
    f(R, "-StaticPropNormals", S, G::Debug),
    f(R, "-stoponexit", S, G::Debug),
    f(R, "-novconfig", S, G::Debug),
    f(R, "-FullMinidumps", S, G::Debug),
];

pub fn valve_flags(tool: ValveTool) -> impl Iterator<Item = &'static ValveFlag> {
    VALVE_FLAGS.iter().filter(move |f| f.tool == tool)
}

pub fn valve_flag(tool: ValveTool, flag: &str) -> Option<&'static ValveFlag> {
    let wanted = flag.trim().trim_start_matches('-');
    valve_flags(tool).find(|f| f.name().eq_ignore_ascii_case(wanted))
}

pub fn check_value(flag: &ValveFlag, value: &str) -> Result<Vec<String>, String> {
    if matches!(flag.value, ValveValue::Text | ValveValue::File | ValveValue::Folder) {
        let one = value.trim().trim_matches('"').trim();
        if one.is_empty() {
            return Err(format!("{} takes one value", flag.flag));
        }
        return Ok(vec![one.to_owned()]);
    }
    let words: Vec<String> = crate::plan::split_args(value);
    let want = flag.value.count();
    if words.len() != want {
        return Err(match want {
            0 => format!("{} takes no value", flag.flag),
            1 => format!("{} takes one value", flag.flag),
            n => format!("{} takes {n} numbers", flag.flag),
        });
    }
    let number = |w: &str| w.parse::<f64>().ok().filter(|x| x.is_finite());
    match flag.value {
        ValveValue::Int { min, max } => {
            let n = words[0].parse::<i64>().map_err(|_| format!("{} takes a whole number", flag.flag))?;
            if !(min..=max).contains(&n) {
                return Err(format!("{} takes a number from {min} to {max}", flag.flag));
            }
        }
        ValveValue::Float { min, max } => {
            let x = number(&words[0]).ok_or_else(|| format!("{} takes a number", flag.flag))?;
            if !(min..=max).contains(&x) {
                return Err(format!("{} takes a number from {min} to {max}", flag.flag));
            }
        }
        ValveValue::Numbers(_) => {
            if words.iter().any(|w| number(w).is_none()) {
                return Err(format!("{} takes numbers", flag.flag));
            }
        }
        ValveValue::Switch | ValveValue::Text | ValveValue::File | ValveValue::Folder => {}
    }
    Ok(words)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn the_catalog_is_well_formed() {
        let mut keys = std::collections::HashSet::new();
        for f in VALVE_FLAGS {
            assert!(f.flag.starts_with('-') && !f.flag.contains(' '), "{}", f.flag);
            assert!(keys.insert(f.key()), "duplicate {}", f.key());
            assert!(f.default.is_empty() || check_value(f, f.default).is_ok(), "{}", f.flag);
            if f.managed.is_some() {
                assert!(matches!(f.group, ValveGroup::Quality | ValveGroup::Speed), "{}", f.flag);
            }
        }
        for tool in ValveTool::ALL {
            assert!(valve_flags(tool).count() > 10, "{tool:?}");
            assert!(valve_flag(tool, "threads").is_some_and(|f| f.managed == Some(ValveManaged::Threads)));
        }
        assert_eq!(valve_flag(ValveTool::Vrad, "staticproplighting").map(|f| f.flag), Some("-StaticPropLighting"));
        assert_eq!(
            valve_flag(ValveTool::Vrad, "-StaticPropLighting").map(ValveFlag::key).as_deref(),
            Some("vrad_staticproplighting")
        );
        assert!(valve_flag(ValveTool::Vvis, "-final").is_none());
    }

    #[test]
    fn values_are_checked() {
        let bounce = valve_flag(ValveTool::Vrad, "-bounce").expect("bounce");
        assert_eq!(check_value(bounce, "8").expect("ok"), ["8"]);
        assert!(check_value(bounce, "").is_err());
        assert!(check_value(bounce, "2000").is_err());
        assert!(check_value(bounce, "x").is_err());
        let trace = valve_flag(ValveTool::Vvis, "-trace").expect("trace");
        assert_eq!(check_value(trace, "3 7").expect("ok"), ["3", "7"]);
        assert!(check_value(trace, "3").is_err());
        let lights = valve_flag(ValveTool::Vrad, "-lights").expect("lights");
        assert_eq!(check_value(lights, "\"C:/my maps/a.rad\"").expect("ok"), ["C:/my maps/a.rad"]);
        assert_eq!(check_value(lights, " C:/my maps/a.rad ").expect("ok"), ["C:/my maps/a.rad"]);
        assert!(check_value(lights, "  ").is_err());
        let low = valve_flag(ValveTool::Vbsp, "-low").expect("low");
        assert!(check_value(low, "").is_ok() && check_value(low, "1").is_err());
        let soft = valve_flag(ValveTool::Vrad, "-softsun").expect("softsun");
        assert!(check_value(soft, "2.5").is_ok() && check_value(soft, "NaN").is_err());
    }
}
