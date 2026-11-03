use std::collections::HashMap;

pub const LUMP: usize = 23;

const MAGIC: &[u8; 4] = b"HPPB";
const VERSION: u32 = 2;
const HEADER: usize = 28;
const CELL: usize = 48;
pub const MAX_CELLS: usize = 1 << 23;

#[derive(Clone, Debug, Default, PartialEq)]
pub struct ProbeVolume {
    pub origin: [f32; 3],
    pub spacing: f32,
    pub coords: Vec<[i32; 3]>,
    pub cubes: Vec<[[f32; 3]; 6]>,
}

pub fn f16_bits(v: f32) -> u16 {
    let bits = v.to_bits();
    let sign = ((bits >> 16) & 0x8000) as u16;
    let exp = ((bits >> 23) & 0xff) as i32;
    let man = bits & 0x007f_ffff;
    if exp == 0xff {
        return if man != 0 { sign | 0x7e00 } else { sign | 0x7c00 };
    }
    let e = exp - 127 + 15;
    if e >= 0x1f {
        return sign | 0x7bff;
    }
    if e <= 0 {
        if e < -10 {
            return sign;
        }
        let m = man | 0x0080_0000;
        let shift = (14 - e) as u32;
        let rounded = (m + (1 << (shift - 1))) >> shift;
        return sign | rounded as u16;
    }
    let rounded = ((e as u32) << 10) + ((man + 0x1000) >> 13);
    sign | (rounded.min(0x7bff) as u16)
}

pub fn f16_value(h: u16) -> f32 {
    let sign = if h & 0x8000 != 0 { -1.0f32 } else { 1.0 };
    let exp = i32::from((h >> 10) & 0x1f);
    let man = f32::from(h & 0x3ff);
    match exp {
        0 => sign * man * 2f32.powi(-24),
        0x1f if man == 0.0 => sign * f32::INFINITY,
        0x1f => f32::NAN,
        _ => sign * (1.0 + man / 1024.0) * 2f32.powi(exp - 15),
    }
}

impl ProbeVolume {
    pub fn cell_count(&self) -> usize {
        self.coords.len()
    }

    pub fn center(&self, c: [i32; 3]) -> [f32; 3] {
        std::array::from_fn(|a| self.origin[a] + (c[a] as f32 + 0.5) * self.spacing)
    }

    pub fn cell_of(&self, p: [f32; 3]) -> [i32; 3] {
        std::array::from_fn(|a| ((p[a] - self.origin[a]) / self.spacing).floor() as i32)
    }

    pub fn is_consistent(&self) -> bool {
        let n = self.coords.len();
        n > 0
            && n <= MAX_CELLS
            && self.cubes.len() == n
            && self.spacing.is_finite()
            && self.spacing > 0.0
            && self.origin.iter().all(|v| v.is_finite())
            && self.coords.iter().all(|c| c.iter().all(|v| (-32768..32768).contains(v)))
    }

    pub fn index(&self) -> HashMap<[i32; 3], usize> {
        self.coords.iter().enumerate().map(|(i, c)| (*c, i)).collect()
    }

    pub fn encode(&self) -> Vec<u8> {
        let mut out = Vec::with_capacity(HEADER + self.cubes.len() * CELL);
        out.extend_from_slice(MAGIC);
        out.extend_from_slice(&VERSION.to_le_bytes());
        for v in self.origin {
            out.extend_from_slice(&v.to_le_bytes());
        }
        out.extend_from_slice(&self.spacing.to_le_bytes());
        out.extend_from_slice(&(self.coords.len() as u32).to_le_bytes());
        for (coord, cube) in self.coords.iter().zip(&self.cubes) {
            for v in coord {
                out.extend_from_slice(&v.to_le_bytes());
            }
            for face in cube {
                for c in face {
                    out.extend_from_slice(&f16_bits(*c).to_le_bytes());
                }
            }
        }
        out
    }

