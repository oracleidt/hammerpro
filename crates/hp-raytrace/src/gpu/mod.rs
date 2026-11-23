use std::time::Instant;

use glam::Vec3A;

use crate::Settings;
use crate::cpu::{Kind, Stat, State, Work};
use crate::device::{DeviceInfo, GpuInfo};
use crate::kernel::Tuning;
use crate::scene::{LightKind, MaterialKind, NONE, Scene};

const COMMON: &str = include_str!("trace.wgsl");
const SOFTWARE: &str = include_str!("bvh.wgsl");
const HARDWARE: &str = include_str!("rq.wgsl");

fn shader(hardware: bool) -> String {
    if hardware { format!("enable wgpu_ray_query;\n{COMMON}\n{HARDWARE}") } else { format!("{COMMON}\n{SOFTWARE}") }
}
const GROUP: u32 = 64;
const RAY_QUERY_TRIANGLES: usize = 300_000;
const MAX_GROUPS_X: u32 = 65535;

pub(crate) struct OpenError {
    pub no_gpu: bool,
    pub note: Option<String>,
}

#[repr(C)]
#[derive(Clone, Copy, Debug, Default, bytemuck::Pod, bytemuck::Zeroable)]
struct Params {
    seed: u32,
    bounces: u32,
    sky_samples: u32,
    style_bounce: u32,
    ao_radius: f32,
    clamp_lum: f32,
    passes: u32,
    nitems: u32,
    ncameras: u32,
    nbounce: u32,
    clear_root: u32,
    clear_tri: u32,
    has_clear: u32,
    mat_off: u32,
    light_off: u32,
    surf_off: u32,
    cam_off: u32,
    lists_off: u32,
    blights_off: u32,
    bcdf_off: u32,
    ainfo_off: u32,
    adata_off: u32,
    valid_off: u32,
    stats_base: u32,
    points_base: u32,
    item_base: u32,
    rq_ids: u32,
    sky_light: u32,
    contact_ao: u32,
    pad0: u32,
    pad1: u32,
    pad2: u32,
}

#[repr(C)]
#[derive(Clone, Copy, Debug, Default, bytemuck::Pod, bytemuck::Zeroable)]
struct GAttr {
    info: [u32; 4],
    uv01: [f32; 4],
    uv2lm0: [f32; 4],
    lm12: [f32; 4],
    normal: [f32; 4],
}

struct Tables {
    table: Vec<[f32; 4]>,
    words: Vec<u32>,
    base: Params,
}

pub(crate) struct Gpu {
    device: wgpu::Device,
    queue: wgpu::Queue,
    direct: wgpu::ComputePipeline,
    indirect: wgpu::ComputePipeline,
    layout: wgpu::BindGroupLayout,
    rq: Option<Rq>,
    timestamps: Option<wgpu::QuerySet>,
    tick: f64,
    use_hw: Option<bool>,
    trial: [Trial; 2],
    trial_turn: u32,
    pub calibration: Option<String>,
    words_cpu: Vec<u32>,
    rq_pipes: Option<(wgpu::ComputePipeline, wgpu::ComputePipeline, wgpu::BindGroupLayout)>,
    nodes: wgpu::Buffer,
    tris: wgpu::Buffer,
    attrs: wgpu::Buffer,
    table: wgpu::Buffer,
    words: wgpu::Buffer,
    cache: wgpu::Buffer,
    batch: Option<wgpu::Buffer>,
    slots: Vec<Slot>,
    next_slot: usize,
    last_done: Option<Instant>,
    submit_seconds: f64,
    duty: f32,
    base: Params,
    budget: u64,
    persistent: u64,
    max_binding: u64,
    seconds_per_unit: [f64; 2],
    resident: u32,
    batch_cap: u64,
    pub dispatches: u64,
    pub prep_seconds: f64,
    pub wait_seconds: f64,
    pub scatter_seconds: f64,
}

fn oct_encode(n: Vec3A) -> u32 {
    let n = n.normalize_or(Vec3A::Z);
    let s = n.x.abs() + n.y.abs() + n.z.abs();
    let mut x = n.x / s;
    let mut y = n.y / s;
    if n.z < 0.0 {
        let ox = (1.0 - y.abs()) * if x >= 0.0 { 1.0 } else { -1.0 };
        let oy = (1.0 - x.abs()) * if y >= 0.0 { 1.0 } else { -1.0 };
        x = ox;
        y = oy;
    }
    let q = |v: f32| ((v.clamp(-1.0, 1.0) * 32767.0).round() as i16) as u16 as u32;
    q(x) | (q(y) << 16)
}

fn pad16(n: usize) -> u64 {
    ((n.max(16) + 15) & !15) as u64
}

