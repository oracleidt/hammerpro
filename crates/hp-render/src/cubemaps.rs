use std::time::{Duration, Instant};

use ahash::{HashMap, HashMapExt};
use glam::Vec3;

use crate::parallax::ParallaxBox;
use crate::{
    CubemapId, CubemapImage, CubemapProbe, CubemapStats, NO_CUBEMAP, ObjectId, Overlay, RenderError, RenderMode,
    Renderer, TexId, ViewKind, ViewOptions, ViewParams,
};

pub(crate) const CUBE_FORMAT: wgpu::TextureFormat = wgpu::TextureFormat::Rgba16Float;
pub const MAX_RESOLUTION: u32 = 256;
pub const DEFAULT_PROBE_SIZE: u32 = 128;
const MIN_RESOLUTION: u32 = 16;
pub const MEMORY_BUDGET: u64 = 160 << 20;
pub const LOW_MEMORY_BUDGET: u64 = 40 << 20;
pub(crate) const PROBE_VIEW: u32 = 0xffff_fc00;
const SETTLE: Duration = Duration::from_millis(750);
const FRAME_SHARE: f32 = 0.15;
const MOTION_QUIET: Duration = Duration::from_millis(400);
const PROBE_CULL_RADIUS: f32 = 16_384.0;
const PROBE_MODEL_DISTANCE: f32 = 4_096.0;
const MAX_SLOTS: usize = 1100;
const TEXEL_BYTES: u64 = 8;

const FACES: [([f32; 3], [f32; 3]); 6] = [
    ([1.0, 0.0, 0.0], [0.0, 1.0, 0.0]),
    ([-1.0, 0.0, 0.0], [0.0, 1.0, 0.0]),
    ([0.0, 1.0, 0.0], [0.0, 0.0, -1.0]),
    ([0.0, -1.0, 0.0], [0.0, 0.0, 1.0]),
    ([0.0, 0.0, 1.0], [0.0, 1.0, 0.0]),
    ([0.0, 0.0, -1.0], [0.0, 1.0, 0.0]),
];

const SKY_EYE: [f32; 3] = [3.0e6, 3.0e6, 3.0e6];

#[derive(Clone, Debug)]
enum Content {
    Free,
    Sky,
    Probe(CubemapProbe),
    Image { size: u32, texels: Vec<[f32; 3]>, origin: Option<Vec3> },
    Texture { texture: TexId, size: u32 },
}

#[derive(Clone, Debug)]
struct Slot {
    content: Content,
    ready: bool,
    scale: f32,
    dirty: bool,
    parallax: Option<ParallaxBox>,
}

impl Slot {
    fn free() -> Self {
        Slot { content: Content::Free, ready: false, scale: 1.0, dirty: false, parallax: None }
    }

    fn origin(&self) -> Option<Vec3> {
        match &self.content {
            Content::Probe(p) => Some(Vec3::from(p.origin)),
            Content::Image { origin, .. } => *origin,
            _ => None,
        }
    }

    fn wanted_size(&self) -> u32 {
        match &self.content {
            Content::Probe(p) => probe_size(p.size),
            Content::Image { size, .. } => *size,
            Content::Texture { size, .. } => *size,
            Content::Sky => 64,
            Content::Free => 0,
        }
    }
}

pub(crate) fn probe_size(size: u32) -> u32 {
    if size == 0 { DEFAULT_PROBE_SIZE } else { size.clamp(1, MAX_RESOLUTION).next_power_of_two() }
}

#[derive(Clone, Copy, Debug, PartialEq)]
struct Source {
    view: u32,
    params: ViewParams,
    options: ViewOptions,
}

#[derive(Clone, Copy, Debug, PartialEq)]
struct BakeKey {
    mode: RenderMode,
    background: [u32; 3],
    show_models: bool,
    options: ViewOptions,
    lighting: (bool, u8, u32),
    sky: Option<crate::SkyView>,
    hdr_sky: bool,
    res: u32,
    epoch: u64,
}

struct CubeGpu {
    texture: wgpu::Texture,
    view: wgpu::TextureView,
    info: wgpu::Buffer,
    capacity: u32,
    res: u32,
    mips: u32,
}

struct Staging {
    texture: wgpu::Texture,
    res: u32,
    mips: u32,
}

struct Blit {
    pipeline: wgpu::RenderPipeline,
    layout: wgpu::BindGroupLayout,
}

#[derive(Clone, Copy, Debug)]
struct Current {
    slot: u16,
    faces: u8,
}

pub(crate) struct Cubes {
    slots: Vec<Slot>,
    gpu: CubeGpu,
    info_dirty: bool,
    staging: Option<Staging>,
    blit: Option<Blit>,
    current: Option<Current>,
    noted: Option<Source>,
    source: Option<Source>,
    drawn_key: Option<BakeKey>,
    pending: Option<(BakeKey, Instant)>,
    auto: bool,
    pub(crate) epoch: u64,
    pub(crate) instances_hash: u64,
    model_candidates: Option<Vec<u16>>,
    textures: HashMap<u32, u16>,
    faces_rendered: u64,
    next_face: Option<Instant>,
    pub(crate) low_memory: bool,
}

const INFO_VECTORS: u64 = 5;

pub type CubemapPixels = (u32, [Vec<[f32; 4]>; 6]);

fn info_buffer(device: &wgpu::Device, capacity: u32) -> wgpu::Buffer {
    device.create_buffer(&wgpu::BufferDescriptor {
        label: Some("hp-render cubemap slots"),
        size: u64::from(capacity.max(1)) * 16 * INFO_VECTORS,
        usage: wgpu::BufferUsages::STORAGE | wgpu::BufferUsages::COPY_DST,
        mapped_at_creation: false,
    })
}

fn mip_count(res: u32) -> u32 {
    32 - res.max(1).leading_zeros()
}

