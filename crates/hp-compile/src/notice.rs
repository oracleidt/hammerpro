use std::fmt;
use std::path::PathBuf;

use serde::{Deserialize, Serialize};

use crate::StepKind;
use crate::log::Severity;

#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize, Deserialize)]
pub enum CrashKind {
    AccessViolation,
    DivisionByZero,
    StackOverflow,
    MemoryCorruption,
    Aborted,
    OutOfMemory,
    Breakpoint,
    MissingDll,
    WrongArchitecture,
    Interrupted,
    IllegalInstruction,
}

impl CrashKind {
    pub fn english(self) -> &'static str {
        match self {
            CrashKind::AccessViolation => "The program crashed (access violation).",
            CrashKind::DivisionByZero => "The program crashed (division by zero).",
            CrashKind::StackOverflow => "The program crashed (stack overflow).",
            CrashKind::MemoryCorruption => "The program crashed (memory corruption).",
            CrashKind::Aborted => "The program aborted (fatal error or stack buffer overrun).",
            CrashKind::OutOfMemory => "The program ran out of memory.",
            CrashKind::Breakpoint => "The program hit a breakpoint (assertion failure).",
            CrashKind::MissingDll => "The program has missing DLLs.",
            CrashKind::WrongArchitecture => "Mismatched architecture (32-bit and 64-bit DLLs mixed).",
            CrashKind::Interrupted => "The program was interrupted (Ctrl+C).",
            CrashKind::IllegalInstruction => "The program crashed (illegal instruction, CPU not supported).",
        }
    }
}

pub fn crash_kind(code: i32) -> Option<CrashKind> {
    let status = u32::from_ne_bytes(code.to_ne_bytes());
    Some(match status {
        0xC000_0005 => CrashKind::AccessViolation,
        0xC000_0094 | 0xC000_008E => CrashKind::DivisionByZero,
        0xC000_00FD => CrashKind::StackOverflow,
        0xC000_0374 => CrashKind::MemoryCorruption,
        0xC000_0409 => CrashKind::Aborted,
        0xC000_0017 | 0xC000_012D => CrashKind::OutOfMemory,
        0x8000_0003 => CrashKind::Breakpoint,
        0xC000_0135 | 0xC000_0139 => CrashKind::MissingDll,
        0xC000_007B => CrashKind::WrongArchitecture,
        0xC000_013A => CrashKind::Interrupted,
        0xC000_001D => CrashKind::IllegalInstruction,
        _ => return None,
    })
}