fn tables(scene: &Scene, state: &State) -> Tables {
    let packed = &state.packed;
    let mut table: Vec<[f32; 4]> = Vec::new();
    let mut words: Vec<u32> = Vec::new();
    let mut base = Params { mat_off: 0, ..Params::default() };
    for m in &scene.materials {
        let kind = match m.kind {
            MaterialKind::Opaque => 0.0,
            MaterialKind::Sky => 1.0,
            MaterialKind::Translucent => 2.0,
            MaterialKind::Water => 3.0,
            MaterialKind::Blocker => 4.0,
        };
        table.push([m.albedo[0], m.albedo[1], m.albedo[2], kind]);
        table.push([m.transmit[0], m.transmit[1], m.transmit[2], m.alpha_cutoff]);
        table.push([f32::from_bits(m.alpha), f32::from_bits(u32::from(m.two_sided)), 0.0, 0.0]);
    }
    base.light_off = table.len() as u32;
    for l in &scene.lights {
        let (kind, stopdot, stopdot2, exponent) = match l.kind {
            LightKind::Point => (0.0, 0.0, 0.0, 0.0),
            LightKind::Spot { stopdot, stopdot2, exponent } => (1.0, stopdot, stopdot2, exponent),
            LightKind::Surface => (2.0, 0.0, 0.0, 0.0),
            LightKind::Sun => (3.0, 0.0, 0.0, 0.0),
            LightKind::SkyAmbient => (4.0, 0.0, 0.0, 0.0),
        };
        table.push([l.position[0], l.position[1], l.position[2], kind]);
        table.push([l.direction[0], l.direction[1], l.direction[2], f32::from(l.style)]);
        table.push([l.intensity[0][0], l.intensity[0][1], l.intensity[0][2], l.reach.min(1.0e30)]);
        table.push([l.intensity[1][0], l.intensity[1][1], l.intensity[1][2], l.size]);
        table.push([l.constant, l.linear, l.quadratic, l.cap.min(1.0e30)]);
        table.push([l.fade.0, l.fade.1, stopdot, stopdot2]);
        table.push([exponent, 0.0, 0.0, 0.0]);
    }
    base.surf_off = table.len() as u32;
    for s in &packed.surfaces {
        let u = |v: u32| f32::from_bits(v);
        table.push([u(s.width), u(s.height), u(s.normals), u(s.slots)]);
        table.push([
            u(u32::from(s.styles[0])),
            u(u32::from(s.styles[1])),
            u(u32::from(s.styles[2])),
            u(u32::from(s.styles[3])),
        ]);
        table.push([u(s.first_luxel), u(s.lights.0), u(s.lights.1), u(u32::from(s.in_skybox))]);
        table.push([u(s.cache), 0.0, 0.0, 0.0]);
    }
    base.cam_off = table.len() as u32;
    for c in &scene.sky_cameras {
        table.push([c.origin[0], c.origin[1], c.origin[2], c.inv_scale]);
    }
    base.ncameras = scene.sky_cameras.len() as u32;
    table.push([0.0; 4]);
    base.lists_off = words.len() as u32;
    words.extend_from_slice(&scene.light_lists);
    base.blights_off = words.len() as u32;
    words.extend_from_slice(&packed.bounce_lights);
    base.bcdf_off = words.len() as u32;
    words.extend(packed.bounce_cdf.iter().map(|c| c.to_bits()));
    base.nbounce = packed.bounce_lights.len() as u32;
    base.ainfo_off = words.len() as u32;
    let mut at = 0u32;
    for a in &scene.alpha_textures {
        words.extend_from_slice(&[at, a.width, a.height, 0]);
        at += a.alpha.len() as u32;
    }
    base.adata_off = words.len() as u32;
    let mut bytes: Vec<u8> = scene.alpha_textures.iter().flat_map(|a| a.alpha.iter().copied()).collect();
    while !bytes.len().is_multiple_of(4) {
        bytes.push(255);
    }
    words.extend(bytes.chunks_exact(4).map(|c| u32::from_le_bytes([c[0], c[1], c[2], c[3]])));
    base.valid_off = words.len() as u32;
    words.extend(std::iter::repeat_n(0u32, scene.luxels.len()));
    words.push(0);
    base.sky_light = packed.sky_light.unwrap_or(NONE);
    base.passes = packed.passes as u32;
    Tables { table, words, base }
}

fn storage(device: &wgpu::Device, label: &str, size: u64, extra: wgpu::BufferUsages) -> wgpu::Buffer {
    device.create_buffer(&wgpu::BufferDescriptor {
        label: Some(label),
        size: size.max(16),
        usage: wgpu::BufferUsages::STORAGE | wgpu::BufferUsages::COPY_DST | extra,
        mapped_at_creation: false,
    })
}

fn bytes_of<T: bytemuck::Pod>(v: &[T]) -> &[u8] {
    bytemuck::cast_slice(v)
}

