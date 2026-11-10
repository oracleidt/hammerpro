use std::path::{Path, PathBuf};
use std::sync::atomic::AtomicBool;

use hp_bsp::{Bsp, PakCompression, PakOptions};

#[derive(Debug, Default)]
pub(crate) struct PackReport {
    pub packed: usize,
    pub pak_bytes: u64,
    pub missing: Vec<String>,
}

pub(crate) fn pak_key(inner: &str) -> String {
    inner.trim().replace('\\', "/").trim_start_matches('/').to_ascii_lowercase()
}

pub(crate) fn valid_entry(inner: &str) -> bool {
    hp_bsp::normalize_entry_name(inner).is_ok()
}

pub(crate) fn pack_into(
    bsp: &Path,
    files: &[(String, PathBuf)],
    cancel: &AtomicBool,
    progress: &(dyn Fn(u64, u64) + Sync),
) -> Result<PackReport, String> {
    let source = Bsp::read(bsp).map_err(|e| e.to_string())?;
    let (mut writer, compression) = {
        let pak = source.pakfile().map_err(|e| e.to_string())?;
        let compression = if pak.any_compressed() { PakCompression::Lzma } else { PakCompression::Store };
        (pak.to_writer().map_err(|e| e.to_string())?, compression)
    };
    for (inner, file) in files {
        writer.add_file(inner, file.clone()).map_err(|e| e.to_string())?;
    }
    let zip = writer.finish_with(PakOptions { compression, level: 6 }, cancel, progress).map_err(|e| e.to_string())?;
    let pak_bytes = zip.len() as u64;
    let missing = {
        let pak = hp_bsp::PakArchive::parse(zip.as_slice()).map_err(|e| e.to_string())?;
        files.iter().filter(|(inner, _)| !pak.contains(&pak_key(inner))).map(|(inner, _)| inner.clone()).collect()
    };
    let mut edit = source.edit();
    edit.set_pakfile(zip).map_err(|e| e.to_string())?;
    edit.save(bsp).map_err(|e| e.to_string())?;
    Ok(PackReport { packed: files.len(), pak_bytes, missing })
}

#[cfg(test)]
pub(crate) fn pak_entries(bsp: &Path) -> Option<Vec<String>> {
    let bsp = Bsp::read(bsp).ok()?;
    let pak = bsp.pakfile().ok()?;
    let mut names: Vec<String> = pak.entries().iter().map(|e| e.key.clone()).collect();
    names.sort();
    Some(names)
}

#[cfg(test)]
pub(crate) fn write_test_bsp(path: &Path, names: &[&str]) {
    let mut pak = hp_bsp::PakWriter::new();
    for name in names {
        pak.add(name, Vec::new()).expect("pak name");
    }
    let zip = pak.finish(PakOptions::default()).expect("pak");
    let data = hp_bsp::synthetic_bsp(20, &[(hp_bsp::lump::PAKFILE, zip)]);
    if let Err(e) = std::fs::write(path, data) {
        panic!("write {}: {e}", path.display());
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::test_util::scratch;

    #[test]
    fn files_are_added_and_replaced() {
        let dir = scratch("pack_into");
        let bsp = dir.join("m.bsp");
        write_test_bsp(&bsp, &["materials/qa/old.vmt", "materials/qa/keep.vtf"]);
        let src = dir.join("pack\u{c9}t\u{e9}");
        std::fs::create_dir_all(&src).unwrap();
        let a = src.join("m\u{e9}tal.vmt");
        let b = src.join("old.vmt");
        std::fs::write(&a, "LightmappedGeneric {}").unwrap();
        std::fs::write(&b, "new content").unwrap();
        let files = vec![("materials/QA/Metal.vmt".to_owned(), a), ("materials\\qa\\old.vmt".to_owned(), b)];
        let done = std::sync::atomic::AtomicU64::new(0);
        let report = pack_into(&bsp, &files, &AtomicBool::new(false), &|n, _| {
            done.store(n, std::sync::atomic::Ordering::Relaxed);
        })
        .expect("pack");
        assert_eq!(report.packed, 2);
        assert!(report.missing.is_empty(), "{:?}", report.missing);
        assert!(report.pak_bytes > 0);
        assert!(done.load(std::sync::atomic::Ordering::Relaxed) >= 2);
        let names = pak_entries(&bsp).expect("pak");
        assert_eq!(names, ["materials/qa/keep.vtf", "materials/qa/metal.vmt", "materials/qa/old.vmt"]);
        let read = Bsp::read(&bsp).unwrap();
        let pak = read.pakfile().unwrap();
        assert_eq!(pak.read_name("materials/qa/old.vmt").unwrap().as_deref(), Some(&b"new content"[..]));
    }

    #[test]
    fn errors_and_cancel_leave_the_bsp_alone() {
        let dir = scratch("pack_errors");
        let bsp = dir.join("m.bsp");
        assert!(pack_into(&bsp, &[], &AtomicBool::new(false), &|_, _| {}).is_err(), "no BSP");
        std::fs::write(&bsp, b"not a bsp at all").unwrap();
        assert!(pack_into(&bsp, &[], &AtomicBool::new(false), &|_, _| {}).is_err());
        write_test_bsp(&bsp, &["a.txt"]);
        let before = std::fs::read(&bsp).unwrap();
        let file = dir.join("x.vmt");
        std::fs::write(&file, "x").unwrap();
        let files = vec![("materials/x.vmt".to_owned(), file)];
        assert!(pack_into(&bsp, &files, &AtomicBool::new(true), &|_, _| {}).is_err(), "cancelled");
        assert_eq!(std::fs::read(&bsp).unwrap(), before);
        assert!(!valid_entry("../escape.vmt") && !valid_entry("") && valid_entry("materials/a.vmt"));
        assert_eq!(pak_key("\\Materials\\A.VMT "), "materials/a.vmt");
    }
}
