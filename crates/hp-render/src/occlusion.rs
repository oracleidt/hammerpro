use std::sync::Arc;
use std::sync::atomic::{AtomicU32, Ordering};

use bytemuck::{Pod, Zeroable};

use crate::arena::{Grow, GrowBuffer};
use crate::pipelines::DEPTH_FORMAT;

const OCCLUSION_WGSL: &str = include_str!("shaders/occlusion.wgsl");
const TILE: u32 = 8;
const GROUP: u32 = 64;
pub(crate) const ARGS_BYTES: u64 = 20;
const READ_IDLE: u32 = u32::MAX;

#[repr(C)]
#[derive(Clone, Copy, Debug, Default, PartialEq, Pod, Zeroable)]
pub(crate) struct CandidateGpu {
    pub aabb_min: [f32; 3],
    pub first: u32,
    pub aabb_max: [f32; 3],
    pub count: u32,
    pub chunk: u32,
    pub pad: [u32; 3],
}

#[repr(C)]
#[derive(Clone, Copy, Debug, Default, PartialEq, Pod, Zeroable)]
struct CullParamsGpu {
    view_proj: [[f32; 4]; 4],
    size: [f32; 2],
    mips: u32,
    count: u32,
}

pub(crate) struct OcclusionPipelines {
    init_ms: (wgpu::BindGroupLayout, wgpu::ComputePipeline),
    init: (wgpu::BindGroupLayout, wgpu::ComputePipeline),
    down: (wgpu::BindGroupLayout, wgpu::ComputePipeline),
    cull_bgl: wgpu::BindGroupLayout,
    prepare: wgpu::ComputePipeline,
    test: wgpu::ComputePipeline,
}

fn entry(binding: u32, ty: wgpu::BindingType) -> wgpu::BindGroupLayoutEntry {
    wgpu::BindGroupLayoutEntry { binding, visibility: wgpu::ShaderStages::COMPUTE, ty, count: None }
}

fn storage_tex() -> wgpu::BindingType {
    wgpu::BindingType::StorageTexture {
        access: wgpu::StorageTextureAccess::WriteOnly,
        format: wgpu::TextureFormat::R32Float,
        view_dimension: wgpu::TextureViewDimension::D2,
    }
}

fn float_tex() -> wgpu::BindingType {
    wgpu::BindingType::Texture {
        sample_type: wgpu::TextureSampleType::Float { filterable: false },
        view_dimension: wgpu::TextureViewDimension::D2,
        multisampled: false,
    }
}

fn buffer(ty: wgpu::BufferBindingType) -> wgpu::BindingType {
    wgpu::BindingType::Buffer { ty, has_dynamic_offset: false, min_binding_size: None }
}

impl OcclusionPipelines {
    pub fn new(device: &wgpu::Device) -> Self {
        let module = device.create_shader_module(wgpu::ShaderModuleDescriptor {
            label: Some("hp-render occlusion"),
            source: wgpu::ShaderSource::Wgsl(OCCLUSION_WGSL.into()),
        });
        let make = |label: &str, entries: &[wgpu::BindGroupLayoutEntry], entry_point: &str| {
            let bgl = device.create_bind_group_layout(&wgpu::BindGroupLayoutDescriptor { label: Some(label), entries });
            let pipe = Self::pipeline(device, &module, &bgl, label, entry_point);
            (bgl, pipe)
        };
        let depth = |multisampled: bool| wgpu::BindingType::Texture {
            sample_type: wgpu::TextureSampleType::Depth,
            view_dimension: wgpu::TextureViewDimension::D2,
            multisampled,
        };
        let init_ms = make("hp-render hiz init ms", &[entry(0, depth(true)), entry(1, storage_tex())], "hiz_init_ms");
        let init = make("hp-render hiz init", &[entry(2, depth(false)), entry(1, storage_tex())], "hiz_init");
        let down = make("hp-render hiz down", &[entry(3, float_tex()), entry(1, storage_tex())], "hiz_down");
        let cull_bgl = device.create_bind_group_layout(&wgpu::BindGroupLayoutDescriptor {
            label: Some("hp-render cull"),
            entries: &[
                entry(4, buffer(wgpu::BufferBindingType::Uniform)),
                entry(5, buffer(wgpu::BufferBindingType::Storage { read_only: true })),
                entry(6, buffer(wgpu::BufferBindingType::Storage { read_only: false })),
                entry(7, buffer(wgpu::BufferBindingType::Storage { read_only: false })),
                entry(8, float_tex()),
                entry(9, buffer(wgpu::BufferBindingType::Storage { read_only: false })),
            ],
        });
        let prepare = Self::pipeline(device, &module, &cull_bgl, "hp-render cull prepare", "cull_prepare");
        let test = Self::pipeline(device, &module, &cull_bgl, "hp-render cull test", "cull_test");
        Self { init_ms, init, down, cull_bgl, prepare, test }
    }

