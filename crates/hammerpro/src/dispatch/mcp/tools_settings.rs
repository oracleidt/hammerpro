use hp_app::AppState;
use hp_app::actions::{Action, format_shortcut, parse_shortcut};
use hp_app::i18n::{Lang, tr_in};
use hp_app::settings::Settings;
use hp_mcp::protocol::ToolResult;
use hp_mcp::tools::{ToolKind, ToolSpec};
use serde_json::{Map, Value, json};

use super::automation::UiOp;
use super::schema::{boolean, integer, object, string};
use super::tools::{Domain, Outcome, R, args};

pub(crate) const DOMAIN: Domain = Domain { category: "settings", specs: SPECS, run };

const MAX_PATH: usize = 128;
const MAX_STRING: usize = 1024;
const MAX_NUMBER: f64 = 1.0e7;

pub(crate) const RANGES: &[(&str, f64, f64)] = &[
    ("view.fov_deg", 40.0, 120.0),
    ("view.camera_speed", 50.0, 20_000.0),
    ("view.camera_fast_multiplier", 1.0, 20.0),
    ("view.mouse_sensitivity", 0.02, 1.0),
    ("view.far_plane", 2048.0, 262_144.0),
    ("view.grid_highlight_every", 2.0, 64.0),
    ("edit.grid_size", hp_app::settings::GRID_MIN, hp_app::settings::GRID_MAX),
    ("edit.undo_levels", 10.0, 10_000.0),
    ("edit.autosave_minutes", 0.0, 120.0),
    ("edit.autosave_keep", 1.0, 100.0),
    ("edit.default_texture_scale", 0.01, 16.0),
    ("ui.ui_scale", 0.75, 2.0),
    ("ui.split_x", hp_ui_viewports::layout::SPLIT_MIN as f64, hp_ui_viewports::layout::SPLIT_MAX as f64),
    ("ui.split_y", hp_ui_viewports::layout::SPLIT_MIN as f64, hp_ui_viewports::layout::SPLIT_MAX as f64),
    ("ui.quadrants", 0.0, 4.0),
    ("ui.thumbnail_size", 32.0, 512.0),
    ("render.near_plane", 0.5, 16.0),
    ("render.model_distance", 0.0, 65_536.0),
    ("render.cull_radius", 256.0, 65_536.0),
    ("render.max_fps", 0.0, 1000.0),
    ("render.light_shadows", 0.0, hp_render::MAX_SHADOWED_LIGHTS as f64),
    ("render.unfocused_fps", 0.0, 1000.0),
    ("render.vram_budget_mb", 0.0, hp_app::settings::MAX_VRAM_BUDGET_MB as f64),
    ("selection.rotation_snap_deg", 1.0, 90.0),
    ("selection.random_yaw_min", -360.0, 360.0),
    ("selection.random_yaw_max", -360.0, 720.0),
    ("selection.random_yaw_snap", 0.0, 180.0),
    ("selection.random_tilt_deg", 0.0, hp_editor::MAX_JITTER_DEG),
    ("grid.per_view.perspective.size", hp_app::settings::GRID_MIN, hp_app::settings::GRID_MAX),
    ("grid.per_view.top.size", hp_app::settings::GRID_MIN, hp_app::settings::GRID_MAX),
    ("grid.per_view.front.size", hp_app::settings::GRID_MIN, hp_app::settings::GRID_MAX),
    ("grid.per_view.side.size", hp_app::settings::GRID_MIN, hp_app::settings::GRID_MAX),
    ("grid.per_view.perspective.origin", -hp_app::settings::GRID_COORD_LIMIT, hp_app::settings::GRID_COORD_LIMIT),
    ("grid.per_view.top.origin", -hp_app::settings::GRID_COORD_LIMIT, hp_app::settings::GRID_COORD_LIMIT),
    ("grid.per_view.front.origin", -hp_app::settings::GRID_COORD_LIMIT, hp_app::settings::GRID_COORD_LIMIT),
    ("grid.per_view.side.origin", -hp_app::settings::GRID_COORD_LIMIT, hp_app::settings::GRID_COORD_LIMIT),
    ("grid.per_view.perspective.major_every", 0.0, hp_app::settings::MAX_MAJOR_EVERY as f64),
    ("grid.per_view.top.major_every", 0.0, hp_app::settings::MAX_MAJOR_EVERY as f64),
    ("grid.per_view.front.major_every", 0.0, hp_app::settings::MAX_MAJOR_EVERY as f64),
    ("grid.per_view.side.major_every", 0.0, hp_app::settings::MAX_MAJOR_EVERY as f64),
    ("lab.model_lod.levels", 3.0, 6.0),
    ("lab.model_lod.reduction", 0.2, 0.9),
];

const READ_ONLY: &[&str] = &[
    "recent_files",
    "language_chosen",
    "keybindings",
    "game",
    "mcp",
    "compile.profiles",
    "compile.launch_game",
    "compile.launch_via_steam",
    "content.game_session",
    "content.open_folder",
];

