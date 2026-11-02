use glam::{DVec2, DVec3};
use hp_app::grid::GridFrame;
use hp_app::view::ViewInfo;
use hp_render::{Overlay, OverlayLine, OverlayPoint, OverlayTri};

pub const DROP_COLOR: [f32; 4] = [1.0, 0.55, 0.16, 1.0];

fn f(v: DVec3) -> [f32; 3] {
    v.as_vec3().to_array()
}

pub fn pixel_world_size(view: &ViewInfo, p: DVec3) -> f64 {
    let ppp = f64::from(view.pixels_per_point.max(0.1));
    if view.is_2d() {
        1.0 / (view.cam2d.zoom.max(1e-6) * ppp)
    } else {
        let h = f64::from(view.rect.height().max(1.0)) * ppp;
        let half = (view.cam3d.fov_y_deg.to_radians() * 0.5).tan().max(1e-3);
        let depth = (p - view.cam3d.pos).dot(view.cam3d.forward()).max(view.cam3d.near.max(0.01));
        2.0 * half * depth / h
    }
}

pub fn screen_axes(view: &ViewInfo) -> (DVec3, DVec3) {
    match view.kind.axes_2d() {
        Some((r, u, _)) => {
            let mut right = DVec3::ZERO;
            let mut up = DVec3::ZERO;
            right[r] = 1.0;
            up[u] = 1.0;
            (right, up)
        }
        None => (view.cam3d.right(), view.cam3d.up()),
    }
}

pub fn thick_line_on_top(out: &mut Overlay, view: &ViewInfo, a: DVec3, b: DVec3, color: [f32; 4]) {
    if !a.is_finite() || !b.is_finite() {
        return;
    }
    let (right, up) = screen_axes(view);
    let (pa, pb) = (pixel_world_size(view, a), pixel_world_size(view, b));
    out.lines_on_top.push(OverlayLine { a: f(a), b: f(b), color });
    for o in [right, -right, up, -up] {
        out.lines_on_top.push(OverlayLine { a: f(a + o * pa), b: f(b + o * pb), color });
    }
}

pub fn box_lines_on_top(out: &mut Overlay, min: DVec3, max: DVec3, color: [f32; 4]) {
    if !min.is_finite() || !max.is_finite() {
        return;
    }
    let c = |i: usize| {
        DVec3::new(
            if i & 1 == 0 { min.x } else { max.x },
            if i & 2 == 0 { min.y } else { max.y },
            if i & 4 == 0 { min.z } else { max.z },
        )
    };
    for (a, b) in [(0, 1), (2, 3), (4, 5), (6, 7), (0, 2), (1, 3), (4, 6), (5, 7), (0, 4), (1, 5), (2, 6), (3, 7)] {
        out.lines_on_top.push(OverlayLine { a: f(c(a)), b: f(c(b)), color });
    }
}

pub fn face_highlight(out: &mut Overlay, view: &ViewInfo, polygon: &[DVec3], color: [f32; 4]) {
    if polygon.len() < 3 || polygon.iter().any(|p| !p.is_finite()) {
        return;
    }
    let fill = [color[0], color[1], color[2], 0.35];
    let p0 = f(polygon[0]);
    for w in polygon[1..].windows(2) {
        out.triangles_on_top.push(OverlayTri { p: [p0, f(w[0]), f(w[1])], color: fill });
    }
    for i in 0..polygon.len() {
        let a = polygon[i];
        let b = polygon[(i + 1) % polygon.len()];
        thick_line_on_top(out, view, a, b, color);
    }
}

pub fn cross_marker(out: &mut Overlay, view: &ViewInfo, p: DVec3, size_px: f64, color: [f32; 4]) {
    if !p.is_finite() {
        return;
    }
    let s = pixel_world_size(view, p) * size_px;
    for axis in [DVec3::X, DVec3::Y, DVec3::Z] {
        out.lines_on_top.push(OverlayLine { a: f(p - axis * s), b: f(p + axis * s), color });
    }
    out.points.push(OverlayPoint { pos: f(p), color, size_px: 6.0 });
}

