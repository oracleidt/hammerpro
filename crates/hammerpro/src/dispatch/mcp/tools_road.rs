use glam::DVec2;
use hp_app::AppState;
use hp_editor::{CurveRecord, EditError};
use hp_mcp::protocol::ToolResult;
use hp_tools_create::curve::apply::Target;
use hp_tools_create::road::apply::{self, ApplyError, RoadCreated, SplitAt};
use hp_tools_create::road::model::{
    MAX_DASH, MAX_FLARE, MAX_LANE_WIDTH, MAX_LANES, MAX_LIFT, MAX_LINE_WIDTH, MAX_POINTS, MAX_SHOULDER, MAX_STEP,
    MAX_THICKNESS, MIN_DASH, MIN_LANE_WIDTH, MIN_LINE_WIDTH, MIN_STEP, MIN_THICKNESS,
};
use hp_tools_create::road::{Attach, Junction, JunctionEnd, LineKind, RoadLayout, RoadMeta, RoadPoint};
use serde_json::{Value, json};

use super::semantic::*;
use super::tools::{Args, MAX_NAME, R, bounds_json, check_class, edit, material_name};

const LINE_NAMES: [&str; 4] = ["none", "solid", "dashed", "double"];
const LAYOUT_NAMES: [&str; 5] = ["path", "tee", "cross", "wye", "cul_de_sac"];
const COORD: f64 = 16384.0;

