use hp_app::AppState;
use hp_app::settings::CombineOrigin;
use hp_mcp::protocol::ToolResult;
use hp_ui_mapping::propper::combine::{CombineSettings, plan, summary};
use serde_json::{Value, json};

use super::semantic::*;
use super::tools::{ApplyFn, Args, MAX_NAME, Outcome, R, Worker, encode_png};

domain! { "props";
    combine_props: Edit, "Combine props",
        "Merge several prop_static (same or different models, any skins) into one new static model to cut \
        draw calls, like Propper++. Each prop's mesh is read from its mdl/vvd/vtx (body group and skin of the \
        prop: the material of the chosen skin is baked), moved into one local space (origin at the center or \
        the bottom center of the selection), merged by material (one draw call per material instead of one \
        per prop and material), its collision taken from the .phy convex pieces (or a convex hull when the \
        model has none; props with solid 0 add none), capped at max_pieces (pieces of a prop, then nearby \
        props, are merged into hulls). Levels of detail are rebuilt with the model LOD settings. The model is \
        written as SMD and QC, compiled with the configured model compiler (compile.model_compiler, default \
        the game's studiomdl, plus compile.model_compiler_args) and exported under models/ of the destination \
        folder (default the game content folder). The props are replaced by one prop_static carrying their \
        shared lighting flags (disableshadows, disablevertexlighting...), fade distances when equal, and solid \
        6 when one of them was solid; the originals go to the hidden visgroup Combined props (originals) \
        (keep_originals, default the setting lab.combine_keep_originals) or are deleted. One undo step. \
        Refused with a clear message for props that are not prop_static, models with bones or animations, \
        props whose lighting flags differ, missing models or more than 150000 triangles. dry_run only plans \
        (triangles, draw calls before and after, collision pieces) without writing anything.",
        {
            "ids": ids("prop_static entity ids (default: the selected prop_static entities)."),
            "model": string("Model path under models/ without extension (default hammerpro/combined/<map>_<hash>)."),
            "destination": string("Absolute content folder the model is exported to (default the game folder)."),
            "origin": choice("Origin of the new model (default lab.combine_origin).", &["center", "bottom"]),
            "keep_originals": boolean("Keep the original props in a hidden visgroup instead of deleting them."),
            "collision": boolean("Build collision (default lab.combine_collision)."),
            "max_pieces": integer("Most convex collision pieces (default lab.combine_max_pieces).", 1, 128),
            "dry_run": boolean("Only plan, write and change nothing (default false)."),
        }, [];
    set_model_compiler: Editor, "Set the model compiler",
        "Read or change the model compiler used by Propper, combine_props and Compile a QC (Settings > \
        Compile > Model compiler, the settings compile.model_compiler and compile.model_compiler_args). \
        Without arguments it answers the current choice and the program it resolves to. path is the program \
        (an existing .exe, for example NekoMDL, a drop in studiomdl replacement); an empty path goes back to \
        automatic (the game's studiomdl in bin/win64, else bin). args are extra arguments put after -game \
        <folder> -nop4 -verbose and before the QC file. Changing path or args runs programs on this \
        computer: needs confirm true, only when the user asked.",
        {
            "path": string("Program path; empty text for automatic (the game's studiomdl)."),
            "args": string("Extra arguments, for example -maxbones 128."),
            "confirm": boolean("The user asked for this (needed to change path or args)."),
        }, [];
    preview_prop_lods: Editor, "Preview the levels of detail of a prop",
        "What the levels of detail of a prop made from brushes (Convert brushes to a prop, Propper) will be, \
        made by the same simplifier as the export so the numbers are the ones that get compiled: per level \
        its triangle count, the part of the full model it keeps and the part it was asked to keep (short_of_target when simplifying further would break the shape), its $lod switch value and about how far \
        the game camera is when it switches to it (1080p, field of view 75). The brushes are ids or \
        solid_ids (default the selection). enabled, levels, keep_percent and distances replace the saved \
        settings lab.model_lod for this preview; apply true saves them into lab.model_lod (the next prop, \
        the model import and an open creation window use them at once). The creation window \
        (run_action brushes_to_prop) shows the level chosen with lod in its 3D preview, or with auto true \
        the level the game would draw at the preview camera distance (setting lab.propper_lod_auto), \
        camera_distance moving its camera; the answer tells what the window shows. With image (default \
        true) the answer also holds a drawing of every level side by side, triangle edges included.",
        {
            "ids": target_ids(),
            "solid_ids": target_solid_ids(),
            "enabled": boolean("Make levels of detail (default lab.model_lod.enabled)."),
            "levels": integer("Levels below the full model (default lab.model_lod.levels).", 3, 6),
            "keep_percent": number("Part of the triangles each level keeps of the level above (default lab.model_lod.reduction times 100).", 20.0, 90.0),
            "distances": {
                "type": "array",
                "items": { "type": "number", "minimum": 0.0, "maximum": 1_000_000.0 },
                "maxItems": 6,
                "description": "Switch value ($lod) of LOD 1 to 6, 0 for automatic (default lab.model_lod.distances).",
            },
            "apply": boolean("Save enabled, levels, keep_percent and distances into lab.model_lod (default false)."),
            "lod": integer("Level the open creation window shows (turns lab.propper_lod_auto off).", 0, 8),
            "auto": boolean("The open creation window picks the level from its camera distance like the game (lab.propper_lod_auto)."),
            "camera_distance": number("Distance in units of the creation window preview camera from the prop center.", 1.0, 1_000_000.0),
            "image": boolean("Add a drawing of every level (default true)."),
            "size": integer("Width and height of the drawing of one level in pixels (default 320).", 128, 1024),
        }, [];
}

