use std::collections::BTreeMap;

use hp_app::AppState;
use hp_app::i18n::Lang;
use hp_app::settings::compilers::{
    self, CompilerProfile, CompilerStep, CompilerStepKind, MAX_COMPILER_NAME, MAX_COMPILER_STEPS, MAX_COMPILER_TEXT,
    MAX_COMPILERS,
};
use hp_compile::ToolPaths;
use hp_compile::valve::{ValveTool, ValveValue, check_value, valve_flag, valve_flags};
use hp_mcp::protocol::ToolResult;
use serde_json::{Map, Value, json};

use super::semantic::*;
use super::tools::{Args, R};

const MAX_KEY: usize = 128;
const MAX_OPTIONS: usize = 128;

domain! { "compile and play";
    list_compilers: Read, "List the compilers",
        "The compilers that can build the map for the current game, the active one first in the answer: Hammer \
        Pro's own compiler (id builtin, the default, cannot be deleted), the Valve tools detected next to the game \
        (vbsp, vvis, vrad of bin, bin/win64 or bin/x64, the compilers set in Hammer's GameConfig.txt and Hammer++'s \
        hammerplusplus_gameconfig.txt, or the Source SDK Base 2013 tools; ids detected:...; their steps can be \
        changed, restore goes back to what was found) and the user's own compilers (ids user:N). Each with its \
        steps (kind bsp, vis, rad, copy, game or custom, program, options, extra arguments, map arguments with \
        variables, working folder, enabled, keep going), mode_flags and the problems found (missing programs, bad \
        option values). Also lists the variables of the arguments.",
        {}, [];
    get_compiler: Read, "Compiler profile",
        "One compiler (default: the active one) with its steps, the problems found, and the command lines it would \
        run for the open map with the current compile settings (with mode_flags, the preset and modes of the \
        compile window add -onlyents, -onlyprops, -fast, -final, -ldr, -hdr, -both and -threads).",
        { "compiler": string("Compiler id or name (list_compilers). Default: the active one.") }, [];
    get_compiler_flags: Read, "Options of Valve's compilers",
        "Every option of Valve's vbsp (step bsp), vvis (step vis) and vrad (step rad) that the steps of a compiler \
        can set (steps[].options of create_compiler and set_compiler): the flag, what it does, the value it takes \
        (switch, integer or number with its range, text, file, folder, numbers), its default, its group, and \
        set_by when the compile window writes it itself while mode_flags is on.",
        {
            "step": choice("Only the options of one step.", &["bsp", "vis", "rad"]),
            "language": choice("Language of the descriptions (default en).", &["en", "fr"]),
        }, [];
    create_compiler: Editor, "Create a compiler",
        "Create a compiler of the user: a blank one (geometry, visibility and lighting steps with no program yet), \
        a copy of another (from: a detected Valve compiler or another of the user's), or with its steps \
        (programs, options as structured fields, extra and map arguments). Compilers run programs on this \
        computer: giving steps or from needs confirm: true (only when the user asked). select: true makes it the \
        compiler of the current game.",
        {
            "name": string("Name (unique, at most 64 characters; a number is added when taken)."),
            "from": string("Id or name of the compiler to copy."),
            "steps": compiler_steps(),
            "mode_flags": boolean("Pass the preset and modes of the compile window to the geometry, visibility and lighting steps (default true)."),
            "select": boolean("Make it the compiler of the current game."),
            "confirm": boolean("The user asked for this (needed with steps or from)."),
        }, ["name"];
    set_compiler: Editor, "Change a compiler",
        "Change a compiler of the user (ids user:N) or a detected one (ids detected:..., the changes are kept and \
        restore: true forgets them): its name (the user's only), mode_flags, or its whole list of steps with \
        their options (steps need confirm: true, only when the user asked: they run programs). Hammer Pro's own \
        compiler is set with set_compile_options. Answers the compiler like get_compiler.",
        {
            "compiler": string("Id or name of the compiler."),
            "name": string("New name (the user's compilers only)."),
            "mode_flags": boolean("Pass the preset and modes of the compile window to the geometry, visibility and lighting steps."),
            "steps": compiler_steps(),
            "restore": boolean("A detected compiler: forget the changes and go back to what was found."),
            "confirm": boolean("The user asked for this (needed with steps)."),
        }, ["compiler"];
    delete_compiler: Editor, "Delete a compiler",
        "Delete a compiler of the user. The games that used it go back to Hammer Pro's compiler. Detected \
        compilers cannot be deleted (set_compiler restore forgets their changes).",
        { "compiler": string("Id or name of the compiler.") }, ["compiler"];
    select_compiler: Editor, "Choose the compiler",
        "Choose the compiler of the current game (or of another game id from list_games): builtin for Hammer \
        Pro's compiler, a detected id or a compiler of the user. Saved in the settings, kept for each game; F9, \
        compile_map and the compile window use it.",
        {
            "compiler": string("Compiler id or name (list_compilers); builtin for Hammer Pro's compiler."),
            "game": string("Game id (list_games). Default: the game of the map."),
        }, ["compiler"];
    reset_compilers: Editor, "Reset the compilers",
        "Delete every compiler of the user and the changes to the detected ones, and go back to Hammer Pro's \
        compiler for every game. Needs confirm: true (only when the user asked).",
        { "confirm": boolean("The user asked for this.") }, ["confirm"];
}

