use std::collections::HashSet;
use std::path::{Path, PathBuf};
use std::sync::Arc;
use std::time::{Duration, Instant};

use rayon::prelude::*;

use crate::custom::{CustomDir, CustomIndex};
use crate::detect::{detect_workshop_dir, path_key};
use crate::dirindex::DirIndex;
use crate::error::FsError;
use crate::progress::MountProgress;
use crate::source::{SourceData, SourceOrigin};

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct MountOptions {
    pub mount_addons: bool,
    pub mount_workshop: bool,
    pub mount_cfg: bool,
    pub mount_downloads: bool,
    pub extra_game_dirs: Vec<PathBuf>,
    pub custom_dirs: Vec<CustomDir>,
    pub app_ids: Vec<u32>,
    pub steam_libraries: Vec<PathBuf>,
}

impl Default for MountOptions {
    fn default() -> Self {
        Self {
            mount_addons: true,
            mount_workshop: true,
            mount_cfg: true,
            mount_downloads: false,
            extra_game_dirs: Vec::new(),
            custom_dirs: Vec::new(),
            app_ids: Vec::new(),
            steam_libraries: Vec::new(),
        }
    }
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub(crate) enum SearchPath {
    Vpk(PathBuf),
    Dir(PathBuf),
    Wildcard(PathBuf),
}

pub(crate) fn strip_prefix_ci<'a>(s: &'a str, prefix: &str) -> Option<&'a str> {
    let head = s.get(..prefix.len())?;
    head.eq_ignore_ascii_case(prefix).then(|| s.get(prefix.len()..)).flatten()
}

fn resolve_search_path(value: &str, root: &Path, game_dir: &Path) -> Option<SearchPath> {
    let value = value.trim();
    if value.is_empty() {
        return None;
    }
    let (base, rest) = if let Some(rest) = strip_prefix_ci(value, "|all_source_engine_paths|") {
        (root, rest)
    } else if let Some(rest) = strip_prefix_ci(value, "|gameinfo_path|") {
        (game_dir, rest)
    } else if Path::new(value).is_absolute() {
        (Path::new(""), value)
    } else {
        (root, value)
    };
    let rest = rest.trim_start_matches(['/', '\\']);
    let (rest, wildcard) = match rest.strip_suffix('*') {
        Some(r) => (r.trim_end_matches(['/', '\\']), true),
        None => (rest.trim_end_matches(['/', '\\']), false),
    };
    let path = if rest.is_empty() || rest == "." {
        base.to_path_buf()
    } else if base.as_os_str().is_empty() {
        PathBuf::from(rest)
    } else {
        base.join(rest)
    };
    Some(if wildcard {
        SearchPath::Wildcard(path)
    } else if rest.to_ascii_lowercase().ends_with(".vpk") {
        SearchPath::Vpk(path)
    } else {
        SearchPath::Dir(path)
    })
}

pub(crate) fn game_search_paths(gameinfo: &str, root: &Path, game_dir: &Path) -> Option<Vec<SearchPath>> {
    let kv = hp_kv::parse(gameinfo).ok()?;
    let paths = kv.child("GameInfo")?.child("FileSystem")?.child("SearchPaths")?;
    Some(
        paths
            .props
            .iter()
            .filter(|(key, _)| key.split('+').any(|t| t.trim().eq_ignore_ascii_case("game")))
            .filter_map(|(_, value)| resolve_search_path(value, root, game_dir))
            .collect(),
    )
}

fn default_search_paths(root: &Path, game_dir: &Path) -> Vec<SearchPath> {
    let se = root.join("sourceengine");
    vec![
        SearchPath::Wildcard(game_dir.join("addons")),
        SearchPath::Vpk(game_dir.join("garrysmod.vpk")),
        SearchPath::Vpk(se.join("hl2_textures.vpk")),
        SearchPath::Vpk(se.join("hl2_sound_vo_english.vpk")),
        SearchPath::Vpk(se.join("hl2_sound_misc.vpk")),
        SearchPath::Vpk(se.join("hl2_misc.vpk")),
        SearchPath::Dir(game_dir.to_path_buf()),
        SearchPath::Dir(se),
        SearchPath::Dir(game_dir.join("download")),
    ]
}

