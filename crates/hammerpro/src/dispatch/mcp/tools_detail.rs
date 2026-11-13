use hp_app::AppState;
use hp_mcp::protocol::ToolResult;
use serde_json::json;

use super::semantic::*;
use super::tools::{Args, R};

domain! { "detail";
    detail_preview: Editor, "Preview the %detailtype grass in the 3D views",
        "Turn the detail props preview on or off (setting render.detail_preview, toolbar button, action \
        toggle_detail_preview) and report it. With it on, every displacement and brush face (world brushes and \
        func_detail) whose material has the %detailtype key gets the detail objects of the matching group of the \
        map's detail.vbsp (worldspawn detailvbsp, else detail.vbsp, else scripts/detail.vbsp of the game content), \
        placed like VBSP does at compile time: the group density per area, the displacement vertex alpha choosing \
        between the groups by their alpha, minAngle and maxAngle against the surface normal, upright, Sprite entries \
        drawn as billboards from the worldspawn detailmaterial (default detail/detailsprites) with their sprite \
        rectangle, spritesize, spriterandomscale, sprite_shape cross or tri and sway (swaying in time, driven by \
        render.animate_materials), Model entries as static models. Objects are generated only on the surfaces \
        within the view distance of the 3D camera (1200 units, fading over the last 400, or the \
        env_detail_controller fademindist and fademaxdist), nearest first, at most 60000 sprites and 1500 models, \
        and are the same for the same map. Without enabled it only reports. The report: enabled, drawn (objects \
        are on screen), detail_vbsp (the file read and the number of types in it, or the error), sprite_material, \
        surfaces (faces and displacements using a %detailtype that the file defines), displacements, brush_faces, \
        surfaces_in_range, sprites, models, capped (the limits cut some), view_distance, fade_distance and types \
        (material, detail_type, surfaces, known: false when the file has no such group), and exact: true once the brush faces are split like the compile (done in the background a moment after each edit), so the preview equals the compiled detail props prop for prop. The player view draws its \
        own compiled detail props and hides this preview.",
        {
            "enabled": boolean("Turn the preview on (true) or off (false); omit to only read it."),
        }, [];
    preview_detail_props: Read, "Preview the detail props of the compile",
        "Compute the detail props (%detailtype grass, sprites and models of detail.vbsp) the compile will emit, \
        without drawing: the same surfaces, seeds, face order and random placement as the compiler (Valve VBSP emits per \
        compiled face, so brush faces are first split exactly like the compile with the compile settings of the map: the \
        result equals the compile prop for prop, exact true), the displacement vertex alpha choosing the groups for WorldVertexTransition \
        materials, and the per face override (set_face_detail_type) applied. Limit to solid_ids or an area \
        (center and radius, or min and max). Reports objects, sprites, models, the count per detail type and \
        per surface (solid, side, material, type, overridden, objects), over_limit (the engine draws at most \
        65535), the fade distances, and up to sample object positions. With enabled it also turns the 3D view \
        preview (detail_preview) on or off.",
        {
            "solid_ids": target_solid_ids(),
            "center": vec3("Area center (with radius)."),
            "radius": number("Area radius.", 1.0, 131072.0),
            "min": vec3("Area box minimum (with max)."),
            "max": vec3("Area box maximum."),
            "sample": integer("Object positions returned (default 0).", 0, 500),
            "surfaces": integer("Surfaces listed (default 50).", 0, 2000),
            "enabled": boolean("Also turn the 3D view preview on (true) or off (false)."),
        }, [];
    list_detail_types: Read, "List the detail types",
        "The detail types of the map's detail.vbsp (worldspawn detailvbsp, else detail.vbsp, else \
        scripts/detail.vbsp): name, density, groups (alpha, and each entry: sprite or model, the model path, \
        amount, shape, sway, upright, min_cos and max_cos of the surface normal), the materials of the map using each type with their \
        face count, the faces given their own type by set_face_detail_type, and the types materials ask for that \
        the file does not define.",
        {
            "filter": string("Only types whose name contains this text."),
        }, [];
    set_face_detail_type: Edit, "Set the detail type of faces",
        "Give faces (brush faces and displacements) their own detail type instead of the %detailtype of their \
        material, so you choose where grass grows: type is a group of detail.vbsp, none to keep the face bare \
        (no detail props even when the material has a %detailtype), or empty to go back to the material. It is \
        stored as the side key hp_detailtype in the VMF (Hammer ignores it), honoured by the Hammer Pro compiler \
        and by the detail props preview. Faces: faces, or ids / solid_ids (every face), default the selected \
        faces. Also the Foliage tool in the Detail per face mode. One undo step.",
        {
            "faces": faces("Faces to change."),
            "ids": target_ids(),
            "solid_ids": target_solid_ids(),
            "type": string("Detail type, none, or empty to use the material."),
        }, ["type"];
}

