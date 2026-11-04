use std::path::{Path, PathBuf};

use hp_app::settings::{
    BspModeSetting, CompilePresetSetting, CompileSettings, FlagChoiceSetting, LightingQuality, LightingSetting,
    PackPolicy, RadModeSetting, VisModeSetting,
};
use hp_app::{Action, AppState};
use hp_compile::flags::{self, FlagInfo, flag_info};
use hp_compile::knowledge::{Fix, Knowledge};
use hp_compile::{LogLocation, StepKind};
use hp_mcp::protocol::ToolResult;
use serde_json::{Value, json};

use super::semantic::*;
use super::tools::{AppFn, Args, Outcome, R, Worker, compile_json, v3};

domain! { "compile and play";
    get_compile_options: Read, "Compile options",
        "The compile settings: preset, steps, geometry, visibility and lighting modes, dynamic ranges (LDR, HDR), \
        the static prop options (static_prop_lighting, static_prop_shadows), the options of the geometry and \
        lighting steps (bsp_options, rad_options: the flags forced on or off in the options menu of the compile \
        settings, the flags of the extra arguments, the flags the preset adds and the resulting command_flags), \
        cordon compile, smart pipeline, packing, cubemaps, threads, copy to the maps folder and its folder, \
        starting the game, expert mode and its profiles, and the compiler chosen for the game (compiler: \
        Hammer Pro's own by default, see list_compilers). set_compile_options changes them.",
        {}, [];
    compile_with_options: Editor, "Compile with options",
        "Compile the saved map with options for this compile only (the settings are put back): preset, steps, \
        modes (entities only, fast vis, final rad...), lighting, cordon only, smart pipeline, hidden objects, \
        threads, an expert profile, or another compiler (compiler: an id or name of list_compilers, builtin for \
        Hammer Pro's own). Never starts the game (compile_map does with launch_game). Poll get_compile_status \
        and get_compile_log.",
        {
            "preset": choice("Preset.", &["draft", "normal", "final", "custom"]),
            "run_bsp": boolean("Build the geometry."),
            "run_vis": boolean("Compute the visibility (done by the geometry step)."),
            "run_rad": boolean("Compute the lighting."),
            "bsp_mode": choice("Geometry: normal, entities only, static props only.", &["normal", "onlyents", "onlyprops"]),
            "vis_mode": choice("Visibility: fast (coarse) or normal (full).", &["fast", "normal"]),
            "rad_mode": choice("Lighting quality.", &["draft", "normal", "final"]),
            "lighting": choice("Dynamic ranges lit.", &["ldr", "hdr", "both"]),
            "static_prop_lighting": boolean("Bake the vertex lighting of the static props when the quality asks for it \
                (Final); false: never (-StaticPropLighting is left out, even from rad_options)."),
            "static_prop_shadows": boolean("Static props cast shadows in the lighting; false: none at all \
                (-nostaticpropshadows, much faster on maps with many props)."),
            "cordon_only": boolean("Compile only what is inside the cordons."),
            "smart": boolean("Smart pipeline (skip steps whose input did not change)."),
            "compile_hidden": boolean("Compile hidden objects too."),
            "threads": integer("Compiler threads (0: all).", 0, 256),
            "profile": string("Expert profile name (runs the profile's steps)."),
            "bsp_options": flag_options("Options of the geometry and visibility step for this compile, on top of the \
                saved ones, like the options menu: flag name without the dash (get_compilers lists them) to true \
                (switch), a number, [cluster, cluster] for trace, a folder of the map folder for embed to force it \
                on; false forces it off (even when the preset adds it); null follows the preset again. Example \
                {\"nodetail\": true, \"luxelscale\": 2, \"radius_override\": 4096}."),
            "rad_options": flag_options("Options of the lighting step for this compile, like bsp_options: \
                {\"final\": true, \"bounce\": 8, \"fast\": false, \"softsun\": 2, \"extrasky\": 4, \"lights\": \
                \"mylights.rad\"} (lights: a file of the map folder)."),
            "copy_to_maps": boolean("Copy the BSP to the game's maps folder after this compile."),
            "compiler": string("Compiler for this compile only: an id or name of list_compilers (builtin: Hammer Pro's own)."),
        }, [];
    set_compile_options: Editor, "Change the compile settings",
        "Change the saved compile settings (the compile window and the toolbar use them): preset, steps, modes, \
        lighting, threads, priority, leaks, cordon, smart pipeline, packing, development addon, cubemaps, \
        progressive play, history, the compile window as a separate window (separate_window), expert profile, copy \
        to the maps folder, its folder, starting the game after a \
        compile, the static prop options (static_prop_lighting, static_prop_shadows) and the options menu of the \
        geometry step (bsp_options: nodetail, nowater, fulldetail, forceskyvis, luxelscale, radius_override, \
        AllowDynamicPropsAsStatic, wsaddons, serveraddons, propdebug, blocksize (0: no blocks)...) and of the \
        lighting step \
        (rad_options: final, fast, bounce, \
        StaticPropLighting, StaticPropPolys, textureshadows, lights, chop, smooth, softsun, extrasky...): each \
        flag forced on (true or a value), forced off (false) or following the preset (null). Starting the game \
        after compiles and the maps folder need confirm: true (only when the user asked). Answers the settings \
        like get_compile_options.",
        {
            "preset": choice("Preset.", &["draft", "normal", "final", "custom"]),
            "run_bsp": boolean("Build the geometry."),
            "run_vis": boolean("Compute the visibility (done by the geometry step)."),
            "run_rad": boolean("Compute the lighting."),
            "bsp_mode": choice("Geometry: normal, entities only, static props only.", &["normal", "onlyents", "onlyprops"]),
            "vis_mode": choice("Visibility: fast (coarse) or normal (full).", &["fast", "normal"]),
            "rad_mode": choice("Lighting quality.", &["draft", "normal", "final"]),
            "lighting": choice("Dynamic ranges lit.", &["ldr", "hdr", "both"]),
            "static_prop_lighting": boolean("Light static props per vertex when the quality asks for it (Final); \
                false: never (-StaticPropLighting is left out, even from rad_options)."),
            "static_prop_shadows": boolean("Static props cast shadows in the lighting; false: none at all \
                (-nostaticpropshadows, much faster on maps with many props). The lighting preview follows it."),
            "threads": integer("Compiler threads (0: automatic).", 0, 256),
            "low_priority": boolean("Keep a processor core for the editor."),
            "stop_on_leak": boolean("Stop after the geometry when the map leaks."),
            "cordon_only": boolean("With an active cordon, compile only the cordoned area."),
            "smart": boolean("Smart pipeline (skip steps whose input did not change)."),
            "compile_hidden": boolean("Compile hidden objects too."),
            "save_before_compile": boolean("Save the map before compiling."),
            "separate_window": boolean("The Compile and run window is a window of its own (it can leave Hammer Pro, \
                go to another screen); false: inside the main window."),
            "load_pointfile_on_leak": boolean("Load the leak pointfile when a compile leaks."),
            "background_precompile": boolean("Compile in the background while the editor is idle."),
            "progressive": boolean("Play now: Draft compile and launch, better lighting finished in the background."),
            "progressive_preset": choice("Quality of the background lighting of progressive play.", &["normal", "final"]),
            "history": boolean("Keep a timing history per map."),
            "pack_custom_content": boolean("Pack the custom content into the BSP."),
            "pack_policy": choice("When packing applies: release_only (Final compiles and releases) or always.", &["release_only", "always"]),
            "dev_addon": boolean("Iteration builds copy the custom content to garrysmod/addons/hammerpro_dev instead of packing it."),
            "keep_cubemaps": boolean("Keep the cubemaps built in game when the copy replaces the map in the maps folder."),
            "expert": boolean("Expert mode: run the active profile instead of the preset."),
            "profile": string("Expert profile to make active (turns expert mode on)."),
            "copy_to_maps": boolean("Copy the BSP to the game's maps folder after a compile."),
            "maps_dir_override": string("Folder the BSP is copied to: an existing absolute folder, or \"\" for garrysmod/maps. Needs confirm."),
            "launch_game": boolean("Start the game after a compile (true needs confirm)."),
            "launch_via_steam": boolean("Start the game through Steam instead of its executable."),
            "bsp_options": flag_options("Options menu of the geometry and visibility step: flag name without the dash \
                to true (switch), a number, [cluster, cluster] for trace, a folder of the map folder for embed to \
                force it on; false forces it off (even when the preset adds it); null follows the preset again. The \
                named flags leave the extra arguments; the other extra arguments are kept."),
            "rad_options": flag_options("Options menu of the lighting step, like bsp_options (lights: a .rad file of \
                the map folder). nostaticpropshadows is static_prop_shadows."),
            "confirm": boolean("The user asked for this (needed to start the game after compiles or to set the maps folder)."),
        }, [];
    cancel_compile: Editor, "Cancel the compile",
        "Stop the running compile.",
        {}, [];
    get_compile_log: Read, "Compile log",
        "The steps and log of the last compile (running or finished): last lines, only warnings and errors, or \
        lines containing a text; counts, leak pointfile and result. Lines reporting a known problem (leak, \
        microbrush, too many planes, missing material... the Valve messages) carry its code, explanation, fix and \
        the brush, side, entity or position they name; problems sums them up. While Hammer Pro's lighting runs, \
        its step carries light: the stage (scene, direct lighting, bounce rays, bounced light, \
        leaf ambient, detail props, static props, writing), stage_percent, stage_index of stage_count, \
        overall_percent, elapsed_s and left_s (estimated seconds left, null while estimating); the log shows these \
        progress lines and a warning when a stage stops advancing. With from_file, the <map>.log the compiler \
        writes next to the map is read instead (compiles of other windows or of the command line too).",
        {
            "tail": integer("Last lines returned (default 200).", 1, 5000),
            "problems_only": boolean("Only warnings and errors."),
            "contains": string("Only lines containing this text."),
            "from_file": boolean("Read <map>.log next to the map instead of the compile of this window."),
        }, [];
    get_compile_stats: Read, "Compile statistics",
        "The statistics of the last compile, read from its log, to optimize the map: the engine limits like \
        vbspinfo (models, brushes, brushsides, planes, vertexes, nodes, texinfo, texdata, displacements, faces, \
        leaves, areas, areaportals, edges, worldlights, lightmap bytes, visdata, entities, entdata, static and \
        detail props, overlays, cubemaps, pakfile: used, maximum, bytes, percent), the brushes by kind \
        (structural, detail, water, tool, in brush entities), entities and faces by class and material, \
        displacements, the areas, the small structural brushes that could be func_detail, the static props \
        without fade distance, the visibility (clusters, portals, visible clusters per cluster, radius, pass \
        times, the clusters that see the most with their place), the lighting (lightmap bytes per pass of the \
        2 GB a face can address, luxels, lights by type, switchable styles, props, bounces, ambient samples, the \
        faces and materials with the most luxels with lightmap scale and place), the time of each stage, and \
        optimization hints in plain words with the brush, side or place they are about. With from_file, \
        <map>.log next to the map is read (its last compile).",
        {
            "from_file": boolean("Read <map>.log next to the map instead of the compile of this window."),
        }, [];
    get_compile_history: Read, "Compile history",
        "The past compiles of the map: date, preset, step times, total time, success, leak, portal counts.",
        { "limit": limit() }, [];
    get_compilers: Read, "Compiler",
        "Hammer Pro's own compiler, the default one (geometry, portals, leak test and visibility with \
        hammerpro --bspc, lighting with hammerpro --rad, packing built in): version, executable, the flags of \
        each step with what they do, and the content arguments the editor adds to both steps so they mount the \
        content it shows (content_args: -wsaddons, -serveraddons, -insert_search_path). Also the compiler chosen \
        for the game (active_compiler): other compilers (Valve tools found next to the game, the user's own \
        profiles) are listed by list_compilers and chosen with select_compiler.",
        {}, [];
    check_leaks: Editor, "Check for leaks",
        "Run the instant leak check (no compile): the result comes in the background, read it with get_leak.",
        {}, [];
    get_leak: Read, "Leak",
        "The result of the instant leak check (leak path from an entity to the void, leaking entities, entities in \
        solids) and the pointfile of the last compile (load_pointfile).",
        { "points": integer("Most path points returned (default 200).", 1, 10_000) }, [];
    load_pointfile: Editor, "Load the pointfile",
        "Load (or with unload: true, remove) the leak pointfile of the last compile, drawn in the views.",
        { "unload": boolean("Remove it instead.") }, [];
    portal_file: Editor, "Portal file",
        "Load the .prt portal file next to the map (or a name in the map folder), build one (build: true), or \
        unload it; answers the clusters and portals loaded.",
        {
            "action": choice("What to do (default info).", &["info", "load", "build", "unload"]),
            "file": string("Portal file in the map folder (default <map>.prt)."),
        }, [];
    bake_lighting: Editor, "Bake the lighting preview",
        "Bake the lighting preview (luxels on the CPU, like VRAD), around the camera only (around_camera), or stop \
        (cancel) the bake, with a quality: draft (fast), normal or final (like the compile presets; saved as the \
        preview quality). A bake shows a quick pass first (the Sun Samples, Ambient Samples and Supersample of the \
        map's light_plus options), then a refined pass at the quality (kind refine in get_lighting_status) replaces \
        it unless the map changed. Read the progress with get_lighting_status; set_render_mode baked shows the result.",
        {
            "action": choice("start (default), around_camera or stop.", &["start", "around_camera", "stop"]),
            "quality": choice("Bake quality (default: the saved one).", &["draft", "normal", "final"]),
        }, [];
    set_lighting_options: Editor, "Change the lighting preview options",
        "Change the lighting preview options saved in the map (the Hammer++ light_plus block, shown by \
        get_lighting_status in options): brightness (0 to 4), bounced light, dynamic updates (bake again once edits \
        stop) and their delay (0 to 60 s), the radius of a bake around the camera (64 to 65536), and the sampling of \
        the quick pass every bake shows first: sun_samples (1 to 256), ambient_samples (sky light directions, 16 to \
        5184) and supersample. The refined pass that follows uses the bake quality. Answers the options.",
        {
            "brightness": number("Brightness of the preview.", 0.0, 4.0),
            "bounced": boolean("Bounced light."),
            "dynamic_updates": boolean("Bake again once edits stop."),
            "update_delay": number("Seconds without edits before the map is baked again.", 0.0, 60.0),
            "bake_radius": number("Radius of a bake around the camera, in units.", 64.0, 65536.0),
            "sun_samples": integer("Sun samples of the quick pass (Hammer++ Sun Samples).", 1, 256),
            "ambient_samples": integer("Sky light directions of the quick pass (Hammer++ Ambient Samples).", 16, 5184),
            "supersample": boolean("Supersample the shadow edges in the quick pass (Hammer++ Supersample)."),
        }, [];
    get_lighting_status: Read, "Lighting preview status",
        "The lighting bake: options (the map's lighting preview options, see set_lighting_options), quality, progress, last result (time, faces, luxels, density), preview statistics (with \
        what lights the props: the world lights, the static props and those with baked vertex lighting), tone \
        (the HDR tone mapping of the focused 3D view: mode, the exposure scale now, its range, the bloom amount, \
        what the env_tonemap_controller of the map sets and the luminance histogram the auto exposure reads), the \
        lightmap budget, and while the lit mode is on the realtime preview: lights drawn, suns, lights left out, \
        crowded spots keeping only their strongest lights, HDR or LDR values, sky light occlusion, projectors (the env_projectedtexture entities drawn, setting render.projected_textures), sky_layers (how \
        many skies lie one above the other toward the sun and straight up: each point is shadowed only by what lies \
        between it and the sky it reaches, like VRAD), prop_shadows (what static props cast shadows with: mesh, \
        collision or none, from the setting render.prop_shadows: auto follows the compile options, \
        -StaticPropPolys for mesh), and the sun (direction it shines along, color, _ambient sky color, \
        SunSpreadAngle, its light_environment) and indirect (the background bake that gives the Lit mode its sky light and bounced light: the setting render.lit_indirect, state off, no_lights, waiting, baking, ready, stale or failed, the probe cells, their spacing, the GPU bytes and the bake time).",
        {}, [];
    launch_game: Editor, "Start the game",
        "Start Garry's Mod on the compiled map, or reload it in the running game (reload: true). Only when the user \
        asked for it: set confirm to true. Refused in sandboxed (automated) runs.",
        { "confirm": boolean("The user asked to start the game."), "reload": boolean("Reload the map in the running game.") }, ["confirm"];
}

