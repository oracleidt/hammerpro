mod arena;
mod bgimages;
mod camera;
mod compiled_light;
mod control_shadow;
mod cubemaps;
mod detail_sprites;
mod extra_passes;
mod fog_volume;
mod gamewire;
mod gpu;
pub mod hdr;
pub mod hooks;
mod indirect;
mod instance_cull;
pub mod lightmap;
mod lights;
mod local_shadow;
pub mod memory;
mod occlusion;
mod opaque;
mod outline;
mod overlay;
pub mod parallax;
mod park;
mod particles;
pub mod pipeline_cache;
mod pipelines;
pub mod profile;
mod projector;
mod reflection;
mod scene;
mod shadow;
mod sky;
mod sky_layers;
mod sprite_layers;
mod static_lines;
mod sun_glow;
mod tangents;
mod textures;
pub mod types;
mod water;

pub use compiled_light::{
    CompiledBatch, CompiledBlock, CompiledLightMode, CompiledLightStats, CompiledLightmap, CompiledVertex,
    LIGHT_STYLES, ModelLighting, ModelLocalLight, NO_COMPILED_LIGHTMAP,
};
pub use cubemaps::{
    DEFAULT_PROBE_SIZE, MAX_RESOLUTION as MAX_CUBEMAP_RESOLUTION, MEMORY_BUDGET as CUBEMAP_MEMORY_BUDGET,
};
pub use hdr::{ToneEnv, ToneMode, ToneSettings};
pub use hooks::{HookContext, PassHook};
pub use indirect::IndirectVolume;
pub use lights::MAX_PER_CLUSTER;
pub use local_shadow::MAX_SHADOWED_LIGHTS;
pub use park::ParkedScene;
pub use particles::{ParticleAnimation, ParticleBlend, ParticleInstance, ParticleOrient};
pub use projector::{MAX_PROJECTORS, PROJECTOR_TEXTURE_SIZE, Projector, ProjectorImage};
pub use sky::HdrSkyFace;
pub use sprite_layers::SPRITE_LAYER_PARTICLES;
pub use tangents::compute_tangents;
pub use types::*;

use ahash::{HashMap, HashMapExt};
use glam::{Mat4, Vec3};

use arena::{Grow, GrowBuffer};
use camera::Camera;
use gpu::{
    InstanceGpu, LightGpu, MaterialUniform, ObjState, SpriteGpu, ViewUniform, pack_rgba, srgb_to_linear,
    srgba_to_linear,
};
use lights::ClusterGrid;
use overlay::{Chunk, OverlayLayout};
use pipelines::{COLOR_FORMAT, DEPTH_FORMAT, DISPLAY_FORMAT, LDR_FORMAT, MESH_PASSES, MeshPass, PipelineSet, Shared};
use scene::{ARENA_ELEM, ArenaKind, NULL_SLOT, Scene};
use textures::TexEntry;

const UNKNOWN_MODEL_HALF: f32 = 8.0;
const TOOL_ALPHA: f32 = 0.55;

struct MaterialGpu {
    uniform: wgpu::Buffer,
    bind_group: wgpu::BindGroup,
    texs: MatTextures,
}

type MatTextures = [Option<TexId>; 9];

struct ModelPartGpu {
    material: u32,
    first: u32,
    count: u32,
    lod: u32,
}

struct ModelGpu {
    vbuf: wgpu::Buffer,
    tbuf: wgpu::Buffer,
    ibuf: wgpu::Buffer,
    parts: Vec<ModelPartGpu>,
    lod_parts: Vec<ModelPartGpu>,
    lod_ranges: Vec<(f32, f32)>,
    edges: Option<(wgpu::Buffer, u32)>,
    collision: Option<(wgpu::Buffer, u32)>,
    collision_hull: Option<(wgpu::Buffer, u32)>,
    shadow_hull: Option<HullGpu>,
    no_collision: bool,
    aabb_min: Vec3,
    aabb_max: Vec3,
    bytes: u64,
}

struct HullGpu {
    verts: wgpu::Buffer,
    tans: wgpu::Buffer,
    count: u32,
}

#[derive(Clone, Copy)]
struct Group {
    key: u32,
    start: u32,
    count: u32,
    hull: u32,
    lod: Option<u32>,
}

struct RetainedGpu {
    buffer: wgpu::Buffer,
    size: u64,
    layout: OverlayLayout,
    chunks: [Vec<Chunk>; 4],
}

#[derive(Clone, Copy, PartialEq)]
struct ClusterKey {
    view_proj: [f32; 16],
    size: [u32; 2],
    lights_gen: u64,
}

struct ViewState {
    size: [u32; 2],
    samples: u32,
    color: wgpu::Texture,
    color_view: wgpu::TextureView,
    display: wgpu::Texture,
    display_srgb: wgpu::TextureView,
    display_view: wgpu::TextureView,
    msaa_view: Option<wgpu::TextureView>,
    depth_view: wgpu::TextureView,
    generation: u64,
    target_bytes: u64,
    uniform: wgpu::Buffer,
    overlay: GrowBuffer,
    clusters: GrowBuffer,
    light_indices: GrowBuffer,
    cluster_gen: u64,
    cluster_key: Option<ClusterKey>,
    bind_group: Option<wgpu::BindGroup>,
    bind_key: (u64, u64),
    sky_pass: Option<(wgpu::Buffer, wgpu::BindGroup, (u64, u64))>,
    draws: u32,
    triangles: u64,
    counts: ViewCounts,
    chunks_seen: u32,
    chunks_skipped: u32,
    occ: Option<occlusion::OcclusionView>,
    outline: Option<outline::OutlineTarget>,
    animated: bool,
}

#[derive(Default)]
struct FrameCounter {
    draws: u32,
    triangles: u64,
    chunks_seen: u32,
    chunks_skipped: u32,
    counts: ViewCounts,
}

impl FrameCounter {
    fn add(&mut self, triangles: u64) {
        self.draws += 1;
        self.triangles += triangles;
    }

    fn world(&mut self, triangles: u64) {
        self.add(triangles);
        self.counts.world_draws += 1;
        self.counts.world_triangles += triangles;
    }

    fn model(&mut self, triangles: u64, instances: u32) {
        self.add(triangles);
        self.counts.model_draws += 1;
        self.counts.model_triangles += triangles;
        self.counts.model_instances += instances;
    }

    fn material(&mut self) {
        self.counts.material_changes += 1;
    }

    fn view_counts(&self) -> ViewCounts {
        ViewCounts {
            draw_calls: self.draws,
            triangles: self.triangles,
            chunks_in_frustum: self.chunks_seen,
            chunks_occluded: self.chunks_skipped,
            ..self.counts
        }
    }
}

pub struct Renderer {
    device: wgpu::Device,
    queue: wgpu::Queue,
    supports_bc: bool,
    max_tex_dim: u32,
    max_buffer: u64,
    max_storage: u64,
    shared: Shared,
    pipelines: [Option<PipelineSet>; 2],
    sampler: wgpu::Sampler,
    sampler_clamp: wgpu::Sampler,
    textures: HashMap<u32, TexEntry>,
    next_tex: u32,
    mat_gpu: Vec<MaterialGpu>,
    scene: Scene,
    arenas: [GrowBuffer; 3],
    states: GrowBuffer,
    scene_gen: u64,
    models: HashMap<u32, ModelGpu>,
    next_model: u32,
    instances: GrowBuffer,
    instance_count: u32,
    instance_groups: Vec<Group>,
    instance_refs: Vec<ObjectId>,
    sprites: GrowBuffer,
    sprite_groups: Vec<Group>,
    sprite_refs: Vec<ObjectId>,
    sprite_layers: std::collections::BTreeMap<u32, sprite_layers::SpriteLayer>,
    lights_buf: GrowBuffer,
    lights_cpu: Vec<LightGpu>,
    dir_lights: u32,
    lights_left_out: u32,
    crowded_clusters: u32,
    crowded_logged: u64,
    sun: SunParams,
    sky_map: Option<shadow::SkyMapGpu>,
    ambient: [f32; 3],
    lights_gen: u64,
    views: HashMap<u32, ViewState>,
    next_generation: u64,
    fallback_view: wgpu::TextureView,
    overlay_scratch: Vec<gpu::ColorVertex>,
    overlay_bytes: Vec<u8>,
    retained: HashMap<u32, RetainedGpu>,
    retained_uploads: u64,
    staging: memory::StagingPacer,
    prewarm: Option<std::thread::JoinHandle<Vec<(usize, PipelineSet)>>>,
    shadow: Option<shadow::ShadowGpu>,
    control_shadow: Option<control_shadow::ControlShadow>,
    shadow_dummy_array: wgpu::TextureView,
    sky_layers: [shadow::SkyAnalysis; 2],
    sky_signature: (u64, u64),
    shadow_sampler: wgpu::Sampler,
    dummy_storage: wgpu::Buffer,
    light_styles: wgpu::Buffer,
    shadow_epoch: u64,
    sun_fit: Option<shadow::ShadowFit>,
    sun_cam: Option<(Mat4, u64)>,
    sun_followup: bool,
    render_serial: u64,
    instance_bounds: Option<(Vec3, Vec3)>,
    instance_boxes: Vec<(Vec3, Vec3)>,
    instance_data: Vec<InstanceGpu>,
    instance_pool: instance_cull::InstancePool,
    instances_dirty: bool,
    content_gen: u64,
    game_hidden: ahash::AHashSet<ObjectId>,
    compiled_objects: ahash::AHashSet<ObjectId>,
    play_hidden: ahash::AHashSet<ObjectId>,
    play_base: ahash::AHashMap<ObjectId, ObjectFlags>,
    view_options: HashMap<u32, ViewOptions>,
    static_lines: std::collections::BTreeMap<u32, static_lines::StaticLayer>,
    sky_bind: Option<([Option<TexId>; 6], wgpu::BindGroup)>,
    hdr_sky: Option<sky::HdrSky>,
    tone: hdr::Tone,
    compiled_sky: compiled_light::sky3d::CompiledSky,
    hooks: Vec<Box<dyn PassHook>>,
    opaque: opaque::OpaquePass,
    lightmap: lightmap::Lightmaps,
    compiled: compiled_light::CompiledLight,
    model_lights: compiled_light::ModelLights,
    environment: Environment,
    sel_chunks: ((u64, u64), Vec<u32>),
    translucent_instances: bool,
    instance_list: Vec<ModelInstance>,
    occ_pipes: Option<occlusion::OcclusionPipelines>,
    outline_pipes: Option<outline::OutlinePipelines>,
    bg_pipes: Option<bgimages::BgPipelines>,
    bg_images: Vec<bgimages::BgImageGpu>,
    water: Option<water::WaterState>,
    fog_volumes: Vec<FogVolume>,
    fog_volume_gpu: Option<fog_volume::FogVolumeState>,
    sun_glow: Option<SunGlow>,
    sun_glow_gpu: Option<sun_glow::SunGlowState>,
    game_outlines: Option<gamewire::OutlineCache>,
    detail: detail_sprites::DetailSprites,
    particles: particles::Particles,
    preview_particles: particles::Particles,
    cubes: cubemaps::Cubes,
    probing: bool,
    quality: Quality,
    profiler: Option<profile::GpuProfiler>,
    frame_cpu: profile::CpuTimes,
    reflection_cpu_ms: f32,
    reflection_active: bool,
    reflecting: Option<reflection::Reflecting>,
    water_reflect: Option<(Mat4, wgpu::TextureView)>,
    local_shadows: local_shadow::LocalShadows,
    indirect: indirect::Indirect,
    projectors: projector::Projectors,
    texture_limit: u32,
    upload_failed: bool,
}

fn classify(desc: Option<&MaterialDesc>) -> Option<(MeshPass, bool)> {
    classify_with(desc, false, false)
}

fn classify_with(desc: Option<&MaterialDesc>, force_opaque: bool, sky_holes: bool) -> Option<(MeshPass, bool)> {
    let Some(d) = desc else { return Some((MeshPass::Opaque, false)) };
    let f = d.flags;
    if f.contains(MaterialFlags::NODRAW_IN_3D) || (sky_holes && f.contains(MaterialFlags::SKY)) {
        return None;
    }
    if f.intersects(MaterialFlags::MODULATE | MaterialFlags::DECAL_MODULATE) {
        return Some((MeshPass::Modulate, f.contains(MaterialFlags::TWO_SIDED)));
    }
    if force_opaque && !f.contains(MaterialFlags::TOOL) {
        let pass = if f.contains(MaterialFlags::ALPHA_TEST) { MeshPass::AlphaTest } else { MeshPass::Opaque };
        return Some((pass, f.contains(MaterialFlags::TWO_SIDED)));
    }
    let pass = if f.contains(MaterialFlags::ADDITIVE) {
        MeshPass::Additive
    } else if f.intersects(MaterialFlags::TRANSLUCENT | MaterialFlags::TOOL) {
        MeshPass::Translucent
    } else if f.contains(MaterialFlags::ALPHA_TEST) {
        MeshPass::AlphaTest
    } else {
        MeshPass::Opaque
    };
    Some((pass, f.contains(MaterialFlags::TWO_SIDED)))
}

fn material_uniform(desc: Option<&MaterialDesc>, env_cube: u32) -> MaterialUniform {
    let Some(d) = desc else {
        return MaterialUniform {
            tint: [1.0; 4],
            inv_tex_size: [1.0 / 64.0; 2],
            alpha_scale: 1.0,
            flags: 0,
            has_tex2: 0,
            scroll: [0.0; 2],
            blend4: 0,
            alpha_ref: 0.5,
            detail_mode: u32::MAX,
            detail_flags: 0,
            pad: 0,
            detail_uv: [1.0, 1.0, 0.0, 0.0],
            detail_tint: [1.0; 4],
            bump_uv: [1.0, 1.0, 0.0, 0.0],
            bump_scroll: [0.0; 2],
            bump_flags: 0,
            bump_layers: 0,
            self_illum: [1.0, 1.0, 1.0, 0.0],
            base_uv: [1.0, 1.0, 0.0, 0.0],
            ..MaterialUniform::default()
        };
    };
    let finite = |v: f32, default: f32| if v.is_finite() { v } else { default };
    let detail = d.detail.filter(|dd| dd.mode <= DetailDesc::MODE_TWO_TEXTURE_MASK);
    let inv = |v: f32| if v.is_finite() && v.abs() > 1e-6 { 1.0 / v } else { 1.0 / 512.0 };
    MaterialUniform {
        tint: srgba_to_linear(d.tint),
        inv_tex_size: [inv(d.tex_size[0]), inv(d.tex_size[1])],
        alpha_scale: if d.flags.contains(MaterialFlags::TOOL) { TOOL_ALPHA } else { 1.0 },
        flags: d.flags.0,
        has_tex2: u32::from(d.texture2.is_some() || d.texture3.is_some()),
        scroll: sanitize_scroll(d.scroll),
        blend4: u32::from(d.texture3.is_some()),
        alpha_ref: if d.alpha_ref.is_finite() { d.alpha_ref.clamp(0.0, 1.0) } else { 0.5 },
        detail_mode: detail.map_or(u32::MAX, |dd| dd.mode),
        detail_flags: detail.map_or(0, |dd| u32::from(dd.raw) | u32::from(dd.sets_alpha) << 1),
        pad: 0,
        detail_uv: detail.map_or([1.0, 1.0, 0.0, 0.0], |dd| {
            [finite(dd.uv[0], 1.0), finite(dd.uv[1], 1.0), finite(dd.uv[2], 0.0), finite(dd.uv[3], 0.0)]
        }),
        detail_tint: detail.map_or([1.0; 4], |dd| dd.tint_blend.map(|v| finite(v, 1.0))),
        bump_uv: d.bump.map_or([1.0, 1.0, 0.0, 0.0], |b| {
            let scale = |v: f32| if v.is_finite() && v.abs() > 1e-4 { v.clamp(-1e4, 1e4) } else { 1.0 };
            let offset = |v: f32| if v.is_finite() { v.clamp(-1e4, 1e4) } else { 0.0 };
            [scale(b.uv[0]), scale(b.uv[1]), offset(b.uv[2]), offset(b.uv[3])]
        }),
        bump_scroll: d.bump.map_or([0.0; 2], |b| sanitize_scroll(b.scroll)),
        bump_flags: d.bump.map_or(0, |b| {
            let four = b.blend4.is_some() && d.texture3.is_some();
            BUMP_ON
                | if b.ssbump { BUMP_SS } else { 0 }
                | if b.texture2.is_some() { BUMP_TWO } else { 0 }
                | if four { BUMP_FOUR } else { 0 }
                | if b.diffuse { 0 } else { BUMP_NO_DIFFUSE }
        }),
        bump_layers: d.bump.and_then(|b| b.blend4).map_or(0, pack_bump_layers),
        self_illum: self_illum_uniform(d.self_illum.as_ref()),
        base_uv: {
            let scale = |v: f32| if v.is_finite() && v.abs() > 1e-4 { v.clamp(-1e4, 1e4) } else { 1.0 };
            let offset = |v: f32| if v.is_finite() { v.clamp(-1e4, 1e4) } else { 0.0 };
            [scale(d.base_uv[0]), scale(d.base_uv[1]), offset(d.base_uv[2]), offset(d.base_uv[3])]
        },
        ..four_way_uniform(d.four_way.as_ref(), env_uniform(d, env_cube))
    }
}

fn four_way_uniform(f: Option<&FourWayDesc>, base: MaterialUniform) -> MaterialUniform {
    let f = f.copied().unwrap_or_default();
    let unit = |v: f32, default: f32| if v.is_finite() { v.clamp(-4.0, 4.0) } else { default };
    let scale = |v: f32| if v.is_finite() && v.abs() > 1e-4 { v.clamp(-1e4, 1e4) } else { 1.0 };
    let uv = |a: usize, b: usize| {
        [scale(f.uv_scale[a][0]), scale(f.uv_scale[a][1]), scale(f.uv_scale[b][0]), scale(f.uv_scale[b][1])]
    };
    MaterialUniform {
        fw_lum_start: f.lum.map(|r| unit(r[0], 0.0)),
        fw_lum_end: f.lum.map(|r| unit(r[1], 1.0)),
        fw_blend_start: f.blend.map(|r| unit(r[0], 0.0)),
        fw_blend_end: f.blend.map(|r| unit(r[1], 1.0)),
        fw_lum_factor: f.lum_blend_factor.map(|v| unit(v, 0.0)),
        fw_mod2x: f.modulate_2x.map(|m| if m { 1.0 } else { 0.0 }),
        fw_uv12: uv(0, 1),
        fw_uv34: uv(2, 3),
        ..base
    }
}

fn self_illum_uniform(s: Option<&SelfIllumDesc>) -> [f32; 4] {
    let Some(s) = s else { return [1.0, 1.0, 1.0, 0.0] };
    let t = s.tint.map(|v| if v.is_finite() { v.clamp(0.0, 64.0) } else { 1.0 });
    let mode = match (s.mask, s.mask_alpha) {
        (None, _) => 0.0,
        (Some(_), false) => 1.0,
        (Some(_), true) => 2.0,
    };
    [t[0], t[1], t[2], mode]
}

const ENV_ON: u32 = 1;
const ENV_MASK_RGB: u32 = 2;
const ENV_MASK_ALPHA: u32 = 4;
const ENV_BASE_INV: u32 = 8;
const ENV_BASE: u32 = 16;
const ENV_PHONG: u32 = 32;
const ENV_NO_TEX1: u32 = 64;
const ENV_NO_TEX2: u32 = 128;
const ENV_FRESNEL: u32 = 256;
const ENV_PARALLAX: u32 = 512;

fn env_uniform(d: &MaterialDesc, env_cube: u32) -> MaterialUniform {
    let Some(e) = d.envmap.filter(|_| d.water.is_none()) else { return MaterialUniform::default() };
    let finite = |v: f32, default: f32, lo: f32, hi: f32| if v.is_finite() { v.clamp(lo, hi) } else { default };
    let mut flags = ENV_ON;
    if e.mask.is_some() {
        flags |= if e.mask_alpha { ENV_MASK_ALPHA } else { ENV_MASK_RGB };
    }
    flags |= match e.alpha_mask {
        EnvmapAlphaMask::None => 0,
        EnvmapAlphaMask::InverseBase => ENV_BASE_INV,
        EnvmapAlphaMask::Base => ENV_BASE,
    };
    let phong = finite(e.phong_fresnel, 0.0, 0.0, 1.0);
    if phong > 0.0 {
        flags |= ENV_PHONG;
    }
    if e.no_envmap[0] {
        flags |= ENV_NO_TEX1;
    }
    if e.no_envmap[1] {
        flags |= ENV_NO_TEX2;
    }
    let fresnel = finite(e.fresnel, 1.0, 0.0, 1.0);
    if fresnel < 1.0 {
        flags |= ENV_FRESNEL;
    }
    if e.parallax {
        flags |= ENV_PARALLAX;
    }
    let tint = e.tint.map(|v| finite(v, 1.0, 0.0, 64.0));
    let sat = e.saturation.map(|v| finite(v, 1.0, -8.0, 8.0));
    let uv = e.mask_uv;
    let scale = |v: f32| if v.is_finite() && v.abs() > 1e-4 { v.clamp(-1e4, 1e4) } else { 1.0 };
    let offset = |v: f32| if v.is_finite() { v.clamp(-1e4, 1e4) } else { 0.0 };
    let ranges = e.fresnel_ranges.map(|v| finite(v, 0.5, 0.0, 64.0));
    MaterialUniform {
        env_flags: flags,
        env_cube,
        env_pad: [0; 2],
        env_tint: [tint[0], tint[1], tint[2], finite(e.contrast, 0.0, 0.0, 1.0)],
        env_sat: [sat[0], sat[1], sat[2], finite(e.light_scale, 0.0, 0.0, 1.0)],
        env_mask_uv: [scale(uv[0]), scale(uv[1]), offset(uv[2]), offset(uv[3])],
        env_fresnel: [fresnel, phong, 0.0, 0.0],
        env_ranges: [ranges[0], ranges[1], ranges[2], 0.0],
        ..MaterialUniform::default()
    }
}

const BUMP_ON: u32 = 1;
const BUMP_SS: u32 = 2;
const BUMP_TWO: u32 = 4;
const BUMP_FOUR: u32 = 8;
const BUMP_NO_DIFFUSE: u32 = 16;

fn pack_bump_layers(f: [f32; 4]) -> u32 {
    f.iter().enumerate().fold(0, |acc, (i, v)| {
        let v = if v.is_finite() { v.clamp(0.0, 1.0) } else { 0.0 };
        acc | (((v * 255.0).round() as u32) << (8 * i))
    })
}

fn sanitize_scroll(s: [f32; 2]) -> [f32; 2] {
    s.map(|v| if v.is_finite() { v.clamp(-100.0, 100.0) } else { 0.0 })
}

fn sanitize_transform(m: &[[f32; 4]; 4]) -> Mat4 {
    let m = Mat4::from_cols_array_2d(m);
    if m.is_finite() { m } else { Mat4::IDENTITY }
}

#[cfg(test)]
fn grid_uniform(grid: &GridParams, ppu: f32) -> ([f32; 4], [f32; 4]) {
    grid_uniform_min(grid, ppu, 4.0)
}

fn grid_uniform_min(grid: &GridParams, ppu: f32, min_px: f32) -> ([f32; 4], [f32; 4]) {
    let opacity = if grid.opacity.is_finite() { grid.opacity.clamp(0.0, 1.0) } else { 1.0 };
    let size = if grid.size.is_finite() { grid.size.clamp(1.0 / 64.0, 65536.0) } else { 16.0 };
    let every = grid.major_every.clamp(2, 1024) as f32;
    let min_px = if min_px.is_finite() { min_px.clamp(2.0, 64.0) } else { 4.0 };
    let fade = |px: f32| ((px - min_px) / min_px).clamp(0.0, 1.0);
    let cell = if grid.cells { opacity } else { 0.0 };
    let minor_alpha = fade(size * ppu) * cell;
    let mut major = size * every;
    while major * ppu < min_px && major < 1.0e7 {
        major *= every;
    }
    let major_alpha = cell * (0.5 + 0.5 * fade(major * ppu));
    let a1024 = if grid.show_1024 && 1024.0 * ppu >= 4.0 { opacity } else { 0.0 };
    ([size, minor_alpha, major, major_alpha], [a1024, opacity, ppu, 0.0])
}

