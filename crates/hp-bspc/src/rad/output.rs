use glam::Vec3A;

use super::bake::FaceLight;
use super::data::{i16_at, i32_at, u16_at};
use super::surface::LitFace;
use crate::emit::lump;
use crate::materials::surf;

pub(crate) fn pow2(k: i32) -> f32 {
    if (-126..=127).contains(&k) { f32::from_bits(((k + 127) as u32) << 23) } else { 2f32.powi(k) }
}

fn pow2_f64(k: i32) -> f64 {
    if (-1022..=1023).contains(&k) { f64::from_bits(((k + 1023) as u64) << 52) } else { 2f64.powi(k) }
}

pub(crate) fn encode(v: Vec3A) -> [u8; 4] {
    let clean = |c: f32| if c.is_finite() { c.max(0.0) } else { 0.0 };
    let (x, y, z) = (clean(v.x), clean(v.y), clean(v.z));
    let max = if x > y {
        if x > z { x } else { z }
    } else if y > z {
        y
    } else {
        z
    };
    if max == 0.0 {
        return [0; 4];
    }
    let biased = ((max.to_bits() & 0x7F80_0000) >> 23) as i32;
    let exponent = if biased == 0 { -128 } else { (biased - 134).clamp(-128, 127) };
    let scalar = pow2_f64(-exponent);
    let m = |c: f32| (f64::from(c) * scalar).clamp(0.0, 255.0) as u8;
    [m(x), m(y), m(z), exponent as i8 as u8]
}

#[cfg(test)]
pub(crate) fn decode(c: [u8; 4]) -> Vec3A {
    let s = 2f32.powi(i32::from(c[3] as i8));
    Vec3A::new(f32::from(c[0]) * s, f32::from(c[1]) * s, f32::from(c[2]) * s)
}

pub(crate) struct Layout {
    pub faces: Vec<(usize, i32, [u8; 4])>,
    pub size: usize,
}

pub(crate) fn layout(faces: &[LitFace], lights: &[FaceLight]) -> Layout {
    let mut out = Vec::with_capacity(faces.len());
    let mut size = 0usize;
    for (f, l) in faces.iter().zip(lights) {
        let styles = l.styles.len();
        size += styles * 4;
        let mut s = [255u8; 4];
        for (k, v) in l.styles.iter().take(4).enumerate() {
            s[k] = *v;
        }
        out.push((f.face, size as i32, s));
        size += f.luxels() * 4 * styles * l.normals;
    }
    Layout { faces: out, size }
}

pub(crate) fn median(values: &[Vec3A]) -> Vec3A {
    if values.is_empty() {
        return Vec3A::ZERO;
    }
    let ch = |c: usize| {
        let mut v: Vec<f32> = values.iter().map(|x| x[c]).collect();
        v.sort_by(f32::total_cmp);
        v[v.len() / 2]
    };
    Vec3A::new(ch(0), ch(1), ch(2))
}

pub(crate) fn finish(v: Vec3A, minlight: f32) -> Vec3A {
    let c = |x: f32| if x.is_finite() { x.max(minlight).max(0.0) } else { minlight.max(0.0) };
    Vec3A::new(c(v.x), c(v.y), c(v.z))
}

pub(crate) fn lighting(faces: &[LitFace], lights: &[FaceLight], layout: &Layout, pass: usize) -> Vec<u8> {
    use rayon::prelude::*;
    let chunks: Vec<Vec<u8>> = faces
        .par_iter()
        .zip(lights)
        .map(|(f, l)| {
            let lux = f.luxels();
            let slots = l.styles.len();
            let mut out = vec![0u8; slots * 4 + lux * 4 * slots * l.normals];
            let ofs = slots * 4;
            for slot in 0..slots {
                let flat = l.index(pass, slot, 0, lux);
                let vals: Vec<Vec3A> =
                    l.values[flat..flat + lux].iter().map(|v| finish(Vec3A::from(*v), f.minlight)).collect();
                let at = ofs - 4 * (slot + 1);
                out[at..at + 4].copy_from_slice(&encode(median(&vals)));
                for n in 0..l.normals {
                    let src = l.index(pass, slot, n, lux);
                    let dst = ofs + ((slot * l.normals + n) * lux) * 4;
                    for i in 0..lux {
                        out[dst + i * 4..dst + i * 4 + 4]
                            .copy_from_slice(&encode(finish(Vec3A::from(l.values[src + i]), f.minlight)));
                    }
                }
            }
            out
        })
        .collect();
    let mut data = Vec::with_capacity(layout.size);
    for c in chunks {
        data.extend_from_slice(&c);
    }
    debug_assert_eq!(data.len(), layout.size);
    data
}

