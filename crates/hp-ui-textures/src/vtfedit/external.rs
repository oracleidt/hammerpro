use std::path::{Path, PathBuf};
use std::time::SystemTime;

pub use hp_app::vtf_external::{
    ExternalEditor, detect, detect_if_ready, detect_uncached, exe_of_command, launch, parse_reg_value, rescan, resolve,
    resolve_nonblocking,
};

type Stamp = Option<(u64, SystemTime)>;

fn stamp(path: &Path) -> Stamp {
    let meta = std::fs::metadata(path).ok()?;
    Some((meta.len(), meta.modified().ok()?))
}

#[derive(Clone, Debug)]
pub struct Session {
    pub texture: String,
    pub work: PathBuf,
    pub dest: PathBuf,
    pub materials: Vec<String>,
    pub editor: String,
    pub synced: u32,
    pub error: Option<String>,
    pub link: Option<(String, String)>,
    stamp: Stamp,
    settle: u8,
}

impl Session {
    pub fn new(texture: &str, work: PathBuf, dest: PathBuf, materials: Vec<String>, editor: &str) -> Self {
        let stamp = stamp(&work);
        Self {
            texture: texture.to_owned(),
            work,
            dest,
            materials,
            editor: editor.to_owned(),
            synced: 0,
            error: None,
            link: None,
            stamp,
            settle: 0,
        }
    }

    pub fn is_in_place(&self) -> bool {
        crate::util::same_path(&self.work, &self.dest)
    }

    pub fn changed(&mut self) -> bool {
        let now = stamp(&self.work);
        if now == self.stamp {
            if self.settle > 0 {
                self.settle -= 1;
                return self.settle == 0;
            }
            return false;
        }
        self.stamp = now;
        self.settle = 2;
        false
    }

    pub fn copy_back(&mut self) -> Result<bool, String> {
        let data = std::fs::read(&self.work).map_err(|e| e.to_string())?;
        if !hp_vtf::is_vtf(&data) {
            return Err(hp_app::t!("vtf.err.not_vtf").to_owned());
        }
        hp_vtf::read_info(&data).map_err(|e| e.to_string())?;
        self.synced += 1;
        if self.is_in_place() {
            return Ok(false);
        }
        super::store::write_bytes_with_backup(&self.dest, &data).map_err(|e| e.to_string())?;
        Ok(true)
    }
}

pub fn work_dir() -> PathBuf {
    hp_app::settings::Settings::cache_dir().join("vtf-edit")
}

pub fn work_file(texture: &str) -> PathBuf {
    let mut p = work_dir();
    for part in texture.split('/').filter(|s| !s.is_empty() && *s != "..") {
        p.push(part);
    }
    let mut s = p.into_os_string();
    s.push(".vtf");
    PathBuf::from(s)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn work_files_stay_inside() {
        let p = work_file("../../evil/brick/wall");
        assert!(p.starts_with(work_dir()), "{p:?}");
        assert!(p.extension().is_some_and(|e| e == "vtf"));
    }

    #[test]
    fn sessions_see_changes_after_they_settle() {
        let dir = std::env::temp_dir().join("hp-vtf-session-test");
        let _ = std::fs::create_dir_all(&dir);
        let f = dir.join("a.vtf");
        std::fs::write(&f, b"x").unwrap();
        let mut s = Session::new("a", f.clone(), f.clone(), Vec::new(), "t");
        assert!(!s.changed());
        std::fs::write(&f, b"yy").unwrap();
        assert!(!s.changed());
        assert!(!s.changed());
        assert!(s.changed());
        assert!(!s.changed());
        let _ = std::fs::remove_file(&f);
    }
}
