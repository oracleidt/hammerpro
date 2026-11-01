use std::path::{Component, Path, PathBuf};

use hp_app::settings::CompileSettings;
use hp_app::settings::compilers::{self, CompilerProfile, CompilerStep, CompilerStepKind};
use hp_app::{t, tf};
use hp_compile::template::{Tokens, expand, expand_args};
use hp_compile::valve::{ValveTool, check_value, valve_flag, valve_flags};
use hp_compile::{BspMode, CompilePlan, Lighting, RadMode, Step, StepKind, ToolPaths, VisMode};

#[derive(Clone, Debug, PartialEq)]
pub(crate) enum Choice {
    Builtin,
    External(CompilerProfile),
    Missing(String),
}

pub(crate) fn detected(tools: &ToolPaths) -> Vec<CompilerProfile> {
    compilers::detect_compilers(&tools.game_dir)
}

pub(crate) fn choice_with(s: &CompileSettings, detected: &[CompilerProfile]) -> Choice {
    let id = s.active_compiler_id();
    if compilers::is_builtin_id(id) {
        return Choice::Builtin;
    }
    let all = compilers::all_compilers(&s.compilers, detected);
    match compilers::find_compiler(&all, id) {
        Some(p) if p.is_builtin() => Choice::Builtin,
        Some(p) => Choice::External(p.clone()),
        None => Choice::Missing(id.to_owned()),
    }
}

pub(crate) fn choice(s: &CompileSettings, tools: &ToolPaths) -> Choice {
    if s.uses_builtin_compiler() {
        return Choice::Builtin;
    }
    choice_with(s, &detected(tools))
}

pub(crate) fn display_name(p: &CompilerProfile) -> String {
    if p.is_builtin() {
        t!("compilers.builtin").to_owned()
    } else if p.is_detected() {
        tf!("compilers.detected_name", p.name)
    } else {
        p.name.clone()
    }
}

pub(crate) fn kind_label(kind: CompilerStepKind) -> &'static str {
    match kind {
        CompilerStepKind::Bsp => t!("compilers.kind.bsp"),
        CompilerStepKind::Vis => t!("compilers.kind.vis"),
        CompilerStepKind::Rad => t!("compilers.kind.rad"),
        CompilerStepKind::Copy => t!("compilers.kind.copy"),
        CompilerStepKind::Game => t!("compilers.kind.game"),
        CompilerStepKind::Custom => t!("compilers.kind.custom"),
    }
}

pub(crate) fn tokens(profile: &CompilerProfile, vmf: &Path, tools: &ToolPaths) -> Tokens {
    let s = |p: &Path| p.to_string_lossy().into_owned();
    let mut tokens = Tokens {
        file: vmf.file_stem().map(|n| n.to_string_lossy().into_owned()).unwrap_or_default(),
        ext: vmf.extension().map_or_else(|| "vmf".to_owned(), |e| e.to_string_lossy().into_owned()),
        path: vmf.parent().map(s).unwrap_or_default(),
        bspdir: s(&tools.maps_dir),
        gamedir: s(&tools.game_dir),
        exedir: tools.game_exe.as_deref().and_then(Path::parent).map(s).unwrap_or_default(),
        game_exe: tools.game_exe.as_deref().map(s).unwrap_or_default(),
        ..Tokens::default()
    };
    let exe_of = |kind: CompilerStepKind, tokens: &Tokens| {
        profile.step(kind).map(|st| expand(st.exe.trim(), tokens)).unwrap_or_default()
    };
    let bsp = exe_of(CompilerStepKind::Bsp, &tokens);
    let vis = exe_of(CompilerStepKind::Vis, &tokens);
    let rad = exe_of(CompilerStepKind::Rad, &tokens);
    tokens.bsp_exe = bsp;
    tokens.vis_exe = vis;
    tokens.light_exe = rad;
    tokens
}

pub(crate) fn find_program(exe: &str) -> Option<PathBuf> {
    let text = exe.trim();
    if text.is_empty() {
        return None;
    }
    let p = PathBuf::from(text);
    if p.is_absolute() {
        return p.is_file().then_some(p);
    }
    if p.components().count() != 1 || !matches!(p.components().next(), Some(Component::Normal(_))) {
        return None;
    }
    let path = std::env::var_os("PATH")?;
    std::env::split_paths(&path).find_map(|dir| {
        let direct = dir.join(&p);
        if direct.is_file() {
            return Some(direct);
        }
        let exe = dir.join(format!("{text}.exe"));
        (cfg!(windows) && p.extension().is_none() && exe.is_file()).then_some(exe)
    })
}

