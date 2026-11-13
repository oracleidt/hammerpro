use glam::DVec3;
use hp_app::AppState;
use hp_app::runtime::soundscape::PreviewRequest;
use hp_kv::Block;
use hp_mcp::protocol::ToolResult;
use hp_soundscape::library::{FlatKind, Library, Origin};
use hp_soundscape::rules::{EmitterKind, decide};
use hp_soundscape::script::{self, Interval, Position, Rule};
use hp_soundscape::soundlevel;
use hp_ui_lab::soundscape::world;
use serde_json::{Map, Value, json};

use super::semantic::*;
use super::tools::{Args, R, edit};

const MAX_NAME: usize = 128;
const MAX_PATH: usize = 600;
const MAX_RULES: usize = 256;

domain! { "soundscapes";
    list_soundscapes: Read, "List the soundscapes",
        "The soundscapes the map can use: those of the game (scripts/soundscapes_manifest.txt and the files it lists, \
        read only) and those of the map file scripts/soundscapes_<map>.txt (editable with edit_soundscape), with \
        their rule counts, DSP and the entities using them, plus every env_soundscape, env_soundscape_proxy and \
        env_soundscape_triggerable of the map with its soundscape name, radius and whether the name exists. A map \
        soundscape with the name of a game soundscape overrides it (overrides_game). The editor window is the \
        action edit_soundscapes (Map > Soundscapes...).",
        {
            "search": string("Keep the soundscapes whose name contains every word."),
            "source": choice("Which soundscapes (default all).", &["all", "game", "map"]),
            "limit": integer("Maximum soundscapes returned (default 300).", 1, 5000),
        }, [];
    get_soundscape: Read, "Read a soundscape",
        "One soundscape with its keys (dsp, dsp_player, dsp_volume) and its rules as typed values: playlooping (volume, pitch, \
        wave, soundlevel, position), playrandom (time, volume, pitch, waves, soundlevel, position) and \
        playsoundscape (name, volume, position, positionoverride, ambientpositionoverride); ranges are [min, max]. Also the \
        KeyValues text, the flattened list of every sound it plays with its nested soundscapes (volumes \
        multiplied, positions overridden like the engine), missing nested names, whether each wave exists in the \
        mounted content, and the entities using it.",
        {
            "name": string("Soundscape name (case insensitive)."),
        }, ["name"];
    edit_soundscape: Edit, "Create, change or delete a map soundscape",
        "Edit the soundscape file of the map, scripts/soundscapes_<map>.txt under root (default: the folder above \
        the maps folder of the saved map, else the game folder), which the game loads for this map. Game \
        soundscape files are never written. A missing soundscape is created (from copy_from when given, which can \
        be a game soundscape). rules replaces every rule, add_rules appends; each rule is an object with type \
        (playlooping, playrandom or playsoundscape) and its keys: volume, pitch and time as a number, [min, max] \
        or \"min,max\"; wave (playlooping) or waves (playrandom, a list); soundlevel (a SNDLVL_ name or a dB \
        number); position (0 to 7, or random for playrandom); name, position (offset added to the positions of the \
        nested soundscape), positionoverride and ambientpositionoverride (playsoundscape). Like the engine, a \
        missing volume plays nothing, a playrandom without pitch is ignored and one without time repeats every frame. assign_to sets the \
        soundscape key of those env_soundscape or env_soundscape_triggerable entities (one undo step \
        MCP: edit_soundscape). Unsaved changes of the editor window are kept and written with the edit. Writing \
        the file is not part of the undo. dry_run returns the new file text without writing.",
        {
            "name": string("Soundscape name, for example gm_mymap.cave (no spaces, quotes or braces)."),
            "rules": serde_json::json!({ "type": "array", "items": { "type": "object", "additionalProperties": true }, "description": "Rules replacing every rule of the soundscape (see the description)." }),
            "add_rules": serde_json::json!({ "type": "array", "items": { "type": "object", "additionalProperties": true }, "description": "Rules appended to the soundscape." }),
            "dsp": integer("DSP room effect (0 none, 1 generic room...). -1 removes the key.", -1, 133),
            "dsp_player": integer("Player DSP effect on top of the room effect. -1 removes the key.", -1, 133),
            "dsp_volume": number("Volume of the room effect (1 by default). -1 removes the key.", -1.0, 2.0),
            "copy_from": string("Start a new soundscape as a copy of this one (game or map)."),
            "rename": string("New name for the soundscape (env_soundscape entities using the old name are updated)."),
            "delete": boolean("Delete the soundscape from the map file."),
            "assign_to": ids("env_soundscape or env_soundscape_triggerable entity ids that should use this soundscape."),
            "root": string("Absolute game or addon folder to write under (default: automatic)."),
            "dry_run": boolean("Return the result without writing anything."),
        }, ["name"];
    preview_soundscape: Editor, "Listen to a soundscape, record the audio",
        "Play a soundscape through the default audio device (Windows) with its nested soundscapes, its random \
        sounds at random times like the engine client, and its room effect (dsp), without distance (every sound \
        at its own volume), until stop. The volume and mute of the soundscape settings (lab.soundscape.volume, \
        lab.soundscape.mute) apply. Hearing in the player view is the setting lab.soundscape.hearing (action \
        toggle_soundscape_hearing, key K in the player view). record_to records what the soundscape output \
        really sends to the audio device (the preview or the player view hearing) for record_seconds into a 16 \
        bit 44100 Hz stereo WAV file; record_status reports when it is written, with its peak and the RMS level \
        of each channel.",
        {
            "name": string("Soundscape name (not needed with stop or the record options)."),
            "stop": boolean("Stop the preview."),
            "volume": number("Preview volume from 0 to 1 (default 1, times the soundscape volume setting).", 0.0, 1.0),
            "record_to": string("Absolute path of a .wav file to record the soundscape output into."),
            "record_seconds": number("Seconds to record (default 5).", 0.1, 120.0),
            "record_status": boolean("Report the state of the last recording."),
        }, [];
    soundscape_at: Read, "Which soundscape plays at a point",
        "The soundscape the player would hear at a point, by the rules of the Source SDK 2013 soundscape system: \
        the soundscape heard before (previous) is tested first and stays while it is enabled, then every enabled \
        env_soundscape, env_soundscape_proxy and env_soundscape_triggerable whose radius is greater than its \
        distance to the ear (radius -1 is everywhere) and whose trace to the ear is clear takes over when there \
        was none or it is nearer. The trace goes from the entity origin to the player's ear (eyes, 64 units above \
        the feet standing) with MASK_SOLID_BRUSHONLY and MASK_WATER: solid world and brush entity brushes, \
        windows, grates, water and solid static props block it, clips, triggers and other props do not, and an \
        entity whose origin is inside or on the surface of a solid brush never sees anyone (startsolid). A \
        trigger_soundscape the player stands in makes its env_soundscape_triggerable current (in_triggers); \
        an env_soundscape_proxy plays the soundscape and positions of its main entity. Lists every candidate \
        with distance, in_range, start_solid and line_of_sight, and the ambient_generic sounds audible there with \
        their gain. The same rules color the floor in the soundscape view (action toggle_soundscape_view).",
        {
            "point": vec3("Ear position (default: the player view eye when it is active, else the 3D camera)."),
            "previous": integer("Entity id of the soundscape entity heard before.", 1, i64::from(u32::MAX)),
        }, [];
}

