use std::path::{Path, PathBuf};
use std::sync::Arc;

use hp_app::AppState;
use hp_app::runtime::ShaderRequest;
use hp_mcp::protocol::ToolResult;
use hp_shader::catalog::CATEGORIES;
use hp_shader::{BakeOptions, Diagnostic, EvalOptions, Graph, ImageCache, ImageLoader, Probe, Severity};
use hp_ui_textures::shader::EditorLoader;
use serde_json::{Value, json};

use super::semantic::*;
use super::tools::{AppFn, Args, MAX_NAME, Outcome, R, Worker, encode_png};
use super::tools_assets::{custom_dirs, inside};

const PRESET_IDS: &[&str] =
    &["blank", "tint", "normal_from_height", "tile_blend", "seamless", "color_grade", "grunge", "material_set"];
const MAX_GRAPH_BYTES: usize = 4 * 1024 * 1024;

fn graph_schema() -> Value {
    json!({
        "type": "object",
        "description": "A shader graph as the .hpshader file stores it: nodes (id, type, pos, params, inputs) and \
            links (from, out, to, in). See read_shader_graph for an example and list_shader_nodes for the node types."
    })
}

domain! { "shader editor";
    list_shader_nodes: Read, "Shader editor nodes",
        "The node types of the shader editor (the node based image and material editor): category, description, \
        input pins (name, type color, float or uv, default), output pins and parameters (type, range, options, \
        default). Unconnected inputs use their default; a float pin reads the brightness of a color.",
        {
            "category": choice("Only this category.", CATEGORIES),
            "query": string("Only node types whose name or description contains this text."),
        }, [];
    list_shader_presets: Read, "Shader editor presets",
        "The preset graphs create_shader_graph and the editor's New menu start from (for example tint, \
        normal_from_height, tile_blend).",
        {}, [];
    create_shader_graph: Edit, "Create a shader graph",
        "Write a new .hpshader file (JSON) from a preset. The file goes in the map folder or a custom texture \
        folder. Then edit_shader_graph changes it, render_shader_graph shows it and bake_shader_graph writes the \
        VTF and VMT files.",
        {
            "path": string("File to write: relative to the map folder, or absolute inside the map or a custom texture folder. The .hpshader extension is added when missing."),
            "preset": choice("Preset to start from (default blank: one texture node into one base texture output).", PRESET_IDS),
            "source": string("Image of the texture node: a file (map or custom texture folder) or a game texture name such as brick/brickwall001a."),
            "overwrite": boolean("Replace the file when it exists (default false)."),
        }, ["path"];
    read_shader_graph: Read, "Read a shader graph",
        "A shader graph as JSON (the .hpshader format) with its problems. Give path for a file, or live: true \
        for the graph open in the shader editor window.",
        {
            "path": string("The .hpshader file (relative to the map folder, or absolute)."),
            "live": boolean("Read the graph of the shader editor window instead."),
        }, [];
    edit_shader_graph: Edit, "Edit a shader graph",
        "Change a shader graph with a list of operations, all or nothing: add_node (type, optional pos, params, \
        inputs, as: an alias usable by the next operations), remove_node (node), set_param (node, name and value, \
        or params), set_input (node, pin, value: a number or 1, 3 or 4 numbers; null clears), connect (from, out, \
        to, in), disconnect (to, in), move_node (node, pos), set_size (width, height: 0 is automatic), clear. \
        Nodes are named by id or alias. Loops and unknown pins are refused. With live: true the graph of the \
        shader editor window changes (the user can undo).",
        {
            "path": string("The .hpshader file to change and save."),
            "live": boolean("Change the graph open in the shader editor window instead."),
            "ops": json!({
                "type": "array",
                "maxItems": hp_shader::MAX_OPS,
                "items": { "type": "object" },
                "description": "Operations, for example {\"op\":\"add_node\",\"type\":\"blur\",\"as\":\"b\",\"params\":{\"radius\":4}} then {\"op\":\"connect\",\"from\":1,\"out\":\"rgba\",\"to\":\"b\",\"in\":\"color\"}.",
            }),
        }, ["ops"];
    validate_shader_graph: Read, "Check a shader graph",
        "Problems of a shader graph: loops, broken links, outputs writing the same file, images that cannot be \
        loaded. Gives the graph from path, graph or live.",
        {
            "path": string("The .hpshader file."),
            "graph": graph_schema(),
            "live": boolean("Check the graph of the shader editor window."),
        }, [];
    render_shader_graph: Read, "Render a shader graph",
        "Compute a shader graph on the CPU and return the image: the first output node, or any node output with \
        node and output. The long edge is size pixels.",
        {
            "path": string("The .hpshader file."),
            "graph": graph_schema(),
            "live": boolean("Render the graph of the shader editor window."),
            "size": integer("Long edge in pixels (default 256).", 16, 1024),
            "node": integer("Node to show (default: the first output node).", 1, 100000),
            "output": string("Output pin of that node (default: its first)."),
        }, [];
    bake_shader_graph: Edit, "Write a shader graph to the game",
        "Compute a shader graph and write its outputs as VTF files (normal maps with the normal flag) and the VMT \
        of the base texture output, in a custom texture folder, like import_images does. Several output nodes \
        write several files (name_normal, name_envmask...). The graph is saved as name.hpshader next to them \
        unless save_graph is false.",
        {
            "path": string("The .hpshader file."),
            "graph": graph_schema(),
            "live": boolean("Write the graph of the shader editor window."),
            "folder": integer("Custom texture folder index (list_asset_sources).", 0, 255),
            "subfolder": string("Subfolder inside it."),
            "name": string("Name of the texture (letters, digits, _ and -)."),
            "surface_prop": string("$surfaceprop of the material."),
            "size": integer("Long edge in pixels (0 or missing: the size of the first image of the graph).", 0, 2048),
            "overwrite": boolean("Replace existing files (default false)."),
            "save_graph": boolean("Also write name.hpshader (default true)."),
        }, ["folder", "name"];
    open_shader_editor: Editor, "Open the shader editor",
        "Open the shader editor window: on a .hpshader file (path), on an image or a game texture (source, with \
        an optional preset), on a graph (graph, with a name) or empty. Shows what the user sees.",
        {
            "path": string("A .hpshader file, or an image file."),
            "source": string("An image file or a game texture or material name."),
            "preset": choice("Preset graph to start from.", PRESET_IDS),
            "graph": graph_schema(),
            "name": string("Name proposed for the written textures."),
        }, [];
}

