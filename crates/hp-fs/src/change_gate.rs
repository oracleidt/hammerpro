use std::path::PathBuf;

pub const MAX_WATCHED_ROOTS: usize = 63;

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum GateWait {
    Changed,
    Woken,
    TimedOut,
    Failed,
}

pub struct WakeEvent {
    #[cfg(windows)]
    handle: windows_sys::Win32::Foundation::HANDLE,
}

unsafe impl Send for WakeEvent {}
unsafe impl Sync for WakeEvent {}

impl std::fmt::Debug for WakeEvent {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("WakeEvent").finish_non_exhaustive()
    }
}

#[cfg(windows)]
fn millis(timeout: Option<std::time::Duration>) -> u32 {
    use windows_sys::Win32::System::Threading::INFINITE;
    timeout.map_or(INFINITE, |t| u32::try_from(t.as_millis()).unwrap_or(INFINITE - 1).min(INFINITE - 1))
}

impl WakeEvent {
    #[cfg(windows)]
    pub fn new() -> Option<Self> {
        use windows_sys::Win32::System::Threading::CreateEventW;
        let handle = unsafe { CreateEventW(std::ptr::null(), 0, 0, std::ptr::null()) };
        (!handle.is_null()).then_some(Self { handle })
    }

    #[cfg(not(windows))]
    pub fn new() -> Option<Self> {
        None
    }

    #[cfg(windows)]
    pub fn set(&self) {
        use windows_sys::Win32::System::Threading::SetEvent;
        unsafe {
            SetEvent(self.handle);
        }
    }

    #[cfg(not(windows))]
    pub fn set(&self) {}

    #[cfg(windows)]
    pub fn wait(&self, timeout: Option<std::time::Duration>) -> GateWait {
        use windows_sys::Win32::Foundation::WAIT_OBJECT_0;
        use windows_sys::Win32::System::Threading::WaitForSingleObject;
        if unsafe { WaitForSingleObject(self.handle, millis(timeout)) } == WAIT_OBJECT_0 {
            GateWait::Woken
        } else {
            GateWait::TimedOut
        }
    }

    #[cfg(not(windows))]
    pub fn wait(&self, timeout: Option<std::time::Duration>) -> GateWait {
        std::thread::sleep(timeout.unwrap_or(std::time::Duration::from_secs(2)));
        GateWait::TimedOut
    }
}

#[cfg(windows)]
impl Drop for WakeEvent {
    fn drop(&mut self) {
        use windows_sys::Win32::Foundation::CloseHandle;
        unsafe {
            CloseHandle(self.handle);
        }
    }
}

pub struct ChangeGate {
    #[cfg(windows)]
    handles: Vec<windows_sys::Win32::Foundation::HANDLE>,
    roots: Vec<PathBuf>,
}

unsafe impl Send for ChangeGate {}

impl std::fmt::Debug for ChangeGate {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("ChangeGate").field("roots", &self.roots).finish_non_exhaustive()
    }
}

impl ChangeGate {
    pub fn watch(roots: &[PathBuf]) -> Option<Self> {
        let mut unique: Vec<PathBuf> = Vec::new();
        for root in roots {
            if !unique.iter().any(|r| root.starts_with(r)) {
                unique.retain(|r| !r.starts_with(root));
                unique.push(root.clone());
            }
        }
        if unique.is_empty() || unique.len() > MAX_WATCHED_ROOTS {
            return None;
        }
        Self::open(unique)
    }

    pub fn roots(&self) -> &[PathBuf] {
        &self.roots
    }

    #[cfg(windows)]
    fn open(roots: Vec<PathBuf>) -> Option<Self> {
        use windows_sys::Win32::Foundation::INVALID_HANDLE_VALUE;
        use windows_sys::Win32::Storage::FileSystem::{
            FILE_NOTIFY_CHANGE_DIR_NAME, FILE_NOTIFY_CHANGE_FILE_NAME, FILE_NOTIFY_CHANGE_LAST_WRITE,
            FILE_NOTIFY_CHANGE_SIZE, FindFirstChangeNotificationW,
        };
        let mut gate = Self { handles: Vec::with_capacity(roots.len()), roots: Vec::new() };
        let filter = FILE_NOTIFY_CHANGE_FILE_NAME
            | FILE_NOTIFY_CHANGE_DIR_NAME
            | FILE_NOTIFY_CHANGE_SIZE
            | FILE_NOTIFY_CHANGE_LAST_WRITE;
        for root in roots {
            let wide = wide(&root);
            let handle = unsafe { FindFirstChangeNotificationW(wide.as_ptr(), 1, filter) };
            if handle.is_null() || handle == INVALID_HANDLE_VALUE {
                return None;
            }
            gate.handles.push(handle);
            gate.roots.push(root);
        }
        Some(gate)
    }

    #[cfg(not(windows))]
    fn open(_roots: Vec<PathBuf>) -> Option<Self> {
        None
    }

    #[cfg(windows)]
    pub fn changed(&mut self) -> bool {
        use windows_sys::Win32::Foundation::WAIT_OBJECT_0;
        use windows_sys::Win32::Storage::FileSystem::FindNextChangeNotification;
        use windows_sys::Win32::System::Threading::WaitForSingleObject;
        let mut changed = false;
        for &handle in &self.handles {
            if unsafe { WaitForSingleObject(handle, 0) } == WAIT_OBJECT_0 {
                changed = true;
                if unsafe { FindNextChangeNotification(handle) } == 0 {
                    return true;
                }
            }
        }
        changed
    }

