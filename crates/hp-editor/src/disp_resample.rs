use glam::DVec3;
use hp_vmf::multiblend::{CHANNELS, MultiBlend};
use hp_vmf::{DispInfo, MAX_DISP_POWER, Solid};

use crate::document::Document;
use crate::selection::FaceRef;
use crate::{EditError, EditResult, Id};

const DEFAULT_TAG: i32 = 9;

#[derive(Clone, Copy, PartialEq, Eq)]
enum Interp {
    Linear,
    Cubic,
}

fn cells(power: u32) -> usize {
    1usize << power
}

fn fetch<const K: usize>(src: &[[f64; K]], n: usize, r: isize, c: isize) -> [f64; K] {
    let last = n as isize - 1;
    if r < 0 {
        let a = fetch(src, n, 0, c);
        let b = fetch(src, n, 1, c);
        return std::array::from_fn(|i| 2.0 * a[i] - b[i]);
    }
    if r > last {
        let a = fetch(src, n, last, c);
        let b = fetch(src, n, last - 1, c);
        return std::array::from_fn(|i| 2.0 * a[i] - b[i]);
    }
    if c < 0 {
        let a = fetch(src, n, r, 0);
        let b = fetch(src, n, r, 1);
        return std::array::from_fn(|i| 2.0 * a[i] - b[i]);
    }
    if c > last {
        let a = fetch(src, n, r, last);
        let b = fetch(src, n, r, last - 1);
        return std::array::from_fn(|i| 2.0 * a[i] - b[i]);
    }
    src.get(r as usize * n + c as usize).copied().unwrap_or([0.0; K])
}

fn catmull<const K: usize>(p: [[f64; K]; 4], t: f64) -> [f64; K] {
    let t2 = t * t;
    let t3 = t2 * t;
    std::array::from_fn(|i| {
        0.5 * (2.0 * p[1][i]
            + (-p[0][i] + p[2][i]) * t
            + (2.0 * p[0][i] - 5.0 * p[1][i] + 4.0 * p[2][i] - p[3][i]) * t2
            + (-p[0][i] + 3.0 * p[1][i] - 3.0 * p[2][i] + p[3][i]) * t3)
    })
}

fn resample_grid<const K: usize>(src: &[[f64; K]], old_p: u32, new_p: u32, mode: Interp) -> Vec<[f64; K]> {
    let n = cells(old_p) + 1;
    let m = cells(new_p) + 1;
    let mut out = Vec::with_capacity(m * m);
    if new_p <= old_p {
        let step = cells(old_p - new_p);
        for r in 0..m {
            for c in 0..m {
                out.push(fetch(src, n, (r * step) as isize, (c * step) as isize));
            }
        }
        return out;
    }
    let ratio = cells(new_p - old_p);
    for r in 0..m {
        let r0 = (r / ratio) as isize;
        let fr = (r % ratio) as f64 / ratio as f64;
        for c in 0..m {
            let c0 = (c / ratio) as isize;
            let fc = (c % ratio) as f64 / ratio as f64;
            let value = match mode {
                Interp::Linear => {
                    let a = fetch(src, n, r0, c0);
                    let b = fetch(src, n, r0, c0 + 1);
                    let d = fetch(src, n, r0 + 1, c0);
                    let e = fetch(src, n, r0 + 1, c0 + 1);
                    std::array::from_fn(|i| {
                        let top = a[i] + (b[i] - a[i]) * fc;
                        let bottom = d[i] + (e[i] - d[i]) * fc;
                        top + (bottom - top) * fr
                    })
                }
                Interp::Cubic => {
                    let rows: [[f64; K]; 4] = std::array::from_fn(|k| {
                        let rr = r0 - 1 + k as isize;
                        catmull(std::array::from_fn(|j| fetch(src, n, rr, c0 - 1 + j as isize)), fc)
                    });
                    catmull(rows, fr)
                }
            };
            out.push(value);
        }
    }
    out
}

fn vec3s(values: &[DVec3]) -> Vec<[f64; 3]> {
    values.iter().map(|v| v.to_array()).collect()
}

fn to_vec3s(values: Vec<[f64; 3]>) -> Vec<DVec3> {
    values.into_iter().map(DVec3::from_array).collect()
}

fn scalars(values: &[f64]) -> Vec<[f64; 1]> {
    values.iter().map(|v| [*v]).collect()
}

fn unit_or(v: DVec3, fallback: DVec3) -> DVec3 {
    let l = v.length();
    if l.is_finite() && l > 1e-9 { v / l } else { fallback }
}

