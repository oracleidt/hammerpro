use glam::{DVec2, DVec3};
use hp_app::view::{Camera2d, Camera3d};

pub const PITCH_LIMIT: f64 = 89.0;
pub const ZOOM_STEP: f64 = 1.2;
pub const FLY_RESPONSE: f64 = 0.07;
pub const STRAFE_FRACTION: f64 = 0.0025;
pub const ARROW_LOOK_RATE: f64 = 90.0;
pub const WHEEL_DOLLY_FRACTION: f64 = 0.064;
pub const MAX_DT: f64 = 0.1;

pub fn wrap_degrees(a: f64) -> f64 {
    if !a.is_finite() {
        return 0.0;
    }
    let r = a.rem_euclid(360.0);
    if r >= 180.0 { r - 360.0 } else { r }
}

pub fn mouselook(cam: &mut Camera3d, delta: DVec2, sensitivity: f64, invert_x: bool, invert_y: bool) {
    if !delta.is_finite() || !sensitivity.is_finite() {
        return;
    }
    let dx = if invert_x { -delta.x } else { delta.x };
    let dy = if invert_y { -delta.y } else { delta.y };
    cam.yaw_deg = wrap_degrees(cam.yaw_deg - dx * sensitivity);
    cam.pitch_deg = (cam.pitch_deg - dy * sensitivity).clamp(-PITCH_LIMIT, PITCH_LIMIT);
}

#[derive(Clone, Copy, Debug, Default, PartialEq)]
pub struct FlyInput {
    pub forward: f64,
    pub right: f64,
    pub up: f64,
    pub fast: bool,
}

impl FlyInput {
    pub fn is_idle(&self) -> bool {
        self.forward == 0.0 && self.right == 0.0 && self.up == 0.0
    }
}

pub fn fly_target_velocity(cam: &Camera3d, input: &FlyInput, speed: f64, fast_multiplier: f64) -> DVec3 {
    let dir = cam.forward() * input.forward + cam.right() * input.right + DVec3::Z * input.up;
    let len = dir.length();
    if !len.is_finite() || len < 1e-9 {
        return DVec3::ZERO;
    }
    let mult = if input.fast { fast_multiplier.max(0.0) } else { 1.0 };
    dir / len * speed.max(0.0) * mult
}

pub fn smooth_velocity(current: DVec3, target: DVec3, dt: f64) -> DVec3 {
    smooth_velocity_with(current, target, dt, FLY_RESPONSE)
}

pub const FIRST_STEP: f64 = 1.0 / 60.0;

pub fn motion_step(dt: f64, from_rest: bool) -> f64 {
    if from_rest && dt.is_finite() { dt.min(FIRST_STEP) } else { dt }
}

pub fn smooth_velocity_with(current: DVec3, target: DVec3, dt: f64, response: f64) -> DVec3 {
    if dt.is_nan() || dt <= 0.0 || !current.is_finite() || !target.is_finite() {
        return if target.is_finite() { target } else { DVec3::ZERO };
    }
    if response.is_nan() || response <= 1e-4 {
        return target;
    }
    let k = 1.0 - (-dt.min(MAX_DT) / response).exp();
    let v = current + (target - current) * k;
    if target == DVec3::ZERO && v.length() < 1.0 { DVec3::ZERO } else { v }
}

pub fn integrate(cam: &mut Camera3d, velocity: DVec3, dt: f64) {
    if dt > 0.0 && velocity.is_finite() {
        cam.pos += velocity * dt.min(MAX_DT);
    }
}

pub fn wheel_dolly(cam: &mut Camera3d, dir: DVec3, notches: f64, units_per_notch: f64) {
    if !notches.is_finite() || !units_per_notch.is_finite() {
        return;
    }
    let d = dir.normalize_or(cam.forward());
    cam.pos += d * notches * units_per_notch;
}