    #[cfg(not(windows))]
    pub fn changed(&mut self) -> bool {
        true
    }

    #[cfg(windows)]
    pub fn wait(&mut self, wake: &WakeEvent, timeout: Option<std::time::Duration>) -> GateWait {
        use windows_sys::Win32::Foundation::{HANDLE, WAIT_OBJECT_0};
        use windows_sys::Win32::Storage::FileSystem::FindNextChangeNotification;
        use windows_sys::Win32::System::Threading::WaitForMultipleObjects;
        let mut all: Vec<HANDLE> = Vec::with_capacity(self.handles.len() + 1);
        all.push(wake.handle);
        all.extend(&self.handles);
        let count = u32::try_from(all.len()).unwrap_or(u32::MAX);
        let r = unsafe { WaitForMultipleObjects(count, all.as_ptr(), 0, millis(timeout)) };
        if r == windows_sys::Win32::Foundation::WAIT_FAILED {
            return GateWait::Failed;
        }
        let index = r.wrapping_sub(WAIT_OBJECT_0) as usize;
        if index == 0 {
            return GateWait::Woken;
        }
        if index < all.len() {
            for &handle in &self.handles {
                unsafe {
                    FindNextChangeNotification(handle);
                }
            }
            return GateWait::Changed;
        }
        GateWait::TimedOut
    }

    #[cfg(not(windows))]
    pub fn wait(&mut self, wake: &WakeEvent, timeout: Option<std::time::Duration>) -> GateWait {
        let _ = wake.wait(timeout);
        GateWait::Changed
    }
}

#[cfg(windows)]
impl Drop for ChangeGate {
    fn drop(&mut self) {
        use windows_sys::Win32::Storage::FileSystem::FindCloseChangeNotification;
        for &handle in &self.handles {
            unsafe {
                FindCloseChangeNotification(handle);
            }
        }
    }
}

#[cfg(windows)]
fn wide(path: &std::path::Path) -> Vec<u16> {
    use std::os::windows::ffi::OsStrExt;
    path.as_os_str().encode_wide().chain(std::iter::once(0)).collect()
}

#[cfg(all(test, windows))]
mod tests {
    use super::*;
    use crate::test_util::TempDir;

    fn wait_for(gate: &mut ChangeGate) -> bool {
        let deadline = std::time::Instant::now() + std::time::Duration::from_secs(5);
        while std::time::Instant::now() < deadline {
            if gate.changed() {
                return true;
            }
            std::thread::sleep(std::time::Duration::from_millis(20));
        }
        false
    }

    #[test]
    fn edits_anywhere_under_a_root_open_the_gate_once() {
        let tmp = TempDir::new("change-gate");
        tmp.write("materials/a/wall.vmt", b"one");
        let root = tmp.path().to_path_buf();
        let mut gate = ChangeGate::watch(&[root.join("materials"), root.clone()]).unwrap();
        assert_eq!(gate.roots(), std::slice::from_ref(&root), "nested roots collapse into one");
        assert!(!gate.changed(), "nothing changed yet");
        tmp.write("materials/a/wall.vmt", b"two two");
        assert!(wait_for(&mut gate));
        std::thread::sleep(std::time::Duration::from_millis(100));
        while gate.changed() {
            std::thread::sleep(std::time::Duration::from_millis(50));
        }
        assert!(!gate.changed(), "quiet again after the change was seen");
        tmp.write("materials/b/new.vtf", b"x");
        assert!(wait_for(&mut gate), "new files in new folders count too");
    }

    #[test]
    fn the_watcher_sleeps_until_a_change_or_a_wake_up() {
        let tmp = TempDir::new("change-gate-wait");
        tmp.write("materials/a.vmt", b"one");
        let mut gate = ChangeGate::watch(&[tmp.path().to_path_buf()]).unwrap();
        let wake = std::sync::Arc::new(WakeEvent::new().unwrap());
        let short = Some(std::time::Duration::from_millis(30));
        assert_eq!(gate.wait(&wake, short), GateWait::TimedOut);
        let waker = std::sync::Arc::clone(&wake);
        let t = std::thread::spawn(move || {
            std::thread::sleep(std::time::Duration::from_millis(50));
            waker.set();
        });
        assert_eq!(gate.wait(&wake, None), GateWait::Woken, "a new watch list wakes the thread");
        t.join().unwrap();
        tmp.write("materials/a.vmt", b"two two");
        assert_eq!(gate.wait(&wake, Some(std::time::Duration::from_secs(5))), GateWait::Changed);
        assert_eq!(wake.wait(short), GateWait::TimedOut);
    }

    #[test]
    fn missing_or_too_many_roots_fall_back_to_polling() {
        assert!(ChangeGate::watch(&[]).is_none());
        assert!(ChangeGate::watch(&[PathBuf::from("Z:/no/such/folder/for/hammer/pro")]).is_none());
        let many: Vec<PathBuf> = (0..=MAX_WATCHED_ROOTS).map(|i| PathBuf::from(format!("C:/w{i}"))).collect();
        assert!(ChangeGate::watch(&many).is_none());
    }
}
