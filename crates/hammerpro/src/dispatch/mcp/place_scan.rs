use std::sync::atomic::{AtomicBool, Ordering};

use glam::{DMat4, DVec3};
use hp_app::AppState;
use hp_editor::placement_fix::{fmt_move, object_key};
use hp_editor::{ObjRef, Problem, Severity};
use hp_geom::Aabb;
use hp_shape::TriMesh;

use super::place_ops::{self, OverlapInfo, Settle};
use super::place_world::{self as pw, EnvOptions, Key, MeshChoice, Obj, World};

pub(super) const SIDE_DIRS: [(&str, DVec3); 5] =
    [("+x", DVec3::X), ("-x", DVec3::NEG_X), ("+y", DVec3::Y), ("-y", DVec3::NEG_Y), ("+z", DVec3::Z)];

pub(crate) const FLOAT_TOLERANCE: f64 = 0.5;
pub(crate) const OVERLAP_TOLERANCE: f64 = 1.0;
pub(crate) const MAX_SINK_FIX: f64 = 16.0;
pub(crate) const MAX_OVERLAP_FIX: f64 = 16.0;
pub(crate) const MAX_DROP: f64 = 4096.0;
const QUICK_AXES: usize = 1;
const NEAR_DROP: f64 = 256.0;
const PARALLEL_FROM: usize = 64;

#[derive(Clone, Copy, Debug, PartialEq)]
pub(super) enum Class {
    Resting,
    Floating(f64),
    Sunk(f64),
    Attached,
    NoSurface,
}

pub(super) fn classify(moving: &TriMesh, world: &World, i: usize, tol: f64, max_drop: f64) -> (Settle, Class) {
    let near = NEAR_DROP.min(max_drop);
    let mut s = place_ops::settle(moving, world, DVec3::NEG_Z, near, &[i], tol.max(0.01));
    if s.delta.is_none() && max_drop > near {
        s = place_ops::settle(moving, world, DVec3::NEG_Z, max_drop, &[i], tol.max(0.01));
    }
    let touching =
        || SIDE_DIRS.iter().any(|(_, d)| place_ops::clearance(moving, world, *d, tol + 0.05, &[i]).is_some());
    let class = match s.delta {
        Some(g) if g > tol => {
            if touching() {
                Class::Attached
            } else {
                Class::Floating(g)
            }
        }
        Some(g) if g < -tol => Class::Sunk(-g),
        Some(_) => Class::Resting,
        None => {
            if touching() {
                Class::Attached
            } else {
                Class::NoSurface
            }
        }
    };
    (s, class)
}

#[derive(Clone, Copy, Debug)]
pub(super) struct ScanOpts {
    pub(super) tol: f64,
    pub(super) overlap_tol: f64,
    pub(super) max_sink: f64,
    pub(super) max_overlap_fix: f64,
    pub(super) max_drop: f64,
    pub(super) overlaps: bool,
    pub(super) skip_hanging: bool,
    pub(super) quick: bool,
}

impl Default for ScanOpts {
    fn default() -> Self {
        Self {
            tol: FLOAT_TOLERANCE,
            overlap_tol: OVERLAP_TOLERANCE,
            max_sink: MAX_SINK_FIX,
            max_overlap_fix: MAX_OVERLAP_FIX,
            max_drop: MAX_DROP,
            overlaps: true,
            skip_hanging: false,
            quick: false,
        }
    }
}

#[derive(Clone, Debug)]
pub(super) struct Finding {
    pub(super) idx: usize,
    pub(super) class: Class,
    pub(super) settle: Settle,
    pub(super) solo: DVec3,
    pub(super) chain: DVec3,
    pub(super) deps: Vec<usize>,
    pub(super) overlaps: Vec<OverlapInfo>,
}

fn class_move(class: Class, max_sink: f64) -> DVec3 {
    match class {
        Class::Floating(g) => DVec3::new(0.0, 0.0, -place_ops::round_back(g)),
        Class::Sunk(d) if d <= max_sink => DVec3::new(0.0, 0.0, -place_ops::round_back(-d)),
        _ => DVec3::ZERO,
    }
}

fn column(b: &Aabb, max_drop: f64) -> Aabb {
    Aabb::new(b.min - DVec3::new(0.0, 0.0, max_drop), b.max).expand(1.0)
}