fn flag_options(description: &str) -> Value {
    json!({ "type": "object", "description": description })
}

fn preset_name(p: CompilePresetSetting) -> &'static str {
    match p {
        CompilePresetSetting::Draft => "draft",
        CompilePresetSetting::Normal => "normal",
        CompilePresetSetting::Final => "final",
        CompilePresetSetting::Custom => "custom",
    }
}

fn preset_of(name: &str) -> CompilePresetSetting {
    match name {
        "draft" => CompilePresetSetting::Draft,
        "normal" => CompilePresetSetting::Normal,
        "final" => CompilePresetSetting::Final,
        _ => CompilePresetSetting::Custom,
    }
}

const MAX_STEP_ARGS: usize = 4096;

fn flag_number(x: f64) -> String {
    if x.fract() == 0.0 && x.abs() < 1e15 { format!("{}", x as i64) } else { format!("{x}") }
}

fn option_value(v: &str) -> Value {
    v.parse::<f64>().ok().filter(|x| x.is_finite()).map_or_else(|| json!(v), |x| json!(x))
}

fn option_of(info: &FlagInfo, values: &[String]) -> Value {
    match (info.value_count(), values) {
        (0, _) | (_, []) => json!(true),
        (1, _) => option_value(&values.join(" ")),
        (_, many) => Value::Array(many.iter().map(|v| option_value(v)).collect()),
    }
}

fn options_json(s: &CompileSettings, step: StepKind) -> Value {
    let (choices, extra) = match step {
        StepKind::Bsp => (&s.bsp_flag_choices, &s.extra_bsp_args),
        _ => (&s.rad_flag_choices, &s.extra_rad_args),
    };
    let name = |f: &str| f.trim_start_matches('-').to_owned();
    let mut forced = serde_json::Map::new();
    for (flag, c) in choices {
        let Some(info) = flag_info(step, flag) else { continue };
        let v = if !c.on {
            json!(false)
        } else if info.value_count() == 0 {
            json!(true)
        } else {
            let words: Vec<String> = if info.value_count() == 1 {
                vec![c.value.trim().to_owned()]
            } else {
                c.value.split_whitespace().map(str::to_owned).collect()
            };
            option_of(info, &words)
        };
        forced.insert(name(info.flag), v);
    }
    let mut options = forced.clone();
    let mut other = Vec::new();
    for g in flags::arg_groups(&hp_compile::split_args(extra), step) {
        let Some(info) = flag_info(step, &g[0]) else {
            other.extend(g);
            continue;
        };
        options.entry(name(info.flag)).or_insert_with(|| option_of(info, &g[1..]));
    }
    let (preset, merged) = hp_ui_mapping::effective_step_flags(s, step);
    json!({
        "options": options,
        "forced": forced,
        "other_arguments": other,
        "preset_flags": preset.join(" "),
        "command_flags": merged.iter().map(|a| hp_compile::quote_arg(a)).collect::<Vec<_>>().join(" "),
    })
}

fn relative_folder(path: &str) -> R<String> {
    let p = path.trim().trim_end_matches(['/', '\\']).replace('\\', "/");
    let bad = |why: &str| Err(format!("invalid folder \"{path}\": {why}"));
    if p.is_empty() || p.len() > 255 {
        return bad("1 to 255 characters");
    }
    if p.starts_with('/') || p.contains(':') {
        return bad("it must be relative to the map folder (no drive, no leading slash)");
    }
    if p.chars().any(|c| c.is_control() || matches!(c, '"' | '<' | '>' | '|' | '?' | '*')) {
        return bad("forbidden character");
    }
    if p.split('/').any(|c| c.is_empty() || c == "." || c == ".." || c.ends_with(' ') || c.ends_with('.')) {
        return bad("empty, \".\" or \"..\" parts are not allowed");
    }
    Ok(p)
}

