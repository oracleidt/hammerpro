use std::sync::OnceLock;

use bytemuck::{Pod, Zeroable};
use glam::{Mat4, Vec3, Vec4};

use crate::hooks::{HookContext, PassHook};
use crate::pipelines::{COLOR_FORMAT, DEPTH_FORMAT, MeshPass};
use crate::{MaterialId, ModelId, ObjectId, Renderer, classify_with};

#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub enum CompiledLightMode {
    #[default]
    Fullbright,
    Ldr,
    Hdr,
}

#[derive(Clone, Copy, Debug)]
pub struct CompiledLightmap<'a> {
    pub width: u32,
    pub height: u32,
    pub texels: &'a [[f32; 3]],
    pub styles: &'a [u8],
    pub bumped: bool,
}

pub const NO_COMPILED_LIGHTMAP: u32 = u32::MAX;

#[derive(Clone, Copy, Debug, Default, PartialEq)]
pub struct CompiledVertex {
    pub pos: [f32; 3],
    pub normal: [f32; 3],
    pub uv: [f32; 2],
    pub alpha: f32,
    pub lm: [f32; 2],
    pub lightmap: u32,
    pub cubemap: u16,
    pub blend: crate::types::BlendVertex,
}

#[derive(Clone, Debug, Default, PartialEq)]
pub struct CompiledBatch {
    pub material: MaterialId,
    pub vertices: Vec<CompiledVertex>,
    pub indices: Vec<u32>,
}

#[derive(Clone, Debug, Default, PartialEq)]
pub struct CompiledBlock {
    pub object: ObjectId,
    pub batches: Vec<CompiledBatch>,
}

#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub struct CompiledLightStats {
    pub mode: CompiledLightMode,
    pub lightmaps: u32,
    pub dropped: u32,
    pub animated: u32,
    pub atlas_size: u32,
    pub atlas_height: u32,
    pub atlas_layers: u32,
    pub blocks: u32,
    pub bytes: u64,
}

#[derive(Clone, Copy, Debug, Default, PartialEq)]
pub struct ModelLocalLight {
    pub kind: u32,
    pub origin: [f32; 3],
    pub dir: [f32; 3],
    pub color: [f32; 3],
    pub attenuation: [f32; 3],
    pub cone: [f32; 3],
    pub radius: f32,
    pub style: u32,
}

#[derive(Clone, Debug, Default, PartialEq)]
pub struct ModelLighting {
    pub object: ObjectId,
    pub sample_origin: Option<[f32; 3]>,
    pub model: Option<ModelId>,
    pub vertex_colors: Vec<[u8; 4]>,
    pub ambient: [[f32; 3]; 6],
    pub lights: Vec<ModelLocalLight>,
    pub styled_lights: Vec<ModelLocalLight>,
}

#[repr(C)]
#[derive(Clone, Copy, Debug, Default, PartialEq, Pod, Zeroable)]
struct VertexGpu {
    pos: [f32; 3],
    uv: [f32; 2],
    lm: [f32; 2],
    alpha: f32,
    slot: u32,
    layer: u32,
    styles: u32,
    stride: f32,
    cube: u32,
    normal: [f32; 3],
    blend: u32,
}

const VERTEX_ATTRS: [wgpu::VertexAttribute; 11] = [
    wgpu::VertexAttribute { format: wgpu::VertexFormat::Float32x3, offset: 0, shader_location: 0 },
    wgpu::VertexAttribute { format: wgpu::VertexFormat::Float32x2, offset: 12, shader_location: 1 },
    wgpu::VertexAttribute { format: wgpu::VertexFormat::Float32x2, offset: 20, shader_location: 2 },
    wgpu::VertexAttribute { format: wgpu::VertexFormat::Float32, offset: 28, shader_location: 3 },
    wgpu::VertexAttribute { format: wgpu::VertexFormat::Uint32, offset: 32, shader_location: 4 },
    wgpu::VertexAttribute { format: wgpu::VertexFormat::Uint32, offset: 36, shader_location: 5 },
    wgpu::VertexAttribute { format: wgpu::VertexFormat::Uint32, offset: 40, shader_location: 6 },
    wgpu::VertexAttribute { format: wgpu::VertexFormat::Float32, offset: 44, shader_location: 7 },
    wgpu::VertexAttribute { format: wgpu::VertexFormat::Uint32, offset: 48, shader_location: 12 },
    wgpu::VertexAttribute { format: wgpu::VertexFormat::Float32x3, offset: 52, shader_location: 13 },
    wgpu::VertexAttribute { format: wgpu::VertexFormat::Uint32, offset: 64, shader_location: 14 },
];

const TRANSFORM_ATTRS: [wgpu::VertexAttribute; 4] = [
    wgpu::VertexAttribute { format: wgpu::VertexFormat::Float32x4, offset: 0, shader_location: 8 },
    wgpu::VertexAttribute { format: wgpu::VertexFormat::Float32x4, offset: 16, shader_location: 9 },
    wgpu::VertexAttribute { format: wgpu::VertexFormat::Float32x4, offset: 32, shader_location: 10 },
    wgpu::VertexAttribute { format: wgpu::VertexFormat::Float32x4, offset: 48, shader_location: 11 },
];

pub const LIGHT_STYLES: usize = 128;
const MAX_LIGHTMAP_STYLES: usize = 4;
const STATIC_STYLES: u32 = u32::MAX;
const LAYER_BUMPED: u32 = 0x4000_0000;
const BUMP_IMAGES: usize = 4;

pub(crate) fn light_styles_buffer(device: &wgpu::Device) -> wgpu::Buffer {
    let buf = device.create_buffer(&wgpu::BufferDescriptor {
        label: Some("hp-render light styles"),
        size: (LIGHT_STYLES * 4) as u64,
        usage: wgpu::BufferUsages::UNIFORM | wgpu::BufferUsages::COPY_DST,
        mapped_at_creation: true,
    });
    if let Ok(mut view) = buf.slice(..).get_mapped_range_mut() {
        view.copy_from_slice(bytemuck::cast_slice(&[1.0f32; LIGHT_STYLES]));
    }
    buf.unmap();
    buf
}

fn pack_styles(styles: &[u8]) -> (u32, usize) {
    if styles.is_empty() {
        return (STATIC_STYLES, 1);
    }
    let n = styles.len().min(MAX_LIGHTMAP_STYLES);
    let mut bytes = [255u8; 4];
    bytes[..n].copy_from_slice(&styles[..n]);
    (u32::from_le_bytes(bytes), n)
}

const IDENTITY: [[f32; 4]; 4] =
    [[1.0, 0.0, 0.0, 0.0], [0.0, 1.0, 0.0, 0.0], [0.0, 0.0, 1.0, 0.0], [0.0, 0.0, 0.0, 1.0]];

#[repr(C)]
#[derive(Clone, Copy, Debug, Default, PartialEq, Pod, Zeroable)]
struct ParamsGpu {
    mode: u32,
    scale: f32,
    pad: [f32; 2],
}

struct Draw {
    material: u32,
    first: u32,
    count: u32,
    faces: Vec<(u32, u32, Vec3)>,
}

