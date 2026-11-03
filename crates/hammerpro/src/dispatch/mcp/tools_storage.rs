use hp_app::AppState;
use hp_app::i18n::{Lang, tr_in};
use hp_app::storage::{
    AppDirs, DeleteSummary, Entry, Progress, Report, Risk, TABLE, delete_rows, find_spec, format_bytes_in, scan_ids,
    scan_report,
};
use hp_mcp::protocol::ToolResult;
use serde_json::{Value, json};

use super::semantic::*;
use super::tools::{Args, R, Worker};

const MAX_ITEMS: usize = 32;

domain! { "storage";
    get_storage_usage: Read, "Disk usage of Hammer Pro",
        "How much disk space Hammer Pro itself uses and what uses the most, scanned on a background thread (the \
        answer comes when the scan is done). Returns total_bytes, total_files and items sorted by size, \
        largest first: id (use it with delete_storage), name, description, risk (safe: caches and temporary \
        files rebuilt on demand; recovery: autosaves and crash reports; user_data: settings, prefabs, sprinkle \
        presets and per map state), bytes, human size, files, percent of the total, paths, in_use_items (kept \
        while Hammer Pro uses them) and freeable_bytes. Only Hammer Pro's own settings, cache, autosave, log, \
        crash and temporary folders are listed: maps and game folders are never part of it.",
        {}, [];
    delete_storage: Destructive, "Delete Hammer Pro files",
        "Delete the files of storage items listed by get_storage_usage, to free disk space. Nothing is deleted \
        unless confirm is true: without it the tool deletes nothing and only reports what would be deleted and \
        how much space it frees. Items of risk user_data (settings, prefabs, sprinkle presets, per map state) \
        also need allow_user_data true. Files in use are skipped and errors are reported per item. Compile \
        related items are refused while a compile runs. The folders of Hammer Pro are emptied, maps and game \
        folders are never touched, and the result cannot be undone.",
        {
            "ids": strings("Item ids from get_storage_usage, for example thumbnails or builds.", MAX_ITEMS),
            "confirm": boolean("Must be true to really delete. Default false: report only."),
            "allow_user_data": boolean("Also allow items of risk user_data. Default false."),
        }, ["ids"];
}

fn en_bytes(n: u64) -> String {
    format_bytes_in(n, Lang::En)
}

fn name_of(key: &str) -> &'static str {
    tr_in(key, Lang::En)
}

fn entry_json(e: &Entry, total: u64) -> Value {
    let percent = if total == 0 { 0.0 } else { (e.bytes as f64 / total as f64 * 1000.0).round() / 10.0 };
    json!({
        "id": e.id,
        "name": name_of(e.name_key),
        "description": name_of(e.desc_key),
        "risk": e.risk.id(),
        "bytes": e.bytes,
        "human": en_bytes(e.bytes),
        "files": e.files,
        "percent": percent,
        "paths": e.paths.iter().map(|p| p.display().to_string()).collect::<Vec<_>>(),
        "in_use_items": e.in_use,
        "freeable_bytes": e.freeable(),
    })
}

fn report_json(r: &Report) -> Value {
    json!({
        "total_bytes": r.total_bytes,
        "total_human": en_bytes(r.total_bytes),
        "total_files": r.total_files,
        "scan_ms": r.millis,
        "items": r.entries.iter().map(|e| entry_json(e, r.total_bytes)).collect::<Vec<_>>(),
    })
}

fn summary_json(s: &DeleteSummary) -> Value {
    json!({
        "freed_bytes": s.freed,
        "freed_human": en_bytes(s.freed),
        "deleted_items": s.deleted,
        "items": s.outcomes.iter().map(|o| json!({
            "id": o.id,
            "deleted": o.deleted,
            "freed_bytes": o.freed,
            "skipped": o.skipped,
            "errors": o.errors.iter().map(|e| json!({ "path": e.path.display().to_string(), "error": e.error })).collect::<Vec<_>>(),
        })).collect::<Vec<_>>(),
    })
}

fn get_storage_usage(state: &mut AppState, _a: &Args<'_>) -> R<super::tools::Outcome> {
    let dirs: AppDirs = state.runtime.storage.dirs();
    Ok(Worker::spawn_then("get_storage_usage", move || {
        let report = scan_report(&dirs, &Progress::default());
        let result = ToolResult::json(report_json(&report));
        let apply: super::tools::ApplyFn = Box::new(move |s: &mut AppState| {
            s.runtime.storage.report = Some(report);
            None
        });
        (result, Some(apply))
    }))
}

