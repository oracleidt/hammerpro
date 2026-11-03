use std::collections::HashSet;
use std::io::{Read, Seek, SeekFrom};
use std::path::{Path, PathBuf};
use std::process::Command;

use crate::settings::{GMOD_APP_ID, InsecureMode, SnifferSettings};

pub const PROCESS_NAMES: &[&str] = &["gmod.exe", "hl2.exe"];

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct GmodInstall {
    pub root: PathBuf,
    pub game_dir: PathBuf,
    pub exe: PathBuf,
    pub is_64bit: bool,
}

impl GmodInstall {
    pub fn console_log(&self) -> PathBuf {
        self.game_dir.join("console.log")
    }
}

pub fn install_at(path: &Path) -> Option<GmodInstall> {
    let mut candidates: Vec<PathBuf> = vec![path.to_path_buf()];
    if let Some(p) = path.parent() {
        candidates.push(p.to_path_buf());
        if let Some(pp) = p.parent() {
            candidates.push(pp.to_path_buf());
            if let Some(ppp) = pp.parent() {
                candidates.push(ppp.to_path_buf());
            }
        }
    }
    for root in candidates {
        let game_dir = root.join("garrysmod");
        if !game_dir.is_dir() {
            continue;
        }
        let x64 = root.join("bin").join("win64").join("gmod.exe");
        if x64.is_file() {
            return Some(GmodInstall { root, game_dir, exe: x64, is_64bit: true });
        }
        for name in ["hl2.exe", "gmod.exe"] {
            let exe = root.join(name);
            if exe.is_file() {
                return Some(GmodInstall { root, game_dir, exe, is_64bit: false });
            }
        }
    }
    None
}

pub fn detect_install(override_dir: &str) -> Result<GmodInstall, String> {
    let t = override_dir.trim();
    if !t.is_empty() {
        return install_at(Path::new(t))
            .ok_or_else(|| format!("{t} is not a Garry's Mod folder (no garrysmod folder and no gmod.exe)"));
    }
    let root = hp_fs::detect_gmod_root()
        .ok_or_else(|| "Garry's Mod was not found; set its folder in the sniffer settings".to_owned())?;
    install_at(&root).ok_or_else(|| format!("{} does not contain a runnable Garry's Mod", root.display()))
}

pub fn split_args(text: &str) -> Vec<String> {
    let mut out = Vec::new();
    let mut cur = String::new();
    let mut quoted = false;
    let mut any = false;
    for c in text.chars() {
        match c {
            '"' => {
                quoted = !quoted;
                any = true;
            }
            c if c.is_whitespace() && !quoted => {
                if any || !cur.is_empty() {
                    out.push(std::mem::take(&mut cur));
                    any = false;
                }
            }
            c => cur.push(c),
        }
    }
    if any || !cur.is_empty() {
        out.push(cur);
    }
    out
}

pub fn build_args(s: &SnifferSettings, host: &str, port: u16, server_vac: Option<bool>) -> Vec<String> {
    let mut args: Vec<String> = Vec::new();
    let user = split_args(&s.launch_options);
    let mut skip_next = false;
    for (i, a) in user.iter().enumerate() {
        if skip_next {
            skip_next = false;
            continue;
        }
        let lower = a.to_ascii_lowercase();
        if lower == "+connect" || lower == "+connect_lobby" {
            skip_next = i + 1 < user.len();
            continue;
        }
        if lower == "-insecure" || lower == "-applaunch" || lower == "-hijack" {
            continue;
        }
        args.push(a.clone());
    }
    if !args.iter().any(|a| a.eq_ignore_ascii_case("-condebug")) {
        args.push("-condebug".to_owned());
    }
    let insecure = match s.insecure {
        InsecureMode::Always => true,
        InsecureMode::Never => false,
        InsecureMode::Auto => server_vac == Some(false),
    };
    if insecure {
        args.push("-insecure".to_owned());
    }
    args.push("+connect".to_owned());
    args.push(format!("{host}:{port}"));
    args
}

