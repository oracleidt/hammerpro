use glam::DVec3;
use hp_app::{AppState, ViewKind};
use hp_mcp::protocol::ToolResult;
use hp_tools_create::pipe::{
    self, HEADINGS, MAX_COLLAR_EVERY, MAX_LENGTH, MAX_MOVES, MAX_RADIUS, MAX_SEGMENTS, MAX_SIDES, MAX_SLICES,
    MIN_SIDES, PieceKind, PipeMove, PipeSpec, Shape,
};
use serde_json::{Value, json};

use super::semantic::*;
use super::tools::{Args, MAX_NAME, R, bounds_json, edit, material_name, v3};

domain! { "pipes";
    create_pipe: Edit, "Create a pipe",
        "Build a modular pipe made of brushes (not props): straight runs cut into brushes, joined by 90 degree \
        elbows made of several brush slices that approximate an arc, with optional collars (flanges). The \
        profile is square or round (a polygon with sides), hollow (wall brushes around an empty tube, wall \
        thickness) or solid. The route is a start, a first direction and a list of moves: a number goes forward \
        by that length, left, right, up and down turn by 90 degrees (up and down follow the up axis of the grid). \
        Lengths are measured between the corners of the route; each bend takes bend_radius off both runs next to \
        it, so a run between two bends must be at least twice bend_radius long. Everything follows the active \
        grid of the chosen view (its rotation, its custom axes and steps): the start snaps to the grid, the \
        directions are the grid axes, and lengths, radius, wall, bend radius and collar sizes round to the grid \
        step unless snap is false (the answer lists the effective values). Collars sit every collar_every brushes \
        of a run, and optionally at both ends and on both sides of every bend; they are rings around the pipe, \
        not overlapping it. The brushes are grouped and selected, one undo step. With preview true nothing is \
        edited: the answer only describes the pipe. The answer gives the end point and direction so the next \
        pipe can continue from it.",
        {
            "start": vec3("Start of the pipe axis (default [0, 0, 0]); snaps to the grid."),
            "direction": choice("First direction along an axis of the grid, x being the first axis of the grid (default +x).", &HEADINGS),
            "direction_vector": vec3("First direction as a world vector, taken to the nearest grid axis (instead of direction)."),
            "moves": {
                "type": "array",
                "maxItems": MAX_MOVES,
                "items": { "description": "A length (number) or a turn (text)." },
                "description": "Moves in order: a length (128), or a text: left, right, up, down (or L R U D), forward128.",
            },
            "path": string("The moves as text, like \"256 L 128 U 64\" (instead of moves)."),
            "length": number("A single straight pipe of this length (instead of moves and path).", 1.0, MAX_LENGTH),
            "view": choice("View whose grid is used (default top).", &["top", "front", "side"]),
            "shape": choice("Profile (default square).", &["square", "round"]),
            "sides": integer("Side count of a round profile (default 12).", i64::from(MIN_SIDES), i64::from(MAX_SIDES)),
            "radius": number("Half the width of the pipe (default 32).", 1.0, MAX_RADIUS),
            "hollow": boolean("A hollow tube made of wall brushes (default true); false makes solid brushes."),
            "wall": number("Wall thickness of a hollow pipe (default 8), smaller than the radius.", 0.5, MAX_RADIUS),
            "segments": integer("Brushes along each straight run (default 4).", 1, i64::from(MAX_SEGMENTS)),
            "segment_length": number("Length of a straight brush instead of segments (default 0: use segments).", 0.0, MAX_LENGTH),
            "bend_radius": number("Radius of the 90 degree bends at the axis (default 64), larger than the radius.", 1.0, MAX_RADIUS * 4.0),
            "bend_slices": integer("Brush slices per 90 degree bend (default 4).", 1, i64::from(MAX_SLICES)),
            "collar_every": integer("A collar between every N brushes of a run (default 0: none).", 0, i64::from(MAX_COLLAR_EVERY)),
            "collar_size": number("How far a collar sticks out of the pipe (default 8).", 0.0, MAX_RADIUS),
            "collar_length": number("Length of a collar along the pipe (default 8).", 0.5, MAX_RADIUS),
            "collar_ends": boolean("A collar at both ends of the pipe (default false)."),
            "collar_bends": boolean("A collar on both sides of every bend (default false)."),
            "snap": boolean("Round lengths and sizes to the grid and use grid axes (default true)."),
            "material": string("Material of the pipe (default: the current material)."),
            "collar_material": string("Material of the collars (default: same as the pipe)."),
            "tex_scale": number("Texture scale of the faces (default: the editor's).", 0.01, 64.0),
            "group": boolean("Group the brushes (default true)."),
            "preview": boolean("Only describe the pipe, change nothing (default false)."),
        }, [];
}

