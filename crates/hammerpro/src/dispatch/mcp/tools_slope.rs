use hp_app::AppState;
use hp_editor::{FaceRef, Id};
use hp_geom::SOURCE_WALKABLE_DEGREES;
use hp_mcp::protocol::ToolResult;
use hp_tools_mesh::disp::blend::{BlendOptions, FALLOFF_NAMES, MODE_NAMES};
use hp_tools_mesh::disp::slope::{self, SlopeOp, SlopeOptions};
use serde_json::{Value, json};

use super::semantic::*;
use super::tools::{AppFn, Args, Outcome, R, undo_label};
use super::tools_disp::{disp_targets, with_faces_selected};

const OPERATIONS: [&str; 7] = ["limit", "ramp", "uniform", "path", "smooth", "measure", "display"];
const MAX_SETS: usize = 16;

domain! { "displacement blending and slopes";
    blend_displacements: Edit, "Blend displacement heights",
        "Blend the heights of two or more displacement sets across a transition zone (the Height blending section of the displacement panel), one undo step, sewn afterwards. A set is a list of solid ids (sets, at least 2, at most 16) or, without sets, the selected displacements (or faces, ids, solid_ids) split by group, else by solid. width is the zone width in units (0.01 to 65536); falloff linear, smooth, ease_in or ease_out; mode average (both sides meet in the middle), a_over_b (set 1 stays and the later sets follow it with a soft edge), b_over_a, max or min (the higher or lower surface wins); match_edges shifts each side by the height gap at the meeting line so it keeps its own relief and the sides meet without a step; alpha blends the texture alpha and the 4-way blend rows across the same zone; match_powers first resamples the lower power displacements to the highest power. Works on overlapping and adjacent displacements of any power. Defaults are the displacement tool options (get_tool_options: blend_width, blend_falloff, blend_mode, blend_match_edges, blend_alpha, blend_match_powers).",
        {
            "sets": { "type": "array", "minItems": 2, "maxItems": MAX_SETS, "items": ids("Solid ids of one set."), "description": "The sets to blend: each a list of solid ids (every displacement of the solids)." },
            "faces": faces("Faces to split into sets (default: the selected displacements)."), "ids": target_ids(), "solid_ids": target_solid_ids(),
            "width": number("Width of the transition zone in units.", 0.01, 65536.0),
            "falloff": choice("Curve of the transition.", &FALLOFF_NAMES),
            "mode": choice("How the sets meet.", &MODE_NAMES),
            "match_edges": boolean("Shift each side by the height gap at the meeting line."),
            "alpha": boolean("Also blend the texture alpha."),
            "match_powers": boolean("Resample the lower power displacements to the highest power first."),
        }, [];
    control_slope: Edit, "Control displacement slopes",
        "Control the slopes of displacements (the Slope control section of the displacement panel), one undo step, sewn afterwards. Slopes are the angles of the triangles from flat (0 degrees) to vertical (90 degrees); heights move along Z and sewn displacements are one surface. operation limit: relax the vertices until no triangle is steeper than slope_max_angle, the mean height is kept (slope_terrace first cuts the heights into flat steps of slope_terrace_step units). ramp: turn the selection into a plane of exactly slope_ramp_angle degrees climbing toward slope_ramp_direction degrees (0 is +X, 90 is +Y), keeping slope_ramp_pivot (average, lowest or highest) in place, slope_ramp_strength and slope_ramp_detail (relief kept) from 0 to 1. uniform: make the slope constant over the whole selection, keeping its mean angle and direction (slope_uniform_strength, slope_uniform_detail). path: make the surface within slope_path_width units of path (list of [x, y, z] points, at least 2) climb at a constant slope from the height of the first point to the height of the last (slope_path_falloff, slope_path_strength). smooth: even out slope changes (slope_smooth_passes, slope_smooth_strength). measure: change nothing, answer the smallest, largest and mean slope in degrees and the best fit plane (angle and direction, usable for ramp). display: choose the slope colors of the 3D view (the walkability display): show, mode (source: the three engine classes; limit: green up to the angle, red above; gradient: colors by angle with the limit in the middle) and walk_angle in degrees (default 45.573, the Source walkable limit, normal Z of 0.7); the legend follows. The slope_* options are the displacement tool options (get_tool_options); options overrides them for this call only. Answers the slope range before and after.",
        {
            "operation": choice("What to do.", &OPERATIONS),
            "faces": faces("Faces (default: the selected displacements)."), "ids": target_ids(), "solid_ids": target_solid_ids(),
            "options": { "type": "object", "description": "Slope option names to values for this call (same names as the displacement tool options, e.g. slope_max_angle, slope_ramp_angle)." },
            "path": { "type": "array", "items": vec3("Path point (world)."), "minItems": 2, "maxItems": 256, "description": "path: the points of the path." },
            "show": boolean("display: turn the slope colors on or off."),
            "mode": choice("display: the color mode.", &["source", "limit", "gradient"]),
            "walk_angle": number("display: the walkable angle in degrees.", 1.0, 89.0),
        }, ["operation"];
    select_displacements_by_slope: Editor, "Select displacements by slope",
        "Select the displacements that have triangles steeper (above) or flatter (below) than an angle in degrees from flat, like Select by slope in the displacement panel. Candidates: faces, ids or solid_ids, else the selected displacements, else every displacement. match any counts a displacement when any of its triangles matches, all only when every triangle does. op replace (default), add or remove changes the selection. angle defaults to the slope_select_angle option (45.573, the Source walkable limit). Answers the faces selected.",
        {
            "angle": number("Angle in degrees from flat.", 0.0, 90.0),
            "mode": choice("Select the displacements above or below the angle (default above).", &["above", "below"]),
            "match": choice("Any triangle or all triangles (default any).", &["any", "all"]),
            "op": choice("How the selection changes (default replace).", &["replace", "add", "remove"]),
            "faces": faces("Candidate faces."), "ids": target_ids(), "solid_ids": target_solid_ids(),
        }, [];
}

