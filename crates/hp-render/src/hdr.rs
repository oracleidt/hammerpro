use std::collections::HashMap;
use std::sync::Arc;
use std::sync::atomic::{AtomicU8, Ordering};

use bytemuck::{Pod, Zeroable};

use crate::pipelines::LDR_FORMAT;
use crate::{CompiledLightMode, Renderer, ViewOptions};

pub const BUCKETS: usize = 16;

const LUMINANCE: [f32; 3] = [0.2126, 0.7152, 0.0722];
const BLOOM_LUMINANCE: [f32; 3] = [0.299, 0.587, 0.114];
const TARGET_PERCENT: f32 = 60.0;
const BRIGHT_PERCENT: f32 = 2.0;
const MIN_AVERAGE_LUMINANCE: f32 = 3.0;
const ADAPT_SECONDS: f32 = 0.15;
const MAX_STEP: f32 = 0.5;
const REGION: [f32; 2] = [0.9, 0.85];
const BLOOM_FORMAT: wgpu::TextureFormat = wgpu::TextureFormat::Rgba16Float;
const MAP_WAITING: u8 = 0;
const MAP_OK: u8 = 1;
const MAP_FAILED: u8 = 2;
const READ_WAIT: std::time::Duration = std::time::Duration::from_millis(50);

pub const BLOOM_EXPONENT: f32 = 1.5;
pub const BLOOM_KERNEL: f32 = 0.375;
pub const BLOOM_SQRT_BLUR: bool = false;
pub const BLOOM_GAIN: f32 = 2.68;

#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub enum ToneMode {
    Off,
    Fixed,
    #[default]
    Auto,
}

#[derive(Clone, Copy, Debug, PartialEq)]
pub struct ToneSettings {
    pub mode: ToneMode,
    pub exposure: f32,
    pub exposure_min: f32,
    pub exposure_max: f32,
    pub bloom: f32,
    pub map_overrides: bool,
}

impl Default for ToneSettings {
    fn default() -> Self {
        Self {
            mode: ToneMode::Auto,
            exposure: 1.0,
            exposure_min: 0.5,
            exposure_max: 2.0,
            bloom: 1.0,
            map_overrides: true,
        }
    }
}

#[derive(Clone, Copy, Debug, Default, PartialEq)]
pub struct ToneEnv {
    pub exposure_min: Option<f32>,
    pub exposure_max: Option<f32>,
    pub bloom_scale: Option<f32>,
}

impl ToneSettings {
    pub fn sanitized(self) -> Self {
        let fin = |v: f32, d: f32| if v.is_finite() { v } else { d };
        Self {
            mode: self.mode,
            exposure: fin(self.exposure, 1.0).clamp(0.01, 64.0),
            exposure_min: fin(self.exposure_min, 0.5).clamp(0.001, 64.0),
            exposure_max: fin(self.exposure_max, 2.0).clamp(0.001, 64.0),
            bloom: fin(self.bloom, 1.0).clamp(0.0, 64.0),
            map_overrides: self.map_overrides,
        }
    }

    pub fn range(&self, env: &ToneEnv) -> (f32, f32) {
        if self.map_overrides {
            (env.exposure_min.unwrap_or(self.exposure_min), env.exposure_max.unwrap_or(self.exposure_max))
        } else {
            (self.exposure_min, self.exposure_max)
        }
    }

    pub fn bloom_amount(&self, env: &ToneEnv) -> f32 {
        let base = if self.map_overrides { env.bloom_scale.unwrap_or(1.0) } else { 1.0 };
        (base * self.bloom).max(0.0)
    }
}

pub(crate) fn clamp_range(v: f32, min: f32, max: f32) -> f32 {
    v.min(max).max(min)
}

fn edge(b: usize) -> f32 {
    (b as f32 / BUCKETS as f32).powf(1.5)
}

#[cfg(test)]
pub(crate) fn bucket_of(lum: f32) -> usize {
    if lum.is_nan() || lum <= 0.0 {
        return 0;
    }
    if lum >= 1.0 {
        return BUCKETS - 1;
    }
    ((lum.powf(2.0 / 3.0) * BUCKETS as f32) as usize).min(BUCKETS - 1)
}

pub(crate) fn location(bins: &[u32; BUCKETS], percent: f32, snap: Option<f32>) -> Option<f32> {
    let total: u64 = bins.iter().map(|b| u64::from(*b)).sum();
    if total == 0 {
        return None;
    }
    let mut tested_pixels = 0.0f32;
    let mut tested_range = 0.0f32;
    for i in (0..BUCKETS).rev() {
        let need = percent / 100.0 - tested_pixels;
        let frac = bins[i] as f32 / total as f32;
        let (lo, hi) = (edge(i), edge(i + 1));
        if frac >= need && frac > 0.0 {
            if let Some(s) = snap.map(|s| s / 100.0)
                && lo <= s
                && s <= hi
            {
                return Some(s);
            }
            let loc = 1.0 - (tested_range + (hi - lo) * (need / frac));
            return Some(loc.clamp(lo, hi));
        }
        tested_pixels += frac;
        tested_range += hi - lo;
    }
    None
}

