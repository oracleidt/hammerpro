use std::path::{Path, PathBuf};
use std::sync::Arc;
use std::sync::atomic::{AtomicU32, AtomicU64, Ordering};

use parking_lot::{Mutex, RwLock};

use crate::assets::AssetCaches;
use crate::custom::{CustomDir, CustomLayout};
use crate::detect::{is_gmod_root, path_key};
use crate::error::FsError;
use crate::material::MaterialCache;
use crate::mount::{
    DeferredMount, MountOptions, Opened, PlanItem, PlanKind, folder_name, game_folder_items, is_deferred, open_all_for,
    open_item, plan_gmod, vpk_label,
};
use crate::path::normalize_path;
use crate::progress::{MountProgress, MountStage};
use crate::source::{Source, SourceData, SourceInfo, SourceOrigin};

pub(crate) type Snapshot = Arc<[Arc<Source>]>;

#[derive(Clone, Debug, Default, PartialEq, Eq)]
pub struct CustomReport {
    pub vmts_checked: usize,
    pub missing_textures: Vec<(String, String)>,
    pub unsafe_names: Vec<String>,
}

impl CustomReport {
    pub const MAX_VMTS: usize = 4096;

    pub fn is_clean(&self) -> bool {
        self.missing_textures.is_empty() && self.unsafe_names.is_empty()
    }
}

#[derive(Clone)]
pub struct GameFs {
    pub(crate) inner: Arc<Inner>,
}

pub(crate) struct Inner {
    root: Option<PathBuf>,
    game_dir: Option<PathBuf>,
    lookup_only: bool,
    sources: RwLock<Snapshot>,
    mutate: Mutex<()>,
    next_id: AtomicU32,
    generation: AtomicU64,
    warnings: Mutex<Vec<String>>,
    pub(crate) assets: AssetCaches,
    pub(crate) materials: MaterialCache,
}

const _: fn() = || {
    fn assert_send_sync<T: Send + Sync>() {}
    assert_send_sync::<GameFs>();
};

impl std::fmt::Debug for GameFs {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("GameFs")
            .field("root", &self.inner.root)
            .field("sources", &self.snapshot().len())
            .field("generation", &self.generation())
            .finish_non_exhaustive()
    }
}

impl Default for GameFs {
    fn default() -> Self {
        Self::empty()
    }
}

enum Insert {
    AfterCustom,
    Bottom,
}

impl GameFs {
    pub fn from_source_mounts(
        root: Option<PathBuf>,
        game_dir: Option<PathBuf>,
        mounts: &[crate::SourceMount],
    ) -> Result<Self, FsError> {
        let fs = Self::with_dirs(root, game_dir);
        let mut opened = Vec::with_capacity(mounts.len());
        for mount in mounts {
            let kind = match &mount.kind {
                crate::SourceMountKind::Dir => PlanKind::GameDir,
                crate::SourceMountKind::Vpk => PlanKind::Vpk,
                crate::SourceMountKind::Gma => PlanKind::Gma { label_prefix: "Addon" },
                crate::SourceMountKind::Custom { prefix } => PlanKind::Custom(CustomDir {
                    path: mount.path.clone(),
                    prefix: prefix.clone(),
                    enabled: mount.enabled,
                }),
            };
            let item = PlanItem {
                kind,
                path: mount.path.clone(),
                label: mount.label.clone(),
                origin: mount.origin,
                enabled: mount.enabled,
            };
            let mut source = open_item(item)?;
            source.label = mount.label.clone();
            opened.push(source);
        }
        *fs.inner.sources.write() = opened.into_iter().map(|source| Arc::new(fs.make_source(source))).collect();
        Ok(fs)
    }

    pub fn empty() -> Self {
        Self::with_dirs(None, None)
    }

    fn with_dirs(root: Option<PathBuf>, game_dir: Option<PathBuf>) -> Self {
        Self {
            inner: Arc::new(Inner {
                root,
                game_dir,
                lookup_only: false,
                sources: RwLock::new(Arc::from(Vec::new())),
                mutate: Mutex::new(()),
                next_id: AtomicU32::new(1),
                generation: AtomicU64::new(1),
                warnings: Mutex::new(Vec::new()),
                assets: AssetCaches::default(),
                materials: MaterialCache::default(),
            }),
        }
    }

