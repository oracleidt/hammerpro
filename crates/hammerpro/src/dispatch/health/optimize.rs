use std::sync::Arc;
use std::sync::atomic::AtomicBool;
use std::time::Instant;

use hp_app::AppState;
use hp_editor::compile_export::CompileExport;
use hp_editor::optimize::areaportals::{self, PortalSuggestion};
use hp_editor::optimize::detail::{DetailJob, DetailSuggestion};
use hp_editor::optimize::hints::{self, HintCandidate};
use hp_editor::optimize::lights::{self, LightAudit, LightFix, LightKnowledge};
use hp_editor::optimize::vis::{self, BspAnalysis, VisReport};
use hp_editor::optimize::{Finding, OptimizeReport, sort_findings};
use hp_editor::{Document, Id, ObjRef};
use hp_geom::Aabb;

const STACK: usize = 64 << 20;
const MEASURE_MARGIN: u64 = 30;
const AUTO_FULL_MAX: usize = 4000;
const AREAPORTAL: &str = "TOOLS/TOOLSAREAPORTAL";

struct ByName;

impl hp_bspc::MaterialSource for ByName {
    fn facts(&self, name: &str) -> hp_bspc::MaterialFacts {
        hp_bspc::materials::facts_by_name(name)
    }
}

pub(crate) struct Prepared {
    pub(crate) map: hp_vmf::Map,
    pub(crate) fs: Option<hp_fs::GameFs>,
    pub(crate) detail: DetailJob,
    pub(crate) revision: u64,
    pub(crate) existing_hints: Vec<Aabb>,
    pub(crate) doors: Vec<PortalSuggestion>,
    pub(crate) door_names: Vec<(Id, String)>,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(crate) struct Options {
    pub(crate) full_vis: bool,
    pub(crate) auto_vis: bool,
    pub(crate) compile: bool,
    pub(crate) detail: bool,
    pub(crate) hint_evals: usize,
    pub(crate) budget_secs: u64,
    pub(crate) measure_after: bool,
    pub(crate) after_doors: bool,
}

impl Default for Options {
    fn default() -> Self {
        Options {
            full_vis: true,
            auto_vis: true,
            compile: true,
            detail: true,
            hint_evals: 0,
            budget_secs: 75,
            measure_after: false,
            after_doors: false,
        }
    }
}

pub(crate) struct Analysis {
    pub(crate) before: Option<Result<BspAnalysis, String>>,
    pub(crate) leaked: bool,
    pub(crate) log: Vec<String>,
    pub(crate) detail: Option<DetailSuggestion>,
    pub(crate) hints: Vec<HintCandidate>,
    pub(crate) hints_note: Option<String>,
    pub(crate) after: Option<Result<VisReport, String>>,
    pub(crate) ms: u64,
    pub(crate) revision: u64,
}

impl Analysis {
    pub(crate) fn report(&self) -> Option<&VisReport> {
        self.before.as_ref().and_then(|b| b.as_ref().ok()).map(|b| &b.report)
    }

    pub(crate) fn bsp(&self) -> Option<&BspAnalysis> {
        self.before.as_ref().and_then(|b| b.as_ref().ok())
    }

    pub(crate) fn error(&self) -> Option<String> {
        match &self.before {
            Some(Err(e)) => Some(e.clone()),
            _ => None,
        }
    }
}

pub(crate) fn model_bounds(state: &AppState) -> impl Fn(&hp_vmf::Entity) -> Option<Aabb> + '_ {
    move |e| state.assets.entity_bounds(e)
}

pub(crate) fn prepare(state: &AppState, doors: bool) -> Prepared {
    let snap = state.doc.compile_snapshot(&CompileExport::default());
    let mut map = snap.map;
    let fs = state.assets.fs().cloned();
    if crate::bspc_instances::has_instances(&map)
        && let Some(path) = state.doc.path()
    {
        let files = crate::bspc_instances::InstanceFiles {
            vmf: path,
            game_dir: fs.as_ref().and_then(|f| f.game_dir()),
            fs: fs.as_ref(),
        };
        let root = fs.as_ref().and_then(|f| f.root()).map(std::path::Path::to_path_buf);
        crate::bspc_instances::merge_instances(&mut map, &files, root.as_deref(), &mut |l| {
            log::debug!("optimize: {l}")
        });
    }
    let model = model_bounds(state);
    let door_list = if doors { areaportals::suggest(&state.doc, None, &model, false) } else { Vec::new() };
    let door_names = door_list
        .iter()
        .filter_map(|d| d.door)
        .filter_map(|d| {
            let n = state.doc.scene().entity(d)?;
            Some((d, n.entity.get("targetname").map(str::trim).unwrap_or_default().to_owned()))
        })
        .collect();
    Prepared {
        map,
        fs,
        detail: hp_editor::optimize::detail::prepare(&state.doc),
        revision: state.doc.scene_revision(),
        existing_hints: hints::existing_hints(&state.doc),
        doors: door_list,
        door_names,
    }
}

