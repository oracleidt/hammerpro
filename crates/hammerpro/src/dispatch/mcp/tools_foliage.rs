use glam::DVec3;
use hp_app::AppState;
use hp_app::foliage::{
    DENSITY_RANGE, FoliageItem, FoliagePalette, FoliageSettings, FoliageStroke, MAX_ITEMS, MAX_PALETTES, MAX_POINTS,
    RADIUS_RANGE, apply_erase, apply_paint, layer_ids, surface_below,
};
use hp_mcp::protocol::ToolResult;
use serde_json::{Value, json};

use super::semantic::*;
use super::tools::{Args, R, undo_label};

const MAX_NAME: usize = 256;
const MAX_REPORTED_IDS: usize = 2000;

domain! { "foliage";
    paint_foliage: Edit, "Paint foliage",
        "Paint props (prop_static by default, or prop_dynamic / prop_physics) with the Foliage tool's brush along a \
        stroke: every point is one dab of the brush (radius), traced straight down from above the whole map (or \
        from the point when from_above is false) onto displacements, world brushes, func_detail and, with \
        on_props, onto other props (grass on rocks), hit on their real mesh. Each dab fills the disc up to the \
        density (props per 256 x 256 units, counting the props of this palette's layer already there, so going \
        over the same place again adds nothing), with the palette models picked by weight, random scale \
        (scale_min to scale_max, written to modelscale), random yaw, align (0 upright to 1 along the surface \
        normal), slope_min and slope_max in degrees, optional height_min and height_max (world Z), spacing (least \
        distance between props of the layer, Poisson disk), avoid_overlap (the real model footprints, from the \
        mesh, of painted and existing props never overlap), drop_to_surface (the real mesh is lowered onto the real surfaces until it touches: \
        no floating, no sinking; a prop whose contact patch is not supported everywhere, over an edge, a hole or a \
        slope outside the limits, is not placed and counted in rejected.unsupported), random_skin and sink (offset along up, negative \
        sinks). The palette: palette names a saved palette of the tool (settings placement.foliage.palettes), or \
        items gives the models (model, weight, classname, enabled) with palette as the name; save true stores it \
        for the tool. Painted props join the visgroup \"Foliage: <palette>\" (created when missing). Options not \
        given use the tool's settings. With preview true nothing is kept: the answer counts what would be placed. \
        One undo step.",
        {
            "points": points_schema("Stroke points, one dab each ([x, y, z])."),
            "radius": number("Brush radius (default: the tool's).", 4.0, 8192.0),
            "palette": string("Palette name (a saved palette, or the name of the items given)."),
            "items": items_schema(),
            "save": boolean("Store the items as a palette of the tool (default false)."),
            "density": number("Props per 256 x 256 units.", 0.01, 500.0),
            "spacing": number("Least distance between two props of the layer.", 0.0, 4096.0),
            "scale_min": number("Smallest random scale.", 0.01, 100.0),
            "scale_max": number("Largest random scale.", 0.01, 100.0),
            "random_yaw": boolean("Random yaw."),
            "align": number("Lean to the surface normal, 0 to 1.", 0.0, 1.0),
            "slope_min": number("Least slope in degrees.", 0.0, 180.0),
            "slope_max": number("Steepest slope in degrees.", 0.0, 180.0),
            "height_min": number("Lowest world Z (turns the height limit on).", -131072.0, 131072.0),
            "height_max": number("Highest world Z (turns the height limit on).", -131072.0, 131072.0),
            "avoid_overlap": boolean("Keep the real footprints apart."),
            "drop_to_surface": boolean("Rest the real mesh on the surface."),
            "random_skin": boolean("Random skin of the model."),
            "on_props": boolean("Paint on other props too."),
            "sink": number("Offset along up, negative sinks.", -4096.0, 4096.0),
            "seed": integer("Random seed.", 0, i64::from(u32::MAX)),
            "from_above": boolean("Trace each point down from above the map (default true)."),
            "preview": boolean("Only count, keep nothing (default false)."),
        }, ["points"];
    erase_foliage: Edit, "Erase foliage",
        "Remove painted props under the Foliage brush along a stroke (each point one dab of radius, traced down \
        like paint_foliage): the props of the layer \"Foliage: <palette>\" of the named palette, or of every \
        foliage layer when palette is omitted, or every prop under the brush with any_prop. One undo step.",
        {
            "points": points_schema("Stroke points, one dab each ([x, y, z])."),
            "radius": number("Brush radius (default: the tool's).", 4.0, 8192.0),
            "palette": string("Only this palette's layer (default: every foliage layer)."),
            "any_prop": boolean("Remove every prop under the brush, painted or not (default false)."),
            "from_above": boolean("Trace each point down from above the map (default true)."),
        }, ["points"];
}