const READ_ONLY_WORDS: &[&str] = &[
    "path", "paths", "dir", "dirs", "exe", "folder", "folders", "file", "files", "args", "program", "library", "root",
    "fgd", "fgds", "command", "url",
];

pub(crate) fn read_only(path: &str) -> Option<&'static str> {
    const COMPILE_TOOL: &[&str] = &[
        "compile.launch_game",
        "compile.launch_via_steam",
        "compile.maps_dir_override",
        "compile.extra_bsp_args",
        "compile.extra_rad_args",
        "compile.bsp_flag_choices",
        "compile.rad_flag_choices",
    ];
    if COMPILE_TOOL.contains(&path) || COMPILE_TOOL.iter().any(|p| path.starts_with(&format!("{p}."))) {
        return Some(
            "change it with set_compile_options (launch_game, launch_via_steam, maps_dir_override, bsp_options, \
             rad_options), checked there",
        );
    }
    const COMPILERS_TOOL: &[&str] = &["compile.compilers", "compile.compiler_by_game"];
    if COMPILERS_TOOL.iter().any(|p| path == *p || path.starts_with(&format!("{p}."))) {
        return Some(
            "change it with create_compiler, set_compiler, delete_compiler, select_compiler and reset_compilers (programs need confirm there)",
        );
    }
    if READ_ONLY.iter().any(|p| path == *p || path.starts_with(&format!("{p}."))) {
        return Some(if path.starts_with("keybindings") {
            "change key bindings with set_keybinding"
        } else {
            "MCP cannot change this setting: ask the user to change it in Settings"
        });
    }
    let named = path.split('.').any(|seg| seg.split('_').any(|w| READ_ONLY_WORDS.contains(&w)));
    named.then_some("files, folders, programs and command lines are only changed by the user in Settings")
}

const SPECS: &[ToolSpec] = &[
    ToolSpec {
        name: "get_settings",
        title: "Read the settings",
        description: "Read Hammer Pro's settings: all of them, a table (\"view\", \"edit\", \"ui\", \"render\"...) or \
            one setting by dotted path (\"view.fov_deg\", \"ui.theme\"). Lists the paths MCP cannot change. \
            The \"discord\" table is Discord Rich Presence (enabled, show_map, show_elapsed, show_tool, \
            application_id). The \"update\" table is the auto update (check_on_startup, show_dialog, skipped_version; \
            see get_update_status).",
        kind: ToolKind::Read,
        schema: || object(json!({ "path": string("Dotted path; empty: everything.") }), &[]),
    },
    ToolSpec {
        name: "set_setting",
        title: "Change a setting",
        description: "Change one setting by dotted path (from get_settings) to a value of the same type, e.g. \
            path \"view.fov_deg\" value 90, \"ui.theme\" \"Dark\", \"edit.snap_to_grid\" false. Applied at once \
            and saved. File paths, programs and game folders are read only.",
        kind: ToolKind::Editor,
        schema: || {
            object(
                json!({
                    "path": string("Dotted path of the setting."),
                    "value": { "description": "New value (number, boolean, string, or array like the current one)." },
                }),
                &["path", "value"],
            )
        },
    },
    ToolSpec {
        name: "list_keybindings",
        title: "List the key bindings",
        description: "The keyboard shortcut of every action (current, default of the preset, customized), the \
            shortcut preset and the conflicts.",
        kind: ToolKind::Read,
        schema: || {
            object(
                json!({
                    "query": string("Only actions whose id or label contains this text."),
                    "customized_only": boolean("Only the shortcuts the user changed."),
                    "limit": integer("Most actions (default 500).", 1, 1000),
                }),
                &[],
            )
        },
    },
    ToolSpec {
        name: "set_keybinding",
        title: "Change a key binding",
        description: "Bind an action to a shortcut like \"Ctrl+Shift+K\", remove its shortcut (\"\"), or give it \
            back its default (reset: true). Answers with the actions that share the shortcut.",
        kind: ToolKind::Editor,
        schema: || {
            object(
                json!({
                    "action": string("Action id (list_actions)."),
                    "shortcut": string("Ctrl+, Shift+, Alt+ and a key (egui names: A, F5, Delete...); \"\" removes it."),
                    "reset": boolean("Give the action its default shortcut back."),
                }),
                &["action"],
            )
        },
    },
];

fn run(state: &mut AppState, name: &str, v: &Value) -> Option<Outcome> {
    let result = match name {
        "get_settings" => get_settings(state, v).map(Outcome::Done),
        "set_setting" => set_setting(state, v),
        "list_keybindings" => list_keybindings(state, v).map(Outcome::Done),
        "set_keybinding" => set_keybinding(state, v).map(Outcome::Done),
        _ => return None,
    };
    Some(result.unwrap_or_else(|e| Outcome::Done(ToolResult::error(format!("{name}: {e}")))))
}

