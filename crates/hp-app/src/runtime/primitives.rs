use std::any::Any;
use std::sync::Arc;

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
#[repr(u8)]
pub enum PrimitiveClass {
    World = 1,
    Props = 2,
    Lighting = 4,
    Shared = 8,
}

impl PrimitiveClass {
    pub fn index(self) -> usize {
        match self {
            Self::World => 0,
            Self::Props => 1,
            Self::Lighting => 2,
            Self::Shared => 3,
        }
    }
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct AllocationProbe {
    pub address: usize,
    pub class: PrimitiveClass,
}

#[derive(Default)]
pub struct ResourceProbes {
    pub points: Vec<AllocationProbe>,
    keep_alive: Vec<Arc<dyn Any + Send + Sync>>,
}

impl ResourceProbes {
    pub fn point(&mut self, address: usize, class: PrimitiveClass) {
        if address > 0 {
            self.points.push(AllocationProbe { address, class });
        }
    }

    pub fn vec<T>(&mut self, values: &Vec<T>, class: PrimitiveClass) {
        if values.capacity() > 0 && size_of::<T>() > 0 {
            self.point(values.as_ptr() as usize, class);
        }
    }

    pub fn string(&mut self, value: &String, class: PrimitiveClass) {
        if value.capacity() > 0 {
            self.point(value.as_ptr() as usize, class);
        }
    }

    pub fn hold<T: Any + Send + Sync>(&mut self, value: &Arc<T>, class: PrimitiveClass) {
        self.point(Arc::as_ptr(value) as usize, class);
        self.keep_alive.push(value.clone());
    }

    pub fn sort(&mut self) {
        self.points.sort_unstable_by_key(|point| (point.address, point.class as u8));
        self.points.dedup();
    }

    pub fn strings(&mut self, values: &Vec<String>, class: PrimitiveClass) {
        self.vec(values, class);
        for value in values {
            self.string(value, class);
        }
    }

    pub fn pairs(&mut self, values: &Vec<(String, String)>, class: PrimitiveClass) {
        self.vec(values, class);
        for (key, value) in values {
            self.string(key, class);
            self.string(value, class);
        }
    }

    pub fn blocks(&mut self, values: &Vec<hp_kv::Block>, class: PrimitiveClass) {
        self.vec(values, class);
        let mut stack: Vec<&hp_kv::Block> = values.iter().collect();
        while let Some(block) = stack.pop() {
            self.string(&block.name, class);
            self.pairs(&block.props, class);
            self.vec(&block.children, class);
            stack.extend(&block.children);
        }
    }
}

#[derive(Clone, Copy, Debug, Default, PartialEq, Eq, serde::Serialize)]
pub struct PrimitiveMemory {
    pub bytes: [u64; 4],
    pub blocks: [u64; 4],
    pub matched_probes: usize,
    pub total_probes: usize,
}

impl PrimitiveMemory {
    pub fn record_block(&mut self, points: &[AllocationProbe], start: usize, bytes: u64, overhead: u8) {
        let end = start.saturating_add(usize::try_from(bytes).unwrap_or(usize::MAX));
        let first = points.partition_point(|point| point.address < start);
        let last = points.partition_point(|point| point.address < end);
        let mask = points[first..last].iter().fold(0u8, |mask, point| mask | point.class as u8);
        let class = match mask {
            0 => return,
            1 => PrimitiveClass::World,
            2 => PrimitiveClass::Props,
            4 => PrimitiveClass::Lighting,
            _ => PrimitiveClass::Shared,
        };
        let index = class.index();
        self.bytes[index] = self.bytes[index].saturating_add(bytes.saturating_add(u64::from(overhead)));
        self.blocks[index] += 1;
        self.matched_probes += last - first;
    }
}

#[derive(Clone, Debug, Default, PartialEq, Eq, serde::Serialize)]
pub struct PrimitiveStats {
    pub solids: usize,
    pub world_materials: usize,
    pub props: usize,
    pub prop_materials: usize,
    pub models_loading: usize,
    pub models_missing: usize,
    pub instances_loading: usize,
    pub instances_missing: usize,
    pub lights: usize,
    pub memory: Option<PrimitiveMemory>,
    pub memory_error: Option<String>,
    pub revision: u64,
    pub collecting: bool,
    pub measurement_us: u64,
}

#[derive(Debug, Default)]
pub struct PrimitivesRuntime {
    pub stats: Option<PrimitiveStats>,
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn shared_heap_blocks_are_counted_once_with_the_reported_overhead() {
        let points = [
            AllocationProbe { address: 101, class: PrimitiveClass::World },
            AllocationProbe { address: 108, class: PrimitiveClass::Props },
            AllocationProbe { address: 201, class: PrimitiveClass::Lighting },
        ];
        let mut memory = PrimitiveMemory { total_probes: points.len(), ..Default::default() };
        memory.record_block(&points, 100, 32, 8);
        memory.record_block(&points, 200, 64, 16);
        memory.record_block(&points, 300, 4096, 32);
        assert_eq!(memory.bytes, [0, 0, 80, 40]);
        assert_eq!(memory.blocks, [0, 0, 1, 1]);
        assert_eq!(memory.matched_probes, 3);
    }

    #[test]
    fn resource_probes_keep_shared_resources_alive_and_ignore_empty_buffers() {
        let model = Arc::new(vec![4u32, 8, 16]);
        let mut probes = ResourceProbes::default();
        probes.hold(&model, PrimitiveClass::Props);
        probes.vec(&model, PrimitiveClass::Props);
        probes.vec(&Vec::<u8>::new(), PrimitiveClass::World);
        probes.sort();
        assert_eq!(Arc::strong_count(&model), 2);
        assert_eq!(probes.points.len(), 2);
        drop(probes);
        assert_eq!(Arc::strong_count(&model), 1);
    }
}
