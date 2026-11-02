use std::alloc::{GlobalAlloc, Layout, System};
use std::cell::Cell;
use std::path::PathBuf;
use std::sync::OnceLock;
use std::sync::atomic::{AtomicBool, AtomicPtr, AtomicU8, Ordering};
use std::time::{Duration, Instant};

const RESERVE: usize = 64 << 20;
const RESERVE_ALIGN: usize = 4096;
const SAVE_WAIT: Duration = Duration::from_secs(10);

const IDLE: u8 = 0;
const WANTED: u8 = 1;
const DONE: u8 = 2;

static RESERVE_PTR: AtomicPtr<u8> = AtomicPtr::new(std::ptr::null_mut());
static LOW_MEMORY: AtomicBool = AtomicBool::new(false);
static IN_REPORT: AtomicBool = AtomicBool::new(false);
static EMERGENCY: AtomicU8 = AtomicU8::new(IDLE);
static DIR: OnceLock<PathBuf> = OnceLock::new();

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
    fatal(&format!("memory allocation of {size} bytes failed (the system is out of memory)"));
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

fn fatal(message: &str) {
    if IN_REPORT.swap(true, Ordering::AcqRel) {
        return;
    }
    let message = message.to_owned();
    let thread = thread_label();
    let write = move || {
        let Some(dir) = DIR.get() else { return };
        let text = crate::crash::report_text(
            &message,
            "",
            &thread,
            "(none: the process ends without a panic)",
            &crate::logger::recent_lines_in_hook(),
        );
        if let Ok(path) = crate::crash::write_report(dir, &text) {
            crate::crash::remember_report(&path);
            let _ = std::io::Write::write_fmt(
                &mut std::io::stderr(),
                format_args!("crash report written to {}\n", path.display()),
            );
        }
    };
    if let Ok(h) = std::thread::Builder::new().name("hp-crash-report".into()).stack_size(1 << 20).spawn(write) {
        let _ = h.join();
    }
    wait_for_emergency_save();
}

pub fn install(dir: PathBuf) {
    let _ = DIR.set(dir);
    IS_MAIN.with(|m| m.set(true));
    take_reserve();
    #[cfg(windows)]
    windows::install();
}

#[cfg(windows)]
mod windows {
    use windows_sys::Win32::Foundation::{
        EXCEPTION_ACCESS_VIOLATION, EXCEPTION_ILLEGAL_INSTRUCTION, EXCEPTION_INT_DIVIDE_BY_ZERO,
        EXCEPTION_STACK_OVERFLOW, HMODULE,
    };
    use windows_sys::Win32::System::Diagnostics::Debug::{
        EXCEPTION_CONTINUE_SEARCH, EXCEPTION_POINTERS, SetUnhandledExceptionFilter,
    };
    use windows_sys::Win32::System::LibraryLoader::{
        GET_MODULE_HANDLE_EX_FLAG_FROM_ADDRESS, GET_MODULE_HANDLE_EX_FLAG_UNCHANGED_REFCOUNT, GetModuleFileNameW,
        GetModuleHandleExW,
    };

    fn code_name(code: i32) -> &'static str {
        match code {
            EXCEPTION_ACCESS_VIOLATION => "access violation",
            EXCEPTION_STACK_OVERFLOW => "stack overflow",
            EXCEPTION_ILLEGAL_INSTRUCTION => "illegal instruction",
            EXCEPTION_INT_DIVIDE_BY_ZERO => "division by zero",
            _ => "native exception",
        }
    }

    fn module_of(address: usize) -> String {
        let mut module: HMODULE = std::ptr::null_mut();
        let flags = GET_MODULE_HANDLE_EX_FLAG_FROM_ADDRESS | GET_MODULE_HANDLE_EX_FLAG_UNCHANGED_REFCOUNT;
        if unsafe { GetModuleHandleExW(flags, address as *const u16, &mut module) } == 0 {
            return "unknown module".to_owned();
        }
        let mut buf = [0u16; 512];
        let n = unsafe { GetModuleFileNameW(module, buf.as_mut_ptr(), buf.len() as u32) } as usize;
        let path = String::from_utf16_lossy(&buf[..n.min(buf.len())]);
        let name = path.rsplit(['\\', '/']).next().unwrap_or(&path).to_owned();
        format!("{name}+0x{:x}", address.wrapping_sub(module as usize))
    }

    unsafe extern "system" fn filter(info: *const EXCEPTION_POINTERS) -> i32 {
        let record = unsafe { info.as_ref().and_then(|i| i.ExceptionRecord.as_ref()) };
        let (code, address) = record.map_or((0, 0), |r| (r.ExceptionCode, r.ExceptionAddress as usize));
        let message = format!(
            "native crash: {} (exception 0x{:08X}) at {} (address 0x{address:x})",
            code_name(code),
            code as u32,
            module_of(address)
        );
        super::fatal(&message);
        EXCEPTION_CONTINUE_SEARCH
    }

    pub(super) fn install() {
        unsafe {
            SetUnhandledExceptionFilter(Some(filter));
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
}
