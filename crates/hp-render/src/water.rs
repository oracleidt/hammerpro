use ahash::{HashMap, HashMapExt};
use bytemuck::{Pod, Zeroable};

use crate::camera::Camera;
use crate::gpu::{GpuVertex, srgb_to_linear};
use crate::pipelines::{self, Blend, CHUNK_ATTRS, COLOR_FORMAT, Desc, Shared};
use crate::scene::ArenaKind;
use crate::{
    ChunkDraw, DrawCtx, FrameCounter, MaterialDesc, MaterialFlags, MeshPass, RenderMode, Renderer, TexId, ViewOptions,
    ViewParams, WaterDesc,
};

pub(crate) const WATER_WGSL: &str =
    concat!(include_str!("shaders/common.wgsl"), "\n", include_str!("shaders/water.wgsl"));

const W_REFRACT: u32 = 1;
const W_REFLECT: u32 = 2;
const W_CHEAP: u32 = 4;
const W_CHEAP_BLEND: u32 = 8;
const W_MULTITEXTURE: u32 = 16;
const W_BASE: u32 = 32;
const W_FRESNEL: u32 = 64;
const W_NORMAL: u32 = 128;
const W_BOTTOM: u32 = 256;
const W_NO_ENVMAP: u32 = 512;
const W_BELOW: u32 = 1024;

#[repr(C)]
#[derive(Clone, Copy, Debug, Default, PartialEq, Pod, Zeroable)]
pub(crate) struct WaterMatUniform {
    pub refract: [f32; 4],
    pub reflect: [f32; 4],
    pub fog: [f32; 4],
    pub ranges: [f32; 4],
    pub bump_uv: [f32; 4],
    pub scroll: [f32; 4],
    pub scroll12: [f32; 4],
    pub frames: [f32; 4],
    pub bottom_refract: [f32; 4],
    pub bottom_fog: [f32; 4],
    pub bottom_ranges: [f32; 4],
    pub flags: [u32; 4],
}

#[repr(C)]
#[derive(Clone, Copy, Debug, Default, PartialEq, Pod, Zeroable)]
struct WaterViewUniform {
    depth: [f32; 4],
    background: [f32; 4],
    under_fog: [f32; 4],
    under_range: [f32; 4],
}

fn finite(v: f32, fallback: f32) -> f32 {
    if v.is_finite() { v } else { fallback }
}

fn linear_rgb(c: [f32; 3]) -> [f32; 3] {
    c.map(|v| srgb_to_linear(finite(v, 1.0).clamp(0.0, 16.0)))
}

fn rgbw(c: [f32; 3], w: f32) -> [f32; 4] {
    [c[0], c[1], c[2], w]
}

fn fog_range(r: [f32; 2]) -> (f32, f32) {
    let start = finite(r[0], 0.0).clamp(-1.0e6, 1.0e6);
    let end = finite(r[1], start + 1.0).clamp(-1.0e6, 1.0e6);
    (start, 1.0 / (end - start).max(1.0))
}

fn speed(v: [f32; 2]) -> [f32; 2] {
    v.map(|s| finite(s, 0.0).clamp(-100.0, 100.0))
}

