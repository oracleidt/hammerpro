use glam::{DVec3, Mat4, Vec3, Vec4};

use crate::Renderer;
use crate::camera::{Camera, frustum_planes, mat_from_rows};
use crate::gpu::{LightGpu, ViewUniform};
use crate::lights::{KIND_POINT, KIND_SPOT};
use crate::pipelines::{DEPTH_FORMAT, ShadowPipelines};

pub(crate) const LOCAL_SHADOW_SIZE: u32 = 512;
pub(crate) const LOCAL_SHADOW_SIZE_MANY: u32 = 256;
pub(crate) const SHADOW_SIZE_LIGHTS: usize = 8;
pub const MAX_SHADOWED_LIGHTS: u32 = 64;
pub(crate) const LAYERS_PER_FRAME: usize = 6;

pub(crate) fn shadow_size(count: usize) -> u32 {
    if count <= SHADOW_SIZE_LIGHTS { LOCAL_SHADOW_SIZE } else { LOCAL_SHADOW_SIZE_MANY }
}
const MAX_SPOT_DEG: f32 = 75.0;
const OWNER_TIMEOUT: u32 = 8;
const NEAR: f32 = 2.0;
const VIEW_MARGIN: f32 = 256.0;

#[derive(Clone, Copy, Debug, PartialEq)]
struct LightKey {
    pos_range: [f32; 4],
    dir: [f32; 4],
    cone: [f32; 4],
    kind: f32,
}

impl LightKey {
    fn of(l: &LightGpu) -> Self {
        Self { pos_range: l.pos_range, dir: l.dir, cone: l.cone, kind: l.color[3] }
    }
}

#[derive(Clone, Debug)]
struct Slot {
    key: LightKey,
    index: usize,
    mats: Vec<Mat4>,
    texel: f32,
    drawn: Vec<Option<u64>>,
}

impl Slot {
    fn ready(&self) -> bool {
        self.drawn.iter().all(Option::is_some)
    }
}

struct LayerGpu {
    uniform: wgpu::Buffer,
    bind_group: Option<(u64, wgpu::BindGroup)>,
}

pub(crate) struct LocalShadows {
    texture: Option<(wgpu::TextureView, Vec<wgpu::TextureView>)>,
    pub dummy: wgpu::TextureView,
    pipelines: Option<ShadowPipelines>,
    layers: Vec<LayerGpu>,
    slots: Vec<Option<Slot>>,
    owner: Option<u32>,
    others: u32,
    lights_gen: u64,
    written: Vec<bool>,
    dirty: bool,
    size: u32,
}

impl LocalShadows {
    pub fn new(device: &wgpu::Device) -> Self {
        let dummy = device
            .create_texture(&wgpu::TextureDescriptor {
                label: Some("hp-render local shadow dummy"),
                size: wgpu::Extent3d { width: 1, height: 1, depth_or_array_layers: 1 },
                mip_level_count: 1,
                sample_count: 1,
                dimension: wgpu::TextureDimension::D2,
                format: DEPTH_FORMAT,
                usage: wgpu::TextureUsages::TEXTURE_BINDING | wgpu::TextureUsages::RENDER_ATTACHMENT,
                view_formats: &[],
            })
            .create_view(&wgpu::TextureViewDescriptor {
                dimension: Some(wgpu::TextureViewDimension::D2Array),
                ..Default::default()
            });
        Self {
            texture: None,
            dummy,
            pipelines: None,
            layers: Vec::new(),
            slots: Vec::new(),
            owner: None,
            others: 0,
            lights_gen: 0,
            written: Vec::new(),
            dirty: false,
            size: LOCAL_SHADOW_SIZE,
        }
    }

    pub fn view(&self) -> &wgpu::TextureView {
        self.texture.as_ref().map_or(&self.dummy, |t| &t.0)
    }

    pub fn bytes(&self) -> u64 {
        self.texture.as_ref().map_or(0, |t| t.1.len() as u64 * u64::from(self.size).pow(2) * 4)
    }

