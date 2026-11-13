mod engine;
mod knowledge;

use std::path::PathBuf;
use std::sync::Arc;

use glam::DVec3;
use hp_editor::edicts::edict_problems;
use hp_editor::limits::cost::{RenderCost, RenderCosts};
use hp_editor::limits::leak::LeakPath;
use hp_editor::limits::portals::{BspVis, PortalFile};
use hp_editor::limits::{LimitProfile, LimitUse, limit_problems, worst};
use hp_editor::{ObjRef, Problem, Severity};
use serde::{Deserialize, Serialize};

pub use engine::{CheckResult, LeakResult};

use crate::sidecar::SidecarSection;
use crate::state::AppState;

const MAX_IGNORED: usize = 4096;

#[derive(Clone, Debug, Default, PartialEq, Eq, Serialize, Deserialize)]
#[serde(default)]
pub struct IgnoredProblems {
    pub keys: Vec<String>,
    pub codes: Vec<String>,
}

impl SidecarSection for IgnoredProblems {
    const NAME: &'static str = "health_ignored";
}

#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub struct ProblemCounts {
    pub errors: usize,
    pub warnings: usize,
    pub infos: usize,
    pub ignored: usize,
}

#[derive(Clone, Debug, PartialEq)]
pub struct ExactLimits {
    pub path: PathBuf,
    pub modified: u64,
    pub uses: Vec<LimitUse>,
    pub areas: usize,
    pub clusters: u32,
}

const WINDOW_FADE_BUFFER: f64 = 10.0;

type PlayKey = (u32, Vec<String>, Vec<(String, bool)>, Vec<bool>);

#[derive(Debug, Default)]
pub struct HealthRuntime {
    pub problems: Vec<Problem>,
    pub counts: ProblemCounts,
    pub revision: u64,
    pub checks: Option<Arc<CheckResult>>,
    pub leak: Option<Arc<LeakResult>>,
    pub exact: Option<ExactLimits>,
    pub ignored: IgnoredProblems,
    pub recheck: bool,
    pub checking: bool,
    pub leak_checking: bool,
    pub profile: LimitProfile,
    pub show_leak: bool,
    pub leak_requested: bool,
    pub leak_version: u64,

    pub portals: Option<Arc<PortalFile>>,
    pub portal_path: Option<PathBuf>,
    pub portal_version: u64,
    pub portal_offer: Option<PathBuf>,
    pub show_portal_window: bool,
    pub vis: Option<Arc<BspVis>>,
    pub vis_path: Option<PathBuf>,
    pub pvs: bool,
    pub pvs_point: Option<DVec3>,
    pub pvs_cluster: Option<u32>,
    pub pvs_visible: Option<Arc<Vec<bool>>>,
    pub pvs_version: u64,
    pub portal_build_running: bool,
    pub portal_build_request: bool,
    pub portal_load_request: Option<PathBuf>,
    pub portal_doc_revision: Option<u64>,
    pub portal_reload_request: bool,
    pub bsp_reload_request: bool,
    pub portal_cancel_request: bool,
    pub show_detail: bool,
    pub costs: Option<Arc<RenderCosts>>,
    pub world: Option<Arc<hp_editor::limits::world::BspWorld>>,
    pub pvs_cost: Option<RenderCost>,
    pub play_from: Option<DVec3>,
    pub play_open_doors: Vec<String>,
    pub play_view: Option<hp_editor::limits::portals::ViewCone>,
    pub play_flow: Option<Arc<hp_editor::limits::portals::AreaFlow>>,
    pub play_portal_states: Vec<(String, bool)>,
    pub play_cluster: Option<u32>,
    pub play_area: Option<u16>,
    pub play_visible: Option<Arc<Vec<bool>>>,
    pub play_cost: Option<RenderCost>,
    pub play_version: u64,
    play_key: Option<PlayKey>,
    pub placement: Vec<Problem>,
    pub placement_serial: Option<u64>,
    pub placement_checking: bool,
    pub placement_ms: u64,

