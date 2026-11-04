use std::path::{Path, PathBuf};

use hp_app::settings::BlenderFormat;

pub const ADDON_ID: &str = "hammerpro_bridge";

const ADDON: &str = r##"bl_info = {
    "name": "Hammer Pro Bridge",
    "author": "Hammer Pro",
    "version": (1, 0, 0),
    "blender": (2, 80, 0),
    "location": "Top bar Hammer Pro menu, 3D View sidebar Hammer Pro tab",
    "description": "Live link with the Hammer Pro level editor",
    "category": "Import-Export",
}

import json
import os

import bpy
from bpy.app.handlers import persistent

DEFAULT_LINK = __LINK__


def link_path():
    return os.environ.get("HAMMERPRO_LINK") or DEFAULT_LINK


def read_link():
    try:
        with open(link_path(), "r", encoding="utf-8") as handle:
            data = json.load(handle)
    except Exception:
        return None
    if not data.get("file"):
        return None
    return data


def call(operator, required, optional):
    try:
        return operator(**required, **optional)
    except TypeError:
        return operator(**required)


def export_scene(path, fmt, selected_only):
    folder = os.path.dirname(path)
    if folder:
        os.makedirs(folder, exist_ok=True)
    try:
        if bpy.ops.object.mode_set.poll():
            bpy.ops.object.mode_set(mode="OBJECT")
    except Exception:
        pass
    if fmt == "obj":
        if hasattr(bpy.ops.wm, "obj_export"):
            call(
                bpy.ops.wm.obj_export,
                {"filepath": path},
                {
                    "export_selected_objects": selected_only,
                    "forward_axis": "NEGATIVE_Z",
                    "up_axis": "Y",
                    "path_mode": "COPY",
                },
            )
        else:
            call(
                bpy.ops.export_scene.obj,
                {"filepath": path},
                {"use_selection": selected_only, "axis_forward": "-Z", "axis_up": "Y", "path_mode": "COPY"},
            )
    elif fmt == "fbx":
        call(
            bpy.ops.export_scene.fbx,
            {"filepath": path},
            {
                "use_selection": selected_only,
                "path_mode": "COPY",
                "embed_textures": True,
                "axis_forward": "-Z",
                "axis_up": "Y",
            },
        )
    else:
        call(
            bpy.ops.export_scene.gltf,
            {"filepath": path},
            {"export_format": "GLB", "use_selection": selected_only, "export_yup": True, "export_apply": True},
        )


def clear_scene():
    for obj in list(bpy.data.objects):
        bpy.data.objects.remove(obj, do_unlink=True)


def enable_io():
    try:
        import addon_utils

        for name in ("io_scene_gltf2", "io_scene_fbx", "io_scene_obj"):
            try:
                addon_utils.enable(name, default_set=False, persistent=False)
            except Exception:
                pass
    except Exception:
        pass


def import_file(path, fmt):
    if fmt == "obj":
        if hasattr(bpy.ops.wm, "obj_import"):
            bpy.ops.wm.obj_import(filepath=path, forward_axis="NEGATIVE_Z", up_axis="Y")
        else:
            bpy.ops.import_scene.obj(filepath=path, axis_forward="-Z", axis_up="Y")
    elif fmt == "fbx":
        bpy.ops.import_scene.fbx(filepath=path)
    else:
        bpy.ops.import_scene.gltf(filepath=path)


def frame_all():
    for window in bpy.context.window_manager.windows:
        for area in window.screen.areas:
            if area.type != "VIEW_3D":
                continue
            region = next((r for r in area.regions if r.type == "WINDOW"), None)
            if region is None:
                continue
            try:
                with bpy.context.temp_override(window=window, area=area, region=region):
                    bpy.ops.view3d.view_all(center=False)
            except AttributeError:
                bpy.ops.view3d.view_all({"window": window, "area": area, "region": region}, center=False)
            return None
    return None


SEEN = {"revision": None}


def load_scene(path, fmt):
    enable_io()
    clear_scene()
    try:
        import_file(path, fmt)
    except Exception as error:
        print("Hammer Pro Bridge: cannot import", path, error)
    bpy.app.timers.register(frame_all, first_interval=0.5)


def start(path, fmt):
    link = read_link()
    SEEN["revision"] = link.get("revision") if link else None
    load_scene(path, fmt)


def poll_link():
    link = read_link()
    if link and SEEN["revision"] is not None and link.get("revision") != SEEN["revision"]:
        SEEN["revision"] = link.get("revision")
        load_scene(link["file"], link.get("format", "gltf"))
    return 1.0


