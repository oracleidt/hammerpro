use glam::DVec3;
use hp_app::{AppState, ToolId};
use hp_mcp::protocol::ToolResult;
use hp_tools_mesh::disp::level::{
    SampleInto, brush_from_values, pick_surface, point_text, probe_ray, sample_report, slope_grade,
};
use serde_json::{Value, json};

use super::semantic::*;
use super::tools::{AppFn, Args, Outcome, R, coord_ok, undo_label, v3};

const MAX_POINTS: usize = 4096;
const INTO: [&str; 5] = ["target", "slope_start", "slope_end", "alpha", "none"];

domain! { "displacement height sampling and slopes";
    sample_displacement_height: Editor, "Sample a height",
        "Sample the surface under a point, like a right click with the displacement tool (Raise to, Flatten with the target plane, Slope, Alpha set): the probe starts 64 units before point and goes along direction (default straight down, [0, 0, -1]) to the first displacement or brush surface. into says where the height goes: target (default: target_height, measured like height_reference: above the base face of the displacement under the point, or the world height along the paint axis; height_reference becomes world when no displacement is under the point), slope_start or slope_end (the point of the slope effect), alpha (alpha_value: the alpha of the nearest vertex) or none (only answer). With into target the point becomes sample_point: the 3D view marks it with a green dot, a green ring at the target level and a label, like a right click, so screenshots show it. Answers the point hit, the displacement face hit or under it, the world height along the paint axis, the height above that face, the target written and the tool options after.",
        {
            "point": vec3("Where to sample (world). The probe starts 64 units before it."),
            "direction": vec3("Direction of the probe (default [0, 0, -1]: straight down)."),
            "into": choice("Where the sampled value goes (default target).", &INTO),
        }, ["point"];
    paint_displacement_slope: Edit, "Paint a displacement slope",
        "Paint a slope between two points with the displacement tool's brush (Paint geometry, effect Slope), like a drag in the 3D view: each vertex under the brush moves along the paint axis toward the height of the line from start to end at its place (heights interpolated between the two points, clamped beyond them), with the brush radius, falloff and the strength (share of the way per sample, default 1: exactly on the slope). Sewn displacements are handled as one surface (the same world line for all) and the selected only rule applies like in the tool. start and end default to the slope_brush_start and slope_brush_end tool options and are stored there; points is the stroke (default: the segment from start to end, a sample every quarter radius). Use it for road ramps or to blend two heights. One undo step. Answers the grade and what changed.",
        {
            "start": vec3("Start of the slope (world): its height along the paint axis is the start height."),
            "end": vec3("End of the slope (world): its height is the end height."),
            "points": { "type": "array", "items": vec3("Brush center (world)."), "minItems": 1, "maxItems": MAX_POINTS, "description": "Stroke samples in order (default: from start to end)." },
            "strength": number("Share of the way to the slope per sample (default 1).", 0.0, 1.0),
            "radius": number("Brush radius for this call (default the tool's).", 1.0, 16384.0),
            "faces": faces("Only these displacements change (default: the selected ones, else every one)."),
        }, [];
}

fn sample_displacement_height(_state: &mut AppState, a: &Args<'_>) -> R<Outcome> {
    let point = a.vec3("point")?;
    let direction = a.opt_vec3("direction")?.unwrap_or(DVec3::NEG_Z);
    if direction.length_squared() < 1e-12 {
        return Err("direction must not be zero".to_owned());
    }
    let into = a.opt_enum("into", &INTO)?.unwrap_or_else(|| "target".to_owned());
    Ok(Outcome::AppFn(AppFn(Box::new(move |app, _ctx| {
        run_sample(&mut app.tools, &mut app.state, point, direction, &into)
    }))))
}

