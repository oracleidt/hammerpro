use std::io::Read;
use std::path::{Path, PathBuf};
use std::process::{Child, Command, Stdio};
use std::sync::Arc;
use std::sync::atomic::{AtomicBool, Ordering};
use std::thread::JoinHandle;
use std::time::{Duration, Instant, SystemTime};

use crossbeam_channel::{Receiver, RecvTimeoutError, Sender};

use crate::log::{LogLocation, Severity, is_leak_line, parse_log_line};
use crate::notice::{Notice, crash_kind};
use crate::pack::{self, PackReport};
use crate::plan::{CompilePlan, Step, StepAction, StepKind, command_line_string};
use crate::progress::{OutputSplitter, Piece};

const POLL: Duration = Duration::from_millis(20);

const PIPE_GRACE: Duration = Duration::from_millis(500);

#[cfg(windows)]
pub(crate) const CREATE_NO_WINDOW: u32 = 0x0800_0000;

#[cfg(windows)]
pub(crate) const ABOVE_NORMAL_PRIORITY_CLASS: u32 = 0x0000_8000;

#[derive(Debug, Clone, PartialEq)]
pub enum CompileEvent {
    StepStarted { index: usize, kind: StepKind, command_line: String },
    Log { step: usize, line: String, severity: Severity, location: Option<LogLocation> },
    Progress { step: usize, fraction: f32, phase: Option<String> },
    StepFinished { index: usize, success: bool, exit_code: Option<i32>, duration_ms: u64 },
    Notice { step: usize, notice: Notice },
    StepSkipped { index: usize, kind: StepKind },
    LeakDetected { pointfile: PathBuf },
    Finished { success: bool, cancelled: bool, leaked: bool, total_ms: u64 },
}

#[derive(Debug)]
pub struct CompileHandle {
    pub events: Receiver<CompileEvent>,
    cancel: Arc<AtomicBool>,
    running: Arc<AtomicBool>,
    thread: Option<JoinHandle<()>>,
}

impl CompileHandle {
    pub fn cancel(&self) {
        self.cancel.store(true, Ordering::Relaxed);
    }

    pub fn is_running(&self) -> bool {
        self.running.load(Ordering::Acquire)
    }

    pub fn is_cancelled(&self) -> bool {
        self.cancel.load(Ordering::Relaxed)
    }

    pub fn wait(mut self) {
        if let Some(thread) = self.thread.take() {
            let _ = thread.join();
        }
    }
}

impl Drop for CompileHandle {
    fn drop(&mut self) {
        self.cancel();
    }
}

pub fn start(plan: CompilePlan, workdir: &Path) -> CompileHandle {
    start_inner(plan, workdir, None)
}

pub fn start_with_waker(plan: CompilePlan, workdir: &Path, waker: impl Fn() + Send + Sync + 'static) -> CompileHandle {
    start_inner(plan, workdir, Some(Arc::new(waker)))
}

type Waker = Arc<dyn Fn() + Send + Sync>;

fn effective_workdir(workdir: &Path, plan: &CompilePlan) -> PathBuf {
    if !workdir.as_os_str().is_empty() {
        return workdir.to_path_buf();
    }
    let map_dir = std::path::absolute(&plan.vmf_path).ok().and_then(|p| p.parent().map(Path::to_path_buf));
    map_dir.filter(|d| !d.as_os_str().is_empty()).unwrap_or_else(|| PathBuf::from("."))
}

fn start_inner(plan: CompilePlan, workdir: &Path, waker: Option<Waker>) -> CompileHandle {
    let (tx, rx) = crossbeam_channel::unbounded();
    let cancel = Arc::new(AtomicBool::new(false));
    let running = Arc::new(AtomicBool::new(true));
    let workdir = effective_workdir(workdir, &plan);
    let worker = Worker {
        plan,
        workdir,
        tx: tx.clone(),
        waker: waker.clone(),
        cancel: Arc::clone(&cancel),
        leaked: false,
        step_leak_text: false,
        step_stats: Vec::new(),
    };
    let running_flag = Arc::clone(&running);
    let spawned = std::thread::Builder::new().name("hp-compile".to_owned()).spawn(move || {
        let _guard = RunningGuard(running_flag);
        worker.run();
    });
    let thread = match spawned {
        Ok(t) => Some(t),
        Err(e) => {
            let _ = tx.send(CompileEvent::Notice { step: 0, notice: Notice::ThreadFailed { error: e.to_string() } });
            let _ = tx.send(CompileEvent::Finished { success: false, cancelled: false, leaked: false, total_ms: 0 });
            running.store(false, Ordering::Release);
            if let Some(w) = &waker {
                w();
            }
            None
        }
    };
    CompileHandle { events: rx, cancel, running, thread }
}

struct RunningGuard(Arc<AtomicBool>);

impl Drop for RunningGuard {
    fn drop(&mut self) {
        self.0.store(false, Ordering::Release);
    }
}

#[derive(Debug, Clone, Copy)]
struct Outcome {
    success: bool,
    exit_code: Option<i32>,
    cancelled: bool,
    started: bool,
}

impl Outcome {
    const OK: Outcome = Outcome { success: true, exit_code: None, cancelled: false, started: false };
    const FAILED: Outcome = Outcome { success: false, exit_code: None, cancelled: false, started: false };
}

enum ReaderMsg {
    Piece(Piece),
    Eof,
}

const MAX_STATS_LINES: usize = 5000;

struct Worker {
    plan: CompilePlan,
    workdir: PathBuf,
    tx: Sender<CompileEvent>,
    waker: Option<Waker>,
    cancel: Arc<AtomicBool>,
    leaked: bool,
    step_leak_text: bool,
    step_stats: Vec<String>,
}

impl Worker {
    fn emit(&self, event: CompileEvent) {
        if self.tx.send(event).is_err() {
            self.cancel.store(true, Ordering::Relaxed);
            return;
        }
        if let Some(w) = &self.waker {
            w();
        }
    }

    fn notice(&self, step: usize, notice: Notice) {
        self.emit(CompileEvent::Notice { step, notice });
    }

    fn cancelled(&self) -> bool {
        self.cancel.load(Ordering::Relaxed)
    }