fn mode_code(mode: RenderMode) -> u32 {
    match mode {
        RenderMode::Textured => 0,
        RenderMode::Flat => 1,
        RenderMode::Wireframe => 2,
        RenderMode::TexturedWire => 3,
        RenderMode::Lit => 4,
        RenderMode::Unshaded => 5,
        RenderMode::SmoothingGroups => 6,
    }
}

fn unique_edges(verts: &[gpu::GpuVertex], indices: &[u32]) -> Vec<u32> {
    let mut first_at: HashMap<[u32; 3], u32> = HashMap::with_capacity(verts.len());
    let canon: Vec<u32> =
        verts.iter().enumerate().map(|(i, v)| *first_at.entry(v.pos.map(f32::to_bits)).or_insert(i as u32)).collect();
    let mut seen: ahash::HashSet<(u32, u32)> = ahash::HashSet::default();
    let mut out = Vec::new();
    for tri in indices.chunks_exact(3) {
        for (a, b) in [(tri[0], tri[1]), (tri[1], tri[2]), (tri[2], tri[0])] {
            let (Some(&ca), Some(&cb)) = (canon.get(a as usize), canon.get(b as usize)) else { continue };
            if ca == cb {
                continue;
            }
            let key = (ca.min(cb), ca.max(cb));
            if seen.insert(key) {
                out.extend_from_slice(&[key.0, key.1]);
            }
        }
    }
    out
}

const OPT_WALKABLE: u32 = 1;
const OPT_FOG: u32 = 2;
const OPT_GRID_3D: u32 = 8;
const OPT_SKY_PASS: u32 = 16;
const OPT_MODEL_FADE: u32 = 32;
const OPT_NEUTRAL_FLAT: u32 = 64;
const OPT_DETAIL_SELECTED: u32 = 128;
const OPT_DETAIL_NONE: u32 = 256;
const OPT_WALK_MARKS: u32 = 512;
const OPT_SOURCE_2D: u32 = 1024;
const OPT_GAME_WIRE: u32 = 2048;
const OPT_COMPILED_LIGHT: u32 = 4096;
const OPT_COMPILED_FULLBRIGHT: u32 = 8192;
const OPT_NORMAL_MAPS: u32 = 16384;
const OPT_CUBEMAPS: u32 = 32768;
const OPT_PROBE: u32 = 65536;
pub(crate) const OPT_SHADOW_PASS: u32 = 1 << 17;
const OPT_MODEL_LIGHT: u32 = 1 << 18;
pub(crate) const OPT_SUN_PASS: u32 = 1 << 19;
pub(crate) const OPT_SKY_CUT: u32 = 1 << 20;
pub(crate) const OPT_SKY_FACES: u32 = 1 << 21;
const OPT_REFLECT_WORLD: u32 = 1 << 22;
const OPT_REFLECT_SKY: u32 = 1 << 23;
const OPT_CLASSIC_COLORS: u32 = 1 << 24;
const OPT_LIGHT_BACKGROUND: u32 = 1 << 25;
const OPT_SKY_PAINT: u32 = 1 << 26;
const OPT_HDR_TINT: u32 = 1 << 27;
const OPT_MODEL_LODS: u32 = 1 << 28;
const VERTEX_DOT_PX: u32 = 3;

struct OptionsUniform {
    opts: [u32; 4],
    grid3d: [f32; 4],
    fog_a: [f32; 4],
    fog_b: [f32; 4],
    fog_c: [f32; 4],
    sky: [f32; 4],
    sky3d: [f32; 4],
    sky_uv: [[f32; 4]; 6],
    sky_paint: [[f32; 4]; 6],
    far_z: f32,
}

fn sky_paint_uniform(p: &SkyPaint, ldr: bool) -> [[f32; 4]; 6] {
    let c3 = |c: [f32; 3]| c.map(|v| finite_or(v, 0.0).clamp(0.0, 64.0));
    let (top, bottom, sun_color, dusk_color) = (c3(p.top), c3(p.bottom), c3(p.sun_color), c3(p.dusk_color));
    let hdr = if ldr { 1.0 } else { finite_or(p.hdr_scale, 0.66).clamp(0.0, 64.0) };
    let dir = Vec3::from(p.sun_dir).normalize_or(Vec3::X);
    let dir = if dir.is_finite() { dir } else { Vec3::X };
    let sun_size = finite_or(p.sun_size, 0.0);
    let sun_exp = if sun_size > 0.0 { (20.0 / sun_size).clamp(0.0, 4096.0) } else { 0.0 };
    let dusk_exp = 2.0 - finite_or(p.dusk_scale, 1.0).clamp(0.0, 16.0);
    let dusk_w = 0.5 * finite_or(p.dusk_intensity, 0.0).clamp(0.0, 64.0);
    [
        [top[0], top[1], top[2], finite_or(p.fade_bias, 1.0).clamp(0.001, 1024.0)],
        [bottom[0], bottom[1], bottom[2], hdr],
        [dir.x, dir.y, dir.z, sun_exp],
        [sun_color[0], sun_color[1], sun_color[2], dusk_exp],
        [dusk_color[0], dusk_color[1], dusk_color[2], dusk_w],
        star_uniform(p),
    ]
}

const STAR_GAIN: f32 = 1.29;
const STAR_LAYER_GAIN: f32 = 1.5;

fn star_uniform(p: &SkyPaint) -> [f32; 4] {
    if !p.draw_stars || p.star_texture.is_none() {
        return [0.0; 4];
    }
    let scale = finite_or(p.star_scale, 0.5).clamp(0.01, 1024.0);
    let fade = finite_or(p.star_fade, 1.0).clamp(0.01, 1024.0);
    let layers = p.star_layers.clamp(1, 3);
    let gain = STAR_GAIN * STAR_LAYER_GAIN.powi(layers as i32 - 1);
    [1.0 / scale, 1.0 / fade, finite_or(p.star_speed, 0.0).clamp(-64.0, 64.0), gain]
}

fn finite_or(v: f32, fallback: f32) -> f32 {
    if v.is_finite() { v } else { fallback }
}

fn light_background(bg: [f32; 3]) -> bool {
    let [r, g, b] = bg.map(|c| if c.is_finite() { c.clamp(0.0, 1.0) } else { 0.0 });
    0.2126 * r + 0.7152 * g + 0.0722 * b > 0.5
}

pub(crate) fn fog_view_color(f: &Fog, forward: Vec3) -> ([f32; 3], Vec3) {
    let c = |c: [f32; 3]| c.map(|v| finite_or(v, 1.0).clamp(0.0, 1.0));
    let dir = Vec3::from(f.dir).normalize_or(Vec3::X);
    let dir = if dir.is_finite() { dir } else { Vec3::X };
    let (primary, secondary) = (c(f.color), c(f.color2));
    if !f.blend {
        return (primary, dir);
    }
    let fwd = forward.normalize_or(Vec3::X);
    let t = (0.5 * fwd.dot(dir) + 0.5).clamp(0.0, 1.0);
    (std::array::from_fn(|i| t * primary[i] + (1.0 - t) * secondary[i]), dir)
}

fn options_uniform(cam: &Camera, o: &ViewOptions, env: &Environment, sky_pass: bool, ldr_sky: bool) -> OptionsUniform {
    let mut bits = 0u32;
    let mut grid3d = [0.0f32; 4];
    let (mut fog_a, mut fog_b, mut fog_c) = ([0.0f32; 4], [0.0f32; 4], [1.0f32, 0.0, 0.0, 1.0]);
    let mut far_z = 0.0f32;
    if o.walk_marks {
        bits |= OPT_WALK_MARKS;
    }
    if o.classic_colors {
        bits |= OPT_CLASSIC_COLORS;
    }
    if o.model_lods && !cam.is_ortho {
        bits |= OPT_MODEL_LODS;
    }
    if cam.is_ortho {
        match o.detail_2d {
            Detail2d::All => {}
            Detail2d::Selected => bits |= OPT_DETAIL_SELECTED,
            Detail2d::None => bits |= OPT_DETAIL_NONE,
        }
        if o.source_brush_2d {
            bits |= OPT_SOURCE_2D;
        }
        let px64 = 64.0 * cam.ppu;
        let hl64 = if o.grid_highlight_64 { ((px64 - 6.0) / 6.0).clamp(0.0, 1.0) } else { 0.0 };
        grid3d = [hl64, if o.grid_dotted { 1.0 } else { 0.0 }, 0.0, finite_or(o.fill_selected_2d, 0.0).clamp(0.0, 1.0)];
    } else {
        if o.walkable {
            bits |= OPT_WALKABLE;
        }
        if o.model_fade {
            bits |= OPT_MODEL_FADE;
        }
        if sky_pass {
            bits |= OPT_SKY_PASS;
        }
        if o.game_wireframe > 0 {
            bits |= OPT_GAME_WIRE;
        }
        if o.normal_maps {
            bits |= OPT_NORMAL_MAPS;
        }
        if o.cubemaps {
            bits |= OPT_CUBEMAPS;
        }
        if let Some(g) = o.grid_3d.filter(|_| !sky_pass) {
            let size = finite_or(g.size, 16.0).clamp(1.0 / 16.0, 65536.0);
            bits |= OPT_GRID_3D;
            grid3d = [
                size,
                g.major_every.clamp(1, 1024) as f32,
                size * finite_or(g.fade_cells, 64.0).clamp(4.0, 4096.0),
                finite_or(g.opacity, 0.45).clamp(0.0, 1.0),
            ];
        }
        let fog = if sky_pass { env.sky_fog } else { env.fog };
        if let Some(f) = fog.filter(|_| o.show_fog) {
            let start = finite_or(f.start, 0.0).clamp(-1.0e6, 1.0e6);
            let end = finite_or(f.end, start + 1.0).max(start + 1.0);
            let (gamma, dir) = fog_view_color(&f, cam.forward);
            let [r, g, b] = gamma.map(srgb_to_linear);
            bits |= OPT_FOG;
            fog_a = [gamma[0], gamma[1], gamma[2], finite_or(f.max_density, 1.0).clamp(0.0, 1.0)];
            fog_b = [r, g, b, start];
            fog_c = [dir.x, dir.y, dir.z, end];
            if !sky_pass {
                far_z = finite_or(f.far_z, 0.0).max(0.0);
            }
        }
    }
    let sprite_px = finite_or(o.sprites_2d_px, 0.0).clamp(0.0, 256.0) as u32;
    let yaw = env.sky.map_or(0.0, |s| finite_or(s.yaw_deg, 0.0)).to_radians();
    let model_distance = finite_or(o.model_distance, 0.0).max(0.0);
    let sky3d = match env.sky_3d {
        Some(s) if s.scale.is_finite() && s.scale >= 1.0 && s.origin.iter().all(|v| v.is_finite()) => {
            [s.origin[0], s.origin[1], s.origin[2], 1.0 / s.scale]
        }
        _ => [0.0, 0.0, 0.0, 1.0],
    };
    let paint = env.sky_paint.filter(|_| o.show_sky && !cam.is_ortho);
    if paint.is_some() {
        bits |= OPT_SKY_PAINT;
    }
    let sky_paint = paint.map_or([[0.0; 4]; 6], |p| sky_paint_uniform(&p, ldr_sky));
    let sky_uv = env.sky.map_or([SkyView::IDENTITY_UV; 6], |s| {
        s.uv.map(|t| {
            if t.iter().all(|v| v.is_finite()) && t[0].abs() > 1e-4 && t[1].abs() > 1e-4 {
                t
            } else {
                SkyView::IDENTITY_UV
            }
        })
    });
    OptionsUniform {
        opts: [bits, sprite_px, VERTEX_DOT_PX, 0],
        grid3d,
        fog_a,
        fog_b,
        fog_c,
        sky: [yaw.cos(), yaw.sin(), finite_or(o.time, 0.0), model_distance],
        sky3d,
        sky_uv,
        sky_paint,
        far_z,
    }
}

impl Renderer {
    pub fn new(device: &wgpu::Device, queue: &wgpu::Queue) -> Self {
        let device = device.clone();
        let queue = queue.clone();
        let limits = device.limits();
        let supports_bc = device.features().contains(wgpu::Features::TEXTURE_COMPRESSION_BC);
        let shared = Shared::new(&device);
        let sampler = device.create_sampler(&wgpu::SamplerDescriptor {
            label: Some("hp-render sampler"),
            address_mode_u: wgpu::AddressMode::Repeat,
            address_mode_v: wgpu::AddressMode::Repeat,
            address_mode_w: wgpu::AddressMode::Repeat,
            mag_filter: wgpu::FilterMode::Linear,
            min_filter: wgpu::FilterMode::Linear,
            mipmap_filter: wgpu::MipmapFilterMode::Linear,
            anisotropy_clamp: 8,
            ..Default::default()
        });
        let sampler_clamp = device.create_sampler(&wgpu::SamplerDescriptor {
            label: Some("hp-render sampler clamp"),
            address_mode_u: wgpu::AddressMode::ClampToEdge,
            address_mode_v: wgpu::AddressMode::ClampToEdge,
            address_mode_w: wgpu::AddressMode::ClampToEdge,
            mag_filter: wgpu::FilterMode::Linear,
            min_filter: wgpu::FilterMode::Linear,
            mipmap_filter: wgpu::MipmapFilterMode::Linear,
            ..Default::default()
        });
        let max_buffer = limits.max_buffer_size;
        let max_storage = limits.max_storage_buffer_binding_size.min(max_buffer);
        let elems = |size: u64| (max_buffer / size).min(u64::from(u32::MAX)) as u32;
        let scene = Scene::new([elems(ARENA_ELEM[0]), elems(ARENA_ELEM[1]), elems(ARENA_ELEM[2])]);
        let vb = wgpu::BufferUsages::VERTEX;
        let arenas = [
            GrowBuffer::new(&device, "hp-render mesh vertices", vb, 1 << 20),
            GrowBuffer::new(&device, "hp-render indices", wgpu::BufferUsages::INDEX, 1 << 20),
            GrowBuffer::new(&device, "hp-render edge vertices", vb, 1 << 19),
        ];
        let storage = wgpu::BufferUsages::STORAGE;
        let states = GrowBuffer::new(&device, "hp-render object states", storage, 1 << 14);
        let fallback = device.create_texture(&wgpu::TextureDescriptor {
            label: Some("hp-render fallback target"),
            size: wgpu::Extent3d { width: 1, height: 1, depth_or_array_layers: 1 },
            mip_level_count: 1,
            sample_count: 1,
            dimension: wgpu::TextureDimension::D2,
            format: LDR_FORMAT,
            usage: wgpu::TextureUsages::RENDER_ATTACHMENT | wgpu::TextureUsages::TEXTURE_BINDING,
            view_formats: &[],
        });
        let shadow_dummy = device
            .create_texture(&wgpu::TextureDescriptor {
                label: Some("hp-render shadow dummy"),
                size: wgpu::Extent3d { width: 1, height: 1, depth_or_array_layers: 1 },
                mip_level_count: 1,
                sample_count: 1,
                dimension: wgpu::TextureDimension::D2,
                format: DEPTH_FORMAT,
                usage: wgpu::TextureUsages::TEXTURE_BINDING,
                view_formats: &[],
            })
            .create_view(&wgpu::TextureViewDescriptor {
                dimension: Some(wgpu::TextureViewDimension::D2Array),
                ..Default::default()
            });
        let shadow_sampler = device.create_sampler(&wgpu::SamplerDescriptor {
            label: Some("hp-render shadow sampler"),
            address_mode_u: wgpu::AddressMode::ClampToEdge,
            address_mode_v: wgpu::AddressMode::ClampToEdge,
            address_mode_w: wgpu::AddressMode::ClampToEdge,
            mag_filter: wgpu::FilterMode::Linear,
            min_filter: wgpu::FilterMode::Linear,
            compare: Some(wgpu::CompareFunction::LessEqual),
            ..Default::default()
        });
        let dummy_storage = device.create_buffer(&wgpu::BufferDescriptor {
            label: Some("hp-render dummy storage"),
            size: 16,
            usage: storage,
            mapped_at_creation: false,
        });
        let light_styles = compiled_light::light_styles_buffer(&device);
        let mut r = Self {
            supports_bc,
            max_tex_dim: limits.max_texture_dimension_2d,
            max_buffer,
            max_storage,
            shared,
            pipelines: [None, None],
            sampler,
            sampler_clamp,
            textures: HashMap::new(),
            next_tex: 3,
            mat_gpu: Vec::new(),
            scene,
            arenas,
            states,
            scene_gen: 1,
            models: HashMap::new(),
            next_model: 1,
            instances: GrowBuffer::new(&device, "hp-render instances", vb, 1 << 12),
            instance_count: 0,
            instance_groups: Vec::new(),
            instance_refs: Vec::new(),
            sprites: GrowBuffer::new(&device, "hp-render sprites", vb, 1 << 12),
            sprite_groups: Vec::new(),
            sprite_refs: Vec::new(),
            sprite_layers: std::collections::BTreeMap::new(),
            lights_buf: GrowBuffer::new(&device, "hp-render lights", storage, size_of::<LightGpu>() as u64),
            lights_cpu: Vec::new(),
            dir_lights: 0,
            lights_left_out: 0,
            crowded_clusters: 0,
            crowded_logged: 0,
            sun: SunParams::default(),
            sky_map: None,
            ambient: [0.0; 3],
            lights_gen: 1,
            views: HashMap::new(),
            next_generation: 1,
            fallback_view: fallback.create_view(&wgpu::TextureViewDescriptor::default()),
            overlay_scratch: Vec::new(),
            overlay_bytes: Vec::new(),
            retained: HashMap::new(),
            retained_uploads: 0,
            staging: memory::StagingPacer::default(),
            prewarm: None,
            shadow: None,
            control_shadow: None,
            shadow_dummy_array: shadow_dummy,
            sky_layers: [None, None],
            sky_signature: (0, 0),
            shadow_sampler,
            dummy_storage,
            light_styles,
            shadow_epoch: 1,
            sun_fit: None,
            sun_cam: None,
            sun_followup: false,
            render_serial: 0,
            instance_bounds: None,
            instance_boxes: Vec::new(),
            instance_data: Vec::new(),
            instance_pool: instance_cull::InstancePool::new(&device),
            instances_dirty: false,
            content_gen: 1,
            game_hidden: ahash::AHashSet::default(),
            compiled_objects: ahash::AHashSet::default(),
            play_hidden: ahash::AHashSet::default(),
            play_base: ahash::AHashMap::default(),
            view_options: HashMap::new(),
            static_lines: std::collections::BTreeMap::new(),
            sky_bind: None,
            hdr_sky: None,
            tone: hdr::Tone::default(),
            compiled_sky: Default::default(),
            hooks: Vec::new(),
            opaque: opaque::OpaquePass::new(&device),
            lightmap: lightmap::Lightmaps::default(),
            compiled: compiled_light::CompiledLight::default(),
            model_lights: compiled_light::ModelLights::default(),
            environment: Environment::default(),
            sel_chunks: ((0, 0), Vec::new()),
            translucent_instances: false,
            instance_list: Vec::new(),
            occ_pipes: None,
            outline_pipes: None,
            bg_pipes: None,
            bg_images: Vec::new(),
            water: None,
            fog_volumes: Vec::new(),
            fog_volume_gpu: None,
            sun_glow: None,
            sun_glow_gpu: None,
            game_outlines: None,
            detail: detail_sprites::DetailSprites::default(),
            particles: particles::Particles::default(),
            preview_particles: particles::Particles::default(),
            cubes: cubemaps::Cubes::new(&device),
            probing: false,
            quality: Quality::default(),
            profiler: profile::GpuProfiler::new(&device, &queue),
            frame_cpu: profile::CpuTimes::default(),
            reflection_cpu_ms: 0.0,
            reflection_active: false,
            reflecting: None,
            water_reflect: None,
            local_shadows: local_shadow::LocalShadows::new(&device),
            indirect: indirect::Indirect::new(),
            projectors: projector::Projectors::new(&device, &queue),
            texture_limit: limits.max_texture_dimension_2d,
            upload_failed: false,
            device,
            queue,
        };
        r.hooks = extra_passes::hooks(&r);
        for (id, up) in [
            (TexId::MISSING, textures::builtin_missing()),
            (TexId::LOADING, textures::builtin_loading()),
            (TexId::WHITE, textures::builtin_white()),
            (TexId::FLAT_NORMAL, textures::builtin_flat_normal()),
        ] {
            if let Ok(p) = textures::prepare(&up, false, r.max_tex_dim) {
                let entry = textures::upload(&r.device, &r.queue, &p, "hp-render builtin");
                r.textures.insert(id.0, entry);
            }
        }
        let mut enc =
            r.device.create_command_encoder(&wgpu::CommandEncoderDescriptor { label: Some("hp-render fallback") });
        r.opaque.record(&mut enc, &r.fallback_view);
        r.queue.submit([enc.finish()]);
        r
    }

    pub fn supports_bc(&self) -> bool {
        self.supports_bc
    }

    pub fn content_generation(&self) -> u64 {
        self.content_gen
    }

    pub fn bump_content_generation(&mut self) {
        self.content_gen += 1;
    }

    pub fn device(&self) -> &wgpu::Device {
        &self.device
    }

    pub fn queue(&self) -> &wgpu::Queue {
        &self.queue
    }

    pub fn add_pass_hook(&mut self, hook: Box<dyn PassHook>) {
        self.hooks.push(hook);
        self.content_gen += 1;
    }

