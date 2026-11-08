use std::sync::Arc;

use crate::Renderer;
use crate::compiled_light::f16_bits;

pub const MAX_PROBES: u32 = 16;
const MAX_CELLS: usize = 1 << 22;
const COORD_BIAS: i32 = 32768;

#[derive(Clone, Debug, PartialEq)]
pub struct IndirectVolume {
    pub origin: [f32; 3],
    pub spacing: f32,
    cells: u32,
    mask: u32,
    table_len: u32,
    words: Vec<u32>,
}

fn keys(c: [i32; 3]) -> (u32, u32) {
    let k = |v: i32| (v + COORD_BIAS) as u32 & 0xffff;
    (k(c[0]) | (k(c[1]) << 16), k(c[2]))
}

fn slot(k0: u32, k1: u32) -> u32 {
    let h = k0.wrapping_mul(0x9E37_79B1) ^ k1.wrapping_mul(0x85EB_CA77);
    h ^ (h >> 15)
}

impl IndirectVolume {
    pub fn cpu_words(&self) -> &[u32] {
        &self.words
    }

    pub fn new(origin: [f32; 3], spacing: f32, coords: &[[i32; 3]], cubes: &[[[f32; 3]; 6]]) -> Option<Self> {
        let n = coords.len();
        if n == 0
            || n > MAX_CELLS
            || cubes.len() != n
            || !spacing.is_finite()
            || spacing <= 0.0
            || !origin.iter().all(|v| v.is_finite())
            || coords.iter().any(|c| c.iter().any(|v| !(-COORD_BIAS..COORD_BIAS).contains(v)))
        {
            return None;
        }
        let mut size = (n * 2).next_power_of_two().max(16);
        let table = loop {
            let mask = size as u32 - 1;
            let mut table = vec![[0u32; 4]; size];
            let mut worst = 0u32;
            for (i, c) in coords.iter().enumerate() {
                let (k0, k1) = keys(*c);
                let mut at = slot(k0, k1);
                let mut steps = 0u32;
                while table[(at & mask) as usize][3] != 0 {
                    if table[(at & mask) as usize][0] == k0 && table[(at & mask) as usize][1] == k1 {
                        break;
                    }
                    at = at.wrapping_add(1);
                    steps += 1;
                }
                table[(at & mask) as usize] = [k0, k1, i as u32, 1];
                worst = worst.max(steps);
            }
            if worst < MAX_PROBES {
                break table;
            }
            size *= 2;
            if size > MAX_CELLS * 8 {
                return None;
            }
        };
        let mask = table.len() as u32 - 1;
        let mut words: Vec<u32> = Vec::with_capacity((table.len() + 3 * n) * 4);
        words.extend(table.iter().flatten());
        for cube in cubes {
            let mut halves = [0u16; 24];
            for (f, face) in cube.iter().enumerate() {
                for (c, v) in face.iter().enumerate() {
                    halves[f * 3 + c] = f16_bits(if v.is_finite() { v.clamp(0.0, 60000.0) } else { 0.0 });
                }
            }
            for pair in halves.chunks_exact(2) {
                words.push(u32::from(pair[0]) | (u32::from(pair[1]) << 16));
            }
        }
        Some(Self { origin, spacing, cells: n as u32, mask, table_len: table.len() as u32, words })
    }

    pub fn cells(&self) -> u32 {
        self.cells
    }

    pub fn valid_cells(&self) -> u32 {
        self.cells
    }

    pub fn gpu_bytes(&self) -> u64 {
        self.words.len() as u64 * 4
    }

    pub fn table_len(&self) -> u32 {
        self.table_len
    }

    pub fn lookup(&self, c: [i32; 3]) -> Option<u32> {
        let (k0, k1) = keys(c);
        let mut at = slot(k0, k1);
        for _ in 0..MAX_PROBES {
            let e = (at & self.mask) as usize * 4;
            if self.words[e + 3] == 0 {
                return None;
            }
            if self.words[e] == k0 && self.words[e + 1] == k1 {
                return Some(self.words[e + 2]);
            }
            at = at.wrapping_add(1);
        }
        None
    }

    pub fn cube_half(&self, cell: u32, face: usize, channel: usize) -> u16 {
        let base = self.table_len as usize * 4 + cell as usize * 12;
        let at = face * 3 + channel;
        let word = self.words[base + at / 2];
        if at.is_multiple_of(2) { (word & 0xffff) as u16 } else { (word >> 16) as u16 }
    }
}

pub(crate) struct Indirect {
    volume: Option<Arc<IndirectVolume>>,
    buffer: Option<wgpu::Buffer>,
}

impl Indirect {
    pub fn new() -> Self {
        Self { volume: None, buffer: None }
    }
}