fn threads() -> usize {
    std::thread::available_parallelism().map_or(2, |n| n.get()).div_ceil(2).max(1)
}

pub(crate) fn compile(
    map: &hp_vmf::Map,
    fs: Option<&hp_fs::GameFs>,
    full: bool,
) -> Result<(BspAnalysis, bool, Vec<String>), String> {
    let mut lines: Vec<String> = Vec::new();
    let opts = hp_bspc::CompileOptions {
        vis: if full { hp_bspc::VisLevel::Full } else { hp_bspc::VisLevel::Fast },
        relaxed_limits: true,
        lean_planes: true,
        preview: true,
        ..Default::default()
    };
    let builder = || rayon::ThreadPoolBuilder::new().num_threads(threads()).thread_name(|i| format!("hp-optimize-{i}"));
    let pool = builder()
        .stack_size(STACK)
        .build()
        .or_else(|_| builder().build())
        .map_err(|e| format!("cannot start the threads: {e}"))?;
    let out = pool
        .install(|| {
            let mut log = |l: &str| {
                if lines.len() < 4000 {
                    lines.push(l.to_owned());
                }
            };
            match fs {
                Some(fs) => {
                    let materials = crate::bspc_cli::GameMaterials { fs: fs.clone(), opts: Default::default() };
                    let models = crate::bspc_cli::GameModels { fs, allow_dynamic: true };
                    hp_bspc::compile(map, &materials, &models, &opts, &mut log)
                }
                None => hp_bspc::compile(map, &ByName, &hp_bspc::props::NoModels, &opts, &mut log),
            }
        })
        .map_err(|e| e.to_string())?;
    let leaked = out.leak.is_some();
    let clusters = u32::try_from(out.clusters).unwrap_or(0);
    let portals = (!out.vis_portals.is_empty())
        .then(|| vis::portal_file(clusters, out.vis_portals.iter().map(|p| (p.clusters, p.points.clone()))));
    let bsp = hp_bsp::Bsp::from_vec(out.bsp).map_err(|e| e.to_string())?;
    let analysis = vis::analyze_bsp(&bsp, portals, &lines)?;
    Ok((analysis, leaked, lines))
}

fn next_ids(map: &hp_vmf::Map) -> (u32, u32) {
    let mut solid = 1u32;
    let mut side = 1u32;
    for e in std::iter::once(&map.world).chain(map.entities.iter()) {
        solid = solid.max(e.id);
        for s in &e.solids {
            solid = solid.max(s.id);
            for x in &s.sides {
                side = side.max(x.id);
            }
        }
    }
    (solid + 1, side + 1)
}

fn add_solid(map_solids: &mut Vec<hp_vmf::Solid>, sides: Vec<hp_vmf::Side>, ids: &mut (u32, u32)) {
    let mut sides = sides;
    for s in &mut sides {
        s.id = ids.1;
        ids.1 += 1;
    }
    map_solids.push(hp_vmf::Solid { id: ids.0, sides, ..hp_vmf::Solid::default() });
    ids.0 += 1;
}

