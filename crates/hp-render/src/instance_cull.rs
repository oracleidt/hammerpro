use std::sync::atomic::{AtomicU64, Ordering};
use std::sync::{Arc, Mutex};

use glam::{Vec3, Vec4};

use crate::arena::GrowBuffer;
use crate::camera::Camera;
use crate::camera::aabb_inside;
#[cfg(test)]
use crate::frame_cache::next_serial;
use crate::frame_cache::{PackCache, PackKey};
use crate::gpu::InstanceGpu;
use crate::{Group, Renderer};

pub(crate) const MIN_INSTANCES: usize = 128;
const POOL_PASSES: u64 = 10;
const ALIGN: u64 = 256;
const PAD: f32 = 4.0;
const PAD_FRACTION: f32 = 0.05;

pub(crate) struct InstancePool {
    pub enabled: bool,
    pub buffer: GrowBuffer,
    cursor: AtomicU64,
    scratch: Mutex<Vec<InstanceGpu>>,
}

impl InstancePool {
    pub fn new(device: &wgpu::Device) -> Self {
        Self {
            enabled: true,
            buffer: GrowBuffer::new(device, "hp-render visible instances", wgpu::BufferUsages::VERTEX, 1 << 12),
            cursor: AtomicU64::new(0),
            scratch: Mutex::new(Vec::new()),
        }
    }

    pub fn reset(&self) {
        self.cursor.store(0, Ordering::Relaxed);
    }
}

pub(crate) struct Packed {
    pub buffer: wgpu::Buffer,
    pub offset: u64,
    pub len: u64,
    pub groups: Arc<[Group]>,
}

impl Renderer {
    pub fn set_instance_culling(&mut self, on: bool) {
        self.instance_pool.enabled = on;
    }

    pub(crate) fn reserve_instance_pool(&mut self) {
        self.instance_pool.reset();
        let count = self.instance_data.len();
        if count == 0 {
            return;
        }
        let want = (count as u64 * size_of::<InstanceGpu>() as u64 + ALIGN) * POOL_PASSES;
        let max = self.max_buffer;
        let _ = self.instance_pool.buffer.ensure(&self.device, &self.queue, want, max, false);
    }