fn compiler_steps() -> Value {
    json!({
        "type": "array",
        "maxItems": MAX_COMPILER_STEPS,
        "description": "The steps in order. kind: bsp (geometry, checked for leaks and the written BSP), vis, rad, \
            copy (args: the file and where to copy it), game (starts a program and does not wait; exe default \
            $gameexe) or custom (any program). exe: the program (full path, or a name found on the PATH). \
            options (bsp, vis and rad steps): the options of Valve's tool as an object, flag name to true (switch) \
            or its value, for example {\"bounce\": 8, \"StaticPropLighting\": true, \"lights\": \"$path/x.rad\"} \
            (get_compiler_flags lists them). extra: free arguments after the options. args: the map arguments \
            (default for bsp, vis and rad: -game $gamedir $path\\\\$file), or all the arguments of other steps, with \
            variables ($path, $file, $ext, $bspdir, $gamedir, $gameexe, $exedir, $bsp_exe, $vis_exe, $light_exe; a \
            value with spaces stays one argument). workdir: the working folder (default the build folder).",
        "items": {
            "type": "object",
            "properties": {
                "kind": { "type": "string", "enum": ["bsp", "vis", "rad", "copy", "game", "custom"] },
                "exe": { "type": "string" },
                "options": { "type": "object" },
                "extra": { "type": "string" },
                "args": { "type": "string" },
                "workdir": { "type": "string" },
                "enabled": { "type": "boolean" },
                "continue_on_error": { "type": "boolean" },
            },
            "required": ["kind"],
            "additionalProperties": false,
        },
    })
}

fn kind_name(p: &CompilerProfile) -> &'static str {
    if p.is_builtin() {
        "builtin"
    } else if p.is_detected() {
        "detected"
    } else {
        "user"
    }
}

fn tool_of(kind: CompilerStepKind) -> Option<ValveTool> {
    match kind {
        CompilerStepKind::Bsp => Some(ValveTool::Vbsp),
        CompilerStepKind::Vis => Some(ValveTool::Vvis),
        CompilerStepKind::Rad => Some(ValveTool::Vrad),
        _ => None,
    }
}

fn options_json(st: &CompilerStep) -> Value {
    let mut out = Map::new();
    for (flag, value) in &st.options {
        let name = flag.trim_start_matches('-').to_owned();
        let switch = tool_of(st.kind).and_then(|t| valve_flag(t, flag)).is_some_and(|f| f.value == ValveValue::Switch);
        out.insert(name, if switch { json!(true) } else { json!(value) });
    }
    Value::Object(out)
}

fn step_json(st: &CompilerStep) -> Value {
    let mut v = json!({
        "kind": st.kind.key(),
        "enabled": st.enabled,
        "exe": st.exe,
        "args": st.args,
        "workdir": st.workdir,
        "continue_on_error": st.continue_on_error,
    });
    if st.kind.is_compile() {
        v["options"] = options_json(st);
        v["extra"] = json!(st.extra);
    }
    v
}