fn view_kind(name: &str) -> ViewKind {
    match name {
        "front" => ViewKind::Front,
        "side" => ViewKind::Side,
        _ => ViewKind::Top,
    }
}

fn moves_arg(a: &Args<'_>) -> R<Vec<PipeMove>> {
    let given = [a.get("moves").is_some(), a.get("path").is_some(), a.get("length").is_some()];
    if given.iter().filter(|g| **g).count() != 1 {
        return Err("give exactly one of moves, path or length".to_owned());
    }
    if let Some(l) = a.opt_f64("length", 1.0, MAX_LENGTH)? {
        return Ok(vec![PipeMove::Forward(l)]);
    }
    if let Some(p) = a.opt_str("path", 4096)? {
        return pipe::parse_moves(p);
    }
    let list = a.get("moves").and_then(Value::as_array).filter(|l| !l.is_empty() && l.len() <= MAX_MOVES);
    let list = list.ok_or_else(|| format!("moves must be 1 to {MAX_MOVES} lengths or turns"))?;
    list.iter()
        .map(|m| match m {
            Value::Number(n) => {
                let l = n.as_f64().filter(|l| l.is_finite() && *l > 0.0 && *l <= MAX_LENGTH);
                l.map(PipeMove::Forward).ok_or_else(|| format!("a length must be a number from 0 to {MAX_LENGTH}"))
            }
            Value::String(s) => pipe::parse_move(s),
            _ => Err("a move is a length or one of left, right, up, down".to_owned()),
        })
        .collect()
}

