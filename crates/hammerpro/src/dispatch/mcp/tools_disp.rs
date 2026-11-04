use glam::DVec3;
use hp_app::AppState;
use hp_editor::{FaceRef, SelectOp};
use hp_geom::{Falloff, PaintBrush, PaintMode};
use hp_mcp::protocol::ToolResult;
use serde_json::{Value, json};

use super::semantic::*;
use super::tools::{AppFn, Args, Outcome, R, Worker, edit, v3};

const MAX_STROKES: usize = 4096;

domain! { "displacements";
    list_displacements: Read, "List displacements",
        "Every displacement face of the map (or of some solids): face, power, elevation, flags and the bounds of \
        its surface.",
        { "solid_ids": ids("Only these solids."), "limit": limit(), "offset": offset() }, [];
    displacement_to_mesh: Editor, "Displacement to mesh",
        "Write displacements to an OBJ file (and its MTL) for Blender or any 3D tool, like the Displacement to mesh command: the displaced vertices, normals, UVs from the texture mapping and one material per texture. Targets: faces, ids or solid_ids, else the selected displacements or those of the selected solids. path is a full path or relative to the map folder (default: <map>_displacements.obj next to the map). units source (default) or meters (1 unit = 0.01905 m); up y (default) or z. Defaults come from the Displacement panel (settings mesh_export_meters and mesh_export_z_up).",
        {
            "faces": faces("Faces (default: the selected displacements)."), "ids": target_ids(), "solid_ids": target_solid_ids(),
            "path": string("OBJ file: full path, or relative to the map folder."),
            "units": choice("Unit of the coordinates.", &["source", "meters"]),
            "up": choice("Up axis.", &["y", "z"]),
            "overwrite": boolean("Replace existing files (default false)."),
        }, [];
    select_displacements: Editor, "Select displacements",
        "Select displacement faces (every one, or those of some solids) for the displacement tools and Apply; \
        op adjacent adds the displacements sharing an edge with the selected ones (the panel's Select adjacent).",
        { "solid_ids": ids("Only the displacements of these solids."), "op": choice("How the selection changes (default replace).", &["replace", "add", "remove", "adjacent"]) }, [];
    get_displacement: Read, "Displacement data",
        "The data of one displacement face: power, row length, start position, elevation, flags, and per vertex \
        world positions, distances and alphas (row by row). multiblend holds the 4-way blend rows written by \
        Hammer++ and CS:GO Hammer and painted with the blend channels (null when the face has none): weights \
        (the amount of textures 1 to 4 per vertex; the engine lays texture 2, 3 then 4 over texture 1 by the \
        second, third and fourth value), alphablend and colors (4 lists of RGB, read by the Multiblend shader \
        only). A Lightmapped_4WayBlend displacement without multiblend shows its vertex alpha as texture 2, in \
        the editor and in the compiled map.",
        { "face": faces("The face (one item)."), "positions": boolean("Include the world positions (default true).") }, ["face"];
    create_displacements: Edit, "Create displacements",
        "Turn quad faces into flat displacements of power 2, 3 or 4 (default 3).",
        { "faces": faces("Faces (default: the selected faces)."), "ids": target_ids(), "solid_ids": target_solid_ids(), "power": integer("Power.", 2, 4) }, [];
    destroy_displacements: Destructive, "Destroy displacements",
        "Turn displacement faces back into plain faces.",
        { "faces": faces("Faces (default: the selected displacements)."), "ids": target_ids(), "solid_ids": target_solid_ids() }, [];
    paint_displacement: Edit, "Paint displacements",
        "Paint strokes on displacements like the displacement tool: raise, lower, smooth, set (height above the \
        face) or alpha, with a radius, an amount and a falloff. Each stroke sample is a center; all samples of a \
        call are one undo step.",
        {
            "faces": faces("Displacements painted (default: the selected ones, else every displacement)."),
            "mode": choice("Operation.", &["raise", "lower", "smooth", "set", "alpha"]),
            "centers": { "type": "array", "items": vec3("Sample center."), "minItems": 1, "maxItems": MAX_STROKES, "description": "Stroke sample centers (world)." },
            "radius": number("Brush radius in units.", 1.0, 16384.0),
            "amount": number("Units for raise, lower and set; 0 to 1 for smooth; -255 to 255 for alpha.", -16384.0, 16384.0),
            "falloff": choice("Fade from the center (default smooth).", &["constant", "linear", "smooth"]),
            "spatial": boolean("Measure the distance in 3D on the surface (default false: in the face plane)."),
        }, ["mode", "centers", "radius", "amount"];
    stroke_displacement: Edit, "Brush stroke on displacements",
        "Paint a stroke through world points with the displacement tool's own brush, exactly like a drag in the 3D \
        view: its mode (geometry, alpha, sculpt), effect, paint axis (normal, subdiv, z, custom, view: toward \
        the 3D camera as it is now, or view_flat: toward it on X and Y only, never up or down), radius, intensity (strength), falloff, spatial, line of sight, live sewing \
        (autosew), blend channel and sculpt settings, read and changed with get_tool_options and \
        set_tool_options (tool displacement). Like the tool, only the selected displacements change when some \
        are selected (faces limits it instead), their unselected neighbours are pinned. Each point is one brush \
        sample; rate based effects (raise, lower, smooth, alpha) apply seconds_per_sample of their per second \
        strength. One undo step.",
        {
            "points": { "type": "array", "items": vec3("Brush center (world, on the surface)."), "minItems": 1, "maxItems": MAX_STROKES, "description": "Stroke samples in order." },
            "invert": boolean("The inverted effect (Shift while painting: lower instead of raise, remove alpha; Ctrl while sculpting)."),
            "smooth": boolean("Sculpt mode: smooth instead (Shift while sculpting)."),
            "seconds_per_sample": number("Time each sample stands for (default 1/60: one frame).", 0.001, 10.0),
            "faces": faces("Only these displacements change (default: the selected ones, else every one)."),
        }, ["points"];
    displacement_noise: Edit, "Displacement noise",
        "Add random height noise between min and max units to displacements.",
        {
            "faces": faces("Faces (default: the selected displacements)."), "ids": target_ids(), "solid_ids": target_solid_ids(),
            "min": number("Lowest offset.", -4096.0, 4096.0), "max": number("Highest offset.", -4096.0, 4096.0),
            "seed": integer("Random seed (default from the map).", 0, i64::from(u32::MAX)),
        }, ["min", "max"];
    terrain_displacements: Edit, "Terrain noise and erosion",
        "Run fractal noise, thermal erosion or hydraulic erosion on displacements (the Tools section of the displacement panel), one undo step, sewn afterwards. fractal_noise adds Perlin noise (octaves) between min and max (default: the noise range settings) along the face normal and is the same everywhere on the map, so neighbours match. thermal_erosion slides material down slopes steeper than the talus angle. hydraulic_erosion lets rain wash the terrain (sediment is dug and dropped, mass kept). Both erosions treat the sewn, connected displacements as one height field and work per displacement for the others. Their parameters are the displacement tool options (get_tool_options: noise_scale, noise_octaves, noise_persistence, terrain_seed, thermal_iterations, thermal_talus, thermal_strength, hydraulic_iterations, hydraulic_rain, hydraulic_evaporation, hydraulic_capacity, hydraulic_deposition, hydraulic_strength); options overrides them for this call only. The same seed gives the same result.",
        {
            "operation": choice("What to run.", &["fractal_noise", "thermal_erosion", "hydraulic_erosion"]),
            "faces": faces("Faces (default: the selected displacements)."), "ids": target_ids(), "solid_ids": target_solid_ids(),
            "options": { "type": "object", "description": "Terrain option names to values for this call (same names as the displacement tool options)." },
            "min": number("fractal_noise: lowest height (default the noise_min setting).", -1024.0, 1024.0),
            "max": number("fractal_noise: highest height (default the noise_max setting).", -1024.0, 1024.0),
        }, ["operation"];
    scale_displacements: Edit, "Scale displacements",
        "Multiply the heights of displacements by a factor.",
        { "faces": faces("Faces (default: the selected displacements)."), "ids": target_ids(), "solid_ids": target_solid_ids(), "factor": number("Factor.", -100.0, 100.0) }, ["factor"];
    invert_displacement_alpha: Edit, "Invert displacement alpha",
        "Invert the blend alpha of displacements (255 minus alpha).",
        { "faces": faces("Faces (default: the selected displacements)."), "ids": target_ids(), "solid_ids": target_solid_ids() }, [];
    set_displacement_flags: Edit, "Displacement flags",
        "Switch the collision flags of displacements (no physics, no hull (player) and no ray collisions, written \
        to the map like Hammer and honoured by the compiler) and whether they cast shadows when the map is lit \
        (cast_shadows, Hammer Pro's compiler; they are still lit).",
        {
            "faces": faces("Faces (default: the selected displacements)."), "ids": target_ids(), "solid_ids": target_solid_ids(),
            "no_physics": boolean("No physics collisions."), "no_hull": boolean("No player and NPC collisions."), "no_ray": boolean("No bullet collisions."),
            "cast_shadows": boolean("Cast shadows when lit (default true)."),
        }, [];
    tag_displacements: Edit, "Tag displacement triangles",
        "Paint the triangle tags of displacements (Hammer's Tag Walkable, Tag Buildable and Tag Remove): force \
        walkable or buildable on or off, give it back to the slope (auto), or mark triangles removed (on) or not \
        (off). Every triangle of the faces, or those whose center is within radius of center. One undo step.",
        {
            "faces": faces("Faces (default: the selected displacements)."), "ids": target_ids(), "solid_ids": target_solid_ids(),
            "tag": choice("The tag.", &["walkable", "buildable", "remove"]),
            "value": choice("What it becomes.", &["on", "off", "auto"]),
            "center": vec3("Only the triangles around this point (world)."),
            "radius": number("Radius around center (default 64).", 0.1, 16384.0),
            "spatial": boolean("Measure the distance in 3D (default false: in the face plane)."),
        }, ["tag", "value"];
    set_displacement_data: Edit, "Set displacement data",
        "Set per vertex data of one displacement: distances along the normals (heights) and alphas, row by row \
        ((2^power + 1) squared values each), the elevation, and the 4-way blend (multiblend) rows of \
        Lightmapped_4WayBlend materials: multiblend_weights (4 numbers per vertex, the amounts of textures 1 to \
        4), alphablend (4 per vertex) and multiblend_colors (4 lists of RGB per vertex). Missing multiblend lists \
        keep their values (a face without multiblend starts from its alphas: texture 2 where painted). \
        remove_multiblend drops the rows. Written to the VMF like Hammer++ and compiled to the multiblend lump.",
        {
            "face": faces("The face (one item)."),
            "distances": { "type": "array", "items": { "type": "number" }, "maxItems": 289, "description": "Heights per vertex." },
            "alphas": { "type": "array", "items": { "type": "number", "minimum": 0, "maximum": 255 }, "maxItems": 289, "description": "Alphas per vertex (0 to 255)." },
            "elevation": number("Elevation of the whole surface.", -16384.0, 16384.0),
            "multiblend_weights": { "type": "array", "items": { "type": "array", "items": { "type": "number", "minimum": 0, "maximum": 1 }, "minItems": 4, "maxItems": 4 }, "maxItems": 289, "description": "Amounts of textures 1 to 4 per vertex (0 to 1)." },
            "alphablend": { "type": "array", "items": { "type": "array", "items": { "type": "number", "minimum": 0, "maximum": 1 }, "minItems": 4, "maxItems": 4 }, "maxItems": 289, "description": "Alpha blend per vertex (4 numbers, 0 to 1)." },
            "multiblend_colors": { "type": "array", "items": { "type": "array", "items": { "type": "array", "items": { "type": "number", "minimum": 0, "maximum": 4 }, "minItems": 3, "maxItems": 3 }, "maxItems": 289 }, "minItems": 4, "maxItems": 4, "description": "Color of textures 1 to 4 per vertex (4 lists of RGB)." },
            "remove_multiblend": boolean("Drop the multiblend rows."),
        }, ["face"];
    subdivide_displacements: Edit, "Subdivide displacements",
        "Smooth the surface formed by displacements like a subdivision surface, then sew it (Hammer's Subdivide).",
        { "faces": faces("Faces (default: the selected displacements)."), "ids": target_ids(), "solid_ids": target_solid_ids() }, [];
    sew_displacements: Edit, "Sew displacements",
        "Make the shared edges and corners of neighbouring displacements identical (Hammer's Sew).",
        { "faces": faces("Faces (default: the selected displacements)."), "ids": target_ids(), "solid_ids": target_solid_ids() }, [];
    resample_displacements: Edit, "Resample displacement power",
        "Change the power (2, 3 or 4) of displacements, like the Power option of the displacement panel. With \
        keep_shape (default true) the shape is kept: offsets, distances, normals, alpha, 4-way blend rows and \
        triangle tags are resampled onto the new grid (interpolated going up, picked going down), flags and \
        allowed vertices are kept, and the displacements are sewn together afterwards. With keep_shape false they \
        are rebuilt flat.",
        {
            "faces": faces("Faces (default: the selected displacements)."),
            "ids": target_ids(),
            "solid_ids": target_solid_ids(),
            "power": integer("New power.", 2, 4),
            "keep_shape": boolean("Keep the shape (default true)."),
        }, ["power"];
}

