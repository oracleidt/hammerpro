use glam::Vec3A;

use super::bake::{Ctx, Part, Sample};
use super::lights::Kind;

const REL: f32 = 0.015;
const ABS: f32 = 0.5;
const STEP: usize = 2;
const SKY_REL: f32 = 0.05;
const SKY_ABS: f32 = 1.0;

pub(crate) fn lights_of(ctx: &Ctx<'_>, cand: &[(usize, usize)]) -> Vec<(usize, usize)> {
    cand.iter()
        .copied()
        .filter(|&(li, _)| ctx.lights.list.get(li).is_some_and(|l| l.kind == Kind::SkyAmbient))
        .collect()
}

pub(crate) fn agree(buf: &[Vec3A], stride: usize, near: &[usize]) -> bool {
    agree_within(buf, stride, near, REL, ABS)
}

fn agree_within(buf: &[Vec3A], stride: usize, near: &[usize], rel: f32, abs: f32) -> bool {
    let Some(&first) = near.first() else { return false };
    (0..stride).all(|k| {
        let (mut lo, mut hi) = (buf[first * stride + k], buf[first * stride + k]);
        for &j in &near[1..] {
            lo = lo.min(buf[j * stride + k]);
            hi = hi.max(buf[j * stride + k]);
        }
        let limit = hi.abs().max(lo.abs()) * rel + Vec3A::splat(abs);
        (hi - lo).cmple(limit).all()
    })
}

fn axis(n: usize) -> Vec<usize> {
    let mut v: Vec<usize> = (0..n).step_by(STEP).collect();
    if n > 0 && v.last() != Some(&(n - 1)) {
        v.push(n - 1);
    }
    v
}

fn bracket(points: &[usize], x: usize) -> (usize, usize) {
    let k = points.partition_point(|p| *p <= x).saturating_sub(1);
    (points[k], points[(k + 1).min(points.len() - 1)])
}

pub(crate) fn face(
    ctx: &Ctx<'_>,
    lights: &[(usize, usize)],
    (slots, nn, w): (usize, usize, usize),
    samples: &[Option<Sample>],
    acc: &mut [Vec3A],
) -> Vec<Vec3A> {
    let stride = ctx.passes * slots * nn;
    let mut sky = vec![Vec3A::ZERO; samples.len() * stride];
    let w = w.max(1);
    let h = samples.len() / w;
    let mut done = vec![false; samples.len()];
    let trace = |i: usize, sky: &mut [Vec3A], done: &mut [bool]| {
        if let Some(sp) = &samples[i] {
            ctx.gather(lights, slots, nn, sp, Part::Ambient, &mut sky[i * stride..(i + 1) * stride]);
        }
        done[i] = true;
    };
    if w * h != samples.len() || w < 3 || h < 3 {
        for i in 0..samples.len() {
            trace(i, &mut sky, &mut done);
        }
    } else {
        let (xs, ys) = (axis(w), axis(h));
        for &t in &ys {
            for &s in &xs {
                trace(t * w + s, &mut sky, &mut done);
            }
        }
        let mut corners = vec![Vec3A::ZERO; 4 * stride];
        for t in 0..h {
            let (y0, y1) = bracket(&ys, t);
            for s in 0..w {
                let i = t * w + s;
                if done[i] {
                    continue;
                }
                let (x0, x1) = bracket(&xs, s);
                let ids = [y0 * w + x0, y0 * w + x1, y1 * w + x0, y1 * w + x1];
                let smooth = samples[i].is_some() && ids.iter().all(|&j| samples[j].is_some()) && {
                    for (k, &j) in ids.iter().enumerate() {
                        corners[k * stride..(k + 1) * stride].copy_from_slice(&sky[j * stride..(j + 1) * stride]);
                    }
                    agree_within(&corners, stride, &[0, 1, 2, 3], SKY_REL, SKY_ABS)
                };
                if !smooth {
                    trace(i, &mut sky, &mut done);
                    continue;
                }
                let fx = if x1 > x0 { (s - x0) as f32 / (x1 - x0) as f32 } else { 0.0 };
                let fy = if y1 > y0 { (t - y0) as f32 / (y1 - y0) as f32 } else { 0.0 };
                for k in 0..stride {
                    let top = corners[k].lerp(corners[stride + k], fx);
                    let bottom = corners[2 * stride + k].lerp(corners[3 * stride + k], fx);
                    sky[i * stride + k] = top.lerp(bottom, fy);
                }
                done[i] = true;
            }
        }
    }
    for (a, v) in acc.iter_mut().zip(&sky) {
        *a += *v;
    }
    sky
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn agreement() {
        let buf = [
            Vec3A::new(100.0, 50.0, 0.0),
            Vec3A::new(101.0, 50.4, 0.3),
            Vec3A::new(104.0, 50.0, 0.0),
            Vec3A::ZERO,
            Vec3A::splat(0.4),
            Vec3A::splat(0.6),
        ];
        assert!(agree(&buf, 1, &[0, 1]));
        assert!(!agree(&buf, 1, &[0, 2]));
        assert!(!agree(&buf, 1, &[0, 1, 2]));
        assert!(agree(&buf, 1, &[0]));
        assert!(!agree(&buf, 1, &[]));
        assert!(agree(&buf, 1, &[3, 4]));
        assert!(!agree(&buf, 1, &[3, 5]));
        assert!(!agree(&buf, 2, &[0, 1]));
    }
}
