use crate::plan::{BspMode, Lighting, RadMode, StepKind, VisMode};

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct FlagInfo {
    pub step: StepKind,
    pub flag: &'static str,
    pub value: bool,
    pub english: &'static str,
}

impl FlagInfo {
    pub fn value_count(&self) -> usize {
        if self.flag.eq_ignore_ascii_case("-trace") { 2 } else { usize::from(self.value) }
    }

    pub fn group(&self) -> FlagGroup {
        flag_group(self.step, self.flag)
    }

    pub fn managed_by(&self) -> Option<ManagedBy> {
        managed_by(self.step, self.flag)
    }

    pub fn key(&self) -> String {
        self.flag.trim_start_matches('-').to_ascii_lowercase()
    }
}

const fn info(step: StepKind, flag: &'static str, value: bool, english: &'static str) -> FlagInfo {
    FlagInfo { step, flag, value, english }
}

pub const FLAGS: &[FlagInfo] = &[
    info(StepKind::Bsp, "-vis", false, "Compute the full visibility: the map renders fast in game."),
    info(StepKind::Bsp, "-fastvis", false, "Quick coarse visibility: much faster to compile, slower to render."),
    info(
        StepKind::Bsp,
        "-leaktest",
        false,
        "Stop when the map leaks: no BSP is written and no time is wasted on the lighting.",
    ),
    info(
        StepKind::Bsp,
        "-onlyents",
        false,
        "Only update the entities of the previous BSP (seconds): geometry, visibility and lighting are kept.",
    ),
    info(
        StepKind::Bsp,
        "-onlyprops",
        false,
        "Only update the static props and detail props of the previous BSP: geometry, visibility and lighting are kept.",
    ),
    info(
        StepKind::Bsp,
        "-game",
        true,
        "Game folder whose materials, models and lights.rad (texture lights) the compiler reads.",
    ),
    info(StepKind::Bsp, "-gmod", true, "Garry's Mod folder whose content the compiler mounts."),
    info(StepKind::Bsp, "-threads", true, "Number of processor threads to use."),
    info(
        StepKind::Bsp,
        "-nodetail",
        false,
        "Leave out every detail brush (func_detail): what is left is what cuts the visibility.",
    ),
    info(StepKind::Bsp, "-nowater", false, "Leave out the water brushes."),
    info(
        StepKind::Bsp,
        "-fulldetail",
        false,
        "Detail brushes become normal brushes: they cut the visibility too (slower vis).",
    ),
    info(
        StepKind::Bsp,
        "-forceskyvis",
        false,
        "Compute the visibility inside the 3D skybox too (it is one cluster by default).",
    ),
    info(StepKind::Bsp, "-micro", true, "Brushes smaller than this volume get a microbrush warning (default 1)."),
    info(
        StepKind::Bsp,
        "-luxelscale",
        true,
        "Scale every lightmap scale by this factor (2 halves the lightmap resolution).",
    ),
    info(StepKind::Bsp, "-minluxelscale", true, "No lightmap scale below this value (default 1)."),
    info(
        StepKind::Bsp,
        "-defaultluxelsize",
        true,
        "Lightmap scale of the faces whose lightmap scale is 0 (default 16).",
    ),
    info(
        StepKind::Bsp,
        "-maxlightmapdim",
        true,
        "Largest lightmap of a face in luxels (default 32, at most 125): bigger faces are cut less.",
    ),
    info(StepKind::Bsp, "-bumpall", false, "Every lit surface gets bumped lightmaps (bigger lighting data)."),
    info(StepKind::Bsp, "-lightifmissing", false, "Surfaces with missing or unlit materials get a lightmap anyway."),
    info(StepKind::Bsp, "-nodrawtriggers", false, "Trigger textures are not drawn at all (smaller map)."),
    info(
        StepKind::Bsp,
        "-AllowDynamicPropsAsStatic",
        false,
        "Keep prop_static props whose model has physics properties (prop_data) instead of deleting them.",
    ),
    info(StepKind::Bsp, "-replacematerials", false, "Replace materials as listed in cfg/materialsub.cfg of the game."),
    info(StepKind::Bsp, "-keepstalezip", false, "Keep the files packed in the previous BSP (the rest is rebuilt)."),
    info(StepKind::Bsp, "-embed", true, "Folder added to the content and packed whole into the BSP."),
    info(StepKind::Bsp, "-low", false, "Run at idle priority: the computer stays responsive."),
    info(StepKind::Bsp, "-nomerge", false, "Debug: do not merge the faces cut by the BSP back together."),
    info(StepKind::Bsp, "-nosubdiv", false, "Debug: do not cut the faces for their lightmaps (large faces fail)."),
    info(StepKind::Bsp, "-notjunc", false, "Debug: do not fix the T-junctions (small cracks may show)."),
    info(StepKind::Bsp, "-noprune", false, "Debug: do not merge the solid nodes of the BSP tree."),
    info(
        StepKind::Bsp,
        "-novirtualmesh",
        false,
        "Displacement collision as triangle soups instead of the virtual terrain mesh.",
    ),
    info(
        StepKind::Bsp,
        "-blocksize",
        true,
        "Size of the blocks the world is cut in before its brushes split it (default 1024; 0 = no blocks, else 128 to 32768): bigger blocks or 0 cut fewer faces and leaves (fewer planes and vertices).",
    ),
    info(
        StepKind::Bsp,
        "-propdebug",
        false,
        "List in the compile log every prop whose model misbehaves (missing or broken model, no collision model, bad skin or body group, outside the world, not a static prop, too many vertices for the prop lighting), with its entity, model and position.",
    ),
    info(StepKind::Bsp, "-noaddons", false, "Do not mount the folder addons of garrysmod/addons."),
    info(StepKind::Bsp, "-wsaddons", false, "Also mount the subscribed workshop items (Valve's VBSP does not)."),
    info(StepKind::Bsp, "-nowsaddons", false, "Mount neither the folder addons nor the workshop items."),
    info(
        StepKind::Bsp,
        "-serveraddons",
        false,
        "Also mount the content downloaded from servers (garrysmod/download and garrysmod/cache/workshop; Valve's VBSP does not).",
    ),
    info(StepKind::Bsp, "-dxlevel", true, "Drop the entities whose mindxlevel/maxdxlevel exclude this DirectX level."),
    info(
        StepKind::Bsp,
        "-radius_override",
        true,
        "Visibility radius: clusters farther apart never see each other, whatever the fog.",
    ),
    info(StepKind::Bsp, "-nosort", false, "Visibility without sorting the portals first (same result)."),
    info(StepKind::Bsp, "-trace", true, "Write the line of sight between two clusters (cluster numbers) to <map>.lin."),
    info(StepKind::Bsp, "-verbose", false, "Print the options of the compile."),
    info(StepKind::Rad, "-ldr", false, "Standard dynamic range lighting only (fastest, what most players see)."),
    info(StepKind::Rad, "-hdr", false, "High dynamic range lighting only."),
    info(StepKind::Rad, "-both", false, "Standard and high dynamic range lighting (two passes, needed for release)."),
    info(StepKind::Rad, "-bounce", true, "Maximum number of light bounces: 1 is much faster, the default is 100."),
    info(StepKind::Rad, "-noextra", false, "No supersampling: harder, blockier shadow edges, faster."),
    info(
        StepKind::Rad,
        "-fast",
        false,
        "Quick preview: few rays, no supersampling, one ambient sample per leaf, much faster.",
    ),
    info(StepKind::Rad, "-final", false, "High quality sky and indirect lighting (more rays): slow."),
    info(
        StepKind::Rad,
        "-StaticPropLighting",
        false,
        "Bake the lighting of every static prop vertex (shadows and bounced light) instead of letting the game light props at run time.",
    ),
    info(
        StepKind::Rad,
        "-StaticPropPolys",
        false,
        "Static props cast shadows with the triangles they draw instead of their collision model (finer shadows, slower).",
    ),
    info(
        StepKind::Rad,
        "-textureshadows",
        false,
        "With -StaticPropPolys: alpha tested prop textures (leaves, fences) of models compiled with $casttextureshadows let light through.",
    ),
    info(StepKind::Rad, "-nossprops", false, "Static props never shadow themselves."),
    info(
        StepKind::Rad,
        "-nostaticpropshadows",
        false,
        "Hammer Pro option: static props cast no shadow at all (lightmaps, bounced light, detail props, ambient light, prop lighting); their shadow models are not even loaded, much faster on maps with many props.",
    ),
    info(
        StepKind::Rad,
        "-nodispshadows",
        false,
        "Hammer Pro option: displacements cast no shadow at all (they are still lit). One displacement can also be kept from casting shadows with the Cast shadows option of the displacement tool.",
    ),
    info(
        StepKind::Rad,
        "-nostylebounce",
        false,
        "Hammer Pro option: named (switchable) lights light only what they see directly, like Valve's VRAD. Without it their light also bounces, in their own light style, and switches with them.",
    ),
    info(StepKind::Rad, "-threads", true, "Number of processor threads to use."),
    info(
        StepKind::Rad,
        "-game",
        true,
        "Game folder whose materials, models and lights.rad (texture lights) the compiler reads.",
    ),
    info(StepKind::Rad, "-gmod", true, "Garry's Mod folder whose content the compiler mounts."),
    info(
        StepKind::Rad,
        "-lights",
        true,
        "Extra texture light file, read after lights.rad and before the map's own .rad.",
    ),
    info(StepKind::Rad, "-chop", true, "Smallest texture light patch in luxels (default 4)."),
    info(StepKind::Rad, "-maxchop", true, "Largest texture light patch in luxels (default 4)."),
    info(StepKind::Rad, "-dispchop", true, "Texture light patch size on displacements in luxels (default 8)."),
    info(StepKind::Rad, "-low", false, "Run at idle priority: the computer stays responsive."),
    info(
        StepKind::Rad,
        "-smooth",
        true,
        "Faces meeting at less than this angle in degrees get smooth shading across their edge (default 45).",
    ),
    info(
        StepKind::Rad,
        "-softsun",
        true,
        "Treat the sun as an area light this many degrees wide: softer shadow edges (0 to 5 is usual, default 0).",
    ),
    info(
        StepKind::Rad,
        "-extrasky",
        true,
        "Trace this many times more sky and indirect light rays (smoother ambient light, slower).",
    ),
    info(
        StepKind::Rad,
        "-luxeldensity",
        true,
        "Scale every lightmap resolution by this factor below 1 (coarser and faster lighting).",
    ),
    info(
        StepKind::Rad,
        "-staticpropsamplescale",
        true,
        "Scale the number of rays that light static prop vertices (with -StaticPropLighting).",
    ),
    info(
        StepKind::Rad,
        "-noskyboxrecurse",
        false,
        "The 3D skybox casts no shadow on the map and gets no light from it.",
    ),
    info(StepKind::Rad, "-fastambient", false, "Quicker, coarser ambient light of the props and dynamic objects."),
    info(
        StepKind::Rad,
        "-centersamples",
        false,
        "Accepted like VRAD: luxels are always sampled at their middle when it is open.",
    ),
    info(
        StepKind::Rad,
        "-StaticPropNormals",
        false,
        "Debug: static props show their vertex normals instead of their lighting.",
    ),
];

