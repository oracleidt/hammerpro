use std::collections::VecDeque;
use std::path::PathBuf;

use egui::{Event, Key, Modifiers, PointerButton, Pos2};

#[derive(Clone, Debug, PartialEq)]
pub enum ScriptPos {
    Window(Pos2),
    ViewFraction { view: usize, x: f32, y: f32 },
    World2d { kind: hp_app::ViewKind, x: f64, y: f64 },
}

#[derive(Clone, Debug, PartialEq)]
pub enum Command {
    Wait(u32),
    Idle(u32),
    Button { pos: ScriptPos, button: PointerButton, mods: Modifiers, down: bool },
    Move(ScriptPos),
    Click { pos: ScriptPos, button: PointerButton, mods: Modifiers, double: bool },
    Drag { from: ScriptPos, to: ScriptPos, button: PointerButton, steps: u32, mods: Modifiers, hold: u32 },
    Key { key: Key, mods: Modifiers },
    KeyState { key: Option<Key>, mods: Modifiers, down: bool },
    Hold { key: Key, frames: u32 },
    Type(String),
    Scroll { pos: ScriptPos, dx: f32, dy: f32, mods: Modifiers, kind: WheelKind },
    Action(String),
    Tool(String),
    Open(PathBuf),
    Material(String),
    Background(f32, String),
    Screenshot(PathBuf),
    ScreenshotWindow { name: String, path: PathBuf },
    Log(String),
    AssertSelected(usize),
    AssertTabs(usize),
    Save(PathBuf),
    CustomFolder { path: PathBuf, prefix: Option<String> },
    Stats,
    Vram,
    History,
    Camera { pos: glam::DVec3, yaw: f64, pitch: f64 },
    Player { eye: glam::DVec3, yaw: f64, pitch: f64 },
    SelectId(u32),
    Grid(f64),
    ToolOption { key: String, value: String },
    ImportModel { path: PathBuf, confirm: bool, scale: Option<f64> },
    ImportModelsBatch(PathBuf),
    MeshToDisp { path: PathBuf, confirm: bool },
    Theme(hp_app::settings::ThemeChoice),
    Setting { path: String, value: String },
    WindowSize(f32, f32),
    WindowMaximize(bool),
    Exit,
}

fn parse_mods(s: &str) -> Modifiers {
    let mut m = Modifiers::NONE;
    for part in s.split('+') {
        match part.to_ascii_lowercase().as_str() {
            "ctrl" => {
                m.ctrl = true;
                m.command = true;
            }
            "shift" => m.shift = true,
            "alt" => m.alt = true,
            _ => {}
        }
    }
    m
}

fn parse_keys(s: &str) -> Option<(Option<Key>, Modifiers)> {
    let mut mods = Modifiers::NONE;
    let mut key = None;
    for part in s.split('+') {
        match part.to_ascii_lowercase().as_str() {
            "ctrl" => {
                mods.ctrl = true;
                mods.command = true;
            }
            "shift" => mods.shift = true,
            "alt" => mods.alt = true,
            _ => key = Some(Key::from_name(part)?),
        }
    }
    Some((key, mods))
}

fn parse_combo(s: &str) -> Option<(Key, Modifiers)> {
    let (key, mods) = parse_keys(s)?;
    key.map(|k| (k, mods))
}

fn parse_button(s: Option<&str>) -> PointerButton {
    match s {
        Some("right") => PointerButton::Secondary,
        Some("middle") => PointerButton::Middle,
        _ => PointerButton::Primary,
    }
}

fn parse_pos(s: &str) -> Option<ScriptPos> {
    if let Some(rest) = s.strip_prefix('@') {
        let (target, coords) = rest.split_once(':')?;
        let (a, b) = coords.split_once(',')?;
        if let Some(v) = target.strip_prefix('v') {
            return Some(ScriptPos::ViewFraction { view: v.parse().ok()?, x: a.parse().ok()?, y: b.parse().ok()? });
        }
        let kind = match target {
            "top" => hp_app::ViewKind::Top,
            "front" => hp_app::ViewKind::Front,
            "side" => hp_app::ViewKind::Side,
            _ => return None,
        };
        return Some(ScriptPos::World2d { kind, x: a.parse().ok()?, y: b.parse().ok()? });
    }
    let (a, b) = s.split_once(',')?;
    Some(ScriptPos::Window(Pos2::new(a.parse().ok()?, b.parse().ok()?)))
}