fn interval_json(i: Interval) -> Value {
    json!([i.min, i.max])
}

fn position_json(p: Position) -> Value {
    match p {
        Position::None => Value::Null,
        Position::Index(i) => json!(i),
        Position::Random => json!("random"),
    }
}

fn rule_json(r: &Rule) -> Value {
    match r {
        Rule::Looping(l) => json!({
            "type": script::RULE_LOOPING,
            "volume": interval_json(l.volume),
            "pitch": interval_json(l.pitch),
            "wave": l.wave,
            "soundlevel": l.soundlevel,
            "position": position_json(l.position),
        }),
        Rule::Random(x) => json!({
            "type": script::RULE_RANDOM,
            "time": interval_json(x.time),
            "volume": interval_json(x.volume),
            "pitch": interval_json(x.pitch),
            "waves": x.waves,
            "soundlevel": x.soundlevel,
            "position": position_json(x.position),
        }),
        Rule::Nested(n) => json!({
            "type": script::RULE_NESTED,
            "name": n.name,
            "volume": interval_json(n.volume),
            "position": n.position,
            "positionoverride": n.position_override,
            "ambientpositionoverride": n.ambient_position_override,
        }),
        Rule::Other(name) => json!({ "type": name, "unknown": true }),
    }
}

struct Loaded {
    library: Library,
    game: Vec<(String, String)>,
    map: world::MapFile,
}

fn load(state: &AppState, root: Option<&std::path::Path>) -> Loaded {
    let game = world::game_texts(state);
    let map = match root {
        Some(r) => world::read_map_file(state, Some(r)),
        None => world::current_map_file(state),
    };
    let library = world::build_library(&game, world::map_rel(state).as_deref(), &map.blocks);
    Loaded { library, game, map }
}

fn users(state: &AppState, name: &str) -> Vec<u32> {
    world::collect_emitters(state)
        .into_iter()
        .filter(|e| e.soundscape.eq_ignore_ascii_case(name))
        .map(|e| e.id)
        .collect()
}

