use std::path::{Path, PathBuf};
use std::sync::RwLock;
use std::sync::atomic::{AtomicBool, Ordering};

const MAGIC: &[u8; 8] = b"HPPCACHE";
const HEADER: usize = 16;

struct Slot {
    device: wgpu::Device,
    cache: wgpu::PipelineCache,
    path: PathBuf,
    saved: u64,
}

static SLOT: RwLock<Option<Slot>> = RwLock::new(None);
static DIRTY: AtomicBool = AtomicBool::new(false);

pub fn file_name(info: &wgpu::AdapterInfo) -> Option<String> {
    wgpu::util::pipeline_cache_key(info).map(|k| format!("{k}.bin"))
}

pub fn wanted_features(adapter: &wgpu::Adapter) -> wgpu::Features {
    let info = adapter.get_info();
    if file_name(&info).is_some() {
        adapter.features() & wgpu::Features::PIPELINE_CACHE
    } else {
        wgpu::Features::empty()
    }
}

fn fnv(data: &[u8]) -> u64 {
    data.iter().fold(0xcbf2_9ce4_8422_2325u64, |h, b| (h ^ u64::from(*b)).wrapping_mul(0x0000_0100_0000_01b3))
}

fn unwrap_file(bytes: &[u8]) -> Option<&[u8]> {
    let (head, payload) = bytes.split_at_checked(HEADER)?;
    let (magic, hash) = head.split_at(8);
    let hash = u64::from_le_bytes(hash.try_into().ok()?);
    (magic == MAGIC && !payload.is_empty() && fnv(payload) == hash).then_some(payload)
}

fn wrap_file(payload: &[u8]) -> Vec<u8> {
    let mut out = Vec::with_capacity(HEADER + payload.len());
    out.extend_from_slice(MAGIC);
    out.extend_from_slice(&fnv(payload).to_le_bytes());
    out.extend_from_slice(payload);
    out
}

pub fn install(device: &wgpu::Device, path: &Path) -> bool {
    if !device.features().contains(wgpu::Features::PIPELINE_CACHE) {
        return false;
    }
    let bytes = std::fs::read(path).ok();
    let data = bytes.as_deref().and_then(unwrap_file);
    if bytes.is_some() && data.is_none() {
        log::info!("hp-render: the pipeline cache {} is not valid and is rebuilt", path.display());
    }
    let scope = device.push_error_scope(wgpu::ErrorFilter::Validation);
    let cache = unsafe {
        device.create_pipeline_cache(&wgpu::PipelineCacheDescriptor {
            label: Some("hp-render pipeline cache"),
            data,
            fallback: true,
        })
    };
    let failed = popped(scope.pop()).is_some_and(|e| e.is_some());
    if failed {
        log::warn!("hp-render: the pipeline cache could not be created, pipelines are built without it");
        return false;
    }
    let saved = data.map_or(0, |d| d.len() as u64);
    if let Ok(mut slot) = SLOT.write() {
        *slot = Some(Slot { device: device.clone(), cache, path: path.to_path_buf(), saved });
    }
    true
}

fn popped<F: std::future::Future>(f: F) -> Option<F::Output> {
    let mut f = std::pin::pin!(f);
    let mut cx = std::task::Context::from_waker(std::task::Waker::noop());
    match f.as_mut().poll(&mut cx) {
        std::task::Poll::Ready(v) => Some(v),
        std::task::Poll::Pending => None,
    }
}

pub(crate) fn with<R>(device: &wgpu::Device, f: impl FnOnce(Option<&wgpu::PipelineCache>) -> R) -> R {
    let slot = SLOT.read().ok();
    let cache = slot.as_ref().and_then(|s| s.as_ref()).filter(|s| s.device == *device).map(|s| &s.cache);
    if cache.is_some() {
        DIRTY.store(true, Ordering::Relaxed);
    }
    f(cache)
}

pub(crate) fn save_if_dirty(device: &wgpu::Device) {
    if DIRTY.swap(false, Ordering::Relaxed) {
        save(device);
    }
}

pub(crate) fn save(device: &wgpu::Device) {
    DIRTY.store(false, Ordering::Relaxed);
    let Ok(mut guard) = SLOT.write() else { return };
    let Some(slot) = guard.as_mut().filter(|s| s.device == *device) else { return };
    let Some(data) = slot.cache.get_data() else { return };
    if data.is_empty() || data.len() as u64 == slot.saved {
        return;
    }
    slot.saved = data.len() as u64;
    let path = slot.path.clone();
    drop(guard);
    let spawned = std::thread::Builder::new().name("hp-pipeline-cache".to_owned()).spawn(move || {
        if let Err(e) = write_atomic(&path, &wrap_file(&data)) {
            log::warn!("hp-render: cannot save the pipeline cache {}: {e}", path.display());
        }
    });
    if let Err(e) = spawned {
        log::warn!("hp-render: cannot start the pipeline cache writer: {e}");
    }
}

fn write_atomic(path: &Path, bytes: &[u8]) -> std::io::Result<()> {
    if let Some(dir) = path.parent() {
        std::fs::create_dir_all(dir)?;
    }
    let tmp = path.with_extension(format!("tmp{}", std::process::id()));
    std::fs::write(&tmp, bytes)?;
    std::fs::rename(&tmp, path).inspect_err(|_| {
        let _ = std::fs::remove_file(&tmp);
    })
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn files_are_checked_before_use() {
        let payload = b"driver data".to_vec();
        let file = wrap_file(&payload);
        assert_eq!(unwrap_file(&file), Some(&payload[..]));
        let mut bad = file.clone();
        if let Some(b) = bad.last_mut() {
            *b ^= 1;
        }
        assert_eq!(unwrap_file(&bad), None);
        assert_eq!(unwrap_file(&file[..file.len() - 1]), None);
        assert_eq!(unwrap_file(b"short"), None);
        assert_eq!(unwrap_file(&wrap_file(&[])), None);
        let mut other = file;
        other[0] = b'X';
        assert_eq!(unwrap_file(&other), None);
    }
}
