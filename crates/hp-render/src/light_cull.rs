use std::sync::Arc;
use std::sync::atomic::{AtomicU32, Ordering};

use bytemuck::{Pod, Zeroable};
use glam::Vec3;

use crate::arena::{Grow, GrowBuffer};
use crate::camera::Camera;
use crate::gpu::LightGpu;
use crate::lights::{ClusterGrid, MAX_PER_CLUSTER, TILE_PX, luminance};

const LIGHT_CULL_WGSL: &str = include_str!("shaders/light_cull.wgsl");
pub(crate) const GROUP_X: u32 = 8;
pub(crate) const GROUP_Y: u32 = 8;
const IDLE: u32 = 0;
const PENDING: u32 = 1;
const READY: u32 = 2;
const FAILED: u32 = 3;
const CROWDED_GROUPS: u32 = 512;

#[repr(C)]
#[derive(Clone, Copy, Debug, Default, PartialEq, Pod, Zeroable)]
pub(crate) struct CandidateGpu {
    pub view: [f32; 4],
    pub world: [f32; 4],
    pub atten: [f32; 4],
    pub info: [u32; 4],
}

#[repr(C)]
#[derive(Clone, Copy, Debug, Default, PartialEq, Pod, Zeroable)]
pub(crate) struct CullParamsGpu {
    pub eye: [f32; 4],
    pub right: [f32; 4],
    pub up: [f32; 4],
    pub forward: [f32; 4],
    pub tan: [f32; 4],
    pub dims: [u32; 4],
    pub misc: [u32; 4],
}

pub(crate) struct LightCullPipeline {
    bgl: wgpu::BindGroupLayout,
    pipeline: wgpu::ComputePipeline,
    crowded: wgpu::ComputePipeline,
}

fn entry(binding: u32, ty: wgpu::BufferBindingType) -> wgpu::BindGroupLayoutEntry {
    wgpu::BindGroupLayoutEntry {
        binding,
        visibility: wgpu::ShaderStages::COMPUTE,
        ty: wgpu::BindingType::Buffer { ty, has_dynamic_offset: false, min_binding_size: None },
        count: None,
    }
}

impl LightCullPipeline {
    pub fn new(device: &wgpu::Device) -> Self {
        let rw = wgpu::BufferBindingType::Storage { read_only: false };
        let bgl = device.create_bind_group_layout(&wgpu::BindGroupLayoutDescriptor {
            label: Some("hp-render light cull"),
            entries: &[
                entry(0, wgpu::BufferBindingType::Uniform),
                entry(1, wgpu::BufferBindingType::Storage { read_only: true }),
                entry(2, wgpu::BufferBindingType::Storage { read_only: true }),
                entry(3, rw),
                entry(4, rw),
                entry(5, rw),
                entry(6, rw),
            ],
        });
        let module = device.create_shader_module(wgpu::ShaderModuleDescriptor {
            label: Some("hp-render light cull"),
            source: wgpu::ShaderSource::Wgsl(LIGHT_CULL_WGSL.into()),
        });
        let layout = device.create_pipeline_layout(&wgpu::PipelineLayoutDescriptor {
            label: Some("hp-render light cull"),
            bind_group_layouts: &[Some(&bgl)],
            immediate_size: 0,
        });
        let compute = |entry: &str| {
            crate::pipeline_cache::with(device, |cache| {
                device.create_compute_pipeline(&wgpu::ComputePipelineDescriptor {
                    label: Some("hp-render light cull"),
                    layout: Some(&layout),
                    module: &module,
                    entry_point: Some(entry),
                    compilation_options: wgpu::PipelineCompilationOptions::default(),
                    cache,
                })
            })
        };
        let pipeline = compute("cs_cull");
        let crowded = compute("cs_crowded");
        Self { bgl, pipeline, crowded }
    }
}

pub(crate) fn supported(device: &wgpu::Device) -> bool {
    let l = device.limits();
    l.max_compute_invocations_per_workgroup >= GROUP_X * GROUP_Y
        && l.max_compute_workgroup_size_x >= GROUP_X
        && l.max_compute_workgroup_size_y >= GROUP_Y
        && l.max_storage_buffers_per_shader_stage >= 6
        && l.max_compute_workgroup_storage_size >= 8192
}

