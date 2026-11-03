use std::path::{Path, PathBuf};
use std::time::{Duration, Instant};

use super::locations::{AppDirs, Root, TABLE};
use super::*;

fn scratch(name: &str) -> (AppDirs, PathBuf) {
    let base = if Path::new("E:/hp-scratch").is_dir() {
        PathBuf::from("E:/hp-scratch/agent-storage")
    } else {
        std::env::temp_dir().join("hp-scratch-agent-storage")
    }
    .join(name);
    let _ = std::fs::remove_dir_all(&base);
    let local = base.join("Local").join("HammerPro");
    let roaming = base.join("Roaming").join("HammerPro");
    let dirs = AppDirs {
        config: roaming.clone(),
        cache: local.clone(),
        autosave: local.join("autosave"),
        logs: local.join("logs"),
        crash: roaming.join("crash"),
        temp: base.join("Temp"),
    };
    for d in [&dirs.config, &dirs.cache, &dirs.autosave, &dirs.logs, &dirs.crash, &dirs.temp] {
        std::fs::create_dir_all(d).unwrap();
    }
    std::fs::create_dir_all(base.join("Maps")).unwrap();
    std::fs::write(base.join("Maps").join("mymap.vmf"), vec![7u8; 4096]).unwrap();
    (dirs, base)
}

fn put(path: &Path, size: usize) {
    std::fs::create_dir_all(path.parent().unwrap()).unwrap();
    std::fs::write(path, vec![1u8; size]).unwrap();
}

fn scan(dirs: &AppDirs) -> Report {
    scan_report(dirs, &Progress::default())
}

fn ids(list: &[&str]) -> Vec<String> {
    list.iter().map(|s| (*s).to_owned()).collect()
}

#[test]
fn sizes_are_summed_per_row_and_sorted() {
    let (d, _base) = scratch("sizes");
    put(&d.cache.join("thumbs").join("a.png"), 1000);
    put(&d.cache.join("thumbs").join("sub").join("b.png"), 500);
    put(&d.cache.join("prefab_thumbs").join("c.png"), 100);
    put(&d.cache.join("builds").join("m").join("m.bsp"), 5000);
    put(&d.autosave.join("x.vmf"), 300);
    put(&d.logs.join("hammerpro.log"), 200);
    put(&d.config.join("settings.toml"), 50);
    put(&d.cache.join("stray.bin"), 7);
    put(&d.cache.join("mcp").join("instances").join("1.json"), 9999);
    let r = scan(&d);
    assert_eq!(r.entry("thumbnails").unwrap().bytes, 1600);
    assert_eq!(r.entry("thumbnails").unwrap().files, 3);
    assert_eq!(r.entry("builds").unwrap().bytes, 5000);
    assert_eq!(r.entry("autosave").unwrap().bytes, 300);
    assert_eq!(r.entry("settings").unwrap().bytes, 50);
    assert_eq!(r.entry("other_cache").unwrap().bytes, 7, "autosave, logs, mcp and known folders are not counted twice");
    assert_eq!(r.total_bytes, 1600 + 5000 + 300 + 200 + 50 + 7);
    assert_eq!(r.entries.len(), TABLE.len());
    assert!(r.entries.windows(2).all(|w| w[0].bytes >= w[1].bytes), "largest first");
    assert_eq!(r.entries[0].id, "builds");
    assert_eq!(r.entry("logs").unwrap().in_use, 1);
    assert_eq!(r.entry("logs").unwrap().freeable(), 0);
    assert!((r.fraction(r.entry("builds").unwrap().bytes) - 5000.0 / r.total_bytes as f32).abs() < 1e-4);
}

#[test]
fn missing_folders_count_zero() {
    let (d, _base) = scratch("missing");
    let r = scan(&d);
    assert_eq!(r.total_bytes, 0);
    assert!(r.entries.iter().all(|e| e.bytes == 0));
    let empty = scan(&AppDirs::default());
    assert_eq!(empty.total_bytes, 0);
}

#[test]
fn byte_formatting() {
    use crate::i18n::Lang;
    let en = |n| format_bytes_in(n, Lang::En);
    assert_eq!(en(0), "0 B");
    assert_eq!(en(1023), "1023 B");
    assert_eq!(en(1536), "1.5 KB");
    assert_eq!(en(5 * 1024 * 1024), "5.0 MB");
    assert_eq!(en(3 << 30), "3.0 GB");
    assert_eq!(format_bytes_in(3 << 30, Lang::Fr), "3.0 Go");
    assert_eq!(format_bytes_in(10, Lang::Fr), "10 o");
}