pub(crate) fn target_scale(bins: &[u32; BUCKETS], current: f32) -> f32 {
    let target = TARGET_PERCENT / 100.0;
    let at = location(bins, BRIGHT_PERCENT, Some(TARGET_PERCENT)).unwrap_or(target).max(1e-4);
    let mut scalar = target / at;
    if let Some(avg) = location(bins, 50.0, None).filter(|l| *l > 0.0) {
        let second = (MIN_AVERAGE_LUMINANCE / 100.0) / avg;
        if second > scalar {
            scalar = second;
        }
    }
    (scalar * current).max(0.001)
}

pub(crate) fn adapt(scale: f32, target: f32, dt: f32, min: f32, max: f32) -> f32 {
    let dt = if dt.is_finite() { dt.clamp(0.0, MAX_STEP) } else { 0.0 };
    let goal = clamp_range(target, min, max);
    let k = 1.0 - (-dt / ADAPT_SECONDS).exp();
    clamp_range(scale * (goal / scale.max(1e-4)).powf(k), min, max)
}

#[repr(C)]
#[derive(Clone, Copy, Debug, Default, PartialEq, Pod, Zeroable)]
struct HdrParams {
    size: [u32; 2],
    inset: [u32; 2],
    lum: [f32; 4],
    bloom: [f32; 4],
    blur: [f32; 4],
}

#[repr(C)]
#[derive(Clone, Copy, Debug, Default, PartialEq, Pod, Zeroable)]
struct BloomParams {
    lum: [f32; 4],
    blur: [f32; 4],
}

#[repr(C)]
#[derive(Clone, Copy, Debug, Default, PartialEq, Pod, Zeroable)]
struct PresentParams {
    bloom: [f32; 4],
    fade: [f32; 4],
}

struct Gpu {
    present: wgpu::RenderPipeline,
    present_bgl: wgpu::BindGroupLayout,
    sampler: wgpu::Sampler,
    hist: wgpu::ComputePipeline,
    hist_bgl: wgpu::BindGroupLayout,
    bloom_bgl: wgpu::BindGroupLayout,
    down: wgpu::RenderPipeline,
    blur_x: wgpu::RenderPipeline,
    blur_y: wgpu::RenderPipeline,
}

struct Hist {
    bins: wgpu::Buffer,
    readback: wgpu::Buffer,
    params: wgpu::Buffer,
    pending: bool,
    copied: bool,
    inflight: Option<(Arc<AtomicU8>, wgpu::SubmissionIndex)>,
}

struct Targets {
    size: [u32; 2],
    a: wgpu::TextureView,
    b: wgpu::TextureView,
    params: wgpu::Buffer,
}

#[derive(Default)]
struct Binds {
    hist: Option<(wgpu::TextureView, wgpu::BindGroup)>,
    bloom: Option<([wgpu::TextureView; 3], [wgpu::BindGroup; 3])>,
    present: Option<([wgpu::TextureView; 2], wgpu::BindGroup)>,
}

struct ViewTone {
    scale: f32,
    mode: crate::RenderMode,
    hist: Option<Hist>,
    bloom: Option<Targets>,
    present: Option<wgpu::Buffer>,
    bins: Option<[u32; BUCKETS]>,
    binds: Binds,
}

impl Default for ViewTone {
    fn default() -> Self {
        Self {
            scale: 1.0,
            mode: crate::RenderMode::Textured,
            hist: None,
            bloom: None,
            present: None,
            bins: None,
            binds: Binds::default(),
        }
    }
}

#[derive(Default)]
pub(crate) struct Tone {
    pub(crate) settings: ToneSettings,
    pub(crate) env: ToneEnv,
    pub(crate) lit_hdr: bool,
    pub(crate) lit_ldr_shading: bool,
    pub(crate) lazy_readback: bool,
    gpu: Option<Gpu>,
    views: HashMap<u32, ViewTone>,
}

fn storage_entry(binding: u32, read_only: bool) -> wgpu::BindGroupLayoutEntry {
    wgpu::BindGroupLayoutEntry {
        binding,
        visibility: wgpu::ShaderStages::COMPUTE,
        ty: wgpu::BindingType::Buffer {
            ty: wgpu::BufferBindingType::Storage { read_only },
            has_dynamic_offset: false,
            min_binding_size: None,
        },
        count: None,
    }
}