const MAX_SURFACES: u64 = 2000;

struct Area {
    solids: Option<Vec<u32>>,
    bounds: Option<(glam::Vec3, glam::Vec3)>,
    circle: Option<(glam::Vec3, f32)>,
}

impl Area {
    fn keeps_surface(&self, s: &hp_assets::detail_preview::DetailSurface) -> bool {
        if let Some(ids) = &self.solids
            && !ids.contains(&s.solid)
        {
            return false;
        }
        if let Some((lo, hi)) = self.bounds
            && (s.maxs.cmplt(lo).any() || s.mins.cmpgt(hi).any())
        {
            return false;
        }
        if let Some((c, r)) = self.circle {
            let d = (s.mins - c).max(c - s.maxs).max(glam::Vec3::ZERO);
            if d.truncate().length() > r {
                return false;
            }
        }
        true
    }

    fn keeps_point(&self, p: glam::Vec3) -> bool {
        if let Some((lo, hi)) = self.bounds
            && (p.cmplt(lo).any() || p.cmpgt(hi).any())
        {
            return false;
        }
        if let Some((c, r)) = self.circle
            && p.truncate().distance(c.truncate()) > r
        {
            return false;
        }
        true
    }
}

fn area_of(state: &AppState, a: &Args<'_>) -> R<Area> {
    let solids = match a.get("solid_ids") {
        Some(_) => Some(
            super::semantic::targets(state, a)?
                .into_iter()
                .filter_map(|o| match o {
                    hp_editor::ObjRef::Solid(id) => Some(id),
                    hp_editor::ObjRef::Entity(_) => None,
                })
                .collect(),
        ),
        None => None,
    };
    let f = |v: glam::DVec3| v.as_vec3();
    let (bounds, circle) =
        match (a.opt_vec3("center")?, a.opt_f64("radius", 1.0, 131072.0)?, a.opt_vec3("min")?, a.opt_vec3("max")?) {
            (Some(c), Some(r), None, None) => (None, Some((f(c), r as f32))),
            (None, None, Some(lo), Some(hi)) => (Some((f(lo.min(hi)), f(lo.max(hi)))), None),
            (None, None, None, None) => (None, None),
            _ => return Err("give center and radius, or min and max".to_owned()),
        };
    Ok(Area { solids, bounds, circle })
}

