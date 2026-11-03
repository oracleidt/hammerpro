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
    pub memory_vvds: usize,
    pub memory_vvd_bytes: u64,
    pub memory_vvd_partial: usize,
    pub memory_vvd_missing_vertices: usize,
    pub memory_vvd_failed: usize,
    pub memory_vtxs: usize,
    pub memory_vtx_bytes: u64,
    pub memory_vtx_partial: usize,
    pub memory_vtx_failed: usize,
    pub memory_phys: usize,
    pub memory_phy_bytes: u64,
    pub memory_materials: usize,
    pub memory_material_bytes: u64,
    pub memory_materials_loaded: usize,
    pub textures_loaded: usize,
    pub textures_video_only: usize,
    pub textures_video_only_bytes: u64,
    pub pak_listed: usize,
    pub pak_listed_identical: usize,
    pub pak_listed_differs: usize,
    pub textures_with_file: usize,
    pub textures_resident_whole: usize,
    pub textures_resident_partial: usize,
    pub vtf_blobs_resident: usize,
    pub vtf_blobs_resident_bytes: u64,
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

const CRLF: &str = "\r\n";
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

pub fn base_name(name: &str) -> &str {
    name.rsplit('/').next().unwrap_or(name)
}

pub fn resolve_model_names(
    headers: &[(u64, String, usize)],
    wanted: &std::collections::HashSet<String>,
) -> BTreeMap<String, (u64, usize)> {
    let mut out: BTreeMap<String, (u64, usize)> = BTreeMap::new();
    for (addr, name, len) in headers {
        if wanted.contains(name) && !out.contains_key(name) {
            out.insert(name.clone(), (*addr, *len));
        }
    }
    let mut claimed: std::collections::HashSet<&str> = wanted.iter().map(String::as_str).collect();
    claimed.retain(|w| out.contains_key(*w));
    let mut taken: std::collections::HashSet<(String, usize)> = std::collections::HashSet::new();
    let mut ordered: Vec<&String> = wanted.iter().collect();
    ordered.sort();
    for w in ordered {
        if out.contains_key(w) {
            continue;
        }
        let mut found: Vec<&(u64, String, usize)> = headers
            .iter()
            .filter(|(_, n, _)| base_name(n) == base_name(w) && !claimed.contains(n.as_str()) && !wanted.contains(n))
            .collect();
        found.sort_by_key(|h| (h.2, h.0));
        found.dedup_by_key(|h| (h.1.clone(), h.2));
        if found.len() == 1 && taken.insert((found[0].1.clone(), found[0].2)) {
            out.insert(w.clone(), (found[0].0, found[0].2));
        }
    }
    out
}

pub fn find_loaded_models<S: MemSource + ?Sized>(
    src: &S,
    opts: &ScanOptions,
    wanted: &std::collections::HashSet<String>,
) -> BTreeMap<String, Vec<u8>> {
    let finder = memchr::memmem::Finder::new(b"IDST").into_owned();
    let opts = ScanOptions { overlap: 96, ..*opts };
    let mut headers: Vec<(u64, String, usize)> = par_chunks(src, &opts, |base, data, own| {
        let mut out = Vec::new();
        for at in finder.find_iter(data) {
            if at >= own {
                break;
            }
            let Some(head) = data.get(at..at + 80) else { continue };
            if let Some((name, len)) = mdl_header(head) {
                out.push((base + at as u64, name, len));
            }
        }
        out
    })
    .into_iter()
    .flatten()
    .collect();
    headers.sort();
    let mut chosen = resolve_model_names(&headers, wanted);
    if wanted.iter().any(|w| !chosen.contains_key(w)) {
        let entries = crate::studiomem::find_cache(src, &opts).map(|c| crate::studiomem::entries(src, c)).unwrap_or_default();
        let unresolved: Vec<&String> = wanted.iter().filter(|w| !chosen.contains_key(*w)).collect();
        for want in unresolved {
            let full = format!("models/{want}");
            let Some(entry) = entries.iter().find(|e| e.name == full) else { continue };
            let Some(hw) = crate::studiomem::read_hardware(src, entry.data) else { continue };
            let mut picks: Vec<(i32, u64, usize)> = Vec::new();
            for (addr, _, len) in headers.iter().filter(|(_, n, _)| base_name(n) == base_name(want)) {
                let bytes = src.read_vec(*addr, *len);
                if bytes.len() == *len && crate::studiomem::consistent(&bytes, &hw) {
                    let sum = i32::from_le_bytes([bytes[8], bytes[9], bytes[10], bytes[11]]);
                    if !picks.iter().any(|p| p.0 == sum) {
                        picks.push((sum, *addr, *len));
                    }
                }
            }
            if picks.len() == 1 {
                chosen.insert(want.clone(), (picks[0].1, picks[0].2));
            }
        }
    }
    let mut found: BTreeMap<String, Vec<u8>> = BTreeMap::new();
    for (want, (addr, len)) in chosen {
        let bytes = src.read_vec(addr, len);
        if bytes.len() == len && mdl_header(&bytes).is_some_and(|(_, l)| l == len) {
            found.insert(want, bytes);
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
    let mut recovered_vvd: std::collections::HashSet<String> = std::collections::HashSet::new();
    let blocks: std::cell::OnceCell<std::collections::BTreeMap<i32, crate::modelmem::RebuiltVvd>> = std::cell::OnceCell::new();
    let studio: std::cell::OnceCell<Vec<crate::studiomem::StudioEntry>> = std::cell::OnceCell::new();
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
            let vvd = recover_vvd(src, opts, &blocks, bytes, &path, &vpath, report, &mut recovered_vvd);
            let entries = studio.get_or_init(|| crate::studiomem::find_cache(src, opts).map(|c| crate::studiomem::entries(src, c)).unwrap_or_default());
            recover_studio(src, entries, bytes, &path, &vpath, vvd.as_deref(), report, &mut recovered_vvd);
        }
    }
    report
        .missing
        .retain(|m| !(m.to_ascii_lowercase().ends_with(".mdl") && found.contains_key(&normalize_model_name(m))));
    report.missing.retain(|m| !recovered_vvd.contains(&m.to_ascii_lowercase().replace(char::from(92u8), "/")));
}

