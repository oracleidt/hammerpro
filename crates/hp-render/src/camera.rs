use glam::{DVec3, Mat4, Vec3, Vec4};

use crate::types::{OrthoAxis, ViewKind, ViewParams};

pub(crate) const ORTHO_EYE_DISTANCE: f32 = 131_072.0;

impl OrthoAxis {
    pub const fn basis(self) -> ([f32; 3], [f32; 3], [f32; 3]) {
        match self {
            OrthoAxis::Top => ([1.0, 0.0, 0.0], [0.0, 1.0, 0.0], [0.0, 0.0, -1.0]),
            OrthoAxis::Front => ([0.0, 1.0, 0.0], [0.0, 0.0, 1.0], [-1.0, 0.0, 0.0]),
            OrthoAxis::Side => ([1.0, 0.0, 0.0], [0.0, 0.0, 1.0], [0.0, 1.0, 0.0]),
        }
    }
}

#[derive(Clone, Copy, Debug)]
pub(crate) struct Camera {
    pub view_proj: Mat4,
    pub eye: Vec3,
    pub right: Vec3,
    pub up: Vec3,
    pub forward: Vec3,
    pub is_ortho: bool,
    pub width: f32,
    pub height: f32,
    pub near: f32,
    pub far: f32,
    pub tan_half_fov: f32,
    pub ortho_center: [f32; 2],
    pub ppu: f32,
    pub half_px: [f32; 2],
    pub planes: [Vec4; 6],
    pub plane_count: usize,
}

fn finite_or(v: f32, fallback: f32) -> f32 {
    if v.is_finite() { v } else { fallback }
}

fn vec_or(v: [f32; 3], fallback: Vec3) -> Vec3 {
    let v = Vec3::from_array(v);
    if v.is_finite() { v } else { fallback }
}

impl Camera {
    pub fn new(params: &ViewParams) -> Self {
        let width = params.size[0].max(1) as f32;
        let height = params.size[1].max(1) as f32;
        match params.kind {
            ViewKind::Perspective { eye, forward, up, fov_y_deg, near, far } => {
                let eye = vec_or(eye, Vec3::ZERO);
                let mut fwd = vec_or(forward, Vec3::X).normalize_or_zero();
                if fwd == Vec3::ZERO {
                    fwd = Vec3::X;
                }
                let up_hint = vec_or(up, Vec3::Z).normalize_or_zero();
                let mut right = fwd.cross(up_hint).normalize_or_zero();
                if right == Vec3::ZERO {
                    let alt = if fwd.z.abs() < 0.9 { Vec3::Z } else { Vec3::Y };
                    right = fwd.cross(alt).normalize_or_zero();
                }
                let up = right.cross(fwd);
                let fov = finite_or(fov_y_deg, 70.0).clamp(1.0, 179.0).to_radians();
                let near = finite_or(near, 1.0).clamp(1e-3, 1e6);
                let far = finite_or(far, 65536.0).max(near * 1.01).min(1e8);
                let tan_half = (fov * 0.5).tan();
                let aspect = width / height;
                let grid = if params.game_pixel_grid { [1.0 / width, -1.0 / height] } else { [0.0, 0.0] };
                let view_proj = perspective_view_proj(eye, right, up, fwd, tan_half, aspect, near, far, grid);
                let planes = frustum_planes(&view_proj);
                Camera {
                    view_proj,
                    eye,
                    right,
                    up,
                    forward: fwd,
                    is_ortho: false,
                    width,
                    height,
                    near,
                    far,
                    tan_half_fov: tan_half,
                    ortho_center: [0.0; 2],
                    ppu: 1.0,
                    half_px: [width * 0.5, height * 0.5],
                    planes,
                    plane_count: 6,
                }
            }
            ViewKind::Ortho { axis, center, pixels_per_unit } => {
                let (r, u, f) = axis.basis();
                let (right, up, fwd) = (Vec3::from(r), Vec3::from(u), Vec3::from(f));
                let center = vec_or(center, Vec3::ZERO);
                let ppu = finite_or(pixels_per_unit, 1.0).clamp(1e-5, 1e5);
                let half_px = [(width * 0.5).floor() + 0.5, (height * 0.5).floor() + 0.5];
                let c = [center.dot(right), center.dot(up)];
                let view_proj = ortho_view_proj(right, up, c, ppu, half_px, width, height);
                let mut planes = frustum_planes(&view_proj);
                planes[4] = Vec4::new(0.0, 0.0, 0.0, 1.0);
                planes[5] = Vec4::new(0.0, 0.0, 0.0, 1.0);
                Camera {
                    view_proj,
                    eye: center - fwd * ORTHO_EYE_DISTANCE,
                    right,
                    up,
                    forward: fwd,
                    is_ortho: true,
                    width,
                    height,
                    near: 1.0,
                    far: ORTHO_EYE_DISTANCE * 2.0,
                    tan_half_fov: 1.0,
                    ortho_center: c,
                    ppu,
                    half_px,
                    planes,
                    plane_count: 4,
                }
            }
        }
    }

