use std::ops::Range;

use glam::{DVec3, Mat4, Vec3, Vec4};

use crate::camera::{Camera, aabb_inside, frustum_planes, mat_from_rows};
use crate::gpu::ViewUniform;
use crate::pipelines::{DEPTH_FORMAT, MeshPass, ShadowPipelines};
use crate::scene::ArenaKind;
use crate::{MaterialFlags, Renderer, classify, sky_layers};

pub(crate) const SHADOW_SIZE: u32 = 4096;
pub(crate) const SHADOW_SIZE_MANY: u32 = 2048;
const FULL_SIZE_LAYERS: u32 = 3;
pub(crate) const SHADOW_DISTANCE: f32 = 8192.0;
const SIZE_STEP: f32 = 512.0;
pub(crate) const SKY_MAP_SIZE: u32 = 2048;
pub(crate) const SKY_MIPS: u32 = 8;
const SKY_MAP_SPAN: f32 = 16384.0;
const SKY_MAP_MIN: f32 = 1024.0;
pub(crate) const MAX_COORD: f32 = 65536.0;

#[derive(Clone, Copy, Debug, PartialEq)]
pub(crate) struct ShadowFit {
    pub view_proj: Mat4,
    pub texel_world: f32,
    pub depth_range: f32,
    pub size: u32,
}

impl ShadowFit {
    pub fn params(&self) -> [f32; 4] {
        [1.0, 2.0 / self.depth_range.max(1.0), 1.0 / self.size.max(1) as f32, self.texel_world * 1.5]
    }

    pub fn soft_params(&self, spread_deg: f32) -> [f32; 4] {
        let spread = if spread_deg.is_finite() { spread_deg.clamp(0.0, 45.0).to_radians().tan() } else { 0.0 };
        [spread, self.depth_range.max(1.0), self.texel_world.max(1e-3), 0.0]
    }
}

pub(crate) fn clamp_bounds(lo: Vec3, hi: Vec3) -> (Vec3, Vec3) {
    let m = Vec3::splat(MAX_COORD);
    (lo.clamp(-m, m), hi.clamp(-m, m))
}

#[derive(Clone, Copy, Debug, PartialEq)]
pub(crate) struct SkyFit {
    pub view_proj: Mat4,
    pub origin: [f32; 2],
    pub size: f32,
    pub z_top: f32,
    pub range: f32,
}

impl SkyFit {
    pub fn params(&self) -> ([f32; 4], [f32; 4]) {
        (
            [self.origin[0], self.origin[1], 1.0 / self.size, 1.0],
            [self.z_top, self.range, self.size / SKY_MAP_SIZE as f32, 0.0],
        )
    }
}

pub(crate) fn sky_fit(eye: Vec3, scene_min: Vec3, scene_max: Vec3) -> Option<SkyFit> {
    if !(scene_min.is_finite() && scene_max.is_finite() && scene_min.cmple(scene_max).all() && eye.is_finite()) {
        return None;
    }
    let (lo, hi) = clamp_bounds(scene_min, scene_max);
    let extent = (hi.x - lo.x).max(hi.y - lo.y);
    let size = ((extent + 64.0) / SKY_MAP_MIN).ceil().max(1.0) * SKY_MAP_MIN;
    let size = size.min(SKY_MAP_SPAN);
    let step = size / 8.0;
    let axis = |e: f32, a: f32, b: f32| {
        if b - a <= size { (a + b) * 0.5 } else { ((e / step).round() * step).clamp(a + size * 0.5, b - size * 0.5) }
    };
    let (cx, cy) = (axis(eye.x, lo.x, hi.x), axis(eye.y, lo.y, hi.y));
    let z_top = hi.z + 16.0;
    let range = (hi.z - lo.z) + 32.0;
    let k = 2.0 / size;
    let rows = [
        [f64::from(k), 0.0, 0.0, f64::from(-cx * k)],
        [0.0, f64::from(k), 0.0, f64::from(-cy * k)],
        [0.0, 0.0, f64::from(-1.0 / range), f64::from(z_top / range)],
        [0.0, 0.0, 0.0, 1.0],
    ];
    let view_proj = mat_from_rows(rows);
    view_proj.is_finite().then_some(SkyFit {
        view_proj,
        origin: [cx - size * 0.5, cy - size * 0.5],
        size,
        z_top,
        range,
    })
}

