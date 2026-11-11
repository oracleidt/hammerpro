use egui::{Event, Modifiers, MouseWheelSource, MouseWheelUnit, Vec2};
use hp_app::settings::{InputSettings, TrackpadMode, TrackpadScroll3d};

pub const POINTS_PER_LINE: f32 = 40.0;
pub const POINTS_PER_PAGE: f32 = 200.0;
pub const MAX_PAN_POINTS: f32 = 240.0;
pub const MAX_ZOOM_NOTCHES: f32 = 3.0;
pub const ORBIT_DEGREES_PER_POINT: f64 = 0.3;
pub const TAP_TRAVEL: f32 = 9.0;
pub const ZOOM_STEP: f32 = 1.2;

#[derive(Clone, Copy, Debug, Default, PartialEq)]
pub struct FrameScroll {
    pub points: Vec2,
    pub pinch: f32,
    pub trackpad_like: bool,
    pub notched: bool,
    pub events: u32,
}

impl FrameScroll {
    pub fn is_empty(&self) -> bool {
        self.events == 0 && self.pinch == 0.0
    }

    pub fn add(&mut self, event: &Event) {
        match event {
            Event::MouseWheel { unit, delta, source, modifiers, .. } => {
                let d = Vec2::new(finite(delta.x), finite(delta.y));
                self.points += d * points_per(*unit);
                self.events += 1;
                match classify(*unit, d, *source, *modifiers) {
                    Kind::Trackpad => self.trackpad_like = true,
                    Kind::Notch => self.notched = true,
                    Kind::Unsure => {}
                }
            }
            Event::Zoom(factor) if factor.is_finite() && *factor > 0.0 => {
                self.pinch += factor.ln() / ZOOM_STEP.ln();
                self.trackpad_like = true;
            }
            _ => {}
        }
    }

    pub fn gather(events: &[Event]) -> Self {
        let mut out = FrameScroll::default();
        for e in events {
            out.add(e);
        }
        out
    }
}

fn finite(v: f32) -> f32 {
    if v.is_finite() { v } else { 0.0 }
}

fn points_per(unit: MouseWheelUnit) -> f32 {
    match unit {
        MouseWheelUnit::Point => 1.0,
        MouseWheelUnit::Line => POINTS_PER_LINE,
        MouseWheelUnit::Page => POINTS_PER_PAGE,
    }
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
enum Kind {
    Trackpad,
    Notch,
    Unsure,
}

fn whole(v: f32) -> bool {
    (v - v.round()).abs() < 1e-4
}

fn classify(unit: MouseWheelUnit, d: Vec2, source: MouseWheelSource, modifiers: Modifiers) -> Kind {
    if matches!(source, MouseWheelSource::Trackpad | MouseWheelSource::Momentum) {
        return Kind::Trackpad;
    }
    if d == Vec2::ZERO {
        return Kind::Unsure;
    }
    match unit {
        MouseWheelUnit::Point => Kind::Trackpad,
        MouseWheelUnit::Page => Kind::Notch,
        MouseWheelUnit::Line => {
            let fractional_x = d.x != 0.0 && !whole(d.x);
            let pinch = (modifiers.ctrl || modifiers.command) && !whole(d.y);
            if fractional_x || pinch {
                Kind::Trackpad
            } else if whole(d.x) && whole(d.y) && d.y != 0.0 && source != MouseWheelSource::Trackpad {
                Kind::Notch
            } else {
                Kind::Unsure
            }
        }
    }
}

#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub struct Detector {
    pub detected: bool,
}

impl Detector {
    pub fn update(&mut self, scroll: &FrameScroll) {
        if scroll.trackpad_like {
            self.detected = true;
        } else if scroll.notched {
            self.detected = false;
        }
    }

    pub fn active(&self, mode: TrackpadMode) -> bool {
        mode.active(self.detected)
    }
}

#[derive(Clone, Copy, Debug, PartialEq)]
pub enum Intent {
    None,
    Wheel { notches: f32, modifiers: Modifiers },
    Pan(Vec2),
    Zoom(f32),
}