    fn run(mut self) {
        let started = Instant::now();
        let mut failed = false;
        let mut cancelled = false;
        for n in std::mem::take(&mut self.plan.notices) {
            self.notice(n.step, n.notice);
        }
        let steps = std::mem::take(&mut self.plan.steps);
        for (index, step) in steps.iter().enumerate().filter(|(_, s)| s.enabled) {
            if self.cancelled() {
                cancelled = true;
                break;
            }
            self.emit(CompileEvent::StepStarted { index, kind: step.kind, command_line: command_line_string(step) });
            let step_start = Instant::now();
            let outcome = match self.restore_cached(index, step.kind) {
                Some(outcome) => outcome,
                None => {
                    let outcome = self.run_step(index, step);
                    self.store_cached(index, step.kind, outcome);
                    outcome
                }
            };
            self.emit(CompileEvent::StepFinished {
                index,
                success: outcome.success,
                exit_code: outcome.exit_code,
                duration_ms: millis(step_start.elapsed()),
            });
            if outcome.cancelled {
                cancelled = true;
                break;
            }
            if step.kind == StepKind::Bsp && self.leaked && self.plan.stop_on_leak {
                self.notice(index, Notice::StoppedOnLeak);
                break;
            }
            if !outcome.success {
                failed = true;
                if !step.continue_on_error {
                    self.notice(index, Notice::PipelineStopped);
                    break;
                }
            }
        }
        let leaked = self.leaked;
        self.emit(CompileEvent::Finished {
            success: !failed && !cancelled && !leaked,
            cancelled,
            leaked,
            total_ms: millis(started.elapsed()),
        });
    }

    fn restore_cached(&mut self, index: usize, kind: StepKind) -> Option<Outcome> {
        let cache = self.plan.cache.as_ref()?;
        let (stage, key) = cache.step(index)?;
        let from = cache.restore(stage, key, &self.plan.bsp_path(), &self.plan.prt_path())?;
        if kind == StepKind::Bsp {
            let _ = std::fs::remove_file(self.plan.lin_path());
            if self.repair_collision(index, &self.plan.bsp_path()).is_err() {
                return None;
            }
        }
        self.notice(index, Notice::FromCache { from });
        for line in cache.stats(stage, key) {
            let (severity, location) = parse_log_line(&line);
            self.emit(CompileEvent::Log { step: index, line, severity, location });
        }
        Some(Outcome::OK)
    }

    fn store_cached(&self, index: usize, kind: StepKind, outcome: Outcome) {
        let Some(cache) = self.plan.cache.as_ref() else { return };
        let Some((stage, key)) = cache.step(index) else { return };
        if !outcome.success || outcome.cancelled || self.leaked || self.cancelled() {
            return;
        }
        let prt = self.plan.prt_path();
        cache.store(stage, key, &self.plan.bsp_path(), (kind == StepKind::Bsp).then_some(prt.as_path()));
        cache.store_stats(stage, key, &self.step_stats);
    }

    fn run_step(&mut self, index: usize, step: &Step) -> Outcome {
        match &step.action {
            StepAction::Run => match step.kind {
                StepKind::Bsp => self.run_bsp(index, step),
                _ => {
                    let outcome = self.run_process(index, step);
                    self.report_failure(index, step, outcome);
                    outcome
                }
            },
            StepAction::Pack { files } => self.run_pack(index, files),
            StepAction::PackDeferred { files } => match self.wait_deferred(index, files) {
                Some(files) => self.run_pack(index, &files),
                None if self.cancelled() => Outcome { cancelled: true, ..Outcome::FAILED },
                None => {
                    self.notice(index, Notice::PackListUnavailable);
                    Outcome::OK
                }
            },
            StepAction::Copy { .. } | StepAction::Spawn if crate::sandbox::is_sandboxed() => {
                self.emit(CompileEvent::Log {
                    step: index,
                    line: crate::sandbox::REFUSED.to_owned(),
                    severity: Severity::Warning,
                    location: None,
                });
                Outcome::OK
            }
            StepAction::Copy { from, to } => {
                let keep_cubemaps = step.kind == StepKind::CopyToMaps;
                self.copy_file(index, from, to, keep_cubemaps)
            }
            StepAction::Delete { path } => self.delete_file(index, path),
            StepAction::Rename { from, to } => self.rename_file(index, from, to),
            StepAction::Spawn => self.spawn_detached(index, step),
        }
    }

    fn run_bsp(&mut self, index: usize, step: &Step) -> Outcome {
        let lin = self.plan.lin_path();
        let lin_before = mtime(&lin);
        let bsp = self.plan.bsp_path();
        let mut outcome = self.run_process(index, step);
        if outcome.cancelled {
            return outcome;
        }
        let lin_after = mtime(&lin);
        let new_lin = lin_after.is_some() && lin_after != lin_before;
        if self.step_leak_text || new_lin {
            self.leaked = true;
            if lin_after.is_some() {
                self.notice(index, Notice::Leaked { pointfile: lin.clone() });
                self.emit(CompileEvent::LeakDetected { pointfile: lin });
            } else {
                self.notice(index, Notice::LeakedNoPointfile);
            }
        }
        if self.leaked && self.plan.stop_on_leak {
            outcome.success = false;
            return outcome;
        }
        self.report_failure(index, step, outcome);
        if outcome.success && !bsp.is_file() {
            outcome.success = false;
            self.notice(index, Notice::BspNotWritten { bsp });
        } else if outcome.success
            && let Err(error) = self.repair_collision(index, &bsp)
        {
            outcome.success = false;
            self.emit(CompileEvent::Log {
                step: index,
                line: format!("Collision repair failed: {error}"),
                severity: Severity::Error,
                location: None,
            });
        }
        outcome
    }

    fn repair_collision(&self, index: usize, path: &Path) -> Result<(), String> {
        let source = match hp_bsp::Bsp::open(path) {
            Ok(source) => source,
            Err(hp_bsp::BspError::NotBsp | hp_bsp::BspError::UnsupportedVersion(_)) => return Ok(()),
            Err(error) => return Err(error.to_string()),
        };
        if let Some((edit, report)) = hp_bsp::bevels::repair_axial_bevels(source).map_err(|error| error.to_string())? {
            edit.save(path).map_err(|error| error.to_string())?;
            self.emit(CompileEvent::Log {
                step: index,
                line: format!(
                    "Fixed {} invalid axial collision bevels in {} brushes; restored {} leaf references",
                    report.sides, report.brushes, report.leaf_references
                ),
                severity: Severity::Info,
                location: None,
            });
        }
        Ok(())
    }

    fn wait_deferred(&self, index: usize, files: &crate::plan::DeferredFiles) -> Option<Vec<(String, PathBuf)>> {
        let mut told = false;
        loop {
            if self.cancelled() {
                return None;
            }
            match files.wait(POLL * 5) {
                Ok(Some(list)) => return Some(list),
                Ok(None) => {
                    if !told {
                        told = true;
                        self.notice(index, Notice::PackWaiting);
                    }
                }
                Err(()) => return None,
            }
        }
    }

