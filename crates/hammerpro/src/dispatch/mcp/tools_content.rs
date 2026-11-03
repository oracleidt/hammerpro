use std::path::PathBuf;

use hp_app::runtime::AssetCollectRequest;
use hp_app::state::RightTab;
use hp_app::{Action, AppState};
use hp_fs::deps::{PackPolicy, ResolveOptions};
use hp_mcp::protocol::ToolResult;
use hp_ui_mapping::mapassets::{self, AssetTree, Filter, Group, Item, Show};
use serde_json::{Value, json};

use super::semantic::*;
use super::tools::{Args, MAX_NAME, Outcome, R, Worker};

domain! { "content and release";
    get_dependency_report: Read, "Dependency report",
        "The content the map uses (materials, models, sounds, particles, textures of materials, model materials...): \
        each file with its source (base game, mounted game, Workshop item, custom folder, generated) and size, what \
        a release ships, the Workshop items it needs, and the missing assets with the objects using them.",
        {
            "shipped_only": boolean("Only the files a release ships."),
            "workshop": boolean("A release ships Workshop content too (default false: listed as required items)."),
            "mounted": boolean("A release ships content of mounted games (default false)."),
            "filter": string("Path pattern, for example \"materials/custom/*\"."),
            "limit": limit(), "offset": offset(),
        }, [];
    open_release_window: Editor, "Release window",
        "Open the Build and save as content window on a tab: build (Final compile, content folder, GMA), report, \
        checklist, addon (addon.json) or publish. The window runs the build on its own threads.",
        { "tab": choice("Tab (default build).", &["build", "report", "checklist", "addon", "publish"]) }, [];
    start_release: Editor, "Build and save as content",
        "Run Build and save as content: the final compile (when the content settings ask for it), the content \
        folder with the BSP and every custom material, model, sound and particle the map uses, and the GMA. The \
        map is saved first. It runs on its own threads: follow it with get_release_status. With analyze_only, \
        only the dependencies are resolved (nothing is written). A build writes files: set confirm to true. \
        With protect, Protect your BSP ships a protected copy of the map (gameplay logic and entity wiring \
        stripped, watermarked) and keeps the full one for the server.",
        {
            "analyze_only": boolean("Only resolve the dependencies (default false)."),
            "confirm": boolean("Required for a build (it compiles and writes the content folder and the GMA)."),
            "protect": boolean("Protect the shipped map with Protect your BSP (default: the content setting)."),
            "watermark": string("Watermark for the protected map (author, server), when protect is on."),
            "game_session": boolean("Build the cubemaps and the navigation mesh in Garry's Mod during the build \
                (starts the game; saved as the content setting). Only when the user asked; refused in automated runs."),
            "keep_cubemaps": boolean("Keep the cubemaps built in game when a compile replaces the map (saved)."),
        }, [];
    get_protect_options: Read, "Protect your BSP options",
        "Every option of Protect your BSP as saved in the content settings: protect, watermark, the decompile \
        breakers (strip_original_faces, randomize_materials), the server lock, and the structure scrambler (shuffle_lumps, \
        junk_gaps, decoys, scramble_header, shuffle_game_lumps, random_pak, reorder_tables, reorder_world, layout_seed).",
        {}, [];
    set_protect_options: Editor, "Change Protect your BSP options",
        "Change options of Protect your BSP (saved in the content settings, used by the next start_release). The \
        structure scrambler makes the shipped BSP hard to parse: shuffle_lumps puts the lump blocks in a random \
        physical order, junk_gaps fills random gaps between them with random bytes, decoys adds unreferenced blocks \
        (random data and stale altered copies of lumps), scramble_header randomizes the map revision, the version of \
        lumps the game ignores and the unused directory entries, shuffle_game_lumps shuffles the game lump directory, \
        random_pak puts the pakfile anywhere, reorder_tables permutes planes, vertexes, edges, texinfo and texdata and \
        renumbers every reference, reorder_world permutes brushes and faces the same way (the face and brush lists of the leaves are kept in order: shuffling them crashed the game). The map \
        loads and plays the same. layout_seed fixes the randomness (empty: new at each compile). preset strongest \
        turns every scrambler option on, none turns them off; individual keys apply after it.",
        {
            "preset": choice("Set every structure scrambler option at once.", &["strongest", "none"]),
            "protect": boolean("Protect your BSP on or off."),
            "watermark": string("Watermark text (author, server)."),
            "strip_original_faces": boolean("Empty the original faces lump."),
            "randomize_materials": boolean("Material randomizer."),
            "lock": boolean("Server lock."),
            "lock_key": string("Secret key of the server lock (only its hash goes in the map)."),
            "lock_block": boolean("Block play on an unauthorized server."),
            "shuffle_lumps": boolean("Random physical order of the lump blocks."),
            "junk_gaps": boolean("Random junk gaps between the lumps."),
            "decoys": boolean("Unreferenced decoy blocks."),
            "scramble_header": boolean("Randomize the header fields the game ignores."),
            "shuffle_game_lumps": boolean("Shuffle the game lump directory."),
            "random_pak": boolean("Pakfile at a random position."),
            "reorder_tables": boolean("Permute planes, vertexes, edges, texinfo and texdata."),
            "reorder_world": boolean("Permute brushes and faces (the leaf lists stay in order)."),
            "layout_seed": string("Seed of the structure scrambler (empty: random at each compile)."),
        }, [];
    get_release_checklist: Read, "Release checklist",
        "The release checklist of the map (saved, compiled, leaks, lighting, cubemaps, navigation mesh, missing \
        content, thumbnail, addon.json, GMA...): each row with ok, warning, failed or unknown and a detail. The \
        first call gathers it on a thread: call again while gathering is true.",
        { "refresh": boolean("Gather it again (after a compile or a release).") }, [];
    get_release_status: Read, "Release progress",
        "Whether the release window is open, the progress of a running release (step and fraction), and the \
        outcome of the last release of the map (folders, GMA, files, bytes, error, the end of its log).",
        {}, [];
    cancel_release: Editor, "Cancel the release",
        "Stop the running release build.",
        {}, [];
    inspect_archive: Editor, "Inspect an archive",
        "Open a GMA, VPK or BSP of the map folder in the pack inspector window (files, sizes, contents).",
        { "file": string("Archive in the map folder (relative).") }, ["file"];
    publish_workshop: Editor, "Publish on the Workshop",
        "Open the Publish tab of the release window, where the user reviews and confirms the upload. Only when the \
        user asked for it: set confirm to true. Refused in sandboxed (automated) runs.",
        { "confirm": boolean("The user asked to publish.") }, ["confirm"];
    list_map_assets: Read, "Map assets",
        "Every asset the map uses, as the Map assets tab lists it, by group: materials (brush faces, displacements \
        with both blend textures, overlays, decals, sprites, the sky, keyvalues) with their textures (editor \
        textures included), models with their files, skins, gibs and animation sounds, sounds (entity keyvalues, \
        soundscripts resolved to their waves, soundscapes and their files), particle files, other files. Each \
        entry with its files: where each one comes from (base game, addon, Workshop, custom folder, loose file...), \
        its size, and the missing ones. Instances are included. Resolved on a worker thread.",
        {
            "show": choice("Entries listed (default custom: with files that are not part of the base game, or \
                missing files).", &["custom", "all", "missing"]),
            "groups": strings("Groups listed: materials, models, sounds, particles, other (default every group).", 5),
            "filter": string("Only entries whose path, or the path of one of their files, contains this text."),
            "files": boolean("List the files of each entry (default true)."),
            "limit": limit(), "offset": offset(),
        }, [];
    collect_map_assets: Editor, "Collect the map's assets",
        "Copy the assets the map uses into a clean folder, like the Collect button of the Map assets tab: every \
        entry of the groups (with the filter) and everything it needs, under materials/, models/, sound/, \
        particles/... (files of VPKs and GMAs are extracted), then a report <map>_assets.txt in the folder. By \
        default only what is not part of the base game. A file the folder already has with other content is kept \
        (and listed in the report) unless replace is true. Runs in the background in the tab: follow it with \
        get_map_assets_status.",
        {
            "output": string("Output folder: an absolute path (created when missing), never mounted game content."),
            "base_game": boolean("Also copy base game assets (default false: every player has them)."),
            "replace": boolean("Replace files the folder has with other content (default false)."),
            "groups": strings("Groups collected: materials, models, sounds, particles, other (default every group).", 5),
            "filter": string("Only entries whose path, or the path of one of their files, contains this text."),
        }, ["output"];
    get_map_assets_status: Read, "Map assets collect status",
        "Progress of the running collect of the Map assets tab (files done and total), and what the last collect \
        gave: output folder, files copied, replaced, already there, kept, failed (with the error), missing, bytes \
        written, the report file.",
        {}, [];
    cancel_collect_map_assets: Editor, "Cancel the collect",
        "Stop the running collect of the Map assets tab (the files copied so far stay, the report says it was \
        cancelled).",
        {}, [];
    get_content_index: Read, "Content index and scan",
        "State of the persistent content index (the saved list of the files of the game folders, custom folders \
        and mounts, validated by folder modification times at launch instead of scanning everything again) and of \
        the scan: whether it is on, the index file, its size and entries, how many folders were checked, reused \
        or scanned again, the duration of the last folder scan, the scan mode (auto, parallel, sequential) and the \
        type of each drive seen (rotational drives are scanned one folder at a time in auto mode). Change it with \
        set_setting: scan.use_index (true or false) and scan.mode (\"auto\", \"parallel\", \"sequential\"), \
        applied at the next mount.",
        {}, [];
    rebuild_content_index: Editor, "Rebuild the content index",
        "Delete the saved content index and mount the game content again with a full scan of every folder, like \
        the Rebuild index button of Settings > Game (and the rebuild_content_index action). Use it when files \
        changed in a way the index missed. The mount runs on its own threads: follow it with get_content_index.",
        {}, [];
    find_unused_assets: Read, "Unused files of a content folder",
        "The files under materials, models, sound and particles of a content folder (an addon, a garrysmod \
        folder, or one of those folders itself) that the open map does not use: what the map needs, the textures \
        of its materials and the files of its models count as used, even when the folder is not mounted. Report \
        only: nothing is deleted.",
        {
            "folder": string("Content folder: an absolute path."),
            "limit": limit(), "offset": offset(),
        }, ["folder"];
}

