use std::sync::OnceLock;

fn pool() -> Option<&'static rayon::ThreadPool> {
    static POOL: OnceLock<Option<rayon::ThreadPool>> = OnceLock::new();
    POOL.get_or_init(|| {
        let cores = std::thread::available_parallelism().map_or(2, |n| n.get());
        rayon::ThreadPoolBuilder::new()
            .num_threads(cores.saturating_sub(1).max(1))
            .thread_name(|i| format!("hp-background-{i}"))
            .stack_size(8 << 20)
            .build()
            .ok()
    })
    .as_ref()
}

pub fn run<R: Send>(f: impl FnOnce() -> R + Send) -> R {
    match pool() {
        Some(p) => p.install(f),
        None => f(),
    }
}

pub fn in_background() -> bool {
    pool().is_some_and(|p| p.current_thread_index().is_some())
}

#[cfg(test)]
mod tests {
    use rayon::prelude::*;

    use super::*;

    #[test]
    fn long_background_work_leaves_the_global_pool_free() {
        let (started_tx, started_rx) = std::sync::mpsc::channel();
        let (stop_tx, stop_rx) = std::sync::mpsc::channel::<()>();
        let stop_rx = std::sync::Mutex::new(stop_rx);
        let worker = std::thread::spawn(move || {
            run(|| {
                assert!(in_background());
                let _ = started_tx.send(());
                (0..64).into_par_iter().for_each(|_| {
                    let rx = stop_rx.lock().unwrap_or_else(std::sync::PoisonError::into_inner);
                    let _ = rx.recv_timeout(std::time::Duration::from_secs(20));
                });
            })
        });
        started_rx.recv().unwrap();
        let start = std::time::Instant::now();
        let sum: u64 = (0..10_000u64).into_par_iter().sum();
        assert_eq!(sum, 49_995_000);
        assert!(start.elapsed() < std::time::Duration::from_secs(5), "{:?}", start.elapsed());
        assert!(!in_background());
        drop(stop_tx);
        worker.join().unwrap();
    }
}