    pub fn ready_count(&self) -> usize {
        self.slots.iter().flatten().filter(|s| s.ready()).count()
    }

    pub fn reset_size(&mut self) {
        self.slots.clear();
        self.texture = None;
        self.layers.clear();
        self.dirty = true;
    }

    pub fn release(&mut self, view_id: u32) {
        if self.owner == Some(view_id) {
            self.owner = None;
        }
    }
}

fn light_maps(l: &LightGpu, size: u32) -> Option<(Vec<Mat4>, f32)> {
    let kind = l.color[3];
    if kind != KIND_POINT && kind != KIND_SPOT {
        return None;
    }
    let pos = Vec3::new(l.pos_range[0], l.pos_range[1], l.pos_range[2]);
    let far = l.pos_range[3].max(NEAR * 4.0);
    let size = size as f32;
    let outer = l.cone[1].clamp(-1.0, 1.0).acos().to_degrees();
    if kind == KIND_SPOT && outer <= MAX_SPOT_DEG {
        let dir = Vec3::new(l.dir[0], l.dir[1], l.dir[2]).normalize_or(Vec3::NEG_Z);
        let half = (outer + 2.0).to_radians().min(85f32.to_radians());
        let up = if dir.z.abs() < 0.99 { Vec3::Z } else { Vec3::Y };
        let m = map_proj(pos, dir, up, half.tan(), far);
        return Some((vec![m], 2.0 * half.tan() / size));
    }
    let tan = 1.0 + 4.0 / size;
    let faces = [
        (Vec3::X, Vec3::Z),
        (Vec3::NEG_X, Vec3::Z),
        (Vec3::Y, Vec3::Z),
        (Vec3::NEG_Y, Vec3::Z),
        (Vec3::Z, Vec3::Y),
        (Vec3::NEG_Z, Vec3::Y),
    ];
    let mats = faces.iter().map(|(d, up)| map_proj(pos, *d, *up, tan, far)).collect();
    Some((mats, 2.0 * tan / size))
}

pub(crate) fn map_proj(eye: Vec3, fwd: Vec3, up_hint: Vec3, tan_half: f32, far: f32) -> Mat4 {
    let (e, f) = (eye.as_dvec3(), fwd.as_dvec3());
    let r = f.cross(up_hint.as_dvec3()).normalize();
    let u = r.cross(f);
    let s = 1.0 / f64::from(tan_half);
    let (n, fa) = (f64::from(NEAR), f64::from(far));
    let a = fa / (fa - n);
    let row = |v: DVec3, w: f64| [v.x, v.y, v.z, w];
    mat_from_rows([
        row(r * s, -r.dot(e) * s),
        row(u * s, -u.dot(e) * s),
        row(f * a, -a * f.dot(e) - a * n),
        row(f, -f.dot(e)),
    ])
}

fn luminance(c: [f32; 4]) -> f32 {
    c[0] * 0.3 + c[1] * 0.59 + c[2] * 0.11
}

fn score(l: &LightGpu, eye: Vec3) -> f32 {
    let pos = Vec3::new(l.pos_range[0], l.pos_range[1], l.pos_range[2]);
    let d = pos.distance(eye).max(64.0);
    let [c, lin, q, cap] = l.atten;
    let dc = d.min(cap.max(1.0));
    let denom = c + dc * (lin + dc * q);
    let falloff = if denom > 1e-12 { 1.0 / denom } else { 1.0 };
    let s = luminance(l.color) * falloff / (1.0 + d / 2048.0);
    if s.is_finite() { s.max(0.0) } else { 0.0 }
}

