use hp_app::AppState;
use hp_editor::optimize::cubemaps::{CubemapReport, Suggestion, size_bytes, size_pixels};
use hp_mcp::protocol::ToolResult;
use serde_json::{Value, json};

use super::semantic::*;
use super::tools::{Args, R, bounds_json, edit, obj_json, v3};
use crate::dispatch::health::cubemaps as engine;

domain! { "cubemaps";
    analyze_cubemaps: Read, "Analyze cubemaps",
        "Analyze the reflections of the map like the Cubemaps tool (Tools > Placement and props > Cubemaps): every face that \
        uses an $envmap env_cubemap material (reflective floors, glass, metal, water), the rooms they form (lines of \
        sight and openings, or the areas of the last visibility compile when there is one), every env_cubemap with \
        its size, sides, room, height above the floor and the reflective faces that take it (VBSP gives each face \
        the nearest env_cubemap in front of it, or the one listing the face in its sides), faces that take the \
        cubemap of another room or one behind a wall, faces far from their cubemap, rooms without a cubemap, \
        cubemaps inside brushes, in the void, at floor level instead of eye height, too close together, too big or \
        too small, too many cubemaps and their memory, and the suggested fixes (suggest_cubemaps places them). \
        Nothing in the map changes. The result is also shown in the Cubemaps window.",
        {
            "rooms": integer("Rooms listed (default 50).", 0, 1000),
            "faces": integer("Reflective faces listed with their cubemap (default 0).", 0, 5000),
            "limit": integer("Findings listed (default 100).", 1, 1000),
        }, [];
    suggest_cubemaps: Edit, "Suggest cubemaps",
        "Suggested env_cubemap positions: one per room with reflective materials and no cubemap that sees it, at eye \
        height (64 units above the floor, lower under a low ceiling), at the weighted centre of the reflective faces, \
        moved out of brushes, several for long rooms, and one where a large reflective area is over 1024 units from the \
        cubemaps of its room, one in open space in front of each group of facade faces that sees no cubemap except \
        through a wall; rooms with little reflective surface (weighted by $envmaptint and the envmap mask) get \
        none; plus sides assignments: faces that would take the cubemap of \
        another room are listed in the sides of a cubemap of their own room (new or existing). apply places them \
        (all, or indices) in one undo step; size sets the cubemapsize of the new ones (0 default, 6 32x32, 7 64x64, \
        8 128x128, 9 256x256). The user does the same with Accept in the Cubemaps window.",
        {
            "apply": boolean("Place the suggestions (default false)."),
            "indices": { "type": "array", "items": { "type": "integer", "minimum": 0 }, "maxItems": 1000, "description": "Indices of the suggestions to apply (default all)." },
            "size": integer("cubemapsize of the new cubemaps (default: the size most cubemaps of the map use, 0 without any).", 0, 12),
        }, [];
    show_cubemap_assignment: Editor, "Show cubemap assignment",
        "Show or hide the cubemap assignment view in the 3D and 2D views: each face coloured by the env_cubemap it \
        will use (reflective faces strong, the others faint), a line from each reflective face to its cubemap (red \
        when it is the cubemap of another room or behind a wall), the cubemaps as boxes of their colour. Off by \
        default; look at it with screenshot_view. window also opens or closes the Cubemaps window, whose suggestions \
        show as ghosts in the views.",
        {
            "show": boolean("Show (true) or hide (false); default toggles."),
            "window": boolean("Open (true) or close (false) the Cubemaps window."),
        }, [];
}

fn suggestion_json(i: usize, s: &Suggestion) -> Value {
    json!({
        "index": i,
        "kind": if s.is_new() { "new" } else { "sides" },
        "origin": v3(s.origin),
        "size": s.size,
        "sides": s.sides,
        "faces": s.faces,
        "room": s.room,
        "cubemap": s.probe,
        "reason": s.reason,
    })
}