domain! { "roads";
    create_road: Edit, "Create a road",
        "Build a road from a spline of points, like the road tool: lanes of a width, shoulders, a lane material, \
        center, lane and edge lines (solid, dashed or double, made of thin convex brushes with their own \
        material, no overlay), banking, and a smooth elevation along the road. Points are [x, y] or \
        [x, y, z]; a point without z takes the ground height under it when follow_terrain is on (the road then \
        also drapes every segment on the brushes and displacements under it, raised by lift), else the height of \
        the previous point (the first one base_height, default 0). Points snap to the editor's grid on x and y \
        unless snap is false. The road is cut into segments of about step units; each lane, line and shoulder \
        of a segment is one convex brush, with the texture running along the road without seams. Refused when \
        a bend is too tight for the width. One undo step, the brushes are selected. Answers road_uid (use it \
        and any solid_id of the road with get_road, edit_road, split_road, add_road_junction, delete_road).",
        {
            "points": { "type": "array", "items": { "type": "array", "items": { "type": "number" }, "minItems": 2, "maxItems": 3 }, "minItems": 2, "maxItems": 512, "description": "Points [x, y] or [x, y, z] the road goes through, in order." },
            "layout": choice("Shape (default path). Presets use exactly two points: center and arm end; cul_de_sac uses start and bulb center. Both heights define a plane. No terrain following, banking or automatic side junctions. Only edge markings are drawn in presets.", &LAYOUT_NAMES),
            "radius": number("Corner radius, or bulb radius for cul_de_sac (default 256). Move the second point farther away to fit the width and radius.", 0.0, 4096.0),
            "lanes": integer("Number of lanes (default 2).", 1, 8),
            "lane_width": number("Width of one lane in units (default 128), within the map bounds.", MIN_LANE_WIDTH, MAX_LANE_WIDTH),
            "shoulder": number("Width of the shoulder on each side in units (default 16; 0 for none, else at least 1).", 0.0, 256.0),
            "thickness": number("How far the brushes go down under the surface (default 8).", 2.0, 128.0),
            "step": number("Target length of a segment in units (default 64).", 16.0, 512.0),
            "tex_scale": number("Texture scale of the faces (default: the editor's).", 0.01, 64.0),
            "material": string("Material of the lanes (default: the current material)."),
            "shoulder_material": string("Material of the shoulders (default: same as the lanes)."),
            "line_material": string("Material of the lines (default dev/reflectivity_90)."),
            "center_material": string("Material of the center line (default: same as the lines)."),
            "center_line": choice("Center line, for an even number of lanes (default dashed).", &LINE_NAMES),
            "edge_line": choice("Lines on the outer edges of the lanes (default solid).", &LINE_NAMES),
            "divider_line": choice("Lines between the other lanes (default dashed).", &LINE_NAMES),
            "line_width": number("Width of a line in units (default 6).", 1.0, 32.0),
            "dash": number("Length of a dash in units (default 96), rounded to whole segments.", 4.0, 1024.0),
            "gap": number("Length of the gap between dashes in units (default 96), rounded to whole segments.", 4.0, 1024.0),
            "roll_start": number("Banking in degrees at the start (default 0, positive raises the left side).", -60.0, 60.0),
            "roll_end": number("Banking in degrees at the end, interpolated along the road (default 0).", -60.0, 60.0),
            "auto_bank": number("Automatic banking for a bend of radius 1000, proportional to the curvature (default 0 is off).", 0.0, 90.0),
            "auto_bank_limit": number("Largest automatic banking in degrees (default 8).", 0.0, 45.0),
            "follow_terrain": boolean("Follow the ground under the road (default false)."),
            "lift": number("Height of the road above the ground when following the terrain (default 1).", -256.0, 256.0),
            "base_height": number("Height of the first point when it has no z and no ground (default 0).", -16384.0, 16384.0),
            "snap": boolean("Snap x and y of the points to the editor's grid (default true)."),
            "entity_class": string("Put every brush in one brush entity of this class (func_detail...) instead of the world."),
        }, ["points"];
    get_road: Read, "Read a road",
        "Read a road made by the road tool or create_road: uid, points with heights, lanes and widths, materials, \
        lines, banking, terrain and junction settings, the ids of its brushes, its length and bounds. With \
        samples the centerline samples are listed too as [distance, x, y, z, roll degrees].",
        {
            "solid_id": integer("A brush of the road (or its brush entity). Default: the selected road.", 1, i64::from(u32::MAX)),
            "road_uid": integer("The uid of the road, instead of a brush id.", 1, i64::from(u32::MAX)),
            "samples": boolean("Also list the centerline samples (default false)."),
        }, [];
    list_roads: Read, "List roads",
        "List the roads of the map: uid, brush ids, entity, number of points, lanes and total width.",
        {}, [];
    edit_road: Edit, "Edit a road",
        "Re-open a road and rebuild it in one undo step: give the solid_id (or road_uid) of the road, else the \
        selected road, and only the fields that change; points (the whole new list, read the current one with \
        get_road), lanes, widths, materials, lines, banking, terrain follow and the like keep their stored \
        values when left out. Points, point heights, insertions and removals are all done by sending the new \
        points list. Roads joined to this road follow the change; the ones that cannot are listed in stale.",
        {
            "solid_id": integer("A brush of the road (or its brush entity). Default: the selected road.", 1, i64::from(u32::MAX)),
            "road_uid": integer("The uid of the road, instead of a brush id.", 1, i64::from(u32::MAX)),
            "points": { "type": "array", "items": { "type": "array", "items": { "type": "number" }, "minItems": 2, "maxItems": 3 }, "minItems": 2, "maxItems": 512, "description": "New points [x, y] or [x, y, z]; a point without z keeps the ground or previous height rule of create_road." },
            "layout": choice("Shape: path, tee, cross, wye, cul_de_sac. Presets need exactly two points and a planar surface.", &LAYOUT_NAMES),
            "radius": number("Corner radius, or bulb radius for cul_de_sac.", 0.0, 4096.0),
            "lanes": integer("Number of lanes.", 1, 8),
            "lane_width": number("Width of one lane in units, within the map bounds.", MIN_LANE_WIDTH, MAX_LANE_WIDTH),
            "shoulder": number("Width of the shoulder on each side (0, or 1 to 256).", 0.0, 256.0),
            "thickness": number("How far the brushes go down under the surface.", 2.0, 128.0),
            "step": number("Target length of a segment in units.", 16.0, 512.0),
            "tex_scale": number("Texture scale of the faces.", 0.01, 64.0),
            "material": string("Material of the lanes."),
            "shoulder_material": string("Material of the shoulders, empty for the same as the lanes."),
            "line_material": string("Material of the lines."),
            "center_material": string("Material of the center line, empty for the same as the lines."),
            "center_line": choice("Center line.", &LINE_NAMES),
            "edge_line": choice("Edge lines.", &LINE_NAMES),
            "divider_line": choice("Lines between lanes.", &LINE_NAMES),
            "line_width": number("Width of a line in units.", 1.0, 32.0),
            "dash": number("Length of a dash in units.", 4.0, 1024.0),
            "gap": number("Length of the gap between dashes in units.", 4.0, 1024.0),
            "roll_start": number("Banking in degrees at the start.", -60.0, 60.0),
            "roll_end": number("Banking in degrees at the end.", -60.0, 60.0),
            "auto_bank": number("Automatic banking for a bend of radius 1000.", 0.0, 90.0),
            "auto_bank_limit": number("Largest automatic banking in degrees.", 0.0, 45.0),
            "follow_terrain": boolean("Follow the ground under the road."),
            "lift": number("Height above the ground when following the terrain.", -256.0, 256.0),
            "base_height": number("Height of the first point when it has no z and no ground.", -16384.0, 16384.0),
            "snap": boolean("Snap x and y of the new points to the editor's grid (default true)."),
        }, [];
    delete_road: Destructive, "Delete a road",
        "Delete all the brushes of a road (and its brush entity when it holds nothing else), in one undo step. \
        Roads joined to it keep their shape.",
        {
            "solid_id": integer("A brush of the road (or its brush entity). Default: the selected road.", 1, i64::from(u32::MAX)),
            "road_uid": integer("The uid of the road, instead of a brush id.", 1, i64::from(u32::MAX)),
        }, [];
    split_road: Edit, "Split a road",
        "Cut a road into two roads in one undo step, joined end to end without a seam (same texture run, banking \
        and height, the cut faces share a plane). Give one of point (index of an inner point, 0 based), at \
        ([x, y] near the road, the nearest place on it) or distance (units along the road from its start). A \
        point is added on the road when the place is not on an existing one. The first part keeps the uid, the \
        second one gets a new uid. Roads joined to the road follow the part they touch.",
        {
            "solid_id": integer("A brush of the road (or its brush entity). Default: the selected road.", 1, i64::from(u32::MAX)),
            "road_uid": integer("The uid of the road, instead of a brush id.", 1, i64::from(u32::MAX)),
            "point": integer("Index of an inner point of the road.", 1, 510),
            "at": vec2("Position [x, y]: the road is cut at the nearest place."),
            "distance": number("Distance in units from the start of the road, measured in the horizontal plane.", 0.0, 1000000.0),
        }, [];
    add_road_junction: Edit, "Join roads",
        "Join the start or end of a road to the side of another road, or remove that junction. The end of the \
        road is moved onto the side of the other road and cut along it, and a rounded flare (intersection piece) \
        of flare units is added on both corners, all in one undo step. The junction is remembered: when the other \
        road is edited or split, the joined road follows it. Refused when the roads are nearly parallel, too far \
        apart (more than 2048 units) or the angle is too shallow.",
        {
            "solid_id": integer("A brush of the road that is joined. Default: the selected road.", 1, i64::from(u32::MAX)),
            "road_uid": integer("The uid of the road that is joined, instead of a brush id.", 1, i64::from(u32::MAX)),
            "target_uid": integer("The uid of the road whose side is joined.", 1, i64::from(u32::MAX)),
            "target_solid_id": integer("A brush of the road whose side is joined, instead of target_uid.", 1, i64::from(u32::MAX)),
            "end": choice("Which end of the road is joined (default end).", &["start", "end"]),
            "flare": number("Length of the rounded corners in units (default 48, 0 for none).", 0.0, 512.0),
            "attach": choice("Where the end of the road meets the other road: outer (the outer edge, shoulder included, default) or lane (the lane edge: the shoulder of the other road is cut away across the width of the joined road and the joined road crosses it).", &["outer", "lane"]),
            "remove": boolean("Remove the junction of the road instead of making one."),
        }, [];
}

