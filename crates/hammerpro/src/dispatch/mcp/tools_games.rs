use hp_app::AppState;
use hp_mcp::protocol::ToolResult;
use serde_json::{Value, json};

use super::semantic::*;
use super::tools::{Args, R};

const MAX_ID: usize = 260;

domain! { "games";
    list_games: Read, "List the games",
        "The Source games Hammer Pro compiles maps for: those found on this computer first (Steam libraries and \
        sourcemods, with their folders), then the others (maps still compile in their format). Each with its id, \
        engine branch, BSP version, static prop format, limits, FGD files and executable.",
        { "installed_only": boolean("Only the games found on this computer.") }, [];
    get_game: Read, "The game of the map",
        "The game the open map is for (its own one, kept with the map, else the default one of Settings > Game) \
        with its profile: folders, BSP format, limits, FGD files, executable.",
        {}, [];
    set_game: Editor, "Choose the game",
        "Make a game (an id from list_games) the game of the open map (scope \"map\", kept with the map), or the \
        default game of every map (scope \"default\", saved in the settings: set confirm). Its content and entity \
        definitions load when it is installed; the compile follows its BSP format, limits and lighting.",
        {
            "id": string("Game id from list_games (gmod, tf, portal2, mod:<folder>...)."),
            "scope": choice("map: this map only (default); default: every map without a game of its own.", &["map", "default"]),
            "confirm": boolean("The user asked to change the default game (scope default)."),
        }, ["id"];
    list_app_mounts: Read, "Steam games mounted by AppID",
        "The Steam games whose content is mounted by AppID (Settings > Game > Steam games by AppID, setting \
        game.mount_app_ids), in priority order, each with its name, install folder and Source game folders as \
        found in the Steam libraries (libraryfolders.vdf, appmanifest_<AppID>.acf); and, with installed, every \
        installed Steam game that has a Source game folder.",
        { "installed": boolean("Also list the installed Source games of the Steam libraries.") }, [];
    set_app_mounts: Editor, "Mount Steam games by AppID",
        "Change the Steam games whose content is mounted (after the content of the map's game): add AppIDs at \
        the end, remove some, or give the whole list in priority order. Saved in the settings \
        (game.mount_app_ids); the content is mounted again for the browsers, the 3D view and the compile. \
        Examples: 220 Half-Life 2, 240 Counter-Strike: Source, 300 Day of Defeat: Source, 380 and 420 the \
        episodes, 400 Portal, 440 Team Fortress 2, 4000 Garry's Mod.",
        {
            "add": app_ids("AppIDs to add at the end (kept once)."),
            "remove": app_ids("AppIDs to remove."),
            "order": app_ids("The whole list, in priority order (replaces it; add and remove apply after)."),
            "refresh": boolean("Search the Steam libraries again first (a game was installed or moved)."),
        }, [];
}

fn app_ids(description: &str) -> Value {
    json!({
        "type": "array",
        "items": { "type": "integer", "minimum": 1, "maximum": hp_fs::MAX_APP_ID },
        "maxItems": 64,
        "description": description,
    })
}

fn ids_arg(a: &Args<'_>, key: &str) -> R<Option<Vec<u32>>> {
    let Some(v) = a.get(key) else { return Ok(None) };
    let list = v.as_array().ok_or_else(|| format!("{key} must be a list of AppIDs"))?;
    if list.len() > 64 {
        return Err(format!("{key} has more than 64 AppIDs"));
    }
    list.iter()
        .map(|x| {
            x.as_u64()
                .filter(|n| *n > 0 && *n <= u64::from(hp_fs::MAX_APP_ID))
                .map(|n| n as u32)
                .ok_or_else(|| format!("{key}: {x} is not a Steam AppID"))
        })
        .collect::<R<Vec<u32>>>()
        .map(Some)
}

fn app_json(id: u32) -> Value {
    match hp_fs::find_steam_app(id) {
        Some(app) => json!({
            "app_id": id,
            "name": app.name,
            "installed": true,
            "install_dir": app.root.display().to_string(),
            "game_dirs": app.game_dirs.iter().map(|d| d.display().to_string()).collect::<Vec<_>>(),
            "mountable": !app.game_dirs.is_empty(),
        }),
        None => json!({
            "app_id": id,
            "name": hp_fs::steam_app_name(id),
            "installed": false,
            "mountable": false,
        }),
    }
}