pub fn grid_3d_step(grid: f64, height: f64) -> f64 {
    let mut step = if grid.is_finite() && grid > 0.0 { grid } else { 16.0 };
    let target = (height.abs() / 24.0).max(1.0);
    while step < target && step < 4096.0 {
        step *= 2.0;
    }
    step
}

pub fn add_grid_3d(out: &mut Overlay, view: &ViewInfo, major_every: u32) {
    const HALF_LINES: i64 = 48;
    const MINOR: [f32; 4] = [0.6, 0.6, 0.65, 0.18];
    const MAJOR: [f32; 4] = [0.75, 0.75, 0.8, 0.35];
    const X_AXIS: [f32; 4] = [0.92, 0.31, 0.31, 0.6];
    const Y_AXIS: [f32; 4] = [0.43, 0.82, 0.35, 0.6];
    let cam = view.cam3d.pos;
    if !cam.is_finite() {
        return;
    }
    let step = grid_3d_step(view.grid, cam.z);
    let major = i64::from(major_every.max(1));
    let ix = (cam.x / step).round() as i64;
    let iy = (cam.y / step).round() as i64;
    let (cx, cy) = (ix as f64 * step, iy as f64 * step);
    let extent = step * HALF_LINES as f64;
    let color = |index: i64, axis: [f32; 4]| {
        if index == 0 {
            axis
        } else if index % major == 0 {
            MAJOR
        } else {
            MINOR
        }
    };
    for i in -HALF_LINES..=HALF_LINES {
        let (gx, gy) = (ix + i, iy + i);
        let (x, y) = (gx as f64 * step, gy as f64 * step);
        out.lines.push(OverlayLine {
            a: f(DVec3::new(x, cy - extent, 0.0)),
            b: f(DVec3::new(x, cy + extent, 0.0)),
            color: color(gx, Y_AXIS),
        });
        out.lines.push(OverlayLine {
            a: f(DVec3::new(cx - extent, y, 0.0)),
            b: f(DVec3::new(cx + extent, y, 0.0)),
            color: color(gy, X_AXIS),
        });
    }
}

const MAX_LATTICE_LINES: f64 = 400.0;
const LATTICE_DEPTH: f64 = -60000.0;

fn grid_color(frame: &GridFrame, major: bool) -> [f32; 4] {
    let alpha = if major { 0.45 } else { 0.2 };
    match frame.color {
        Some(c) => [f32::from(c[0]) / 255.0, f32::from(c[1]) / 255.0, f32::from(c[2]) / 255.0, alpha + 0.1],
        None => {
            let g = if major { 0.72 } else { 0.55 };
            [g, g, g + 0.04, alpha]
        }
    }
}

fn clip_line(p: DVec2, d: DVec2, min: DVec2, max: DVec2) -> Option<(f64, f64)> {
    let (mut t0, mut t1) = (f64::NEG_INFINITY, f64::INFINITY);
    for k in 0..2 {
        if d[k].abs() < 1e-12 {
            if p[k] < min[k] || p[k] > max[k] {
                return None;
            }
            continue;
        }
        let a = (min[k] - p[k]) / d[k];
        let b = (max[k] - p[k]) / d[k];
        t0 = t0.max(a.min(b));
        t1 = t1.min(a.max(b));
    }
    (t0 < t1 && t0.is_finite() && t1.is_finite()).then_some((t0, t1))
}