impl Gpu {
    pub fn open(
        scene: &Scene,
        state: &State,
        settings: &Settings,
        log: &mut dyn FnMut(&str),
    ) -> Result<(Gpu, DeviceInfo), OpenError> {
        if settings.device == crate::DevicePreference::Cpu {
            return Err(OpenError { no_gpu: false, note: Some("Ray tracing on the CPU (asked)".to_owned()) });
        }
        if std::env::var_os("HAMMERPRO_RT_NO_GPU").is_some() {
            return Err(OpenError { no_gpu: true, note: None });
        }
        let instance = wgpu::Instance::new(wgpu::InstanceDescriptor::new_without_display_handle());
        let adapter = pollster::block_on(instance.request_adapter(&wgpu::RequestAdapterOptions {
            power_preference: wgpu::PowerPreference::HighPerformance,
            force_fallback_adapter: false,
            compatible_surface: None,
            ..Default::default()
        }))
        .map_err(|_| OpenError { no_gpu: true, note: None })?;
        let info = adapter.get_info();
        if info.device_type == wgpu::DeviceType::Cpu {
            return Err(OpenError { no_gpu: true, note: Some(format!("{} is a software adapter", info.name)) });
        }
        let limits = adapter.limits();
        let required = wgpu::Limits {
            max_storage_buffer_binding_size: limits.max_storage_buffer_binding_size,
            max_buffer_size: limits.max_buffer_size,
            max_storage_buffers_per_shader_stage: limits.max_storage_buffers_per_shader_stage.min(16),
            max_compute_workgroups_per_dimension: limits.max_compute_workgroups_per_dimension,
            ..wgpu::Limits::default()
        };
        let required_rq = wgpu::Limits {
            max_blas_primitive_count: limits.max_blas_primitive_count,
            max_blas_geometry_count: limits.max_blas_geometry_count,
            max_tlas_instance_count: limits.max_tlas_instance_count,
            max_acceleration_structures_per_shader_stage: limits.max_acceleration_structures_per_shader_stage,
            max_buffers_and_acceleration_structures_per_shader_stage: limits
                .max_buffers_and_acceleration_structures_per_shader_stage
                .max(required.max_buffers_and_acceleration_structures_per_shader_stage),
            ..required.clone()
        };
        let forced = std::env::var_os("HAMMERPRO_RT_RAY_QUERY").is_some();
        let big = scene.triangles.len() >= RAY_QUERY_TRIANGLES;
        let hw_rq = adapter.features().contains(wgpu::Features::EXPERIMENTAL_RAY_QUERY)
            && std::env::var_os("HAMMERPRO_RT_NO_RAY_QUERY").is_none()
            && (big || forced);
        let ask = |rq: bool| {
            #[allow(unsafe_code)]
            let experimental = if rq {
                unsafe { wgpu::ExperimentalFeatures::enabled() }
            } else {
                wgpu::ExperimentalFeatures::disabled()
            };
            pollster::block_on(adapter.request_device(&wgpu::DeviceDescriptor {
                label: Some("hp-raytrace"),
                required_features: (if rq { wgpu::Features::EXPERIMENTAL_RAY_QUERY } else { wgpu::Features::empty() })
                    | (adapter.features() & wgpu::Features::TIMESTAMP_QUERY),
                required_limits: if rq { required_rq.clone() } else { required.clone() },
                experimental_features: experimental,
                ..Default::default()
            }))
        };
        let refused = |e: wgpu::RequestDeviceError| OpenError {
            no_gpu: true,
            note: Some(format!("the graphics card refused a compute device: {e}")),
        };
        let (device, queue, hw_rq) = match ask(hw_rq) {
            Ok((d, q)) => (d, q, hw_rq),
            Err(_) if hw_rq => {
                let (d, q) = ask(false).map_err(refused)?;
                (d, q, false)
            }
            Err(e) => return Err(refused(e)),
        };
        let max_binding = limits.max_storage_buffer_binding_size.min(limits.max_buffer_size);
        let budget = settings.vram_budget.or_else(crate::device::env_budget).unwrap_or_else(|| {
            let probe = hp_render::memory::VramProbe::new(&info);
            match probe.query() {
                Some(v) if v.budget > 0 => (v.budget.saturating_sub(v.usage) as f64 * 0.7) as u64,
                Some(v) if v.dedicated > 0 => v.dedicated / 2,
                _ => 1 << 30,
            }
            .max(256 << 20)
        });
        device.on_uncaptured_error(std::sync::Arc::new(|e| {
            eprintln!("hp-raytrace: graphics error: {e}");
        }));
        let scope = device.push_error_scope(wgpu::ErrorFilter::Validation);
        let module = device.create_shader_module(wgpu::ShaderModuleDescriptor {
            label: Some("hp-raytrace trace"),
            source: wgpu::ShaderSource::Wgsl(shader(false).into()),
        });
        let entry = |binding: u32, read_only: bool| wgpu::BindGroupLayoutEntry {
            binding,
            visibility: wgpu::ShaderStages::COMPUTE,
            ty: wgpu::BindingType::Buffer {
                ty: wgpu::BufferBindingType::Storage { read_only },
                has_dynamic_offset: false,
                min_binding_size: None,
            },
            count: None,
        };
        let mut entries = vec![wgpu::BindGroupLayoutEntry {
            binding: 0,
            visibility: wgpu::ShaderStages::COMPUTE,
            ty: wgpu::BindingType::Buffer {
                ty: wgpu::BufferBindingType::Uniform,
                has_dynamic_offset: false,
                min_binding_size: None,
            },
            count: None,
        }];
        for b in 1..8 {
            entries.push(entry(b, true));
        }
        entries.push(entry(8, false));
        let layout = device.create_bind_group_layout(&wgpu::BindGroupLayoutDescriptor {
            label: Some("hp-raytrace"),
            entries: &entries,
        });
        let pl = device.create_pipeline_layout(&wgpu::PipelineLayoutDescriptor {
            label: Some("hp-raytrace"),
            bind_group_layouts: &[Some(&layout)],
            immediate_size: 0,
        });
        let pipe = |name: &str| {
            device.create_compute_pipeline(&wgpu::ComputePipelineDescriptor {
                label: Some(name),
                layout: Some(&pl),
                module: &module,
                entry_point: Some(name),
                compilation_options: Default::default(),
                cache: None,
            })
        };
        let direct = pipe("direct_main");
        let indirect = pipe("indirect_main");
        if let Some(e) = pollster::block_on(scope.pop()) {
            return Err(OpenError {
                no_gpu: false,
                note: Some(format!("the ray tracing shader did not build on {}: {e}", info.name)),
            });
        }
        let dummy = |label: &str| storage(&device, label, 16, wgpu::BufferUsages::empty());
        let rq_pipes = if hw_rq { make_rq_pipelines(&device, &entries) } else { None };
        let mut gpu = Gpu {
            rq: None,
            timestamps: None,
            tick: 0.0,
            use_hw: None,
            trial: [Trial::default(); 2],
            trial_turn: 0,
            calibration: None,
            words_cpu: Vec::new(),
            rq_pipes,
            nodes: dummy("nodes"),
            tris: dummy("tris"),
            attrs: dummy("attrs"),
            table: dummy("table"),
            words: dummy("words"),
            cache: dummy("cache"),
            batch: None,
            slots: Vec::new(),
            next_slot: 0,
            last_done: None,
            submit_seconds: settings.submit_seconds,
            duty: settings.duty,
            device,
            queue,
            direct,
            indirect,
            layout,
            base: Params::default(),
            budget,
            persistent: 0,
            max_binding,
            seconds_per_unit: [0.0; 2],
            resident: 0,
            batch_cap: 0,
            dispatches: 0,
            prep_seconds: 0.0,
            wait_seconds: 0.0,
            scatter_seconds: 0.0,
        };
        if gpu.device.features().contains(wgpu::Features::TIMESTAMP_QUERY) {
            gpu.timestamps = Some(gpu.device.create_query_set(&wgpu::QuerySetDescriptor {
                label: Some("hp-raytrace timing"),
                ty: wgpu::QueryType::Timestamp,
                count: (IN_FLIGHT * 2) as u32,
            }));
            gpu.tick = f64::from(gpu.queue.get_timestamp_period()) * 1e-9;
        }
        gpu.upload_scene(scene, state).map_err(|e| OpenError { no_gpu: false, note: Some(e) })?;
        let mut rq_note = None;
        if gpu.rq_pipes.is_some() {
            if let Err(e) = gpu.build_rq(scene) {
                rq_note = Some(format!("hardware ray queries off: {e}"));
                gpu.rq = None;
                gpu.rq_pipes = None;
            }
        } else if hw_rq {
            rq_note =
                Some("hardware ray queries unavailable on this driver: the compute BVH traversal is used".to_owned());
        }
        if let Some(n) = &rq_note {
            log(n);
        }
        let backend = format!("{:?}", info.backend);
        log(&format!(
            "ray tracing device: {} ({backend}), {} MB of scene data on the card, {} MB budget",
            info.name,
            gpu.persistent >> 20,
            budget >> 20
        ));
        let dinfo = DeviceInfo {
            gpu: Some(GpuInfo {
                name: info.name.clone(),
                backend,
                hardware_ray_query: gpu.rq.is_some(),
                vram_budget: budget,
            }),
            threads: rayon::current_num_threads(),
            no_gpu: false,
            note: rq_note,
        };
        Ok((gpu, dinfo))
    }

