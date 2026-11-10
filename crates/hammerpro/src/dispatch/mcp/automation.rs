use std::collections::VecDeque;
use std::sync::{Arc, Mutex, PoisonError};
use std::time::{Duration, Instant};

use egui::{Key, Modifiers, PointerButton, Pos2, Rect};
use glam::DVec3;
use hp_app::{Action, AppState, ViewKind};
use hp_mcp::protocol::ToolResult;
use serde_json::{Value, json};

use super::describe;
use crate::script::{self, Command, ScriptPos, Step};

pub(crate) type Reply = Box<dyn FnOnce(ToolResult) + Send>;

const SETTLE_FRAMES: u64 = 1;
const DEADLINE: Duration = Duration::from_secs(60);
const MAX_QUEUED_STEPS: usize = 4096;
const MAX_WAITING: usize = 64;

pub(crate) trait UiHost {
    fn state(&mut self) -> &mut AppState;
    fn execute(&mut self, ctx: &egui::Context, action: Action);
    fn view_rects(&self) -> [Option<(ViewKind, Rect)>; 4];
    fn logical_rects(&self) -> [Option<Rect>; 4];
    fn open_maps(&self) -> Vec<Value>;
    fn prompts(&self) -> Vec<&'static str>;
}

#[derive(Clone, Copy, Debug, PartialEq)]
pub(crate) enum ViewSel {
    Index(usize),
    Kind(ViewKind),
    Logical,
}

#[derive(Clone, Copy, Debug, PartialEq)]
pub(crate) enum InView {
    Fraction(f32, f32),
    Points(f32, f32),
    World(DVec3),
}

#[derive(Clone, Copy, Debug, PartialEq)]
pub(crate) enum Target {
    Window(Pos2),
    View { view: ViewSel, at: InView },
}

#[derive(Clone, Copy, Debug, PartialEq)]
pub(crate) enum PointerKind {
    Move,
    Click { button: PointerButton, double: bool },
    Drag { to: Target, button: PointerButton, steps: u32 },
    Scroll { dy: f32 },
}

