use hp_app::{AppState, Tool, ToolId};
use hp_mcp::protocol::ToolResult;
use serde_json::{Value, json};

use super::semantic::*;
use super::tools::{AppFn, Args, Outcome, R};

const TOOL_NAMES: &[&str] = &[
    "select",
    "camera",
    "entity",
    "block",
    "texture",
    "decal",
    "overlay",
    "clip",
    "slice",
    "extrude",
    "bevel",
    "vertex",
    "path",
    "polygon",
    "curve",
    "sprinkle",
    "displacement",
    "cordon",
    "road",
    "pipe",
];

const MAX_OPTIONS: usize = 64;

domain! { "tool options";
    get_tool_options: Read, "Tool options",
        "The options of the editing tools by name with their current values and what each takes: the selection \
        tool (element: object, face, edge, vertex, what a click in a 3D view selects; handles: scale, rotate, skew; box: touching, inside, centers; gizmo on/off and gizmo_mode: move, \
        rotate, scale; gizmo_local; stretch_handles: the arrows on the faces of the selected brushes in the 3D \
        view that stretch the selection along an axis, saved with the settings), the block tool (primitive incl. stairs, spiral_stairs, ramp, pillar, doorway, sides, arch wall, arc, start, rise, torus, steps, step_height, open, inner, spiral_angle, spiral_start, slope, direction, base, capital, inset, door_width, door_height, thickness, door_arch, door_arch_radius, door_arch_segments, reorient, the shape preview window, texture scale, \
        create as, keep active), the curve tool (subdivisions 2 to 4096, thickness 1 to 1024, height or auto, \
        tex_scale, create_as, keep_active), the road tool (lanes, lane_width, shoulder, thickness, step, \
        tex_scale, material, shoulder_material, line_material, center_material, center_line, edge_line, \
        divider_line (none, solid, dashed, double), line_width, dash, gap, roll_start, roll_end, auto_bank, \
        auto_bank_limit, follow_terrain, lift, base_height, create_as, keep_active), the pipe tool (shape, sides, radius, hollow, wall, segments, segment_length, bend_radius, bend_slices, collar_every, collar_size, collar_length, collar_ends, collar_bends, tex_scale, path, heading, depth, keep_active; create_pipe builds a pipe without the tool), the clip tool (keep: front, back, both; three_point; axis of the \
        Shift+click cut in the 3D view: auto, x, y, z; the cut face material follows the \
        brush.clip_current_material setting), the slice tool (orientation: left_right or top_bottom; segments 1 to 64, the number of cuts; slice_brushes slices without the tool), the path tool (class, base_name, loop, relink, offset; saved with the settings), the extrude / carve \
        tool (mode: carve, extrude; depth; through; new_faces: face, current), the bevel tool (radius, segments, \
        inverse, new_faces: face, current), the vertex tool (merge_distance 0 to 16, weld_distance 0.001 to 64 and weld_target: center, first, last, for the weld_brush_vertices tool and the weld actions) and the displacement tool's brush (mode: select, geometry, alpha, sculpt, tag; \
        effect; axis: normal, subdiv, center, view, view_flat (toward the camera on X and Y only), x, y, z, \
        custom; radius, strength (intensity), smooth, \
        target height, falloff, spatial, line of sight, autosew (live sewing), selected only, continuous, alpha \
        effect, value and strength, blend channel, sculpt push, carve, project, pinch, grab, inflate and flatten settings with bounds limit and \
        falloff position and final, tag kind and value, grid and select masks, and the terrain options of the Tools section: noise_type random or fractal, noise_scale, noise_octaves, noise_persistence, terrain_seed, thermal_* and hydraulic_*, run by terrain_displacements, the height blending options blend_*, run by blend_displacements, and the slope options slope_*, run by control_slope and select_displacements_by_slope). The displacement options are \
        saved with the settings. Default: every tool that has options, and which tool is active.",
        { "tool": choice("Only this tool.", TOOL_NAMES) }, [];
    set_tool_options: Editor, "Change tool options",
        "Change options of an editing tool like its panel does (names and values from get_tool_options), e.g. \
        tool select, options {\"stretch_handles\": false}, or tool displacement, options {\"axis\": \"view\", \"strength\": 200, \"autosew\": true}, or tool block, \
        options {\"kind\": \"arch\", \"arc\": 180, \"preview\": true}. Values are text, numbers, booleans or \
        lists of numbers (custom_axis, carve_profile). With activate the tool becomes the active one. Answers \
        the tool's options.",
        {
            "tool": choice("The tool (default: the active one).", TOOL_NAMES),
            "options": { "type": "object", "description": "Option names to values." },
            "activate": boolean("Make it the active tool (default false)."),
        }, ["options"];
}

