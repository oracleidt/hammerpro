use std::collections::HashMap;
use std::path::Path;
use std::sync::OnceLock;

use parking_lot::Mutex;

#[derive(Clone, Copy, Debug, Default, PartialEq, Eq, Hash)]
pub enum ScanMode {
    #[default]
    Auto,
    Parallel,
    Sequential,
}

impl ScanMode {
    pub const ALL: [ScanMode; 3] = [ScanMode::Auto, ScanMode::Parallel, ScanMode::Sequential];

    pub fn as_str(self) -> &'static str {
        match self {
            ScanMode::Auto => "auto",
            ScanMode::Parallel => "parallel",
            ScanMode::Sequential => "sequential",
        }
    }

    pub fn parse(text: &str) -> Option<ScanMode> {
        match text.trim().to_ascii_lowercase().as_str() {
            "auto" => Some(ScanMode::Auto),
            "parallel" => Some(ScanMode::Parallel),
            "sequential" => Some(ScanMode::Sequential),
            _ => None,
        }
    }
}

pub trait DriveProbe: Send + Sync {
    fn volume_of(&self, path: &Path) -> Option<String>;

    fn seek_penalty(&self, volume: &str) -> Option<bool>;
}

pub fn decide(mode: ScanMode, seek_penalty: Option<bool>) -> bool {
    match mode {
        ScanMode::Parallel => true,
        ScanMode::Sequential => false,
        ScanMode::Auto => seek_penalty != Some(true),
    }
}

pub fn parallel_with(probe: &dyn DriveProbe, mode: ScanMode, path: &Path) -> bool {
    match mode {
        ScanMode::Parallel => true,
        ScanMode::Sequential => false,
        ScanMode::Auto => {
            let penalty = probe.volume_of(path).and_then(|volume| probe.seek_penalty(&volume));
            decide(mode, penalty)
        }
    }
}

pub struct SystemProbe {
    cache: Mutex<HashMap<String, Option<bool>>>,
}

impl SystemProbe {
    fn new() -> Self {
        Self { cache: Mutex::new(HashMap::new()) }
    }

    pub fn known_volumes(&self) -> Vec<(String, Option<bool>)> {
        let mut list: Vec<(String, Option<bool>)> = self.cache.lock().iter().map(|(k, v)| (k.clone(), *v)).collect();
        list.sort();
        list
    }
}

impl DriveProbe for SystemProbe {
    fn volume_of(&self, path: &Path) -> Option<String> {
        os::volume_of(path)
    }

    fn seek_penalty(&self, volume: &str) -> Option<bool> {
        if let Some(known) = self.cache.lock().get(volume) {
            return *known;
        }
        let answer = os::seek_penalty(volume);
        log::info!(
            "drive {volume}: {}",
            match answer {
                Some(true) => "rotational (seek penalty)",
                Some(false) => "no seek penalty (SSD or NVMe)",
                None => "type unknown, treated as SSD",
            }
        );
        self.cache.lock().insert(volume.to_owned(), answer);
        answer
    }
}

pub fn system_probe() -> &'static SystemProbe {
    static PROBE: OnceLock<SystemProbe> = OnceLock::new();
    PROBE.get_or_init(SystemProbe::new)
}

#[cfg(windows)]
mod os {
    use std::ffi::OsStr;
    use std::os::windows::ffi::OsStrExt;
    use std::path::Path;

    use windows_sys::Win32::Foundation::{CloseHandle, INVALID_HANDLE_VALUE};
    use windows_sys::Win32::Storage::FileSystem::{
        CreateFileW, FILE_SHARE_READ, FILE_SHARE_WRITE, GetVolumeNameForVolumeMountPointW, GetVolumePathNameW,
        OPEN_EXISTING,
    };
    use windows_sys::Win32::System::IO::DeviceIoControl;
    use windows_sys::Win32::System::Ioctl::{
        DEVICE_SEEK_PENALTY_DESCRIPTOR, IOCTL_STORAGE_QUERY_PROPERTY, PropertyStandardQuery, STORAGE_PROPERTY_QUERY,
        StorageDeviceSeekPenaltyProperty,
    };

    fn wide(text: &OsStr) -> Vec<u16> {
        text.encode_wide().chain(std::iter::once(0)).collect()
    }

    fn to_string(buf: &[u16]) -> String {
        let len = buf.iter().position(|c| *c == 0).unwrap_or(buf.len());
        String::from_utf16_lossy(buf.get(..len).unwrap_or_default())
    }

    fn existing_ancestor(path: &Path) -> Option<std::path::PathBuf> {
        let mut probe = path;
        loop {
            if let Ok(found) = std::fs::canonicalize(probe) {
                return Some(found);
            }
            probe = probe.parent()?;
        }
    }

    pub(super) fn volume_of(path: &Path) -> Option<String> {
        let absolute = existing_ancestor(path)?;
        let input = wide(absolute.as_os_str());
        let mut root = vec![0u16; 1024];
        let ok = unsafe { GetVolumePathNameW(input.as_ptr(), root.as_mut_ptr(), root.len() as u32) };
        if ok == 0 {
            return None;
        }
        let root = wide(OsStr::new(&to_string(&root)));
        let mut name = vec![0u16; 64];
        let ok = unsafe { GetVolumeNameForVolumeMountPointW(root.as_ptr(), name.as_mut_ptr(), name.len() as u32) };
        if ok == 0 {
            return None;
        }
        Some(to_string(&name))
    }

