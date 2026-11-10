use std::sync::atomic::AtomicBool;

use hp_app::AppState;
use hp_app::settings::BlenderFormat;
use hp_mcp::protocol::ToolResult;
use hp_ui_entities::model_import::batch::{
    FileResult, Formats, GridOptions, Status, clamp_threads, collect_files, names_for, run, scan,
};
use hp_ui_entities::model_import::content_root;
use serde_json::{Value, json};

use super::semantic::*;
use super::tools::{ApplyFn, Args, Outcome, R, Worker};
use crate::batch::{Spec, parse_collision, parse_up};
use crate::blender::{self, SendOptions, find, pull};

const MAX_RESULTS: usize = 500;

domain! { "blender";
    send_to_blender: Editor, "Send to Blender",
        "Send the selection to Blender and link it for a round trip. Selected displacements (or solids \
        that have displacements) are exported one object per displacement, named \
        HPDISP_<solid>_<side>_p<power>, with their materials and UVs; edit or sculpt them in Blender, \
        and on the update each one is written back into the same displacement (alpha, flags, tags, \
        lightmap scale and blend data of untouched vertices are kept, seams are sewn again). Without \
        displacements, selected brushes are exported as a mesh and come back as brushes, and a selected \
        prop with a model is exported as its render mesh and comes back as a recompiled model. The files \
        go in the Hammer Pro working folder and Blender is started with a generated script that imports \
        them and loads the Hammer Pro Bridge add-on (an Update in Hammer Pro button, and an export on \
        save). The format defaults to the setting blender.format (gltf, obj or fbx). With the live link \
        on, each time Blender writes the same file Hammer Pro applies it. When Blender is not found the \
        file is still exported and the answer says so (set the program with blender_setup). Read the \
        state with blender_status and pull by hand with blender_pull.",
        {
            "format": choice("File format; default the blender.format setting.", &["gltf", "obj", "fbx"]),
            "launch": boolean("Start Blender (default true). False only writes the files."),
            "live_link": boolean("Watch the file for changes from Blender (default the blender.live_link setting)."),
        }, [];
    blender_status: Read, "Blender bridge status",
        "The state of the Blender bridge: whether Blender was found and where, the settings blender.path, \
        blender.format and blender.live_link, the working folder, the current link (file, format, what it is \
        linked to, pulls done), whether an update is running, and the last message.",
        {}, [];
    blender_pull: Edit, "Update from Blender",
        "Read the file of the Blender link now, without waiting for Blender to save it. Displacements: \
        each object named HPDISP_<solid>_<side>_p<power> whose grid is intact is written back into its \
        displacement (only the vertices moved in Blender change), a displacement whose mesh was \
        remodeled is reshaped by projecting the new mesh on it (mesh to displacement), a new mesh \
        becomes new displacements, and the seams are sewn again. Brushes are replaced by brushes rebuilt \
        from the mesh (a mesh too complex for brushes is placed as a prop). A prop link recompiles the \
        model (textures, collision, levels of detail) and refreshes the props that use it. One undo \
        step. Fails without a link (send_to_blender first).",
        {}, [];
    blender_setup: Editor, "Set up the Blender bridge",
        "Change the Blender bridge settings and maintain it: path (the Blender program, checked: it must be a \
        file named blender*, or a Blender folder), format, live_link, detect (search Blender again and report \
        it), install_addon (write the Hammer Pro Bridge add-on into Blender's add-on folders, or into the \
        working folder when Blender has none) and stop_link (end the live link). Returns blender_status.",
        {
            "path": string("Path to the Blender program (blender.exe) or to its folder; empty text clears it."),
            "format": choice("Default export format.", &["gltf", "obj", "fbx"]),
            "live_link": boolean("Default for the live link."),
            "detect": boolean("Search for Blender and report what is found."),
            "install_addon": boolean("Install the Hammer Pro Bridge add-on."),
            "stop_link": boolean("End the current live link."),
        }, [];
    import_models_batch: Edit, "Import models in batch",
        "Convert many OBJ, FBX, glTF and GLB files into Garry's Mod models in one run (the batch form of the \
        model import): files or folders (recursive option), a naming template with {name} (file name), {folder} \
        (its folder) and {index}, a prefix, the import options (scale, units, up axis, origin, kind, surface \
        property, mass, collision) and the saved model options (levels of detail, material parameters), \
        parallel conversion, skip or replace of models that exist, and optionally one prop per model placed in a \
        grid in front of the camera (one undo step). Models go in models/hammerpro of the game content. With \
        dry_run nothing is written: it returns the preview table (triangles, size, name, warnings). Returns a \
        per file result (ok, warning, error, skipped) and a summary.",
        {
            "paths": strings("Files or folders to convert.", 2000),
            "recursive": boolean("Search folders recursively (default false)."),
            "formats": string("Formats to take from folders: obj, fbx, gltf separated by commas (default all)."),
            "template": string("Name template, default {name}. Also {folder} and {index}."),
            "prefix": string("Text put in front of every name."),
            "scale": number("Extra scale after the unit conversion, 0.001 to 1000 (default 1).", 0.001, 1000.0),
            "convert_units": boolean("Meters of the file become Source units (default true)."),
            "up": choice("Up axis of the files (default auto: from the file).", &["auto", "y", "z"]),
            "center": boolean("Put the origin at the bottom center of each model (default true)."),
            "kind": choice("Class of the props: static (prop_static, default) or physics (prop_physics).", &["static", "physics"]),
            "surface_prop": string("Physics and sound material (default default)."),
            "mass": number("Mass in kg of physics props, 0.1 to 50000 (default 20).", 0.1, 50000.0),
            "collision": choice("Collision model (default pieces).", &["pieces", "box", "none"]),
            "lod": boolean("Make levels of detail (default the saved lab.model_lod.enabled)."),
            "lod_levels": integer("Number of levels of detail, 3 to 6 (default the saved lab.model_lod.levels).", 3, 6),
            "lod_keep": number("Percent of the triangles each level keeps from the previous one, 20 to 90.", 20.0, 90.0),
            "keep_params": boolean("Write material parameters into the VMTs (default the saved lab.model_keep_params)."),
            "overwrite": boolean("Replace models that exist (default false: they are skipped)."),
            "threads": integer("Models converted at the same time, 1 to 16 (default 2 to 4 from the processor).", 1, 16),
            "place": boolean("Place one prop per converted model in a grid in front of the 3D camera."),
            "columns": integer("Columns of the grid (default 5).", 1, 256),
            "gap": number("Gap between props in units (default 64).", 0.0, 16384.0),
            "dry_run": boolean("Only read the files and return the preview table."),
        }, ["paths"];
}

