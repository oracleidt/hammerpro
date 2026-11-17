use egui::{Event, Key, Pos2};
use glam::{DMat4, DVec2, DVec3};
use hp_app::ViewInfo;
use hp_app::grid::GridFrame;

use crate::math::{closest_param_on_line, ray_plane, rotation_about, scale_about, snap, snap_angle};

#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash)]
pub enum ModalKind {
    Move,
    Rotate,
    Scale,
}

impl ModalKind {
    pub fn hint_key(self) -> &'static str {
        match self {
            ModalKind::Move => "seltool.hint.modal_move",
            ModalKind::Rotate => "seltool.hint.modal_rotate",
            ModalKind::Scale => "seltool.hint.modal_scale",
        }
    }
}

#[derive(Clone, Copy, Debug, Default, PartialEq, Eq, Hash)]
pub enum Lock {
    #[default]
    Free,
    Axis(usize),
    Plane(usize),
}

impl Lock {
    pub fn toggled(self, k: usize, plane: bool) -> Lock {
        let k = k.min(2);
        let wanted = if plane { Lock::Plane(k) } else { Lock::Axis(k) };
        if self == wanted { Lock::Free } else { wanted }
    }

    pub fn label(self) -> &'static str {
        const AXES: [&str; 3] = ["X", "Y", "Z"];
        const PLANES: [&str; 3] = ["YZ", "ZX", "XY"];
        match self {
            Lock::Free => "",
            Lock::Axis(k) => AXES[k.min(2)],
            Lock::Plane(k) => PLANES[k.min(2)],
        }
    }
}

#[derive(Clone, Copy, Debug, PartialEq)]
pub struct MoveSnap {
    pub reference: DVec3,
    pub grid: f64,
    pub frame: GridFrame,
}

impl MoveSnap {
    pub fn plain(reference: DVec3, size: f64) -> MoveSnap {
        MoveSnap { reference, grid: size, frame: GridFrame::uniform(size) }
    }
}

pub fn screen_angle(c: Pos2, p: Pos2) -> f64 {
    f64::from(-(p.y - c.y)).atan2(f64::from(p.x - c.x)).to_degrees()
}

fn toward_viewer(view: &ViewInfo, p: DVec3) -> DVec3 {
    if view.is_2d() {
        view.kind.toward_viewer()
    } else {
        (view.cam3d.pos - p).try_normalize().unwrap_or(-view.cam3d.forward())
    }
}

pub fn modal_move(
    view: &ViewInfo,
    pivot: DVec3,
    axes: [DVec3; 3],
    lock: Lock,
    start: Pos2,
    pos: Pos2,
    snapping: Option<MoveSnap>,
) -> DVec3 {
    let raw = raw_move(view, pivot, axes, lock, start, pos);
    let raw = if raw.is_finite() { raw.clamp(DVec3::splat(-1e6), DVec3::splat(1e6)) } else { DVec3::ZERO };
    let Some(s) = snapping else { return raw };
    match lock {
        Lock::Axis(k) => {
            let a = axes[k.min(2)];
            let amount = raw.dot(a);
            match world_axis(a) {
                Some((w, sign)) => a * sign * (s.frame.axis(w).snap(s.reference[w] + amount * sign) - s.reference[w]),
                None => a * snap(amount, s.grid),
            }
        }
        _ => {
            let mut out = DVec3::ZERO;
            match view.kind.axes_2d() {
                Some((r, u, d)) if raw[d].abs() < 1e-9 => {
                    let reference = DVec2::new(s.reference[r], s.reference[u]);
                    let d2 = s.frame.snap_delta_2d(view.kind, reference, DVec2::new(raw[r], raw[u]));
                    out[r] = if raw[r].abs() < 1e-9 && !s.frame.is_rotated() { 0.0 } else { d2.x };
                    out[u] = if raw[u].abs() < 1e-9 && !s.frame.is_rotated() { 0.0 } else { d2.y };
                }
                _ if s.frame.is_rotated() && matches!(lock, Lock::Free) => out = s.frame.snap_delta(s.reference, raw),
                _ => {
                    for k in 0..3 {
                        out[k] = if raw[k].abs() < 1e-9 {
                            0.0
                        } else {
                            s.frame.axis(k).snap(s.reference[k] + raw[k]) - s.reference[k]
                        };
                    }
                }
            }
            if let Lock::Plane(n) = lock
                && world_axis(axes[n.min(2)]).is_none()
            {
                return raw;
            }
            out
        }
    }
}