    pub(crate) fn instance_source<'a>(&'a self, packed: Option<&'a Packed>) -> (wgpu::BufferSlice<'a>, &'a [Group]) {
        match packed {
            Some(p) => (p.buffer.slice(p.offset..p.offset + p.len), &p.groups[..]),
            None => (self.instances.buffer.slice(..), &self.instance_groups),
        }
    }

    pub(crate) fn pack_visible(&self, planes: &[Vec4]) -> Option<Packed> {
        self.pack_visible_with(planes, None)
    }

    pub(crate) fn pack_view_cached(&mut self, view_id: u32, cam: &Camera, model_lods: bool) -> Option<Packed> {
        let key = PackKey {
            stamp: self.scene_stamp(),
            view_proj: cam.view_proj.to_cols_array(),
            up: cam.up.to_array(),
            height: cam.height,
            lods: model_lods,
            enabled: self.instance_pool.enabled,
        };
        let cached = self.views.get(&view_id).and_then(|v| v.packed.as_ref()).filter(|c| c.key == key);
        if let Some(c) = cached.filter(|_| self.frame_caching) {
            return c.packed();
        }
        let lods = (!crate::profile::baseline("lods")).then_some((cam, model_lods));
        let planes = &cam.planes[..cam.plane_count];
        let mut out = self.instance_pool.scratch.lock().ok()?;
        out.clear();
        let groups = self.cull_instances(planes, lods, &mut out);
        let previous = self.views.get_mut(&view_id).and_then(|v| v.packed.take()).and_then(|c| c.buffer);
        let cache = match groups {
            None => PackCache {
                key,
                #[cfg(test)]
                serial: next_serial(),
                buffer: previous,
                len: 0,
                groups: Arc::from(Vec::new()),
                skipped: true,
            },
            Some(groups) => {
                let bytes: &[u8] = bytemuck::cast_slice(&out);
                let len = (bytes.len() as u64).max(4);
                let size = len.next_multiple_of(ALIGN);
                let buffer = match previous {
                    Some(b) if b.size() >= size && b.size() / 4 <= size => b,
                    _ => self.device.create_buffer(&wgpu::BufferDescriptor {
                        label: Some("hp-render view instances"),
                        size: size.next_power_of_two().max(ALIGN),
                        usage: wgpu::BufferUsages::VERTEX | wgpu::BufferUsages::COPY_DST,
                        mapped_at_creation: false,
                    }),
                };
                if !bytes.is_empty() {
                    self.queue.write_buffer(&buffer, 0, bytes);
                }
                PackCache {
                    key,
                    #[cfg(test)]
                    serial: next_serial(),
                    buffer: Some(buffer),
                    len,
                    groups: Arc::from(groups),
                    skipped: false,
                }
            }
        };
        drop(out);
        let packed = cache.packed();
        if let Some(v) = self.views.get_mut(&view_id) {
            v.packed = Some(cache);
        }
        packed
    }

    fn pack_visible_with(&self, planes: &[Vec4], lods: Option<(&Camera, bool)>) -> Option<Packed> {
        let mut out = self.instance_pool.scratch.lock().ok()?;
        out.clear();
        let groups = self.cull_instances(planes, lods, &mut out)?;
        let bytes: &[u8] = bytemuck::cast_slice(&out);
        let len = bytes.len() as u64;
        let offset = self.instance_pool.cursor.fetch_add(len.next_multiple_of(ALIGN).max(ALIGN), Ordering::Relaxed);
        if offset + len.max(4) > self.instance_pool.buffer.size {
            return None;
        }
        if len > 0 {
            self.queue.write_buffer(&self.instance_pool.buffer.buffer, offset, bytes);
        }
        Some(Packed {
            buffer: self.instance_pool.buffer.buffer.clone(),
            offset,
            len: len.max(4),
            groups: Arc::from(groups),
        })
    }

    fn cull_instances(
        &self,
        planes: &[Vec4],
        lods: Option<(&Camera, bool)>,
        out: &mut Vec<InstanceGpu>,
    ) -> Option<Vec<Group>> {
        let total = self.instance_data.len();
        let cull = total >= MIN_INSTANCES
            && self.instance_boxes.len() == total
            && self.instance_pool.enabled
            && !crate::profile::baseline("instances");
        let choose_lods = lods.is_some() && self.models.values().any(|m| !m.lod_parts.is_empty());
        if total == 0 || (!cull && !choose_lods) {
            return None;
        }
        let mut groups: Vec<Group> = Vec::with_capacity(self.instance_groups.len().min(1024));
        for g in &self.instance_groups {
            let lod_model = choose_lods.then(|| self.models.get(&g.key)).flatten().filter(|model| {
                model.lod_ranges.iter().all(|(lo, hi)| hi > lo)
                    && model.parts.iter().chain(&model.lod_parts).all(|part| {
                        let desc = self.scene.materials.get(part.material as usize).and_then(|m| m.desc);
                        crate::classify(desc.as_ref()).is_some_and(|(pass, _)| {
                            matches!(pass, crate::MeshPass::Opaque | crate::MeshPass::AlphaTest)
                        })
                    })
            });
            let mesh_end = g.start + g.count - g.hull.min(g.count);
            for i in g.start..g.start + g.count {
                let Some(data) = self.instance_data.get(i as usize) else { continue };
                if cull && let Some((lo, hi)) = self.instance_boxes.get(i as usize) {
                    let pad = Vec3::splat(PAD) + (*hi - *lo) * PAD_FRACTION;
                    if !aabb_inside(planes, *lo - pad, *hi + pad) {
                        continue;
                    }
                }
                let start = out.len() as u32;
                let hull = u32::from(i >= mesh_end);
                let lod = if data.tint[3] >= 0.999 {
                    lods.and_then(|(cam, on)| {
                        let model = lod_model?;
                        if !on {
                            return Some(0);
                        }
                        choose_lod(cam, data, &model.lod_ranges)
                    })
                } else {
                    None
                };
                out.push(*data);
                match groups.last_mut() {
                    Some(last) if last.key == g.key && last.lod == lod && (last.hull == 0 || hull != 0) => {
                        last.count += 1;
                        last.hull += hull;
                    }
                    _ => groups.push(Group { key: g.key, start, count: 1, hull, lod }),
                }
            }
        }
        if out.len() >= total && !choose_lods {
            return None;
        }
        Some(groups)
    }
}

impl PackCache {
    pub fn packed(&self) -> Option<Packed> {
        if self.skipped {
            return None;
        }
        Some(Packed { buffer: self.buffer.clone()?, offset: 0, len: self.len, groups: self.groups.clone() })
    }
}

fn choose_lod(cam: &Camera, instance: &InstanceGpu, ranges: &[(f32, f32)]) -> Option<u32> {
    let origin = Vec3::from_slice(&instance.m3);
    let up = cam.up * 0.5;
    let a = cam.view_proj * (origin + up).extend(1.0);
    let b = cam.view_proj * (origin - up).extend(1.0);
    let y = |p: Vec4| if p.w >= 0.001 { p.y / p.w } else { p.y * 1000.0 };
    let width = cam.height * (y(b) - y(a)).abs();
    let metric = if width != 0.0 { 100.0 / width } else { 0.0 };
    let margin = metric.abs().max(1.0) * 1e-4;
    ranges
        .iter()
        .enumerate()
        .find_map(|(i, &(lo, hi))| (metric > lo + margin && metric < hi - margin).then_some(i as u32))
}