fn allowed_dirs(state: &AppState) -> Vec<PathBuf> {
    let mut v = custom_dirs(state);
    if let Ok(d) = map_dir(state) {
        v.push(d);
    }
    v
}

fn shader_path(state: &AppState, p: &str) -> R<PathBuf> {
    let p = p.trim();
    if p.is_empty() {
        return Err("path is empty".to_owned());
    }
    let given = PathBuf::from(p);
    let path = if given.is_absolute() {
        if given.extension().is_some_and(|e| e.eq_ignore_ascii_case(hp_shader::EXTENSION)) {
            given
        } else {
            given.with_extension(hp_shader::EXTENSION)
        }
    } else {
        map_dir(state)?.join(relative_file(p, hp_shader::EXTENSION)?)
    };
    if !inside(&path, &allowed_dirs(state)) {
        return Err(format!(
            "{p}: shader graphs are only read and written in the map folder and the custom texture folders"
        ));
    }
    Ok(path)
}

struct Given {
    graph: Graph,
    path: Option<PathBuf>,
    warnings: Vec<String>,
}

fn graph_source(state: &AppState, a: &Args<'_>) -> R<Given> {
    let path = a.opt_str("path", 255)?;
    let inline = a.get("graph");
    let live = a.opt_bool("live")?.unwrap_or(false);
    let count = usize::from(path.is_some()) + usize::from(inline.is_some()) + usize::from(live);
    if count > 1 {
        return Err("give only one of path, graph and live".to_owned());
    }
    if let Some(p) = path {
        let file = shader_path(state, p)?;
        let (graph, warnings) = hp_shader::load_file(&file)?;
        return Ok(Given { graph, path: Some(file), warnings });
    }
    if let Some(v) = inline {
        let text = serde_json::to_string(v).map_err(|e| e.to_string())?;
        if text.len() > MAX_GRAPH_BYTES {
            return Err("graph is too big".to_owned());
        }
        let (graph, warnings) = Graph::parse(&text)?;
        return Ok(Given { graph, path: None, warnings });
    }
    let live_graph = state
        .runtime
        .shader
        .live
        .as_ref()
        .ok_or("the shader editor has no graph yet: give path or graph, or open it first (open_shader_editor)")?;
    let (graph, warnings) = Graph::parse(&live_graph.json)?;
    Ok(Given { graph, path: live_graph.path.clone(), warnings })
}