    fn pipeline(
        device: &wgpu::Device,
        module: &wgpu::ShaderModule,
        bgl: &wgpu::BindGroupLayout,
        label: &str,
        entry_point: &str,
    ) -> wgpu::ComputePipeline {
        let layout = device.create_pipeline_layout(&wgpu::PipelineLayoutDescriptor {
            label: Some(label),
            bind_group_layouts: &[Some(bgl)],
            immediate_size: 0,
        });
        device.create_compute_pipeline(&wgpu::ComputePipelineDescriptor {
            label: Some(label),
            layout: Some(&layout),
            module,
            entry_point: Some(entry_point),
            compilation_options: wgpu::PipelineCompilationOptions::default(),
            cache: None,
        })
    }
}

pub(crate) struct StatsReadback {
    buffer: wgpu::Buffer,
    state: Arc<AtomicU32>,
    mapping: bool,
}

impl StatsReadback {
    pub fn new(device: &wgpu::Device) -> Self {
        let buffer = device.create_buffer(&wgpu::BufferDescriptor {
            label: Some("hp-render cull stats read"),
            size: 16,
            usage: wgpu::BufferUsages::MAP_READ | wgpu::BufferUsages::COPY_DST,
            mapped_at_creation: false,
        });
        Self { buffer, state: Arc::new(AtomicU32::new(READ_IDLE)), mapping: false }
    }

    pub fn copy_from(&self, enc: &mut wgpu::CommandEncoder, source: &wgpu::Buffer) {
        if !self.mapping {
            enc.copy_buffer_to_buffer(source, 0, &self.buffer, 0, 16);
        }
    }

    pub fn after_submit(&mut self) {
        if self.mapping {
            return;
        }
        self.mapping = true;
        let state = self.state.clone();
        let buffer = self.buffer.clone();
        self.buffer.slice(..).map_async(wgpu::MapMode::Read, move |r| {
            let value = match r {
                Ok(()) => match buffer.slice(..).get_mapped_range() {
                    Ok(m) => {
                        let value = m.get(..4).map(|b| u32::from_le_bytes([b[0], b[1], b[2], b[3]]));
                        drop(m);
                        buffer.unmap();
                        value
                    }
                    Err(_) => None,
                },
                Err(_) => None,
            };
            state.store(value.unwrap_or(0).min(READ_IDLE - 1), Ordering::Release);
        });
    }

    pub fn poll(&mut self) -> Option<u32> {
        if !self.mapping {
            return None;
        }
        let v = self.state.swap(READ_IDLE, Ordering::AcqRel);
        if v == READ_IDLE {
            return None;
        }
        self.mapping = false;
        Some(v)
    }
}

pub(crate) struct OcclusionView {
    target_generation: u64,
    hiz_size: [u32; 2],
    mips: u32,
    _hiz: wgpu::Texture,
    hiz_all: wgpu::TextureView,
    init_bind: wgpu::BindGroup,
    down_binds: Vec<wgpu::BindGroup>,
    params: wgpu::Buffer,
    candidates: GrowBuffer,
    visible: GrowBuffer,
    pub args: GrowBuffer,
    stats: wgpu::Buffer,
    stats_read: StatsReadback,
    pub occluded: u32,
    cull_bind: Option<((u64, u64, u64), wgpu::BindGroup)>,
    buffer_gen: u64,
}