pub(crate) struct Prepared {
    pub params: CullParamsGpu,
    pub candidates: Vec<CandidateGpu>,
    pub slices: Vec<u32>,
}

pub(crate) fn cull_radius(l: &LightGpu) -> f32 {
    let range = l.pos_range[3];
    if l.cone[3] > l.cone[2] && l.cone[3].is_finite() { range.max(l.cone[3]) } else { range }
}

pub(crate) fn prepare(
    cam: &Camera,
    grid: &ClusterGrid,
    lights: &[LightGpu],
    first_local: u32,
    delta_only: bool,
    cap: u32,
) -> Prepared {
    let mut by_slice: Vec<Vec<u32>> = vec![Vec::new(); grid.slices as usize];
    let mut candidates: Vec<CandidateGpu> = Vec::new();
    for (i, l) in lights.iter().enumerate().skip(first_local as usize) {
        if delta_only {
            if !crate::live_lighting::has_delta(l) {
                continue;
            }
        } else if l.delta_color[3] > 0.5 {
            continue;
        }
        let p = Vec3::new(l.pos_range[0], l.pos_range[1], l.pos_range[2]);
        let r = cull_radius(l);
        if !(p.is_finite() && r.is_finite()) {
            continue;
        }
        let d = p - cam.eye;
        let (vx, vy, vz) = (d.dot(cam.right), d.dot(cam.up), d.dot(cam.forward));
        if vz + r < grid.near || vz - r > cam.far {
            continue;
        }
        let color = if delta_only { l.delta_color.map(f32::abs) } else { l.color };
        let lum = luminance(color);
        let c = candidates.len() as u32;
        candidates.push(CandidateGpu {
            view: [vx, vy, vz, r],
            world: [p.x, p.y, p.z, l.pos_range[3]],
            atten: l.atten,
            info: [i as u32, if lum.is_finite() { lum.max(0.0) } else { 0.0 }.to_bits(), 0, 0],
        });
        let pad = r + r.abs() * 1e-5 + 0.01;
        let (s0, s1) = (grid.slice_of((vz - pad) / 1.01), grid.slice_of((vz + pad) * 1.01));
        for list in &mut by_slice[s0 as usize..=s1 as usize] {
            list.push(c);
        }
    }
    let mut slices: Vec<u32> =
        Vec::with_capacity(grid.slices as usize + 1 + by_slice.iter().map(Vec::len).sum::<usize>());
    let mut at = grid.slices + 1;
    for list in &by_slice {
        slices.push(at);
        at += list.len() as u32;
    }
    slices.push(at);
    for list in by_slice {
        slices.extend(list);
    }
    let aspect = cam.width / cam.height.max(1.0);
    let params = CullParamsGpu {
        eye: [cam.eye.x, cam.eye.y, cam.eye.z, grid.near],
        right: [cam.right.x, cam.right.y, cam.right.z, grid.scale],
        up: [cam.up.x, cam.up.y, cam.up.z, cam.width],
        forward: [cam.forward.x, cam.forward.y, cam.forward.z, cam.height],
        tan: [cam.tan_half_fov * aspect, cam.tan_half_fov, TILE_PX as f32, cam.far * 1.001],
        dims: [grid.tiles_x, grid.tiles_y, grid.slices, candidates.len() as u32],
        misc: [if cap == 0 { MAX_PER_CLUSTER } else { cap.min(MAX_PER_CLUSTER) }, 0, 0, 0],
    };
    Prepared { params, candidates, slices }
}

pub(crate) fn index_bytes(grid: &ClusterGrid) -> u64 {
    grid.count() as u64 * u64::from(MAX_PER_CLUSTER) * 2
}

pub(crate) struct CullView {
    params: wgpu::Buffer,
    candidates: GrowBuffer,
    slices: GrowBuffer,
    crowded_list: GrowBuffer,
    stats: wgpu::Buffer,
    readback: wgpu::Buffer,
    state: Arc<AtomicU32>,
    bind_group: Option<(u64, wgpu::BindGroup)>,
    generation: u64,
    pending: Option<[u32; 3]>,
    pub crowded: Option<u32>,
}