struct McpLoader {
    inner: EditorLoader,
    allowed: Vec<PathBuf>,
}

impl ImageLoader for McpLoader {
    fn load(&self, source: &str, max_dim: u32) -> Result<hp_shader::RgbaImage, String> {
        if let Some(file) = self.inner.files.resolve(source)
            && !inside(&file, &self.allowed)
        {
            return Err(format!("{source}: images are only read from the map folder and the custom texture folders"));
        }
        self.inner.load(source, max_dim)
    }
}

fn loader_for(state: &AppState, graph_path: Option<&Path>) -> Arc<McpLoader> {
    let mut dirs: Vec<PathBuf> = Vec::new();
    if let Some(d) = graph_path.and_then(Path::parent) {
        dirs.push(d.to_path_buf());
    }
    if let Ok(d) = map_dir(state) {
        dirs.push(d);
    }
    Arc::new(McpLoader { inner: EditorLoader::new(dirs, state.assets.fs().cloned()), allowed: allowed_dirs(state) })
}

fn diag_json(d: &Diagnostic) -> Value {
    json!({ "severity": d.severity.name(), "node": d.node, "code": d.code, "message": d.message() })
}

fn list_shader_nodes(_state: &mut AppState, a: &Args<'_>) -> R<ToolResult> {
    let v = hp_shader::describe_nodes(a.opt_str("category", 32)?, a.opt_str("query", 64)?)?;
    Ok(ToolResult::json(v))
}

fn list_shader_presets(_state: &mut AppState, _a: &Args<'_>) -> R<ToolResult> {
    Ok(ToolResult::json(json!({ "presets": hp_shader::describe_presets() })))
}

fn create_shader_graph(state: &mut AppState, a: &Args<'_>) -> R<ToolResult> {
    let file = shader_path(state, a.str("path", 255)?)?;
    let overwrite = a.opt_bool("overwrite")?.unwrap_or(false);
    if file.exists() && !overwrite {
        return Err(format!("{} already exists (overwrite: true replaces it)", file.display()));
    }
    let preset = a.opt_str("preset", 32)?.unwrap_or("blank");
    let source = a.opt_str("source", MAX_NAME)?.unwrap_or("").trim().to_owned();
    if !source.is_empty() {
        let as_file = PathBuf::from(&source);
        if (as_file.is_absolute() || source.contains(':')) && !inside(&as_file, &allowed_dirs(state)) {
            return Err(format!("{source}: images are only read from the map folder and the custom texture folders"));
        }
    }
    let mut graph = hp_shader::preset(preset, &source).ok_or_else(|| format!("unknown preset \"{preset}\""))?;
    if let Some(dir) = file.parent() {
        hp_shader::relativize_sources(&mut graph, dir);
    }
    hp_shader::save_file(&file, &graph)?;
    Ok(ToolResult::json(json!({
        "path": file.display().to_string(),
        "preset": preset,
        "nodes": graph.nodes.len(),
        "graph": serde_json::from_str::<Value>(&graph.to_json()).unwrap_or(Value::Null),
    })))
}

fn read_shader_graph(state: &mut AppState, a: &Args<'_>) -> R<ToolResult> {
    let g = graph_source(state, a)?;
    let live = state.runtime.shader.live.as_ref();
    let from_live = a.opt_bool("live")?.unwrap_or(false);
    Ok(ToolResult::json(json!({
        "path": g.path.as_ref().map(|p| p.display().to_string()),
        "unsaved_changes": from_live.then(|| live.is_some_and(|l| l.dirty)),
        "repaired": g.warnings,
        "problems": g.graph.validate().iter().map(diag_json).collect::<Vec<_>>(),
        "summary": hp_shader::describe_graph(&g.graph),
        "graph": serde_json::from_str::<Value>(&g.graph.to_json()).unwrap_or(Value::Null),
    })))
}

