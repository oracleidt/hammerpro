use std::path::{Path, PathBuf};
use std::time::Instant;

use hp_fs::{
    CustomDir, CustomMode, GameFs, MountOptions, SourceKind, SourceOrigin, detect_gmod_root, detect_steam_libraries,
    detect_workshop_dir,
};
use rayon::prelude::*;

const DEFAULT_GMOD_DIR: &str = r"D:\SteamLibrary\steamapps\common\GarrysMod";

fn gmod_dir() -> Option<PathBuf> {
    let dir = std::env::var_os("HAMMERPRO_GMOD_DIR").map_or_else(|| PathBuf::from(DEFAULT_GMOD_DIR), PathBuf::from);
    if dir.join("garrysmod").is_dir() {
        Some(dir)
    } else {
        eprintln!("skipping: Garry's Mod not found at {}", dir.display());
        None
    }
}

fn same_dir(a: &Path, b: &Path) -> bool {
    let key =
        |p: &Path| std::fs::canonicalize(p).unwrap_or_else(|_| p.to_path_buf()).to_string_lossy().to_ascii_lowercase();
    key(a) == key(b)
}

fn mount_all(root: &Path) -> GameFs {
    let start = Instant::now();
    let fs = GameFs::for_gmod(root, &MountOptions::default()).expect("mount");
    let elapsed = start.elapsed();
    let sources = fs.sources();
    let files: usize = sources.iter().map(|s| s.file_count).sum();
    eprintln!("mounted {} sources ({files} files) in {:.1} ms", sources.len(), elapsed.as_secs_f64() * 1000.0);
    for w in fs.warnings() {
        eprintln!("  warning: {w}");
    }
    fs
}

#[test]
fn detects_this_install() {
    let Some(expected) = gmod_dir() else { return };
    let start = Instant::now();
    let libraries = detect_steam_libraries();
    let root = detect_gmod_root();
    eprintln!(
        "detection took {:.1} ms: libraries {libraries:?}, gmod {root:?}",
        start.elapsed().as_secs_f64() * 1000.0
    );
    if std::env::var_os("HAMMERPRO_GMOD_DIR").is_some() {
        return;
    }
    let root = root.expect("Garry's Mod not detected");
    assert!(same_dir(&root, &expected), "{} != {}", root.display(), expected.display());
    assert!(libraries.iter().any(|l| expected.starts_with(l)));
    let workshop = detect_workshop_dir(&root).expect("workshop folder");
    assert!(workshop.ends_with(Path::new("workshop").join("content").join("4000")));
}

#[test]
fn mounts_everything_and_indexes_assets() {
    let Some(root) = gmod_dir() else { return };
    let fs = mount_all(&root);
    let sources = fs.sources();
    let labels: Vec<&str> = sources.iter().map(|s| s.label.as_str()).collect();
    assert!(labels.contains(&"garrysmod.vpk"), "{labels:?}");
    assert!(labels.contains(&"hl2_textures.vpk"), "{labels:?}");
    let at = |l: &str| labels.iter().position(|x| *x == l);
    assert!(at("garrysmod.vpk") < at("garrysmod") && at("garrysmod") < at("hl2_textures.vpk"), "{labels:?}");
    assert!(labels.contains(&"hl2_misc.vpk"), "{labels:?}");
    let workshop = sources.iter().filter(|s| s.origin == SourceOrigin::Workshop).count();
    let legacy = sources.iter().filter(|s| matches!(&s.kind, SourceKind::Gma(g) if g.is_legacy())).count();
    eprintln!("workshop items mounted: {workshop} ({legacy} legacy)");

    let start = Instant::now();
    let materials = fs.materials();
    let t_materials = start.elapsed();
    let start = Instant::now();
    let models = fs.models();
    let t_models = start.elapsed();
    let start = Instant::now();
    let sounds = fs.sounds();
    let t_sounds = start.elapsed();
    eprintln!(
        "materials {} in {:.1} ms, models {} in {:.1} ms, sounds {} in {:.1} ms",
        materials.len(),
        t_materials.as_secs_f64() * 1000.0,
        models.len(),
        t_models.as_secs_f64() * 1000.0,
        sounds.len(),
        t_sounds.as_secs_f64() * 1000.0
    );
    let start = Instant::now();
    let again = fs.materials();
    eprintln!("cached materials() in {:.3} ms", start.elapsed().as_secs_f64() * 1000.0);
    assert!(std::sync::Arc::ptr_eq(&materials, &again));
    assert!(materials.len() > 10_000);
    assert!(models.len() > 1_000);
    assert!(sounds.len() > 1_000);
    assert!(materials.windows(2).all(|w| w[0].name < w[1].name));
    assert!(materials.iter().any(|m| m.name == "concrete/concretefloor001a"));
    assert!(models.iter().any(|m| m.name == "models/props_c17/oildrum001.mdl"));

    let vmt = fs.read_text("Materials\\Concrete\\ConcreteFloor001a.vmt").expect("vmt");
    assert!(vmt.to_ascii_lowercase().contains("$basetexture"));
    let material = fs.load_material("concrete/concretefloor001a").expect("material");
    let base = material.base_texture().expect("base texture");
    let start = Instant::now();
    let thumb = fs.load_texture(&base, Some(128)).expect("thumbnail");
    eprintln!(
        "thumbnail of {base}: {}x{} (original {}x{}) in {:.2} ms",
        thumb.width,
        thumb.height,
        thumb.original_width,
        thumb.original_height,
        start.elapsed().as_secs_f64() * 1000.0
    );
    assert!(thumb.width <= 128 && thumb.height <= 128);
    assert_eq!(thumb.rgba.len(), (thumb.width * thumb.height * 4) as usize);
    assert_eq!(fs.texture_size(&base), Some((thumb.original_width, thumb.original_height)));
    let raw = fs.load_texture_raw(&base).expect("raw vtf");
    assert!(raw.starts_with(b"VTF\0"));

    let sample: Vec<&str> = materials.iter().step_by(37).take(1000).map(|m| m.name.as_str()).collect();
    let start = Instant::now();
    let decoded = sample
        .par_iter()
        .filter(|name| {
            fs.material(name).and_then(|m| m.preview_texture()).is_some_and(|t| fs.load_texture(&t, Some(128)).is_ok())
        })
        .count();
    eprintln!("{decoded}/{} material thumbnails in {:.1} ms", sample.len(), start.elapsed().as_secs_f64() * 1000.0);
    assert!(decoded > sample.len() / 2);

    let unresolved = sample.iter().filter_map(|m| fs.material(m)).filter(|m| m.is_patch()).count();
    eprintln!("unresolved patch materials in the sample: {unresolved}");
}

