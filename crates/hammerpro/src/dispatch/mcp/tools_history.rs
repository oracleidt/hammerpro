use glam::DVec3;
use hp_app::AppState;
use hp_editor::{Document, PasteSpecial};
use hp_mcp::protocol::ToolResult;
use hp_mcp::tools::MAX_VMF_TEXT;
use serde_json::json;

use super::semantic::*;
use super::tools::{AppFn, Args, Outcome, R, bounds_json, edit, obj_json};

domain! { "history and clipboard";
    get_history: Read, "Undo history",
        "The undo steps (most recent last), the redo steps (next first), their names in the language of the \
        interface, the memory they use and the limits. With the setting edit.undo_restores_selection (on by \
        default) selection changes are steps too, named Selection or Face selection: they never mark the map as \
        modified.",
        {}, [];
    undo_steps: Editor, "Undo several steps",
        "Undo several steps at once (stops when there is nothing left to undo).",
        { "count": integer("Steps to undo.", 1, 1000) }, ["count"];
    redo_steps: Editor, "Redo several steps",
        "Redo several steps at once.",
        { "count": integer("Steps to redo.", 1, 1000) }, ["count"];
    copy_objects: Editor, "Copy",
        "Copy objects (default: the selection) to the system clipboard as VMF text, like Ctrl+C (Hammer and \
        Hammer++ can paste it).",
        { "ids": target_ids(), "solid_ids": target_solid_ids() }, [];
    cut_objects: Destructive, "Cut",
        "Copy objects (default: the selection) to the system clipboard and delete them, like Ctrl+X.",
        { "ids": target_ids(), "solid_ids": target_solid_ids() }, [];
    paste_clipboard: Edit, "Paste",
        "Paste the VMF text of the system clipboard (from Hammer Pro, Hammer or Hammer++), moved by an offset.",
        { "offset": vec3("Offset [x, y, z] (default none).") }, [];
    paste_special: Edit, "Paste special",
        "Paste VMF text (text, or the system clipboard with from_clipboard) several times, each copy moved and \
        rotated from the previous one around a pivot, optionally grouped, with unique or prefixed entity names.",
        {
            "text": string("VMF text (get_vmf_text gives some)."),
            "from_clipboard": boolean("Use the system clipboard instead of text."),
            "copies": integer("Number of copies (default 1).", 1, 256),
            "offset": vec3("Offset between copies."),
            "rotation": vec3("Rotation between copies, degrees around X, Y and Z."),
            "pivot": vec3("Rotation pivot (default: the center of the pasted objects)."),
            "start_at_original": boolean("The first copy is exactly over the original."),
            "group": boolean("Group each copy."),
            "unique_names": boolean("Rename entities whose name exists already (name_2...)."),
            "name_prefix": string("Prefix added to every entity name."),
            "preview": boolean("Do not paste: show the copies as a green wireframe ghost in the 3D and 2D views, like the live preview of the Paste special dialog, and answer how many objects, copies and lines it holds with the bounds of the ghost. The ghost stays until clear_preview or a real paste."),
            "clear_preview": boolean("Remove the ghost left by preview (no text needed)."),
        }, [];
}

fn get_history(state: &mut AppState, _a: &Args<'_>) -> R<ToolResult> {
    let doc = &state.doc;
    let (undo, redo) = doc.history_len();
    let limits = doc.undo_limits();
    let text =
        |labels: Vec<&str>| -> Vec<String> { labels.into_iter().map(hp_ui_shell::menu::undo_step_text).collect() };
    Ok(ToolResult::json(json!({
        "undo": doc.undo_labels(),
        "undo_text": text(doc.undo_labels()),
        "redo": doc.redo_labels(),
        "redo_text": text(doc.redo_labels()),
        "selection_steps": doc.records_selection(),
        "undo_steps": undo,
        "redo_steps": redo,
        "next_undo": doc.undo_label(),
        "next_redo": doc.redo_label(),
        "bytes": doc.history_bytes(),
        "limits": format!("{limits:?}"),
    })))
}