fn is_disp(state: &AppState, f: FaceRef) -> bool {
    state.doc.face_side(f).is_some_and(|s| s.disp.is_some())
}

pub(super) fn disp_targets(state: &AppState, a: &Args<'_>, all_default: bool) -> R<Vec<FaceRef>> {
    let given = a.get("faces").is_some() || a.get("ids").is_some() || a.get("solid_ids").is_some();
    let faces = if given || !all_default {
        match target_faces(state, a, "faces") {
            Ok(f) => f,
            Err(_) if !given => Vec::new(),
            Err(e) => return Err(e),
        }
    } else {
        state.doc.selected_faces()
    };
    let mut disp: Vec<FaceRef> = faces.into_iter().filter(|f| is_disp(state, *f)).collect();
    if disp.is_empty() && !given {
        let solids = state.doc.selected_solid_ids();
        disp = state.doc.disp_faces().into_iter().filter(|f| solids.contains(&f.solid)).collect();
        if disp.is_empty() && all_default {
            disp = state.doc.disp_faces();
        }
    }
    if disp.is_empty() {
        return Err("no displacement among the faces (create_displacements makes some)".to_owned());
    }
    Ok(disp)
}

fn one_face(state: &AppState, a: &Args<'_>) -> R<FaceRef> {
    let f = opt_faces(&state.doc, a, "face")?.unwrap_or_default();
    let [f] = f.as_slice() else { return Err("face must be exactly one {solid, side}".to_owned()) };
    if !is_disp(state, *f) {
        return Err(format!("solid {} side {} is not a displacement", f.solid, f.side));
    }
    Ok(*f)
}

fn list_displacements(state: &mut AppState, a: &Args<'_>) -> R<ToolResult> {
    let only = opt_solids(&state.doc, a, "solid_ids")?;
    let faces: Vec<FaceRef> =
        state.doc.disp_faces().into_iter().filter(|f| only.as_ref().is_none_or(|o| o.contains(&f.solid))).collect();
    let (faces, total) = page(faces, a)?;
    let scene = state.doc.scene();
    let list: Vec<Value> = faces
        .into_iter()
        .filter_map(|f| {
            let node = scene.solid(f.solid)?;
            let d = node.solid.sides.get(f.side)?.disp.as_ref()?;
            let bounds = node.disp_mesh(f.side).map(|m| hp_geom::Aabb::from_points(m.positions.iter().copied()));
            Some(json!({
                "face": face_json(f),
                "power": d.power,
                "elevation": d.elevation,
                "no_physics": d.flags & hp_geom::DISP_FLAG_NO_PHYSICS != 0,
                "no_hull": d.flags & hp_geom::DISP_FLAG_NO_HULL != 0,
                "no_ray": d.flags & hp_geom::DISP_FLAG_NO_RAY != 0,
                "cast_shadows": d.casts_shadows(),
                "lightmap_scale": node.solid.sides.get(f.side).map(|s| s.lightmap_scale),
                "bounds": super::tools::bounds_json(bounds),
            }))
        })
        .collect();
    Ok(ToolResult::json(json!({ "total": total, "displacements": list })))
}