fn finding_json(i: usize, f: &hp_editor::optimize::Finding) -> Value {
    json!({
        "index": i,
        "code": f.code,
        "impact": f.impact.id(),
        "message": f.message,
        "objects": f.objects.iter().take(100).map(|o| obj_json(*o)).collect::<Vec<_>>(),
        "points": f.points.iter().take(16).map(|p| v3(*p)).collect::<Vec<_>>(),
    })
}

fn summary(r: &CubemapReport) -> Value {
    json!({
        "cubemaps": r.probes.len(),
        "memory_mb": (r.memory_bytes as f64 / 104_857.6).round() / 10.0,
        "reflective_faces": r.reflective,
        "rooms": r.rooms.len(),
        "faces_other_room": r.wrong,
        "faces_through_wall": r.blocked,
        "faces_without_better_cubemap": r.unfixable,
        "faces_far": r.far,
        "compiler_areas": r.compiler_areas,
        "suggestions": r.suggestions.len(),
        "findings": r.findings.len(),
    })
}

fn analyze_cubemaps(state: &mut AppState, a: &Args<'_>) -> R<ToolResult> {
    let rooms = a.opt_u64("rooms", 0, 1000)?.unwrap_or(50) as usize;
    let faces = a.opt_u64("faces", 0, 5000)?.unwrap_or(0) as usize;
    let limit = a.opt_u64("limit", 1, 1000)?.unwrap_or(100) as usize;
    let r = engine::refresh(state);
    Ok(ToolResult::json(json!({
        "summary": summary(&r),
        "cubemaps": r.probes.iter().map(|p| json!({
            "id": p.id,
            "origin": v3(p.origin),
            "cubemapsize": p.size,
            "pixels": size_pixels(p.size),
            "memory_kb": size_bytes(p.size) / 1024,
            "sides": p.sides.len(),
            "room": p.room,
            "reflective_faces": p.faces,
            "height_above_floor": p.floor_gap.map(|g| g.round()),
            "inside_brush": p.inside,
            "outside_map": p.outside,
        })).collect::<Vec<_>>(),
        "rooms": r.rooms.iter().enumerate().take(rooms).map(|(i, x)| json!({
            "index": i,
            "center": v3(x.center),
            "bounds": bounds_json(Some(x.bounds)),
            "reflective_faces": x.faces.len(),
            "reflective_surface": x.surface.round(),
            "kinds": x.kinds.iter().map(|(k, n)| json!({ "kind": k.id(), "faces": n })).collect::<Vec<_>>(),
            "cubemaps": x.probes.iter().filter_map(|p| r.probes.get(*p).map(|p| p.id)).collect::<Vec<_>>(),
            "faces_other_room": x.wrong,
            "area": x.area,
        })).collect::<Vec<_>>(),
        "faces": r.faces.iter().filter(|f| f.reflective).take(faces).map(|f| json!({
            "solid": f.solid,
            "side": f.side,
            "center": v3(f.center),
            "spot": v3(f.spot),
            "normal": v3(f.normal),
            "kind": f.surface.id(),
            "strength": (f.strength * 100.0).round() / 100.0,
            "cubemap": f.probe.and_then(|p| r.probes.get(p)).map(|p| p.id),
            "from_sides": f.manual,
            "distance": f.distance.round(),
            "through_wall": f.blocked,
            "other_room": f.foreign,
            "room": f.room,
        })).collect::<Vec<_>>(),
        "findings": r.findings.iter().take(limit).enumerate().map(|(i, f)| finding_json(i, f)).collect::<Vec<_>>(),
        "suggestions": r.suggestions.iter().enumerate().map(|(i, s)| suggestion_json(i, s)).collect::<Vec<_>>(),
    })))
}

