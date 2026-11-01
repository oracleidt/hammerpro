use hp_app::AppState;
use hp_editor::{EntityQuery, TextField, TextQuery};
use hp_mcp::protocol::ToolResult;
use hp_vmf::Connection;
use serde_json::{Value, json};

use super::semantic::*;
use super::tools::{Args, MAX_NAME, MAX_VALUE, R, check_class, check_key, check_value, edit, v3};

const MAX_GRAPH: usize = 5000;

domain! { "entities and io";
    list_entity_classes: Read, "List entity classes",
        "The entity classes of the game's FGD (point or brush), with a name pattern and their descriptions.",
        {
            "pattern": string("Class pattern with * and ? (default every class)."),
            "kind": choice("Point or brush classes (default both).", &["point", "brush"]),
            "limit": limit(), "offset": offset(),
        }, [];
    get_entity_class: Read, "Entity class",
        "Everything the FGD says about a class: description, bases, helpers (model, sprite, size...), every \
        property with type, default, description, choices and flags, inputs and outputs.",
        { "classname": string("Class name.") }, ["classname"];
    check_entity: Read, "Check an entity",
        "Check the keyvalues and outputs of an entity against the FGD: unknown class, unknown keys, values that \
        are not one of the choices, outputs the class does not have, inputs the target classes do not have, \
        targets that do not exist.",
        { "id": integer("Entity id.", 1, i64::from(u32::MAX)) }, ["id"];
    set_keyvalue: Edit, "Set a keyvalue on entities",
        "Set (or with value null, remove) one key on several entities at once, or rename a key (rename_to).",
        {
            "ids": ids("Entity ids (default: the selected entities)."),
            "key": string("Key."),
            "value": string("Value (omit to remove the key)."),
            "rename_to": string("New key name (renames instead of setting)."),
        }, ["key"];
    set_entity_class: Edit, "Change the class of entities",
        "Change the class of entities (keyvalues stay). A brush entity whose new FGD class has an origin key \
        (func_tracktrain, func_tanktrain, func_rotating...) and has none gets it at the center of its solids, like \
        Hammer.",
        { "ids": ids("Entity ids (default: the selected entities)."), "classname": string("New class.") }, ["classname"];
    set_spawnflags: Edit, "Spawnflags",
        "Switch spawnflags of entities on or off, by bit value or by their FGD name.",
        {
            "ids": ids("Entity ids (default: the selected entities)."),
            "set": strings("Flags switched on: bit values (\"4\") or FGD names.", 32),
            "clear": strings("Flags switched off.", 32),
        }, [];
    list_connections: Read, "Connections of an entity",
        "The outputs of an entity (index, output, target, input, parameter, delay, times to fire) and the outputs \
        of other entities that target it.",
        { "id": integer("Entity id.", 1, i64::from(u32::MAX)) }, ["id"];
    add_connection: Edit, "Add an output",
        "Add an output (connection) to entities: output, target name, input, parameter, delay, times to fire \
        (-1: always).",
        {
            "ids": ids("Entity ids (default: the selected entities)."),
            "output": string("Output (OnTrigger...)."),
            "target": string("Target name (or !activator, !self...)."),
            "input": string("Input (Open, Kill...)."),
            "parameter": string("Parameter (default empty)."),
            "delay": number("Delay in seconds (default 0).", 0.0, 100_000.0),
            "times": integer("Times to fire, -1 for always (default -1).", -1, 1_000_000),
        }, ["output", "target", "input"];
    edit_connection: Edit, "Edit an output",
        "Change fields of one output of an entity (index from list_connections).",
        {
            "id": integer("Entity id.", 1, i64::from(u32::MAX)),
            "index": integer("Output index.", 0, 100_000),
            "output": string("Output."), "target": string("Target name."), "input": string("Input."),
            "parameter": string("Parameter."),
            "delay": number("Delay in seconds.", 0.0, 100_000.0),
            "times": integer("Times to fire, -1 for always.", -1, 1_000_000),
        }, ["id", "index"];
    remove_connections: Destructive, "Remove outputs",
        "Remove one output by index, or every output of entities matching output, target and input patterns.",
        {
            "ids": ids("Entity ids (default: the selected entities)."),
            "index": integer("Output index (with one entity).", 0, 100_000),
            "output": string("Output pattern."), "target": string("Target pattern."), "input": string("Input pattern."),
        }, [];
    get_io_graph: Read, "I/O graph",
        "Every connection of the map (from entity, output, target, input, parameter, delay) with the entities the \
        target name resolves to, and the broken ones (no entity has the target name).",
        { "broken_only": boolean("Only connections whose target exists nowhere."), "limit": limit(), "offset": offset() }, [];
    rename_targetname: Edit, "Rename an entity name",
        "Rename a targetname on every entity that has it, and (default) in every output, parameter and key that \
        refers to it.",
        { "old": string("Current name."), "new": string("New name."), "update_references": boolean("Fix references (default true).") }, ["old", "new"];
    find_entities: Read, "Find entities",
        "Find entities by class pattern, targetname pattern, and a key with a value pattern.",
        {
            "classname": string("Class pattern."), "targetname": string("Name pattern."),
            "key": string("Key that must exist."), "value": string("Value pattern of key."),
            "include_hidden": boolean("Hidden entities too (default true)."),
            "limit": limit(), "offset": offset(),
        }, [];
    find_text: Read, "Find text",
        "Find text in keyvalues, key names and outputs of entities (Find and Replace).",
        {
            "find": string("Text."), "whole_word": boolean("Whole words only."), "case_sensitive": boolean("Match case."),
            "key_names": boolean("Search key names too."), "ids": ids("Only these entities."),
            "visible_only": boolean("Visible entities only."), "limit": limit(), "offset": offset(),
        }, ["find"];
    replace_text: Edit, "Replace text",
        "Replace text in keyvalues, key names and outputs of entities (every match of find_text).",
        {
            "find": string("Text."), "replace": string("Replacement."),
            "whole_word": boolean("Whole words only."), "case_sensitive": boolean("Match case."),
            "key_names": boolean("Key names too."), "ids": ids("Only these entities."), "visible_only": boolean("Visible entities only."),
        }, ["find", "replace"];
    center_entity_origins: Edit, "Center origins",
        "Move the origin of brush entities (func_door_rotating...) to the center of their solids.",
        { "ids": ids("Entity ids (default: the selected entities).") }, [];
    entity_report: Read, "Entity report",
        "How many entities of each class the map has (and brush entity solids).",
        {}, [];
    create_entity_fgd: Edit, "Create an entity with its defaults",
        "Create a point entity like the entity tool: the FGD defaults of its class (keyvalues and spawnflags), then \
        the keyvalues given, at an origin with angles.",
        {
            "classname": string("Class."), "origin": vec3("Origin."), "angles": vec3("Angles [pitch, yaw, roll]."),
            "keyvalues": keyvalues("Keyvalues set over the defaults."),
        }, ["classname", "origin"];
    copy_keyvalues: Edit, "Copy keyvalues",
        "Copy keyvalues of one entity onto others (Copy and Paste keyvalues): every key but id, classname, origin \
        and targetname, or only the keys listed.",
        {
            "from": integer("Source entity id.", 1, i64::from(u32::MAX)),
            "ids": ids("Target entities (default: the selected entities)."),
            "keys": strings("Only these keys.", 256),
        }, ["from"];
    list_targetnames: Read, "Entity names",
        "Every targetname of the map with its entities and how many outputs aim at it.",
        { "pattern": string("Name pattern."), "limit": limit(), "offset": offset() }, [];
    query_box: Read, "Objects in a box",
        "The solids and entities touching (or inside) a box, without changing the selection.",
        {
            "min": vec3("Box minimum."), "max": vec3("Box maximum."),
            "inside": boolean("Only objects entirely inside."),
            "limit": limit(),
        }, ["min", "max"];
    aim_entities: Edit, "Aim entities",
        "Turn entities (spot lights, cameras, props, info_player_start...) to face a point or another entity: \
        their angles (and the pitch key of light_spot and light_environment, which VRAD reads with up positive).",
        {
            "ids": ids("Entity ids (default: the selected entities)."),
            "point": vec3("Point to face."),
            "target": integer("Entity to face.", 1, i64::from(u32::MAX)),
            "yaw_only": boolean("Only turn around the vertical axis."),
        }, [];
    set_max_distance: Edit, "Set the maximum distance of entities",
        "One maximum distance for entities, written to the key each class honours in Garry's Mod: light and \
        light_spot _distance (the light stops there: models in game, lightmaps with Hammer Pro's compiler), props, \
        info_overlay and env_detail_controller fademaxdist (with a fademindist start so they fade out), \
        env_lightglow OuterMaxDist, func_lod DisappearMaxDist, light_dynamic distance and point_spotlight \
        SpotlightLength (only shortened). 0 removes the limit. Classes the game cannot limit (logic entities, \
        triggers) are skipped and listed. get_entity_class gives the key of a class (max_distance).",
        {
            "ids": ids("Entity ids (default: the selected entities)."),
            "distance": number("Maximum distance in units (0: no limit).", 0.0, 1_000_000.0),
        }, ["distance"];
    set_current_entity_class: Editor, "Entity tool class",
        "The class the entity tool places next (and its default model for prop classes).",
        { "classname": string("Class."), "model": string("Model path for props (models/....mdl).") }, ["classname"];
}

