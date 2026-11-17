use std::path::{Path, PathBuf};

use hp_app::AppState;
use hp_app::settings::CompileSettings;
use hp_light::rad_doc::{Pass, RadDoc, TexLightLine};
use hp_mcp::protocol::ToolResult;
use serde_json::{Value, json};

use super::semantic::*;
use super::tools::{Args, R};

const MAX_RAD_BYTES: u64 = 4 << 20;

domain! { "light files";
    edit_rad_file: Editor, "Read or edit a .rad light file",
        "Read, create or edit a .rad light file (the one of the compile settings rad_file by default, see \
        set_compile_options rad_file_enabled and rad_file; or file: an absolute path or a path relative to the map \
        folder, or \"map\" for <map>.rad beside the map). Returns its texture lights (material, color 0 to 255, \
        brightness, the optional HDR color and brightness used by the HDR pass, pass both, ldr or hdr for the \
        ldr: and hdr: prefixed lines), its noshadow materials (static prop meshes with them cast no shadow) and its \
        forcetextureshadow models (static props of these models cast shadows from their render mesh with the alpha \
        of their textures, never their collision model). set_texlights adds or replaces lines, remove_texlights, \
        add_noshadow, remove_noshadow, add_forcetextureshadow and remove_forcetextureshadow edit the lists; the \
        other lines and comments are kept. create true makes the file when it is missing; use true also selects it \
        as the compile rad_file (rad_file_enabled on). Hammer Pro's compiler, Valve's vrad.exe (-lights) and the \
        Baked, Player view, Ray traced and Lit previews read it; the previews bake again within a second of a save. \
        The game lights.rad is never written: copy what you need into your own file.",
        {
            "file": string("The .rad file: absolute, relative to the map folder, or \"map\" (<map>.rad). Default: the compile rad_file."),
            "create": boolean("Create the file when it is missing."),
            "use": boolean("Select this file as the compile rad_file and turn rad_file_enabled on."),
            "set_texlights": { "type": "array", "maxItems": 512, "description": "Texture lights to add or replace.", "items": { "type": "object", "properties": {
                "material": { "type": "string", "description": "Material name like lights/white001." },
                "color": { "type": "array", "items": { "type": "number", "minimum": 0, "maximum": 100000 }, "minItems": 3, "maxItems": 3, "description": "r g b, 0 to 255 usually." },
                "brightness": { "type": "number", "minimum": 0, "maximum": 1000000, "description": "Brightness (the fourth value)." },
                "hdr_color": { "type": "array", "items": { "type": "number", "minimum": 0, "maximum": 100000 }, "minItems": 3, "maxItems": 3, "description": "Optional HDR r g b (the HDR pass uses it)." },
                "hdr_brightness": { "type": "number", "minimum": 0, "maximum": 1000000, "description": "Optional HDR brightness." },
                "pass": { "type": "string", "enum": ["both", "ldr", "hdr"], "description": "Line prefix: both (none, default), ldr: or hdr:." }
            }, "required": ["material", "color", "brightness"] } },
            "remove_texlights": { "type": "array", "maxItems": 512, "items": { "type": "string" }, "description": "Materials whose texture light lines go away." },
            "add_noshadow": { "type": "array", "maxItems": 512, "items": { "type": "string" }, "description": "Materials whose static prop meshes cast no shadow." },
            "remove_noshadow": { "type": "array", "maxItems": 512, "items": { "type": "string" }, "description": "noshadow lines to remove." },
            "add_forcetextureshadow": { "type": "array", "maxItems": 512, "items": { "type": "string" }, "description": "Models (props/fence.mdl or models/props/fence.mdl) whose static props shadow with their render mesh and texture alpha." },
            "remove_forcetextureshadow": { "type": "array", "maxItems": 512, "items": { "type": "string" }, "description": "forcetextureshadow lines to remove." },
        }, [];
}