fn steps(state: &mut AppState, a: &Args<'_>, undo: bool) -> R<ToolResult> {
    let count = a.opt_u64("count", 1, 1000)?.ok_or("count is required")? as usize;
    if state.doc.in_transaction() {
        return Err("Hammer Pro is in the middle of another edit; try again".to_owned());
    }
    let mut labels = Vec::new();
    for _ in 0..count {
        let label = if undo { state.doc.undo_label() } else { state.doc.redo_label() }.map(str::to_owned);
        let done = if undo { state.doc.undo() } else { state.doc.redo() };
        if !done {
            break;
        }
        labels.push(label.unwrap_or_default());
    }
    Ok(ToolResult::json(json!({ if undo { "undone" } else { "redone" }: labels, "count": labels.len() })))
}

fn undo_steps(state: &mut AppState, a: &Args<'_>) -> R<ToolResult> {
    steps(state, a, true)
}

fn redo_steps(state: &mut AppState, a: &Args<'_>) -> R<ToolResult> {
    steps(state, a, false)
}

fn set_clipboard(app: &mut crate::app::HammerProApp, text: String) -> Result<(), String> {
    let clipboard = app.clipboard.as_mut().ok_or("the system clipboard is not available")?;
    clipboard.set_text(text).map_err(|e| format!("the system clipboard refused the text: {e}"))
}

fn get_clipboard(app: &mut crate::app::HammerProApp) -> Result<String, String> {
    let clipboard = app.clipboard.as_mut().ok_or("the system clipboard is not available")?;
    let text = clipboard.get_text().map_err(|_| "the system clipboard has no text".to_owned())?;
    if text.len() > MAX_VMF_TEXT {
        return Err(format!("the clipboard text is larger than {} MB", MAX_VMF_TEXT / (1024 * 1024)));
    }
    Ok(text)
}

fn copy_objects(state: &mut AppState, a: &Args<'_>) -> R<Outcome> {
    let objs = targets(state, a)?;
    let text = state.doc.copy(&objs);
    if text.len() > MAX_VMF_TEXT {
        return Err("the objects are too large for the clipboard".to_owned());
    }
    let count = objs.len();
    Ok(Outcome::AppFn(AppFn(Box::new(move |app, _ctx| match set_clipboard(app, text) {
        Ok(()) => ToolResult::json(json!({ "copied": count })),
        Err(e) => ToolResult::error(format!("copy_objects: {e}")),
    }))))
}

fn cut_objects(state: &mut AppState, a: &Args<'_>) -> R<Outcome> {
    let objs = targets(state, a)?;
    let text = state.doc.copy(&objs);
    if text.len() > MAX_VMF_TEXT {
        return Err("the objects are too large for the clipboard".to_owned());
    }
    Ok(Outcome::AppFn(AppFn(Box::new(move |app, _ctx| {
        if let Err(e) = set_clipboard(app, text) {
            return ToolResult::error(format!("cut_objects: {e} (nothing was deleted)"));
        }
        match edit(&mut app.state, "cut_objects", |doc| doc.delete(&objs)) {
            Ok(n) => ToolResult::json(json!({ "cut": n })),
            Err(e) => ToolResult::error(format!("cut_objects: {e}")),
        }
    }))))
}

fn paste_text(state: &mut AppState, tool: &str, text: &str, offset: DVec3) -> R<ToolResult> {
    Document::clipboard_bounds(text).map_err(|e| format!("the text has no map objects: {e}"))?;
    let objs = edit(state, tool, |doc| doc.paste(text, offset))?;
    Ok(ToolResult::json(json!({
        "pasted": objs.len(),
        "objects": objs.iter().take(200).map(|o| obj_json(*o)).collect::<Vec<_>>(),
    })))
}

fn paste_clipboard(_state: &mut AppState, a: &Args<'_>) -> R<Outcome> {
    let offset = a.opt_vec3("offset")?.unwrap_or(DVec3::ZERO);
    Ok(Outcome::AppFn(AppFn(Box::new(move |app, _ctx| {
        let r = get_clipboard(app).and_then(|text| paste_text(&mut app.state, "paste_clipboard", &text, offset));
        r.unwrap_or_else(|e| ToolResult::error(format!("paste_clipboard: {e}")))
    }))))
}