fn fgd(state: &AppState) -> R<&hp_fgd::ResolvedFgd> {
    state.assets.fgd().ok_or_else(|| "no FGD is loaded (configure the game in Hammer Pro's settings)".to_owned())
}

fn kind_name(k: &hp_fgd::ClassKind) -> String {
    format!("{k:?}").to_ascii_lowercase()
}

fn is_brush_class(c: &hp_fgd::ResolvedClass) -> bool {
    matches!(c.kind, hp_fgd::ClassKind::Solid)
}

fn class_name(a: &Args<'_>, key: &str) -> R<String> {
    let c = a.str(key, MAX_NAME)?.trim().to_owned();
    check_class(&c)?;
    Ok(c)
}

fn pat(a: &Args<'_>, key: &str) -> R<Option<String>> {
    Ok(a.opt_str(key, MAX_NAME)?.map(str::trim).filter(|s| !s.is_empty()).map(str::to_owned))
}

fn list_entity_classes(state: &mut AppState, a: &Args<'_>) -> R<ToolResult> {
    let f = fgd(state)?;
    let pattern = pat(a, "pattern")?;
    let kind = a.opt_enum("kind", &["point", "brush"])?;
    let mut list: Vec<&hp_fgd::ResolvedClass> = f
        .iter()
        .filter(|c| !matches!(c.kind, hp_fgd::ClassKind::Base))
        .filter(|c| pattern.as_deref().is_none_or(|p| hp_editor::glob_match(p, &c.name)))
        .filter(|c| match kind.as_deref() {
            Some("brush") => is_brush_class(c),
            Some(_) => !is_brush_class(c),
            None => true,
        })
        .collect();
    list.sort_by(|x, y| x.name.cmp(&y.name));
    let (list, total) = page(list, a)?;
    let out: Vec<Value> = list
        .into_iter()
        .map(|c| {
            let d: String = c.description.chars().take(200).collect();
            json!({ "classname": c.name, "kind": kind_name(&c.kind), "description": d })
        })
        .collect();
    Ok(ToolResult::json(json!({ "total": total, "classes": out })))
}

fn get_entity_class(state: &mut AppState, a: &Args<'_>) -> R<ToolResult> {
    let name = class_name(a, "classname")?;
    let f = fgd(state)?;
    let c = f.class(&name).ok_or_else(|| format!("the FGD has no class {name} (list_entity_classes)"))?;
    Ok(ToolResult::json(class_json(c)))
}

fn class_json(c: &hp_fgd::ResolvedClass) -> Value {
    let props: Vec<Value> = c
        .props
        .iter()
        .map(|p| {
            json!({
                "name": p.name,
                "type": p.kind.to_string(),
                "display_name": p.display_name,
                "default": p.default,
                "description": p.description,
                "readonly": p.readonly,
                "choices": p.choices.iter().map(|ch| json!({ "value": ch.value, "label": ch.display })).collect::<Vec<_>>(),
                "flags": p.flags.iter().map(|fl| json!({ "bit": fl.value, "label": fl.display, "default_on": fl.default_on })).collect::<Vec<_>>(),
            })
        })
        .collect();
    let io = |l: &[hp_fgd::IoDef]| -> Vec<Value> {
        l.iter()
            .map(|d| json!({ "name": d.name, "type": d.arg_type.to_string(), "description": d.description }))
            .collect()
    };
    json!({
        "classname": c.name,
        "kind": kind_name(&c.kind),
        "description": c.description,
        "bases": c.bases,
        "helpers": c.helpers.iter().map(|h| json!({ "name": h.name, "args": h.args })).collect::<Vec<_>>(),
        "max_distance": max_distance_json(c),
        "properties": props,
        "inputs": io(&c.inputs),
        "outputs": io(&c.outputs),
    })
}

fn target_classes(state: &AppState, target: &str) -> Vec<String> {
    let t = target.to_ascii_lowercase();
    if t.starts_with('!') {
        return Vec::new();
    }
    state
        .doc
        .scene()
        .entities()
        .values()
        .filter(|e| e.entity.get("targetname").is_some_and(|n| hp_editor::glob_match(&t, &n.to_ascii_lowercase())))
        .map(|e| e.entity.classname.to_ascii_lowercase())
        .collect()
}