pub(crate) fn mode_flags(
    kind: CompilerStepKind,
    modes: hp_compile::StepModes,
    lighting: Lighting,
    threads: Option<u32>,
) -> Vec<String> {
    let mut out: Vec<String> = Vec::new();
    match kind {
        CompilerStepKind::Bsp => match modes.bsp {
            BspMode::OnlyEnts => out.push("-onlyents".to_owned()),
            BspMode::OnlyProps => out.push("-onlyprops".to_owned()),
            BspMode::EntsAndProps => {
                out.push("-onlyents".to_owned());
                out.push("-onlyprops".to_owned());
            }
            BspMode::Full | BspMode::Off => {}
        },
        CompilerStepKind::Vis => {
            if modes.vis == VisMode::Fast {
                out.push("-fast".to_owned());
            }
        }
        CompilerStepKind::Rad => {
            match modes.rad {
                RadMode::Draft => out.push("-fast".to_owned()),
                RadMode::Final => out.push("-final".to_owned()),
                RadMode::Normal | RadMode::Off => {}
            }
            out.push(lighting.flag().to_owned());
        }
        _ => return out,
    }
    if let Some(n) = threads {
        out.push("-threads".to_owned());
        out.push(n.max(1).to_string());
    }
    out
}

pub(crate) fn valve_tool(kind: CompilerStepKind) -> Option<ValveTool> {
    match kind {
        CompilerStepKind::Bsp => Some(ValveTool::Vbsp),
        CompilerStepKind::Vis => Some(ValveTool::Vvis),
        CompilerStepKind::Rad => Some(ValveTool::Vrad),
        _ => None,
    }
}

pub(crate) fn option_args(
    profile: &CompilerProfile,
    st: &CompilerStep,
    tokens: &Tokens,
) -> Result<Vec<String>, String> {
    let Some(tool) = valve_tool(st.kind) else { return Ok(Vec::new()) };
    let mut out = Vec::new();
    for key in st.options.keys() {
        if valve_flag(tool, key).is_none() {
            return Err(tf!("compilers.err_unknown_option", key));
        }
    }
    for flag in valve_flags(tool) {
        let Some((_, value)) =
            st.options.iter().find(|(k, _)| valve_flag(tool, k).is_some_and(|f| f.flag == flag.flag))
        else {
            continue;
        };
        if profile.mode_flags && flag.managed.is_some() {
            continue;
        }
        let words = check_value(flag, value)?;
        out.push(flag.flag.to_owned());
        out.extend(words.iter().map(|w| expand(w, tokens)));
    }
    Ok(out)
}

fn compile_args(
    profile: &CompilerProfile,
    st: &CompilerStep,
    cx: &Context<'_>,
    tokens: &Tokens,
) -> Result<Vec<String>, String> {
    let mut args = Vec::new();
    if profile.mode_flags && st.kind.is_compile() {
        args.extend(mode_flags(st.kind, cx.modes, cx.lighting, cx.threads));
    }
    args.extend(option_args(profile, st, tokens)?);
    if st.kind.is_compile() {
        args.extend(expand_args(&st.extra, tokens));
    }
    args.extend(expand_args(&st.args, tokens));
    Ok(args)
}

fn step_off(kind: CompilerStepKind, modes: hp_compile::StepModes) -> bool {
    match kind {
        CompilerStepKind::Bsp => modes.bsp == BspMode::Off,
        CompilerStepKind::Vis => modes.vis == VisMode::Off,
        CompilerStepKind::Rad => modes.rad == RadMode::Off,
        _ => false,
    }
}

fn full_path(text: &str) -> Option<PathBuf> {
    let p = PathBuf::from(text.trim());
    (p.is_absolute() && !p.components().any(|c| matches!(c, Component::ParentDir))).then_some(p)
}

pub(crate) struct Context<'a> {
    pub vmf: &'a Path,
    pub tools: &'a ToolPaths,
    pub modes: hp_compile::StepModes,
    pub lighting: Lighting,
    pub threads: Option<u32>,
}