impl CullView {
    pub fn new(device: &wgpu::Device) -> Self {
        let storage = wgpu::BufferUsages::STORAGE;
        Self {
            params: device.create_buffer(&wgpu::BufferDescriptor {
                label: Some("hp-render light cull params"),
                size: size_of::<CullParamsGpu>() as u64,
                usage: wgpu::BufferUsages::UNIFORM | wgpu::BufferUsages::COPY_DST,
                mapped_at_creation: false,
            }),
            candidates: GrowBuffer::new(device, "hp-render light cull candidates", storage, 64),
            slices: GrowBuffer::new(device, "hp-render light cull slices", storage, 64),
            crowded_list: GrowBuffer::new(device, "hp-render light cull crowded clusters", storage, 64),
            stats: device.create_buffer(&wgpu::BufferDescriptor {
                label: Some("hp-render light cull stats"),
                size: 16,
                usage: storage | wgpu::BufferUsages::COPY_DST | wgpu::BufferUsages::COPY_SRC,
                mapped_at_creation: false,
            }),
            readback: device.create_buffer(&wgpu::BufferDescriptor {
                label: Some("hp-render light cull readback"),
                size: 16,
                usage: wgpu::BufferUsages::MAP_READ | wgpu::BufferUsages::COPY_DST,
                mapped_at_creation: false,
            }),
            state: Arc::new(AtomicU32::new(IDLE)),
            bind_group: None,
            generation: 0,
            pending: None,
            crowded: None,
        }
    }

    pub fn bytes(&self) -> u64 {
        self.candidates.size + self.slices.size + self.crowded_list.size + 64
    }

    pub fn upload(&mut self, device: &wgpu::Device, queue: &wgpu::Queue, prepared: &Prepared, max: u64) -> bool {
        let cand: &[u8] = bytemuck::cast_slice(&prepared.candidates);
        let slices: &[u8] = bytemuck::cast_slice(&prepared.slices);
        let a = self.candidates.ensure(device, queue, cand.len() as u64, max, false);
        let b = self.slices.ensure(device, queue, slices.len() as u64, max, false);
        let dims = prepared.params.dims;
        let clusters = u64::from(dims[0]) * u64::from(dims[1]) * u64::from(dims[2]);
        let c = self.crowded_list.ensure(device, queue, clusters * 8, max, false);
        if a == Grow::TooLarge || b == Grow::TooLarge || c == Grow::TooLarge {
            return false;
        }
        if a == Grow::Grown || b == Grow::Grown || c == Grow::Grown {
            self.generation += 1;
        }
        queue.write_buffer(&self.params, 0, bytemuck::bytes_of(&prepared.params));
        if !cand.is_empty() {
            queue.write_buffer(&self.candidates.buffer, 0, cand);
        }
        queue.write_buffer(&self.slices.buffer, 0, slices);
        let groups = [
            prepared.params.dims[0].div_ceil(GROUP_X),
            prepared.params.dims[1].div_ceil(GROUP_Y),
            prepared.params.dims[2],
        ];
        self.pending = Some(groups);
        true
    }

    pub fn poll(&mut self) -> Option<u32> {
        match self.state.load(Ordering::Acquire) {
            READY => {
                let value = self.readback.slice(..).get_mapped_range().ok().map(|m| {
                    let mut b = [0u8; 4];
                    b.copy_from_slice(&m[0..4]);
                    u32::from_le_bytes(b)
                });
                self.readback.unmap();
                self.state.store(IDLE, Ordering::Release);
                if value.is_some() {
                    self.crowded = value;
                }
                value
            }
            FAILED => {
                self.readback.unmap();
                self.state.store(IDLE, Ordering::Release);
                None
            }
            _ => None,
        }
    }

    pub fn has_pending(&self) -> bool {
        self.pending.is_some()
    }

    pub fn reading(&self) -> bool {
        self.state.load(Ordering::Acquire) == PENDING
    }