@persistent
def on_save(_dummy):
    link = read_link()
    if not link:
        return
    if not getattr(bpy.context.scene, "hammerpro_export_on_save", True):
        return
    try:
        export_scene(link["file"], link.get("format", "gltf"), bool(link.get("selected_only", False)))
    except Exception as error:
        print("Hammer Pro Bridge: export failed:", error)


class HAMMERPRO_OT_update(bpy.types.Operator):
    bl_idname = "hammerpro.update"
    bl_label = "Update in Hammer Pro"
    bl_description = "Export the scene to the file Hammer Pro watches"

    selected_only: bpy.props.BoolProperty(name="Selected only", default=False)

    def execute(self, context):
        link = read_link()
        if not link:
            self.report({"ERROR"}, "No Hammer Pro link: use Send to Blender in Hammer Pro first")
            return {"CANCELLED"}
        try:
            export_scene(
                link["file"],
                link.get("format", "gltf"),
                self.selected_only or bool(link.get("selected_only", False)),
            )
        except Exception as error:
            self.report({"ERROR"}, str(error))
            return {"CANCELLED"}
        self.report({"INFO"}, "Sent to Hammer Pro")
        return {"FINISHED"}


class HAMMERPRO_PT_panel(bpy.types.Panel):
    bl_label = "Hammer Pro"
    bl_idname = "HAMMERPRO_PT_panel"
    bl_space_type = "VIEW_3D"
    bl_region_type = "UI"
    bl_category = "Hammer Pro"

    def draw(self, context):
        layout = self.layout
        link = read_link()
        if link:
            layout.label(text=os.path.basename(link["file"]), icon="LINKED")
        else:
            layout.label(text="Not linked", icon="UNLINKED")
        layout.operator("hammerpro.update", icon="EXPORT")
        layout.prop(context.scene, "hammerpro_export_on_save")


class HAMMERPRO_MT_menu(bpy.types.Menu):
    bl_label = "Hammer Pro"
    bl_idname = "HAMMERPRO_MT_menu"

    def draw(self, context):
        self.layout.operator("hammerpro.update", icon="EXPORT")
        self.layout.prop(context.scene, "hammerpro_export_on_save")


def draw_menu(self, context):
    self.layout.menu("HAMMERPRO_MT_menu")


CLASSES = (HAMMERPRO_OT_update, HAMMERPRO_PT_panel, HAMMERPRO_MT_menu)


def register():
    for cls in CLASSES:
        try:
            bpy.utils.register_class(cls)
        except ValueError:
            pass
    bpy.types.Scene.hammerpro_export_on_save = bpy.props.BoolProperty(
        name="Export on save",
        description="Send the scene to Hammer Pro each time the file is saved",
        default=True,
    )
    bpy.types.TOPBAR_MT_editor_menus.append(draw_menu)
    if on_save not in bpy.app.handlers.save_post:
        bpy.app.handlers.save_post.append(on_save)
    if not bpy.app.timers.is_registered(poll_link):
        bpy.app.timers.register(poll_link, first_interval=1.0, persistent=True)


def unregister():
    if bpy.app.timers.is_registered(poll_link):
        bpy.app.timers.unregister(poll_link)
    if on_save in bpy.app.handlers.save_post:
        bpy.app.handlers.save_post.remove(on_save)
    bpy.types.TOPBAR_MT_editor_menus.remove(draw_menu)
    del bpy.types.Scene.hammerpro_export_on_save
    for cls in reversed(CLASSES):
        bpy.utils.unregister_class(cls)


if __name__ == "__main__":
    register()
"##;

const STARTUP: &str = r##"import importlib.util
import os
import sys

import bpy

ADDON = __ADDON__
SOURCE = __SOURCE__
FORMAT = __FORMAT__

os.environ["HAMMERPRO_LINK"] = __LINK__


def load_addon():
    module = sys.modules.get("hammerpro_bridge")
    if module is not None and "hammerpro_bridge" in bpy.context.preferences.addons.keys():
        return module
    spec = importlib.util.spec_from_file_location("hammerpro_bridge", ADDON)
    module = importlib.util.module_from_spec(spec)
    sys.modules["hammerpro_bridge"] = module
    spec.loader.exec_module(module)
    module.register()
    return module


try:
    load_addon().start(SOURCE, FORMAT)
except Exception as error:
    print("Hammer Pro Bridge: cannot start:", error)
"##;

fn py(text: &str) -> String {
    serde_json::to_string(text).unwrap_or_else(|_| "\"\"".to_owned())
}

fn py_path(path: &Path) -> String {
    py(&path.display().to_string())
}

pub fn addon_source(default_link: &Path) -> String {
    ADDON.replace("__LINK__", &py_path(default_link))
}