fn detail_report(
    doc: &hp_editor::Document,
    dict: &hp_bspc::detail_props::DetailDict,
    lookup: &mut dyn FnMut(&str) -> Option<String>,
    area: &Area,
    sample: usize,
    listed: usize,
    split: Option<&hp_app::runtime::render::DetailSplit>,
) -> serde_json::Value {
    use hp_assets::detail_preview::{collect_surfaces, generate_ordered};
    use hp_bspc::detail_props::kind;
    let found = collect_surfaces(doc, lookup, &|t| dict.find(t).is_some(), split);
    let lumps = generate_ordered(&found.surfaces, dict, hp_bspc::detail_props::MAX_OBJECTS);
    let (mut objects, mut sprites, mut models) = (0usize, 0usize, 0usize);
    let mut per_type: Vec<(String, usize, usize)> = Vec::new();
    let mut surfaces = Vec::new();
    let mut points = Vec::new();
    let mut used = 0usize;
    for (s, lump) in found.surfaces.iter().zip(&lumps).filter(|(s, _)| area.keeps_surface(s)) {
        let kept: Vec<_> = lump.objects.iter().filter(|o| area.keeps_point(o.origin)).collect();
        if kept.is_empty() && area.circle.is_some() {
            continue;
        }
        used += 1;
        let m = kept.iter().filter(|o| o.kind == kind::MODEL).count();
        objects += kept.len();
        models += m;
        sprites += kept.len() - m;
        match per_type.iter_mut().find(|t| t.0 == s.type_name) {
            Some(t) => {
                t.1 += 1;
                t.2 += kept.len();
            }
            None => per_type.push((s.type_name.clone(), 1, kept.len())),
        }
        if surfaces.len() < listed {
            surfaces.push(json!({
                "solid": s.solid,
                "side": s.side_index,
                "side_id": s.side_id,
                "material": s.material,
                "detail_type": s.type_name,
                "overridden": s.overridden,
                "displacement": matches!(s.surface, hp_bspc::detail_props::ScatterSurface::Disp { .. }),
                "objects": kept.len(),
            }));
        }
        for o in kept {
            if points.len() >= sample {
                break;
            }
            let name = if o.kind == kind::MODEL {
                lump.models.get(usize::from(o.dict)).cloned().unwrap_or_default()
            } else {
                "sprite".to_owned()
            };
            points.push(json!({ "origin": [o.origin.x, o.origin.y, o.origin.z], "kind": name }));
        }
    }
    let fade = hp_editor::limits::world::detail_fade(
        doc.scene()
            .entities()
            .values()
            .map(|n| &n.entity)
            .find(|e| e.classname.eq_ignore_ascii_case("env_detail_controller"))
            .map(|e| {
                let f = |k: &str, d: f32| e.get(k).and_then(|v| v.trim().parse::<f32>().ok()).unwrap_or(d);
                (f("fademindist", 400.0), f("fademaxdist", 1200.0))
            }),
    );
    json!({
        "exact": found.exact,
        "objects": objects,
        "sprites": sprites,
        "models": models,
        "over_limit": objects > hp_bspc::detail_props::MAX_OBJECTS,
        "surfaces_used": used,
        "displacements": found.displacements,
        "brush_faces": found.brush_faces,
        "faces_without_detail": found.disabled,
        "types": per_type.iter().map(|(t, n, o)| json!({ "detail_type": t, "surfaces": n, "objects": o })).collect::<Vec<_>>(),
        "missing_types": found.counts.iter().filter(|c| dict.find(&c.1).is_none()).map(|c| json!({ "material": c.0, "detail_type": c.1, "faces": c.2 })).collect::<Vec<_>>(),
        "surfaces": surfaces,
        "sample": points,
        "view_distance": fade[0],
        "fade_distance": fade[1],
    })
}

fn game_dict(state: &AppState) -> R<(String, hp_bspc::detail_props::DetailDict)> {
    let fs = state.assets.fs().ok_or("the game content is not loaded (set_game)")?;
    let wanted = hp_assets::detail_preview::detail_vbsp_name(state.doc.scene().world());
    hp_assets::detail_preview::load_detail_dict(fs, &wanted)
}

fn material_lookup(state: &AppState) -> impl FnMut(&str) -> Option<String> + '_ {
    let mut cache: std::collections::HashMap<String, Option<String>> = std::collections::HashMap::new();
    move |m: &str| {
        let key = m.to_ascii_lowercase();
        if let Some(hit) = cache.get(&key) {
            return hit.clone();
        }
        let found = state.assets.fs().and_then(|fs| hp_assets::detail_preview::material_detail_type(fs, m));
        cache.insert(key, found.clone());
        found
    }
}

