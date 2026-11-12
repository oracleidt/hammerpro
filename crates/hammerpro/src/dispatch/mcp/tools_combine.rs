use hp_app::AppState;
use hp_app::settings::CombineOrigin;
use hp_mcp::protocol::ToolResult;
use hp_ui_mapping::propper::combine::{CombineSettings, plan, summary};
use serde_json::{Value, json};

use super::semantic::*;
use super::tools::{ApplyFn, Args, MAX_NAME, Outcome, R, Worker};

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