    pub fn aabb_visible(&self, min: Vec3, max: Vec3) -> bool {
        aabb_inside(&self.planes[..self.plane_count], min, max)
    }

    pub fn world_to_screen(&self, p: Vec3) -> Option<[f32; 2]> {
        let clip = self.view_proj * p.extend(1.0);
        if !self.is_ortho && clip.w <= 1e-6 {
            return None;
        }
        let ndc = clip.truncate() / clip.w;
        if !ndc.is_finite() {
            return None;
        }
        Some([(ndc.x + 1.0) * 0.5 * self.width, (1.0 - ndc.y) * 0.5 * self.height])
    }

    pub fn screen_ray(&self, px: [f32; 2]) -> (Vec3, Vec3) {
        if self.is_ortho {
            let p = self.screen_to_plane(px);
            (p - self.forward * ORTHO_EYE_DISTANCE, self.forward)
        } else {
            let nx = px[0] / self.width * 2.0 - 1.0;
            let ny = 1.0 - px[1] / self.height * 2.0;
            let aspect = self.width / self.height;
            let dir =
                self.forward + self.right * (nx * self.tan_half_fov * aspect) + self.up * (ny * self.tan_half_fov);
            (self.eye, dir.normalize_or(self.forward))
        }
    }

    pub fn screen_to_plane(&self, px: [f32; 2]) -> Vec3 {
        let r = self.ortho_center[0] + (px[0] - self.half_px[0]) / self.ppu;
        let u = self.ortho_center[1] - (px[1] - self.half_px[1]) / self.ppu;
        let depth = (self.eye + self.forward * ORTHO_EYE_DISTANCE).dot(self.forward);
        self.right * r + self.up * u + self.forward * depth
    }

    pub fn clipped_by(&self, plane: Vec4) -> Camera {
        if self.is_ortho || !plane.is_finite() || plane.dot(self.eye.extend(1.0)) >= 0.0 {
            return *self;
        }
        let aspect = self.width / self.height;
        let (tx, ty) = (self.tan_half_fov * aspect, self.tan_half_fov);
        let mut scale = f32::INFINITY;
        for depth in [self.near, self.far] {
            for sx in [-1.0f32, 1.0] {
                for sy in [-1.0f32, 1.0] {
                    let corner = self.eye + (self.forward + self.right * (sx * tx) + self.up * (sy * ty)) * depth;
                    let height = plane.dot(corner.extend(1.0));
                    if height > 1e-3 {
                        scale = scale.min(depth / height);
                    }
                }
            }
        }
        if !scale.is_finite() {
            return *self;
        }
        let m = self.view_proj;
        let z = m.row(3) - plane * scale;
        let view_proj = Mat4::from_cols(m.row(0), m.row(1), z, m.row(3)).transpose();
        let mut out = *self;
        out.view_proj = view_proj;
        out.planes = frustum_planes(&view_proj);
        out.plane_count = 6;
        out
    }
}