fn format_arg(a: &Args<'_>) -> R<Option<BlenderFormat>> {
    Ok(a.opt_enum("format", &["gltf", "glb", "obj", "fbx"])?.and_then(|f| BlenderFormat::from_id(&f)))
}

fn send_to_blender(state: &mut AppState, a: &Args<'_>) -> R<ToolResult> {
    let opts = SendOptions {
        format: format_arg(a)?,
        launch: a.opt_bool("launch")?.unwrap_or(true),
        live: a.opt_bool("live_link")?,
    };
    let r = blender::send(state, &opts)?;
    let found = r.blender.is_some();
    Ok(ToolResult::json(json!({
        "file": r.file.display().to_string(),
        "format": r.format.id(),
        "linked": r.kind,
        "triangles": r.triangles,
        "materials": r.materials,
        "live_link": r.live,
        "blender_started": r.launched,
        "blender_already_open": r.reused,
        "blender": r.blender.map(|p| p.display().to_string()),
        "blender_found": found,
        "problem": r.problem,
        "next": "edit in Blender, then save (export on save) or press Update in Hammer Pro; Hammer Pro picks the change up by itself, or call blender_pull",
    })))
}

fn blender_status(state: &mut AppState, _a: &Args<'_>) -> R<ToolResult> {
    if !state.runtime.blender.exe_checked {
        blender::resolve_exe(state);
    }
    Ok(ToolResult::json(blender::status_json(state)))
}

fn report_json(r: pull::Report) -> Value {
    json!({
        "updated": r.kind,
        "message": r.text,
        "model": r.model,
        "solid_ids": r.solids,
        "prop_ids": r.props,
        "warnings": r.warnings,
    })
}