fn flag_values(info: &FlagInfo, v: &Value, map_dir: Option<&Path>, key: &str) -> R<Vec<String>> {
    let name = info.flag;
    let lower = name.to_ascii_lowercase();
    let dir = || map_dir.ok_or_else(|| format!("{key}: {name} names a file of the map folder: save the map first"));
    match (info.value_count(), v) {
        (0, Value::Bool(true)) => Ok(Vec::new()),
        (0, _) => Err(format!("{key}: {name} is a switch: true, false to force it off, null to follow the preset")),
        (_, Value::String(s)) if lower == "-embed" => {
            let rel = relative_folder(s)?;
            let path = dir()?.join(&rel);
            if !path.is_dir() {
                return Err(format!("{key}: no folder {rel} in the map folder"));
            }
            Ok(vec![path.display().to_string()])
        }
        (_, Value::String(s)) if lower == "-lights" => {
            let rel = relative_file(s, "rad")?;
            let path = dir()?.join(&rel);
            if !path.is_file() {
                return Err(format!("{key}: no file {rel} in the map folder"));
            }
            Ok(vec![path.display().to_string()])
        }
        (_, _) if lower == "-embed" || lower == "-lights" => {
            Err(format!("{key}: {name} takes a path relative to the map folder"))
        }
        (2, Value::Array(list)) if list.len() == 2 => list
            .iter()
            .map(|x| {
                x.as_u64()
                    .filter(|n| *n <= 1_000_000)
                    .map(|n| n.to_string())
                    .ok_or_else(|| format!("{key}: {name} takes two cluster numbers"))
            })
            .collect(),
        (2, _) => Err(format!("{key}: {name} takes [cluster, cluster]")),
        (_, Value::Number(n)) if lower == "-blocksize" => match n.as_f64().and_then(hp_bspc::compile::block_size_of) {
            Some(size) => Ok(vec![size.to_string()]),
            None => Err(format!(
                "{key}: {name} takes 0 (no blocks) or a size from {} to {}",
                hp_bspc::compile::MIN_BLOCK_SIZE,
                hp_bspc::compile::MAX_BLOCK_SIZE
            )),
        },
        (_, Value::Number(n)) => match n.as_f64().filter(|x| x.is_finite() && (0.0..=1.0e6).contains(x)) {
            Some(x) => Ok(vec![flag_number(x)]),
            None => Err(format!("{key}: {name} takes a number from 0 to 1000000")),
        },
        _ => Err(format!("{key}: {name} takes a number (false forces it off, null follows the preset)")),
    }
}

fn apply_flag_options(
    s: &mut CompileSettings,
    step: StepKind,
    opts: &Value,
    map_dir: Option<&Path>,
    key: &str,
) -> R<()> {
    let map = opts.as_object().ok_or_else(|| format!("{key} must be an object: flag names to values"))?;
    if map.len() > 64 {
        return Err(format!("{key}: at most 64 options"));
    }
    let step_name = if step == StepKind::Bsp { "geometry" } else { "lighting" };
    let (choices, extra) = match step {
        StepKind::Bsp => (&mut s.bsp_flag_choices, &mut s.extra_bsp_args),
        _ => (&mut s.rad_flag_choices, &mut s.extra_rad_args),
    };
    let mut groups = flags::arg_groups(&hp_compile::split_args(extra), step);
    for (name, v) in map {
        let flag = format!("-{}", name.trim().trim_start_matches('-'));
        let info = flag_info(step, &flag).ok_or_else(|| {
            format!("{key}: the {step_name} step has no option \"{name}\" (get_compilers lists them)")
        })?;
        if let Some(by) = info.managed_by() {
            return Err(format!("{key}: Hammer Pro writes {} itself: set {} instead", info.flag, by.option()));
        }
        let choice = match v {
            Value::Null => None,
            Value::Bool(false) => Some(FlagChoiceSetting { on: false, value: String::new() }),
            _ => Some(FlagChoiceSetting { on: true, value: flag_values(info, v, map_dir, key)?.join(" ") }),
        };
        groups.retain(|g| !g[0].eq_ignore_ascii_case(info.flag));
        choices.retain(|k, _| !k.eq_ignore_ascii_case(info.flag));
        if let Some(c) = choice {
            choices.insert(info.flag.to_owned(), c);
        }
    }
    let out = groups.iter().flatten().map(|t| hp_compile::quote_arg(t)).collect::<Vec<_>>().join(" ");
    let values: usize = choices.values().map(|c| c.value.len() + 32).sum();
    if out.len() + values > MAX_STEP_ARGS {
        return Err(format!("{key}: the arguments would be too long (at most {MAX_STEP_ARGS} characters)"));
    }
    *extra = out;
    Ok(())
}

fn compile_options_json(state: &AppState) -> Value {
    let s = &state.settings.compile;
    let lower = |v: String| v.to_ascii_lowercase();
    let mut out = json!({
        "preset": preset_name(s.preset),
        "presets": ["draft", "normal", "final", "custom"],
        "run_bsp": s.run_bsp, "run_vis": s.run_vis, "run_rad": s.run_rad,
        "bsp_mode": lower(format!("{:?}", s.bsp_mode)),
        "vis_mode": lower(format!("{:?}", s.vis_mode)),
        "rad_mode": lower(format!("{:?}", s.rad_mode)),
        "lighting": lower(format!("{:?}", s.lighting)),
        "static_prop_lighting": s.static_prop_lighting,
        "static_prop_shadows": s.static_prop_shadows,
        "bsp_options": options_json(s, StepKind::Bsp),
        "rad_options": options_json(s, StepKind::Rad),
        "extra_bsp_args": s.extra_bsp_args,
        "extra_rad_args": s.extra_rad_args,
    });
    let rest = json!({
        "cordon_only": s.compile_cordon_only,
        "cordon_material": state.settings.visibility.cordon_material,
        "smart": s.smart_recompile,
        "background_precompile": s.background_precompile,
        "compile_hidden": s.compile_hidden,
        "threads": s.threads,
        "low_priority": s.low_priority,
        "stop_on_leak": s.stop_on_leak,
        "load_pointfile_on_leak": s.load_pointfile_on_leak,
        "save_before_compile": s.save_before_compile,
        "separate_window": s.separate_window,
        "pack_custom_content": s.pack_custom_content,
        "pack_policy": match s.pack_policy { PackPolicy::ReleaseOnly => "release_only", PackPolicy::Always => "always" },
        "dev_addon": s.dev_addon,
        "keep_cubemaps": state.settings.content.keep_cubemaps,
        "copy_to_maps": s.copy_to_maps,
        "maps_dir_override": s.maps_dir_override.as_ref().map(|p| p.display().to_string()),
        "launch_game": s.launch_game,
        "launch_via_steam": s.launch_via_steam,
        "game_args": s.game_args,
        "game_exe": s.game_exe_path.as_ref().map(|p| p.display().to_string()),
        "progressive": s.progressive,
        "progressive_preset": preset_name(s.progressive_preset),
        "history": s.history,
        "expert": s.expert,
        "compiler": s.active_compiler_id(),
        "profiles": s.profiles.iter().enumerate().map(|(i, p)| json!({
            "index": i,
            "name": p.name,
            "active": i == s.active_profile,
            "steps": p.steps.iter().map(|st| json!({ "enabled": st.enabled, "action": st.action, "program": st.program, "args": st.args })).collect::<Vec<_>>(),
        })).collect::<Vec<_>>(),
        "status": compile_json(state),
    });
    if let (Value::Object(m), Value::Object(r)) = (&mut out, rest) {
        m.extend(r);
    }
    out
}

fn get_compile_options(state: &mut AppState, _a: &Args<'_>) -> R<ToolResult> {
    Ok(ToolResult::json(compile_options_json(state)))
}

fn apply_options(state: &AppState, s: &mut CompileSettings, a: &Args<'_>, like_window: bool) -> R<()> {
    let preset = a.opt_enum("preset", &["draft", "normal", "final", "custom"])?.map(|p| preset_of(&p));
    if let Some(p) = preset {
        if like_window {
            hp_ui_mapping::CompileDialog::mcp_select_preset(s, p);
        } else {
            s.preset = p;
        }
    }
    let any_step =
        ["run_bsp", "run_vis", "run_rad", "bsp_mode", "vis_mode", "rad_mode"].iter().any(|k| a.get(k).is_some());
    if any_step && preset.is_none() && !like_window {
        s.preset = CompilePresetSetting::Custom;
    }
    if let Some(v) = a.opt_bool("run_bsp")? {
        s.run_bsp = v;
    }
    if let Some(v) = a.opt_bool("run_vis")? {
        s.run_vis = v;
    }
    if let Some(v) = a.opt_bool("run_rad")? {
        s.run_rad = v;
    }
    if let Some(m) = a.opt_enum("bsp_mode", &["normal", "onlyents", "onlyprops"])? {
        s.bsp_mode = match m.as_str() {
            "onlyents" => BspModeSetting::OnlyEnts,
            "onlyprops" => BspModeSetting::OnlyProps,
            _ => BspModeSetting::Normal,
        };
    }
    if let Some(m) = a.opt_enum("vis_mode", &["fast", "normal"])? {
        s.vis_mode = if m == "fast" { VisModeSetting::Fast } else { VisModeSetting::Normal };
    }
    if let Some(m) = a.opt_enum("rad_mode", &["draft", "normal", "final"])? {
        s.rad_mode = match m.as_str() {
            "draft" => RadModeSetting::Draft,
            "final" => RadModeSetting::Final,
            _ => RadModeSetting::Normal,
        };
    }
    if any_step && like_window {
        hp_ui_mapping::CompileDialog::mcp_settle_preset(s);
    }
    if let Some(l) = a.opt_enum("lighting", &["ldr", "hdr", "both"])? {
        s.lighting = match l.as_str() {
            "ldr" => LightingSetting::Ldr,
            "hdr" => LightingSetting::Hdr,
            _ => LightingSetting::Both,
        };
    }
    if let Some(v) = a.opt_bool("static_prop_lighting")? {
        s.static_prop_lighting = v;
    }
    if let Some(v) = a.opt_bool("static_prop_shadows")? {
        s.static_prop_shadows = v;
    }
    if let Some(v) = a.opt_bool("cordon_only")? {
        s.compile_cordon_only = v;
    }
    if let Some(v) = a.opt_bool("smart")? {
        s.smart_recompile = v;
    }
    if let Some(v) = a.opt_bool("compile_hidden")? {
        s.compile_hidden = v;
    }
    if let Some(t) = a.opt_u64("threads", 0, 256)? {
        s.threads = (t > 0).then_some(t as u32);
    }
    if let Some(name) = a.opt_str("profile", 128)? {
        let i = s
            .profiles
            .iter()
            .position(|p| p.name.eq_ignore_ascii_case(name.trim()))
            .ok_or_else(|| format!("no expert profile is named \"{name}\" (get_compile_options)"))?;
        s.expert = true;
        s.active_profile = i;
    }
    let dir = state.doc.path().and_then(Path::parent);
    if let Some(o) = a.get("bsp_options") {
        apply_flag_options(s, StepKind::Bsp, o, dir, "bsp_options")?;
    }
    if let Some(o) = a.get("rad_options") {
        apply_flag_options(s, StepKind::Rad, o, dir, "rad_options")?;
    }
    if let Some(v) = a.opt_bool("copy_to_maps")? {
        s.copy_to_maps = v;
    }
    Ok(())
}