pub fn recover_materials_from_memory<S: MemSource + ?Sized>(
    src: &S,
    opts: &ScanOptions,
    fs: &GameFs,
    dest: &Path,
    report: &mut AssetReport,
) {
    let found = crate::matmem::find_materials(src, opts);
    report.memory_materials_loaded = found.len();
    let mut seen: std::collections::HashSet<String> = std::collections::HashSet::new();
    for m in found {
        let Some(text) = m.vmt else { continue };
        let name = m.name.trim().replace(char::from(92u8), "/").to_ascii_lowercase();
        let vpath = format!("materials/{}.vmt", name.trim_start_matches('/'));
        if !seen.insert(vpath.clone()) || fs.exists(&vpath) {
            continue;
        }
        let Some(path) = hp_decompile::pak::safe_join(dest, &vpath) else { continue };
        if let Some(dir) = path.parent()
            && std::fs::create_dir_all(dir).is_err()
        {
            continue;
        }
        if std::fs::write(&path, text.as_bytes()).is_ok() {
            report.memory_materials += 1;
            report.memory_material_bytes += text.len() as u64;
            *report.by_class.entry(AssetClass::Material.label().to_owned()).or_default() += 1;
        }
    }
}

const RESIDENCY_MAX_BYTES: usize = 768 << 20;

fn residency<S: MemSource + ?Sized>(
    src: &S,
    opts: &ScanOptions,
    fs: &GameFs,
    found: &[crate::matmem::MemTexture],
    blobs: &[crate::texmem::VtfBlob],
    dest: &Path,
    report: &mut AssetReport,
) {
    let mut files: Vec<(String, Vec<u8>)> = Vec::new();
    let mut seen: std::collections::HashSet<String> = std::collections::HashSet::new();
    let mut total = 0usize;
    for t in found {
        let name = t.name.trim().replace(char::from(92u8), "/").to_ascii_lowercase();
        if !seen.insert(name.clone()) {
            continue;
        }
        let Some(data) = fs.read(&format!("materials/{}.vtf", name.trim_start_matches('/'))) else { continue };
        total += data.len();
        if total > RESIDENCY_MAX_BYTES {
            break;
        }
        files.push((name, data));
    }
    report.vtf_blobs_resident = blobs.len();
    report.vtf_blobs_resident_bytes = blobs.iter().map(|b| b.bytes.len() as u64).sum();
    let cov = crate::texmem::coverage(src, opts, &files, blobs);
    report.textures_with_file = cov.len();
    let mut lines: Vec<String> = Vec::new();
    for c in &cov {
        if c.whole {
            report.textures_resident_whole += 1;
        } else if c.found > 0 {
            report.textures_resident_partial += 1;
        }
        lines.push(format!(
            "materials/{}.vtf	{} of {} sampled windows resident ({:.1} percent)	{}",
            c.name,
            c.found,
            c.windows,
            c.fraction() * 100.0,
            if c.whole { "whole file resident" } else if c.found > 0 { "partly resident" } else { "video memory only" }
        ));
    }
    if lines.is_empty() {
        return;
    }
    lines.sort();
    let _ = std::fs::create_dir_all(dest);
    let _ = std::fs::write(dest.join("texture_residency.txt"), lines.join(CRLF) + CRLF);
}