fn uniform_entry(binding: u32, visibility: wgpu::ShaderStages) -> wgpu::BindGroupLayoutEntry {
    wgpu::BindGroupLayoutEntry {
        binding,
        visibility,
        ty: wgpu::BindingType::Buffer {
            ty: wgpu::BufferBindingType::Uniform,
            has_dynamic_offset: false,
            min_binding_size: None,
        },
        count: None,
    }
}

fn texture_entry(binding: u32, visibility: wgpu::ShaderStages, filterable: bool) -> wgpu::BindGroupLayoutEntry {
    wgpu::BindGroupLayoutEntry {
        binding,
        visibility,
        ty: wgpu::BindingType::Texture {
            sample_type: wgpu::TextureSampleType::Float { filterable },
            view_dimension: wgpu::TextureViewDimension::D2,
            multisampled: false,
        },
        count: None,
    }
}

fn sampler_entry(binding: u32) -> wgpu::BindGroupLayoutEntry {
    wgpu::BindGroupLayoutEntry {
        binding,
        visibility: wgpu::ShaderStages::FRAGMENT,
        ty: wgpu::BindingType::Sampler(wgpu::SamplerBindingType::Filtering),
        count: None,
    }
}

fn fullscreen_pipeline(
    device: &wgpu::Device,
    module: &wgpu::ShaderModule,
    layout: &wgpu::PipelineLayout,
    fs: &str,
    format: wgpu::TextureFormat,
) -> wgpu::RenderPipeline {
    device.create_render_pipeline(&wgpu::RenderPipelineDescriptor {
        label: Some("hp-render hdr"),
        layout: Some(layout),
        vertex: wgpu::VertexState {
            module,
            entry_point: Some("vs_full"),
            compilation_options: wgpu::PipelineCompilationOptions::default(),
            buffers: &[],
        },
        primitive: wgpu::PrimitiveState::default(),
        depth_stencil: None,
        multisample: wgpu::MultisampleState::default(),
        fragment: Some(wgpu::FragmentState {
            module,
            entry_point: Some(fs),
            compilation_options: wgpu::PipelineCompilationOptions::default(),
            targets: &[Some(wgpu::ColorTargetState { format, blend: None, write_mask: wgpu::ColorWrites::ALL })],
        }),
        multiview_mask: None,
        cache: None,
    })
}

impl Gpu {
    fn new(device: &wgpu::Device) -> Self {
        let frag = wgpu::ShaderStages::FRAGMENT;
        let present_bgl = device.create_bind_group_layout(&wgpu::BindGroupLayoutDescriptor {
            label: Some("hp-render present"),
            entries: &[
                texture_entry(0, frag, false),
                texture_entry(1, frag, true),
                sampler_entry(2),
                uniform_entry(3, frag),
            ],
        });
        let present_module = device.create_shader_module(wgpu::ShaderModuleDescriptor {
            label: Some("hp-render present"),
            source: wgpu::ShaderSource::Wgsl(include_str!("shaders/present.wgsl").into()),
        });
        let present_layout = device.create_pipeline_layout(&wgpu::PipelineLayoutDescriptor {
            label: Some("hp-render present"),
            bind_group_layouts: &[Some(&present_bgl)],
            immediate_size: 0,
        });
        let present = fullscreen_pipeline(device, &present_module, &present_layout, "fs_present", LDR_FORMAT);
        let bloom_bgl = device.create_bind_group_layout(&wgpu::BindGroupLayoutDescriptor {
            label: Some("hp-render bloom"),
            entries: &[texture_entry(0, frag, true), sampler_entry(1), uniform_entry(2, frag)],
        });
        let bloom_module = device.create_shader_module(wgpu::ShaderModuleDescriptor {
            label: Some("hp-render bloom"),
            source: wgpu::ShaderSource::Wgsl(include_str!("shaders/bloom.wgsl").into()),
        });
        let bloom_layout = device.create_pipeline_layout(&wgpu::PipelineLayoutDescriptor {
            label: Some("hp-render bloom"),
            bind_group_layouts: &[Some(&bloom_bgl)],
            immediate_size: 0,
        });
        let down = fullscreen_pipeline(device, &bloom_module, &bloom_layout, "fs_down", BLOOM_FORMAT);
        let blur_x = fullscreen_pipeline(device, &bloom_module, &bloom_layout, "fs_blur_x", BLOOM_FORMAT);
        let blur_y = fullscreen_pipeline(device, &bloom_module, &bloom_layout, "fs_blur_y", BLOOM_FORMAT);
        let hist_bgl = device.create_bind_group_layout(&wgpu::BindGroupLayoutDescriptor {
            label: Some("hp-render histogram"),
            entries: &[
                texture_entry(0, wgpu::ShaderStages::COMPUTE, false),
                storage_entry(1, false),
                uniform_entry(2, wgpu::ShaderStages::COMPUTE),
            ],
        });
        let hist_module = device.create_shader_module(wgpu::ShaderModuleDescriptor {
            label: Some("hp-render histogram"),
            source: wgpu::ShaderSource::Wgsl(include_str!("shaders/hdr.wgsl").into()),
        });
        let hist_layout = device.create_pipeline_layout(&wgpu::PipelineLayoutDescriptor {
            label: Some("hp-render histogram"),
            bind_group_layouts: &[Some(&hist_bgl)],
            immediate_size: 0,
        });
        let hist = device.create_compute_pipeline(&wgpu::ComputePipelineDescriptor {
            label: Some("hp-render histogram"),
            layout: Some(&hist_layout),
            module: &hist_module,
            entry_point: Some("histogram"),
            compilation_options: wgpu::PipelineCompilationOptions::default(),
            cache: None,
        });
        let sampler = device.create_sampler(&wgpu::SamplerDescriptor {
            label: Some("hp-render hdr"),
            address_mode_u: wgpu::AddressMode::ClampToEdge,
            address_mode_v: wgpu::AddressMode::ClampToEdge,
            mag_filter: wgpu::FilterMode::Linear,
            min_filter: wgpu::FilterMode::Linear,
            ..Default::default()
        });
        Self { present, present_bgl, sampler, hist, hist_bgl, bloom_bgl, down, blur_x, blur_y }
    }
}

