use std::future::Future;
use std::sync::atomic::{AtomicU64, Ordering};
use std::task::{Context, Poll, Waker};

static OUT_OF_MEMORY: AtomicU64 = AtomicU64::new(0);

pub fn out_of_memory_events() -> u64 {
    OUT_OF_MEMORY.load(Ordering::Relaxed)
}

pub fn note_out_of_memory() {
    OUT_OF_MEMORY.fetch_add(1, Ordering::Relaxed);
}

pub fn is_out_of_memory_text(text: &str) -> bool {
    let lower = text.to_ascii_lowercase();
    lower.contains("out of memory") || lower.contains("outofmemory")
}

pub(crate) fn guarded<T>(device: &wgpu::Device, f: impl FnOnce() -> T) -> (T, bool) {
    let scope = device.push_error_scope(wgpu::ErrorFilter::OutOfMemory);
    let out = f();
    let mut popped = std::pin::pin!(scope.pop());
    let mut cx = Context::from_waker(Waker::noop());
    let oom = matches!(popped.as_mut().poll(&mut cx), Poll::Ready(Some(_)));
    if oom {
        note_out_of_memory();
        log::warn!("hp-render: the graphics card is out of memory, an allocation was refused");
    }
    (out, oom)
}

pub(crate) const STAGING_SLICE: u64 = 16 << 20;
const STAGING_WAIT: std::time::Duration = std::time::Duration::from_secs(10);

#[derive(Default)]
pub(crate) struct StagingPacer {
    pending: u64,
    last: Option<wgpu::SubmissionIndex>,
}

impl StagingPacer {
    pub fn wrote(&mut self, device: &wgpu::Device, queue: &wgpu::Queue, bytes: u64) {
        self.pending = self.pending.saturating_add(bytes);
        if self.pending < STAGING_SLICE {
            return;
        }
        self.pending = 0;
        let index = queue.submit([]);
        if let Some(prev) = self.last.replace(index) {
            let _ = device.poll(wgpu::PollType::Wait { submission_index: Some(prev), timeout: Some(STAGING_WAIT) });
        }
    }
}

#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub struct VramInfo {
    pub budget: u64,
    pub usage: u64,
    pub dedicated: u64,
}

pub struct VramProbe {
    #[cfg(windows)]
    adapter: Option<dxgi::Adapter>,
}

impl VramProbe {
    pub fn new(info: &wgpu::AdapterInfo) -> Self {
        #[cfg(windows)]
        {
            Self { adapter: dxgi::Adapter::find(info.vendor, info.device, &info.name) }
        }
        #[cfg(not(windows))]
        {
            let _ = info;
            Self {}
        }
    }

    pub fn none() -> Self {
        Self {
            #[cfg(windows)]
            adapter: None,
        }
    }

    pub fn available(&self) -> bool {
        #[cfg(windows)]
        {
            self.adapter.is_some()
        }
        #[cfg(not(windows))]
        {
            false
        }
    }

    pub fn query(&self) -> Option<VramInfo> {
        #[cfg(windows)]
        {
            self.adapter.as_ref().and_then(dxgi::Adapter::query)
        }
        #[cfg(not(windows))]
        {
            None
        }
    }
}

#[cfg(windows)]
mod dxgi {
    use windows::Win32::Graphics::Dxgi::{
        CreateDXGIFactory1, DXGI_MEMORY_SEGMENT_GROUP_LOCAL, DXGI_QUERY_VIDEO_MEMORY_INFO, IDXGIAdapter3, IDXGIFactory1,
    };
    use windows::core::Interface;

    use super::VramInfo;

    pub(super) struct Adapter {
        raw: IDXGIAdapter3,
        dedicated: u64,
    }

    unsafe impl Send for Adapter {}
    unsafe impl Sync for Adapter {}

    fn name_of(desc: &[u16]) -> String {
        let end = desc.iter().position(|&c| c == 0).unwrap_or(desc.len());
        String::from_utf16_lossy(&desc[..end])
    }

    impl Adapter {
        pub(super) fn find(vendor: u32, device: u32, name: &str) -> Option<Self> {
            let factory: IDXGIFactory1 = unsafe { CreateDXGIFactory1() }.ok()?;
            let mut best: Option<(u8, Self)> = None;
            for i in 0..16 {
                let Ok(adapter) = (unsafe { factory.EnumAdapters1(i) }) else { break };
                let Ok(desc) = (unsafe { adapter.GetDesc1() }) else { continue };
                let Ok(raw) = adapter.cast::<IDXGIAdapter3>() else { continue };
                let same_ids = desc.VendorId == vendor && desc.DeviceId == device;
                let same_name = name_of(&desc.Description).trim().eq_ignore_ascii_case(name.trim());
                let rank = match (same_ids, same_name) {
                    (true, true) => 3,
                    (true, false) => 2,
                    (false, true) => 1,
                    _ => continue,
                };
                let dedicated = desc.DedicatedVideoMemory as u64;
                let better = best.as_ref().is_none_or(|(r, b)| rank > *r || (rank == *r && dedicated > b.dedicated));
                if better {
                    best = Some((rank, Self { raw, dedicated }));
                }
            }
            best.map(|(_, a)| a)
        }

        pub(super) fn query(&self) -> Option<VramInfo> {
            let mut info = DXGI_QUERY_VIDEO_MEMORY_INFO::default();
            unsafe { self.raw.QueryVideoMemoryInfo(0, DXGI_MEMORY_SEGMENT_GROUP_LOCAL, &mut info) }.ok()?;
            (info.Budget > 0).then_some(VramInfo {
                budget: info.Budget,
                usage: info.CurrentUsage,
                dedicated: self.dedicated,
            })
        }
    }
}

#[cfg(test)]
pub(crate) fn gpu_test_lock() -> std::sync::MutexGuard<'static, ()> {
    static LOCK: std::sync::Mutex<()> = std::sync::Mutex::new(());
    LOCK.lock().unwrap_or_else(std::sync::PoisonError::into_inner)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn out_of_memory_texts_are_recognized() {
        assert!(is_out_of_memory_text("wgpu error: Out of Memory"));
        assert!(is_out_of_memory_text("Device(OutOfMemory)"));
        assert!(!is_out_of_memory_text("Validation Error"));
        let before = out_of_memory_events();
        note_out_of_memory();
        assert!(out_of_memory_events() > before);
    }

    #[test]
    fn refused_allocations_are_seen() {
        let _serial = gpu_test_lock();
        let instance = wgpu::Instance::new(wgpu::InstanceDescriptor::new_without_display_handle());
        let Ok(adapter) = pollster::block_on(instance.request_adapter(&wgpu::RequestAdapterOptions::default())) else {
            return;
        };
        let Ok((device, _queue)) = pollster::block_on(adapter.request_device(&wgpu::DeviceDescriptor::default()))
        else {
            return;
        };
        let (_, oom) = guarded(&device, || {
            device.create_buffer(&wgpu::BufferDescriptor {
                label: Some("small"),
                size: 256,
                usage: wgpu::BufferUsages::VERTEX,
                mapped_at_creation: false,
            })
        });
        assert!(!oom);
        let probe = VramProbe::new(&adapter.get_info());
        if let Some(v) = probe.query() {
            assert!(v.budget > 0 && v.usage <= v.budget * 4, "{v:?}");
        }
        assert!(VramProbe::none().query().is_none());
    }
}