fn round(x: f64) -> f64 {
    (x * 1000.0).round() / 1000.0
}

fn select_displacements(state: &mut AppState, a: &Args<'_>) -> R<ToolResult> {
    let only = opt_solids(&state.doc, a, "solid_ids")?;
    let faces: Vec<FaceRef> =
        state.doc.disp_faces().into_iter().filter(|f| only.as_ref().is_none_or(|o| o.contains(&f.solid))).collect();
    let op = match a.opt_enum("op", &["replace", "add", "remove", "adjacent"])?.as_deref() {
        Some("add") => SelectOp::Add,
        Some("remove") => SelectOp::Remove,
        Some("adjacent") => {
            if only.is_some() {
                state.doc.select_faces(&faces, SelectOp::Add);
            }
            let added = hp_tools_mesh::DisplacementTool::new().select_adjacent(state);
            return Ok(ToolResult::json(json!({ "added": added, "selected_faces": state.doc.selected_faces().len() })));
        }
        _ => SelectOp::Replace,
    };
    if op == SelectOp::Replace {
        state.doc.clear_selection();
    }
    state.doc.select_faces(&faces, op);
    Ok(ToolResult::json(json!({ "faces": faces.len(), "selected_faces": state.doc.selected_faces().len() })))
}

fn get_displacement(state: &mut AppState, a: &Args<'_>) -> R<ToolResult> {
    let f = one_face(state, a)?;
    let with_positions = a.opt_bool("positions")?.unwrap_or(true);
    let node = state.doc.scene().solid(f.solid).ok_or("the solid is gone")?;
    let d = node.solid.sides[f.side].disp.as_ref().ok_or("not a displacement")?;
    let mut out = json!({
        "face": face_json(f),
        "power": d.power,
        "row_length": d.row_len(),
        "start_position": v3(d.start_position),
        "elevation": d.elevation,
        "flags": d.flags,
        "cast_shadows": d.casts_shadows(),
        "subdivided": d.subdiv,
        "triangle_tags": d.triangle_tags,
        "walkable_buildable": node.disp_mesh(f.side).map(|m| hp_geom::disp_tri_states(d, m).into_iter().map(|(w, b)| [w, b]).collect::<Vec<_>>()),
        "removed_triangles": d.triangle_tags.iter().filter(|t| *t & hp_geom::DISP_TAG_FORCE_REMOVE_BIT != 0).count(),
        "distances": d.distances.iter().map(|x| round(*x)).collect::<Vec<_>>(),
        "alphas": d.alphas.iter().map(|x| round(*x)).collect::<Vec<_>>(),
        "multiblend": hp_vmf::multiblend::MultiBlend::of(d).map(|mb| json!({
            "weights": mb.weights.iter().map(|w| w.map(round)).collect::<Vec<_>>(),
            "alphablend": mb.alphas.iter().map(|w| w.map(round)).collect::<Vec<_>>(),
            "colors": mb.colors.iter().map(|c| c.iter().map(|v| v.map(round)).collect::<Vec<_>>()).collect::<Vec<_>>(),
        })),
    });
    if with_positions && let Some(m) = node.disp_mesh(f.side) {
        out["positions"] = Value::Array(m.positions.iter().map(|p| v3(*p)).collect());
    }
    Ok(ToolResult::json(out))
}

fn create_displacements(state: &mut AppState, a: &Args<'_>) -> R<ToolResult> {
    let faces = target_faces(state, a, "faces")?;
    let power = a.opt_u64("power", 2, 4)?.unwrap_or(3) as u32;
    let n = edit(state, "create_displacements", |doc| doc.create_displacements(&faces, power))?;
    Ok(ToolResult::json(json!({ "created": n, "power": power })))
}

fn destroy_displacements(state: &mut AppState, a: &Args<'_>) -> R<ToolResult> {
    let faces = disp_targets(state, a, false)?;
    let n = edit(state, "destroy_displacements", |doc| doc.destroy_displacements(&faces))?;
    Ok(ToolResult::json(json!({ "destroyed": n })))
}

fn paint_displacement(state: &mut AppState, a: &Args<'_>) -> R<ToolResult> {
    let faces = disp_targets(state, a, true)?;
    let mode = match a.opt_enum("mode", &["raise", "lower", "smooth", "set", "alpha"])?.as_deref() {
        Some("raise") => PaintMode::Raise,
        Some("lower") => PaintMode::Lower,
        Some("smooth") => PaintMode::Smooth,
        Some("set") => PaintMode::Set,
        _ => PaintMode::AlphaPaint,
    };
    let radius = a.opt_f64("radius", 1.0, 16384.0)?.ok_or("radius is required")?;
    let amount = a.opt_f64("amount", -16384.0, 16384.0)?.ok_or("amount is required")?;
    match mode {
        PaintMode::Smooth if !(0.0..=1.0).contains(&amount) => {
            return Err("smooth takes an amount from 0 to 1".to_owned());
        }
        PaintMode::AlphaPaint if amount.abs() > 255.0 => {
            return Err("alpha takes an amount from -255 to 255".to_owned());
        }
        PaintMode::Raise | PaintMode::Lower if amount < 0.0 => {
            return Err("raise and lower take a positive amount".to_owned());
        }
        _ => {}
    }
    let falloff = match a.opt_enum("falloff", &["constant", "linear", "smooth"])?.as_deref() {
        Some("constant") => Falloff::Constant,
        Some("linear") => Falloff::Linear,
        _ => Falloff::Smooth,
    };
    let spatial = a.opt_bool("spatial")?.unwrap_or(false);
    let list = a.get("centers").and_then(Value::as_array).filter(|l| !l.is_empty() && l.len() <= MAX_STROKES);
    let list = list.ok_or_else(|| format!("centers must be 1 to {MAX_STROKES} [x, y, z] points"))?;
    let mut centers = Vec::with_capacity(list.len());
    for c in list {
        let xyz: Vec<f64> = c
            .as_array()
            .filter(|x| x.len() == 3)
            .into_iter()
            .flatten()
            .filter_map(Value::as_f64)
            .filter(|f| f.is_finite() && f.abs() <= 131072.0)
            .collect();
        if xyz.len() != 3 {
            return Err("centers must contain [x, y, z] points with numbers from -131072 to 131072".to_owned());
        }
        centers.push(DVec3::new(xyz[0], xyz[1], xyz[2]));
    }
    let samples = centers.len();
    let moved = edit(state, "paint_displacement", |doc| {
        let mut moved = 0;
        for center in centers {
            let brush = PaintBrush { center, radius, amount, mode, falloff, spatial };
            moved += doc.disp_paint(&faces, &brush)?;
        }
        Ok(moved)
    })?;
    Ok(ToolResult::json(json!({ "samples": samples, "faces": faces.len(), "vertices_changed": moved })))
}