fn options_settings(state: &AppState, a: &Args<'_>) -> R<CompileSettings> {
    let mut s = state.settings.compile.clone();
    apply_options(state, &mut s, a, false)?;
    if let Some(c) = a.opt_str("compiler", 128)? {
        s.set_active_compiler(&super::tools_compilers::compiler_arg(state, c)?);
    }
    if a.opt_bool("cordon_only")? == Some(true) && !state.doc.cordons_in_use() {
        return Err("cordon_only needs an active cordon (add_cordon)".to_owned());
    }
    if !s.expert && !s.run_bsp && !s.run_vis && !s.run_rad {
        return Err("at least one step must run".to_owned());
    }
    s.launch_game = false;
    Ok(s)
}

fn set_compile_options(state: &mut AppState, a: &Args<'_>) -> R<ToolResult> {
    if a.map.is_empty() {
        return Err("give at least one setting (get_compile_options lists them)".to_owned());
    }
    let confirmed = a.opt_bool("confirm")? == Some(true);
    let mut s = state.settings.compile.clone();
    apply_options(state, &mut s, a, true)?;
    for (key, slot) in [
        ("low_priority", &mut s.low_priority),
        ("stop_on_leak", &mut s.stop_on_leak),
        ("save_before_compile", &mut s.save_before_compile),
        ("separate_window", &mut s.separate_window),
        ("load_pointfile_on_leak", &mut s.load_pointfile_on_leak),
        ("background_precompile", &mut s.background_precompile),
        ("progressive", &mut s.progressive),
        ("history", &mut s.history),
        ("pack_custom_content", &mut s.pack_custom_content),
        ("dev_addon", &mut s.dev_addon),
        ("launch_via_steam", &mut s.launch_via_steam),
    ] {
        if let Some(v) = a.opt_bool(key)? {
            *slot = v;
        }
    }
    if let Some(p) = a.opt_enum("progressive_preset", &["normal", "final"])? {
        s.progressive_preset = preset_of(&p);
    }
    if let Some(p) = a.opt_enum("pack_policy", &["release_only", "always"])? {
        s.pack_policy = if p == "always" { PackPolicy::Always } else { PackPolicy::ReleaseOnly };
    }
    if let Some(v) = a.opt_bool("expert")? {
        if v && s.profiles.is_empty() {
            return Err("there is no expert profile (the compile window makes them)".to_owned());
        }
        s.expert = v;
    }
    if let Some(v) = a.opt_bool("launch_game")? {
        if v && !confirmed {
            return Err("starting the game after every compile: only when the user asked (confirm: true)".to_owned());
        }
        if v && hp_compile::sandbox::is_sandboxed() {
            return Err(format!("refused in this automated run: {}", hp_compile::sandbox::REFUSED));
        }
        s.launch_game = v;
    }
    if let Some(dir) = a.opt_str("maps_dir_override", 1024)? {
        let dir = dir.trim();
        if dir.is_empty() {
            s.maps_dir_override = None;
        } else {
            if !confirmed {
                return Err(
                    "the BSP would be copied to another folder: only when the user asked (confirm: true)".to_owned()
                );
            }
            let path = std::path::PathBuf::from(dir);
            if !path.is_absolute() || !path.is_dir() {
                return Err(format!("\"{dir}\" is not an existing absolute folder"));
            }
            s.maps_dir_override = Some(path);
        }
    }
    if !s.expert && !s.run_bsp && !s.run_vis && !s.run_rad {
        return Err("at least one step must run".to_owned());
    }
    if let Some(v) = a.opt_bool("keep_cubemaps")? {
        state.settings.content.keep_cubemaps = v;
    }
    state.settings.compile = s;
    state.settings_dirty = true;
    Ok(ToolResult::json(compile_options_json(state)))
}

fn compile_with_options(state: &mut AppState, a: &Args<'_>) -> R<Outcome> {
    if state.doc.path().is_none() {
        return Err("save the map first (save_map with a name)".to_owned());
    }
    if state.runtime.compile.progress.is_some() {
        return Err("a compile is already running (get_compile_status, cancel_compile)".to_owned());
    }
    let wanted = options_settings(state, a)?;
    Ok(Outcome::AppFn(AppFn(Box::new(move |app, ctx| {
        if app.compile.is_running() {
            return ToolResult::error("compile_with_options: a compile is already running");
        }
        app.restore_compile_settings();
        let old = std::mem::replace(&mut app.state.settings.compile, wanted.clone());
        app.compile.start_now(ctx, &mut app.state);
        let started = app.compile.is_running();
        let mut back = old.clone();
        back.preset = wanted.preset;
        back.launch_game = false;
        app.state.settings.compile = back;
        app.pkg.mcp.compile_restore =
            Some(super::CompileRestore { preset: (old.preset, wanted.preset), launch: (old.launch_game, false) });
        if !started {
            app.restore_compile_settings();
            return ToolResult::error("compile_with_options: the compile did not start (see get_compile_log)");
        }
        ToolResult::json(json!({
            "started": true,
            "preset": preset_name(wanted.preset),
            "steps": { "bsp": wanted.run_bsp, "vis": wanted.run_vis, "rad": wanted.run_rad },
            "expert_profile": wanted.expert.then(|| wanted.profiles.get(wanted.active_profile).map(|p| p.name.clone())).flatten(),
            "compiler": wanted.active_compiler_id(),
            "note": "Poll get_compile_status until running is false, then read get_compile_log.",
        }))
    }))))
}

fn cancel_compile(_state: &mut AppState, _a: &Args<'_>) -> R<Outcome> {
    Ok(Outcome::AppFn(AppFn(Box::new(|app, _ctx| {
        if !app.compile.is_running() {
            return ToolResult::json(json!({ "cancelled": false, "note": "no compile is running" }));
        }
        app.compile.cancel_compile(&mut app.state);
        ToolResult::json(json!({ "cancelled": true }))
    }))))
}

fn fix_json(f: Fix) -> Value {
    match f {
        Fix::GoToLocation => json!({ "fix": "go_to_location", "tools": ["select_objects", "frame_objects"] }),
        Fix::OpenProblems => json!({ "fix": "problems", "tools": ["check_problems", "fix_problems"] }),
        Fix::OpenDocs(url) => json!({ "fix": "docs", "url": url }),
        Fix::LoadLeak => json!({ "fix": "load_leak", "tools": ["load_pointfile", "get_leak"] }),
    }
}

fn knowledge_json(k: &Knowledge) -> Value {
    json!({ "code": k.code, "explanation": k.english, "fix": fix_json(k.fix) })
}

fn location_json(l: &LogLocation) -> Value {
    json!({
        "brush": l.brush_id, "side": l.side_id, "entity": l.entity_id,
        "entity_class": l.entity_class, "position": l.position,
    })
}

fn line_json(
    severity: &str,
    step: Option<usize>,
    text: &str,
    loc: Option<&LogLocation>,
    k: Option<&Knowledge>,
) -> Value {
    let mut v = json!({ "severity": severity, "step": step, "text": text });
    if let Some(l) = loc.filter(|l| !l.is_empty()) {
        v["location"] = location_json(l);
    }
    if let Some(k) = k {
        v["problem"] = knowledge_json(k);
    }
    v
}

fn problems_summary<'a>(known: impl Iterator<Item = &'a Knowledge>) -> Vec<Value> {
    let mut seen: Vec<(&Knowledge, usize)> = Vec::new();
    for k in known {
        match seen.iter_mut().find(|(s, _)| s.code == k.code) {
            Some((_, n)) => *n += 1,
            None => seen.push((k, 1)),
        }
    }
    seen.into_iter()
        .map(|(k, n)| {
            let mut v = knowledge_json(k);
            v["lines"] = json!(n);
            v
        })
        .collect()
}

fn log_files(state: &AppState) -> Vec<PathBuf> {
    let Some(map) = state.doc.path() else { return Vec::new() };
    let (Some(dir), Some(stem)) = (map.parent(), map.file_stem()) else { return Vec::new() };
    let stem = stem.to_string_lossy();
    let build = dir.join(".hammerpro_build");
    vec![dir.join(format!("{stem}.log")), build.join(format!("{stem}.log")), build.join(format!("{stem}_cordon.log"))]
}

const MAX_LOG_READ: u64 = 16 * 1024 * 1024;

fn read_log_file(files: &[PathBuf], tail: usize, problems: bool, contains: Option<&str>) -> ToolResult {
    let newest = files
        .iter()
        .filter_map(|p| std::fs::metadata(p).ok().filter(std::fs::Metadata::is_file).map(|m| (p, m)))
        .max_by_key(|(_, m)| m.modified().ok());
    let Some((path, meta)) = newest else {
        return ToolResult::json(json!({
            "from_file": true,
            "found": false,
            "looked_in": files.iter().map(|p| p.display().to_string()).collect::<Vec<_>>(),
            "note": "no <map>.log: compile the map first",
        }));
    };
    let bytes = {
        use std::io::{Read as _, Seek as _};
        let mut out = Vec::new();
        let read = std::fs::File::open(path).and_then(|mut f| {
            f.seek(std::io::SeekFrom::Start(meta.len().saturating_sub(MAX_LOG_READ)))?;
            f.take(MAX_LOG_READ).read_to_end(&mut out)
        });
        if let Err(e) = read {
            return ToolResult::error(format!("get_compile_log: cannot read {}: {e}", path.display()));
        }
        out
    };
    let text = String::from_utf8_lossy(&bytes);
    let needle = contains.map(str::to_ascii_lowercase);
    let lines: Vec<(&'static str, &str, Option<LogLocation>, Option<&'static Knowledge>)> = text
        .lines()
        .map(|l| l.trim_end_matches('\r'))
        .filter(|l| !l.trim().is_empty())
        .map(|l| {
            let (sev, loc) = hp_compile::parse_log_line(l);
            (hp_ui_mapping::CompileDialog::mcp_severity_name(sev), l, loc, hp_compile::knowledge::explain(l))
        })
        .collect();
    let warnings = lines.iter().filter(|l| l.0 == "warning").count();
    let errors = lines.iter().filter(|l| l.0 == "error").count();
    let summary = problems_summary(lines.iter().filter_map(|l| l.3));
    let kept: Vec<&(&'static str, &str, Option<LogLocation>, Option<&'static Knowledge>)> = lines
        .iter()
        .filter(|l| !problems || l.0 == "warning" || l.0 == "error")
        .filter(|l| needle.as_deref().is_none_or(|n| l.1.to_ascii_lowercase().contains(n)))
        .collect();
    let skip = kept.len().saturating_sub(tail);
    ToolResult::json(json!({
        "from_file": true,
        "found": true,
        "file": path.display().to_string(),
        "bytes": meta.len(),
        "note": "The compiler appends to this file: it can hold several compiles, the last one at the end.",
        "warnings": warnings,
        "errors": errors,
        "problems": summary,
        "total_lines": lines.len(),
        "lines": kept.iter().skip(skip).map(|l| line_json(l.0, None, l.1, l.2.as_ref(), l.3)).collect::<Vec<_>>(),
    }))
}