fn suggest_cubemaps(state: &mut AppState, a: &Args<'_>) -> R<ToolResult> {
    let apply = a.opt_bool("apply")?.unwrap_or(false);
    let size = a.opt_u64("size", 0, 12)?.map(|s| s as i32);
    let indices: Option<Vec<usize>> = match a.get("indices") {
        None => None,
        Some(v) => Some(
            v.as_array()
                .ok_or("indices must be an array of integers")?
                .iter()
                .map(|x| x.as_u64().map(|n| n as usize).ok_or("indices must be an array of integers"))
                .collect::<Result<_, _>>()?,
        ),
    };
    let r = engine::refresh(state);
    let mut out = json!({
        "summary": summary(&r),
        "suggestions": r.suggestions.iter().enumerate().map(|(i, s)| suggestion_json(i, s)).collect::<Vec<_>>(),
    });
    if apply {
        let list = engine::chosen(state, indices.as_deref(), size);
        if list.is_empty() {
            out["applied"] = json!([]);
            return Ok(ToolResult::json(out));
        }
        let ids = edit(state, "suggest_cubemaps", |doc| doc.place_cubemaps(&list))?;
        let after = engine::refresh(state);
        out["applied"] = json!(ids);
        out["after"] = summary(&after);
    }
    Ok(ToolResult::json(out))
}

fn show_cubemap_assignment(state: &mut AppState, a: &Args<'_>) -> R<ToolResult> {
    let show = a.opt_bool("show")?.unwrap_or(!state.runtime.cubemaps.view);
    if let Some(w) = a.opt_bool("window")? {
        state.runtime.cubemaps.show = w;
    }
    state.runtime.cubemaps.view = show;
    let fresh = state.runtime.cubemaps.report.is_none() || state.runtime.cubemaps.stale(state.doc.scene_revision());
    if (show || state.runtime.cubemaps.show) && fresh {
        engine::refresh(state);
    }
    let rt = &state.runtime.cubemaps;
    Ok(ToolResult::json(json!({
        "showing": rt.view,
        "window": rt.show,
        "summary": rt.report.as_deref().map(summary),
    })))
}

#[cfg(test)]
mod tests {
    use glam::DVec3;
    use serde_json::json;

    use super::super::semantic_tests::ok as call;

    fn room(state: &mut hp_app::AppState) {
        use hp_editor::optimize::sample::{block, shell};
        let v = DVec3::new;
        shell(&mut state.doc, v(0.0, 0.0, 0.0), v(512.0, 512.0, 256.0), 16.0, &[]);
        block(&mut state.doc, v(32.0, 32.0, 0.0), v(480.0, 480.0, 2.0), "TILE/TILEFLOOR001A");
    }

    #[test]
    fn analyze_suggest_apply_and_show() {
        let mut state = hp_app::testing::test_state();
        room(&mut state);
        let r = call(&mut state, "analyze_cubemaps", json!({ "faces": 10 }));
        assert_eq!(r["summary"]["cubemaps"], 0);
        assert!(r["summary"]["reflective_faces"].as_u64().unwrap_or(0) >= 1);
        assert!(r["findings"].as_array().is_some_and(|f| f.iter().any(|x| x["code"] == "cubemap_none")));
        assert!(state.runtime.cubemaps.report.is_some(), "the window shows the same report");
        let s = call(&mut state, "suggest_cubemaps", json!({}));
        assert_eq!(s["suggestions"].as_array().map(Vec::len), Some(1));
        let before = state.doc.scene().entities().len();
        let applied = call(&mut state, "suggest_cubemaps", json!({ "apply": true, "size": 7 }));
        assert_eq!(applied["applied"].as_array().map(Vec::len), Some(1));
        assert_eq!(state.doc.scene().entities().len(), before + 1);
        assert_eq!(applied["after"]["cubemaps"], 1);
        let probe = state.doc.scene().entities().values().find(|n| n.entity.classname == "env_cubemap").expect("probe");
        assert_eq!(probe.entity.get("cubemapsize"), Some("7"));
        let z = probe.entity.origin().map(|o| o.z).unwrap_or_default();
        assert!((60.0..=72.0).contains(&z), "eye height: {z}");
        let shown = call(&mut state, "show_cubemap_assignment", json!({ "show": true, "window": true }));
        assert_eq!(shown["showing"], true);
        assert!(state.runtime.cubemaps.view && state.runtime.cubemaps.show);
        let hidden = call(&mut state, "show_cubemap_assignment", json!({}));
        assert_eq!(hidden["showing"], false);
    }
}