pub(crate) fn water_uniform(desc: &MaterialDesc, w: &WaterDesc) -> WaterMatUniform {
    let inv = |v: f32| if v.is_finite() && v.abs() > 1e-6 { 1.0 / v } else { 1.0 / 512.0 };
    let amount = |v: f32| finite(v, 0.0).clamp(-64.0, 64.0);
    let (fog_start, fog_inv) = fog_range(w.fog_range);
    let cheap_start = finite(w.cheap_range[0], 500.0).max(0.0);
    let cheap_end = finite(w.cheap_range[1], 1000.0).max(cheap_start + 1.0);
    let bump = if w.bump_uv.iter().all(|v| v.is_finite()) { w.bump_uv } else { [1.0, 1.0, 0.0, 0.0] };
    let scroll = speed(w.bump_scroll);
    let (s1, s2) = (speed(w.scroll1), speed(w.scroll2));
    let [cols, rows, count] = w.frames;
    let animated = count > 1 && cols >= 1 && rows >= 1 && cols.saturating_mul(rows) >= count;
    let frames = if animated {
        [cols as f32, rows as f32, count as f32, finite(w.frame_rate, 0.0).clamp(0.0, 240.0)]
    } else {
        [1.0, 1.0, 1.0, 0.0]
    };
    let mut flags = 0;
    for (on, bit) in [
        (w.refract, W_REFRACT),
        (w.reflect, W_REFLECT),
        (w.cheap, W_CHEAP),
        (w.cheap_blend, W_CHEAP_BLEND),
        (s1[0] != 0.0, W_MULTITEXTURE),
        (w.base_texture.is_some(), W_BASE),
        (w.fresnel, W_FRESNEL),
        (w.normal_map.is_some(), W_NORMAL),
        (w.bottom.is_some(), W_BOTTOM),
        (!w.envmap && !w.cheap, W_NO_ENVMAP),
        (!w.above_water, W_BELOW),
    ] {
        if on {
            flags |= bit;
        }
    }
    let (bottom_refract, bottom_fog, bottom_start) = match &w.bottom {
        Some(b) => {
            let (start, inv) = fog_range(b.fog_range);
            (rgbw(linear_rgb(b.refract_tint), amount(b.refract_amount)), rgbw(linear_rgb(b.fog_color), inv), start)
        }
        None => ([1.0, 1.0, 1.0, 0.0], [0.0, 0.0, 0.0, 1.0], 0.0),
    };
    WaterMatUniform {
        refract: rgbw(linear_rgb(w.refract_tint), amount(w.refract_amount)),
        reflect: rgbw(linear_rgb(w.reflect_tint), amount(w.reflect_amount)),
        fog: rgbw(linear_rgb(w.fog_color), fog_inv),
        ranges: [fog_start, fog_start + 1.0 / fog_inv, cheap_start, cheap_end],
        bump_uv: bump,
        scroll: [scroll[0], scroll[1], inv(desc.tex_size[0]), inv(desc.tex_size[1])],
        scroll12: [s1[0], s1[1], s2[0], s2[1]],
        frames,
        bottom_refract,
        bottom_fog,
        bottom_ranges: [bottom_start, 0.0, finite(w.reflect_blend, 1.0).clamp(0.0, 1.0), 0.0],
        flags: [flags, 0, 0, 0],
    }
}

pub(crate) fn far_uniform(desc: &MaterialDesc, w: &WaterDesc) -> Option<WaterMatUniform> {
    if w.cheap || !(w.reflect || w.refract) || !w.above_water {
        return None;
    }
    let mut u = water_uniform(desc, w);
    u.flags[0] = (u.flags[0] & !(W_REFLECT | W_REFRACT)) | W_CHEAP;
    Some(u)
}

fn box_distance(eye: glam::Vec3, lo: glam::Vec3, hi: glam::Vec3) -> f32 {
    eye.clamp(lo, hi).distance(eye)
}

pub(crate) fn animates(w: &WaterDesc) -> bool {
    let moving = |v: [f32; 2]| speed(v) != [0.0; 2];
    w.normal_map.is_some()
        && (moving(w.bump_scroll)
            || (speed(w.scroll1)[0] != 0.0 && (moving(w.scroll1) || moving(w.scroll2)))
            || (w.frames[2] > 1 && finite(w.frame_rate, 0.0) > 0.0))
}

pub(crate) fn applies(params: &ViewParams, opts: &ViewOptions, is_ortho: bool) -> bool {
    !is_ortho
        && opts.water_preview
        && matches!(
            params.mode,
            RenderMode::Textured | RenderMode::TexturedWire | RenderMode::Lit | RenderMode::Unshaded
        )
}

pub(crate) struct WaterFrame {
    pub draws: Vec<ChunkDraw>,
    pub animated: bool,
    pub under: bool,
}

struct WaterPipes {
    surface: wgpu::RenderPipeline,
    depth: wgpu::RenderPipeline,
    under_fog: wgpu::RenderPipeline,
}

struct WaterMatGpu {
    key: (MaterialDesc, [bool; 2]),
    bind: wgpu::BindGroup,
    far: Option<(wgpu::BindGroup, f32)>,
}

struct WaterViewGpu {
    generation: u64,
    copy: wgpu::Texture,
    uniform: wgpu::Buffer,
    bind: wgpu::BindGroup,
}