fn list_soundscapes(state: &mut AppState, a: &Args<'_>) -> R<ToolResult> {
    let words: Vec<String> =
        a.opt_str("search", 200)?.unwrap_or("").split_whitespace().map(str::to_ascii_lowercase).collect();
    let source = a.opt_enum("source", &["all", "game", "map"])?.unwrap_or_else(|| "all".to_owned());
    let limit = a.opt_u64("limit", 1, 5000)?.unwrap_or(300) as usize;
    let l = load(state, None);
    let emitters = world::collect_emitters(state);
    let mut list = Vec::new();
    let mut total = 0;
    for e in l.library.entries() {
        let name = e.block.name.trim();
        let lower = name.to_ascii_lowercase();
        if !words.iter().all(|w| lower.contains(w)) || (source != "all" && source != e.origin.id()) {
            continue;
        }
        total += 1;
        if list.len() >= limit {
            continue;
        }
        let s = e.soundscape();
        let count = |k: &str| s.rules.iter().filter(|r| r.kind() == k).count();
        let used: Vec<u32> =
            emitters.iter().filter(|x| x.soundscape.eq_ignore_ascii_case(name)).map(|x| x.id).collect();
        list.push(json!({
            "name": name,
            "source": e.origin.id(),
            "file": e.file,
            "playlooping": count(script::RULE_LOOPING),
            "playrandom": count(script::RULE_RANDOM),
            "playsoundscape": count(script::RULE_NESTED),
            "dsp": s.dsp,
            "overrides_game": e.origin == Origin::Map && l.library.game_has(name),
            "used_by": used,
        }));
    }
    let entities: Vec<Value> = emitters
        .iter()
        .map(|e| {
            json!({
                "id": e.id,
                "class": e.kind.class(),
                "soundscape": e.soundscape,
                "found": !e.soundscape.is_empty() && l.library.get(&e.soundscape).is_some(),
                "radius": e.radius,
                "enabled": e.enabled,
                "origin": e.origin,
                "targetname": e.targetname,
                "positions": e.positions.iter().map(|(i, t, at)| json!({ "index": i, "target": t, "found": at.is_some(), "origin": at })).collect::<Vec<_>>(),
                "main": e.main,
            })
        })
        .collect();
    let rel = world::map_rel(state);
    Ok(ToolResult::json(json!({
        "map_file": {
            "reference": rel,
            "path": l.map.path.as_ref().map(|p| p.display().to_string()),
            "exists": l.map.loaded_from.is_some(),
            "legacy_location": l.map.legacy,
            "unsaved_editor_changes": rel.as_ref().is_some_and(|r| state.runtime.lab.soundscape.unsaved.contains_key(r)),
            "count": l.map.blocks.len(),
        },
        "game_files": l.game.iter().map(|(f, _)| f.clone()).collect::<Vec<_>>(),
        "total": total,
        "soundscapes": list,
        "entities": entities,
    })))
}

fn soundscape_json(state: &AppState, l: &Loaded, name: &str) -> R<Value> {
    let entry =
        l.library.get(name).ok_or_else(|| format!("no soundscape named \"{name}\" (list_soundscapes lists them)"))?;
    let s = entry.soundscape();
    let flat = l.library.flatten(name);
    let mut waves: Vec<Value> = Vec::new();
    let mut seen: Vec<String> = Vec::new();
    for snd in &flat.sounds {
        for w in &snd.waves {
            let Some(path) = script::wave_path(w) else { continue };
            if seen.contains(&path) {
                continue;
            }
            let found = hp_ui_lab::media::read_game(state, &path).is_some();
            waves.push(json!({ "wave": w, "path": path, "found": found }));
            seen.push(path);
        }
    }
    let sounds: Vec<Value> = flat
        .sounds
        .iter()
        .map(|x| {
            json!({
                "kind": if x.kind == FlatKind::Looping { script::RULE_LOOPING } else { script::RULE_RANDOM },
                "chain": x.chain,
                "waves": x.waves,
                "volume": interval_json(x.volume),
                "master": x.master(),
                "pitch": interval_json(x.pitch),
                "time": if x.kind == FlatKind::Random { interval_json(x.time) } else { Value::Null },
                "soundlevel": x.soundlevel,
                "position": position_json(x.position),
            })
        })
        .collect();
    Ok(json!({
        "name": s.name,
        "source": entry.origin.id(),
        "file": entry.file,
        "read_only": entry.origin == Origin::Game,
        "overrides_game": entry.origin == Origin::Map && l.library.game_has(name),
        "dsp": s.dsp,
        "dsp_volume": s.dsp_volume,
        "dsp_player": s.dsp_player,
        "soundmixer": s.soundmixer,
        "rules": s.rules.iter().map(rule_json).collect::<Vec<_>>(),
        "keyvalues": script::write_file(std::slice::from_ref(&entry.block)),
        "flattened": { "dsp": flat.dsp, "dsp_player": flat.dsp_player, "dsp_volume": flat.dsp_volume, "sounds": sounds, "missing": flat.missing, "cycles": flat.cycles },
        "waves": waves,
        "used_by": users(state, name),
    }))
}

fn get_soundscape(state: &mut AppState, a: &Args<'_>) -> R<ToolResult> {
    let name = a.str("name", MAX_NAME)?.trim().to_owned();
    let l = load(state, None);
    Ok(ToolResult::json(soundscape_json(state, &l, &name)?))
}

fn interval_value(key: &str, v: &Value, lo: f32, hi: f32) -> R<String> {
    let bad = || format!("{key} must be a number, [min, max] or \"min,max\" from {lo} to {hi}");
    let i = match v {
        Value::Number(n) => Interval::fixed(n.as_f64().ok_or_else(bad)? as f32),
        Value::String(s) => Interval::parse(s).ok_or_else(bad)?,
        Value::Array(items) if items.len() == 2 => {
            let a = items[0].as_f64().ok_or_else(bad)? as f32;
            let b = items[1].as_f64().ok_or_else(bad)? as f32;
            Interval { min: a.min(b), max: a.max(b) }
        }
        _ => return Err(bad()),
    };
    if i.min < lo || i.max > hi || !i.min.is_finite() || !i.max.is_finite() {
        return Err(bad());
    }
    Ok(i.to_text())
}

fn text_value(key: &str, v: &Value) -> R<String> {
    let s = v.as_str().ok_or_else(|| format!("{key} must be a string"))?.trim();
    if s.contains(['"', '{', '}']) || s.chars().any(char::is_control) || s.len() > MAX_PATH {
        return Err(format!("{key} holds characters a soundscape file cannot store"));
    }
    Ok(s.to_owned())
}