fn world_axis(a: DVec3) -> Option<(usize, f64)> {
    let k = a.abs().max_position();
    (a[k].abs() > 1.0 - 1e-9).then(|| (k, a[k].signum()))
}

fn raw_move(view: &ViewInfo, pivot: DVec3, axes: [DVec3; 3], lock: Lock, start: Pos2, pos: Pos2) -> DVec3 {
    if view.is_2d() {
        let d2 = view.screen_to_2d(pos) - view.screen_to_2d(start);
        let d = view.kind.from_2d(d2, 0.0);
        let depth = view.kind.toward_viewer();
        return match lock {
            Lock::Free => d,
            Lock::Axis(k) => {
                let a = axes[k.min(2)];
                let flat = a - depth * a.dot(depth);
                if flat.length_squared() < 0.04 {
                    a * (d2.y * a.dot(depth).signum())
                } else {
                    a * (d.dot(flat) / flat.length_squared())
                }
            }
            Lock::Plane(n) => {
                let a = axes[n.min(2)];
                d - a * d.dot(a)
            }
        };
    }
    let (o0, d0) = view.ray(start);
    let (o1, d1) = view.ray(pos);
    match lock {
        Lock::Axis(k) => {
            let a = axes[k.min(2)];
            match (closest_param_on_line(o0, d0, pivot, a), closest_param_on_line(o1, d1, pivot, a)) {
                (Some(t0), Some(t1)) => a * (t1 - t0),
                _ => DVec3::ZERO,
            }
        }
        Lock::Plane(n) => plane_delta(o0, d0, o1, d1, pivot, axes[n.min(2)]),
        Lock::Free => plane_delta(o0, d0, o1, d1, pivot, view.cam3d.forward()),
    }
}

fn plane_delta(o0: DVec3, d0: DVec3, o1: DVec3, d1: DVec3, p: DVec3, n: DVec3) -> DVec3 {
    match (ray_plane(o0, d0, p, n), ray_plane(o1, d1, p, n)) {
        (Some(a), Some(b)) => b - a,
        _ => DVec3::ZERO,
    }
}

pub fn rotation_axis(view: &ViewInfo, pivot: DVec3, axes: [DVec3; 3], lock: Lock) -> DVec3 {
    match lock {
        Lock::Axis(k) | Lock::Plane(k) => axes[k.min(2)],
        Lock::Free => toward_viewer(view, pivot),
    }
}

pub fn modal_angle(view: &ViewInfo, pivot: DVec3, axis: DVec3, start: Pos2, pos: Pos2, step: f64, free: bool) -> f64 {
    let Some(c) = view.world_to_screen(pivot) else { return 0.0 };
    if c.distance(pos) < 2.0 || c.distance(start) < 2.0 {
        return 0.0;
    }
    let sign = if axis.dot(toward_viewer(view, pivot)) >= 0.0 { 1.0 } else { -1.0 };
    let raw = crate::math::normalize_deg((screen_angle(c, pos) - screen_angle(c, start)) * sign);
    crate::math::normalize_deg(snap_angle(raw, step, free))
}

pub fn modal_factor(view: &ViewInfo, pivot: DVec3, start: Pos2, pos: Pos2, snapping: bool) -> f64 {
    let Some(c) = view.world_to_screen(pivot) else { return 1.0 };
    let d0 = f64::from(c.distance(start)).max(8.0);
    let f = f64::from(c.distance(pos)) / d0;
    let f = if snapping { (f * 10.0).round() / 10.0 } else { f };
    if f.is_finite() { f.clamp(0.01, 100.0) } else { 1.0 }
}

pub fn lock_scale(lock: Lock, factor: f64) -> DVec3 {
    match lock {
        Lock::Free => DVec3::splat(factor),
        Lock::Axis(k) => {
            let mut s = DVec3::ONE;
            s[k.min(2)] = factor;
            s
        }
        Lock::Plane(n) => {
            let mut s = DVec3::splat(factor);
            s[n.min(2)] = 1.0;
            s
        }
    }
}