    #[allow(clippy::too_many_arguments)]
    pub fn record(
        &mut self,
        device: &wgpu::Device,
        queue: &wgpu::Queue,
        pipe: &LightCullPipeline,
        enc: &mut wgpu::CommandEncoder,
        clusters: &wgpu::Buffer,
        indices: &wgpu::Buffer,
        key: u64,
    ) -> bool {
        let Some(groups) = self.pending.take() else { return false };
        let key = key ^ self.generation.rotate_left(32);
        if self.bind_group.as_ref().is_none_or(|(k, _)| *k != key) {
            let bg = device.create_bind_group(&wgpu::BindGroupDescriptor {
                label: Some("hp-render light cull"),
                layout: &pipe.bgl,
                entries: &[
                    wgpu::BindGroupEntry { binding: 0, resource: self.params.as_entire_binding() },
                    wgpu::BindGroupEntry { binding: 1, resource: self.candidates.buffer.as_entire_binding() },
                    wgpu::BindGroupEntry { binding: 2, resource: self.slices.buffer.as_entire_binding() },
                    wgpu::BindGroupEntry { binding: 3, resource: clusters.as_entire_binding() },
                    wgpu::BindGroupEntry { binding: 4, resource: indices.as_entire_binding() },
                    wgpu::BindGroupEntry { binding: 5, resource: self.stats.as_entire_binding() },
                    wgpu::BindGroupEntry { binding: 6, resource: self.crowded_list.buffer.as_entire_binding() },
                ],
            });
            self.bind_group = Some((key, bg));
        }
        let Some((_, bg)) = self.bind_group.as_ref() else { return false };
        let read = self.state.load(Ordering::Acquire) == IDLE;
        queue.write_buffer(&self.stats, 0, &[0u8; 16]);
        {
            let mut pass = enc.begin_compute_pass(&wgpu::ComputePassDescriptor {
                label: Some("hp-render light cull"),
                timestamp_writes: None,
            });
            pass.set_pipeline(&pipe.pipeline);
            pass.set_bind_group(0, bg, &[]);
            pass.dispatch_workgroups(groups[0], groups[1], groups[2]);
        }
        {
            let mut pass = enc.begin_compute_pass(&wgpu::ComputePassDescriptor {
                label: Some("hp-render light cull crowded clusters"),
                timestamp_writes: None,
            });
            pass.set_pipeline(&pipe.crowded);
            pass.set_bind_group(0, bg, &[]);
            let clusters = groups[0] * GROUP_X * groups[1] * GROUP_Y * groups[2];
            pass.dispatch_workgroups(clusters.clamp(1, CROWDED_GROUPS), 1, 1);
        }
        if read {
            enc.copy_buffer_to_buffer(&self.stats, 0, &self.readback, 0, 16);
        }
        read
    }

