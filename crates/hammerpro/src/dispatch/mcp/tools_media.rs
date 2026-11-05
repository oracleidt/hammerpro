use hp_app::AppState;
use hp_audio::{Edit, Sound};
use hp_colorgrade::{FOLDERS, Grade};
use hp_mcp::protocol::ToolResult;
use hp_ui_lab::media;
use serde_json::{Value, json};

use super::semantic::*;
use super::tools::{Args, R};

const MAX_PATH: usize = 600;
const MAX_BINS: u64 = 2048;

domain! { "sound and color";
    sound_info: Read, "Read a sound file",
        "Decode a sound (WAV, MP3 or OGG) from the game content, the mounted addons, the project folders or a file \
        on disk and report its sample rate, channels, duration, peak level, loop markers (WAV cue and sampler \
        chunks) and whether the engine plays its rate correctly, with an optional waveform of min and max pairs. \
        The window is the action sound_editor (run_action) or open_media_editor.",
        {
            "path": string("Game path (sound/ambient/wind.wav, or ambient/wind.wav) or the absolute path of a file on disk."),
            "bins": integer("Waveform points to return (0 for none, default 64).", 0, MAX_BINS as i64),
        }, ["path"];
    sound_edit: Editor, "Edit a sound and save it as WAV",
        "Edit a sound and save the result as a 16 bit PCM WAV file (the only output format; MP3 and OGG inputs are \
        decoded and written as WAV): gain in dB or normalization to a peak level, trim (start and end, or start and \
        length), fade in and out, loop (writes the cue and sampler chunks the Source engine reads to repeat the \
        sound), mono mix down and sample rate conversion to 11025, 22050 or 44100 Hz. Written into the content \
        folder: root (default: the folder above the maps folder of the saved map, else the game folder) plus \
        output (default: the source path with the .wav extension under sound/). An existing file is replaced \
        only with overwrite true. dry_run reports the result without writing. Undo does not cover files.",
        {
            "path": string("Game path or absolute file path of the source sound."),
            "output": string("Output path relative to root, for example sound/mymap/boom.wav (the sound/ folder and the .wav extension are added when missing). Default: derived from the source."),
            "root": string("Absolute folder to write into (the addon or game folder). Default: automatic."),
            "gain_db": number("Gain in dB (default 0).", -60.0, 60.0),
            "normalize_db": number("Instead of gain_db: set the gain so the peak of the kept part reaches this level in dB (for example -1).", -60.0, 0.0),
            "start": number("Start of the kept part in seconds (default 0).", 0.0, 36000.0),
            "end": number("End of the kept part in seconds (default: the end of the sound).", 0.0, 36000.0),
            "length": number("Length of the kept part in seconds, instead of end.", 0.001, 36000.0),
            "fade_in": number("Fade in in seconds.", 0.0, 600.0),
            "fade_out": number("Fade out in seconds.", 0.0, 600.0),
            "looped": boolean("Write loop markers so the engine repeats the sound (default: keep the loop state of the source)."),
            "loop_start": number("Loop start in seconds from the start of the kept part (default 0).", 0.0, 36000.0),
            "mono": boolean("Mix a stereo sound down to mono."),
            "sample_rate": integer("Convert to this sample rate in Hz (11025, 22050 or 44100 are the rates Source plays correctly).", 8000, 96000),
            "overwrite": boolean("Replace the output file when it exists (default false)."),
            "dry_run": boolean("Report what would be written without writing (default false)."),
        }, ["path"];
    sound_preview: Editor, "Play a sound edit",
        "Play a sound through the default audio device, with the same edit options as sound_edit applied, or stop \
        the playback (stop true). Playback exists on Windows only. The sound is not written anywhere.",
        {
            "path": string("Game path or absolute file path of the source sound (not needed with stop)."),
            "stop": boolean("Stop the playback instead of starting it."),
            "gain_db": number("Gain in dB (default 0).", -60.0, 60.0),
            "normalize_db": number("Instead of gain_db: normalize the peak to this level in dB.", -60.0, 0.0),
            "start": number("Start of the kept part in seconds.", 0.0, 36000.0),
            "end": number("End of the kept part in seconds.", 0.0, 36000.0),
            "length": number("Length of the kept part in seconds, instead of end.", 0.001, 36000.0),
            "fade_in": number("Fade in in seconds.", 0.0, 600.0),
            "fade_out": number("Fade out in seconds.", 0.0, 600.0),
            "looped": boolean("Repeat the sound until stopped."),
            "mono": boolean("Mix a stereo sound down to mono."),
            "sample_rate": integer("Convert to this sample rate in Hz.", 8000, 96000),
            "volume": number("Playback volume from 0 to 1 (default 0.8).", 0.0, 1.0),
        }, [];
    list_map_sounds: Read, "Sounds used by the map",
        "The sound files named by the key values of the map entities (ambient_generic message, soundscape and \
        trigger sounds, and every other key value ending in .wav, .mp3 or .ogg), as game paths under sound/.",
        {}, [];
    color_correction_presets: Read, "Color correction presets and parameters",
        "The grading presets (neutral, black_white, sepia, warm, cool, high_contrast, faded, night, vivid, \
        cinematic, horror, invert) with their parameters, the grade parameters with their ranges and defaults, \
        and the path convention: a color_correction entity reads a 32x32x32 RGB lookup table (a .raw file of \
        98304 bytes) named by its filename key value.",
        {}, [];
    color_correction_generate: Editor, "Create a color correction lookup table",
        "Build a 32x32x32 color lookup table (.raw, 98304 bytes, red varying fastest) from a preset or a grade and \
        save it so a color_correction entity can use it (filename key value = the returned reference). The grade \
        object has exposure (EV), brightness, contrast, gamma, saturation, vibrance, hue (degrees), temperature, \
        magenta (green to magenta), shadows, highlights, tint {color [r,g,b], amount}, colorize {color, amount}, \
        levels (four channels in the order master, red, green, blue, each {in_black, in_white, gamma, out_black, \
        out_white}), curves (four channels, each {points: [[x, y], ...]}) and invert; unlisted values keep their \
        neutral defaults and unknown keys are refused (color_correction_presets lists the ranges). base starts \
        from an existing .raw (an imported table) and applies the grade on top. The file is written to \
        root/folder/name.raw (folder default scripts/colorcorrection; materials/correction and \
        materials/colorcorrection are the other usual folders). An existing file is replaced only with overwrite \
        true. dry_run reports without writing.",
        {
            "name": string("File name without folder, for example cc_warm (the .raw extension is added)."),
            "preset": choice("Start from a preset.", &["neutral", "black_white", "sepia", "warm", "cool", "high_contrast", "faded", "night", "vivid", "cinematic", "horror", "invert"]),
            "grade": json!({ "type": "object", "additionalProperties": true, "description": "Grade parameters (see the tool description). Cannot be combined with preset." }),
            "base": string("Existing .raw to start from: a game path (scripts/colorcorrection/x.raw) or an absolute file path."),
            "folder": string("Folder under root (default scripts/colorcorrection)."),
            "root": string("Absolute folder to write into (the addon or game folder). Default: automatic."),
            "overwrite": boolean("Replace the file when it exists (default false)."),
            "dry_run": boolean("Report without writing (default false)."),
        }, ["name"];
    color_correction_inspect: Read, "Inspect a color lookup table",
        "Check a .raw color lookup table (it must be exactly 98304 bytes) and report whether it is the identity, \
        its average color and how far it moves colors (mean and maximum shift out of 255).",
        {
            "path": string("Game path (scripts/colorcorrection/x.raw) or absolute path of a .raw file."),
        }, ["path"];
    color_correction_preview: Editor, "Preview a color correction in the 3D views",
        "Apply a color lookup table to every 3D view at full weight, whatever the entities of the map say, until \
        off is true: from a preset, a grade (with an optional base) or an existing .raw (path). Nothing is saved \
        and the map is not changed. The color_correction entities of the map are previewed by the setting \
        lab.color_correction_preview (action toggle_color_correction_preview) and color_correction_zones reports \
        them; the 3D render mode color_corrected (set_render_mode) shows them without that setting.",
        {
            "preset": choice("Preview a preset.", &["neutral", "black_white", "sepia", "warm", "cool", "high_contrast", "faded", "night", "vivid", "cinematic", "horror", "invert"]),
            "grade": json!({ "type": "object", "additionalProperties": true, "description": "Grade parameters as in color_correction_generate." }),
            "base": string("Existing .raw to start from."),
            "path": string("Preview an existing .raw file as it is."),
            "off": boolean("Stop the preview."),
        }, [];
    color_correction_zones: Read, "color_correction entities and their weight",
        "Every color_correction and color_correction_volume entity with its lookup file (found or not), enabled \
        state, distance from the 3D camera (the editor camera, or the player view camera), falloff, weight and \
        status (active, disabled, out_of_range, outside_volume, excluded by an exclusive entity, no_file, \
        file_not_found), and what is applied now. The weight follows the engine: full weight inside minfalloff, \
        fading to 0 at maxfalloff, times maxweight; fadeInDuration and fadeOutDuration smooth enabling and \
        disabling; an exclusive entity with weight hides the others; several entities blend. Needs the setting \
        lab.color_correction_preview on, or the 3D render mode color_corrected (set_render_mode, Alt+9), for the weights to be applied to the views.",
        {}, [];
    open_media_editor: Editor, "Open the sound or color correction editor",
        "Open the sound editor or the color correction editor window, optionally loading a sound (a game path or \
        a file) or a .raw table into it. The same windows open with the actions sound_editor and \
        color_correction_editor (run_action).",
        {
            "editor": choice("Which editor.", &["sound", "color"]),
            "path": string("Sound or .raw table to load."),
        }, ["editor"];
}