pub fn parse(text: &str) -> Result<Vec<Command>, String> {
    let mut out = Vec::new();
    for (n, raw) in text.lines().enumerate() {
        let line = raw.split('#').next().unwrap_or("").trim();
        if line.is_empty() {
            continue;
        }
        let err = || format!("script line {}: cannot parse `{line}`", n + 1);
        let mut parts = line.split_whitespace();
        let cmd = parts.next().unwrap_or("");
        let rest: Vec<&str> = parts.collect();
        let arg = |i: usize| rest.get(i).copied();
        let tail = line.split_once(char::is_whitespace).map(|(_, t)| t).unwrap_or("").trim().to_owned();
        let c = match cmd {
            "wait" => Command::Wait(arg(0).and_then(|v| v.parse().ok()).ok_or_else(err)?),
            "idle" => Command::Idle(arg(0).and_then(|v| v.parse().ok()).ok_or_else(err)?),
            "press" | "release" => Command::Button {
                pos: arg(0).and_then(parse_pos).ok_or_else(err)?,
                button: parse_button(arg(1)),
                mods: arg(2).map(parse_mods).unwrap_or_default(),
                down: cmd == "press",
            },
            "move" => Command::Move(arg(0).and_then(parse_pos).ok_or_else(err)?),
            "click" | "dclick" => Command::Click {
                pos: arg(0).and_then(parse_pos).ok_or_else(err)?,
                button: parse_button(arg(1)),
                mods: arg(2).map(parse_mods).unwrap_or_default(),
                double: cmd == "dclick",
            },
            "drag" => Command::Drag {
                from: arg(0).and_then(parse_pos).ok_or_else(err)?,
                to: arg(1).and_then(parse_pos).ok_or_else(err)?,
                button: parse_button(arg(2)),
                steps: arg(3).and_then(|v| v.parse().ok()).unwrap_or(8).max(1),
                mods: arg(4).map(parse_mods).unwrap_or_default(),
                hold: arg(5).and_then(|v| v.parse().ok()).unwrap_or(0),
            },
            "key" => {
                let (key, mods) = arg(0).and_then(parse_combo).ok_or_else(err)?;
                Command::Key { key, mods }
            }
            "keydown" | "keyup" => {
                let (key, mods) = arg(0).and_then(parse_keys).ok_or_else(err)?;
                Command::KeyState { key, mods, down: cmd == "keydown" }
            }
            "hold" => Command::Hold {
                key: arg(0).and_then(Key::from_name).ok_or_else(err)?,
                frames: arg(1).and_then(|v| v.parse().ok()).unwrap_or(30),
            },
            "type" => Command::Type(tail),
            "scroll" => Command::Scroll {
                pos: arg(0).and_then(parse_pos).ok_or_else(err)?,
                dx: 0.0,
                dy: arg(1).and_then(|v| v.parse().ok()).ok_or_else(err)?,
                mods: arg(2).map(parse_mods).unwrap_or_default(),
                kind: WheelKind::Wheel,
            },
            "trackpad" => Command::Scroll {
                pos: arg(0).and_then(parse_pos).ok_or_else(err)?,
                dx: arg(1).and_then(|v| v.parse().ok()).ok_or_else(err)?,
                dy: arg(2).and_then(|v| v.parse().ok()).ok_or_else(err)?,
                mods: arg(3).map(parse_mods).unwrap_or_default(),
                kind: WheelKind::Trackpad,
            },
            "pinch" => Command::Scroll {
                pos: arg(0).and_then(parse_pos).ok_or_else(err)?,
                dx: 0.0,
                dy: arg(1).and_then(|v| v.parse().ok()).filter(|f: &f32| f.is_finite() && *f > 0.0).ok_or_else(err)?,
                mods: Modifiers::NONE,
                kind: WheelKind::Pinch,
            },
            "action" => Command::Action(arg(0).ok_or_else(err)?.to_owned()),
            "tool" => Command::Tool(arg(0).ok_or_else(err)?.to_owned()),
            "open" => Command::Open(PathBuf::from(tail)),
            "material" => Command::Material(arg(0).ok_or_else(err)?.to_owned()),
            "background" => {
                let opacity = arg(0).and_then(|v| v.parse().ok()).ok_or_else(err)?;
                let path = tail.split_once(char::is_whitespace).map(|(_, p)| p.trim()).unwrap_or("").to_owned();
                Command::Background(opacity, path)
            }
            "screenshot" => Command::Screenshot(PathBuf::from(tail)),
            "screenshot_window" => {
                let name = arg(0).ok_or_else(err)?.to_owned();
                let path = tail.strip_prefix(name.as_str()).map(str::trim).filter(|p| !p.is_empty()).ok_or_else(err)?;
                Command::ScreenshotWindow { path: PathBuf::from(path), name }
            }
            "log" => Command::Log(tail),
            "assert_selected" => Command::AssertSelected(arg(0).and_then(|v| v.parse().ok()).ok_or_else(err)?),
            "assert_tabs" => Command::AssertTabs(arg(0).and_then(|v| v.parse().ok()).ok_or_else(err)?),
            "save" => Command::Save(PathBuf::from(tail)),
            "custom_folder" => {
                let (path, prefix) = match tail.split_once('|') {
                    Some((p, x)) => (p.trim().to_owned(), Some(x.trim().to_owned())),
                    None => (tail.clone(), None),
                };
                Command::CustomFolder { path: PathBuf::from(path), prefix }
            }
            "stats" => Command::Stats,
            "vram" => Command::Vram,
            "history" => Command::History,
            "camera" => {
                let n: Vec<f64> = rest.iter().filter_map(|v| v.parse().ok()).collect();
                if n.len() != 5 {
                    return Err(err());
                }
                Command::Camera { pos: glam::DVec3::new(n[0], n[1], n[2]), yaw: n[3], pitch: n[4] }
            }
            "player" => {
                let n: Vec<f64> = rest.iter().filter_map(|v| v.parse().ok()).collect();
                if n.len() != 5 {
                    return Err(err());
                }
                Command::Player { eye: glam::DVec3::new(n[0], n[1], n[2]), yaw: n[3], pitch: n[4] }
            }
            "select_id" => Command::SelectId(arg(0).and_then(|v| v.parse().ok()).ok_or_else(err)?),
            "grid" => Command::Grid(arg(0).and_then(|v| v.parse().ok()).ok_or_else(err)?),
            "tool_option" => Command::ToolOption {
                key: arg(0).ok_or_else(err)?.to_owned(),
                value: arg(1).ok_or_else(err)?.to_owned(),
            },
            "mesh_to_disp" => Command::MeshToDisp { path: PathBuf::from(tail), confirm: false },
            "mesh_to_disp_now" => Command::MeshToDisp { path: PathBuf::from(tail), confirm: true },
            "import_models_batch" => {
                if tail.is_empty() {
                    return Err(err());
                }
                Command::ImportModelsBatch(PathBuf::from(tail))
            }
            "import_model" => Command::ImportModel { path: PathBuf::from(tail), confirm: false, scale: None },
            "import_model_now" => Command::ImportModel { path: PathBuf::from(tail), confirm: true, scale: None },
            "import_model_scaled" => {
                let scale = arg(0).and_then(|v| v.parse::<f64>().ok()).filter(|v| *v > 0.0).ok_or_else(err)?;
                let path = tail.split_once(char::is_whitespace).map(|(_, p)| p.trim()).unwrap_or("");
                if path.is_empty() {
                    return Err(err());
                }
                Command::ImportModel { path: PathBuf::from(path), confirm: true, scale: Some(scale) }
            }
            "theme" => Command::Theme(match arg(0).ok_or_else(err)? {
                "dark" => hp_app::settings::ThemeChoice::Dark,
                "light" => hp_app::settings::ThemeChoice::Light,
                _ => return Err(err()),
            }),
            "setting" => {
                let path = arg(0).ok_or_else(err)?.to_owned();
                let value = tail.split_once(char::is_whitespace).map(|(_, v)| v.trim()).unwrap_or("");
                if value.is_empty() {
                    return Err(err());
                }
                Command::Setting { path, value: value.to_owned() }
            }
            "window_size" => Command::WindowSize(
                arg(0).and_then(|v| v.parse().ok()).filter(|v: &f32| *v >= 1.0).ok_or_else(err)?,
                arg(1).and_then(|v| v.parse().ok()).filter(|v: &f32| *v >= 1.0).ok_or_else(err)?,
            ),
            "window_maximize" => Command::WindowMaximize(match arg(0).ok_or_else(err)? {
                "on" => true,
                "off" => false,
                _ => return Err(err()),
            }),
            "exit" => Command::Exit,
            _ => return Err(err()),
        };
        out.push(c);
    }
    Ok(out)
}