fn profile_json(state: &AppState, p: &CompilerProfile, tools: Option<&ToolPaths>) -> Value {
    let active = state.settings.compile.active_compiler_id();
    json!({
        "id": p.id,
        "name": hp_ui_mapping::compiler_name(p),
        "kind": kind_name(p),
        "active": p.id.eq_ignore_ascii_case(active),
        "editable": !p.is_builtin(),
        "changed": p.is_detected() && compilers::is_edited(&state.settings.compile.compilers, &p.id),
        "mode_flags": p.mode_flags,
        "steps": p.steps.iter().map(step_json).collect::<Vec<_>>(),
        "problems": hp_ui_mapping::compiler_problems(p, tools),
    })
}

fn variables() -> Value {
    json!({
        "$path": "folder of the compiled VMF (the .hammerpro_build folder next to the map)",
        "$file": "map name, without folder or extension",
        "$ext": "map extension (vmf)",
        "$bspdir": "folder compiled maps are copied to",
        "$gamedir": "game folder (with gameinfo.txt)",
        "$gameexe": "game executable (also $game_exe)",
        "$exedir": "folder of the game executable",
        "$bsp_exe": "program of the geometry step",
        "$vis_exe": "program of the visibility step",
        "$light_exe": "program of the lighting step",
        "$$": "a $",
    })
}

fn find(state: &AppState, key: &str, tools: Option<&ToolPaths>) -> R<CompilerProfile> {
    let all = hp_ui_mapping::compiler_list(state, tools);
    compilers::find_compiler(&all, key)
        .cloned()
        .ok_or_else(|| format!("no compiler is named \"{}\" (list_compilers)", key.trim()))
}

fn store(state: &mut AppState, p: CompilerProfile) {
    let list = &mut state.settings.compile.compilers;
    match list.iter_mut().find(|q| q.id.eq_ignore_ascii_case(&p.id)) {
        Some(slot) => *slot = p,
        None => list.push(p),
    }
    state.settings_dirty = true;
}

fn text_field(m: &Map<String, Value>, key: &str, i: usize) -> R<String> {
    match m.get(key) {
        None => Ok(String::new()),
        Some(Value::String(s)) if s.len() <= MAX_COMPILER_TEXT && !s.chars().any(char::is_control) => Ok(s.clone()),
        Some(Value::String(_)) => {
            Err(format!("steps[{i}].{key}: at most {MAX_COMPILER_TEXT} characters, no control characters"))
        }
        Some(_) => Err(format!("steps[{i}].{key} must be a string")),
    }
}

fn bool_field(m: &Map<String, Value>, key: &str, i: usize, default: bool) -> R<bool> {
    match m.get(key) {
        None => Ok(default),
        Some(Value::Bool(b)) => Ok(*b),
        Some(_) => Err(format!("steps[{i}].{key} must be true or false")),
    }
}

fn parse_options(kind: CompilerStepKind, v: &Value, i: usize) -> R<BTreeMap<String, String>> {
    let map = v.as_object().ok_or_else(|| format!("steps[{i}].options must be an object: flag name to value"))?;
    let Some(tool) = tool_of(kind) else {
        return if map.is_empty() {
            Ok(BTreeMap::new())
        } else {
            Err(format!("steps[{i}].options: only bsp, vis and rad steps have options"))
        };
    };
    if map.len() > MAX_OPTIONS {
        return Err(format!("steps[{i}].options: at most {MAX_OPTIONS}"));
    }
    let mut out = BTreeMap::new();
    for (name, value) in map {
        let flag = valve_flag(tool, name).ok_or_else(|| {
            format!("steps[{i}].options: {} has no option \"{name}\" (get_compiler_flags)", tool.key())
        })?;
        let text = match value {
            Value::Null | Value::Bool(false) => continue,
            Value::Bool(true) if flag.value == ValveValue::Switch => String::new(),
            Value::Bool(true) => return Err(format!("steps[{i}].options.{name}: {} takes a value", flag.flag)),
            Value::Number(n) => n.to_string(),
            Value::String(s) if s.len() <= MAX_COMPILER_TEXT && !s.chars().any(char::is_control) => s.clone(),
            Value::Array(list) if list.iter().all(Value::is_number) => {
                list.iter().map(ToString::to_string).collect::<Vec<_>>().join(" ")
            }
            _ => return Err(format!("steps[{i}].options.{name}: true, a number, a text or a list of numbers")),
        };
        check_value(flag, &text).map_err(|e| format!("steps[{i}].options.{name}: {e}"))?;
        out.insert(flag.flag.to_owned(), text);
    }
    Ok(out)
}

