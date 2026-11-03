use std::path::{Path, PathBuf};

use hp_app::AppState;
use hp_app::runtime::TabRequest;
use hp_mcp::protocol::ToolResult;
use serde_json::{Value, json};

use super::semantic::*;
use super::tools::{AppFn, ApplyFn, Args, MAX_VALUE, Outcome, R, Worker, check_key, check_value, edit, obj_json};

const MAX_IMPORT_BYTES: u64 = 256 * 1024 * 1024;

domain! { "documents";
    list_tabs: Read, "Open maps",
        "The maps open in this window (tabs): id, title, file, unsaved changes, and the one shown.",
        {}, [];
    switch_tab: Editor, "Show a map",
        "Show another open map (tab id from list_tabs); the tools then work on it.",
        { "id": integer("Tab id.", 0, i64::MAX) }, ["id"];
    close_tab: Editor, "Close a map",
        "Close an open map that has no unsaved changes (save it first), or every other map (others: true).",
        { "id": integer("Tab id.", 0, i64::MAX), "others": boolean("Close every other saved map instead.") }, ["id"];
    new_map: Editor, "New map",
        "Open a new empty map in a new tab.",
        {}, [];
    list_recent_maps: Read, "Recent maps",
        "The maps opened recently (most recent first) and whether the files still exist.",
        {}, [];
    import_vmf: Edit, "Import a VMF",
        "Merge another VMF (in the map folder) into the map: its solids, entities and groups get new ids, its \
        visgroups are merged by name. The imported objects are selected.",
        { "file": string("VMF file, relative to the map folder.") }, ["file"];
    export_objects: Editor, "Export objects to a VMF",
        "Write objects (default: the selection) to a new VMF in the map folder (Export selection).",
        {
            "ids": target_ids(), "solid_ids": target_solid_ids(),
            "file": string("New VMF file, relative to the map folder."),
            "overwrite": boolean("Replace an existing file (default false)."),
        }, ["file"];
    export_obj: Editor, "Export OBJ",
        "Write the visible brush and displacement geometry with texture coordinates to an OBJ (and MTL) file in the \
        map folder, for 3D tools.",
        { "file": string("OBJ file, relative to the map folder."), "overwrite": boolean("Replace existing files (default false).") }, ["file"];
    decompile_bsp: Editor, "BSP Decompiler",
        "BSP Decompiler: decompile a compiled map (.bsp in the map folder) into a new map tab (brushes, entities, \
        displacements, overlays), on a worker thread; list_tabs shows it when done. To recover the map of a Garry's \
        Mod server you join, use the BSP Sniffer tools (sniffer_start).",
        { "file": string("BSP file, relative to the map folder.") }, ["file"];
    nav_mesh: Editor, "Navigation mesh",
        "The navigation mesh shown with the map (file, areas, isolated areas), or load a .nav file of the map folder.",
        { "load": string("A .nav file to show, relative to the map folder.") }, [];
    get_map_properties: Read, "Map properties",
        "The worldspawn keyvalues (skyname, detail material, light settings...), the map file, its format version \
        and the editor version that saved it, and compiler_keys: the worldspawn keys Hammer Pro's lighting compiler \
        reads (_ao, _aoradius, _aostrength, _aosamples: ambient occlusion; _lightsrad: the map's texture light \
        file) with their value, default and meaning.",
        {}, [];
    set_map_properties: Edit, "Set map properties",
        "Set or remove worldspawn keyvalues (skyname, detailmaterial, detailvbsp, maxpropscreenwidth...). A value of \
        \"\" removes the key. Lighting compiler keys: _ao (1 on, 0 off, \"\" the compile decides: on for Normal and \
        Final), _aoradius (units, default 96), _aostrength (0 to 1, default 0.75), _aosamples (rays, default 32, \
        64 with Final), _lightsrad (a texture light file read like -lights: relative to the game folder or the map \
        folder, or a full path). The -ao, -noao, -aoradius, -aostrength and -aosamples compile options win over \
        the keys.",
        { "keyvalues": keyvalues("Keys and values.") }, ["keyvalues"];
}

fn list_tabs(state: &mut AppState, _a: &Args<'_>) -> R<ToolResult> {
    let w = &state.runtime.workflow;
    let tabs: Vec<Value> = w
        .tabs
        .iter()
        .map(|t| {
            json!({
                "id": t.id,
                "title": t.title,
                "file": t.path.as_ref().map(|p| p.display().to_string()),
                "unsaved": t.dirty,
                "shown": t.id == w.active_tab,
            })
        })
        .collect();
    Ok(ToolResult::json(json!({ "tabs": tabs, "shown": w.active_tab })))
}