    pub fn after_submit(&self) {
        if self.state.compare_exchange(IDLE, PENDING, Ordering::AcqRel, Ordering::Acquire).is_err() {
            return;
        }
        let state = self.state.clone();
        self.readback.slice(..).map_async(wgpu::MapMode::Read, move |r| {
            state.store(if r.is_ok() { READY } else { FAILED }, Ordering::Release);
        });
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::lights::{ClusterBounds, build_clusters, light_score, pack_lights};
    use crate::types::{LightKind, PreviewLight, RenderMode, ViewKind, ViewParams};

    fn gpu() -> Option<(wgpu::Device, wgpu::Queue)> {
        let instance = wgpu::Instance::new(wgpu::InstanceDescriptor::new_without_display_handle());
        let adapter = pollster::block_on(instance.request_adapter(&wgpu::RequestAdapterOptions::default())).ok()?;
        pollster::block_on(adapter.request_device(&wgpu::DeviceDescriptor::default())).ok()
    }

    fn cam() -> Camera {
        Camera::new(&ViewParams {
            game_pixel_grid: false,
            size: [700, 450],
            kind: ViewKind::Perspective {
                eye: [10.0, -20.0, 30.0],
                forward: [1.0, 0.2, -0.1],
                up: [0.0, 0.0, 1.0],
                fov_y_deg: 75.0,
                near: 1.0,
                far: 20000.0,
            },
            mode: RenderMode::Lit,
            grid: None,
            background: [0.0; 3],
            selection_color: [1.0; 4],
            show_sprites: false,
            show_models: false,
            msaa: false,
        })
    }

    fn lights() -> Vec<PreviewLight> {
        let mut out = Vec::new();
        let mut seed = 99u32;
        let mut next = || {
            seed = seed.wrapping_mul(1664525).wrapping_add(1013904223);
            (seed >> 8) as f32 / 16_777_216.0
        };
        for _ in 0..500 {
            let pos = [next() * 7000.0 - 800.0, next() * 5000.0 - 2500.0, next() * 1200.0 - 400.0];
            let kind = if next() < 0.3 {
                LightKind::Spot { dir: [0.0, 0.0, -1.0], inner_deg: 20.0, outer_deg: 40.0 }
            } else {
                LightKind::Point
            };
            out.push(PreviewLight {
                kind,
                pos,
                color: [1.0, 0.8, 0.6],
                intensity: 0.5 + next(),
                range: 40.0 + next() * 3000.0,
                falloff: None,
            });
        }
        for k in 0..170 {
            let pos = [900.0 + (k % 13) as f32 * 7.0, 40.0 + (k / 13) as f32 * 9.0, 20.0];
            out.push(PreviewLight {
                kind: LightKind::Point,
                pos,
                color: [1.0; 3],
                intensity: 0.2 + k as f32 * 0.01,
                range: 600.0,
                falloff: None,
            });
        }
        out.push(PreviewLight {
            kind: LightKind::Directional { dir: [0.2, 0.1, -1.0] },
            pos: [0.0; 3],
            color: [1.0; 3],
            intensity: 1.0,
            range: 0.0,
            falloff: None,
        });
        out
    }

    fn read(device: &wgpu::Device, queue: &wgpu::Queue, src: &wgpu::Buffer, size: u64) -> Vec<u32> {
        let dst = device.create_buffer(&wgpu::BufferDescriptor {
            label: None,
            size,
            usage: wgpu::BufferUsages::MAP_READ | wgpu::BufferUsages::COPY_DST,
            mapped_at_creation: false,
        });
        let mut enc = device.create_command_encoder(&wgpu::CommandEncoderDescriptor::default());
        enc.copy_buffer_to_buffer(src, 0, &dst, 0, size);
        queue.submit([enc.finish()]);
        dst.slice(..).map_async(wgpu::MapMode::Read, |_| {});
        let _ = device.poll(wgpu::PollType::wait_indefinitely());
        let data = dst.slice(..).get_mapped_range().expect("mapped");
        bytemuck::cast_slice(&data).to_vec()
    }

    type GpuClusters = (Vec<[u32; 2]>, Vec<u32>, u32);

    fn run_gpu(
        device: &wgpu::Device,
        queue: &wgpu::Queue,
        cam: &Camera,
        packed: &crate::lights::Packed,
        delta: bool,
    ) -> GpuClusters {
        run_gpu_capped(device, queue, cam, packed, delta, 0)
    }

    fn run_gpu_capped(
        device: &wgpu::Device,
        queue: &wgpu::Queue,
        cam: &Camera,
        packed: &crate::lights::Packed,
        delta: bool,
        cap: u32,
    ) -> GpuClusters {
        let grid = ClusterGrid::new(cam);
        let prepared = prepare(cam, &grid, &packed.lights, packed.n_dir, delta, cap);
        let storage = wgpu::BufferUsages::STORAGE;
        let table = GrowBuffer::new(device, "t", storage, grid.count() as u64 * 8);
        let list = GrowBuffer::new(device, "l", storage, index_bytes(&grid));
        let pipe = LightCullPipeline::new(device);
        let mut view = CullView::new(device);
        assert!(view.upload(device, queue, &prepared, 1 << 30));
        let mut enc = device.create_command_encoder(&wgpu::CommandEncoderDescriptor::default());
        assert!(view.record(device, queue, &pipe, &mut enc, &table.buffer, &list.buffer, 1));
        queue.submit([enc.finish()]);
        view.after_submit();
        let _ = device.poll(wgpu::PollType::wait_indefinitely());
        let crowded = view.poll().expect("crowded count read back");
        let t = read(device, queue, &table.buffer, grid.count() as u64 * 8);
        let l = read(device, queue, &list.buffer, index_bytes(&grid));
        (t.chunks(2).map(|c| [c[0], c[1]]).collect(), l, crowded)
    }

    fn unpack(list: &[u32], at: [u32; 2]) -> Vec<u32> {
        (at[0]..at[0] + at[1]).map(|i| (list[(i >> 1) as usize] >> ((i & 1) * 16)) & 0xffff).collect()
    }

    #[test]
    fn gpu_clusters_hold_every_light_the_cpu_finds_and_only_lights_that_reach() {
        let _serial = crate::memory::gpu_test_lock();
        let Some((device, queue)) = gpu() else { return };
        if !supported(&device) {
            return;
        }
        let c = cam();
        let grid = ClusterGrid::new(&c);
        let packed = pack_lights(&lights());
        let cpu = build_clusters(&c, &grid, &packed.lights, packed.n_dir, false);
        let (table, list, crowded) = run_gpu(&device, &queue, &c, &packed, false);
        assert!(crowded > 0, "the packed lights crowd some clusters");
        let plane = grid.tiles_x * grid.tiles_y;
        let mut compared = 0;
        let mut crowded_seen = 0;
        let (mut gpu_refs, mut cpu_refs) = (0u64, 0u64);
        let view_of = |i: u32| {
            let l = &packed.lights[i as usize];
            let d = Vec3::new(l.pos_range[0], l.pos_range[1], l.pos_range[2]) - c.eye;
            (Vec3::new(d.dot(c.right), d.dot(c.up), d.dot(c.forward)), cull_radius(l))
        };
        for (ci, at) in table.iter().enumerate() {
            let ci = ci as u32;
            let (s, rest) = (ci / plane, ci % plane);
            let (x, y) = (rest % grid.tiles_x, rest / grid.tiles_x);
            let bounds = ClusterBounds::new(&c, &grid, x, y, s);
            let gpu_set = unpack(&list, *at);
            for &i in &gpu_set {
                let (p, r) = view_of(i);
                assert!(bounds.intersects(p, r * 1.01 + 1.0), "cluster {ci} holds light {i} that does not reach it");
            }
            let mut sorted = gpu_set.clone();
            sorted.sort_unstable();
            sorted.dedup();
            assert_eq!(sorted.len(), gpu_set.len(), "no light twice in cluster {ci}");
            if at[1] >= MAX_PER_CLUSTER {
                crowded_seen += 1;
                let center = grid.center(&c, x, y, s);
                let kept = gpu_set
                    .iter()
                    .map(|&i| light_score(&packed.lights[i as usize], center))
                    .fold(f32::INFINITY, f32::min);
                for (i, l) in packed.lights.iter().enumerate().skip(packed.n_dir as usize) {
                    let (p, r) = view_of(i as u32);
                    if gpu_set.contains(&(i as u32)) || !bounds.intersects(p, r * 0.99) {
                        continue;
                    }
                    assert!(light_score(l, center) <= kept * 1.001 + 1e-12, "a stronger light was left out of {ci}");
                }
                continue;
            }
            let cpu_at = cpu.table[ci as usize];
            gpu_refs += u64::from(at[1]);
            cpu_refs += u64::from(cpu_at[1].min(MAX_PER_CLUSTER));
            compared += 1;
        }
        let mut seed = 4242u32;
        let mut next = || {
            seed = seed.wrapping_mul(1664525).wrapping_add(1013904223);
            (seed >> 8) as f32 / 16_777_216.0
        };
        let mut checked = 0;
        for (i, l) in packed.lights.iter().enumerate().skip(packed.n_dir as usize) {
            let center = Vec3::new(l.pos_range[0], l.pos_range[1], l.pos_range[2]);
            for _ in 0..200 {
                let dir = Vec3::new(next() * 2.0 - 1.0, next() * 2.0 - 1.0, next() * 2.0 - 1.0).normalize_or_zero();
                let p = center + dir * (next() * l.pos_range[3] * 0.999);
                let clip = c.view_proj * p.extend(1.0);
                let depth = (p - c.eye).dot(c.forward);
                if clip.w <= 1e-3 || depth <= grid.near || depth >= grid.far {
                    continue;
                }
                let (nx, ny) = (clip.x / clip.w, clip.y / clip.w);
                if nx.abs() >= 1.0 || ny.abs() >= 1.0 {
                    continue;
                }
                let px = (nx + 1.0) * 0.5 * c.width;
                let py = (1.0 - ny) * 0.5 * c.height;
                let (x, y) = ((px as u32 / TILE_PX).min(grid.tiles_x - 1), (py as u32 / TILE_PX).min(grid.tiles_y - 1));
                let at = table[((grid.slice_of(depth) * grid.tiles_y + y) * grid.tiles_x + x) as usize];
                if at[1] >= MAX_PER_CLUSTER {
                    continue;
                }
                assert!(
                    unpack(&list, at).contains(&(i as u32)),
                    "light {i} misses a point {depth} units deep it lights"
                );
                checked += 1;
            }
        }
        assert!(checked > 5000, "{checked} points checked");
        assert!(gpu_refs <= cpu_refs, "the GPU lists are not longer than the CPU ones: {gpu_refs} > {cpu_refs}");
        assert!(compared > 1000 && crowded_seen > 0, "{compared} clusters compared, {crowded_seen} crowded");
        let again = run_gpu(&device, &queue, &c, &packed, false);
        assert_eq!((again.0, again.1), (table, list), "the GPU build is deterministic");
    }

    #[test]
    fn a_light_cap_keeps_the_strongest_lights_of_each_cluster() {
        let _serial = crate::memory::gpu_test_lock();
        let Some((device, queue)) = gpu() else { return };
        if !supported(&device) {
            return;
        }
        let c = cam();
        let grid = ClusterGrid::new(&c);
        let packed = pack_lights(&lights());
        let (full_table, full_list, _) = run_gpu(&device, &queue, &c, &packed, false);
        let (table, list, _) = run_gpu_capped(&device, &queue, &c, &packed, false, 32);
        let plane = grid.tiles_x * grid.tiles_y;
        let mut capped = 0;
        for (ci, (at, full)) in table.iter().zip(&full_table).enumerate() {
            let kept = unpack(&list, *at);
            let all = unpack(&full_list, *full);
            assert!(kept.len() <= 32, "cluster {ci} keeps {} lights", kept.len());
            if all.len() <= 32 {
                assert_eq!(kept, all, "cluster {ci} under the cap keeps every light in order");
                continue;
            }
            capped += 1;
            assert_eq!(kept.len(), 32);
            let ci = ci as u32;
            let (s, rest) = (ci / plane, ci % plane);
            let center = grid.center(&c, rest % grid.tiles_x, rest / grid.tiles_x, s);
            let weakest_kept =
                kept.iter().map(|&i| light_score(&packed.lights[i as usize], center)).fold(f32::INFINITY, f32::min);
            for i in all.iter().filter(|i| !kept.contains(i)) {
                assert!(
                    light_score(&packed.lights[*i as usize], center) <= weakest_kept * 1.001 + 1e-12,
                    "a stronger light was left out of cluster {ci}"
                );
            }
        }
        assert!(capped > 10, "{capped} clusters reached the cap");
    }

    #[test]
    fn delta_lights_alone_reach_the_delta_clusters() {
        let _serial = crate::memory::gpu_test_lock();
        let Some((device, queue)) = gpu() else { return };
        if !supported(&device) {
            return;
        }
        let c = cam();
        let mut packed = pack_lights(&lights()[..40]);
        let first = packed.n_dir as usize;
        for (k, l) in packed.lights.iter_mut().enumerate().skip(first) {
            if k % 3 == 0 {
                l.delta_color = [0.5, 0.0, 0.0, 0.0];
            }
        }
        let (table, list, _) = run_gpu(&device, &queue, &c, &packed, true);
        let mut seen = 0;
        for at in &table {
            for i in unpack(&list, *at) {
                assert!(i % 3 == 0, "only lights with a difference: {i}");
                seen += 1;
            }
        }
        assert!(seen > 0);
        let (table, list, _) = run_gpu(&device, &queue, &c, &packed, false);
        assert!(table.iter().any(|at| unpack(&list, *at).iter().any(|i| i % 3 != 0)));
    }
}