fn preview_detail_props(state: &mut AppState, a: &Args<'_>) -> R<ToolResult> {
    if let Some(on) = a.opt_bool("enabled")? {
        state.settings.render.detail_preview = on;
        if !on {
            state.runtime.render.detail_preview = Default::default();
        }
    }
    let area = area_of(state, a)?;
    let sample = a.opt_u64("sample", 0, 500)?.unwrap_or(0) as usize;
    let listed = a.opt_u64("surfaces", 0, MAX_SURFACES)?.unwrap_or(50) as usize;
    let (file, dict) = game_dict(state)?;
    let split = crate::dispatch::detail_split::split_now(state);
    let mut lookup = material_lookup(state);
    let mut out = detail_report(&state.doc, &dict, &mut lookup, &area, sample, listed, split.as_deref());
    if let serde_json::Value::Object(m) = &mut out {
        m.insert("detail_vbsp".to_owned(), json!(file));
        m.insert("preview_enabled".to_owned(), json!(state.settings.render.detail_preview));
    }
    Ok(ToolResult::json(out))
}

fn types_report(
    doc: &hp_editor::Document,
    dict: &hp_bspc::detail_props::DetailDict,
    lookup: &mut dyn FnMut(&str) -> Option<String>,
    filter: &str,
) -> serde_json::Value {
    let found = hp_assets::detail_preview::collect_surfaces(doc, lookup, &|_| true, None);
    let overrides: Vec<&hp_assets::detail_preview::DetailSurface> =
        found.surfaces.iter().filter(|s| s.overridden).collect();
    let f = filter.to_ascii_lowercase();
    let types: Vec<serde_json::Value> = dict
        .types
        .iter()
        .filter(|t| f.is_empty() || t.name.to_ascii_lowercase().contains(&f))
        .map(|t| {
            let groups: Vec<serde_json::Value> = t
                .groups
                .iter()
                .map(|g| {
                    let entries: Vec<serde_json::Value> = g
                        .models
                        .iter()
                        .map(|m| {
                            let shape = match m.kind {
                                Some(hp_bspc::detail_props::kind::SHAPE_CROSS) => "cross",
                                Some(hp_bspc::detail_props::kind::SHAPE_TRI) => "tri",
                                _ => "flat",
                            };
                            json!({
                                "kind": if m.model.is_some() { "model" } else { "sprite" },
                                "model": m.model,
                                "amount": m.amount,
                                "shape": shape,
                                "sway": m.sway,
                                "upright": m.upright,
                                "min_cos": m.min_cos,
                                "max_cos": m.max_cos,
                            })
                        })
                        .collect();
                    json!({ "alpha": g.alpha, "entries": entries })
                })
                .collect();
            let used: Vec<serde_json::Value> = found
                .counts
                .iter()
                .filter(|c| c.1.eq_ignore_ascii_case(&t.name))
                .map(|c| json!({ "material": c.0, "faces": c.2 }))
                .collect();
            json!({ "name": t.name, "density": t.density, "groups": groups, "used_by": used })
        })
        .collect();
    json!({
        "types": types,
        "overridden_faces": overrides.iter().map(|s| json!({ "solid": s.solid, "side": s.side_index, "detail_type": s.type_name })).collect::<Vec<_>>(),
        "faces_without_detail": found.disabled,
        "missing_types": found.counts.iter().filter(|c| dict.find(&c.1).is_none()).map(|c| json!({ "material": c.0, "detail_type": c.1, "faces": c.2 })).collect::<Vec<_>>(),
    })
}

fn list_detail_types(state: &mut AppState, a: &Args<'_>) -> R<ToolResult> {
    let filter = a.opt_str("filter", 128)?.unwrap_or("").trim().to_owned();
    let (file, dict) = game_dict(state)?;
    let mut lookup = material_lookup(state);
    let mut out = types_report(&state.doc, &dict, &mut lookup, &filter);
    if let serde_json::Value::Object(m) = &mut out {
        m.insert("detail_vbsp".to_owned(), json!(file));
    }
    Ok(ToolResult::json(out))
}

