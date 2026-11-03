use std::path::{Component, Path, PathBuf};

use crate::settings::Settings;

#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash)]
pub enum Root {
    Config,
    Cache,
    Autosave,
    Logs,
    Crash,
    Temp,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq, PartialOrd, Ord)]
pub enum Risk {
    Safe,
    Recovery,
    UserData,
}

impl Risk {
    pub fn id(self) -> &'static str {
        match self {
            Risk::Safe => "safe",
            Risk::Recovery => "recovery",
            Risk::UserData => "user_data",
        }
    }
}

#[derive(Clone, Copy, Debug)]
pub enum Filter {
    All,
    Prefixes(&'static [&'static str]),
    Unclaimed,
}

#[derive(Clone, Copy, Debug)]
pub struct Part {
    pub root: Root,
    pub rel: &'static str,
    pub filter: Filter,
}

#[derive(Clone, Copy, Debug)]
pub struct Spec {
    pub id: &'static str,
    pub name_key: &'static str,
    pub desc_key: &'static str,
    pub risk: Risk,
    pub uses_compile: bool,
    pub parts: &'static [Part],
}

const fn dir(root: Root, rel: &'static str) -> Part {
    Part { root, rel, filter: Filter::All }
}

const TEMP_ROW_PREFIXES: &[&str] = &["hammerpro_", "hammerpro-light", ".hammerpro_build"];
const UPDATE_PREFIXES: &[&str] = &["hammerpro-update"];
const INDEX_PREFIXES: &[&str] = &["content_index"];
const SETTINGS_PREFIXES: &[&str] = &["settings"];
const RESERVED_CACHE_NAMES: &[&str] = &["mcp"];