    fn run_pack(&mut self, index: usize, files: &[(String, PathBuf)]) -> Outcome {
        let mut valid: Vec<(String, PathBuf)> = Vec::with_capacity(files.len());
        for (inner, source) in files {
            let inner = inner.replace('\\', "/");
            let inner = inner.trim().trim_start_matches('/');
            if !pack::valid_entry(inner) {
                self.notice(index, Notice::PackInvalidPath { path: inner.to_owned() });
                continue;
            }
            if !source.is_file() {
                self.notice(index, Notice::PackMissingFile { file: source.clone() });
                continue;
            }
            valid.push((inner.to_owned(), source.clone()));
        }
        if valid.is_empty() {
            self.notice(index, Notice::NothingToPack);
            return Outcome::OK;
        }
        self.notice(index, Notice::Packing { count: valid.len() });
        let tx = self.tx.clone();
        let waker = self.waker.clone();
        let last = std::sync::atomic::AtomicU32::new(u32::MAX);
        let progress = move |done: u64, total: u64| {
            let percent = u32::try_from(done.saturating_mul(100) / total.max(1)).unwrap_or(100);
            if last.swap(percent, Ordering::Relaxed) == percent {
                return;
            }
            let fraction = percent as f32 / 100.0;
            if tx.send(CompileEvent::Progress { step: index, fraction, phase: None }).is_ok()
                && let Some(w) = &waker
            {
                w();
            }
        };
        match pack::pack_into(&self.plan.bsp_path(), &valid, &self.cancel, &progress) {
            Ok(PackReport { packed, pak_bytes, missing }) => {
                self.notice(index, Notice::Packed { count: packed, bytes: pak_bytes });
                for (inner, _) in &valid {
                    self.emit(CompileEvent::Log {
                        step: index,
                        line: format!("  {inner}"),
                        severity: Severity::Info,
                        location: None,
                    });
                }
                if !missing.is_empty() {
                    self.notice(index, Notice::PackNotInBsp { files: missing });
                }
                Outcome::OK
            }
            Err(_) if self.cancelled() => {
                self.notice(index, Notice::Cancelled);
                Outcome { cancelled: true, ..Outcome::FAILED }
            }
            Err(error) => {
                self.notice(index, Notice::PackFailed { error });
                Outcome::FAILED
            }
        }
    }

    fn copy_file(&self, index: usize, from: &Path, to: &Path, keep_cubemaps: bool) -> Outcome {
        if !from.is_file() {
            self.notice(index, Notice::CopySourceMissing { from: from.to_path_buf() });
            return Outcome::FAILED;
        }
        if let Some(parent) = to.parent()
            && let Err(e) = std::fs::create_dir_all(parent)
        {
            self.notice(index, Notice::CreateDirFailed { dir: parent.to_path_buf(), error: e.to_string() });
            return Outcome::FAILED;
        }
        if keep_cubemaps {
            match crate::cubemaps::copy_keeping_cubemaps(from, to) {
                Ok(Some(count)) => {
                    self.notice(index, Notice::CubemapsKept { count, bsp: to.to_path_buf() });
                    self.notice(index, Notice::Copied { from: from.to_path_buf(), to: to.to_path_buf() });
                    return Outcome::OK;
                }
                Ok(None) => {}
                Err(e) => {
                    self.notice(index, Notice::ReplaceFailed { to: to.to_path_buf(), error: e.to_string() });
                    return Outcome::FAILED;
                }
            }
        }
        let file_name = to.file_name().map_or_else(|| "map".into(), |n| n.to_string_lossy());
        let tmp = to.with_file_name(format!("{file_name}.hptmp"));
        if let Err(e) = std::fs::copy(from, &tmp) {
            let _ = std::fs::remove_file(&tmp);
            self.notice(index, Notice::CopyFailed { from: from.to_path_buf(), to: tmp, error: e.to_string() });
            return Outcome::FAILED;
        }
        if let Err(e) = std::fs::rename(&tmp, to) {
            let _ = std::fs::remove_file(&tmp);
            self.notice(index, Notice::ReplaceFailed { to: to.to_path_buf(), error: e.to_string() });
            return Outcome::FAILED;
        }
        self.notice(index, Notice::Copied { from: from.to_path_buf(), to: to.to_path_buf() });
        Outcome::OK
    }

    fn delete_file(&self, index: usize, path: &Path) -> Outcome {
        if !path.is_absolute() {
            self.notice(index, Notice::FileOpFailed { path: path.to_path_buf(), error: "relative path".to_owned() });
            return Outcome::FAILED;
        }
        match std::fs::remove_file(path) {
            Ok(()) => Outcome::OK,
            Err(e) if e.kind() == std::io::ErrorKind::NotFound => Outcome::OK,
            Err(e) => {
                self.notice(index, Notice::FileOpFailed { path: path.to_path_buf(), error: e.to_string() });
                Outcome::FAILED
            }
        }
    }

    fn rename_file(&self, index: usize, from: &Path, to: &Path) -> Outcome {
        if !from.is_absolute() || !to.is_absolute() {
            self.notice(index, Notice::FileOpFailed { path: from.to_path_buf(), error: "relative path".to_owned() });
            return Outcome::FAILED;
        }
        match std::fs::rename(from, to) {
            Ok(()) => Outcome::OK,
            Err(e) => {
                self.notice(index, Notice::FileOpFailed { path: from.to_path_buf(), error: e.to_string() });
                Outcome::FAILED
            }
        }
    }

    fn spawn_detached(&self, index: usize, step: &Step) -> Outcome {
        let mut cmd = Command::new(&step.exe);
        cmd.args(&step.args).stdin(Stdio::null()).stdout(Stdio::null()).stderr(Stdio::null());
        match step.workdir.as_deref().or_else(|| step.exe.parent().filter(|p| p.is_dir())) {
            Some(dir) => cmd.current_dir(dir),
            None => cmd.current_dir(&self.workdir),
        };
        match cmd.spawn() {
            Ok(child) => {
                reap_in_background(child);
                self.notice(index, Notice::Started { exe: step.exe.clone() });
                Outcome::OK
            }
            Err(e) => {
                self.notice(index, Notice::StartFailed { exe: step.exe.clone(), error: e.to_string() });
                Outcome::FAILED
            }
        }
    }