pub(crate) fn simulate(
    map: &mut hp_vmf::Map,
    detail: &[Id],
    hint_list: &[HintCandidate],
    doors: &[(PortalSuggestion, String)],
) {
    let mut ids = next_ids(map);
    if !detail.is_empty() {
        let set: std::collections::HashSet<Id> = detail.iter().copied().collect();
        let (moved, kept): (Vec<hp_vmf::Solid>, Vec<hp_vmf::Solid>) =
            std::mem::take(&mut map.world.solids).into_iter().partition(|s| set.contains(&s.id));
        map.world.solids = kept;
        if !moved.is_empty() {
            map.entities.push(hp_vmf::Entity {
                id: ids.0,
                classname: "func_detail".to_owned(),
                solids: moved,
                ..hp_vmf::Entity::default()
            });
            ids.0 += 1;
        }
    }
    for c in hint_list {
        if let Ok(sides) = c.sides() {
            add_solid(&mut map.world.solids, sides, &mut ids);
        }
    }
    for (d, name) in doors {
        let mut next = 0u32;
        let Ok(sides) = hp_geom::hull_solid(&d.bounds.corners(), AREAPORTAL, 0.25, 16, &mut || {
            next += 1;
            next
        }) else {
            continue;
        };
        let mut solids = Vec::new();
        add_solid(&mut solids, sides, &mut ids);
        let mut props = vec![("StartOpen".to_owned(), "1".to_owned()), ("PortalVersion".to_owned(), "1".to_owned())];
        if !name.is_empty() {
            props.push(("target".to_owned(), name.clone()));
        }
        map.entities.push(hp_vmf::Entity {
            id: ids.0,
            classname: "func_areaportal".to_owned(),
            props,
            solids,
            ..hp_vmf::Entity::default()
        });
        ids.0 += 1;
    }
}

fn evaluate_hints(
    p: &Prepared,
    base: &BspAnalysis,
    full: bool,
    max: usize,
    start: Instant,
    budget: u64,
    base_secs: f64,
) -> (Vec<HintCandidate>, Option<String>) {
    let Some(portals) = base.portals.as_ref() else { return (Vec::new(), None) };
    let visible = hints::visible_counts(&base.vis);
    let mut list = hints::candidates(&base.vis, portals, &visible, &p.existing_hints);
    if max == 0 || list.is_empty() {
        return (list, None);
    }
    let base_pvs = base.report.total_pvs as i64;
    let mut note = None;
    let evals = list.len().min(max);
    for (i, c) in list.iter_mut().take(evals).enumerate() {
        if start.elapsed().as_secs_f64() + base_secs > budget as f64 {
            note = Some(format!("time budget reached: {i} of {evals} hint candidates measured"));
            break;
        }
        let mut map = p.map.clone();
        simulate(&mut map, &[], std::slice::from_ref(c), &[]);
        if let Ok((a, false, _)) = compile(&map, p.fs.as_ref(), full) {
            c.gain = Some(base_pvs - a.report.total_pvs as i64);
        }
    }
    list.sort_by(|a, b| b.gain.unwrap_or(i64::MIN).cmp(&a.gain.unwrap_or(i64::MIN)));
    let good: Vec<HintCandidate> = list.iter().filter(|c| c.gain.is_some_and(|g| g > 0)).cloned().collect();
    if good.len() > 1 && start.elapsed().as_secs_f64() + base_secs <= budget as f64 {
        let mut map = p.map.clone();
        simulate(&mut map, &[], &good, &[]);
        if let Ok((a, false, _)) = compile(&map, p.fs.as_ref(), full) {
            let together = base_pvs - a.report.total_pvs as i64;
            let best = good[0].gain.unwrap_or(0);
            if together < best {
                for c in list.iter_mut().skip(1) {
                    if c.gain.is_some_and(|g| g > 0) {
                        c.gain = Some(0);
                    }
                }
                note = Some(format!(
                    "the hints work against each other ({together} together, {best} for the best one alone): only the best one is kept"
                ));
            }
        }
    }
    (list, note)
}