fn check_entity(state: &mut AppState, a: &Args<'_>) -> R<ToolResult> {
    let id = entity_id(&state.doc, a, "id")?;
    let f = fgd(state)?;
    let e = &state.doc.scene().entity(id).ok_or("gone")?.entity;
    let issues = entity_issues(state, f, e);
    Ok(ToolResult::json(json!({ "id": id, "classname": e.classname, "issues": issues })))
}

fn entity_issues(state: &AppState, f: &hp_fgd::ResolvedFgd, e: &hp_vmf::Entity) -> Vec<String> {
    let mut issues: Vec<String> = Vec::new();
    let Some(class) = f.class(&e.classname) else {
        return vec![format!("the FGD has no class {}", e.classname)];
    };
    for (k, v) in &e.props {
        if matches!(k.as_str(), "classname" | "id" | "origin" | "angles" | "spawnflags") || k.starts_with("replace") {
            continue;
        }
        match class.props.iter().find(|p| p.name.eq_ignore_ascii_case(k)) {
            None => issues.push(format!("key {k} is not in the FGD of {}", class.name)),
            Some(p) if !p.choices.is_empty() && !p.choices.iter().any(|c| c.value == *v) => {
                issues.push(format!("{k} = \"{v}\" is not one of its choices"));
            }
            _ => {}
        }
    }
    for c in &e.connections {
        if !class.outputs.iter().any(|o| o.name.eq_ignore_ascii_case(&c.output)) {
            issues.push(format!("{} has no output {}", class.name, c.output));
        }
        if c.target.starts_with('!') {
            continue;
        }
        let targets = target_classes(state, &c.target);
        if targets.is_empty() {
            issues.push(format!("no entity is named {} (output {})", c.target, c.output));
            continue;
        }
        let known = targets
            .iter()
            .any(|t| f.class(t).is_none_or(|tc| tc.inputs.iter().any(|i| i.name.eq_ignore_ascii_case(&c.input))));
        if !known {
            issues.push(format!("the entities named {} have no input {}", c.target, c.input));
        }
    }
    issues
}

fn set_keyvalue(state: &mut AppState, a: &Args<'_>) -> R<ToolResult> {
    let ids = target_entities(state, a, "ids")?;
    let key = a.str("key", 63)?.to_owned();
    check_key(&key)?;
    if key.eq_ignore_ascii_case("classname") {
        return Err("use set_entity_class to change the class".to_owned());
    }
    let n = match (a.opt_str("value", MAX_VALUE)?, a.opt_str("rename_to", 63)?) {
        (Some(_), Some(_)) => return Err("give value or rename_to, not both".to_owned()),
        (Some(v), None) => {
            check_value(&key, v)?;
            let v = v.to_owned();
            edit(state, "set_keyvalue", |doc| doc.set_entity_key(&ids, &key, &v))?
        }
        (None, Some(to)) => {
            check_key(to)?;
            let to = to.to_owned();
            edit(state, "set_keyvalue", |doc| doc.rename_entity_key(&ids, &key, &to))?
        }
        (None, None) => edit(state, "set_keyvalue", |doc| doc.remove_entity_key(&ids, &key))?,
    };
    Ok(ToolResult::json(json!({ "entities_changed": n })))
}

fn set_entity_class(state: &mut AppState, a: &Args<'_>) -> R<ToolResult> {
    let ids = target_entities(state, a, "ids")?;
    let class = class_name(a, "classname")?;
    let origin = hp_app::runtime::helpers::brush_entity_needs_origin(state.assets.fgd(), &class);
    let n = edit(state, "set_entity_class", |doc| {
        let n = doc.set_entity_class(&ids, &class)?;
        if origin {
            doc.add_missing_origins(&ids)?;
        }
        Ok(n)
    })?;
    let mut out = json!({ "entities_changed": n, "classname": class });
    if state.assets.fgd().is_some_and(|f| f.class(&class).is_none()) {
        out["warning"] = json!(format!("the FGD has no class {class}"));
    }
    Ok(ToolResult::json(out))
}

fn flag_bits(state: &AppState, class: &str, names: &[String]) -> R<u32> {
    let mut bits = 0u32;
    for n in names {
        let n = n.trim();
        if let Ok(v) = n.parse::<u32>() {
            bits |= v;
            continue;
        }
        let def = state
            .assets
            .fgd()
            .and_then(|f| f.class(class))
            .and_then(|c| c.props.iter().find(|p| p.name.eq_ignore_ascii_case("spawnflags")))
            .and_then(|p| p.flags.iter().find(|fl| fl.display.eq_ignore_ascii_case(n)));
        bits |= def.map(|fl| fl.value).ok_or_else(|| format!("{class} has no spawnflag named \"{n}\""))?;
    }
    Ok(bits)
}

fn set_spawnflags(state: &mut AppState, a: &Args<'_>) -> R<ToolResult> {
    let ids = target_entities(state, a, "ids")?;
    let set = a.strings("set", 32)?;
    let clear = a.strings("clear", 32)?;
    if set.is_empty() && clear.is_empty() {
        return Err("give set or clear".to_owned());
    }
    let mut plan = Vec::new();
    for id in &ids {
        let e = &state.doc.scene().entity(*id).ok_or("gone")?.entity;
        let on = flag_bits(state, &e.classname, &set)?;
        let off = flag_bits(state, &e.classname, &clear)?;
        let old: u32 = e.get("spawnflags").and_then(|v| v.trim().parse().ok()).unwrap_or(0);
        plan.push((*id, (old | on) & !off));
    }
    edit(state, "set_spawnflags", |doc| {
        for (id, flags) in &plan {
            doc.edit_entity(*id, |e| e.set("spawnflags", flags.to_string()))?;
        }
        Ok(())
    })?;
    Ok(ToolResult::json(
        json!({ "spawnflags": plan.iter().map(|(id, f)| json!({ "id": id, "spawnflags": f })).collect::<Vec<_>>() }),
    ))
}

fn conn_json(i: usize, c: &Connection) -> Value {
    json!({
        "index": i,
        "output": c.output,
        "target": c.target,
        "input": c.input,
        "parameter": c.param,
        "delay": c.delay,
        "times": c.times,
    })
}

fn list_connections(state: &mut AppState, a: &Args<'_>) -> R<ToolResult> {
    let id = entity_id(&state.doc, a, "id")?;
    let e = &state.doc.scene().entity(id).ok_or("gone")?.entity;
    let outputs: Vec<Value> = e.connections.iter().enumerate().map(|(i, c)| conn_json(i, c)).collect();
    let name = e.get("targetname").unwrap_or_default().to_owned();
    let incoming: Vec<Value> = if name.is_empty() {
        Vec::new()
    } else {
        state
            .doc
            .outputs_targeting(&name)
            .into_iter()
            .filter_map(|(from, i)| {
                let c = state.doc.scene().entity(from)?.entity.connections.get(i)?;
                let mut v = conn_json(i, c);
                v["from"] = json!(from);
                Some(v)
            })
            .collect()
    };
    Ok(ToolResult::json(json!({ "id": id, "targetname": name, "outputs": outputs, "inputs": incoming })))
}

