use std::path::{Path, PathBuf};

use hp_app::AppState;
use hp_mcp::protocol::ToolResult;
use serde_json::json;

use super::semantic::*;
use super::tools::{Args, Outcome, R, Worker};

const MAX_PATH: usize = 1024;
const MAX_EXTRA: usize = 64;
const MAX_NOTES: usize = 200;

domain! { "entity definitions";
    get_fgd_info: Read, "Entity definitions",
        "The FGD files loaded (the game's, the Hammer++ one when installed, then the extra files), the override, \
        how many classes they define (point and brush), @mapsize, material exclusions and auto visgroups, files \
        that could not be read and the problems found inside them.",
        { "notes": integer("Most problem notes returned (default 50).", 0, MAX_NOTES as i64) }, [];
    add_fgd: Editor, "Add an FGD file",
        "Add an extra FGD file (a gamemode's, Mapbase, a custom one) after the others and load the definitions \
        again. Saved in the settings: set confirm to true when the user asked for it.",
        {
            "path": string("Absolute path of the .fgd file."),
            "confirm": boolean("The user asked for this change of the settings."),
        }, ["path", "confirm"];
    remove_fgd: Editor, "Remove an FGD file",
        "Remove an extra FGD file from the settings and load the definitions again. Set confirm to true when the \
        user asked for it.",
        {
            "path": string("Path of the extra .fgd file, as get_fgd_info lists it."),
            "confirm": boolean("The user asked for this change of the settings."),
        }, ["path", "confirm"];
    reload_fgd: Editor, "Reload the entity definitions",
        "Read the FGD files again (after editing one). Files that change on disk are also reloaded on their own \
        within a few seconds.",
        {}, [];
    validate_fgd: Read, "Check an FGD file",
        "Parse an FGD file without loading it in the editor: the classes it defines, its @include files, and every \
        problem found (file:line: message). Runs on a worker thread.",
        {
            "path": string("Absolute path of the .fgd file."),
            "notes": integer("Most problem notes returned (default 100).", 0, MAX_NOTES as i64),
        }, ["path"];
}

fn fgd_path(a: &Args<'_>, key: &str) -> R<PathBuf> {
    let raw = a.str(key, MAX_PATH)?.trim();
    if raw.chars().any(char::is_control) {
        return Err(format!("{key} holds control characters"));
    }
    let path = super::semantic::local_path(raw)?;
    if !path.is_absolute() {
        return Err(format!("{key} must be an absolute path"));
    }
    if !path.extension().is_some_and(|e| e.eq_ignore_ascii_case("fgd")) {
        return Err(format!("{key} must name a .fgd file"));
    }
    Ok(path)
}

fn same_file(a: &Path, b: &Path) -> bool {
    match (std::fs::canonicalize(a), std::fs::canonicalize(b)) {
        (Ok(x), Ok(y)) => x == y,
        _ => a == b,
    }
}

fn paths_json(paths: &[PathBuf]) -> Vec<String> {
    paths.iter().map(|p| p.display().to_string()).collect()
}

fn get_fgd_info(state: &mut AppState, a: &Args<'_>) -> R<ToolResult> {
    let notes = a.opt_u64("notes", 0, MAX_NOTES as u64)?.unwrap_or(50) as usize;
    let report = state.assets.fgd_report();
    let game = &state.settings.game;
    let classes = state.assets.fgd().map(|f| {
        json!({
            "total": f.len(),
            "point": f.point_classes().len(),
            "brush": f.solid_classes().len(),
        })
    });
    Ok(ToolResult::json(json!({
        "loading": report.loading,
        "files": paths_json(&report.files),
        "override": game.fgd_override.as_ref().map(|p| p.display().to_string()),
        "extra_files": paths_json(&game.extra_fgds),
        "classes": classes,
        "unreadable": report.warnings,
        "notes_total": report.notes.len(),
        "notes": report.notes.iter().take(notes).collect::<Vec<_>>(),
    })))
}

fn reload(state: &mut AppState) {
    state.assets.reload_fgd(&state.settings);
    state.settings_dirty = true;
}

