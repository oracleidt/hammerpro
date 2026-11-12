use std::sync::atomic::AtomicBool;

use hp_app::AppState;
use hp_editor::optimize::detail::DetailKind;
use hp_mcp::protocol::ToolResult;
use serde_json::{Value, json};

use super::semantic::*;
use super::tools::{ApplyFn, Args, Outcome, R, Worker, undo_label, v3};
use crate::dispatch::brush::func_detail::{
    self as fd, Begin, Checked, MakeError, Policy, filter_from, pick, select_picked, selection_world_solids, split_of,
};

domain! { "func detail";
    make_func_detail: Edit, "Make func_detail",
        "Turn world brushes (ids / solid_ids, or the selection) into func_detail in one undo step, like the Make \
        func_detail action (Ctrl+Alt+D). split \"selection\" (default: the setting brush.detail.split) merges them \
        into one func_detail, \"cluster\" makes one func_detail per group of touching brushes; brushes of different \
        groups always go to different func_detail so the groups stay intact, and a func_detail of the selection \
        receives the new brushes. Visgroups and textures are kept. Brushes already in a func_detail stay, brushes of \
        other brush entities and displacements are skipped (reported). Before converting, the leak flood checks \
        which brushes seal the map (part of the outer shell between the playable space and the void, a func_detail \
        does not seal): sealing \"skip\" (default) converts the others and reports the sealing ones, \"refuse\" \
        converts nothing when one seals, \"allow\" converts everything without checking. When the map already leaks \
        the check cannot run: everything is converted and verified is false. Returns the func_detail entity ids, \
        the converted and refused solid ids.",
        {
            "ids": ids("Solids or brush entities to convert (default: the selection)."),
            "solid_ids": ids("Solid ids (when an entity has the same id)."),
            "split": choice("selection (one func_detail) or cluster (one per group of touching brushes).", &["selection", "cluster"]),
            "sealing": choice("skip (default), refuse or allow.", &["skip", "refuse", "allow"]),
        }, [];
    select_detail_candidates: Editor, "Select func_detail candidates",
        "Select the world brushes that should be func_detail, with the rules of the Map > Optimize advisor (small \
        and decorative pieces, steps and stairs, trims, pillars, rubble, complex shapes, never part of the hull: \
        verified with the leak flood so that converting them all never opens the map). Filters default to the \
        settings brush.detail.* of the Select func_detail candidates window. Nothing in the map changes: review the \
        selection, then call make_func_detail (or press Make func_detail). Returns each candidate (solid, kind, \
        size, center), how many sealing brushes were left out, and whether the check was verified (false when the \
        map leaks).",
        {
            "max_size": number("Largest size on any axis, in units (default brush.detail.max_size, 1024).", 16.0, 16384.0),
            "kinds": strings("Only these kinds: step (stairs), trim, pillar, small (rubble), complex.", 5),
            "within": ids("Only look at these solids (default: the whole map)."),
            "within_selection": boolean("Only look inside the current selection (default brush.detail.in_selection when something is selected)."),
            "select": boolean("Replace the selection with the candidates (default true)."),
            "limit": limit(),
        }, [];
}

fn make_func_detail(state: &mut AppState, a: &Args<'_>) -> R<Outcome> {
    let objs = targets(state, a)?;
    let split = match a.opt_enum("split", &["selection", "cluster"])? {
        Some(s) => hp_editor::DetailSplit::parse(&s).ok_or("bad split")?,
        None => split_of(state.settings.brush.detail.split),
    };
    let policy =
        a.opt_enum("sealing", &["skip", "refuse", "allow"])?.and_then(|s| Policy::parse(&s)).unwrap_or(Policy::Skip);
    match fd::begin(state, &objs, split, policy != Policy::Allow) {
        Begin::Nothing(plan) => Err(format!(
            "no world brush to convert ({} already func_detail, {} in other brush entities, {} displacements)",
            plan.already.len(),
            plan.other_entity.len(),
            plan.displacements.len()
        )),
        Begin::Ready(c) => Ok(finish(state, &c, policy).into()),
        Begin::Check(p) => Ok(Worker::spawn_then("make_func_detail", move || {
            let c = p.run(&AtomicBool::new(false));
            let apply: ApplyFn = Box::new(move |state: &mut AppState| Some(finish(state, &c, policy)));
            (ToolResult::error("make_func_detail: no result"), Some(apply))
        })),
    }
}

fn finish(state: &mut AppState, c: &Checked, policy: Policy) -> ToolResult {
    let label = undo_label("make_func_detail");
    let check = c.check.clone().unwrap_or_default();
    match fd::apply(state, c, policy, Some(&label)) {
        Ok(made) => ToolResult::json(json!({
            "func_detail": made.entities,
            "converted": made.converted,
            "refused_sealing": made.refused,
            "already_func_detail": c.plan.already,
            "other_brush_entity": c.plan.other_entity,
            "displacements": c.plan.displacements,
            "sealing_checked": c.check.is_some(),
            "verified": check.verified,
            "map_leaks": check.map_leaks,
            "leak_check_incomplete": check.incomplete,
            "split": c.split.id(),
        })),
        Err(e @ MakeError::AllSealing(_)) => {
            let ids = if let MakeError::AllSealing(ids) = &e { ids.clone() } else { Vec::new() };
            ToolResult::error(format!("make_func_detail: {} ({ids:?})", e.english()))
        }
        Err(e) => ToolResult::error(format!("make_func_detail: {}", e.english())),
    }
}

