use std::fmt;

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct Region {
    pub base: u64,
    pub size: u64,
    pub writable: bool,
    pub image: bool,
}

pub trait MemSource: Sync {
    fn regions(&self) -> Vec<Region>;
    fn read(&self, addr: u64, buf: &mut [u8]) -> usize;

    fn module_base(&self, _name: &str) -> Option<u64> {
        None
    }

    fn pointer_width(&self) -> usize {
        8
    }

    fn read_vec(&self, addr: u64, len: usize) -> Vec<u8> {
        let mut v = vec![0u8; len];
        let n = self.read(addr, &mut v);
        v.truncate(n);
        v
    }
}

#[derive(Default)]
pub struct VecMem {
    pub blocks: Vec<(u64, Vec<u8>)>,
    pub modules: Vec<(String, u64)>,
}

impl VecMem {
    pub fn add(&mut self, base: u64, data: Vec<u8>) {
        self.blocks.push((base, data));
    }
}

impl MemSource for VecMem {
    fn module_base(&self, name: &str) -> Option<u64> {
        self.modules.iter().find(|(n, _)| n.eq_ignore_ascii_case(name)).map(|(_, b)| *b)
    }

    fn regions(&self) -> Vec<Region> {
        self.blocks
            .iter()
            .map(|(b, d)| Region { base: *b, size: d.len() as u64, writable: true, image: false })
            .collect()
    }

    fn read(&self, addr: u64, buf: &mut [u8]) -> usize {
        for (b, d) in &self.blocks {
            if addr >= *b && addr < *b + d.len() as u64 {
                let off = (addr - *b) as usize;
                let n = buf.len().min(d.len() - off);
                buf[..n].copy_from_slice(&d[off..off + n]);
                return n;
            }
        }
        0
    }
}

#[derive(Debug)]
pub struct AccessError(pub String);

impl fmt::Display for AccessError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.write_str(&self.0)
    }
}

impl std::error::Error for AccessError {}

#[cfg(windows)]
pub use win::{
    GameProcess, enable_debug_privilege, find_processes, is_running, kill_pid, process_image_path, process_is_64bit,
};

#[cfg(not(windows))]
pub fn is_running(_pid: u32) -> bool {
    false
}

#[cfg(not(windows))]
pub fn process_is_64bit(_pid: u32) -> Option<bool> {
    None
}

#[cfg(not(windows))]
pub fn process_image_path(_pid: u32) -> Option<std::path::PathBuf> {
    None
}

#[cfg(not(windows))]
pub fn kill_pid(_pid: u32) -> bool {
    false
}

#[cfg(not(windows))]
pub fn find_processes(_names: &[&str]) -> Vec<(u32, u32, String)> {
    Vec::new()
}

#[cfg(windows)]
mod win {
    use super::{AccessError, MemSource, Region};
    use windows_sys::Win32::Foundation::{CloseHandle, HANDLE, INVALID_HANDLE_VALUE, LUID};
    use windows_sys::Win32::Security::{
        AdjustTokenPrivileges, LUID_AND_ATTRIBUTES, LookupPrivilegeValueW, SE_PRIVILEGE_ENABLED,
        TOKEN_ADJUST_PRIVILEGES, TOKEN_PRIVILEGES, TOKEN_QUERY,
    };
    use windows_sys::Win32::System::Diagnostics::Debug::ReadProcessMemory;
    use windows_sys::Win32::System::Diagnostics::ToolHelp::{
        CreateToolhelp32Snapshot, PROCESSENTRY32W, Process32FirstW, Process32NextW, TH32CS_SNAPPROCESS,
    };
    use windows_sys::Win32::System::Memory::{
        MEM_COMMIT, MEM_IMAGE, MEMORY_BASIC_INFORMATION, PAGE_EXECUTE_READWRITE, PAGE_EXECUTE_WRITECOPY, PAGE_GUARD,
        PAGE_NOACCESS, PAGE_READWRITE, PAGE_WRITECOPY, VirtualQueryEx,
    };
    use windows_sys::Win32::System::Threading::{
        GetCurrentProcess, IsWow64Process, OpenProcess, OpenProcessToken, PROCESS_QUERY_INFORMATION, PROCESS_TERMINATE,
        PROCESS_VM_READ, QueryFullProcessImageNameW, TerminateProcess, WaitForSingleObject,
    };

    pub struct GameProcess {
        pub pid: u32,
        handle: HANDLE,
    }

    unsafe impl Send for GameProcess {}
    unsafe impl Sync for GameProcess {}

    impl Drop for GameProcess {
        fn drop(&mut self) {
            unsafe {
                CloseHandle(self.handle);
            }
        }
    }