fn get_compile_log(state: &mut AppState, a: &Args<'_>) -> R<Outcome> {
    let tail = a.opt_u64("tail", 1, 5000)?.unwrap_or(200) as usize;
    let problems = a.opt_bool("problems_only")?.unwrap_or(false);
    let contains = a.opt_str("contains", 256)?.map(str::to_owned);
    if a.opt_bool("from_file")? == Some(true) {
        if state.doc.path().is_none() {
            return Err("the map has no file: save and compile it first".to_owned());
        }
        let files = log_files(state);
        return Ok(Worker::spawn("get_compile_log", move || {
            read_log_file(&files, tail, problems, contains.as_deref())
        }));
    }
    Ok(Outcome::AppFn(AppFn(Box::new(move |app, _ctx| {
        let Some(v) = app.compile.mcp_view(tail, problems, contains.as_deref()) else {
            return ToolResult::json(json!({
                "compiled": false,
                "note": "no compile ran in this window yet (from_file: true reads <map>.log)",
            }));
        };
        let summary = app
            .compile
            .mcp_view(usize::MAX, false, None)
            .map(|all| problems_summary(all.lines.iter().filter_map(|l| l.knowledge)))
            .unwrap_or_default();
        ToolResult::json(json!({
            "compiled": true,
            "map": v.map,
            "running": v.running,
            "steps": v.steps.iter().map(|s| json!({
                "step": s.kind, "status": s.status, "fraction": s.fraction, "phase": s.phase,
                "light": s.phase.as_deref().and_then(hp_compile::parse_light_progress).map(|p| json!({
                    "stage": p.stage, "stage_percent": (p.stage_fraction * 100.0).round(),
                    "stage_index": p.index, "stage_count": p.count, "overall_percent": (p.overall * 100.0).round(),
                    "elapsed_s": p.elapsed_s, "left_s": p.left_s,
                })),
                "duration_ms": s.duration_ms, "exit_code": s.exit_code,
            })).collect::<Vec<_>>(),
            "warnings": v.warnings,
            "errors": v.errors,
            "leak_pointfile": v.leak_file.map(|p| p.display().to_string()),
            "result": v.result.map(|(ok, cancelled, leaked, ms)| json!({ "success": ok, "cancelled": cancelled, "leaked": leaked, "total_ms": ms })),
            "problems": summary,
            "total_lines": v.total_lines,
            "lines": v.lines.iter().map(|l| line_json(l.severity, l.step, &l.text, l.location.as_ref(), l.knowledge)).collect::<Vec<_>>(),
        }))
    }))))
}

fn stats_json(stats: &hp_compile::stats::CompileStats, hints: &[hp_compile::stats::Hint]) -> Value {
    let hints: Vec<Value> = hints
        .iter()
        .map(|h| {
            let mut v = serde_json::to_value(h).unwrap_or(Value::Null);
            v["text"] = json!(h.english());
            v["location"] = h.location.as_ref().map_or(Value::Null, location_json);
            v
        })
        .collect();
    json!({
        "found": !stats.is_empty(),
        "stats": serde_json::to_value(stats).unwrap_or(Value::Null),
        "hints": hints,
    })
}

fn get_compile_stats(state: &mut AppState, a: &Args<'_>) -> R<Outcome> {
    if a.opt_bool("from_file")? == Some(true) {
        if state.doc.path().is_none() {
            return Err("the map has no file: save and compile it first".to_owned());
        }
        let files = log_files(state);
        return Ok(Worker::spawn("get_compile_stats", move || {
            let newest = files
                .iter()
                .filter_map(|p| std::fs::metadata(p).ok().filter(std::fs::Metadata::is_file).map(|m| (p, m)))
                .max_by_key(|(_, m)| m.modified().ok());
            let Some((path, meta)) = newest else {
                return ToolResult::json(
                    json!({ "from_file": true, "found": false, "note": "no <map>.log: compile the map first" }),
                );
            };
            let bytes = {
                use std::io::{Read as _, Seek as _};
                let mut out = Vec::new();
                let read = std::fs::File::open(path).and_then(|mut f| {
                    f.seek(std::io::SeekFrom::Start(meta.len().saturating_sub(MAX_LOG_READ)))?;
                    f.take(MAX_LOG_READ).read_to_end(&mut out)
                });
                if let Err(e) = read {
                    return ToolResult::error(format!("get_compile_stats: cannot read {}: {e}", path.display()));
                }
                out
            };
            let st = hp_compile::stats::parse_text(&String::from_utf8_lossy(&bytes));
            let mut v = stats_json(&st, &hp_compile::stats::hints(&st));
            v["from_file"] = json!(true);
            v["file"] = json!(path.display().to_string());
            ToolResult::json(v)
        }));
    }
    Ok(Outcome::AppFn(AppFn(Box::new(move |app, _ctx| {
        let Some((map, running, st, hints)) = app.compile.mcp_stats() else {
            return ToolResult::json(json!({
                "compiled": false,
                "note": "no compile ran in this window yet (from_file: true reads <map>.log)",
            }));
        };
        let mut v = stats_json(&st, &hints);
        v["compiled"] = json!(true);
        v["map"] = json!(map);
        v["running"] = json!(running);
        ToolResult::json(v)
    }))))
}

fn history_file(state: &AppState) -> PathBuf {
    let key = hp_app::sidecar::map_key(state.doc.path());
    hp_app::settings::Settings::cache_dir().join("compile_history").join(format!("{key}.jsonl"))
}

fn get_compile_history(state: &mut AppState, a: &Args<'_>) -> R<Outcome> {
    let limit = a.opt_u64("limit", 1, hp_mcp::tools::MAX_LIST)?.unwrap_or(100) as usize;
    let path = history_file(state);
    Ok(Worker::spawn("get_compile_history", move || {
        let entries = hp_compile::history::load(&path);
        let total = entries.len();
        let list: Vec<Value> = entries
            .iter()
            .rev()
            .take(limit)
            .map(|e| {
                json!({
                    "when_unix": e.when,
                    "preset": e.preset,
                    "background": e.background,
                    "success": e.success,
                    "leaked": e.leaked,
                    "total_ms": e.total_ms,
                    "portal_clusters": e.portalclusters,
                    "portals": e.numportals,
                    "steps": e.steps.iter().map(|s| format!("{s:?}")).collect::<Vec<_>>(),
                })
            })
            .collect();
        ToolResult::json(json!({ "total": total, "compiles": list }))
    }))
}

fn get_compilers(state: &mut AppState, _a: &Args<'_>) -> R<ToolResult> {
    let flags = |step: hp_compile::StepKind| -> Vec<Value> {
        hp_compile::flags::FLAGS
            .iter()
            .filter(|f| f.step == step)
            .map(|f| {
                json!({
                    "flag": f.flag,
                    "takes_value": f.value,
                    "what": f.english,
                    "group": f.group().key(),
                    "set_by": f.managed_by().map(|m| m.option()),
                })
            })
            .collect()
    };
    let exe = std::env::current_exe().map(|p| p.display().to_string()).unwrap_or_default();
    Ok(ToolResult::json(json!({
        "compiler": "Hammer Pro",
        "version": env!("CARGO_PKG_VERSION"),
        "executable": exe,
        "choice": true,
        "active_compiler": state.settings.compile.active_compiler_id(),
        "steps": [
            {
                "step": "bsp",
                "does": "geometry, portals, leak test (pointfile) and visibility",
                "command": format!("{} [flags] <map.vmf>", hp_compile::BSPC_ARG),
                "flags": flags(hp_compile::StepKind::Bsp),
            },
            {
                "step": "rad",
                "does": "lighting (LDR, HDR or both)",
                "command": format!("{} [flags] <map>", hp_compile::RAD_ARG),
                "flags": flags(hp_compile::StepKind::Rad),
            },
            { "step": "pack", "does": "custom content packed into the BSP pakfile, in process" },
        ],
        "content_args": hp_ui_mapping::compile_content_args(state.assets.fs()),
    })))
}

fn check_leaks(state: &mut AppState, _a: &Args<'_>) -> R<ToolResult> {
    state.request(Action::CheckLeaks);
    Ok(ToolResult::json(json!({ "requested": true, "note": "Read the result with get_leak in a moment." })))
}

fn get_leak(state: &mut AppState, a: &Args<'_>) -> R<ToolResult> {
    let max = a.opt_u64("points", 1, 10_000)?.unwrap_or(200) as usize;
    let h = &state.runtime.health;
    let instant = h.leak.as_ref().map(|l| {
        let r = &l.report;
        json!({
            "leak": r.leak.as_ref().map(|p| json!({
                "entity": p.entity,
                "points": p.points.iter().take(max).map(|x| v3(*x)).collect::<Vec<_>>(),
            })),
            "leaking_entities": r.leaking.iter().take(500).collect::<Vec<_>>(),
            "entities_in_solids": r.in_solid.iter().take(500).collect::<Vec<_>>(),
            "incomplete": r.incomplete,
            "ms": l.ms,
            "up_to_date": l.serial == state.doc.revision(),
        })
    });
    let pointfile = state.pointfile.as_ref().map(|p| {
        json!({
            "file": p.path.display().to_string(),
            "points": p.points.len(),
            "path": p.points.iter().take(max).map(|x| v3(*x)).collect::<Vec<_>>(),
        })
    });
    Ok(ToolResult::json(
        json!({ "checking": h.leak_checking, "instant_check": instant, "compile_pointfile": pointfile }),
    ))
}

fn load_pointfile(state: &mut AppState, a: &Args<'_>) -> R<ToolResult> {
    let unload = a.opt_bool("unload")?.unwrap_or(false);
    if !unload && state.doc.path().is_none() {
        return Err("the map has no file: compile it first".to_owned());
    }
    state.request(if unload { Action::UnloadPointfile } else { Action::LoadPointfile });
    Ok(ToolResult::json(
        json!({ "requested": if unload { "unload" } else { "load" }, "note": "get_leak shows the pointfile." }),
    ))
}