pub(crate) fn fit(cam: &Camera, dir: Vec3, scene_min: Vec3, scene_max: Vec3, map_size: u32) -> Option<ShadowFit> {
    if !(scene_min.is_finite() && scene_max.is_finite() && scene_min.cmple(scene_max).all()) {
        return None;
    }
    let far = cam.far.min(SHADOW_DISTANCE);
    let aspect = cam.width / cam.height;
    let mut lo = Vec3::splat(f32::INFINITY);
    let mut hi = Vec3::splat(f32::NEG_INFINITY);
    for d in [cam.near, far] {
        let hh = d * cam.tan_half_fov;
        let hw = hh * aspect;
        let c = cam.eye + cam.forward * d;
        for (sx, sy) in [(-1.0, -1.0), (1.0, -1.0), (1.0, 1.0), (-1.0, 1.0)] {
            let p = c + cam.right * (hw * sx) + cam.up * (hh * sy);
            lo = lo.min(p);
            hi = hi.max(p);
        }
    }
    let (rlo, rhi) = (lo.max(scene_min), hi.min(scene_max));
    if !rlo.cmple(rhi).all() {
        return None;
    }
    let f = dir.normalize_or(Vec3::NEG_Z).as_dvec3();
    let hint = if f.z.abs() < 0.99 { DVec3::Z } else { DVec3::Y };
    let r = f.cross(hint).normalize();
    let u = r.cross(f);
    let corners = |a: Vec3, b: Vec3| {
        (0..8).map(move |i| {
            DVec3::new(
                if i & 1 != 0 { f64::from(b.x) } else { f64::from(a.x) },
                if i & 2 != 0 { f64::from(b.y) } else { f64::from(a.y) },
                if i & 4 != 0 { f64::from(b.z) } else { f64::from(a.z) },
            )
        })
    };
    let (mut x0, mut x1, mut y0, mut y1) = (f64::MAX, f64::MIN, f64::MAX, f64::MIN);
    for p in corners(rlo, rhi) {
        x0 = x0.min(p.dot(r));
        x1 = x1.max(p.dot(r));
        y0 = y0.min(p.dot(u));
        y1 = y1.max(p.dot(u));
    }
    let (mut d0, mut d1) = (f64::MAX, f64::MIN);
    for p in corners(scene_min, scene_max) {
        d0 = d0.min(p.dot(f));
        d1 = d1.max(p.dot(f));
    }
    let step = f64::from(SIZE_STEP);
    let size = ((x1 - x0).max(y1 - y0) + 64.0).div_euclid(step) * step + step;
    let texel = size / f64::from(map_size.max(1));
    let snap = |v: f64| (v / texel).floor() * texel;
    let (cx, cy) = ((x0 + x1) * 0.5, (y0 + y1) * 0.5);
    let (ox, oy) = (snap(cx - size * 0.5), snap(cy - size * 0.5));
    let (d0, d1) = (d0 - 64.0, d1 + 64.0);
    let range = d1 - d0;
    let k = 2.0 / size;
    let rows = [
        [r.x * k, r.y * k, r.z * k, -ox * k - 1.0],
        [u.x * k, u.y * k, u.z * k, -oy * k - 1.0],
        [f.x / range, f.y / range, f.z / range, -d0 / range],
        [0.0, 0.0, 0.0, 1.0],
    ];
    let view_proj = mat_from_rows(rows);
    view_proj.is_finite().then_some(ShadowFit {
        view_proj,
        texel_world: texel as f32,
        depth_range: range as f32,
        size: map_size.max(1),
    })
}

pub(crate) fn reversed_depth(view_proj: Mat4) -> Mat4 {
    Mat4::from_cols(Vec4::X, Vec4::Y, Vec4::new(0.0, 0.0, -1.0, 0.0), Vec4::new(0.0, 0.0, 1.0, 1.0)) * view_proj
}

fn casts_no_shadow(desc: Option<&crate::MaterialDesc>) -> bool {
    desc.is_some_and(|d| {
        d.flags.contains(MaterialFlags::SKY)
            || (d.flags.contains(MaterialFlags::NODRAW_IN_3D) && !d.flags.contains(MaterialFlags::LIGHT_BLOCKER))
    })
}

fn caster_alpha(desc: Option<&crate::MaterialDesc>) -> Option<bool> {
    if desc.is_some_and(|d| d.flags.contains(MaterialFlags::LIGHT_BLOCKER | MaterialFlags::NODRAW_IN_3D)) {
        return Some(false);
    }
    match classify(desc) {
        Some((MeshPass::Opaque, _)) => Some(false),
        Some((MeshPass::AlphaTest, _)) => Some(true),
        _ => None,
    }
}

pub(crate) const SUN_CUT_SIZE: u32 = 2048;
pub(crate) const SKY_CUT_SIZE: u32 = 1024;
const CUT_EPSILON: f32 = 0.5;

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(crate) enum Layers {
    Sun,
    Sky,
}

impl Layers {
    fn index(self) -> usize {
        match self {
            Layers::Sun => 0,
            Layers::Sky => 1,
        }
    }

    fn sun_first(self) -> bool {
        self == Layers::Sun
    }

    pub(crate) fn sizes(self, layers: u32) -> (u32, u32) {
        match self {
            Layers::Sun if layers > FULL_SIZE_LAYERS => (SHADOW_SIZE_MANY, SUN_CUT_SIZE),
            Layers::Sun => (SHADOW_SIZE, SUN_CUT_SIZE),
            Layers::Sky => (SKY_MAP_SIZE, SKY_CUT_SIZE),
        }
    }

    fn mips(self) -> u32 {
        match self {
            Layers::Sun => 1,
            Layers::Sky => SKY_MIPS,
        }
    }

    pub(crate) fn bytes(self, layers: u32) -> u64 {
        let (size, cut) = self.sizes(layers);
        let occ = u64::from(size).pow(2) * if self.mips() > 1 { 4 } else { 3 } / 3;
        (occ + u64::from(cut).pow(2)) * 4 * u64::from(layers.max(1))
    }
}