fn list_app_mounts(state: &mut AppState, a: &Args<'_>) -> R<ToolResult> {
    let mounted: Vec<Value> = state.settings.game.mount_app_ids.iter().map(|id| app_json(*id)).collect();
    let mut out = json!({ "mounted": mounted });
    if a.opt_bool("installed")?.unwrap_or(false) {
        let installed: Vec<Value> = hp_fs::installed_source_apps()
            .into_iter()
            .map(|app| json!({ "app_id": app.app_id, "name": app.name, "install_dir": app.root.display().to_string() }))
            .collect();
        out["installed"] = Value::Array(installed);
    }
    Ok(ToolResult::json(out))
}

fn set_app_mounts(state: &mut AppState, a: &Args<'_>) -> R<ToolResult> {
    let order = ids_arg(a, "order")?;
    let add = ids_arg(a, "add")?.unwrap_or_default();
    let remove = ids_arg(a, "remove")?.unwrap_or_default();
    if a.opt_bool("refresh")?.unwrap_or(false) {
        hp_fs::refresh_steam_apps();
    }
    if order.is_none() && add.is_empty() && remove.is_empty() {
        return Err("give add, remove or order".to_owned());
    }
    let before = state.settings.game.mount_app_ids.clone();
    let mut list: Vec<u32> = Vec::new();
    for id in order.unwrap_or_else(|| before.clone()).into_iter().chain(add) {
        if !list.contains(&id) {
            list.push(id);
        }
    }
    list.retain(|id| !remove.contains(id));
    let changed = list != before;
    if changed {
        state.settings.game.mount_app_ids = list.clone();
        state.settings_dirty = true;
    }
    let apps: Vec<Value> = list.iter().map(|id| app_json(*id)).collect();
    let missing: Vec<u32> =
        apps.iter().filter(|v| v["mountable"] != true).filter_map(|v| v["app_id"].as_u64().map(|n| n as u32)).collect();
    Ok(ToolResult::json(json!({
        "changed": changed,
        "mounted": apps,
        "not_mountable": missing,
        "note": if changed { "The content is mounting again (get_map_info game_content and list_asset_sources show it)." } else { "Nothing changed." },
    })))
}

fn profile_json(p: &hp_fs::GameProfile) -> Value {
    let f = p.format();
    json!({
        "id": p.id,
        "name": p.name,
        "app_id": p.app_id,
        "installed": p.installed,
        "branch": p.branch.map(|b| b.name()),
        "bsp_version": p.branch.map(|_| f.version),
        "lump_table": p.branch.map(|_| if f.layout == hp_bsp::LumpLayout::VersionFirst { "version first (Left 4 Dead 2)" } else { "standard" }),
        "static_props": p.branch.map(|_| f.static_props.name()),
        "worldlights_version": p.branch.map(|_| f.worldlights_version),
        "limits": p.branch.map(|_| if f.limits == hp_bsp::limits::LimitSet::Gmod { "Garry's Mod" } else { "Valve" }),
        "lighting": p.lighting.flag(),
        "game_dir": (!p.game_dir.as_os_str().is_empty()).then(|| p.game_dir.display().to_string()),
        "install_dir": (!p.root.as_os_str().is_empty()).then(|| p.root.display().to_string()),
        "executable": p.exe.as_ref().map(|e| e.display().to_string()),
        "fgds": p.fgds.iter().map(|f| f.display().to_string()).collect::<Vec<_>>(),
        "source_mod": p.source_mod,
        "mapbase": p.mapbase,
    })
}

fn list_games(_state: &mut AppState, a: &Args<'_>) -> R<ToolResult> {
    let installed_only = a.opt_bool("installed_only")?.unwrap_or(false);
    let list = hp_fs::game_catalogue();
    let games: Vec<Value> = list.iter().filter(|p| p.installed || !installed_only).map(profile_json).collect();
    Ok(ToolResult::json(json!({ "count": games.len(), "games": games })))
}

fn get_game(state: &mut AppState, _a: &Args<'_>) -> R<ToolResult> {
    let g = &state.settings.game;
    let profile = g.active_profile();
    Ok(ToolResult::json(json!({
        "active": g.active_game(),
        "default": g.game,
        "map_game": g.map_game,
        "is_gmod": g.is_gmod(),
        "profile": profile.as_ref().map(profile_json),
    })))
}