pub(crate) fn patch_faces(raw: &[u8], layout: &Layout) -> Vec<u8> {
    let mut out = raw.to_vec();
    for rec in out.chunks_exact_mut(56) {
        rec[16..20].copy_from_slice(&[255; 4]);
        rec[20..24].copy_from_slice(&(-1i32).to_le_bytes());
    }
    for (face, ofs, styles) in &layout.faces {
        if let Some(rec) = out.get_mut(face * 56..face * 56 + 56) {
            rec[16..20].copy_from_slice(styles);
            rec[20..24].copy_from_slice(&ofs.to_le_bytes());
        }
    }
    out
}

pub(crate) fn rescale_luxels(src: hp_bsp::Bsp, density: f32) -> Result<Vec<u8>, String> {
    let get = |i: usize| src.lump(i).map(|c| c.into_owned()).unwrap_or_default();
    let mut texinfo = get(lump::TEXINFO);
    for rec in texinfo.chunks_exact_mut(72) {
        for k in 0..8 {
            let at = 32 + k * 4;
            let v = f32::from_le_bytes([rec[at], rec[at + 1], rec[at + 2], rec[at + 3]]) * density;
            rec[at..at + 4].copy_from_slice(&v.to_le_bytes());
        }
    }
    let (verts, edges, surfedges) = (get(lump::VERTEXES), get(lump::EDGES), get(lump::SURFEDGES));
    let vertex = |i: usize| -> Option<[f64; 3]> {
        let at = i * 12;
        (at + 12 <= verts.len()).then(|| std::array::from_fn(|k| f64::from(super::data::f32_at(&verts, at + k * 4))))
    };
    let fix = |faces: &mut Vec<u8>| {
        for rec in faces.chunks_exact_mut(56) {
            let (first, count, ti) = (i32_at(rec, 4), i16_at(rec, 8), i16_at(rec, 10));
            let Ok(ti) = usize::try_from(ti) else { continue };
            let Some(t) = texinfo.get(ti * 72..ti * 72 + 72) else { continue };
            if (i32_at(t, 64) as u32) & (surf::SKY | surf::NOLIGHT) != 0 {
                continue;
            }
            let lm: [[f64; 4]; 2] = std::array::from_fn(|r| {
                std::array::from_fn(|k| f64::from(super::data::f32_at(t, 32 + (r * 4 + k) * 4)))
            });
            let (mut lo, mut hi) = ([f64::INFINITY; 2], [f64::NEG_INFINITY; 2]);
            for e in 0..usize::try_from(count).unwrap_or(0) {
                let Some(se) = usize::try_from(first)
                    .ok()
                    .map(|f| f + e)
                    .filter(|k| k * 4 + 4 <= surfedges.len())
                    .map(|k| i32_at(&surfedges, k * 4))
                else {
                    continue;
                };
                let (ei, side) = (se.unsigned_abs() as usize, usize::from(se < 0));
                if ei * 4 + 4 > edges.len() {
                    continue;
                }
                let Some(p) = vertex(usize::from(u16_at(&edges, ei * 4 + side * 2))) else { continue };
                for r in 0..2 {
                    let v = p[0] * lm[r][0] + p[1] * lm[r][1] + p[2] * lm[r][2] + lm[r][3];
                    lo[r] = lo[r].min(v);
                    hi[r] = hi[r].max(v);
                }
            }
            if !(lo[0].is_finite() && lo[1].is_finite() && hi[0].is_finite() && hi[1].is_finite()) {
                continue;
            }
            for r in 0..2 {
                let (mins, maxs) = (lo[r].floor(), hi[r].ceil());
                rec[28 + r * 4..32 + r * 4].copy_from_slice(&(mins as i32).to_le_bytes());
                rec[36 + r * 4..40 + r * 4].copy_from_slice(&((maxs - mins) as i32).to_le_bytes());
            }
        }
    };
    let mut lumps = Vec::new();
    for l in [lump::FACES, lump::FACES_HDR] {
        let mut faces = get(l);
        if faces.is_empty() {
            continue;
        }
        fix(&mut faces);
        lumps.push((l, faces, src.lump_info(l).map_or(1, |i| i.version)));
    }
    let version = src.lump_info(lump::TEXINFO).map_or(0, |i| i.version);
    lumps.push((lump::TEXINFO, texinfo, version));
    let mut edit = src.edit();
    let fail = |e: hp_bsp::BspError| e.to_string();
    for (l, data, version) in lumps {
        edit.replace_lump_with_version(l, data, version).map_err(fail)?;
    }
    edit.to_vec().map_err(fail)
}

