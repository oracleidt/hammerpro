use glam::DVec3;
use hp_app::grid::{GridFrame, GridSource};
use hp_app::settings::{GRID_COORD_LIMIT, GRID_MAX, GRID_MIN, GridMode, GridSpec, MAX_MAJOR_EVERY, ViewGrid};
use hp_app::{AppState, ViewKind};
use hp_mcp::protocol::ToolResult;
use serde_json::{Value, json};

use super::semantic::*;
use super::tools::{Args, R, undo_label, v3};

const VIEWS: &[(&str, ViewKind)] =
    &[("3d", ViewKind::Perspective), ("top", ViewKind::Top), ("front", ViewKind::Front), ("side", ViewKind::Side)];
const VIEW_NAMES: &[&str] = &["3d", "top", "front", "side"];
const MODES: &[&str] = &["global", "per_view", "custom", "selection"];
const SCOPES: &[&str] = &["map", "preset"];

domain! { "grids";
    list_grids: Read, "List the grids",
        "The editing grids: the grid mode (global: one size for every view like Hammer; per_view: each view has \
        its own grid; custom: a named custom grid; selection: the grid attached to the selected objects), the \
        active grid of each view (steps per axis, origin, angles, where it comes from), the grids of the views, \
        the custom grids of the map (with how many objects carry each) and the presets of the settings, the grid \
        of the selection, whether objects use their grid when edited or drawn on.",
        {}, [];
    set_grid_mode: Editor, "Choose the grid",
        "Choose the grid mode and change the global and per view grids (settings, no undo step): mode (global, \
        per_view, custom, selection); custom: the name of a custom grid to make active (mode custom); size: the \
        grid size, any number from 0.01 to 4096 (12, 1.5...) of the global grid, or with view of that view's \
        grid; origin and major_every of a view's grid (view needed); object_grids: editing an object that carries \
        a grid or drawing on it snaps to its grid; plane_3d: draw custom grids in the 3D view.",
        {
            "mode": choice("Grid mode.", MODES),
            "custom": string("Name of a custom grid to make active."),
            "view": choice("The view whose grid size, origin and major_every change (per view mode).", VIEW_NAMES),
            "size": number("Grid size in units.", GRID_MIN, GRID_MAX),
            "origin": vec3("Origin of the view's grid (a node sits there)."),
            "major_every": integer("Major line every N lines (0: the view setting).", 0, i64::from(MAX_MAJOR_EVERY)),
            "object_grids": boolean("Objects use their grid."),
            "plane_3d": boolean("Draw custom grids in the 3D view."),
        }, [];
    rotate_grid: Edit, "Rotate the grid",
        "Rotate the active grid of a 2D view by an angle around the axis of that view (any value in degrees, \
        0 resets, 45 is the quick turn): angle sets it, delta adds to it, align turns it to the direction of the \
        selected face, brush edge or entity (the longest edge seen in that view). view (top, front, side; \
        default: the view of the grid controls) picks the view. For the global and per view grids the angle \
        is kept in the settings; for a custom grid it changes its angles (one undo step in the map). Every tool \
        then snaps to the rotated grid and the 2D view draws it. The result lists the grids with the new angle.",
        {
            "angle": number("Angle of the grid in degrees.", -360.0, 360.0),
            "delta": number("Degrees to add to the current angle.", -360.0, 360.0),
            "align": boolean("Turn the grid to the selected face, brush or entity."),
            "view": choice("The 2D view to rotate the grid in (default: the target view).", VIEW_NAMES),
        }, [];
    create_grid: Edit, "Create a custom grid",
        "Create a custom grid: name, step (one number, or [x, y, z] for a different step per axis, decimals \
        allowed), origin, angles [pitch, yaw, roll] (to follow a rotated building or a slope), major_every, \
        limits (bounds {min, max}: outside, the global grid applies), color [r, g, b]. With from \"selection\" \
        or \"face\" the grid is fitted to the selected object (its axes, a node on its corner) or face (its \
        plane), the other fields changing that. scope map (default, saved with the map, one undo step) or preset \
        (saved in the settings, offered in every map). activate (default true) makes it the active grid.",
        {
            "name": string("Unique name (at most 48 characters)."),
            "step": step_schema(),
            "origin": vec3("A node of the grid."),
            "angles": vec3("Orientation [pitch, yaw, roll] in degrees."),
            "major_every": integer("Major line every N lines (0: the view setting).", 0, i64::from(MAX_MAJOR_EVERY)),
            "bounds": bounds_schema(),
            "color": color_schema(),
            "from": choice("Fit the grid to the selected object or face.", &["selection", "face"]),
            "scope": choice("map (default) or preset.", SCOPES),
            "activate": boolean("Make it the active grid (default true)."),
        }, ["name"];
    edit_grid: Edit, "Edit a custom grid",
        "Change a custom grid (list_grids): new_name (objects carrying it follow), step, origin, angles, \
        major_every, bounds (null: no limits), color (null: the theme colour). scope map (default) or preset.",
        {
            "name": string("The grid to change."),
            "new_name": string("New name."),
            "step": step_schema(),
            "origin": vec3("A node of the grid."),
            "angles": vec3("Orientation [pitch, yaw, roll] in degrees."),
            "major_every": integer("Major line every N lines (0: the view setting).", 0, i64::from(MAX_MAJOR_EVERY)),
            "bounds": bounds_schema(),
            "color": color_schema(),
            "scope": choice("map (default) or preset.", SCOPES),
        }, ["name"];
    delete_grid: Destructive, "Delete a custom grid",
        "Delete a custom grid of the map (the objects carrying it lose it, in the same undo step) or a preset.",
        { "name": string("The grid to delete."), "scope": choice("map (default) or preset.", SCOPES) }, ["name"];
    attach_grid: Edit, "Attach a grid to objects",
        "Attach a custom grid to objects (default: the selection) or groups, so editing them or drawing on them \
        snaps to it (a preset is copied into the map first); name null or absent detaches their grid. Saved in \
        the editor block of the objects, which Hammer ignores.",
        {
            "name": string("The grid (absent: detach)."),
            "ids": target_ids(), "solid_ids": target_solid_ids(),
            "group_ids": ids("Group ids."),
        }, [];
}