fn points_schema(description: &str) -> Value {
    json!({
        "type": "array",
        "items": { "type": "array", "items": { "type": "number" }, "minItems": 3, "maxItems": 3 },
        "minItems": 1,
        "maxItems": MAX_POINTS,
        "description": description,
    })
}

fn items_schema() -> Value {
    json!({
        "type": "array",
        "minItems": 1,
        "maxItems": MAX_ITEMS,
        "items": {
            "type": "object",
            "properties": {
                "model": { "type": "string", "description": "Model path." },
                "weight": { "type": "number", "minimum": 0.001, "maximum": 1000 },
                "classname": { "type": "string", "enum": ["prop_static", "prop_dynamic", "prop_physics", "prop_dynamic_override", "prop_physics_multiplayer"] },
                "enabled": { "type": "boolean" },
            },
            "required": ["model"],
            "additionalProperties": false,
        },
        "description": "Palette models: model, weight (default 1), classname (default prop_static), enabled.",
    })
}

fn points(a: &Args<'_>) -> R<Vec<DVec3>> {
    let list = a.get("points").and_then(Value::as_array).filter(|l| !l.is_empty() && l.len() <= MAX_POINTS);
    let list = list.ok_or_else(|| format!("points must be 1 to {MAX_POINTS} [x, y, z] points"))?;
    list.iter()
        .map(|c| {
            let xyz: Vec<f64> = c
                .as_array()
                .filter(|x| x.len() == 3)
                .into_iter()
                .flatten()
                .filter_map(Value::as_f64)
                .filter(|f| f.is_finite() && f.abs() <= 131072.0)
                .collect();
            if xyz.len() == 3 {
                Ok(DVec3::new(xyz[0], xyz[1], xyz[2]))
            } else {
                Err("points must contain [x, y, z] points with numbers from -131072 to 131072".to_owned())
            }
        })
        .collect()
}

fn items(a: &Args<'_>) -> R<Option<Vec<FoliageItem>>> {
    let Some(list) = a.get("items") else { return Ok(None) };
    let list =
        list.as_array().filter(|l| !l.is_empty() && l.len() <= MAX_ITEMS).ok_or("items must be 1 to 64 objects")?;
    list.iter()
        .map(|v| {
            let ia = super::tools::args(v, &["model", "weight", "classname", "enabled"])
                .map_err(|e| format!("items: {e}"))?;
            let model = ia.str("model", MAX_NAME)?.trim();
            if model.is_empty() {
                return Err("items: model is empty".to_owned());
            }
            let mut item = FoliageItem::new(model);
            item.weight = ia.opt_f64("weight", 0.001, 1000.0)?.unwrap_or(1.0);
            if let Some(c) = ia.opt_str("classname", 64)? {
                item.classname = c.trim().to_owned();
            }
            item.enabled = ia.opt_bool("enabled")?.unwrap_or(true);
            Ok(item)
        })
        .collect::<R<Vec<_>>>()
        .map(Some)
}

