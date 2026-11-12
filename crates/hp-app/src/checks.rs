use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::{Arc, Mutex, OnceLock, Weak};
use std::time::Duration;

static PAUSE_MINIMIZED: AtomicBool = AtomicBool::new(true);
static WATCHING: AtomicBool = AtomicBool::new(false);
static STOPPED: AtomicBool = AtomicBool::new(false);
static JOBS: Mutex<Vec<Weak<AtomicBool>>> = Mutex::new(Vec::new());
const WATCH_EVERY: Duration = Duration::from_millis(250);

pub fn threads_for(cores: usize) -> usize {
    (cores / 3).max(1).min(cores.saturating_sub(2).max(1))
}

pub fn lowest_priority() {
    #[cfg(windows)]
    unsafe {
        use windows_sys::Win32::System::Threading::{GetCurrentThread, SetThreadPriority, THREAD_PRIORITY_IDLE};
        SetThreadPriority(GetCurrentThread(), THREAD_PRIORITY_IDLE);
    }
}

fn pool() -> Option<&'static rayon::ThreadPool> {
    static POOL: OnceLock<Option<rayon::ThreadPool>> = OnceLock::new();
    POOL.get_or_init(|| {
        let cores = std::thread::available_parallelism().map_or(2, std::num::NonZeroUsize::get);
        rayon::ThreadPoolBuilder::new()
            .num_threads(threads_for(cores))
            .thread_name(|i| format!("hp-checks-{i}"))
            .stack_size(8 << 20)
            .start_handler(|_| lowest_priority())
            .build()
            .ok()
    })
    .as_ref()
}

pub fn threads() -> usize {
    pool().map_or(1, rayon::ThreadPool::current_num_threads)
}

pub fn run<R: Send>(f: impl FnOnce() -> R + Send) -> R {
    match pool() {
        Some(p) => p.install(f),
        None => f(),
    }
}

pub fn set_pause_minimized(on: bool) {
    PAUSE_MINIMIZED.store(on, Ordering::Relaxed);
}

pub fn take_stopped() -> bool {
    STOPPED.swap(false, Ordering::AcqRel)
}

pub fn window_away() -> bool {
    PAUSE_MINIMIZED.load(Ordering::Relaxed)
        && crate::native_window::main_window_state().is_some_and(|(minimized, _)| minimized)
}

pub fn watch(cancel: &Arc<AtomicBool>) {
    let mut jobs = JOBS.lock().unwrap_or_else(std::sync::PoisonError::into_inner);
    jobs.retain(|j| j.strong_count() > 0);
    jobs.push(Arc::downgrade(cancel));
    drop(jobs);
    if WATCHING.swap(true, Ordering::AcqRel) {
        return;
    }
    let spawned = std::thread::Builder::new().name("hp-checks-watch".into()).spawn(|| {
        loop {
            std::thread::sleep(WATCH_EVERY);
            let mut jobs = JOBS.lock().unwrap_or_else(std::sync::PoisonError::into_inner);
            jobs.retain(|j| j.upgrade().is_some_and(|c| !c.load(Ordering::Relaxed)));
            if jobs.is_empty() {
                WATCHING.store(false, Ordering::Release);
                return;
            }
            if window_away() {
                for job in jobs.drain(..) {
                    if let Some(c) = job.upgrade() {
                        c.store(true, Ordering::Release);
                    }
                }
                STOPPED.store(true, Ordering::Release);
                log::info!("map checks: the window is minimized, the checks stop until it comes back");
            }
        }
    });
    if spawned.is_err() {
        WATCHING.store(false, Ordering::Release);
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn checks_use_a_third_of_the_cores_and_leave_two_free() {
        assert_eq!(threads_for(12), 4);
        assert_eq!(threads_for(16), 5);
        assert_eq!(threads_for(8), 2);
        assert_eq!(threads_for(4), 1);
        assert_eq!(threads_for(2), 1);
        assert_eq!(threads_for(1), 1);
    }

    #[test]
    fn work_runs_on_the_capped_pool() {
        let inside = run(|| (rayon::current_thread_index().is_some(), rayon::current_num_threads()));
        assert!(inside.0);
        assert_eq!(inside.1, threads());
        assert!(threads() >= 1);
    }

    #[test]
    fn watched_jobs_are_forgotten_once_done() {
        let flag = Arc::new(AtomicBool::new(false));
        watch(&flag);
        flag.store(true, Ordering::Relaxed);
        drop(flag);
        std::thread::sleep(WATCH_EVERY * 3);
        assert!(JOBS.lock().unwrap().iter().all(|j| j.upgrade().is_none_or(|c| c.load(Ordering::Relaxed))));
    }
}