pub fn pan_3d(cam: &mut Camera3d, delta: DVec2, depth: f64, view_height_points: f64) {
    if !delta.is_finite() || view_height_points.is_nan() || view_height_points <= 0.0 {
        return;
    }
    let half = (cam.fov_y_deg.to_radians() * 0.5).tan().max(1e-3);
    let units_per_point = 2.0 * half * depth.clamp(1.0, 1e6) / view_height_points;
    cam.pos += -cam.right() * delta.x * units_per_point + cam.up() * delta.y * units_per_point;
}

pub fn pan_2d(cam: &mut Camera2d, delta: DVec2) {
    if !delta.is_finite() || cam.zoom.is_nan() || cam.zoom <= 0.0 {
        return;
    }
    cam.center.x -= delta.x / cam.zoom;
    cam.center.y += delta.y / cam.zoom;
}

pub fn zoom_2d(cam: &mut Camera2d, notches: f64, anchor: Option<DVec2>) {
    if !notches.is_finite() || notches == 0.0 {
        return;
    }
    let factor = ZOOM_STEP.powf(notches.clamp(-20.0, 20.0));
    let anchor = anchor.filter(|a| a.is_finite()).unwrap_or(cam.center);
    cam.zoom_at(factor, anchor);
}

pub fn strafe_3d(cam: &mut Camera3d, delta: DVec2, units_per_point: f64) {
    if !delta.is_finite() || !units_per_point.is_finite() {
        return;
    }
    cam.pos += cam.right() * delta.x * units_per_point - cam.forward() * delta.y * units_per_point;
}

pub fn arrow_look(cam: &mut Camera3d, yaw: f64, pitch: f64, dt: f64) {
    if !yaw.is_finite() || !pitch.is_finite() || !dt.is_finite() || dt <= 0.0 {
        return;
    }
    let step = ARROW_LOOK_RATE * dt.min(MAX_DT);
    cam.yaw_deg = wrap_degrees(cam.yaw_deg - yaw * step);
    cam.pitch_deg = (cam.pitch_deg + pitch * step).clamp(-PITCH_LIMIT, PITCH_LIMIT);
}

pub fn preset_zoom(digit: u8) -> Option<f64> {
    (1..=9).contains(&digit).then(|| 2f64.powi(i32::from(digit) - 5).clamp(Camera2d::MIN_ZOOM, Camera2d::MAX_ZOOM))
}