fn spec_from(state: &AppState, a: &Args<'_>, frame: &hp_app::grid::GridFrame) -> R<PipeSpec> {
    let mut spec =
        PipeSpec { moves: moves_arg(a)?, start: a.opt_vec3("start")?.unwrap_or(DVec3::ZERO), ..PipeSpec::default() };
    spec.direction = match (a.opt_enum("direction", &HEADINGS)?, opt_dir(a, "direction_vector")?) {
        (Some(_), Some(_)) => return Err("give direction or direction_vector, not both".to_owned()),
        (Some(h), None) => pipe::heading_vector(frame, HEADINGS.iter().position(|x| *x == h).unwrap_or(0)),
        (None, Some(v)) => v,
        (None, None) => frame.axes.x_axis,
    };
    if let Some(s) = a.opt_enum("shape", &["square", "round"])? {
        spec.shape = Shape::parse(&s).unwrap_or(Shape::Square);
    }
    if let Some(v) = a.opt_u64("sides", u64::from(MIN_SIDES), u64::from(MAX_SIDES))? {
        spec.sides = v as u32;
    }
    spec.radius = a.opt_f64("radius", 1.0, MAX_RADIUS)?.unwrap_or(spec.radius);
    spec.hollow = a.opt_bool("hollow")?.unwrap_or(spec.hollow);
    spec.wall = a.opt_f64("wall", 0.5, MAX_RADIUS)?.unwrap_or(spec.wall);
    if let Some(v) = a.opt_u64("segments", 1, u64::from(MAX_SEGMENTS))? {
        spec.segments = v as u32;
    }
    spec.segment_length = a.opt_f64("segment_length", 0.0, MAX_LENGTH)?.unwrap_or(0.0);
    spec.bend_radius = a.opt_f64("bend_radius", 1.0, MAX_RADIUS * 4.0)?.unwrap_or(spec.bend_radius);
    if let Some(v) = a.opt_u64("bend_slices", 1, u64::from(MAX_SLICES))? {
        spec.bend_slices = v as u32;
    }
    if let Some(v) = a.opt_u64("collar_every", 0, u64::from(MAX_COLLAR_EVERY))? {
        spec.collar_every = v as u32;
    }
    spec.collar_size = a.opt_f64("collar_size", 0.0, MAX_RADIUS)?.unwrap_or(spec.collar_size);
    spec.collar_length = a.opt_f64("collar_length", 0.5, MAX_RADIUS)?.unwrap_or(spec.collar_length);
    spec.collar_ends = a.opt_bool("collar_ends")?.unwrap_or(false);
    spec.collar_bends = a.opt_bool("collar_bends")?.unwrap_or(false);
    let wants_collars = spec.collar_every > 0 || spec.collar_ends || spec.collar_bends;
    if !wants_collars {
        spec.collar_size = 0.0;
    }
    spec.snap = a.opt_bool("snap")?.unwrap_or(true);
    spec.material = match a.opt_str("material", MAX_NAME)? {
        Some(m) => material_name(m)?,
        None => state.current_material.clone(),
    };
    spec.collar_material = a.opt_str("collar_material", MAX_NAME)?.map(material_name).transpose()?;
    spec.tex_scale = a.opt_f64("tex_scale", 0.01, 64.0)?.unwrap_or_else(|| {
        let s = state.settings.edit.default_texture_scale;
        if s.is_finite() && s.abs() > 1e-6 { s } else { spec.tex_scale }
    });
    Ok(spec)
}

fn create_pipe(state: &mut AppState, a: &Args<'_>) -> R<ToolResult> {
    let kind = view_kind(&a.opt_enum("view", &["top", "front", "side"])?.unwrap_or_else(|| "top".to_owned()));
    let frame = state.grid_frame(kind);
    let spec = spec_from(state, a, &frame)?;
    let group = a.opt_bool("group")?.unwrap_or(true);
    let preview = a.opt_bool("preview")?.unwrap_or(false);
    let plan = pipe::plan(&spec, &frame).map_err(|e| e.to_string())?;
    let s = &plan.spec;
    let mut out = json!({
        "preview": preview,
        "brushes": plan.pieces.len(),
        "straight_brushes": plan.count(PieceKind::Straight),
        "bend_brushes": plan.count(PieceKind::Bend),
        "collar_brushes": plan.count(PieceKind::Collar),
        "runs": plan.runs,
        "bends": plan.bends,
        "collars": plan.collars,
        "length": (plan.length * 1000.0).round() / 1000.0,
        "start": v3(s.start + DVec3::ZERO),
        "end": v3(plan.end + DVec3::ZERO),
        "end_direction": v3(plan.end_direction + DVec3::ZERO),
        "bounds": bounds_json(Some(plan.bounds)),
        "effective": {
            "shape": s.shape.name(),
            "sides": s.sides,
            "radius": s.radius,
            "hollow": s.hollow,
            "wall": s.wall,
            "segments": s.segments,
            "bend_radius": s.bend_radius,
            "bend_slices": s.bend_slices,
            "collar_size": s.collar_size,
            "collar_length": s.collar_length,
            "moves": pipe::format_moves(&s.moves),
        },
    });
    if preview {
        return Ok(ToolResult::json(out));
    }
    let lightmap = match state.settings.edit.default_lightmap_scale {
        l if l > 0 => l,
        _ => 16,
    };
    let created = edit(state, "create_pipe", |doc| pipe::apply::create_in(doc, &plan, lightmap, group))?;
    state.runtime.brush.last_created = Some(plan.bounds);
    if let Value::Object(map) = &mut out {
        map.insert("solid_ids".to_owned(), json!(created.ids));
        map.insert("group_id".to_owned(), json!(created.group));
    }
    Ok(ToolResult::json(out))
}

