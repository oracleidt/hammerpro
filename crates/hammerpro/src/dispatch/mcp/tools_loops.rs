use glam::DVec3;
use hp_app::runtime::EdgeRequest;
use hp_app::{Action, AppState, ToolId};
use hp_editor::{FaceRef, Id, ObjRef, SelectOp};
use hp_geom::VertexEditOptions;
use hp_mcp::protocol::ToolResult;
use hp_tools_mesh::disp::loops::{self as disp_loops, LoopAlong, LoopEdit};
use hp_tools_mesh::vertex::loops::{EdgeCmd, EdgeSet, Seed, compute_edges};
use hp_tools_mesh::vertex::xform::{LoopSpec, RotateAxis, transform_loop};
use serde_json::{Value, json};

use super::semantic::*;
use super::tools::{AppFn, Args, Outcome, R, coord_ok, v3};

const MAX_SEEDS: usize = 256;
const MAX_PASSES: u64 = 64;

domain! { "edge loops";
    select_edge_loop: Editor, "Select edge loop",
        "Find the edge loop through seed edges, like Alt+click on an edge in the vertex tool. The loop runs \
        straight through vertices where four edges meet, follows the boundary of a face at the corners of a \
        brush, goes across the brushes that share edges, follows the edge of a displacement grid and stops at \
        vertices where five or more edges meet. toward picks the face when a loop could go around either of \
        two. Answers the edges (solid, index for bevel_edges, both ends), the vertices and whether the loop \
        is closed. Bevel the whole loop by giving the edges to bevel_edges, or move it with move_edge_loop.",
        {
            "solid": integer("Solid id of the seed edges given as indices or points.", 1, i64::from(u32::MAX)),
            "edge": { "description": "One seed edge: an index from get_brush_edges or [[x, y, z], [x, y, z]] (on solid)." },
            "edges": { "type": "array", "maxItems": 256, "description": "Seed edges: indices, [[x, y, z], [x, y, z]] pairs, or {solid, edge} / {solid, a, b} objects." },
            "solid_ids": ids("The brushes the loop may run through (default: the seed brushes, the selected brushes and the brushes touching them, up to 256)."),
            "toward": vec3("Direction toward the viewer: at the corners of a brush a loop follows the face that faces this way (default: the larger face)."),
            "select": boolean("Also select the result in the vertex tool and switch to it (default false)."),
        }, [];
    select_edge_ring: Editor, "Select edge ring",
        "Find the edge ring through seed edges, like Ctrl+Alt+click on an edge in the vertex tool: the parallel \
        edges found by crossing four sided faces (the four edges of a box that run the same way, or all the \
        rungs of a ladder). Answers the same as select_edge_loop.",
        {
            "solid": integer("Solid id of the seed edges given as indices or points.", 1, i64::from(u32::MAX)),
            "edge": { "description": "One seed edge: an index from get_brush_edges or [[x, y, z], [x, y, z]] (on solid)." },
            "edges": { "type": "array", "maxItems": 256, "description": "Seed edges: indices, [[x, y, z], [x, y, z]] pairs, or {solid, edge} / {solid, a, b} objects." },
            "solid_ids": ids("The brushes the loop may run through (default: the seed brushes, the selected brushes and the brushes touching them, up to 256)."),
            "toward": vec3("Direction toward the viewer: at the corners of a brush a loop follows the face that faces this way (default: the larger face)."),
            "select": boolean("Also select the result in the vertex tool and switch to it (default false)."),
        }, [];
    grow_edge_selection: Editor, "Grow or shrink an edge selection",
        "Grow, shrink or take the boundary of a selection given by seed edges, like the Grow selection, Shrink \
        selection and Select boundary commands of the vertex tool. grow adds every vertex next to the selected \
        ones, times steps, shrink drops the vertices that touch an unselected vertex, boundary keeps the edges \
        on the border of the selected region (or the vertices with an unselected neighbour when no face is \
        fully selected). The selection is the seed edges and their vertices; the answer lists the edges and \
        vertices of the result.",
        {
            "solid": integer("Solid id of the seed edges given as indices or points.", 1, i64::from(u32::MAX)),
            "edge": { "description": "One seed edge: an index from get_brush_edges or [[x, y, z], [x, y, z]] (on solid)." },
            "edges": { "type": "array", "maxItems": 256, "description": "Seed edges: indices, [[x, y, z], [x, y, z]] pairs, or {solid, edge} / {solid, a, b} objects." },
            "solid_ids": ids("The brushes the loop may run through (default: the seed brushes, the selected brushes and the brushes touching them, up to 256)."),
            "toward": vec3("Direction toward the viewer: at the corners of a brush a loop follows the face that faces this way (default: the larger face)."),
            "select": boolean("Also select the result in the vertex tool and switch to it (default false)."),
            "mode": choice("What to do.", &["grow", "shrink", "boundary"]),
            "times": integer("Steps for grow and shrink (default 1).", 1, 64),
        }, ["mode"];
    move_edge_loop: Edit, "Move an edge loop",
        "Move, scale, rotate or slide the vertices of an edge loop (or ring, or just the seed edges) as one, in \
        one undo step, like dragging the loop with the gizmo, Scale vertices, Rotate vertices and Slide edge \
        loop in the vertex tool. expand says what the seeds grow into: loop (default), ring or none. move adds \
        delta; scale multiplies the distances to center by factor; rotate turns degrees around the axis x, y, \
        z or normal (the normal of the plane of the loop) through center; slide moves each vertex a fraction \
        amount (-1 to 1) of its edge along the neighbouring faces, positive toward one side of the loop, \
        negative toward the other, and on a brush corner ring along its other edge. center defaults to the \
        middle of the loop. Every brush that shares the loop vertices moves with it, and the call fails with \
        the reason, changing nothing, when a brush would become concave or flat. uv_lock keeps the textures on \
        the faces (default true).",
        {
            "solid": integer("Solid id of the seed edges given as indices or points.", 1, i64::from(u32::MAX)),
            "edge": { "description": "One seed edge: an index from get_brush_edges or [[x, y, z], [x, y, z]] (on solid)." },
            "edges": { "type": "array", "maxItems": 256, "description": "Seed edges: indices, [[x, y, z], [x, y, z]] pairs, or {solid, edge} / {solid, a, b} objects." },
            "solid_ids": ids("The brushes the loop may run through (default: the seed brushes, the selected brushes and the brushes touching them, up to 256)."),
            "toward": vec3("Direction toward the viewer: at the corners of a brush a loop follows the face that faces this way (default: the larger face)."),
            "select": boolean("Also select the result in the vertex tool and switch to it (default false)."),
            "expand": choice("What the seed edges grow into (default loop).", &["loop", "ring", "none"]),
            "operation": choice("What to do with the vertices.", &["move", "scale", "rotate", "slide"]),
            "delta": vec3("move: the offset in units."),
            "factor": number("scale: the factor (1 keeps the size).", 0.0, 100.0),
            "axis": choice("rotate: the axis (default z).", &["x", "y", "z", "normal"]),
            "degrees": number("rotate: the angle in degrees.", -360.0, 360.0),
            "amount": number("slide: the fraction of the edge, -1 to 1.", -1.0, 1.0),
            "center": vec3("scale and rotate: the center (default the middle of the loop)."),
            "uv_lock": boolean("Textures follow the vertices (default true)."),
        }, ["operation"];
    select_displacement_loop: Editor, "Select a displacement loop",
        "Find the row or column of the vertex grid of a displacement that passes through a vertex, across the \
        sewn neighbouring displacements, like Alt+click on a grid line in the displacement tool. face is the \
        displacement face, row and col pick the vertex (0 to 2^power), direction row follows the row \
        (varying col) and column the column. Answers the world positions of the vertices along the loop, \
        whether it is closed and how many displacements it crosses. smooth_displacement_loop and \
        set_displacement_loop_height act on it.",
        {
            "face": faces("The displacement face (one item)."),
            "row": integer("Row of the vertex.", 0, 16),
            "col": integer("Column of the vertex.", 0, 16),
            "direction": choice("Follow the row or the column (default row).", &["row", "column"]),
            "select": boolean("Also select the loop in the displacement tool and switch to it (default false)."),
        }, ["face", "row", "col"];
    smooth_displacement_loop: Edit, "Smooth a displacement loop",
        "Smooth the vertices of a displacement row or column (see select_displacement_loop for how it is \
        found) along their own line: each vertex moves along the face normal toward the average of its two \
        neighbours on the loop, strength per pass, passes times; the ends of an open loop stay. Sewn \
        neighbours follow and stay sewn, one undo step. Answers how many vertices changed.",
        {
            "face": faces("The displacement face (one item)."),
            "row": integer("Row of the vertex.", 0, 16),
            "col": integer("Column of the vertex.", 0, 16),
            "direction": choice("Follow the row or the column (default row).", &["row", "column"]),
            "strength": number("Fraction of the way to the neighbours' average per pass (default 0.5).", 0.0, 1.0),
            "passes": integer("How many times to smooth (default 1).", 1, 64),
        }, ["face", "row", "col"];
    set_displacement_loop_height: Edit, "Set the height of a displacement loop",
        "Set the height above the face (along the face normal, like Raise to) of every vertex of a displacement \
        row or column (see select_displacement_loop for how it is found), sewn neighbours following, one undo \
        step. Answers how many vertices changed.",
        {
            "face": faces("The displacement face (one item)."),
            "row": integer("Row of the vertex.", 0, 16),
            "col": integer("Column of the vertex.", 0, 16),
            "direction": choice("Follow the row or the column (default row).", &["row", "column"]),
            "height": number("The height in units.", -16384.0, 16384.0),
        }, ["face", "row", "col", "height"];
}

