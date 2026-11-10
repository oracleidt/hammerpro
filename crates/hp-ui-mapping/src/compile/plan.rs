use std::collections::HashSet;
use std::path::{Path, PathBuf};

use hp_app::settings::{
    BspModeSetting, CompilePresetSetting, CompileSettings, LightingSetting, PackPolicy, RadModeSetting, VisModeSetting,
};
use hp_app::{t, tf};
use hp_compile::{
    BspMode, CompilePlan, DeferredFiles, LaunchMethod, Lighting, PlanOptions, Preset, RadMode, Severity as LogSeverity,
    StepModes, ToolPaths, VisMode,
};

use crate::util::plural;

pub(crate) fn preset_of(p: CompilePresetSetting) -> Preset {
    match p {
        CompilePresetSetting::Draft => Preset::Draft,
        CompilePresetSetting::Normal => Preset::Normal,
        CompilePresetSetting::Final => Preset::Final,
        CompilePresetSetting::Custom => Preset::Custom,
    }
}

pub(crate) fn lighting_of(l: LightingSetting) -> Lighting {
    match l {
        LightingSetting::Ldr => Lighting::Ldr,
        LightingSetting::Hdr => Lighting::Hdr,
        LightingSetting::Both => Lighting::Both,
    }
}

pub(crate) fn custom_modes(s: &CompileSettings) -> StepModes {
    StepModes {
        bsp: match s.bsp_mode {
            BspModeSetting::Normal => BspMode::Full,
            BspModeSetting::OnlyEnts => BspMode::OnlyEnts,
            BspModeSetting::OnlyProps => BspMode::OnlyProps,
        },
        vis: match s.vis_mode {
            VisModeSetting::Fast => VisMode::Fast,
            VisModeSetting::Normal => VisMode::Full,
        },
        rad: match s.rad_mode {
            RadModeSetting::Draft => RadMode::Draft,
            RadModeSetting::Normal => RadMode::Normal,
            RadModeSetting::Final => RadMode::Final,
        },
    }
}

pub(crate) fn set_custom_modes(s: &mut CompileSettings, m: StepModes) {
    s.run_bsp = m.bsp != BspMode::Off;
    s.run_vis = m.vis != VisMode::Off;
    s.run_rad = m.rad != RadMode::Off;
    match m.bsp {
        BspMode::Full => s.bsp_mode = BspModeSetting::Normal,
        BspMode::OnlyEnts | BspMode::EntsAndProps => s.bsp_mode = BspModeSetting::OnlyEnts,
        BspMode::OnlyProps => s.bsp_mode = BspModeSetting::OnlyProps,
        BspMode::Off => {}
    }
    match m.vis {
        VisMode::Fast => s.vis_mode = VisModeSetting::Fast,
        VisMode::Full => s.vis_mode = VisModeSetting::Normal,
        VisMode::Off => {}
    }
    match m.rad {
        RadMode::Draft => s.rad_mode = RadModeSetting::Draft,
        RadMode::Normal => s.rad_mode = RadModeSetting::Normal,
        RadMode::Final => s.rad_mode = RadModeSetting::Final,
        RadMode::Off => {}
    }
}

pub(crate) fn effective_modes(s: &CompileSettings) -> StepModes {
    let base = if s.preset == CompilePresetSetting::Custom { custom_modes(s) } else { preset_of(s.preset).modes() };
    StepModes {
        bsp: if s.run_bsp { base.bsp } else { BspMode::Off },
        vis: if s.run_vis { base.vis } else { VisMode::Off },
        rad: if s.run_rad { base.rad } else { RadMode::Off },
    }
}

pub(crate) fn apply_preset(s: &mut CompileSettings, p: CompilePresetSetting) {
    s.preset = p;
    if p != CompilePresetSetting::Custom {
        set_custom_modes(s, preset_of(p).modes());
    }
}

pub(crate) fn is_release_quality(s: &CompileSettings) -> bool {
    effective_modes(s).rad == RadMode::Final
}

pub(crate) fn packs_content(s: &CompileSettings, release: bool) -> bool {
    s.pack_custom_content && (release || s.pack_policy == PackPolicy::Always || is_release_quality(s))
}

pub(crate) fn threads_of(s: &CompileSettings) -> Option<u32> {
    s.threads.filter(|t| *t > 0).or_else(|| hp_compile::flags::auto_threads(s.low_priority))
}

#[derive(Clone, Debug, Default)]
pub(crate) struct PlanExtras {
    pub modes: Option<StepModes>,
    pub deferred_pack: Option<DeferredFiles>,
    pub launch_flags: Vec<&'static str>,
    pub content_args: Vec<String>,
}

pub fn content_args(fs: Option<&hp_fs::GameFs>) -> Vec<String> {
    let Some(fs) = fs else { return Vec::new() };
    let sources: Vec<hp_fs::SourceInfo> = fs.sources().into_iter().filter(|s| s.enabled).collect();
    let mut args = Vec::new();
    if sources.iter().any(|s| s.origin == hp_fs::SourceOrigin::Workshop) {
        args.push("-wsaddons".to_owned());
    }
    if sources.iter().any(|s| s.origin == hp_fs::SourceOrigin::Download) {
        args.push("-serveraddons".to_owned());
    }
    let mut folders: Vec<&Path> = Vec::new();
    for s in &sources {
        if s.origin == hp_fs::SourceOrigin::Extra
            && matches!(s.kind, hp_fs::SourceKind::Dir)
            && !folders.contains(&s.path.as_path())
        {
            folders.push(&s.path);
        }
    }
    for f in folders {
        args.push("-insert_search_path".to_owned());
        args.push(f.to_string_lossy().into_owned());
    }
    args
}

fn steam_url_of(p: &hp_fs::GameProfile) -> String {
    if p.source_mod {
        let game = p.game_dir.display().to_string().replace('\\', "/").replace(' ', "%20");
        format!("steam://rungameid/{}//-game%20%22{game}%22%20+map%20{{map}}", p.app_id)
    } else {
        format!("steam://rungameid/{}//+map%20{{map}}", p.app_id)
    }
}

