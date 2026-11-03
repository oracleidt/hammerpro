use std::collections::{HashMap, HashSet};
use std::sync::{Mutex, OnceLock, PoisonError};

use bytemuck::{Pod, Zeroable};
use glam::{Mat4, Vec3, Vec4};

use crate::hooks::{HookContext, PassHook};

#[path = "post.rs"]
pub(crate) mod post;
use crate::pipelines::{COLOR_FORMAT, DEPTH_FORMAT, MeshPass};
use crate::{MaterialId, ObjectId, Renderer, classify_with};

#[repr(C)]
#[derive(Clone, Copy, Debug, Default, PartialEq, Pod, Zeroable)]
pub struct LightmapVertex {
    pub pos: [f32; 3],
    pub uv: [f32; 2],
    pub lm: [f32; 2],
    pub alpha: f32,
    pub face: u32,
    pub blend: u32,
}

pub fn pack_blend(weights: [f32; 4]) -> u32 {
    crate::gpu::pack_blend(weights)
}

#[derive(Clone, Copy, Debug)]
pub struct LightmapBatch<'a> {
    pub material: MaterialId,
    pub vertices: &'a [LightmapVertex],
    pub indices: &'a [u32],
}

pub mod face_flags {
    pub const DISP: u32 = 4;
    pub const WIDE: u32 = 8;
    pub const POWER_MASK: u32 = 0xf00;
    pub const POWER_SHIFT: u32 = 8;
}

const VALID: u32 = 1;
const BAKED: u32 = 2;
const BUMPED: u32 = 16;
const STATIC_FLAGS: u32 = face_flags::DISP | face_flags::WIDE | face_flags::POWER_MASK;

#[derive(Clone, Copy, Debug, PartialEq)]
pub struct LightmapFace {
    pub object: ObjectId,
    pub generation: u64,
    pub scale: f32,
    pub flags: u32,
}

#[derive(Clone, Copy, Debug, PartialEq)]
pub struct LightmapRect {
    pub layer: u32,
    pub x: u32,
    pub y: u32,
    pub w: u32,
    pub h: u32,
    pub density: f32,
    pub bumped: bool,
}

impl LightmapRect {
    pub fn images(&self) -> u32 {
        if self.bumped { 4 } else { 1 }
    }
}

#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub enum LightmapDisplay {
    #[default]
    Off,
    Baked,
    Grid,
}

#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub struct LightmapStats {
    pub faces: u32,
    pub valid_faces: u32,
    pub baked_faces: u32,
    pub blocks: u32,
    pub layers: u32,
    pub bytes: u64,
}

#[repr(C)]
#[derive(Clone, Copy, Debug, Default, PartialEq, Pod, Zeroable)]
struct FaceGpu {
    origin: [f32; 2],
    density: f32,
    layer: u32,
    slot: u32,
    flags: u32,
    scale: f32,
    dims: u32,
    cube: u32,
    pad: u32,
}

const LDR_BIT: u32 = 0x100;

#[repr(C)]
#[derive(Clone, Copy, Debug, Default, PartialEq, Pod, Zeroable)]
struct ParamsGpu {
    inv_size: [f32; 2],
    exposure: f32,
    mode: u32,
}

#[derive(Clone, Copy, Debug, Default)]
struct FaceCpu {
    gpu: FaceGpu,
    object: ObjectId,
    live: bool,
}

#[derive(Clone, Copy, Debug, Default)]
pub(crate) struct FaceGeo {
    pub center: Vec3,
    pub normal: Vec3,
}

struct Draw {
    material: u32,
    first: u32,
    count: u32,
}

struct BlockGpu {
    vbuf: wgpu::Buffer,
    ibuf: wgpu::Buffer,
    draws: Vec<Draw>,
    lo: Vec3,
    hi: Vec3,
    bytes: u64,
}

struct Atlas {
    texture: wgpu::Texture,
    view: wgpu::TextureView,
    size: u32,
    layers: u32,
}

struct Pipelines {
    baked: [[wgpu::RenderPipeline; 2]; 2],
    grid: [[wgpu::RenderPipeline; 2]; 2],
}

#[derive(Default)]
pub(crate) struct Lightmaps {
    pub(crate) color: post::ColorLut,
    display: LightmapDisplay,
    exposure: f32,
    ldr: bool,
    tracking: bool,
    counter: u64,
    generations: HashMap<ObjectId, u64>,
    changed: HashSet<ObjectId>,
    faces: Vec<FaceCpu>,
    geo: HashMap<u32, FaceGeo>,
    by_object: HashMap<ObjectId, Vec<u32>>,
    dirty: Mutex<Option<(u32, u32)>>,
    face_buf: Option<wgpu::Buffer>,
    params_buf: Option<wgpu::Buffer>,
    params: ParamsGpu,
    atlas: Option<Atlas>,
    bind_group: Option<wgpu::BindGroup>,
    layout: OnceLock<wgpu::BindGroupLayout>,
    blocks: HashMap<u32, BlockGpu>,
    pipelines: [OnceLock<Pipelines>; 2],
    pipeline_layout: OnceLock<wgpu::PipelineLayout>,
    module: OnceLock<wgpu::ShaderModule>,
}

