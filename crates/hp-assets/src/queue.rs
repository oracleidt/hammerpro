use std::collections::VecDeque;
use std::sync::Arc;
use std::sync::atomic::{AtomicBool, AtomicU8, AtomicU64, AtomicUsize, Ordering};
use std::thread::JoinHandle;

use ahash::{AHashMap, AHashSet};
use crossbeam_channel::Sender;
use parking_lot::{Condvar, Mutex, RwLock};

use crate::jobs::{Job, JobResult};
use crate::texture::TexFailure;

pub(crate) const THUMB_STALE_FRAMES: u64 = 3;
pub(crate) const INFLIGHT_TEXTURE_BYTES: u64 = 96 << 20;
const INFLIGHT_WAIT: std::time::Duration = std::time::Duration::from_secs(2);

#[derive(Default)]
pub(crate) struct Inflight {
    bytes: Mutex<u64>,
    cv: Condvar,
}

impl Inflight {
    pub(crate) fn hold(self: &Arc<Self>, bytes: u64) -> Held {
        *self.bytes.lock() += bytes;
        Held { pool: Some(Arc::clone(self)), bytes }
    }

    pub(crate) fn wait_below(&self, cap: u64, shutdown: &AtomicBool) {
        let deadline = std::time::Instant::now() + INFLIGHT_WAIT;
        let mut held = self.bytes.lock();
        while *held >= cap && !shutdown.load(Ordering::Acquire) {
            if self.cv.wait_until(&mut held, deadline).timed_out() {
                return;
            }
        }
    }
}

pub(crate) struct Held {
    pool: Option<Arc<Inflight>>,
    bytes: u64,
}

impl Drop for Held {
    fn drop(&mut self) {
        if let Some(pool) = self.pool.take() {
            let mut held = pool.bytes.lock();
            *held = held.saturating_sub(self.bytes);
            drop(held);
            pool.cv.notify_all();
        }
    }
}

#[derive(Clone)]
pub(crate) struct FsEnv {
    pub fs: hp_fs::GameFs,
    pub epoch: u64,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(crate) enum Lane {
    High,
    Thumb,
    Low,
}

#[derive(Clone, Copy, Debug, PartialEq)]
pub(crate) enum BoundsState {
    Pending,
    Ready { min: [f32; 3], max: [f32; 3] },
    Failed,
}

#[derive(Default)]
struct Queues {
    high: VecDeque<Job>,
    thumbs: Vec<Job>,
    low: VecDeque<Job>,
}

impl Queues {
    fn len(&self) -> usize {
        self.high.len() + self.thumbs.len() + self.low.len()
    }

    fn pop(&mut self) -> Option<Job> {
        self.high.pop_front().or_else(|| self.thumbs.pop()).or_else(|| self.low.pop_front())
    }
}

pub(crate) const BC_UNKNOWN: u8 = 0;
pub(crate) const BC_YES: u8 = 1;
pub(crate) const BC_NO: u8 = 2;

pub(crate) struct Shared {
    queues: Mutex<Queues>,
    cv: Condvar,
    pub frame: AtomicU64,
    pub shutdown: AtomicBool,
    ctx: Mutex<Option<egui::Context>>,
    pub tx: Sender<JobResult>,
    pub env: RwLock<Option<FsEnv>>,
    pub bounds: RwLock<AHashMap<String, BoundsState>>,
    pub bc: AtomicU8,
    pub running: AtomicUsize,
    broken: Mutex<(u64, AHashSet<String>)>,
    pub thumb_cache: RwLock<Option<Arc<crate::thumb_cache::ThumbDiskCache>>>,
    pub inflight: Arc<Inflight>,
}

impl Shared {
    pub(crate) fn new(tx: Sender<JobResult>) -> Self {
        Self {
            queues: Mutex::new(Queues::default()),
            cv: Condvar::new(),
            frame: AtomicU64::new(1),
            shutdown: AtomicBool::new(false),
            ctx: Mutex::new(None),
            tx,
            env: RwLock::new(None),
            bounds: RwLock::new(AHashMap::new()),
            bc: AtomicU8::new(BC_UNKNOWN),
            running: AtomicUsize::new(0),
            broken: Mutex::new((0, AHashSet::new())),
            thumb_cache: RwLock::new(None),
            inflight: Arc::default(),
        }
    }