#[derive(Clone, Debug, PartialEq)]
pub(crate) enum UiOp {
    RunAction(Action),
    Pointer { tool: &'static str, kind: PointerKind, at: Target, mods: Modifiers },
    Key { key: Key, mods: Modifiers, hold: u32 },
    Type(String),
    WaitFrames(u32),
    Screenshot { max_size: u32, region: Option<Rect>, settle_ms: u32, window: Option<&'static str> },
    Describe { console: usize },
    Views,
    Settings { settings: Box<hp_app::settings::Settings>, path: String, value: Value },
}

type ImageSlot = Arc<Mutex<Option<Arc<egui::ColorImage>>>>;

enum Wait {
    Frames { until: u64, result: Value },
    Input { until_step: u64, result: Value },
    ShotLater { at: Instant, frame: u64, max_size: u32, region: Option<Rect>, window: Option<&'static str> },
    Screenshot { image: ImageSlot, max_size: u32, region: Option<Rect>, pixels_per_point: f32 },
    Describe { console: usize },
}

struct Waiting {
    tool: &'static str,
    reply: Reply,
    wait: Wait,
    deadline: Instant,
}

#[derive(Default)]
pub(crate) struct UiAutomation {
    queue: VecDeque<Step>,
    queued: u64,
    injected: u64,
    frame: u64,
    waiting: Vec<Waiting>,
}

impl UiAutomation {
    pub(crate) fn busy(&self) -> bool {
        !self.waiting.is_empty() || !self.queue.is_empty()
    }

    pub(crate) fn start(&mut self, host: &mut dyn UiHost, ctx: &egui::Context, op: UiOp, reply: Reply) {
        if self.waiting.len() >= MAX_WAITING {
            reply(ToolResult::error("Too many calls are waiting for Hammer Pro; wait for their answers first."));
            return;
        }
        let deadline = Instant::now() + DEADLINE;
        match op {
            UiOp::RunAction(action) => reply(run_action(host, ctx, action)),
            UiOp::Views => {
                let (rects, logical) = (host.view_rects(), host.logical_rects());
                reply(ToolResult::json(super::tools_views::views_json(host.state(), rects, logical)));
            }
            UiOp::Settings { settings, path, value } => {
                reply(super::tools_settings::apply(host.state(), ctx, *settings, &path, value));
            }
            UiOp::WaitFrames(n) => {
                let result = json!({ "frames": n });
                self.wait("wait_frames", reply, Wait::Frames { until: self.frame + u64::from(n), result }, deadline);
            }
            UiOp::Pointer { tool, kind, at, mods } => match pointer_commands(host, ctx, kind, at, mods) {
                Ok((commands, result)) => self.queue_input(tool, commands, result, reply, deadline),
                Err(e) => reply(ToolResult::error(format!("{tool}: {e}"))),
            },
            UiOp::Key { key, mods, hold } => {
                let commands = key_commands(key, mods, hold);
                let combo = hp_app::actions::format_shortcut(&egui::KeyboardShortcut::new(mods, key));
                self.queue_input("ui_key", commands, json!({ "key": combo, "hold_frames": hold }), reply, deadline);
            }
            UiOp::Type(text) => {
                let result = json!({ "typed_chars": text.chars().count() });
                self.queue_input("ui_type", vec![Command::Type(text)], result, reply, deadline);
            }
            UiOp::Screenshot { max_size, region, settle_ms, window } => {
                let at = Instant::now() + Duration::from_millis(u64::from(settle_ms));
                let wait = Wait::ShotLater { at, frame: self.frame + 2, max_size, region, window };
                self.wait("screenshot_window", reply, wait, deadline);
            }
            UiOp::Describe { console } => {
                describe::request_tree(ctx);
                self.wait("describe_ui", reply, Wait::Describe { console }, deadline);
            }
        }
        ctx.request_repaint();
    }

    fn wait(&mut self, tool: &'static str, reply: Reply, wait: Wait, deadline: Instant) {
        self.waiting.push(Waiting { tool, reply, wait, deadline });
    }

    fn queue_input(
        &mut self,
        tool: &'static str,
        commands: Vec<Command>,
        result: Value,
        reply: Reply,
        deadline: Instant,
    ) {
        let steps = script::expand(commands);
        if self.queue.len() + steps.len() > MAX_QUEUED_STEPS {
            reply(ToolResult::error(format!("{tool}: too much input is waiting; wait for the previous calls")));
            return;
        }
        self.queued += steps.len() as u64;
        self.queue.extend(steps);
        self.wait(tool, reply, Wait::Input { until_step: self.queued, result }, deadline);
    }

    pub(crate) fn inject(&mut self, ctx: &egui::Context, raw: &mut egui::RawInput) {
        let Some(step) = self.queue.pop_front() else { return };
        self.injected += 1;
        if let Step::Events(events) = step {
            for ev in &events {
                let pos = match script::event_pos(ev) {
                    Some(ScriptPos::Window(p)) => Some(*p),
                    _ => None,
                };
                raw.events.push(egui::Event::ModifiersChanged(script::event_mods(ev)));
                raw.events.extend(script::to_event(ev, pos));
            }
        }
        ctx.request_repaint();
    }

    pub(crate) fn poll(&mut self, host: &mut dyn UiHost, ctx: &egui::Context) {
        self.frame += 1;
        if !self.busy() {
            return;
        }
        ctx.request_repaint();
        let now = Instant::now();
        let mut i = 0;
        while i < self.waiting.len() {
            let w = &mut self.waiting[i];
            let done = match &mut w.wait {
                Wait::Frames { until, .. } => self.frame >= *until,
                Wait::Input { until_step, result } => {
                    if self.injected >= *until_step {
                        let result = std::mem::take(result);
                        w.wait = Wait::Frames { until: self.frame + SETTLE_FRAMES, result };
                    }
                    false
                }
                Wait::ShotLater { at, frame, max_size, region, window } => {
                    let ready = now >= *at && self.frame >= *frame;
                    let (separate, max_size, region) = (window.is_some(), *max_size, *region);
                    if ready && !separate {
                        w.wait = request_screenshot(ctx, max_size, region);
                    }
                    ready && separate
                }
                Wait::Screenshot { image, .. } => image.lock().unwrap_or_else(PoisonError::into_inner).is_some(),
                Wait::Describe { .. } => describe::tree_ready(ctx),
            };
            if done {
                let w = self.waiting.swap_remove(i);
                finish(host, ctx, w);
            } else if now >= w.deadline {
                let w = self.waiting.swap_remove(i);
                if matches!(w.wait, Wait::Input { .. }) {
                    self.injected += self.queue.len() as u64;
                    self.queue.clear();
                }
                if matches!(w.wait, Wait::Describe { .. }) {
                    describe::cancel(ctx);
                }
                (w.reply)(ToolResult::error(format!(
                    "{}: Hammer Pro did not finish in time (is the window minimized?)",
                    w.tool
                )));
            } else {
                i += 1;
            }
        }
    }
}

fn request_screenshot(ctx: &egui::Context, max_size: u32, region: Option<Rect>) -> Wait {
    let image: ImageSlot = Arc::default();
    let slot = image.clone();
    let waker = ctx.clone();
    ctx.request_screenshot(move |shot| {
        *slot.lock().unwrap_or_else(PoisonError::into_inner) = Some(shot);
        waker.request_repaint();
    });
    Wait::Screenshot { image, max_size, region, pixels_per_point: ctx.pixels_per_point() }
}

fn finish(host: &mut dyn UiHost, ctx: &egui::Context, w: Waiting) {
    match w.wait {
        Wait::Frames { mut result, .. } | Wait::Input { mut result, .. } => {
            if let Value::Object(m) = &mut result {
                m.insert("after".to_owned(), editor_summary(host.state()));
            }
            (w.reply)(ToolResult::json(result));
        }
        Wait::Screenshot { image, max_size, region, pixels_per_point } => {
            let Some(shot) = image.lock().unwrap_or_else(PoisonError::into_inner).take() else {
                (w.reply)(ToolResult::error("screenshot_window: no image"));
                return;
            };
            let mut label = "Hammer Pro window".to_owned();
            for name in hp_app::native_window::separate_windows() {
                label.push_str(&format!(
                    " (the {} is open as a separate window, not in this image: screenshot_window with window \
                     \"{name}\" shows it)",
                    separate_window_title(name)
                ));
            }
            encode_on_worker(shot, max_size, region, pixels_per_point, label, w.reply);
        }
        Wait::ShotLater { window: Some(name), max_size, region, .. } => {
            separate_window_shot(host.state(), ctx, name, max_size, region, w.reply);
        }
        Wait::ShotLater { .. } => (w.reply)(ToolResult::error("screenshot_window: not taken")),
        Wait::Describe { console } => {
            let tree = describe::take_tree(ctx);
            let value = describe::describe(host, ctx, tree.as_ref(), console);
            (w.reply)(ToolResult::json(value));
        }
    }
}

fn editor_summary(state: &AppState) -> Value {
    json!({
        "tool": format!("{:?}", state.tool).to_ascii_lowercase(),
        "selected": state.doc.selected_objects().len(),
        "undo": state.doc.undo_label(),
        "modified": state.doc.is_dirty(),
    })
}

fn run_action(host: &mut dyn UiHost, ctx: &egui::Context, action: Action) -> ToolResult {
    let state = host.state();
    if state.doc.in_transaction() {
        return ToolResult::error("run_action: Hammer Pro is in the middle of another edit; try again");
    }
    let before = state.console.revision;
    let history = matches!(action, Action::Undo | Action::Redo);
    if !history {
        state.doc.begin(super::tools::undo_label("run_action"));
    }
    host.execute(ctx, action);
    let state = host.state();
    let recorded = !history && state.doc.in_transaction() && state.doc.commit();
    let new = (state.console.revision.saturating_sub(before) as usize).min(state.console.len());
    let messages: Vec<&str> = state.console.lines().rev().take(new.min(50)).rev().map(|l| l.text.as_str()).collect();
    let checked = match action.check(state) {
        hp_app::actions::Check::None => Value::Null,
        hp_app::actions::Check::Toggle(on) | hp_app::actions::Check::Radio(on) => Value::Bool(on),
    };
    ToolResult::json(json!({
        "ran": action.id(),
        "undo_step": recorded,
        "checked": checked,
        "messages": messages,
        "after": editor_summary(state),
    }))
}

pub(crate) fn resolve(host: &mut dyn UiHost, ctx: &egui::Context, target: Target) -> Result<Pos2, String> {
    let window = ctx.viewport_rect();
    let p = match target {
        Target::Window(p) => p,
        Target::View { view, at } => {
            let rects = host.view_rects();
            let logical = host.logical_rects();
            let (index, kind, rect) = match view {
                ViewSel::Index(i) => match (rects.get(i).copied().flatten(), logical.get(i).copied().flatten()) {
                    (Some((kind, rect)), _) => (i, Some(kind), rect),
                    (None, Some(rect)) => (i, None, rect),
                    (None, None) => return Err(format!("view {i} is not shown (another view is maximized?)")),
                },
                ViewSel::Kind(k) => rects
                    .iter()
                    .enumerate()
                    .find_map(|(i, r)| r.filter(|(kind, _)| *kind == k).map(|(kind, rect)| (i, Some(kind), rect)))
                    .ok_or_else(|| format!("no {} view is shown", view_name(k)))?,
                ViewSel::Logical => logical
                    .iter()
                    .enumerate()
                    .find_map(|(i, r)| r.map(|rect| (i, None, rect)))
                    .ok_or("no logical view is shown (set_view_layout with a \"logical\" quadrant)")?,
            };
            let name = kind.map_or("logical", view_name);
            let p = match at {
                InView::Fraction(x, y) => egui::pos2(rect.left() + rect.width() * x, rect.top() + rect.height() * y),
                InView::Points(x, y) => rect.min + egui::vec2(x, y),
                InView::World(w) => {
                    let Some(kind) = kind else {
                        return Err("the logical view has no world positions: give fx and fy, or x and y".to_owned());
                    };
                    let state = host.state();
                    let info = hp_app::ViewInfo {
                        index,
                        kind,
                        rect,
                        cam3d: state.cameras.cam3d,
                        cam2d: state.cameras.cam2d(kind),
                        pixels_per_point: ctx.pixels_per_point(),
                        grid: state.grid_size(),
                    };
                    info.world_to_screen(w)
                        .ok_or_else(|| format!("the point is behind the camera of the {} view", view_name(kind)))?
                }
            };
            if !rect.contains(p) {
                return Err(format!(
                    "the point is outside the {} view ({:.0}, {:.0} is not in {})",
                    name,
                    p.x,
                    p.y,
                    rect_text(rect)
                ));
            }
            p
        }
    };
    if !p.x.is_finite() || !p.y.is_finite() || !window.contains(p) {
        return Err(format!("{:.0}, {:.0} is outside the window ({})", p.x, p.y, rect_text(window)));
    }
    Ok(p)
}

fn rect_text(r: Rect) -> String {
    format!("x {:.0} to {:.0}, y {:.0} to {:.0}", r.left(), r.right(), r.top(), r.bottom())
}

pub(crate) fn view_name(kind: ViewKind) -> &'static str {
    match kind {
        ViewKind::Perspective => "3d",
        ViewKind::Top => "top",
        ViewKind::Front => "front",
        ViewKind::Side => "side",
    }
}

pub(crate) fn quadrant_view_name(view: hp_app::QuadrantView) -> &'static str {
    view.kind().map_or("logical", view_name)
}

fn pointer_commands(
    host: &mut dyn UiHost,
    ctx: &egui::Context,
    kind: PointerKind,
    at: Target,
    mods: Modifiers,
) -> Result<(Vec<Command>, Value), String> {
    let p = resolve(host, ctx, at)?;
    let pos = ScriptPos::Window(p);
    let at_json = json!([p.x, p.y]);
    Ok(match kind {
        PointerKind::Move => (vec![Command::Move(pos)], json!({ "moved_to": at_json })),
        PointerKind::Click { button, double } => {
            (vec![Command::Click { pos, button, mods, double }], json!({ "clicked_at": at_json, "double": double }))
        }
        PointerKind::Drag { to, button, steps } => {
            let q = resolve(host, ctx, to)?;
            (
                vec![Command::Drag { from: pos, to: ScriptPos::Window(q), button, steps, mods }],
                json!({ "dragged_from": at_json, "dragged_to": [q.x, q.y] }),
            )
        }
        PointerKind::Scroll { dy } => {
            (vec![Command::Scroll { pos, dy, mods }], json!({ "scrolled_at": at_json, "dy": dy }))
        }
    })
}

fn key_commands(key: Key, mods: Modifiers, hold: u32) -> Vec<Command> {
    if hold == 0 {
        return vec![Command::Key { key, mods }];
    }
    let mut out = Vec::new();
    let any_mod = mods.any();
    if any_mod {
        out.push(Command::KeyState { key: None, mods, down: true });
    }
    out.push(Command::Hold { key, frames: hold });
    if any_mod {
        out.push(Command::KeyState { key: None, mods, down: false });
    }
    out
}

fn separate_window_shot(
    state: &AppState,
    ctx: &egui::Context,
    name: &str,
    max_size: u32,
    region: Option<Rect>,
    reply: Reply,
) {
    match hp_app::native_window::capture_window(name) {
        Some(Ok(shot)) => {
            let id = hp_app::native_window::viewport_of(name);
            let width =
                ctx.input(|i| id.and_then(|id| i.raw.viewports.get(&id)).and_then(|v| v.inner_rect).map(|r| r.width()));
            let ppp = width.filter(|w| *w > 0.0).map_or(ctx.pixels_per_point(), |w| shot.size[0] as f32 / w);
            let label = format!("{} (separate window)", separate_window_title(name));
            encode_on_worker(Arc::new(shot), max_size, region, ppp, label, reply);
        }
        Some(Err(e)) => reply(ToolResult::error(format!("screenshot_window: {e}"))),
        None if name != hp_ui_mapping::native_window::COMPILE_WINDOW => reply(ToolResult::error(format!(
            "screenshot_window: the {} is not open as a separate window (describe_ui lists the separate windows)",
            separate_window_title(name)
        ))),
        None if state.ui.show_compile => reply(ToolResult::error(
            "screenshot_window: the Compile and run window is inside the main window (compile.separate_window is \
             off): take the main window",
        )),
        None => reply(ToolResult::error(
            "screenshot_window: the Compile and run window is not open (run_action run_map opens it; \
             get_compile_status and get_compile_log read the compile without it)",
        )),
    }
}

pub(crate) fn separate_window_title(name: &str) -> &'static str {
    match name {
        "compile" => "Compile and run window",
        "uv_editor" => "UV editor",
        "portals" => "portals window",
        _ => "window",
    }
}