fn tool_name(id: ToolId) -> &'static str {
    ToolId::ALL.iter().position(|t| *t == id).and_then(|i| TOOL_NAMES.get(i).copied()).unwrap_or("select")
}

fn tool_of(name: &str) -> Option<ToolId> {
    TOOL_NAMES.iter().position(|n| *n == name).and_then(|i| ToolId::ALL.get(i).copied())
}

fn options_json(tool: &dyn Tool) -> Value {
    Value::Array(
        tool.option_values()
            .into_iter()
            .map(|o| json!({ "name": o.key, "value": o.value, "takes": o.takes }))
            .collect(),
    )
}

pub(crate) fn tool_options_json(tools: &[Box<dyn Tool>], active: ToolId, which: Option<ToolId>) -> Value {
    let list: Vec<Value> = tools
        .iter()
        .filter(|t| which.is_none_or(|w| t.id() == w))
        .filter(|t| which.is_some() || !t.option_values().is_empty())
        .map(|t| json!({ "tool": tool_name(t.id()), "active": t.id() == active, "options": options_json(t.as_ref()) }))
        .collect();
    json!({ "active_tool": tool_name(active), "tools": list })
}

pub(crate) fn option_text(name: &str, v: &Value) -> R<String> {
    Ok(match v {
        Value::Bool(b) => if *b { "on" } else { "off" }.to_owned(),
        Value::Number(n) => match n.as_f64().filter(|x| x.is_finite()) {
            Some(x) => format!("{x}"),
            None => return Err(format!("{name}: the number must be finite")),
        },
        Value::String(s) if s.len() <= 128 => s.trim().to_owned(),
        Value::Array(list) if list.len() <= 8 => {
            let nums: Option<Vec<String>> =
                list.iter().map(|x| x.as_f64().filter(|f| f.is_finite()).map(|f| format!("{f}"))).collect();
            nums.ok_or_else(|| format!("{name}: a list holds numbers only"))?.join(" ")
        }
        _ => return Err(format!("{name}: give text, a number, a boolean or a short list of numbers")),
    })
}

pub(crate) fn set_tool_options_on(
    tools: &mut [Box<dyn Tool>],
    state: &mut AppState,
    id: ToolId,
    options: &[(String, String)],
) -> ToolResult {
    let Some(tool) = tools.iter_mut().find(|t| t.id() == id) else {
        return ToolResult::error(format!("set_tool_options: the {} tool is not available", tool_name(id)));
    };
    let known = tool.option_values();
    if known.is_empty() {
        return ToolResult::error(format!(
            "set_tool_options: the {} tool has no options to set (the selection, block, curve, road, clip, extrude, bevel, vertex, path and displacement tools have)",
            tool_name(id)
        ));
    }
    if let Some((k, _)) = options.iter().find(|(k, _)| !known.iter().any(|o| o.key == k)) {
        let names: Vec<&str> = known.iter().map(|o| o.key).collect();
        return ToolResult::error(format!(
            "set_tool_options: the {} tool has no option \"{k}\" (options: {})",
            tool_name(id),
            names.join(", ")
        ));
    }
    let mut set = Vec::new();
    for (k, v) in options {
        if !tool.set_option(state, k, v) {
            let takes = known.iter().find(|o| o.key == k).map_or("", |o| o.takes);
            let done = if set.is_empty() { String::new() } else { format!(" ({} were set)", set.join(", ")) };
            return ToolResult::error(format!("set_tool_options: {k} cannot be \"{v}\": it takes {takes}{done}"));
        }
        set.push(k.as_str());
    }
    ToolResult::json(json!({ "tool": tool_name(id), "set": set, "options": options_json(tool.as_ref()) }))
}