fn set_game(state: &mut AppState, a: &Args<'_>) -> R<ToolResult> {
    let id = a.str("id", MAX_ID)?.trim().to_owned();
    let for_map = a.opt_enum("scope", &["map", "default"])?.as_deref() != Some("default");
    if !for_map && a.opt_bool("confirm")? != Some(true) {
        return Err(
            "the default game is saved in the settings: set confirm to true when the user asked for it".to_owned()
        );
    }
    let list = hp_fs::game_catalogue();
    let Some(p) = list.iter().find(|p| p.id.eq_ignore_ascii_case(&id)) else {
        return Err(format!("no game {id} (list_games gives the ids)"));
    };
    let p = p.clone();
    let changed = hp_app::games::choose_game(state, &p.id, for_map);
    Ok(ToolResult::json(json!({
        "game": p.id,
        "scope": if for_map { "map" } else { "default" },
        "changed": changed,
        "installed": p.installed,
        "note": if p.installed {
            "Its content and entity definitions are loading (get_fgd_info shows when they are loaded)."
        } else {
            "Not installed: the content mounted now stays; maps compile in its format and limits, without copy nor launch."
        },
    })))
}

#[cfg(test)]
mod tests {
    use serde_json::json;

    use super::super::semantic_tests::{err, ok};

    #[test]
    fn games() {
        let mut s = hp_app::testing::test_state();
        let all = ok(&mut s, "list_games", json!({}));
        let games = all["games"].as_array().unwrap();
        let gmod = games.iter().find(|g| g["id"] == "gmod").expect("Garry's Mod is listed");
        assert_eq!(gmod["bsp_version"], 20);
        assert_eq!(gmod["limits"], "Garry's Mod");
        let l4d2 = games.iter().find(|g| g["id"] == "left4dead2").expect("listed");
        assert_eq!(l4d2["bsp_version"], 21);
        assert_eq!(l4d2["static_props"], "v9");
        assert!(!games.iter().any(|g| g["id"] == "cs2" || g["id"] == "contagion"));
        let installed = ok(&mut s, "list_games", json!({ "installed_only": true }));
        assert!(installed["games"].as_array().unwrap().iter().all(|g| g["installed"] == true));
        let g = ok(&mut s, "get_game", json!({}));
        assert_eq!(g["active"], "gmod");
        assert_eq!(g["is_gmod"], true);
        let r = ok(&mut s, "set_game", json!({ "id": "left4dead2" }));
        assert_eq!(r["scope"], "map");
        assert_eq!(ok(&mut s, "get_game", json!({}))["active"], "left4dead2");
        assert_eq!(s.settings.compile.target.as_deref(), Some("left4dead2"));
        ok(&mut s, "set_game", json!({ "id": "gmod" }));
        assert_eq!(ok(&mut s, "get_game", json!({}))["is_gmod"], true);
        err(&mut s, "set_game", json!({ "id": "tf", "scope": "default" }));
        err(&mut s, "set_game", json!({ "id": "nope" }));
        err(&mut s, "set_game", json!({ "id": "contagion" }));
        ok(&mut s, "set_game", json!({ "id": "tf", "scope": "default", "confirm": true }));
        assert_eq!(s.settings.game.game, "tf");
        assert_eq!(ok(&mut s, "get_game", json!({}))["default"], "tf");
    }

    #[test]
    fn app_mounts() {
        let mut s = hp_app::testing::test_state();
        let v = ok(&mut s, "list_app_mounts", json!({}));
        assert_eq!(v["mounted"], json!([]));
        assert!(v.get("installed").is_none());
        let r = ok(&mut s, "set_app_mounts", json!({ "add": [240, 440, 240] }));
        assert_eq!(r["changed"], true);
        assert_eq!(s.settings.game.mount_app_ids, [240, 440]);
        assert!(s.settings_dirty);
        assert_eq!(r["mounted"][0]["app_id"], 240);
        ok(&mut s, "set_app_mounts", json!({ "order": [440, 240], "add": [220], "remove": [240] }));
        assert_eq!(s.settings.game.mount_app_ids, [440, 220]);
        let r = ok(&mut s, "set_app_mounts", json!({ "add": [440] }));
        assert_eq!(r["changed"], false);
        let v = ok(&mut s, "list_app_mounts", json!({ "installed": true }));
        assert_eq!(v["mounted"].as_array().map(Vec::len), Some(2));
        assert!(v["installed"].is_array());
        err(&mut s, "set_app_mounts", json!({}));
        err(&mut s, "set_app_mounts", json!({ "add": [0] }));
        err(&mut s, "set_app_mounts", json!({ "add": ["tf"] }));
        assert_eq!(s.settings.game.mount_app_ids, [440, 220]);
    }
}