pub(crate) fn choose(
    lights: &[LightGpu],
    first_local: usize,
    cam: &Camera,
    count: usize,
    kept: &[usize],
) -> Vec<usize> {
    if count == 0 {
        return Vec::new();
    }
    let planes = frustum_planes(&cam.view_proj);
    let mut scored: Vec<(f32, usize)> = lights
        .iter()
        .enumerate()
        .skip(first_local)
        .filter_map(|(i, l)| {
            let pos = Vec3::new(l.pos_range[0], l.pos_range[1], l.pos_range[2]);
            let r = l.pos_range[3];
            let inside = planes[..5].iter().all(|p| p.dot(pos.extend(1.0)) >= -r);
            if !inside || pos.distance(cam.eye) > r + VIEW_MARGIN + cam.far.min(16384.0) {
                return None;
            }
            let mut s = score(l, cam.eye);
            if kept.contains(&i) {
                s *= 1.5;
            }
            (s > 0.0).then_some((s, i))
        })
        .collect();
    scored.sort_by(|a, b| b.0.total_cmp(&a.0).then(a.1.cmp(&b.1)));
    scored.into_iter().take(count).map(|(_, i)| i).collect()
}

impl Renderer {
    pub(crate) fn prepare_local_shadows(&mut self, view_id: u32, cam: &Camera, count: u32) -> Vec<usize> {
        let count = count.min(MAX_SHADOWED_LIGHTS) as usize;
        let ls = &mut self.local_shadows;
        if !self.probing {
            if ls.owner == Some(view_id) {
                ls.others = 0;
            } else {
                ls.others += 1;
                if ls.owner.is_none() || ls.others > OWNER_TIMEOUT {
                    ls.owner = Some(view_id);
                    ls.others = 0;
                }
            }
        }
        let owner = ls.owner == Some(view_id) && !self.probing;
        if ls.lights_gen != self.lights_gen {
            ls.lights_gen = self.lights_gen;
            for slot in ls.slots.iter_mut() {
                let found = slot.as_ref().and_then(|s| {
                    self.lights_cpu
                        .iter()
                        .enumerate()
                        .skip(self.dir_lights as usize)
                        .find(|(_, l)| LightKey::of(l) == s.key)
                });
                match (slot.as_mut(), found) {
                    (Some(s), Some((i, _))) => s.index = i,
                    _ => *slot = None,
                }
            }
            ls.dirty = true;
        }
        if owner {
            if ls.slots.len() != count {
                ls.size = shadow_size(count).min(self.quality.local_shadow_size);
                ls.slots = vec![None; count];
                ls.texture = None;
                ls.layers.clear();
                ls.dirty = true;
                self.scene_gen += 1;
            }
            let kept: Vec<usize> = ls.slots.iter().flatten().map(|s| s.index).collect();
            let chosen = choose(&self.lights_cpu, self.dir_lights as usize, cam, count, &kept);
            for slot in ls.slots.iter_mut() {
                if slot.as_ref().is_some_and(|s| !chosen.contains(&s.index)) {
                    *slot = None;
                    ls.dirty = true;
                }
            }
            for i in chosen {
                if ls.slots.iter().flatten().any(|s| s.index == i) {
                    continue;
                }
                let Some(free) = ls.slots.iter_mut().find(|s| s.is_none()) else { break };
                let l = &self.lights_cpu[i];
                let Some((mats, texel)) = light_maps(l, ls.size) else { continue };
                let drawn = vec![None; mats.len()];
                *free = Some(Slot { key: LightKey::of(l), index: i, mats, texel, drawn });
            }
        }
        let ls = &mut self.local_shadows;
        if count > 0 && ls.slots.len() == count && ls.texture.is_none() {
            let layers = (count * 6) as u32;
            let texture = self.device.create_texture(&wgpu::TextureDescriptor {
                label: Some("hp-render local shadow maps"),
                size: wgpu::Extent3d { width: ls.size, height: ls.size, depth_or_array_layers: layers },
                mip_level_count: 1,
                sample_count: 1,
                dimension: wgpu::TextureDimension::D2,
                format: DEPTH_FORMAT,
                usage: wgpu::TextureUsages::RENDER_ATTACHMENT | wgpu::TextureUsages::TEXTURE_BINDING,
                view_formats: &[],
            });
            let sampled = texture.create_view(&wgpu::TextureViewDescriptor {
                dimension: Some(wgpu::TextureViewDimension::D2Array),
                ..Default::default()
            });
            let per_layer = (0..layers)
                .map(|k| {
                    texture.create_view(&wgpu::TextureViewDescriptor {
                        dimension: Some(wgpu::TextureViewDimension::D2),
                        base_array_layer: k,
                        array_layer_count: Some(1),
                        ..Default::default()
                    })
                })
                .collect();
            ls.texture = Some((sampled, per_layer));
            for s in ls.slots.iter_mut().flatten() {
                s.drawn.iter_mut().for_each(|d| *d = None);
            }
            self.scene_gen += 1;
            ls.dirty = true;
        }
        let epoch = self.shadow_epoch;
        let mut todo: Vec<(bool, usize)> = Vec::new();
        for (k, slot) in ls.slots.iter().enumerate() {
            let Some(s) = slot else { continue };
            for (f, d) in s.drawn.iter().enumerate() {
                if *d != Some(epoch) {
                    todo.push((d.is_some(), k * 6 + f));
                }
            }
        }
        todo.sort_by_key(|t| t.0);
        let layers: Vec<usize> = todo.into_iter().take(LAYERS_PER_FRAME).map(|t| t.1).collect();
        self.write_local_shadow_lights(&layers);
        layers
    }

