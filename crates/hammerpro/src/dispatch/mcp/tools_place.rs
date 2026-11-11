use glam::{DMat3, DMat4, DQuat, DVec2, DVec3};
use hp_app::AppState;
use hp_editor::ObjRef;
use hp_geom::Aabb;
use hp_mcp::protocol::{Content, ToolResult};
use hp_shape::TriMesh;
use serde_json::{Value, json};

use super::place_ops::{self, OverlapInfo, Settle};
use super::place_world::{self as pw, EnvOptions, Key, MeshChoice, Obj, Resolved, World};
use super::semantic::*;
use super::tools::{Args, MAX_NAME, Outcome, R, Worker, check_class, encode_png};

pub(super) const UNITS: &str = "Source units (1 unit = 1 inch = 2.54 cm), Z up, angles are pitch yaw roll in degrees";

const SIDES: &[&str] = &["+x", "-x", "+y", "-y", "+z", "-z"];
const VIEWS: &[&str] = &["top", "bottom", "front", "back", "side", "other_side", "iso"];

domain! { "placement";
    get_object_shape: Read, "Exact shape of objects or models",
        "The real shape of anything in the map or of a model file, measured on the real geometry (the .mdl mesh after \
        angles, modelscale and body groups, the .phy collision mesh, brush planes, displaced surfaces, overlay quads, \
        instance contents, groups as one object), never a generic box. Answers in world space: exact bounds and size, \
        bottom and top Z, the oriented box in the object's own axes, the footprint polygon on the ground (convex \
        hull of the mesh seen from above) with its smallest rectangle, the contact patch (the lowest points that \
        touch the floor), the pivot (origin) against the mesh (origin_above_bottom: how high the origin sits above \
        the lowest point), attachment points, render and collision bounds, horizontal slices (outline and size at \
        several heights: what it looks like) and with detail full a top height map. Give model (with origin, angles, \
        scale, body) to measure a model before placing it. Units: Source units, Z up.",
        {
            "ids": target_ids(), "solid_ids": target_solid_ids(),
            "group_ids": ids("Groups measured as one object each."),
            "model": string("A model path (models/....mdl) to measure without placing it."),
            "origin": vec3("model only: where the origin would be (default [0, 0, 0])."),
            "angles": vec3("model only: pitch yaw roll (default [0, 0, 0])."),
            "scale": number("model only: modelscale (default 1).", 0.01, 100.0),
            "body": integer("model only: body group value (default 0).", 0, 65535),
            "mesh": choice("render (what you see), collision (the .phy physics mesh) or both (default).", &["render", "collision", "both"]),
            "detail": choice("summary (default) or full (more slices and the top height map).", &["summary", "full"]),
            "slices": integer("Horizontal slices (default 4, full: 8).", 1, 32),
            "height_cells": integer("Cells on the longest side of the height map (default 12, full only).", 2, 32),
            "include_tools": boolean("Keep invisible tool faces (clip, trigger, hint...) in brush shapes (default false)."),
        }, [];
    check_placement: Read, "Check how objects sit",
        "For each object (default: the selection), measured on real meshes against everything around it (world \
        brushes, brush entities, displacements, props with their real mesh): status (resting, floating, sunk, \
        attached when it touches a wall or ceiling but not the floor, no_surface when nothing is below), gap_below \
        (positive: floating that high above the surface under it, negative: sunk that deep), what it rests on, the free \
        distance on each side (+x, -x, +y, -y, +z), the sides it touches, and every object it interpenetrates with the \
        penetration depth and the smallest push that frees it. Use it after placing things.",
        {
            "ids": target_ids(), "solid_ids": target_solid_ids(),
            "group_ids": ids("Groups checked as one object each."),
            "mesh": choice("render (default) or collision meshes.", &["render", "collision"]),
            "tolerance": number("Distance counted as touching (default 0.05).", 0.0, 16.0),
            "clearance": number("Longest side distance measured (default 128).", 1.0, 4096.0),
            "include_tools": boolean("Count invisible tool brushes (clip, trigger...) as obstacles (default false)."),
        }, [];
    place_object: Edit, "Place an object precisely",
        "Move an existing object (ids, solid_ids or group_ids: moved together as one unit) or create a new prop \
        (model, classname) so that its real mesh touches the surface exactly: no gap, no overlap. Modes: drop (default: \
        from position or where it is, fall along direction, default down, until the real lowest point touches the real \
        surface, sunk objects are lifted out first), on (on top of target, at position x y or the target's center), \
        against (slide along direction until flush with a wall; facing away turns a model so its front, +X, points \
        away from the wall), next_to (beside target on side, flush, centered or aligned, then dropped to the floor), \
        at (only put it at position and angles). position sets the anchor: origin (default for entities), \
        bottom_center (center of the lowest face of its bounds) or center. gap leaves a space. The result is checked \
        again (gap_below and overlaps). One undo step; dry_run computes without changing the map.",
        {
            "ids": target_ids(), "solid_ids": target_solid_ids(),
            "group_ids": ids("Groups to place (moved as one unit)."),
            "model": string("Create this model (models/....mdl) instead of moving objects."),
            "classname": string("Class of the created entity (default prop_static)."),
            "keyvalues": keyvalues("Keyvalues of the created entity (skin, body, modelscale, solid...)."),
            "mode": choice("drop (default), on, against, next_to or at.", &["drop", "on", "against", "next_to", "at"]),
            "position": vec3("Where the anchor goes before settling (drop, on: x y; at: exact)."),
            "anchor": choice("Point that position names (default origin for entities, bottom_center for brushes).", &["origin", "bottom_center", "center"]),
            "angles": vec3("Absolute pitch yaw roll for entities and new models."),
            "rotate_yaw": number("Turn around Z by these degrees first (any object).", -360.0, 360.0),
            "target": integer("on, next_to: the object to place on or beside (entity id, or solid id).", 1, i64::from(u32::MAX)),
            "side": choice("next_to: side of target (default +x).", SIDES),
            "align": choice("next_to: alignment on the other horizontal axis (default center).", &["center", "min", "max"]),
            "direction": vec3("drop: fall direction (default [0, 0, -1]); against: direction toward the wall (required)."),
            "facing": choice("against: keep (default), away (front +X away from the wall) or toward.", &["keep", "away", "toward"]),
            "align_to_surface": boolean("drop, on: tilt the object to the surface normal (default false)."),
            "fit": choice("drop, on: normal (default) or supports: tilt until the lowest points (wheels, legs, feet) all touch the ground when they can (the largest gap under them is made as small as possible), else say why.", &["normal", "supports"]),
            "then_drop": boolean("against, next_to: drop to the floor afterwards (default false for against, true for next_to sides)."),
            "gap": number("Space left at the contact (default 0).", 0.0, 256.0),
            "mesh": choice("render (default) or collision meshes for the contact.", &["render", "collision"]),
            "max_distance": number("Longest travel (default 4096).", 1.0, 65536.0),
            "ignore_ids": ids("Objects that are not obstacles."),
            "include_tools": boolean("Count invisible tool brushes as surfaces (default false)."),
            "dry_run": boolean("Compute and answer without changing the map (default false)."),
        }, [];
    find_floating_objects: Edit, "Find floating and sunk objects",
        "Scan the whole map or a box for objects that float above the surface under them or are sunk into it, \
        measured on real meshes (props by default; entities or all for brush entities and world brushes too). \
        Objects touching a wall or ceiling (wall lamps, hanging signs) are attached, not floating. fix moves the \
        floating ones down until they touch, fix_sunk lifts the sunk ones out (up to max_sink), lowest first so \
        stacks settle together, in one undo step.",
        {
            "min": vec3("Box minimum (default: whole map)."), "max": vec3("Box maximum."),
            "ids": ids("Only these objects."),
            "include": choice("props (default: entities with a model), entities (also brush entities) or all (also world brushes).", &["props", "entities", "all"]),
            "tolerance": number("Gap or depth that is fine (default 0.5).", 0.0, 64.0),
            "mesh": choice("render (default) or collision meshes.", &["render", "collision"]),
            "fix": boolean("Drop the floating objects onto their surface (default false)."),
            "fix_sunk": boolean("Lift the sunk objects out (default false)."),
            "max_sink": number("fix_sunk: deepest sink fixed (default 16).", 0.0, 1024.0),
            "max_drop": number("Longest distance searched below (default 4096).", 1.0, 65536.0),
            "limit": limit(),
        }, [];
    render_shape: Read, "Drawing of a shape with dimensions",
        "A PNG drawing of objects, of a model file or of every object in a box, from several axes at once (top, \
        front, side, iso by default) with the true mesh shaded, a unit grid, the size on each axis, the origin \
        (red), the lowest point (green), attachments and the ground line. Collision meshes can be drawn as a wire \
        over the render mesh (mesh both). Use it to see what a model really looks like before placing it.",
        {
            "ids": target_ids(), "solid_ids": target_solid_ids(),
            "group_ids": ids("Groups drawn."),
            "model": string("A model path (models/....mdl) to draw."),
            "angles": vec3("model only: pitch yaw roll."),
            "scale": number("model only: modelscale (default 1).", 0.01, 100.0),
            "body": integer("model only: body group value (default 0).", 0, 65535),
            "min": vec3("Draw everything in this box (with max)."), "max": vec3("Box maximum."),
            "views": strings("Views: top, bottom, front (from +X), back, side (from -Y), other_side, iso (default top, front, side, iso).", 7),
            "width": integer("Image width (default 1024).", 256, 2048),
            "height": integer("Image height (default 768).", 256, 2048),
            "mesh": choice("render (default), collision, or both (collision as a wire).", &["render", "collision", "both"]),
            "markers": boolean("Draw origin, lowest point and attachments (default true)."),
            "dimensions": boolean("Draw sizes (default true)."),
            "grid": boolean("Draw the unit grid (default true)."),
            "include_tools": boolean("Draw invisible tool brushes too (default false)."),
        }, [];
}

pub(super) fn compact(v: Value) -> ToolResult {
    let text = serde_json::to_string(&v).unwrap_or_default();
    ToolResult { content: vec![Content::Text(text)], structured: v.is_object().then_some(v), is_error: false }
}

pub(super) fn r2(x: f64) -> f64 {
    place_ops::round(x)
}