    fn run_process(&mut self, index: usize, step: &Step) -> Outcome {
        self.step_leak_text = false;
        self.step_stats.clear();
        let mut cmd = Command::new(&step.exe);
        cmd.args(&step.args)
            .current_dir(step.workdir.as_deref().unwrap_or(&self.workdir))
            .stdin(Stdio::null())
            .stdout(Stdio::piped())
            .stderr(Stdio::piped());
        #[cfg(windows)]
        {
            use std::os::windows::process::CommandExt;
            cmd.creation_flags(CREATE_NO_WINDOW | ABOVE_NORMAL_PRIORITY_CLASS);
        }
        let mut child = match cmd.spawn() {
            Ok(c) => c,
            Err(e) => {
                self.notice(index, Notice::StartFailed { exe: step.exe.clone(), error: e.to_string() });
                return Outcome::FAILED;
            }
        };
        let killer = Killer::new(&child);

        let (tx, rx) = crossbeam_channel::unbounded();
        let mut open_streams = 0usize;
        if let Some(out) = child.stdout.take()
            && spawn_reader("hp-compile-stdout", out, tx.clone())
        {
            open_streams += 1;
        }
        if let Some(err) = child.stderr.take()
            && spawn_reader("hp-compile-stderr", err, tx.clone())
        {
            open_streams += 1;
        }
        drop(tx);

        let mut status = None;
        let mut cancelled = false;
        let mut exited_at: Option<Instant> = None;
        loop {
            if !cancelled && self.cancelled() {
                cancelled = true;
                killer.kill(&mut child);
                self.notice(index, Notice::Cancelled);
            }
            if open_streams > 0 {
                match rx.recv_timeout(POLL) {
                    Ok(msg) => {
                        self.handle_reader_msg(index, step.kind, msg, &mut open_streams);
                        while let Ok(msg) = rx.try_recv() {
                            self.handle_reader_msg(index, step.kind, msg, &mut open_streams);
                        }
                    }
                    Err(RecvTimeoutError::Timeout) => {}
                    Err(RecvTimeoutError::Disconnected) => open_streams = 0,
                }
            } else if status.is_none() {
                std::thread::sleep(POLL);
            }
            if status.is_none() {
                match child.try_wait() {
                    Ok(Some(s)) => {
                        status = Some(s);
                        exited_at = Some(Instant::now());
                    }
                    Ok(None) => {}
                    Err(e) => {
                        self.notice(index, Notice::QueryFailed { error: e.to_string() });
                        killer.kill(&mut child);
                        status = child.wait().ok();
                        break;
                    }
                }
            }
            if status.is_some() && (open_streams == 0 || exited_at.is_some_and(|t| t.elapsed() >= PIPE_GRACE)) {
                break;
            }
        }
        drop(killer);
        while let Ok(msg) = rx.try_recv() {
            self.handle_reader_msg(index, step.kind, msg, &mut open_streams);
        }

        let exit_code = status.and_then(|s| s.code());
        let success = !cancelled && status.is_some_and(|s| s.success());
        Outcome { success, exit_code, cancelled, started: true }
    }

    fn report_failure(&self, index: usize, step: &Step, outcome: Outcome) {
        if outcome.success || outcome.cancelled || !outcome.started {
            return;
        }
        let program =
            step.exe.file_name().map_or_else(|| step.exe.to_string_lossy(), |n| n.to_string_lossy()).into_owned();
        let notice = match outcome.exit_code {
            Some(code) => match crash_kind(code) {
                Some(kind) => Notice::Crashed { program, code, kind },
                None => Notice::ExitCode { program, code },
            },
            None => Notice::Terminated { program },
        };
        self.notice(index, notice);
    }

    fn handle_reader_msg(&mut self, index: usize, kind: StepKind, msg: ReaderMsg, open: &mut usize) {
        match msg {
            ReaderMsg::Eof => *open = open.saturating_sub(1),
            ReaderMsg::Piece(Piece::Progress { fraction, phase }) => {
                self.emit(CompileEvent::Progress { step: index, fraction, phase });
            }
            ReaderMsg::Piece(Piece::Line(line)) => {
                let (severity, location) = parse_log_line(&line);
                if kind == StepKind::Bsp && is_leak_line(&line) {
                    self.step_leak_text = true;
                }
                if (!self.step_stats.is_empty() || crate::stats::is_section_header(&line))
                    && self.step_stats.len() < MAX_STATS_LINES
                {
                    self.step_stats.push(line.clone());
                }
                self.emit(CompileEvent::Log { step: index, line, severity, location });
            }
        }
    }
}

struct Killer {
    #[cfg(windows)]
    job: Option<crate::job::Job>,
}

impl Killer {
    #[cfg_attr(not(windows), allow(unused_variables))]
    fn new(child: &Child) -> Killer {
        Killer {
            #[cfg(windows)]
            job: crate::job::Job::for_child(child),
        }
    }

    fn kill(&self, child: &mut Child) {
        #[cfg(windows)]
        if let Some(job) = &self.job {
            job.terminate();
        }
        let _ = child.kill();
    }
}

fn spawn_reader(name: &str, mut stream: impl Read + Send + 'static, tx: Sender<ReaderMsg>) -> bool {
    std::thread::Builder::new()
        .name(name.to_owned())
        .spawn(move || {
            let mut splitter = OutputSplitter::new();
            let mut buf = vec![0u8; 16 * 1024];
            let mut send = |p: Piece| {
                let _ = tx.send(ReaderMsg::Piece(p));
            };
            loop {
                match stream.read(&mut buf) {
                    Ok(0) => break,
                    Ok(n) => splitter.feed(buf.get(..n).unwrap_or_default(), &mut send),
                    Err(e) if e.kind() == std::io::ErrorKind::Interrupted => {}
                    Err(_) => break,
                }
            }
            splitter.finish(&mut send);
            let _ = tx.send(ReaderMsg::Eof);
        })
        .is_ok()
}

fn reap_in_background(child: Child) {
    #[cfg(unix)]
    {
        let mut child = child;
        let _ = std::thread::Builder::new().name("hp-compile-reaper".to_owned()).spawn(move || {
            let _ = child.wait();
        });
    }
    #[cfg(not(unix))]
    drop(child);
}

fn mtime(path: &Path) -> Option<SystemTime> {
    std::fs::metadata(path).and_then(|m| m.modified()).ok()
}