    pub fn for_gmod(root: &Path, opts: &MountOptions) -> Result<Self, FsError> {
        let (fs, rest) = Self::for_gmod_staged(root, opts)?;
        fs.finish_mount(rest);
        Ok(fs)
    }

    pub fn for_gmod_staged(root: &Path, opts: &MountOptions) -> Result<(Self, DeferredMount), FsError> {
        Self::for_gmod_staged_with(root, opts, &MountProgress::new())
    }

    pub fn for_gmod_staged_with(
        root: &Path,
        opts: &MountOptions,
        progress: &MountProgress,
    ) -> Result<(Self, DeferredMount), FsError> {
        progress.set_stage(MountStage::Planning);
        let start = std::time::Instant::now();
        let root = match root.parent() {
            Some(parent) if !is_gmod_root(root) && is_gmod_root(parent) && root.join("gameinfo.txt").is_file() => {
                parent
            }
            _ => root,
        };
        let (items, mut warnings) = plan_gmod(root, opts)?;
        let (late, items): (Vec<PlanItem>, Vec<PlanItem>) = items.into_iter().partition(|i| is_deferred(i.origin));
        let planned = start.elapsed();
        let (opened, open_warnings) = open_all_for(items, Some(progress), opts.lookup_only);
        log::info!(
            "mounted {} sources from {} in {:?} (planning {:?})",
            opened.len(),
            root.display(),
            start.elapsed(),
            planned
        );
        warnings.extend(open_warnings);
        let mut fs = Self::with_dirs(Some(root.to_path_buf()), Some(root.join("garrysmod")));
        Arc::get_mut(&mut fs.inner).expect("new filesystem").lookup_only = opts.lookup_only;
        let list: Vec<Arc<Source>> = opened.into_iter().map(|o| Arc::new(fs.make_source(o))).collect();
        *fs.inner.sources.write() = Arc::from(list);
        *fs.inner.warnings.lock() = warnings;
        Ok((fs, DeferredMount { items: late }))
    }

    pub fn for_game(profile: &crate::games::GameProfile, opts: &MountOptions) -> Result<Self, FsError> {
        let (fs, rest) = Self::for_game_staged_with(profile, opts, &MountProgress::new())?;
        fs.finish_mount(rest);
        Ok(fs)
    }

    pub fn for_game_staged_with(
        profile: &crate::games::GameProfile,
        opts: &MountOptions,
        progress: &MountProgress,
    ) -> Result<(Self, DeferredMount), FsError> {
        if profile.is_gmod() {
            return Self::for_gmod_staged_with(&profile.root, opts, progress);
        }
        progress.set_stage(MountStage::Planning);
        let start = std::time::Instant::now();
        let (items, mut warnings) = crate::games::plan_game(profile, opts)?;
        let (opened, open_warnings) = open_all_for(items, Some(progress), opts.lookup_only);
        log::info!(
            "mounted {} sources of {} from {} in {:?}",
            opened.len(),
            profile.name,
            profile.game_dir.display(),
            start.elapsed()
        );
        warnings.extend(open_warnings);
        let root = if profile.root.as_os_str().is_empty() {
            profile.game_dir.parent().map(Path::to_path_buf)
        } else {
            Some(profile.root.clone())
        };
        let mut fs = Self::with_dirs(root, Some(profile.game_dir.clone()));
        Arc::get_mut(&mut fs.inner).expect("new filesystem").lookup_only = opts.lookup_only;
        let list: Vec<Arc<Source>> = opened.into_iter().map(|o| Arc::new(fs.make_source(o))).collect();
        *fs.inner.sources.write() = Arc::from(list);
        *fs.inner.warnings.lock() = warnings;
        Ok((fs, DeferredMount::default()))
    }

    pub fn finish_mount(&self, rest: DeferredMount) -> usize {
        self.finish_mount_with(rest, &MountProgress::new())
    }