fn game_lighting(s: &CompileSettings, profile: Option<&hp_fs::GameProfile>) -> Lighting {
    let lighting = lighting_of(s.lighting);
    if profile.is_some_and(|p| p.lighting == hp_fs::LightingPasses::HdrOnly) && lighting == Lighting::Ldr {
        return Lighting::Hdr;
    }
    lighting
}

pub(crate) fn effective_lighting(s: &CompileSettings) -> Lighting {
    let target = s.target.as_deref().filter(|t| !t.eq_ignore_ascii_case(hp_fs::GMOD_ID));
    let profile = target.and_then(|t| hp_fs::resolve_game(t, s.target_dir.as_deref()));
    game_lighting(s, profile.as_ref())
}

pub(crate) fn plan_options(
    s: &CompileSettings,
    game_folder: &str,
    pack_files: Vec<(String, PathBuf)>,
    compile_game_dir: Option<PathBuf>,
) -> PlanOptions {
    let target = s.target.clone().filter(|t| !t.eq_ignore_ascii_case(hp_fs::GMOD_ID));
    let profile = target.as_deref().and_then(|t| hp_fs::resolve_game(t, s.target_dir.as_deref()));
    let installed = profile.as_ref().is_none_or(|p| p.installed);
    let lighting = game_lighting(s, profile.as_ref());
    PlanOptions {
        lighting,
        threads: threads_of(s),
        extra_bsp_args: s.extra_bsp_args.clone(),
        extra_rad_args: s.extra_rad_args.clone(),
        pack_files: if s.pack_custom_content { pack_files } else { Vec::new() },
        deferred_pack: None,
        copy_to_maps: s.copy_to_maps && installed,
        launch_game: s.launch_game && installed,
        launch_method: if s.launch_via_steam { LaunchMethod::SteamUrl } else { LaunchMethod::Exe },
        game_args: hp_compile::launch::game_args_for(&s.game_args, game_folder),
        steam_url: profile.as_ref().map_or_else(|| hp_compile::DEFAULT_STEAM_URL.to_owned(), steam_url_of),
        compile_game_dir,
        content_args: Vec::new(),
        modes: Some(effective_modes(s)),
        stop_on_leak: s.stop_on_leak,
        bsp_flags: super::options_ui::flag_choices(&s.bsp_flag_choices),
        rad_flags: super::options_ui::flag_choices(&s.rad_flag_choices),
        static_prop_lighting: s.static_prop_lighting,
        static_prop_shadows: s.static_prop_shadows,
        target,
    }
}

#[cfg(test)]
pub(crate) fn build_plan(
    s: &CompileSettings,
    tools: &ToolPaths,
    vmf: &Path,
    pack_files: Vec<(String, PathBuf)>,
    compile_game_dir: Option<PathBuf>,
) -> Result<CompilePlan, hp_compile::CompileError> {
    build_plan_with(s, tools, vmf, pack_files, compile_game_dir, PlanExtras::default())
}

pub(crate) fn build_plan_with(
    s: &CompileSettings,
    tools: &ToolPaths,
    vmf: &Path,
    pack_files: Vec<(String, PathBuf)>,
    compile_game_dir: Option<PathBuf>,
    extras: PlanExtras,
) -> Result<CompilePlan, hp_compile::CompileError> {
    let mut folder = tools.game_dir.file_name().map(|n| n.to_string_lossy().into_owned()).unwrap_or_default();
    if let Some(p) = s
        .target
        .as_deref()
        .and_then(|t| hp_fs::resolve_game(t, s.target_dir.as_deref()))
        .filter(|p| p.installed && p.source_mod)
    {
        folder = p.game_arg();
    }
    let mut opts = plan_options(s, &folder, pack_files, compile_game_dir);
    opts.game_args = hp_compile::launch::game_args_with(&s.game_args, &folder, &extras.launch_flags);
    if let Some(m) = extras.modes {
        opts.modes = Some(m);
    }
    opts.deferred_pack = extras.deferred_pack;
    opts.content_args = extras.content_args;
    CompilePlan::preset(preset_of(s.preset), tools, vmf, &opts)
}

pub(crate) struct PrepInput {
    pub vmf: PathBuf,
    pub settings: CompileSettings,
    pub gmod_root: Option<PathBuf>,
    pub fs: Option<hp_fs::GameFs>,
    pub materials: Vec<String>,
    pub models: Vec<String>,
    pub roots: Vec<hp_fs::deps::DepRoot>,
    pub cache_dir: PathBuf,
    pub content_dir: PathBuf,
}

pub(crate) struct PrepOutput {
    pub result: Result<(CompilePlan, PathBuf), String>,
    pub messages: Vec<(LogSeverity, String)>,
}

pub(crate) fn stage_custom_models(
    fs: &hp_fs::GameFs,
    models: &[String],
    content_dir: &Path,
) -> Result<Vec<PathBuf>, String> {
    const PARTS: [&str; 7] = [".mdl", ".vvd", ".dx90.vtx", ".dx80.vtx", ".sw.vtx", ".phy", ".ani"];
    let custom = |vpath: &str| {
        fs.which_source(vpath).and_then(|id| fs.source(id)).is_some_and(|s| s.origin == hp_fs::SourceOrigin::Custom)
    };
    let mut out = Vec::new();
    for model in models {
        let model = model.trim().replace('\\', "/").to_ascii_lowercase();
        let Some(base) = model.strip_suffix(".mdl").filter(|b| !b.contains("..") && custom(&model)) else { continue };
        for part in PARTS {
            let vpath = format!("{base}{part}");
            let Some(bytes) = fs.read(&vpath) else { continue };
            let dest = content_dir.join(&vpath);
            if std::fs::read(&dest).ok().as_deref() != Some(bytes.as_slice()) {
                if let Some(dir) = dest.parent() {
                    std::fs::create_dir_all(dir).map_err(|e| format!("{}: {e}", dir.display()))?;
                }
                std::fs::write(&dest, &bytes).map_err(|e| format!("{}: {e}", dest.display()))?;
            }
            out.push(dest);
        }
    }
    Ok(out)
}