pub fn flag_info(step: StepKind, flag: &str) -> Option<&'static FlagInfo> {
    FLAGS.iter().find(|f| f.step == step && f.flag.eq_ignore_ascii_case(flag))
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, PartialOrd, Ord)]
pub enum FlagGroup {
    Managed,
    Quality,
    Speed,
    Geometry,
    Visibility,
    Lightmaps,
    Props,
    Lights,
    Content,
    Debug,
}

impl FlagGroup {
    pub const ALL: [FlagGroup; 10] = [
        FlagGroup::Quality,
        FlagGroup::Speed,
        FlagGroup::Geometry,
        FlagGroup::Visibility,
        FlagGroup::Lightmaps,
        FlagGroup::Props,
        FlagGroup::Lights,
        FlagGroup::Content,
        FlagGroup::Debug,
        FlagGroup::Managed,
    ];

    pub fn key(self) -> &'static str {
        match self {
            FlagGroup::Managed => "managed",
            FlagGroup::Quality => "quality",
            FlagGroup::Speed => "speed",
            FlagGroup::Geometry => "geometry",
            FlagGroup::Visibility => "visibility",
            FlagGroup::Lightmaps => "lightmaps",
            FlagGroup::Props => "props",
            FlagGroup::Lights => "lights",
            FlagGroup::Content => "content",
            FlagGroup::Debug => "debug",
        }
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub enum ManagedBy {
    VisMode,
    BspMode,
    StopOnLeak,
    Threads,
    Lighting,
    Game,
    StaticPropShadows,
}

impl ManagedBy {
    pub fn option(self) -> &'static str {
        match self {
            ManagedBy::VisMode => "vis_mode",
            ManagedBy::BspMode => "bsp_mode",
            ManagedBy::StopOnLeak => "stop_on_leak",
            ManagedBy::Threads => "threads",
            ManagedBy::Lighting => "lighting",
            ManagedBy::Game => "the game configuration (Settings > Game)",
            ManagedBy::StaticPropShadows => "static_prop_shadows",
        }
    }