fn edit_shader_graph(state: &mut AppState, a: &Args<'_>) -> R<Outcome> {
    let ops: Vec<Value> = a.get("ops").and_then(Value::as_array).cloned().ok_or("ops must be a list")?;
    let live = a.opt_bool("live")?.unwrap_or(false);
    let path = a.opt_str("path", 255)?;
    if live == path.is_some() {
        return Err("give either path or live: true".to_owned());
    }
    if live {
        return Ok(Outcome::AppFn(AppFn(Box::new(move |app, _ctx| {
            let editor = app.texture_dialogs.shader_editor();
            match editor.apply_ops(&ops) {
                Ok(applied) => ToolResult::json(json!({
                    "applied": applied,
                    "summary": hp_shader::describe_graph(editor.graph()),
                    "problems": editor.graph().validate().iter().map(diag_json).collect::<Vec<_>>(),
                })),
                Err(e) => ToolResult::error(format!("edit_shader_graph: {e}")),
            }
        }))));
    }
    let file = shader_path(state, path.unwrap_or_default())?;
    let (mut graph, _) = hp_shader::load_file(&file)?;
    let applied = hp_shader::apply_ops(&mut graph, &ops)?;
    if let Some(dir) = file.parent() {
        hp_shader::relativize_sources(&mut graph, dir);
    }
    hp_shader::save_file(&file, &graph)?;
    Ok(ToolResult::json(json!({
        "path": file.display().to_string(),
        "applied": applied,
        "summary": hp_shader::describe_graph(&graph),
        "problems": graph.validate().iter().map(diag_json).collect::<Vec<_>>(),
    }))
    .into())
}

fn validate_shader_graph(state: &mut AppState, a: &Args<'_>) -> R<Outcome> {
    let g = graph_source(state, a)?;
    let loader = loader_for(state, g.path.as_deref());
    let warnings = g.warnings.clone();
    Ok(Worker::spawn("validate_shader_graph", move || {
        let mut problems = g.graph.validate();
        let opts = EvalOptions { width: 32, height: 32, outputs: true, ..EvalOptions::default() };
        let eval = hp_shader::evaluate(&g.graph, loader.as_ref(), &ImageCache::new(), &opts);
        for d in eval.diagnostics {
            if !problems.iter().any(|p| p.node == d.node && p.code == d.code) {
                problems.push(d);
            }
        }
        let errors = problems.iter().filter(|d| d.severity == Severity::Error).count();
        ToolResult::json(json!({
            "valid": errors == 0,
            "errors": errors,
            "repaired": warnings,
            "problems": problems.iter().map(diag_json).collect::<Vec<_>>(),
            "outputs": g.graph.output_ids().iter().map(|id| json!({
                "node": id,
                "target": g.graph.output_target(*id),
                "file_suffix": g.graph.output_suffix(*id),
            })).collect::<Vec<_>>(),
        }))
    }))
}

fn render_shader_graph(state: &mut AppState, a: &Args<'_>) -> R<Outcome> {
    let g = graph_source(state, a)?;
    let size = a.opt_u64("size", 16, 1024)?.unwrap_or(256) as u32;
    let node = a.opt_u64("node", 1, 100_000)?.map(|n| n as u32);
    let output = a.opt_str("output", 32)?.map(str::to_owned);
    let probe = match node {
        Some(n) => {
            let def = g.graph.def_of(n).ok_or_else(|| format!("no node {n}"))?;
            let out = match output {
                Some(o) => {
                    if def.output(&o).is_none() && !def.is_output() {
                        return Err(format!("{} has no output \"{o}\"", def.id));
                    }
                    o
                }
                None => def.outputs.first().map_or_else(|| "color".to_owned(), |p| p.name.to_owned()),
            };
            Probe { node: n, output: out }
        }
        None => {
            let id = *g.graph.output_ids().first().ok_or("the graph has no output node: give node")?;
            Probe { node: id, output: "color".to_owned() }
        }
    };
    let loader = loader_for(state, g.path.as_deref());
    Ok(Worker::spawn("render_shader_graph", move || {
        let cache = ImageCache::new();
        let (fw, fh) = hp_shader::auto_size(&g.graph, loader.as_ref(), &cache);
        let scale = size as f32 / fw.max(fh) as f32;
        let w = ((fw as f32 * scale).round() as u32).clamp(4, size);
        let h = ((fh as f32 * scale).round() as u32).clamp(4, size);
        let opts =
            EvalOptions { width: w, height: h, outputs: false, probe: Some(probe.clone()), ..EvalOptions::default() };
        let eval = hp_shader::evaluate(&g.graph, loader.as_ref(), &cache, &opts);
        let Some(img) = eval.probe else {
            return ToolResult::error("render_shader_graph: nothing to show for that node");
        };
        let problems: Vec<String> = eval.diagnostics.iter().map(Diagnostic::message).collect();
        let caption = format!(
            "{}x{} (node {}, {}){}",
            img.width,
            img.height,
            probe.node,
            probe.output,
            if problems.is_empty() { String::new() } else { format!("; problems: {}", problems.join("; ")) }
        );
        match encode_png(img.width, img.height, img.pixels) {
            Ok(png) => ToolResult::png(&png, caption),
            Err(e) => ToolResult::error(format!("render_shader_graph: {e}")),
        }
    }))
}