fn get_dependency_report(state: &mut AppState, a: &Args<'_>) -> R<Outcome> {
    let fs = state.assets.fs().cloned().ok_or("the game content is not loaded (configure the game first)")?;
    let roots = hp_ui_mapping::release::mcp::dependency_roots(state);
    let map_name = state
        .doc
        .path()
        .and_then(|p| p.file_stem())
        .map_or_else(|| "untitled".to_owned(), |s| s.to_string_lossy().into_owned());
    let shipped_only = a.opt_bool("shipped_only")?.unwrap_or(false);
    let policy = PackPolicy {
        workshop: a.opt_bool("workshop")?.unwrap_or(false),
        mounted: a.opt_bool("mounted")?.unwrap_or(false),
    };
    let filter = a.opt_str("filter", MAX_NAME)?.map(|f| f.trim().replace('\\', "/").to_ascii_lowercase());
    let limit = a.opt_u64("limit", 1, hp_mcp::tools::MAX_LIST)?.unwrap_or(100) as usize;
    let offset = a.opt_u64("offset", 0, u64::from(u32::MAX))?.unwrap_or(0) as usize;
    Ok(Worker::spawn("get_dependency_report", move || {
        let cache = hp_fs::deps::DepsCache::new();
        let opts = ResolveOptions { map_name: &map_name, ..ResolveOptions::default() };
        let graph = fs.resolve_dependencies(&roots, &opts, &cache);
        let keep = |p: &str| filter.as_deref().is_none_or(|f| hp_editor::glob_match(f, p));
        let files: Vec<Value> = graph
            .nodes
            .iter()
            .filter(|n| keep(&n.vpath))
            .filter(|n| !shipped_only || n.ships(policy))
            .map(|n| {
                json!({
                    "path": n.vpath,
                    "kind": format!("{:?}", n.kind).to_ascii_lowercase(),
                    "source": n.source_label,
                    "origin": n.origin.map(|o| format!("{o:?}").to_ascii_lowercase()),
                    "workshop_id": n.workshop_id,
                    "in_base_game": n.in_base,
                    "bytes": n.size,
                    "shipped": n.ships(policy),
                    "used_by": n.reasons.iter().take(3).map(|r| format!("{r:?}")).collect::<Vec<_>>(),
                })
            })
            .collect();
        let total = files.len();
        let shipped_bytes: u64 = graph.shipped(policy).map(|n| n.size).sum();
        let missing: Vec<Value> = graph
            .missing
            .iter()
            .take(500)
            .map(|m| {
                json!({
                    "kind": format!("{:?}", m.kind).to_ascii_lowercase(),
                    "name": m.name,
                    "used_by": m.reasons.iter().take(3).map(|r| format!("{r:?}")).collect::<Vec<_>>(),
                })
            })
            .collect();
        ToolResult::json(json!({
            "roots": roots.len(),
            "total": total,
            "files": files.into_iter().skip(offset).take(limit).collect::<Vec<_>>(),
            "shipped_files": graph.shipped(policy).count(),
            "shipped_bytes": shipped_bytes,
            "by_origin": graph.by_origin().iter().map(|o| json!({
                "origin": o.origin.map(|x| format!("{x:?}").to_ascii_lowercase()),
                "files": o.files,
                "bytes": o.bytes,
            })).collect::<Vec<_>>(),
            "workshop_items": graph.workshop_items().iter().map(|(id, label)| json!({ "id": id, "label": label })).collect::<Vec<_>>(),
            "missing": missing,
            "missing_total": graph.missing.len(),
            "warnings": graph.warnings.iter().take(100).map(|w| format!("{w:?}")).collect::<Vec<_>>(),
        }))
    }))
}