fn game_dir(state: &AppState) -> Option<PathBuf> {
    state.assets.fs().and_then(hp_fs::GameFs::game_dir).map(Path::to_path_buf)
}

pub(super) fn rad_files_json(state: &AppState) -> Value {
    let map = state.doc.path();
    let mut out = Vec::new();
    let mut push = |role: &str, p: PathBuf| {
        out.push(json!({ "role": role, "path": p.display().to_string(), "exists": p.is_file() }));
    };
    if let Some(g) = game_dir(state) {
        push("game", g.join("lights.rad"));
    }
    if let Some(p) = state.settings.compile.lights_file().and_then(|f| hp_app::settings::resolve_rad_path(&f, map)) {
        push("chosen", p);
    }
    if let Some(m) = map {
        push("map", m.with_extension("rad"));
    }
    Value::Array(out)
}

pub(super) fn apply_rad_options(state: &AppState, s: &mut CompileSettings, a: &Args<'_>) -> R<()> {
    let enabled = a.opt_bool("rad_file_enabled")?;
    if let Some(file) = a.opt_str("rad_file", 1024)? {
        let file = file.trim();
        if !file.is_empty() {
            let path = hp_app::settings::resolve_rad_path(file, state.doc.path())
                .ok_or_else(|| "rad_file: a path is needed".to_owned())?;
            if !path.is_file() {
                return Err(format!(
                    "rad_file: {} does not exist (edit_rad_file with create true makes it)",
                    path.display()
                ));
            }
        }
        s.rad_file = file.to_owned();
        s.rad_file_enabled = enabled.unwrap_or(!file.is_empty());
    } else if let Some(v) = enabled {
        if v && s.rad_file.trim().is_empty() {
            return Err("rad_file_enabled needs rad_file (a .rad file)".to_owned());
        }
        s.rad_file_enabled = v;
    }
    Ok(())
}

fn target_file(state: &AppState, a: &Args<'_>) -> R<PathBuf> {
    let map = state.doc.path();
    match a.opt_str("file", 1024)?.map(str::trim).filter(|f| !f.is_empty()) {
        Some(f) if f.eq_ignore_ascii_case("map") => {
            map.map(|m| m.with_extension("rad")).ok_or_else(|| "file \"map\": save the map first".to_owned())
        }
        Some(f) => hp_app::settings::resolve_rad_path(f, map).ok_or_else(|| "file: a path is needed".to_owned()),
        None => state
            .settings
            .compile
            .lights_file()
            .and_then(|f| hp_app::settings::resolve_rad_path(&f, map))
            .ok_or_else(|| "no rad_file is chosen: give file (or set_compile_options rad_file)".to_owned()),
    }
}

fn strings(a: &Args<'_>, key: &str) -> R<Vec<String>> {
    match a.get(key) {
        None => Ok(Vec::new()),
        Some(Value::Array(list)) => list
            .iter()
            .map(|v| v.as_str().map(|s| s.trim().to_owned()).ok_or_else(|| format!("{key}: expected strings")))
            .collect(),
        Some(_) => Err(format!("{key}: expected an array of strings")),
    }
}

fn triple(v: Option<&Value>, key: &str) -> R<Option<[f32; 3]>> {
    let Some(v) = v else { return Ok(None) };
    let list = v.as_array().filter(|l| l.len() == 3).ok_or_else(|| format!("{key}: expected [r, g, b]"))?;
    let mut out = [0.0f32; 3];
    for (o, x) in out.iter_mut().zip(list) {
        let n = x.as_f64().filter(|n| n.is_finite() && *n >= 0.0).ok_or_else(|| format!("{key}: expected numbers"))?;
        *o = n as f32;
    }
    Ok(Some(out))
}

