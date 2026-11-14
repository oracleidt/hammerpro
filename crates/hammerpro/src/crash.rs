use std::cell::Cell;
use std::path::{Path, PathBuf};
use std::sync::Mutex;
use std::sync::atomic::{AtomicUsize, Ordering};
use std::time::{SystemTime, UNIX_EPOCH};

use hp_app::settings::Settings;

const PENDING: &str = "pending.txt";
const RUNNING_PREFIX: &str = "running-";
const LOG_TAIL: usize = 200;
const KEEP_REPORTS: usize = 10;
const MAX_WORKER_REPORTS: usize = 3;

thread_local! {
    static IN_HOOK: Cell<bool> = const { Cell::new(false) };
}
static WORKER_REPORTS: AtomicUsize = AtomicUsize::new(0);
static LAST_REPORT: Mutex<Option<PathBuf>> = Mutex::new(None);

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct PendingCrash {
    pub report: PathBuf,
    pub emergency_save: Option<PathBuf>,
    pub original: Option<PathBuf>,
}

fn now_secs() -> u64 {
    SystemTime::now().duration_since(UNIX_EPOCH).map(|d| d.as_secs()).unwrap_or(0)
}

fn now_millis() -> u128 {
    SystemTime::now().duration_since(UNIX_EPOCH).map(|d| d.as_millis()).unwrap_or(0)
}

pub fn report_text(message: &str, location: &str, thread: &str, backtrace: &str, log: &[String]) -> String {
    let mut out = String::new();
    out.push_str(&format!("Hammer Pro {} crash report\n", env!("CARGO_PKG_VERSION")));
    out.push_str(&format!("time (unix): {}\n", now_secs()));
    out.push_str(&format!("os: {} {}\n", std::env::consts::OS, std::env::consts::ARCH));
    out.push_str(&format!("thread: {thread}\nlocation: {location}\nmessage: {message}\n\nbacktrace:\n{backtrace}\n"));
    out.push_str("\nlast log lines:\n");
    for line in log {
        out.push_str(line);
        out.push('\n');
    }
    out
}

pub fn write_report(dir: &Path, text: &str) -> std::io::Result<PathBuf> {
    let path = write_report_file(dir, text, "")?;
    std::fs::write(dir.join(PENDING), format!("{}\n", path.display()))?;
    Ok(path)
}

pub fn write_worker_report(dir: &Path, text: &str) -> std::io::Result<PathBuf> {
    write_report_file(dir, text, "-worker")
}

fn write_report_file(dir: &Path, text: &str, suffix: &str) -> std::io::Result<PathBuf> {
    std::fs::create_dir_all(dir)?;
    let stamp = now_millis();
    let mut path = dir.join(format!("crash-{stamp}{suffix}.txt"));
    let mut n = 1;
    while path.exists() {
        path = dir.join(format!("crash-{stamp}{suffix}-{n}.txt"));
        n += 1;
    }
    std::fs::write(&path, text)?;
    prune_reports(dir);
    Ok(path)
}

pub fn read_report(path: &Path) -> Option<String> {
    std::fs::read_to_string(path).ok().filter(|t| !t.trim().is_empty())
}

pub fn add_emergency_save(dir: &Path, report: &Path, save: &Path, original: Option<&Path>) {
    use std::io::Write;
    if let Ok(mut f) = std::fs::OpenOptions::new().append(true).open(report) {
        let _ = writeln!(f, "\nemergency save: {}", save.display());
    }
    let original = original.map(|p| p.display().to_string()).unwrap_or_default();
    let _ = std::fs::write(dir.join(PENDING), format!("{}\n{}\n{original}\n", report.display(), save.display()));
}

fn prune_reports(dir: &Path) {
    let Ok(entries) = std::fs::read_dir(dir) else { return };
    let mut reports: Vec<PathBuf> = entries
        .filter_map(Result::ok)
        .map(|e| e.path())
        .filter(|p| p.file_name().is_some_and(|n| n.to_string_lossy().starts_with("crash-")))
        .collect();
    reports.sort();
    let excess = reports.len().saturating_sub(KEEP_REPORTS);
    for old in reports.into_iter().take(excess) {
        let _ = std::fs::remove_file(old);
    }
}