pub(super) fn p3(v: DVec3) -> Value {
    json!([r2(v.x), r2(v.y), r2(v.z)])
}

fn p2(v: DVec2) -> Value {
    json!([r2(v.x), r2(v.y)])
}

pub(super) fn who(o: &Obj) -> Value {
    let mut v = json!({ "id": o.key.id(), "kind": o.key.kind(), "type": o.kind });
    if !o.class.is_empty() {
        v["class"] = json!(o.class);
    }
    if let Some(m) = o.model_path() {
        v["model"] = json!(m);
    }
    v
}

fn mesh_choice(a: &Args<'_>) -> R<MeshChoice> {
    Ok(match a.opt_enum("mesh", &["render", "collision", "both"])?.as_deref() {
        Some("collision") => MeshChoice::Collision,
        _ => MeshChoice::Render,
    })
}

fn group_list(state: &AppState, a: &Args<'_>) -> R<Vec<u32>> {
    let g = a.ids("group_ids")?;
    for id in &g {
        if state.doc.scene().group(*id).is_none() {
            return Err(format!("no group has the id {id}"));
        }
    }
    Ok(g)
}

fn picked(state: &AppState, a: &Args<'_>, allow_empty: bool) -> R<Vec<ObjRef>> {
    if a.get("ids").is_some() || a.get("solid_ids").is_some() {
        return targets(state, a);
    }
    if a.get("group_ids").is_some() || allow_empty {
        return Ok(Vec::new());
    }
    targets(state, a)
}

fn model_arg(a: &Args<'_>) -> R<Option<String>> {
    let Some(m) = a.opt_str("model", MAX_NAME)? else { return Ok(None) };
    let m = m.trim().replace('\\', "/").to_ascii_lowercase();
    if !m.starts_with("models/") || !m.ends_with(".mdl") || m.contains("..") || m.contains(':') {
        return Err("model must look like models/.../name.mdl".to_owned());
    }
    Ok(Some(m))
}

fn angles_arg(a: &Args<'_>, key: &str) -> R<Option<DVec3>> {
    match a.opt_vec3(key)? {
        Some(v) if v.abs().max_element() > 3600.0 => Err(format!("{key} must be degrees from -3600 to 3600")),
        other => Ok(other),
    }
}

fn model_obj(path: &str, origin: DVec3, angles: DVec3, scale: f64, body: u32, class: &str) -> Obj {
    Obj {
        key: Key::Entity(0),
        kind: if class.starts_with("prop_") { "prop" } else { "model_entity" },
        class: class.to_owned(),
        meshes: Vec::new(),
        models: vec![pw::ModelRef {
            path: path.to_owned(),
            body,
            matrix: pw::model_matrix(origin, angles, scale),
            fallback: None,
        }],
        origin: Some(origin),
        angles,
        scale,
        hint: Aabb::new(origin, origin).expand(1024.0 * scale.max(1.0)),
        refs: Vec::new(),
        pending: false,
        solid_ids: Vec::new(),
    }
}

fn objects_of(state: &AppState, a: &Args<'_>, include_tools: bool) -> R<Vec<Obj>> {
    let mut out = Vec::new();
    for g in group_list(state, a)? {
        if let Some(o) = pw::group_obj(state, g, include_tools) {
            out.push(o);
        }
    }
    for r in picked(state, a, !out.is_empty())? {
        if let Some(o) = pw::obj_for(state, r, include_tools) {
            out.push(o);
        }
    }
    if out.is_empty() {
        return Err("nothing to measure: give ids, solid_ids, group_ids or model, or select objects".to_owned());
    }
    Ok(out)
}

fn simplify_hull(mut h: Vec<DVec2>, max: usize) -> Vec<DVec2> {
    while h.len() > max {
        let n = h.len();
        let mut best = (f64::INFINITY, 0);
        for i in 0..n {
            let (a, b, c) = (h[(i + n - 1) % n], h[i], h[(i + 1) % n]);
            let area = (b - a).perp_dot(c - a).abs();
            if area < best.0 {
                best = (area, i);
            }
        }
        h.remove(best.1);
    }
    h
}

fn shape_json(o: &Obj, r: &Resolved, col: Option<&Resolved>, slices: usize, cells: Option<usize>) -> Value {
    let mesh = r.compound();
    let b = mesh.bounds();
    let mut v = who(o);
    if b.is_empty() {
        v["error"] = json!("no geometry");
        return v;
    }
    let size = b.size();
    v["bounds"] = json!({ "min": p3(b.min), "max": p3(b.max), "size": p3(size) });
    v["center"] = p3(b.center());
    v["bottom_z"] = json!(r2(b.min.z));
    v["top_z"] = json!(r2(b.max.z));
    if let Some(origin) = o.origin {
        v["origin"] = p3(origin);
        if !matches!(o.key, Key::Group(_)) {
            v["angles"] = p3(o.angles);
        }
        if (o.scale - 1.0).abs() > 1e-9 {
            v["scale"] = json!(r2(o.scale));
        }
        let bottom_center = DVec3::new(b.center().x, b.center().y, b.min.z);
        v["pivot"] = json!({
            "origin_above_bottom": r2(origin.z - b.min.z),
            "bottom_center_from_origin": p3(bottom_center - origin),
            "center_from_origin": p3(b.center() - origin),
        });
        if let Some(ob) = hp_shape::oriented_box(&mesh, o.rotation()) {
            v["oriented_box"] = json!({
                "size_forward_left_up": p3(ob.size),
                "center": p3(ob.center),
                "angles": p3(o.angles),
            });
        }
    }
    let hull = hp_shape::footprint(&mesh);
    if let Some(rect) = hp_shape::min_area_rect(&hull) {
        v["footprint"] = json!({
            "area": r2(hp_shape::polygon_area(&hull)),
            "rect": { "center": p2(rect.center), "length": r2(rect.size.x), "width": r2(rect.size.y), "yaw": r2(rect.angle) },
            "outline": simplify_hull(hull, 16).into_iter().map(p2).collect::<Vec<_>>(),
        });
    }
    if let Some(c) = hp_shape::contact_patch(&mesh, 0.5) {
        v["contact"] = json!({
            "z": r2(c.z),
            "area": r2(c.area),
            "center": p2(c.center),
            "outline": simplify_hull(c.outline, 12).into_iter().map(p2).collect::<Vec<_>>(),
        });
    }
    v["triangles"] = json!(mesh.tris.len());
    if matches!(o.kind, "solid" | "brush_entity") && o.models.is_empty() {
        let pts = mesh.welded(0.01).used_vertices();
        v["faces"] = json!(
            r.pieces
                .iter()
                .map(|p| {
                    let mut tags = p.tags.clone();
                    tags.sort_unstable();
                    tags.dedup();
                    tags.len()
                })
                .sum::<usize>()
        );
        if pts.len() <= 64 {
            v["vertices"] = json!(pts.into_iter().map(p3).collect::<Vec<_>>());
        }
    }
    if let Some(col) = col {
        let cm = col.compound();
        let cb = cm.bounds();
        if !cb.is_empty() && col.shapes.iter().any(|(s, _)| !s.collision.is_empty()) {
            v["collision"] = json!({
                "bounds": { "min": p3(cb.min), "max": p3(cb.max), "size": p3(cb.size()) },
                "pieces": col.pieces.len(),
                "bottom_z": r2(cb.min.z),
            });
        } else if !o.models.is_empty() {
            v["collision"] = json!({ "none": "no .phy collision model: the game uses no physics shape, static props use the render mesh or the bounding box" });
        }
    }
    let attachments: Vec<Value> = r
        .shapes
        .iter()
        .flat_map(|(s, m)| {
            s.attachments.iter().take(32).map(move |a| {
                json!({
                    "name": a.name,
                    "pos": p3(m.transform_point3(a.pos)),
                    "forward": p3(m.transform_vector3(a.forward).normalize_or_zero()),
                })
            })
        })
        .collect();
    if !attachments.is_empty() {
        v["attachments"] = json!(attachments);
    }
    if let Some((s, _)) = r.shapes.first() {
        if !s.surface_prop.is_empty() {
            v["surface_prop"] = json!(s.surface_prop);
        }
        v["static_prop_model"] = json!(s.static_prop);
    }
    let sl: Vec<Value> = hp_shape::slices(&mesh, slices)
        .into_iter()
        .map(|s| {
            if s.outline.is_empty() {
                return json!({ "z": r2(s.z), "empty": true });
            }
            json!({
                "z": r2(s.z),
                "size": [r2(s.max.x - s.min.x), r2(s.max.y - s.min.y)],
                "center": p2((s.min + s.max) * 0.5),
                "area": r2(s.area),
                "outline": simplify_hull(s.outline, 8).into_iter().map(p2).collect::<Vec<_>>(),
            })
        })
        .collect();
    v["slices"] = json!(sl);
    if let Some(cells) = cells
        && let Some(hm) = hp_shape::height_map(&mesh, cells)
    {
        let rows: Vec<Value> = (0..hm.ny)
            .map(|j| {
                json!(
                    (0..hm.nx)
                        .map(|i| hm.top[j * hm.nx + i].map(|z| (z - b.min.z) * 10.0).map(|z| z.round() / 10.0))
                        .collect::<Vec<_>>()
                )
            })
            .collect();
        v["height_map"] = json!({
            "origin": p2(hm.min),
            "cell": r2(hm.cell),
            "about": "top[j][i]: height of the highest surface above bottom_z at x = origin.x + (i + 0.5) * cell, y = origin.y + (j + 0.5) * cell; null: nothing there",
            "top": rows,
        });
    }
    if !r.notes.is_empty() {
        v["notes"] = json!(r.notes);
    }
    v
}