    pub fn pass_hook_names(&self) -> Vec<&'static str> {
        self.hooks.iter().map(|h| h.name()).collect()
    }

    pub fn set_view_options(&mut self, view_id: u32, options: ViewOptions) {
        if self.view_options.get(&view_id) != Some(&options) {
            self.view_options.insert(view_id, options);
            self.content_gen += 1;
        }
    }

    pub fn view_options(&self, view_id: u32) -> ViewOptions {
        self.view_options.get(&view_id).copied().unwrap_or_default()
    }

    pub fn set_environment(&mut self, env: Environment) {
        if self.environment != env {
            self.environment = env;
            self.content_gen += 1;
        }
    }

    pub fn environment(&self) -> Environment {
        self.environment
    }

    pub fn create_texture(&mut self, up: &TextureUpload) -> TexId {
        match self.try_create_texture(up) {
            Ok(id) => id,
            Err(e) => {
                log::warn!("hp-render: {e}");
                TexId::MISSING
            }
        }
    }

    pub fn try_create_texture(&mut self, up: &TextureUpload) -> Result<TexId, RenderError> {
        let mut p = textures::prepare(up, self.supports_bc, self.max_tex_dim)?;
        textures::limit_size(&mut p, self.texture_limit);
        let (entry, oom) =
            memory::guarded(&self.device, || textures::upload(&self.device, &self.queue, &p, "hp-render texture"));
        if oom {
            return Err(RenderError::OutOfMemory);
        }
        self.staging.wrote(&self.device, &self.queue, entry.bytes);
        self.content_gen += 1;
        let id = self.next_tex;
        self.next_tex = self.next_tex.checked_add(1).filter(|v| *v != TexId::FLAT_NORMAL.0).unwrap_or(3);
        self.textures.insert(id, entry);
        Ok(TexId(id))
    }

    pub fn set_texture_limit(&mut self, size: u32) {
        self.texture_limit = size.clamp(64, self.max_tex_dim);
    }

    pub fn texture_limit(&self) -> u32 {
        self.texture_limit
    }

    pub fn max_texture_size(&self) -> u32 {
        self.max_tex_dim
    }

    pub fn take_upload_failed(&mut self) -> bool {
        std::mem::take(&mut self.upload_failed)
    }

    pub fn memory(&self) -> MemoryReport {
        let geometry = self.arenas.iter().map(|a| a.size).sum::<u64>()
            + self.states.size
            + self.instances.size
            + self.sprites.size
            + self.lights_buf.size;
        let textures = self.textures.values().map(|t| t.bytes).sum::<u64>();
        let models = self.models.values().map(|m| m.bytes).sum::<u64>();
        let lightmaps = self.lightmap_stats().bytes;
        let compiled = self.compiled_lighting().map_or(0, |c| c.bytes);
        let total = self.stats().approx_gpu_bytes;
        let other = total.saturating_sub(geometry + textures + models);
        MemoryReport { geometry, textures, models, lightmaps, compiled, other, total: total + lightmaps + compiled }
    }

    pub fn texture_view(&self, id: TexId) -> Option<&wgpu::TextureView> {
        self.textures.get(&id.0).map(|t| &t.view)
    }

    pub fn destroy_texture(&mut self, id: TexId) {
        if id.is_builtin() || self.textures.remove(&id.0).is_none() {
            return;
        }
        self.content_gen += 1;
        if self.sky_bind.as_ref().is_some_and(|(faces, _)| faces.contains(&Some(id))) {
            self.sky_bind = None;
        }
        self.forget_bg_texture(id);
        self.forget_cube_texture(id);
        for i in 0..self.mat_gpu.len() {
            let uses = self.mat_gpu.get(i).is_some_and(|g| g.texs.contains(&Some(id)));
            if uses {
                self.refresh_material_bind_group(i);
            }
            let desc = self.scene.materials.get(i).and_then(|m| m.desc);
            if desc.and_then(|d| d.envmap).and_then(|e| e.texture) == Some(id) {
                let cube = self.material_cube(desc.as_ref());
                if let Some(g) = self.mat_gpu.get(i) {
                    self.queue.write_buffer(&g.uniform, 0, bytemuck::bytes_of(&material_uniform(desc.as_ref(), cube)));
                }
            }
        }
    }

    fn texture_view_or_missing(&self, id: TexId) -> Option<&wgpu::TextureView> {
        self.textures.get(&id.0).or_else(|| self.textures.get(&TexId::MISSING.0)).map(|t| &t.view)
    }

    fn resolve_textures(&self, desc: Option<&MaterialDesc>) -> MatTextures {
        let exists = |t: TexId| if self.textures.contains_key(&t.0) { t } else { TexId::MISSING };
        match desc {
            None => [Some(TexId::MISSING), None, None, None, None, None, None, None, None],
            Some(d) => [
                Some(d.texture.map_or(TexId::WHITE, exists)),
                d.texture2.map(exists),
                d.texture3.map(exists),
                d.texture4.map(exists),
                d.detail.map(|dd| if self.textures.contains_key(&dd.texture.0) { dd.texture } else { TexId::WHITE }),
                d.bump.map(|b| if self.textures.contains_key(&b.texture.0) { b.texture } else { TexId::FLAT_NORMAL }),
                d.bump
                    .and_then(|b| b.texture2)
                    .map(|t| if self.textures.contains_key(&t.0) { t } else { TexId::FLAT_NORMAL }),
                d.envmap.and_then(|e| e.mask).map(|t| if self.textures.contains_key(&t.0) { t } else { TexId::WHITE }),
                d.self_illum
                    .and_then(|s| s.mask)
                    .map(|t| if self.textures.contains_key(&t.0) { t } else { TexId::WHITE }),
            ],
        }
    }

    fn material_bind_group(&self, uniform: &wgpu::Buffer, texs: &MatTextures) -> Option<wgpu::BindGroup> {
        let view = |i: usize| self.texture_view_or_missing(texs[i].unwrap_or(TexId::WHITE));
        let normal = |i: usize| self.texture_view_or_missing(texs[i].unwrap_or(TexId::FLAT_NORMAL));
        let (v1, v2, v3, v4, v5) = (view(0)?, view(1)?, view(2)?, view(3)?, view(4)?);
        let (v6, v7) = (normal(5)?, normal(6)?);
        let (v8, v9) = (view(7)?, view(8)?);
        Some(self.device.create_bind_group(&wgpu::BindGroupDescriptor {
            label: Some("hp-render material"),
            layout: &self.shared.material_bgl,
            entries: &[
                wgpu::BindGroupEntry { binding: 0, resource: wgpu::BindingResource::TextureView(v1) },
                wgpu::BindGroupEntry { binding: 1, resource: wgpu::BindingResource::TextureView(v2) },
                wgpu::BindGroupEntry { binding: 2, resource: uniform.as_entire_binding() },
                wgpu::BindGroupEntry { binding: 3, resource: wgpu::BindingResource::TextureView(v3) },
                wgpu::BindGroupEntry { binding: 4, resource: wgpu::BindingResource::TextureView(v4) },
                wgpu::BindGroupEntry { binding: 5, resource: wgpu::BindingResource::TextureView(v5) },
                wgpu::BindGroupEntry { binding: 6, resource: wgpu::BindingResource::TextureView(v6) },
                wgpu::BindGroupEntry { binding: 7, resource: wgpu::BindingResource::TextureView(v7) },
                wgpu::BindGroupEntry { binding: 8, resource: wgpu::BindingResource::TextureView(v8) },
                wgpu::BindGroupEntry { binding: 9, resource: wgpu::BindingResource::TextureView(v9) },
            ],
        }))
    }

    fn refresh_material_bind_group(&mut self, i: usize) {
        let desc = self.scene.materials.get(i).and_then(|m| m.desc);
        let texs = self.resolve_textures(desc.as_ref());
        let Some(g) = self.mat_gpu.get(i) else { return };
        if let Some(bg) = self.material_bind_group(&g.uniform, &texs)
            && let Some(g) = self.mat_gpu.get_mut(i)
        {
            g.bind_group = bg;
            g.texs = texs;
        }
    }

    fn sync_materials(&mut self) {
        while self.mat_gpu.len() < self.scene.materials.len() {
            let i = self.mat_gpu.len();
            let desc = self.scene.materials.get(i).and_then(|m| m.desc);
            let uniform = self.device.create_buffer(&wgpu::BufferDescriptor {
                label: Some("hp-render material uniform"),
                size: size_of::<MaterialUniform>() as u64,
                usage: wgpu::BufferUsages::UNIFORM | wgpu::BufferUsages::COPY_DST,
                mapped_at_creation: false,
            });
            let cube = self.material_cube(desc.as_ref());
            self.queue.write_buffer(&uniform, 0, bytemuck::bytes_of(&material_uniform(desc.as_ref(), cube)));
            let texs = self.resolve_textures(desc.as_ref());
            let Some(bind_group) = self.material_bind_group(&uniform, &texs) else {
                log::error!("hp-render: built-in textures are missing");
                return;
            };
            self.mat_gpu.push(MaterialGpu { uniform, bind_group, texs });
        }
    }

    pub fn set_material(&mut self, id: MaterialId, desc: &MaterialDesc) {
        let i = self.scene.material_slot(id) as usize;
        let unreflected = |d: &MaterialDesc| MaterialDesc { envmap: None, ..*d };
        let old = self.scene.materials.get(i).and_then(|m| m.desc);
        if old.map(|o| unreflected(&o)) != Some(unreflected(desc)) {
            self.cubes.epoch += 1;
        }
        if let Some(m) = self.scene.materials.get_mut(i) {
            m.desc = Some(*desc);
        }
        self.sync_materials();
        self.shadow_epoch += 1;
        self.content_gen += 1;
        let texs = self.resolve_textures(Some(desc));
        let cube = self.material_cube(Some(desc));
        let Some(g) = self.mat_gpu.get(i) else { return };
        self.queue.write_buffer(&g.uniform, 0, bytemuck::bytes_of(&material_uniform(Some(desc), cube)));
        if g.texs != texs {
            self.refresh_material_bind_group(i);
        }
    }

    pub fn view_shows_animation(&self, view_id: u32) -> bool {
        self.views.get(&view_id).is_some_and(|v| v.animated)
    }

    fn shows_animation(&self, lists: &[Vec<ChunkDraw>; MESH_PASSES]) -> bool {
        let scrolls = |i: u32| {
            self.scene
                .materials
                .get(i as usize)
                .and_then(|m| m.desc)
                .is_some_and(|d| sanitize_scroll(d.scroll) != [0.0; 2])
        };
        lists.iter().flatten().any(|d| scrolls(d.material))
            || (self.instance_count > 0 && self.models.values().flat_map(|m| &m.parts).any(|p| scrolls(p.material)))
    }

    pub fn has_animated_materials(&self) -> bool {
        let scrolls =
            |i: u32| {
                self.scene.materials.get(i as usize).and_then(|m| m.desc).is_some_and(|d| {
                    sanitize_scroll(d.scroll) != [0.0; 2] || d.water.is_some_and(|w| water::animates(&w))
                })
            };
        let in_world = self.scene.materials.iter().enumerate().any(|(i, m)| !m.chunks.is_empty() && scrolls(i as u32));
        in_world
            || (self.instance_count > 0 && self.models.values().flat_map(|m| &m.parts).any(|p| scrolls(p.material)))
    }

    pub fn material(&self, id: MaterialId) -> Option<MaterialDesc> {
        self.scene.find_material(id).and_then(|i| self.scene.materials.get(i as usize)).and_then(|m| m.desc)
    }

    pub fn upsert_object(&mut self, id: ObjectId, mesh: ObjectMesh) {
        self.scene.upsert(id, mesh);
        self.lightmap.object_changed(id);
        self.content_gen += 1;
        self.cubes.epoch += 1;
    }

    pub fn hint_material_triangles(&mut self, hints: &[(MaterialId, u64)]) {
        for &(id, tris) in hints {
            self.scene.hint_material_triangles(id, tris);
        }
        self.sync_materials();
    }

    pub fn remove_object(&mut self, id: ObjectId) {
        self.scene.remove(id);
        self.lightmap.object_changed(id);
        self.content_gen += 1;
        self.cubes.epoch += 1;
    }

    pub fn clear_objects(&mut self) {
        self.scene.clear();
        self.lightmap.objects_cleared();
        self.shadow_epoch += 1;
        self.content_gen += 1;
        self.cubes.epoch += 1;
    }

    pub fn object_count(&self) -> usize {
        self.scene.objects.len()
    }

    pub fn set_object_flags(&mut self, id: ObjectId, flags: ObjectFlags) {
        let flags = if self.game_hidden.contains(&id) { flags | ObjectFlags::GAME_HIDDEN } else { flags };
        let flags = if self.compiled_objects.contains(&id) { flags | ObjectFlags::COMPILED_LIGHT } else { flags };
        let flags = if self.play_hidden.contains(&id) {
            self.play_base.insert(id, flags);
            flags | ObjectFlags::HIDDEN
        } else {
            flags
        };
        if self.scene.flags(id) != Some(flags) {
            self.content_gen += 1;
            let old = self.scene.flags(id).unwrap_or_default();
            let seen = ObjectFlags::HIDDEN | ObjectFlags::NO_3D;
            if (old.0 ^ flags.0) & seen.0 != 0 && !flags.contains(ObjectFlags::COMPILED_LIGHT) {
                self.cubes.epoch += 1;
            }
            if (old.0 ^ flags.0) & ObjectFlags::HULL_SHADOW.0 != 0 && !self.instance_list.is_empty() {
                self.instances_dirty = true;
            }
        }
        self.scene.set_flags(id, flags);
    }

    pub fn set_object_tint(&mut self, id: ObjectId, rgba: [f32; 4]) {
        let packed = pack_rgba(rgba.map(|c| if c.is_finite() { c } else { 1.0 }));
        if self.scene.tint(id) != Some(packed) {
            self.content_gen += 1;
            self.shadow_epoch += 1;
            self.cubes.epoch += 1;
        }
        self.scene.set_tint(id, packed);
    }

    pub fn object_tint(&self, id: ObjectId) -> Option<[f32; 4]> {
        self.scene.tint(id).map(|t| [0, 8, 16, 24].map(|s| ((t >> s) & 0xff) as f32 / 255.0))
    }

    pub fn object_flags(&self, id: ObjectId) -> Option<ObjectFlags> {
        self.scene.flags(id)
    }

    pub fn set_object_wire_color(&mut self, id: ObjectId, rgb: [u8; 3]) {
        self.content_gen += 1;
        self.scene.set_wire_color(id, rgb);
    }

    pub fn set_object_wire_colors(&mut self, id: ObjectId, rgb: [u8; 3], classic: [u8; 3]) {
        self.content_gen += 1;
        self.scene.set_wire_color(id, rgb);
        self.scene.set_classic_color(id, (classic != rgb).then_some(classic));
    }

    pub fn object_wire_colors(&self, id: ObjectId) -> Option<(u32, u32)> {
        let color = self.scene.wire_color(id)?;
        Some((color, self.scene.classic_color(id).unwrap_or(0)))
    }

    pub fn flush(&mut self) {
        self.flush_with(true);
    }

    pub fn flush_on_this_thread(&mut self) {
        self.flush_with(false);
    }

    pub fn set_bulk_upload(&mut self, on: bool) {
        self.scene.bulk = on;
    }

    fn write_arena(&mut self, w: scene::Write) {
        let buf = &self.arenas[w.arena as usize];
        if w.data.is_empty() || w.offset + w.data.len() as u64 > buf.size {
            return;
        }
        self.queue.write_buffer(&buf.buffer, w.offset, &w.data);
        let bytes = w.data.len() as u64;
        drop(w);
        self.staging.wrote(&self.device, &self.queue, bytes);
    }

    fn flush_with(&mut self, parallel: bool) {
        if std::mem::take(&mut self.instances_dirty) {
            self.pack_instances();
        }
        let mut out = self.scene.flush_deferred(parallel);
        let rebuilt = std::mem::take(&mut out.rebuilt);
        if !out.writes.is_empty() || !rebuilt.is_empty() || !out.state_ranges.is_empty() {
            self.shadow_epoch += 1;
        }
        self.sync_materials();
        for ((buf, alloc), elem) in self.arenas.iter_mut().zip(&self.scene.arenas).zip(ARENA_ELEM) {
            let need = u64::from(alloc.end()) * elem;
            if buf.ensure(&self.device, &self.queue, need, self.max_buffer, true) == Grow::TooLarge {
                self.upload_failed = true;
            }
        }
        for batch in rebuilt.chunks(scene::ASSEMBLE_BATCH) {
            for w in self.scene.assemble_chunks(batch, parallel) {
                self.write_arena(w);
            }
        }
        for w in std::mem::take(&mut out.writes) {
            self.write_arena(w);
        }
        let state_size = size_of::<ObjState>() as u64;
        let need = u64::from(out.states_len) * state_size;
        match self.states.ensure(&self.device, &self.queue, need, self.max_storage, true) {
            Grow::Grown => self.scene_gen += 1,
            Grow::TooLarge => {
                self.upload_failed = true;
                log::error!("hp-render: the object state buffer cannot grow")
            }
            Grow::Fits => {}
        }
        for &(s, e) in &out.state_ranges {
            let data = self.scene.state_range(s, e);
            if !data.is_empty() && u64::from(e) * state_size <= self.states.size {
                self.queue.write_buffer(&self.states.buffer, u64::from(s) * state_size, bytemuck::cast_slice(data));
            }
        }
    }

    pub fn create_model(&mut self, parts: &[ModelPart]) -> ModelId {
        let model = self.build_model(&[(0.0, parts)]);
        self.insert_model(model)
    }

    pub fn create_model_lods(&mut self, lods: &[ModelLod]) -> ModelId {
        let refs: Vec<(f32, &[ModelPart])> = lods.iter().map(|l| (l.switch_point, &l.parts[..])).collect();
        let model = self.build_model(&refs);
        self.insert_model(model)
    }

    pub fn set_model_lods(&mut self, id: ModelId, lods: &[ModelLod]) {
        if !self.models.contains_key(&id.0) {
            return;
        }
        let refs: Vec<(f32, &[ModelPart])> = lods.iter().map(|l| (l.switch_point, &l.parts[..])).collect();
        let mut model = self.build_model(&refs);
        if let Some(old) = self.models.remove(&id.0) {
            model.bytes += old.collision.as_ref().map_or(0, |(b, _)| b.size());
            model.bytes += old.collision_hull.as_ref().map_or(0, |(b, _)| b.size());
            model.bytes += old.shadow_hull.as_ref().map_or(0, |h| h.verts.size() + h.tans.size());
            model.collision = old.collision;
            model.collision_hull = old.collision_hull;
            model.shadow_hull = old.shadow_hull;
            model.no_collision = old.no_collision;
        }
        self.models.insert(id.0, model);
        self.shadow_epoch += 1;
        self.content_gen += 1;
    }

    pub fn model_lod_count(&self, id: ModelId) -> usize {
        self.models.get(&id.0).map_or(0, |m| {
            let mut lods: Vec<u32> = m.lod_parts.iter().map(|p| p.lod).collect();
            lods.dedup();
            1 + lods.len()
        })
    }

    fn insert_model(&mut self, model: ModelGpu) -> ModelId {
        self.staging.wrote(&self.device, &self.queue, model.bytes);
        let id = self.next_model;
        self.next_model = self.next_model.checked_add(1).unwrap_or(1);
        self.models.insert(id, model);
        self.content_gen += 1;
        ModelId(id)
    }

    fn build_model(&mut self, lods: &[(f32, &[ModelPart])]) -> ModelGpu {
        let switch: Vec<f32> = lods.iter().map(|l| l.0).collect();
        let ranges = gamewire::lod_ranges(&switch);
        let mut verts = Vec::new();
        let mut tans: Vec<u32> = Vec::new();
        let mut indices = Vec::new();
        let mut out_parts = Vec::new();
        let mut lod_parts = Vec::new();
        let mut bounds = [Vec3::splat(f32::INFINITY), Vec3::splat(f32::NEG_INFINITY)];
        let mut lod_bounds = bounds;
        let mut first_lod_end = 0;
        for (l, ((_, parts), range)) in lods.iter().zip(&ranges).enumerate() {
            for p in *parts {
                let first = indices.len() as u32;
                let v0 = verts.len();
                let b = if l == 0 { &mut bounds } else { &mut lod_bounds };
                scene::pack_triangles(&p.vertices, &[], &[], &p.indices, NULL_SLOT, &mut verts, &mut indices, b);
                for v in &mut verts[v0..] {
                    v.slot = range.0.to_bits();
                    v.blend = range.1.to_bits();
                }
                let added = verts.len() - v0;
                if l == 0 && added > 0 {
                    let computed;
                    let t = if p.tangents.len() == p.vertices.len() {
                        &p.tangents
                    } else {
                        computed = tangents::compute_tangents(&p.vertices, &p.indices);
                        &computed
                    };
                    tans.extend(t.iter().take(added).map(|t| tangents::pack_tangent(*t)));
                }
                tans.resize(verts.len(), 0);
                let count = indices.len() as u32 - first;
                if count > 0 {
                    let part =
                        ModelPartGpu { material: self.scene.material_slot(p.material), first, count, lod: l as u32 };
                    if l == 0 { out_parts.push(part) } else { lod_parts.push(part) }
                }
            }
            if l == 0 {
                first_lod_end = indices.len();
            }
        }
        self.sync_materials();
        let too_large = |n: usize, elem: usize| (n as u64).saturating_mul(elem as u64) > self.max_buffer;
        if too_large(verts.len(), size_of::<gpu::GpuVertex>()) || too_large(indices.len(), 4) {
            log::warn!("hp-render: model exceeds the device buffer limit, drawn as a box");
            verts.clear();
            tans.clear();
            indices.clear();
            out_parts.clear();
            lod_parts.clear();
            first_lod_end = 0;
        }
        let [lo, hi] = bounds;
        let (aabb_min, aabb_max) = if lo.cmple(hi).all() {
            (lo, hi)
        } else {
            (Vec3::splat(-UNKNOWN_MODEL_HALF), Vec3::splat(UNKNOWN_MODEL_HALF))
        };
        if verts.is_empty() {
            verts.push(gpu::GpuVertex::default());
        }
        tans.resize(verts.len(), 0);
        if indices.is_empty() {
            indices.extend_from_slice(&[0, 0, 0]);
        }
        let failed = std::cell::Cell::new(false);
        let make = |label: &str, data: &[u8], usage: wgpu::BufferUsages| {
            let size = (data.len() as u64).next_multiple_of(wgpu::COPY_BUFFER_ALIGNMENT);
            let desc = |size: u64| wgpu::BufferDescriptor {
                label: Some(label),
                size,
                usage: usage | wgpu::BufferUsages::COPY_DST,
                mapped_at_creation: false,
            };
            let (buf, oom) = memory::guarded(&self.device, || self.device.create_buffer(&desc(size)));
            if oom {
                failed.set(true);
                return self.device.create_buffer(&desc(16));
            }
            self.queue.write_buffer(&buf, 0, data);
            buf
        };
        let vdata: &[u8] = bytemuck::cast_slice(&verts);
        let tdata: &[u8] = bytemuck::cast_slice(&tans);
        let idata: &[u8] = bytemuck::cast_slice(&indices);
        let edge_list = if out_parts.is_empty() { Vec::new() } else { unique_edges(&verts, &indices[..first_lod_end]) };
        let edata: &[u8] = bytemuck::cast_slice(&edge_list);
        let edges = (!edge_list.is_empty())
            .then(|| (make("hp-render model edges", edata, wgpu::BufferUsages::INDEX), edge_list.len() as u32));
        let vbuf = make("hp-render model vertices", vdata, wgpu::BufferUsages::VERTEX);
        let tbuf = make("hp-render model tangents", tdata, wgpu::BufferUsages::VERTEX);
        let ibuf = make("hp-render model indices", idata, wgpu::BufferUsages::INDEX);
        let failed = failed.get();
        if failed {
            self.upload_failed = true;
        }
        ModelGpu {
            vbuf,
            tbuf,
            ibuf,
            parts: if failed { Vec::new() } else { out_parts },
            lod_parts: if failed { Vec::new() } else { lod_parts },
            lod_ranges: ranges,
            edges: if failed { None } else { edges },
            collision: None,
            collision_hull: None,
            shadow_hull: None,
            no_collision: false,
            aabb_min,
            aabb_max,
            bytes: if failed { 0 } else { (vdata.len() + tdata.len() + idata.len() + edata.len()) as u64 },
        }
    }

    pub fn set_model_collision(&mut self, model: ModelId, segments: &[[[f32; 3]; 2]]) {
        let limit = (self.max_buffer / (2 * size_of::<gpu::GpuVertex>() as u64)) as usize;
        let Some(m) = self.models.get_mut(&model.0) else { return };
        self.content_gen += 1;
        let verts: Vec<gpu::GpuVertex> = segments
            .iter()
            .take(limit)
            .filter(|s| s.iter().flatten().all(|v| v.is_finite()))
            .flat_map(|s| s.map(|pos| gpu::GpuVertex { pos, ..Default::default() }))
            .collect();
        if verts.is_empty() {
            m.collision = None;
            return;
        }
        let data: &[u8] = bytemuck::cast_slice(&verts);
        let buf = self.device.create_buffer(&wgpu::BufferDescriptor {
            label: Some("hp-render model collision"),
            size: (data.len() as u64).next_multiple_of(wgpu::COPY_BUFFER_ALIGNMENT),
            usage: wgpu::BufferUsages::VERTEX | wgpu::BufferUsages::COPY_DST,
            mapped_at_creation: false,
        });
        self.queue.write_buffer(&buf, 0, data);
        m.bytes += data.len() as u64;
        m.collision = Some((buf, verts.len() as u32));
    }

    pub fn set_model_collision_hull(&mut self, model: ModelId, triangles: &[[[f32; 3]; 3]]) {
        let limit = (self.max_buffer / (3 * size_of::<gpu::GpuVertex>() as u64)) as usize;
        let Some(m) = self.models.get_mut(&model.0) else { return };
        self.content_gen += 1;
        let verts: Vec<gpu::GpuVertex> = triangles
            .iter()
            .take(limit)
            .filter(|t| t.iter().flatten().all(|v| v.is_finite()))
            .flat_map(|t| {
                let [a, b, c] = t.map(Vec3::from);
                let n = (b - a).cross(c - a).normalize_or_zero().to_array();
                let nrm_alpha = gpu::pack_normal_alpha(n, 1.0);
                t.map(|pos| gpu::GpuVertex { pos, nrm_alpha, ..Default::default() })
            })
            .collect();
        if verts.is_empty() {
            m.collision_hull = None;
            return;
        }
        let data: &[u8] = bytemuck::cast_slice(&verts);
        let buf = self.device.create_buffer(&wgpu::BufferDescriptor {
            label: Some("hp-render model collision hull"),
            size: (data.len() as u64).next_multiple_of(wgpu::COPY_BUFFER_ALIGNMENT),
            usage: wgpu::BufferUsages::VERTEX | wgpu::BufferUsages::COPY_DST,
            mapped_at_creation: false,
        });
        self.queue.write_buffer(&buf, 0, data);
        m.bytes += data.len() as u64;
        m.collision_hull = Some((buf, verts.len() as u32));
        m.no_collision = false;
    }

    pub fn set_model_shadow_hull(&mut self, model: ModelId, triangles: &[[[f32; 3]; 3]]) {
        let limit = (self.max_buffer / (3 * size_of::<gpu::GpuVertex>() as u64)) as usize;
        let Some(m) = self.models.get_mut(&model.0) else { return };
        if let Some(old) = m.shadow_hull.take() {
            m.bytes = m.bytes.saturating_sub(old.verts.size() + old.tans.size());
        }
        self.shadow_epoch += 1;
        self.content_gen += 1;
        let verts: Vec<gpu::GpuVertex> = triangles
            .iter()
            .take(limit)
            .filter(|t| t.iter().flatten().all(|v| v.is_finite()))
            .flat_map(|t| {
                t.map(|pos| gpu::GpuVertex {
                    pos,
                    nrm_alpha: gpu::pack_normal_alpha([0.0; 3], 1.0),
                    ..Default::default()
                })
            })
            .collect();
        if verts.is_empty() {
            return;
        }
        let data: &[u8] = bytemuck::cast_slice(&verts);
        let tans = vec![0u32; verts.len()];
        let tdata: &[u8] = bytemuck::cast_slice(&tans);
        let make = |label: &str, bytes: &[u8]| {
            let buf = self.device.create_buffer(&wgpu::BufferDescriptor {
                label: Some(label),
                size: (bytes.len() as u64).next_multiple_of(wgpu::COPY_BUFFER_ALIGNMENT),
                usage: wgpu::BufferUsages::VERTEX | wgpu::BufferUsages::COPY_DST,
                mapped_at_creation: false,
            });
            self.queue.write_buffer(&buf, 0, bytes);
            buf
        };
        let hull = HullGpu {
            verts: make("hp-render model shadow hull", data),
            tans: make("hp-render model shadow hull tangents", tdata),
            count: verts.len() as u32,
        };
        let Some(m) = self.models.get_mut(&model.0) else { return };
        m.bytes += hull.verts.size() + hull.tans.size();
        m.shadow_hull = Some(hull);
    }

    pub fn has_model_shadow_hull(&self, model: ModelId) -> bool {
        self.models.get(&model.0).is_some_and(|m| m.shadow_hull.is_some())
    }

    pub fn set_model_no_collision(&mut self, model: ModelId) {
        if let Some(m) = self.models.get_mut(&model.0) {
            self.content_gen += 1;
            m.no_collision = true;
        }
    }

    pub fn has_model(&self, model: ModelId) -> bool {
        self.models.contains_key(&model.0)
    }

    pub fn has_model_collision(&self, model: ModelId) -> bool {
        self.models.get(&model.0).is_some_and(|m| m.collision.is_some())
    }

    pub fn destroy_model(&mut self, id: ModelId) {
        if self.models.remove(&id.0).is_some() {
            self.shadow_epoch += 1;
            self.content_gen += 1;
        }
    }

    pub fn set_model_instances(&mut self, instances: &[ModelInstance]) {
        let limit = (self.max_buffer / size_of::<InstanceGpu>() as u64) as usize;
        if instances.len() > limit {
            log::warn!("hp-render: {} model instances exceed the device limit of {limit}", instances.len());
        }
        self.instance_list = instances[..instances.len().min(limit)].to_vec();
        let seen = self.probe_instances_hash();
        if seen != self.cubes.instances_hash {
            self.cubes.instances_hash = seen;
            self.cubes.epoch += 1;
        }
        self.pack_instances();
    }

    fn probe_instances_hash(&self) -> u64 {
        use std::hash::{Hash, Hasher};
        let mut h = ahash::AHasher::default();
        for i in &self.instance_list {
            if self.scene.flags(i.object).is_some_and(|f| f.contains(ObjectFlags::NO_PROBE)) {
                continue;
            }
            (i.model.0, i.object).hash(&mut h);
            for v in i.transform.iter().flatten().chain(&i.tint).chain(&i.fade) {
                v.to_bits().hash(&mut h);
            }
        }
        h.finish()
    }

    fn pack_instances(&mut self) {
        let list = std::mem::take(&mut self.instance_list);
        let instances = &list[..];
        let hulls: Vec<bool> = instances
            .iter()
            .map(|m| self.scene.flags(m.object).is_some_and(|f| f.contains(ObjectFlags::HULL_SHADOW)))
            .collect();
        let hull = |i: usize| hulls.get(i).copied().unwrap_or(false);
        let mut order: Vec<usize> = (0..instances.len()).collect();
        order.sort_by_key(|&i| (instances.get(i).map_or(0, |m| m.model.0), hull(i)));
        let mut boxes = Vec::with_capacity(instances.len());
        let mut gpu_data = Vec::with_capacity(instances.len());
        let mut refs = Vec::with_capacity(instances.len());
        self.translucent_instances = false;
        let mut groups: Vec<Group> = Vec::new();
        let mut lo = Vec3::INFINITY;
        let mut hi = Vec3::NEG_INFINITY;
        let cube_points = self.model_cube_points();
        for &i in &order {
            let Some(inst) = instances.get(i) else { continue };
            let slot = self.scene.acquire_ref(inst.object);
            refs.push(inst.object);
            let m = sanitize_transform(&inst.transform);
            let (bmin, bmax) = self
                .models
                .get(&inst.model.0)
                .map_or((Vec3::splat(-UNKNOWN_MODEL_HALF), Vec3::splat(UNKNOWN_MODEL_HALF)), |md| {
                    (md.aabb_min, md.aabb_max)
                });
            let (mut ilo, mut ihi) = (Vec3::INFINITY, Vec3::NEG_INFINITY);
            for c in 0..8 {
                let corner = Vec3::new(
                    if c & 1 != 0 { bmax.x } else { bmin.x },
                    if c & 2 != 0 { bmax.y } else { bmin.y },
                    if c & 4 != 0 { bmax.z } else { bmin.z },
                );
                let w = m.transform_point3(corner);
                ilo = ilo.min(w);
                ihi = ihi.max(w);
            }
            lo = lo.min(ilo);
            hi = hi.max(ihi);
            boxes.push((ilo, ihi));
            let cols = m.to_cols_array_2d();
            let tint = if inst.tint.iter().all(|c| c.is_finite()) { inst.tint } else { [1.0; 4] };
            let detail = inst.fade.iter().all(|f| f.is_finite()) && inst.fade[0] <= 0.0 && inst.fade[1] < 0.0;
            self.translucent_instances |= tint[3] < 0.999 || detail;
            let fade =
                if detail || inst.fade.iter().all(|f| f.is_finite() && *f >= 0.0) { inst.fade } else { [0.0; 2] };
            gpu_data.push(InstanceGpu {
                m0: cols[0],
                m1: cols[1],
                m2: cols[2],
                m3: cols[3],
                tint,
                bmin: bmin.to_array(),
                slot,
                bmax: bmax.to_array(),
                fade,
                light: self.model_lights.record_of(inst.object, inst.model),
                cube: u32::from(self.cubes.model_cubemap(&cube_points, m.w_axis.truncate())),
            });
            let n = gpu_data.len() as u32;
            let h = u32::from(hull(i));
            match groups.last_mut() {
                Some(g) if g.key == inst.model.0 => {
                    g.count += 1;
                    g.hull += h;
                }
                _ => groups.push(Group { key: inst.model.0, start: n - 1, count: 1, hull: h, lod: None }),
            }
        }
        for id in std::mem::replace(&mut self.instance_refs, refs) {
            self.scene.release_ref(id);
        }
        let bytes: &[u8] = bytemuck::cast_slice(&gpu_data);
        let _ = self.instances.ensure(&self.device, &self.queue, bytes.len() as u64, self.max_buffer, false);
        if !bytes.is_empty() && bytes.len() as u64 <= self.instances.size {
            self.queue.write_buffer(&self.instances.buffer, 0, bytes);
        }
        self.instance_count = gpu_data.len() as u32;
        self.instance_groups = groups;
        self.instance_boxes = boxes;
        self.instance_data = gpu_data;
        self.instance_bounds = lo.cmple(hi).all().then_some((lo, hi));
        self.shadow_epoch += 1;
        self.content_gen += 1;
        self.instance_list = list;
    }

    pub fn set_sprites(&mut self, sprites: &[SpriteInstance]) {
        let (data, refs, groups) = self.pack_sprites(sprites);
        for id in std::mem::replace(&mut self.sprite_refs, refs) {
            self.scene.release_ref(id);
        }
        let bytes: &[u8] = bytemuck::cast_slice(&data);
        let _ = self.sprites.ensure(&self.device, &self.queue, bytes.len() as u64, self.max_buffer, false);
        if !bytes.is_empty() && bytes.len() as u64 <= self.sprites.size {
            self.queue.write_buffer(&self.sprites.buffer, 0, bytes);
        }
        self.sprite_groups = groups;
        self.content_gen += 1;
    }

    fn pack_sprites(&mut self, sprites: &[SpriteInstance]) -> (Vec<SpriteGpu>, Vec<ObjectId>, Vec<Group>) {
        let limit = (self.max_buffer / size_of::<SpriteGpu>() as u64) as usize;
        let sprites = &sprites[..sprites.len().min(limit)];
        let mut keyed: Vec<(u32, &SpriteInstance)> =
            sprites.iter().map(|s| (self.scene.material_slot(s.material), s)).collect();
        self.sync_materials();
        keyed.sort_by_key(|(m, _)| *m);
        let mut data = Vec::with_capacity(keyed.len());
        let mut refs = Vec::with_capacity(keyed.len());
        let mut groups: Vec<Group> = Vec::new();
        for (mat, s) in keyed {
            if !s.pos.iter().all(|c| c.is_finite()) {
                continue;
            }
            let slot = self.scene.acquire_ref(s.object);
            refs.push(s.object);
            data.push(SpriteGpu {
                pos: s.pos,
                size: if s.size.is_finite() { s.size.max(0.0) } else { 0.0 },
                color: pack_rgba(s.color),
                slot,
            });
            let n = data.len() as u32;
            match groups.last_mut() {
                Some(g) if g.key == mat => g.count += 1,
                _ => groups.push(Group { key: mat, start: n - 1, count: 1, hull: 0, lod: None }),
            }
        }
        (data, refs, groups)
    }

    pub fn light_count(&self) -> usize {
        self.lights_cpu.len()
    }

    pub fn set_lights(&mut self, lights: &[PreviewLight], ambient: [f32; 3]) {
        let lights::Packed { lights: packed, n_dir, left_out } = lights::pack_lights(lights);
        if left_out > 0 {
            log::warn!(
                "hp-render: {left_out} preview lights left out (at most {} lights and {} suns)",
                lights::MAX_LIGHTS - lights::MAX_DIRECTIONAL,
                lights::MAX_DIRECTIONAL
            );
        }
        self.lights_left_out = left_out;
        let bytes: &[u8] = bytemuck::cast_slice(&packed);
        if self.lights_buf.ensure(&self.device, &self.queue, bytes.len() as u64, self.max_storage, false) == Grow::Grown
        {
            self.scene_gen += 1;
        }
        if !bytes.is_empty() && bytes.len() as u64 <= self.lights_buf.size {
            self.queue.write_buffer(&self.lights_buf.buffer, 0, bytes);
        }
        self.lights_cpu = packed;
        self.dir_lights = n_dir;
        self.ambient = ambient.map(|c| if c.is_finite() { c.max(0.0) } else { 0.0 });
        self.lights_gen += 1;
        self.cubes.epoch += 1;
        self.content_gen += 1;
    }

    pub fn set_sun(&mut self, sun: SunParams) {
        let sun = SunParams { spread_deg: finite_or(sun.spread_deg, 0.0).clamp(0.0, 90.0), ..sun };
        if self.sun != sun {
            self.sun = sun;
            self.lights_gen += 1;
            self.cubes.epoch += 1;
            self.content_gen += 1;
        }
    }

    pub fn light_stats(&self) -> LightStats {
        LightStats {
            lights: self.lights_cpu.len() as u32,
            directional: self.dir_lights,
            left_out: self.lights_left_out,
            crowded_clusters: self.crowded_clusters,
            sky_occlusion: self.sun.sky_ambient,
            sky_layers: [
                self.sky_layer_info(shadow::Layers::Sun).map_or(0, |l| l.layers()),
                self.sky_layer_info(shadow::Layers::Sky).map_or(0, |l| l.layers()),
            ],
        }
    }

    fn ensure_pipelines(&mut self, samples: u32) -> usize {
        let i = usize::from(samples > 1);
        if self.pipelines[i].is_none()
            && let Some(handle) = self.prewarm.take()
        {
            for (k, set) in handle.join().unwrap_or_default() {
                if let Some(slot) = self.pipelines.get_mut(k).filter(|p| p.is_none()) {
                    *slot = Some(set);
                }
            }
        }
        if self.pipelines[i].is_none() {
            self.pipelines[i] = Some(PipelineSet::new(&self.device, &self.shared, samples));
        }
        pipeline_cache::save_if_dirty(&self.device);
        i
    }

    fn ensure_view(&mut self, id: u32, size: [u32; 2], samples: u32) {
        if self.views.get(&id).is_some_and(|v| v.size == size && v.samples == samples) {
            return;
        }
        let extent = wgpu::Extent3d { width: size[0], height: size[1], depth_or_array_layers: 1 };
        let tex = |label: &str, format: wgpu::TextureFormat, samples: u32, usage: wgpu::TextureUsages| {
            self.device.create_texture(&wgpu::TextureDescriptor {
                label: Some(label),
                size: extent,
                mip_level_count: 1,
                sample_count: samples,
                dimension: wgpu::TextureDimension::D2,
                format,
                usage,
                view_formats: &[],
            })
        };
        let att = wgpu::TextureUsages::RENDER_ATTACHMENT;
        let color = tex(
            "hp-render view color",
            COLOR_FORMAT,
            1,
            att | wgpu::TextureUsages::TEXTURE_BINDING | wgpu::TextureUsages::COPY_SRC,
        );
        let display = self.device.create_texture(&wgpu::TextureDescriptor {
            label: Some("hp-render view display"),
            size: extent,
            mip_level_count: 1,
            sample_count: 1,
            dimension: wgpu::TextureDimension::D2,
            format: LDR_FORMAT,
            usage: att | wgpu::TextureUsages::TEXTURE_BINDING | wgpu::TextureUsages::COPY_SRC,
            view_formats: &[DISPLAY_FORMAT],
        });
        let msaa = (samples > 1).then(|| tex("hp-render view msaa", COLOR_FORMAT, samples, att));
        let depth = tex("hp-render view depth", DEPTH_FORMAT, samples, att | wgpu::TextureUsages::TEXTURE_BINDING);
        let px = u64::from(size[0]) * u64::from(size[1]);
        let target_bytes =
            px * 12 + px * 4 * u64::from(samples) + if samples > 1 { px * 8 * u64::from(samples) } else { 0 };
        let generation = self.next_generation;
        self.next_generation += 1;
        let color_view = color.create_view(&wgpu::TextureViewDescriptor::default());
        let display_srgb = display.create_view(&wgpu::TextureViewDescriptor::default());
        let display_view =
            display.create_view(&wgpu::TextureViewDescriptor { format: Some(DISPLAY_FORMAT), ..Default::default() });
        let msaa_view = msaa.map(|t| t.create_view(&wgpu::TextureViewDescriptor::default()));
        let depth_view = depth.create_view(&wgpu::TextureViewDescriptor::default());
        if let Some(v) = self.views.get_mut(&id) {
            v.size = size;
            v.samples = samples;
            v.color = color;
            v.color_view = color_view;
            v.display = display;
            v.display_srgb = display_srgb;
            v.display_view = display_view;
            v.msaa_view = msaa_view;
            v.depth_view = depth_view;
            v.generation = generation;
            v.target_bytes = target_bytes;
            v.cluster_key = None;
            return;
        }
        let uniform = self.device.create_buffer(&wgpu::BufferDescriptor {
            label: Some("hp-render view uniform"),
            size: size_of::<ViewUniform>() as u64,
            usage: wgpu::BufferUsages::UNIFORM | wgpu::BufferUsages::COPY_DST,
            mapped_at_creation: false,
        });
        let storage = wgpu::BufferUsages::STORAGE;
        let view = ViewState {
            size,
            samples,
            color,
            color_view,
            display,
            display_srgb,
            display_view,
            msaa_view,
            depth_view,
            generation,
            target_bytes,
            uniform,
            overlay: GrowBuffer::new(&self.device, "hp-render overlay", wgpu::BufferUsages::VERTEX, 1 << 12),
            clusters: GrowBuffer::new(&self.device, "hp-render clusters", storage, 16),
            light_indices: GrowBuffer::new(&self.device, "hp-render light indices", storage, 16),
            cluster_gen: 1,
            cluster_key: None,
            bind_group: None,
            bind_key: (0, 0),
            sky_pass: None,
            draws: 0,
            triangles: 0,
            counts: ViewCounts::default(),
            chunks_seen: 0,
            chunks_skipped: 0,
            occ: None,
            outline: None,
            animated: false,
        };
        self.views.insert(id, view);
    }

    fn update_clusters(&mut self, id: u32, cam: &Camera) -> ClusterGrid {
        let grid = ClusterGrid::new(cam);
        let key = ClusterKey {
            view_proj: cam.view_proj.to_cols_array(),
            size: [cam.width as u32, cam.height as u32],
            lights_gen: self.lights_gen,
        };
        let Some(view) = self.views.get_mut(&id) else { return grid };
        if view.cluster_key == Some(key) {
            return grid;
        }
        let lights::Clusters { table, list, crowded } =
            lights::build_clusters(cam, &grid, &self.lights_cpu, self.dir_lights);
        if crowded > 0 && self.crowded_logged != self.lights_gen {
            self.crowded_logged = self.lights_gen;
            log::info!(
                "hp-render: {crowded} light clusters reach more than {} lights: the weakest there are left out",
                lights::MAX_PER_CLUSTER
            );
        }
        self.crowded_clusters = crowded;
        let Some(view) = self.views.get_mut(&id) else { return grid };
        let tbytes: &[u8] = bytemuck::cast_slice(&table);
        let lbytes: &[u8] = bytemuck::cast_slice(&list);
        let g1 = view.clusters.ensure(&self.device, &self.queue, tbytes.len() as u64, self.max_storage, false);
        let g2 = view.light_indices.ensure(&self.device, &self.queue, lbytes.len() as u64, self.max_storage, false);
        if g1 == Grow::Grown || g2 == Grow::Grown {
            view.cluster_gen += 1;
        }
        if g1 == Grow::TooLarge || g2 == Grow::TooLarge {
            return ClusterGrid::default();
        }
        if !tbytes.is_empty() {
            self.queue.write_buffer(&view.clusters.buffer, 0, tbytes);
        }
        if !lbytes.is_empty() {
            self.queue.write_buffer(&view.light_indices.buffer, 0, lbytes);
        }
        view.cluster_key = Some(key);
        grid
    }

    fn ensure_view_bind_group(&mut self, id: u32) {
        let Some(view) = self.views.get(&id) else { return };
        let key = (self.scene_gen, view.cluster_gen);
        if view.bind_group.is_some() && view.bind_key == key {
            return;
        }
        let bg = self.view_bind_group_with(&view.uniform, view);
        if let Some(view) = self.views.get_mut(&id) {
            view.bind_group = Some(bg);
            view.bind_key = key;
        }
    }

    fn view_bind_group_with(&self, uniform: &wgpu::Buffer, view: &ViewState) -> wgpu::BindGroup {
        self.device.create_bind_group(&wgpu::BindGroupDescriptor {
            label: Some("hp-render view"),
            layout: &self.shared.view_bgl,
            entries: &[
                wgpu::BindGroupEntry { binding: 0, resource: uniform.as_entire_binding() },
                wgpu::BindGroupEntry { binding: 1, resource: self.states.buffer.as_entire_binding() },
                wgpu::BindGroupEntry { binding: 2, resource: wgpu::BindingResource::Sampler(&self.sampler) },
                wgpu::BindGroupEntry { binding: 3, resource: wgpu::BindingResource::Sampler(&self.sampler_clamp) },
                wgpu::BindGroupEntry { binding: 4, resource: self.lights_buf.buffer.as_entire_binding() },
                wgpu::BindGroupEntry { binding: 5, resource: view.clusters.buffer.as_entire_binding() },
                wgpu::BindGroupEntry { binding: 6, resource: view.light_indices.buffer.as_entire_binding() },
                wgpu::BindGroupEntry {
                    binding: 7,
                    resource: wgpu::BindingResource::TextureView(
                        self.shadow.as_ref().map_or(&self.shadow_dummy_array, |s| &s.view),
                    ),
                },
                wgpu::BindGroupEntry { binding: 8, resource: wgpu::BindingResource::Sampler(&self.shadow_sampler) },
                wgpu::BindGroupEntry {
                    binding: 9,
                    resource: self.model_lights.records.as_ref().unwrap_or(&self.dummy_storage).as_entire_binding(),
                },
                wgpu::BindGroupEntry {
                    binding: 10,
                    resource: self.model_lights.colors.as_ref().unwrap_or(&self.dummy_storage).as_entire_binding(),
                },
                wgpu::BindGroupEntry { binding: 11, resource: self.light_styles.as_entire_binding() },
                wgpu::BindGroupEntry { binding: 12, resource: wgpu::BindingResource::TextureView(self.cubes.view()) },
                wgpu::BindGroupEntry { binding: 13, resource: self.cubes.info().as_entire_binding() },
                wgpu::BindGroupEntry {
                    binding: 14,
                    resource: wgpu::BindingResource::TextureView(self.local_shadows.view()),
                },
                wgpu::BindGroupEntry {
                    binding: 15,
                    resource: wgpu::BindingResource::TextureView(
                        self.sky_map.as_ref().map_or(&self.shadow_dummy_array, |s| &s.view),
                    ),
                },
                wgpu::BindGroupEntry {
                    binding: 16,
                    resource: wgpu::BindingResource::TextureView(
                        self.shadow.as_ref().map_or(&self.shadow_dummy_array, |s| &s.cut),
                    ),
                },
                wgpu::BindGroupEntry {
                    binding: 17,
                    resource: wgpu::BindingResource::TextureView(
                        self.sky_map.as_ref().map_or(&self.shadow_dummy_array, |s| &s.cut),
                    ),
                },
                wgpu::BindGroupEntry {
                    binding: 22,
                    resource: wgpu::BindingResource::TextureView(
                        self.control_shadow.as_ref().map_or(&self.shadow_dummy_array, |s| &s.view),
                    ),
                },
                wgpu::BindGroupEntry { binding: 18, resource: self.indirect_buffer().as_entire_binding() },
                wgpu::BindGroupEntry { binding: 19, resource: self.projectors.buffer.as_entire_binding() },
                wgpu::BindGroupEntry {
                    binding: 20,
                    resource: wgpu::BindingResource::TextureView(&self.projectors.view),
                },
                wgpu::BindGroupEntry {
                    binding: 21,
                    resource: wgpu::BindingResource::TextureView(
                        self.projector_shadow_view().unwrap_or(&self.shadow_dummy_array),
                    ),
                },
            ],
        })
    }

    #[allow(clippy::too_many_arguments)]
    fn view_uniform(
        &self,
        cam: &Camera,
        params: &ViewParams,
        opts: &ViewOptions,
        grid: Option<ClusterGrid>,
        shadow: Option<&shadow::ShadowFit>,
        sky_pass: bool,
        tone: f32,
    ) -> ViewUniform {
        let (grid_a, grid_b) = match (cam.is_ortho, params.grid.as_ref()) {
            (true, Some(g)) => grid_uniform_min(g, cam.ppu, opts.grid_min_px),
            _ => ([1.0, 0.0, 1.0, 0.0], [0.0, 0.0, cam.ppu, 0.0]),
        };
        let (dims, z) = match grid {
            Some(g) => ([g.tiles_x, g.tiles_y, g.slices, lights::TILE_PX], [g.near, g.far, g.scale, 0.0]),
            None => ([0; 4], [1.0, 2.0, 0.0, 0.0]),
        };
        let lit = params.mode == RenderMode::Lit && !cam.is_ortho;
        let mut env = self.environment;
        let mut fog_opts = *opts;
        if opts.fog_volume_inside
            && !cam.is_ortho
            && let Some(v) = fog_volume::inside_volume(&self.fog_volumes, cam.eye.to_array())
        {
            env.fog = v.fog;
            fog_opts.show_fog = true;
        }
        let ldr_sky = opts.compiled_lighting && self.compiled.mode() == Some(CompiledLightMode::Ldr);
        let mut o = options_uniform(cam, &fog_opts, &env, sky_pass, ldr_sky);
        if self.hdr_lit(opts, params.mode, cam.is_ortho) {
            o.opts[0] |= OPT_HDR_TINT;
        }
        if !cam.is_ortho
            && params.mode == RenderMode::Flat
            && self.lightmap_display() == lightmap::LightmapDisplay::Grid
        {
            o.opts[0] |= OPT_NEUTRAL_FLAT;
        }
        if cam.is_ortho && light_background(params.background) {
            o.opts[0] |= OPT_LIGHT_BACKGROUND;
        }
        if self.probing {
            o.opts[0] |= OPT_PROBE;
        }
        if let Some(r) = &self.reflecting {
            if r.world_only {
                o.opts[0] |= OPT_REFLECT_WORLD;
            }
            if r.sky_only {
                o.opts[0] |= OPT_REFLECT_SKY;
            }
        }
        if !cam.is_ortho && opts.compiled_lighting && compiled_light::textured_mode(params.mode) {
            match self.compiled.mode() {
                Some(CompiledLightMode::Fullbright) => o.opts[0] |= OPT_COMPILED_LIGHT | OPT_COMPILED_FULLBRIGHT,
                Some(_) => o.opts[0] |= OPT_COMPILED_LIGHT,
                None => {}
            }
        }
        let model_light = !cam.is_ortho
            && !sky_pass
            && params.mode == RenderMode::Lit
            && o.opts[0] & OPT_COMPILED_LIGHT == 0
            && self.lightmap_display() == lightmap::LightmapDisplay::Baked;
        if model_light {
            o.opts[0] |= OPT_MODEL_LIGHT;
        }
        ViewUniform {
            view_proj: cam.view_proj.to_cols_array_2d(),
            eye: cam.eye.extend(if cam.is_ortho { 1.0 } else { 0.0 }).to_array(),
            right: cam.right.extend(0.0).to_array(),
            up: cam.up.extend(0.0).to_array(),
            forward: cam.forward.extend(cam.near).to_array(),
            viewport: [cam.width, cam.height, 1.0 / cam.width, 1.0 / cam.height],
            selection: srgba_to_linear(params.selection_color),
            grid_a,
            grid_b,
            ortho: [cam.ortho_center[0], cam.ortho_center[1], cam.half_px[0], cam.half_px[1]],
            ambient: [self.ambient[0], self.ambient[1], self.ambient[2], 0.0],
            cluster_dims: dims,
            cluster_z: z,
            misc: [
                mode_code(params.mode),
                if lit { self.dir_lights } else { 0 },
                if o.opts[0] & OPT_COMPILED_LIGHT != 0 {
                    tone
                } else if model_light {
                    self.lightmap_exposure() * tone
                } else if lit {
                    tone
                } else {
                    1.0
                }
                .to_bits(),
                tone.to_bits(),
            ],
            shadow_vp: shadow.map_or(Mat4::IDENTITY, |s| s.view_proj).to_cols_array_2d(),
            shadow: shadow.map_or([0.0; 4], shadow::ShadowFit::params),
            opts: o.opts,
            grid3d: o.grid3d,
            fog_a: o.fog_a,
            fog_b: o.fog_b,
            fog_c: o.fog_c,
            sky: o.sky,
            sky3d: o.sky3d,
            sky_uv: o.sky_uv,
            proj: [cam.tan_half_fov * cam.width / cam.height, cam.tan_half_fov, o.far_z, 0.0],
            sky_vis_a: [0.0; 4],
            sky_vis_b: [0.0; 4],
            sun_soft: [0.0; 4],
            sky_paint: o.sky_paint,
            indirect_a: [0.0; 4],
            indirect_b: [1.0, 1.0, 1.0, 0.0],
            sky_march: [[0.0; 4]; 18],
            control_vp: Mat4::IDENTITY.to_cols_array_2d(),
            control_params: [0.0; 4],
            control_color: [0.0; 4],
        }
    }

    fn upload_overlay(&mut self, id: u32, overlay: &Overlay) -> OverlayLayout {
        if overlay.is_empty() {
            return OverlayLayout::default();
        }
        let Some((layout, total)) = OverlayLayout::of(overlay) else {
            log::warn!("hp-render: overlay too large, skipped");
            return OverlayLayout::default();
        };
        let Some(view) = self.views.get_mut(&id) else { return OverlayLayout::default() };
        if view.overlay.ensure(&self.device, &self.queue, total, self.max_buffer, false) == Grow::TooLarge {
            log::warn!("hp-render: overlay too large, skipped");
            return OverlayLayout::default();
        }
        let bytes = &mut self.overlay_bytes;
        bytes.clear();
        overlay::encode(overlay, &mut self.overlay_scratch, usize::MAX, &mut |b| bytes.extend_from_slice(b));
        self.queue.write_buffer(&view.overlay.buffer, 0, bytes);
        layout
    }

    pub fn set_retained_overlay(&mut self, view_id: u32, overlay: &Overlay) {
        self.retained_uploads += 1;
        self.content_gen += 1;
        let previous = self.retained.remove(&view_id);
        if overlay.is_empty() {
            return;
        }
        let layout = OverlayLayout::of(overlay).filter(|(_, total)| *total <= self.max_buffer);
        let Some((layout, total)) = layout else {
            log::warn!("hp-render: retained overlay too large, skipped");
            return;
        };
        let size = total.max(16).next_multiple_of(wgpu::COPY_BUFFER_ALIGNMENT);
        let buffer = match previous {
            Some(p) if p.size >= size && p.size / 2 <= size => p.buffer,
            _ => self.device.create_buffer(&wgpu::BufferDescriptor {
                label: Some("hp-render retained overlay"),
                size,
                usage: wgpu::BufferUsages::VERTEX | wgpu::BufferUsages::COPY_DST,
                mapped_at_creation: false,
            }),
        };
        let mut offset = 0u64;
        let queue = &self.queue;
        let mut scratch = Vec::new();
        overlay::encode(overlay, &mut scratch, overlay::UPLOAD_BLOCK, &mut |bytes| {
            queue.write_buffer(&buffer, offset, bytes);
            offset += bytes.len() as u64;
        });
        let chunks = overlay::chunks(overlay, &layout);
        let size = buffer.size();
        self.retained.insert(view_id, RetainedGpu { buffer, size, layout, chunks });
    }

    pub fn clear_retained_overlay(&mut self, view_id: u32) {
        if self.retained.remove(&view_id).is_some() {
            self.retained_uploads += 1;
            self.content_gen += 1;
        }
    }

    pub fn has_retained_overlay(&self, view_id: u32) -> bool {
        self.retained.contains_key(&view_id)
    }

    pub fn render_view(&mut self, view_id: u32, params: &ViewParams, overlay: &Overlay) -> &wgpu::TextureView {
        let started = std::time::Instant::now();
        self.reserve_instance_pool();
        let is_reflection = self.reflecting.is_some();
        let profiled = !self.probing && !matches!(params.kind, ViewKind::Ortho { .. });
        if !is_reflection {
            self.reflection_cpu_ms = 0.0;
            if !self.probing {
                self.render_serial += 1;
            }
        }
        if profiled
            && !is_reflection
            && let Some(p) = self.profiler.as_mut()
        {
            p.collect(&self.device);
        }
        self.flush();
        self.write_cube_info();
        let mut params = *params;
        params.size = [params.size[0].clamp(1, self.max_tex_dim), params.size[1].clamp(1, self.max_tex_dim)];
        let opts = self.view_options(view_id);
        let reflecting = self.reflecting;
        let mut cam = Camera::new(&params);
        if reflecting.is_none() && !cam.is_ortho {
            self.reflection_cpu_ms = 0.0;
            self.reflection_active = false;
        }
        if let Some(r) = &reflecting {
            cam = cam.clipped_by(r.plane);
        }
        let samples = if params.msaa { 4 } else { 1 };
        let pi = self.ensure_pipelines(samples);
        self.ensure_view(view_id, params.size, samples);
        let mut lap = profile::Lap::new();
        let chain = !self.probing && reflecting.is_none() && self.tone_chain(&opts, params.mode, cam.is_ortho);
        self.prepare_tone(view_id, params.size, params.mode, chain);
        let tone = reflecting.map_or_else(|| self.tone_scale_for(view_id, chain), |r| r.tone);
        let lit = params.mode == RenderMode::Lit && !cam.is_ortho && !self.baked_lit(&cam, params.mode);
        if !cam.is_ortho && opts.cubemaps && reflecting.is_none() {
            self.cubes.note_view(view_id, &params, &opts);
        }
        let setup_ms = lap.take();
        self.water_reflect = None;
        let reflect = if reflecting.is_none() && !self.probing && water::applies(&params, &opts, cam.is_ortho) {
            self.render_reflection(&params, &opts, &cam, tone)
        } else {
            None
        };
        if reflecting.is_none() && !cam.is_ortho {
            self.reflection_active = reflect.is_some();
        }
        self.water_reflect = reflect;
        let _ = lap.take();
        let fit_cam = reflecting.map_or(cam, |r| r.main_cam);
        let grid = lit.then(|| self.update_clusters(view_id, &cam));
        let clusters_ms = lap.take();
        let control = if !cam.is_ortho
            && matches!(params.mode, RenderMode::Textured | RenderMode::TexturedWire | RenderMode::Lit)
        {
            opts.shadow_control.and_then(|c| self.prepare_control_shadow(&fit_cam, c))
        } else {
            None
        };
        let shadow = if lit { self.prepare_shadow(&fit_cam) } else { None };
        let sky_map = if lit && self.sun.sky_ambient { self.prepare_sky_map(&fit_cam) } else { None };
        let local_layers = if reflecting.is_some() {
            Vec::new()
        } else if lit {
            self.prepare_local_shadows(view_id, &cam, opts.light_shadows)
        } else {
            self.local_shadows.release(view_id);
            Vec::new()
        };
        let shadow_prep_ms = lap.take();
        let mut uniform = self.view_uniform(&cam, &params, &opts, grid, shadow.as_ref().map(|s| &s.0), false, tone);
        if let (Some((fit, _)), Some(config)) = (&control, opts.shadow_control) {
            uniform.control_vp = fit.view_proj.to_cols_array_2d();
            uniform.control_params = [1.0, fit.depth_range, config.distance, (fit.texel_world * 0.5).max(0.05)];
            uniform.control_color = [config.color[0], config.color[1], config.color[2], 1.0];
        }
        if let Some((fit, _)) = &shadow {
            uniform.sun_soft = fit.soft_params(self.sun.spread_deg);
            uniform.sun_soft[3] = self.layer_count(shadow::Layers::Sun) as f32;
        }
        if let Some((fit, _)) = &sky_map {
            (uniform.sky_vis_a, uniform.sky_vis_b) = fit.params();
            uniform.sky_vis_b[3] = self.layer_count(shadow::Layers::Sky) as f32;
            if !profile::baseline("skymarch") {
                uniform.sky_march = sky_layers::march_steps(uniform.sky_vis_b[2]);
            }
        }
        (uniform.indirect_a, uniform.indirect_b) = self.indirect_uniform(lit && opts.indirect_light);
        let shaded_3d = !cam.is_ortho && params.mode != RenderMode::Wireframe;
        let env = self.environment;
        let sky_cam = match env.sky_3d {
            Some(s)
                if opts.show_sky_3d
                    && shaded_3d
                    && (self.scene.sky_3d > 0 || (opts.compiled_lighting && !self.compiled_sky.objects.is_empty())) =>
            {
                sky::sky_camera(&cam, &s)
            }
            _ => None,
        };
        let sky_uniform = sky_cam.as_ref().map(|sc| self.view_uniform(sc, &params, &opts, None, None, true, tone));
        let game_wire = if cam.is_ortho { 0 } else { opts.game_wireframe.min(3) };
        if !cam.is_ortho {
            self.ensure_detail_pipes(samples);
            self.ensure_particle_pipes(samples);
        }
        let lines_mode = self.device.features().contains(wgpu::Features::POLYGON_MODE_LINE);
        if game_wire == 3 || (game_wire > 0 && !lines_mode) {
            let cache = self.game_outlines.get_or_insert_with(|| gamewire::OutlineCache::new(&self.device));
            for c in std::iter::once(&cam).chain(sky_cam.as_ref()) {
                cache.prepare(&self.device, &self.queue, &self.scene, c, self.max_buffer);
            }
        }
        let sky_bg = match env.sky_paint.filter(|_| opts.show_sky && shaded_3d) {
            Some(p) => Some(SkyView::new([Some(p.star_texture.unwrap_or(TexId::WHITE)); 6], 0.0)),
            None => env.sky.filter(|_| opts.show_sky && shaded_3d),
        };
        if let Some(sky) = sky_bg {
            self.ensure_sky_bind(sky.faces);
        }
        let layout = self.upload_overlay(view_id, overlay);
        self.ensure_view_bind_group(view_id);
        let water_frame = self.prepare_water(view_id, &cam, &params, &opts);
        let fog_frame = self.prepare_fog_volumes(view_id, &cam, &params, &opts);
        let sun_frame = self.prepare_sun_glow(view_id, &cam, &params, &opts);
        let particle_refraction =
            !cam.is_ortho && params.mode != RenderMode::Wireframe && self.prepare_particle_refraction(view_id);
        if sky_cam.is_some() {
            self.ensure_sky_pass(view_id);
        }
        if cam.is_ortho && opts.fill_selected_2d > 0.0 {
            self.update_selected_chunks();
        }
        if cam.is_ortho {
            self.ensure_bg_pipelines(samples);
        }
        let outline_on = opts.selection_outline && !cam.is_ortho && !self.scene.selected.is_empty();
        if outline_on {
            self.update_selected_chunks();
            self.ensure_outline(view_id);
        }
        let mut occ = if opts.occlusion_culling && shaded_3d && occlusion::supported() {
            self.take_occlusion(view_id)
        } else {
            if let Some(v) = self.views.get_mut(&view_id) {
                v.occ = None;
            }
            None
        };
        let mut hooks = std::mem::take(&mut self.hooks);
        let mut animated = false;
        let packed = if shaded_3d && params.show_models && !self.probing {
            self.pack_view_instances(&cam, opts.model_lods)
        } else {
            None
        };
        let models_ms = lap.take();

        let mut enc =
            self.device.create_command_encoder(&wgpu::CommandEncoderDescriptor { label: Some("hp-render view") });
        let mut counter = FrameCounter::default();
        let recording = std::time::Instant::now();
        if profiled && let Some(p) = self.profiler.as_mut() {
            p.begin(is_reflection);
            p.mark(&mut enc, 0);
        }
        if let Some((fit, true)) = &shadow {
            let (draws, tris) = self.record_shadow(&mut enc, fit);
            counter.draws += draws;
            counter.triangles += tris;
        }
        if let Some((fit, true)) = &control {
            let (draws, tris) = self.record_control_shadow(&mut enc, fit);
            counter.draws += draws;
            counter.triangles += tris;
        }
        if let Some((fit, true)) = &sky_map {
            let (draws, tris) = self.record_sky_map(&mut enc, fit);
            counter.draws += draws;
            counter.triangles += tris;
        }
        if !local_layers.is_empty() {
            let (draws, tris) = self.record_local_shadows(&mut enc, &local_layers);
            counter.draws += draws;
            counter.triangles += tris;
        }
        if self.projectors.has_shadow_maps() {
            let (draws, tris) = self.record_projector_shadows(&mut enc);
            counter.draws += draws;
            counter.triangles += tris;
        }
        if let Some(p) = self.profiler.as_ref() {
            p.mark(&mut enc, 1);
        }
        if let (Some(view), Some(ps)) = (self.views.get(&view_id), self.pipelines[pi].as_ref())
            && let Some(bind_group) = view.bind_group.as_ref()
        {
            self.queue.write_buffer(&view.uniform, 0, bytemuck::bytes_of(&uniform));
            let sky_bind = match (&sky_uniform, &view.sky_pass) {
                (Some(u), Some((buffer, bg, _))) => {
                    self.queue.write_buffer(buffer, 0, bytemuck::bytes_of(u));
                    Some(bg)
                }
                _ => None,
            };
            let hook_cx = HookContext {
                renderer: self,
                view_id,
                params: &params,
                options: &opts,
                view_proj: cam.view_proj.to_cols_array_2d(),
                eye: cam.eye.to_array(),
                is_ortho: cam.is_ortho,
                size: params.size,
                samples,
                color: &view.color_view,
                color_texture: &view.color,
                view_bind_group: bind_group,
            };
            for h in hooks.iter_mut() {
                h.prepare(&hook_cx, &mut enc);
            }
            let bg = if uniform.opts[0] & OPT_FOG != 0 && shaded_3d {
                [0, 1, 2].map(|i| f64::from(uniform.fog_b[i]))
            } else {
                params.background.map(|c| f64::from(srgb_to_linear(c)))
            };
            let clear = wgpu::Color { r: bg[0], g: bg[1], b: bg[2], a: 1.0 };
            let (target, resolve) = match &view.msaa_view {
                Some(m) => (m, Some(&view.color_view)),
                None => (&view.color_view, None),
            };
            let ctx = DrawCtx {
                r: self,
                ps,
                cam: &cam,
                params: &params,
                opts: &opts,
                view,
                layout,
                retained: self.retained.get(&view_id),
                hooks: &hooks,
                hook_cx: &hook_cx,
                bind_group,
                sky_holes: (opts.sky_see_through || self.baked_view(&cam)) && (sky_bg.is_some() || sky_cam.is_some()),
                water: water_frame.as_ref(),
                packed: packed.as_ref(),
            };
            let depth = || {
                Some(wgpu::RenderPassDepthStencilAttachment {
                    view: &view.depth_view,
                    depth_ops: Some(wgpu::Operations { load: wgpu::LoadOp::Clear(0.0), store: wgpu::StoreOp::Discard }),
                    stencil_ops: None,
                })
            };
            let lists = if cam.is_ortho { Default::default() } else { ctx.collect_chunks(&mut counter) };
            animated =
                !cam.is_ortho && (self.shows_animation(&lists) || water_frame.as_ref().is_some_and(|w| w.animated));
            let mut cands: Vec<ChunkDraw> = Vec::new();
            let mut occ_args: Option<wgpu::Buffer> = None;
            if let (Some(o), Some(pipes)) = (occ.as_mut(), self.occ_pipes.as_ref()) {
                cands = lists[0].iter().chain(&lists[1]).copied().collect();
                let gpu: Vec<occlusion::CandidateGpu> = cands.iter().map(ChunkDraw::candidate).collect();
                let vp = cam.view_proj.to_cols_array_2d();
                let slots = self.scene.chunks.len();
                if o.prepare(&self.device, &self.queue, pipes, &mut enc, vp, &gpu, slots, self.max_storage) {
                    occ_args = Some(o.args.buffer.clone());
                }
            }
            let mut color_load = wgpu::LoadOp::Clear(clear);
            let mut sky_drawn = false;
            if let (Some(sky_bind), Some(sc)) = (sky_bind, sky_cam.as_ref()) {
                let mut pass = enc.begin_render_pass(&wgpu::RenderPassDescriptor {
                    label: Some("hp-render 3D skybox pass"),
                    color_attachments: &[Some(wgpu::RenderPassColorAttachment {
                        view: target,
                        depth_slice: None,
                        resolve_target: None,
                        ops: wgpu::Operations { load: color_load, store: wgpu::StoreOp::Store },
                    })],
                    depth_stencil_attachment: depth(),
                    timestamp_writes: None,
                    occlusion_query_set: None,
                    multiview_mask: None,
                });
                pass.set_bind_group(0, bind_group, &[]);
                ctx.draw_sky_background(&mut pass, sky_bg.as_ref(), &mut counter);
                pass.set_bind_group(0, sky_bind, &[]);
                DrawCtx { cam: sc, water: None, packed: None, ..ctx }.record_sky_3d(&mut pass, &mut counter);
                color_load = wgpu::LoadOp::Load;
                sky_drawn = true;
            }
            let two_phase = occ_args.is_some();
            let split = (water_frame.is_some() || fog_frame.is_some() || sun_frame.is_some() || particle_refraction)
                && !cam.is_ortho;
            {
                let (resolve_now, color_store, depth_store) = if two_phase {
                    (None, wgpu::StoreOp::Store, wgpu::StoreOp::Store)
                } else if split {
                    (resolve, wgpu::StoreOp::Store, wgpu::StoreOp::Store)
                } else {
                    let store = if resolve.is_some() { wgpu::StoreOp::Discard } else { wgpu::StoreOp::Store };
                    (resolve, store, wgpu::StoreOp::Discard)
                };
                let mut pass = enc.begin_render_pass(&wgpu::RenderPassDescriptor {
                    label: Some("hp-render view pass"),
                    color_attachments: &[Some(wgpu::RenderPassColorAttachment {
                        view: target,
                        depth_slice: None,
                        resolve_target: resolve_now,
                        ops: wgpu::Operations { load: color_load, store: color_store },
                    })],
                    depth_stencil_attachment: Some(wgpu::RenderPassDepthStencilAttachment {
                        view: &view.depth_view,
                        depth_ops: Some(wgpu::Operations { load: wgpu::LoadOp::Clear(0.0), store: depth_store }),
                        stencil_ops: None,
                    }),
                    timestamp_writes: None,
                    occlusion_query_set: None,
                    multiview_mask: None,
                });
                pass.set_bind_group(0, bind_group, &[]);
                if cam.is_ortho {
                    ctx.record_2d(&mut pass, &mut counter);
                } else {
                    if !sky_drawn {
                        ctx.draw_sky_background(&mut pass, sky_bg.as_ref(), &mut counter);
                    }
                    let first = occ_args.as_ref().map(|a| (&cands[..], a));
                    ctx.record_3d_opaque(&mut pass, &lists, first, &mut counter);
                    if !two_phase && split {
                        ctx.record_3d_rest_solid(&mut pass, &lists, None, &mut counter);
                    } else if !two_phase {
                        ctx.record_3d_rest(&mut pass, &lists, None, &mut counter);
                    }
                }
            }
            if let (Some(args), Some(o), Some(pipes)) = (occ_args.as_ref(), occ.as_mut(), self.occ_pipes.as_ref()) {
                o.cull(&self.device, pipes, &mut enc, samples, cands.len() as u32);
                let mut pass = enc.begin_render_pass(&wgpu::RenderPassDescriptor {
                    label: Some("hp-render view pass 2"),
                    color_attachments: &[Some(wgpu::RenderPassColorAttachment {
                        view: target,
                        depth_slice: None,
                        resolve_target: resolve,
                        ops: wgpu::Operations {
                            load: wgpu::LoadOp::Load,
                            store: if resolve.is_some() && !split {
                                wgpu::StoreOp::Discard
                            } else {
                                wgpu::StoreOp::Store
                            },
                        },
                    })],
                    depth_stencil_attachment: Some(wgpu::RenderPassDepthStencilAttachment {
                        view: &view.depth_view,
                        depth_ops: Some(wgpu::Operations {
                            load: wgpu::LoadOp::Load,
                            store: if split { wgpu::StoreOp::Store } else { wgpu::StoreOp::Discard },
                        }),
                        stencil_ops: None,
                    }),
                    timestamp_writes: None,
                    occlusion_query_set: None,
                    multiview_mask: None,
                });
                pass.set_bind_group(0, bind_group, &[]);
                if split {
                    ctx.record_3d_rest_solid(&mut pass, &lists, Some((&cands[..], args)), &mut counter);
                } else {
                    ctx.record_3d_rest(&mut pass, &lists, Some((&cands[..], args)), &mut counter);
                }
            }
            if split {
                let keep = fog_frame.is_some() || sun_frame.is_some() || particle_refraction;
                ctx.record_water(&mut enc, water_frame.as_ref(), view_id, &lists, keep, &mut counter);
            }
            if let Some(frame) = fog_frame.as_ref().filter(|_| split) {
                ctx.record_fog_volumes(&mut enc, frame, view_id, &mut counter);
            }
            if let Some(frame) = sun_frame.as_ref() {
                ctx.record_sun_glow(&mut enc, frame, view_id, &mut counter);
            }
            for h in hooks.iter_mut() {
                h.finish(&hook_cx, &mut enc);
            }
        }
        self.hooks = hooks;
        if particle_refraction {
            self.record_particle_refraction(&mut enc, view_id, &mut counter);
        }
        if let Some(p) = self.profiler.as_ref() {
            p.mark(&mut enc, 2);
        }
        let present = self.views.get(&view_id).map(|v| (v.color.clone(), v.color_view.clone(), v.display_srgb.clone()));
        if let Some((color, color_view, display)) = present {
            let inputs = hdr::FrameInputs {
                color: &color,
                color_view: &color_view,
                display: &display,
                size: params.size,
                chain,
                fade: if cam.is_ortho || is_reflection { [0.0; 4] } else { opts.screen_fade },
                fade_modulate: opts.fade_modulate,
            };
            self.record_present(&mut enc, view_id, &inputs);
        }
        if outline_on
            && let Some(view) = self.views.get(&view_id)
            && let Some(bind_group) = view.bind_group.as_ref()
        {
            self.record_outline(&mut enc, view, &cam, &params, bind_group, &mut counter);
        }
        let sample = if let Some(p) = self.profiler.as_mut() {
            p.mark(&mut enc, 3);
            p.end(&mut enc)
        } else {
            None
        };
        let submitting = std::time::Instant::now();
        self.queue.submit([enc.finish()]);
        if let (Some(p), Some(i)) = (self.profiler.as_ref(), sample) {
            p.after_submit(i);
        }
        let ms = |a: std::time::Instant, b: std::time::Instant| b.duration_since(a).as_secs_f32() * 1000.0;
        if is_reflection {
            self.reflection_cpu_ms = ms(started, submitting);
        } else if profiled {
            self.frame_cpu = profile::CpuTimes {
                prepare_ms: ms(started, recording) - self.reflection_cpu_ms,
                record_ms: ms(recording, submitting),
                submit_ms: ms(submitting, std::time::Instant::now()),
                setup_ms,
                clusters_ms,
                shadow_prep_ms,
                models_ms,
            };
        }
        if let Some(o) = occ.as_mut() {
            o.after_submit();
            counter.chunks_skipped += o.occluded;
        }
        if let Some(view) = self.views.get_mut(&view_id) {
            view.draws = counter.draws;
            view.triangles = counter.triangles;
            view.counts = counter.view_counts();
            view.chunks_seen = counter.chunks_seen;
            view.chunks_skipped = counter.chunks_skipped;
            view.animated = animated;
            if occ.is_some() {
                view.occ = occ;
            }
            return &view.display_view;
        }
        &self.fallback_view
    }

    #[allow(clippy::too_many_arguments)]
    fn record_outline(
        &self,
        enc: &mut wgpu::CommandEncoder,
        view: &ViewState,
        cam: &Camera,
        params: &ViewParams,
        bind_group: &wgpu::BindGroup,
        n: &mut FrameCounter,
    ) {
        let (Some(pipes), Some(target)) = (self.outline_pipes.as_ref(), view.outline.as_ref()) else { return };
        let Some(any_material) = self.mat_gpu.first() else { return };
        {
            let mut pass = enc.begin_render_pass(&wgpu::RenderPassDescriptor {
                label: Some("hp-render outline mask"),
                color_attachments: &[Some(wgpu::RenderPassColorAttachment {
                    view: &target.view,
                    depth_slice: None,
                    resolve_target: None,
                    ops: wgpu::Operations {
                        load: wgpu::LoadOp::Clear(wgpu::Color::BLACK),
                        store: wgpu::StoreOp::Store,
                    },
                })],
                depth_stencil_attachment: None,
                timestamp_writes: None,
                occlusion_query_set: None,
                multiview_mask: None,
            });
            pass.set_bind_group(0, bind_group, &[]);
            pass.set_bind_group(1, &any_material.bind_group, &[]);
            let scene = &self.scene;
            let ranges: Vec<(u32, u32)> = self
                .sel_chunks
                .1
                .iter()
                .filter_map(|&c| scene.chunks.get(c as usize).and_then(Option::as_ref))
                .filter(|ch| !ch.is_edge() && ch.used_i > 0 && cam.aabb_visible(ch.aabb_min, ch.aabb_max))
                .map(|ch| (ch.idx.offset, ch.used_i))
                .collect();
            if !ranges.is_empty() {
                pass.set_pipeline(&pipes.mask_chunk);
                pass.set_vertex_buffer(0, self.arenas[ArenaKind::MeshVerts as usize].buffer.slice(..));
                pass.set_index_buffer(
                    self.arenas[ArenaKind::Indices as usize].buffer.slice(..),
                    wgpu::IndexFormat::Uint32,
                );
                for (first, count) in ranges {
                    pass.draw_indexed(first..first + count, 0, 0..1);
                    n.add(u64::from(count / 3));
                }
            }
            if params.show_models && self.instance_count > 0 {
                pass.set_pipeline(&pipes.mask_model);
                pass.set_vertex_buffer(1, self.instances.buffer.slice(..));
                for g in &self.instance_groups {
                    let Some(model) = self.models.get(&g.key) else { continue };
                    pass.set_vertex_buffer(0, model.vbuf.slice(..));
                    pass.set_vertex_buffer(2, model.tbuf.slice(..));
                    pass.set_index_buffer(model.ibuf.slice(..), wgpu::IndexFormat::Uint32);
                    for part in &model.parts {
                        pass.draw_indexed(part.first..part.first + part.count, 0, g.start..g.start + g.count);
                        n.add(0);
                    }
                }
            }
        }
        let mut pass = enc.begin_render_pass(&wgpu::RenderPassDescriptor {
            label: Some("hp-render outline"),
            color_attachments: &[Some(wgpu::RenderPassColorAttachment {
                view: &view.display_srgb,
                depth_slice: None,
                resolve_target: None,
                ops: wgpu::Operations { load: wgpu::LoadOp::Load, store: wgpu::StoreOp::Store },
            })],
            depth_stencil_attachment: None,
            timestamp_writes: None,
            occlusion_query_set: None,
            multiview_mask: None,
        });
        pass.set_pipeline(&pipes.outline);
        pass.set_bind_group(0, bind_group, &[]);
        pass.set_bind_group(1, &target.bind, &[]);
        pass.draw(0..3, 0..1);
        n.add(1);
    }

    fn ensure_outline(&mut self, id: u32) {
        if self.outline_pipes.is_none() {
            self.outline_pipes = Some(outline::OutlinePipelines::new(&self.device, &self.shared));
        }
        let Some(pipes) = self.outline_pipes.as_ref() else { return };
        let Some(view) = self.views.get_mut(&id) else { return };
        if view.outline.as_ref().is_none_or(|o| o.generation != view.generation) {
            view.outline = Some(pipes.target(&self.device, view.size, view.generation));
        }
    }

    fn take_occlusion(&mut self, id: u32) -> Option<occlusion::OcclusionView> {
        if self.occ_pipes.is_none() {
            self.occ_pipes = Some(occlusion::OcclusionPipelines::new(&self.device));
        }
        let pipes = self.occ_pipes.as_ref()?;
        let view = self.views.get_mut(&id)?;
        match view.occ.take() {
            Some(o) if o.matches(view.generation) => Some(o),
            _ => Some(occlusion::OcclusionView::new(
                &self.device,
                pipes,
                &view.depth_view,
                view.size,
                view.samples,
                view.generation,
            )),
        }
    }

    fn ensure_sky_pass(&mut self, id: u32) {
        let Some(view) = self.views.get(&id) else { return };
        let key = (self.scene_gen, view.cluster_gen);
        if view.sky_pass.as_ref().is_some_and(|(_, _, k)| *k == key) {
            return;
        }
        let buffer = match view.sky_pass.as_ref() {
            Some((b, _, _)) => b.clone(),
            None => self.device.create_buffer(&wgpu::BufferDescriptor {
                label: Some("hp-render sky pass uniform"),
                size: size_of::<ViewUniform>() as u64,
                usage: wgpu::BufferUsages::UNIFORM | wgpu::BufferUsages::COPY_DST,
                mapped_at_creation: false,
            }),
        };
        let bg = self.view_bind_group_with(&buffer, view);
        if let Some(view) = self.views.get_mut(&id) {
            view.sky_pass = Some((buffer, bg, key));
        }
    }

    fn update_selected_chunks(&mut self) {
        let key = (self.scene.selected_gen, self.scene.membership_gen);
        if self.sel_chunks.0 == key {
            return;
        }
        let chunks = self.scene.chunks_of(self.scene.selected.iter());
        self.sel_chunks = (key, chunks);
    }

    pub fn view_texture(&self, view_id: u32) -> Option<&wgpu::TextureView> {
        self.views.get(&view_id).map(|v| &v.display_view)
    }

    pub fn view_texture_generation(&self, view_id: u32) -> u64 {
        self.views.get(&view_id).map_or(0, |v| v.generation)
    }

    pub fn remove_view(&mut self, view_id: u32) {
        self.remove_particle_refraction_view(view_id);
        self.views.remove(&view_id);
        self.retained.remove(&view_id);
        self.forget_water_view(view_id);
        self.forget_fog_volume_view(view_id);
        self.forget_sun_glow_view(view_id);
        self.forget_tone_view(view_id);
    }

    pub fn snapshot_view(&self, view_id: u32) -> Option<(wgpu::Texture, wgpu::TextureView)> {
        let view = self.views.get(&view_id)?;
        let [w, h] = view.size;
        let extent = wgpu::Extent3d { width: w, height: h, depth_or_array_layers: 1 };
        let tex = self.device.create_texture(&wgpu::TextureDescriptor {
            label: Some("hp-render view snapshot"),
            size: extent,
            mip_level_count: 1,
            sample_count: 1,
            dimension: wgpu::TextureDimension::D2,
            format: LDR_FORMAT,
            usage: wgpu::TextureUsages::TEXTURE_BINDING | wgpu::TextureUsages::COPY_DST,
            view_formats: &[DISPLAY_FORMAT],
        });
        let mut enc =
            self.device.create_command_encoder(&wgpu::CommandEncoderDescriptor { label: Some("hp-render snapshot") });
        enc.copy_texture_to_texture(view.display.as_image_copy(), tex.as_image_copy(), extent);
        self.queue.submit([enc.finish()]);
        let display =
            tex.create_view(&wgpu::TextureViewDescriptor { format: Some(DISPLAY_FORMAT), ..Default::default() });
        Some((tex, display))
    }

    pub fn read_view_rgba8(&self, view_id: u32) -> Result<(u32, u32, Vec<u8>), RenderError> {
        let view = self.views.get(&view_id).ok_or(RenderError::UnknownView(view_id))?;
        let [w, h] = view.size;
        let row = w * 4;
        let padded = row.next_multiple_of(wgpu::COPY_BYTES_PER_ROW_ALIGNMENT);
        let buf = self.device.create_buffer(&wgpu::BufferDescriptor {
            label: Some("hp-render readback"),
            size: u64::from(padded) * u64::from(h),
            usage: wgpu::BufferUsages::MAP_READ | wgpu::BufferUsages::COPY_DST,
            mapped_at_creation: false,
        });
        let mut enc =
            self.device.create_command_encoder(&wgpu::CommandEncoderDescriptor { label: Some("hp-render readback") });
        enc.copy_texture_to_buffer(
            wgpu::TexelCopyTextureInfo {
                texture: &view.display,
                mip_level: 0,
                origin: wgpu::Origin3d::ZERO,
                aspect: wgpu::TextureAspect::All,
            },
            wgpu::TexelCopyBufferInfo {
                buffer: &buf,
                layout: wgpu::TexelCopyBufferLayout { offset: 0, bytes_per_row: Some(padded), rows_per_image: Some(h) },
            },
            wgpu::Extent3d { width: w, height: h, depth_or_array_layers: 1 },
        );
        self.queue.submit([enc.finish()]);
        let (tx, rx) = std::sync::mpsc::channel();
        buf.slice(..).map_async(wgpu::MapMode::Read, move |r| {
            let _ = tx.send(r);
        });
        self.device.poll(wgpu::PollType::wait_indefinitely()).map_err(|e| RenderError::Readback(e.to_string()))?;
        rx.recv()
            .map_err(|e| RenderError::Readback(e.to_string()))?
            .map_err(|e| RenderError::Readback(e.to_string()))?;
        let mut out = Vec::with_capacity((row * h) as usize);
        {
            let mapped = buf.slice(..).get_mapped_range().map_err(|e| RenderError::Readback(e.to_string()))?;
            for y in 0..h as usize {
                let start = y * padded as usize;
                if let Some(r) = mapped.get(start..start + row as usize) {
                    out.extend_from_slice(r);
                }
            }
        }
        buf.unmap();
        Ok((w, h, out))
    }

    pub fn set_game_hidden(&mut self, ids: ahash::AHashSet<ObjectId>) {
        let changed: Vec<ObjectId> = self.game_hidden.symmetric_difference(&ids).copied().collect();
        self.game_hidden = ids;
        for id in changed {
            let Some(old) = self.scene.flags(id) else { continue };
            let flags = if self.game_hidden.contains(&id) {
                old | ObjectFlags::GAME_HIDDEN
            } else {
                ObjectFlags(old.0 & !ObjectFlags::GAME_HIDDEN.0)
            };
            if flags != old {
                self.scene.set_flags(id, flags);
                self.content_gen += 1;
            }
        }
    }

    pub fn set_play_hidden(&mut self, ids: ahash::AHashSet<ObjectId>) {
        let added: Vec<ObjectId> = ids.difference(&self.play_hidden).copied().collect();
        let removed: Vec<ObjectId> = self.play_hidden.difference(&ids).copied().collect();
        let base: Vec<(ObjectId, ObjectFlags)> =
            removed.iter().filter_map(|id| self.play_base.get(id).map(|f| (*id, *f))).collect();
        for id in &added {
            if let Some(old) = self.scene.flags(*id) {
                self.play_base.insert(*id, old);
                self.scene.set_flags(*id, old | ObjectFlags::HIDDEN);
                self.content_gen += 1;
            }
        }
        self.play_hidden = ids;
        for (id, old) in base {
            self.play_base.remove(&id);
            self.scene.set_flags(id, old);
            self.content_gen += 1;
        }
    }

    pub fn install_pipeline_cache(&self, path: &std::path::Path) -> bool {
        pipeline_cache::install(&self.device, path)
    }

    pub fn prewarm_pipelines(&mut self) {
        if self.prewarm.is_some() || self.pipelines.iter().any(Option::is_some) {
            return;
        }
        let device = self.device.clone();
        let shared = self.shared.clone();
        let spawned = std::thread::Builder::new().name("hp-render-pipelines".to_owned()).spawn(move || {
            let (msaa, single) =
                rayon::join(|| PipelineSet::new(&device, &shared, 4), || PipelineSet::new(&device, &shared, 1));
            vec![(1, msaa), (0, single)]
        });
        match spawned {
            Ok(handle) => self.prewarm = Some(handle),
            Err(e) => log::warn!("hp-render: cannot start the pipeline thread: {e}"),
        }
    }

    pub fn set_quality(&mut self, quality: Quality) {
        if self.quality == quality {
            return;
        }
        let sun_changed = self.quality.sun_shadow_size != quality.sun_shadow_size;
        let local_changed = self.quality.local_shadow_size != quality.local_shadow_size;
        self.quality = quality;
        if sun_changed {
            self.shadow = None;
            self.scene_gen += 1;
        }
        if local_changed {
            self.local_shadows.reset_size();
            self.scene_gen += 1;
        }
        self.shadow_epoch += 1;
        self.content_gen += 1;
    }

    pub fn wants_another_frame(&self) -> bool {
        self.sun_followup
    }

    pub fn quality(&self) -> Quality {
        self.quality
    }

    pub fn frame_times(&self) -> profile::FrameTimes {
        profile::FrameTimes {
            cpu: self.frame_cpu,
            gpu: self.profiler.as_ref().and_then(|p| p.latest),
            reflection_gpu_ms: if self.reflection_active {
                self.profiler.as_ref().map_or(0.0, |p| p.latest_reflection)
            } else {
                0.0
            },
            reflection_cpu_ms: self.reflection_cpu_ms,
            gpu_supported: self.profiler.is_some(),
        }
    }

    pub fn view_counts(&self, view_id: u32) -> Option<ViewCounts> {
        self.views.get(&view_id).map(|v| v.counts)
    }

    pub fn stats(&self) -> RenderStats {
        let mut s = RenderStats {
            objects: self.scene.stored_objects,
            chunks: self.scene.live_chunks,
            textures: self.textures.len() as u32,
            retained_uploads: self.retained_uploads,
            ..Default::default()
        };
        let mut bytes = self.arenas.iter().map(|a| a.size).sum::<u64>()
            + self.states.size
            + self.instances.size
            + self.sprites.size
            + self.lights_buf.size
            + self.textures.values().map(|t| t.bytes).sum::<u64>()
            + self.models.values().map(|m| m.bytes).sum::<u64>()
            + self.mat_gpu.len() as u64 * size_of::<MaterialUniform>() as u64;
        if let Some(t) = &self.shadow {
            bytes += shadow::Layers::Sun.bytes(t.layers, self.quality.sun_shadow_size);
        }
        if let Some(t) = &self.sky_map {
            bytes += shadow::Layers::Sky.bytes(t.layers, u32::MAX);
        }
        bytes += self.local_shadows.bytes();
        s.shadowed_lights = self.local_shadows.ready_count() as u32;
        s.preview_lights = self.lights_cpu.len() as u32;
        s.lights_left_out = self.lights_left_out;
        s.crowded_clusters = self.crowded_clusters;
        for v in self.views.values() {
            s.draw_calls += v.draws;
            s.triangles += v.triangles;
            s.chunks_in_frustum += v.chunks_seen;
            s.chunks_occluded += v.chunks_skipped;
            bytes += v.target_bytes + v.overlay.size + v.clusters.size + v.light_indices.size;
        }
        s.retained_overlay_bytes = self.retained.values().map(|r| r.size).sum();
        s.static_lines = self.static_lines.values().map(|l| u64::from(l.count)).sum();
        s.content_generation = self.content_gen;
        bytes += self.static_lines.values().map(|l| l.buffer.size()).sum::<u64>();
        s.approx_gpu_bytes = bytes + s.retained_overlay_bytes + self.water_bytes();
        s
    }
}