fn uniform_buffer(device: &wgpu::Device, label: &str, size: usize) -> wgpu::Buffer {
    device.create_buffer(&wgpu::BufferDescriptor {
        label: Some(label),
        size: size as u64,
        usage: wgpu::BufferUsages::UNIFORM | wgpu::BufferUsages::COPY_DST,
        mapped_at_creation: false,
    })
}

fn bloom_target(device: &wgpu::Device, size: [u32; 2]) -> wgpu::TextureView {
    device
        .create_texture(&wgpu::TextureDescriptor {
            label: Some("hp-render bloom"),
            size: wgpu::Extent3d { width: size[0], height: size[1], depth_or_array_layers: 1 },
            mip_level_count: 1,
            sample_count: 1,
            dimension: wgpu::TextureDimension::D2,
            format: BLOOM_FORMAT,
            usage: wgpu::TextureUsages::RENDER_ATTACHMENT | wgpu::TextureUsages::TEXTURE_BINDING,
            view_formats: &[],
        })
        .create_view(&wgpu::TextureViewDescriptor::default())
}

fn fullscreen_pass(
    enc: &mut wgpu::CommandEncoder,
    pipeline: &wgpu::RenderPipeline,
    bind: &wgpu::BindGroup,
    target: &wgpu::TextureView,
) {
    let mut rp = enc.begin_render_pass(&wgpu::RenderPassDescriptor {
        label: Some("hp-render hdr"),
        color_attachments: &[Some(wgpu::RenderPassColorAttachment {
            view: target,
            depth_slice: None,
            resolve_target: None,
            ops: wgpu::Operations { load: wgpu::LoadOp::Clear(wgpu::Color::BLACK), store: wgpu::StoreOp::Store },
        })],
        depth_stencil_attachment: None,
        timestamp_writes: None,
        occlusion_query_set: None,
        multiview_mask: None,
    });
    rp.set_pipeline(pipeline);
    rp.set_bind_group(0, bind, &[]);
    rp.draw(0..3, 0..1);
}

pub(crate) struct FrameInputs<'a> {
    pub color: &'a wgpu::Texture,
    pub color_view: &'a wgpu::TextureView,
    pub display: &'a wgpu::TextureView,
    pub size: [u32; 2],
    pub chain: bool,
    pub fade: [f32; 4],
    pub fade_modulate: bool,
}

impl Renderer {
    pub fn set_tone_settings(&mut self, settings: ToneSettings) {
        let s = settings.sanitized();
        if self.tone.settings != s {
            self.tone.settings = s;
            self.content_gen += 1;
        }
    }

    pub fn set_lazy_exposure(&mut self, on: bool) {
        self.tone.lazy_readback = on;
    }

    pub fn tone_settings(&self) -> ToneSettings {
        self.tone.settings
    }

    pub fn set_tone_env(&mut self, env: ToneEnv) {
        let fin = |v: Option<f32>| v.filter(|x| x.is_finite() && *x >= 0.0);
        let env = ToneEnv {
            exposure_min: fin(env.exposure_min),
            exposure_max: fin(env.exposure_max),
            bloom_scale: fin(env.bloom_scale),
        };
        if self.tone.env != env {
            self.tone.env = env;
            self.content_gen += 1;
        }
    }

    pub fn tone_env(&self) -> ToneEnv {
        self.tone.env
    }

    pub fn set_lit_ldr_shading(&mut self, on: bool) {
        if self.tone.lit_ldr_shading != on {
            self.tone.lit_ldr_shading = on;
            self.content_gen += 1;
        }
    }