fn compiler_json(state: &AppState) -> Value {
    let resolved = hp_app::model_compiler::resolve(&state.settings);
    json!({
        "path": state.settings.compile.model_compiler.as_ref().map(|p| p.display().to_string()),
        "automatic": hp_app::model_compiler::custom(&state.settings).is_none(),
        "args": state.settings.compile.model_compiler_args,
        "resolved": resolved.as_ref().ok().map(|p| p.display().to_string()),
        "error": resolved.err(),
        "name": hp_app::model_compiler::name(&state.settings),
    })
}

fn set_model_compiler(state: &mut AppState, a: &Args<'_>) -> R<ToolResult> {
    let path = a.opt_str("path", 1024)?;
    let extra = a.opt_str("args", 1024)?;
    let confirmed = a.opt_bool("confirm")?;
    if path.is_none() && extra.is_none() {
        return Ok(ToolResult::json(compiler_json(state)));
    }
    if confirmed != Some(true) {
        return Err(
            "the model compiler runs programs on this computer: only when the user asked (confirm: true)".to_owned()
        );
    }
    let new_path = match path.map(str::trim) {
        None => state.settings.compile.model_compiler.clone(),
        Some("") => None,
        Some(p) => {
            let p = local_path(p)?;
            let exe = p.extension().is_some_and(|e| e.eq_ignore_ascii_case("exe"));
            if !p.is_absolute() || !exe || !p.is_file() {
                return Err(format!("{} is not an existing .exe program", p.display()));
            }
            Some(p)
        }
    };
    if let Some(args) = extra
        && args.chars().any(char::is_control)
    {
        return Err("args must be one line".to_owned());
    }
    state.settings.compile.model_compiler = new_path;
    if let Some(args) = extra {
        args.trim().clone_into(&mut state.settings.compile.model_compiler_args);
    }
    state.settings_dirty = true;
    Ok(ToolResult::json(compiler_json(state)))
}

