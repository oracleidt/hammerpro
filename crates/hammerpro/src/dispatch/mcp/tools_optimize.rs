use std::collections::HashMap;
use std::sync::atomic::AtomicBool;

use hp_app::AppState;
use hp_editor::optimize::areaportals::{self, PortalSuggestion};
use hp_editor::optimize::detail::DetailKind;
use hp_editor::optimize::vis::VisReport;
use hp_editor::optimize::{Finding, Topic};
use hp_editor::{Id, ObjRef};
use hp_geom::Aabb;
use hp_mcp::protocol::ToolResult;
use serde_json::{Value, json};

use super::semantic::*;
use super::tools::{Args, Outcome, R, Worker, bounds_json, edit, obj_json, undo_label, v3};
use crate::dispatch::health::optimize::{self as engine, Analysis, Level, Options};

domain! { "optimization";
    optimize_map: Edit, "Optimize the map",
        "Run every optimization check of the map in the background (a BSP and visibility compile of the current map, \
        func_detail candidates verified against leaks, hint brushes measured by compiling, areaportal checks and \
        suggestions, light coherence) and return one report sorted by impact, each finding with what to do. apply \
        \"safe\" applies the safe fixes in one undo step (verified func_detail, measured hints, door areaportals, \
        areaportal repairs, blocked, redundant and broken lights, a missing light_environment); \"all\" also moves \
        floating lights to their lamps, adds lights at lamps without light, harmonizes lamp colours and puts \
        areaportals in doorless openings. measure compiles again with the fixes and returns the visleaf and PVS \
        numbers before and after. Run it before finishing a map. On a big map the call can answer running: true \
        while it goes on in the background: call it again with the same arguments.",
        {
            "apply": choice("none (default: report only), safe or all.", &["none", "safe", "all"]),
            "hints": boolean("Measure hint brush candidates by compiling (default true)."),
            "max_hints": integer("Hint candidates measured (default 8).", 0, 32),
            "vis": choice("Visibility used to measure: auto (default: full on maps up to 4000 brushes, fast above), full (exact) or fast (quicker, conservative).", &["auto", "full", "fast"]),
            "measure": boolean("Compile again with the fixes and report before and after (default true when apply is not none)."),
            "limit": integer("Findings returned (default 60).", 1, 1000),
        }, [];
    analyze_visibility: Read, "Analyze visibility",
        "Compile the BSP, portals and visibility of the current map in the background (nothing is written next to the \
        map) and report visleaves (leaves, clusters, portals, areas with their leaf counts), the PVS (total, average, \
        largest), the estimated VVIS cost, the worst visleaves (most visible leaves, triangles drawn), tiny fragmented \
        leaves, world brushes that cut leaves and should be func_detail, and concrete fixes. show displays the leaves \
        and portals in the views (like show_visleaves). On a big map the call can answer running: true: call it \
        again with the same arguments.",
        {
            "vis": choice("auto (default), full or fast.", &["auto", "full", "fast"]),
            "show": boolean("Show the visleaves and portals of this analysis in the 2D and 3D views."),
            "top": integer("Worst visleaves listed (default 10).", 0, 50),
        }, [];
    suggest_func_detail: Edit, "Suggest func_detail",
        "World brushes that should be func_detail (small, steps, trims, pillars, rubble and complex shapes that are \
        not part of the hull), verified with the leak check so turning them into func_detail never opens the map. \
        apply converts them (or only ids / kinds) into one func_detail in one undo step. measure compiles before \
        and after to give the visleaf counts.",
        {
            "apply": boolean("Convert the candidates to func_detail (default false)."),
            "ids": ids("Only these candidate solid ids."),
            "kinds": strings("Only these kinds: step, trim, pillar, small, complex.", 5),
            "measure": boolean("Compile before and after to count visleaves (default false)."),
            "limit": limit(),
        }, [];
    suggest_hint_brushes: Edit, "Suggest hint brushes",
        "Hint brush planes that split the worst visleaves well, found from the portals (the top of doorways, the \
        edges of openings at corridor corners), each measured by compiling the map with it: gain is how many fewer \
        visible leaf pairs it gives. apply adds the hints with a positive gain (hint face, skip on the other faces) in \
        one undo step.",
        {
            "apply": boolean("Add the hints that measured a gain (default false)."),
            "max_candidates": integer("Candidates measured (default 8).", 1, 32),
            "vis": choice("auto (default), full or fast.", &["auto", "full", "fast"]),
        }, [];
    show_visleaves: Editor, "Show visleaves",
        "Show or hide the visleaves and portals of the last visibility analysis (analyze_visibility or optimize_map) \
        in the 2D and 3D views, coloured per leaf, optionally as a heat map of the triangles drawn from each leaf; \
        look at them with screenshot_view.",
        {
            "show": boolean("Show (true) or hide (false); default toggles."),
            "heat": boolean("Colour the portals by the triangles drawn from each leaf."),
            "xray": boolean("Draw the portals through the walls in 3D."),
            "leaf_grid": boolean("Show the 1024 unit grid where VBSP always cuts leaves."),
        }, [];
    check_areaportals: Edit, "Check areaportals",
        "Validate every func_areaportal and func_areaportalwindow: one brush, thin, in a narrow opening, touching the \
        world on all four sides, not touching another areaportal, linked to a door that exists with the same start \
        state, and really separating two areas (from a background compile). fix repairs what can be repaired in one \
        undo step (start state, door link, fit to the opening, delete useless ones).",
        {
            "fix": boolean("Repair the fixable problems (default false)."),
            "codes": strings("Only fix these codes.", 16),
            "compile": boolean("Compile in the background to know the areas (default true)."),
        }, [];
    suggest_areaportals: Edit, "Suggest areaportals",
        "Openings that should get an areaportal: door frames without one, and doorless openings that are the only \
        way between two groups of visleaves (from the portals of a background compile), each box fitted exactly to \
        the opening. apply places them (all, or indices) in one undo step; door portals get linked to their door.",
        {
            "apply": boolean("Place the suggested areaportals (default false)."),
            "indices": { "type": "array", "items": { "type": "integer", "minimum": 0 }, "maxItems": 1000, "description": "Indices of the suggestions to place." },
            "include_open": boolean("Also suggest doorless openings (default true)."),
        }, [];
    audit_lights: Edit, "Audit lights",
        "Check that the lights are coherent: lights with no lamp near them, lamp props, light textures and sprites \
        with no light, lights inside brushes or props, lights of identical lamps with a different colour or \
        brightness, redundant lights, spot lights aimed into walls, too many switchable lights, a missing \
        light_environment, broken falloff. fix applies the fixes (all, or kinds: unblock, move_to_fixture, \
        add_light, harmonize, merge, delete, falloff, add_sun) in one undo step.",
        {
            "fix": boolean("Apply the fixes (default false)."),
            "kinds": strings("Only these fix kinds.", 8),
            "limit": integer("Findings returned (default 100).", 1, 1000),
        }, [];
}