pub(crate) fn profile_steps(profile: &CompilerProfile, cx: &Context<'_>) -> Result<Vec<Step>, String> {
    let name = display_name(profile);
    let tokens = tokens(profile, cx.vmf, cx.tools);
    let mut out = Vec::with_capacity(profile.steps.len());
    for (i, st) in profile.steps.iter().enumerate() {
        let n = i + 1;
        let label = kind_label(st.kind);
        let enabled = st.enabled && !step_off(st.kind, cx.modes);
        let args = match compile_args(profile, st, cx, &tokens) {
            Ok(a) => a,
            Err(e) if enabled => return Err(tf!("compilers.err_option", name, n, label, e)),
            Err(_) => Vec::new(),
        };
        let mut step = match st.kind {
            CompilerStepKind::Copy => {
                if args.len() != 2 && enabled {
                    return Err(tf!("compilers.err_copy_args", name, n));
                }
                let mut paths = Vec::with_capacity(2);
                for a in &args {
                    match full_path(a) {
                        Some(p) => paths.push(p),
                        None if !enabled => break,
                        None => return Err(tf!("compilers.err_copy_path", name, n, a)),
                    }
                }
                if paths.len() == 2 {
                    let to = paths.remove(1);
                    let from = paths.remove(0);
                    let kind =
                        if to.starts_with(&cx.tools.maps_dir) { StepKind::CopyToMaps } else { StepKind::Custom };
                    Step::copy(kind, from, to)
                } else {
                    Step::copy(StepKind::Custom, PathBuf::new(), PathBuf::new())
                }
            }
            kind => {
                let raw = if kind == CompilerStepKind::Game && st.exe.trim().is_empty() {
                    compilers::GAME_EXE
                } else {
                    st.exe.trim()
                };
                let exe = expand(raw, &tokens);
                let exe = exe.trim();
                if enabled {
                    if exe.is_empty() {
                        return Err(tf!("compilers.err_no_program", name, n, label));
                    }
                    if find_program(exe).is_none() {
                        return Err(tf!("compilers.err_missing_exe", name, n, label, exe));
                    }
                }
                let step_kind = match kind {
                    CompilerStepKind::Bsp => StepKind::Bsp,
                    CompilerStepKind::Vis => StepKind::Vis,
                    CompilerStepKind::Rad => StepKind::Rad,
                    CompilerStepKind::Game => StepKind::LaunchGame,
                    CompilerStepKind::Copy | CompilerStepKind::Custom => StepKind::Custom,
                };
                Step::new(step_kind, exe, args)
            }
        };
        let dir = expand(st.workdir.trim(), &tokens);
        if !dir.trim().is_empty() {
            let p = PathBuf::from(dir.trim());
            if enabled && !p.is_dir() {
                return Err(tf!("compilers.err_workdir", name, n, label, p.display()));
            }
            step.workdir = Some(p);
        }
        step.enabled = enabled;
        step.continue_on_error = st.continue_on_error;
        out.push(step);
    }
    if !out.iter().any(|s| s.enabled) {
        return Err(tf!("compilers.err_no_steps", name));
    }
    Ok(out)
}

pub(crate) fn replace_compile_steps(mut plan: CompilePlan, steps: Vec<Step>) -> CompilePlan {
    let old = std::mem::take(&mut plan.steps);
    let old_kinds: Vec<StepKind> = old.iter().map(|s| s.kind).collect();
    let mut rest_index = Vec::new();
    let mut rest = Vec::new();
    for (i, s) in old.into_iter().enumerate() {
        if !matches!(s.kind, StepKind::Bsp | StepKind::Rad) {
            rest_index.push(i);
            rest.push(s);
        }
    }
    let offset = steps.len();
    for n in &mut plan.notices {
        n.step = match old_kinds.get(n.step) {
            Some(k @ (StepKind::Bsp | StepKind::Rad)) => steps.iter().position(|s| s.kind == *k).unwrap_or(0),
            Some(_) => rest_index.iter().position(|i| *i == n.step).map_or(0, |p| p + offset),
            None => 0,
        };
    }
    plan.steps = steps;
    plan.steps.extend(rest);
    plan.cache = None;
    plan
}

pub(crate) fn external_plan(
    plan: CompilePlan,
    profile: &CompilerProfile,
    s: &CompileSettings,
    tools: &ToolPaths,
    lighting: Lighting,
) -> Result<CompilePlan, String> {
    let vmf = plan.vmf_path.clone();
    let cx = Context { vmf: &vmf, tools, modes: plan.modes, lighting, threads: super::plan::threads_of(s) };
    let steps = profile_steps(profile, &cx)?;
    Ok(replace_compile_steps(plan, steps))
}

pub(crate) fn signature(profile: &CompilerProfile) -> String {
    let mut out = format!("compiler {} {}", profile.id, profile.mode_flags);
    for st in &profile.steps {
        let stamp = find_program(&st.exe)
            .and_then(|p| hp_compile::incremental::file_stamp(&p))
            .map_or_else(String::new, |(size, mtime)| format!("{size}|{mtime}"));
        out.push_str(&format!("\n{:?} {} {} {} {} {stamp}", st.kind, st.enabled, st.exe, st.args, st.workdir));
    }
    out
}

pub(crate) fn step_problems(profile: &CompilerProfile, tokens: Option<&Tokens>) -> Vec<String> {
    let name = display_name(profile);
    let mut out = Vec::new();
    if profile.is_builtin() {
        return out;
    }
    for (i, st) in profile.steps.iter().enumerate().filter(|(_, s)| s.enabled) {
        let n = i + 1;
        let label = kind_label(st.kind);
        if st.kind == CompilerStepKind::Copy {
            if hp_compile::split_args(&st.args).len() != 2 {
                out.push(tf!("compilers.err_copy_args", name, n));
            }
            continue;
        }
        if let Err(e) = option_args(profile, st, tokens.unwrap_or(&Tokens::default())) {
            out.push(tf!("compilers.err_option", name, n, label, e));
        }
        let raw = if st.kind == CompilerStepKind::Game && st.exe.trim().is_empty() {
            compilers::GAME_EXE
        } else {
            st.exe.trim()
        };
        let exe = tokens.map_or_else(|| raw.to_owned(), |t| expand(raw, t));
        let exe = exe.trim();
        if exe.is_empty() {
            out.push(tf!("compilers.err_no_program", name, n, label));
        } else if (tokens.is_some() || !exe.contains('$')) && find_program(exe).is_none() {
            out.push(tf!("compilers.err_missing_exe", name, n, label, exe));
        }
    }
    if !profile.steps.iter().any(|s| s.enabled) {
        out.push(tf!("compilers.err_no_steps", name));
    }
    out
}

