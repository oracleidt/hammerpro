use std::path::{Component, Path, PathBuf};

pub(crate) const DEV_ADDON: &str = "hammerpro_dev";

pub(crate) fn dev_addon_dir(game_dir: &Path) -> PathBuf {
    game_dir.join("addons").join(DEV_ADDON)
}

pub(crate) fn is_safe_material(name: &str) -> bool {
    !name.is_empty()
        && name.len() < 260
        && !name.chars().any(|c| c.is_control() || matches!(c, ':' | '*' | '?' | '"' | '<' | '>' | '|'))
        && Path::new(name).components().all(|c| matches!(c, Component::Normal(_)))
        && !name.starts_with(['/', '\\'])
}

#[derive(Debug, Default)]
pub(crate) struct DevAddonReport {
    pub written: usize,
    pub failed: Vec<(String, String)>,
}

pub(crate) struct DevContent<'a> {
    pub materials: &'a [String],
    pub models: &'a [String],
    pub roots: &'a [hp_fs::deps::DepRoot],
    pub map: &'a str,
}

pub(crate) fn has_custom_content(fs: &hp_fs::GameFs) -> bool {
    fs.sources().iter().any(|s| s.enabled && s.origin == hp_fs::SourceOrigin::Custom)
}

fn dependency_roots(content: &DevContent<'_>) -> Vec<hp_fs::deps::DepRoot> {
    use hp_fs::deps::{DepRoot, RootKind};
    content
        .models
        .iter()
        .map(|name| DepRoot { kind: RootKind::Model, name: name.clone(), why: String::new() })
        .chain(content.roots.iter().filter(|r| !matches!(r.kind, RootKind::Material | RootKind::Sprite)).cloned())
        .collect()
}

pub(crate) fn export(fs: &hp_fs::GameFs, game_dir: &Path, content: &DevContent<'_>) -> Result<DevAddonReport, String> {
    if hp_compile::sandbox::is_sandboxed() {
        return Err(hp_compile::sandbox::REFUSED.to_owned());
    }
    let dir = dev_addon_dir(game_dir);
    if !game_dir.is_dir() {
        return Err(format!("{}: not a folder", game_dir.display()));
    }
    if !has_custom_content(fs) {
        return Ok(DevAddonReport::default());
    }
    let safe: Vec<String> = content.materials.iter().filter(|m| is_safe_material(m)).cloned().collect();
    let roots = dependency_roots(content);
    if safe.is_empty() && roots.is_empty() {
        return Ok(DevAddonReport::default());
    }
    std::fs::create_dir_all(&dir).map_err(|e| format!("{}: {e}", dir.display()))?;
    let mut report = if safe.is_empty() {
        hp_fs::ExportReport::default()
    } else {
        fs.export_custom_materials(&safe, &dir).map_err(|e| e.to_string())?
    };
    if !roots.is_empty() {
        let map = content.map.trim();
        let map = if is_safe_material(map) && !map.contains(['/', '\\']) { map } else { "" };
        let more = fs.export_custom_dependencies(&roots, map, &dir, &hp_fs::deps::DepsCache::new());
        report.written.extend(more.written);
        report.up_to_date.extend(more.up_to_date);
        report.failed.extend(more.failed);
    }
    if report.written.is_empty() && report.up_to_date.is_empty() && report.failed.is_empty() {
        let _ = std::fs::remove_dir(&dir);
    }
    Ok(DevAddonReport {
        written: report.written.len(),
        failed: report.failed.into_iter().map(|f| (f.path, f.error)).collect(),
    })
}