fn num(a: &Args<'_>, key: &str, lo: f64, hi: f64) -> R<Option<f32>> {
    Ok(a.opt_f64(key, lo, hi)?.map(|v| v as f32))
}

fn load_sound(state: &AppState, a: &Args<'_>) -> R<(Sound, String)> {
    let path = a.str("path", MAX_PATH)?;
    let (bytes, label) = media::read_source(state, path, media::game_sound_path)?;
    let sound = hp_audio::decode(&bytes, &label).map_err(|e| format!("{label}: {e}"))?;
    Ok((sound, label))
}

fn edit_from(a: &Args<'_>, sound: &Sound) -> R<Edit> {
    let mut edit = Edit::from_sound(sound);
    if let Some(v) = a.opt_f64("start", 0.0, 36000.0)? {
        edit.start = v;
    }
    let end = a.opt_f64("end", 0.0, 36000.0)?;
    let length = a.opt_f64("length", 0.001, 36000.0)?;
    match (end, length) {
        (Some(_), Some(_)) => return Err("give end or length, not both".to_owned()),
        (Some(e), None) => edit.end = Some(e),
        (None, Some(l)) => edit.end = Some(edit.start + l),
        (None, None) => {}
    }
    if edit.end.is_some_and(|e| e <= edit.start) {
        return Err("end must be after start".to_owned());
    }
    if edit.start >= sound.duration() {
        return Err(format!("start is past the end of the sound ({:.3} s)", sound.duration()));
    }
    if let Some(v) = a.opt_f64("fade_in", 0.0, 600.0)? {
        edit.fade_in = v;
    }
    if let Some(v) = a.opt_f64("fade_out", 0.0, 600.0)? {
        edit.fade_out = v;
    }
    if let Some(v) = a.opt_bool("looped")? {
        edit.looped = v;
        edit.loop_start = 0.0;
    }
    if let Some(v) = a.opt_f64("loop_start", 0.0, 36000.0)? {
        edit.loop_start = v;
    }
    if let Some(v) = a.opt_bool("mono")? {
        edit.mono = v;
    }
    if let Some(v) = a.opt_u64("sample_rate", 8000, 96000)? {
        edit.rate = Some(v as u32);
    }
    match (num(a, "gain_db", -60.0, 60.0)?, num(a, "normalize_db", -60.0, 0.0)?) {
        (Some(_), Some(_)) => return Err("give gain_db or normalize_db, not both".to_owned()),
        (Some(g), None) => edit.gain_db = g,
        (None, Some(n)) => edit.gain_db = hp_audio::normalize_gain_db(sound, &edit, n).clamp(-60.0, 60.0),
        (None, None) => {}
    }
    Ok(edit)
}