pub fn add_lattice_2d(out: &mut Overlay, view: &ViewInfo, frame: &GridFrame, major_every: u32, min_px: f64) {
    let Some((r, u, d)) = view.kind.axes_2d() else { return };
    let lattice = frame.lattice_2d(view.kind);
    let zoom = view.cam2d.zoom * f64::from(view.pixels_per_point.max(0.1));
    if !(zoom.is_finite() && zoom > 0.0) {
        return;
    }
    let half = DVec2::new(f64::from(view.rect.width()), f64::from(view.rect.height())) * 0.5 / view.cam2d.zoom;
    let mut lo = view.cam2d.center - half;
    let mut hi = view.cam2d.center + half;
    if let Some(b) = frame.bounds {
        lo = lo.max(DVec2::new(b.min[r], b.min[u]));
        hi = hi.min(DVec2::new(b.max[r], b.max[u]));
        if lo.x >= hi.x || lo.y >= hi.y {
            return;
        }
    }
    let area = (lattice.a.x * lattice.b.y - lattice.a.y * lattice.b.x).abs();
    if !(area.is_finite() && area > 1e-12) {
        return;
    }
    let major = u64::from(if frame.major_every > 0 { frame.major_every } else { major_every.max(1) });
    let depth = LATTICE_DEPTH * view.kind.toward_viewer()[d].signum();
    let to_world = |p: DVec2| {
        let mut w = DVec3::ZERO;
        w[r] = p.x;
        w[u] = p.y;
        w[d] = depth;
        w
    };
    let corners = [lo, DVec2::new(hi.x, lo.y), hi, DVec2::new(lo.x, hi.y)];
    for (step, along, index) in [(lattice.a, lattice.b, 0usize), (lattice.b, lattice.a, 1usize)] {
        let spacing = area / along.length().max(1e-12) * zoom;
        let mut mult: u64 = 1;
        while spacing * (mult as f64) < min_px && mult < (1 << 30) {
            mult *= 2;
        }
        let coords: Vec<f64> = corners.iter().filter_map(|c| lattice.coords(*c)).map(|c| c[index]).collect();
        if coords.len() < 4 {
            return;
        }
        let first = coords.iter().copied().fold(f64::INFINITY, f64::min).floor() as i64;
        let last = coords.iter().copied().fold(f64::NEG_INFINITY, f64::max).ceil() as i64;
        while ((last - first) as f64) / (mult as f64) > MAX_LATTICE_LINES {
            mult *= 2;
        }
        let m = mult as i64;
        let mut i = first.div_euclid(m) * m;
        while i <= last {
            let p = lattice.origin + step * i as f64;
            if let Some((t0, t1)) = clip_line(p, along, lo, hi) {
                let color = grid_color(frame, i % major as i64 == 0);
                out.lines.push(OverlayLine { a: f(to_world(p + along * t0)), b: f(to_world(p + along * t1)), color });
            }
            i += m;
        }
    }
}

