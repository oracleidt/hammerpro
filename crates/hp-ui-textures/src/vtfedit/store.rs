use std::path::{Path, PathBuf};

use hp_app::AppState;

use crate::matedit::{WriteOutcome, backup_path};

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Kind {
    Disk,
    Archive,
    Generated,
    Missing,
}

#[derive(Clone, Debug)]
pub struct Located {
    pub name: String,
    pub kind: Kind,
    pub bytes: Option<Vec<u8>>,
    pub disk: Option<PathBuf>,
    pub image_file: Option<PathBuf>,
    pub label: String,
}

pub fn normalize(name: &str) -> String {
    hp_vmt::normalize_texture_path(name.trim().trim_end_matches(".vtf"))
}

pub fn locate(fs: &hp_fs::GameFs, texture: &str) -> Located {
    let name = normalize(texture);
    let vpath = format!("materials/{name}.vtf");
    let label = fs
        .which_source(&vpath)
        .and_then(|id| fs.source(id))
        .map(|s| s.label)
        .or_else(|| fs.texture_source(&name).and_then(|id| fs.source(id)).map(|s| s.label))
        .unwrap_or_default();
    if let Some(bytes) = (!name.is_empty()).then(|| fs.load_texture_raw(&name)).flatten() {
        let disk = fs.disk_path(&vpath);
        let kind = if disk.is_some() { Kind::Disk } else { Kind::Archive };
        return Located { name, kind, bytes: Some(bytes), disk, image_file: None, label };
    }
    if let Some(img) = (!name.is_empty()).then(|| fs.custom_image(&name)).flatten() {
        return Located { name, kind: Kind::Generated, bytes: None, disk: None, image_file: Some(img.path), label };
    }
    Located { name, kind: Kind::Missing, bytes: None, disk: None, image_file: None, label }
}

#[derive(Clone, Debug)]
pub struct Target {
    pub path: PathBuf,
    pub name: String,
}

pub fn custom_target(state: &AppState, dest: usize, name: &str) -> Result<Target, String> {
    let dir =
        state.settings.game.custom_texture_dirs.get(dest).ok_or_else(|| hp_app::t!("vtf.err.no_custom").to_owned())?;
    if !dir.enabled {
        return Err(hp_app::t!("vtf.err.custom_disabled").to_owned());
    }
    let clean = clean_name(name)?;
    let layout = crate::folders::layout_of(state, dir);
    let (path, virtual_name) = crate::folders::asset_target(&dir.path, &layout, &clean, "vtf");
    Ok(Target { path, name: virtual_name })
}

pub fn chosen_target(state: &AppState, dest: usize, name: &str, file: &str) -> Result<Target, String> {
    if file.trim().is_empty() {
        return custom_target(state, dest, name);
    }
    let mut path = PathBuf::from(file.trim());
    if !path.is_absolute() || path.file_name().is_none() || path.is_dir() {
        return Err(hp_app::t!("vtf.err.bad_path").to_owned());
    }
    path.set_extension("vtf");
    let name = crate::matedit_single::texture_path_of(&path)
        .unwrap_or_else(|| path.file_stem().unwrap_or_default().to_string_lossy().into_owned());
    Ok(Target { path, name: clean_name(&name)? })
}

pub fn clean_name(name: &str) -> Result<String, String> {
    let n = name.trim().trim_end_matches(".vtf").replace('\\', "/").to_ascii_lowercase();
    let parts: Vec<&str> = n.split('/').filter(|p| !p.is_empty()).collect();
    if parts.is_empty() || parts.iter().any(|p| *p == ".." || *p == "." || p.contains(':')) {
        return Err(hp_app::t!("vtf.err.bad_name").to_owned());
    }
    let ok = parts.iter().all(|p| {
        p.chars().all(|c| c.is_ascii_alphanumeric() || matches!(c, '_' | '-' | ' ' | '.' | '+')) && !p.ends_with('.')
    });
    if !ok {
        return Err(hp_app::t!("vtf.err.bad_name").to_owned());
    }
    Ok(parts.join("/"))
}

pub fn target_for(state: &AppState, loc: &Located, dest: usize) -> Result<Target, String> {
    match (loc.kind, &loc.disk) {
        (Kind::Disk, Some(p)) => Ok(Target { path: p.clone(), name: loc.name.clone() }),
        _ => custom_target(state, dest, &loc.name),
    }
}

