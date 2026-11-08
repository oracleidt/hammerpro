use hp_app::runtime::primitives::{PrimitiveMemory, ResourceProbes};

pub(crate) fn measure(probes: &mut ResourceProbes) -> Result<PrimitiveMemory, u32> {
    probes.sort();
    platform::measure(probes)
}

#[cfg(not(windows))]
mod platform {
    use super::*;

    pub(super) fn measure(_probes: &ResourceProbes) -> Result<PrimitiveMemory, u32> {
        Err(50)
    }
}

#[cfg(windows)]
mod platform {
    use windows_sys::Win32::Foundation::{ERROR_NO_MORE_ITEMS, GetLastError, HANDLE};
    use windows_sys::Win32::System::Memory::{GetProcessHeap, HeapLock, HeapUnlock, HeapWalk, PROCESS_HEAP_ENTRY};
    use windows_sys::Win32::System::SystemServices::PROCESS_HEAP_ENTRY_BUSY;

    use super::*;

    struct LockedHeap(HANDLE);

    impl Drop for LockedHeap {
        fn drop(&mut self) {
            unsafe { HeapUnlock(self.0) };
        }
    }

    pub(super) fn measure(probes: &ResourceProbes) -> Result<PrimitiveMemory, u32> {
        let heap = unsafe { GetProcessHeap() };
        if heap.is_null() || unsafe { HeapLock(heap) } == 0 {
            return Err(unsafe { GetLastError() });
        }
        let locked = LockedHeap(heap);
        let mut entry = PROCESS_HEAP_ENTRY::default();
        let mut memory = PrimitiveMemory { total_probes: probes.points.len(), ..Default::default() };
        let error = loop {
            if memory.matched_probes == memory.total_probes {
                break ERROR_NO_MORE_ITEMS;
            }
            if unsafe { HeapWalk(heap, &mut entry) } == 0 {
                break unsafe { GetLastError() };
            }
            if entry.wFlags & PROCESS_HEAP_ENTRY_BUSY as u16 != 0 {
                memory.record_block(&probes.points, entry.lpData as usize, u64::from(entry.cbData), entry.cbOverhead);
            }
        };
        drop(locked);
        if error == ERROR_NO_MORE_ITEMS { Ok(memory) } else { Err(error) }
    }
}

#[cfg(all(test, windows))]
mod tests {
    use hp_app::runtime::primitives::PrimitiveClass;

    use super::*;

    #[test]
    fn small_heap_allocations_are_measured_individually_without_unrelated_buffers() {
        let buffers: Vec<Vec<u8>> = (0..600).map(|index| vec![7; [16, 48, 256][index % 3]]).collect();
        let unrelated = vec![31u8; 2 << 20];
        let mut probes = ResourceProbes::default();
        for buffer in &buffers {
            probes.vec(buffer, PrimitiveClass::World);
        }
        let memory = measure(&mut probes).unwrap();
        assert_eq!(memory.matched_probes, 600);
        assert_eq!(memory.blocks[0], 600);
        let payload = buffers.iter().map(Vec::len).sum::<usize>() as u64;
        assert!(memory.bytes[0] >= payload && memory.bytes[0] < payload + 600 * 128);
        assert_eq!(unrelated[0], 31);
        assert_eq!(memory.bytes[1..], [0, 0, 0]);
    }

    #[test]
    fn live_windows_heap_measurement_resolves_buffers_and_shared_arc_headers() {
        let world = vec![17u8; 48_000];
        let props = std::sync::Arc::new(vec![31u8; 24_000]);
        let mut probes = ResourceProbes::default();
        probes.vec(&world, PrimitiveClass::World);
        probes.hold(&props, PrimitiveClass::Props);
        probes.vec(&props, PrimitiveClass::Props);
        probes.hold(&props, PrimitiveClass::Lighting);
        let memory = measure(&mut probes).expect("Windows heap enumeration");
        assert_eq!(memory.matched_probes, memory.total_probes);
        assert!(memory.bytes[0] >= 48_000 && memory.bytes[0] < 49_000);
        assert!(memory.bytes[1] >= 24_000 && memory.bytes[1] < 25_000);
        assert!(memory.bytes[3] > 0);
        assert_eq!(memory.bytes[2], 0);
    }
}
