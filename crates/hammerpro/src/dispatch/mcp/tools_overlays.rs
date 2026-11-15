use glam::{DVec2, DVec3};
use hp_app::AppState;
use hp_editor::{FaceRef, PickOptions};
use hp_mcp::protocol::ToolResult;
use serde_json::{Value, json};

use super::semantic::*;
use super::tools::{Args, MAX_NAME, R, edit, material_name, v3};

const MAX_OVERLAY_FACES: usize = 64;

domain! { "overlays and decals";
    list_overlays: Read, "List overlays",
        "The info_overlay entities: material, faces (solid and side), origin, basis, UV corners and render order.",
        { "limit": limit(), "offset": offset() }, [];
    create_overlay: Edit, "Create an overlay",
        "Place an info_overlay with a material on a face (and more faces it spreads over), centered on a point of \
        the face (default: the face center), the size of the texture or a given size, rotated around the face \
        normal.",
        {
            "face": faces("The face it lies on (one item)."),
            "extra_faces": faces("More faces it covers."),
            "material": string("Overlay material (default: the current material)."),
            "center": vec3("Center (projected on the face plane)."),
            "size": vec2("Width and height in units (default: the texture size)."),
            "rotation": number("Rotation around the face normal in degrees.", -360.0, 360.0),
            "render_order": integer("Render order (0 to 3).", 0, 3),
        }, ["face"];
    set_overlay: Edit, "Edit an overlay",
        "Change an overlay: its faces (the whole list), material, size, rotation around its normal (added), or \
        render order.",
        {
            "id": integer("info_overlay id.", 1, i64::from(u32::MAX)),
            "faces": faces("Every face it covers."),
            "material": string("Material."),
            "size": vec2("Width and height in units."),
            "rotate_by": number("Degrees added around the normal.", -360.0, 360.0),
            "render_order": integer("Render order (0 to 3).", 0, 3),
        }, ["id"];
    trim_overlay_faces: Edit, "Trim overlay faces",
        "Remove from overlays (info_overlay) the brush faces their projected rectangle does not touch, in one \
        undo step: fewer fragments, and back under the 64 face engine limit. Faces that touch the rectangle \
        and side ids that no longer exist are kept; an overlay touching none of its faces is left as is. \
        Default: the selected overlays. preview reports what would be removed without editing.",
        {
            "ids": ids("info_overlay ids (default: the selected overlays)."),
            "all": boolean("Every overlay of the map."),
            "preview": boolean("Only report, do not edit (default false)."),
        }, [];
    list_decals: Read, "List decals",
        "The infodecal entities: id, material (texture), origin, targetname.",
        { "limit": limit(), "offset": offset() }, [];
    create_decal: Edit, "Create a decal",
        "Place an infodecal with a decal material on the surface a ray hits (origin and direction), or at a point.",
        {
            "material": string("Decal material (default: the current material)."),
            "origin": vec3("Point, or start of the ray when direction is given."),
            "direction": vec3("Ray direction toward the surface (for example [0, 0, -1])."),
            "low_priority": boolean("Low priority (can be replaced by other decals)."),
        }, ["origin"];
}

fn fmt(v: DVec3) -> String {
    hp_editor::instances::fmt_vec(v)
}

fn face_of_side(state: &AppState, side_id: u32) -> Option<FaceRef> {
    state
        .doc
        .scene()
        .solids()
        .iter()
        .find_map(|(id, n)| n.solid.sides.iter().position(|s| s.id == side_id).map(|i| FaceRef::new(*id, i)))
}

fn side_id(state: &AppState, f: FaceRef) -> R<u32> {
    state.doc.face_side(f).map(|s| s.id).ok_or_else(|| format!("solid {} has no side {}", f.solid, f.side))
}

fn face_center_normal(state: &AppState, f: FaceRef) -> R<(DVec3, DVec3)> {
    let poly = state.doc.face_polygon(f).ok_or("the face has no geometry")?;
    if poly.is_empty() {
        return Err("the face has no geometry".to_owned());
    }
    let center = poly.iter().copied().sum::<DVec3>() / poly.len() as f64;
    let side = state.doc.face_side(f).ok_or("no such face")?;
    let plane = hp_geom::Plane::from_side(side).ok_or("the face plane is invalid")?;
    Ok((center, plane.normal))
}

fn material_or_current(state: &AppState, a: &Args<'_>) -> R<String> {
    match a.opt_str("material", MAX_NAME)? {
        Some(m) => material_name(m),
        None => Ok(state.current_material.clone()),
    }
}

fn is_overlay(e: &hp_vmf::Entity) -> bool {
    hp_editor::side_index::overlay::is_overlay_class(&e.classname)
}

