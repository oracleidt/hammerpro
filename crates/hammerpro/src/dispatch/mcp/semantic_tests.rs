use hp_app::AppState;
use hp_mcp::protocol::ToolResult;
use hp_mcp::tools::ToolKind;
use serde_json::{Value, json};

use super::tools::{DOMAINS, Domain, Outcome, all_specs, run, undo_label};

pub(crate) fn call(state: &mut AppState, name: &str, args: Value) -> ToolResult {
    match run(state, name, &args) {
        Outcome::Done(r) => r,
        Outcome::App(op) => ToolResult::error(format!("app op {op:?}")),
        Outcome::AppFn(_) => ToolResult::error("app closure"),
        Outcome::Worker(w) => w.wait(state, std::time::Duration::from_secs(60)),
        #[allow(unreachable_patterns)]
        _ => ToolResult::error("deferred"),
    }
}

pub(crate) fn ok(state: &mut AppState, name: &str, args: Value) -> Value {
    let r = call(state, name, args.clone());
    assert!(!r.is_error, "{name} {args}: {}", r.first_text());
    r.structured.clone().unwrap_or_else(|| serde_json::from_str(r.first_text()).unwrap_or(Value::Null))
}

pub(crate) fn err(state: &mut AppState, name: &str, args: Value) -> String {
    let r = call(state, name, args.clone());
    assert!(r.is_error, "{name} {args} should fail: {}", r.first_text());
    r.first_text().to_owned()
}

pub(crate) fn needs_app(state: &mut AppState, name: &str, args: Value) -> bool {
    !matches!(run(state, name, &args), Outcome::Done(_))
}

fn samples(p: &Value) -> Vec<Value> {
    let mut out = Vec::new();
    match p["type"].as_str() {
        Some("integer") => {
            let min = p["minimum"].as_i64().unwrap_or(0);
            let max = p["maximum"].as_i64().unwrap_or(1000);
            out.extend([json!(min.max(2).min(max)), json!(min), json!(max), json!(min - 1), json!(1.5)]);
        }
        Some("number") => {
            let min = p["minimum"].as_f64().unwrap_or(-10.0);
            let max = p["maximum"].as_f64().unwrap_or(10.0);
            out.extend([json!((min + max) / 2.0), json!(min), json!(max), json!(max * 2.0 + 1.0)]);
        }
        Some("boolean") => out.extend([json!(true), json!(false), json!("yes")]),
        Some("string") => match p["enum"].as_array() {
            Some(values) => out.extend(values.iter().cloned().chain([json!("nope")])),
            None => out.extend([json!("dev/dev_measuregeneric01b"), json!(""), json!("a\"b"), json!("x".repeat(5000))]),
        },
        Some("array") => match p["items"]["type"].as_str() {
            Some("number") => out.extend([json!([16, 32, 64]), json!([0, 0, 0]), json!([1e12, 0, 0]), json!([1, 2])]),
            Some("integer") => out.extend([json!([2]), json!([4, 5, 6, 7]), json!([999_999]), json!([0]), json!([])]),
            Some("string") => out.extend([json!(["light", "prop_*"]), json!([""]), json!([1])]),
            Some("object") => out.extend([
                json!([{ "solid": 2, "side": 0 }]),
                json!([{ "solid": 2, "side": 99 }]),
                json!([{ "solid": 99_999, "side": 0 }]),
                json!([{}]),
            ]),
            Some("array") => out.extend([json!([[0, 0, 0], [64, 0, 0], [0, 64, 0]]), json!([[0, 0, 0]])]),
            _ => out.push(json!([])),
        },
        Some("object") => out.extend([json!({ "targetname": "door1" }), json!({ "id": "5" }), json!([])]),
        _ => out.push(Value::Null),
    }
    out.push(json!({ "bad": [1] }));
    out
}

const SEMANTIC: &[&Domain] = &[
    &super::tools_edit::DOMAIN,
    &super::tools_weld::DOMAIN,
    &super::tools_loops::DOMAIN,
    &super::tools_roadside::DOMAIN,
    &super::tools_pipe::DOMAIN,
    &super::tools_slice::DOMAIN,
    &super::tools_groups::DOMAIN,
    &super::tools_disp::DOMAIN,
    &super::tools_slope::DOMAIN,
    &super::tools_terrain::DOMAIN,
    &super::tools_meshdisp::DOMAIN,
    &super::tools_instances::DOMAIN,
    &super::tools_entities::DOMAIN,
    &super::tools_place::DOMAIN,
    &super::tools_assets::DOMAIN,
    &super::tools_compile::DOMAIN,
    &super::tools_compilers::DOMAIN,
    &super::tools_health::DOMAIN,
    &super::tools_content::DOMAIN,
    &super::tools_docs::DOMAIN,
    &super::tools_history::DOMAIN,
    &super::tools_overlays::DOMAIN,
    &super::tools_sky::DOMAIN,
    &super::tools_fgd::DOMAIN,
    &super::tools_toolopts::DOMAIN,
    &super::tools_shader::DOMAIN,
    &super::tools_vtf::DOMAIN,
    &super::tools_func_detail::DOMAIN,
];

fn domain_specs() -> impl Iterator<Item = &'static hp_mcp::tools::ToolSpec> {
    SEMANTIC.iter().flat_map(|d| d.specs.iter())
}