pub(crate) fn run_sample(
    tools: &mut [Box<dyn hp_app::Tool>],
    state: &mut AppState,
    point: DVec3,
    direction: DVec3,
    into: &str,
) -> ToolResult {
    let Some(tool) = tools.iter_mut().find(|t| t.id() == ToolId::Displacement) else {
        return ToolResult::error("sample_displacement_height: the displacement tool is not available");
    };
    let brush = brush_from_values(&tool.option_values());
    let Some(hit) = pick_surface(state, &probe_ray(point, direction)) else {
        return ToolResult::error(
            "sample_displacement_height: no displacement or brush surface along the probe (try another point or direction)",
        );
    };
    let report = sample_report(&state.doc, hit, &brush);
    let mut set: Vec<(&str, String)> = Vec::new();
    match SampleInto::parse(into) {
        Some(SampleInto::Target) => {
            set.push(("height_reference", if report.world { "world" } else { "face" }.to_owned()));
            set.push(("target_height", format!("{}", report.target.clamp(-16384.0, 16384.0))));
            set.push(("sample_point", point_text(Some(hit.point))));
        }
        Some(SampleInto::SlopeStart) => set.push(("slope_brush_start", point_text(Some(hit.point)))),
        Some(SampleInto::SlopeEnd) => set.push(("slope_brush_end", point_text(Some(hit.point)))),
        Some(SampleInto::Alpha) => match report.alpha {
            Some(alpha) => set.push(("alpha_value", format!("{}", alpha.round()))),
            None => {
                return ToolResult::error("sample_displacement_height: alpha needs a displacement under the point");
            }
        },
        None => {}
    }
    for (k, v) in &set {
        if !tool.set_option(state, k, v) {
            return ToolResult::error(format!("sample_displacement_height: could not set {k} to {v}"));
        }
    }
    let options = tool.option_values();
    let pick = |key: &str| options.iter().find(|o| o.key == key).map(|o| o.value.clone());
    ToolResult::json(json!({
        "point": v3(hit.point),
        "surface": if hit.disp.is_some() { "displacement" } else { "brush" },
        "face": report.face.map(face_json),
        "axis": v3(report.axis),
        "world_height": round(report.world_height),
        "height_above_face": report.face_height.map(round),
        "reference": if report.world { "world" } else { "face" },
        "target": round(report.target),
        "alpha": report.alpha.map(round),
        "into": into,
        "set": set.iter().map(|(k, _)| *k).collect::<Vec<_>>(),
        "target_height": pick("target_height"),
        "height_reference": pick("height_reference"),
        "slope_brush_start": pick("slope_brush_start"),
        "slope_brush_end": pick("slope_brush_end"),
        "alpha_value": pick("alpha_value"),
        "sample_point": pick("sample_point"),
    }))
}

fn round(x: f64) -> f64 {
    (x * 1000.0).round() / 1000.0
}

fn points_of(a: &Args<'_>) -> R<Option<Vec<DVec3>>> {
    let Some(list) = a.get("points") else { return Ok(None) };
    let list = list.as_array().filter(|l| !l.is_empty() && l.len() <= MAX_POINTS);
    let list = list.ok_or_else(|| format!("points must be 1 to {MAX_POINTS} [x, y, z] points"))?;
    let mut out = Vec::with_capacity(list.len());
    for p in list {
        let xyz: Vec<f64> = p
            .as_array()
            .filter(|x| x.len() == 3)
            .into_iter()
            .flatten()
            .filter_map(Value::as_f64)
            .filter(|f| coord_ok(*f))
            .collect();
        let [x, y, z] = xyz.as_slice() else {
            return Err("points must contain [x, y, z] points".to_owned());
        };
        out.push(DVec3::new(*x, *y, *z));
    }
    Ok(Some(out))
}

pub(crate) fn segment_points(a: DVec3, b: DVec3, radius: f64) -> Vec<DVec3> {
    let spacing = (radius * 0.25).max(1.0);
    let steps = ((a.distance(b) / spacing).ceil() as usize).clamp(1, MAX_POINTS - 1);
    (0..=steps).map(|i| a.lerp(b, i as f64 / steps as f64)).collect()
}

fn paint_displacement_slope(state: &mut AppState, a: &Args<'_>) -> R<Outcome> {
    let start = a.opt_vec3("start")?;
    let end = a.opt_vec3("end")?;
    let points = points_of(a)?;
    let strength = a.opt_f64("strength", 0.0, 1.0)?.unwrap_or(1.0);
    let radius = a.opt_f64("radius", 1.0, 16384.0)?;
    let faces = opt_faces(&state.doc, a, "faces")?;
    if let Some(list) = &faces {
        if list.is_empty() {
            return Err("faces is empty".to_owned());
        }
        if let Some(f) = list.iter().find(|f| state.doc.face_side(**f).is_none_or(|s| s.disp.is_none())) {
            return Err(format!("solid {} side {} is not a displacement", f.solid, f.side));
        }
    }
    if state.doc.in_transaction() {
        return Err("Hammer Pro is in the middle of another edit (a drag in a view?); try again".to_owned());
    }
    let call = SlopeCall { start, end, points, strength, radius, faces };
    Ok(Outcome::AppFn(AppFn(Box::new(move |app, _ctx| run_slope(&mut app.tools, &mut app.state, &call)))))
}

