use hp_app::AppState;
use hp_mcp::protocol::ToolResult;
use serde_json::{Value, json};

use super::semantic::*;
use super::tools::{Args, R};
use crate::livelink;

domain! { "live link";
    live_link_start: Editor, "Start the Garry's Mod live link",
        "Start the live link with Garry's Mod (Tools > Integrations > Start the live link), the tool that shows \
        entity edits in the running game without a recompile. It installs the small Lua addon \
        garrysmod/addons/hammerpro_livelink (removable with live_link_stop remove_addon), opens a session \
        (garrysmod/data/hammerpro_livelink, a random token, localhost files only, the addon obeys only while \
        the session is alive) and takes the map as it is now as the state of the game. Then compile and run \
        the map once (compile_map, launch_game); a game already running loads the addon by itself (setting \
        livelink.load_into_running_game). While it is on: moved, rotated, created and deleted props and point \
        entities and their keyvalues (color, skin, model, renderfx, sounds and others the engine accepts) and \
        new outputs are applied in game at once; prop_static changes show as client side models until the \
        next compile; light and light_spot changes show as a dynamic light or projected texture marked as a \
        preview; brush and lighting changes are listed as needing a compile. The editor selection is \
        highlighted in game. The optional flags change the livelink settings first. Returns live_link_status.",
        {
            "camera_to_game": boolean("The player follows the 3D camera (setting livelink.camera_to_game)."),
            "camera_from_game": boolean("The 3D camera follows the player (setting livelink.camera_from_game)."),
            "highlight_selection": boolean("Highlight the editor selection in game."),
            "light_previews": boolean("Preview light changes in game."),
        }, [];
    live_link_stop: Editor, "Stop the Garry's Mod live link",
        "Stop the live link: the session ends, the addon stops obeying and clears its previews and \
        highlights in game (entities already moved stay as they are until the map reloads). remove_addon also \
        deletes the addon folder garrysmod/addons/hammerpro_livelink and the files of the link in \
        garrysmod/data/hammerpro_livelink (this works when the link is off too). Returns live_link_status.",
        {
            "remove_addon": boolean("Also remove the addon from the Garry's Mod folder."),
        }, [];
    live_link_status: Read, "Live link status",
        "The state of the Garry's Mod live link: on or off, state (off, waiting, connected, other_map, lost), \
        map open here and map running in game, how the addon was loaded (mounted with the game or injected \
        into the running game), latency of the last applied change and its average in ms, operations waiting \
        to be applied, entities changed since the compile, needs_compile reasons (brushes, lighting, \
        static_props, brush_entities, removed_outputs, class_changed, not_in_game), map entities found in game \
        (matched_by: how many by Hammer id, name, origin and brush order) and those not found, changes not sent and why, what the game shows (entities created, static prop and \
        light previews), the player eye when the 3D camera follows the player, the last errors from the game, \
        the livelink settings and whether the addon is installed.",
        {}, [];
    live_link_push: Editor, "Push changes to the game",
        "Send the pending edits to the game now (they are sent by themselves within a frame or two; this \
        forces it and returns the status). resync clears what the game was told and sends every change since \
        the compile again (after the game was reloaded by hand, or when an entity looks wrong). mark_compiled \
        makes the map as it is now the state of the game (use it after compiling and reloading outside Hammer \
        Pro). Fails when the link is off.",
        {
            "resync": boolean("Send every change since the compile again."),
            "mark_compiled": boolean("Count the map as it is now as what the game runs."),
        }, [];
    live_link_fire: Editor, "Fire an input in game",
        "Fire an entity input in the running game to test logic, like the Fire input in game window: target is \
        a target name of the map (wildcards ending in * work), !player, or # followed by an entity id of the \
        map (found through the entity matching), input is the input name (Open, Toggle, TurnOn, Trigger, \
        SetAnimation...), param its parameter, delay in seconds. Inputs that run code or console commands \
        (Command, RunCode, RunPassedCode) and entities that run them are refused. Needs the link connected.",
        {
            "target": string("Target name, !player or #<entity id>."),
            "input": string("Input name."),
            "param": string("Parameter of the input (default empty)."),
            "delay": number("Delay in seconds (default 0).", 0.0, 3600.0),
        }, ["target", "input"];
    live_link_teleport: Editor, "Teleport the player",
        "Move the player of the running game: by default to the 3D camera of the editor (eye at the camera, \
        same direction), or to origin (eye position) and angles (pitch yaw roll) when given; noclip (default \
        true) keeps the player floating there. back sends the player back where they were before the first \
        teleport. Needs the link connected.",
        {
            "origin": vec3("Eye position; default the 3D camera."),
            "angles": vec3("Pitch, yaw, roll in degrees; default the 3D camera direction."),
            "noclip": boolean("Put the player in noclip (default true)."),
            "back": boolean("Send the player back instead."),
        }, [];
}

