use std::sync::Arc;
use std::sync::atomic::{AtomicU32, Ordering};

pub const MARKS: usize = 4;
const SAMPLES: usize = 6;
const STRIDE: u64 = 256;
const IDLE: u32 = 0;
const PENDING: u32 = 1;
const READY: u32 = 2;
const FAILED: u32 = 3;

#[derive(Clone, Copy, Debug, Default, PartialEq)]
pub struct GpuTimes {
    pub shadows_ms: f32,
    pub scene_ms: f32,
    pub post_ms: f32,
    pub total_ms: f32,
}

#[derive(Clone, Copy, Debug, Default, PartialEq)]
pub struct CpuTimes {
    pub prepare_ms: f32,
    pub record_ms: f32,
    pub submit_ms: f32,
    pub setup_ms: f32,
    pub clusters_ms: f32,
    pub shadow_prep_ms: f32,
    pub models_ms: f32,
}

pub(crate) struct Lap(std::time::Instant);

impl Lap {
    pub fn new() -> Self {
        Self(std::time::Instant::now())
    }

    pub fn take(&mut self) -> f32 {
        let now = std::time::Instant::now();
        let ms = now.duration_since(self.0).as_secs_f32() * 1000.0;
        self.0 = now;
        ms
    }
}

#[derive(Clone, Copy, Debug, Default, PartialEq)]
pub struct FrameTimes {
    pub cpu: CpuTimes,
    pub gpu: Option<GpuTimes>,
    pub reflection_gpu_ms: f32,
    pub reflection_cpu_ms: f32,
    pub gpu_supported: bool,
}

struct Sample {
    read: wgpu::Buffer,
    state: Arc<AtomicU32>,
    reflection: bool,
}

pub(crate) struct GpuProfiler {
    set: wgpu::QuerySet,
    resolve: wgpu::Buffer,
    samples: Vec<Sample>,
    period: f32,
    active: Option<usize>,
    pub latest: Option<GpuTimes>,
    pub latest_reflection: f32,
}

pub fn wanted_features(adapter: &wgpu::Adapter) -> wgpu::Features {
    let both = wgpu::Features::TIMESTAMP_QUERY | wgpu::Features::TIMESTAMP_QUERY_INSIDE_ENCODERS;
    if adapter.features().contains(both) { both } else { wgpu::Features::empty() }
}

impl GpuProfiler {
    pub fn new(device: &wgpu::Device, queue: &wgpu::Queue) -> Option<Self> {
        let both = wgpu::Features::TIMESTAMP_QUERY | wgpu::Features::TIMESTAMP_QUERY_INSIDE_ENCODERS;
        if !device.features().contains(both) {
            return None;
        }
        let set = device.create_query_set(&wgpu::QuerySetDescriptor {
            label: Some("hp-render timestamps"),
            ty: wgpu::QueryType::Timestamp,
            count: (SAMPLES * MARKS) as u32,
        });
        let resolve = device.create_buffer(&wgpu::BufferDescriptor {
            label: Some("hp-render timestamp resolve"),
            size: STRIDE * SAMPLES as u64,
            usage: wgpu::BufferUsages::QUERY_RESOLVE | wgpu::BufferUsages::COPY_SRC,
            mapped_at_creation: false,
        });
        let samples = (0..SAMPLES)
            .map(|_| Sample {
                read: device.create_buffer(&wgpu::BufferDescriptor {
                    label: Some("hp-render timestamp read"),
                    size: (MARKS * 8) as u64,
                    usage: wgpu::BufferUsages::MAP_READ | wgpu::BufferUsages::COPY_DST,
                    mapped_at_creation: false,
                }),
                state: Arc::new(AtomicU32::new(IDLE)),
                reflection: false,
            })
            .collect();
        let period = queue.get_timestamp_period();
        (period.is_finite() && period > 0.0).then_some(Self {
            set,
            resolve,
            samples,
            period,
            active: None,
            latest: None,
            latest_reflection: 0.0,
        })
    }

    pub fn collect(&mut self, device: &wgpu::Device) {
        let _ = device.poll(wgpu::PollType::Poll);
        let period = self.period;
        for s in self.samples.iter_mut() {
            match s.state.load(Ordering::Acquire) {
                READY => {
                    let marks: Option<[u64; MARKS]> = s.read.slice(..).get_mapped_range().ok().map(|m| {
                        let mut out = [0u64; MARKS];
                        for (i, o) in out.iter_mut().enumerate() {
                            let mut b = [0u8; 8];
                            b.copy_from_slice(&m[i * 8..i * 8 + 8]);
                            *o = u64::from_le_bytes(b);
                        }
                        out
                    });
                    s.read.unmap();
                    s.state.store(IDLE, Ordering::Release);
                    let Some(t) = marks else { continue };
                    let ms = |a: usize, b: usize| t[b].saturating_sub(t[a]) as f32 * period / 1.0e6;
                    if s.reflection {
                        self.latest_reflection = ms(0, MARKS - 1);
                    } else {
                        self.latest = Some(GpuTimes {
                            shadows_ms: ms(0, 1),
                            scene_ms: ms(1, 2),
                            post_ms: ms(2, 3),
                            total_ms: ms(0, MARKS - 1),
                        });
                    }
                }
                FAILED => {
                    s.read.unmap();
                    s.state.store(IDLE, Ordering::Release);
                }
                _ => {}
            }
        }
    }

    pub fn begin(&mut self, reflection: bool) {
        self.active = self.samples.iter().position(|s| s.state.load(Ordering::Acquire) == IDLE);
        if let Some(i) = self.active {
            self.samples[i].reflection = reflection;
        }
    }

    pub fn mark(&self, enc: &mut wgpu::CommandEncoder, slot: usize) {
        if let Some(i) = self.active {
            enc.write_timestamp(&self.set, (i * MARKS + slot) as u32);
        }
    }

    pub fn end(&mut self, enc: &mut wgpu::CommandEncoder) -> Option<usize> {
        let i = self.active.take()?;
        let first = (i * MARKS) as u32;
        let offset = i as u64 * STRIDE;
        enc.resolve_query_set(&self.set, first..first + MARKS as u32, &self.resolve, offset);
        enc.copy_buffer_to_buffer(&self.resolve, offset, &self.samples[i].read, 0, (MARKS * 8) as u64);
        self.samples[i].state.store(PENDING, Ordering::Release);
        Some(i)
    }

    pub fn after_submit(&self, sample: usize) {
        let s = &self.samples[sample];
        let state = s.state.clone();
        s.read.slice(..).map_async(wgpu::MapMode::Read, move |r| {
            state.store(if r.is_ok() { READY } else { FAILED }, Ordering::Release);
        });
    }
}

pub fn baseline(feature: &str) -> bool {
    static FLAGS: std::sync::OnceLock<Vec<String>> = std::sync::OnceLock::new();
    let flags = FLAGS.get_or_init(|| {
        std::env::var("HP_PERF_BASELINE")
            .map(|v| v.split(',').map(|s| s.trim().to_ascii_lowercase()).filter(|s| !s.is_empty()).collect())
            .unwrap_or_default()
    });
    flags.iter().any(|f| f == feature || f == "all")
}