pub(crate) fn stage_for_compilers(
    s: &CompileSettings,
    fs: Option<&hp_fs::GameFs>,
    tools: &ToolPaths,
    (materials, models): (&[String], &[String]),
    content_dir: &Path,
    messages: &mut Vec<(LogSeverity, String)>,
) -> (Option<PathBuf>, Vec<PathBuf>) {
    let mut staged_files = Vec::new();
    let mut compile_game_dir = None;
    if s.export_custom_materials {
        match fs {
            Some(fs) => match fs.export_custom_materials(materials, &tools.game_dir) {
                Ok(report) => {
                    messages.push((
                        LogSeverity::Info,
                        tf!("compile.exported", report.written.len(), report.up_to_date.len()),
                    ));
                    staged_files.extend(report.written.iter().chain(&report.up_to_date).cloned());
                    for f in report.failed {
                        messages.push((LogSeverity::Warning, tf!("compile.export_failed", f.path, f.error)));
                    }
                }
                Err(e) => messages.push((LogSeverity::Warning, tf!("compile.export_error", e))),
            },
            None => messages.push((LogSeverity::Warning, t!("compile.no_content").to_owned())),
        }
    } else if let Some(fs) = fs {
        let staged = stage_custom_materials(&tools.game_dir, content_dir, |dir| {
            fs.export_custom_materials(materials, dir).map_err(|e| e.to_string())
        });
        match staged {
            Ok(Some(report)) => {
                let n = report.materials;
                messages.push((LogSeverity::Info, tf!(plural(n, "compile.staged_one", "compile.staged"), n)));
                for (path, error) in report.failed {
                    messages.push((LogSeverity::Warning, tf!("compile.export_failed", path, error)));
                }
                staged_files = report.files;
                compile_game_dir = Some(content_dir.to_path_buf());
            }
            Ok(None) => {}
            Err(e) => messages.push((LogSeverity::Warning, tf!("compile.stage_failed", e))),
        }
    }
    if compile_game_dir.is_none() && tools.game_dir.join("gameinfo.txt").is_file() {
        let dir = content_dir.to_path_buf();
        remove_stale_files(&dir.join("materials"), &HashSet::new());
        match write_game_folder(&tools.game_dir, &dir) {
            Ok(()) => compile_game_dir = Some(dir),
            Err(e) => messages.push((LogSeverity::Warning, tf!("compile.stage_failed", e))),
        }
    }
    if let (Some(fs), Some(dir)) = (fs, compile_game_dir.as_deref()) {
        match stage_custom_models(fs, models, dir) {
            Ok(files) => staged_files.extend(files),
            Err(e) => messages.push((LogSeverity::Warning, tf!("compile.stage_failed", e))),
        }
    }
    (compile_game_dir, staged_files)
}

pub(crate) fn prepare(input: PrepInput) -> PrepOutput {
    let mut messages: Vec<(LogSeverity, String)> = Vec::new();
    let fail = |messages: Vec<(LogSeverity, String)>, e: String| PrepOutput { result: Err(e), messages };

    let Some(root) = input.gmod_root.clone().or_else(hp_fs::detect_gmod_root) else {
        return fail(messages, t!("compile.err_no_game").to_owned());
    };
    let resolved = match super::tools::resolve(&root, &input.settings) {
        Ok(r) => r,
        Err(e) => return fail(messages, e),
    };
    messages.extend(resolved.messages);
    let tools = resolved.tools;
    let missing = tools.missing_files();
    if !missing.is_empty() {
        let list: Vec<String> = missing.iter().map(|p| p.display().to_string()).collect();
        return fail(messages, tf!("compile.err_missing_files", list.join(", ")));
    }
    messages.push((LogSeverity::Info, super::tools::tools_text(&tools)));

    let s = &input.settings;
    let (compile_game_dir, _) = stage_for_compilers(
        s,
        input.fs.as_ref(),
        &tools,
        (&input.materials, &input.models),
        &input.content_dir,
        &mut messages,
    );
    if let Some(fs) = &input.fs {
        messages.extend(resized_image_messages(fs, &input.materials));
    }
    let mut pack_files = Vec::new();
    if s.pack_custom_content {
        match &input.fs {
            Some(fs) => {
                let map = input.vmf.file_stem().and_then(|n| n.to_str()).unwrap_or_default();
                pack_files = fs.pack_list_with(&input.materials, &input.models, &input.roots, map, &input.cache_dir);
                let n = pack_files.len();
                messages.push((LogSeverity::Info, tf!(plural(n, "compile.pack_count_one", "compile.pack_count"), n)));
            }
            None => messages.push((LogSeverity::Warning, t!("compile.no_content").to_owned())),
        }
    }

    let external = match super::compilers::choice(s, &tools) {
        super::compilers::Choice::Builtin => None,
        super::compilers::Choice::External(p) => Some(p),
        super::compilers::Choice::Missing(id) => {
            messages.push((LogSeverity::Warning, tf!("compilers.missing_choice", id)));
            None
        }
    };
    let extras = PlanExtras { content_args: content_args(input.fs.as_ref()), ..PlanExtras::default() };
    match build_plan_with(s, &tools, &input.vmf, pack_files, compile_game_dir, extras) {
        Ok(mut plan) => {
            if plan.enabled_steps().next().is_none() {
                return fail(messages, t!("compile.err_no_steps").to_owned());
            }
            if let Some(profile) = &external {
                match super::compilers::fix_instances_in_file(&plan.vmf_path) {
                    Ok(0) => {}
                    Ok(n) => messages.push((LogSeverity::Info, tf!("compilers.instances_fixed", n))),
                    Err(e) => return fail(messages, e),
                }
                plan = match super::compilers::external_plan(plan, profile, s, &tools, effective_lighting(s)) {
                    Ok(p) => p,
                    Err(e) => return fail(messages, e),
                };
                messages.push((LogSeverity::Info, tf!("compilers.running", super::compilers::display_name(profile))));
            }
            let workdir = work_dir(&plan.vmf_path);
            PrepOutput { result: Ok((plan, workdir)), messages }
        }
        Err(e) => fail(messages, super::notices::plan_error_text(&e)),
    }
}

pub(crate) fn work_dir(vmf: &Path) -> PathBuf {
    let abs = std::path::absolute(vmf).unwrap_or_else(|_| vmf.to_path_buf());
    abs.parent().filter(|p| !p.as_os_str().is_empty()).map_or_else(|| PathBuf::from("."), Path::to_path_buf)
}