fn position_value(key: &str, v: &Value, allow_random: bool) -> R<String> {
    match v {
        Value::Number(n) => match n.as_u64() {
            Some(i) if i < u64::from(script::MAX_POSITIONS) => Ok(i.to_string()),
            _ => Err(format!("{key} must be an integer from 0 to 7")),
        },
        Value::String(s) if allow_random && s.trim().eq_ignore_ascii_case("random") => Ok("random".to_owned()),
        _ => Err(format!("{key} must be an integer from 0 to 7{}", if allow_random { " or \"random\"" } else { "" })),
    }
}

fn level_value(v: &Value) -> R<String> {
    let text = match v {
        Value::Number(n) => n.as_f64().map(|f| format!("{f:.0}")).unwrap_or_default(),
        Value::String(s) => s.trim().to_owned(),
        _ => String::new(),
    };
    soundlevel::parse(&text).ok_or("soundlevel must be a SNDLVL_ name (SNDLVL_70dB) or a dB number")?;
    Ok(text)
}

fn rule_block(v: &Value) -> R<Block> {
    let o = v.as_object().ok_or("every rule must be an object")?;
    let kind = o.get("type").and_then(Value::as_str).map(str::to_ascii_lowercase).ok_or("every rule needs a type")?;
    let allowed: &[&str] = match kind.as_str() {
        script::RULE_LOOPING => &["type", "volume", "pitch", "wave", "soundlevel", "position"],
        script::RULE_RANDOM => &["type", "volume", "pitch", "time", "waves", "soundlevel", "position"],
        script::RULE_NESTED => &["type", "name", "volume", "position", "positionoverride", "ambientpositionoverride"],
        _ => return Err(format!("unknown rule type \"{kind}\" (playlooping, playrandom or playsoundscape)")),
    };
    if let Some(k) = o.keys().find(|k| !allowed.contains(&k.as_str())) {
        return Err(format!("{kind} has no key \"{k}\" (expected: {})", allowed.join(", ")));
    }
    let mut b = Block::new(kind.as_str());
    let get = |k: &str| o.get(k).filter(|v| !v.is_null());
    if let Some(v) = get("time") {
        b.push_prop("time", interval_value("time", v, 0.0, 3600.0)?);
    }
    if let Some(v) = get("volume") {
        b.push_prop("volume", interval_value("volume", v, 0.0, 1.0)?);
    }
    if let Some(v) = get("pitch") {
        b.push_prop("pitch", interval_value("pitch", v, 1.0, 255.0)?);
    }
    if let Some(v) = get("soundlevel") {
        b.push_prop("soundlevel", level_value(v)?);
    }
    if let Some(v) = get("position") {
        b.push_prop("position", position_value("position", v, kind == script::RULE_RANDOM)?);
    }
    match kind.as_str() {
        script::RULE_LOOPING => {
            let wave = get("wave").ok_or("playlooping needs a wave")?;
            b.push_prop("wave", text_value("wave", wave)?);
        }
        script::RULE_RANDOM => {
            let list = get("waves").and_then(Value::as_array).ok_or("playrandom needs waves (a list)")?;
            if list.is_empty() || list.len() > 256 {
                return Err("playrandom needs 1 to 256 waves".to_owned());
            }
            let mut waves = Block::new("rndwave");
            for w in list {
                waves.push_prop("wave", text_value("waves", w)?);
            }
            b.push_child(waves);
        }
        _ => {
            let name = text_value("name", get("name").ok_or("playsoundscape needs a name")?)?;
            if !script::valid_name(&name) {
                return Err(format!("\"{name}\" is not a valid soundscape name"));
            }
            b.push_prop("name", name);
            for k in ["positionoverride", "ambientpositionoverride"] {
                if let Some(v) = get(k) {
                    b.push_prop(k, position_value(k, v, false)?);
                }
            }
        }
    }
    Ok(b)
}

fn rule_list(a: &Args<'_>, key: &str) -> R<Option<Vec<Block>>> {
    let Some(v) = a.get(key) else { return Ok(None) };
    let list = v.as_array().ok_or_else(|| format!("{key} must be a list of rules"))?;
    if list.len() > MAX_RULES {
        return Err(format!("{key} holds more than {MAX_RULES} rules"));
    }
    list.iter().map(rule_block).collect::<R<Vec<_>>>().map(Some).map_err(|e| format!("{key}: {e}"))
}

fn soundscape_targets(state: &AppState, ids: &[u32]) -> R<()> {
    let scene = state.doc.scene();
    for id in ids {
        let n = scene.entity(*id).ok_or_else(|| format!("no entity {id}"))?;
        if !matches!(EmitterKind::from_class(&n.entity.classname), Some(EmitterKind::Main | EmitterKind::Triggerable)) {
            return Err(format!(
                "entity {id} is a {}, not an env_soundscape or env_soundscape_triggerable",
                n.entity.classname
            ));
        }
    }
    Ok(())
}