fn settings_from(state: &AppState, a: &Args<'_>) -> R<FoliageSettings> {
    let mut s = state.settings.placement.foliage.clone();
    if let Some(r) = a.opt_f64("radius", *RADIUS_RANGE.start(), *RADIUS_RANGE.end())? {
        s.radius = r;
    }
    if let Some(v) = a.opt_f64("density", *DENSITY_RANGE.start(), *DENSITY_RANGE.end())? {
        s.density = v;
    }
    let nums: [(&str, f64, f64, &mut f64); 9] = [
        ("spacing", 0.0, 4096.0, &mut s.spacing),
        ("scale_min", 0.01, 100.0, &mut s.scale_min),
        ("scale_max", 0.01, 100.0, &mut s.scale_max),
        ("align", 0.0, 1.0, &mut s.align),
        ("slope_min", 0.0, 180.0, &mut s.slope_min),
        ("slope_max", 0.0, 180.0, &mut s.slope_max),
        ("height_min", -131072.0, 131072.0, &mut s.height_min),
        ("height_max", -131072.0, 131072.0, &mut s.height_max),
        ("sink", -4096.0, 4096.0, &mut s.sink),
    ];
    for (key, lo, hi, slot) in nums {
        if let Some(v) = a.opt_f64(key, lo, hi)? {
            *slot = v;
        }
    }
    if a.get("scale_min").is_some() && a.get("scale_max").is_none() {
        s.scale_max = s.scale_min;
    }
    if a.get("height_min").is_some() || a.get("height_max").is_some() {
        s.height_limit = true;
        if a.get("height_min").is_none() {
            s.height_min = -131072.0;
        }
        if a.get("height_max").is_none() {
            s.height_max = 131072.0;
        }
    }
    let flags: [(&str, &mut bool); 5] = [
        ("random_yaw", &mut s.random_yaw),
        ("avoid_overlap", &mut s.avoid_overlap),
        ("drop_to_surface", &mut s.drop_to_surface),
        ("random_skin", &mut s.random_skin),
        ("on_props", &mut s.on_props),
    ];
    for (key, slot) in flags {
        if let Some(v) = a.opt_bool(key)? {
            *slot = v;
        }
    }
    let name = a.opt_str("palette", MAX_NAME)?.map(str::trim).filter(|n| !n.is_empty());
    match (items(a)?, name) {
        (Some(list), name) => {
            let palette =
                FoliagePalette { name: name.unwrap_or(hp_app::foliage::DEFAULT_PALETTE).to_owned(), items: list };
            match s.palettes.iter().position(|p| p.name.eq_ignore_ascii_case(&palette.name)) {
                Some(i) => {
                    s.palettes[i] = palette;
                    s.active = i;
                }
                None => {
                    if s.palettes.len() >= MAX_PALETTES {
                        return Err(format!("the tool holds at most {MAX_PALETTES} palettes"));
                    }
                    s.palettes.push(palette);
                    s.active = s.palettes.len() - 1;
                }
            }
        }
        (None, Some(n)) => {
            let i = s.palettes.iter().position(|p| p.name.eq_ignore_ascii_case(n)).ok_or_else(|| {
                let names: Vec<&str> = s.palettes.iter().map(|p| p.name.as_str()).collect();
                format!("no palette \"{n}\" (palettes: {}); give items to make one", names.join(", "))
            })?;
            s.active = i;
        }
        (None, None) => {}
    }
    if s.palette().is_none_or(|p| !p.items.iter().any(FoliageItem::usable)) {
        return Err("the palette has no usable model: give items [{\"model\": ...}] or a palette name".to_owned());
    }
    s.validate()?;
    Ok(s)
}