fn special_options(a: &Args<'_>) -> R<PasteSpecial> {
    let prefix = a.opt_str("name_prefix", 64)?.unwrap_or_default().to_owned();
    if prefix.contains(['"', '\n', '\r', ',', ' ']) {
        return Err("name_prefix must not contain quotes, commas, spaces or line breaks".to_owned());
    }
    let rotation = a.opt_vec3("rotation")?.unwrap_or(DVec3::ZERO);
    if rotation.abs().max_element() > 360.0 {
        return Err("rotation must be degrees from -360 to 360".to_owned());
    }
    Ok(PasteSpecial {
        copies: a.opt_u64("copies", 1, 256)?.unwrap_or(1) as u32,
        offset: a.opt_vec3("offset")?.unwrap_or(DVec3::ZERO),
        rotation,
        pivot: a.opt_vec3("pivot")?,
        start_at_original: a.opt_bool("start_at_original")?.unwrap_or(false),
        group: a.opt_bool("group")?.unwrap_or(false),
        unique_names: a.opt_bool("unique_names")?.unwrap_or(false),
        name_prefix: prefix,
    })
}

fn paste_special_text(state: &mut AppState, text: &str, opts: &PasteSpecial, preview: bool) -> R<ToolResult> {
    Document::clipboard_bounds(text).map_err(|e| format!("the text has no map objects: {e}"))?;
    if preview {
        let ghost = Document::paste_special_preview(text, opts).map_err(|e| e.to_string())?;
        let mut bounds = hp_geom::Aabb::EMPTY;
        for [a, b] in &ghost.lines {
            bounds.extend(*a);
            bounds.extend(*b);
        }
        let answer = json!({
            "preview": true,
            "objects": ghost.objects,
            "copies": ghost.copies,
            "shown_copies": ghost.shown_copies,
            "lines": ghost.lines.len(),
            "bounds": bounds_json((!bounds.is_empty()).then_some(bounds)),
        });
        let wf = &mut state.runtime.workflow;
        wf.paste_preview_serial = wf.paste_preview_serial.wrapping_add(1);
        wf.paste_preview = Some((wf.paste_preview_serial, std::sync::Arc::new(ghost)));
        return Ok(ToolResult::json(answer));
    }
    state.runtime.workflow.paste_preview = None;
    let objs = edit(state, "paste_special", |doc| doc.paste_special(text, opts))?;
    Ok(ToolResult::json(json!({
        "pasted": objs.len(),
        "objects": objs.iter().take(200).map(|o| obj_json(*o)).collect::<Vec<_>>(),
    })))
}

fn paste_special(state: &mut AppState, a: &Args<'_>) -> R<Outcome> {
    if a.opt_bool("clear_preview")?.unwrap_or(false) {
        let had = state.runtime.workflow.paste_preview.take().is_some();
        return Ok(Outcome::Done(ToolResult::json(json!({ "cleared": had }))));
    }
    let opts = special_options(a)?;
    let preview = a.opt_bool("preview")?.unwrap_or(false);
    match (a.opt_str("text", MAX_VMF_TEXT)?, a.opt_bool("from_clipboard")?.unwrap_or(false)) {
        (Some(_), true) => Err("give text or from_clipboard, not both".to_owned()),
        (Some(text), false) => paste_special_text(state, text, &opts, preview).map(Outcome::Done),
        (None, true) => Ok(Outcome::AppFn(AppFn(Box::new(move |app, _ctx| {
            let r = get_clipboard(app).and_then(|text| paste_special_text(&mut app.state, &text, &opts, preview));
            r.unwrap_or_else(|e| ToolResult::error(format!("paste_special: {e}")))
        })))),
        (None, false) => Err("give text, or from_clipboard: true".to_owned()),
    }
}

#[cfg(test)]
mod tests {
    use serde_json::json;

    use super::super::semantic_tests::{err, needs_app, ok};