pub(crate) struct SlopeCall {
    pub start: Option<DVec3>,
    pub end: Option<DVec3>,
    pub points: Option<Vec<DVec3>>,
    pub strength: f64,
    pub radius: Option<f64>,
    pub faces: Option<Vec<hp_editor::FaceRef>>,
}

pub(crate) fn run_slope(tools: &mut [Box<dyn hp_app::Tool>], state: &mut AppState, call: &SlopeCall) -> ToolResult {
    let SlopeCall { start, end, points, strength, radius, faces } = call;
    let (start, end, strength, radius) = (*start, *end, *strength, *radius);
    let cam = (state.cameras.cam3d.pos, state.cameras.cam3d.forward());
    let Some(tool) = tools.iter_mut().find(|t| t.id() == ToolId::Displacement) else {
        return ToolResult::error("paint_displacement_slope: the displacement tool is not available");
    };
    let before = tool.option_values();
    let value = |key: &str| before.iter().find(|o| o.key == key).map(|o| o.value.clone()).unwrap_or_default();
    let kept: Vec<(&str, String)> =
        ["mode", "effect", "slope_brush_strength", "radius"].iter().map(|k| (*k, value(k))).collect();
    let mut set: Vec<(&str, String)> = Vec::new();
    if let Some(p) = start {
        set.push(("slope_brush_start", point_text(Some(p))));
    }
    if let Some(p) = end {
        set.push(("slope_brush_end", point_text(Some(p))));
    }
    set.push(("mode", "geometry".to_owned()));
    set.push(("effect", "slope".to_owned()));
    set.push(("slope_brush_strength", format!("{strength}")));
    if let Some(r) = radius {
        set.push(("radius", format!("{r}")));
    }
    let restore = |tool: &mut Box<dyn hp_app::Tool>, state: &mut AppState| {
        for (k, v) in &kept {
            tool.set_option(state, k, v);
        }
    };
    for (k, v) in &set {
        if !tool.set_option(state, k, v) {
            restore(tool, state);
            return ToolResult::error(format!("paint_displacement_slope: {k} cannot be {v}"));
        }
    }
    let brush = brush_from_values(&tool.option_values());
    let Some((a, b)) = brush.slope_start.zip(brush.slope_end) else {
        restore(tool, state);
        return ToolResult::error(
            "paint_displacement_slope: give start and end (or set slope_brush_start and slope_brush_end, or sample them with sample_displacement_height)",
        );
    };
    let points = points.clone().unwrap_or_else(|| segment_points(a, b, brush.radius));
    let stroke = hp_app::tool::ScriptedStroke {
        points,
        invert: false,
        smooth: false,
        boost: false,
        seconds_per_sample: 1.0 / 60.0,
        eye: Some(cam.0),
        forward: Some(cam.1),
        faces: faces.clone(),
        undo_label: undo_label("paint_displacement_slope"),
    };
    let out = tool.scripted_stroke(state, &stroke);
    restore(tool, state);
    let axis = hp_tools_mesh::disp::level::level_axis(&brush, Some(DVec3::Z));
    let grade = slope_grade(a, b, axis);
    match out {
        None => ToolResult::error("paint_displacement_slope: the displacement tool has no brush"),
        Some(Err(e)) => ToolResult::error(format!("paint_displacement_slope: {e}")),
        Some(Ok(out)) => {
            let mut v = json!({
                "start": v3(a),
                "end": v3(b),
                "start_height": round(a.dot(axis)),
                "end_height": round(b.dot(axis)),
                "degrees": grade.map(|g| round(g.0)),
                "percent": grade.map(|g| round(g.1)),
                "length": grade.map(|g| round(g.2)),
                "samples": stroke.points.len(),
                "samples_painted": out.samples_painted,
                "vertices_changed": out.vertices_changed,
                "faces": out.faces.iter().map(|f| face_json(*f)).collect::<Vec<_>>(),
            });
            if let Some(note) = out.note {
                v["note"] = json!(note);
            }
            ToolResult::json(v)
        }
    }
}

#[cfg(test)]
mod tests {
    #![allow(clippy::unwrap_used, clippy::expect_used, clippy::panic, clippy::float_cmp)]

    use super::*;