fn invalid(e: ApplyError) -> EditError {
    match e {
        ApplyError::Edit(e) => e,
        other => EditError::Invalid(other.to_string()),
    }
}

fn lightmap_scale(state: &AppState) -> i32 {
    match state.settings.edit.default_lightmap_scale {
        s if s > 0 => s,
        _ => 16,
    }
}

fn point_list(state: &AppState, a: &Args<'_>, base: Option<&RoadMeta>) -> R<Option<Vec<RoadPoint>>> {
    let snap = a.opt_bool("snap")?.unwrap_or(true);
    let base_height = a.opt_f64("base_height", -COORD, COORD)?;
    let Some(value) = a.get("points") else { return Ok(None) };
    let list = value.as_array().filter(|l| (2..=MAX_POINTS).contains(&l.len()));
    let list = list.ok_or_else(|| format!("points must be 2 to {MAX_POINTS} [x, y] or [x, y, z] points"))?;
    let g = grid(state);
    let follow = a.opt_bool("follow_terrain")?.or(base.map(|b| b.follow_terrain)).unwrap_or(false);
    let lift = a.opt_f64("lift", -256.0, 256.0)?.or(base.map(|b| b.lift)).unwrap_or(1.0);
    let mut z_prev = base_height.unwrap_or(0.0);
    let skip = if follow { apply::road_piece_ids(&state.doc) } else { std::collections::HashSet::new() };
    let mut out = Vec::with_capacity(list.len());
    for p in list {
        let nums: Vec<f64> = p
            .as_array()
            .into_iter()
            .flatten()
            .filter_map(Value::as_f64)
            .filter(|f| f.is_finite() && f.abs() <= COORD)
            .collect();
        let count = p.as_array().map_or(0, Vec::len);
        if nums.len() != count || !(2..=3).contains(&count) {
            return Err(format!("points must be [x, y] or [x, y, z] with numbers from -{COORD} to {COORD}"));
        }
        let xy = DVec2::new(nums[0], nums[1]);
        let xy = if snap { hp_tools_create::curve::snap_point(xy, g) } else { xy };
        let z = match nums.get(2) {
            Some(z) => *z,
            None if follow => apply::ground_height(&state.doc, &skip, xy, z_prev).map_or(z_prev, |g| g + lift),
            None => z_prev,
        };
        z_prev = z;
        out.push(RoadPoint::new(xy.x, xy.y, z));
    }
    Ok(Some(out))
}