fn add_fgd(state: &mut AppState, a: &Args<'_>) -> R<ToolResult> {
    if a.opt_bool("confirm")? != Some(true) {
        return Err(
            "adding an FGD file changes the settings: set confirm to true when the user asked for it".to_owned()
        );
    }
    let path = fgd_path(a, "path")?;
    if !path.is_file() {
        return Err(format!("no file {}", path.display()));
    }
    let extra = &mut state.settings.game.extra_fgds;
    if extra.iter().any(|p| same_file(p, &path)) {
        return Ok(ToolResult::json(json!({ "added": false, "note": "already in the list" })));
    }
    if extra.len() >= MAX_EXTRA {
        return Err(format!("at most {MAX_EXTRA} extra FGD files"));
    }
    extra.push(path.clone());
    reload(state);
    Ok(ToolResult::json(json!({
        "added": path.display().to_string(),
        "note": "Loading: get_fgd_info shows the result when loading is false.",
    })))
}

fn remove_fgd(state: &mut AppState, a: &Args<'_>) -> R<ToolResult> {
    if a.opt_bool("confirm")? != Some(true) {
        return Err(
            "removing an FGD file changes the settings: set confirm to true when the user asked for it".to_owned()
        );
    }
    let raw = a.str("path", MAX_PATH)?.trim();
    let path = PathBuf::from(raw);
    let extra = &mut state.settings.game.extra_fgds;
    let before = extra.len();
    extra.retain(|p| !(p == &path || same_file(p, &path)));
    if extra.len() == before {
        return Err(format!("{raw} is not an extra FGD file (get_fgd_info lists them)"));
    }
    reload(state);
    Ok(ToolResult::json(json!({ "removed": raw })))
}

fn reload_fgd(state: &mut AppState, _a: &Args<'_>) -> R<ToolResult> {
    state.assets.reload_fgd(&state.settings);
    Ok(ToolResult::json(json!({ "reloading": true })))
}

fn validate_fgd(_state: &mut AppState, a: &Args<'_>) -> R<Outcome> {
    let path = fgd_path(a, "path")?;
    let notes = a.opt_u64("notes", 0, MAX_NOTES as u64)?.unwrap_or(100) as usize;
    Ok(Worker::spawn("validate_fgd", move || {
        let fgd = match hp_fgd::Fgd::load(&path) {
            Ok(f) => f,
            Err(e) => return ToolResult::error(format!("validate_fgd: {e}")),
        };
        let mut all = fgd.warnings.clone();
        let resolved = fgd.resolve();
        all.extend(resolved.warnings.iter().cloned());
        ToolResult::json(json!({
            "file": path.display().to_string(),
            "classes": resolved.len(),
            "point_classes": resolved.point_classes().len(),
            "brush_classes": resolved.solid_classes().len(),
            "notes_total": all.len(),
            "notes": all.iter().take(notes).collect::<Vec<_>>(),
            "ok": all.is_empty(),
        }))
    }))
}

#[cfg(test)]
mod tests {
    use serde_json::json;

    use super::super::semantic_tests::{err, ok};

    #[test]
    fn fgd_files() {
        let mut s = hp_app::testing::test_state();
        let dir = std::env::temp_dir().join(format!("hp_mcp_fgd_{}", std::process::id()));
        std::fs::create_dir_all(&dir).unwrap();
        let file = dir.join("custom.fgd");
        std::fs::write(&file, "@PointClass = my_entity : \"Mine\" [ health(integer) : \"Health\" : 10 ]\n@Broken")
            .unwrap();
        let path = file.display().to_string();
        let info = ok(&mut s, "get_fgd_info", json!({}));
        assert!(info["extra_files"].as_array().unwrap().is_empty());
        err(&mut s, "add_fgd", json!({ "path": path, "confirm": false }));
        err(&mut s, "add_fgd", json!({ "path": "custom.fgd", "confirm": true }));
        err(&mut s, "add_fgd", json!({ "path": dir.join("a.txt").display().to_string(), "confirm": true }));
        err(&mut s, "add_fgd", json!({ "path": dir.join("missing.fgd").display().to_string(), "confirm": true }));
        ok(&mut s, "add_fgd", json!({ "path": path, "confirm": true }));
        assert_eq!(s.settings.game.extra_fgds.len(), 1);
        assert_eq!(ok(&mut s, "add_fgd", json!({ "path": path, "confirm": true }))["added"], false);
        let v = ok(&mut s, "validate_fgd", json!({ "path": path }));
        assert_eq!(v["classes"], 1);
        assert_eq!(v["ok"], false, "the broken line is reported: {v}");
        ok(&mut s, "reload_fgd", json!({}));
        err(&mut s, "remove_fgd", json!({ "path": "C:/nothing.fgd", "confirm": true }));
        ok(&mut s, "remove_fgd", json!({ "path": path, "confirm": true }));
        assert!(s.settings.game.extra_fgds.is_empty());
        let _ = std::fs::remove_dir_all(&dir);
    }
}