pub(crate) fn resolve_vpk(path: &Path) -> Option<PathBuf> {
    let name = path.file_name()?.to_string_lossy().into_owned();
    let lower = name.to_ascii_lowercase();
    if lower.ends_with("_dir.vpk") {
        return path.is_file().then(|| path.to_path_buf());
    }
    let stem = name.get(..name.len().checked_sub(4)?)?;
    let dir = path.with_file_name(format!("{stem}_dir.vpk"));
    if dir.is_file() { Some(dir) } else { path.is_file().then(|| path.to_path_buf()) }
}

pub(crate) fn vpk_label(path: &Path) -> String {
    let name = path.file_name().map(|n| n.to_string_lossy().into_owned()).unwrap_or_default();
    let lower = name.to_ascii_lowercase();
    if lower.ends_with("_dir.vpk") {
        let stem = name.get(..name.len() - "_dir.vpk".len()).unwrap_or(&name);
        format!("{stem}.vpk")
    } else {
        name
    }
}

pub(crate) fn folder_name(path: &Path) -> String {
    path.file_name().map(|n| n.to_string_lossy().into_owned()).unwrap_or_else(|| path.display().to_string())
}

pub(crate) fn parse_mount_cfg(text: &str) -> Vec<(String, PathBuf)> {
    let Ok(kv) = hp_kv::parse(text) else {
        return Vec::new();
    };
    let Some(block) = kv.child("mountcfg") else {
        return Vec::new();
    };
    block
        .props
        .iter()
        .filter(|(_, v)| !v.trim().is_empty())
        .map(|(k, v)| (k.clone(), PathBuf::from(v.trim())))
        .collect()
}

#[derive(Clone, Debug)]
pub(crate) enum PlanKind {
    GameDir,
    Vpk,
    Gma { label_prefix: &'static str },
    Custom(CustomDir),
}

#[derive(Clone, Debug)]
pub(crate) struct PlanItem {
    pub(crate) kind: PlanKind,
    pub(crate) path: PathBuf,
    pub(crate) label: String,
    pub(crate) origin: SourceOrigin,
    pub(crate) enabled: bool,
}

impl PlanItem {
    pub(crate) fn new(kind: PlanKind, path: PathBuf, label: String, origin: SourceOrigin) -> Self {
        Self { kind, path, label, origin, enabled: true }
    }