fn select_detail_candidates(state: &mut AppState, a: &Args<'_>) -> R<Outcome> {
    let mut s = state.settings.brush.detail.clone();
    if let Some(m) = a.opt_f64("max_size", 16.0, 16384.0)? {
        s.max_size = m;
    }
    let kinds = a.strings("kinds", 5)?;
    if !kinds.is_empty() {
        let parsed: Vec<DetailKind> = kinds
            .iter()
            .map(|k| {
                DetailKind::parse(k).ok_or_else(|| format!("unknown kind '{k}' (step, trim, pillar, small, complex)"))
            })
            .collect::<R<_>>()?;
        s.steps = parsed.contains(&DetailKind::Step);
        s.trims = parsed.contains(&DetailKind::Trim);
        s.pillars = parsed.contains(&DetailKind::Pillar);
        s.small = parsed.contains(&DetailKind::Small);
        s.complex = parsed.contains(&DetailKind::Complex);
    }
    let has_selection = !state.doc.selected_objects().is_empty();
    let within = if a.get("within").is_some() {
        Some(a.ids("within")?)
    } else if a.opt_bool("within_selection")?.unwrap_or(s.in_selection && has_selection) {
        if !has_selection {
            return Err("within_selection: nothing is selected".to_owned());
        }
        Some(selection_world_solids(state))
    } else {
        None
    };
    let select = a.opt_bool("select")?.unwrap_or(true);
    let limit = a.opt_u64("limit", 1, hp_mcp::tools::MAX_LIST)?.unwrap_or(100) as usize;
    let filter = filter_from(&s, within);
    let (job, revision) = fd::candidate_job(state, &filter);
    Ok(Worker::spawn_then("select_detail_candidates", move || {
        let suggestion = job.run(None, &AtomicBool::new(false));
        let apply: ApplyFn = Box::new(move |state: &mut AppState| {
            if state.doc.scene_revision() != revision {
                return Some(ToolResult::error(
                    "select_detail_candidates: the map changed during the check: run it again",
                ));
            }
            let picked = pick(state, &suggestion);
            if select {
                select_picked(state, &picked);
            }
            let list: Vec<Value> = suggestion
                .candidates
                .iter()
                .filter(|c| picked.ids.contains(&c.id))
                .take(limit)
                .map(|c| json!({ "solid": c.id, "kind": c.kind.id(), "size": v3(c.size), "center": v3(c.center) }))
                .collect();
            Some(ToolResult::json(json!({
                "count": picked.ids.len(),
                "selected": select && !picked.ids.is_empty(),
                "candidates": list,
                "kept_for_sealing": picked.kept_for_sealing,
                "verified": picked.verified,
                "map_leaks": picked.map_leaks,
                "leak_check_incomplete": picked.incomplete,
                "filters": {
                    "max_size": filter.max_size,
                    "kinds": filter.kinds.iter().map(|k| k.id()).collect::<Vec<_>>(),
                    "within": filter.within.as_ref().map(Vec::len),
                },
                "note": "nothing was changed in the map: call make_func_detail to convert the selection",
            })))
        });
        (ToolResult::error("select_detail_candidates: no result"), Some(apply))
    }))
}

#[cfg(test)]
mod tests {
    use glam::DVec3;
    use hp_editor::{Document, ObjRef, Owner};
    use serde_json::json;

    use super::super::tools::{Outcome, run};

    fn call(state: &mut hp_app::AppState, name: &str, args: serde_json::Value) -> ToolResult {
        match run(state, name, &args) {
            Outcome::Done(r) => r,
            Outcome::Worker(w) => w.wait(state, std::time::Duration::from_secs(60)),
            _ => panic!("{name}: unexpected outcome"),
        }
    }

    use hp_mcp::protocol::ToolResult;

    fn box_sides(min: DVec3, max: DVec3) -> Vec<hp_vmf::Side> {
        let (x0, y0, z0, x1, y1, z1) = (min.x, min.y, min.z, max.x, max.y, max.z);
        let v = DVec3::new;
        [
            [v(x0, y1, z1), v(x1, y1, z1), v(x1, y0, z1)],
            [v(x0, y0, z0), v(x1, y0, z0), v(x1, y1, z0)],
            [v(x1, y1, z1), v(x0, y1, z1), v(x0, y1, z0)],
            [v(x1, y0, z0), v(x0, y0, z0), v(x0, y0, z1)],
            [v(x1, y1, z0), v(x1, y0, z0), v(x1, y0, z1)],
            [v(x0, y1, z1), v(x0, y0, z1), v(x0, y0, z0)],
        ]
        .into_iter()
        .map(|plane| hp_vmf::Side {
            plane,
            material: "BRICK/WALL".to_owned(),
            uaxis: hp_vmf::TexAxis { axis: DVec3::X, shift: 0.0, scale: 0.25 },
            vaxis: hp_vmf::TexAxis { axis: DVec3::NEG_Y, shift: 0.0, scale: 0.25 },
            ..Default::default()
        })
        .collect()
    }