pub const TABLE: &[Spec] = &[
    Spec {
        id: "settings",
        name_key: "storage.row.settings",
        desc_key: "storage.row.settings.desc",
        risk: Risk::UserData,
        uses_compile: false,
        parts: &[Part { root: Root::Config, rel: "", filter: Filter::Prefixes(SETTINGS_PREFIXES) }],
    },
    Spec {
        id: "prefabs",
        name_key: "storage.row.prefabs",
        desc_key: "storage.row.prefabs.desc",
        risk: Risk::UserData,
        uses_compile: false,
        parts: &[dir(Root::Config, "prefabs")],
    },
    Spec {
        id: "sprinkle",
        name_key: "storage.row.sprinkle",
        desc_key: "storage.row.sprinkle.desc",
        risk: Risk::UserData,
        uses_compile: false,
        parts: &[dir(Root::Config, "sprinkle")],
    },
    Spec {
        id: "map_state",
        name_key: "storage.row.map_state",
        desc_key: "storage.row.map_state.desc",
        risk: Risk::UserData,
        uses_compile: false,
        parts: &[dir(Root::Cache, "maps")],
    },
    Spec {
        id: "autosave",
        name_key: "storage.row.autosave",
        desc_key: "storage.row.autosave.desc",
        risk: Risk::Recovery,
        uses_compile: false,
        parts: &[dir(Root::Autosave, "")],
    },
    Spec {
        id: "crash",
        name_key: "storage.row.crash",
        desc_key: "storage.row.crash.desc",
        risk: Risk::Recovery,
        uses_compile: false,
        parts: &[dir(Root::Crash, "")],
    },
    Spec {
        id: "logs",
        name_key: "storage.row.logs",
        desc_key: "storage.row.logs.desc",
        risk: Risk::Safe,
        uses_compile: false,
        parts: &[dir(Root::Logs, "")],
    },
    Spec {
        id: "thumbnails",
        name_key: "storage.row.thumbnails",
        desc_key: "storage.row.thumbnails.desc",
        risk: Risk::Safe,
        uses_compile: false,
        parts: &[dir(Root::Cache, "thumbs"), dir(Root::Cache, "prefab_thumbs")],
    },
    Spec {
        id: "pack_cache",
        name_key: "storage.row.pack_cache",
        desc_key: "storage.row.pack_cache.desc",
        risk: Risk::Safe,
        uses_compile: true,
        parts: &[dir(Root::Cache, "pack")],
    },
    Spec {
        id: "compile_content",
        name_key: "storage.row.compile_content",
        desc_key: "storage.row.compile_content.desc",
        risk: Risk::Safe,
        uses_compile: true,
        parts: &[dir(Root::Cache, "compile_content")],
    },
    Spec {
        id: "builds",
        name_key: "storage.row.builds",
        desc_key: "storage.row.builds.desc",
        risk: Risk::Safe,
        uses_compile: true,
        parts: &[dir(Root::Cache, "builds")],
    },
    Spec {
        id: "compile_temp",
        name_key: "storage.row.compile_temp",
        desc_key: "storage.row.compile_temp.desc",
        risk: Risk::Safe,
        uses_compile: true,
        parts: &[
            dir(Root::Cache, "spec"),
            dir(Root::Cache, "untitled"),
            dir(Root::Cache, "progressive"),
            dir(Root::Cache, "portals"),
        ],
    },
    Spec {
        id: "compile_history",
        name_key: "storage.row.compile_history",
        desc_key: "storage.row.compile_history.desc",
        risk: Risk::Safe,
        uses_compile: true,
        parts: &[dir(Root::Cache, "compile_history")],
    },
    Spec {
        id: "release_builds",
        name_key: "storage.row.release_builds",
        desc_key: "storage.row.release_builds.desc",
        risk: Risk::Safe,
        uses_compile: true,
        parts: &[dir(Root::Cache, "release")],
    },
    Spec {
        id: "decompiled",
        name_key: "storage.row.decompiled",
        desc_key: "storage.row.decompiled.desc",
        risk: Risk::Safe,
        uses_compile: false,
        parts: &[dir(Root::Cache, "decompiled"), dir(Root::Cache, "propsrc")],
    },
    Spec {
        id: "sniffer",
        name_key: "storage.row.sniffer",
        desc_key: "storage.row.sniffer.desc",
        risk: Risk::UserData,
        uses_compile: false,
        parts: &[dir(Root::Cache, "sniffer")],
    },
    Spec {
        id: "content_index",
        name_key: "storage.row.content_index",
        desc_key: "storage.row.content_index.desc",
        risk: Risk::Safe,
        uses_compile: false,
        parts: &[Part { root: Root::Cache, rel: "", filter: Filter::Prefixes(INDEX_PREFIXES) }],
    },
    Spec {
        id: "updates",
        name_key: "storage.row.updates",
        desc_key: "storage.row.updates.desc",
        risk: Risk::Safe,
        uses_compile: false,
        parts: &[Part { root: Root::Temp, rel: "", filter: Filter::Prefixes(UPDATE_PREFIXES) }],
    },
    Spec {
        id: "temp",
        name_key: "storage.row.temp",
        desc_key: "storage.row.temp.desc",
        risk: Risk::Safe,
        uses_compile: false,
        parts: &[Part { root: Root::Temp, rel: "", filter: Filter::Prefixes(TEMP_ROW_PREFIXES) }],
    },
    Spec {
        id: "other_cache",
        name_key: "storage.row.other_cache",
        desc_key: "storage.row.other_cache.desc",
        risk: Risk::Safe,
        uses_compile: false,
        parts: &[Part { root: Root::Cache, rel: "", filter: Filter::Unclaimed }],
    },
];

pub fn find_spec(id: &str) -> Option<&'static Spec> {
    TABLE.iter().find(|s| s.id == id)
}

#[derive(Clone, Debug, Default, PartialEq, Eq)]
pub struct AppDirs {
    pub config: PathBuf,
    pub cache: PathBuf,
    pub autosave: PathBuf,
    pub logs: PathBuf,
    pub crash: PathBuf,
    pub temp: PathBuf,
}

fn owned(p: &Path) -> bool {
    p.components().any(|c| c.as_os_str() == "HammerPro")
}

impl AppDirs {
    pub fn current() -> AppDirs {
        let keep = |p: PathBuf| if owned(&p) { p } else { PathBuf::new() };
        AppDirs {
            config: keep(Settings::config_dir()),
            cache: keep(Settings::cache_dir()),
            autosave: keep(Settings::autosave_dir()),
            logs: keep(Settings::log_dir()),
            crash: keep(Settings::crash_dir()),
            temp: std::env::temp_dir(),
        }
    }

    pub fn root(&self, root: Root) -> &Path {
        match root {
            Root::Config => &self.config,
            Root::Cache => &self.cache,
            Root::Autosave => &self.autosave,
            Root::Logs => &self.logs,
            Root::Crash => &self.crash,
            Root::Temp => &self.temp,
        }
    }