    pub(crate) fn custom(dir: &CustomDir) -> Self {
        Self {
            label: format!("Custom: {}", folder_name(&dir.path)),
            path: dir.path.clone(),
            kind: PlanKind::Custom(dir.clone()),
            origin: SourceOrigin::Custom,
            enabled: dir.enabled,
        }
    }
}

pub(crate) struct Opened {
    pub(crate) label: String,
    pub(crate) path: PathBuf,
    pub(crate) origin: SourceOrigin,
    pub(crate) enabled: bool,
    pub(crate) data: SourceData,
}

pub(crate) fn open_item(item: PlanItem) -> Result<Opened, FsError> {
    let mut label = item.label;
    let data = match item.kind {
        PlanKind::GameDir => SourceData::Dir(DirIndex::build_game_dir(&item.path)?),
        PlanKind::Vpk => SourceData::Vpk(Arc::new(hp_vpk::VpkArchive::open(&item.path)?)),
        PlanKind::Gma { label_prefix } => {
            let gma = hp_gma::GmaArchive::open(&item.path)?;
            let title = gma.name().trim();
            let title = if title.is_empty() { label.as_str() } else { title };
            label = format!("{label_prefix}: {title}");
            SourceData::Gma(Arc::new(gma))
        }
        PlanKind::Custom(dir) => SourceData::Custom(Box::new(CustomIndex::build(&dir)?)),
    };
    Ok(Opened { label, path: item.path, origin: item.origin, enabled: item.enabled, data })
}

pub(crate) fn open_all(items: Vec<PlanItem>) -> (Vec<Opened>, Vec<String>) {
    open_all_with(items, None)
}

type OpenResult = (String, Duration, Result<Opened, FsError>);

pub(crate) fn open_all_with(items: Vec<PlanItem>, progress: Option<&MountProgress>) -> (Vec<Opened>, Vec<String>) {
    if let Some(p) = progress {
        p.begin_opening(items.len());
    }
    let slow = progress.and(slow_mount());
    let run = |(i, item): (usize, PlanItem)| -> (usize, OpenResult) {
        let what = format!("{} ({})", item.label, item.path.display());
        if let Some(p) = progress {
            p.started(&progress_label(&item));
        }
        if let Some(step) = slow {
            std::thread::sleep(step * (i as u32 + 1));
        }
        let start = Instant::now();
        let result = open_item(item);
        if let Some(p) = progress {
            p.finished_one();
        }
        (i, (what, start.elapsed(), result))
    };
    type NumberedItems = Vec<(usize, PlanItem)>;
    let (sequential, parallel): (NumberedItems, NumberedItems) =
        items.into_iter().enumerate().partition(|(_, item)| !crate::scan::parallel_for(&item.path));
    let mut done: Vec<(usize, OpenResult)> = if sequential.is_empty() {
        parallel.into_par_iter().map(run).collect()
    } else {
        log::info!("{} sources are on a rotational drive and open one at a time", sequential.len());
        let (a, b) = rayon::join(
            || parallel.into_par_iter().map(&run).collect::<Vec<_>>(),
            || sequential.into_iter().map(&run).collect::<Vec<_>>(),
        );
        a.into_iter().chain(b).collect()
    };
    done.sort_by_key(|(i, _)| *i);
    let results: Vec<OpenResult> = done.into_iter().map(|(_, r)| r).collect();
    log_slowest(&results);
    let mut opened = Vec::with_capacity(results.len());
    let mut warnings = Vec::new();
    for (what, _, result) in results {
        match result {
            Ok(o) => opened.push(o),
            Err(e) => {
                log::warn!("cannot mount {what}: {e}");
                warnings.push(format!("cannot mount {what}: {e}"));
            }
        }
    }
    crate::scan::flush_index();
    (opened, warnings)
}

fn dedupe_class(kind: &PlanKind) -> u8 {
    match kind {
        PlanKind::GameDir | PlanKind::Custom(_) => 0,
        PlanKind::Vpk => 1,
        PlanKind::Gma { .. } => 2,
    }
}

fn is_game_folder(path: &Path) -> bool {
    std::fs::read_dir(path).is_ok_and(|read| {
        read.flatten().any(|entry| {
            entry.file_name().to_str().is_some_and(|n| n.eq_ignore_ascii_case("materials"))
                && std::fs::metadata(entry.path()).is_ok_and(|m| m.is_dir())
        })
    })
}

pub(crate) fn dedupe_items(items: Vec<PlanItem>) -> Vec<PlanItem> {
    let mut first: std::collections::HashMap<(u8, String), usize> = std::collections::HashMap::new();
    let mut out: Vec<PlanItem> = Vec::with_capacity(items.len());
    for item in items {
        let key = (dedupe_class(&item.kind), path_key(&item.path));
        let Some(&at) = first.get(&key) else {
            first.insert(key, out.len());
            out.push(item);
            continue;
        };
        let Some(kept) = out.get_mut(at) else { continue };
        let mixed = matches!(kept.kind, PlanKind::Custom(_)) != matches!(item.kind, PlanKind::Custom(_));
        if mixed && !is_game_folder(&item.path) {
            out.push(item);
            continue;
        }
        log::info!("{} ({}) is already mounted as {}: skipped", item.label, item.path.display(), kept.label);
        kept.enabled |= item.enabled;
    }
    out
}

fn slow_mount() -> Option<Duration> {
    static SLOW: std::sync::OnceLock<Option<Duration>> = std::sync::OnceLock::new();
    *SLOW.get_or_init(|| {
        let ms: u64 = std::env::var("HP_SLOW_MOUNT_MS").ok()?.trim().parse().ok()?;
        (ms > 0).then(|| Duration::from_millis(ms.min(5000)))
    })
}

fn progress_label(item: &PlanItem) -> String {
    match item.kind {
        PlanKind::Gma { .. } => folder_name(&item.path),
        _ => item.label.clone(),
    }
}

fn log_slowest<T>(results: &[(String, Duration, T)]) {
    let total: Duration = results.iter().map(|(_, d, _)| *d).sum();
    let mut slow: Vec<(&str, Duration)> =
        results.iter().filter(|(_, d, _)| *d >= SLOW_SOURCE).map(|(w, d, _)| (w.as_str(), *d)).collect();
    if slow.is_empty() {
        return;
    }
    slow.sort_by_key(|(_, d)| std::cmp::Reverse(*d));
    log::info!(
        "{} of {} sources took {:?} or more to open ({:?} of work in total); slowest:",
        slow.len(),
        results.len(),
        SLOW_SOURCE,
        total
    );
    for (what, d) in slow.iter().take(SLOWEST_LOGGED) {
        log::info!("  {d:?}  {what}");
    }
}

const SLOW_SOURCE: Duration = Duration::from_millis(500);
const SLOWEST_LOGGED: usize = 10;

#[derive(Debug, Default)]
pub struct DeferredMount {
    pub(crate) items: Vec<PlanItem>,
}

impl DeferredMount {
    pub fn len(&self) -> usize {
        self.items.len()
    }