fn stroke_of(state: &AppState, a: &Args<'_>) -> R<hp_app::tool::ScriptedStroke> {
    let list = a.get("points").and_then(Value::as_array).filter(|l| !l.is_empty() && l.len() <= MAX_STROKES);
    let list = list.ok_or_else(|| format!("points must be 1 to {MAX_STROKES} [x, y, z] points"))?;
    let mut points = Vec::with_capacity(list.len());
    for p in list {
        let xyz: Vec<f64> = p
            .as_array()
            .filter(|x| x.len() == 3)
            .into_iter()
            .flatten()
            .filter_map(Value::as_f64)
            .filter(|f| f.is_finite() && f.abs() <= 131_072.0)
            .collect();
        if xyz.len() != 3 {
            return Err("points must contain [x, y, z] points with numbers from -131072 to 131072".to_owned());
        }
        points.push(DVec3::new(xyz[0], xyz[1], xyz[2]));
    }
    let faces = opt_faces(&state.doc, a, "faces")?;
    if let Some(list) = &faces {
        if list.is_empty() {
            return Err("faces is empty".to_owned());
        }
        if let Some(f) = list.iter().find(|f| state.doc.face_side(**f).is_none_or(|s| s.disp.is_none())) {
            return Err(format!("solid {} side {} is not a displacement", f.solid, f.side));
        }
    }
    let cam = &state.cameras.cam3d;
    Ok(hp_app::tool::ScriptedStroke {
        points,
        invert: a.opt_bool("invert")?.unwrap_or(false),
        smooth: a.opt_bool("smooth")?.unwrap_or(false),
        seconds_per_sample: a.opt_f64("seconds_per_sample", 0.001, 10.0)?.unwrap_or(1.0 / 60.0) as f32,
        eye: Some(cam.pos),
        forward: Some(cam.forward()),
        faces,
        undo_label: super::tools::undo_label("stroke_displacement"),
    })
}

