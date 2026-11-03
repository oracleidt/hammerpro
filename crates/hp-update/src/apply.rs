use std::path::{Path, PathBuf};
use std::process::Command;
use std::time::{Duration, Instant};

use crate::config::PROGRAM_NAME;
use crate::error::UpdateError;
use crate::zip::Archive;

pub const APPLY_ARG: &str = "--apply-update";
const MAX_PROGRAM_SIZE: u64 = 2 * 1024 * 1024 * 1024;
const HELPER_NAME: &str = "hammerpro-updater.exe";

#[cfg(windows)]
const CREATE_NO_WINDOW: u32 = 0x0800_0000;
#[cfg(windows)]
const DETACHED_PROCESS: u32 = 0x0000_0008;

#[derive(Clone, Debug)]
pub struct ApplyJob {
    pub zip: PathBuf,
    pub target: PathBuf,
    pub pid: u32,
    pub relaunch: bool,
    pub kill_by_name: bool,
    pub wait_for_exit: Duration,
    pub swap_deadline: Duration,
    pub retry_every: Duration,
}

impl ApplyJob {
    pub fn new(zip: PathBuf, target: PathBuf, pid: u32) -> Self {
        Self {
            zip,
            target,
            pid,
            relaunch: true,
            kill_by_name: true,
            wait_for_exit: Duration::from_secs(20),
            swap_deadline: Duration::from_secs(40),
            retry_every: Duration::from_millis(250),
        }
    }

    pub fn from_args(args: &[String]) -> Option<ApplyJob> {
        let [zip, target, pid] = args else { return None };
        Some(ApplyJob::new(PathBuf::from(zip), PathBuf::from(target), pid.parse().ok()?))
    }

    pub fn staged_path(&self) -> PathBuf {
        suffixed(&self.target, ".new")
    }

    pub fn backup_path(&self) -> PathBuf {
        suffixed(&self.target, ".old")
    }
}

fn suffixed(path: &Path, suffix: &str) -> PathBuf {
    let mut name = path.file_name().map(|n| n.to_os_string()).unwrap_or_default();
    name.push(suffix);
    path.with_file_name(name)
}

#[derive(Clone, Debug)]
pub struct Helper {
    pub exe: PathBuf,
}

impl Helper {
    pub fn prepare() -> Result<Helper, UpdateError> {
        let me = std::env::current_exe()?;
        let dir = std::env::temp_dir().join(format!("HammerPro-updater-{}", std::process::id()));
        std::fs::create_dir_all(&dir)?;
        let exe = dir.join(HELPER_NAME);
        std::fs::copy(&me, &exe)?;
        Ok(Helper { exe })
    }

    pub fn spawn(&self, zip: &Path) -> Result<u32, UpdateError> {
        let me = std::env::current_exe()?;
        let mut cmd = Command::new(&self.exe);
        cmd.arg(APPLY_ARG).arg(zip).arg(&me).arg(std::process::id().to_string());
        hide(&mut cmd, true);
        let child = cmd.spawn().map_err(|e| UpdateError::Apply(format!("cannot start the updater: {e}")))?;
        Ok(child.id())
    }
}

#[cfg(windows)]
fn hide(cmd: &mut Command, detached: bool) {
    use std::os::windows::process::CommandExt;
    cmd.creation_flags(CREATE_NO_WINDOW | if detached { DETACHED_PROCESS } else { 0 });
}

#[cfg(not(windows))]
fn hide(_cmd: &mut Command, _detached: bool) {}

pub fn cleanup_stale() {
    let Ok(read) = std::fs::read_dir(std::env::temp_dir()) else { return };
    let own = format!("HammerPro-updater-{}", std::process::id());
    for entry in read.flatten() {
        let name = entry.file_name().to_string_lossy().into_owned();
        if !(name.starts_with("HammerPro-updater-") || name.starts_with("HammerPro-update-")) || name == own {
            continue;
        }
        let old = entry
            .metadata()
            .and_then(|m| m.modified())
            .ok()
            .and_then(|t| t.elapsed().ok())
            .is_some_and(|age| age > Duration::from_secs(3600));
        if old {
            let _ = std::fs::remove_dir_all(entry.path());
        }
    }
}

pub fn pid_alive(pid: u32) -> bool {
    if pid == 0 {
        return false;
    }
    imp::alive(pid)
}

fn wait_for_exit(pid: u32, limit: Duration) -> bool {
    let start = Instant::now();
    while pid_alive(pid) {
        if start.elapsed() >= limit {
            return false;
        }
        std::thread::sleep(Duration::from_millis(100));
    }
    true
}

#[cfg(windows)]
mod imp {
    use windows_sys::Win32::Foundation::CloseHandle;
    use windows_sys::Win32::System::Threading::{OpenProcess, WaitForSingleObject};