fn groups_arg(a: &Args<'_>) -> R<Vec<Group>> {
    a.strings("groups", 5)?
        .iter()
        .map(|g| {
            Group::from_id(&g.trim().to_ascii_lowercase())
                .ok_or_else(|| format!("unknown group {g} (materials, models, sounds, particles, other)"))
        })
        .collect()
}

fn folder_arg(a: &Args<'_>, key: &str) -> R<PathBuf> {
    let raw = a.str(key, 1024)?.trim();
    let path = PathBuf::from(raw);
    if raw.is_empty() || !path.is_absolute() {
        return Err(format!("{key} must be an absolute folder path"));
    }
    Ok(path)
}

fn kind_id(kind: hp_fs::deps::DepKind) -> String {
    format!("{kind:?}").to_ascii_lowercase()
}

fn file_json(tree: &AssetTree, item: Item) -> Value {
    match tree.node(item) {
        Some(n) => json!({
            "path": n.vpath,
            "kind": kind_id(n.kind),
            "from": mapassets::origin_label(n),
            "origin": n.origin.map(|o| format!("{o:?}").to_ascii_lowercase()),
            "source": n.source_label,
            "bytes": n.size,
            "base_game": mapassets::is_base(n),
        }),
        None => json!({ "path": tree.name(item), "kind": kind_id(tree.kind(item)), "missing": true }),
    }
}