fn get_object_shape(state: &mut AppState, a: &Args<'_>) -> R<Outcome> {
    let include_tools = a.opt_bool("include_tools")?.unwrap_or(false);
    let detail_full = a.opt_enum("detail", &["summary", "full"])?.as_deref() == Some("full");
    let slices = a.opt_u64("slices", 1, 32)?.unwrap_or(if detail_full { 8 } else { 4 }) as usize;
    let cells_arg = a.opt_u64("height_cells", 2, 32)?;
    let cells = if detail_full { Some(cells_arg.unwrap_or(12) as usize) } else { None };
    let mesh = a.opt_enum("mesh", &["render", "collision", "both"])?.unwrap_or_else(|| "both".to_owned());
    let objs = match model_arg(a)? {
        Some(path) => {
            if a.get("ids").is_some() || a.get("solid_ids").is_some() || a.get("group_ids").is_some() {
                return Err("give model or ids, not both".to_owned());
            }
            let origin = a.opt_vec3("origin")?.unwrap_or(DVec3::ZERO);
            let angles = angles_arg(a, "angles")?.unwrap_or(DVec3::ZERO);
            let scale = a.opt_f64("scale", 0.01, 100.0)?.unwrap_or(1.0);
            let body = a.opt_u64("body", 0, 65535)?.unwrap_or(0) as u32;
            vec![model_obj(&path, origin, angles, scale, body, "prop_static")]
        }
        None => {
            for k in ["origin", "angles", "scale", "body"] {
                if a.get(k).is_some() {
                    return Err(format!("{k} only goes with model"));
                }
            }
            let objs = objects_of(state, a, include_tools)?;
            if objs.len() > 64 {
                return Err("at most 64 objects per call".to_owned());
            }
            objs
        }
    };
    let fs = state.assets.fs().cloned();
    let model_only = objs.len() == 1 && objs[0].key == Key::Entity(0);
    Ok(Worker::spawn("get_object_shape", move || {
        let fs = fs.as_ref();
        let mut out = Vec::new();
        for o in &objs {
            let main = if mesh == "collision" { MeshChoice::Collision } else { MeshChoice::Render };
            let r = pw::resolve(o, fs, main);
            if model_only && r.shapes.is_empty() {
                return ToolResult::error(format!("get_object_shape: {}", r.notes.join("; ")));
            }
            let col = (mesh == "both" && !o.models.is_empty()).then(|| pw::resolve(o, fs, MeshChoice::Collision));
            let mut v = shape_json(o, &r, col.as_ref(), slices, cells);
            if model_only {
                v.as_object_mut().map(|m| m.remove("id"));
                v.as_object_mut().map(|m| m.remove("kind"));
            }
            v["mesh"] = json!(main.name());
            out.push(v);
        }
        compact(json!({ "units": UNITS, "objects": out }))
    }))
}

struct Unit {
    obj: Obj,
    refs: Vec<ObjRef>,
}

fn units_of(state: &AppState, a: &Args<'_>, include_tools: bool, group_roots: bool) -> R<Vec<Unit>> {
    let groups = group_list(state, a)?;
    let refs = picked(state, a, !groups.is_empty())?;
    Ok(units_from(state, groups, refs, include_tools, group_roots))
}

fn units_from(
    state: &AppState,
    groups: Vec<u32>,
    refs: Vec<ObjRef>,
    include_tools: bool,
    group_roots: bool,
) -> Vec<Unit> {
    let mut out: Vec<Unit> = Vec::new();
    for g in groups {
        if let Some(o) = pw::group_obj(state, g, include_tools) {
            out.push(Unit { refs: o.refs.clone(), obj: o });
        }
    }
    let scene = state.doc.scene();
    for r in refs {
        if group_roots && let Some(g) = scene.object_group(r).map(|g| scene.group_root(g)) {
            if out.iter().any(|u| u.obj.key == Key::Group(g)) {
                continue;
            }
            if let Some(o) = pw::group_obj(state, g, include_tools) {
                out.push(Unit { refs: o.refs.clone(), obj: o });
            }
            continue;
        }
        if let Some(o) = pw::obj_for(state, r, include_tools) {
            out.push(Unit { refs: vec![r], obj: o });
        }
    }
    out
}

fn env_for(state: &AppState, refs: &[ObjRef], region: Option<Aabb>, include_tools: bool, ignore: &[u32]) -> Vec<Obj> {
    let (mut ss, mut se) = pw::moving_sets(state, refs);
    for id in ignore {
        if state.doc.scene().entity(*id).is_some() {
            se.insert(*id);
            ss.extend(state.doc.scene().entity_solid_ids(*id));
        } else {
            ss.insert(*id);
        }
    }
    pw::environment(state, &ss, &se, &EnvOptions { include_tools, include_points: false, region })
}

fn region_around(hint: &Aabb, dir: DVec3, dist: f64) -> Aabb {
    let back = Aabb::new(hint.min - dir * hint.size().length(), hint.max - dir * hint.size().length());
    let ahead = Aabb::new(hint.min + dir * dist, hint.max + dir * dist);
    hint.union(&back).union(&ahead).expand(64.0)
}

fn overlaps_json(world: &World, list: &[OverlapInfo]) -> Vec<Value> {
    list.iter()
        .map(|o| {
            let mut v = who(&world.objs[o.owner]);
            v["depth"] = json!(r2(o.depth));
            v["push"] = p3(o.push);
            v["at"] = p3(o.point);
            if o.contained {
                v["inside"] = json!(true);
            }
            v
        })
        .collect()
}

fn contact_json(world: &World, s: &Settle) -> Value {
    match &s.contact {
        Some((owner, c)) => {
            let mut v = who(&world.objs[*owner]);
            v["point"] = p3(c.point);
            v["normal"] = p3(c.normal);
            v
        }
        None => Value::Null,
    }
}

const SIDE_DIRS: [(&str, DVec3); 5] =
    [("+x", DVec3::X), ("-x", DVec3::NEG_X), ("+y", DVec3::Y), ("-y", DVec3::NEG_Y), ("+z", DVec3::Z)];

fn check_unit(moving: &TriMesh, world: &World, skip: &[usize], tol: f64, reach: f64) -> Value {
    let below = place_ops::settle(moving, world, DVec3::NEG_Z, 4096.0, skip, tol);
    let mut clear = serde_json::Map::new();
    let mut touching: Vec<&str> = Vec::new();
    for (name, d) in SIDE_DIRS {
        match place_ops::clearance(moving, world, d, reach, skip) {
            Some((t, _)) => {
                if t <= tol {
                    touching.push(name);
                }
                clear.insert(name.to_owned(), json!(r2(t)));
            }
            None => {
                clear.insert(name.to_owned(), Value::Null);
            }
        }
    }
    let ov = place_ops::overlaps(moving, world, skip, tol.max(0.01));
    let gap = below.delta;
    let status = match gap {
        Some(g) if g < -tol => "sunk",
        Some(g) if g <= tol => "resting",
        _ if !touching.is_empty() => "attached",
        Some(_) => "floating",
        None => "no_surface",
    };
    if gap.is_some_and(|g| g.abs() <= tol) {
        touching.insert(0, "-z");
    }
    let mut v = json!({
        "status": status,
        "gap_below": gap.map(r2),
        "below": contact_json(world, &below),
        "clearance": clear,
        "touching": touching,
        "overlaps": overlaps_json(world, &ov),
    });
    if let Some(k) = below.lifted_by.filter(|_| below.lift > tol) {
        v["sunk_into"] = who(&world.objs[k]);
    }
    if !below.ignored.is_empty() {
        v["ignored_below"] = json!(below.ignored.iter().map(|i| who(&world.objs[*i])).collect::<Vec<_>>());
    }
    v
}

fn check_placement(state: &mut AppState, a: &Args<'_>) -> R<Outcome> {
    let include_tools = a.opt_bool("include_tools")?.unwrap_or(false);
    let tol = a.opt_f64("tolerance", 0.0, 16.0)?.unwrap_or(0.05);
    let reach = a.opt_f64("clearance", 1.0, 4096.0)?.unwrap_or(128.0);
    let choice = mesh_choice(a)?;
    let units = units_of(state, a, include_tools, false)?;
    if units.is_empty() {
        return Err("nothing to check: give ids, solid_ids or group_ids, or select objects".to_owned());
    }
    if units.len() > 64 {
        return Err("at most 64 objects per call".to_owned());
    }
    let jobs: Vec<(Obj, Vec<Obj>)> = units
        .into_iter()
        .map(|u| {
            let region = region_around(&u.obj.hint, DVec3::NEG_Z, 4096.0).union(&u.obj.hint.expand(reach + 64.0));
            let env = env_for(state, &u.refs, Some(region), include_tools, &[]);
            (u.obj, env)
        })
        .collect();
    let fs = state.assets.fs().cloned();
    Ok(Worker::spawn("check_placement", move || {
        let fs = fs.as_ref();
        let mut out = Vec::new();
        for (obj, env) in jobs {
            let r = pw::resolve(&obj, fs, choice);
            let moving = r.compound();
            let world = World::build(env, fs, choice, None);
            let mut v = who(&obj);
            let mut c = check_unit(&moving, &world, &[], tol, reach);
            if let Some(m) = c.as_object_mut() {
                for (k, x) in std::mem::take(m) {
                    v[k] = x;
                }
            }
            if !r.notes.is_empty() {
                v["notes"] = json!(r.notes);
            }
            out.push(v);
        }
        compact(json!({ "units": UNITS, "tolerance": tol, "objects": out }))
    }))
}

#[derive(Clone, Debug)]
struct PlaceSpec {
    mode: String,
    position: Option<DVec3>,
    anchor: Option<String>,
    rotate_yaw: f64,
    side: String,
    align: String,
    direction: Option<DVec3>,
    facing: String,
    align_to_surface: bool,
    then_drop: Option<bool>,
    gap: f64,
    max: f64,
    choice: MeshChoice,
    pre: DMat4,
    fit: Fit,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
enum Fit {
    Normal,
    Supports,
}

fn about(pivot: DVec3, rot: DQuat) -> DMat4 {
    DMat4::from_translation(pivot) * DMat4::from_quat(rot) * DMat4::from_translation(-pivot)
}

struct Placed {
    xf: DMat4,
    report: Value,
}

struct Scene<'a> {
    world: &'a World,
    spec: &'a PlaceSpec,
    skip: &'a [usize],
}