fn texlight_of(v: &Value) -> R<TexLightLine> {
    let material = v
        .get("material")
        .and_then(Value::as_str)
        .map(str::trim)
        .filter(|m| !m.is_empty() && !m.contains(char::is_whitespace))
        .ok_or_else(|| "set_texlights: material is needed (no spaces)".to_owned())?;
    let color = triple(v.get("color"), "color")?.ok_or_else(|| "set_texlights: color is needed".to_owned())?;
    let number = |key: &str| -> R<Option<f32>> {
        match v.get(key) {
            None => Ok(None),
            Some(x) => x
                .as_f64()
                .filter(|n| n.is_finite() && *n >= 0.0)
                .map(|n| Some(n as f32))
                .ok_or_else(|| format!("set_texlights: {key} must be a positive number")),
        }
    };
    let brightness = number("brightness")?.ok_or_else(|| "set_texlights: brightness is needed".to_owned())?;
    let hdr = match (triple(v.get("hdr_color"), "hdr_color")?, number("hdr_brightness")?) {
        (Some(c), Some(b)) => Some((c, b)),
        (None, None) => None,
        _ => return Err("set_texlights: give both hdr_color and hdr_brightness".to_owned()),
    };
    let pass = match v.get("pass").and_then(Value::as_str) {
        None => Pass::Both,
        Some(p) => Pass::parse(p).ok_or_else(|| format!("set_texlights: pass {p} is not both, ldr or hdr"))?,
    };
    Ok(TexLightLine { pass, material: material.to_owned(), color, brightness, hdr })
}

pub(crate) fn doc_json(doc: &RadDoc) -> Value {
    json!({
        "texlights": doc.texlights().iter().map(|t| json!({
            "material": t.material,
            "pass": t.pass.name(),
            "color": t.color,
            "brightness": t.brightness,
            "hdr_color": t.hdr.map(|h| h.0),
            "hdr_brightness": t.hdr.map(|h| h.1),
        })).collect::<Vec<_>>(),
        "noshadow": doc.no_shadow(),
        "forcetextureshadow": doc.force_texture_shadow(),
    })
}

fn edit_rad_file(state: &mut AppState, a: &Args<'_>) -> R<ToolResult> {
    let path = target_file(state, a)?;
    if game_dir(state).is_some_and(|g| path == g.join("lights.rad")) && a.map.keys().any(|k| k != "file") {
        return Err("the game lights.rad is never written: edit your own file".to_owned());
    }
    let exists = path.is_file();
    let create = a.opt_bool("create")? == Some(true);
    if !exists && !create {
        return Err(format!("{} does not exist (create true makes it)", path.display()));
    }
    let mut doc = if exists {
        if std::fs::metadata(&path).map_or(0, |m| m.len()) > MAX_RAD_BYTES {
            return Err(format!("{} is larger than 4 MB", path.display()));
        }
        let bytes = std::fs::read(&path).map_err(|e| format!("{}: {e}", path.display()))?;
        RadDoc::parse(&String::from_utf8_lossy(&bytes))
    } else {
        RadDoc::default()
    };
    let before = doc.clone();
    let mut texlights = Vec::new();
    if let Some(v) = a.get("set_texlights") {
        let list = v.as_array().ok_or_else(|| "set_texlights: expected an array".to_owned())?;
        for item in list {
            texlights.push(texlight_of(item)?);
        }
    }
    for t in texlights {
        doc.set_texlight(t);
    }
    for m in strings(a, "remove_texlights")? {
        doc.remove_texlight(&m, None);
    }
    for m in strings(a, "add_noshadow")? {
        doc.add_no_shadow(&m);
    }
    for m in strings(a, "remove_noshadow")? {
        doc.remove_no_shadow(&m);
    }
    for m in strings(a, "add_forcetextureshadow")? {
        doc.add_force_texture_shadow(&m);
    }
    for m in strings(a, "remove_forcetextureshadow")? {
        doc.remove_force_texture_shadow(&m);
    }
    let written = doc != before || !exists;
    if written {
        if let Some(dir) = path.parent() {
            std::fs::create_dir_all(dir).map_err(|e| format!("{}: {e}", dir.display()))?;
        }
        std::fs::write(&path, doc.text()).map_err(|e| format!("{}: {e}", path.display()))?;
    }
    let used = a.opt_bool("use")? == Some(true);
    if used {
        let s = &mut state.settings.compile;
        s.rad_file = path.display().to_string();
        s.rad_file_enabled = true;
        state.settings_dirty = true;
    }
    let mut out = doc_json(&doc);
    if let Value::Object(m) = &mut out {
        m.insert("file".into(), json!(path.display().to_string()));
        m.insert("written".into(), json!(written));
        m.insert(
            "used".into(),
            json!(state.settings.compile.chosen_rad_file().is_some_and(|f| {
                hp_app::settings::resolve_rad_path(f, state.doc.path()).is_some_and(|p| p == path)
            })),
        );
    }
    Ok(ToolResult::json(out))
}