fn conn_field(a: &Args<'_>, key: &str, required: bool) -> R<Option<String>> {
    let Some(v) = a.opt_str(key, MAX_NAME)? else {
        return if required { Err(format!("{key} is required")) } else { Ok(None) };
    };
    let v = v.trim();
    if v.contains([',', '\u{1b}', '"', '\n', '\r', '\0']) {
        return Err(format!("{key} must not contain commas, quotes or line breaks"));
    }
    if required && v.is_empty() && key != "parameter" {
        return Err(format!("{key} must not be empty"));
    }
    Ok(Some(v.to_owned()))
}

fn add_connection(state: &mut AppState, a: &Args<'_>) -> R<ToolResult> {
    let ids = target_entities(state, a, "ids")?;
    let c = Connection {
        output: conn_field(a, "output", true)?.unwrap_or_default(),
        target: conn_field(a, "target", true)?.unwrap_or_default(),
        input: conn_field(a, "input", true)?.unwrap_or_default(),
        param: conn_field(a, "parameter", false)?.unwrap_or_default(),
        delay: a.opt_f64("delay", 0.0, 100_000.0)?.unwrap_or(0.0) as f32,
        times: opt_i64(a, "times", -1, 1_000_000)?.map_or(-1, |t| t as i32),
        ..Default::default()
    };
    let n = edit(state, "add_connection", |doc| doc.add_connection(&ids, &c))?;
    let mut out = json!({ "entities_changed": n });
    if !c.target.starts_with('!') && target_classes(state, &c.target).is_empty() {
        out["warning"] = json!(format!("no entity is named {} yet", c.target));
    }
    Ok(ToolResult::json(out))
}

fn edit_connection(state: &mut AppState, a: &Args<'_>) -> R<ToolResult> {
    let id = entity_id(&state.doc, a, "id")?;
    let index = a.opt_u64("index", 0, 100_000)?.ok_or("index is required")? as usize;
    let e = &state.doc.scene().entity(id).ok_or("gone")?.entity;
    let mut c =
        e.connections.get(index).cloned().ok_or_else(|| format!("entity {id} has {} outputs", e.connections.len()))?;
    if let Some(v) = conn_field(a, "output", false)? {
        c.output = v;
    }
    if let Some(v) = conn_field(a, "target", false)? {
        c.target = v;
    }
    if let Some(v) = conn_field(a, "input", false)? {
        c.input = v;
    }
    if let Some(v) = conn_field(a, "parameter", false)? {
        c.param = v;
    }
    if let Some(d) = a.opt_f64("delay", 0.0, 100_000.0)? {
        c.delay = d as f32;
    }
    if let Some(t) = opt_i64(a, "times", -1, 1_000_000)? {
        c.times = t as i32;
    }
    if c.output.is_empty() || c.target.is_empty() || c.input.is_empty() {
        return Err("output, target and input must not be empty".to_owned());
    }
    let json = conn_json(index, &c);
    edit(state, "edit_connection", |doc| doc.set_connection(id, index, c))?;
    Ok(ToolResult::json(json))
}

fn remove_connections(state: &mut AppState, a: &Args<'_>) -> R<ToolResult> {
    let ids = target_entities(state, a, "ids")?;
    if let Some(index) = a.opt_u64("index", 0, 100_000)? {
        let [id] = ids.as_slice() else { return Err("index needs exactly one entity in ids".to_owned()) };
        if a.get("output").is_some() || a.get("target").is_some() || a.get("input").is_some() {
            return Err("index cannot be combined with patterns".to_owned());
        }
        let id = *id;
        let count = state.doc.scene().entity(id).map_or(0, |e| e.entity.connections.len());
        if index as usize >= count {
            return Err(format!("entity {id} has {count} outputs"));
        }
        edit(state, "remove_connections", |doc| doc.remove_connection(id, index as usize))?;
        return Ok(ToolResult::json(json!({ "removed": 1 })));
    }
    let (o, t, i) = (pat(a, "output")?, pat(a, "target")?, pat(a, "input")?);
    if o.is_none() && t.is_none() && i.is_none() {
        return Err("give index, or output, target or input patterns (\"*\" removes every output)".to_owned());
    }
    let m = |p: &Option<String>, s: &str| p.as_deref().is_none_or(|p| hp_editor::glob_match(p, s));
    let pred = |c: &Connection| m(&o, &c.output) && m(&t, &c.target) && m(&i, &c.input);
    let n = edit(state, "remove_connections", |doc| doc.remove_connections_where(&ids, &pred))?;
    Ok(ToolResult::json(json!({ "removed": n })))
}

fn get_io_graph(state: &mut AppState, a: &Args<'_>) -> R<ToolResult> {
    let broken_only = a.opt_bool("broken_only")?.unwrap_or(false);
    let scene = state.doc.scene();
    let mut names: std::collections::HashMap<String, Vec<u32>> = std::collections::HashMap::new();
    for (id, e) in scene.entities() {
        if let Some(n) = e.entity.get("targetname").filter(|n| !n.is_empty()) {
            names.entry(n.to_ascii_lowercase()).or_default().push(*id);
        }
    }
    let mut ids: Vec<u32> = scene.entities().keys().copied().collect();
    ids.sort_unstable();
    let mut all = Vec::new();
    for id in ids {
        let Some(e) = scene.entity(id) else { continue };
        for (i, c) in e.entity.connections.iter().enumerate() {
            let t = c.target.to_ascii_lowercase();
            let targets: Vec<u32> = if t.starts_with('!') {
                Vec::new()
            } else if t.contains('*') {
                names
                    .iter()
                    .filter(|(n, _)| hp_editor::glob_match(&t, n))
                    .flat_map(|(_, v)| v.iter().copied())
                    .collect()
            } else {
                names.get(&t).cloned().unwrap_or_default()
            };
            let broken = !t.starts_with('!') && targets.is_empty();
            if broken_only && !broken {
                continue;
            }
            let mut v = conn_json(i, c);
            v["from"] = json!(id);
            v["from_class"] = json!(e.entity.classname);
            v["targets"] = json!(targets.iter().take(50).collect::<Vec<_>>());
            v["broken"] = json!(broken);
            all.push(v);
            if all.len() >= MAX_GRAPH * 4 {
                break;
            }
        }
    }
    let (page_items, total) = page(all, a)?;
    Ok(ToolResult::json(json!({ "total": total, "connections": page_items })))
}