fn settle_into(
    moving: &TriMesh,
    xf: &mut DMat4,
    sc: &Scene<'_>,
    dir: DVec3,
    report: &mut serde_json::Map<String, Value>,
    label: &str,
) -> R<Settle> {
    let (world, spec, skip) = (sc.world, sc.spec, sc.skip);
    let cur = moving.transformed(xf);
    let s = place_ops::settle(&cur, world, dir, spec.max, skip, 0.01);
    let Some(t) = s.delta else {
        return Err(format!("{label}: nothing to touch within {} units along {:?}", spec.max, p3(dir)));
    };
    let t = place_ops::round_back(t - spec.gap);
    *xf = DMat4::from_translation(dir * t) * *xf;
    report.insert(format!("{label}_contact"), contact_json(world, &s));
    report.insert(format!("{label}_distance"), json!(r2(t)));
    if s.lift > 0.01 {
        report.insert(format!("{label}_lifted_out"), json!(r2(s.lift)));
    }
    if !s.ignored.is_empty() {
        report.insert(
            format!("{label}_ignored_overlapping"),
            json!(s.ignored.iter().map(|i| who(&world.objs[*i])).collect::<Vec<_>>()),
        );
    }
    Ok(s)
}

fn side_dir(side: &str) -> DVec3 {
    match side {
        "-x" => DVec3::NEG_X,
        "+y" => DVec3::Y,
        "-y" => DVec3::NEG_Y,
        "+z" => DVec3::Z,
        "-z" => DVec3::NEG_Z,
        _ => DVec3::X,
    }
}

fn compute_place(
    obj: &Obj,
    moving: &TriMesh,
    world: &World,
    target: Option<usize>,
    spec: &PlaceSpec,
    skip: &[usize],
) -> R<Placed> {
    let mut xf = spec.pre;
    let mut rep = serde_json::Map::new();
    let b0 = moving.bounds();
    if b0.is_empty() {
        return Err("the object has no geometry".to_owned());
    }
    if spec.rotate_yaw.abs() > 1e-9 {
        let pivot = obj.origin.unwrap_or(b0.center());
        xf = about(pivot, DQuat::from_rotation_z(spec.rotate_yaw.to_radians())) * xf;
    }
    if let Some(p) = spec.position {
        let cur = moving.transformed(&xf).bounds();
        let bottom = DVec3::new(cur.center().x, cur.center().y, cur.min.z);
        let anchor = match (spec.anchor.as_deref(), obj.origin) {
            (Some("center"), _) => cur.center(),
            (Some("bottom_center"), _) | (_, None) => bottom,
            (_, Some(o)) => xf.transform_point3(o),
        };
        let mut d = p - anchor;
        if spec.mode == "on" {
            d.z = 0.0;
        }
        xf = DMat4::from_translation(d) * xf;
    }
    let tb = target.map(|t| world.resolved[t].bounds());
    match spec.mode.as_str() {
        "at" => {}
        "drop" => {
            let dir = spec.direction.unwrap_or(DVec3::NEG_Z);
            let s = settle_into(moving, &mut xf, &Scene { world, spec, skip }, dir, &mut rep, "drop")?;
            if (spec.align_to_surface || spec.fit == Fit::Supports)
                && let Some((_, c)) = s.contact
            {
                if dir.z < -0.999 {
                    rest_on_surface(moving, &mut xf, world, spec, skip, &mut rep, c.normal)?;
                } else {
                    let cur = moving.transformed(&xf);
                    let rot = DQuat::from_rotation_arc(-dir, c.normal.normalize());
                    if rot.angle_between(DQuat::IDENTITY) > 1e-4 {
                        let pivot = DVec3::new(cur.bounds().center().x, cur.bounds().center().y, c.point.z);
                        xf = about(pivot, rot) * xf;
                        xf = DMat4::from_translation(-dir * 2.0) * xf;
                        settle_into(moving, &mut xf, &Scene { world, spec, skip }, dir, &mut rep, "aligned")?;
                    }
                }
            }
            if spec.fit == Fit::Supports && dir.z < -0.999 {
                fit_supports(moving, &mut xf, &Scene { world, spec, skip }, &mut rep)?;
            }
        }
        "on" => {
            let t = target.ok_or("on needs target")?;
            let tb = tb.unwrap_or(Aabb::EMPTY);
            let cur = moving.transformed(&xf).bounds();
            let xy = spec.position.map_or(tb.center(), |p| DVec3::new(p.x, p.y, 0.0));
            let lift = tb.max.z + 1.0 - cur.min.z;
            let d = if spec.position.is_some() {
                DVec3::new(0.0, 0.0, lift)
            } else {
                DVec3::new(xy.x - cur.center().x, xy.y - cur.center().y, lift)
            };
            xf = DMat4::from_translation(d) * xf;
            let s = settle_into(moving, &mut xf, &Scene { world, spec, skip }, DVec3::NEG_Z, &mut rep, "drop")?;
            if s.contact.as_ref().is_some_and(|(o, _)| *o != t) {
                rep.insert("warning".to_owned(), json!("it landed on another object than target (see drop_contact)"));
            }
            if (spec.align_to_surface || spec.fit == Fit::Supports)
                && let Some((_, c)) = s.contact
            {
                rest_on_surface(moving, &mut xf, world, spec, skip, &mut rep, c.normal)?;
            }
            if spec.fit == Fit::Supports {
                fit_supports(moving, &mut xf, &Scene { world, spec, skip }, &mut rep)?;
            }
        }
        "against" => {
            let dir = spec.direction.ok_or("against needs direction (toward the wall)")?;
            let flat = DVec3::new(dir.x, dir.y, 0.0).try_normalize();
            if spec.facing != "keep"
                && let Some(flat) = flat
            {
                let want = if spec.facing == "away" { -flat } else { flat };
                let fwd = DMat3::from_mat4(xf) * obj.rotation().x_axis;
                let have = DVec3::new(fwd.x, fwd.y, 0.0).try_normalize().unwrap_or(DVec3::X);
                let yaw = have.y.atan2(have.x);
                let goal = want.y.atan2(want.x);
                let pivot =
                    obj.origin.map_or_else(|| moving.transformed(&xf).bounds().center(), |o| xf.transform_point3(o));
                xf = about(pivot, DQuat::from_rotation_z(goal - yaw)) * xf;
            }
            settle_into(moving, &mut xf, &Scene { world, spec, skip }, dir, &mut rep, "against")?;
            if spec.then_drop.unwrap_or(false) {
                settle_into(moving, &mut xf, &Scene { world, spec, skip }, DVec3::NEG_Z, &mut rep, "drop")?;
            }
        }
        "next_to" => {
            let t = target.ok_or("next_to needs target")?;
            let tb = tb.unwrap_or(Aabb::EMPTY);
            let sd = side_dir(&spec.side);
            let cur = moving.transformed(&xf).bounds();
            let k = sd.abs().max_position();
            let mut d = DVec3::ZERO;
            for axis in 0..3 {
                if axis == k {
                    continue;
                }
                if axis == 2 && k != 2 {
                    d[2] = tb.min.z - cur.min.z;
                    continue;
                }
                d[axis] = match spec.align.as_str() {
                    "min" => tb.min[axis] - cur.min[axis],
                    "max" => tb.max[axis] - cur.max[axis],
                    _ => tb.center()[axis] - cur.center()[axis],
                };
            }
            let margin = 4.0 + spec.gap;
            d[k] = if sd[k] > 0.0 { tb.max[k] + margin - cur.min[k] } else { tb.min[k] - margin - cur.max[k] };
            xf = DMat4::from_translation(d) * xf;
            if k != 2 && spec.then_drop.unwrap_or(true) {
                let up = DMat4::from_translation(DVec3::Z * 1.0);
                xf = up * xf;
                if settle_into(moving, &mut xf, &Scene { world, spec, skip }, DVec3::NEG_Z, &mut rep, "drop").is_err() {
                    xf = DMat4::from_translation(DVec3::NEG_Z) * xf;
                }
            }
            let s = settle_into(moving, &mut xf, &Scene { world, spec, skip }, -sd, &mut rep, "next_to")?;
            if s.contact.as_ref().is_some_and(|(o, _)| *o != t) {
                rep.insert(
                    "warning".to_owned(),
                    json!("something else is between the object and target (see next_to_contact)"),
                );
            }
        }
        other => return Err(format!("unknown mode {other}")),
    }
    Ok(Placed { xf, report: Value::Object(rep) })
}

fn verify(moving: &TriMesh, world: &World) -> Value {
    let below = place_ops::settle(moving, world, DVec3::NEG_Z, 4096.0, &[], 0.01);
    let ov = place_ops::overlaps(moving, world, &[], 0.02);
    json!({
        "gap_below": below.delta.map(r2),
        "below": contact_json(world, &below),
        "overlaps": overlaps_json(world, &ov),
        "ok": ov.is_empty(),
    })
}