pub fn route(scroll: &FrameScroll, modifiers: Modifiers, settings: &InputSettings) -> Intent {
    let zoom_held = modifiers.ctrl || modifiers.command;
    let pinch = scroll.pinch * settings.zoom_speed();
    if zoom_held || scroll.pinch != 0.0 {
        let scrolled = if zoom_held { scroll.points.y / POINTS_PER_LINE * settings.zoom_speed() } else { 0.0 };
        let notches = (pinch + scrolled).clamp(-MAX_ZOOM_NOTCHES, MAX_ZOOM_NOTCHES);
        return if notches == 0.0 { Intent::None } else { Intent::Zoom(notches) };
    }
    if scroll.points == Vec2::ZERO {
        return Intent::None;
    }
    if modifiers.alt || modifiers.shift {
        let along = if scroll.points.y != 0.0 { scroll.points.y } else { scroll.points.x };
        let notches = (along / POINTS_PER_LINE).clamp(-MAX_ZOOM_NOTCHES, MAX_ZOOM_NOTCHES);
        let modifiers = Modifiers { alt: false, ..modifiers };
        return Intent::Wheel { notches, modifiers };
    }
    let pan = scroll.points * settings.pan_speed();
    let len = pan.length();
    let pan = if len > MAX_PAN_POINTS { pan * (MAX_PAN_POINTS / len) } else { pan };
    Intent::Pan(pan)
}

#[derive(Clone, Copy, Debug, PartialEq)]
pub enum Motion3d {
    Orbit { yaw: f64, pitch: f64 },
    Move(Vec2),
}

pub fn pan_in_3d(pan: Vec2, settings: &InputSettings) -> Motion3d {
    match settings.trackpad_scroll_3d {
        TrackpadScroll3d::Orbit => {
            let k = ORBIT_DEGREES_PER_POINT * f64::from(settings.orbit_speed());
            Motion3d::Orbit { yaw: -f64::from(pan.x) * k, pitch: -f64::from(pan.y) * k }
        }
        TrackpadScroll3d::Move => Motion3d::Move(pan),
    }
}