    const SYNCHRONIZE: u32 = 0x0010_0000;

    pub fn alive(pid: u32) -> bool {
        unsafe {
            let handle = OpenProcess(SYNCHRONIZE, 0, pid);
            if handle.is_null() {
                return false;
            }
            let waited = WaitForSingleObject(handle, 0);
            CloseHandle(handle);
            waited != 0
        }
    }
}

#[cfg(not(windows))]
mod imp {
    pub fn alive(pid: u32) -> bool {
        std::path::Path::new(&format!("/proc/{pid}")).exists()
    }
}

fn kill_pid(pid: u32) {
    #[cfg(windows)]
    {
        let mut cmd = Command::new("taskkill");
        cmd.args(["/F", "/PID", &pid.to_string()]);
        hide(&mut cmd, false);
        let _ = cmd.output();
    }
    #[cfg(not(windows))]
    {
        let _ = Command::new("kill").args(["-9", &pid.to_string()]).output();
    }
}

fn kill_image(name: &str) {
    #[cfg(windows)]
    {
        let mut cmd = Command::new("taskkill");
        cmd.args(["/F", "/IM", name, "/FI", &format!("PID ne {}", std::process::id())]);
        hide(&mut cmd, false);
        let _ = cmd.output();
    }
    #[cfg(not(windows))]
    {
        let _ = name;
    }
}

pub fn apply(job: &ApplyJob, log: &mut dyn FnMut(String)) -> Result<(), UpdateError> {
    if job.pid != 0 && job.pid != std::process::id() {
        log(format!("waiting for process {} to exit", job.pid));
        if !wait_for_exit(job.pid, job.wait_for_exit) {
            log(format!("process {} still runs, killing it", job.pid));
            kill_pid(job.pid);
            if !wait_for_exit(job.pid, Duration::from_secs(5)) {
                return Err(UpdateError::Apply(format!("process {} does not exit", job.pid)));
            }
        }
    }
    let staged = job.staged_path();
    let backup = job.backup_path();
    let _ = std::fs::remove_file(&staged);
    let extracted = stage(job, &staged, log);
    if let Err(e) = extracted {
        let _ = std::fs::remove_file(&staged);
        return Err(e);
    }
    let started = Instant::now();
    let mut killed_by_name = false;
    let mut attempt = 0u32;
    loop {
        attempt += 1;
        match swap(&job.target, &staged, &backup) {
            Ok(()) => {
                log(format!("replaced {} (attempt {attempt})", job.target.display()));
                return Ok(());
            }
            Err(e) => {
                let elapsed = started.elapsed();
                if elapsed >= job.swap_deadline {
                    let _ = std::fs::remove_file(&staged);
                    return Err(UpdateError::Apply(format!(
                        "cannot replace {} ({e}); the previous version was kept",
                        job.target.display()
                    )));
                }
                if job.kill_by_name && !killed_by_name && elapsed >= job.swap_deadline / 3 {
                    killed_by_name = true;
                    if let Some(name) = job.target.file_name().and_then(|n| n.to_str()) {
                        log(format!(
                            "{} is locked ({e}), closing the remaining {name} processes",
                            job.target.display()
                        ));
                        kill_image(name);
                    }
                }
                std::thread::sleep(job.retry_every);
            }
        }
    }
}

fn stage(job: &ApplyJob, staged: &Path, log: &mut dyn FnMut(String)) -> Result<(), UpdateError> {
    let file = std::fs::File::open(&job.zip)?;
    let mut archive = Archive::open(std::io::BufReader::new(file))?;
    let entry = archive
        .find_file(PROGRAM_NAME)
        .cloned()
        .ok_or_else(|| UpdateError::BadZip(format!("{PROGRAM_NAME} is not in the archive")))?;
    let size = archive.extract_to(&entry, staged, MAX_PROGRAM_SIZE)?;
    log(format!("extracted {} ({size} bytes)", entry.name));
    verify_program(staged)
}

fn verify_program(path: &Path) -> Result<(), UpdateError> {
    use std::io::Read;
    let mut head = [0u8; 4];
    let mut file = std::fs::File::open(path)?;
    let n = file.read(&mut head)?;
    let ok = if cfg!(windows) { n >= 2 && &head[..2] == b"MZ" } else { n >= 4 && &head == b"\x7fELF" };
    if ok { Ok(()) } else { Err(UpdateError::BadZip("the program in the archive is not an executable".into())) }
}

fn swap(target: &Path, staged: &Path, backup: &Path) -> std::io::Result<()> {
    let had_target = target.exists();
    if had_target {
        if backup.exists() {
            std::fs::remove_file(backup)?;
        }
        std::fs::rename(target, backup)?;
    }
    match std::fs::rename(staged, target) {
        Ok(()) => Ok(()),
        Err(e) => {
            if had_target {
                let _ = std::fs::rename(backup, target);
            }
            Err(e)
        }
    }
}

