use egui::{Key, Modifiers, PointerButton, Pos2, Rect};
use hp_app::ViewKind;
use hp_mcp::tools::{ToolKind, ToolSpec};
use serde_json::{Map, Value, json};

use super::automation::{InView, PointerKind, Target, UiOp, ViewSel};
use super::schema::{integer, number, object, one_of, strings, vec3};
use super::tools::{Args, Domain, Outcome, R, args};

pub(crate) const MAX_FRAMES: u64 = 600;
const MAX_TYPE_CHARS: usize = 4096;
const MAX_SHOT: u64 = 4096;
const DEFAULT_SHOT: u64 = 1600;
const MAX_POINT: f64 = 100_000.0;

pub(crate) const DOMAIN: Domain = Domain { category: "ui automation", specs: SPECS, run };

const VIEWS: &[&str] = &["0", "1", "2", "3", "3d", "top", "front", "side", "logical"];
const BUTTONS: &[&str] = &["left", "right", "middle"];

fn position_props() -> Map<String, Value> {
    let p = json!({
        "x": number("Window x in points (or, with view, points from the view's left edge).", -MAX_POINT, MAX_POINT),
        "y": number("Window y in points (or, with view, points from the view's top edge).", -MAX_POINT, MAX_POINT),
        "view": one_of("A view: quadrant 0 to 3 (top-left, top-right, bottom-left, bottom-right) or the first 3d, top, front, side or logical (2D logical, no world points) view.", VIEWS),
        "fx": number("With view: fraction of the view's width (0 left, 1 right).", 0.0, 1.0),
        "fy": number("With view: fraction of the view's height (0 top, 1 bottom).", 0.0, 1.0),
        "world": vec3("With view: a world point [x, y, z] in Source units, projected with the view's camera."),
    });
    match p {
        Value::Object(m) => m,
        _ => Map::new(),
    }
}

fn mods_prop() -> Value {
    strings("Modifier keys held: ctrl, shift, alt.")
}

fn pointer_schema(extra: Value) -> Value {
    let mut props = position_props();
    props.insert("mods".to_owned(), mods_prop());
    if let Value::Object(e) = extra {
        props.extend(e);
    }
    object(Value::Object(props), &[])
}

fn point_schema(description: &str) -> Value {
    let mut p = position_props();
    p.remove("view");
    json!({ "type": "object", "properties": p, "additionalProperties": false, "description": description })
}