pub fn write_bytes_with_backup(path: &Path, data: &[u8]) -> std::io::Result<WriteOutcome> {
    let existed = path.is_file();
    let mut backup = None;
    if existed {
        let bak = backup_path(path);
        if !bak.exists() {
            std::fs::copy(path, &bak)?;
            backup = Some(bak);
        }
    } else if let Some(dir) = path.parent() {
        std::fs::create_dir_all(dir)?;
    }
    let mut tmp = path.as_os_str().to_owned();
    tmp.push(".tmp");
    let tmp = PathBuf::from(tmp);
    std::fs::write(&tmp, data)?;
    if let Err(e) = std::fs::rename(&tmp, path) {
        let _ = std::fs::remove_file(&tmp);
        return Err(e);
    }
    Ok(WriteOutcome { backup, created: !existed })
}

pub fn refresh(state: &mut AppState, written: &Path, texture: &str, materials: &[String]) {
    if let Some(fs) = state.assets.fs() {
        let ids: Vec<u32> = fs
            .sources()
            .into_iter()
            .filter(|s| match &s.kind {
                hp_fs::SourceKind::Custom(dir) => written.starts_with(&dir.path),
                _ => false,
            })
            .map(|s| s.id)
            .collect();
        fs.refresh_sources(&ids);
    }
    let mut names: Vec<String> = materials.to_vec();
    names.push(texture.to_owned());
    if !state.current_material.is_empty() {
        names.push(state.current_material.clone());
    }
    names.sort();
    names.dedup();
    for n in names {
        state.assets.invalidate_material(&n);
    }
}

#[derive(Clone)]
pub struct MaterialPlan {
    pub shader: String,
    pub surface_prop: Option<String>,
    pub translucent: bool,
    pub alphatest: bool,
}

pub fn create_vmt(target: &Target, plan: &MaterialPlan, overwrite: bool) -> Result<Option<PathBuf>, String> {
    let path = target.path.with_extension("vmt");
    if path.is_file() && !overwrite {
        return Ok(None);
    }
    let opts = hp_vmt::ImageMaterialOptions {
        shader: plan.shader.clone(),
        surfaceprop: plan.surface_prop.clone(),
        translucent: plan.translucent && !plan.alphatest,
        alphatest: plan.alphatest,
        ..hp_vmt::ImageMaterialOptions::default()
    };
    let text = hp_vmt::write(&hp_vmt::for_image(&target.name, &opts));
    crate::matedit::write_with_backup(&path, &text).map_err(|e| format!("{}: {e}", path.display()))?;
    Ok(Some(path))
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn names_are_cleaned() {
        assert_eq!(clean_name(" Custom\\Brick/Wall01.vtf ").as_deref(), Ok("custom/brick/wall01"));
        assert!(clean_name("").is_err());
        assert!(clean_name("../x").is_err());
        assert!(clean_name("a/../b").is_err());
        assert!(clean_name("c:/x").is_err());
        assert!(clean_name("a/b*c").is_err());
        assert_eq!(normalize("Brick/Wall.vtf"), "brick/wall");
    }

    #[test]
    fn writes_keep_one_backup() {
        let dir = std::env::temp_dir().join("hp-vtf-store-test");
        let _ = std::fs::remove_dir_all(&dir);
        let f = dir.join("sub").join("a.vtf");
        let w = write_bytes_with_backup(&f, b"one").unwrap();
        assert!(w.created && w.backup.is_none());
        let w = write_bytes_with_backup(&f, b"two").unwrap();
        assert!(!w.created && w.backup.is_some());
        let w = write_bytes_with_backup(&f, b"three").unwrap();
        assert!(w.backup.is_none());
        assert_eq!(std::fs::read(backup_path(&f)).unwrap(), b"one");
        assert_eq!(std::fs::read(&f).unwrap(), b"three");
        let _ = std::fs::remove_dir_all(&dir);
    }

    #[test]
    fn vmt_is_created_once() {
        let dir = std::env::temp_dir().join("hp-vtf-vmt-test");
        let _ = std::fs::remove_dir_all(&dir);
        let target = Target { path: dir.join("wall.vtf"), name: "custom/wall".into() };
        let plan = MaterialPlan {
            shader: "UnlitGeneric".into(),
            surface_prop: Some("brick".into()),
            translucent: true,
            alphatest: false,
        };
        let p = create_vmt(&target, &plan, false).unwrap().expect("created");
        let text = std::fs::read_to_string(&p).unwrap();
        assert!(
            text.contains("UnlitGeneric") && text.contains("custom/wall") && text.contains("$translucent"),
            "{text}"
        );
        assert!(create_vmt(&target, &plan, false).unwrap().is_none());
        let _ = std::fs::remove_dir_all(&dir);
    }
}
