use std::sync::OnceLock;

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Level {
    BelowNormal,
    Idle,
}

pub fn cores() -> usize {
    std::thread::available_parallelism().map_or(4, std::num::NonZeroUsize::get)
}

pub fn background_threads_for(cores: usize) -> usize {
    (cores / 2).min(cores.saturating_sub(2)).max(1)
}

pub fn background_threads() -> usize {
    background_threads_for(cores())
}

pub fn foreground_threads_for(cores: usize) -> usize {
    cores.saturating_sub(2).max(1)
}

pub fn foreground_threads() -> usize {
    foreground_threads_for(cores())
}

pub fn lower_current_thread(level: Level) {
    #[cfg(windows)]
    unsafe {
        use windows_sys::Win32::System::Threading::{
            GetCurrentThread, SetThreadPriority, THREAD_PRIORITY_BELOW_NORMAL, THREAD_PRIORITY_IDLE,
        };
        let p = match level {
            Level::BelowNormal => THREAD_PRIORITY_BELOW_NORMAL,
            Level::Idle => THREAD_PRIORITY_IDLE,
        };
        SetThreadPriority(GetCurrentThread(), p);
    }
    #[cfg(not(windows))]
    let _ = level;
}

pub fn builder(name: &'static str, threads: usize, level: Level) -> rayon::ThreadPoolBuilder {
    rayon::ThreadPoolBuilder::new()
        .num_threads(threads.max(1))
        .thread_name(move |i| format!("{name}-{i}"))
        .start_handler(move |_| lower_current_thread(level))
}

pub fn pool(name: &'static str, threads: usize, level: Level) -> Option<rayon::ThreadPool> {
    builder(name, threads, level).build().map_err(|e| log::warn!("cannot start the {name} threads: {e}")).ok()
}

pub fn configure_global() {
    static DONE: OnceLock<bool> = OnceLock::new();
    DONE.get_or_init(|| {
        builder("hp-shared", foreground_threads(), Level::BelowNormal)
            .stack_size(8 << 20)
            .build_global()
            .map_err(|e| log::warn!("the shared thread pool was already set up: {e}"))
            .is_ok()
    });
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn background_work_leaves_cores_free() {
        assert_eq!(background_threads_for(1), 1);
        assert_eq!(background_threads_for(2), 1);
        assert_eq!(background_threads_for(4), 2);
        assert_eq!(background_threads_for(6), 3);
        assert_eq!(background_threads_for(12), 6);
        assert_eq!(background_threads_for(32), 16);
        for c in 3..64 {
            assert!(background_threads_for(c) <= c / 2);
            assert!(background_threads_for(c) + 2 <= c);
        }
        assert_eq!(foreground_threads_for(12), 10);
        assert_eq!(foreground_threads_for(1), 1);
    }

    #[test]
    fn pools_run_work_on_their_own_capped_threads() {
        let p = pool("hp-sched-test", 2, Level::Idle).unwrap();
        assert_eq!(p.current_num_threads(), 2);
        let name = p.install(|| std::thread::current().name().map(str::to_owned));
        assert!(name.is_some_and(|n| n.starts_with("hp-sched-test-")));
    }
}