    fn buffer(&self, label: &str, bytes: &[u8]) -> Result<wgpu::Buffer, String> {
        if bytes.len() as u64 > self.max_binding {
            return Err(format!(
                "the {label} data ({} MB) is larger than what the graphics card binds at once ({} MB)",
                bytes.len() >> 20,
                self.max_binding >> 20
            ));
        }
        let b = storage(&self.device, label, pad16(bytes.len()), wgpu::BufferUsages::empty());
        self.queue.write_buffer(&b, 0, bytes);
        Ok(b)
    }

    fn upload_scene(&mut self, scene: &Scene, state: &State) -> Result<(), String> {
        let p = &state.packed;
        let mut nodes = p.opaque.nodes.clone();
        let clear_root = nodes.len() as u32;
        nodes.extend_from_slice(&p.clear.nodes);
        let mut tris = p.opaque.tris.clone();
        let clear_tri = tris.len() as u32;
        tris.extend_from_slice(&p.clear.tris);
        let attrs: Vec<GAttr> = p
            .attrs
            .iter()
            .map(|a| GAttr {
                info: [a.material, a.surface, 0, 0],
                uv01: [a.uv[0][0], a.uv[0][1], a.uv[1][0], a.uv[1][1]],
                uv2lm0: [a.uv[2][0], a.uv[2][1], a.lm[0][0], a.lm[0][1]],
                lm12: [a.lm[1][0], a.lm[1][1], a.lm[2][0], a.lm[2][1]],
                normal: [a.normal[0], a.normal[1], a.normal[2], 0.0],
            })
            .collect();
        let t = tables(scene, state);
        let cache_len = p.cache.len().max(1) * 16;
        let total = (nodes.len() * 32
            + tris.len() * 48
            + attrs.len() * 80
            + t.table.len() * 16
            + t.words.len() * 4
            + cache_len) as u64;
        if total > self.budget {
            return Err(format!(
                "the scene needs {} MB on the graphics card, more than its {} MB budget",
                total >> 20,
                self.budget >> 20
            ));
        }
        self.nodes = self.buffer("nodes", bytes_of(&nodes))?;
        self.tris = self.buffer("tris", bytes_of(&tris))?;
        self.attrs = self.buffer("attrs", bytes_of(&attrs))?;
        self.table = self.buffer("table", bytes_of(&t.table))?;
        self.words = self.buffer("words", bytes_of(&t.words))?;
        self.words_cpu = t.words.clone();
        self.cache = storage(&self.device, "cache", pad16(cache_len), wgpu::BufferUsages::empty());
        if cache_len as u64 > self.max_binding {
            return Err("the light cache is larger than what the graphics card binds at once".to_owned());
        }
        self.base = t.base;
        self.base.clear_root = clear_root;
        self.base.clear_tri = clear_tri;
        self.base.has_clear = u32::from(!p.clear.tris.is_empty());
        self.persistent = total;
        self.resident = 0;
        self.batch = None;
        self.slots.clear();
        let point_words = scene.points.len() * 8;
        let point_bytes = (point_words * 4) as u64;
        let headroom = self.budget.saturating_sub(total);
        if point_bytes > 0 && point_bytes * 2 < headroom && point_bytes + (64 << 20) < self.max_binding {
            let mut words: Vec<u32> = Vec::with_capacity(point_words);
            for p in &scene.points {
                words.extend_from_slice(&[p.pos[0].to_bits(), p.pos[1].to_bits(), p.pos[2].to_bits()]);
                for n in &p.normals {
                    words.push(oct_encode(Vec3A::from_array(*n)));
                }
                words.push(0);
            }
            let cap = (point_bytes + (32 << 20)).min(self.max_binding);
            let b = storage(&self.device, "hp-raytrace points", cap, wgpu::BufferUsages::empty());
            self.queue.write_buffer(&b, 0, bytes_of(&words));
            self.batch = Some(b);
            self.batch_cap = cap;
            self.resident = point_words as u32;
            self.persistent += cap;
        }
        Ok(())
    }

    pub fn upload_lights(&mut self, scene: &Scene, state: &State) -> Result<(), String> {
        self.upload_scene(scene, state)?;
        if self.rq_pipes.is_some() {
            self.build_rq(scene)?;
        }
        Ok(())
    }

    pub fn upload_cache(&mut self, state: &State) -> Result<(), String> {
        let data: Vec<[f32; 4]> = state.packed.cache.iter().map(|c| [c[0], c[1], c[2], 0.0]).collect();
        if !data.is_empty() {
            self.queue.write_buffer(&self.cache, 0, bytes_of(&data));
        }
        let valid: Vec<u32> = state.packed.cache_valid.iter().map(|v| u32::from(*v)).collect();
        if !valid.is_empty() {
            self.queue.write_buffer(&self.words, u64::from(self.base.valid_off) * 4, bytes_of(&valid));
        }
        Ok(())
    }

    pub fn batch_bytes_per_item(&self) -> u64 {
        32 + 8 * 32 * 9 + 32 * 12 + 16
    }

    pub fn max_items(&self) -> usize {
        let room = self.budget.saturating_sub(self.persistent).max(64 << 20);
        let by_budget = (room / 3 / self.batch_bytes_per_item()) as usize;
        let by_binding = if self.resident > 0 {
            ((self.batch_cap / 4).saturating_sub(u64::from(self.resident)) / IN_FLIGHT as u64 / 8) as usize
        } else {
            (self.max_binding / (8 * 32 * 9 + 32)) as usize
        };
        by_budget.min(by_binding).clamp(256, (MAX_GROUPS_X * GROUP) as usize * 4)
    }

    pub fn seconds_per_unit(&self, kind: Kind) -> f64 {
        self.seconds_per_unit[usize::from(kind == Kind::Indirect)]
    }

    pub fn run(
        &mut self,
        scene: &Scene,
        state: &mut State,
        tune: Tuning,
        kind: Kind,
        work: &[Work],
    ) -> Result<(), String> {
        let hardware = match self.use_hw {
            Some(h) => h,
            None if self.rq.is_some() && self.rq_pipes.is_some() => {
                self.trial_turn = self.trial_turn.wrapping_add(1);
                self.trial_turn.is_multiple_of(2)
            }
            None => false,
        };
        self.dispatch(scene, state, tune, kind, work, hardware, true)
    }