pub(super) fn scan(world: &mut World, cands: &[usize], o: &ScanOpts, cancel: Option<&AtomicBool>) -> Vec<Finding> {
    let mut order: Vec<usize> = cands.to_vec();
    order.sort_by(|x, y| world.bounds_of(*x).min.z.total_cmp(&world.bounds_of(*y).min.z));
    let cand_set: std::collections::HashSet<usize> = cands.iter().copied().collect();
    let shared: &World = world;
    let one = |i: usize| -> Option<Finding> {
        if cancel.is_some_and(|c| c.load(Ordering::Relaxed)) {
            return None;
        }
        let moving = shared.resolved[i].compound();
        if o.quick && !o.overlaps && quick_resting(shared, i, &moving, o.tol) {
            return None;
        }
        let (settle, mut class) = classify(&moving, shared, i, o.tol, o.max_drop);
        if o.skip_hanging && matches!(class, Class::Floating(_)) && hanging(shared, i, &moving) {
            class = Class::Attached;
        }
        let mut overlaps = Vec::new();
        if o.overlaps {
            let mut skip = vec![i];
            for (_, k) in shared.near(&shared.bounds_of(i).expand(o.overlap_tol)) {
                if k != i && cand_set.contains(&k) && !skip.contains(&k) && reports_pair(shared, k, i) {
                    skip.push(k);
                }
            }
            overlaps = place_ops::overlaps_on(&moving, shared, &skip, o.overlap_tol, QUICK_AXES);
            overlaps.retain(|x| Some(x.owner) != settle.lifted_by && x.push.z <= 0.5 * x.depth.max(1e-9));
        }
        let solo = class_move(class, o.max_sink);
        Some(Finding { idx: i, class, settle, solo, chain: solo, deps: Vec::new(), overlaps })
    };
    let found: Vec<Option<Finding>> = if order.len() >= PARALLEL_FROM {
        use rayon::prelude::*;
        let threads = std::thread::available_parallelism().map_or(2, |n| (n.get() / 2).max(1));
        match rayon::ThreadPoolBuilder::new().num_threads(threads).thread_name(|i| format!("hp-placement-{i}")).build()
        {
            Ok(pool) => pool.install(|| order.par_iter().map(|&i| one(i)).collect()),
            Err(_) => order.iter().map(|&i| one(i)).collect(),
        }
    } else {
        order.iter().map(|&i| one(i)).collect()
    };
    if cancel.is_some_and(|c| c.load(Ordering::Relaxed)) {
        return Vec::new();
    }
    let mut out: Vec<Finding> = found.into_iter().flatten().collect();
    let mut moved: Vec<usize> = Vec::new();
    for f in &mut out {
        if cancel.is_some_and(|c| c.load(Ordering::Relaxed)) {
            return Vec::new();
        }
        if f.solo == DVec3::ZERO {
            continue;
        }
        let col = column(&world.bounds_of(f.idx), o.max_drop);
        let deps: Vec<usize> = moved.iter().copied().filter(|m| world.bounds_of(*m).intersects(&col)).collect();
        if !deps.is_empty() {
            let moving = world.resolved[f.idx].compound();
            let (_, class) = classify(&moving, world, f.idx, o.tol, o.max_drop);
            f.chain = class_move(class, o.max_sink);
            f.deps = deps;
        }
        if f.chain != DVec3::ZERO {
            world.move_obj(f.idx, &DMat4::from_translation(f.chain));
            moved.push(f.idx);
        }
    }
    out
}

const HANGING_WORDS: &[&str] = &["lamp", "light", "chain", "hanging", "sign", "wire", "cable"];
pub(crate) const HANG_REACH: f64 = 1.0;

pub(super) fn hanging(world: &World, i: usize, moving: &TriMesh) -> bool {
    let obj = &world.objs[i];
    let named = obj.models.iter().any(|m| {
        let stem = m.path.rsplit('/').next().unwrap_or(&m.path).to_ascii_lowercase();
        HANGING_WORDS.iter().any(|w| stem.contains(w))
    });
    if named {
        return true;
    }
    let b = world.bounds_of(i);
    let h = b.size().z.max(1e-6);
    let hook = world.resolved[i]
        .shapes
        .iter()
        .any(|(shape, mx)| shape.attachments.iter().any(|a| mx.transform_point3(a.pos).z >= b.max.z - 0.15 * h));
    if hook {
        return true;
    }
    SIDE_DIRS.iter().any(|(_, d)| place_ops::clearance(moving, world, *d, HANG_REACH, &[i]).is_some())
}