pub fn startup_script(addon: &Path, source: &Path, format: BlenderFormat, link: &Path) -> String {
    STARTUP
        .replace("__ADDON__", &py_path(addon))
        .replace("__SOURCE__", &py_path(source))
        .replace("__FORMAT__", &py(format.id()))
        .replace("__LINK__", &py_path(link))
}

pub fn revision() -> u128 {
    std::time::SystemTime::now().duration_since(std::time::UNIX_EPOCH).map_or(0, |d| d.as_millis())
}

pub fn link_json(file: &Path, format: BlenderFormat, export_on_save: bool, revision: u128) -> String {
    serde_json::json!({
        "version": 1,
        "revision": revision.to_string(),
        "file": file.display().to_string(),
        "format": format.id(),
        "export_on_save": export_on_save,
        "selected_only": false,
    })
    .to_string()
}

pub fn config_roots() -> Vec<PathBuf> {
    let Some(base) = hp_app::settings::Settings::config_dir().parent().map(Path::to_path_buf) else {
        return Vec::new();
    };
    if cfg!(windows) {
        vec![base.join("Blender Foundation").join("Blender")]
    } else if cfg!(target_os = "macos") {
        vec![base.join("Blender")]
    } else {
        vec![base.join("blender")]
    }
}

pub fn is_version_dir(name: &str) -> bool {
    let mut parts = name.split('.');
    let major = parts.next().is_some_and(|p| !p.is_empty() && p.chars().all(|c| c.is_ascii_digit()));
    let minor = parts.next().is_some_and(|p| !p.is_empty() && p.chars().all(|c| c.is_ascii_digit()));
    major && minor && parts.next().is_none()
}

pub fn version_dirs(root: &Path) -> Vec<String> {
    let Ok(entries) = std::fs::read_dir(root) else { return Vec::new() };
    let mut out: Vec<String> = entries
        .flatten()
        .filter(|e| e.path().is_dir())
        .map(|e| e.file_name().to_string_lossy().into_owned())
        .filter(|n| is_version_dir(n))
        .collect();
    out.sort_by_key(|n| std::cmp::Reverse(super::find::version_of(n)));
    out
}

pub fn exe_version(exe: &Path) -> Option<String> {
    let dir = exe.parent()?;
    if let Ok(entries) = std::fs::read_dir(dir) {
        let mut found: Vec<String> = entries
            .flatten()
            .filter(|e| e.path().is_dir())
            .map(|e| e.file_name().to_string_lossy().into_owned())
            .filter(|n| is_version_dir(n))
            .collect();
        found.sort_by_key(|n| std::cmp::Reverse(super::find::version_of(n)));
        if let Some(v) = found.into_iter().next() {
            return Some(v);
        }
    }
    let name = dir.file_name()?.to_string_lossy().into_owned();
    let v = super::find::version_of(&name);
    (v.len() >= 2).then(|| format!("{}.{}", v[0], v[1]))
}

