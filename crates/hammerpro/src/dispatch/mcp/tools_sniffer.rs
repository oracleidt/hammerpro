use hp_app::AppState;
use hp_mcp::protocol::ToolResult;
use hp_sniffer::SettingValue;
use serde_json::{Value, json};

use super::semantic::*;
use super::tools::{Args, R};

domain! { "bsp sniffer";
    sniffer_start: Editor, "Start the BSP Sniffer",
        "Start a BSP Sniffer run (Garry's Mod only): launches Garry's Mod through Steam, joins the server, waits for \
        the map to load and the engine to finish building it, reads the map as the game loaded it from the game memory \
        (brushes, materials, texture axes, displacements, static props, overlays, cubemaps, area portals and \
        entities), merges it with the map file that the client has (the game cache, maps, download, addons, workshop \
        archives, read through its lump directory, so a protected, shuffled or padded file works) field by field after \
        a consistency check against the memory, rebuilds a BSP, decompiles it to a VMF and saves the content. The \
        memory is the main source and the matching map file fills in what the memory does not keep (displacement \
        alphas, prop fade distances and flags, cubemap sizes, overlay order, hidden side texture axes, the pakfile); a \
        file that does not match is ignored. The result lists the source of each category (memory, map file, game \
        content, absent). Runs in the background: poll sniffer_status and read sniffer_log. Needs the setting \
        authorized = true (the user confirms they may test the server; set it with sniffer_set_settings only when \
        the user said so). Takes the game lock rule of the machine: only one run at a time. Protected maps (Protect \
        your BSP: logic removed, material names scrambled, original faces emptied, watermark, server lock, lumps \
        shuffled) are recovered the same way: the scrambled material names are restored from the patch files of the \
        pakfile of the protected map file, and guessed from the loaded materials when no map file is found; the \
        report says how many of each. The address can also be map:<name> to start a map of this computer in a local \
        Garry's Mod (listen server), or map: alone with attach_running to read the map the running game has loaded.",
        { "address": string("Server as ip:port, or map:<name> to start a map of this computer in a local Garry's Mod (a listen server, the client memory is the same as a real join), or map: alone together with attach_running to read the map the running game has loaded; saved in the settings. Default: the saved address.") }, [];
    sniffer_cancel: Editor, "Cancel the BSP Sniffer",
        "Stop the running BSP Sniffer; the game it started is closed when kill_game is on.",
        {}, [];
    sniffer_status: Read, "BSP Sniffer status",
        "Phase (idle, preflight, querying, launching, waiting_game, waiting_map, capturing, verifying, rebuilding, \
        decompiling, extracting, finishing, done, failed, cancelled), progress 0 to 1, detail, elapsed seconds, game \
        process, map, error, and when done the output files and the comparison counts.",
        {}, [];
    sniffer_log: Read, "BSP Sniffer log",
        "The log of the current or last run. Give from (the next value of the previous call) to read only new lines.",
        {
            "from": integer("First line to return (default 0).", 0, i64::MAX),
            "limit": integer("At most this many lines (default 200).", 1, 2000),
        }, [];
    sniffer_result: Read, "BSP Sniffer report",
        "The full report of the last finished run: the source of the geometry, what was decoded from the game memory \
        (faces, texture axes, displacements, static props, overlays, cubemaps, area portals), the number of map file \
        reads, the cross check against the map file, the lumps confirmed byte for \
        byte, the counts of the recovered BSP and of the VMF (brushes, sides, nodraw, displacements, props, \
        entities), the lumps copied from the map file as they are with their bytes and the refused ones, the \
        content saved and the timings. As text and as JSON.",
        {}, [];
    sniffer_get_settings: Read, "BSP Sniffer settings",
        "Every BSP Sniffer setting with its value and the list of accepted keys.",
        {}, [];
    sniffer_set_settings: Editor, "Change BSP Sniffer settings",
        "Change BSP Sniffer settings (saved). Keys: address, authorized, output_dir, file_name_template ({map} {ip} \
        {port} {date} {time} {datetime}), gmod_dir (empty: automatic), launch_options, insecure (auto, always, \
        never), attach_running, process_timeout_secs, load_timeout_secs, settle_secs, poll_secs, save_models, \
        save_materials, save_sounds, save_other, keep_bsp, only_non_stock, kill_game, auto_disconnect, \
        open_when_done and decompile.brushes, decompile.displacements, decompile.entities, \
        decompile.brush_entities, decompile.static_props, decompile.overlays, decompile.cubemaps, \
        decompile.areaportals, decompile.occluders, decompile.group_details. The panel is the action bsp_sniffer \
        (run_action).",
        { "settings": json!({ "type": "object", "additionalProperties": true, "description": "Keys and values." }) },
        ["settings"];
}