#[derive(Clone, Copy)]
struct DrawCtx<'a> {
    r: &'a Renderer,
    ps: &'a PipelineSet,
    cam: &'a Camera,
    params: &'a ViewParams,
    opts: &'a ViewOptions,
    view: &'a ViewState,
    layout: OverlayLayout,
    retained: Option<&'a RetainedGpu>,
    hooks: &'a [Box<dyn PassHook>],
    hook_cx: &'a HookContext<'a>,
    bind_group: &'a wgpu::BindGroup,
    sky_holes: bool,
    water: Option<&'a water::WaterFrame>,
    packed: Option<&'a instance_cull::Packed>,
}

#[derive(Clone, Copy)]
struct ChunkDraw {
    material: u32,
    pass: MeshPass,
    two_sided: bool,
    first: u32,
    count: u32,
    dist: f32,
    chunk: u32,
    aabb_min: Vec3,
    aabb_max: Vec3,
}

impl ChunkDraw {
    fn candidate(&self) -> occlusion::CandidateGpu {
        occlusion::CandidateGpu {
            aabb_min: self.aabb_min.to_array(),
            first: self.first,
            aabb_max: self.aabb_max.to_array(),
            count: self.count,
            chunk: self.chunk,
            pad: [0; 3],
        }
    }
}