    pub fn finish_mount_with(&self, rest: DeferredMount, progress: &MountProgress) -> usize {
        if rest.is_empty() {
            return 0;
        }
        let start = std::time::Instant::now();
        let count = rest.len();
        let threads = std::thread::available_parallelism().map_or(4, |n| n.get()).clamp(2, 8);
        let pool = rayon::ThreadPoolBuilder::new()
            .num_threads(threads)
            .thread_name(|i| format!("hp-fs-mount-{i}"))
            .build()
            .map_err(|e| log::warn!("cannot start the mount threads: {e}"))
            .ok();
        let items = rest.items;
        let (opened, warnings) = match pool {
            Some(pool) => pool.install(|| open_all_for(items, Some(progress), self.inner.lookup_only)),
            None => open_all_for(items, Some(progress), self.inner.lookup_only),
        };
        let added = opened.len();
        self.insert(opened, Insert::Bottom);
        self.inner.warnings.lock().extend(warnings);
        log::info!("mounted {added} of {count} workshop and downloaded sources in {:?}", start.elapsed());
        added
    }

    pub fn root(&self) -> Option<&Path> {
        self.inner.root.as_deref()
    }

    pub fn game_dir(&self) -> Option<&Path> {
        self.inner.game_dir.as_deref()
    }

    pub fn generation(&self) -> u64 {
        self.inner.generation.load(Ordering::Acquire)
    }

    pub fn warnings(&self) -> Vec<String> {
        self.inner.warnings.lock().clone()
    }

    pub(crate) fn snapshot(&self) -> Snapshot {
        Arc::clone(&self.inner.sources.read())
    }

    fn bump(&self) {
        self.inner.generation.fetch_add(1, Ordering::AcqRel);
    }

    fn make_source(&self, o: Opened) -> Source {
        let id = self.inner.next_id.fetch_add(1, Ordering::Relaxed);
        Source::new(id, o.label, o.path, o.origin, o.enabled, o.data)
    }

    fn mutate<R>(&self, f: impl FnOnce(&mut Vec<Arc<Source>>) -> R) -> R {
        let _guard = self.inner.mutate.lock();
        let mut list = self.snapshot().to_vec();
        let result = f(&mut list);
        *self.inner.sources.write() = Arc::from(list);
        self.bump();
        result
    }

    fn insert(&self, opened: Vec<Opened>, at: Insert) -> Vec<u32> {
        let sources: Vec<Arc<Source>> = opened.into_iter().map(|o| Arc::new(self.make_source(o))).collect();
        let ids = sources.iter().map(|s| s.id).collect();
        self.mutate(|list| {
            let index = match at {
                Insert::Bottom => list.len(),
                Insert::AfterCustom => list.iter().rposition(|s| s.origin == SourceOrigin::Custom).map_or(0, |i| i + 1),
            };
            list.splice(index..index, sources);
        });
        ids
    }

    pub fn sources(&self) -> Vec<SourceInfo> {
        self.snapshot().iter().map(|s| s.info()).collect()
    }

    pub fn source(&self, id: u32) -> Option<SourceInfo> {
        self.snapshot().iter().find(|s| s.id == id).map(|s| s.info())
    }

    pub fn custom_dir_layout(&self, id: u32) -> Option<CustomLayout> {
        self.snapshot().iter().find(|s| s.id == id).and_then(|s| match &s.data {
            SourceData::Custom(c) => Some(c.layout().clone()),
            _ => None,
        })
    }

    pub fn custom_layout_of_path(&self, path: &Path) -> Option<CustomLayout> {
        let key = |p: &Path| {
            let s = p.to_string_lossy().replace('\\', "/");
            let s = s.trim_end_matches('/').to_owned();
            if cfg!(windows) { s.to_lowercase() } else { s }
        };
        let wanted = key(path);
        self.snapshot().iter().find_map(|s| match &s.data {
            SourceData::Custom(c) if key(c.root()) == wanted => Some(c.layout().clone()),
            _ => None,
        })
    }

    pub fn custom_dir_report(&self, id: u32) -> Option<CustomReport> {
        let snapshot = self.snapshot();
        let source = snapshot.iter().find(|s| s.id == id)?;
        let SourceData::Custom(index) = &source.data else { return None };
        let mut report = CustomReport { unsafe_names: index.unsafe_names(), ..CustomReport::default() };
        let mut vmts: Vec<&str> = index.vmt_files().collect();
        vmts.sort_unstable();
        for vpath in vmts.into_iter().take(CustomReport::MAX_VMTS) {
            report.vmts_checked += 1;
            let Some(material) = source.read(vpath).and_then(|b| hp_vmt::parse_bytes(&b).ok()) else { continue };
            let material = if material.is_patch() {
                hp_vmt::resolve_patch(&material, &|p| self.read_text(p)).unwrap_or(material)
            } else {
                material
            };
            let Some(texture) = material.base_texture().map(|t| hp_vmt::normalize_texture_path(&t)) else {
                continue;
            };
            if !texture.is_empty() && crate::texture::find_texture(&snapshot, &texture).is_none() {
                let name = crate::path::strip_extension(vpath.strip_prefix("materials/").unwrap_or(vpath));
                report.missing_textures.push((name.to_owned(), texture));
            }
        }
        Some(report)
    }