fn tab_id(state: &AppState, a: &Args<'_>) -> R<u64> {
    let id = a.opt_u64("id", 0, u64::MAX)?.ok_or("id is required")?;
    if !state.runtime.workflow.tabs.iter().any(|t| t.id == id) {
        return Err(format!("no open map has the tab id {id} (list_tabs)"));
    }
    Ok(id)
}

fn switch_tab(state: &mut AppState, a: &Args<'_>) -> R<ToolResult> {
    let id = tab_id(state, a)?;
    if state.runtime.workflow.tab_request.is_some() {
        return Err("another tab change is pending; try again".to_owned());
    }
    state.runtime.workflow.tab_request = Some(TabRequest::Switch(id));
    Ok(ToolResult::json(json!({ "switching_to": id, "note": "The map is shown from the next frame on." })))
}

fn close_tab(state: &mut AppState, a: &Args<'_>) -> R<ToolResult> {
    let id = tab_id(state, a)?;
    let others = a.opt_bool("others")?.unwrap_or(false);
    let tabs = &state.runtime.workflow.tabs;
    let dirty: Vec<&str> = tabs
        .iter()
        .filter(|t| if others { t.id != id } else { t.id == id })
        .filter(|t| t.dirty)
        .map(|t| t.title.as_str())
        .collect();
    if !dirty.is_empty() {
        return Err(format!("these maps have unsaved changes (save them first): {}", dirty.join(", ")));
    }
    if state.runtime.workflow.tab_request.is_some() {
        return Err("another tab change is pending; try again".to_owned());
    }
    state.runtime.workflow.tab_request = Some(if others { TabRequest::CloseOthers(id) } else { TabRequest::Close(id) });
    Ok(ToolResult::json(json!({ "closing": if others { "others" } else { "this" }, "id": id })))
}

fn new_map(_state: &mut AppState, _a: &Args<'_>) -> R<Outcome> {
    Ok(Outcome::AppFn(AppFn(Box::new(|app, ctx| {
        app.execute(ctx, hp_app::Action::NewMap);
        ToolResult::json(json!({ "requested": true, "note": "list_tabs shows the new map." }))
    }))))
}

fn list_recent_maps(state: &mut AppState, _a: &Args<'_>) -> R<ToolResult> {
    let list: Vec<Value> = state
        .settings
        .recent_files
        .iter()
        .map(|p| json!({ "file": p.display().to_string(), "exists": p.is_file() }))
        .collect();
    Ok(ToolResult::json(json!({ "recent": list })))
}

fn map_file(state: &AppState, a: &Args<'_>, ext: &str) -> R<(String, PathBuf)> {
    let rel = relative_file(a.str("file", 255)?, ext)?;
    let path = map_dir(state)?.join(&rel);
    Ok((rel, path))
}

fn import_vmf(state: &mut AppState, a: &Args<'_>) -> R<Outcome> {
    let (rel, path) = map_file(state, a, "vmf")?;
    let meta = std::fs::metadata(&path).map_err(|_| format!("no file {rel} in the map folder"))?;
    if meta.len() > MAX_IMPORT_BYTES {
        return Err(format!("{rel} is larger than 256 MB"));
    }
    if state.doc.path().is_some_and(|p| same_file(p, &path)) {
        return Err("a map cannot import itself (duplicate_objects copies objects)".to_owned());
    }
    Ok(Worker::spawn_then("import_vmf", move || match hp_vmf::load(&path) {
        Err(e) => (ToolResult::error(format!("import_vmf: {rel}: {e}")), None),
        Ok(map) => {
            let apply: ApplyFn = Box::new(move |state: &mut AppState| {
                let r = edit(state, "import_vmf", |doc| doc.import_map(map));
                Some(match r {
                    Ok(objs) => ToolResult::json(json!({
                        "imported": objs.len(),
                        "objects": objs.iter().take(200).map(|o| obj_json(*o)).collect::<Vec<_>>(),
                    })),
                    Err(e) => ToolResult::error(format!("import_vmf: {e}")),
                })
            });
            (ToolResult::error("import_vmf: not applied"), Some(apply))
        }
    }))
}

fn same_file(a: &Path, b: &Path) -> bool {
    match (std::fs::canonicalize(a), std::fs::canonicalize(b)) {
        (Ok(x), Ok(y)) => x == y,
        _ => a == b,
    }
}