#[derive(Clone, Copy, PartialEq, Eq)]
enum BoxSet {
    All,
    Missing,
}

impl DrawCtx<'_> {
    fn chunk_visible(&self, ch: &scene::Chunk, n: &mut FrameCounter) -> bool {
        if !self.cam.aabb_visible(ch.aabb_min, ch.aabb_max) {
            return false;
        }
        n.chunks_seen += 1;
        let radius = self.opts.cull_radius;
        let far = !self.cam.is_ortho && radius > 0.0 && {
            let closest = self.cam.eye.clamp(ch.aabb_min, ch.aabb_max);
            closest.distance_squared(self.cam.eye) > radius * radius
        };
        if (ch.all_hidden && !self.r.probing) || far {
            n.chunks_skipped += 1;
            return false;
        }
        true
    }

    fn visible_edge_chunks(&self, n: &mut FrameCounter) -> Vec<(u32, u32)> {
        self.visible_vertex_chunks(&self.r.scene.edge_chunks, n)
    }

    fn visible_mark_chunks(&self, n: &mut FrameCounter) -> Vec<(u32, u32)> {
        if !self.opts.walk_marks || self.r.scene.mark_chunks.is_empty() {
            return Vec::new();
        }
        self.visible_vertex_chunks(&self.r.scene.mark_chunks, n)
    }

    fn visible_vertex_chunks(&self, list: &[u32], n: &mut FrameCounter) -> Vec<(u32, u32)> {
        let scene = &self.r.scene;
        let mut out: Vec<(u32, u32)> = Vec::with_capacity(list.len());
        for &c in list {
            let Some(Some(ch)) = scene.chunks.get(c as usize) else { continue };
            if ch.used_v == 0 || ch.vtx.len == 0 || !self.chunk_visible(ch, n) {
                continue;
            }
            out.push((ch.vtx.offset, ch.used_v));
        }
        out.sort_unstable_by_key(|r| r.0);
        let mut merged: Vec<(u32, u32)> = Vec::with_capacity(out.len());
        for (first, count) in out {
            match merged.last_mut() {
                Some(last) if last.0 + last.1 == first => last.1 += count,
                _ => merged.push((first, count)),
            }
        }
        merged
    }

    fn draw_marks(&self, pass: &mut wgpu::RenderPass<'_>, pipeline: usize, n: &mut FrameCounter) {
        let list = self.visible_mark_chunks(n);
        let Some(pipeline) = self.ps.marks.get(pipeline).filter(|_| !list.is_empty()) else { return };
        pass.set_pipeline(pipeline);
        pass.set_vertex_buffer(0, self.r.arenas[ArenaKind::EdgeVerts as usize].buffer.slice(..));
        for (first, count) in list {
            pass.draw(first..first + count, 0..1);
            n.add(u64::from(count / 3));
        }
    }

    fn draw_edges(
        &self,
        pass: &mut wgpu::RenderPass<'_>,
        pipeline: &wgpu::RenderPipeline,
        list: &[(u32, u32)],
        n: &mut FrameCounter,
    ) {
        if list.is_empty() {
            return;
        }
        pass.set_pipeline(pipeline);
        pass.set_vertex_buffer(0, self.r.arenas[ArenaKind::EdgeVerts as usize].buffer.slice(..));
        for &(first, count) in list {
            pass.draw(first..first + count, 0..1);
            n.add(0);
        }
    }

    fn draw_dots(&self, pass: &mut wgpu::RenderPass<'_>, list: &[(u32, u32)], n: &mut FrameCounter) {
        if list.is_empty() {
            return;
        }
        pass.set_pipeline(&self.ps.dots);
        pass.set_vertex_buffer(0, self.r.arenas[ArenaKind::EdgeVerts as usize].buffer.slice(..));
        for &(first, count) in list {
            pass.draw(0..6, first..first + count);
            n.add(u64::from(count) * 2);
        }
    }

    fn draw_boxes(
        &self,
        pass: &mut wgpu::RenderPass<'_>,
        pipeline: &wgpu::RenderPipeline,
        set: BoxSet,
        n: &mut FrameCounter,
    ) {
        if !self.params.show_models || self.r.instance_count == 0 || self.game_wire() > 0 {
            return;
        }
        pass.set_pipeline(pipeline);
        pass.set_vertex_buffer(0, self.r.instances.buffer.slice(..));
        match set {
            BoxSet::Missing => {
                let need_edges = self.cam.is_ortho;
                let only = self.collision_only();
                for g in &self.r.instance_groups {
                    let drawn = self.r.models.get(&g.key).is_some_and(|m| {
                        if only {
                            m.collision_hull.is_some() || m.no_collision
                        } else {
                            !need_edges || m.edges.is_some()
                        }
                    });
                    if !drawn {
                        pass.draw(0..24, g.start..g.start + g.count);
                        n.add(0);
                    }
                }
            }
            BoxSet::All => {
                pass.draw(0..24, 0..self.r.instance_count);
                n.add(0);
            }
        }
    }

    fn draw_model_edges(&self, pass: &mut wgpu::RenderPass<'_>, pipeline: &wgpu::RenderPipeline, n: &mut FrameCounter) {
        if !self.params.show_models || self.r.instance_count == 0 {
            return;
        }
        let mut bound = false;
        for g in &self.r.instance_groups {
            let Some(model) = self.r.models.get(&g.key) else { continue };
            let Some((edges, count)) = &model.edges else { continue };
            if !bound {
                pass.set_pipeline(pipeline);
                pass.set_vertex_buffer(1, self.r.instances.buffer.slice(..));
                bound = true;
            }
            pass.set_vertex_buffer(0, model.vbuf.slice(..));
            pass.set_index_buffer(edges.slice(..), wgpu::IndexFormat::Uint32);
            pass.draw_indexed(0..*count, 0, g.start..g.start + g.count);
            n.add(0);
        }
    }

    fn collision_only(&self) -> bool {
        self.opts.collision_only && !self.cam.is_ortho
    }

    fn draw_collision(&self, pass: &mut wgpu::RenderPass<'_>, n: &mut FrameCounter) {
        let only = self.collision_only();
        if !(self.opts.collision_models || only) || !self.params.show_models || self.r.instance_count == 0 {
            return;
        }
        if only {
            self.draw_collision_hulls(pass, n);
        }
        let mut bound = false;
        for g in &self.r.instance_groups {
            let Some((lines, count)) = self.r.models.get(&g.key).and_then(|m| m.collision.as_ref()) else { continue };
            if !bound {
                pass.set_pipeline(&self.ps.collision);
                pass.set_vertex_buffer(1, self.r.instances.buffer.slice(..));
                bound = true;
            }
            pass.set_vertex_buffer(0, lines.slice(..));
            pass.draw(0..*count, g.start..g.start + g.count);
            n.add(0);
        }
    }

    fn draw_collision_hulls(&self, pass: &mut wgpu::RenderPass<'_>, n: &mut FrameCounter) {
        let mut bound = false;
        for g in &self.r.instance_groups {
            let Some((tris, count)) = self.r.models.get(&g.key).and_then(|m| m.collision_hull.as_ref()) else {
                continue;
            };
            if !bound {
                pass.set_pipeline(&self.ps.collision_hull);
                pass.set_vertex_buffer(1, self.r.instances.buffer.slice(..));
                bound = true;
            }
            pass.set_vertex_buffer(0, tris.slice(..));
            pass.draw(0..*count, g.start..g.start + g.count);
            n.add(u64::from(*count / 3) * u64::from(g.count));
        }
        let mut bound = false;
        for g in &self.r.instance_groups {
            if !self.r.models.get(&g.key).is_some_and(|m| m.no_collision) {
                continue;
            }
            if !bound {
                pass.set_pipeline(&self.ps.box_no_collision);
                pass.set_vertex_buffer(0, self.r.instances.buffer.slice(..));
                bound = true;
            }
            pass.draw(0..24, g.start..g.start + g.count);
            n.add(0);
        }
    }

    fn draw_models_2d(&self, pass: &mut wgpu::RenderPass<'_>, selected: bool, n: &mut FrameCounter) {
        let ps = self.ps;
        let boxes = if selected { &ps.box_selected } else { &ps.box_unselected };
        let wire = match self.opts.models_2d {
            Models2d::Boxes => false,
            Models2d::Wireframe => true,
            Models2d::SelectedOnly => selected,
        };
        if wire {
            self.draw_boxes(pass, boxes, BoxSet::Missing, n);
            self.draw_model_edges(pass, &ps.model_edges[usize::from(selected)], n);
        } else {
            self.draw_boxes(pass, boxes, BoxSet::All, n);
        }
    }

    fn draw_static_lines(&self, pass: &mut wgpu::RenderPass<'_>, on_top: bool, n: &mut FrameCounter) {
        let mut ranges = Vec::new();
        for layer in self.r.static_lines.values() {
            let s = layer.style;
            let shown = if self.cam.is_ortho { s.in_2d } else { s.in_3d };
            if !s.visible || !shown || s.on_top != on_top || layer.count == 0 {
                continue;
            }
            overlay::visible_ranges(&layer.chunks, |min, max| self.cam.aabb_visible(min, max), &mut ranges);
            if ranges.is_empty() {
                continue;
            }
            pass.set_vertex_buffer(0, layer.buffer.slice(..));
            if s.width_px <= 1.5 {
                pass.set_pipeline(&self.ps.overlay_lines[usize::from(on_top)]);
                for &(first, count) in &ranges {
                    pass.draw(first..first + count, 0..1);
                    n.add(0);
                }
            } else {
                pass.set_pipeline(&self.ps.wide_lines[usize::from(on_top)]);
                pass.set_bind_group(1, &layer.width_bind, &[]);
                for &(first, count) in &ranges {
                    pass.draw(0..6, first / 2..(first + count) / 2);
                    n.add(u64::from(count));
                }
            }
        }
    }

    fn draw_overlay(&self, pass: &mut wgpu::RenderPass<'_>, on_top: bool, n: &mut FrameCounter) {
        if let Some(r) = self.retained
            && !r.layout.layer_is_empty(on_top)
        {
            let visible = |chunks: &[Chunk]| {
                let mut ranges = Vec::new();
                overlay::visible_ranges(chunks, |min, max| self.cam.aabb_visible(min, max), &mut ranges);
                ranges
            };
            let (lines, tris) = if on_top { (1, 3) } else { (0, 2) };
            let (lines, tris) = (visible(&r.chunks[lines]), visible(&r.chunks[tris]));
            self.draw_overlay_ranges(pass, &r.buffer, &r.layout, on_top, &lines, &tris, n);
        }
        let l = self.layout;
        if !l.layer_is_empty(on_top) {
            let (lines, tris) = l.layer(on_top);
            let ranges = |r: (u32, u32)| if r.1 > 0 { vec![r] } else { Vec::new() };
            self.draw_overlay_ranges(pass, &self.view.overlay.buffer, &l, on_top, &ranges(lines), &ranges(tris), n);
        }
    }

    #[allow(clippy::too_many_arguments)]
    fn draw_overlay_ranges(
        &self,
        pass: &mut wgpu::RenderPass<'_>,
        buf: &wgpu::Buffer,
        layout: &OverlayLayout,
        on_top: bool,
        lines: &[(u32, u32)],
        tris: &[(u32, u32)],
        n: &mut FrameCounter,
    ) {
        let points = if on_top { layout.points } else { 0 };
        if lines.is_empty() && tris.is_empty() && points == 0 {
            return;
        }
        let i = usize::from(on_top);
        pass.set_vertex_buffer(0, buf.slice(..));
        if !tris.is_empty() {
            pass.set_pipeline(&self.ps.overlay_tris[i]);
            for &(first, count) in tris {
                pass.draw(first..first + count, 0..1);
                n.add(u64::from(count / 3));
            }
        }
        if !lines.is_empty() {
            pass.set_pipeline(&self.ps.overlay_lines[i]);
            for &(first, count) in lines {
                pass.draw(first..first + count, 0..1);
                n.add(0);
            }
        }
        if points > 0 {
            pass.set_pipeline(&self.ps.points);
            pass.set_vertex_buffer(0, buf.slice(layout.points_offset..));
            pass.draw(0..6, 0..points);
            n.add(u64::from(points) * 2);
        }
    }

    fn draw_fill(&self, pass: &mut wgpu::RenderPass<'_>, n: &mut FrameCounter) {
        let scene = &self.r.scene;
        let Some(any_material) = self.r.mat_gpu.first() else { return };
        let mut ranges: Vec<(u32, u32)> = Vec::new();
        for &c in &self.r.sel_chunks.1 {
            let Some(Some(ch)) = scene.chunks.get(c as usize) else { continue };
            if ch.is_edge() || ch.used_i == 0 || ch.idx.len == 0 || !self.cam.aabb_visible(ch.aabb_min, ch.aabb_max) {
                continue;
            }
            ranges.push((ch.idx.offset, ch.used_i));
        }
        if ranges.is_empty() {
            return;
        }
        pass.set_pipeline(&self.ps.fill);
        pass.set_bind_group(1, &any_material.bind_group, &[]);
        pass.set_vertex_buffer(0, self.r.arenas[ArenaKind::MeshVerts as usize].buffer.slice(..));
        pass.set_index_buffer(self.r.arenas[ArenaKind::Indices as usize].buffer.slice(..), wgpu::IndexFormat::Uint32);
        for (first, count) in ranges {
            pass.draw_indexed(first..first + count, 0, 0..1);
            n.add(u64::from(count / 3));
        }
    }

    fn draw_background_images(&self, pass: &mut wgpu::RenderPass<'_>, n: &mut FrameCounter) {
        let ViewKind::Ortho { axis, .. } = self.params.kind else { return };
        let i = usize::from(self.view.samples > 1);
        let Some(pipeline) = self.r.bg_pipes.as_ref().and_then(|p| p.pipelines[i].as_ref()) else { return };
        let mut bound = false;
        for img in self.r.bg_images.iter().filter(|img| img.axis == axis) {
            if !bound {
                pass.set_pipeline(pipeline);
                bound = true;
            }
            pass.set_bind_group(1, &img.bind, &[]);
            pass.set_vertex_buffer(0, img.buffer.slice(..));
            pass.draw(0..6, 0..1);
            n.add(2);
        }
    }

    fn record_2d(&self, pass: &mut wgpu::RenderPass<'_>, n: &mut FrameCounter) {
        self.draw_background_images(pass, n);
        if self.params.grid.is_some() {
            pass.set_pipeline(&self.ps.grid);
            pass.draw(0..3, 0..1);
            n.add(1);
        }
        if self.opts.fill_selected_2d > 0.0 && !self.r.scene.selected.is_empty() {
            self.draw_fill(pass, n);
        }
        self.draw_marks(pass, 1, n);
        let edges = self.visible_edge_chunks(n);
        self.draw_edges(pass, &self.ps.edge_unselected, &edges, n);
        self.draw_models_2d(pass, false, n);
        self.draw_edges(pass, &self.ps.edge_selected, &edges, n);
        self.draw_models_2d(pass, true, n);
        if self.opts.sprites_2d_px > 0.0 {
            self.draw_sprites(pass, n);
        }
        let dots = self.opts.vertex_dots_min_zoom;
        if dots > 0.0 && self.cam.ppu >= dots {
            self.draw_dots(pass, &edges, n);
        }
        if self.r.scene.invalid > 0 {
            self.draw_edges(pass, &self.ps.edge_invalid, &edges, n);
        }
        self.draw_static_lines(pass, false, n);
        self.draw_overlay(pass, false, n);
        self.draw_static_lines(pass, true, n);
        self.draw_overlay(pass, true, n);
    }

    fn collect_chunks(&self, n: &mut FrameCounter) -> [Vec<ChunkDraw>; MESH_PASSES] {
        let scene = &self.r.scene;
        let mut lists: [Vec<ChunkDraw>; MESH_PASSES] = Default::default();
        for (mi, m) in scene.materials.iter().enumerate() {
            if m.chunks.is_empty() {
                continue;
            }
            let Some((pass, two_sided)) = classify_with(m.desc.as_ref(), self.opts.force_opaque, self.sky_holes) else {
                continue;
            };
            if (self.water.is_some() || self.r.reflecting.is_some()) && m.desc.is_some_and(|d| d.water.is_some()) {
                continue;
            }
            if self.editor_only(m.desc.as_ref()) {
                continue;
            }
            let translucent = matches!(pass, MeshPass::Translucent | MeshPass::Additive);
            for &c in &m.chunks {
                let Some(Some(ch)) = scene.chunks.get(c as usize) else { continue };
                if ch.used_i == 0 || ch.idx.len == 0 || !self.chunk_visible(ch, n) {
                    continue;
                }
                let dist =
                    if translucent { ((ch.aabb_min + ch.aabb_max) * 0.5 - self.cam.eye).length_squared() } else { 0.0 };
                lists[pass as usize].push(ChunkDraw {
                    material: mi as u32,
                    pass,
                    two_sided,
                    first: ch.idx.offset,
                    count: ch.used_i,
                    dist,
                    chunk: c,
                    aabb_min: ch.aabb_min,
                    aabb_max: ch.aabb_max,
                });
            }
        }
        lists
    }

    fn editor_only(&self, desc: Option<&MaterialDesc>) -> bool {
        self.r.probing && desc.is_some_and(|d| d.texture.is_none() || d.flags.contains(MaterialFlags::TOOL))
    }

    fn faded_chunks(&self) -> Vec<ChunkDraw> {
        let scene = &self.r.scene;
        if scene.translucent.is_empty() {
            return Vec::new();
        }
        let mut out = Vec::new();
        for c in scene.chunks_of(scene.translucent.iter()) {
            let Some(Some(ch)) = scene.chunks.get(c as usize) else { continue };
            if ch.is_edge() || ch.used_i == 0 || ch.idx.len == 0 || !self.cam.aabb_visible(ch.aabb_min, ch.aabb_max) {
                continue;
            }
            let desc = scene.materials.get(ch.key.material as usize).and_then(|m| m.desc);
            if (self.water.is_some() || self.r.reflecting.is_some()) && desc.is_some_and(|d| d.water.is_some()) {
                continue;
            }
            if self.editor_only(desc.as_ref()) {
                continue;
            }
            let Some((pass, two_sided)) = classify_with(desc.as_ref(), self.opts.force_opaque, self.sky_holes) else {
                continue;
            };
            if !matches!(pass, MeshPass::Opaque | MeshPass::AlphaTest) {
                continue;
            }
            let dist = ((ch.aabb_min + ch.aabb_max) * 0.5 - self.cam.eye).length_squared();
            out.push(ChunkDraw {
                material: ch.key.material,
                pass,
                two_sided,
                first: ch.idx.offset,
                count: ch.used_i,
                dist,
                chunk: c,
                aabb_min: ch.aabb_min,
                aabb_max: ch.aabb_max,
            });
        }
        out.sort_by(|a, b| b.dist.total_cmp(&a.dist));
        out
    }

    fn draw_chunks(&self, pass: &mut wgpu::RenderPass<'_>, list: &[ChunkDraw], faded: bool, n: &mut FrameCounter) {
        if list.is_empty() {
            return;
        }
        pass.set_vertex_buffer(0, self.r.arenas[ArenaKind::MeshVerts as usize].buffer.slice(..));
        pass.set_index_buffer(self.r.arenas[ArenaKind::Indices as usize].buffer.slice(..), wgpu::IndexFormat::Uint32);
        if !faded && self.depth_prepass() {
            let mut two: Option<bool> = None;
            for d in list.iter().filter(|d| d.pass == MeshPass::Opaque) {
                if two != Some(d.two_sided) {
                    pass.set_pipeline(&self.ps.prepass[usize::from(d.two_sided)]);
                    two = Some(d.two_sided);
                }
                pass.draw_indexed(d.first..d.first + d.count, 0, 0..1);
            }
        }
        let mut pipe: Option<(MeshPass, bool)> = None;
        let mut mat = u32::MAX;
        let mut i = 0;
        while i < list.len() {
            let d = list[i];
            let (first, mut count) = (d.first, d.count);
            let mut j = i + 1;
            while let Some(next) = list.get(j) {
                if next.material == d.material
                    && next.pass == d.pass
                    && next.two_sided == d.two_sided
                    && next.first == first + count
                {
                    count += next.count;
                    j += 1;
                } else {
                    break;
                }
            }
            if pipe != Some((d.pass, d.two_sided)) {
                let p = if faded {
                    &self.ps.chunk_faded[usize::from(d.pass == MeshPass::AlphaTest)][usize::from(d.two_sided)]
                } else if d.pass == MeshPass::Opaque && self.depth_prepass() {
                    &self.ps.chunk_equal[usize::from(d.two_sided)]
                } else {
                    &self.ps.chunk[d.pass as usize][usize::from(d.two_sided)]
                };
                pass.set_pipeline(p);
                pipe = Some((d.pass, d.two_sided));
            }
            if mat != d.material {
                if let Some(g) = self.r.mat_gpu.get(d.material as usize) {
                    pass.set_bind_group(1, &g.bind_group, &[]);
                    n.material();
                }
                mat = d.material;
            }
            pass.draw_indexed(first..first + count, 0, 0..1);
            n.world(u64::from(count / 3));
            i = j;
        }
    }

    fn depth_prepass(&self) -> bool {
        self.params.mode == RenderMode::Lit
            && !self.cam.is_ortho
            && !self.r.probing
            && !crate::profile::baseline("prepass")
    }

    fn game_wire(&self) -> u8 {
        if self.cam.is_ortho { 0 } else { self.opts.game_wireframe.min(3) }
    }

    fn draw_game_wire(
        &self,
        pass: &mut wgpu::RenderPass<'_>,
        lists: &[Vec<ChunkDraw>; MESH_PASSES],
        n: &mut FrameCounter,
    ) {
        let level = self.game_wire();
        if level == 0 {
            return;
        }
        let scene = &self.r.scene;
        let wired = |d: &&ChunkDraw| {
            scene
                .materials
                .get(d.material as usize)
                .is_none_or(|m| m.desc.as_ref().is_none_or(|desc| gamewire::wire_material(desc.flags)))
        };
        let draws: Vec<&ChunkDraw> = lists.iter().flatten().filter(wired).collect();
        let pipes = &self.ps.game_wire;
        let mut uncounted = FrameCounter::default();
        for set in 0..2 {
            if set == 1 {
                self.draw_entity_faces(pass, lists, &mut uncounted);
            }
            match (&pipes.world_tris, level) {
                (Some(tris), 1 | 2) => {
                    pass.set_pipeline(&tris[set][usize::from(level == 2)]);
                    pass.set_vertex_buffer(0, self.r.arenas[ArenaKind::MeshVerts as usize].buffer.slice(..));
                    pass.set_index_buffer(
                        self.r.arenas[ArenaKind::Indices as usize].buffer.slice(..),
                        wgpu::IndexFormat::Uint32,
                    );
                    for d in &draws {
                        pass.draw_indexed(d.first..d.first + d.count, 0, 0..1);
                        uncounted.add(0);
                    }
                }
                _ => {
                    if let Some(cache) = &self.r.game_outlines {
                        pass.set_pipeline(&pipes.world_outlines[set]);
                        pass.set_vertex_buffer(0, self.r.arenas[ArenaKind::MeshVerts as usize].buffer.slice(..));
                        pass.set_index_buffer(cache.buffer.buffer.slice(..), wgpu::IndexFormat::Uint32);
                        for d in &draws {
                            if let Some((first, count)) = cache.range(d.chunk) {
                                pass.draw_indexed(first..first + count, 0, 0..1);
                                uncounted.add(0);
                            }
                        }
                    }
                }
            }
        }
        self.draw_model_game_wire(pass, n);
        self.r.draw_detail_sprites(pass, self.view.samples, true, n);
    }

    fn draw_entity_faces(
        &self,
        pass: &mut wgpu::RenderPass<'_>,
        lists: &[Vec<ChunkDraw>; MESH_PASSES],
        n: &mut FrameCounter,
    ) {
        if self.r.scene.brush_entities == 0 {
            return;
        }
        let pipes = &self.ps.game_wire.entity_faces;
        pass.set_vertex_buffer(0, self.r.arenas[ArenaKind::MeshVerts as usize].buffer.slice(..));
        pass.set_index_buffer(self.r.arenas[ArenaKind::Indices as usize].buffer.slice(..), wgpu::IndexFormat::Uint32);
        let mut mat = u32::MAX;
        let mut pipe: Option<(bool, bool)> = None;
        for d in lists[MeshPass::Opaque as usize].iter().chain(&lists[MeshPass::AlphaTest as usize]) {
            let key = (d.pass == MeshPass::AlphaTest, d.two_sided);
            if pipe != Some(key) {
                pass.set_pipeline(&pipes[usize::from(key.0)][usize::from(key.1)]);
                pipe = Some(key);
            }
            if mat != d.material {
                if let Some(g) = self.r.mat_gpu.get(d.material as usize) {
                    pass.set_bind_group(1, &g.bind_group, &[]);
                }
                mat = d.material;
            }
            pass.draw_indexed(d.first..d.first + d.count, 0, 0..1);
            n.add(0);
        }
    }

    fn draw_model_game_wire(&self, pass: &mut wgpu::RenderPass<'_>, n: &mut FrameCounter) {
        let r = self.r;
        if !self.params.show_models || r.instance_count == 0 {
            return;
        }
        let pipes = &self.ps.game_wire;
        pass.set_pipeline(pipes.models.as_ref().unwrap_or(&pipes.model_edges));
        pass.set_vertex_buffer(1, r.instances.buffer.slice(..));
        for g in &r.instance_groups {
            let Some(m) = r.models.get(&g.key) else { continue };
            let instances = g.start..g.start + g.count;
            pass.set_vertex_buffer(0, m.vbuf.slice(..));
            if pipes.models.is_some() {
                pass.set_index_buffer(m.ibuf.slice(..), wgpu::IndexFormat::Uint32);
                for part in m.parts.iter().chain(&m.lod_parts) {
                    pass.draw_indexed(part.first..part.first + part.count, 0, instances.clone());
                    if part.lod == 0 {
                        n.model(u64::from(part.count / 3) * u64::from(g.count), g.count);
                    }
                }
            } else if let Some((edges, count)) = &m.edges {
                pass.set_index_buffer(edges.slice(..), wgpu::IndexFormat::Uint32);
                pass.draw_indexed(0..*count, 0, instances);
                n.model(0, g.count);
            }
        }
    }

    fn draw_models(&self, pass: &mut wgpu::RenderPass<'_>, wanted: &[MeshPass], faded: bool, n: &mut FrameCounter) {
        let r = self.r;
        if !self.params.show_models || r.instance_count == 0 || self.game_wire() > 0 || self.collision_only() {
            return;
        }
        let mut bound_instances = false;
        let mut pipe: Option<(MeshPass, bool)> = None;
        let (source, groups) = r.instance_source(self.packed);
        for g in groups {
            let Some(model) = r.models.get(&g.key) else { continue };
            let mut bound_model = false;
            for part in model.parts.iter().chain(&model.lod_parts) {
                if g.lod.is_some_and(|lod| lod != part.lod) {
                    continue;
                }
                let desc = r.scene.materials.get(part.material as usize).and_then(|m| m.desc);
                let Some((p, two)) = classify_with(desc.as_ref(), self.opts.force_opaque, false) else { continue };
                if !wanted.contains(&p) {
                    continue;
                }
                if !bound_instances {
                    pass.set_vertex_buffer(1, source);
                    bound_instances = true;
                }
                if !bound_model {
                    pass.set_vertex_buffer(0, model.vbuf.slice(..));
                    pass.set_vertex_buffer(2, model.tbuf.slice(..));
                    pass.set_index_buffer(model.ibuf.slice(..), wgpu::IndexFormat::Uint32);
                    bound_model = true;
                }
                if pipe != Some((p, two)) {
                    let pipeline = if faded {
                        &self.ps.model_faded[usize::from(p == MeshPass::AlphaTest)][usize::from(two)]
                    } else {
                        &self.ps.model[p as usize][usize::from(two)]
                    };
                    pass.set_pipeline(pipeline);
                    pipe = Some((p, two));
                }
                if let Some(mg) = r.mat_gpu.get(part.material as usize) {
                    pass.set_bind_group(1, &mg.bind_group, &[]);
                    n.material();
                }
                pass.draw_indexed(part.first..part.first + part.count, 0, g.start..g.start + g.count);
                if g.lod.is_some() || part.lod == 0 {
                    n.model(u64::from(part.count / 3) * u64::from(g.count), g.count);
                }
            }
        }
    }

    fn draw_sprites(&self, pass: &mut wgpu::RenderPass<'_>, n: &mut FrameCounter) {
        let r = self.r;
        if !self.params.show_sprites {
            return;
        }
        let layers = (!self.cam.is_ortho).then_some(&r.sprite_layers).into_iter().flat_map(|l| l.values());
        let lists = std::iter::once((&r.sprites, &r.sprite_groups)).chain(layers.map(|l| (&l.buffer, &l.groups)));
        let mut bound = false;
        for (buffer, groups) in lists {
            if groups.is_empty() {
                continue;
            }
            if !bound {
                pass.set_pipeline(&self.ps.sprite);
                bound = true;
            }
            pass.set_vertex_buffer(0, buffer.buffer.slice(..));
            for g in groups {
                if let Some(mg) = r.mat_gpu.get(g.key as usize) {
                    pass.set_bind_group(1, &mg.bind_group, &[]);
                    pass.draw(0..6, g.start..g.start + g.count);
                    n.add(u64::from(g.count) * 2);
                    n.counts.sprites += g.count;
                }
            }
        }
    }

    fn draw_sky_background(&self, pass: &mut wgpu::RenderPass<'_>, sky: Option<&SkyView>, n: &mut FrameCounter) {
        let Some(sky) = sky else { return };
        let bind = match self.r.hdr_sky_bind(self.opts, self.params.mode, self.cam.is_ortho) {
            Some(bind) => bind,
            None => {
                let Some((faces, bind)) = self.r.sky_bind.as_ref() else { return };
                if *faces != sky.faces || faces.iter().all(Option::is_none) {
                    return;
                }
                bind
            }
        };
        pass.set_pipeline(&self.ps.sky);
        pass.set_bind_group(1, bind, &[]);
        pass.draw(0..3, 0..1);
        n.add(1);
    }

    fn record_sky_3d(&self, pass: &mut wgpu::RenderPass<'_>, n: &mut FrameCounter) {
        let lists = self.collect_chunks(&mut FrameCounter::default());
        let [opaque, alpha_test, translucent, additive, modulate] = &lists;
        self.draw_chunks(pass, opaque, false, n);
        self.draw_chunks(pass, alpha_test, false, n);
        self.draw_models(pass, &[MeshPass::Opaque, MeshPass::AlphaTest], false, n);
        self.record_compiled_sky(pass, false, n);
        self.draw_chunks(pass, modulate, false, n);
        self.draw_models(pass, &[MeshPass::Modulate], false, n);
        let mut sorted: Vec<ChunkDraw> = translucent.clone();
        sorted.extend_from_slice(additive);
        sorted.sort_by(|a, b| b.dist.total_cmp(&a.dist));
        self.draw_chunks(pass, &sorted, false, n);
        self.record_compiled_sky(pass, true, n);
        self.draw_models(pass, &[MeshPass::Translucent, MeshPass::Additive], false, n);
        self.draw_game_wire(pass, &lists, n);
    }

    fn run_hooks(&self, pass: &mut wgpu::RenderPass<'_>, translucent: bool) {
        if self.hooks.is_empty() {
            return;
        }
        for h in self.hooks {
            if translucent {
                h.draw_translucent(self.hook_cx, pass);
            } else {
                h.draw_opaque(self.hook_cx, pass);
            }
        }
        pass.set_bind_group(0, self.bind_group, &[]);
    }

    fn record_3d_opaque(
        &self,
        pass: &mut wgpu::RenderPass<'_>,
        lists: &[Vec<ChunkDraw>; MESH_PASSES],
        indirect: Option<(&[ChunkDraw], &wgpu::Buffer)>,
        n: &mut FrameCounter,
    ) {
        if self.params.mode == RenderMode::Wireframe {
            return;
        }
        match indirect {
            Some((cands, args)) => self.draw_chunks_indirect(pass, cands, args, n),
            None => {
                self.draw_chunks(pass, &lists[0], false, n);
                self.draw_chunks(pass, &lists[1], false, n);
            }
        }
        self.draw_models(pass, &[MeshPass::Opaque, MeshPass::AlphaTest], false, n);
    }

    fn record_3d_rest(
        &self,
        pass: &mut wgpu::RenderPass<'_>,
        lists: &[Vec<ChunkDraw>; MESH_PASSES],
        indirect: Option<(&[ChunkDraw], &wgpu::Buffer)>,
        n: &mut FrameCounter,
    ) {
        self.record_3d_rest_solid(pass, lists, indirect, n);
        self.record_3d_rest_blended(pass, lists, n);
    }

    fn record_3d_rest_solid(
        &self,
        pass: &mut wgpu::RenderPass<'_>,
        _lists: &[Vec<ChunkDraw>; MESH_PASSES],
        indirect: Option<(&[ChunkDraw], &wgpu::Buffer)>,
        n: &mut FrameCounter,
    ) {
        let mode = self.params.mode;
        if self.r.probing {
            if let Some((cands, args)) = indirect {
                self.draw_chunks_indirect(pass, cands, args, n);
            }
            self.run_hooks(pass, false);
            return;
        }
        let edges = self.visible_edge_chunks(&mut FrameCounter::default());
        if mode == RenderMode::Wireframe {
            self.draw_marks(pass, 2, n);
        }
        if mode == RenderMode::Wireframe {
            self.draw_edges(pass, &self.ps.edge_unselected, &edges, n);
            self.draw_boxes(pass, &self.ps.box_unselected, BoxSet::All, n);
            self.run_hooks(pass, false);
        } else {
            if let Some((cands, args)) = indirect {
                self.draw_chunks_indirect(pass, cands, args, n);
            }
            self.draw_boxes(pass, &self.ps.box_depth, BoxSet::Missing, n);
            if self.opts.model_distance > 0.0 {
                self.draw_boxes(pass, &self.ps.box_far, BoxSet::All, n);
            }
            self.run_hooks(pass, false);
            self.draw_marks(pass, 0, n);
            if mode == RenderMode::TexturedWire {
                self.draw_edges(pass, &self.ps.edge_depth, &edges, n);
            }
            if self.r.scene.always_edges > 0 {
                self.draw_edges(pass, &self.ps.edge_always, &edges, n);
            }
        }
    }

    fn record_3d_rest_blended(
        &self,
        pass: &mut wgpu::RenderPass<'_>,
        lists: &[Vec<ChunkDraw>; MESH_PASSES],
        n: &mut FrameCounter,
    ) {
        let mode = self.params.mode;
        let edges = self.visible_edge_chunks(&mut FrameCounter::default());
        if mode != RenderMode::Wireframe {
            self.draw_chunks(pass, &self.faded_chunks(), true, n);
            if self.r.translucent_instances {
                self.draw_models(pass, &[MeshPass::Opaque, MeshPass::AlphaTest], true, n);
            }
            self.draw_chunks(pass, &lists[MeshPass::Modulate as usize], false, n);
            self.draw_models(pass, &[MeshPass::Modulate], false, n);
            let mut sorted: Vec<ChunkDraw> = lists[2].clone();
            sorted.extend_from_slice(&lists[3]);
            sorted.sort_by(|a, b| b.dist.total_cmp(&a.dist));
            self.draw_chunks(pass, &sorted, false, n);
            self.draw_models(pass, &[MeshPass::Translucent, MeshPass::Additive], false, n);
            if self.game_wire() == 0 {
                self.r.draw_detail_sprites(pass, self.view.samples, false, n);
            }
            let water = self.water.map(|w| &w.draws[..]).unwrap_or_default();
            if water.is_empty() {
                self.draw_game_wire(pass, lists, n);
            } else {
                let mut with_water = lists.clone();
                with_water[MeshPass::Translucent as usize].extend_from_slice(water);
                self.draw_game_wire(pass, &with_water, n);
            }
        }
        self.draw_sprites(pass, n);
        self.run_hooks(pass, true);
        if self.r.probing {
            return;
        }
        self.r.draw_particles(pass, self.view.samples, n);
        self.draw_collision(pass, n);
        self.draw_overlay(pass, false, n);
        self.draw_static_lines(pass, false, n);
        self.draw_edges(pass, &self.ps.edge_selected, &edges, n);
        self.draw_boxes(pass, &self.ps.box_selected, BoxSet::All, n);
        if self.r.scene.invalid > 0 {
            self.draw_edges(pass, &self.ps.edge_invalid, &edges, n);
        }
        self.draw_static_lines(pass, true, n);
        self.draw_overlay(pass, true, n);
    }

    fn draw_chunks_indirect(
        &self,
        pass: &mut wgpu::RenderPass<'_>,
        list: &[ChunkDraw],
        args: &wgpu::Buffer,
        n: &mut FrameCounter,
    ) {
        if list.is_empty() {
            return;
        }
        pass.set_vertex_buffer(0, self.r.arenas[ArenaKind::MeshVerts as usize].buffer.slice(..));
        pass.set_index_buffer(self.r.arenas[ArenaKind::Indices as usize].buffer.slice(..), wgpu::IndexFormat::Uint32);
        if self.depth_prepass() {
            let mut two: Option<bool> = None;
            for (i, d) in list.iter().enumerate().filter(|(_, d)| d.pass == MeshPass::Opaque) {
                if two != Some(d.two_sided) {
                    pass.set_pipeline(&self.ps.prepass[usize::from(d.two_sided)]);
                    two = Some(d.two_sided);
                }
                pass.draw_indexed_indirect(args, i as u64 * occlusion::ARGS_BYTES);
            }
        }
        let mut pipe: Option<(MeshPass, bool)> = None;
        let mut mat = u32::MAX;
        for (i, d) in list.iter().enumerate() {
            if pipe != Some((d.pass, d.two_sided)) {
                let pipeline = if d.pass == MeshPass::Opaque && self.depth_prepass() {
                    &self.ps.chunk_equal[usize::from(d.two_sided)]
                } else {
                    &self.ps.chunk[d.pass as usize][usize::from(d.two_sided)]
                };
                pass.set_pipeline(pipeline);
                pipe = Some((d.pass, d.two_sided));
            }
            if mat != d.material {
                if let Some(g) = self.r.mat_gpu.get(d.material as usize) {
                    pass.set_bind_group(1, &g.bind_group, &[]);
                    n.material();
                }
                mat = d.material;
            }
            pass.draw_indexed_indirect(args, i as u64 * occlusion::ARGS_BYTES);
            n.world(u64::from(d.count / 3));
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn the_star_layer_follows_the_sky_paint_keys() {
        let off = SkyPaint { draw_stars: false, star_texture: Some(TexId::WHITE), ..SkyPaint::BLACK };
        assert_eq!(star_uniform(&off), [0.0; 4]);
        let one = SkyPaint {
            draw_stars: true,
            star_texture: Some(TexId::WHITE),
            star_scale: 2.0,
            star_fade: 0.5,
            star_speed: 0.03,
            ..SkyPaint::BLACK
        };
        let u = star_uniform(&one);
        assert_eq!((u[0], u[1], u[2]), (0.5, 2.0, 0.03));
        assert!((u[3] - STAR_GAIN).abs() < 1e-6);
        let three = star_uniform(&SkyPaint { star_layers: 3, ..one });
        assert!((three[3] - STAR_GAIN * 2.25).abs() < 1e-5, "each layer adds half again like the game");
        assert_eq!(star_uniform(&SkyPaint { star_texture: None, ..one }), [0.0; 4]);
    }

    #[test]
    fn light_backgrounds_are_detected() {
        assert!(!light_background([20.0 / 255.0, 20.0 / 255.0, 22.0 / 255.0]));
        assert!(light_background([236.0 / 255.0, 236.0 / 255.0, 232.0 / 255.0]));
        assert!(!light_background([0.2, 0.3, 1.0]));
        assert!(!light_background([f32::NAN, 0.0, 0.0]));
    }

    #[test]
    fn renderer_is_send_and_sync() {
        fn check<T: Send + Sync>() {}
        check::<Renderer>();
    }

    #[test]
    fn classify_materials() {
        let d = |flags| MaterialDesc { flags, ..Default::default() };
        assert_eq!(classify(None), Some((MeshPass::Opaque, false)));
        assert_eq!(classify(Some(&d(MaterialFlags::TOOL))), Some((MeshPass::Translucent, false)));
        assert_eq!(
            classify(Some(&d(MaterialFlags::ADDITIVE | MaterialFlags::TWO_SIDED))),
            Some((MeshPass::Additive, true))
        );
        assert_eq!(classify(Some(&d(MaterialFlags::ALPHA_TEST))), Some((MeshPass::AlphaTest, false)));
        assert_eq!(classify(Some(&d(MaterialFlags::NODRAW_IN_3D))), None);
    }

    #[test]
    fn grid_adapts_to_zoom() {
        let g = GridParams { size: 16.0, major_every: 8, show_1024: true, opacity: 1.0, cells: true };
        let (a, b) = grid_uniform(&g, 1.0);
        assert_eq!(a[0], 16.0);
        assert!(a[1] > 0.9, "16 px minor lines are visible");
        assert_eq!(a[2], 128.0);
        assert_eq!(b[0], 1.0);
        let (a, b) = grid_uniform(&g, 0.002);
        assert_eq!(a[1], 0.0);
        assert!(a[2] * 0.002 >= 4.0);
        assert_eq!(b[0], 0.0);
        let (a, _) = grid_uniform_min(&g, 1.0, 32.0);
        assert_eq!(a[1], 0.0);
        assert!(a[2] >= 32.0);
        let (a, _) = grid_uniform_min(&g, 1.0, f32::NAN);
        assert!(a[1] > 0.9, "a broken limit falls back to 4 px");
        let bad = GridParams { size: f32::NAN, major_every: 0, show_1024: false, opacity: f32::INFINITY, cells: true };
        let (a, b) = grid_uniform(&bad, 1.0);
        assert!(a.iter().chain(b.iter()).all(|v| v.is_finite()));
        let bare = GridParams { cells: false, ..g };
        let (a, b) = grid_uniform(&bare, 1.0);
        assert_eq!((a[1], a[3]), (0.0, 0.0), "no cell lines");
        assert_eq!((b[0], b[1]), (1.0, 1.0), "the 1024 lines and the axes stay");
    }

    #[test]
    fn material_uniform_sanitizes() {
        let u = material_uniform(
            Some(&MaterialDesc {
                tex_size: [0.0, f32::NAN],
                tint: [f32::NAN, 2.0, 0.5, f32::NAN],
                flags: MaterialFlags::TOOL,
                ..Default::default()
            }),
            u32::MAX,
        );
        assert!(u.inv_tex_size.iter().all(|v| v.is_finite()));
        assert!(u.tint.iter().all(|v| v.is_finite()));
        assert_eq!(u.alpha_scale, TOOL_ALPHA);
        assert_eq!(u.bump_flags, 0);
        assert_eq!(u.env_flags, 0, "no reflection without $envmap");
    }

    #[test]
    fn envmap_uniform_follows_the_material() {
        let env = EnvmapDesc {
            tint: [f32::NAN, 2.0, 0.25],
            contrast: 3.0,
            saturation: [0.5; 3],
            mask: Some(TexId::WHITE),
            mask_uv: [2.0, f32::NAN, 0.5, 0.0],
            alpha_mask: EnvmapAlphaMask::InverseBase,
            fresnel: 0.25,
            light_scale: 0.5,
            no_envmap: [true, false],
            ..Default::default()
        };
        let u = material_uniform(Some(&MaterialDesc { envmap: Some(env), ..Default::default() }), 7);
        assert_eq!(u.env_cube, 7);
        assert_eq!(u.env_flags, ENV_ON | ENV_MASK_RGB | ENV_BASE_INV | ENV_NO_TEX1 | ENV_FRESNEL);
        assert_eq!(u.env_tint, [1.0, 2.0, 0.25, 1.0], "tint sanitized, contrast clamped");
        assert_eq!(u.env_sat, [0.5, 0.5, 0.5, 0.5]);
        assert_eq!(u.env_mask_uv, [2.0, 1.0, 0.5, 0.0]);
        assert_eq!(u.env_fresnel[0], 0.25);
        let phong = EnvmapDesc {
            mask: Some(TexId::WHITE),
            mask_alpha: true,
            alpha_mask: EnvmapAlphaMask::Base,
            phong_fresnel: 1.0,
            ..Default::default()
        };
        let u = material_uniform(Some(&MaterialDesc { envmap: Some(phong), ..Default::default() }), u32::MAX);
        assert_eq!(u.env_flags, ENV_ON | ENV_MASK_ALPHA | ENV_BASE | ENV_PHONG);
        assert_eq!(u.env_ranges, [0.0, 0.5, 1.0, 0.0]);
        let water = MaterialDesc {
            envmap: Some(EnvmapDesc::default()),
            water: Some(WaterDesc::default()),
            ..Default::default()
        };
        assert_eq!(material_uniform(Some(&water), u32::MAX).env_flags, 0);
    }

    #[test]
    fn bump_uniform_is_sanitized() {
        let bump = BumpDesc {
            texture: TexId::WHITE,
            texture2: Some(TexId::WHITE),
            blend4: None,
            ssbump: true,
            diffuse: true,
            uv: [f32::NAN, 0.0, f32::INFINITY, 0.25],
            scroll: [f32::NAN, 1000.0],
        };
        let u = material_uniform(Some(&MaterialDesc { bump: Some(bump), ..Default::default() }), u32::MAX);
        assert_eq!(u.bump_flags, BUMP_ON | BUMP_SS | BUMP_TWO);
        assert_eq!(u.bump_uv, [1.0, 1.0, 0.0, 0.25]);
        assert_eq!(u.bump_scroll, [0.0, 100.0]);
        let plain = material_uniform(
            Some(&MaterialDesc { bump: Some(BumpDesc::new(TexId::WHITE)), ..Default::default() }),
            u32::MAX,
        );
        assert_eq!(plain.bump_flags, BUMP_ON);
        assert_eq!(plain.bump_layers, 0);
        let reflection_only = material_uniform(
            Some(&MaterialDesc { bump: Some(BumpDesc { diffuse: false, ..bump }), ..Default::default() }),
            u32::MAX,
        );
        assert_eq!(reflection_only.bump_flags, BUMP_ON | BUMP_SS | BUMP_TWO | BUMP_NO_DIFFUSE);
    }

    #[test]
    fn four_way_blend_parameters_reach_the_uniform() {
        let f = FourWayDesc {
            lum: [[0.1, 0.5], [f32::NAN, 0.2], [0.0, 1.0], [9.0, 0.0]],
            blend: [[0.0, 1.0], [0.8, 0.9], [0.75, 1.0], [0.9, f32::INFINITY]],
            lum_blend_factor: [0.0, 0.5, 0.7, 1.0],
            uv_scale: [[1.0, 1.0], [2.0, 3.0], [0.0, f32::NAN], [0.5, 0.5]],
            modulate_2x: [false, true, false, false],
        };
        let u = material_uniform(Some(&MaterialDesc { four_way: Some(f), ..Default::default() }), u32::MAX);
        assert_eq!(u.fw_lum_start, [0.1, 0.0, 0.0, 4.0]);
        assert_eq!(u.fw_lum_end, [0.5, 0.2, 1.0, 0.0]);
        assert_eq!(u.fw_blend_start, [0.0, 0.8, 0.75, 0.9]);
        assert_eq!(u.fw_blend_end, [1.0, 0.9, 1.0, 1.0]);
        assert_eq!(u.fw_lum_factor, [0.0, 0.5, 0.7, 1.0]);
        assert_eq!(u.fw_mod2x, [0.0, 1.0, 0.0, 0.0]);
        assert_eq!(u.fw_uv12, [1.0, 1.0, 2.0, 3.0]);
        assert_eq!(u.fw_uv34, [1.0, 1.0, 0.5, 0.5]);
        let plain = material_uniform(Some(&MaterialDesc::default()), u32::MAX);
        assert_eq!((plain.fw_lum_end, plain.fw_uv12), ([1.0; 4], [1.0; 4]), "defaults when the shader has none");
    }

    #[test]
    fn four_way_blend_bump_uniform() {
        let bump = BumpDesc {
            texture2: Some(TexId::WHITE),
            blend4: Some([0.0, 1.0, f32::NAN, 0.5]),
            ..BumpDesc::new(TexId::WHITE)
        };
        let four = MaterialDesc {
            bump: Some(bump),
            texture2: Some(TexId::WHITE),
            texture3: Some(TexId::WHITE),
            texture4: Some(TexId::WHITE),
            ..Default::default()
        };
        let u = material_uniform(Some(&four), u32::MAX);
        assert_eq!(u.bump_flags, BUMP_ON | BUMP_TWO | BUMP_FOUR);
        assert_eq!(u.bump_layers, 255 << 8 | 128 << 24);
        let wvt = material_uniform(Some(&MaterialDesc { texture3: None, ..four }), u32::MAX);
        assert_eq!(wvt.bump_flags, BUMP_ON | BUMP_TWO);
        assert_eq!(pack_bump_layers([2.0, -1.0, 0.0, f32::INFINITY]), 255);
    }
}