pub fn check_lighting(bytes: &[u8]) -> Vec<String> {
    let mut errors = Vec::new();
    let bsp = match hp_bsp::Bsp::from_vec(bytes.to_vec()) {
        Ok(b) => b,
        Err(e) => return vec![format!("the BSP cannot be read: {e}")],
    };
    let get = |i: usize| bsp.lump(i).map(|c| c.into_owned()).unwrap_or_default();
    let texinfo = get(lump::TEXINFO);
    let leafs = get(lump::LEAFS);
    let leaf_size = if bsp.lump_info(lump::LEAFS).map_or(1, |l| l.version) == 0 { 56 } else { 32 };
    let n_leafs = leafs.len() / leaf_size;
    for (faces_lump, light_lump, name) in
        [(lump::FACES, lump::LIGHTING, "LDR"), (lump::FACES_HDR, lump::LIGHTING_HDR, "HDR")]
    {
        let faces = get(faces_lump);
        let light = get(light_lump);
        for i in 0..faces.len() / 56 {
            let o = i * 56;
            let ofs = i32_at(&faces, o + 20);
            if ofs < 0 {
                continue;
            }
            let styles = faces[o + 16..o + 20].iter().take_while(|s| **s != 255).count();
            let ti = i16_at(&faces, o + 10);
            let flags = usize::try_from(ti).ok().map_or(0, |t| i32_at(&texinfo, t * 72 + 64) as u32);
            let bumps = if flags & surf::BUMPLIGHT != 0 { 4 } else { 1 };
            let (w, h) = (i32_at(&faces, o + 36) + 1, i32_at(&faces, o + 40) + 1);
            if w < 1 || h < 1 {
                errors.push(format!("{name} face {i}: bad lightmap size"));
                continue;
            }
            let need = w as usize * h as usize * 4 * styles * bumps;
            if (ofs as usize) < styles * 4 || ofs as usize + need > light.len() {
                errors.push(format!("{name} face {i}: lightmap {ofs}+{need} outside of {} bytes", light.len()));
            }
            if errors.len() > 50 {
                return errors;
            }
        }
    }
    for (index_lump, light_lump, name) in [
        (lump::LEAF_AMBIENT_INDEX, lump::LEAF_AMBIENT_LIGHTING, "LDR"),
        (lump::LEAF_AMBIENT_INDEX_HDR, lump::LEAF_AMBIENT_LIGHTING_HDR, "HDR"),
    ] {
        let index = get(index_lump);
        let samples = get(light_lump);
        if index.is_empty() {
            continue;
        }
        if index.len() != n_leafs * 4 {
            errors.push(format!("{name} ambient index: {} entries for {n_leafs} leaves", index.len() / 4));
        }
        if samples.len() % 28 != 0 {
            errors.push(format!("{name} ambient samples: {} bytes, not a multiple of 28", samples.len()));
        }
        let n = samples.len() / 28;
        for l in 0..index.len() / 4 {
            let (count, first) = (usize::from(u16_at(&index, l * 4)), usize::from(u16_at(&index, l * 4 + 2)));
            if (count > 0 && first + count > n) || (count == 0 && first >= n_leafs.max(1)) {
                errors.push(format!("{name} ambient of leaf {l}: {first}+{count} out of range"));
                break;
            }
        }
    }
    for (i, name) in [(15usize, "LDR"), (54, "HDR")] {
        let size = hp_bsp::format::worldlight_size(bsp.lump_info(i).map_or(0, |l| l.version));
        if !get(i).len().is_multiple_of(size) {
            errors.push(format!("{name} world lights: not a multiple of {size} bytes"));
        }
    }
    errors
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn powers_of_two_are_exact() {
        for k in -200..200 {
            assert_eq!(pow2(k).to_bits(), 2f32.powi(k).to_bits(), "{k}");
            assert_eq!(pow2_f64(k).to_bits(), 2f64.powi(k).to_bits(), "{k}");
        }
    }

    #[test]
    fn rgbe_like_vrad() {
        assert_eq!(encode(Vec3A::ZERO), [0, 0, 0, 0]);
        assert_eq!(encode(Vec3A::splat(240.0)), [240, 240, 240, 0]);
        assert_eq!(encode(Vec3A::new(1000.0, 500.0, 0.0)), [250, 125, 0, 2]);
        assert_eq!(encode(Vec3A::new(1.5, 0.75, f32::NAN)), [192, 96, 0, (-7i8) as u8]);
        assert_eq!(encode(Vec3A::splat(-3.0)), [0, 0, 0, 0]);
        for v in [0.001f32, 0.3, 7.0, 255.0, 256.0, 1e6, 1e-30] {
            let d = decode(encode(Vec3A::splat(v)));
            assert!((d.x - v).abs() <= v / 128.0, "{v} {d}");
        }
        assert_eq!(encode(Vec3A::splat(f32::MAX)), [255, 255, 255, 120]);
        assert_eq!(median(&[Vec3A::ONE, Vec3A::ZERO, Vec3A::splat(5.0)]), Vec3A::ONE);
    }
}
