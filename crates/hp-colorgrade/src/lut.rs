use crate::{Grade, LUT_SIZE, RAW_BYTES, RGBA_BYTES};

pub fn is_raw(bytes: &[u8]) -> bool {
    bytes.len() == RAW_BYTES
}

fn grid_step() -> f32 {
    255.0 / (LUT_SIZE - 1) as f32
}

pub fn identity_raw() -> Vec<u8> {
    let step = grid_step();
    let mut v = Vec::with_capacity(RAW_BYTES);
    for i in 0..LUT_SIZE * LUT_SIZE * LUT_SIZE {
        let (r, g, b) = (i % LUT_SIZE, (i / LUT_SIZE) % LUT_SIZE, i / (LUT_SIZE * LUT_SIZE));
        v.extend([r, g, b].map(|c| (c as f32 * step).round() as u8));
    }
    v
}

pub fn generate(grade: &Grade, base: Option<&[u8]>) -> Vec<u8> {
    let engine = grade.engine();
    let base = base.filter(|b| is_raw(b));
    let n = LUT_SIZE * LUT_SIZE * LUT_SIZE;
    let mut out = Vec::with_capacity(RAW_BYTES);
    let scale = 1.0 / (LUT_SIZE - 1) as f32;
    for i in 0..n {
        let c = match base {
            Some(b) => [0, 1, 2].map(|k| f32::from(b[i * 3 + k]) / 255.0),
            None => {
                let (r, g, b) = (i % LUT_SIZE, (i / LUT_SIZE) % LUT_SIZE, i / (LUT_SIZE * LUT_SIZE));
                [r as f32 * scale, g as f32 * scale, b as f32 * scale]
            }
        };
        let o = engine.apply(c);
        out.extend(o.map(|v| (v * 255.0).round().clamp(0.0, 255.0) as u8));
    }
    out
}

pub fn to_rgba(raw: &[u8]) -> Option<Vec<u8>> {
    if !is_raw(raw) {
        return None;
    }
    let mut out = Vec::with_capacity(RGBA_BYTES);
    for px in raw.chunks_exact(3) {
        out.extend_from_slice(px);
        out.push(255);
    }
    Some(out)
}

pub fn from_rgba(rgba: &[u8]) -> Option<Vec<u8>> {
    if rgba.len() != RGBA_BYTES {
        return None;
    }
    Some(rgba.chunks_exact(4).flat_map(|p| [p[0], p[1], p[2]]).collect())
}

pub fn blend_rgba(luts: &[(&[u8], f32)]) -> Option<Vec<u8>> {
    let valid: Vec<(&[u8], f32)> = luts
        .iter()
        .filter(|(b, w)| is_raw(b) && w.is_finite() && *w > 0.0)
        .map(|(b, w)| (*b, w.clamp(0.0, 1.0)))
        .collect();
    if valid.is_empty() {
        return None;
    }
    let total: f32 = valid.iter().map(|(_, w)| *w).sum();
    let scale = if total > 1.0 { 1.0 / total } else { 1.0 };
    let identity = 1.0 - total.min(1.0);
    let n = LUT_SIZE * LUT_SIZE * LUT_SIZE;
    let step = grid_step();
    let mut out = Vec::with_capacity(RGBA_BYTES);
    for i in 0..n {
        let (r, g, b) = (i % LUT_SIZE, (i / LUT_SIZE) % LUT_SIZE, i / (LUT_SIZE * LUT_SIZE));
        let mut c = [r as f32 * step * identity, g as f32 * step * identity, b as f32 * step * identity];
        for (bytes, w) in &valid {
            let w = w * scale;
            for (k, v) in c.iter_mut().enumerate() {
                *v += f32::from(bytes[i * 3 + k]) * w;
            }
        }
        out.extend(c.map(|v| v.round().clamp(0.0, 255.0) as u8));
        out.push(255);
    }
    Some(out)
}

pub fn sample_rgb(raw: &[u8], rgb: [u8; 3]) -> [u8; 3] {
    if !is_raw(raw) {
        return rgb;
    }
    let max = (LUT_SIZE - 1) as f32;
    let pos = rgb.map(|v| f32::from(v) / 255.0 * max);
    let lo = pos.map(|p| (p.floor() as usize).min(LUT_SIZE - 2));
    let frac = [0, 1, 2].map(|k| pos[k] - lo[k] as f32);
    let at = |r: usize, g: usize, b: usize, k: usize| f32::from(raw[((b * LUT_SIZE + g) * LUT_SIZE + r) * 3 + k]);
    let mut out = [0u8; 3];
    for (k, o) in out.iter_mut().enumerate() {
        let mut acc = 0.0;
        for corner in 0..8 {
            let (dr, dg, db) = (corner & 1, (corner >> 1) & 1, (corner >> 2) & 1);
            let w = (if dr == 1 { frac[0] } else { 1.0 - frac[0] })
                * (if dg == 1 { frac[1] } else { 1.0 - frac[1] })
                * (if db == 1 { frac[2] } else { 1.0 - frac[2] });
            acc += w * at(lo[0] + dr, lo[1] + dg, lo[2] + db, k);
        }
        *o = acc.round().clamp(0.0, 255.0) as u8;
    }
    out
}

