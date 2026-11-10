use std::sync::Arc;
use std::time::{Duration, Instant};

use egui::{Rect, ViewportCommand};
use hp_app::{Action, AppState, ViewKind};
use hp_mcp::protocol::ToolResult;
use hp_mcp::server::{McpServer, ServerConfig};
use serde_json::{Map, Value, json};

use super::automation::{Reply, UiAutomation, UiHost};
use super::tools::{self, Outcome};
use super::tools_help::{catalog, example};

pub(crate) struct TestHost {
    pub(crate) state: AppState,
    pub(crate) executed: Vec<Action>,
    pub(crate) text: String,
    pub(crate) clicks: usize,
}

impl TestHost {
    pub(crate) fn new() -> Self {
        Self { state: hp_app::testing::test_state(), executed: Vec::new(), text: String::new(), clicks: 0 }
    }

    fn quadrant(&self, i: usize) -> hp_app::QuadrantView {
        hp_ui_viewports::layout::quadrant_view(&self.state.settings.ui.quadrants, i)
    }
}

fn quadrant_rect(i: usize) -> Rect {
    let (x, y) = [(0.0, 60.0), (600.0, 60.0), (0.0, 460.0), (600.0, 460.0)][i.min(3)];
    Rect::from_min_size(egui::pos2(x, y), egui::vec2(600.0, 400.0))
}

pub(crate) const WINDOW: egui::Vec2 = egui::vec2(1600.0, 900.0);

impl UiHost for TestHost {
    fn state(&mut self) -> &mut AppState {
        &mut self.state
    }

    fn execute(&mut self, _ctx: &egui::Context, action: Action) {
        self.executed.push(action);
        match action {
            Action::SelectAll => self.state.doc.select_all(),
            Action::Undo => {
                self.state.doc.undo();
            }
            Action::Redo => {
                self.state.doc.redo();
            }
            _ => {}
        }
    }

    fn view_rects(&self) -> [Option<(ViewKind, Rect)>; 4] {
        std::array::from_fn(|i| self.quadrant(i).kind().map(|kind| (kind, quadrant_rect(i))))
    }

    fn logical_rects(&self) -> [Option<Rect>; 4] {
        std::array::from_fn(|i| self.quadrant(i).is_logical().then(|| quadrant_rect(i)))
    }

    fn open_maps(&self) -> Vec<Value> {
        vec![json!({ "title": self.state.map_title(), "active": true })]
    }

    fn prompts(&self) -> Vec<&'static str> {
        Vec::new()
    }
}

pub(crate) fn frame(ctx: &egui::Context, host: &mut TestHost, ui: &mut UiAutomation) {
    let mut raw =
        egui::RawInput { screen_rect: Some(Rect::from_min_size(egui::Pos2::ZERO, WINDOW)), ..Default::default() };
    ui.inject(ctx, &mut raw);
    let mut out = ctx.run_ui(raw, |root| {
        let ctx = root.ctx().clone();
        ui.poll(host, &ctx);
        egui::Window::new("Test dialog").default_pos([1250.0, 100.0]).show(&ctx, |w| {
            w.label("Grid size");
            w.text_edit_singleline(&mut host.text);
            if w.button("Apply").clicked() {
                host.clicks += 1;
            }
        });
    });
    out.textures_delta.clear();
    for (_, vo) in std::mem::take(&mut out.viewport_output) {
        for cmd in vo.commands {
            if let ViewportCommand::Screenshot(cb) = cmd {
                let size = [(WINDOW.x / 4.0) as usize, (WINDOW.y / 4.0) as usize];
                cb.complete(Arc::new(egui::ColorImage::filled(size, egui::Color32::from_gray(40))));
            }
        }
    }
}

pub(crate) fn dispatch(
    ctx: &egui::Context,
    host: &mut TestHost,
    ui: &mut UiAutomation,
    name: &str,
    args: &Value,
    reply: Reply,
) {
    match tools::run(&mut host.state, name, args) {
        Outcome::Done(r) => reply(r),
        Outcome::App(op) => reply(ToolResult::error(format!("{name}: needs the application ({op:?})"))),
        Outcome::AppFn(_) => reply(ToolResult::error(format!("{name}: needs the application"))),
        Outcome::Ui(op) => ui.start(host, ctx, op, reply),
        Outcome::Worker(w) => reply(w.wait(&mut host.state, std::time::Duration::from_secs(60))),
    }
}

