use hp_app::AppState;
use hp_editor::{FaceRef, Problem};
use hp_mcp::protocol::ToolResult;
use serde_json::{Value, json};

use super::semantic::*;
use hp_mcp::tools::MAX_LIST;

use super::tools::{Args, MAX_NAME, Outcome, R, Worker, bounds_json, edit, obj_json};

domain! { "map health";
    get_problems: Read, "Problems",
        "The problems of the map (like Check for problems, with the background checks: limits, leaks, entity \
        budget when they ran): severity, code, message, objects, faces, fixable. Filters by severity, code \
        pattern and object.",
        {
            "severity": choice("Least severity (default info).", &["info", "warning", "error"]),
            "code": string("Code pattern, for example \"missing_*\"."),
            "object": integer("Only problems about this entity or solid id.", 1, i64::from(u32::MAX)),
            "limit": limit(), "offset": offset(),
        }, [];
    fix_problems: Edit, "Fix problems",
        "Fix the fixable problems (all, or those with these codes, or these indices of get_problems) in one step.",
        {
            "codes": strings("Problem codes to fix.", 64),
            "indices": { "type": "array", "items": { "type": "integer", "minimum": 0 }, "maxItems": 10000, "description": "Indices from get_problems (no filters)." },
            "all": boolean("Every fixable problem."),
        }, [];
    get_map_limits: Read, "Engine limits",
        "How much of each engine limit the map uses (brushes, sides, planes, entities, overlays, lightmaps...) with \
        the biggest users, estimated by the background check or exact from the compiled BSP, and the entity \
        (edict) budget.",
        { "top": integer("Biggest users listed per limit (default 5).", 0, 50) }, [];
    get_map_stats: Read, "Map statistics",
        "Counts of the map: solids, world solids, faces, entities (point and brush), displacements, materials, \
        groups, visgroups, invalid solids, bounds, and the undo history size.",
        {}, [];
    find_hidden_faces: Read, "Find hidden faces",
        "Faces entirely covered by other brushes (they could get nodraw with apply_nodraw). Computed on a worker \
        thread.",
        { "limit": integer("Most faces returned (default 1000).", 1, 100_000) }, [];
    find_duplicate_brushes: Edit, "Find redundant brushwork",
        "Redundant objects of the map: brushes that are exact copies of another brush of the same entity (same \
        vertices within 1/16 unit, any face order and texture), brushes fully inside an identical-content solid \
        brush of the same entity, and point or brush entities stacked on an identical one (same class, model, \
        angles and origin). Lists each copy with the object it duplicates; select selects the copies, delete \
        deletes them and keeps the originals (one undo step).",
        {
            "kinds": strings("Kinds to look for: brushes, contained, entities (default all three).", 3),
            "select": boolean("Select the redundant copies (default false)."),
            "delete": boolean("Delete the redundant copies (default false)."),
            "limit": limit(),
        }, [];
    recheck_map: Editor, "Check the map again",
        "Run the background problem and limit checks again now (get_problems shows the result when checking is \
        false).",
        {}, [];
}

fn severity_rank(s: hp_editor::Severity) -> u8 {
    match s {
        hp_editor::Severity::Info => 0,
        hp_editor::Severity::Warning => 1,
        hp_editor::Severity::Error => 2,
    }
}

fn severity_name(s: hp_editor::Severity) -> &'static str {
    ["info", "warning", "error"][severity_rank(s) as usize]
}

pub(super) fn current_problems(state: &mut AppState) -> (Vec<Problem>, &'static str) {
    let serial = state.doc.content_serial();
    let fresh = state.runtime.health.checks.as_ref().is_some_and(|c| c.serial == serial);
    if fresh {
        return (state.runtime.health.problems.clone(), "background check");
    }
    hp_app::runtime::HealthRuntime::check_now(state);
    (state.runtime.health.problems.clone(), "checked now")
}

fn problem_json(i: usize, p: &Problem) -> Value {
    json!({
        "index": i,
        "severity": severity_name(p.severity),
        "code": p.code,
        "message": p.message,
        "objects": p.objects.iter().take(50).map(|o| obj_json(*o)).collect::<Vec<_>>(),
        "object_count": p.objects.len(),
        "faces": p.faces.iter().take(50).map(|f| face_json(*f)).collect::<Vec<_>>(),
        "fixable": p.fixable,
    })
}

fn get_problems(state: &mut AppState, a: &Args<'_>) -> R<ToolResult> {
    let least = match a.opt_enum("severity", &["info", "warning", "error"])?.as_deref() {
        Some("error") => 2,
        Some("warning") => 1,
        _ => 0,
    };
    let code = a.opt_str("code", MAX_NAME)?.map(str::to_owned);
    let object = a.opt_u64("object", 1, u64::from(u32::MAX))?.map(|n| n as u32);
    let (problems, source) = current_problems(state);
    let total_all = problems.len();
    let kept: Vec<Value> = problems
        .iter()
        .enumerate()
        .filter(|(_, p)| severity_rank(p.severity) >= least)
        .filter(|(_, p)| code.as_deref().is_none_or(|c| hp_editor::glob_match(c, p.code)))
        .filter(|(_, p)| {
            object.is_none_or(|id| p.objects.iter().any(|o| o.id() == id) || p.faces.iter().any(|f| f.solid == id))
        })
        .map(|(i, p)| problem_json(i, p))
        .collect();
    let (kept, total) = page(kept, a)?;
    let h = &state.runtime.health;
    Ok(ToolResult::json(json!({
        "source": source,
        "checking": h.checking,
        "counts": { "errors": h.counts.errors, "warnings": h.counts.warnings, "infos": h.counts.infos, "ignored": h.counts.ignored },
        "all_problems": total_all,
        "total": total,
        "problems": kept,
    })))
}

