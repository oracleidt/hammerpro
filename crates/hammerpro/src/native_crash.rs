use std::alloc::{GlobalAlloc, Layout, System};
use std::cell::Cell;
use std::path::{Path, PathBuf};
use std::sync::atomic::{AtomicBool, AtomicPtr, AtomicU8, AtomicU64, Ordering};
use std::sync::{Mutex, OnceLock};
use std::time::{Duration, Instant};

const RESERVE: usize = 64 << 20;
const RESERVE_ALIGN: usize = 4096;
const SAVE_WAIT: Duration = Duration::from_secs(10);
const MAX_FRAMES: usize = 64;
const BLACKBOX_PREFIX: &str = "blackbox-";
const BLACKBOX_EVERY: Duration = Duration::from_secs(5);
const WATCH_EVERY: Duration = Duration::from_millis(50);
const ANSWER_EVERY: Duration = Duration::from_millis(250);
const SLOW_FRAME_MS: u64 = 250;
const SLOW_FRAME_LOG_GAP: Duration = Duration::from_secs(2);
const HANG_CAPTURES_MS: [u64; 3] = [2_000, 10_000, 30_000];

const IDLE: u8 = 0;
const WANTED: u8 = 1;
const DONE: u8 = 2;

static RESERVE_PTR: AtomicPtr<u8> = AtomicPtr::new(std::ptr::null_mut());
static LOW_MEMORY: AtomicBool = AtomicBool::new(false);
static IN_REPORT: AtomicBool = AtomicBool::new(false);
static EMERGENCY: AtomicU8 = AtomicU8::new(IDLE);
static DIR: OnceLock<PathBuf> = OnceLock::new();
static STARTED: OnceLock<Instant> = OnceLock::new();
static FRAME: AtomicU64 = AtomicU64::new(0);
static FRAME_AT_MS: AtomicU64 = AtomicU64::new(0);
static FRAME_BUSY: AtomicBool = AtomicBool::new(false);
static VRAM_USAGE: AtomicU64 = AtomicU64::new(0);
static VRAM_BUDGET: AtomicU64 = AtomicU64::new(0);
static LONGEST_FREEZE_MS: AtomicU64 = AtomicU64::new(0);
static FROZEN_FOR_MS: AtomicU64 = AtomicU64::new(0);
static CONTEXT: Mutex<String> = Mutex::new(String::new());
static ACTIVITY: Mutex<String> = Mutex::new(String::new());
static LAST_FREEZE: Mutex<String> = Mutex::new(String::new());

thread_local! {
    static IS_MAIN: Cell<bool> = const { Cell::new(false) };
}

fn reserve_layout() -> Layout {
    Layout::from_size_align(RESERVE, RESERVE_ALIGN).unwrap_or(Layout::new::<u8>())
}

pub struct GuardedAlloc;

unsafe impl GlobalAlloc for GuardedAlloc {
    unsafe fn alloc(&self, layout: Layout) -> *mut u8 {
        let p = unsafe { System.alloc(layout) };
        if p.is_null() { failed(layout.size(), || unsafe { System.alloc(layout) }) } else { p }
    }

    unsafe fn alloc_zeroed(&self, layout: Layout) -> *mut u8 {
        let p = unsafe { System.alloc_zeroed(layout) };
        if p.is_null() { failed(layout.size(), || unsafe { System.alloc_zeroed(layout) }) } else { p }
    }

    unsafe fn dealloc(&self, ptr: *mut u8, layout: Layout) {
        unsafe { System.dealloc(ptr, layout) }
    }

    unsafe fn realloc(&self, ptr: *mut u8, layout: Layout, new_size: usize) -> *mut u8 {
        let p = unsafe { System.realloc(ptr, layout, new_size) };
        if p.is_null() { failed(new_size, || unsafe { System.realloc(ptr, layout, new_size) }) } else { p }
    }
}

fn release_reserve() -> bool {
    let p = RESERVE_PTR.swap(std::ptr::null_mut(), Ordering::AcqRel);
    if p.is_null() {
        return false;
    }
    unsafe { System.dealloc(p, reserve_layout()) };
    true
}

pub fn take_reserve() {
    if !RESERVE_PTR.load(Ordering::Acquire).is_null() {
        return;
    }
    let p = unsafe { System.alloc(reserve_layout()) };
    if p.is_null() {
        return;
    }
    if RESERVE_PTR.compare_exchange(std::ptr::null_mut(), p, Ordering::AcqRel, Ordering::Acquire).is_err() {
        unsafe { System.dealloc(p, reserve_layout()) };
    }
}

pub fn has_reserve() -> bool {
    !RESERVE_PTR.load(Ordering::Acquire).is_null()
}

fn failed(size: usize, again: impl Fn() -> *mut u8) -> *mut u8 {
    if release_reserve() {
        LOW_MEMORY.store(true, Ordering::Release);
        let p = again();
        if !p.is_null() {
            return p;
        }
    }
    let frames = current_stack();
    fatal(
        &format!("memory allocation of {size} bytes failed (the system is out of memory)"),
        Native { frames, ..Native::default() },
    );
    std::ptr::null_mut()
}

pub fn take_low_memory() -> bool {
    LOW_MEMORY.swap(false, Ordering::AcqRel)
}

pub fn emergency_wanted() -> bool {
    EMERGENCY.load(Ordering::Acquire) == WANTED
}

pub fn emergency_done() {
    let _ = EMERGENCY.compare_exchange(WANTED, DONE, Ordering::AcqRel, Ordering::Acquire);
}

fn now_ms() -> u64 {
    STARTED.get_or_init(Instant::now).elapsed().as_millis() as u64
}

pub fn frame_started(frame: u64) {
    FRAME.store(frame, Ordering::Relaxed);
    FRAME_AT_MS.store(now_ms(), Ordering::Relaxed);
    FRAME_BUSY.store(true, Ordering::Release);
}

pub fn frame_finished() {
    FRAME_BUSY.store(false, Ordering::Release);
}

pub fn set_vram(usage: u64, budget: u64) {
    VRAM_USAGE.store(usage, Ordering::Relaxed);
    VRAM_BUDGET.store(budget, Ordering::Relaxed);
}

fn replace(slot: &Mutex<String>, value: &str) -> bool {
    let mut s = match slot.lock() {
        Ok(s) => s,
        Err(p) => p.into_inner(),
    };
    if *s == value {
        return false;
    }
    s.clear();
    s.push_str(value);
    true
}

fn read(slot: &Mutex<String>) -> String {
    for _ in 0..25 {
        match slot.try_lock() {
            Ok(s) => return s.clone(),
            Err(std::sync::TryLockError::Poisoned(p)) => return p.into_inner().clone(),
            Err(std::sync::TryLockError::WouldBlock) => std::thread::sleep(Duration::from_millis(2)),
        }
    }
    "(busy)".to_owned()
}