    pub fn enable_debug_privilege() -> bool {
        unsafe {
            let mut token: HANDLE = std::ptr::null_mut();
            if OpenProcessToken(GetCurrentProcess(), TOKEN_ADJUST_PRIVILEGES | TOKEN_QUERY, &mut token) == 0 {
                return false;
            }
            let name: Vec<u16> = "SeDebugPrivilege\0".encode_utf16().collect();
            let mut luid = LUID { LowPart: 0, HighPart: 0 };
            if LookupPrivilegeValueW(std::ptr::null(), name.as_ptr(), &mut luid) == 0 {
                CloseHandle(token);
                return false;
            }
            let tp = TOKEN_PRIVILEGES {
                PrivilegeCount: 1,
                Privileges: [LUID_AND_ATTRIBUTES { Luid: luid, Attributes: SE_PRIVILEGE_ENABLED }],
            };
            let ok = AdjustTokenPrivileges(token, 0, &tp, 0, std::ptr::null_mut(), std::ptr::null_mut()) != 0
                && windows_sys::Win32::Foundation::GetLastError() == 0;
            CloseHandle(token);
            ok
        }
    }

    pub fn find_processes(names: &[&str]) -> Vec<(u32, u32, String)> {
        let mut out = Vec::new();
        unsafe {
            let snap = CreateToolhelp32Snapshot(TH32CS_SNAPPROCESS, 0);
            if snap == INVALID_HANDLE_VALUE {
                return out;
            }
            let mut e: PROCESSENTRY32W = std::mem::zeroed();
            e.dwSize = std::mem::size_of::<PROCESSENTRY32W>() as u32;
            let mut ok = Process32FirstW(snap, &mut e) != 0;
            while ok {
                let len = e.szExeFile.iter().position(|c| *c == 0).unwrap_or(e.szExeFile.len());
                let name = String::from_utf16_lossy(&e.szExeFile[..len]);
                if names.iter().any(|n| n.eq_ignore_ascii_case(&name)) {
                    out.push((e.th32ProcessID, e.th32ParentProcessID, name));
                }
                ok = Process32NextW(snap, &mut e) != 0;
            }
            CloseHandle(snap);
        }
        out
    }

    pub fn is_running(pid: u32) -> bool {
        unsafe {
            let h = OpenProcess(PROCESS_QUERY_INFORMATION | 0x0010_0000, 0, pid);
            if h.is_null() {
                return false;
            }
            let r = WaitForSingleObject(h, 0);
            CloseHandle(h);
            r != 0
        }
    }

    pub fn kill_pid(pid: u32) -> bool {
        unsafe {
            let h = OpenProcess(PROCESS_TERMINATE, 0, pid);
            if h.is_null() {
                return false;
            }
            let ok = TerminateProcess(h, 1) != 0;
            CloseHandle(h);
            ok
        }
    }

    pub fn process_image_path(pid: u32) -> Option<std::path::PathBuf> {
        unsafe {
            let h = OpenProcess(0x1000, 0, pid);
            if h.is_null() {
                return None;
            }
            let mut buf = vec![0u16; 1024];
            let mut len = buf.len() as u32;
            let ok = QueryFullProcessImageNameW(h, 0, buf.as_mut_ptr(), &mut len) != 0;
            CloseHandle(h);
            ok.then(|| std::path::PathBuf::from(String::from_utf16_lossy(&buf[..len as usize])))
        }
    }

    pub fn process_is_64bit(pid: u32) -> Option<bool> {
        unsafe {
            let h = OpenProcess(PROCESS_QUERY_INFORMATION, 0, pid);
            if h.is_null() {
                return None;
            }
            let mut wow = 0;
            let ok = IsWow64Process(h, &mut wow) != 0;
            CloseHandle(h);
            if !ok {
                return None;
            }
            Some(wow == 0 && cfg!(target_pointer_width = "64"))
        }
    }

    impl GameProcess {
        pub fn open(pid: u32) -> Result<GameProcess, AccessError> {
            enable_debug_privilege();
            unsafe {
                let handle = OpenProcess(PROCESS_QUERY_INFORMATION | PROCESS_VM_READ, 0, pid);
                if handle.is_null() {
                    return Err(AccessError(format!(
                        "cannot open process {pid} for reading (error {}); run Hammer Pro as the same user as the game, or as administrator",
                        windows_sys::Win32::Foundation::GetLastError()
                    )));
                }
                Ok(GameProcess { pid, handle })
            }
        }
    }

    impl MemSource for GameProcess {
        fn regions(&self) -> Vec<Region> {
            let mut out = Vec::new();
            let mut addr: usize = 0;
            unsafe {
                loop {
                    let mut mbi: MEMORY_BASIC_INFORMATION = std::mem::zeroed();
                    let n = VirtualQueryEx(
                        self.handle,
                        addr as *const _,
                        &mut mbi,
                        std::mem::size_of::<MEMORY_BASIC_INFORMATION>(),
                    );
                    if n == 0 {
                        break;
                    }
                    let base = mbi.BaseAddress as usize;
                    let size = mbi.RegionSize;
                    if mbi.State == MEM_COMMIT && mbi.Protect & (PAGE_NOACCESS | PAGE_GUARD) == 0 && mbi.Protect != 0 {
                        let writable = mbi.Protect
                            & (PAGE_READWRITE | PAGE_WRITECOPY | PAGE_EXECUTE_READWRITE | PAGE_EXECUTE_WRITECOPY)
                            != 0;
                        out.push(Region {
                            base: base as u64,
                            size: size as u64,
                            writable,
                            image: mbi.Type == MEM_IMAGE,
                        });
                    }
                    let next = base.wrapping_add(size);
                    if next <= addr || next >= 0x7fff_ffff_ffff {
                        break;
                    }
                    addr = next;
                }
            }
            out
        }

