#![forbid(unsafe_code)]

mod bake;
pub mod catalog;
mod edit;
mod eval;
mod graph;
mod ops;
mod presets;

pub use bake::{BakeOptions, BakeReport, BakedFile, bake, sanitize_stem};
pub use edit::{MAX_OPS, apply_ops, describe_graph, describe_nodes, describe_presets, node_json};
pub use eval::{
    EvalOptions, Evaluation, FileLoader, ImageCache, ImageLoader, OutputImage, Probe, Source, auto_size,
    downscale_to_fit, evaluate,
};
pub use graph::{
    Diagnostic, FORMAT, Graph, Link, MAX_LINKS, MAX_NODES, MAX_STOPS, Node, Params, Settings, Severity, VERSION,
    default_param, normalize_param, sanitize_suffix,
};
pub use hp_vtf::RgbaImage;
pub use presets::{PRESETS, PresetInfo, preset};

pub const MAX_BAKE_SIZE: u32 = 2048;
pub const DEFAULT_BAKE_SIZE: u32 = 512;
pub const EXTENSION: &str = "hpshader";

use std::path::{Path, PathBuf};

pub fn companion_path(image: &Path) -> PathBuf {
    image.with_extension(EXTENSION)
}

pub fn load_file(path: &Path) -> Result<(Graph, Vec<String>), String> {
    let meta = std::fs::metadata(path).map_err(|e| format!("{}: {e}", path.display()))?;
    if meta.len() as usize > graph::MAX_BYTES {
        return Err(format!("{}: the shader file is too big", path.display()));
    }
    let text = std::fs::read_to_string(path).map_err(|e| format!("{}: {e}", path.display()))?;
    Graph::parse(&text).map_err(|e| format!("{}: {e}", path.display()))
}

pub fn save_file(path: &Path, graph: &Graph) -> Result<(), String> {
    if let Some(dir) = path.parent().filter(|d| !d.as_os_str().is_empty()) {
        std::fs::create_dir_all(dir).map_err(|e| format!("{}: {e}", dir.display()))?;
    }
    bake::write_atomic(path, graph.to_json().as_bytes())
}

pub fn relativize_sources(graph: &mut Graph, base: &Path) {
    graph.map_sources(|s| {
        let p = Path::new(s);
        match p.strip_prefix(base) {
            Ok(rel) if p.is_absolute() => rel.to_string_lossy().replace('\\', "/"),
            _ => s.to_owned(),
        }
    });
}