pub(crate) type SkyAnalysis = Option<(([u32; 3], u64), sky_layers::SkyLayers)>;

pub(crate) struct ShadowGpu {
    pub view: wgpu::TextureView,
    pub cut: wgpu::TextureView,
    occ_layers: Vec<wgpu::TextureView>,
    cut_layers: Vec<wgpu::TextureView>,
    cut_below: Vec<wgpu::TextureView>,
    uniforms: Vec<wgpu::Buffer>,
    cut_uniforms: Vec<wgpu::Buffer>,
    bind_groups: Option<(u64, Vec<wgpu::BindGroup>)>,
    cut_bind_groups: Option<(u64, Vec<wgpu::BindGroup>)>,
    pipelines: ShadowPipelines,
    key: Option<([f32; 16], u64, u32)>,
    size: u32,
    cut_size: u32,
    pub layers: u32,
    mips: Option<SkyMips>,
}

pub(crate) type SkyMapGpu = ShadowGpu;

struct SkyMips {
    pipeline: wgpu::RenderPipeline,
    targets: Vec<wgpu::TextureView>,
    groups: Vec<wgpu::BindGroup>,
}

fn sky_mips(device: &wgpu::Device, texture: &wgpu::Texture, layers: u32) -> SkyMips {
    let bgl = device.create_bind_group_layout(&wgpu::BindGroupLayoutDescriptor {
        label: Some("hp-render sky mips"),
        entries: &[wgpu::BindGroupLayoutEntry {
            binding: 0,
            visibility: wgpu::ShaderStages::FRAGMENT,
            ty: wgpu::BindingType::Texture {
                sample_type: wgpu::TextureSampleType::Depth,
                view_dimension: wgpu::TextureViewDimension::D2,
                multisampled: false,
            },
            count: None,
        }],
    });
    let layout = device.create_pipeline_layout(&wgpu::PipelineLayoutDescriptor {
        label: Some("hp-render sky mips"),
        bind_group_layouts: &[Some(&bgl)],
        immediate_size: 0,
    });
    let module = device.create_shader_module(wgpu::ShaderModuleDescriptor {
        label: Some("hp-render sky mips"),
        source: wgpu::ShaderSource::Wgsl(include_str!("shaders/sky_mip.wgsl").into()),
    });
    let pipeline = device.create_render_pipeline(&wgpu::RenderPipelineDescriptor {
        label: Some("hp-render sky mips"),
        layout: Some(&layout),
        vertex: wgpu::VertexState {
            module: &module,
            entry_point: Some("vs_full"),
            compilation_options: wgpu::PipelineCompilationOptions::default(),
            buffers: &[],
        },
        primitive: wgpu::PrimitiveState::default(),
        depth_stencil: Some(wgpu::DepthStencilState {
            format: DEPTH_FORMAT,
            depth_write_enabled: Some(true),
            depth_compare: Some(wgpu::CompareFunction::Always),
            stencil: wgpu::StencilState::default(),
            bias: wgpu::DepthBiasState::default(),
        }),
        multisample: wgpu::MultisampleState::default(),
        fragment: Some(wgpu::FragmentState {
            module: &module,
            entry_point: Some("fs_min"),
            compilation_options: wgpu::PipelineCompilationOptions::default(),
            targets: &[],
        }),
        multiview_mask: None,
        cache: None,
    });
    let one = |mip: u32, layer: u32| {
        texture.create_view(&wgpu::TextureViewDescriptor {
            dimension: Some(wgpu::TextureViewDimension::D2),
            base_mip_level: mip,
            mip_level_count: Some(1),
            base_array_layer: layer,
            array_layer_count: Some(1),
            ..Default::default()
        })
    };
    let mut targets = Vec::new();
    let mut groups = Vec::new();
    for m in 1..SKY_MIPS {
        for k in 0..layers.max(1) {
            targets.push(one(m, k));
            let src = one(m - 1, k);
            groups.push(device.create_bind_group(&wgpu::BindGroupDescriptor {
                label: Some("hp-render sky mips"),
                layout: &bgl,
                entries: &[wgpu::BindGroupEntry { binding: 0, resource: wgpu::BindingResource::TextureView(&src) }],
            }));
        }
    }
    SkyMips { pipeline, targets, groups }
}

fn layered_depth(device: &wgpu::Device, size: u32, layers: u32, mips: u32, label: &str) -> wgpu::Texture {
    device.create_texture(&wgpu::TextureDescriptor {
        label: Some(label),
        size: wgpu::Extent3d { width: size, height: size, depth_or_array_layers: layers.max(1) },
        mip_level_count: mips,
        sample_count: 1,
        dimension: wgpu::TextureDimension::D2,
        format: DEPTH_FORMAT,
        usage: wgpu::TextureUsages::RENDER_ATTACHMENT | wgpu::TextureUsages::TEXTURE_BINDING,
        view_formats: &[],
    })
}

fn array_view(t: &wgpu::Texture, first: u32, count: u32) -> wgpu::TextureView {
    t.create_view(&wgpu::TextureViewDescriptor {
        dimension: Some(wgpu::TextureViewDimension::D2Array),
        base_array_layer: first,
        array_layer_count: Some(count.max(1)),
        ..Default::default()
    })
}