fn sound_json(s: &Sound) -> Value {
    json!({
        "sample_rate": s.rate,
        "channels": s.channels,
        "frames": s.frames(),
        "duration_seconds": (s.duration() * 1000.0).round() / 1000.0,
        "peak_db": ((20.0 * f64::from(s.peak().max(1e-6)).log10()) * 10.0).round() / 10.0,
        "looped": s.looped,
        "loop_start_seconds": if s.rate == 0 { 0.0 } else { f64::from(s.loop_start) / f64::from(s.rate) },
        "source_friendly": s.is_source_friendly(),
    })
}

fn sound_info(state: &mut AppState, a: &Args<'_>) -> R<ToolResult> {
    let (sound, label) = load_sound(state, a)?;
    let bins = a.opt_u64("bins", 0, MAX_BINS)?.unwrap_or(64) as usize;
    let mut out = sound_json(&sound);
    out["source"] = json!(label);
    if bins > 0 {
        out["waveform"] =
            json!(hp_audio::peaks(&sound, bins).into_iter().map(|(lo, hi)| json!([lo, hi])).collect::<Vec<_>>());
    }
    if !sound.is_source_friendly() {
        out["note"] =
            json!("Source plays 11025, 22050 and 44100 Hz sounds correctly; convert with sound_edit sample_rate.");
    }
    Ok(ToolResult::json(out))
}