#[test]
fn workshop_content_is_readable() {
    let Some(root) = gmod_dir() else { return };
    let fs = GameFs::for_gmod(&root, &MountOptions::default()).expect("mount");
    let workshop: Vec<u32> = fs.sources().iter().filter(|s| s.origin == SourceOrigin::Workshop).map(|s| s.id).collect();
    if workshop.is_empty() {
        eprintln!("skipping: no workshop items");
        return;
    }
    let materials = fs.materials();
    let from_workshop: Vec<&str> =
        materials.iter().filter(|m| workshop.contains(&m.source)).map(|m| m.name.as_str()).collect();
    eprintln!("{} materials come from workshop items", from_workshop.len());
    for name in from_workshop.iter().take(20) {
        let vmt = format!("materials/{name}.vmt");
        let source = fs.which_source(&vmt).expect("source");
        assert!(workshop.contains(&source));
        assert!(fs.read(&vmt).is_some(), "{vmt}");
    }
    let before = materials.len();
    for id in &workshop {
        fs.set_source_enabled(*id, false);
    }
    let after = fs.materials().len();
    eprintln!("materials with workshop disabled: {after} (was {before})");
    assert!(after <= before);
    if !from_workshop.is_empty() {
        assert!(after < before);
    }
}

#[test]
fn custom_folder_inside_game_materials() {
    let Some(root) = gmod_dir() else { return };
    let materials_dir = root.join("garrysmod").join("materials");
    let Some(sub) =
        std::fs::read_dir(&materials_dir).ok().and_then(|r| r.flatten().map(|e| e.path()).find(|p| p.is_dir()))
    else {
        eprintln!("skipping: no folder in garrysmod/materials");
        return;
    };
    let fs = GameFs::empty();
    let start = Instant::now();
    let id = fs.add_custom_dir(CustomDir::new(&sub)).expect("custom dir");
    let layout = fs.custom_dir_layout(id).expect("layout");
    let name = sub.file_name().unwrap().to_string_lossy().to_ascii_lowercase();
    assert_eq!(layout.mode, CustomMode::InsideMaterials);
    assert_eq!(layout.prefix, name);
    let materials = fs.materials();
    eprintln!(
        "custom folder {} ({} materials) indexed in {:.1} ms",
        sub.display(),
        materials.len(),
        start.elapsed().as_secs_f64() * 1000.0
    );
    assert!(materials.iter().all(|m| m.name.starts_with(&format!("{name}/"))));
}