pub(crate) fn source_dir_of(vmf: &Path) -> Option<PathBuf> {
    let dir = vmf.parent()?;
    let in_build =
        dir.file_name().is_some_and(|n| n.to_string_lossy().eq_ignore_ascii_case(super::snapshot::BUILD_DIR));
    in_build.then(|| dir.parent().map(Path::to_path_buf)).flatten()
}

pub(crate) fn point_instances_at_the_map_folder(map: &mut hp_vmf::Map, source_dir: &Path) -> usize {
    let mut fixed = 0;
    for e in map.entities.iter_mut().filter(|e| e.classname.eq_ignore_ascii_case("func_instance")) {
        let Some(file) = e.get("file").map(|f| f.trim().replace('\\', "/")) else { continue };
        if file.is_empty() || Path::new(&file).is_absolute() || file.starts_with("../") {
            continue;
        }
        let with_ext = if Path::new(&file).extension().is_some() { file.clone() } else { format!("{file}.vmf") };
        if source_dir.join(&with_ext).is_file() {
            e.set("file", format!("../{file}"));
            fixed += 1;
        }
    }
    fixed
}

pub(crate) fn fix_instances_in_file(vmf: &Path) -> Result<usize, String> {
    let Some(dir) = source_dir_of(vmf) else { return Ok(0) };
    let mut map = hp_vmf::load(vmf).map_err(|e| format!("{}: {e}", vmf.display()))?;
    let n = point_instances_at_the_map_folder(&mut map, &dir);
    if n > 0 {
        hp_vmf::save_with(&map, vmf, &hp_vmf::SaveOptions { backup: false, sync: false })
            .map_err(|e| format!("{}: {e}", vmf.display()))?;
    }
    Ok(n)
}

#[cfg(test)]
mod tests {
    use super::*;
    use hp_app::settings::compilers::CompilerStep;
    use hp_compile::{CompileEvent, StepAction, StepModes};

    fn scratch(name: &str) -> PathBuf {
        let dir = std::env::temp_dir().join(format!("hp_ui_compilers_{name}_{}", std::process::id()));
        let _ = std::fs::remove_dir_all(&dir);
        std::fs::create_dir_all(&dir).expect("scratch");
        std::path::absolute(dir).expect("absolute")
    }

    fn fake_game(root: &Path) -> ToolPaths {
        let game = root.join("Garrys Mod").join("garrysmod");
        std::fs::create_dir_all(game.join("maps")).expect("game");
        std::fs::write(game.join("gameinfo.txt"), "\"GameInfo\" {}").expect("gameinfo");
        ToolPaths {
            compiler: root.join("hammerpro.exe"),
            game_exe: None,
            game_dir: game.clone(),
            maps_dir: game.join("maps"),
        }
    }

    fn cx<'a>(vmf: &'a Path, tools: &'a ToolPaths, modes: StepModes) -> Context<'a> {
        Context { vmf, tools, modes, lighting: Lighting::Both, threads: None }
    }

    #[cfg(windows)]
    fn shell() -> (String, &'static str) {
        let sys = std::env::var("SystemRoot").unwrap_or_else(|_| r"C:\Windows".to_owned());
        (format!(r"{sys}\System32\cmd.exe"), "/D /C")
    }

    #[cfg(not(windows))]
    fn shell() -> (String, &'static str) {
        ("/bin/sh".to_owned(), "-c")
    }