    engine: engine::Engine,
}

impl HealthRuntime {
    pub fn profile_for(state: &AppState) -> LimitProfile {
        let auto = if state.settings.game.is_gmod() { LimitProfile::Gmod64 } else { LimitProfile::Stock };
        state.settings.health.profile().unwrap_or(auto)
    }

    pub fn tick(state: &mut AppState) -> bool {
        let profile = Self::profile_for(state);
        let live = state.settings.health.live_checks;
        let leak = state.settings.health.live_leak_check || state.runtime.health.leak_requested;
        let mode = Self::leak_mode(state, leak);
        let force = std::mem::take(&mut state.runtime.health.recheck);
        let pace = engine::Pace { force, ..Self::pace(state) };
        let AppState { doc, assets, runtime, .. } = state;
        let rt = &mut runtime.health;
        let tick = rt.engine.tick(doc, assets.as_mut(), profile, live, mode, pace);
        rt.profile = profile;
        let mut changed = false;
        if let Some(c) = tick.checks {
            rt.checks = Some(Arc::new(c));
            if !leak && rt.leak.take().is_some() {
                rt.leak_version += 1;
            }
            changed = true;
        }
        if let Some(l) = tick.leak {
            let path = l.report.leak.clone();
            if rt.leak.as_ref().map(|old| &old.report.leak) != Some(&path) {
                rt.leak_version += 1;
            }
            rt.leak = Some(Arc::new(l));
            changed = true;
        }
        rt.checking = tick.busy;
        rt.leak_checking = rt.engine.leak_pending();
        if changed {
            rt.merge();
        }
        tick.busy
    }

    fn leak_mode(state: &AppState, on: bool) -> engine::LeakMode {
        engine::LeakMode { on, cordons: state.settings.compile.compile_cordon_only && state.doc.cordons_in_use() }
    }

    pub fn user_busy(state: &AppState) -> bool {
        state.doc.in_transaction() || state.runtime.selection.drag.is_some() || crate::checks::window_away()
    }

    fn pace(state: &AppState) -> engine::Pace {
        crate::checks::set_pause_minimized(state.settings.health.pause_minimized);
        engine::Pace { delay: state.settings.health.check_delay(), hold: Self::user_busy(state), force: false }
    }

    pub fn check_now(state: &mut AppState) {
        let profile = Self::profile_for(state);
        let mode = Self::leak_mode(state, state.settings.health.live_leak_check);
        let AppState { doc, assets, runtime, .. } = state;
        let rt = &mut runtime.health;
        let (checks, leak_result) = rt.engine.check_now(doc, assets.as_mut(), profile, mode);
        rt.profile = profile;
        rt.checks = Some(Arc::new(checks));
        rt.leak = leak_result.map(Arc::new);
        rt.leak_version += 1;
        rt.checking = false;
        rt.leak_checking = false;
        rt.recheck = false;
        rt.merge();
    }

    fn merge(&mut self) {
        let mut all: Vec<Problem> = Vec::new();
        if let Some(c) = &self.checks {
            all.extend(c.problems.iter().cloned());
            all.extend(limit_problems(&c.limits));
            all.extend(edict_problems(&c.edicts));
        }
        if let Some(l) = &self.leak {
            all.extend(l.problems.iter().cloned());
        }
        all.extend(self.placement.iter().cloned());
        all.sort_by_key(|p| std::cmp::Reverse(p.severity));
        self.problems = all;
        self.recount();
    }

    pub fn set_placement(&mut self, problems: Vec<Problem>, serial: Option<u64>, ms: u64) {
        self.placement = problems;
        self.placement_serial = serial;
        self.placement_ms = ms;
        self.merge();
    }

    pub fn busy(&self) -> bool {
        self.checking || self.placement_checking
    }