fn point_pair(v: &Value) -> Option<[DVec3; 2]> {
    let list = v.as_array().filter(|l| l.len() == 2)?;
    let point = |p: &Value| -> Option<DVec3> {
        let c: Vec<f64> = p.as_array().filter(|c| c.len() == 3)?.iter().filter_map(Value::as_f64).collect();
        (c.len() == 3 && c.iter().all(|x| coord_ok(*x))).then(|| DVec3::new(c[0], c[1], c[2]))
    };
    Some([point(&list[0])?, point(&list[1])?])
}

fn indexed(state: &AppState, solid: u32, v: &Value) -> R<Seed> {
    let n = state.doc.scene().solid(solid).ok_or_else(|| format!("no solid has the id {solid}"))?;
    let edges = if n.geom.valid { n.geom.edges() } else { Vec::new() };
    let i = v.as_u64().map(|i| i as usize).filter(|i| *i < edges.len());
    let i = i.ok_or_else(|| format!("solid {solid} has {} edges (0 to {})", edges.len(), edges.len().max(1) - 1))?;
    Ok((solid, edges[i][0], edges[i][1]))
}

fn seed_item(state: &AppState, solid: Option<u32>, v: &Value) -> R<Seed> {
    let bad = "each edge is an index, [[x, y, z], [x, y, z]], {solid, edge} or {solid, a, b}";
    let need = || solid.ok_or_else(|| "give solid for edges given as indices or points".to_owned());
    match v {
        Value::Number(_) => indexed(state, need()?, v),
        Value::Array(_) => {
            let [p, q] = point_pair(v).ok_or(bad)?;
            Ok((need()?, p, q))
        }
        Value::Object(o) => {
            let sid = match o.get("solid") {
                Some(s) => {
                    let id = s.as_u64().and_then(|x| u32::try_from(x).ok()).ok_or("solid must be a solid id")?;
                    if state.doc.scene().solid(id).is_none() {
                        return Err(format!("no solid has the id {id}"));
                    }
                    id
                }
                None => need()?,
            };
            match (o.get("edge"), o.get("a"), o.get("b")) {
                (Some(e), None, None) => indexed(state, sid, e),
                (None, Some(p), Some(q)) => {
                    let [p, q] = point_pair(&json!([p, q])).ok_or(bad)?;
                    Ok((sid, p, q))
                }
                _ => Err(bad.to_owned()),
            }
        }
        _ => Err(bad.to_owned()),
    }
}

