use std::collections::BTreeMap;
use std::path::{Path, PathBuf};
use std::sync::atomic::AtomicBool;

use hp_bsp::Bsp;
use hp_fs::deps::{DepKind, DepRoot, DepsCache, PackPolicy, ResolveOptions, RootKind};
use hp_fs::{GameFs, MountOptions, SourceOrigin};

use crate::procmem::MemSource;
use crate::scan::{ScanOptions, par_chunks};
use crate::settings::SnifferSettings;

#[derive(Clone, Copy, Debug, PartialEq, Eq, PartialOrd, Ord)]
pub enum AssetClass {
    Model,
    Material,
    Sound,
    Other,
}

impl AssetClass {
    pub fn label(self) -> &'static str {
        match self {
            AssetClass::Model => "models",
            AssetClass::Material => "materials",
            AssetClass::Sound => "sounds",
            AssetClass::Other => "other",
        }
    }
}

pub fn class_of_path(vpath: &str) -> AssetClass {
    let p = vpath.trim_start_matches('/').to_ascii_lowercase();
    if p.starts_with("models/") {
        AssetClass::Model
    } else if p.starts_with("materials/") {
        AssetClass::Material
    } else if p.starts_with("sound/") {
        AssetClass::Sound
    } else {
        AssetClass::Other
    }
}

pub fn class_of_dep(kind: DepKind, vpath: &str) -> AssetClass {
    match kind {
        DepKind::Model | DepKind::ModelPart => AssetClass::Model,
        DepKind::Material | DepKind::Texture => AssetClass::Material,
        DepKind::Sound => AssetClass::Sound,
        DepKind::SoundScript | DepKind::Soundscape | DepKind::Particle | DepKind::ParticleManifest => {
            let _ = vpath;
            AssetClass::Other
        }
        DepKind::File => class_of_path(vpath),
    }
}

pub fn wants(s: &SnifferSettings, class: AssetClass) -> bool {
    match class {
        AssetClass::Model => s.save_models,
        AssetClass::Material => s.save_materials,
        AssetClass::Sound => s.save_sounds,
        AssetClass::Other => s.save_other,
    }
}

#[derive(Clone, Debug, Default, PartialEq, Eq)]
pub struct AssetReport {
    pub pak_files: usize,
    pub pak_bytes: u64,
    pub pak_skipped_stock: usize,
    pub pak_skipped_unsafe: usize,
    pub content_files: usize,
    pub content_bytes: u64,
    pub content_failed: usize,
    pub stock_skipped: usize,
    pub missing: Vec<String>,
    pub by_class: BTreeMap<String, usize>,
    pub warnings: Vec<String>,
    pub memory_models: usize,
    pub memory_model_bytes: u64,
    pub memory_models_wanted: usize,
}

impl AssetReport {
    pub fn total_files(&self) -> usize {
        self.pak_files + self.content_files
    }
}

pub fn extract_pak(
    bsp: &Bsp,
    dest: &Path,
    s: &SnifferSettings,
    stock: Option<&dyn Fn(&str) -> bool>,
    skip: &std::collections::HashSet<String>,
    report: &mut AssetReport,
) {
    let Ok(pak) = bsp.pakfile() else { return };
    for entry in pak.entries() {
        let class = class_of_path(&entry.name);
        if !wants(s, class) || skip.contains(&entry.key.replace('\\', "/")) {
            continue;
        }
        if entry.size as u64 > hp_decompile::pak::MAX_FILE_BYTES
            || report.pak_files >= hp_decompile::pak::MAX_FILES
            || report.pak_bytes + entry.size as u64 > hp_decompile::pak::MAX_TOTAL_BYTES
        {
            report.pak_skipped_unsafe += 1;
            continue;
        }
        if s.only_non_stock && stock.is_some_and(|f| f(&entry.key)) {
            report.pak_skipped_stock += 1;
            continue;
        }
        let Some(path) = hp_decompile::pak::safe_join(dest, &entry.name) else {
            report.pak_skipped_unsafe += 1;
            continue;
        };
        let Ok(data) = pak.read(entry) else {
            report.pak_skipped_unsafe += 1;
            continue;
        };
        if let Some(dir) = path.parent()
            && std::fs::create_dir_all(dir).is_err()
        {
            report.pak_skipped_unsafe += 1;
            continue;
        }
        if std::fs::write(&path, &data).is_err() {
            report.pak_skipped_unsafe += 1;
            continue;
        }
        report.pak_files += 1;
        report.pak_bytes += data.len() as u64;
        *report.by_class.entry(class.label().to_owned()).or_default() += 1;
    }
}