fn set_face_detail_type(state: &mut AppState, a: &Args<'_>) -> R<ToolResult> {
    let value = a.str("type", 128)?.trim().to_owned();
    if value.chars().any(|c| c.is_whitespace() || c == '"') {
        return Err("type is one word: a detail.vbsp group, none, or empty".to_owned());
    }
    let faces = super::semantic::target_faces(state, a, "faces")?;
    let mut by_solid: Vec<(u32, Vec<usize>)> = Vec::new();
    for f in &faces {
        match by_solid.iter_mut().find(|(id, _)| *id == f.solid) {
            Some((_, sides)) => sides.push(f.side),
            None => by_solid.push((f.solid, vec![f.side])),
        }
    }
    let key = hp_app::foliage::DETAIL_OVERRIDE_KEY;
    let changed = super::tools::edit(state, "set_face_detail_type", |doc| {
        let mut n = 0usize;
        for (id, sides) in &by_solid {
            let did = doc.edit_solid(*id, |s| {
                for i in sides {
                    let Some(side) = s.sides.get_mut(*i) else { continue };
                    side.extra_props.retain(|(k, _)| !k.eq_ignore_ascii_case(key));
                    if !value.is_empty() {
                        side.extra_props.push((key.to_owned(), value.clone()));
                    }
                }
            })?;
            if did {
                n += sides.len();
            }
        }
        Ok(n)
    })?;
    Ok(ToolResult::json(json!({
        "faces": faces.len(),
        "changed": changed,
        "type": if value.is_empty() { serde_json::Value::Null } else { json!(value) },
        "meaning": if value.is_empty() {
            "the faces use the %detailtype of their material again"
        } else if value.eq_ignore_ascii_case(hp_app::foliage::DETAIL_OVERRIDE_NONE) {
            "the faces get no detail props"
        } else {
            "the faces get the detail props of this type"
        },
    })))
}

fn detail_preview(state: &mut AppState, a: &Args<'_>) -> R<ToolResult> {
    if let Some(on) = a.opt_bool("enabled")? {
        state.settings.render.detail_preview = on;
        if !on {
            state.runtime.render.detail_preview = Default::default();
        }
    }
    let on = state.settings.render.detail_preview;
    let r = &state.runtime.render.detail_preview;
    let note = if !on {
        "off"
    } else if state.runtime.lab.player.active {
        "hidden while the player view is active: it draws the compiled detail props"
    } else if r.dict_error.is_some() {
        "the detail.vbsp could not be read"
    } else if r.surfaces == 0 && r.types.is_empty() {
        "no visible face uses a material with a %detailtype (or the first frame is not drawn yet)"
    } else if r.surfaces == 0 {
        "the materials name %detailtype groups that the detail.vbsp does not define"
    } else {
        "ok"
    };
    Ok(ToolResult::json(json!({
        "enabled": on,
        "drawn": r.drawn,
        "detail_vbsp": { "file": r.dict_file, "types": r.dict_types, "error": r.dict_error },
        "sprite_material": r.sprite_material,
        "surfaces": r.surfaces,
        "displacements": r.displacements,
        "brush_faces": r.brush_faces,
        "surfaces_in_range": r.surfaces_in_range,
        "sprites": r.sprites,
        "models": r.models,
        "capped": r.capped,
        "exact": r.exact,
        "view_distance": r.view_distance,
        "fade_distance": r.fade_distance,
        "types": r.types.iter().map(|t| json!({
            "material": t.material,
            "detail_type": t.detail_type,
            "surfaces": t.surfaces,
            "known": t.known,
        })).collect::<Vec<_>>(),
        "note": note,
    })))
}

#[cfg(test)]
mod tests {
    use serde_json::json;

    use super::super::semantic_tests::{err, ok};

    const VBSP: &str = "detail { grass { \"density\" \"3000\" Low { \"alpha\" \"0\" A { \"sprite\" \"0 0 64 32 512\" \"amount\" \"1\" } } High { \"alpha\" \"1\" B { \"model\" \"models/rock.mdl\" \"amount\" \"1\" } } } }";

    fn disp_state() -> hp_app::AppState {
        let mut state = hp_app::testing::test_state();
        state.doc.create_displacements(&[hp_editor::FaceRef::new(2, 0)], 2).expect("disp");
        state
    }