    #[test]
    fn steps_follow_the_modes_and_quote_paths_with_spaces() {
        crate::register_translations();
        let root = scratch("modes");
        let tools = fake_game(&root);
        let bin = root.join("Valve tools");
        std::fs::create_dir_all(&bin).expect("bin");
        for t in ["vbsp.exe", "vvis.exe", "vrad.exe"] {
            std::fs::write(bin.join(t), b"MZ").expect("tool");
        }
        let mut p = CompilerProfile::blank("Valve");
        p.id = "user:1".into();
        for (st, exe) in p.steps.iter_mut().zip(["vbsp.exe", "vvis.exe", "vrad.exe"]) {
            st.exe = bin.join(exe).display().to_string();
        }
        let vmf = root.join("my maps").join(".hammerpro_build").join("gm_x.vmf");
        let modes = StepModes::new(BspMode::Full, VisMode::Fast, RadMode::Final);
        let steps = profile_steps(&p, &cx(&vmf, &tools, modes)).expect("steps");
        let kinds: Vec<StepKind> = steps.iter().map(|s| s.kind).collect();
        assert_eq!(kinds, [StepKind::Bsp, StepKind::Vis, StepKind::Rad]);
        assert_eq!(
            steps[0].args,
            ["-game", &tools.game_dir.display().to_string(), &vmf.with_extension("").display().to_string()]
        );
        assert_eq!(steps[1].args[0], "-fast");
        assert_eq!(steps[2].args[..2], ["-final", "-both"]);
        let line = steps[0].command_line();
        assert!(line.contains(&format!("\"{}\"", tools.game_dir.display())), "{line}");
        assert!(line.starts_with(&format!("\"{}\"", bin.join("vbsp.exe").display())), "{line}");

        let ents = StepModes::new(BspMode::OnlyEnts, VisMode::Off, RadMode::Off);
        let steps = profile_steps(&p, &cx(&vmf, &tools, ents)).expect("steps");
        assert_eq!(steps.iter().map(|s| s.enabled).collect::<Vec<_>>(), [true, false, false]);
        assert_eq!(steps[0].args[0], "-onlyents");

        p.mode_flags = false;
        let steps = profile_steps(&p, &cx(&vmf, &tools, modes)).expect("steps");
        assert_eq!(steps[2].args[0], "-game", "no mode flags");

        p.steps[1].exe = bin.join("missing.exe").display().to_string();
        let err = profile_steps(&p, &cx(&vmf, &tools, modes)).expect_err("missing exe");
        assert!(err.contains("missing.exe") && err.contains('2'), "{err}");
        p.steps[1].enabled = false;
        assert!(profile_steps(&p, &cx(&vmf, &tools, modes)).is_ok(), "a disabled step is not checked");
        p.steps.iter_mut().for_each(|s| s.enabled = false);
        assert!(profile_steps(&p, &cx(&vmf, &tools, modes)).is_err());
        assert!(find_program("").is_none());
        assert!(find_program(&bin.join("vbsp.exe").display().to_string()).is_some());
        assert!(find_program(r"..\vbsp.exe").is_none());
        assert!(find_program(&shell().0).is_some());
    }

    #[test]
    fn options_and_extra_arguments_come_before_the_map() {
        crate::register_translations();
        let root = scratch("options");
        let tools = fake_game(&root);
        let exe = root.join("vrad.exe");
        std::fs::write(&exe, b"MZ").expect("tool");
        let mut p = CompilerProfile::blank("Valve");
        p.id = "user:3".into();
        p.steps.truncate(0);
        let mut st = CompilerStep::new(CompilerStepKind::Rad, exe.display().to_string(), compilers::COMPILE_ARGS);
        st.options.insert("-bounce".into(), "8".into());
        st.options.insert("-StaticPropLighting".into(), String::new());
        st.options.insert("-final".into(), String::new());
        st.options.insert("-lights".into(), "$path/my lights.rad".into());
        st.extra = "-noskyboxrecurse".into();
        p.steps.push(st);
        let vmf = root.join("maps").join(".hammerpro_build").join("gm_x.vmf");
        let modes = StepModes::new(BspMode::Full, VisMode::Full, RadMode::Normal);
        let cx = Context { vmf: &vmf, tools: &tools, modes, lighting: Lighting::Ldr, threads: Some(3) };
        let steps = profile_steps(&p, &cx).expect("steps");
        let a = &steps[0].args;
        assert_eq!(a[..3], ["-ldr", "-threads", "3"], "{a:?}");
        let pos = |f: &str| a.iter().position(|x| x == f).unwrap_or(usize::MAX);
        assert!(pos("-bounce") < pos("-StaticPropLighting") && pos("-StaticPropLighting") < pos("-lights"), "{a:?}");
        assert_eq!(a[pos("-bounce") + 1], "8");
        assert_eq!(PathBuf::from(&a[pos("-lights") + 1]), vmf.parent().expect("dir").join("my lights.rad"));
        assert_eq!(pos("-final"), usize::MAX, "the window's quality decides -final");
        assert!(pos("-noskyboxrecurse") < pos("-game"));
        assert_eq!(a.last().map(String::as_str), Some(vmf.with_extension("").display().to_string().as_str()));
        p.mode_flags = false;
        let steps = profile_steps(&p, &cx).expect("steps");
        assert!(steps[0].args.contains(&"-final".to_owned()) && !steps[0].args.contains(&"-ldr".to_owned()));
        p.steps[0].options.insert("-bounce".into(), "lots".into());
        let err = profile_steps(&p, &cx).expect_err("bad value");
        assert!(err.contains("-bounce"), "{err}");
        assert_eq!(step_problems(&p, None).len(), 1);
        p.steps[0].options.remove("-bounce");
        p.steps[0].options.insert("-nonsense".into(), String::new());
        assert!(profile_steps(&p, &cx).expect_err("unknown").contains("-nonsense"));
    }

