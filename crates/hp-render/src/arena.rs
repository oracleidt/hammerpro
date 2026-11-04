#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(crate) struct Range32 {
    pub offset: u32,
    pub len: u32,
}

impl Range32 {
    pub fn end(self) -> u32 {
        self.offset + self.len
    }
}

#[derive(Clone, Debug, Default)]
pub(crate) struct RangeAlloc {
    free: Vec<Range32>,
    end: u32,
    limit: u32,
}

impl RangeAlloc {
    pub fn new(limit: u32) -> Self {
        Self { free: Vec::new(), end: 0, limit }
    }

    pub fn end(&self) -> u32 {
        self.end
    }

    pub fn limit(&self) -> u32 {
        self.limit
    }

    pub fn alloc(&mut self, len: u32) -> Option<Range32> {
        if len == 0 {
            return Some(Range32 { offset: 0, len: 0 });
        }
        if let Some(i) = self.free.iter().position(|b| b.len >= len) {
            let block = self.free[i];
            let r = Range32 { offset: block.offset, len };
            if block.len == len {
                self.free.remove(i);
            } else {
                self.free[i] = Range32 { offset: block.offset + len, len: block.len - len };
            }
            return Some(r);
        }
        let new_end = self.end.checked_add(len)?;
        if new_end > self.limit {
            return None;
        }
        let r = Range32 { offset: self.end, len };
        self.end = new_end;
        Some(r)
    }

    pub fn free(&mut self, r: Range32) {
        if r.len == 0 {
            return;
        }
        let i = self.free.partition_point(|b| b.offset < r.offset);
        self.free.insert(i, r);
        if i + 1 < self.free.len() && self.free[i].end() == self.free[i + 1].offset {
            self.free[i].len += self.free[i + 1].len;
            self.free.remove(i + 1);
        }
        if i > 0 && self.free[i - 1].end() == self.free[i].offset {
            self.free[i - 1].len += self.free[i].len;
            self.free.remove(i);
        }
        if let Some(last) = self.free.last().copied()
            && last.end() == self.end
        {
            self.end = last.offset;
            self.free.pop();
        }
    }

    #[cfg(test)]
    pub fn used(&self) -> u32 {
        self.end - self.free.iter().map(|b| b.len).sum::<u32>()
    }
}

const GROW_DOUBLE_BELOW: u64 = 16 << 20;

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(crate) enum Grow {
    Fits,
    Grown,
    TooLarge,
}

pub(crate) struct GrowBuffer {
    pub label: &'static str,
    pub usage: wgpu::BufferUsages,
    pub buffer: wgpu::Buffer,
    pub size: u64,
}

impl GrowBuffer {
    pub fn new(device: &wgpu::Device, label: &'static str, usage: wgpu::BufferUsages, size: u64) -> Self {
        let size = size.max(16).next_multiple_of(wgpu::COPY_BUFFER_ALIGNMENT);
        let usage = usage | wgpu::BufferUsages::COPY_DST | wgpu::BufferUsages::COPY_SRC;
        let buffer = device.create_buffer(&wgpu::BufferDescriptor {
            label: Some(label),
            size,
            usage,
            mapped_at_creation: false,
        });
        Self { label, usage, buffer, size }
    }

    pub fn ensure(&mut self, device: &wgpu::Device, queue: &wgpu::Queue, needed: u64, max: u64, keep: bool) -> Grow {
        if needed <= self.size {
            return Grow::Fits;
        }
        let max = max - max % wgpu::COPY_BUFFER_ALIGNMENT;
        if needed > max {
            return Grow::TooLarge;
        }
        let step = if self.size < GROW_DOUBLE_BELOW { self.size.saturating_mul(2) } else { self.size + self.size / 4 };
        let new_size =
            step.max(needed.saturating_add(needed / 8)).min(max).next_multiple_of(wgpu::COPY_BUFFER_ALIGNMENT);
        let make = |size: u64| {
            crate::memory::guarded(device, || {
                device.create_buffer(&wgpu::BufferDescriptor {
                    label: Some(self.label),
                    size,
                    usage: self.usage,
                    mapped_at_creation: false,
                })
            })
        };
        let (mut buffer, mut oom) = make(new_size);
        let mut new_size = new_size;
        if oom && new_size > needed {
            let exact = needed.next_multiple_of(wgpu::COPY_BUFFER_ALIGNMENT);
            (buffer, oom) = make(exact);
            new_size = exact;
        }
        if oom {
            log::warn!(
                "hp-render: {} cannot grow to {} MB: the graphics card is out of memory",
                self.label,
                needed >> 20
            );
            return Grow::TooLarge;
        }
        if keep {
            let mut enc =
                device.create_command_encoder(&wgpu::CommandEncoderDescriptor { label: Some("hp-render grow copy") });
            enc.copy_buffer_to_buffer(&self.buffer, 0, &buffer, 0, self.size);
            queue.submit([enc.finish()]);
        }
        self.buffer = buffer;
        self.size = new_size;
        Grow::Grown
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn alloc_free_coalesce() {
        let mut a = RangeAlloc::new(1000);
        let r1 = a.alloc(10).unwrap();
        let r2 = a.alloc(20).unwrap();
        let r3 = a.alloc(30).unwrap();
        assert_eq!((r1.offset, r2.offset, r3.offset), (0, 10, 30));
        assert_eq!(a.end(), 60);
        a.free(r2);
        let r4 = a.alloc(5).unwrap();
        assert_eq!(r4.offset, 10);
        a.free(r1);
        a.free(r4);
        let r5 = a.alloc(30).unwrap();
        assert_eq!(r5.offset, 0);
        a.free(r5);
        a.free(r3);
        assert_eq!(a.end(), 0);
        assert_eq!(a.used(), 0);
    }

    #[test]
    fn limit_is_enforced() {
        let mut a = RangeAlloc::new(100);
        assert!(a.alloc(60).is_some());
        assert!(a.alloc(50).is_none());
        assert!(a.alloc(40).is_some());
        assert!(a.alloc(1).is_none());
        assert_eq!(a.alloc(0).map(|r| r.len), Some(0));
        assert_eq!(a.limit(), 100);
    }

    #[test]
    fn random_stress_keeps_invariants() {
        let mut a = RangeAlloc::new(1 << 20);
        let mut live: Vec<Range32> = Vec::new();
        let mut seed = 12345u32;
        let mut rnd = || {
            seed ^= seed << 13;
            seed ^= seed >> 17;
            seed ^= seed << 5;
            seed
        };
        for _ in 0..5000 {
            if live.is_empty() || rnd() % 3 != 0 {
                let len = rnd() % 100 + 1;
                if let Some(r) = a.alloc(len) {
                    for o in &live {
                        assert!(r.end() <= o.offset || o.end() <= r.offset, "overlap");
                    }
                    live.push(r);
                }
            } else {
                let i = (rnd() as usize) % live.len();
                a.free(live.swap_remove(i));
            }
        }
        let total: u32 = live.iter().map(|r| r.len).sum();
        assert_eq!(a.used(), total);
        for r in live.drain(..) {
            a.free(r);
        }
        assert_eq!(a.end(), 0);
    }
}