fn plan_json(request: &hp_app::runtime::CombineRequest) -> Value {
    let origin = request.creation.geometry.origin;
    json!({
        "props": request.props,
        "model": format!("models/{}.mdl", request.creation.model),
        "triangles": request.creation.geometry.tris.len(),
        "draw_calls_before": request.draw_calls.0,
        "draw_calls_after": request.draw_calls.1,
        "collision_pieces": request.creation.geometry.pieces.len(),
        "origin": [origin.x, origin.y, origin.z],
        "destination": request.creation.destination.display().to_string(),
        "keep_originals": request.keep_originals,
        "keyvalues": request.keys.iter().map(|(k, v)| json!([k, v])).collect::<Vec<_>>(),
        "warnings": request.warnings,
        "summary": summary(request),
    })
}

fn combine_props(state: &mut AppState, a: &Args<'_>) -> R<Outcome> {
    let props = if a.get("ids").is_some() { a.ids("ids")? } else { hp_app::actions::selected_static_props(state) };
    let mut settings = CombineSettings::from_state(state, &props);
    if let Some(model) = a.opt_str("model", MAX_NAME)? {
        model.clone_into(&mut settings.model);
    }
    if let Some(dest) = a.opt_str("destination", 1024)? {
        let path = local_path(dest)?;
        if !path.is_absolute() {
            return Err("destination must be an absolute folder".to_owned());
        }
        settings.destination = path;
    }
    match a.opt_enum("origin", &["center", "bottom"])?.as_deref() {
        Some("bottom") => settings.origin = CombineOrigin::Bottom,
        Some(_) => settings.origin = CombineOrigin::Center,
        None => {}
    }
    if let Some(keep) = a.opt_bool("keep_originals")? {
        settings.keep_originals = keep;
    }
    if let Some(c) = a.opt_bool("collision")? {
        settings.collision = c;
    }
    if let Some(n) = a.opt_u64("max_pieces", 1, 128)? {
        settings.max_pieces = n as usize;
    }
    let fs =
        state.assets.fs().cloned().ok_or("the game content is not loaded (set the game folder in Settings > Game)")?;
    let request = plan(&fs, &state.doc, &props, &settings)?;
    if a.opt_bool("dry_run")?.unwrap_or(false) {
        let mut body = plan_json(&request);
        body["dry_run"] = json!(true);
        return Ok(ToolResult::json(body).into());
    }
    if hp_compile::sandbox::is_sandboxed() {
        return Err("automated runs do not compile models".to_owned());
    }
    if state.runtime.lab.combine_busy {
        return Err("a combine is already running".to_owned());
    }
    let exe = hp_app::model_compiler::resolve(&state.settings)?;
    let args = state.settings.compile.model_compiler_args.clone();
    let request = Box::new(request);
    Ok(Worker::spawn_then("combine_props", move || {
        let (lines, built) = crate::dispatch::combine::build_blocking(&fs, request, &exe, &args);
        match built {
            Err((_, e)) => {
                let tail: Vec<&str> = lines.iter().rev().take(40).map(|(_, l)| l.as_str()).collect();
                let tail: Vec<&str> = tail.into_iter().rev().collect();
                (ToolResult::error(format!("combine_props: {e}\n{}", tail.join("\n"))), None)
            }
            Ok((request, prepared)) => {
                let apply: ApplyFn = Box::new(move |state: &mut AppState| {
                    for (s, line) in lines {
                        state.console.push(s, line);
                    }
                    Some(match crate::dispatch::combine::apply(state, &request, &prepared) {
                        Ok(id) => {
                            hp_ui_mapping::propper::combine::report_done(state, &request, &prepared.model);
                            let mut body = plan_json(&request);
                            body["prop"] = json!(id);
                            body["model"] = json!(format!("models/{}.mdl", prepared.model));
                            body["exported_to"] = json!(prepared.game_dir.display().to_string());
                            ToolResult::json(body)
                        }
                        Err(e) => ToolResult::error(format!("combine_props: {e}")),
                    })
                });
                (ToolResult::error("combine_props: not applied"), Some(apply))
            }
        }
    }))
}