fn place_object(state: &mut AppState, a: &Args<'_>) -> R<Outcome> {
    let include_tools = a.opt_bool("include_tools")?.unwrap_or(false);
    let mode = a.opt_enum("mode", &["drop", "on", "against", "next_to", "at"])?.unwrap_or_else(|| "drop".to_owned());
    let mut spec = PlaceSpec {
        position: a.opt_vec3("position")?,
        anchor: a.opt_enum("anchor", &["origin", "bottom_center", "center"])?,
        rotate_yaw: a.opt_f64("rotate_yaw", -360.0, 360.0)?.unwrap_or(0.0),
        side: a.opt_enum("side", SIDES)?.unwrap_or_else(|| "+x".to_owned()),
        align: a.opt_enum("align", &["center", "min", "max"])?.unwrap_or_else(|| "center".to_owned()),
        direction: opt_dir(a, "direction")?,
        facing: a.opt_enum("facing", &["keep", "away", "toward"])?.unwrap_or_else(|| "keep".to_owned()),
        align_to_surface: a.opt_bool("align_to_surface")?.unwrap_or(false),
        then_drop: a.opt_bool("then_drop")?,
        gap: a.opt_f64("gap", 0.0, 256.0)?.unwrap_or(0.0),
        max: a.opt_f64("max_distance", 1.0, 65536.0)?.unwrap_or(4096.0),
        choice: mesh_choice(a)?,
        pre: DMat4::IDENTITY,
        fit: if a.opt_enum("fit", &["normal", "supports"])?.as_deref() == Some("supports") {
            Fit::Supports
        } else {
            Fit::Normal
        },
        mode,
    };
    if spec.mode == "against" && spec.direction.is_none() {
        return Err("against needs direction (toward the wall), for example [0, 1, 0]".to_owned());
    }
    if spec.mode == "at" && spec.position.is_none() && a.get("angles").is_none() && spec.rotate_yaw == 0.0 {
        return Err("at needs position, angles or rotate_yaw".to_owned());
    }
    let dry = a.opt_bool("dry_run")?.unwrap_or(false);
    let ignore = a.ids("ignore_ids")?;
    let angles = angles_arg(a, "angles")?;
    let created = model_arg(a)?;
    let (obj, refs, new_entity) = match &created {
        Some(path) => {
            if a.get("ids").is_some() || a.get("solid_ids").is_some() || a.get("group_ids").is_some() {
                return Err("give model (to create) or ids (to move), not both".to_owned());
            }
            let class = a.opt_str("classname", 63)?.unwrap_or("prop_static").trim().to_owned();
            check_class(&class)?;
            let mut kv = a.keyvalues("keyvalues")?;
            kv.retain(|(k, _)| !["classname", "origin", "angles", "model"].iter().any(|x| k.eq_ignore_ascii_case(x)));
            let probe = hp_vmf::Entity { classname: class.clone(), props: kv.clone(), ..hp_vmf::Entity::default() };
            let scale = pw::entity_scale(&probe);
            let body = probe.get("body").and_then(|s| s.trim().parse::<u32>().ok()).unwrap_or(0);
            let origin = spec.position.ok_or("a new model needs position")?;
            let ang = angles.unwrap_or(DVec3::ZERO);
            let mut o = model_obj(path, origin, ang, scale, body, &class);
            o.origin = Some(origin);
            (o, Vec::new(), Some((class, kv, path.clone())))
        }
        None => {
            for k in ["classname", "keyvalues"] {
                if a.get(k).is_some() {
                    return Err(format!("{k} only goes with model"));
                }
            }
            let units = units_of(state, a, include_tools, false)?;
            if units.is_empty() {
                return Err("give ids, solid_ids, group_ids or model, or select objects".to_owned());
            }
            let refs: Vec<ObjRef> = units.iter().flat_map(|u| u.refs.clone()).collect();
            let obj = if units.len() == 1 {
                units.into_iter().next().map(|u| u.obj).ok_or("no object")?
            } else {
                pw::compound(state, Key::Group(0), &refs, include_tools).ok_or("no object")?
            };
            if let Some(ang) = angles {
                if obj.models.len() != 1 || obj.origin.is_none() || !obj.meshes.is_empty() {
                    return Err("angles only goes with one entity that has a model; use rotate_yaw for other objects"
                        .to_owned());
                }
                let o = obj.origin.unwrap_or_default();
                let delta = hp_geom::angles_to_quat(ang) * hp_geom::angles_to_quat(obj.angles).inverse();
                spec.pre = about(o, delta.normalize());
            }
            (obj, refs, None)
        }
    };
    let target = match a.opt_u64("target", 1, u64::from(u32::MAX))? {
        Some(t) => {
            let t = t as u32;
            let scene = state.doc.scene();
            let key = if scene.entity(t).is_some() {
                Key::Entity(t)
            } else if scene.solid(t).is_some() {
                Key::Solid(t)
            } else {
                return Err(format!("no entity or solid has the id {t}"));
            };
            if refs.iter().any(|r| r.id() == t) {
                return Err("target is one of the moved objects".to_owned());
            }
            Some(key)
        }
        None if matches!(spec.mode.as_str(), "on" | "next_to") => return Err(format!("{} needs target", spec.mode)),
        None => None,
    };
    let dir = match spec.mode.as_str() {
        "against" => spec.direction.unwrap_or(DVec3::X),
        "next_to" => -side_dir(&spec.side),
        _ => spec.direction.unwrap_or(DVec3::NEG_Z),
    };
    let mut hint = obj.hint;
    if let Some(p) = spec.position {
        hint = hint.union(&Aabb::new(p, p).expand(hint.size().max_element().max(64.0)));
    }
    let mut region = region_around(&hint, dir, spec.max).union(&region_around(&hint, DVec3::NEG_Z, spec.max));
    if let Some(t) = target {
        let tb = match t {
            Key::Entity(id) => pw::entity_obj(state, id, include_tools).map(|o| o.hint),
            Key::Solid(id) => pw::solid_obj(state, id, include_tools).map(|o| o.hint),
            Key::Group(_) => None,
        };
        if let Some(tb) = tb {
            region = region.union(&tb.expand(hint.size().max_element() * 2.0 + 64.0));
            region = region.union(&region_around(&tb.expand(hint.size().max_element() + 16.0), DVec3::NEG_Z, spec.max));
        }
    }
    let mut env = env_for(state, &refs, Some(region), include_tools, &ignore);
    if let Some(t) = target
        && !env.iter().any(|o| o.key == t)
    {
        let extra = match t {
            Key::Entity(id) => pw::entity_obj(state, id, include_tools),
            Key::Solid(id) => pw::solid_obj(state, id, include_tools),
            Key::Group(_) => None,
        };
        env.extend(extra);
    }
    let fs = state.assets.fs().cloned();
    let revision = state.doc.revision();
    Ok(Worker::spawn_then("place_object", move || {
        let fs = fs.as_ref();
        let r = pw::resolve(&obj, fs, spec.choice);
        if new_entity.is_some() && r.shapes.is_empty() {
            return (ToolResult::error(format!("place_object: {}", r.notes.join("; "))), None);
        }
        let moving = r.compound();
        let world = World::build(env, fs, spec.choice, None);
        let tidx = target.and_then(|t| world.objs.iter().position(|o| o.key == t));
        if target.is_some() && tidx.is_none() {
            return (ToolResult::error("place_object: target has no geometry"), None);
        }
        let placed = match compute_place(&obj, &moving, &world, tidx, &spec, &[]) {
            Ok(p) => p,
            Err(e) => return (ToolResult::error(format!("place_object: {e}")), None),
        };
        let fin = moving.transformed(&placed.xf);
        let mut report = placed.report;
        report["mode"] = json!(spec.mode);
        report["check"] = verify(&fin, &world);
        let fb = fin.bounds();
        report["bounds"] = json!({ "min": p3(fb.min), "max": p3(fb.max), "size": p3(fb.size()) });
        let lin = DMat3::from_mat4(placed.xf);
        let new_origin = obj.origin.map(|o| placed.xf.transform_point3(o));
        let new_angles = obj.origin.map(|_| {
            let q = DQuat::from_mat3(&lin).normalize();
            hp_geom::clean_angles(hp_geom::transform_angles(obj.angles, q))
        });
        if let Some(o) = new_origin {
            report["origin"] = p3(o);
        }
        if let Some(ang) = new_angles {
            report["angles"] = p3(ang);
        }
        if !r.notes.is_empty() {
            report["notes"] = json!(r.notes);
        }
        if dry {
            report["dry_run"] = json!(true);
            return (compact(report), None);
        }
        let xf = placed.xf;
        let apply: super::tools::ApplyFn = Box::new(move |state: &mut AppState| {
            if state.doc.revision() != revision {
                return Some(ToolResult::error("place_object: the map changed while computing; call again"));
            }
            let mut report = report;
            let res = match &new_entity {
                Some((class, kv, path)) => {
                    let mut kv = kv.clone();
                    kv.push(("model".to_owned(), path.clone()));
                    let ang = new_angles.unwrap_or_default();
                    kv.push(("angles".to_owned(), hp_vmf::fmt_vec3(ang)));
                    let origin = new_origin.unwrap_or_default();
                    super::tools::edit(state, "place_object", |doc| doc.create_point_entity(class, origin, &kv)).map(
                        |id| {
                            report["id"] = json!(id);
                            report["created"] = json!(true);
                        },
                    )
                }
                None => {
                    let rigid = DMat3::from_mat4(xf).abs_diff_eq(DMat3::IDENTITY, 1e-12);
                    let t = xf.w_axis.truncate();
                    report["moved"] = p3(t);
                    super::tools::edit(state, "place_object", |doc| {
                        if rigid { doc.translate(&refs, t) } else { doc.transform(&refs, xf) }
                    })
                }
            };
            Some(match res {
                Ok(()) => compact(report),
                Err(e) => ToolResult::error(format!("place_object: {e}")),
            })
        });
        (ToolResult::error("place_object: not applied"), Some(apply))
    }))
}