fn step_schema() -> Value {
    json!({
        "description": "Step: one number for every axis, or [x, y, z].",
        "oneOf": [
            { "type": "number", "minimum": GRID_MIN, "maximum": GRID_MAX },
            { "type": "array", "items": { "type": "number", "minimum": GRID_MIN, "maximum": GRID_MAX }, "minItems": 3, "maxItems": 3 },
        ],
    })
}

fn bounds_schema() -> Value {
    json!({
        "description": "Limits {min: [x, y, z], max: [x, y, z]}, or null for none.",
        "type": ["object", "null"],
        "properties": { "min": vec3("Minimum."), "max": vec3("Maximum.") },
        "required": ["min", "max"],
        "additionalProperties": false,
    })
}

fn color_schema() -> Value {
    json!({
        "description": "Colour [r, g, b] 0 to 255, or null for the theme colour.",
        "type": ["array", "null"],
        "items": { "type": "integer", "minimum": 0, "maximum": 255 },
        "minItems": 3,
        "maxItems": 3,
    })
}

fn view_name(kind: ViewKind) -> &'static str {
    VIEWS.iter().find(|(_, k)| *k == kind).map_or("top", |(n, _)| n)
}

fn view_kind(name: &str) -> ViewKind {
    VIEWS.iter().find(|(n, _)| *n == name).map_or(ViewKind::Top, |(_, k)| *k)
}

