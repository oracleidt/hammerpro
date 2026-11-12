#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub(crate) struct Usage {
    pub peak_working_set: u64,
    pub peak_committed: u64,
    pub working_set: u64,
    pub committed: u64,
}

impl Usage {
    pub fn peak_mb(self) -> u64 {
        self.peak_working_set >> 20
    }
}

#[cfg(windows)]
fn of_handle(handle: windows_sys::Win32::Foundation::HANDLE) -> Option<Usage> {
    use windows_sys::Win32::System::ProcessStatus::{K32GetProcessMemoryInfo, PROCESS_MEMORY_COUNTERS};
    let mut c = PROCESS_MEMORY_COUNTERS {
        cb: size_of::<PROCESS_MEMORY_COUNTERS>() as u32,
        PageFaultCount: 0,
        PeakWorkingSetSize: 0,
        WorkingSetSize: 0,
        QuotaPeakPagedPoolUsage: 0,
        QuotaPagedPoolUsage: 0,
        QuotaPeakNonPagedPoolUsage: 0,
        QuotaNonPagedPoolUsage: 0,
        PagefileUsage: 0,
        PeakPagefileUsage: 0,
    };
    let ok = unsafe { K32GetProcessMemoryInfo(handle, &raw mut c, c.cb) };
    (ok != 0).then_some(Usage {
        peak_working_set: c.PeakWorkingSetSize as u64,
        peak_committed: c.PeakPagefileUsage as u64,
        working_set: c.WorkingSetSize as u64,
        committed: c.PagefileUsage as u64,
    })
}

#[cfg(windows)]
pub(crate) fn of_child(child: &std::process::Child) -> Option<Usage> {
    use std::os::windows::io::AsRawHandle;
    of_handle(child.as_raw_handle())
}

#[cfg(not(windows))]
pub(crate) fn of_child(_child: &std::process::Child) -> Option<Usage> {
    None
}

#[cfg(windows)]
pub(crate) fn set_child_priority(child: &std::process::Child, priority: hp_app::settings::BakePriority) {
    use hp_app::settings::BakePriority;
    use std::os::windows::io::AsRawHandle;
    use windows_sys::Win32::System::Threading::{
        BELOW_NORMAL_PRIORITY_CLASS, IDLE_PRIORITY_CLASS, NORMAL_PRIORITY_CLASS, SetPriorityClass,
    };
    let class = match priority {
        BakePriority::Idle => IDLE_PRIORITY_CLASS,
        BakePriority::BelowNormal => BELOW_NORMAL_PRIORITY_CLASS,
        BakePriority::Normal => NORMAL_PRIORITY_CLASS,
    };
    unsafe {
        SetPriorityClass(child.as_raw_handle(), class);
    }
}

#[cfg(not(windows))]
pub(crate) fn set_child_priority(_child: &std::process::Child, _priority: hp_app::settings::BakePriority) {}

#[cfg(windows)]
pub(crate) fn lower_thread_priority() {
    use windows_sys::Win32::System::Threading::{GetCurrentThread, SetThreadPriority, THREAD_PRIORITY_BELOW_NORMAL};
    unsafe {
        SetThreadPriority(GetCurrentThread(), THREAD_PRIORITY_BELOW_NORMAL);
    }
    lower_thread_memory_priority();
}

#[cfg(not(windows))]
pub(crate) fn lower_thread_priority() {}

#[cfg(windows)]
pub(crate) fn of_self() -> Option<Usage> {
    of_handle(unsafe { windows_sys::Win32::System::Threading::GetCurrentProcess() })
}

#[cfg(not(windows))]
pub(crate) fn of_self() -> Option<Usage> {
    None
}

#[cfg(windows)]
pub(crate) fn lower_process_memory_priority() {
    use windows_sys::Win32::System::Threading::{
        GetCurrentProcess, MEMORY_PRIORITY_INFORMATION, MEMORY_PRIORITY_VERY_LOW, ProcessMemoryPriority,
        SetProcessInformation,
    };
    let info = MEMORY_PRIORITY_INFORMATION { MemoryPriority: MEMORY_PRIORITY_VERY_LOW };
    unsafe {
        SetProcessInformation(
            GetCurrentProcess(),
            ProcessMemoryPriority,
            (&raw const info).cast(),
            size_of::<MEMORY_PRIORITY_INFORMATION>() as u32,
        );
    }
}

#[cfg(not(windows))]
pub(crate) fn lower_process_memory_priority() {}

#[cfg(windows)]
pub(crate) fn lower_thread_memory_priority() {
    use windows_sys::Win32::System::Threading::{
        GetCurrentThread, MEMORY_PRIORITY_INFORMATION, MEMORY_PRIORITY_VERY_LOW, SetThreadInformation,
        ThreadMemoryPriority,
    };
    let info = MEMORY_PRIORITY_INFORMATION { MemoryPriority: MEMORY_PRIORITY_VERY_LOW };
    unsafe {
        SetThreadInformation(
            GetCurrentThread(),
            ThreadMemoryPriority,
            (&raw const info).cast(),
            size_of::<MEMORY_PRIORITY_INFORMATION>() as u32,
        );
    }
}

#[cfg(not(windows))]
pub(crate) fn lower_thread_memory_priority() {}