const SPECS: &[ToolSpec] = &[
    ToolSpec {
        name: "screenshot_window",
        title: "Screenshot of the window",
        description: "PNG of the whole Hammer Pro window as the user sees it (title bar with the menus, panels, \
            dialogs, views), optionally a region in window points and downscaled so the longest side is at most \
            max_size pixels. The Compile and run window is a separate window by default (setting \
            compile.separate_window): it is not in the main window's image; window \"compile\" captures it, \
            \"uv_editor\" and \"portals\" the UV editor and the portals window when they are separate windows \
            (describe_ui lists them in separate_windows).",
        kind: ToolKind::Read,
        schema: || {
            object(
                json!({
                    "window": one_of("Which window: main (default), compile (the Compile and run window), uv_editor (the UV editor) or portals (the portals window), the last three when open as separate windows.", &SHOT_WINDOWS),
                    "max_size": integer("Longest side in pixels (default 1600).", 64, MAX_SHOT as i64),
                    "settle_ms": integer("Milliseconds to let animations end first (default 150).", 0, 2000),
                    "region": { "type": "array", "items": { "type": "number" }, "minItems": 4, "maxItems": 4, "description": "[x, y, width, height] in window points (describe_ui gives the rectangles)." },
                }),
                &[],
            )
        },
    },
    ToolSpec {
        name: "describe_ui",
        title: "Describe the interface",
        description: "What is on screen: open maps, views and their rectangles, panels, open windows and dialogs \
            with their visible texts, open menus, tooltips, the focused widget, the status bar, toasts, the end of \
            the console, the dialogs open as separate windows of the system (separate_windows: the Compile and \
            run window by default, the UV editor and the portals window when taken out) and the buttons of Hammer \
            Pro's title bar (title_bar: minimize, maximize or restore, close, with their rectangles for ui_click; \
            null with the system's frames, setting ui.native_frames).",
        kind: ToolKind::Read,
        schema: || object(json!({ "console_lines": integer("Console lines to include (default 20).", 0, 200) }), &[]),
    },
    ToolSpec {
        name: "ui_move",
        title: "Move the pointer",
        description: "Move the mouse pointer (hover: tooltips, highlights). Position: window x, y, or a view with \
            fx, fy, x, y or a world point.",
        kind: ToolKind::Editor,
        schema: || pointer_schema(json!({})),
    },
    ToolSpec {
        name: "ui_click",
        title: "Click",
        description: "Click like the user: press and release a mouse button. Position: window x, y, or a view with \
            fx, fy, x, y or a world point (e.g. view \"top\", world [0, 0, 0]). In the 3D view the Selection and \
            Camera tools select the clicked object (mods [\"ctrl\"]: add or remove it); while flying in mouselook \
            (ui_key \"Z\") a left click selects at the crosshair, a right click applies the current material to the \n            targeted face (Shift: its whole brush) and a middle click picks its material; the keys of fly_apply_texture \n            (T), fly_apply_texture_brush (F), fly_pick_texture (V) and fly_align_texture (N) do the same from the keyboard.",
        kind: ToolKind::Editor,
        schema: || pointer_schema(json!({ "button": one_of("Default left.", BUTTONS) })),
    },
    ToolSpec {
        name: "ui_double_click",
        title: "Double click",
        description: "Double click like the user. Position: window x, y, or a view with fx, fy, x, y or a world \
            point.",
        kind: ToolKind::Editor,
        schema: || pointer_schema(json!({ "button": one_of("Default left.", BUTTONS) })),
    },
    ToolSpec {
        name: "ui_drag",
        title: "Drag",
        description: "Press a mouse button at from, move to to in steps, release (draw a block, move objects, pan \
            with the right button, drag a slider). from and to take x, y, fx, fy or world; view applies to both. \
            hold keeps the button down at to for that many frames before releasing: with the setting \
            workflow.drag_autopan on, a left drag held near or past the edge of a 2D view scrolls that view.",
        kind: ToolKind::Editor,
        schema: || {
            object(
                json!({
                    "view": one_of("A view for both points (see ui_click).", VIEWS),
                    "from": point_schema("Start point."),
                    "to": point_schema("End point."),
                    "button": one_of("Default left.", BUTTONS),
                    "steps": integer("Pointer moves between the two points (default 12).", 1, 200),
                    "hold": integer("Frames to keep the button down at to before releasing (default 0).", 0, 600),
                    "mods": mods_prop(),
                }),
                &["from", "to"],
            )
        },
    },
    ToolSpec {
        name: "ui_scroll",
        title: "Mouse wheel",
        description: "Turn the mouse wheel at a position: positive dy zooms a view in (or scrolls a list up), \
            with mods ctrl it changes the grid in a 2D view. trackpad true sends a two finger trackpad scroll of \
            dx, dy points instead (pans a 2D view when trackpad mode is on, see input.trackpad_mode), pinch sends \
            a trackpad pinch with that zoom factor (above 1 zooms in).",
        kind: ToolKind::Editor,
        schema: || {
            pointer_schema(json!({
                "dy": number("Wheel lines, or trackpad points with trackpad (positive: forward).", -2000.0, 2000.0),
                "dx": number("Sideways trackpad points (with trackpad).", -2000.0, 2000.0),
                "trackpad": { "type": "boolean", "description": "Send a two finger trackpad scroll (dx, dy in points) instead of wheel lines." },
                "pinch": number("Trackpad pinch zoom factor instead of a scroll (1.25 zooms in, 0.8 out).", 0.1, 10.0),
            }))
        },
    },
    ToolSpec {
        name: "ui_key",
        title: "Press a key",
        description: "Press and release a key or shortcut like \"Ctrl+Shift+S\", \"Escape\", \"Enter\", \"F5\", \
            \"Delete\", \"ArrowUp\"; hold_frames keeps it down (camera flying with W A S D).",
        kind: ToolKind::Editor,
        schema: || {
            object(
                json!({
                    "key": { "type": "string", "description": "Key name with optional Ctrl+, Shift+, Alt+ (egui names: A, Num1, F5, Escape, Enter, Tab, Space, Delete, Backspace, ArrowLeft...)." },
                    "hold_frames": integer("Frames to keep the key down (default 0: a tap).", 0, MAX_FRAMES as i64),
                }),
                &["key"],
            )
        },
    },
    ToolSpec {
        name: "ui_type",
        title: "Type text",
        description: "Type text into the focused text field (click the field first; use ui_key for Enter, Tab \
            and shortcuts).",
        kind: ToolKind::Editor,
        schema: || {
            object(
                json!({ "text": { "type": "string", "description": "Text to type (no control characters).", "maxLength": MAX_TYPE_CHARS } }),
                &["text"],
            )
        },
    },
    ToolSpec {
        name: "wait_frames",
        title: "Wait for frames",
        description: "Wait until Hammer Pro drew this many frames (animations, loading, a dialog opening), then \
            answer with the editor state.",
        kind: ToolKind::Read,
        schema: || object(json!({ "frames": integer("Frames to wait (default 2).", 1, MAX_FRAMES as i64) }), &[]),
    },
];