pub fn addon_folders(roots: &[PathBuf], exe: Option<&Path>) -> Vec<PathBuf> {
    let mut out = Vec::new();
    for root in roots {
        for v in version_dirs(root).into_iter().take(6) {
            out.push(root.join(v).join("scripts").join("addons"));
        }
    }
    if out.is_empty()
        && let (Some(root), Some(v)) = (roots.first(), exe.and_then(exe_version))
    {
        out.push(root.join(v).join("scripts").join("addons"));
    }
    out
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct Installed {
    pub files: Vec<PathBuf>,
    pub in_blender: bool,
}

pub fn install(roots: &[PathBuf], exe: Option<&Path>, link: &Path, fallback_dir: &Path) -> Result<Installed, String> {
    let source = addon_source(link);
    let folders = addon_folders(roots, exe);
    let mut files = Vec::new();
    for folder in &folders {
        std::fs::create_dir_all(folder).map_err(|e| format!("{}: {e}", folder.display()))?;
        let file = folder.join(format!("{ADDON_ID}.py"));
        std::fs::write(&file, &source).map_err(|e| format!("{}: {e}", file.display()))?;
        files.push(file);
    }
    if files.is_empty() {
        std::fs::create_dir_all(fallback_dir).map_err(|e| format!("{}: {e}", fallback_dir.display()))?;
        let file = fallback_dir.join(format!("{ADDON_ID}.py"));
        std::fs::write(&file, &source).map_err(|e| format!("{}: {e}", file.display()))?;
        return Ok(Installed { files: vec![file], in_blender: false });
    }
    Ok(Installed { files, in_blender: true })
}

pub fn write_working_files(
    work: &Path,
    source: &Path,
    format: BlenderFormat,
    export_on_save: bool,
) -> Result<(PathBuf, PathBuf), String> {
    std::fs::create_dir_all(work).map_err(|e| format!("{}: {e}", work.display()))?;
    let link = work.join("link.json");
    let addon = work.join(format!("{ADDON_ID}.py"));
    let startup = work.join("startup.py");
    let write = |p: &Path, text: &str| std::fs::write(p, text).map_err(|e| format!("{}: {e}", p.display()));
    write(&link, &link_json(source, format, export_on_save, revision()))?;
    write(&addon, &addon_source(&link))?;
    write(&startup, &startup_script(&addon, source, format, &link))?;
    Ok((startup, link))
}

#[cfg(test)]
mod tests {
    use super::*;

    fn scratch(tag: &str) -> PathBuf {
        let dir = std::env::temp_dir().join(format!("hp_blender_addon_{tag}_{}", std::process::id()));
        let _ = std::fs::remove_dir_all(&dir);
        std::fs::create_dir_all(&dir).expect("dir");
        dir
    }

    #[test]
    fn scripts_embed_their_paths_as_python_literals() {
        let link = Path::new("C:\\Users\\a b\\link.json");
        let addon = addon_source(link);
        assert!(addon.contains("DEFAULT_LINK = \"C:\\\\Users\\\\a b\\\\link.json\""), "{addon}");
        assert!(!addon.contains("__LINK__"));
        let script = startup_script(Path::new("/x/addon.py"), Path::new("/x/a\"b.glb"), BlenderFormat::Fbx, link);
        assert!(script.contains("FORMAT = \"fbx\"") && script.contains("a\\\"b.glb"), "{script}");
        assert!(!script.contains("__"));
        for text in [&addon, &script] {
            assert!(!text.contains('\u{2014}'), "no em dash");
            assert!(!text.lines().any(|l| l.trim_start().starts_with('#')), "no comment");
        }
        assert!(addon.contains("hammerpro.update") && addon.contains("Update in Hammer Pro"));
        assert!(addon.contains("save_post") && addon.contains("TOPBAR_MT_editor_menus"));
        let json: serde_json::Value =
            serde_json::from_str(&link_json(Path::new("a.glb"), BlenderFormat::Gltf, true, 5)).unwrap();
        assert_eq!(json["format"], "gltf");
        assert_eq!(json["export_on_save"], true);
    }

    #[test]
    fn version_folders_pick_the_scripts_folder() {
        assert!(is_version_dir("4.2") && is_version_dir("2.93") && !is_version_dir("4") && !is_version_dir("4.2.1"));
        assert!(!is_version_dir("config") && !is_version_dir("4.x"));
        let dir = scratch("folders");
        let root = dir.join("Blender");
        for v in ["3.6", "4.2", "notes"] {
            std::fs::create_dir_all(root.join(v)).expect("v");
        }
        assert_eq!(version_dirs(&root), ["4.2", "3.6"]);
        let folders = addon_folders(std::slice::from_ref(&root), None);
        assert_eq!(folders.len(), 2);
        assert!(folders[0].ends_with("scripts/addons") || folders[0].ends_with("scripts\\addons"));
        let fresh = dir.join("fresh");
        let exe_dir = dir.join("Blender 4.1");
        std::fs::create_dir_all(exe_dir.join("4.1")).expect("exe dir");
        std::fs::write(exe_dir.join("blender.exe"), "x").expect("exe");
        let guess = addon_folders(std::slice::from_ref(&fresh), Some(&exe_dir.join("blender.exe")));
        assert_eq!(guess.len(), 1);
        assert!(guess[0].to_string_lossy().contains("4.1"));
        let none = addon_folders(std::slice::from_ref(&fresh), None);
        assert!(none.is_empty());
        let _ = std::fs::remove_dir_all(&dir);
    }

    #[test]
    fn installing_writes_every_version_or_the_working_folder() {
        let dir = scratch("install");
        let root = dir.join("Blender");
        std::fs::create_dir_all(root.join("4.2")).expect("v");
        let link = dir.join("link.json");
        let work = dir.join("work");
        let done = install(std::slice::from_ref(&root), None, &link, &work).expect("install");
        assert!(done.in_blender && done.files.len() == 1);
        assert!(std::fs::read_to_string(&done.files[0]).expect("read").contains("Hammer Pro Bridge"));
        let elsewhere = install(&[dir.join("none")], None, &link, &work).expect("fallback");
        assert!(!elsewhere.in_blender);
        assert!(elsewhere.files[0].starts_with(&work));
        let (startup, link_file) =
            write_working_files(&work, &dir.join("part.glb"), BlenderFormat::Gltf, true).expect("work");
        assert!(startup.is_file() && link_file.is_file());
        assert!(work.join("hammerpro_bridge.py").is_file());
        let _ = std::fs::remove_dir_all(&dir);
    }
}
