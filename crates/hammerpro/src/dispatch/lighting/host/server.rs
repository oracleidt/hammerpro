use std::io::Write;
use std::sync::atomic::{AtomicU64, Ordering};
use std::sync::{Arc, Mutex};

use super::engine::Engine;
use super::{Reply, Request, read_frame, write_frame};

pub(crate) fn idle_priority() {
    #[cfg(windows)]
    unsafe {
        use windows_sys::Win32::System::Threading::{GetCurrentThread, SetThreadPriority, THREAD_PRIORITY_IDLE};
        SetThreadPriority(GetCurrentThread(), THREAD_PRIORITY_IDLE);
    }
    super::super::memory::lower_thread_memory_priority();
}

pub(crate) fn run(args: &[String]) -> i32 {
    let threads = args.first().and_then(|t| t.parse::<usize>().ok()).filter(|t| *t > 0);
    super::super::memory::lower_process_memory_priority();
    let mut pool = rayon::ThreadPoolBuilder::new().thread_name(|i| format!("hp-light-host-{i}")).stack_size(8 << 20);
    if let Some(n) = threads {
        pool = pool.num_threads(n);
    }
    let _ = pool.build_global();
    let (tx, rx) = crossbeam_channel::unbounded::<(Request, Vec<u8>)>();
    let cancel = Arc::new(AtomicU64::new(0));
    let flag = cancel.clone();
    let latest = Arc::new(AtomicU64::new(0));
    let newest = latest.clone();
    let reader = std::thread::Builder::new().name("hp-light-host-in".to_owned()).spawn(move || {
        let mut input = std::io::BufReader::with_capacity(1 << 20, std::io::stdin().lock());
        loop {
            match read_frame::<_, Request>(&mut input) {
                Ok(Some((Request::Cancel { job }, _))) => {
                    flag.fetch_max(job, Ordering::AcqRel);
                }
                Ok(Some(frame)) => {
                    if let Request::Compile { job, .. } | Request::Relight { job, .. } = &frame.0 {
                        newest.fetch_max(*job, Ordering::AcqRel);
                    }
                    if tx.send(frame).is_err() {
                        break;
                    }
                }
                Ok(None) | Err(_) => break,
            }
        }
    });
    if reader.is_err() {
        return 2;
    }
    let output = Arc::new(Mutex::new(()));
    let out: super::engine::Shared = Arc::new(move |reply: Reply, blob: Vec<u8>| {
        let mut frame = Vec::with_capacity(blob.len() + 256);
        if write_frame(&mut frame, &reply, &blob).is_err() {
            return;
        }
        drop(blob);
        let _guard = output.lock();
        let mut stdout = std::io::stdout().lock();
        if stdout.write_all(&frame).and_then(|()| stdout.flush()).is_err() {
            std::process::exit(3);
        }
    });
    let mut engine = Engine::default();
    let stack = std::thread::Builder::new().name("hp-light-host".to_owned()).stack_size(64 << 20);
    let result = std::thread::scope(|scope| {
        let worker = stack.spawn_scoped(scope, || {
            for (req, blob) in rx {
                let job = match &req {
                    Request::Compile { job, .. }
                    | Request::Geometry { job }
                    | Request::Relight { job, .. }
                    | Request::Models { job, .. }
                    | Request::Costs { job, .. } => *job,
                    _ => 0,
                };
                let background = matches!(req, Request::Models { .. });
                let cancelled = || {
                    job != 0
                        && (cancel.load(Ordering::Acquire) >= job
                            || (background && latest.load(Ordering::Acquire) > job))
                };
                if cancelled() {
                    out(Reply::Failed { job, cancelled: true, message: "cancelled".into() }, Vec::new());
                    continue;
                }
                let name = match &req {
                    Request::Setup { .. } => "setup",
                    Request::Compile { .. } => "compile",
                    Request::Geometry { .. } => "geometry",
                    Request::Relight { .. } => "relight",
                    Request::Models { .. } => "models",
                    Request::Costs { .. } => "costs",
                    Request::Cancel { .. } => "cancel",
                    Request::Release => "release",
                };
                let started = std::time::Instant::now();
                engine.handle(req, blob, &out, &cancelled);
                let line = format!("{name} {job} handled in {} ms", started.elapsed().as_millis());
                out(Reply::Log { line }, Vec::new());
            }
        });
        worker.map(|w| w.join().is_ok())
    });
    let _ = std::io::stdout().flush();
    match result {
        Ok(true) => 0,
        _ => 1,
    }
}