const SHADER: &str = concat!(
    include_str!("shaders/common.wgsl"),
    "\n",
    include_str!("shaders/material.wgsl"),
    "\n",
    include_str!("shaders/parallax.wgsl"),
    "\n",
    include_str!("shaders/lightmap.wgsl")
);

const VERTEX_ATTRS: [wgpu::VertexAttribute; 6] = [
    wgpu::VertexAttribute { format: wgpu::VertexFormat::Float32x3, offset: 0, shader_location: 0 },
    wgpu::VertexAttribute { format: wgpu::VertexFormat::Float32x2, offset: 12, shader_location: 1 },
    wgpu::VertexAttribute { format: wgpu::VertexFormat::Float32x2, offset: 20, shader_location: 2 },
    wgpu::VertexAttribute { format: wgpu::VertexFormat::Float32, offset: 28, shader_location: 3 },
    wgpu::VertexAttribute { format: wgpu::VertexFormat::Uint32, offset: 32, shader_location: 4 },
    wgpu::VertexAttribute { format: wgpu::VertexFormat::Uint32, offset: 36, shader_location: 5 },
];

const ATLAS_FORMAT: wgpu::TextureFormat = wgpu::TextureFormat::Rg11b10Ufloat;
const UPLOAD_CHUNK: u64 = 32 << 20;

impl Lightmaps {
    fn bind_layout(&self, device: &wgpu::Device) -> &wgpu::BindGroupLayout {
        self.layout.get_or_init(|| {
            device.create_bind_group_layout(&wgpu::BindGroupLayoutDescriptor {
                label: Some("hp-render lightmap"),
                entries: &[
                    wgpu::BindGroupLayoutEntry {
                        binding: 0,
                        visibility: wgpu::ShaderStages::FRAGMENT,
                        ty: wgpu::BindingType::Texture {
                            sample_type: wgpu::TextureSampleType::Float { filterable: true },
                            view_dimension: wgpu::TextureViewDimension::D2Array,
                            multisampled: false,
                        },
                        count: None,
                    },
                    wgpu::BindGroupLayoutEntry {
                        binding: 1,
                        visibility: wgpu::ShaderStages::VERTEX | wgpu::ShaderStages::FRAGMENT,
                        ty: wgpu::BindingType::Buffer {
                            ty: wgpu::BufferBindingType::Storage { read_only: true },
                            has_dynamic_offset: false,
                            min_binding_size: None,
                        },
                        count: None,
                    },
                    wgpu::BindGroupLayoutEntry {
                        binding: 2,
                        visibility: wgpu::ShaderStages::VERTEX | wgpu::ShaderStages::FRAGMENT,
                        ty: wgpu::BindingType::Buffer {
                            ty: wgpu::BufferBindingType::Uniform,
                            has_dynamic_offset: false,
                            min_binding_size: None,
                        },
                        count: None,
                    },
                ],
            })
        })
    }

    fn mark(&self, i: u32) {
        let mut d = self.dirty.lock().unwrap_or_else(PoisonError::into_inner);
        *d = Some(match *d {
            Some((a, b)) => (a.min(i), b.max(i + 1)),
            None => (i, i + 1),
        });
    }

    fn mark_all(&self) {
        let n = self.faces.len() as u32;
        *self.dirty.lock().unwrap_or_else(PoisonError::into_inner) = (n > 0).then_some((0, n));
    }

    fn upload_dirty(&self, queue: &wgpu::Queue) -> bool {
        let Some(buf) = &self.face_buf else { return false };
        let Some((a, b)) = self.dirty.lock().unwrap_or_else(PoisonError::into_inner).take() else { return false };
        let b = b.min(self.faces.len() as u32);
        if a >= b {
            return false;
        }
        let data: Vec<FaceGpu> = self.faces[a as usize..b as usize].iter().map(|f| f.gpu).collect();
        let offset = u64::from(a) * size_of::<FaceGpu>() as u64;
        if offset + (data.len() * size_of::<FaceGpu>()) as u64 > buf.size() {
            return false;
        }
        queue.write_buffer(buf, offset, bytemuck::cast_slice(&data));
        true
    }

    pub(crate) fn display(&self) -> LightmapDisplay {
        self.display
    }

    fn generation(&self, object: ObjectId) -> u64 {
        self.generations.get(&object).copied().unwrap_or(0)
    }

    pub(crate) fn object_changed(&mut self, object: ObjectId) {
        let has_faces = self.by_object.contains_key(&object);
        if !self.tracking && !has_faces {
            return;
        }
        self.counter += 1;
        let counter = self.counter;
        self.generations.insert(object, counter);
        if self.tracking {
            self.changed.insert(object);
        }
        if has_faces {
            let list = self.by_object.get(&object).cloned().unwrap_or_default();
            for i in list {
                if let Some(f) = self.faces.get_mut(i as usize)
                    && f.gpu.flags & VALID != 0
                {
                    f.gpu.flags &= !VALID;
                    self.mark(i);
                }
            }
        }
    }

    pub(crate) fn objects_cleared(&mut self) {
        let objects: Vec<ObjectId> = self.by_object.keys().copied().collect();
        for o in objects {
            self.object_changed(o);
        }
    }
}