    fn recount(&mut self) {
        let mut c = ProblemCounts::default();
        for p in &self.problems {
            if self.is_ignored(p) {
                c.ignored += 1;
                continue;
            }
            match p.severity {
                Severity::Error => c.errors += 1,
                Severity::Warning => c.warnings += 1,
                Severity::Info => c.infos += 1,
            }
        }
        self.counts = c;
        self.revision = self.revision.wrapping_add(1);
    }

    pub fn is_ignored(&self, p: &Problem) -> bool {
        self.ignored.codes.iter().any(|c| c == p.code) || self.ignored.keys.contains(&p.key())
    }

    fn store_ignored(state: &mut AppState) {
        let rt = &mut state.runtime.health;
        let over = rt.ignored.keys.len().saturating_sub(MAX_IGNORED);
        rt.ignored.keys.drain(..over);
        state.sidecar.set(&rt.ignored);
        rt.recount();
    }

    pub fn ignore(state: &mut AppState, p: &Problem) {
        let key = p.key();
        let rt = &mut state.runtime.health;
        if !rt.ignored.keys.contains(&key) {
            rt.ignored.keys.push(key);
        }
        Self::store_ignored(state);
    }

    pub fn ignore_code(state: &mut AppState, code: &str) {
        let rt = &mut state.runtime.health;
        if !rt.ignored.codes.iter().any(|c| c == code) {
            rt.ignored.codes.push(code.to_owned());
        }
        Self::store_ignored(state);
    }

    pub fn unignore(state: &mut AppState, p: &Problem) {
        let key = p.key();
        let rt = &mut state.runtime.health;
        rt.ignored.keys.retain(|k| *k != key);
        rt.ignored.codes.retain(|c| c != p.code);
        Self::store_ignored(state);
    }

    pub fn clear_ignored(state: &mut AppState) {
        state.runtime.health.ignored = IgnoredProblems::default();
        Self::store_ignored(state);
    }

    pub fn map_loaded(state: &mut AppState) {
        let ignored: IgnoredProblems = state.sidecar.get();
        let rt = &mut state.runtime.health;
        let show_window = rt.show_portal_window;
        let show_detail = rt.show_detail;
        *rt = HealthRuntime { ignored, show_portal_window: show_window, show_detail, ..HealthRuntime::default() };
        rt.recount();
    }

    pub fn show_detail_suggestions(state: &mut AppState) {
        let rt = &mut state.runtime.health;
        rt.show_detail = true;
        if !state.settings.health.live_leak_check && !rt.leak_requested {
            rt.leak_requested = true;
            rt.recheck = true;
        }
    }

    pub fn seal_input(state: &mut AppState) -> hp_editor::limits::leak::LeakInput {
        let mut cache: std::collections::HashMap<String, hp_editor::limits::checks::MaterialFacts> =
            std::collections::HashMap::new();
        for n in state.doc.scene().solids().values() {
            for side in &n.solid.sides {
                if cache.contains_key(&side.material) {
                    continue;
                }
                let facts = match state.assets.material_info(&side.material) {
                    Some(info) => knowledge::material_facts(&info),
                    None => hp_editor::limits::checks::MaterialFacts::guess(&side.material),
                };
                cache.insert(side.material.clone(), facts);
            }
        }
        state.doc.leak_input(&|m: &str| {
            cache.get(m).copied().unwrap_or_else(|| hp_editor::limits::checks::MaterialFacts::guess(m))
        })
    }

    pub fn current_leak_report(state: &AppState) -> Option<&hp_editor::limits::leak::LeakReport> {
        state.runtime.health.leak.as_ref().filter(|l| l.serial == state.doc.content_serial()).map(|l| &l.report)
    }

    pub fn detail_candidates(&self) -> &[ObjRef] {
        self.leak
            .as_ref()
            .and_then(|l| l.problems.iter().find(|p| p.code == "detail_candidate"))
            .map_or(&[], |p| p.objects.as_slice())
    }