fn delete_storage(state: &mut AppState, a: &Args<'_>) -> R<super::tools::Outcome> {
    let list = a.get("ids").and_then(Value::as_array).ok_or("ids must be a list of item ids")?;
    if list.is_empty() || list.len() > MAX_ITEMS {
        return Err(format!("ids must hold 1 to {MAX_ITEMS} item ids"));
    }
    let mut ids: Vec<String> = Vec::new();
    for v in list {
        let id = v.as_str().ok_or("ids must be strings")?;
        let spec = find_spec(id).ok_or_else(|| {
            format!("unknown item \"{id}\" (ids: {})", TABLE.iter().map(|s| s.id).collect::<Vec<_>>().join(", "))
        })?;
        if !ids.iter().any(|i| i == id) {
            ids.push(id.to_owned());
        }
        if spec.risk == Risk::UserData && !a.opt_bool("allow_user_data")?.unwrap_or(false) {
            return Err(format!("\"{id}\" holds your own data: pass allow_user_data true as well as confirm true"));
        }
        let compiling = state.runtime.compile.progress.is_some() || state.runtime.compile.background.is_some();
        if spec.uses_compile && compiling {
            return Err(format!("\"{id}\" is used by the running compile: wait for it to finish"));
        }
    }
    let confirm = a.opt_bool("confirm")?.unwrap_or(false);
    if state.runtime.storage.deleting() {
        return Err("a cleanup is already running in the Settings window".to_owned());
    }
    let dirs = state.runtime.storage.dirs();
    if !confirm {
        return Ok(Worker::spawn("delete_storage", move || {
            let names: Vec<&str> = ids.iter().map(String::as_str).collect();
            let report = scan_ids(&dirs, &names, &Progress::default());
            let freed: u64 = report.entries.iter().map(Entry::freeable).sum();
            let listing: Vec<String> = report
                .entries
                .iter()
                .map(|e| {
                    format!(
                        "{} ({}, {})",
                        e.id,
                        en_bytes(e.freeable()),
                        e.paths.first().map_or(String::new(), |p| p.display().to_string())
                    )
                })
                .collect();
            ToolResult::error(format!(
                "delete_storage: nothing was deleted because confirm is not true. It would delete {} and free {}. \
                 Call again with confirm true to do it.",
                listing.join("; "),
                en_bytes(freed)
            ))
        }));
    }
    Ok(Worker::spawn_then("delete_storage", move || {
        let outcomes = match delete_rows(&dirs, &ids, &Progress::default()) {
            Ok(o) => o,
            Err(e) => return (ToolResult::error(format!("delete_storage: {e}")), None),
        };
        let summary = DeleteSummary::from_outcomes(outcomes);
        let mut result = summary_json(&summary);
        let report = scan_report(&dirs, &Progress::default());
        result["total_bytes_now"] = json!(report.total_bytes);
        result["total_human_now"] = json!(en_bytes(report.total_bytes));
        let apply: super::tools::ApplyFn = Box::new(move |s: &mut AppState| {
            s.runtime.storage.report = Some(report);
            s.runtime.storage.summary = Some(summary);
            None
        });
        (ToolResult::json(result), Some(apply))
    }))
}

#[cfg(test)]
mod tests {
    use std::path::{Path, PathBuf};

    use hp_app::storage::AppDirs;
    use serde_json::{Value, json};

    use super::super::tools::{Outcome, run};

    fn scratch(name: &str) -> (AppDirs, PathBuf) {
        let base = if Path::new("E:/hp-scratch").is_dir() {
            PathBuf::from("E:/hp-scratch/agent-storage")
        } else {
            std::env::temp_dir().join("hp-scratch-agent-storage")
        }
        .join(name);
        let _ = std::fs::remove_dir_all(&base);
        let local = base.join("Local").join("HammerPro");
        let roaming = base.join("Roaming").join("HammerPro");
        let dirs = AppDirs {
            config: roaming.clone(),
            cache: local.clone(),
            autosave: local.join("autosave"),
            logs: local.join("logs"),
            crash: roaming.join("crash"),
            temp: base.join("Temp"),
        };
        for d in [&dirs.config, &dirs.cache, &dirs.autosave, &dirs.logs, &dirs.crash, &dirs.temp] {
            std::fs::create_dir_all(d).unwrap();
        }
        (dirs, base)
    }

    fn put(path: &Path, size: usize) {
        std::fs::create_dir_all(path.parent().unwrap()).unwrap();
        std::fs::write(path, vec![1u8; size]).unwrap();
    }