fn run(_state: &mut hp_app::AppState, name: &str, v: &Value) -> Option<Outcome> {
    let op = match name {
        "screenshot_window" => screenshot(v),
        "describe_ui" => describe(v),
        "ui_move" => pointer(v, "ui_move", |_| Ok(PointerKind::Move)),
        "ui_click" => pointer(v, "ui_click", |a| Ok(PointerKind::Click { button: button(a)?, double: false })),
        "ui_double_click" => {
            pointer(v, "ui_double_click", |a| Ok(PointerKind::Click { button: button(a)?, double: true }))
        }
        "ui_drag" => drag(v),
        "ui_scroll" => pointer(v, "ui_scroll", scroll_kind),
        "ui_key" => key(v),
        "ui_type" => type_text(v),
        "wait_frames" => wait(v),
        _ => return None,
    };
    Some(match op {
        Ok(op) => Outcome::Ui(op),
        Err(e) => Outcome::Done(hp_mcp::protocol::ToolResult::error(format!("{name}: {e}"))),
    })
}

const SHOT_WINDOWS: [&str; 4] = ["main", "compile", "uv_editor", "portals"];

fn screenshot(v: &Value) -> R<UiOp> {
    let a = args(v, &["window", "max_size", "region", "settle_ms"])?;
    let window = a.opt_enum("window", &SHOT_WINDOWS)?.and_then(|w| SHOT_WINDOWS[1..].iter().copied().find(|n| *n == w));
    let settle_ms = a.opt_u64("settle_ms", 0, 2000)?.unwrap_or(150) as u32;
    let max_size = a.opt_u64("max_size", 64, MAX_SHOT)?.unwrap_or(DEFAULT_SHOT) as u32;
    let region = match a.get("region") {
        None => None,
        Some(r) => {
            let bad = || "region must be [x, y, width, height] in window points (width and height above 0)".to_owned();
            let n = r.as_array().filter(|n| n.len() == 4).ok_or_else(bad)?;
            let mut f = [0f32; 4];
            for (o, x) in f.iter_mut().zip(n) {
                *o = x.as_f64().filter(|x| x.is_finite() && x.abs() <= MAX_POINT).ok_or_else(bad)? as f32;
            }
            if f[2] < 1.0 || f[3] < 1.0 {
                return Err(bad());
            }
            Some(Rect::from_min_size(egui::pos2(f[0], f[1]), egui::vec2(f[2], f[3])))
        }
    };
    Ok(UiOp::Screenshot { max_size, region, settle_ms, window })
}

fn describe(v: &Value) -> R<UiOp> {
    let a = args(v, &["console_lines"])?;
    Ok(UiOp::Describe { console: a.opt_u64("console_lines", 0, 200)?.unwrap_or(20) as usize })
}

fn wait(v: &Value) -> R<UiOp> {
    let a = args(v, &["frames"])?;
    Ok(UiOp::WaitFrames(a.opt_u64("frames", 1, MAX_FRAMES)?.unwrap_or(2) as u32))
}

fn button(a: &Args<'_>) -> R<PointerButton> {
    Ok(match a.opt_enum("button", BUTTONS)?.as_deref() {
        Some("right") => PointerButton::Secondary,
        Some("middle") => PointerButton::Middle,
        _ => PointerButton::Primary,
    })
}