fn default_output(label: &str) -> String {
    if media::is_external(label) {
        let stem = std::path::Path::new(label)
            .file_stem()
            .map_or_else(|| "sound".to_owned(), |s| s.to_string_lossy().into_owned());
        format!("sound/{stem}.wav")
    } else {
        media::game_sound_path(label)
    }
}

fn sound_edit(state: &mut AppState, a: &Args<'_>) -> R<ToolResult> {
    let (sound, label) = load_sound(state, a)?;
    let edit = edit_from(a, &sound)?;
    let out = hp_audio::apply(&sound, &edit);
    let wanted = a.opt_str("output", MAX_PATH)?.map_or_else(|| default_output(&label), str::to_owned);
    let rel = media::clean_rel(&wanted, media::SOUND_PREFIX, "wav")?;
    let root = media::resolve_root(state, a.opt_str("root", MAX_PATH)?)?;
    let target = root.join(&rel);
    let overwrite = a.opt_bool("overwrite")?.unwrap_or(false);
    let dry = a.opt_bool("dry_run")?.unwrap_or(false);
    let bytes = hp_audio::encode_wav(&out);
    let mut result = json!({
        "source": label,
        "output": target.display().to_string(),
        "reference": rel,
        "bytes": bytes.len(),
        "gain_db": edit.gain_db,
        "result": sound_json(&out),
        "exists": target.exists(),
        "written": false,
    });
    if dry {
        result["dry_run"] = json!(true);
        return Ok(ToolResult::json(result));
    }
    media::write_file(&root, &rel, &bytes, overwrite)?;
    hp_ui_entities::model_import::remount_after_write(state);
    result["written"] = json!(true);
    Ok(ToolResult::json(result))
}

fn sound_preview(state: &mut AppState, a: &Args<'_>) -> R<ToolResult> {
    if a.opt_bool("stop")?.unwrap_or(false) {
        hp_audio::stop();
        return Ok(ToolResult::json(json!({ "playing": false })));
    }
    let (sound, label) = load_sound(state, a)?;
    let edit = edit_from(a, &sound)?;
    let out = hp_audio::apply(&sound, &edit);
    let volume = num(a, "volume", 0.0, 1.0)?.unwrap_or(0.8);
    if !hp_audio::play(&out, 0.0, None, edit.looped, volume) {
        return Err("playback is not available (no audio device, or not Windows)".to_owned());
    }
    Ok(ToolResult::json(json!({
        "playing": true,
        "source": label,
        "duration_seconds": out.duration(),
        "looped": edit.looped,
        "note": "Call sound_preview with stop true to stop.",
    })))
}