    fn call(s: &mut hp_app::AppState, tool: &str, args: Value) -> hp_mcp::protocol::ToolResult {
        match run(s, tool, &args) {
            Outcome::Done(r) => r,
            Outcome::Worker(w) => w.wait(s, std::time::Duration::from_secs(20)),
            other => panic!("unexpected {other:?}"),
        }
    }

    #[test]
    fn usage_lists_items_largest_first() {
        let mut s = hp_app::testing::test_state();
        let (dirs, _base) = scratch("mcp_usage");
        put(&dirs.cache.join("thumbs").join("a.png"), 100);
        put(&dirs.cache.join("builds").join("m.bsp"), 9000);
        s.runtime.storage.dirs = Some(dirs);
        let r = call(&mut s, "get_storage_usage", json!({}));
        assert!(!r.is_error);
        let v = r.structured.unwrap();
        assert_eq!(v["total_bytes"], 9100);
        assert_eq!(v["items"][0]["id"], "builds");
        assert_eq!(v["items"][0]["risk"], "safe");
        assert_eq!(v["items"][0]["percent"], 98.9);
        assert!(s.runtime.storage.report.is_some());
        assert!(call(&mut s, "get_storage_usage", json!({ "x": 1 })).is_error);
    }

    #[test]
    fn deleting_needs_confirm_and_user_data_needs_its_own_flag() {
        let mut s = hp_app::testing::test_state();
        let (dirs, base) = scratch("mcp_delete");
        put(&dirs.cache.join("builds").join("m.bsp"), 9000);
        put(&dirs.config.join("settings.toml"), 50);
        put(&base.join("Maps").join("mymap.vmf"), 10);
        s.runtime.storage.dirs = Some(dirs.clone());

        let r = call(&mut s, "delete_storage", json!({ "ids": ["builds"] }));
        assert!(r.is_error);
        assert!(r.first_text().contains("nothing was deleted"), "{}", r.first_text());
        assert!(dirs.cache.join("builds").join("m.bsp").is_file());

        let r = call(&mut s, "delete_storage", json!({ "ids": ["builds"], "confirm": false }));
        assert!(r.is_error && dirs.cache.join("builds").join("m.bsp").is_file());

        let r = call(&mut s, "delete_storage", json!({ "ids": ["settings"], "confirm": true }));
        assert!(r.is_error && r.first_text().contains("allow_user_data"));
        assert!(dirs.config.join("settings.toml").is_file());

        assert!(call(&mut s, "delete_storage", json!({ "ids": ["nope"], "confirm": true })).is_error);
        assert!(call(&mut s, "delete_storage", json!({ "ids": [], "confirm": true })).is_error);
        assert!(call(&mut s, "delete_storage", json!({ "ids": ["../Maps"], "confirm": true })).is_error);
        assert!(call(&mut s, "delete_storage", json!({ "confirm": true })).is_error);
        assert!(call(&mut s, "delete_storage", json!({ "ids": ["builds"], "confirm": "yes" })).is_error);

        let r = call(&mut s, "delete_storage", json!({ "ids": ["builds"], "confirm": true }));
        assert!(!r.is_error, "{}", r.first_text());
        let v = r.structured.unwrap();
        assert_eq!(v["freed_bytes"], 9000);
        assert_eq!(v["items"][0]["id"], "builds");
        assert!(!dirs.cache.join("builds").join("m.bsp").exists());
        assert_eq!(s.runtime.storage.report.as_ref().unwrap().total_bytes, 50);
        assert_eq!(s.runtime.storage.summary.as_ref().unwrap().freed, 9000);

        let r =
            call(&mut s, "delete_storage", json!({ "ids": ["settings"], "confirm": true, "allow_user_data": true }));
        assert!(!r.is_error);
        assert!(!dirs.config.join("settings.toml").exists());
        assert!(dirs.config.is_dir());
        assert!(base.join("Maps").join("mymap.vmf").is_file());
    }

    #[test]
    fn compile_items_are_refused_while_compiling() {
        let mut s = hp_app::testing::test_state();
        let (dirs, _base) = scratch("mcp_compiling");
        put(&dirs.cache.join("builds").join("m.bsp"), 10);
        s.runtime.storage.dirs = Some(dirs.clone());
        s.runtime.compile.progress =
            Some(hp_app::runtime::CompileProgress { step: String::new(), fraction: 0.0, elapsed_s: 0.0 });
        let r = call(&mut s, "delete_storage", json!({ "ids": ["builds"], "confirm": true }));
        assert!(r.is_error && r.first_text().contains("compile"));
        assert!(dirs.cache.join("builds").join("m.bsp").is_file());
        let r = call(&mut s, "delete_storage", json!({ "ids": ["thumbnails"], "confirm": true }));
        assert!(!r.is_error);
    }
}