fn cube_array(device: &wgpu::Device, capacity: u32, res: u32) -> CubeGpu {
    let mips = mip_count(res);
    let texture = device.create_texture(&wgpu::TextureDescriptor {
        label: Some("hp-render cubemaps"),
        size: wgpu::Extent3d { width: res, height: res, depth_or_array_layers: capacity * 6 },
        mip_level_count: mips,
        sample_count: 1,
        dimension: wgpu::TextureDimension::D2,
        format: CUBE_FORMAT,
        usage: wgpu::TextureUsages::TEXTURE_BINDING | wgpu::TextureUsages::COPY_DST | wgpu::TextureUsages::COPY_SRC,
        view_formats: &[],
    });
    let view = texture.create_view(&wgpu::TextureViewDescriptor {
        label: Some("hp-render cubemaps"),
        dimension: Some(wgpu::TextureViewDimension::CubeArray),
        array_layer_count: Some(capacity * 6),
        ..Default::default()
    });
    CubeGpu { texture, view, info: info_buffer(device, capacity), capacity, res, mips }
}

impl Cubes {
    pub(crate) fn new(device: &wgpu::Device) -> Self {
        Cubes {
            slots: vec![Slot { content: Content::Sky, ready: false, scale: 1.0, dirty: true, parallax: None }],
            gpu: cube_array(device, 1, MIN_RESOLUTION),
            info_dirty: true,
            staging: None,
            blit: None,
            current: None,
            noted: None,
            source: None,
            drawn_key: None,
            pending: None,
            auto: true,
            epoch: 1,
            instances_hash: 0,
            model_candidates: None,
            textures: HashMap::new(),
            faces_rendered: 0,
            next_face: None,
            low_memory: false,
        }
    }

    pub(crate) fn view(&self) -> &wgpu::TextureView {
        &self.gpu.view
    }

    pub(crate) fn info(&self) -> &wgpu::Buffer {
        &self.gpu.info
    }

    pub(crate) fn note_view(&mut self, view: u32, params: &ViewParams, options: &ViewOptions) {
        if view == PROBE_VIEW || self.noted.is_some_and(|n| n.view < view) {
            return;
        }
        self.noted = Some(Source { view, params: *params, options: *options });
    }

    fn alloc(&mut self, content: Content) -> Option<u16> {
        let slot = Slot { content, ready: false, scale: 1.0, dirty: true, parallax: None };
        if let Some(i) = self.slots.iter().skip(1).position(|s| matches!(s.content, Content::Free)) {
            self.slots[i + 1] = slot;
            self.info_dirty = true;
            return Some(i as u16 + 1);
        }
        if self.slots.len() >= MAX_SLOTS {
            return None;
        }
        self.slots.push(slot);
        self.info_dirty = true;
        Some(self.slots.len() as u16 - 1)
    }

    fn model_points(&self) -> Vec<(Vec3, u16)> {
        let ok = |i: usize| self.model_candidates.as_ref().is_none_or(|c| c.contains(&(i as u16)));
        self.slots
            .iter()
            .enumerate()
            .filter(|(i, _)| ok(*i))
            .filter_map(|(i, s)| s.origin().map(|o| (o, i as u16)))
            .collect()
    }

    pub(crate) fn model_cubemap(&self, points: &[(Vec3, u16)], origin: Vec3) -> u16 {
        nearest(points, origin).unwrap_or(NO_CUBEMAP)
    }

    pub(crate) fn face_cubemap(&self, object: ObjectId, center: Vec3, normal: Vec3) -> u16 {
        let probes: Vec<(u16, &CubemapProbe)> = self
            .slots
            .iter()
            .enumerate()
            .filter_map(|(i, s)| match &s.content {
                Content::Probe(p) => Some((i as u16, p)),
                _ => None,
            })
            .collect();
        face_cubemap_of(&probes, object, center, normal)
    }

    pub(crate) fn material_cube(
        &mut self,
        desc: Option<&crate::MaterialDesc>,
        width: &dyn Fn(TexId) -> Option<u32>,
    ) -> u32 {
        const SURFACE: u32 = u32::MAX;
        let Some(e) = desc.and_then(|d| d.envmap) else { return SURFACE };
        if let Some(c) = e.cubemap {
            return u32::from(c.0);
        }
        let Some((t, size)) = e.texture.and_then(|t| width(t).map(|size| (t, size))) else { return SURFACE };
        if let Some(&s) = self.textures.get(&t.0) {
            if let Some(slot) = self.slots.get_mut(usize::from(s))
                && let Content::Texture { size: previous, .. } = &mut slot.content
                && *previous != size
            {
                *previous = size;
                slot.dirty = true;
                slot.ready = false;
                self.info_dirty = true;
            }
            return u32::from(s);
        }
        match self.alloc(Content::Texture { texture: t, size }) {
            Some(s) => {
                self.textures.insert(t.0, s);
                u32::from(s)
            }
            None => SURFACE,
        }
    }

    fn wanted_resolution(&self) -> u32 {
        let asked = self.slots.iter().map(Slot::wanted_size).max().unwrap_or(0);
        let mut res = asked.clamp(MIN_RESOLUTION, MAX_RESOLUTION).next_power_of_two().min(MAX_RESOLUTION);
        let cubes = self.slots.len() as u64;
        let budget = if self.low_memory { LOW_MEMORY_BUDGET } else { MEMORY_BUDGET };
        while res > MIN_RESOLUTION && array_bytes(cubes, res) > budget {
            res /= 2;
        }
        res
    }

    fn mark_all_dirty(&mut self) {
        for s in &mut self.slots {
            if !matches!(s.content, Content::Free) {
                s.dirty = true;
            }
        }
        self.current = None;
    }
}

pub(crate) fn f16_to_f32(h: u16) -> f32 {
    let sign = if h & 0x8000 != 0 { -1.0 } else { 1.0 };
    let exp = i32::from((h >> 10) & 0x1f);
    let mant = f32::from(h & 0x3ff);
    match exp {
        0 => sign * mant * 2f32.powi(-24),
        31 => {
            if mant == 0.0 {
                sign * f32::INFINITY
            } else {
                f32::NAN
            }
        }
        _ => sign * (1.0 + mant / 1024.0) * 2f32.powi(exp - 15),
    }
}