fn list_overlays(state: &mut AppState, a: &Args<'_>) -> R<ToolResult> {
    let mut ids: Vec<u32> =
        state.doc.scene().entities().iter().filter(|(_, e)| is_overlay(&e.entity)).map(|(id, _)| *id).collect();
    ids.sort_unstable();
    let (ids, total) = page(ids, a)?;
    let list: Vec<Value> = ids
        .into_iter()
        .filter_map(|id| {
            let e = &state.doc.scene().entity(id)?.entity;
            let sides: Vec<u32> =
                e.get("sides").unwrap_or_default().split_whitespace().filter_map(|s| s.parse().ok()).collect();
            let faces: Vec<Value> = sides
                .iter()
                .map(|sid| face_of_side(state, *sid).map_or(json!({ "side_id": sid, "missing": true }), face_json))
                .collect();
            let uv: Vec<Option<Value>> =
                ["uv0", "uv1", "uv2", "uv3"].iter().map(|k| e.get(k).and_then(hp_vmf::parse_vec3).map(v3)).collect();
            Some(json!({
                "id": id,
                "material": e.get("material"),
                "faces": faces,
                "origin": e.get("origin").and_then(hp_vmf::parse_vec3).map(v3),
                "basis_normal": e.get("BasisNormal").and_then(hp_vmf::parse_vec3).map(v3),
                "uv": uv,
                "render_order": e.get("RenderOrder"),
            }))
        })
        .collect();
    Ok(ToolResult::json(json!({ "total": total, "overlays": list })))
}

fn face_list(state: &AppState, a: &Args<'_>, key: &str) -> R<Vec<FaceRef>> {
    let f = opt_faces(&state.doc, a, key)?.unwrap_or_default();
    if f.len() > MAX_OVERLAY_FACES {
        return Err(format!("an overlay covers at most {MAX_OVERLAY_FACES} faces"));
    }
    Ok(f)
}

fn turned_basis(u: DVec3, v: DVec3, normal: DVec3, deg: f64) -> (DVec3, DVec3) {
    if deg == 0.0 {
        return (u, v);
    }
    let q = glam::DQuat::from_axis_angle(normal.try_normalize().unwrap_or(DVec3::Z), deg.to_radians());
    (q * u, q * v)
}

fn uv_corners(size: DVec2) -> [(String, String); 4] {
    let (hw, hh) = (size.x.abs() * 0.5, size.y.abs() * 0.5);
    let uv = |x: f64, y: f64| fmt(DVec3::new(x, y, 0.0));
    [
        ("uv0".to_owned(), uv(-hw, -hh)),
        ("uv1".to_owned(), uv(-hw, hh)),
        ("uv2".to_owned(), uv(hw, hh)),
        ("uv3".to_owned(), uv(hw, -hh)),
    ]
}

fn opt_size(a: &Args<'_>) -> R<Option<DVec2>> {
    let s = opt_vec2(a, "size", 0.5, 65536.0)?;
    Ok(s)
}

fn create_overlay(state: &mut AppState, a: &Args<'_>) -> R<ToolResult> {
    let main = face_list(state, a, "face")?;
    let [face] = main.as_slice() else { return Err("face must be exactly one {solid, side}".to_owned()) };
    let face = *face;
    let mut all = vec![face];
    for f in face_list(state, a, "extra_faces")? {
        if !all.contains(&f) {
            all.push(f);
        }
    }
    if all.len() > MAX_OVERLAY_FACES {
        return Err(format!("an overlay covers at most {MAX_OVERLAY_FACES} faces"));
    }
    let material = material_or_current(state, a)?;
    let (center, normal) = face_center_normal(state, face)?;
    let plane = hp_geom::Plane::from_side(state.doc.face_side(face).ok_or("no such face")?).ok_or("invalid face")?;
    let center = plane.project(a.opt_vec3("center")?.unwrap_or(center));
    let size = match opt_size(a)? {
        Some(s) => s,
        None => state
            .assets
            .material_info(&material)
            .filter(|i| i.width > 0 && i.height > 0)
            .map_or(DVec2::splat(128.0), |i| DVec2::new(f64::from(i.width), f64::from(i.height))),
    };
    let side_ids: Vec<String> = all.iter().map(|f| side_id(state, *f).map(|s| s.to_string())).collect::<R<_>>()?;
    let mut kv = hp_tools_create::overlay_keyvalues(&material, side_id(state, face)?, center, normal, size);
    let rotation = a.opt_f64("rotation", -360.0, 360.0)?.unwrap_or(0.0);
    if rotation != 0.0 {
        let (u, v) = hp_tools_create::overlay_basis(normal);
        let (u, v) = turned_basis(u, v, normal, rotation);
        for (k, val) in &mut kv {
            match k.as_str() {
                "BasisU" => *val = fmt(u),
                "BasisV" => *val = fmt(v),
                _ => {}
            }
        }
    }
    for (k, val) in &mut kv {
        if k == "sides" {
            *val = side_ids.join(" ");
        }
    }
    if let Some(o) = a.opt_u64("render_order", 0, 3)? {
        kv.push(("RenderOrder".to_owned(), o.to_string()));
    }
    let id = edit(state, "create_overlay", |doc| doc.create_point_entity("info_overlay", center, &kv))?;
    let mut out = json!({ "id": id, "material": material, "faces": all.iter().map(|f| face_json(*f)).collect::<Vec<_>>(), "size": [size.x, size.y] });
    if let Some(w) = super::tools::material_warning(state, &material) {
        out["warning"] = json!(w);
    }
    Ok(ToolResult::json(out))
}

