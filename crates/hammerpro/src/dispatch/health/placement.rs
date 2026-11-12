use std::sync::Arc;
use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::mpsc::{Receiver, TryRecvError};
use std::time::{Duration, Instant};

use hp_app::AppState;
use hp_app::state::AssetStatus;
use hp_editor::Problem;

const DEBOUNCE: Duration = Duration::from_millis(700);
const SLICE: Duration = Duration::from_millis(4);

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
struct Want {
    serial: u64,
    content: u64,
    overlaps: bool,
}

struct Job {
    want: Want,
    serial: u64,
    started: Instant,
    cancel: Arc<AtomicBool>,
    rx: Receiver<Option<Vec<Problem>>>,
}

#[derive(Default)]
pub(crate) struct PlacementChecks {
    seen: Option<Want>,
    changed_at: Option<Instant>,
    done: Option<Want>,
    job: Option<Job>,
    forced: bool,
    builder: Option<(crate::dispatch::mcp::MapScanBuilder, Want)>,
    pub(crate) last_prepare_ms: u64,
}

impl Drop for PlacementChecks {
    fn drop(&mut self) {
        if let Some(j) = &self.job {
            j.cancel.store(true, Ordering::Relaxed);
        }
    }
}

fn content_generation(state: &AppState) -> u64 {
    state.assets.fs().map_or(0, |f| f.generation())
}

impl PlacementChecks {
    pub(crate) fn map_loaded(&mut self) {
        if let Some(j) = self.job.take() {
            j.cancel.store(true, Ordering::Relaxed);
        }
        *self = PlacementChecks::default();
    }

    pub(crate) fn force(&mut self) {
        self.forced = true;
    }

    pub(crate) fn tick(&mut self, state: &mut AppState) -> bool {
        if let Some(job) = &self.job {
            match job.rx.try_recv() {
                Ok(result) => {
                    let job = self.job.take();
                    if let (Some(problems), Some(job)) = (result, job) {
                        let ms = job.started.elapsed().as_millis() as u64;
                        log::info!(
                            "placement check: {} problems, {ms} ms on a worker after {} ms on the main thread in slices",
                            problems.len(),
                            self.last_prepare_ms
                        );
                        if state.doc.content_serial() == job.serial {
                            state.runtime.health.set_placement(problems, Some(job.serial), ms);
                            self.done = Some(job.want);
                        }
                    }
                }
                Err(TryRecvError::Empty) => {
                    if state.doc.content_serial() == job.serial {
                        state.runtime.health.placement_checking = true;
                        return true;
                    }
                    job.cancel.store(true, Ordering::Relaxed);
                    self.job = None;
                }
                Err(TryRecvError::Disconnected) => self.job = None,
            }
        }
        state.runtime.health.placement_checking = false;
        if !state.settings.placement.problem_checks {
            if !state.runtime.health.placement.is_empty() || state.runtime.health.placement_serial.is_some() {
                state.runtime.health.set_placement(Vec::new(), None, 0);
            }
            self.done = None;
            self.forced = false;
            self.builder = None;
            return false;
        }
        let want = Want {
            serial: state.doc.content_serial(),
            content: content_generation(state),
            overlaps: state.settings.placement.overlap_check,
        };
        if self.builder.as_ref().is_some_and(|(b, w)| b.serial() != want.serial || *w != want) {
            self.builder = None;
        }
        if self.builder.is_none() {
            let force = std::mem::take(&mut self.forced);
            if self.done == Some(want) && !force {
                return false;
            }
            if !state.settings.health.live_checks && !force {
                return false;
            }
            if self.seen != Some(want) {
                self.seen = Some(want);
                self.changed_at = Some(Instant::now());
            }
            let waiting = self.changed_at.is_some_and(|t| t.elapsed() < DEBOUNCE);
            if (waiting && !force)
                || state.doc.in_transaction()
                || matches!(state.assets.status(), AssetStatus::Mounting)
            {
                if force {
                    self.forced = true;
                }
                return true;
            }
            self.builder = Some((crate::dispatch::mcp::MapScanBuilder::new(state), want));
        }
        let Some((builder, _)) = self.builder.as_mut() else { return false };
        let Some(scan) = builder.step(state, SLICE) else {
            state.runtime.health.placement_checking = true;
            return true;
        };
        self.last_prepare_ms = builder.spent.as_millis() as u64;
        self.builder = None;
        let started = Instant::now();
        if scan.candidates() == 0 {
            state.runtime.health.set_placement(Vec::new(), Some(scan.serial), self.last_prepare_ms);
            self.done = Some(want);
            return false;
        }
        let fs = state.assets.fs().cloned();
        let cancel = Arc::new(AtomicBool::new(false));
        let flag = Arc::clone(&cancel);
        let (tx, rx) = std::sync::mpsc::channel();
        let serial = scan.serial;
        let spawned = std::thread::Builder::new().name("hp-placement-check".into()).spawn(move || {
            let r = std::panic::catch_unwind(std::panic::AssertUnwindSafe(|| {
                crate::dispatch::mcp::run_map_scan(scan, fs.as_ref(), &flag)
            }));
            let _ = tx.send(r.unwrap_or_else(|_| {
                log::error!("the placement check failed (panic)");
                None
            }));
        });
        match spawned {
            Ok(_) => {
                self.job = Some(Job { want, serial, started, cancel, rx });
                state.runtime.health.placement_checking = true;
                true
            }
            Err(e) => {
                log::error!("cannot start the placement check: {e}");
                self.done = Some(want);
                false
            }
        }
    }
}

