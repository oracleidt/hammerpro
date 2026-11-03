use hp_app::AppState;
use hp_mcp::protocol::ToolResult;
use hp_mcp::tools::{ToolKind, ToolSpec};
use serde_json::{Map, Value, json};

use super::schema::{object, string};
use super::tools::{DOMAINS, Domain, Outcome, R, args};

pub(crate) const DOMAIN: Domain = Domain { category: "help", specs: SPECS, run };

const SPECS: &[ToolSpec] = &[ToolSpec {
    name: "get_capabilities",
    title: "Help: what the tools can do",
    description: "Every Hammer Pro MCP tool by category with an example call, and tips; or one category, or one \
        tool with its full description and argument schema. Start here.",
    kind: ToolKind::Read,
    schema: || {
        object(
            json!({
                "category": string("Only this category (as listed)."),
                "tool": string("One tool: its description, schema and example."),
            }),
            &[],
        )
    },
}];

const CORE: &[(&str, &str)] = &[
    ("get_map_info", "map"),
    ("list_entities", "entities"),
    ("get_entity", "entities"),
    ("create_entity", "entities"),
    ("set_entity_keyvalues", "entities"),
    ("delete_objects", "brushes and objects"),
    ("list_solids", "brushes and objects"),
    ("create_brush", "brushes and objects"),
    ("apply_material", "brushes and objects"),
    ("move_objects", "brushes and objects"),
    ("rotate_objects", "brushes and objects"),
    ("select_objects", "selection"),
    ("get_selection", "selection"),
    ("clear_selection", "selection"),
    ("search_materials", "assets"),
    ("search_models", "assets"),
    ("get_vmf_text", "clipboard"),
    ("paste_vmf_text", "clipboard"),
    ("check_problems", "map health"),
    ("compile_map", "compile"),
    ("get_compile_status", "compile"),
    ("save_map", "files"),
    ("open_map", "files"),
    ("undo", "history"),
    ("redo", "history"),
    ("set_camera", "views"),
    ("screenshot_view", "views"),
];

