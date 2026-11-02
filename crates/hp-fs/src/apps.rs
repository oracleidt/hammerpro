use std::collections::HashMap;
use std::path::{Path, PathBuf};

use crate::detect::path_key;
use crate::games::{GameProfile, known_game_of_app, plan_game, read_gameinfo};
use crate::mount::{MountOptions, PlanItem, game_folder_items};
use crate::source::SourceOrigin;

pub const MAX_APP_ID: u32 = 99_999_999;

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct SteamApp {
    pub app_id: u32,
    pub name: String,
    pub root: PathBuf,
    pub library: PathBuf,
    pub game_dirs: Vec<PathBuf>,
}

#[derive(Clone, Debug, Default, PartialEq, Eq)]
struct Manifest {
    name: Option<String>,
    install_dir: Option<String>,
}

fn read_manifest(steamapps: &Path, app_id: u32) -> Option<Manifest> {
    let bytes = std::fs::read(steamapps.join(format!("appmanifest_{app_id}.acf"))).ok()?;
    let kv = hp_kv::parse(&String::from_utf8_lossy(&bytes)).ok()?;
    let state = kv.child("AppState")?;
    let get = |k: &str| state.get(k).map(|v| v.trim().to_owned()).filter(|v| !v.is_empty());
    Some(Manifest { name: get("name"), install_dir: get("installdir") })
}

fn game_dirs_of(root: &Path, app_id: u32) -> Vec<PathBuf> {
    if let Some(g) = known_game_of_app(app_id) {
        let dir = root.join(g.game_folder);
        if dir.join("gameinfo.txt").is_file() {
            return vec![dir];
        }
    }
    let Ok(read) = std::fs::read_dir(root) else {
        return Vec::new();
    };
    let mut dirs: Vec<PathBuf> =
        read.flatten().map(|e| e.path()).filter(|p| p.join("gameinfo.txt").is_file()).collect();
    dirs.sort_by_key(|p| p.file_name().map(|n| n.to_string_lossy().to_ascii_lowercase()));
    let own: Vec<PathBuf> =
        dirs.iter().filter(|d| read_gameinfo(d).is_some_and(|g| g.app_id == app_id)).cloned().collect();
    if own.is_empty() { dirs } else { own }
}

pub fn find_steam_app_in(app_id: u32, libraries: &[PathBuf]) -> Option<SteamApp> {
    if app_id == 0 {
        return None;
    }
    let known = known_game_of_app(app_id);
    for lib in libraries {
        let steamapps = lib.join("steamapps");
        let manifest = read_manifest(&steamapps, app_id);
        let mut candidates: Vec<String> = manifest.iter().filter_map(|m| m.install_dir.clone()).collect();
        if manifest.is_none() {
            candidates.extend(known.iter().flat_map(|g| g.install_dirs.iter().map(|d| (*d).to_owned())));
        }
        for dir in candidates {
            let root = steamapps.join("common").join(&dir);
            if !root.is_dir() {
                continue;
            }
            let game_dirs = game_dirs_of(&root, app_id);
            if game_dirs.is_empty() && manifest.is_none() {
                continue;
            }
            let name = known
                .map(|g| g.name.to_owned())
                .or_else(|| manifest.as_ref().and_then(|m| m.name.clone()))
                .unwrap_or(dir);
            return Some(SteamApp { app_id, name, root, library: lib.clone(), game_dirs });
        }
    }
    None
}

pub fn find_steam_app(app_id: u32) -> Option<SteamApp> {
    let mut cache = APPS.lock();
    let cache = cache.get_or_insert_with(HashMap::new);
    if let Some(found) = cache.get(&app_id) {
        return found.clone();
    }
    let found = find_steam_app_in(app_id, &crate::detect::detect_steam_libraries());
    cache.insert(app_id, found.clone());
    found
}