fn rename_targetname(state: &mut AppState, a: &Args<'_>) -> R<ToolResult> {
    let old = a.str("old", MAX_NAME)?.trim().to_owned();
    let new = a.str("new", MAX_NAME)?.trim().to_owned();
    if old.is_empty() || new.is_empty() {
        return Err("old and new must not be empty".to_owned());
    }
    check_value("new", &new)?;
    if new.contains([',', '\u{1b}']) {
        return Err("new must not contain commas".to_owned());
    }
    if state.doc.entities_named(&old).is_empty() {
        return Err(format!("no entity is named {old}"));
    }
    let refs = a.opt_bool("update_references")?.unwrap_or(true);
    let n = edit(state, "rename_targetname", |doc| doc.rename_targetname(&old, &new, refs))?;
    Ok(ToolResult::json(json!({ "changed": n })))
}

fn find_entities(state: &mut AppState, a: &Args<'_>) -> R<ToolResult> {
    let q = EntityQuery {
        classname: pat(a, "classname")?,
        targetname: pat(a, "targetname")?,
        key: pat(a, "key")?,
        value: pat(a, "value")?,
        include_hidden: a.opt_bool("include_hidden")?.unwrap_or(true),
    };
    if q.value.is_some() && q.key.is_none() {
        return Err("value needs key".to_owned());
    }
    let mut ids = state.doc.find_entities(&q);
    ids.sort_unstable();
    let (ids, total) = page(ids, a)?;
    let list: Vec<Value> = ids
        .into_iter()
        .filter_map(|id| {
            let e = &state.doc.scene().entity(id)?.entity;
            Some(json!({
                "id": id,
                "classname": e.classname,
                "targetname": e.get("targetname"),
                "origin": e.get("origin").and_then(hp_vmf::parse_vec3).map(v3),
            }))
        })
        .collect();
    Ok(ToolResult::json(json!({ "total": total, "entities": list })))
}

fn text_query(state: &AppState, a: &Args<'_>) -> R<TextQuery> {
    let find = a.str("find", hp_editor::MAX_TEXT_LEN.min(MAX_VALUE))?.to_owned();
    if find.is_empty() {
        return Err("find must not be empty".to_owned());
    }
    let only = if a.get("ids").is_some() { Some(entity_ids(&state.doc, a, "ids")?) } else { None };
    Ok(TextQuery {
        find,
        whole_word: a.opt_bool("whole_word")?.unwrap_or(false),
        case_sensitive: a.opt_bool("case_sensitive")?.unwrap_or(false),
        key_names: a.opt_bool("key_names")?.unwrap_or(false),
        only,
        visible_only: a.opt_bool("visible_only")?.unwrap_or(false),
    })
}

fn field_json(f: &TextField) -> Value {
    match f {
        TextField::Value(k) => json!({ "value_of": k }),
        TextField::KeyName(k) => json!({ "key_name": k }),
        TextField::Connection { index, part } => {
            json!({ "output": index, "part": format!("{part:?}").to_ascii_lowercase() })
        }
    }
}

fn find_text(state: &mut AppState, a: &Args<'_>) -> R<ToolResult> {
    let q = text_query(state, a)?;
    let matches = state.doc.find_text(&q);
    let (matches, total) = page(matches, a)?;
    let list: Vec<Value> = matches
        .iter()
        .map(|m| json!({ "entity": m.entity, "world": m.world, "field": field_json(&m.field), "text": m.text, "count": m.ranges.len() }))
        .collect();
    Ok(ToolResult::json(json!({ "total": total, "matches": list })))
}

fn replace_text(state: &mut AppState, a: &Args<'_>) -> R<ToolResult> {
    let q = text_query(state, a)?;
    let with = a.str("replace", MAX_VALUE)?.to_owned();
    if with.contains(['"', '\n', '\r', '\0']) {
        return Err("replace must not contain quotes or line breaks".to_owned());
    }
    let matches = state.doc.find_text(&q);
    if matches.is_empty() {
        return Ok(ToolResult::json(json!({ "replaced": 0 })));
    }
    let n = edit(state, "replace_text", |doc| doc.replace_text(&q, &matches, &with))?;
    Ok(ToolResult::json(json!({ "replaced": n })))
}

fn center_entity_origins(state: &mut AppState, a: &Args<'_>) -> R<ToolResult> {
    let ids = target_entities(state, a, "ids")?;
    let n = edit(state, "center_entity_origins", |doc| doc.center_origins(&ids))?;
    Ok(ToolResult::json(json!({ "entities_changed": n })))
}

fn entity_report(state: &mut AppState, _a: &Args<'_>) -> R<ToolResult> {
    let mut by_class: std::collections::BTreeMap<String, (usize, usize)> = std::collections::BTreeMap::new();
    let scene = state.doc.scene();
    for (id, e) in scene.entities() {
        let entry = by_class.entry(e.entity.classname.to_ascii_lowercase()).or_default();
        entry.0 += 1;
        entry.1 += scene.entity_solid_count(*id);
    }
    let mut list: Vec<Value> =
        by_class.into_iter().map(|(c, (n, s))| json!({ "classname": c, "count": n, "solids": s })).collect();
    list.sort_by(|x, y| y["count"].as_u64().cmp(&x["count"].as_u64()));
    Ok(ToolResult::json(json!({ "entities": scene.entities().len(), "classes": list })))
}

fn create_entity_fgd(state: &mut AppState, a: &Args<'_>) -> R<ToolResult> {
    let class = class_name(a, "classname")?;
    let origin = a.vec3("origin")?;
    let mut kv: Vec<(String, String)> = state
        .assets
        .fgd()
        .and_then(|f| f.class(&class))
        .map(|c| c.default_keyvalues())
        .unwrap_or_default()
        .into_iter()
        .filter(|(k, _)| !matches!(k.to_ascii_lowercase().as_str(), "origin" | "classname" | "id"))
        .collect();
    let mut set = |k: String, v: String| match kv.iter_mut().find(|(x, _)| x.eq_ignore_ascii_case(&k)) {
        Some(slot) => slot.1 = v,
        None => kv.push((k, v)),
    };
    if let Some(angles) = a.opt_vec3("angles")? {
        set("angles".to_owned(), hp_vmf::fmt_vec3(angles));
    }
    for (k, v) in a.keyvalues("keyvalues")? {
        if !matches!(k.to_ascii_lowercase().as_str(), "origin" | "classname") {
            set(k, v);
        }
    }
    let known = state.assets.fgd().is_some_and(|f| f.class(&class).is_some());
    let id = edit(state, "create_entity_fgd", |doc| doc.create_point_entity(&class, origin, &kv))?;
    let mut out = json!({ "id": id, "classname": class, "keyvalues": kv.len() });
    if !known {
        out["warning"] = json!(format!("the FGD has no class {class}: no defaults were added"));
    }
    Ok(ToolResult::json(out))
}

