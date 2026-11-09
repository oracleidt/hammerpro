use std::path::{Path, PathBuf};

use hp_app::settings::{CustomDirSetting, Settings};
use hp_fs::{CustomLayout, CustomMode};

use crate::util::same_path;

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum AddOutcome {
    Added(usize),
    AlreadyPresent(usize),
}

impl AddOutcome {
    pub fn index(self) -> usize {
        match self {
            AddOutcome::Added(i) | AddOutcome::AlreadyPresent(i) => i,
        }
    }
}

pub fn sanitize_prefix(prefix: &str) -> String {
    let safe = hp_fs::safe_material_name(prefix.trim());
    match safe.strip_prefix("materials") {
        Some("") => String::new(),
        Some(rest) if rest.starts_with('/') => rest.trim_start_matches('/').to_owned(),
        _ => safe,
    }
}

pub fn add_custom_dir(settings: &mut Settings, path: &Path) -> AddOutcome {
    let dirs = &mut settings.game.custom_texture_dirs;
    if let Some(i) = dirs.iter().position(|d| same_path(&d.path, path)) {
        if let Some(d) = dirs.get_mut(i) {
            d.enabled = true;
        }
        return AddOutcome::AlreadyPresent(i);
    }
    dirs.push(CustomDirSetting {
        path: path.to_path_buf(),
        prefix: hp_fs::suggest_prefix(path),
        enabled: true,
        include_models: true,
    });
    AddOutcome::Added(dirs.len() - 1)
}

pub fn remove_custom_dir(settings: &mut Settings, index: usize) -> Option<CustomDirSetting> {
    let dirs = &mut settings.game.custom_texture_dirs;
    (index < dirs.len()).then(|| dirs.remove(index))
}

pub fn set_custom_enabled(settings: &mut Settings, index: usize, enabled: bool) -> bool {
    match settings.game.custom_texture_dirs.get_mut(index) {
        Some(d) if d.enabled != enabled => {
            d.enabled = enabled;
            true
        }
        _ => false,
    }
}

pub fn set_custom_prefix(settings: &mut Settings, index: usize, prefix: &str) -> bool {
    let Some(d) = settings.game.custom_texture_dirs.get_mut(index) else { return false };
    let mut clean = sanitize_prefix(prefix);
    if clean.is_empty() {
        clean = hp_fs::suggest_prefix(&d.path);
    }
    if d.prefix == clean {
        return false;
    }
    d.prefix = clean;
    true
}

pub fn import_target(dir: &Path, layout: &CustomLayout) -> (PathBuf, String) {
    match layout.mode {
        CustomMode::GameFolder => {
            let prefix = hp_fs::suggest_prefix(dir);
            let mut dest = dir.join("materials");
            for part in prefix.split('/') {
                dest.push(part);
            }
            (dest, prefix)
        }
        CustomMode::InsideMaterials | CustomMode::Prefixed | CustomMode::Detected => {
            (dir.to_path_buf(), layout.prefix.clone())
        }
    }
}

pub fn layout_of(state: &hp_app::AppState, dir: &CustomDirSetting) -> CustomLayout {
    if let Some(layout) = state.assets.fs().and_then(|fs| fs.custom_layout_of_path(&dir.path)) {
        return layout;
    }
    hp_fs::custom_layout(&hp_fs::CustomDir {
        path: dir.path.clone(),
        prefix: dir.prefix.clone(),
        enabled: dir.enabled,
        include_models: dir.include_models,
    })
}

pub fn vmt_target(dir: &Path, layout: &CustomLayout, name: &str) -> (PathBuf, String) {
    asset_target(dir, layout, name, "vmt")
}