fn mods(a: &Args<'_>) -> R<Modifiers> {
    let mut m = Modifiers::NONE;
    for name in a.strings("mods", 3)? {
        match name.to_ascii_lowercase().as_str() {
            "ctrl" | "control" | "cmd" | "command" => {
                m.ctrl = true;
                m.command = true;
            }
            "shift" => m.shift = true,
            "alt" => m.alt = true,
            other => return Err(format!("unknown modifier \"{other}\" (ctrl, shift, alt)")),
        }
    }
    Ok(m)
}

const VIEW_ERROR: &str = "view must be \"0\" to \"3\", \"3d\", \"top\", \"front\", \"side\" or \"logical\"";

fn view_sel(a: &Args<'_>) -> R<Option<ViewSel>> {
    let Some(v) = a.get("view") else { return Ok(None) };
    let text = match v {
        Value::String(s) => s.trim().to_ascii_lowercase(),
        Value::Number(n) => n.to_string(),
        _ => return Err(VIEW_ERROR.to_owned()),
    };
    Ok(Some(match text.as_str() {
        "0" | "1" | "2" | "3" => ViewSel::Index(text.parse().unwrap_or(0)),
        "3d" | "perspective" | "camera" => ViewSel::Kind(ViewKind::Perspective),
        "top" => ViewSel::Kind(ViewKind::Top),
        "front" => ViewSel::Kind(ViewKind::Front),
        "side" => ViewSel::Kind(ViewKind::Side),
        "logical" | "2d_logical" => ViewSel::Logical,
        _ => return Err(VIEW_ERROR.to_owned()),
    }))
}

fn pair(a: &Args<'_>, kx: &str, ky: &str, min: f64, max: f64) -> R<Option<(f32, f32)>> {
    match (a.opt_f64(kx, min, max)?, a.opt_f64(ky, min, max)?) {
        (Some(x), Some(y)) => Ok(Some((x as f32, y as f32))),
        (None, None) => Ok(None),
        _ => Err(format!("give both {kx} and {ky}")),
    }
}

fn target(a: &Args<'_>, view: Option<ViewSel>) -> R<Target> {
    let xy = pair(a, "x", "y", -MAX_POINT, MAX_POINT)?;
    let fxy = pair(a, "fx", "fy", 0.0, 1.0)?;
    let world = a.opt_vec3("world")?;
    let given = usize::from(xy.is_some()) + usize::from(fxy.is_some()) + usize::from(world.is_some());
    if given > 1 {
        return Err("give one position: x and y, fx and fy, or world".to_owned());
    }
    match view {
        None => {
            if fxy.is_some() || world.is_some() {
                return Err("fx, fy and world need a view".to_owned());
            }
            let (x, y) = xy.ok_or("give x and y (window points), or a view")?;
            Ok(Target::Window(Pos2::new(x, y)))
        }
        Some(view) => {
            if view == ViewSel::Logical && world.is_some() {
                return Err("the logical view has no world positions: give fx and fy, or x and y".to_owned());
            }
            let at = match (xy, fxy, world) {
                (Some((x, y)), _, _) => InView::Points(x, y),
                (_, Some((x, y)), _) => InView::Fraction(x, y),
                (_, _, Some(w)) => InView::World(w),
                _ => InView::Fraction(0.5, 0.5),
            };
            Ok(Target::View { view, at })
        }
    }
}

const POSITION_ARGS: &[&str] = &["x", "y", "view", "fx", "fy", "world", "mods"];

