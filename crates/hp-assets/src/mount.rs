use std::path::{Path, PathBuf};
use std::sync::Arc;

use hp_app::settings::Settings;
use hp_fs::{CustomDir, GameFs, MountOptions};

use crate::jobs::{AssetLists, MountData, MountOutcome};

#[derive(Clone, Debug, PartialEq)]
pub(crate) struct MountRequest {
    pub root: Option<PathBuf>,
    pub opts: MountOptions,
    pub game: Option<GameRef>,
    pub scan: hp_fs::ScanConfig,
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub(crate) struct GameRef {
    pub id: String,
    pub dir: Option<PathBuf>,
}

impl GameRef {
    pub(crate) fn of(settings: &Settings) -> Option<GameRef> {
        let g = &settings.game;
        (!g.is_gmod()).then(|| GameRef {
            id: g.active_game().to_owned(),
            dir: g.game_dir_of(g.active_game()).map(Path::to_path_buf),
        })
    }

    pub(crate) fn profile(&self) -> Option<hp_fs::GameProfile> {
        hp_fs::resolve_game(&self.id, self.dir.as_deref())
    }
}

impl MountRequest {
    pub(crate) fn from_settings(settings: &Settings) -> Self {
        let g = &settings.game;
        let custom_dirs = g
            .custom_texture_dirs
            .iter()
            .filter(|d| !d.path.as_os_str().is_empty())
            .map(|d| {
                let prefix = if d.prefix.trim().is_empty() {
                    hp_fs::suggest_prefix(&d.path)
                } else {
                    d.prefix.trim().to_owned()
                };
                CustomDir { path: d.path.clone(), prefix, enabled: d.enabled, include_models: d.include_models }
            })
            .collect();
        Self {
            root: g.gmod_root.clone().filter(|p| !p.as_os_str().is_empty()),
            opts: MountOptions {
                lookup_only: false,
                mount_addons: g.mount_addons,
                mount_workshop: g.mount_workshop,
                mount_cfg: g.mount_cfg,
                mount_downloads: g.mount_downloads,
                extra_game_dirs: g.extra_game_dirs.clone(),
                custom_dirs,
                app_ids: g.mount_app_ids.iter().copied().filter(|id| *id > 0).collect(),
                steam_libraries: Vec::new(),
            },
            game: GameRef::of(settings),
            scan: settings.scan.to_config(),
        }
    }
}

pub(crate) fn install_root(path: &Path) -> PathBuf {
    if !path.join("garrysmod").is_dir()
        && path.file_name().is_some_and(|n| n.eq_ignore_ascii_case("garrysmod"))
        && let Some(parent) = path.parent()
    {
        return parent.to_path_buf();
    }
    path.to_path_buf()
}

const HAMMERPP_DIRS: [&[&str]; 2] = [&["bin", "win64", "hammerplusplus"], &["bin", "hammerplusplus"]];

const HAMMERPP_FOLDER: &str = "hammerplusplus";

pub(crate) fn hammerpp_dir(root: &Path) -> Option<PathBuf> {
    HAMMERPP_DIRS
        .iter()
        .map(|parts| parts.iter().fold(root.to_path_buf(), |p, c| p.join(c)))
        .find(|dir| dir.join("hammerplusplus_fgd.fgd").is_file())
}

pub(crate) fn is_editor_only_source(path: &Path) -> bool {
    path.file_name().is_some_and(|n| n.eq_ignore_ascii_case(HAMMERPP_FOLDER))
        && path
            .parent()
            .and_then(Path::file_name)
            .is_some_and(|n| n.eq_ignore_ascii_case("win64") || n.eq_ignore_ascii_case("bin"))
}

pub(crate) fn mount_hammerpp(fs: &GameFs, root: &Path) {
    if let Some(dir) = hammerpp_dir(root)
        && let Err(e) = fs.add_game_dir(&dir)
    {
        log::warn!("mount: Hammer++ folder {}: {e}", dir.display());
    }
}

pub(crate) fn run_mount(req: &MountRequest, progress: &hp_fs::MountProgress) -> MountOutcome {
    hp_fs::configure_scan(req.scan.clone());
    if let Some(profile) = req.game.as_ref().and_then(GameRef::profile).filter(|p| p.installed) {
        return run_game_mount(&profile, &req.opts, progress);
    }
    let start = std::time::Instant::now();
    let root = match req.root.clone().or_else(hp_fs::detect_gmod_root) {
        Some(r) => install_root(&r),
        None => return MountOutcome::NotConfigured,
    };
    match GameFs::for_gmod_staged_with(&root, &req.opts, progress) {
        Ok((fs, rest)) => {
            mount_hammerpp(&fs, &root);
            progress.set_stage(hp_fs::MountStage::Indexing);
            for w in fs.warnings() {
                log::warn!("mount: {w}");
            }
            let lists = AssetLists::build(&fs);
            log::info!(
                "game content mounted from {} in {:?}: {} materials, {} models ({} workshop and downloaded sources \
                 still loading)",
                root.display(),
                start.elapsed(),
                lists.materials.len(),
                lists.models.len(),
                rest.len()
            );
            MountOutcome::Mounted(Box::new(MountData { root, fs, lists, rest }))
        }
        Err(e) => MountOutcome::Failed(e.to_string()),
    }
}

fn run_game_mount(profile: &hp_fs::GameProfile, opts: &MountOptions, progress: &hp_fs::MountProgress) -> MountOutcome {
    let start = std::time::Instant::now();
    match GameFs::for_game_staged_with(profile, opts, progress) {
        Ok((fs, rest)) => {
            mount_hammerpp(&fs, &profile.root);
            progress.set_stage(hp_fs::MountStage::Indexing);
            for w in fs.warnings() {
                log::warn!("mount: {w}");
            }
            let lists = AssetLists::build(&fs);
            log::info!(
                "{} content mounted from {} in {:?}: {} materials, {} models",
                profile.name,
                profile.game_dir.display(),
                start.elapsed(),
                lists.materials.len(),
                lists.models.len()
            );
            MountOutcome::Mounted(Box::new(MountData { root: profile.root.clone(), fs, lists, rest }))
        }
        Err(e) => MountOutcome::Failed(e.to_string()),
    }
}

#[derive(Clone, Debug, PartialEq)]
pub(crate) struct FgdRequest {
    pub root: Option<PathBuf>,
    pub override_fgd: Option<PathBuf>,
    pub extra: Vec<PathBuf>,
    pub game: Option<GameRef>,
}

impl FgdRequest {
    pub(crate) fn from_settings(settings: &Settings) -> Self {
        let g = &settings.game;
        Self {
            root: g.gmod_root.clone().filter(|p| !p.as_os_str().is_empty()),
            override_fgd: g.fgd_override.clone().filter(|p| !p.as_os_str().is_empty()),
            extra: g.extra_fgds.clone(),
            game: GameRef::of(settings),
        }
    }
}

pub(crate) fn fgd_paths(
    root: Option<&Path>,
    override_fgd: Option<&Path>,
    extra: &[PathBuf],
) -> (Vec<PathBuf>, Vec<String>) {
    let mut paths: Vec<PathBuf> = Vec::new();
    let mut warnings = Vec::new();
    let push = |p: PathBuf, paths: &mut Vec<PathBuf>| {
        if !paths.iter().any(|q| q == &p) {
            paths.push(p);
        }
    };
    match override_fgd {
        Some(o) if o.is_file() => push(o.to_path_buf(), &mut paths),
        Some(o) => warnings.push(format!("FGD override not found: {}", o.display())),
        None => {
            if let Some(root) = root {
                let base = root.join("bin").join("garrysmod.fgd");
                if base.is_file() {
                    push(base, &mut paths);
                } else {
                    warnings.push(format!("game FGD not found: {}", base.display()));
                }
            }
        }
    }
    if let Some(dir) = root.and_then(hammerpp_dir) {
        push(dir.join("hammerplusplus_fgd.fgd"), &mut paths);
    }
    for e in extra {
        if e.is_file() {
            push(e.clone(), &mut paths);
        } else if !e.as_os_str().is_empty() {
            warnings.push(format!("FGD not found: {}", e.display()));
        }
    }
    (paths, warnings)
}

pub(crate) fn game_fgd_paths(
    p: &hp_fs::GameProfile,
    override_fgd: Option<&Path>,
    extra: &[PathBuf],
) -> (Vec<PathBuf>, Vec<String>) {
    let mut paths: Vec<PathBuf> = Vec::new();
    let mut warnings = Vec::new();
    match override_fgd {
        Some(o) if o.is_file() => paths.push(o.to_path_buf()),
        Some(o) => warnings.push(format!("FGD override not found: {}", o.display())),
        None if p.fgds.is_empty() => warnings.push(format!("game FGD of {} not found", p.name)),
        None => paths.extend(p.fgds.iter().cloned()),
    }
    if !p.root.as_os_str().is_empty()
        && let Some(dir) = hammerpp_dir(&p.root)
    {
        let f = dir.join("hammerplusplus_fgd.fgd");
        if !paths.contains(&f) {
            paths.push(f);
        }
    }
    for e in extra {
        if e.is_file() {
            if !paths.contains(e) {
                paths.push(e.clone());
            }
        } else if !e.as_os_str().is_empty() {
            warnings.push(format!("FGD not found: {}", e.display()));
        }
    }
    (paths, warnings)
}

pub(crate) struct FgdOutcome {
    pub fgd: Option<Arc<hp_fgd::ResolvedFgd>>,
    pub warnings: Vec<String>,
    pub notes: Vec<String>,
    pub files: Vec<PathBuf>,
}

pub(crate) fn run_fgd(req: &FgdRequest) -> FgdOutcome {
    let profile = req.game.as_ref().and_then(GameRef::profile).filter(|p| p.installed);
    let ((paths, mut warnings), include_dirs) = match &profile {
        Some(p) => (game_fgd_paths(p, req.override_fgd.as_deref(), &req.extra), p.fgd_dirs.clone()),
        None => {
            let root = req.root.clone().or_else(hp_fs::detect_gmod_root).map(|r| install_root(&r));
            let found = fgd_paths(root.as_deref(), req.override_fgd.as_deref(), &req.extra);
            let dirs: Vec<PathBuf> = root.iter().map(|r| r.join("bin")).collect();
            (found, dirs)
        }
    };
    let gmod = profile.is_none();
    if paths.is_empty() {
        return FgdOutcome { fgd: None, warnings, notes: Vec::new(), files: Vec::new() };
    }
    let fgd = match hp_fgd::Fgd::load_with_include_dirs(&paths, &include_dirs) {
        Ok(f) => f,
        Err(e) => {
            warnings.push(e.to_string());
            let mut merged = hp_fgd::Fgd::default();
            for p in &paths {
                match hp_fgd::Fgd::load_with_include_dirs([p], &include_dirs) {
                    Ok(f) => merged.merge(f),
                    Err(e) => warnings.push(e.to_string()),
                }
            }
            merged
        }
    };
    let mut fgd = fgd;
    fgd.merge(hp_fgd::Fgd::parse(hp_editor::sky::BUILTIN_FGD, None));
    let mut notes = fgd.warnings.clone();
    let mut resolved = fgd.resolve();
    if !gmod {
        resolved.remove_class(hp_editor::sky::SWITCH_CLASS);
    }
    notes.extend(resolved.warnings.iter().cloned());
    for n in &notes {
        log::debug!("FGD: {n}");
    }
    log::info!("FGD loaded: {} classes from {} files", resolved.len(), paths.len());
    FgdOutcome { fgd: Some(Arc::new(resolved)), warnings, notes, files: paths }
}

pub(crate) fn fgd_note_lines(notes: &[String], max: usize) -> Vec<String> {
    let mut out: Vec<String> = notes.iter().take(max).map(|n| hp_app::tf!("assets.fgd_note", n)).collect();
    if notes.len() > max {
        out.push(hp_app::tf!("assets.fgd_more", notes.len() - max));
    }
    out
}

pub(crate) fn sources_line(sources: &[hp_fs::SourceInfo], max: usize) -> String {
    let mut parts: Vec<String> = Vec::new();
    let mut workshop = 0usize;
    let flush = |workshop: &mut usize, parts: &mut Vec<String>| {
        if *workshop > 0 {
            parts.push(hp_app::tf!("assets.workshop_items", *workshop));
            *workshop = 0;
        }
    };
    for s in sources.iter().filter(|s| s.enabled) {
        if s.origin == hp_fs::SourceOrigin::Workshop {
            workshop += 1;
            continue;
        }
        flush(&mut workshop, &mut parts);
        parts.push(s.label.clone());
    }
    flush(&mut workshop, &mut parts);
    let total = parts.len();
    if total > max {
        parts.truncate(max);
        parts.push(hp_app::tf!("assets.more_sources", total - max));
    }
    hp_app::tf!("assets.mounted_sources", parts.join(", "))
}

#[cfg(test)]
mod tests {
    use super::*;
    use hp_app::settings::CustomDirSetting;