    #[test]
    fn copy_game_custom_and_working_folders() {
        crate::register_translations();
        let root = scratch("extras");
        let mut tools = fake_game(&root);
        let gmod = root.join("Garrys Mod").join("gmod.exe");
        std::fs::write(&gmod, b"MZ").expect("game exe");
        tools.game_exe = Some(gmod.clone());
        let vmf = root.join("maps").join(".hammerpro_build").join("gm_x.vmf");
        let (sh, _) = shell();
        let p = CompilerProfile {
            id: "user:2".into(),
            name: "Extras".into(),
            mode_flags: true,
            steps: vec![
                CompilerStep::new(CompilerStepKind::Custom, sh.clone(), "/C echo $file"),
                CompilerStep::of_kind(CompilerStepKind::Copy),
                CompilerStep { workdir: "$exedir".into(), ..CompilerStep::of_kind(CompilerStepKind::Game) },
            ],
        };
        let steps = profile_steps(&p, &cx(&vmf, &tools, StepModes::default())).expect("steps");
        assert_eq!(steps[0].kind, StepKind::Custom);
        assert_eq!(steps[0].args, ["/C", "echo", "gm_x"], "custom steps get no mode flags");
        match &steps[1].action {
            StepAction::Copy { from, to } => {
                assert_eq!(from, &vmf.with_extension("bsp"));
                assert_eq!(to, &tools.maps_dir.join("gm_x.bsp"));
            }
            other => panic!("{other:?}"),
        }
        assert_eq!(steps[1].kind, StepKind::CopyToMaps);
        assert_eq!(steps[2].kind, StepKind::LaunchGame);
        assert_eq!(steps[2].exe, gmod);
        assert_eq!(steps[2].action, StepAction::Spawn);
        assert_eq!(steps[2].workdir.as_deref(), gmod.parent());
        assert!(steps[2].args.contains(&"+map".to_owned()) && steps[2].args.contains(&"gm_x".to_owned()));

        let bad_copy = CompilerProfile {
            steps: vec![CompilerStep::new(CompilerStepKind::Copy, "", "relative.bsp $bspdir")],
            ..p.clone()
        };
        assert!(profile_steps(&bad_copy, &cx(&vmf, &tools, StepModes::default())).is_err());
        let one = CompilerProfile { steps: vec![CompilerStep::new(CompilerStepKind::Copy, "", "$path")], ..p.clone() };
        assert!(profile_steps(&one, &cx(&vmf, &tools, StepModes::default())).is_err());
        let mut off = p.clone();
        off.steps[1].enabled = false;
        off.steps[1].args = "only-one".into();
        let steps = profile_steps(&off, &cx(&vmf, &tools, StepModes::default())).expect("a disabled copy is kept");
        assert_eq!(steps.len(), off.steps.len(), "one step per step of the compiler");
        assert!(!steps[1].enabled);
        let bad_dir = CompilerProfile {
            steps: vec![CompilerStep {
                workdir: root.join("no folder").display().to_string(),
                ..CompilerStep::new(CompilerStepKind::Custom, sh, "")
            }],
            ..p.clone()
        };
        let err = profile_steps(&bad_dir, &cx(&vmf, &tools, StepModes::default())).expect_err("workdir");
        assert!(err.contains("no folder"), "{err}");
        tools.game_exe = None;
        let no_game = CompilerProfile { steps: vec![CompilerStep::of_kind(CompilerStepKind::Game)], ..p };
        assert!(profile_steps(&no_game, &cx(&vmf, &tools, StepModes::default())).is_err());
    }

    #[test]
    fn the_pipeline_keeps_its_pack_copy_and_launch_steps() {
        crate::register_translations();
        let root = scratch("replace");
        let tools = fake_game(&root);
        let vmf = root.join("gm_x.vmf");
        let mut plan = CompilePlan::new(&vmf);
        plan.steps.push(Step::new(StepKind::Bsp, "hammerpro.exe", vec![]));
        plan.steps.push(Step::new(StepKind::Rad, "hammerpro.exe", vec![]));
        plan.steps.push(Step::copy(StepKind::CopyToMaps, vmf.with_extension("bsp"), tools.maps_dir.join("gm_x.bsp")));
        plan.notices.push(hp_compile::PlanNotice { step: 2, notice: hp_compile::Notice::PipelineStopped });
        plan.notices.push(hp_compile::PlanNotice { step: 1, notice: hp_compile::Notice::PipelineStopped });
        let ext = vec![
            Step::new(StepKind::Bsp, "vbsp.exe", vec![]),
            Step::new(StepKind::Vis, "vvis.exe", vec![]),
            Step::new(StepKind::Rad, "vrad.exe", vec![]),
        ];
        let out = replace_compile_steps(plan, ext);
        let kinds: Vec<StepKind> = out.steps.iter().map(|s| s.kind).collect();
        assert_eq!(kinds, [StepKind::Bsp, StepKind::Vis, StepKind::Rad, StepKind::CopyToMaps]);
        assert_eq!(out.notices[0].step, 3);
        assert_eq!(out.notices[1].step, 2);
        assert!(out.cache.is_none());
    }

