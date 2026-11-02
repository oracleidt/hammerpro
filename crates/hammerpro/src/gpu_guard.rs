use std::sync::Arc;
use std::sync::Mutex;

const MAX_PENDING: usize = 8;
const LOGGED: u64 = 20;

#[derive(Debug, Default)]
struct Pending {
    errors: Vec<String>,
    count: u64,
    dropped: u64,
    lost: Option<String>,
    lost_once: bool,
}

static PENDING: Mutex<Pending> =
    Mutex::new(Pending { errors: Vec::new(), count: 0, dropped: 0, lost: None, lost_once: false });

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
    with_pending(|p| {
        if !p.lost_once {
            p.lost_once = true;
            p.lost = Some(reason.to_owned());
        }
    });
}

pub fn take() -> (Vec<String>, u64, Option<String>) {
    with_pending(|p| (std::mem::take(&mut p.errors), std::mem::take(&mut p.dropped), p.lost.take()))
}

pub fn install(device: &wgpu::Device) {
    device.on_uncaptured_error(Arc::new(|error: wgpu::Error| record_error(&error.to_string())));
    device.set_device_lost_callback(|reason, message| {
        if reason != wgpu::DeviceLostReason::Destroyed {
            record_lost(&format!("{reason:?}: {message}"));
        }
    });
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
        let (errors, dropped, lost) = take();
        assert_eq!(errors.len(), 2, "{errors:?}");
        assert!(errors[0].starts_with("Validation Error Caused by: In a pass"));
        assert!(errors[0].ends_with("..."), "long errors are cut");
        assert_eq!(dropped, 1, "the repeated error is counted, not kept");
        assert_eq!(lost, None);
        record_lost("Unknown: device removed");
        record_lost("Unknown: again");
        let (_, _, lost) = take();
        assert_eq!(lost.as_deref(), Some("Unknown: device removed"));
        assert_eq!(take().2, None, "a loss is reported once");
    }
}