    pub fn set_lit_hdr(&mut self, hdr: bool) {
        if self.tone.lit_hdr != hdr {
            self.tone.lit_hdr = hdr;
            self.content_gen += 1;
        }
    }

    pub(crate) fn tone_chain(&self, opts: &ViewOptions, mode: crate::RenderMode, ortho: bool) -> bool {
        self.tone.settings.mode != ToneMode::Off && self.hdr_lit(opts, mode, ortho)
    }

    pub(crate) fn hdr_lit(&self, opts: &ViewOptions, mode: crate::RenderMode, ortho: bool) -> bool {
        if ortho {
            return false;
        }
        let compiled = opts.compiled_lighting && crate::compiled_light::textured_mode(mode);
        match self.compiled.mode() {
            Some(CompiledLightMode::Hdr) if compiled => true,
            Some(_) if compiled => false,
            _ => mode == crate::RenderMode::Lit && self.tone.lit_hdr,
        }
    }

    pub fn tone_scale(&self, view_id: u32) -> f32 {
        match self.tone.settings.mode {
            ToneMode::Off => 1.0,
            ToneMode::Fixed => self.tone.settings.exposure,
            ToneMode::Auto => self.tone.views.get(&view_id).map_or(1.0, |v| v.scale),
        }
    }

    pub(crate) fn tone_scale_for(&self, view_id: u32, chain: bool) -> f32 {
        if chain { self.tone_scale(view_id) } else { 1.0 }
    }

    pub fn tone_bloom(&self) -> f32 {
        self.tone.settings.bloom_amount(&self.tone.env)
    }

    pub(crate) fn prepare_tone(&mut self, view_id: u32, size: [u32; 2], mode: crate::RenderMode, chain: bool) {
        if self.tone.gpu.is_none() {
            self.tone.gpu = Some(Gpu::new(&self.device));
        }
        if !chain {
            if let Some(v) = self.tone.views.get_mut(&view_id) {
                v.scale = 1.0;
                v.mode = mode;
            }
            return;
        }
        let device = &self.device;
        let want_bloom = self.tone.settings.bloom_amount(&self.tone.env) > 0.0;
        let auto = self.tone.settings.mode == ToneMode::Auto;
        let quarter = [size[0].div_ceil(4).max(1), size[1].div_ceil(4).max(1)];
        let v = self.tone.views.entry(view_id).or_default();
        v.mode = mode;
        if auto && v.hist.is_none() {
            let buf = |label, size: u64, usage| {
                device.create_buffer(&wgpu::BufferDescriptor {
                    label: Some(label),
                    size,
                    usage,
                    mapped_at_creation: false,
                })
            };
            v.hist = Some(Hist {
                bins: buf(
                    "hp-render exposure bins",
                    (BUCKETS * 4) as u64,
                    wgpu::BufferUsages::STORAGE | wgpu::BufferUsages::COPY_SRC | wgpu::BufferUsages::COPY_DST,
                ),
                readback: buf(
                    "hp-render exposure readback",
                    (BUCKETS * 4) as u64,
                    wgpu::BufferUsages::MAP_READ | wgpu::BufferUsages::COPY_DST,
                ),
                params: uniform_buffer(device, "hp-render exposure params", size_of::<HdrParams>()),
                pending: false,
                copied: false,
                inflight: None,
            });
        }
        if want_bloom && v.bloom.as_ref().is_none_or(|b| b.size != quarter) {
            v.bloom = Some(Targets {
                size: quarter,
                a: bloom_target(device, quarter),
                b: bloom_target(device, quarter),
                params: uniform_buffer(device, "hp-render bloom params", size_of::<BloomParams>()),
            });
        }
        if v.present.is_none() {
            v.present = Some(uniform_buffer(device, "hp-render present params", size_of::<PresentParams>()));
        }
    }

