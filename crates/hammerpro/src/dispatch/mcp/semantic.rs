use glam::DVec3;
use hp_app::AppState;
use hp_editor::{Document, FaceRef, ObjRef};
use hp_mcp::tools::{MAX_LIST, ToolSpec};
use serde_json::{Value, json};

use super::tools::{Args, MAX_IDS, R, args, objects};

macro_rules! domain {
    ($category:literal; $( $name:ident : $kind:ident, $title:literal, $desc:literal, { $($schema:tt)* }, [$($req:literal),*]; )*) => {
        pub(crate) const DOMAIN: super::tools::Domain =
            super::tools::Domain { category: $category, specs: SPECS, run: run_tool };

        const SPECS: &[hp_mcp::tools::ToolSpec] = &[
            $( hp_mcp::tools::ToolSpec {
                name: stringify!($name),
                title: $title,
                description: $desc,
                kind: hp_mcp::tools::ToolKind::$kind,
                schema: || super::semantic::object(serde_json::json!({ $($schema)* }), &[$($req),*]),
            }, )*
        ];

        fn run_tool(
            state: &mut hp_app::AppState,
            name: &str,
            v: &serde_json::Value,
        ) -> Option<super::tools::Outcome> {
            let spec = SPECS.iter().find(|s| s.name == name)?;
            let result: super::tools::R<super::tools::Outcome> = match name {
                $( stringify!($name) => super::semantic::checked(spec, v).and_then(|a| $name(state, &a).map(Into::into)), )*
                _ => return None,
            };
            Some(result.unwrap_or_else(|e| hp_mcp::protocol::ToolResult::error(format!("{name}: {e}")).into()))
        }
    };
}
pub(super) use domain;

pub(super) fn object(properties: Value, required: &[&str]) -> Value {
    json!({ "type": "object", "properties": properties, "required": required, "additionalProperties": false })
}

pub(super) fn vec3(description: &str) -> Value {
    json!({ "type": "array", "items": { "type": "number" }, "minItems": 3, "maxItems": 3, "description": description })
}

pub(super) fn vec2(description: &str) -> Value {
    json!({ "type": "array", "items": { "type": "number" }, "minItems": 2, "maxItems": 2, "description": description })
}

pub(super) fn ids(description: &str) -> Value {
    json!({ "type": "array", "items": { "type": "integer", "minimum": 1 }, "maxItems": MAX_IDS, "description": description })
}

pub(super) fn target_ids() -> Value {
    ids("Entity or solid ids (an id shared by an entity and a solid names the entity). Default: the selection.")
}

pub(super) fn target_solid_ids() -> Value {
    ids("Solid ids, when a solid shares its id with an entity.")
}

pub(super) fn integer(description: &str, min: i64, max: i64) -> Value {
    json!({ "type": "integer", "minimum": min, "maximum": max, "description": description })
}

pub(super) fn number(description: &str, min: f64, max: f64) -> Value {
    json!({ "type": "number", "minimum": min, "maximum": max, "description": description })
}

pub(super) fn string(description: &str) -> Value {
    json!({ "type": "string", "description": description })
}

pub(super) fn boolean(description: &str) -> Value {
    json!({ "type": "boolean", "description": description })
}

pub(super) fn choice(description: &str, values: &[&str]) -> Value {
    json!({ "type": "string", "enum": values, "description": description })
}

pub(super) fn strings(description: &str, max: usize) -> Value {
    json!({ "type": "array", "items": { "type": "string" }, "maxItems": max, "description": description })
}

pub(super) fn keyvalues(description: &str) -> Value {
    json!({ "type": "object", "additionalProperties": { "type": "string" }, "description": description })
}

pub(super) fn faces(description: &str) -> Value {
    json!({
        "type": "array",
        "maxItems": MAX_IDS,
        "items": {
            "type": "object",
            "properties": {
                "solid": { "type": "integer", "minimum": 1 },
                "side": { "type": "integer", "minimum": 0 },
            },
            "required": ["solid", "side"],
            "additionalProperties": false,
        },
        "description": description,
    })
}

pub(super) fn limit() -> Value {
    integer("Most items returned (default 100).", 1, MAX_LIST as i64)
}

pub(super) fn offset() -> Value {
    integer("Items skipped first, for paging (default 0).", 0, i64::from(u32::MAX))
}

pub(super) fn axis(description: &str) -> Value {
    choice(description, &["x", "y", "z"])
}