fn root_kind(k: hp_editor::deps_roots::RootKind) -> Option<RootKind> {
    use hp_editor::deps_roots::RootKind as E;
    Some(match k {
        E::Material => RootKind::Material,
        E::Texture => RootKind::Texture,
        E::Model => RootKind::Model,
        E::Sprite => RootKind::Sprite,
        E::Sound => RootKind::Sound,
        E::Particle => RootKind::Particle,
        E::Soundscape => RootKind::Soundscape,
        E::Skybox => RootKind::Skybox,
        E::File => RootKind::File,
        E::Instance => return None,
    })
}

pub fn roots_for(map: &hp_vmf::Map) -> Vec<DepRoot> {
    let classify = |_: &str, _: &str| None;
    hp_editor::deps_roots::map_asset_roots(map, &classify)
        .into_iter()
        .filter_map(|r| Some(DepRoot { kind: root_kind(r.kind)?, name: r.name, why: "map".to_owned() }))
        .collect()
}

pub fn mount_for(install_root: &Path) -> Result<GameFs, String> {
    let opts = MountOptions { mount_downloads: true, mount_cfg: true, ..MountOptions::default() };
    GameFs::for_gmod(install_root, &opts).map_err(|e| e.to_string())
}

pub fn is_stock(fs: &GameFs, vpath: &str) -> bool {
    let Some(id) = fs.which_source(vpath) else { return false };
    fs.source(id).is_some_and(|s| s.origin == SourceOrigin::BaseGame)
}

pub fn collect_content(
    fs: &GameFs,
    map: &hp_vmf::Map,
    map_name: &str,
    dest: &Path,
    s: &SnifferSettings,
    cancel: &AtomicBool,
    report: &mut AssetReport,
) {
    let roots = roots_for(map);
    if roots.is_empty() {
        return;
    }
    let cache = DepsCache::new();
    let graph = fs.resolve_dependencies(
        &roots,
        &ResolveOptions { map_name, cancel: Some(cancel), editor_textures: false },
        &cache,
    );
    for m in &graph.missing {
        if report.missing.len() < 500 {
            report.missing.push(m.name.clone());
        }
    }
    let policy = PackPolicy { workshop: true, mounted: false };
    let mut chosen = Vec::new();
    for n in &graph.nodes {
        let class = class_of_dep(n.kind, &n.vpath);
        if !wants(s, class) {
            continue;
        }
        if s.only_non_stock && !n.ships(policy) {
            report.stock_skipped += 1;
            continue;
        }
        chosen.push((class, n));
    }
    let nodes: Vec<&hp_fs::deps::DepNode> = chosen.iter().map(|(_, n)| *n).collect();
    let run = fs.collect_dependencies(
        &nodes,
        dest,
        &hp_fs::collect::CollectOptions {
            existing: hp_fs::collect::ExistingFiles::Replace,
            cancel: Some(cancel),
            progress: None,
        },
    );
    let mut class_of: BTreeMap<&str, AssetClass> = BTreeMap::new();
    for (class, n) in &chosen {
        class_of.insert(n.vpath.as_str(), *class);
    }
    for f in &run.files {
        use hp_fs::collect::CollectOutcome as O;
        match &f.outcome {
            O::Copied | O::Replaced | O::Same | O::Skipped => {
                report.content_files += 1;
                report.content_bytes += f.bytes;
                if let Some(c) = class_of.get(f.vpath.as_str()) {
                    *report.by_class.entry(c.label().to_owned()).or_default() += 1;
                }
            }
            O::Failed(e) => {
                report.content_failed += 1;
                if report.warnings.len() < 50 {
                    report.warnings.push(format!("{}: {e}", f.vpath));
                }
            }
        }
    }
}

pub fn normalize_model_name(name: &str) -> String {
    let n = name.trim().replace(char::from(92u8), "/").to_ascii_lowercase();
    n.strip_prefix("models/").map(str::to_owned).unwrap_or(n)
}

const MDL_MIN_LEN: i32 = 200;
const MDL_MAX_LEN: i32 = 128 << 20;

