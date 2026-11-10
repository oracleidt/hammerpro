use std::path::{Path, PathBuf};
use std::sync::Arc;
use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::mpsc::{self, Receiver, TryRecvError};
use std::time::{Duration, Instant};

use hp_app::runtime::CompileProgress;
use hp_app::settings::{CompilePresetSetting, CompileSettings, Settings};
use hp_app::{AppState, Severity, t, tf};
use hp_compile::CompileEvent;

use super::plan::{self, PrepInput};
use super::snapshot::{self, used_assets};

const COPY_WAIT: Duration = Duration::from_secs(300);
const COPY_RETRY: Duration = Duration::from_secs(2);

enum Event {
    Progress(f32),
    Waiting,
    Done(Result<String, String>),
}

pub(crate) struct Job {
    pub snapshot_vmf: PathBuf,
    pub map_name: String,
    pub dir: PathBuf,
    pub settings: CompileSettings,
    pub root: Option<PathBuf>,
    pub fs: Option<hp_fs::GameFs>,
    pub output_bsp: Option<PathBuf>,
}

#[derive(Default)]
pub(crate) struct Progressive {
    job: Option<(Receiver<Event>, Arc<AtomicBool>, Instant)>,
    fraction: f32,
}

impl Progressive {
    pub(crate) fn cancel(&mut self) {
        if let Some((_, cancel, _)) = self.job.take() {
            cancel.store(true, Ordering::Relaxed);
        }
    }

    pub(crate) fn progress(&self) -> Option<CompileProgress> {
        self.job.as_ref().map(|(_, _, t0)| CompileProgress {
            step: t!("compile.progressive_running").to_owned(),
            fraction: self.fraction,
            elapsed_s: t0.elapsed().as_secs_f32(),
        })
    }

    pub(crate) fn start(&mut self, ctx: &egui::Context, job: Job) {
        self.cancel();
        let Job { snapshot_vmf, map_name, dir, settings, root, fs, output_bsp } = job;
        let mut s = settings;
        let quality = match s.progressive_preset {
            CompilePresetSetting::Final => CompilePresetSetting::Final,
            _ => CompilePresetSetting::Normal,
        };
        plan::apply_preset(&mut s, quality);
        s.low_priority = true;
        s.copy_to_maps = true;
        s.launch_game = false;
        s.pack_custom_content = false;
        let cancel = Arc::new(AtomicBool::new(false));
        let c2 = Arc::clone(&cancel);
        let (tx, rx) = mpsc::channel();
        let repaint = ctx.clone();
        let spawned = std::thread::Builder::new().name("hp-compile-progressive".into()).spawn(move || {
            let send = |e: Event| {
                let _ = tx.send(e);
                repaint.request_repaint();
            };
            let result = run(&snapshot_vmf, &dir, &map_name, (s, root, fs, output_bsp), &c2, &send);
            send(Event::Done(result));
        });
        if spawned.is_ok() {
            self.job = Some((rx, cancel, Instant::now()));
            self.fraction = 0.0;
        }
    }

    pub(crate) fn poll(&mut self, state: &mut AppState) {
        let Some((rx, _, _)) = &self.job else { return };
        loop {
            match rx.try_recv() {
                Ok(Event::Progress(f)) => self.fraction = f,
                Ok(Event::Waiting) => state.notify(Severity::Info, t!("compile.progressive_waiting")),
                Ok(Event::Done(result)) => {
                    self.job = None;
                    match result {
                        Ok(map) => state.notify(Severity::Success, tf!("compile.progressive_done", map)),
                        Err(e) if !e.is_empty() => {
                            state.notify(Severity::Warning, tf!("compile.progressive_failed", e))
                        }
                        Err(_) => {}
                    }
                    return;
                }
                Err(TryRecvError::Empty) => return,
                Err(TryRecvError::Disconnected) => {
                    self.job = None;
                    return;
                }
            }
        }
    }
}

pub(crate) fn work_dir(map_key: &str) -> PathBuf {
    let folder: String = map_key.chars().map(|c| if c.is_ascii_alphanumeric() || c == '-' { c } else { '_' }).collect();
    Settings::cache_dir().join("progressive").join(folder)
}