fn parse_steps(v: &Value) -> R<Vec<CompilerStep>> {
    let list = v.as_array().ok_or("steps must be a list of steps")?;
    if list.len() > MAX_COMPILER_STEPS {
        return Err(format!("steps: at most {MAX_COMPILER_STEPS} steps"));
    }
    let mut out = Vec::with_capacity(list.len());
    for (i, item) in list.iter().enumerate() {
        let m = item.as_object().ok_or_else(|| format!("steps[{i}] must be an object"))?;
        for k in m.keys() {
            if !["kind", "exe", "options", "extra", "args", "workdir", "enabled", "continue_on_error"]
                .contains(&k.as_str())
            {
                return Err(format!("steps[{i}]: unknown field {k}"));
            }
        }
        let kind = m
            .get("kind")
            .and_then(Value::as_str)
            .and_then(CompilerStepKind::from_key)
            .ok_or_else(|| format!("steps[{i}].kind: bsp, vis, rad, copy, game or custom"))?;
        let defaults = CompilerStep::of_kind(kind);
        let exe = if m.contains_key("exe") { text_field(m, "exe", i)? } else { defaults.exe };
        let args = if m.contains_key("args") { text_field(m, "args", i)? } else { defaults.args };
        let options = m.get("options").map(|o| parse_options(kind, o, i)).transpose()?.unwrap_or_default();
        let extra = text_field(m, "extra", i)?;
        if !extra.is_empty() && !kind.is_compile() {
            return Err(format!("steps[{i}].extra: only bsp, vis and rad steps have extra arguments (use args)"));
        }
        out.push(CompilerStep {
            kind,
            exe,
            args,
            extra,
            options,
            workdir: text_field(m, "workdir", i)?,
            enabled: bool_field(m, "enabled", i, true)?,
            continue_on_error: bool_field(m, "continue_on_error", i, false)?,
        });
    }
    Ok(out)
}

fn clean_name(name: &str) -> R<String> {
    let n = name.trim();
    if n.is_empty() || n.chars().count() > MAX_COMPILER_NAME || n.chars().any(char::is_control) {
        return Err(format!("name: 1 to {MAX_COMPILER_NAME} characters, no control characters"));
    }
    if compilers::is_builtin_id(n) || n.to_ascii_lowercase().starts_with(compilers::DETECTED_PREFIX) {
        return Err(format!("name: \"{n}\" is reserved"));
    }
    Ok(n.to_owned())
}

fn needs_confirm(what: &str) -> String {
    format!("{what} run programs on this computer: only when the user asked (confirm: true)")
}

fn list_compilers(state: &mut AppState, _a: &Args<'_>) -> R<ToolResult> {
    let tools = hp_ui_mapping::compiler_tools(state);
    let tools = tools.as_ref();
    let mut all = hp_ui_mapping::compiler_list(state, tools);
    let active = state.settings.compile.active_compiler_id().to_owned();
    all.sort_by_key(|p| !p.id.eq_ignore_ascii_case(&active));
    Ok(ToolResult::json(json!({
        "game": state.settings.compile.game_key(),
        "active": active,
        "compilers": all.iter().map(|p| profile_json(state, p, tools)).collect::<Vec<_>>(),
        "chosen_by_game": state.settings.compile.compiler_by_game,
        "variables": variables(),
        "step_kinds": CompilerStepKind::ALL.iter().map(|k| k.key()).collect::<Vec<_>>(),
    })))
}