fn encode_on_worker(
    shot: Arc<egui::ColorImage>,
    max_size: u32,
    region: Option<Rect>,
    ppp: f32,
    label: String,
    reply: Reply,
) {
    let reply = Arc::new(Mutex::new(Some(reply)));
    let worker_reply = reply.clone();
    let spawned = std::thread::Builder::new().name("hp-mcp-screenshot".into()).spawn(move || {
        let result = encode_png(&shot, max_size, region, ppp)
            .map(|(png, w, h)| ToolResult::png(&png, format!("{label}, {w} x {h} pixels")))
            .unwrap_or_else(|e| ToolResult::error(format!("screenshot_window: {e}")));
        if let Some(r) = worker_reply.lock().unwrap_or_else(PoisonError::into_inner).take() {
            r(result);
        }
    });
    if let Err(e) = spawned
        && let Some(r) = reply.lock().unwrap_or_else(PoisonError::into_inner).take()
    {
        r(ToolResult::error(format!("screenshot_window: {e}")));
    }
}

pub(crate) fn encode_png(
    shot: &egui::ColorImage,
    max_size: u32,
    region: Option<Rect>,
    ppp: f32,
) -> Result<(Vec<u8>, u32, u32), String> {
    let [w, h] = shot.size;
    if w == 0 || h == 0 || shot.pixels.len() != w * h {
        return Err("the window image is empty".to_owned());
    }
    let mut rgba = Vec::with_capacity(w * h * 4);
    for c in &shot.pixels {
        rgba.extend_from_slice(&c.to_array());
    }
    let full = image::RgbaImage::from_raw(w as u32, h as u32, rgba).ok_or("bad image size")?;
    let mut img = match region {
        None => full,
        Some(r) => {
            let ppp = if ppp.is_finite() && ppp > 0.0 { ppp } else { 1.0 };
            let x0 = (r.left() * ppp).floor().clamp(0.0, w as f32) as u32;
            let y0 = (r.top() * ppp).floor().clamp(0.0, h as f32) as u32;
            let x1 = (r.right() * ppp).ceil().clamp(0.0, w as f32) as u32;
            let y1 = (r.bottom() * ppp).ceil().clamp(0.0, h as f32) as u32;
            if x1 <= x0 || y1 <= y0 {
                return Err(format!("the region is outside the window ({w} x {h} pixels)"));
            }
            image::imageops::crop_imm(&full, x0, y0, x1 - x0, y1 - y0).to_image()
        }
    };
    let longest = img.width().max(img.height());
    if longest > max_size {
        let scale = f64::from(max_size) / f64::from(longest);
        let nw = ((f64::from(img.width()) * scale).round() as u32).max(1);
        let nh = ((f64::from(img.height()) * scale).round() as u32).max(1);
        img = image::imageops::resize(&img, nw, nh, image::imageops::FilterType::Triangle);
    }
    let (iw, ih) = img.dimensions();
    let mut png = Vec::new();
    image::DynamicImage::ImageRgba8(img)
        .write_to(&mut std::io::Cursor::new(&mut png), image::ImageFormat::Png)
        .map_err(|e| format!("PNG encoding failed: {e}"))?;
    Ok((png, iw, ih))
}