pub fn mdl_header(head: &[u8]) -> Option<(String, usize)> {
    if head.len() < 80 || &head[0..4] != b"IDST" {
        return None;
    }
    let version = i32::from_le_bytes([head[4], head[5], head[6], head[7]]);
    if !(44..=49).contains(&version) {
        return None;
    }
    let raw = &head[12..76];
    let end = raw.iter().position(|&b| b == 0)?;
    if end < 5 || !raw[..end].iter().all(|&b| (0x20..0x7f).contains(&b)) {
        return None;
    }
    let name = String::from_utf8_lossy(&raw[..end]).into_owned();
    if !name.to_ascii_lowercase().ends_with(".mdl") {
        return None;
    }
    let len = i32::from_le_bytes([head[76], head[77], head[78], head[79]]);
    if !(MDL_MIN_LEN..=MDL_MAX_LEN).contains(&len) {
        return None;
    }
    Some((normalize_model_name(&name), len as usize))
}

pub fn find_loaded_models<S: MemSource + ?Sized>(
    src: &S,
    opts: &ScanOptions,
    wanted: &std::collections::HashSet<String>,
) -> BTreeMap<String, Vec<u8>> {
    let finder = memchr::memmem::Finder::new(b"IDST").into_owned();
    let opts = ScanOptions { overlap: 96, ..*opts };
    let hits: Vec<(u64, String, usize)> = par_chunks(src, &opts, |base, data, own| {
        let mut out = Vec::new();
        for at in finder.find_iter(data) {
            if at >= own {
                break;
            }
            let Some(head) = data.get(at..at + 80) else { continue };
            if let Some((name, len)) = mdl_header(head)
                && wanted.contains(&name)
            {
                out.push((base + at as u64, name, len));
            }
        }
        out
    })
    .into_iter()
    .flatten()
    .collect();
    let mut found: BTreeMap<String, Vec<u8>> = BTreeMap::new();
    for (addr, name, len) in hits {
        if found.contains_key(&name) {
            continue;
        }
        let bytes = src.read_vec(addr, len);
        if bytes.len() == len && mdl_header(&bytes).is_some_and(|(n, l)| n == name && l == len) {
            found.insert(name, bytes);
        }
    }
    found
}

pub fn recover_models_from_memory<S: MemSource + ?Sized>(
    src: &S,
    opts: &ScanOptions,
    missing: &[String],
    dest: &Path,
    report: &mut AssetReport,
) {
    let wanted: std::collections::HashSet<String> =
        missing.iter().filter(|m| m.to_ascii_lowercase().ends_with(".mdl")).map(|m| normalize_model_name(m)).collect();
    report.memory_models_wanted = wanted.len();
    if wanted.is_empty() {
        return;
    }
    let found = find_loaded_models(src, opts, &wanted);
    for m in missing {
        if !m.to_ascii_lowercase().ends_with(".mdl") {
            continue;
        }
        let Some(bytes) = found.get(&normalize_model_name(m)) else { continue };
        let vpath = if m.to_ascii_lowercase().starts_with("models/") { m.clone() } else { format!("models/{m}") };
        let Some(path) = hp_decompile::pak::safe_join(dest, &vpath.replace(char::from(92u8), "/")) else { continue };
        if let Some(dir) = path.parent()
            && std::fs::create_dir_all(dir).is_err()
        {
            continue;
        }
        if std::fs::write(&path, bytes).is_ok() {
            report.memory_models += 1;
            report.memory_model_bytes += bytes.len() as u64;
            *report.by_class.entry(AssetClass::Model.label().to_owned()).or_default() += 1;
        }
    }
    report
        .missing
        .retain(|m| !(m.to_ascii_lowercase().ends_with(".mdl") && found.contains_key(&normalize_model_name(m))));
}

pub fn content_dir_for(out: &Path, stem: &str) -> PathBuf {
    out.join(format!("{stem}_content"))
}

#[cfg(test)]
mod tests {
    use super::*;

    fn scratch(name: &str) -> PathBuf {
        let d = std::env::temp_dir().join(format!("hp_sniffer_assets_{name}_{}", std::process::id()));
        let _ = std::fs::remove_dir_all(&d);
        std::fs::create_dir_all(&d).expect("dir");
        d
    }