    pub fn set_source_enabled(&self, id: u32, enabled: bool) -> bool {
        let _guard = self.inner.mutate.lock();
        let Some(source) = self.snapshot().iter().find(|s| s.id == id).cloned() else {
            return false;
        };
        if source.enabled.swap(enabled, Ordering::AcqRel) != enabled {
            self.bump();
        }
        true
    }

    fn mounted_ids(&self, path: &Path, wanted: impl Fn(&SourceData) -> bool) -> Vec<u32> {
        let key = path_key(path);
        self.snapshot().iter().filter(|s| wanted(&s.data) && path_key(&s.path) == key).map(|s| s.id).collect()
    }

    pub fn add_custom_dir(&self, dir: CustomDir) -> Result<u32, FsError> {
        if let Some(id) = self.mounted_ids(&dir.path, |d| matches!(d, SourceData::Custom(_))).first() {
            return Ok(*id);
        }
        let opened = open_item(PlanItem::custom(&dir))?;
        Ok(self.insert(vec![opened], Insert::AfterCustom).first().copied().unwrap_or_default())
    }

    pub fn add_game_dir(&self, path: &Path) -> Result<Vec<u32>, FsError> {
        if !path.is_dir() {
            return Err(FsError::NotADirectory(path.to_path_buf()));
        }
        let mut warnings = Vec::new();
        let label = format!("Extra: {}", folder_name(path));
        let items = game_folder_items(path, SourceOrigin::Extra, &label, &mut warnings);
        let mut ids = Vec::new();
        let mut fresh = Vec::new();
        for item in items {
            let same = self.mounted_ids(&item.path, |d| matches!(d, SourceData::Dir(_) | SourceData::Vpk(_)));
            if same.is_empty() {
                fresh.push(item);
            } else {
                ids.extend(same);
            }
        }
        let (opened, open_warnings) = open_all_for(fresh, None, self.inner.lookup_only);
        warnings.extend(open_warnings);
        self.inner.warnings.lock().extend(warnings);
        ids.extend(self.insert(opened, Insert::Bottom));
        Ok(ids)
    }

    pub fn add_vpk(&self, path: &Path) -> Result<u32, FsError> {
        if let Some(id) = self.mounted_ids(path, |d| matches!(d, SourceData::Vpk(_))).first() {
            return Ok(*id);
        }
        let item = PlanItem {
            kind: PlanKind::Vpk,
            path: path.to_path_buf(),
            label: vpk_label(path),
            origin: SourceOrigin::Extra,
            enabled: true,
        };
        let opened = open_item(item)?;
        Ok(self.insert(vec![opened], Insert::Bottom).first().copied().unwrap_or_default())
    }

    pub fn add_gma(&self, path: &Path) -> Result<u32, FsError> {
        if let Some(id) = self.mounted_ids(path, |d| matches!(d, SourceData::Gma(_))).first() {
            return Ok(*id);
        }
        let item = PlanItem {
            kind: PlanKind::Gma { label_prefix: "Addon" },
            path: path.to_path_buf(),
            label: folder_name(path),
            origin: SourceOrigin::Extra,
            enabled: true,
        };
        let opened = open_item(item)?;
        Ok(self.insert(vec![opened], Insert::Bottom).first().copied().unwrap_or_default())
    }

    pub fn remove_source(&self, id: u32) -> bool {
        self.mutate(|list| {
            let before = list.len();
            list.retain(|s| s.id != id);
            list.len() != before
        })
    }

    pub fn refresh(&self) {
        self.refresh_where(|_| true);
    }

    pub fn refresh_sources(&self, ids: &[u32]) {
        if !ids.is_empty() {
            self.refresh_where(|s| ids.contains(&s.id));
        }
    }