fn vis_options(a: &Args<'_>, o: Options) -> R<Options> {
    Ok(match a.opt_enum("vis", &["auto", "full", "fast"])?.as_deref() {
        Some("full") => Options { full_vis: true, auto_vis: false, ..o },
        Some("fast") => Options { full_vis: false, auto_vis: false, ..o },
        _ => Options { full_vis: true, auto_vis: true, ..o },
    })
}

fn finding_json(i: usize, f: &Finding) -> Value {
    json!({
        "index": i,
        "topic": f.topic.id(),
        "code": f.code,
        "impact": f.impact.id(),
        "message": f.message,
        "objects": f.objects.iter().take(100).map(|o| obj_json(*o)).collect::<Vec<_>>(),
        "object_count": f.objects.len(),
        "points": f.points.iter().take(16).map(|p| v3(*p)).collect::<Vec<_>>(),
        "boxes": f.boxes.iter().take(8).map(|b| bounds_json(Some(*b))).collect::<Vec<_>>(),
        "fix": f.fix,
    })
}

fn vis_json(r: &VisReport, top: usize) -> Value {
    json!({
        "leaves": r.leaves,
        "empty_leaves": r.empty_leaves,
        "visleaves": r.clusters,
        "portals": r.portals,
        "areas": r.areas,
        "area_leaves": r.area_stats.iter().map(|a| json!({ "area": a.area, "visleaves": a.clusters, "leaves": a.leaves })).collect::<Vec<_>>(),
        "has_vis": r.has_vis,
        "total_pvs": r.total_pvs,
        "average_pvs": (r.average_pvs * 10.0).round() / 10.0,
        "max_pvs": r.max_pvs,
        "vis_cost": r.vis_work,
        "average_triangles_drawn": r.average_triangles.round(),
        "worst": r.worst.iter().take(top).map(|w| json!({
            "visleaf": w.cluster,
            "area": w.area,
            "center": v3(w.bounds.center()),
            "bounds": bounds_json(Some(w.bounds)),
            "visible_leaves": w.visible,
            "portals": w.portals,
            "triangles_drawn": w.triangles,
        })).collect::<Vec<_>>(),
        "tiny_leaves": r.small_clusters,
        "areaportals": r.areaportals.iter().map(|p| json!({ "entity": p.entity, "portal_number": p.number, "areas": p.areas, "window": p.window })).collect::<Vec<_>>(),
        "compile_warnings": r.log_warnings,
    })
}