pub(crate) fn run(p: Prepared, o: Options, cancel: &AtomicBool) -> Analysis {
    let start = Instant::now();
    let solids = p.map.world.solids.len() + p.map.entities.iter().map(|e| e.solids.len()).sum::<usize>();
    let o = Options { full_vis: o.full_vis && !(o.auto_vis && solids > AUTO_FULL_MAX), ..o };
    let mut a = Analysis {
        before: None,
        leaked: false,
        log: Vec::new(),
        detail: None,
        hints: Vec::new(),
        hints_note: None,
        after: None,
        ms: 0,
        revision: p.revision,
    };
    let mut base_secs = 0.0;
    if o.compile {
        let t = Instant::now();
        let built = compile(&p.map, p.fs.as_ref(), o.full_vis);
        base_secs = t.elapsed().as_secs_f64();
        match built {
            Ok((b, leaked, log)) => {
                a.leaked = leaked;
                a.log = log;
                a.before = Some(Ok(b));
            }
            Err(e) => a.before = Some(Err(e)),
        }
    }
    if o.detail {
        let vis = a.bsp().map(|b| &b.vis);
        a.detail = Some(p.detail.clone().run(vis, cancel));
    }
    if !a.leaked
        && let Some(base) = a.bsp()
    {
        let (list, note) = evaluate_hints(&p, base, o.full_vis, o.hint_evals, start, o.budget_secs, base_secs);
        a.hints = list;
        a.hints_note = note;
    }
    let too_slow = start.elapsed().as_secs_f64() + base_secs > (o.budget_secs + MEASURE_MARGIN) as f64;
    if o.measure_after && !a.leaked && a.bsp().is_some() && too_slow {
        a.after = Some(Err(format!(
            "not measured: one compile takes {base_secs:.0} s, too long for the time limit; run analyze_visibility after the fixes"
        )));
    } else if o.measure_after && !a.leaked && a.bsp().is_some() {
        let mut map = p.map.clone();
        let detail: Vec<Id> = a.detail.as_ref().filter(|d| d.verified).map(|d| d.ids()).unwrap_or_default();
        let good: Vec<HintCandidate> = a.hints.iter().filter(|c| c.gain.is_some_and(|g| g > 0)).cloned().collect();
        let doors: Vec<(PortalSuggestion, String)> = if o.after_doors {
            p.doors
                .iter()
                .filter(|d| d.sealed)
                .map(|d| {
                    let name = d
                        .door
                        .and_then(|id| p.door_names.iter().find(|(i, _)| *i == id))
                        .map(|(_, n)| n.clone())
                        .unwrap_or_default();
                    (d.clone(), name)
                })
                .collect()
        } else {
            Vec::new()
        };
        simulate(&mut map, &detail, &good, &doors);
        a.after = Some(compile(&map, p.fs.as_ref(), o.full_vis).map(|(b, _, _)| b.report));
    }
    a.ms = start.elapsed().as_millis() as u64;
    a
}

pub(crate) fn light_audit(state: &AppState) -> LightAudit {
    let fs = state.assets.fs();
    let selfillum = |m: &str| {
        lights::light_material_by_name(m)
            || fs.and_then(|f| f.material(m)).is_some_and(|mat| mat.get("$selfillum").is_some_and(|v| v.trim() != "0"))
    };
    let texlight = |m: &str| m.to_ascii_lowercase().replace('\\', "/").starts_with("lights/");
    let model = model_bounds(state);
    let max = state.runtime.health.profile.max_switchable_lights();
    let k =
        LightKnowledge { model_bounds: &model, light_material: &selfillum, texlight: &texlight, max_switchable: max };
    lights::audit(&state.doc, &k)
}

pub(crate) struct Composed {
    pub(crate) report: OptimizeReport,
    pub(crate) lights: LightAudit,
    pub(crate) portal_issues: Vec<Finding>,
    pub(crate) portal_suggestions: Vec<PortalSuggestion>,
}

pub(crate) fn compose(state: &AppState, a: &Analysis, include_open: bool) -> Composed {
    let model = model_bounds(state);
    let mut findings: Vec<Finding> = Vec::new();
    if a.leaked {
        findings.push(
            Finding::new(
                hp_editor::optimize::Topic::Visleaves,
                "map_leaks",
                hp_editor::optimize::Impact::High,
                "The map leaks: without a sealed hull there is no visibility at all (every visleaf sees everything). Fix \
                 the leak first (Map > Check for leaks)."
                    .to_owned(),
            )
            .score(1.0e6),
        );
    }
    if let Some(r) = a.report().filter(|_| !a.leaked) {
        findings.extend(r.findings());
    }
    if let Some(d) = &a.detail
        && let Some(f) = d.finding()
    {
        findings.push(f);
    }
    findings.extend(a.hints.iter().filter(|c| c.gain.is_some_and(|g| g > 0)).take(12).map(HintCandidate::finding));
    let portal_issues = areaportals::check(&state.doc, a.report(), &model);
    findings.extend(portal_issues.iter().cloned());
    let portals = a.bsp().and_then(|b| b.portals.as_ref());
    let portal_suggestions = areaportals::suggest(&state.doc, portals, &model, include_open);
    findings.extend(portal_suggestions.iter().map(PortalSuggestion::finding));
    let lights = light_audit(state);
    findings.extend(lights.findings.iter().cloned());
    findings.extend(super::cubemaps::analyze_with(state, a.bsp().map(|b| &b.vis)).findings);
    sort_findings(&mut findings);
    let report = OptimizeReport {
        vis: a.report().cloned(),
        findings,
        compile_error: a.error(),
        leaked: a.leaked,
        revision: a.revision,
        ms: a.ms,
    };
    Composed { report, lights, portal_issues, portal_suggestions }
}