fn bake_shader_graph(state: &mut AppState, a: &Args<'_>) -> R<Outcome> {
    let mut g = graph_source(state, a)?;
    let index = a.opt_u64("folder", 0, 255)?.ok_or("folder is required")? as usize;
    let dir =
        state.settings.game.custom_texture_dirs.get(index).ok_or("folder: no custom texture folder has this index")?;
    if !dir.enabled {
        return Err("folder: this custom texture folder is disabled".to_owned());
    }
    let layout = hp_ui_textures::folders::layout_of(state, dir);
    let (mut dest, mut prefix) = hp_ui_textures::folders::import_target(&dir.path, &layout);
    if let Some(sub) = a.opt_str("subfolder", 128)? {
        let sub = relative_file(sub, "dir")?;
        let sub = sub.trim_end_matches(".dir").to_ascii_lowercase();
        dest = dest.join(&sub);
        prefix = if prefix.is_empty() { sub } else { format!("{}/{sub}", prefix.trim_matches('/')) };
    }
    let surface_prop = a.opt_str("surface_prop", 63)?.map(str::trim).filter(|s| !s.is_empty()).map(str::to_owned);
    if surface_prop.as_deref().is_some_and(|s| !s.chars().all(|c| c.is_ascii_alphanumeric() || c == '_')) {
        return Err("surface_prop must be letters, digits and underscores".to_owned());
    }
    let name = hp_shader::sanitize_stem(a.str("name", 64)?);
    if name.is_empty() {
        return Err("name must contain letters or digits".to_owned());
    }
    if let Some(size) = a.opt_u64("size", 0, 2048)? {
        g.graph.settings.width = size as u32;
        g.graph.settings.height = 0;
    }
    let overwrite = a.opt_bool("overwrite")?.unwrap_or(false);
    let save_graph = a.opt_bool("save_graph")?.unwrap_or(true);
    let loader = loader_for(state, g.path.as_deref());
    let opts = BakeOptions {
        dest_dir: dest.clone(),
        prefix,
        stem: name.clone(),
        surface_prop,
        overwrite,
        width: 0,
        height: 0,
    };
    let graph = g.graph;
    Ok(Worker::spawn_then("bake_shader_graph", move || {
        let cache = ImageCache::new();
        let graph_file = dest.join(format!("{name}.{}", hp_shader::EXTENSION));
        if save_graph && !overwrite && graph_file.exists() {
            return (ToolResult::error(format!("bake_shader_graph: {} already exists", graph_file.display())), None);
        }
        let report = match hp_shader::bake(&graph, loader.as_ref(), &cache, &opts) {
            Ok(r) => r,
            Err(e) => return (ToolResult::error(format!("bake_shader_graph: {e}")), None),
        };
        let mut saved = None;
        if save_graph {
            let mut copy = graph.clone();
            hp_shader::relativize_sources(&mut copy, &dest);
            match hp_shader::save_file(&graph_file, &copy) {
                Ok(()) => saved = Some(graph_file.display().to_string()),
                Err(e) => return (ToolResult::error(format!("bake_shader_graph: {e}")), None),
            }
        }
        let first =
            report.material.as_ref().map(|m| m.0.clone()).or_else(|| report.files.first().map(|f| f.texture.clone()));
        let result = ToolResult::json(json!({
            "files": report.files.iter().map(|f| json!({
                "texture": f.texture,
                "target": f.target,
                "vtf": f.vtf.display().to_string(),
                "width": f.width,
                "height": f.height,
                "bytes": f.bytes,
            })).collect::<Vec<_>>(),
            "material": report.material.as_ref().map(|m| m.0.clone()),
            "vmt": report.material.as_ref().map(|m| m.1.display().to_string()),
            "graph_file": saved,
            "problems": report.diagnostics.iter().map(diag_json).collect::<Vec<_>>(),
        }));
        let apply: super::tools::ApplyFn = Box::new(move |state: &mut AppState| {
            if let Some(name) = &first {
                state.use_material(name);
            }
            state.assets.remount(&state.settings);
            None
        });
        (result, Some(apply))
    }))
}