    #[test]
    fn segment_points_cover_the_segment() {
        let pts = segment_points(DVec3::ZERO, DVec3::new(100.0, 0.0, 50.0), 64.0);
        assert_eq!(pts.first().copied(), Some(DVec3::ZERO));
        assert_eq!(pts.last().copied(), Some(DVec3::new(100.0, 0.0, 50.0)));
        assert!(pts.windows(2).all(|w| w[0].distance(w[1]) <= 16.0 + 1e-9));
        assert_eq!(segment_points(DVec3::ZERO, DVec3::ZERO, 64.0).len(), 2);
        assert!(segment_points(DVec3::ZERO, DVec3::new(1e6, 0.0, 0.0), 1.0).len() <= MAX_POINTS);
    }

    fn option(tools: &[Box<dyn hp_app::Tool>], key: &str) -> String {
        let tool = tools.iter().find(|t| t.id() == ToolId::Displacement).unwrap();
        tool.option_values().into_iter().find(|o| o.key == key).map(|o| o.value).unwrap()
    }

    #[test]
    fn sample_and_slope_through_mcp() {
        use super::super::semantic_tests::{err, needs_app, ok};
        let mut s = hp_app::testing::test_state();
        let mut tools = hp_tools_mesh::tools();
        assert!(needs_app(&mut s, "sample_displacement_height", json!({ "point": [0, 0, 0] })));
        assert!(needs_app(&mut s, "paint_displacement_slope", json!({ "start": [0, 0, 0], "end": [1, 0, 0] })));
        err(&mut s, "sample_displacement_height", json!({}));
        err(&mut s, "sample_displacement_height", json!({ "point": [0, 0, 0], "direction": [0, 0, 0] }));
        err(&mut s, "paint_displacement_slope", json!({ "points": [] }));
        err(&mut s, "paint_displacement_slope", json!({ "faces": [{ "solid": 2, "side": 0 }] }));
        ok(&mut s, "create_displacements", json!({ "faces": [{ "solid": 2, "side": 0 }], "power": 3 }));
        s.doc.clear_selection();
        let tool = tools.iter_mut().find(|t| t.id() == ToolId::Displacement).unwrap();
        assert!(tool.set_option(&mut s, "falloff", "constant"));

        let r = run_sample(&mut tools, &mut s, DVec3::new(10.0, 10.0, 5.0), DVec3::NEG_Z, "target");
        assert!(!r.is_error, "{}", r.first_text());
        let v = r.structured.clone().unwrap();
        assert_eq!(v["surface"], "displacement");
        assert_eq!(v["reference"], "face");
        assert_eq!(option(&tools, "target_height"), "0");
        assert_eq!(option(&tools, "sample_point"), "10 10 0", "the sampled point is shown like a right click");
        let r = run_sample(&mut tools, &mut s, DVec3::new(0.0, 0.0, 500.0), DVec3::Z, "target");
        assert!(r.is_error, "nothing above");

        let r = run_slope(
            &mut tools,
            &mut s,
            &SlopeCall {
                start: Some(DVec3::new(-200.0, 0.0, 0.0)),
                end: Some(DVec3::new(200.0, 0.0, 80.0)),
                points: None,
                strength: 1.0,
                radius: Some(200.0),
                faces: None,
            },
        );
        assert!(!r.is_error, "{}", r.first_text());
        let v = r.structured.clone().unwrap();
        assert!(v["vertices_changed"].as_u64().unwrap() > 0, "{v}");
        assert!((v["percent"].as_f64().unwrap() - 20.0).abs() < 1e-6);
        assert_eq!(s.doc.undo_label(), Some("MCP: paint_displacement_slope"));
        assert_eq!(option(&tools, "effect"), "raise", "the effect is restored");
        assert_eq!(option(&tools, "radius"), "64", "the radius is restored");
        assert_eq!(option(&tools, "slope_brush_end"), "200 0 80", "the points stay");
        let m = s.doc.scene().solid(2).and_then(|n| n.disp_mesh(0)).unwrap();
        for (b, p) in m.base_positions.iter().zip(&m.positions) {
            if b.x.abs() <= 150.0 && b.y.abs() <= 100.0 {
                let want = (b.x + 200.0) / 400.0 * 80.0;
                assert!((p.z - want).abs() < 1e-6, "{p} {want}");
            }
        }
        let r = run_sample(&mut tools, &mut s, DVec3::new(-100.0, -50.0, 200.0), DVec3::NEG_Z, "slope_start");
        assert!(!r.is_error);
        assert!(
            option(&tools, "slope_brush_start").starts_with("-100 -50 20"),
            "{}",
            option(&tools, "slope_brush_start")
        );
    }
}