fn status(state: &AppState) -> ToolResult {
    ToolResult::json(livelink::status_json(state))
}

fn with_note(state: &AppState, note: String) -> ToolResult {
    let mut v: Value = livelink::status_json(state);
    v["message"] = json!(note);
    ToolResult::json(v)
}

fn live_link_start(state: &mut AppState, a: &Args<'_>) -> R<ToolResult> {
    let s = &mut state.settings.livelink;
    let before = s.clone();
    if let Some(v) = a.opt_bool("camera_to_game")? {
        s.camera_to_game = v;
    }
    if let Some(v) = a.opt_bool("camera_from_game")? {
        s.camera_from_game = v;
    }
    if s.camera_to_game && s.camera_from_game {
        return Err("camera_to_game and camera_from_game cannot both be on".to_owned());
    }
    if let Some(v) = a.opt_bool("highlight_selection")? {
        s.highlight_selection = v;
    }
    if let Some(v) = a.opt_bool("light_previews")? {
        s.light_previews = v;
    }
    if *s != before {
        state.settings_dirty = true;
    }
    let note = livelink::start(state)?;
    Ok(with_note(state, note))
}

fn live_link_stop(state: &mut AppState, a: &Args<'_>) -> R<ToolResult> {
    let note = if a.opt_bool("remove_addon")?.unwrap_or(false) {
        livelink::remove_addon(state)?
    } else if livelink::stop(state) {
        hp_app::t!("livelink.stopped").to_owned()
    } else {
        hp_app::t!("livelink.off").to_owned()
    };
    Ok(with_note(state, note))
}

fn live_link_status(state: &mut AppState, _a: &Args<'_>) -> R<ToolResult> {
    Ok(status(state))
}

fn live_link_push(state: &mut AppState, a: &Args<'_>) -> R<ToolResult> {
    if state.runtime.livelink.session.is_none() {
        return Err(hp_app::t!("livelink.off").to_owned());
    }
    let mut notes = Vec::new();
    if a.opt_bool("mark_compiled")?.unwrap_or(false) {
        notes.push(livelink::mark_compiled(state)?);
    }
    if a.opt_bool("resync")?.unwrap_or(false) {
        notes.push(livelink::resync(state)?);
    }
    livelink::push_now(state);
    Ok(with_note(state, notes.join(" ")))
}

fn live_link_fire(state: &mut AppState, a: &Args<'_>) -> R<ToolResult> {
    let target = a.str("target", 128)?;
    let input = a.str("input", 128)?;
    let param = a.opt_str("param", 512)?.unwrap_or("");
    let delay = a.opt_f64("delay", 0.0, 3600.0)?.unwrap_or(0.0);
    let note = livelink::fire(state, target, input, param, delay)?;
    Ok(with_note(state, note))
}

fn live_link_teleport(state: &mut AppState, a: &Args<'_>) -> R<ToolResult> {
    if a.opt_bool("back")?.unwrap_or(false) {
        let note = livelink::back(state)?;
        return Ok(with_note(state, note));
    }
    let (cam_o, cam_a) = livelink::camera_pose(state);
    let origin = a.opt_vec3("origin")?.map(|v| v.to_array()).unwrap_or(cam_o);
    let angles = a.opt_vec3("angles")?.map(|v| v.to_array()).unwrap_or(cam_a);
    let noclip = a.opt_bool("noclip")?.unwrap_or(true);
    let note = livelink::teleport(state, Some((origin, angles)), noclip)?;
    Ok(with_note(state, note))
}