fn compare(before: &VisReport, after: &VisReport) -> Value {
    let pct = |b: f64, a: f64| if b > 0.0 { ((b - a) / b * 1000.0).round() / 10.0 } else { 0.0 };
    json!({
        "visleaves": [before.clusters, after.clusters],
        "leaves": [before.leaves, after.leaves],
        "portals": [before.portals, after.portals],
        "areas": [before.areas, after.areas],
        "total_pvs": [before.total_pvs, after.total_pvs],
        "average_pvs": [(before.average_pvs * 10.0).round() / 10.0, (after.average_pvs * 10.0).round() / 10.0],
        "vis_cost": [before.vis_work, after.vis_work],
        "visleaves_saved_percent": pct(f64::from(before.clusters), f64::from(after.clusters)),
        "pvs_saved_percent": pct(before.total_pvs as f64, after.total_pvs as f64),
        "vis_cost_saved_percent": pct(before.vis_work as f64, after.vis_work as f64),
    })
}

fn door_models(state: &AppState) -> HashMap<Id, Aabb> {
    state
        .doc
        .scene()
        .entities()
        .values()
        .filter(|n| areaportals::is_door_class(&n.entity.classname))
        .filter_map(|n| state.assets.entity_bounds(&n.entity).map(|b| (n.entity.id, b)))
        .collect()
}

fn changed(state: &AppState, a: &Analysis) -> bool {
    state.doc.scene_revision() != a.revision
}

const AXES: [&str; 3] = ["x", "y", "z"];

const CHANGED: &str = "the map changed while the analysis ran: nothing applied, run it again";

type Then = Box<dyn FnOnce(&mut AppState, Analysis) -> ToolResult + Send>;

const WAIT: std::time::Duration = std::time::Duration::from_secs(100);
const MAX_PENDING: usize = 4;

struct Pending {
    key: String,
    rx: std::sync::mpsc::Receiver<Analysis>,
    started: std::time::Instant,
}

static PENDING: std::sync::Mutex<Vec<Pending>> = std::sync::Mutex::new(Vec::new());

fn args_key(a: &Args<'_>) -> String {
    serde_json::to_string(&a.map).unwrap_or_default()
}

fn take_pending(key: &str) -> Option<Pending> {
    let mut list = PENDING.lock().ok()?;
    let i = list.iter().position(|p| p.key == key)?;
    Some(list.remove(i))
}

fn park(p: Pending) {
    if let Ok(mut list) = PENDING.lock() {
        while list.len() >= MAX_PENDING {
            list.remove(0);
        }
        list.push(p);
    }
}

fn spawn(tool: &'static str, state: &AppState, args: String, doors: bool, o: Options, then: Then) -> Outcome {
    let key = format!("{tool} {args} {}", state.doc.scene_revision());
    let pending = match take_pending(&key) {
        Some(p) => p,
        None => {
            let p = engine::prepare(state, doors);
            let (tx, rx) = std::sync::mpsc::channel();
            let spawned = std::thread::Builder::new().name(format!("hp-{tool}")).spawn(move || {
                let r = std::panic::catch_unwind(std::panic::AssertUnwindSafe(|| {
                    engine::run(p, o, &AtomicBool::new(false))
                }));
                if let Ok(a) = r {
                    let _ = tx.send(a);
                }
            });
            if let Err(e) = spawned {
                return ToolResult::error(format!("{tool}: cannot start a worker thread: {e}")).into();
            }
            Pending { key, rx, started: std::time::Instant::now() }
        }
    };
    Worker::spawn_then(tool, move || match pending.rx.recv_timeout(WAIT) {
        Ok(a) => {
            let placeholder = ToolResult::error(format!("{tool}: no result"));
            let apply: super::tools::ApplyFn = Box::new(move |state: &mut AppState| Some(then(state, a)));
            (placeholder, Some(apply))
        }
        Err(std::sync::mpsc::RecvTimeoutError::Timeout) => {
            let elapsed = pending.started.elapsed().as_secs();
            park(pending);
            let note = format!(
                "The map is big: the analysis is still running in the background ({elapsed} s so far). Call {tool} \
                 again with the same arguments, without editing the map, to get the result."
            );
            (ToolResult::json(json!({ "running": true, "elapsed_s": elapsed, "note": note })), None)
        }
        Err(std::sync::mpsc::RecvTimeoutError::Disconnected) => {
            (ToolResult::error(format!("{tool}: the analysis stopped (see the Hammer Pro log)")), None)
        }
    })
}