pub fn add_grid_plane_3d(out: &mut Overlay, view: &ViewInfo, frame: &GridFrame, major_every: u32) {
    const HALF_LINES: i64 = 40;
    let cam = view.cam3d.pos;
    if !cam.is_finite() {
        return;
    }
    let local_cam = frame.to_local(cam);
    let height = local_cam.z.abs();
    let base = [frame.step.x, frame.step.y];
    let steps = base.map(|s| {
        let mut step = s;
        let target = (height / 24.0).max(s);
        while step < target && step < 4096.0 {
            step *= 2.0;
        }
        step
    });
    let (mut lo, mut hi) = (DVec2::splat(f64::NEG_INFINITY), DVec2::splat(f64::INFINITY));
    if let Some(b) = frame.bounds {
        let local: Vec<DVec3> = b.corners().iter().map(|c| frame.to_local(*c)).collect();
        lo = DVec2::new(
            local.iter().map(|p| p.x).fold(f64::INFINITY, f64::min),
            local.iter().map(|p| p.y).fold(f64::INFINITY, f64::min),
        );
        hi = DVec2::new(
            local.iter().map(|p| p.x).fold(f64::NEG_INFINITY, f64::max),
            local.iter().map(|p| p.y).fold(f64::NEG_INFINITY, f64::max),
        );
    }
    let major = i64::from(if frame.major_every > 0 { frame.major_every } else { major_every.max(1) });
    let center = [(local_cam.x / steps[0]).round() as i64, (local_cam.y / steps[1]).round() as i64];
    let extent = [steps[0] * HALF_LINES as f64, steps[1] * HALF_LINES as f64];
    for axis in 0..2 {
        let other = 1 - axis;
        for k in -HALF_LINES..=HALF_LINES {
            let index = center[axis] + k;
            let at = index as f64 * steps[axis];
            if at < lo[axis] - 1e-9 || at > hi[axis] + 1e-9 {
                continue;
            }
            let c = center[other] as f64 * steps[other];
            let from = (c - extent[other]).max(lo[other]);
            let to = (c + extent[other]).min(hi[other]);
            if from >= to {
                continue;
            }
            let point = |t: f64| {
                let mut l = DVec3::ZERO;
                l[axis] = at;
                l[other] = t;
                frame.to_world(l)
            };
            let color = grid_color(frame, index == 0 || index % major == 0);
            out.lines.push(OverlayLine { a: f(point(from)), b: f(point(to)), color });
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use egui::{Rect, pos2, vec2};
    use glam::DVec2;
    use hp_app::view::{Camera2d, Camera3d, ViewKind};

    fn view(kind: ViewKind) -> ViewInfo {
        ViewInfo {
            index: 0,
            kind,
            rect: Rect::from_min_size(pos2(0.0, 0.0), vec2(801.0, 601.0)),
            cam3d: Camera3d { pos: DVec3::new(0.0, 0.0, 128.0), yaw_deg: 0.0, pitch_deg: 0.0, ..Camera3d::default() },
            cam2d: Camera2d { center: DVec2::ZERO, zoom: 2.0 },
            pixels_per_point: 1.0,
            grid: 16.0,
        }
    }

    #[test]
    fn pixel_size_2d_and_3d() {
        assert!((pixel_world_size(&view(ViewKind::Top), DVec3::ZERO) - 0.5).abs() < 1e-12);
        let v = view(ViewKind::Perspective);
        let near = pixel_world_size(&v, DVec3::new(100.0, 0.0, 128.0));
        let far = pixel_world_size(&v, DVec3::new(1000.0, 0.0, 128.0));
        assert!((far / near - 10.0).abs() < 1e-9);
    }

    #[test]
    fn thick_lines_stay_one_pixel_wide_along_receding_edges() {
        let v = view(ViewKind::Perspective);
        let (a, b) = (DVec3::new(20.0, -40.0, 100.0), DVec3::new(2000.0, -40.0, 100.0));
        let mut o = Overlay::default();
        thick_line_on_top(&mut o, &v, a, b, DROP_COLOR);
        let screen =
            |p: [f32; 3]| v.world_to_screen(DVec3::new(p[0].into(), p[1].into(), p[2].into())).expect("screen");
        let (sa, sb) = (screen(o.lines_on_top[0].a), screen(o.lines_on_top[0].b));
        for l in &o.lines_on_top[1..] {
            let (da, db) = (screen(l.a).distance(sa), screen(l.b).distance(sb));
            assert!((da - 1.0).abs() < 0.05 && (db - 1.0).abs() < 0.05, "offsets {da} {db} px");
        }
    }

    #[test]
    fn boxes_faces_and_markers() {
        let mut o = Overlay::default();
        box_lines_on_top(&mut o, DVec3::splat(-8.0), DVec3::splat(8.0), DROP_COLOR);
        assert_eq!(o.lines_on_top.len(), 12);
        let mut o = Overlay::default();
        let quad = [DVec3::ZERO, DVec3::new(64.0, 0.0, 0.0), DVec3::new(64.0, 64.0, 0.0), DVec3::new(0.0, 64.0, 0.0)];
        face_highlight(&mut o, &view(ViewKind::Perspective), &quad, DROP_COLOR);
        assert_eq!(o.triangles_on_top.len(), 2);
        assert_eq!(o.lines_on_top.len(), 4 * 5);
        let mut o = Overlay::default();
        face_highlight(&mut o, &view(ViewKind::Perspective), &quad[..2], DROP_COLOR);
        assert!(o.is_empty());
        cross_marker(&mut o, &view(ViewKind::Front), DVec3::new(1.0, 2.0, 3.0), 8.0, DROP_COLOR);
        assert_eq!(o.lines_on_top.len(), 3);
        assert_eq!(o.points.len(), 1);
    }

    #[test]
    fn grid_3d() {
        assert_eq!(grid_3d_step(16.0, 100.0), 16.0);
        assert_eq!(grid_3d_step(16.0, 2000.0), 128.0);
        assert_eq!(grid_3d_step(f64::NAN, 0.0), 16.0);
        assert!(grid_3d_step(1.0, 1e12) <= 4096.0);
        let mut o = Overlay::default();
        add_grid_3d(&mut o, &view(ViewKind::Perspective), 8);
        assert_eq!(o.lines.len(), 2 * 97);
    }
}