fn scroll_kind(a: &Args<'_>) -> R<PointerKind> {
    use crate::script::WheelKind;
    let trackpad = a.opt_bool("trackpad")?.unwrap_or(false);
    let dx = a.opt_f64("dx", -2000.0, 2000.0)?;
    if let Some(f) = a.opt_f64("pinch", 0.1, 10.0)? {
        if a.get("dy").is_some() || dx.is_some() || trackpad {
            return Err("pinch goes alone: no dy, dx or trackpad with it".to_owned());
        }
        return Ok(PointerKind::Scroll { dx: 0.0, dy: f as f32, kind: WheelKind::Pinch });
    }
    if trackpad {
        let dy = a.opt_f64("dy", -2000.0, 2000.0)?.unwrap_or(0.0);
        let dx = dx.unwrap_or(0.0);
        if dx == 0.0 && dy == 0.0 {
            return Err("a trackpad scroll needs dx or dy".to_owned());
        }
        return Ok(PointerKind::Scroll { dx: dx as f32, dy: dy as f32, kind: WheelKind::Trackpad });
    }
    if dx.is_some() {
        return Err("dx needs trackpad true (a mouse wheel only turns forward and back)".to_owned());
    }
    let dy = a.opt_f64("dy", -50.0, 50.0)?.ok_or("dy is required")?;
    Ok(PointerKind::Scroll { dx: 0.0, dy: dy as f32, kind: WheelKind::Wheel })
}

fn pointer(v: &Value, tool: &'static str, kind: impl Fn(&Args<'_>) -> R<PointerKind>) -> R<UiOp> {
    let mut allowed = POSITION_ARGS.to_vec();
    match tool {
        "ui_click" | "ui_double_click" => allowed.push("button"),
        "ui_scroll" => allowed.extend(["dy", "dx", "trackpad", "pinch"]),
        _ => {}
    }
    let a = args(v, &allowed)?;
    let kind = kind(&a)?;
    let at = target(&a, view_sel(&a)?)?;
    Ok(UiOp::Pointer { tool, kind, at, mods: mods(&a)? })
}

fn drag(v: &Value) -> R<UiOp> {
    let a = args(v, &["view", "from", "to", "button", "steps", "hold", "mods"])?;
    let view = view_sel(&a)?;
    let point = |key: &str| -> R<Target> {
        let p = a.get(key).ok_or_else(|| format!("{key} is required"))?;
        let inner = args(p, &["x", "y", "fx", "fy", "world"]).map_err(|e| format!("{key}: {e}"))?;
        target(&inner, view).map_err(|e| format!("{key}: {e}"))
    };
    let from = point("from")?;
    let to = point("to")?;
    let steps = a.opt_u64("steps", 1, 200)?.unwrap_or(12) as u32;
    let hold = a.opt_u64("hold", 0, 600)?.unwrap_or(0) as u32;
    Ok(UiOp::Pointer {
        tool: "ui_drag",
        kind: PointerKind::Drag { to, button: button(&a)?, steps, hold },
        at: from,
        mods: mods(&a)?,
    })
}

pub(crate) fn parse_combo(text: &str) -> R<(Key, Modifiers)> {
    let mut m = Modifiers::NONE;
    let mut key = None;
    for part in text.split('+').map(str::trim) {
        match part.to_ascii_lowercase().as_str() {
            "ctrl" | "control" | "cmd" | "command" => {
                m.ctrl = true;
                m.command = true;
            }
            "shift" => m.shift = true,
            "alt" => m.alt = true,
            "" => return Err(format!("\"{text}\" is not a key combination")),
            _ if key.is_some() => return Err(format!("\"{text}\" has two keys")),
            _ => {
                key = Some(
                    Key::from_name(part)
                        .or_else(|| (part.len() == 1).then(|| Key::from_name(&part.to_ascii_uppercase())).flatten())
                        .ok_or_else(|| {
                            format!("unknown key \"{part}\" (egui key names: A, Num1, F5, Escape, Enter...)")
                        })?,
                );
            }
        }
    }
    key.map(|k| (k, m)).ok_or_else(|| format!("\"{text}\" has no key (only modifiers)"))
}

fn key(v: &Value) -> R<UiOp> {
    let a = args(v, &["key", "hold_frames"])?;
    let (key, mods) = parse_combo(a.str("key", 64)?)?;
    let hold = a.opt_u64("hold_frames", 0, MAX_FRAMES)?.unwrap_or(0) as u32;
    Ok(UiOp::Key { key, mods, hold })
}

fn type_text(v: &Value) -> R<UiOp> {
    let a = args(v, &["text"])?;
    let text = a.str("text", MAX_TYPE_CHARS * 4)?;
    if text.is_empty() {
        return Err("text is empty".to_owned());
    }
    if text.chars().count() > MAX_TYPE_CHARS {
        return Err(format!("text is too long (at most {MAX_TYPE_CHARS} characters)"));
    }
    if text.chars().any(char::is_control) {
        return Err("text has control characters (use ui_key for Enter, Tab...)".to_owned());
    }
    Ok(UiOp::Type(text.to_owned()))
}

