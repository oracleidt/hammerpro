use std::collections::{BTreeMap, HashMap};
use std::path::{Path, PathBuf};

use glam::DVec3;
use hp_bspc::MaterialFacts;
use hp_bspc::materials::{contents, surf};
use hp_geom::{Aabb, Ray, SolidGeom};
use hp_vmf::{Entity, Map};

pub(crate) const FADE_MIN: f64 = 800.0;
pub(crate) const FADE_MAX: f64 = 4000.0;
pub(crate) const FADE_MAX_RADIUS: f64 = 200.0;
pub(crate) const DETAIL_FADE_MIN: f64 = 400.0;
pub(crate) const DETAIL_FADE_MAX: f64 = 1200.0;
pub(crate) const MERGE_CELL: f64 = 512.0;
pub(crate) const MERGE_LINK: f64 = 384.0;
pub(crate) const MERGE_MAX_PROPS: usize = 24;
pub(crate) const MERGE_MAX_MODELS: usize = 96;
pub(crate) const OCCLUDER_MAX: usize = 16;
pub(crate) const OCCLUDER_MIN_PROPS: usize = 3;
pub(crate) const OCCLUDER_RANGE: f64 = 2048.0;
pub(crate) const HINT_CANDIDATES: usize = 8;
pub(crate) const HINT_BUDGET_SECS: u64 = 180;
pub(crate) const MERGE_MODEL_TIMEOUT_SECS: u64 = 180;
pub(crate) const MERGE_MAX_WORKERS: usize = 4;
pub(crate) const PROGRESS_TICK_SECS: u64 = 5;
const OCCLUDER: &str = "TOOLS/TOOLSOCCLUDER";
const NODRAW: &str = "TOOLS/TOOLSNODRAW";

#[derive(Debug, Default, Clone, PartialEq, Eq)]
pub(crate) struct OptimizeArgs {
    pub(crate) merge_props: bool,
    pub(crate) hints: bool,
    pub(crate) fade: bool,
    pub(crate) occluders: bool,
    pub(crate) model_compiler: Option<PathBuf>,
    pub(crate) model_compiler_args: String,
}

impl OptimizeArgs {
    pub(crate) fn any(&self) -> bool {
        self.merge_props || self.hints || self.fade || self.occluders
    }

    pub(crate) fn flags(&self) -> Vec<&'static str> {
        [
            (self.hints, "-autohints"),
            (self.occluders, "-autooccluders"),
            (self.fade, "-autofade"),
            (self.merge_props, "-mergestaticprops"),
        ]
        .into_iter()
        .filter_map(|(on, f)| on.then_some(f))
        .collect()
    }
}

pub(crate) fn default_model_compiler(root: &Path) -> Option<PathBuf> {
    hp_app::model_compiler::studiomdl_exe(root)
}

pub(crate) fn merged_dir(vmf: &Path) -> PathBuf {
    let stem = vmf.file_stem().map_or_else(|| "map".into(), |s| s.to_string_lossy().into_owned());
    vmf.with_file_name(format!("{stem}_merged_props"))
}

fn model_of(e: &Entity) -> Option<String> {
    let m = e.get("model")?.trim().replace('\\', "/").to_ascii_lowercase();
    m.ends_with(".mdl").then_some(m)
}

fn scale_of(e: &Entity) -> f64 {
    let read = |k: &str| e.get(k).and_then(|s| s.trim().parse::<f64>().ok()).filter(|s| s.is_finite() && *s > 0.0);
    read("modelscale").or_else(|| read("uniformscale")).unwrap_or(1.0)
}

fn is_prop_class(class: &str) -> bool {
    let c = class.to_ascii_lowercase();
    c == "prop_static" || c.starts_with("prop_physics") || c.starts_with("prop_dynamic")
}

#[derive(Clone, Copy, Debug, PartialEq)]
pub(crate) struct PropBox {
    pub(crate) center: DVec3,
    pub(crate) radius: f64,
}

pub(crate) fn prop_box(e: &Entity, bounds: &dyn Fn(&str) -> Option<(DVec3, DVec3)>) -> Option<PropBox> {
    let origin = e.origin().filter(|o| o.is_finite())?;
    let (lo, hi) = model_of(e).and_then(|m| bounds(&m))?;
    let scale = scale_of(e);
    let rot = hp_geom::angle_matrix(e.angles().filter(|a| a.is_finite()).unwrap_or(DVec3::ZERO));
    Some(PropBox { center: origin + rot * ((lo + hi) * 0.5 * scale), radius: (hi - lo).length() * 0.5 * scale })
}

fn unset(v: Option<&str>) -> bool {
    v.and_then(|s| s.trim().parse::<f64>().ok()).is_none_or(|x| !x.is_finite() || x <= 0.0)
}

fn round_to(v: f64, step: f64) -> f64 {
    (v / step).round() * step
}

pub(crate) fn fade_for_radius(radius: f64) -> Option<(f64, f64)> {
    if !radius.is_finite() || radius <= 0.0 || radius > FADE_MAX_RADIUS {
        return None;
    }
    let max = round_to((radius * 60.0).clamp(FADE_MIN, FADE_MAX), 50.0);
    Some((round_to(max * 0.75, 50.0), max))
}

pub(crate) fn fade_for_detail(size: f64) -> Option<(f64, f64)> {
    if !size.is_finite() || size <= 0.0 {
        return None;
    }
    let max = round_to((size * 30.0).clamp(DETAIL_FADE_MIN, DETAIL_FADE_MAX), 50.0);
    Some((round_to(max * 0.6, 50.0), max))
}