    pub(crate) fn record_present(&mut self, enc: &mut wgpu::CommandEncoder, view_id: u32, f: &FrameInputs<'_>) {
        let device = &self.device;
        let queue = &self.queue;
        let Tone { settings, env, gpu, views, .. } = &mut self.tone;
        let Some(gpu) = gpu.as_ref() else { return };
        let amount = if f.chain { settings.bloom_amount(env) } else { 0.0 };
        let auto = f.chain && settings.mode == ToneMode::Auto;
        let v = views.entry(view_id).or_default();
        let mut bloom_ready = false;
        let mut bloom_view: Option<wgpu::TextureView> = None;
        if f.chain {
            if auto && let Some(h) = v.hist.as_mut() {
                let inset = [
                    (f.size[0] as f32 * 0.5 * (1.0 - REGION[0])) as u32,
                    (f.size[1] as f32 * 0.5 * (1.0 - REGION[1])) as u32,
                ];
                let params = HdrParams {
                    size: f.size,
                    inset,
                    lum: [LUMINANCE[0], LUMINANCE[1], LUMINANCE[2], 0.0],
                    bloom: [0.0; 4],
                    blur: [0.0; 4],
                };
                queue.write_buffer(&h.params, 0, bytemuck::bytes_of(&params));
                if v.binds.hist.as_ref().is_none_or(|(k, _)| k != f.color_view) {
                    let bg = device.create_bind_group(&wgpu::BindGroupDescriptor {
                        label: Some("hp-render histogram"),
                        layout: &gpu.hist_bgl,
                        entries: &[
                            wgpu::BindGroupEntry {
                                binding: 0,
                                resource: wgpu::BindingResource::TextureView(f.color_view),
                            },
                            wgpu::BindGroupEntry { binding: 1, resource: h.bins.as_entire_binding() },
                            wgpu::BindGroupEntry { binding: 2, resource: h.params.as_entire_binding() },
                        ],
                    });
                    v.binds.hist = Some((f.color_view.clone(), bg));
                }
                if h.inflight.is_none()
                    && let Some((_, bg)) = v.binds.hist.as_ref()
                {
                    enc.clear_buffer(&h.bins, 0, None);
                    let mut pass = enc.begin_compute_pass(&wgpu::ComputePassDescriptor {
                        label: Some("hp-render histogram"),
                        timestamp_writes: None,
                    });
                    pass.set_pipeline(&gpu.hist);
                    pass.set_bind_group(0, bg, &[]);
                    let w = f.size[0].saturating_sub(2 * inset[0]).div_ceil(2).div_ceil(16).max(1);
                    let hh = f.size[1].saturating_sub(2 * inset[1]).div_ceil(2).div_ceil(16).max(1);
                    pass.dispatch_workgroups(w, hh, 1);
                    drop(pass);
                    enc.copy_buffer_to_buffer(&h.bins, 0, &h.readback, 0, (BUCKETS * 4) as u64);
                    h.pending = true;
                    h.copied = true;
                }
            }
            if amount > 0.0
                && let Some(b) = v.bloom.as_ref()
            {
                let lum = [BLOOM_LUMINANCE[0], BLOOM_LUMINANCE[1], BLOOM_LUMINANCE[2], BLOOM_EXPONENT];
                let blur = [BLOOM_KERNEL, if BLOOM_SQRT_BLUR { 1.0 } else { 0.0 }, 0.0, 0.0];
                queue.write_buffer(&b.params, 0, bytemuck::bytes_of(&BloomParams { lum, blur }));
                let key = [f.color_view.clone(), b.a.clone(), b.b.clone()];
                if v.binds.bloom.as_ref().is_none_or(|(k, _)| *k != key) {
                    let bind = |src: &wgpu::TextureView| {
                        device.create_bind_group(&wgpu::BindGroupDescriptor {
                            label: Some("hp-render bloom"),
                            layout: &gpu.bloom_bgl,
                            entries: &[
                                wgpu::BindGroupEntry { binding: 0, resource: wgpu::BindingResource::TextureView(src) },
                                wgpu::BindGroupEntry {
                                    binding: 1,
                                    resource: wgpu::BindingResource::Sampler(&gpu.sampler),
                                },
                                wgpu::BindGroupEntry { binding: 2, resource: b.params.as_entire_binding() },
                            ],
                        })
                    };
                    let groups = [bind(f.color_view), bind(&b.a), bind(&b.b)];
                    v.binds.bloom = Some((key, groups));
                }
                if let Some((_, [down, blur_x, blur_y])) = v.binds.bloom.as_ref() {
                    fullscreen_pass(enc, &gpu.down, down, &b.a);
                    fullscreen_pass(enc, &gpu.blur_x, blur_x, &b.b);
                    fullscreen_pass(enc, &gpu.blur_y, blur_y, &b.a);
                    bloom_ready = true;
                    bloom_view = Some(b.a.clone());
                }
            }
        }
        let _ = f.color;
        let params = PresentParams {
            bloom: [
                if bloom_ready { amount * BLOOM_GAIN } else { 0.0 },
                f32::from(u8::from(f.fade_modulate)),
                0.0,
                0.0,
            ],
            fade: f.fade,
        };
        if v.present.is_none() {
            v.present = Some(uniform_buffer(device, "hp-render present params", size_of::<PresentParams>()));
            v.binds.present = None;
        }
        let Some(buffer) = v.present.as_ref() else { return };
        queue.write_buffer(buffer, 0, bytemuck::bytes_of(&params));
        let bloom_view = bloom_view.unwrap_or_else(|| f.color_view.clone());
        let key = [f.color_view.clone(), bloom_view];
        if v.binds.present.as_ref().is_none_or(|(k, _)| *k != key) {
            let bg = device.create_bind_group(&wgpu::BindGroupDescriptor {
                label: Some("hp-render present"),
                layout: &gpu.present_bgl,
                entries: &[
                    wgpu::BindGroupEntry { binding: 0, resource: wgpu::BindingResource::TextureView(&key[0]) },
                    wgpu::BindGroupEntry { binding: 1, resource: wgpu::BindingResource::TextureView(&key[1]) },
                    wgpu::BindGroupEntry { binding: 2, resource: wgpu::BindingResource::Sampler(&gpu.sampler) },
                    wgpu::BindGroupEntry { binding: 3, resource: buffer.as_entire_binding() },
                ],
            });
            v.binds.present = Some((key, bg));
        }
        let Some((_, bg)) = v.binds.present.as_ref() else { return };
        let mut rp = enc.begin_render_pass(&wgpu::RenderPassDescriptor {
            label: Some("hp-render present"),
            color_attachments: &[Some(wgpu::RenderPassColorAttachment {
                view: f.display,
                depth_slice: None,
                resolve_target: None,
                ops: wgpu::Operations { load: wgpu::LoadOp::Clear(wgpu::Color::BLACK), store: wgpu::StoreOp::Store },
            })],
            depth_stencil_attachment: None,
            timestamp_writes: None,
            occlusion_query_set: None,
            multiview_mask: None,
        });
        rp.set_pipeline(&gpu.present);
        rp.set_bind_group(0, bg, &[]);
        rp.draw(0..3, 0..1);
    }