fn wrong_type(schema: &Value) -> Option<Value> {
    if schema.get("enum").is_some() {
        return Some(json!("not-a-choice"));
    }
    Some(match schema.get("type").and_then(Value::as_str)? {
        "string" => json!(12345),
        "number" => json!("x"),
        "integer" => json!(1.5),
        "boolean" => json!("yes"),
        "array" => json!({ "a": 1 }),
        "object" => json!([1, 2]),
        _ => return None,
    })
}

fn out_of_range(schema: &Value) -> Option<Value> {
    let max = schema.get("maximum")?.as_f64()?;
    Some(match schema.get("type").and_then(Value::as_str) {
        Some("integer") => json!((max as i64).checked_add(1).filter(|_| max < 9.0e18)?),
        _ => json!(max + 1.0 + max.abs()),
    })
}

fn calls_for(spec: &hp_mcp::tools::ToolSpec) -> Vec<(Value, bool)> {
    let ex = example(spec);
    let mut out = vec![(ex.clone(), false), (json!({ "zzz_unknown": 1 }), true)];
    let schema = (spec.schema)();
    for (name, s) in schema.get("properties").and_then(Value::as_object).into_iter().flatten() {
        let with = |v: Value| {
            let mut m: Map<String, Value> = ex.as_object().cloned().unwrap_or_default();
            m.insert(name.clone(), v);
            Value::Object(m)
        };
        if let Some(v) = wrong_type(s) {
            out.push((with(v), true));
        }
        if let Some(v) = out_of_range(s) {
            out.push((with(v), true));
        }
    }
    out
}

const GENERIC: &[&super::tools::Domain] = &[
    &super::tools_actions::DOMAIN,
    &super::tools_settings::DOMAIN,
    &super::tools_views::DOMAIN,
    &super::tools_ui::DOMAIN,
    &super::tools_help::DOMAIN,
];