    pub fn key(self) -> &'static str {
        match self {
            ManagedBy::VisMode => "vis_mode",
            ManagedBy::BspMode => "bsp_mode",
            ManagedBy::StopOnLeak => "stop_on_leak",
            ManagedBy::Threads => "threads",
            ManagedBy::Lighting => "lighting",
            ManagedBy::Game => "game",
            ManagedBy::StaticPropShadows => "static_prop_shadows",
        }
    }
}

pub fn managed_by(step: StepKind, flag: &str) -> Option<ManagedBy> {
    let f = flag.to_ascii_lowercase();
    let both = match f.as_str() {
        "-threads" => Some(ManagedBy::Threads),
        "-game" | "-gmod" => Some(ManagedBy::Game),
        _ => None,
    };
    both.or(match (step, f.as_str()) {
        (StepKind::Bsp, "-vis" | "-fastvis") => Some(ManagedBy::VisMode),
        (StepKind::Bsp, "-onlyents" | "-onlyprops") => Some(ManagedBy::BspMode),
        (StepKind::Bsp, "-leaktest") => Some(ManagedBy::StopOnLeak),
        (StepKind::Rad, "-ldr" | "-hdr" | "-both") => Some(ManagedBy::Lighting),
        (StepKind::Rad, "-nostaticpropshadows") => Some(ManagedBy::StaticPropShadows),
        _ => None,
    })
}