pub fn set_context(text: &str) {
    replace(&CONTEXT, text);
}

pub fn set_activity(text: &str) {
    replace(&ACTIVITY, text);
}

fn mb(bytes: u64) -> u64 {
    bytes / (1024 * 1024)
}

pub fn state_text() -> String {
    let mut out = String::new();
    let context = read(&CONTEXT);
    let activity = read(&ACTIVITY);
    out.push_str(&format!("editor: {}\n", if context.is_empty() { "(no map yet)" } else { &context }));
    out.push_str(&format!("activity: {}\n", if activity.is_empty() { "editing" } else { &activity }));
    let spans = hp_app::runtime::main_spans();
    if !spans.is_empty() {
        out.push_str(&format!("main thread inside: {}\n", spans.join(" > ")));
    }
    out.push_str(&format!("running for: {} s\n", now_ms() / 1000));
    let frame = FRAME.load(Ordering::Relaxed);
    let since = now_ms().saturating_sub(FRAME_AT_MS.load(Ordering::Relaxed));
    out.push_str(&format!("frames drawn: {frame} (last one started {since} ms ago)\n"));
    out.push_str(&format!("memory: {}\n", memory_text()));
    let (usage, budget) = (VRAM_USAGE.load(Ordering::Relaxed), VRAM_BUDGET.load(Ordering::Relaxed));
    if usage > 0 || budget > 0 {
        out.push_str(&format!("graphics memory: {} MB used, budget {} MB\n", mb(usage), mb(budget)));
    }
    let frozen = FROZEN_FOR_MS.load(Ordering::Relaxed);
    if frozen > 0 {
        out.push_str(&format!("window not answering for: {frozen} ms\n"));
    }
    let longest = LONGEST_FREEZE_MS.load(Ordering::Relaxed);
    if longest > 0 {
        out.push_str(&format!("longest freeze of this run: {longest} ms\n"));
    }
    let freeze = read(&LAST_FREEZE);
    if !freeze.is_empty() {
        out.push_str(&format!("last freeze:\n{freeze}"));
    }
    out
}

#[derive(Default)]
struct Native {
    frames: Vec<usize>,
    dump: Option<DumpTarget>,
}

#[derive(Clone, Copy)]
struct DumpTarget {
    thread_id: u32,
    pointers: usize,
}

fn wait_for_emergency_save() {
    if IS_MAIN.with(Cell::get) || DIR.get().is_none() {
        return;
    }
    if EMERGENCY.compare_exchange(IDLE, WANTED, Ordering::AcqRel, Ordering::Acquire).is_err() {
        return;
    }
    let start = Instant::now();
    while EMERGENCY.load(Ordering::Acquire) == WANTED && start.elapsed() < SAVE_WAIT {
        std::thread::sleep(Duration::from_millis(20));
    }
}

fn thread_label() -> String {
    if IS_MAIN.with(Cell::get) {
        "main".to_owned()
    } else {
        std::thread::current().name().unwrap_or("unnamed").to_owned()
    }
}

pub fn demangle(name: &str) -> String {
    let Some(body) = name.trim_start_matches('_').strip_prefix("ZN") else { return name.to_owned() };
    let bytes = body.as_bytes();
    let mut parts: Vec<String> = Vec::new();
    let mut i = 0;
    while i < bytes.len() && bytes[i] != b'E' {
        let start = i;
        while i < bytes.len() && bytes[i].is_ascii_digit() {
            i += 1;
        }
        let Ok(len) = body[start..i].parse::<usize>() else { return name.to_owned() };
        let Some(part) = body.get(i..i + len) else { return name.to_owned() };
        parts.push(part.to_owned());
        i += len;
    }
    if parts
        .last()
        .is_some_and(|p| p.len() == 17 && p.starts_with('h') && p[1..].chars().all(|c| c.is_ascii_hexdigit()))
    {
        parts.pop();
    }
    let tail = body.get(i + 1..).unwrap_or("");
    let mut out = parts.join("::");
    for (from, to) in [
        ("$LT$", "<"),
        ("$GT$", ">"),
        ("$RF$", "&"),
        ("$BP$", "*"),
        ("$SP$", "@"),
        ("$C$", ","),
        ("$LP$", "("),
        ("$RP$", ")"),
        ("$u20$", " "),
        ("$u27$", "'"),
        ("$u5b$", "["),
        ("$u5d$", "]"),
        ("$u7b$", "{"),
        ("$u7d$", "}"),
        ("$u7e$", "~"),
        ("..", "::"),
    ] {
        out = out.replace(from, to);
    }
    out + tail
}

pub fn frames_text(frames: &[usize]) -> String {
    if frames.is_empty() {
        return "(no stack)\n".to_owned();
    }
    let names = symbolize(frames);
    let mut out = String::new();
    for (i, (addr, name)) in frames.iter().zip(names).enumerate() {
        out.push_str(&format!("{i:>3}: {} {name}\n", module_of(*addr)));
    }
    out
}

fn fatal(message: &str, native: Native) {
    if IN_REPORT.swap(true, Ordering::AcqRel) {
        return;
    }
    let message = message.to_owned();
    let thread = thread_label();
    let write = move || {
        let Some(dir) = DIR.get() else { return };
        let dump = native.dump.map(|d| write_minidump(dir, d));
        let mut details = state_text();
        match &dump {
            Some(Ok(path)) => details.push_str(&format!("minidump: {}\n", path.display())),
            Some(Err(e)) => details.push_str(&format!("minidump: not written ({e})\n")),
            None => {}
        }
        let backtrace = format!("{}\n{details}", frames_text(&native.frames));
        let text = crate::crash::report_text(&message, "", &thread, &backtrace, &crate::logger::recent_lines_in_hook());
        if let Ok(path) = crate::crash::write_report(dir, &text) {
            crate::crash::remember_report(&path);
            let _ = std::io::Write::write_fmt(
                &mut std::io::stderr(),
                format_args!("crash report written to {}\n", path.display()),
            );
        }
    };
    if let Ok(h) = std::thread::Builder::new().name("hp-crash-report".into()).stack_size(4 << 20).spawn(write) {
        let _ = h.join();
    }
    wait_for_emergency_save();
}

pub fn blackbox_path(dir: &Path, pid: u32) -> PathBuf {
    dir.join(format!("{BLACKBOX_PREFIX}{pid}.txt"))
}

pub fn blackbox_pid(name: &str) -> Option<u32> {
    name.strip_prefix(BLACKBOX_PREFIX)?.strip_suffix(".txt")?.parse().ok()
}