fn settings_json(s: &hp_sniffer::SnifferSettings) -> Value {
    let mut out = serde_json::Map::new();
    let mut keys: Vec<String> = hp_sniffer::settings::SETTING_KEYS
        .iter()
        .filter(|(k, _)| !k.contains('<'))
        .map(|(k, _)| (*k).to_owned())
        .collect();
    keys.extend(hp_sniffer::DecompileSettings::KEYS.iter().map(|k| format!("decompile.{k}")));
    for k in keys {
        if let Some(v) = s.get(&k) {
            out.insert(
                k,
                match v {
                    SettingValue::Bool(b) => json!(b),
                    SettingValue::Number(n) => json!(n),
                    SettingValue::Text(t) => json!(t),
                },
            );
        }
    }
    Value::Object(out)
}

fn to_setting(v: &Value) -> Option<SettingValue> {
    match v {
        Value::Bool(b) => Some(SettingValue::Bool(*b)),
        Value::Number(n) => n.as_f64().map(SettingValue::Number),
        Value::String(s) => Some(SettingValue::Text(s.clone())),
        _ => None,
    }
}

fn outcome_json(o: &hp_sniffer::session::Outcome) -> Value {
    let r = &o.report;
    json!({
        "vmf": o.vmf_path.display().to_string(),
        "bsp": o.bsp_path.as_ref().map(|p| p.display().to_string()),
        "content_dir": o.content_dir.as_ref().map(|p| p.display().to_string()),
        "report_file": o.report_path.display().to_string(),
        "map": r.map,
        "geometry_source": r.source,
        "memory_only": r.memory_only,
        "geometry": r.geometry,
        "map_file_reads": r.map_file_opens,
        "lumps_taken_from_the_map_file": {
            "names": r.passthrough.names(),
            "bytes": r.passthrough.bytes(),
            "refused": r.passthrough.refused.iter().map(|i| hp_bsp::lump_name(*i)).collect::<Vec<_>>(),
        },
        "protected_map": {
            "detected": r.unprotect.protected(),
            "scrambled_material_names": r.unprotect.garbage_names,
            "restored_from_patch_files": r.unprotect.restored_from_patch,
            "guessed_from_loaded_materials": r.unprotect.guessed,
            "custom_materials_kept_scrambled": r.unprotect.kept_custom,
            "unresolved": r.unprotect.unresolved,
            "watermark": r.unprotect.watermark,
            "lock_hash": r.unprotect.lock_hash,
        },
        "memory": {
            "render_data_found": r.memory.render_found,
            "faces": r.memory.faces,
            "texinfo": r.memory.texinfo,
            "vertexes": r.memory.vertexes,
            "displacements": r.memory.displacements,
            "static_props": r.memory.props,
            "static_prop_models": r.memory.prop_models,
            "overlays": r.memory.overlays,
            "cubemaps": r.memory.cubemaps,
            "area_portals": r.memory.area_portals,
            "area_portal_vertexes": r.memory.area_portal_verts,
        },
        "filled_from_file": {
            "file_used": r.merge.file_used,
            "displacements": [r.merge.displacements_from_file, r.merge.displacements],
            "static_props": [r.merge.props_from_file, r.merge.props],
            "cubemap_sizes": [r.merge.cubemaps_from_file, r.merge.cubemaps],
            "overlay_orders": [r.merge.overlays_from_file, r.merge.overlays],
            "side_texture_axes": [r.merge.sides_from_file, r.merge.sides],
            "pakfile_files": r.merge.pak_files,
        },
        "sources": r
            .sources
            .iter()
            .map(|c| json!({ "category": c.category, "source": c.source, "detail": c.detail }))
            .collect::<Vec<_>>(),
        "cross_check": r
            .cross_check
            .iter()
            .map(|c| json!({ "name": c.name, "memory": c.memory, "file": c.file, "equal": c.memory == c.file }))
            .collect::<Vec<_>>(),
        "entities_in_memory": r.memory_entities,
        "entities_in_file": r.file_entities,
        "entities_identical": r.entities_identical,
        "lumps_verified_in_memory": r.verified_lumps,
        "bytes_verified_in_memory": r.verified_bytes,
        "loaded_map": {
            "brushes": r.bsp_counts.brushes,
            "brush_sides": r.bsp_counts.brush_sides,
            "displacements": r.bsp_counts.displacements,
            "static_props": r.bsp_counts.static_props,
            "overlays": r.bsp_counts.overlays,
            "cubemaps": r.bsp_counts.cubemaps,
            "area_portals": r.bsp_counts.area_portals,
            "faces": r.bsp_counts.faces,
            "planes": r.bsp_counts.planes,
            "vertexes": r.bsp_counts.vertexes,
            "materials": r.bsp_counts.materials,
            "nodraw_sides": r.bsp_counts.nodraw_sides,
            "tool_sides": r.bsp_counts.tool_sides,
            "sky_sides": r.bsp_counts.sky_sides,
            "clip_sides": r.bsp_counts.clip_sides,
            "trigger_sides": r.bsp_counts.trigger_sides,
        },
        "vmf": {
            "solids": r.vmf.solids,
            "world_solids": r.vmf.world_solids,
            "entity_solids": r.vmf.entity_solids,
            "sides": r.vmf.sides,
            "displacement_sides": r.vmf.displacement_sides,
            "entities": r.vmf.entities,
            "nodraw_sides": r.vmf.nodraw_sides,
            "tool_sides": r.vmf.tool_sides,
            "sky_sides": r.vmf.sky_sides,
            "clip_sides": r.vmf.clip_sides,
            "trigger_sides": r.vmf.trigger_sides,
            "props": r.vmf.props,
            "overlays": r.vmf.overlays,
            "cubemaps": r.vmf.cubemaps,
            "func_detail": r.vmf.details,
            "outputs": r.vmf.connections,
            "materials": r.vmf.materials,
        },
        "content": {
            "pakfile_files": r.assets.pak_files,
            "game_content_files": r.assets.content_files,
            "stock_skipped": r.assets.stock_skipped + r.assets.pak_skipped_stock,
            "missing": r.assets.missing.len(),
            "models_recovered_from_memory": r.assets.memory_models,
            "bytes": r.assets.pak_bytes + r.assets.content_bytes,
        },
        "timings": r.timings.iter().map(|(k, v)| json!({ "step": k, "seconds": v })).collect::<Vec<_>>(),
        "notes": r.notes,
    })
}