    pub fn is_empty(&self) -> bool {
        self.items.is_empty()
    }
}

pub(crate) fn is_deferred(origin: SourceOrigin) -> bool {
    matches!(origin, SourceOrigin::Workshop | SourceOrigin::Download)
}

pub(crate) fn list_dir(path: &Path) -> Vec<(String, PathBuf, bool)> {
    let Ok(read) = std::fs::read_dir(path) else {
        return Vec::new();
    };
    let mut out: Vec<(String, PathBuf, bool)> = read
        .flatten()
        .filter_map(|e| {
            let name = e.file_name().into_string().ok()?;
            let path = e.path();
            let file_type = e.file_type().ok()?;
            let is_dir =
                if file_type.is_symlink() { std::fs::metadata(&path).ok()?.is_dir() } else { file_type.is_dir() };
            Some((name, path, is_dir))
        })
        .collect();
    out.sort_by_cached_key(|(name, _, _)| name.to_ascii_lowercase());
    out
}

pub(crate) fn has_ext(name: &str, ext: &str) -> bool {
    let lower = name.to_ascii_lowercase();
    lower.len() > ext.len() + 1
        && lower.ends_with(ext)
        && lower.as_bytes().get(lower.len() - ext.len() - 1) == Some(&b'.')
}

pub(crate) fn game_folder_items(
    path: &Path,
    origin: SourceOrigin,
    label: &str,
    warnings: &mut Vec<String>,
) -> Vec<PlanItem> {
    if !path.is_dir() {
        warnings.push(format!("{label}: folder not found: {}", path.display()));
        return Vec::new();
    }
    let mut items: Vec<PlanItem> = list_dir(path)
        .into_iter()
        .filter(|(name, _, is_dir)| !is_dir && name.to_ascii_lowercase().ends_with("_dir.vpk"))
        .map(|(_, p, _)| {
            let l = format!("{label} ({})", vpk_label(&p));
            PlanItem::new(PlanKind::Vpk, p, l, origin)
        })
        .collect();
    items.push(PlanItem::new(PlanKind::GameDir, path.to_path_buf(), label.to_owned(), origin));
    items
}

const BASE_GAMEMODES: &[&str] = &["base", "sandbox", "terrortown"];

pub(crate) fn plan_gmod(root: &Path, opts: &MountOptions) -> Result<(Vec<PlanItem>, Vec<String>), FsError> {
    let game_dir = root.join("garrysmod");
    if !game_dir.is_dir() {
        return Err(FsError::NotAGameDir(root.to_path_buf()));
    }
    let mut warnings = Vec::new();
    let gameinfo_path = game_dir.join("gameinfo.txt");
    let search = match std::fs::read(&gameinfo_path) {
        Ok(bytes) => game_search_paths(&String::from_utf8_lossy(&bytes), root, &game_dir),
        Err(e) => {
            warnings.push(format!("cannot read {}: {e}", gameinfo_path.display()));
            None
        }
    };
    let search = search.unwrap_or_else(|| default_search_paths(root, &game_dir));

    let game_key = path_key(&game_dir);
    let addons_dir = game_dir.join("addons");
    let addons_key = path_key(&addons_dir);
    let download_dir = game_dir.join("download");
    let download_key = path_key(&download_dir);

    let mut seen: HashSet<String> = HashSet::new();
    let mut base_vpks: Vec<PathBuf> = Vec::new();
    let mut base_dirs: Vec<PathBuf> = Vec::new();
    let mut wildcards: Vec<PathBuf> = Vec::new();
    for sp in search {
        match sp {
            SearchPath::Vpk(p) => match resolve_vpk(&p) {
                Some(dir) => {
                    if seen.insert(path_key(&dir)) {
                        base_vpks.push(dir);
                    }
                }
                None => warnings.push(format!("VPK listed in gameinfo.txt not found: {}", p.display())),
            },
            SearchPath::Dir(p) => {
                let key = path_key(&p);
                if key != game_key && key != download_key && p.is_dir() && seen.insert(key) {
                    base_dirs.push(p);
                }
            }
            SearchPath::Wildcard(p) => {
                if path_key(&p) != addons_key {
                    wildcards.push(p);
                }
            }
        }
    }
    let fallbacks = game_dir.join("fallbacks_dir.vpk");
    let fallbacks_key = path_key(&fallbacks);
    if fallbacks.is_file() && seen.insert(path_key(&fallbacks)) {
        base_vpks.push(fallbacks);
    }
    for (name, p, is_dir) in list_dir(&root.join("sourceengine")) {
        if !is_dir && name.to_ascii_lowercase().ends_with("_dir.vpk") && seen.insert(path_key(&p)) {
            base_vpks.push(p);
        }
    }

    let mut items = Vec::new();
    items.extend(opts.custom_dirs.iter().map(PlanItem::custom));

    if opts.mount_addons {
        for (name, p, is_dir) in list_dir(&addons_dir) {
            if is_dir {
                items.push(PlanItem::new(PlanKind::GameDir, p, format!("Addon: {name}"), SourceOrigin::Addon));
            } else if has_ext(&name, "gma") {
                let stem = name.get(..name.len() - 4).unwrap_or(&name).to_owned();
                items.push(PlanItem::new(PlanKind::Gma { label_prefix: "Addon" }, p, stem, SourceOrigin::Addon));
            }
        }
        for dir in &wildcards {
            for (name, p, is_dir) in list_dir(dir) {
                if is_dir {
                    items.push(PlanItem::new(PlanKind::GameDir, p, format!("Addon: {name}"), SourceOrigin::Addon));
                }
            }
        }
    }
    let (own_vpks, other_vpks): (Vec<PathBuf>, Vec<PathBuf>) = base_vpks
        .into_iter()
        .partition(|p| p.parent().is_some_and(|d| path_key(d) == game_key) && path_key(p) != fallbacks_key);
    for p in own_vpks {
        let label = vpk_label(&p);
        items.push(PlanItem::new(PlanKind::Vpk, p, label, SourceOrigin::BaseGame));
    }
    items.push(PlanItem::new(PlanKind::GameDir, game_dir.clone(), "garrysmod".to_owned(), SourceOrigin::GameLoose));
    for p in other_vpks {
        let label = vpk_label(&p);
        items.push(PlanItem::new(PlanKind::Vpk, p, label, SourceOrigin::BaseGame));
    }
    for (name, p, is_dir) in list_dir(&game_dir.join("gamemodes")) {
        let content = p.join("content");
        if is_dir && content.is_dir() {
            let origin = if BASE_GAMEMODES.iter().any(|g| g.eq_ignore_ascii_case(&name)) {
                SourceOrigin::BaseGame
            } else {
                SourceOrigin::GameLoose
            };
            items.push(PlanItem::new(PlanKind::GameDir, content, format!("Gamemode: {name}"), origin));
        }
    }
    for p in base_dirs {
        let label = folder_name(&p);
        items.push(PlanItem::new(PlanKind::GameDir, p, label, SourceOrigin::BaseGame));
    }
    if opts.mount_cfg {
        let cfg = game_dir.join("cfg").join("mount.cfg");
        if let Ok(bytes) = std::fs::read(&cfg) {
            for (key, path) in parse_mount_cfg(&String::from_utf8_lossy(&bytes)) {
                let label = format!("Mount: {key}");
                items.extend(game_folder_items(&path, SourceOrigin::Mounted, &label, &mut warnings));
            }
        }
    }
    for path in &opts.extra_game_dirs {
        let label = format!("Extra: {}", folder_name(path));
        items.extend(game_folder_items(path, SourceOrigin::Extra, &label, &mut warnings));
    }
    let apps = crate::apps::app_items(opts, &mut warnings);
    crate::apps::append_unique(&mut items, apps);
    if opts.mount_workshop
        && let Some(workshop) = detect_workshop_dir(root)
    {
        items.extend(workshop_items(&workshop));
    }
    if opts.mount_downloads {
        if download_dir.is_dir() {
            items.push(PlanItem::new(PlanKind::GameDir, download_dir, "Downloads".to_owned(), SourceOrigin::Download));
        }
        for (name, p, is_dir) in list_dir(&game_dir.join("cache").join("workshop")) {
            if !is_dir && has_ext(&name, "gma") {
                let stem = name.get(..name.len() - 4).unwrap_or(&name).to_owned();
                items.push(PlanItem::new(PlanKind::Gma { label_prefix: "Server" }, p, stem, SourceOrigin::Download));
            }
        }
    }
    let items = crate::mount::dedupe_items(items);
    Ok((items, warnings))
}

pub(crate) fn workshop_items(workshop: &Path) -> Vec<PlanItem> {
    let mut folders: Vec<(String, PathBuf, bool)> = list_dir(workshop).into_iter().filter(|(_, _, d)| *d).collect();
    folders.sort_by_key(|(name, _, _)| (name.parse::<u64>().unwrap_or(u64::MAX), name.clone()));
    let per_folder: Vec<Vec<PlanItem>> = folders
        .par_iter()
        .map(|(id, folder, _)| {
            list_dir(folder)
                .into_iter()
                .filter(|(name, _, is_dir)| !is_dir && (has_ext(name, "gma") || has_ext(name, "bin")))
                .map(|(_, p, _)| {
                    PlanItem::new(PlanKind::Gma { label_prefix: "Workshop" }, p, id.clone(), SourceOrigin::Workshop)
                })
                .collect()
        })
        .collect();
    per_folder.into_iter().flatten().collect()
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::test_util::TempDir;

    const GMOD_GAMEINFO: &str = r#""GameInfo"
{
	game	"Garry's Mod"
	FileSystem
	{
		SteamAppId				4000
		SearchPaths
		{
			game+mod			garrysmod/addons/*
			game+mod			garrysmod/garrysmod.vpk
			game				|all_source_engine_paths|sourceengine/hl2_textures.vpk
			platform			|all_source_engine_paths|platform/platform_misc.vpk
			mod+mod_write+default_write_path		|gameinfo_path|.
			game+game_write		garrysmod
			gamebin				garrysmod/bin
			game				|all_source_engine_paths|sourceengine
			game+download		garrysmod/download
		}
	}
}"#;

    #[test]
    fn gameinfo_search_paths() {
        let root = Path::new("R");
        let game = root.join("garrysmod");
        let paths = game_search_paths(GMOD_GAMEINFO, root, &game).unwrap();
        assert_eq!(
            paths,
            vec![
                SearchPath::Wildcard(root.join("garrysmod/addons")),
                SearchPath::Vpk(root.join("garrysmod/garrysmod.vpk")),
                SearchPath::Vpk(root.join("sourceengine/hl2_textures.vpk")),
                SearchPath::Dir(root.join("garrysmod")),
                SearchPath::Dir(root.join("sourceengine")),
                SearchPath::Dir(root.join("garrysmod/download")),
            ]
        );
        assert!(game_search_paths("\"GameInfo\" { }", root, &game).is_none());
        let gi = "GameInfo { FileSystem { SearchPaths { game |gameinfo_path|. game |GAMEINFO_PATH|sub/x.vpk } } }";
        assert_eq!(
            game_search_paths(gi, root, &game).unwrap(),
            vec![SearchPath::Dir(game.clone()), SearchPath::Vpk(game.join("sub/x.vpk"))]
        );
    }

    #[test]
    fn vpk_resolution_and_labels() {
        let tmp = TempDir::new("vpk-resolve");
        tmp.write("pak01_dir.vpk", b"");
        tmp.write("single.vpk", b"");
        assert_eq!(resolve_vpk(&tmp.path().join("pak01.vpk")), Some(tmp.path().join("pak01_dir.vpk")));
        assert_eq!(resolve_vpk(&tmp.path().join("pak01_dir.vpk")), Some(tmp.path().join("pak01_dir.vpk")));
        assert_eq!(resolve_vpk(&tmp.path().join("single.vpk")), Some(tmp.path().join("single.vpk")));
        assert_eq!(resolve_vpk(&tmp.path().join("missing.vpk")), None);
        assert_eq!(vpk_label(Path::new("x/garrysmod_dir.vpk")), "garrysmod.vpk");
        assert_eq!(vpk_label(Path::new("x/Single.vpk")), "Single.vpk");
    }

    #[test]
    fn mount_cfg_entries() {
        let text =
            "\"mountcfg\"\n{\n//\t\"tf\" \"C:\\tf\"\n\t\"cstrike\"\t\"C:\\steamcmd\\cstrike\"\n\t\"empty\" \"\"\n}\n";
        let entries = parse_mount_cfg(text);
        assert_eq!(entries, vec![("cstrike".to_owned(), PathBuf::from(r"C:\steamcmd\cstrike"))]);
        assert!(parse_mount_cfg("nothing").is_empty());
    }

    #[test]
    fn extension_check() {
        assert!(has_ext("a.GMA", "gma"));
        assert!(!has_ext("gma", "gma"));
        assert!(!has_ext("agma", "gma"));
        assert!(has_ext("x_legacy.bin", "bin"));
    }
}