impl OcclusionView {
    pub fn new(
        device: &wgpu::Device,
        pipes: &OcclusionPipelines,
        depth: &wgpu::TextureView,
        size: [u32; 2],
        samples: u32,
        target_generation: u64,
    ) -> Self {
        let hiz_size = [size[0].div_ceil(2).max(1), size[1].div_ceil(2).max(1)];
        let mips = 32 - hiz_size[0].max(hiz_size[1]).leading_zeros();
        let hiz = device.create_texture(&wgpu::TextureDescriptor {
            label: Some("hp-render hiz"),
            size: wgpu::Extent3d { width: hiz_size[0], height: hiz_size[1], depth_or_array_layers: 1 },
            mip_level_count: mips,
            sample_count: 1,
            dimension: wgpu::TextureDimension::D2,
            format: wgpu::TextureFormat::R32Float,
            usage: wgpu::TextureUsages::STORAGE_BINDING | wgpu::TextureUsages::TEXTURE_BINDING,
            view_formats: &[],
        });
        let mip_view = |level: u32| {
            hiz.create_view(&wgpu::TextureViewDescriptor {
                label: Some("hp-render hiz mip"),
                base_mip_level: level,
                mip_level_count: Some(1),
                ..Default::default()
            })
        };
        let views: Vec<wgpu::TextureView> = (0..mips).map(mip_view).collect();
        let hiz_all = hiz.create_view(&wgpu::TextureViewDescriptor::default());
        let (init_bgl, depth_binding) = if samples > 1 { (&pipes.init_ms.0, 0) } else { (&pipes.init.0, 2) };
        let init_bind = device.create_bind_group(&wgpu::BindGroupDescriptor {
            label: Some("hp-render hiz init"),
            layout: init_bgl,
            entries: &[
                wgpu::BindGroupEntry { binding: depth_binding, resource: wgpu::BindingResource::TextureView(depth) },
                wgpu::BindGroupEntry { binding: 1, resource: wgpu::BindingResource::TextureView(&views[0]) },
            ],
        });
        let down_binds = (1..mips as usize)
            .map(|k| {
                device.create_bind_group(&wgpu::BindGroupDescriptor {
                    label: Some("hp-render hiz down"),
                    layout: &pipes.down.0,
                    entries: &[
                        wgpu::BindGroupEntry {
                            binding: 3,
                            resource: wgpu::BindingResource::TextureView(&views[k - 1]),
                        },
                        wgpu::BindGroupEntry { binding: 1, resource: wgpu::BindingResource::TextureView(&views[k]) },
                    ],
                })
            })
            .collect();
        let storage = wgpu::BufferUsages::STORAGE;
        let stats = device.create_buffer(&wgpu::BufferDescriptor {
            label: Some("hp-render cull stats"),
            size: 16,
            usage: storage | wgpu::BufferUsages::COPY_SRC | wgpu::BufferUsages::COPY_DST,
            mapped_at_creation: false,
        });
        let stats_read = StatsReadback::new(device);
        Self {
            target_generation,
            hiz_size,
            mips,
            _hiz: hiz,
            hiz_all,
            init_bind,
            down_binds,
            params: device.create_buffer(&wgpu::BufferDescriptor {
                label: Some("hp-render cull params"),
                size: size_of::<CullParamsGpu>() as u64,
                usage: wgpu::BufferUsages::UNIFORM | wgpu::BufferUsages::COPY_DST,
                mapped_at_creation: false,
            }),
            candidates: GrowBuffer::new(device, "hp-render cull candidates", storage, 1 << 12),
            visible: GrowBuffer::new(device, "hp-render chunk visibility", storage, 1 << 10),
            args: GrowBuffer::new(device, "hp-render cull args", storage | wgpu::BufferUsages::INDIRECT, 1 << 12),
            stats,
            stats_read,
            occluded: 0,
            cull_bind: None,
            buffer_gen: 1,
        }
    }

    pub fn matches(&self, target_generation: u64) -> bool {
        self.target_generation == target_generation
    }