fn support_points(moving: &TriMesh) -> Vec<DVec3> {
    let b = moving.bounds();
    let band = b.min.z + (b.size().z * 0.05).max(0.25);
    let c = b.center();
    let mut best: [Option<DVec3>; 4] = [None; 4];
    for t in &moving.tris {
        for &k in t {
            let v = moving.verts[k as usize];
            if v.z > band {
                continue;
            }
            let q = usize::from(v.x >= c.x) + 2 * usize::from(v.y >= c.y);
            if best[q].is_none_or(|p| v.z < p.z) {
                best[q] = Some(v);
            }
        }
    }
    best.into_iter().flatten().collect()
}

pub(super) fn quick_resting(world: &World, i: usize, moving: &TriMesh, tol: f64) -> bool {
    let pts = support_points(moving);
    if pts.len() < 3 {
        return false;
    }
    let lowest = pts.iter().map(|p| p.z).fold(f64::INFINITY, f64::min);
    let reach = 2.0 * tol;
    let mut touching = 0usize;
    for p in &pts {
        let start = DVec3::new(p.x, p.y, p.z + tol);
        let column = Aabb::new(start - DVec3::new(0.0, 0.0, reach), start).expand(0.5);
        let env: Vec<&TriMesh> = world.near(&column).into_iter().filter(|(_, o)| *o != i).map(|(m, _)| m).collect();
        if hp_shape::ray_cast(&env, start, DVec3::NEG_Z, reach).is_some() {
            touching += 1;
        }
    }
    touching == pts.len() && pts.iter().all(|p| p.z - lowest <= tol)
}

pub(super) fn sky_rooms(world: &World, cameras: &[DVec3]) -> Vec<Aabb> {
    let mut out = Vec::new();
    for &c in cameras {
        let mut lo = c;
        let mut hi = c;
        let mut closed = true;
        for d in [DVec3::X, DVec3::NEG_X, DVec3::Y, DVec3::NEG_Y, DVec3::Z, DVec3::NEG_Z] {
            let reach = SKY_REACH;
            let seg = Aabb::new(c, c).union(&Aabb::new(c + d * reach, c + d * reach)).expand(1.0);
            let near: Vec<&TriMesh> = world
                .near(&seg)
                .into_iter()
                .filter(|(_, o)| matches!(world.objs[*o].key, Key::Solid(_)) || world.objs[*o].models.is_empty())
                .map(|(m, _)| m)
                .collect();
            let Some(hit) = hp_shape::ray_cast(&near, c, d, reach) else {
                closed = false;
                break;
            };
            let p = c + d * hit.t;
            lo = lo.min(p);
            hi = hi.max(p);
        }
        if closed {
            out.push(Aabb::new(lo, hi).expand(1.0));
        }
    }
    out
}

const SKY_REACH: f64 = 16384.0;

pub(super) fn sky_cameras(state: &AppState) -> Vec<DVec3> {
    state
        .doc
        .scene()
        .entities()
        .values()
        .filter(|n| n.entity.classname.eq_ignore_ascii_case("sky_camera"))
        .filter_map(|n| n.entity.origin().filter(|o| o.is_finite()))
        .collect()
}

pub(super) fn outside_sky(world: &World, cands: Vec<usize>, rooms: &[Aabb]) -> Vec<usize> {
    if rooms.is_empty() {
        return cands;
    }
    cands.into_iter().filter(|i| !rooms.iter().any(|r| r.contains_point(world.bounds_of(*i).center()))).collect()
}

pub(super) fn label(o: &Obj) -> String {
    let class = if o.class.is_empty() { o.key.kind() } else { o.class.as_str() };
    match o.model_path() {
        Some(m) => {
            let stem = m.rsplit('/').next().unwrap_or(m).trim_end_matches(".mdl");
            format!("{class} #{} ({stem})", o.key.id())
        }
        None => format!("{class} #{}", o.key.id()),
    }
}

fn fingerprint(o: &Obj) -> DVec3 {
    o.origin.filter(|p| p.is_finite()).unwrap_or(o.hint.min)
}

fn num(x: f64) -> String {
    let r = (x * 100.0).round() / 100.0;
    if r == r.trunc() { format!("{}", r as i64) } else { format!("{r}") }
}

fn key_ref(world: &World, i: usize) -> Option<ObjRef> {
    match world.objs[i].key {
        Key::Entity(id) => Some(ObjRef::Entity(id)),
        Key::Solid(id) => Some(ObjRef::Solid(id)),
        Key::Group(_) => world.objs[i].refs.first().copied(),
    }
}

fn reports_pair(world: &World, a: usize, b: usize) -> bool {
    let (va, vb) = (volume(&world.bounds_of(a)), volume(&world.bounds_of(b)));
    va < vb - 1e-6 || ((va - vb).abs() <= 1e-6 && world.objs[a].key.id() < world.objs[b].key.id())
}