fn optimize_map(state: &mut AppState, a: &Args<'_>) -> R<Outcome> {
    let level = match a.opt_enum("apply", &["none", "safe", "all"])?.as_deref() {
        Some("safe") => Some(Level::Safe),
        Some("all") => Some(Level::All),
        _ => None,
    };
    let hints = a.opt_bool("hints")?.unwrap_or(true);
    let max_hints = a.opt_u64("max_hints", 0, 32)?.unwrap_or(8) as usize;
    let measure = a.opt_bool("measure")?.unwrap_or(level.is_some());
    let limit = a.opt_u64("limit", 1, 1000)?.unwrap_or(60) as usize;
    let o = vis_options(
        a,
        Options {
            hint_evals: if hints { max_hints } else { 0 },
            measure_after: measure,
            after_doors: true,
            ..Options::default()
        },
    )?;
    let models = door_models(state);
    Ok(spawn(
        "optimize_map",
        state,
        args_key(a),
        true,
        o,
        Box::new(move |state, an| {
            let c = engine::compose(state, &an, true);
            let mut out = json!({
                "time_ms": an.ms,
                "leaked": an.leaked,
                "compile_error": an.error(),
                "visibility": an.report().map(|r| vis_json(r, 5)),
                "counts": {
                    "high": c.report.count(hp_editor::optimize::Impact::High),
                    "medium": c.report.count(hp_editor::optimize::Impact::Medium),
                    "low": c.report.count(hp_editor::optimize::Impact::Low),
                },
                "findings": c.report.findings.iter().take(limit).enumerate().map(|(i, f)| finding_json(i, f)).collect::<Vec<_>>(),
                "total_findings": c.report.findings.len(),
            });
            if let Some(n) = &an.hints_note {
                out["hints_note"] = json!(n);
            }
            if let (Some(before), Some(after)) = (an.report(), an.after.as_ref()) {
                out["before_after"] = match after {
                    Ok(r) => compare(before, r),
                    Err(e) => json!({ "error": e }),
                };
                out["before_after_note"] = json!(
                    "measured with the verified func_detail, the measured hints and the door areaportals applied"
                );
            }
            if let Some(level) = level {
                if changed(state, &an) {
                    out["applied"] = json!({ "error": CHANGED });
                } else {
                    let model = move |e: &hp_vmf::Entity| models.get(&e.id).copied();
                    let label = undo_label("optimize_map");
                    let r = state.doc.transaction(label, |doc| {
                        Ok::<_, hp_editor::EditError>(engine::apply(doc, &an, &c, level, &model))
                    });
                    out["applied"] = match r {
                        Ok(x) => json!({
                            "func_detail_brushes": x.func_detail,
                            "hint_brushes": x.hints,
                            "areaportals_placed": x.areaportals_placed,
                            "areaportals_repaired": x.areaportals_fixed,
                            "light_changes": x.lights_fixed,
                            "errors": x.errors,
                            "undo": "one undo step: MCP: optimize_map",
                        }),
                        Err(e) => json!({ "error": e.to_string() }),
                    };
                    state.runtime.health.recheck = true;
                }
            }
            engine::store(state, &an, c.report);
            ToolResult::json(out)
        }),
    ))
}

fn analyze_visibility(state: &mut AppState, a: &Args<'_>) -> R<Outcome> {
    let show = a.opt_bool("show")?.unwrap_or(false);
    let top = a.opt_u64("top", 0, 50)?.unwrap_or(10) as usize;
    let o = vis_options(a, Options::default())?;
    Ok(spawn(
        "analyze_visibility",
        state,
        args_key(a),
        false,
        o,
        Box::new(move |state, an| {
            let c = engine::compose(state, &an, false);
            let vis: Vec<Value> = c
                .report
                .findings
                .iter()
                .filter(|f| matches!(f.topic, Topic::Visleaves | Topic::Detail | Topic::Hints))
                .enumerate()
                .map(|(i, f)| finding_json(i, f))
                .collect();
            let detail = an.detail.as_ref().map(|d| {
                json!({
                    "candidates": d.candidates.len(),
                    "leaves_cut": d.leaves_touched(),
                    "verified_against_leaks": d.verified,
                    "by_kind": DetailKind::ALL.iter().map(|k| (k.id().to_owned(), json!(d.count(*k)))).collect::<serde_json::Map<_, _>>(),
                    "worst_cutters": d.candidates.iter().take(10).map(|c| json!({ "solid": c.id, "kind": c.kind.id(), "visleaves_touched": c.leaves_touched, "center": v3(c.center) })).collect::<Vec<_>>(),
                })
            });
            let out = json!({
                "time_ms": an.ms,
                "leaked": an.leaked,
                "compile_error": an.error(),
                "visibility": an.report().map(|r| vis_json(r, top)),
                "func_detail": detail,
                "fixes": vis,
                "next": "suggest_func_detail, suggest_hint_brushes and suggest_areaportals apply the fixes; show_visleaves and screenshot_view show the leaves.",
            });
            engine::store(state, &an, c.report);
            if show {
                engine::set_view(state, true);
            }
            ToolResult::json(out)
        }),
    ))
}