pub fn format_exit_code(code: i32) -> String {
    if crash_kind(code).is_some() {
        format!("0x{:08X}", u32::from_ne_bytes(code.to_ne_bytes()))
    } else {
        code.to_string()
    }
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub enum Notice {
    ThreadFailed { error: String },
    PipelineStopped,
    Leaked { pointfile: PathBuf },
    LeakedNoPointfile,
    BspNotWritten { bsp: PathBuf },
    PackInvalidPath { path: String },
    PackMissingFile { file: PathBuf },
    NothingToPack,
    PackFailed { error: String },
    Packing { count: usize },
    Packed { count: usize, bytes: u64 },
    PackNotInBsp { files: Vec<String> },
    CopySourceMissing { from: PathBuf },
    CreateDirFailed { dir: PathBuf, error: String },
    CopyFailed { from: PathBuf, to: PathBuf, error: String },
    ReplaceFailed { to: PathBuf, error: String },
    Copied { from: PathBuf, to: PathBuf },
    CubemapsKept { count: usize, bsp: PathBuf },
    Started { exe: PathBuf },
    StartFailed { exe: PathBuf, error: String },
    Cancelled,
    QueryFailed { error: String },
    Crashed { program: String, code: i32, kind: CrashKind },
    ExitCode { program: String, code: i32 },
    Terminated { program: String },
    NoPreviousBsp { bsp: PathBuf },
    StoppedOnLeak,
    FromCache { from: PathBuf },
    PackWaiting,
    PackListUnavailable,
    FileOpFailed { path: PathBuf, error: String },
    StepOutOfMemory { step: StepKind, bytes: u64 },
    CrashedDuring { step: StepKind, lines: Vec<String> },
}

impl Notice {
    pub fn severity(&self) -> Severity {
        match self {
            Notice::Copied { .. } | Notice::Started { .. } | Notice::FromCache { .. } | Notice::Packed { .. } => {
                Severity::Success
            }
            Notice::Packing { .. } | Notice::PackWaiting | Notice::CubemapsKept { .. } | Notice::NothingToPack => {
                Severity::Info
            }
            Notice::PackInvalidPath { .. }
            | Notice::PackMissingFile { .. }
            | Notice::PackNotInBsp { .. }
            | Notice::Cancelled
            | Notice::NoPreviousBsp { .. }
            | Notice::PackListUnavailable => Severity::Warning,
            Notice::ThreadFailed { .. }
            | Notice::PipelineStopped
            | Notice::Leaked { .. }
            | Notice::LeakedNoPointfile
            | Notice::BspNotWritten { .. }
            | Notice::PackFailed { .. }
            | Notice::CopySourceMissing { .. }
            | Notice::CreateDirFailed { .. }
            | Notice::CopyFailed { .. }
            | Notice::ReplaceFailed { .. }
            | Notice::StartFailed { .. }
            | Notice::QueryFailed { .. }
            | Notice::Crashed { .. }
            | Notice::ExitCode { .. }
            | Notice::Terminated { .. }
            | Notice::StoppedOnLeak
            | Notice::FileOpFailed { .. }
            | Notice::StepOutOfMemory { .. }
            | Notice::CrashedDuring { .. } => Severity::Error,
        }
    }
}

impl fmt::Display for Notice {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Notice::ThreadFailed { error } => write!(f, "Could not start the compile thread: {error}"),
            Notice::PipelineStopped => f.write_str("Compile stopped: this step failed."),
            Notice::Leaked { pointfile } => write!(
                f,
                "The map leaked: an entity can reach the void. Leak path written to {}.",
                pointfile.display()
            ),
            Notice::LeakedNoPointfile => f.write_str("The map leaked (no pointfile was written)."),
            Notice::BspNotWritten { bsp } => write!(f, "The compiler did not write {}.", bsp.display()),
            Notice::PackInvalidPath { path } => write!(f, "Pack: invalid path {path:?}, skipped."),
            Notice::PackMissingFile { file } => write!(f, "Pack: {} does not exist, skipped.", file.display()),
            Notice::NothingToPack => f.write_str("Nothing to pack."),
            Notice::PackFailed { error } => write!(f, "Could not pack the files into the BSP: {error}"),
            Notice::Packing { count } => write!(f, "Packing {count} file(s)."),
            Notice::Packed { count, bytes } => {
                write!(f, "Packed {count} file(s) into the BSP (pakfile: {}).", format_bytes(*bytes))
            }
            Notice::PackNotInBsp { files } => {
                write!(f, "Pack: {} file(s) are missing from the BSP after packing: {}", files.len(), files.join(", "))
            }
            Notice::CopySourceMissing { from } => {
                write!(f, "Cannot copy {}: the file does not exist.", from.display())
            }
            Notice::CreateDirFailed { dir, error } => write!(f, "Could not create {}: {error}", dir.display()),
            Notice::CopyFailed { from, to, error } => {
                write!(f, "Could not copy {} to {}: {error}", from.display(), to.display())
            }
            Notice::ReplaceFailed { to, error } => write!(
                f,
                "Could not replace {}: {error}. If the map is loaded in the game, change map or disconnect first.",
                to.display()
            ),
            Notice::Copied { from, to } => write!(f, "Copied {} to {}.", from.display(), to.display()),
            Notice::CubemapsKept { count, bsp } => {
                write!(f, "Kept {count} cubemap file(s) built in game in {}.", bsp.display())
            }
            Notice::Started { exe } => write!(f, "Started {}.", exe.display()),
            Notice::StartFailed { exe, error } => write!(f, "Could not start {}: {error}", exe.display()),
            Notice::Cancelled => f.write_str("Cancelled."),
            Notice::QueryFailed { error } => write!(f, "Could not query the process: {error}"),
            Notice::Crashed { program, code, kind } => {
                write!(f, "{program} failed with exit code {}: {}", format_exit_code(*code), kind.english())
            }
            Notice::ExitCode { program, code } => write!(f, "{program} failed with exit code {code}."),
            Notice::Terminated { program } => write!(f, "{program} was terminated."),
            Notice::NoPreviousBsp { bsp } => write!(
                f,
                "{} does not exist yet: a full compile runs instead of the partial one.",
                bsp.display()
            ),
            Notice::StoppedOnLeak => f.write_str(
                "Compile stopped after the BSP build: the map leaks. Fix the leak (follow the leak path), or allow compiling leaking maps.",
            ),
            Notice::FromCache { from } => write!(f, "Reused the identical earlier result {}.", from.display()),
            Notice::PackWaiting => f.write_str("Waiting for the list of files to pack."),
            Notice::PackListUnavailable => f.write_str("The list of files to pack could not be prepared."),
            Notice::FileOpFailed { path, error } => write!(f, "Could not change {}: {error}", path.display()),
            Notice::StepOutOfMemory { step, bytes } => write!(
                f,
                "The {} step ran out of memory: it asked for {} in one block. One part of the map is probably far bigger than the rest (a huge or broken face, a very fine lightmap scale, a corrupted displacement): run Check for problems, or send the compile log to the Hammer Pro developers.",
                step.english(),
                format_bytes(*bytes)
            ),
            Notice::CrashedDuring { step, lines } => {
                write!(f, "The {} step stopped. Last lines of its log:", step.english())?;
                for l in lines {
                    write!(f, "\n  {l}")?;
                }
                Ok(())
            }
        }
    }
}