pub fn list_video_only_textures<S: MemSource + ?Sized>(
    src: &S,
    opts: &ScanOptions,
    fs: &GameFs,
    dest: &Path,
    report: &mut AssetReport,
) {
    let found = crate::matmem::find_textures(src, opts);
    report.textures_loaded = found.len();
    let blobs = crate::texmem::find_blobs(src, opts);
    let mut lines: Vec<String> = Vec::new();
    let mut seen: std::collections::HashSet<String> = std::collections::HashSet::new();
    for t in found.iter() {
        let name = t.name.trim().replace(char::from(92u8), "/").to_ascii_lowercase();
        let vpath = format!("materials/{}.vtf", name.trim_start_matches('/'));
        if t.width == 0 || t.height == 0 || !seen.insert(vpath.clone()) || fs.exists(&vpath) {
            continue;
        }
        report.textures_video_only += 1;
        report.textures_video_only_bytes += t.bytes;
        let same = blobs
            .iter()
            .filter(|b| b.width == t.width && b.height == t.height && b.frames == t.frames && b.format == t.format && (b.faces == 6) == t.cube)
            .count();
        lines.push(format!(
            "{vpath}\t{}x{}\tformat {}\tflags {:#x}\tframes {}\t{}\t{} bytes\tresident whole files of the same size and format: {same}",
            t.width,
            t.height,
            t.format,
            t.flags,
            t.frames,
            if t.cube { "cubemap" } else { "texture" },
            t.bytes
        ));
    }
    residency(src, opts, fs, &found, &blobs, dest, report);
    if lines.is_empty() {
        return;
    }
    lines.sort();
    let _ = std::fs::create_dir_all(dest);
    let text = lines.join(CRLF) + CRLF;
    let _ = std::fs::write(dest.join("textures_in_video_memory.txt"), text);
}

#[allow(clippy::too_many_arguments)]
fn recover_vvd<S: MemSource + ?Sized>(
    src: &S,
    opts: &ScanOptions,
    blocks: &std::cell::OnceCell<std::collections::BTreeMap<i32, crate::modelmem::RebuiltVvd>>,
    mdl: &[u8],
    mdl_path: &Path,
    vpath: &str,
    report: &mut AssetReport,
    recovered: &mut std::collections::HashSet<String>,
) -> Option<Vec<u8>> {
    let checksum = crate::modelmem::mdl_checksum(mdl)?;
    let by_pointer = crate::modelmem::vertex_pointers_wide(mdl, src.pointer_width())
        .and_then(|ptrs| crate::modelmem::rebuild_vvd(src, ptrs, checksum))
        .filter(|v| v.missing_vertices == 0);
    let vvd = match by_pointer {
        Some(v) => v,
        None => {
            let map = blocks.get_or_init(|| crate::modelmem::find_vvd_blocks(src, opts));
            match map.get(&checksum) {
                Some(v) => v.clone(),
                None => {
                    report.memory_vvd_failed += 1;
                    return None;
                }
            }
        }
    };
    if vvd.missing_vertices > 0 {
        report.memory_vvd_partial += 1;
        report.memory_vvd_missing_vertices += vvd.missing_vertices;
    }
    let out = mdl_path.with_extension("vvd");
    if std::fs::write(&out, &vvd.bytes).is_ok() {
        report.memory_vvds += 1;
        report.memory_vvd_bytes += vvd.bytes.len() as u64;
        *report.by_class.entry(AssetClass::Model.label().to_owned()).or_default() += 1;
        let lower = vpath.to_ascii_lowercase().replace(char::from(92u8), "/");
        recovered.insert(format!("{}vvd", &lower[..lower.len() - 3]));
        return Some(vvd.bytes);
    }
    None
}