#[derive(Clone, Debug)]
pub enum Step {
    Events(Vec<PendingEvent>),
    Idle,
    Sleep(u32),
    App(Command),
}

#[derive(Clone, Debug)]
pub enum PendingEvent {
    Move { pos: ScriptPos, mods: Modifiers },
    Button { pos: ScriptPos, button: PointerButton, pressed: bool, mods: Modifiers },
    Key { key: Key, pressed: bool, mods: Modifiers },
    Text { text: String, mods: Modifiers },
    Wheel { pos: ScriptPos, dx: f32, dy: f32, mods: Modifiers, kind: WheelKind },
    Modifiers(Modifiers),
}

#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub enum WheelKind {
    #[default]
    Wheel,
    Trackpad,
    Pinch,
}

pub fn expand(commands: Vec<Command>) -> VecDeque<Step> {
    let mut steps = VecDeque::new();
    let mut held = Modifiers::NONE;
    for c in commands {
        match c {
            Command::Wait(n) => (0..n).for_each(|_| steps.push_back(Step::Idle)),
            Command::Idle(ms) => {
                steps.push_back(Step::Sleep(ms));
                steps.push_back(Step::Idle);
            }
            Command::Button { pos, button, mods, down } => {
                let mods = mods | held;
                steps.push_back(Step::Events(vec![PendingEvent::Move { pos: pos.clone(), mods }]));
                steps.push_back(Step::Events(vec![PendingEvent::Button { pos, button, pressed: down, mods }]));
                steps.push_back(Step::Idle);
            }
            Command::Move(pos) => {
                steps.push_back(Step::Events(vec![PendingEvent::Move { pos, mods: held }]));
                steps.push_back(Step::Idle);
            }
            Command::Click { pos, button, mods, double } => {
                let times = if double { 2 } else { 1 };
                let mods = mods | held;
                steps.push_back(Step::Events(vec![PendingEvent::Move { pos: pos.clone(), mods }]));
                steps.push_back(Step::Idle);
                for _ in 0..times {
                    steps.push_back(Step::Events(vec![PendingEvent::Button {
                        pos: pos.clone(),
                        button,
                        pressed: true,
                        mods,
                    }]));
                    steps.push_back(Step::Events(vec![PendingEvent::Button {
                        pos: pos.clone(),
                        button,
                        pressed: false,
                        mods,
                    }]));
                }
                steps.push_back(Step::Idle);
                steps.push_back(Step::Idle);
            }
            Command::Drag { from, to, button, steps: n, mods, hold } => {
                let mods = mods | held;
                steps.push_back(Step::Events(vec![PendingEvent::Move { pos: from.clone(), mods }]));
                steps.push_back(Step::Idle);
                steps.push_back(Step::Events(vec![PendingEvent::Button {
                    pos: from.clone(),
                    button,
                    pressed: true,
                    mods,
                }]));
                for i in 1..=n {
                    let pos = lerp_pos(&from, &to, i as f32 / n as f32);
                    steps.push_back(Step::Events(vec![PendingEvent::Move { pos, mods }]));
                }
                (0..hold).for_each(|_| steps.push_back(Step::Idle));
                steps.push_back(Step::Idle);
                steps.push_back(Step::Events(vec![PendingEvent::Button { pos: to, button, pressed: false, mods }]));
                steps.push_back(Step::Idle);
                steps.push_back(Step::Idle);
            }
            Command::Key { key, mods } => {
                let mods = mods | held;
                steps.push_back(Step::Events(vec![PendingEvent::Key { key, pressed: true, mods }]));
                steps.push_back(Step::Events(vec![PendingEvent::Key { key, pressed: false, mods }]));
                steps.push_back(Step::Idle);
            }
            Command::KeyState { key, mods, down } => {
                held = if down { held | mods } else { remove_mods(held, mods) };
                let event = match key {
                    Some(key) => PendingEvent::Key { key, pressed: down, mods: held },
                    None => PendingEvent::Modifiers(held),
                };
                steps.push_back(Step::Events(vec![event]));
                steps.push_back(Step::Idle);
            }
            Command::Hold { key, frames } => {
                steps.push_back(Step::Events(vec![PendingEvent::Key { key, pressed: true, mods: held }]));
                (0..frames).for_each(|_| steps.push_back(Step::Idle));
                steps.push_back(Step::Events(vec![PendingEvent::Key { key, pressed: false, mods: held }]));
                steps.push_back(Step::Idle);
            }
            Command::Type(text) => {
                steps.push_back(Step::Events(vec![PendingEvent::Text { text, mods: held }]));
                steps.push_back(Step::Idle);
            }
            Command::Scroll { pos, dx, dy, mods, kind } => {
                let mods = mods | held;
                steps.push_back(Step::Events(vec![PendingEvent::Move { pos: pos.clone(), mods }]));
                steps.push_back(Step::Events(vec![PendingEvent::Wheel { pos, dx, dy, mods, kind }]));
                steps.push_back(Step::Idle);
            }
            other => {
                steps.push_back(Step::App(other));
                steps.push_back(Step::Idle);
            }
        }
    }
    steps
}