fn line_kind(a: &Args<'_>, key: &str) -> R<Option<LineKind>> {
    Ok(a.opt_enum(key, &LINE_NAMES)?.and_then(|s| LineKind::parse(&s)))
}

fn material_arg(a: &Args<'_>, key: &str, allow_empty: bool) -> R<Option<String>> {
    match a.opt_str(key, MAX_NAME)? {
        Some("") if allow_empty => Ok(Some(String::new())),
        Some(m) => Ok(Some(material_name(m)?)),
        None => Ok(None),
    }
}

fn apply_fields(state: &AppState, a: &Args<'_>, m: &mut RoadMeta) -> R<()> {
    if let Some(v) = a.opt_enum("layout", &LAYOUT_NAMES)? {
        m.layout = RoadLayout::parse(&v).ok_or("invalid road layout")?;
    }
    if let Some(v) = a.opt_f64("radius", 0.0, hp_tools_create::road::model::MAX_RADIUS)? {
        m.radius = v;
    }
    if let Some(v) = a.opt_u64("lanes", 1, u64::from(MAX_LANES))? {
        m.lanes = v as u32;
    }
    if let Some(v) = a.opt_f64("lane_width", MIN_LANE_WIDTH, MAX_LANE_WIDTH)? {
        m.lane_width = v;
    }
    if let Some(v) = a.opt_f64("shoulder", 0.0, MAX_SHOULDER)? {
        m.shoulder = v;
    }
    if let Some(v) = a.opt_f64("thickness", MIN_THICKNESS, MAX_THICKNESS)? {
        m.thickness = v;
    }
    if let Some(v) = a.opt_f64("step", MIN_STEP, MAX_STEP)? {
        m.step = v;
    }
    if let Some(v) = a.opt_f64("tex_scale", 0.01, 64.0)? {
        m.tex_scale = v;
    }
    if let Some(v) = material_arg(a, "material", false)? {
        m.material = v;
    }
    if let Some(v) = material_arg(a, "shoulder_material", true)? {
        m.shoulder_material = v;
    }
    if let Some(v) = material_arg(a, "line_material", false)? {
        m.line_material = v;
    }
    if let Some(v) = material_arg(a, "center_material", true)? {
        m.center_material = v;
    }
    if let Some(k) = line_kind(a, "center_line")? {
        m.center = k;
    }
    if let Some(k) = line_kind(a, "edge_line")? {
        m.edge = k;
    }
    if let Some(k) = line_kind(a, "divider_line")? {
        m.divider = k;
    }
    if let Some(v) = a.opt_f64("line_width", MIN_LINE_WIDTH, MAX_LINE_WIDTH)? {
        m.line_width = v;
    }
    if let Some(v) = a.opt_f64("dash", MIN_DASH, MAX_DASH)? {
        m.dash = v;
    }
    if let Some(v) = a.opt_f64("gap", MIN_DASH, MAX_DASH)? {
        m.gap = v;
    }
    if let Some(v) = a.opt_f64("roll_start", -60.0, 60.0)? {
        m.bank.roll_start = v;
    }
    if let Some(v) = a.opt_f64("roll_end", -60.0, 60.0)? {
        m.bank.roll_end = v;
    }
    if let Some(v) = a.opt_f64("auto_bank", 0.0, 90.0)? {
        m.bank.auto_gain = v;
    }
    if let Some(v) = a.opt_f64("auto_bank_limit", 0.0, 45.0)? {
        m.bank.auto_limit = v;
    }
    if let Some(v) = a.opt_bool("follow_terrain")? {
        m.follow_terrain = v;
    }
    if let Some(v) = a.opt_f64("lift", -MAX_LIFT, MAX_LIFT)? {
        m.lift = v;
    }
    if let Some(points) = point_list(state, a, Some(m))? {
        m.points = points;
    }
    Ok(())
}