impl Renderer {
    pub fn set_indirect_volume(&mut self, volume: Option<Arc<IndirectVolume>>) {
        if self.indirect.volume == volume {
            return;
        }
        self.indirect.volume = None;
        self.indirect.buffer = None;
        self.scene_gen += 1;
        self.content_gen += 1;
        let Some(v) = volume else { return };
        let bytes = v.gpu_bytes();
        if bytes > self.max_storage {
            log::warn!("hp-render: the indirect light volume of {bytes} bytes is too large for the GPU");
            return;
        }
        let buffer = self.device.create_buffer(&wgpu::BufferDescriptor {
            label: Some("hp-render indirect light"),
            size: bytes,
            usage: wgpu::BufferUsages::STORAGE | wgpu::BufferUsages::COPY_DST,
            mapped_at_creation: false,
        });
        self.queue.write_buffer(&buffer, 0, bytemuck::cast_slice(&v.words));
        self.indirect.buffer = Some(buffer);
        self.indirect.volume = Some(v);
    }

    pub fn indirect_volume(&self) -> Option<&Arc<IndirectVolume>> {
        self.indirect.volume.as_ref().filter(|_| self.indirect.buffer.is_some())
    }

    pub(crate) fn indirect_buffer(&self) -> &wgpu::Buffer {
        self.indirect.buffer.as_ref().unwrap_or(&self.dummy_storage)
    }

    pub(crate) fn indirect_uniform(&self, enabled: bool) -> ([f32; 4], [f32; 4]) {
        match self.indirect_volume().filter(|_| enabled) {
            Some(v) => (
                [v.origin[0], v.origin[1], v.origin[2], 1.0 / v.spacing],
                [f32::from_bits(v.mask), f32::from_bits(v.table_len), 0.0, 1.0],
            ),
            None => ([0.0; 4], [f32::from_bits(0), f32::from_bits(0), 0.0, 0.0]),
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn cube(k: f32) -> [[f32; 3]; 6] {
        std::array::from_fn(|f| [k + f as f32, 0.5, 0.25])
    }

    #[test]
    fn volumes_hash_their_cells_and_pack_the_cubes() {
        let coords = [[0, 0, 0], [1, 0, 0], [-3, 7, -2], [32767, -32768, 5]];
        let cubes = [cube(1.0), cube(10.0), cube(20.0), cube(30.0)];
        let v = IndirectVolume::new([1.0, 2.0, 3.0], 32.0, &coords, &cubes).expect("volume");
        assert_eq!((v.cells(), v.valid_cells()), (4, 4));
        for (i, c) in coords.iter().enumerate() {
            assert_eq!(v.lookup(*c), Some(i as u32), "{c:?}");
        }
        assert_eq!(v.lookup([5, 5, 5]), None);
        assert_eq!(v.lookup([0, 0, 1]), None);
        for face in 0..6usize {
            assert_eq!(v.cube_half(2, face, 0), f16_bits(20.0 + face as f32));
            assert_eq!(v.cube_half(2, face, 1), f16_bits(0.5));
            assert_eq!(v.cube_half(2, face, 2), f16_bits(0.25));
        }
        assert_eq!(v.gpu_bytes(), (v.table_len() as u64 + 3 * 4) * 16);
        assert!(IndirectVolume::new([0.0; 3], 32.0, &coords, &cubes[..2]).is_none());
        assert!(IndirectVolume::new([0.0; 3], 0.0, &coords, &cubes).is_none());
        assert!(IndirectVolume::new([f32::NAN; 3], 32.0, &coords, &cubes).is_none());
        assert!(IndirectVolume::new([0.0; 3], 32.0, &[], &[]).is_none());
        assert!(IndirectVolume::new([0.0; 3], 32.0, &[[40000, 0, 0]], &cubes[..1]).is_none());
        let hostile = IndirectVolume::new([0.0; 3], 8.0, &[[0, 0, 0]], &[[[f32::NAN, f32::INFINITY, -4.0]; 6]])
            .expect("sanitized");
        assert_eq!((hostile.cube_half(0, 0, 0), hostile.cube_half(0, 5, 2)), (0, 0));
    }

    #[test]
    fn many_cells_keep_the_probe_sequences_short() {
        let coords: Vec<[i32; 3]> = (0..60_000).map(|i| [i % 100, (i / 100) % 100, i / 10_000]).collect();
        let cubes = vec![cube(1.0); coords.len()];
        let v = IndirectVolume::new([0.0; 3], 32.0, &coords, &cubes).expect("volume");
        assert!(coords.iter().step_by(37).all(|c| v.lookup(*c).is_some()));
        assert!(v.lookup([200, 200, 200]).is_none());
    }
}