pub fn click_travel(active: bool, mouse: f32) -> f32 {
    if active { mouse.max(TAP_TRAVEL) } else { mouse }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn wheel(unit: MouseWheelUnit, x: f32, y: f32, source: MouseWheelSource, modifiers: Modifiers) -> Event {
        Event::MouseWheel { unit, delta: Vec2::new(x, y), phase: egui::TouchPhase::Move, source, modifiers }
    }

    fn line(x: f32, y: f32) -> Event {
        wheel(MouseWheelUnit::Line, x, y, MouseWheelSource::Unknown, Modifiers::NONE)
    }

    #[test]
    fn a_mouse_wheel_is_never_taken_for_a_trackpad() {
        let mut d = Detector::default();
        for e in [line(0.0, 1.0), line(0.0, -3.0), line(0.0, 2.0)] {
            let s = FrameScroll::gather(&[e]);
            assert!(s.notched && !s.trackpad_like);
            d.update(&s);
            assert!(!d.active(TrackpadMode::Auto));
        }
        assert!(d.active(TrackpadMode::On));
        let s = FrameScroll::gather(&[line(0.0, 0.25)]);
        d.update(&s);
        assert!(!d.active(TrackpadMode::Auto), "a high resolution wheel alone does not switch");
    }

    #[test]
    fn trackpad_evidence_latches_until_a_wheel_notch() {
        let mut d = Detector::default();
        for e in [
            wheel(MouseWheelUnit::Point, 0.0, 3.5, MouseWheelSource::Unknown, Modifiers::NONE),
            wheel(MouseWheelUnit::Line, 0.0, 1.0, MouseWheelSource::Trackpad, Modifiers::NONE),
            wheel(MouseWheelUnit::Line, 0.0, 1.0, MouseWheelSource::Momentum, Modifiers::NONE),
            line(0.3, 0.1),
            wheel(MouseWheelUnit::Line, 0.0, 0.4, MouseWheelSource::Unknown, Modifiers::CTRL),
            Event::Zoom(1.1),
        ] {
            let mut d1 = Detector::default();
            d1.update(&FrameScroll::gather(std::slice::from_ref(&e)));
            assert!(d1.active(TrackpadMode::Auto), "{e:?}");
            assert!(!d1.active(TrackpadMode::Off), "{e:?}");
        }
        d.update(&FrameScroll::gather(&[line(0.3, 0.1)]));
        assert!(d.detected);
        d.update(&FrameScroll::gather(&[line(0.0, 0.2)]));
        assert!(d.detected, "vertical fractions keep the trackpad");
        d.update(&FrameScroll::default());
        assert!(d.detected, "idle frames keep it");
        d.update(&FrameScroll::gather(&[line(0.0, 1.0)]));
        assert!(!d.detected, "a whole wheel notch means a mouse again");
    }

    #[test]
    fn scroll_pans_and_pinch_zooms() {
        let s = InputSettings::default();
        let pan = route(&FrameScroll::gather(&[line(0.5, -0.25)]), Modifiers::NONE, &s);
        assert_eq!(pan, Intent::Pan(Vec2::new(20.0, -10.0)));
        let pts = wheel(MouseWheelUnit::Point, 3.0, 4.0, MouseWheelSource::Trackpad, Modifiers::NONE);
        assert_eq!(route(&FrameScroll::gather(&[pts]), Modifiers::NONE, &s), Intent::Pan(Vec2::new(3.0, 4.0)));
        let ctrl = wheel(MouseWheelUnit::Line, 0.0, 0.5, MouseWheelSource::Unknown, Modifiers::CTRL);
        assert_eq!(route(&FrameScroll::gather(&[ctrl]), Modifiers::CTRL, &s), Intent::Zoom(0.5));
        match route(&FrameScroll::gather(&[Event::Zoom(1.2)]), Modifiers::NONE, &s) {
            Intent::Zoom(n) => assert!((n - 1.0).abs() < 1e-4, "{n}"),
            other => panic!("{other:?}"),
        }
        match route(&FrameScroll::gather(&[Event::Zoom(1.0 / 1.44)]), Modifiers::NONE, &s) {
            Intent::Zoom(n) => assert!((n + 2.0).abs() < 1e-3, "{n}"),
            other => panic!("{other:?}"),
        }
        assert_eq!(route(&FrameScroll::default(), Modifiers::NONE, &s), Intent::None);
        assert_eq!(route(&FrameScroll::gather(&[Event::Zoom(1.0)]), Modifiers::NONE, &s), Intent::None);
    }

    #[test]
    fn modifiers_give_back_the_wheel() {
        let s = InputSettings::default();
        let scroll = FrameScroll::gather(&[line(0.0, 0.5)]);
        assert_eq!(
            route(&scroll, Modifiers::ALT, &s),
            Intent::Wheel { notches: 0.5, modifiers: Modifiers::NONE },
            "Alt + scroll is a plain wheel"
        );
        assert_eq!(route(&scroll, Modifiers::SHIFT, &s), Intent::Wheel { notches: 0.5, modifiers: Modifiers::SHIFT });
        let sideways = FrameScroll::gather(&[line(-0.75, 0.0)]);
        assert_eq!(
            route(&sideways, Modifiers::SHIFT, &s),
            Intent::Wheel { notches: -0.75, modifiers: Modifiers::SHIFT },
            "macOS turns Shift + scroll sideways"
        );
    }

    #[test]
    fn no_huge_jumps() {
        let s = InputSettings { trackpad_pan_speed: 4.0, trackpad_zoom_speed: 4.0, ..InputSettings::default() };
        match route(&FrameScroll::gather(&[line(0.0, 40.0)]), Modifiers::NONE, &s) {
            Intent::Pan(p) => assert!((p.length() - MAX_PAN_POINTS).abs() < 1e-3, "{p:?}"),
            other => panic!("{other:?}"),
        }
        assert_eq!(
            route(&FrameScroll::gather(&[Event::Zoom(50.0)]), Modifiers::NONE, &s),
            Intent::Zoom(MAX_ZOOM_NOTCHES)
        );
        let bad = wheel(MouseWheelUnit::Line, f32::NAN, f32::INFINITY, MouseWheelSource::Unknown, Modifiers::NONE);
        let scroll = FrameScroll::gather(&[bad, Event::Zoom(f32::NAN), Event::Zoom(-1.0)]);
        assert_eq!(scroll.points, Vec2::ZERO);
        assert_eq!(route(&scroll, Modifiers::NONE, &s), Intent::None);
        let fractions: Vec<Event> = (0..10).map(|_| line(0.0, 0.1)).collect();
        match route(&FrameScroll::gather(&fractions), Modifiers::NONE, &InputSettings::default()) {
            Intent::Pan(p) => assert!((p.y - 40.0).abs() < 1e-3, "fractions add up: {p:?}"),
            other => panic!("{other:?}"),
        }
    }

    #[test]
    fn orbit_or_move_in_3d() {
        let mut s = InputSettings::default();
        match pan_in_3d(Vec2::new(10.0, -20.0), &s) {
            Motion3d::Orbit { yaw, pitch } => {
                assert!(yaw < 0.0 && pitch > 0.0);
                assert!((yaw + 3.0).abs() < 1e-9 && (pitch - 6.0).abs() < 1e-9);
            }
            other => panic!("{other:?}"),
        }
        s.trackpad_scroll_3d = TrackpadScroll3d::Move;
        assert_eq!(pan_in_3d(Vec2::new(1.0, 2.0), &s), Motion3d::Move(Vec2::new(1.0, 2.0)));
    }

    #[test]
    fn taps_tolerate_a_little_finger_motion() {
        assert_eq!(click_travel(false, 4.0), 4.0);
        assert_eq!(click_travel(true, 4.0), TAP_TRAVEL);
        assert_eq!(click_travel(true, 20.0), 20.0);
    }
}