fn volume(b: &Aabb) -> f64 {
    if b.is_empty() { 0.0 } else { b.size().x * b.size().y * b.size().z }
}

pub(super) fn problems(world: &World, findings: &[Finding], o: &ScanOpts) -> Vec<Problem> {
    let cand: std::collections::HashSet<usize> = findings.iter().map(|f| f.idx).collect();
    let mut out = Vec::new();
    for f in findings {
        let obj = &world.objs[f.idx];
        let Some(me) = key_ref(world, f.idx) else { continue };
        let what = label(obj);
        let fp = fmt_move(fingerprint(obj));
        let deps: Vec<String> = f.deps.iter().filter_map(|d| key_ref(world, *d)).map(object_key).collect();
        let below = f.settle.contact.as_ref().map(|(k, _)| *k);
        match f.class {
            Class::Floating(g) => {
                let other = below.map_or_else(String::new, |k| label(&world.objs[k]));
                let message = format!("{what} floats {} units above {other}", num(g));
                let mut args = vec![what.clone(), num(g), other, fmt_move(f.solo), fmt_move(f.chain), fp.clone()];
                args.extend(deps.iter().cloned());
                out.push(Problem::new(Severity::Warning, "floating_object", message, vec![me], true).with_args(args));
            }
            Class::Sunk(d) => {
                let into = f.settle.lifted_by.or(below);
                let other = into.map_or_else(String::new, |k| label(&world.objs[k]));
                let message = format!("{what} is sunk {} units into {other}", num(d));
                let fixable = d <= o.max_sink;
                let mut args = vec![what.clone(), num(d), other, fmt_move(f.solo), fmt_move(f.chain), fp.clone()];
                args.extend(deps.iter().cloned());
                let mut objects = vec![me];
                if let Some(r) = into.and_then(|k| key_ref(world, k)) {
                    objects.push(r);
                }
                out.push(Problem::new(Severity::Info, "sunk_object", message, objects, fixable).with_args(args));
            }
            _ => {}
        }
        for ov in &f.overlaps {
            if cand.contains(&ov.owner) && reports_pair(world, ov.owner, f.idx) {
                continue;
            }
            let Some(them) = key_ref(world, ov.owner) else { continue };
            let other = label(&world.objs[ov.owner]);
            let message = format!("{what} overlaps {other} by {} units", num(ov.depth));
            let push = ov.push.try_normalize().map_or(DVec3::ZERO, |d| ov.push + d * 0.01);
            let push = push.map(place_ops::round);
            let fixable = ov.depth <= o.max_overlap_fix && push != DVec3::ZERO && !ov.contained;
            let args = vec![what.clone(), num(ov.depth), other, fmt_move(push), fmt_move(push), fp.clone()];
            out.push(
                Problem::new(Severity::Info, "overlapping_objects", message, vec![me, them], fixable).with_args(args),
            );
        }
    }
    out
}

pub(crate) struct MapScan {
    env: Vec<Obj>,
    cands: Vec<Key>,
    skies: Vec<DVec3>,
    overlaps: bool,
    pub(crate) serial: u64,
}

impl MapScan {
    pub(crate) fn candidates(&self) -> usize {
        self.cands.len()
    }
}

pub(super) fn is_candidate(state: &AppState, o: &Obj) -> bool {
    if o.models.is_empty() || !o.meshes.is_empty() {
        return false;
    }
    let Key::Entity(id) = o.key else { return false };
    prop_entity(state, id)
        && state.doc.scene().entity(id).is_some_and(|n| n.entity.get("parentname").is_none_or(|p| p.trim().is_empty()))
}

pub(super) fn drop_helpers(state: &AppState, env: &mut Vec<Obj>) {
    env.retain(|o| match o.key {
        Key::Entity(id) if !o.models.is_empty() && o.meshes.is_empty() => prop_entity(state, id),
        _ => true,
    });
}

pub(super) fn prop_entity(state: &AppState, id: u32) -> bool {
    let scene = state.doc.scene();
    if scene.is_brush_entity(id) {
        return false;
    }
    scene.entity(id).is_some_and(|n| {
        !hp_editor::instances::is_instance(&n.entity)
            && n.entity.get("model").is_some_and(|m| {
                let m = m.trim();
                m.len() > 4 && m.get(m.len() - 4..).is_some_and(|e| e.eq_ignore_ascii_case(".mdl"))
            })
    })
}