pub fn flag_group(step: StepKind, flag: &str) -> FlagGroup {
    if managed_by(step, flag).is_some() {
        return FlagGroup::Managed;
    }
    match (step, flag.to_ascii_lowercase().as_str()) {
        (_, "-low") => FlagGroup::Speed,
        (
            StepKind::Bsp,
            "-nodetail" | "-nowater" | "-micro" | "-nodrawtriggers" | "-novirtualmesh" | "-replacematerials",
        )
        | (StepKind::Bsp, "-allowdynamicpropsasstatic" | "-dxlevel" | "-blocksize") => FlagGroup::Geometry,
        (StepKind::Bsp, "-fulldetail" | "-forceskyvis" | "-radius_override" | "-nosort") => FlagGroup::Visibility,
        (StepKind::Bsp, "-luxelscale" | "-minluxelscale" | "-defaultluxelsize" | "-maxlightmapdim" | "-bumpall")
        | (StepKind::Bsp, "-lightifmissing") => FlagGroup::Lightmaps,
        (StepKind::Bsp, "-keepstalezip" | "-embed" | "-noaddons" | "-wsaddons" | "-nowsaddons" | "-serveraddons") => {
            FlagGroup::Content
        }
        (
            StepKind::Rad,
            "-final" | "-bounce" | "-noextra" | "-extrasky" | "-smooth" | "-softsun" | "-centersamples",
        )
        | (StepKind::Rad, "-nodispshadows" | "-nostylebounce" | "-ao" | "-noao" | "-aoradius" | "-aostrength")
        | (StepKind::Rad, "-aosamples") => FlagGroup::Quality,
        (StepKind::Rad, "-fast" | "-fastambient" | "-luxeldensity") => FlagGroup::Speed,
        (StepKind::Rad, "-staticproplighting" | "-staticproppolys" | "-textureshadows" | "-nossprops")
        | (StepKind::Rad, "-staticpropsamplescale") => FlagGroup::Props,
        (StepKind::Rad, "-lights" | "-chop" | "-maxchop" | "-dispchop" | "-noskyboxrecurse") => FlagGroup::Lights,
        _ => FlagGroup::Debug,
    }
}

