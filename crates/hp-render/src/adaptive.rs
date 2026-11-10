use std::sync::Arc;
use std::sync::atomic::{AtomicU32, Ordering};
use std::time::{Duration, Instant};

pub const MAX_LEVEL: u8 = 3;
pub const TARGET_MS: f32 = 14.0;
const SLOW: f32 = 1.2;
const CALM: f32 = 0.5;
const WINDOW: Duration = Duration::from_millis(1000);
const IDLE: Duration = Duration::from_millis(3000);
const MIN_SAMPLES: u32 = 12;
const HEAVY_SAMPLES: u32 = 3;
const HEAVY_FRAME_MS: f32 = 150.0;
const SLOW_WINDOWS: u32 = 2;
const CALM_WINDOWS: u32 = 4;
const RETRY: Duration = Duration::from_secs(20);

#[derive(Clone, Copy, Debug, Default, PartialEq)]
pub struct AdaptiveStats {
    pub enabled: bool,
    pub level: u8,
    pub max_level: u8,
    pub gpu_ms: f32,
    pub target_ms: f32,
    pub changes: u32,
    pub timestamps: bool,
}

#[derive(Default)]
pub(crate) struct Governor {
    enabled: bool,
    level: u8,
    start: Option<Instant>,
    last_sample: Option<Instant>,
    sum: f32,
    n: u32,
    slow: u32,
    calm: u32,
    avg: f32,
    changes: u32,
    lowered_at: [Option<Instant>; MAX_LEVEL as usize + 1],
}

impl Governor {
    pub fn level(&self) -> u8 {
        if self.enabled { self.level } else { 0 }
    }

    pub fn stats(&self, timestamps: bool) -> AdaptiveStats {
        AdaptiveStats {
            enabled: self.enabled,
            level: self.level(),
            max_level: MAX_LEVEL,
            gpu_ms: self.avg,
            target_ms: TARGET_MS,
            changes: self.changes,
            timestamps,
        }
    }

    fn reset_window(&mut self) {
        self.start = None;
        self.sum = 0.0;
        self.n = 0;
    }

    pub fn set_enabled(&mut self, on: bool) -> bool {
        if self.enabled == on {
            return false;
        }
        let was = self.level();
        self.enabled = on;
        self.reset_window();
        self.slow = 0;
        self.calm = 0;
        if !on {
            self.level = 0;
            self.lowered_at = Default::default();
        }
        was != self.level()
    }

    pub fn observe(&mut self, gpu_ms: f32, now: Instant) -> bool {
        if !self.enabled || !gpu_ms.is_finite() || gpu_ms <= 0.0 {
            return false;
        }
        let idle = IDLE + Duration::from_secs_f32((self.avg.max(gpu_ms) * 0.003).min(60.0));
        if self.last_sample.is_some_and(|t| now.duration_since(t) > idle) {
            self.reset_window();
        }
        self.last_sample = Some(now);
        let start = *self.start.get_or_insert(now);
        self.sum += gpu_ms;
        self.n += 1;
        let steady = now.duration_since(start) >= WINDOW && self.n >= MIN_SAMPLES;
        let heavy = self.n >= HEAVY_SAMPLES && self.sum >= HEAVY_FRAME_MS * self.n as f32;
        if !steady && !heavy {
            return false;
        }
        let avg = self.sum / self.n as f32;
        self.avg = avg;
        self.reset_window();
        if avg > TARGET_MS * SLOW {
            self.calm = 0;
            self.slow += 1;
            if self.slow >= SLOW_WINDOWS && self.level < MAX_LEVEL {
                self.lowered_at[self.level as usize] = Some(now);
                self.level += 1;
                self.slow = 0;
                self.changes += 1;
                return true;
            }
        } else if avg < TARGET_MS * CALM {
            self.slow = 0;
            self.calm += 1;
            let back = self.level.saturating_sub(1) as usize;
            let waited = self.lowered_at[back].is_none_or(|t| now.duration_since(t) >= RETRY);
            if self.calm >= CALM_WINDOWS && self.level > 0 && waited {
                self.level -= 1;
                self.calm = 0;
                self.changes += 1;
                return true;
            }
        } else {
            self.slow = 0;
            self.calm = 0;
        }
        false
    }
}

#[derive(Default)]
pub(crate) struct SubmitTimer {
    latest: Arc<AtomicU32>,
    pending: Arc<AtomicU32>,
}

impl SubmitTimer {
    pub fn after_submit(&self, queue: &wgpu::Queue) {
        if self.pending.swap(1, Ordering::AcqRel) == 1 {
            return;
        }
        let latest = self.latest.clone();
        let pending = self.pending.clone();
        let start = Instant::now();
        queue.on_submitted_work_done(move || {
            let ms = start.elapsed().as_secs_f32() * 1000.0;
            latest.store(ms.to_bits(), Ordering::Release);
            pending.store(0, Ordering::Release);
        });
    }

    pub fn take(&self) -> Option<f32> {
        let bits = self.latest.swap(0, Ordering::AcqRel);
        (bits != 0).then(|| f32::from_bits(bits))
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn feed(g: &mut Governor, t: &mut Instant, secs: f32, ms: f32) -> Vec<u8> {
        let mut levels = Vec::new();
        let steps = (secs * 1000.0 / ms.max(1.0)) as usize;
        for _ in 0..steps {
            *t += Duration::from_secs_f32(ms.max(1.0) / 1000.0);
            if g.observe(ms, *t) {
                levels.push(g.level());
            }
        }
        levels
    }

    #[test]
    fn fast_cards_never_change_anything() {
        let mut g = Governor::default();
        g.set_enabled(true);
        let mut t = Instant::now();
        assert!(feed(&mut g, &mut t, 120.0, 5.0).is_empty());
        assert_eq!(g.level(), 0);
    }

    #[test]
    fn slow_cards_step_down_one_level_at_a_time_and_come_back_slowly() {
        let mut g = Governor::default();
        g.set_enabled(true);
        let mut t = Instant::now();
        assert_eq!(feed(&mut g, &mut t, 2.5, 30.0), vec![1]);
        assert_eq!(feed(&mut g, &mut t, 4.5, 30.0), vec![2, 3]);
        assert!(feed(&mut g, &mut t, 6.0, 30.0).is_empty());
        assert_eq!(g.level(), MAX_LEVEL);
        assert!(feed(&mut g, &mut t, 6.0, 4.0).is_empty(), "no quick bounce back");
        assert_eq!(feed(&mut g, &mut t, 80.0, 4.0), vec![2, 1, 0]);
        assert_eq!(feed(&mut g, &mut t, 2.5, 30.0), vec![1]);
        assert!(g.set_enabled(false));
        assert_eq!(g.level(), 0);
        assert!(feed(&mut g, &mut t, 10.0, 60.0).is_empty(), "off never lowers");
    }

    #[test]
    fn a_card_far_too_slow_for_many_samples_still_steps_down() {
        let mut g = Governor::default();
        g.set_enabled(true);
        let mut t = Instant::now();
        let mut levels = Vec::new();
        for _ in 0..30 {
            t += Duration::from_millis(2500);
            if g.observe(2000.0, t) {
                levels.push(g.level());
            }
        }
        assert_eq!(levels, vec![1, 2, 3]);
    }

    #[test]
    fn idle_gaps_do_not_count() {
        let mut g = Governor::default();
        g.set_enabled(true);
        let mut t = Instant::now();
        for _ in 0..30 {
            t += Duration::from_secs(5);
            assert!(!g.observe(40.0, t));
        }
        assert_eq!(g.level(), 0);
    }
}