fn suggest_func_detail(state: &mut AppState, a: &Args<'_>) -> R<Outcome> {
    let apply = a.opt_bool("apply")?.unwrap_or(false);
    let only = a.ids("ids")?;
    let kinds = a.strings("kinds", 5)?;
    if let Some(bad) = kinds.iter().find(|k| !DetailKind::ALL.iter().any(|d| d.id() == k.as_str())) {
        return Err(format!("unknown kind '{bad}' (step, trim, pillar, small, complex)"));
    }
    let measure = a.opt_bool("measure")?.unwrap_or(false);
    let limit = a.opt_u64("limit", 1, hp_mcp::tools::MAX_LIST)?.unwrap_or(200) as usize;
    let o = Options { compile: measure, measure_after: measure, ..Options::default() };
    Ok(spawn(
        "suggest_func_detail",
        state,
        args_key(a),
        false,
        o,
        Box::new(move |state, an| {
            let Some(d) = an.detail.as_ref() else { return ToolResult::error("suggest_func_detail: no result") };
            let chosen: Vec<&hp_editor::optimize::detail::DetailCandidate> = d
                .candidates
                .iter()
                .filter(|c| only.is_empty() || only.contains(&c.id))
                .filter(|c| kinds.is_empty() || kinds.iter().any(|k| k == c.kind.id()))
                .collect();
            let mut out = json!({
                "verified_against_leaks": d.verified,
                "map_leaks": d.map_leaks,
                "leak_check_incomplete": d.incomplete,
                "total": d.candidates.len(),
                "kept_structural_for_sealing": d.kept_for_sealing.len(),
                "candidates": chosen.iter().take(limit).map(|c| json!({
                    "solid": c.id, "kind": c.kind.id(), "size": v3(c.size), "center": v3(c.center), "visleaves_touched": c.leaves_touched,
                })).collect::<Vec<_>>(),
            });
            if let (Some(b), Some(Ok(after))) = (an.report(), an.after.as_ref()) {
                out["before_after"] = compare(b, after);
            }
            if apply {
                if !d.verified {
                    out["applied"] = json!({ "error": "not verified (the map leaks or is too complex for the leak check): fix the leak first" });
                } else if changed(state, &an) {
                    out["applied"] = json!({ "error": CHANGED });
                } else if chosen.is_empty() {
                    out["applied"] = json!(0);
                } else {
                    let objs: Vec<ObjRef> = chosen.iter().map(|c| ObjRef::Solid(c.id)).collect();
                    out["applied"] =
                        match edit(state, "suggest_func_detail", |doc| doc.tie_to_entity(&objs, "func_detail", &[])) {
                            Ok(e) => json!({ "func_detail": e, "brushes": objs.len() }),
                            Err(e) => json!({ "error": e }),
                        };
                    state.runtime.health.recheck = true;
                }
            }
            ToolResult::json(out)
        }),
    ))
}

fn suggest_hint_brushes(state: &mut AppState, a: &Args<'_>) -> R<Outcome> {
    let apply = a.opt_bool("apply")?.unwrap_or(false);
    let max = a.opt_u64("max_candidates", 1, 32)?.unwrap_or(8) as usize;
    let o = vis_options(a, Options { detail: false, hint_evals: max, measure_after: true, ..Options::default() })?;
    Ok(spawn(
        "suggest_hint_brushes",
        state,
        args_key(a),
        false,
        o,
        Box::new(move |state, an| {
            if an.leaked {
                return ToolResult::error("suggest_hint_brushes: the map leaks, fix the leak first (check_leaks)");
            }
            if let Some(e) = an.error() {
                return ToolResult::error(format!("suggest_hint_brushes: the compile failed: {e}"));
            }
            let mut out = json!({
                "time_ms": an.ms,
                "candidates": an.hints.iter().enumerate().map(|(i, c)| json!({
                    "index": i,
                    "reason": c.reason.id(),
                    "axis": AXES[c.axis.min(2)],
                    "at": c.at,
                    "hint_face": bounds_json(Some(c.rect)),
                    "brush": bounds_json(Some(c.brush_box())),
                    "visleaf": c.cluster,
                    "gain": c.gain,
                })).collect::<Vec<_>>(),
                "note": an.hints_note,
            });
            if let (Some(b), Some(Ok(after))) = (an.report(), an.after.as_ref()) {
                out["before_after"] = compare(b, after);
            }
            if apply {
                let good: Vec<_> = an.hints.iter().filter(|c| c.gain.is_some_and(|g| g > 0)).cloned().collect();
                out["applied"] = if good.is_empty() {
                    json!(0)
                } else if changed(state, &an) {
                    json!({ "error": CHANGED })
                } else {
                    match edit(state, "suggest_hint_brushes", |doc| doc.add_hint_brushes(&good)) {
                        Ok(ids) => json!({ "solids": ids }),
                        Err(e) => json!({ "error": e }),
                    }
                };
            }
            ToolResult::json(out)
        }),
    ))
}