pub(crate) fn store(state: &mut AppState, a: &Analysis, report: OptimizeReport) {
    let rt = &mut state.runtime.optimize;
    rt.report = Some(Arc::new(report));
    rt.error = a.error();
    rt.selected = None;
    rt.version = rt.version.wrapping_add(1);
    match a.bsp() {
        Some(b) => {
            rt.portals = b.portals.clone().map(Arc::new);
            rt.vis = Some(Arc::new(b.vis.clone()));
            rt.costs = b.costs.clone().map(Arc::new);
        }
        None => {
            rt.portals = None;
            rt.vis = None;
            rt.costs = None;
        }
    }
    if state.runtime.optimize.view {
        set_view(state, true);
    }
}

fn analysis_path(state: &AppState) -> std::path::PathBuf {
    let dir = state.doc.path().and_then(|p| p.parent()).map(std::path::Path::to_path_buf).unwrap_or_default();
    dir.join(hp_app::runtime::ANALYSIS_PORTALS)
}

pub(crate) fn showing_analysis(state: &AppState) -> bool {
    state
        .runtime
        .health
        .portal_path
        .as_ref()
        .is_some_and(|p| p.file_name().is_some_and(|n| n.to_string_lossy() == hp_app::runtime::ANALYSIS_PORTALS))
}

pub(crate) fn set_view(state: &mut AppState, on: bool) -> bool {
    if on {
        let Some(file) = state.runtime.optimize.portals.clone() else {
            state.runtime.optimize.view = false;
            return false;
        };
        let path = analysis_path(state);
        let vis = state.runtime.optimize.vis.clone();
        let costs = state.runtime.optimize.costs.clone();
        let rt = &mut state.runtime.health;
        rt.set_portals(path.clone(), (*file).clone());
        if let Some(v) = vis {
            rt.set_vis(path.with_extension("bsp"), (*v).clone());
        }
        if let Some(c) = costs {
            rt.set_costs((*c).clone());
        }
        state.runtime.optimize.view = true;
        true
    } else {
        if showing_analysis(state) {
            state.runtime.health.clear_portals();
        }
        state.runtime.optimize.view = false;
        true
    }
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(crate) enum Level {
    Safe,
    All,
}

#[derive(Debug, Default)]
pub(crate) struct Applied {
    pub(crate) func_detail: usize,
    pub(crate) hints: usize,
    pub(crate) areaportals_placed: usize,
    pub(crate) areaportals_fixed: usize,
    pub(crate) lights_fixed: usize,
    pub(crate) errors: Vec<String>,
}

const SAFE_LIGHT_FIXES: &[&str] = &["unblock", "merge", "falloff", "delete", "add_sun"];
const ALL_LIGHT_FIXES: &[&str] =
    &["unblock", "merge", "falloff", "delete", "add_sun", "move_to_fixture", "add_light", "harmonize"];
pub(crate) const SAFE_PORTAL_FIXES: &[&str] = &["door_state", "link_door", "delete", "refit"];

pub(crate) fn light_fix_kinds(level: Level) -> &'static [&'static str] {
    match level {
        Level::Safe => SAFE_LIGHT_FIXES,
        Level::All => ALL_LIGHT_FIXES,
    }
}

pub(crate) fn fix_portal(
    doc: &mut Document,
    f: &Finding,
    model: &dyn Fn(&hp_vmf::Entity) -> Option<Aabb>,
) -> hp_editor::EditResult<bool> {
    let Some(ObjRef::Entity(id)) = f.objects.first().copied() else { return Ok(false) };
    if doc.scene().entity(id).is_none() {
        return Ok(false);
    }
    match f.fix {
        Some("door_state") => doc.sync_areaportal_state(id),
        Some("link_door") => doc.link_areaportal_door(id, model),
        Some("delete") => Ok(doc.delete(&[ObjRef::Entity(id)])? > 0),
        Some("refit") => doc.refit_areaportal(id),
        _ => Ok(false),
    }
}