fn tool_options(app: &crate::app::HammerProApp) -> (BlendOptions, SlopeOptions) {
    let values = app
        .tools
        .iter()
        .find(|t| t.id() == hp_app::ToolId::Displacement)
        .map(|t| t.option_values())
        .unwrap_or_default();
    (BlendOptions::from_values(&values), SlopeOptions::from_values(&values))
}

fn solid_sets(state: &AppState, a: &Args<'_>) -> R<Option<Vec<Vec<FaceRef>>>> {
    let Some(list) = a.get("sets") else { return Ok(None) };
    let list = list.as_array().ok_or("sets must be an array of arrays of solid ids")?;
    if list.len() < 2 || list.len() > MAX_SETS {
        return Err(format!("sets needs 2 to {MAX_SETS} sets"));
    }
    let mut out = Vec::new();
    for (i, item) in list.iter().enumerate() {
        let ids: Vec<Id> = item
            .as_array()
            .ok_or_else(|| format!("sets[{i}] must be an array of solid ids"))?
            .iter()
            .map(|x| x.as_u64().filter(|n| (1..=u64::from(u32::MAX)).contains(n)).map(|n| n as Id))
            .collect::<Option<_>>()
            .ok_or_else(|| format!("sets[{i}] must contain positive integer solid ids"))?;
        let faces: Vec<FaceRef> = state.doc.disp_faces().into_iter().filter(|f| ids.contains(&f.solid)).collect();
        if faces.is_empty() {
            return Err(format!("sets[{i}] has no displacement"));
        }
        out.push(faces);
    }
    Ok(Some(out))
}