fn edit_soundscape(state: &mut AppState, a: &Args<'_>) -> R<ToolResult> {
    let name = a.str("name", MAX_NAME)?.trim().to_owned();
    if !script::valid_name(&name) {
        return Err(format!("\"{name}\" is not a valid soundscape name (no spaces, quotes or braces)"));
    }
    let rules = rule_list(a, "rules")?;
    let add_rules = rule_list(a, "add_rules")?;
    let dsp = match a.get("dsp") {
        None => None,
        Some(v) => match v.as_i64() {
            Some(d) if (-1..=133).contains(&d) => Some(d),
            _ => return Err("dsp must be an integer from -1 to 133".to_owned()),
        },
    };
    let dsp_player = match a.get("dsp_player") {
        None => None,
        Some(v) => match v.as_i64() {
            Some(d) if (-1..=133).contains(&d) => Some(d),
            _ => return Err("dsp_player must be an integer from -1 to 133".to_owned()),
        },
    };
    let dsp_volume = a.opt_f64("dsp_volume", -1.0, 2.0)?;
    let copy_from = a.opt_str("copy_from", MAX_NAME)?.map(|s| s.trim().to_owned());
    let rename = a.opt_str("rename", MAX_NAME)?.map(|s| s.trim().to_owned());
    let delete = a.opt_bool("delete")?.unwrap_or(false);
    let assign = match a.get("assign_to") {
        Some(_) => a.ids("assign_to")?,
        None => Vec::new(),
    };
    let dry = a.opt_bool("dry_run")?.unwrap_or(false);
    let root = match a.opt_str("root", MAX_PATH)?.map(str::trim).filter(|r| !r.is_empty()) {
        Some(r) => Some(super::semantic::local_path(r).map(|_| hp_ui_lab::media::resolve_root(state, Some(r)))??),
        None => None,
    };
    if delete
        && (rules.is_some() || add_rules.is_some() || rename.is_some() || copy_from.is_some() || !assign.is_empty())
    {
        return Err("delete cannot be combined with other changes".to_owned());
    }
    soundscape_targets(state, &assign)?;
    let rel = world::map_rel(state).ok_or("save the map first: the soundscape file is named after it")?;
    let l = load(state, root.as_deref());
    if let Some(e) = &l.map.error {
        return Err(format!("the soundscape file cannot be read, it is left alone: {e}"));
    }
    let path =
        l.map.path.clone().ok_or("no folder to write the soundscape file to: set the game folder or pass root")?;
    let mut blocks = l.map.blocks.clone();
    let at = blocks.iter().position(|b| b.name.trim().eq_ignore_ascii_case(&name));
    let mut created = false;
    let final_name;
    if delete {
        let i = at.ok_or_else(|| format!("the map file has no soundscape named \"{name}\""))?;
        blocks.remove(i);
        final_name = name.clone();
    } else {
        let i = match at {
            Some(i) => i,
            None => {
                let mut b = match &copy_from {
                    Some(src) => l
                        .library
                        .get(src)
                        .map(|e| e.block.clone())
                        .ok_or_else(|| format!("copy_from: no soundscape named \"{src}\""))?,
                    None => {
                        let mut b = Block::new(name.as_str());
                        b.set("dsp", "1");
                        b
                    }
                };
                b.name.clone_from(&name);
                blocks.push(b);
                created = true;
                blocks.len() - 1
            }
        };
        let b = &mut blocks[i];
        if let Some(list) = rules {
            b.children = list;
        }
        if let Some(list) = add_rules {
            b.children.extend(list);
        }
        match dsp {
            Some(-1) => {
                b.remove("dsp");
            }
            Some(d) => b.set("dsp", d.to_string()),
            None => {}
        }
        match dsp_player {
            Some(-1) => {
                b.remove("dsp_player");
            }
            Some(d) => b.set("dsp_player", d.to_string()),
            None => {}
        }
        match dsp_volume {
            Some(f) if f < 0.0 => {
                b.remove("dsp_volume");
            }
            Some(f) => b.set("dsp_volume", script::num(f as f32)),
            None => {}
        }
        if let Some(new) = &rename {
            if !script::valid_name(new) {
                return Err(format!("rename: \"{new}\" is not a valid soundscape name"));
            }
            if blocks.iter().enumerate().any(|(j, x)| j != i && x.name.trim().eq_ignore_ascii_case(new)) {
                return Err(format!("rename: the map file already has \"{new}\""));
            }
            blocks[i].name.clone_from(new);
        }
        final_name = blocks[i].name.trim().to_owned();
    }
    let text = script::write_file(&blocks);
    let renamed_users: Vec<u32> = if rename.is_some() { users(state, &name) } else { Vec::new() };
    let mut out = json!({
        "path": path.display().to_string(),
        "reference": rel,
        "created": created,
        "deleted": delete,
        "name": final_name,
        "overrides_game": !delete && l.library.game_has(&final_name),
        "written": false,
    });
    if dry {
        out["dry_run"] = json!(true);
        out["text"] = json!(text);
        return Ok(ToolResult::json(out));
    }
    world::check_target(state, &l.game, &path)?;
    world::write_map_file(&path, &blocks)?;
    state.runtime.lab.soundscape.unsaved.remove(&rel);
    hp_ui_entities::model_import::remount_after_write(state);
    state.runtime.lab.soundscape.library_serial += 1;
    out["written"] = json!(true);
    let mut changed_ids: Vec<u32> = assign.clone();
    changed_ids.extend(renamed_users.iter().copied());
    changed_ids.sort_unstable();
    changed_ids.dedup();
    if !changed_ids.is_empty() && !delete {
        let n = edit(state, "edit_soundscape", |doc| doc.set_entity_key(&changed_ids, "soundscape", &final_name))
            .map_err(|e| format!("the file was written but the entities were not changed: {e}"))?;
        out["entities_changed"] = json!(n);
    }
    if !delete {
        let l = load(state, root.as_deref());
        if let Ok(v) = soundscape_json(state, &l, &final_name) {
            out["soundscape"] = v;
        }
    }
    Ok(ToolResult::json(out))
}

