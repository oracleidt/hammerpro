use std::path::{Path, PathBuf};
use std::time::SystemTime;

use serde::{Deserialize, Serialize};

use crate::stable_hash::StableHasher;

pub const DEFAULT_MAX_BYTES: u64 = 2 * 1024 * 1024 * 1024;
pub const MAX_ENTRY_BYTES: u64 = 768 * 1024 * 1024;

pub const VIS_CACHE_ARG: &str = "-viscache";

const VIS_EXT: &str = "hpvis";

#[derive(Debug, Clone, PartialEq, Eq, Default, Serialize, Deserialize)]
#[serde(default)]
pub struct PlanCache {
    pub dir: PathBuf,
    pub map_key: String,
    pub steps: Vec<(usize, String, u64)>,
    pub max_bytes: u64,
}

impl PlanCache {
    pub fn step(&self, index: usize) -> Option<(&str, u64)> {
        self.steps.iter().find(|(i, _, _)| *i == index).map(|(_, s, k)| (s.as_str(), *k))
    }

    fn entry(&self, stage: &str, key: u64, ext: &str) -> Option<PathBuf> {
        if !is_safe_name(&self.map_key) || !is_safe_name(stage) {
            return None;
        }
        Some(self.dir.join(&self.map_key).join(format!("{stage}-{key:016x}.{ext}")))
    }

    pub fn lookup(&self, stage: &str, key: u64) -> Option<(PathBuf, Option<PathBuf>)> {
        let bsp = self.entry(stage, key, "bsp")?;
        if !bsp.is_file() {
            return None;
        }
        let prt = self.entry(stage, key, "prt").filter(|p| p.is_file());
        Some((bsp, prt))
    }

    pub fn restore(&self, stage: &str, key: u64, bsp: &Path, prt: &Path) -> Option<PathBuf> {
        let (from_bsp, from_prt) = self.lookup(stage, key)?;
        copy_atomic(&from_bsp, bsp).ok()?;
        touch(bsp);
        if let Some(p) = from_prt {
            copy_atomic(&p, prt).ok()?;
            touch(prt);
        }
        touch(&from_bsp);
        Some(from_bsp)
    }

    pub fn store(&self, stage: &str, key: u64, bsp: &Path, prt: Option<&Path>) {
        let Some(to_bsp) = self.entry(stage, key, "bsp") else { return };
        let size = std::fs::metadata(bsp).map(|m| m.len()).unwrap_or(u64::MAX);
        if size > MAX_ENTRY_BYTES {
            return;
        }
        if let Some(dir) = to_bsp.parent()
            && std::fs::create_dir_all(dir).is_err()
        {
            return;
        }
        if copy_atomic(bsp, &to_bsp).is_err() {
            return;
        }
        if let Some(prt) = prt.filter(|p| p.is_file())
            && let Some(to_prt) = self.entry(stage, key, "prt")
        {
            let _ = copy_atomic(prt, &to_prt);
        }
        prune(&self.dir, if self.max_bytes == 0 { DEFAULT_MAX_BYTES } else { self.max_bytes });
    }

    pub fn store_stats(&self, stage: &str, key: u64, lines: &[String]) {
        let Some(path) = self.entry(stage, key, "stats") else { return };
        if lines.is_empty() || !path.with_extension("bsp").is_file() {
            return;
        }
        let tmp = path.with_extension("stats.hptmp");
        if std::fs::write(&tmp, lines.join("\n")).is_ok() && std::fs::rename(&tmp, &path).is_err() {
            let _ = std::fs::remove_file(&tmp);
        }
    }

    pub fn stats(&self, stage: &str, key: u64) -> Vec<String> {
        self.entry(stage, key, "stats")
            .and_then(|p| std::fs::read_to_string(p).ok())
            .map(|t| t.lines().map(str::to_owned).collect())
            .unwrap_or_default()
    }
}

fn is_safe_name(s: &str) -> bool {
    !s.is_empty()
        && s.len() <= 128
        && s.bytes().all(|b| b.is_ascii_alphanumeric() || matches!(b, b'_' | b'-' | b'.'))
        && !s.bytes().all(|b| b == b'.')
}

fn touch(path: &Path) {
    let _ = std::fs::File::options().write(true).open(path).and_then(|f| f.set_modified(SystemTime::now()));
}

fn copy_atomic(from: &Path, to: &Path) -> std::io::Result<()> {
    let name = to.file_name().map_or_else(|| "file".into(), |n| n.to_string_lossy().into_owned());
    let tmp = to.with_file_name(format!("{name}.hptmp"));
    if let Err(e) = std::fs::copy(from, &tmp) {
        let _ = std::fs::remove_file(&tmp);
        return Err(e);
    }
    std::fs::rename(&tmp, to).inspect_err(|_| {
        let _ = std::fs::remove_file(&tmp);
    })
}

pub fn prune(dir: &Path, max_bytes: u64) {
    let mut files: Vec<(SystemTime, u64, PathBuf)> = Vec::new();
    let Ok(maps) = std::fs::read_dir(dir) else { return };
    for map in maps.flatten().take(10_000) {
        let Ok(entries) = std::fs::read_dir(map.path()) else { continue };
        for e in entries.flatten().take(10_000) {
            let p = e.path();
            let ext_ok = p
                .extension()
                .is_some_and(|x| ["bsp", "prt", "stats", VIS_EXT].iter().any(|e| x.eq_ignore_ascii_case(e)));
            let Ok(meta) = e.metadata() else { continue };
            if ext_ok && meta.is_file() {
                files.push((meta.modified().unwrap_or(SystemTime::UNIX_EPOCH), meta.len(), p));
            }
        }
    }
    let mut total: u64 = files.iter().map(|f| f.1).sum();
    if total <= max_bytes {
        return;
    }
    files.sort_by_key(|f| f.0);
    for (_, len, path) in files {
        if total <= max_bytes {
            break;
        }
        if std::fs::remove_file(&path).is_ok() {
            total = total.saturating_sub(len);
        }
    }
}