fn face_runs(vertices: &[CompiledVertex], indices: &[u32]) -> Vec<(u32, u32, Vec3)> {
    let mut out: Vec<(u32, u32, Vec3)> = Vec::new();
    let (mut start, mut top) = (0u32, None::<u32>);
    let (mut lo, mut hi) = (Vec3::splat(f32::INFINITY), Vec3::splat(f32::NEG_INFINITY));
    let tris = indices.len() / 3;
    for (i, t) in indices.chunks_exact(3).enumerate() {
        let (min, max) = (t.iter().copied().min().unwrap_or(0), t.iter().copied().max().unwrap_or(0));
        if top.is_some_and(|top| min > top) {
            out.push((start, i as u32 - start, (lo + hi) * 0.5));
            start = i as u32;
            lo = Vec3::splat(f32::INFINITY);
            hi = Vec3::splat(f32::NEG_INFINITY);
            top = None;
        }
        top = Some(top.map_or(max, |top| top.max(max)));
        for &v in t.iter() {
            if let Some(p) = vertices.get(v as usize).map(|v| Vec3::from(v.pos)).filter(|p| p.is_finite()) {
                lo = lo.min(p);
                hi = hi.max(p);
            }
        }
    }
    if (start as usize) < tris {
        out.push((start, tris as u32 - start, (lo + hi) * 0.5));
    }
    for f in &mut out {
        if !f.2.is_finite() {
            f.2 = Vec3::ZERO;
        }
    }
    out
}

struct Segment {
    vbuf: wgpu::Buffer,
    ibuf: wgpu::Buffer,
}

struct BlockGpu {
    object: ObjectId,
    moved: bool,
    segment: u32,
    base_vertex: i32,
    draws: Vec<Draw>,
    lo: Vec3,
    hi: Vec3,
    bytes: u64,
}

struct Pipelines {
    opaque: [[wgpu::RenderPipeline; 2]; 2],
    blended: [[wgpu::RenderPipeline; 2]; 2],
    faded: [[wgpu::RenderPipeline; 2]; 2],
}

#[derive(Default)]
pub(crate) struct CompiledLight {
    mode: CompiledLightMode,
    pub(crate) present: bool,
    blocks: Vec<BlockGpu>,
    segments: Vec<Segment>,
    order: Vec<(u32, u32, u32)>,
    blocks_gen: u64,
    states: Vec<(crate::ObjectFlags, f32)>,
    states_key: (u64, u64),
    visible: Vec<bool>,
    visible_for: Option<[f32; 16]>,
    transforms: Vec<[[f32; 4]; 4]>,
    transform_buf: Option<wgpu::Buffer>,
    atlas: Option<(wgpu::Texture, u32, u32)>,
    params: Option<wgpu::Buffer>,
    bind_group: Option<wgpu::BindGroup>,
    layout: OnceLock<wgpu::BindGroupLayout>,
    pipeline_layout: OnceLock<wgpu::PipelineLayout>,
    module: OnceLock<wgpu::ShaderModule>,
    pipelines: [OnceLock<Pipelines>; 2],
    stats: CompiledLightStats,
    style_values: Vec<f32>,
}

pub(crate) const SHADER: &str = concat!(
    include_str!("shaders/common.wgsl"),
    "\n",
    include_str!("shaders/material.wgsl"),
    "\n",
    include_str!("shaders/parallax.wgsl"),
    "\n",
    include_str!("shaders/compiled_light.wgsl")
);

const ATLAS_FORMAT: wgpu::TextureFormat = wgpu::TextureFormat::Rgb9e5Ufloat;
const TEXEL_BYTES: u32 = 4;
const UPLOAD_BAND_BYTES: usize = 32 << 20;
const MAX_PAGE: u32 = 4096;
const PAD: u32 = 1;

pub(crate) fn f16_bits(v: f32) -> u16 {
    let x = v.to_bits();
    let sign = ((x >> 16) & 0x8000) as u16;
    let exp = ((x >> 23) & 0xff) as i32;
    let mant = x & 0x7f_ffff;
    if exp == 0xff {
        return sign | 0x7c00 | if mant != 0 { 0x200 } else { 0 };
    }
    let e = exp - 127 + 15;
    if e >= 0x1f {
        return sign | 0x7c00;
    }
    if e <= 0 {
        if e < -10 {
            return sign;
        }
        let m = mant | 0x80_0000;
        let shift = (14 - e) as u32;
        let half = m >> shift;
        let rest = m & ((1 << shift) - 1);
        let mid = 1 << (shift - 1);
        let round = rest > mid || (rest == mid && half & 1 != 0);
        return sign | (half + u32::from(round)) as u16;
    }
    let half = ((e as u32) << 10) | (mant >> 13);
    let rest = mant & 0x1fff;
    let round = rest > 0x1000 || (rest == 0x1000 && half & 1 != 0);
    sign | (half + u32::from(round)) as u16
}

pub(crate) type Place = (u32, u32, u32);

pub(crate) fn rgb9e5(c: [f32; 3]) -> u32 {
    const MAX: f32 = 65408.0;
    let clean = |v: f32| if v.is_finite() { v.clamp(0.0, MAX) } else { 0.0 };
    let [r, g, b] = c.map(clean);
    let top = r.max(g).max(b);
    if top <= 0.0 {
        return 0;
    }
    let pow2 = |e: i32| f32::from_bits(((e + 127).clamp(1, 254) as u32) << 23);
    let floor_log2 = ((top.to_bits() >> 23) & 0xff) as i32 - 127;
    let mut exp = floor_log2.max(-16) + 16;
    if (top / pow2(exp - 24) + 0.5).floor() >= 512.0 {
        exp += 1;
    }
    let inv = 1.0 / pow2(exp - 24);
    let q = |v: f32| ((v * inv + 0.5) as u32).min(511);
    q(r) | (q(g) << 9) | (q(b) << 18) | ((exp.clamp(0, 31) as u32) << 27)
}

#[cfg(test)]
pub(crate) fn rgb9e5_value(v: u32) -> [f32; 3] {
    let scale = 2f32.powi((v >> 27) as i32 - 24);
    [(v & 511) as f32 * scale, ((v >> 9) & 511) as f32 * scale, ((v >> 18) & 511) as f32 * scale]
}

pub(crate) struct Shelf {
    pub layer: u32,
    pub y: u32,
    pub height: u32,
    pub members: Vec<usize>,
}

pub(crate) struct Packing {
    pub places: Vec<Option<Place>>,
    pub shelves: Vec<Shelf>,
    pub layers: u32,
    pub height: u32,
}

pub(crate) fn pack_atlas(sizes: &[(u32, u32)], width: u32, height: u32, max_layers: u32) -> Packing {
    let mut order: Vec<usize> = (0..sizes.len()).collect();
    order.sort_by(|&a, &b| sizes[b].1.cmp(&sizes[a].1).then(sizes[b].0.cmp(&sizes[a].0)).then(a.cmp(&b)));
    let mut places = vec![None; sizes.len()];
    let mut shelves: Vec<Shelf> = Vec::new();
    let (mut layer, mut x, mut y, mut shelf) = (0u32, 0u32, 0u32, 0u32);
    let (mut used_layers, mut used_height) = (0, 0);
    for i in order {
        let (w, h) = (sizes[i].0 + 2 * PAD, sizes[i].1 + 2 * PAD);
        if w > width || h > height || sizes[i].0 == 0 || sizes[i].1 == 0 {
            continue;
        }
        if x + w > width {
            y += shelf;
            x = 0;
            shelf = 0;
        }
        if y + h > height {
            layer += 1;
            x = 0;
            y = 0;
            shelf = 0;
        }
        if layer >= max_layers {
            break;
        }
        if x == 0 {
            shelves.push(Shelf { layer, y, height: 0, members: Vec::new() });
        }
        if let Some(s) = shelves.last_mut() {
            s.height = s.height.max(h);
            s.members.push(i);
        }
        places[i] = Some((layer, x + PAD, y + PAD));
        used_layers = layer + 1;
        used_height = used_height.max(y + h);
        x += w;
        shelf = shelf.max(h);
    }
    Packing { places, shelves, layers: used_layers, height: used_height }
}

