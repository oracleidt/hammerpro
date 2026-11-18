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

pub const MIN_GPU_BLOCK: u64 = 4 << 20;
pub const MAX_GPU_BLOCK: u64 = 16 << 20;

pub fn memory_hints() -> wgpu::MemoryHints {
    wgpu::MemoryHints::Manual { suballocated_device_memory_block_size: MIN_GPU_BLOCK..MAX_GPU_BLOCK }
}

pub(crate) const STAGING_SLICE: u64 = 16 << 20;
pub(crate) const LOW_STAGING_SLICE: u64 = 4 << 20;
pub(crate) const MIN_SQUEEZED_TEXTURE: u32 = 128;
const STAGING_WAIT: std::time::Duration = std::time::Duration::from_secs(10);

#[derive(Clone, Copy, Debug)]
pub(crate) struct MemoryBudget {
    pub budget: u64,
    pub untracked: u64,
    pub low: bool,
    pub lightmap_layers: u32,
    pub squeezed: u64,
    pub refused: u64,
    pub wants_room: bool,
}

impl Default for MemoryBudget {
    fn default() -> Self {
        Self {
            budget: 0,
            untracked: 0,
            low: false,
            lightmap_layers: u32::MAX,
            squeezed: 0,
            refused: 0,
            wants_room: false,
        }
    }
}

impl MemoryBudget {
    pub fn active(&self) -> bool {
        self.budget > 0
    }

    pub fn fits(&self, tracked: u64, bytes: u64) -> bool {
        !self.active() || tracked.saturating_add(self.untracked).saturating_add(bytes) <= self.budget
    }
}

pub(crate) struct StagingPacer {
    pending: u64,
    slice: u64,
    last: Option<wgpu::SubmissionIndex>,
}

impl Default for StagingPacer {
    fn default() -> Self {
        Self { pending: 0, slice: STAGING_SLICE, last: None }
    }
}

impl StagingPacer {
    pub fn set_slice(&mut self, bytes: u64) {
        self.slice = bytes.max(1 << 20);
    }