fn get_compiler(state: &mut AppState, a: &Args<'_>) -> R<ToolResult> {
    let key = a.opt_str("compiler", MAX_KEY)?.map(str::to_owned);
    let tools = hp_ui_mapping::compiler_tools(state);
    let tools = tools.as_ref();
    let p = match key {
        Some(k) => find(state, &k, tools)?,
        None => {
            let id = state.settings.compile.active_compiler_id().to_owned();
            find(state, &id, tools).unwrap_or_else(|_| CompilerProfile::builtin())
        }
    };
    let mut out = profile_json(state, &p, tools);
    match hp_ui_mapping::compiler_commands(state, &p, tools) {
        Ok(lines) => out["commands"] = json!(lines),
        Err(e) => out["commands_error"] = json!(e),
    }
    if p.is_builtin() {
        out["note"] =
            json!("Hammer Pro's compiler: get_compilers lists its steps and flags, set_compile_options sets them.");
    }
    Ok(ToolResult::json(out))
}

fn value_json(v: ValveValue) -> Value {
    match v {
        ValveValue::Int { min, max } => json!({ "takes": "integer", "min": min, "max": max }),
        ValveValue::Float { min, max } => json!({ "takes": "number", "min": min, "max": max }),
        ValveValue::Numbers(n) => json!({ "takes": "numbers", "count": n }),
        other => json!({ "takes": other.key() }),
    }
}

fn get_compiler_flags(_state: &mut AppState, a: &Args<'_>) -> R<ToolResult> {
    let step = a.opt_enum("step", &["bsp", "vis", "rad"])?;
    let lang = if a.opt_enum("language", &["en", "fr"])?.as_deref() == Some("fr") { Lang::Fr } else { Lang::En };
    let mut steps = Map::new();
    for (key, tool) in [("bsp", ValveTool::Vbsp), ("vis", ValveTool::Vvis), ("rad", ValveTool::Vrad)] {
        if step.as_deref().is_some_and(|s| s != key) {
            continue;
        }
        let flags: Vec<Value> = valve_flags(tool)
            .map(|f| {
                let mut v = json!({
                    "flag": f.flag,
                    "name": f.name(),
                    "what": hp_ui_mapping::valve_flag_text(f, lang),
                    "group": f.group.key(),
                    "group_title": hp_ui_mapping::valve_group_text(f.group, lang),
                    "default": (!f.default.is_empty()).then_some(f.default),
                    "set_by": f.managed.map(|m| hp_ui_mapping::valve_managed_text(m, lang)),
                });
                if let (Value::Object(m), Value::Object(t)) = (&mut v, value_json(f.value)) {
                    m.extend(t);
                }
                v
            })
            .collect();
        steps.insert(key.to_owned(), json!({ "tool": tool.key(), "flags": flags }));
    }
    Ok(ToolResult::json(json!({
        "steps": steps,
        "note": "Set them in steps[].options of create_compiler or set_compiler: flag name to true or its value.",
    })))
}

fn create_compiler(state: &mut AppState, a: &Args<'_>) -> R<ToolResult> {
    let confirmed = a.opt_bool("confirm")?;
    let select = a.opt_bool("select")?;
    let mode_flags = a.opt_bool("mode_flags")?;
    let name = clean_name(a.str("name", 256)?)?;
    if state.settings.compile.compilers.len() >= MAX_COMPILERS {
        return Err(format!("at most {MAX_COMPILERS} compilers (delete_compiler)"));
    }
    let from = a.opt_str("from", MAX_KEY)?.map(str::to_owned);
    let steps = a.get("steps").map(parse_steps).transpose()?;
    let tools = hp_ui_mapping::compiler_tools(state);
    let tools = tools.as_ref();
    let mut p = match &from {
        Some(key) => {
            let src = find(state, key, tools)?;
            if src.is_builtin() { CompilerProfile::blank(&name) } else { src }
        }
        None => CompilerProfile::blank(&name),
    };
    if (from.as_deref().is_some_and(|k| !compilers::is_builtin_id(k)) || steps.is_some()) && confirmed != Some(true) {
        return Err(needs_confirm("compilers"));
    }
    if let Some(s) = steps {
        p.steps = s;
    }
    if let Some(m) = mode_flags {
        p.mode_flags = m;
    }
    let all = hp_ui_mapping::compiler_list(state, tools);
    p.id = compilers::new_user_id(&state.settings.compile.compilers);
    p.name = compilers::unique_name(&all, &name);
    let id = p.id.clone();
    state.settings.compile.compilers.push(p.clone());
    if select == Some(true) {
        state.settings.compile.set_active_compiler(&id);
    }
    state.settings_dirty = true;
    Ok(ToolResult::json(json!({ "created": profile_json(state, &p, tools) })))
}