fn blender_pull(state: &mut AppState, _a: &Args<'_>) -> R<Outcome> {
    if state.runtime.blender.pull_in_progress {
        return Err("an update from Blender is already running".to_owned());
    }
    let job = pull::job_of(state)?;
    state.runtime.blender.pull_in_progress = true;
    Ok(Worker::spawn_then("blender_pull", move || {
        let answer = std::panic::catch_unwind(std::panic::AssertUnwindSafe(|| pull::compute(job)))
            .unwrap_or_else(|_| Err("internal error".to_owned()));
        let apply: ApplyFn = Box::new(move |state: &mut AppState| {
            state.runtime.blender.pull_in_progress = false;
            Some(match answer.and_then(|o| pull::apply(state, o)) {
                Ok(r) => ToolResult::json(report_json(r)),
                Err(e) => ToolResult::error(format!("blender_pull: {e}")),
            })
        });
        (ToolResult::error("blender_pull: the update was not applied"), Some(apply))
    }))
}

fn blender_setup(state: &mut AppState, a: &Args<'_>) -> R<ToolResult> {
    let mut changed: Vec<&str> = Vec::new();
    if let Some(path) = a.opt_str("path", 1024)? {
        let path = path.trim();
        if path.is_empty() {
            state.settings.blender.path.clear();
        } else {
            super::semantic::local_path(path)?;
            let exe = find::resolve(path).ok_or_else(|| {
                format!(
                    "{path} is not a Blender program: give blender.exe (a file named blender...) or the Blender folder"
                )
            })?;
            state.settings.blender.path = exe.display().to_string();
        }
        changed.push("path");
    }
    if let Some(f) = format_arg(a)? {
        state.settings.blender.format = f;
        changed.push("format");
    }
    if let Some(live) = a.opt_bool("live_link")? {
        state.settings.blender.live_link = live;
        changed.push("live_link");
    }
    if !changed.is_empty() {
        state.settings_dirty = true;
    }
    let mut extra = serde_json::Map::new();
    if a.opt_bool("detect")?.unwrap_or(false) || changed.contains(&"path") {
        let found = blender::resolve_exe(state);
        extra.insert("detected".to_owned(), json!(found.map(|p| p.display().to_string())));
    }
    if a.opt_bool("install_addon")?.unwrap_or(false) {
        let done = blender::install_addon(state)?;
        extra.insert(
            "addon_files".to_owned(),
            json!(done.files.iter().map(|p| p.display().to_string()).collect::<Vec<_>>()),
        );
        extra.insert("addon_in_blender_folder".to_owned(), json!(done.in_blender));
    }
    if a.opt_bool("stop_link")?.unwrap_or(false) {
        extra.insert("link_stopped".to_owned(), json!(blender::stop(state)));
    }
    let mut out = blender::status_json(state);
    out["changed"] = json!(changed);
    if let Some(map) = out.as_object_mut() {
        map.extend(extra);
    }
    Ok(ToolResult::json(out))
}

fn spec_from(state: &AppState, a: &Args<'_>) -> R<Spec> {
    let mut spec = Spec::from_settings(&state.settings);
    for p in a.strings("paths", 2000)? {
        spec.inputs.push(super::semantic::local_path(&p)?);
    }
    if spec.inputs.is_empty() {
        return Err("paths needs at least one file or folder".to_owned());
    }
    spec.recursive = a.opt_bool("recursive")?.unwrap_or(false);
    if let Some(f) = a.opt_str("formats", 64)? {
        spec.formats = Formats::parse(f)?;
    }
    let o = &mut spec.opts;
    if let Some(t) = a.opt_str("template", 128)? {
        o.template = t.to_owned();
    }
    if let Some(p) = a.opt_str("prefix", 64)? {
        o.prefix = p.to_owned();
    }
    if let Some(s) = a.opt_f64("scale", 0.001, 1000.0)? {
        o.convert.import.scale = s;
    }
    if let Some(b) = a.opt_bool("convert_units")? {
        o.convert.import.convert_units = b;
    }
    if let Some(u) = a.opt_enum("up", &["auto", "y", "z"])? {
        o.convert.import.up = parse_up(&u)?;
    }
    if let Some(b) = a.opt_bool("center")? {
        o.convert.import.center = b;
    }
    if let Some(k) = a.opt_enum("kind", &["static", "physics"])? {
        o.convert.static_prop = k == "static";
    }
    if let Some(s) = a.opt_str("surface_prop", 64)? {
        o.convert.surface_prop = s.trim().to_owned();
    }
    if let Some(m) = a.opt_f64("mass", 0.1, 50000.0)? {
        o.convert.mass = m;
    }
    if let Some(c) = a.opt_enum("collision", &["pieces", "box", "none"])? {
        o.convert.collision = parse_collision(&c)?;
    }
    if let Some(b) = a.opt_bool("lod")? {
        o.convert.lod.enabled = b;
    }
    if let Some(n) = a.opt_u64("lod_levels", 3, 6)? {
        o.convert.lod.levels = n as usize;
    }
    if let Some(pct) = a.opt_f64("lod_keep", 20.0, 90.0)? {
        o.convert.lod.reduction = pct / 100.0;
    }
    if let Some(b) = a.opt_bool("keep_params")? {
        o.convert.keep_params = b;
    }
    o.overwrite = a.opt_bool("overwrite")?.unwrap_or(false);
    if let Some(t) = a.opt_u64("threads", 1, 16)? {
        o.threads = clamp_threads(t as usize);
    }
    spec.dry_run = a.opt_bool("dry_run")?.unwrap_or(false);
    Ok(spec)
}