#[test]
fn guard_only_accepts_items_inside_the_roots() {
    let (d, base) = scratch("guard");
    put(&d.cache.join("thumbs").join("a.png"), 10);
    put(&d.config.join("settings.toml"), 10);
    let ok = d.guard(Root::Cache, &d.cache.join("thumbs").join("a.png"));
    assert!(ok.is_ok(), "{ok:?}");
    assert!(d.guard(Root::Cache, &d.cache.join("thumbs")).is_ok());
    assert!(d.guard(Root::Cache, &d.cache).is_err(), "the root itself");
    assert!(d.guard(Root::Cache, &base.join("Maps").join("mymap.vmf")).is_err(), "a user map");
    assert!(d.guard(Root::Cache, &base.join("Maps")).is_err());
    assert!(d.guard(Root::Cache, &d.config.join("settings.toml")).is_err(), "another root");
    assert!(d.guard(Root::Cache, &d.cache.join("thumbs").join("..").join("..").join("Maps")).is_err(), "dot dot");
    assert!(d.guard(Root::Cache, Path::new("relative/path")).is_err());
    assert!(d.guard(Root::Cache, &d.cache.join("missing").join("a.png")).is_err(), "unresolvable parent");
    assert!(AppDirs::default().guard(Root::Cache, &d.cache.join("thumbs")).is_err(), "empty root");
    let drive = AppDirs { cache: base.ancestors().last().unwrap().to_path_buf(), ..AppDirs::default() };
    assert!(drive.guard(Root::Cache, &base).is_err(), "a drive is never a root");
}

#[test]
fn temp_items_need_the_hammer_pro_prefix_directly_in_temp() {
    let (d, _base) = scratch("temp_guard");
    put(&d.temp.join("hammerpro_lab").join("a.bin"), 10);
    put(&d.temp.join("other_tool").join("a.bin"), 10);
    assert!(d.guard(Root::Temp, &d.temp.join("hammerpro_lab")).is_ok());
    assert!(d.guard(Root::Temp, &d.temp.join("other_tool")).is_err());
    assert!(d.guard(Root::Temp, &d.temp.join("hammerpro_lab").join("a.bin")).is_err(), "only top level items");
    let r = scan(&d);
    assert_eq!(r.entry("temp").unwrap().bytes, 10, "foreign temp items are not listed");
}

#[test]
fn deleting_a_row_keeps_everything_else() {
    let (d, base) = scratch("delete");
    put(&d.cache.join("thumbs").join("a.png"), 1000);
    put(&d.cache.join("thumbs").join("sub").join("b.png"), 500);
    put(&d.cache.join("builds").join("m.bsp"), 5000);
    put(&d.config.join("settings.toml"), 50);
    let out = delete_rows(&d, &ids(&["thumbnails"]), &Progress::default()).unwrap();
    assert_eq!(out.len(), 1);
    assert_eq!(out[0].freed, 1500);
    assert_eq!(out[0].deleted, 2);
    assert!(out[0].errors.is_empty());
    assert!(d.cache.join("thumbs").is_dir(), "the folder itself stays");
    assert_eq!(std::fs::read_dir(d.cache.join("thumbs")).unwrap().count(), 0);
    assert!(d.cache.join("builds").join("m.bsp").is_file());
    assert!(d.config.join("settings.toml").is_file());
    assert!(base.join("Maps").join("mymap.vmf").is_file());
    let r = scan(&d);
    assert_eq!(r.entry("thumbnails").unwrap().bytes, 0);
    assert_eq!(r.entry("builds").unwrap().bytes, 5000);
}

#[test]
fn deleting_many_rows_reports_each_and_refuses_unknown_ids() {
    let (d, base) = scratch("multi");
    put(&d.cache.join("thumbs").join("a.png"), 100);
    put(&d.cache.join("builds").join("m.bsp"), 200);
    put(&d.autosave.join("x.vmf"), 300);
    put(&d.cache.join("stray.bin"), 40);
    let bad = delete_rows(&d, &ids(&["thumbnails", "../Maps"]), &Progress::default());
    assert!(bad.is_err());
    assert!(d.cache.join("thumbs").join("a.png").is_file(), "nothing is deleted when an id is unknown");
    let out =
        delete_rows(&d, &ids(&["thumbnails", "builds", "builds", "autosave", "other_cache"]), &Progress::default())
            .unwrap();
    assert_eq!(
        out.iter().map(|o| o.id.as_str()).collect::<Vec<_>>(),
        ["thumbnails", "builds", "autosave", "other_cache"]
    );
    assert_eq!(out.iter().map(|o| o.freed).sum::<u64>(), 100 + 200 + 300 + 40);
    assert!(!d.cache.join("stray.bin").exists());
    assert!(d.autosave.is_dir() && d.cache.is_dir());
    assert!(base.join("Maps").join("mymap.vmf").is_file());
}