fn set_compiler(state: &mut AppState, a: &Args<'_>) -> R<ToolResult> {
    let confirmed = a.opt_bool("confirm")?;
    let restore = a.opt_bool("restore")?;
    let key = a.str("compiler", MAX_KEY)?.to_owned();
    let name = a.opt_str("name", 256)?.map(clean_name).transpose()?;
    let steps = a.get("steps").map(parse_steps).transpose()?;
    let mode_flags = a.opt_bool("mode_flags")?;
    if name.is_none() && steps.is_none() && mode_flags.is_none() && restore != Some(true) {
        return Err("give name, mode_flags, steps or restore".to_owned());
    }
    if steps.is_some() && confirmed != Some(true) {
        return Err(needs_confirm("compiler steps"));
    }
    let tools = hp_ui_mapping::compiler_tools(state);
    let tools = tools.as_ref();
    let mut p = find(state, &key, tools)?;
    if p.is_builtin() {
        return Err("Hammer Pro's own compiler is set with set_compile_options".to_owned());
    }
    if restore == Some(true) {
        if !p.is_detected() {
            return Err("restore is for detected compilers (delete_compiler removes a compiler of the user)".to_owned());
        }
        state.settings.compile.compilers.retain(|q| !q.id.eq_ignore_ascii_case(&p.id));
        state.settings_dirty = true;
        let p = find(state, &key, tools)?;
        return Ok(ToolResult::json(profile_json(state, &p, tools)));
    }
    if let Some(n) = name {
        if !p.is_user() {
            return Err("detected compilers keep the name of their folder".to_owned());
        }
        let all = hp_ui_mapping::compiler_list(state, tools);
        if all.iter().any(|q| q.id != p.id && q.name.trim().eq_ignore_ascii_case(&n)) {
            return Err(format!("another compiler is already named \"{n}\""));
        }
        p.name = n;
    }
    if let Some(s) = steps {
        p.steps = s;
    }
    if let Some(m) = mode_flags {
        p.mode_flags = m;
    }
    if p.is_detected() && state.settings.compile.compilers.len() >= MAX_COMPILERS {
        return Err(format!("at most {MAX_COMPILERS} compilers and changed detected ones"));
    }
    store(state, p.clone());
    Ok(ToolResult::json(profile_json(state, &p, tools)))
}

fn delete_compiler(state: &mut AppState, a: &Args<'_>) -> R<ToolResult> {
    let key = a.str("compiler", MAX_KEY)?.to_owned();
    let tools = hp_ui_mapping::compiler_tools(state);
    let p = find(state, &key, tools.as_ref())?;
    if !p.is_user() {
        return Err(format!(
            "{} cannot be deleted: only the user's compilers can (set_compiler restore forgets the changes of a detected one)",
            hp_ui_mapping::compiler_name(&p)
        ));
    }
    state.settings.compile.remove_compiler(&p.id);
    state.settings_dirty = true;
    Ok(ToolResult::json(json!({ "deleted": p.id, "active": state.settings.compile.active_compiler_id() })))
}

fn select_compiler(state: &mut AppState, a: &Args<'_>) -> R<ToolResult> {
    let key = a.str("compiler", MAX_KEY)?.to_owned();
    let game = a.opt_str("game", MAX_KEY)?.map(|g| g.trim().to_owned()).filter(|g| !g.is_empty());
    let tools = hp_ui_mapping::compiler_tools(state);
    let tools = tools.as_ref();
    let p = if compilers::is_builtin_id(&key) { CompilerProfile::builtin() } else { find(state, &key, tools)? };
    let s = &mut state.settings.compile;
    match &game {
        Some(g) => {
            let saved = s.target.clone();
            s.target = Some(g.clone());
            s.set_active_compiler(&p.id);
            s.target = saved;
        }
        None => s.set_active_compiler(&p.id),
    }
    state.settings_dirty = true;
    let problems = hp_ui_mapping::compiler_problems(&p, tools);
    Ok(ToolResult::json(json!({
        "game": game.map_or_else(|| state.settings.compile.game_key(), |g| g.to_ascii_lowercase()),
        "compiler": p.id,
        "name": hp_ui_mapping::compiler_name(&p),
        "problems": problems,
    })))
}