        fn pointer_width(&self) -> usize {
            if super::process_is_64bit(self.pid) == Some(false) { 4 } else { 8 }
        }

        fn module_base(&self, name: &str) -> Option<u64> {
            use windows_sys::Win32::System::Diagnostics::ToolHelp::{
                MODULEENTRY32W, Module32FirstW, Module32NextW, TH32CS_SNAPMODULE, TH32CS_SNAPMODULE32,
            };
            unsafe {
                let snap = CreateToolhelp32Snapshot(TH32CS_SNAPMODULE | TH32CS_SNAPMODULE32, self.pid);
                if snap == INVALID_HANDLE_VALUE {
                    return None;
                }
                let mut e: MODULEENTRY32W = std::mem::zeroed();
                e.dwSize = std::mem::size_of::<MODULEENTRY32W>() as u32;
                let mut found = None;
                let mut ok = Module32FirstW(snap, &mut e) != 0;
                while ok {
                    let len = e.szModule.iter().position(|c| *c == 0).unwrap_or(e.szModule.len());
                    if String::from_utf16_lossy(&e.szModule[..len]).eq_ignore_ascii_case(name) {
                        found = Some(e.modBaseAddr as u64);
                        break;
                    }
                    ok = Module32NextW(snap, &mut e) != 0;
                }
                CloseHandle(snap);
                found
            }
        }

        fn read(&self, addr: u64, buf: &mut [u8]) -> usize {
            let mut got = 0usize;
            unsafe {
                ReadProcessMemory(self.handle, addr as *const _, buf.as_mut_ptr().cast(), buf.len(), &mut got);
            }
            got
        }
    }
}

#[cfg(all(test, windows))]
mod live_tests {
    use super::*;
    use crate::scan::{ScanOptions, find_entity_lumps};

    #[test]
    fn the_memory_of_a_live_process_is_read() {
        let magic: Vec<u8> = (0..200_000u32).map(|i| (i.wrapping_mul(2_654_435_761) >> 13) as u8).collect();
        let p = GameProcess::open(std::process::id()).expect("own process");
        let addr = magic.as_ptr() as u64;
        let mut back = vec![0u8; magic.len()];
        assert_eq!(p.read(addr, &mut back), magic.len());
        assert_eq!(back, magic);
        let regions = p.regions();
        assert!(regions.iter().any(|r| addr >= r.base && addr < r.base + r.size && r.writable && !r.image));
        assert!(regions.windows(2).all(|w| w[0].base < w[1].base));
        let mut nothing = vec![0u8; 16];
        assert_eq!(p.read(8, &mut nothing), 0);
        assert_eq!(p.read_vec(addr + 10, 5), magic[10..15].to_vec());
    }

    #[test]
    fn a_planted_entity_lump_is_found_in_a_live_process() {
        let mut text = String::from("{\n\"classname\" \"worldspawn\"\n\"skyname\" \"sky_live_test\"\n}\n");
        for i in 0..40 {
            text.push_str(&format!("{{\n\"classname\" \"info_target\"\n\"targetname\" \"live{i}\"\n}}\n"));
        }
        let mut block = vec![0u8; 1 << 20];
        block.extend_from_slice(text.as_bytes());
        block.extend_from_slice(&vec![0u8; 1 << 20]);
        let block = std::hint::black_box(block);
        let p = GameProcess::open(std::process::id()).expect("own process");
        let opts = ScanOptions { writable_only: true, min_region: 1 << 12, ..ScanOptions::default() };
        let hits = find_entity_lumps(&p, &opts);
        assert!(
            hits.iter().any(|h| h.text.contains("sky_live_test") && h.text.contains("live39") && h.entities >= 41),
            "{} hits",
            hits.len()
        );
        assert!(std::hint::black_box(&block).len() > 8192);
    }

    #[test]
    fn process_helpers_agree_with_the_system() {
        let me = std::process::id();
        assert!(is_running(me));
        assert!(!is_running(0x7fff_fff0));
        assert_eq!(process_is_64bit(me), Some(cfg!(target_pointer_width = "64")));
        let path = process_image_path(me).expect("image path");
        assert!(path.is_file());
        assert!(GameProcess::open(0x7fff_fff0).is_err());
        assert!(!kill_pid(0x7fff_fff0));
        let own_name = path.file_name().expect("name").to_string_lossy().into_owned();
        let found = find_processes(&[own_name.as_str()]);
        assert!(found.iter().any(|(pid, _, _)| *pid == me));
    }
}
