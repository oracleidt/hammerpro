#[path = "../../hammerpro/src/bspc_cli.rs"]
#[allow(dead_code)]
mod bspc_cli;
#[path = "../../hammerpro/src/bspc_game.rs"]
#[allow(dead_code)]
mod bspc_game;

#[allow(dead_code)]
mod bspc_optimize {
    use std::path::{Path, PathBuf};

    use glam::DVec3;
    use hp_bspc::MaterialFacts;
    use hp_geom::Aabb;
    use hp_vmf::{Entity, Map};

    #[derive(Debug, Default, Clone, PartialEq, Eq)]
    pub(crate) struct OptimizeArgs {
        pub(crate) merge_props: bool,
        pub(crate) hints: bool,
        pub(crate) fade: bool,
        pub(crate) occluders: bool,
        pub(crate) model_compiler: Option<PathBuf>,
        pub(crate) model_compiler_args: String,
    }

    pub(crate) const HINT_CANDIDATES: usize = 8;
    pub(crate) const HINT_BUDGET_SECS: u64 = 180;

    impl OptimizeArgs {
        pub(crate) fn any(&self) -> bool {
            self.merge_props || self.hints || self.fade || self.occluders
        }

        pub(crate) fn flags(&self) -> Vec<&'static str> {
            [
                (self.hints, "-autohints"),
                (self.occluders, "-autooccluders"),
                (self.fade, "-autofade"),
                (self.merge_props, "-mergestaticprops"),
            ]
            .into_iter()
            .filter_map(|(on, f)| on.then_some(f))
            .collect()
        }
    }

    pub(crate) fn default_model_compiler(_root: &Path) -> Option<PathBuf> {
        None
    }

    pub(crate) fn merged_dir(vmf: &Path) -> PathBuf {
        let stem = vmf.file_stem().map_or_else(|| "map".into(), |s| s.to_string_lossy().into_owned());
        vmf.with_file_name(format!("{stem}_merged_props"))
    }

    #[derive(Debug, Default, Clone, PartialEq)]
    pub(crate) struct FadeReport {
        pub(crate) props: usize,
        pub(crate) kept: usize,
        pub(crate) skybox: usize,
        pub(crate) big: usize,
        pub(crate) detail: Option<(f64, f64)>,
    }

    pub(crate) fn auto_fade(
        _map: &mut Map,
        _bounds: &dyn Fn(&str) -> Option<(DVec3, DVec3)>,
        _facts: &dyn Fn(&str) -> MaterialFacts,
        _detail_size: &dyn Fn(&[String]) -> Option<f64>,
    ) -> FadeReport {
        FadeReport::default()
    }

    pub(crate) fn detail_size(
        _fs: &hp_fs::GameFs,
        _world: &Entity,
        _types: &[String],
        _bounds: &dyn Fn(&str) -> Option<(DVec3, DVec3)>,
    ) -> Option<f64> {
        None
    }

    #[derive(Clone, Debug, PartialEq)]
    pub(crate) struct Occluder {
        pub(crate) slab: Aabb,
        pub(crate) axis: usize,
        pub(crate) props: usize,
    }

    pub(crate) fn plan_occluders(
        _map: &Map,
        _bounds: &dyn Fn(&str) -> Option<(DVec3, DVec3)>,
        _facts: &dyn Fn(&str) -> MaterialFacts,
    ) -> Vec<Occluder> {
        Vec::new()
    }

    pub(crate) fn add_occluders(_map: &mut Map, _list: &[Occluder]) -> usize {
        0
    }

    #[derive(Debug, Default, Clone, PartialEq)]
    pub(crate) struct MergeReport {
        pub(crate) props: usize,
        pub(crate) models: usize,
        pub(crate) draw_before: usize,
        pub(crate) draw_after: usize,
        pub(crate) failed: Vec<String>,
    }

    #[allow(clippy::too_many_arguments)]
    pub(crate) fn merge_static_props(
        _map: &mut Map,
        _fs: &hp_fs::GameFs,
        _exe: &Path,
        _extra_args: &str,
        _out_dir: &Path,
        _stem: &str,
        _facts: &dyn Fn(&str) -> MaterialFacts,
        log: &(dyn Fn(&str) + Sync),
    ) -> MergeReport {
        log("Warning: -mergestaticprops is not supported by this tool (use hammerpro --bspc)");
        MergeReport::default()
    }

    #[derive(Debug, Default, Clone, PartialEq)]
    pub(crate) struct HintReport {
        pub(crate) added: usize,
        pub(crate) measured: usize,
        pub(crate) base_pvs: u64,
        pub(crate) gain: i64,
        pub(crate) note: Option<String>,
    }

    pub(crate) fn auto_hints(
        _map: &mut Map,
        _fs: Option<&hp_fs::GameFs>,
        _log: &(dyn Fn(&str) + Sync),
    ) -> Result<HintReport, String> {
        Err("-autohints is not supported by this tool (use hammerpro --bspc)".into())
    }
}

#[allow(dead_code)]
mod bspc_instances {
    use std::path::Path;

    pub(crate) struct InstanceFiles<'a> {
        pub(crate) vmf: &'a Path,
        pub(crate) game_dir: Option<&'a Path>,
        pub(crate) fs: Option<&'a hp_fs::GameFs>,
    }

    pub(crate) fn has_instances(map: &hp_vmf::Map) -> bool {
        map.entities.iter().any(|e| e.classname.eq_ignore_ascii_case("func_instance"))
    }

    pub(crate) fn merge_instances(
        _map: &mut hp_vmf::Map,
        _files: &InstanceFiles<'_>,
        _root: Option<&Path>,
        log: &mut dyn FnMut(&str),
    ) -> usize {
        log("Warning: func_instance is not merged by this tool (use hammerpro --bspc)");
        0
    }

    pub(crate) fn collapse(
        _map: &mut hp_vmf::Map,
        _files: &InstanceFiles<'_>,
        _fgd: Option<&hp_fgd::ResolvedFgd>,
        log: &mut dyn FnMut(&str),
    ) -> usize {
        log("Warning: func_instance is not merged by this tool (use hammerpro --bspc)");
        0
    }
}

fn main() {
    let args: Vec<String> = std::env::args().skip(1).collect();
    std::process::exit(bspc_cli::run(&args));
}