    fn write_local_shadow_lights(&mut self, drawing: &[usize]) {
        let ls = &mut self.local_shadows;
        let ready: Vec<bool> = ls
            .slots
            .iter()
            .enumerate()
            .map(|(k, s)| {
                s.as_ref().is_some_and(|s| {
                    s.drawn.iter().enumerate().all(|(f, d)| d.is_some() || drawing.contains(&(k * 6 + f)))
                })
            })
            .collect();
        if !ls.dirty && ready == ls.written {
            return;
        }
        let mut out = self.lights_cpu.clone();
        let mut records: Vec<LightGpu> = Vec::new();
        for (k, slot) in ls.slots.iter().enumerate() {
            if let Some(s) = slot.as_ref().filter(|_| !ready[k])
                && let Some(light) = out.get_mut(s.index)
            {
                light.color = [0.0, 0.0, 0.0, light.color[3]];
            }
            let Some(s) = slot.as_ref().filter(|_| ready[k]) else { continue };
            let Some(light) = out.get_mut(s.index) else { continue };
            let first = self.lights_cpu.len() + records.len();
            light.extra[2] = (first + 1) as f32;
            light.extra[3] = s.mats.len() as f32;
            let far = s.key.pos_range[3].max(NEAR * 4.0);
            for (f, m) in s.mats.iter().enumerate() {
                let c = m.to_cols_array_2d();
                records.push(LightGpu {
                    pos_range: c[0],
                    color: c[1],
                    dir: c[2],
                    cone: c[3],
                    atten: [(k * 6 + f) as f32, s.texel, NEAR, far],
                    extra: [0.0; 4],
                });
            }
        }
        out.extend(records);
        let bytes: &[u8] = bytemuck::cast_slice(&out);
        if self.lights_buf.ensure(&self.device, &self.queue, bytes.len() as u64, self.max_storage, false)
            == crate::arena::Grow::Grown
        {
            self.scene_gen += 1;
        }
        if !bytes.is_empty() && bytes.len() as u64 <= self.lights_buf.size {
            self.queue.write_buffer(&self.lights_buf.buffer, 0, bytes);
        }
        let ls = &mut self.local_shadows;
        ls.written = ready;
        ls.dirty = false;
    }