#[test]
fn items_in_use_are_skipped_and_reported() {
    let (d, _base) = scratch("in_use");
    put(&d.logs.join("hammerpro.log"), 100);
    put(&d.logs.join("hammerpro.old.log"), 60);
    put(&d.crash.join("running-123.txt"), 5);
    put(&d.crash.join("crash-1.txt"), 20);
    let out = delete_rows(&d, &ids(&["logs", "crash"]), &Progress::default()).unwrap();
    assert!(d.logs.join("hammerpro.log").is_file());
    assert!(!d.logs.join("hammerpro.old.log").exists());
    assert!(d.crash.join("running-123.txt").is_file());
    assert!(!d.crash.join("crash-1.txt").exists());
    assert_eq!(out[0].skipped, 1);
    assert!(out[0].errors[0].error.contains("in use"));
    assert_eq!(out[0].freed, 60);
    let temp_name = format!("hammerpro-light-{}-1.bsp", std::process::id());
    put(&d.temp.join(&temp_name), 10);
    put(&d.temp.join("hammerpro-light-1-1.bsp"), 10);
    let out = delete_rows(&d, &ids(&["temp"]), &Progress::default()).unwrap();
    assert!(d.temp.join(&temp_name).is_file(), "files of this session stay");
    assert!(!d.temp.join("hammerpro-light-1-1.bsp").exists());
    assert_eq!(out[0].deleted, 1);
}

#[cfg(windows)]
#[test]
fn a_locked_file_is_reported_and_the_rest_is_deleted() {
    use std::os::windows::fs::OpenOptionsExt;
    let (d, _base) = scratch("locked");
    put(&d.cache.join("thumbs").join("a.png"), 10);
    put(&d.cache.join("thumbs").join("locked.png"), 20);
    put(&d.cache.join("thumbs").join("z.png"), 30);
    let _held =
        std::fs::OpenOptions::new().read(true).share_mode(0).open(d.cache.join("thumbs").join("locked.png")).unwrap();
    let out = delete_rows(&d, &ids(&["thumbnails"]), &Progress::default()).unwrap();
    assert_eq!(out[0].deleted, 2);
    assert_eq!(out[0].skipped, 1);
    assert_eq!(out[0].errors.len(), 1);
    assert!(out[0].errors[0].path.ends_with("locked.png"));
    assert_eq!(out[0].freed, 40);
    assert!(d.cache.join("thumbs").join("locked.png").is_file());
}

#[test]
fn read_only_files_are_deleted() {
    let (d, _base) = scratch("readonly");
    let f = d.cache.join("thumbs").join("ro.png");
    put(&f, 10);
    let mut perm = std::fs::metadata(&f).unwrap().permissions();
    perm.set_readonly(true);
    std::fs::set_permissions(&f, perm).unwrap();
    let out = delete_rows(&d, &ids(&["thumbnails"]), &Progress::default()).unwrap();
    assert!(out[0].errors.is_empty(), "{:?}", out[0].errors);
    assert!(!f.exists());
}

fn wait_idle(rt: &mut StorageRuntime) {
    let start = Instant::now();
    while rt.busy() {
        rt.poll();
        assert!(start.elapsed() < Duration::from_secs(20), "the job did not finish");
        std::thread::sleep(Duration::from_millis(5));
    }
}