fn sniffer_start(state: &mut AppState, a: &Args<'_>) -> R<ToolResult> {
    if let Some(addr) = a.opt_str("address", 300)? {
        state.settings.sniffer.set("address", &SettingValue::Text(addr.to_owned())).map_err(|e| e.to_string())?;
        state.settings_dirty = true;
    }
    hp_ui_lab::sniffer::start(state, None)?;
    Ok(ToolResult::json(json!({
        "started": true,
        "address": state.settings.sniffer.address,
        "note": "Poll sniffer_status and read sniffer_log. The game is closed at the end when kill_game is on.",
    })))
}

fn sniffer_cancel(state: &mut AppState, _a: &Args<'_>) -> R<ToolResult> {
    if !hp_ui_lab::sniffer::is_running(state) {
        return Err("no sniff is running".to_owned());
    }
    hp_ui_lab::sniffer::cancel(state);
    Ok(ToolResult::json(json!({ "cancelling": true })))
}

fn sniffer_status(state: &mut AppState, _a: &Args<'_>) -> R<ToolResult> {
    let rt = &state.runtime.lab.sniffer;
    let Some(h) = &rt.handle else {
        return Ok(ToolResult::json(json!({
            "phase": "idle",
            "running": false,
            "authorized": state.settings.sniffer.authorized,
            "address": state.settings.sniffer.address,
        })));
    };
    let s = h.status();
    let mut out = json!({
        "phase": s.phase.id(),
        "running": s.phase.is_running(),
        "progress": s.progress,
        "detail": s.detail,
        "elapsed_seconds": s.elapsed.as_secs_f64(),
        "game_process": s.game_pid,
        "map": s.map,
        "error": s.error,
    });
    if let Some(Ok(o)) = h.outcome() {
        out["result"] = outcome_json(&o);
    }
    Ok(ToolResult::json(out))
}

fn sniffer_log(state: &mut AppState, a: &Args<'_>) -> R<ToolResult> {
    let from = a.opt_u64("from", 0, u64::MAX / 2)?.unwrap_or(0) as usize;
    let limit = a.opt_u64("limit", 1, 2000)?.unwrap_or(200) as usize;
    let Some(h) = &state.runtime.lab.sniffer.handle else {
        return Ok(ToolResult::json(json!({ "lines": [], "next": 0 })));
    };
    let (lines, next) = h.log_since(from);
    let take = lines.len().min(limit);
    let consumed = next - (lines.len() - take);
    let rows: Vec<Value> = lines
        .into_iter()
        .take(take)
        .map(|l| {
            json!({
                "t": (l.seconds * 10.0).round() / 10.0,
                "level": match l.level {
                    hp_sniffer::session::Level::Info => "info",
                    hp_sniffer::session::Level::Warn => "warn",
                    hp_sniffer::session::Level::Error => "error",
                },
                "text": l.text,
            })
        })
        .collect();
    Ok(ToolResult::json(json!({ "lines": rows, "next": consumed })))
}