fn blend_displacements(state: &mut AppState, a: &Args<'_>) -> R<Outcome> {
    let sets = match solid_sets(state, a)? {
        Some(s) => s,
        None => hp_tools_mesh::disp::blend::partition(&state.doc, &disp_targets(state, a, false)?),
    };
    if sets.len() < 2 {
        return Err("blending needs two sets: give sets, or select displacements of two groups or solids".to_owned());
    }
    let width = a.opt_f64("width", 0.01, 65536.0)?;
    let falloff = a.opt_enum("falloff", &FALLOFF_NAMES)?;
    let mode = a.opt_enum("mode", &MODE_NAMES)?;
    let (edges, alpha, powers) = (a.opt_bool("match_edges")?, a.opt_bool("alpha")?, a.opt_bool("match_powers")?);
    if state.doc.in_transaction() {
        return Err("Hammer Pro is in the middle of another edit (a drag in a view?); try again".to_owned());
    }
    Ok(Outcome::AppFn(AppFn(Box::new(move |app, _ctx| {
        let (mut opts, _) = tool_options(app);
        if let Some(w) = width {
            opts.width = w;
        }
        for (key, value) in [("blend_falloff", falloff), ("blend_mode", mode)] {
            if let Some(v) = value {
                opts.set(key, &v);
            }
        }
        for (key, value) in [("blend_match_edges", edges), ("blend_alpha", alpha), ("blend_match_powers", powers)] {
            if let Some(v) = value {
                opts.set(key, if v { "on" } else { "off" });
            }
        }
        let state = &mut app.state;
        state.doc.begin(undo_label("blend_displacements"));
        let moved = hp_tools_mesh::disp::blend::apply(state, &sets, &opts);
        state.doc.commit();
        ToolResult::json(json!({
            "sets": sets.len(),
            "faces": sets.iter().map(Vec::len).sum::<usize>(),
            "vertices_changed": moved,
        }))
    }))))
}

fn range_json(m: Option<slope::Measure>) -> Value {
    match m {
        Some(m) => json!({
            "triangles": m.stats.triangles,
            "min": m.stats.min,
            "max": m.stats.max,
            "mean": m.stats.mean,
            "plane": if m.fitted { json!({ "angle": m.angle, "direction": m.direction }) } else { Value::Null },
        }),
        None => Value::Null,
    }
}

fn path_points(a: &Args<'_>) -> R<Option<Vec<glam::DVec3>>> {
    let Some(v) = a.get("path") else { return Ok(None) };
    let list = v.as_array().filter(|l| (2..=256).contains(&l.len())).ok_or("path must hold 2 to 256 points")?;
    let mut pts = Vec::new();
    for p in list {
        let c: Vec<f64> = p
            .as_array()
            .filter(|c| c.len() == 3)
            .and_then(|c| c.iter().map(|x| x.as_f64().filter(|f| f.is_finite())).collect())
            .ok_or("every path point must be [x, y, z]")?;
        pts.push(glam::DVec3::new(c[0], c[1], c[2]));
    }
    Ok(Some(pts))
}

fn display_slopes(state: &mut AppState, a: &Args<'_>) -> R<Outcome> {
    let show = a.opt_bool("show")?;
    let mode = a.opt_enum("mode", &["source", "limit", "gradient"])?;
    let angle = a.opt_f64("walk_angle", 1.0, 89.0)?;
    let r = &mut state.settings.render;
    if let Some(s) = show {
        r.disp_walkable = s;
    }
    if let Some(m) = mode {
        r.disp_walk_mode = hp_app::settings::DispWalkMode::ALL.into_iter().find(|d| d.name() == m).unwrap_or_default();
    }
    if let Some(w) = angle {
        r.disp_walk_angle = w as f32;
    }
    state.settings_dirty = true;
    let r = &state.settings.render;
    Ok(Outcome::Done(ToolResult::json(json!({
        "show": r.disp_walkable,
        "mode": r.disp_walk_mode.name(),
        "walk_angle": r.disp_walk_angle,
        "source_limit": SOURCE_WALKABLE_DEGREES,
    }))))
}