fn list_map_sounds(state: &mut AppState, _a: &Args<'_>) -> R<ToolResult> {
    let list = media::map_sounds(state);
    Ok(ToolResult::json(json!({ "total": list.len(), "sounds": list })))
}

const PRESET_IDS: &[&str] = &[
    "neutral",
    "black_white",
    "sepia",
    "warm",
    "cool",
    "high_contrast",
    "faded",
    "night",
    "vivid",
    "cinematic",
    "horror",
    "invert",
];

fn grade_json(g: &Grade) -> Value {
    serde_json::to_value(g).unwrap_or(Value::Null)
}

fn color_correction_presets(_state: &mut AppState, _a: &Args<'_>) -> R<ToolResult> {
    let presets: Vec<Value> =
        hp_colorgrade::PRESETS.iter().map(|p| json!({ "id": p.id, "grade": grade_json(&(p.build)()) })).collect();
    Ok(ToolResult::json(json!({
        "presets": presets,
        "defaults": grade_json(&Grade::default()),
        "ranges": {
            "exposure": [-5, 5], "brightness": [-1, 1], "contrast": [-1, 1], "gamma": [0.1, 5],
            "saturation": [0, 4], "vibrance": [-1, 1], "hue": [-180, 180], "temperature": [-1, 1],
            "magenta": [-1, 1], "shadows": [-1, 1], "highlights": [-1, 1],
            "tint.amount": [0, 1], "colorize.amount": [0, 1], "levels.*": [0, 1], "curves.*.points": "[[x, y], ...] with x and y from 0 to 1",
        },
        "levels_order": ["master", "red", "green", "blue"],
        "table": { "size": hp_colorgrade::LUT_SIZE, "bytes": hp_colorgrade::RAW_BYTES, "layout": "RGB bytes, red varies fastest, then green, then blue" },
        "folders": FOLDERS,
        "path_convention": "The color_correction entity key filename (Lookup Filename) is a path relative to the game folder or an addon, for example scripts/colorcorrection/cc_warm.raw; the engine reads the table from that path through its file system, so the file must be shipped at the same path. scripts/colorcorrection is the default here; materials/correction (the Source SDK convention) and materials/colorcorrection work the same way as long as the key value matches the file path.",
    })))
}

fn read_raw(state: &AppState, path: &str) -> R<Vec<u8>> {
    let bytes = if media::is_external(path) {
        std::fs::read(path.trim()).map_err(|e| format!("cannot read {path}: {e}"))?
    } else {
        media::read_game(state, path.trim().replace('\\', "/").trim_start_matches('/'))
            .ok_or_else(|| format!("\"{path}\" was not found in the game content or the project folders"))?
    };
    if !hp_colorgrade::is_raw(&bytes) {
        return Err(format!(
            "{path} is {} bytes: a color lookup table is exactly {} bytes (32x32x32 RGB)",
            bytes.len(),
            hp_colorgrade::RAW_BYTES
        ));
    }
    Ok(bytes)
}

fn build_table(state: &AppState, a: &Args<'_>) -> R<(Vec<u8>, Value)> {
    let preset = a.opt_enum("preset", PRESET_IDS)?;
    let grade_value = a.get("grade");
    if grade_value.is_some_and(|v| !v.is_object()) {
        return Err("grade must be an object".to_owned());
    }
    if preset.is_some() && grade_value.is_some() {
        return Err("give preset or grade, not both".to_owned());
    }
    let grade = match (&preset, grade_value) {
        (Some(id), _) => hp_colorgrade::preset(id).ok_or_else(|| format!("unknown preset {id}"))?,
        (None, Some(v)) => serde_json::from_value::<Grade>(v.clone()).map_err(|e| format!("grade: {e}"))?,
        (None, None) => Grade::default(),
    };
    let base = match a.opt_str("base", MAX_PATH)? {
        Some(p) => Some(read_raw(state, p)?),
        None => None,
    };
    let lut = hp_colorgrade::generate(&grade, base.as_deref());
    Ok((lut, grade_json(&grade.clamped())))
}