pub fn menu_flags(step: StepKind) -> Vec<(FlagGroup, Vec<&'static FlagInfo>)> {
    FlagGroup::ALL
        .iter()
        .map(|g| (*g, FLAGS.iter().filter(|f| f.step == step && f.group() == *g).collect::<Vec<_>>()))
        .filter(|(_, list)| !list.is_empty())
        .collect()
}

pub fn arg_groups(tokens: &[String], step: StepKind) -> Vec<Vec<String>> {
    let mut out = Vec::new();
    let mut i = 0;
    while i < tokens.len() {
        let n = flag_info(step, &tokens[i]).map_or(0, FlagInfo::value_count);
        let end = (i + 1 + n).min(tokens.len());
        out.push(tokens[i..end].to_vec());
        i = end;
    }
    out
}

#[derive(Debug, Clone, PartialEq, Eq, Hash, Default, serde::Serialize, serde::Deserialize)]
#[serde(default)]
pub struct FlagChoice {
    pub flag: String,
    pub on: bool,
    pub value: String,
}

impl FlagChoice {
    pub fn args(&self, step: StepKind) -> Option<Vec<String>> {
        let info = flag_info(step, self.flag.trim())?;
        if !self.on || info.managed_by().is_some() {
            return None;
        }
        let mut out = vec![info.flag.to_owned()];
        match info.value_count() {
            0 => {}
            1 => {
                let v = self.value.trim();
                if v.is_empty() {
                    return None;
                }
                out.push(v.to_owned());
            }
            n => {
                let words: Vec<&str> = self.value.split_whitespace().collect();
                if words.len() != n {
                    return None;
                }
                out.extend(words.iter().map(|w| (*w).to_owned()));
            }
        }
        Some(out)
    }
}

pub fn merge_flags(
    step: StepKind,
    preset: &[&str],
    choices: &[FlagChoice],
    extra: &[String],
) -> (Vec<String>, Vec<String>) {
    let preset: Vec<String> = preset.iter().map(|s| (*s).to_owned()).collect();
    let mut flags = arg_groups(&preset, step);
    let mut extra = arg_groups(extra, step);
    let same = |g: &[String], flag: &str| g.first().is_some_and(|a| a.eq_ignore_ascii_case(flag));
    for g in &extra {
        if let Some(info) = g.first().and_then(|a| flag_info(step, a)) {
            flags.retain(|p| !same(p, info.flag));
        }
    }
    for c in choices {
        let Some(info) = flag_info(step, c.flag.trim()) else { continue };
        if info.managed_by().is_some() {
            continue;
        }
        if c.on {
            let Some(args) = c.args(step) else { continue };
            flags.retain(|p| !same(p, info.flag));
            extra.retain(|p| !same(p, info.flag));
            flags.push(args);
        } else {
            flags.retain(|p| !same(p, info.flag));
            extra.retain(|p| !same(p, info.flag));
        }
    }
    (flags.into_iter().flatten().collect(), extra.into_iter().flatten().collect())
}

pub fn logical_cores() -> u32 {
    let n = std::thread::available_parallelism().map_or(1, std::num::NonZeroUsize::get);
    u32::try_from(n).unwrap_or(u32::MAX).max(1)
}

pub fn auto_threads(spare_a_core: bool) -> Option<u32> {
    spare_a_core.then(|| logical_cores().saturating_sub(1).max(1))
}

pub fn bsp_flags(mode: BspMode, vis: VisMode, stop_on_leak: bool) -> Vec<&'static str> {
    let mut flags = match mode {
        BspMode::Off => Vec::new(),
        BspMode::OnlyEnts => vec!["-onlyents"],
        BspMode::OnlyProps => vec!["-onlyprops"],
        BspMode::EntsAndProps => vec!["-onlyents", "-onlyprops"],
        BspMode::Full => {
            let mut out = Vec::new();
            match vis {
                VisMode::Full => out.push("-vis"),
                VisMode::Fast => out.push("-fastvis"),
                VisMode::Off => {}
            }
            if stop_on_leak {
                out.push("-leaktest");
            }
            out
        }
    };
    if mode != BspMode::Off {
        flags.push("-AllowDynamicPropsAsStatic");
    }
    flags
}