impl UiHost for crate::app::HammerProApp {
    fn state(&mut self) -> &mut AppState {
        &mut self.state
    }

    fn execute(&mut self, ctx: &egui::Context, action: Action) {
        crate::app::HammerProApp::execute(self, ctx, action);
    }

    fn view_rects(&self) -> [Option<(ViewKind, Rect)>; 4] {
        self.viewports.view_rects()
    }

    fn logical_rects(&self) -> [Option<Rect>; 4] {
        self.viewports.logical_rects()
    }

    fn open_maps(&self) -> Vec<Value> {
        let active = self.tabs.active_id();
        self.tabs
            .infos(&self.state)
            .into_iter()
            .map(|t| {
                json!({
                    "title": t.title,
                    "file": t.path.map(|p| p.display().to_string()),
                    "modified": t.dirty,
                    "active": t.id == active,
                })
            })
            .collect()
    }

    fn prompts(&self) -> Vec<&'static str> {
        let mut out = Vec::new();
        if self.pending.is_some() || self.unsaved_asked.is_some() {
            out.push("unsaved changes: save, discard or cancel");
        }
        if self.hollow_prompt.is_some() {
            out.push("hollow: wall thickness");
        }
        if self.recover_prompt.is_some() {
            out.push("recover an autosave");
        }
        if self.crash_prompt.is_some() {
            out.push("crash report");
        }
        if self.compile_exit_prompt {
            out.push("a compile is running: exit anyway");
        }
        out
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn screenshots_are_cropped_and_scaled() {
        let shot = egui::ColorImage::filled([400, 200], egui::Color32::from_rgb(10, 20, 30));
        let (png, w, h) = encode_png(&shot, 100, None, 1.0).unwrap();
        assert_eq!((w, h), (100, 50));
        assert!(png.starts_with(b"\x89PNG"));
        let decoded = image::load_from_memory(&png).unwrap().to_rgba8();
        assert_eq!(decoded.get_pixel(10, 10).0, [10, 20, 30, 255]);
        let region = Rect::from_min_size(egui::pos2(10.0, 10.0), egui::vec2(50.0, 20.0));
        let (_, w, h) = encode_png(&shot, 4096, Some(region), 2.0).unwrap();
        assert_eq!((w, h), (100, 40));
        let region = Rect::from_min_size(egui::pos2(150.0, 90.0), egui::vec2(500.0, 500.0));
        assert_eq!(encode_png(&shot, 4096, Some(region), 1.0).unwrap().1, 250);
        let outside = Rect::from_min_size(egui::pos2(1000.0, 1000.0), egui::vec2(10.0, 10.0));
        assert!(encode_png(&shot, 4096, Some(outside), 1.0).unwrap_err().contains("outside"));
        assert!(encode_png(&shot, 4096, Some(region), f32::NAN).is_ok());
        let empty = egui::ColorImage::filled([0, 0], egui::Color32::BLACK);
        assert!(encode_png(&empty, 100, None, 1.0).is_err());
    }