fn spec_json(g: &GridSpec) -> Value {
    json!({
        "name": g.name,
        "step": g.step,
        "origin": g.origin,
        "angles": g.angles,
        "major_every": g.major_every,
        "bounds": g.bounds.map(|[a, b]| json!({ "min": a, "max": b })),
        "color": g.color,
    })
}

fn view_grid_json(g: &ViewGrid) -> Value {
    json!({ "size": g.size, "origin": g.origin, "major_every": g.major_every, "angle": g.angle })
}

fn source_json(s: &GridSource) -> Value {
    match s {
        GridSource::Global => json!({ "kind": "global" }),
        GridSource::View(k) => json!({ "kind": "view", "view": view_name(*k) }),
        GridSource::Map(n) => json!({ "kind": "map", "name": n }),
        GridSource::Preset(n) => json!({ "kind": "preset", "name": n }),
    }
}

fn frame_json(f: &GridFrame) -> Value {
    json!({
        "step": v3(f.step),
        "origin": v3(f.origin),
        "angles": v3(hp_geom::clean_angles(hp_geom::matrix_angles(&f.axes))),
        "rotated": f.is_rotated(),
        "size": f.nominal(),
    })
}

pub(super) fn grids_json(state: &AppState) -> Value {
    let g = &state.settings.grid;
    let active: Vec<Value> = VIEWS
        .iter()
        .map(|(name, kind)| {
            let a = state.active_grid(*kind);
            json!({
                "view": name,
                "source": source_json(&a.source),
                "angle": state.grid_angle(*kind),
                "grid": frame_json(&a.frame),
            })
        })
        .collect();
    let map: Vec<Value> = state
        .map_grids()
        .iter()
        .map(|s| {
            let mut v = spec_json(s);
            v["objects"] = json!(state.doc.grid_user_count(&s.name));
            v
        })
        .collect();
    let target = state.target_grid();
    json!({
        "mode": g.mode.name(),
        "custom": if g.custom.is_empty() { Value::Null } else { json!(g.custom) },
        "global_size": state.settings.edit.grid_size,
        "snap": state.settings.edit.snap_to_grid,
        "object_grids": g.object_grids,
        "plane_3d": g.plane_3d,
        "target_view": view_name(state.grid_target_view()),
        "target": {
            "source": source_json(&target.source),
            "angle": state.grid_angle(state.grid_target_view()),
            "grid": frame_json(&target.frame),
        },
        "active": active,
        "per_view": {
            "3d": view_grid_json(&g.per_view.perspective),
            "top": view_grid_json(&g.per_view.top),
            "front": view_grid_json(&g.per_view.front),
            "side": view_grid_json(&g.per_view.side),
        },
        "map_grids": map,
        "presets": g.presets.iter().map(spec_json).collect::<Vec<_>>(),
        "selection_grid": state.selection_grid_name(),
    })
}

fn list_grids(state: &mut AppState, _a: &Args<'_>) -> R<ToolResult> {
    Ok(ToolResult::json(grids_json(state)))
}

fn rotate_grid(state: &mut AppState, a: &Args<'_>) -> R<ToolResult> {
    let angle = a.opt_f64("angle", -360.0, 360.0)?;
    let delta = a.opt_f64("delta", -360.0, 360.0)?;
    let align = a.opt_bool("align")?.unwrap_or(false);
    if angle.is_none() && delta.is_none() && !align {
        return Err("give angle, delta or align".to_owned());
    }
    let kind = a.opt_enum("view", VIEW_NAMES)?.map_or_else(|| state.grid_target_view(), |v| view_kind(&v));
    in_step(state, "rotate_grid", |state| {
        if align {
            state.align_grid_to_selection(kind)?;
        }
        if let Some(angle) = angle {
            state.set_grid_angle(kind, angle)?;
        }
        if let Some(delta) = delta {
            state.rotate_grid(kind, delta)?;
        }
        Ok(())
    })?;
    Ok(ToolResult::json(grids_json(state)))
}