pub fn key_args(args: &[String], map_arg: &str, game_dir: &str) -> Vec<String> {
    let mut out = Vec::with_capacity(args.len());
    let mut i = 0;
    while let Some(a) = args.get(i) {
        let lower = a.to_ascii_lowercase();
        match lower.as_str() {
            "-low" | "-leaktest" => {}
            "-threads" | VIS_CACHE_ARG => i += 1,
            _ if a == map_arg => out.push("$map".to_owned()),
            _ if a == game_dir => out.push("$game".to_owned()),
            _ => out.push(a.clone()),
        }
        i += 1;
    }
    out
}

pub fn stage_key(input: u64, exe: &Path, args: &[String]) -> u64 {
    let mut h = StableHasher::new();
    std::hash::Hasher::write_u64(&mut h, input);
    h.str(&exe.to_string_lossy());
    if let Some((size, mtime)) = crate::incremental::file_stamp(exe) {
        std::hash::Hasher::write_u64(&mut h, size);
        std::hash::Hasher::write_u64(&mut h, mtime);
    }
    for a in args {
        h.str(a);
    }
    h.finish_u64()
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::test_util::scratch;

    #[test]
    fn store_lookup_restore_and_prune() {
        let dir = scratch("cache_basic");
        let cache = PlanCache {
            dir: dir.join("builds"),
            map_key: "gm_x_1234".into(),
            steps: vec![(0, "bsp".into(), 42)],
            max_bytes: 0,
        };
        assert_eq!(cache.step(0), Some(("bsp", 42)));
        assert_eq!(cache.step(1), None);
        assert!(cache.lookup("bsp", 42).is_none());
        let bsp = dir.join("m.bsp");
        let prt = dir.join("m.prt");
        std::fs::write(&bsp, b"BSP-1").unwrap();
        std::fs::write(&prt, b"PRT-1").unwrap();
        cache.store("bsp", 42, &bsp, Some(&prt));
        assert!(cache.lookup("bsp", 42).is_some_and(|(_, p)| p.is_some()));
        std::fs::write(&bsp, b"changed").unwrap();
        std::fs::remove_file(&prt).unwrap();
        let old = SystemTime::now() - std::time::Duration::from_secs(3600);
        for e in cache.lookup("bsp", 42).map(|(b, p)| [Some(b), p]).into_iter().flatten().flatten() {
            std::fs::File::options().write(true).open(&e).unwrap().set_modified(old).unwrap();
        }
        let before = SystemTime::now() - std::time::Duration::from_secs(2);
        assert!(cache.restore("bsp", 42, &bsp, &prt).is_some());
        for p in [&bsp, &prt] {
            let m = std::fs::metadata(p).unwrap().modified().unwrap();
            assert!(m >= before, "a restored file counts as written now, so the views reload it: {p:?}");
        }
        assert!(cache.stats("bsp", 42).is_empty());
        cache.store_stats("bsp", 42, &["---- Map statistics ----".to_owned(), "#1  x".to_owned()]);
        assert_eq!(cache.stats("bsp", 42), ["---- Map statistics ----", "#1  x"]);
        assert_eq!(std::fs::read(&bsp).unwrap(), b"BSP-1");
        assert_eq!(std::fs::read(&prt).unwrap(), b"PRT-1");
        let bad = PlanCache { map_key: "../x".into(), ..cache.clone() };
        bad.store("bsp", 1, &bsp, None);
        assert!(bad.lookup("bsp", 1).is_none());
        assert!(!dir.join("x").exists());
        for k in 0..5u64 {
            std::fs::write(&bsp, vec![b'x'; 1000]).unwrap();
            cache.store("rad", k, &bsp, None);
            std::thread::sleep(std::time::Duration::from_millis(20));
        }
        prune(&dir.join("builds"), 2500);
        let left: Vec<u64> = (0..5).filter(|k| cache.lookup("rad", *k).is_some()).collect();
        assert!(left.len() <= 2 && left.contains(&4), "{left:?}");
    }

    #[test]
    fn keys_ignore_speed_flags_and_paths() {
        let a: Vec<String> =
            ["-leaktest", "-low", "-threads", "8", "-viscache", "C:/c/m", "-game", "C:/g", "C:/maps/.b/m"]
                .iter()
                .map(|s| (*s).to_owned())
                .collect();
        let b: Vec<String> = ["-game", "D:/other", "D:/cache/spec/m"].iter().map(|s| (*s).to_owned()).collect();
        assert_eq!(key_args(&a, "C:/maps/.b/m", "C:/g"), key_args(&b, "D:/cache/spec/m", "D:/other"));
        assert_eq!(key_args(&a, "C:/maps/.b/m", "C:/g"), ["-game", "$game", "$map"]);
        let exe = Path::new("Z:/nope/hammerpro.exe");
        assert_eq!(stage_key(1, exe, &b), stage_key(1, exe, &b));
        assert_ne!(stage_key(1, exe, &b), stage_key(2, exe, &b));
        assert_ne!(stage_key(1, exe, &b), stage_key(1, exe, &a));
    }
}
