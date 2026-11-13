use std::collections::{HashMap, HashSet};
use std::sync::OnceLock;

use glam::DVec2;
use hp_app::state::{AppState, ModelState};
use hp_editor::{Document, Id, ObjRef, SelectOp};

use super::build::{Bounds, GenEnv, GenError, Plan, model_names, plan};
use super::model::{Mode, SplineMeta};
use super::store::{self, Applied, StoreError, UNDO_CREATE, UNDO_EDIT, WriteSpec};

#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub struct PropFinish {
    pub drop: bool,
    pub avoid: bool,
}

#[derive(Clone, Debug, Default, PartialEq, Eq)]
pub struct FinishReport {
    pub dropped: usize,
    pub removed: Vec<Id>,
}

pub type Finisher = fn(&mut AppState, &[Id], PropFinish) -> FinishReport;

static FINISHER: OnceLock<Finisher> = OnceLock::new();
const GROUND_LIFT: f64 = 64.0;

pub fn set_finisher(f: Finisher) {
    let _ = FINISHER.set(f);
}

pub fn has_finisher() -> bool {
    FINISHER.get().is_some()
}

#[derive(Clone, Debug, Default, PartialEq)]
pub struct Generated {
    pub applied: Applied,
    pub plan: Plan,
    pub dropped: usize,
    pub removed: usize,
    pub pending_models: Vec<String>,
}

#[derive(Debug)]
pub enum SplineError {
    Gen(GenError),
    Store(StoreError),
    Busy,
}

impl std::fmt::Display for SplineError {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            SplineError::Gen(e) => e.fmt(f),
            SplineError::Store(e) => e.fmt(f),
            SplineError::Busy => write!(f, "another edit is in progress"),
        }
    }
}

impl From<GenError> for SplineError {
    fn from(e: GenError) -> Self {
        SplineError::Gen(e)
    }
}

impl From<StoreError> for SplineError {
    fn from(e: StoreError) -> Self {
        SplineError::Store(e)
    }
}

pub fn model_bounds(state: &mut AppState, meta: &SplineMeta) -> (HashMap<String, Bounds>, Vec<String>) {
    let mut known = HashMap::new();
    let mut pending = Vec::new();
    for m in model_names(meta) {
        match state.assets.model_info(&m) {
            ModelState::Ready(info) => {
                known.insert(m, (info.bounds_min, info.bounds_max));
            }
            ModelState::Loading => pending.push(m),
            ModelState::Failed(_) => {}
        }
    }
    (known, pending)
}

pub fn excluded_solids(doc: &Document, anchor: Option<Id>) -> HashSet<Id> {
    let mut out = HashSet::new();
    let Some(a) = anchor else { return out };
    let Some(uid) = store::anchor_uid(doc, a) else { return out };
    for o in store::outputs_of(doc, a, &uid) {
        match o {
            ObjRef::Solid(id) => {
                out.insert(id);
            }
            ObjRef::Entity(id) => out.extend(doc.scene().entity_solid_ids(id)),
        }
    }
    out
}

pub fn ground_below(doc: &Document, exclude: &HashSet<Id>, p: DVec2, z: f64) -> Option<f64> {
    for lift in [GROUND_LIFT, GROUND_LIFT * 8.0] {
        let ray = hp_geom::Ray::new(glam::DVec3::new(p.x, p.y, z + lift), glam::DVec3::NEG_Z);
        let filter = |o: ObjRef| matches!(o, ObjRef::Solid(id) if !exclude.contains(&id));
        let opts = hp_editor::PickOptions { include_entities: false, filter: Some(&filter), ..Default::default() };
        if let Some(hit) = doc.pick_ray(&ray, &opts, &|_| None)
            && hit.point.z.is_finite()
        {
            return Some(hit.point.z);
        }
    }
    crate::road::apply::ground_height(doc, exclude, p, z)
}

pub fn plan_in(
    doc: &Document,
    meta: &SplineMeta,
    bounds: HashMap<String, Bounds>,
    exclude: &HashSet<Id>,
) -> Result<Plan, GenError> {
    let ground = |p: DVec2, z: f64| ground_below(doc, exclude, p, z);
    let env = GenEnv { ground: Some(&ground), bounds };
    plan(meta, &env)
}

pub fn lightmap(state: &AppState) -> i32 {
    match state.settings.edit.default_lightmap_scale {
        l if l > 0 => l,
        _ => 16,
    }
}

pub fn generate(state: &mut AppState, anchor: Option<Id>, meta: &SplineMeta) -> Result<Generated, SplineError> {
    generate_with(state, anchor, meta, HashMap::new())
}

pub fn generate_with(
    state: &mut AppState,
    anchor: Option<Id>,
    meta: &SplineMeta,
    known: HashMap<String, Bounds>,
) -> Result<Generated, SplineError> {
    let meta = meta.clone().clamped();
    let (mut bounds, mut pending) = model_bounds(state, &meta);
    for (k, v) in known {
        pending.retain(|p| *p != k);
        bounds.entry(k).or_insert(v);
    }
    let exclude = excluded_solids(&state.doc, anchor);
    let plan = plan_in(&state.doc, &meta, bounds, &exclude)?;
    let label = if anchor.is_some() { UNDO_EDIT } else { UNDO_CREATE };
    state.doc.begin(label);
    let spec = WriteSpec { anchor, meta: &meta, plan: &plan, lightmap: lightmap(state) };
    let mut applied = match store::write(&mut state.doc, &spec) {
        Ok(a) => a,
        Err(e) => {
            state.doc.cancel();
            return Err(e.into());
        }
    };
    let mut out = Generated { pending_models: pending, ..Generated::default() };
    let finish = PropFinish { drop: meta.drop, avoid: meta.avoid_overlap };
    if meta.mode == Mode::Props
        && (finish.drop || finish.avoid)
        && !applied.props.is_empty()
        && let Some(f) = FINISHER.get()
    {
        let report = f(state, &applied.props, finish);
        out.dropped = report.dropped;
        out.removed = report.removed.len();
        applied.props.retain(|id| !report.removed.contains(id));
        applied.outputs.retain(|o| !matches!(o, ObjRef::Entity(id) if report.removed.contains(id)));
        let mut all = vec![ObjRef::Entity(applied.anchor)];
        all.extend(applied.outputs.iter().copied());
        state.doc.select(&all, SelectOp::Replace);
    }
    state.doc.commit();
    out.applied = applied;
    out.plan = plan;
    Ok(out)
}

pub fn save_only(state: &mut AppState, anchor: Id, meta: &SplineMeta) -> Result<(), SplineError> {
    state.doc.transaction(UNDO_EDIT, |doc| store::save_meta(doc, anchor, meta)).map_err(SplineError::Store)
}