fn stats_json(lut: &[u8]) -> Value {
    match hp_colorgrade::stats(lut) {
        Some(s) => json!({
            "identity": s.identity,
            "average_rgb": s.average.map(|v| (v * 10.0).round() / 10.0),
            "mean_shift": (s.mean_shift * 100.0).round() / 100.0,
            "max_shift": s.max_shift,
        }),
        None => Value::Null,
    }
}

fn color_correction_generate(state: &mut AppState, a: &Args<'_>) -> R<ToolResult> {
    let (lut, grade) = build_table(state, a)?;
    let folder = a.opt_str("folder", MAX_PATH)?.unwrap_or(hp_colorgrade::DEFAULT_FOLDER).trim_matches('/').to_owned();
    let rel = media::clean_rel(a.str("name", MAX_PATH)?, &folder, "raw")?;
    let root = media::resolve_root(state, a.opt_str("root", MAX_PATH)?)?;
    let target = root.join(&rel);
    let overwrite = a.opt_bool("overwrite")?.unwrap_or(false);
    let mut result = json!({
        "output": target.display().to_string(),
        "reference": rel,
        "keyvalues": { "filename": rel },
        "bytes": lut.len(),
        "grade": grade,
        "table": stats_json(&lut),
        "exists": target.exists(),
        "written": false,
    });
    if a.opt_bool("dry_run")?.unwrap_or(false) {
        result["dry_run"] = json!(true);
        return Ok(ToolResult::json(result));
    }
    media::write_file(&root, &rel, &lut, overwrite)?;
    hp_ui_entities::model_import::remount_after_write(state);
    state.runtime.lab.sound_color.lut_serial += 1;
    result["written"] = json!(true);
    Ok(ToolResult::json(result))
}

fn color_correction_inspect(state: &mut AppState, a: &Args<'_>) -> R<ToolResult> {
    let path = a.str("path", MAX_PATH)?;
    let lut = read_raw(state, path)?;
    let mut out = stats_json(&lut);
    out["path"] = json!(path);
    out["bytes"] = json!(lut.len());
    out["valid"] = json!(true);
    Ok(ToolResult::json(out))
}

fn color_correction_preview(state: &mut AppState, a: &Args<'_>) -> R<ToolResult> {
    if a.opt_bool("off")?.unwrap_or(false) {
        state.runtime.lab.sound_color.set_preview(None, "");
        return Ok(ToolResult::json(json!({ "previewing": false })));
    }
    let lut = match a.opt_str("path", MAX_PATH)? {
        Some(p) => {
            if a.get("preset").is_some() || a.get("grade").is_some() || a.get("base").is_some() {
                return Err("path cannot be combined with preset, grade or base".to_owned());
            }
            read_raw(state, p)?
        }
        None => build_table(state, a)?.0,
    };
    let rgba = hp_colorgrade::to_rgba(&lut).ok_or("the table is not 98304 bytes")?;
    let label = a.opt_str("preset", 64)?.or(a.opt_str("path", MAX_PATH)?).unwrap_or("grade").to_owned();
    state.runtime.lab.sound_color.set_preview(Some(rgba), &label);
    Ok(ToolResult::json(json!({
        "previewing": true,
        "label": label,
        "table": stats_json(&lut),
        "note": "Applied to every 3D view until color_correction_preview with off true. The Hammer++ map color correction is a separate layer.",
    })))
}