impl Renderer {
    pub fn set_lightmap_display(&mut self, display: LightmapDisplay, exposure: f32) {
        let exposure = if exposure.is_finite() { exposure.clamp(0.0, 64.0) } else { 1.0 };
        let lm = &mut self.lightmap;
        if lm.display != display || lm.exposure != exposure {
            lm.display = display;
            lm.exposure = exposure;
            self.write_lightmap_params();
            self.content_gen += 1;
        }
        self.ensure_lightmap_resources();
    }

    pub fn set_lightmap_ldr(&mut self, ldr: bool) {
        if self.lightmap.ldr != ldr {
            self.lightmap.ldr = ldr;
            self.write_lightmap_params();
            self.content_gen += 1;
        }
    }

    pub fn lightmap_ldr(&self) -> bool {
        self.lightmap.ldr
    }

    pub(crate) fn baked_view(&self, cam: &crate::camera::Camera) -> bool {
        !cam.is_ortho && self.lightmap.display == LightmapDisplay::Baked && !self.lightmap.blocks.is_empty()
    }

    pub fn lightmap_display(&self) -> LightmapDisplay {
        self.lightmap.display
    }

    pub(crate) fn lightmap_exposure(&self) -> f32 {
        self.lightmap.exposure
    }

    pub fn set_lightmap_tracking(&mut self, on: bool) {
        self.lightmap.tracking = on;
        if !on {
            self.lightmap.changed.clear();
        }
    }

    pub fn lightmap_generation(&self, object: ObjectId) -> u64 {
        self.lightmap.generation(object)
    }

    pub fn take_lightmap_changes(&mut self) -> Vec<ObjectId> {
        let mut v: Vec<ObjectId> = self.lightmap.changed.drain().collect();
        v.sort_unstable();
        v
    }

    fn write_lightmap_params(&mut self) {
        let lm = &mut self.lightmap;
        let size = lm.atlas.as_ref().map_or(1, |a| a.size).max(1) as f32;
        lm.params = ParamsGpu {
            inv_size: [1.0 / size; 2],
            exposure: lm.exposure,
            mode: match lm.display {
                LightmapDisplay::Off => 0,
                LightmapDisplay::Baked => 1,
                LightmapDisplay::Grid => 2,
            } | if lm.ldr { LDR_BIT } else { 0 },
        };
        if let Some(b) = &lm.params_buf {
            self.queue.write_buffer(b, 0, bytemuck::bytes_of(&lm.params));
        }
    }

    pub fn add_lightmap_faces(&mut self, faces: &[(u32, LightmapFace)]) {
        let limit = (self.max_storage / size_of::<FaceGpu>() as u64).min(u64::from(u32::MAX)) as usize;
        for &(index, f) in faces {
            let i = index as usize;
            if i >= limit {
                continue;
            }
            self.detach_lightmap_face(index);
            if self.lightmap.faces.len() <= i {
                self.lightmap.faces.resize(i + 1, FaceCpu::default());
            }
            let slot = self.scene.acquire_ref(f.object);
            let valid = f.generation >= self.lightmap.generation(f.object);
            let scale = if f.scale.is_finite() && f.scale > 0.0 { f.scale } else { 16.0 };
            self.lightmap.faces[i] = FaceCpu {
                gpu: FaceGpu {
                    origin: [0.0; 2],
                    density: 1.0,
                    layer: 0,
                    slot,
                    flags: (f.flags & STATIC_FLAGS) | if valid { VALID } else { 0 },
                    scale,
                    dims: 0x0001_0001,
                    cube: u32::from(crate::NO_CUBEMAP),
                    pad: 0,
                },
                object: f.object,
                live: true,
            };
            self.lightmap.by_object.entry(f.object).or_default().push(index);
            self.lightmap.mark(index);
            self.assign_lightmap_cubemap(index);
        }
        self.flush_lightmap_faces();
        self.ensure_lightmap_resources();
    }

    fn detach_lightmap_face(&mut self, index: u32) {
        let Some(old) = self.lightmap.faces.get_mut(index as usize) else { return };
        if !old.live {
            return;
        }
        old.live = false;
        old.gpu.flags = 0;
        let object = old.object;
        self.lightmap.mark(index);
        if let Some(list) = self.lightmap.by_object.get_mut(&object) {
            list.retain(|i| *i != index);
            if list.is_empty() {
                self.lightmap.by_object.remove(&object);
            }
        }
        self.scene.release_ref(object);
    }

    pub fn remove_lightmap_faces(&mut self, faces: &[u32]) {
        for &i in faces {
            self.detach_lightmap_face(i);
        }
        self.flush_lightmap_faces();
    }

    pub fn revalidate_lightmap_object(&mut self, object: ObjectId, generation: u64) {
        if generation < self.lightmap.generation(object) {
            return;
        }
        let list = self.lightmap.by_object.get(&object).cloned().unwrap_or_default();
        for i in list {
            if let Some(f) = self.lightmap.faces.get_mut(i as usize)
                && f.gpu.flags & VALID == 0
            {
                f.gpu.flags |= VALID;
                self.lightmap.mark(i);
            }
        }
        self.flush_lightmap_faces();
    }