fn write_blackbox() {
    let Some(dir) = DIR.get() else { return };
    let path = blackbox_path(dir, std::process::id());
    let tmp = path.with_extension("tmp");
    let text = format!("written at unix {}\n{}", unix_now(), state_text());
    if std::fs::write(&tmp, text).is_ok() {
        let _ = std::fs::rename(&tmp, &path);
    }
}

pub fn remove_blackbox() {
    if let Some(dir) = DIR.get() {
        let _ = std::fs::remove_file(blackbox_path(dir, std::process::id()));
    }
}

fn unix_now() -> u64 {
    std::time::SystemTime::now().duration_since(std::time::UNIX_EPOCH).map(|d| d.as_secs()).unwrap_or(0)
}

pub fn install(dir: PathBuf) {
    let _ = STARTED.set(Instant::now());
    let _ = DIR.set(dir);
    IS_MAIN.with(|m| m.set(true));
    hp_app::runtime::mark_main_thread();
    take_reserve();
    #[cfg(windows)]
    windows::install();
}

pub fn start_watchdog() {
    let _ = std::thread::Builder::new().name("hp-watchdog".into()).stack_size(2 << 20).spawn(watchdog);
    let _ = std::thread::Builder::new().name("hp-frame-sampler".into()).stack_size(2 << 20).spawn(frame_sampler);
}

fn frame_sampler() {
    let forced = std::env::var("HAMMERPRO_SLOW_FRAME_MS").ok().and_then(|v| v.trim().parse::<u64>().ok());
    let threshold = forced.unwrap_or(SLOW_FRAME_MS).max(20);
    let gap = if forced.is_some() { Duration::ZERO } else { SLOW_FRAME_LOG_GAP };
    let poll = if forced.is_some() { Duration::from_millis(10) } else { WATCH_EVERY };
    let mut slow_frame = u64::MAX;
    let mut next_sample_ms = threshold;
    let mut slow_logged_at = Instant::now() - SLOW_FRAME_LOG_GAP;
    loop {
        std::thread::sleep(poll);
        let frame = FRAME.load(Ordering::Relaxed);
        if frame != slow_frame {
            if slow_logged_at.elapsed() < gap {
                continue;
            }
            slow_frame = frame;
            next_sample_ms = threshold;
        }
        let running = now_ms().saturating_sub(FRAME_AT_MS.load(Ordering::Relaxed));
        if !FRAME_BUSY.load(Ordering::Acquire) || running < next_sample_ms {
            continue;
        }
        next_sample_ms = next_sample_ms.saturating_mul(2);
        slow_logged_at = Instant::now();
        let frames = main_thread_stack();
        if FRAME.load(Ordering::Relaxed) == frame {
            let spans = hp_app::runtime::main_spans();
            log::info!(
                "slow frame {frame}: {running} ms so far, main thread inside {}:\n{}",
                if spans.is_empty() { "(no span)".to_owned() } else { spans.join(" > ") },
                frames_text(&frames)
            );
        }
    }
}

fn watchdog() {
    let mut last_blackbox = Instant::now() - BLACKBOX_EVERY;
    let mut answered = Instant::now();
    let mut captures = 0usize;
    let mut last_answer_check = Instant::now();
    loop {
        std::thread::sleep(WATCH_EVERY);
        if last_answer_check.elapsed() < ANSWER_EVERY {
            continue;
        }
        last_answer_check = Instant::now();
        #[cfg(windows)]
        windows::keep_filter();
        match main_window_answers() {
            Some(true) | None => {
                let frozen = answered.elapsed().as_millis() as u64;
                if captures > 0 {
                    log::warn!("the window answered again after a freeze of {frozen} ms");
                    write_blackbox();
                    last_blackbox = Instant::now();
                }
                answered = Instant::now();
                captures = 0;
                FROZEN_FOR_MS.store(0, Ordering::Relaxed);
            }
            Some(false) => {
                let frozen = answered.elapsed().as_millis() as u64;
                FROZEN_FOR_MS.store(frozen, Ordering::Relaxed);
                LONGEST_FREEZE_MS.fetch_max(frozen, Ordering::Relaxed);
                if captures < HANG_CAPTURES_MS.len() && frozen >= HANG_CAPTURES_MS[captures] {
                    captures += 1;
                    let frames = main_thread_stack();
                    let spans = hp_app::runtime::main_spans();
                    let text = format!(
                        "  window frozen for {frozen} ms, main thread inside: {}\n{}",
                        if spans.is_empty() { "(no span)".to_owned() } else { spans.join(" > ") },
                        frames_text(&frames)
                    );
                    replace(&LAST_FREEZE, &text);
                    write_blackbox();
                    last_blackbox = Instant::now();
                    log::warn!("the window has not answered for {frozen} ms, main thread stack:\n{text}");
                }
            }
        }
        if last_blackbox.elapsed() >= BLACKBOX_EVERY {
            write_blackbox();
            last_blackbox = Instant::now();
        }
    }
}

pub fn previous_run_text(dir: &Path, pid: u32) -> String {
    let mut out = String::new();
    let path = blackbox_path(dir, pid);
    match std::fs::read_to_string(&path) {
        Ok(text) => {
            let age = std::fs::metadata(&path)
                .and_then(|m| m.modified())
                .ok()
                .and_then(|t| t.elapsed().ok())
                .map(|d| d.as_secs());
            out.push_str("\nstate of that run (saved every 5 s");
            if let Some(age) = age {
                out.push_str(&format!(", last saved {age} s ago"));
            }
            out.push_str("):\n");
            out.push_str(&text);
            let frozen = text.lines().any(|l| l.starts_with("window not answering for:"));
            out.push_str(&format!(
                "verdict: {}\n",
                if frozen {
                    "the window was frozen when the run ended (see the freeze stack above)"
                } else {
                    "the window was answering normally at the last save"
                }
            ));
        }
        Err(_) => out.push_str("\nstate of that run: (not recorded)\n"),
    }
    let _ = std::fs::remove_file(&path);
    match windows_error_event(pid) {
        Some(event) => out.push_str(&format!("\nWindows error log: {event}\n")),
        None => out.push_str(
            "\nWindows error log: no crash or hang recorded for this process: the process was most likely \
             ended from outside (Task Manager, a script, a shutdown or a power loss)\n",
        ),
    }
    out
}

pub fn prune_blackboxes(dir: &Path, alive: &dyn Fn(u32) -> bool) {
    let Ok(entries) = std::fs::read_dir(dir) else { return };
    for e in entries.filter_map(Result::ok) {
        let name = e.file_name().to_string_lossy().into_owned();
        if let Some(pid) = blackbox_pid(&name)
            && pid != std::process::id()
            && !alive(pid)
        {
            let _ = std::fs::remove_file(e.path());
        }
    }
}

