use egui::{Pos2, Rect, Vec2};
use glam::DVec2;
use hp_app::view::Camera2d;

pub const EDGE_MARGIN: f32 = 16.0;
pub const START_SPEED: f32 = 60.0;
pub const GAIN: f32 = 12.0;
pub const MAX_SPEED: f32 = 4000.0;
pub const MAX_STEP_SECS: f32 = 0.05;

fn margin_for(span: f32) -> f32 {
    EDGE_MARGIN.min((span * 0.25).max(0.0))
}

pub fn speed_for_depth(depth: f32) -> f32 {
    if !depth.is_finite() || depth <= 0.0 {
        return if depth.is_finite() { 0.0 } else { MAX_SPEED };
    }
    (START_SPEED + GAIN * depth).min(MAX_SPEED)
}

pub fn axis_speed(pos: f32, lo: f32, hi: f32) -> f32 {
    if !(pos.is_finite() && lo.is_finite() && hi.is_finite()) || hi <= lo {
        return 0.0;
    }
    let margin = margin_for(hi - lo);
    let low = lo + margin - pos;
    let high = pos - (hi - margin);
    if low > 0.0 {
        -speed_for_depth(low)
    } else if high > 0.0 {
        speed_for_depth(high)
    } else {
        0.0
    }
}

pub fn edge_velocity(rect: Rect, pos: Pos2) -> Vec2 {
    Vec2::new(axis_speed(pos.x, rect.left(), rect.right()), axis_speed(pos.y, rect.top(), rect.bottom()))
}

pub fn scroll_step(rect: Rect, pos: Pos2, dt: f32) -> Vec2 {
    let dt = if dt.is_finite() { dt.clamp(0.0, MAX_STEP_SECS) } else { 0.0 };
    edge_velocity(rect, pos) * dt
}

pub fn apply(cam: &mut Camera2d, scroll: Vec2) -> bool {
    if scroll == Vec2::ZERO || !scroll.is_finite() {
        return false;
    }
    let before = cam.center;
    crate::camera::pan_2d(cam, -DVec2::new(f64::from(scroll.x), f64::from(scroll.y)));
    cam.center != before
}

#[cfg(test)]
mod tests {
    use super::*;
    use egui::vec2;

    fn rect() -> Rect {
        Rect::from_min_size(Pos2::new(100.0, 50.0), vec2(800.0, 600.0))
    }

    #[test]
    fn no_scroll_away_from_the_edges() {
        let r = rect();
        assert_eq!(edge_velocity(r, r.center()), Vec2::ZERO);
        assert_eq!(edge_velocity(r, Pos2::new(r.left() + EDGE_MARGIN + 1.0, r.top() + EDGE_MARGIN + 1.0)), Vec2::ZERO);
        assert_eq!(edge_velocity(r, Pos2::new(r.right() - EDGE_MARGIN, r.center().y)), Vec2::ZERO);
    }

    #[test]
    fn scrolls_toward_the_edge_the_cursor_nears() {
        let r = rect();
        let c = r.center();
        assert!(edge_velocity(r, Pos2::new(r.right() - 2.0, c.y)).x > 0.0);
        assert!(edge_velocity(r, Pos2::new(r.left() + 2.0, c.y)).x < 0.0);
        assert!(edge_velocity(r, Pos2::new(c.x, r.top() - 30.0)).y < 0.0);
        assert!(edge_velocity(r, Pos2::new(c.x, r.bottom() + 30.0)).y > 0.0);
        let corner = edge_velocity(r, Pos2::new(r.right() + 40.0, r.bottom() + 40.0));
        assert!(corner.x > 0.0 && corner.y > 0.0);
        assert!((corner.x - corner.y).abs() < 1e-3);
    }

    #[test]
    fn speed_grows_with_the_distance_past_the_edge() {
        let r = rect();
        let c = r.center();
        let mut last = 0.0;
        for d in [-EDGE_MARGIN + 1.0, -4.0, 0.0, 10.0, 50.0, 150.0, 300.0] {
            let v = edge_velocity(r, Pos2::new(r.right() + d, c.y)).x;
            assert!(v > last, "{d}: {v} after {last}");
            last = v;
        }
        let at_edge = edge_velocity(r, Pos2::new(r.right(), c.y)).x;
        assert!((at_edge - (START_SPEED + GAIN * EDGE_MARGIN)).abs() < 1e-3, "{at_edge}");
        assert_eq!(edge_velocity(r, Pos2::new(r.right() + 1.0e6, c.y)).x, MAX_SPEED);
        assert_eq!(edge_velocity(r, Pos2::new(r.left() - 1.0e6, c.y)).x, -MAX_SPEED);
    }

    #[test]
    fn small_views_keep_a_quiet_middle() {
        let r = Rect::from_min_size(Pos2::ZERO, vec2(40.0, 40.0));
        assert_eq!(edge_velocity(r, r.center()), Vec2::ZERO);
        assert!(edge_velocity(r, Pos2::new(39.0, 20.0)).x > 0.0);
        assert_eq!(axis_speed(5.0, 10.0, 10.0), 0.0);
        assert_eq!(axis_speed(f32::NAN, 0.0, 10.0), 0.0);
        assert_eq!(edge_velocity(Rect::NOTHING, Pos2::ZERO), Vec2::ZERO);
    }

    #[test]
    fn steps_are_time_based_and_capped() {
        let r = rect();
        let p = Pos2::new(r.right() + 20.0, r.center().y);
        let v = edge_velocity(r, p).x;
        assert!((scroll_step(r, p, 0.01).x - v * 0.01).abs() < 1e-4);
        assert!((scroll_step(r, p, 5.0).x - v * MAX_STEP_SECS).abs() < 1e-3);
        assert_eq!(scroll_step(r, p, -1.0), Vec2::ZERO);
        assert_eq!(scroll_step(r, p, f32::NAN), Vec2::ZERO);
    }

    #[test]
    fn the_camera_moves_toward_the_cursor_side() {
        let mut cam = Camera2d { center: DVec2::new(0.0, 0.0), zoom: 0.5 };
        assert!(apply(&mut cam, Vec2::new(10.0, 0.0)));
        assert!((cam.center - DVec2::new(20.0, 0.0)).length() < 1e-9, "{:?}", cam.center);
        assert!(apply(&mut cam, Vec2::new(0.0, 10.0)));
        assert!((cam.center - DVec2::new(20.0, -20.0)).length() < 1e-9, "screen down is world down");
        assert!(!apply(&mut cam, Vec2::ZERO));
        assert!(!apply(&mut cam, Vec2::new(f32::NAN, 1.0)));
    }
}