    #[test]
    fn choices_fall_back_when_a_profile_is_gone() {
        let mut s = CompileSettings::default();
        assert_eq!(choice_with(&s, &[]), Choice::Builtin);
        let mut p = CompilerProfile::blank("Mine");
        p.id = "user:1".into();
        s.compilers.push(p.clone());
        s.set_active_compiler("user:1");
        assert_eq!(choice_with(&s, &[]), Choice::External(p));
        s.set_active_compiler("detected:valve:bin");
        assert_eq!(choice_with(&s, &[]), Choice::Missing("detected:valve:bin".into()));
        let found = CompilerProfile { id: "detected:valve:bin".into(), ..CompilerProfile::blank("Valve (bin)") };
        assert_eq!(choice_with(&s, std::slice::from_ref(&found)), Choice::External(found));
    }

    #[test]
    fn instances_point_back_at_the_map_folder() {
        let root = scratch("instances");
        std::fs::create_dir_all(root.join("instances")).expect("dir");
        std::fs::write(root.join("instances").join("a.vmf"), "world {}").expect("instance");
        let build = root.join(super::super::snapshot::BUILD_DIR);
        std::fs::create_dir_all(&build).expect("build");
        let text = "world { \"id\" \"1\" \"classname\" \"worldspawn\" }\nentity { \"id\" \"2\" \"classname\" \"func_instance\" \"file\" \"instances\\a.vmf\" }\nentity { \"id\" \"3\" \"classname\" \"func_instance\" \"file\" \"instances/missing\" }\n";
        let vmf = build.join("gm_x.vmf");
        std::fs::write(&vmf, text).expect("vmf");
        assert_eq!(source_dir_of(&vmf).as_deref(), Some(root.as_path()));
        assert_eq!(source_dir_of(&root.join("gm_x.vmf")), None);
        assert_eq!(fix_instances_in_file(&vmf).expect("fix"), 1);
        let map = hp_vmf::load(&vmf).expect("load");
        assert_eq!(map.entities[0].get("file"), Some("../instances/a.vmf"));
        assert_eq!(map.entities[1].get("file"), Some("instances/missing"));
        assert_eq!(fix_instances_in_file(&vmf).expect("again"), 0, "already fixed");
    }

    #[test]
    fn a_fake_external_compiler_writes_the_bsp_that_is_copied_to_the_game() {
        crate::register_translations();
        let root = scratch("fake run");
        let tools = fake_game(&root);
        let build = root.join("my maps").join(".hammerpro_build");
        std::fs::create_dir_all(&build).expect("build");
        let vmf = build.join("gm_fake.vmf");
        std::fs::write(&vmf, "world {}").expect("vmf");
        let tools_dir = root.join("fake tools");
        std::fs::create_dir_all(&tools_dir).expect("tools");
        #[cfg(windows)]
        let (bsp_exe, rad_exe) = {
            let bsp = tools_dir.join("fake vbsp.cmd");
            let rad = tools_dir.join("fake vrad.cmd");
            std::fs::write(&bsp, "@echo off\r\necho fake vbsp %~n1\r\ncopy /Y \"%~1.vmf\" \"%~1.bsp\" >nul\r\n")
                .expect("vbsp");
            std::fs::write(&rad, "@echo off\r\necho fake vrad\r\necho lit>>\"%~1.bsp\"\r\nexit /b 3\r\n")
                .expect("vrad");
            (bsp, rad)
        };
        #[cfg(not(windows))]
        let (bsp_exe, rad_exe) = {
            use std::os::unix::fs::PermissionsExt;
            let bsp = tools_dir.join("fake vbsp");
            let rad = tools_dir.join("fake vrad");
            std::fs::write(&bsp, "#!/bin/sh\necho fake vbsp $(basename \"$1\")\ncp \"$1.vmf\" \"$1.bsp\"\n")
                .expect("vbsp");
            std::fs::write(&rad, "#!/bin/sh\necho fake vrad\necho lit >> \"$1.bsp\"\nexit 3\n").expect("vrad");
            for f in [&bsp, &rad] {
                std::fs::set_permissions(f, std::fs::Permissions::from_mode(0o755)).expect("chmod");
            }
            (bsp, rad)
        };
        let map_arg = if cfg!(windows) { r"$path\$file" } else { "$path/$file" };
        let mut p = CompilerProfile {
            id: "user:9".into(),
            name: "Fake".into(),
            mode_flags: false,
            steps: vec![
                CompilerStep::new(CompilerStepKind::Bsp, bsp_exe.display().to_string(), map_arg),
                CompilerStep::new(CompilerStepKind::Rad, rad_exe.display().to_string(), map_arg),
            ],
        };
        p.steps[1].continue_on_error = true;
        let mut plan = CompilePlan::new(&vmf);
        plan.steps.push(Step::copy(
            StepKind::CopyToMaps,
            vmf.with_extension("bsp"),
            tools.maps_dir.join("gm_fake.bsp"),
        ));
        let s = CompileSettings::default();
        let plan = external_plan(plan, &p, &s, &tools, Lighting::Ldr).expect("plan");
        let handle = hp_compile::start(plan, &build);
        let mut events = Vec::new();
        while let Ok(ev) = handle.events.recv_timeout(std::time::Duration::from_secs(60)) {
            let done = matches!(ev, CompileEvent::Finished { .. });
            events.push(ev);
            if done {
                break;
            }
        }
        handle.wait();
        let lines: Vec<String> = events
            .iter()
            .filter_map(|e| match e {
                CompileEvent::Log { line, .. } => Some(line.clone()),
                _ => None,
            })
            .collect();
        assert!(lines.iter().any(|l| l.contains("fake vbsp gm_fake")), "{lines:?}");
        assert!(lines.iter().any(|l| l.contains("fake vrad")), "{lines:?}");
        let exits: Vec<(bool, Option<i32>)> = events
            .iter()
            .filter_map(|e| match e {
                CompileEvent::StepFinished { success, exit_code, .. } => Some((*success, *exit_code)),
                _ => None,
            })
            .collect();
        assert_eq!(exits[0], (true, Some(0)));
        assert_eq!(exits[1], (false, Some(3)), "the exit code is reported");
        assert!(exits[2].0, "the copy still runs after a step that may fail");
        let copied = std::fs::read_to_string(tools.maps_dir.join("gm_fake.bsp")).expect("copied");
        assert!(copied.starts_with("world {}") && copied.contains("lit"), "{copied}");
    }