fn set_grid_mode(state: &mut AppState, a: &Args<'_>) -> R<ToolResult> {
    if a.map.is_empty() {
        return Err("give mode, custom, size, view, origin, major_every, object_grids or plane_3d".to_owned());
    }
    let view = a.opt_enum("view", VIEW_NAMES)?.map(|v| view_kind(&v));
    let size = a.opt_f64("size", GRID_MIN, GRID_MAX)?;
    let origin = a.opt_vec3("origin")?;
    let major = a.opt_u64("major_every", 0, u64::from(MAX_MAJOR_EVERY))?;
    if view.is_none() && (origin.is_some() || major.is_some()) {
        return Err("origin and major_every change the grid of a view: give view".to_owned());
    }
    if let Some(o) = origin
        && o.abs().max_element() > GRID_COORD_LIMIT
    {
        return Err(format!("origin must be within +/-{GRID_COORD_LIMIT}"));
    }
    if let Some(name) = a.opt_str("custom", 128)? {
        state.use_custom_grid(name)?;
    }
    if let Some(mode) = a.opt_enum("mode", MODES)? {
        let mode = GridMode::from_name(&mode).unwrap_or_default();
        if mode == GridMode::Custom && state.find_grid(&state.settings.grid.custom).is_none() {
            return Err("no custom grid is chosen: give custom (list_grids shows them) or create_grid".to_owned());
        }
        state.set_grid_mode(mode);
    }
    match view {
        Some(kind) => {
            let g = state.settings.grid.per_view.get_mut(kind);
            if let Some(s) = size {
                g.size = s;
            }
            if let Some(o) = origin {
                g.origin = o.to_array();
            }
            if let Some(m) = major {
                g.major_every = m as u32;
            }
            *g = g.sanitized();
        }
        None => {
            if let Some(s) = size {
                state.settings.edit.grid_size = hp_app::settings::clamp_grid(s, state.settings.edit.grid_size);
            }
        }
    }
    if let Some(b) = a.opt_bool("object_grids")? {
        state.settings.grid.object_grids = b;
    }
    if let Some(b) = a.opt_bool("plane_3d")? {
        state.settings.grid.plane_3d = b;
    }
    state.settings_dirty = true;
    Ok(ToolResult::json(grids_json(state)))
}

fn in_map(a: &Args<'_>) -> R<bool> {
    Ok(a.opt_enum("scope", SCOPES)?.as_deref() != Some("preset"))
}

fn grid_name(a: &Args<'_>, key: &str) -> R<Option<String>> {
    match a.opt_str(key, 256)? {
        None => Ok(None),
        Some(n) => {
            let clean = hp_editor::clean_grid_name(n);
            if clean.is_empty() || clean != n.trim() {
                return Err(format!(
                    "{key} must be a name of 1 to {} characters without quotes, braces or line breaks",
                    hp_editor::MAX_GRID_NAME
                ));
            }
            Ok(Some(clean))
        }
    }
}

