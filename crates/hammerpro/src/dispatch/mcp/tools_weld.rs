use hp_app::AppState;
use hp_geom::VertexEditOptions;
use hp_mcp::protocol::ToolResult;
use hp_tools_mesh::vertex::weld::{self, WeldReport, WeldTarget};
use serde_json::{Value, json};

use super::semantic::*;
use super::tools::{Args, R, edit};

domain! { "vertex welding";
    weld_brush_vertices: Edit, "Weld brush vertices",
        "Advanced vertex welding of the vertex tool, in one undo step. mode vertices: merge the listed vertices \
        of one solid (indices from get_brush_vertices) into their center, or into the first or last one (lowest \
        or highest index); refused with the reason when the solid would become concave or flat. mode distance: \
        merge every pair of vertices of the solids closer than distance (default 1 unit), repeated until \
        stable; solids that would become invalid are left as they are and listed in refused. mode cleanup: \
        merge vertices closer than 0.01 and remove the degenerate faces and edges and the unused faces of the \
        solids. Welding already drops the faces it collapses.",
        {
            "mode": choice("What to weld.", &["vertices", "distance", "cleanup"]),
            "solid": integer("Solid id (mode vertices).", 1, i64::from(u32::MAX)),
            "vertices": { "type": "array", "items": { "type": "integer", "minimum": 0 }, "minItems": 2, "maxItems": 4096, "description": "Vertex indices to weld (mode vertices, get_brush_vertices)." },
            "target": choice("Where the welded vertices go (mode vertices, default center).", &["center", "first", "last"]),
            "distance": number("Weld distance in units (mode distance, default 1).", 0.001, weld::MAX_WELD_DISTANCE),
            "ids": target_ids(),
            "solid_ids": target_solid_ids(),
            "uv_lock": boolean("Textures follow the vertices (default true)."),
        }, ["mode"];
}

fn report_json(r: &WeldReport) -> Value {
    json!({
        "solids_changed": r.solids_changed,
        "vertices_welded": r.vertices_welded,
        "faces_removed": r.faces_removed,
        "refused": r.refused.iter().map(|(id, why)| json!({ "solid": id, "reason": why })).collect::<Vec<_>>(),
    })
}

fn weld_brush_vertices(state: &mut AppState, a: &Args<'_>) -> R<ToolResult> {
    let mode = a.opt_enum("mode", &["vertices", "distance", "cleanup"])?.ok_or("mode is required")?;
    let opts = VertexEditOptions { uv_lock: a.opt_bool("uv_lock")?.unwrap_or(true) };
    let report = if mode == "vertices" {
        let id = solid_id(&state.doc, a, "solid")?;
        let target = match a.opt_enum("target", &["center", "first", "last"])? {
            Some(t) => WeldTarget::parse(&t).unwrap_or_default(),
            None => WeldTarget::Center,
        };
        let list = a.get("vertices").and_then(Value::as_array).ok_or("vertices must be an array of indices")?;
        let indices: Vec<usize> = list.iter().filter_map(Value::as_u64).map(|i| i as usize).collect();
        if indices.len() != list.len() {
            return Err("vertices must be indices (get_brush_vertices)".to_owned());
        }
        edit(state, "weld_brush_vertices", |doc| {
            weld::weld_vertices_in_solid(doc, id, &indices, target, opts).map_err(hp_editor::EditError::Invalid)
        })?
    } else {
        let solids = target_solids(state, a, "solid_ids")?;
        let report = if mode == "distance" {
            let distance =
                a.opt_f64("distance", 0.001, weld::MAX_WELD_DISTANCE)?.unwrap_or(weld::DEFAULT_WELD_DISTANCE);
            edit(state, "weld_brush_vertices", |doc| Ok(weld::weld_solids_by_distance(doc, &solids, distance, opts)))?
        } else {
            edit(state, "weld_brush_vertices", |doc| Ok(weld::cleanup_solids(doc, &solids, opts)))?
        };
        if report.solids_changed == 0 && !report.refused.is_empty() {
            let why: Vec<String> = report.refused.iter().map(|(id, w)| format!("solid {id}: {w}")).collect();
            return Err(format!("nothing welded, the solids would become invalid ({})", why.join("; ")));
        }
        report
    };
    Ok(ToolResult::json(report_json(&report)))
}