fn fix_problems(state: &mut AppState, a: &Args<'_>) -> R<ToolResult> {
    let codes = a.strings("codes", 64)?;
    let all = a.opt_bool("all")?.unwrap_or(false);
    let indices: Option<Vec<usize>> = match a.get("indices") {
        None => None,
        Some(v) => Some(
            v.as_array()
                .filter(|l| l.len() <= 10_000)
                .ok_or("indices must be an array of integers")?
                .iter()
                .map(|x| x.as_u64().map(|n| n as usize).ok_or("indices must be an array of integers"))
                .collect::<Result<_, _>>()?,
        ),
    };
    let given = usize::from(all) + usize::from(!codes.is_empty()) + usize::from(indices.is_some());
    if given != 1 {
        return Err("give exactly one of all, codes or indices".to_owned());
    }
    let (problems, _) = current_problems(state);
    let chosen: Vec<Problem> = match &indices {
        Some(ix) => {
            if let Some(bad) = ix.iter().find(|i| **i >= problems.len()) {
                return Err(format!(
                    "index {bad} does not exist ({} problems; call get_problems again)",
                    problems.len()
                ));
            }
            ix.iter().map(|i| problems[*i].clone()).collect()
        }
        None => {
            problems.into_iter().filter(|p| all || codes.iter().any(|c| hp_editor::glob_match(c, p.code))).collect()
        }
    };
    let fixable: Vec<Problem> = chosen.into_iter().filter(|p| p.fixable).collect();
    if fixable.is_empty() {
        return Ok(ToolResult::json(json!({ "fixed": 0, "note": "nothing fixable among these problems" })));
    }
    let n = edit(state, "fix_problems", |doc| doc.fix_problems(&fixable))?;
    state.runtime.health.recheck = true;
    Ok(ToolResult::json(json!({ "fixed": n, "asked": fixable.len() })))
}

const DUPLICATE_KINDS: [(&str, &str); 3] = [
    ("brushes", hp_editor::duplicates::CODE_BRUSH),
    ("contained", hp_editor::duplicates::CODE_CONTAINED),
    ("entities", hp_editor::duplicates::CODE_ENTITY),
];

fn find_duplicate_brushes(state: &mut AppState, a: &Args<'_>) -> R<ToolResult> {
    let kinds = a.strings("kinds", 3)?;
    if let Some(bad) = kinds.iter().find(|k| !DUPLICATE_KINDS.iter().any(|(name, _)| name == k)) {
        return Err(format!("unknown kind '{bad}' (brushes, contained, entities)"));
    }
    let select = a.opt_bool("select")?.unwrap_or(false);
    let delete = a.opt_bool("delete")?.unwrap_or(false);
    if select && delete {
        return Err("give select or delete, not both".to_owned());
    }
    let limit = a.opt_u64("limit", 1, MAX_LIST)?.unwrap_or(100) as usize;
    let (problems, source) = current_problems(state);
    let mut items: Vec<Value> = Vec::new();
    let mut objs: Vec<hp_editor::ObjRef> = Vec::new();
    let mut counts = serde_json::Map::new();
    for (name, code) in DUPLICATE_KINDS {
        if !kinds.is_empty() && !kinds.iter().any(|k| k == name) {
            continue;
        }
        let mut n = 0;
        for p in problems.iter().filter(|p| p.code == code) {
            for (i, o) in p.objects.iter().enumerate() {
                let of = p.arg(i).split_once('=').and_then(|(_, keep)| keep.parse::<u32>().ok());
                items.push(json!({ "kind": name, "copy": obj_json(*o), "of": of }));
                objs.push(*o);
                n += 1;
            }
        }
        counts.insert(name.to_owned(), json!(n));
    }
    let total = items.len();
    let mut out = json!({ "source": source, "total": total, "counts": counts, "items": items.into_iter().take(limit).collect::<Vec<_>>() });
    if delete && !objs.is_empty() {
        let n = edit(state, "find_duplicate_brushes", |doc| doc.delete_redundant(&objs))?;
        state.runtime.health.recheck = true;
        out["deleted"] = json!(n);
    } else if select {
        state.doc.set_selection(&objs);
        out["selected"] = json!(objs.len());
    }
    Ok(ToolResult::json(out))
}