fn set_overlay(state: &mut AppState, a: &Args<'_>) -> R<ToolResult> {
    let id = entity_id(&state.doc, a, "id")?;
    let e = state.doc.scene().entity(id).map(|n| n.entity.clone()).ok_or("gone")?;
    if !is_overlay(&e) {
        return Err(format!("{id} is not an overlay"));
    }
    let mut changes: Vec<(String, String)> = Vec::new();
    if a.get("faces").is_some() {
        let faces = face_list(state, a, "faces")?;
        if faces.is_empty() {
            return Err("an overlay needs at least one face".to_owned());
        }
        let ids: Vec<String> = faces.iter().map(|f| side_id(state, *f).map(|s| s.to_string())).collect::<R<_>>()?;
        changes.push(("sides".to_owned(), ids.join(" ")));
    }
    if a.get("material").is_some() {
        changes.push(("material".to_owned(), material_or_current(state, a)?));
    }
    if let Some(size) = opt_size(a)? {
        changes.extend(uv_corners(size));
    }
    if let Some(deg) = a.opt_f64("rotate_by", -360.0, 360.0)? {
        let get = |k: &str| e.get(k).and_then(hp_vmf::parse_vec3);
        let (u, v, n) = (
            get("BasisU").unwrap_or(DVec3::X),
            get("BasisV").unwrap_or(DVec3::Y),
            get("BasisNormal").unwrap_or(DVec3::Z),
        );
        let (u, v) = turned_basis(u, v, n, deg);
        changes.push(("BasisU".to_owned(), fmt(u)));
        changes.push(("BasisV".to_owned(), fmt(v)));
    }
    if let Some(o) = a.opt_u64("render_order", 0, 3)? {
        changes.push(("RenderOrder".to_owned(), o.to_string()));
    }
    if changes.is_empty() {
        return Err("give faces, material, size, rotate_by or render_order".to_owned());
    }
    edit(state, "set_overlay", |doc| {
        doc.edit_entity(id, |e| {
            for (k, v) in &changes {
                e.set(k, v.clone());
            }
        })
    })?;
    Ok(ToolResult::json(json!({ "id": id, "changed": changes.iter().map(|(k, _)| k.as_str()).collect::<Vec<_>>() })))
}

fn trim_overlay_faces(state: &mut AppState, a: &Args<'_>) -> R<ToolResult> {
    let ids: Vec<u32> = if a.opt_bool("all")?.unwrap_or(false) {
        state.doc.scene().entities().iter().filter(|(_, e)| is_overlay(&e.entity)).map(|(id, _)| *id).collect()
    } else if a.get("ids").is_some() {
        let ids = entity_ids(&state.doc, a, "ids")?;
        if let Some(id) = ids.iter().find(|id| state.doc.scene().entity(**id).is_none_or(|n| !is_overlay(&n.entity))) {
            return Err(format!("{id} is not an overlay"));
        }
        ids
    } else {
        state
            .doc
            .selected_entity_ids()
            .into_iter()
            .filter(|id| state.doc.scene().entity(*id).is_some_and(|n| is_overlay(&n.entity)))
            .collect()
    };
    if ids.is_empty() {
        return Err("no overlay: give ids, all, or select overlays first".to_owned());
    }
    let survey: Vec<Value> = ids
        .iter()
        .filter_map(|id| hp_editor::optimize::overlays::overlay_faces(&state.doc, *id))
        .map(|o| {
            json!({
                "id": o.id,
                "faces": o.listed,
                "touched": o.touched.len(),
                "untouched_side_ids": o.untouched,
                "missing_side_ids": o.missing,
                "trimmable": o.trimmed().is_some(),
            })
        })
        .collect();
    if a.opt_bool("preview")?.unwrap_or(false) {
        return Ok(ToolResult::json(json!({ "preview": true, "overlays": survey })));
    }
    let r = edit(state, "trim_overlay_faces", |doc| doc.trim_overlay_faces(&ids))?;
    Ok(ToolResult::json(json!({
        "removed": r.removed,
        "trimmed": r.overlays.iter().map(|(id, kept, removed)| json!({ "id": id, "kept": kept, "removed": removed })).collect::<Vec<_>>(),
        "overlays": survey,
        "undo": if r.removed > 0 { "one undo step: MCP: trim_overlay_faces" } else { "nothing changed" },
    })))
}