fn array_bytes(cubes: u64, res: u32) -> u64 {
    let r = u64::from(res);
    cubes * 6 * r * r * TEXEL_BYTES * 4 / 3
}

pub(crate) fn face_cubemap_of(probes: &[(u16, &CubemapProbe)], object: ObjectId, center: Vec3, normal: Vec3) -> u16 {
    for (id, p) in probes {
        for (o, plane) in &p.sides {
            let n = Vec3::new(plane[0], plane[1], plane[2]);
            if *o == object && n.dot(normal) > 0.99 && (n.dot(center) - plane[3]).abs() < 2.0 {
                return *id;
            }
        }
    }
    let points: Vec<(Vec3, u16)> = probes.iter().map(|(id, p)| (Vec3::from(p.origin), *id)).collect();
    nearest_in_front(&points, center, normal).unwrap_or(NO_CUBEMAP)
}

pub(crate) fn nearest(points: &[(Vec3, u16)], p: Vec3) -> Option<u16> {
    let mut best: Option<(f32, u16)> = None;
    for &(o, id) in points {
        let d = o.distance_squared(p);
        if best.is_none_or(|b| d < b.0) {
            best = Some((d, id));
        }
    }
    best.map(|b| b.1)
}

pub(crate) fn nearest_in_front(points: &[(Vec3, u16)], center: Vec3, normal: Vec3) -> Option<u16> {
    let mut best: Option<(f32, u16)> = None;
    for &(o, id) in points {
        let d = o - center;
        let dist = d.length();
        let dir = if dist > 1e-10 { d / dist } else { Vec3::ZERO };
        if dir.dot(normal) >= 0.0 && best.is_none_or(|b| dist < b.0) {
            best = Some((dist, id));
        }
    }
    best.map(|b| b.1).or_else(|| nearest(points, center))
}

fn probe_mode(mode: RenderMode) -> RenderMode {
    if mode == RenderMode::Lit { RenderMode::Lit } else { RenderMode::Textured }
}

fn probe_options(o: &ViewOptions, sky: bool) -> ViewOptions {
    let radius = if o.cull_radius > 0.0 { o.cull_radius.min(PROBE_CULL_RADIUS) } else { PROBE_CULL_RADIUS };
    ViewOptions {
        grid_3d: None,
        show_sky: true,
        show_sky_3d: o.show_sky_3d && !sky,
        sky_see_through: true,
        occlusion_culling: false,
        walkable: false,
        walk_marks: false,
        cull_radius: radius,
        collision_models: false,
        collision_only: false,
        selection_outline: false,
        game_wireframe: 0,
        model_distance: PROBE_MODEL_DISTANCE,
        cubemaps: false,
        fog_volumes: false,
        fog_volume_inside: false,
        light_cap: 0,
        ..*o
    }
}

impl Renderer {
    pub fn create_cubemap_probe(&mut self, probe: &CubemapProbe) -> Option<CubemapId> {
        let mut p = probe.clone();
        p.origin = p.origin.map(|v| if v.is_finite() { v } else { 0.0 });
        let id = self.cubes.alloc(Content::Probe(p))?;
        self.cubemaps_changed();
        Some(CubemapId(id))
    }

    pub fn update_cubemap_probe(&mut self, id: CubemapId, probe: &CubemapProbe) {
        let Some(slot) = self.cubes.slots.get_mut(usize::from(id.0)) else { return };
        let Content::Probe(old) = &slot.content else { return };
        let mut p = probe.clone();
        p.origin = p.origin.map(|v| if v.is_finite() { v } else { 0.0 });
        if *old == p {
            return;
        }
        let redraw = old.origin != p.origin || old.size != p.size;
        slot.content = Content::Probe(p);
        if redraw {
            slot.dirty = true;
            if self.cubes.current.is_some_and(|c| c.slot == id.0) {
                self.cubes.current = None;
            }
        }
        self.cubemaps_changed();
    }

    pub fn create_cubemap_image(&mut self, image: &CubemapImage<'_>) -> Option<CubemapId> {
        let n = (image.size as usize).checked_mul(image.size as usize)?;
        if image.size == 0 || image.size > 4096 || image.faces.iter().any(|f| f.len() != n) {
            return None;
        }
        let clean = |c: &[f32; 3]| c.map(|v| if v.is_finite() { v.clamp(0.0, 65504.0) } else { 0.0 });
        let texels: Vec<[f32; 3]> = image.faces.iter().flat_map(|f| f.iter().map(clean)).collect();
        let origin = image.origin.map(Vec3::from).filter(|o| o.is_finite());
        let id = self.cubes.alloc(Content::Image { size: image.size, texels, origin })?;
        self.cubemaps_changed();
        Some(CubemapId(id))
    }

    pub fn set_cubemap_parallax(&mut self, id: CubemapId, parallax: Option<ParallaxBox>) {
        if id == CubemapId::SKY {
            return;
        }
        let Some(slot) = self.cubes.slots.get_mut(usize::from(id.0)) else { return };
        if matches!(slot.content, Content::Free) || slot.parallax == parallax {
            return;
        }
        slot.parallax = parallax;
        self.cubes.info_dirty = true;
        self.content_gen += 1;
    }

    pub fn cubemap_parallax(&self, id: CubemapId) -> Option<ParallaxBox> {
        self.cubes.slots.get(usize::from(id.0)).and_then(|s| s.parallax)
    }

    pub fn destroy_cubemap(&mut self, id: CubemapId) {
        if id == CubemapId::SKY {
            return;
        }
        let Some(slot) = self.cubes.slots.get_mut(usize::from(id.0)) else { return };
        if matches!(slot.content, Content::Free) {
            return;
        }
        if let Content::Texture { texture: t, .. } = slot.content {
            self.cubes.textures.remove(&t.0);
        }
        *slot = Slot::free();
        if self.cubes.current.is_some_and(|c| c.slot == id.0) {
            self.cubes.current = None;
        }
        self.cubes.info_dirty = true;
        self.cubemaps_changed();
    }