fn layer_view(t: &wgpu::Texture, layer: u32) -> wgpu::TextureView {
    t.create_view(&wgpu::TextureViewDescriptor {
        dimension: Some(wgpu::TextureViewDimension::D2),
        base_mip_level: 0,
        mip_level_count: Some(1),
        base_array_layer: layer,
        array_layer_count: Some(1),
        ..Default::default()
    })
}

fn depth_pass<'a>(
    enc: &'a mut wgpu::CommandEncoder,
    view: &'a wgpu::TextureView,
    clear: f32,
    label: &str,
) -> wgpu::RenderPass<'a> {
    enc.begin_render_pass(&wgpu::RenderPassDescriptor {
        label: Some(label),
        color_attachments: &[],
        depth_stencil_attachment: Some(wgpu::RenderPassDepthStencilAttachment {
            view,
            depth_ops: Some(wgpu::Operations { load: wgpu::LoadOp::Clear(clear), store: wgpu::StoreOp::Store }),
            stencil_ops: None,
        }),
        timestamp_writes: None,
        occlusion_query_set: None,
        multiview_mask: None,
    })
}

pub(crate) struct CasterCull<'a> {
    pub basis: &'a sky_layers::Basis,
    pub region: &'a sky_layers::LayerRegion,
}

impl CasterCull<'_> {
    fn keeps(&self, lo: Vec3, hi: Vec3) -> bool {
        self.region.keeps(self.basis, lo, hi)
    }
}

impl Renderer {
    fn scene_bounds(&self) -> (Vec3, Vec3) {
        let (mut lo, mut hi) = self.instance_bounds.unwrap_or((Vec3::INFINITY, Vec3::NEG_INFINITY));
        for ch in self.scene.chunks.iter().flatten() {
            if !ch.is_edge() && ch.used_i > 0 {
                lo = lo.min(ch.aabb_min);
                hi = hi.max(ch.aabb_max);
            }
        }
        if lo.cmple(hi).all() { clamp_bounds(lo, hi) } else { (lo, hi) }
    }

    fn layers_mut(&mut self, which: Layers) -> &mut Option<ShadowGpu> {
        match which {
            Layers::Sun => &mut self.shadow,
            Layers::Sky => &mut self.sky_map,
        }
    }

    fn layers(&self, which: Layers) -> Option<&ShadowGpu> {
        match which {
            Layers::Sun => self.shadow.as_ref(),
            Layers::Sky => self.sky_map.as_ref(),
        }
    }

    pub(crate) fn sky_layer_info(&self, which: Layers) -> Option<&sky_layers::SkyLayers> {
        self.sky_layers[which.index()].as_ref().map(|(_, l)| l)
    }

    fn analyze_sky(&mut self, which: Layers, travel: Vec3) -> u32 {
        if self.sky_signature.0 != self.shadow_epoch {
            self.sky_signature = (self.shadow_epoch, self.scene.sky_signature());
        }
        let key = (travel.to_array().map(f32::to_bits), self.sky_signature.1);
        let slot = which.index();
        if self.sky_layers[slot].as_ref().is_none_or(|(k, _)| *k != key) {
            let mut tris = Vec::new();
            self.scene.sky_triangles(travel, &mut tris);
            let found = sky_layers::analyze(&sky_layers::Basis::of(travel), &tris, which.sun_first());
            self.sky_layers[slot] = Some((key, found));
        }
        self.sky_layers[slot].as_ref().map_or(1, |(_, l)| l.layers())
    }

    fn create_layers(&self, which: Layers, layers: u32) -> ShadowGpu {
        let (size, cut_size) = which.sizes(layers);
        let label = match which {
            Layers::Sun => "hp-render shadow map",
            Layers::Sky => "hp-render sky visibility map",
        };
        let occ = layered_depth(&self.device, size, layers, which.mips(), label);
        let cut = layered_depth(&self.device, cut_size, layers, 1, "hp-render sky cut map");
        let uniform = || {
            self.device.create_buffer(&wgpu::BufferDescriptor {
                label: Some("hp-render shadow uniform"),
                size: size_of::<ViewUniform>() as u64,
                usage: wgpu::BufferUsages::UNIFORM | wgpu::BufferUsages::COPY_DST,
                mapped_at_creation: false,
            })
        };
        ShadowGpu {
            view: array_view(&occ, 0, layers),
            cut: array_view(&cut, 0, layers),
            occ_layers: (0..layers).map(|k| layer_view(&occ, k)).collect(),
            cut_layers: (0..layers).map(|k| layer_view(&cut, k)).collect(),
            cut_below: (1..layers).map(|k| array_view(&cut, 0, k)).collect(),
            uniforms: (0..layers).map(|_| uniform()).collect(),
            cut_uniforms: (0..layers).map(|_| uniform()).collect(),
            bind_groups: None,
            cut_bind_groups: None,
            pipelines: ShadowPipelines::new(&self.device, &self.shared),
            key: None,
            size,
            cut_size,
            layers,
            mips: (which == Layers::Sky).then(|| sky_mips(&self.device, &occ, layers)),
        }
    }

