use alloc::{string::String, vec::Vec};
use core::hash::{Hash, Hasher};
use core::sync::atomic::{AtomicU64, Ordering};
use std::path::{Path, PathBuf};
use std::sync::RwLock;

const MAGIC: &[u8; 8] = b"HPDXSC01";
const HEADER: usize = 24;
const MAX_FILES: usize = 4096;

static DIR: RwLock<Option<PathBuf>> = RwLock::new(None);
static ACTIVE: RwLock<Option<String>> = RwLock::new(None);
static HITS: AtomicU64 = AtomicU64::new(0);
static MISSES: AtomicU64 = AtomicU64::new(0);
static COMPILED: AtomicU64 = AtomicU64::new(0);
static COMPILE_MICROS: AtomicU64 = AtomicU64::new(0);
static WRITE_ERRORS: AtomicU64 = AtomicU64::new(0);

#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub struct ShaderCompileStats {
    pub disk_hits: u64,
    pub disk_misses: u64,
    pub compiled: u64,
    pub compile_micros: u64,
    pub write_errors: u64,
}

pub fn set_shader_disk_cache(dir: Option<PathBuf>) {
    if let Some(dir) = dir.as_deref() {
        prune(dir);
    }
    if let Ok(mut slot) = DIR.write() {
        *slot = dir;
    }
}

pub fn active_shader_compiler() -> Option<String> {
    ACTIVE.read().ok().and_then(|a| a.clone())
}

pub(super) fn set_active_compiler(name: &str) {
    if let Ok(mut slot) = ACTIVE.write() {
        *slot = Some(name.into());
    }
}

pub fn shader_disk_cache_dir() -> Option<PathBuf> {
    DIR.read().ok().and_then(|d| d.clone())
}

pub fn shader_compile_stats() -> ShaderCompileStats {
    ShaderCompileStats {
        disk_hits: HITS.load(Ordering::Relaxed),
        disk_misses: MISSES.load(Ordering::Relaxed),
        compiled: COMPILED.load(Ordering::Relaxed),
        compile_micros: COMPILE_MICROS.load(Ordering::Relaxed),
        write_errors: WRITE_ERRORS.load(Ordering::Relaxed),
    }
}

pub(super) fn note_compile(micros: u64) {
    COMPILED.fetch_add(1, Ordering::Relaxed);
    COMPILE_MICROS.fetch_add(micros, Ordering::Relaxed);
}

fn fnv(data: &[u8], seed: u64) -> u64 {
    data.iter()
        .fold(seed, |h, b| (h ^ u64::from(*b)).wrapping_mul(0x0000_0100_0000_01b3))
}

pub(super) fn key(source: &str, entry_point: &str, full_stage: &str, compiler: &str) -> String {
    let mut sip = std::collections::hash_map::DefaultHasher::new();
    (source, entry_point, full_stage, compiler).hash(&mut sip);
    let mut joined = Vec::with_capacity(source.len() + 64);
    for part in [compiler, full_stage, entry_point, source] {
        joined.extend_from_slice(&(part.len() as u64).to_le_bytes());
        joined.extend_from_slice(part.as_bytes());
    }
    format!(
        "{:016x}{:016x}",
        fnv(&joined, 0xcbf2_9ce4_8422_2325),
        sip.finish()
    )
}

fn unwrap_file(bytes: &[u8]) -> Option<&[u8]> {
    let (head, payload) = bytes.split_at_checked(HEADER)?;
    let (magic, rest) = head.split_at(8);
    let (len, hash) = rest.split_at(8);
    let len = u64::from_le_bytes(len.try_into().ok()?);
    let hash = u64::from_le_bytes(hash.try_into().ok()?);
    (magic == MAGIC
        && !payload.is_empty()
        && payload.len() as u64 == len
        && fnv(payload, 0x8422_2325_cbf2_9ce4) == hash)
        .then_some(payload)
}