    fn record_trial(&mut self, hardware: bool, gpu: f64, units: u64) {
        if self.use_hw.is_some() || units < TRIAL_UNITS || !(gpu.is_finite() && gpu > 0.0) {
            return;
        }
        let t = &mut self.trial[usize::from(hardware)];
        t.batches += 1;
        if t.batches <= TRIAL_SKIP {
            return;
        }
        t.seconds += gpu;
        t.units += units;
        let [c, h] = self.trial;
        if c.batches < TRIAL_SKIP + TRIAL_BATCHES || h.batches < TRIAL_SKIP + TRIAL_BATCHES {
            return;
        }
        let per = |t: Trial| t.seconds / t.units.max(1) as f64 * 1.0e9;
        let hw = per(h) < per(c);
        self.use_hw = Some(hw);
        self.calibration = Some(format!(
            "measured on {} batches each: hardware ray query {:.1} ns per sample, compute BVH traversal {:.1} ns per sample: {} used",
            TRIAL_BATCHES,
            per(h),
            per(c),
            if hw { "hardware ray query" } else { "compute BVH traversal" }
        ));
    }

    #[allow(clippy::too_many_arguments)]
    fn dispatch(
        &mut self,
        scene: &Scene,
        state: &mut State,
        tune: Tuning,
        kind: Kind,
        work: &[Work],
        hardware: bool,
        apply: bool,
    ) -> Result<(), String> {
        let started = Instant::now();
        let resident = self.resident > 0;
        if self.slots.is_empty() {
            let count = if resident { IN_FLIGHT } else { 1 };
            let room = if resident { (self.batch_cap / 4).saturating_sub(u64::from(self.resident)) } else { 0 };
            let per = (room / count as u64) as u32;
            for k in 0..count {
                let params = self.device.create_buffer(&wgpu::BufferDescriptor {
                    label: Some("hp-raytrace params"),
                    size: std::mem::size_of::<Params>() as u64,
                    usage: wgpu::BufferUsages::UNIFORM | wgpu::BufferUsages::COPY_DST,
                    mapped_at_creation: false,
                });
                self.slots.push(Slot {
                    params,
                    out: None,
                    staging: None,
                    resolve: None,
                    item_off: self.resident + per * k as u32,
                    item_cap: per,
                    pending: None,
                });
            }
        }
        let si = self.next_slot % self.slots.len();
        self.next_slot = self.next_slot.wrapping_add(1);
        if self.slots[si].pending.is_some() {
            self.complete(si, state)?;
        }
        let mut words: Vec<u32> = Vec::with_capacity(work.len() * 8);
        let mut points: Vec<u32> = Vec::new();
        let mut out_at = 0u32;
        let mut widths = Vec::with_capacity(work.len());
        for w in work {
            let l = &scene.luxels[w.luxel as usize];
            let width = u32::from(state.lux_width[w.luxel as usize]);
            let pfirst = if resident { l.first_point } else { (points.len() / 8) as u32 };
            for k in 0..if resident { 0 } else { l.points } {
                let p = &scene.points[(l.first_point + k) as usize];
                points.extend_from_slice(&[p.pos[0].to_bits(), p.pos[1].to_bits(), p.pos[2].to_bits()]);
                for n in &p.normals {
                    points.push(oct_encode(Vec3A::from_array(*n)));
                }
                points.push(0);
            }
            words.extend_from_slice(&[w.luxel, w.surface, w.start, w.count, pfirst, l.points, out_at, 0]);
            widths.push((out_at, width));
            out_at += width;
        }
        let (item_base, points_base) = if resident {
            (self.slots[si].item_off, 0u32)
        } else {
            let at = words.len() as u32;
            words.extend_from_slice(&points);
            (0u32, at)
        };
        let stats_base = out_at * 3;
        let out_floats = stats_base as usize + work.len() * 4;
        let mut params = self.base;
        params.seed = tune.seed;
        params.bounces = tune.bounces;
        params.sky_samples = tune.sky_samples;
        params.style_bounce = u32::from(tune.style_bounce);
        params.ao_radius = tune.ao_radius;
        params.contact_ao = u32::from(tune.contact_ao);
        params.clamp_lum = tune.clamp;
        params.nitems = work.len() as u32;
        params.stats_base = stats_base;
        params.points_base = points_base;
        params.item_base = item_base;
        params.rq_ids = self.rq.as_ref().map_or(0, |r| r.ids_off);
        let batch_bytes = (words.len() * 4) as u64 + u64::from(item_base) * 4;
        let out_bytes = (out_floats * 4) as u64;
        if batch_bytes > self.max_binding || out_bytes > self.max_binding {
            return Err("a ray tracing batch is larger than what the graphics card binds at once".to_owned());
        }
        if resident && words.len() as u64 > u64::from(self.slots[si].item_cap) {
            return Err("a ray tracing batch does not fit next to the sample points on the graphics card".to_owned());
        }
        if !resident {
            self.ensure_batch(batch_bytes);
        }
        let device = self.device.clone();
        let make = |slot: &mut Option<wgpu::Buffer>, size: u64, usage: wgpu::BufferUsages| {
            let size = size.max(16).next_power_of_two();
            if slot.as_ref().is_none_or(|b| b.size() < size) {
                *slot = Some(device.create_buffer(&wgpu::BufferDescriptor {
                    label: Some("hp-raytrace batch"),
                    size,
                    usage,
                    mapped_at_creation: false,
                }));
            }
        };
        make(&mut self.slots[si].out, out_bytes, wgpu::BufferUsages::STORAGE | wgpu::BufferUsages::COPY_SRC);
        let ts_at = out_bytes.div_ceil(256) * 256;
        make(&mut self.slots[si].staging, ts_at + 16, wgpu::BufferUsages::MAP_READ | wgpu::BufferUsages::COPY_DST);
        if self.timestamps.is_some() && self.slots[si].resolve.is_none() {
            self.slots[si].resolve = Some(self.device.create_buffer(&wgpu::BufferDescriptor {
                label: Some("hp-raytrace timing"),
                size: 256,
                usage: wgpu::BufferUsages::QUERY_RESOLVE | wgpu::BufferUsages::COPY_SRC,
                mapped_at_creation: false,
            }));
        }
        let slot = &self.slots[si];
        let (Some(batch), Some(out), Some(staging)) = (&self.batch, &slot.out, &slot.staging) else {
            return Err("graphics buffers missing".to_owned());
        };
        self.queue.write_buffer(batch, u64::from(item_base) * 4, bytes_of(&words));
        self.queue.write_buffer(&slot.params, 0, bytemuck::bytes_of(&params));
        let hardware = hardware && self.rq.is_some() && self.rq_pipes.is_some();
        let layout = match (&self.rq_pipes, hardware) {
            (Some((_, _, l)), true) => l,
            _ => &self.layout,
        };
        let mut entries = vec![
            wgpu::BindGroupEntry { binding: 0, resource: slot.params.as_entire_binding() },
            wgpu::BindGroupEntry { binding: 1, resource: self.nodes.as_entire_binding() },
            wgpu::BindGroupEntry { binding: 2, resource: self.tris.as_entire_binding() },
            wgpu::BindGroupEntry { binding: 3, resource: self.attrs.as_entire_binding() },
            wgpu::BindGroupEntry { binding: 4, resource: self.table.as_entire_binding() },
            wgpu::BindGroupEntry { binding: 5, resource: self.words.as_entire_binding() },
            wgpu::BindGroupEntry { binding: 6, resource: self.cache.as_entire_binding() },
            wgpu::BindGroupEntry { binding: 7, resource: batch.as_entire_binding() },
            wgpu::BindGroupEntry { binding: 8, resource: out.as_entire_binding() },
        ];
        if let (true, Some(rq)) = (hardware, &self.rq) {
            entries.push(wgpu::BindGroupEntry {
                binding: 9,
                resource: wgpu::BindingResource::AccelerationStructure(&rq.tlas),
            });
        }
        let bind = self.device.create_bind_group(&wgpu::BindGroupDescriptor {
            label: Some("hp-raytrace"),
            layout,
            entries: &entries,
        });
        let mut enc =
            self.device.create_command_encoder(&wgpu::CommandEncoderDescriptor { label: Some("hp-raytrace") });
        {
            let writes = self.timestamps.as_ref().map(|q| wgpu::ComputePassTimestampWrites {
                query_set: q,
                beginning_of_pass_write_index: Some(si as u32 * 2),
                end_of_pass_write_index: Some(si as u32 * 2 + 1),
            });
            let mut pass = enc.begin_compute_pass(&wgpu::ComputePassDescriptor {
                label: Some("hp-raytrace"),
                timestamp_writes: writes,
            });
            let pipe = match (&self.rq_pipes, hardware, kind) {
                (Some((d, _, _)), true, Kind::Direct) => d,
                (Some((_, i, _)), true, Kind::Indirect) => i,
                (_, _, Kind::Direct) => &self.direct,
                (_, _, Kind::Indirect) => &self.indirect,
            };
            pass.set_pipeline(pipe);
            pass.set_bind_group(0, &bind, &[]);
            let groups = (work.len() as u32).div_ceil(GROUP);
            let gx = groups.min(MAX_GROUPS_X);
            let gy = groups.div_ceil(MAX_GROUPS_X);
            pass.dispatch_workgroups(gx, gy, 1);
        }
        enc.copy_buffer_to_buffer(out, 0, staging, 0, out_bytes);
        let timed = match (&self.timestamps, &slot.resolve) {
            (Some(q), Some(r)) => {
                enc.resolve_query_set(q, si as u32 * 2..si as u32 * 2 + 2, r, 0);
                enc.copy_buffer_to_buffer(r, 0, staging, ts_at, 16);
                true
            }
            _ => false,
        };
        let map_len = if timed { ts_at + 16 } else { out_bytes };
        self.prep_seconds += started.elapsed().as_secs_f64();
        let index = self.queue.submit([enc.finish()]);
        let rx: MapResult = std::sync::Arc::new(std::sync::Mutex::new(None));
        let tx = rx.clone();
        staging.slice(..map_len).map_async(wgpu::MapMode::Read, move |r| {
            if let Ok(mut slot) = tx.lock() {
                *slot = Some(r);
            }
        });
        let units: u64 = work.iter().map(|w| u64::from(w.count)).sum();
        self.slots[si].pending = Some(Pending {
            index,
            kind,
            work: work.to_vec(),
            widths,
            stats_base,
            out_bytes,
            map_len,
            timed: timed.then_some(ts_at),
            rx,
            submitted: Instant::now(),
            units,
            apply,
            hardware,
        });
        if !apply || self.slots.len() == 1 {
            self.complete(si, state)?;
        }
        if self.duty < 1.0 {
            let spu = self.seconds_per_unit[usize::from(kind == Kind::Indirect)];
            let busy = (spu * units as f64).min(0.25);
            let rest = busy * (1.0 / f64::from(self.duty.max(0.05)) - 1.0);
            if rest > 0.0 {
                std::thread::sleep(std::time::Duration::from_secs_f64(rest.min(0.5)));
            }
        }
        Ok(())
    }