fn open_shader_editor(state: &mut AppState, a: &Args<'_>) -> R<ToolResult> {
    let mut req = ShaderRequest::default();
    if let Some(p) = a.opt_str("path", 255)? {
        let as_graph = Path::new(p).extension().is_some_and(|e| e.eq_ignore_ascii_case(hp_shader::EXTENSION));
        if as_graph {
            let file = shader_path(state, p)?;
            if !file.is_file() {
                return Err(format!("{p}: no such file"));
            }
            req.path = Some(file);
        } else {
            let file = PathBuf::from(p.trim());
            let file = if file.is_absolute() { file } else { map_dir(state)?.join(p.trim().replace('\\', "/")) };
            if !inside(&file, &allowed_dirs(state)) || !file.is_file() {
                return Err(format!("{p}: not an image of the map folder or a custom texture folder"));
            }
            req.path = Some(file);
        }
    }
    if let Some(s) = a.opt_str("source", MAX_NAME)? {
        let as_file = PathBuf::from(s.trim());
        if (as_file.is_absolute() || s.contains(':')) && !inside(&as_file, &allowed_dirs(state)) {
            return Err(format!("{s}: images are only read from the map folder and the custom texture folders"));
        }
        req.source = Some(s.trim().to_owned());
    }
    if let Some(p) = a.opt_enum("preset", PRESET_IDS)? {
        req.preset = Some(p.to_owned());
    }
    if let Some(v) = a.get("graph") {
        let text = serde_json::to_string(v).map_err(|e| e.to_string())?;
        if text.len() > MAX_GRAPH_BYTES {
            return Err("graph is too big".to_owned());
        }
        Graph::parse(&text)?;
        req.graph_json = Some(text);
    }
    req.name = a.opt_str("name", 64)?.map(str::to_owned);
    state.runtime.shader.open(req);
    Ok(ToolResult::json(
        json!({ "open": true, "note": "The window opens on the next frame; read_shader_graph with live: true reads its graph." }),
    ))
}

#[cfg(test)]
mod tests {
    use hp_app::settings::CustomDirSetting;

    use super::super::semantic_tests::{call, err, ok};
    use super::*;

    fn scratch(name: &str) -> PathBuf {
        let root = std::env::var_os("HP_SCRATCH").map_or_else(std::env::temp_dir, PathBuf::from);
        let dir = root.join("hammerpro-mcp-shader").join(name);
        let _ = std::fs::remove_dir_all(&dir);
        let _ = std::fs::create_dir_all(&dir);
        dir
    }

    fn state_in(dir: &Path) -> AppState {
        let mut s = hp_app::testing::test_state();
        s.doc.set_path(Some(dir.join("test.vmf")));
        let px: Vec<u8> = (0..16 * 16).flat_map(|i| [(i % 16 * 16) as u8, 80, 120, 255]).collect();
        image::save_buffer(dir.join("wall.png"), &px, 16, 16, image::ColorType::Rgba8).unwrap();
        s
    }

    #[test]
    fn preset_ids_match_the_engine() {
        let engine: Vec<&str> = hp_shader::PRESETS.iter().map(|p| p.id).collect();
        assert_eq!(PRESET_IDS, engine.as_slice());
    }

    #[test]
    fn specs_are_well_formed() {
        assert_eq!(SPECS.len(), 9);
        for t in SPECS {
            assert!(t.name.chars().all(|c| c.is_ascii_lowercase() || c == '_'), "{}", t.name);
            assert!(t.description.len() > 20 && !t.description.contains("  "), "{}", t.name);
            let d = t.definition();
            let text = serde_json::to_string(&d).unwrap();
            assert!(!text.contains("oneOf") && !text.contains("anyOf") && !text.contains("\"type\":["), "{}", t.name);
            for c in ['\u{2013}', '\u{2014}'] {
                assert!(!text.contains(c), "{}", t.name);
            }
            let schema = &d["inputSchema"];
            for r in schema["required"].as_array().unwrap() {
                assert!(schema["properties"].get(r.as_str().unwrap()).is_some(), "{}: {r}", t.name);
            }
        }
    }

