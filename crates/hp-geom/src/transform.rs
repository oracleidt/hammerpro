use glam::{DMat3, DMat4, DVec3, DVec4};
use hp_vmf::{DispInfo, Side, Solid, TexAxis};

use crate::texture::{effective_scale, ensure_valid_axes};
use crate::util::{clean_decimal, clean_point, clean_unit};
use crate::{GeomError, Plane, build_solid};

struct Xf {
    m: DMat4,
    l: DMat3,
    linv_t: DMat3,
    t: DVec3,
    mirror: bool,
    rigid: bool,
}

impl Xf {
    fn new(m: DMat4) -> Result<Self, GeomError> {
        if !m.is_finite() {
            return Err(GeomError::BadTransform);
        }
        let last = m.row(3);
        if !last.abs_diff_eq(DVec4::new(0.0, 0.0, 0.0, 1.0), 1e-9) {
            return Err(GeomError::BadTransform);
        }
        let l = DMat3::from_mat4(m);
        let l = DMat3::from_cols(clean_unit(l.x_axis), clean_unit(l.y_axis), clean_unit(l.z_axis));
        let t = m.w_axis.truncate();
        let t = DVec3::new(clean_decimal(t.x), clean_decimal(t.y), clean_decimal(t.z));
        let det = l.determinant();
        if !det.is_finite() || det.abs() < 1e-12 {
            return Err(GeomError::BadTransform);
        }
        let m = DMat4::from_cols(l.x_axis.extend(0.0), l.y_axis.extend(0.0), l.z_axis.extend(0.0), t.extend(1.0));
        let linv_t = l.inverse().transpose();
        let rigid = (l.transpose() * l).abs_diff_eq(DMat3::IDENTITY, 1e-9);
        Ok(Self { m, l, linv_t, t, mirror: det < 0.0, rigid })
    }

    #[inline]
    fn point(&self, p: DVec3) -> DVec3 {
        clean_point(self.m.transform_point3(p))
    }
}

#[inline]
fn is_unit_factor(k: f64) -> bool {
    (k - 1.0).abs() <= 1e-9
}

pub fn transform_solid(
    solid: &mut Solid,
    m: DMat4,
    texture_lock: bool,
    texture_scale_lock: bool,
) -> Result<(), GeomError> {
    transform_sides(&mut solid.sides, m, texture_lock, texture_scale_lock, None)
}

pub fn transform_solid_anchored(
    solid: &mut Solid,
    m: DMat4,
    texture_lock: bool,
    texture_scale_lock: bool,
    anchor: DVec3,
) -> Result<(), GeomError> {
    transform_sides(&mut solid.sides, m, texture_lock, texture_scale_lock, Some(anchor))
}

pub fn transform_sides(
    sides: &mut [Side],
    m: DMat4,
    texture_lock: bool,
    texture_scale_lock: bool,
    anchor: Option<DVec3>,
) -> Result<(), GeomError> {
    let xf = Xf::new(m)?;
    let anchor = if texture_lock && !texture_scale_lock && !xf.rigid {
        anchor.unwrap_or_else(|| sides_center(sides))
    } else {
        anchor.unwrap_or(DVec3::ZERO)
    };
    for side in sides.iter_mut() {
        transform_side(side, &xf, texture_lock, texture_scale_lock, anchor);
    }
    if sides.iter().any(|s| s.disp.is_some()) {
        let geom = build_solid(sides);
        for (i, side) in sides.iter_mut().enumerate() {
            if let (Some(d), Some(face)) = (side.disp.as_deref_mut(), geom.face_for_side(i)) {
                crate::update_disp_tags(d, &face.vertices);
            }
        }
    }
    Ok(())
}

pub fn translate_solid(solid: &mut Solid, delta: DVec3, texture_lock: bool) {
    translate_sides(&mut solid.sides, delta, texture_lock);
}

