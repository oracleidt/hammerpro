use glam::DVec2;
use hp_app::{AppState, ViewKind};
use hp_editor::{EditError, ObjRef, SelectOp};
use hp_mcp::protocol::ToolResult;
use hp_tools_create::curve::{self, CurveMeta, Profile, Section, apply};
use serde_json::{Value, json};

use super::semantic::grid;
use super::tools::{Args, MAX_NAME, R, bounds_json, check_class, edit, material_name};

const PROFILE_NAMES: [&str; 3] = ["flat", "curb", "ditch"];

fn view_kind(name: &str) -> ViewKind {
    match name {
        "top" => ViewKind::Top,
        "front" => ViewKind::Front,
        _ => ViewKind::Side,
    }
}

fn view_name(kind: ViewKind) -> &'static str {
    match kind {
        ViewKind::Top => "top",
        ViewKind::Front => "front",
        _ => "side",
    }
}

fn depth_axis(kind: ViewKind) -> usize {
    match kind {
        ViewKind::Top => 2,
        ViewKind::Front => 0,
        _ => 1,
    }
}

fn section_args(a: &Args<'_>, base: Section) -> R<Section> {
    let mut s = base;
    if let Some(p) = a.opt_enum("profile", &PROFILE_NAMES)? {
        s.profile = Profile::parse(&p).ok_or("unknown profile")?;
    }
    if let Some(v) = a.opt_f64("curb_width", 1.0, curve::section::MAX_CURB_WIDTH)? {
        s.curb_width = v;
    }
    if let Some(v) = a.opt_f64("curb_height", 0.0, curve::section::MAX_CURB_HEIGHT)? {
        s.curb_height = v;
    }
    if let Some(v) = a.opt_f64("ditch_depth", 0.0, curve::section::MAX_DITCH_DEPTH)? {
        s.ditch_depth = v;
    }
    if let Some(v) = a.opt_f64("roll_start", -curve::section::MAX_ROLL, curve::section::MAX_ROLL)? {
        s.roll_start = v;
    }
    if let Some(v) = a.opt_f64("roll_end", -curve::section::MAX_ROLL, curve::section::MAX_ROLL)? {
        s.roll_end = v;
    }
    if let Some(v) = a.opt_f64("auto_bank", 0.0, curve::section::MAX_AUTO_GAIN)? {
        s.auto_gain = v;
    }
    if let Some(v) = a.opt_f64("auto_bank_limit", 0.0, curve::section::MAX_AUTO_LIMIT)? {
        s.auto_limit = v;
    }
    Ok(s)
}

fn points_arg(state: &AppState, a: &Args<'_>) -> R<Option<Vec<DVec2>>> {
    let Some(value) = a.get("points") else { return Ok(None) };
    let list = value.as_array().filter(|l| (2..=curve::MAX_CURVE_POINTS).contains(&l.len()));
    let list = list.ok_or_else(|| format!("points must be 2 to {} [a, b] points", curve::MAX_CURVE_POINTS))?;
    let snap = a.opt_bool("snap")?.unwrap_or(true);
    let g = grid(state);
    let mut points = Vec::with_capacity(list.len());
    for p in list {
        let ab: Vec<f64> = p
            .as_array()
            .filter(|x| x.len() == 2)
            .into_iter()
            .flatten()
            .filter_map(Value::as_f64)
            .filter(|f| f.is_finite() && f.abs() <= 16384.0)
            .collect();
        if ab.len() != 2 {
            return Err("points must contain [a, b] points with numbers from -16384 to 16384".to_owned());
        }
        let p = DVec2::new(ab[0], ab[1]);
        points.push(if snap { curve::snap_point(p, g) } else { p });
    }
    Ok(Some(points))
}

fn lightmap_scale(state: &AppState) -> i32 {
    match state.settings.edit.default_lightmap_scale {
        s if s > 0 => s,
        _ => 16,
    }
}

fn meta_json(meta: &CurveMeta) -> Value {
    let s = &meta.section;
    json!({
        "view": view_name(meta.kind),
        "points": meta.points.iter().map(|p| [p.x, p.y]).collect::<Vec<_>>(),
        "depth": [meta.depth.0, meta.depth.1],
        "subdivisions": meta.segments,
        "thickness": meta.thickness,
        "tex_scale": meta.tex_scale,
        "profile": s.profile.name(),
        "curb_width": s.curb_width,
        "curb_height": s.curb_height,
        "ditch_depth": s.ditch_depth,
        "roll_start": s.roll_start,
        "roll_end": s.roll_end,
        "auto_bank": s.auto_gain,
        "auto_bank_limit": s.auto_limit,
    })
}

fn invalid(e: apply::ApplyError) -> EditError {
    match e {
        apply::ApplyError::Edit(e) => e,
        other => EditError::Invalid(other.to_string()),
    }
}