fn find_floating_objects(state: &mut AppState, a: &Args<'_>) -> R<Outcome> {
    let region = match (a.opt_vec3("min")?, a.opt_vec3("max")?) {
        (Some(lo), Some(hi)) => Some(Aabb::new(lo, hi)),
        (None, None) => None,
        _ => return Err("give both min and max".to_owned()),
    };
    let include = a.opt_enum("include", &["props", "entities", "all"])?.unwrap_or_else(|| "props".to_owned());
    let tol = a.opt_f64("tolerance", 0.0, 64.0)?.unwrap_or(0.5);
    let choice = mesh_choice(a)?;
    let fix = a.opt_bool("fix")?.unwrap_or(false);
    let fix_sunk = a.opt_bool("fix_sunk")?.unwrap_or(false);
    let max_sink = a.opt_f64("max_sink", 0.0, 1024.0)?.unwrap_or(16.0);
    let max_drop = a.opt_f64("max_drop", 1.0, 65536.0)?.unwrap_or(4096.0);
    let limit = a.opt_u64("limit", 1, hp_mcp::tools::MAX_LIST)?.unwrap_or(100) as usize;
    let only = a.ids("ids")?;
    let scene = state.doc.scene();
    for id in &only {
        if scene.entity(*id).is_none() && scene.solid(*id).is_none() {
            return Err(format!("no entity or solid has the id {id}"));
        }
    }
    let env_region = region.map(|r| Aabb::new(r.min - DVec3::new(0.0, 0.0, max_drop), r.max).expand(64.0));
    let all = pw::environment(
        state,
        &Default::default(),
        &Default::default(),
        &EnvOptions { include_tools: false, include_points: false, region: env_region },
    );
    let cand_keys: Vec<Key> = all
        .iter()
        .filter(|o| {
            if !only.is_empty() {
                return only.contains(&o.key.id());
            }
            match include.as_str() {
                "props" => !o.models.is_empty() && o.meshes.is_empty(),
                "entities" => matches!(o.key, Key::Entity(_)),
                _ => true,
            }
        })
        .map(|o| o.key)
        .collect();
    let fs = state.assets.fs().cloned();
    let revision = state.doc.revision();
    Ok(Worker::spawn_then("find_floating_objects", move || {
        let fs = fs.as_ref();
        let mut world = World::build(all, fs, choice, None);
        let mut cands: Vec<usize> = cand_keys
            .iter()
            .filter_map(|k| world.objs.iter().position(|o| o.key == *k))
            .filter(|i| region.is_none_or(|r| world.resolved[*i].bounds().intersects(&r)))
            .collect();
        cands.sort_by(|x, y| world.resolved[*x].bounds().min.z.total_cmp(&world.resolved[*y].bounds().min.z));
        let (mut floating, mut sunk, mut nothing, mut fixed) = (Vec::new(), Vec::new(), Vec::new(), Vec::new());
        let mut attached = 0usize;
        let mut moves: Vec<(Vec<ObjRef>, DVec3)> = Vec::new();
        for i in &cands {
            let moving = world.resolved[*i].compound();
            let s = place_ops::settle(&moving, &world, DVec3::NEG_Z, max_drop, &[*i], tol.max(0.01));
            let touching = || {
                SIDE_DIRS.iter().any(|(_, d)| place_ops::clearance(&moving, &world, *d, tol + 0.05, &[*i]).is_some())
            };
            let mut item = who(&world.objs[*i]);
            let fix_by = match s.delta {
                Some(g) if g > tol => {
                    if touching() {
                        attached += 1;
                        None
                    } else {
                        item["gap"] = json!(r2(g));
                        item["below"] = contact_json(&world, &s);
                        floating.push(item);
                        fix.then_some(g)
                    }
                }
                Some(g) if g < -tol => {
                    item["depth"] = json!(r2(-g));
                    if let Some(k) = s.lifted_by {
                        item["into"] = who(&world.objs[k]);
                    }
                    sunk.push(item);
                    (fix_sunk && -g <= max_sink).then_some(g)
                }
                Some(_) => None,
                None => {
                    if touching() {
                        attached += 1;
                    } else {
                        nothing.push(item);
                    }
                    None
                }
            };
            if let Some(g) = fix_by {
                let t = place_ops::round_back(g);
                let d = DVec3::new(0.0, 0.0, -t);
                for p in &mut world.resolved[*i].pieces {
                    *p = p.translated(d);
                }
                moves.push((world.objs[*i].refs.clone(), d));
                let mut f = who(&world.objs[*i]);
                f["moved_z"] = json!(r2(-t));
                fixed.push(f);
            }
        }
        let report = json!({
            "units": UNITS,
            "checked": cands.len(),
            "tolerance": tol,
            "floating_total": floating.len(),
            "sunk_total": sunk.len(),
            "no_surface_total": nothing.len(),
            "attached": attached,
            "floating": floating.into_iter().take(limit).collect::<Vec<_>>(),
            "sunk": sunk.into_iter().take(limit).collect::<Vec<_>>(),
            "no_surface": nothing.into_iter().take(limit).collect::<Vec<_>>(),
            "fixed_total": fixed.len(),
            "fixed": fixed.into_iter().take(limit).collect::<Vec<_>>(),
        });
        if moves.is_empty() {
            return (compact(report), None);
        }
        let apply: super::tools::ApplyFn = Box::new(move |state: &mut AppState| {
            if state.doc.revision() != revision {
                return Some(ToolResult::error("find_floating_objects: the map changed while computing; call again"));
            }
            let res = super::tools::edit(state, "find_floating_objects", |doc| {
                for (refs, d) in &moves {
                    doc.translate(refs, *d)?;
                }
                Ok(())
            });
            Some(match res {
                Ok(()) => compact(report),
                Err(e) => ToolResult::error(format!("find_floating_objects: {e}")),
            })
        });
        (ToolResult::error("find_floating_objects: not applied"), Some(apply))
    }))
}

const COLORS: &[(&str, [u8; 3])] = &[
    ("prop", [214, 150, 92]),
    ("model_entity", [214, 150, 92]),
    ("solid", [130, 150, 178]),
    ("brush_entity", [120, 170, 210]),
    ("displacement", [120, 178, 110]),
    ("overlay", [222, 206, 96]),
    ("point_entity", [196, 120, 220]),
    ("instance", [150, 200, 220]),
    ("group", [200, 160, 120]),
];

fn color_of(kind: &str) -> [u8; 3] {
    COLORS.iter().find(|(k, _)| *k == kind).map_or([180, 180, 180], |(_, c)| *c)
}

fn render_shape(state: &mut AppState, a: &Args<'_>) -> R<Outcome> {
    let include_tools = a.opt_bool("include_tools")?.unwrap_or(false);
    let mesh = a.opt_enum("mesh", &["render", "collision", "both"])?.unwrap_or_else(|| "render".to_owned());
    let mut views = Vec::new();
    for v in a.strings("views", 7)? {
        let v = v.trim().to_ascii_lowercase();
        let k = hp_shape::ViewKind::parse(&v)
            .ok_or_else(|| format!("unknown view {v} (expected: {})", VIEWS.join(", ")))?;
        if !views.contains(&k) {
            views.push(k);
        }
    }
    if views.is_empty() {
        views =
            vec![hp_shape::ViewKind::Top, hp_shape::ViewKind::Front, hp_shape::ViewKind::Side, hp_shape::ViewKind::Iso];
    }
    let width = a.opt_u64("width", 256, 2048)?.unwrap_or(1024) as u32;
    let height = a.opt_u64("height", 256, 2048)?.unwrap_or(768) as u32;
    let markers = a.opt_bool("markers")?.unwrap_or(true);
    let dims = a.opt_bool("dimensions")?.unwrap_or(true);
    let grid = a.opt_bool("grid")?.unwrap_or(true);
    let angles = angles_arg(a, "angles")?.unwrap_or(DVec3::ZERO);
    let scale = a.opt_f64("scale", 0.01, 100.0)?.unwrap_or(1.0);
    let body = a.opt_u64("body", 0, 65535)?.unwrap_or(0) as u32;
    let region = match (a.opt_vec3("min")?, a.opt_vec3("max")?) {
        (Some(lo), Some(hi)) => Some(Aabb::new(lo, hi)),
        (None, None) => None,
        _ => return Err("give both min and max".to_owned()),
    };
    let (objs, label) = match (model_arg(a)?, region) {
        (Some(path), None) => {
            (vec![model_obj(&path, DVec3::ZERO, angles, scale, body, "prop_static")], path.to_ascii_uppercase())
        }
        (None, Some(r)) => {
            if r.size().min_element() <= 0.0 {
                return Err("the box is empty".to_owned());
            }
            let mut objs = pw::environment(
                state,
                &Default::default(),
                &Default::default(),
                &EnvOptions { include_tools, include_points: true, region: Some(r) },
            );
            for (id, n) in state.doc.scene().entities() {
                let overlay = n.entity.classname.to_ascii_lowercase().starts_with("info_overlay");
                if overlay
                    && let Some(o) = pw::entity_obj(state, *id, include_tools)
                    && o.hint.intersects(&r)
                {
                    objs.push(o);
                }
            }
            if objs.is_empty() {
                return Err("nothing in the box".to_owned());
            }
            (objs, format!("BOX {} TO {}", fmt_v(r.min), fmt_v(r.max)))
        }
        (Some(_), Some(_)) => return Err("give model or min and max, not both".to_owned()),
        (None, None) => {
            let objs = objects_of(state, a, include_tools)?;
            let label = if objs.len() == 1 {
                let o = &objs[0];
                format!("{} #{}", o.model_path().map_or(o.class.clone(), str::to_owned), o.key.id())
                    .to_ascii_uppercase()
            } else {
                format!("{} OBJECTS", objs.len())
            };
            (objs, label)
        }
    };
    let fs = state.assets.fs().cloned();
    let clip = region;
    Ok(Worker::spawn("render_shape", move || {
        let fs = fs.as_ref();
        let mut layers = Vec::new();
        let mut marks = Vec::new();
        let mut total = Aabb::EMPTY;
        let mut tris = 0usize;
        let mut notes: Vec<String> = Vec::new();
        for o in &objs {
            let main = if mesh == "collision" { MeshChoice::Collision } else { MeshChoice::Render };
            let r = pw::resolve(o, fs, main);
            notes.extend(r.notes.iter().cloned());
            let m = r.compound();
            let b = m.bounds();
            if b.is_empty() {
                continue;
            }
            tris += m.tris.len();
            if tris > 3_000_000 {
                notes.push("too many triangles: some objects are not drawn".to_owned());
                break;
            }
            total = total.union(&b);
            if mesh == "both" && !o.models.is_empty() {
                let c = pw::resolve(o, fs, MeshChoice::Collision);
                if c.shapes.iter().any(|(s, _)| !s.collision.is_empty()) {
                    layers.push(hp_shape::Layer { mesh: c.compound(), color: [80, 230, 240], wire: true });
                }
            }
            layers.push(hp_shape::Layer { mesh: m, color: color_of(o.kind), wire: false });
            if markers && clip.is_none() && objs.len() <= 8 {
                if let Some(origin) = o.origin {
                    marks.push(hp_shape::Marker { pos: origin, color: [255, 70, 70], label: "ORIGIN".to_owned() });
                }
                let low = layers
                    .last()
                    .map(|l| &l.mesh)
                    .and_then(|m| m.used_vertices().into_iter().min_by(|x, y| x.z.total_cmp(&y.z)));
                if let Some(p) = low {
                    marks.push(hp_shape::Marker {
                        pos: p,
                        color: [90, 230, 110],
                        label: format!("LOW Z {}", fmt_n(p.z)),
                    });
                }
                for (s, mx) in &r.shapes {
                    for att in s.attachments.iter().take(8) {
                        marks.push(hp_shape::Marker {
                            pos: mx.transform_point3(att.pos),
                            color: [90, 210, 255],
                            label: att.name.to_ascii_uppercase(),
                        });
                    }
                }
            }
        }
        if layers.is_empty() {
            return ToolResult::error(format!("render_shape: nothing to draw ({})", notes.join("; ")));
        }
        let size = total.size();
        let title = format!("{label}   SIZE X {} Y {} Z {} UNITS", fmt_n(size.x), fmt_n(size.y), fmt_n(size.z));
        let img = hp_shape::render_sheet(
            &layers,
            &marks,
            &hp_shape::SheetOptions { width, height, views, dimensions: dims, grid, title, ground: Some(total.min.z) },
        );
        match encode_png(img.width, img.height, img.rgba) {
            Ok(png) => {
                let mut caption = format!(
                    "{label}: bounds min {} max {}, size {} x {} x {} units (Z up)",
                    fmt_v(total.min),
                    fmt_v(total.max),
                    fmt_n(size.x),
                    fmt_n(size.y),
                    fmt_n(size.z)
                );
                if !notes.is_empty() {
                    caption.push_str(&format!(". Notes: {}", notes.join("; ")));
                }
                ToolResult::png(&png, caption)
            }
            Err(e) => ToolResult::error(format!("render_shape: {e}")),
        }
    }))
}