    #[test]
    fn catalog_tools() {
        let mut s = hp_app::testing::test_state();
        let all = ok(&mut s, "list_shader_nodes", json!({}));
        assert!(all["nodes"].as_array().unwrap().len() > 40);
        let filter = ok(&mut s, "list_shader_nodes", json!({ "category": "filter", "query": "blur" }));
        assert_eq!(filter["nodes"][0]["type"], "blur");
        err(&mut s, "list_shader_nodes", json!({ "category": "nope" }));
        let presets = ok(&mut s, "list_shader_presets", json!({}));
        assert_eq!(presets["presets"].as_array().unwrap().len(), PRESET_IDS.len());
    }

    #[test]
    fn graph_files_are_created_edited_checked_and_rendered() {
        let dir = scratch("files");
        let mut s = state_in(&dir);
        let r = ok(&mut s, "create_shader_graph", json!({ "path": "wall", "preset": "tint", "source": "wall.png" }));
        assert_eq!(r["nodes"], 3);
        assert!(dir.join("wall.hpshader").is_file());
        err(&mut s, "create_shader_graph", json!({ "path": "wall" }));
        let again = json!({ "path": "wall", "overwrite": true, "preset": "seamless", "source": "wall.png" });
        ok(&mut s, "create_shader_graph", again);
        let r = ok(&mut s, "read_shader_graph", json!({ "path": "wall.hpshader" }));
        assert_eq!(r["summary"]["nodes"].as_array().unwrap().len(), 3);
        assert!(r["problems"].as_array().unwrap().is_empty());
        err(&mut s, "read_shader_graph", json!({ "live": true }));
        err(&mut s, "read_shader_graph", json!({ "path": "wall", "live": true }));

        let ops = json!([
            { "op": "add_node", "type": "blur", "as": "b", "params": { "radius": 2 } },
            { "op": "connect", "from": 2, "out": "color", "to": "b", "in": "color" },
            { "op": "connect", "from": "b", "out": "color", "to": 3, "in": "color" },
        ]);
        let r = ok(&mut s, "edit_shader_graph", json!({ "path": "wall", "ops": ops }));
        assert_eq!(r["applied"][0]["id"], 4);
        let before = std::fs::read_to_string(dir.join("wall.hpshader")).unwrap();
        let bad = json!([{ "op": "connect", "from": 3, "out": "x", "to": 1, "in": "uv" }]);
        err(&mut s, "edit_shader_graph", json!({ "path": "wall", "ops": bad }));
        assert_eq!(
            std::fs::read_to_string(dir.join("wall.hpshader")).unwrap(),
            before,
            "a refused edit changes nothing"
        );
        err(&mut s, "edit_shader_graph", json!({ "ops": [] }));
        err(&mut s, "edit_shader_graph", json!({ "path": "wall", "live": true, "ops": [] }));

        let v = ok(&mut s, "validate_shader_graph", json!({ "path": "wall" }));
        assert_eq!(v["valid"], true, "{v}");
        let r = call(&mut s, "render_shader_graph", json!({ "path": "wall", "size": 32 }));
        assert!(!r.is_error, "{}", r.first_text());
        let r = call(&mut s, "render_shader_graph", json!({ "path": "wall", "size": 32, "node": 4 }));
        assert!(!r.is_error, "{}", r.first_text());
        err(&mut s, "render_shader_graph", json!({ "path": "wall", "node": 99 }));
        err(&mut s, "render_shader_graph", json!({ "graph": { "nodes": [] } }));

        let missing = json!({ "nodes": [
            { "id": 1, "type": "texture", "params": { "source": "gone.png" } },
            { "id": 2, "type": "output" }
        ], "links": [{ "from": 1, "out": "rgba", "to": 2, "in": "color" }] });
        let v = ok(&mut s, "validate_shader_graph", json!({ "graph": missing }));
        assert!(v["problems"].as_array().unwrap().iter().any(|p| p["code"] == "missing_texture"), "{v}");
    }