    pub(crate) fn report_texture(&self, epoch: u64, failure: TexFailure) {
        let TexFailure::Broken { texture, reason } = failure else { return };
        let texture = hp_vmt::normalize_texture_path(&texture);
        let first = {
            let mut broken = self.broken.lock();
            if broken.0 != epoch {
                *broken = (epoch, AHashSet::new());
            }
            broken.1.insert(texture.clone())
        };
        if first {
            log::warn!("{}", hp_app::tf!("assets.texture_broken", texture, reason));
        }
    }

    #[cfg(test)]
    pub(crate) fn broken_count(&self, epoch: u64) -> usize {
        let broken = self.broken.lock();
        if broken.0 == epoch { broken.1.len() } else { 0 }
    }

    pub(crate) fn push(&self, lane: Lane, job: Job) {
        {
            let mut q = self.queues.lock();
            match lane {
                Lane::High => q.high.push_back(job),
                Lane::Thumb => q.thumbs.push(job),
                Lane::Low => q.low.push_back(job),
            }
        }
        self.cv.notify_one();
    }

    pub(crate) fn queued(&self) -> usize {
        let q = self.queues.lock();
        q.len() + self.running.load(Ordering::Acquire)
    }

    pub(crate) fn clear_thumbs(&self) {
        self.queues.lock().thumbs.clear();
    }

    pub(crate) fn env(&self) -> Option<FsEnv> {
        self.env.read().clone()
    }

    pub(crate) fn current_frame(&self) -> u64 {
        self.frame.load(Ordering::Relaxed)
    }

    pub(crate) fn set_ctx(&self, ctx: &egui::Context) {
        let mut slot = self.ctx.lock();
        if slot.as_ref() != Some(ctx) {
            *slot = Some(ctx.clone());
        }
    }

    pub(crate) fn ctx(&self) -> Option<egui::Context> {
        self.ctx.lock().clone()
    }

    pub(crate) fn request_repaint(&self) {
        let ctx = self.ctx.lock().clone();
        if let Some(ctx) = ctx {
            ctx.request_repaint();
        }
    }

    pub(crate) fn send(&self, result: JobResult) {
        if self.tx.send(result).is_ok() {
            self.request_repaint();
        }
    }

    pub(crate) fn stop(&self) {
        self.shutdown.store(true, Ordering::Release);
        self.inflight.cv.notify_all();
        self.queues.lock().thumbs.clear();
        self.cv.notify_all();
    }

    fn next_job(&self) -> Option<Job> {
        let mut q = self.queues.lock();
        loop {
            if self.shutdown.load(Ordering::Acquire) {
                return None;
            }
            if let Some(job) = q.pop() {
                self.running.fetch_add(1, Ordering::AcqRel);
                return Some(job);
            }
            self.cv.wait(&mut q);
        }
    }
}

pub(crate) fn worker_count() -> usize {
    let cores = std::thread::available_parallelism().map_or(4, std::num::NonZeroUsize::get);
    cores.saturating_sub(1).clamp(2, 16)
}

pub(crate) fn spawn_workers(shared: &Arc<Shared>, n: usize) -> Vec<JoinHandle<()>> {
    (0..n)
        .filter_map(|i| {
            let shared = Arc::clone(shared);
            std::thread::Builder::new()
                .name(format!("hp-assets-{i}"))
                .spawn(move || {
                    while let Some(job) = shared.next_job() {
                        let result = crate::jobs::run(job, &shared);
                        let sent = result.is_none_or(|r| shared.tx.send(r).is_ok());
                        shared.running.fetch_sub(1, Ordering::AcqRel);
                        if !sent {
                            return;
                        }
                        shared.request_repaint();
                    }
                })
                .map_err(|e| log::error!("cannot start asset worker: {e}"))
                .ok()
        })
        .collect()
}