pub(crate) fn atlas_layout(sizes: &[(u32, u32)], max_dim: u32, max_layers: u32) -> (Packing, u32) {
    let fits = |&&(w, h): &&(u32, u32)| w > 0 && h > 0 && w + 2 * PAD <= max_dim && h + 2 * PAD <= max_dim;
    let area: u64 = sizes.iter().filter(fits).map(|&(w, h)| u64::from(w + 2 * PAD) * u64::from(h + 2 * PAD)).sum();
    let widest = sizes.iter().filter(fits).map(|&(w, _)| w + 2 * PAD).max().unwrap_or(1);
    let side = ((area as f64 * 1.15).sqrt().ceil() as u32).max(widest).max(16).next_multiple_of(16).min(max_dim);
    let packing = pack_atlas(sizes, side, max_dim, max_layers);
    if packing.layers <= 1 || side >= max_dim {
        return (packing, side);
    }
    (pack_atlas(sizes, max_dim, max_dim, max_layers), max_dim)
}

struct BuiltBlock {
    verts: Vec<VertexGpu>,
    indices: Vec<u32>,
    draws: Vec<Draw>,
    lo: Vec3,
    hi: Vec3,
}

struct AtlasRef<'a> {
    places: &'a [Option<Place>],
    style_of: &'a [(u32, f32, u32)],
    inv: [f32; 2],
    limit: usize,
}

fn build_block(
    block: &CompiledBlock,
    slot: u32,
    materials: &[(MaterialId, u32)],
    atlas: &AtlasRef<'_>,
) -> Option<BuiltBlock> {
    let total_v: usize = block.batches.iter().map(|b| b.vertices.len()).sum();
    let total_i: usize = block.batches.iter().map(|b| b.indices.len()).sum();
    let mut verts: Vec<VertexGpu> = Vec::with_capacity(total_v.min(atlas.limit));
    let mut indices: Vec<u32> = Vec::with_capacity(total_i);
    let mut draws = Vec::with_capacity(block.batches.len());
    let mut kept: Vec<u32> = Vec::new();
    let (mut lo, mut hi) = (Vec3::splat(f32::INFINITY), Vec3::splat(f32::NEG_INFINITY));
    let mut batches: Vec<&CompiledBatch> = block.batches.iter().collect();
    batches.sort_by_key(|b| b.material);
    for b in batches {
        if verts.len() + b.vertices.len() > atlas.limit {
            log::warn!("hp-render: compiled lighting block too large, truncated");
            break;
        }
        let base = verts.len() as u32;
        let first = indices.len() as u32;
        for v in &b.vertices {
            let p = Vec3::from(v.pos);
            let ok = p.is_finite() && v.uv.iter().chain(&v.lm).all(|c| c.is_finite()) && v.alpha.is_finite();
            if ok {
                lo = lo.min(p);
                hi = hi.max(p);
            }
            let place = atlas.places.get(v.lightmap as usize).copied().flatten();
            let (layer, lm) = match place {
                Some((layer, x, y)) => {
                    (layer, [(x as f32 + v.lm[0]) * atlas.inv[0], (y as f32 + v.lm[1]) * atlas.inv[1]])
                }
                None => (NO_COMPILED_LIGHTMAP, [0.0; 2]),
            };
            let (styles, stride, bump) = match place {
                Some(_) => atlas.style_of[v.lightmap as usize],
                None => (STATIC_STYLES, 0.0, 0),
            };
            let layer = if layer == NO_COMPILED_LIGHTMAP { layer } else { layer | bump };
            verts.push(if ok {
                VertexGpu {
                    pos: v.pos,
                    uv: v.uv,
                    lm,
                    alpha: v.alpha,
                    slot,
                    layer,
                    styles,
                    stride,
                    cube: u32::from(v.cubemap),
                    normal: if Vec3::from(v.normal).is_finite() { v.normal } else { [0.0, 0.0, 1.0] },
                    blend: crate::gpu::pack_blend_vertex(&v.blend),
                }
            } else {
                VertexGpu {
                    slot,
                    layer: NO_COMPILED_LIGHTMAP,
                    styles: STATIC_STYLES,
                    cube: u32::from(crate::NO_CUBEMAP),
                    ..Default::default()
                }
            });
        }
        let n = b.vertices.len() as u32;
        kept.clear();
        for t in b.indices.chunks_exact(3) {
            if !t.iter().all(|i| *i < n) {
                continue;
            }
            let [a, bb, c] = [t[0], t[1], t[2]].map(|i| &b.vertices[i as usize]);
            let (pa, pb, pc) = (Vec3::from(a.pos), Vec3::from(bb.pos), Vec3::from(c.pos));
            if !(pa.is_finite() && pb.is_finite() && pc.is_finite()) {
                continue;
            }
            let geo = (pb - pa).cross(pc - pa);
            let avg = Vec3::from(a.normal) + Vec3::from(bb.normal) + Vec3::from(c.normal);
            if geo.dot(avg) < 0.0 {
                indices.extend_from_slice(&[base + t[0], base + t[2], base + t[1]]);
            } else {
                indices.extend_from_slice(&[base + t[0], base + t[1], base + t[2]]);
            }
            kept.extend_from_slice(t);
        }
        let count = indices.len() as u32 - first;
        if count > 0 {
            let faces = face_runs(&b.vertices, &kept)
                .into_iter()
                .map(|(tri, tris, center)| (first + 3 * tri, 3 * tris, center))
                .collect();
            let material = materials.iter().find(|(m, _)| *m == b.material).map_or(0, |(_, s)| *s);
            draws.push(Draw { material, first, count, faces });
        }
    }
    (!draws.is_empty() && lo.cmple(hi).all()).then_some(BuiltBlock { verts, indices, draws, lo, hi })
}