const EXAMPLES: &[(&str, &str)] = &[
    ("create_brush", r#"{"min":[0,0,0],"max":[512,512,16],"material":"dev/dev_measuregeneric01b"}"#),
    ("create_entity", r#"{"classname":"light","origin":[0,0,128],"keyvalues":{"_light":"255 255 255 400"}}"#),
    ("list_entities", r#"{"classname":"prop_*","limit":20}"#),
    ("set_camera", r#"{"position":[-512,-512,256],"look_at":[0,0,0]}"#),
    ("screenshot_view", r#"{"view":"3d","width":1280,"height":720}"#),
    ("compile_map", r#"{"preset":"draft"}"#),
    ("save_map", r#"{}"#),
    ("run_action", r#"{"action":"select_all"}"#),
    ("list_actions", r#"{"category":"view","query":"render"}"#),
    ("set_setting", r#"{"path":"view.fov_deg","value":90}"#),
    ("get_settings", r#"{"path":"view"}"#),
    ("set_keybinding", r#"{"action":"carve","shortcut":"Ctrl+Shift+C"}"#),
    ("set_view_layout", r#"{"quadrants":["3d","top","front","side"],"maximize":"none"}"#),
    ("set_render_mode", r#"{"mode":"wireframe"}"#),
    ("zoom_2d", r#"{"view":"top","factor":2}"#),
    ("pan_2d", r#"{"view":"top","center":[0,0,0]}"#),
    ("player_view", r#"{"active":true,"wireframe":1}"#),
    ("screenshot_window", r#"{"max_size":1280}"#),
    ("describe_ui", r#"{"console_lines":10}"#),
    ("ui_click", r#"{"view":"top","world":[0,0,0]}"#),
    ("ui_double_click", r#"{"x":400,"y":300}"#),
    ("ui_drag", r#"{"view":"top","from":{"world":[0,0,0]},"to":{"world":[256,256,0]}}"#),
    ("ui_scroll", r#"{"view":"3d","dy":3}"#),
    ("ui_key", r#"{"key":"Ctrl+Z"}"#),
    ("ui_type", r#"{"text":"prop_physics"}"#),
    ("ui_move", r#"{"view":"3d","fx":0.5,"fy":0.5}"#),
    ("wait_frames", r#"{"frames":5}"#),
    ("get_capabilities", r#"{"category":"ui automation"}"#),
    ("use_instance", r#"{"pid":1234}"#),
    ("delete_storage", r#"{"ids":["thumbnails"],"confirm":false}"#),
];

const ORDER: &[&str] = &[
    "help",
    "map",
    "files",
    "bsp sniffer",
    "brushes and objects",
    "entities",
    "selection",
    "history",
    "clipboard",
    "assets",
    "map health",
    "compile",
    "views",
    "actions",
    "settings",
    "ui automation",
    "windows",
];

const TIPS: &[&str] = &[
    "Look before and after you edit: screenshot_view (one view) or screenshot_window (the whole interface), \
     and describe_ui for the texts of dialogs, menus and the console.",
    "Every edit is one undo step named \"MCP: <tool>\"; undo takes it back.",
    "run_action reaches every command of the menus (list_actions lists them, enabled and checked).",
    "ui_click, ui_drag, ui_key and ui_type act like the user, at window points, view fractions or world \
     points of a view; wait_frames lets animations and loading settle.",
    "Coordinates are Source units, Z up; 2D views: top looks down Z, front looks along -X, side along +Y.",
];

pub(crate) fn catalog() -> Vec<(&'static str, &'static ToolSpec)> {
    let mut out: Vec<(&'static str, &'static ToolSpec)> = hp_mcp::tools::EDITOR_TOOLS
        .iter()
        .map(|t| (CORE.iter().find(|(n, _)| *n == t.name).map_or("map", |(_, c)| *c), t))
        .collect();
    for d in DOMAINS {
        out.extend(d.specs.iter().map(|t| (d.category, t)));
    }
    out.extend(hp_mcp::tools::BRIDGE_TOOLS.iter().map(|t| ("windows", t)));
    out
}

fn categories(catalog: &[(&'static str, &'static ToolSpec)]) -> Vec<&'static str> {
    let mut out: Vec<&'static str> = ORDER.iter().copied().filter(|c| catalog.iter().any(|(k, _)| k == c)).collect();
    for (c, _) in catalog {
        if !out.contains(c) {
            out.push(c);
        }
    }
    out
}

fn sample(schema: &Value) -> Value {
    if let Some(first) = schema.get("enum").and_then(Value::as_array).and_then(|e| e.first()) {
        return first.clone();
    }
    let min = schema.get("minimum").and_then(Value::as_f64);
    let max = schema.get("maximum").and_then(Value::as_f64);
    match schema.get("type").and_then(Value::as_str) {
        Some("integer") => {
            let v = 1.0f64.clamp(min.unwrap_or(f64::MIN), max.unwrap_or(f64::MAX));
            json!(v as i64)
        }
        Some("number") => json!(0.0f64.clamp(min.unwrap_or(f64::MIN), max.unwrap_or(f64::MAX))),
        Some("boolean") => json!(true),
        Some("string") => json!("text"),
        Some("array") => {
            let n = schema.get("minItems").and_then(Value::as_u64).unwrap_or(1).min(8);
            let item = schema.get("items").map_or(Value::Null, sample);
            Value::Array((0..n).map(|_| item.clone()).collect())
        }
        Some("object") => example_of(schema),
        _ => Value::Null,
    }
}

fn example_of(schema: &Value) -> Value {
    let mut out = Map::new();
    let props = schema.get("properties");
    for r in schema.get("required").and_then(Value::as_array).into_iter().flatten() {
        if let Some(name) = r.as_str() {
            let s = props.and_then(|p| p.get(name)).cloned().unwrap_or(Value::Null);
            out.insert(name.to_owned(), sample(&s));
        }
    }
    Value::Object(out)
}

pub(crate) fn example(spec: &ToolSpec) -> Value {
    EXAMPLES
        .iter()
        .find(|(n, _)| *n == spec.name)
        .and_then(|(_, text)| serde_json::from_str(text).ok())
        .unwrap_or_else(|| example_of(&(spec.schema)()))
}

fn distance(a: &str, b: &str) -> usize {
    let (a, b) = (a.as_bytes(), b.as_bytes());
    let mut prev: Vec<usize> = (0..=b.len()).collect();
    for (i, ca) in a.iter().enumerate() {
        let mut cur = vec![i + 1; b.len() + 1];
        for (j, cb) in b.iter().enumerate() {
            cur[j + 1] = (prev[j] + usize::from(ca != cb)).min(prev[j + 1] + 1).min(cur[j] + 1);
        }
        prev = cur;
    }
    prev[b.len()]
}

pub(crate) fn suggest<'a>(wanted: &str, names: impl Iterator<Item = &'a str>) -> Vec<&'a str> {
    let wanted = wanted.trim().to_ascii_lowercase();
    if wanted.len() > 64 {
        return Vec::new();
    }
    let words: Vec<&str> = wanted.split('_').filter(|w| w.len() > 2).collect();
    let mut scored: Vec<(usize, &str)> = names
        .filter_map(|n| {
            let d = distance(&wanted, n);
            if d <= (wanted.len() / 4).max(2) {
                Some((d, n))
            } else if words.iter().any(|w| n.contains(w)) {
                Some((100, n))
            } else {
                None
            }
        })
        .collect();
    scored.sort();
    scored.into_iter().take(10).map(|(_, n)| n).collect()
}

fn kind_name(k: ToolKind) -> &'static str {
    match k {
        ToolKind::Read => "reads",
        ToolKind::Edit => "edits the map",
        ToolKind::Destructive => "edits the map (can remove things)",
        ToolKind::Editor => "changes the editor",
    }
}

fn run(_state: &mut AppState, name: &str, v: &Value) -> Option<Outcome> {
    (name == "get_capabilities")
        .then(|| Outcome::Done(capabilities(v).unwrap_or_else(|e| ToolResult::error(format!("{name}: {e}")))))
}

fn capabilities(v: &Value) -> R<ToolResult> {
    let a = args(v, &["category", "tool"])?;
    let catalog = catalog();
    let only = a.opt_str("category", 64)?.map(|c| c.trim().to_ascii_lowercase());
    if let Some(tool) = a.opt_str("tool", 64)? {
        if only.is_some() {
            return Err("give category or tool, not both".to_owned());
        }
        let tool = tool.trim();
        let (category, spec) = catalog.iter().find(|(_, t)| t.name == tool).ok_or_else(|| {
            let close = suggest(tool, catalog.iter().map(|(_, t)| t.name));
            format!("no tool \"{tool}\" (close: {})", close.join(", "))
        })?;
        return Ok(ToolResult::json(json!({
            "name": spec.name,
            "title": spec.title,
            "category": category,
            "effect": kind_name(spec.kind),
            "description": spec.description,
            "input_schema": (spec.schema)(),
            "example": example(spec),
        })));
    }
    let cats = categories(&catalog);
    if let Some(c) = &only
        && !cats.contains(&c.as_str())
    {
        return Err(format!("no category \"{c}\" (categories: {})", cats.join(", ")));
    }
    let list: Vec<Value> = cats
        .iter()
        .filter(|c| only.as_deref().is_none_or(|o| o == **c))
        .map(|c| {
            let tools: Vec<Value> = catalog
                .iter()
                .filter(|(k, _)| k == c)
                .map(|(_, t)| {
                    let mut e = json!({ "name": t.name, "title": t.title, "example": example(t) });
                    if only.is_some() {
                        e["description"] = json!(t.description);
                    }
                    e
                })
                .collect();
            json!({ "category": c, "tools": tools })
        })
        .collect();
    Ok(ToolResult::json(json!({
        "server": format!("Hammer Pro {}", env!("CARGO_PKG_VERSION")),
        "tools": catalog.len(),
        "categories": list,
        "tips": TIPS,
        "more": "get_capabilities with tool: \"<name>\" gives the description and the argument schema.",
    })))
}

#[cfg(test)]
pub(crate) const DOCS_BEGIN: &str = "<!-- BEGIN TOOL LIST: generated from the registry, see tools_help.rs -->";
#[cfg(test)]
pub(crate) const DOCS_END: &str = "<!-- END TOOL LIST -->";

#[cfg(test)]
fn arg_text(s: &Value) -> String {
    let range = |s: &Value| match (s.get("minimum"), s.get("maximum")) {
        (Some(a), Some(b)) => format!(" {a} to {b}"),
        _ => String::new(),
    };
    if let Some(e) = s.get("enum").and_then(Value::as_array) {
        let names: Vec<String> = e.iter().map(|x| x.as_str().map_or_else(|| x.to_string(), str::to_owned)).collect();
        return format!("one of {}", names.join(", "));
    }
    match s.get("type").and_then(Value::as_str) {
        Some("integer") => format!("integer{}", range(s)),
        Some("number") => format!("number{}", range(s)),
        Some("boolean") => "true or false".to_owned(),
        Some("string") => "text".to_owned(),
        Some("array") => {
            let item = s.get("items").map_or_else(|| "values".to_owned(), arg_text);
            match (s.get("minItems").and_then(Value::as_u64), s.get("maxItems").and_then(Value::as_u64)) {
                (Some(a), Some(b)) if a == b => format!("{a} x {item}"),
                _ => format!("list of {item}"),
            }
        }
        Some("object") => match s.get("properties").and_then(Value::as_object) {
            Some(p) if !p.is_empty() => format!("object ({})", p.keys().cloned().collect::<Vec<_>>().join(", ")),
            _ => "object".to_owned(),
        },
        _ => "any value".to_owned(),
    }
}

#[cfg(test)]
fn md(text: &str) -> String {
    text.replace('|', "\\|").replace('\n', " ")
}

#[cfg(test)]
pub(crate) fn tools_markdown() -> String {
    let catalog = catalog();
    let mut out = String::new();
    out.push_str(DOCS_BEGIN);
    out.push_str(&format!(
        "\n\n{} tools. Arguments marked * are required. `get_capabilities` gives the same list with an example \
         call for each tool.\n",
        catalog.len()
    ));
    for c in categories(&catalog) {
        let mut title = c.to_owned();
        if let Some(first) = title.get_mut(0..1) {
            first.make_ascii_uppercase();
        }
        out.push_str(&format!("\n### {title}\n\n| Tool | What it does | Arguments |\n|---|---|---|\n"));
        for (_, t) in catalog.iter().filter(|(k, _)| *k == c) {
            let schema = (t.schema)();
            let required: Vec<&str> = schema
                .get("required")
                .and_then(Value::as_array)
                .into_iter()
                .flatten()
                .filter_map(Value::as_str)
                .collect();
            let argsv: Vec<String> = schema
                .get("properties")
                .and_then(Value::as_object)
                .into_iter()
                .flatten()
                .map(|(name, s)| {
                    let star = if required.contains(&name.as_str()) { "*" } else { "" };
                    format!("`{name}`{star}: {}", arg_text(s))
                })
                .collect();
            let argsv = if argsv.is_empty() { "none".to_owned() } else { argsv.join("; ") };
            out.push_str(&format!("| `{}` | {} | {} |\n", t.name, md(t.description), md(&argsv)));
        }
    }
    out.push('\n');
    out.push_str(DOCS_END);
    out
}

#[cfg(test)]
pub(crate) fn replace_docs_list(doc: &str, generated: &str) -> Option<String> {
    let start = doc.find(DOCS_BEGIN)?;
    let end = doc[start..].find(DOCS_END)? + start + DOCS_END.len();
    Some(format!("{}{}{}", &doc[..start], generated, &doc[end..]))
}

#[cfg(test)]
mod tests {
    use super::*;

    fn caps(v: Value) -> ToolResult {
        let mut state = hp_app::testing::test_state();
        match run(&mut state, "get_capabilities", &v) {
            Some(Outcome::Done(r)) => r,
            other => panic!("{other:?}"),
        }
    }

    #[test]
    fn every_tool_is_listed_with_an_example() {
        let catalog = catalog();
        let r = caps(json!({}));
        let v = r.structured.unwrap();
        assert_eq!(v["tools"].as_u64().unwrap() as usize, catalog.len());
        let listed: usize =
            v["categories"].as_array().unwrap().iter().map(|c| c["tools"].as_array().unwrap().len()).sum();
        assert_eq!(listed, catalog.len());
        for (_, t) in &catalog {
            let e = example(t);
            assert!(e.is_object(), "{}", t.name);
            let schema = (t.schema)();
            let props = schema["properties"].as_object().unwrap();
            for k in e.as_object().unwrap().keys() {
                assert!(props.contains_key(k), "{}: example argument {k}", t.name);
            }
            for r in schema.get("required").and_then(Value::as_array).into_iter().flatten() {
                assert!(e.get(r.as_str().unwrap()).is_some(), "{}: example misses {r}", t.name);
            }
        }
        for (name, text) in EXAMPLES {
            assert!(serde_json::from_str::<Value>(text).is_ok(), "{name}");
            assert!(catalog.iter().any(|(_, t)| t.name == *name), "example of an unknown tool {name}");
        }
        for (name, _) in CORE {
            assert!(hp_mcp::tools::EDITOR_TOOLS.iter().any(|t| t.name == *name), "{name} is not a core tool");
        }

        let one = caps(json!({ "tool": "ui_click" })).structured.unwrap();
        assert_eq!(one["category"], "ui automation");
        assert!(one["input_schema"]["properties"]["view"].is_object());
        let cat = caps(json!({ "category": "Settings" })).structured.unwrap();
        assert_eq!(cat["categories"].as_array().unwrap().len(), 1);
        assert!(cat["categories"][0]["tools"][0]["description"].is_string());
        assert!(caps(json!({ "tool": "ui_clik" })).first_text().contains("close: ui_click"));
        assert!(caps(json!({ "category": "zzz" })).is_error);
        assert!(caps(json!({ "x": 1 })).is_error);
    }

    #[test]
    fn docs_list_every_tool() {
        let generated = tools_markdown();
        for (_, t) in catalog() {
            assert!(generated.contains(&format!("| `{}` |", t.name)), "{}", t.name);
        }
        for c in ['\u{2013}', '\u{2014}'] {
            assert!(!generated.contains(c));
        }
        let path = std::path::Path::new(env!("CARGO_MANIFEST_DIR")).join("../../docs/MCP.md");
        let doc = std::fs::read_to_string(&path).expect("docs/MCP.md");
        let updated = replace_docs_list(&doc, &generated).expect("docs/MCP.md has the tool list markers");
        if updated != doc {
            if std::env::var_os("HP_UPDATE_MCP_DOCS").is_some() {
                std::fs::write(&path, updated).expect("write docs/MCP.md");
            } else {
                panic!(
                    "the tool list of docs/MCP.md is out of date: run \
                     `HP_UPDATE_MCP_DOCS=1 cargo test -p hammerpro docs_list_every_tool` (with your --target-dir)"
                );
            }
        }
    }

    #[test]
    fn markdown_replacement() {
        let doc = format!("a\n{DOCS_BEGIN}\nold\n{DOCS_END}\nb\n");
        assert_eq!(replace_docs_list(&doc, "NEW").unwrap(), "a\nNEW\nb\n");
        assert!(replace_docs_list("no markers", "x").is_none());
        assert_eq!(arg_text(&json!({ "type": "integer", "minimum": 1, "maximum": 5 })), "integer 1 to 5");
        assert_eq!(
            arg_text(&json!({ "type": "array", "items": { "type": "number" }, "minItems": 3, "maxItems": 3 })),
            "3 x number"
        );
        assert_eq!(md("a|b"), "a\\|b");
        assert_eq!(distance("ui_clik", "ui_click"), 1);
        assert_eq!(suggest("ui_clik", ["ui_click", "ui_key", "get_views"].into_iter()), vec!["ui_click"]);
        assert_eq!(suggest("select_everything", ["select_all", "carve"].into_iter()), vec!["select_all"]);
        assert!(suggest(&"x".repeat(100), ["x"].into_iter()).is_empty());
    }
}