fn wrap_file(payload: &[u8]) -> Vec<u8> {
    let mut out = Vec::with_capacity(HEADER + payload.len());
    out.extend_from_slice(MAGIC);
    out.extend_from_slice(&(payload.len() as u64).to_le_bytes());
    out.extend_from_slice(&fnv(payload, 0x8422_2325_cbf2_9ce4).to_le_bytes());
    out.extend_from_slice(payload);
    out
}

pub(super) fn load(key: &str) -> Option<Vec<u8>> {
    let dir = shader_disk_cache_dir()?;
    let found = std::fs::read(dir.join(format!("{key}.bin")))
        .ok()
        .and_then(|bytes| unwrap_file(&bytes).map(<[u8]>::to_vec));
    if found.is_some() {
        HITS.fetch_add(1, Ordering::Relaxed);
    } else {
        MISSES.fetch_add(1, Ordering::Relaxed);
    }
    found
}

pub(super) fn store(key: &str, payload: &[u8]) {
    let Some(dir) = shader_disk_cache_dir() else {
        return;
    };
    if payload.is_empty() || write_atomic(&dir, key, &wrap_file(payload)).is_err() {
        WRITE_ERRORS.fetch_add(1, Ordering::Relaxed);
    }
}

fn write_atomic(dir: &Path, key: &str, bytes: &[u8]) -> std::io::Result<()> {
    std::fs::create_dir_all(dir)?;
    let path = dir.join(format!("{key}.bin"));
    let tmp = dir.join(format!(
        "{key}.tmp{}-{:?}",
        std::process::id(),
        std::thread::current().id()
    ));
    std::fs::write(&tmp, bytes)?;
    std::fs::rename(&tmp, &path).inspect_err(|_| {
        let _ = std::fs::remove_file(&tmp);
    })
}

fn prune(dir: &Path) {
    let Ok(entries) = std::fs::read_dir(dir) else {
        return;
    };
    let files: Vec<PathBuf> = entries.flatten().map(|e| e.path()).collect();
    let stale = |p: &PathBuf| {
        p.extension()
            .and_then(|e| e.to_str())
            .is_some_and(|e| e.starts_with("tmp"))
    };
    let too_many = files.len() > MAX_FILES;
    for path in files {
        if too_many || stale(&path) {
            let _ = std::fs::remove_file(path);
        }
    }
}

pub(super) fn bytes_of(shader: &super::CompiledShader) -> Vec<u8> {
    let native = shader.create_native_shader();
    if native.pShaderBytecode.is_null() || native.BytecodeLength == 0 {
        return Vec::new();
    }
    unsafe {
        core::slice::from_raw_parts(native.pShaderBytecode.cast::<u8>(), native.BytecodeLength)
            .to_vec()
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn files_are_checked_before_use() {
        let payload = b"DXBC compiled".to_vec();
        let file = wrap_file(&payload);
        assert_eq!(unwrap_file(&file), Some(&payload[..]));
        let mut bad = file.clone();
        if let Some(b) = bad.last_mut() {
            *b ^= 1;
        }
        assert_eq!(unwrap_file(&bad), None);
        assert_eq!(unwrap_file(&file[..file.len() - 1]), None);
        assert_eq!(unwrap_file(&wrap_file(&[])), None);
    }

    #[test]
    fn keys_depend_on_every_part() {
        let k = key("float4 main()", "main", "ps_5_1", "fxc");
        assert_eq!(k.len(), 32);
        assert_eq!(k, key("float4 main()", "main", "ps_5_1", "fxc"));
        assert_ne!(k, key("float4 main() ", "main", "ps_5_1", "fxc"));
        assert_ne!(k, key("float4 main()", "main2", "ps_5_1", "fxc"));
        assert_ne!(k, key("float4 main()", "main", "vs_5_1", "fxc"));
        assert_ne!(k, key("float4 main()", "main", "ps_5_1", "dxc"));
    }
}