pub fn steam_app_name(app_id: u32) -> Option<String> {
    find_steam_app(app_id).map(|a| a.name).or_else(|| known_game_of_app(app_id).map(|g| g.name.to_owned()))
}

pub fn installed_source_apps_in(libraries: &[PathBuf]) -> Vec<SteamApp> {
    let mut out: Vec<SteamApp> = Vec::new();
    for lib in libraries {
        let Ok(read) = std::fs::read_dir(lib.join("steamapps")) else { continue };
        let mut ids: Vec<u32> = read
            .flatten()
            .filter_map(|e| {
                let name = e.file_name().to_string_lossy().to_ascii_lowercase();
                name.strip_prefix("appmanifest_")?.strip_suffix(".acf")?.parse().ok()
            })
            .collect();
        ids.sort_unstable();
        for id in ids {
            if out.iter().any(|a| a.app_id == id) {
                continue;
            }
            if let Some(app) = find_steam_app_in(id, std::slice::from_ref(lib)).filter(|a| !a.game_dirs.is_empty()) {
                out.push(app);
            }
        }
    }
    out.sort_by_key(|a| a.name.to_ascii_lowercase());
    out
}

pub fn installed_source_apps() -> Vec<SteamApp> {
    let mut cache = INSTALLED.lock();
    if let Some(list) = cache.as_ref() {
        return list.clone();
    }
    let list = installed_source_apps_in(&crate::detect::detect_steam_libraries());
    *cache = Some(list.clone());
    list
}

pub fn refresh_steam_apps() {
    *APPS.lock() = None;
    *INSTALLED.lock() = None;
}

static APPS: parking_lot::Mutex<Option<HashMap<u32, Option<SteamApp>>>> = parking_lot::Mutex::new(None);
static INSTALLED: parking_lot::Mutex<Option<Vec<SteamApp>>> = parking_lot::Mutex::new(None);

pub(crate) fn app_items(opts: &MountOptions, warnings: &mut Vec<String>) -> Vec<PlanItem> {
    if opts.app_ids.is_empty() {
        return Vec::new();
    }
    let detected;
    let libraries: &[PathBuf] = if opts.steam_libraries.is_empty() {
        detected = crate::detect::detect_steam_libraries();
        &detected
    } else {
        &opts.steam_libraries
    };
    let mut items = Vec::new();
    let mut done: Vec<u32> = Vec::new();
    for &id in &opts.app_ids {
        if done.contains(&id) {
            continue;
        }
        done.push(id);
        let Some(app) = find_steam_app_in(id, libraries) else {
            warnings.push(format!("Steam app {id} is not installed in the Steam libraries of this computer"));
            continue;
        };
        if app.game_dirs.is_empty() {
            warnings.push(format!("Steam app {id} ({}): no Source game folder in {}", app.name, app.root.display()));
            continue;
        }
        items.extend(items_of_app(&app, libraries, warnings));
    }
    items
}

fn items_of_app(app: &SteamApp, libraries: &[PathBuf], warnings: &mut Vec<String>) -> Vec<PlanItem> {
    let label = format!("App {} {}", app.app_id, app.name);
    let plain = MountOptions { custom_dirs: Vec::new(), extra_game_dirs: Vec::new(), ..MountOptions::default() };
    let mut out = Vec::new();
    for dir in &app.game_dirs {
        let profile = known_game_of_app(app.app_id)
            .filter(|g| dir.file_name().is_some_and(|n| n.eq_ignore_ascii_case(g.game_folder)))
            .and_then(|g| GameProfile::installed_in(g, &app.root))
            .or_else(|| GameProfile::from_game_dir(dir, libraries));
        let planned = profile.as_ref().and_then(|p| plan_game(p, &plain).ok());
        let items = match planned {
            Some((items, more)) => {
                warnings.extend(more);
                items
            }
            None => game_folder_items(dir, SourceOrigin::Extra, &label, warnings),
        };
        for mut item in items {
            item.origin = SourceOrigin::Extra;
            if !item.label.starts_with(&label) {
                item.label = format!("{label}: {}", item.label);
            }
            out.push(item);
        }
    }
    out
}