fn meta_json(m: &RoadMeta) -> Value {
    json!({
        "road_uid": m.uid,
        "points": m.points.iter().map(|p| [p.x, p.y, p.z]).collect::<Vec<_>>(),
        "layout": m.layout.name(),
        "radius": m.radius,
        "lanes": m.lanes,
        "lane_width": m.lane_width,
        "shoulder": m.shoulder,
        "total_width": m.total_width(),
        "thickness": m.thickness,
        "step": m.step,
        "tex_scale": m.tex_scale,
        "material": m.material,
        "shoulder_material": m.shoulder_material_name(),
        "line_material": m.line_material,
        "center_material": m.center_material_name(),
        "center_line": m.center.name(),
        "edge_line": m.edge.name(),
        "divider_line": m.divider.name(),
        "line_width": m.line_width,
        "dash": m.dash,
        "gap": m.gap,
        "roll_start": m.bank.roll_start,
        "roll_end": m.bank.roll_end,
        "auto_bank": m.bank.auto_gain,
        "auto_bank_limit": m.bank.auto_limit,
        "follow_terrain": m.follow_terrain,
        "lift": m.lift,
        "junction": m.junction.map(|j| json!({ "target_uid": j.target, "end": j.end.name(), "flare": j.flare, "attach": j.attach.name() })),
    })
}

fn find(state: &AppState, a: &Args<'_>, id_key: &str, uid_key: &str) -> R<(CurveRecord, RoadMeta)> {
    if let Some(uid) = a.opt_u64(uid_key, 1, u64::from(u32::MAX))? {
        return apply::find_road(&state.doc, uid as u32)
            .map(|e| (e.record, e.meta))
            .ok_or_else(|| format!("no road has the uid {uid}"));
    }
    let piece = if let Some(id) = a.opt_u64(id_key, 1, u64::from(u32::MAX))? {
        apply::road_piece_of(&state.doc, id as u32)
            .ok_or_else(|| format!("{id} is not a brush made by the road tool"))?
    } else {
        apply::selected_road_piece(&state.doc)
            .ok_or("give a solid_id or road_uid of a road, or select one made by the road tool or create_road")?
    };
    apply::read_road(&state.doc, piece).ok_or_else(|| "this road has no readable road data".to_owned())
}

fn finish(state: &mut AppState, meta: &RoadMeta, made: &RoadCreated) -> R<ToolResult> {
    let bounds =
        made.ids.iter().filter_map(|id| state.doc.scene().solid(*id).map(|n| n.bounds)).reduce(|a, b| a.union(&b));
    state.runtime.brush.last_created = bounds;
    let mut out = meta_json(meta);
    if let Value::Object(map) = &mut out {
        map.insert("road_uid".to_owned(), json!(made.uid));
        map.insert("solid_ids".to_owned(), json!(made.ids));
        map.insert("entity_id".to_owned(), json!(made.entity));
        map.insert("count".to_owned(), json!(made.ids.len()));
        map.insert("length".to_owned(), json!(made.length));
        map.insert("bounds".to_owned(), bounds_json(bounds));
        map.insert("refreshed".to_owned(), json!(made.refreshed));
        map.insert("stale".to_owned(), json!(made.stale));
    }
    Ok(ToolResult::json(out))
}

fn create_road(state: &mut AppState, a: &Args<'_>) -> R<ToolResult> {
    let mut meta = RoadMeta { material: state.current_material.clone(), ..RoadMeta::default() };
    let s = state.settings.edit.default_texture_scale;
    if s.is_finite() && s.abs() > 1e-6 {
        meta.tex_scale = s;
    }
    apply_fields(state, a, &mut meta)?;
    if meta.points.len() < 2 {
        return Err(format!("points must be 2 to {MAX_POINTS} [x, y] or [x, y, z] points"));
    }
    let meta = meta.clamped();
    let class = a.opt_str("entity_class", MAX_NAME)?.map(|c| c.trim().to_owned());
    if let Some(c) = &class {
        check_class(c)?;
    }
    apply::build_in_doc(&state.doc, &meta, &[]).map_err(|e| e.to_string())?;
    let lightmap = lightmap_scale(state);
    let made = edit(state, "create_road", |doc| {
        let target = match &class {
            Some(c) => Target::NewEntity { class: c, keyvalues: &[] },
            None => Target::World,
        };
        apply::create_road(doc, &meta, lightmap, target).map_err(invalid)
    })?;
    let mut shown = meta;
    shown.uid = made.uid;
    finish(state, &shown, &made)
}