pub struct HelperReport {
    pub target: PathBuf,
    pub error: Option<UpdateError>,
    pub log_path: PathBuf,
}

pub fn run_helper(args: &[String]) -> Option<HelperReport> {
    let job = ApplyJob::from_args(args)?;
    let log_path = std::env::current_exe()
        .ok()
        .and_then(|p| p.parent().map(|d| d.join("update.log")))
        .unwrap_or_else(|| std::env::temp_dir().join("hammerpro-update.log"));
    let mut lines: Vec<String> = Vec::new();
    let result = apply(&job, &mut |line| lines.push(line));
    let error = result.err();
    if let Some(e) = &error {
        lines.push(e.to_string());
    }
    lines.push(if error.is_none() { "done".to_owned() } else { "rolled back".to_owned() });
    let _ = std::fs::write(&log_path, lines.join("\n") + "\n");
    Some(HelperReport { target: job.target, error, log_path })
}

pub fn relaunch(target: &Path) -> Result<(), UpdateError> {
    let mut cmd = Command::new(target);
    if let Some(dir) = target.parent() {
        cmd.current_dir(dir);
    }
    hide(&mut cmd, true);
    cmd.spawn().map(|_| ()).map_err(|e| UpdateError::Apply(format!("cannot start {}: {e}", target.display())))
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::zip::fake::{Item, build};

    fn scratch(name: &str) -> PathBuf {
        let dir = std::env::var_os("HP_UPDATE_TEST_DIR")
            .map(PathBuf::from)
            .unwrap_or_else(std::env::temp_dir)
            .join(format!("hp_update_apply_{}_{name}", std::process::id()));
        let _ = std::fs::remove_dir_all(&dir);
        std::fs::create_dir_all(&dir).unwrap();
        dir
    }

    fn job(dir: &Path, zip: Vec<u8>) -> ApplyJob {
        let zip_path = dir.join("update.zip");
        std::fs::write(&zip_path, zip).unwrap();
        let target = dir.join("install").join("hammerpro.exe");
        std::fs::create_dir_all(target.parent().unwrap()).unwrap();
        std::fs::write(&target, b"MZ-old-program").unwrap();
        let mut j = ApplyJob::new(zip_path, target, 0);
        j.relaunch = false;
        j.kill_by_name = false;
        j.retry_every = Duration::from_millis(20);
        j.swap_deadline = Duration::from_secs(5);
        j
    }

    fn good_zip() -> Vec<u8> {
        build(&[
            Item { name: "HammerPro-9.9.9/readme.txt", data: b"notes", deflate: false },
            Item { name: "HammerPro-9.9.9/hammerpro.exe", data: b"MZ-new-program-bytes", deflate: true },
        ])
    }

    #[test]
    fn replaces_the_program_keeps_a_backup_and_cleans_up() {
        let dir = scratch("ok");
        let j = job(&dir, good_zip());
        let mut log = Vec::new();
        apply(&j, &mut |l| log.push(l)).unwrap();
        assert_eq!(std::fs::read(&j.target).unwrap(), b"MZ-new-program-bytes");
        assert_eq!(std::fs::read(j.backup_path()).unwrap(), b"MZ-old-program");
        assert!(!j.staged_path().exists());
        assert!(log.iter().any(|l| l.contains("replaced")));
        std::fs::write(&j.target, b"MZ-second-old").unwrap();
        apply(&j, &mut |_| {}).unwrap();
        assert_eq!(std::fs::read(j.backup_path()).unwrap(), b"MZ-second-old", "the backup is renewed");
        let _ = std::fs::remove_dir_all(&dir);
    }

    #[test]
    fn installs_even_when_no_program_exists_yet() {
        let dir = scratch("fresh");
        let j = job(&dir, good_zip());
        std::fs::remove_file(&j.target).unwrap();
        apply(&j, &mut |_| {}).unwrap();
        assert_eq!(std::fs::read(&j.target).unwrap(), b"MZ-new-program-bytes");
        assert!(!j.backup_path().exists());
        let _ = std::fs::remove_dir_all(&dir);
    }

    #[test]
    fn a_bad_archive_leaves_the_old_program_untouched() {
        let dir = scratch("bad");
        let no_exe = job(&dir, build(&[Item { name: "readme.txt", data: b"x", deflate: false }]));
        assert!(matches!(apply(&no_exe, &mut |_| {}), Err(UpdateError::BadZip(_))));
        assert_eq!(std::fs::read(&no_exe.target).unwrap(), b"MZ-old-program");
        assert!(!no_exe.staged_path().exists() && !no_exe.backup_path().exists());

        let not_exe = job(&dir, build(&[Item { name: "hammerpro.exe", data: b"just text", deflate: false }]));
        assert!(matches!(apply(&not_exe, &mut |_| {}), Err(UpdateError::BadZip(_))));
        assert_eq!(std::fs::read(&not_exe.target).unwrap(), b"MZ-old-program");

        let mut zip = build(&[Item { name: "hammerpro.exe", data: b"MZ-new-program-bytes", deflate: false }]);
        let at = zip.windows(6).position(|w| w == b"MZ-new").unwrap();
        zip[at + 4] ^= 0x55;
        let corrupt = job(&dir, zip);
        assert!(apply(&corrupt, &mut |_| {}).is_err());
        assert_eq!(std::fs::read(&corrupt.target).unwrap(), b"MZ-old-program");

        let missing = job(&dir, good_zip());
        let mut gone = missing.clone();
        gone.zip = dir.join("nothing.zip");
        assert!(matches!(apply(&gone, &mut |_| {}), Err(UpdateError::Io(_))));
        let _ = std::fs::remove_dir_all(&dir);
    }

    #[cfg(windows)]
    #[test]
    fn retries_while_the_program_is_locked_then_succeeds() {
        use std::os::windows::fs::OpenOptionsExt;
        let dir = scratch("lock");
        let j = job(&dir, good_zip());
        let held = std::fs::OpenOptions::new().read(true).share_mode(0).open(&j.target).unwrap();
        let release = std::thread::spawn(move || {
            std::thread::sleep(Duration::from_millis(700));
            drop(held);
        });
        let mut attempts = Vec::new();
        apply(&j, &mut |l| attempts.push(l)).unwrap();
        release.join().unwrap();
        assert_eq!(std::fs::read(&j.target).unwrap(), b"MZ-new-program-bytes");
        let line = attempts.iter().find(|l| l.contains("attempt")).unwrap();
        assert!(!line.ends_with("(attempt 1)"), "{line}");
        let _ = std::fs::remove_dir_all(&dir);
    }

    #[cfg(windows)]
    #[test]
    fn gives_up_after_the_deadline_and_keeps_the_old_program() {
        use std::os::windows::fs::OpenOptionsExt;
        let dir = scratch("stuck");
        let mut j = job(&dir, good_zip());
        j.swap_deadline = Duration::from_millis(400);
        let held = std::fs::OpenOptions::new().read(true).share_mode(0).open(&j.target).unwrap();
        let err = apply(&j, &mut |_| {}).unwrap_err();
        assert!(matches!(&err, UpdateError::Apply(m) if m.contains("previous version was kept")), "{err}");
        drop(held);
        assert_eq!(std::fs::read(&j.target).unwrap(), b"MZ-old-program");
        assert!(!j.staged_path().exists());
        let _ = std::fs::remove_dir_all(&dir);
    }

    #[test]
    fn waits_for_a_running_process_to_end() {
        let dir = scratch("wait");
        let mut j = job(&dir, good_zip());
        #[cfg(windows)]
        let mut child = Command::new("cmd").args(["/C", "ping -n 2 127.0.0.1 >nul"]).spawn().unwrap();
        #[cfg(not(windows))]
        let mut child = Command::new("sleep").arg("1").spawn().unwrap();
        j.pid = child.id();
        let started = Instant::now();
        let waiter = std::thread::spawn(move || {
            let _ = child.wait();
        });
        apply(&j, &mut |_| {}).unwrap();
        waiter.join().unwrap();
        assert!(started.elapsed() >= Duration::from_millis(300));
        assert!(!pid_alive(j.pid));
        let _ = std::fs::remove_dir_all(&dir);
    }

    #[test]
    fn parses_the_helper_command_line() {
        let args: Vec<String> =
            ["C:\\t\\u.zip", "C:\\Apps\\hammerpro.exe", "4242"].iter().map(|s| (*s).to_owned()).collect();
        let j = ApplyJob::from_args(&args).unwrap();
        assert_eq!((j.pid, j.zip.to_str().unwrap()), (4242, "C:\\t\\u.zip"));
        assert_eq!(j.backup_path().file_name().unwrap(), "hammerpro.exe.old");
        assert_eq!(j.staged_path().file_name().unwrap(), "hammerpro.exe.new");
        assert!(ApplyJob::from_args(&args[..2]).is_none());
        let mut bad = args.clone();
        bad[2] = "x".into();
        assert!(ApplyJob::from_args(&bad).is_none());
        assert!(!pid_alive(0));
    }

    #[test]
    fn stale_cleanup_ignores_recent_and_foreign_folders() {
        let recent = std::env::temp_dir().join(format!("HammerPro-update-test-{}", std::process::id()));
        std::fs::create_dir_all(&recent).unwrap();
        cleanup_stale();
        assert!(recent.exists());
        let _ = std::fs::remove_dir_all(&recent);
    }
}