#[allow(clippy::too_many_arguments)]
pub(crate) fn perspective_view_proj(
    eye: Vec3,
    right: Vec3,
    up: Vec3,
    fwd: Vec3,
    tan_half: f32,
    aspect: f32,
    near: f32,
    far: f32,
    grid: [f32; 2],
) -> Mat4 {
    let (e, r, u, f) = (eye.as_dvec3(), right.as_dvec3(), up.as_dvec3(), fwd.as_dvec3());
    let (n, fa) = (f64::from(near), f64::from(far));
    let sy = 1.0 / f64::from(tan_half);
    let sx = sy / f64::from(aspect);
    let a = n / (fa - n);
    let b = n * fa / (fa - n);
    let (gx, gy) = (f64::from(grid[0]), f64::from(grid[1]));
    let row = |v: DVec3, w: f64| [v.x, v.y, v.z, w];
    let depth = row(f, -f.dot(e));
    let mut x = row(r * sx, -r.dot(e) * sx);
    let mut y = row(u * sy, -u.dot(e) * sy);
    for i in 0..4 {
        x[i] += depth[i] * gx;
        y[i] += depth[i] * gy;
    }
    let rows = [x, y, row(-f * a, a * f.dot(e) + b), depth];
    mat_from_rows(rows)
}

fn ortho_view_proj(right: Vec3, up: Vec3, c: [f32; 2], ppu: f32, half_px: [f32; 2], width: f32, height: f32) -> Mat4 {
    let (ppu, w, h) = (f64::from(ppu), f64::from(width), f64::from(height));
    let (r, u) = (right.as_dvec3(), up.as_dvec3());
    let kx = 2.0 * ppu / w;
    let ky = 2.0 * ppu / h;
    let ox = (f64::from(half_px[0]) - f64::from(c[0]) * ppu) * 2.0 / w - 1.0;
    let oy = 1.0 - (f64::from(half_px[1]) + f64::from(c[1]) * ppu) * 2.0 / h;
    let rows = [
        [r.x * kx, r.y * kx, r.z * kx, ox],
        [u.x * ky, u.y * ky, u.z * ky, oy],
        [0.0, 0.0, 0.0, 0.5],
        [0.0, 0.0, 0.0, 1.0],
    ];
    mat_from_rows(rows)
}

pub(crate) fn mat_from_rows(rows: [[f64; 4]; 4]) -> Mat4 {
    let col = |i: usize| Vec4::new(rows[0][i] as f32, rows[1][i] as f32, rows[2][i] as f32, rows[3][i] as f32);
    Mat4::from_cols(col(0), col(1), col(2), col(3))
}

pub(crate) fn aabb_inside(planes: &[Vec4], min: Vec3, max: Vec3) -> bool {
    for plane in planes {
        let n = plane.truncate();
        let p = Vec3::new(
            if n.x >= 0.0 { max.x } else { min.x },
            if n.y >= 0.0 { max.y } else { min.y },
            if n.z >= 0.0 { max.z } else { min.z },
        );
        if n.dot(p) + plane.w < 0.0 {
            return false;
        }
    }
    true
}

pub(crate) fn frustum_planes(m: &Mat4) -> [Vec4; 6] {
    let (r0, r1, r2, r3) = (m.row(0), m.row(1), m.row(2), m.row(3));
    let norm = |p: Vec4| {
        let l = p.truncate().length();
        if l > 1e-20 { p / l } else { Vec4::new(0.0, 0.0, 0.0, 1.0) }
    };
    [norm(r3 + r0), norm(r3 - r0), norm(r3 + r1), norm(r3 - r1), norm(r2), norm(r3 - r2)]
}

impl ViewParams {
    pub fn view_proj(&self) -> [[f32; 4]; 4] {
        Camera::new(self).view_proj.to_cols_array_2d()
    }