pub(crate) fn settings_json(s: &Settings) -> Value {
    let mut v = serde_json::to_value(s).unwrap_or_else(|_| json!({}));
    if let Value::Object(m) = &mut v {
        macro_rules! tables {
            ($($t:ident),* $(,)?) => {
                $( m.insert(stringify!($t).to_owned(), serde_json::to_value(&s.$t).unwrap_or(Value::Null)); )*
            };
        }
        tables!(
            selection, brush, workflow, instances, visibility, helpers, mesh, entities, render, lighting, placement,
            startup, launch, content, health, lab, mcp, grid, discord, scan, update, sniffer,
        );
    }
    v
}

fn check_path(path: &str) -> R<Vec<&str>> {
    let path = path.trim();
    if path.len() > MAX_PATH {
        return Err(format!("path is too long (at most {MAX_PATH} characters)"));
    }
    if path.is_empty() {
        return Ok(Vec::new());
    }
    let segs: Vec<&str> = path.split('.').collect();
    if segs.iter().any(|s| s.is_empty() || !s.chars().all(|c| c.is_ascii_alphanumeric() || c == '_')) {
        return Err(format!("\"{path}\" is not a setting path (names and indexes separated by dots)"));
    }
    Ok(segs)
}

fn lookup<'a>(root: &'a Value, segs: &[&str]) -> Option<&'a Value> {
    segs.iter().try_fold(root, |v, seg| match v {
        Value::Object(m) => m.get(*seg),
        Value::Array(a) => seg.parse::<usize>().ok().and_then(|i| a.get(i)),
        _ => None,
    })
}

fn lookup_mut<'a>(root: &'a mut Value, segs: &[&str]) -> Option<&'a mut Value> {
    segs.iter().try_fold(root, |v, seg| match v {
        Value::Object(m) => m.get_mut(*seg),
        Value::Array(a) => seg.parse::<usize>().ok().and_then(|i| a.get_mut(i)),
        _ => None,
    })
}

fn children(v: Option<&Value>) -> String {
    match v {
        Some(Value::Object(m)) => m.keys().take(60).cloned().collect::<Vec<_>>().join(", "),
        _ => String::new(),
    }
}

fn read_only_paths(root: &Value) -> Vec<String> {
    let mut out = Vec::new();
    fn walk(v: &Value, path: &str, out: &mut Vec<String>) {
        if !path.is_empty() && read_only(path).is_some() {
            out.push(path.to_owned());
            return;
        }
        if let Value::Object(m) = v {
            for (k, child) in m {
                let p = if path.is_empty() { k.clone() } else { format!("{path}.{k}") };
                walk(child, &p, out);
            }
        }
    }
    walk(root, "", &mut out);
    out
}

fn get_settings(state: &AppState, v: &Value) -> R<ToolResult> {
    let a = args(v, &["path"])?;
    let path = a.opt_str("path", MAX_PATH)?.unwrap_or("").trim();
    let segs = check_path(path)?;
    let all = settings_json(&state.settings);
    let Some(value) = lookup(&all, &segs) else {
        let parent = lookup(&all, &segs[..segs.len().saturating_sub(1)]);
        return Err(format!("no setting \"{path}\" (names here: {})", children(parent)));
    };
    let mut out = json!({ "path": path, "value": value });
    if segs.is_empty() {
        out["read_only"] = json!(read_only_paths(&all));
    } else if let Some(why) = read_only(path) {
        out["read_only"] = json!(why);
    }
    Ok(ToolResult::json(out))
}

fn same_kind(old: &Value, new: &Value) -> bool {
    matches!(
        (old, new),
        (Value::Null, _)
            | (Value::Bool(_), Value::Bool(_))
            | (Value::Number(_), Value::Number(_))
            | (Value::String(_), Value::String(_))
            | (Value::Array(_), Value::Array(_))
            | (Value::Object(_), Value::Object(_))
    ) || (new.is_null() && !old.is_object())
}

fn check_value(path: &str, v: &Value, range: Option<(f64, f64)>, depth: usize) -> R<()> {
    if depth > 8 {
        return Err("the value is nested too deeply".to_owned());
    }
    match v {
        Value::Number(n) => {
            let x = n.as_f64().filter(|x| x.is_finite()).ok_or_else(|| format!("{path} must be a finite number"))?;
            let (min, max) = range.unwrap_or((-MAX_NUMBER, MAX_NUMBER));
            if x < min || x > max {
                return Err(format!("{path} must be from {min} to {max} (got {x})"));
            }
            Ok(())
        }
        Value::String(s) if s.len() > MAX_STRING => Err(format!("{path} is too long (at most {MAX_STRING} bytes)")),
        Value::Array(a) if a.len() > 256 => Err(format!("{path} has too many items (at most 256)")),
        Value::Array(a) => a.iter().try_for_each(|x| check_value(path, x, range, depth + 1)),
        Value::Object(m) if m.len() > 256 => Err(format!("{path} has too many entries")),
        Value::Object(m) => m.values().try_for_each(|x| check_value(path, x, range, depth + 1)),
        _ => Ok(()),
    }
}