fn record_json(state: &AppState) -> Value {
    use hp_app::runtime::soundscape::RecordState;
    match &state.runtime.lab.soundscape.record {
        None => json!({ "state": "none" }),
        Some(RecordState::Recording { path, seconds }) => {
            json!({ "state": "recording", "path": path.display().to_string(), "seconds": seconds })
        }
        Some(RecordState::Written { path, seconds, peak, rms }) => json!({
            "state": "written",
            "path": path.display().to_string(),
            "seconds": seconds,
            "peak": peak,
            "rms": rms,
        }),
        Some(RecordState::Failed(e)) => json!({ "state": "failed", "error": e }),
    }
}

fn preview_soundscape(state: &mut AppState, a: &Args<'_>) -> R<ToolResult> {
    if a.opt_bool("record_status")?.unwrap_or(false) {
        return Ok(ToolResult::json(json!({ "record": record_json(state) })));
    }
    let mut out = Map::new();
    if let Some(path) = a.opt_str("record_to", MAX_PATH)? {
        let file = super::semantic::local_path(path)?;
        if !file.is_absolute() || !file.extension().is_some_and(|e| e.eq_ignore_ascii_case("wav")) {
            return Err("record_to must be an absolute path ending in .wav".to_owned());
        }
        let seconds = a.opt_f64("record_seconds", 0.1, 120.0)?.unwrap_or(5.0) as f32;
        state.runtime.lab.soundscape.record_request =
            Some(hp_app::runtime::soundscape::RecordRequest { path: file.clone(), seconds });
        out.insert("recording".into(), json!({ "path": file.display().to_string(), "seconds": seconds }));
    } else if a.get("record_seconds").is_some() {
        return Err("record_seconds needs record_to".to_owned());
    }
    if a.opt_bool("stop")?.unwrap_or(false) {
        state.runtime.lab.soundscape.preview_request = Some(PreviewRequest::Stop);
        out.insert("playing".into(), json!(false));
        return Ok(ToolResult::json(Value::Object(out)));
    }
    let Some(name) = a.opt_str("name", MAX_NAME)?.map(str::trim).filter(|n| !n.is_empty()).map(str::to_owned) else {
        if out.is_empty() {
            return Err("name is required (or stop, record_to or record_status)".to_owned());
        }
        return Ok(ToolResult::json(Value::Object(out)));
    };
    let volume = a.opt_f64("volume", 0.0, 1.0)?.unwrap_or(1.0) as f32;
    let l = load(state, None);
    let entry =
        l.library.get(&name).ok_or_else(|| format!("no soundscape named \"{name}\" (list_soundscapes lists them)"))?;
    let real = entry.block.name.trim().to_owned();
    let flat = l.library.flatten(&real);
    state.runtime.lab.soundscape.preview_request = Some(PreviewRequest::Play { name: real.clone(), volume });
    let looping = flat.sounds.iter().filter(|s| s.kind == FlatKind::Looping).count();
    out.insert("playing".into(), json!(hp_audio::mixer::ambience_available()));
    out.insert("name".into(), json!(real));
    out.insert("playlooping".into(), json!(looping));
    out.insert("playrandom".into(), json!(flat.sounds.len() - looping));
    out.insert("dsp".into(), json!(flat.dsp));
    out.insert("missing".into(), json!(flat.missing));
    out.insert("volume".into(), json!(volume));
    out.insert("output_volume".into(), json!(state.settings.lab.soundscape.output_volume()));
    out.insert(
        "note".into(),
        json!("Call preview_soundscape with stop true to stop. Playback exists on Windows only."),
    );
    Ok(ToolResult::json(Value::Object(out)))
}