pub(crate) fn append_unique(items: &mut Vec<PlanItem>, more: Vec<PlanItem>) {
    let mut seen: std::collections::HashSet<String> = items.iter().map(|i| path_key(&i.path)).collect();
    for item in more {
        if seen.insert(path_key(&item.path)) {
            items.push(item);
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::test_util::TempDir;

    fn gameinfo(title: &str, app: u32, paths: &str) -> String {
        format!("\"GameInfo\" {{ game \"{title}\" FileSystem {{ SteamAppId {app} SearchPaths {{ {paths} }} }} }}")
    }

    fn steam() -> TempDir {
        let t = TempDir::new("steam-apps");
        let lib1 = "lib1/steamapps";
        let lib2 = "lib2/steamapps";
        t.write(&format!("{lib1}/appmanifest_240.acf"), b"\"AppState\" { \"appid\" \"240\" \"name\" \"Counter-Strike: Source\" \"installdir\" \"Counter-Strike Source\" }");
        t.write(
            &format!("{lib1}/common/Counter-Strike Source/cstrike/gameinfo.txt"),
            gameinfo("Counter-Strike Source", 240, "game |gameinfo_path|. game cstrike/cstrike_pak.vpk").as_bytes(),
        );
        t.write(&format!("{lib1}/common/Counter-Strike Source/cstrike/cstrike_pak_dir.vpk"), b"");
        t.write(&format!("{lib1}/common/Counter-Strike Source/cstrike/materials/de/floor.vmt"), b"x");
        t.write(
            &format!("{lib2}/appmanifest_420.acf"),
            b"\"AppState\" { \"appid\" \"420\" \"name\" \"Half-Life 2: Episode Two\" \"installdir\" \"Half-Life 2\" }",
        );
        t.write(
            &format!("{lib2}/common/Half-Life 2/ep2/gameinfo.txt"),
            gameinfo("Half-Life 2: Episode Two", 220, "game |gameinfo_path|. game |all_source_engine_paths|hl2")
                .as_bytes(),
        );
        t.write(
            &format!("{lib2}/common/Half-Life 2/hl2/gameinfo.txt"),
            gameinfo("Half-Life 2", 220, "game |gameinfo_path|.").as_bytes(),
        );
        t.write(&format!("{lib2}/common/Half-Life 2/ep2/materials/x.vmt"), b"x");
        t.write(&format!("{lib2}/common/Half-Life 2/hl2/materials/y.vmt"), b"x");
        t.write(
            &format!("{lib2}/appmanifest_999001.acf"),
            b"\"AppState\" { \"appid\" \"999001\" \"name\" \"My Mod\" \"installdir\" \"My Mod\" }",
        );
        t.write(
            &format!("{lib2}/common/My Mod/mymod/gameinfo.txt"),
            gameinfo("My Mod", 999_001, "game |gameinfo_path|.").as_bytes(),
        );
        t.write(&format!("{lib2}/common/My Mod/mymod/materials/z.vmt"), b"x");
        t.write(
            &format!("{lib2}/appmanifest_12345.acf"),
            b"\"AppState\" { \"appid\" \"12345\" \"name\" \"Not Source\" \"installdir\" \"NotSource\" }",
        );
        t.mkdir(&format!("{lib2}/common/NotSource/data"));
        t
    }

    fn libs(t: &TempDir) -> Vec<PathBuf> {
        vec![t.path().join("lib1"), t.path().join("lib2")]
    }

    #[test]
    fn resolves_app_ids_through_the_manifests() {
        let t = steam();
        let libs = libs(&t);
        let css = find_steam_app_in(240, &libs).expect("css");
        assert_eq!(css.name, "Counter-Strike: Source");
        assert_eq!(css.library, libs[0]);
        assert_eq!(css.game_dirs, [css.root.join("cstrike")]);
        let ep2 = find_steam_app_in(420, &libs).expect("ep2");
        assert_eq!(ep2.root, libs[1].join("steamapps/common/Half-Life 2"));
        assert_eq!(ep2.game_dirs, [ep2.root.join("ep2")], "the known game folder, not every folder");
        let own = find_steam_app_in(999_001, &libs).expect("mod");
        assert_eq!((own.name.as_str(), own.game_dirs.len()), ("My Mod", 1));
        let other = find_steam_app_in(12345, &libs).expect("installed");
        assert!(other.game_dirs.is_empty());
        assert_eq!(find_steam_app_in(440, &libs), None);
        assert_eq!(find_steam_app_in(0, &libs), None);
        let listed: Vec<u32> = installed_source_apps_in(&libs).iter().map(|a| a.app_id).collect();
        assert_eq!(listed, [240, 420, 999_001]);
    }

    #[test]
    fn app_ids_mount_their_game_content() {
        let t = steam();
        let opts = MountOptions {
            app_ids: vec![240, 420, 440, 12345, 240],
            steam_libraries: libs(&t),
            ..MountOptions::default()
        };
        let mut warnings = Vec::new();
        let items = app_items(&opts, &mut warnings);
        let labels: Vec<&str> = items.iter().map(|i| i.label.as_str()).collect();
        assert!(items.iter().all(|i| i.origin == SourceOrigin::Extra), "{labels:?}");
        assert!(
            labels.iter().any(|l| l.starts_with("App 240 Counter-Strike: Source") && l.contains("cstrike_pak.vpk")),
            "{labels:?}"
        );
        let has = |rel: &str| items.iter().any(|i| i.path.ends_with(rel));
        assert!(has("cstrike") && has("cstrike/cstrike_pak_dir.vpk"));
        assert!(
            has("Half-Life 2/ep2") && has("Half-Life 2/hl2"),
            "episode two mounts what its gameinfo lists: {labels:?}"
        );
        assert_eq!(items.iter().filter(|i| i.path.ends_with("cstrike")).count(), 1, "listed twice, mounted once");
        assert_eq!(warnings.len(), 2, "{warnings:?}");
        assert!(warnings[0].contains("440") && warnings[1].contains("12345"), "{warnings:?}");
    }

    #[test]
    fn game_fs_mounts_app_ids() {
        let t = steam();
        let root = t.mkdir("gmod/GarrysMod");
        t.write("gmod/GarrysMod/garrysmod/materials/own.vmt", b"x");
        let opts = MountOptions {
            mount_workshop: false,
            mount_cfg: false,
            app_ids: vec![240, 999_001],
            steam_libraries: libs(&t),
            ..MountOptions::default()
        };
        let fs = crate::GameFs::for_gmod(&root, &opts).expect("mount");
        assert!(fs.exists("materials/own.vmt"));
        assert!(fs.exists("materials/de/floor.vmt"), "Counter-Strike: Source content");
        assert!(fs.exists("materials/z.vmt"), "the mod content");
        let sources = fs.sources();
        let first_app = sources.iter().position(|s| s.label.starts_with("App ")).expect("app sources");
        let own = sources.iter().position(|s| s.label == "garrysmod").expect("own folder");
        assert!(own < first_app, "the game's own content wins");
        let profile = GameProfile::from_game_dir(&t.path().join("lib2/steamapps/common/My Mod/mymod"), &libs(&t))
            .expect("profile");
        let fs = crate::GameFs::for_game(
            &profile,
            &MountOptions { app_ids: vec![240], steam_libraries: libs(&t), ..MountOptions::default() },
        )
        .expect("mount");
        assert!(fs.exists("materials/z.vmt") && fs.exists("materials/de/floor.vmt"));
    }
}