    pub fn add_lightmap_block(&mut self, id: u32, batches: &[LightmapBatch<'_>]) {
        self.remove_lightmap_block(id);
        let mut verts: Vec<LightmapVertex> = Vec::new();
        let mut indices: Vec<u32> = Vec::new();
        let mut draws = Vec::new();
        let mut lo = Vec3::splat(f32::INFINITY);
        let mut hi = Vec3::splat(f32::NEG_INFINITY);
        let limit = (self.max_buffer / size_of::<LightmapVertex>() as u64).min(u64::from(u32::MAX / 2)) as usize;
        let mut geo: HashMap<u32, (Vec3, u32, Vec3)> = HashMap::new();
        for b in batches {
            for v in b.vertices {
                let p = Vec3::from(v.pos);
                if p.is_finite() {
                    let e = geo.entry(v.face).or_insert((Vec3::ZERO, 0, Vec3::ZERO));
                    e.0 += p;
                    e.1 += 1;
                }
            }
            for t in b.indices.chunks_exact(3) {
                let [Some(a), Some(c1), Some(c2)] = [t[0], t[1], t[2]].map(|i| b.vertices.get(i as usize)) else {
                    continue;
                };
                let n = (Vec3::from(c1.pos) - Vec3::from(a.pos)).cross(Vec3::from(c2.pos) - Vec3::from(a.pos));
                if n.is_finite()
                    && let Some(e) = geo.get_mut(&a.face)
                {
                    e.2 += n;
                }
            }
        }
        for b in batches {
            if verts.len() + b.vertices.len() > limit {
                log::warn!("hp-render: lightmap block too large, truncated");
                break;
            }
            let base = verts.len() as u32;
            let first = indices.len() as u32;
            for v in b.vertices {
                let p = Vec3::from(v.pos);
                let ok = p.is_finite() && v.uv.iter().chain(&v.lm).all(|c| c.is_finite()) && v.alpha.is_finite();
                if ok {
                    lo = lo.min(p);
                    hi = hi.max(p);
                }
                verts.push(if ok { *v } else { LightmapVertex { face: u32::MAX, ..Default::default() } });
            }
            let n = b.vertices.len() as u32;
            for t in b.indices.chunks_exact(3) {
                if t.iter().all(|i| *i < n) {
                    indices.extend(t.iter().map(|i| base + i));
                }
            }
            let count = indices.len() as u32 - first;
            if count > 0 {
                let material = self.scene.material_slot(b.material);
                draws.push(Draw { material, first, count });
            }
        }
        self.sync_materials();
        for (face, (sum, count, normal)) in geo {
            let normal = normal.normalize_or_zero();
            if count > 0 && normal != Vec3::ZERO {
                self.lightmap.geo.insert(face, FaceGeo { center: sum / count as f32, normal });
                self.assign_lightmap_cubemap(face);
            }
        }
        self.flush_lightmap_faces();
        if draws.is_empty() || !lo.cmple(hi).all() {
            return;
        }
        let make = |label: &str, data: &[u8], usage: wgpu::BufferUsages| {
            let buf = self.device.create_buffer(&wgpu::BufferDescriptor {
                label: Some(label),
                size: (data.len() as u64).next_multiple_of(wgpu::COPY_BUFFER_ALIGNMENT).max(16),
                usage: usage | wgpu::BufferUsages::COPY_DST,
                mapped_at_creation: false,
            });
            self.queue.write_buffer(&buf, 0, data);
            buf
        };
        let vdata: &[u8] = bytemuck::cast_slice(&verts);
        let idata: &[u8] = bytemuck::cast_slice(&indices);
        let bytes = (vdata.len() + idata.len()) as u64;
        let block = BlockGpu {
            vbuf: make("hp-render lightmap vertices", vdata, wgpu::BufferUsages::VERTEX),
            ibuf: make("hp-render lightmap indices", idata, wgpu::BufferUsages::INDEX),
            draws,
            lo,
            hi,
            bytes,
        };
        self.lightmap.blocks.insert(id, block);
        self.cubes.epoch += 1;
        self.content_gen += 1;
    }

    pub fn remove_lightmap_block(&mut self, id: u32) {
        if self.lightmap.blocks.remove(&id).is_some() {
            self.content_gen += 1;
        }
    }

    pub fn ensure_lightmap_layers(&mut self, size: u32, layers: u32) -> bool {
        let limits = self.device.limits();
        let size = size.clamp(1, self.max_tex_dim);
        if layers == 0 || layers > limits.max_texture_array_layers {
            return false;
        }
        if let Some(a) = &self.lightmap.atlas
            && a.size == size
            && a.layers >= layers
        {
            return true;
        }
        let keep = self.lightmap.atlas.as_ref().filter(|a| a.size == size).map(|a| (a.texture.clone(), a.layers));
        let layers = match &keep {
            Some((_, old)) => layers.max(old * 2).min(limits.max_texture_array_layers),
            None => layers,
        };
        let (texture, oom) = crate::memory::guarded(&self.device, || {
            self.device.create_texture(&wgpu::TextureDescriptor {
                label: Some("hp-render lightmap atlas"),
                size: wgpu::Extent3d { width: size, height: size, depth_or_array_layers: layers },
                mip_level_count: 1,
                sample_count: 1,
                dimension: wgpu::TextureDimension::D2,
                format: ATLAS_FORMAT,
                usage: wgpu::TextureUsages::TEXTURE_BINDING
                    | wgpu::TextureUsages::COPY_DST
                    | wgpu::TextureUsages::COPY_SRC,
                view_formats: &[],
            })
        });
        if oom {
            return false;
        }
        if let Some((old, old_layers)) = keep {
            let mut enc = self
                .device
                .create_command_encoder(&wgpu::CommandEncoderDescriptor { label: Some("hp-render lightmap grow") });
            enc.copy_texture_to_texture(
                old.as_image_copy(),
                texture.as_image_copy(),
                wgpu::Extent3d { width: size, height: size, depth_or_array_layers: old_layers },
            );
            self.queue.submit([enc.finish()]);
        } else {
            for (i, f) in self.lightmap.faces.iter_mut().enumerate() {
                if f.gpu.flags & BAKED != 0 {
                    f.gpu.flags &= !(BAKED | BUMPED);
                    let i = i as u32;
                    let d = &self.lightmap.dirty;
                    let mut d = d.lock().unwrap_or_else(PoisonError::into_inner);
                    *d = Some(match *d {
                        Some((a, b)) => (a.min(i), b.max(i + 1)),
                        None => (i, i + 1),
                    });
                }
            }
        }
        let view = texture.create_view(&wgpu::TextureViewDescriptor {
            dimension: Some(wgpu::TextureViewDimension::D2Array),
            ..Default::default()
        });
        self.lightmap.atlas = Some(Atlas { texture, view, size, layers });
        self.lightmap.bind_group = None;
        self.write_lightmap_params();
        self.flush_lightmap_faces();
        self.ensure_lightmap_resources();
        true
    }

    pub fn write_lightmap(&mut self, face: u32, rect: LightmapRect, texels: &[u32]) {
        self.write_lightmaps(&[(face, rect, texels)]);
    }

    pub fn write_lightmaps(&mut self, writes: &[(u32, LightmapRect, &[u32])]) {
        self.cubes.epoch += 1;
        let Some(atlas) = &self.lightmap.atlas else { return };
        let align = wgpu::COPY_BYTES_PER_ROW_ALIGNMENT;
        let mut ok: Vec<(u32, LightmapRect, &[u32], u64, u32)> = Vec::with_capacity(writes.len());
        for &(face, rect, texels) in writes {
            let images = rect.images();
            let across = rect.w.checked_mul(images);
            let fits = rect.w > 0
                && rect.h > 0
                && rect.layer < atlas.layers
                && across.and_then(|a| rect.x.checked_add(a)).is_some_and(|e| e <= atlas.size)
                && rect.y.checked_add(rect.h).is_some_and(|e| e <= atlas.size)
                && texels.len() as u64 == u64::from(rect.w) * u64::from(rect.h) * u64::from(images);
            if !fits || !self.lightmap.faces.get(face as usize).is_some_and(|f| f.live) {
                continue;
            }
            let row = (rect.w * 4).next_multiple_of(align);
            let bytes = u64::from(row) * u64::from(rect.h) * u64::from(images);
            if bytes <= self.max_buffer {
                ok.push((face, rect, texels, bytes, row));
            }
        }
        let chunk_limit = UPLOAD_CHUNK.min(self.max_buffer);
        let mut done = 0usize;
        while done < ok.len() {
            let mut end = done;
            let mut total = 0u64;
            while end < ok.len() && (end == done || total + ok[end].3 <= chunk_limit) {
                total += ok[end].3;
                end += 1;
            }
            let chunk = &ok[done..end];
            let (staging, oom) = crate::memory::guarded(&self.device, || {
                self.device.create_buffer(&wgpu::BufferDescriptor {
                    label: Some("hp-render lightmap upload"),
                    size: total.max(4),
                    usage: wgpu::BufferUsages::COPY_SRC,
                    mapped_at_creation: true,
                })
            });
            if oom {
                break;
            }
            let mut offsets = Vec::with_capacity(chunk.len());
            {
                let Ok(mut view) = staging.slice(..).get_mapped_range_mut() else { break };
                let mut offset = 0u64;
                for (_, rect, texels, bytes, row) in chunk {
                    offsets.push(offset);
                    for y in 0..(rect.h * rect.images()) as usize {
                        let src = texels.get(y * rect.w as usize..(y + 1) * rect.w as usize).unwrap_or(&[]);
                        let at = offset as usize + y * *row as usize;
                        let bytes: &[u8] = bytemuck::cast_slice(src);
                        if !bytes.is_empty() && at + bytes.len() <= view.len() {
                            view.slice(at..at + bytes.len()).copy_from_slice(bytes);
                        }
                    }
                    offset += bytes;
                }
            }
            staging.unmap();
            let mut enc = self
                .device
                .create_command_encoder(&wgpu::CommandEncoderDescriptor { label: Some("hp-render lightmap upload") });
            for ((_, rect, _, _, row), offset) in chunk.iter().zip(&offsets) {
                for k in 0..rect.images() {
                    enc.copy_buffer_to_texture(
                        wgpu::TexelCopyBufferInfo {
                            buffer: &staging,
                            layout: wgpu::TexelCopyBufferLayout {
                                offset: *offset + u64::from(k) * u64::from(*row) * u64::from(rect.h),
                                bytes_per_row: Some(*row),
                                rows_per_image: Some(rect.h),
                            },
                        },
                        wgpu::TexelCopyTextureInfo {
                            texture: &atlas.texture,
                            mip_level: 0,
                            origin: wgpu::Origin3d { x: rect.x + k * rect.w, y: rect.y, z: rect.layer },
                            aspect: wgpu::TextureAspect::All,
                        },
                        wgpu::Extent3d { width: rect.w, height: rect.h, depth_or_array_layers: 1 },
                    );
                }
            }
            self.queue.submit([enc.finish()]);
            done = end;
        }
        ok.truncate(done);
        if ok.is_empty() {
            return;
        }
        for (face, rect, ..) in ok {
            if let Some(f) = self.lightmap.faces.get_mut(face as usize) {
                f.gpu.origin = [rect.x as f32, rect.y as f32];
                f.gpu.layer = rect.layer;
                f.gpu.density = if rect.density.is_finite() && rect.density > 0.0 { rect.density } else { 1.0 };
                f.gpu.dims = rect.w.min(0xffff) | (rect.h.min(0xffff) << 16);
                f.gpu.flags |= BAKED;
                if rect.bumped {
                    f.gpu.flags |= BUMPED;
                } else {
                    f.gpu.flags &= !BUMPED;
                }
            }
            self.lightmap.mark(face);
        }
        self.flush_lightmap_faces();
        self.content_gen += 1;
    }

    pub fn clear_lightmap_faces(&mut self, faces: &[u32]) {
        for &i in faces {
            if let Some(f) = self.lightmap.faces.get_mut(i as usize)
                && f.gpu.flags & BAKED != 0
            {
                f.gpu.flags &= !(BAKED | BUMPED);
                self.lightmap.mark(i);
            }
        }
        self.flush_lightmap_faces();
    }

    pub fn clear_lightmaps(&mut self) {
        let objects: Vec<ObjectId> = self.lightmap.faces.iter().filter(|f| f.live).map(|f| f.object).collect();
        for o in objects {
            self.scene.release_ref(o);
        }
        let lm = &mut self.lightmap;
        lm.faces.clear();
        lm.geo.clear();
        lm.by_object.clear();
        lm.blocks.clear();
        lm.generations.clear();
        lm.changed.clear();
        *lm.dirty.lock().unwrap_or_else(PoisonError::into_inner) = None;
        lm.face_buf = None;
        lm.atlas = None;
        lm.bind_group = None;
        self.content_gen += 1;
    }

    pub fn lightmap_stats(&self) -> LightmapStats {
        let lm = &self.lightmap;
        let mut s = LightmapStats {
            blocks: lm.blocks.len() as u32,
            layers: lm.atlas.as_ref().map_or(0, |a| a.layers),
            ..Default::default()
        };
        for f in lm.faces.iter().filter(|f| f.live) {
            s.faces += 1;
            if f.gpu.flags & VALID != 0 {
                s.valid_faces += 1;
            }
            if f.gpu.flags & BAKED != 0 {
                s.baked_faces += 1;
            }
        }
        s.bytes = lm.blocks.values().map(|b| b.bytes).sum::<u64>()
            + lm.face_buf.as_ref().map_or(0, wgpu::Buffer::size)
            + lm.atlas.as_ref().map_or(0, |a| u64::from(a.size) * u64::from(a.size) * u64::from(a.layers) * 4);
        s
    }

    pub(crate) fn assign_lightmap_cubemap(&mut self, index: u32) {
        let Some(geo) = self.lightmap.geo.get(&index).copied() else { return };
        let Some(f) = self.lightmap.faces.get(index as usize).filter(|f| f.live) else { return };
        let cube = u32::from(self.cubes.face_cubemap(f.object, geo.center, geo.normal));
        if let Some(f) = self.lightmap.faces.get_mut(index as usize)
            && f.gpu.cube != cube
        {
            f.gpu.cube = cube;
            self.lightmap.mark(index);
        }
    }

    pub(crate) fn assign_lightmap_cubemaps(&mut self) {
        if self.lightmap.geo.is_empty() {
            return;
        }
        for i in 0..self.lightmap.faces.len() as u32 {
            self.assign_lightmap_cubemap(i);
        }
        self.flush_lightmap_faces();
    }

    fn flush_lightmap_faces(&mut self) {
        let lm = &mut self.lightmap;
        let need = (lm.faces.len().max(1) * size_of::<FaceGpu>()) as u64;
        let grow = lm.face_buf.as_ref().is_none_or(|b| b.size() < need);
        if grow {
            let size = need.next_power_of_two().max(4096).min(self.max_storage.max(need));
            let buf = self.device.create_buffer(&wgpu::BufferDescriptor {
                label: Some("hp-render lightmap faces"),
                size,
                usage: wgpu::BufferUsages::STORAGE | wgpu::BufferUsages::COPY_DST,
                mapped_at_creation: false,
            });
            lm.face_buf = Some(buf);
            lm.bind_group = None;
            lm.mark_all();
        }
        if lm.upload_dirty(&self.queue) {
            self.content_gen += 1;
        }
        if grow {
            self.ensure_lightmap_resources();
        }
    }

    fn ensure_lightmap_resources(&mut self) {
        if self.lightmap.display == LightmapDisplay::Off {
            return;
        }
        if self.lightmap.atlas.is_none() {
            self.ensure_lightmap_layers(1, 1);
        }
        if self.lightmap.params_buf.is_none() {
            let b = self.device.create_buffer(&wgpu::BufferDescriptor {
                label: Some("hp-render lightmap params"),
                size: size_of::<ParamsGpu>() as u64,
                usage: wgpu::BufferUsages::UNIFORM | wgpu::BufferUsages::COPY_DST,
                mapped_at_creation: false,
            });
            self.lightmap.params_buf = Some(b);
            self.write_lightmap_params();
        }
        if self.lightmap.face_buf.is_none() {
            self.flush_lightmap_faces();
        }
        if self.lightmap.bind_group.is_some() {
            return;
        }
        let lm = &self.lightmap;
        let layout = lm.bind_layout(&self.device);
        let (Some(atlas), Some(faces), Some(params)) = (&lm.atlas, &lm.face_buf, &lm.params_buf) else { return };
        let bg = self.device.create_bind_group(&wgpu::BindGroupDescriptor {
            label: Some("hp-render lightmap"),
            layout,
            entries: &[
                wgpu::BindGroupEntry { binding: 0, resource: wgpu::BindingResource::TextureView(&atlas.view) },
                wgpu::BindGroupEntry { binding: 1, resource: faces.as_entire_binding() },
                wgpu::BindGroupEntry { binding: 2, resource: params.as_entire_binding() },
            ],
        });
        self.lightmap.bind_group = Some(bg);
    }
}

fn pipeline(
    device: &wgpu::Device,
    module: &wgpu::ShaderModule,
    layout: &wgpu::PipelineLayout,
    samples: u32,
    fs: &str,
    alpha_test: bool,
    two_sided: bool,
) -> wgpu::RenderPipeline {
    let constants = [("ALPHA_TEST", if alpha_test { 1.0 } else { 0.0 })];
    let options = wgpu::PipelineCompilationOptions { constants: &constants, zero_initialize_workgroup_memory: false };
    let buffers = [Some(wgpu::VertexBufferLayout {
        array_stride: size_of::<LightmapVertex>() as u64,
        step_mode: wgpu::VertexStepMode::Vertex,
        attributes: &VERTEX_ATTRS,
    })];
    device.create_render_pipeline(&wgpu::RenderPipelineDescriptor {
        label: Some("hp-render lightmap"),
        layout: Some(layout),
        vertex: wgpu::VertexState {
            module,
            entry_point: Some("vs_lm"),
            compilation_options: options.clone(),
            buffers: &buffers,
        },
        primitive: wgpu::PrimitiveState {
            topology: wgpu::PrimitiveTopology::TriangleList,
            strip_index_format: None,
            front_face: wgpu::FrontFace::Ccw,
            cull_mode: if two_sided { None } else { Some(wgpu::Face::Back) },
            unclipped_depth: false,
            polygon_mode: wgpu::PolygonMode::Fill,
            conservative: false,
        },
        depth_stencil: Some(wgpu::DepthStencilState {
            format: DEPTH_FORMAT,
            depth_write_enabled: Some(false),
            depth_compare: Some(wgpu::CompareFunction::GreaterEqual),
            stencil: wgpu::StencilState::default(),
            bias: wgpu::DepthBiasState { constant: 4, slope_scale: 0.5, clamp: 0.0 },
        }),
        multisample: wgpu::MultisampleState { count: samples, mask: !0, alpha_to_coverage_enabled: false },
        fragment: Some(wgpu::FragmentState {
            module,
            entry_point: Some(fs),
            compilation_options: options,
            targets: &[Some(wgpu::ColorTargetState {
                format: COLOR_FORMAT,
                blend: None,
                write_mask: wgpu::ColorWrites::ALL,
            })],
        }),
        multiview_mask: None,
        cache: None,
    })
}

fn box_visible(view_proj: &Mat4, lo: Vec3, hi: Vec3) -> bool {
    let mut outside = [true; 5];
    for c in 0..8 {
        let p = Vec3::new(
            if c & 1 != 0 { hi.x } else { lo.x },
            if c & 2 != 0 { hi.y } else { lo.y },
            if c & 4 != 0 { hi.z } else { lo.z },
        );
        let q: Vec4 = *view_proj * p.extend(1.0);
        let tests = [q.x < -q.w, q.x > q.w, q.y < -q.w, q.y > q.w, q.w <= 0.0];
        for (o, t) in outside.iter_mut().zip(tests) {
            *o &= t;
        }
    }
    !outside.iter().any(|o| *o)
}

pub(crate) struct LightmapPass;

impl PassHook for LightmapPass {
    fn name(&self) -> &'static str {
        "lightmap"
    }