fn portals_json(state: &AppState) -> Value {
    let h = &state.runtime.health;
    json!({
        "loaded": h.portals.as_ref().map(|p| json!({
            "clusters": p.clusters,
            "portals": p.portals.len(),
            "bounds": super::tools::bounds_json(Some(p.bounds)),
        })),
        "file": h.portal_path.as_ref().map(|p| p.display().to_string()),
        "building": h.portal_build_running,
    })
}

fn portal_file(state: &mut AppState, a: &Args<'_>) -> R<ToolResult> {
    let action = a.opt_enum("action", &["info", "load", "build", "unload"])?.unwrap_or_else(|| "info".to_owned());
    let file = a.opt_str("file", 255)?;
    if file.is_some() && action != "load" {
        return Err("file is only for the load action".to_owned());
    }
    match action.as_str() {
        "load" => {
            let dir = map_dir(state)?;
            let name = match file {
                Some(f) => relative_file(f, "prt")?,
                None => {
                    let stem = state.doc.path().and_then(|p| p.file_stem()).map(|s| s.to_string_lossy().into_owned());
                    format!("{}.prt", stem.ok_or("the map has no file")?)
                }
            };
            let path = dir.join(&name);
            if !path.is_file() {
                return Err(format!("no portal file {name} in the map folder (compile the map, or action build)"));
            }
            state.runtime.health.portal_load_request = Some(path);
        }
        "build" => {
            if state.runtime.health.portal_build_running {
                return Err("portals are being built already".to_owned());
            }
            state.request(Action::BuildPortals);
        }
        "unload" => state.request(Action::UnloadPortalFile),
        _ => {}
    }
    Ok(ToolResult::json(portals_json(state)))
}

fn bake_lighting(state: &mut AppState, a: &Args<'_>) -> R<ToolResult> {
    let action = match a.opt_enum("action", &["start", "around_camera", "stop"])?.as_deref() {
        Some("around_camera") => Action::BakeAroundCamera,
        Some("stop") => Action::StopBake,
        _ => Action::BakeLighting,
    };
    let quality = a.opt_enum("quality", &["draft", "normal", "final"])?.map(|q| match q.as_str() {
        "draft" => LightingQuality::Draft,
        "final" => LightingQuality::Final,
        _ => LightingQuality::Normal,
    });
    if action == Action::StopBake {
        if quality.is_some() {
            return Err("quality is for a bake that starts (start or around_camera)".to_owned());
        }
        if state.runtime.lighting.progress.is_none() {
            return Ok(ToolResult::json(json!({ "stopped": false, "note": "no bake is running" })));
        }
    }
    if let Some(q) = quality
        && q != state.settings.lighting.quality
    {
        state.settings.lighting.quality = q;
        state.settings_dirty = true;
    }
    state.request(action);
    Ok(ToolResult::json(json!({
        "requested": action.id(),
        "quality": quality_name(state.settings.lighting.quality),
        "note": "Read the progress with get_lighting_status.",
    })))
}

fn set_lighting_options(state: &mut AppState, a: &Args<'_>) -> R<ToolResult> {
    if a.map.is_empty() {
        return Err("give at least one option (get_lighting_status lists them in options)".to_owned());
    }
    let mut o = state.doc.light_plus();
    if let Some(v) = a.opt_f64("brightness", 0.0, 4.0)? {
        o.brightness_scale = v as f32;
    }
    if let Some(v) = a.opt_bool("bounced")? {
        o.bounced = v;
    }
    if let Some(v) = a.opt_bool("dynamic_updates")? {
        o.incremental = v;
    }
    if let Some(v) = a.opt_f64("update_delay", 0.0, 60.0)? {
        o.incremental_delay = v as f32;
    }
    if let Some(v) = a.opt_f64("bake_radius", 64.0, 65536.0)? {
        o.bake_dist = v as f32;
    }
    if let Some(v) = a.opt_u64("sun_samples", 1, 256)? {
        o.samples_sun = v as u32;
    }
    if let Some(v) = a.opt_u64("ambient_samples", 16, 5184)? {
        o.samples_ambient = v as u32;
    }
    if let Some(v) = a.opt_bool("supersample")? {
        o.supersample = v;
    }
    state.doc.set_light_plus(&o).map_err(|e| e.to_string())?;
    Ok(ToolResult::json(lighting_options(&state.doc.light_plus())))
}

fn lighting_options(o: &hp_editor::luxels::LightPlus) -> Value {
    json!({
        "brightness": o.brightness_scale, "bounced": o.bounced, "dynamic_updates": o.incremental,
        "update_delay": o.incremental_delay, "bake_radius": o.bake_dist, "sun_samples": o.samples_sun,
        "ambient_samples": o.samples_ambient, "supersample": o.supersample,
    })
}

fn quality_name(q: LightingQuality) -> &'static str {
    match q {
        LightingQuality::Draft => "draft",
        LightingQuality::Normal => "normal",
        LightingQuality::Final => "final",
    }
}

fn prop_shadows_name(p: hp_app::settings::StaticPropShadows) -> &'static str {
    match p {
        hp_app::settings::StaticPropShadows::None => "none",
        hp_app::settings::StaticPropShadows::Collision => "collision",
        hp_app::settings::StaticPropShadows::Mesh => "mesh",
    }
}

fn get_lighting_status(state: &mut AppState, _a: &Args<'_>) -> R<ToolResult> {
    let l = &state.runtime.lighting;
    let lower = |v: String| v.to_ascii_lowercase();
    Ok(ToolResult::json(json!({
        "options": lighting_options(&state.doc.light_plus()),
        "quality": quality_name(state.settings.lighting.quality),
        "shown": state.settings.view.render_mode == hp_app::settings::RenderModeSetting::Baked,
        "baking": l.progress.as_ref().map(|p| json!({
            "kind": lower(format!("{:?}", p.kind)), "phase": lower(format!("{:?}", p.phase)),
            "fraction": p.fraction, "elapsed_s": p.elapsed_s, "faces": p.faces,
        })),
        "last": l.last.as_ref().map(|s| json!({
            "kind": lower(format!("{:?}", s.kind)), "cancelled": s.cancelled, "seconds": s.seconds,
            "faces": s.faces, "luxels": s.luxels, "density": s.density, "atlas_full": s.atlas_full,
        })),
        "has_bake": l.has_bake,
        "stale": l.stale,
        "no_lights": l.no_lights,
        "preview": {
            "faces": l.stats.faces, "baked_faces": l.stats.baked_faces, "luxels": l.stats.luxels,
            "triangles": l.stats.triangles, "layers": l.stats.layers, "gpu_bytes": l.stats.gpu_bytes,
            "models": {
                "world_lights": l.stats.world_lights, "static_props": l.stats.static_props,
                "vertex_lit_props": l.stats.vertex_lit_props,
            },
        },
        "realtime": state.assets.light_preview().map(|r| json!({
            "lights": r.lights, "suns": r.directional, "left_out": r.left_out,
            "crowded_clusters": r.crowded_clusters, "max_lights_per_cluster": r.max_per_cluster,
            "hdr_values": r.hdr, "sky_occlusion": r.sky_occlusion,
            "projectors": state.gfx.as_ref().map_or(0, |g| g.renderer.projector_count()),
            "sky_layers": { "sun": r.sky_layers[0], "sky_light": r.sky_layers[1] },
            "prop_shadows": prop_shadows_name(hp_app::runtime::render::prop_shadows(&state.settings)),
            "prop_shadows_setting": lower(format!("{:?}", state.settings.render.prop_shadows)),
            "indirect": {
                "setting": state.settings.render.lit_indirect, "state": l.indirect.phase.name(),
                "cells": l.indirect.cells, "cells_in_the_air": l.indirect.valid_cells, "spacing": l.indirect.spacing,
                "gpu_bytes": l.indirect.gpu_bytes, "bake_seconds": l.indirect.seconds,
            },
            "sun": r.sun.map(|s| json!({
                "direction": s.direction, "color": s.color, "ambient": s.ambient,
                "spread_deg": s.spread_deg, "entity_origin": s.origin,
            })),
        })),
        "tone": state.gfx.as_ref().map(|g| {
            let r = &g.renderer;
            let view = state.ui.focused_view.unwrap_or(0) as u32;
            let (settings, env) = (r.tone_settings(), r.tone_env());
            let (min, max) = settings.range(&env);
            json!({
                "mode": lower(format!("{:?}", settings.mode)),
                "scale": r.tone_scale(view),
                "exposure_range": [min, max],
                "bloom_amount": r.tone_bloom(),
                "map_controller": {
                    "exposure_min": env.exposure_min, "exposure_max": env.exposure_max, "bloom_scale": env.bloom_scale,
                },
                "histogram": r.tone_histogram(view),
            })
        }),
        "budget": l.budget.as_ref().map(|b| json!({
            "faces": b.faces, "luxels": b.luxels, "bytes": b.bytes, "limit": b.limit,
            "tiny_faces": b.tiny_faces, "wide_faces": b.wide_faces, "oversized_displacements": b.oversized_displacements,
            "top": b.top.iter().take(20).map(|e| json!({ "face": { "solid": e.solid, "side": e.side }, "bytes": e.bytes, "scale": e.scale, "luxels": e.dims, "displacement": e.displacement })).collect::<Vec<_>>(),
        })),
    })))
}

fn launch_game(state: &mut AppState, a: &Args<'_>) -> R<ToolResult> {
    if a.opt_bool("confirm")? != Some(true) {
        return Err("start the game only when the user asked for it (confirm: true)".to_owned());
    }
    if hp_compile::sandbox::is_sandboxed() {
        return Err(format!("refused in this automated run: {}", hp_compile::sandbox::REFUSED));
    }
    if state.doc.path().is_none() {
        return Err("save and compile the map first".to_owned());
    }
    let action = if a.opt_bool("reload")?.unwrap_or(false) { Action::ReloadInGame } else { Action::LaunchGame };
    if !action.enabled(state) {
        return Err(format!(
            "{} is not possible now (compile the map first, or start the game before reloading)",
            action.id()
        ));
    }
    state.request(action);
    Ok(ToolResult::json(json!({ "requested": action.id() })))
}

#[cfg(test)]
mod tests {
    use serde_json::json;

    use super::super::semantic_tests::{err, needs_app, ok};