    pub(crate) fn record_local_shadows(&mut self, enc: &mut wgpu::CommandEncoder, layers: &[usize]) -> (u32, u64) {
        if layers.is_empty() || self.local_shadows.texture.is_none() {
            return (0, 0);
        }
        if self.local_shadows.pipelines.is_none() {
            self.local_shadows.pipelines = Some(ShadowPipelines::new(&self.device, &self.shared));
        }
        let scene_gen = self.scene_gen;
        let epoch = self.shadow_epoch;
        for &layer in layers {
            let (k, f) = (layer / 6, layer % 6);
            let Some(Some(slot)) = self.local_shadows.slots.get(k) else { continue };
            let Some(m) = slot.mats.get(f).copied() else { continue };
            while self.local_shadows.layers.len() <= layer {
                let uniform = self.device.create_buffer(&wgpu::BufferDescriptor {
                    label: Some("hp-render local shadow uniform"),
                    size: size_of::<ViewUniform>() as u64,
                    usage: wgpu::BufferUsages::UNIFORM | wgpu::BufferUsages::COPY_DST,
                    mapped_at_creation: false,
                });
                self.local_shadows.layers.push(LayerGpu { uniform, bind_group: None });
            }
            let u = ViewUniform {
                view_proj: m.to_cols_array_2d(),
                eye: [0.0; 4],
                viewport: [1.0; 4],
                opts: [crate::OPT_SHADOW_PASS, 0, 0, 0],
                ..Default::default()
            };
            let lg = &self.local_shadows.layers[layer];
            self.queue.write_buffer(&lg.uniform, 0, bytemuck::bytes_of(&u));
            if lg.bind_group.as_ref().is_none_or(|(g, _)| *g != scene_gen) {
                let bg = self.shadow_pass_bind_group(&lg.uniform, "hp-render local shadow pass");
                self.local_shadows.layers[layer].bind_group = Some((scene_gen, bg));
            }
        }
        let (mut draws, mut tris) = (0u32, 0u64);
        let ls = &self.local_shadows;
        let (Some((_, views)), Some(pipelines)) = (ls.texture.as_ref(), ls.pipelines.as_ref()) else { return (0, 0) };
        let mut done: Vec<usize> = Vec::new();
        for &layer in layers {
            let (k, f) = (layer / 6, layer % 6);
            let Some(Some(slot)) = ls.slots.get(k) else { continue };
            let Some(m) = slot.mats.get(f) else { continue };
            let (Some(target), Some(LayerGpu { bind_group: Some((_, bg)), .. })) =
                (views.get(layer), ls.layers.get(layer))
            else {
                continue;
            };
            let mut pass = enc.begin_render_pass(&wgpu::RenderPassDescriptor {
                label: Some("hp-render local shadow pass"),
                color_attachments: &[],
                depth_stencil_attachment: Some(wgpu::RenderPassDepthStencilAttachment {
                    view: target,
                    depth_ops: Some(wgpu::Operations { load: wgpu::LoadOp::Clear(1.0), store: wgpu::StoreOp::Store }),
                    stencil_ops: None,
                }),
                timestamp_writes: None,
                occlusion_query_set: None,
                multiview_mask: None,
            });
            pass.set_bind_group(0, bg, &[]);
            let planes: [Vec4; 6] = frustum_planes(m);
            let packed = self.pack_visible(&planes);
            let (d, t) = self.draw_shadow_casters(&mut pass, pipelines, &planes, None, packed.as_ref(), false);
            draws += d;
            tris += t;
            done.push(layer);
        }
        for layer in done {
            if let Some(Some(slot)) = self.local_shadows.slots.get_mut(layer / 6)
                && let Some(d) = slot.drawn.get_mut(layer % 6)
            {
                *d = Some(epoch);
            }
        }
        (draws, tris)
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::types::{RenderMode, ViewKind, ViewParams};

    fn cam() -> Camera {
        Camera::new(&ViewParams {
            game_pixel_grid: false,
            size: [640, 480],
            kind: ViewKind::Perspective {
                eye: [0.0; 3],
                forward: [1.0, 0.0, 0.0],
                up: [0.0, 0.0, 1.0],
                fov_y_deg: 90.0,
                near: 1.0,
                far: 10000.0,
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

    fn light(pos: [f32; 3], range: f32, kind: f32, lum: f32) -> LightGpu {
        LightGpu {
            pos_range: [pos[0], pos[1], pos[2], range],
            color: [lum, lum, lum, kind],
            dir: [1.0, 0.0, 0.0, 0.0],
            cone: [0.9, 0.8, 0.0, 0.0],
            atten: [0.0, 0.0, 1.0e-4, 1.0e9],
            extra: [0.0; 4],
        }
    }

    #[test]
    fn the_brightest_lights_in_view_are_chosen() {
        let lights = [
            light([300.0, 0.0, 0.0], 500.0, KIND_POINT, 1.0),
            light([-3000.0, 0.0, 0.0], 500.0, KIND_POINT, 100.0),
            light([2000.0, 0.0, 0.0], 3000.0, KIND_POINT, 500.0),
            light([200.0, 50.0, 0.0], 500.0, KIND_SPOT, 0.2),
        ];
        let c = cam();
        assert_eq!(choose(&lights, 0, &c, 2, &[]), vec![2, 0]);
        assert_eq!(choose(&lights, 0, &c, 8, &[]).len(), 3, "the light behind is out of the view");
        assert!(choose(&lights, 0, &c, 0, &[]).is_empty());
        assert_eq!(choose(&lights, 1, &c, 8, &[]), vec![2, 3]);
        let close =
            [light([300.0, 0.0, 0.0], 500.0, KIND_POINT, 1.0), light([300.0, 1.0, 0.0], 500.0, KIND_POINT, 1.2)];
        assert_eq!(choose(&close, 0, &c, 1, &[0]), vec![0]);
        assert_eq!(choose(&close, 0, &c, 1, &[]), vec![1]);
    }

    #[test]
    fn maps_cover_the_light() {
        let point = light([0.0, 0.0, 100.0], 800.0, KIND_POINT, 1.0);
        let (mats, texel) = light_maps(&point, LOCAL_SHADOW_SIZE).expect("point maps");
        assert_eq!(mats.len(), 6);
        assert!(texel > 0.0 && texel < 0.01);
        for (f, d) in [Vec3::X, Vec3::NEG_X, Vec3::Y, Vec3::NEG_Y, Vec3::Z, Vec3::NEG_Z].iter().enumerate() {
            let p = mats[f] * (Vec3::new(0.0, 0.0, 100.0) + *d * 300.0).extend(1.0);
            let n = p.truncate() / p.w;
            assert!(n.x.abs() < 1e-3 && n.y.abs() < 1e-3 && (0.0..1.0).contains(&n.z), "face {f}: {n:?}");
        }
        let spot = LightGpu { dir: [0.0, 0.0, -1.0, 0.0], ..light([0.0, 0.0, 100.0], 800.0, KIND_SPOT, 1.0) };
        let (mats, _) = light_maps(&spot, LOCAL_SHADOW_SIZE).expect("spot map");
        assert_eq!(mats.len(), 1);
        let p = mats[0] * Vec4::new(0.0, 0.0, 0.0, 1.0);
        assert!((p.x / p.w).abs() < 1e-3 && (p.y / p.w).abs() < 1e-3);
        let wide = LightGpu { cone: [0.5, 0.0, 0.0, 0.0], ..spot };
        assert_eq!(light_maps(&wide, LOCAL_SHADOW_SIZE).map(|m| m.0.len()), Some(6));
        let sun = light([0.0; 3], 1.0, crate::lights::KIND_DIRECTIONAL, 1.0);
        assert!(light_maps(&sun, LOCAL_SHADOW_SIZE).is_none());
        let (_, fine) = light_maps(&point, LOCAL_SHADOW_SIZE).expect("point maps");
        let (_, coarse) = light_maps(&point, LOCAL_SHADOW_SIZE_MANY).expect("point maps");
        assert!(coarse > fine * 1.9, "{coarse} {fine}");
        assert_eq!(shadow_size(SHADOW_SIZE_LIGHTS), LOCAL_SHADOW_SIZE);
        assert_eq!(shadow_size(SHADOW_SIZE_LIGHTS + 1), LOCAL_SHADOW_SIZE_MANY);
    }
}