pub fn event_matches(xml: &str, pid: u32) -> Option<String> {
    let data: Vec<&str> = xml
        .split("<Data")
        .skip(1)
        .filter_map(|d| {
            let start = d.find('>')? + 1;
            let end = d.find("</Data>")?;
            (start <= end).then(|| &d[start..end])
        })
        .collect();
    let hex = format!("{pid:x}");
    let pid_hit = data.iter().any(|d| {
        let d = d.trim().to_ascii_lowercase();
        d == hex || d == format!("0x{hex}") || d == pid.to_string()
    });
    let ours = data.iter().any(|d| d.to_ascii_lowercase().contains("hammerpro"));
    if !pid_hit || !ours {
        return None;
    }
    let id = xml.split("<EventID").nth(1).and_then(|s| s.split('>').nth(1)).and_then(|s| s.split('<').next());
    match id.map(str::trim) {
        Some("1000") if data.len() > 7 => Some(format!(
            "native crash, exception {} in module {} {} at offset {} (application {} {})",
            data[6], data[3], data[4], data[7], data[0], data[1]
        )),
        Some("1002") => {
            Some("Windows closed the window because it had stopped answering (application hang)".to_owned())
        }
        _ => Some(format!("event {}", data.join(", "))),
    }
}

#[cfg(windows)]
fn windows_error_event(pid: u32) -> Option<String> {
    use std::os::windows::process::CommandExt;
    const CREATE_NO_WINDOW: u32 = 0x0800_0000;
    let out = std::process::Command::new("wevtutil")
        .args([
            "qe",
            "Application",
            "/q:*[System[(EventID=1000 or EventID=1002) and TimeCreated[timediff(@SystemTime) <= 1209600000]]]",
            "/c:60",
            "/rd:true",
            "/f:xml",
        ])
        .creation_flags(CREATE_NO_WINDOW)
        .output()
        .ok()?;
    let text = String::from_utf8_lossy(&out.stdout);
    text.split("<Event ").skip(1).find_map(|e| event_matches(e, pid))
}

#[cfg(not(windows))]
fn windows_error_event(_pid: u32) -> Option<String> {
    None
}

#[cfg(windows)]
fn memory_text() -> String {
    windows::memory_text()
}

#[cfg(not(windows))]
fn memory_text() -> String {
    "(unknown)".to_owned()
}

#[cfg(windows)]
fn symbolize(frames: &[usize]) -> Vec<String> {
    windows::symbolize(frames)
}

#[cfg(not(windows))]
fn symbolize(frames: &[usize]) -> Vec<String> {
    frames.iter().map(|_| String::new()).collect()
}

#[cfg(windows)]
fn module_of(address: usize) -> String {
    windows::module_of(address)
}

#[cfg(not(windows))]
fn module_of(address: usize) -> String {
    format!("0x{address:x}")
}

#[cfg(all(windows, target_arch = "x86_64"))]
fn current_stack() -> Vec<usize> {
    windows::current_stack()
}

#[cfg(not(all(windows, target_arch = "x86_64")))]
fn current_stack() -> Vec<usize> {
    Vec::new()
}

#[cfg(all(windows, target_arch = "x86_64"))]
fn main_thread_stack() -> Vec<usize> {
    windows::main_thread_stack()
}

#[cfg(not(all(windows, target_arch = "x86_64")))]
fn main_thread_stack() -> Vec<usize> {
    Vec::new()
}

#[cfg(windows)]
fn main_window_answers() -> Option<bool> {
    windows::main_window_answers()
}

#[cfg(not(windows))]
fn main_window_answers() -> Option<bool> {
    None
}

#[cfg(windows)]
fn write_minidump(dir: &Path, target: DumpTarget) -> Result<PathBuf, String> {
    windows::write_minidump(dir, target)
}

#[cfg(not(windows))]
fn write_minidump(_dir: &Path, _target: DumpTarget) -> Result<PathBuf, String> {
    Err("not supported on this system".to_owned())
}

#[cfg(windows)]
mod windows {
    use std::path::{Path, PathBuf};
    use std::sync::Mutex;
    use std::sync::atomic::{AtomicU32, AtomicUsize, Ordering};

    use windows_sys::Win32::Foundation::{
        DUPLICATE_SAME_ACCESS, DuplicateHandle, EXCEPTION_ACCESS_VIOLATION, EXCEPTION_ARRAY_BOUNDS_EXCEEDED,
        EXCEPTION_DATATYPE_MISALIGNMENT, EXCEPTION_FLT_DIVIDE_BY_ZERO, EXCEPTION_ILLEGAL_INSTRUCTION,
        EXCEPTION_IN_PAGE_ERROR, EXCEPTION_INT_DIVIDE_BY_ZERO, EXCEPTION_INT_OVERFLOW, EXCEPTION_PRIV_INSTRUCTION,
        EXCEPTION_STACK_OVERFLOW, HANDLE, HMODULE, HWND, LPARAM,
    };
    use windows_sys::Win32::System::Diagnostics::Debug::{
        AddVectoredExceptionHandler, EXCEPTION_CONTINUE_SEARCH, EXCEPTION_POINTERS, IMAGEHLP_LINEW64,
        LPTOP_LEVEL_EXCEPTION_FILTER, MINIDUMP_EXCEPTION_INFORMATION, MiniDumpNormal, MiniDumpWithThreadInfo,
        MiniDumpWithUnloadedModules, MiniDumpWriteDump, SYMBOL_INFOW, SYMOPT_DEFERRED_LOADS,
        SYMOPT_FAIL_CRITICAL_ERRORS, SYMOPT_LOAD_LINES, SYMOPT_UNDNAME, SetUnhandledExceptionFilter, SymFromAddrW,
        SymGetLineFromAddrW64, SymInitializeW, SymSetOptions,
    };
    use windows_sys::Win32::System::LibraryLoader::{
        GET_MODULE_HANDLE_EX_FLAG_FROM_ADDRESS, GET_MODULE_HANDLE_EX_FLAG_UNCHANGED_REFCOUNT, GetModuleFileNameW,
        GetModuleHandleExW,
    };
    use windows_sys::Win32::System::ProcessStatus::{
        K32GetProcessMemoryInfo, PROCESS_MEMORY_COUNTERS, PROCESS_MEMORY_COUNTERS_EX,
    };
    use windows_sys::Win32::System::SystemInformation::{GlobalMemoryStatusEx, MEMORYSTATUSEX};
    use windows_sys::Win32::System::Threading::{
        GetCurrentProcess, GetCurrentProcessId, GetCurrentThread, GetCurrentThreadId, GetCurrentThreadStackLimits,
    };
    use windows_sys::Win32::UI::WindowsAndMessaging::{
        EnumThreadWindows, IsWindowVisible, SMTO_ABORTIFHUNG, SendMessageTimeoutW, WM_NULL,
    };

