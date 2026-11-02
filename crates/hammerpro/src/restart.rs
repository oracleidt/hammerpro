use std::path::{Path, PathBuf};

pub const RESTORE_ARG: &str = "--restore-session";
const FILE: &str = "restart-session.txt";

#[derive(Clone, Debug, PartialEq, Eq)]
pub enum Entry {
    Map(PathBuf),
    Recovered { save: PathBuf, original: Option<PathBuf> },
}

impl Entry {
    pub fn path(&self) -> &Path {
        match self {
            Entry::Map(p) => p,
            Entry::Recovered { save, .. } => save,
        }
    }
}

pub fn to_text(entries: &[Entry]) -> String {
    let mut out = String::new();
    for e in entries {
        match e {
            Entry::Map(p) => out.push_str(&format!("map\t{}\n", p.display())),
            Entry::Recovered { save, original } => out.push_str(&format!(
                "recovered\t{}\t{}\n",
                save.display(),
                original.as_ref().map(|p| p.display().to_string()).unwrap_or_default()
            )),
        }
    }
    out
}

pub fn parse(text: &str) -> Vec<Entry> {
    text.lines()
        .filter_map(|line| {
            let mut parts = line.split('\t');
            let kind = parts.next()?;
            let first = parts.next().map(str::trim).filter(|p| !p.is_empty()).map(PathBuf::from)?;
            match kind {
                "map" => Some(Entry::Map(first)),
                "recovered" => {
                    let original = parts.next().map(str::trim).filter(|p| !p.is_empty()).map(PathBuf::from);
                    Some(Entry::Recovered { save: first, original })
                }
                _ => None,
            }
        })
        .collect()
}

pub fn write(dir: &Path, entries: &[Entry]) -> std::io::Result<PathBuf> {
    std::fs::create_dir_all(dir)?;
    let path = dir.join(FILE);
    std::fs::write(&path, to_text(entries))?;
    Ok(path)
}

pub fn take(path: &Path) -> Vec<Entry> {
    let entries = std::fs::read_to_string(path).map(|t| parse(&t)).unwrap_or_default();
    let _ = std::fs::remove_file(path);
    entries.into_iter().filter(|e| e.path().is_file()).collect()
}

pub fn relaunch(entries: &[Entry]) {
    let list = match write(&hp_app::settings::Settings::crash_dir(), entries) {
        Ok(p) => p,
        Err(e) => {
            log::error!("cannot write the list of maps to open again: {e}");
            return;
        }
    };
    let exe = match std::env::current_exe() {
        Ok(e) => e,
        Err(e) => {
            log::error!("cannot find the Hammer Pro program to restart it: {e}");
            return;
        }
    };
    match std::process::Command::new(exe).arg(RESTORE_ARG).arg(&list).spawn() {
        Ok(child) => log::info!("Hammer Pro restarts (process {})", child.id()),
        Err(e) => log::error!("cannot restart Hammer Pro: {e}"),
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn session_lists_round_trip_and_skip_missing_files() {
        let dir = std::env::temp_dir().join(format!("hammerpro_restart_{}", std::process::id()));
        let _ = std::fs::remove_dir_all(&dir);
        std::fs::create_dir_all(&dir).unwrap();
        let map = dir.join("room.vmf");
        let save = dir.join("room_emergency.vmf");
        std::fs::write(&map, "versioninfo {}").unwrap();
        std::fs::write(&save, "versioninfo {}").unwrap();
        let entries = vec![
            Entry::Recovered { save: save.clone(), original: Some(map.clone()) },
            Entry::Map(map.clone()),
            Entry::Recovered { save: save.clone(), original: None },
            Entry::Map(dir.join("gone.vmf")),
        ];
        assert_eq!(parse(&to_text(&entries)), entries);
        let file = write(&dir, &entries).unwrap();
        let back = take(&file);
        assert_eq!(back, entries[..3].to_vec(), "missing maps are left out");
        assert!(!file.exists(), "the list is used once");
        assert!(parse("junk\nmap\t\nrecovered\t\n").is_empty());
        let _ = std::fs::remove_dir_all(&dir);
    }
}