#[test]
fn every_tool_through_the_protocol() {
    let _lang = hp_app::testing::lang_lock();
    let dir = std::env::temp_dir().join(format!("hp_mcp_harness_{}", std::process::id()));
    let _ = std::fs::remove_dir_all(&dir);
    let mut config = ServerConfig::new("harness");
    config.dir = dir.clone();
    config.tools = tools::all_definitions();
    let server = McpServer::start(config).expect("server");

    let catalog = catalog();
    let mut plan: Vec<(String, Value, bool, &'static str)> = Vec::new();
    for (category, spec) in &catalog {
        if *category == "windows" {
            continue;
        }
        for (args, must_fail) in calls_for(spec) {
            plan.push((spec.name.to_owned(), args, must_fail, category));
        }
        plan.push((spec.name.to_owned(), json!([1, 2]), true, category));
    }
    let mut messages = vec![
        json!({"jsonrpc":"2.0","id":"init","method":"initialize","params":{"protocolVersion":"2025-06-18","capabilities":{},"clientInfo":{"name":"harness","version":"1"}}}),
        json!({"jsonrpc":"2.0","method":"notifications/initialized"}),
        json!({"jsonrpc":"2.0","id":"list","method":"tools/list"}),
    ];
    for (i, (name, args, _, _)) in plan.iter().enumerate() {
        messages.push(json!({"jsonrpc":"2.0","id":i,"method":"tools/call","params":{"name":name,"arguments":args}}));
    }
    let client_dir = dir.clone();
    let client = std::thread::spawn(move || {
        let mut input = Vec::new();
        for m in &messages {
            hp_mcp::protocol::write_message(&mut input, m).unwrap();
        }
        let options = hp_mcp::bridge::BridgeOptions {
            dir: client_dir,
            pinned: None,
            call_timeout: Duration::from_secs(60),
            connect_timeout: Duration::from_secs(5),
            tools: super::tool_specs(),
        };
        let mut out = Vec::new();
        hp_mcp::bridge::run(std::io::Cursor::new(input), &mut out, options).unwrap();
        String::from_utf8(out).unwrap().lines().map(|l| serde_json::from_str::<Value>(l).unwrap()).collect::<Vec<_>>()
    });

    let ctx = egui::Context::default();
    let mut host = TestHost::new();
    let mut ui = UiAutomation::default();
    let deadline = Instant::now() + Duration::from_secs(300);
    let mut slow: Vec<String> = Vec::new();
    while !client.is_finished() && Instant::now() < deadline {
        let mut idle = true;
        while let Some(req) = server.try_recv() {
            idle = false;
            let name = req.name.clone();
            let args = req.arguments.clone();
            let reply: Reply = Box::new(move |r| req.respond(r));
            let started = Instant::now();
            dispatch(&ctx, &mut host, &mut ui, &name, &args, reply);
            let took = started.elapsed();
            if took > Duration::from_millis(100) {
                slow.push(format!("{name} {args}: {} ms", took.as_millis()));
            }
        }
        if ui.busy() {
            idle = false;
        }
        frame(&ctx, &mut host, &mut ui);
        if idle {
            std::thread::sleep(Duration::from_millis(1));
        }
    }
    let answers = client.join().expect("client");
    if !slow.is_empty() {
        eprintln!(
            "slow tool calls on the UI thread:
{}",
            slow.join(
                "
"
            )
        );
    }
    let _ = std::fs::remove_dir_all(&dir);

    let list = answers.iter().find(|a| a["id"] == "list").expect("tools/list answer");
    let listed: Vec<&str> =
        list["result"]["tools"].as_array().unwrap().iter().map(|t| t["name"].as_str().unwrap()).collect();
    for (_, spec) in &catalog {
        assert!(listed.contains(&spec.name), "{} is not listed", spec.name);
    }

    let generic: Vec<&str> = GENERIC.iter().flat_map(|d| d.specs.iter().map(|t| t.name)).collect();
    let mut problems = Vec::new();
    for (i, (name, args, must_fail, _category)) in plan.iter().enumerate() {
        let Some(answer) = answers.iter().find(|a| a["id"] == json!(i)) else {
            problems.push(format!("{name} {args}: no answer"));
            continue;
        };
        if answer.get("error").is_some() {
            if !args.is_array() {
                problems.push(format!("{name} {args}: protocol error {}", answer["error"]));
            }
            continue;
        }
        let result = &answer["result"];
        let is_error = result["isError"].as_bool().unwrap_or(false);
        let text = result["content"][0]["text"].as_str().unwrap_or("");
        if text.contains("internal error") || text.contains("unknown tool") || text.contains("panicked") {
            problems.push(format!("{name} {args}: {text}"));
        }
        if *must_fail && !is_error {
            problems.push(format!("{name} {args}: accepted invalid arguments"));
        }
        if is_error && !args.is_array() && !text.contains(name.as_str()) {
            problems.push(format!("{name} {args}: the error does not name the tool: {text}"));
        }
        if !must_fail && is_error && generic.contains(&name.as_str()) {
            problems.push(format!("{name} {args}: the example failed: {text}"));
        }
    }
    assert!(problems.is_empty(), "{} problems:\n{}", problems.len(), problems.join("\n"));
    assert!(host.executed.contains(&Action::SelectAll), "run_action ran select_all");
    assert!(!host.state.doc.selected_objects().is_empty());
}

#[test]
fn run_action_undo_and_redo_really_step_the_history() {
    let ctx = egui::Context::default();
    let mut host = TestHost::new();
    let mut ui = UiAutomation::default();
    frame(&ctx, &mut host, &mut ui);
    let answers: Arc<std::sync::Mutex<Vec<ToolResult>>> = Arc::default();
    let call = |host: &mut TestHost, ui: &mut UiAutomation, name: &str, args: Value| {
        let sink = answers.clone();
        let before = answers.lock().unwrap().len();
        dispatch(&ctx, host, ui, name, &args, Box::new(move |r| sink.lock().unwrap().push(r)));
        for _ in 0..50 {
            if answers.lock().unwrap().len() > before {
                break;
            }
            frame(&ctx, host, ui);
        }
        let r = answers.lock().unwrap().get(before).cloned().unwrap_or_else(|| panic!("{name} was answered"));
        assert!(!r.is_error, "{name}: {}", r.first_text());
        r.structured.unwrap_or_default()
    };
    let solids = host.state.doc.stats().solids;
    call(&mut host, &mut ui, "delete_objects", json!({ "solid_ids": [3] }));
    assert_eq!(host.state.doc.stats().solids, solids - 1);
    let undone = call(&mut host, &mut ui, "run_action", json!({ "action": "undo" }));
    assert_eq!(undone["undo_step"], false, "undo is not an undo step itself");
    assert_eq!(host.state.doc.stats().solids, solids, "run_action undo restored the solid");
    call(&mut host, &mut ui, "run_action", json!({ "action": "redo" }));
    assert_eq!(host.state.doc.stats().solids, solids - 1, "run_action redo deleted it again");
    assert!(!host.state.doc.in_transaction());
}

#[test]
fn logical_view_is_reachable() {
    use super::automation::{InView, Target, UiOp, ViewSel, resolve};
    let ctx = egui::Context::default();
    let mut host = TestHost::new();
    let mut ui = UiAutomation::default();
    frame(&ctx, &mut host, &mut ui);
    let answers: Arc<std::sync::Mutex<Vec<ToolResult>>> = Arc::default();
    let call = |host: &mut TestHost, ui: &mut UiAutomation, name: &str, args: Value| {
        let sink = answers.clone();
        let before = answers.lock().unwrap().len();
        dispatch(&ctx, host, ui, name, &args, Box::new(move |r| sink.lock().unwrap().push(r)));
        for _ in 0..50 {
            if answers.lock().unwrap().len() > before {
                break;
            }
            frame(&ctx, host, ui);
        }
        let r = answers.lock().unwrap().get(before).cloned().unwrap_or_else(|| panic!("{name} was answered"));
        assert!(!r.is_error, "{name}: {}", r.first_text());
        r.structured.unwrap_or_default()
    };
    let none = resolve(&mut host, &ctx, Target::View { view: ViewSel::Logical, at: InView::Fraction(0.5, 0.5) });
    assert!(none.unwrap_err().contains("no logical view"));
    let r = call(&mut host, &mut ui, "set_view_layout", json!({ "quadrants": ["3d", "logical", "front", "side"] }));
    assert_eq!(r["quadrants"][1], "logical");
    let views = call(&mut host, &mut ui, "get_views", json!({}));
    assert_eq!(views["views"][1]["kind"], "logical", "{views}");
    assert_eq!(views["views"][1]["visible"], true);
    assert_eq!(views["views"][1]["rect"], json!([600.0, 60.0, 600.0, 400.0]));
    assert_eq!(views["views"][2]["kind"], "front");
    let center = quadrant_rect(1).center();
    let at = InView::Fraction(0.5, 0.5);
    assert_eq!(resolve(&mut host, &ctx, Target::View { view: ViewSel::Logical, at }), Ok(center));
    assert_eq!(resolve(&mut host, &ctx, Target::View { view: ViewSel::Index(1), at }), Ok(center));
    let world = InView::World(glam::DVec3::ZERO);
    let e = resolve(&mut host, &ctx, Target::View { view: ViewSel::Logical, at: world }).unwrap_err();
    assert!(e.contains("no world positions"), "{e}");
    let e = resolve(&mut host, &ctx, Target::View { view: ViewSel::Kind(ViewKind::Top), at }).unwrap_err();
    assert!(e.contains("no top view"), "{e}");
    match tools::run(&mut host.state, "ui_click", &json!({ "view": "logical", "fx": 0.25, "fy": 0.5 })) {
        Outcome::Ui(UiOp::Pointer { at: Target::View { view: ViewSel::Logical, .. }, .. }) => {}
        other => panic!("ui_click in the logical view: {other:?}"),
    }
    let described = call(&mut host, &mut ui, "describe_ui", json!({}));
    let kinds: Vec<&str> = described["views"].as_array().unwrap().iter().filter_map(|v| v["kind"].as_str()).collect();
    assert!(kinds.contains(&"logical"), "{described}");
}

#[test]
fn ui_automation_drives_widgets() {
    let ctx = egui::Context::default();
    let mut host = TestHost::new();
    let mut ui = UiAutomation::default();
    let answers: Arc<std::sync::Mutex<Vec<(String, ToolResult)>>> = Arc::default();
    let call = |host: &mut TestHost, ui: &mut UiAutomation, name: &str, args: Value| {
        let sink = answers.clone();
        let tool = name.to_owned();
        let before = answers.lock().unwrap().len();
        let reply: Reply = Box::new(move |r| sink.lock().unwrap().push((tool, r)));
        dispatch(&ctx, host, ui, name, &args, reply);
        let deadline = Instant::now() + Duration::from_secs(20);
        while Instant::now() < deadline {
            if answers.lock().unwrap().len() > before && !ui.busy() {
                break;
            }
            frame(&ctx, host, ui);
            if !ui.busy() {
                std::thread::sleep(Duration::from_millis(2));
            }
        }
        assert!(answers.lock().unwrap().len() > before, "{name} was answered");
        frame(&ctx, host, ui);
    };
    for _ in 0..3 {
        frame(&ctx, &mut host, &mut ui);
    }
    call(&mut host, &mut ui, "describe_ui", json!({}));
    let described = answers.lock().unwrap().last().unwrap().1.structured.clone().unwrap();
    let win = described["windows"].as_array().unwrap().iter().find(|w| w["title"] == "Test dialog").cloned();
    let win = win.unwrap_or_else(|| panic!("the test dialog is described: {described}"));
    assert!(win["texts"].as_array().unwrap().iter().any(|t| t == "Grid size"), "{win}");
    assert!(win["controls"].as_array().unwrap().iter().any(|c| c["text"] == "Apply"), "{win}");
    assert_eq!(described["views"].as_array().unwrap().len(), 4);
    assert_eq!(described["accessibility_tree"], true);

    let at = |role: &str, text: &str| {
        let c = win["controls"].as_array().unwrap().iter().find(|c| c["role"] == role && c["text"] == text).cloned();
        let c = c.unwrap_or_else(|| panic!("a {role} in {win}"));
        egui::pos2(c["at"][0].as_f64().unwrap() as f32, c["at"][1].as_f64().unwrap() as f32)
    };
    let (button, field) = (at("button", "Apply"), at("text field", ""));
    call(&mut host, &mut ui, "ui_click", json!({ "x": button.x, "y": button.y }));
    assert_eq!(host.clicks, 1, "the click reached the button");
    call(&mut host, &mut ui, "ui_click", json!({ "x": field.x, "y": field.y }));
    call(&mut host, &mut ui, "ui_type", json!({ "text": "128" }));
    assert_eq!(host.text, "128", "the text reached the field");
    call(&mut host, &mut ui, "ui_key", json!({ "key": "Backspace" }));
    assert_eq!(host.text, "12");
    call(&mut host, &mut ui, "describe_ui", json!({ "console_lines": 3 }));
    let described = answers.lock().unwrap().last().unwrap().1.structured.clone().unwrap();
    assert_eq!(described["focused"]["role"], "TextInput", "{}", described["focused"]);
    assert_eq!(described["focused"]["value"], "12");

    call(&mut host, &mut ui, "screenshot_window", json!({ "max_size": 200 }));
    let (name, shot) = answers.lock().unwrap().last().cloned().unwrap();
    assert_eq!(name, "screenshot_window");
    assert!(!shot.is_error, "{}", shot.first_text());
    let v = shot.to_value("2025-06-18");
    let png = v["content"].as_array().unwrap().iter().find(|c| c["type"] == "image").expect("an image");
    assert_eq!(png["mimeType"], "image/png");

    call(&mut host, &mut ui, "wait_frames", json!({ "frames": 3 }));
    let (_, waited) = answers.lock().unwrap().last().cloned().unwrap();
    assert_eq!(waited.structured.unwrap()["frames"], 3);

    call(&mut host, &mut ui, "ui_click", json!({ "view": "top", "world": [100000, 0, 0] }));
    let (_, r) = answers.lock().unwrap().last().cloned().unwrap();
    assert!(r.is_error && r.first_text().contains("outside the top view"), "{}", r.first_text());
    call(&mut host, &mut ui, "ui_click", json!({ "x": 5000, "y": 5 }));
    let (_, r) = answers.lock().unwrap().last().cloned().unwrap();
    assert!(r.is_error && r.first_text().contains("outside the window"), "{}", r.first_text());
}

fn chrome_frame(ctx: &egui::Context, host: &mut TestHost, ui: &mut UiAutomation, sent: &mut Vec<ViewportCommand>) {
    let mut raw =
        egui::RawInput { screen_rect: Some(Rect::from_min_size(egui::Pos2::ZERO, WINDOW)), ..Default::default() };
    ui.inject(ctx, &mut raw);
    let mut out = ctx.run_ui(raw, |root| {
        let ctx = root.ctx().clone();
        ui.poll(host, &ctx);
        let native = host.state.settings.ui.native_frames;
        let spec = hp_app::chrome::ChromeSpec::main("test.vmf - Hammer Pro".to_owned(), native);
        let mut chrome = hp_app::chrome::Chrome::begin(root, spec);
        chrome.title_bar(root, |t| t.label("File"));
        chrome.body(root, |b| b.label("content"));
        chrome.end(root);
    });
    out.textures_delta.clear();
    for (_, vo) in std::mem::take(&mut out.viewport_output) {
        for cmd in vo.commands {
            match cmd {
                ViewportCommand::Screenshot(cb) => {
                    let size = [(WINDOW.x / 4.0) as usize, (WINDOW.y / 4.0) as usize];
                    cb.complete(Arc::new(egui::ColorImage::filled(size, egui::Color32::from_gray(40))));
                }
                other => sent.push(other),
            }
        }
    }
}

#[test]
fn title_bar_buttons_are_described_and_clickable() {
    hp_app::chrome::set_transparency_available(true);
    let ctx = egui::Context::default();
    let mut host = TestHost::new();
    let mut ui = UiAutomation::default();
    let mut sent = Vec::new();
    let answers: Arc<std::sync::Mutex<Vec<(String, ToolResult)>>> = Arc::default();
    let call =
        |host: &mut TestHost, ui: &mut UiAutomation, sent: &mut Vec<ViewportCommand>, name: &str, args: Value| {
            let sink = answers.clone();
            let tool = name.to_owned();
            let before = answers.lock().unwrap().len();
            let reply: Reply = Box::new(move |r| sink.lock().unwrap().push((tool, r)));
            dispatch(&ctx, host, ui, name, &args, reply);
            let deadline = Instant::now() + Duration::from_secs(20);
            while Instant::now() < deadline {
                if answers.lock().unwrap().len() > before && !ui.busy() {
                    break;
                }
                chrome_frame(&ctx, host, ui, sent);
            }
            assert!(answers.lock().unwrap().len() > before, "{name} was answered");
            chrome_frame(&ctx, host, ui, sent);
            answers.lock().unwrap().last().unwrap().1.clone()
        };
    for _ in 0..3 {
        chrome_frame(&ctx, &mut host, &mut ui, &mut sent);
    }
    let described = call(&mut host, &mut ui, &mut sent, "describe_ui", json!({})).structured.unwrap();
    let bar = &described["title_bar"];
    let names: Vec<&str> = bar["buttons"].as_array().unwrap().iter().filter_map(|b| b["name"].as_str()).collect();
    assert_eq!(names, ["minimize", "maximize", "close"], "{bar}");
    assert_eq!(bar["maximized"], false);
    assert_eq!(bar["round_corners"], true);
    let center = |name: &str| {
        let b = bar["buttons"].as_array().unwrap().iter().find(|b| b["name"] == name).unwrap();
        let r: Vec<f64> = b["rect"].as_array().unwrap().iter().map(|v| v.as_f64().unwrap()).collect();
        json!({ "x": r[0] + r[2] / 2.0, "y": r[1] + r[3] / 2.0 })
    };
    for (name, expected) in [
        ("minimize", ViewportCommand::Minimized(true)),
        ("maximize", ViewportCommand::Maximized(true)),
        ("close", ViewportCommand::Close),
    ] {
        sent.clear();
        let r = call(&mut host, &mut ui, &mut sent, "ui_click", center(name));
        assert!(!r.is_error, "{}", r.first_text());
        assert!(sent.contains(&expected), "{name}: {sent:?}");
    }
    host.state.settings.ui.native_frames = true;
    sent.clear();
    let described = call(&mut host, &mut ui, &mut sent, "describe_ui", json!({})).structured.unwrap();
    assert!(described["title_bar"].is_null(), "{}", described["title_bar"]);
}