pub(crate) fn resized_image_messages(fs: &hp_fs::GameFs, materials: &[String]) -> Vec<(LogSeverity, String)> {
    let mut resized = fs.resized_custom_images(materials);
    resized.sort_unstable_by(|a, b| a.texture.cmp(&b.texture));
    resized.dedup_by(|a, b| a.texture == b.texture);
    resized
        .into_iter()
        .map(|r| {
            let (w, h) = r.size;
            let (gw, gh) = r.game_size;
            (LogSeverity::Warning, tf!("compile.resized_image", r.texture, w, h, gw, gh))
        })
        .collect()
}

#[derive(Debug, Default)]
pub(crate) struct StageReport {
    pub materials: usize,
    pub files: Vec<PathBuf>,
    pub failed: Vec<(String, String)>,
}

const GAME_FOLDER_FILES: &[&str] = &["lights.rad", "detail.vbsp", "cfg/mount.cfg", "cfg/mountdepots.txt"];

pub(crate) fn stage_custom_materials(
    game_dir: &Path,
    content_dir: &Path,
    export: impl FnOnce(&Path) -> Result<hp_fs::ExportReport, String>,
) -> Result<Option<StageReport>, String> {
    std::fs::create_dir_all(content_dir).map_err(|e| format!("{}: {e}", content_dir.display()))?;
    let report = export(content_dir)?;
    let keep: HashSet<PathBuf> =
        report.written.iter().chain(&report.up_to_date).map(|p| normalize_for_compare(p)).collect();
    remove_stale_files(&content_dir.join("materials"), &keep);
    let materials = report
        .written
        .iter()
        .chain(&report.up_to_date)
        .filter(|p| p.extension().is_some_and(|e| e.eq_ignore_ascii_case("vmt")))
        .count();
    let failed: Vec<(String, String)> = report.failed.into_iter().map(|f| (f.path, f.error)).collect();
    if keep.is_empty() {
        return Ok(None);
    }
    write_game_folder(game_dir, content_dir)?;
    let files = report.written.iter().chain(&report.up_to_date).cloned().collect();
    Ok(Some(StageReport { materials, files, failed }))
}

pub(crate) fn write_game_folder(game_dir: &Path, content_dir: &Path) -> Result<(), String> {
    std::fs::create_dir_all(content_dir).map_err(|e| format!("{}: {e}", content_dir.display()))?;
    let gameinfo_path = game_dir.join("gameinfo.txt");
    let original = std::fs::read_to_string(&gameinfo_path).map_err(|e| format!("{}: {e}", gameinfo_path.display()))?;
    let generated = compile_gameinfo(&original, game_dir, content_dir)
        .ok_or_else(|| format!("{}: no SearchPaths block", gameinfo_path.display()))?;
    let target = content_dir.join("gameinfo.txt");
    std::fs::write(&target, generated).map_err(|e| format!("{}: {e}", target.display()))?;
    for rel in GAME_FOLDER_FILES {
        let from = game_dir.join(rel);
        let to = content_dir.join(rel);
        if !from.is_file() {
            let _ = std::fs::remove_file(&to);
            continue;
        }
        if let Some(parent) = to.parent() {
            let _ = std::fs::create_dir_all(parent);
        }
        std::fs::copy(&from, &to).map_err(|e| format!("{}: {e}", to.display()))?;
    }
    Ok(())
}

fn normalize_for_compare(p: &Path) -> PathBuf {
    let s = p.to_string_lossy().replace('\\', "/");
    PathBuf::from(if cfg!(windows) { s.to_lowercase() } else { s })
}

fn remove_stale_files(dir: &Path, keep: &HashSet<PathBuf>) {
    let Ok(entries) = std::fs::read_dir(dir) else { return };
    for entry in entries.flatten() {
        let path = entry.path();
        if path.is_dir() {
            remove_stale_files(&path, keep);
            let _ = std::fs::remove_dir(&path);
        } else if !keep.contains(&normalize_for_compare(&path)) {
            let _ = std::fs::remove_file(&path);
        }
    }
}

fn gameinfo_path_text(p: &Path) -> String {
    let abs = std::path::absolute(p).unwrap_or_else(|_| p.to_path_buf());
    abs.to_string_lossy().replace('\\', "/").trim_end_matches('/').to_owned()
}

pub(crate) fn compile_gameinfo(original: &str, game_dir: &Path, content_dir: &Path) -> Option<String> {
    use std::fmt::Write as _;
    const TOKEN: &str = "|gameinfo_path|";
    let game = gameinfo_path_text(game_dir);
    let content = gameinfo_path_text(content_dir);
    let lower = original.to_ascii_lowercase();
    let block = lower.find("searchpaths")?;
    let open = block + lower.get(block..)?.find('{')?;
    let mut out = String::with_capacity(original.len() + 256);
    out.push_str(original.get(..=open)?);
    out.push_str("\n\t\t\t// Added by Hammer Pro: the custom materials used by the map\n");
    let _ = writeln!(out, "\t\t\tgame\t\"{content}\"");
    for line in original.get(open + 1..)?.split_inclusive('\n') {
        if !line.to_ascii_lowercase().contains(TOKEN) {
            out.push_str(line);
            continue;
        }
        let body = line.trim_end_matches(['\r', '\n']);
        let ending = &line[body.len()..];
        let code = body.find("//").map_or(body, |c| &body[..c]);
        let trimmed = code.trim_start();
        let indent = &code[..code.len() - trimmed.len()];
        let Some(split) = trimmed.find(char::is_whitespace) else {
            out.push_str(line);
            continue;
        };
        let (key, value) = (&trimmed[..split], trimmed[split..].trim().trim_matches('"'));
        let at = value.to_ascii_lowercase().find(TOKEN).unwrap_or(0);
        let rest = value.get(at + TOKEN.len()..).unwrap_or_default().trim_start_matches(['/', '\\']);
        let value = match rest {
            "" | "." => game.clone(),
            r => format!("{game}/{r}"),
        };
        let _ = write!(out, "{indent}{key}\t\"{value}\"{ending}");
    }
    Some(out)
}

#[cfg(test)]
mod tests {
    use super::*;
    use hp_compile::{StepAction, StepKind};