    #[test]
    fn real_valve_tools_compile_a_map_when_garrys_mod_is_installed() {
        crate::register_translations();
        let gmod = Path::new(r"D:\SteamLibrary\steamapps\common\GarrysMod");
        let source = Path::new(env!("CARGO_MANIFEST_DIR")).join("../../../test_maps/hammerpro_test_cozy.vmf");
        if !gmod.join("garrysmod").join("gameinfo.txt").is_file() || !source.is_file() {
            eprintln!("Garry's Mod or the test map not found, skipping");
            return;
        }
        let found = compilers::detect_compilers(&gmod.join("garrysmod"));
        let Some(valve) = found.iter().find(|p| p.id == "detected:valve:bin/win64").or_else(|| found.first()) else {
            eprintln!("no Valve tools next to the game, skipping");
            return;
        };
        let root = scratch("real valve");
        let maps = root.join("maps out");
        std::fs::create_dir_all(&maps).expect("maps");
        let build = root.join("my map").join(super::super::snapshot::BUILD_DIR);
        std::fs::create_dir_all(&build).expect("build");
        let vmf = build.join("hp_valve_test.vmf");
        std::fs::copy(&source, &vmf).expect("vmf");
        let tools = ToolPaths {
            compiler: root.join("hammerpro.exe"),
            game_exe: None,
            game_dir: gmod.join("garrysmod"),
            maps_dir: maps.clone(),
        };
        let mut plan = CompilePlan::new(&vmf);
        plan.modes = StepModes::new(BspMode::Full, VisMode::Fast, RadMode::Draft);
        plan.steps.push(Step::copy(StepKind::CopyToMaps, vmf.with_extension("bsp"), maps.join("hp_valve_test.bsp")));
        let s = CompileSettings::default();
        let plan = external_plan(plan, valve, &s, &tools, Lighting::Ldr).expect("plan");
        assert!(plan.steps.iter().any(|st| st.kind == StepKind::Rad && st.args.contains(&"-fast".to_owned())));
        let handle = hp_compile::start(plan, &build);
        let mut finished = None;
        let mut lines = Vec::new();
        while let Ok(ev) = handle.events.recv_timeout(std::time::Duration::from_secs(300)) {
            match ev {
                CompileEvent::Log { line, .. } => lines.push(line),
                CompileEvent::Finished { success, .. } => {
                    finished = Some(success);
                    break;
                }
                _ => {}
            }
        }
        handle.wait();
        assert_eq!(finished, Some(true), "{}", lines.join("\n"));
        let bsp = std::fs::read(maps.join("hp_valve_test.bsp")).expect("copied to the maps folder");
        assert_eq!(&bsp[..4], b"VBSP");
        assert!(lines.iter().any(|l| l.contains("Writing") || l.contains("writing")), "{lines:?}");
    }
}