#[cfg(test)]
mod tests {
    use super::*;

    fn op(name: &str, v: Value) -> Result<UiOp, String> {
        let mut state = hp_app::testing::test_state();
        match run(&mut state, name, &v) {
            Some(Outcome::Ui(op)) => Ok(op),
            Some(Outcome::Done(r)) => Err(r.first_text().to_owned()),
            other => panic!("{name}: {other:?}"),
        }
    }

    #[test]
    fn positions() {
        let o = op("ui_click", json!({ "x": 10, "y": 20 })).unwrap();
        assert!(matches!(o, UiOp::Pointer { at: Target::Window(p), .. } if p == Pos2::new(10.0, 20.0)));
        let o =
            op("ui_click", json!({ "view": "top", "world": [1, 2, 3], "button": "right", "mods": ["Ctrl", "shift"] }))
                .unwrap();
        let UiOp::Pointer { at, kind, mods, .. } = o else { panic!() };
        assert_eq!(
            at,
            Target::View { view: ViewSel::Kind(ViewKind::Top), at: InView::World(glam::DVec3::new(1.0, 2.0, 3.0)) }
        );
        assert_eq!(kind, PointerKind::Click { button: PointerButton::Secondary, double: false });
        assert!(mods.ctrl && mods.command && mods.shift && !mods.alt);
        let o = op("ui_move", json!({ "view": "2" })).unwrap();
        assert!(matches!(
            o,
            UiOp::Pointer { at: Target::View { view: ViewSel::Index(2), at: InView::Fraction(..) }, .. }
        ));
        let o = op("ui_click", json!({ "view": "Logical", "x": 40, "y": 60 })).unwrap();
        assert!(matches!(o, UiOp::Pointer { at: Target::View { view: ViewSel::Logical, at: InView::Points(..) }, .. }));
        for (v, needle) in [
            (json!({ "view": "logical", "world": [1, 2, 3] }), "no world positions"),
            (json!({}), "x and y"),
            (json!({ "x": 1 }), "both x and y"),
            (json!({ "fx": 0.5, "fy": 0.5 }), "need a view"),
            (json!({ "view": "top", "x": 1, "y": 1, "fx": 0.5, "fy": 0.5 }), "one position"),
            (json!({ "view": "left" }), "view must be"),
            (json!({ "view": "top", "fx": 2, "fy": 0 }), "fx must be"),
            (json!({ "x": "a", "y": 1 }), "x must be"),
            (json!({ "x": 1, "y": 1, "mods": ["hyper"] }), "unknown modifier"),
            (json!({ "x": 1, "y": 1, "button": "side" }), "button must be"),
            (json!({ "x": 1, "y": 1, "dy": 1 }), "unknown argument"),
            (json!({ "view": "top", "world": [1, 2] }), "world must be"),
        ] {
            let e = op("ui_click", v.clone()).unwrap_err();
            assert!(e.contains(needle), "{v}: {e}");
        }
    }

