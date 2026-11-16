use glam::Vec3A;

pub const PLASTIC: f32 = 1.324_718;
pub const R2_A: f32 = 1.0 / PLASTIC;
pub const R2_B: f32 = 1.0 / (PLASTIC * PLASTIC);
pub const R1_A: f32 = 0.618_034;

#[inline]
pub fn pcg(v: u32) -> u32 {
    let state = v.wrapping_mul(747_796_405).wrapping_add(2_891_336_453);
    let word = ((state >> ((state >> 28).wrapping_add(4))) ^ state).wrapping_mul(277_803_737);
    (word >> 22) ^ word
}

#[inline]
pub fn hash3(a: u32, b: u32, c: u32) -> u32 {
    pcg(a ^ pcg(b ^ pcg(c)))
}

#[inline]
pub fn to_unit(h: u32) -> f32 {
    (h >> 8) as f32 * (1.0 / 16_777_216.0)
}

#[derive(Clone, Copy, Debug)]
pub struct Rng {
    pub state: u32,
}

impl Rng {
    pub fn new(seed: u32, luxel: u32, sample: u32) -> Rng {
        Rng { state: hash3(seed, luxel, sample) }
    }

    #[inline]
    pub fn uniform(&mut self) -> f32 {
        self.state = pcg(self.state);
        to_unit(self.state)
    }
}

#[inline]
pub fn fract(x: f32) -> f32 {
    x - x.floor()
}

#[inline]
pub fn r2(n: u32, seed: u32, luxel: u32, dim: u32) -> (f32, f32) {
    let h = hash3(seed ^ 0x5bd1_e995, luxel, dim);
    let ox = to_unit(h);
    let oy = to_unit(pcg(h));
    let nf = n as f32;
    (fract(ox + nf * R2_A), fract(oy + nf * R2_B))
}

#[inline]
pub fn r1(n: u32, seed: u32, luxel: u32, dim: u32) -> f32 {
    let o = to_unit(hash3(seed ^ 0x68e3_1da4, luxel, dim));
    fract(o + n as f32 * R1_A)
}

#[inline]
pub fn frame(n: Vec3A) -> (Vec3A, Vec3A) {
    let sign = if n.z >= 0.0 { 1.0 } else { -1.0 };
    let a = -1.0 / (sign + n.z);
    let b = n.x * n.y * a;
    (Vec3A::new(1.0 + sign * n.x * n.x * a, sign * b, -sign * n.x), Vec3A::new(b, sign + n.y * n.y * a, -n.y))
}

#[inline]
pub fn cosine(n: Vec3A, u: f32, v: f32) -> Vec3A {
    let r = u.sqrt();
    let phi = std::f32::consts::TAU * v;
    let (t, b) = frame(n);
    let z = (1.0 - u).max(0.0).sqrt();
    (t * (r * phi.cos()) + b * (r * phi.sin()) + n * z).normalize_or(n)
}

#[inline]
pub fn cone(axis: Vec3A, sin_max: f32, u: f32, v: f32) -> Vec3A {
    if sin_max <= 0.0 {
        return axis;
    }
    let cos_max = (1.0 - sin_max * sin_max).max(0.0).sqrt();
    let z = 1.0 - u * (1.0 - cos_max);
    let r = (1.0 - z * z).max(0.0).sqrt();
    let phi = std::f32::consts::TAU * v;
    let (t, b) = frame(axis);
    (t * (r * phi.cos()) + b * (r * phi.sin()) + axis * z).normalize_or(axis)
}

#[inline]
pub fn disk(u: f32, v: f32) -> (f32, f32) {
    let r = u.sqrt();
    let phi = std::f32::consts::TAU * v;
    (r * phi.cos(), r * phi.sin())
}

#[inline]
pub fn luminance(c: Vec3A) -> f32 {
    c.dot(Vec3A::new(0.2126, 0.7152, 0.0722))
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn cosine_samples_stay_in_the_hemisphere_and_average_two_thirds() {
        let n = Vec3A::new(0.3, -0.5, 0.8).normalize();
        let mut sum = 0.0;
        let count = 4096;
        for i in 0..count {
            let (u, v) = r2(i, 7, 3, 0);
            let d = cosine(n, u, v);
            assert!(d.dot(n) >= -1e-4);
            assert!((d.length() - 1.0).abs() < 1e-3);
            sum += d.dot(n);
        }
        let mean = sum / count as f32;
        assert!((mean - 2.0 / 3.0).abs() < 0.01, "{mean}");
    }

    #[test]
    fn r2_is_stratified() {
        let mut cells = [0u32; 16];
        for i in 0..64 {
            let (u, v) = r2(i, 1, 2, 3);
            cells[(u * 4.0) as usize + 4 * (v * 4.0) as usize] += 1;
        }
        assert!(cells.iter().all(|&c| (2..=6).contains(&c)), "{cells:?}");
    }

    #[test]
    fn cones_stay_inside() {
        let axis = Vec3A::new(0.0, 0.6, -0.8);
        let s = 0.1f32;
        for i in 0..256 {
            let (u, v) = r2(i, 0, 0, 0);
            let d = cone(axis, s, u, v);
            assert!(d.dot(axis) >= (1.0 - s * s).sqrt() - 1e-4);
        }
    }
}