fn list_map_assets(state: &mut AppState, a: &Args<'_>) -> R<Outcome> {
    let show = match a.opt_enum("show", &["custom", "all", "missing"])?.as_deref() {
        Some(id) => Show::from_id(id).unwrap_or_default(),
        None => Show::Custom,
    };
    let chosen = groups_arg(a)?;
    let mut groups = [chosen.is_empty(); 5];
    for g in chosen {
        groups[g.index()] = true;
    }
    let text = a.opt_str("filter", MAX_NAME)?.unwrap_or("").to_owned();
    let with_files = a.opt_bool("files")?.unwrap_or(true);
    let limit = a.opt_u64("limit", 1, hp_mcp::tools::MAX_LIST)?.unwrap_or(100) as usize;
    let offset = a.opt_u64("offset", 0, u64::from(u32::MAX))?.unwrap_or(0) as usize;
    let input = mapassets::ResolveInput::of(state, mapassets::new_cache())?;
    Ok(Worker::spawn("list_map_assets", move || {
        let tree = match mapassets::resolve_tree(&input) {
            Ok(t) => t,
            Err(e) => return ToolResult::error(format!("list_map_assets: {e}")),
        };
        let filter = Filter { show, groups, text };
        let visible = tree.visible(&filter);
        let base_game = show == Show::All;
        let by_group: Vec<Value> = Group::ALL
            .iter()
            .filter(|g| filter.groups[g.index()])
            .map(|&g| {
                let of_group: Vec<usize> = visible.iter().copied().filter(|&i| tree.entries[i].group == g).collect();
                let t = tree.totals(&of_group, base_game);
                json!({
                    "group": g.id(),
                    "entries": of_group.len(),
                    "files": t.files,
                    "bytes": t.bytes,
                    "missing": t.missing,
                })
            })
            .collect();
        let totals = tree.totals(&visible, base_game);
        let entries: Vec<Value> = visible
            .iter()
            .skip(offset)
            .take(limit)
            .map(|&i| {
                let e = &tree.entries[i];
                let mut v = json!({
                    "path": tree.name(e.item),
                    "group": e.group.id(),
                    "kind": kind_id(tree.kind(e.item)),
                    "custom": e.custom,
                    "missing": e.missing,
                    "custom_bytes": e.custom_bytes,
                    "bytes": e.all_bytes,
                });
                if let Some(n) = tree.node(e.item) {
                    v["from"] = json!(mapassets::origin_label(n));
                    v["source"] = json!(n.source_label);
                }
                if with_files {
                    v["files"] = json!(e.closure.iter().skip(1).map(|&f| file_json(&tree, f)).collect::<Vec<_>>());
                }
                v
            })
            .collect();
        ToolResult::json(json!({
            "show": show.id(),
            "total": visible.len(),
            "groups": by_group,
            "files_to_collect": totals.files,
            "bytes_to_collect": totals.bytes,
            "missing": totals.missing,
            "entries": entries,
        }))
    }))
}

fn collect_map_assets(state: &mut AppState, a: &Args<'_>) -> R<ToolResult> {
    let output = folder_arg(a, "output")?;
    let groups: Vec<String> = groups_arg(a)?.into_iter().map(|g| g.id().to_owned()).collect();
    let req = AssetCollectRequest {
        output: output.clone(),
        base_game: a.opt_bool("base_game")?.unwrap_or(false),
        replace: a.opt_bool("replace")?.unwrap_or(false),
        groups,
        filter: a.opt_str("filter", MAX_NAME)?.unwrap_or("").trim().to_owned(),
    };
    if let Some(problem) = mapassets::output_problem(state, &output) {
        return Err(problem);
    }
    let rt = &mut state.runtime.content.map_assets;
    if rt.progress.is_some() || rt.collect_request.is_some() {
        return Err("a collect is already running (get_map_assets_status, cancel_collect_map_assets)".to_owned());
    }
    rt.collect_request = Some(req);
    state.settings.content.show_map_assets = true;
    state.settings_dirty = true;
    state.ui.right_tab = RightTab::Package(mapassets::TAB);
    Ok(ToolResult::json(json!({
        "started": true,
        "output": output.display().to_string(),
        "note": "The Map assets tab runs it: follow it with get_map_assets_status.",
    })))
}

fn get_map_assets_status(state: &mut AppState, _a: &Args<'_>) -> R<ToolResult> {
    let rt = &state.runtime.content.map_assets;
    let last = rt.last_collect.as_ref().map(|s| {
        json!({
            "ok": s.error.is_none() && !s.cancelled,
            "error": s.error,
            "cancelled": s.cancelled,
            "output": s.output.display().to_string(),
            "copied": s.copied,
            "replaced": s.replaced,
            "already_there": s.same,
            "kept": s.skipped,
            "failed": s.failed.iter().take(200).map(|(p, e)| json!({ "path": p, "error": e })).collect::<Vec<_>>(),
            "failed_total": s.failed.len(),
            "missing": s.missing.iter().take(200).collect::<Vec<_>>(),
            "missing_total": s.missing.len(),
            "bytes_written": s.bytes,
            "report": s.report.as_ref().map(|p| p.display().to_string()),
        })
    });
    Ok(ToolResult::json(json!({
        "running": rt.progress.map(|(done, total)| json!({ "done": done, "total": total })),
        "starting": rt.collect_request.is_some(),
        "last_collect": last,
    })))
}

fn cancel_collect_map_assets(state: &mut AppState, _a: &Args<'_>) -> R<ToolResult> {
    let rt = &mut state.runtime.content.map_assets;
    let running = rt.progress.is_some() || rt.collect_request.is_some();
    rt.collect_request = None;
    if running {
        rt.cancel_requested = true;
    }
    Ok(ToolResult::json(json!({ "cancelled": running })))
}

fn find_unused_assets(state: &mut AppState, a: &Args<'_>) -> R<Outcome> {
    let folder = folder_arg(a, "folder")?;
    if !folder.is_dir() {
        return Err(format!("{} is not a folder", folder.display()));
    }
    let limit = a.opt_u64("limit", 1, hp_mcp::tools::MAX_LIST)?.unwrap_or(100) as usize;
    let offset = a.opt_u64("offset", 0, u64::from(u32::MAX))?.unwrap_or(0) as usize;
    let input = mapassets::ResolveInput::of(state, mapassets::new_cache())?;
    Ok(Worker::spawn("find_unused_assets", move || {
        let report = mapassets::resolve_tree(&input).and_then(|tree| mapassets::unused_report(&tree, &folder));
        match report {
            Ok(r) => ToolResult::json(json!({
                "root": r.root.display().to_string(),
                "folders": r.folders,
                "scanned": r.scanned,
                "used": r.used,
                "unused_total": r.unused.len(),
                "unused_bytes": r.unused_bytes,
                "unused": r.unused.iter().skip(offset).take(limit).map(|u| json!({ "path": u.path, "bytes": u.bytes })).collect::<Vec<_>>(),
                "note": "Report only: nothing was deleted.",
            })),
            Err(e) => ToolResult::error(format!("find_unused_assets: {e}")),
        }
    }))
}

