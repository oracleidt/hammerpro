use std::collections::{HashMap, VecDeque};
use std::io::Write;
use std::sync::{Mutex, OnceLock};
use std::time::{Duration, Instant};

use crossbeam_channel::{Receiver, Sender};
use hp_app::Severity;

const RECENT_LINES: usize = 200;

fn recent() -> &'static Mutex<VecDeque<String>> {
    static RECENT: OnceLock<Mutex<VecDeque<String>>> = OnceLock::new();
    RECENT.get_or_init(|| Mutex::new(VecDeque::with_capacity(RECENT_LINES)))
}

pub fn recent_lines() -> Vec<String> {
    match recent().lock() {
        Ok(lines) => lines.iter().cloned().collect(),
        Err(poisoned) => poisoned.into_inner().iter().cloned().collect(),
    }
}

pub fn recent_lines_in_hook() -> Vec<String> {
    for _ in 0..50 {
        match recent().try_lock() {
            Ok(lines) => return lines.iter().cloned().collect(),
            Err(std::sync::TryLockError::Poisoned(poisoned)) => return poisoned.into_inner().iter().cloned().collect(),
            Err(std::sync::TryLockError::WouldBlock) => std::thread::sleep(std::time::Duration::from_millis(2)),
        }
    }
    vec!["(the recent log lines were busy)".to_owned()]
}

fn remember(line: &str) {
    let mut lines = match recent().lock() {
        Ok(l) => l,
        Err(poisoned) => poisoned.into_inner(),
    };
    if lines.len() >= RECENT_LINES {
        lines.pop_front();
    }
    lines.push_back(line.to_owned());
}

const BURST: u32 = 5;
const WINDOW: Duration = Duration::from_secs(10);
const MAX_SHAPES: usize = 512;

#[derive(Default)]
struct Limiter {
    shapes: HashMap<String, (Instant, u32, u64)>,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
enum Admit {
    Yes,
    YesAfterHidden(u64),
    No,
}

fn shape(target: &str, text: &str) -> String {
    let mut key = String::with_capacity(target.len() + 64);
    key.push_str(target);
    key.push(' ');
    key.extend(text.chars().filter(|c| !c.is_ascii_digit()).take(64));
    key
}

impl Limiter {
    fn admit(&mut self, key: String, now: Instant) -> Admit {
        if self.shapes.len() >= MAX_SHAPES && !self.shapes.contains_key(&key) {
            self.shapes.retain(|_, (start, _, _)| now.duration_since(*start) < WINDOW);
            if self.shapes.len() >= MAX_SHAPES {
                self.shapes.clear();
            }
        }
        let entry = self.shapes.entry(key).or_insert((now, 0, 0));
        if now.duration_since(entry.0) >= WINDOW {
            let hidden = entry.2;
            *entry = (now, 1, 0);
            return if hidden > 0 { Admit::YesAfterHidden(hidden) } else { Admit::Yes };
        }
        entry.1 += 1;
        if entry.1 <= BURST {
            Admit::Yes
        } else {
            entry.2 += 1;
            Admit::No
        }
    }
}

struct Logger {
    tx: Sender<(Severity, String)>,
    file: Option<Mutex<std::fs::File>>,
    limiter: Mutex<Limiter>,
}

fn is_ours(target: &str) -> bool {
    target.starts_with("hp") || target.starts_with("hammerpro")
}

fn to_console(target: &str, level: log::Level) -> bool {
    is_ours(target) && level <= log::Level::Warn
}

impl log::Log for Logger {
    fn enabled(&self, metadata: &log::Metadata<'_>) -> bool {
        metadata.level() <= if is_ours(metadata.target()) { log::Level::Info } else { log::Level::Warn }
    }

    fn log(&self, record: &log::Record<'_>) {
        if !self.enabled(record.metadata()) {
            return;
        }
        let mut text = format!("{}", record.args());
        let admit = match self.limiter.lock() {
            Ok(mut l) => l.admit(shape(record.target(), &text), Instant::now()),
            Err(poisoned) => poisoned.into_inner().admit(shape(record.target(), &text), Instant::now()),
        };
        match admit {
            Admit::No => return,
            Admit::YesAfterHidden(n) => text.push_str(&format!(" ({n} similar messages were hidden)")),
            Admit::Yes => {}
        }
        let line = format!("[{}] {}: {}", record.level(), record.target(), text);
        eprintln!("{line}");
        remember(&line);
        if let Some(file) = &self.file
            && let Ok(mut f) = file.lock()
        {
            let _ = writeln!(f, "{line}");
        }
        if to_console(record.target(), record.level()) {
            let severity = if record.level() == log::Level::Error { Severity::Error } else { Severity::Warning };
            let _ = self.tx.try_send((severity, text));
        }
    }

    fn flush(&self) {
        if let Some(file) = &self.file
            && let Ok(mut f) = file.lock()
        {
            let _ = f.flush();
        }
    }
}

fn open_log_file() -> Option<std::fs::File> {
    let dir = hp_app::settings::Settings::log_dir();
    std::fs::create_dir_all(&dir).ok()?;
    let path = dir.join("hammerpro.log");
    if path.exists() {
        let _ = std::fs::rename(&path, dir.join("hammerpro.old.log"));
    }
    std::fs::File::create(path).ok()
}

pub fn init(to_file: bool) -> Receiver<(Severity, String)> {
    let (tx, rx) = crossbeam_channel::bounded(4096);
    let file = if to_file { open_log_file().map(Mutex::new) } else { None };
    if log::set_boxed_logger(Box::new(Logger { tx, file, limiter: Mutex::new(Limiter::default()) })).is_ok() {
        log::set_max_level(log::LevelFilter::Info);
    }
    rx
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn only_our_warnings_reach_the_console() {
        assert!(!to_console("hp_fs::fs", log::Level::Info), "English diagnostics stay out of the console");
        assert!(to_console("hp_fs::fs", log::Level::Warn));
        assert!(to_console("hammerpro::files", log::Level::Error));
        assert!(!to_console("wgpu_core", log::Level::Error));
    }

    #[test]
    fn repeated_messages_are_rate_limited() {
        let mut l = Limiter::default();
        let t0 = Instant::now();
        let key = |i: u32| shape("egui_winit", &format!("CursorPosition(Pos2 {{ x: {i}, y: 3 }}): os error 0"));
        let shown = (0..500).filter(|&i| l.admit(key(i), t0) != Admit::No).count();
        assert_eq!(shown, BURST as usize, "only the first ones of a burst are written");
        assert_eq!(l.admit(shape("hp_render", "other"), t0), Admit::Yes, "other messages are not hidden");
        assert_eq!(l.admit(key(1), t0 + WINDOW), Admit::YesAfterHidden(500 - u64::from(BURST)));
        assert_eq!(l.admit(key(2), t0 + WINDOW), Admit::Yes);
        for i in 0..(MAX_SHAPES as u32 * 2) {
            l.admit(shape("x", &format!("{}", char::from_u32(0x4e00 + i).unwrap_or('a'))), t0);
        }
        assert!(l.shapes.len() <= MAX_SHAPES);
    }

    #[test]
    fn recent_lines_are_bounded() {
        for i in 0..(RECENT_LINES + 10) {
            remember(&format!("line {i}"));
        }
        let lines = recent_lines();
        assert!(lines.len() <= RECENT_LINES);
        assert!(lines.last().is_some_and(|l| l.starts_with("line ")));
    }
}