pub(super) fn create_curve_brushes(state: &mut AppState, a: &Args<'_>) -> R<ToolResult> {
    let view = a.opt_enum("view", &["top", "front", "side"])?.ok_or("view is required")?;
    let kind = view_kind(&view);
    let points = points_arg(state, a)?
        .ok_or_else(|| format!("points must be 2 to {} [a, b] points", curve::MAX_CURVE_POINTS))?;
    let segments = a.opt_u64("subdivisions", 2, 4096)?.map_or(16, |n| n as u32);
    let thickness = a.opt_f64("thickness", 1.0, 1024.0)?.unwrap_or(16.0);
    let (auto_lo, auto_hi) =
        hp_tools_create::new_depth_range(state, state.runtime.brush.last_created, depth_axis(kind));
    let lo = a.opt_f64("base", -16384.0, 16384.0)?.unwrap_or(auto_lo);
    let height = a.opt_f64("height", 1.0, 16384.0)?.unwrap_or(auto_hi - auto_lo);
    let material = match a.opt_str("material", MAX_NAME)? {
        Some(m) => material_name(m)?,
        None => state.current_material.clone(),
    };
    let class = a.opt_str("entity_class", MAX_NAME)?.map(|c| c.trim().to_owned());
    if let Some(c) = &class {
        check_class(c)?;
    }
    let tex_scale = a.opt_f64("tex_scale", 0.01, 64.0)?.unwrap_or_else(|| {
        let s = state.settings.edit.default_texture_scale;
        if s.is_finite() && s.abs() > 1e-6 { s } else { hp_geom::DEFAULT_TEXTURE_SCALE }
    });
    let section = section_args(a, Section::default())?.clamped();
    let meta = CurveMeta { kind, points, depth: (lo, lo + height), segments, thickness, tex_scale, section };
    let layout = curve::layout(&meta.points, segments, section.total_width(thickness)).map_err(|e| e.to_string())?;
    layout.check().map_err(|e| e.to_string())?;
    let lightmap = lightmap_scale(state);
    let created = edit(state, "create_curve_brushes", |doc| {
        let target = match &class {
            Some(c) => apply::Target::NewEntity { class: c, keyvalues: &[] },
            None => apply::Target::World,
        };
        apply::create_curve(doc, &meta, &material, lightmap, target).map_err(invalid)
    })?;
    finish(state, &meta, &created, &material, layout.length)
}

fn finish(
    state: &mut AppState,
    meta: &CurveMeta,
    created: &apply::Created,
    material: &str,
    length: f64,
) -> R<ToolResult> {
    let bounds =
        created.ids.iter().filter_map(|id| state.doc.scene().solid(*id).map(|n| n.bounds)).reduce(|a, b| a.union(&b));
    state.runtime.brush.last_created = bounds;
    let mut out = meta_json(meta);
    if let Value::Object(map) = &mut out {
        map.insert("solid_ids".to_owned(), json!(created.ids));
        map.insert("entity_id".to_owned(), json!(created.entity));
        map.insert("count".to_owned(), json!(created.ids.len()));
        map.insert("length".to_owned(), json!(length));
        map.insert("material".to_owned(), json!(material));
        map.insert("bounds".to_owned(), bounds_json(bounds));
    }
    Ok(ToolResult::json(out))
}

fn piece_arg(state: &AppState, a: &Args<'_>) -> R<u32> {
    if let Some(id) = a.opt_u64("solid_id", 1, u64::from(u32::MAX))? {
        let id = id as u32;
        if state.doc.curve_tag(id).is_some() {
            return Ok(id);
        }
        if let Some(found) =
            state.doc.scene().entity_solid_ids(id).into_iter().find(|s| state.doc.curve_tag(*s).is_some())
        {
            return Ok(found);
        }
        return Err(format!("{id} is not a brush made by the curve tool"));
    }
    state.doc.selected_curve_piece().ok_or_else(|| {
        "give a solid_id of a curve brush, or select one made by the curve tool or create_curve_brushes".to_owned()
    })
}

pub(super) fn get_curve_brushes(state: &mut AppState, a: &Args<'_>) -> R<ToolResult> {
    let piece = piece_arg(state, a)?;
    let (record, meta) = apply::read_curve(&state.doc, piece).ok_or("this curve has no readable curve data")?;
    let mut out = meta_json(&meta);
    if let Value::Object(map) = &mut out {
        map.insert("solid_ids".to_owned(), json!(record.ids));
        let entity = match record.owner {
            hp_editor::Owner::Entity(e) => Some(e),
            hp_editor::Owner::World => None,
        };
        map.insert("entity_id".to_owned(), json!(entity));
    }
    Ok(ToolResult::json(out))
}

pub(super) fn edit_curve_brushes(state: &mut AppState, a: &Args<'_>) -> R<ToolResult> {
    let piece = piece_arg(state, a)?;
    let (record, mut meta) = apply::read_curve(&state.doc, piece).ok_or("this curve has no readable curve data")?;
    if let Some(points) = points_arg(state, a)? {
        meta.points = points;
    }
    if let Some(n) = a.opt_u64("subdivisions", 2, 4096)? {
        meta.segments = n as u32;
    }
    if let Some(t) = a.opt_f64("thickness", 1.0, 1024.0)? {
        meta.thickness = t;
    }
    let height = a.opt_f64("height", 1.0, 16384.0)?.unwrap_or(meta.depth.1 - meta.depth.0);
    let base = a.opt_f64("base", -16384.0, 16384.0)?.unwrap_or(meta.depth.0);
    meta.depth = (base, base + height);
    if let Some(t) = a.opt_f64("tex_scale", 0.01, 64.0)? {
        meta.tex_scale = t;
    }
    meta.section = section_args(a, meta.section)?.clamped();
    let material = match a.opt_str("material", MAX_NAME)? {
        Some(m) => material_name(m)?,
        None => apply::material_of(&state.doc, &record).unwrap_or_else(|| state.current_material.clone()),
    };
    let layout = curve::layout(&meta.points, meta.segments, meta.section.total_width(meta.thickness))
        .map_err(|e| e.to_string())?;
    layout.check().map_err(|e| e.to_string())?;
    let lightmap = lightmap_scale(state);
    let created = edit(state, "edit_curve_brushes", |doc| {
        let made = apply::replace_curve(doc, &record, &meta, &material, lightmap).map_err(invalid)?;
        if made.entity.is_none() {
            let refs: Vec<ObjRef> = made.ids.iter().map(|id| ObjRef::Solid(*id)).collect();
            doc.select(&refs, SelectOp::Replace);
        }
        Ok(made)
    })?;
    finish(state, &meta, &created, &material, layout.length)
}