pub fn take_pending(dir: &Path) -> Option<PendingCrash> {
    let marker = dir.join(PENDING);
    let text = std::fs::read_to_string(&marker).ok()?;
    let _ = std::fs::remove_file(&marker);
    let mut lines = text.lines().map(str::trim);
    let report = PathBuf::from(lines.next().filter(|l| !l.is_empty())?);
    let emergency_save = lines.next().filter(|l| !l.is_empty()).map(PathBuf::from).filter(|p| p.is_file());
    let original = lines.next().filter(|l| !l.is_empty() && emergency_save.is_some()).map(PathBuf::from);
    Some(PendingCrash { report, emergency_save, original })
}

fn marker_name(pid: u32) -> String {
    format!("{RUNNING_PREFIX}{pid}.txt")
}

pub fn unexpected_end_text(previous_log: Option<&str>) -> String {
    let mut out = String::new();
    out.push_str(&format!("Hammer Pro {} crash report\n", env!("CARGO_PKG_VERSION")));
    out.push_str(&format!("time (unix): {}\n", now_secs()));
    out.push_str(&format!("os: {} {}\n", std::env::consts::OS, std::env::consts::ARCH));
    out.push_str("message: the previous run ended without closing normally and without a panic ");
    out.push_str("(graphics driver crash, allocation failure, stack overflow, or the process was killed)\n");
    out.push_str("\nlast log lines of that run:\n");
    match previous_log {
        Some(log) => {
            let lines: Vec<&str> = log.lines().collect();
            for line in &lines[lines.len().saturating_sub(LOG_TAIL)..] {
                out.push_str(line);
                out.push('\n');
            }
        }
        None => out.push_str("(no log file)\n"),
    }
    out
}

pub fn begin_session(
    dir: &Path,
    pid: u32,
    previous_log: Option<&Path>,
    alive: &dyn Fn(u32) -> bool,
) -> Option<PendingCrash> {
    let mut dead = false;
    if let Ok(entries) = std::fs::read_dir(dir) {
        for e in entries.filter_map(Result::ok) {
            let name = e.file_name().to_string_lossy().into_owned();
            let Some(other) = name
                .strip_prefix(RUNNING_PREFIX)
                .and_then(|n| n.strip_suffix(".txt"))
                .and_then(|n| n.parse::<u32>().ok())
            else {
                continue;
            };
            if other != pid && !alive(other) {
                dead = true;
                let _ = std::fs::remove_file(e.path());
            }
        }
    }
    let mut pending = take_pending(dir);
    if pending.is_none() && dead {
        let log = previous_log.and_then(|p| std::fs::read_to_string(p).ok());
        if let Ok(report) = write_report_file(dir, &unexpected_end_text(log.as_deref()), "-unexpected") {
            pending = Some(PendingCrash { report, emergency_save: None, original: None });
        }
    }
    if std::fs::create_dir_all(dir).is_ok() {
        let _ = std::fs::write(dir.join(marker_name(pid)), format!("{}\n", now_secs()));
    }
    pending
}

pub fn end_session(dir: &Path, pid: u32) {
    let _ = std::fs::remove_file(dir.join(marker_name(pid)));
}

pub fn write_device_lost_report(dir: &Path, reason: &str) -> std::io::Result<PathBuf> {
    let message = format!("the graphics device was lost: {reason}");
    let text = report_text(&message, "", "main", "(none: a driver reset, not a panic)", &crate::logger::recent_lines());
    let path = write_report(dir, &text)?;
    if let Ok(mut last) = LAST_REPORT.lock() {
        *last = Some(path.clone());
    }
    Ok(path)
}

pub fn remember_report(path: &Path) {
    if let Ok(mut last) = LAST_REPORT.lock() {
        *last = Some(path.to_path_buf());
    }
}

pub fn last_report() -> Option<PathBuf> {
    LAST_REPORT.lock().ok().and_then(|r| r.clone())
}

fn panic_message(info: &std::panic::PanicHookInfo<'_>) -> String {
    let payload = info.payload();
    if let Some(s) = payload.downcast_ref::<&str>() {
        (*s).to_owned()
    } else if let Some(s) = payload.downcast_ref::<String>() {
        s.clone()
    } else {
        "(no message)".to_owned()
    }
}

pub fn install_hook() {
    crate::native_crash::install(Settings::crash_dir());
    install_hook_in(Settings::crash_dir());
}