pub(crate) fn export_switch_lua(game_dir: &Path) -> Result<bool, String> {
    if hp_compile::sandbox::is_sandboxed() {
        return Err(hp_compile::sandbox::REFUSED.to_owned());
    }
    if !game_dir.is_dir() {
        return Err(format!("{}: not a folder", game_dir.display()));
    }
    let dir = dev_addon_dir(game_dir);
    let mut wrote = false;
    for (vpath, content) in hp_editor::sky::switch_lua_files() {
        let path = crate::release::files::safe_join(&dir, vpath).ok_or_else(|| "bad Lua path".to_owned())?;
        if std::fs::read(&path).is_ok_and(|old| old == content.as_bytes()) {
            continue;
        }
        if let Some(parent) = path.parent() {
            std::fs::create_dir_all(parent).map_err(|e| format!("{}: {e}", parent.display()))?;
        }
        let tmp = path.with_extension("lua.tmp");
        std::fs::write(&tmp, content).map_err(|e| format!("{}: {e}", tmp.display()))?;
        std::fs::rename(&tmp, &path).map_err(|e| {
            let _ = std::fs::remove_file(&tmp);
            format!("{}: {e}", path.display())
        })?;
        wrote = true;
    }
    Ok(wrote)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn material_names_cannot_escape() {
        assert!(is_safe_material("custom/brick"));
        assert!(is_safe_material("a/b/c_d-e"));
        for bad in ["", "../evil", "a/../../b", "/abs", "\\abs", "C:/x", "a\u{0}b", "a?b", "c:x"] {
            assert!(!is_safe_material(bad), "{bad:?}");
        }
    }

    fn tga(w: u16, h: u16) -> Vec<u8> {
        let mut v = vec![0u8, 0, 2, 0, 0, 0, 0, 0, 0, 0, 0, 0];
        v.extend_from_slice(&w.to_le_bytes());
        v.extend_from_slice(&h.to_le_bytes());
        v.extend_from_slice(&[24, 0]);
        v.resize(v.len() + usize::from(w) * usize::from(h) * 3, 128);
        v
    }

    #[test]
    fn the_skybox_switch_script_goes_to_the_dev_addon() {
        let dir = crate::test_scratch("dev_addon_switch");
        let game = dir.join("garrysmod");
        assert!(export_switch_lua(&game).is_err(), "no game folder");
        std::fs::create_dir_all(&game).expect("game");
        assert_eq!(export_switch_lua(&game), Ok(true));
        let addon = dev_addon_dir(&game);
        assert_eq!(
            std::fs::read_to_string(addon.join("lua/entities/point_hammerpro_skybox.lua")).expect("ent lua"),
            hp_editor::sky::SWITCH_LUA
        );
        assert_eq!(
            std::fs::read_to_string(addon.join("lua/autorun/hammerpro_skybox.lua")).expect("engine lua"),
            hp_editor::sky::SWITCH_ENGINE_LUA
        );
        assert_eq!(export_switch_lua(&game), Ok(false), "up to date");
        let _ = std::fs::remove_dir_all(&dir);
    }

    #[test]
    fn custom_materials_go_to_the_dev_addon() {
        let dir = crate::test_scratch("dev_addon");
        let pics = dir.join("pics");
        std::fs::create_dir_all(&pics).expect("pics");
        std::fs::write(pics.join("wall.tga"), tga(16, 16)).expect("tga");
        let game = dir.join("garrysmod");
        std::fs::create_dir_all(&game).expect("game");
        let fs = hp_fs::GameFs::empty();
        fs.add_custom_dir(hp_fs::CustomDir::with_prefix(&pics, "pics")).expect("custom dir");
        let used = vec!["pics/wall".to_owned(), "concrete/floor".to_owned(), "../../escape".to_owned()];
        let r = export(&fs, &game, &materials_only(&used)).expect("export");
        assert!(r.written >= 1, "{r:?}");
        let addon = dev_addon_dir(&game);
        assert!(addon.join("materials/pics/wall.vmt").is_file());
        assert!(addon.join("materials/pics/wall.vtf").is_file());
        let again = export(&fs, &game, &materials_only(&used)).expect("export");
        assert_eq!(again.written, 0, "up to date");
        let other = dir.join("other_game");
        std::fs::create_dir_all(&other).expect("other");
        export(&fs, &other, &materials_only(&["concrete/floor".to_owned()])).expect("export");
        assert!(!dev_addon_dir(&other).exists());
        assert!(export(&fs, &dir.join("missing"), &materials_only(&used)).is_err());
    }

    fn materials_only(materials: &[String]) -> DevContent<'_> {
        DevContent { materials, models: &[], roots: &[], map: "gm_w" }
    }

    #[test]
    fn custom_models_sounds_and_map_scripts_go_to_the_dev_addon() {
        use hp_fs::deps::{DepRoot, RootKind};
        let dir = crate::test_scratch("dev_addon_content");
        let mine = dir.join("mine");
        let files: [(&str, &[u8]); 9] = [
            ("models/w10/cube.mdl", b"IDST"),
            ("models/w10/cube.vvd", b"IDSV"),
            ("models/w10/cube.dx90.vtx", b"vtx"),
            ("models/w10/cube.phy", b"phy"),
            ("sound/w10/hum.wav", b"RIFF0001"),
            ("sound/w10/beep.wav", b"RIFF0002"),
            ("scripts/soundscapes_gm_w.txt", b"\"W10.Cave\" { \"playlooping\" { \"wave\" \"w10/hum.wav\" } }"),
            ("maps/gm_w_level_sounds.txt", b"\"W10.Beep\" { \"wave\" \"w10/beep.wav\" }"),
            ("materials/correction/w10.raw", &[7; 64]),
        ];
        for (rel, bytes) in files {
            let path = mine.join(rel);
            std::fs::create_dir_all(path.parent().expect("parent")).expect("dir");
            std::fs::write(path, bytes).expect("file");
        }
        let game = dir.join("garrysmod");
        std::fs::create_dir_all(&game).expect("game");
        let fs = hp_fs::GameFs::empty();
        let root = |kind, name: &str| DepRoot { kind, name: name.to_owned(), why: String::new() };
        let roots = vec![
            root(RootKind::Soundscape, "W10.Cave"),
            root(RootKind::Sound, "W10.Beep"),
            root(RootKind::File, "materials/correction/w10.raw"),
            root(RootKind::Model, "models/props_c17/oildrum001.mdl"),
        ];
        let models = vec!["models/w10/cube.mdl".to_owned()];
        let content = DevContent { materials: &[], models: &models, roots: &roots, map: "gm_w" };
        assert_eq!(export(&fs, &game, &content).expect("export").written, 0, "no custom folder, nothing to copy");
        assert!(!dev_addon_dir(&game).exists());
        fs.add_custom_dir(hp_fs::CustomDir::new(&mine)).expect("custom dir");
        let r = export(&fs, &game, &content).expect("export");
        assert!(r.failed.is_empty(), "{:?}", r.failed);
        assert_eq!(r.written, files.len(), "{r:?}");
        let addon = dev_addon_dir(&game);
        for (rel, bytes) in files {
            assert_eq!(std::fs::read(addon.join(rel)).expect(rel), bytes, "{rel}");
        }
        assert!(!addon.join("models/props_c17").exists(), "game models stay in the game");
        assert_eq!(export(&fs, &game, &content).expect("export").written, 0, "up to date");
        let _ = std::fs::remove_dir_all(&dir);
    }
}