fn fmt_n(x: f64) -> String {
    let r = r2(x);
    if (r - r.round()).abs() < 1e-9 { format!("{}", r.round() as i64) } else { format!("{r}") }
}

fn fmt_v(v: DVec3) -> String {
    format!("{} {} {}", fmt_n(v.x), fmt_n(v.y), fmt_n(v.z))
}

pub(super) fn exact_ray(state: &AppState) -> impl Fn(&hp_vmf::Entity, &hp_geom::Ray) -> hp_editor::EntityRay + '_ {
    move |e, ray| {
        let Some(path) = pw::model_of(state, e) else { return hp_editor::EntityRay::Bounds };
        let Ok(shape) = pw::model_shape(state.assets.fs(), &path, pw::entity_body(e)) else {
            return hp_editor::EntityRay::Bounds;
        };
        let o = e.origin().filter(|o| o.is_finite()).unwrap_or(DVec3::ZERO);
        let ang = e.angles().filter(|a| a.is_finite()).unwrap_or(DVec3::ZERO);
        let m = shape.render.transformed(&pw::model_matrix(o, ang, pw::entity_scale(e)));
        match hp_shape::ray_cast(&[&m], ray.origin, ray.dir, f64::INFINITY) {
            Some(h) => hp_editor::EntityRay::Hit { t: h.t, normal: h.normal },
            None => hp_editor::EntityRay::Miss,
        }
    }
}

pub(super) fn exact_bounds_fn(state: &AppState, exact: bool) -> impl Fn(&hp_vmf::Entity) -> Option<Aabb> + '_ {
    move |e| {
        let base = state.assets.entity_bounds(e);
        if !exact {
            return base;
        }
        let Some(path) = pw::model_of(state, e) else { return base };
        let Some(shape) = pw::cached_shape(state.assets.fs(), &path, pw::entity_body(e)) else { return base };
        let o = e.origin().filter(|o| o.is_finite()).unwrap_or(DVec3::ZERO);
        let ang = e.angles().filter(|a| a.is_finite()).unwrap_or(DVec3::ZERO);
        let b = shape.render.transformed(&pw::model_matrix(o, ang, pw::entity_scale(e))).bounds();
        if b.is_empty() { base } else { Some(base.map_or(b, |x| x.union(&b))) }
    }
}

fn env_keys(state: &AppState, refs: &[ObjRef]) -> Vec<Key> {
    let scene = state.doc.scene();
    let mut out = Vec::new();
    for r in refs {
        let k = match *r {
            ObjRef::Entity(id) => Key::Entity(id),
            ObjRef::Solid(id) => match scene.solid(id).map(|n| n.owner) {
                Some(hp_editor::Owner::Entity(e)) => Key::Entity(e),
                _ => Key::Solid(id),
            },
        };
        if !out.contains(&k) {
            out.push(k);
        }
    }
    out
}

struct DropPrep {
    env: Vec<Obj>,
    jobs: Vec<(Obj, Vec<ObjRef>, Vec<Key>)>,
    spec: PlaceSpec,
    dir: DVec3,
}

fn drop_spec(dir: DVec3, align: bool, gap: f64, max: f64, choice: MeshChoice) -> PlaceSpec {
    PlaceSpec {
        mode: "drop".to_owned(),
        position: None,
        anchor: None,
        rotate_yaw: 0.0,
        side: "+x".to_owned(),
        align: "center".to_owned(),
        direction: Some(dir),
        facing: "keep".to_owned(),
        align_to_surface: align,
        then_drop: None,
        gap,
        max,
        choice,
        pre: DMat4::IDENTITY,
        fit: Fit::Normal,
    }
}

fn drop_prepare(state: &AppState, units: Vec<Unit>, include_tools: bool, spec: PlaceSpec) -> DropPrep {
    let dir = spec.direction.unwrap_or(DVec3::NEG_Z);
    let mut region = Aabb::EMPTY;
    for u in &units {
        region = region.union(&region_around(&u.obj.hint, dir, spec.max));
    }
    let opts = EnvOptions { include_tools, include_points: false, region: Some(region) };
    let env = pw::environment(state, &Default::default(), &Default::default(), &opts);
    let jobs = units
        .into_iter()
        .map(|u| {
            let keys = env_keys(state, &u.refs);
            (u.obj, u.refs, keys)
        })
        .collect();
    DropPrep { env, jobs, spec, dir }
}

type Moves = Vec<(Vec<ObjRef>, DMat4)>;

fn drop_run(prep: DropPrep, fs: Option<&hp_fs::GameFs>) -> (Moves, Value) {
    let DropPrep { env, jobs, spec, dir } = prep;
    let mut world = World::build(env, fs, spec.choice, None);
    let mut order: Vec<(usize, f64)> = jobs
        .iter()
        .enumerate()
        .map(|(i, (o, _, _))| (i, pw::resolve(o, fs, spec.choice).bounds().center().dot(dir)))
        .collect();
    order.sort_by(|x, y| y.1.total_cmp(&x.1));
    let mut moves: Moves = Vec::new();
    let mut results = Vec::new();
    let mut stayed = Vec::new();
    for (i, _) in order {
        let (obj, refs, keys) = &jobs[i];
        let skip: Vec<usize> =
            world.objs.iter().enumerate().filter(|(_, o)| keys.contains(&o.key)).map(|(k, _)| k).collect();
        let r = pw::resolve(obj, fs, spec.choice);
        let moving = r.compound();
        let turnable = obj.origin.is_some() && !matches!(obj.kind, "brush_entity" | "displacement" | "group");
        let plain;
        let spec_i = if spec.align_to_surface && !turnable {
            plain = PlaceSpec { align_to_surface: false, ..spec.clone() };
            &plain
        } else {
            &spec
        };
        match compute_place(obj, &moving, &world, None, spec_i, &skip) {
            Ok(p) => {
                let mut v = who(obj);
                if let Some(m) = p.report.as_object() {
                    for (k, x) in m {
                        v[k.as_str()] = x.clone();
                    }
                }
                for k in &skip {
                    for piece in &mut world.resolved[*k].pieces {
                        *piece = piece.transformed(&p.xf);
                    }
                }
                if p.xf != DMat4::IDENTITY {
                    moves.push((refs.clone(), p.xf));
                }
                results.push(v);
            }
            Err(e) => {
                let mut v = who(obj);
                v["reason"] = json!(e);
                stayed.push(v);
            }
        }
    }
    let report = json!({
        "units": UNITS,
        "moved": moves.len(),
        "objects": jobs.len(),
        "results": results,
        "stayed": stayed,
    });
    (moves, report)
}

fn apply_moves(doc: &mut hp_editor::Document, moves: &Moves) -> hp_editor::EditResult<()> {
    for (refs, xf) in moves {
        if DMat3::from_mat4(*xf).abs_diff_eq(DMat3::IDENTITY, 1e-12) {
            doc.translate(refs, xf.w_axis.truncate())?;
        } else {
            doc.transform(refs, *xf)?;
        }
        for r in refs {
            let ObjRef::Entity(id) = *r else { continue };
            let Some(o) = doc.scene().entity(id).and_then(|e| e.entity.origin()) else { continue };
            let clean = o.map(|v| {
                let r = (v * 10000.0).round() / 10000.0;
                if r == 0.0 { 0.0 } else { r }
            });
            if clean != o && (clean - o).abs().max_element() < 1e-3 {
                doc.edit_entity(id, |e| e.set("origin", hp_vmf::fmt_vec3(clean)))?;
            }
        }
    }
    Ok(())
}

pub(crate) fn mesh_drop(
    state: &mut AppState,
    objs: &[ObjRef],
    dir: DVec3,
    align: bool,
    label: &str,
) -> hp_editor::EditResult<usize> {
    let units = units_from(state, Vec::new(), objs.to_vec(), false, true);
    if units.is_empty() {
        return Ok(0);
    }
    let prep = drop_prepare(state, units, false, drop_spec(dir, align, 0.0, 16384.0, MeshChoice::Render));
    let fs = state.assets.fs().cloned();
    let (moves, _) = drop_run(prep, fs.as_ref());
    if moves.is_empty() {
        return Ok(0);
    }
    let n = moves.len();
    if state.doc.in_transaction() {
        apply_moves(&mut state.doc, &moves)?;
    } else {
        state.doc.transaction(label.to_owned(), |doc| apply_moves(doc, &moves))?;
    }
    Ok(n)
}

pub(super) fn drop_to_surface(state: &mut AppState, a: &Args<'_>) -> R<Outcome> {
    let include_tools = a.opt_bool("include_tools")?.unwrap_or(false);
    let dir = opt_dir(a, "direction")?.unwrap_or(DVec3::NEG_Z);
    let spec = drop_spec(
        dir,
        a.opt_bool("align")?.unwrap_or(false),
        a.opt_f64("gap", 0.0, 256.0)?.unwrap_or(0.0),
        a.opt_f64("max_distance", 1.0, 65536.0)?.unwrap_or(16384.0),
        mesh_choice(a)?,
    );
    let units = units_of(state, a, include_tools, true)?;
    if units.is_empty() {
        return Err("give ids (or solid_ids), or select objects first".to_owned());
    }
    if units.len() > 512 {
        return Err("at most 512 objects per call".to_owned());
    }
    let prep = drop_prepare(state, units, include_tools, spec);
    let fs = state.assets.fs().cloned();
    let revision = state.doc.revision();
    Ok(Worker::spawn_then("drop_to_surface", move || {
        let (moves, report) = drop_run(prep, fs.as_ref());
        if moves.is_empty() {
            return (compact(report), None);
        }
        let apply: super::tools::ApplyFn = Box::new(move |state: &mut AppState| {
            if state.doc.revision() != revision {
                return Some(ToolResult::error("drop_to_surface: the map changed while computing; call again"));
            }
            Some(match super::tools::edit(state, "drop_to_surface", |doc| apply_moves(doc, &moves)) {
                Ok(()) => compact(report),
                Err(e) => ToolResult::error(format!("drop_to_surface: {e}")),
            })
        });
        (ToolResult::error("drop_to_surface: not applied"), Some(apply))
    }))
}