fn all_entities(map: &Map) -> impl Iterator<Item = &Entity> {
    std::iter::once(&map.world).chain(map.entities.iter())
}

fn next_ids(map: &Map) -> (u32, u32) {
    let mut solid = 1u32;
    let mut side = 1u32;
    for e in all_entities(map) {
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

fn has_material(solid: &hp_vmf::Solid, needle: &str) -> bool {
    solid.sides.iter().any(|s| s.material.to_ascii_lowercase().contains(needle))
}

fn blocks(f: &MaterialFacts) -> bool {
    f.contents & contents::SOLID != 0
        && f.contents & (contents::TRANSLUCENT | contents::WINDOW | contents::GRATE) == 0
        && f.surf & surf::TRANS == 0
}

pub(crate) fn blockers(map: &Map, facts: &dyn Fn(&str) -> MaterialFacts) -> Vec<SolidGeom> {
    map.world
        .solids
        .iter()
        .filter(|s| !s.hidden && !s.sides.is_empty())
        .filter(|s| s.sides.iter().all(|x| x.disp.is_none() && blocks(&facts(&x.material))))
        .map(|s| hp_geom::build_solid(&s.sides))
        .filter(|g| g.valid && !g.faces.is_empty())
        .collect()
}

pub(crate) fn segment_blocked(a: DVec3, b: DVec3, solids: &[SolidGeom]) -> bool {
    let span = Aabb::from_points([a, b]);
    let ray = Ray { origin: a, dir: b - a };
    solids
        .iter()
        .filter(|g| g.aabb.intersects(&span))
        .any(|g| hp_geom::ray_solid(&ray, g).is_some_and(|(t, _)| (0.0..=1.0).contains(&t)))
}

fn inside_solid(p: DVec3, solids: &[SolidGeom]) -> bool {
    solids.iter().any(|g| g.aabb.contains_point(p) && g.faces.iter().all(|f| f.plane.distance(p) < -0.01))
}

pub(crate) fn skybox_box(map: &Map) -> Option<Aabb> {
    let cam = map.entities.iter().find(|e| e.classname.eq_ignore_ascii_case("sky_camera"))?.origin()?;
    let walls: Vec<SolidGeom> = map
        .world
        .solids
        .iter()
        .filter(|s| has_material(s, "toolsskybox"))
        .map(|s| hp_geom::build_solid(&s.sides))
        .filter(|g| g.valid)
        .collect();
    let mut lo = cam - DVec3::splat(2048.0);
    let mut hi = cam + DVec3::splat(2048.0);
    for axis in 0..3 {
        for sign in [-1.0, 1.0] {
            let mut dir = DVec3::ZERO;
            dir[axis] = sign;
            let ray = Ray { origin: cam, dir };
            let hit = walls.iter().filter_map(|g| hp_geom::ray_solid(&ray, g).map(|(t, _)| t)).reduce(f64::min);
            if let Some(t) = hit {
                if sign < 0.0 {
                    lo[axis] = cam[axis] - t;
                } else {
                    hi[axis] = cam[axis] + t;
                }
            }
        }
    }
    Some(Aabb::new(lo, hi))
}

#[derive(Debug, Default, Clone, PartialEq)]
pub(crate) struct FadeReport {
    pub(crate) props: usize,
    pub(crate) kept: usize,
    pub(crate) skybox: usize,
    pub(crate) big: usize,
    pub(crate) detail: Option<(f64, f64)>,
}

pub(crate) fn auto_fade(
    map: &mut Map,
    bounds: &dyn Fn(&str) -> Option<(DVec3, DVec3)>,
    facts: &dyn Fn(&str) -> MaterialFacts,
    detail_size: &dyn Fn(&[String]) -> Option<f64>,
) -> FadeReport {
    let mut report = FadeReport::default();
    let sky = skybox_box(map);
    for e in map.entities.iter_mut().filter(|e| e.classname.eq_ignore_ascii_case("prop_static")) {
        if !unset(e.get("fademaxdist")) || e.get("screenspacefade").is_some_and(|v| v.trim() == "1") {
            report.kept += 1;
            continue;
        }
        let Some(b) = prop_box(e, bounds) else { continue };
        if sky.is_some_and(|s| s.contains_point(b.center)) {
            report.skybox += 1;
            continue;
        }
        let Some((min, max)) = fade_for_radius(b.radius) else {
            report.big += 1;
            continue;
        };
        e.set("fademindist", format!("{min}"));
        e.set("fademaxdist", format!("{max}"));
        report.props += 1;
    }
    if map.entities.iter().any(|e| e.classname.eq_ignore_ascii_case("env_detail_controller")) {
        return report;
    }
    let mut types: Vec<String> = Vec::new();
    let mut seen: HashMap<String, ()> = HashMap::new();
    for side in all_entities(map).flat_map(|e| e.solids.iter()).flat_map(|s| s.sides.iter()) {
        let key = side.material.to_ascii_lowercase();
        if seen.insert(key, ()).is_some() {
            continue;
        }
        if let Some(t) = facts(&side.material).detail_type
            && !types.iter().any(|x| x.eq_ignore_ascii_case(&t))
        {
            types.push(t);
        }
    }
    if types.is_empty() {
        return report;
    }
    let Some((min, max)) = detail_size(&types).and_then(fade_for_detail) else { return report };
    let id = next_ids(map).0;
    map.entities.push(Entity {
        id,
        classname: "env_detail_controller".to_owned(),
        props: vec![
            ("origin".to_owned(), "0 0 0".to_owned()),
            ("fademindist".to_owned(), format!("{min}")),
            ("fademaxdist".to_owned(), format!("{max}")),
        ],
        ..Entity::default()
    });
    report.detail = Some((min, max));
    report
}

pub(crate) fn detail_size(
    fs: &hp_fs::GameFs,
    world: &Entity,
    types: &[String],
    bounds: &dyn Fn(&str) -> Option<(DVec3, DVec3)>,
) -> Option<f64> {
    let file = world.get("detailvbsp").map(str::trim).filter(|s| !s.is_empty()).unwrap_or("detail.vbsp");
    let text = fs.read(file)?;
    let dict = hp_bspc::detail_props::parse_detail_vbsp(&String::from_utf8_lossy(&text)).ok()?;
    let mut size: f64 = 0.0;
    for t in dict.types.iter().filter(|t| types.iter().any(|x| x.eq_ignore_ascii_case(&t.name))) {
        for m in t.groups.iter().flat_map(|g| g.models.iter()) {
            let s = match &m.model {
                Some(path) => bounds(&path.replace('\\', "/").to_ascii_lowercase())
                    .map_or(0.0, |(lo, hi)| (hi - lo).max_element()),
                None => {
                    let w = f64::from((m.pos[1][0] - m.pos[0][0]).abs());
                    let h = f64::from((m.pos[1][1] - m.pos[0][1]).abs());
                    w.max(h)
                }
            };
            size = size.max(s);
        }
    }
    (size > 0.0).then_some(size)
}

#[derive(Clone, Debug, PartialEq)]
pub(crate) struct Occluder {
    pub(crate) slab: Aabb,
    pub(crate) axis: usize,
    pub(crate) props: usize,
}

pub(crate) fn plan_occluders(
    map: &Map,
    bounds: &dyn Fn(&str) -> Option<(DVec3, DVec3)>,
    facts: &dyn Fn(&str) -> MaterialFacts,
) -> Vec<Occluder> {
    let solids = blockers(map, facts);
    let props: Vec<PropBox> = map
        .entities
        .iter()
        .filter(|e| is_prop_class(&e.classname) && !e.hidden)
        .filter_map(|e| {
            prop_box(e, bounds)
                .or_else(|| e.origin().filter(|o| o.is_finite()).map(|o| PropBox { center: o, radius: 16.0 }))
        })
        .collect();
    let points: Vec<DVec3> = map.entities.iter().filter(|e| e.solids.is_empty()).filter_map(Entity::origin).collect();
    let sky = skybox_box(map);
    let mut out: Vec<(f64, Occluder)> = Vec::new();
    for solid in map.world.solids.iter().filter(|s| !s.hidden && s.sides.len() == 6) {
        let normals: Vec<DVec3> = solid.sides.iter().map(hp_vmf::Side::normal).collect();
        if normals.iter().any(|n| !n.is_finite() || n.abs().max_element() < 0.9999) {
            continue;
        }
        let g = hp_geom::build_solid(&solid.sides);
        if !g.valid {
            continue;
        }
        let size = g.aabb.size();
        let axis = if size.x <= size.y && size.x <= size.z {
            0
        } else if size.y <= size.z {
            1
        } else {
            2
        };
        let (u, v) = ((axis + 1) % 3, (axis + 2) % 3);
        let (long, short) = (size[u].max(size[v]), size[u].min(size[v]));
        if size[axis] < 8.0 || long < 256.0 || short < 128.0 {
            continue;
        }
        if sky.is_some_and(|s| s.intersects(&g.aabb)) {
            continue;
        }
        let big_faces_opaque = solid.sides.iter().filter(|s| s.normal()[axis].abs() > 0.9999).all(|s| {
            let f = facts(&s.material);
            blocks(&f) && f.surf & (surf::SKY | surf::SKY2D | surf::HINT | surf::SKIP | surf::NODRAW) == 0
        });
        if !big_faces_opaque {
            continue;
        }
        let (lo, hi) = (g.aabb.min, g.aabb.max);
        let behind = |p: &PropBox, sign: f64| {
            let d = if sign > 0.0 { p.center[axis] - hi[axis] } else { lo[axis] - p.center[axis] };
            d > p.radius
                && d < OCCLUDER_RANGE
                && p.center[u] - p.radius > lo[u]
                && p.center[u] + p.radius < hi[u]
                && p.center[v] - p.radius > lo[v]
                && p.center[v] + p.radius < hi[v]
        };
        let near = |p: DVec3, sign: f64| {
            let d = if sign > 0.0 { p[axis] - hi[axis] } else { lo[axis] - p[axis] };
            d > 0.0
                && d < OCCLUDER_RANGE
                && p[u] > lo[u] - 512.0
                && p[u] < hi[u] + 512.0
                && p[v] > lo[v] - 512.0
                && p[v] < hi[v] + 512.0
        };
        let count = |sign: f64| props.iter().filter(|p| behind(p, sign)).count();
        let (front, back) = (count(1.0), count(-1.0));
        let (sign, hidden) = if front >= back { (1.0, front) } else { (-1.0, back) };
        if hidden < OCCLUDER_MIN_PROPS || !points.iter().any(|p| near(*p, -sign)) {
            continue;
        }
        let mut slab_lo = lo;
        let mut slab_hi = hi;
        for k in [u, v] {
            slab_lo[k] += 2.0;
            slab_hi[k] -= 2.0;
        }
        if sign > 0.0 {
            slab_lo[axis] = hi[axis] + 1.0;
            slab_hi[axis] = hi[axis] + 2.0;
        } else {
            slab_lo[axis] = lo[axis] - 2.0;
            slab_hi[axis] = lo[axis] - 1.0;
        }
        let slab = Aabb::new(slab_lo, slab_hi);
        if slab.corners().iter().any(|c| inside_solid(*c, &solids)) || inside_solid(slab.center(), &solids) {
            continue;
        }
        out.push((long * short * hidden as f64, Occluder { slab, axis, props: hidden }));
    }
    out.sort_by(|a, b| b.0.total_cmp(&a.0));
    out.truncate(OCCLUDER_MAX);
    out.into_iter().map(|(_, o)| o).collect()
}

pub(crate) fn add_occluders(map: &mut Map, list: &[Occluder]) -> usize {
    let mut ids = next_ids(map);
    let mut added = 0;
    for o in list {
        let mut next = ids.1;
        let Ok(mut sides) = hp_geom::hull_solid(&o.slab.corners(), NODRAW, 0.25, 16, &mut || {
            next += 1;
            next
        }) else {
            continue;
        };
        ids.1 = next + 1;
        for s in &mut sides {
            if s.normal()[o.axis].abs() > 0.999 {
                s.material = OCCLUDER.to_owned();
            }
        }
        let solid = hp_vmf::Solid { id: ids.0, sides, ..hp_vmf::Solid::default() };
        ids.0 += 1;
        map.entities.push(Entity {
            id: ids.0,
            classname: "func_occluder".to_owned(),
            props: vec![("StartActive".to_owned(), "1".to_owned())],
            solids: vec![solid],
            ..Entity::default()
        });
        ids.0 += 1;
        added += 1;
    }
    added
}

const MERGE_IGNORED_KEYS: [&str; 6] = ["origin", "angles", "model", "hammerid", "modelscale", "uniformscale"];

pub(crate) fn merge_key(e: &Entity) -> Option<String> {
    if !e.classname.eq_ignore_ascii_case("prop_static") || e.hidden || model_of(e).is_none() {
        return None;
    }
    let named = |k: &str| e.get(k).is_some_and(|v| !v.trim().is_empty());
    if named("targetname") || named("parentname") || !e.connections.is_empty() {
        return None;
    }
    let mut keys: BTreeMap<String, String> = BTreeMap::new();
    for (k, v) in &e.props {
        let k = k.to_ascii_lowercase();
        if !MERGE_IGNORED_KEYS.contains(&k.as_str()) {
            keys.insert(k, v.trim().to_owned());
        }
    }
    for (k, default) in [("skin", "0"), ("solid", "6"), ("disableshadows", "0"), ("disablevertexlighting", "0")] {
        keys.entry(k.to_owned()).or_insert_with(|| default.to_owned());
    }
    Some(keys.iter().map(|(k, v)| format!("{k}={v}")).collect::<Vec<_>>().join(";"))
}

fn find(parent: &mut [usize], i: usize) -> usize {
    let mut r = i;
    while parent[r] != r {
        r = parent[r];
    }
    let mut i = i;
    while parent[i] != r {
        let n = parent[i];
        parent[i] = r;
        i = n;
    }
    r
}

pub(crate) fn merge_clusters(
    map: &Map,
    group_of: &mut dyn FnMut(&Entity) -> Option<String>,
    solids: &[SolidGeom],
) -> Vec<Vec<usize>> {
    type Cell = (String, [i64; 3]);
    let mut groups: BTreeMap<Cell, Vec<(usize, DVec3)>> = BTreeMap::new();
    for (i, e) in map.entities.iter().enumerate() {
        let Some(origin) = e.origin().filter(|o| o.is_finite()) else { continue };
        let Some(key) = group_of(e) else { continue };
        let cell = (origin / MERGE_CELL).floor();
        groups.entry((key, [cell.x as i64, cell.y as i64, cell.z as i64])).or_default().push((i, origin));
    }
    let mut out = Vec::new();
    for members in groups.values().filter(|m| m.len() >= 2) {
        let mut parent: Vec<usize> = (0..members.len()).collect();
        for a in 0..members.len() {
            for b in a + 1..members.len() {
                let (pa, pb) = (members[a].1, members[b].1);
                if pa.distance(pb) > MERGE_LINK || find(&mut parent, a) == find(&mut parent, b) {
                    continue;
                }
                let lift = DVec3::new(0.0, 0.0, 8.0);
                if segment_blocked(pa + lift, pb + lift, solids) {
                    continue;
                }
                let (ra, rb) = (find(&mut parent, a), find(&mut parent, b));
                parent[ra] = rb;
            }
        }
        let mut comps: BTreeMap<usize, Vec<usize>> = BTreeMap::new();
        for (k, member) in members.iter().enumerate() {
            let r = find(&mut parent, k);
            comps.entry(r).or_default().push(member.0);
        }
        for comp in comps.into_values() {
            for chunk in comp.chunks(MERGE_MAX_PROPS) {
                if chunk.len() >= 2 {
                    out.push(chunk.to_vec());
                }
            }
        }
    }
    out
}

#[derive(Debug, Default, Clone, PartialEq)]
pub(crate) struct MergeReport {
    pub(crate) props: usize,
    pub(crate) models: usize,
    pub(crate) draw_before: usize,
    pub(crate) draw_after: usize,
    pub(crate) failed: Vec<String>,
}

pub(crate) fn progress_line(
    label: &str,
    done: usize,
    total: usize,
    step: &str,
    elapsed: std::time::Duration,
) -> String {
    use hp_bspc::rad::progress::duration_text;
    let done = done.min(total);
    let pct = (done * 100).checked_div(total).unwrap_or(100);
    let left = if done == total {
        "done".to_owned()
    } else if done == 0 {
        "estimating the time left".to_owned()
    } else {
        let rest = elapsed.as_secs_f64() * (total - done) as f64 / done as f64;
        format!("about {} left", duration_text(std::time::Duration::from_secs_f64(rest.ceil())))
    };
    format!("Optimizing {pct}%: {label}: {step}, {} elapsed, {left}", duration_text(elapsed))
}

pub(crate) struct Stage<'a> {
    label: &'static str,
    start: std::time::Instant,
    state: std::sync::Mutex<(usize, usize, String)>,
    last: std::sync::Mutex<std::time::Instant>,
    log: &'a (dyn Fn(&str) + Sync),
}

impl<'a> Stage<'a> {
    pub(crate) fn new(label: &'static str, total: usize, log: &'a (dyn Fn(&str) + Sync)) -> Self {
        let state = std::sync::Mutex::new((0, total, String::new()));
        let now = std::time::Instant::now();
        Stage { label, start: now, state, last: std::sync::Mutex::new(now), log }
    }

    fn line(&self) -> String {
        let state = self.state.lock().unwrap_or_else(std::sync::PoisonError::into_inner);
        progress_line(self.label, state.0, state.1, &state.2, self.start.elapsed())
    }

    pub(crate) fn report(&self, done: usize, step: String) {
        {
            let mut state = self.state.lock().unwrap_or_else(std::sync::PoisonError::into_inner);
            state.0 = done.max(state.0);
            state.2 = step;
        }
        self.say();
    }

    fn say(&self) {
        *self.last.lock().unwrap_or_else(std::sync::PoisonError::into_inner) = std::time::Instant::now();
        (self.log)(&self.line());
    }

    pub(crate) fn set_total(&self, total: usize) {
        self.state.lock().unwrap_or_else(std::sync::PoisonError::into_inner).1 = total;
    }

    pub(crate) fn total(&self) -> usize {
        self.state.lock().unwrap_or_else(std::sync::PoisonError::into_inner).1
    }

    pub(crate) fn run<R>(&self, work: impl FnOnce() -> R) -> R {
        let (stop, wait) = std::sync::mpsc::channel::<()>();
        std::thread::scope(|scope| {
            scope.spawn(move || {
                while let Err(std::sync::mpsc::RecvTimeoutError::Timeout) =
                    wait.recv_timeout(std::time::Duration::from_secs(PROGRESS_TICK_SECS))
                {
                    let quiet = self.last.lock().unwrap_or_else(std::sync::PoisonError::into_inner).elapsed();
                    if quiet.as_secs_f64() >= PROGRESS_TICK_SECS as f64 - 0.5 {
                        self.say();
                    }
                }
            });
            let out = work();
            drop(stop);
            out
        })
    }
}

pub(crate) fn merge_workers(groups: usize) -> usize {
    let cores = std::thread::available_parallelism().map_or(1, std::num::NonZeroUsize::get);
    (cores / 2).clamp(1, MERGE_MAX_WORKERS).min(groups.max(1))
}

type Built = (hp_ui_mapping::propper::PreparedProp, Vec<(String, String)>, (usize, usize));

#[allow(clippy::too_many_arguments)]
pub(crate) fn merge_static_props(
    map: &mut Map,
    fs: &hp_fs::GameFs,
    exe: &Path,
    extra_args: &str,
    out_dir: &Path,
    stem: &str,
    facts: &dyn Fn(&str) -> MaterialFacts,
    log: &(dyn Fn(&str) + Sync),
) -> MergeReport {
    use hp_ui_mapping::propper::combine;
    let mut report = MergeReport::default();
    let statics = map.entities.iter().filter(|e| merge_key(e).is_some()).count();
    if statics < 2 {
        return report;
    }
    log(&format!("Optimizing 0%: Merging static props: grouping {statics} static props by model and materials..."));
    let solids = blockers(map, facts);
    let mut materials: HashMap<String, Option<String>> = HashMap::new();
    let mut group_of = |e: &Entity| -> Option<String> {
        let key = merge_key(e)?;
        let model = model_of(e)?;
        let id = format!("{model}|{}|{}", e.get("skin").unwrap_or("0").trim(), e.get("body").unwrap_or("0").trim());
        let mats = materials
            .entry(id)
            .or_insert_with(|| {
                combine::prop_materials(fs, e)
                    .ok()
                    .filter(|(m, tris)| !m.is_empty() && *tris > 0)
                    .map(|(m, _)| m.into_iter().collect::<Vec<_>>().join(","))
            })
            .clone()?;
        Some(format!("{key}|{mats}"))
    };
    let mut clusters = merge_clusters(map, &mut group_of, &solids);
    clusters.sort_by_key(|c| std::cmp::Reverse(c.len()));
    clusters.truncate(MERGE_MAX_MODELS);
    if clusters.is_empty() {
        return report;
    }
    let _ = std::fs::remove_dir_all(out_dir);
    let source_dir = out_dir.with_file_name(format!(
        "{}_src",
        out_dir.file_name().map_or_else(|| "merged".into(), |n| n.to_string_lossy().into_owned())
    ));
    let stem: String =
        stem.to_ascii_lowercase().chars().filter(|c| c.is_ascii_alphanumeric() || *c == '_').take(40).collect();
    let total = clusters.len();
    let workers = merge_workers(total);
    log(&format!(
        "-mergestaticprops: {total} groups to build with the model compiler, {workers} at a time (each one stopped after {MERGE_MODEL_TIMEOUT_SECS} s)"
    ));
    let entities: Vec<Vec<Entity>> =
        clusters.iter().map(|c| c.iter().map(|&i| map.entities[i].clone()).collect()).collect();
    let results: Vec<std::sync::Mutex<Option<Result<Built, String>>>> =
        (0..total).map(|_| std::sync::Mutex::new(None)).collect();
    let next = std::sync::atomic::AtomicUsize::new(0);
    let finished = std::sync::atomic::AtomicUsize::new(0);
    let finish_lock = std::sync::Mutex::new(());
    let stage = Stage::new("Merging static props", total, log);
    stage.report(0, format!("building group 1 of {total}"));
    let build_one = |n: usize| -> Result<Built, String> {
        let settings = combine::CombineSettings {
            origin: hp_app::settings::CombineOrigin::Center,
            keep_originals: false,
            collision: true,
            max_pieces: 16,
            model: format!("hammerpro/merged/{stem}_{n:03}"),
            destination: out_dir.to_path_buf(),
            source_dir: source_dir.clone(),
            lod: hp_app::settings::ModelLodSettings::default(),
        };
        let planned = combine::plan_entities(fs, &entities[n], &settings, 0)?;
        let draw = planned.draw_calls;
        let keys = planned.keys.clone();
        let mut errors: Vec<String> = Vec::new();
        let limits = combine::BuildLimits {
            timeout: Some(std::time::Duration::from_secs(MERGE_MODEL_TIMEOUT_SECS)),
            cancel: None,
            finish: Some(&finish_lock),
        };
        let built = combine::build_limited(fs, planned.creation, exe, extra_args, &limits, &mut |sev, line| {
            if sev == hp_compile::Severity::Error && errors.len() < 4 {
                errors.push(line);
            }
        });
        match built {
            Ok(p) => Ok((p, keys, draw)),
            Err(e) => {
                for l in &errors {
                    log(l);
                }
                Err(e)
            }
        }
    };
    stage.run(|| {
        std::thread::scope(|scope| {
            for _ in 0..workers {
                scope.spawn(|| {
                    loop {
                        let n = next.fetch_add(1, std::sync::atomic::Ordering::Relaxed);
                        if n >= total {
                            break;
                        }
                        let r = build_one(n);
                        let what = if r.is_ok() { "built" } else { "left as is" };
                        *results[n].lock().unwrap_or_else(std::sync::PoisonError::into_inner) = Some(r);
                        let done = finished.fetch_add(1, std::sync::atomic::Ordering::Relaxed) + 1;
                        stage.report(done, format!("{done} of {total} groups done (the last one {what})"));
                    }
                });
            }
        });
    });
    let mut ids = next_ids(map);
    let mut removed = vec![false; map.entities.len()];
    let mut added = Vec::new();
    for (cluster, slot) in clusters.iter().zip(results) {
        let result = slot.into_inner().unwrap_or_else(std::sync::PoisonError::into_inner);
        let (prepared, keys, draw) = match result {
            Some(Ok(b)) => b,
            Some(Err(e)) => {
                report.failed.push(e);
                continue;
            }
            None => continue,
        };
        let o = prepared.origin;
        let mut props = vec![
            ("model".to_owned(), format!("models/{}.mdl", prepared.model)),
            ("origin".to_owned(), format!("{} {} {}", o.x, o.y, o.z)),
            ("angles".to_owned(), "0 0 0".to_owned()),
        ];
        props.extend(
            keys.into_iter().filter(|(k, _)| !["model", "origin", "angles"].iter().any(|x| k.eq_ignore_ascii_case(x))),
        );
        added.push(Entity { id: ids.0, classname: "prop_static".to_owned(), props, ..Entity::default() });
        ids.0 += 1;
        for &i in cluster {
            removed[i] = true;
        }
        report.props += cluster.len();
        report.models += 1;
        report.draw_before += draw.0;
        report.draw_after += draw.1;
    }
    let _ = std::fs::remove_dir_all(&source_dir);
    let mut k = 0;
    map.entities.retain(|_| {
        let keep = !removed[k];
        k += 1;
        keep
    });
    map.entities.extend(added);
    report
}

#[derive(Debug, Default, Clone, PartialEq)]
pub(crate) struct HintReport {
    pub(crate) added: usize,
    pub(crate) measured: usize,
    pub(crate) base_pvs: u64,
    pub(crate) gain: i64,
    pub(crate) note: Option<String>,
}

pub(crate) fn existing_hints(map: &Map) -> Vec<Aabb> {
    map.world
        .solids
        .iter()
        .filter(|s| s.sides.iter().any(|x| x.material.eq_ignore_ascii_case("tools/toolshint")))
        .map(|s| hp_geom::build_solid(&s.sides).aabb.expand(4.0))
        .collect()
}

pub(crate) fn auto_hints(
    map: &mut Map,
    fs: Option<&hp_fs::GameFs>,
    log: &(dyn Fn(&str) + Sync),
) -> Result<HintReport, String> {
    use crate::dispatch::health::optimize;
    let start = std::time::Instant::now();
    let total = HINT_CANDIDATES + 2;
    let stage = Stage::new("Auto hints", total, log);
    stage.report(0, "reference compile with fast visibility".to_owned());
    let (base, leaked, _) = stage.run(|| optimize::compile(map, fs, false))?;
    if leaked {
        return Err("the map leaks".to_owned());
    }
    let base_secs = start.elapsed().as_secs_f64();
    let existing = existing_hints(map);
    let (list, note) = stage.run(|| {
        optimize::evaluate_hints(
            map,
            fs,
            &existing,
            &base,
            false,
            HINT_CANDIDATES,
            start,
            HINT_BUDGET_SECS,
            base_secs,
            &mut |i, n| {
                let step = if i < n {
                    format!("measuring {} of {n}", i + 1)
                } else {
                    "checking the hints together".to_owned()
                };
                stage.set_total(n + 2);
                stage.report(1 + i.min(n), step);
            },
        )
    });
    stage.report(stage.total(), "done".to_owned());
    let good: Vec<_> = list.iter().filter(|c| c.gain.is_some_and(|g| g > 0)).cloned().collect();
    let report = HintReport {
        added: good.len(),
        measured: list.iter().filter(|c| c.gain.is_some()).count(),
        base_pvs: base.report.total_pvs,
        gain: good.iter().filter_map(|c| c.gain).sum(),
        note,
    };
    optimize::simulate(map, &[], &good, &[]);
    Ok(report)
}

#[cfg(test)]
mod tests {
    use super::*;

    fn by_name(name: &str) -> MaterialFacts {
        let mut f = hp_bspc::materials::facts_by_name(name);
        if name.starts_with("glass/") {
            f.contents |= contents::TRANSLUCENT;
        }
        f
    }

    fn block(id: u32, lo: [f64; 3], hi: [f64; 3], material: &str) -> hp_vmf::Solid {
        let mut next = id * 10;
        let sides = hp_geom::hull_solid(
            &Aabb::new(DVec3::from(lo), DVec3::from(hi)).corners(),
            material,
            0.25,
            16,
            &mut || {
                next += 1;
                next
            },
        )
        .expect("box");
        hp_vmf::Solid { id, sides, ..hp_vmf::Solid::default() }
    }

    fn prop(id: u32, at: [f64; 3], model: &str, extra: &[(&str, &str)]) -> Entity {
        let mut props = vec![
            ("origin".to_owned(), format!("{} {} {}", at[0], at[1], at[2])),
            ("model".to_owned(), model.to_owned()),
        ];
        props.extend(extra.iter().map(|(k, v)| ((*k).to_owned(), (*v).to_owned())));
        Entity { id, classname: "prop_static".to_owned(), props, ..Entity::default() }
    }

    fn sized(path: &str) -> Option<(DVec3, DVec3)> {
        match path {
            "models/can.mdl" => Some((DVec3::splat(-4.0), DVec3::splat(4.0))),
            "models/crate.mdl" => Some((DVec3::splat(-20.0), DVec3::splat(20.0))),
            "models/building.mdl" => Some((DVec3::splat(-400.0), DVec3::splat(400.0))),
            _ => None,
        }
    }

    #[test]
    fn autofade_sizes_props_and_keeps_mapper_values() {
        let mut map = Map::default();
        map.entities.push(prop(1, [0.0, 0.0, 0.0], "models/can.mdl", &[]));
        map.entities.push(prop(2, [100.0, 0.0, 0.0], "models/crate.mdl", &[("fademaxdist", "0")]));
        map.entities.push(prop(
            3,
            [200.0, 0.0, 0.0],
            "models/crate.mdl",
            &[("fademindist", "100"), ("fademaxdist", "300")],
        ));
        map.entities.push(prop(4, [300.0, 0.0, 0.0], "models/building.mdl", &[]));
        map.entities.push(prop(5, [10000.0, 0.0, 0.0], "models/can.mdl", &[]));
        map.entities.push(Entity {
            id: 6,
            classname: "sky_camera".to_owned(),
            props: vec![("origin".to_owned(), "10000 0 0".to_owned())],
            ..Entity::default()
        });
        let r = auto_fade(&mut map, &sized, &by_name, &|_| None);
        assert_eq!((r.props, r.kept, r.big, r.skybox), (2, 1, 1, 1));
        let max = |i: usize| map.entities[i].get("fademaxdist").and_then(|v| v.parse::<f64>().ok()).unwrap_or(0.0);
        assert!(max(0) > 0.0 && max(0) < max(1), "small props fade earlier");
        assert!(max(1) <= FADE_MAX);
        assert_eq!(map.entities[2].get("fademaxdist"), Some("300"));
        assert!(map.entities[3].get("fademaxdist").is_none() && map.entities[4].get("fademaxdist").is_none());
        let min = map.entities[0].get("fademindist").and_then(|v| v.parse::<f64>().ok()).unwrap_or(0.0);
        assert!(min > 0.0 && min < max(0));
        assert_eq!(fade_for_detail(16.0), Some((300.0, 500.0)));
        assert_eq!(fade_for_detail(200.0).map(|f| f.1), Some(DETAIL_FADE_MAX));
    }

    #[test]
    fn merge_clusters_stay_in_one_room() {
        let mut map = Map::default();
        map.world.solids.push(block(1, [96.0, -512.0, -64.0], [112.0, 512.0, 256.0], "brick/brickwall001a"));
        map.entities.push(prop(10, [0.0, 0.0, 0.0], "models/crate.mdl", &[]));
        map.entities.push(prop(11, [40.0, 0.0, 0.0], "models/crate.mdl", &[]));
        map.entities.push(prop(12, [60.0, 60.0, 0.0], "models/crate.mdl", &[]));
        map.entities.push(prop(13, [200.0, 0.0, 0.0], "models/crate.mdl", &[]));
        map.entities.push(prop(14, [240.0, 0.0, 0.0], "models/crate.mdl", &[]));
        map.entities.push(prop(15, [20.0, 20.0, 0.0], "models/crate.mdl", &[("skin", "1")]));
        map.entities.push(prop(16, [30.0, -20.0, 0.0], "models/crate.mdl", &[("targetname", "door_crate")]));
        map.entities.push(prop(17, [30.0, -40.0, 0.0], "models/crate.mdl", &[("disableshadows", "1")]));
        let solids = blockers(&map, &by_name);
        assert_eq!(solids.len(), 1);
        let mut key = |e: &Entity| merge_key(e);
        let mut clusters = merge_clusters(&map, &mut key, &solids);
        clusters.iter_mut().for_each(|c| c.sort_unstable());
        clusters.sort();
        assert_eq!(clusters, vec![vec![0, 1, 2], vec![3, 4]]);
    }

    #[test]
    fn autooccluders_cover_big_walls_hiding_props() {
        let mut map = Map::default();
        map.world.solids.push(block(1, [0.0, -512.0, 0.0], [16.0, 512.0, 256.0], "brick/brickwall001a"));
        map.world.solids.push(block(2, [600.0, -512.0, 0.0], [616.0, -480.0, 64.0], "brick/brickwall001a"));
        map.world.solids.push(block(3, [-600.0, -512.0, 0.0], [-584.0, 512.0, 256.0], "glass/glasswindow001a"));
        for (i, y) in [-200.0, 0.0, 200.0, 300.0].iter().enumerate() {
            map.entities.push(prop(10 + i as u32, [300.0, *y, 64.0], "models/crate.mdl", &[]));
            map.entities.push(prop(20 + i as u32, [-900.0, *y, 64.0], "models/crate.mdl", &[]));
        }
        map.entities.push(Entity {
            id: 30,
            classname: "info_player_start".to_owned(),
            props: vec![("origin".to_owned(), "-300 0 0".to_owned())],
            ..Entity::default()
        });
        let list = plan_occluders(&map, &sized, &by_name);
        assert_eq!(list.len(), 1, "{list:?}");
        let o = &list[0];
        assert_eq!((o.axis, o.props), (0, 4));
        assert!(o.slab.min.x > 16.0 && o.slab.max.x <= 18.0);
        let before = map.entities.len();
        assert_eq!(add_occluders(&mut map, &list), 1);
        let occ = &map.entities[before];
        assert_eq!(occ.classname, "func_occluder");
        let faces = occ.solids[0].sides.iter().filter(|s| s.material == OCCLUDER).count();
        assert_eq!(faces, 2);
        let mut ids: Vec<u32> =
            map.entities.iter().map(|e| e.id).chain(map.world.solids.iter().map(|s| s.id)).collect();
        ids.extend(map.entities.iter().flat_map(|e| e.solids.iter().map(|s| s.id)));
        let n = ids.len();
        ids.sort_unstable();
        ids.dedup();
        assert_eq!(ids.len(), n, "unique ids");
    }

    #[test]
    fn optimization_progress_lines_give_percent_and_time_left() {
        let d = std::time::Duration::from_secs;
        assert_eq!(
            progress_line("Merging static props", 0, 32, "building group 1 of 32", d(0)),
            "Optimizing 0%: Merging static props: building group 1 of 32, 0 s elapsed, estimating the time left"
        );
        assert_eq!(
            progress_line("Merging static props", 8, 32, "8 of 32 groups done", d(12)),
            "Optimizing 25%: Merging static props: 8 of 32 groups done, 12 s elapsed, about 36 s left"
        );
        assert_eq!(
            progress_line("Auto hints", 10, 10, "done", d(75)),
            "Optimizing 100%: Auto hints: done, 1 min 15 s elapsed, done"
        );
        assert_eq!(merge_workers(1), 1);
        assert!((1..=MERGE_MAX_WORKERS).contains(&merge_workers(32)));
        let o = OptimizeArgs { occluders: true, merge_props: true, ..OptimizeArgs::default() };
        assert_eq!(o.flags(), vec!["-autooccluders", "-mergestaticprops"]);
        let lines = std::sync::Mutex::new(Vec::new());
        let log = |l: &str| lines.lock().unwrap().push(l.to_owned());
        let stage = Stage::new("Auto hints", 4, &log);
        stage.report(1, "measuring 1 of 2".to_owned());
        stage.set_total(3);
        stage.report(3, "done".to_owned());
        drop(stage);
        let lines = lines.into_inner().unwrap();
        assert!(lines[0].starts_with("Optimizing 25%: Auto hints: measuring 1 of 2"), "{lines:?}");
        assert!(lines[1].starts_with("Optimizing 100%: Auto hints: done"), "{lines:?}");
    }

    #[test]
    fn existing_hints_are_found_and_merged_dir_is_beside_the_map() {
        let mut map = Map::default();
        map.world.solids.push(block(1, [0.0, 0.0, 0.0], [64.0, 64.0, 64.0], "tools/toolshint"));
        map.world.solids.push(block(2, [100.0, 0.0, 0.0], [164.0, 64.0, 64.0], "brick/brickwall001a"));
        assert_eq!(existing_hints(&map).len(), 1);
        assert_eq!(merged_dir(Path::new("E:/maps/gm_x.vmf")), PathBuf::from("E:/maps/gm_x_merged_props"));
    }
}