fn get_road(state: &mut AppState, a: &Args<'_>) -> R<ToolResult> {
    let (record, meta) = find(state, a, "solid_id", "road_uid")?;
    let road = apply::build_in_doc(&state.doc, &meta, &record.ids).map_err(|e| e.to_string())?;
    let mut out = meta_json(&meta);
    if let Value::Object(map) = &mut out {
        map.insert("solid_ids".to_owned(), json!(record.ids));
        let entity = match record.owner {
            hp_editor::Owner::Entity(e) => Some(e),
            hp_editor::Owner::World => None,
        };
        map.insert("entity_id".to_owned(), json!(entity));
        map.insert("length".to_owned(), json!(road.length));
        map.insert("segments".to_owned(), json!(road.samples.len().saturating_sub(1)));
        map.insert("bounds".to_owned(), bounds_json(road.bounds()));
        if a.opt_bool("samples")?.unwrap_or(false) {
            let list: Vec<Value> =
                road.samples.iter().map(|s| json!([s.s, s.center.x, s.center.y, s.center.z, s.roll])).collect();
            map.insert("samples".to_owned(), json!(list));
        }
    }
    Ok(ToolResult::json(out))
}

fn list_roads(state: &mut AppState, _a: &Args<'_>) -> R<ToolResult> {
    let list: Vec<Value> = hp_tools_create::road::summarize(&state.doc)
        .into_iter()
        .map(|r| {
            json!({
                "road_uid": r.uid,
                "solid_ids": r.solid_ids,
                "entity_id": r.entity_id,
                "points": r.points,
                "lanes": r.lanes,
                "total_width": r.total_width,
            })
        })
        .collect();
    Ok(ToolResult::json(json!({ "count": list.len(), "roads": list })))
}

fn edit_road(state: &mut AppState, a: &Args<'_>) -> R<ToolResult> {
    let (record, mut meta) = find(state, a, "solid_id", "road_uid")?;
    apply_fields(state, a, &mut meta)?;
    let meta = meta.clamped();
    apply::build_in_doc(&state.doc, &meta, &record.ids).map_err(|e| e.to_string())?;
    let lightmap = lightmap_scale(state);
    let made = edit(state, "edit_road", |doc| apply::replace_road(doc, &record, &meta, lightmap).map_err(invalid))?;
    finish(state, &meta, &made)
}

fn delete_road(state: &mut AppState, a: &Args<'_>) -> R<ToolResult> {
    let (record, meta) = find(state, a, "solid_id", "road_uid")?;
    let lightmap = lightmap_scale(state);
    let n = edit(state, "delete_road", |doc| apply::delete_road(doc, &record, lightmap).map_err(invalid))?;
    Ok(ToolResult::json(json!({ "road_uid": meta.uid, "deleted_brushes": n })))
}

fn split_road(state: &mut AppState, a: &Args<'_>) -> R<ToolResult> {
    let (record, meta) = find(state, a, "solid_id", "road_uid")?;
    let at = match (a.opt_u64("point", 1, 510)?, a.get("at"), a.opt_f64("distance", 0.0, 1_000_000.0)?) {
        (Some(p), None, None) => SplitAt::Point(p as usize),
        (None, Some(v), None) => {
            let xy = v
                .as_array()
                .filter(|l| l.len() == 2)
                .and_then(|l| Some(DVec2::new(l[0].as_f64()?, l[1].as_f64()?)))
                .filter(|p| p.is_finite())
                .ok_or("at must be [x, y]")?;
            SplitAt::Near(xy)
        }
        (None, None, Some(d)) => SplitAt::Arc(d),
        _ => return Err("give exactly one of point, at or distance".to_owned()),
    };
    let lightmap = lightmap_scale(state);
    let (first, second) =
        edit(state, "split_road", |doc| apply::split_road(doc, &record, &meta, at, lightmap).map_err(invalid))?;
    let part =
        |m: &RoadCreated| json!({ "road_uid": m.uid, "solid_ids": m.ids, "entity_id": m.entity, "length": m.length });
    Ok(ToolResult::json(json!({ "first": part(&first), "second": part(&second), "stale": first.stale })))
}