    pub fn decode(bytes: &[u8]) -> Option<Self> {
        if bytes.len() < HEADER || &bytes[..4] != MAGIC {
            return None;
        }
        let word = |o: usize| u32::from_le_bytes([bytes[o], bytes[o + 1], bytes[o + 2], bytes[o + 3]]);
        if word(4) != VERSION {
            return None;
        }
        let float = |o: usize| f32::from_bits(word(o));
        let origin = [float(8), float(12), float(16)];
        let spacing = float(20);
        let n = word(24) as usize;
        let body = &bytes[HEADER..];
        if n == 0 || n > MAX_CELLS || body.len() != n * CELL {
            return None;
        }
        let mut coords = Vec::with_capacity(n);
        let mut cubes = Vec::with_capacity(n);
        for cell in body.chunks_exact(CELL) {
            let int = |o: usize| i32::from_le_bytes([cell[o], cell[o + 1], cell[o + 2], cell[o + 3]]);
            coords.push([int(0), int(4), int(8)]);
            let mut cube = [[0.0f32; 3]; 6];
            for (k, face) in cube.iter_mut().enumerate() {
                for (c, v) in face.iter_mut().enumerate() {
                    let o = 12 + (k * 3 + c) * 2;
                    let value = f16_value(u16::from_le_bytes([cell[o], cell[o + 1]]));
                    *v = if value.is_finite() { value.max(0.0) } else { 0.0 };
                }
            }
            cubes.push(cube);
        }
        let volume = Self { origin, spacing, coords, cubes };
        volume.is_consistent().then_some(volume)
    }

    pub fn sample(&self, p: [f32; 3], n: [f32; 3]) -> Option<[f32; 3]> {
        self.sample_with(&self.index(), p, n)
    }