    #[test]
    fn request_from_settings() {
        let mut s = Settings::default();
        s.game.gmod_root = Some(PathBuf::from("C:/Games/GarrysMod"));
        s.game.mount_workshop = true;
        s.game.custom_texture_dirs.push(CustomDirSetting {
            path: PathBuf::from("C:/Art/My Bricks"),
            prefix: String::new(),
            enabled: false,
            include_models: true,
        });
        s.game.custom_texture_dirs.push(CustomDirSetting {
            path: PathBuf::from("C:/Art/b"),
            prefix: "custom/b".into(),
            enabled: true,
            include_models: false,
        });
        let r = MountRequest::from_settings(&s);
        assert_eq!(r.root.as_deref(), Some(Path::new("C:/Games/GarrysMod")));
        assert!(r.opts.mount_workshop && r.opts.mount_addons && r.opts.mount_downloads);
        assert_eq!(r.opts.custom_dirs.len(), 2);
        assert_eq!(r.opts.custom_dirs[0].prefix, "custom/my_bricks");
        assert!(!r.opts.custom_dirs[0].enabled);
        assert_eq!(r.opts.custom_dirs[1].prefix, "custom/b");
        assert!(r.opts.custom_dirs[0].include_models);
        assert!(!r.opts.custom_dirs[1].include_models);
        assert!(r.opts.app_ids.is_empty());
        s.game.mount_app_ids = vec![240, 0, 440];
        assert_eq!(MountRequest::from_settings(&s).opts.app_ids, [240, 440]);
        s.game.gmod_root = Some(PathBuf::new());
        assert_eq!(MountRequest::from_settings(&s).root, None);
    }

