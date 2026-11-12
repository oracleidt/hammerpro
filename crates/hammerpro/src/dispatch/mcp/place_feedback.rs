use hp_app::AppState;
use hp_editor::{Changes, ObjRef, Owner};
use hp_mcp::protocol::{Content, ToolResult};
use serde_json::Value;

use super::tools::{AppFn, Outcome};

pub(super) const FEEDBACK_TOOLS: &[&str] = &[
    "create_entity",
    "create_entity_fgd",
    "move_objects",
    "transform_objects",
    "rotate_objects",
    "resize_objects",
    "flip_objects",
    "align_objects",
    "random_rotate_objects",
    "snap_objects_to_grid",
    "set_entity_keyvalues",
    "set_keyvalue",
    "paste_vmf_text",
    "paste_clipboard",
    "paste_special",
    "duplicate_objects",
    "array_duplicate",
    "sprinkle",
    "insert_prefab",
    "import_models_batch",
    "create_brush",
    "create_primitive",
];

pub(super) const SETTLE_TOOLS: &[&str] = &["create_entity", "create_entity_fgd"];

pub(super) struct Watch {
    tool: &'static str,
    earlier: Changes,
}

pub(super) fn tool_name(name: &str) -> Option<&'static str> {
    FEEDBACK_TOOLS.iter().copied().find(|t| *t == name)
}

pub(super) fn begin(state: &mut AppState, name: &str) -> Option<Watch> {
    let tool = tool_name(name)?;
    Some(Watch { tool, earlier: state.doc.take_changes() })
}

fn touched(state: &AppState) -> Vec<ObjRef> {
    let ch = state.doc.pending_changes();
    let scene = state.doc.scene();
    let mut out: Vec<ObjRef> = Vec::new();
    for id in &ch.entities_changed {
        if scene.entity(*id).is_some() && !out.contains(&ObjRef::Entity(*id)) {
            out.push(ObjRef::Entity(*id));
        }
    }
    for id in &ch.solids_changed {
        let Some(n) = scene.solid(*id) else { continue };
        let r = match n.owner {
            Owner::Entity(e) => ObjRef::Entity(e),
            Owner::World => ObjRef::Solid(*id),
        };
        if !out.contains(&r) {
            out.push(r);
        }
    }
    out
}

fn add_field(result: &mut ToolResult, key: &str, value: Value) {
    if let Some(Value::Object(m)) = result.structured.as_mut() {
        m.insert(key.to_owned(), value);
        let pretty = matches!(result.content.first(), Some(Content::Text(t)) if t.contains('\n'));
        let text = if pretty {
            serde_json::to_string_pretty(&result.structured).unwrap_or_default()
        } else {
            serde_json::to_string(&result.structured).unwrap_or_default()
        };
        if let Some(Content::Text(t)) = result.content.first_mut() {
            *t = text;
        }
        return;
    }
    let line = match &value {
        Value::String(s) => format!("{key}: {s}"),
        v => format!("{key}: {v}"),
    };
    result.content.push(Content::Text(line));
}

pub(super) fn finish(state: &mut AppState, watch: Option<Watch>, result: &mut ToolResult) {
    let Some(Watch { tool, earlier }) = watch else { return };
    let list = touched(state);
    let created = if SETTLE_TOOLS.contains(&tool) {
        state.doc.last_step_created().map(|(_, c)| c).unwrap_or_default()
    } else {
        Vec::new()
    };
    state.doc.restore_changes(earlier);
    if result.is_error || list.is_empty() {
        return;
    }
    if state.settings.placement.auto_settle_on_create && !created.is_empty() {
        let n = super::place_ui::auto_settle_created(state, &created, true);
        if n > 0 {
            add_field(result, "auto_settled", Value::from(n));
        }
    }
    if let Some(note) = super::place_ui::placement_note(state, &list) {
        add_field(result, "placement_note", Value::String(note));
    }
}

pub(super) fn finish_outcome(state: &mut AppState, watch: Option<Watch>, outcome: Outcome) -> Outcome {
    let Some(w) = watch else { return outcome };
    match outcome {
        Outcome::Done(mut r) => {
            finish(state, Some(w), &mut r);
            Outcome::Done(r)
        }
        Outcome::Worker(mut worker) => {
            state.doc.restore_changes(w.earlier);
            worker.feedback = Some(w.tool);
            Outcome::Worker(worker)
        }
        Outcome::AppFn(f) => {
            state.doc.restore_changes(w.earlier);
            let tool = w.tool;
            Outcome::AppFn(AppFn(Box::new(move |app, ctx| {
                let watch = begin(&mut app.state, tool);
                let mut r = (f.0)(app, ctx);
                finish(&mut app.state, watch, &mut r);
                r
            })))
        }
        other => {
            state.doc.restore_changes(w.earlier);
            other
        }
    }
}