#[test]
fn pack_list_for_real_content() {
    let Some(root) = gmod_dir() else { return };
    let fs = GameFs::for_gmod(&root, &MountOptions::default()).expect("mount");
    let cache = Path::new(env!("CARGO_MANIFEST_DIR")).join("../../target/hp-fs/scratch/pack-cache");
    let materials: Vec<String> = fs.materials().iter().take(50).map(|m| m.name.clone()).collect();
    let models: Vec<String> = fs.models().iter().take(20).map(|m| m.name.clone()).collect();
    let start = Instant::now();
    let list = fs.pack_list(&materials, &models, &cache);
    eprintln!(
        "pack list of {} materials and {} models: {} files in {:.1} ms",
        materials.len(),
        models.len(),
        list.len(),
        start.elapsed().as_secs_f64() * 1000.0
    );
    for (internal, disk) in &list {
        assert!(disk.is_file(), "{internal} -> {}", disk.display());
        assert!(!disk.starts_with(&root) || !disk.starts_with(&cache));
    }
    let _ = std::fs::remove_dir_all(&cache);
}

#[test]
fn keyword_index_of_the_whole_game() {
    let Some(root) = gmod_dir() else { return };
    let opts = MountOptions { mount_workshop: false, ..MountOptions::default() };
    let fs = GameFs::for_gmod(&root, &opts).expect("mount");
    let materials = fs.materials().len();
    let start = Instant::now();
    let quick = fs.material_keywords_quick();
    eprintln!(
        "quick keyword index: {} materials read in {:.1} ms",
        quick.scanned(),
        start.elapsed().as_secs_f64() * 1000.0
    );
    let start = Instant::now();
    let keywords = fs.material_keywords();
    let elapsed = start.elapsed();
    assert!(keywords.is_complete());
    eprintln!(
        "keyword index: {} of {materials} materials have keywords, built in {:.1} ms",
        keywords.len(),
        elapsed.as_secs_f64() * 1000.0
    );
    assert_eq!(keywords.scanned(), materials);
    let again = Instant::now();
    let _ = fs.material_keywords();
    assert!(again.elapsed().as_millis() < 50);
    assert!(elapsed.as_secs() < 30, "{elapsed:?}");
}

#[test]
fn scan_timing_report() {
    let Some(root) = gmod_dir() else { return };
    let index = std::env::var_os("HP_SCAN_INDEX").map(PathBuf::from);
    let mode = std::env::var("HP_SCAN_MODE").ok().and_then(|m| hp_fs::ScanMode::parse(&m)).unwrap_or_default();
    hp_fs::configure_scan(hp_fs::ScanConfig { index_file: index, mode });
    let runs = std::env::var("HP_SCAN_RUNS").ok().and_then(|v| v.parse::<usize>().ok()).unwrap_or(3);
    for run in 0..runs {
        let start = Instant::now();
        let fs = GameFs::for_gmod(&root, &MountOptions::default()).expect("mount");
        let elapsed = start.elapsed();
        let stats = hp_fs::scan_stats();
        eprintln!(
            "  index {} ({} entries, {} bytes): dirs checked {}, reused {}, rescanned {}, mode {:?}, drives {:?}",
            stats.index_state,
            stats.index_entries,
            stats.index_bytes,
            stats.dirs_checked,
            stats.dirs_reused,
            stats.dirs_rescanned,
            stats.mode,
            stats.volumes
        );
        let sources = fs.sources();
        let files: usize = sources.iter().map(|s| s.file_count).sum();
        let mut vpk = std::time::Duration::ZERO;
        let mut vpk_files = 0usize;
        for s in &sources {
            if let SourceKind::Vpk(_) = s.kind {
                let t = Instant::now();
                let _ = hp_vpk::VpkArchive::open(&s.path);
                vpk += t.elapsed();
                vpk_files += s.file_count;
            }
        }
        eprintln!(
            "scan run {run}: {} sources, {files} files, mount {:.0} ms (vpk reopen alone {:.0} ms for {vpk_files} entries)",
            sources.len(),
            elapsed.as_secs_f64() * 1000.0,
            vpk.as_secs_f64() * 1000.0
        );
    }
}

#[test]
fn scan_tree_timing() {
    let Some(tree) = std::env::var_os("HP_SCAN_TREE").map(PathBuf::from) else { return };
    let mode = std::env::var("HP_SCAN_MODE").ok().and_then(|m| hp_fs::ScanMode::parse(&m)).unwrap_or_default();
    let index = std::env::var_os("HP_SCAN_INDEX").map(PathBuf::from);
    hp_fs::configure_scan(hp_fs::ScanConfig { index_file: index, mode });
    let fs = GameFs::empty();
    for pass in 0..2 {
        let start = Instant::now();
        let id = fs.add_custom_dir(CustomDir::with_prefix(&tree, "bench")).expect("scan");
        let elapsed = start.elapsed();
        let files = fs.sources().iter().find(|s| s.id == id).map_or(0, |s| s.file_count);
        let stats = hp_fs::scan_stats();
        eprintln!(
            "tree pass {pass} mode {mode:?}: {files} material files, {:.0} ms, folders checked {} reused {} scanned {}",
            elapsed.as_secs_f64() * 1000.0,
            stats.dirs_checked,
            stats.dirs_reused,
            stats.dirs_rescanned
        );
        fs.remove_source(id);
    }
}