fn release_json(state: &AppState) -> Value {
    let c = &state.runtime.content;
    let path = |p: &Option<PathBuf>| p.as_ref().map(|p| p.display().to_string());
    json!({
        "window_open": c.release_open,
        "running": c.progress.as_ref().map(|(step, f)| json!({ "step": step, "fraction": f })),
        "starting": c.start_requested.is_some(),
        "cancel_requested": c.cancel_requested,
        "last_release": c.last_release.as_ref().map(|r| json!({
            "ok": r.error.is_none(),
            "error": r.error,
            "output_folder": path(&r.out_root),
            "addon_folder": path(&r.addon_dir),
            "gma": path(&r.gma),
            "files": r.files,
            "bytes": r.bytes,
            "gma_bytes": r.gma_bytes,
            "duration_ms": r.duration_ms,
            "protected_server_bsp": path(&r.protected_server_bsp),
            "log_tail": r.log_tail,
        })),
    })
}

fn gmod_only(state: &AppState) -> R<()> {
    if state.settings.game.is_gmod() {
        Ok(())
    } else {
        Err(format!(
            "the release tools (GMA addon, Workshop) are for Garry's Mod; this map is for {} (compile it with compile_map)",
            state.settings.game.active_game()
        ))
    }
}

fn start_release(state: &mut AppState, a: &Args<'_>) -> R<ToolResult> {
    gmod_only(state)?;
    let analyze_only = a.opt_bool("analyze_only")?.unwrap_or(false);
    if !analyze_only && a.opt_bool("confirm")? != Some(true) {
        return Err("a build compiles and writes the content folder and the GMA: set confirm to true".to_owned());
    }
    if state.doc.path().is_none() {
        return Err("save the map first (save_map with a name)".to_owned());
    }
    if state.runtime.content.progress.is_some() || state.runtime.content.start_requested.is_some() {
        return Err("a release is already running (get_release_status, cancel_release)".to_owned());
    }
    if let Some(session) = a.opt_bool("game_session")? {
        if session && analyze_only {
            return Err("game_session is for a build, not analyze_only".to_owned());
        }
        if session && hp_compile::sandbox::is_sandboxed() {
            return Err(format!(
                "game_session starts the game: refused in this automated run: {}",
                hp_compile::sandbox::REFUSED
            ));
        }
        state.settings.content.game_session = session;
        state.settings_dirty = true;
    }
    if let Some(keep) = a.opt_bool("keep_cubemaps")? {
        state.settings.content.keep_cubemaps = keep;
        state.settings_dirty = true;
    }
    if let Some(protect) = a.opt_bool("protect")? {
        state.settings.content.protect = protect;
        state.settings_dirty = true;
    }
    if let Some(mark) = a.opt_str("watermark", MAX_NAME)? {
        state.settings.content.protect_watermark = mark.chars().filter(|c| !c.is_control()).take(120).collect();
        state.settings_dirty = true;
    }
    state.runtime.content.start_requested = Some(analyze_only);
    state.runtime.content.release_open = true;
    Ok(ToolResult::json(json!({
        "started": if analyze_only { "analysis" } else { "build" },
        "note": "Follow it with get_release_status.",
    })))
}

fn protect_json(state: &AppState) -> Value {
    let c = &state.settings.content;
    json!({
        "protect": c.protect,
        "watermark": c.protect_watermark,
        "strip_original_faces": c.protect_strip_original_faces,
        "randomize_materials": c.protect_randomize,
        "lock": c.protect_lock,
        "lock_key_set": !c.protect_lock_key.trim().is_empty(),
        "lock_block": c.protect_lock_block,
        "shuffle_lumps": c.protect_shuffle_lumps,
        "junk_gaps": c.protect_junk_gaps,
        "decoys": c.protect_decoys,
        "scramble_header": c.protect_scramble_header,
        "shuffle_game_lumps": c.protect_shuffle_game_lumps,
        "random_pak": c.protect_random_pak,
        "reorder_tables": c.protect_reorder_tables,
        "reorder_world": c.protect_reorder_world,
        "layout_seed": c.protect_layout_seed,
    })
}

fn get_protect_options(state: &mut AppState, _a: &Args<'_>) -> R<ToolResult> {
    Ok(ToolResult::json(protect_json(state)))
}