fn values_match(a: &Value, b: &Value) -> bool {
    match (a, b) {
        (Value::Number(x), Value::Number(y)) => match (x.as_f64(), y.as_f64()) {
            (Some(x), Some(y)) => (x - y).abs() <= 1e-6 * x.abs().max(y.abs()).max(1.0),
            _ => x == y,
        },
        (Value::Array(x), Value::Array(y)) => x.len() == y.len() && x.iter().zip(y).all(|(x, y)| values_match(x, y)),
        (Value::Object(x), Value::Object(y)) => {
            x.len() == y.len() && x.iter().all(|(k, v)| y.get(k).is_some_and(|w| values_match(v, w)))
        }
        _ => a == b,
    }
}

fn set_setting(state: &AppState, v: &Value) -> R<Outcome> {
    let a = args(v, &["path", "value"])?;
    let path = a.str("path", MAX_PATH)?.trim();
    let value = a.map.get("value").ok_or("value is required")?;
    let segs = check_path(path)?;
    if segs.is_empty() {
        return Err("path is required (e.g. \"view.fov_deg\")".to_owned());
    }
    if let Some(why) = read_only(path) {
        return Err(format!("{path} is read only: {why}"));
    }
    let mut all = settings_json(&state.settings);
    let Some(slot) = lookup_mut(&mut all, &segs) else {
        let parent = lookup(&all, &segs[..segs.len() - 1]);
        return Err(format!("no setting \"{path}\" (names here: {})", children(parent)));
    };
    if slot.is_object() {
        return Err(format!("{path} is a table: set one of its settings ({})", children(Some(slot))));
    }
    if !same_kind(slot, value) {
        return Err(format!("{path} takes a value like {slot} (got {value})"));
    }
    let range = RANGES.iter().find(|(p, _, _)| *p == path).map(|(_, min, max)| (*min, *max));
    check_value(path, value, range, 0)?;
    *slot = value.clone();
    let new: Settings = serde_json::from_value(all).map_err(|e| {
        match serde_json::from_value::<Settings>(settings_json(&state.settings)) {
            Ok(_) => format!("invalid value for {path}: {e}"),
            Err(other) => {
                format!("the settings hold a value MCP cannot read back ({other}); change it in Settings first")
            }
        }
    })?;
    let back = settings_json(&new);
    let stored = lookup(&back, &segs).cloned().unwrap_or(Value::Null);
    if !values_match(&stored, value) {
        return Err(format!("{path} cannot hold {value} (it would become {stored})"));
    }
    Ok(Outcome::Ui(UiOp::Settings { settings: Box::new(new), path: path.to_owned(), value: stored }))
}

pub(crate) fn script_set(state: &mut AppState, ctx: &egui::Context, path: &str, value: &str) -> Result<(), String> {
    let value: Value = serde_json::from_str(value).unwrap_or_else(|_| Value::String(value.to_owned()));
    match set_setting(state, &json!({ "path": path, "value": value }))? {
        Outcome::Ui(UiOp::Settings { settings, path, value }) => {
            let r = apply(state, ctx, *settings, &path, value);
            if r.is_error { Err(r.first_text().to_owned()) } else { Ok(()) }
        }
        _ => Err("not a setting change".to_owned()),
    }
}

pub(crate) fn apply(state: &mut AppState, ctx: &egui::Context, new: Settings, path: &str, value: Value) -> ToolResult {
    let old = std::mem::replace(&mut state.settings, new);
    if state.settings.language != old.language {
        state.settings.language_chosen = true;
        hp_app::i18n::set_lang(state.settings.language);
    }
    let s = &state.settings;
    if s.ui.theme != old.ui.theme || s.ui.ui_scale != old.ui.ui_scale || s.ui.accent_color != old.ui.accent_color {
        hp_app::theme::set_accent(s.ui.accent_color);
        hp_app::theme::apply(ctx, s.ui.theme, s.ui.ui_scale);
    }
    if s.workflow.keymap_preset != old.workflow.keymap_preset {
        state.keys.set_preset(state.settings.workflow.keymap_preset);
    }
    state.apply_selection_undo_setting();
    state.settings_dirty = true;
    ToolResult::json(json!({ "path": path, "value": value, "saved": true }))
}

fn list_keybindings(state: &AppState, v: &Value) -> R<ToolResult> {
    let a = args(v, &["query", "customized_only", "limit"])?;
    let query = a.opt_str("query", 128)?.map(str::to_lowercase);
    let customized_only = a.opt_bool("customized_only")?.unwrap_or(false);
    let limit = a.opt_u64("limit", 1, 1000)?.unwrap_or(500) as usize;
    let keys = &state.keys;
    let mut by_shortcut: Map<String, Value> = Map::new();
    for act in Action::ALL {
        if let Some(sc) = keys.shortcut(*act) {
            let entry = by_shortcut.entry(format_shortcut(&sc)).or_insert_with(|| json!([]));
            if let Value::Array(list) = entry {
                list.push(json!(act.id()));
            }
        }
    }
    let conflicts: Map<String, Value> =
        by_shortcut.into_iter().filter(|(_, ids)| ids.as_array().is_some_and(|a| a.len() > 1)).collect();
    let fmt = |sc: Option<egui::KeyboardShortcut>| sc.map(|sc| format_shortcut(&sc));
    let list: Vec<Value> = Action::ALL
        .iter()
        .copied()
        .filter(|act| !customized_only || keys.is_customized(*act))
        .filter(|act| {
            query
                .as_deref()
                .is_none_or(|q| act.id().contains(q) || tr_in(&act.label_key(), Lang::En).to_lowercase().contains(q))
        })
        .take(limit)
        .map(|act| {
            json!({
                "action": act.id(),
                "label": tr_in(&act.label_key(), Lang::En),
                "shortcut": fmt(keys.shortcut(act)),
                "default": fmt(keys.preset_shortcut(act)),
                "customized": keys.is_customized(act),
                "aliases": keys.aliases(act).iter().map(format_shortcut).collect::<Vec<_>>(),
            })
        })
        .collect();
    Ok(ToolResult::json(json!({
        "preset": format!("{:?}", keys.preset()),
        "bindings": list,
        "conflicts": conflicts,
    })))
}