pub(crate) struct MapScanBuilder {
    serial: u64,
    solids: Vec<u32>,
    entities: Vec<u32>,
    pos: usize,
    env: Vec<Obj>,
    pub(crate) spent: std::time::Duration,
}

const SCAN_OPTS: EnvOptions = EnvOptions { include_tools: false, include_points: false, region: None };

impl MapScanBuilder {
    pub(crate) fn new(state: &AppState) -> Self {
        let scene = state.doc.scene();
        MapScanBuilder {
            serial: state.doc.content_serial(),
            solids: scene.solids().keys().copied().collect(),
            entities: scene.entities().keys().copied().collect(),
            pos: 0,
            env: Vec::new(),
            spent: std::time::Duration::ZERO,
        }
    }

    pub(crate) fn serial(&self) -> u64 {
        self.serial
    }

    pub(crate) fn step(&mut self, state: &AppState, budget: std::time::Duration) -> Option<MapScan> {
        let start = std::time::Instant::now();
        let none = hp_editor::IdSet::default();
        let total = self.solids.len() + self.entities.len();
        while self.pos < total {
            let i = self.pos;
            self.pos += 1;
            let o = if i < self.solids.len() {
                pw::env_solid(state, self.solids[i], &none, &SCAN_OPTS)
            } else {
                pw::env_entity(state, self.entities[i - self.solids.len()], &none, &none, &SCAN_OPTS)
            };
            self.env.extend(o);
            if self.pos.is_multiple_of(64) && start.elapsed() >= budget {
                break;
            }
        }
        if self.pos < total {
            self.spent += start.elapsed();
            return None;
        }
        let mut env = std::mem::take(&mut self.env);
        drop_helpers(state, &mut env);
        let cands = env.iter().filter(|o| is_candidate(state, o)).map(|o| o.key).collect();
        self.spent += start.elapsed();
        Some(MapScan {
            env,
            cands,
            skies: sky_cameras(state),
            overlaps: state.settings.placement.overlap_check,
            serial: self.serial,
        })
    }
}

pub(crate) fn prepare_map_scan(state: &AppState) -> MapScan {
    let env = pw::environment(
        state,
        &Default::default(),
        &Default::default(),
        &EnvOptions { include_tools: false, include_points: false, region: None },
    );
    let mut env = env;
    drop_helpers(state, &mut env);
    let cands = env.iter().filter(|o| is_candidate(state, o)).map(|o| o.key).collect();
    MapScan {
        env,
        cands,
        skies: sky_cameras(state),
        overlaps: state.settings.placement.overlap_check,
        serial: state.doc.content_serial(),
    }
}

pub(crate) fn run_map_scan(job: MapScan, fs: Option<&hp_fs::GameFs>, cancel: &AtomicBool) -> Option<Vec<Problem>> {
    let MapScan { env, cands, skies, overlaps, .. } = job;
    if cands.is_empty() {
        return Some(Vec::new());
    }
    let started = std::time::Instant::now();
    let mut world = World::build_chunked(env, fs, MeshChoice::Render);
    let built = started.elapsed().as_millis();
    if cancel.load(Ordering::Relaxed) {
        return None;
    }
    let keys: std::collections::HashSet<Key> = cands.into_iter().collect();
    let idx: Vec<usize> = world
        .objs
        .iter()
        .enumerate()
        .filter(|(i, o)| keys.contains(&o.key) && world.resolved[*i].notes.is_empty())
        .map(|(i, _)| i)
        .collect();
    let rooms = sky_rooms(&world, &skies);
    let idx = outside_sky(&world, idx, &rooms);
    let opts = ScanOpts { overlaps, skip_hanging: true, quick: true, ..ScanOpts::default() };
    let findings = scan(&mut world, &idx, &opts, Some(cancel));
    if cancel.load(Ordering::Relaxed) {
        return None;
    }
    log::info!(
        "placement check: {} props, world built in {built} ms, scanned in {} ms",
        idx.len(),
        started.elapsed().as_millis() - built
    );
    Some(problems(&world, &findings, &opts))
}

pub(crate) fn scan_now(state: &mut AppState) {
    let start = std::time::Instant::now();
    if !state.settings.placement.problem_checks {
        state.runtime.health.set_placement(Vec::new(), None, 0);
        return;
    }
    let job = prepare_map_scan(state);
    let serial = job.serial;
    let fs = state.assets.fs().cloned();
    let cancel = AtomicBool::new(false);
    if let Some(p) = run_map_scan(job, fs.as_ref(), &cancel) {
        let ms = start.elapsed().as_millis() as u64;
        state.runtime.health.set_placement(p, Some(serial), ms);
    }
}