pub fn dolly_step(camera_speed: f64, fast: bool, fast_multiplier: f64) -> f64 {
    let base = camera_speed.max(1.0) * WHEEL_DOLLY_FRACTION;
    if fast { base * fast_multiplier.max(1.0) } else { base }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn motion_after_rest_starts_with_one_frame_step() {
        assert_eq!(motion_step(0.25, true), FIRST_STEP);
        assert_eq!(motion_step(0.005, true), 0.005);
        assert_eq!(motion_step(0.05, false), 0.05);
        let mut cam = Camera3d::default();
        let start = cam.pos;
        let target = DVec3::new(800.0, 0.0, 0.0);
        let step = motion_step(0.25, true);
        let v = smooth_velocity_with(DVec3::ZERO, target, step, 0.1);
        integrate(&mut cam, v, step);
        assert!((cam.pos - start).length() < 800.0 * FIRST_STEP, "no jump after an idle gap");
    }

    fn cam() -> Camera3d {
        Camera3d { pos: DVec3::ZERO, yaw_deg: 0.0, pitch_deg: 0.0, ..Camera3d::default() }
    }

    #[test]
    fn wrap() {
        assert_eq!(wrap_degrees(190.0), -170.0);
        assert_eq!(wrap_degrees(-190.0), 170.0);
        assert_eq!(wrap_degrees(180.0), -180.0);
        assert_eq!(wrap_degrees(f64::NAN), 0.0);
    }

    #[test]
    fn mouselook_directions_and_clamp() {
        let mut c = cam();
        mouselook(&mut c, DVec2::new(100.0, 0.0), 0.15, false, false);
        assert!((c.yaw_deg + 15.0).abs() < 1e-9);
        assert!(c.forward().y < 0.0);
        mouselook(&mut c, DVec2::new(100.0, 0.0), 0.15, true, false);
        assert!(c.yaw_deg.abs() < 1e-9);
        mouselook(&mut c, DVec2::new(0.0, -100.0), 0.15, false, false);
        assert!((c.pitch_deg - 15.0).abs() < 1e-9);
        mouselook(&mut c, DVec2::new(0.0, -100.0), 0.15, false, true);
        assert!(c.pitch_deg.abs() < 1e-9);
        mouselook(&mut c, DVec2::new(0.0, -100000.0), 0.15, false, false);
        assert_eq!(c.pitch_deg, PITCH_LIMIT);
        mouselook(&mut c, DVec2::new(f64::NAN, 0.0), 0.15, false, false);
        assert_eq!(c.pitch_deg, PITCH_LIMIT);
    }

    #[test]
    fn fly_velocity_is_normalized() {
        let c = cam();
        let input = FlyInput { forward: 1.0, right: 1.0, up: 0.0, fast: false };
        let v = fly_target_velocity(&c, &input, 1000.0, 3.0);
        assert!((v.length() - 1000.0).abs() < 1e-6);
        assert!(v.x > 0.0 && v.y < 0.0, "forward is +X, right is -Y: {v:?}");
        let fast = fly_target_velocity(&c, &FlyInput { fast: true, ..input }, 1000.0, 3.0);
        assert!((fast.length() - 3000.0).abs() < 1e-6);
        assert_eq!(fly_target_velocity(&c, &FlyInput::default(), 1000.0, 3.0), DVec3::ZERO);
        let up = fly_target_velocity(&c, &FlyInput { up: 1.0, ..FlyInput::default() }, 500.0, 3.0);
        assert!((up - DVec3::new(0.0, 0.0, 500.0)).length() < 1e-9);
    }

    #[test]
    fn wasd_with_dt_moves_expected_distance() {
        let mut c = cam();
        let target = fly_target_velocity(&c, &FlyInput { forward: 1.0, ..FlyInput::default() }, 1000.0, 3.0);
        let mut v = DVec3::ZERO;
        for _ in 0..100 {
            v = smooth_velocity(v, target, 0.01);
            integrate(&mut c, v, 0.01);
        }
        let expected = 1000.0 * (1.0 - FLY_RESPONSE);
        assert!((c.pos.x - expected).abs() < 15.0, "{}", c.pos.x);
        let mut c2 = cam();
        let mut v2 = DVec3::ZERO;
        for _ in 0..25 {
            v2 = smooth_velocity(v2, target, 0.04);
            integrate(&mut c2, v2, 0.04);
        }
        assert!((c2.pos.x - c.pos.x).abs() < 40.0, "{} vs {}", c2.pos.x, c.pos.x);
        for _ in 0..100 {
            v = smooth_velocity(v, DVec3::ZERO, 0.01);
        }
        assert_eq!(v, DVec3::ZERO);
        let mut c3 = cam();
        integrate(&mut c3, DVec3::new(1000.0, 0.0, 0.0), 10.0);
        assert!((c3.pos.x - 100.0).abs() < 1e-9);
    }

    #[test]
    fn fly_response_setting() {
        let target = DVec3::new(1000.0, 0.0, 0.0);
        assert_eq!(smooth_velocity_with(DVec3::ZERO, target, 0.01, 0.0), target);
        assert_eq!(smooth_velocity_with(target, DVec3::ZERO, 0.01, 0.0), DVec3::ZERO);
        let quick = smooth_velocity_with(DVec3::ZERO, target, 0.01, 0.05);
        let slow = smooth_velocity_with(DVec3::ZERO, target, 0.01, 0.5);
        assert!(quick.x > slow.x && slow.x > 0.0, "{quick:?} {slow:?}");
        assert_eq!(smooth_velocity_with(DVec3::ZERO, target, 0.01, f64::NAN), target);
        let a = smooth_velocity(DVec3::ZERO, target, 0.01);
        assert_eq!(a, smooth_velocity_with(DVec3::ZERO, target, 0.01, FLY_RESPONSE));
    }

    #[test]
    fn strafe_arrows_and_presets() {
        let mut c = cam();
        strafe_3d(&mut c, DVec2::new(0.0, -10.0), 2.0);
        assert!((c.pos - DVec3::new(20.0, 0.0, 0.0)).length() < 1e-9, "{:?}", c.pos);
        strafe_3d(&mut c, DVec2::new(10.0, 0.0), 2.0);
        assert!((c.pos - DVec3::new(20.0, -20.0, 0.0)).length() < 1e-9, "{:?}", c.pos);
        strafe_3d(&mut c, DVec2::new(f64::NAN, 0.0), 2.0);
        assert!(c.pos.is_finite());
        let mut c = cam();
        arrow_look(&mut c, 1.0, 0.0, 0.05);
        assert!((c.yaw_deg + 4.5).abs() < 1e-9, "right turns right: {}", c.yaw_deg);
        arrow_look(&mut c, 0.0, 1.0, 0.05);
        assert!((c.pitch_deg - 4.5).abs() < 1e-9);
        arrow_look(&mut c, 0.0, 1.0, 1000.0);
        assert!(c.pitch_deg <= PITCH_LIMIT);
        assert_eq!(preset_zoom(5), Some(1.0));
        assert_eq!(preset_zoom(1), Some(1.0 / 16.0));
        assert_eq!(preset_zoom(9), Some(16.0));
        assert_eq!(preset_zoom(0), None);
        assert_eq!(preset_zoom(10), None);
    }

    #[test]
    fn dolly_and_pan_3d() {
        let mut c = cam();
        wheel_dolly(&mut c, DVec3::ZERO, 2.0, 64.0);
        assert!((c.pos - DVec3::new(128.0, 0.0, 0.0)).length() < 1e-9);
        let mut c = cam();
        c.fov_y_deg = 90.0;
        pan_3d(&mut c, DVec2::new(10.0, 0.0), 300.0, 600.0);
        assert!((c.pos - DVec3::new(0.0, 10.0, 0.0)).length() < 1e-9, "{:?}", c.pos);
        pan_3d(&mut c, DVec2::new(0.0, 10.0), 300.0, 600.0);
        assert!((c.pos.z - 10.0).abs() < 1e-9);
        assert!((dolly_step(1000.0, false, 3.0) - 64.0).abs() < 1e-9);
        assert!((dolly_step(1000.0, true, 3.0) - 192.0).abs() < 1e-9);
    }

    #[test]
    fn pan_and_zoom_2d_keep_anchor() {
        let mut c = Camera2d { center: DVec2::ZERO, zoom: 2.0 };
        pan_2d(&mut c, DVec2::new(20.0, 10.0));
        assert!((c.center - DVec2::new(-10.0, 5.0)).length() < 1e-9);
        let mut c = Camera2d { center: DVec2::new(0.0, 0.0), zoom: 1.0 };
        let anchor = DVec2::new(100.0, -40.0);
        let before = (anchor - c.center) * c.zoom;
        zoom_2d(&mut c, 1.0, Some(anchor));
        assert!((c.zoom - 1.2).abs() < 1e-9);
        let after = (anchor - c.center) * c.zoom;
        assert!((before - after).length() < 1e-9, "anchor must stay under the cursor");
        zoom_2d(&mut c, -1.0, Some(anchor));
        assert!((c.zoom - 1.0).abs() < 1e-9);
        let center = c.center;
        zoom_2d(&mut c, 3.0, None);
        assert!((c.center - center).length() < 1e-9);
        zoom_2d(&mut c, 1000.0, None);
        assert!(c.zoom <= Camera2d::MAX_ZOOM);
    }
}