fn dab_center(state: &AppState, p: DVec3, from_above: bool) -> Option<(DVec3, DVec3)> {
    let start = if from_above {
        let top = state.doc.scene().solids().values().map(|n| n.bounds.max.z).fold(p.z, f64::max);
        DVec3::new(p.x, p.y, top + 64.0)
    } else {
        p + DVec3::Z * 64.0
    };
    surface_below(state, start, 1.0).map(|h| (h.point, h.normal))
}

fn paint_foliage(state: &mut AppState, a: &Args<'_>) -> R<ToolResult> {
    let pts = points(a)?;
    let settings = settings_from(state, a)?;
    let from_above = a.opt_bool("from_above")?.unwrap_or(true);
    let preview = a.opt_bool("preview")?.unwrap_or(false);
    let seed = a.opt_u64("seed", 0, u64::from(u32::MAX))?.unwrap_or_else(|| state.doc.revision() ^ 0x0F01_1A6E);
    if state.doc.in_transaction() {
        return Err("Hammer Pro is in the middle of another edit (a drag in a view?); try again".to_owned());
    }
    state.doc.begin(undo_label("paint_foliage"));
    let mut stroke = FoliageStroke::begin(state, &settings, false, seed);
    let (mut dabs, mut missed) = (0usize, 0usize);
    for p in &pts {
        let Some((c, n)) = dab_center(state, *p, from_above) else {
            missed += 1;
            continue;
        };
        dabs += 1;
        if let Err(e) = apply_paint(state, &mut stroke, c, n) {
            state.doc.cancel();
            return Err(e);
        }
    }
    let placed = stroke.placed_ids.len();
    let layer = stroke.palette.layer_name();
    let group = stroke.layer_groups().first().copied();
    if preview {
        state.doc.cancel();
    } else {
        state.doc.commit();
        if a.opt_bool("save")?.unwrap_or(false) {
            state.settings.placement.foliage.palettes.clone_from(&settings.palettes);
            state.settings.placement.foliage.active = settings.active;
            state.settings_dirty = true;
        }
    }
    let st = &stroke.stats;
    let ids: Vec<u32> =
        if preview { Vec::new() } else { stroke.placed_ids.iter().take(MAX_REPORTED_IDS).copied().collect() };
    Ok(ToolResult::json(json!({
        "placed": if preview { 0 } else { placed },
        "would_place": placed,
        "preview": preview,
        "palette": stroke.palette.name,
        "layer": layer,
        "visgroup": if preview { None } else { group },
        "dabs": dabs,
        "points_without_surface": missed,
        "ids": ids,
        "ids_truncated": !preview && placed > MAX_REPORTED_IDS,
        "models": stroke.model_report().iter().map(|(m, mesh, r, h)| json!({
            "model": m,
            "real_mesh": mesh,
            "footprint_radius": (r * 100.0).round() / 100.0,
            "height": (h * 100.0).round() / 100.0,
        })).collect::<Vec<_>>(),
        "rejected": {
            "no_surface": st.missed,
            "slope": st.slope,
            "height": st.height,
            "spacing": st.spacing,
            "overlap": st.overlap,
            "unsupported": st.support,
        },
    })))
}