fn seeds_of(state: &AppState, a: &Args<'_>) -> R<Vec<Seed>> {
    let solid = if a.get("solid").is_some() { Some(solid_id(&state.doc, a, "solid")?) } else { None };
    let mut items: Vec<&Value> = a.get("edge").into_iter().collect();
    if let Some(list) = a.get("edges") {
        items.extend(list.as_array().ok_or("edges must be an array")?);
    }
    if items.is_empty() {
        return Err("give edge or edges".to_owned());
    }
    if items.len() > MAX_SEEDS {
        return Err(format!("at most {MAX_SEEDS} seed edges per call"));
    }
    let mut out: Vec<Seed> = Vec::with_capacity(items.len());
    for v in items {
        let s = seed_item(state, solid, v)?;
        if !out.iter().any(|x| x.0 == s.0 && x.1.distance(s.1) < 1e-3 && x.2.distance(s.2) < 1e-3) {
            out.push(s);
        }
    }
    Ok(out)
}

fn scope_ids(state: &AppState, a: &Args<'_>, seeds: &[Seed]) -> R<Vec<Id>> {
    if let Some(ids) = opt_solids(&state.doc, a, "solid_ids")? {
        if ids.is_empty() {
            return Err("solid_ids is empty".to_owned());
        }
        return Ok(ids);
    }
    let selected = state.doc.selected_solid_ids();
    let mut pool: Vec<Id> = Vec::new();
    for (id, _, _) in seeds {
        if pool.contains(id) {
            continue;
        }
        for x in state.doc.loop_pool(*id, &selected) {
            if !pool.contains(&x) {
                pool.push(x);
            }
        }
    }
    Ok(pool)
}