fn soundscape_at(state: &mut AppState, a: &Args<'_>) -> R<ToolResult> {
    let ear = match a.opt_vec3("point")? {
        Some(p) => p,
        None => hp_ui_lab::soundscape::view::ear_of(state),
    };
    let previous_id = a.opt_u64("previous", 1, u64::from(u32::MAX))?.map(|v| v as u32);
    let emitters = world::collect_emitters(state);
    let previous = match previous_id {
        Some(id) => Some(
            emitters
                .iter()
                .position(|e| e.id == id)
                .ok_or_else(|| format!("previous: entity {id} is not a soundscape entity"))?,
        ),
        None => None,
    };
    world::refresh_props(state);
    let l = load(state, None);
    let triggers = world::collect_triggers(state);
    let (bmin, bmax) = world::player_box(ear - DVec3::Z * 64.0, false);
    let inside = world::touching(&triggers, bmin, bmax);
    let sight = world::sight_fn(state);
    let d =
        decide(&emitters, ear.to_array(), previous, |e, to| sight(DVec3::from_array(e.origin), DVec3::from_array(to)));
    let active = d.active.and_then(|i| emitters.get(i)).map(|e| {
        json!({
            "id": e.id,
            "class": e.kind.class(),
            "soundscape": e.soundscape,
            "found": l.library.get(&e.soundscape).is_some(),
            "kept_previous": d.kept_previous,
            "distance": (e.distance(ear.to_array()) * 10.0).round() / 10.0,
        })
    });
    let candidates: Vec<Value> = emitters
        .iter()
        .enumerate()
        .map(|(i, e)| {
            let c = d.candidates.iter().find(|c| c.index == i);
            let mut m = Map::new();
            m.insert("id".into(), json!(e.id));
            m.insert("class".into(), json!(e.kind.class()));
            m.insert("soundscape".into(), json!(e.soundscape));
            m.insert("radius".into(), json!(e.radius));
            m.insert("enabled".into(), json!(e.enabled));
            m.insert("distance".into(), json!((e.distance(ear.to_array()) * 10.0).round() / 10.0));
            m.insert("in_range".into(), json!(c.is_some_and(|c| c.in_range)));
            m.insert("line_of_sight".into(), json!(c.is_some_and(|c| c.line_of_sight)));
            m.insert("start_solid".into(), json!(world::start_solid(state, DVec3::from_array(e.origin))));
            if c.is_none() {
                m.insert("skipped".into(), json!(if e.enabled { "no soundscape name" } else { "disabled" }));
            }
            Value::Object(m)
        })
        .collect();
    let ambients: Vec<Value> = world::collect_ambients(state)
        .iter()
        .map(|x| {
            let dist = ear.distance(DVec3::from_array(x.origin)) as f32;
            let gain = soundlevel::gain(x.volume, x.soundlevel, dist);
            json!({
                "id": x.id,
                "message": x.message,
                "distance": (f64::from(dist) * 10.0).round() / 10.0,
                "radius": x.radius,
                "soundlevel": x.soundlevel,
                "gain": (f64::from(gain) * 1000.0).round() / 1000.0,
                "audible": gain > 0.002 && !x.start_silent,
                "looped": x.looped,
                "start_silent": x.start_silent,
                "play_everywhere": x.everywhere,
            })
        })
        .collect();
    Ok(ToolResult::json(json!({
        "point": [ear.x, ear.y, ear.z],
        "active": active,
        "candidates": candidates,
        "ambient_generic": ambients,
        "in_triggers": triggers.iter().filter(|t| inside.contains(&t.id)).map(|t| json!({
            "id": t.id,
            "soundscape": t.soundscape,
            "triggerable": t.triggerable,
            "note": "entering this trigger makes its env_soundscape_triggerable current",
        })).collect::<Vec<_>>(),
        "rule": "previous first, then the nearest enabled soundscape with radius greater than the distance (or -1) and a clear MASK_SOLID_BRUSHONLY|MASK_WATER trace from its origin to the ear; otherwise the previous one stays",
    })))
}

#[cfg(test)]
mod tests {
    use serde_json::json;

    use super::super::semantic_tests::{err, ok};

    fn scratch(name: &str) -> std::path::PathBuf {
        let base = if std::path::Path::new("E:/hp-scratch").is_dir() {
            std::path::PathBuf::from("E:/hp-scratch/v206/z04/mcp")
        } else {
            std::env::temp_dir().join("hp-ss-mcp")
        };
        let dir = base.join(format!("{name}-{}", std::process::id()));
        let _ = std::fs::remove_dir_all(&dir);
        std::fs::create_dir_all(dir.join("maps")).unwrap();
        dir
    }

    #[test]
    fn edit_list_get_and_assign_a_map_soundscape() {
        let dir = scratch("edit");
        let mut s = hp_app::testing::test_state();
        err(&mut s, "edit_soundscape", json!({ "name": "gm_ss.cave" }));
        s.doc.set_path(Some(dir.join("maps").join("gm_ss.vmf")));
        let id = s.doc.create_point_entity("env_soundscape", glam::DVec3::new(0.0, 0.0, 9000.0), &[]).unwrap();
        let r = ok(
            &mut s,
            "edit_soundscape",
            json!({
                "name": "gm_ss.cave",
                "dsp": 23,
                "rules": [
                    { "type": "playlooping", "volume": 0.6, "pitch": [95, 105], "wave": "ambient/cave_hit1.wav" },
                    { "type": "playrandom", "time": "4,9", "volume": [0.3, 0.5], "waves": ["ambient/drip1.wav", "ambient/drip2.wav"], "position": "random", "soundlevel": "SNDLVL_70dB" },
                    { "type": "playsoundscape", "name": "gm_ss.base", "volume": 0.5 }
                ],
                "assign_to": [id]
            }),
        );
        assert_eq!(r["written"], true);
        assert_eq!(r["created"], true);
        assert_eq!(r["entities_changed"], 1);
        let file = dir.join("scripts").join("soundscapes_gm_ss.txt");
        assert!(file.is_file());
        assert_eq!(s.doc.scene().entity(id).and_then(|n| n.entity.get("soundscape")), Some("gm_ss.cave"));
        let g = ok(&mut s, "get_soundscape", json!({ "name": "GM_SS.CAVE" }));
        assert_eq!(g["dsp"], 23);
        assert_eq!(g["rules"].as_array().unwrap().len(), 3);
        assert_eq!(g["rules"][1]["position"], "random");
        assert_eq!(g["flattened"]["missing"][0], "gm_ss.base");
        assert_eq!(g["used_by"][0], id);
        let l = ok(&mut s, "list_soundscapes", json!({ "source": "map" }));
        assert_eq!(l["total"], 1);
        assert_eq!(l["entities"][0]["found"], true);
        let dry =
            ok(&mut s, "edit_soundscape", json!({ "name": "gm_ss.cave", "rename": "gm_ss.cave2", "dry_run": true }));
        assert!(dry["text"].as_str().unwrap().contains("gm_ss.cave2"));
        assert_eq!(s.doc.scene().entity(id).and_then(|n| n.entity.get("soundscape")), Some("gm_ss.cave"), "dry run");
        ok(&mut s, "edit_soundscape", json!({ "name": "gm_ss.cave", "rename": "gm_ss.cave2" }));
        assert_eq!(
            s.doc.scene().entity(id).and_then(|n| n.entity.get("soundscape")),
            Some("gm_ss.cave2"),
            "users follow a rename"
        );
        ok(
            &mut s,
            "edit_soundscape",
            json!({ "name": "gm_ss.base", "copy_from": "gm_ss.cave2", "add_rules": [{ "type": "playlooping", "volume": "0.2", "wave": "x.wav", "position": 0 }] }),
        );
        let base = ok(&mut s, "get_soundscape", json!({ "name": "gm_ss.base" }));
        assert_eq!(base["rules"].as_array().unwrap().len(), 4);
        ok(&mut s, "edit_soundscape", json!({ "name": "gm_ss.base", "delete": true }));
        err(&mut s, "get_soundscape", json!({ "name": "gm_ss.base" }));
        let _ = std::fs::remove_dir_all(&dir);
    }