    fn prepare_layers(&mut self, which: Layers, view_proj: Mat4, travel: Vec3, depth_range: f32) -> Option<bool> {
        let layers = self.analyze_sky(which, travel);
        if self.layers(which).is_none_or(|t| t.layers != layers) {
            let target = self.create_layers(which, layers);
            *self.layers_mut(which) = Some(target);
            self.scene_gen += 1;
        }
        let key = (view_proj.to_cols_array(), self.shadow_epoch, layers);
        let scene_gen = self.scene_gen;
        let queue = self.queue.clone();
        let target = self.layers_mut(which).as_mut()?;
        let dirty = target.key != Some(key);
        if dirty {
            let size = target.size as f32;
            let cut_size = target.cut_size as f32;
            let range = depth_range.max(1.0);
            let from_sun = which.sun_first();
            let cut_proj = if from_sun { reversed_depth(view_proj) } else { view_proj };
            for k in 0..target.layers {
                let base = ViewUniform {
                    view_proj: cut_proj.to_cols_array_2d(),
                    eye: [0.0; 4],
                    forward: [travel.x, travel.y, travel.z, 0.0],
                    viewport: [cut_size, cut_size, 1.0 / cut_size, 1.0 / cut_size],
                    opts: [crate::OPT_SUN_PASS | crate::OPT_SKY_FACES, 0, 0, 0],
                    sun_soft: [sky_layers::LAYER_GAP / range, k as f32, 0.0, 0.0],
                    ..Default::default()
                };
                let occluders = ViewUniform {
                    view_proj: view_proj.to_cols_array_2d(),
                    viewport: [size, size, 1.0 / size, 1.0 / size],
                    opts: [crate::OPT_SHADOW_PASS | crate::OPT_SUN_PASS | crate::OPT_SKY_CUT, 0, 0, 0],
                    sun_soft: [CUT_EPSILON / range, k as f32, if from_sun { 1.0 } else { 0.0 }, 0.0],
                    ..base
                };
                if let (Some(c), Some(o)) = (target.cut_uniforms.get(k as usize), target.uniforms.get(k as usize)) {
                    queue.write_buffer(c, 0, bytemuck::bytes_of(&base));
                    queue.write_buffer(o, 0, bytemuck::bytes_of(&occluders));
                }
            }
            target.key = Some(key);
        }
        let stale = target.bind_groups.as_ref().is_none_or(|(g, _)| *g != scene_gen)
            || target.cut_bind_groups.as_ref().is_none_or(|(g, _)| *g != scene_gen);
        if stale {
            let t = self.layers(which)?;
            let bgs: Vec<wgpu::BindGroup> = t
                .uniforms
                .iter()
                .map(|u| self.shadow_pass_bind_group_with(u, "hp-render shadow pass", Some(&t.cut)))
                .collect();
            let cut_bgs: Vec<wgpu::BindGroup> = t
                .cut_uniforms
                .iter()
                .enumerate()
                .map(|(k, u)| {
                    let below = k.checked_sub(1).and_then(|i| t.cut_below.get(i));
                    self.shadow_pass_bind_group_with(u, "hp-render sky cut pass", below)
                })
                .collect();
            let t = self.layers_mut(which).as_mut()?;
            t.bind_groups = Some((scene_gen, bgs));
            t.cut_bind_groups = Some((scene_gen, cut_bgs));
        }
        Some(dirty)
    }

    pub(crate) fn prepare_shadow(&mut self, cam: &Camera) -> Option<(ShadowFit, bool)> {
        let sun = self.lights_cpu.first().filter(|_| self.dir_lights > 0)?;
        let dir = Vec3::new(sun.dir[0], sun.dir[1], sun.dir[2]).normalize_or(Vec3::NEG_Z);
        let (lo, hi) = self.scene_bounds();
        let layers = self.analyze_sky(Layers::Sun, dir);
        let fit = fit(cam, dir, lo, hi, Layers::Sun.sizes(layers).0)?;
        let dirty = self.prepare_layers(Layers::Sun, fit.view_proj, dir, fit.depth_range)?;
        Some((fit, dirty))
    }

    pub(crate) fn prepare_sky_map(&mut self, cam: &Camera) -> Option<(SkyFit, bool)> {
        let (lo, hi) = self.scene_bounds();
        let fit = sky_fit(cam.eye, lo, hi)?;
        let dirty = self.prepare_layers(Layers::Sky, fit.view_proj, Vec3::NEG_Z, fit.range)?;
        Some((fit, dirty))
    }

    pub(crate) fn layer_count(&self, which: Layers) -> u32 {
        self.layers(which).map_or(1, |t| t.layers)
    }