fn lod_settings(state: &AppState, a: &Args<'_>) -> R<hp_app::settings::ModelLodSettings> {
    let mut lod = state.settings.lab.model_lod;
    if let Some(enabled) = a.opt_bool("enabled")? {
        lod.enabled = enabled;
    }
    if let Some(levels) = a.opt_u64("levels", 3, 6)? {
        lod.levels = levels as u32;
    }
    if let Some(keep) = a.opt_f64("keep_percent", 20.0, 90.0)? {
        lod.reduction = (keep / 100.0) as f32;
    }
    if let Some(values) = a.get("distances") {
        let values = values.as_array().ok_or("distances must be a list of numbers")?;
        if values.len() > 6 {
            return Err("distances holds at most 6 values".to_owned());
        }
        let mut distances = [None; 6];
        for (slot, value) in distances.iter_mut().zip(values) {
            let value = value.as_f64().filter(|v| v.is_finite() && (0.0..=1_000_000.0).contains(v));
            let value = value.ok_or("distances must be numbers from 0 to 1000000")?;
            *slot = (value > 0.0).then_some(value);
        }
        lod.distances = distances;
    }
    Ok(lod.clamped())
}

fn level_mesh(geometry: &hp_editor::prop_export::PropGeometry) -> hp_shape::TriMesh {
    let mut mesh = hp_shape::TriMesh::new();
    for tri in &geometry.tris {
        let corners = tri.pos.map(|p| mesh.add_vertex(p));
        mesh.add_tri(corners, 0);
    }
    mesh
}

fn lod_sheet(levels: &[hp_ui_mapping::propper::lod_preview::LodLevelPreview], size: u32) -> hp_shape::Image {
    let columns = levels.len().clamp(1, 4) as u32;
    let rows = (levels.len() as u32).div_ceil(columns).max(1);
    let (width, height) = (size * columns, size * rows);
    let mut rgba = [28u8, 30, 34, 255].repeat((width * height) as usize);
    let full = levels.first().map_or(0, |level| level.triangles);
    for (index, level) in levels.iter().enumerate() {
        let mesh = level_mesh(&level.geometry);
        let title = if level.index == 0 {
            format!("LOD 0  {} TRIS", level.triangles)
        } else {
            format!("LOD {}  {} TRIS  {:.0} PCT", level.index, level.triangles, level.percent_of(full))
        };
        let panel = hp_shape::render_sheet(
            &[
                hp_shape::Layer { mesh: mesh.clone(), color: [170, 180, 200], wire: false },
                hp_shape::Layer { mesh, color: [40, 220, 255], wire: true },
            ],
            &[],
            &hp_shape::SheetOptions {
                width: size,
                height: size,
                views: vec![hp_shape::ViewKind::Iso],
                dimensions: false,
                grid: false,
                title,
                ground: None,
            },
        );
        let (x0, y0) = ((index as u32 % columns) * size, (index as u32 / columns) * size);
        let len = (panel.width.min(size) * 4) as usize;
        for y in 0..panel.height.min(size) {
            let from = (y * panel.width * 4) as usize;
            let to = (((y0 + y) * width + x0) * 4) as usize;
            rgba[to..to + len].copy_from_slice(&panel.rgba[from..from + len]);
        }
    }
    hp_shape::Image { width, height, rgba }
}