fn export_objects(state: &mut AppState, a: &Args<'_>) -> R<Outcome> {
    let objs = targets(state, a)?;
    let (rel, path) = map_file(state, a, "vmf")?;
    let overwrite = a.opt_bool("overwrite")?.unwrap_or(false);
    if state.doc.path().is_some_and(|p| same_file(p, &path)) {
        return Err("export_objects cannot replace the map itself (save_map saves it)".to_owned());
    }
    if path.exists() && !overwrite {
        return Err(format!("{rel} exists already (overwrite: true replaces it)"));
    }
    let map = state.doc.instance_map(&objs, glam::DVec3::ZERO, None).map_err(|e| e.to_string())?;
    let count = objs.len();
    Ok(Worker::spawn("export_objects", move || {
        if let Some(dir) = path.parent()
            && let Err(e) = std::fs::create_dir_all(dir)
        {
            return ToolResult::error(format!("export_objects: cannot create the folder: {e}"));
        }
        match crate::export::write_vmf(&map, &path) {
            Ok(()) => ToolResult::json(json!({ "written": path.display().to_string(), "objects": count })),
            Err(e) => ToolResult::error(format!("export_objects: {e}")),
        }
    }))
}

fn export_obj(state: &mut AppState, a: &Args<'_>) -> R<Outcome> {
    let (rel, path) = map_file(state, a, "obj")?;
    let overwrite = a.opt_bool("overwrite")?.unwrap_or(false);
    if !overwrite && (path.exists() || path.with_extension("mtl").exists()) {
        return Err(format!("{rel} (or its .mtl) exists already (overwrite: true replaces it)"));
    }
    if state.doc.scene().solids().is_empty() {
        return Err("the map has no brushes to export".to_owned());
    }
    let input = crate::export::ObjInput::from_state(state);
    Ok(Worker::spawn("export_obj", move || match crate::export::export_obj(&input, &path) {
        Ok(stats) => ToolResult::json(json!({
            "written": path.display().to_string(),
            "faces": stats.faces,
            "triangles": stats.triangles,
        })),
        Err(e) => ToolResult::error(format!("export_obj: {e}")),
    }))
}

fn decompile_bsp(state: &mut AppState, a: &Args<'_>) -> R<ToolResult> {
    let (rel, path) = map_file(state, a, "bsp")?;
    if !path.is_file() {
        return Err(format!("no file {rel} in the map folder"));
    }
    if state.runtime.lab.decompiling.is_some() || state.runtime.lab.open_bsp.is_some() {
        return Err("a map is being decompiled already; try again when it is open".to_owned());
    }
    state.runtime.lab.open_bsp = Some(path);
    Ok(ToolResult::json(json!({ "decompiling": rel, "note": "list_tabs shows the decompiled map when done." })))
}

fn nav_mesh(state: &mut AppState, a: &Args<'_>) -> R<ToolResult> {
    if let Some(f) = a.opt_str("load", 255)? {
        let rel = relative_file(f, "nav")?;
        let path = map_dir(state)?.join(&rel);
        if !path.is_file() {
            return Err(format!("no file {rel} in the map folder"));
        }
        state.runtime.lab.nav_file = Some(path);
    }
    let lab = &state.runtime.lab;
    Ok(ToolResult::json(json!({
        "shown": lab.nav_summary.as_ref().map(|(p, areas, isolated)| json!({
            "file": p.display().to_string(), "areas": areas, "isolated_areas": isolated,
        })),
        "chosen_file": lab.nav_file.as_ref().map(|p| p.display().to_string()),
        "note": "A mesh being loaded shows up in a moment.",
    })))
}

fn get_map_properties(state: &mut AppState, _a: &Args<'_>) -> R<ToolResult> {
    let scene = state.doc.scene();
    let world = scene.world();
    let kv: serde_json::Map<String, Value> =
        world.props.iter().filter(|(k, _)| k != "id").map(|(k, v)| (k.clone(), Value::String(v.clone()))).collect();
    let vi = scene.version_info();
    let compiler_keys: Vec<Value> = hp_ui_entities::compiler_world_props()
        .iter()
        .map(|p| {
            json!({
                "key": p.name,
                "name": p.display_name,
                "value": world.get(&p.name),
                "default": p.default,
                "choices": p.choices.iter().map(|c| json!({ "value": c.value, "name": c.display })).collect::<Vec<_>>(),
                "help": p.description,
            })
        })
        .collect();
    Ok(ToolResult::json(json!({
        "file": state.doc.path().map(|p| p.display().to_string()),
        "keyvalues": kv,
        "compiler_keys": compiler_keys,
        "format_version": vi.format_version,
        "editor_version": vi.editor_version,
        "editor_build": vi.editor_build,
        "map_version": vi.map_version,
    })))
}