fn add_road_junction(state: &mut AppState, a: &Args<'_>) -> R<ToolResult> {
    let (record, meta) = find(state, a, "solid_id", "road_uid")?;
    let lightmap = lightmap_scale(state);
    let junction = if a.opt_bool("remove")?.unwrap_or(false) {
        None
    } else {
        let target = if let Some(uid) = a.opt_u64("target_uid", 1, u64::from(u32::MAX))? {
            uid as u32
        } else if let Some(id) = a.opt_u64("target_solid_id", 1, u64::from(u32::MAX))? {
            let piece = apply::road_piece_of(&state.doc, id as u32)
                .ok_or_else(|| format!("{id} is not a brush made by the road tool"))?;
            apply::read_road(&state.doc, piece).map(|(_, m)| m.uid).ok_or("the other road has no road data")?
        } else {
            return Err("give target_uid or target_solid_id, or remove: true".to_owned());
        };
        let end = match a.opt_enum("end", &["start", "end"])? {
            Some(e) => JunctionEnd::parse(&e).unwrap_or(JunctionEnd::End),
            None => JunctionEnd::End,
        };
        let flare = a.opt_f64("flare", 0.0, MAX_FLARE)?.unwrap_or(48.0);
        let attach = match a.opt_enum("attach", &["outer", "lane"])? {
            Some(s) => Attach::parse(&s).unwrap_or_default(),
            None => Attach::Outer,
        };
        Some(Junction { end, target, flare, attach })
    };
    let made = edit(state, "add_road_junction", |doc| {
        apply::set_junction(doc, &record, &meta, junction, lightmap).map_err(invalid)
    })?;
    let mut shown = meta;
    shown.junction = junction;
    finish(state, &shown, &made)
}

#[cfg(test)]
mod tests {
    use serde_json::json;

    use super::super::semantic_tests::{err, ok};

    fn ids(v: &serde_json::Value) -> Vec<u32> {
        v["solid_ids"].as_array().unwrap().iter().map(|x| x.as_u64().unwrap() as u32).collect()
    }

    #[test]
    fn wide_roads_and_editable_junction_shapes_are_available_through_mcp() {
        let mut state = hp_app::testing::test_state();
        let wide = ok(
            &mut state,
            "create_road",
            json!({"points":[[0,0,0],[1024,0,0]],"lanes":1,"lane_width":4096,"shoulder":0,"center_line":"none","edge_line":"none","divider_line":"none","snap":false}),
        );
        assert_eq!(wide["lane_width"].as_f64(), Some(4096.0));
        for (i, layout) in ["tee", "cross", "wye", "cul_de_sac"].iter().enumerate() {
            let y = 4000 + i * 2500;
            let made = ok(
                &mut state,
                "create_road",
                json!({"points":[[0,y,0],[1024,y,32]],"layout":layout,"radius":256,"snap":false}),
            );
            let uid = made["road_uid"].as_u64().unwrap();
            let read = ok(&mut state, "get_road", json!({"road_uid":uid}));
            assert_eq!(read["layout"], *layout);
            let changed = ok(&mut state, "edit_road", json!({"road_uid":uid,"radius":192}));
            assert_eq!(changed["radius"].as_f64(), Some(192.0));
        }
        err(&mut state, "create_road", json!({"points":[[0,0],[1024,0],[2048,0]],"layout":"cross"}));
    }