    fn cubemaps_changed(&mut self) {
        self.content_gen += 1;
        if !self.instance_list.is_empty() {
            self.pack_instances();
        }
        self.assign_lightmap_cubemaps();
    }

    pub fn rebake_cubemaps(&mut self) {
        self.cubes.mark_all_dirty();
        self.cubes.drawn_key = None;
        self.cubes.pending = None;
    }

    pub fn set_cubemap_auto(&mut self, on: bool) {
        self.cubes.auto = on;
    }

    pub fn set_model_cubemaps(&mut self, ids: Option<&[CubemapId]>) {
        let list = ids.map(|l| l.iter().map(|c| c.0).collect::<Vec<u16>>());
        if self.cubes.model_candidates != list {
            self.cubes.model_candidates = list;
            if !self.instance_list.is_empty() {
                self.pack_instances();
            }
        }
    }

    pub fn nearest_cubemap(&self, origin: [f32; 3]) -> Option<CubemapId> {
        let points = self.cubes.model_points();
        nearest(&points, Vec3::from(origin)).map(CubemapId)
    }

    pub(crate) fn model_cube_points(&self) -> Vec<(Vec3, u16)> {
        self.cubes.model_points()
    }

    pub fn cubemap_lighting(&self) -> Option<crate::CompiledLightMode> {
        let k = self.cubes.drawn_key?;
        (k.options.compiled_lighting && k.lighting.0).then(|| self.compiled.mode()).flatten()
    }

    pub fn cubemap_ready(&self, id: CubemapId) -> bool {
        self.cubes.slots.get(usize::from(id.0)).is_some_and(|s| s.ready)
    }

    pub fn cubemap_stats(&self) -> CubemapStats {
        let c = &self.cubes;
        let mut s = CubemapStats {
            resolution: c.gpu.res,
            bytes: array_bytes(u64::from(c.gpu.capacity), c.gpu.res),
            faces_rendered: c.faces_rendered,
            ..Default::default()
        };
        for slot in &c.slots {
            match slot.content {
                Content::Probe(_) => {
                    s.probes += 1;
                    s.ready += u32::from(slot.ready);
                    if slot.dirty {
                        s.pending_faces += 6;
                    }
                }
                Content::Image { .. } => s.images += 1,
                Content::Texture { .. } => s.textures += 1,
                Content::Sky => {
                    if slot.dirty {
                        s.pending_faces += 6;
                    }
                }
                Content::Free => {}
            }
        }
        if let Some(cur) = c.current {
            s.pending_faces = s.pending_faces.saturating_sub(u32::from(cur.faces));
        }
        s
    }

    pub(crate) fn forget_cube_texture(&mut self, id: TexId) {
        if let Some(slot) = self.cubes.textures.remove(&id.0) {
            if let Some(s) = self.cubes.slots.get_mut(usize::from(slot)) {
                *s = Slot::free();
            }
            self.cubes.info_dirty = true;
        }
    }

    pub(crate) fn material_cube(&mut self, desc: Option<&crate::MaterialDesc>) -> u32 {
        let textures = &self.textures;
        let width = |t: TexId| textures.get(&t.0).map(|entry| entry.view.texture().width());
        self.cubes.material_cube(desc, &width)
    }

    pub fn cubemaps_waiting(&self) -> bool {
        self.cubes.source.is_some() && self.cubemap_stats().pending_faces > 0
    }

    pub fn step_cubemaps(&mut self, budget: Duration) -> u32 {
        if let Some(n) = self.cubes.noted.take() {
            self.cubes.source = Some(n);
        }
        self.ensure_cube_resolution();
        self.upload_cube_images();
        let Some(source) = self.cubes.source else { return self.cubemap_stats().pending_faces };
        self.check_bake_key(&source);
        let start = Instant::now();
        let quiet = self.last_motion.is_none_or(|t| start.duration_since(t) >= MOTION_QUIET);
        if quiet && self.cubes.next_face.is_none_or(|t| start >= t) {
            while let Some(cur) = self.next_cube_face() {
                let face = Instant::now();
                self.draw_probe_face(cur, &source);
                let took = face.elapsed();
                self.cubes.next_face = Some(Instant::now() + took.mul_f32(1.0 / FRAME_SHARE - 1.0));
                if start.elapsed() >= budget {
                    break;
                }
            }
        }
        self.write_cube_info();
        self.cubemap_stats().pending_faces
    }

    fn check_bake_key(&mut self, source: &Source) {
        let opts = probe_options(&source.options, false);
        let compiled = self.compiled.mode();
        let key = BakeKey {
            mode: probe_mode(source.params.mode),
            background: source.params.background.map(f32::to_bits),
            show_models: source.params.show_models,
            options: ViewOptions { time: 0.0, ..opts },
            lighting: (compiled.is_some(), compiled.map_or(0, |m| m as u8), self.lightmap_display() as u32),
            sky: self.environment.sky,
            hdr_sky: self.hdr_sky.is_some(),
            res: self.cubes.gpu.res,
            epoch: if self.cubes.auto { self.cubes.epoch } else { 0 },
        };
        let Some(drawn) = self.cubes.drawn_key else {
            self.cubes.drawn_key = Some(key);
            return;
        };
        if drawn == key {
            self.cubes.pending = None;
            return;
        }
        let look_changed = BakeKey { epoch: drawn.epoch, ..key } != drawn;
        let now = Instant::now();
        let settled = match self.cubes.pending {
            Some((k, t)) if k == key => now.duration_since(t) >= SETTLE,
            _ => {
                self.cubes.pending = Some((key, now));
                false
            }
        };
        if look_changed || settled {
            self.cubes.mark_all_dirty();
            self.cubes.drawn_key = Some(key);
            self.cubes.pending = None;
        }
    }