    #[test]
    fn bad_rules_and_targets_are_refused() {
        let dir = scratch("bad");
        let mut s = hp_app::testing::test_state();
        s.doc.set_path(Some(dir.join("maps").join("gm_bad.vmf")));
        for rules in [
            json!([{ "type": "playloop", "wave": "a.wav" }]),
            json!([{ "type": "playlooping", "wave": "a.wav", "volume": 3 }]),
            json!([{ "type": "playlooping" }]),
            json!([{ "type": "playlooping", "wave": "a.wav", "position": "random" }]),
            json!([{ "type": "playrandom", "waves": [] }]),
            json!([{ "type": "playrandom", "waves": ["a\"b.wav"] }]),
            json!([{ "type": "playsoundscape", "name": "bad name" }]),
            json!([{ "type": "playlooping", "wave": "a.wav", "extra": 1 }]),
            json!([{ "type": "playlooping", "wave": "a.wav", "soundlevel": "SNDLVL_LOUD" }]),
        ] {
            err(&mut s, "edit_soundscape", json!({ "name": "gm_bad.x", "rules": rules }));
        }
        err(&mut s, "edit_soundscape", json!({ "name": "bad name" }));
        err(&mut s, "edit_soundscape", json!({ "name": "gm_bad.x", "delete": true }));
        err(&mut s, "edit_soundscape", json!({ "name": "gm_bad.x", "assign_to": [2] }));
        err(&mut s, "preview_soundscape", json!({ "name": "nothing.here" }));
        err(&mut s, "preview_soundscape", json!({}));
        assert!(!dir.join("scripts").exists(), "nothing written by refused calls");
        let _ = std::fs::remove_dir_all(&dir);
    }

    #[test]
    fn the_active_soundscape_follows_the_engine_rules() {
        let mut s = hp_app::testing::test_state();
        let kv =
            |name: &str, r: &str| vec![("soundscape".to_owned(), name.to_owned()), ("radius".to_owned(), r.to_owned())];
        let a =
            s.doc.create_point_entity("env_soundscape", glam::DVec3::new(0.0, 0.0, 9000.0), &kv("x.a", "512")).unwrap();
        let b = s
            .doc
            .create_point_entity("env_soundscape", glam::DVec3::new(600.0, 0.0, 9000.0), &kv("x.b", "512"))
            .unwrap();
        let amb = vec![("message".to_owned(), "ambient/wind.wav".to_owned()), ("radius".to_owned(), "1250".to_owned())];
        s.doc.create_point_entity("ambient_generic", glam::DVec3::new(0.0, 100.0, 9000.0), &amb).unwrap();
        let r = ok(&mut s, "soundscape_at", json!({ "point": [100, 0, 9000] }));
        assert_eq!(r["active"]["id"], a);
        let r = ok(&mut s, "soundscape_at", json!({ "point": [400, 0, 9000] }));
        assert_eq!(r["active"]["id"], b, "nearest in range");
        let r = ok(&mut s, "soundscape_at", json!({ "point": [3000, 0, 9000], "previous": a }));
        assert_eq!(r["active"]["id"], a);
        assert_eq!(r["active"]["kept_previous"], true);
        let r = ok(&mut s, "soundscape_at", json!({ "point": [3000, 0, 9000] }));
        assert!(r["active"].is_null());
        assert_eq!(r["ambient_generic"][0]["audible"], true);
        err(&mut s, "soundscape_at", json!({ "previous": 2 }));
    }

    #[test]
    fn recording_requests_and_status() {
        let mut s = hp_app::testing::test_state();
        let dir = scratch("rec");
        let path = dir.join("out.wav");
        let r =
            ok(&mut s, "preview_soundscape", json!({ "record_to": path.display().to_string(), "record_seconds": 2 }));
        assert_eq!(r["recording"]["seconds"], 2.0);
        assert!(s.runtime.lab.soundscape.record_request.is_some());
        let st = ok(&mut s, "preview_soundscape", json!({ "record_status": true }));
        assert_eq!(st["record"]["state"], "none", "the request is picked up by the hearing driver");
        err(&mut s, "preview_soundscape", json!({ "record_to": "relative.wav" }));
        err(&mut s, "preview_soundscape", json!({ "record_to": dir.join("x.mp3").display().to_string() }));
        err(&mut s, "preview_soundscape", json!({ "record_seconds": 3 }));
        let _ = std::fs::remove_dir_all(&dir);
    }
}