fn resample_tags(tags: &[i32], old_p: u32, new_p: u32) -> Vec<i32> {
    let cells_old = cells(old_p);
    let cells_new = cells(new_p);
    let tag_at = |r: usize, c: usize, t: usize| tags.get(2 * (r * cells_old + c) + t).copied().unwrap_or(DEFAULT_TAG);
    let mut out = Vec::with_capacity(2 * cells_new * cells_new);
    for r in 0..cells_new {
        for c in 0..cells_new {
            let (sr, sc) = if new_p >= old_p {
                let ratio = cells(new_p - old_p);
                (r / ratio, c / ratio)
            } else {
                let step = cells(old_p - new_p);
                ((r * step + step / 2).min(cells_old - 1), (c * step + step / 2).min(cells_old - 1))
            };
            out.push(tag_at(sr, sc, 0));
            out.push(tag_at(sr, sc, 1));
        }
    }
    out
}

fn resample_multiblend(mb: &MultiBlend, old_p: u32, new_p: u32) -> MultiBlend {
    let mut out = MultiBlend::new(0);
    out.weights = resample_grid::<CHANNELS>(&mb.weights, old_p, new_p, Interp::Linear);
    out.alphas = resample_grid::<CHANNELS>(&mb.alphas, old_p, new_p, Interp::Linear);
    for (dst, src) in out.colors.iter_mut().zip(&mb.colors) {
        *dst = resample_grid::<3>(src, old_p, new_p, Interp::Linear);
    }
    out.normalize();
    out
}

fn well_formed(d: &DispInfo) -> bool {
    let count = d.row_len() * d.row_len();
    d.power >= 2
        && d.power <= MAX_DISP_POWER
        && d.normals.len() == count
        && d.distances.len() == count
        && d.offsets.len() == count
        && d.offset_normals.len() == count
        && d.alphas.len() == count
}

pub fn resample_power(d: &DispInfo, power: u32) -> Option<DispInfo> {
    if !(2..=MAX_DISP_POWER).contains(&power) || !well_formed(d) {
        return None;
    }
    if power == d.power {
        return Some(d.clone());
    }
    let (old_p, new_p) = (d.power, power);
    let fallback = d.normals.first().copied().unwrap_or(DVec3::Z);
    let mut out = d.clone();
    out.power = new_p;
    out.normals = to_vec3s(resample_grid(&vec3s(&d.normals), old_p, new_p, Interp::Linear))
        .into_iter()
        .map(|v| unit_or(v, fallback))
        .collect();
    out.offset_normals = to_vec3s(resample_grid(&vec3s(&d.offset_normals), old_p, new_p, Interp::Linear))
        .into_iter()
        .map(|v| unit_or(v, fallback))
        .collect();
    out.offsets = to_vec3s(resample_grid(&vec3s(&d.offsets), old_p, new_p, Interp::Cubic));
    out.distances =
        resample_grid(&scalars(&d.distances), old_p, new_p, Interp::Cubic).into_iter().map(|v| v[0]).collect();
    out.alphas = resample_grid(&scalars(&d.alphas), old_p, new_p, Interp::Linear)
        .into_iter()
        .map(|v| v[0].clamp(0.0, 255.0))
        .collect();
    out.triangle_tags = resample_tags(&d.triangle_tags, old_p, new_p);
    let blend = MultiBlend::of(d);
    hp_vmf::multiblend::remove_multiblend(&mut out);
    if let Some(mb) = blend {
        resample_multiblend(&mb, old_p, new_p).write_to(&mut out);
    }
    Some(out)
}