fn get_tool_options(_state: &mut AppState, a: &Args<'_>) -> R<Outcome> {
    let which = a.opt_enum("tool", TOOL_NAMES)?.and_then(|n| tool_of(&n));
    Ok(Outcome::AppFn(AppFn(Box::new(move |app, _ctx| {
        ToolResult::json(tool_options_json(&app.tools, app.state.tool, which))
    }))))
}

fn set_tool_options(state: &mut AppState, a: &Args<'_>) -> R<Outcome> {
    let id = match a.opt_enum("tool", TOOL_NAMES)? {
        Some(n) => tool_of(&n).ok_or("unknown tool")?,
        None => state.tool,
    };
    let map = a.get("options").and_then(Value::as_object).ok_or("options must be an object: names to values")?;
    if map.is_empty() || map.len() > MAX_OPTIONS {
        return Err(format!("give 1 to {MAX_OPTIONS} options"));
    }
    let mut options = Vec::with_capacity(map.len());
    for (k, v) in map {
        if k.is_empty() || k.len() > 64 || !k.chars().all(|c| c.is_ascii_alphanumeric() || c == '_') {
            return Err(format!("\"{k}\" is not an option name"));
        }
        options.push((k.clone(), option_text(k, v)?));
    }
    let activate = a.opt_bool("activate")?.unwrap_or(false);
    if activate && !id.is_ready() {
        return Err(format!("the {} tool is not available", tool_name(id)));
    }
    Ok(Outcome::AppFn(AppFn(Box::new(move |app, _ctx| {
        let r = set_tool_options_on(&mut app.tools, &mut app.state, id, &options);
        if activate && !r.is_error && app.state.tool != id {
            app.state.request(id.action());
        }
        r
    }))))
}

#[cfg(test)]
mod tests {
    use hp_app::ToolId;
    use serde_json::{Value, json};

    use super::{set_tool_options_on, tool_options_json};
    use crate::dispatch::mcp::semantic_tests::{err, needs_app};

    fn tools() -> Vec<Box<dyn hp_app::Tool>> {
        let mut t = hp_tools_select::tools();
        t.extend(hp_tools_create::tools());
        t.extend(hp_tools_mesh::tools());
        t
    }

    fn value(v: &Value, tool: &str, name: &str) -> String {
        let t = v["tools"].as_array().unwrap().iter().find(|t| t["tool"] == tool).expect("tool");
        let o = t["options"].as_array().unwrap().iter().find(|o| o["name"] == name).expect("option");
        o["value"].as_str().unwrap().to_owned()
    }

    #[test]
    fn tool_names_follow_the_tools() {
        assert_eq!(super::TOOL_NAMES.len(), ToolId::ALL.len());
        for id in ToolId::ALL {
            let name = super::tool_name(id);
            assert_eq!(name, format!("{id:?}").to_ascii_lowercase(), "{id:?}");
            assert_eq!(super::tool_of(name), Some(id));
        }
    }