    pub fn part_dir(&self, part: &Part) -> PathBuf {
        let root = self.root(part.root);
        if root.as_os_str().is_empty() {
            return PathBuf::new();
        }
        if part.rel.is_empty() { root.to_path_buf() } else { root.join(part.rel) }
    }

    fn claimed(&self) -> (Vec<String>, Vec<&'static str>) {
        let mut names: Vec<String> = RESERVED_CACHE_NAMES.iter().map(|s| (*s).to_owned()).collect();
        let mut prefixes: Vec<&'static str> = Vec::new();
        for part in TABLE.iter().flat_map(|s| s.parts.iter()) {
            if let (Root::Cache, "", Filter::Prefixes(p)) = (part.root, part.rel, part.filter) {
                prefixes.extend(p.iter().copied());
                continue;
            }
            if matches!(part.filter, Filter::Unclaimed) {
                continue;
            }
            let full = self.part_dir(part);
            if full.as_os_str().is_empty() {
                continue;
            }
            if let Ok(rel) = full.strip_prefix(&self.cache)
                && let Some(first) = rel.components().next()
            {
                names.push(first.as_os_str().to_string_lossy().into_owned());
            }
        }
        (names, prefixes)
    }

    pub fn list_part(&self, part: &Part) -> Vec<PathBuf> {
        let dir = self.part_dir(part);
        if dir.as_os_str().is_empty() {
            return Vec::new();
        }
        let Ok(read) = std::fs::read_dir(&dir) else { return Vec::new() };
        let (claimed, claimed_prefixes) =
            if matches!(part.filter, Filter::Unclaimed) { self.claimed() } else { (Vec::new(), Vec::new()) };
        let mut out = Vec::new();
        for entry in read.flatten() {
            let name = entry.file_name().to_string_lossy().into_owned();
            let keep = match part.filter {
                Filter::All => true,
                Filter::Prefixes(p) => {
                    let lower = name.to_ascii_lowercase();
                    p.iter().any(|x| lower.starts_with(x))
                }
                Filter::Unclaimed => {
                    let lower = name.to_ascii_lowercase();
                    !claimed.iter().any(|c| c.eq_ignore_ascii_case(&name))
                        && !claimed_prefixes.iter().any(|p| lower.starts_with(p))
                }
            };
            if keep {
                out.push(entry.path());
            }
        }
        out.sort();
        out
    }

    pub fn guard(&self, root: Root, path: &Path) -> Result<PathBuf, String> {
        let base = self.root(root);
        if base.as_os_str().is_empty() {
            return Err("this location is not available".to_owned());
        }
        if !path.is_absolute() || path.components().any(|c| matches!(c, Component::ParentDir | Component::CurDir)) {
            return Err("not an absolute plain path".to_owned());
        }
        let name = path.file_name().ok_or("the path has no name")?;
        let base = base.canonicalize().map_err(|e| format!("the folder cannot be resolved: {e}"))?;
        if base.parent().is_none() || base.components().count() < 3 {
            return Err("refusing to clean a drive or top level folder".to_owned());
        }
        let parent = path.parent().ok_or("the path has no parent")?;
        let parent = parent.canonicalize().map_err(|e| format!("the parent folder cannot be resolved: {e}"))?;
        let inside = parent.starts_with(&base);
        if !inside {
            return Err("outside the folders of Hammer Pro".to_owned());
        }
        if root == Root::Temp {
            let lower = name.to_string_lossy().to_ascii_lowercase();
            let known = TABLE
                .iter()
                .flat_map(|s| s.parts.iter())
                .filter(|p| p.root == Root::Temp)
                .any(|p| matches!(p.filter, Filter::Prefixes(list) if list.iter().any(|x| lower.starts_with(x))));
            if parent != base || !known {
                return Err("not a Hammer Pro temporary item".to_owned());
            }
        }
        Ok(parent.join(name))
    }
}

pub fn in_use(root: Root, path: &Path) -> Option<&'static str> {
    let name = path.file_name()?.to_string_lossy().to_ascii_lowercase();
    match root {
        Root::Logs if name == "hammerpro.log" => Some("the log being written"),
        Root::Crash if name.starts_with("running-") => Some("a running session marker"),
        Root::Temp => {
            let pid = std::process::id();
            let tagged = |sep: &str| name.contains(&format!("{sep}{pid}-")) || name.ends_with(&format!("{sep}{pid}"));
            if tagged("-") || tagged("_") { Some("used by this session") } else { None }
        }
        _ => None,
    }
}