    pub fn sample_with(&self, index: &HashMap<[i32; 3], usize>, p: [f32; 3], n: [f32; 3]) -> Option<[f32; 3]> {
        if !self.is_consistent() {
            return None;
        }
        let g: [f32; 3] = std::array::from_fn(|a| (p[a] - self.origin[a]) / self.spacing - 0.5);
        let base = g.map(|v| v.floor() as i32);
        let frac: [f32; 3] = std::array::from_fn(|a| g[a] - g[a].floor());
        let faces = [usize::from(n[0] < 0.0), 2 + usize::from(n[1] < 0.0), 4 + usize::from(n[2] < 0.0)];
        let sq = [n[0] * n[0], n[1] * n[1], n[2] * n[2]];
        let mut sum = [0.0f32; 3];
        let mut weight = 0.0f32;
        for corner in 0..8usize {
            let off = [(corner & 1) as i32, ((corner >> 1) & 1) as i32, ((corner >> 2) & 1) as i32];
            let mut w = 1.0f32;
            let mut at = [0i32; 3];
            for a in 0..3 {
                at[a] = base[a] + off[a];
                w *= if off[a] == 1 { frac[a] } else { 1.0 - frac[a] };
            }
            let Some(&i) = index.get(&at) else { continue };
            if w <= 0.0 {
                continue;
            }
            let cube = &self.cubes[i];
            for c in 0..3 {
                sum[c] += w * (sq[0] * cube[faces[0]][c] + sq[1] * cube[faces[1]][c] + sq[2] * cube[faces[2]][c]);
            }
            weight += w;
        }
        (weight > 1e-4).then(|| sum.map(|c| c / weight))
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn volume() -> ProbeVolume {
        let mut coords = Vec::new();
        let mut cubes = Vec::new();
        for z in 0..2 {
            for y in 0..2 {
                for x in 0..3 {
                    if (x, y, z) == (2, 1, 0) {
                        continue;
                    }
                    let i = coords.len();
                    coords.push([x, y, z]);
                    cubes.push(std::array::from_fn(|k| [i as f32 * 0.1, k as f32 * 0.25, 0.5]));
                }
            }
        }
        ProbeVolume { origin: [0.0; 3], spacing: 32.0, coords, cubes }
    }

    #[test]
    fn half_floats_round_trip() {
        for v in [0.0f32, 1.0, 0.5, 0.0003, std::f32::consts::PI, 100.0, 60000.0, 1.0e-6] {
            let back = f16_value(f16_bits(v));
            assert!((back - v).abs() <= v.abs() * 1.0e-3 + 1.0e-7, "{v} -> {back}");
        }
        assert_eq!(f16_value(f16_bits(1.0e9)), 65504.0);
        assert_eq!(f16_value(f16_bits(-0.0)).to_bits(), (-0.0f32).to_bits());
        assert!(f16_value(f16_bits(f32::NAN)).is_nan());
        assert_eq!(f16_bits(f32::INFINITY), 0x7c00);
    }

    #[test]
    fn encoding_round_trips_and_rejects_damage() {
        let v = volume();
        let bytes = v.encode();
        assert_eq!(bytes.len(), HEADER + 11 * CELL);
        let back = ProbeVolume::decode(&bytes).expect("decode");
        assert_eq!((back.origin, back.spacing), (v.origin, v.spacing));
        assert_eq!(back.coords, v.coords);
        for (a, b) in back.cubes.iter().flatten().flatten().zip(v.cubes.iter().flatten().flatten()) {
            assert!((a - b).abs() < 2.0e-3, "{a} {b}");
        }
        assert!(ProbeVolume::decode(&bytes[..bytes.len() - 1]).is_none());
        assert!(ProbeVolume::decode(&bytes[..10]).is_none());
        let mut bad = bytes.clone();
        bad[0] = b'X';
        assert!(ProbeVolume::decode(&bad).is_none());
        let mut huge = bytes.clone();
        huge[24..28].copy_from_slice(&u32::MAX.to_le_bytes());
        assert!(ProbeVolume::decode(&huge).is_none());
        let mut far = bytes.clone();
        far[HEADER..HEADER + 4].copy_from_slice(&1_000_000i32.to_le_bytes());
        assert!(ProbeVolume::decode(&far).is_none(), "coordinates must fit the GPU keys");
        let mut nan = bytes;
        nan[20..24].copy_from_slice(&f32::NAN.to_le_bytes());
        assert!(ProbeVolume::decode(&nan).is_none());
    }

    #[test]
    fn sampling_follows_the_cube_and_skips_missing_cells() {
        let v = volume();
        let at = |c: [i32; 3]| v.center(c);
        let up = v.sample(at([0, 0, 0]), [0.0, 0.0, 1.0]).expect("sample");
        assert!((up[1] - 4.0 * 0.25).abs() < 1e-5, "+z face: {up:?}");
        let down = v.sample(at([0, 0, 0]), [0.0, 0.0, -1.0]).expect("sample");
        assert!((down[1] - 5.0 * 0.25).abs() < 1e-5);
        let diag = v.sample(at([1, 0, 0]), [0.0, 0.6, 0.8]).expect("sample");
        assert!((diag[1] - (0.36 * 2.0 * 0.25 + 0.64 * 4.0 * 0.25)).abs() < 1e-5);
        let between = v.sample([32.0, 16.0, 16.0], [0.0, 0.0, 1.0]).expect("sample");
        assert!((between[0] - 0.05).abs() < 1e-5, "halfway between cells 0 and 1: {between:?}");
        let corner = v.sample([75.0, 40.0, 20.0], [0.0, 0.0, 1.0]).expect("a missing cell is skipped");
        assert!(corner[0].is_finite());
        assert!(v.sample([-500.0, 0.0, 0.0], [0.0, 0.0, 1.0]).is_none());
        assert!(ProbeVolume::default().sample([0.0; 3], [0.0, 0.0, 1.0]).is_none());
        assert_eq!(v.cell_of([40.0, -1.0, 63.9]), [1, -1, 1]);
    }
}