    pub fn wrote(&mut self, device: &wgpu::Device, queue: &wgpu::Queue, bytes: u64) {
        self.pending = self.pending.saturating_add(bytes);
        if self.pending < self.slice {
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

#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub(crate) struct HeavyFlags {
    pub sun: bool,
    pub sky: bool,
    pub local: bool,
    pub control: bool,
}

#[derive(Clone, Copy, Debug, Default)]
pub(crate) struct HeavyUse {
    last: [Option<std::time::Instant>; 4],
}

impl HeavyUse {
    pub fn note_at(&mut self, now: std::time::Instant, used: [bool; 4]) {
        for (slot, used) in self.last.iter_mut().zip(used) {
            if used {
                *slot = Some(now);
            }
        }
    }

    fn is_stale(last: Option<std::time::Instant>, now: std::time::Instant, idle: std::time::Duration) -> bool {
        last.is_none_or(|t| now.saturating_duration_since(t) >= idle)
    }

    pub fn stale(&self, now: std::time::Instant, idle: std::time::Duration) -> HeavyFlags {
        let s = self.last.map(|l| Self::is_stale(l, now, idle));
        HeavyFlags { sun: s[0], sky: s[1], local: s[2], control: s[3] }
    }

    pub fn next_release(
        &self,
        now: std::time::Instant,
        idle: std::time::Duration,
        held: [bool; 4],
    ) -> Option<std::time::Duration> {
        self.last
            .iter()
            .zip(held)
            .filter(|(_, held)| *held)
            .map(|(last, _)| {
                last.map_or(std::time::Duration::ZERO, |t| idle.saturating_sub(now.saturating_duration_since(t)))
            })
            .min()
    }
}

#[derive(Clone, Debug, Default, PartialEq, Eq)]
pub struct AllocationGroup {
    pub label: String,
    pub count: u32,
    pub bytes: u64,
}

#[derive(Clone, Debug, Default)]
pub struct AllocatorSummary {
    pub allocated: u64,
    pub reserved: u64,
    pub blocks: usize,
    pub groups: Vec<AllocationGroup>,
    pub block_use: Vec<(u64, u64)>,
}

impl AllocatorSummary {
    pub fn category_bytes(&self) -> Vec<(&'static str, u64)> {
        let mut out: Vec<(&'static str, u64)> = Vec::new();
        for g in &self.groups {
            let c = category_of(&g.label);
            match out.iter_mut().find(|(n, _)| *n == c) {
                Some(e) => e.1 += g.bytes,
                None => out.push((c, g.bytes)),
            }
        }
        out.sort_by_key(|e| std::cmp::Reverse(e.1));
        out
    }
}

pub fn group_label(name: &str) -> String {
    let mut out = String::with_capacity(name.len());
    let mut digits = false;
    for c in name.chars() {
        if c.is_ascii_digit() {
            if !digits {
                out.push('#');
            }
            digits = true;
        } else {
            digits = false;
            out.push(c);
        }
    }
    let t = out.trim();
    if t.is_empty() { "unlabeled".to_owned() } else { t.to_owned() }
}

pub fn category_of(label: &str) -> &'static str {
    let l = label.to_ascii_lowercase();
    let has = |k: &[&str]| k.iter().any(|w| l.contains(w));
    if has(&["egui", "thumbnail", "thumb", "preview icon", "font"]) {
        "interface"
    } else if has(&["staging", "readback", "upload", "mapped"]) {
        "staging"
    } else if has(&["lightmap", "compiled", "baked", "bake"]) {
        "lightmaps"
    } else if has(&["shadow", "sky cut", "sky visibility", "sky mip"]) {
        "shadows"
    } else if has(&["cube", "envmap", "reflection", "probe"]) {
        "cubemaps_reflections"
    } else if has(&["ray", "bvh", "rt "]) {
        "ray_tracing"
    } else if has(&["particle", "beam", "rope", "sprite"]) {
        "particles_sprites"
    } else if has(&["skin", "bone", "anim"]) {
        "animation"
    } else if has(&["model", "mdl", "prop", "instance"]) {
        "models"
    } else if has(&["texture", "material", "vtf", "mip"]) {
        "textures"
    } else if has(&[
        "depth",
        "msaa",
        "hdr",
        "view color",
        "view display",
        "target",
        "bloom",
        "post",
        "pick",
        "outline",
        "resolve",
        "tone",
        "hiz",
    ]) {
        "render_targets"
    } else if has(&["arena", "vert", "indic", "index", "chunk", "scene", "line", "overlay", "state", "edge"]) {
        "geometry"
    } else if has(&["light", "cluster", "cull"]) {
        "lighting"
    } else {
        "other"
    }
}

pub fn allocator_summary(device: &wgpu::Device) -> Option<AllocatorSummary> {
    let report = device.generate_allocator_report()?;
    let mut groups: Vec<AllocationGroup> = Vec::new();
    let mut index: std::collections::HashMap<String, usize> = std::collections::HashMap::new();
    for a in &report.allocations {
        let label = group_label(&a.name);
        let i = *index.entry(label.clone()).or_insert_with(|| {
            groups.push(AllocationGroup { label, count: 0, bytes: 0 });
            groups.len() - 1
        });
        groups[i].count += 1;
        groups[i].bytes += a.size;
    }
    groups.sort_by_key(|g| std::cmp::Reverse(g.bytes));
    let mut block_use: Vec<(u64, u64)> = report
        .blocks
        .iter()
        .map(|b| {
            let used = report.allocations.get(b.allocations.clone()).map_or(0, |a| a.iter().map(|x| x.size).sum());
            (b.size, used)
        })
        .collect();
    block_use.sort_by_key(|(size, used)| std::cmp::Reverse(size.saturating_sub(*used)));
    Some(AllocatorSummary {
        allocated: report.total_allocated_bytes,
        reserved: report.total_reserved_bytes,
        blocks: report.blocks.len(),
        groups,
        block_use,
    })
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
    fn budgets_refuse_what_does_not_fit() {
        let none = MemoryBudget::default();
        assert!(!none.active() && none.fits(u64::MAX / 2, 1 << 40));
        let b = MemoryBudget { budget: 1000, untracked: 200, ..MemoryBudget::default() };
        assert!(b.fits(500, 300));
        assert!(!b.fits(500, 301));
        assert!(!b.fits(900, 0), "untracked memory counts");
    }

    #[test]
    fn heavy_features_become_stale_after_the_idle_time() {
        let idle = std::time::Duration::from_secs(8);
        let t0 = std::time::Instant::now();
        let mut h = HeavyUse::default();
        assert_eq!(h.stale(t0, idle), HeavyFlags { sun: true, sky: true, local: true, control: true });
        h.note_at(t0, [true, false, true, false]);
        let s = h.stale(t0 + std::time::Duration::from_secs(2), idle);
        assert!(!s.sun && s.sky && !s.local && s.control);
        assert!(h.stale(t0 + idle, idle).sun);
        let wait = h.next_release(t0 + std::time::Duration::from_secs(3), idle, [true, false, false, false]);
        assert_eq!(wait, Some(std::time::Duration::from_secs(5)));
        assert_eq!(h.next_release(t0, idle, [false; 4]), None);
        assert_eq!(h.next_release(t0, idle, [false, true, false, false]), Some(std::time::Duration::ZERO));
    }

    #[test]
    fn allocation_labels_group_and_sort_into_categories() {
        assert_eq!(group_label("egui_texid_Managed(12)"), "egui_texid_Managed(#)");
        assert_eq!(group_label(""), "unlabeled");
        assert_eq!(category_of("hp-render lightmap atlas"), "lightmaps");
        assert_eq!(category_of("hp-render texture"), "textures");
        assert_eq!(category_of("hp-render sun shadow"), "shadows");
        let s = AllocatorSummary {
            groups: vec![
                AllocationGroup { label: "a texture".into(), count: 1, bytes: 10 },
                AllocationGroup { label: "b texture".into(), count: 1, bytes: 5 },
                AllocationGroup { label: "depth".into(), count: 1, bytes: 30 },
            ],
            ..AllocatorSummary::default()
        };
        assert_eq!(s.category_bytes(), vec![("render_targets", 30), ("textures", 15)]);
    }

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