fn sniffer_result(state: &mut AppState, _a: &Args<'_>) -> R<ToolResult> {
    let Some(h) = &state.runtime.lab.sniffer.handle else { return Err("no sniff has run yet".to_owned()) };
    match h.outcome() {
        Some(Ok(o)) => Ok(ToolResult::json(json!({ "summary": outcome_json(&o), "report": o.report.render_text() }))),
        Some(Err(e)) => Err(format!("the last sniff did not finish: {e}")),
        None => Err("the sniff is still running (sniffer_status)".to_owned()),
    }
}

fn sniffer_get_settings(state: &mut AppState, _a: &Args<'_>) -> R<ToolResult> {
    let keys: Vec<Value> =
        hp_sniffer::settings::SETTING_KEYS.iter().map(|(k, kind)| json!({ "key": k, "type": kind })).collect();
    Ok(ToolResult::json(json!({ "settings": settings_json(&state.settings.sniffer), "keys": keys })))
}

fn sniffer_set_settings(state: &mut AppState, a: &Args<'_>) -> R<ToolResult> {
    let Some(Value::Object(map)) = a.get("settings") else { return Err("settings must be an object".to_owned()) };
    if map.is_empty() {
        return Err("settings is empty".to_owned());
    }
    if hp_ui_lab::sniffer::is_running(state) {
        return Err("a sniff is running: cancel it or wait before changing the settings".to_owned());
    }
    let mut next = state.settings.sniffer.clone();
    for (k, v) in map {
        let value = to_setting(v).ok_or_else(|| format!("{k}: give a boolean, a number or a text"))?;
        next.set(k, &value)?;
    }
    next.validate()?;
    state.settings.sniffer = next;
    state.settings_dirty = true;
    Ok(ToolResult::json(json!({ "settings": settings_json(&state.settings.sniffer) })))
}

#[cfg(test)]
mod tests {
    use serde_json::json;

    use super::super::semantic_tests::{err, ok};

    #[test]
    fn settings_are_readable_and_writable() {
        let mut s = hp_app::testing::test_state();
        let v = ok(&mut s, "sniffer_get_settings", json!({}));
        assert_eq!(v["settings"]["kill_game"], json!(true));
        assert_eq!(v["settings"]["decompile.displacements"], json!(true));
        assert!(v["keys"].as_array().is_some_and(|k| k.len() > 20));
        ok(
            &mut s,
            "sniffer_set_settings",
            json!({ "settings": { "address": "1.2.3.4:27016", "kill_game": false, "decompile.overlays": false, "load_timeout_secs": 100, "insecure": "never" } }),
        );
        assert_eq!(s.settings.sniffer.address, "1.2.3.4:27016");
        assert!(!s.settings.sniffer.kill_game);
        assert!(!s.settings.sniffer.decompile.overlays);
        assert_eq!(s.settings.sniffer.load_timeout_secs, 100);
        let e = err(&mut s, "sniffer_set_settings", json!({ "settings": { "map_file": "never" } }));
        assert!(e.contains("unknown"), "{e}");
        let e = err(&mut s, "sniffer_set_settings", json!({ "settings": { "geometry": "memory" } }));
        assert!(e.contains("unknown"), "{e}");
        for bad in [
            json!({ "settings": {} }),
            json!({ "settings": { "nope": true } }),
            json!({ "settings": { "insecure": "maybe" } }),
            json!({ "settings": { "decompile.nope": true } }),
            json!({ "settings": { "kill_game": [1] } }),
        ] {
            err(&mut s, "sniffer_set_settings", bad);
        }
    }

    #[test]
    fn starting_needs_the_authorization() {
        let mut s = hp_app::testing::test_state();
        let e = err(&mut s, "sniffer_start", json!({ "address": "1.2.3.4:27015" }));
        assert!(e.contains("authorized"), "{e}");
        assert_eq!(s.settings.sniffer.address, "1.2.3.4:27015", "the address is saved anyway");
        assert_eq!(ok(&mut s, "sniffer_status", json!({}))["phase"], json!("idle"));
        err(&mut s, "sniffer_cancel", json!({}));
        err(&mut s, "sniffer_result", json!({}));
        assert_eq!(ok(&mut s, "sniffer_log", json!({ "from": 3 }))["next"], json!(0));
    }
}