    fn next_cube_face(&mut self) -> Option<Current> {
        if let Some(c) = self.cubes.current
            && c.faces < 6
            && self.cubes.slots.get(usize::from(c.slot)).is_some_and(|s| s.dirty)
        {
            return Some(c);
        }
        self.cubes.current = None;
        let slot =
            self.cubes.slots.iter().position(|s| s.dirty && matches!(s.content, Content::Sky | Content::Probe(_)))?;
        let c = Current { slot: slot as u16, faces: 0 };
        self.cubes.current = Some(c);
        Some(c)
    }

    fn ensure_cube_resolution(&mut self) {
        let res = self.cubes.wanted_resolution();
        let max_layers = self.device.limits().max_texture_array_layers.max(6);
        let max_cubes = (max_layers / 6).max(1);
        let need = (self.cubes.slots.len() as u32).min(max_cubes);
        let gpu = &self.cubes.gpu;
        if gpu.res == res && gpu.capacity >= need {
            return;
        }
        let capacity = if gpu.res == res { need.next_power_of_two().min(max_cubes).max(need) } else { need.max(1) };
        let new = cube_array(&self.device, capacity, res);
        if self.cubes.gpu.res == res {
            let old = &self.cubes.gpu;
            let mut enc = self
                .device
                .create_command_encoder(&wgpu::CommandEncoderDescriptor { label: Some("hp-render cubemaps grow") });
            for mip in 0..old.mips {
                let side = (res >> mip).max(1);
                enc.copy_texture_to_texture(
                    wgpu::TexelCopyTextureInfo {
                        texture: &old.texture,
                        mip_level: mip,
                        origin: wgpu::Origin3d::ZERO,
                        aspect: wgpu::TextureAspect::All,
                    },
                    wgpu::TexelCopyTextureInfo {
                        texture: &new.texture,
                        mip_level: mip,
                        origin: wgpu::Origin3d::ZERO,
                        aspect: wgpu::TextureAspect::All,
                    },
                    wgpu::Extent3d { width: side, height: side, depth_or_array_layers: old.capacity * 6 },
                );
            }
            self.queue.submit([enc.finish()]);
        } else {
            for s in &mut self.cubes.slots {
                s.ready = false;
            }
            self.cubes.mark_all_dirty();
            self.cubes.staging = None;
        }
        self.cubes.gpu = new;
        self.cubes.info_dirty = true;
        self.scene_gen += 1;
        self.content_gen += 1;
    }

    pub(crate) fn write_cube_info(&mut self) {
        if !self.cubes.info_dirty {
            return;
        }
        self.cubes.info_dirty = false;
        let cap = self.cubes.gpu.capacity as usize;
        let data: Vec<[f32; 4]> = (0..cap)
            .flat_map(|i| {
                let slot = self.cubes.slots.get(i);
                let head = match slot {
                    Some(s) if s.ready => [1.0, s.scale, 0.0, 0.0],
                    _ => [0.0, 1.0, 0.0, 0.0],
                };
                let b = slot.and_then(|s| s.parallax).map_or([[0.0; 4]; 4], |b| b.gpu());
                [head, b[0], b[1], b[2], b[3]]
            })
            .collect();
        self.queue.write_buffer(&self.cubes.gpu.info, 0, bytemuck::cast_slice(&data));
    }

    fn ensure_staging(&mut self) {
        let res = self.cubes.gpu.res;
        if self.cubes.staging.as_ref().is_some_and(|s| s.res == res) {
            return;
        }
        let mips = mip_count(res);
        let texture = self.device.create_texture(&wgpu::TextureDescriptor {
            label: Some("hp-render cubemap staging"),
            size: wgpu::Extent3d { width: res, height: res, depth_or_array_layers: 6 },
            mip_level_count: mips,
            sample_count: 1,
            dimension: wgpu::TextureDimension::D2,
            format: CUBE_FORMAT,
            usage: wgpu::TextureUsages::RENDER_ATTACHMENT
                | wgpu::TextureUsages::TEXTURE_BINDING
                | wgpu::TextureUsages::COPY_SRC,
            view_formats: &[],
        });
        self.cubes.staging = Some(Staging { texture, res, mips });
    }

    fn ensure_blit(&mut self) {
        if self.cubes.blit.is_some() {
            return;
        }
        let device = &self.device;
        let module = device.create_shader_module(wgpu::ShaderModuleDescriptor {
            label: Some("hp-render cubemap blit"),
            source: wgpu::ShaderSource::Wgsl(include_str!("shaders/cube_blit.wgsl").into()),
        });
        let layout = device.create_bind_group_layout(&wgpu::BindGroupLayoutDescriptor {
            label: Some("hp-render cubemap blit"),
            entries: &[
                wgpu::BindGroupLayoutEntry {
                    binding: 0,
                    visibility: wgpu::ShaderStages::FRAGMENT,
                    ty: wgpu::BindingType::Texture {
                        sample_type: wgpu::TextureSampleType::Float { filterable: true },
                        view_dimension: wgpu::TextureViewDimension::D2,
                        multisampled: false,
                    },
                    count: None,
                },
                wgpu::BindGroupLayoutEntry {
                    binding: 1,
                    visibility: wgpu::ShaderStages::FRAGMENT,
                    ty: wgpu::BindingType::Sampler(wgpu::SamplerBindingType::Filtering),
                    count: None,
                },
                wgpu::BindGroupLayoutEntry {
                    binding: 2,
                    visibility: wgpu::ShaderStages::FRAGMENT,
                    ty: wgpu::BindingType::Buffer {
                        ty: wgpu::BufferBindingType::Uniform,
                        has_dynamic_offset: false,
                        min_binding_size: None,
                    },
                    count: None,
                },
            ],
        });
        let pl = device.create_pipeline_layout(&wgpu::PipelineLayoutDescriptor {
            label: Some("hp-render cubemap blit"),
            bind_group_layouts: &[Some(&layout)],
            immediate_size: 0,
        });
        let pipeline = device.create_render_pipeline(&wgpu::RenderPipelineDescriptor {
            label: Some("hp-render cubemap blit"),
            layout: Some(&pl),
            vertex: wgpu::VertexState {
                module: &module,
                entry_point: Some("vs_blit"),
                compilation_options: wgpu::PipelineCompilationOptions::default(),
                buffers: &[],
            },
            primitive: wgpu::PrimitiveState::default(),
            depth_stencil: None,
            multisample: wgpu::MultisampleState::default(),
            fragment: Some(wgpu::FragmentState {
                module: &module,
                entry_point: Some("fs_blit"),
                compilation_options: wgpu::PipelineCompilationOptions::default(),
                targets: &[Some(wgpu::ColorTargetState {
                    format: CUBE_FORMAT,
                    blend: None,
                    write_mask: wgpu::ColorWrites::ALL,
                })],
            }),
            multiview_mask: None,
            cache: None,
        });
        self.cubes.blit = Some(Blit { pipeline, layout });
    }