pub fn build_local_args(s: &SnifferSettings, map: &str) -> Vec<String> {
    let mut args = build_args(s, "127.0.0.1", 27015, Some(false));
    let cut = args.iter().rposition(|a| a == "+connect").unwrap_or(args.len());
    args.truncate(cut);
    if !args.iter().any(|a| a.eq_ignore_ascii_case("-insecure")) {
        args.push("-insecure".to_owned());
    }
    for a in ["+sv_lan", "1", "+maxplayers", "2", "+map"] {
        args.push(a.to_owned());
    }
    args.push(map.to_owned());
    args
}

pub fn insecure_applies(s: &SnifferSettings, server_vac: Option<bool>) -> bool {
    match s.insecure {
        InsecureMode::Always => true,
        InsecureMode::Never => false,
        InsecureMode::Auto => server_vac == Some(false),
    }
}

pub fn steam_exe(install: &GmodInstall) -> Option<PathBuf> {
    if !cfg!(windows) {
        return None;
    }
    let out = Command::new("reg").args(["query", r"HKCU\Software\Valve\Steam", "/v", "SteamExe"]).output().ok();
    if let Some(out) = out {
        let text = String::from_utf8_lossy(&out.stdout).into_owned();
        for line in text.lines() {
            if let Some(pos) = line.find("REG_SZ") {
                let p = PathBuf::from(line[pos + 6..].trim().replace('/', "\\"));
                if p.is_file() {
                    return Some(p);
                }
            }
        }
    }
    let mut cur = install.root.as_path();
    while let Some(parent) = cur.parent() {
        let candidate = parent.join("steam.exe");
        if candidate.is_file() {
            return Some(candidate);
        }
        cur = parent;
    }
    let fallback = PathBuf::from(r"C:\Program Files (x86)\Steam\steam.exe");
    fallback.is_file().then_some(fallback)
}

pub fn percent_encode(s: &str) -> String {
    let mut out = String::new();
    for b in s.bytes() {
        if b.is_ascii_alphanumeric() || matches!(b, b'-' | b'_' | b'.' | b'~' | b'+' | b':') {
            out.push(b as char);
        } else {
            out.push_str(&format!("%{b:02X}"));
        }
    }
    out
}

pub fn launch_game(install: &GmodInstall, args: &[String]) -> Result<String, String> {
    if let Some(steam) = steam_exe(install) {
        let mut cmd = Command::new(&steam);
        cmd.arg("-applaunch").arg(GMOD_APP_ID.to_string()).args(args);
        hide_window(&mut cmd);
        cmd.spawn().map_err(|e| format!("cannot start Steam ({}): {e}", steam.display()))?;
        return Ok(format!("steam -applaunch {GMOD_APP_ID} {}", args.join(" ")));
    }
    let url = format!("steam://run/{GMOD_APP_ID}//{}/", percent_encode(&args.join(" ")));
    let mut cmd = Command::new("cmd");
    cmd.args(["/C", "start", "", &url]);
    hide_window(&mut cmd);
    cmd.spawn().map_err(|e| format!("cannot open the Steam link: {e}"))?;
    Ok(url)
}

pub fn hijack_command(install: &GmodInstall, command: &str) -> Result<(), String> {
    let mut cmd = Command::new(&install.exe);
    cmd.current_dir(&install.root).arg("-hijack").arg(command);
    hide_window(&mut cmd);
    cmd.spawn().map(|_| ()).map_err(|e| format!("cannot run {}: {e}", install.exe.display()))
}

pub fn confirms_disconnect(line: &str) -> bool {
    let l = line.trim().to_ascii_lowercase();
    l.starts_with("disconnect")
        || l.contains("disconnecting from")
        || l.contains("disconnected from")
        || l.contains("disconnect by user")
        || l.starts_with("restoring gamemode")
        || l.starts_with("lost connection")
        || l.contains("connection closed")
}

pub const DISCONNECT_ATTEMPTS: usize = 3;

pub fn disconnect_game(install: &GmodInstall, tail: &mut LogTail, wait: std::time::Duration) -> bool {
    let per_attempt = (wait / DISCONNECT_ATTEMPTS as u32).max(std::time::Duration::from_secs(4));
    for attempt in 0..DISCONNECT_ATTEMPTS {
        let command = if attempt + 1 == DISCONNECT_ATTEMPTS { "+disconnect +map_background" } else { "+disconnect" };
        if hijack_command(install, command).is_err() {
            continue;
        }
        let deadline = std::time::Instant::now() + per_attempt;
        while std::time::Instant::now() < deadline {
            if tail.poll().iter().any(|line| confirms_disconnect(line)) {
                return true;
            }
            std::thread::sleep(std::time::Duration::from_millis(300));
        }
    }
    false
}