pub fn rad_lighting(mode: RadMode, chosen: Lighting) -> Lighting {
    match (mode, chosen) {
        (RadMode::Final, _) => Lighting::Both,
        (RadMode::Draft, Lighting::Both) => Lighting::Ldr,
        (_, l) => l,
    }
}

pub const STATIC_PROP_LIGHTING: &str = "-StaticPropLighting";
pub const NO_STATIC_PROP_SHADOWS: &str = "-nostaticpropshadows";

pub fn rad_flags(mode: RadMode) -> Vec<&'static str> {
    match mode {
        RadMode::Off | RadMode::Normal => Vec::new(),
        RadMode::Draft => vec!["-bounce", "1", "-fast"],
        RadMode::Final => vec!["-final", STATIC_PROP_LIGHTING],
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn every_added_flag_is_explained() {
        for mode in [BspMode::Full, BspMode::OnlyEnts, BspMode::OnlyProps, BspMode::EntsAndProps] {
            for vis in [VisMode::Off, VisMode::Fast, VisMode::Full] {
                for leak in [true, false] {
                    for f in bsp_flags(mode, vis, leak) {
                        assert!(flag_info(StepKind::Bsp, f).is_some(), "{f}");
                    }
                }
            }
        }
        for mode in [RadMode::Draft, RadMode::Normal, RadMode::Final] {
            for f in rad_flags(mode) {
                if f.starts_with('-') {
                    assert!(flag_info(StepKind::Rad, f).is_some(), "{f}");
                }
            }
        }
        assert!(flag_info(StepKind::Bsp, "-FASTVIS").is_some());
        assert!(flag_info(StepKind::Rad, "-vis").is_none());
        assert_eq!(flag_info(StepKind::Rad, "-noextra").map(FlagInfo::key).as_deref(), Some("noextra"));
        let mut keys: Vec<String> = FLAGS.iter().map(|f| format!("{:?}{}", f.step, f.key())).collect();
        let n = keys.len();
        keys.sort();
        keys.dedup();
        assert_eq!(keys.len(), n, "one entry per step and flag");
        for f in FLAGS {
            assert!(!f.english.contains('\u{2014}'), "{}", f.flag);
        }
    }

    #[test]
    fn mode_flags() {
        assert_eq!(bsp_flags(BspMode::Full, VisMode::Full, true), ["-vis", "-leaktest", "-AllowDynamicPropsAsStatic"]);
        assert_eq!(bsp_flags(BspMode::Full, VisMode::Fast, false), ["-fastvis", "-AllowDynamicPropsAsStatic"]);
        assert_eq!(bsp_flags(BspMode::Full, VisMode::Off, false), ["-AllowDynamicPropsAsStatic"]);
        assert_eq!(bsp_flags(BspMode::OnlyEnts, VisMode::Full, true), ["-onlyents", "-AllowDynamicPropsAsStatic"]);
        assert_eq!(
            bsp_flags(BspMode::EntsAndProps, VisMode::Off, false),
            ["-onlyents", "-onlyprops", "-AllowDynamicPropsAsStatic"]
        );
        assert_eq!(rad_flags(RadMode::Draft), ["-bounce", "1", "-fast"]);
        assert_eq!(rad_flags(RadMode::Final), ["-final", "-StaticPropLighting"]);
        assert!(rad_flags(RadMode::Normal).is_empty());
        assert_eq!(rad_lighting(RadMode::Final, Lighting::Ldr), Lighting::Both);
        assert_eq!(rad_lighting(RadMode::Draft, Lighting::Both), Lighting::Ldr);
        assert_eq!(rad_lighting(RadMode::Draft, Lighting::Hdr), Lighting::Hdr);
        assert_eq!(rad_lighting(RadMode::Normal, Lighting::Both), Lighting::Both);
    }

    #[test]
    fn menu_lists_every_flag_once() {
        for step in [StepKind::Bsp, StepKind::Rad] {
            let listed: Vec<&str> = menu_flags(step).iter().flat_map(|(_, l)| l.iter().map(|f| f.flag)).collect();
            let all: Vec<&str> = FLAGS.iter().filter(|f| f.step == step).map(|f| f.flag).collect();
            assert_eq!(listed.len(), all.len(), "{step:?}");
            for f in all {
                assert!(listed.contains(&f), "{f}");
            }
        }
        assert_eq!(flag_group(StepKind::Rad, "-bounce"), FlagGroup::Quality);
        assert_eq!(flag_group(StepKind::Rad, "-StaticPropLighting"), FlagGroup::Props);
        assert_eq!(flag_group(StepKind::Bsp, "-radius_override"), FlagGroup::Visibility);
        assert_eq!(flag_group(StepKind::Bsp, "-nomerge"), FlagGroup::Debug);
        assert_eq!(managed_by(StepKind::Rad, "-NoStaticPropShadows"), Some(ManagedBy::StaticPropShadows));
        assert_eq!(managed_by(StepKind::Bsp, "-vis"), Some(ManagedBy::VisMode));
        assert_eq!(managed_by(StepKind::Rad, "-threads"), Some(ManagedBy::Threads));
        assert_eq!(managed_by(StepKind::Rad, "-final"), None);
        let info = flag_info(StepKind::Rad, NO_STATIC_PROP_SHADOWS).expect("explained");
        assert_eq!(info.group(), FlagGroup::Managed);
        assert_eq!(flag_info(StepKind::Bsp, "-trace").map(FlagInfo::value_count), Some(2));
    }

    fn choice(flag: &str, on: bool, value: &str) -> FlagChoice {
        FlagChoice { flag: flag.to_owned(), on, value: value.to_owned() }
    }

    fn words(s: &str) -> Vec<String> {
        crate::split_args(s)
    }

    #[test]
    fn merged_flags() {
        let draft = rad_flags(RadMode::Draft);
        let (f, e) = merge_flags(StepKind::Rad, &draft, &[], &words("-softsun 2"));
        assert_eq!((f, e), (words("-bounce 1 -fast"), words("-softsun 2")));
        let (f, e) = merge_flags(StepKind::Rad, &draft, &[], &words("-bounce 4 -x"));
        assert_eq!((f, e), (words("-fast"), words("-bounce 4 -x")));
        let choices = [choice("-bounce", true, "8"), choice("-FAST", false, ""), choice("-softsun", false, "")];
        let (f, e) = merge_flags(StepKind::Rad, &draft, &choices, &words("-bounce 4 -softsun 2 -noextra"));
        assert_eq!((f, e), (words("-bounce 8"), words("-noextra")));
        let ignored = [choice("-bounce", true, " "), choice("-threads", true, "3"), choice("-nope", true, "")];
        let (f, e) = merge_flags(StepKind::Rad, &draft, &ignored, &[]);
        assert_eq!((f, e), (words("-bounce 1 -fast"), Vec::<String>::new()));
        let bsp = [choice("-trace", true, "3 7"), choice("-embed", true, r"C:\my maps\extra")];
        let (f, _) = merge_flags(StepKind::Bsp, &bsp_flags(BspMode::Full, VisMode::Full, true), &bsp, &[]);
        assert_eq!(
            f,
            ["-vis", "-leaktest", "-AllowDynamicPropsAsStatic", "-trace", "3", "7", "-embed", r"C:\my maps\extra"]
        );
        assert!(choice("-trace", true, "3").args(StepKind::Bsp).is_none());
        assert!(choice("-final", false, "").args(StepKind::Rad).is_none());
        assert_eq!(choice("-FINAL", true, "x").args(StepKind::Rad), Some(vec!["-final".to_owned()]));
    }

    #[test]
    fn dynamic_models_are_kept_by_default_and_strict_validation_can_be_restored() {
        let default = bsp_flags(BspMode::Full, VisMode::Full, true);
        assert!(default.contains(&"-AllowDynamicPropsAsStatic"));
        let strict = [choice("-AllowDynamicPropsAsStatic", false, "")];
        let (args, _) = merge_flags(StepKind::Bsp, &default, &strict, &[]);
        assert!(!args.iter().any(|arg| arg.eq_ignore_ascii_case("-AllowDynamicPropsAsStatic")));
        assert!(args.iter().any(|arg| arg == "-vis"));
    }

    #[test]
    fn thread_policy() {
        assert_eq!(auto_threads(false), None);
        let n = auto_threads(true).unwrap_or(0);
        assert!(n >= 1 && n <= logical_cores());
    }
}