    fn bsp_with_pak() -> Bsp {
        let mut w = hp_bsp::PakWriter::new();
        w.add("materials/custom/wall.vmt", b"LightmappedGeneric {}".to_vec()).expect("add");
        w.add("materials/stock/brick.vmt", b"LightmappedGeneric {}".to_vec()).expect("add");
        w.add("models/custom/box.mdl", b"IDST".to_vec()).expect("add");
        w.add("sound/custom/hum.wav", b"RIFF".to_vec()).expect("add");
        w.add("scripts/custom.txt", b"\"x\" {}".to_vec()).expect("add");
        w.add("lua/autorun/x.lua", b"print(1)".to_vec()).expect("add");
        let zip = w.finish(hp_bsp::PakOptions::default()).expect("zip");
        Bsp::from_vec(hp_bsp::synthetic_bsp(20, &[(hp_bsp::lump::PAKFILE, zip)])).expect("bsp")
    }

    #[test]
    fn classes_follow_the_folder() {
        assert_eq!(class_of_path("models/a/b.mdl"), AssetClass::Model);
        assert_eq!(class_of_path("Materials/a.vmt"), AssetClass::Material);
        assert_eq!(class_of_path("sound/a.wav"), AssetClass::Sound);
        assert_eq!(class_of_path("scripts/a.txt"), AssetClass::Other);
        assert_eq!(class_of_dep(DepKind::ModelPart, "models/a.vvd"), AssetClass::Model);
        assert_eq!(class_of_dep(DepKind::Texture, "materials/a.vtf"), AssetClass::Material);
    }

    #[test]
    fn the_pakfile_is_sorted_into_what_the_user_asked_for() {
        let bsp = bsp_with_pak();
        let all = SnifferSettings::default();
        let dir = scratch("all");
        let mut r = AssetReport::default();
        extract_pak(&bsp, &dir, &all, None, &Default::default(), &mut r);
        assert_eq!(r.pak_files, 6, "{r:?}");
        assert!(dir.join("materials").join("custom").join("wall.vmt").is_file());
        assert!(dir.join("lua").join("autorun").join("x.lua").is_file());
        let _ = std::fs::remove_dir_all(&dir);

        let only_models =
            SnifferSettings { save_materials: false, save_sounds: false, save_other: false, ..all.clone() };
        let dir = scratch("models");
        let mut r = AssetReport::default();
        extract_pak(&bsp, &dir, &only_models, None, &Default::default(), &mut r);
        assert_eq!(r.pak_files, 1);
        assert!(dir.join("models").join("custom").join("box.mdl").is_file());
        assert!(!dir.join("materials").exists());
        let _ = std::fs::remove_dir_all(&dir);

        let dir = scratch("stock");
        let mut r = AssetReport::default();
        let stock = |p: &str| p.contains("stock/");
        extract_pak(&bsp, &dir, &all, Some(&stock), &Default::default(), &mut r);
        assert_eq!(r.pak_files, 5);
        assert_eq!(r.pak_skipped_stock, 1);
        assert!(!dir.join("materials").join("stock").exists());
        let keep = SnifferSettings { only_non_stock: false, ..all };
        let mut r = AssetReport::default();
        extract_pak(&bsp, &dir, &keep, Some(&stock), &Default::default(), &mut r);
        assert_eq!(r.pak_files, 6);
        let _ = std::fs::remove_dir_all(&dir);
    }

    #[test]
    fn roots_cover_materials_models_and_sounds() {
        let vmf = r#"
world
{
	"id" "1"
	"classname" "worldspawn"
	"skyname" "sky_day01_01"
}
entity
{
	"id" "2"
	"classname" "prop_static"
	"model" "models/custom/box.mdl"
	"origin" "0 0 0"
}
entity
{
	"id" "3"
	"classname" "ambient_generic"
	"message" "ambient/hum.wav"
	"origin" "0 0 0"
}
"#;
        let map = hp_vmf::parse(vmf).expect("vmf");
        let roots = roots_for(&map);
        assert!(roots.iter().any(|r| r.kind == RootKind::Model && r.name == "models/custom/box.mdl"));
        assert!(roots.iter().any(|r| r.kind == RootKind::Sound && r.name == "ambient/hum.wav"));
        assert!(roots.iter().any(|r| r.kind == RootKind::Skybox));
    }
}