    fn complete(&mut self, si: usize, state: &mut State) -> Result<(), String> {
        let Some(p) = self.slots[si].pending.take() else { return Ok(()) };
        let waited = Instant::now();
        self.device
            .poll(wgpu::PollType::Wait {
                submission_index: Some(p.index),
                timeout: Some(std::time::Duration::from_secs(60)),
            })
            .map_err(|e| format!("the graphics card stopped answering: {e}"))?;
        let mapped = p.rx.lock().map_err(|e| e.to_string())?.take();
        match mapped {
            Some(Ok(())) => {}
            Some(Err(e)) => return Err(format!("reading the ray tracing results failed: {e}")),
            None => return Err("the graphics card did not return the ray tracing results".to_owned()),
        }
        let now = Instant::now();
        self.wait_seconds += now.duration_since(waited).as_secs_f64();
        let begin = match self.last_done {
            Some(t) if t > p.submitted => t,
            _ => p.submitted,
        };
        let gpu = now.duration_since(begin).as_secs_f64();
        self.last_done = Some(now);
        let Some(staging) = self.slots[si].staging.as_ref() else { return Err("graphics buffers missing".to_owned()) };
        let slice = staging.slice(..p.map_len);
        let mut gpu = gpu;
        if let Some(at) = p.timed {
            let data = slice.get_mapped_range().map_err(|e| format!("reading the ray tracing timing failed: {e:?}"))?;
            let t: &[u64] = bytemuck::cast_slice(&data[at as usize..at as usize + 16]);
            if t[1] > t[0] {
                gpu = (t[1] - t[0]) as f64 * self.tick;
            }
        }
        if p.apply {
            let data =
                slice.get_mapped_range().map_err(|e| format!("reading the ray tracing results failed: {e:?}"))?;
            let f: &[f32] = bytemuck::cast_slice(&data[..p.out_bytes as usize]);
            let (buf, stats) = match p.kind {
                Kind::Direct => (&mut state.direct, &mut state.dstat),
                Kind::Indirect => (&mut state.indirect, &mut state.istat),
            };
            for (i, (w, (at, width))) in p.work.iter().zip(&p.widths).enumerate() {
                let l = w.luxel as usize;
                let off = state.lux_offset[l] as usize;
                for k in 0..*width as usize {
                    let s = (*at as usize + k) * 3;
                    buf[off + k] += Vec3A::new(f[s], f[s + 1], f[s + 2]);
                }
                let sb = p.stats_base as usize + i * 4;
                let st: &mut Stat = &mut stats[l];
                st.n += f[sb];
                st.sum += f[sb + 1];
                st.sq += f[sb + 2];
                st.ao += f[sb + 3];
            }
        }
        staging.unmap();
        self.scatter_seconds += now.elapsed().as_secs_f64();
        self.dispatches += 1;
        if p.apply {
            self.record_trial(p.hardware, gpu, p.units);
        }
        if p.units >= 8192 {
            let spu = gpu / p.units as f64;
            let slot = &mut self.seconds_per_unit[usize::from(p.kind == Kind::Indirect)];
            *slot = if *slot > 0.0 { *slot * 0.7 + spu * 0.3 } else { spu };
        }
        Ok(())
    }