#[cfg(windows)]
fn hide_window(cmd: &mut Command) {
    use std::os::windows::process::CommandExt;
    cmd.creation_flags(0x0800_0000);
}

#[cfg(not(windows))]
fn hide_window(_cmd: &mut Command) {}

#[cfg(windows)]
pub fn game_pids() -> Vec<u32> {
    let all = crate::procmem::find_processes(PROCESS_NAMES);
    let pids: HashSet<u32> = all.iter().map(|p| p.0).collect();
    all.iter().filter(|p| !pids.contains(&p.1)).map(|p| p.0).collect()
}

#[cfg(not(windows))]
pub fn game_pids() -> Vec<u32> {
    Vec::new()
}

pub fn same_path(a: &Path, b: &Path) -> bool {
    let norm = |p: &Path| p.to_string_lossy().replace('/', "\\").to_ascii_lowercase();
    norm(a) == norm(b)
}

#[cfg(windows)]
pub fn game_pids_for(install: &GmodInstall) -> Vec<u32> {
    let all = crate::procmem::find_processes(PROCESS_NAMES);
    let mine: Vec<(u32, u32)> = all
        .iter()
        .filter(|p| crate::procmem::process_image_path(p.0).is_none_or(|path| same_path(&path, &install.exe)))
        .map(|p| (p.0, p.1))
        .collect();
    let pids: HashSet<u32> = mine.iter().map(|p| p.0).collect();
    mine.iter().filter(|p| !pids.contains(&p.1)).map(|p| p.0).collect()
}

#[cfg(not(windows))]
pub fn game_pids_for(_install: &GmodInstall) -> Vec<u32> {
    Vec::new()
}

#[cfg(windows)]
pub fn child_pids(parent: u32) -> Vec<u32> {
    crate::procmem::find_processes(PROCESS_NAMES).into_iter().filter(|p| p.1 == parent).map(|p| p.0).collect()
}

#[cfg(not(windows))]
pub fn child_pids(_parent: u32) -> Vec<u32> {
    Vec::new()
}

#[cfg(windows)]
pub fn kill_game(pid: u32) -> bool {
    let children = child_pids(pid);
    for c in &children {
        crate::procmem::kill_pid(*c);
    }
    let ok = crate::procmem::kill_pid(pid);
    for _ in 0..40 {
        let alive = crate::procmem::is_running(pid) || children.iter().any(|c| crate::procmem::is_running(*c));
        if !alive {
            return true;
        }
        for c in child_pids(pid) {
            crate::procmem::kill_pid(c);
        }
        std::thread::sleep(std::time::Duration::from_millis(250));
    }
    ok && !crate::procmem::is_running(pid)
}

#[cfg(not(windows))]
pub fn kill_game(_pid: u32) -> bool {
    false
}

#[derive(Clone, Debug, Default, PartialEq, Eq)]
pub struct LogEvents {
    pub connecting: bool,
    pub connected: bool,
    pub map: Option<String>,
    pub problems: Vec<String>,
    pub lines: usize,
}

pub fn scan_log_lines(text: &str, events: &mut LogEvents) {
    for line in text.lines() {
        events.lines += 1;
        let l = line.trim();
        if l.starts_with("Connecting to ") {
            events.connecting = true;
        } else if l.starts_with("Connected to ") {
            events.connected = true;
        } else if let Some(m) = l.strip_prefix("Map: ") {
            events.map = Some(m.trim().to_owned());
        }
        let lower = l.to_ascii_lowercase();
        let bad = lower.contains("connection failed")
            || lower.contains("connect failed")
            || lower.starts_with("disconnect")
            || lower.contains("you have been kicked")
            || lower.contains("you have been banned")
            || lower.contains("vac") && (lower.contains("secure") || lower.contains("banned"))
            || lower.contains("server is full")
            || lower.contains("bad password")
            || lower.contains("connection to server lost");
        if bad && events.problems.len() < 20 {
            events.problems.push(l.chars().take(200).collect());
        }
    }
}