pub(crate) struct WaterState {
    mat_bgl: wgpu::BindGroupLayout,
    view_bgl: [wgpu::BindGroupLayout; 2],
    layouts: [wgpu::PipelineLayout; 2],
    depth_layout: wgpu::PipelineLayout,
    modules: [wgpu::ShaderModule; 2],
    pipes: [Option<WaterPipes>; 2],
    mats: HashMap<u32, WaterMatGpu>,
    views: HashMap<u32, WaterViewGpu>,
    blank_sky: wgpu::BindGroup,
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

fn texture_entry(binding: u32) -> wgpu::BindGroupLayoutEntry {
    wgpu::BindGroupLayoutEntry {
        binding,
        visibility: wgpu::ShaderStages::FRAGMENT,
        ty: wgpu::BindingType::Texture {
            sample_type: wgpu::TextureSampleType::Float { filterable: true },
            view_dimension: wgpu::TextureViewDimension::D2,
            multisampled: false,
        },
        count: None,
    }
}

impl WaterState {
    fn new(device: &wgpu::Device, shared: &Shared, white: &wgpu::TextureView) -> Self {
        let fragment = wgpu::ShaderStages::FRAGMENT;
        let mat_bgl = device.create_bind_group_layout(&wgpu::BindGroupLayoutDescriptor {
            label: Some("hp-render water material"),
            entries: &[uniform_entry(0, fragment), texture_entry(1), texture_entry(2)],
        });
        let view_bgl = [false, true].map(|multisampled| {
            device.create_bind_group_layout(&wgpu::BindGroupLayoutDescriptor {
                label: Some("hp-render water view"),
                entries: &[
                    uniform_entry(0, fragment),
                    texture_entry(1),
                    wgpu::BindGroupLayoutEntry {
                        binding: 2,
                        visibility: fragment,
                        ty: wgpu::BindingType::Texture {
                            sample_type: wgpu::TextureSampleType::Depth,
                            view_dimension: wgpu::TextureViewDimension::D2,
                            multisampled,
                        },
                        count: None,
                    },
                ],
            })
        });
        let layouts = [0, 1].map(|i| {
            device.create_pipeline_layout(&wgpu::PipelineLayoutDescriptor {
                label: Some("hp-render water layout"),
                bind_group_layouts: &[
                    Some(&shared.view_bgl),
                    Some(&mat_bgl),
                    Some(&view_bgl[i]),
                    Some(&shared.sky_bgl),
                ],
                immediate_size: 0,
            })
        });
        let depth_layout = device.create_pipeline_layout(&wgpu::PipelineLayoutDescriptor {
            label: Some("hp-render water depth layout"),
            bind_group_layouts: &[Some(&shared.view_bgl)],
            immediate_size: 0,
        });
        let modules = [false, true].map(|multisampled| {
            let src = water_source(multisampled);
            device.create_shader_module(wgpu::ShaderModuleDescriptor {
                label: Some("hp-render water"),
                source: wgpu::ShaderSource::Wgsl(src.into()),
            })
        });
        let entries: Vec<wgpu::BindGroupEntry<'_>> = (0..6)
            .map(|i| wgpu::BindGroupEntry { binding: i, resource: wgpu::BindingResource::TextureView(white) })
            .collect();
        let blank_sky = device.create_bind_group(&wgpu::BindGroupDescriptor {
            label: Some("hp-render water blank sky"),
            layout: &shared.sky_bgl,
            entries: &entries,
        });
        Self {
            mat_bgl,
            view_bgl,
            layouts,
            depth_layout,
            modules,
            pipes: [None, None],
            mats: HashMap::new(),
            views: HashMap::new(),
            blank_sky,
        }
    }