    #[test]
    fn compile_options_and_requests() {
        let mut s = hp_app::testing::test_state();
        let o = ok(&mut s, "get_compile_options", json!({}));
        assert!(o["presets"].as_array().unwrap().len() == 4);
        err(&mut s, "compile_with_options", json!({ "preset": "draft" }));
        s.doc.set_path(Some(std::env::temp_dir().join("hp_mcp_compile_test.vmf")));
        err(&mut s, "compile_with_options", json!({ "run_bsp": false, "run_vis": false, "run_rad": false }));
        err(&mut s, "compile_with_options", json!({ "cordon_only": true }));
        err(&mut s, "compile_with_options", json!({ "profile": "nope" }));
        let r = super::super::semantic_tests::call(
            &mut s,
            "compile_with_options",
            json!({ "bsp_mode": "onlyents", "lighting": "hdr" }),
        );
        assert_eq!(r.first_text(), "app closure");
        let w = super::options_settings(
            &s,
            &super::super::semantic::checked(
                super::SPECS.iter().find(|t| t.name == "compile_with_options").unwrap(),
                &json!({ "bsp_mode": "onlyents", "threads": 4 }),
            )
            .unwrap(),
        )
        .unwrap();
        assert_eq!(w.bsp_mode, hp_app::settings::BspModeSetting::OnlyEnts);
        assert_eq!(w.threads, Some(4));
        assert_eq!(w.preset, hp_app::settings::CompilePresetSetting::Custom);
        assert!(!w.launch_game);
        assert!(needs_app(&mut s, "cancel_compile", json!({})));
        assert!(needs_app(&mut s, "get_compile_log", json!({ "tail": 5 })));
        ok(&mut s, "check_leaks", json!({}));
        assert!(s.requested_actions.contains(&hp_app::Action::CheckLeaks));
        let l = ok(&mut s, "get_leak", json!({}));
        assert!(l["compile_pointfile"].is_null());
        ok(&mut s, "bake_lighting", json!({}));
        assert!(s.requested_actions.contains(&hp_app::Action::BakeLighting));
        ok(&mut s, "get_lighting_status", json!({}));
        let p = ok(&mut s, "portal_file", json!({}));
        assert!(p["loaded"].is_null());
        err(&mut s, "portal_file", json!({ "action": "load", "file": "../x.prt" }));
        err(&mut s, "portal_file", json!({ "action": "load" }));
        err(&mut s, "launch_game", json!({ "confirm": false }));
        let c = ok(&mut s, "get_compilers", json!({}));
        assert_eq!(c["compiler"], "Hammer Pro");
        assert_eq!(c["choice"], true);
        assert_eq!(c["active_compiler"], "builtin");
        assert!(c["steps"][0]["flags"].as_array().is_some_and(|f| f.iter().any(|f| f["flag"] == "-vis")));
        let h = ok(&mut s, "get_compile_history", json!({}));
        assert!(h["total"].is_number());
        let rad = c["steps"][1]["flags"].as_array().unwrap();
        for f in ["-final", "-softsun", "-extrasky", "-smooth", "-StaticPropPolys", "-textureshadows", "-lights"] {
            assert!(rad.iter().any(|x| x["flag"] == f), "{f}");
        }
        let bsp = c["steps"][0]["flags"].as_array().unwrap();
        for f in ["-nodetail", "-forceskyvis", "-radius_override", "-AllowDynamicPropsAsStatic", "-wsaddons"] {
            assert!(bsp.iter().any(|x| x["flag"] == f), "{f}");
        }
    }

    fn scratch(name: &str) -> std::path::PathBuf {
        let dir = std::env::temp_dir().join(format!("hp_mcp_{name}_{}", std::process::id()));
        let _ = std::fs::remove_dir_all(&dir);
        std::fs::create_dir_all(&dir).expect("scratch folder");
        dir
    }

    #[test]
    fn step_options_are_forced_like_the_menu() {
        use hp_app::settings::{CompilePresetSetting, CompileSettings, FlagChoiceSetting};
        use hp_compile::StepKind::{Bsp, Rad};

        use super::{apply_flag_options as apply, options_json};
        let mut s =
            CompileSettings { extra_bsp_args: "-nodetail -micro 2 -foo".to_owned(), ..CompileSettings::default() };
        apply(
            &mut s,
            Bsp,
            &json!({ "nodetail": false, "luxelscale": 2, "trace": [1, 5], "allowdynamicpropsasstatic": true }),
            None,
            "bsp_options",
        )
        .unwrap();
        assert_eq!(s.extra_bsp_args, "-micro 2 -foo");
        assert_eq!(s.bsp_flag_choices["-nodetail"], FlagChoiceSetting { on: false, value: String::new() });
        assert_eq!(s.bsp_flag_choices["-luxelscale"].value, "2");
        assert_eq!(s.bsp_flag_choices["-trace"].value, "1 5");
        assert!(s.bsp_flag_choices["-AllowDynamicPropsAsStatic"].on);
        let o = options_json(&s, Bsp);
        assert_eq!(o["options"]["luxelscale"], 2.0);
        assert_eq!(o["options"]["trace"], json!([1.0, 5.0]));
        assert_eq!(o["options"]["AllowDynamicPropsAsStatic"], true);
        assert_eq!(o["options"]["nodetail"], false);
        assert_eq!(o["options"]["micro"], 2.0);
        assert!(o["forced"].get("micro").is_none() && o["forced"]["nodetail"] == false);
        assert_eq!(o["other_arguments"], json!(["-foo"]));
        assert_eq!(o["preset_flags"], "-vis -leaktest");
        let line = o["command_flags"].as_str().unwrap();
        assert!(line.contains("-luxelscale 2") && line.contains("-trace 1 5") && !line.contains("-nodetail"), "{line}");
        apply(&mut s, Bsp, &json!({ "luxelscale": 0.5, "micro": null, "trace": null }), None, "b").unwrap();
        assert_eq!(s.bsp_flag_choices["-luxelscale"].value, "0.5");
        assert!(!s.bsp_flag_choices.contains_key("-trace"));
        assert_eq!(s.extra_bsp_args, "-foo");
        apply(&mut s, Bsp, &json!({ "blocksize": 0, "propdebug": true }), None, "b").unwrap();
        assert_eq!(s.bsp_flag_choices["-blocksize"].value, "0");
        assert!(s.bsp_flag_choices["-propdebug"].on);
        apply(&mut s, Bsp, &json!({ "blocksize": 4096.5 }), None, "b").unwrap();
        assert_eq!(s.bsp_flag_choices["-blocksize"].value, "4096");
        for bad in [json!({ "blocksize": 64 }), json!({ "blocksize": 40000 }), json!({ "blocksize": true })] {
            assert!(apply(&mut s.clone(), Bsp, &bad, None, "bsp_options").unwrap_err().contains("blocksize"), "{bad}");
        }
        apply(&mut s, Bsp, &json!({ "blocksize": null, "propdebug": null }), None, "b").unwrap();
        for bad in [
            json!({ "threads": 4 }),
            json!({ "vis": true }),
            json!({ "game": "c:/x" }),
            json!({ "nope": true }),
            json!({ "nodetail": 3 }),
            json!({ "luxelscale": true }),
            json!({ "luxelscale": -1 }),
            json!({ "trace": [1] }),
            json!({ "embed": "../x" }),
            json!({ "final": true }),
            json!([1]),
        ] {
            assert!(apply(&mut s.clone(), Bsp, &bad, None, "bsp_options").is_err(), "{bad}");
        }
        for bad in [
            json!({ "ldr": true }),
            json!({ "bounce": "x" }),
            json!({ "lights": "x.rad" }),
            json!({ "nostaticpropshadows": true }),
        ] {
            let e = apply(&mut s.clone(), Rad, &bad, None, "rad_options").unwrap_err();
            if bad.get("nostaticpropshadows").is_some() {
                assert!(e.contains("static_prop_shadows"), "{e}");
            }
        }
        let mut r = CompileSettings { preset: CompilePresetSetting::Draft, ..CompileSettings::default() };
        apply(
            &mut r,
            Rad,
            &json!({ "fast": false, "bounce": 8, "StaticPropLighting": true, "softsun": 2.5, "extrasky": 4, "chop": 2 }),
            None,
            "rad_options",
        )
        .unwrap();
        let o = options_json(&r, Rad);
        assert_eq!(o["preset_flags"], "-bounce 1 -fast");
        assert_eq!(o["command_flags"], "-StaticPropLighting -bounce 8 -chop 2 -extrasky 4 -softsun 2.5");
        assert_eq!(o["forced"]["fast"], false);
        r.static_prop_lighting = false;
        r.static_prop_shadows = false;
        let o = options_json(&r, Rad);
        assert_eq!(o["command_flags"], "-bounce 8 -chop 2 -extrasky 4 -softsun 2.5 -nostaticpropshadows");
        let dir = scratch("rad_lights").join("my maps");
        std::fs::create_dir_all(dir.join("content")).unwrap();
        std::fs::write(dir.join("extra.rad"), "lights/white 255 255 255 200\n").unwrap();
        apply(&mut r, Rad, &json!({ "lights": "extra.rad" }), Some(&dir), "rad_options").unwrap();
        assert_eq!(r.rad_flag_choices["-lights"].value, dir.join("extra.rad").display().to_string());
        let line = options_json(&r, Rad)["command_flags"].as_str().unwrap().to_owned();
        assert!(line.contains("-lights \"") && line.contains("extra.rad"), "{line}");
        apply(&mut r, Bsp, &json!({ "embed": "content" }), Some(&dir), "bsp_options").unwrap();
        assert!(r.bsp_flag_choices["-embed"].value.ends_with("content"));
        assert!(apply(&mut r, Bsp, &json!({ "embed": "missing" }), Some(&dir), "bsp_options").is_err());
        let _ = std::fs::remove_dir_all(dir.parent().unwrap());
    }