    #[test]
    fn options_of_the_block_and_displacement_tools() {
        let mut state = hp_app::testing::test_state();
        let mut tools = tools();
        let all = tool_options_json(&tools, ToolId::Select, None);
        assert_eq!(all["active_tool"], "select");
        assert_eq!(value(&all, "displacement", "axis"), "normal");
        assert_eq!(value(&all, "block", "kind"), "block");
        let opts = |list: &[(&str, &str)]| -> Vec<(String, String)> {
            list.iter().map(|(k, v)| ((*k).to_owned(), (*v).to_owned())).collect()
        };
        let r = set_tool_options_on(
            &mut tools,
            &mut state,
            ToolId::Displacement,
            &opts(&[("axis", "view"), ("strength", "300"), ("autosew", "on"), ("custom_axis", "1 0 0")]),
        );
        assert!(!r.is_error, "{}", r.first_text());
        let now = tool_options_json(&tools, ToolId::Select, Some(ToolId::Displacement));
        assert_eq!(value(&now, "displacement", "axis"), "view");
        assert_eq!(value(&now, "displacement", "strength"), "300");
        assert_eq!(value(&now, "displacement", "autosew"), "on");
        assert_eq!(value(&now, "displacement", "custom_axis"), "1 0 0");
        assert!(set_tool_options_on(&mut tools, &mut state, ToolId::Displacement, &opts(&[("nope", "1")])).is_error);
        assert!(set_tool_options_on(&mut tools, &mut state, ToolId::Displacement, &opts(&[("radius", "-5")])).is_error);
        assert!(set_tool_options_on(&mut tools, &mut state, ToolId::Displacement, &opts(&[("create", "3")])).is_error);
        let r = set_tool_options_on(
            &mut tools,
            &mut state,
            ToolId::Block,
            &opts(&[
                ("kind", "arch"),
                ("arc", "180"),
                ("preview", "on"),
                ("reorient", "off"),
                ("create_as", "func_detail"),
            ]),
        );
        assert!(!r.is_error, "{}", r.first_text());
        let now = tool_options_json(&tools, ToolId::Block, Some(ToolId::Block));
        assert_eq!(value(&now, "block", "kind"), "arch");
        assert_eq!(value(&now, "block", "arc"), "180");
        assert_eq!(value(&now, "block", "preview"), "on");
        assert_eq!(value(&now, "block", "create_as"), "func_detail");
        assert!(!state.settings.brush.reorient_primitives);
        let r = set_tool_options_on(
            &mut tools,
            &mut state,
            ToolId::Curve,
            &opts(&[("subdivisions", "300"), ("thickness", "24"), ("height", "96"), ("create_as", "func_detail")]),
        );
        assert!(!r.is_error, "{}", r.first_text());
        let now = tool_options_json(&tools, ToolId::Curve, Some(ToolId::Curve));
        assert_eq!(value(&now, "curve", "subdivisions"), "300");
        assert_eq!(value(&now, "curve", "thickness"), "24");
        assert_eq!(value(&now, "curve", "height"), "96");
        assert!(
            set_tool_options_on(&mut tools, &mut state, ToolId::Curve, &opts(&[("subdivisions", "4097")])).is_error
        );
        assert!(set_tool_options_on(&mut tools, &mut state, ToolId::Curve, &opts(&[("thickness", "0")])).is_error);
        for t in &mut tools {
            for o in t.option_values() {
                assert!(t.set_option(&mut state, o.key, &o.value), "{:?} {} {}", t.id(), o.key, o.value);
            }
        }
        let r = set_tool_options_on(
            &mut tools,
            &mut state,
            ToolId::Select,
            &opts(&[("stretch_handles", "off"), ("gizmo_mode", "rotate"), ("handles", "skew")]),
        );
        assert!(!r.is_error, "{}", r.first_text());
        assert!(!state.settings.selection.stretch_handles);
        let now = tool_options_json(&tools, ToolId::Select, Some(ToolId::Select));
        assert_eq!(value(&now, "select", "stretch_handles"), "off");
        assert_eq!(value(&now, "select", "gizmo_mode"), "rotate");
        assert!(set_tool_options_on(&mut tools, &mut state, ToolId::Select, &opts(&[("box", "round")])).is_error);
        assert!(set_tool_options_on(&mut tools, &mut state, ToolId::Clip, &opts(&[("mode", "x")])).is_error);
        assert!(needs_app(&mut state, "get_tool_options", json!({ "tool": "block" })));
        assert!(needs_app(
            &mut state,
            "set_tool_options",
            json!({ "tool": "displacement", "options": { "axis": "view" } })
        ));
        err(&mut state, "set_tool_options", json!({ "options": {} }));
        err(&mut state, "set_tool_options", json!({ "options": { "a b": 1 } }));
        err(&mut state, "set_tool_options", json!({ "options": { "radius": { "x": 1 } } }));
    }
}