pub fn asset_target(dir: &Path, layout: &CustomLayout, name: &str, ext: &str) -> (PathBuf, String) {
    let join = |base: PathBuf, rel: &str| -> PathBuf {
        let mut p = base;
        for part in rel.split('/').filter(|s| !s.is_empty()) {
            p.push(part);
        }
        let mut s = p.into_os_string();
        s.push(".");
        s.push(ext);
        PathBuf::from(s)
    };
    match layout.mode {
        CustomMode::GameFolder => (join(dir.join("materials"), name), name.to_owned()),
        CustomMode::InsideMaterials | CustomMode::Prefixed | CustomMode::Detected => {
            let prefix = layout.prefix.trim_matches('/');
            if prefix.is_empty() {
                return (join(dir.to_path_buf(), name), name.to_owned());
            }
            match name.strip_prefix(prefix).and_then(|r| r.strip_prefix('/')) {
                Some(rest) => (join(dir.to_path_buf(), rest), name.to_owned()),
                None => (join(dir.to_path_buf(), name), format!("{prefix}/{name}")),
            }
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn prefixes() {
        assert_eq!(sanitize_prefix("  Custom\\My Bricks// "), "custom/my_bricks");
        assert_eq!(sanitize_prefix("/a//b/"), "a/b");
        assert_eq!(sanitize_prefix("x:y*z"), "x_y_z");
        assert_eq!(sanitize_prefix(""), "");
        assert_eq!(sanitize_prefix("Textures Été"), "textures_ete");
        assert_eq!(sanitize_prefix("materials\\Mine"), "mine");
        assert_eq!(sanitize_prefix("materials"), "");
        assert_eq!(sanitize_prefix("materialsx/a"), "materialsx/a");
        assert_eq!(sanitize_prefix("../a/./b"), "a/b");
    }

    #[test]
    fn add_remove_enable() {
        let mut s = Settings::default();
        let a = add_custom_dir(&mut s, Path::new("C:/Art/My Bricks"));
        assert_eq!(a, AddOutcome::Added(0));
        assert_eq!(s.game.custom_texture_dirs[0].prefix, "custom/my_bricks");
        assert!(s.game.custom_texture_dirs[0].enabled);

        assert!(set_custom_enabled(&mut s, 0, false));
        assert!(!set_custom_enabled(&mut s, 0, false));
        let again = add_custom_dir(&mut s, Path::new("C:/Art/My Bricks/"));
        assert_eq!(again, AddOutcome::AlreadyPresent(0));
        assert!(s.game.custom_texture_dirs[0].enabled);
        assert_eq!(s.game.custom_texture_dirs.len(), 1);

        let b = add_custom_dir(&mut s, Path::new("D:/Other"));
        assert_eq!(b.index(), 1);
        assert!(set_custom_prefix(&mut s, 1, "Mine/Stuff"));
        assert_eq!(s.game.custom_texture_dirs[1].prefix, "mine/stuff");
        assert!(!set_custom_prefix(&mut s, 1, "mine/stuff"));
        assert!(set_custom_prefix(&mut s, 1, "  "));
        assert_eq!(s.game.custom_texture_dirs[1].prefix, "custom/other");
        assert!(!set_custom_prefix(&mut s, 9, "x"));

        let removed = remove_custom_dir(&mut s, 0).map(|d| d.path);
        assert_eq!(removed, Some(PathBuf::from("C:/Art/My Bricks")));
        assert_eq!(s.game.custom_texture_dirs.len(), 1);
        assert!(remove_custom_dir(&mut s, 5).is_none());
    }

    #[test]
    fn targets() {
        let prefixed = CustomLayout { mode: CustomMode::Prefixed, prefix: "custom/bricks".into() };
        let (dest, prefix) = import_target(Path::new("C:/Bricks"), &prefixed);
        assert_eq!(dest, PathBuf::from("C:/Bricks"));
        assert_eq!(prefix, "custom/bricks");

        let game = CustomLayout { mode: CustomMode::GameFolder, prefix: String::new() };
        let (dest, prefix) = import_target(Path::new("C:/Pack"), &game);
        assert_eq!(dest, Path::new("C:/Pack").join("materials").join("custom").join("pack"));
        assert_eq!(prefix, "custom/pack");

        let (p, n) = vmt_target(Path::new("C:/Bricks"), &prefixed, "custom/bricks/red/wall");
        assert_eq!(p, Path::new("C:/Bricks").join("red").join("wall.vmt"));
        assert_eq!(n, "custom/bricks/red/wall");
        let (p, n) = vmt_target(Path::new("C:/Bricks"), &prefixed, "concrete/floor");
        assert_eq!(p, Path::new("C:/Bricks").join("concrete").join("floor.vmt"));
        assert_eq!(n, "custom/bricks/concrete/floor");
        let (p, n) = vmt_target(Path::new("C:/Pack"), &game, "concrete/floor");
        assert_eq!(p, Path::new("C:/Pack").join("materials").join("concrete").join("floor.vmt"));
        assert_eq!(n, "concrete/floor");
        let (p, n) = asset_target(Path::new("C:/Bricks"), &prefixed, "hp_variants/n_bk1", "vtf");
        assert_eq!(p, Path::new("C:/Bricks").join("hp_variants").join("n_bk1.vtf"));
        assert_eq!(n, "custom/bricks/hp_variants/n_bk1");
    }
}