fn edges_json(state: &AppState, set: &EdgeSet) -> Value {
    let list: Vec<Value> = set
        .edges
        .iter()
        .map(|r| {
            let index = state.doc.scene().solid(r.solid).and_then(|n| {
                n.geom.edges().iter().position(|[p, q]| {
                    (p.distance(r.a) < 1e-3 && q.distance(r.b) < 1e-3)
                        || (p.distance(r.b) < 1e-3 && q.distance(r.a) < 1e-3)
                })
            });
            json!({ "solid": r.solid, "edge": index, "a": v3(r.a), "b": v3(r.b) })
        })
        .collect();
    json!({
        "edge_count": list.len(),
        "vertex_count": set.vertices.len(),
        "closed": set.closed,
        "edges": list,
        "vertices": set.vertices.iter().map(|p| v3(*p)).collect::<Vec<_>>(),
    })
}

fn select_in_editor(set: &EdgeSet, result: Value) -> Outcome {
    let ids: Vec<Id> = {
        let mut v: Vec<Id> = Vec::new();
        for r in &set.edges {
            if !v.contains(&r.solid) {
                v.push(r.solid);
            }
        }
        v
    };
    let seeds: Vec<Seed> = set.edges.iter().map(|r| (r.solid, r.a, r.b)).collect();
    Outcome::AppFn(AppFn(Box::new(move |app, _ctx| {
        if ids.is_empty() {
            return ToolResult::json(result);
        }
        let objs: Vec<ObjRef> = ids.iter().map(|id| ObjRef::Solid(*id)).collect();
        app.state.doc.select(&objs, SelectOp::Replace);
        app.state.runtime.mesh.edge_request = Some(EdgeRequest { seeds, toward: None, additive: false, exact: true });
        if app.state.tool != ToolId::Vertex && app.has_tool(ToolId::Vertex) {
            app.switch_tool(ToolId::Vertex);
        }
        if !app.active_tool_action(Action::EdgeSelectLoop) {
            app.state.runtime.mesh.edge_request = None;
            return ToolResult::error("the vertex tool is not available");
        }
        ToolResult::json(result)
    })))
}