fn apply_fields(spec: &mut GridSpec, a: &Args<'_>) -> R<()> {
    if let Some(v) = a.get("step") {
        let bad = || format!("step must be a number or [x, y, z] from {GRID_MIN} to {GRID_MAX}");
        let ok = |x: &Value| x.as_f64().filter(|f| f.is_finite() && (GRID_MIN..=GRID_MAX).contains(f));
        spec.step = match v {
            Value::Array(arr) if arr.len() == 3 => {
                let mut s = [0.0; 3];
                for (o, x) in s.iter_mut().zip(arr) {
                    *o = ok(x).ok_or_else(bad)?;
                }
                s
            }
            other => [ok(other).ok_or_else(bad)?; 3],
        };
    }
    if let Some(o) = a.opt_vec3("origin")? {
        if o.abs().max_element() > GRID_COORD_LIMIT {
            return Err(format!("origin must be within +/-{GRID_COORD_LIMIT}"));
        }
        spec.origin = o.to_array();
    }
    if let Some(an) = a.opt_vec3("angles")? {
        if an.abs().max_element() > 360.0 {
            return Err("angles must be from -360 to 360 degrees".to_owned());
        }
        spec.angles = an.to_array();
    }
    if let Some(m) = a.opt_u64("major_every", 0, u64::from(MAX_MAJOR_EVERY))? {
        spec.major_every = m as u32;
    }
    match a.map.get("bounds") {
        Some(Value::Null) => spec.bounds = None,
        Some(Value::Object(b)) => {
            let read = |k: &str| -> R<DVec3> {
                let bad = || format!("bounds.{k} must be [x, y, z] within +/-{GRID_COORD_LIMIT}");
                let arr = b.get(k).and_then(Value::as_array).filter(|x| x.len() == 3).ok_or_else(bad)?;
                let mut out = [0.0; 3];
                for (o, x) in out.iter_mut().zip(arr) {
                    *o = x.as_f64().filter(|f| f.is_finite() && f.abs() <= GRID_COORD_LIMIT).ok_or_else(bad)?;
                }
                Ok(DVec3::from_array(out))
            };
            let (min, max) = (read("min")?, read("max")?);
            if (max - min).min_element() <= 1e-6 {
                return Err("bounds must have a size on every axis (max above min)".to_owned());
            }
            spec.bounds = Some([min.to_array(), max.to_array()]);
        }
        Some(_) => return Err("bounds must be {min, max} or null".to_owned()),
        None => {}
    }
    match a.map.get("color") {
        Some(Value::Null) => spec.color = None,
        Some(Value::Array(arr)) if arr.len() == 3 => {
            let mut c = [0u8; 3];
            for (o, x) in c.iter_mut().zip(arr) {
                *o = x.as_u64().filter(|n| *n <= 255).ok_or("color must be [r, g, b] from 0 to 255")? as u8;
            }
            spec.color = Some(c);
        }
        Some(_) => return Err("color must be [r, g, b] from 0 to 255, or null".to_owned()),
        None => {}
    }
    Ok(())
}

fn in_step<T>(state: &mut AppState, tool: &str, f: impl FnOnce(&mut AppState) -> R<T>) -> R<T> {
    if state.doc.in_transaction() {
        return Err("Hammer Pro is in the middle of another edit (a drag in a view?); try again".to_owned());
    }
    state.doc.begin(undo_label(tool));
    match f(state) {
        Ok(v) => {
            state.doc.commit();
            Ok(v)
        }
        Err(e) => {
            state.doc.cancel();
            Err(e)
        }
    }
}

fn create_grid(state: &mut AppState, a: &Args<'_>) -> R<ToolResult> {
    let name = grid_name(a, "name")?.ok_or("name is required")?;
    let map = in_map(a)?;
    if state.find_grid(&name).is_some_and(|(_, s)| matches!(s, GridSource::Map(_)) == map) {
        return Err(format!("a grid is already named \"{name}\": use edit_grid"));
    }
    let mut spec = match a.opt_enum("from", &["selection", "face"])?.as_deref() {
        Some("face") => state.grid_from_face(&name).ok_or("select a face first (select_faces)")?,
        Some(_) => state.grid_from_selection(&name).ok_or("select an object first")?,
        None => GridSpec::uniform(&name, state.settings.edit.grid_size),
    };
    apply_fields(&mut spec, a)?;
    in_step(state, "create_grid", |state| state.put_custom_grid("", spec, map, false))?;
    if a.opt_bool("activate")?.unwrap_or(true) {
        state.use_custom_grid(&name)?;
    }
    Ok(ToolResult::json(grids_json(state)))
}