    #[allow(clippy::too_many_arguments)]
    pub fn prepare(
        &mut self,
        device: &wgpu::Device,
        queue: &wgpu::Queue,
        pipes: &OcclusionPipelines,
        enc: &mut wgpu::CommandEncoder,
        view_proj: [[f32; 4]; 4],
        candidates: &[CandidateGpu],
        chunk_slots: usize,
        max_storage: u64,
    ) -> bool {
        let _ = device.poll(wgpu::PollType::Poll);
        self.poll_stats();
        let n = candidates.len() as u64;
        let needs = [
            (&mut self.candidates, n * size_of::<CandidateGpu>() as u64, false),
            (&mut self.args, n * ARGS_BYTES, false),
            (&mut self.visible, chunk_slots as u64 * 4, true),
        ];
        for (buf, bytes, keep) in needs {
            match buf.ensure(device, queue, bytes.max(16), max_storage, keep) {
                Grow::Grown => self.buffer_gen += 1,
                Grow::TooLarge => return false,
                Grow::Fits => {}
            }
        }
        if candidates.is_empty() {
            return true;
        }
        queue.write_buffer(&self.candidates.buffer, 0, bytemuck::cast_slice(candidates));
        let params = CullParamsGpu {
            view_proj,
            size: [self.hiz_size[0] as f32, self.hiz_size[1] as f32],
            mips: self.mips,
            count: candidates.len() as u32,
        };
        queue.write_buffer(&self.params, 0, bytemuck::bytes_of(&params));
        queue.write_buffer(&self.stats, 0, &[0u8; 16]);
        let bind = self.cull_bind(device, pipes);
        let mut pass = enc.begin_compute_pass(&wgpu::ComputePassDescriptor {
            label: Some("hp-render cull prepare"),
            timestamp_writes: None,
        });
        pass.set_pipeline(&pipes.prepare);
        pass.set_bind_group(0, &bind, &[]);
        pass.dispatch_workgroups((candidates.len() as u32).div_ceil(GROUP), 1, 1);
        true
    }

    fn cull_bind(&mut self, device: &wgpu::Device, pipes: &OcclusionPipelines) -> wgpu::BindGroup {
        let key = (self.buffer_gen, self.target_generation, 0);
        if let Some((k, bg)) = &self.cull_bind
            && *k == key
        {
            return bg.clone();
        }
        let bg = device.create_bind_group(&wgpu::BindGroupDescriptor {
            label: Some("hp-render cull"),
            layout: &pipes.cull_bgl,
            entries: &[
                wgpu::BindGroupEntry { binding: 4, resource: self.params.as_entire_binding() },
                wgpu::BindGroupEntry { binding: 5, resource: self.candidates.buffer.as_entire_binding() },
                wgpu::BindGroupEntry { binding: 6, resource: self.visible.buffer.as_entire_binding() },
                wgpu::BindGroupEntry { binding: 7, resource: self.args.buffer.as_entire_binding() },
                wgpu::BindGroupEntry { binding: 8, resource: wgpu::BindingResource::TextureView(&self.hiz_all) },
                wgpu::BindGroupEntry { binding: 9, resource: self.stats.as_entire_binding() },
            ],
        });
        self.cull_bind = Some((key, bg.clone()));
        bg
    }

    pub fn cull(
        &mut self,
        device: &wgpu::Device,
        pipes: &OcclusionPipelines,
        enc: &mut wgpu::CommandEncoder,
        samples: u32,
        count: u32,
    ) {
        if count == 0 {
            return;
        }
        let bind = self.cull_bind(device, pipes);
        {
            let mut pass = enc.begin_compute_pass(&wgpu::ComputePassDescriptor {
                label: Some("hp-render hiz"),
                timestamp_writes: None,
            });
            let init = if samples > 1 { &pipes.init_ms.1 } else { &pipes.init.1 };
            pass.set_pipeline(init);
            pass.set_bind_group(0, &self.init_bind, &[]);
            pass.dispatch_workgroups(self.hiz_size[0].div_ceil(TILE), self.hiz_size[1].div_ceil(TILE), 1);
            pass.set_pipeline(&pipes.down.1);
            for (k, bg) in self.down_binds.iter().enumerate() {
                let w = (self.hiz_size[0] >> (k + 1)).max(1);
                let h = (self.hiz_size[1] >> (k + 1)).max(1);
                pass.set_bind_group(0, bg, &[]);
                pass.dispatch_workgroups(w.div_ceil(TILE), h.div_ceil(TILE), 1);
            }
            pass.set_pipeline(&pipes.test);
            pass.set_bind_group(0, &bind, &[]);
            pass.dispatch_workgroups(count.div_ceil(GROUP), 1, 1);
        }
        self.stats_read.copy_from(enc, &self.stats);
    }

    pub fn after_submit(&mut self) {
        self.stats_read.after_submit();
    }

    fn poll_stats(&mut self) {
        if let Some(v) = self.stats_read.poll() {
            self.occluded = v;
        }
    }
}