fn reset_compilers(state: &mut AppState, a: &Args<'_>) -> R<ToolResult> {
    if a.opt_bool("confirm")? != Some(true) {
        return Err("only when the user asked (confirm: true)".to_owned());
    }
    let removed = state.settings.compile.compilers.len();
    state.settings.compile.reset_compilers();
    state.settings_dirty = true;
    Ok(ToolResult::json(json!({ "removed": removed, "active": compilers::BUILTIN_COMPILER })))
}

pub(super) fn compiler_arg(state: &AppState, key: &str) -> R<String> {
    if compilers::is_builtin_id(key) {
        return Ok(compilers::BUILTIN_COMPILER.to_owned());
    }
    let tools = hp_ui_mapping::compiler_tools(state);
    Ok(find(state, key, tools.as_ref())?.id)
}

#[cfg(test)]
mod tests {
    use super::*;

    fn call(state: &mut AppState, name: &str, args: Value) -> ToolResult {
        match super::super::tools::run(state, name, &args) {
            super::super::tools::Outcome::Done(r) => r,
            other => panic!("{name}: {other:?}"),
        }
    }

    fn ok(state: &mut AppState, name: &str, args: Value) -> Value {
        let r = call(state, name, args);
        assert!(!r.is_error, "{name}: {}", r.first_text());
        r.structured.unwrap_or_default()
    }