#[cfg(test)]
mod tests {
    use serde_json::json;

    use super::super::semantic_tests::{err, ok};

    #[test]
    fn pipes_preview_create_and_chain() {
        let mut s = hp_app::testing::test_state();
        let before = s.doc.stats().solids;
        let p = ok(&mut s, "create_pipe", json!({ "path": "256 L 256", "preview": true }));
        assert_eq!(p["preview"], true);
        assert_eq!(s.doc.stats().solids, before, "a preview changes nothing");
        let brushes = p["brushes"].as_u64().unwrap();
        assert_eq!(p["bends"], 1);
        assert_eq!(p["end"], json!([256.0, 256.0, 0.0]));
        assert_eq!(p["end_direction"], json!([0.0, 1.0, 0.0]));

        let r = ok(&mut s, "create_pipe", json!({ "path": "256 L 256" }));
        assert_eq!(r["solid_ids"].as_array().unwrap().len() as u64, brushes);
        assert!(r["group_id"].as_u64().is_some());
        assert_eq!(s.doc.stats().solids, before + brushes as usize);
        assert_eq!(s.doc.undo_label(), Some("MCP: create_pipe"));

        let next = ok(
            &mut s,
            "create_pipe",
            json!({ "start": r["end"], "direction_vector": r["end_direction"], "moves": [128, "up", 192], "shape": "round", "sides": 8, "hollow": false, "preview": true }),
        );
        assert_eq!(next["start"], r["end"]);
        assert_eq!(next["effective"]["shape"], "round");
        assert_eq!(next["end"], json!([256.0, 384.0, 192.0]));

        let straight =
            ok(&mut s, "create_pipe", json!({ "length": 200, "radius": 20, "segments": 2, "preview": true }));
        assert_eq!(straight["effective"]["radius"], 16.0, "the radius snaps to the grid");
        let free = ok(&mut s, "create_pipe", json!({ "length": 200, "radius": 20, "snap": false, "preview": true }));
        assert_eq!(free["effective"]["radius"], 20.0);

        let collars = ok(
            &mut s,
            "create_pipe",
            json!({ "length": 256, "collar_every": 2, "collar_ends": true, "preview": true }),
        );
        assert_eq!(collars["collars"], 3);
        assert!(collars["collar_brushes"].as_u64().unwrap() > 0);

        let front =
            ok(&mut s, "create_pipe", json!({ "length": 64, "view": "front", "direction": "+z", "preview": true }));
        assert_eq!(front["end"], json!([0.0, 0.0, 64.0]));
    }

    #[test]
    fn bad_pipes_are_refused() {
        let mut s = hp_app::testing::test_state();
        let solids = s.doc.stats().solids;
        err(&mut s, "create_pipe", json!({}));
        err(&mut s, "create_pipe", json!({ "length": 100, "path": "100" }));
        err(&mut s, "create_pipe", json!({ "path": "40 L 200" }));
        err(&mut s, "create_pipe", json!({ "path": "100 sideways" }));
        err(&mut s, "create_pipe", json!({ "moves": [100, true] }));
        err(&mut s, "create_pipe", json!({ "length": 100, "radius": 16, "wall": 16 }));
        err(&mut s, "create_pipe", json!({ "length": 100, "direction": "+x", "direction_vector": [1, 0, 0] }));
        err(&mut s, "create_pipe", json!({ "length": 100, "material": "bad name" }));
        err(
            &mut s,
            "create_pipe",
            json!({ "path": "256 L 256 R 256 L 256 R 256", "shape": "round", "sides": 48, "segments": 256}),
        );
        assert_eq!(s.doc.stats().solids, solids);
    }
}