fn copy_keyvalues(state: &mut AppState, a: &Args<'_>) -> R<ToolResult> {
    let from = entity_id(&state.doc, a, "from")?;
    let targets: Vec<u32> = target_entities(state, a, "ids")?.into_iter().filter(|id| *id != from).collect();
    if targets.is_empty() {
        return Err("give target entities other than the source".to_owned());
    }
    let only: Vec<String> = a.strings("keys", 256)?.into_iter().map(|k| k.to_ascii_lowercase()).collect();
    let src = &state.doc.scene().entity(from).ok_or("gone")?.entity;
    let kv: Vec<(String, String)> = src
        .props
        .iter()
        .filter(|(k, _)| {
            let k = k.to_ascii_lowercase();
            if only.is_empty() {
                !matches!(k.as_str(), "id" | "classname" | "origin" | "targetname")
            } else {
                only.contains(&k)
            }
        })
        .cloned()
        .collect();
    if kv.is_empty() {
        return Err("the source has none of these keys".to_owned());
    }
    edit(state, "copy_keyvalues", |doc| {
        for (k, v) in &kv {
            doc.set_entity_key(&targets, k, v)?;
        }
        Ok(())
    })?;
    Ok(ToolResult::json(
        json!({ "keys": kv.iter().map(|(k, _)| k.as_str()).collect::<Vec<_>>(), "entities": targets.len() }),
    ))
}

fn list_targetnames(state: &mut AppState, a: &Args<'_>) -> R<ToolResult> {
    let pattern = pat(a, "pattern")?;
    let mut names: std::collections::BTreeMap<String, Vec<u32>> = std::collections::BTreeMap::new();
    for (id, e) in state.doc.scene().entities() {
        if let Some(n) = e.entity.get("targetname").filter(|n| !n.is_empty())
            && pattern.as_deref().is_none_or(|p| hp_editor::glob_match(p, n))
        {
            names.entry(n.to_owned()).or_default().push(*id);
        }
    }
    let list: Vec<(String, Vec<u32>)> = names.into_iter().collect();
    let (list, total) = page(list, a)?;
    let out: Vec<Value> = list
        .into_iter()
        .map(|(n, mut ids)| {
            ids.sort_unstable();
            let inputs = state.doc.outputs_targeting(&n).len();
            json!({ "name": n, "entities": ids, "outputs_aimed": inputs })
        })
        .collect();
    Ok(ToolResult::json(json!({ "total": total, "names": out })))
}

fn query_box(state: &mut AppState, a: &Args<'_>) -> R<ToolResult> {
    let b = hp_geom::Aabb::new(a.vec3("min")?, a.vec3("max")?);
    let inside = a.opt_bool("inside")?.unwrap_or(false);
    let limit = a.opt_u64("limit", 1, hp_mcp::tools::MAX_LIST)?.unwrap_or(100) as usize;
    let f = bounds_fn(state);
    let scene = state.doc.scene();
    let hit = |o: &hp_geom::Aabb| {
        if inside {
            b.min.cmple(o.min).all() && o.max.cmple(b.max).all()
        } else {
            b.min.cmple(o.max).all() && o.min.cmple(b.max).all()
        }
    };
    let mut solids: Vec<u32> =
        scene.solids().keys().copied().filter(|id| scene.solid_bounds(*id).is_some_and(|sb| hit(&sb))).collect();
    let mut entities: Vec<u32> = scene
        .entities()
        .iter()
        .filter(|(id, e)| {
            let eb = if scene.is_brush_entity(**id) { scene.entity_solid_bounds(**id) } else { f(&e.entity) };
            eb.or_else(|| e.entity.get("origin").and_then(hp_vmf::parse_vec3).map(|p| hp_geom::Aabb::new(p, p)))
                .is_some_and(|eb| hit(&eb))
        })
        .map(|(id, _)| *id)
        .collect();
    solids.sort_unstable();
    entities.sort_unstable();
    let ents: Vec<Value> = entities
        .iter()
        .take(limit)
        .map(|id| json!({ "id": id, "classname": scene.entity(*id).map(|n| n.entity.classname.clone()) }))
        .collect();
    Ok(ToolResult::json(json!({
        "solids_total": solids.len(),
        "entities_total": entities.len(),
        "solids": solids.into_iter().take(limit).collect::<Vec<_>>(),
        "entities": ents,
    })))
}

fn entity_origin(state: &AppState, id: u32) -> Option<glam::DVec3> {
    let scene = state.doc.scene();
    let e = &scene.entity(id)?.entity;
    e.get("origin").and_then(hp_vmf::parse_vec3).or_else(|| scene.entity_solid_bounds(id).map(|b| b.center()))
}

fn max_distance_json(c: &hp_fgd::ResolvedClass) -> Value {
    use hp_editor::max_distance::{DistanceKind, rule_for};
    match rule_for(&c.name, &|k| Some(c.property(k).is_some())) {
        Some(r) => {
            let kind = match r.kind {
                DistanceKind::LightCutoff => "light_cutoff",
                DistanceKind::LightLength => "light_length",
                DistanceKind::ViewFade => "view_fade",
                DistanceKind::GlowFade => "glow_fade",
                DistanceKind::BrushFade => "brush_fade",
            };
            json!({ "key": r.key, "kind": kind, "default": r.default })
        }
        None => Value::Null,
    }
}

fn set_max_distance(state: &mut AppState, a: &Args<'_>) -> R<ToolResult> {
    let ids = target_entities(state, a, "ids")?;
    let d = a.opt_f64("distance", 0.0, hp_editor::max_distance::MAX_DISTANCE)?.ok_or("distance is required")?;
    let mut known: std::collections::HashMap<(String, String), Option<bool>> = std::collections::HashMap::new();
    for id in &ids {
        if let Some(n) = state.doc.scene().entity(*id) {
            let class = n.entity.classname.to_ascii_lowercase();
            for key in ["fademaxdist"] {
                let has = hp_ui_entities::max_distance::fgd_has_key(state.assets.fgd(), &class, key);
                known.insert((class.clone(), key.to_owned()), has);
            }
        }
    }
    let has_key = |c: &str, k: &str| known.get(&(c.to_ascii_lowercase(), k.to_ascii_lowercase())).copied().flatten();
    let label = super::tools::undo_label("set_max_distance");
    let r = edit(state, "set_max_distance", |doc| doc.set_max_distance(&ids, d, &label, &has_key))?;
    Ok(ToolResult::json(json!({
        "distance": d.round(),
        "changed": r.changed,
        "unchanged": r.unchanged.len(),
        "skipped": r.skipped.len(),
        "skipped_classes": r.skipped_classes,
        "message": hp_ui_entities::max_distance::report_text(&r),
    })))
}