fn set_protect_options(state: &mut AppState, a: &Args<'_>) -> R<ToolResult> {
    let keys = [
        "preset",
        "protect",
        "watermark",
        "strip_original_faces",
        "randomize_materials",
        "lock",
        "lock_key",
        "lock_block",
        "shuffle_lumps",
        "junk_gaps",
        "decoys",
        "scramble_header",
        "shuffle_game_lumps",
        "random_pak",
        "reorder_tables",
        "reorder_world",
        "layout_seed",
    ];
    if !keys.iter().any(|k| a.get(k).is_some()) {
        return Err("give at least one option to change".to_owned());
    }
    let mut next = state.settings.content.clone();
    if let Some(preset) = a.opt_str("preset", 16)? {
        let on = match preset {
            "strongest" => true,
            "none" => false,
            other => return Err(format!("preset must be strongest or none, not {other}")),
        };
        next.protect_shuffle_lumps = on;
        next.protect_junk_gaps = on;
        next.protect_decoys = on;
        next.protect_scramble_header = on;
        next.protect_shuffle_game_lumps = on;
        next.protect_random_pak = on;
        next.protect_reorder_tables = on;
        next.protect_reorder_world = on;
    }
    let flag = |key: &str, slot: &mut bool| -> R<()> {
        if let Some(v) = a.opt_bool(key)? {
            *slot = v;
        }
        Ok(())
    };
    flag("protect", &mut next.protect)?;
    flag("strip_original_faces", &mut next.protect_strip_original_faces)?;
    flag("randomize_materials", &mut next.protect_randomize)?;
    flag("lock", &mut next.protect_lock)?;
    flag("lock_block", &mut next.protect_lock_block)?;
    flag("shuffle_lumps", &mut next.protect_shuffle_lumps)?;
    flag("junk_gaps", &mut next.protect_junk_gaps)?;
    flag("decoys", &mut next.protect_decoys)?;
    flag("scramble_header", &mut next.protect_scramble_header)?;
    flag("shuffle_game_lumps", &mut next.protect_shuffle_game_lumps)?;
    flag("random_pak", &mut next.protect_random_pak)?;
    flag("reorder_tables", &mut next.protect_reorder_tables)?;
    flag("reorder_world", &mut next.protect_reorder_world)?;
    if let Some(mark) = a.opt_str("watermark", MAX_NAME)? {
        next.protect_watermark = mark.chars().filter(|c| !c.is_control()).take(120).collect();
    }
    if let Some(key) = a.opt_str("lock_key", MAX_NAME)? {
        next.protect_lock_key = key.to_owned();
    }
    if let Some(seed) = a.opt_str("layout_seed", MAX_NAME)? {
        next.protect_layout_seed = seed.chars().filter(|c| !c.is_control()).take(120).collect();
    }
    state.settings.content = next;
    state.settings_dirty = true;
    Ok(ToolResult::json(protect_json(state)))
}

fn get_release_checklist(state: &mut AppState, a: &Args<'_>) -> R<ToolResult> {
    if state.doc.path().is_none() {
        return Err("save the map first (save_map with a name)".to_owned());
    }
    let refresh = a.opt_bool("refresh")?.unwrap_or(false);
    let c = &mut state.runtime.content;
    match (&c.checklist, refresh) {
        (Some(rows), false) => {
            let rows: Vec<Value> =
                rows.iter().map(|r| json!({ "label": r.label, "status": r.status, "detail": r.detail })).collect();
            let ready = rows.iter().all(|r| r["status"] != "failed");
            Ok(ToolResult::json(json!({ "gathering": false, "ready_to_release": ready, "rows": rows })))
        }
        _ => {
            if refresh {
                c.checklist = None;
            }
            c.checklist_requested = true;
            Ok(ToolResult::json(json!({ "gathering": true, "note": "Call get_release_checklist again in a moment." })))
        }
    }
}

fn open_release_window(state: &mut AppState, a: &Args<'_>) -> R<ToolResult> {
    gmod_only(state)?;
    let tab: &'static str = match a.opt_enum("tab", &["build", "report", "checklist", "addon", "publish"])?.as_deref() {
        Some("report") => "report",
        Some("checklist") => "checklist",
        Some("addon") => "addon",
        Some("publish") => return Err("use publish_workshop for the Publish tab".to_owned()),
        _ => "build",
    };
    if state.doc.path().is_none() {
        return Err("save the map first (save_map with a name)".to_owned());
    }
    state.runtime.content.release_open = true;
    state.runtime.content.release_tab = Some(tab);
    Ok(ToolResult::json(release_json(state)))
}

fn content_index_json(state: &AppState) -> Value {
    let stats = hp_fs::scan_stats();
    let drives: Vec<Value> = stats
        .volumes
        .iter()
        .map(|(volume, penalty)| {
            let kind = match penalty {
                Some(true) => "rotational",
                Some(false) => "solid state",
                None => "unknown (treated as solid state)",
            };
            json!({ "volume": volume, "type": kind })
        })
        .collect();
    json!({
        "use_index": state.settings.scan.use_index,
        "mode": state.settings.scan.mode.to_fs().as_str(),
        "index_file": state.settings.scan.use_index.then(hp_app::settings::ScanSettings::index_file),
        "index_state": stats.index_state,
        "index_entries": stats.index_entries,
        "index_bytes": stats.index_bytes,
        "folders_checked": stats.dirs_checked,
        "folders_reused": stats.dirs_reused,
        "folders_scanned": stats.dirs_rescanned,
        "folder_scans": stats.walks,
        "last_folder_scan_ms": stats.last_walk_ms,
        "drives": drives,
        "mounting": state.assets.status() == hp_app::state::AssetStatus::Mounting,
    })
}

fn get_content_index(state: &mut AppState, _a: &Args<'_>) -> R<ToolResult> {
    Ok(ToolResult::json(content_index_json(state)))
}

fn rebuild_content_index(state: &mut AppState, _a: &Args<'_>) -> R<ToolResult> {
    hp_fs::clear_index();
    state.assets.remount(&state.settings);
    Ok(ToolResult::json(json!({ "rebuilding": true, "note": "Follow it with get_content_index." })))
}