    #[test]
    fn fgd_order() {
        let dir = PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("../../target/hp-assets/scratch/fgd_order");
        let bin = dir.join("bin");
        let hpp = bin.join("win64").join("hammerplusplus");
        std::fs::create_dir_all(&hpp).unwrap();
        std::fs::create_dir_all(dir.join("garrysmod")).unwrap();
        std::fs::write(bin.join("garrysmod.fgd"), "@PointClass size(-4 -4 -4, 4 4 4) = a : \"A\" []").unwrap();
        std::fs::write(hpp.join("hammerplusplus_fgd.fgd"), "@ExtendClass color(1 2 3) = a : \"\" []").unwrap();
        let extra = dir.join("extra.fgd");
        std::fs::write(&extra, "@PointClass = b : \"B\" []").unwrap();
        let missing = dir.join("nope.fgd");
        let (paths, warnings) = fgd_paths(Some(&dir), None, &[extra.clone(), missing]);
        assert_eq!(paths, vec![bin.join("garrysmod.fgd"), hpp.join("hammerplusplus_fgd.fgd"), extra.clone()]);
        assert_eq!(warnings.len(), 1);

        let (paths, _) = fgd_paths(Some(&dir), Some(&extra), &[]);
        assert_eq!(paths[0], extra);

        let req = FgdRequest { root: Some(dir.clone()), override_fgd: None, extra: vec![extra], game: None };
        let fgd = run_fgd(&req).fgd.unwrap();
        let a = fgd.class("a").unwrap();
        assert_eq!(a.color(), Some([1, 2, 3]), "Hammer++ extension applied");
        assert!(fgd.class("b").is_some());
        assert_eq!(install_root(&dir.join("garrysmod")), dir);
        assert_eq!(install_root(&dir), dir);
        assert_eq!(hammerpp_dir(&dir), Some(hpp.clone()));
        assert!(is_editor_only_source(&hpp));
        assert!(!is_editor_only_source(&dir.join("garrysmod")));
        assert_eq!(hammerpp_dir(&dir.join("garrysmod")), None);
    }