const MAX_TYPED: usize = 16;

pub fn typed_push(buffer: &mut String, ch: char) -> bool {
    let ch = if ch == ',' { '.' } else { ch };
    let ok = match ch {
        '0'..='9' => true,
        '.' => !buffer.contains('.'),
        '-' => buffer.is_empty(),
        _ => false,
    };
    if ok && buffer.len() < MAX_TYPED {
        buffer.push(ch);
    }
    ok
}

pub fn typed_value(buffer: &str) -> Option<f64> {
    buffer.trim().parse::<f64>().ok().filter(|v| v.is_finite())
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum TypedKey {
    Char(char),
    Backspace,
}

fn digit_key(key: Key) -> Option<char> {
    const DIGITS: [Key; 10] =
        [Key::Num0, Key::Num1, Key::Num2, Key::Num3, Key::Num4, Key::Num5, Key::Num6, Key::Num7, Key::Num8, Key::Num9];
    DIGITS.iter().position(|k| *k == key).and_then(|i| char::from_digit(i as u32, 10))
}

fn key_char(key: Key, physical: Option<Key>) -> Option<char> {
    physical.and_then(digit_key).or_else(|| match key {
        Key::Minus => Some('-'),
        Key::Period | Key::Comma => Some('.'),
        k => digit_key(k),
    })
}

fn numeric_text(s: &str) -> bool {
    !s.is_empty() && s.chars().all(|c| c.is_ascii_digit() || matches!(c, '-' | '.' | ','))
}

pub fn take_typed(events: &mut Vec<Event>) -> Vec<TypedKey> {
    let mut out = Vec::new();
    let mut pending: Option<char> = None;
    events.retain(|e| match e {
        Event::Key { key, physical_key, pressed: true, modifiers, .. }
            if !modifiers.ctrl && !modifiers.alt && !modifiers.command && !modifiers.mac_cmd =>
        {
            if let Some(c) = pending.take() {
                out.push(TypedKey::Char(c));
            }
            if *key == Key::Backspace {
                out.push(TypedKey::Backspace);
                return false;
            }
            match key_char(*key, *physical_key) {
                Some(c) => {
                    pending = Some(c);
                    false
                }
                None => true,
            }
        }
        Event::Text(s) if numeric_text(s) => {
            pending = None;
            out.extend(s.chars().map(TypedKey::Char));
            false
        }
        Event::Text(_) => match pending.take() {
            Some(c) => {
                out.push(TypedKey::Char(c));
                false
            }
            None => true,
        },
        _ => true,
    });
    if let Some(c) = pending {
        out.push(TypedKey::Char(c));
    }
    out
}

pub fn typed_axis(view: &ViewInfo, pivot: DVec3, axes: [DVec3; 3], lock: Lock) -> DVec3 {
    match lock {
        Lock::Axis(k) | Lock::Plane(k) => axes[k.min(2)],
        Lock::Free if view.is_2d() => view.kind.toward_viewer(),
        Lock::Free => {
            let t = toward_viewer(view, pivot);
            let k = (0..3).max_by(|a, b| t.dot(axes[*a]).abs().total_cmp(&t.dot(axes[*b]).abs())).unwrap_or(2);
            let a = axes[k];
            if t.dot(a) < 0.0 { -a } else { a }
        }
    }
}

pub fn axis_label(axis: DVec3) -> String {
    let k = axis.abs().max_position();
    let name = ["X", "Y", "Z"][k];
    if axis[k] < 0.0 { format!("-{name}") } else { name.to_owned() }
}

#[derive(Clone, Copy, Debug, PartialEq)]
pub struct Modal {
    pub kind: ModalKind,
    pub lock: Lock,
    pub start: Pos2,
    pub axes: [DVec3; 3],
    pub delta: DVec3,
    pub angle: f64,
    pub axis: DVec3,
    pub scale: DVec3,
}

impl Modal {
    pub fn new(kind: ModalKind, start: Pos2, axes: [DVec3; 3]) -> Self {
        Self { kind, lock: Lock::Free, start, axes, delta: DVec3::ZERO, angle: 0.0, axis: DVec3::Z, scale: DVec3::ONE }
    }

    pub fn matrix(&self, pivot: DVec3) -> DMat4 {
        match self.kind {
            ModalKind::Move => DMat4::from_translation(self.delta),
            ModalKind::Rotate => rotation_about(pivot, self.axis, self.angle),
            ModalKind::Scale => scale_about(pivot, self.axes, self.scale),
        }
    }

    pub fn is_identity(&self) -> bool {
        match self.kind {
            ModalKind::Move => self.delta.length_squared() < 1e-12,
            ModalKind::Rotate => self.angle.abs() < 1e-9,
            ModalKind::Scale => (self.scale - DVec3::ONE).abs().max_element() < 1e-9,
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::gizmo::WORLD_AXES;
    use egui::{Rect, vec2};
    use glam::DVec2;
    use hp_app::ViewKind;
    use hp_app::view::{Camera2d, Camera3d};

    fn top() -> ViewInfo {
        ViewInfo {
            index: 1,
            kind: ViewKind::Top,
            rect: Rect::from_min_size(Pos2::ZERO, vec2(800.0, 600.0)),
            cam3d: Camera3d::default(),
            cam2d: Camera2d { center: DVec2::ZERO, zoom: 1.0 },
            pixels_per_point: 1.0,
            grid: 16.0,
        }
    }

    #[test]
    fn locks_toggle() {
        assert_eq!(Lock::Free.toggled(0, false), Lock::Axis(0));
        assert_eq!(Lock::Axis(0).toggled(0, false), Lock::Free);
        assert_eq!(Lock::Axis(0).toggled(2, true), Lock::Plane(2));
        assert_eq!(Lock::Plane(2).label(), "XY");
        assert_eq!(Lock::Axis(9).label(), "Z");
    }

    #[test]
    fn moves_in_a_2d_view() {
        let v = top();
        let c = v.world_to_screen(DVec3::ZERO).expect("center");
        let p = c + vec2(37.0, -21.0);
        assert_eq!(modal_move(&v, DVec3::ZERO, WORLD_AXES, Lock::Free, c, p, None), DVec3::new(37.0, 21.0, 0.0));
        assert_eq!(modal_move(&v, DVec3::ZERO, WORLD_AXES, Lock::Axis(0), c, p, None), DVec3::new(37.0, 0.0, 0.0));
        assert_eq!(modal_move(&v, DVec3::ZERO, WORLD_AXES, Lock::Axis(2), c, p, None), DVec3::new(0.0, 0.0, 21.0));
        assert_eq!(modal_move(&v, DVec3::ZERO, WORLD_AXES, Lock::Plane(1), c, p, None), DVec3::new(37.0, 0.0, 0.0));
        let snapped = MoveSnap::plain(DVec3::new(-256.0, 0.0, 0.0), 16.0);
        assert_eq!(
            modal_move(&v, DVec3::ZERO, WORLD_AXES, Lock::Free, c, p, Some(snapped)),
            DVec3::new(32.0, 16.0, 0.0)
        );
    }

    #[test]
    fn moves_in_the_3d_view() {
        let cam = Camera3d { pos: DVec3::new(0.0, -800.0, 0.0), yaw_deg: 90.0, pitch_deg: 0.0, ..Camera3d::default() };
        let v = ViewInfo { index: 0, kind: ViewKind::Perspective, cam3d: cam, ..top() };
        let c = v.world_to_screen(DVec3::ZERO).expect("center");
        let x = v.world_to_screen(DVec3::new(64.0, 0.0, 0.0)).expect("x");
        let d = modal_move(&v, DVec3::ZERO, WORLD_AXES, Lock::Free, c, x, None);
        assert!((d - DVec3::new(64.0, 0.0, 0.0)).length() < 1e-3, "{d:?}");
        let d = modal_move(&v, DVec3::ZERO, WORLD_AXES, Lock::Axis(2), c, x, None);
        assert!(d.length() < 1e-3, "sideways motion does not move along Z: {d:?}");
    }

    #[test]
    fn rotation_and_scale() {
        let v = top();
        let c = v.world_to_screen(DVec3::ZERO).expect("center");
        let start = c + vec2(100.0, 0.0);
        let end = c + vec2(0.0, -100.0);
        let axis = rotation_axis(&v, DVec3::ZERO, WORLD_AXES, Lock::Free);
        assert_eq!(axis, DVec3::Z);
        assert!((modal_angle(&v, DVec3::ZERO, axis, start, end, 15.0, false) - 90.0).abs() < 1e-9);
        assert!((modal_angle(&v, DVec3::ZERO, DVec3::NEG_Z, start, end, 15.0, false) + 90.0).abs() < 1e-9);
        assert_eq!(modal_angle(&v, DVec3::ZERO, axis, c, end, 15.0, false), 0.0, "start on the pivot");
        assert!((modal_factor(&v, DVec3::ZERO, start, c + vec2(150.0, 0.0), true) - 1.5).abs() < 1e-9);
        assert_eq!(modal_factor(&v, DVec3::ZERO, start, c, false), 0.01);
        assert_eq!(lock_scale(Lock::Axis(1), 2.0), DVec3::new(1.0, 2.0, 1.0));
        assert_eq!(lock_scale(Lock::Plane(1), 2.0), DVec3::new(2.0, 1.0, 2.0));
        let mut m = Modal::new(ModalKind::Scale, start, WORLD_AXES);
        assert!(m.is_identity());
        m.scale = DVec3::new(2.0, 1.0, 1.0);
        let mat = m.matrix(DVec3::new(10.0, 0.0, 0.0));
        assert!((mat.transform_point3(DVec3::new(20.0, 5.0, 0.0)) - DVec3::new(30.0, 5.0, 0.0)).length() < 1e-9);
    }

    #[test]
    fn typed_numbers_come_from_digits_text_and_the_numpad() {
        let mut b = String::new();
        for ch in ['-', '9', ',', '5'] {
            assert!(typed_push(&mut b, ch));
        }
        assert!(!typed_push(&mut b, '.'));
        assert!(!typed_push(&mut b, '-'));
        assert!(!typed_push(&mut b, 'x'));
        assert_eq!(typed_value(&b), Some(-9.5));
        assert_eq!(typed_value("-"), None);
        let k = |key: Key, physical: Option<Key>| Event::Key {
            key,
            physical_key: physical,
            pressed: true,
            repeat: false,
            modifiers: egui::Modifiers::NONE,
        };
        let mut events = vec![
            k(Key::Minus, None),
            Event::Text("-".to_owned()),
            k(Key::Num1, Some(Key::Num1)),
            Event::Text("&".to_owned()),
            k(Key::Num8, Some(Key::Num8)),
            k(Key::Backspace, None),
            Event::Text("0".to_owned()),
            k(Key::X, Some(Key::X)),
            Event::Text("x".to_owned()),
            k(Key::Period, None),
        ];
        let typed = take_typed(&mut events);
        use TypedKey::{Backspace, Char};
        assert_eq!(typed, vec![Char('-'), Char('1'), Char('8'), Backspace, Char('0'), Char('.')]);
        assert_eq!(events.len(), 2, "the axis key stays for the lock: {events:?}");
    }

    #[test]
    fn typed_angles_turn_around_a_world_axis_in_3d() {
        let v = top();
        assert_eq!(typed_axis(&v, DVec3::ZERO, WORLD_AXES, Lock::Free), DVec3::Z);
        assert_eq!(typed_axis(&v, DVec3::ZERO, WORLD_AXES, Lock::Axis(0)), DVec3::X);
        let cam =
            Camera3d { pos: DVec3::new(0.0, -800.0, 100.0), yaw_deg: 90.0, pitch_deg: -7.0, ..Camera3d::default() };
        let v3 = ViewInfo { index: 0, kind: ViewKind::Perspective, cam3d: cam, ..top() };
        let a = typed_axis(&v3, DVec3::ZERO, WORLD_AXES, Lock::Free);
        assert_eq!(a, DVec3::NEG_Y);
        assert_eq!(axis_label(a), "-Y");
        assert_eq!(typed_axis(&v3, DVec3::ZERO, WORLD_AXES, Lock::Plane(2)), DVec3::Z);
    }
}