    fn ensure_batch(&mut self, size: u64) {
        let size = size.max(16).next_power_of_two();
        if self.batch.as_ref().is_some_and(|b| b.size() >= size) {
            return;
        }
        self.batch = Some(self.device.create_buffer(&wgpu::BufferDescriptor {
            label: Some("hp-raytrace batch"),
            size,
            usage: wgpu::BufferUsages::STORAGE | wgpu::BufferUsages::COPY_DST,
            mapped_at_creation: false,
        }));
    }

    pub fn set_pacing(&mut self, submit_seconds: f64, duty: f32) {
        self.submit_seconds = submit_seconds.clamp(0.001, 0.25);
        self.duty = if duty.is_finite() { duty.clamp(0.05, 1.0) } else { 1.0 };
    }

    pub fn submit_seconds(&self) -> f64 {
        self.submit_seconds
    }

    pub fn timing(&self) -> String {
        let mut out = format!(
            "{} GPU batches: {:.1} s preparing, {:.1} s on the card, {:.1} s gathering",
            self.dispatches, self.prep_seconds, self.wait_seconds, self.scatter_seconds
        );
        if let Some(c) = &self.calibration {
            out.push_str("; ");
            out.push_str(c);
        }
        out
    }

    pub fn read_back(&mut self, state: &mut State, _kind: Kind) -> Result<(), String> {
        let n = self.slots.len();
        for k in 0..n {
            let si = (self.next_slot + k) % n.max(1);
            self.complete(si, state)?;
        }
        Ok(())
    }
}

const IN_FLIGHT: usize = 3;

type MapResult = std::sync::Arc<std::sync::Mutex<Option<Result<(), wgpu::BufferAsyncError>>>>;

pub(crate) struct Slot {
    params: wgpu::Buffer,
    out: Option<wgpu::Buffer>,
    staging: Option<wgpu::Buffer>,
    resolve: Option<wgpu::Buffer>,
    item_off: u32,
    item_cap: u32,
    pending: Option<Pending>,
}

pub(crate) struct Pending {
    index: wgpu::SubmissionIndex,
    kind: Kind,
    work: Vec<Work>,
    widths: Vec<(u32, u32)>,
    stats_base: u32,
    out_bytes: u64,
    map_len: u64,
    timed: Option<u64>,
    rx: MapResult,
    submitted: Instant,
    units: u64,
    apply: bool,
    hardware: bool,
}

#[derive(Clone, Copy, Debug, Default)]
struct Trial {
    seconds: f64,
    units: u64,
    batches: u32,
}

const TRIAL_UNITS: u64 = 8192;
const TRIAL_SKIP: u32 = 1;
const TRIAL_BATCHES: u32 = 6;

pub(crate) struct Rq {
    tlas: wgpu::Tlas,
    _blas: Vec<wgpu::Blas>,
    _vertices: Vec<wgpu::Buffer>,
    ids_off: u32,
}

fn make_rq_pipelines(
    device: &wgpu::Device,
    base: &[wgpu::BindGroupLayoutEntry],
) -> Option<(wgpu::ComputePipeline, wgpu::ComputePipeline, wgpu::BindGroupLayout)> {
    let scope = device.push_error_scope(wgpu::ErrorFilter::Validation);
    let module = device.create_shader_module(wgpu::ShaderModuleDescriptor {
        label: Some("hp-raytrace ray query"),
        source: wgpu::ShaderSource::Wgsl(shader(true).into()),
    });
    let mut entries = base.to_vec();
    entries.push(wgpu::BindGroupLayoutEntry {
        binding: 9,
        visibility: wgpu::ShaderStages::COMPUTE,
        ty: wgpu::BindingType::AccelerationStructure { vertex_return: false },
        count: None,
    });
    let layout = device.create_bind_group_layout(&wgpu::BindGroupLayoutDescriptor {
        label: Some("hp-raytrace rq"),
        entries: &entries,
    });
    let pl = device.create_pipeline_layout(&wgpu::PipelineLayoutDescriptor {
        label: Some("hp-raytrace rq"),
        bind_group_layouts: &[Some(&layout)],
        immediate_size: 0,
    });
    let pipe = |name: &str| {
        device.create_compute_pipeline(&wgpu::ComputePipelineDescriptor {
            label: Some(name),
            layout: Some(&pl),
            module: &module,
            entry_point: Some(name),
            compilation_options: Default::default(),
            cache: None,
        })
    };
    let d = pipe("direct_main");
    let i = pipe("indirect_main");
    if let Some(e) = pollster::block_on(scope.pop()) {
        eprintln!("hp-raytrace: hardware ray query pipeline: {e:?}");
        return None;
    }
    Some((d, i, layout))
}