    pub(super) fn seek_penalty(volume: &str) -> Option<bool> {
        let device = volume.trim_end_matches('\\');
        if !device.starts_with(r"\\?\Volume{") {
            return None;
        }
        let name = wide(OsStr::new(device));
        let handle = unsafe {
            CreateFileW(
                name.as_ptr(),
                0,
                FILE_SHARE_READ | FILE_SHARE_WRITE,
                std::ptr::null(),
                OPEN_EXISTING,
                0,
                std::ptr::null_mut(),
            )
        };
        if handle == INVALID_HANDLE_VALUE {
            return None;
        }
        let mut query: STORAGE_PROPERTY_QUERY = unsafe { std::mem::zeroed() };
        query.PropertyId = StorageDeviceSeekPenaltyProperty;
        query.QueryType = PropertyStandardQuery;
        let mut out: DEVICE_SEEK_PENALTY_DESCRIPTOR = unsafe { std::mem::zeroed() };
        let mut returned = 0u32;
        let ok = unsafe {
            DeviceIoControl(
                handle,
                IOCTL_STORAGE_QUERY_PROPERTY,
                (&raw const query).cast(),
                std::mem::size_of::<STORAGE_PROPERTY_QUERY>() as u32,
                (&raw mut out).cast(),
                std::mem::size_of::<DEVICE_SEEK_PENALTY_DESCRIPTOR>() as u32,
                &mut returned,
                std::ptr::null_mut(),
            )
        };
        unsafe { CloseHandle(handle) };
        if ok == 0 || (returned as usize) < std::mem::size_of::<DEVICE_SEEK_PENALTY_DESCRIPTOR>() {
            return None;
        }
        Some(out.IncursSeekPenalty)
    }
}

#[cfg(not(windows))]
mod os {
    use std::path::Path;

    pub(super) fn volume_of(_path: &Path) -> Option<String> {
        None
    }

    pub(super) fn seek_penalty(_volume: &str) -> Option<bool> {
        None
    }
}

#[cfg(test)]
mod tests {
    use std::path::PathBuf;

    use super::*;

    struct Fake {
        volumes: Vec<(&'static str, &'static str, Option<bool>)>,
    }

    impl DriveProbe for Fake {
        fn volume_of(&self, path: &Path) -> Option<String> {
            let text = path.to_string_lossy();
            self.volumes.iter().find(|(prefix, _, _)| text.starts_with(prefix)).map(|(_, v, _)| (*v).to_owned())
        }

        fn seek_penalty(&self, volume: &str) -> Option<bool> {
            self.volumes.iter().find(|(_, v, _)| *v == volume).and_then(|(_, _, p)| *p)
        }
    }

    fn fake() -> Fake {
        Fake { volumes: vec![("C:/", "vol-c", Some(false)), ("D:/", "vol-d", Some(true)), ("E:/", "vol-e", None)] }
    }

    #[test]
    fn auto_goes_sequential_only_on_a_rotational_volume() {
        assert!(decide(ScanMode::Auto, Some(false)));
        assert!(decide(ScanMode::Auto, None));
        assert!(!decide(ScanMode::Auto, Some(true)));
    }

    #[test]
    fn forced_modes_ignore_the_drive() {
        for penalty in [Some(true), Some(false), None] {
            assert!(decide(ScanMode::Parallel, penalty));
            assert!(!decide(ScanMode::Sequential, penalty));
        }
    }

    #[test]
    fn decision_is_per_volume() {
        let probe = fake();
        assert!(parallel_with(&probe, ScanMode::Auto, &PathBuf::from("C:/games")));
        assert!(!parallel_with(&probe, ScanMode::Auto, &PathBuf::from("D:/SteamLibrary")));
        assert!(parallel_with(&probe, ScanMode::Auto, &PathBuf::from("E:/unknown-type")));
        assert!(parallel_with(&probe, ScanMode::Auto, &PathBuf::from("Z:/no-volume")));
        assert!(parallel_with(&probe, ScanMode::Parallel, &PathBuf::from("D:/SteamLibrary")));
        assert!(!parallel_with(&probe, ScanMode::Sequential, &PathBuf::from("C:/games")));
    }

    #[test]
    fn mode_names_roundtrip() {
        for mode in ScanMode::ALL {
            assert_eq!(ScanMode::parse(mode.as_str()), Some(mode));
        }
        assert_eq!(ScanMode::parse(" AUTO "), Some(ScanMode::Auto));
        assert_eq!(ScanMode::parse("fast"), None);
    }

    #[test]
    fn system_probe_never_panics_on_odd_paths() {
        let probe = system_probe();
        let _ = parallel_with(probe, ScanMode::Auto, Path::new(""));
        let _ = parallel_with(probe, ScanMode::Auto, Path::new("definitely/not/here"));
        let _ = parallel_with(probe, ScanMode::Auto, &std::env::temp_dir());
    }
}
