use glam::Vec3A;

pub(crate) const HEADER: usize = 88;
pub(crate) const FACES: usize = 7;
pub(crate) const FORMAT_DXT1: u32 = 13;
pub(crate) const FORMAT_RGBA16161616F: u32 = 24;
pub(crate) const FLAGS_LDR: u32 = 0x400c;
pub(crate) const FLAGS_HDR: u32 = 0x600c;
const GAMMA: f32 = 1.0 / 2.2;
const HALF_MAX: f32 = 65_504.0;

pub(crate) fn finite(c: Vec3A) -> Vec3A {
    let f = |v: f32| if v.is_nan() { 0.0 } else { v.clamp(0.0, HALF_MAX) };
    Vec3A::new(f(c.x), f(c.y), f(c.z))
}

pub(crate) fn mip_count(size: u32) -> u32 {
    32 - size.max(1).leading_zeros()
}

pub(crate) fn mips(face: &[Vec3A], size: u32) -> Vec<Vec<Vec3A>> {
    let mut out = vec![face.to_vec()];
    let mut side = size as usize;
    while side > 1 {
        let half = side / 2;
        let prev = out.last().expect("level");
        let mut next = Vec::with_capacity(half * half);
        for y in 0..half {
            for x in 0..half {
                let at = |dx: usize, dy: usize| prev[(y * 2 + dy) * side + x * 2 + dx];
                next.push((at(0, 0) + at(1, 0) + at(0, 1) + at(1, 1)) * 0.25);
            }
        }
        out.push(next);
        side = half;
    }
    out
}

pub(crate) fn f32_to_f16(v: f32) -> u16 {
    let v = if v.is_nan() { 0.0 } else { v.clamp(0.0, HALF_MAX) };
    let bits = v.to_bits();
    let exp = ((bits >> 23) & 0xff) as i32 - 127 + 15;
    let man = bits & 0x7f_ffff;
    if v == 0.0 || exp < -10 {
        return 0;
    }
    if exp <= 0 {
        let full = man | 0x80_0000;
        let shift = (14 - exp) as u32;
        let m = full >> shift;
        let rest = full & ((1 << shift) - 1);
        let halfway = 1 << (shift - 1);
        let round = rest > halfway || (rest == halfway && (m & 1) != 0);
        return (m + u32::from(round)) as u16;
    }
    let mut h = ((exp as u32) << 10) | (man >> 13);
    let rest = man & 0x1fff;
    if rest > 0x1000 || (rest == 0x1000 && (h & 1) != 0) {
        h += 1;
    }
    h.min(0x7bff) as u16
}

pub(crate) fn f16_to_f32(h: u16) -> f32 {
    let exp = i32::from((h >> 10) & 0x1f);
    let man = f32::from(h & 0x3ff);
    let sign = if h & 0x8000 != 0 { -1.0 } else { 1.0 };
    match exp {
        0 => sign * man * (1.0 / 16_777_216.0),
        31 => 0.0,
        e => sign * (1.0 + man / 1024.0) * 2f32.powi(e - 15),
    }
}

pub(crate) fn to_ldr(c: Vec3A) -> [u8; 4] {
    let g = |v: f32| (if v.is_nan() { 0.0 } else { v.clamp(0.0, 1.0) }.powf(GAMMA) * 255.0 + 0.5) as u8;
    [g(c.x), g(c.y), g(c.z), 255]
}

fn pack(level: &[Vec3A], side: usize, hdr: bool) -> Vec<u8> {
    if hdr {
        let mut out = Vec::with_capacity(level.len() * 8);
        for c in level {
            for v in [c.x, c.y, c.z, 1.0] {
                out.extend_from_slice(&f32_to_f16(v).to_le_bytes());
            }
        }
        return out;
    }
    let rgba: Vec<u8> = level.iter().flat_map(|c| to_ldr(*c)).collect();
    let format = texpresso::Format::Bc1;
    let mut out = vec![0u8; format.compressed_size(side, side)];
    let params = texpresso::Params { algorithm: texpresso::Algorithm::IterativeClusterFit, ..Default::default() };
    format.compress(&rgba, side, side, params, &mut out);
    out
}

pub(crate) fn level_bytes(side: u32, hdr: bool) -> usize {
    if hdr { side as usize * side as usize * 8 } else { side.div_ceil(4) as usize * side.div_ceil(4) as usize * 8 }
}

pub(crate) fn cubemap(faces: &[Vec<Vec3A>; 6], size: u32, hdr: bool) -> Vec<u8> {
    let size = size.max(1);
    let mip_n = mip_count(size);
    let faces: Vec<Vec<Vec3A>> = faces.iter().map(|f| f.iter().map(|c| finite(*c)).collect()).collect();
    let chains: Vec<Vec<Vec<Vec3A>>> = faces.iter().map(|f| mips(f, size)).collect();
    let mut sum = Vec3A::ZERO;
    let mut n = 0usize;
    for f in &faces {
        for c in f {
            sum += c.clamp(Vec3A::ZERO, Vec3A::ONE);
            n += 1;
        }
    }
    let refl = if n > 0 { sum / n as f32 } else { Vec3A::ZERO };
    let mut h: Vec<u8> = Vec::new();
    h.extend_from_slice(b"VTF\0");
    h.extend_from_slice(&7u32.to_le_bytes());
    h.extend_from_slice(&4u32.to_le_bytes());
    h.extend_from_slice(&(HEADER as u32).to_le_bytes());
    h.extend_from_slice(&(size as u16).to_le_bytes());
    h.extend_from_slice(&(size as u16).to_le_bytes());
    h.extend_from_slice(&(if hdr { FLAGS_HDR } else { FLAGS_LDR }).to_le_bytes());
    h.extend_from_slice(&1u16.to_le_bytes());
    h.extend_from_slice(&0u16.to_le_bytes());
    h.extend_from_slice(&[0u8; 4]);
    for v in [refl.x, refl.y, refl.z] {
        h.extend_from_slice(&v.to_le_bytes());
    }
    h.extend_from_slice(&[0u8; 4]);
    h.extend_from_slice(&1f32.to_le_bytes());
    h.extend_from_slice(&(if hdr { FORMAT_RGBA16161616F } else { FORMAT_DXT1 }).to_le_bytes());
    h.push(mip_n as u8);
    h.extend_from_slice(&(-1i32).to_le_bytes());
    h.push(0);
    h.push(0);
    h.extend_from_slice(&1u16.to_le_bytes());
    h.extend_from_slice(&[0u8; 3]);
    h.extend_from_slice(&1u32.to_le_bytes());
    h.extend_from_slice(&[0u8; 8]);
    h.extend_from_slice(&[0x30, 0, 0, 0]);
    h.extend_from_slice(&(HEADER as u32).to_le_bytes());
    debug_assert_eq!(h.len(), HEADER);
    for m in (0..mip_n as usize).rev() {
        let side = (size >> m).max(1);
        for chain in &chains {
            h.extend_from_slice(&pack(&chain[m], side as usize, hdr));
        }
        let black = if hdr {
            let mut b = Vec::with_capacity(level_bytes(side, true));
            for _ in 0..side * side {
                for v in [0.0f32, 0.0, 0.0, 1.0] {
                    b.extend_from_slice(&f32_to_f16(v).to_le_bytes());
                }
            }
            b
        } else {
            pack(&vec![Vec3A::ZERO; (side * side) as usize], side as usize, false)
        };
        h.extend_from_slice(&black);
    }
    h
}