fn limit_json(u: &hp_editor::limits::LimitUse, top: usize) -> Value {
    json!({
        "limit": u.limit.id(),
        "used": u.used,
        "max": u.max,
        "fraction": (u.fraction() * 1000.0).round() / 1000.0,
        "top": u.top.iter().take(top).map(|(o, n)| json!({ "object": obj_json(*o), "uses": n })).collect::<Vec<_>>(),
    })
}

fn get_map_limits(state: &mut AppState, a: &Args<'_>) -> R<ToolResult> {
    let top = a.opt_u64("top", 0, 50)?.unwrap_or(5) as usize;
    let h = &state.runtime.health;
    let estimated = h.checks.as_ref().map(|c| {
        json!({
            "up_to_date": c.serial == state.doc.revision(),
            "limits": c.limits.iter().map(|u| limit_json(u, top)).collect::<Vec<_>>(),
            "edicts": {
                "used": c.edicts.used,
                "left": c.edicts.left(),
                "by_class": c.edicts.by_class.iter().take(30).map(|(k, n)| json!({ "classname": k, "count": n })).collect::<Vec<_>>(),
            },
            "ms": c.ms,
        })
    });
    let exact = h.exact.as_ref().map(|e| {
        json!({
            "bsp": e.path.display().to_string(),
            "limits": e.uses.iter().map(|u| limit_json(u, top)).collect::<Vec<_>>(),
            "areas": e.areas,
            "clusters": e.clusters,
        })
    });
    Ok(ToolResult::json(json!({
        "profile": format!("{:?}", h.profile).to_ascii_lowercase(),
        "checking": h.checking,
        "estimated": estimated,
        "exact_from_bsp": exact,
        "note": if estimated.is_none() { "The background check has not run yet: call recheck_map, then again." } else { "" },
    })))
}

fn get_map_stats(state: &mut AppState, _a: &Args<'_>) -> R<ToolResult> {
    let s = state.doc.stats();
    let (undo, redo) = state.doc.history_len();
    Ok(ToolResult::json(json!({
        "solids": s.solids,
        "world_solids": s.world_solids,
        "faces": s.faces,
        "entities": s.entities,
        "point_entities": s.point_entities,
        "brush_entities": s.brush_entities,
        "displacements": s.displacements,
        "unique_materials": s.unique_materials,
        "groups": s.groups,
        "visgroups": s.visgroups,
        "invalid_solids": s.invalid_solids,
        "bounds": bounds_json(s.bounds),
        "undo_steps": undo,
        "redo_steps": redo,
        "history_bytes": state.doc.history_bytes(),
        "unsaved_changes": state.doc.is_dirty(),
    })))
}

fn find_hidden_faces(state: &mut AppState, a: &Args<'_>) -> R<Outcome> {
    let limit = a.opt_u64("limit", 1, 100_000)?.unwrap_or(1000) as usize;
    let (ids, solids) = state.doc.hidden_face_scan();
    Ok(Worker::spawn("find_hidden_faces", move || {
        let found = hp_geom::hidden_faces(&solids);
        let mut faces: Vec<FaceRef> = found
            .iter()
            .filter_map(|h| {
                let id = *ids.get(h.solid)?;
                let face = solids.get(h.solid)?.geom.faces.get(h.face)?;
                Some(FaceRef { solid: id, side: face.side_index })
            })
            .collect();
        faces.sort();
        faces.dedup();
        let total = faces.len();
        ToolResult::json(json!({
            "total": total,
            "faces": faces.into_iter().take(limit).map(face_json).collect::<Vec<_>>(),
            "note": "apply_nodraw with these faces gives them nodraw (one undo step).",
        }))
    }))
}

fn recheck_map(state: &mut AppState, _a: &Args<'_>) -> R<ToolResult> {
    state.runtime.health.recheck = true;
    Ok(ToolResult::json(json!({ "requested": true })))
}

#[cfg(test)]
mod tests {
    use serde_json::json;

    use super::super::semantic_tests::{err, ok};

    #[test]
    fn problems_limits_stats_and_hidden_faces() {
        let mut s = hp_app::testing::test_state();
        let p = ok(&mut s, "get_problems", json!({}));
        assert_eq!(p["source"], "checked now");
        let all = p["all_problems"].as_u64().unwrap();
        let e = ok(&mut s, "get_problems", json!({ "severity": "error" }));
        assert!(e["total"].as_u64().unwrap() <= all);
        err(&mut s, "fix_problems", json!({}));
        err(&mut s, "fix_problems", json!({ "all": true, "codes": ["x"] }));
        err(&mut s, "fix_problems", json!({ "indices": [100000] }));
        ok(&mut s, "fix_problems", json!({ "all": true }));
        let l = ok(&mut s, "get_map_limits", json!({}));
        assert!(!l["estimated"].is_null());
        let st = ok(&mut s, "get_map_stats", json!({}));
        assert_eq!(st["solids"].as_u64().unwrap() as usize, s.doc.stats().solids);
        ok(&mut s, "create_primitive", json!({ "min": [0, 0, 0], "max": [64, 64, 64] }));
        let h = ok(&mut s, "find_hidden_faces", json!({}));
        assert!(h["total"].is_number());
        ok(&mut s, "recheck_map", json!({}));
        assert!(s.runtime.health.recheck);
    }
}