fn set_keybinding(state: &mut AppState, v: &Value) -> R<ToolResult> {
    let a = args(v, &["action", "shortcut", "reset"])?;
    let id = a.str("action", 64)?.trim().to_ascii_lowercase();
    let action = Action::from_id(&id).filter(|a| a.is_ready()).ok_or_else(|| format!("unknown action \"{id}\""))?;
    let reset = a.opt_bool("reset")?.unwrap_or(false);
    let text = a.opt_str("shortcut", 64)?;
    let shortcut = match (reset, text) {
        (true, Some(_)) => return Err("give shortcut or reset, not both".to_owned()),
        (true, None) => None,
        (false, None) => return Err("give shortcut (\"\" removes it) or reset: true".to_owned()),
        (false, Some(t)) if t.trim().is_empty() => None,
        (false, Some(t)) => Some(
            super::tools_ui::parse_combo(t)
                .map(|(k, m)| egui::KeyboardShortcut::new(hp_app::actions::normalize_modifiers(m), k))?,
        ),
    };
    let shortcut = shortcut.map(|sc| parse_shortcut(&format_shortcut(&sc)).unwrap_or(sc));
    if reset {
        state.keys.reset(action);
    } else {
        state.keys.set(action, shortcut);
    }
    state.settings.keybindings = state.keys.to_settings();
    state.settings_dirty = true;
    let now = state.keys.shortcut(action);
    let shared: Vec<&str> = now
        .map(|sc| {
            Action::ALL
                .iter()
                .filter(|o| {
                    **o != action
                        && state.keys.shortcut(**o).is_some_and(|s| format_shortcut(&s) == format_shortcut(&sc))
                })
                .map(|o| o.id())
                .collect()
        })
        .unwrap_or_default();
    Ok(ToolResult::json(json!({
        "action": action.id(),
        "shortcut": now.map(|sc| format_shortcut(&sc)),
        "customized": state.keys.is_customized(action),
        "conflicts_with": shared,
    })))
}

#[cfg(test)]
mod tests {
    use serde::de::{self, Visitor};

    use super::*;

