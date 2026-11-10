use std::sync::OnceLock;

fn pool() -> Option<&'static rayon::ThreadPool> {
    static POOL: OnceLock<Option<rayon::ThreadPool>> = OnceLock::new();
    POOL.get_or_init(|| {
        let threads = (std::thread::available_parallelism().map_or(4, std::num::NonZeroUsize::get) / 2).clamp(2, 8);
        rayon::ThreadPoolBuilder::new()
            .num_threads(threads)
            .thread_name(|i| format!("hp-render-{i}"))
            .build()
            .map_err(|e| log::warn!("hp-render: no worker pool of its own ({e}), the shared one is used"))
            .ok()
    })
    .as_ref()
}

pub fn run<R: Send>(f: impl FnOnce() -> R + Send) -> R {
    match pool() {
        Some(p) if rayon::current_thread_index().is_none() || p.current_thread_index().is_some() => p.install(f),
        _ => f(),
    }
}

#[cfg(test)]
mod tests {
    use std::time::{Duration, Instant};

    use rayon::prelude::*;

    #[test]
    fn work_runs_even_while_the_shared_pool_is_busy() {
        for _ in 0..rayon::current_num_threads() {
            rayon::spawn(|| std::thread::sleep(Duration::from_millis(600)));
        }
        let start = Instant::now();
        let sum: u64 = super::run(|| (0..200_000u64).into_par_iter().map(|v| v % 7).sum());
        assert_eq!(sum, (0..200_000u64).map(|v| v % 7).sum::<u64>());
        assert!(start.elapsed() < Duration::from_millis(500), "{:?}", start.elapsed());
    }
}