    fn tools() -> ToolPaths {
        ToolPaths {
            compiler: PathBuf::from("C:/HammerPro/hammerpro.exe"),
            game_exe: Some(PathBuf::from("C:/Games/GarrysMod/gmod.exe")),
            game_dir: PathBuf::from("C:/Games/GarrysMod/garrysmod"),
            maps_dir: PathBuf::from("C:/Games/GarrysMod/garrysmod/maps"),
        }
    }

    fn vmf() -> PathBuf {
        PathBuf::from("C:/maps/gm_test.vmf")
    }

    fn kinds(plan: &CompilePlan) -> Vec<(StepKind, bool)> {
        plan.steps.iter().map(|s| (s.kind, s.enabled)).collect()
    }

    #[test]
    fn plan_from_default_settings() {
        let s = CompileSettings { launch_game: false, ..CompileSettings::default() };
        let plan = build_plan(&s, &tools(), &vmf(), Vec::new(), None).expect("plan");
        assert_eq!(kinds(&plan), vec![(StepKind::Bsp, true), (StepKind::Rad, true), (StepKind::CopyToMaps, true)]);
        assert!(plan.steps.iter().take(2).all(|st| st.exe == Path::new("C:/HammerPro/hammerpro.exe")));
        let bsp = &plan.steps[0].args;
        assert_eq!(bsp[0], hp_compile::BSPC_ARG);
        assert!(bsp.contains(&"-vis".to_owned()) && bsp.contains(&"-leaktest".to_owned()), "{bsp:?}");
        let rad = &plan.steps[1].args;
        assert_eq!(rad[..2], [hp_compile::RAD_ARG, "-ldr"]);
        assert!(plan.stop_on_leak);
        let cores = hp_compile::flags::logical_cores();
        if cores > 1 {
            assert!(bsp.windows(2).any(|w| w[0] == "-threads" && w[1] == (cores - 1).to_string()), "{bsp:?}");
        }
        let s = CompileSettings { low_priority: false, launch_game: false, ..CompileSettings::default() };
        let plan = build_plan(&s, &tools(), &vmf(), Vec::new(), None).expect("plan");
        assert!(!plan.steps[0].args.contains(&"-threads".to_owned()));
    }

    #[test]
    fn prop_options_and_flag_choices_reach_the_steps() {
        use hp_app::settings::FlagChoiceSetting;
        let rad_of = |s: &CompileSettings| {
            let plan = build_plan(s, &tools(), &vmf(), Vec::new(), None).expect("plan");
            plan.steps.iter().find(|st| st.kind == StepKind::Rad).map(|st| st.args.clone()).unwrap_or_default()
        };
        for preset in [
            CompilePresetSetting::Draft,
            CompilePresetSetting::Normal,
            CompilePresetSetting::Final,
            CompilePresetSetting::Custom,
        ] {
            for (lighting, shadows) in [(true, true), (true, false), (false, true), (false, false)] {
                let mut s = CompileSettings { launch_game: false, ..CompileSettings::default() };
                apply_preset(&mut s, preset);
                s.static_prop_lighting = lighting;
                s.static_prop_shadows = shadows;
                let rad = rad_of(&s);
                let has = |f: &str| rad.iter().any(|a| a == f);
                let fin = preset == CompilePresetSetting::Final;
                assert_eq!(has("-StaticPropLighting"), lighting && fin, "{preset:?} {lighting} {shadows}");
                assert_eq!(has("-nostaticpropshadows"), !shadows, "{preset:?} {lighting} {shadows}");
                assert_eq!(has("-final"), fin);
            }
        }
        let mut s = CompileSettings { launch_game: false, ..CompileSettings::default() };
        apply_preset(&mut s, CompilePresetSetting::Draft);
        s.rad_flag_choices.insert("-fast".into(), FlagChoiceSetting { on: false, value: String::new() });
        s.rad_flag_choices.insert("-bounce".into(), FlagChoiceSetting { on: true, value: "3".into() });
        s.bsp_flag_choices.insert("-nodetail".into(), FlagChoiceSetting { on: true, value: String::new() });
        s.extra_bsp_args = "-nodetail".into();
        let plan = build_plan(&s, &tools(), &vmf(), Vec::new(), None).expect("plan");
        let rad = &plan.steps.iter().find(|st| st.kind == StepKind::Rad).expect("rad").args;
        assert!(!rad.iter().any(|a| a == "-fast"));
        assert!(rad.windows(2).any(|w| w == ["-bounce", "3"]) && rad.iter().filter(|a| *a == "-bounce").count() == 1);
        let bsp = &plan.steps[0].args;
        assert_eq!(bsp.iter().filter(|a| *a == "-nodetail").count(), 1, "{bsp:?}");
        let o = plan_options(&s, "garrysmod", Vec::new(), None);
        assert!(o.static_prop_lighting && o.static_prop_shadows);
        assert_eq!(o.rad_flags.len(), 2);
        assert!(o.bsp_flags.iter().any(|c| c.flag == "-nodetail" && c.on));
    }

    #[test]
    fn the_compile_mounts_the_content_the_editor_shows() {
        let dir = crate::test_scratch("content_args");
        let root = dir.join("GarrysMod");
        let download = root.join("garrysmod/download/materials/rockford");
        let extra = dir.join("css/cstrike");
        for d in [&download, &extra.join("materials")] {
            std::fs::create_dir_all(d).expect("dirs");
        }
        std::fs::write(download.join("road.vmt"), "\"LightmappedGeneric\" { \"$basetexture\" \"rockford/road\" }")
            .expect("vmt");
        assert!(content_args(None).is_empty());
        let opts = hp_fs::MountOptions {
            mount_workshop: false,
            mount_downloads: true,
            extra_game_dirs: vec![extra.clone()],
            ..hp_fs::MountOptions::default()
        };
        let fs = hp_fs::GameFs::for_gmod(&root, &opts).expect("mount");
        assert!(fs.material("rockford/road").is_some(), "the editor shows it");
        let args = content_args(Some(&fs));
        let folder = extra.to_string_lossy().into_owned();
        assert_eq!(args, ["-serveraddons".to_owned(), "-insert_search_path".to_owned(), folder]);
        let s = CompileSettings { launch_game: false, ..CompileSettings::default() };
        let extras = PlanExtras { content_args: args, ..PlanExtras::default() };
        let plan = build_plan_with(&s, &tools(), &vmf(), Vec::new(), None, extras).expect("plan");
        for kind in [StepKind::Bsp, StepKind::Rad] {
            let step = plan.steps.iter().find(|st| st.kind == kind).expect("step");
            assert!(step.args.iter().any(|a| a == "-serveraddons"), "{kind:?} {:?}", step.args);
            let folder = step.args.windows(2).any(|w| w[0] == "-insert_search_path" && Path::new(&w[1]) == extra);
            assert!(folder, "{:?}", step.args);
        }
        for src in fs.sources().iter().filter(|s| s.origin == hp_fs::SourceOrigin::Download) {
            fs.set_source_enabled(src.id, false);
        }
        assert!(!content_args(Some(&fs)).iter().any(|a| a == "-serveraddons"));
        let plain = hp_fs::GameFs::for_gmod(
            &root,
            &hp_fs::MountOptions { mount_workshop: false, ..hp_fs::MountOptions::default() },
        )
        .expect("mount");
        assert!(content_args(Some(&plain)).is_empty());
        let _ = std::fs::remove_dir_all(&dir);
    }