pub struct LogTail {
    path: PathBuf,
    offset: u64,
    pub events: LogEvents,
}

impl LogTail {
    pub fn new(path: PathBuf) -> LogTail {
        let offset = std::fs::metadata(&path).map(|m| m.len()).unwrap_or(0);
        LogTail { path, offset, events: LogEvents::default() }
    }

    pub fn poll(&mut self) -> Vec<String> {
        let Ok(mut f) = std::fs::File::open(&self.path) else { return Vec::new() };
        let len = f.metadata().map(|m| m.len()).unwrap_or(0);
        if len < self.offset {
            self.offset = 0;
        }
        if len == self.offset || f.seek(SeekFrom::Start(self.offset)).is_err() {
            return Vec::new();
        }
        let mut buf = Vec::new();
        let _ = f.take(len - self.offset).read_to_end(&mut buf);
        self.offset += buf.len() as u64;
        let text = String::from_utf8_lossy(&buf).into_owned();
        scan_log_lines(&text, &mut self.events);
        text.lines().map(|l| l.to_owned()).collect()
    }
}

pub fn pids_set(install: &GmodInstall) -> HashSet<u32> {
    game_pids_for(install).into_iter().collect()
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn a_local_game_starts_the_map_instead_of_connecting() {
        let s = SnifferSettings::default();
        let args = build_local_args(&s, "hpt_prot");
        assert!(!args.iter().any(|a| a == "+connect"));
        assert!(args.iter().any(|a| a == "-insecure"));
        assert!(args.iter().any(|a| a == "-condebug"));
        let at = args.iter().position(|a| a == "+map").expect("map");
        assert_eq!(args.get(at + 1).map(String::as_str), Some("hpt_prot"));
        assert!(args.windows(2).any(|w| w[0] == "+maxplayers" && w[1] == "2"));
    }

    #[test]
    fn arguments_are_split_like_a_shell() {
        assert_eq!(
            split_args("-windowed -w 800  +map \"gm construct\""),
            vec!["-windowed", "-w", "800", "+map", "gm construct"]
        );
        assert!(split_args("   ").is_empty());
        assert_eq!(split_args("\"\""), vec![""]);
    }

    #[test]
    fn the_command_line_always_joins_the_server() {
        let mut s = SnifferSettings::default();
        let args = build_args(&s, "1.2.3.4", 27016, Some(true));
        assert_eq!(&args[args.len() - 2..], ["+connect", "1.2.3.4:27016"]);
        assert!(args.contains(&"-condebug".to_owned()));
        assert!(!args.contains(&"-insecure".to_owned()), "secure server, auto mode");
        let insecure = build_args(&s, "1.2.3.4", 27016, Some(false));
        assert!(insecure.contains(&"-insecure".to_owned()));
        let unknown = build_args(&s, "1.2.3.4", 27016, None);
        assert!(!unknown.contains(&"-insecure".to_owned()));
        s.insecure = InsecureMode::Always;
        assert!(build_args(&s, "h", 1, Some(true)).contains(&"-insecure".to_owned()));
        s.insecure = InsecureMode::Never;
        assert!(!build_args(&s, "h", 1, Some(false)).contains(&"-insecure".to_owned()));
    }

    #[test]
    fn user_options_cannot_redirect_the_connection() {
        let s = SnifferSettings {
            launch_options: "-windowed +connect evil:1 -insecure -condebug -hijack +volume 0".to_owned(),
            insecure: InsecureMode::Never,
            ..SnifferSettings::default()
        };
        let args = build_args(&s, "good", 27015, Some(true));
        assert!(!args.iter().any(|a| a.contains("evil")));
        assert_eq!(args.iter().filter(|a| *a == "-condebug").count(), 1);
        assert!(!args.contains(&"-insecure".to_owned()));
        assert!(!args.contains(&"-hijack".to_owned()));
        assert!(args.contains(&"+volume".to_owned()));
        assert_eq!(args.last().map(String::as_str), Some("good:27015"));
    }

    #[test]
    fn the_console_log_is_followed() {
        let mut ev = LogEvents::default();
        scan_log_lines(
            "Connecting to 1.2.3.4:27015...\nConnected to 1.2.3.4:27015\n\nMap: rp_rockford_v2b\nPlayers: 3\nDisconnect: kicked by console\n",
            &mut ev,
        );
        assert!(ev.connecting && ev.connected);
        assert_eq!(ev.map.as_deref(), Some("rp_rockford_v2b"));
        assert_eq!(ev.problems.len(), 1);
        assert!(ev.problems[0].starts_with("Disconnect"));
    }

    #[test]
    fn the_log_tail_reads_only_new_text_and_survives_truncation() {
        let dir = std::env::temp_dir().join(format!("hp_sniffer_log_{}", std::process::id()));
        let _ = std::fs::remove_dir_all(&dir);
        std::fs::create_dir_all(&dir).expect("dir");
        let path = dir.join("console.log");
        std::fs::write(&path, "old line\nConnected to old\n").expect("write");
        let mut tail = LogTail::new(path.clone());
        assert!(tail.poll().is_empty());
        assert!(!tail.events.connected);
        std::fs::write(&path, "old line\nConnected to old\nMap: gm_flatgrass\n").expect("write");
        assert_eq!(tail.poll(), vec!["Map: gm_flatgrass"]);
        assert_eq!(tail.events.map.as_deref(), Some("gm_flatgrass"));
        std::fs::write(&path, "Connecting to x\n").expect("write");
        assert_eq!(tail.poll(), vec!["Connecting to x"]);
        assert!(tail.events.connecting);
        let _ = std::fs::remove_dir_all(&dir);
    }

    #[test]
    fn urls_are_encoded() {
        assert_eq!(percent_encode("-w 800 +connect 1.2.3.4:27015"), "-w%20800%20+connect%201.2.3.4:27015");
    }

    #[test]
    fn installs_are_recognised_by_their_files() {
        let dir = std::env::temp_dir().join(format!("hp_sniffer_install_{}", std::process::id()));
        let _ = std::fs::remove_dir_all(&dir);
        let root = dir.join("GarrysMod");
        std::fs::create_dir_all(root.join("garrysmod")).expect("dir");
        std::fs::create_dir_all(root.join("bin").join("win64")).expect("dir");
        assert!(install_at(&root).is_none());
        std::fs::write(root.join("hl2.exe"), b"x").expect("write");
        let x86 = install_at(&root).expect("32 bit");
        assert!(!x86.is_64bit);
        std::fs::write(root.join("bin").join("win64").join("gmod.exe"), b"x").expect("write");
        let x64 = install_at(&root.join("garrysmod")).expect("64 bit from the game folder");
        assert!(x64.is_64bit);
        assert_eq!(x64.root, root);
        assert_eq!(x64.console_log(), root.join("garrysmod").join("console.log"));
        assert!(detect_install(&dir.join("nothing").display().to_string()).is_err());
        let _ = std::fs::remove_dir_all(&dir);
    }

    #[cfg(windows)]
    #[test]
    fn only_the_given_process_is_closed() {
        let mut keep = std::process::Command::new("ping")
            .args(["-n", "60", "127.0.0.1"])
            .stdout(std::process::Stdio::null())
            .spawn()
            .expect("spawn");
        let mut victim = std::process::Command::new("ping")
            .args(["-n", "60", "127.0.0.1"])
            .stdout(std::process::Stdio::null())
            .spawn()
            .expect("spawn");
        assert!(crate::procmem::is_running(victim.id()));
        assert!(kill_game(victim.id()));
        assert!(!crate::procmem::is_running(victim.id()));
        assert!(crate::procmem::is_running(keep.id()), "other processes are left alone");
        let _ = victim.wait();
        let _ = keep.kill();
        let _ = keep.wait();
    }

    #[test]
    fn a_disconnect_is_recognised_in_the_console() {
        assert!(confirms_disconnect("Restoring gamemode to sandbox"));
        assert!(confirms_disconnect("Disconnect: kicked by console"));
        assert!(confirms_disconnect("  Disconnecting from server"));
        assert!(confirms_disconnect("Disconnect by user."));
        assert!(confirms_disconnect("Lost connection to server"));
        assert!(!confirms_disconnect("Le joueur Bob a rejoint la partie"));
        assert!(!confirms_disconnect("Connected to 1.2.3.4:27015"));
        assert!(!confirms_disconnect("Map: gm_flatgrass"));
    }
}