    #[allow(clippy::too_many_arguments)]
    fn record_blit(
        &self,
        enc: &mut wgpu::CommandEncoder,
        src: &wgpu::TextureView,
        rect: [f32; 4],
        scale: f32,
        target: &wgpu::Texture,
        layer: u32,
        mip: u32,
    ) {
        let Some(blit) = self.cubes.blit.as_ref() else { return };
        let params: [f32; 8] = [rect[0], rect[1], rect[2], rect[3], scale, scale, scale, 0.0];
        let uniform = self.device.create_buffer(&wgpu::BufferDescriptor {
            label: Some("hp-render cubemap blit"),
            size: 32,
            usage: wgpu::BufferUsages::UNIFORM | wgpu::BufferUsages::COPY_DST,
            mapped_at_creation: false,
        });
        self.queue.write_buffer(&uniform, 0, bytemuck::cast_slice(&params));
        let bind = self.device.create_bind_group(&wgpu::BindGroupDescriptor {
            label: Some("hp-render cubemap blit"),
            layout: &blit.layout,
            entries: &[
                wgpu::BindGroupEntry { binding: 0, resource: wgpu::BindingResource::TextureView(src) },
                wgpu::BindGroupEntry { binding: 1, resource: wgpu::BindingResource::Sampler(&self.sampler_clamp) },
                wgpu::BindGroupEntry { binding: 2, resource: uniform.as_entire_binding() },
            ],
        });
        let view = target.create_view(&wgpu::TextureViewDescriptor {
            label: Some("hp-render cubemap face"),
            dimension: Some(wgpu::TextureViewDimension::D2),
            base_mip_level: mip,
            mip_level_count: Some(1),
            base_array_layer: layer,
            array_layer_count: Some(1),
            ..Default::default()
        });
        let mut pass = enc.begin_render_pass(&wgpu::RenderPassDescriptor {
            label: Some("hp-render cubemap blit"),
            color_attachments: &[Some(wgpu::RenderPassColorAttachment {
                view: &view,
                depth_slice: None,
                resolve_target: None,
                ops: wgpu::Operations { load: wgpu::LoadOp::Clear(wgpu::Color::BLACK), store: wgpu::StoreOp::Store },
            })],
            depth_stencil_attachment: None,
            timestamp_writes: None,
            occlusion_query_set: None,
            multiview_mask: None,
        });
        pass.set_pipeline(&blit.pipeline);
        pass.set_bind_group(0, &bind, &[]);
        pass.draw(0..3, 0..1);
    }

    fn finish_staging(&mut self, slot: u16, scale: f32) {
        let Some(staging) = self.cubes.staging.as_ref() else { return };
        let mut enc = self
            .device
            .create_command_encoder(&wgpu::CommandEncoderDescriptor { label: Some("hp-render cubemap mips") });
        for mip in 1..staging.mips {
            for layer in 0..6 {
                let src = staging.texture.create_view(&wgpu::TextureViewDescriptor {
                    dimension: Some(wgpu::TextureViewDimension::D2),
                    base_mip_level: mip - 1,
                    mip_level_count: Some(1),
                    base_array_layer: layer,
                    array_layer_count: Some(1),
                    ..Default::default()
                });
                self.record_blit(&mut enc, &src, [1.0, 1.0, 0.0, 0.0], 1.0, &staging.texture, layer, mip);
            }
        }
        let gpu = &self.cubes.gpu;
        if u32::from(slot) < gpu.capacity && staging.res == gpu.res {
            for mip in 0..staging.mips.min(gpu.mips) {
                let side = (staging.res >> mip).max(1);
                enc.copy_texture_to_texture(
                    wgpu::TexelCopyTextureInfo {
                        texture: &staging.texture,
                        mip_level: mip,
                        origin: wgpu::Origin3d::ZERO,
                        aspect: wgpu::TextureAspect::All,
                    },
                    wgpu::TexelCopyTextureInfo {
                        texture: &gpu.texture,
                        mip_level: mip,
                        origin: wgpu::Origin3d { x: 0, y: 0, z: u32::from(slot) * 6 },
                        aspect: wgpu::TextureAspect::All,
                    },
                    wgpu::Extent3d { width: side, height: side, depth_or_array_layers: 6 },
                );
            }
        }
        self.queue.submit([enc.finish()]);
        if let Some(s) = self.cubes.slots.get_mut(usize::from(slot)) {
            s.ready = true;
            s.dirty = false;
            s.scale = scale;
        }
        self.cubes.info_dirty = true;
        self.content_gen += 1;
    }