    #[test]
    fn roads_are_created_read_edited_split_joined_and_deleted() {
        let mut s = hp_app::testing::test_state();
        s.doc.clear_selection();
        let before = s.doc.stats().solids;
        let (undo, _) = s.doc.history_len();
        let r = ok(
            &mut s,
            "create_road",
            json!({ "points": [[-200, 0, 0], [0, 40, 24], [200, 0, 0]], "lanes": 2, "step": 64, "material": "concrete/concretefloor001a", "center_line": "double", "snap": false }),
        );
        let uid = r["road_uid"].as_u64().unwrap();
        let count = r["count"].as_u64().unwrap() as usize;
        assert!(count > 20, "{count}");
        assert_eq!(s.doc.stats().solids, before + count);
        assert_eq!(s.doc.history_len().0, undo + 1, "one undo step");
        assert_eq!(r["points"][1], json!([0.0, 40.0, 24.0]));
        assert_eq!(r["total_width"], json!(288.0));
        let brushes = ids(&r);
        assert_eq!(s.doc.selected_solid_ids().len(), count, "selected");

        let got = ok(&mut s, "get_road", json!({ "solid_id": brushes[3], "samples": true }));
        assert_eq!(got["road_uid"], json!(uid));
        assert_eq!(got["center_line"], "double");
        assert!(got["samples"].as_array().unwrap().len() > 3);
        let listed = ok(&mut s, "list_roads", json!({}));
        assert_eq!(listed["count"], 1);

        let edited = ok(
            &mut s,
            "edit_road",
            json!({ "road_uid": uid, "lanes": 4, "points": [[-200, 0, 0], [200, 0, 40]], "snap": false }),
        );
        assert_eq!(edited["lanes"], 4);
        assert_eq!(edited["points"][1], json!([200.0, 0.0, 40.0]));
        let (undo_before, _) = s.doc.history_len();
        let again = ok(&mut s, "edit_road", json!({ "road_uid": uid, "shoulder": 0 }));
        assert_eq!(s.doc.history_len().0, undo_before + 1);
        assert_eq!(again["total_width"], json!(512.0));

        let cut = ok(&mut s, "split_road", json!({ "road_uid": uid, "distance": 200 }));
        let second_uid = cut["second"]["road_uid"].as_u64().unwrap();
        assert_ne!(second_uid, uid);
        assert_eq!(ok(&mut s, "list_roads", json!({}))["count"], 2);
        err(&mut s, "split_road", json!({ "road_uid": uid }));
        err(&mut s, "split_road", json!({ "road_uid": uid, "distance": 1 }));

        let branch = ok(
            &mut s,
            "create_road",
            json!({ "points": [[-100, -700], [-100, -300]], "snap": false, "base_height": 0, "lanes": 1, "lane_width": 64, "shoulder": 0 }),
        );
        let branch_uid = branch["road_uid"].as_u64().unwrap();
        let joined = ok(
            &mut s,
            "add_road_junction",
            json!({ "road_uid": branch_uid, "target_uid": uid, "end": "end", "flare": 32 }),
        );
        assert_eq!(joined["junction"]["target_uid"], json!(uid));
        let read = ok(&mut s, "get_road", json!({ "road_uid": branch_uid, "samples": true }));
        let last_y = read["samples"].as_array().unwrap().last().unwrap()[2].as_f64().unwrap();
        assert!((last_y + 256.0).abs() < 1.0, "{last_y}");
        err(&mut s, "add_road_junction", json!({ "road_uid": uid, "target_uid": uid }));
        let freed = ok(&mut s, "add_road_junction", json!({ "road_uid": branch_uid, "remove": true }));
        assert!(freed["junction"].is_null());

        let solids = s.doc.stats().solids;
        let gone = ok(&mut s, "delete_road", json!({ "road_uid": branch_uid }));
        assert!(gone["deleted_brushes"].as_u64().unwrap() > 0);
        assert!(s.doc.stats().solids < solids);
        assert!(s.doc.undo());
        assert_eq!(s.doc.stats().solids, solids);
    }

    #[test]
    fn bad_roads_are_refused_without_changes() {
        let mut s = hp_app::testing::test_state();
        let solids = s.doc.stats().solids;
        err(&mut s, "create_road", json!({ "points": [[0, 0]] }));
        err(&mut s, "create_road", json!({ "points": [[0, 0], [1, 1]] }));
        err(&mut s, "create_road", json!({ "points": [[0, 0], [256, 0]], "lanes": 9 }));
        err(&mut s, "create_road", json!({ "points": [[0, 0], [256, 0]], "center_line": "wavy" }));
        err(&mut s, "create_road", json!({ "points": [[0, 0, 0, 0], [256, 0]] }));
        err(&mut s, "create_road", json!({ "points": [[0, 0], [256, 0]], "lane_width": 16, "line_width": 30 }));
        err(&mut s, "create_road", json!({ "points": [[0, 0], [256, 0]], "entity_class": "bad class" }));
        err(&mut s, "get_road", json!({ "solid_id": 4_000_000 }));
        err(&mut s, "get_road", json!({}));
        assert_eq!(s.doc.stats().solids, solids);
        let made = ok(&mut s, "create_road", json!({ "points": [[-200, 0], [200, 0]], "entity_class": "func_detail" }));
        assert!(made["entity_id"].as_u64().is_some());
        let got = ok(&mut s, "get_road", json!({ "solid_id": made["entity_id"] }));
        assert_eq!(got["road_uid"], made["road_uid"]);
    }
}