    #[test]
    fn history_and_paste() {
        let mut s = hp_app::testing::test_state();
        ok(&mut s, "duplicate_objects", json!({ "solid_ids": [3] }));
        ok(&mut s, "duplicate_objects", json!({ "solid_ids": [3] }));
        let h = ok(&mut s, "get_history", json!({}));
        assert_eq!(h["undo_steps"], 2);
        let r = ok(&mut s, "undo_steps", json!({ "count": 5 }));
        assert_eq!(r["count"], 2);
        assert_eq!(r["undone"][0], "MCP: duplicate_objects");
        let r = ok(&mut s, "redo_steps", json!({ "count": 1 }));
        assert_eq!(r["count"], 1);
        let text = s.doc.copy(&[hp_editor::ObjRef::Solid(3)]);
        let n = s.doc.stats().solids;
        let r = ok(&mut s, "paste_special", json!({ "text": text, "copies": 3, "offset": [0, 0, 160], "group": true }));
        assert_eq!(r["pasted"].as_u64().unwrap(), 3);
        assert_eq!(s.doc.stats().solids, n + 3);
        assert_eq!(s.doc.undo_label(), Some("MCP: paste_special"));
        let n = s.doc.stats().solids;
        let r = ok(
            &mut s,
            "paste_special",
            json!({ "text": text, "copies": 2, "offset": [0, 0, 96], "rotation": [0, 0, 45], "preview": true }),
        );
        assert_eq!((r["copies"].as_u64(), r["shown_copies"].as_u64()), (Some(2), Some(2)));
        assert!(r["lines"].as_u64().unwrap() > 0);
        assert_eq!(s.doc.stats().solids, n, "a preview changes nothing");
        assert!(s.runtime.workflow.paste_preview.is_some());
        let r = ok(&mut s, "paste_special", json!({ "clear_preview": true }));
        assert_eq!(r["cleared"], true);
        assert!(s.runtime.workflow.paste_preview.is_none());
        err(&mut s, "paste_special", json!({ "text": "not a map" }));
        err(&mut s, "paste_special", json!({}));
        err(&mut s, "paste_special", json!({ "text": "x", "from_clipboard": true }));
        assert!(needs_app(&mut s, "paste_special", json!({ "from_clipboard": true })));
        assert!(needs_app(&mut s, "copy_objects", json!({ "solid_ids": [3] })));
        assert!(needs_app(&mut s, "cut_objects", json!({ "solid_ids": [3] })));
        assert!(needs_app(&mut s, "paste_clipboard", json!({})));
        s.doc.clear_selection();
        err(&mut s, "copy_objects", json!({}));
    }

    #[test]
    fn selection_steps_through_mcp() {
        let mut s = hp_app::testing::test_state();
        s.apply_selection_undo_setting();
        let dirty = s.doc.is_dirty();
        let before = s.doc.history_len().0;
        ok(&mut s, "select_objects", json!({ "solid_ids": [2] }));
        ok(&mut s, "select_objects", json!({ "solid_ids": [3], "mode": "add" }));
        ok(&mut s, "clear_selection", json!({}));
        ok(&mut s, "select_faces", json!({ "faces": [{ "solid": 2, "side": 0 }, { "solid": 3, "side": 1 }] }));
        ok(&mut s, "clear_selection", json!({}));
        let h = ok(&mut s, "get_history", json!({}));
        assert_eq!(h["undo_steps"], before + 5);
        assert_eq!(h["selection_steps"], true);
        let labels: Vec<&str> = h["undo"].as_array().unwrap().iter().filter_map(|v| v.as_str()).collect();
        assert_eq!(labels[before..], ["Selection", "Selection", "Selection", "Face selection", "Face selection"]);
        assert_eq!(h["undo_text"].as_array().unwrap().len(), before + 5);
        assert_eq!(s.doc.is_dirty(), dirty, "selection steps do not modify the map");
        let r = ok(&mut s, "undo", json!({}));
        assert_eq!(r["undone"], "Face selection");
        let faces = ok(&mut s, "get_selection", json!({}));
        assert_eq!(s.doc.selected_faces().len(), 2, "{faces}");
        let r = ok(&mut s, "undo_steps", json!({ "count": 2 }));
        assert_eq!(r["count"], 2);
        assert_eq!(s.doc.selected_objects(), [hp_editor::ObjRef::Solid(2), hp_editor::ObjRef::Solid(3)]);
        let h = ok(&mut s, "get_history", json!({}));
        assert_eq!(h["redo"][0], "Selection");
        assert!(ok(&mut s, "redo", json!({}))["redone"] == "Selection");
        assert!(s.doc.selection().is_empty());
        ok(&mut s, "duplicate_objects", json!({ "solid_ids": [3] }));
        assert_eq!(s.doc.undo_label(), Some("MCP: duplicate_objects"));
        let ctx = egui::Context::default();
        super::super::tools_settings::script_set(&mut s, &ctx, "edit.undo_restores_selection", "false")
            .expect("set_setting");
        assert!(!s.settings.edit.undo_restores_selection);
        let n = s.doc.history_len().0;
        ok(&mut s, "select_objects", json!({ "solid_ids": [2] }));
        assert_eq!(s.doc.history_len().0, n);
        assert!(!ok(&mut s, "get_history", json!({}))["selection_steps"].as_bool().unwrap());
    }
}