    #[test]
    fn drag_scroll_keys_text_and_frames() {
        let o = op(
            "ui_drag",
            json!({ "view": "top", "from": { "world": [0, 0, 0] }, "to": { "fx": 1, "fy": 1 }, "steps": 3 }),
        )
        .unwrap();
        assert!(matches!(o, UiOp::Pointer { kind: PointerKind::Drag { steps: 3, hold: 0, .. }, .. }));
        let held = op("ui_drag", json!({ "from": { "x": 1, "y": 1 }, "to": { "x": 9, "y": 1 }, "hold": 90 })).unwrap();
        assert!(matches!(held, UiOp::Pointer { kind: PointerKind::Drag { hold: 90, .. }, .. }));
        assert!(op("ui_drag", json!({ "from": { "x": 1, "y": 1 }, "to": { "x": 9, "y": 1 }, "hold": 601 })).is_err());
        assert!(op("ui_drag", json!({ "from": { "x": 1, "y": 1 } })).unwrap_err().contains("to is required"));
        assert!(
            op("ui_drag", json!({ "from": { "x": 1, "y": 1, "z": 1 }, "to": { "x": 1, "y": 1 } }))
                .unwrap_err()
                .contains("from: unknown argument")
        );
        assert!(op("ui_scroll", json!({ "x": 1, "y": 1 })).unwrap_err().contains("dy is required"));
        assert!(matches!(
            op("ui_scroll", json!({ "x": 1, "y": 1, "dy": -2 })).unwrap(),
            UiOp::Pointer { kind: PointerKind::Scroll { .. }, .. }
        ));
        use crate::script::WheelKind;
        assert!(matches!(
            op("ui_scroll", json!({ "x": 1, "y": 1, "dx": 30, "dy": -2, "trackpad": true })).unwrap(),
            UiOp::Pointer { kind: PointerKind::Scroll { dx: 30.0, dy: -2.0, kind: WheelKind::Trackpad }, .. }
        ));
        assert!(matches!(
            op("ui_scroll", json!({ "x": 1, "y": 1, "pinch": 1.25 })).unwrap(),
            UiOp::Pointer { kind: PointerKind::Scroll { kind: WheelKind::Pinch, .. }, .. }
        ));
        assert!(op("ui_scroll", json!({ "x": 1, "y": 1, "dx": 3 })).unwrap_err().contains("trackpad"));
        assert!(op("ui_scroll", json!({ "x": 1, "y": 1, "trackpad": true })).unwrap_err().contains("dx or dy"));
        assert!(op("ui_scroll", json!({ "x": 1, "y": 1, "pinch": 2, "dy": 1 })).unwrap_err().contains("alone"));

        assert_eq!(parse_combo("Ctrl+Shift+S").unwrap().0, Key::S);
        assert_eq!(
            parse_combo("escape").unwrap_err(),
            "unknown key \"escape\" (egui key names: A, Num1, F5, Escape, Enter...)"
        );
        assert_eq!(parse_combo("a").unwrap().0, Key::A);
        assert!(parse_combo("Ctrl+").is_err());
        assert!(parse_combo("Ctrl").unwrap_err().contains("no key"));
        assert!(parse_combo("A+B").unwrap_err().contains("two keys"));
        assert!(matches!(op("ui_key", json!({ "key": "W", "hold_frames": 30 })).unwrap(), UiOp::Key { hold: 30, .. }));
        assert!(op("ui_key", json!({ "key": "W", "hold_frames": 100000 })).is_err());

        assert_eq!(op("ui_type", json!({ "text": "Bonjour é" })).unwrap(), UiOp::Type("Bonjour é".to_owned()));
        assert!(op("ui_type", json!({ "text": "a\nb" })).unwrap_err().contains("control"));
        assert!(op("ui_type", json!({ "text": "" })).is_err());
        assert!(op("ui_type", json!({ "text": "x".repeat(MAX_TYPE_CHARS + 1) })).is_err());

        assert_eq!(op("wait_frames", json!({})).unwrap(), UiOp::WaitFrames(2));
        assert!(op("wait_frames", json!({ "frames": 0 })).is_err());
        assert_eq!(op("describe_ui", json!({ "console_lines": 5 })).unwrap(), UiOp::Describe { console: 5 });
        assert!(matches!(
            op("screenshot_window", json!({ "max_size": 800, "region": [0, 0, 100, 50] })).unwrap(),
            UiOp::Screenshot { max_size: 800, region: Some(_), settle_ms: 150, window: None }
        ));
        assert!(matches!(
            op("screenshot_window", json!({ "window": "compile" })).unwrap(),
            UiOp::Screenshot { region: None, window: Some("compile"), .. }
        ));
        assert!(matches!(
            op("screenshot_window", json!({ "window": "uv_editor" })).unwrap(),
            UiOp::Screenshot { window: Some("uv_editor"), .. }
        ));
        assert!(matches!(
            op("screenshot_window", json!({ "window": "main" })).unwrap(),
            UiOp::Screenshot { window: None, .. }
        ));
        assert!(op("screenshot_window", json!({ "window": "player" })).is_err());
        assert!(op("screenshot_window", json!({ "region": [0, 0, 0, 50] })).is_err());
        assert!(op("screenshot_window", json!({ "max_size": 10 })).is_err());
    }
}