    fn record_layers(
        &self,
        enc: &mut wgpu::CommandEncoder,
        which: Layers,
        view_proj: &Mat4,
        travel: Vec3,
    ) -> (u32, u64) {
        let Some(t) = self.layers(which) else { return (0, 0) };
        let (Some((_, bind_groups)), Some((_, cut_groups))) = (t.bind_groups.as_ref(), t.cut_bind_groups.as_ref())
        else {
            return (0, 0);
        };
        let planes: [Vec4; 6] = frustum_planes(view_proj);
        let (mut draws, mut tris) = (0u32, 0u64);
        for (k, (target, group)) in t.cut_layers.iter().zip(cut_groups).enumerate() {
            let mut pass = depth_pass(enc, target, 0.0, "hp-render sky cut pass");
            pass.set_bind_group(0, group, &[]);
            let pipeline = if k == 0 { &t.pipelines.sky_cut } else { &t.pipelines.sky_peel };
            let (d, n) = self.draw_sky_cut(&mut pass, pipeline, &planes);
            draws += d;
            tris += n;
        }
        let basis = sky_layers::Basis::of(travel);
        let info = self.sky_layer_info(which);
        for (k, (target, group)) in t.occ_layers.iter().zip(bind_groups).enumerate() {
            let mut pass = depth_pass(enc, target, 1.0, "hp-render shadow pass");
            pass.set_bind_group(0, group, &[]);
            let cull = match (k, info.and_then(|l| l.region(k as u32))) {
                (0, _) => None,
                (_, Some(region)) => Some(CasterCull { basis: &basis, region }),
                (_, None) => continue,
            };
            let (d, n) = self.draw_shadow_casters(&mut pass, &t.pipelines, &planes, cull.as_ref());
            draws += d;
            tris += n;
        }
        if let Some(mips) = &t.mips {
            for (target, group) in mips.targets.iter().zip(&mips.groups) {
                let mut pass = depth_pass(enc, target, 1.0, "hp-render sky mips");
                pass.set_pipeline(&mips.pipeline);
                pass.set_bind_group(0, group, &[]);
                pass.draw(0..3, 0..1);
            }
        }
        (draws, tris)
    }

    pub(crate) fn record_sky_map(&self, enc: &mut wgpu::CommandEncoder, fit: &SkyFit) -> (u32, u64) {
        self.record_layers(enc, Layers::Sky, &fit.view_proj, Vec3::NEG_Z)
    }

    pub(crate) fn record_shadow(&self, enc: &mut wgpu::CommandEncoder, fit: &ShadowFit) -> (u32, u64) {
        let dir = self
            .lights_cpu
            .first()
            .map_or(Vec3::NEG_Z, |s| Vec3::new(s.dir[0], s.dir[1], s.dir[2]).normalize_or(Vec3::NEG_Z));
        self.record_layers(enc, Layers::Sun, &fit.view_proj, dir)
    }

    pub(crate) fn shadow_pass_bind_group(&self, uniform: &wgpu::Buffer, label: &str) -> wgpu::BindGroup {
        self.shadow_pass_bind_group_with(uniform, label, None)
    }

    fn shadow_pass_bind_group_with(
        &self,
        uniform: &wgpu::Buffer,
        label: &str,
        cut: Option<&wgpu::TextureView>,
    ) -> wgpu::BindGroup {
        let dummy = &self.shadow_dummy_array;
        self.device.create_bind_group(&wgpu::BindGroupDescriptor {
            label: Some(label),
            layout: &self.shared.view_bgl,
            entries: &[
                wgpu::BindGroupEntry { binding: 0, resource: uniform.as_entire_binding() },
                wgpu::BindGroupEntry { binding: 1, resource: self.states.buffer.as_entire_binding() },
                wgpu::BindGroupEntry { binding: 2, resource: wgpu::BindingResource::Sampler(&self.sampler) },
                wgpu::BindGroupEntry { binding: 3, resource: wgpu::BindingResource::Sampler(&self.sampler_clamp) },
                wgpu::BindGroupEntry { binding: 4, resource: self.lights_buf.buffer.as_entire_binding() },
                wgpu::BindGroupEntry { binding: 5, resource: self.dummy_storage.as_entire_binding() },
                wgpu::BindGroupEntry { binding: 6, resource: self.dummy_storage.as_entire_binding() },
                wgpu::BindGroupEntry { binding: 7, resource: wgpu::BindingResource::TextureView(dummy) },
                wgpu::BindGroupEntry { binding: 8, resource: wgpu::BindingResource::Sampler(&self.shadow_sampler) },
                wgpu::BindGroupEntry { binding: 9, resource: self.dummy_storage.as_entire_binding() },
                wgpu::BindGroupEntry { binding: 10, resource: self.dummy_storage.as_entire_binding() },
                wgpu::BindGroupEntry { binding: 11, resource: self.light_styles.as_entire_binding() },
                wgpu::BindGroupEntry { binding: 12, resource: wgpu::BindingResource::TextureView(self.cubes.view()) },
                wgpu::BindGroupEntry { binding: 13, resource: self.cubes.info().as_entire_binding() },
                wgpu::BindGroupEntry {
                    binding: 14,
                    resource: wgpu::BindingResource::TextureView(&self.local_shadows.dummy),
                },
                wgpu::BindGroupEntry { binding: 15, resource: wgpu::BindingResource::TextureView(dummy) },
                wgpu::BindGroupEntry {
                    binding: 16,
                    resource: wgpu::BindingResource::TextureView(cut.unwrap_or(dummy)),
                },
                wgpu::BindGroupEntry { binding: 17, resource: wgpu::BindingResource::TextureView(dummy) },
                wgpu::BindGroupEntry { binding: 18, resource: self.dummy_storage.as_entire_binding() },
                wgpu::BindGroupEntry { binding: 19, resource: self.projectors.buffer.as_entire_binding() },
                wgpu::BindGroupEntry {
                    binding: 20,
                    resource: wgpu::BindingResource::TextureView(&self.projectors.view),
                },
                wgpu::BindGroupEntry {
                    binding: 21,
                    resource: wgpu::BindingResource::TextureView(&self.shadow_dummy_array),
                },
            ],
        })
    }