fn run(
    snapshot_vmf: &Path,
    dir: &Path,
    map_name: &str,
    (settings, root, fs, output_bsp): (CompileSettings, Option<PathBuf>, Option<hp_fs::GameFs>, Option<PathBuf>),
    cancel: &AtomicBool,
    send: &dyn Fn(Event),
) -> Result<String, String> {
    let map = hp_vmf::load(snapshot_vmf).map_err(|e| e.to_string())?;
    let (materials, models) = used_assets(&map);
    std::fs::create_dir_all(dir).map_err(|e| format!("{}: {e}", dir.display()))?;
    let vmf = dir.join(format!("{map_name}.vmf"));
    std::fs::copy(snapshot_vmf, &vmf).map_err(|e| format!("{}: {e}", vmf.display()))?;
    let out = plan::prepare(PrepInput {
        vmf,
        settings,
        gmod_root: root,
        fs,
        materials,
        models,
        roots: snapshot::pack_roots(&map),
        cache_dir: Settings::cache_dir().join("pack"),
        content_dir: Settings::cache_dir().join("compile_content"),
    });
    let (mut plan, workdir) = out.result?;
    let bsp = plan.bsp_path();
    let target = plan.steps.iter().find_map(|s| match &s.action {
        hp_compile::StepAction::Copy { to, .. } => Some(to.clone()),
        _ => None,
    });
    plan.steps.retain(|s| s.kind != hp_compile::StepKind::CopyToMaps);
    let steps = plan.steps.len().max(1) as f32;
    let handle = hp_compile::start(plan, &workdir);
    let mut current = 0usize;
    let ok = loop {
        if cancel.load(Ordering::Relaxed) {
            handle.cancel();
        }
        match handle.events.recv_timeout(Duration::from_millis(100)) {
            Ok(CompileEvent::StepStarted { index, .. }) => current = index,
            Ok(CompileEvent::Progress { fraction, .. }) => send(Event::Progress((current as f32 + fraction) / steps)),
            Ok(CompileEvent::Finished { success, cancelled, .. }) => break success && !cancelled,
            Ok(_) => {}
            Err(e) if e.is_timeout() => {}
            Err(_) => break false,
        }
    };
    handle.wait();
    if cancel.load(Ordering::Relaxed) {
        return Err(String::new());
    }
    if !ok {
        return Err(t!("compile.progressive_compile_failed").to_owned());
    }
    let Some(target) = target else { return Err(t!("compile.err_no_game").to_owned()) };
    if hp_compile::sandbox::is_sandboxed() {
        return Err(hp_compile::sandbox::REFUSED.to_owned());
    }
    if let Some(output) = output_bsp.filter(|output| *output != target) {
        copy_atomic(&bsp, &output).map_err(|e| format!("{}: {e}", output.display()))?;
    }
    let t0 = Instant::now();
    let mut told = false;
    loop {
        match copy_atomic(&bsp, &target) {
            Ok(()) => return Ok(map_name.to_owned()),
            Err(e) => {
                if t0.elapsed() > COPY_WAIT {
                    return Err(format!("{}: {e}", target.display()));
                }
                if !told {
                    told = true;
                    send(Event::Waiting);
                }
            }
        }
        let wait = Instant::now();
        while wait.elapsed() < COPY_RETRY {
            if cancel.load(Ordering::Relaxed) {
                return Err(String::new());
            }
            std::thread::sleep(Duration::from_millis(100));
        }
    }
}

fn copy_atomic(from: &Path, to: &Path) -> std::io::Result<()> {
    if let Some(parent) = to.parent() {
        std::fs::create_dir_all(parent)?;
    }
    let name = to.file_name().map_or_else(|| "map".into(), |n| n.to_string_lossy().into_owned());
    let tmp = to.with_file_name(format!("{name}.hptmp"));
    std::fs::copy(from, &tmp)?;
    std::fs::rename(&tmp, to).inspect_err(|_| {
        let _ = std::fs::remove_file(&tmp);
    })
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn fails_cleanly_without_a_game_and_reports() {
        crate::register_translations();
        let dir = crate::test_scratch("progressive");
        let vmf = dir.join("gm_p.vmf");
        std::fs::write(&vmf, "world { \"id\" \"1\" \"classname\" \"worldspawn\" }").expect("vmf");
        let mut p = Progressive::default();
        let ctx = egui::Context::default();
        let job = Job {
            snapshot_vmf: vmf,
            map_name: "gm_p".into(),
            dir: dir.join("work"),
            settings: CompileSettings::default(),
            output_bsp: None,
            root: Some(dir.join("no_game")),
            fs: None,
        };
        p.start(&ctx, job);
        assert!(p.progress().is_some());
        let mut state = hp_app::testing::test_state();
        let t0 = Instant::now();
        while p.progress().is_some() && t0.elapsed() < Duration::from_secs(20) {
            p.poll(&mut state);
            std::thread::sleep(Duration::from_millis(10));
        }
        assert!(p.progress().is_none());
        let copied = dir.join("a.bin");
        std::fs::write(&copied, b"x").expect("write");
        copy_atomic(&copied, &dir.join("out").join("b.bin")).expect("copy");
        assert!(dir.join("out").join("b.bin").is_file());
    }
}