fn fill_shelf(
    band: &mut [u32],
    shelf: &Shelf,
    width: u32,
    lightmaps: &[CompiledLightmap<'_>],
    images: &[usize],
    places: &[Option<Place>],
) {
    let pad = PAD as usize;
    let width = width as usize;
    let mut line: Vec<u32> = Vec::new();
    for &i in &shelf.members {
        let (Some(l), Some(&n), Some(Some((_, x0, y0)))) = (lightmaps.get(i), images.get(i), places.get(i)) else {
            continue;
        };
        let (w, h) = (l.width as usize, l.height as usize);
        let top = *y0 as usize - pad - shelf.y as usize;
        for k in 0..n {
            let first = k * w * h;
            let left = *x0 as usize + k * (w + 2 * pad) - pad;
            for y in 0..h {
                let src = &l.texels[first + y * w..first + (y + 1) * w];
                line.clear();
                line.extend(std::iter::repeat_n(rgb9e5(src[0]), pad));
                line.extend(src.iter().map(|t| rgb9e5(*t)));
                line.extend(std::iter::repeat_n(rgb9e5(src[w - 1]), pad));
                let rows: &[usize] = match (y == 0, y + 1 == h) {
                    (true, true) => &[0, 1, 2],
                    (true, false) => &[0, 1],
                    (false, true) => &[1, 2],
                    (false, false) => &[1],
                };
                for &r in rows {
                    let row = top + y + r;
                    let at = row * width + left;
                    band[at..at + line.len()].copy_from_slice(&line);
                }
            }
        }
    }
}

impl CompiledLight {
    fn state(&self, r: &Renderer, i: usize, b: &BlockGpu) -> (crate::ObjectFlags, f32) {
        match self.states.get(i).filter(|_| self.states_key == (r.scene.state_gen, self.blocks_gen)) {
            Some(&s) => s,
            None => (r.object_flags(b.object).unwrap_or_default(), r.object_tint(b.object).map_or(1.0, |t| t[3])),
        }
    }

    fn framed(&self, view_proj: &Mat4) -> Option<&[bool]> {
        let same = self.visible_for == Some(view_proj.to_cols_array()) && self.visible.len() == self.blocks.len();
        same.then_some(&self.visible[..])
    }

    fn bind_block(&self, pass: &mut wgpu::RenderPass<'_>, block: &BlockGpu) {
        if let Some(seg) = self.segments.get(block.segment as usize) {
            pass.set_vertex_buffer(0, seg.vbuf.slice(..));
            pass.set_index_buffer(seg.ibuf.slice(..), wgpu::IndexFormat::Uint32);
        }
    }

    fn bind_layout(&self, device: &wgpu::Device) -> &wgpu::BindGroupLayout {
        self.layout.get_or_init(|| {
            device.create_bind_group_layout(&wgpu::BindGroupLayoutDescriptor {
                label: Some("hp-render compiled light"),
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
                        visibility: wgpu::ShaderStages::FRAGMENT,
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

    pub(crate) fn mode(&self) -> Option<CompiledLightMode> {
        self.present.then_some(self.mode)
    }
}

impl Renderer {
    pub fn set_compiled_lighting(
        &mut self,
        mode: CompiledLightMode,
        lightmaps: &[CompiledLightmap<'_>],
        blocks: &[CompiledBlock],
    ) -> CompiledLightStats {
        use rayon::prelude::*;
        self.clear_compiled_lighting();
        self.cubes.epoch += 1;
        let limits = self.device.limits();
        let per_style = |l: &CompiledLightmap<'_>| if l.bumped { BUMP_IMAGES } else { 1 };
        let images: Vec<usize> = lightmaps.iter().map(|l| pack_styles(l.styles).1 * per_style(l)).collect();
        let sizes: Vec<(u32, u32)> = lightmaps
            .iter()
            .zip(&images)
            .map(|(l, &n)| {
                let texels = (l.width as usize) * (l.height as usize);
                let ok = l.width > 0 && l.height > 0 && texels * l.styles.len().max(1) * per_style(l) == l.texels.len();
                let across = (n as u64) * u64::from(l.width + 2 * PAD) - u64::from(2 * PAD);
                match u32::try_from(across) {
                    Ok(across) if ok => (across, l.height),
                    _ => (0, 0),
                }
            })
            .collect();
        let max_dim = MAX_PAGE.min(self.max_tex_dim);
        let max_layers = limits.max_texture_array_layers.min(256);
        let (packing, width) = atlas_layout(&sizes, max_dim, max_layers);
        let layers = packing.layers.max(1);
        let height = if packing.layers > 1 { max_dim } else { packing.height.max(1) };
        let places = &packing.places;
        let mut stats = CompiledLightStats {
            mode,
            lightmaps: places.iter().filter(|p| p.is_some()).count() as u32,
            dropped: sizes.iter().zip(places).filter(|(s, p)| s.0 > 0 && p.is_none()).count() as u32,
            atlas_size: width,
            atlas_height: height,
            atlas_layers: layers,
            animated: lightmaps.iter().zip(places).filter(|(l, p)| p.is_some() && !l.styles.is_empty()).count() as u32,
            ..Default::default()
        };
        let layer_texels = (width as usize) * (height as usize);
        let mut data = vec![0u32; layer_texels * layers as usize];
        {
            let mut bands: Vec<(&mut [u32], &Shelf)> = Vec::with_capacity(packing.shelves.len());
            let mut rest: &mut [u32] = &mut data;
            let mut at = 0usize;
            for shelf in &packing.shelves {
                let start = shelf.layer as usize * layer_texels + shelf.y as usize * width as usize;
                let rows = (shelf.height.min(height.saturating_sub(shelf.y))) as usize;
                let (_, tail) = std::mem::take(&mut rest).split_at_mut(start.saturating_sub(at).min(rest.len()));
                let take = (rows * width as usize).min(tail.len());
                let (band, tail) = tail.split_at_mut(take);
                rest = tail;
                at = start + take;
                bands.push((band, shelf));
            }
            crate::pool::run(|| {
                bands
                    .into_par_iter()
                    .for_each(|(band, shelf)| fill_shelf(band, shelf, width, lightmaps, &images, places));
            });
        }
        let (texture, oom) = crate::memory::guarded(&self.device, || {
            self.device.create_texture(&wgpu::TextureDescriptor {
                label: Some("hp-render compiled lightmaps"),
                size: wgpu::Extent3d { width, height, depth_or_array_layers: layers },
                mip_level_count: 1,
                sample_count: 1,
                dimension: wgpu::TextureDimension::D2,
                format: ATLAS_FORMAT,
                usage: wgpu::TextureUsages::TEXTURE_BINDING | wgpu::TextureUsages::COPY_DST,
                view_formats: &[],
            })
        });
        if oom {
            return CompiledLightStats { lightmaps: 0, dropped: stats.lightmaps + stats.dropped, ..stats };
        }
        let row_bytes = width as usize * TEXEL_BYTES as usize;
        let band = ((UPLOAD_BAND_BYTES / row_bytes.max(1)).max(1) as u32).min(height);
        let all: &[u8] = bytemuck::cast_slice(&data);
        for layer in 0..layers {
            let mut y = 0;
            while y < height {
                let rows = band.min(height - y);
                let start = (layer as usize * height as usize + y as usize) * row_bytes;
                let Some(chunk) = all.get(start..start + rows as usize * row_bytes) else { break };
                self.queue.write_texture(
                    wgpu::TexelCopyTextureInfo {
                        texture: &texture,
                        mip_level: 0,
                        origin: wgpu::Origin3d { x: 0, y, z: layer },
                        aspect: wgpu::TextureAspect::All,
                    },
                    chunk,
                    wgpu::TexelCopyBufferLayout {
                        offset: 0,
                        bytes_per_row: Some(width * TEXEL_BYTES),
                        rows_per_image: Some(rows),
                    },
                    wgpu::Extent3d { width, height: rows, depth_or_array_layers: 1 },
                );
                self.queue.submit([]);
                y += rows;
            }
        }
        drop(data);
        let atlas_bytes = u64::from(width) * u64::from(height) * u64::from(layers) * u64::from(TEXEL_BYTES);
        let inv = [1.0 / width as f32, 1.0 / height as f32];
        let style_of: Vec<(u32, f32, u32)> = lightmaps
            .iter()
            .map(|l| {
                let bump = if l.bumped { LAYER_BUMPED } else { 0 };
                (pack_styles(l.styles).0, (l.width + 2 * PAD) as f32 * inv[0], bump)
            })
            .collect();
        let vertex_size = size_of::<VertexGpu>() as u64;
        let limit = (self.max_buffer / vertex_size).min(u64::from(u32::MAX / 2)) as usize;
        let index_limit = (self.max_buffer / 4).min(u64::from(u32::MAX / 2)) as usize;
        let slots: Vec<u32> = blocks.iter().map(|b| self.scene.acquire_ref(b.object)).collect();
        let materials: Vec<Vec<(MaterialId, u32)>> = blocks
            .iter()
            .map(|b| b.batches.iter().map(|x| (x.material, self.scene.material_slot(x.material))).collect())
            .collect();
        let atlas = AtlasRef { places, style_of: &style_of, inv, limit };
        let built: Vec<Option<BuiltBlock>> = crate::pool::run(|| {
            blocks
                .par_iter()
                .zip(slots.par_iter())
                .zip(materials.par_iter())
                .map(|((block, &slot), mats)| build_block(block, slot, mats, &atlas))
                .collect()
        });
        let mut gpu_blocks = Vec::with_capacity(blocks.len());
        let mut segments: Vec<Segment> = Vec::new();
        let (mut seg_v, mut seg_i): (Vec<VertexGpu>, Vec<u32>) = (Vec::new(), Vec::new());
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
        let flush = |seg_v: &mut Vec<VertexGpu>, seg_i: &mut Vec<u32>, segments: &mut Vec<Segment>| {
            if seg_v.is_empty() {
                return;
            }
            let vdata: &[u8] = bytemuck::cast_slice(seg_v);
            let idata: &[u8] = bytemuck::cast_slice(seg_i);
            segments.push(Segment {
                vbuf: make("hp-render compiled vertices", vdata, wgpu::BufferUsages::VERTEX),
                ibuf: make("hp-render compiled indices", idata, wgpu::BufferUsages::INDEX),
            });
            seg_v.clear();
            seg_i.clear();
        };
        for (block, b) in blocks.iter().zip(built) {
            let Some(mut b) = b else {
                self.scene.release_ref(block.object);
                continue;
            };
            if seg_v.len() + b.verts.len() > limit || seg_i.len() + b.indices.len() > index_limit {
                flush(&mut seg_v, &mut seg_i, &mut segments);
            }
            let first_index = seg_i.len() as u32;
            for d in &mut b.draws {
                d.first += first_index;
                for f in &mut d.faces {
                    f.0 += first_index;
                }
            }
            let bytes = (b.verts.len() as u64) * vertex_size + (b.indices.len() as u64) * 4;
            gpu_blocks.push(BlockGpu {
                object: block.object,
                moved: false,
                segment: segments.len() as u32,
                base_vertex: seg_v.len() as i32,
                draws: b.draws,
                lo: b.lo,
                hi: b.hi,
                bytes,
            });
            seg_v.append(&mut b.verts);
            seg_i.append(&mut b.indices);
        }
        flush(&mut seg_v, &mut seg_i, &mut segments);
        drop((seg_v, seg_i));
        let mut order: Vec<(u32, u32, u32)> = Vec::new();
        for (k, b) in gpu_blocks.iter().enumerate() {
            for (d, draw) in b.draws.iter().enumerate() {
                order.push((draw.material, k as u32, d as u32));
            }
        }
        order.sort_unstable_by_key(|&(material, block, draw)| {
            (material, gpu_blocks[block as usize].segment, block, draw)
        });
        self.sync_materials();
        let params = ParamsGpu {
            mode: match mode {
                CompiledLightMode::Fullbright => 0,
                CompiledLightMode::Ldr => 1,
                CompiledLightMode::Hdr => 2,
            },
            scale: 1.0,
            pad: [0.0; 2],
        };
        let pbuf = self.device.create_buffer(&wgpu::BufferDescriptor {
            label: Some("hp-render compiled light params"),
            size: size_of::<ParamsGpu>() as u64,
            usage: wgpu::BufferUsages::UNIFORM | wgpu::BufferUsages::COPY_DST,
            mapped_at_creation: false,
        });
        self.queue.write_buffer(&pbuf, 0, bytemuck::bytes_of(&params));
        let view = texture.create_view(&wgpu::TextureViewDescriptor {
            dimension: Some(wgpu::TextureViewDimension::D2Array),
            ..Default::default()
        });
        let cl = &mut self.compiled;
        let bg = self.device.create_bind_group(&wgpu::BindGroupDescriptor {
            label: Some("hp-render compiled light"),
            layout: cl.bind_layout(&self.device),
            entries: &[
                wgpu::BindGroupEntry { binding: 0, resource: wgpu::BindingResource::TextureView(&view) },
                wgpu::BindGroupEntry { binding: 1, resource: pbuf.as_entire_binding() },
            ],
        });
        stats.blocks = gpu_blocks.len() as u32;
        let transforms = vec![IDENTITY; gpu_blocks.len().max(1)];
        let tbuf = self.device.create_buffer(&wgpu::BufferDescriptor {
            label: Some("hp-render compiled transforms"),
            size: (transforms.len() * 64) as u64,
            usage: wgpu::BufferUsages::VERTEX | wgpu::BufferUsages::COPY_DST,
            mapped_at_creation: false,
        });
        self.queue.write_buffer(&tbuf, 0, bytemuck::cast_slice(&transforms));
        let cl = &mut self.compiled;
        cl.transforms = transforms;
        cl.transform_buf = Some(tbuf);
        stats.bytes = atlas_bytes + gpu_blocks.iter().map(|b| b.bytes).sum::<u64>();
        cl.blocks = gpu_blocks;
        cl.segments = segments;
        cl.order = order;
        cl.blocks_gen += 1;
        cl.visible_for = None;
        cl.atlas = Some((texture, width, layers));
        cl.params = Some(pbuf);
        cl.bind_group = Some(bg);
        cl.mode = mode;
        cl.present = true;
        cl.stats = stats;
        self.content_gen += 1;
        stats
    }

    pub fn clear_compiled_lighting(&mut self) {
        let cl = std::mem::take(&mut self.compiled.blocks);
        for b in &cl {
            self.scene.release_ref(b.object);
        }
        let had = self.compiled.present;
        let c = &mut self.compiled;
        c.segments.clear();
        c.order.clear();
        c.blocks_gen += 1;
        c.states.clear();
        c.visible.clear();
        c.visible_for = None;
        c.transforms.clear();
        c.transform_buf = None;
        c.atlas = None;
        c.params = None;
        c.bind_group = None;
        c.present = false;
        c.mode = CompiledLightMode::Fullbright;
        c.stats = CompiledLightStats::default();
        if had {
            self.content_gen += 1;
        }
    }

    pub fn set_compiled_transform(&mut self, object: ObjectId, transform: Option<[[f32; 4]; 4]>) {
        let m = transform.filter(|m| m.iter().flatten().all(|v| v.is_finite())).unwrap_or(IDENTITY);
        let cl = &mut self.compiled;
        let mut changed = false;
        for (i, b) in cl.blocks.iter_mut().enumerate() {
            if b.object != object || cl.transforms.get(i) == Some(&m) {
                continue;
            }
            cl.transforms[i] = m;
            b.moved = m != IDENTITY;
            if let Some(buf) = &cl.transform_buf {
                self.queue.write_buffer(buf, (i * 64) as u64, bytemuck::cast_slice(&m));
            }
            changed = true;
        }
        if changed {
            self.content_gen += 1;
        }
    }

    pub(crate) fn prepare_compiled_frame(&mut self, view_proj: &Mat4) {
        let cl = &mut self.compiled;
        if cl.blocks.is_empty() {
            return;
        }
        let key = (self.scene.state_gen, cl.blocks_gen);
        if cl.states_key != key || cl.states.len() != cl.blocks.len() {
            let scene = &self.scene;
            cl.states = cl
                .blocks
                .iter()
                .map(|b| {
                    let flags = scene.flags(b.object).unwrap_or_default();
                    let alpha = scene.tint(b.object).map_or(1.0, |t| (t >> 24) as f32 / 255.0);
                    (flags, alpha)
                })
                .collect();
            cl.states_key = key;
        }
        let vp = view_proj.to_cols_array();
        cl.visible = cl.blocks.iter().map(|b| b.moved || box_visible(view_proj, b.lo, b.hi)).collect();
        cl.visible_for = Some(vp);
    }

    pub fn compiled_lighting(&self) -> Option<CompiledLightStats> {
        self.compiled.present.then_some(self.compiled.stats)
    }

    pub fn set_light_styles(&mut self, values: &[f32]) {
        let clean: Vec<f32> = (0..LIGHT_STYLES)
            .map(|i| values.get(i).copied().filter(|v| v.is_finite()).map_or(1.0, |v| v.clamp(0.0, 64.0)))
            .collect();
        let current = &self.compiled.style_values;
        let same = if current.is_empty() { clean.iter().all(|v| *v == 1.0) } else { *current == clean };
        if same {
            return;
        }
        self.queue.write_buffer(&self.light_styles, 0, bytemuck::cast_slice(&clean));
        self.compiled.style_values = clean;
        self.content_gen += 1;
    }

    pub fn light_styles(&self) -> Vec<f32> {
        if self.compiled.style_values.is_empty() { vec![1.0; LIGHT_STYLES] } else { self.compiled.style_values.clone() }
    }

    pub fn set_compiled_objects(&mut self, ids: ahash::AHashSet<ObjectId>) {
        let changed: Vec<ObjectId> = self.compiled_objects.symmetric_difference(&ids).copied().collect();
        self.compiled_objects = ids;
        for id in changed {
            let Some(old) = self.scene.flags(id) else { continue };
            let flags = if self.compiled_objects.contains(&id) {
                old | crate::ObjectFlags::COMPILED_LIGHT
            } else {
                crate::ObjectFlags(old.0 & !crate::ObjectFlags::COMPILED_LIGHT.0)
            };
            if flags != old {
                self.scene.set_flags(id, flags);
                self.content_gen += 1;
            }
            if let Some(base) = self.play_base.get_mut(&id) {
                base.0 =
                    (base.0 & !crate::ObjectFlags::COMPILED_LIGHT.0) | (flags.0 & crate::ObjectFlags::COMPILED_LIGHT.0);
            }
        }
    }
}

#[allow(clippy::too_many_arguments)]
fn opaque_pipeline(
    device: &wgpu::Device,
    module: &wgpu::ShaderModule,
    layout: &wgpu::PipelineLayout,
    samples: u32,
    alpha_test: bool,
    two_sided: bool,
    blend: Option<bool>,
    tint_opacity: bool,
) -> wgpu::RenderPipeline {
    let mode = match blend {
        None => 0.0,
        Some(false) => 1.0,
        Some(true) => 2.0,
    };
    let constants = [
        ("ALPHA_TEST", if alpha_test { 1.0 } else { 0.0 }),
        ("BLEND_MODE", mode),
        ("TINT_OPACITY", if tint_opacity { 1.0 } else { 0.0 }),
    ];
    let options = wgpu::PipelineCompilationOptions { constants: &constants, zero_initialize_workgroup_memory: false };
    let buffers = [
        Some(wgpu::VertexBufferLayout {
            array_stride: size_of::<VertexGpu>() as u64,
            step_mode: wgpu::VertexStepMode::Vertex,
            attributes: &VERTEX_ATTRS,
        }),
        Some(wgpu::VertexBufferLayout {
            array_stride: 64,
            step_mode: wgpu::VertexStepMode::Instance,
            attributes: &TRANSFORM_ATTRS,
        }),
    ];
    let blend_state = match blend {
        None => None,
        Some(false) => Some(wgpu::BlendState {
            color: wgpu::BlendComponent {
                src_factor: wgpu::BlendFactor::SrcAlpha,
                dst_factor: wgpu::BlendFactor::OneMinusSrcAlpha,
                operation: wgpu::BlendOperation::Add,
            },
            alpha: wgpu::BlendComponent {
                src_factor: wgpu::BlendFactor::One,
                dst_factor: wgpu::BlendFactor::OneMinusSrcAlpha,
                operation: wgpu::BlendOperation::Add,
            },
        }),
        Some(true) => Some(wgpu::BlendState {
            color: wgpu::BlendComponent {
                src_factor: wgpu::BlendFactor::One,
                dst_factor: wgpu::BlendFactor::One,
                operation: wgpu::BlendOperation::Add,
            },
            alpha: wgpu::BlendComponent {
                src_factor: wgpu::BlendFactor::Zero,
                dst_factor: wgpu::BlendFactor::One,
                operation: wgpu::BlendOperation::Add,
            },
        }),
    };
    let bias = if blend.is_none() {
        wgpu::DepthBiasState { constant: 4, slope_scale: 0.5, clamp: 0.0 }
    } else {
        wgpu::DepthBiasState::default()
    };
    device.create_render_pipeline(&wgpu::RenderPipelineDescriptor {
        label: Some("hp-render compiled light"),
        layout: Some(layout),
        vertex: wgpu::VertexState {
            module,
            entry_point: Some("vs_compiled"),
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
            bias,
        }),
        multisample: wgpu::MultisampleState { count: samples, mask: !0, alpha_to_coverage_enabled: false },
        fragment: Some(wgpu::FragmentState {
            module,
            entry_point: Some("fs_compiled"),
            compilation_options: options,
            targets: &[Some(wgpu::ColorTargetState {
                format: COLOR_FORMAT,
                blend: blend_state,
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

pub(crate) fn textured_mode(mode: crate::RenderMode) -> bool {
    use crate::RenderMode;
    matches!(mode, RenderMode::Textured | RenderMode::TexturedWire | RenderMode::Lit | RenderMode::Unshaded)
}

pub(crate) fn drawn_in(cx: &HookContext<'_>) -> bool {
    !cx.is_ortho && cx.options.compiled_lighting && cx.renderer.compiled.present && textured_mode(cx.params.mode)
}

const RECORD_VEC4: usize = 28;
const RECORD_LIGHTS: usize = 4;
const MAX_COLORS: usize = 1 << 24;

#[derive(Default)]
pub(crate) struct ModelLights {
    pub(crate) records: Option<wgpu::Buffer>,
    pub(crate) colors: Option<wgpu::Buffer>,
    of: ahash::AHashMap<ObjectId, (u32, Option<ModelId>, u32)>,
}

impl ModelLights {
    pub(crate) fn record_of(&self, object: ObjectId, model: ModelId) -> u32 {
        match self.of.get(&object) {
            Some(&(colored, Some(m), _)) if m == model => colored,
            Some(&(_, _, plain)) => plain,
            None => u32::MAX,
        }
    }
}

fn record(l: &ModelLighting, offset: usize, count: usize) -> [[f32; 4]; RECORD_VEC4] {
    let mut r = [[0.0f32; 4]; RECORD_VEC4];
    let clean = |v: f32| if v.is_finite() { v } else { 0.0 };
    for (o, c) in r.iter_mut().zip(&l.ambient) {
        *o = [clean(c[0]).max(0.0), clean(c[1]).max(0.0), clean(c[2]).max(0.0), 0.0];
    }
    let list = if count > 0 { &l.styled_lights } else { &l.lights };
    let lights: Vec<&ModelLocalLight> = list.iter().take(RECORD_LIGHTS).collect();
    r[6] = [offset as f32, count as f32, lights.len() as f32, 0.0];
    if let Some(origin) = l.sample_origin.filter(|origin| origin.iter().all(|v| v.is_finite())) {
        r[7] = [origin[0], origin[1], origin[2], 1.0];
    }
    for (k, li) in lights.iter().enumerate() {
        let at = 8 + k * 5;
        let v = |a: [f32; 3], w: f32| [clean(a[0]), clean(a[1]), clean(a[2]), clean(w)];
        r[at] = v(li.origin, li.kind.min(2) as f32);
        r[at + 1] = v(li.dir, li.radius.max(0.0));
        r[at + 2] = v(li.color.map(|c| c.max(0.0)), li.style.min(LIGHT_STYLES as u32 - 1) as f32);
        r[at + 3] = v(li.attenuation, 0.0);
        r[at + 4] = v(li.cone, 0.0);
    }
    r
}

impl Renderer {
    pub fn set_model_lighting(&mut self, lights: &[ModelLighting]) {
        let mut records: Vec<[[f32; 4]; RECORD_VEC4]> = Vec::with_capacity(lights.len());
        let mut colors: Vec<u32> = Vec::new();
        let mut of = ahash::AHashMap::with_capacity(lights.len());
        let limit = (self.max_storage / (RECORD_VEC4 * 16) as u64) as usize;
        for l in lights {
            if records.len() + 2 > limit {
                break;
            }
            let plain = records.len() as u32;
            records.push(record(l, 0, 0));
            let mut colored = plain;
            let n = l.vertex_colors.len();
            if n > 0 && l.model.is_some() && colors.len() + n <= MAX_COLORS {
                colored = records.len() as u32;
                records.push(record(l, colors.len(), n));
                colors.extend(l.vertex_colors.iter().map(|c| u32::from_le_bytes(*c)));
            }
            of.insert(l.object, (colored, l.model, plain));
        }
        let make = |label: &str, data: &[u8]| {
            let buf = self.device.create_buffer(&wgpu::BufferDescriptor {
                label: Some(label),
                size: (data.len() as u64).next_multiple_of(wgpu::COPY_BUFFER_ALIGNMENT).max(16),
                usage: wgpu::BufferUsages::STORAGE | wgpu::BufferUsages::COPY_DST,
                mapped_at_creation: false,
            });
            self.queue.write_buffer(&buf, 0, data);
            buf
        };
        let changed = !(records.is_empty() && self.model_lights.records.is_none());
        if records.is_empty() {
            self.model_lights.records = None;
            self.model_lights.colors = None;
        } else {
            self.model_lights.records = Some(make("hp-render model lights", bytemuck::cast_slice(&records)));
            self.model_lights.colors =
                (!colors.is_empty()).then(|| make("hp-render model light colors", bytemuck::cast_slice(&colors)));
        }
        self.model_lights.of = of;
        if changed {
            self.scene_gen += 1;
            self.pack_instances();
        }
    }
}

fn shader_and_layout<'a>(r: &'a Renderer, device: &wgpu::Device) -> (&'a wgpu::ShaderModule, &'a wgpu::PipelineLayout) {
    let cl = &r.compiled;
    let module = cl.module.get_or_init(|| {
        device.create_shader_module(wgpu::ShaderModuleDescriptor {
            label: Some("hp-render compiled light"),
            source: wgpu::ShaderSource::Wgsl(SHADER.into()),
        })
    });
    let layout = cl.pipeline_layout.get_or_init(|| {
        device.create_pipeline_layout(&wgpu::PipelineLayoutDescriptor {
            label: Some("hp-render compiled light"),
            bind_group_layouts: &[Some(&r.shared.view_bgl), Some(&r.shared.material_bgl), Some(cl.bind_layout(device))],
            immediate_size: 0,
        })
    });
    (module, layout)
}

pub(crate) mod sky3d;

#[derive(Default)]
pub(crate) struct CompiledLightPass;

impl CompiledLightPass {
    fn draw(cx: &HookContext<'_>, pass: &mut wgpu::RenderPass<'_>, translucent: bool) {
        if !drawn_in(cx) {
            return;
        }
        let r = cx.renderer;
        let cl = &r.compiled;
        let (Some(bind_group), Some(transforms)) = (&cl.bind_group, &cl.transform_buf) else { return };
        let device = cx.device();
        let (module, layout) = shader_and_layout(r, device);
        let si = usize::from(cx.samples > 1);
        let pipes = cl.pipelines[si].get_or_init(|| {
            let make =
                |alpha, two, blend, tint| opaque_pipeline(device, module, layout, cx.samples, alpha, two, blend, tint);
            let opaque = [false, true].map(|alpha| [false, true].map(|two| make(alpha, two, None, false)));
            let blended = [false, true].map(|add| [false, true].map(|two| make(false, two, Some(add), false)));
            let faded = [false, true].map(|alpha| [false, true].map(|two| make(alpha, two, Some(false), true)));
            Pipelines { opaque, blended, faded }
        });
        let view_proj = Mat4::from_cols_array_2d(&cx.view_proj);
        let eye = Vec3::from(cx.eye);
        let water_pass = crate::water::applies(cx.params, cx.options, cx.is_ortho);
        let by_water = |desc: Option<&crate::MaterialDesc>| water_pass && desc.is_some_and(|d| d.water.is_some());
        pass.set_bind_group(0, cx.view_bind_group, &[]);
        pass.set_bind_group(2, bind_group, &[]);
        pass.set_vertex_buffer(1, transforms.slice(..));
        type Key = (usize, usize, usize);
        let set = |key: Key| match key.0 {
            0 => &pipes.opaque[key.1][key.2],
            1 => &pipes.blended[key.1][key.2],
            _ => &pipes.faded[key.1][key.2],
        };
        if !translucent {
            let framed = cl.framed(&view_proj);
            let visible: Vec<bool> = cl
                .blocks
                .iter()
                .enumerate()
                .map(|(i, b)| {
                    let (flags, alpha) = cl.state(r, i, b);
                    let hidden = flags.contains(crate::ObjectFlags::NO_3D)
                        || (flags.contains(crate::ObjectFlags::HIDDEN) && !r.probing);
                    let seen = match framed {
                        Some(v) => v.get(i).copied().unwrap_or(false),
                        None => b.moved || box_visible(&view_proj, b.lo, b.hi),
                    };
                    !hidden && alpha >= 0.999 && seen
                })
                .collect();
            let mut keys: Vec<Option<Option<Key>>> = vec![None; r.scene.materials.len()];
            let mut current: Option<Key> = None;
            let mut material = u32::MAX;
            let mut segment = u32::MAX;
            for &(mat, index, draw) in &cl.order {
                if !visible.get(index as usize).copied().unwrap_or(false) {
                    continue;
                }
                let Some(slot) = keys.get_mut(mat as usize) else { continue };
                let key = *slot.get_or_insert_with(|| {
                    let desc = r.scene.materials.get(mat as usize).and_then(|m| m.desc);
                    if by_water(desc.as_ref()) {
                        return None;
                    }
                    match classify_with(desc.as_ref(), cx.options.force_opaque, false)? {
                        (MeshPass::Opaque, two_sided) => Some((0, 0, usize::from(two_sided))),
                        (MeshPass::AlphaTest, two_sided) => Some((0, 1, usize::from(two_sided))),
                        _ => None,
                    }
                });
                let Some(key) = key else { continue };
                let Some(mg) = r.mat_gpu.get(mat as usize) else { continue };
                let block = &cl.blocks[index as usize];
                let Some(d) = block.draws.get(draw as usize) else { continue };
                if current != Some(key) {
                    pass.set_pipeline(set(key));
                    current = Some(key);
                }
                if material != mat {
                    pass.set_bind_group(1, &mg.bind_group, &[]);
                    material = mat;
                }
                if segment != block.segment {
                    cl.bind_block(pass, block);
                    segment = block.segment;
                }
                pass.draw_indexed(d.first..d.first + d.count, block.base_vertex, index..index + 1);
            }
            return;
        }
        let framed = cl.framed(&view_proj);
        let order = cl
            .blocks
            .iter()
            .enumerate()
            .filter(|(i, b)| match framed {
                Some(v) => v.get(*i).copied().unwrap_or(false),
                None => b.moved || box_visible(&view_proj, b.lo, b.hi),
            })
            .map(|(i, b)| (i as u32, b));
        let mut faces: Vec<(f32, u32, &BlockGpu, u32, u32, u32, Key)> = Vec::new();
        for (index, block) in order {
            let (flags, alpha) = cl.state(r, index as usize, block);
            let hidden =
                flags.contains(crate::ObjectFlags::NO_3D) || (flags.contains(crate::ObjectFlags::HIDDEN) && !r.probing);
            if !flags.contains(crate::ObjectFlags::COMPILED_LIGHT) || hidden {
                continue;
            }
            if alpha <= 0.0 {
                continue;
            }
            let faded = alpha < 0.999;
            let m = cl.transforms.get(index as usize).map_or(Mat4::IDENTITY, Mat4::from_cols_array_2d);
            for d in &block.draws {
                let desc = r.scene.materials.get(d.material as usize).and_then(|m| m.desc);
                if by_water(desc.as_ref()) {
                    continue;
                }
                let Some((kind, two_sided)) = classify_with(desc.as_ref(), cx.options.force_opaque, false) else {
                    continue;
                };
                let key = match kind {
                    MeshPass::Translucent => (1, 0, usize::from(two_sided)),
                    MeshPass::Additive => (1, 1, usize::from(two_sided)),
                    MeshPass::Opaque if faded => (2, 0, usize::from(two_sided)),
                    MeshPass::AlphaTest if faded => (2, 1, usize::from(two_sided)),
                    _ => continue,
                };
                for &(first, count, center) in &d.faces {
                    let dist = (m.transform_point3(center) - eye).length_squared();
                    faces.push((dist, index, block, d.material, first, count, key));
                }
            }
        }
        faces.sort_by(|a, b| b.0.total_cmp(&a.0));
        let mut current: Option<Key> = None;
        let mut material = u32::MAX;
        let mut bound: Option<u32> = None;
        for (_, index, block, mat, first, count, key) in faces {
            let Some(mg) = r.mat_gpu.get(mat as usize) else { continue };
            if current != Some(key) {
                pass.set_pipeline(set(key));
                current = Some(key);
            }
            if material != mat {
                pass.set_bind_group(1, &mg.bind_group, &[]);
                material = mat;
            }
            if bound != Some(block.segment) {
                cl.bind_block(pass, block);
                bound = Some(block.segment);
            }
            pass.draw_indexed(first..first + count, block.base_vertex, index..index + 1);
        }
    }
}

impl PassHook for CompiledLightPass {
    fn name(&self) -> &'static str {
        "compiled_light"
    }

    fn draw_opaque(&self, cx: &HookContext<'_>, pass: &mut wgpu::RenderPass<'_>) {
        Self::draw(cx, pass, false);
    }

    fn draw_translucent(&self, cx: &HookContext<'_>, pass: &mut wgpu::RenderPass<'_>) {
        Self::draw(cx, pass, true);
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn layouts() {
        assert_eq!(size_of::<VertexGpu>(), 68);
        assert_eq!(std::mem::offset_of!(VertexGpu, styles), 40);
        assert_eq!(std::mem::offset_of!(VertexGpu, stride), 44);
        assert_eq!(size_of::<ParamsGpu>(), 16);
    }

    #[test]
    fn light_styles_pack_per_byte() {
        assert_eq!(pack_styles(&[]), (STATIC_STYLES, 1));
        assert_eq!(pack_styles(&[0, 10]), (u32::from_le_bytes([0, 10, 255, 255]), 2));
        assert_eq!(pack_styles(&[0, 32, 33, 34, 35]), (u32::from_le_bytes([0, 32, 33, 34]), 4), "four at most");
        let across = 2 * (3 + 2 * PAD) - 2 * PAD;
        let packing = pack_atlas(&[(across, 2)], 16, 16, 1);
        let (_, x, _) = packing.places[0].expect("placed");
        assert!(x + across + PAD <= 16);
    }

    #[test]
    fn half_floats() {
        assert_eq!(f16_bits(0.0), 0);
        assert_eq!(f16_bits(1.0), 0x3c00);
        assert_eq!(f16_bits(2.0), 0x4000);
        assert_eq!(f16_bits(0.5), 0x3800);
        assert_eq!(f16_bits(-1.0), 0xbc00);
        assert_eq!(f16_bits(65504.0), 0x7bff);
        assert_eq!(f16_bits(1.0e6), 0x7c00);
        assert_eq!(f16_bits(1.0 + 1.0 / 2048.0), 0x3c00);
        assert_eq!(f16_bits(1.0 + 3.0 / 2048.0), 0x3c02);
        assert_eq!(f16_bits(2f32.powi(-24)), 1);
        assert_eq!(f16_bits(1.0e-9), 0);
        assert_eq!(f16_bits(f32::NAN) & 0x7c00, 0x7c00);
    }

    #[test]
    fn atlas_packing_keeps_lightmaps_apart() {
        let sizes = [(4, 4), (10, 2), (3, 7), (1, 1), (0, 0), (40, 40), (17, 5)];
        let packing = pack_atlas(&sizes, 32, 32, 8);
        let (places, layers) = (&packing.places, packing.layers);
        assert!(places[4].is_none(), "empty lightmap");
        assert!(places[5].is_none(), "larger than a layer");
        let mut rects = Vec::new();
        for (i, p) in places.iter().enumerate() {
            if i == 4 || i == 5 {
                continue;
            }
            let (layer, x, y) = p.expect("placed");
            let (w, h) = sizes[i];
            assert!(layer < layers && x >= PAD && y >= PAD && x + w + PAD <= 32 && y + h + PAD <= 32);
            assert!(y + h + PAD <= packing.height, "inside the used height");
            rects.push((layer, x - PAD, y - PAD, w + 2 * PAD, h + 2 * PAD));
        }
        for (a, ra) in rects.iter().enumerate() {
            for rb in &rects[a + 1..] {
                let apart = ra.0 != rb.0
                    || ra.1 + ra.3 <= rb.1
                    || rb.1 + rb.3 <= ra.1
                    || ra.2 + ra.4 <= rb.2
                    || rb.2 + rb.4 <= ra.2;
                assert!(apart, "{ra:?} {rb:?}");
            }
        }
        let placed: usize = packing.shelves.iter().map(|s| s.members.len()).sum();
        assert_eq!(placed, places.iter().filter(|p| p.is_some()).count(), "every placed lightmap is on a shelf");
        for shelf in &packing.shelves {
            for &i in &shelf.members {
                let (layer, _, y) = places[i].expect("placed");
                assert!(layer == shelf.layer && y - PAD >= shelf.y && y + sizes[i].1 + PAD <= shelf.y + shelf.height);
            }
        }
        let many = vec![(14, 14); 9];
        let p = pack_atlas(&many, 32, 32, 2);
        assert_eq!(p.layers, 2);
        assert_eq!(p.places.iter().filter(|x| x.is_some()).count(), 8, "4 per layer");
    }

    #[test]
    fn the_atlas_is_only_as_large_as_its_lightmaps() {
        let (p, width) = atlas_layout(&[(4, 4); 100], 4096, 64);
        assert_eq!(p.layers, 1);
        assert!(width <= 80 && p.height <= 80, "{width} x {}", p.height);
        let (p, width) = atlas_layout(&[(600, 10)], 4096, 64);
        assert_eq!((width, p.height), (608, 12));
        let (p, width) = atlas_layout(&[(30, 30); 7000], 2048, 64);
        assert_eq!(width, 2048);
        assert!(p.layers >= 2 && p.places.iter().all(Option::is_some));
        let sizes: Vec<(u32, u32)> = (0..5000u32).map(|i| (1 + i % 31, 1 + (i * 7) % 29)).collect();
        let (p, width) = atlas_layout(&sizes, 4096, 64);
        let area: u64 = sizes.iter().map(|&(w, h)| u64::from(w + 2) * u64::from(h + 2)).sum();
        let used = u64::from(width) * u64::from(p.height) * u64::from(p.layers);
        assert!(used < area * 3 / 2, "packed {area} texels into {used}");
    }

    #[test]
    fn shared_exponent_texels_keep_the_lightmap_values() {
        assert_eq!(rgb9e5([0.0; 3]), 0);
        assert_eq!(rgb9e5([f32::NAN, -1.0, f32::NEG_INFINITY]), 0);
        for c in [[1.0, 0.5, 0.25], [0.003, 0.002, 0.001], [100.0, 3.0, 0.0], [65000.0, 1.0, 9.0], [1.0e-7, 0.0, 0.0]] {
            let back = rgb9e5_value(rgb9e5(c));
            let top = c.iter().copied().fold(0.0f32, f32::max);
            for (a, b) in c.iter().zip(back) {
                assert!((a - b).abs() <= top / 256.0 + 1.0e-7, "{c:?} -> {back:?}");
            }
        }
        let back = rgb9e5_value(rgb9e5([1.0e9, 1.0, 1.0]));
        assert!((back[0] - 65408.0).abs() < 1.0, "the brightest value is clamped like a half float: {back:?}");
        for e in -8..8 {
            let v = 2f32.powi(e) * 0.75;
            let back = rgb9e5_value(rgb9e5([v, v, v]));
            assert!((back[0] - v).abs() <= v / 512.0, "{v} -> {back:?}");
        }
    }
}