fn show_visleaves(state: &mut AppState, a: &Args<'_>) -> R<ToolResult> {
    let s = &mut state.settings.health;
    for (key, field) in [("heat", &mut s.portal_heat), ("xray", &mut s.portal_xray), ("leaf_grid", &mut s.leaf_grid)] {
        if let Some(v) = a.opt_bool(key)? {
            *field = v;
            state.settings_dirty = true;
        }
    }
    state.runtime.health.portal_version = state.runtime.health.portal_version.wrapping_add(1);
    let show = a.opt_bool("show")?.unwrap_or(!state.runtime.optimize.view);
    if show && state.runtime.optimize.portals.is_none() {
        return Err("no visibility analysis yet: run analyze_visibility or optimize_map first".to_owned());
    }
    engine::set_view(state, show);
    let rt = &state.runtime.optimize;
    Ok(ToolResult::json(json!({
        "showing": rt.view,
        "portals": rt.portals.as_ref().map_or(0, |p| p.portals.len()),
        "visleaves": rt.portals.as_ref().map_or(0, |p| p.clusters),
        "heat": state.settings.health.portal_heat,
        "xray": state.settings.health.portal_xray,
        "leaf_grid": state.settings.health.leaf_grid,
        "stale": rt.stale(state.doc.scene_revision()),
    })))
}

fn portal_check_result(state: &mut AppState, an: Option<&Analysis>, fix: bool, codes: &[String]) -> ToolResult {
    let models = door_models(state);
    let model = move |e: &hp_vmf::Entity| models.get(&e.id).copied();
    let issues = areaportals::check(&state.doc, an.and_then(Analysis::report), &model);
    let mut out = json!({
        "areaportals": state.doc.scene().entities().values().filter(|n| areaportals::is_portal_class(&n.entity.classname)).count(),
        "areas": an.and_then(Analysis::report).map(|r| r.areas),
        "compile_error": an.and_then(Analysis::error),
        "issues": issues.iter().enumerate().map(|(i, f)| finding_json(i, f)).collect::<Vec<_>>(),
    });
    if fix {
        let chosen: Vec<Finding> = issues
            .into_iter()
            .filter(|f| f.fix.is_some_and(|k| engine::SAFE_PORTAL_FIXES.contains(&k)))
            .filter(|f| codes.is_empty() || codes.iter().any(|c| c == f.code))
            .collect();
        let r = edit(state, "check_areaportals", |doc| {
            let mut n = 0;
            for f in &chosen {
                if engine::fix_portal(doc, f, &model)? {
                    n += 1;
                }
            }
            Ok(n)
        });
        out["fixed"] = match r {
            Ok(n) => json!(n),
            Err(e) => json!({ "error": e }),
        };
        state.runtime.health.recheck = true;
    }
    ToolResult::json(out)
}

fn check_areaportals(state: &mut AppState, a: &Args<'_>) -> R<Outcome> {
    let fix = a.opt_bool("fix")?.unwrap_or(false);
    let codes = a.strings("codes", 16)?;
    if !a.opt_bool("compile")?.unwrap_or(true) {
        return Ok(portal_check_result(state, None, fix, &codes).into());
    }
    let o = Options { detail: false, ..Options::default() };
    Ok(spawn(
        "check_areaportals",
        state,
        args_key(a),
        false,
        o,
        Box::new(move |state, an| portal_check_result(state, Some(&an), fix, &codes)),
    ))
}

