use std::path::{Path, PathBuf};
use std::sync::OnceLock;

use ahash::AHashMap;
use parking_lot::Mutex;

use crate::error::FsError;
use crate::path::{normalize_path, under_prefix};

pub(crate) const ASSET_DIRS: &[&str] =
    &["materials", "models", "sound", "particles", "maps", "resource", "scripts", "scenes"];

pub(crate) const MAX_DEPTH: u32 = 48;

pub(crate) const MAX_LINKS: u32 = 4;

pub(crate) fn walk_files(
    root: &Path,
    top_dirs: Option<&[&str]>,
    keep_file: &(dyn Fn(&str) -> bool + Sync),
    profile: Option<&str>,
) -> Result<Vec<String>, FsError> {
    crate::scan::walk(root, top_dirs, keep_file, profile)
}

pub(crate) const PROFILE_GAME: &str = "game";

pub(crate) const PROFILE_MATERIALS: &str = "materials";

type CaseChildren = AHashMap<String, PathBuf>;

#[derive(Debug)]
pub(crate) struct DirIndex {
    root: PathBuf,
    files: OnceLock<AHashMap<Box<str>, Box<str>>>,
    lookups: Mutex<AHashMap<String, Option<PathBuf>>>,
    case_dirs: Mutex<AHashMap<PathBuf, CaseChildren>>,
}

impl DirIndex {
    pub(crate) fn build_game_dir(root: &Path) -> Result<Self, FsError> {
        let rels = walk_files(root, Some(ASSET_DIRS), &|_| true, Some(PROFILE_GAME))?;
        Ok(Self::from_rels(root, rels))
    }

    pub(crate) fn from_rels(root: &Path, rels: Vec<String>) -> Self {
        let mut files = AHashMap::with_capacity(rels.len());
        for rel in rels {
            let key: Box<str> = normalize_path(&rel).into();
            files.entry(key).or_insert_with(|| rel.into_boxed_str());
        }
        Self {
            root: root.to_path_buf(),
            files: OnceLock::from(files),
            lookups: Mutex::default(),
            case_dirs: Mutex::default(),
        }
    }

    pub(crate) fn for_lookup(root: &Path) -> Self {
        Self {
            root: root.to_path_buf(),
            files: OnceLock::new(),
            lookups: Mutex::default(),
            case_dirs: Mutex::default(),
        }
    }

    pub(crate) fn root(&self) -> &Path {
        &self.root
    }

    pub(crate) fn len(&self) -> usize {
        self.files.get().map_or(0, |files| files.len())
    }

    #[inline]
    pub(crate) fn contains(&self, vpath: &str) -> bool {
        self.files.get().map_or_else(|| self.disk_path(vpath).is_some(), |files| files.contains_key(vpath))
    }

    pub(crate) fn disk_path(&self, vpath: &str) -> Option<PathBuf> {
        if let Some(files) = self.files.get() {
            return files.get(vpath).map(|rel| self.root.join(&**rel));
        }
        if let Some(hit) = self.lookups.lock().get(vpath) {
            return hit.clone();
        }
        let resolved = self.resolve(vpath);
        self.lookups.lock().insert(vpath.to_owned(), resolved.clone());
        resolved
    }

    fn resolve(&self, vpath: &str) -> Option<PathBuf> {
        let parts: Vec<_> = vpath.split('/').collect();
        if !parts.first().is_some_and(|p| ASSET_DIRS.iter().any(|d| d.eq_ignore_ascii_case(p)))
            || parts.iter().any(|p| p.is_empty() || *p == "." || *p == ".." || p.contains(['\\', ':']))
            || parts.len() > MAX_DEPTH as usize + 1
        {
            return None;
        }
        let direct = self.root.join(vpath);
        if direct.is_file() {
            return Some(direct);
        }
        let mut path = self.root.clone();
        for part in parts {
            path = self.case_child(&path, part)?;
        }
        path.is_file().then_some(path)
    }

    fn case_child(&self, directory: &Path, name: &str) -> Option<PathBuf> {
        let mut directories = self.case_dirs.lock();
        let children = directories.entry(directory.to_path_buf()).or_insert_with(|| {
            let mut children = CaseChildren::new();
            if let Ok(read) = std::fs::read_dir(directory) {
                for entry in read.flatten() {
                    if let Ok(name) = entry.file_name().into_string() {
                        children.entry(name.to_ascii_lowercase()).or_insert_with(|| entry.path());
                    }
                }
            }
            children
        });
        children.get(&name.to_ascii_lowercase()).cloned()
    }