    fn refresh_where(&self, wanted: impl Fn(&Source) -> bool + Sync) {
        use rayon::prelude::*;
        let mut fresh: std::collections::HashMap<u32, Source> =
            self.snapshot().par_iter().filter(|s| wanted(s)).filter_map(|s| s.reindexed()).map(|s| (s.id, s)).collect();
        if fresh.is_empty() {
            return;
        }
        self.mutate(|list| {
            for slot in list.iter_mut() {
                if let Some(new) = fresh.remove(&slot.id) {
                    new.enabled.store(slot.is_enabled(), Ordering::Release);
                    *slot = Arc::new(new);
                }
            }
        });
    }

    pub fn read(&self, path: &str) -> Option<Vec<u8>> {
        let vpath = normalize_path(path);
        self.snapshot().iter().filter(|s| s.is_enabled() && s.contains(&vpath)).find_map(|s| s.read(&vpath))
    }

    pub fn read_with<R>(&self, path: &str, f: impl FnOnce(&[u8]) -> R) -> Option<R> {
        let vpath = normalize_path(path);
        let snapshot = self.snapshot();
        let bytes =
            snapshot.iter().filter(|s| s.is_enabled() && s.contains(&vpath)).find_map(|s| s.read_cow(&vpath))?;
        Some(f(&bytes))
    }

    pub fn read_text(&self, path: &str) -> Option<String> {
        self.read(path).map(decode_text)
    }

    pub fn exists(&self, path: &str) -> bool {
        self.which_source(path).is_some()
    }

    pub fn which_source(&self, path: &str) -> Option<u32> {
        let vpath = normalize_path(path);
        self.snapshot().iter().find(|s| s.is_enabled() && s.contains(&vpath)).map(|s| s.id)
    }

    pub fn disk_path(&self, path: &str) -> Option<PathBuf> {
        let vpath = normalize_path(path);
        self.snapshot().iter().find(|s| s.is_enabled() && s.contains(&vpath)).and_then(|s| s.disk_path(&vpath))
    }

    pub fn watch_path(&self, path: &str) -> Option<(u32, PathBuf)> {
        let vpath = normalize_path(path);
        let snapshot = self.snapshot();
        let source = snapshot.iter().find(|s| s.is_enabled() && s.contains(&vpath))?;
        source.watch_path(&vpath).map(|p| (source.id, p))
    }
}