fn suggest_areaportals(state: &mut AppState, a: &Args<'_>) -> R<Outcome> {
    let apply = a.opt_bool("apply")?.unwrap_or(false);
    let include_open = a.opt_bool("include_open")?.unwrap_or(true);
    let indices: Option<Vec<usize>> = match a.get("indices") {
        None => None,
        Some(v) => Some(
            v.as_array()
                .ok_or("indices must be an array of integers")?
                .iter()
                .map(|x| x.as_u64().map(|n| n as usize).ok_or("indices must be an array of integers"))
                .collect::<Result<_, _>>()?,
        ),
    };
    let o = Options { detail: false, full_vis: false, auto_vis: false, ..Options::default() };
    let models = door_models(state);
    Ok(spawn(
        "suggest_areaportals",
        state,
        args_key(a),
        false,
        o,
        Box::new(move |state, an| {
            let model = move |e: &hp_vmf::Entity| models.get(&e.id).copied();
            let list: Vec<PortalSuggestion> =
                areaportals::suggest(&state.doc, an.bsp().and_then(|b| b.portals.as_ref()), &model, include_open);
            let mut out = json!({
                "compile_error": an.error(),
                "leaked": an.leaked,
                "areas_now": an.report().map(|r| r.areas),
                "suggestions": list.iter().enumerate().map(|(i, s)| {
                    let mut v = finding_json(i, &s.finding());
                    v["door"] = json!(s.door);
                    v["box"] = bounds_json(Some(s.bounds));
                    v["visleaves_each_side"] = json!([s.sides.0, s.sides.1]);
                    v["sealed"] = json!(s.sealed);
                    v
                }).collect::<Vec<_>>(),
            });
            if apply {
                let chosen: Vec<PortalSuggestion> = list
                    .iter()
                    .enumerate()
                    .filter(|(i, s)| s.sealed && indices.as_ref().is_none_or(|ix| ix.contains(i)))
                    .map(|(_, s)| s.clone())
                    .collect();
                let r = edit(state, "suggest_areaportals", |doc| {
                    let mut ids = Vec::new();
                    for s in &chosen {
                        ids.push(doc.place_areaportal(s.bounds, s.door)?);
                    }
                    Ok(ids)
                });
                out["applied"] = match r {
                    Ok(ids) => json!({ "areaportals": ids }),
                    Err(e) => json!({ "error": e }),
                };
                state.runtime.health.recheck = true;
            }
            ToolResult::json(out)
        }),
    ))
}

fn audit_lights(state: &mut AppState, a: &Args<'_>) -> R<ToolResult> {
    let fix = a.opt_bool("fix")?.unwrap_or(false);
    let kinds = a.strings("kinds", 8)?;
    let known = engine::light_fix_kinds(Level::All);
    if let Some(bad) = kinds.iter().find(|k| !known.contains(&k.as_str())) {
        return Err(format!("unknown fix kind '{bad}' ({})", known.join(", ")));
    }
    let limit = a.opt_u64("limit", 1, 1000)?.unwrap_or(100) as usize;
    let audit = engine::light_audit(state);
    let mut out = json!({
        "lights": audit.lights,
        "fixtures": audit.fixtures,
        "total": audit.findings.len(),
        "findings": audit.findings.iter().take(limit).enumerate().map(|(i, f)| finding_json(i, f)).collect::<Vec<_>>(),
    });
    if fix {
        let k: Vec<&str> = kinds.iter().map(String::as_str).collect();
        let fixes = audit.fixes_for(&k);
        out["fixed"] = if fixes.is_empty() {
            json!(0)
        } else {
            match edit(state, "audit_lights", |doc| doc.apply_light_fixes(&fixes)) {
                Ok(n) => json!(n),
                Err(e) => json!({ "error": e }),
            }
        };
    }
    Ok(ToolResult::json(out))
}

#[cfg(test)]
mod tests {
    use serde_json::json;

    use super::super::semantic_tests::{err, ok};

    fn sample_state() -> (hp_app::AppState, hp_editor::optimize::sample::Sample) {
        let mut s = hp_app::testing::test_state();
        let sample = hp_editor::optimize::sample::two_rooms();
        let text = sample.doc.to_vmf_string();
        s.doc = hp_editor::Document::from_map(hp_vmf::parse(&text).expect("vmf"));
        (s, sample)
    }

    #[test]
    fn long_analyses_are_kept_by_key() {
        let make = |k: &str| {
            let (_tx, rx) = std::sync::mpsc::channel();
            super::Pending { key: k.to_owned(), rx, started: std::time::Instant::now() }
        };
        for i in 0..6 {
            super::park(make(&format!("pending-test {i}")));
        }
        assert!(super::take_pending("pending-test 0").is_none(), "the oldest are dropped");
        assert!(super::take_pending("pending-test 5").is_some());
        assert!(super::take_pending("pending-test 5").is_none(), "taken once");
    }

    #[test]
    fn sample_maps_compile_and_can_be_written() {
        let doc = hp_editor::optimize::sample::row_of_rooms(4);
        let text = doc.to_vmf_string();
        let map = hp_vmf::parse(&text).expect("vmf");
        let (a, leaked, _) = crate::dispatch::health::optimize::compile(&map, None, false).expect("compile");
        assert!(!leaked);
        assert!(a.report.clusters > 8);
        if let Some(dir) = std::env::var_os("HP_OPTIMIZE_SAMPLES") {
            let dir = std::path::PathBuf::from(dir);
            std::fs::create_dir_all(&dir).expect("dir");
            std::fs::write(dir.join("opt_row_of_rooms.vmf"), text).expect("write");
            std::fs::write(dir.join("opt_two_rooms.vmf"), hp_editor::optimize::sample::two_rooms().doc.to_vmf_string())
                .expect("write");
        }
    }