    pub(crate) fn for_each_path<'a>(&'a self, prefix: &str, ext: Option<&str>, f: &mut dyn FnMut(&'a str)) {
        let files = self.files.get_or_init(|| {
            walk_files(&self.root, Some(ASSET_DIRS), &|_| true, Some(PROFILE_GAME))
                .map(|rels| Self::from_rels(&self.root, rels).files.into_inner().unwrap_or_default())
                .unwrap_or_default()
        });
        for key in files.keys() {
            if under_prefix(key, prefix) && ext.is_none_or(|e| crate::path::extension(key) == e) {
                f(key);
            }
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::test_util::TempDir;
    use std::fs;

    #[test]
    fn indexes_asset_dirs_case_insensitively() {
        let tmp = TempDir::new("dirindex");
        tmp.write("Materials/Concrete/Floor01.VMT", b"x");
        tmp.write("materials/concrete/sub/a.vtf", b"y");
        tmp.write("models/props/crate.mdl", b"m");
        tmp.write("lua/autorun/x.lua", b"l");
        tmp.write("readme.txt", b"r");
        let index = DirIndex::build_game_dir(tmp.path()).unwrap();
        assert_eq!(index.len(), 3);
        assert!(index.contains("materials/concrete/floor01.vmt"));
        assert!(index.contains("materials/concrete/sub/a.vtf"));
        assert!(!index.contains("lua/autorun/x.lua"));
        assert!(!index.contains("readme.txt"));
        let disk = index.disk_path("materials/concrete/floor01.vmt").unwrap();
        assert_eq!(fs::read(disk).unwrap(), b"x");
        let mut vmts = Vec::new();
        index.for_each_path("materials", Some("vmt"), &mut |p| vmts.push(p.to_owned()));
        assert_eq!(vmts, ["materials/concrete/floor01.vmt"]);
        let mut all = Vec::new();
        index.for_each_path("", None, &mut |p| all.push(p));
        assert_eq!(all.len(), 3);
    }

    #[test]
    fn targeted_reads_do_not_walk_or_index_unrelated_content() {
        let tmp = TempDir::new("lookup-only");
        tmp.write("Materials/Mixed/Smoke.VMT", b"smoke");
        tmp.write("models/props/crate.mdl", b"model");
        tmp.write("materials/unrelated/nested/unused.vmt", b"unused");
        tmp.write("lua/autorun/x.lua", b"lua");
        let index = DirIndex::for_lookup(tmp.path());
        for path in ["materials/mixed/smoke.vmt", "models/props/crate.mdl"] {
            assert!(index.contains(path));
            assert!(index.disk_path(path).unwrap().is_file());
        }
        for path in ["materials/missing.vmt", "lua/autorun/x.lua", "materials/../readme.txt"] {
            assert!(!index.contains(path));
        }
        assert!(index.files.get().is_none(), "reads never enumerate the full content tree");
        let mut listed = Vec::new();
        index.for_each_path("materials", Some("vmt"), &mut |p| listed.push(p.to_owned()));
        listed.sort();
        assert_eq!(listed, ["materials/mixed/smoke.vmt", "materials/unrelated/nested/unused.vmt"]);
        assert!(index.files.get().is_some(), "enumeration still builds a complete index on demand");
    }

    #[test]
    fn different_missing_names_reuse_the_requested_directory_snapshot() {
        let tmp = TempDir::new("lookup-misses");
        tmp.write("Materials/Flat/Existing.VMT", b"existing");
        tmp.write("Materials/Other/Unrelated.VMT", b"unrelated");
        let index = DirIndex::for_lookup(tmp.path());
        assert!(!index.contains("materials/flat/missing0.vmt"));
        tmp.write("Materials/Flat/Added.VMT", b"added after the directory was listed");
        for i in 1..100 {
            assert!(!index.contains(&format!("materials/flat/missing{i}.vmt")));
        }
        let directories = index.case_dirs.lock();
        assert_eq!(directories.len(), 3);
        let flat = directories.get(&tmp.path().join("Materials").join("Flat")).unwrap();
        assert!(flat.contains_key("existing.vmt"));
        assert!(!flat.contains_key("added.vmt"), "different misses must not enumerate the directory again");
        assert!(!directories.contains_key(&tmp.path().join("Materials").join("Other")));
        assert!(index.files.get().is_none());
    }

    #[test]
    #[ignore = "set HP_FS_BENCH_ROOT to an existing large game content directory"]
    fn compare_indexed_and_targeted_mount_cost() {
        let root = std::env::var_os("HP_FS_BENCH_ROOT").expect("HP_FS_BENCH_ROOT");
        let root = Path::new(&root);
        let started = std::time::Instant::now();
        let indexed = DirIndex::build_game_dir(root).unwrap();
        let indexed_time = started.elapsed();
        let started = std::time::Instant::now();
        let targeted = DirIndex::for_lookup(root);
        let mut checked = 0;
        indexed.for_each_path("materials", Some("vmt"), &mut |path| {
            if checked < 8 {
                assert_eq!(
                    fs::read(indexed.disk_path(path).unwrap()).unwrap(),
                    fs::read(targeted.disk_path(path).unwrap()).unwrap()
                );
                checked += 1;
            }
        });
        eprintln!(
            "{} files: indexed mount {indexed_time:?}, targeted mount and {checked} reads {:?}",
            indexed.len(),
            started.elapsed()
        );
        assert!(targeted.files.get().is_none());
    }

    #[test]
    fn walk_filters_files_and_reports_missing_root() {
        let tmp = TempDir::new("walk");
        tmp.write("a.png", b"");
        tmp.write("b.txt", b"");
        tmp.write("x/y/z.png", b"");
        let files = walk_files(tmp.path(), None, &|n| n.ends_with(".png"), None).unwrap();
        assert_eq!(files, ["a.png", "x/y/z.png"]);
        assert!(walk_files(&tmp.path().join("missing"), None, &|_| true, None).is_err());
    }

    #[cfg(unix)]
    #[test]
    fn link_loops_are_bounded() {
        let tmp = TempDir::new("walk-loop");
        tmp.write("d/a.vmt", b"");
        std::os::unix::fs::symlink(tmp.path().join("d"), tmp.path().join("d/l1")).unwrap();
        std::os::unix::fs::symlink(tmp.path().join("d"), tmp.path().join("d/l2")).unwrap();
        let files = walk_files(tmp.path(), None, &|_| true, None).unwrap();
        assert!(files.contains(&"d/a.vmt".to_owned()));
        assert!(files.len() < 64, "{}", files.len());
    }
}
