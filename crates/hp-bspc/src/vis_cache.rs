use std::path::{Path, PathBuf};

use super::VisPortal;

const MAGIC: &[u8; 4] = b"HPVC";
const FORMAT: u32 = 1;
const HEADER: usize = 4 + 4 + 8 + 8 + 4 + 8 + 8;

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct VisCache {
    pub dir: PathBuf,
    pub salt: u64,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct VisKey {
    a: u64,
    b: u64,
}

pub struct VisInput<'a> {
    pub clusters: usize,
    pub portals: &'a [VisPortal],
    pub fast: bool,
    pub sort: bool,
    pub radius_override: Option<f64>,
    pub fog_radius: Option<f64>,
    pub sky: &'a [u8],
}

#[inline]
fn mix(mut z: u64) -> u64 {
    z = (z ^ (z >> 30)).wrapping_mul(0xBF58_476D_1CE4_E5B9);
    z = (z ^ (z >> 27)).wrapping_mul(0x94D0_49BB_1331_11EB);
    z ^ (z >> 31)
}

struct Lanes([u64; 2], u64);

impl Lanes {
    fn new(salt: u64) -> Lanes {
        Lanes([mix(salt ^ 0x5AFE_C0DE_1234_5678), mix(salt ^ 0x0DDB_A11C_AFE5_7EA1)], 0)
    }

    #[inline]
    fn word(&mut self, w: u64) {
        self.1 = self.1.wrapping_add(1);
        self.0[0] = mix(self.0[0] ^ w).wrapping_add(0x9E37_79B9_7F4A_7C15);
        self.0[1] = mix(self.0[1].rotate_left(23) ^ w ^ self.1.wrapping_mul(0xD6E8_FEB8_6659_FD93));
    }

    fn f64(&mut self, v: f64) {
        self.word(if v == 0.0 { 0 } else { v.to_bits() });
    }

    fn bytes(&mut self, b: &[u8]) {
        self.word(b.len() as u64);
        for chunk in b.chunks(8) {
            let mut w = [0u8; 8];
            w[..chunk.len()].copy_from_slice(chunk);
            self.word(u64::from_le_bytes(w));
        }
    }

    fn finish(&self) -> VisKey {
        VisKey { a: mix(self.0[0] ^ self.1), b: mix(self.0[1].wrapping_add(self.1)) }
    }
}

pub fn key(input: &VisInput<'_>, salt: u64) -> VisKey {
    let mut h = Lanes::new(salt);
    h.bytes(env!("CARGO_PKG_VERSION").as_bytes());
    h.word(u64::from(FORMAT));
    h.word(input.clusters as u64);
    h.word(input.portals.len() as u64);
    for p in input.portals {
        h.word(u64::from(p.clusters[0]) | (u64::from(p.clusters[1]) << 32));
        h.word(p.points.len() as u64);
        for q in &p.points {
            h.f64(q.x);
            h.f64(q.y);
            h.f64(q.z);
        }
    }
    h.word(u64::from(input.fast) | (u64::from(input.sort) << 1));
    for r in [input.radius_override, input.fog_radius] {
        match r {
            Some(v) => {
                h.word(1);
                h.f64(v);
            }
            None => h.word(0),
        }
    }
    h.bytes(if input.fast { input.sky } else { &[] });
    h.finish()
}

fn entry(dir: &Path, key: VisKey) -> PathBuf {
    dir.join(format!("vis-{:016x}.hpvis", key.a))
}

fn checksum(data: &[u8]) -> u64 {
    let mut h = Lanes::new(0x4855_5f56_4953);
    h.bytes(data);
    h.finish().a
}

pub fn load(dir: &Path, key: VisKey, clusters: usize) -> Option<Vec<u8>> {
    let data = std::fs::read(entry(dir, key)).ok()?;
    let head = data.get(..HEADER)?;
    let u32_at = |o: usize| u32::from_le_bytes([head[o], head[o + 1], head[o + 2], head[o + 3]]);
    let u64_at = |o: usize| {
        let mut b = [0u8; 8];
        b.copy_from_slice(&head[o..o + 8]);
        u64::from_le_bytes(b)
    };
    if &head[..4] != MAGIC
        || u32_at(4) != FORMAT
        || u64_at(8) != key.a
        || u64_at(16) != key.b
        || u32_at(24) as usize != clusters
    {
        return None;
    }
    let len = usize::try_from(u64_at(28)).ok()?;
    let lump = data.get(HEADER..)?;
    if lump.len() != len || checksum(lump) != u64_at(36) {
        return None;
    }
    let _ = std::fs::File::options()
        .write(true)
        .open(entry(dir, key))
        .and_then(|f| f.set_modified(std::time::SystemTime::now()));
    Some(lump.to_vec())
}

pub fn store(dir: &Path, key: VisKey, clusters: usize, lump: &[u8]) -> std::io::Result<()> {
    std::fs::create_dir_all(dir)?;
    let mut data = Vec::with_capacity(HEADER + lump.len());
    data.extend_from_slice(MAGIC);
    data.extend_from_slice(&FORMAT.to_le_bytes());
    data.extend_from_slice(&key.a.to_le_bytes());
    data.extend_from_slice(&key.b.to_le_bytes());
    data.extend_from_slice(&u32::try_from(clusters).unwrap_or(u32::MAX).to_le_bytes());
    data.extend_from_slice(&(lump.len() as u64).to_le_bytes());
    data.extend_from_slice(&checksum(lump).to_le_bytes());
    data.extend_from_slice(lump);
    let path = entry(dir, key);
    let tmp = path.with_extension(format!("hpvis.{}.tmp", std::process::id()));
    std::fs::write(&tmp, &data)?;
    std::fs::rename(&tmp, &path).inspect_err(|_| {
        let _ = std::fs::remove_file(&tmp);
    })?;
    prune(dir, MAX_ENTRIES);
    Ok(())
}

pub const MAX_ENTRIES: usize = 6;

fn prune(dir: &Path, keep: usize) {
    let Ok(read) = std::fs::read_dir(dir) else { return };
    let mut files: Vec<(std::time::SystemTime, PathBuf)> = read
        .flatten()
        .map(|e| e.path())
        .filter(|p| p.extension().is_some_and(|x| x.eq_ignore_ascii_case("hpvis")))
        .filter_map(|p| Some((std::fs::metadata(&p).ok()?.modified().ok()?, p)))
        .collect();
    if files.len() <= keep {
        return;
    }
    files.sort_by_key(|f| std::cmp::Reverse(f.0));
    for (_, p) in files.into_iter().skip(keep) {
        let _ = std::fs::remove_file(p);
    }
}