    pub fn leak_path(&self) -> Option<&LeakPath> {
        self.leak.as_ref()?.report.leak.as_ref()
    }

    pub fn worst_limit(&self) -> Option<&LimitUse> {
        worst(&self.checks.as_ref()?.limits)
    }

    pub fn on_visible(doc: &hp_editor::Document, p: &Problem) -> bool {
        if p.objects.is_empty() {
            return true;
        }
        p.objects.iter().any(|o| doc.scene().contains(*o) && doc.is_visible(*o))
    }

    pub fn existing_objects(doc: &hp_editor::Document, p: &Problem) -> Vec<ObjRef> {
        p.objects.iter().copied().filter(|o| doc.scene().contains(*o)).collect()
    }

    pub fn set_portals(&mut self, path: PathBuf, file: PortalFile) {
        self.portals = Some(Arc::new(file));
        self.portal_path = Some(path);
        self.portal_version = self.portal_version.wrapping_add(1);
        self.portal_offer = None;
    }

    pub fn clear_portals(&mut self) {
        if self.portals.take().is_some() {
            self.portal_version = self.portal_version.wrapping_add(1);
        }
        self.portal_path = None;
    }

    pub fn set_vis(&mut self, path: PathBuf, vis: BspVis) {
        self.vis = Some(Arc::new(vis));
        self.vis_path = Some(path);
        self.costs = None;
        self.pvs_cost = None;
        self.pvs_cluster = None;
        self.pvs_visible = None;
        self.pvs_version = self.pvs_version.wrapping_add(1);
    }

    pub fn set_costs(&mut self, costs: RenderCosts) {
        self.costs = Some(Arc::new(costs));
        self.pvs_cost = None;
        self.pvs_cluster = None;
        self.pvs_visible = None;
        self.pvs_version = self.pvs_version.wrapping_add(1);
    }

    pub fn set_exact(&mut self, path: PathBuf, modified: u64, counts: &[(&str, u64)], areas: usize, clusters: u32) {
        let uses = hp_editor::limits::exact_limits(counts, self.profile);
        self.exact = Some(ExactLimits { path, modified, uses, areas, clusters });
    }

    pub fn update_play(state: &mut AppState) {
        let rt = &mut state.runtime.health;
        let (Some(from), Some(vis)) = (rt.play_from, rt.vis.clone()) else {
            if rt.play_visible.take().is_some() || rt.play_key.take().is_some() || rt.play_flow.take().is_some() {
                rt.play_cluster = None;
                rt.play_area = None;
                rt.play_cost = None;
                rt.play_version = rt.play_version.wrapping_add(1);
            }
            return;
        };
        let cluster = vis.cluster_at(from);
        let open_doors = &rt.play_open_doors;
        let states = &rt.play_portal_states;
        let portal_open = |number: u16| match vis.portal_entities().iter().find(|p| p.number == number) {
            Some(p) if !p.name.is_empty() && states.iter().any(|(n, _)| n.eq_ignore_ascii_case(&p.name)) => {
                states.iter().find(|(n, _)| n.eq_ignore_ascii_case(&p.name)).is_some_and(|(_, o)| *o)
            }
            Some(p) if p.window => {
                vis.portal_distance(number, from).is_none_or(|d| d <= f64::from(p.fade_dist) + WINDOW_FADE_BUFFER)
            }
            Some(p) if !p.door.is_empty() => open_doors.iter().any(|d| d.eq_ignore_ascii_case(&p.door)),
            Some(p) => p.start_open,
            None => true,
        };
        let cone = rt.play_view.unwrap_or(hp_editor::limits::portals::ViewCone {
            eye: from,
            forward: DVec3::X,
            right: DVec3::NEG_Y,
            up: DVec3::Z,
            tan_x: 1.0e3,
            tan_y: 1.0e3,
        });
        let flow = vis.flow_areas(&cone, portal_open);
        let areas = flow.visible.clone();
        rt.play_area = flow.own_area.or_else(|| vis.area_at(from));
        rt.play_flow = Some(Arc::new(flow));
        let mut portal_states = rt.play_portal_states.clone();
        portal_states.sort();
        let key = cluster.map(|c| (c, rt.play_open_doors.clone(), portal_states, areas.clone()));
        if key == rt.play_key && (key.is_none() || rt.play_visible.is_some()) {
            return;
        }
        rt.play_key = key;
        rt.play_cluster = cluster;
        let cluster_areas = vis.cluster_areas();
        rt.play_visible = cluster.and_then(|c| vis.visible_from(c)).map(|mut seen| {
            for (c, v) in seen.iter_mut().enumerate() {
                let area = cluster_areas.get(c).copied().unwrap_or(0);
                *v &= areas.get(usize::from(area)).copied().unwrap_or(true);
            }
            Arc::new(seen)
        });
        rt.play_cost = rt.costs.as_ref().zip(rt.play_visible.as_ref()).map(|(c, v)| c.cost(v));
        rt.play_version = rt.play_version.wrapping_add(1);
    }