    fn draw_probe_face(&mut self, cur: Current, source: &Source) {
        let Some(slot) = self.cubes.slots.get(usize::from(cur.slot)) else {
            self.cubes.current = None;
            return;
        };
        let (eye, sky) = match &slot.content {
            Content::Probe(p) => (p.origin, false),
            Content::Sky => (SKY_EYE, true),
            _ => {
                self.cubes.current = None;
                return;
            }
        };
        let res = self.cubes.gpu.res;
        let face = usize::from(cur.faces.min(5));
        let (forward, up) = FACES[face];
        let far = match source.params.kind {
            ViewKind::Perspective { far, .. } if far.is_finite() && far > 2.0 => far.max(65_536.0),
            _ => 131_072.0,
        };
        let params = ViewParams {
            size: [res, res],
            kind: ViewKind::Perspective {
                eye,
                forward,
                up,
                fov_y_deg: 90.0,
                near: 1.0,
                far: if sky { 1.01 } else { far },
            },
            mode: probe_mode(source.params.mode),
            grid: None,
            background: source.params.background,
            selection_color: [0.0; 4],
            show_sprites: false,
            show_models: source.params.show_models && !sky,
            msaa: false,
            game_pixel_grid: false,
        };
        let options = probe_options(&source.options, sky);
        self.view_options.insert(PROBE_VIEW, options);
        let scale = 1.0;
        self.probing = true;
        let content = self.content_gen;
        self.render_view(PROBE_VIEW, &params, &Overlay::default());
        self.content_gen = content;
        self.probing = false;
        self.cubes.faces_rendered += 1;
        self.ensure_staging();
        self.ensure_blit();
        let Some(view) = self.views.get(&PROBE_VIEW) else { return };
        let src = view.color.create_view(&wgpu::TextureViewDescriptor::default());
        let mut enc = self
            .device
            .create_command_encoder(&wgpu::CommandEncoderDescriptor { label: Some("hp-render cubemap face") });
        if let Some(staging) = self.cubes.staging.as_ref() {
            self.record_blit(&mut enc, &src, [-1.0, 1.0, 1.0, 0.0], scale, &staging.texture, face as u32, 0);
        }
        self.queue.submit([enc.finish()]);
        let next = Current { slot: cur.slot, faces: cur.faces + 1 };
        if next.faces >= 6 {
            self.cubes.current = None;
            self.finish_staging(cur.slot, scale);
        } else {
            self.cubes.current = Some(next);
        }
    }

    fn upload_cube_images(&mut self) {
        let waiting: Vec<u16> = (0..self.cubes.slots.len())
            .filter(|&i| {
                let s = &self.cubes.slots[i];
                s.dirty && matches!(s.content, Content::Image { .. } | Content::Texture { .. })
            })
            .map(|i| i as u16)
            .collect();
        if waiting.is_empty() {
            return;
        }
        self.ensure_staging();
        self.ensure_blit();
        for slot in waiting {
            match self.cubes.slots[usize::from(slot)].content.clone() {
                Content::Image { size, texels, .. } => self.upload_image(slot, size, &texels),
                Content::Texture { texture: t, .. } => self.upload_texture_cube(slot, t),
                _ => {}
            }
        }
        self.write_cube_info();
    }

    fn upload_image(&mut self, slot: u16, size: u32, texels: &[[f32; 3]]) {
        let texture = self.device.create_texture(&wgpu::TextureDescriptor {
            label: Some("hp-render cubemap image"),
            size: wgpu::Extent3d { width: size, height: size, depth_or_array_layers: 6 },
            mip_level_count: 1,
            sample_count: 1,
            dimension: wgpu::TextureDimension::D2,
            format: CUBE_FORMAT,
            usage: wgpu::TextureUsages::TEXTURE_BINDING | wgpu::TextureUsages::COPY_DST,
            view_formats: &[],
        });
        let half = crate::compiled_light::f16_bits;
        let data: Vec<u16> = texels.iter().flat_map(|c| [half(c[0]), half(c[1]), half(c[2]), half(1.0)]).collect();
        self.queue.write_texture(
            texture.as_image_copy(),
            bytemuck::cast_slice(&data),
            wgpu::TexelCopyBufferLayout { offset: 0, bytes_per_row: Some(size * 8), rows_per_image: Some(size) },
            wgpu::Extent3d { width: size, height: size, depth_or_array_layers: 6 },
        );
        let Some(staging) = self.cubes.staging.as_ref() else { return };
        let mut enc = self
            .device
            .create_command_encoder(&wgpu::CommandEncoderDescriptor { label: Some("hp-render cubemap image") });
        for layer in 0..6 {
            let src = texture.create_view(&wgpu::TextureViewDescriptor {
                dimension: Some(wgpu::TextureViewDimension::D2),
                base_array_layer: layer,
                array_layer_count: Some(1),
                ..Default::default()
            });
            self.record_blit(&mut enc, &src, [1.0, 1.0, 0.0, 0.0], 1.0, &staging.texture, layer, 0);
        }
        self.queue.submit([enc.finish()]);
        self.finish_staging(slot, 1.0);
    }

    fn upload_texture_cube(&mut self, slot: u16, t: TexId) {
        let Some(entry) = self.textures.get(&t.0) else {
            if let Some(s) = self.cubes.slots.get_mut(usize::from(slot)) {
                s.dirty = false;
            }
            return;
        };
        let Some(staging) = self.cubes.staging.as_ref() else { return };
        let mut enc = self
            .device
            .create_command_encoder(&wgpu::CommandEncoderDescriptor { label: Some("hp-render cubemap texture") });
        for layer in 0..6u32 {
            let rect = [1.0, 1.0 / 6.0, 0.0, layer as f32 / 6.0];
            self.record_blit(&mut enc, &entry.view, rect, 1.0, &staging.texture, layer, 0);
        }
        self.queue.submit([enc.finish()]);
        self.finish_staging(slot, 1.0);
    }