fn get_release_status(state: &mut AppState, _a: &Args<'_>) -> R<ToolResult> {
    Ok(ToolResult::json(release_json(state)))
}

fn cancel_release(state: &mut AppState, _a: &Args<'_>) -> R<ToolResult> {
    if state.runtime.content.progress.is_none() {
        return Ok(ToolResult::json(json!({ "cancelled": false, "note": "no release is running" })));
    }
    state.runtime.content.cancel_requested = true;
    Ok(ToolResult::json(json!({ "cancelled": true })))
}

fn inspect_archive(state: &mut AppState, a: &Args<'_>) -> R<ToolResult> {
    let raw = a.str("file", 255)?;
    let ext = std::path::Path::new(raw.trim())
        .extension()
        .and_then(|e| e.to_str())
        .map(str::to_ascii_lowercase)
        .filter(|e| ["gma", "vpk", "bsp", "bin"].contains(&e.as_str()))
        .ok_or("file must be a .gma, .vpk, .bsp or .bin archive")?;
    let rel = relative_file(raw, &ext)?;
    let path: PathBuf = map_dir(state)?.join(rel);
    if !path.is_file() {
        return Err(format!("no file {raw} in the map folder"));
    }
    state.runtime.content.inspector_request = Some(path.clone());
    state.runtime.content.inspector_open = true;
    Ok(ToolResult::json(json!({ "inspecting": path.display().to_string() })))
}

fn publish_workshop(state: &mut AppState, a: &Args<'_>) -> R<ToolResult> {
    gmod_only(state)?;
    if a.opt_bool("confirm")? != Some(true) {
        return Err("publish only when the user asked for it (confirm: true)".to_owned());
    }
    if hp_compile::sandbox::is_sandboxed() {
        return Err(format!("refused in this automated run: {}", hp_compile::sandbox::REFUSED));
    }
    if state.doc.path().is_none() {
        return Err("save the map first".to_owned());
    }
    state.request(Action::PublishWorkshop);
    Ok(ToolResult::json(json!({
        "opened": "publish",
        "note": "The Publish tab is open: the user reviews and confirms the upload there.",
    })))
}

#[cfg(test)]
mod tests {
    use serde_json::json;

    use super::super::semantic_tests::{err, ok};

    #[test]
    fn content_index_tools() {
        let mut s = hp_app::testing::test_state();
        let v = ok(&mut s, "get_content_index", json!({}));
        assert_eq!(v["use_index"], true);
        assert_eq!(v["mode"], "auto");
        assert!(v["drives"].is_array());
        assert!(v["folders_checked"].is_number());
        s.settings.scan.use_index = false;
        let v = ok(&mut s, "get_content_index", json!({}));
        assert!(v["index_file"].is_null());
        assert_eq!(ok(&mut s, "rebuild_content_index", json!({}))["rebuilding"], true);
    }

    #[test]
    fn release_controls() {
        let mut s = hp_app::testing::test_state();
        err(&mut s, "get_dependency_report", json!({}));
        err(&mut s, "open_release_window", json!({}));
        let dir = std::env::temp_dir().join(format!("hp_mcp_content_{}", std::process::id()));
        std::fs::create_dir_all(&dir).unwrap();
        s.doc.set_path(Some(dir.join("m.vmf")));
        let r = ok(&mut s, "open_release_window", json!({ "tab": "checklist" }));
        assert_eq!(r["window_open"], true);
        assert_eq!(s.runtime.content.release_tab, Some("checklist"));
        err(&mut s, "open_release_window", json!({ "tab": "publish" }));
        ok(&mut s, "get_release_status", json!({}));
        assert_eq!(ok(&mut s, "cancel_release", json!({}))["cancelled"], false);
        err(&mut s, "inspect_archive", json!({ "file": "m.txt" }));
        err(&mut s, "inspect_archive", json!({ "file": "../m.gma" }));
        std::fs::write(dir.join("m.gma"), b"GMAD").unwrap();
        ok(&mut s, "inspect_archive", json!({ "file": "m.gma" }));
        assert!(s.runtime.content.inspector_request.is_some());
        err(&mut s, "publish_workshop", json!({ "confirm": false }));
        err(&mut s, "set_protect_options", json!({}));
        err(&mut s, "set_protect_options", json!({ "preset": "weak" }));
        err(&mut s, "set_protect_options", json!({ "decoys": "yes" }));
        let p = ok(&mut s, "set_protect_options", json!({ "preset": "none", "decoys": true, "layout_seed": "abc" }));
        assert_eq!(
            (p["decoys"].clone(), p["shuffle_lumps"].clone(), p["layout_seed"].clone()),
            (json!(true), json!(false), json!("abc"))
        );
        assert!(s.settings.content.protect_decoys && !s.settings.content.protect_reorder_world);
        let p =
            ok(&mut s, "set_protect_options", json!({ "preset": "strongest", "random_pak": false, "lock_key": "k" }));
        assert_eq!(
            (p["reorder_world"].clone(), p["random_pak"].clone(), p["lock_key_set"].clone()),
            (json!(true), json!(false), json!(true))
        );
        assert_eq!(ok(&mut s, "get_protect_options", json!({}))["shuffle_game_lumps"], json!(true));
        err(&mut s, "start_release", json!({}));
        err(&mut s, "start_release", json!({ "analyze_only": true, "game_session": true }));
        assert_eq!(s.runtime.content.start_requested, None);
        assert_eq!(
            ok(&mut s, "start_release", json!({ "analyze_only": true, "keep_cubemaps": false, "game_session": false }))
                ["started"],
            "analysis"
        );
        assert!(!s.settings.content.keep_cubemaps && !s.settings.content.game_session);
        s.runtime.content.start_requested = None;
        assert_eq!(ok(&mut s, "start_release", json!({ "analyze_only": true }))["started"], "analysis");
        assert_eq!(s.runtime.content.start_requested, Some(true));
        err(&mut s, "start_release", json!({ "confirm": true }));
        assert_eq!(ok(&mut s, "get_release_status", json!({}))["starting"], true);
        assert_eq!(ok(&mut s, "get_release_checklist", json!({}))["gathering"], true);
        assert!(s.runtime.content.checklist_requested);
        s.runtime.content.checklist =
            Some(vec![hp_app::runtime::ChecklistRow { label: "Saved".into(), status: "ok", detail: String::new() }]);
        let c = ok(&mut s, "get_release_checklist", json!({}));
        assert_eq!((c["gathering"].clone(), c["ready_to_release"].clone()), (json!(false), json!(true)));
        s.runtime.content.last_release =
            Some(hp_app::runtime::ReleaseReport { error: Some("leak".into()), ..Default::default() });
        assert_eq!(ok(&mut s, "get_release_status", json!({}))["last_release"]["ok"], false);
        let _ = std::fs::remove_dir_all(&dir);
    }