    #[test]
    fn the_compile_mounts_the_steam_games_added_by_app_id() {
        let dir = crate::test_scratch("content_app_ids");
        let root = dir.join("GarrysMod");
        std::fs::create_dir_all(root.join("garrysmod")).expect("gmod");
        let steamapps = dir.join("lib/steamapps");
        let css = steamapps.join("common/Counter-Strike Source/cstrike");
        std::fs::create_dir_all(css.join("materials")).expect("css");
        std::fs::write(
            css.join("gameinfo.txt"),
            "\"GameInfo\" { game \"CSS\" FileSystem { SteamAppId 240 SearchPaths { game |gameinfo_path|. } } }",
        )
        .expect("gameinfo");
        std::fs::write(
            steamapps.join("appmanifest_240.acf"),
            "\"AppState\" { \"appid\" \"240\" \"installdir\" \"Counter-Strike Source\" }",
        )
        .expect("manifest");
        let opts = hp_fs::MountOptions {
            mount_workshop: false,
            app_ids: vec![240],
            steam_libraries: vec![dir.join("lib")],
            ..hp_fs::MountOptions::default()
        };
        let fs = hp_fs::GameFs::for_gmod(&root, &opts).expect("mount");
        let args = content_args(Some(&fs));
        let pos = args.iter().position(|a| a == "-insert_search_path").expect("search path");
        assert_eq!(Path::new(&args[pos + 1]), css.as_path(), "{args:?}");
        let _ = std::fs::remove_dir_all(&dir);
    }

    #[test]
    fn launch_flags_reach_the_game() {
        let s = CompileSettings { launch_game: true, game_args: "-console".into(), ..CompileSettings::default() };
        let extras = PlanExtras { launch_flags: vec!["-noworkshop", "-console"], ..PlanExtras::default() };
        let plan = build_plan_with(&s, &tools(), &vmf(), Vec::new(), None, extras).expect("plan");
        let launch = plan.steps.iter().find(|st| st.kind == StepKind::LaunchGame).expect("launch step");
        let at = |flag: &str| launch.args.iter().position(|a| a == flag);
        assert!(at("-noworkshop").is_some_and(|i| at("-console").is_some_and(|c| c < i)), "{:?}", launch.args);
        assert_eq!(launch.args.iter().filter(|a| *a == "-console").count(), 1, "not added twice");
        assert!(launch.args.windows(2).any(|w| w[0] == "+map" && w[1] == "gm_test"));
        let plan = build_plan(&s, &tools(), &vmf(), Vec::new(), None).expect("plan");
        let launch = plan.steps.iter().find(|st| st.kind == StepKind::LaunchGame).expect("launch step");
        assert!(!launch.args.iter().any(|a| a == "-noworkshop"));
    }

    #[test]
    fn plan_presets_steps_and_options() {
        let s = CompileSettings {
            preset: CompilePresetSetting::Draft,
            lighting: LightingSetting::Hdr,
            run_vis: false,
            threads: Some(4),
            extra_rad_args: "-bounce 2".into(),
            copy_to_maps: false,
            launch_game: true,
            pack_custom_content: true,
            ..CompileSettings::default()
        };
        let pack = vec![("materials/a.vmt".to_owned(), PathBuf::from("C:/c/a.vmt"))];
        let plan = build_plan(&s, &tools(), &vmf(), pack, None).expect("plan");
        assert_eq!(
            kinds(&plan),
            vec![(StepKind::Bsp, true), (StepKind::Rad, true), (StepKind::Pack, true), (StepKind::LaunchGame, true)]
        );
        assert!(!plan.steps[0].args.iter().any(|a| a == "-vis" || a == "-fastvis"));
        let rad = &plan.steps[1].args;
        assert_eq!(&rad[..3], &[hp_compile::RAD_ARG, "-hdr", "-fast"]);
        assert!(rad.windows(2).any(|w| w[0] == "-threads" && w[1] == "4"));
        assert!(rad.windows(2).any(|w| w[0] == "-bounce" && w[1] == "2"), "the user's arguments come last");
        assert_eq!(rad.iter().filter(|a| *a == "-bounce").count(), 1);
        assert!(matches!(&plan.steps[2].action, StepAction::Pack { files } if files.len() == 1));
        assert!(plan.steps[2].exe.as_os_str().is_empty(), "packed in process");
        let launch = &plan.steps[3];
        assert!(launch.args.contains(&"-hijack".to_owned()));
        assert!(launch.args.windows(2).any(|w| w[0] == "+map" && w[1] == "gm_test"));
        assert!(launch.args.windows(2).any(|w| w[0] == "-game" && w[1] == "garrysmod"));

        let s2 = CompileSettings { pack_custom_content: false, launch_game: false, ..s.clone() };
        let plan2 = build_plan(&s2, &tools(), &vmf(), vec![("a".into(), PathBuf::from("b"))], None).expect("plan");
        assert!(plan2.steps.iter().all(|st| st.kind != StepKind::Pack));

        let s3 = CompileSettings { launch_via_steam: true, ..s.clone() };
        let plan3 = build_plan(&s3, &tools(), &vmf(), Vec::new(), None).expect("plan");
        assert!(plan3.steps.last().is_some_and(|st| st.args.iter().any(|a| a.starts_with("steam://"))));

        let s4 = CompileSettings { preset: CompilePresetSetting::Final, ..CompileSettings::default() };
        let plan4 = build_plan(&s4, &tools(), &vmf(), Vec::new(), None).expect("plan");
        assert_eq!(plan4.steps[1].args[..3], [hp_compile::RAD_ARG, "-both", "-final"]);

        assert!(build_plan(&s4, &tools(), Path::new("C:/maps/my map.vmf"), Vec::new(), None).is_err());
    }