fn erase_foliage(state: &mut AppState, a: &Args<'_>) -> R<ToolResult> {
    let pts = points(a)?;
    let mut settings = state.settings.placement.foliage.clone();
    if let Some(r) = a.opt_f64("radius", *RADIUS_RANGE.start(), *RADIUS_RANGE.end())? {
        settings.radius = r;
    }
    let any = a.opt_bool("any_prop")?.unwrap_or(false);
    let name = a.opt_str("palette", MAX_NAME)?.map(str::trim).filter(|n| !n.is_empty()).map(str::to_owned);
    let groups = layer_ids(&state.doc, name.as_deref());
    if !any && groups.is_empty() {
        return Err(match name {
            Some(n) => format!("there is no layer \"{}\" in the map", hp_app::foliage::layer_name(&n)),
            None => {
                "the map has no foliage layer (visgroups \"Foliage: ...\"); use any_prop to erase every prop".to_owned()
            }
        });
    }
    let from_above = a.opt_bool("from_above")?.unwrap_or(true);
    if state.doc.in_transaction() {
        return Err("Hammer Pro is in the middle of another edit (a drag in a view?); try again".to_owned());
    }
    let palette_name = name.clone().unwrap_or_default();
    settings.palettes = vec![FoliagePalette { name: palette_name, items: Vec::new() }];
    settings.active = 0;
    state.doc.begin(undo_label("erase_foliage"));
    let mut stroke = FoliageStroke::begin_with_groups(state, &settings, true, 1, groups);
    stroke.erase_all_layers = any;
    let mut removed = 0usize;
    for p in &pts {
        let Some((c, n)) = dab_center(state, *p, from_above) else { continue };
        match apply_erase(state, &mut stroke, c, n) {
            Ok(k) => removed += k,
            Err(e) => {
                state.doc.cancel();
                return Err(e);
            }
        }
    }
    state.doc.commit();
    Ok(ToolResult::json(json!({ "removed": removed, "any_prop": any, "palette": name })))
}

#[cfg(test)]
mod tests {
    use serde_json::json;

    use super::super::semantic_tests::{err, ok};

    fn items() -> serde_json::Value {
        json!([{ "model": "models/props_foliage/bush.mdl", "weight": 2 }, { "model": "models/props_junk/rock001a.mdl" }])
    }

    #[test]
    fn paint_then_erase_in_one_undo_step_each() {
        let mut state = hp_app::testing::test_state();
        let before = state.doc.stats().entities;
        let p = ok(
            &mut state,
            "paint_foliage",
            json!({ "points": [[0, 0, 0], [100, 0, 0]], "radius": 160, "density": 12, "palette": "Hill", "items": items(), "seed": 3 }),
        );
        let placed = p["placed"].as_u64().unwrap() as usize;
        assert!(placed > 5, "{p}");
        assert_eq!(p["layer"], "Foliage: Hill");
        assert_eq!(state.doc.stats().entities, before + placed);
        assert_eq!(state.doc.undo_label(), Some("MCP: paint_foliage"));
        let preview = ok(
            &mut state,
            "paint_foliage",
            json!({ "points": [[0, -150, 0]], "palette": "Hill", "items": items(), "preview": true }),
        );
        assert_eq!(preview["placed"], 0);
        assert_eq!(state.doc.stats().entities, before + placed);
        let gone = ok(&mut state, "erase_foliage", json!({ "points": [[0, 0, 0]], "radius": 160, "palette": "Hill" }));
        let removed = gone["removed"].as_u64().unwrap() as usize;
        assert!(removed > 0 && removed <= placed, "{gone}");
        assert!(state.doc.undo());
        assert_eq!(state.doc.stats().entities, before + placed);
        let all = ok(&mut state, "erase_foliage", json!({ "points": [[0, 0, 0], [100, 0, 0]], "radius": 400 }));
        assert_eq!(all["removed"].as_u64().unwrap() as usize, placed);
    }

    #[test]
    fn options_are_checked() {
        let mut state = hp_app::testing::test_state();
        assert!(err(&mut state, "paint_foliage", json!({ "points": [[0, 0, 0]] })).contains("no usable model"));
        assert!(
            err(&mut state, "paint_foliage", json!({ "points": [[0, 0, 0]], "palette": "nope" }))
                .contains("no palette")
        );
        assert!(err(&mut state, "erase_foliage", json!({ "points": [[0, 0, 0]] })).contains("no foliage layer"));
        let saved = ok(
            &mut state,
            "paint_foliage",
            json!({ "points": [[0, 0, 0]], "palette": "Trees", "items": items(), "save": true, "density": 1 }),
        );
        assert_eq!(saved["palette"], "Trees");
        assert!(state.settings.placement.foliage.palettes.iter().any(|p| p.name == "Trees"));
    }
}