impl Document {
    pub fn resample_displacements(&mut self, faces: &[FaceRef], power: u32) -> EditResult<usize> {
        if !(2..=MAX_DISP_POWER).contains(&power) {
            return Err(hp_geom::GeomError::BadPower(power).into());
        }
        let mut updates: Vec<(Id, Solid)> = Vec::new();
        let mut count = 0;
        let mut seen = std::collections::HashSet::new();
        for f in faces {
            if !seen.insert(*f) {
                continue;
            }
            let Some(n) = self.scene.solids.get(&f.solid) else { continue };
            let Some(old) = n.solid.sides.get(f.side).and_then(|s| s.disp.as_deref()) else { continue };
            if old.power == power {
                continue;
            }
            let new =
                resample_power(old, power).ok_or_else(|| EditError::Invalid("malformed displacement".to_owned()))?;
            let idx = match updates.iter().position(|(id, _)| *id == f.solid) {
                Some(i) => i,
                None => {
                    updates.push((f.solid, n.solid.clone()));
                    updates.len() - 1
                }
            };
            if let Some(s) = updates.get_mut(idx).and_then(|(_, s)| s.sides.get_mut(f.side)) {
                s.disp = Some(Box::new(new));
                count += 1;
            }
        }
        if count == 0 {
            return Ok(0);
        }
        self.tx("Resample displacement power", |doc| {
            doc.update_solids(updates);
            Ok(count)
        })
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn ramp(power: u32) -> DispInfo {
        let mut d = DispInfo::new_flat(power, DVec3::ZERO, DVec3::Z);
        let n = d.row_len();
        for r in 0..n {
            for c in 0..n {
                let k = r * n + c;
                let u = c as f64 / (n - 1) as f64;
                let v = r as f64 / (n - 1) as f64;
                d.distances[k] = 10.0 * u + 4.0 * v;
                d.offsets[k] = DVec3::new(2.0 * u, 3.0 * v, 0.0);
                d.alphas[k] = 255.0 * u;
            }
        }
        d
    }

    fn close(a: f64, b: f64) -> bool {
        (a - b).abs() < 1e-6
    }

    #[test]
    fn flat_stays_flat() {
        let mut d = DispInfo::new_flat(2, DVec3::ZERO, DVec3::Z);
        d.distances.iter_mut().for_each(|v| *v = 7.0);
        for p in [3, 4] {
            let up = resample_power(&d, p).expect("up");
            assert_eq!(up.distances.len(), (cells(p) + 1).pow(2));
            assert_eq!(up.triangle_tags.len(), 2 * cells(p).pow(2));
            assert!(up.distances.iter().all(|v| close(*v, 7.0)));
            assert!(up.normals.iter().all(|v| v.abs_diff_eq(DVec3::Z, 1e-9)));
            assert!(up.offsets.iter().all(|v| v.length() < 1e-9));
        }
    }

    #[test]
    fn ramp_stays_a_ramp() {
        let d = ramp(2);
        for p in [3, 4] {
            let up = resample_power(&d, p).expect("up");
            let reference = ramp(p);
            for k in 0..reference.distances.len() {
                assert!(close(up.distances[k], reference.distances[k]));
                assert!(up.offsets[k].abs_diff_eq(reference.offsets[k], 1e-6));
                assert!(close(up.alphas[k], reference.alphas[k]));
            }
        }
        let down = resample_power(&ramp(4), 2).expect("down");
        let reference = ramp(2);
        for k in 0..reference.distances.len() {
            assert!(close(down.distances[k], reference.distances[k]));
            assert!(close(down.alphas[k], reference.alphas[k]));
        }
    }

    #[test]
    fn up_then_down_returns_the_original() {
        let mut d = ramp(2);
        d.distances[7] = 33.0;
        d.offsets[12] = DVec3::new(1.0, -5.0, 8.0);
        d.triangle_tags[3] = 1;
        d.allowed_verts = vec![5; 10];
        d.flags = 6;
        let back = resample_power(&resample_power(&d, 4).expect("up"), 2).expect("down");
        for k in 0..d.distances.len() {
            assert!(close(back.distances[k], d.distances[k]));
            assert!(back.offsets[k].abs_diff_eq(d.offsets[k], 1e-6));
            assert!(close(back.alphas[k], d.alphas[k]));
        }
        assert_eq!(back.triangle_tags.len(), d.triangle_tags.len());
        assert_eq!(back.allowed_verts, d.allowed_verts);
        assert_eq!(back.flags, 6);
        assert_eq!(back.power, 2);
    }

    #[test]
    fn multiblend_follows_the_power() {
        let mut d = ramp(2);
        let mut mb = MultiBlend::new(25);
        mb.weights[6] = [0.0, 0.0, 1.0, 0.0];
        assert!(mb.write_to(&mut d));
        let up = resample_power(&d, 3).expect("up");
        let got = MultiBlend::of(&up).expect("rows");
        assert_eq!(got.len(), 81);
        assert!(got.weights.iter().all(|w| close(w.iter().sum::<f64>(), 1.0)));
        assert!(close(got.weights[2 * 9 + 2][2], 1.0));
    }

    #[test]
    fn rejects_bad_power() {
        let d = ramp(2);
        assert!(resample_power(&d, 5).is_none());
        assert!(resample_power(&d, 1).is_none());
    }
}