pub(super) fn with_model_shape(fs: Option<&hp_fs::GameFs>, path: &str, mut v: Value) -> Value {
    match pw::model_shape(fs, path, 0) {
        Ok(shape) => {
            let b = shape.render.bounds();
            let mut s = json!({
                "units": UNITS,
                "bounds": { "min": p3(b.min), "max": p3(b.max), "size": p3(b.size()) },
                "origin_above_bottom": r2(-b.min.z),
                "triangles": shape.render.tris.len(),
            });
            if let Some(rect) = hp_shape::min_area_rect(&hp_shape::footprint(&shape.render)) {
                s["footprint_rect"] = json!({ "center": p2(rect.center), "length": r2(rect.size.x), "width": r2(rect.size.y), "yaw": r2(rect.angle) });
            }
            let c = shape.collision_mesh().bounds();
            s["collision"] = if c.is_empty() {
                json!(null)
            } else {
                json!({ "min": p3(c.min), "max": p3(c.max), "size": p3(c.size()), "pieces": shape.collision.len() })
            };
            if !shape.attachments.is_empty() {
                s["attachments"] = json!(
                    shape
                        .attachments
                        .iter()
                        .take(32)
                        .map(|a| json!({ "name": a.name, "pos": p3(a.pos) }))
                        .collect::<Vec<_>>()
                );
            }
            v["shape"] = s;
        }
        Err(e) => v["shape_error"] = json!(e),
    }
    v
}

pub(super) fn obj_in_box(state: &AppState, o: Option<Obj>, b: &Aabb, inside: bool) -> bool {
    let Some(o) = o else { return false };
    let r = pw::resolve(&o, state.assets.fs(), MeshChoice::Render);
    if inside {
        let mb = r.bounds();
        return !mb.is_empty() && b.contains_aabb(&mb);
    }
    r.pieces.iter().any(|m| hp_shape::touches_box(m, b))
}

fn support_points(moving: &TriMesh, xf: &DMat4) -> Vec<DVec3> {
    let b = moving.bounds();
    let band = b.min.z + (b.size().z * 0.2).max(0.5);
    let c = b.center();
    let mut best: [Option<DVec3>; 4] = [None; 4];
    for v in moving.used_vertices() {
        if v.z > band {
            continue;
        }
        let q = usize::from(v.x >= c.x) + 2 * usize::from(v.y >= c.y);
        if best[q].is_none_or(|p| v.z < p.z) {
            best[q] = Some(v);
        }
    }
    best.into_iter().flatten().map(|p| xf.transform_point3(p)).collect()
}

fn rest_on_surface(
    moving: &TriMesh,
    xf: &mut DMat4,
    world: &World,
    spec: &PlaceSpec,
    skip: &[usize],
    rep: &mut serde_json::Map<String, Value>,
    fallback: DVec3,
) -> R<()> {
    let mut tilted = false;
    for _ in 0..4 {
        let cur = moving.transformed(xf);
        let b = cur.bounds();
        let env = place_ops::env_near(world, &Aabb::new(b.min - DVec3::new(0.0, 0.0, 512.0), b.max).expand(8.0), skip);
        let mut pts: Vec<DVec3> = Vec::new();
        for p in support_points(moving, xf) {
            if let Some(h) = hp_shape::ray_cast(&env.meshes, p + DVec3::Z * 0.5, DVec3::NEG_Z, 512.0) {
                pts.push(DVec3::new(p.x, p.y, p.z - h.point.z));
            }
        }
        let normal = if pts.len() >= 3 {
            let n = pts.len() as f64;
            let m = pts.iter().copied().sum::<DVec3>() / n;
            let (mut sxx, mut sxy, mut syy, mut sxz, mut syz) = (0.0, 0.0, 0.0, 0.0, 0.0);
            for p in &pts {
                let d = *p - m;
                sxx += d.x * d.x;
                sxy += d.x * d.y;
                syy += d.y * d.y;
                sxz += d.x * d.z;
                syz += d.y * d.z;
            }
            let det = sxx * syy - sxy * sxy;
            if det.abs() < 1e-9 {
                fallback
            } else {
                let a = (sxz * syy - syz * sxy) / det;
                let bb = (syz * sxx - sxz * sxy) / det;
                DVec3::new(a, bb, 1.0).normalize()
            }
        } else if tilted {
            break;
        } else {
            fallback
        };
        let rot = DQuat::from_rotation_arc(DVec3::Z, normal.normalize());
        if rot.angle_between(DQuat::IDENTITY) < 1e-4 {
            break;
        }
        let pivot = DVec3::new(b.center().x, b.center().y, b.min.z);
        *xf = DMat4::from_translation(DVec3::Z * 2.0) * about(pivot, rot) * *xf;
        settle_into(moving, xf, &Scene { world, spec, skip }, DVec3::NEG_Z, rep, "aligned")?;
        tilted = true;
    }
    if tilted {
        let cur = moving.transformed(xf);
        let b = cur.bounds();
        let env = place_ops::env_near(world, &Aabb::new(b.min - DVec3::new(0.0, 0.0, 512.0), b.max).expand(8.0), skip);
        let gaps: Vec<f64> = support_points(moving, xf)
            .into_iter()
            .filter_map(|p| {
                hp_shape::ray_cast(&env.meshes, p + DVec3::Z * 0.5, DVec3::NEG_Z, 512.0).map(|h| r2(p.z - h.point.z))
            })
            .collect();
        rep.insert("support_gaps".to_owned(), json!(gaps));
    }
    Ok(())
}

fn support_gaps(moving: &TriMesh, xf: &DMat4, world: &World, skip: &[usize]) -> Vec<(DVec3, f64)> {
    let b = moving.transformed(xf).bounds();
    let env = place_ops::env_near(world, &Aabb::new(b.min - DVec3::new(0.0, 0.0, 512.0), b.max).expand(8.0), skip);
    support_points(moving, xf)
        .into_iter()
        .filter_map(|p| {
            hp_shape::ray_cast(&env.meshes, p + DVec3::Z * 0.5, DVec3::NEG_Z, 512.0).map(|h| (p, p.z - h.point.z))
        })
        .collect()
}

fn fit_try(moving: &TriMesh, base: &DMat4, sc: &Scene<'_>, ax: f64, ay: f64) -> Option<(f64, DMat4)> {
    let b = moving.transformed(base).bounds();
    let pivot = DVec3::new(b.center().x, b.center().y, b.min.z);
    let rot = DQuat::from_rotation_x(ax.to_radians()) * DQuat::from_rotation_y(ay.to_radians());
    let mut x = DMat4::from_translation(DVec3::Z * 2.0) * about(pivot, rot) * *base;
    let mut scratch = serde_json::Map::new();
    settle_into(moving, &mut x, sc, DVec3::NEG_Z, &mut scratch, "fit").ok()?;
    let gaps = support_gaps(moving, &x, sc.world, sc.skip);
    if gaps.len() < 3 {
        return None;
    }
    Some((gaps.iter().map(|g| g.1).fold(0.0, f64::max), x))
}

fn fit_supports(moving: &TriMesh, xf: &mut DMat4, sc: &Scene<'_>, rep: &mut serde_json::Map<String, Value>) -> R<()> {
    let base = *xf;
    let Some(mut best) = fit_try(moving, &base, sc, 0.0, 0.0).map(|(g, x)| (g, x, 0.0, 0.0)) else {
        rep.insert("fit_note".to_owned(), json!("fewer than 3 low points find a surface below: no support fit"));
        return Ok(());
    };
    for step in [4.0, 1.0, 0.25] {
        let (cx, cy) = (best.2, best.3);
        for i in -2..=2 {
            for j in -2..=2 {
                let (ax, ay) = (cx + f64::from(i) * step, cy + f64::from(j) * step);
                if (i == 0 && j == 0) || ax.abs() > 30.0 || ay.abs() > 30.0 {
                    continue;
                }
                if let Some((g, x)) = fit_try(moving, &base, sc, ax, ay)
                    && g < best.0 - 1e-6
                {
                    best = (g, x, ax, ay);
                }
            }
        }
    }
    *xf = best.1;
    let cur = moving.transformed(xf);
    let gaps = support_gaps(moving, xf, sc.world, sc.skip);
    let s = place_ops::settle(&cur, sc.world, DVec3::NEG_Z, 64.0, sc.skip, 0.01);
    rep.insert("support_gaps".to_owned(), json!(gaps.iter().map(|g| r2(g.1)).collect::<Vec<_>>()));
    rep.insert("fit".to_owned(), json!({ "max_gap": r2(best.0), "tilt_x": r2(best.2), "tilt_y": r2(best.3) }));
    rep.insert("drop_contact".to_owned(), contact_json(sc.world, &s));
    if best.0 > 0.25 {
        let at = s.contact.map(|(_, c)| c.point);
        let near_support = at.is_some_and(|p| gaps.iter().any(|(q, _)| (q.truncate() - p.truncate()).length() < 4.0));
        let why = match at {
            Some(p) if !near_support => format!(
                "the lowest points cannot all touch: the object rests on the surface at {} between its supports \
                 (the ground bulges or rises under it); the best tilt leaves at most {} units under a support",
                fmt_v(p),
                fmt_n(best.0)
            ),
            _ => format!(
                "the lowest points cannot all touch the uneven surface; the best tilt leaves at most {} units under a support",
                fmt_n(best.0)
            ),
        };
        rep.insert("fit_note".to_owned(), json!(why));
    }
    Ok(())
}