fn list_decals(state: &mut AppState, a: &Args<'_>) -> R<ToolResult> {
    let mut ids: Vec<u32> = state
        .doc
        .scene()
        .entities()
        .iter()
        .filter(|(_, e)| e.entity.classname.eq_ignore_ascii_case("infodecal"))
        .map(|(id, _)| *id)
        .collect();
    ids.sort_unstable();
    let (ids, total) = page(ids, a)?;
    let list: Vec<Value> = ids
        .into_iter()
        .filter_map(|id| {
            let e = &state.doc.scene().entity(id)?.entity;
            Some(json!({
                "id": id,
                "material": e.get("texture"),
                "origin": e.get("origin").and_then(hp_vmf::parse_vec3).map(v3),
                "targetname": e.get("targetname"),
            }))
        })
        .collect();
    Ok(ToolResult::json(json!({ "total": total, "decals": list })))
}

fn create_decal(state: &mut AppState, a: &Args<'_>) -> R<ToolResult> {
    let material = material_or_current(state, a)?;
    let origin = a.vec3("origin")?;
    let point = match opt_dir(a, "direction")? {
        None => origin,
        Some(dir) => {
            let opts = PickOptions { include_entities: false, ..Default::default() };
            let f = bounds_fn(state);
            let hit = state.doc.pick_ray(&hp_geom::Ray::new(origin, dir), &opts, &f);
            hit.ok_or("the ray hits no surface")?.point
        }
    };
    let mut kv = vec![("texture".to_owned(), material.clone())];
    if a.opt_bool("low_priority")?.unwrap_or(false) {
        kv.push(("LowPriority".to_owned(), "1".to_owned()));
    }
    let id = edit(state, "create_decal", |doc| doc.create_point_entity("infodecal", point, &kv))?;
    Ok(ToolResult::json(json!({ "id": id, "origin": v3(point), "material": material })))
}

#[cfg(test)]
mod tests {
    use serde_json::json;

    use super::super::semantic_tests::{err, ok};

    #[test]
    fn overlays_and_decals() {
        let mut s = hp_app::testing::test_state();
        let top = json!([{ "solid": 2, "side": 0 }]);
        let r = ok(
            &mut s,
            "create_overlay",
            json!({ "face": top, "material": "overlays/road01", "center": [10, 20, 99], "size": [128, 64], "rotation": 90 }),
        );
        let id = r["id"].as_u64().unwrap();
        let e = s.doc.scene().entity(id as u32).unwrap().entity.clone();
        assert_eq!(e.get("sides"), Some("1"));
        assert_eq!(hp_vmf::parse_vec3(e.get("origin").unwrap()).unwrap().z, 0.0, "projected on the face");
        let l = ok(&mut s, "list_overlays", json!({}));
        assert_eq!(l["total"], 1);
        assert_eq!(l["overlays"][0]["faces"][0]["solid"], 2);
        ok(
            &mut s,
            "set_overlay",
            json!({ "id": id, "faces": [{ "solid": 2, "side": 0 }, { "solid": 3, "side": 1 }], "rotate_by": -90, "render_order": 1 }),
        );
        let e = s.doc.scene().entity(id as u32).unwrap().entity.clone();
        assert_eq!(e.get("sides").unwrap().split(' ').count(), 2);
        assert_eq!(e.get("RenderOrder"), Some("1"));
        err(&mut s, "set_overlay", json!({ "id": 5, "render_order": 1 }));
        err(&mut s, "set_overlay", json!({ "id": id }));
        err(&mut s, "create_overlay", json!({ "face": [] }));
        let r = ok(
            &mut s,
            "create_decal",
            json!({ "material": "decals/lambdaspray_2a", "origin": [0, 0, 64], "direction": [0, 0, -1] }),
        );
        assert_eq!(r["origin"], json!([0.0, 0.0, 0.0]));
        assert_eq!(ok(&mut s, "list_decals", json!({}))["total"], 1);
        err(&mut s, "create_decal", json!({ "origin": [0, 0, 64], "direction": [0, 0, 1] }));
    }
}