pub(super) fn checked<'a>(spec: &ToolSpec, v: &'a Value) -> R<Args<'a>> {
    let schema = (spec.schema)();
    let names: Vec<&str> =
        schema["properties"].as_object().map(|p| p.keys().map(String::as_str).collect()).unwrap_or_default();
    let a = args(v, &names)?;
    for r in schema["required"].as_array().into_iter().flatten().filter_map(Value::as_str) {
        if a.get(r).is_none() {
            return Err(format!("{r} is required"));
        }
    }
    Ok(a)
}

pub(super) fn targets(state: &AppState, a: &Args<'_>) -> R<Vec<ObjRef>> {
    let objs = objects(&state.doc, a)?;
    if a.get("ids").is_some() || a.get("solid_ids").is_some() {
        if objs.is_empty() {
            return Err("ids and solid_ids are empty".to_owned());
        }
        return Ok(objs);
    }
    let sel = state.doc.selected_objects();
    if sel.is_empty() {
        return Err("give ids (or solid_ids), or select objects first".to_owned());
    }
    Ok(sel)
}

pub(super) fn opt_solids(doc: &Document, a: &Args<'_>, key: &str) -> R<Option<Vec<u32>>> {
    if a.get(key).is_none() {
        return Ok(None);
    }
    let ids = a.ids(key)?;
    for id in &ids {
        if doc.scene().solid(*id).is_none() {
            return Err(format!("no solid has the id {id}"));
        }
    }
    Ok(Some(ids))
}

pub(super) fn target_solids(state: &AppState, a: &Args<'_>, key: &str) -> R<Vec<u32>> {
    if let Some(ids) = opt_solids(&state.doc, a, key)? {
        if ids.is_empty() {
            return Err(format!("{key} is empty"));
        }
        return Ok(ids);
    }
    let objs = targets(state, a)?;
    let solids = state.doc.scene().expand_solids(&objs);
    if solids.is_empty() {
        return Err("the objects have no solids".to_owned());
    }
    Ok(solids)
}

pub(super) fn entity_ids(doc: &Document, a: &Args<'_>, key: &str) -> R<Vec<u32>> {
    let ids = a.ids(key)?;
    for id in &ids {
        if doc.scene().entity(*id).is_none() {
            return Err(format!("no entity has the id {id}"));
        }
    }
    Ok(ids)
}

pub(super) fn target_entities(state: &AppState, a: &Args<'_>, key: &str) -> R<Vec<u32>> {
    if a.get(key).is_some() {
        let ids = entity_ids(&state.doc, a, key)?;
        if ids.is_empty() {
            return Err(format!("{key} is empty"));
        }
        return Ok(ids);
    }
    let sel = state.doc.selected_entity_ids();
    if sel.is_empty() {
        return Err(format!("give {key}, or select entities first"));
    }
    Ok(sel)
}

pub(super) fn entity_id(doc: &Document, a: &Args<'_>, key: &str) -> R<u32> {
    let id = a.id(key)?;
    if doc.scene().entity(id).is_none() {
        return Err(format!("no entity has the id {id}"));
    }
    Ok(id)
}

pub(super) fn solid_id(doc: &Document, a: &Args<'_>, key: &str) -> R<u32> {
    let id = a.id(key)?;
    if doc.scene().solid(id).is_none() {
        return Err(format!("no solid has the id {id}"));
    }
    Ok(id)
}

pub(super) fn opt_faces(doc: &Document, a: &Args<'_>, key: &str) -> R<Option<Vec<FaceRef>>> {
    let Some(list) = a.get(key) else { return Ok(None) };
    let list = list
        .as_array()
        .filter(|l| l.len() <= MAX_IDS)
        .ok_or_else(|| format!("{key} must be an array of at most {MAX_IDS} {{solid, side}} objects"))?;
    let mut out = Vec::with_capacity(list.len());
    for f in list {
        let fa = args(f, &["solid", "side"]).map_err(|e| format!("{key}: {e}"))?;
        let solid = fa.id("solid").map_err(|e| format!("{key}: {e}"))?;
        let side = fa.opt_u64("side", 0, 4096)?.ok_or_else(|| format!("{key}: side is required"))? as usize;
        let count =
            doc.scene().solid(solid).map(|s| s.solid.sides.len()).ok_or(format!("no solid has the id {solid}"))?;
        if side >= count {
            return Err(format!("solid {solid} has {count} sides (side {side} does not exist)"));
        }
        let face = FaceRef::new(solid, side);
        if !out.contains(&face) {
            out.push(face);
        }
    }
    Ok(Some(out))
}