pub fn format_bytes(bytes: u64) -> String {
    const KB: f64 = 1024.0;
    let b = bytes as f64;
    if b < KB {
        format!("{bytes} B")
    } else if b < KB * KB {
        format!("{:.1} KB", b / KB)
    } else if b < KB * KB * KB {
        format!("{:.1} MB", b / (KB * KB))
    } else {
        format!("{:.2} GB", b / (KB * KB * KB))
    }
}

pub fn failed_allocation(line: &str) -> Option<u64> {
    let rest = line.split("memory allocation of ").nth(1)?;
    let (n, tail) = rest.split_once(' ')?;
    tail.starts_with("bytes failed").then_some(())?;
    n.parse().ok()
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn crash_codes() {
        assert_eq!(crash_kind(-1_073_741_819), Some(CrashKind::AccessViolation));
        assert_eq!(crash_kind(i32::from_ne_bytes(0xC000_0135u32.to_ne_bytes())), Some(CrashKind::MissingDll));
        assert_eq!(crash_kind(1), None);
        assert_eq!(format_exit_code(-1_073_741_819), "0xC0000005");
        assert_eq!(format_exit_code(3), "3");
    }

    #[test]
    fn english_text_and_severity() {
        let n = Notice::ExitCode { program: "hammerpro.exe".into(), code: 1 };
        assert_eq!(n.to_string(), "hammerpro.exe failed with exit code 1.");
        assert_eq!(n.severity(), Severity::Error);
        let n =
            Notice::Crashed { program: "hammerpro.exe".into(), code: -1_073_741_819, kind: CrashKind::AccessViolation };
        assert_eq!(
            n.to_string(),
            "hammerpro.exe failed with exit code 0xC0000005: The program crashed (access violation)."
        );
        assert_eq!(Notice::Copied { from: "a".into(), to: "b".into() }.severity(), Severity::Success);
        assert_eq!(Notice::NothingToPack.severity(), Severity::Info);
        assert_eq!(Notice::Cancelled.to_string(), "Cancelled.");
        assert_eq!(
            Notice::Packed { count: 2, bytes: 1536 }.to_string(),
            "Packed 2 file(s) into the BSP (pakfile: 1.5 KB)."
        );
        assert_eq!(format_bytes(12), "12 B");
        assert_eq!(format_bytes(3 * 1024 * 1024), "3.0 MB");
        assert_eq!(format_bytes(5_001_064_080), "4.66 GB");
    }

    #[test]
    fn failed_allocations_are_read_from_the_log() {
        assert_eq!(failed_allocation("memory allocation of 5001064080 bytes failed"), Some(5_001_064_080));
        assert_eq!(failed_allocation("x: memory allocation of 12 bytes failed (out)"), Some(12));
        assert_eq!(failed_allocation("memory allocation of many bytes failed"), None);
        assert_eq!(failed_allocation("Visibility 15%"), None);
        let n = Notice::StepOutOfMemory { step: StepKind::Rad, bytes: 5_001_064_080 };
        assert!(n.to_string().starts_with("The lighting step ran out of memory: it asked for 4.66 GB"), "{n}");
        let n = Notice::CrashedDuring { step: StepKind::Bsp, lines: vec!["a".into(), "b".into()] };
        assert_eq!(n.to_string(), "The BSP step stopped. Last lines of its log:\n  a\n  b");
        assert_eq!(n.severity(), Severity::Error);
    }
}