    pub fn world_to_screen(&self, p: [f32; 3]) -> Option<[f32; 2]> {
        Camera::new(self).world_to_screen(Vec3::from(p))
    }

    pub fn screen_ray(&self, px: [f32; 2]) -> ([f32; 3], [f32; 3]) {
        let (o, d) = Camera::new(self).screen_ray(px);
        (o.to_array(), d.to_array())
    }

    pub fn screen_to_world_2d(&self, px: [f32; 2]) -> Option<[f32; 3]> {
        match self.kind {
            ViewKind::Ortho { .. } => Some(Camera::new(self).screen_to_plane(px).to_array()),
            ViewKind::Perspective { .. } => None,
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::types::RenderMode;

    fn params(kind: ViewKind) -> ViewParams {
        ViewParams {
            size: [200, 100],
            kind,
            mode: RenderMode::Textured,
            grid: None,
            background: [0.0; 3],
            selection_color: [1.0, 0.0, 0.0, 0.5],
            show_sprites: true,
            show_models: true,
            msaa: false,
            game_pixel_grid: false,
        }
    }

    #[test]
    fn ortho_top_orientation() {
        let p = params(ViewKind::Ortho { axis: OrthoAxis::Top, center: [0.0; 3], pixels_per_unit: 1.0 });
        let c = p.world_to_screen([0.0, 0.0, 0.0]).unwrap_or_default();
        assert_eq!(c, [100.5, 50.5]);
        let x = p.world_to_screen([10.0, 0.0, 0.0]).unwrap_or_default();
        assert!(x[0] > c[0] && (x[1] - c[1]).abs() < 1e-3);
        let y = p.world_to_screen([0.0, 10.0, 0.0]).unwrap_or_default();
        assert!(y[1] < c[1], "+Y must go up on screen");
        let back = p.screen_to_world_2d([110.5, 40.5]).unwrap_or_default();
        assert!((back[0] - 10.0).abs() < 1e-3 && (back[1] - 10.0).abs() < 1e-3);
    }

    #[test]
    fn ortho_front_and_side_orientation() {
        let f = params(ViewKind::Ortho { axis: OrthoAxis::Front, center: [0.0; 3], pixels_per_unit: 2.0 });
        let c = f.world_to_screen([0.0; 3]).unwrap_or_default();
        let y = f.world_to_screen([0.0, 10.0, 0.0]).unwrap_or_default();
        let z = f.world_to_screen([0.0, 0.0, 10.0]).unwrap_or_default();
        assert!(y[0] > c[0] && z[1] < c[1]);
        let s = params(ViewKind::Ortho { axis: OrthoAxis::Side, center: [0.0; 3], pixels_per_unit: 2.0 });
        let x = s.world_to_screen([10.0, 0.0, 0.0]).unwrap_or_default();
        let z = s.world_to_screen([0.0, 0.0, 10.0]).unwrap_or_default();
        assert!(x[0] > c[0] && z[1] < c[1]);
    }

    #[test]
    fn the_game_pixel_grid_moves_the_view_by_half_a_pixel_like_the_engine() {
        let kind = ViewKind::Perspective {
            eye: [0.0, 0.0, 0.0],
            forward: [1.0, 0.0, 0.0],
            up: [0.0, 0.0, 1.0],
            fov_y_deg: 90.0,
            near: 1.0,
            far: 1000.0,
        };
        let plain = Camera::new(&params(kind));
        let grid = Camera::new(&ViewParams { game_pixel_grid: true, ..params(kind) });
        let p = Vec3::new(100.0, 13.0, 7.0);
        let a = plain.world_to_screen(p).expect("on screen");
        let b = grid.world_to_screen(p).expect("on screen");
        assert!((b[0] - a[0] - 0.5).abs() < 1e-3, "{a:?} {b:?}");
        assert!((b[1] - a[1] - 0.5).abs() < 1e-3, "{a:?} {b:?}");
        let ortho = Camera::new(&ViewParams {
            game_pixel_grid: true,
            ..params(ViewKind::Ortho { axis: OrthoAxis::Top, center: [0.0; 3], pixels_per_unit: 1.0 })
        });
        let o = Camera::new(&params(ViewKind::Ortho { axis: OrthoAxis::Top, center: [0.0; 3], pixels_per_unit: 1.0 }));
        let q = Vec3::new(3.0, 4.0, 0.0);
        assert_eq!(ortho.world_to_screen(q), o.world_to_screen(q));
    }

    #[test]
    fn perspective_projection_and_ray() {
        let p = params(ViewKind::Perspective {
            eye: [0.0, 0.0, 0.0],
            forward: [1.0, 0.0, 0.0],
            up: [0.0, 0.0, 1.0],
            fov_y_deg: 90.0,
            near: 1.0,
            far: 1000.0,
        });
        let s = p.world_to_screen([100.0, 0.0, 0.0]).unwrap_or_default();
        assert!((s[0] - 100.0).abs() < 1e-3 && (s[1] - 50.0).abs() < 1e-3);
        let l = p.world_to_screen([100.0, 10.0, 0.0]).unwrap_or_default();
        assert!(l[0] < s[0]);
        assert!(p.world_to_screen([-10.0, 0.0, 0.0]).is_none());
        let (o, d) = p.screen_ray([100.0, 50.0]);
        assert_eq!(o, [0.0; 3]);
        assert!((d[0] - 1.0).abs() < 1e-5);
        let m = Mat4::from_cols_array_2d(&p.view_proj());
        let near = m * Vec4::new(1.0, 0.0, 0.0, 1.0);
        let far = m * Vec4::new(1000.0, 0.0, 0.0, 1.0);
        assert!((near.z / near.w - 1.0).abs() < 1e-4);
        assert!((far.z / far.w).abs() < 1e-4);
    }

    #[test]
    fn frustum_culling() {
        let p = params(ViewKind::Perspective {
            eye: [0.0; 3],
            forward: [1.0, 0.0, 0.0],
            up: [0.0, 0.0, 1.0],
            fov_y_deg: 60.0,
            near: 1.0,
            far: 1000.0,
        });
        let cam = Camera::new(&p);
        assert!(cam.aabb_visible(Vec3::new(10.0, -1.0, -1.0), Vec3::new(12.0, 1.0, 1.0)));
        assert!(!cam.aabb_visible(Vec3::new(-12.0, -1.0, -1.0), Vec3::new(-10.0, 1.0, 1.0)));
        assert!(!cam.aabb_visible(Vec3::new(2000.0, -1.0, -1.0), Vec3::new(2010.0, 1.0, 1.0)));
        let o = params(ViewKind::Ortho { axis: OrthoAxis::Top, center: [0.0; 3], pixels_per_unit: 1.0 });
        let cam = Camera::new(&o);
        assert!(cam.aabb_visible(Vec3::new(-10.0, -10.0, -5000.0), Vec3::new(10.0, 10.0, 5000.0)));
        assert!(!cam.aabb_visible(Vec3::new(500.0, 0.0, 0.0), Vec3::new(510.0, 1.0, 1.0)));
    }

    #[test]
    fn degenerate_params_do_not_panic() {
        let p = ViewParams {
            size: [0, 0],
            ..params(ViewKind::Perspective {
                eye: [f32::NAN; 3],
                forward: [0.0; 3],
                up: [0.0; 3],
                fov_y_deg: f32::INFINITY,
                near: -1.0,
                far: 0.0,
            })
        };
        let cam = Camera::new(&p);
        assert!(cam.view_proj.is_finite());
        let o = ViewParams {
            kind: ViewKind::Ortho { axis: OrthoAxis::Side, center: [f32::NAN; 3], pixels_per_unit: 0.0 },
            ..p
        };
        assert!(Camera::new(&o).view_proj.is_finite());
    }
}
