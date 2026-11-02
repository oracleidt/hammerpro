use egui::Pos2;

pub const WARP_DISTANCE: f32 = 24.0;
const MAX_MISSES: u32 = 6;

#[derive(Clone, Copy, Debug, Default)]
pub struct CursorWarp {
    sent: Option<(Pos2, Pos2)>,
    misses: u32,
    was_focused: bool,
}

impl CursorWarp {
    pub fn reset(&mut self) {
        *self = Self { was_focused: self.was_focused, ..Self::default() };
    }

    pub fn gave_up(&self) -> bool {
        self.misses >= MAX_MISSES
    }

    pub fn request(&mut self, focused: bool, pointer: Option<Pos2>, moved: bool, target: Pos2) -> bool {
        if focused != self.was_focused {
            self.was_focused = focused;
            self.sent = None;
            self.misses = 0;
        }
        if !focused {
            return false;
        }
        let Some(p) = pointer else { return false };
        if let Some((from, to)) = self.sent.take() {
            if p.distance(to) < p.distance(from) {
                self.misses = 0;
            } else {
                self.misses = self.misses.saturating_add(1);
                if self.gave_up() {
                    log::debug!("the cursor cannot be moved back to the view, mouselook keeps working without it");
                }
            }
        }
        if self.gave_up() || !moved || p.distance(target) <= WARP_DISTANCE {
            return false;
        }
        self.sent = Some((p, target));
        true
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn no_warp_without_focus() {
        let mut w = CursorWarp::default();
        let c = Pos2::new(100.0, 100.0);
        assert!(!w.request(false, Some(Pos2::new(300.0, 300.0)), true, c));
        assert!(w.request(true, Some(Pos2::new(300.0, 300.0)), true, c));
    }

    #[test]
    fn failing_warps_stop_after_a_few_tries() {
        let mut w = CursorWarp::default();
        let c = Pos2::new(100.0, 100.0);
        let mut sent = 0;
        for i in 0..100 {
            let p = Pos2::new(300.0 + i as f32, 300.0);
            if w.request(true, Some(p), true, c) {
                sent += 1;
            }
        }
        assert!(w.gave_up());
        assert!(sent <= MAX_MISSES as usize + 1, "{sent} warps");
        assert!(!w.request(false, Some(Pos2::new(300.0, 300.0)), true, c));
        assert!(w.request(true, Some(Pos2::new(300.0, 300.0)), true, c), "focus back: try again");
    }

    #[test]
    fn working_warps_keep_going() {
        let mut w = CursorWarp::default();
        let c = Pos2::new(100.0, 100.0);
        for _ in 0..50 {
            assert!(w.request(true, Some(Pos2::new(160.0, 100.0)), true, c));
            assert!(!w.request(true, Some(Pos2::new(110.0, 100.0)), true, c));
        }
        assert!(!w.gave_up());
        assert!(!w.request(true, Some(Pos2::new(160.0, 100.0)), false, c), "no motion, no warp");
    }
}