#[test]
fn domain_tools_are_well_formed_and_unique() {
    let mut names = std::collections::HashSet::new();
    for t in all_specs().chain(hp_mcp::tools::BRIDGE_TOOLS) {
        assert!(names.insert(t.name), "duplicate tool {}", t.name);
    }
    for d in SEMANTIC {
        assert!(DOMAINS.iter().any(|r| std::ptr::eq(*r, *d)), "{} is not registered", d.category);
        assert!(!d.category.is_empty() && d.category.chars().all(|c| c.is_ascii_lowercase() || c == ' '));
    }
    for t in domain_specs() {
        assert!(t.name.len() <= 64 && t.name.chars().all(|c| c.is_ascii_lowercase() || c.is_ascii_digit() || c == '_'));
        assert!(t.description.len() > 20 && !t.description.contains("  "), "{}", t.name);
        let d = t.definition();
        let text = serde_json::to_string(&d).unwrap();
        assert!(!text.contains("oneOf") && !text.contains("anyOf") && !text.contains("\"type\":["), "{}", t.name);
        for c in ['\u{2013}', '\u{2014}'] {
            assert!(!text.contains(c), "{}", t.name);
        }
        let schema = &d["inputSchema"];
        assert_eq!(schema["type"], "object");
        for r in schema["required"].as_array().unwrap() {
            assert!(schema["properties"].get(r.as_str().unwrap()).is_some(), "{}: {r}", t.name);
        }
    }
}

#[test]
fn edits_have_translated_undo_labels() {
    hp_app::register_translations();
    for t in domain_specs().filter(|t| matches!(t.kind, ToolKind::Edit | ToolKind::Destructive)) {
        let key = hp_ui_shell::menu::undo_key(&undo_label(t.name));
        assert!(hp_app::i18n::has_key(&key), "missing translation {key}");
    }
}

#[test]
fn bad_arguments_leave_no_undo_step() {
    for t in domain_specs() {
        let mut state = hp_app::testing::test_state();
        let before = state.doc.history_len();
        for args in [json!({ "no_such_argument": 1 }), json!([1, 2]), json!("text")] {
            let r = call(&mut state, t.name, args.clone());
            assert!(r.is_error, "{} accepted {args}", t.name);
        }
        assert_eq!(state.doc.history_len(), before, "{}", t.name);
        assert!(!state.doc.in_transaction(), "{}", t.name);
    }
}

#[test]
fn no_argument_makes_a_tool_panic() {
    for t in domain_specs() {
        let schema = (t.schema)();
        let props = schema["properties"].as_object().cloned().unwrap_or_default();
        let required: Vec<String> =
            schema["required"].as_array().unwrap().iter().map(|r| r.as_str().unwrap().to_owned()).collect();
        let base: serde_json::Map<String, Value> =
            required.iter().map(|r| (r.clone(), samples(&props[r])[0].clone())).collect();
        let mut state = hp_app::testing::test_state();
        let _ = call(&mut state, t.name, Value::Object(base.clone()));
        let _ = call(&mut state, t.name, json!({}));
        for (k, p) in &props {
            for s in samples(p) {
                let mut args = base.clone();
                args.insert(k.clone(), s);
                let mut state = hp_app::testing::test_state();
                state.doc.select(&[hp_editor::ObjRef::Solid(3)], hp_editor::SelectOp::Replace);
                let _ = call(&mut state, t.name, Value::Object(args));
                assert!(!state.doc.in_transaction(), "{} left a transaction open", t.name);
            }
        }
    }
}

#[test]
fn file_tools_refuse_network_and_device_paths() {
    use super::semantic::{local_path, relative_file};
    let bs = char::from(92);
    let unc = format!("{bs}{bs}attacker{bs}share{bs}map.vmf");
    let verbatim = format!("{bs}{bs}?{bs}C:{bs}maps{bs}map.vmf");
    let device = format!("{bs}{bs}.{bs}PhysicalDrive0");
    for bad in [
        unc.as_str(),
        "//attacker/share/map.vmf",
        verbatim.as_str(),
        device.as_str(),
        "C:/maps/con.vmf",
        "C:/maps/NUL",
        "C:/maps/lpt1.txt",
        "C:secret.obj",
        "C:",
        "C:/maps/map.vmf:stream",
        "C:/maps/a\u{1}b.vmf",
        "",
    ] {
        assert!(local_path(bad).is_err(), "{bad:?}");
    }
    for good in ["C:/maps/gm_test.vmf", "D:/GMC/maps/console.vmf", "relative/sub/file.obj", "C:/maps/com10.txt"] {
        assert!(local_path(good).is_ok(), "{good}");
    }
    assert!(relative_file("con", "vmf").is_err());
    assert!(relative_file("parts/aux.vmf", "vmf").is_err());
    assert!(relative_file("parts/auxiliary", "vmf").is_ok());
    let mut s = hp_app::testing::test_state();
    let e = err(&mut s, "open_map", json!({ "path": unc }));
    assert!(e.contains("UNC"), "{e}");
    let e = err(&mut s, "validate_fgd", json!({ "path": "//host/share/base.fgd" }));
    assert!(e.contains("UNC"), "{e}");
    let e = err(&mut s, "import_models_batch", json!({ "paths": ["//host/share/models"] }));
    assert!(e.contains("UNC"), "{e}");
}

#[test]
fn team_mapping_undo_goes_through_the_shared_history() {
    let mut s = hp_app::testing::test_state();
    assert!(!needs_app(&mut s, "undo", json!({})), "a local map undoes in the document");
    s.runtime.collaboration.connected = true;
    s.runtime.collaboration.active_map = true;
    for (name, args) in [
        ("undo", json!({})),
        ("redo", json!({})),
        ("undo_steps", json!({ "count": 3 })),
        ("redo_steps", json!({ "count": 1 })),
    ] {
        assert!(needs_app(&mut s, name, args), "{name} must reach Team Mapping history through the app");
    }
    assert!(call(&mut s, "undo", json!({ "x": 1 })).is_error);
}