    fn room(state: &mut hp_app::AppState) -> (Vec<u32>, u32, u32) {
        let mut doc = Document::new_empty();
        let (s, w) = (256.0, 16.0);
        let wall =
            |doc: &mut Document, a: DVec3, b: DVec3| doc.create_solid(box_sides(a, b), Owner::World).expect("wall");
        let walls = vec![
            wall(&mut doc, DVec3::new(-s - w, -s - w, -s - w), DVec3::new(-s, s + w, s + w)),
            wall(&mut doc, DVec3::new(s, -s - w, -s - w), DVec3::new(s + w, s + w, s + w)),
            wall(&mut doc, DVec3::new(-s, -s - w, -s - w), DVec3::new(s, -s, s + w)),
            wall(&mut doc, DVec3::new(-s, s, -s - w), DVec3::new(s, s + w, s + w)),
            wall(&mut doc, DVec3::new(-s, -s, -s - w), DVec3::new(s, s, -s)),
            wall(&mut doc, DVec3::new(-s, -s, s), DVec3::new(s, s, s + w)),
        ];
        let pillar = wall(&mut doc, DVec3::new(-232.0, 120.0, -256.0), DVec3::new(-168.0, 184.0, 0.0));
        let step = wall(&mut doc, DVec3::new(100.0, -200.0, -256.0), DVec3::new(132.0, 100.0, -240.0));
        doc.create_point_entity("info_player_start", DVec3::new(150.0, 150.0, -240.0), &[]).expect("spawn");
        doc.clear_history();
        state.doc = doc;
        (walls, pillar, step)
    }

    #[test]
    fn make_func_detail_skips_refuses_or_allows_sealing_brushes() {
        hp_app::register_translations();
        let mut state = hp_app::testing::test_state();
        let (walls, pillar, step) = room(&mut state);
        let r = call(&mut state, "make_func_detail", json!({ "solid_ids": [pillar, walls[2]], "sealing": "refuse" }));
        assert!(r.is_error, "{}", r.first_text());
        assert_eq!(state.doc.scene().solid(pillar).expect("pillar").owner, Owner::World);
        let r = call(&mut state, "make_func_detail", json!({ "solid_ids": [pillar, walls[2]] }));
        assert!(!r.is_error, "{}", r.first_text());
        let v = r.structured.clone().expect("json");
        assert_eq!(v["converted"], json!([pillar]));
        assert_eq!(v["refused_sealing"], json!([walls[2]]));
        assert_eq!(v["verified"], json!(true));
        assert_eq!(state.doc.undo_label(), Some("MCP: make_func_detail"));
        assert!(matches!(state.doc.scene().solid(pillar).expect("pillar").owner, Owner::Entity(_)));
        assert_eq!(state.doc.scene().solid(walls[2]).expect("wall").owner, Owner::World);
        let r = call(&mut state, "make_func_detail", json!({ "solid_ids": [walls[2]] }));
        assert!(r.is_error, "{}", r.first_text());
        let r = call(
            &mut state,
            "make_func_detail",
            json!({ "solid_ids": [walls[2], step], "sealing": "allow", "split": "cluster" }),
        );
        assert!(!r.is_error, "{}", r.first_text());
        let v = r.structured.clone().expect("json");
        assert_eq!(v["sealing_checked"], json!(false));
        assert_eq!(v["func_detail"].as_array().map(Vec::len), Some(2));
        let r = call(&mut state, "make_func_detail", json!({ "solid_ids": [pillar] }));
        assert!(r.is_error && r.first_text().contains("already"), "{}", r.first_text());
    }

    #[test]
    fn select_detail_candidates_selects_and_filters_without_editing() {
        hp_app::register_translations();
        let mut state = hp_app::testing::test_state();
        let (walls, pillar, step) = room(&mut state);
        let r = call(&mut state, "select_detail_candidates", json!({}));
        assert!(!r.is_error, "{}", r.first_text());
        let v = r.structured.clone().expect("json");
        assert_eq!(v["verified"], json!(true));
        let sel = state.doc.selected_objects();
        assert!(sel.contains(&ObjRef::Solid(pillar)) && sel.contains(&ObjRef::Solid(step)));
        assert!(!walls.iter().any(|w| sel.contains(&ObjRef::Solid(*w))));
        assert!(state.doc.undo_label().is_none());
        let r = call(&mut state, "select_detail_candidates", json!({ "kinds": ["pillars"], "select": false }));
        let v = r.structured.clone().expect("json");
        assert_eq!(v["count"], json!(1));
        assert_eq!(v["candidates"][0]["solid"], json!(pillar));
        let r = call(&mut state, "select_detail_candidates", json!({ "kinds": ["nope"] }));
        assert!(r.is_error);
    }
}