    static MAIN_THREAD: AtomicUsize = AtomicUsize::new(0);
    static MAIN_THREAD_ID: AtomicU32 = AtomicU32::new(0);
    static MAIN_STACK_HIGH: AtomicUsize = AtomicUsize::new(0);
    static SYMBOLS: Mutex<bool> = Mutex::new(false);

    fn code_name(code: i32) -> &'static str {
        match code {
            EXCEPTION_ACCESS_VIOLATION => "access violation",
            EXCEPTION_STACK_OVERFLOW => "stack overflow",
            EXCEPTION_ILLEGAL_INSTRUCTION => "illegal instruction",
            EXCEPTION_INT_DIVIDE_BY_ZERO => "division by zero",
            EXCEPTION_FLT_DIVIDE_BY_ZERO => "floating point division by zero",
            EXCEPTION_INT_OVERFLOW => "integer overflow",
            EXCEPTION_IN_PAGE_ERROR => "in page error (a file or the page file could not be read)",
            EXCEPTION_PRIV_INSTRUCTION => "privileged instruction",
            EXCEPTION_ARRAY_BOUNDS_EXCEEDED => "array bounds exceeded",
            EXCEPTION_DATATYPE_MISALIGNMENT => "misaligned data",
            _ => "native exception",
        }
    }

    fn is_fatal(code: i32) -> bool {
        code == EXCEPTION_STACK_OVERFLOW
    }

    pub(super) fn module_of(address: usize) -> String {
        let mut module: HMODULE = std::ptr::null_mut();
        let flags = GET_MODULE_HANDLE_EX_FLAG_FROM_ADDRESS | GET_MODULE_HANDLE_EX_FLAG_UNCHANGED_REFCOUNT;
        if unsafe { GetModuleHandleExW(flags, address as *const u16, &mut module) } == 0 {
            return format!("0x{address:x}");
        }
        let mut buf = [0u16; 512];
        let n = unsafe { GetModuleFileNameW(module, buf.as_mut_ptr(), buf.len() as u32) } as usize;
        let path = String::from_utf16_lossy(&buf[..n.min(buf.len())]);
        let name = path.rsplit(['\\', '/']).next().unwrap_or(&path).to_owned();
        format!("{name}+0x{:x}", address.wrapping_sub(module as usize))
    }

    fn init_symbols() {
        let mut ready = match SYMBOLS.lock() {
            Ok(r) => r,
            Err(p) => p.into_inner(),
        };
        if *ready {
            return;
        }
        *ready = true;
        let exe_dir =
            std::env::current_exe().ok().and_then(|p| p.parent().map(|d| d.as_os_str().to_owned())).map(|d| {
                use std::os::windows::ffi::OsStrExt;
                d.encode_wide().chain(std::iter::once(0)).collect::<Vec<u16>>()
            });
        unsafe {
            SymSetOptions(SYMOPT_UNDNAME | SYMOPT_DEFERRED_LOADS | SYMOPT_LOAD_LINES | SYMOPT_FAIL_CRITICAL_ERRORS);
            let path = exe_dir.as_ref().map_or(std::ptr::null(), |d| d.as_ptr());
            SymInitializeW(GetCurrentProcess(), path, 1);
        }
    }

    pub(super) fn symbolize(frames: &[usize]) -> Vec<String> {
        init_symbols();
        let process = unsafe { GetCurrentProcess() };
        const NAME: usize = 512;
        frames
            .iter()
            .map(|&addr| {
                let mut buf = vec![0u64; (std::mem::size_of::<SYMBOL_INFOW>() + NAME * 2) / 8 + 1];
                let info = buf.as_mut_ptr().cast::<SYMBOL_INFOW>();
                let mut displacement = 0u64;
                let name = unsafe {
                    (*info).SizeOfStruct = std::mem::size_of::<SYMBOL_INFOW>() as u32;
                    (*info).MaxNameLen = NAME as u32;
                    if SymFromAddrW(process, addr as u64, &mut displacement, info) == 0 {
                        return String::new();
                    }
                    let len = ((*info).NameLen as usize).min(NAME);
                    let name_ptr = std::ptr::addr_of!((*info).Name).cast::<u16>();
                    String::from_utf16_lossy(std::slice::from_raw_parts(name_ptr, len))
                };
                let mut line = IMAGEHLP_LINEW64 {
                    SizeOfStruct: std::mem::size_of::<IMAGEHLP_LINEW64>() as u32,
                    ..Default::default()
                };
                let mut line_disp = 0u32;
                let place = unsafe {
                    if SymGetLineFromAddrW64(process, addr as u64, &mut line_disp, &mut line) != 0
                        && !line.FileName.is_null()
                    {
                        let mut n = 0;
                        while *line.FileName.add(n) != 0 && n < 1024 {
                            n += 1;
                        }
                        let file = String::from_utf16_lossy(std::slice::from_raw_parts(line.FileName, n));
                        let short: String = file
                            .rsplit(['\\', '/'])
                            .take(3)
                            .collect::<Vec<_>>()
                            .into_iter()
                            .rev()
                            .collect::<Vec<_>>()
                            .join("/");
                        format!(" ({short}:{})", line.LineNumber)
                    } else {
                        String::new()
                    }
                };
                format!("{}+0x{displacement:x}{place}", super::demangle(&name))
            })
            .collect()
    }

    pub(super) fn memory_text() -> String {
        let mut out = String::new();
        unsafe {
            let mut pmc: PROCESS_MEMORY_COUNTERS_EX = std::mem::zeroed();
            pmc.cb = std::mem::size_of::<PROCESS_MEMORY_COUNTERS_EX>() as u32;
            if K32GetProcessMemoryInfo(
                GetCurrentProcess(),
                (&mut pmc as *mut PROCESS_MEMORY_COUNTERS_EX).cast::<PROCESS_MEMORY_COUNTERS>(),
                pmc.cb,
            ) != 0
            {
                out.push_str(&format!(
                    "process {} MB in RAM (peak {} MB), {} MB committed",
                    super::mb(pmc.WorkingSetSize as u64),
                    super::mb(pmc.PeakWorkingSetSize as u64),
                    super::mb(pmc.PrivateUsage as u64)
                ));
            }
            let mut ms: MEMORYSTATUSEX = std::mem::zeroed();
            ms.dwLength = std::mem::size_of::<MEMORYSTATUSEX>() as u32;
            if GlobalMemoryStatusEx(&mut ms) != 0 {
                out.push_str(&format!(
                    "; system {} of {} MB RAM free, {} of {} MB commit free",
                    super::mb(ms.ullAvailPhys),
                    super::mb(ms.ullTotalPhys),
                    super::mb(ms.ullAvailPageFile),
                    super::mb(ms.ullTotalPageFile)
                ));
            }
        }
        out
    }

    #[repr(C, align(16))]
    #[derive(Clone, Copy)]
    struct Context(windows_sys::Win32::System::Diagnostics::Debug::CONTEXT);

    #[cfg(target_arch = "x86_64")]
    fn walk(mut ctx: Context, low: usize, high: usize, out: &mut [usize; super::MAX_FRAMES]) -> usize {
        use windows_sys::Win32::System::Diagnostics::Debug::{
            RtlLookupFunctionEntry, RtlVirtualUnwind, UNW_FLAG_NHANDLER,
        };
        let mut n = 0;
        while n < out.len() {
            let pc = ctx.0.Rip;
            if pc == 0 {
                break;
            }
            out[n] = pc as usize;
            n += 1;
            let rsp = ctx.0.Rsp as usize;
            if rsp < low || rsp + 8 > high {
                break;
            }
            let mut base = 0u64;
            let entry = unsafe { RtlLookupFunctionEntry(pc, &mut base, std::ptr::null_mut()) };
            if entry.is_null() {
                ctx.0.Rip = unsafe { std::ptr::read(rsp as *const u64) };
                ctx.0.Rsp += 8;
            } else {
                let mut data = std::ptr::null_mut();
                let mut frame = 0u64;
                unsafe {
                    RtlVirtualUnwind(
                        UNW_FLAG_NHANDLER,
                        base,
                        pc,
                        entry,
                        &mut ctx.0,
                        &mut data,
                        &mut frame,
                        std::ptr::null_mut(),
                    );
                }
            }
            if (ctx.0.Rsp as usize) <= rsp {
                break;
            }
        }
        n
    }

    #[cfg(target_arch = "x86_64")]
    fn own_limits() -> (usize, usize) {
        let (mut low, mut high) = (0usize, 0usize);
        unsafe { GetCurrentThreadStackLimits(&mut low, &mut high) };
        (low, high)
    }

    #[cfg(target_arch = "x86_64")]
    pub(super) fn current_stack() -> Vec<usize> {
        use windows_sys::Win32::System::Diagnostics::Debug::RtlCaptureContext;
        let mut ctx: Context = unsafe { std::mem::zeroed() };
        unsafe { RtlCaptureContext(&mut ctx.0) };
        let (low, high) = own_limits();
        let mut frames = [0usize; super::MAX_FRAMES];
        let n = walk(ctx, low, high, &mut frames);
        frames[..n].iter().skip(2).copied().collect()
    }

    #[cfg(target_arch = "x86_64")]
    pub(super) fn main_thread_stack() -> Vec<usize> {
        use windows_sys::Win32::System::Diagnostics::Debug::{CONTEXT_FULL_AMD64, GetThreadContext};
        use windows_sys::Win32::System::Threading::{ResumeThread, SuspendThread};
        const COPY: usize = 1 << 20;
        let thread = MAIN_THREAD.load(Ordering::Acquire) as HANDLE;
        let high = MAIN_STACK_HIGH.load(Ordering::Acquire);
        if thread.is_null() || high == 0 {
            return Vec::new();
        }
        let mut copy = vec![0u8; COPY];
        let mut ctx: Context = unsafe { std::mem::zeroed() };
        ctx.0.ContextFlags = CONTEXT_FULL_AMD64;
        let copied;
        unsafe {
            if SuspendThread(thread) == u32::MAX {
                return Vec::new();
            }
            if GetThreadContext(thread, &mut ctx.0) == 0 {
                ResumeThread(thread);
                return Vec::new();
            }
            let rsp = ctx.0.Rsp as usize;
            if rsp == 0 || rsp >= high {
                ResumeThread(thread);
                return Vec::new();
            }
            copied = (high - rsp).min(COPY);
            std::ptr::copy_nonoverlapping(rsp as *const u8, copy.as_mut_ptr(), copied);
            ResumeThread(thread);
        }
        let orig_low = ctx.0.Rsp as usize;
        let orig_high = orig_low + copied;
        let base = copy.as_ptr() as usize;
        let shift = |v: u64| -> u64 {
            let v = v as usize;
            if v >= orig_low && v < orig_high { (v - orig_low + base) as u64 } else { v as u64 }
        };
        let words = copied / 8;
        let slots = unsafe { std::slice::from_raw_parts_mut(copy.as_mut_ptr().cast::<u64>(), words) };
        for w in slots.iter_mut() {
            *w = shift(*w);
        }
        ctx.0.Rsp = shift(ctx.0.Rsp);
        ctx.0.Rbp = shift(ctx.0.Rbp);
        ctx.0.Rbx = shift(ctx.0.Rbx);
        ctx.0.Rsi = shift(ctx.0.Rsi);
        ctx.0.Rdi = shift(ctx.0.Rdi);
        ctx.0.R12 = shift(ctx.0.R12);
        ctx.0.R13 = shift(ctx.0.R13);
        ctx.0.R14 = shift(ctx.0.R14);
        ctx.0.R15 = shift(ctx.0.R15);
        let mut frames = [0usize; super::MAX_FRAMES];
        let n = walk(ctx, base, base + words * 8, &mut frames);
        frames[..n].to_vec()
    }

    struct Found(HWND);

    unsafe extern "system" fn pick(hwnd: HWND, lparam: LPARAM) -> i32 {
        let found = unsafe { &mut *(lparam as *mut Found) };
        if unsafe { IsWindowVisible(hwnd) } != 0 {
            found.0 = hwnd;
            return 0;
        }
        1
    }

    pub(super) fn main_window_answers() -> Option<bool> {
        let id = MAIN_THREAD_ID.load(Ordering::Acquire);
        if id == 0 {
            return None;
        }
        let mut found = Found(std::ptr::null_mut());
        unsafe { EnumThreadWindows(id, Some(pick), &mut found as *mut Found as LPARAM) };
        if found.0.is_null() {
            return None;
        }
        let mut result = 0usize;
        let ok = unsafe { SendMessageTimeoutW(found.0, WM_NULL, 0, 0, SMTO_ABORTIFHUNG, 1000, &mut result) };
        Some(ok != 0)
    }

    pub(super) fn write_minidump(dir: &Path, target: super::DumpTarget) -> Result<PathBuf, String> {
        use std::os::windows::io::AsRawHandle;
        let stamp =
            std::time::SystemTime::now().duration_since(std::time::UNIX_EPOCH).map_err(|e| e.to_string())?.as_millis();
        let path = dir.join(format!("crash-{stamp}.dmp"));
        prune_dumps(dir);
        let _ = std::fs::create_dir_all(dir);
        let file = std::fs::File::create(&path).map_err(|e| format!("{}: {e}", path.display()))?;
        let info = MINIDUMP_EXCEPTION_INFORMATION {
            ThreadId: target.thread_id,
            ExceptionPointers: target.pointers as *mut EXCEPTION_POINTERS,
            ClientPointers: 0,
        };
        let ok = unsafe {
            MiniDumpWriteDump(
                GetCurrentProcess(),
                GetCurrentProcessId(),
                file.as_raw_handle() as HANDLE,
                MiniDumpNormal | MiniDumpWithThreadInfo | MiniDumpWithUnloadedModules,
                &info,
                std::ptr::null(),
                std::ptr::null(),
            )
        };
        let error = std::io::Error::last_os_error();
        drop(file);
        if ok == 0 {
            let _ = std::fs::remove_file(&path);
            return Err(error.to_string());
        }
        Ok(path)
    }

    fn prune_dumps(dir: &Path) {
        let Ok(entries) = std::fs::read_dir(dir) else { return };
        let mut dumps: Vec<PathBuf> = entries
            .filter_map(Result::ok)
            .map(|e| e.path())
            .filter(|p| p.extension().is_some_and(|e| e == "dmp"))
            .collect();
        dumps.sort();
        let excess = dumps.len().saturating_sub(2);
        for old in dumps.into_iter().take(excess) {
            let _ = std::fs::remove_file(old);
        }
    }

    fn describe(record: &windows_sys::Win32::System::Diagnostics::Debug::EXCEPTION_RECORD) -> String {
        let code = record.ExceptionCode;
        let address = record.ExceptionAddress as usize;
        let mut message = format!(
            "native crash: {} (exception 0x{:08X}) at {} (address 0x{address:x})",
            code_name(code),
            code as u32,
            module_of(address)
        );
        if (code == EXCEPTION_ACCESS_VIOLATION || code == EXCEPTION_IN_PAGE_ERROR) && record.NumberParameters >= 2 {
            let kind = match record.ExceptionInformation[0] {
                0 => "reading",
                1 => "writing",
                8 => "executing",
                _ => "using",
            };
            message.push_str(&format!(", {kind} address 0x{:x}", record.ExceptionInformation[1]));
        }
        message
    }

    unsafe fn report(info: *const EXCEPTION_POINTERS) {
        let Some(pointers) = (unsafe { info.as_ref() }) else { return };
        let Some(record) = (unsafe { pointers.ExceptionRecord.as_ref() }) else { return };
        let mut frames = [0usize; super::MAX_FRAMES];
        let mut n = 0;
        #[cfg(target_arch = "x86_64")]
        if let Some(ctx) = unsafe { pointers.ContextRecord.as_ref() } {
            let (low, high) = own_limits();
            let mut copy: Context = unsafe { std::mem::zeroed() };
            copy.0 = *ctx;
            n = walk(copy, low, high, &mut frames);
        }
        if n == 0 {
            frames[0] = record.ExceptionAddress as usize;
            n = 1;
        }
        let dump = super::DumpTarget { thread_id: unsafe { GetCurrentThreadId() }, pointers: info as usize };
        let message = describe(record);
        super::fatal(&message, super::Native { frames: frames[..n].to_vec(), dump: Some(dump) });
    }

    unsafe extern "system" fn filter(info: *const EXCEPTION_POINTERS) -> i32 {
        unsafe { report(info) };
        EXCEPTION_CONTINUE_SEARCH
    }

    unsafe extern "system" fn vectored(info: *mut EXCEPTION_POINTERS) -> i32 {
        let code = unsafe { info.as_ref().and_then(|i| i.ExceptionRecord.as_ref()).map(|r| r.ExceptionCode) };
        if code.is_some_and(is_fatal) {
            unsafe { report(info) };
        }
        EXCEPTION_CONTINUE_SEARCH
    }

    fn ours() -> LPTOP_LEVEL_EXCEPTION_FILTER {
        Some(filter)
    }

    pub(super) fn keep_filter() {
        let previous = unsafe { SetUnhandledExceptionFilter(ours()) };
        if previous.map(|f| f as usize) != ours().map(|f| f as usize) && previous.is_some() {
            log::warn!("a library replaced the crash handler, it was put back");
        }
    }

    pub(super) fn install() {
        unsafe {
            SetUnhandledExceptionFilter(ours());
            AddVectoredExceptionHandler(0, Some(vectored));
            let mut handle: HANDLE = std::ptr::null_mut();
            let process = GetCurrentProcess();
            if DuplicateHandle(process, GetCurrentThread(), process, &mut handle, 0, 0, DUPLICATE_SAME_ACCESS) != 0 {
                MAIN_THREAD.store(handle as usize, Ordering::Release);
            }
            MAIN_THREAD_ID.store(GetCurrentThreadId(), Ordering::Release);
            let (mut low, mut high) = (0usize, 0usize);
            GetCurrentThreadStackLimits(&mut low, &mut high);
            MAIN_STACK_HIGH.store(high, Ordering::Release);
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn a_refused_allocation_uses_the_reserve_then_fails_without_aborting() {
        take_reserve();
        assert!(has_reserve());
        assert!(release_reserve());
        assert!(!release_reserve());
        take_reserve();
        assert!(has_reserve());
        let layout = Layout::from_size_align(1 << 60, 8).unwrap();
        let p = unsafe { GuardedAlloc.alloc(layout) };
        assert!(p.is_null());
        assert!(!has_reserve(), "the reserve was given back to try again");
        assert!(take_low_memory());
        assert!(!take_low_memory());
        IN_REPORT.store(false, Ordering::Release);
        take_reserve();
        let small = Layout::from_size_align(64, 8).unwrap();
        let q = unsafe { GuardedAlloc.alloc_zeroed(small) };
        assert!(!q.is_null());
        let q = unsafe { GuardedAlloc.realloc(q, small, 128) };
        assert!(!q.is_null());
        unsafe { GuardedAlloc.dealloc(q, Layout::from_size_align(128, 8).unwrap()) };
        assert!(!emergency_wanted());
    }

    #[test]
    fn the_state_text_has_memory_activity_and_map() {
        set_context("map big.vmf (120000 solids, 9000 entities, 73.2 MB), render mode Lit");
        set_activity("opening big.vmf");
        set_vram(3 << 30, 6 << 30);
        let text = state_text();
        assert!(text.contains("120000 solids") && text.contains("opening big.vmf"), "{text}");
        assert!(text.contains("graphics memory: 3072 MB used, budget 6144 MB"), "{text}");
        assert!(text.contains("memory: "), "{text}");
        set_activity("");
        assert!(state_text().contains("activity: editing"));
    }

    #[cfg(all(windows, target_arch = "x86_64"))]
    #[test]
    fn stacks_are_walked_and_named() {
        let frames = current_stack();
        assert!(frames.len() >= 3, "{frames:?}");
        let text = frames_text(&frames);
        assert!(text.contains("hammerpro") && text.contains("+0x"), "{text}");
        assert!(text.contains("RtlUserThreadStart") || text.contains("BaseThreadInitThunk"), "{text}");
    }

    const NATIVE_CHILD_DIR: &str = "HP_NATIVE_CRASH_CHILD_DIR";
    const NATIVE_CHILD_KIND: &str = "HP_NATIVE_CRASH_CHILD_KIND";

    #[inline(never)]
    fn recurse_forever(depth: u64) -> u64 {
        let pad = std::hint::black_box([depth; 64]);
        if std::hint::black_box(depth) == u64::MAX {
            return pad[0];
        }
        recurse_forever(depth + 1) + pad[3]
    }

    #[test]
    #[ignore = "run in a child process by native_crashes_leave_a_full_report"]
    fn native_crash_child() {
        let Some(dir) = std::env::var_os(NATIVE_CHILD_DIR) else { return };
        install(PathBuf::from(dir));
        set_context("map child.vmf (12 solids, 3 entities, 0 selected)");
        set_activity("testing a native crash");
        match std::env::var(NATIVE_CHILD_KIND).as_deref() {
            Ok("stack") => {
                std::hint::black_box(recurse_forever(0));
            }
            _ => unsafe {
                let p = std::hint::black_box(16usize) as *mut u32;
                std::ptr::write_volatile(p, 7);
            },
        }
    }

    #[cfg(all(windows, target_arch = "x86_64"))]
    #[test]
    fn native_crashes_leave_a_full_report() {
        for (kind, expected) in [("access", "access violation"), ("stack", "stack overflow")] {
            let dir = std::env::temp_dir().join(format!("hammerpro_native_{kind}_{}", std::process::id()));
            let _ = std::fs::remove_dir_all(&dir);
            let out = std::process::Command::new(std::env::current_exe().unwrap())
                .args([
                    "--exact",
                    "native_crash::tests::native_crash_child",
                    "--ignored",
                    "--nocapture",
                    "--test-threads",
                    "1",
                ])
                .env(NATIVE_CHILD_DIR, &dir)
                .env(NATIVE_CHILD_KIND, kind)
                .stdin(std::process::Stdio::null())
                .output()
                .unwrap();
            assert!(!out.status.success(), "the child crashed");
            let files: Vec<PathBuf> =
                std::fs::read_dir(&dir).unwrap().filter_map(Result::ok).map(|e| e.path()).collect();
            let report = files
                .iter()
                .find(|p| p.extension().is_some_and(|e| e == "txt") && p.to_string_lossy().contains("crash-"))
                .and_then(|p| std::fs::read_to_string(p).ok())
                .unwrap_or_else(|| panic!("{kind}: no report in {files:?}: {}", String::from_utf8_lossy(&out.stderr)));
            assert!(report.contains(expected), "{kind}: {report}");
            assert!(report.contains("hammerpro") && report.contains("+0x"), "{kind}: {report}");
            assert!(report.contains("map child.vmf") && report.contains("testing a native crash"), "{kind}: {report}");
            assert!(report.contains("memory: process"), "{kind}: {report}");
            if kind == "access" {
                assert!(report.contains("writing address 0x10"), "{report}");
                assert!(
                    files.iter().any(|p| p.extension().is_some_and(|e| e == "dmp")),
                    "a minidump: {files:?} {report}"
                );
            } else {
                assert!(report.contains("recurse_forever"), "{report}");
            }
            let _ = std::fs::remove_dir_all(&dir);
        }
    }

    #[test]
    fn rust_names_are_demangled() {
        assert_eq!(
            demangle("ZN8wgpu_hal4dx128D3D12Lib13create_device17h7ebe125e28067302E"),
            "wgpu_hal::dx12::D3D12Lib::create_device"
        );
        assert_eq!(
            demangle(
                "_ZN4core3ptr85drop_in_place$LT$std..rt..lang_start$LT$$LP$$RP$$GT$..$u7b$$u7b$closure$u7d$$u7d$$GT$17h6c3a1c2f1e2d3b4aE.llvm.123"
            ),
            "core::ptr::drop_in_place<std::rt::lang_start<()>::{{closure}}>.llvm.123"
        );
        assert_eq!(demangle("RtlUserThreadStart"), "RtlUserThreadStart");
    }

    #[test]
    fn windows_events_are_matched_by_process() {
        let crash = "<System><EventID Qualifiers='0'>1000</EventID></System><EventData><Data>hammerpro.exe</Data>\
            <Data>2.2.4.0</Data><Data>0x0</Data><Data>nvoglv64.dll</Data><Data>31.0.15.1</Data><Data>0x0</Data>\
            <Data>0xc0000005</Data><Data>0x12345</Data><Data>0x3e84</Data><Data>01dd</Data>\
            <Data>C:\\HammerPro\\hammerpro.exe</Data></EventData>";
        let text = event_matches(crash, 0x3e84).expect("matched");
        assert!(text.contains("0xc0000005") && text.contains("nvoglv64.dll") && text.contains("0x12345"), "{text}");
        assert_eq!(event_matches(crash, 0x3e85), None);
        let hang = "<System><EventID Qualifiers='0'>1002</EventID></System><EventData><Data>hammerpro.exe</Data>\
            <Data>2.2.4.0</Data><Data>3e84</Data></EventData>";
        assert!(event_matches(hang, 0x3e84).unwrap().contains("stopped answering"));
    }

    #[test]
    fn blackboxes_are_read_once_then_removed() {
        let dir = std::env::temp_dir().join(format!("hammerpro_blackbox_{}", std::process::id()));
        let _ = std::fs::remove_dir_all(&dir);
        std::fs::create_dir_all(&dir).unwrap();
        std::fs::write(blackbox_path(&dir, 77), "editor: map x.vmf\nwindow not answering for: 9000 ms\n").unwrap();
        assert_eq!(blackbox_pid("blackbox-77.txt"), Some(77));
        let text = previous_run_text(&dir, 77);
        assert!(text.contains("map x.vmf") && text.contains("frozen when the run ended"), "{text}");
        assert!(!blackbox_path(&dir, 77).exists());
        std::fs::write(blackbox_path(&dir, 78), "x").unwrap();
        prune_blackboxes(&dir, &|_| false);
        assert!(!blackbox_path(&dir, 78).exists());
        let _ = std::fs::remove_dir_all(&dir);
    }
}