    struct Fields(Vec<&'static str>);

    impl<'de> de::Deserializer<'de> for &mut Fields {
        type Error = de::value::Error;

        fn deserialize_any<V: Visitor<'de>>(self, _: V) -> Result<V::Value, Self::Error> {
            Err(de::Error::custom("only structs"))
        }

        fn deserialize_struct<V: Visitor<'de>>(
            self,
            _: &'static str,
            fields: &'static [&'static str],
            _: V,
        ) -> Result<V::Value, Self::Error> {
            self.0 = fields.to_vec();
            Err(de::Error::custom("done"))
        }

        serde::forward_to_deserialize_any! {
            bool i8 i16 i32 i64 i128 u8 u16 u32 u64 u128 f32 f64 char str string bytes byte_buf option unit
            unit_struct newtype_struct seq tuple tuple_struct map enum identifier ignored_any
        }
    }

    fn call(state: &mut AppState, name: &str, v: Value) -> Outcome {
        run(state, name, &v).expect("a settings tool")
    }

    fn ok(state: &mut AppState, name: &str, v: Value) -> Value {
        match call(state, name, v) {
            Outcome::Done(r) if !r.is_error => r.structured.unwrap(),
            other => panic!("{name}: {other:?}"),
        }
    }

    fn err(state: &mut AppState, name: &str, v: Value) -> String {
        match call(state, name, v) {
            Outcome::Done(r) if r.is_error => r.first_text().to_owned(),
            other => panic!("{name} should fail: {other:?}"),
        }
    }

    fn set(state: &mut AppState, path: &str, value: Value) -> Value {
        match call(state, "set_setting", json!({ "path": path, "value": value })) {
            Outcome::Ui(UiOp::Settings { settings, path, value }) => {
                let ctx = egui::Context::default();
                apply(state, &ctx, *settings, &path, value).structured.unwrap()
            }
            other => panic!("{path}: {other:?}"),
        }
    }

    #[test]
    fn every_settings_table_is_in_the_json() {
        let mut fields = Fields(Vec::new());
        let _ = <Settings as serde::Deserialize>::deserialize(&mut fields);
        assert!(fields.0.len() > 20, "{:?}", fields.0);
        let v = settings_json(&Settings::default());
        for f in &fields.0 {
            assert!(v.get(*f).is_some(), "settings_json misses the table \"{f}\": add it to `tables!`");
        }
        for (path, min, max) in RANGES {
            let segs: Vec<&str> = path.split('.').collect();
            let value = lookup(&v, &segs).unwrap_or_else(|| panic!("{path} is not a setting"));
            assert!(value.is_number() || value.is_array(), "{path}");
            assert!(min < max && read_only(path).is_none(), "{path}");
        }
    }

    #[test]
    fn read_only_settings() {
        for p in [
            "recent_files",
            "game.gmod_root",
            "game.custom_texture_dirs",
            "compile.game_exe_path",
            "compile.extra_bsp_args",
            "compile.profiles.0.steps",
            "placement.prefab_library",
            "mcp.start_with_app",
            "keybindings",
            "compile.launch_game",
            "compile.compilers",
            "compile.compilers.0.steps",
            "compile.compiler_by_game",
        ] {
            assert!(read_only(p).is_some(), "{p}");
        }
        for p in ["view.fov_deg", "ui.theme", "health.limit_profile", "edit.snap_to_grid", "compile.preset"] {
            assert!(read_only(p).is_none(), "{p}");
        }
        let all = settings_json(&Settings::default());
        let ro = read_only_paths(&all);
        assert!(ro.iter().any(|p| p == "game"));
        fn leaves(v: &Value, path: &str, out: &mut Vec<String>) {
            match v {
                Value::Object(m) => m.iter().for_each(|(k, c)| {
                    leaves(c, &if path.is_empty() { k.clone() } else { format!("{path}.{k}") }, out)
                }),
                _ => out.push(path.to_owned()),
            }
        }
        let mut all_leaves = Vec::new();
        leaves(&all, "", &mut all_leaves);
        for p in all_leaves.iter().filter(|p| p.contains("path") || p.contains("_dir") || p.contains("exe")) {
            assert!(read_only(p).is_some(), "{p}");
        }
    }

    #[test]
    fn model_lod_settings_are_reachable() {
        let _lang = hp_app::testing::lang_lock();
        let mut state = hp_app::testing::test_state();
        let lod = ok(&mut state, "get_settings", json!({ "path": "lab.model_lod" }));
        assert_eq!(lod["value"]["enabled"], true);
        assert_eq!(lod["value"]["levels"], 4);
        set(&mut state, "lab.model_lod.levels", json!(5));
        set(&mut state, "lab.model_lod.reduction", json!(0.6));
        set(&mut state, "lab.model_lod.enabled", json!(false));
        assert_eq!(state.settings.lab.model_lod.levels, 5);
        assert!((state.settings.lab.model_lod.reduction - 0.6).abs() < 1e-6);
        assert!(!state.settings.lab.model_lod.enabled);
        let e = err(&mut state, "set_setting", json!({ "path": "lab.model_lod.levels", "value": 2 }));
        assert!(e.contains("from 3 to 6"), "{e}");
        let e = err(&mut state, "set_setting", json!({ "path": "lab.model_lod.reduction", "value": 0.05 }));
        assert!(e.contains("from 0.2"), "{e}");
    }

    #[test]
    fn discord_settings_are_reachable() {
        let _lang = hp_app::testing::lang_lock();
        let mut state = hp_app::testing::test_state();
        let d = ok(&mut state, "get_settings", json!({ "path": "discord" }));
        assert_eq!(d["value"]["enabled"], true);
        assert_eq!(d["value"]["show_map"], true);
        assert_eq!(d["value"]["application_id"], "");
        set(&mut state, "discord.show_map", json!(false));
        set(&mut state, "discord.application_id", json!("1234567890123456789"));
        set(&mut state, "discord.enabled", json!(false));
        let s = &state.settings.discord;
        assert!(!s.enabled && !s.show_map && s.show_tool);
        assert_eq!(s.application_id, "1234567890123456789");
        assert!(read_only("discord.project_url").is_some());
    }

    #[test]
    fn lit_mode_settings_are_reachable() {
        let _lang = hp_app::testing::lang_lock();
        let mut state = hp_app::testing::test_state();
        let v = ok(&mut state, "get_settings", json!({ "path": "render" }));
        assert_eq!(v["value"]["lit_indirect"], true, "the bounced light of the Lit mode is on by default");
        assert_eq!(v["value"]["light_shadows"], 16);
        set(&mut state, "render.lit_indirect", json!(false));
        set(&mut state, "render.light_shadows", json!(64));
        assert!(!state.settings.render.lit_indirect);
        assert_eq!(state.settings.render.light_shadows, 64);
        assert!(read_only("render.lit_indirect").is_none() && read_only("render.light_shadows").is_none());
    }

    #[test]
    fn scan_settings_are_reachable() {
        let _lang = hp_app::testing::lang_lock();
        let mut state = hp_app::testing::test_state();
        let v = ok(&mut state, "get_settings", json!({ "path": "scan" }));
        assert_eq!(v["value"]["use_index"], true);
        assert_eq!(v["value"]["mode"], "auto");
        set(&mut state, "scan.mode", json!("sequential"));
        set(&mut state, "scan.use_index", json!(false));
        assert_eq!(state.settings.scan.mode, hp_app::settings::ScanModeSetting::Sequential);
        assert!(!state.settings.scan.use_index);
        assert!(state.settings.scan.to_config().index_file.is_none());
        assert_eq!(state.settings.scan.to_config().mode, hp_fs::ScanMode::Sequential);
        assert!(read_only("scan.mode").is_none() && read_only("scan.use_index").is_none());
    }

    #[test]
    fn update_settings_are_reachable() {
        let _lang = hp_app::testing::lang_lock();
        let mut state = hp_app::testing::test_state();
        let v = ok(&mut state, "get_settings", json!({ "path": "update" }));
        assert_eq!(v["value"]["check_on_startup"], true);
        assert_eq!(v["value"]["show_dialog"], true);
        assert_eq!(v["value"]["skipped_version"], "");
        set(&mut state, "update.check_on_startup", json!(false));
        set(&mut state, "update.show_dialog", json!(false));
        set(&mut state, "update.skipped_version", json!("v1.0.3"));
        assert!(!state.settings.update.check_on_startup);
        assert!(!state.settings.update.show_dialog);
        assert_eq!(state.settings.update.skipped_version, "v1.0.3");
        let v = ok(&mut state, "get_settings", json!({ "path": "update.skipped_version" }));
        assert_eq!(v["value"], "v1.0.3");
        assert!(read_only("update.check_on_startup").is_none());
        assert!(read_only("update.show_dialog").is_none());
        assert!(read_only("update.skipped_version").is_none());
    }

    #[test]
    fn compile_window_settings_are_reachable() {
        let _lang = hp_app::testing::lang_lock();
        let mut state = hp_app::testing::test_state();
        let v = ok(&mut state, "get_settings", json!({ "path": "compile.separate_window" }));
        assert_eq!(v["value"], true);
        set(&mut state, "compile.separate_window", json!(false));
        assert!(!state.settings.compile.separate_window);
        let place = json!({ "x": 2700, "y": 100, "width": 900, "height": 700, "monitor": "\\\\.\\DISPLAY2",
            "monitor_x": 2560, "monitor_y": 0 });
        set(&mut state, "compile.window", place.clone());
        let p = state.settings.compile.window.clone().expect("placement");
        assert_eq!((p.x, p.width, p.monitor.as_str()), (2700, 900, "\\\\.\\DISPLAY2"));
        let v = ok(&mut state, "get_settings", json!({ "path": "compile.window" }));
        assert_eq!(v["value"], place);
        let e = err(&mut state, "set_setting", json!({ "path": "compile.separate_window", "value": "yes" }));
        assert!(!e.is_empty());
    }

    #[test]
    fn window_frame_settings_are_reachable() {
        let _lang = hp_app::testing::lang_lock();
        let mut state = hp_app::testing::test_state();
        let v = ok(&mut state, "get_settings", json!({ "path": "ui.native_frames" }));
        assert_eq!(v["value"], false, "Hammer Pro's frames by default");
        set(&mut state, "ui.native_frames", json!(true));
        assert!(state.settings.ui.native_frames);
        set(&mut state, "ui.native_frames", json!(false));
        assert!(!state.settings.ui.native_frames);
        let place = json!({ "x": 100, "y": 80, "width": 640, "height": 860, "monitor": "", "monitor_x": 0,
            "monitor_y": 0 });
        set(&mut state, "ui.uv_window", place.clone());
        assert_eq!(state.settings.ui.uv_window.as_ref().map(|p| p.height), Some(860));
        set(&mut state, "ui.portals_window", place);
        assert_eq!(state.settings.ui.portals_window.as_ref().map(|p| p.x), Some(100));
        assert!(!err(&mut state, "set_setting", json!({ "path": "ui.native_frames", "value": 1 })).is_empty());
    }

    #[test]
    fn get_and_set_settings() {
        let _lang = hp_app::testing::lang_lock();
        let mut state = hp_app::testing::test_state();
        let all = ok(&mut state, "get_settings", json!({}));
        assert!(all["value"]["view"]["fov_deg"].is_number());
        assert!(all["value"]["lab"].is_object(), "default tables are listed too");
        assert!(all["read_only"].as_array().unwrap().iter().any(|p| p == "mcp"));
        let fov = ok(&mut state, "get_settings", json!({ "path": "view.fov_deg" }));
        assert_eq!(fov["value"].as_f64().unwrap() as f32, state.settings.view.fov_deg);
        assert!(err(&mut state, "get_settings", json!({ "path": "view.nope" })).contains("fov_deg"));
        assert!(err(&mut state, "get_settings", json!({ "path": "view..x" })).contains("not a setting path"));

        let r = set(&mut state, "view.fov_deg", json!(95));
        assert_eq!(r["value"], 95.0);
        assert_eq!(state.settings.view.fov_deg, 95.0);
        assert!(state.settings_dirty);
        set(&mut state, "edit.snap_to_grid", json!(false));
        assert!(!state.settings.edit.snap_to_grid);
        set(&mut state, "selection.stretch_handles", json!(false));
        assert!(!state.settings.selection.stretch_handles);
        set(&mut state, "ui.quadrants", json!([1, 2, 3, 0]));
        assert_eq!(state.settings.ui.quadrants, [1, 2, 3, 0]);
        set(&mut state, "ui.quadrants", json!([4, 2, 3, 0]));
        assert_eq!(hp_app::QuadrantView::from_index(state.settings.ui.quadrants[0]), hp_app::QuadrantView::Logical);
        let theme = settings_json(&state.settings)["ui"]["theme"].clone();
        set(&mut state, "ui.theme", theme);
        let fr = serde_json::to_value(Lang::Fr).unwrap();
        set(&mut state, "language", fr);
        assert_eq!(hp_app::i18n::lang(), Lang::Fr);
        set(&mut state, "language", serde_json::to_value(Lang::En).unwrap());
        assert_eq!(hp_app::i18n::lang(), Lang::En);

        for (path, value, needle) in [
            ("view.fov_deg", json!(500), "from 40 to 120"),
            ("view.fov_deg", json!("wide"), "takes a value like"),
            ("view.fov_deg", json!(null), "invalid value"),
            ("ui.quadrants", json!([1, 2, 3, 9]), "from 0 to 4"),
            ("ui.quadrants", json!([1, 2]), "invalid value"),
            ("ui.theme", json!("Neon"), "invalid value"),
            ("edit.undo_levels", json!(-5), "from 10"),
            ("edit.undo_levels", json!(12.5), "invalid value"),
            ("view", json!(1), "is a table"),
            ("view.zzz", json!(1), "no setting"),
            ("game.gmod_root", json!("C:/x"), "read only"),
            ("compile.game_exe_path", json!("C:/evil.exe"), "read only"),
            ("keybindings", json!({}), "set_keybinding"),
            ("edit.default_material", json!("x".repeat(5000)), "too long"),
        ] {
            let e = err(&mut state, "set_setting", json!({ "path": path, "value": value }));
            assert!(e.contains(needle), "{path} = {value}: {e}");
        }
        assert!(err(&mut state, "set_setting", json!({ "path": "view.fov_deg" })).contains("value is required"));
        assert!(err(&mut state, "set_setting", json!({ "path": "", "value": 1 })).contains("path is required"));
        assert_eq!(state.settings.view.fov_deg, 95.0, "failed calls change nothing");
        let mut broken = hp_app::testing::test_state();
        broken.settings.view.fov_deg = f32::NAN;
        let e = err(&mut broken, "set_setting", json!({ "path": "edit.snap_to_grid", "value": false }));
        assert!(e.contains("cannot read back"), "{e}");
    }

    #[test]
    fn key_bindings() {
        let _lang = hp_app::testing::lang_lock();
        let mut state = hp_app::testing::test_state();
        let all = ok(&mut state, "list_keybindings", json!({ "limit": 1000 }));
        let save = all["bindings"].as_array().unwrap().iter().find(|b| b["action"] == "save_map").unwrap().clone();
        assert_eq!(save["shortcut"], "Ctrl+S");
        assert_eq!(save["customized"], false);

        let r = ok(&mut state, "set_keybinding", json!({ "action": "carve", "shortcut": "ctrl+shift+F9" }));
        assert_eq!(r["shortcut"], "Ctrl+Shift+F9");
        assert_eq!(r["customized"], true);
        assert_eq!(state.settings.keybindings.get("carve").map(String::as_str), Some("Ctrl+Shift+F9"));
        let custom = ok(&mut state, "list_keybindings", json!({ "customized_only": true }));
        assert_eq!(custom["bindings"].as_array().unwrap().len(), 1);
        let r = ok(&mut state, "set_keybinding", json!({ "action": "carve", "shortcut": "Ctrl+S" }));
        assert_eq!(r["conflicts_with"], json!(["save_map"]));
        let r = ok(&mut state, "set_keybinding", json!({ "action": "carve", "shortcut": "" }));
        assert!(r["shortcut"].is_null());
        let r = ok(&mut state, "set_keybinding", json!({ "action": "carve", "reset": true }));
        assert_eq!(r["customized"], false);
        assert!(state.settings.keybindings.is_empty());

        assert!(err(&mut state, "set_keybinding", json!({ "action": "carve" })).contains("give shortcut"));
        assert!(
            err(&mut state, "set_keybinding", json!({ "action": "carve", "shortcut": "Ctrl+Nope" }))
                .contains("unknown key")
        );
        assert!(
            err(&mut state, "set_keybinding", json!({ "action": "nope", "shortcut": "A" })).contains("unknown action")
        );
        assert!(
            err(&mut state, "set_keybinding", json!({ "action": "carve", "shortcut": "A", "reset": true }))
                .contains("not both")
        );
    }
}