fn color_correction_zones(state: &mut AppState, _a: &Args<'_>) -> R<ToolResult> {
    let eye = state.cameras.cam3d.pos.to_array();
    let zones = hp_ui_lab::color_zone::collect_zones(state);
    let rt = &state.runtime.lab.sound_color;
    let list: Vec<Value> = zones
        .iter()
        .map(|(z, class)| {
            let found = !z.filename.is_empty() && media::read_lut(state, &z.filename).is_some();
            let live = rt.zones.iter().find(|r| r.id == z.id);
            let falloff = z.falloff(eye);
            let target = if z.active(eye) { z.max_weight * falloff } else { 0.0 };
            let status = match live {
                Some(r) => r.status.clone(),
                None if z.filename.is_empty() => "no_file".to_owned(),
                None if !found => "file_not_found".to_owned(),
                None if !z.enabled => "disabled".to_owned(),
                None if !z.inside(eye) => "outside_volume".to_owned(),
                None if target <= 0.0 => "out_of_range".to_owned(),
                None => "active".to_owned(),
            };
            json!({
                "id": z.id,
                "class": class,
                "targetname": z.name,
                "filename": z.filename,
                "file_found": found,
                "enabled": z.enabled,
                "exclusive": z.exclusive,
                "min_falloff": z.min_falloff,
                "max_falloff": z.max_falloff,
                "max_weight": z.max_weight,
                "fade_in": z.fade_in,
                "fade_out": z.fade_out,
                "distance": (z.distance(eye) * 100.0).round() / 100.0,
                "falloff": falloff,
                "weight": live.map_or(target, |r| r.weight),
                "status": status,
            })
        })
        .collect();
    Ok(ToolResult::json(json!({
        "preview_enabled": state.settings.lab.color_correction_preview
            || state.settings.view.render_mode == hp_app::settings::RenderModeSetting::ColorCorrected,
        "editor_preview": rt.preview_lut.is_some(),
        "applied": rt.zone_applied,
        "camera": eye,
        "total": list.len(),
        "zones": list,
    })))
}

fn open_media_editor(state: &mut AppState, a: &Args<'_>) -> R<ToolResult> {
    let editor = a.opt_enum("editor", &["sound", "color"])?.ok_or("editor is required")?;
    let path = a.opt_str("path", MAX_PATH)?.map(str::to_owned);
    let rt = &mut state.runtime.lab.sound_color;
    if editor == "sound" {
        rt.sound_open = true;
        rt.open_sound = path;
    } else {
        rt.color_open = true;
        rt.open_lut = path;
    }
    Ok(ToolResult::json(json!({ "opened": editor })))
}

#[cfg(test)]
mod tests {
    use super::*;
    use serde_json::json;

    fn run_one(state: &mut AppState, name: &str, v: Value) -> hp_mcp::protocol::ToolResult {
        super::super::semantic_tests::call(state, name, v)
    }

    fn scratch(name: &str) -> std::path::PathBuf {
        let dir = std::path::PathBuf::from("E:/hp-scratch/sndcc").join(format!("{name}-{}", std::process::id()));
        let _ = std::fs::remove_dir_all(&dir);
        std::fs::create_dir_all(&dir).unwrap();
        dir
    }

    fn wav(frames: usize) -> Vec<u8> {
        let s = Sound {
            rate: 48_000,
            channels: 2,
            samples: (0..frames * 2).map(|i| (i as f32 * 0.01).sin() * 0.4).collect(),
            looped: false,
            loop_start: 0,
        };
        hp_audio::encode_wav(&s)
    }