    pub fn read_cubemap(&self, id: CubemapId) -> Result<CubemapPixels, RenderError> {
        let slot = self.cubes.slots.get(usize::from(id.0)).filter(|s| s.ready);
        let Some(slot) = slot else { return Err(RenderError::Readback(format!("cubemap {} is not ready", id.0))) };
        let gpu = &self.cubes.gpu;
        let res = gpu.res;
        let row = (res * 8).next_multiple_of(wgpu::COPY_BYTES_PER_ROW_ALIGNMENT);
        let size = u64::from(row) * u64::from(res) * 6;
        let buffer = self.device.create_buffer(&wgpu::BufferDescriptor {
            label: Some("hp-render cubemap readback"),
            size,
            usage: wgpu::BufferUsages::COPY_DST | wgpu::BufferUsages::MAP_READ,
            mapped_at_creation: false,
        });
        let mut enc = self
            .device
            .create_command_encoder(&wgpu::CommandEncoderDescriptor { label: Some("hp-render cubemap read") });
        enc.copy_texture_to_buffer(
            wgpu::TexelCopyTextureInfo {
                texture: &gpu.texture,
                mip_level: 0,
                origin: wgpu::Origin3d { x: 0, y: 0, z: u32::from(id.0) * 6 },
                aspect: wgpu::TextureAspect::All,
            },
            wgpu::TexelCopyBufferInfo {
                buffer: &buffer,
                layout: wgpu::TexelCopyBufferLayout { offset: 0, bytes_per_row: Some(row), rows_per_image: Some(res) },
            },
            wgpu::Extent3d { width: res, height: res, depth_or_array_layers: 6 },
        );
        self.queue.submit([enc.finish()]);
        let slice = buffer.slice(..);
        let (tx, rx) = std::sync::mpsc::channel();
        slice.map_async(wgpu::MapMode::Read, move |r| {
            let _ = tx.send(r);
        });
        self.device
            .poll(wgpu::PollType::Wait { submission_index: None, timeout: None })
            .map_err(|e| RenderError::Readback(e.to_string()))?;
        rx.recv()
            .map_err(|e| RenderError::Readback(e.to_string()))?
            .map_err(|e| RenderError::Readback(e.to_string()))?;
        let data = slice.get_mapped_range().map_err(|e| RenderError::Readback(e.to_string()))?;
        let f = |b: &[u8]| f16_to_f32(u16::from_le_bytes([b[0], b[1]])) * slot.scale;
        let faces: [Vec<[f32; 4]>; 6] = std::array::from_fn(|face| {
            let mut out = Vec::with_capacity((res * res) as usize);
            for y in 0..res {
                let start = (face as u64 * u64::from(res) + u64::from(y)) * u64::from(row);
                for x in 0..res {
                    let o = (start + u64::from(x) * 8) as usize;
                    let px = &data[o..o + 8];
                    out.push([f(&px[0..2]), f(&px[2..4]), f(&px[4..6]), 1.0]);
                }
            }
            out
        });
        drop(data);
        buffer.unmap();
        Ok((res, faces))
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn vbsp_picks_the_nearest_probe_in_front() {
        let points =
            [(Vec3::new(0.0, 0.0, 100.0), 1), (Vec3::new(0.0, 0.0, -20.0), 2), (Vec3::new(500.0, 0.0, 10.0), 3)];
        assert_eq!(nearest_in_front(&points, Vec3::ZERO, Vec3::Z), Some(1));
        assert_eq!(nearest_in_front(&points, Vec3::ZERO, -Vec3::Z), Some(2));
        assert_eq!(nearest_in_front(&points, Vec3::new(400.0, 0.0, 0.0), Vec3::X), Some(3));
        let behind = [(Vec3::new(-50.0, 0.0, 0.0), 7), (Vec3::new(-10.0, 0.0, 0.0), 8)];
        assert_eq!(nearest_in_front(&behind, Vec3::ZERO, Vec3::X), Some(8));
        assert_eq!(nearest_in_front(&[], Vec3::ZERO, Vec3::X), None);
        assert_eq!(nearest_in_front(&[(Vec3::new(5.0, 0.0, 0.0), 4)], Vec3::ZERO, Vec3::Z), Some(4));
    }

    #[test]
    fn lightmapped_faces_follow_the_sides_key() {
        let above = CubemapProbe { origin: [0.0, 0.0, 100.0], ..Default::default() };
        let forcing = CubemapProbe { origin: [0.0, 0.0, -500.0], size: 0, sides: vec![(9, [0.0, 0.0, 1.0, 0.0])] };
        let probes = [(1, &above), (2, &forcing)];
        assert_eq!(face_cubemap_of(&probes, 9, Vec3::new(10.0, 0.0, 0.5), Vec3::Z), 2, "the sides key");
        assert_eq!(face_cubemap_of(&probes, 8, Vec3::ZERO, Vec3::Z), 1, "another object: nearest in front");
        assert_eq!(face_cubemap_of(&probes, 9, Vec3::new(0.0, 0.0, 50.0), Vec3::Z), 1, "another plane");
        assert_eq!(face_cubemap_of(&[], 9, Vec3::ZERO, Vec3::Z), NO_CUBEMAP);
    }

    #[test]
    fn probe_sizes_and_budget() {
        assert_eq!(probe_size(0), DEFAULT_PROBE_SIZE);
        assert_eq!(probe_size(32), 32);
        assert_eq!(probe_size(48), 64);
        assert_eq!(probe_size(4096), MAX_RESOLUTION);
        assert!(array_bytes(1024, 32) < MEMORY_BUDGET, "every sample of a map fits at the game's default size");
        assert_eq!(probe_mode(RenderMode::Lit), RenderMode::Lit);
        assert_eq!(probe_mode(RenderMode::TexturedWire), RenderMode::Textured);
        let o = probe_options(
            &ViewOptions {
                grid_3d: Some(Default::default()),
                cubemaps: true,
                fog_volumes: true,
                fog_volume_inside: true,
                ..Default::default()
            },
            true,
        );
        assert!(o.grid_3d.is_none() && !o.cubemaps && o.show_sky && !o.show_sky_3d);
        assert!(!o.fog_volumes && !o.fog_volume_inside, "the fog volume preview stays out of reflections");
    }

    #[test]
    fn half_floats_round_trip() {
        for v in [0.0f32, 1.0, 0.5, 4.0, 65504.0, 1.0e-5, -2.0] {
            let back = f16_to_f32(crate::compiled_light::f16_bits(v));
            assert!((back - v).abs() <= v.abs() * 1e-3 + 1e-7, "{v} {back}");
        }
    }

    #[test]
    fn faces_follow_the_direct3d_layout() {
        for (forward, up) in FACES {
            let (f, u) = (Vec3::from(forward), Vec3::from(up));
            assert!(f.dot(u).abs() < 1e-6);
        }
    }
}