    #[test]
    fn console_lines_for_fgd_notes_and_sources() {
        hp_app::register_translations();
        crate::register_translations();
        let notes: Vec<String> = (0..5).map(|i| format!("base.fgd:{i}: bad")).collect();
        let lines = fgd_note_lines(&notes, 3);
        assert_eq!(lines.len(), 4);
        assert!(lines[0].contains("base.fgd:0: bad"), "{lines:?}");
        assert!(lines[3].contains('2'), "the rest is counted: {lines:?}");
        assert!(fgd_note_lines(&[], 3).is_empty());

        let fs = hp_fs::GameFs::empty();
        let dir = PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("../../target/hp-assets/scratch/sources_line");
        std::fs::create_dir_all(dir.join("a/materials")).unwrap();
        std::fs::create_dir_all(dir.join("b/materials")).unwrap();
        fs.add_game_dir(&dir.join("a")).unwrap();
        fs.add_game_dir(&dir.join("b")).unwrap();
        let line = sources_line(&fs.sources(), 40);
        assert!(line.contains('a') && line.contains('b'), "{line}");
        let cut = sources_line(&fs.sources(), 1);
        assert!(cut.contains('1'), "{cut}");
        let _ = std::fs::remove_dir_all(&dir);
    }

    #[test]
    fn fgd_problems_are_reported() {
        let dir = PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("../../target/hp-assets/scratch/fgd_notes");
        std::fs::create_dir_all(&dir).unwrap();
        let bad = dir.join("bad.fgd");
        std::fs::write(
            &bad,
            "@PointClass = a : \"A\" [ key(integer) : \"K\" : ]
@NoSuchThing x
",
        )
        .unwrap();
        let req = FgdRequest { root: None, override_fgd: Some(bad), extra: Vec::new(), game: None };
        let out = run_fgd(&req);
        assert!(!out.notes.is_empty(), "{:?}", out.notes);
        let _ = std::fs::remove_dir_all(&dir);
    }