pub fn translate_sides(sides: &mut [Side], delta: DVec3, texture_lock: bool) {
    if !delta.is_finite() {
        return;
    }
    for side in sides.iter_mut() {
        for p in &mut side.plane {
            *p = clean_point(*p + delta);
        }
        if texture_lock {
            for a in [&mut side.uaxis, &mut side.vaxis] {
                a.shift = clean_decimal(a.shift - delta.dot(a.axis) / effective_scale(a.scale));
            }
        }
        if let Some(vp) = &mut side.vertices_plus {
            for v in vp.iter_mut() {
                *v = clean_point(*v + delta);
            }
        }
        if let Some(d) = &mut side.disp {
            d.start_position = clean_point(d.start_position + delta);
        }
    }
}

fn sides_center(sides: &[Side]) -> DVec3 {
    let g = build_solid(sides);
    if g.valid {
        return g.aabb.center();
    }
    let pts: Vec<DVec3> = sides.iter().flat_map(|s| s.plane).filter(|p| p.is_finite()).collect();
    crate::util::polygon_center(&pts)
}

fn transform_side(side: &mut Side, xf: &Xf, lock: bool, scale_lock: bool, anchor: DVec3) {
    let old_normal = Plane::from_side(side).map(|p| p.normal);
    let mut pts = side.plane.map(|p| xf.point(p));
    if xf.mirror {
        pts.swap(0, 2);
    }
    side.plane = pts;
    if lock {
        lock_axis(&mut side.uaxis, xf, scale_lock, anchor);
        lock_axis(&mut side.vaxis, xf, scale_lock, anchor);
    } else if scale_lock {
        for a in [&mut side.uaxis, &mut side.vaxis] {
            let k = (xf.linv_t * a.axis).length();
            if k.is_finite() && k > 1e-12 && !is_unit_factor(k) {
                a.scale = clean_decimal(effective_scale(a.scale) / k);
            }
        }
    }
    if let Some(p) = Plane::from_side(side) {
        ensure_valid_axes(side, p.normal);
    }
    if let Some(vp) = &mut side.vertices_plus {
        for v in vp.iter_mut() {
            *v = xf.point(*v);
        }
        if xf.mirror {
            vp.reverse();
        }
    }
    if let Some(d) = &mut side.disp {
        transform_disp(d, xf, old_normal);
    }
}

fn lock_axis(a: &mut TexAxis, xf: &Xf, scale_lock: bool, anchor: DVec3) {
    let s = effective_scale(a.scale);
    let a2 = xf.linv_t * a.axis;
    let k = a2.length();
    if !k.is_finite() || k < 1e-12 {
        return;
    }
    let dir = clean_unit(a2 / k);
    if scale_lock || xf.rigid {
        if is_unit_factor(k) {
            a.shift = clean_decimal(a.shift - xf.t.dot(dir) / s);
            a.scale = s;
        } else {
            a.shift = clean_decimal(a.shift - xf.t.dot(a2) / s);
            a.scale = clean_decimal(s / k);
        }
        a.axis = dir;
    } else {
        let u_anchor = anchor.dot(a.axis) / s + a.shift;
        let moved = xf.m.transform_point3(anchor);
        a.axis = dir;
        a.scale = s;
        a.shift = clean_decimal(u_anchor - moved.dot(dir) / s);
    }
}

fn transform_disp(d: &mut DispInfo, xf: &Xf, old_face_normal: Option<DVec3>) {
    d.start_position = xf.point(d.start_position);
    for (n, dist) in d.normals.iter_mut().zip(d.distances.iter_mut()) {
        let v = xf.l * *n;
        let len = v.length();
        if len.is_finite() && len > 1e-12 {
            *n = clean_unit(v / len);
            if !is_unit_factor(len) {
                *dist *= len;
            }
        }
    }
    for o in d.offsets.iter_mut() {
        *o = xf.l * *o;
    }
    for n in d.offset_normals.iter_mut() {
        if let Some(t) = (xf.linv_t * *n).try_normalize() {
            *n = clean_unit(t);
        }
    }
    if let Some(fnorm) = old_face_normal {
        let k = (xf.linv_t * fnorm).length();
        if k.is_finite() && k > 1e-12 && !is_unit_factor(k) {
            d.elevation /= k;
        }
    }
    if xf.mirror {
        let n = d.row_len();
        transpose_grid(&mut d.normals, n);
        transpose_grid(&mut d.distances, n);
        transpose_grid(&mut d.offsets, n);
        transpose_grid(&mut d.offset_normals, n);
        transpose_grid(&mut d.alphas, n);
        transpose_tags(&mut d.triangle_tags, n.saturating_sub(1));
        if let Some(mut blend) = hp_vmf::multiblend::MultiBlend::of(d) {
            transpose_grid(&mut blend.weights, n);
            transpose_grid(&mut blend.alphas, n);
            for colors in &mut blend.colors {
                transpose_grid(colors, n);
            }
            blend.write_to(d);
        }
    }
}

