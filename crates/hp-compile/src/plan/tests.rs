use super::*;
use crate::flags::FlagChoice;

const EXE: &str = r"C:\Program Files\HammerPro\hammerpro.exe";

fn tools() -> ToolPaths {
    ToolPaths {
        compiler: PathBuf::from(EXE),
        game_exe: Some(PathBuf::from(r"D:\Steam\GarrysMod\gmod.exe")),
        game_dir: PathBuf::from(r"D:\Steam\GarrysMod\garrysmod\"),
        maps_dir: PathBuf::from(r"D:\Steam\GarrysMod\garrysmod\maps"),
    }
}

#[cfg(windows)]
fn vmf() -> PathBuf {
    PathBuf::from(r"C:\My Maps\gm_test.vmf")
}

#[cfg(windows)]
fn args(plan: &CompilePlan, kind: StepKind) -> Vec<String> {
    plan.steps.iter().find(|s| s.kind == kind).map(|s| s.args.clone()).unwrap_or_default()
}

fn kinds(plan: &CompilePlan) -> Vec<StepKind> {
    plan.steps.iter().map(|s| s.kind).collect()
}

#[test]
#[cfg(windows)]
fn draft_preset() {
    let plan = CompilePlan::preset(Preset::Draft, &tools(), &vmf(), &PlanOptions::default()).unwrap();
    assert_eq!(kinds(&plan), vec![StepKind::Bsp, StepKind::Rad, StepKind::CopyToMaps]);
    let root = r"D:\Steam\GarrysMod";
    let game = r"D:\Steam\GarrysMod\garrysmod";
    let map = r"C:\My Maps\gm_test";
    assert_eq!(
        args(&plan, StepKind::Bsp),
        vec!["--bspc", "-gmod", root, "-fastvis", "-AllowDynamicPropsAsStatic", "-game", game, map]
    );
    assert_eq!(
        args(&plan, StepKind::Rad),
        vec!["--rad", "-ldr", "-bounce", "1", "-fast", "-gmod", root, "-game", game, map]
    );
    assert!(plan.steps.iter().take(2).all(|s| s.exe == Path::new(EXE)));
    match &plan.steps[2].action {
        StepAction::Copy { from, to } => {
            assert_eq!(from, &PathBuf::from(r"C:\My Maps\gm_test.bsp"));
            assert_eq!(to, &PathBuf::from(r"D:\Steam\GarrysMod\garrysmod\maps\gm_test.bsp"));
        }
        other => panic!("unexpected {other:?}"),
    }
    assert!(plan.steps.iter().all(|s| s.enabled && !s.continue_on_error));
    assert_eq!(plan.map_name(), Some("gm_test"));
    assert_eq!(plan.lin_path(), PathBuf::from(r"C:\My Maps\gm_test.lin"));
    assert!(plan.notices.is_empty());
    assert!(!plan.is_partial());

    let opts = PlanOptions { stop_on_leak: true, ..PlanOptions::default() };
    let plan = CompilePlan::preset(Preset::Draft, &tools(), &vmf(), &opts).unwrap();
    assert_eq!(args(&plan, StepKind::Bsp)[3..5], ["-fastvis", "-leaktest"]);
    assert!(plan.stop_on_leak);
}

#[test]
#[cfg(windows)]
fn normal_and_final_presets() {
    let opts = PlanOptions {
        lighting: Lighting::Ldr,
        threads: Some(6),
        extra_rad_args: "-bounce 4 -x \"C:\\my file\"".to_owned(),
        copy_to_maps: false,
        ..PlanOptions::default()
    };
    let normal = CompilePlan::preset(Preset::Normal, &tools(), &vmf(), &opts).unwrap();
    assert_eq!(kinds(&normal), vec![StepKind::Bsp, StepKind::Rad]);
    assert_eq!(args(&normal, StepKind::Bsp)[3..7], ["-vis", "-AllowDynamicPropsAsStatic", "-threads", "6"]);
    let rad = args(&normal, StepKind::Rad);
    assert_eq!(&rad[..6], &["--rad", "-ldr", "-threads", "6", "-bounce", "4"]);
    assert!(rad.contains(&r"C:\my file".to_owned()));

    let fin = CompilePlan::preset(Preset::Final, &tools(), &vmf(), &opts).unwrap();
    let rad = args(&fin, StepKind::Rad);
    assert_eq!(&rad[..3], &["--rad", "-both", "-final"]);
    assert!(args(&fin, StepKind::Bsp).contains(&"-vis".to_owned()));
    let custom = CompilePlan::preset(Preset::Custom, &tools(), &vmf(), &opts).unwrap();
    assert_eq!(custom.steps, normal.steps);
    let both = PlanOptions { lighting: Lighting::Both, ..opts.clone() };
    let draft = CompilePlan::preset(Preset::Draft, &tools(), &vmf(), &both).unwrap();
    assert_eq!(args(&draft, StepKind::Rad)[1], "-ldr");
    let no_vis = PlanOptions { modes: Some(StepModes::new(BspMode::Full, VisMode::Off, RadMode::Off)), ..opts };
    let plan = CompilePlan::preset(Preset::Custom, &tools(), &vmf(), &no_vis).unwrap();
    assert_eq!(kinds(&plan), vec![StepKind::Bsp]);
    assert!(!args(&plan, StepKind::Bsp).iter().any(|a| a == "-vis" || a == "-fastvis"));
}

#[test]
#[cfg(windows)]
fn pack_and_launch_steps() {
    let opts = PlanOptions {
        pack_files: vec![("materials/hp/a.vmt".to_owned(), PathBuf::from(r"C:\c\a.vmt"))],
        launch_game: true,
        ..PlanOptions::default()
    };
    let plan = CompilePlan::preset(Preset::Normal, &tools(), &vmf(), &opts).unwrap();
    assert_eq!(
        kinds(&plan),
        vec![StepKind::Bsp, StepKind::Rad, StepKind::Pack, StepKind::CopyToMaps, StepKind::LaunchGame]
    );
    let pack = &plan.steps[2];
    assert!(pack.exe.as_os_str().is_empty(), "packed in process");
    assert_eq!(pack.args, vec![r"C:\My Maps\gm_test.bsp"]);
    assert_eq!(pack.command_line(), r#"pack "C:\My Maps\gm_test.bsp""#);
    assert!(matches!(&pack.action, StepAction::Pack { files } if files.len() == 1));

    let launch = &plan.steps[4];
    assert_eq!(launch.action, StepAction::Spawn);
    assert_eq!(launch.exe, PathBuf::from(r"D:\Steam\GarrysMod\gmod.exe"));
    assert_eq!(launch.args, vec!["-steam", "-game", "garrysmod", "-console", "-dev", "-hijack", "+map", "gm_test"]);

    let opts2 = PlanOptions { launch_game: true, game_args: "-windowed -w 1280".to_owned(), ..PlanOptions::default() };
    let plan2 = CompilePlan::preset(Preset::Draft, &tools(), &vmf(), &opts2).unwrap();
    assert_eq!(plan2.steps.last().unwrap().args, vec!["-windowed", "-w", "1280", "+map", "gm_test"]);

    let (deferred, _sender) = DeferredFiles::new();
    let opts3 = PlanOptions { deferred_pack: Some(deferred.clone()), ..PlanOptions::default() };
    let plan3 = CompilePlan::preset(Preset::Final, &tools(), &vmf(), &opts3).unwrap();
    let pack = plan3.steps.iter().find(|s| s.kind == StepKind::Pack).expect("pack step");
    assert!(matches!(&pack.action, StepAction::PackDeferred { files } if *files == deferred));
}

#[test]
#[cfg(windows)]
fn steam_url_launch() {
    let opts = PlanOptions { launch_game: true, launch_method: LaunchMethod::SteamUrl, ..PlanOptions::default() };
    let plan = CompilePlan::preset(Preset::Draft, &tools(), &vmf(), &opts).unwrap();
    let step = plan.steps.last().unwrap();
    assert_eq!(step.kind, StepKind::LaunchGame);
    assert_eq!(step.args.last().unwrap(), "steam://rungameid/4000//+map%20gm_test");

    let mut t = tools();
    t.game_exe = None;
    let opts = PlanOptions { launch_game: true, ..PlanOptions::default() };
    let plan = CompilePlan::preset(Preset::Draft, &t, &vmf(), &opts).unwrap();
    assert!(plan.steps.last().unwrap().args.last().unwrap().starts_with("steam://"));
}

#[test]
fn bad_map_names() {
    let t = tools();
    let o = PlanOptions::default();
    assert!(matches!(
        CompilePlan::preset(Preset::Draft, &t, Path::new(r"C:\maps\gm_a.v2.vmf"), &o),
        Err(CompileError::InvalidMapName { problem: MapNameProblem::Dot, .. })
    ));
    let err = CompilePlan::preset(Preset::Draft, &t, Path::new(r"C:\maps\my map.vmf"), &o);
    assert!(
        matches!(&err, Err(CompileError::InvalidMapName { problem: MapNameProblem::Whitespace, name }) if name == "my map")
    );
    assert!(err.err().is_some_and(|e| e.to_string().contains("spaces")));
    assert!(matches!(CompilePlan::preset(Preset::Draft, &t, Path::new(""), &o), Err(CompileError::InvalidMapPath(_))));
}

#[test]
fn split_and_quote() {
    assert_eq!(split_args(""), Vec::<String>::new());
    assert_eq!(split_args("  -a  -b 2 "), vec!["-a", "-b", "2"]);
    assert_eq!(split_args(r#"-lights "C:\my dir\x.rad" -x"#), vec!["-lights", r"C:\my dir\x.rad", "-x"]);
    assert_eq!(split_args(r#"a "" b"#), vec!["a", "", "b"]);
    assert_eq!(split_args(r#"say \"hi\""#), vec!["say", "\"hi\""]);
    assert_eq!(split_args(r"C:\path\to"), vec![r"C:\path\to"]);

    assert_eq!(quote_arg("-fast"), "-fast");
    assert_eq!(quote_arg(""), "\"\"");
    assert_eq!(quote_arg(r"C:\My Maps\a"), r#""C:\My Maps\a""#);
    assert_eq!(quote_arg(r"C:\My Maps\"), r#""C:\My Maps\\""#);
    assert_eq!(quote_arg(r#"say "hi""#), r#""say \"hi\"""#);
}

#[test]
#[cfg(windows)]
fn command_lines() {
    let plan = CompilePlan::preset(Preset::Draft, &tools(), &vmf(), &PlanOptions::default()).unwrap();
    assert_eq!(
        command_line_string(&plan.steps[0]),
        r#""C:\Program Files\HammerPro\hammerpro.exe" --bspc -gmod D:\Steam\GarrysMod -fastvis -AllowDynamicPropsAsStatic -game D:\Steam\GarrysMod\garrysmod "C:\My Maps\gm_test""#
    );
    assert_eq!(
        plan.steps[2].command_line(),
        r#"copy "C:\My Maps\gm_test.bsp" D:\Steam\GarrysMod\garrysmod\maps\gm_test.bsp"#
    );
}

#[test]
#[cfg(windows)]
fn content_args_reach_both_steps_once() {
    let content: Vec<String> =
        ["-serveraddons", "-wsaddons", "-insert_search_path", "E:/css", "-insert_search_path", "E:/hl2"]
            .iter()
            .map(|s| (*s).to_owned())
            .collect();
    let opts =
        PlanOptions { content_args: content.clone(), extra_bsp_args: "-WSADDONS".to_owned(), ..PlanOptions::default() };
    let plan = CompilePlan::preset(Preset::Normal, &tools(), &vmf(), &opts).unwrap();
    let game = r"D:\Steam\GarrysMod\garrysmod";
    let map = r"C:\My Maps\gm_test";
    let b = args(&plan, StepKind::Bsp);
    let tail = ["-serveraddons", "-insert_search_path", "E:/css", "-insert_search_path", "E:/hl2", "-game", game, map];
    assert_eq!(b[b.len() - tail.len()..], tail[..], "{b:?}");
    assert_eq!(b.iter().filter(|a| a.eq_ignore_ascii_case("-wsaddons")).count(), 1, "{b:?}");
    let r = args(&plan, StepKind::Rad);
    let at = r.iter().position(|a| a == "-serveraddons").expect("lighting content");
    assert_eq!(r[at..at + content.len()], content[..], "{r:?}");
    assert!(r.ends_with(&[
        "-gmod".to_owned(),
        r"D:\Steam\GarrysMod".to_owned(),
        "-game".to_owned(),
        game.to_owned(),
        map.to_owned()
    ]));
    let plain = CompilePlan::preset(Preset::Normal, &tools(), &vmf(), &PlanOptions::default()).unwrap();
    assert!(plain.steps.iter().all(|s| !s.args.iter().any(|a| a.ends_with("addons"))));
}

#[test]
#[cfg(windows)]
fn compile_game_dir_only_for_the_compiler() {
    let opts = PlanOptions {
        compile_game_dir: Some(PathBuf::from(r"C:\Cache\compile_game\")),
        pack_files: vec![("materials/hp/a.vmt".to_owned(), PathBuf::from(r"C:\c.vmt"))],
        ..PlanOptions::default()
    };
    let plan = CompilePlan::preset(Preset::Normal, &tools(), &vmf(), &opts).unwrap();
    let a = args(&plan, StepKind::Bsp);
    assert!(a.windows(2).any(|w| w[0] == "-game" && w[1] == r"C:\Cache\compile_game"), "{a:?}");
    assert!(a.windows(2).any(|w| w[0] == "-gmod" && w[1] == r"D:\Steam\GarrysMod"), "{a:?}");
    let r = args(&plan, StepKind::Rad);
    assert!(r.windows(2).any(|w| w[0] == "-game" && w[1] == r"C:\Cache\compile_game"), "{r:?}");
    assert!(r.windows(2).any(|w| w[0] == "-gmod" && w[1] == r"D:\Steam\GarrysMod"), "{r:?}");
    match &plan.steps.iter().find(|s| s.kind == StepKind::CopyToMaps).unwrap().action {
        StepAction::Copy { to, .. } => assert!(to.starts_with(r"D:\Steam\GarrysMod\garrysmod\maps")),
        other => panic!("unexpected {other:?}"),
    }
}

#[test]
fn options_roundtrip_defaults() {
    let o = PlanOptions::default();
    assert_eq!(o.lighting, Lighting::Ldr);
    assert!(o.copy_to_maps && !o.launch_game && !o.stop_on_leak);
    assert_eq!(o.game_args, DEFAULT_GAME_ARGS);
    assert_eq!(crate::launch::url_encode("gm_a b+c"), "gm_a%20b%2Bc");
    let old: PlanOptions = serde_json::from_str(r#"{"extra_vis_args":"-fast","caps":{"bsp":{}},"threads":4}"#).unwrap();
    assert_eq!(old.threads, Some(4));
}

#[test]
fn presets_and_modes() {
    for p in [Preset::Draft, Preset::Normal, Preset::Final] {
        assert_eq!(Preset::of_modes(p.modes()), p);
    }
    let custom = StepModes::new(BspMode::OnlyEnts, VisMode::Off, RadMode::Off);
    assert_eq!(Preset::of_modes(custom), Preset::Custom);
    assert_eq!(Preset::Custom.modes(), Preset::Normal.modes());
    let de = serde::de::value::StrDeserializer::<serde::de::value::Error>::new("Fast");
    assert_eq!(Preset::deserialize(de).expect("alias"), Preset::Draft);
}

fn partial_dir(name: &str, with_bsp: bool) -> PathBuf {
    let dir = crate::test_util::scratch(name);
    if with_bsp {
        std::fs::write(dir.join("gm_part.bsp"), b"VBSP").unwrap();
    }
    dir.join("gm_part.vmf")
}

#[test]
fn partial_modes() {
    let vmf = partial_dir("plan_partial", true);
    let opts = PlanOptions {
        modes: Some(StepModes::new(BspMode::OnlyEnts, VisMode::Full, RadMode::Off)),
        launch_game: true,
        ..PlanOptions::default()
    };
    let plan = CompilePlan::preset(Preset::Normal, &tools(), &vmf, &opts).unwrap();
    assert_eq!(kinds(&plan), vec![StepKind::Bsp, StepKind::CopyToMaps, StepKind::LaunchGame]);
    assert!(plan.steps[0].args.contains(&"-onlyents".to_owned()));
    assert!(!plan.steps[0].args.iter().any(|a| a == "-vis"));
    assert_eq!(plan.modes.vis, VisMode::Off);
    assert!(plan.is_partial() && plan.notices.is_empty());
    let launch = &plan.steps[2].args;
    let stale = launch.iter().position(|a| a == "-allowstalezip").expect("-allowstalezip");
    assert!(stale < launch.iter().position(|a| a == "+map").unwrap_or(0));

    let opts = PlanOptions {
        modes: Some(StepModes::new(BspMode::OnlyEnts, VisMode::Off, RadMode::Normal)),
        copy_to_maps: false,
        ..PlanOptions::default()
    };
    let plan = CompilePlan::preset(Preset::Normal, &tools(), &vmf, &opts).unwrap();
    assert_eq!(kinds(&plan), vec![StepKind::Bsp, StepKind::Rad]);

    let opts = PlanOptions {
        modes: Some(StepModes::new(BspMode::EntsAndProps, VisMode::Off, RadMode::Draft)),
        copy_to_maps: false,
        ..PlanOptions::default()
    };
    let plan = CompilePlan::preset(Preset::Normal, &tools(), &vmf, &opts).unwrap();
    assert_eq!(kinds(&plan), vec![StepKind::Bsp, StepKind::Rad]);
    let bsp = &plan.steps[0].args;
    assert!(bsp.contains(&"-onlyents".to_owned()) && bsp.contains(&"-onlyprops".to_owned()));

    let vmf = partial_dir("plan_partial_nobsp", false);
    let opts = PlanOptions {
        modes: Some(StepModes::new(BspMode::OnlyEnts, VisMode::Off, RadMode::Off)),
        copy_to_maps: false,
        launch_game: true,
        ..PlanOptions::default()
    };
    let plan = CompilePlan::preset(Preset::Draft, &tools(), &vmf, &opts).unwrap();
    assert_eq!(kinds(&plan), vec![StepKind::Bsp, StepKind::Rad, StepKind::LaunchGame]);
    assert_eq!(plan.modes, Preset::Draft.modes());
    assert!(matches!(&plan.notices[..], [PlanNotice { step: 0, notice: Notice::NoPreviousBsp { .. } }]));
    assert!(!plan.steps[2].args.iter().any(|a| a == "-allowstalezip"));

    let opts = PlanOptions {
        modes: Some(StepModes::new(BspMode::Off, VisMode::Full, RadMode::Normal)),
        copy_to_maps: false,
        ..PlanOptions::default()
    };
    let plan = CompilePlan::preset(Preset::Custom, &tools(), &vmf, &opts).unwrap();
    assert_eq!(kinds(&plan), vec![StepKind::Rad]);
    assert_eq!(plan.modes.vis, VisMode::Off);
}

#[cfg(windows)]
fn rad_flags_of(plan: &CompilePlan) -> Vec<String> {
    let rad = args(plan, StepKind::Rad);
    let end = rad.iter().position(|a| a == "-gmod").unwrap_or(rad.len());
    rad[2..end].to_vec()
}

#[test]
#[cfg(windows)]
fn static_prop_settings_per_preset() {
    let has = |plan: &CompilePlan, flag: &str| args(plan, StepKind::Rad).iter().any(|a| a == flag);
    for preset in [Preset::Draft, Preset::Normal, Preset::Final, Preset::Custom] {
        for lighting in [true, false] {
            for shadows in [true, false] {
                let opts = PlanOptions {
                    static_prop_lighting: lighting,
                    static_prop_shadows: shadows,
                    ..PlanOptions::default()
                };
                let plan = CompilePlan::preset(preset, &tools(), &vmf(), &opts).unwrap();
                assert_eq!(has(&plan, "-StaticPropLighting"), lighting && preset == Preset::Final, "{preset:?}");
                assert_eq!(has(&plan, "-nostaticpropshadows"), !shadows, "{preset:?}");
                assert_eq!(has(&plan, "-final"), preset == Preset::Final);
                assert_eq!(has(&plan, "-fast"), preset == Preset::Draft);
                let rad = args(&plan, StepKind::Rad);
                assert!(rad.iter().filter(|a| a.eq_ignore_ascii_case("-nostaticpropshadows")).count() <= 1);
                assert!(!args(&plan, StepKind::Bsp).iter().any(|a| a.contains("staticprop")));
            }
        }
    }
    let fin = |opts: &PlanOptions| rad_flags_of(&CompilePlan::preset(Preset::Final, &tools(), &vmf(), opts).unwrap());
    assert_eq!(fin(&PlanOptions::default()), ["-final", "-StaticPropLighting"]);
    let off = PlanOptions { static_prop_lighting: false, static_prop_shadows: false, ..PlanOptions::default() };
    assert_eq!(fin(&off), ["-final", "-nostaticpropshadows"]);
    let forced = PlanOptions {
        extra_rad_args: "-staticproplighting -nostaticpropshadows -softsun 1".to_owned(),
        rad_flags: vec![FlagChoice { flag: "-StaticPropLighting".into(), on: true, value: String::new() }],
        ..off.clone()
    };
    let flags = fin(&forced);
    assert!(!flags.iter().any(|a| a.eq_ignore_ascii_case("-StaticPropLighting")), "{flags:?}");
    assert_eq!(flags.iter().filter(|a| a.eq_ignore_ascii_case("-nostaticpropshadows")).count(), 1, "{flags:?}");
    let normal_lit = PlanOptions { rad_flags: forced.rad_flags.clone(), ..PlanOptions::default() };
    let plan = CompilePlan::preset(Preset::Normal, &tools(), &vmf(), &normal_lit).unwrap();
    assert_eq!(rad_flags_of(&plan), ["-StaticPropLighting"]);
    let old: PlanOptions = serde_json::from_str(r#"{"threads":2}"#).unwrap();
    assert!(old.static_prop_lighting && old.static_prop_shadows && old.rad_flags.is_empty());
}

#[test]
#[cfg(windows)]
fn flag_choices_in_steps() {
    let choice = |flag: &str, on: bool, value: &str| FlagChoice { flag: flag.into(), on, value: value.into() };
    let opts = PlanOptions {
        threads: Some(4),
        extra_bsp_args: "-nodetail -luxelscale 2".to_owned(),
        extra_rad_args: "-bounce 4 -softsun 2".to_owned(),
        bsp_flags: vec![choice("-luxelscale", true, "4"), choice("-forceskyvis", true, ""), choice("-vis", false, "")],
        rad_flags: vec![choice("-fast", false, ""), choice("-extrasky", true, "2")],
        stop_on_leak: true,
        ..PlanOptions::default()
    };
    let plan = CompilePlan::preset(Preset::Draft, &tools(), &vmf(), &opts).unwrap();
    let bsp = args(&plan, StepKind::Bsp);
    assert_eq!(
        bsp[3..13],
        [
            "-fastvis",
            "-leaktest",
            "-AllowDynamicPropsAsStatic",
            "-luxelscale",
            "4",
            "-forceskyvis",
            "-threads",
            "4",
            "-nodetail",
            "-game"
        ]
    );
    assert_eq!(rad_flags_of(&plan), ["-extrasky", "2", "-threads", "4", "-bounce", "4", "-softsun", "2"]);
    for st in &plan.steps {
        let mut flags: Vec<String> =
            st.args.iter().filter(|a| a.starts_with('-')).map(|a| a.to_ascii_lowercase()).collect();
        let n = flags.len();
        flags.sort();
        flags.dedup();
        assert_eq!(flags.len(), n, "no duplicate in {:?}", st.args);
    }
}

#[test]
#[cfg(windows)]
fn another_game_gets_its_target_and_no_gmod_root() {
    let tf = ToolPaths {
        compiler: PathBuf::from(EXE),
        game_exe: Some(PathBuf::from(r"D:\Steam\Team Fortress 2\tf_win64.exe")),
        game_dir: PathBuf::from(r"D:\Steam\Team Fortress 2\tf"),
        maps_dir: PathBuf::from(r"D:\Steam\Team Fortress 2\tf\maps"),
    };
    let opts = PlanOptions { target: Some("tf".to_owned()), ..PlanOptions::default() };
    let plan = CompilePlan::preset(Preset::Draft, &tf, &vmf(), &opts).unwrap();
    let game = r"D:\Steam\Team Fortress 2\tf";
    let map = r"C:\My Maps\gm_test";
    assert_eq!(
        args(&plan, StepKind::Bsp),
        vec!["--bspc", "-fastvis", "-AllowDynamicPropsAsStatic", "-target", "tf", "-game", game, map]
    );
    assert_eq!(
        args(&plan, StepKind::Rad),
        vec!["--rad", "-ldr", "-bounce", "1", "-fast", "-target", "tf", "-game", game, map]
    );
    let gmod = PlanOptions { target: Some("gmod".to_owned()), ..PlanOptions::default() };
    let a = CompilePlan::preset(Preset::Draft, &tools(), &vmf(), &gmod).unwrap();
    let b = CompilePlan::preset(Preset::Draft, &tools(), &vmf(), &PlanOptions::default()).unwrap();
    assert_eq!(
        a.steps.iter().map(|s| s.args.clone()).collect::<Vec<_>>(),
        b.steps.iter().map(|s| s.args.clone()).collect::<Vec<_>>()
    );
}

#[test]
#[cfg(windows)]
fn the_cache_keeps_the_visibility_of_the_builtin_compiler() {
    let builds = PathBuf::from(r"C:\cache\builds");
    let plan = CompilePlan::preset(Preset::Normal, &tools(), &vmf(), &PlanOptions::default()).unwrap();
    let keys = plan.clone().with_cache(3, builds.clone(), "gm test", 0);
    let bsp = args(&keys, StepKind::Bsp);
    let at = bsp.iter().position(|a| a == crate::cache::VIS_CACHE_ARG).expect("the visibility cache folder is given");
    assert_eq!(bsp[at + 1], builds.join("gm_test").to_string_lossy());
    assert_eq!(bsp.last().map(String::as_str), Some(r"C:\My Maps\gm_test"));
    assert!(!args(&keys, StepKind::Rad).iter().any(|a| a == crate::cache::VIS_CACHE_ARG));
    let mut moved = plan.clone();
    for step in &mut moved.steps {
        step.args.retain(|a| a != "-threads");
    }
    let other = moved.with_cache(3, PathBuf::from(r"D:\elsewhere"), "gm test", 0);
    assert_eq!(keys.cache.as_ref().map(|c| c.steps.clone()), other.cache.as_ref().map(|c| c.steps.clone()));
    let again = keys.clone().with_cache(3, builds.clone(), "gm test", 0);
    assert_eq!(args(&again, StepKind::Bsp).iter().filter(|a| *a == crate::cache::VIS_CACHE_ARG).count(), 1);

    let modes = StepModes::new(BspMode::Full, VisMode::Off, RadMode::Normal);
    let opts = PlanOptions { modes: Some(modes), ..PlanOptions::default() };
    let no_vis =
        CompilePlan::preset(Preset::Normal, &tools(), &vmf(), &opts).unwrap().with_cache(3, builds.clone(), "m", 0);
    assert!(!args(&no_vis, StepKind::Bsp).iter().any(|a| a == crate::cache::VIS_CACHE_ARG));

    let mut external = plan;
    if let Some(step) = external.step_mut(StepKind::Bsp) {
        step.exe = PathBuf::from(r"C:\sdk\vbsp.exe");
        step.args.retain(|a| a != BSPC_ARG);
    }
    let external = external.with_cache(3, builds, "m", 0);
    assert!(!args(&external, StepKind::Bsp).iter().any(|a| a == crate::cache::VIS_CACHE_ARG));
}