    fn ensure_pipes(&mut self, device: &wgpu::Device, samples: u32) {
        let i = usize::from(samples > 1);
        if self.pipes[i].is_some() {
            return;
        }
        let vb = [Some(pipelines::vertex_layout(size_of::<GpuVertex>(), wgpu::VertexStepMode::Vertex, &CHUNK_ATTRS))];
        let (module, layout) = (&self.modules[i], &self.layouts[i]);
        let surface = |fs: &str, depth_write: bool, blend: Blend| {
            let layout = if depth_write { &self.depth_layout } else { layout };
            pipelines::build(
                device,
                samples,
                Desc {
                    label: "hp-render water",
                    module,
                    vs: "vs_water",
                    fs,
                    layout,
                    buffers: &vb,
                    topology: wgpu::PrimitiveTopology::TriangleList,
                    cull: None,
                    depth_test: true,
                    depth_write,
                    blend,
                    constants: &[],
                },
            )
        };
        let under_fog = pipelines::build(
            device,
            samples,
            Desc {
                label: "hp-render underwater fog",
                module,
                vs: "vs_full_water",
                fs: "fs_under_fog",
                layout,
                buffers: &[],
                topology: wgpu::PrimitiveTopology::TriangleList,
                cull: None,
                depth_test: false,
                depth_write: false,
                blend: Blend::Alpha,
                constants: &[],
            },
        );
        self.pipes[i] = Some(WaterPipes {
            surface: surface("fs_water", false, Blend::Replace),
            depth: surface("fs_water_depth", true, Blend::Additive),
            under_fog,
        });
    }
}

pub(crate) fn water_source(multisampled: bool) -> String {
    if multisampled {
        WATER_WGSL.replace("var scene_depth: texture_depth_2d;", "var scene_depth: texture_depth_multisampled_2d;")
    } else {
        WATER_WGSL.to_owned()
    }
}

impl Renderer {
    pub(crate) fn prepare_water(
        &mut self,
        view_id: u32,
        cam: &Camera,
        params: &ViewParams,
        opts: &ViewOptions,
    ) -> Option<WaterFrame> {
        if !applies(params, opts, cam.is_ortho) {
            return None;
        }
        let mut draws = Vec::new();
        let mut animated = false;
        let mut under: Option<(WaterDesc, u32)> = None;
        let radius = finite(opts.cull_radius, 0.0).max(0.0);
        for (mi, m) in self.scene.materials.iter().enumerate() {
            let Some((desc, w)) = m.desc.as_ref().and_then(|d| d.water.map(|w| (d, w))) else { continue };
            if m.chunks.is_empty() || desc.flags.contains(MaterialFlags::NODRAW_IN_3D) {
                continue;
            }
            let mut used = false;
            for &c in &m.chunks {
                let Some(Some(ch)) = self.scene.chunks.get(c as usize) else { continue };
                if ch.used_i == 0 || ch.idx.len == 0 || ch.all_hidden {
                    continue;
                }
                if under.is_none() {
                    for id in ch.members.keys() {
                        let Some(mesh) = self.scene.objects.get(id).and_then(|o| o.stored.as_ref()) else { continue };
                        let e = cam.eye;
                        let compiled =
                            self.scene.flags(*id).is_some_and(|f| f.contains(crate::ObjectFlags::COMPILED_LIGHT));
                        let over = !compiled
                            || (e.x >= ch.aabb_min.x
                                && e.x <= ch.aabb_max.x
                                && e.y >= ch.aabb_min.y
                                && e.y <= ch.aabb_max.y);
                        if over && e.cmpge(mesh.aabb_min).all() && e.cmple(mesh.aabb_max).all() && e.z < ch.aabb_max.z {
                            under = Some((w, mi as u32));
                            break;
                        }
                    }
                }
                if !cam.aabb_visible(ch.aabb_min, ch.aabb_max) {
                    continue;
                }
                if radius > 0.0 && cam.eye.clamp(ch.aabb_min, ch.aabb_max).distance_squared(cam.eye) > radius * radius {
                    continue;
                }
                used = true;
                draws.push(ChunkDraw {
                    material: mi as u32,
                    pass: MeshPass::Opaque,
                    two_sided: true,
                    first: ch.idx.offset,
                    count: ch.used_i,
                    dist: 0.0,
                    chunk: c,
                    aabb_min: ch.aabb_min,
                    aabb_max: ch.aabb_max,
                });
            }
            animated |= used && animates(&w);
        }
        if draws.is_empty() && under.is_none() {
            return None;
        }
        let samples = if params.msaa { 4 } else { 1 };
        if self.water.is_none() {
            let white = self.texture_view_or_missing(TexId::WHITE)?.clone();
            self.water = Some(WaterState::new(&self.device, &self.shared, &white));
        }
        let sky = self.environment.sky.filter(|s| s.faces.iter().any(Option::is_some));
        if let Some(s) = sky {
            self.ensure_sky_bind(s.faces);
        }
        let has_sky = sky.is_some_and(|s| self.sky_bind.as_ref().is_some_and(|(f, _)| *f == s.faces))
            || (sky.is_some() && self.hdr_sky_bind(opts, params.mode, cam.is_ortho).is_some());
        let mut wanted: Vec<u32> = draws.iter().map(|d| d.material).chain(under.map(|u| u.1)).collect();
        wanted.sort_unstable();
        wanted.dedup();
        for slot in wanted {
            self.ensure_water_material(slot);
        }
        let (gen_now, size) = {
            let v = self.views.get(&view_id)?;
            (v.generation, v.size)
        };
        let water = self.water.as_mut()?;
        water.ensure_pipes(&self.device, samples);
        let fresh = water.views.get(&view_id).is_none_or(|v| v.generation != gen_now);
        if fresh {
            let view = self.views.get(&view_id)?;
            let copy = self.device.create_texture(&wgpu::TextureDescriptor {
                label: Some("hp-render water scene copy"),
                size: wgpu::Extent3d { width: size[0], height: size[1], depth_or_array_layers: 1 },
                mip_level_count: 1,
                sample_count: 1,
                dimension: wgpu::TextureDimension::D2,
                format: COLOR_FORMAT,
                usage: wgpu::TextureUsages::TEXTURE_BINDING | wgpu::TextureUsages::COPY_DST,
                view_formats: &[],
            });
            let uniform = self.device.create_buffer(&wgpu::BufferDescriptor {
                label: Some("hp-render water view uniform"),
                size: size_of::<WaterViewUniform>() as u64,
                usage: wgpu::BufferUsages::UNIFORM | wgpu::BufferUsages::COPY_DST,
                mapped_at_creation: false,
            });
            let copy_view = copy.create_view(&wgpu::TextureViewDescriptor::default());
            let bind = self.device.create_bind_group(&wgpu::BindGroupDescriptor {
                label: Some("hp-render water view"),
                layout: &water.view_bgl[usize::from(view.samples > 1)],
                entries: &[
                    wgpu::BindGroupEntry { binding: 0, resource: uniform.as_entire_binding() },
                    wgpu::BindGroupEntry { binding: 1, resource: wgpu::BindingResource::TextureView(&copy_view) },
                    wgpu::BindGroupEntry { binding: 2, resource: wgpu::BindingResource::TextureView(&view.depth_view) },
                ],
            });
            water.views.insert(view_id, WaterViewGpu { generation: gen_now, copy, uniform, bind });
        }
        let (n, f) = (f64::from(cam.near.max(1e-3)), f64::from(cam.far.max(cam.near + 1.0)));
        let a = (n / (f - n)) as f32;
        let b = (n * f / (f - n)) as f32;
        let bg = params.background.map(|c| srgb_to_linear(finite(c, 0.0)));
        let (under_fog, under_range) = match &under {
            Some((w, _)) => {
                let (start, inv) = fog_range(w.fog_range);
                (rgbw(linear_rgb(w.fog_color), inv), [start, start + 1.0 / inv, 1.0, 0.0])
            }
            None => ([0.0; 4], [0.0, 1.0, 0.0, 0.0]),
        };
        let u = WaterViewUniform {
            depth: [a, b, if has_sky { 1.0 } else { 0.0 }, 0.0],
            background: [bg[0], bg[1], bg[2], 1.0],
            under_fog,
            under_range,
        };
        if let Some(v) = water.views.get(&view_id) {
            self.queue.write_buffer(&v.uniform, 0, bytemuck::bytes_of(&u));
        }
        Some(WaterFrame { draws, animated, under: under.is_some() })
    }