fn preview_prop_lods(state: &mut AppState, a: &Args<'_>) -> R<Outcome> {
    let solids = target_solids(state, a, "solid_ids")?;
    let lod = lod_settings(state, a)?;
    let image = a.opt_bool("image")?.unwrap_or(true);
    let size = a.opt_u64("size", 128, 1024)?.unwrap_or(320) as u32;
    let apply = a.opt_bool("apply")?.unwrap_or(false);
    let auto = a.opt_bool("auto")?;
    let level = a.opt_u64("lod", 0, 8)?;
    let distance = a.opt_f64("camera_distance", 1.0, 1_000_000.0)?;
    let geometry = std::sync::Arc::new(hp_ui_mapping::propper::prop_geometry(state, &solids)?);
    if apply && state.settings.lab.model_lod != lod {
        state.settings.lab.model_lod = lod;
        state.settings_dirty = true;
    }
    if let Some(auto) = auto {
        state.settings.lab.propper_lod_auto = auto;
        state.settings_dirty = true;
    }
    if let Some(level) = level {
        state.runtime.lab.propper_lod_view = level as usize;
        if state.settings.lab.propper_lod_auto {
            state.settings.lab.propper_lod_auto = false;
            state.settings_dirty = true;
        }
    }
    if distance.is_some() {
        state.runtime.lab.propper_lod_camera_request = distance;
    }
    let lab = &state.runtime.lab;
    let window = if lab.propper_create_open {
        json!({
            "open": true,
            "shows_lod": lab.propper_lod_view,
            "auto": state.settings.lab.propper_lod_auto,
            "camera_distance": lab.propper_lod_camera,
            "triangles": lab.propper_lod_levels.iter().map(|(tris, _)| *tris).collect::<Vec<_>>(),
            "note": "the window applies changes at its next frame: call again or take a screenshot to see them",
        })
    } else {
        json!({ "open": false })
    };
    Ok(Worker::spawn("preview_prop_lods", move || {
        let levels = hp_ui_mapping::propper::lod_preview::levels(&geometry, lod);
        let full = levels.first().map_or(0, |level| level.triangles);
        let rows: Vec<Value> = levels
            .iter()
            .map(|level| {
                json!({
                    "lod": level.index,
                    "triangles": level.triangles,
                    "percent_of_full": (level.percent_of(full) * 10.0).round() / 10.0,
                    "target_percent": (level.target * 10.0).round() / 10.0,
                    "short_of_target": level.short_of_target(full),
                    "switch": level.switch,
                    "game_distance": level.game_distance().round(),
                })
            })
            .collect();
        let body = json!({
            "solids": solids.len(),
            "settings": {
                "enabled": lod.enabled,
                "levels": lod.levels,
                "keep_percent": (f64::from(lod.reduction) * 100.0).round(),
                "distances": lod.distances.map(|d| d.unwrap_or(0.0)),
            },
            "levels": rows,
            "window": window,
        });
        let mut result = ToolResult::json(body.clone());
        if image {
            let sheet = lod_sheet(&levels, size);
            match encode_png(sheet.width, sheet.height, sheet.rgba) {
                Ok(png) => result.content.extend(ToolResult::png(&png, "levels of detail, LOD 0 first").content),
                Err(e) => return ToolResult::error(format!("preview_prop_lods: {e}")),
            }
        }
        result.structured = Some(body);
        result
    }))
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::dispatch::mcp::tools::run as run_tool;

    fn call(state: &mut AppState, name: &str, v: Value) -> ToolResult {
        match run_tool(state, name, &v) {
            Outcome::Done(r) => r,
            Outcome::Worker(w) => w.wait(state, std::time::Duration::from_secs(60)),
            other => panic!("unexpected {other:?}"),
        }
    }

    #[test]
    fn the_model_compiler_needs_confirm_and_an_existing_program() {
        let mut state = hp_app::testing::test_state();
        let read = call(&mut state, "set_model_compiler", json!({}));
        assert!(!read.is_error, "{}", read.first_text());
        let dir = std::env::temp_dir().join(format!("hp_model_compiler_mcp_{}", std::process::id()));
        std::fs::create_dir_all(&dir).unwrap();
        let exe = dir.join("nekomdl.exe");
        let path = exe.display().to_string();
        let refused = call(&mut state, "set_model_compiler", json!({ "path": path }));
        assert!(refused.is_error && refused.first_text().contains("confirm"));
        let missing = call(&mut state, "set_model_compiler", json!({ "path": path, "confirm": true }));
        assert!(missing.is_error, "{}", missing.first_text());
        std::fs::write(&exe, b"x").unwrap();
        let ok =
            call(&mut state, "set_model_compiler", json!({ "path": path, "args": "-maxbones 128", "confirm": true }));
        assert!(!ok.is_error, "{}", ok.first_text());
        assert_eq!(state.settings.compile.model_compiler.as_deref(), Some(exe.as_path()));
        assert_eq!(state.settings.compile.model_compiler_args, "-maxbones 128");
        let v: Value = serde_json::from_str(ok.first_text()).unwrap();
        assert_eq!(v["resolved"], json!(path));
        let back = call(&mut state, "set_model_compiler", json!({ "path": "", "confirm": true }));
        assert!(!back.is_error);
        assert!(state.settings.compile.model_compiler.is_none());
        let set = call(&mut state, "set_setting", json!({ "path": "compile.model_compiler", "value": path }));
        assert!(set.is_error && set.first_text().contains("set_model_compiler"));
        let _ = std::fs::remove_dir_all(&dir);
    }

    #[test]
    fn prop_lod_preview_counts_every_level_and_drives_the_window() {
        let mut state = hp_app::testing::test_state();
        let none = call(&mut state, "preview_prop_lods", json!({}));
        assert!(none.is_error, "{}", none.first_text());
        let mut ids = Vec::new();
        for i in 0..6 {
            let x = i * 40;
            let made = call(&mut state, "create_primitive", json!({ "min": [x, 0, 0], "max": [x + 32, 32, 32] }));
            assert!(!made.is_error, "{}", made.first_text());
            let v: Value = serde_json::from_str(made.first_text()).unwrap();
            ids.push(v["solid_ids"][0].as_u64().unwrap());
        }
        let r = call(
            &mut state,
            "preview_prop_lods",
            json!({ "solid_ids": ids, "levels": 3, "keep_percent": 60, "distances": [50, 0, 0], "lod": 1, "size": 128 }),
        );
        assert!(!r.is_error, "{}", r.first_text());
        let v = r.structured.clone().unwrap();
        let levels = v["levels"].as_array().unwrap();
        assert_eq!(levels[0]["triangles"], json!(72));
        assert!(levels.len() >= 2, "{v}");
        assert_eq!(levels[1]["switch"], json!(50.0));
        assert_eq!(v["settings"]["keep_percent"], json!(60.0));
        assert!(r.content.iter().any(|c| matches!(c, hp_mcp::protocol::Content::Image { .. })));
        assert_eq!(state.runtime.lab.propper_lod_view, 1);
        assert_eq!(state.settings.lab.model_lod, hp_app::settings::ModelLodSettings::default());
        let applied = call(
            &mut state,
            "preview_prop_lods",
            json!({ "solid_ids": ids, "keep_percent": 70, "apply": true, "image": false, "auto": true }),
        );
        assert!(!applied.is_error, "{}", applied.first_text());
        assert!((state.settings.lab.model_lod.reduction - 0.7).abs() < 1e-6);
        assert!(state.settings.lab.propper_lod_auto);
        assert_eq!(applied.content.len(), 1);
        let bad =
            call(&mut state, "preview_prop_lods", json!({ "solid_ids": ids, "distances": [1, 2, 3, 4, 5, 6, 7] }));
        assert!(bad.is_error);
    }

    #[test]
    fn combining_needs_two_static_props() {
        let mut state = hp_app::testing::test_state();
        let e = state
            .doc
            .create_point_entity("prop_dynamic", glam::DVec3::ZERO, &[("model".into(), "models/a.mdl".into())])
            .unwrap();
        let r = call(&mut state, "combine_props", json!({ "ids": [e], "dry_run": true }));
        assert!(r.is_error, "{}", r.first_text());
    }
}