fn millis(d: Duration) -> u64 {
    u64::try_from(d.as_millis()).unwrap_or(u64::MAX)
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::test_util::scratch;

    fn collect(handle: CompileHandle) -> Vec<CompileEvent> {
        let mut events = Vec::new();
        let deadline = Instant::now() + Duration::from_secs(60);
        while Instant::now() < deadline {
            match handle.events.recv_timeout(Duration::from_millis(100)) {
                Ok(e) => events.push(e),
                Err(RecvTimeoutError::Disconnected) => break,
                Err(RecvTimeoutError::Timeout) => {}
            }
        }
        handle.wait();
        events
    }

    fn finished(events: &[CompileEvent]) -> (bool, bool, bool) {
        match events.last() {
            Some(CompileEvent::Finished { success, cancelled, leaked, .. }) => (*success, *cancelled, *leaked),
            other => panic!("last event is not Finished: {other:?}"),
        }
    }

    fn log_lines(events: &[CompileEvent]) -> Vec<(usize, String, Severity)> {
        events
            .iter()
            .filter_map(|e| match e {
                CompileEvent::Log { step, line, severity, .. } => Some((*step, line.clone(), *severity)),
                _ => None,
            })
            .collect()
    }

    fn notices(events: &[CompileEvent]) -> Vec<(usize, Notice)> {
        events
            .iter()
            .filter_map(|e| match e {
                CompileEvent::Notice { step, notice } => Some((*step, notice.clone())),
                _ => None,
            })
            .collect()
    }

    fn shell_step(script: &str) -> Step {
        #[cfg(windows)]
        {
            Step::new(StepKind::Custom, "cmd.exe", vec!["/D".into(), "/C".into(), script.into()])
        }
        #[cfg(not(windows))]
        {
            Step::new(StepKind::Custom, "sh", vec!["-c".into(), script.into()])
        }
    }

    #[test]
    fn runs_steps_and_streams_output() {
        let dir = scratch("runner_ok");
        let mut plan = CompilePlan::new(dir.join("m.vmf"));
        plan.steps.push(shell_step("echo hello && echo Brush 12: WARNING, microbrush"));
        let mut disabled = shell_step("echo never");
        disabled.enabled = false;
        plan.steps.push(disabled);
        plan.steps.push(shell_step("echo BasePortalVis: 0...1...2...3...4...5...6...7...8...9...10 (0)"));
        let events = collect(start(plan, &dir));
        assert_eq!(finished(&events), (true, false, false));
        let logs = log_lines(&events);
        assert!(logs.iter().any(|(s, l, _)| *s == 0 && l.trim() == "hello"));
        assert!(logs.iter().any(|(s, l, sev)| *s == 0 && l.contains("microbrush") && *sev == Severity::Warning));
        assert!(!logs.iter().any(|(_, l, _)| l.contains("never")));
        assert!(
            events
                .iter()
                .any(|e| matches!(e, CompileEvent::Log { location: Some(LogLocation { brush_id: Some(12), .. }), .. }))
        );
        assert!(events.iter().any(|e| matches!(
            e,
            CompileEvent::Progress { step: 2, fraction, phase: Some(p) } if *fraction == 1.0 && p == "BasePortalVis"
        )));
        let started: Vec<usize> = events
            .iter()
            .filter_map(|e| match e {
                CompileEvent::StepStarted { index, .. } => Some(*index),
                _ => None,
            })
            .collect();
        assert_eq!(started, vec![0, 2]);
    }

    #[test]
    fn failure_stops_pipeline_unless_continue_on_error() {
        let dir = scratch("runner_fail");
        let mut plan = CompilePlan::new(dir.join("m.vmf"));
        plan.steps.push(shell_step("exit 3"));
        plan.steps.push(shell_step("echo second"));
        let events = collect(start(plan.clone(), &dir));
        assert_eq!(finished(&events), (false, false, false));
        assert!(
            events
                .iter()
                .any(|e| matches!(e, CompileEvent::StepFinished { index: 0, success: false, exit_code: Some(3), .. }))
        );
        assert!(!events.iter().any(|e| matches!(e, CompileEvent::StepStarted { index: 1, .. })));
        let n = notices(&events);
        assert!(n.iter().any(|(_, n)| matches!(n, Notice::ExitCode { code: 3, .. })), "{n:?}");
        assert!(n.iter().any(|(_, n)| *n == Notice::PipelineStopped));

        plan.steps[0].continue_on_error = true;
        let events = collect(start(plan, &dir));
        assert_eq!(finished(&events), (false, false, false));
        assert!(events.iter().any(|e| matches!(e, CompileEvent::StepFinished { index: 1, success: true, .. })));
    }

    #[test]
    fn missing_exe_fails_cleanly() {
        let dir = scratch("runner_missing");
        let mut plan = CompilePlan::new(dir.join("m.vmf"));
        plan.steps.push(Step::new(StepKind::Custom, dir.join("does_not_exist.exe"), vec![]));
        let events = collect(start(plan, &dir));
        assert!(!finished(&events).0);
        let n = notices(&events);
        assert!(n.iter().any(|(_, n)| matches!(n, Notice::StartFailed { .. }) && n.severity() == Severity::Error));
        assert!(n.iter().any(|(_, n)| n.to_string().starts_with("Could not start")));
    }

    #[test]
    fn cancel_kills_running_process() {
        let dir = scratch("runner_cancel");
        let mut plan = CompilePlan::new(dir.join("m.vmf"));
        #[cfg(windows)]
        plan.steps.push(Step::new(StepKind::Custom, "ping.exe", vec!["-n".into(), "30".into(), "127.0.0.1".into()]));
        #[cfg(not(windows))]
        plan.steps.push(Step::new(StepKind::Custom, "sleep", vec!["30".into()]));
        plan.steps.push(shell_step("echo after"));
        let handle = start(plan, &dir);
        let first = handle.events.recv_timeout(Duration::from_secs(10)).unwrap();
        assert!(matches!(first, CompileEvent::StepStarted { index: 0, .. }));
        std::thread::sleep(Duration::from_millis(300));
        assert!(handle.is_running());
        let t = Instant::now();
        handle.cancel();
        let events = collect(handle);
        assert!(t.elapsed() < Duration::from_secs(5), "cancel took {:?}", t.elapsed());
        assert_eq!(finished(&events), (false, true, false));
        assert!(!events.iter().any(|e| matches!(e, CompileEvent::StepStarted { index: 1, .. })));
    }

    #[test]
    fn copy_and_pack_builtin_steps() {
        let dir = scratch("runner_copy");
        let vmf = dir.join("gm_copy.vmf");
        std::fs::write(dir.join("gm_copy.bsp"), b"VBSP fake").unwrap();
        let maps = dir.join("maps");
        let mut plan = CompilePlan::new(&vmf);
        plan.steps.push(Step::copy(StepKind::CopyToMaps, dir.join("gm_copy.bsp"), maps.join("gm_copy.bsp")));
        plan.steps.push(Step::pack(
            &dir.join("gm_copy.bsp"),
            StepAction::Pack { files: vec![("materials/x.vmt".into(), dir.join("missing.vmt"))] },
        ));
        let events = collect(start(plan, &dir));
        assert_eq!(finished(&events), (true, false, false));
        assert_eq!(std::fs::read(maps.join("gm_copy.bsp")).unwrap(), b"VBSP fake");
        assert_eq!(std::fs::read(dir.join("gm_copy.bsp")).unwrap(), b"VBSP fake");
        assert!(!maps.join("gm_copy.bsp.hptmp").exists());
        let n = notices(&events);
        assert!(n.iter().any(|(_, n)| *n == Notice::NothingToPack));
        assert!(n.iter().any(|(_, n)| matches!(n, Notice::PackMissingFile { .. })));
        assert!(n.iter().any(|(_, n)| matches!(n, Notice::Copied { .. })));

        let mut plan = CompilePlan::new(&vmf);
        plan.steps.push(Step::copy(StepKind::CopyToMaps, dir.join("nope.bsp"), maps.join("nope.bsp")));
        assert!(!finished(&collect(start(plan, &dir))).0);
    }

    #[test]
    fn files_are_packed_into_the_bsp() {
        let dir = scratch("runner_pack");
        let src_dir = dir.join("pack\u{c9}t\u{e9}");
        std::fs::create_dir_all(&src_dir).unwrap();
        let src = src_dir.join("metal.vmt");
        std::fs::write(&src, "LightmappedGeneric {}").unwrap();
        let bsp = dir.join("m.bsp");
        pack::write_test_bsp(&bsp, &["materials/old.vmt"]);
        let mut plan = CompilePlan::new(dir.join("m.vmf"));
        plan.steps.push(Step::pack(
            &bsp,
            StepAction::Pack {
                files: vec![("\\materials\\qa\\metal.vmt".into(), src), ("../escape.vmt".into(), dir.join("x.vmt"))],
            },
        ));
        let events = collect(start(plan, &dir));
        assert_eq!(finished(&events), (true, false, false), "{:?}", notices(&events));
        assert_eq!(pack::pak_entries(&bsp).unwrap(), ["materials/old.vmt", "materials/qa/metal.vmt"]);
        let n = notices(&events);
        assert!(n.iter().any(|(_, n)| matches!(n, Notice::Packed { count: 1, .. })), "{n:?}");
        assert!(n.iter().any(|(_, n)| matches!(n, Notice::PackInvalidPath { .. })), "{n:?}");
        assert!(log_lines(&events).iter().any(|(_, l, _)| l.trim() == "materials/qa/metal.vmt"));

        std::fs::write(&bsp, b"broken").unwrap();
        let file = dir.join("a.vmt");
        std::fs::write(&file, "x").unwrap();
        let mut plan = CompilePlan::new(dir.join("m.vmf"));
        plan.steps.push(Step::pack(&bsp, StepAction::Pack { files: vec![("materials/a.vmt".into(), file)] }));
        let events = collect(start(plan, &dir));
        assert!(!finished(&events).0);
        assert!(notices(&events).iter().any(|(_, n)| matches!(n, Notice::PackFailed { .. })));
        assert_eq!(std::fs::read(&bsp).unwrap(), b"broken");
    }

    #[test]
    fn empty_workdir_runs_in_the_map_folder() {
        let dir = scratch("runner_empty_workdir");
        let mut plan = CompilePlan::new(dir.join("m.vmf"));
        #[cfg(windows)]
        plan.steps.push(shell_step("cd"));
        #[cfg(not(windows))]
        plan.steps.push(shell_step("pwd"));
        let events = collect(start(plan, Path::new("")));
        assert_eq!(finished(&events), (true, false, false), "{:?}", notices(&events));
        let logs = log_lines(&events);
        let name = dir.file_name().unwrap().to_string_lossy().into_owned();
        assert!(logs.iter().any(|(_, l, _)| l.trim_end().ends_with(&name)), "{logs:?}");
        assert_eq!(effective_workdir(Path::new(""), &CompilePlan::new("")), PathBuf::from("."));
        let rel = effective_workdir(Path::new(""), &CompilePlan::new("room.vmf"));
        assert!(rel.is_absolute() && rel == std::env::current_dir().unwrap(), "{}", rel.display());
    }

    #[test]
    fn a_step_runs_in_its_own_working_directory() {
        let dir = scratch("runner_step_workdir");
        let other = dir.join("other folder");
        std::fs::create_dir_all(&other).unwrap();
        let mut plan = CompilePlan::new(dir.join("m.vmf"));
        #[cfg(windows)]
        let mut step = shell_step("cd");
        #[cfg(not(windows))]
        let mut step = shell_step("pwd");
        step.workdir = Some(other.clone());
        plan.steps.push(step);
        let events = collect(start(plan, &dir));
        assert_eq!(finished(&events), (true, false, false), "{:?}", notices(&events));
        let logs = log_lines(&events);
        assert!(logs.iter().any(|(_, l, _)| l.trim_end().ends_with("other folder")), "{logs:?}");
    }

    #[test]
    fn leak_detection_from_fake_vbsp() {
        let dir = scratch("runner_leak");
        let vmf = dir.join("leaky.vmf");
        let lin = dir.join("leaky.lin");
        let bsp = dir.join("leaky.bsp");
        std::fs::write(&bsp, b"x").unwrap();
        #[cfg(windows)]
        let script = format!(
            "echo **** leaked **** && echo Entity info_player_start (-680.00 0.00 8.00) leaked! && (echo 1 2 3)>{}",
            lin.display()
        );
        #[cfg(not(windows))]
        let script = format!(
            "echo '**** leaked ****'; echo 'Entity info_player_start (-680.00 0.00 8.00) leaked!'; echo '1 2 3' > '{}'",
            lin.display()
        );
        let mut step = shell_step(&script);
        step.kind = StepKind::Bsp;
        let mut plan = CompilePlan::new(&vmf);
        plan.steps.push(step);
        let events = collect(start(plan, &dir));
        assert_eq!(finished(&events), (false, false, true));
        assert!(events.iter().any(|e| matches!(e, CompileEvent::LeakDetected { pointfile } if *pointfile == lin)));
        assert!(events.iter().any(|e| matches!(
            e,
            CompileEvent::Log { location: Some(LogLocation { entity_class: Some(c), position: Some(_), .. }), .. }
                if c == "info_player_start"
        )));
        assert_eq!(crate::read_pointfile(&lin).unwrap(), vec![[1.0, 2.0, 3.0]]);
    }

    #[test]
    fn waker_is_called() {
        let dir = scratch("runner_waker");
        let mut plan = CompilePlan::new(dir.join("m.vmf"));
        plan.steps.push(shell_step("echo hi"));
        let count = Arc::new(std::sync::atomic::AtomicUsize::new(0));
        let c2 = Arc::clone(&count);
        let handle = start_with_waker(plan, &dir, move || {
            c2.fetch_add(1, Ordering::Relaxed);
        });
        let events = collect(handle);
        assert!(finished(&events).0);
        assert_eq!(count.load(Ordering::Relaxed), events.len());
    }

    #[test]
    fn dropping_the_handle_cancels() {
        let dir = scratch("runner_drop");
        let mut plan = CompilePlan::new(dir.join("m.vmf"));
        #[cfg(windows)]
        plan.steps.push(Step::new(StepKind::Custom, "ping.exe", vec!["-n".into(), "30".into(), "127.0.0.1".into()]));
        #[cfg(not(windows))]
        plan.steps.push(Step::new(StepKind::Custom, "sleep", vec!["30".into()]));
        let handle = start(plan, &dir);
        let first = handle.events.recv_timeout(Duration::from_secs(10)).unwrap();
        assert!(matches!(first, CompileEvent::StepStarted { .. }));
        let running = Arc::clone(&handle.running);
        let t = Instant::now();
        drop(handle);
        while running.load(Ordering::Acquire) && t.elapsed() < Duration::from_secs(10) {
            std::thread::sleep(Duration::from_millis(10));
        }
        assert!(!running.load(Ordering::Acquire), "worker still running after drop");
    }

    #[test]
    fn spawn_step_does_not_wait() {
        let dir = scratch("runner_spawn");
        let mut plan = CompilePlan::new(dir.join("m.vmf"));
        let mut step = shell_step("exit 0");
        step.kind = StepKind::LaunchGame;
        step.action = StepAction::Spawn;
        plan.steps.push(step);
        let events = collect(start(plan, &dir));
        assert!(finished(&events).0);
        assert!(notices(&events).iter().any(|(_, n)| matches!(n, Notice::Started { .. })));
    }

    fn leaking_bsp_step(dir: &Path, name: &str) -> Step {
        let lin = dir.join(format!("{name}.lin"));
        let bsp = dir.join(format!("{name}.bsp"));
        #[cfg(windows)]
        let script = format!("echo **** leaked **** && (echo 1 2 3)>{} && (echo x)>{}", lin.display(), bsp.display());
        #[cfg(not(windows))]
        let script =
            format!("echo '**** leaked ****'; echo '1 2 3' > '{}'; echo x > '{}'", lin.display(), bsp.display());
        let mut step = shell_step(&script);
        step.kind = StepKind::Bsp;
        step
    }

    #[test]
    fn leak_keeps_going_when_asked() {
        let dir = scratch("runner_leak_continue");
        let vmf = dir.join("leaky2.vmf");
        let mut plan = CompilePlan::new(&vmf);
        plan.steps.push(leaking_bsp_step(&dir, "leaky2"));
        let mut rad = shell_step("echo lighting");
        rad.kind = StepKind::Rad;
        plan.steps.push(rad);
        plan.steps.push(Step::copy(StepKind::CopyToMaps, dir.join("leaky2.bsp"), dir.join("maps").join("leaky2.bsp")));
        let events = collect(start(plan, &dir));
        assert_eq!(finished(&events), (false, false, true));
        assert!(events.iter().any(|e| matches!(e, CompileEvent::StepFinished { index: 1, success: true, .. })));
        assert!(events.iter().any(|e| matches!(e, CompileEvent::StepFinished { index: 2, success: true, .. })));
        assert!(dir.join("maps").join("leaky2.bsp").is_file(), "the leaking map is copied for testing");
        let n = notices(&events);
        assert!(n.iter().any(|(s, n)| *s == 0 && matches!(n, Notice::Leaked { .. })));
        assert!(!n.iter().any(|(_, n)| *n == Notice::PipelineStopped));
    }

    #[test]
    fn stop_on_leak_ends_after_the_bsp_build() {
        let dir = scratch("runner_stop_on_leak");
        let vmf = dir.join("leaky4.vmf");
        let mut plan = CompilePlan::new(&vmf);
        plan.stop_on_leak = true;
        let lin = dir.join("leaky4.lin");
        #[cfg(windows)]
        let script = format!("echo **** leaked **** && (echo 1 2 3)>{} && exit 1", lin.display());
        #[cfg(not(windows))]
        let script = format!("echo '**** leaked ****'; echo '1 2 3' > '{}'; exit 1", lin.display());
        let mut bsp = shell_step(&script);
        bsp.kind = StepKind::Bsp;
        plan.steps.push(bsp);
        let mut rad = shell_step("echo rad-should-not-run");
        rad.kind = StepKind::Rad;
        plan.steps.push(rad);
        plan.notices
            .push(crate::plan::PlanNotice { step: 0, notice: Notice::NoPreviousBsp { bsp: dir.join("leaky4.bsp") } });
        let events = collect(start(plan, &dir));
        assert_eq!(finished(&events), (false, false, true));
        assert!(!events.iter().any(|e| matches!(e, CompileEvent::StepStarted { index: 1, .. })));
        let n = notices(&events);
        assert!(n.iter().any(|(s, n)| *s == 0 && *n == Notice::StoppedOnLeak), "{n:?}");
        assert!(
            !n.iter().any(|(_, n)| matches!(
                n,
                Notice::ExitCode { .. } | Notice::PipelineStopped | Notice::BspNotWritten { .. }
            )),
            "{n:?}"
        );
        assert!(matches!(events.first(), Some(CompileEvent::Notice { notice: Notice::NoPreviousBsp { .. }, .. })));
        assert!(events.iter().any(|e| matches!(e, CompileEvent::LeakDetected { .. })));
    }

    #[test]
    fn deferred_pack_waits_for_its_list() {
        let dir = scratch("runner_deferred");
        let src = dir.join("a.vmt");
        std::fs::write(&src, "x").unwrap();
        let bsp = dir.join("m.bsp");
        pack::write_test_bsp(&bsp, &[]);
        let (files, sender) = crate::plan::DeferredFiles::new();
        let mut plan = CompilePlan::new(dir.join("m.vmf"));
        plan.steps.push(shell_step("echo first"));
        plan.steps.push(Step::pack(&bsp, StepAction::PackDeferred { files }));
        let handle = start(plan, &dir);
        let mut first_done = false;
        let t0 = Instant::now();
        while !first_done && t0.elapsed() < Duration::from_secs(20) {
            if let Ok(CompileEvent::StepFinished { index: 0, .. }) =
                handle.events.recv_timeout(Duration::from_millis(100))
            {
                first_done = true;
            }
        }
        assert!(first_done);
        std::thread::sleep(Duration::from_millis(200));
        sender.send(vec![("materials/x/a.vmt".into(), src.clone())]);
        let events = collect(handle);
        assert_eq!(finished(&events), (true, false, false), "{:?}", notices(&events));
        assert!(notices(&events).iter().any(|(_, n)| *n == Notice::PackWaiting));
        assert_eq!(pack::pak_entries(&bsp).unwrap(), ["materials/x/a.vmt"]);

        let (files, sender) = crate::plan::DeferredFiles::new();
        drop(sender);
        let mut plan = CompilePlan::new(dir.join("m.vmf"));
        plan.steps.push(Step::pack(&bsp, StepAction::PackDeferred { files }));
        plan.steps.push(shell_step("echo after"));
        let events = collect(start(plan, &dir));
        assert_eq!(finished(&events), (true, false, false));
        assert!(notices(&events).iter().any(|(_, n)| *n == Notice::PackListUnavailable));
    }

    #[test]
    fn identical_steps_come_from_the_cache() {
        let dir = scratch("runner_cache");
        let vmf = dir.join("cached.vmf");
        let bsp = dir.join("cached.bsp");
        let prt = dir.join("cached.prt");
        let marker = dir.join("ran.txt");
        let make_plan = || {
            let mut plan = CompilePlan::new(&vmf);
            #[cfg(windows)]
            let script = format!(
                "echo ---- Map statistics ---- && (echo BSP)>{} && (echo PRT)>{} && (echo x)>>{}",
                bsp.display(),
                prt.display(),
                marker.display()
            );
            #[cfg(not(windows))]
            let script = format!(
                "echo '---- Map statistics ----'; echo BSP > '{}'; echo PRT > '{}'; echo x >> '{}'",
                bsp.display(),
                prt.display(),
                marker.display()
            );
            let mut b = shell_step(&script);
            b.kind = StepKind::Bsp;
            plan.steps.push(b);
            #[cfg(windows)]
            let rad_script = format!("(echo RAD)>>{} && (echo y)>>{}", bsp.display(), marker.display());
            #[cfg(not(windows))]
            let rad_script = format!("echo RAD >> '{}'; echo y >> '{}'", bsp.display(), marker.display());
            let mut r = shell_step(&rad_script);
            r.kind = StepKind::Rad;
            plan.steps.push(r);
            plan.modes = crate::plan::StepModes::default();
            plan.with_cache(7, dir.join("builds"), "cached map", 0)
        };
        let plan = make_plan();
        let keys = plan.cache.as_ref().map(|c| c.steps.len()).unwrap_or(0);
        assert_eq!(keys, 2);
        let events = collect(start(plan, &dir));
        assert_eq!(finished(&events), (true, false, false), "{:?}", notices(&events));
        assert!(!notices(&events).iter().any(|(_, n)| matches!(n, Notice::FromCache { .. })));
        let after_first = std::fs::read(&bsp).unwrap();
        let runs = std::fs::read_to_string(&marker).unwrap().lines().count();
        assert_eq!(runs, 2);
        std::fs::remove_file(&bsp).unwrap();
        std::fs::remove_file(&prt).unwrap();
        std::fs::write(dir.join("cached.lin"), "0 0 0").unwrap();
        let events = collect(start(make_plan(), &dir));
        assert_eq!(finished(&events), (true, false, false));
        let from_cache = notices(&events).iter().filter(|(_, n)| matches!(n, Notice::FromCache { .. })).count();
        assert_eq!(from_cache, 2);
        let stats = events
            .iter()
            .filter(|e| matches!(e, CompileEvent::Log { line, .. } if line.trim() == "---- Map statistics ----"))
            .count();
        assert_eq!(stats, 1, "{events:?}");
        assert_eq!(std::fs::read_to_string(&marker).unwrap().lines().count(), runs, "no process ran");
        assert_eq!(std::fs::read(&bsp).unwrap(), after_first);
        assert!(prt.is_file() && !dir.join("cached.lin").exists());
        let mut other = make_plan();
        other = CompilePlan { cache: None, ..other }.with_cache(8, dir.join("builds"), "cached map", 0);
        let events = collect(start(other, &dir));
        assert!(!notices(&events).iter().any(|(_, n)| matches!(n, Notice::FromCache { .. })));
        let mut partial = CompilePlan::new(&vmf);
        partial.modes.bsp = crate::plan::BspMode::OnlyEnts;
        assert!(partial.with_cache(1, dir.join("builds"), "m", 0).cache.is_none());
    }

    #[test]
    fn builtin_delete_and_rename() {
        let dir = std::path::absolute(scratch("runner_fileops")).unwrap();
        let a = dir.join("a.txt");
        let b = dir.join("b.txt");
        std::fs::write(&a, "x").unwrap();
        let mut plan = CompilePlan::new(dir.join("m.vmf"));
        let op = |action| Step {
            kind: StepKind::Custom,
            enabled: true,
            exe: PathBuf::new(),
            args: vec![],
            continue_on_error: false,
            action,
            workdir: None,
        };
        plan.steps.push(op(StepAction::Rename { from: a.clone(), to: b.clone() }));
        plan.steps.push(op(StepAction::Delete { path: b.clone() }));
        plan.steps.push(op(StepAction::Delete { path: dir.join("missing.txt") }));
        let events = collect(start(plan, &dir));
        assert_eq!(finished(&events), (true, false, false), "{:?}", notices(&events));
        assert!(!a.exists() && !b.exists());
        let mut plan = CompilePlan::new(dir.join("m.vmf"));
        plan.steps.push(op(StepAction::Delete { path: PathBuf::from("relative.txt") }));
        let events = collect(start(plan, &dir));
        assert!(!finished(&events).0);
        assert!(notices(&events).iter().any(|(_, n)| matches!(n, Notice::FileOpFailed { .. })));
    }
}