fn remove_mods(held: Modifiers, released: Modifiers) -> Modifiers {
    let ctrl = released.ctrl || released.command;
    Modifiers {
        alt: held.alt && !released.alt,
        shift: held.shift && !released.shift,
        ctrl: held.ctrl && !ctrl,
        command: held.command && !ctrl,
        mac_cmd: held.mac_cmd && !released.mac_cmd,
    }
}

fn lerp_pos(a: &ScriptPos, b: &ScriptPos, t: f32) -> ScriptPos {
    match (a, b) {
        (ScriptPos::Window(p), ScriptPos::Window(q)) => ScriptPos::Window(p.lerp(*q, t)),
        (ScriptPos::ViewFraction { view, x, y }, ScriptPos::ViewFraction { x: x2, y: y2, .. }) => {
            ScriptPos::ViewFraction { view: *view, x: x + (x2 - x) * t, y: y + (y2 - y) * t }
        }
        (ScriptPos::World2d { kind, x, y }, ScriptPos::World2d { x: x2, y: y2, .. }) => {
            ScriptPos::World2d { kind: *kind, x: x + (x2 - x) * f64::from(t), y: y + (y2 - y) * f64::from(t) }
        }
        _ => {
            if t < 1.0 {
                a.clone()
            } else {
                b.clone()
            }
        }
    }
}