    #[test]
    fn sound_round_trip() {
        let dir = scratch("sound");
        let src = dir.join("in.wav");
        std::fs::write(&src, wav(48_000)).unwrap();
        let mut state = hp_app::testing::test_state();
        let src_s = src.display().to_string();
        let info = run_one(&mut state, "sound_info", json!({ "path": src_s, "bins": 8 }));
        let v = info.structured.unwrap();
        assert_eq!(v["sample_rate"], 48_000);
        assert_eq!(v["source_friendly"], false);
        assert_eq!(v["waveform"].as_array().unwrap().len(), 8);
        let out = run_one(
            &mut state,
            "sound_edit",
            json!({
                "path": src_s, "root": dir.display().to_string(), "output": "sound/test/out",
                "start": 0.25, "length": 0.5, "gain_db": -3, "looped": true, "mono": true, "sample_rate": 22050,
            }),
        );
        assert!(!out.is_error, "{}", out.first_text());
        let file = dir.join("sound/test/out.wav");
        let back = hp_audio::decode(&std::fs::read(&file).unwrap(), "wav").unwrap();
        assert_eq!((back.rate, back.channels, back.looped), (22_050, 1, true));
        assert!((back.duration() - 0.5).abs() < 0.01);
        let again = run_one(
            &mut state,
            "sound_edit",
            json!({ "path": src_s, "root": dir.display().to_string(), "output": "sound/test/out" }),
        );
        assert!(again.is_error);
        let dry = run_one(
            &mut state,
            "sound_edit",
            json!({ "path": src_s, "root": dir.display().to_string(), "output": "sound/test/out", "dry_run": true }),
        );
        assert!(!dry.is_error);
        assert!(
            run_one(
                &mut state,
                "sound_edit",
                json!({ "path": src_s, "gain_db": 1, "normalize_db": -1, "root": dir.display().to_string() })
            )
            .is_error
        );
        let _ = std::fs::remove_dir_all(&dir);
    }

    #[test]
    fn color_tables() {
        let dir = scratch("color");
        let mut state = hp_app::testing::test_state();
        let root = dir.display().to_string();
        let made = run_one(
            &mut state,
            "color_correction_generate",
            json!({ "name": "cc_test", "root": root, "preset": "sepia" }),
        );
        assert!(!made.is_error, "{}", made.first_text());
        let v = made.structured.unwrap();
        assert_eq!(v["reference"], "scripts/colorcorrection/cc_test.raw");
        let file = dir.join("scripts/colorcorrection/cc_test.raw");
        assert_eq!(std::fs::metadata(&file).unwrap().len(), 98_304);
        let inspected = run_one(&mut state, "color_correction_inspect", json!({ "path": file.display().to_string() }));
        assert_eq!(inspected.structured.unwrap()["identity"], false);
        let both = run_one(
            &mut state,
            "color_correction_generate",
            json!({ "name": "x", "root": root, "preset": "sepia", "grade": { "gamma": 1.2 } }),
        );
        assert!(both.is_error);
        let typo = run_one(
            &mut state,
            "color_correction_generate",
            json!({ "name": "x", "root": root, "grade": { "satration": 0.5 } }),
        );
        assert!(typo.is_error);
        let custom = run_one(
            &mut state,
            "color_correction_generate",
            json!({
                "name": "materials/correction/cc_curve", "folder": "materials/correction", "root": root,
                "base": file.display().to_string(),
                "grade": { "saturation": 0.5, "curves": [{ "points": [[0, 0], [0.5, 0.6], [1, 1]] }, {}, {}, {}] },
            }),
        );
        assert!(!custom.is_error, "{}", custom.first_text());
        assert!(dir.join("materials/correction/cc_curve.raw").is_file());
        let bad = dir.join("short.raw");
        std::fs::write(&bad, [1u8, 2, 3]).unwrap();
        assert!(run_one(&mut state, "color_correction_inspect", json!({ "path": bad.display().to_string() })).is_error);
        let on = run_one(&mut state, "color_correction_preview", json!({ "preset": "night" }));
        assert!(!on.is_error);
        assert!(state.runtime.lab.sound_color.preview_lut.is_some());
        run_one(&mut state, "color_correction_preview", json!({ "off": true }));
        assert!(state.runtime.lab.sound_color.preview_lut.is_none());
        let zones = run_one(&mut state, "color_correction_zones", json!({}));
        assert_eq!(zones.structured.unwrap()["total"], 0);
        let presets = run_one(&mut state, "color_correction_presets", json!({}));
        assert_eq!(presets.structured.unwrap()["presets"].as_array().unwrap().len(), PRESET_IDS.len());
        let _ = std::fs::remove_dir_all(&dir);
    }
}