fn aim_entities(state: &mut AppState, a: &Args<'_>) -> R<ToolResult> {
    let ids = target_entities(state, a, "ids")?;
    let point = match (a.opt_vec3("point")?, a.opt_u64("target", 1, u64::from(u32::MAX))?) {
        (Some(p), None) => p,
        (None, Some(t)) => {
            let t = t as u32;
            if state.doc.scene().entity(t).is_none() {
                return Err(format!("no entity has the id {t}"));
            }
            entity_origin(state, t).ok_or("the target has no origin")?
        }
        _ => return Err("give point or target".to_owned()),
    };
    let yaw_only = a.opt_bool("yaw_only")?.unwrap_or(false);
    let mut plan = Vec::new();
    for id in &ids {
        let o = entity_origin(state, *id).ok_or_else(|| format!("entity {id} has no origin"))?;
        let d = point - o;
        if d.length() < 1e-6 {
            return Err(format!("entity {id} is at the point it should face"));
        }
        let yaw = d.y.atan2(d.x).to_degrees().rem_euclid(360.0);
        let up = if yaw_only { 0.0 } else { (d.z / d.length()).asin().to_degrees() };
        let class = state.doc.scene().entity(*id).map(|e| e.entity.classname.to_ascii_lowercase()).unwrap_or_default();
        let vrad = matches!(class.as_str(), "light_spot" | "light_environment");
        let pitch = if vrad { up } else { -up };
        let r = |x: f64| (x * 100.0).round() / 100.0 + 0.0;
        plan.push((*id, format!("{} {} 0", r(pitch), r(yaw)), vrad.then(|| format!("{}", r(up)))));
    }
    edit(state, "aim_entities", |doc| {
        for (id, angles, pitch) in &plan {
            doc.edit_entity(*id, |e| {
                e.set("angles", angles.clone());
                if let Some(p) = pitch {
                    e.set("pitch", p.clone());
                }
            })?;
        }
        Ok(())
    })?;
    Ok(ToolResult::json(
        json!({ "aimed": plan.iter().map(|(id, angles, _)| json!({ "id": id, "angles": angles })).collect::<Vec<_>>() }),
    ))
}

fn set_current_entity_class(state: &mut AppState, a: &Args<'_>) -> R<ToolResult> {
    let class = class_name(a, "classname")?;
    let model = match a.opt_str("model", MAX_NAME)? {
        Some(m) => {
            let m = m.trim().replace('\\', "/").to_ascii_lowercase();
            if !m.starts_with("models/") || !m.ends_with(".mdl") || m.contains("..") {
                return Err("model must look like models/.../name.mdl".to_owned());
            }
            Some(m)
        }
        None => None,
    };
    state.current_entity_class = class.clone();
    if model.is_some() {
        state.current_model = model.clone();
    }
    Ok(ToolResult::json(json!({ "classname": class, "model": state.current_model })))
}

#[cfg(test)]
mod tests {
    use serde_json::json;

    use super::super::semantic_tests::{err, ok};

    #[test]
    fn keyvalues_classes_and_flags() {
        let mut s = hp_app::testing::test_state();
        ok(&mut s, "set_keyvalue", json!({ "ids": [4, 5], "key": "targetname", "value": "thing" }));
        let r = ok(&mut s, "find_entities", json!({ "targetname": "thing" }));
        assert_eq!(r["total"], 2);
        ok(&mut s, "set_keyvalue", json!({ "ids": [4], "key": "targetname", "rename_to": "parentname" }));
        assert_eq!(s.doc.scene().entity(4).unwrap().entity.get("parentname"), Some("thing"));
        ok(&mut s, "set_keyvalue", json!({ "ids": [4], "key": "parentname" }));
        assert!(s.doc.scene().entity(4).unwrap().entity.get("parentname").is_none());
        err(&mut s, "set_keyvalue", json!({ "ids": [4], "key": "id", "value": "3" }));
        err(&mut s, "set_keyvalue", json!({ "ids": [999], "key": "a", "value": "3" }));
        ok(&mut s, "set_entity_class", json!({ "ids": [5], "classname": "light_spot" }));
        assert_eq!(s.doc.scene().entity(5).unwrap().entity.classname, "light_spot");
        ok(&mut s, "set_spawnflags", json!({ "ids": [6], "set": ["1", "64"] }));
        ok(&mut s, "set_spawnflags", json!({ "ids": [6], "clear": ["1"] }));
        assert_eq!(s.doc.scene().entity(6).unwrap().entity.get("spawnflags"), Some("64"));
        let r = ok(&mut s, "entity_report", json!({}));
        assert!(
            r["classes"].as_array().unwrap().iter().any(|c| c["classname"] == "trigger_multiple" && c["solids"] == 1)
        );
        ok(
            &mut s,
            "set_current_entity_class",
            json!({ "classname": "prop_physics", "model": "models/props_c17/oildrum001.mdl" }),
        );
        assert_eq!(s.current_entity_class, "prop_physics");
        err(&mut s, "set_current_entity_class", json!({ "classname": "x", "model": "../a.mdl" }));
        ok(&mut s, "center_entity_origins", json!({ "ids": [6] }));
        ok(&mut s, "set_entity_class", json!({ "ids": [5], "classname": "light_spot" }));
        for id in [4u32, 5] {
            let o = super::entity_origin(&s, id).unwrap();
            ok(&mut s, "aim_entities", json!({ "ids": [id], "point": [o.x, o.y, o.z - 100.0] }));
        }
        assert_eq!(s.doc.scene().entity(5).unwrap().entity.get("pitch"), Some("-90"), "VRAD lights: down is -90");
        assert!(
            s.doc.scene().entity(4).unwrap().entity.get("angles").unwrap().starts_with("90 "),
            "entities: down is +90"
        );
        let o = super::entity_origin(&s, 4).unwrap();
        let r = ok(
            &mut s,
            "aim_entities",
            json!({ "ids": [4], "point": [o.x + 10.0, o.y + 10.0, o.z + 500.0], "yaw_only": true }),
        );
        assert_eq!(r["aimed"][0]["angles"], "0 45 0");
        err(&mut s, "aim_entities", json!({ "ids": [4] }));
        let r = ok(
            &mut s,
            "create_entity_fgd",
            json!({ "classname": "info_target", "origin": [1, 2, 3], "angles": [0, 90, 0], "keyvalues": { "targetname": "t1" } }),
        );
        assert!(r["warning"].is_string(), "no FGD in the tests");
        let t = r["id"].as_u64().unwrap() as u32;
        assert_eq!(s.doc.scene().entity(t).unwrap().entity.get("angles"), Some("0 90 0"));
        ok(&mut s, "set_keyvalue", json!({ "ids": [5], "key": "_light", "value": "255 0 0 200" }));
        let r = ok(&mut s, "copy_keyvalues", json!({ "from": 5, "ids": [t], "keys": ["_light"] }));
        assert_eq!(r["entities"], 1);
        assert_eq!(s.doc.scene().entity(t).unwrap().entity.get("_light"), Some("255 0 0 200"));
        err(&mut s, "copy_keyvalues", json!({ "from": 5, "ids": [5] }));
        let n = ok(&mut s, "list_targetnames", json!({ "pattern": "t*" }));
        assert_eq!(n["names"][0]["entities"][0], t);
        let q =
            ok(&mut s, "query_box", json!({ "min": [-1000, -1000, -100], "max": [1000, 1000, 1000], "inside": true }));
        assert!(q["solids_total"].as_u64().unwrap() >= 2);
        assert!(q["entities"].as_array().unwrap().iter().any(|e| e["id"] == t));
    }