    #[test]
    fn saved_compile_settings() {
        let mut s = hp_app::testing::test_state();
        err(&mut s, "set_compile_options", json!({}));
        let o = ok(
            &mut s,
            "set_compile_options",
            json!({
                "bsp_options": { "nodetail": true, "forceskyvis": true },
                "rad_options": { "softsun": 2, "StaticPropPolys": true, "textureshadows": true },
                "copy_to_maps": false,
                "vis_mode": "fast",
                "keep_cubemaps": false,
                "pack_policy": "always",
            }),
        );
        let c = &s.settings.compile;
        assert!(c.extra_bsp_args.is_empty() && c.extra_rad_args.is_empty());
        let on = |m: &std::collections::BTreeMap<String, hp_app::settings::FlagChoiceSetting>, f: &str| {
            m.get(f).is_some_and(|c| c.on)
        };
        assert!(on(&c.bsp_flag_choices, "-nodetail") && on(&c.bsp_flag_choices, "-forceskyvis"));
        assert!(on(&c.rad_flag_choices, "-StaticPropPolys") && on(&c.rad_flag_choices, "-textureshadows"));
        assert_eq!(c.rad_flag_choices["-softsun"].value, "2");
        assert!(!c.copy_to_maps && !s.settings.content.keep_cubemaps);
        assert_eq!(c.pack_policy, hp_app::settings::PackPolicy::Always);
        assert_eq!(c.preset, hp_app::settings::CompilePresetSetting::Custom);
        assert_eq!(o["bsp_options"]["options"]["nodetail"], true);
        assert_eq!(o["rad_options"]["options"]["softsun"], 2.0);
        assert_eq!(o["bsp_options"]["command_flags"], "-fastvis -leaktest -forceskyvis -nodetail");
        assert!(s.settings_dirty);
        assert_eq!((o["static_prop_lighting"].as_bool(), o["static_prop_shadows"].as_bool()), (Some(true), Some(true)));
        let o =
            ok(&mut s, "set_compile_options", json!({ "static_prop_lighting": false, "static_prop_shadows": false }));
        assert!(!s.settings.compile.static_prop_lighting && !s.settings.compile.static_prop_shadows);
        assert_eq!(o["static_prop_shadows"], false);
        assert!(o["rad_options"]["command_flags"].as_str().unwrap().ends_with("-nostaticpropshadows"));
        ok(&mut s, "set_compile_options", json!({ "static_prop_lighting": true, "static_prop_shadows": true }));
        let o = ok(&mut s, "set_compile_options", json!({ "preset": "final" }));
        let c = &s.settings.compile;
        assert_eq!(c.preset, hp_app::settings::CompilePresetSetting::Final);
        assert_eq!(c.rad_mode, hp_app::settings::RadModeSetting::Final);
        assert_eq!(c.vis_mode, hp_app::settings::VisModeSetting::Normal);
        assert_eq!(o["rad_options"]["preset_flags"], "-final -StaticPropLighting");
        ok(&mut s, "set_compile_options", json!({ "rad_mode": "draft", "vis_mode": "fast" }));
        assert_eq!(s.settings.compile.preset, hp_app::settings::CompilePresetSetting::Draft);
        ok(&mut s, "set_compile_options", json!({ "copy_to_maps": true, "launch_game": false }));
        assert!(s.settings.compile.copy_to_maps && !s.settings.compile.launch_game);
        err(&mut s, "set_compile_options", json!({ "launch_game": true }));
        if !hp_compile::sandbox::is_sandboxed() {
            ok(&mut s, "set_compile_options", json!({ "launch_game": true, "confirm": true }));
            assert!(s.settings.compile.launch_game);
        }
        let dir = scratch("maps_dir");
        err(&mut s, "set_compile_options", json!({ "maps_dir_override": dir.display().to_string() }));
        err(&mut s, "set_compile_options", json!({ "maps_dir_override": "relative/maps", "confirm": true }));
        let o = ok(
            &mut s,
            "set_compile_options",
            json!({ "maps_dir_override": dir.display().to_string(), "confirm": true }),
        );
        assert_eq!(s.settings.compile.maps_dir_override.as_deref(), Some(dir.as_path()));
        assert_eq!(o["maps_dir_override"], dir.display().to_string());
        ok(&mut s, "set_compile_options", json!({ "maps_dir_override": "" }));
        assert!(s.settings.compile.maps_dir_override.is_none());
        let _ = std::fs::remove_dir_all(&dir);
        let before = s.settings.compile.clone();
        err(&mut s, "set_compile_options", json!({ "copy_to_maps": false, "bsp_options": { "threads": 2 } }));
        err(&mut s, "set_compile_options", json!({ "run_bsp": false, "run_vis": false, "run_rad": false }));
        err(&mut s, "set_compile_options", json!({ "expert": true }));
        err(&mut s, "set_compile_options", json!({ "rad_options": { "bounce": 4, "nostaticpropshadows": true } }));
        assert_eq!(s.settings.compile, before);
        for p in [
            "compile.launch_game",
            "compile.maps_dir_override",
            "compile.extra_rad_args",
            "compile.rad_flag_choices",
            "compile.bsp_flag_choices",
        ] {
            let r = super::super::semantic_tests::call(&mut s, "set_setting", json!({ "path": p, "value": true }));
            assert!(r.is_error && r.first_text().contains("set_compile_options"), "{p}");
        }
        s.doc.set_path(Some(std::env::temp_dir().join("hp_mcp_compile_options.vmf")));
        let one = super::options_settings(
            &s,
            &super::super::semantic::checked(
                super::SPECS.iter().find(|t| t.name == "compile_with_options").unwrap(),
                &json!({
                    "rad_options": { "final": true, "bounce": 2 },
                    "bsp_options": { "nodetail": false },
                    "copy_to_maps": false,
                    "static_prop_shadows": false,
                }),
            )
            .unwrap(),
        )
        .unwrap();
        assert!(on(&one.rad_flag_choices, "-final") && on(&one.rad_flag_choices, "-StaticPropPolys"));
        assert_eq!(one.rad_flag_choices["-bounce"].value, "2");
        assert!(!on(&one.bsp_flag_choices, "-nodetail") && on(&one.bsp_flag_choices, "-forceskyvis"));
        assert!(!one.copy_to_maps && !one.launch_game && !one.static_prop_shadows);
        assert!(on(&s.settings.compile.bsp_flag_choices, "-nodetail") && s.settings.compile.static_prop_shadows);
    }

    #[test]
    fn compile_log_file_problems_and_bake_quality() {
        let mut s = hp_app::testing::test_state();
        err(&mut s, "get_compile_log", json!({ "from_file": true }));
        let dir = scratch("compile_log");
        s.doc.set_path(Some(dir.join("gm_test.vmf")));
        let r = ok(&mut s, "get_compile_log", json!({ "from_file": true }));
        assert_eq!(r["found"], false);
        std::fs::write(dir.join("gm_test.log"), "old compile\r\n").unwrap();
        std::thread::sleep(std::time::Duration::from_millis(50));
        let build = dir.join(".hammerpro_build");
        std::fs::create_dir_all(&build).unwrap();
        std::fs::write(
            build.join("gm_test.log"),
            "Hammer Pro geometry\r\nBrush 12: WARNING, microbrush\r\n**** leaked ****\r\nDone\r\n",
        )
        .unwrap();
        let r = ok(&mut s, "get_compile_log", json!({ "from_file": true }));
        assert_eq!(r["found"], true);
        assert!(r["file"].as_str().unwrap().contains(".hammerpro_build"));
        let codes: Vec<&str> = r["problems"].as_array().unwrap().iter().filter_map(|p| p["code"].as_str()).collect();
        assert!(codes.contains(&"microbrush") && codes.contains(&"leak"), "{codes:?}");
        let lines = r["lines"].as_array().unwrap();
        let micro = lines.iter().find(|l| l["text"].as_str().unwrap().contains("microbrush")).unwrap();
        assert_eq!(micro["problem"]["code"], "microbrush");
        assert_eq!(micro["location"]["brush"], 12);
        assert!(micro["problem"]["explanation"].as_str().unwrap().len() > 20);
        let r = ok(&mut s, "get_compile_log", json!({ "from_file": true, "problems_only": true, "tail": 1 }));
        assert_eq!(r["lines"].as_array().unwrap().len(), 1);
        let r = ok(&mut s, "get_compile_log", json!({ "from_file": true, "contains": "DONE" }));
        assert_eq!(r["lines"][0]["text"], "Done");
        assert!(needs_app(&mut s, "get_compile_stats", json!({})));
        let r = ok(&mut s, "get_compile_stats", json!({ "from_file": true }));
        assert_eq!(r["found"], false);
        std::fs::write(
            build.join("gm_test.log"),
            "Hammer Pro map compiler (hp-bspc 0)\r\n---- Map statistics ----\r\n\
             brushsides                   60000/65536      480000/524288  ( 91.6%)\r\n\
             ---- Geometry details ----\r\nBrushes: 40 total, 30 structural, 5 detail, 1 water, 2 tool, 2 in brush entities\r\n\
             Small structural brushes (top 1 of 25):\r\n#1  brush 77  16 x 16 x 16 units  at (8 8 8)\r\n",
        )
        .unwrap();
        let r = ok(&mut s, "get_compile_stats", json!({ "from_file": true }));
        assert_eq!(r["found"], true);
        assert_eq!(r["stats"]["limits"][0]["name"], "brushsides");
        assert_eq!(r["stats"]["geometry"]["brushes"]["structural"], 30);
        let hints = r["hints"].as_array().unwrap();
        let small = hints.iter().find(|h| h["kind"] == "small_brushes").unwrap();
        assert_eq!(small["location"]["brush"], 77);
        assert!(small["text"].as_str().unwrap().contains("func_detail"));
        assert!(hints.iter().any(|h| h["kind"] == "near_limit" && h["level"] == "important"));
        let _ = std::fs::remove_dir_all(&dir);
        let r = ok(&mut s, "bake_lighting", json!({ "quality": "final" }));
        assert_eq!(r["quality"], "final");
        assert_eq!(s.settings.lighting.quality, hp_app::settings::LightingQuality::Final);
        assert!(s.requested_actions.contains(&hp_app::Action::BakeLighting));
        ok(&mut s, "bake_lighting", json!({ "action": "around_camera", "quality": "draft" }));
        assert_eq!(s.settings.lighting.quality, hp_app::settings::LightingQuality::Draft);
        err(&mut s, "bake_lighting", json!({ "action": "stop", "quality": "normal" }));
        let st = ok(&mut s, "get_lighting_status", json!({}));
        assert_eq!(st["quality"], "draft");
        assert_eq!(st["options"]["ambient_samples"], 40, "the Hammer++ defaults");
        let o =
            ok(&mut s, "set_lighting_options", json!({ "ambient_samples": 80, "supersample": true, "sun_samples": 3 }));
        assert_eq!(
            (o["ambient_samples"].as_u64(), o["sun_samples"].as_u64(), o["supersample"].as_bool()),
            (Some(80), Some(3), Some(true))
        );
        let plus = s.doc.light_plus();
        assert_eq!((plus.samples_ambient, plus.samples_sun, plus.supersample), (80, 3, true));
        err(&mut s, "set_lighting_options", json!({}));
        err(&mut s, "set_lighting_options", json!({ "ambient_samples": 4 }));
    }
}