    pub fn step_tone(&mut self, view_id: u32, dt: f32) -> f32 {
        let (min, max) = self.tone.settings.range(&self.tone.env);
        let auto = self.tone.settings.mode == ToneMode::Auto;
        let mode = self.tone.views.get(&view_id).map(|v| v.mode);
        let active = mode.is_some_and(|m| self.tone_chain(&self.view_options(view_id), m, false));
        let Some(v) = self.tone.views.get_mut(&view_id) else { return self.tone_scale(view_id) };
        if !active {
            v.scale = 1.0;
            return self.tone_scale(view_id);
        }
        if !auto {
            return self.tone_scale(view_id);
        }
        let Some(h) = v.hist.as_mut() else { return v.scale };
        if h.inflight.is_none() {
            if !std::mem::take(&mut h.pending) {
                return v.scale;
            }
            let copied = std::mem::take(&mut h.copied);
            let index = match self.last_submit.clone().filter(|_| copied) {
                Some(index) => index,
                None => {
                    let mut enc = self
                        .device
                        .create_command_encoder(&wgpu::CommandEncoderDescriptor { label: Some("hp-render exposure") });
                    enc.copy_buffer_to_buffer(&h.bins, 0, &h.readback, 0, (BUCKETS * 4) as u64);
                    self.queue.submit([enc.finish()])
                }
            };
            let state = Arc::new(AtomicU8::new(MAP_WAITING));
            let done = state.clone();
            h.readback.slice(..).map_async(wgpu::MapMode::Read, move |r| {
                done.store(if r.is_ok() { MAP_OK } else { MAP_FAILED }, Ordering::Release);
            });
            h.inflight = Some((state, index));
        }
        let Some((state, index)) = h.inflight.as_ref() else { return v.scale };
        let _ = if self.tone.lazy_readback && !crate::profile::baseline("exposure") {
            self.device.poll(wgpu::PollType::Poll)
        } else {
            self.device.poll(wgpu::PollType::Wait { submission_index: Some(index.clone()), timeout: Some(READ_WAIT) })
        };
        let mapped = match state.load(Ordering::Acquire) {
            MAP_WAITING => return v.scale,
            s => s == MAP_OK,
        };
        h.inflight = None;
        if !mapped {
            return v.scale;
        }
        let mut bins = [0u32; BUCKETS];
        match h.readback.slice(..).get_mapped_range() {
            Ok(data) => bins.copy_from_slice(&bytemuck::cast_slice::<u8, u32>(&data)[..BUCKETS]),
            Err(_) => return v.scale,
        }
        h.readback.unmap();
        v.bins = Some(bins);
        let target = target_scale(&bins, v.scale);
        let next = adapt(v.scale, target, dt, min, max);
        let changed = (next / v.scale).ln().abs() > 1e-4;
        let moving = (clamp_range(target, min, max) / next).ln().abs() > 0.004;
        v.scale = next;
        if changed || moving {
            self.redraw_view(view_id);
        }
        next
    }

    pub fn tone_histogram(&self, view_id: u32) -> Option<[u32; BUCKETS]> {
        self.tone.views.get(&view_id).and_then(|v| v.bins)
    }

    pub fn inherit_tone(&mut self, from: u32, to: u32) {
        let scale = self.tone.views.get(&from).map(|v| v.scale);
        if let Some(scale) = scale {
            self.tone.views.entry(to).or_default().scale = scale;
        }
    }

