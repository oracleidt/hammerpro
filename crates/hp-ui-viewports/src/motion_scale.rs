pub const BUDGET_MS: f32 = 10.0;
pub const FLOOR: f32 = 0.5;
const STEP: f32 = 0.0625;
const GROW: f32 = 0.0625;
const MAX_DROP: f32 = 0.125;
const SMOOTHING: f32 = 0.35;
const NO_GAIN: f32 = 0.85;
const BLOCK_FRAMES: u32 = 240;
const SETTLE_FRAMES: u32 = 6;

static CURRENT: std::sync::atomic::AtomicU32 = std::sync::atomic::AtomicU32::new(0x3f80_0000);

pub fn current() -> f32 {
    f32::from_bits(CURRENT.load(std::sync::atomic::Ordering::Relaxed))
}

fn publish(scale: f32) {
    CURRENT.store(scale.to_bits(), std::sync::atomic::Ordering::Relaxed);
}

#[derive(Clone, Copy, Debug, PartialEq)]
pub struct MotionScale {
    scale: f32,
    smooth: f32,
    full_ms: f32,
    blocked: u32,
    settle: u32,
}

impl Default for MotionScale {
    fn default() -> Self {
        Self { scale: 1.0, smooth: 0.0, full_ms: 0.0, blocked: 0, settle: 0 }
    }
}

impl MotionScale {
    pub fn scale(&self) -> f32 {
        self.scale
    }

    pub fn update(&mut self, active: bool, gpu_ms: f32) -> f32 {
        if !active {
            self.scale = 1.0;
            self.smooth = 0.0;
            publish(1.0);
            return 1.0;
        }
        if gpu_ms.is_finite() && gpu_ms > 0.0 {
            self.smooth = if self.smooth > 0.0 { self.smooth + (gpu_ms - self.smooth) * SMOOTHING } else { gpu_ms };
            if self.scale >= 1.0 {
                self.full_ms = self.smooth;
            }
            let before = self.scale;
            if self.blocked > 0 {
                self.blocked -= 1;
                self.scale = 1.0;
            } else if self.settle > 0 {
                self.settle -= 1;
            } else if self.scale < 1.0 && self.full_ms > 0.0 && self.smooth > self.full_ms * NO_GAIN {
                self.blocked = BLOCK_FRAMES;
                self.scale = 1.0;
            } else {
                let want = (self.scale * (BUDGET_MS / self.smooth).sqrt()).clamp(FLOOR, 1.0);
                let next =
                    if want < self.scale { want.max(self.scale - MAX_DROP) } else { (self.scale + GROW).min(want) };
                let stepped = (next / STEP).floor() * STEP;
                self.scale = stepped.clamp(FLOOR, 1.0);
            }
            if self.scale < before {
                self.settle = SETTLE_FRAMES;
            }
        }
        publish(self.scale);
        self.scale
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn run(m: &mut MotionScale, frames: usize, cost: impl Fn(f32) -> f32) -> f32 {
        for _ in 0..frames {
            let ms = cost(m.scale());
            m.update(true, ms);
        }
        m.scale()
    }

    #[test]
    fn a_cheap_frame_keeps_the_full_resolution() {
        let mut m = MotionScale::default();
        assert_eq!(run(&mut m, 30, |_| 3.0), 1.0);
    }

    #[test]
    fn a_pixel_bound_view_lowers_the_resolution_and_rest_restores_it() {
        let mut m = MotionScale::default();
        let s = run(&mut m, 40, |s| 30.0 * s * s);
        assert!((FLOOR..0.75).contains(&s), "{s}");
        let ms = 30.0 * s * s;
        assert!(ms <= BUDGET_MS * 1.3, "{ms}");
        assert_eq!(m.update(false, 40.0), 1.0, "a still camera draws the full picture");
    }

    #[test]
    fn a_view_that_does_not_get_cheaper_keeps_its_resolution() {
        let mut m = MotionScale::default();
        let s = run(&mut m, 60, |_| 14.0);
        assert_eq!(s, 1.0, "lowering the resolution must not cost quality for nothing");
    }

    #[test]
    fn one_slow_frame_does_not_drop_straight_to_the_floor() {
        let mut m = MotionScale::default();
        run(&mut m, 10, |_| 6.0);
        let s = m.update(true, 80.0);
        assert!(s >= 1.0 - MAX_DROP, "{s}");
        assert_eq!(m.update(true, 80.0), s, "the next frames wait for the timing of the new size");
    }

    #[test]
    fn the_resolution_grows_back_slowly_when_the_frames_get_cheap() {
        let mut m = MotionScale::default();
        let low = run(&mut m, 40, |s| 30.0 * s * s);
        let next = run(&mut m, 1, |_| 1.0);
        assert!(next > low && next - low <= GROW + 1e-6, "{low} {next}");
        assert_eq!(m.update(true, f32::NAN), next, "a missing timing keeps the scale");
    }

    #[test]
    fn the_scale_moves_in_coarse_steps_so_the_picture_is_not_reallocated_every_frame() {
        let mut m = MotionScale::default();
        let s = run(&mut m, 5, |s| 17.0 * s * s);
        assert_eq!((s / STEP).fract(), 0.0, "{s}");
    }
}
