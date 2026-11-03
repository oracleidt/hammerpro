use std::path::{Path, PathBuf};
use std::thread::sleep;
use std::time::Duration;

use crate::scan::{IndexCache, WalkReport, walk_in};
use crate::test_util::TempDir;

fn settle() {
    sleep(Duration::from_millis(60));
}

fn walk_it(cache: &IndexCache, root: &Path, parallel: bool) -> (Vec<String>, WalkReport) {
    walk_in(cache, parallel, 0, root, None, &|_| true, Some("test")).unwrap()
}

fn fixture(tmp: &TempDir) -> PathBuf {
    let root = tmp.mkdir("game");
    for rel in ["materials/a/one.vmt", "materials/a/b/two.vtf", "models/m.mdl", "sound/s.wav", "top.txt"] {
        tmp.write(&format!("game/{rel}"), b"x");
    }
    settle();
    root
}

#[test]
fn index_file_roundtrip_gives_the_same_list_without_rescanning() {
    let tmp = TempDir::new("index-roundtrip");
    let root = fixture(&tmp);
    let file = tmp.path().join("cache").join("index.bin");
    let first = IndexCache::new(Some(file.clone()));
    let (full, report) = walk_it(&first, &root, true);
    assert_eq!(full.len(), 5);
    assert_eq!(report.reused, 0);
    assert!(report.rescanned >= 5);
    first.flush();
    assert!(file.is_file());
    assert!(!tmp.path().join("cache").join("index.bin.tmp").exists());

    let second = IndexCache::new(Some(file));
    let (again, report) = walk_it(&second, &root, true);
    assert_eq!(again, full);
    assert_eq!(report.rescanned, 0);
    assert_eq!(report.reused, report.checked);
    assert!(report.checked >= 5);
}

#[test]
fn corrupt_or_foreign_index_files_fall_back_to_a_full_scan() {
    let tmp = TempDir::new("index-corrupt");
    let root = fixture(&tmp);
    let file = tmp.path().join("index.bin");
    let cache = IndexCache::new(Some(file.clone()));
    let (full, _) = walk_it(&cache, &root, false);
    cache.flush();
    let good = std::fs::read(&file).unwrap();

    let mut variants: Vec<Vec<u8>> = vec![Vec::new(), b"garbage".to_vec(), good[..good.len() / 2].to_vec()];
    let mut flipped = good.clone();
    let mid = flipped.len() - 3;
    flipped[mid] ^= 0xff;
    variants.push(flipped);
    let mut foreign = good;
    foreign[8] = 77;
    variants.push(foreign);
    for bad in variants {
        std::fs::write(&file, &bad).unwrap();
        let cache = IndexCache::new(Some(file.clone()));
        let (list, report) = walk_it(&cache, &root, true);
        assert_eq!(list, full);
        assert_eq!(report.reused, 0);
        cache.flush();
        let healed = IndexCache::new(Some(file.clone()));
        let (list, report) = walk_it(&healed, &root, true);
        assert_eq!(list, full);
        assert_eq!(report.rescanned, 0);
    }
}

#[test]
fn only_changed_folders_are_rescanned() {
    let tmp = TempDir::new("index-changes");
    let root = fixture(&tmp);
    let cache = IndexCache::new(Some(tmp.path().join("index.bin")));
    let (base, _) = walk_it(&cache, &root, true);
    settle();

    tmp.write("game/materials/a/b/new.vtf", b"n");
    let (list, report) = walk_it(&cache, &root, true);
    assert_eq!(list.len(), base.len() + 1);
    assert!(list.contains(&"materials/a/b/new.vtf".to_owned()));
    assert_eq!(report.rescanned, 1);
    settle();

    tmp.write("game/models/deep/er/x.mdl", b"n");
    let (list, report) = walk_it(&cache, &root, true);
    assert!(list.contains(&"models/deep/er/x.mdl".to_owned()));
    assert_eq!(report.rescanned, 3);
    settle();

    std::fs::remove_file(root.join("sound/s.wav")).unwrap();
    std::fs::remove_dir_all(root.join("materials/a/b")).unwrap();
    let (list, report) = walk_it(&cache, &root, false);
    assert!(!list.contains(&"sound/s.wav".to_owned()));
    assert!(!list.iter().any(|f| f.starts_with("materials/a/b/")));
    assert_eq!(report.rescanned, 2);

    let (list, report) = walk_it(&cache, &root, false);
    let fresh = IndexCache::new(None);
    let (expected, _) = walk_it(&fresh, &root, true);
    assert_eq!(list, expected);
    assert_eq!(report.rescanned, 0);
}

#[test]
fn recently_modified_folders_are_not_trusted() {
    let tmp = TempDir::new("index-racy");
    let root = fixture(&tmp);
    let cache = IndexCache::new(Some(tmp.path().join("index.bin")));
    walk_in(&cache, true, i64::MAX / 2, &root, None, &|_| true, Some("test")).unwrap();
    let (_, report) = walk_in(&cache, true, i64::MAX / 2, &root, None, &|_| true, Some("test")).unwrap();
    assert_eq!(report.reused, 0);
    assert!(report.rescanned >= 5);
}

#[test]
fn profiles_and_roots_do_not_share_entries() {
    let tmp = TempDir::new("index-keys");
    let root = fixture(&tmp);
    let cache = IndexCache::new(Some(tmp.path().join("index.bin")));
    let (all, _) = walk_in(&cache, true, 0, &root, None, &|_| true, Some("one")).unwrap();
    let (vmts, report) = walk_in(&cache, true, 0, &root, None, &|n| n.ends_with(".vmt"), Some("two")).unwrap();
    assert_eq!(vmts, ["materials/a/one.vmt"]);
    assert!(all.len() > vmts.len());
    assert_eq!(report.reused, 0);
    let (top, _) = walk_in(&cache, true, 0, &root, Some(&["materials"]), &|_| true, Some("three")).unwrap();
    assert!(top.iter().all(|f| f.starts_with("materials/")));
}

#[test]
fn disabled_cache_never_writes_and_missing_root_errors() {
    let tmp = TempDir::new("index-off");
    let root = fixture(&tmp);
    let cache = IndexCache::new(None);
    let (list, report) = walk_it(&cache, &root, true);
    assert_eq!(list.len(), 5);
    assert_eq!(report.reused, 0);
    cache.flush();
    assert!(walk_in(&cache, true, 0, &tmp.path().join("missing"), None, &|_| true, Some("x")).is_err());
    let on = IndexCache::new(Some(tmp.path().join("i.bin")));
    assert!(walk_in(&on, true, 0, &tmp.path().join("missing"), None, &|_| true, Some("x")).is_err());
}

#[test]
fn sequential_and_parallel_scans_agree() {
    let tmp = TempDir::new("index-modes");
    let root = fixture(&tmp);
    let a = IndexCache::new(None);
    let (par, _) = walk_it(&a, &root, true);
    let (seq, _) = walk_it(&a, &root, false);
    assert_eq!(par, seq);
}

#[test]
fn clear_removes_the_file_and_the_entries() {
    let tmp = TempDir::new("index-clear");
    let root = fixture(&tmp);
    let file = tmp.path().join("index.bin");
    let cache = IndexCache::new(Some(file.clone()));
    walk_it(&cache, &root, true);
    cache.flush();
    assert!(file.is_file());
    cache.clear();
    assert!(!file.exists());
    let (_, report) = walk_it(&cache, &root, true);
    assert_eq!(report.reused, 0);
}