pub(crate) fn apply(
    doc: &mut Document,
    a: &Analysis,
    c: &Composed,
    level: Level,
    model: &dyn Fn(&hp_vmf::Entity) -> Option<Aabb>,
) -> Applied {
    let mut out = Applied::default();
    if let Some(d) = a.detail.as_ref().filter(|d| d.verified && !d.candidates.is_empty()) {
        let objs: Vec<ObjRef> =
            d.ids().into_iter().filter(|id| doc.scene().solid(*id).is_some()).map(ObjRef::Solid).collect();
        match doc.tie_to_entity(&objs, "func_detail", &[]) {
            Ok(_) => out.func_detail = objs.len(),
            Err(e) => out.errors.push(format!("func_detail: {e}")),
        }
    }
    let good: Vec<HintCandidate> = a.hints.iter().filter(|h| h.gain.is_some_and(|g| g > 0)).cloned().collect();
    if !good.is_empty() {
        match doc.add_hint_brushes(&good) {
            Ok(ids) => out.hints = ids.len(),
            Err(e) => out.errors.push(format!("hints: {e}")),
        }
    }
    for f in c.portal_issues.iter().filter(|f| f.fix.is_some_and(|k| SAFE_PORTAL_FIXES.contains(&k))) {
        match fix_portal(doc, f, model) {
            Ok(true) => out.areaportals_fixed += 1,
            Ok(false) => {}
            Err(e) => out.errors.push(format!("{}: {e}", f.code)),
        }
    }
    for s in c.portal_suggestions.iter().filter(|s| s.sealed && (s.door.is_some() || level == Level::All)) {
        if s.door.is_some_and(|d| doc.scene().entity(d).is_none()) {
            continue;
        }
        match doc.place_areaportal(s.bounds, s.door) {
            Ok(_) => out.areaportals_placed += 1,
            Err(e) => out.errors.push(format!("areaportal: {e}")),
        }
    }
    let fixes: Vec<LightFix> = c.lights.fixes_for(light_fix_kinds(level));
    if !fixes.is_empty() {
        match doc.apply_light_fixes(&fixes) {
            Ok(n) => out.lights_fixed = n,
            Err(e) => out.errors.push(format!("lights: {e}")),
        }
    }
    out
}

pub(crate) struct Job {
    rx: std::sync::mpsc::Receiver<Analysis>,
    cancel: Arc<AtomicBool>,
}

impl Job {
    pub(crate) fn start(state: &mut AppState) -> Option<Job> {
        let prepared = prepare(state, false);
        let (tx, rx) = std::sync::mpsc::channel();
        let cancel = Arc::new(AtomicBool::new(false));
        let flag = Arc::clone(&cancel);
        let opts = Options { hint_evals: 6, budget_secs: 120, ..Options::default() };
        let spawned = std::thread::Builder::new().name("hp-optimize".to_owned()).spawn(move || {
            let r = std::panic::catch_unwind(std::panic::AssertUnwindSafe(|| run(prepared, opts, &flag)));
            if let Ok(a) = r {
                let _ = tx.send(a);
            }
        });
        match spawned {
            Ok(_) => {
                state.runtime.optimize.running = true;
                state.runtime.optimize.error = None;
                Some(Job { rx, cancel })
            }
            Err(e) => {
                state.runtime.optimize.error = Some(e.to_string());
                None
            }
        }
    }

    pub(crate) fn poll(&self, state: &mut AppState) -> bool {
        match self.rx.try_recv() {
            Ok(a) => {
                let c = compose(state, &a, true);
                store(state, &a, c.report);
                state.runtime.optimize.running = false;
                true
            }
            Err(std::sync::mpsc::TryRecvError::Empty) => false,
            Err(std::sync::mpsc::TryRecvError::Disconnected) => {
                state.runtime.optimize.running = false;
                state.runtime.optimize.error = Some("the analysis stopped (see the Hammer Pro log)".to_owned());
                true
            }
        }
    }

    pub(crate) fn cancel(&self) {
        self.cancel.store(true, std::sync::atomic::Ordering::Relaxed);
    }
}