    fn prepare(&mut self, cx: &HookContext<'_>, _encoder: &mut wgpu::CommandEncoder) {
        cx.renderer.lightmap.upload_dirty(cx.queue());
    }

    fn draw_opaque(&self, cx: &HookContext<'_>, pass: &mut wgpu::RenderPass<'_>) {
        let r = cx.renderer;
        let lm = &r.lightmap;
        if cx.is_ortho || lm.display == LightmapDisplay::Off || lm.blocks.is_empty() {
            return;
        }
        let Some(bind_group) = &lm.bind_group else { return };
        let device = cx.device();
        let module = lm.module.get_or_init(|| {
            device.create_shader_module(wgpu::ShaderModuleDescriptor {
                label: Some("hp-render lightmap"),
                source: wgpu::ShaderSource::Wgsl(SHADER.into()),
            })
        });
        let layout = lm.pipeline_layout.get_or_init(|| {
            device.create_pipeline_layout(&wgpu::PipelineLayoutDescriptor {
                label: Some("hp-render lightmap"),
                bind_group_layouts: &[
                    Some(&r.shared.view_bgl),
                    Some(&r.shared.material_bgl),
                    Some(lm.bind_layout(device)),
                ],
                immediate_size: 0,
            })
        });
        let si = usize::from(cx.samples > 1);
        let pipes = lm.pipelines[si].get_or_init(|| {
            let mk = |fs: &str| {
                [false, true].map(|alpha| {
                    [false, true].map(|two| pipeline(cx.device(), module, layout, cx.samples, fs, alpha, two))
                })
            };
            Pipelines { baked: mk("fs_baked"), grid: mk("fs_grid") }
        });
        let set = if lm.display == LightmapDisplay::Grid { &pipes.grid } else { &pipes.baked };
        let view_proj = Mat4::from_cols_array_2d(&cx.view_proj);
        pass.set_bind_group(0, cx.view_bind_group, &[]);
        pass.set_bind_group(2, bind_group, &[]);
        let mut current: Option<(usize, usize)> = None;
        let mut material = u32::MAX;
        for block in lm.blocks.values() {
            if !box_visible(&view_proj, block.lo, block.hi) {
                continue;
            }
            pass.set_vertex_buffer(0, block.vbuf.slice(..));
            pass.set_index_buffer(block.ibuf.slice(..), wgpu::IndexFormat::Uint32);
            for d in &block.draws {
                let desc = r.scene.materials.get(d.material as usize).and_then(|m| m.desc);
                let Some((kind, two_sided)) = classify_with(desc.as_ref(), cx.options.force_opaque, false) else {
                    continue;
                };
                let alpha = match kind {
                    MeshPass::Opaque => 0,
                    MeshPass::AlphaTest => 1,
                    MeshPass::Translucent | MeshPass::Additive | MeshPass::Modulate => continue,
                };
                let Some(mg) = r.mat_gpu.get(d.material as usize) else { continue };
                let key = (alpha, usize::from(two_sided));
                if current != Some(key) {
                    pass.set_pipeline(&set[key.0][key.1]);
                    current = Some(key);
                }
                if material != d.material {
                    pass.set_bind_group(1, &mg.bind_group, &[]);
                    material = d.material;
                }
                pass.draw_indexed(d.first..d.first + d.count, 0, 0..1);
            }
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn layouts() {
        assert_eq!(size_of::<LightmapVertex>(), 40);
        assert_eq!(VERTEX_ATTRS.last().map(|a| a.offset), Some(36));
        assert_eq!(size_of::<FaceGpu>(), 40, "LmFace of lightmap.wgsl, with its cubemap");
        assert_eq!(size_of::<ParamsGpu>(), 16);
    }

    #[test]
    fn bump_flags_stay_clear_of_the_static_ones() {
        assert_eq!(BUMPED & (STATIC_FLAGS | VALID | BAKED), 0);
        let r = LightmapRect { layer: 0, x: 0, y: 0, w: 3, h: 2, density: 1.0, bumped: true };
        assert_eq!(r.images(), 4);
        assert_eq!(LightmapRect { bumped: false, ..r }.images(), 1);
        assert_eq!(pack_blend([1.0, 0.0, 0.0, 0.0]), crate::gpu::BLEND_PAINTED);
        assert_eq!(pack_blend([0.0, 1.0, 0.0, 0.0]), 1023 | crate::gpu::BLEND_PAINTED);
    }

    #[test]
    fn frustum_test() {
        let vp = Mat4::from_cols(Vec4::W, -Vec4::X, Vec4::Y, Vec4::Z);
        assert!(box_visible(&vp, Vec3::new(100.0, -10.0, -10.0), Vec3::new(120.0, 10.0, 10.0)));
        assert!(!box_visible(&vp, Vec3::new(-120.0, -10.0, -10.0), Vec3::new(-100.0, 10.0, 10.0)), "behind");
        assert!(!box_visible(&vp, Vec3::new(100.0, 500.0, -10.0), Vec3::new(120.0, 520.0, 10.0)), "left");
        assert!(box_visible(&vp, Vec3::splat(-5.0), Vec3::splat(5.0)));
    }

    #[test]
    fn generations_hide_stale_faces() {
        let mut lm = Lightmaps::default();
        lm.object_changed(5);
        assert_eq!(lm.generation(5), 0);
        lm.tracking = true;
        lm.object_changed(5);
        assert_eq!(lm.generation(5), 1);
        assert!(lm.changed.contains(&5));
        lm.faces.push(FaceCpu { gpu: FaceGpu { flags: VALID, ..Default::default() }, object: 5, live: true });
        lm.by_object.insert(5, vec![0]);
        lm.tracking = false;
        lm.object_changed(5);
        assert_eq!(lm.generation(5), 2, "objects with faces always count");
        assert_eq!(lm.faces[0].gpu.flags & VALID, 0, "stale faces are hidden");
        assert_eq!(*lm.dirty.lock().unwrap(), Some((0, 1)));
        lm.objects_cleared();
        assert_eq!(lm.generation(5), 3);
    }
}