#[cfg(test)]
mod tests {
    use glam::DVec3;
    use hp_shape::{ModelShape, TriMesh};

    use super::*;

    #[test]
    fn the_background_check_reports_floating_props_and_follows_edits() {
        let m = "models/hp_test/bg_crate.mdl";
        let shape = TriMesh::convex_box(DVec3::new(-8.0, -8.0, 0.0), DVec3::new(8.0, 8.0, 16.0), 0);
        crate::dispatch::mcp::pin_shape(
            m,
            0,
            ModelShape { path: m.to_owned(), render: shape, ..ModelShape::default() },
        );
        let mut state = hp_app::testing::test_state();
        let planes = [
            "(-256 256 0) (256 256 0) (256 -256 0)",
            "(-256 -256 -16) (256 -256 -16) (256 256 -16)",
            "(-256 256 0) (-256 -256 0) (-256 -256 -16)",
            "(256 256 -16) (256 -256 -16) (256 -256 0)",
            "(256 256 0) (-256 256 0) (-256 256 -16)",
            "(256 -256 -16) (-256 -256 -16) (-256 -256 0)",
        ];
        let sides: Vec<String> = planes
            .iter()
            .enumerate()
            .map(|(i, p)| {
                format!(
                    "side {{ \"id\" \"{}\" \"plane\" \"{p}\" \"material\" \"DEV/DEV_MEASUREGENERIC01B\" \"uaxis\" \"[1 0 0 0] 0.25\" \"vaxis\" \"[0 -1 0 0] 0.25\" }}",
                    i + 10
                )
            })
            .collect();
        let text = format!(
            "world {{ \"id\" \"1\" \"classname\" \"worldspawn\" solid {{ \"id\" \"2\" {} }} }}",
            sides.join(" ")
        );
        state.doc = hp_editor::Document::from_map(hp_vmf::parse(&text).expect("vmf"));
        let kv = [("model".to_owned(), m.to_owned())];
        let id = state.doc.create_point_entity("prop_physics", DVec3::new(0.0, 0.0, 40.0), &kv).expect("prop");
        let mut checks = PlacementChecks::default();
        checks.force();
        let start = Instant::now();
        while state.runtime.health.placement.is_empty() {
            checks.tick(&mut state);
            assert!(start.elapsed() < Duration::from_secs(20), "no placement result");
            std::thread::sleep(Duration::from_millis(10));
        }
        assert_eq!(state.runtime.health.placement[0].code, "floating_object");
        assert!(state.runtime.health.problems.iter().any(|p| p.code == "floating_object"));
        state.doc.translate(&[hp_editor::ObjRef::Entity(id)], DVec3::new(0.0, 0.0, -40.0)).expect("drop");
        let start = Instant::now();
        while !state.runtime.health.placement.is_empty() {
            checks.tick(&mut state);
            assert!(start.elapsed() < Duration::from_secs(20), "the result did not follow the edit");
            std::thread::sleep(Duration::from_millis(10));
        }
        state.settings.placement.problem_checks = false;
        checks.tick(&mut state);
        assert!(state.runtime.health.placement_serial.is_none());
    }
}