impl Gpu {
    fn build_rq(&mut self, scene: &Scene) -> Result<(), String> {
        let mut sets: [Vec<u32>; 4] = Default::default();
        for (i, t) in scene.triangles.iter().enumerate() {
            let a = Vec3A::from_array(t.p[0]);
            let n = (Vec3A::from_array(t.p[1]) - a).cross(Vec3A::from_array(t.p[2]) - a);
            if n.length().is_nan() || n.length() <= 1e-9 || !n.is_finite() {
                continue;
            }
            let Some(m) = scene.materials.get(t.material as usize) else { continue };
            let set = match m.kind {
                MaterialKind::Water => continue,
                MaterialKind::Translucent => 3,
                MaterialKind::Blocker => 2,
                _ if m.alpha != NONE => 1,
                _ => 0,
            };
            sets[set].push(i as u32);
        }
        let masks = [1u8, 1, 2, 4];
        let opaque = [true, false, true, false];
        let mut ids: Vec<u32> = Vec::new();
        let mut blas = Vec::new();
        let mut vertices = Vec::new();
        let mut sizes = Vec::new();
        let mut instances = Vec::new();
        let scope = self.device.push_error_scope(wgpu::ErrorFilter::Validation);
        for (k, set) in sets.iter().enumerate() {
            if set.is_empty() {
                continue;
            }
            let mut v: Vec<[f32; 3]> = Vec::with_capacity(set.len() * 3);
            for &i in set {
                v.extend_from_slice(&scene.triangles[i as usize].p);
            }
            let bytes = bytes_of(&v);
            if bytes.len() as u64 > self.max_binding {
                return Err("the scene is too large for one acceleration structure".to_owned());
            }
            let vb = self.device.create_buffer(&wgpu::BufferDescriptor {
                label: Some("hp-raytrace vertices"),
                size: pad16(bytes.len()),
                usage: wgpu::BufferUsages::BLAS_INPUT | wgpu::BufferUsages::COPY_DST,
                mapped_at_creation: false,
            });
            self.queue.write_buffer(&vb, 0, bytes);
            let size = wgpu::BlasTriangleGeometrySizeDescriptor {
                vertex_format: wgpu::VertexFormat::Float32x3,
                vertex_count: v.len() as u32,
                index_format: None,
                index_count: None,
                flags: if opaque[k] {
                    wgpu::AccelerationStructureGeometryFlags::OPAQUE
                } else {
                    wgpu::AccelerationStructureGeometryFlags::empty()
                },
            };
            let b = self.device.create_blas(
                &wgpu::CreateBlasDescriptor {
                    label: Some("hp-raytrace blas"),
                    flags: wgpu::AccelerationStructureFlags::PREFER_FAST_TRACE,
                    update_mode: wgpu::AccelerationStructureUpdateMode::Build,
                },
                wgpu::BlasGeometrySizeDescriptors::Triangles { descriptors: vec![size.clone()] },
            );
            instances.push((blas.len(), ids.len() as u32, masks[k]));
            ids.extend_from_slice(set);
            blas.push(b);
            vertices.push(vb);
            sizes.push(size);
        }
        if blas.is_empty() {
            return Err("no geometry".to_owned());
        }
        let mut tlas = self.device.create_tlas(&wgpu::CreateTlasDescriptor {
            label: Some("hp-raytrace tlas"),
            max_instances: instances.len() as u32,
            flags: wgpu::AccelerationStructureFlags::PREFER_FAST_TRACE,
            update_mode: wgpu::AccelerationStructureUpdateMode::Build,
        });
        let identity = [1.0, 0.0, 0.0, 0.0, 0.0, 1.0, 0.0, 0.0, 0.0, 0.0, 1.0, 0.0];
        for (slot, (b, base, mask)) in instances.iter().enumerate() {
            tlas[slot] = Some(wgpu::TlasInstance::new(&blas[*b], identity, *base, *mask));
        }
        let entries: Vec<wgpu::BlasBuildEntry<'_>> = blas
            .iter()
            .zip(&vertices)
            .zip(&sizes)
            .map(|((b, vb), size)| wgpu::BlasBuildEntry {
                blas: b,
                geometry: wgpu::BlasGeometries::TriangleGeometries(vec![wgpu::BlasTriangleGeometry {
                    size,
                    vertex_buffer: vb,
                    first_vertex: 0,
                    vertex_stride: 12,
                    index_buffer: None,
                    first_index: None,
                    transform_buffer: None,
                    transform_buffer_offset: None,
                }]),
            })
            .collect();
        let mut enc =
            self.device.create_command_encoder(&wgpu::CommandEncoderDescriptor { label: Some("hp-raytrace build") });
        enc.build_acceleration_structures(entries.iter(), std::iter::once(&tlas));
        let index = self.queue.submit([enc.finish()]);
        self.device
            .poll(wgpu::PollType::Wait {
                submission_index: Some(index),
                timeout: Some(std::time::Duration::from_secs(120)),
            })
            .map_err(|e| format!("building the acceleration structure timed out: {e}"))?;
        if let Some(e) = pollster::block_on(scope.pop()) {
            return Err(format!("the acceleration structure did not build: {e}"));
        }
        let off = self.base.valid_off + scene.luxels.len() as u32 + 1;
        let need = (u64::from(off) + ids.len() as u64) * 4;
        if need > self.max_binding {
            return Err("the triangle list is larger than what the card binds at once".to_owned());
        }
        self.words_cpu.truncate(off as usize);
        self.words_cpu.resize(off as usize, 0);
        self.words_cpu.extend_from_slice(&ids);
        self.words = self.buffer("words", bytes_of(&self.words_cpu))?;
        self.rq = Some(Rq { tlas, _blas: blas, _vertices: vertices, ids_off: off });
        Ok(())
    }
}
