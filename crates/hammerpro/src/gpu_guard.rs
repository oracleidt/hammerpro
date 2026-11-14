use std::sync::Arc;
use std::sync::Mutex;
use std::sync::atomic::{AtomicBool, AtomicU64, Ordering};

const MAX_PENDING: usize = 8;
const LOGGED: u64 = 20;

#[derive(Debug, Default)]
struct Pending {
    errors: Vec<String>,
    count: u64,
    dropped: u64,
    lost: Option<String>,
}

static PENDING: Mutex<Pending> = Mutex::new(Pending { errors: Vec::new(), count: 0, dropped: 0, lost: None });
static GENERATION: AtomicU64 = AtomicU64::new(0);
static SIMULATING: AtomicBool = AtomicBool::new(false);

fn with_pending<T>(f: impl FnOnce(&mut Pending) -> T) -> T {
    let mut p = PENDING.lock().unwrap_or_else(std::sync::PoisonError::into_inner);
    f(&mut p)
}

fn summary(text: &str) -> String {
    let lines: Vec<&str> = text.lines().map(str::trim).filter(|l| !l.is_empty()).take(4).collect();
    let mut s = lines.join(" ");
    if s.chars().count() > 400 {
        s = s.chars().take(400).collect::<String>() + "...";
    }
    s
}

pub fn record_error(text: &str) {
    if hp_render::memory::is_out_of_memory_text(text) {
        hp_render::memory::note_out_of_memory();
    }
    let s = summary(text);
    let first = with_pending(|p| {
        p.count += 1;
        let first = p.count <= LOGGED;
        if p.errors.len() < MAX_PENDING && !p.errors.contains(&s) {
            p.errors.push(s);
        } else {
            p.dropped += 1;
        }
        first
    });
    if first {
        log::error!("graphics error (the frame is skipped): {text}");
    } else {
        log::debug!("graphics error: {text}");
    }
}

pub fn record_lost(reason: &str) {
    log::error!("the graphics device was lost: {reason}");
    egui_wgpu::recovery::notify_device_lost();
    with_pending(|p| {
        if p.lost.is_none() {
            p.lost = Some(reason.to_owned());
        }
    });
}

pub fn take() -> (Vec<String>, u64) {
    with_pending(|p| (std::mem::take(&mut p.errors), std::mem::take(&mut p.dropped)))
}

pub fn take_lost() -> Option<String> {
    with_pending(|p| p.lost.take())
}

fn lost_callback(generation: u64, reason: wgpu::DeviceLostReason, message: &str) {
    if GENERATION.load(Ordering::SeqCst) != generation {
        return;
    }
    let simulated = SIMULATING.swap(false, Ordering::SeqCst);
    if reason == wgpu::DeviceLostReason::Destroyed && !simulated {
        return;
    }
    if simulated {
        record_lost(&format!("simulated for a test: {message}"));
    } else {
        record_lost(&format!("{reason:?}: {message}"));
    }
}

pub fn install(device: &wgpu::Device) {
    let generation = GENERATION.fetch_add(1, Ordering::SeqCst) + 1;
    with_pending(|p| p.lost = None);
    device.on_uncaptured_error(Arc::new(|error: wgpu::Error| record_error(&error.to_string())));
    device.set_device_lost_callback(move |reason, message| lost_callback(generation, reason, &message));
}

pub fn simulate_loss(device: &wgpu::Device) {
    log::warn!("graphics device loss simulated for a test");
    SIMULATING.store(true, Ordering::SeqCst);
    device.destroy();
    if SIMULATING.swap(false, Ordering::SeqCst) {
        record_lost("simulated for a test: the device was destroyed");
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn errors_are_kept_once_and_summarized() {
        let _ = take();
        let long = format!("Validation Error\n\nCaused by:\n  In a pass\n  {}\n  more\n", "x".repeat(600));
        record_error(&long);
        record_error(&long);
        record_error("Out of memory");
        let (errors, dropped) = take();
        assert_eq!(errors.len(), 2, "{errors:?}");
        assert!(errors[0].starts_with("Validation Error Caused by: In a pass"));
        assert!(errors[0].ends_with("..."), "long errors are cut");
        assert_eq!(dropped, 1, "the repeated error is counted, not kept");
        let _ = take_lost();
        record_lost("Unknown: device removed");
        record_lost("Unknown: again");
        assert!(egui_wgpu::recovery::device_lost_pending(), "the painter is told to recreate the device");
        assert_eq!(take_lost().as_deref(), Some("Unknown: device removed"));
        assert_eq!(take_lost(), None, "a loss is reported once");
        let generation = GENERATION.load(Ordering::SeqCst);
        lost_callback(generation + 7, wgpu::DeviceLostReason::Unknown, "an older device");
        assert_eq!(take_lost(), None, "an old device does not count");
        lost_callback(generation, wgpu::DeviceLostReason::Destroyed, "dropped");
        assert_eq!(take_lost(), None, "a device dropped on purpose does not count");
        lost_callback(generation, wgpu::DeviceLostReason::Unknown, "removed");
        assert_eq!(take_lost().as_deref(), Some("Unknown: removed"));
    }
}