pub(crate) fn run_stroke(
    tools: &mut [Box<dyn hp_app::Tool>],
    state: &mut AppState,
    stroke: &hp_app::tool::ScriptedStroke,
) -> ToolResult {
    let Some(tool) = tools.iter_mut().find(|t| t.id() == hp_app::ToolId::Displacement) else {
        return ToolResult::error("stroke_displacement: the displacement tool is not available");
    };
    match tool.scripted_stroke(state, stroke) {
        None => ToolResult::error("stroke_displacement: the displacement tool has no brush"),
        Some(Err(e)) => ToolResult::error(format!("stroke_displacement: {e}")),
        Some(Ok(out)) => {
            let mut v = json!({
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

fn stroke_displacement(state: &mut AppState, a: &Args<'_>) -> R<Outcome> {
    let stroke = stroke_of(state, a)?;
    if state.doc.in_transaction() {
        return Err("Hammer Pro is in the middle of another edit (a drag in a view?); try again".to_owned());
    }
    Ok(Outcome::AppFn(AppFn(Box::new(move |app, _ctx| run_stroke(&mut app.tools, &mut app.state, &stroke)))))
}

fn displacement_noise(state: &mut AppState, a: &Args<'_>) -> R<ToolResult> {
    let faces = disp_targets(state, a, false)?;
    let min = a.opt_f64("min", -4096.0, 4096.0)?.ok_or("min is required")?;
    let max = a.opt_f64("max", -4096.0, 4096.0)?.ok_or("max is required")?;
    if min > max {
        return Err("min must not be above max".to_owned());
    }
    let seed = a.opt_u64("seed", 0, u64::from(u32::MAX))?.unwrap_or_else(|| state.doc.revision() ^ 0x5eed_0fd1);
    let n = edit(state, "displacement_noise", |doc| doc.disp_noise(&faces, min, max, seed))?;
    Ok(ToolResult::json(json!({ "faces": n, "seed": seed })))
}

fn terrain_displacements(state: &mut AppState, a: &Args<'_>) -> R<Outcome> {
    use hp_tools_mesh::disp::erode::{TerrainOp, TerrainOptions, apply};
    let (op, name) =
        match a.opt_enum("operation", &["fractal_noise", "thermal_erosion", "hydraulic_erosion"])?.as_deref() {
            Some("fractal_noise") => (TerrainOp::Noise, "fractal noise"),
            Some("thermal_erosion") => (TerrainOp::Thermal, "thermal erosion"),
            Some("hydraulic_erosion") => (TerrainOp::Hydraulic, "hydraulic erosion"),
            _ => return Err("give operation: fractal_noise, thermal_erosion or hydraulic_erosion".to_owned()),
        };
    let faces = disp_targets(state, a, false)?;
    let mut overrides: Vec<(String, String)> = Vec::new();
    if let Some(map) = a.get("options") {
        let map = map.as_object().ok_or("options must be an object: names to values")?;
        let probe = TerrainOptions::default().values();
        for (k, v) in map {
            if !probe.iter().any(|o| o.key == k) {
                let names: Vec<&str> = probe.iter().map(|o| o.key).collect();
                return Err(format!("\"{k}\" is not a terrain option (options: {})", names.join(", ")));
            }
            overrides.push((k.clone(), super::tools_toolopts::option_text(k, v)?));
        }
    }
    let min = a.opt_f64("min", -1024.0, 1024.0)?;
    let max = a.opt_f64("max", -1024.0, 1024.0)?;
    if state.doc.in_transaction() {
        return Err("Hammer Pro is in the middle of another edit (a drag in a view?); try again".to_owned());
    }
    Ok(Outcome::AppFn(AppFn(Box::new(move |app, _ctx| {
        let base = app
            .tools
            .iter()
            .find(|t| t.id() == hp_app::ToolId::Displacement)
            .map(|t| TerrainOptions::from_values(&t.option_values()))
            .unwrap_or_default();
        let mut opts = base;
        for (k, v) in &overrides {
            if !opts.set(k, v) {
                return ToolResult::error(format!("terrain_displacements: {k} cannot be \"{v}\""));
            }
        }
        let kept = (app.state.settings.mesh.noise_min, app.state.settings.mesh.noise_max);
        if min.is_some() || max.is_some() {
            app.state.settings.mesh.noise_min = min.unwrap_or(kept.0);
            app.state.settings.mesh.noise_max = max.unwrap_or(kept.1);
        }
        let moved = with_faces_selected(&mut app.state, "terrain_displacements", &faces, |s| {
            if op == TerrainOp::Noise {
                opts.noise_kind = hp_tools_mesh::disp::erode::NoiseKind::Fractal;
            }
            apply(s, op, &opts)
        });
        (app.state.settings.mesh.noise_min, app.state.settings.mesh.noise_max) = kept;
        match moved {
            Ok(n) => ToolResult::json(json!({ "operation": name, "faces": faces.len(), "vertices_moved": n })),
            Err(e) => ToolResult::error(format!("terrain_displacements: {e}")),
        }
    }))))
}

fn scale_displacements(state: &mut AppState, a: &Args<'_>) -> R<ToolResult> {
    let faces = disp_targets(state, a, false)?;
    let factor = a.opt_f64("factor", -100.0, 100.0)?.ok_or("factor is required")?;
    let n = edit(state, "scale_displacements", |doc| doc.scale_disps(&faces, factor))?;
    Ok(ToolResult::json(json!({ "faces": n })))
}

fn invert_displacement_alpha(state: &mut AppState, a: &Args<'_>) -> R<ToolResult> {
    let faces = disp_targets(state, a, false)?;
    let n = edit(state, "invert_displacement_alpha", |doc| doc.invert_disp_alpha(&faces))?;
    Ok(ToolResult::json(json!({ "faces": n })))
}

fn set_displacement_flags(state: &mut AppState, a: &Args<'_>) -> R<ToolResult> {
    let faces = disp_targets(state, a, false)?;
    let changes = [
        (hp_geom::DISP_FLAG_NO_PHYSICS, a.opt_bool("no_physics")?),
        (hp_geom::DISP_FLAG_NO_HULL, a.opt_bool("no_hull")?),
        (hp_geom::DISP_FLAG_NO_RAY, a.opt_bool("no_ray")?),
    ];
    let shadows = a.opt_bool("cast_shadows")?;
    if changes.iter().all(|(_, v)| v.is_none()) && shadows.is_none() {
        return Err("give no_physics, no_hull, no_ray or cast_shadows".to_owned());
    }
    let label = super::tools::undo_label("set_displacement_flags");
    let n = edit(state, "set_displacement_flags", |doc| {
        let mut n = 0;
        for (mask, on) in changes {
            if let Some(on) = on {
                n = n.max(doc.set_disp_flags(&faces, mask, on)?);
            }
        }
        if let Some(on) = shadows {
            n = n.max(hp_tools_mesh::disp::commands::set_cast_shadows_of(doc, &faces, on, &label)?);
        }
        Ok(n)
    })?;
    Ok(ToolResult::json(json!({ "faces": n })))
}

fn tag_displacements(state: &mut AppState, a: &Args<'_>) -> R<ToolResult> {
    use hp_tools_mesh::disp::tags::{TagKind, TagValue, tag_all, tag_brush};
    let faces = disp_targets(state, a, false)?;
    let kind = match a.opt_enum("tag", &["walkable", "buildable", "remove"])?.as_deref() {
        Some("walkable") => TagKind::Walkable,
        Some("buildable") => TagKind::Buildable,
        Some("remove") => TagKind::Remove,
        _ => return Err("give tag: walkable, buildable or remove".to_owned()),
    };
    let value = match a.opt_enum("value", &["on", "off", "auto"])?.as_deref() {
        Some("on") => TagValue::On,
        Some("off") => TagValue::Off,
        Some("auto") => TagValue::Auto,
        _ => return Err("give value: on, off or auto".to_owned()),
    };
    let center = a.opt_vec3("center")?;
    let radius = a.opt_f64("radius", 0.1, 16384.0)?.unwrap_or(64.0);
    let spatial = a.opt_bool("spatial")?.unwrap_or(false);
    let label = super::tools::undo_label("tag_displacements");
    let n = edit(state, "tag_displacements", |doc| match center {
        Some(c) => tag_brush(doc, &faces, c, radius, spatial, kind, value, &label),
        None => tag_all(doc, &faces, kind, value, &label),
    })?;
    Ok(ToolResult::json(json!({ "faces": faces.len(), "triangles_changed": n })))
}

fn set_displacement_data(state: &mut AppState, a: &Args<'_>) -> R<ToolResult> {
    let f = one_face(state, a)?;
    let mut d = state.doc.face_side(f).and_then(|s| s.disp.as_deref().cloned()).ok_or("not a displacement")?;
    let count = d.row_len() * d.row_len();
    let numbers = |key: &str, lo: f64, hi: f64| -> R<Option<Vec<f64>>> {
        let Some(v) = a.get(key) else { return Ok(None) };
        let list = v.as_array().filter(|l| l.len() == count);
        let list = list.ok_or_else(|| format!("{key} must have {count} numbers (power {})", d.power))?;
        list.iter()
            .map(|x| x.as_f64().filter(|x| x.is_finite() && (lo..=hi).contains(x)))
            .collect::<Option<Vec<f64>>>()
            .map(Some)
            .ok_or_else(|| format!("{key} must contain numbers from {lo} to {hi}"))
    };
    let tuples = |v: &Value, key: &str, per: usize, hi: f64| -> R<Vec<Vec<f64>>> {
        let list = v.as_array().filter(|l| l.len() == count);
        let list = list.ok_or_else(|| format!("{key} must have {count} items (power {})", d.power))?;
        list.iter()
            .map(|t| {
                t.as_array()
                    .filter(|t| t.len() == per)?
                    .iter()
                    .map(|x| x.as_f64().filter(|x| x.is_finite() && (0.0..=hi).contains(x)))
                    .collect::<Option<Vec<f64>>>()
            })
            .collect::<Option<Vec<Vec<f64>>>>()
            .ok_or_else(|| format!("{key} must hold {per} numbers from 0 to {hi} per vertex"))
    };
    let distances = numbers("distances", -16384.0, 16384.0)?;
    let alphas = numbers("alphas", 0.0, 255.0)?;
    let elevation = a.opt_f64("elevation", -16384.0, 16384.0)?;
    let weights = a.get("multiblend_weights").map(|v| tuples(v, "multiblend_weights", 4, 1.0)).transpose()?;
    let alphablend = a.get("alphablend").map(|v| tuples(v, "alphablend", 4, 1.0)).transpose()?;
    let colors = match a.get("multiblend_colors") {
        None => None,
        Some(v) => {
            let lists = v.as_array().filter(|l| l.len() == hp_vmf::multiblend::CHANNELS);
            let lists = lists.ok_or("multiblend_colors must hold 4 lists, one per texture")?;
            Some(lists.iter().map(|l| tuples(l, "multiblend_colors", 3, 4.0)).collect::<R<Vec<_>>>()?)
        }
    };
    let remove = a.opt_bool("remove_multiblend")?.unwrap_or(false);
    let blend_given = weights.is_some() || alphablend.is_some() || colors.is_some();
    if remove && blend_given {
        return Err("remove_multiblend cannot be combined with multiblend data".to_owned());
    }
    if distances.is_none() && alphas.is_none() && elevation.is_none() && !blend_given && !remove {
        return Err("give distances, alphas, elevation, multiblend data or remove_multiblend".to_owned());
    }
    if remove && !hp_vmf::multiblend::remove_multiblend(&mut d) {
        return Err("the displacement has no multiblend rows".to_owned());
    }
    if let Some(x) = distances {
        d.distances = x;
        if d.normals.len() != count {
            let n = state.doc.face_side(f).and_then(hp_geom::Plane::from_side).map_or(DVec3::Z, |p| p.normal);
            d.normals = vec![n; count];
        }
    }
    if let Some(x) = alphas {
        d.alphas = x;
    }
    if blend_given {
        let mut mb = hp_vmf::multiblend::MultiBlend::of_or_alphas(&d)
            .filter(|m| m.len() == count)
            .unwrap_or_else(|| hp_vmf::multiblend::MultiBlend::new(count));
        for (slot, w) in mb.weights.iter_mut().zip(weights.iter().flatten()) {
            *slot = std::array::from_fn(|k| w[k]);
        }
        for (slot, w) in mb.alphas.iter_mut().zip(alphablend.iter().flatten()) {
            *slot = std::array::from_fn(|k| w[k]);
        }
        for (channel, list) in mb.colors.iter_mut().zip(colors.iter().flatten()) {
            for (slot, c) in channel.iter_mut().zip(list) {
                *slot = std::array::from_fn(|k| c[k]);
            }
        }
        if !mb.write_to(&mut d) {
            return Err("the displacement grid does not match its power".to_owned());
        }
    }
    if let Some(e) = elevation {
        d.elevation = e;
    }
    let n = edit(state, "set_displacement_data", |doc| {
        doc.set_disp_data(&super::tools::undo_label("set_displacement_data"), vec![(f, d)])
    })?;
    Ok(ToolResult::json(json!({ "faces": n })))
}

pub(super) fn with_faces_selected<T>(
    state: &mut AppState,
    tool: &str,
    faces: &[FaceRef],
    f: impl FnOnce(&mut AppState) -> T,
) -> R<T> {
    if state.doc.in_transaction() {
        return Err("Hammer Pro is in the middle of another edit; try again".to_owned());
    }
    let objs = state.doc.selected_objects();
    let old_faces = state.doc.selected_faces();
    let record = state.doc.records_selection();
    state.doc.set_record_selection(false);
    state.doc.clear_selection();
    state.doc.select_faces(faces, SelectOp::Replace);
    state.doc.begin(super::tools::undo_label(tool));
    let out = f(state);
    state.doc.commit();
    state.doc.clear_selection();
    state.doc.select(&objs, SelectOp::Replace);
    state.doc.select_faces(&old_faces, SelectOp::Add);
    state.doc.set_record_selection(record);
    Ok(out)
}

fn mesh_path(state: &AppState, a: &Args<'_>) -> R<std::path::PathBuf> {
    let Some(given) = a.opt_str("path", 1024)? else {
        let stem = state.doc.path().and_then(|p| p.file_stem()).map(|s| s.to_string_lossy().into_owned());
        let stem = stem.ok_or_else(|| "save the map first or give a path".to_owned())?;
        return Ok(super::semantic::map_dir(state)?.join(format!("{stem}_displacements.obj")));
    };
    let path = std::path::Path::new(given);
    if path.is_absolute() {
        let named = path.file_name().is_some_and(|n| !n.is_empty());
        if !named || path.components().any(|c| matches!(c, std::path::Component::ParentDir)) {
            return Err(format!("invalid path \"{given}\""));
        }
        return Ok(if path.extension().is_none() { path.with_extension("obj") } else { path.to_path_buf() });
    }
    let rel = super::semantic::relative_file(given, "obj")?;
    Ok(super::semantic::map_dir(state)?.join(rel))
}

fn displacement_to_mesh(state: &mut AppState, a: &Args<'_>) -> R<Outcome> {
    let faces = disp_targets(state, a, true)?;
    let path = mesh_path(state, a)?;
    if !path.extension().is_some_and(|e| e.eq_ignore_ascii_case("obj")) {
        return Err("path must end with .obj".to_owned());
    }
    let overwrite = a.opt_bool("overwrite")?.unwrap_or(false);
    if !overwrite && (path.exists() || path.with_extension("mtl").exists()) {
        return Err(format!("{} (or its .mtl) exists already (overwrite: true replaces it)", path.display()));
    }
    let mut layout = crate::export::ObjLayout::from_settings(&state.settings.mesh);
    match a.opt_str("units", 16)? {
        Some("source") => layout.meters = false,
        Some("meters") => layout.meters = true,
        Some(other) => return Err(format!("units must be source or meters, not {other}")),
        None => {}
    }
    match a.opt_str("up", 16)? {
        Some("y") => layout.z_up = false,
        Some("z") => layout.z_up = true,
        Some(other) => return Err(format!("up must be y or z, not {other}")),
        None => {}
    }
    let input = crate::export::ObjInput::from_displacements(state, &faces, layout);
    Ok(Worker::spawn("displacement_to_mesh", move || {
        if let Some(dir) = path.parent()
            && let Err(e) = std::fs::create_dir_all(dir)
        {
            return ToolResult::error(format!("displacement_to_mesh: cannot create the folder: {e}"));
        }
        match crate::export::export_obj(&input, &path) {
            Ok(stats) => ToolResult::json(json!({
                "written": path.display().to_string(),
                "displacements": stats.faces,
                "triangles": stats.triangles,
                "units": if layout.meters { "meters" } else { "source" },
                "up": if layout.z_up { "z" } else { "y" },
            })),
            Err(e) => ToolResult::error(format!("displacement_to_mesh: {e}")),
        }
    }))
}

fn subdivide_displacements(state: &mut AppState, a: &Args<'_>) -> R<ToolResult> {
    let faces = disp_targets(state, a, false)?;
    let n = with_faces_selected(state, "subdivide_displacements", &faces, hp_tools_mesh::disp::commands::subdivide)?;
    Ok(ToolResult::json(json!({ "faces": faces.len(), "vertices_moved": n })))
}

fn sew_displacements(state: &mut AppState, a: &Args<'_>) -> R<ToolResult> {
    let faces = disp_targets(state, a, false)?;
    if faces.len() < 2 {
        return Err("sewing needs at least two displacements".to_owned());
    }
    let n = with_faces_selected(state, "sew_displacements", &faces, |s| hp_tools_mesh::DisplacementTool::new().sew(s))?;
    Ok(ToolResult::json(json!({ "faces": faces.len(), "vertices_moved": n })))
}

fn resample_displacements(state: &mut AppState, a: &Args<'_>) -> R<ToolResult> {
    let faces = disp_targets(state, a, false)?;
    let power = a.opt_u64("power", 2, 4)?.ok_or_else(|| "power is required".to_owned())? as u32;
    let keep = a.opt_bool("keep_shape")?.unwrap_or(true);
    let n = with_faces_selected(state, "resample_displacements", &faces, |s| {
        hp_tools_mesh::DisplacementTool::new().set_power_with(s, power, keep)
    })?;
    Ok(ToolResult::json(json!({ "resampled": n, "power": power, "keep_shape": keep })))
}

#[cfg(test)]
mod tests {
    use serde_json::json;

    use super::super::semantic_tests::{err, ok};

    #[test]
    fn resample_keeps_the_shape_through_mcp() {
        let mut s = hp_app::testing::test_state();
        let top = json!([{ "solid": 2, "side": 0 }]);
        ok(&mut s, "create_displacements", json!({ "faces": top, "power": 2 }));
        let heights = vec![6.0; 25];
        ok(&mut s, "set_displacement_data", json!({ "face": top, "distances": heights }));
        let r = ok(&mut s, "resample_displacements", json!({ "faces": top, "power": 3 }));
        assert_eq!(r["resampled"], 1);
        let d = ok(&mut s, "get_displacement", json!({ "face": top, "positions": false }));
        assert_eq!(d["power"], 3);
        assert_eq!(d["distances"].as_array().map(Vec::len), Some(81));
        assert_eq!(d["distances"][40], 6.0);
        ok(&mut s, "resample_displacements", json!({ "faces": top, "power": 2, "keep_shape": false }));
        let d = ok(&mut s, "get_displacement", json!({ "face": top, "positions": false }));
        assert_eq!(d["distances"][12], 0.0);
        err(&mut s, "resample_displacements", json!({ "faces": top }));
    }

    #[test]
    fn multiblend_rows_through_mcp() {
        let mut s = hp_app::testing::test_state();
        let top = json!([{ "solid": 2, "side": 0 }]);
        ok(&mut s, "create_displacements", json!({ "faces": top, "power": 2 }));
        assert_eq!(ok(&mut s, "get_displacement", json!({ "face": top }))["multiblend"], json!(null));
        let mut alphas = vec![0.0; 25];
        alphas[1] = 255.0;
        let mut weights = vec![json!([1, 0, 0, 0]); 25];
        weights[2] = json!([0, 0, 0.5, 0.5]);
        ok(&mut s, "set_displacement_data", json!({ "face": top, "alphas": alphas }));
        ok(&mut s, "set_displacement_data", json!({ "face": top, "multiblend_weights": weights }));
        let d = ok(&mut s, "get_displacement", json!({ "face": top }));
        assert_eq!(d["multiblend"]["weights"][2], json!([0.0, 0.0, 0.5, 0.5]));
        assert_eq!(d["multiblend"]["colors"].as_array().map(Vec::len), Some(4));
        let colors: Vec<_> = (0..4).map(|k| vec![json!([k, 1, 1]); 25]).collect();
        ok(&mut s, "set_displacement_data", json!({ "face": top, "multiblend_colors": colors }));
        let d = ok(&mut s, "get_displacement", json!({ "face": top }));
        assert_eq!(d["multiblend"]["colors"][3][0], json!([3.0, 1.0, 1.0]));
        assert_eq!(d["multiblend"]["weights"][2], json!([0.0, 0.0, 0.5, 0.5]), "kept");
        assert!(s.doc.to_vmf_string().contains("multiblend_color_3"));
        err(&mut s, "set_displacement_data", json!({ "face": top, "multiblend_weights": [[1, 0, 0, 0]] }));
        err(
            &mut s,
            "set_displacement_data",
            json!({ "face": top, "multiblend_weights": vec![json!([2, 0, 0, 0]); 25] }),
        );
        err(
            &mut s,
            "set_displacement_data",
            json!({ "face": top, "remove_multiblend": true, "alphablend": vec![json!([0, 0, 0, 0]); 25] }),
        );
        ok(&mut s, "set_displacement_data", json!({ "face": top, "remove_multiblend": true }));
        assert_eq!(ok(&mut s, "get_displacement", json!({ "face": top }))["multiblend"], json!(null));
        err(&mut s, "set_displacement_data", json!({ "face": top, "remove_multiblend": true }));
        ok(&mut s, "set_displacement_data", json!({ "face": top, "alphablend": vec![json!([0, 0, 0, 1]); 25] }));
        let d = ok(&mut s, "get_displacement", json!({ "face": top }));
        assert_eq!(d["multiblend"]["weights"][1], json!([0.0, 1.0, 0.0, 0.0]), "seeded from the alpha");
    }

    #[test]
    fn displacement_tools() {
        let mut s = hp_app::testing::test_state();
        let top = json!([{ "solid": 2, "side": 0 }]);
        let r = ok(&mut s, "create_displacements", json!({ "faces": top, "power": 2 }));
        assert_eq!(r["created"], 1);
        let l = ok(&mut s, "list_displacements", json!({}));
        assert_eq!(l["total"], 1);
        let r = ok(&mut s, "select_displacements", json!({}));
        assert_eq!(r["selected_faces"], 1);
        let d = ok(&mut s, "get_displacement", json!({ "face": top }));
        assert_eq!(d["row_length"], 5);
        assert_eq!(d["positions"].as_array().unwrap().len(), 25);
        let r = ok(
            &mut s,
            "paint_displacement",
            json!({ "mode": "raise", "centers": [[0, 0, 0], [16, 0, 0]], "radius": 128, "amount": 32, "falloff": "constant" }),
        );
        assert!(r["vertices_changed"].as_u64().unwrap() > 0);
        assert_eq!(s.doc.undo_label(), Some("MCP: paint_displacement"));
        err(
            &mut s,
            "paint_displacement",
            json!({ "mode": "smooth", "centers": [[0, 0, 0]], "radius": 10, "amount": 4 }),
        );
        ok(&mut s, "displacement_noise", json!({ "faces": top, "min": -4, "max": 4, "seed": 7 }));
        err(&mut s, "displacement_noise", json!({ "faces": top, "min": 4, "max": -4 }));
        ok(&mut s, "scale_displacements", json!({ "faces": top, "factor": 0.5 }));
        ok(&mut s, "invert_displacement_alpha", json!({ "faces": top }));
        ok(&mut s, "set_displacement_flags", json!({ "faces": top, "no_physics": true }));
        let l = ok(&mut s, "list_displacements", json!({}));
        assert_eq!(l["displacements"][0]["no_physics"], true);
        assert_eq!(l["displacements"][0]["cast_shadows"], true);
        ok(&mut s, "set_displacement_flags", json!({ "faces": top, "cast_shadows": false }));
        assert_eq!(ok(&mut s, "list_displacements", json!({}))["displacements"][0]["cast_shadows"], false);
        assert!(s.doc.to_vmf_string().contains("\"hp_cast_shadows\" \"0\""));
        let r = ok(
            &mut s,
            "tag_displacements",
            json!({ "faces": top, "tag": "walkable", "value": "off", "center": [0, 0, 0], "radius": 40 }),
        );
        let some = r["triangles_changed"].as_u64().unwrap();
        assert!(some > 0 && some < 32, "{some}");
        assert_eq!(s.doc.undo_label(), Some("MCP: tag_displacements"));
        let r = ok(&mut s, "tag_displacements", json!({ "faces": top, "tag": "remove", "value": "on" }));
        assert_eq!(r["triangles_changed"], 32);
        let d = ok(&mut s, "get_displacement", json!({ "face": top, "positions": false }));
        assert_eq!(d["removed_triangles"], 32);
        assert_eq!(d["cast_shadows"], false);
        let walk = d["walkable_buildable"].as_array().unwrap();
        assert_eq!(walk.iter().filter(|w| w[0] == false).count() as u64, some);
        err(&mut s, "tag_displacements", json!({ "faces": top, "tag": "lava", "value": "on" }));
        assert_eq!(ok(&mut s, "select_displacements", json!({ "op": "adjacent" }))["added"], 0);
        let heights: Vec<f64> = (0..25).map(f64::from).collect();
        ok(&mut s, "set_displacement_data", json!({ "face": top, "distances": heights, "elevation": 2 }));
        let d = ok(&mut s, "get_displacement", json!({ "face": top, "positions": false }));
        assert_eq!(d["distances"][24], 24.0);
        assert_eq!(d["elevation"], 2.0);
        err(&mut s, "set_displacement_data", json!({ "face": top, "alphas": [1, 2] }));
        err(&mut s, "get_displacement", json!({ "face": [{ "solid": 3, "side": 0 }] }));
        ok(&mut s, "subdivide_displacements", json!({ "faces": top }));
        err(&mut s, "sew_displacements", json!({ "faces": top }));
        ok(&mut s, "destroy_displacements", json!({ "faces": top }));
        assert_eq!(ok(&mut s, "list_displacements", json!({}))["total"], 0);
        err(&mut s, "destroy_displacements", json!({ "faces": top }));
    }

    fn positions(s: &hp_app::AppState, solid: u32, side: usize) -> Vec<glam::DVec3> {
        s.doc.scene().solid(solid).and_then(|n| n.disp_mesh(side)).map(|m| m.positions.clone()).expect("displacement")
    }

    fn two_tiles(s: &mut hp_app::AppState) -> (u32, usize) {
        ok(s, "create_displacements", json!({ "faces": [{ "solid": 2, "side": 0 }], "power": 2 }));
        let b = ok(s, "create_brush", json!({ "min": [256, -256, -16], "max": [768, 256, 0] }));
        let id = b["solid_ids"][0].as_u64().unwrap() as u32;
        let node = s.doc.scene().solid(id).unwrap();
        let side = (0..node.solid.sides.len())
            .find(|i| s.doc.face_side(hp_editor::FaceRef::new(id, *i)).is_some_and(|f| f.normal().z > 0.9))
            .expect("top side");
        ok(s, "create_displacements", json!({ "faces": [{ "solid": id, "side": side }], "power": 2 }));
        (id, side)
    }

    #[test]
    fn displacement_to_mesh_writes_an_obj() {
        let dir = std::env::temp_dir().join(format!("hp_disp_mesh_{}", std::process::id()));
        std::fs::create_dir_all(&dir).unwrap();
        let mut s = hp_app::testing::test_state();
        s.doc.set_path(Some(dir.join("main.vmf")));
        err(&mut s, "displacement_to_mesh", json!({}));
        let top = json!([{ "solid": 2, "side": 0 }]);
        ok(&mut s, "create_displacements", json!({ "faces": top, "power": 2 }));
        let r = ok(&mut s, "displacement_to_mesh", json!({ "faces": top }));
        assert_eq!(r["displacements"], 1);
        assert_eq!(r["triangles"], 32);
        let obj = std::fs::read_to_string(dir.join("main_displacements.obj")).unwrap();
        assert!(obj.contains("Source units, Y up") && obj.lines().filter(|l| l.starts_with("v ")).count() == 25);
        assert!(dir.join("main_displacements.mtl").is_file());
        err(&mut s, "displacement_to_mesh", json!({ "faces": top }));
        let r = ok(&mut s, "displacement_to_mesh", json!({ "path": "out/m", "units": "meters", "up": "z" }));
        assert_eq!(r["up"], "z");
        let obj = std::fs::read_to_string(dir.join("out/m.obj")).unwrap();
        assert!(obj.contains("meters, Z up"));
        err(&mut s, "displacement_to_mesh", json!({ "faces": top, "path": "../x", "overwrite": true }));
        err(&mut s, "displacement_to_mesh", json!({ "faces": top, "units": "feet", "overwrite": true }));
        let _ = std::fs::remove_dir_all(&dir);
    }

    #[test]
    fn brush_strokes_like_the_tool() {
        use super::super::tools_toolopts::set_tool_options_on;
        let mut s = hp_app::testing::test_state();
        let mut tools = hp_tools_mesh::tools();
        let opts = |list: &[(&str, &str)]| -> Vec<(String, String)> {
            list.iter().map(|(k, v)| ((*k).to_owned(), (*v).to_owned())).collect()
        };
        err(&mut s, "stroke_displacement", json!({ "points": [] }));
        err(&mut s, "stroke_displacement", json!({ "points": [[0, 0]] }));
        err(&mut s, "stroke_displacement", json!({ "points": [[0, 0, 0]], "faces": [{ "solid": 2, "side": 0 }] }));
        assert!(super::super::semantic_tests::needs_app(
            &mut s,
            "stroke_displacement",
            json!({ "points": [[0, 0, 0]] })
        ));
        let stroke = |s: &hp_app::AppState, points: Vec<[f64; 3]>, extra: serde_json::Value| {
            let mut args = json!({ "points": points });
            for (k, v) in extra.as_object().unwrap() {
                args[k] = v.clone();
            }
            let spec = super::SPECS.iter().find(|t| t.name == "stroke_displacement").unwrap();
            super::stroke_of(s, &super::super::semantic::checked(spec, &args).unwrap()).unwrap()
        };
        let st = stroke(&s, vec![[0.0, 0.0, 0.0]], json!({}));
        let r = super::run_stroke(&mut tools, &mut s, &st);
        assert!(r.is_error && r.first_text().contains("no displacement"), "{}", r.first_text());
        let (east, east_side) = two_tiles(&mut s);

        let r = set_tool_options_on(
            &mut tools,
            &mut s,
            hp_app::ToolId::Displacement,
            &opts(&[
                ("mode", "geometry"),
                ("effect", "raise"),
                ("axis", "view"),
                ("strength", "600"),
                ("radius", "64"),
                ("falloff", "constant"),
            ]),
        );
        assert!(!r.is_error, "{}", r.first_text());
        s.cameras.cam3d.pos = glam::DVec3::new(-900.0, -300.0, 500.0);
        s.cameras.cam3d.yaw_deg = 20.0;
        s.cameras.cam3d.pitch_deg = -30.0;
        let toward_eye = -s.cameras.cam3d.forward().normalize();
        let before = positions(&s, 2, 0);
        let history = s.doc.history_len();
        let st = stroke(&s, vec![[-128.0, 0.0, 0.0]], json!({ "seconds_per_sample": 0.1 }));
        let r = super::run_stroke(&mut tools, &mut s, &st);
        assert!(!r.is_error, "{}", r.first_text());
        let v = r.structured.clone().unwrap();
        assert!(v["vertices_changed"].as_u64().unwrap() > 0, "{v}");
        assert_eq!(s.doc.history_len().0, history.0 + 1);
        assert_eq!(s.doc.undo_label(), Some("MCP: stroke_displacement"));
        let after = positions(&s, 2, 0);
        let moved: Vec<glam::DVec3> =
            before.iter().zip(&after).map(|(a, b)| *b - *a).filter(|d| d.length() > 1e-6).collect();
        assert!(!moved.is_empty());
        for d in &moved {
            assert!((d.length() - 60.0).abs() < 1e-3, "{d}");
            assert!(d.normalize().dot(toward_eye) > 0.999, "{d} {toward_eye}");
        }
        let st = stroke(&s, vec![[-128.0, 0.0, 0.0]], json!({ "invert": true, "seconds_per_sample": 0.1 }));
        let r = super::run_stroke(&mut tools, &mut s, &st);
        assert!(!r.is_error);
        let back = positions(&s, 2, 0);
        assert!(before.iter().zip(&back).all(|(a, b)| a.distance(*b) < 1e-6));

        let r = set_tool_options_on(&mut tools, &mut s, hp_app::ToolId::Displacement, &opts(&[("axis", "view_flat")]));
        assert!(!r.is_error, "{}", r.first_text());
        let listed = super::super::tools_toolopts::tool_options_json(
            &tools,
            hp_app::ToolId::Displacement,
            Some(hp_app::ToolId::Displacement),
        )
        .to_string();
        assert!(listed.contains("view_flat"), "{listed}");
        for (yaw, pitch, flat) in
            [(20.0f64, -30.0, None), (90.0, -90.0, Some(glam::DVec3::NEG_Y)), (0.0, -89.0, Some(glam::DVec3::NEG_X))]
        {
            s.cameras.cam3d.yaw_deg = yaw;
            s.cameras.cam3d.pitch_deg = pitch;
            let f = s.cameras.cam3d.forward();
            let flat = flat.unwrap_or_else(|| glam::DVec3::new(-f.x, -f.y, 0.0).normalize());
            let before = positions(&s, 2, 0);
            let st = stroke(&s, vec![[-128.0, 0.0, 0.0]], json!({ "seconds_per_sample": 0.1 }));
            let r = super::run_stroke(&mut tools, &mut s, &st);
            assert!(!r.is_error, "{}", r.first_text());
            let after = positions(&s, 2, 0);
            let moved: Vec<glam::DVec3> =
                before.iter().zip(&after).map(|(a, b)| *b - *a).filter(|d| d.length() > 1e-6).collect();
            assert!(!moved.is_empty(), "pitch {pitch}");
            for d in &moved {
                assert!(d.z.abs() < 1e-6, "pitch {pitch}: {d} goes up or down");
                assert!(d.normalize().dot(flat) > 0.999, "pitch {pitch}: {d} vs {flat}");
            }
            ok(&mut s, "undo", json!({}));
        }
        s.cameras.cam3d.yaw_deg = 20.0;
        s.cameras.cam3d.pitch_deg = -30.0;

        set_tool_options_on(
            &mut tools,
            &mut s,
            hp_app::ToolId::Displacement,
            &opts(&[("axis", "z"), ("autosew", "on"), ("radius", "96")]),
        );
        let east_before = positions(&s, east, east_side);
        let st = stroke(&s, vec![[250.0, 0.0, 0.0]], json!({ "seconds_per_sample": 0.05 }));
        let r = super::run_stroke(&mut tools, &mut s, &st);
        let v = r.structured.clone().unwrap();
        assert_eq!(v["faces"].as_array().unwrap().len(), 2, "{v}");
        let floor = positions(&s, 2, 0);
        let east_now = positions(&s, east, east_side);
        assert!(east_before.iter().zip(&east_now).any(|(a, b)| a.distance(*b) > 1e-3));
        for p in floor.iter().filter(|p| (p.x - 256.0).abs() < 1e-3) {
            assert!(east_now.iter().any(|q| q.distance(*p) < 1e-3), "crack at {p}");
        }

        ok(&mut s, "select_faces", json!({ "faces": [{ "solid": 2, "side": 0 }] }));
        let east_before = positions(&s, east, east_side);
        let floor_before = positions(&s, 2, 0);
        let st = stroke(&s, vec![[200.0, 0.0, 0.0]], json!({ "seconds_per_sample": 0.05 }));
        let r = super::run_stroke(&mut tools, &mut s, &st);
        assert!(!r.is_error);
        assert_eq!(positions(&s, east, east_side), east_before);
        let floor = positions(&s, 2, 0);
        assert_ne!(floor, floor_before);
        for (p, q) in floor_before.iter().zip(&floor).filter(|(p, _)| (p.x - 256.0).abs() < 1e-3) {
            assert!(p.distance(*q) < 1e-6, "{p} moved to {q}");
        }
        let st = stroke(&s, vec![[600.0, 0.0, 0.0]], json!({}));
        let r = super::run_stroke(&mut tools, &mut s, &st);
        let v = r.structured.clone().unwrap();
        assert_eq!(v["vertices_changed"], 0);
        assert!(v["note"].as_str().unwrap().contains("limited"), "{v}");
        let east_face = json!([{ "solid": east, "side": east_side }]);
        let st = stroke(&s, vec![[600.0, 0.0, 0.0]], json!({ "faces": east_face }));
        let r = super::run_stroke(&mut tools, &mut s, &st);
        assert!(r.structured.clone().unwrap()["vertices_changed"].as_u64().unwrap() > 0);

        set_tool_options_on(&mut tools, &mut s, hp_app::ToolId::Displacement, &opts(&[("mode", "select")]));
        let st = stroke(&s, vec![[0.0, 0.0, 0.0]], json!({}));
        let r = super::run_stroke(&mut tools, &mut s, &st);
        assert!(r.is_error && r.first_text().contains("select mode"));
        assert!(!s.doc.in_transaction());
    }
}