    fn ensure_water_material(&mut self, slot: u32) {
        let Some(desc) = self.scene.materials.get(slot as usize).and_then(|m| m.desc) else { return };
        let Some(w) = desc.water else { return };
        let exists = |t: Option<TexId>| t.is_some_and(|t| self.textures.contains_key(&t.0));
        let key = (desc, [exists(w.normal_map), exists(w.base_texture)]);
        let Some(state) = self.water.as_ref() else { return };
        if state.mats.get(&slot).is_some_and(|m| m.key == key) {
            return;
        }
        let tex = |t: Option<TexId>, fallback: TexId| {
            let id = t.filter(|t| self.textures.contains_key(&t.0)).unwrap_or(fallback);
            self.texture_view_or_missing(id)
        };
        let mut w_live = w;
        if !key.1[0] {
            w_live.normal_map = None;
        }
        if !key.1[1] {
            w_live.base_texture = None;
        }
        let (Some(normal), Some(base)) = (tex(w.normal_map, TexId::WHITE), tex(w.base_texture, TexId::WHITE)) else {
            return;
        };
        let uniform = self.device.create_buffer(&wgpu::BufferDescriptor {
            label: Some("hp-render water material"),
            size: size_of::<WaterMatUniform>() as u64,
            usage: wgpu::BufferUsages::UNIFORM | wgpu::BufferUsages::COPY_DST,
            mapped_at_creation: false,
        });
        self.queue.write_buffer(&uniform, 0, bytemuck::bytes_of(&water_uniform(&desc, &w_live)));
        let make_bind = |uniform: &wgpu::Buffer| {
            self.device.create_bind_group(&wgpu::BindGroupDescriptor {
                label: Some("hp-render water material"),
                layout: &state.mat_bgl,
                entries: &[
                    wgpu::BindGroupEntry { binding: 0, resource: uniform.as_entire_binding() },
                    wgpu::BindGroupEntry { binding: 1, resource: wgpu::BindingResource::TextureView(normal) },
                    wgpu::BindGroupEntry { binding: 2, resource: wgpu::BindingResource::TextureView(base) },
                ],
            })
        };
        let bind = make_bind(&uniform);
        let far = far_uniform(&desc, &w_live).map(|u| {
            let buf = self.device.create_buffer(&wgpu::BufferDescriptor {
                label: Some("hp-render far water material"),
                size: size_of::<WaterMatUniform>() as u64,
                usage: wgpu::BufferUsages::UNIFORM | wgpu::BufferUsages::COPY_DST,
                mapped_at_creation: false,
            });
            self.queue.write_buffer(&buf, 0, bytemuck::bytes_of(&u));
            let end = finite(w.cheap_range[1], 1000.0).max(finite(w.cheap_range[0], 500.0).max(0.0) + 1.0);
            (make_bind(&buf), end)
        });
        if let Some(state) = self.water.as_mut() {
            state.mats.insert(slot, WaterMatGpu { key, bind, far });
        }
    }