pub(crate) fn decode_text(bytes: Vec<u8>) -> String {
    let utf16 = |rest: &[u8], le: bool| {
        let units = rest.chunks_exact(2).map(|c| match c {
            [a, b] if le => u16::from_le_bytes([*a, *b]),
            [a, b] => u16::from_be_bytes([*a, *b]),
            _ => 0xFFFD,
        });
        char::decode_utf16(units).map(|r| r.unwrap_or(char::REPLACEMENT_CHARACTER)).collect()
    };
    match bytes.as_slice() {
        [0xFF, 0xFE, rest @ ..] => utf16(rest, true),
        [0xFE, 0xFF, rest @ ..] => utf16(rest, false),
        [0xEF, 0xBB, 0xBF, rest @ ..] => String::from_utf8_lossy(rest).into_owned(),
        _ => match String::from_utf8(bytes) {
            Ok(s) => s,
            Err(e) => String::from_utf8_lossy(e.as_bytes()).into_owned(),
        },
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::test_util::{TempDir, png};

    fn fake_gmod(tmp: &TempDir) -> PathBuf {
        tmp.write(
            "gmod/garrysmod/gameinfo.txt",
            br#""GameInfo" { FileSystem { SearchPaths {
                game+mod garrysmod/addons/*
                game+mod garrysmod/garrysmod.vpk
                game+game_write garrysmod
                game |all_source_engine_paths|sourceengine
            } } }"#,
        );
        tmp.write(
            "gmod/garrysmod/materials/Shared/Wall.vmt",
            b"\"LightmappedGeneric\" { \"$basetexture\" \"shared/wall\" }",
        );
        tmp.write("gmod/garrysmod/materials/game/only.vmt", b"\"UnlitGeneric\" { }");
        tmp.write("gmod/garrysmod/models/a.mdl", b"IDST");
        tmp.write("gmod/garrysmod/addons/MyAddon/materials/shared/wall.vmt", b"addon");
        tmp.write("gmod/garrysmod/addons/MyAddon/materials/addon/a.vmt", b"addon a");
        tmp.write("gmod/garrysmod/addons/MyAddon/sound/x/Beep.WAV", b"RIFF");
        tmp.write("gmod/sourceengine/materials/shared/wall.vmt", b"base");
        tmp.write("gmod/sourceengine/materials/base/b.vmt", b"base b");
        tmp.path().join("gmod")
    }

    #[test]
    fn mount_snapshot_preserves_priority_custom_prefixes_and_disabled_sources() {
        let tmp = TempDir::new("fs-snapshot");
        let root = fake_gmod(&tmp);
        let custom = tmp.mkdir("art");
        tmp.write("art/pixel.png", &png(2, 2, [7, 11, 13, 255]));
        let fs = GameFs::for_gmod(
            &root,
            &MountOptions {
                mount_workshop: false,
                custom_dirs: vec![CustomDir::with_prefix(&custom, "testprefix")],
                ..Default::default()
            },
        )
        .expect("mount");
        let addon = fs.sources().iter().find(|s| s.origin == SourceOrigin::Addon).expect("addon").id;
        fs.set_source_enabled(addon, false);
        let mounts: Vec<_> = fs.sources().iter().map(SourceInfo::mount).collect();
        let restored =
            GameFs::from_source_mounts(fs.root().map(Path::to_path_buf), fs.game_dir().map(Path::to_path_buf), &mounts)
                .expect("restore");
        assert_eq!(restored.root(), fs.root());
        assert_eq!(restored.game_dir(), fs.game_dir());
        assert_eq!(restored.sources().iter().map(SourceInfo::mount).collect::<Vec<_>>(), mounts);
        for path in
            ["materials/shared/wall.vmt", "models/a.mdl", "materials/custom/testprefix/pixel.vmt", "sound/x/beep.wav"]
        {
            assert_eq!(restored.read(path), fs.read(path), "{path}");
        }
    }

    #[test]
    fn compiler_lookup_mount_preserves_search_priority_and_extra_paths() {
        let tmp = TempDir::new("compiler-lookup-mount");
        let root = fake_gmod(&tmp);
        let indexed = GameFs::for_gmod(&root, &MountOptions { mount_workshop: false, ..Default::default() }).unwrap();
        let lookup =
            GameFs::for_gmod(&root, &MountOptions { lookup_only: true, mount_workshop: false, ..Default::default() })
                .unwrap();
        let extra = tmp.mkdir("extra");
        tmp.write("extra/materials/extra/new.vmt", b"extra");
        indexed.add_game_dir(&extra).unwrap();
        lookup.add_game_dir(&extra).unwrap();
        for path in [
            "materials/shared/wall.vmt",
            "materials/addon/a.vmt",
            "models/a.mdl",
            "sound/x/beep.wav",
            "materials/base/b.vmt",
            "materials/extra/new.vmt",
            "materials/absent.vmt",
        ] {
            assert_eq!(indexed.read(path), lookup.read(path), "{path}");
            assert_eq!(indexed.which_source(path), lookup.which_source(path), "{path}");
        }
        assert!(
            lookup
                .snapshot()
                .iter()
                .filter_map(|s| match &s.data {
                    SourceData::Dir(d) => Some(d.len()),
                    _ => None,
                })
                .all(|len| len == 0)
        );
    }

    #[test]
    fn mounts_in_priority_order_and_looks_up_case_insensitively() {
        let tmp = TempDir::new("fs-order");
        let root = fake_gmod(&tmp);
        let custom = tmp.mkdir("art");
        tmp.write("art/Shared/Wall.png", &png(2, 2, [1, 2, 3, 255]));
        let opts = MountOptions {
            mount_workshop: false,
            custom_dirs: vec![CustomDir::with_prefix(&custom, "")],
            ..MountOptions::default()
        };
        let fs = GameFs::for_gmod(&root, &opts).unwrap();
        let labels: Vec<String> = fs.sources().into_iter().map(|s| s.label).collect();
        assert_eq!(labels, ["Custom: art", "Addon: MyAddon", "garrysmod", "sourceengine"]);
        assert!(fs.warnings().iter().any(|w| w.contains("garrysmod.vpk")), "{:?}", fs.warnings());

        let ids: Vec<u32> = fs.sources().iter().map(|s| s.id).collect();
        let (custom_id, addon_id, game_id, base_id) = (ids[0], ids[1], ids[2], ids[3]);
        assert_eq!(fs.which_source("MATERIALS\\Shared\\wall.VMT"), Some(addon_id));
        assert_eq!(fs.read("materials/shared/wall.vmt").unwrap(), b"addon");
        assert_eq!(fs.which_source("materials/addon/a.vmt"), Some(addon_id));
        assert_eq!(fs.which_source("materials/base/b.vmt"), Some(base_id));
        assert_eq!(fs.which_source("sound/x/beep.wav"), Some(addon_id));
        assert!(fs.exists("models/A.mdl"));
        assert_eq!(fs.read_with("models/a.mdl", |b| b.len()), Some(4));
        assert_eq!(fs.read_with("models/none.mdl", |b| b.len()), None);
        assert!(!fs.exists("materials/missing.vmt"));
        assert!(fs.disk_path("materials/game/only.vmt").unwrap().ends_with("only.vmt"));

        assert_eq!(fs.which_source("materials/custom/art/shared/wall.vmt"), Some(custom_id));

        let generation = fs.generation();
        assert!(fs.set_source_enabled(addon_id, false));
        assert!(fs.generation() > generation);
        assert_eq!(
            fs.read("materials/shared/wall.vmt").unwrap(),
            b"\"LightmappedGeneric\" { \"$basetexture\" \"shared/wall\" }"
        );
        assert!(fs.set_source_enabled(game_id, false));
        assert_eq!(fs.read("materials/shared/wall.vmt").unwrap(), b"base");
        assert!(!fs.set_source_enabled(9999, false));

        assert!(fs.remove_source(base_id));
        assert!(!fs.remove_source(base_id));
        assert_eq!(fs.read("materials/shared/wall.vmt"), None);
        assert_eq!(fs.sources().len(), 3);
    }

    #[test]
    fn add_custom_dir_goes_after_custom_and_refresh_sees_new_files() {
        let tmp = TempDir::new("fs-add");
        let root = fake_gmod(&tmp);
        let fs = GameFs::for_gmod(
            &root,
            &MountOptions { mount_workshop: false, mount_addons: false, ..MountOptions::default() },
        )
        .unwrap();
        let first = tmp.mkdir("one/materials/shared");
        let second = tmp.mkdir("two");
        tmp.write("one/materials/shared/wall.vmt", b"custom one");
        tmp.write("two/Wall.vmt", b"custom two");
        let id1 = fs.add_custom_dir(CustomDir::new(&first)).unwrap();
        let id2 = fs.add_custom_dir(CustomDir::with_prefix(&second, "shared")).unwrap();
        let order: Vec<u32> = fs.sources().iter().map(|s| s.id).collect();
        assert_eq!(&order[..2], &[id1, id2]);
        assert_eq!(fs.read("materials/shared/wall.vmt").unwrap(), b"custom one");
        assert!(fs.set_source_enabled(id1, false));
        assert_eq!(fs.read("materials/shared/wall.vmt").unwrap(), b"custom two");

        assert!(!fs.exists("materials/shared/new.vmt"));
        tmp.write("two/New.vmt", b"new");
        tmp.write("gmod/garrysmod/materials/fresh.vmt", b"fresh");
        fs.refresh();
        assert!(fs.exists("materials/shared/new.vmt"));
        assert!(fs.exists("materials/fresh.vmt"));
        assert_eq!(fs.source(id1).map(|s| s.enabled), Some(false));
        assert!(fs.add_custom_dir(CustomDir::new(tmp.path().join("missing"))).is_err());
    }

    #[test]
    fn not_a_game_dir() {
        let tmp = TempDir::new("fs-bad");
        assert!(matches!(GameFs::for_gmod(tmp.path(), &MountOptions::default()), Err(FsError::NotAGameDir(_))));
        let empty = GameFs::empty();
        assert!(empty.sources().is_empty());
        assert_eq!(empty.read("a"), None);
    }

    #[test]
    fn text_decoding() {
        assert_eq!(decode_text(b"abc".to_vec()), "abc");
        assert_eq!(decode_text(b"\xEF\xBB\xBFabc".to_vec()), "abc");
        assert_eq!(decode_text(b"a\xFFb".to_vec()), "a\u{FFFD}b");
        assert_eq!(decode_text(vec![0xFF, 0xFE, b'h', 0, b'i', 0]), "hi");
        assert_eq!(decode_text(vec![0xFE, 0xFF, 0, b'h', 0, b'i', 0]), "hi");
        assert_eq!(decode_text(Vec::new()), "");
    }

    #[test]
    fn accepts_the_garrysmod_folder_as_root() {
        let tmp = TempDir::new("fs-root");
        let root = fake_gmod(&tmp);
        let opts = MountOptions { mount_workshop: false, ..MountOptions::default() };
        let fs = GameFs::for_gmod(&root.join("garrysmod"), &opts).unwrap();
        assert_eq!(fs.root(), Some(root.as_path()));
        assert_eq!(fs.game_dir(), Some(root.join("garrysmod").as_path()));
        assert!(fs.exists("materials/base/b.vmt"));
    }

    #[test]
    fn custom_folder_report_and_layout_lookup() {
        let tmp = TempDir::new("fs-report");
        tmp.write("g/materials/base/wall.vtf", b"VTF\0");
        tmp.write("c/ok.vmt", b"LightmappedGeneric { $basetexture base/wall }");
        tmp.write("c/broken.vmt", b"LightmappedGeneric { $basetexture nowhere/tex }");
        tmp.write("c/Bad Name.vmt", b"LightmappedGeneric { $basetexture base/wall }");
        tmp.write("c/Image Été.png", &png(2, 2, [1, 1, 1, 255]));
        let fs = GameFs::empty();
        fs.add_game_dir(&tmp.path().join("g")).unwrap();
        let id = fs.add_custom_dir(CustomDir::with_prefix(tmp.path().join("c"), "mine")).unwrap();
        let report = fs.custom_dir_report(id).unwrap();
        assert_eq!(report.vmts_checked, 3);
        assert_eq!(report.missing_textures, [("mine/broken".to_owned(), "nowhere/tex".to_owned())]);
        assert_eq!(report.unsafe_names, ["Bad Name.vmt"]);
        assert!(!report.is_clean());
        assert!(fs.custom_dir_report(id + 100).is_none());
        let layout = fs.custom_layout_of_path(&tmp.path().join("c/")).unwrap();
        assert_eq!(layout.prefix, "mine");
        assert!(fs.custom_layout_of_path(&tmp.path().join("g")).is_none());
    }

    #[test]
    fn watch_paths_and_partial_refresh() {
        let tmp = TempDir::new("fs-watch");
        tmp.write("g/materials/base/wall.vmt", b"LightmappedGeneric { $basetexture base/wall }");
        tmp.write("c/Brick.png", &png(2, 2, [1, 1, 1, 255]));
        let fs = GameFs::empty();
        let game = fs.add_game_dir(&tmp.path().join("g")).unwrap()[0];
        let custom = fs.add_custom_dir(CustomDir::with_prefix(tmp.path().join("c"), "mine")).unwrap();
        let (id, path) = fs.watch_path("Materials/Base/Wall.VMT").unwrap();
        assert_eq!(id, game);
        assert_eq!(path, tmp.path().join("g").join("materials/base/wall.vmt"));
        let (id, path) = fs.watch_path("materials/mine/brick.vmt").unwrap();
        assert_eq!(id, custom);
        assert_eq!(path, tmp.path().join("c").join("Brick.png"));
        assert!(fs.watch_path("materials/missing.vmt").is_none());
        tmp.write("c/Stone.png", &png(2, 2, [2, 2, 2, 255]));
        tmp.write("g/materials/base/new.vmt", b"x");
        let before = fs.generation();
        fs.refresh_sources(&[custom]);
        assert!(fs.generation() > before);
        assert!(fs.exists("materials/mine/stone.vmt"));
        assert!(!fs.exists("materials/base/new.vmt"), "the other source was not re-indexed");
        let before = fs.generation();
        fs.refresh_sources(&[]);
        fs.refresh_sources(&[9999]);
        assert_eq!(fs.generation(), before, "nothing to re-index changes nothing");
    }
}