    #[test]
    fn maximum_distance() {
        let mut s = hp_app::testing::test_state();
        ok(&mut s, "set_entity_class", json!({ "ids": [5], "classname": "light_spot" }));
        let r = ok(&mut s, "set_max_distance", json!({ "ids": [5, 6], "distance": 300.4 }));
        assert_eq!(r["changed"], json!([5]));
        assert_eq!((r["unchanged"].as_u64(), r["skipped"].as_u64()), (Some(0), Some(1)));
        assert_eq!(r["skipped_classes"], json!(["trigger_multiple"]));
        assert_eq!(s.doc.scene().entity(5).unwrap().entity.get("_distance"), Some("300"));
        assert_eq!(s.doc.undo_labels().last().copied(), Some("MCP: set_max_distance"));
        let again = ok(&mut s, "set_max_distance", json!({ "ids": [5], "distance": 300 }));
        assert_eq!(again["unchanged"], 1);
        err(&mut s, "set_max_distance", json!({ "ids": [5] }));
        err(&mut s, "set_max_distance", json!({ "ids": [5], "distance": -1 }));
    }

    const FGD: &str = r#"
@BaseClass = Targetname [ targetname(target_source) : "Name" ]
@PointClass base(Targetname) studio() = prop_test : "A test prop"
[
    skin(choices) : "Skin" : "0" = [ 0 : "Default" 1 : "Red" ]
    spawnflags(flags) = [ 1 : "Start asleep" : 0 ]
    input Wake(void) : "Wake up"
    output OnWake(void) : "Woken"
]
"#;

    #[test]
    fn fgd_classes_and_checks() {
        let f = hp_fgd::Fgd::parse(FGD, None).resolve();
        let c = f.class("prop_test").unwrap();
        let j = super::class_json(c);
        assert_eq!(j["kind"], "point");
        assert!(
            j["properties"]
                .as_array()
                .unwrap()
                .iter()
                .any(|p| p["name"] == "skin" && p["choices"][1]["label"] == "Red")
        );
        assert_eq!(j["inputs"][0]["name"], "Wake");
        assert!(j["max_distance"].is_null(), "no fade keys");
        let light =
            hp_fgd::Fgd::parse("@PointClass = light [ _distance(integer) : \"Maximum distance\" : 0 ]", None).resolve();
        assert_eq!(super::class_json(light.class("light").unwrap())["max_distance"]["key"], "_distance");
        let mut s = hp_app::testing::test_state();
        ok(&mut s, "set_entity_class", json!({ "ids": [5], "classname": "prop_test" }));
        ok(&mut s, "set_keyvalue", json!({ "ids": [5], "key": "skin", "value": "7" }));
        ok(&mut s, "add_connection", json!({ "ids": [5], "output": "OnBoom", "target": "nobody", "input": "Wake" }));
        let e = s.doc.scene().entity(5).unwrap().entity.clone();
        let issues = super::entity_issues(&s, &f, &e).join("; ");
        assert!(
            issues.contains("not one of its choices")
                && issues.contains("no output OnBoom")
                && issues.contains("nobody"),
            "{issues}"
        );
        assert!(issues.contains("key style") || issues.contains("is not in the FGD"), "{issues}");
    }

    #[test]
    fn connections_and_text() {
        let mut s = hp_app::testing::test_state();
        ok(&mut s, "set_keyvalue", json!({ "ids": [5], "key": "targetname", "value": "lamp" }));
        ok(
            &mut s,
            "add_connection",
            json!({ "ids": [6], "output": "OnTrigger", "target": "lamp", "input": "TurnOn", "delay": 0.5 }),
        );
        let r = ok(
            &mut s,
            "add_connection",
            json!({ "ids": [6], "output": "OnTrigger", "target": "nobody", "input": "Kill" }),
        );
        assert!(r["warning"].is_string());
        let l = ok(&mut s, "list_connections", json!({ "id": 5 }));
        assert_eq!(l["inputs"][0]["from"], 6);
        let g = ok(&mut s, "get_io_graph", json!({ "broken_only": true }));
        let broken = g["connections"].as_array().unwrap();
        assert!(broken.iter().all(|c| c["broken"] == true));
        assert_eq!(broken.iter().filter(|c| c["target"] == "nobody").count(), 1);
        ok(&mut s, "edit_connection", json!({ "id": 6, "index": 2, "target": "lamp", "times": 1 }));
        err(&mut s, "edit_connection", json!({ "id": 6, "index": 9, "target": "x" }));
        err(
            &mut s,
            "add_connection",
            json!({ "ids": [6], "output": "On,Trigger", "target": "lamp", "input": "TurnOn" }),
        );
        ok(&mut s, "rename_targetname", json!({ "old": "lamp", "new": "lamp2" }));
        let l = ok(&mut s, "list_connections", json!({ "id": 6 }));
        assert_eq!(l["outputs"][1]["target"], "lamp2");
        assert_eq!(l["outputs"][2]["target"], "lamp2");
        let f = ok(&mut s, "find_text", json!({ "find": "lamp2" }));
        assert!(f["total"].as_u64().unwrap() >= 3);
        let r = ok(&mut s, "replace_text", json!({ "find": "lamp2", "replace": "bulb", "whole_word": true }));
        assert!(r["replaced"].as_u64().unwrap() >= 3);
        assert_eq!(s.doc.undo_label(), Some("MCP: replace_text"));
        let r = ok(&mut s, "remove_connections", json!({ "ids": [6], "input": "Turn*" }));
        assert_eq!(r["removed"], 2);
        err(&mut s, "remove_connections", json!({ "ids": [6] }));
        err(&mut s, "rename_targetname", json!({ "old": "nobody_at_all", "new": "x" }));
        let e = err(&mut s, "get_entity_class", json!({ "classname": "light" }));
        assert!(e.contains("FGD") || e.contains("class"));
    }
}
