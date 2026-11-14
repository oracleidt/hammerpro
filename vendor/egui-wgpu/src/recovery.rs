use std::cell::Cell;
use std::sync::Mutex;
use std::sync::atomic::{AtomicBool, AtomicU64, Ordering};

use crate::RenderState;

static LOST: AtomicBool = AtomicBool::new(false);
static STAGING_FAILURES: AtomicU64 = AtomicU64::new(0);
static RECREATIONS: AtomicU64 = AtomicU64::new(0);
static FAILED_ATTEMPTS: AtomicU64 = AtomicU64::new(0);
static RECREATED: Mutex<Option<RenderState>> = Mutex::new(None);

type ShadowFilter = Box<dyn Fn(epaint::TextureId) -> bool + Send + Sync>;

static SHADOW_FILTER: std::sync::RwLock<Option<ShadowFilter>> = std::sync::RwLock::new(None);

pub fn set_shadow_filter(filter: impl Fn(epaint::TextureId) -> bool + Send + Sync + 'static) {
    *SHADOW_FILTER
        .write()
        .unwrap_or_else(std::sync::PoisonError::into_inner) = Some(Box::new(filter));
}

pub(crate) fn keep_shadow(id: epaint::TextureId) -> bool {
    SHADOW_FILTER
        .read()
        .unwrap_or_else(std::sync::PoisonError::into_inner)
        .as_ref()
        .is_none_or(|f| f(id))
}

thread_local! {
    static GUARDED: Cell<bool> = const { Cell::new(false) };
}

pub fn notify_device_lost() {
    LOST.store(true, Ordering::SeqCst);
}

pub fn device_lost_pending() -> bool {
    LOST.load(Ordering::SeqCst)
}

pub(crate) fn take_lost() -> bool {
    LOST.swap(false, Ordering::SeqCst)
}

pub(crate) fn note_staging_failure() {
    STAGING_FAILURES.fetch_add(1, Ordering::Relaxed);
}

pub fn staging_failures() -> u64 {
    STAGING_FAILURES.load(Ordering::Relaxed)
}

pub fn recreations() -> u64 {
    RECREATIONS.load(Ordering::Relaxed)
}

pub fn failed_attempts() -> u64 {
    FAILED_ATTEMPTS.load(Ordering::Relaxed)
}

pub(crate) fn note_failed_attempt() {
    FAILED_ATTEMPTS.fetch_add(1, Ordering::Relaxed);
}

pub(crate) fn publish(state: RenderState) {
    RECREATIONS.fetch_add(1, Ordering::Relaxed);
    FAILED_ATTEMPTS.store(0, Ordering::Relaxed);
    *RECREATED
        .lock()
        .unwrap_or_else(std::sync::PoisonError::into_inner) = Some(state);
}

pub fn take_recreated() -> Option<RenderState> {
    RECREATED
        .lock()
        .unwrap_or_else(std::sync::PoisonError::into_inner)
        .take()
}

pub fn in_guarded_paint() -> bool {
    GUARDED.with(Cell::get)
}

pub(crate) struct Guard(bool);

impl Guard {
    pub(crate) fn enter() -> Self {
        Self(GUARDED.with(|g| g.replace(true)))
    }
}

impl Drop for Guard {
    fn drop(&mut self) {
        let previous = self.0;
        GUARDED.with(|g| g.set(previous));
    }
}

pub(crate) fn block_on<F: std::future::Future>(future: F) -> F::Output {
    let mut future = std::pin::pin!(future);
    let mut cx = std::task::Context::from_waker(std::task::Waker::noop());
    loop {
        if let std::task::Poll::Ready(out) = future.as_mut().poll(&mut cx) {
            return out;
        }
        std::thread::yield_now();
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn a_loss_is_taken_once() {
        notify_device_lost();
        assert!(device_lost_pending());
        assert!(take_lost());
        assert!(!take_lost());
        assert!(!in_guarded_paint());
        {
            let _g = Guard::enter();
            assert!(in_guarded_paint());
        }
        assert!(!in_guarded_paint());
        assert_eq!(block_on(async { 7 }), 7);
    }
}