    #[test]
    fn paths_are_kept_inside_the_map_and_custom_folders() {
        let dir = scratch("paths");
        let mut s = state_in(&dir);
        assert!(err(&mut s, "create_shader_graph", json!({ "path": "C:/Windows/evil.hpshader" })).contains("only"));
        err(&mut s, "create_shader_graph", json!({ "path": "../evil" }));
        err(&mut s, "read_shader_graph", json!({ "path": "C:/Windows/win.ini" }));
        let graph = json!({ "nodes": [
            { "id": 1, "type": "texture", "params": { "source": "C:/Windows/Web/Wallpaper/Windows/img0.jpg" } },
            { "id": 2, "type": "output" }
        ], "links": [{ "from": 1, "out": "rgba", "to": 2, "in": "color" }] });
        let v = ok(&mut s, "validate_shader_graph", json!({ "graph": graph }));
        assert!(!v["problems"].as_array().unwrap().is_empty(), "outside images are not read: {v}");
        err(&mut s, "open_shader_editor", json!({ "path": "C:/Windows/win.ini" }));
        err(&mut s, "open_shader_editor", json!({ "source": "C:/Windows/win.ini" }));
    }

    #[test]
    fn baking_writes_vtf_and_vmt_and_the_graph() {
        let dir = scratch("bake");
        let mut s = state_in(&dir);
        let out = dir.join("materials");
        std::fs::create_dir_all(&out).unwrap();
        let custom = CustomDirSetting { path: out, prefix: "custom/test".into(), enabled: true };
        s.settings.game.custom_texture_dirs.push(custom);
        ok(
            &mut s,
            "create_shader_graph",
            json!({ "path": "set", "preset": "normal_from_height", "source": "wall.png" }),
        );
        err(&mut s, "bake_shader_graph", json!({ "path": "set", "folder": 5, "name": "x" }));
        err(&mut s, "bake_shader_graph", json!({ "path": "set", "folder": 0, "name": "___" }));
        err(&mut s, "bake_shader_graph", json!({ "path": "set", "folder": 0, "name": "x", "surface_prop": "a b" }));
        let args = json!({ "path": "set", "folder": 0, "name": "Brick Wall", "surface_prop": "brick", "size": 32 });
        let r = ok(&mut s, "bake_shader_graph", args);
        assert_eq!(r["files"].as_array().unwrap().len(), 2, "{r}");
        let vmt = PathBuf::from(r["vmt"].as_str().unwrap());
        assert!(vmt.is_file(), "{r}");
        let dest = vmt.parent().unwrap().to_path_buf();
        assert!(dest.join("brick_wall.vtf").is_file() && dest.join("brick_wall_normal.vtf").is_file());
        assert!(dest.join("brick_wall.hpshader").is_file());
        let info = hp_vtf::read_info(&std::fs::read(dest.join("brick_wall.vtf")).unwrap()).unwrap();
        assert_eq!((info.width, info.height), (32, 32));
        let text = std::fs::read_to_string(&vmt).unwrap();
        assert!(text.contains("$bumpmap") && text.contains("brick_wall_normal"), "{text}");
        assert_eq!(s.current_material, r["material"].as_str().unwrap());
        err(&mut s, "bake_shader_graph", json!({ "path": "set", "folder": 0, "name": "Brick Wall" }));
        let again = json!({ "path": "set", "folder": 0, "name": "Brick Wall", "overwrite": true, "save_graph": false });
        ok(&mut s, "bake_shader_graph", again);
    }

    #[test]
    fn the_editor_window_opens_from_a_tool() {
        let dir = scratch("open");
        let mut s = state_in(&dir);
        ok(&mut s, "create_shader_graph", json!({ "path": "g", "source": "wall.png" }));
        ok(&mut s, "open_shader_editor", json!({ "path": "g.hpshader" }));
        assert!(s.runtime.shader.show);
        let req = s.runtime.shader.request.take().unwrap();
        assert_eq!(req.path.unwrap().file_name().unwrap(), "g.hpshader");
        ok(&mut s, "open_shader_editor", json!({ "source": "brick/brickwall001a", "preset": "tint" }));
        assert_eq!(s.runtime.shader.request.take().unwrap().preset.as_deref(), Some("tint"));
        let inline = json!({ "nodes": [{ "id": 1, "type": "output" }] });
        ok(&mut s, "open_shader_editor", json!({ "graph": inline, "name": "x" }));
        assert!(s.runtime.shader.request.take().unwrap().graph_json.is_some());
        err(&mut s, "open_shader_editor", json!({ "path": "missing.hpshader" }));
    }
}