fn edge_command(state: &mut AppState, a: &Args<'_>, cmd: EdgeCmd, times: usize) -> R<Outcome> {
    let seeds = seeds_of(state, a)?;
    let ids = scope_ids(state, a, &seeds)?;
    let toward = a.opt_vec3("toward")?;
    let set = compute_edges(&state.doc, &ids, cmd, &seeds, toward, times)?;
    let result = edges_json(state, &set);
    if a.opt_bool("select")?.unwrap_or(false) {
        return Ok(select_in_editor(&set, result));
    }
    Ok(ToolResult::json(result).into())
}

fn select_edge_loop(state: &mut AppState, a: &Args<'_>) -> R<Outcome> {
    edge_command(state, a, EdgeCmd::Loop, 1)
}

fn select_edge_ring(state: &mut AppState, a: &Args<'_>) -> R<Outcome> {
    edge_command(state, a, EdgeCmd::Ring, 1)
}

fn grow_edge_selection(state: &mut AppState, a: &Args<'_>) -> R<Outcome> {
    let mode = a.opt_enum("mode", &["grow", "shrink", "boundary"])?.ok_or("mode is required")?;
    let times = a.opt_u64("times", 1, MAX_PASSES)?.unwrap_or(1) as usize;
    edge_command(state, a, EdgeCmd::parse(&mode).ok_or("unknown mode")?, times)
}

fn move_edge_loop(state: &mut AppState, a: &Args<'_>) -> R<ToolResult> {
    let seeds = seeds_of(state, a)?;
    let ids = scope_ids(state, a, &seeds)?;
    let toward = a.opt_vec3("toward")?;
    let expand = match a.opt_enum("expand", &["loop", "ring", "none"])?.as_deref() {
        Some("none") => None,
        Some("ring") => Some(EdgeCmd::Ring),
        _ => Some(EdgeCmd::Loop),
    };
    let op = a.opt_enum("operation", &["move", "scale", "rotate", "slide"])?.ok_or("operation is required")?;
    let center = a.opt_vec3("center")?;
    let spec = match op.as_str() {
        "move" => LoopSpec::Move(a.opt_vec3("delta")?.ok_or("move needs delta")?),
        "scale" => LoopSpec::Scale { factor: a.opt_f64("factor", 0.0, 100.0)?.ok_or("scale needs factor")?, center },
        "rotate" => {
            let axis = match a.opt_enum("axis", &["x", "y", "z", "normal"])? {
                Some(n) => RotateAxis::parse(&n).unwrap_or_default(),
                None => RotateAxis::Z,
            };
            LoopSpec::Rotate {
                axis,
                degrees: a.opt_f64("degrees", -360.0, 360.0)?.ok_or("rotate needs degrees")?,
                center,
            }
        }
        _ => LoopSpec::Slide(a.opt_f64("amount", -1.0, 1.0)?.ok_or("slide needs amount")?),
    };
    let opts = VertexEditOptions { uv_lock: a.opt_bool("uv_lock")?.unwrap_or(true) };
    if state.doc.in_transaction() {
        return Err("Hammer Pro is in the middle of another edit (a drag in a view?); try again".to_owned());
    }
    let report = transform_loop(&mut state.doc, &ids, &seeds, expand, toward, &spec, opts)?;
    Ok(ToolResult::json(json!({
        "solids_changed": report.solids,
        "vertices": report.vertices,
        "displacements_lost": report.lost_disps,
    })))
}