pub(crate) const fn supported() -> bool {
    matches!(DEPTH_FORMAT, wgpu::TextureFormat::Depth32Float)
}

#[cfg(test)]
mod tests {
    use super::*;

    fn gpu() -> Option<(wgpu::Device, wgpu::Queue)> {
        let instance = wgpu::Instance::new(wgpu::InstanceDescriptor::new_without_display_handle());
        let adapter = pollster::block_on(instance.request_adapter(&wgpu::RequestAdapterOptions::default())).ok()?;
        pollster::block_on(adapter.request_device(&wgpu::DeviceDescriptor::default())).ok()
    }

    fn errors(device: &wgpu::Device) -> Arc<std::sync::Mutex<Vec<String>>> {
        let seen = Arc::new(std::sync::Mutex::new(Vec::new()));
        let sink = seen.clone();
        device.on_uncaptured_error(Arc::new(move |e: wgpu::Error| {
            if let Ok(mut v) = sink.lock() {
                v.push(e.to_string());
            }
        }));
        seen
    }

    fn source(device: &wgpu::Device, queue: &wgpu::Queue, value: u32) -> wgpu::Buffer {
        let b = device.create_buffer(&wgpu::BufferDescriptor {
            label: Some("stats source"),
            size: 16,
            usage: wgpu::BufferUsages::COPY_SRC | wgpu::BufferUsages::COPY_DST,
            mapped_at_creation: false,
        });
        queue.write_buffer(&b, 0, bytemuck::cast_slice(&[value, 0, 0, 0]));
        b
    }

    fn round(device: &wgpu::Device, queue: &wgpu::Queue, r: &mut StatsReadback, src: &wgpu::Buffer) {
        let mut enc = device.create_command_encoder(&wgpu::CommandEncoderDescriptor::default());
        r.copy_from(&mut enc, src);
        queue.submit([enc.finish()]);
        r.after_submit();
    }

    fn wait(device: &wgpu::Device) {
        let _ = device.poll(wgpu::PollType::wait_indefinitely());
    }

    #[test]
    fn stats_are_read_back_and_owners_can_go_away_any_time() {
        let _serial = crate::memory::gpu_test_lock();
        let Some((device, queue)) = gpu() else { return };
        let seen = errors(&device);
        let src = source(&device, &queue, 42);
        let mut r = StatsReadback::new(&device);
        round(&device, &queue, &mut r, &src);
        wait(&device);
        assert_eq!(r.poll(), Some(42));
        assert_eq!(r.poll(), None, "read once");
        queue.write_buffer(&src, 0, bytemuck::cast_slice(&[7u32, 0, 0, 0]));
        round(&device, &queue, &mut r, &src);
        wait(&device);
        assert_eq!(r.poll(), Some(7), "the buffer is mapped again after a read");
        for _ in 0..8 {
            let mut gone = StatsReadback::new(&device);
            round(&device, &queue, &mut gone, &src);
            drop(gone);
        }
        let mut late = StatsReadback::new(&device);
        round(&device, &queue, &mut late, &src);
        wait(&device);
        drop(late);
        wait(&device);
        let seen = seen.lock().unwrap_or_else(std::sync::PoisonError::into_inner);
        assert!(seen.is_empty(), "{seen:?}");
    }

    #[test]
    fn a_lost_device_does_not_unmap_destroyed_stats() {
        let _serial = crate::memory::gpu_test_lock();
        let Some((device, queue)) = gpu() else { return };
        let seen = errors(&device);
        let src = source(&device, &queue, 3);
        let mut r = StatsReadback::new(&device);
        round(&device, &queue, &mut r, &src);
        device.destroy();
        wait(&device);
        assert!(r.poll().is_some(), "a failed map still ends");
        assert!(r.poll().is_none());
        drop(r);
        wait(&device);
        let seen = seen.lock().unwrap_or_else(std::sync::PoisonError::into_inner);
        assert!(!seen.iter().any(|e| e.contains("unmap")), "{seen:?}");
    }

    #[test]
    fn layouts_match_the_shader() {
        assert_eq!(size_of::<CandidateGpu>(), 48);
        assert_eq!(size_of::<CullParamsGpu>(), 80);
        assert!(supported());
        assert!(!OCCLUSION_WGSL.contains('\u{2014}') && !OCCLUSION_WGSL.contains('\u{2013}'));
    }
}