    #[test]
    fn hammerpp_content_is_mounted_for_display_only() {
        let dir = PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("../../target/hp-assets/scratch/hammerpp_mount");
        let _ = std::fs::remove_dir_all(&dir);
        let game = dir.join("garrysmod");
        let hpp = dir.join("bin").join("win64").join("hammerplusplus");
        for d in [game.join("models/editor"), hpp.join("models/editor"), hpp.join("materials/models/editor")] {
            std::fs::create_dir_all(d).unwrap();
        }
        let gameinfo = "GameInfo { FileSystem { SearchPaths { game |gameinfo_path|. } } }";
        std::fs::write(game.join("gameinfo.txt"), gameinfo).unwrap();
        std::fs::write(game.join("models/editor/spot.mdl"), b"game").unwrap();
        std::fs::write(hpp.join("hammerplusplus_fgd.fgd"), "").unwrap();
        std::fs::write(hpp.join("models/editor/spot_color.mdl"), b"hpp").unwrap();
        std::fs::write(hpp.join("models/editor/spot.mdl"), b"hpp copy").unwrap();
        std::fs::write(hpp.join("materials/models/editor/spot_sheet_color.vmt"), "VertexLitGeneric { }").unwrap();
        let opts = MountOptions { mount_workshop: false, ..MountOptions::default() };
        let req = MountRequest { root: Some(dir.clone()), opts, game: None, scan: hp_fs::ScanConfig::default() };
        let MountOutcome::Mounted(data) = run_mount(&req, &hp_fs::MountProgress::new()) else { panic!("mount failed") };
        assert_eq!(data.fs.read("models/editor/spot_color.mdl").as_deref(), Some(&b"hpp"[..]));
        assert_eq!(data.fs.read("models/editor/spot.mdl").as_deref(), Some(&b"game"[..]), "the game keeps priority");
        let models: Vec<&str> = data.lists.models.iter().map(|m| m.name.as_str()).collect();
        assert!(models.contains(&"models/editor/spot.mdl"), "{models:?}");
        assert!(!models.contains(&"models/editor/spot_color.mdl"), "editor only models are not offered: {models:?}");
        assert!(!data.lists.materials.iter().any(|m| m.name.contains("spot_sheet_color")));
        let _ = std::fs::remove_dir_all(&dir);
    }
    #[test]
    fn another_game_mounts_its_content_and_fgd() {
        let dir = PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("../../target/hp-assets/scratch/other_game");
        let _ = std::fs::remove_dir_all(&dir);
        let root = dir.join("Team Fortress 2");
        let game = root.join("tf");
        std::fs::create_dir_all(game.join("materials/tf")).unwrap();
        std::fs::create_dir_all(root.join("bin")).unwrap();
        std::fs::write(game.join("gameinfo.txt"), "GameInfo { game \"Team Fortress 2\" GameData \"tf.fgd\" FileSystem { SteamAppId 440 SearchPaths { game |gameinfo_path|. } } }").unwrap();
        std::fs::write(game.join("materials/tf/wall.vmt"), "LightmappedGeneric { }").unwrap();
        std::fs::write(root.join("bin/tf.fgd"), "@include \"base.fgd\"\n@PointClass = item_teamflag : \"Flag\" []\n@PointClass = env_cubemap : \"Cubemap\" [ cubemapsize(integer) : \"Size\" : 0 ]").unwrap();
        std::fs::write(root.join("bin/base.fgd"), "@PointClass = info_player_teamspawn : \"Spawn\" []").unwrap();
        let mut s = Settings::default();
        s.game.game = "tf".to_owned();
        s.game.game_dirs.insert("tf".to_owned(), game.clone());
        let req = MountRequest::from_settings(&s);
        assert_eq!(req.game, Some(GameRef { id: "tf".to_owned(), dir: Some(game.clone()) }));
        let MountOutcome::Mounted(data) = run_mount(&req, &hp_fs::MountProgress::new()) else { panic!("mount failed") };
        assert!(data.fs.exists("materials/tf/wall.vmt"));
        assert_eq!(data.fs.game_dir(), Some(game.as_path()));
        let out = run_fgd(&FgdRequest::from_settings(&s));
        assert!(out.warnings.is_empty(), "{:?}", out.warnings);
        assert_eq!(out.files, vec![root.join("bin").join("tf.fgd")]);
        let fgd = out.fgd.unwrap();
        assert!(
            fgd.class("item_teamflag").is_some() && fgd.class("info_player_teamspawn").is_some(),
            "base.fgd included"
        );
        assert!(fgd.class(hp_editor::sky::SWITCH_CLASS).is_none(), "the Lua skybox switch is Garry's Mod's");
        assert!(!fgd.point_classes().iter().any(|c| c == hp_editor::sky::SWITCH_CLASS));
        assert!(fgd.class("parallax_obb").is_some());
        assert!(fgd.class("env_cubemap").and_then(|c| c.property("parallaxobb")).is_some());
        let gmod = run_fgd(&FgdRequest {
            root: None,
            override_fgd: Some(root.join("bin/tf.fgd")),
            extra: Vec::new(),
            game: None,
        });
        assert!(gmod.fgd.unwrap().class(hp_editor::sky::SWITCH_CLASS).is_some());
        let _ = std::fs::remove_dir_all(&dir);
    }
}