    fn draw_sky_cut(
        &self,
        pass: &mut wgpu::RenderPass<'_>,
        pipeline: &wgpu::RenderPipeline,
        planes: &[Vec4; 6],
    ) -> (u32, u64) {
        let (mut draws, mut tris) = (0u32, 0u64);
        let mut set = false;
        for m in &self.scene.materials {
            if !m.desc.as_ref().is_some_and(|d| d.flags.contains(MaterialFlags::SKY)) {
                continue;
            }
            for &c in &m.chunks {
                let Some(Some(ch)) = self.scene.chunks.get(c as usize) else { continue };
                if ch.used_i == 0 || ch.idx.len == 0 || !aabb_inside(planes, ch.aabb_min, ch.aabb_max) {
                    continue;
                }
                if !set {
                    pass.set_pipeline(pipeline);
                    pass.set_vertex_buffer(0, self.arenas[ArenaKind::MeshVerts as usize].buffer.slice(..));
                    pass.set_index_buffer(
                        self.arenas[ArenaKind::Indices as usize].buffer.slice(..),
                        wgpu::IndexFormat::Uint32,
                    );
                    set = true;
                }
                pass.draw_indexed(ch.idx.offset..ch.idx.offset + ch.used_i, 0, 0..1);
                draws += 1;
                tris += u64::from(ch.used_i / 3);
            }
        }
        (draws, tris)
    }