    #[test]
    fn compilers_through_mcp() {
        let _lang = hp_app::testing::lang_lock();
        hp_ui_mapping::register_translations();
        hp_app::i18n::set_lang(Lang::En);
        let mut state = hp_app::testing::test_state();
        let list = ok(&mut state, "list_compilers", json!({}));
        assert_eq!(list["active"], "builtin");
        assert_eq!(list["compilers"][0]["kind"], "builtin");
        assert!(list["variables"]["$gamedir"].is_string());

        let blank = ok(&mut state, "create_compiler", json!({ "name": "Mine", "select": true }));
        assert_eq!(blank["created"]["id"], "user:1");
        assert_eq!(blank["created"]["steps"].as_array().map(Vec::len), Some(3));
        assert!(blank["created"]["problems"].as_array().is_some_and(|p| !p.is_empty()), "no programs yet");
        assert_eq!(state.settings.compile.active_compiler_id(), "user:1");

        let steps = json!([
            { "kind": "bsp", "exe": "C:/Program Files/Tools/vbsp.exe", "options": { "nodetail": true, "micro": 2, "block": [1, 2] } },
            { "kind": "rad", "exe": "C:/Program Files/Tools/vrad.exe", "options": { "bounce": 8, "StaticPropLighting": true, "lights": "$path/my lights.rad", "fast": false }, "extra": "-noskyboxrecurse" },
            { "kind": "custom", "exe": "cmd.exe", "args": "/C echo $file", "workdir": "$path", "continue_on_error": true },
        ]);
        let refused = call(&mut state, "set_compiler", json!({ "compiler": "mine", "steps": steps.clone() }));
        assert!(refused.is_error && refused.first_text().contains("confirm"));
        let set = ok(
            &mut state,
            "set_compiler",
            json!({ "compiler": "mine", "steps": steps, "confirm": true, "mode_flags": false }),
        );
        assert_eq!(set["steps"][2]["workdir"], "$path");
        assert_eq!(set["mode_flags"], false);
        assert_eq!(set["steps"][1]["options"]["bounce"], "8");
        assert_eq!(set["steps"][1]["options"]["StaticPropLighting"], true);
        assert!(set["steps"][1]["options"].get("fast").is_none(), "false leaves an option out");
        assert_eq!(set["steps"][1]["extra"], "-noskyboxrecurse");
        assert!(set["steps"][2].get("options").is_none());
        assert!(set["problems"].to_string().contains("vbsp.exe"), "{set}");
        let p = &state.settings.compile.compilers[0];
        assert_eq!(p.steps[0].options.get("-block").map(String::as_str), Some("1 2"));
        assert_eq!(p.steps[1].options.get("-lights").map(String::as_str), Some("$path/my lights.rad"));
        assert!(p.steps[2].continue_on_error);

        for bad in [
            json!([{ "kind": "nope" }]),
            json!([{ "kind": "rad", "options": { "nonsense": true } }]),
            json!([{ "kind": "rad", "options": { "bounce": 5000 } }]),
            json!([{ "kind": "rad", "options": { "bounce": true } }]),
            json!([{ "kind": "copy", "options": { "bounce": 2 } }]),
            json!([{ "kind": "custom", "extra": "-x" }]),
            json!([{ "kind": "vis", "options": { "trace": [1] } }]),
        ] {
            let r = call(&mut state, "set_compiler", json!({ "compiler": "mine", "steps": bad, "confirm": true }));
            assert!(r.is_error, "{bad}");
        }
        assert!(call(&mut state, "set_compiler", json!({ "compiler": "builtin", "name": "x" })).is_error);
        assert!(call(&mut state, "create_compiler", json!({ "name": "builtin" })).is_error);
        assert!(call(&mut state, "set_compiler", json!({ "compiler": "mine" })).is_error);
        assert!(call(&mut state, "set_compiler", json!({ "compiler": "mine", "restore": true })).is_error);

        let got = ok(&mut state, "get_compiler", json!({}));
        assert_eq!(got["id"], "user:1");
        assert!(got.get("commands").is_some() || got.get("commands_error").is_some());

        let flags = ok(&mut state, "get_compiler_flags", json!({ "step": "rad" }));
        let rad = flags["steps"]["rad"]["flags"].as_array().expect("rad flags");
        let bounce = rad.iter().find(|f| f["name"] == "bounce").expect("bounce");
        assert_eq!((bounce["takes"].as_str(), bounce["max"].as_i64()), (Some("integer"), Some(1000)));
        assert!(bounce["what"].as_str().is_some_and(|w| w.contains("bounces")));
        assert!(rad.iter().any(|f| f["name"] == "final" && f["set_by"].is_string()));
        assert!(flags["steps"].get("bsp").is_none());
        let fr = ok(&mut state, "get_compiler_flags", json!({ "language": "fr" }));
        assert_eq!(fr["steps"].as_object().map(Map::len), Some(3));
        assert!(fr["steps"]["vis"]["flags"][0]["what"].as_str().is_some_and(|w| w.contains("visibilité")));

        let copy = call(&mut state, "create_compiler", json!({ "name": "Copy", "from": "user:1" }));
        assert!(copy.is_error, "copies need confirm");
        let copy = ok(&mut state, "create_compiler", json!({ "name": "Mine", "from": "user:1", "confirm": true }));
        assert_eq!(copy["created"]["name"], "Mine (2)");
        assert!(call(&mut state, "set_compiler", json!({ "compiler": "user:2", "name": "MINE" })).is_error);

        ok(&mut state, "select_compiler", json!({ "compiler": "user:2", "game": "cstrike" }));
        assert_eq!(state.settings.compile.compiler_by_game.get("cstrike").map(String::as_str), Some("user:2"));
        assert_eq!(state.settings.compile.active_compiler_id(), "user:1");
        ok(&mut state, "select_compiler", json!({ "compiler": "builtin" }));
        assert!(state.settings.compile.uses_builtin_compiler());
        assert!(call(&mut state, "select_compiler", json!({ "compiler": "nothing" })).is_error);

        let del = ok(&mut state, "delete_compiler", json!({ "compiler": "user:2" }));
        assert_eq!(del["deleted"], "user:2");
        assert!(!state.settings.compile.compiler_by_game.contains_key("cstrike"));
        assert!(call(&mut state, "delete_compiler", json!({ "compiler": "builtin" })).is_error);
        assert!(call(&mut state, "reset_compilers", json!({ "confirm": false })).is_error);
        ok(&mut state, "reset_compilers", json!({ "confirm": true }));
        assert!(state.settings.compile.compilers.is_empty());
        assert_eq!(compiler_arg(&state, "BUILTIN").as_deref(), Ok("builtin"));
        assert!(compiler_arg(&state, "user:5").is_err());
    }
}