fn set_map_properties(state: &mut AppState, a: &Args<'_>) -> R<ToolResult> {
    let kv = a.keyvalues("keyvalues")?;
    if kv.is_empty() {
        return Err("keyvalues is empty".to_owned());
    }
    for (k, v) in &kv {
        check_key(k)?;
        check_value(k, v)?;
        if k.eq_ignore_ascii_case("classname") {
            return Err("the worldspawn class cannot change".to_owned());
        }
        if v.len() > MAX_VALUE {
            return Err(format!("the value of {k} is too long"));
        }
    }
    edit(state, "set_map_properties", |doc| {
        for (k, v) in &kv {
            if v.is_empty() {
                doc.remove_world_key(k)?;
            } else {
                doc.set_world_key(k, v)?;
            }
        }
        Ok(())
    })?;
    get_map_properties(state, a)
}

#[cfg(test)]
mod tests {
    use serde_json::json;

    use super::super::semantic_tests::{err, needs_app, ok};

    #[test]
    fn documents_and_properties() {
        let mut s = hp_app::testing::test_state();
        let t = ok(&mut s, "list_tabs", json!({}));
        assert!(t["tabs"].is_array());
        err(&mut s, "switch_tab", json!({ "id": 99 }));
        err(&mut s, "close_tab", json!({ "id": 99 }));
        assert!(needs_app(&mut s, "new_map", json!({})));
        ok(&mut s, "list_recent_maps", json!({}));
        ok(&mut s, "set_map_properties", json!({ "keyvalues": { "skyname": "sky_borealis01", "detailmaterial": "" } }));
        let p = ok(&mut s, "get_map_properties", json!({}));
        assert_eq!(p["keyvalues"]["skyname"], "sky_borealis01");
        assert!(p["keyvalues"].get("detailmaterial").is_none());
        assert_eq!(s.doc.undo_label(), Some("MCP: set_map_properties"));
        err(&mut s, "set_map_properties", json!({ "keyvalues": { "classname": "x" } }));
        err(&mut s, "export_objects", json!({ "solid_ids": [2], "file": "part" }));
        err(&mut s, "import_vmf", json!({ "file": "part" }));
        err(&mut s, "decompile_bsp", json!({ "file": "x" }));
        let n = ok(&mut s, "nav_mesh", json!({}));
        assert!(n["shown"].is_null());
        err(&mut s, "nav_mesh", json!({ "load": "x.nav" }));
    }

    #[test]
    fn export_then_import() {
        let dir = std::env::temp_dir().join(format!("hp_mcp_docs_{}", std::process::id()));
        std::fs::create_dir_all(&dir).unwrap();
        let mut s = hp_app::testing::test_state();
        s.doc.set_path(Some(dir.join("main.vmf")));
        ok(&mut s, "export_objects", json!({ "solid_ids": [3], "ids": [5], "file": "parts/wall" }));
        assert!(dir.join("parts/wall.vmf").is_file());
        err(&mut s, "export_objects", json!({ "solid_ids": [3], "file": "parts/wall" }));
        err(&mut s, "export_objects", json!({ "solid_ids": [3], "file": "main", "overwrite": true }));
        let n = s.doc.stats().solids;
        let r = ok(&mut s, "import_vmf", json!({ "file": "parts/wall.vmf" }));
        assert_eq!(r["imported"], 2);
        assert_eq!(s.doc.stats().solids, n + 1);
        let r = ok(&mut s, "export_obj", json!({ "file": "mesh" }));
        assert!(r["triangles"].as_u64().unwrap() > 0);
        assert!(dir.join("mesh.obj").is_file());
        err(&mut s, "export_obj", json!({ "file": "mesh" }));
        std::fs::write(dir.join("main.bsp"), b"VBSP").unwrap();
        ok(&mut s, "decompile_bsp", json!({ "file": "main" }));
        assert!(s.runtime.lab.open_bsp.is_some());
        err(&mut s, "decompile_bsp", json!({ "file": "main" }));
        std::fs::write(dir.join("main.nav"), b"x").unwrap();
        ok(&mut s, "nav_mesh", json!({ "load": "main" }));
        assert!(s.runtime.lab.nav_file.is_some());
        let _ = std::fs::remove_dir_all(&dir);
    }
}