    pub(crate) fn draw_shadow_casters(
        &self,
        pass: &mut wgpu::RenderPass<'_>,
        pipelines: &ShadowPipelines,
        planes: &[Vec4; 6],
        cull: Option<&CasterCull<'_>>,
    ) -> (u32, u64) {
        let (mut draws, mut tris) = (0u32, 0u64);
        pass.set_vertex_buffer(0, self.arenas[ArenaKind::MeshVerts as usize].buffer.slice(..));
        pass.set_index_buffer(self.arenas[ArenaKind::Indices as usize].buffer.slice(..), wgpu::IndexFormat::Uint32);
        let mut current: Option<bool> = None;
        for (mi, m) in self.scene.materials.iter().enumerate() {
            if casts_no_shadow(m.desc.as_ref()) {
                continue;
            }
            let Some(alpha) = caster_alpha(m.desc.as_ref()) else { continue };
            let mut bound = false;
            for &c in &m.chunks {
                let Some(Some(ch)) = self.scene.chunks.get(c as usize) else { continue };
                if ch.used_i == 0 || ch.idx.len == 0 || !aabb_inside(planes, ch.aabb_min, ch.aabb_max) {
                    continue;
                }
                if cull.is_some_and(|k| !k.keeps(ch.aabb_min, ch.aabb_max)) {
                    continue;
                }
                if current != Some(alpha) {
                    pass.set_pipeline(if alpha { &pipelines.chunk_alpha } else { &pipelines.chunk });
                    current = Some(alpha);
                }
                if alpha
                    && !bound
                    && let Some(g) = self.mat_gpu.get(mi)
                {
                    pass.set_bind_group(1, &g.bind_group, &[]);
                    bound = true;
                }
                pass.draw_indexed(ch.idx.offset..ch.idx.offset + ch.used_i, 0, 0..1);
                draws += 1;
                tris += u64::from(ch.used_i / 3);
            }
        }
        if self.instance_count == 0 {
            return (draws, tris);
        }
        pass.set_vertex_buffer(1, self.instances.buffer.slice(..));
        let mut runs: Vec<Range<u32>> = Vec::new();
        let mut current: Option<u8> = None;
        for g in &self.instance_groups {
            let Some(model) = self.models.get(&g.key) else { continue };
            let hull = model.shadow_hull.as_ref().filter(|_| g.hull > 0);
            let mesh_count = if hull.is_some() { g.count - g.hull.min(g.count) } else { g.count };
            let culled = |range: Range<u32>, runs: &mut Vec<Range<u32>>| {
                runs.clear();
                match cull {
                    None => runs.push(range),
                    Some(k) => {
                        for i in range {
                            let keep = self.instance_boxes.get(i as usize).is_some_and(|(lo, hi)| k.keeps(*lo, *hi));
                            match runs.last_mut() {
                                Some(r) if keep && r.end == i => r.end = i + 1,
                                _ if keep => runs.push(i..i + 1),
                                _ => {}
                            }
                        }
                    }
                }
            };
            culled(g.start..g.start + mesh_count, &mut runs);
            if !runs.is_empty() {
                let mut bound = false;
                for part in &model.parts {
                    let desc = self.scene.materials.get(part.material as usize).and_then(|m| m.desc);
                    if casts_no_shadow(desc.as_ref()) {
                        continue;
                    }
                    let alpha = match classify(desc.as_ref()) {
                        Some((MeshPass::Opaque, _)) => false,
                        Some((MeshPass::AlphaTest, _)) => true,
                        _ => continue,
                    };
                    if alpha {
                        let Some(mat) = self.mat_gpu.get(part.material as usize) else { continue };
                        pass.set_bind_group(1, &mat.bind_group, &[]);
                    }
                    let kind = u8::from(alpha);
                    if current != Some(kind) {
                        pass.set_pipeline(if alpha { &pipelines.model_alpha } else { &pipelines.model });
                        current = Some(kind);
                    }
                    if !bound {
                        pass.set_vertex_buffer(0, model.vbuf.slice(..));
                        pass.set_vertex_buffer(2, model.tbuf.slice(..));
                        pass.set_index_buffer(model.ibuf.slice(..), wgpu::IndexFormat::Uint32);
                        bound = true;
                    }
                    for r in &runs {
                        pass.draw_indexed(part.first..part.first + part.count, 0, r.clone());
                        draws += 1;
                        tris += u64::from(part.count / 3) * u64::from(r.end - r.start);
                    }
                }
            }
            let Some(h) = hull else { continue };
            culled(g.start + mesh_count..g.start + g.count, &mut runs);
            if runs.is_empty() {
                continue;
            }
            if current != Some(0) {
                pass.set_pipeline(&pipelines.model);
                current = Some(0);
            }
            pass.set_vertex_buffer(0, h.verts.slice(..));
            pass.set_vertex_buffer(2, h.tans.slice(..));
            for r in &runs {
                pass.draw(0..h.count, r.clone());
                draws += 1;
                tris += u64::from(h.count / 3) * u64::from(r.end - r.start);
            }
        }
        (draws, tris)
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::types::{RenderMode, ViewKind, ViewParams};

    fn cam(eye: [f32; 3], forward: [f32; 3]) -> Camera {
        Camera::new(&ViewParams {
            game_pixel_grid: false,
            size: [800, 600],
            kind: ViewKind::Perspective { eye, forward, up: [0.0, 0.0, 1.0], fov_y_deg: 70.0, near: 4.0, far: 30000.0 },
            mode: RenderMode::Lit,
            grid: None,
            background: [0.0; 3],
            selection_color: [1.0; 4],
            show_sprites: false,
            show_models: false,
            msaa: false,
        })
    }

    #[test]
    fn fit_contains_visible_scene() {
        let c = cam([0.0, -2000.0, 500.0], [0.0, 1.0, -0.3]);
        let (smin, smax) = (Vec3::new(-5000.0, -5000.0, -100.0), Vec3::new(5000.0, 5000.0, 1000.0));
        let f = fit(&c, Vec3::new(0.3, 0.2, -1.0), smin, smax, SHADOW_SIZE).expect("fit");
        let p = f.view_proj * Vec4::new(0.0, 0.0, 0.0, 1.0);
        assert!(p.x.abs() < 1.0 && p.y.abs() < 1.0 && (0.0..=1.0).contains(&p.z), "{p:?}");
        let q = f.view_proj * Vec4::new(-30.0, -20.0, 100.0, 1.0);
        assert!(q.z < p.z);
        for i in 0..8 {
            let v = Vec3::new(
                if i & 1 != 0 { smax.x } else { smin.x },
                if i & 2 != 0 { smax.y } else { smin.y },
                if i & 4 != 0 { smax.z } else { smin.z },
            );
            let z = (f.view_proj * v.extend(1.0)).z;
            assert!((0.0..=1.0).contains(&z), "corner {v:?} depth {z}");
        }
        assert!(f.texel_world > 0.0 && f.texel_world < 8.0);
    }

    #[test]
    fn fit_is_stable_and_handles_degenerate_input() {
        let (smin, smax) = (Vec3::splat(-1000.0), Vec3::splat(1000.0));
        let a = fit(&cam([0.0; 3], [1.0, 0.0, 0.0]), Vec3::NEG_Z, smin, smax, SHADOW_SIZE).expect("fit");
        let b = fit(&cam([0.01, 0.0, 0.0], [1.0, 0.0, 0.0]), Vec3::NEG_Z, smin, smax, SHADOW_SIZE).expect("fit");
        assert_eq!(a.view_proj, b.view_proj, "tiny camera moves keep the same texels");
        assert!(fit(&cam([0.0; 3], [1.0, 0.0, 0.0]), Vec3::new(0.0, 0.0, -1.0), smin, smax, SHADOW_SIZE).is_some());
        assert!(
            fit(&cam([0.0; 3], [1.0, 0.0, 0.0]), Vec3::NEG_Z, Vec3::INFINITY, Vec3::NEG_INFINITY, SHADOW_SIZE)
                .is_none()
        );
        let far_scene = (Vec3::splat(100_000.0), Vec3::splat(100_100.0));
        assert!(fit(&cam([0.0; 3], [1.0, 0.0, 0.0]), Vec3::NEG_Z, far_scene.0, far_scene.1, SHADOW_SIZE).is_none());
        assert!(fit(&cam([0.0; 3], [1.0, 0.0, 0.0]), Vec3::NAN, smin, smax, SHADOW_SIZE).is_some());
    }
}