fn control_slope(state: &mut AppState, a: &Args<'_>) -> R<Outcome> {
    let operation = a.opt_enum("operation", &OPERATIONS)?.ok_or("operation is required")?;
    if operation == "display" {
        return display_slopes(state, a);
    }
    let mut overrides: Vec<(String, String)> = Vec::new();
    if let Some(map) = a.get("options") {
        let map = map.as_object().ok_or("options must be an object: names to values")?;
        let probe = SlopeOptions::default().values();
        for (k, v) in map {
            if !probe.iter().any(|o| o.key == k) {
                let names: Vec<&str> = probe.iter().map(|o| o.key).collect();
                return Err(format!("\"{k}\" is not a slope option (options: {})", names.join(", ")));
            }
            overrides.push((k.clone(), super::tools_toolopts::option_text(k, v)?));
        }
    }
    let path = path_points(a)?;
    let faces = disp_targets(state, a, false)?;
    if state.doc.in_transaction() {
        return Err("Hammer Pro is in the middle of another edit (a drag in a view?); try again".to_owned());
    }
    Ok(Outcome::AppFn(AppFn(Box::new(move |app, _ctx| {
        let (_, mut opts) = tool_options(app);
        for (k, v) in &overrides {
            if !opts.set(k, v) {
                return ToolResult::error(format!("control_slope: {k} cannot be \"{v}\""));
            }
        }
        if let Some(p) = &path {
            opts.path.clone_from(p);
        }
        let before = slope::measure(&app.state.doc, &faces);
        if operation == "measure" {
            return ToolResult::json(json!({ "faces": faces.len(), "slope": range_json(before) }));
        }
        let op = match operation.as_str() {
            "limit" => SlopeOp::Limit,
            "ramp" => SlopeOp::Ramp,
            "uniform" => SlopeOp::Uniform,
            "path" => SlopeOp::Path,
            _ => SlopeOp::Smooth,
        };
        if op == SlopeOp::Path && opts.path.len() < 2 {
            return ToolResult::error("control_slope: path needs path (2 or more points) or the slope_path option");
        }
        let moved = with_faces_selected(&mut app.state, "control_slope", &faces, |s| slope::apply(s, op, &opts));
        match moved {
            Ok(n) => {
                let after = slope::measure(&app.state.doc, &faces);
                ToolResult::json(json!({
                    "operation": operation,
                    "faces": faces.len(),
                    "vertices_moved": n,
                    "before": range_json(before),
                    "after": range_json(after),
                }))
            }
            Err(e) => ToolResult::error(format!("control_slope: {e}")),
        }
    }))))
}

fn select_displacements_by_slope(state: &mut AppState, a: &Args<'_>) -> R<Outcome> {
    let candidates = disp_targets(state, a, true)?;
    let angle = a.opt_f64("angle", 0.0, 90.0)?;
    let mode = a.opt_enum("mode", &["above", "below"])?;
    let which = a.opt_enum("match", &["any", "all"])?;
    let op = a.opt_enum("op", &["replace", "add", "remove"])?;
    if state.doc.in_transaction() {
        return Err("Hammer Pro is in the middle of another edit (a drag in a view?); try again".to_owned());
    }
    Ok(Outcome::AppFn(AppFn(Box::new(move |app, _ctx| {
        let (_, mut opts) = tool_options(app);
        if let Some(n) = angle {
            opts.select_angle = n;
        }
        for (key, value) in [("slope_select_mode", mode), ("slope_select_match", which), ("slope_select_op", op)] {
            if let Some(v) = value {
                opts.set(key, &v);
            }
        }
        let n = slope::select_among(&mut app.state, candidates, &opts);
        let faces: Vec<Value> =
            app.state.doc.selected_faces().iter().map(|f| json!({ "solid": f.solid, "side": f.side })).collect();
        ToolResult::json(json!({ "matched": n, "selected": faces }))
    }))))
}