    pub fn reset_tone(&mut self, view_id: u32) {
        if let Some(v) = self.tone.views.get_mut(&view_id) {
            v.scale = 1.0;
            v.bins = None;
        }
    }

    pub(crate) fn forget_tone_view(&mut self, view_id: u32) {
        self.tone.views.remove(&view_id);
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn bins_at(lum: f32, n: u32) -> [u32; BUCKETS] {
        let mut b = [0u32; BUCKETS];
        b[bucket_of(lum)] = n;
        b
    }

    #[test]
    fn buckets_follow_the_engine_edges() {
        assert_eq!(bucket_of(0.0), 0);
        assert_eq!(bucket_of(f32::NAN), 0);
        assert_eq!(bucket_of(0.9999), 15);
        assert_eq!(bucket_of(7.0), 15);
        assert_eq!(bucket_of(edge(11) + 1e-4), 11);
        assert_eq!(bucket_of(edge(12) - 1e-4), 11);
        assert!((edge(16) - 1.0).abs() < 1e-6 && edge(0) == 0.0);
    }

    #[test]
    fn the_exposure_settles_where_the_bright_pixels_reach_the_target() {
        let mut b = [0u32; BUCKETS];
        b[3] = 900;
        b[11] = 100;
        let at = location(&b, BRIGHT_PERCENT, None).expect("location");
        assert!(at > edge(11) && at < edge(12), "{at}");
        let t = target_scale(&b, 1.37);
        assert!(
            (t - 1.37).abs() < 1e-6,
            "the engine keeps its scale while the bright pixels sit in the target bucket: {t}"
        );
        let snapped = location(&b, BRIGHT_PERCENT, Some(TARGET_PERCENT)).expect("snap");
        assert!((snapped - 0.6).abs() < 1e-6);
        let mut low = [0u32; BUCKETS];
        low[3] = 900;
        low[9] = 100;
        let below = location(&low, BRIGHT_PERCENT, None).expect("location");
        let t = target_scale(&low, 1.37);
        assert!((t - 1.37 * 0.6 / below).abs() < 1e-4, "{t}");
        assert!(adapt(1.37, t, 0.05, 0.5, 2.0) > 1.37);
    }

    #[test]
    fn dark_and_bright_scenes_move_the_scale() {
        let dark = bins_at(0.05, 1000);
        assert!(target_scale(&dark, 1.0) > 3.0);
        let bright = bins_at(0.95, 1000);
        let t = target_scale(&bright, 1.0);
        assert!(t < 0.75 && t > 0.5, "{t}");
        assert!(target_scale(&[0; BUCKETS], 1.0) == 1.0);
    }

    #[test]
    fn the_average_floor_brightens_dim_scenes() {
        let mut b = [0u32; BUCKETS];
        b[0] = 990;
        b[5] = 10;
        let t = target_scale(&b, 1.0);
        let top = location(&b, 2.0, Some(60.0)).unwrap_or(0.6);
        assert!(top < 0.3);
        assert!(t >= 0.6 / top - 1e-3, "{t}");
    }

    #[test]
    fn adaptation_converges_inside_the_range() {
        let mut s = 1.0;
        for _ in 0..120 {
            s = adapt(s, 1.8, 0.05, 0.7, 1.8);
        }
        assert!((s - 1.8).abs() < 1e-3, "{s}");
        let k = 1.0 - (-0.5f32 / ADAPT_SECONDS).exp();
        assert!((adapt(1.0, 9.0, 0.5, 0.5, 2.0) - 2.0f32.powf(k)).abs() < 1e-5);
        assert!(adapt(3.0, 3.0, 0.1, 0.5, 2.0) <= 2.0);
        assert_eq!(adapt(1.0, 1.0, f32::NAN, 0.5, 2.0), 1.0);
    }

    #[test]
    fn map_overrides_apply_only_when_allowed() {
        let env = ToneEnv { exposure_min: Some(0.7), exposure_max: Some(1.8), bloom_scale: Some(0.3) };
        let s = ToneSettings::default();
        assert_eq!(s.range(&env), (0.7, 1.8));
        assert!((s.bloom_amount(&env) - 0.3).abs() < 1e-6);
        let off = ToneSettings { map_overrides: false, ..s };
        assert_eq!(off.range(&env), (0.5, 2.0));
        assert_eq!(off.bloom_amount(&env), 1.0);
        assert_eq!(ToneSettings { bloom: 0.0, ..s }.bloom_amount(&ToneEnv::default()), 0.0);
        let bad =
            ToneSettings { exposure: f32::NAN, exposure_min: -3.0, exposure_max: f32::INFINITY, bloom: -1.0, ..s }
                .sanitized();
        assert!(bad.exposure == 1.0 && bad.exposure_min > 0.0 && bad.exposure_max == 2.0 && bad.bloom == 0.0);
    }
}