#[test]
fn background_scan_and_delete_jobs() {
    let _lang = crate::testing::lang_lock();
    crate::register_translations();
    let (d, _base) = scratch("jobs");
    put(&d.cache.join("builds").join("m.bsp"), 4000);
    put(&d.cache.join("thumbs").join("a.png"), 100);
    let mut rt = StorageRuntime::default();
    rt.dirs = Some(d.clone());
    rt.start_scan();
    assert!(rt.scanning());
    wait_idle(&mut rt);
    assert_eq!(rt.report.as_ref().unwrap().total_bytes, 4100);

    rt.selected.insert("builds".to_owned());
    assert!(rt.start_delete(ids(&["builds"]), true).is_err(), "no compile cleanup while compiling");
    assert!(rt.start_delete(ids(&["nope"]), false).is_err());
    assert!(d.cache.join("builds").join("m.bsp").is_file());
    rt.start_delete(ids(&["builds"]), false).unwrap();
    assert!(rt.deleting());
    assert!(rt.selected.is_empty());
    wait_idle(&mut rt);
    let summary = rt.summary.as_ref().unwrap();
    assert_eq!(summary.freed, 4000);
    assert_eq!(rt.report.as_ref().unwrap().total_bytes, 100, "a new scan follows the cleanup");
    assert!(!d.cache.join("builds").join("m.bsp").exists());
    assert!(d.cache.join("thumbs").join("a.png").is_file());
}

#[test]
fn current_dirs_are_the_hammer_pro_folders() {
    let d = AppDirs::current();
    for root in [Root::Config, Root::Cache, Root::Autosave, Root::Logs, Root::Crash] {
        let p = d.root(root);
        assert!(p.as_os_str().is_empty() || p.components().any(|c| c.as_os_str() == "HammerPro"), "{p:?}");
    }
}

#[test]
fn every_row_is_translated() {
    let _lang = crate::testing::lang_lock();
    crate::register_translations();
    let mut seen = std::collections::HashSet::new();
    for spec in TABLE {
        assert!(seen.insert(spec.id), "{}", spec.id);
        for key in [spec.name_key, spec.desc_key] {
            assert!(crate::i18n::has_key(key), "{key}");
            assert!(!crate::i18n::tr_in(key, crate::i18n::Lang::Fr).is_empty());
        }
    }
    for (k, en, fr) in TR {
        assert!(k.starts_with("storage."));
        assert!(!en.is_empty() && !fr.is_empty(), "{k}");
        for text in [en, fr] {
            assert!(!text.contains('\u{2014}') && !text.contains('\u{2013}'), "{k}");
        }
    }
}

#[test]
fn index_file_and_update_downloads_have_their_own_rows() {
    let (d, _base) = scratch("index_updates");
    put(&d.cache.join("content_index.bin"), 700);
    put(&d.cache.join("content_index.bin.tmp"), 30);
    put(&d.cache.join("stray.bin"), 5);
    put(&d.temp.join("HammerPro-update-1-v1.0.3").join("HammerPro.zip"), 900);
    put(&d.temp.join("hammerpro-update.log"), 11);
    put(&d.temp.join("hammerpro_lab").join("a.bin"), 3);
    let r = scan(&d);
    assert_eq!(r.entry("content_index").unwrap().bytes, 730);
    assert_eq!(r.entry("other_cache").unwrap().bytes, 5);
    assert_eq!(r.entry("updates").unwrap().bytes, 911);
    assert_eq!(r.entry("temp").unwrap().bytes, 3);
    let out = delete_rows(&d, &ids(&["content_index", "updates"]), &Progress::default()).unwrap();
    assert_eq!(out.iter().map(|o| o.freed).sum::<u64>(), 730 + 911);
    assert!(d.cache.join("stray.bin").is_file());
    assert!(d.temp.join("hammerpro_lab").is_dir());
}

#[cfg(windows)]
#[test]
fn a_junction_to_user_files_is_removed_not_followed() {
    let (d, base) = scratch("junction");
    let maps = base.join("Maps");
    std::fs::create_dir_all(d.cache.join("thumbs")).unwrap();
    let link = d.cache.join("thumbs").join("maps_link");
    let status = std::process::Command::new("cmd")
        .args(["/C", "mklink", "/J"])
        .arg(link.to_string_lossy().replace('/', "\\"))
        .arg(maps.to_string_lossy().replace('/', "\\"))
        .stdout(std::process::Stdio::null())
        .status()
        .unwrap();
    assert!(status.success());
    assert!(link.join("mymap.vmf").is_file());
    let r = scan(&d);
    assert!(r.entry("thumbnails").unwrap().bytes < 4096, "the target of a link is not counted");
    let out = delete_rows(&d, &ids(&["thumbnails"]), &Progress::default()).unwrap();
    assert!(out[0].errors.is_empty(), "{:?}", out[0].errors);
    assert!(!link.exists());
    assert!(maps.join("mymap.vmf").is_file(), "the map behind the link is untouched");
}