fn disp_seed(state: &AppState, a: &Args<'_>) -> R<(FaceRef, disp_loops::LoopSel)> {
    let list = opt_faces(&state.doc, a, "face")?.ok_or("face is required")?;
    let [face] = list[..] else { return Err("give exactly one face".to_owned()) };
    let row = a.opt_u64("row", 0, 16)?.ok_or("row is required")? as usize;
    let col = a.opt_u64("col", 0, 16)?.ok_or("col is required")? as usize;
    let along = match a.opt_enum("direction", &["row", "column"])?.as_deref() {
        Some("column") => LoopAlong::Column,
        _ => LoopAlong::Row,
    };
    let sel = disp_loops::loop_from_vertex(&state.doc, face, row, col, along)?;
    Ok((face, sel))
}

fn view_json(view: &disp_loops::LoopView) -> Value {
    json!({
        "vertex_count": view.positions.len(),
        "closed": view.closed,
        "displacements": view.faces,
        "vertices": view.positions.iter().map(|p| v3(*p)).collect::<Vec<_>>(),
    })
}

fn select_displacement_loop(state: &mut AppState, a: &Args<'_>) -> R<Outcome> {
    let (face, sel) = disp_seed(state, a)?;
    let view = disp_loops::loop_view(&state.doc, &sel).ok_or("no loop goes through that vertex")?;
    let result = view_json(&view);
    if !a.opt_bool("select")?.unwrap_or(false) {
        return Ok(ToolResult::json(result).into());
    }
    Ok(Outcome::AppFn(AppFn(Box::new(move |app, _ctx| {
        app.state.runtime.mesh.disp_loop_request = Some(hp_app::runtime::DispLoopRequest {
            solid: face.solid,
            side: face.side,
            base_a: sel.base_a,
            base_b: sel.base_b,
        });
        if app.state.tool != ToolId::Displacement && app.has_tool(ToolId::Displacement) {
            app.switch_tool(ToolId::Displacement);
        }
        ToolResult::json(result)
    }))))
}

fn edit_displacement_loop(state: &mut AppState, a: &Args<'_>, edit: LoopEdit) -> R<ToolResult> {
    let (_, sel) = disp_seed(state, a)?;
    if state.doc.in_transaction() {
        return Err("Hammer Pro is in the middle of another edit (a drag in a view?); try again".to_owned());
    }
    let changed = disp_loops::edit_loop(&mut state.doc, &sel, edit)?;
    Ok(ToolResult::json(json!({ "vertices_changed": changed })))
}

fn smooth_displacement_loop(state: &mut AppState, a: &Args<'_>) -> R<ToolResult> {
    let strength = a.opt_f64("strength", 0.0, 1.0)?.unwrap_or(0.5);
    let passes = a.opt_u64("passes", 1, MAX_PASSES)?.unwrap_or(1) as u32;
    edit_displacement_loop(state, a, LoopEdit::Smooth { strength, passes })
}

fn set_displacement_loop_height(state: &mut AppState, a: &Args<'_>) -> R<ToolResult> {
    let height = a.opt_f64("height", -16384.0, 16384.0)?.ok_or("height is required")?;
    edit_displacement_loop(state, a, LoopEdit::Height(height))
}

#[cfg(test)]
mod tests {
    use glam::DVec3;
    use serde_json::json;

    use super::super::semantic_tests::{err, ok};

    fn cube(s: &mut hp_app::AppState, min: DVec3, max: DVec3) -> u32 {
        let params = hp_geom::PrimitiveParams { bounds: hp_geom::Aabb::new(min, max), ..Default::default() };
        s.doc.create_primitive(&params, hp_editor::Owner::World).unwrap()[0]
    }