fn result_json(r: &FileResult) -> Value {
    json!({
        "file": r.path.display().to_string(),
        "name": r.name,
        "status": r.status.id(),
        "message": r.message,
        "model": r.model_path(),
        "triangles": r.converted.as_ref().map(|c| c.triangles),
    })
}

fn import_models_batch(state: &mut AppState, a: &Args<'_>) -> R<Outcome> {
    let spec = spec_from(state, a)?;
    let files = collect_files(&spec.inputs, spec.recursive, spec.formats);
    if files.is_empty() {
        return Err("no OBJ, FBX or glTF file found in paths".to_owned());
    }
    let root = content_root(state);
    if spec.dry_run {
        let opts = spec.opts.clone();
        return Ok(Worker::spawn("import_models_batch", move || {
            let entries =
                scan(&files, &opts, root.as_deref(), &AtomicBool::new(false), &std::sync::atomic::AtomicUsize::new(0));
            let table: Vec<Value> = entries
                .iter()
                .take(MAX_RESULTS)
                .map(|e| {
                    json!({
                        "file": e.path.display().to_string(),
                        "name": e.name,
                        "triangles": e.triangles,
                        "size": [e.size.x, e.size.y, e.size.z],
                        "materials": e.materials,
                        "exists": e.exists,
                        "warnings": e.warnings,
                        "error": e.error,
                    })
                })
                .collect();
            ToolResult::json(json!({ "dry_run": true, "files": entries.len(), "preview": table }))
        }));
    }
    let root = root.ok_or("the game content is not loaded (set the game folder in Settings > Game)")?;
    let place = a.opt_bool("place")?.unwrap_or(false);
    let grid = GridOptions {
        columns: a.opt_u64("columns", 1, 256)?.map_or(GridOptions::default().columns, |c| c as usize),
        gap: a.opt_f64("gap", 0.0, 16384.0)?.unwrap_or(GridOptions::default().gap),
    };
    let class = if spec.opts.convert.static_prop { "prop_static" } else { "prop_physics" };
    let names = names_for(&files, &spec.opts.template, &spec.opts.prefix);
    let opts = spec.opts.clone();
    Ok(Worker::spawn_then("import_models_batch", move || {
        let (results, summary) = run(&files, &names, &opts, &root, &AtomicBool::new(false), &|_| {});
        let items: Vec<(String, (glam::DVec3, glam::DVec3))> = results
            .iter()
            .filter(|r| matches!(r.status, Status::Ok | Status::Warning))
            .filter_map(|r| r.converted.as_ref().map(|c| (c.model.clone(), c.bounds)))
            .collect();
        let table: Vec<Value> = results.iter().take(MAX_RESULTS).map(result_json).collect();
        let mut body = json!({
            "summary": summary.text(),
            "files": summary.total,
            "ok": summary.ok,
            "warnings": summary.warnings,
            "errors": summary.errors,
            "skipped": summary.skipped,
            "not_run": summary.cancelled,
            "milliseconds": summary.millis,
            "results": table,
            "truncated": results.len() > MAX_RESULTS,
        });
        let converted = summary.converted();
        let apply: ApplyFn = Box::new(move |state: &mut AppState| {
            if converted > 0 {
                hp_ui_entities::model_import::remount_after_write(state);
            }
            if place && !items.is_empty() {
                match crate::batch::place_grid(state, &items, class, grid) {
                    Ok(n) => body["placed_props"] = json!(n),
                    Err(e) => body["place_error"] = json!(e),
                }
            }
            Some(ToolResult::json(body))
        });
        (ToolResult::error("import_models_batch: not applied"), Some(apply))
    }))
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::dispatch::mcp::tools::run as run_tool;

    fn call(state: &mut AppState, name: &str, v: Value) -> ToolResult {
        match run_tool(state, name, &v) {
            Outcome::Done(r) => r,
            Outcome::Worker(w) => w.wait(state, std::time::Duration::from_secs(60)),
            other => panic!("unexpected {other:?}"),
        }
    }

    #[test]
    fn the_setup_tool_checks_the_program_and_reports_the_state() {
        let mut state = hp_app::testing::test_state();
        let work = std::env::temp_dir().join(format!("hp_blender_mcp_{}", std::process::id()));
        std::fs::create_dir_all(&work).unwrap();
        state.runtime.blender.work_dir = Some(work.join("work"));
        let bad = call(&mut state, "blender_setup", json!({ "path": work.join("notepad.exe").display().to_string() }));
        assert!(bad.is_error);
        let exe = work.join(find::exe_names()[0]);
        std::fs::write(&exe, "x").unwrap();
        let ok = call(
            &mut state,
            "blender_setup",
            json!({ "path": exe.display().to_string(), "format": "fbx", "live_link": false }),
        );
        assert!(!ok.is_error, "{}", ok.first_text());
        assert_eq!(state.settings.blender.format, BlenderFormat::Fbx);
        assert!(!state.settings.blender.live_link);
        assert_eq!(state.settings.blender.path, exe.display().to_string());
        let v: Value = serde_json::from_str(ok.first_text()).unwrap();
        assert_eq!(v["blender_found"], true);
        let status = call(&mut state, "blender_status", json!({}));
        assert!(!status.is_error);
        let pull = call(&mut state, "blender_pull", json!({}));
        assert!(pull.is_error, "no link yet");
        let install = call(&mut state, "blender_setup", json!({ "install_addon": true }));
        assert!(!install.is_error, "{}", install.first_text());
        let _ = std::fs::remove_dir_all(&work);
    }

    #[test]
    fn a_batch_converts_through_the_tool() {
        let mut state = hp_app::testing::test_state();
        let dir = std::env::temp_dir().join(format!("hp_batch_mcp_{}", std::process::id()));
        let _ = std::fs::remove_dir_all(&dir);
        std::fs::create_dir_all(&dir).unwrap();
        std::fs::write(
            dir.join("t.obj"),
            "v 0 0 0\nv 10 0 0\nv 0 10 0\nv 0 0 10\nf 1 2 3\nf 1 2 4\nf 1 3 4\nf 2 3 4\n",
        )
        .unwrap();
        let dry =
            call(&mut state, "import_models_batch", json!({ "paths": [dir.display().to_string()], "dry_run": true }));
        assert!(!dry.is_error, "{}", dry.first_text());
        let v: Value = serde_json::from_str(dry.first_text()).unwrap();
        assert_eq!(v["files"], 1);
        assert_eq!(v["preview"][0]["triangles"], 4);
        let none =
            call(&mut state, "import_models_batch", json!({ "paths": [dir.join("missing").display().to_string()] }));
        assert!(none.is_error);
        let v = json!({ "paths": [dir.display().to_string()], "lod_levels": 5, "lod_keep": 40 });
        let a = super::super::tools::args(&v, &["paths", "lod_levels", "lod_keep"]).unwrap();
        let spec = spec_from(&state, &a).unwrap();
        assert_eq!(spec.opts.convert.lod.levels, 5);
        assert!((spec.opts.convert.lod.reduction - 0.4).abs() < 1e-9);
        let bad =
            call(&mut state, "import_models_batch", json!({ "paths": [dir.display().to_string()], "lod_levels": 9 }));
        assert!(bad.is_error);
        let _ = std::fs::remove_dir_all(&dir);
    }
}