pub fn to_event(ev: &PendingEvent, pos: Option<Pos2>) -> Vec<Event> {
    match ev {
        PendingEvent::Move { .. } => pos.map(|p| vec![Event::PointerMoved(p)]).unwrap_or_default(),
        PendingEvent::Button { button, pressed, mods, .. } => pos
            .map(|p| vec![Event::PointerButton { pos: p, button: *button, pressed: *pressed, modifiers: *mods }])
            .unwrap_or_default(),
        PendingEvent::Key { key, pressed, mods } => {
            let mut out = vec![Event::Key {
                key: *key,
                physical_key: Some(*key),
                pressed: *pressed,
                repeat: false,
                modifiers: *mods,
            }];
            if *pressed && mods.command && !mods.shift && !mods.alt {
                match key {
                    Key::C => out.push(Event::Copy),
                    Key::X => out.push(Event::Cut),
                    _ => {}
                }
            }
            out
        }
        PendingEvent::Text { text, .. } => vec![Event::Text(text.clone())],
        PendingEvent::Wheel { dy, mods, kind: WheelKind::Wheel, .. } => vec![Event::MouseWheel {
            unit: egui::MouseWheelUnit::Line,
            delta: egui::vec2(0.0, *dy),
            phase: egui::TouchPhase::Move,
            modifiers: *mods,
            source: egui::MouseWheelSource::Wheel,
        }],
        PendingEvent::Wheel { dx, dy, mods, kind: WheelKind::Trackpad, .. } => vec![Event::MouseWheel {
            unit: egui::MouseWheelUnit::Point,
            delta: egui::vec2(*dx, *dy),
            phase: egui::TouchPhase::Move,
            modifiers: *mods,
            source: egui::MouseWheelSource::Trackpad,
        }],
        PendingEvent::Wheel { dy, kind: WheelKind::Pinch, .. } => vec![Event::Zoom(*dy)],
        PendingEvent::Modifiers(_) => Vec::new(),
    }
}

