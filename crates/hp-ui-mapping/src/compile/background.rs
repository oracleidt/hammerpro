use std::path::PathBuf;
use std::sync::mpsc::{self, Receiver, TryRecvError};
use std::time::{Duration, Instant};

use hp_app::runtime::{CompileBadge, CompileProgress};
use hp_app::settings::Settings;
use hp_app::state::AssetStatus;
use hp_app::{AppState, t};
use hp_compile::{CompileEvent, CompileHandle};

use super::snapshot::{self, PrepError, SnapshotInput, SnapshotOutput};
use super::{CompileDialog, game_root, history_file};

pub(crate) const IDLE: Duration = Duration::from_secs(3);
const CHEAP_OBJECTS: usize = 12_000;
const CHEAP_SNAPSHOT_MS: u64 = 6;
const POWER_CHECK: Duration = Duration::from_secs(30);

pub(crate) fn snapshot_is_cheap(doc: &hp_editor::Document, last_ms: Option<u64>) -> bool {
    match last_ms {
        Some(ms) => ms <= CHEAP_SNAPSHOT_MS,
        None => doc.scene().solids().len() + doc.scene().entities().len() <= CHEAP_OBJECTS,
    }
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(crate) struct ContentKey {
    doc: u64,
    serial: u64,
}

impl ContentKey {
    pub(crate) fn of(state: &AppState) -> Self {
        ContentKey { doc: state.sidecar.serial(), serial: state.doc.content_serial() }
    }
}

pub(crate) struct Background {
    last_snapshot_ms: Option<u64>,
    seen: Option<ContentKey>,
    changed_at: Instant,
    done_for: Option<ContentKey>,
    prep: Option<Receiver<SnapshotOutput>>,
    job: Option<CompileHandle>,
    started: Instant,
    fraction: f32,
    leak: Option<PathBuf>,
    power_checked: Option<Instant>,
    on_battery: bool,
}

impl Default for Background {
    fn default() -> Self {
        Self {
            last_snapshot_ms: None,
            seen: None,
            changed_at: Instant::now(),
            done_for: None,
            prep: None,
            job: None,
            started: Instant::now(),
            fraction: 0.0,
            leak: None,
            power_checked: None,
            on_battery: false,
        }
    }
}

impl Background {
    pub(crate) fn note_snapshot_cost(&mut self, ms: u64) {
        self.last_snapshot_ms = Some(ms);
    }

    pub(crate) fn cancel(&mut self) {
        if let Some(job) = self.job.take() {
            job.cancel();
        }
        self.prep = None;
    }

    pub(crate) fn is_running(&self) -> bool {
        self.job.is_some() || self.prep.is_some()
    }

    pub(crate) fn progress(&self) -> Option<CompileProgress> {
        self.is_running().then(|| CompileProgress {
            step: t!("compile.background_running").to_owned(),
            fraction: self.fraction,
            elapsed_s: self.started.elapsed().as_secs_f32(),
        })
    }

    fn enabled(&mut self, state: &AppState) -> bool {
        let s = &state.settings.compile;
        if !s.background_precompile
            || !s.low_priority
            || !s.uses_builtin_compiler()
            || state.assets.status() != AssetStatus::Ready
        {
            return false;
        }
        if self.power_checked.is_none_or(|t| t.elapsed() > POWER_CHECK) {
            self.power_checked = Some(Instant::now());
            self.on_battery = hp_compile::power::on_battery();
        }
        !self.on_battery && state.assets.fs().is_some()
    }

    pub(crate) fn tick(&mut self, ctx: &egui::Context, state: &mut AppState, busy: bool) {
        let key = ContentKey::of(state);
        if self.seen != Some(key) {
            self.seen = Some(key);
            self.changed_at = Instant::now();
            self.cancel();
        }
        self.poll(ctx, state, key);
        if busy || self.is_running() || self.done_for == Some(key) || !self.enabled(state) {
            return;
        }
        let idle = self.changed_at.elapsed();
        if idle < IDLE {
            ctx.request_repaint_after(IDLE - idle);
            return;
        }
        self.done_for = Some(key);
        if state.doc.scene().solids().is_empty() || !snapshot_is_cheap(&state.doc, self.last_snapshot_ms) {
            return;
        }
        let s = state.settings.compile.clone();
        let t0 = Instant::now();
        let snap = state.doc.compile_snapshot(&CompileDialog::export_of(&s, &state.settings.visibility));
        let snapshot_ms = u64::try_from(t0.elapsed().as_millis()).unwrap_or(u64::MAX);
        self.last_snapshot_ms = Some(snapshot_ms);
        let Ok(mut target) = snapshot::target_for(
            state.doc.path(),
            snap.is_cordoned(),
            &Settings::cache_dir().join("untitled"),
            state.settings.compile.bsp_beside_vmf,
        ) else {
            return;
        };
        let map_key = hp_app::sidecar::map_key(target.source.as_deref());
        let folder: String =
            map_key.chars().map(|c| if c.is_ascii_alphanumeric() || c == '-' { c } else { '_' }).collect();
        target.build_dir = Settings::cache_dir().join("spec").join(folder);
        let input = SnapshotInput {
            history_file: s.history.then(|| history_file(&map_key)),
            snapshot: snap,
            target,
            settings: s,
            gmod_root: game_root(state),
            fs: state.assets.fs().cloned(),
            mounting: false,
            previous: None,
            cache_dir: Settings::cache_dir().join("pack"),
            content_dir: Settings::cache_dir().join("compile_content"),
            builds_dir: Some(Settings::cache_dir().join("builds")),
            release: false,
            background: true,
            dry_run: false,
            snapshot_ms,
            launch_flags: Vec::new(),
        };
        let (tx, rx) = mpsc::channel();
        let repaint = ctx.clone();
        let spawned = std::thread::Builder::new().name("hp-compile-background".into()).spawn(move || {
            let _ = tx.send(snapshot::prepare_snapshot(input));
            repaint.request_repaint();
        });
        if spawned.is_ok() {
            self.prep = Some(rx);
            self.started = Instant::now();
            self.fraction = 0.0;
            self.leak = None;
        }
    }

    fn poll(&mut self, ctx: &egui::Context, state: &mut AppState, key: ContentKey) {
        if let Some(rx) = &self.prep {
            match rx.try_recv() {
                Ok(out) => {
                    self.prep = None;
                    match out.result {
                        Ok(p) if self.seen == Some(key) => {
                            let repaint = ctx.clone();
                            self.job = Some(hp_compile::start_with_waker(p.plan, &p.workdir, move || {
                                repaint.request_repaint();
                            }));
                        }
                        Ok(_) | Err(PrepError::NeedsMount(_) | PrepError::Failed(_)) => {}
                    }
                }
                Err(TryRecvError::Disconnected) => self.prep = None,
                Err(TryRecvError::Empty) => {}
            }
        }
        let Some(job) = &self.job else { return };
        let mut finished = None;
        while let Ok(ev) = job.events.try_recv() {
            match ev {
                CompileEvent::Progress { fraction, .. } => self.fraction = fraction.clamp(0.0, 1.0),
                CompileEvent::LeakDetected { pointfile } => self.leak = Some(pointfile),
                CompileEvent::Finished { success, leaked, cancelled, .. } => {
                    finished = Some((success, leaked, cancelled))
                }
                _ => {}
            }
        }
        let Some((success, leaked, cancelled)) = finished else { return };
        self.job = None;
        if cancelled {
            return;
        }
        if leaked {
            state.runtime.compile.badge = Some(CompileBadge::Leak);
            if let Some(lin) = self.leak.take()
                && state.settings.compile.load_pointfile_on_leak
                && state.pointfile.as_ref().is_none_or(|p| p.path != lin)
            {
                super::load_pointfile(state, &lin);
            }
        } else if success && state.runtime.compile.badge == Some(CompileBadge::Leak) {
            state.runtime.compile.badge = None;
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn cheap_snapshots() {
        let doc = hp_editor::Document::new_empty();
        assert!(snapshot_is_cheap(&doc, None));
        assert!(snapshot_is_cheap(&doc, Some(2)));
        assert!(!snapshot_is_cheap(&doc, Some(40)));
    }

    #[test]
    fn edits_reset_the_idle_timer_and_disabled_does_nothing() {
        let mut state = hp_app::testing::test_state();
        let ctx = egui::Context::default();
        let mut bg = Background::default();
        state.settings.compile.background_precompile = false;
        bg.tick(&ctx, &mut state, false);
        assert!(!bg.is_running());
        let first = bg.changed_at;
        std::thread::sleep(Duration::from_millis(5));
        state.doc.set_entity_key(&[5], "targetname", "x").ok();
        bg.tick(&ctx, &mut state, false);
        assert!(bg.changed_at > first, "an edit restarts the quiet time");
        let t = bg.changed_at;
        state.doc.set_selection(&[hp_editor::ObjRef::Solid(2)]);
        bg.tick(&ctx, &mut state, false);
        assert_eq!(bg.changed_at, t);
        assert!(bg.progress().is_none());
    }
}