#[cfg(test)]
mod tests {
    use glam::DVec3;
    use serde_json::json;

    use super::super::semantic_tests::{err, ok};

    fn new_box(s: &mut hp_app::AppState) -> u32 {
        let bounds = hp_geom::Aabb::new(DVec3::ZERO, DVec3::new(128.0, 128.0, 64.0));
        let params = hp_geom::PrimitiveParams { bounds, ..Default::default() };
        s.doc.create_primitive(&params, hp_editor::Owner::World).unwrap()[0]
    }

    fn index_of(s: &hp_app::AppState, id: u32, p: DVec3) -> usize {
        s.doc.solid_vertices(id).unwrap().0.iter().position(|q| q.distance(p) < 1e-6).unwrap()
    }

    #[test]
    fn weld_modes_work_and_refuse_concave_results() {
        let mut s = hp_app::testing::test_state();
        let id = new_box(&mut s);
        let corner = index_of(&s, id, DVec3::new(0.0, 0.0, 64.0));
        let diagonal = index_of(&s, id, DVec3::new(128.0, 128.0, 64.0));
        let before = s.doc.history_len();
        let why = err(
            &mut s,
            "weld_brush_vertices",
            json!({ "mode": "vertices", "solid": id, "vertices": [corner, diagonal] }),
        );
        assert!(why.contains("invalid"), "{why}");
        assert_eq!(s.doc.history_len(), before);
        err(&mut s, "weld_brush_vertices", json!({ "mode": "vertices", "solid": id, "vertices": [corner] }));
        err(&mut s, "weld_brush_vertices", json!({ "mode": "vertices", "solid": id, "vertices": [corner, 99] }));
        err(&mut s, "weld_brush_vertices", json!({ "mode": "vertices", "vertices": [0, 1] }));

        let far = index_of(&s, id, DVec3::new(128.0, 0.0, 64.0));
        ok(&mut s, "move_brush_vertices", json!({ "solid": id, "vertices": [far], "offset": [-126, 0, 0] }));
        let (a, b) = (index_of(&s, id, DVec3::new(0.0, 0.0, 64.0)), index_of(&s, id, DVec3::new(2.0, 0.0, 64.0)));
        let steps = s.doc.history_len().0;
        let r = ok(
            &mut s,
            "weld_brush_vertices",
            json!({ "mode": "vertices", "solid": id, "vertices": [a, b], "target": "first" }),
        );
        assert_eq!((r["solids_changed"].as_u64(), r["vertices_welded"].as_u64()), (Some(1), Some(1)));
        assert_eq!(s.doc.history_len().0, steps + 1);
        assert_eq!(s.doc.solid_vertices(id).unwrap().0.len(), 7);
        assert!(s.doc.undo());
        assert_eq!(s.doc.solid_vertices(id).unwrap().0.len(), 8);

        let r = ok(&mut s, "weld_brush_vertices", json!({ "mode": "distance", "solid_ids": [id], "distance": 3 }));
        assert_eq!(r["vertices_welded"], 1);
        let r = ok(&mut s, "weld_brush_vertices", json!({ "mode": "distance", "solid_ids": [id], "distance": 3 }));
        assert_eq!(r["solids_changed"], 0);
        let r = ok(&mut s, "weld_brush_vertices", json!({ "mode": "cleanup", "solid_ids": [id] }));
        assert_eq!(r["solids_changed"], 0);
        err(&mut s, "weld_brush_vertices", json!({ "mode": "distance", "solid_ids": [id], "distance": 0 }));
        err(&mut s, "weld_brush_vertices", json!({ "mode": "weld" }));
    }
}