pub fn event_pos(ev: &PendingEvent) -> Option<&ScriptPos> {
    match ev {
        PendingEvent::Move { pos: p, .. }
        | PendingEvent::Button { pos: p, .. }
        | PendingEvent::Wheel { pos: p, .. } => Some(p),
        PendingEvent::Key { .. } | PendingEvent::Text { .. } | PendingEvent::Modifiers(_) => None,
    }
}

pub fn event_mods(ev: &PendingEvent) -> Modifiers {
    match ev {
        PendingEvent::Move { mods, .. }
        | PendingEvent::Button { mods, .. }
        | PendingEvent::Key { mods, .. }
        | PendingEvent::Text { mods, .. }
        | PendingEvent::Wheel { mods, .. }
        | PendingEvent::Modifiers(mods) => *mods,
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn parses_commands() {
        let script = "# comment\nwait 3\nclick @v1:0.5,0.25 right\ndrag @top:0,0 @top:128,64 left 4 shift\nkey Ctrl+Shift+S\naction run_map\nscreenshot out.png\nexit\n";
        let cmds = parse(script).unwrap();
        assert_eq!(cmds.len(), 7);
        assert!(matches!(cmds[1], Command::Click { button: PointerButton::Secondary, .. }));
        match &cmds[2] {
            Command::Drag { mods, steps, hold, .. } => {
                assert!(mods.shift);
                assert_eq!(*steps, 4);
                assert_eq!(*hold, 0);
            }
            other => panic!("{other:?}"),
        }
        let held = parse("drag @top:0,0 @top:900,0 left 4 - 30\n").unwrap();
        assert!(matches!(held[0], Command::Drag { hold: 30, steps: 4, .. }));
        let steps = expand(held);
        let idles = steps.iter().filter(|s| matches!(s, Step::Idle)).count();
        assert!(idles >= 30, "{idles}");
        match &cmds[3] {
            Command::Key { key, mods } => {
                assert_eq!(*key, Key::S);
                assert!(mods.ctrl && mods.shift);
            }
            other => panic!("{other:?}"),
        }
    }

    #[test]
    fn parses_player_eyes() {
        let cmds = parse("player -480 -300 64 35 -8\n").unwrap();
        match &cmds[0] {
            Command::Player { eye, yaw, pitch } => {
                assert_eq!(*eye, glam::DVec3::new(-480.0, -300.0, 64.0));
                assert_eq!((*yaw, *pitch), (35.0, -8.0));
            }
            other => panic!("{other:?}"),
        }
        assert!(parse("player 1 2 3\n").is_err());
    }

    #[test]
    fn parses_tool_options() {
        let cmds = parse("tool_option kind arch\ntool_option start 45").unwrap();
        assert_eq!(cmds[0], Command::ToolOption { key: "kind".to_owned(), value: "arch".to_owned() });
        assert_eq!(cmds[1], Command::ToolOption { key: "start".to_owned(), value: "45".to_owned() });
        assert!(parse("tool_option sides").is_err());
    }

    #[test]
    fn parses_model_imports() {
        let cmds = parse(
            "import_model C:/art/My Chair.fbx
import_model_now models/box.obj
import_model_scaled 0.01 a b.glb
",
        )
        .unwrap();
        let path = |p: &str| PathBuf::from(p);
        assert_eq!(cmds[0], Command::ImportModel { path: path("C:/art/My Chair.fbx"), confirm: false, scale: None });
        assert_eq!(cmds[1], Command::ImportModel { path: path("models/box.obj"), confirm: true, scale: None });
        assert_eq!(cmds[2], Command::ImportModel { path: path("a b.glb"), confirm: true, scale: Some(0.01) });
        assert!(parse("import_model_scaled 0 x.obj").is_err());
        assert_eq!(
            parse("import_models_batch C:/art/props").unwrap()[0],
            Command::ImportModelsBatch(path("C:/art/props"))
        );
        assert!(parse("import_models_batch").is_err());
    }

    #[test]
    fn parses_mesh_to_displacement() {
        let cmds = parse(
            "mesh_to_disp C:/art/hill.obj
mesh_to_disp_now a b.glb
",
        )
        .unwrap();
        assert_eq!(cmds[0], Command::MeshToDisp { path: PathBuf::from("C:/art/hill.obj"), confirm: false });
        assert_eq!(cmds[1], Command::MeshToDisp { path: PathBuf::from("a b.glb"), confirm: true });
    }

    #[test]
    fn parses_press_release_and_idle() {
        let cmds = parse(
            "press @v0:0.5,0.5
idle 500
release @v0:0.5,0.5 left shift",
        )
        .unwrap();
        assert!(matches!(cmds[0], Command::Button { down: true, button: PointerButton::Primary, .. }));
        assert_eq!(cmds[1], Command::Idle(500));
        assert!(matches!(cmds[2], Command::Button { down: false, mods, .. } if mods.shift));
        let steps = expand(cmds);
        assert!(steps.iter().any(|s| matches!(s, Step::Sleep(500))));
        let presses = steps
            .iter()
            .filter_map(|s| match s {
                Step::Events(evs) => Some(evs.clone()),
                _ => None,
            })
            .flatten()
            .filter_map(|e| match e {
                PendingEvent::Button { pressed, .. } => Some(pressed),
                _ => None,
            })
            .collect::<Vec<_>>();
        assert_eq!(presses, vec![true, false], "one press, one release");
        assert!(parse("idle soon").is_err());
        assert!(parse("press").is_err());
    }

    #[test]
    fn parses_themes() {
        let cmds = parse("theme light\ntheme dark").unwrap();
        assert_eq!(cmds[0], Command::Theme(hp_app::settings::ThemeChoice::Light));
        assert_eq!(cmds[1], Command::Theme(hp_app::settings::ThemeChoice::Dark));
        assert!(parse("theme pink").is_err());
        assert!(parse("theme").is_err());
    }

    #[test]
    fn parses_settings() {
        let cmds = parse(
            "setting render.normal_maps false
setting view.fov_deg 90",
        )
        .unwrap();
        assert_eq!(cmds[0], Command::Setting { path: "render.normal_maps".into(), value: "false".into() });
        assert_eq!(cmds[1], Command::Setting { path: "view.fov_deg".into(), value: "90".into() });
        assert!(parse("setting render.normal_maps").is_err());
    }

    #[test]
    fn parses_window_commands() {
        let cmds = parse("window_size 320 200\nwindow_maximize on\nwindow_maximize off").unwrap();
        assert_eq!(
            cmds,
            vec![Command::WindowSize(320.0, 200.0), Command::WindowMaximize(true), Command::WindowMaximize(false)]
        );
        assert!(parse("window_size 0 10").is_err());
        assert!(parse("window_size 10").is_err());
        assert!(parse("window_maximize maybe").is_err());
    }

    #[test]
    fn rejects_garbage() {
        assert!(parse("frobnicate 1").is_err());
        assert!(parse("click nowhere").is_err());
    }

    #[test]
    fn expands_drag() {
        let steps = expand(parse("drag 0,0 10,10 left 5").unwrap());
        assert!(steps.len() >= 9);
    }

    fn all_mods(steps: &VecDeque<Step>) -> Vec<Modifiers> {
        steps
            .iter()
            .filter_map(|s| match s {
                Step::Events(evs) => Some(evs.iter().map(event_mods).collect::<Vec<_>>()),
                _ => None,
            })
            .flatten()
            .collect()
    }

    #[test]
    fn drags_and_scrolls_keep_their_modifiers() {
        let steps = expand(parse("drag @top:20,20 @top:84,20 left 8 shift").unwrap());
        let mods = all_mods(&steps);
        assert!(mods.len() >= 10);
        assert!(mods.iter().all(|m| m.shift), "{mods:?}");
        let steps = expand(parse("scroll 10,10 1 ctrl").unwrap());
        let wheel = steps.iter().find_map(|s| match s {
            Step::Events(evs) => evs.iter().find(|e| matches!(e, PendingEvent::Wheel { .. })).cloned(),
            _ => None,
        });
        let events = to_event(&wheel.unwrap(), Some(Pos2::ZERO));
        assert!(matches!(events[0], Event::MouseWheel { modifiers, .. } if modifiers.ctrl));
    }

    #[test]
    fn trackpad_scrolls_and_pinches() {
        let wheel_of = |text: &str| {
            let steps = expand(parse(text).unwrap());
            let wheel = steps.iter().find_map(|s| match s {
                Step::Events(evs) => evs.iter().find(|e| matches!(e, PendingEvent::Wheel { .. })).cloned(),
                _ => None,
            });
            to_event(&wheel.unwrap(), Some(Pos2::ZERO))
        };
        let events = wheel_of("trackpad 10,10 3 -4 alt");
        assert!(matches!(
            events[0],
            Event::MouseWheel {
                unit: egui::MouseWheelUnit::Point,
                source: egui::MouseWheelSource::Trackpad,
                delta,
                modifiers,
                ..
            } if delta == egui::vec2(3.0, -4.0) && modifiers.alt
        ));
        assert!(matches!(wheel_of("pinch 10,10 1.25")[0], Event::Zoom(f) if (f - 1.25).abs() < 1e-6));
        assert!(parse("pinch 10,10 0").is_err());
        assert!(parse("pinch 10,10 -2").is_err());
    }

    #[test]
    fn keydown_holds_keys_across_commands() {
        let steps = expand(parse("keydown Shift\nmove 1,1\nclick 2,2\nkeyup Shift\nmove 3,3").unwrap());
        let mods = all_mods(&steps);
        let (held, released) = mods.split_at(mods.len() - 2);
        assert!(held.iter().all(|m| m.shift), "{mods:?}");
        assert!(released.iter().all(|m| !m.shift), "{mods:?}");
        let steps = expand(parse("keydown Space\ndrag 0,0 10,10\nkeyup Space").unwrap());
        let keys: Vec<(Key, bool)> = steps
            .iter()
            .filter_map(|s| match s {
                Step::Events(evs) => Some(evs.clone()),
                _ => None,
            })
            .flatten()
            .filter_map(|e| match e {
                PendingEvent::Key { key, pressed, .. } => Some((key, pressed)),
                _ => None,
            })
            .collect();
        assert_eq!(keys, vec![(Key::Space, true), (Key::Space, false)]);
        assert!(parse("keydown Nonsense").is_err());
    }
}