    pub(crate) fn forget_water_view(&mut self, view_id: u32) {
        if let Some(w) = self.water.as_mut() {
            w.views.remove(&view_id);
        }
    }

    pub(crate) fn water_bytes(&self) -> u64 {
        self.water
            .as_ref()
            .map_or(0, |w| w.views.values().map(|v| u64::from(v.copy.width()) * u64::from(v.copy.height()) * 4).sum())
    }
}

impl DrawCtx<'_> {
    pub(crate) fn record_water(
        &self,
        enc: &mut wgpu::CommandEncoder,
        frame: Option<&WaterFrame>,
        view_id: u32,
        lists: &[Vec<ChunkDraw>; crate::pipelines::MESH_PASSES],
        keep: bool,
        n: &mut FrameCounter,
    ) {
        let r = self.r;
        let view = self.view;
        let (target, resolve) = match &view.msaa_view {
            Some(m) => (m, Some(&view.color_view)),
            None => (&view.color_view, None),
        };
        let ready = frame.and(r.water.as_ref()).and_then(|state| {
            let frame = frame?;
            let pipes = state.pipes[usize::from(view.samples > 1)].as_ref()?;
            let wv = state.views.get(&view_id)?;
            let first = frame.draws.first().map(|d| d.material);
            let any_mat = first.and_then(|m| state.mats.get(&m)).or_else(|| state.mats.values().next())?;
            let hdr = r.environment.sky.and_then(|_| r.hdr_sky_bind(self.opts, self.params.mode, self.cam.is_ortho));
            let sky = match (hdr, r.environment.sky, r.sky_bind.as_ref()) {
                (Some(bind), _, _) => bind,
                (None, Some(s), Some((faces, bind))) if *faces == s.faces => bind,
                _ => &state.blank_sky,
            };
            Some((state, pipes, wv, any_mat, sky))
        });
        let bind_common = |pass: &mut wgpu::RenderPass<'_>| {
            if let Some((_, _, wv, any_mat, sky)) = ready {
                pass.set_bind_group(0, self.bind_group, &[]);
                pass.set_bind_group(1, &any_mat.bind, &[]);
                pass.set_bind_group(2, &wv.bind, &[]);
                pass.set_bind_group(3, sky, &[]);
            }
        };
        let draw_surfaces = |pass: &mut wgpu::RenderPass<'_>, depth_only: bool, n: &mut FrameCounter| {
            let Some((state, pipes, ..)) = ready else { return };
            let Some(frame) = frame.filter(|f| !f.draws.is_empty()) else { return };
            pass.set_pipeline(if depth_only { &pipes.depth } else { &pipes.surface });
            pass.set_vertex_buffer(0, r.arenas[ArenaKind::MeshVerts as usize].buffer.slice(..));
            pass.set_index_buffer(r.arenas[ArenaKind::Indices as usize].buffer.slice(..), wgpu::IndexFormat::Uint32);
            let mut mat = (u32::MAX, false);
            let eye = self.cam.eye;
            for d in &frame.draws {
                if !depth_only {
                    let Some(m) = state.mats.get(&d.material) else { continue };
                    let far = m.far.as_ref().filter(|(_, end)| box_distance(eye, d.aabb_min, d.aabb_max) > *end);
                    if mat != (d.material, far.is_some()) {
                        pass.set_bind_group(1, far.map_or(&m.bind, |(b, _)| b), &[]);
                        n.material();
                        mat = (d.material, far.is_some());
                    }
                }
                pass.draw_indexed(d.first..d.first + d.count, 0, 0..1);
                n.world(u64::from(d.count / 3));
            }
        };
        if let Some((_, pipes, wv, ..)) = ready {
            let [w, h] = view.size;
            enc.copy_texture_to_texture(
                view.color.as_image_copy(),
                wv.copy.as_image_copy(),
                wgpu::Extent3d { width: w, height: h, depth_or_array_layers: 1 },
            );
            let mut pass = enc.begin_render_pass(&wgpu::RenderPassDescriptor {
                label: Some("hp-render water pass"),
                color_attachments: &[Some(wgpu::RenderPassColorAttachment {
                    view: target,
                    depth_slice: None,
                    resolve_target: None,
                    ops: wgpu::Operations { load: wgpu::LoadOp::Load, store: wgpu::StoreOp::Store },
                })],
                depth_stencil_attachment: Some(wgpu::RenderPassDepthStencilAttachment {
                    view: &view.depth_view,
                    depth_ops: None,
                    stencil_ops: None,
                }),
                timestamp_writes: None,
                occlusion_query_set: None,
                multiview_mask: None,
            });
            bind_common(&mut pass);
            if frame.is_some_and(|f| f.under) {
                pass.set_pipeline(&pipes.under_fog);
                pass.draw(0..3, 0..1);
                n.add(1);
            }
            draw_surfaces(&mut pass, false, n);
        }
        let mut pass = enc.begin_render_pass(&wgpu::RenderPassDescriptor {
            label: Some("hp-render after water pass"),
            color_attachments: &[Some(wgpu::RenderPassColorAttachment {
                view: target,
                depth_slice: None,
                resolve_target: resolve,
                ops: wgpu::Operations {
                    load: wgpu::LoadOp::Load,
                    store: if resolve.is_some() && !keep { wgpu::StoreOp::Discard } else { wgpu::StoreOp::Store },
                },
            })],
            depth_stencil_attachment: Some(wgpu::RenderPassDepthStencilAttachment {
                view: &view.depth_view,
                depth_ops: Some(wgpu::Operations {
                    load: wgpu::LoadOp::Load,
                    store: if keep { wgpu::StoreOp::Store } else { wgpu::StoreOp::Discard },
                }),
                stencil_ops: None,
            }),
            timestamp_writes: None,
            occlusion_query_set: None,
            multiview_mask: None,
        });
        pass.set_bind_group(0, self.bind_group, &[]);
        draw_surfaces(&mut pass, true, &mut FrameCounter::default());
        self.record_3d_rest_blended(&mut pass, lists, n);
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn shader_variants_parse() {
        for ms in [false, true] {
            let src = water_source(ms);
            assert!(!src.contains('\u{2014}') && !src.contains('\u{2013}'), "dash character");
            assert_eq!(src.contains("texture_depth_multisampled_2d"), ms);
            assert!(src.contains("struct View"));
        }
        assert_eq!(size_of::<WaterMatUniform>(), 192);
        assert_eq!(size_of::<WaterViewUniform>(), 64);
    }

    #[test]
    fn expensive_water_turns_cheap_far_away() {
        let d = MaterialDesc::default();
        let w = WaterDesc { envmap: true, cheap_range: [2000.0, 3000.0], ..Default::default() };
        let near = water_uniform(&d, &w);
        assert_eq!(near.flags[0] & (W_REFLECT | W_REFRACT | W_CHEAP), W_REFLECT | W_REFRACT);
        assert_eq!(near.flags[0] & (W_NO_ENVMAP | W_BELOW), 0);
        let far = far_uniform(&d, &w).expect("expensive water has a LOD");
        assert_eq!(far.flags[0] & (W_REFLECT | W_REFRACT | W_CHEAP), W_CHEAP);
        assert_eq!(far.ranges[3], 3000.0);
        let bare = far_uniform(&d, &WaterDesc { envmap: false, ..w }).expect("lod");
        assert_ne!(bare.flags[0] & W_NO_ENVMAP, 0);
        assert!(far_uniform(&d, &WaterDesc { cheap: true, reflect: false, ..w }).is_none());
        let under = WaterDesc { above_water: false, reflect: false, ..w };
        assert!(far_uniform(&d, &under).is_none());
        assert_ne!(water_uniform(&d, &under).flags[0] & W_BELOW, 0);
        assert!((box_distance(glam::Vec3::new(0.0, 0.0, 10.0), glam::Vec3::ZERO, glam::Vec3::ONE) - 9.0).abs() < 1e-5);
    }

    #[test]
    fn uniform_follows_the_material() {
        let d = MaterialDesc { tex_size: [256.0, 128.0], ..Default::default() };
        let w = WaterDesc {
            refract_amount: 1.0,
            reflect_amount: 0.4,
            fog_color: [7.0 / 255.0, 58.0 / 255.0, 66.0 / 255.0],
            fog_range: [0.0, 1024.0],
            scroll1: [0.01, 0.01],
            scroll2: [-0.025, 0.025],
            normal_map: Some(TexId::WHITE),
            ..Default::default()
        };
        let u = water_uniform(&d, &w);
        let on = W_REFRACT | W_REFLECT | W_MULTITEXTURE | W_NORMAL | W_FRESNEL;
        assert_eq!(u.flags[0] & on, on);
        assert!(u.flags[0] & W_CHEAP == 0 && u.flags[0] & W_BOTTOM == 0);
        assert_eq!(u.scroll[2], 1.0 / 256.0);
        assert!((u.fog[3] - 1.0 / 1024.0).abs() < 1e-9);
        assert_eq!(u.ranges[1], 1024.0);
        assert!(u.fog[1] > u.fog[0], "linear fog color keeps its hue");
        assert!(animates(&w));
        assert!(!animates(&WaterDesc { normal_map: Some(TexId::WHITE), ..Default::default() }));
        let bad = WaterDesc {
            refract_amount: f32::NAN,
            fog_range: [f32::INFINITY, f32::NAN],
            bump_uv: [f32::NAN; 4],
            frames: [0, 0, 9],
            frame_rate: f32::NAN,
            ..Default::default()
        };
        let u = water_uniform(&MaterialDesc { tex_size: [0.0, f32::NAN], ..Default::default() }, &bad);
        let all: Vec<f32> = [u.refract, u.reflect, u.fog, u.ranges, u.bump_uv, u.scroll, u.frames].concat();
        assert!(all.iter().all(|v| v.is_finite()), "{all:?}");
        assert_eq!(u.frames, [1.0, 1.0, 1.0, 0.0], "a bad frame grid is not animated");
    }
}