fn edit_grid(state: &mut AppState, a: &Args<'_>) -> R<ToolResult> {
    let name = grid_name(a, "name")?.ok_or("name is required")?;
    let map = in_map(a)?;
    let list = if map { state.map_grids() } else { state.settings.grid.presets.clone() };
    let mut spec = list
        .into_iter()
        .find(|g| g.name.eq_ignore_ascii_case(&name))
        .ok_or_else(|| format!("no {} grid is named \"{name}\" (list_grids)", if map { "map" } else { "preset" }))?;
    let old = spec.name.clone();
    if let Some(n) = grid_name(a, "new_name")? {
        spec.name = n;
    }
    apply_fields(&mut spec, a)?;
    in_step(state, "edit_grid", |state| state.put_custom_grid(&old, spec, map, false))?;
    Ok(ToolResult::json(grids_json(state)))
}

fn delete_grid(state: &mut AppState, a: &Args<'_>) -> R<ToolResult> {
    let name = grid_name(a, "name")?.ok_or("name is required")?;
    let map = in_map(a)?;
    let deleted = in_step(state, "delete_grid", |state| state.delete_custom_grid(&name, map))?;
    if !deleted {
        return Err(format!("no {} grid is named \"{name}\" (list_grids)", if map { "map" } else { "preset" }));
    }
    Ok(ToolResult::json(grids_json(state)))
}

fn attach_grid(state: &mut AppState, a: &Args<'_>) -> R<ToolResult> {
    let name = grid_name(a, "name")?;
    let groups = a.ids("group_ids")?;
    for g in &groups {
        if state.doc.scene().group(*g).is_none() {
            return Err(format!("no group has the id {g}"));
        }
    }
    let objs = if groups.is_empty() || a.get("ids").is_some() || a.get("solid_ids").is_some() {
        targets(state, a)?
    } else {
        Vec::new()
    };
    let spec = match &name {
        Some(n) => Some(state.find_grid(n).ok_or_else(|| format!("no grid is named \"{n}\" (list_grids)"))?),
        None => None,
    };
    let changed = in_step(state, "attach_grid", |state| {
        if let Some((spec, GridSource::Preset(_))) = &spec {
            state.put_custom_grid("", spec.clone(), true, false)?;
        }
        let grid = spec.as_ref().map(|(s, _)| s.name.clone());
        state.doc.set_object_grid(&objs, &groups, grid.as_deref()).map_err(|e| e.to_string())
    })?;
    let mut out = grids_json(state);
    out["changed"] = json!(changed);
    Ok(ToolResult::json(out))
}

#[cfg(test)]
mod tests {
    use super::super::semantic_tests::{err, ok};
    use super::*;

    fn call(state: &mut AppState, name: &str, args: Value) -> Value {
        ok(state, name, args)
    }

    fn fails(state: &mut AppState, name: &str, args: Value) -> String {
        err(state, name, args)
    }

    #[test]
    fn rotate_grid_through_mcp() {
        let mut state = hp_app::testing::test_state();
        fails(&mut state, "rotate_grid", json!({}));
        let v = call(&mut state, "rotate_grid", json!({ "view": "top", "angle": 45 }));
        assert_eq!(v["active"][1]["angle"], 45.0);
        assert_eq!(v["per_view"]["top"]["angle"], 45.0);
        assert!(state.grid_frame(ViewKind::Top).is_rotated());
        assert!(!state.grid_frame(ViewKind::Front).is_rotated());
        let v = call(&mut state, "rotate_grid", json!({ "view": "top", "delta": -15.5 }));
        assert_eq!(v["active"][1]["angle"], 29.5);
        let v = call(&mut state, "rotate_grid", json!({ "view": "top", "angle": 0 }));
        assert_eq!(v["active"][1]["angle"], 0.0);
        fails(&mut state, "rotate_grid", json!({ "angle": 400 }));
        fails(&mut state, "rotate_grid", json!({ "align": true }));
        call(&mut state, "create_grid", json!({ "name": "R", "step": 8 }));
        let v = call(&mut state, "rotate_grid", json!({ "view": "top", "angle": 30 }));
        assert_eq!(v["map_grids"][0]["angles"][1], 30.0);
        assert!(state.doc.undo_label().is_some_and(|l| l.contains("rotate_grid")));
    }