#[allow(clippy::too_many_arguments)]
fn recover_studio<S: MemSource + ?Sized>(
    src: &S,
    entries: &[crate::studiomem::StudioEntry],
    mdl: &[u8],
    mdl_path: &Path,
    vpath: &str,
    vvd: Option<&[u8]>,
    report: &mut AssetReport,
    recovered: &mut std::collections::HashSet<String>,
) {
    let full = crate::studiomem::normalize_name(vpath);
    let Some(entry) = entries.iter().find(|e| e.name == full) else { return };
    let stem = &full[..full.len() - 3];
    if let Some(hw) = crate::studiomem::read_hardware(src, entry.data) {
        match crate::studiomem::rebuild_vtx(mdl, &hw, vvd) {
            Some(vtx) => {
                let out = mdl_path.with_extension("dx90.vtx");
                if std::fs::write(&out, &vtx.bytes).is_ok() {
                    report.memory_vtxs += 1;
                    report.memory_vtx_bytes += vtx.bytes.len() as u64;
                    if vtx.stats.missing_lods > 0 || vtx.stats.unknown_vertices > 0 || vtx.stats.unresolved_bones > 0 {
                        report.memory_vtx_partial += 1;
                    }
                    *report.by_class.entry(AssetClass::Model.label().to_owned()).or_default() += 1;
                    recovered.insert(format!("{stem}dx90.vtx"));
                    for variant in ["dx80.vtx", "sw.vtx"] {
                        let wanted = format!("{stem}{variant}");
                        let listed = report.missing.iter().any(|m| m.to_ascii_lowercase().replace(char::from(92u8), "/") == wanted);
                        if listed && std::fs::write(mdl_path.with_extension(variant), &vtx.bytes).is_ok() {
                            report.memory_vtx_bytes += vtx.bytes.len() as u64;
                            recovered.insert(wanted);
                        }
                    }
                }
            }
            None => report.memory_vtx_failed += 1,
        }
    }
    let checksum = crate::modelmem::mdl_checksum(mdl).unwrap_or(0);
    if let Some(vc) = crate::phymem::parse_vcollide(src, entry.data + crate::studiomem::DATA_VCOLLIDE_AT) {
        let mut text = vc.text;
        crate::phymem::restore_name_case(&mut text, &crate::phymem::case_table(mdl));
        let phy = crate::phymem::rebuild_phy(&vc.solids, &text, checksum);
        if std::fs::write(mdl_path.with_extension("phy"), &phy).is_ok() {
            report.memory_phys += 1;
            report.memory_phy_bytes += phy.len() as u64;
            *report.by_class.entry(AssetClass::Model.label().to_owned()).or_default() += 1;
            recovered.insert(format!("{stem}phy"));
        }
    }
}

pub fn list_pak_directory<S: MemSource + ?Sized>(src: &S, opts: &ScanOptions, dest: &Path, report: &mut AssetReport) {
    let entries = crate::pakdir::find_directory(src, opts);
    if entries.is_empty() {
        return;
    }
    let rows = crate::pakdir::compare_in(&entries, dest);
    report.pak_listed = rows.len();
    report.pak_listed_identical = rows.iter().filter(|r| r.1 == crate::pakdir::Status::Identical).count();
    report.pak_listed_differs = rows.iter().filter(|r| r.1 == crate::pakdir::Status::Differs).count();
    let _ = std::fs::create_dir_all(dest);
    let _ = std::fs::write(dest.join("pakfile_listing.txt"), crate::pakdir::listing_text(&rows));
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

    #[test]
    fn a_model_whose_header_name_differs_is_matched_by_its_file_name() {
        let headers = vec![
            (0x1000u64, "anthon/door_new.mdl".to_owned(), 7980usize),
            (0x2000, "props_c17/oildrum001.mdl".to_owned(), 5000),
            (0x3000, "props_c17/oildrum001.mdl".to_owned(), 5000),
        ];
        let wanted: std::collections::HashSet<String> =
            ["hpcustom/door_new.mdl".to_owned(), "props_c17/oildrum001.mdl".to_owned(), "x/none.mdl".to_owned()]
                .into_iter()
                .collect();
        let got = resolve_model_names(&headers, &wanted);
        assert_eq!(got["hpcustom/door_new.mdl"], (0x1000, 7980));
        assert_eq!(got["props_c17/oildrum001.mdl"], (0x2000, 5000));
        assert!(!got.contains_key("x/none.mdl"));
        let twice: std::collections::HashSet<String> = ["a/door_new.mdl".to_owned(), "b/door_new.mdl".to_owned()].into_iter().collect();
        assert_eq!(resolve_model_names(&headers, &twice).len(), 1, "one header serves one wanted name");
    }
}