    #[test]
    fn face_overrides_drive_the_report() {
        assert_eq!(hp_app::foliage::DETAIL_OVERRIDE_KEY, hp_bspc::detail_props::OVERRIDE_KEY);
        assert_eq!(hp_app::foliage::DETAIL_OVERRIDE_NONE, hp_bspc::detail_props::OVERRIDE_NONE);
        let mut state = disp_state();
        let dict = hp_bspc::detail_props::parse_detail_vbsp(VBSP).expect("vbsp");
        let area = super::Area { solids: None, bounds: None, circle: None };
        let mut none = |_: &str| None;
        let empty = super::detail_report(&state.doc, &dict, &mut none, &area, 5, 10, None);
        assert_eq!(empty["objects"], 0);
        let set =
            ok(&mut state, "set_face_detail_type", json!({ "faces": [{ "solid": 2, "side": 0 }], "type": "grass" }));
        assert_eq!(set["changed"], 1);
        assert_eq!(state.doc.undo_label(), Some("MCP: set_face_detail_type"));
        let r = super::detail_report(&state.doc, &dict, &mut none, &area, 5, 10, None);
        assert!(r["objects"].as_u64().unwrap() > 50, "{r}");
        assert_eq!(r["types"][0]["detail_type"], "grass");
        assert_eq!(r["surfaces"][0]["overridden"], true);
        assert_eq!(r["sample"].as_array().unwrap().len(), 5);
        let circle = super::Area { solids: None, bounds: None, circle: Some((glam::Vec3::ZERO, 64.0)) };
        let small = super::detail_report(&state.doc, &dict, &mut none, &circle, 0, 10, None);
        assert!(small["objects"].as_u64().unwrap() < r["objects"].as_u64().unwrap());
        let types = super::types_report(&state.doc, &dict, &mut none, "");
        assert_eq!(types["types"][0]["name"], "grass");
        assert_eq!(types["overridden_faces"][0]["solid"], 2);
        assert_eq!(types["types"][0]["groups"][1]["entries"][0]["kind"], "model");
        ok(&mut state, "set_face_detail_type", json!({ "faces": [{ "solid": 2, "side": 0 }], "type": "none" }));
        let mut grass = |_: &str| Some("grass".to_owned());
        let off = super::detail_report(&state.doc, &dict, &mut grass, &area, 0, 10, None);
        assert!(off["faces_without_detail"].as_u64().unwrap() >= 1);
        assert!(off["surfaces"].as_array().unwrap().iter().all(|s| !(s["solid"] == 2 && s["side"] == 0)));
        ok(&mut state, "set_face_detail_type", json!({ "faces": [{ "solid": 2, "side": 0 }], "type": "" }));
        let side = state.doc.scene().solid(2).unwrap().solid.sides[0].extra_props.clone();
        assert!(side.iter().all(|(k, _)| k != "hp_detailtype"));
        assert!(
            err(&mut state, "set_face_detail_type", json!({ "faces": [{ "solid": 2, "side": 0 }], "type": "a b" }))
                .contains("one word")
        );
    }

    #[test]
    fn the_game_is_needed_for_the_file() {
        let mut state = disp_state();
        assert!(err(&mut state, "list_detail_types", json!({})).contains("not loaded"));
        assert!(err(&mut state, "preview_detail_props", json!({ "center": [0, 0, 0] })).contains("center and radius"));
    }

    #[test]
    fn the_preview_is_switched_and_read() {
        let mut state = hp_app::testing::test_state();
        let off = ok(&mut state, "detail_preview", json!({}));
        assert_eq!(off["enabled"], false);
        assert_eq!(off["note"], "off");
        let on = ok(&mut state, "detail_preview", json!({ "enabled": true }));
        assert!(state.settings.render.detail_preview);
        assert_eq!(on["enabled"], true);
        assert_eq!(on["types"], json!([]));
        assert_eq!(on["drawn"], false);
        let back = ok(&mut state, "detail_preview", json!({ "enabled": false }));
        assert_eq!(back["enabled"], false);
        assert!(!state.settings.render.detail_preview);
    }
}