pub(crate) fn transpose_grid<T: Copy>(v: &mut [T], n: usize) {
    if n == 0 || v.len() != n * n {
        return;
    }
    for i in 0..n {
        for j in (i + 1)..n {
            v.swap(i * n + j, j * n + i);
        }
    }
}

fn transpose_tags(tags: &mut [i32], quads: usize) {
    if quads == 0 || tags.len() != 2 * quads * quads {
        return;
    }
    for i in 0..quads {
        for j in (i + 1)..quads {
            let a = 2 * (i * quads + j);
            let b = 2 * (j * quads + i);
            tags.swap(a, b);
            tags.swap(a + 1, b + 1);
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::test_support::*;
    use crate::{FaceGeom, texel_uv};
    use glam::{DQuat, DVec2};

    fn uv_snapshot(sides: &[Side]) -> Vec<(usize, Vec<(DVec3, DVec2)>)> {
        let g = build_solid(sides);
        assert!(g.valid);
        g.faces
            .iter()
            .map(|f: &FaceGeom| {
                let s = &sides[f.side_index];
                (f.side_index, f.vertices.iter().map(|&v| (v, texel_uv(s, v))).collect())
            })
            .collect()
    }

    fn check_glued(before: &[(usize, Vec<(DVec3, DVec2)>)], after: &[Side], m: DMat4) {
        for (si, verts) in before {
            for (p, uv) in verts {
                let q = m.transform_point3(*p);
                let uv2 = texel_uv(&after[*si], q);
                assert!((uv2 - *uv).length() < 1e-6, "side {si}: {uv:?} -> {uv2:?}");
            }
        }
    }

    fn assert_outward(sides: &[Side]) {
        let g = build_solid(sides);
        assert!(g.valid);
        for f in &g.faces {
            let n = crate::polygon_normal(&f.vertices).expect("normal");
            assert!((n - f.plane.normal).length() < 1e-6);
            let c = g.aabb.center();
            assert!(f.plane.distance(c) < 0.0, "normal must point away from the center");
        }
    }

    fn textured_box() -> Solid {
        let mut sides = hammer_box(DVec3::new(-32.0, 0.0, 8.0), DVec3::new(96.0, 48.0, 72.0));
        for (i, s) in sides.iter_mut().enumerate() {
            s.uaxis.shift = 3.0 * i as f64;
            s.vaxis.shift = -7.0 + i as f64;
            s.uaxis.scale = 0.25 + 0.05 * i as f64;
        }
        Solid { id: 1, sides, ..Default::default() }
    }

    #[test]
    fn texture_lock_translation() {
        let mut s = textured_box();
        let before = uv_snapshot(&s.sides);
        let d = DVec3::new(13.0, -27.5, 64.0);
        translate_solid(&mut s, d, true);
        check_glued(&before, &s.sides, DMat4::from_translation(d));
        let mut s2 = textured_box();
        transform_solid(&mut s2, DMat4::from_translation(d), true, false).expect("transform");
        check_glued(&before, &s2.sides, DMat4::from_translation(d));
        let mut s3 = textured_box();
        translate_solid(&mut s3, d, false);
        let p = DVec3::new(1.0, 2.0, 3.0);
        assert_eq!(texel_uv(&s3.sides[0], p), texel_uv(&textured_box().sides[0], p));
    }

    #[test]
    fn texture_lock_rotation() {
        let base = textured_box();
        let before = uv_snapshot(&base.sides);
        let q = DQuat::from_axis_angle(DVec3::new(0.3, -0.5, 0.8).normalize(), 0.7);
        let m = DMat4::from_rotation_translation(q, DVec3::new(10.0, 20.0, -5.0));
        let mut s = base.clone();
        transform_solid(&mut s, m, true, false).expect("transform");
        check_glued(&before, &s.sides, m);
        assert_outward(&s.sides);
        let m90 = DMat4::from_rotation_z(90f64.to_radians());
        let mut s = base;
        transform_solid(&mut s, m90, true, false).expect("transform");
        check_glued(&before, &s.sides, m90);
        for side in &s.sides {
            for p in side.plane {
                assert_eq!(p, p.round());
            }
            for c in side.uaxis.axis.to_array() {
                assert!(c == 0.0 || c.abs() == 1.0);
            }
        }
    }

    fn editor_rotation(pivot: DVec3, axis: DVec3, degrees: f64) -> DMat4 {
        let q = DQuat::from_axis_angle(axis, degrees.to_radians());
        DMat4::from_translation(pivot) * DMat4::from_quat(q) * DMat4::from_translation(-pivot)
    }

    #[test]
    fn right_angle_turns_write_clean_values() {
        let cube = Solid {
            id: 1,
            sides: hammer_box(DVec3::new(-64.0, -64.0, 0.0), DVec3::new(64.0, 64.0, 128.0)),
            ..Default::default()
        };
        let before = uv_snapshot(&cube.sides);
        let pivot = DVec3::new(0.0, 0.0, 64.0);
        for (axis, deg) in [(DVec3::Y, 90.0), (DVec3::Y, -90.0), (DVec3::X, 90.0), (DVec3::Z, 180.0), (DVec3::Z, 270.0)]
        {
            let m = editor_rotation(pivot, axis, deg);
            for scale_lock in [false, true] {
                let mut s = cube.clone();
                transform_solid_anchored(&mut s, m, true, scale_lock, pivot).expect("transform");
                check_glued(&before, &s.sides, m);
                for side in &s.sides {
                    for p in side.plane {
                        assert_eq!(p, p.round(), "{axis} {deg}: plane point {p}");
                    }
                    for a in [&side.uaxis, &side.vaxis] {
                        assert_eq!(a.scale, 0.25, "{axis} {deg}: scale is kept exactly");
                        assert_eq!(a.shift, a.shift.round(), "{axis} {deg}: shift {}", a.shift);
                        assert!(a.axis.to_array().iter().all(|c| *c == 0.0 || c.abs() == 1.0), "{:?}", a.axis);
                    }
                }
            }
        }
        let mut s = cube.clone();
        transform_solid_anchored(&mut s, editor_rotation(pivot, DVec3::Y, 90.0), true, false, pivot)
            .expect("transform");
        assert!(s.sides.iter().any(|side| side.uaxis.axis == DVec3::NEG_Z && side.uaxis.shift == 256.0));
        let odd_pivot = DVec3::new(-123.456_789_123, 37.25, 64.0);
        let quarter = editor_rotation(odd_pivot, DVec3::Z, 90.0);
        let mut s = cube.clone();
        for _ in 0..4 {
            transform_solid_anchored(&mut s, quarter, true, false, odd_pivot).expect("transform");
            let mut xs: Vec<f64> = s.sides.iter().flat_map(|side| side.plane.map(|p| p.x)).collect();
            xs.sort_by(f64::total_cmp);
            xs.dedup();
            assert_eq!(xs.len(), 2, "{xs:?}");
            assert!(s.sides.iter().all(|side| side.uaxis.scale == 0.25 && side.vaxis.scale == 0.25));
        }
        for (a, b) in s.sides.iter().zip(&cube.sides) {
            for (p, q) in a.plane.iter().zip(&b.plane) {
                assert!(p.distance(*q) < 1e-9, "{p} vs {q}");
            }
            assert_eq!((a.uaxis.axis, a.vaxis.axis), (b.uaxis.axis, b.vaxis.axis));
            assert!((a.uaxis.shift - b.uaxis.shift).abs() < 1e-9 && (a.vaxis.shift - b.vaxis.shift).abs() < 1e-9);
        }
        let mut s = cube.clone();
        let mut d = DispInfo::new_flat(2, DVec3::new(-64.0, 64.0, 128.0), DVec3::Z);
        d.distances[3] = 10.0;
        d.elevation = 3.0;
        s.sides[0].disp = Some(Box::new(d));
        transform_solid(&mut s, editor_rotation(pivot, DVec3::Y, 90.0), true, false).expect("transform");
        let d = s.sides[0].disp.as_ref().expect("disp");
        assert_eq!((d.distances[3], d.elevation, d.normals[3]), (10.0, 3.0, DVec3::X));
    }

    #[test]
    fn texture_scale_lock() {
        let base = textured_box();
        let before = uv_snapshot(&base.sides);
        let m = DMat4::from_scale_rotation_translation(
            DVec3::new(2.0, 0.5, 1.5),
            DQuat::from_rotation_z(0.3),
            DVec3::new(5.0, 6.0, 7.0),
        );
        let mut s = base.clone();
        transform_solid(&mut s, m, true, true).expect("transform");
        check_glued(&before, &s.sides, m);
        assert_outward(&s.sides);
        let scale = DMat4::from_scale(DVec3::new(2.0, 1.0, 1.0));
        let mut s = base.clone();
        transform_solid(&mut s, scale, true, false).expect("transform");
        for (a, b) in s.sides.iter().zip(&base.sides) {
            assert_eq!(a.uaxis.scale, b.uaxis.scale);
        }
        let center = DVec3::new(32.0, 24.0, 40.0);
        let moved = scale.transform_point3(center);
        for (a, b) in s.sides.iter().zip(&base.sides) {
            assert!((texel_uv(a, moved) - texel_uv(b, center)).length() < 1e-9);
        }
        let mut s = base.clone();
        transform_solid(&mut s, scale, false, true).expect("transform");
        assert!((s.sides[0].uaxis.scale - base.sides[0].uaxis.scale * 2.0).abs() < 1e-12);
        assert!((s.sides[0].vaxis.scale - base.sides[0].vaxis.scale).abs() < 1e-12);
    }

    #[test]
    fn mirror_keeps_normals_outward() {
        let base = textured_box();
        let m = DMat4::from_scale(DVec3::new(-1.0, 1.0, 1.0));
        let mut s = base.clone();
        s.sides[0].vertices_plus = Some(vec![DVec3::ZERO, DVec3::X, DVec3::Y]);
        transform_solid(&mut s, m, true, true).expect("transform");
        assert_outward(&s.sides);
        let g0 = build_solid(&base.sides);
        let g1 = build_solid(&s.sides);
        assert!((g0.volume() - g1.volume()).abs() < 1e-6);
        assert_eq!(g1.aabb.min.x, -96.0);
        assert_eq!(
            s.sides[0].vertices_plus.as_deref(),
            Some(&[DVec3::new(0.0, 1.0, 0.0), DVec3::new(-1.0, 0.0, 0.0), DVec3::ZERO][..])
        );
        let before = uv_snapshot(&base.sides);
        check_glued(&before, &s.sides, m);
    }

    #[test]
    fn bad_matrices_rejected() {
        let mut s = textured_box();
        let orig = s.clone();
        assert_eq!(
            transform_solid(&mut s, DMat4::from_scale(DVec3::new(1.0, 0.0, 1.0)), true, true),
            Err(GeomError::BadTransform)
        );
        let mut proj = DMat4::IDENTITY;
        proj.x_axis.w = 0.5;
        assert_eq!(transform_solid(&mut s, proj, false, false), Err(GeomError::BadTransform));
        let nan = DMat4::from_translation(DVec3::new(f64::NAN, 0.0, 0.0));
        assert_eq!(transform_solid(&mut s, nan, false, false), Err(GeomError::BadTransform));
        assert_eq!(s, orig);
    }

    #[test]
    fn rotation_without_lock_fixes_axes() {
        let mut s = textured_box();
        let m = DMat4::from_rotation_x(90f64.to_radians());
        transform_solid(&mut s, m, false, false).expect("transform");
        for side in &s.sides {
            let n = Plane::from_side(side).expect("plane").normal;
            assert!(crate::texture_axes_valid(side, n));
        }
    }

    #[test]
    fn displacement_transform() {
        let mut s = textured_box();
        let mut d = DispInfo::new_flat(2, DVec3::new(-32.0, 0.0, 72.0), DVec3::Z);
        d.distances[3] = 10.0;
        d.alphas[1] = 255.0;
        let mut blend = hp_vmf::multiblend::MultiBlend::new(25);
        blend.weights[1] = [0.0, 0.0, 1.0, 0.0];
        blend.colors[3][1] = [0.5, 0.25, 1.0];
        assert!(blend.write_to(&mut d));
        s.sides[0].disp = Some(Box::new(d));
        let m = DMat4::from_translation(DVec3::new(0.0, 0.0, 16.0)) * DMat4::from_scale(DVec3::new(1.0, 1.0, 2.0));
        transform_solid(&mut s, m, true, true).expect("transform");
        let d = s.sides[0].disp.as_ref().expect("disp");
        assert_eq!(d.start_position, DVec3::new(-32.0, 0.0, 160.0));
        assert_eq!(d.distances[3], 20.0);
        assert_eq!(d.normals[3], DVec3::Z);
        let mx = DMat4::from_scale(DVec3::new(-1.0, 1.0, 1.0));
        transform_solid(&mut s, mx, false, false).expect("transform");
        let d = s.sides[0].disp.as_ref().expect("disp");
        assert_eq!(d.distances[3 * 5], 20.0);
        assert_eq!(d.alphas[5], 255.0);
        let blend = hp_vmf::multiblend::MultiBlend::of(d).expect("4-way rows kept");
        assert_eq!((blend.weights[5], blend.colors[3][5]), ([0.0, 0.0, 1.0, 0.0], [0.5, 0.25, 1.0]), "follows the alpha");
        assert_eq!(blend.weights[1], [1.0, 0.0, 0.0, 0.0]);
    }

    #[test]
    fn rotation_updates_displacement_tags() {
        let mut s = textured_box();
        let face = build_solid(&s.sides).face_for_side(0).expect("top").vertices.clone();
        let d = crate::create_disp(&face, 2).expect("disp");
        assert!(d.triangle_tags.iter().all(|&t| t == 9));
        s.sides[0].disp = Some(Box::new(d));
        let m = DMat4::from_rotation_x(90f64.to_radians());
        transform_solid(&mut s, m, true, false).expect("transform");
        assert!(s.sides[0].disp.as_ref().expect("disp").triangle_tags.iter().all(|&t| t == 0));
        transform_solid(&mut s, m.inverse(), true, false).expect("transform");
        assert!(s.sides[0].disp.as_ref().expect("disp").triangle_tags.iter().all(|&t| t == 9));
    }

    #[test]
    fn transpose_helpers() {
        let mut g = vec![0, 1, 2, 3, 4, 5, 6, 7, 8];
        transpose_grid(&mut g, 3);
        assert_eq!(g, vec![0, 3, 6, 1, 4, 7, 2, 5, 8]);
        let mut short = vec![1, 2, 3];
        transpose_grid(&mut short, 3);
        assert_eq!(short, vec![1, 2, 3]);
        let mut tags = vec![1, 2, 3, 4, 5, 6, 7, 8];
        transpose_tags(&mut tags, 2);
        assert_eq!(tags, vec![1, 2, 5, 6, 3, 4, 7, 8]);
    }
}