    #[test]
    fn preparation_without_game_fails_cleanly() {
        let dir = crate::test_scratch("prep_no_game");
        let input = PrepInput {
            vmf: dir.join("gm_x.vmf"),
            settings: CompileSettings::default(),
            gmod_root: Some(dir.join("no_such_install")),
            fs: None,
            materials: Vec::new(),
            models: Vec::new(),
            roots: Vec::new(),
            cache_dir: dir.join("cache"),
            content_dir: dir.join("content"),
        };
        let out = prepare(input);
        assert!(out.result.is_err());
    }

    const GMOD_GAMEINFO: &str = "\"GameInfo\"\n{\n\tgame\t\"Garry's Mod\"\n\t\"GameData\"\t\t\"garrysmod.fgd\"\n\tFileSystem\n\t{\n\t\tSteamAppId\t\t\t\t4000\n\t\tSearchPaths\n\t\t{\n\t\t\tgame+mod\t\t\tgarrysmod/addons/*\n\t\t\tgame\t\t\t\t|all_source_engine_paths|sourceengine/hl2_textures.vpk\n\t\t\tmod+mod_write+default_write_path\t\t|gameinfo_path|.\n\t\t\tgame+game_write\t\tgarrysmod\n\t\t\tgame\t\"|GameInfo_Path|sub dir\" // comment\n\t\t}\n\t}\n}\n";

    #[test]
    fn generated_gameinfo_adds_the_content_folder() {
        let game = Path::new("D:/Steam Library/GarrysMod/garrysmod");
        let content = Path::new("C:/Cache/compile_content");
        let text = compile_gameinfo(GMOD_GAMEINFO, game, content).expect("gameinfo");
        let lines: Vec<&str> = text.lines().map(str::trim).collect();
        let first = lines.iter().position(|l| l.starts_with("SearchPaths")).expect("block") + 2;
        assert!(lines[first].starts_with("//"));
        let content_line = lines[first + 1];
        assert!(content_line.starts_with("game\t\"") && content_line.ends_with("compile_content\""), "{content_line}");
        assert!(text.contains("mod+mod_write+default_write_path\t\"D:/Steam Library/GarrysMod/garrysmod\""), "{text}");
        assert!(text.contains("game\t\"D:/Steam Library/GarrysMod/garrysmod/sub dir\""), "{text}");
        assert!(!text.to_ascii_lowercase().contains("|gameinfo_path|"));
        assert!(text.contains("game+mod\t\t\tgarrysmod/addons/*"));
        assert!(text.contains("|all_source_engine_paths|sourceengine/hl2_textures.vpk"));
        assert!(text.contains("\"GameData\"\t\t\"garrysmod.fgd\""));
        assert!(compile_gameinfo("\"GameInfo\" { }", game, content).is_none());
    }