#[cfg(test)]
mod tests {
    use serde_json::json;

    use super::super::semantic_tests::{err, ok};

    #[test]
    fn rad_files_are_edited_and_chosen() {
        let mut s = hp_app::testing::test_state();
        let dir = std::env::temp_dir().join(format!("hp_mcp_rad_{}", std::process::id()));
        let _ = std::fs::remove_dir_all(&dir);
        std::fs::create_dir_all(&dir).unwrap();
        let file = dir.join("mine.rad").display().to_string();
        err(&mut s, "edit_rad_file", json!({}));
        err(&mut s, "edit_rad_file", json!({ "file": file }));
        err(&mut s, "set_compile_options", json!({ "rad_file": file }));
        let v = ok(
            &mut s,
            "edit_rad_file",
            json!({
                "file": file, "create": true, "use": true,
                "set_texlights": [
                    { "material": "lights/panel", "color": [255, 240, 200], "brightness": 300 },
                    { "material": "lights/red", "color": [255, 0, 0], "brightness": 50, "hdr_color": [255, 0, 0], "hdr_brightness": 400 }
                ],
                "add_forcetextureshadow": ["props_wasteland/exterior_fence002a.mdl"],
                "add_noshadow": ["foliage/leaves"]
            }),
        );
        assert_eq!(v["written"], true);
        assert_eq!(v["used"], true);
        assert_eq!(v["texlights"].as_array().unwrap().len(), 2);
        assert_eq!(v["texlights"][1]["hdr_brightness"], 400.0);
        let text = std::fs::read_to_string(&file).unwrap();
        assert!(text.contains("lights/red 255 0 0 50 255 0 0 400"), "{text}");
        assert!(text.contains("forcetextureshadow props_wasteland/exterior_fence002a.mdl"));
        assert!(s.settings.compile.rad_file_enabled);
        let o = ok(&mut s, "get_compile_options", json!({}));
        assert_eq!(o["rad_file_enabled"], true);
        assert!(o["rad_files"].as_array().unwrap().iter().any(|f| f["role"] == "chosen" && f["exists"] == true));
        let v = ok(
            &mut s,
            "edit_rad_file",
            json!({ "remove_texlights": ["lights/red"], "remove_noshadow": ["foliage/leaves"] }),
        );
        assert_eq!(v["texlights"].as_array().unwrap().len(), 1);
        assert_eq!(v["noshadow"].as_array().unwrap().len(), 0);
        ok(&mut s, "set_compile_options", json!({ "rad_file_enabled": false }));
        assert!(!s.settings.compile.rad_file_enabled);
        err(
            &mut s,
            "edit_rad_file",
            json!({ "file": file, "set_texlights": [{ "material": "x", "color": [1, 2], "brightness": 1 }] }),
        );
        ok(&mut s, "set_compile_options", json!({ "rad_file": "" }));
        assert!(!s.settings.compile.rad_file_enabled && s.settings.compile.rad_file.is_empty());
        let _ = std::fs::remove_dir_all(&dir);
    }
}