pub(super) fn target_faces(state: &AppState, a: &Args<'_>, key: &str) -> R<Vec<FaceRef>> {
    if let Some(f) = opt_faces(&state.doc, a, key)? {
        if f.is_empty() {
            return Err(format!("{key} is empty"));
        }
        return Ok(f);
    }
    if a.get("ids").is_none() && a.get("solid_ids").is_none() {
        let sel = state.doc.selected_faces();
        if !sel.is_empty() {
            return Ok(sel);
        }
    }
    let objs = targets(state, a)?;
    let faces = state.doc.all_faces_of(&objs);
    if faces.is_empty() {
        return Err("the objects have no faces".to_owned());
    }
    Ok(faces)
}

pub(super) fn opt_axis(a: &Args<'_>, key: &str) -> R<Option<usize>> {
    Ok(a.opt_enum(key, &["x", "y", "z"])?.map(|s| match s.as_str() {
        "x" => 0,
        "y" => 1,
        _ => 2,
    }))
}

pub(super) fn opt_vec2(a: &Args<'_>, key: &str, min: f64, max: f64) -> R<Option<glam::DVec2>> {
    let Some(v) = a.get(key) else { return Ok(None) };
    let bad = || format!("{key} must be [a, b] with numbers from {min} to {max}");
    let arr = v.as_array().filter(|x| x.len() == 2).ok_or_else(bad)?;
    let x = arr[0].as_f64().filter(|f| f.is_finite() && (min..=max).contains(f)).ok_or_else(bad)?;
    let y = arr[1].as_f64().filter(|f| f.is_finite() && (min..=max).contains(f)).ok_or_else(bad)?;
    Ok(Some(glam::DVec2::new(x, y)))
}

pub(super) fn opt_i64(a: &Args<'_>, key: &str, min: i64, max: i64) -> R<Option<i64>> {
    let Some(v) = a.get(key) else { return Ok(None) };
    match v.as_i64().or_else(|| v.as_f64().filter(|f| f.fract() == 0.0 && f.abs() < 9.0e15).map(|f| f as i64)) {
        Some(n) if (min..=max).contains(&n) => Ok(Some(n)),
        _ => Err(format!("{key} must be an integer from {min} to {max}")),
    }
}

pub(super) fn opt_dir(a: &Args<'_>, key: &str) -> R<Option<DVec3>> {
    match a.opt_vec3(key)? {
        None => Ok(None),
        Some(d) => d.try_normalize().map(Some).ok_or_else(|| format!("{key} must not be zero")),
    }
}

pub(super) fn page<T>(items: Vec<T>, a: &Args<'_>) -> R<(Vec<T>, usize)> {
    let limit = a.opt_u64("limit", 1, MAX_LIST)?.unwrap_or(100) as usize;
    let offset = a.opt_u64("offset", 0, u64::from(u32::MAX))?.unwrap_or(0) as usize;
    let total = items.len();
    Ok((items.into_iter().skip(offset).take(limit).collect(), total))
}

pub(super) fn face_json(f: FaceRef) -> Value {
    json!({ "solid": f.solid, "side": f.side })
}

pub(super) fn bounds_fn(state: &AppState) -> impl Fn(&hp_vmf::Entity) -> Option<hp_geom::Aabb> + '_ {
    move |e| state.assets.entity_bounds(e)
}

pub(super) fn grid(state: &AppState) -> f64 {
    state.grid_size().max(1.0 / 64.0)
}

pub(super) fn relative_file(path: &str, ext: &str) -> R<String> {
    let p = path.trim().replace('\\', "/");
    let bad = |why: &str| Err(format!("invalid file \"{path}\": {why}"));
    if p.is_empty() || p.len() > 255 {
        return bad("1 to 255 characters");
    }
    if p.starts_with('/') || p.contains(':') {
        return bad("it must be relative (no drive, no leading slash)");
    }
    if p.chars().any(|c| c.is_control() || matches!(c, '"' | '<' | '>' | '|' | '?' | '*')) {
        return bad("forbidden character");
    }
    if p.split('/').any(|c| c.is_empty() || c == "." || c == ".." || c.ends_with(' ') || c.ends_with('.')) {
        return bad("empty, \".\" or \"..\" parts are not allowed");
    }
    let lower = p.to_ascii_lowercase();
    if lower.ends_with(&format!(".{ext}")) { Ok(p) } else { Ok(format!("{p}.{ext}")) }
}

pub(super) fn map_dir(state: &AppState) -> R<std::path::PathBuf> {
    state
        .doc
        .path()
        .and_then(|p| p.parent())
        .map(std::path::Path::to_path_buf)
        .ok_or_else(|| "save the map first (save_map with a name): the file goes next to it".to_owned())
}