    #[test]
    fn analyze_and_show_visleaves() {
        let (mut s, _) = sample_state();
        err(&mut s, "show_visleaves", json!({ "show": true }));
        let r = ok(&mut s, "analyze_visibility", json!({ "show": true }));
        assert_eq!(r["leaked"], false, "{r}");
        let v = &r["visibility"];
        assert!(v["visleaves"].as_u64().unwrap() >= 3, "{r}");
        assert!(v["portals"].as_u64().unwrap() >= 2, "{r}");
        assert!(v["total_pvs"].as_u64().unwrap() > 0);
        assert!(r["func_detail"]["candidates"].as_u64().unwrap() >= 8, "{r}");
        assert!(s.runtime.optimize.view && s.runtime.health.portals.is_some());
        let h = ok(&mut s, "show_visleaves", json!({ "show": false, "heat": true }));
        assert_eq!(h["showing"], false);
        assert!(s.runtime.health.portals.is_none());
        assert!(s.settings.health.portal_heat);
    }

    #[test]
    fn func_detail_lowers_visleaves() {
        let (mut s, sample) = sample_state();
        err(&mut s, "suggest_func_detail", json!({ "kinds": ["nope"] }));
        let r = ok(&mut s, "suggest_func_detail", json!({ "measure": true, "apply": true }));
        assert_eq!(r["verified_against_leaks"], true, "{r}");
        let ba = &r["before_after"]["visleaves"];
        assert!(ba[1].as_u64().unwrap() < ba[0].as_u64().unwrap(), "{r}");
        assert_eq!(r["applied"]["brushes"].as_u64().unwrap() as usize, sample.crates.len(), "{r}");
        assert_eq!(s.doc.undo_label().map(str::to_owned), Some("MCP: suggest_func_detail".to_owned()));
    }

    #[test]
    fn areaportals_and_lights() {
        let (mut s, sample) = sample_state();
        let r = ok(&mut s, "suggest_areaportals", json!({ "apply": true }));
        let list = r["suggestions"].as_array().unwrap();
        assert!(list.iter().any(|x| x["door"] == json!(sample.door)), "{r}");
        assert!(!r["applied"]["areaportals"].as_array().unwrap().is_empty(), "{r}");
        let c = ok(&mut s, "check_areaportals", json!({}));
        assert!(c["areas"].as_u64().unwrap() >= 2, "{c}");
        assert!(!c["issues"].as_array().unwrap().iter().any(|i| i["code"] == "areaportal_not_sealed"), "{c}");
        let l = ok(&mut s, "audit_lights", json!({}));
        assert!(l["total"].as_u64().unwrap() >= 3, "{l}");
        err(&mut s, "audit_lights", json!({ "kinds": ["paint"] }));
        let f = ok(&mut s, "audit_lights", json!({ "fix": true, "kinds": ["unblock", "merge", "add_light"] }));
        assert!(f["fixed"].as_u64().unwrap() >= 3, "{f}");
    }

    #[test]
    fn optimize_map_applies_and_measures() {
        let (mut s, _) = sample_state();
        let r = ok(&mut s, "optimize_map", json!({ "apply": "safe", "max_hints": 4 }));
        assert!(r["total_findings"].as_u64().unwrap() > 3, "{r}");
        let ba = &r["before_after"];
        assert!(ba["visleaves"][1].as_u64().unwrap() <= ba["visleaves"][0].as_u64().unwrap(), "{r}");
        assert!(ba["areas"][1].as_u64().unwrap() > ba["areas"][0].as_u64().unwrap(), "{r}");
        assert!(r["applied"]["func_detail_brushes"].as_u64().unwrap() > 0, "{r}");
        assert!(r["applied"]["areaportals_placed"].as_u64().unwrap() > 0, "{r}");
        assert_eq!(s.doc.undo_label().map(str::to_owned), Some("MCP: optimize_map".to_owned()));
        assert!(s.runtime.optimize.report.is_some());
        let again = ok(&mut s, "check_areaportals", json!({ "compile": false }));
        assert!(again["areaportals"].as_u64().unwrap() >= 1);
        let h = ok(&mut s, "suggest_hint_brushes", json!({ "max_candidates": 2 }));
        assert!(h["candidates"].is_array(), "{h}");
    }
}