    #[test]
    fn custom_materials_are_staged_in_a_game_folder_of_their_own() {
        let dir = crate::test_scratch("stage_custom");
        let game = dir.join("garrysmod");
        std::fs::create_dir_all(game.join("cfg")).expect("game dir");
        std::fs::write(game.join("gameinfo.txt"), GMOD_GAMEINFO).expect("gameinfo");
        std::fs::write(game.join("lights.rad"), "x").expect("lights");
        std::fs::write(game.join("cfg/mount.cfg"), "\"mountcfg\" { }").expect("mount");
        let content = dir.join("content");
        let stale = content.join("materials/old/gone.vmt");
        std::fs::create_dir_all(stale.parent().expect("parent")).expect("stale dir");
        std::fs::write(&stale, "old").expect("stale");
        let export = |dir: &Path| {
            let vmt = dir.join("materials/custom/brick.vmt");
            let vtf = dir.join("materials/custom/brick.vtf");
            std::fs::create_dir_all(vmt.parent().expect("parent")).map_err(|e| e.to_string())?;
            std::fs::write(&vmt, "LightmappedGeneric { }").map_err(|e| e.to_string())?;
            std::fs::write(&vtf, "VTF").map_err(|e| e.to_string())?;
            Ok(hp_fs::ExportReport { written: vec![vmt], up_to_date: vec![vtf], failed: Vec::new() })
        };
        let report = stage_custom_materials(&game, &content, export).expect("staged").expect("custom content");
        assert_eq!(report.materials, 1);
        assert!(content.join("materials/custom/brick.vmt").is_file());
        assert!(!stale.exists(), "stale files are removed");
        assert!(!content.join("materials/old").exists(), "empty folders are removed");
        let gameinfo = std::fs::read_to_string(content.join("gameinfo.txt")).expect("generated gameinfo");
        assert!(gameinfo.contains("compile_content") || gameinfo.contains("/content\""), "{gameinfo}");
        assert!(content.join("lights.rad").is_file());
        assert!(content.join("cfg/mount.cfg").is_file());
        assert!(!content.join("detail.vbsp").exists(), "missing files are not invented");
        let none = stage_custom_materials(&game, &content, |_| Ok(hp_fs::ExportReport::default())).expect("ok");
        assert!(none.is_none());
        assert!(!content.join("materials/custom/brick.vmt").exists(), "unused materials are removed");
        let mut names: Vec<String> = std::fs::read_dir(&game)
            .expect("read")
            .flatten()
            .map(|e| e.file_name().to_string_lossy().into_owned())
            .collect();
        names.sort();
        assert_eq!(names, vec!["cfg", "gameinfo.txt", "lights.rad"]);
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
    fn resized_custom_images_are_logged() {
        let _lang = crate::lang_lock();
        crate::register_translations();
        let dir = crate::test_scratch("resized_images");
        let pics = dir.join("pics");
        std::fs::create_dir_all(&pics).expect("pics dir");
        std::fs::write(pics.join("npot.tga"), tga(100, 60)).expect("npot");
        std::fs::write(pics.join("pow.tga"), tga(64, 32)).expect("pow");
        let fs = hp_fs::GameFs::empty();
        fs.add_custom_dir(hp_fs::CustomDir::with_prefix(&pics, "pics")).expect("custom dir");
        let used: Vec<String> =
            ["pics/pow", "pics/npot", "concrete/concretefloor001a"].iter().map(|s| (*s).to_owned()).collect();
        hp_app::i18n::set_lang(hp_app::i18n::Lang::En);
        let en = resized_image_messages(&fs, &used);
        hp_app::i18n::set_lang(hp_app::i18n::Lang::Fr);
        let fr = resized_image_messages(&fs, &used);
        hp_app::i18n::set_lang(hp_app::i18n::Lang::En);
        assert_eq!(en.len(), 1, "only the image whose sides are not powers of two: {en:?}");
        assert_eq!(en[0].0, LogSeverity::Warning);
        assert_eq!(
            en[0].1,
            "pics/npot: the 100x60 image is resized to 128x64 for the game (sides must be powers of two)"
        );
        assert!(fr[0].1.starts_with("pics/npot : l'image de 100x60 est redimensionnée en 128x64"), "{fr:?}");
        assert!(resized_image_messages(&fs, &["concrete/concretefloor001a".to_owned()]).is_empty());
    }

    #[test]
    fn custom_models_are_staged_for_the_compiler() {
        let dir = crate::test_scratch("custom_models_staged");
        let custom = dir.join("mine");
        std::fs::create_dir_all(custom.join("models/props")).expect("models dir");
        std::fs::create_dir_all(custom.join("materials/props")).expect("materials dir");
        for (name, text) in
            [("crate.mdl", "IDST"), ("crate.vvd", "IDSV"), ("crate.dx90.vtx", "vtx"), ("crate.phy", "phy")]
        {
            std::fs::write(custom.join("models/props").join(name), text).expect("model file");
        }
        std::fs::write(custom.join("materials/props/crate.vmt"), "VertexLitGeneric {}").expect("vmt");
        let fs = hp_fs::GameFs::empty();
        fs.add_custom_dir(hp_fs::CustomDir::new(&custom)).expect("custom dir");
        let content = dir.join("content");
        let used = vec!["models/props/crate.mdl".to_owned(), "models/props_c17/oildrum001.mdl".to_owned()];
        let staged = stage_custom_models(&fs, &used, &content).expect("staged");
        assert_eq!(staged.len(), 4, "{staged:?}");
        assert_eq!(std::fs::read(content.join("models/props/crate.mdl")).expect("mdl"), b"IDST");
        assert_eq!(std::fs::read(content.join("models/props/crate.phy")).expect("phy"), b"phy");
        assert!(!content.join("models/props_c17").exists(), "game models are not copied");
        let again = stage_custom_models(&fs, &used, &content).expect("staged again");
        assert_eq!(again, staged);
    }

    #[test]
    fn relative_map_paths_get_an_absolute_work_folder() {
        let cwd = std::env::current_dir().expect("cwd");
        assert_eq!(work_dir(Path::new("room.vmf")), cwd);
        assert_eq!(work_dir(Path::new("comp/room.vmf")), cwd.join("comp"));
        assert_eq!(work_dir(Path::new("C:/maps/gm_x.vmf")), std::path::absolute("C:/maps").expect("abs"));
        let s = CompileSettings { launch_game: false, ..CompileSettings::default() };
        let plan = build_plan(&s, &tools(), Path::new("room.vmf"), Vec::new(), None).expect("plan");
        assert!(plan.vmf_path.is_absolute());
        assert_eq!(work_dir(&plan.vmf_path), cwd);
    }

    #[test]
    fn plan_uses_the_generated_game_folder_for_the_compilers() {
        let s = CompileSettings { launch_game: false, ..CompileSettings::default() };
        let staged = PathBuf::from("C:/Cache/compile_content");
        let plan = build_plan(&s, &tools(), &vmf(), Vec::new(), Some(staged)).expect("plan");
        let bsp = &plan.steps[0].args;
        let game = bsp.iter().position(|a| a == "-game").map(|i| bsp[i + 1].clone()).unwrap_or_default();
        assert!(game.replace('\\', "/").ends_with("Cache/compile_content"), "{game}");
    }
}

#[cfg(test)]
mod mode_tests {
    use super::*;

    #[test]
    fn presets_and_step_choices() {
        let mut s = CompileSettings::default();
        apply_preset(&mut s, CompilePresetSetting::Draft);
        assert_eq!(effective_modes(&s), Preset::Draft.modes());
        assert_eq!((s.vis_mode, s.rad_mode), (VisModeSetting::Fast, RadModeSetting::Draft));
        s.preset = CompilePresetSetting::Custom;
        s.bsp_mode = BspModeSetting::OnlyEnts;
        s.run_vis = false;
        s.run_rad = false;
        assert_eq!(effective_modes(&s), StepModes::new(BspMode::OnlyEnts, VisMode::Off, RadMode::Off));
        apply_preset(&mut s, CompilePresetSetting::Final);
        assert!(s.run_vis && s.run_rad && is_release_quality(&s));
        assert!(packs_content(&s, false));
        apply_preset(&mut s, CompilePresetSetting::Normal);
        assert!(!packs_content(&s, false), "iteration builds do not pack");
        assert!(packs_content(&s, true), "releases do");
        s.pack_policy = PackPolicy::Always;
        assert!(packs_content(&s, false));
        let old = CompileSettings { run_vis: false, ..CompileSettings::default() };
        assert_eq!(effective_modes(&old).vis, VisMode::Off);
        assert_eq!(threads_of(&CompileSettings::default()), hp_compile::flags::auto_threads(true));
        assert_eq!(threads_of(&CompileSettings { low_priority: false, ..CompileSettings::default() }), None);
        assert_eq!(threads_of(&CompileSettings { threads: Some(3), ..CompileSettings::default() }), Some(3));
    }
}
