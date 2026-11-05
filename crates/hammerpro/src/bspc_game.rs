use std::path::{Path, PathBuf};

const GENERATED_MARK: &str = "Added by Hammer Pro";

#[derive(Clone, Debug)]
pub(crate) struct CliGame {
    pub(crate) profile: Option<hp_fs::GameProfile>,
    pub(crate) format: hp_bsp::BspFormat,
    pub(crate) name: String,
}

fn is_gmod_folder(dir: &Path) -> bool {
    dir.join("garrysmod").join("gameinfo.txt").is_file()
        || (dir.file_name().is_some_and(|n| n.eq_ignore_ascii_case("garrysmod")) && dir.join("gameinfo.txt").is_file())
        || dir.parent().is_some_and(|p| p.join("garrysmod").join("gameinfo.txt").is_file())
}

fn real_game_folder(dir: &Path) -> bool {
    std::fs::read_to_string(dir.join("gameinfo.txt")).is_ok_and(|t| !t.contains(GENERATED_MARK))
}

pub(crate) fn resolve(target: Option<&str>, game: Option<&Path>) -> Result<CliGame, String> {
    let target = target.map(str::trim).filter(|t| !t.is_empty());
    let known = match target {
        Some(t) => Some(hp_fs::known_game(t).ok_or_else(|| {
            let ids: Vec<&str> = hp_fs::KNOWN_GAMES.iter().filter(|g| g.branch.is_some()).map(|g| g.id).collect();
            format!("unknown -target {t} (the games: {})", ids.join(", "))
        })?),
        None => None,
    };
    if let Some(k) = known
        && k.branch.is_none()
    {
        return Err(format!("{} is not supported: {}", k.name, k.unsupported.unwrap_or("")));
    }
    let libraries = hp_fs::detect_steam_libraries();
    let from_folder = game.filter(|g| !is_gmod_folder(g) && real_game_folder(g)).and_then(|g| {
        hp_fs::GameProfile::from_game_dir(&hp_fs::games::game_folder_of(g, target.unwrap_or("")), &libraries)
    });
    let profile = match (known, from_folder) {
        (Some(k), _) if k.id == hp_fs::GMOD_ID => None,
        (_, Some(p)) => Some(p),
        (Some(k), None) => hp_fs::resolve_game(k.id, None).filter(|p| p.installed),
        (None, None) => None,
    };
    if let Some(p) = profile.as_ref().filter(|p| p.branch.is_none()) {
        return Err(format!("{} is not supported: {}", p.name, p.unsupported.as_deref().unwrap_or("")));
    }
    let (format, name) = match (known, &profile) {
        (Some(k), _) => (k.branch.unwrap_or_default().format(), k.name.to_owned()),
        (None, Some(p)) => (p.format(), p.name.clone()),
        (None, None) => (hp_bsp::BspFormat::default(), "Garry's Mod".to_owned()),
    };
    Ok(CliGame { profile, format, name })
}

impl CliGame {
    #[cfg(test)]
    pub(crate) fn mount(&self, opts: &hp_fs::MountOptions) -> Option<Result<hp_fs::GameFs, hp_fs::FsError>> {
        let p = self.profile.as_ref()?;
        Some(hp_fs::GameFs::for_game(p, opts))
    }

    pub(crate) fn game_dir(&self) -> Option<&Path> {
        self.profile.as_ref().map(|p| p.game_dir.as_path())
    }

    pub(crate) fn root(&self) -> Option<PathBuf> {
        self.profile.as_ref().map(|p| p.root.clone()).filter(|r| !r.as_os_str().is_empty())
    }

    pub(crate) fn fgd(&self) -> Option<hp_fgd::ResolvedFgd> {
        let p = self.profile.as_ref()?;
        let fgd = hp_fgd::Fgd::load_with_include_dirs(&p.fgds, &p.fgd_dirs).ok()?;
        Some(fgd.resolve())
    }

    pub(crate) fn describe(&self) -> String {
        let f = &self.format;
        format!(
            "Target: {} (BSP version {}, static props {}, {} limits)",
            self.name,
            f.version,
            f.static_props.name(),
            if f.limits == hp_bsp::limits::LimitSet::Gmod { "Garry's Mod" } else { "Valve's" }
        )
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn scratch(name: &str) -> PathBuf {
        let dir = std::env::temp_dir().join(format!("hp-cli-game-{name}-{}", std::process::id()));
        let _ = std::fs::remove_dir_all(&dir);
        std::fs::create_dir_all(&dir).unwrap();
        dir
    }

    #[test]
    fn targets_and_game_folders() {
        let g = resolve(None, None).unwrap();
        assert!(g.profile.is_none());
        assert_eq!(g.format, hp_bsp::BspFormat::default());
        let dir = scratch("targets");
        let g = resolve(Some("left4dead2"), None).unwrap();
        assert_eq!(g.format.version, 21);
        assert!(g.describe().contains("Left 4 Dead 2") && g.describe().contains("BSP version 21"), "{}", g.describe());
        let tf = dir.join("Team Fortress 2").join("tf");
        std::fs::create_dir_all(&tf).unwrap();
        std::fs::write(
            tf.join("gameinfo.txt"),
            "GameInfo { game \"Team Fortress 2\" FileSystem { SteamAppId 440 SearchPaths { game |gameinfo_path|. } } }",
        )
        .unwrap();
        let g = resolve(None, Some(&tf)).unwrap();
        assert_eq!(g.profile.as_ref().map(|p| p.id.as_str()), Some("tf"));
        assert_eq!(g.format.branch, hp_bsp::Branch::Source2013);
        assert_eq!(g.game_dir(), Some(tf.as_path()));
        assert!(g.mount(&hp_fs::MountOptions::default()).is_some_and(|r| r.is_ok()));
        let generated = dir.join("compile_game");
        std::fs::create_dir_all(&generated).unwrap();
        std::fs::write(
            generated.join("gameinfo.txt"),
            format!("GameInfo {{ FileSystem {{ SearchPaths {{\n// {GENERATED_MARK}\ngame x }} }} }}"),
        )
        .unwrap();
        let g = resolve(Some("portal2"), Some(&generated)).unwrap();
        assert_eq!(g.format.branch, hp_bsp::Branch::Portal2);
        assert!(g.profile.as_ref().is_none_or(|p| p.id == "portal2"));
        let gm = dir.join("GarrysMod").join("garrysmod");
        std::fs::create_dir_all(&gm).unwrap();
        std::fs::write(gm.join("gameinfo.txt"), "GameInfo { }").unwrap();
        assert!(resolve(None, Some(&gm)).unwrap().profile.is_none());
        assert!(resolve(Some("nope"), None).unwrap_err().contains("unknown -target nope"));
        assert!(resolve(Some("contagion"), None).unwrap_err().contains("not supported"));
        let _ = std::fs::remove_dir_all(&dir);
    }
}