pub fn install_hook_in(dir: PathBuf) {
    let previous = std::panic::take_hook();
    std::panic::set_hook(Box::new(move |info| {
        use std::io::Write as _;
        previous(info);
        if IN_HOOK.with(|h| h.replace(true)) {
            return;
        }
        let thread = std::thread::current();
        let name = thread.name().unwrap_or("unnamed").to_owned();
        let message = panic_message(info);
        let location = info.location().map(|l| l.to_string()).unwrap_or_default();
        if name == "main" && !egui_wgpu::recovery::in_guarded_paint() {
            let backtrace = std::backtrace::Backtrace::force_capture().to_string();
            let text = report_text(&message, &location, &name, &backtrace, &crate::logger::recent_lines_in_hook());
            match write_report(&dir, &text) {
                Ok(path) => {
                    let _ = writeln!(std::io::stderr(), "crash report written to {}", path.display());
                    if let Ok(mut last) = LAST_REPORT.lock() {
                        *last = Some(path);
                    }
                }
                Err(e) => {
                    let _ = writeln!(std::io::stderr(), "cannot write the crash report: {e}");
                }
            }
        } else if WORKER_REPORTS.fetch_add(1, Ordering::Relaxed) < MAX_WORKER_REPORTS {
            let backtrace = std::backtrace::Backtrace::force_capture().to_string();
            let text = report_text(&message, &location, &name, &backtrace, &crate::logger::recent_lines_in_hook());
            match write_worker_report(&dir, &text) {
                Ok(path) => {
                    log::error!("panic in thread {name} at {location}: {message} (report: {})", path.display())
                }
                Err(e) => log::error!("panic in thread {name} at {location}: {message} (no report: {e})"),
            }
        } else {
            log::error!("panic in thread {name} at {location}: {message}");
        }
        IN_HOOK.with(|h| h.set(false));
    }));
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn reports_are_shown_once_with_their_emergency_save() {
        let dir = std::env::temp_dir().join(format!("hammerpro_crash_{}", std::process::id()));
        let _ = std::fs::remove_dir_all(&dir);
        let text = report_text("boom", "src/app.rs:1:1", "main", "frames", &["[INFO] hp: hello".to_owned()]);
        assert!(text.contains("boom") && text.contains("[INFO] hp: hello"));
        let report = write_report(&dir, &text).unwrap();
        let save = dir.join("map_emergency.vmf");
        std::fs::write(&save, "versioninfo {}").unwrap();
        let map = dir.join("room.vmf");
        add_emergency_save(&dir, &report, &save, Some(&map));
        let pending = take_pending(&dir).unwrap();
        assert_eq!(pending, PendingCrash { report: report.clone(), emergency_save: Some(save), original: Some(map) });
        assert!(std::fs::read_to_string(&report).unwrap().contains("emergency save"));
        assert_eq!(take_pending(&dir), None, "shown once");
        let _ = std::fs::remove_dir_all(&dir);
    }

    #[test]
    fn worker_reports_are_not_pending_and_never_overwrite() {
        let dir = std::env::temp_dir().join(format!("hammerpro_crash_worker_{}", std::process::id()));
        let _ = std::fs::remove_dir_all(&dir);
        let a = write_worker_report(&dir, "first").unwrap();
        let b = write_worker_report(&dir, "second").unwrap();
        assert_ne!(a, b, "two reports of one instant keep both");
        assert_eq!(read_report(&a).as_deref(), Some("first"));
        assert_eq!(read_report(&b).as_deref(), Some("second"));
        assert!(a.file_name().unwrap().to_string_lossy().starts_with("crash-"), "pruned with the others");
        assert_eq!(take_pending(&dir), None, "a worker panic is not offered at the next start");
        assert_eq!(read_report(&dir.join("gone.txt")), None);
        let _ = std::fs::remove_dir_all(&dir);
    }

    #[test]
    fn a_run_that_did_not_close_is_reported_once() {
        let dir = std::env::temp_dir().join(format!("hammerpro_crash_session_{}", std::process::id()));
        let _ = std::fs::remove_dir_all(&dir);
        let log = dir.join("previous.log");
        std::fs::create_dir_all(&dir).unwrap();
        std::fs::write(&log, "[INFO] hammerpro: hello\n[ERROR] hp_render: last words\n").unwrap();
        let dead = |_: u32| false;
        assert_eq!(begin_session(&dir, 10, Some(&log), &dead), None);
        end_session(&dir, 10);
        assert_eq!(begin_session(&dir, 11, Some(&log), &dead), None);
        let pending = begin_session(&dir, 12, Some(&log), &dead).expect("a report");
        let text = read_report(&pending.report).unwrap();
        assert!(text.contains("without closing normally") && text.contains("last words"), "{text}");
        assert_eq!(pending.emergency_save, None);
        let running = |pid: u32| pid == 12;
        assert_eq!(begin_session(&dir, 13, Some(&log), &running), None, "a live instance is not a crash");
        end_session(&dir, 13);
        end_session(&dir, 12);
        let _ = begin_session(&dir, 14, None, &dead);
        let report = write_report(&dir, "panic report").unwrap();
        let pending = begin_session(&dir, 15, None, &dead).expect("the panic report");
        assert_eq!(pending.report, report);
        let _ = std::fs::remove_dir_all(&dir);
    }

    #[test]
    fn a_lost_device_leaves_a_pending_report() {
        let dir = std::env::temp_dir().join(format!("hammerpro_crash_lost_{}", std::process::id()));
        let _ = std::fs::remove_dir_all(&dir);
        let report = write_device_lost_report(&dir, "Unknown: driver reset").unwrap();
        assert_eq!(last_report().as_deref(), Some(report.as_path()));
        let text = read_report(&report).unwrap();
        assert!(text.contains("the graphics device was lost: Unknown: driver reset"), "{text}");
        assert_eq!(take_pending(&dir).map(|p| p.report), Some(report));
        let _ = std::fs::remove_dir_all(&dir);
    }

    const HOOK_CHILD_DIR: &str = "HP_CRASH_HOOK_CHILD_DIR";

    #[test]
    #[ignore = "run in a child process by panics_in_any_thread_leave_reports_even_with_stderr_closed"]
    fn panic_hook_child() {
        let Some(dir) = std::env::var_os(HOOK_CHILD_DIR) else { return };
        install_hook_in(PathBuf::from(dir));
        let spawn = |name: &str, message: &'static str| {
            std::thread::Builder::new().name(name.to_owned()).spawn(move || panic!("{message}")).unwrap()
        };
        assert!(spawn("hp-worker-test", "worker boom").join().is_err());
        assert!(spawn("main", "main boom").join().is_err());
        let _ = std::panic::take_hook();
    }

    #[test]
    fn panics_in_any_thread_leave_reports_even_with_stderr_closed() {
        let dir = std::env::temp_dir().join(format!("hammerpro_crash_hook_{}", std::process::id()));
        let _ = std::fs::remove_dir_all(&dir);
        let (reader, writer) = std::io::pipe().unwrap();
        drop(reader);
        let out = std::process::Command::new(std::env::current_exe().unwrap())
            .args(["--exact", "crash::tests::panic_hook_child", "--ignored", "--nocapture", "--test-threads", "1"])
            .env(HOOK_CHILD_DIR, &dir)
            .stdin(std::process::Stdio::null())
            .stderr(writer)
            .output()
            .unwrap();
        let stdout = String::from_utf8_lossy(&out.stdout);
        assert!(
            out.status.success(),
            "the hook must not abort the process: {:?}
{stdout}",
            out.status
        );
        assert!(stdout.contains("1 passed"), "{stdout}");
        let reports: Vec<String> = std::fs::read_dir(&dir)
            .unwrap()
            .filter_map(Result::ok)
            .map(|e| e.path())
            .filter(|p| p.file_name().is_some_and(|n| n.to_string_lossy().starts_with("crash-")))
            .filter_map(|p| read_report(&p))
            .collect();
        assert!(
            reports.iter().any(|r| r.contains("thread: hp-worker-test") && r.contains("message: worker boom")),
            "{reports:?}"
        );
        assert!(reports.iter().any(|r| r.contains("thread: main") && r.contains("message: main boom")), "{reports:?}");
        let pending = take_pending(&dir).expect("a main thread panic is offered at the next start");
        assert!(read_report(&pending.report).is_some_and(|r| r.contains("main boom")));
        let _ = std::fs::remove_dir_all(&dir);
    }

    #[test]
    fn old_reports_are_pruned() {
        let dir = std::env::temp_dir().join(format!("hammerpro_crash_prune_{}", std::process::id()));
        let _ = std::fs::remove_dir_all(&dir);
        std::fs::create_dir_all(&dir).unwrap();
        for i in 0..(KEEP_REPORTS + 5) {
            std::fs::write(dir.join(format!("crash-{:05}.txt", i)), "x").unwrap();
        }
        prune_reports(&dir);
        let count = std::fs::read_dir(&dir).unwrap().count();
        assert_eq!(count, KEEP_REPORTS);
        assert!(dir.join(format!("crash-{:05}.txt", KEEP_REPORTS + 4)).exists(), "the newest stay");
        let _ = std::fs::remove_dir_all(&dir);
    }
}