pub fn apply_to_rgba(raw: &[u8], pixels: &mut [u8]) {
    if !is_raw(raw) {
        return;
    }
    for px in pixels.chunks_exact_mut(4) {
        let o = sample_rgb(raw, [px[0], px[1], px[2]]);
        px[..3].copy_from_slice(&o);
    }
}

#[derive(Clone, Debug, PartialEq)]
pub struct LutStats {
    pub identity: bool,
    pub average: [f32; 3],
    pub mean_shift: f32,
    pub max_shift: f32,
}

pub fn stats(raw: &[u8]) -> Option<LutStats> {
    if !is_raw(raw) {
        return None;
    }
    let id = identity_raw();
    let n = (raw.len() / 3) as f32;
    let mut sum = [0.0_f32; 3];
    let mut shift = 0.0_f32;
    let mut max = 0.0_f32;
    for (px, base) in raw.chunks_exact(3).zip(id.chunks_exact(3)) {
        for k in 0..3 {
            sum[k] += f32::from(px[k]);
            let d = (f32::from(px[k]) - f32::from(base[k])).abs();
            shift += d;
            max = max.max(d);
        }
    }
    Some(LutStats { identity: max <= 1.0, average: sum.map(|s| s / n), mean_shift: shift / (n * 3.0), max_shift: max })
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::Colorize;

    #[test]
    fn neutral_grade_is_identity() {
        let lut = generate(&Grade::default(), None);
        assert_eq!(lut.len(), RAW_BYTES);
        assert_eq!(lut, identity_raw());
        assert!(stats(&lut).unwrap().identity);
    }

    #[test]
    fn grades_change_the_table() {
        let bright = generate(&Grade { brightness: 0.2, ..Grade::default() }, None);
        assert!(bright.iter().zip(identity_raw()).all(|(a, b)| *a >= b));
        let bw = generate(&Grade { saturation: 0.0, ..Grade::default() }, None);
        let last = bw.len() - 3;
        assert_eq!(&bw[last..], &[255, 255, 255]);
        let mid = ((16 * LUT_SIZE + 16) * LUT_SIZE + 16) * 3;
        assert!(bw[mid].abs_diff(bw[mid + 1]) <= 1 && bw[mid + 1].abs_diff(bw[mid + 2]) <= 1);
        let inv = generate(&Grade { invert: true, ..Grade::default() }, None);
        assert_eq!(&inv[..3], &[255, 255, 255]);
        let tinted =
            generate(&Grade { colorize: Colorize { color: [1.0, 0.0, 0.0], amount: 1.0 }, ..Grade::default() }, None);
        assert!(tinted.chunks_exact(3).all(|p| p[1] <= 1 && p[2] <= 1));
    }

    #[test]
    fn curve_and_levels() {
        let mut g = Grade::default();
        g.curves[0].points = vec![[0.0, 0.0], [0.5, 0.75], [1.0, 1.0]];
        let lut = generate(&g, None);
        let mid = ((16 * LUT_SIZE + 16) * LUT_SIZE + 16) * 3;
        assert!(lut[mid] > 150);
        let mut g = Grade::default();
        g.levels[1].in_white = 0.5;
        let lut = generate(&g, None);
        let red_half = 16 * 3;
        assert_eq!(lut[red_half], 255);
        assert_eq!(lut[red_half + 1], 0);
    }

    #[test]
    fn base_lut_is_graded_on_top() {
        let inv = generate(&Grade { invert: true, ..Grade::default() }, None);
        let back = generate(&Grade { invert: true, ..Grade::default() }, Some(&inv));
        assert_eq!(back, identity_raw());
    }

    #[test]
    fn blending_and_sampling() {
        let id = identity_raw();
        let inv = generate(&Grade { invert: true, ..Grade::default() }, None);
        assert!(blend_rgba(&[]).is_none());
        assert!(blend_rgba(&[(&id[..9], 1.0)]).is_none());
        let half = blend_rgba(&[(&inv, 0.5)]).unwrap();
        assert_eq!(half.len(), RGBA_BYTES);
        assert_eq!(&half[..4], &[128, 128, 128, 255]);
        assert_eq!(sample_rgb(&id, [10, 200, 77]), [10, 200, 77]);
        assert_eq!(sample_rgb(&inv, [0, 0, 0]), [255, 255, 255]);
        let rgba = to_rgba(&id).unwrap();
        assert_eq!(from_rgba(&rgba).unwrap(), id);
        let mut px = [0u8, 0, 0, 255, 255, 255, 255, 255];
        apply_to_rgba(&inv, &mut px);
        assert_eq!(px, [255, 255, 255, 255, 0, 0, 0, 255]);
    }
}