    #[test]
    fn loops_and_rings_of_two_boxes() {
        let mut s = hp_app::testing::test_state();
        let a = cube(&mut s, DVec3::ZERO, DVec3::new(64.0, 64.0, 64.0));
        let b = cube(&mut s, DVec3::new(64.0, 0.0, 0.0), DVec3::new(128.0, 64.0, 64.0));
        let seed = json!([[0, 0, 64], [64, 0, 64]]);
        let r = ok(&mut s, "select_edge_loop", json!({ "solid": a, "edge": seed, "toward": [0, 0, 1] }));
        let text = r.to_string();
        assert!(text.contains("\"closed\":true"), "{text}");
        let v: serde_json::Value = serde_json::from_str(&text).unwrap_or_default();
        let _ = v;
        let r = ok(&mut s, "select_edge_ring", json!({ "solid": a, "edge": seed }));
        assert!(r.to_string().contains("\"edge_count\":4"));
        let r = ok(&mut s, "grow_edge_selection", json!({ "solid": a, "edge": seed, "mode": "grow" }));
        assert!(r.to_string().contains("\"vertex_count\":"));
        err(&mut s, "select_edge_loop", json!({ "solid": a }));
        err(&mut s, "select_edge_loop", json!({ "solid": a, "edge": [[0, 0, 0], [5, 5, 5]] }));
        err(&mut s, "grow_edge_selection", json!({ "solid": a, "edge": seed, "mode": "x" }));
        let _ = b;
    }

    #[test]
    fn moving_a_loop_is_one_undo_step() {
        let mut s = hp_app::testing::test_state();
        let a = cube(&mut s, DVec3::ZERO, DVec3::new(64.0, 64.0, 64.0));
        let seed = json!([[0, 0, 64], [64, 0, 64]]);
        let before = s.doc.history_len();
        let r = ok(
            &mut s,
            "move_edge_loop",
            json!({ "solid": a, "solid_ids": [a], "edge": seed, "operation": "move", "delta": [0, 0, 16], "toward": [0, 0, 1] }),
        );
        assert!(r.to_string().contains("\"vertices\":4"), "{r}");
        assert_eq!(s.doc.history_len().0, before.0 + 1);
        let pts = s.doc.solid_vertices(a).unwrap().0;
        assert_eq!(pts.iter().filter(|p| (p.z - 80.0).abs() < 1e-6).count(), 4);
        s.doc.undo();
        let pts = s.doc.solid_vertices(a).unwrap().0;
        assert_eq!(pts.iter().filter(|p| (p.z - 64.0).abs() < 1e-6).count(), 4);
        ok(
            &mut s,
            "move_edge_loop",
            json!({ "solid": a, "solid_ids": [a], "edge": seed, "operation": "scale", "factor": 0.5, "toward": [0, 0, 1] }),
        );
        s.doc.undo();
        ok(
            &mut s,
            "move_edge_loop",
            json!({ "solid": a, "solid_ids": [a], "edge": seed, "operation": "rotate", "degrees": 10, "toward": [0, 0, 1] }),
        );
        s.doc.undo();
        ok(
            &mut s,
            "move_edge_loop",
            json!({ "solid": a, "solid_ids": [a], "edge": seed, "operation": "slide", "amount": 0.25, "toward": [0, 0, 1] }),
        );
        err(&mut s, "move_edge_loop", json!({ "solid": a, "solid_ids": [a], "edge": seed, "operation": "move" }));
        let steps = s.doc.history_len();
        err(
            &mut s,
            "move_edge_loop",
            json!({ "solid": a, "solid_ids": [a], "edge": seed, "operation": "scale", "factor": 0.0, "toward": [0, 0, 1] }),
        );
        assert_eq!(s.doc.history_len(), steps, "a refused edit leaves no undo step");
    }

    #[test]
    fn displacement_loops_have_no_vertices_on_plain_faces() {
        let mut s = hp_app::testing::test_state();
        let a = cube(&mut s, DVec3::ZERO, DVec3::new(64.0, 64.0, 64.0));
        err(&mut s, "select_displacement_loop", json!({ "face": [{ "solid": a, "side": 0 }], "row": 0, "col": 0 }));
        err(&mut s, "smooth_displacement_loop", json!({ "face": [{ "solid": a, "side": 0 }], "row": 0, "col": 0 }));
        err(&mut s, "set_displacement_loop_height", json!({ "face": [{ "solid": a, "side": 0 }], "row": 0, "col": 0 }));
    }
}