    #[test]
    fn map_assets_tools() {
        let mut s = hp_app::testing::test_state();
        err(&mut s, "list_map_assets", json!({}));
        err(&mut s, "collect_map_assets", json!({ "output": "relative" }));
        err(&mut s, "find_unused_assets", json!({ "folder": "relative" }));
        err(&mut s, "list_map_assets", json!({ "groups": ["nope"] }));
        assert_eq!(ok(&mut s, "cancel_collect_map_assets", json!({}))["cancelled"], false);
        assert_eq!(ok(&mut s, "get_map_assets_status", json!({}))["last_collect"], serde_json::Value::Null);
        let dir = std::env::temp_dir().join(format!("hp_mcp_map_assets_{}", std::process::id()));
        let _ = std::fs::remove_dir_all(&dir);
        let write = |rel: &str, bytes: &[u8]| {
            let p = dir.join(rel);
            std::fs::create_dir_all(p.parent().unwrap()).unwrap();
            std::fs::write(p, bytes).unwrap();
        };
        write("gmod/garrysmod/gameinfo.txt", b"GameInfo { FileSystem { SearchPaths { game garrysmod } } }");
        write(
            "gmod/garrysmod/materials/dev/dev_measuregeneric01b.vmt",
            b"\"LightmappedGeneric\" { \"$basetexture\" \"dev/measure\" }",
        );
        write("gmod/garrysmod/materials/dev/measure.vtf", b"VTF\0");
        write("addon/materials/dev/old.vtf", b"x");
        let opts = hp_fs::MountOptions { mount_workshop: false, mount_cfg: false, ..Default::default() };
        let fs = hp_fs::GameFs::for_gmod(&dir.join("gmod"), &opts).unwrap();
        s.assets = Box::new(hp_app::testing::MockAssets::with_fs(fs));
        let list = ok(&mut s, "list_map_assets", json!({ "groups": ["materials"], "filter": "measure" }));
        assert_eq!(list["total"], 1, "{list}");
        let entry = &list["entries"][0];
        assert_eq!(entry["path"], "materials/dev/dev_measuregeneric01b.vmt");
        assert_eq!(entry["files"][0]["path"], "materials/dev/measure.vtf");
        assert_eq!(list["groups"][0]["group"], "materials");
        let missing = ok(&mut s, "list_map_assets", json!({ "show": "missing", "files": false }));
        assert!(missing["total"].as_u64().unwrap() >= 1, "{missing}");
        let unused = ok(&mut s, "find_unused_assets", json!({ "folder": dir.join("addon").display().to_string() }));
        assert_eq!(unused["unused"][0]["path"], "materials/dev/old.vtf");
        err(&mut s, "collect_map_assets", json!({ "output": dir.join("gmod/garrysmod").display().to_string() }));
        let out = dir.join("out");
        let r =
            ok(&mut s, "collect_map_assets", json!({ "output": out.display().to_string(), "groups": ["materials"] }));
        assert_eq!(r["started"], true);
        assert!(s.settings.content.show_map_assets);
        err(&mut s, "collect_map_assets", json!({ "output": out.display().to_string() }));
        assert_eq!(ok(&mut s, "get_map_assets_status", json!({}))["starting"], true);
        let mut panel = hp_ui_mapping::mapassets::MapAssetsPanel::new();
        let ctx = egui::Context::default();
        let start = std::time::Instant::now();
        while s.runtime.content.map_assets.last_collect.is_none() {
            assert!(start.elapsed() < std::time::Duration::from_secs(60));
            panel.tick(&ctx, &mut s, false);
            std::thread::sleep(std::time::Duration::from_millis(10));
        }
        let status = ok(&mut s, "get_map_assets_status", json!({}));
        assert_eq!(status["last_collect"]["ok"], true, "{status}");
        assert!(status["last_collect"]["copied"].as_u64().unwrap() >= 2);
        assert!(out.join("materials/dev/measure.vtf").is_file());
        assert!(status["last_collect"]["report"].as_str().is_some_and(|p| std::path::Path::new(p).is_file()));
        let _ = std::fs::remove_dir_all(&dir);
    }
}