    pub fn update_pvs(state: &mut AppState) {
        let rt = &mut state.runtime.health;
        if !rt.pvs {
            if rt.pvs_visible.take().is_some() {
                rt.pvs_cluster = None;
                rt.pvs_cost = None;
                rt.pvs_version = rt.pvs_version.wrapping_add(1);
            }
            return;
        }
        let Some(vis) = rt.vis.clone() else { return };
        let from = rt.pvs_point.unwrap_or(state.cameras.cam3d.pos);
        let cluster = vis.cluster_at(from);
        if cluster == rt.pvs_cluster && (cluster.is_none() || rt.pvs_visible.is_some()) {
            return;
        }
        rt.pvs_cluster = cluster;
        rt.pvs_visible = cluster.and_then(|c| vis.visible_from(c)).map(Arc::new);
        rt.pvs_cost = rt.costs.as_ref().zip(rt.pvs_visible.as_ref()).map(|(c, v)| c.cost(v));
        rt.pvs_version = rt.pvs_version.wrapping_add(1);
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn box_sides(min: glam::DVec3, max: glam::DVec3) -> Vec<hp_vmf::Side> {
        use glam::DVec3;
        let (x0, y0, z0, x1, y1, z1) = (min.x, min.y, min.z, max.x, max.y, max.z);
        let v = DVec3::new;
        let faces = [
            ([v(x0, y1, z1), v(x1, y1, z1), v(x1, y0, z1)], (DVec3::X, DVec3::NEG_Y)),
            ([v(x0, y0, z0), v(x1, y0, z0), v(x1, y1, z0)], (DVec3::X, DVec3::NEG_Y)),
            ([v(x1, y1, z1), v(x0, y1, z1), v(x0, y1, z0)], (DVec3::X, DVec3::NEG_Z)),
            ([v(x1, y0, z0), v(x0, y0, z0), v(x0, y0, z1)], (DVec3::X, DVec3::NEG_Z)),
            ([v(x1, y1, z0), v(x1, y0, z0), v(x1, y0, z1)], (DVec3::Y, DVec3::NEG_Z)),
            ([v(x0, y1, z1), v(x0, y0, z1), v(x0, y0, z0)], (DVec3::Y, DVec3::NEG_Z)),
        ];
        faces
            .iter()
            .map(|(plane, (u, w))| hp_vmf::Side {
                plane: *plane,
                material: "BRICK/WALL".to_owned(),
                uaxis: hp_vmf::TexAxis { axis: *u, shift: 0.0, scale: 0.25 },
                vaxis: hp_vmf::TexAxis { axis: *w, shift: 0.0, scale: 0.25 },
                ..Default::default()
            })
            .collect()
    }

    fn wait_for_results(state: &mut AppState) {
        let start = std::time::Instant::now();
        loop {
            HealthRuntime::tick(state);
            let rt = &state.runtime.health;
            if rt.checks.is_some() && rt.leak.is_some() && !rt.checking {
                break;
            }
            assert!(start.elapsed() < std::time::Duration::from_secs(30), "no results");
            std::thread::sleep(std::time::Duration::from_millis(20));
        }
    }

    #[test]
    fn live_checks_find_and_follow_problems() {
        let mut state = crate::testing::test_state();
        wait_for_results(&mut state);
        let before = state.runtime.health.revision;
        let n = state.runtime.health.problems.len();
        state.doc.delete(&[ObjRef::Entity(4)]).expect("delete");
        HealthRuntime::tick(&mut state);
        assert!(state.runtime.health.checking, "waiting for the map to stay still");
        let start = std::time::Instant::now();
        while !state.runtime.health.problems.iter().any(|p| p.code == "no_player_spawn") {
            HealthRuntime::tick(&mut state);
            assert!(start.elapsed() < std::time::Duration::from_secs(30));
            std::thread::sleep(std::time::Duration::from_millis(20));
        }
        assert!(state.runtime.health.revision != before);
        assert!(state.runtime.health.problems.len() > n);
        assert!(state.runtime.health.counts.warnings >= 1);
    }

    #[test]
    fn ignored_problems_are_counted_apart_and_saved() {
        let mut state = crate::testing::test_state();
        state.doc.delete(&[ObjRef::Entity(4)]).expect("delete");
        HealthRuntime::check_now(&mut state);
        let p = state.runtime.health.problems.iter().find(|p| p.code == "no_player_spawn").cloned().expect("spawn");
        let warnings = state.runtime.health.counts.warnings;
        HealthRuntime::ignore(&mut state, &p);
        assert!(state.runtime.health.is_ignored(&p));
        assert_eq!(state.runtime.health.counts.warnings, warnings - 1);
        assert_eq!(state.runtime.health.counts.ignored, 1);
        let saved: IgnoredProblems = state.sidecar.get();
        assert_eq!(saved.keys, vec![p.key()]);
        HealthRuntime::ignore_code(&mut state, "no_player_spawn");
        assert_eq!(state.sidecar.get::<IgnoredProblems>().codes, vec!["no_player_spawn".to_owned()]);
        HealthRuntime::clear_ignored(&mut state);
        assert_eq!(state.runtime.health.counts.warnings, warnings);
        state.sidecar.set(&IgnoredProblems { keys: vec!["x".into()], codes: Vec::new() });
        HealthRuntime::map_loaded(&mut state);
        assert_eq!(state.runtime.health.ignored.keys, vec!["x".to_owned()]);
        assert!(state.runtime.health.problems.is_empty());
    }

    #[test]
    fn profile_follows_the_setting() {
        let mut state = crate::testing::test_state();
        assert_eq!(HealthRuntime::profile_for(&state), LimitProfile::Gmod64);
        state.settings.game.map_game = Some("tf".to_owned());
        assert_eq!(HealthRuntime::profile_for(&state), LimitProfile::Stock, "another game: Valve's limits");
        state.settings.game.map_game = None;
        state.settings.health.set_profile(Some(LimitProfile::Stock));
        assert_eq!(HealthRuntime::profile_for(&state), LimitProfile::Stock);
        state.settings.health.set_profile(None);
        assert_eq!(HealthRuntime::profile_for(&state), LimitProfile::Gmod64);
    }

    #[test]
    fn sealed_rooms_get_verified_detail_candidates() {
        use glam::DVec3;
        use hp_editor::Owner;
        let mut state = crate::testing::test_state();
        let mut doc = hp_editor::Document::new_empty();
        let (s, t) = (256.0, 16.0);
        let wall = |doc: &mut hp_editor::Document, a: DVec3, b: DVec3| {
            doc.create_solid(box_sides(a, b), Owner::World).expect("wall")
        };
        wall(&mut doc, DVec3::new(-s - t, -s - t, -s - t), DVec3::new(-s, s + t, s + t));
        wall(&mut doc, DVec3::new(s, -s - t, -s - t), DVec3::new(s + t, s + t, s + t));
        wall(&mut doc, DVec3::new(-s, -s - t, -s - t), DVec3::new(s, -s, s + t));
        wall(&mut doc, DVec3::new(-s, s, -s - t), DVec3::new(s, s + t, s + t));
        wall(&mut doc, DVec3::new(-s, -s, -s - t), DVec3::new(s, s, -s));
        wall(&mut doc, DVec3::new(-s, -s, s), DVec3::new(s, s, s + t));
        let pillar = wall(&mut doc, DVec3::new(-32.0, -32.0, -256.0), DVec3::new(32.0, 32.0, -200.0));
        doc.create_point_entity("info_player_start", DVec3::new(100.0, 100.0, -240.0), &[]).expect("spawn");
        state.doc = doc;
        HealthRuntime::check_now(&mut state);
        let leak = state.runtime.health.leak.clone().expect("leak result");
        assert!(!leak.report.leaks(), "{:?}", leak.report);
        assert!(!leak.report.hull.contains(&pillar), "the pillar does not touch the void");
        let d = state.runtime.health.problems.iter().find(|p| p.code == "detail_candidate").expect("candidate");
        assert_eq!(d.objects, vec![ObjRef::Solid(pillar)]);
    }

    #[test]
    fn the_leak_check_runs_with_the_checks() {
        let mut state = crate::testing::test_state();
        HealthRuntime::check_now(&mut state);
        let leak = state.runtime.health.leak.clone().expect("leak result");
        assert!(leak.report.leaks() || leak.report.no_world, "{:?}", leak.report);
        state.settings.health.live_leak_check = false;
        HealthRuntime::check_now(&mut state);
        assert!(state.runtime.health.leak.is_none());
    }

    #[test]
    fn outputs_aimed_at_instance_contents_are_not_missing_targets() {
        use glam::DVec3;
        use hp_vmf::Connection;
        let dir = std::env::temp_dir().join(format!("hp_health_instances_{}", std::process::id()));
        let _ = std::fs::remove_dir_all(&dir);
        std::fs::create_dir_all(&dir).expect("dir");
        let text = "world
{
	\"id\" \"1\"
	\"classname\" \"worldspawn\"
}
entity
{
	\"id\" \"2\"
	\"classname\" \"func_button\"
	\"targetname\" \"button\"
	\"origin\" \"0 0 0\"
}
";
        std::fs::write(dir.join("panel.vmf"), text).expect("write");
        let mut state = crate::testing::test_state();
        state.doc.set_path(Some(dir.join("main.vmf")));
        let at = DVec3::new(0.0, 0.0, 64.0);
        let kv = [("file".to_owned(), "panel.vmf".to_owned()), ("targetname".to_owned(), "lift".to_owned())];
        state.doc.create_point_entity("func_instance", at, &kv).expect("instance");
        let relay = state.doc.create_point_entity("logic_relay", at, &[]).expect("relay");
        for target in ["lift-button", "ghost"] {
            let c = Connection::parse("OnTrigger", &format!("{target},Press,,0,-1"));
            state.doc.add_connection(&[relay], &c).expect("output");
        }
        let start = std::time::Instant::now();
        let args = loop {
            HealthRuntime::check_now(&mut state);
            let p = state.runtime.health.problems.iter().find(|p| p.code == "missing_output_target").cloned();
            let args = p.map(|p| p.args).unwrap_or_default();
            if args == vec!["ghost".to_owned()] {
                break args;
            }
            assert!(start.elapsed() < std::time::Duration::from_secs(20), "instance names never known: {args:?}");
            std::thread::sleep(std::time::Duration::from_millis(20));
        };
        assert_eq!(args, vec!["ghost".to_owned()]);
        let _ = std::fs::remove_dir_all(&dir);
    }
}