    #[test]
    fn keys_with_modifiers_are_held_around_a_hold() {
        let c = key_commands(Key::W, Modifiers::SHIFT, 10);
        assert_eq!(c.len(), 3);
        assert!(matches!(c[1], Command::Hold { key: Key::W, frames: 10 }));
        assert_eq!(key_commands(Key::A, Modifiers::NONE, 0), vec![Command::Key { key: Key::A, mods: Modifiers::NONE }]);
        assert_eq!(key_commands(Key::A, Modifiers::NONE, 5).len(), 1);
    }

    #[test]
    fn too_many_waiting_calls_are_refused() {
        let ctx = egui::Context::default();
        let mut host = super::super::harness::TestHost::new();
        let mut ui = UiAutomation::default();
        let answers: Arc<Mutex<Vec<ToolResult>>> = Arc::default();
        for _ in 0..MAX_WAITING + 3 {
            let sink = answers.clone();
            ui.start(
                &mut host,
                &ctx,
                UiOp::WaitFrames(MAX_FRAMES_TEST),
                Box::new(move |r| sink.lock().unwrap().push(r)),
            );
        }
        let got = answers.lock().unwrap();
        assert_eq!(got.len(), 3);
        assert!(got.iter().all(|r| r.is_error && r.first_text().contains("Too many")));
        assert!(ui.busy());
    }

    const MAX_FRAMES_TEST: u32 = 600;
}