    #[test]
    fn grids_through_mcp() {
        let mut state = hp_app::testing::test_state();
        let v = call(&mut state, "list_grids", json!({}));
        assert_eq!(v["mode"], "global");
        assert_eq!(v["active"][1]["grid"]["size"], 16.0);
        call(&mut state, "set_grid_mode", json!({ "mode": "per_view" }));
        let v = call(&mut state, "set_grid_mode", json!({ "view": "front", "size": 12, "origin": [0, 4, 0] }));
        assert_eq!(v["per_view"]["front"]["size"], 12.0);
        assert_eq!(state.grid_frame(ViewKind::Front).nominal(), 12.0);
        assert_eq!(state.grid_frame(ViewKind::Top).nominal(), 16.0);
        fails(&mut state, "set_grid_mode", json!({ "origin": [1, 2, 3] }));
        fails(&mut state, "set_grid_mode", json!({ "size": 0 }));
        let v = call(
            &mut state,
            "create_grid",
            json!({ "name": "Stairs", "step": [12, 12, 8], "angles": [0, 30, 0], "bounds": { "min": [0, 0, 0], "max": [512, 512, 256] } }),
        );
        assert_eq!(v["mode"], "custom");
        assert_eq!(v["map_grids"][0]["step"], json!([12.0, 12.0, 8.0]));
        assert_eq!(state.doc.undo_label(), Some("MCP: create_grid"));
        fails(&mut state, "create_grid", json!({ "name": "Stairs" }));
        fails(&mut state, "create_grid", json!({ "name": "bad\"name" }));
        fails(&mut state, "create_grid", json!({ "name": "x", "step": 0 }));
        call(
            &mut state,
            "edit_grid",
            json!({ "name": "stairs", "new_name": "Steps", "color": [255, 0, 0], "bounds": null }),
        );
        let v = call(&mut state, "list_grids", json!({}));
        assert_eq!(v["map_grids"][0]["name"], "Steps");
        assert_eq!(v["map_grids"][0]["bounds"], Value::Null);
        assert_eq!(v["custom"], "Steps", "the active grid follows the rename");
        let solid = state.doc.scene().solids().keys().next().copied().expect("a solid");
        let v = call(&mut state, "attach_grid", json!({ "name": "Steps", "solid_ids": [solid] }));
        assert_eq!(v["changed"], 1);
        assert_eq!(state.doc.object_grid(hp_editor::ObjRef::Solid(solid)), Some("Steps"));
        assert_eq!(v["map_grids"][0]["objects"], 1);
        call(&mut state, "attach_grid", json!({ "solid_ids": [solid] }));
        assert_eq!(state.doc.object_grid(hp_editor::ObjRef::Solid(solid)), None);
        fails(&mut state, "attach_grid", json!({ "name": "nothing", "solid_ids": [solid] }));
        call(&mut state, "create_grid", json!({ "name": "Fine", "step": 1.5, "scope": "preset", "activate": false }));
        assert_eq!(state.settings.grid.presets.len(), 1);
        call(&mut state, "attach_grid", json!({ "name": "Fine", "solid_ids": [solid] }));
        assert!(state.map_grids().iter().any(|g| g.name == "Fine"));
        call(&mut state, "delete_grid", json!({ "name": "Fine" }));
        assert_eq!(state.doc.object_grid(hp_editor::ObjRef::Solid(solid)), None);
        call(&mut state, "delete_grid", json!({ "name": "Fine", "scope": "preset" }));
        fails(&mut state, "delete_grid", json!({ "name": "Fine", "scope": "preset" }));
        let v = call(&mut state, "set_grid_mode", json!({ "mode": "global", "size": 8 }));
        assert_eq!(v["mode"], "global");
        assert_eq!(state.grid_size(), 8.0);
    }
}
