use std::sync::Mutex;
use std::sync::atomic::{AtomicU64, Ordering};

use glam::{Vec3, Vec4};

use crate::arena::GrowBuffer;
use crate::camera::aabb_inside;
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
    pub offset: u64,
    pub len: u64,
    pub groups: Vec<Group>,
}

impl Renderer {
    pub fn set_instance_culling(&mut self, on: bool) {
        self.instance_pool.enabled = on;
    }

    pub(crate) fn reserve_instance_pool(&mut self) {
        self.instance_pool.reset();
        let count = self.instance_data.len();
        if count < MIN_INSTANCES {
            return;
        }
        let want = (count as u64 * size_of::<InstanceGpu>() as u64 + ALIGN) * POOL_PASSES;
        let max = self.max_buffer;
        let _ = self.instance_pool.buffer.ensure(&self.device, &self.queue, want, max, false);
    }

    pub(crate) fn instance_source<'a>(&'a self, packed: Option<&'a Packed>) -> (wgpu::BufferSlice<'a>, &'a [Group]) {
        match packed {
            Some(p) => (self.instance_pool.buffer.buffer.slice(p.offset..p.offset + p.len), &p.groups),
            None => (self.instances.buffer.slice(..), &self.instance_groups),
        }
    }

    pub(crate) fn pack_visible(&self, planes: &[Vec4]) -> Option<Packed> {
        let total = self.instance_data.len();
        if total < MIN_INSTANCES
            || self.instance_boxes.len() != total
            || !self.instance_pool.enabled
            || crate::profile::baseline("instances")
        {
            return None;
        }
        let mut out = self.instance_pool.scratch.lock().ok()?;
        out.clear();
        let mut groups: Vec<Group> = Vec::with_capacity(self.instance_groups.len().min(1024));
        for g in &self.instance_groups {
            let start = out.len() as u32;
            let mesh_end = g.start + g.count - g.hull.min(g.count);
            let mut hull = 0u32;
            for i in g.start..g.start + g.count {
                let (Some(data), Some((lo, hi))) =
                    (self.instance_data.get(i as usize), self.instance_boxes.get(i as usize))
                else {
                    continue;
                };
                let pad = Vec3::splat(PAD) + (*hi - *lo) * PAD_FRACTION;
                if !aabb_inside(planes, *lo - pad, *hi + pad) {
                    continue;
                }
                out.push(*data);
                hull += u32::from(i >= mesh_end);
            }
            let count = out.len() as u32 - start;
            if count > 0 {
                groups.push(Group { key: g.key, start, count, hull });
            }
        }
        if out.len() >= total {
            return None;
        }
        let bytes: &[u8] = bytemuck::cast_slice(&out);
        let len = bytes.len() as u64;
        let offset = self.instance_pool.cursor.fetch_add(len.next_multiple_of(ALIGN).max(ALIGN), Ordering::Relaxed);
        if offset + len.max(4) > self.instance_pool.buffer.size {
            return None;
        }
        if len > 0 {
            self.queue.write_buffer(&self.instance_pool.buffer.buffer, offset, bytes);
        }
        Some(Packed { offset, len: len.max(4), groups })
    }
}
