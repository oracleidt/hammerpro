use glam::Vec3A;

use crate::sampling::luminance;
use crate::scene::Scene;

const KERNEL: [f32; 5] = [1.0 / 16.0, 1.0 / 4.0, 3.0 / 8.0, 1.0 / 4.0, 1.0 / 16.0];

pub struct Geometry {
    pub width: usize,
    pub height: usize,
    pub pos: Vec<Vec3A>,
    pub normal: Vec<Vec3A>,
    pub valid: Vec<bool>,
    pub spacing: f32,
    pub channels: usize,
    pub probe: usize,
}

impl Geometry {
    pub fn of(scene: &Scene, si: usize) -> Geometry {
        let s = &scene.surfaces[si];
        let (w, h) = (s.width as usize, s.height as usize);
        let lux = w * h;
        let mut pos = vec![Vec3A::ZERO; lux];
        let mut normal = vec![Vec3A::Z; lux];
        let mut valid = vec![false; lux];
        for i in 0..lux {
            let l = &scene.luxels[s.first_luxel as usize + i];
            if l.points == 0 {
                continue;
            }
            let mut c = Vec3A::ZERO;
            for k in 0..l.points {
                c += Vec3A::from_array(scene.points[(l.first_point + k) as usize].pos);
            }
            pos[i] = c / l.points as f32;
            normal[i] = Vec3A::from_array(scene.points[l.first_point as usize].normals[0]);
            valid[i] = true;
        }
        let mut sum = 0.0f32;
        let mut count = 0u32;
        for y in 0..h {
            for x in 0..w {
                let i = y * w + x;
                if !valid[i] {
                    continue;
                }
                if x + 1 < w && valid[i + 1] {
                    sum += pos[i].distance(pos[i + 1]);
                    count += 1;
                }
                if y + 1 < h && valid[i + w] {
                    sum += pos[i].distance(pos[i + w]);
                    count += 1;
                }
            }
        }
        let spacing = if count > 0 { (sum / count as f32).max(0.5) } else { 16.0 };
        let passes = scene.passes as usize;
        let channels = passes * s.styles.len().max(1) * s.normals.max(1) as usize;
        let probe = (passes - 1) * s.styles.len().max(1) * s.normals.max(1) as usize;
        Geometry { width: w, height: h, pos, normal, valid, spacing, channels, probe }
    }
}

pub fn denoise(g: &Geometry, values: &mut [[f32; 3]], var: &[f32], strength: f32, keep_edges: bool) {
    let (w, h) = (g.width, g.height);
    let lux = w * h;
    if strength <= 0.0 || lux < 2 || values.len() < g.channels * lux {
        return;
    }
    let iterations = if keep_edges { 2 } else { 4 };
    let lum_sigma = if keep_edges { 1.5 } else { 4.0 } * strength;
    let mut cur: Vec<Vec3A> = values.iter().map(|v| Vec3A::from_array(*v)).collect();
    let mut v: Vec<f32> = var.to_vec();
    v.resize(lux, 0.0);
    let mut next = cur.clone();
    let mut next_var = v.clone();
    for it in 0..iterations {
        let step = 1usize << it;
        let pos_sigma = g.spacing * (step as f32 * 1.5 + 1.0) * strength.max(0.25);
        for y in 0..h {
            for x in 0..w {
                let i = y * w + x;
                if !g.valid[i] {
                    continue;
                }
                let lp = luminance(cur[g.probe * lux + i]);
                let sd = v[i].max(0.0).sqrt();
                let lscale = lum_sigma * sd + 1e-3 + if keep_edges { 0.0 } else { 0.02 * lp.abs() * strength };
                let mut wsum = 0.0f32;
                let mut vsum = 0.0f32;
                let mut acc = [Vec3A::ZERO; 64];
                let ch = g.channels.min(64);
                for (dy, ky) in KERNEL.iter().enumerate() {
                    let yy = y as isize + (dy as isize - 2) * step as isize;
                    if yy < 0 || yy >= h as isize {
                        continue;
                    }
                    for (dx, kx) in KERNEL.iter().enumerate() {
                        let xx = x as isize + (dx as isize - 2) * step as isize;
                        if xx < 0 || xx >= w as isize {
                            continue;
                        }
                        let j = yy as usize * w + xx as usize;
                        if !g.valid[j] {
                            continue;
                        }
                        let dp = g.pos[i].distance_squared(g.pos[j]);
                        let wp = (-dp / (2.0 * pos_sigma * pos_sigma)).exp();
                        let wn = g.normal[i].dot(g.normal[j]).max(0.0).powi(32);
                        let lq = luminance(cur[g.probe * lux + j]);
                        let wl = (-(lp - lq).abs() / lscale).exp();
                        let wt = kx * ky * wp * wn * wl;
                        if wt <= 1e-6 {
                            continue;
                        }
                        wsum += wt;
                        vsum += wt * wt * v[j];
                        for c in 0..ch {
                            acc[c] += cur[c * lux + j] * wt;
                        }
                    }
                }
                if wsum > 0.0 {
                    for c in 0..ch {
                        next[c * lux + i] = acc[c] / wsum;
                    }
                    next_var[i] = vsum / (wsum * wsum);
                }
            }
        }
        std::mem::swap(&mut cur, &mut next);
        std::mem::swap(&mut v, &mut next_var);
        next.copy_from_slice(&cur);
        next_var.copy_from_slice(&v);
    }
    for (o, c) in values.iter_mut().zip(&cur) {
        *o = c.to_array();
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn geo(w: usize, h: usize) -> Geometry {
        let lux = w * h;
        Geometry {
            width: w,
            height: h,
            pos: (0..lux).map(|i| Vec3A::new((i % w) as f32 * 8.0, (i / w) as f32 * 8.0, 0.0)).collect(),
            normal: vec![Vec3A::Z; lux],
            valid: vec![true; lux],
            spacing: 8.0,
            channels: 1,
            probe: 0,
        }
    }

    #[test]
    fn noise_drops_and_edges_stay() {
        let (w, h) = (32, 32);
        let g = geo(w, h);
        let mut rng = crate::sampling::Rng::new(1, 2, 3);
        let truth = |x: usize| if x < 16 { 10.0f32 } else { 100.0 };
        let mut vals: Vec<[f32; 3]> = (0..w * h)
            .map(|i| {
                let t = truth(i % w);
                let n = (rng.uniform() - 0.5) * 0.4 * t;
                [t + n; 3]
            })
            .collect();
        let var: Vec<f32> = (0..w * h).map(|i| (0.4 * truth(i % w)).powi(2) / 12.0).collect();
        let err = |v: &[[f32; 3]]| -> f32 {
            v.iter().enumerate().map(|(i, c)| (c[0] - truth(i % w)).abs() / truth(i % w)).sum::<f32>() / v.len() as f32
        };
        let before = err(&vals);
        denoise(&g, &mut vals, &var, 1.0, false);
        let after = err(&vals);
        assert!(after < before * 0.5, "{before} {after}");
        for y in 2..h - 2 {
            assert!(vals[y * w + 14][0] < 30.0, "{}", vals[y * w + 14][0]);
            assert!(vals[y * w + 17][0] > 70.0, "{}", vals[y * w + 17][0]);
        }
    }
}
