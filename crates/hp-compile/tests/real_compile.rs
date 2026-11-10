use std::fmt::Write as _;
use std::path::{Path, PathBuf};
use std::time::{Duration, Instant};

use hp_compile::{
    CompileEvent, CompilePlan, Notice, PlanOptions, Preset, Severity, StepKind, ToolPaths, read_pointfile,
};

const GMOD: &str = r"D:\SteamLibrary\steamapps\common\GarrysMod";

fn workspace() -> PathBuf {
    let manifest = Path::new(env!("CARGO_MANIFEST_DIR"));
    manifest.parent().and_then(Path::parent).unwrap_or(manifest).to_path_buf()
}

fn scratch(name: &str) -> PathBuf {
    let dir = workspace().join("target").join("hp-compile").join("scratch").join("integration").join(name);
    let _ = std::fs::remove_dir_all(&dir);
    std::fs::create_dir_all(&dir).expect("create scratch dir");
    dir
}

fn tools() -> Option<ToolPaths> {
    let exe = std::env::var_os("HAMMERPRO_EXE").map(PathBuf::from).unwrap_or_else(|| {
        workspace().join("target").join("release").join(if cfg!(windows) { "hammerpro.exe" } else { "hammerpro" })
    });
    if !exe.is_file() {
        eprintln!("{} not found, skipping", exe.display());
        return None;
    }
    let tools = ToolPaths::with_compiler(exe, Path::new(GMOD));
    if tools.is_none() {
        eprintln!("Garry's Mod not found at {GMOD}, skipping");
    }
    tools
}

fn run(plan: CompilePlan, workdir: &Path) -> Vec<(Duration, CompileEvent)> {
    let t0 = Instant::now();
    let handle = hp_compile::start(plan, workdir);
    let mut events = Vec::new();
    let deadline = t0 + Duration::from_secs(600);
    while Instant::now() < deadline {
        match handle.events.recv_timeout(Duration::from_millis(200)) {
            Ok(e) => {
                let at = t0.elapsed();
                match &e {
                    CompileEvent::Log { line, severity, location, .. } => {
                        let tag = match severity {
                            Severity::Info => "    ",
                            Severity::Success => " OK ",
                            Severity::Warning => "WARN",
                            Severity::Error => "ERR ",
                        };
                        let mut s = format!("{:>8.3}s [{tag}] {line}", at.as_secs_f64());
                        if let Some(loc) = location {
                            let _ = write!(s, "   -> {loc:?}");
                        }
                        eprintln!("{s}");
                    }
                    CompileEvent::Progress { .. } => {}
                    other => eprintln!("{:>8.3}s {other:?}", at.as_secs_f64()),
                }
                events.push((at, e));
            }
            Err(crossbeam_channel::RecvTimeoutError::Disconnected) => break,
            Err(crossbeam_channel::RecvTimeoutError::Timeout) => {}
        }
    }
    handle.wait();
    events
}

fn finished(events: &[(Duration, CompileEvent)]) -> (bool, bool, bool, u64) {
    match events.last().map(|e| &e.1) {
        Some(CompileEvent::Finished { success, cancelled, leaked, total_ms }) => {
            (*success, *cancelled, *leaked, *total_ms)
        }
        other => panic!("last event is not Finished: {other:?}"),
    }
}

fn step_summary(plan: &CompilePlan, events: &[(Duration, CompileEvent)]) {
    for (_, e) in events {
        if let CompileEvent::StepFinished { index, success, exit_code, duration_ms } = e {
            let kind = plan.steps.get(*index).map(|s| s.kind);
            let progress = events
                .iter()
                .filter(|(_, ev)| matches!(ev, CompileEvent::Progress { step, .. } if step == index))
                .count();
            eprintln!(
                "step {index} {kind:?}: success={success} exit={exit_code:?} {duration_ms} ms, {progress} progress events"
            );
        }
    }
}

fn solid(out: &mut String, id: &mut u32, min: [i32; 3], max: [i32; 3], material: &str) {
    let [x0, y0, z0] = min;
    let [x1, y1, z1] = max;
    let faces = [
        (format!("({x0} {y1} {z1}) ({x1} {y1} {z1}) ({x1} {y0} {z1})"), "[1 0 0 0] 0.25", "[0 -1 0 0] 0.25"),
        (format!("({x0} {y0} {z0}) ({x1} {y0} {z0}) ({x1} {y1} {z0})"), "[1 0 0 0] 0.25", "[0 -1 0 0] 0.25"),
        (format!("({x0} {y1} {z1}) ({x0} {y0} {z1}) ({x0} {y0} {z0})"), "[0 1 0 0] 0.25", "[0 0 -1 0] 0.25"),
        (format!("({x1} {y1} {z0}) ({x1} {y0} {z0}) ({x1} {y0} {z1})"), "[0 1 0 0] 0.25", "[0 0 -1 0] 0.25"),
        (format!("({x1} {y1} {z1}) ({x0} {y1} {z1}) ({x0} {y1} {z0})"), "[1 0 0 0] 0.25", "[0 0 -1 0] 0.25"),
        (format!("({x1} {y0} {z0}) ({x0} {y0} {z0}) ({x0} {y0} {z1})"), "[1 0 0 0] 0.25", "[0 0 -1 0] 0.25"),
    ];
    let solid_id = *id;
    *id += 1;
    let _ = writeln!(out, "\tsolid\n\t{{\n\t\t\"id\" \"{solid_id}\"");
    for (plane, u, v) in faces {
        let side_id = *id;
        *id += 1;
        let _ = writeln!(
            out,
            "\t\tside\n\t\t{{\n\t\t\t\"id\" \"{side_id}\"\n\t\t\t\"plane\" \"{plane}\"\n\t\t\t\"material\" \"{material}\"\n\t\t\t\"uaxis\" \"{u}\"\n\t\t\t\"vaxis\" \"{v}\"\n\t\t\t\"rotation\" \"0\"\n\t\t\t\"lightmapscale\" \"16\"\n\t\t\t\"smoothing_groups\" \"0\"\n\t\t}}"
        );
    }
    let _ = writeln!(out, "\t}}");
}

fn room_vmf(hole: bool) -> String {
    let mut out = String::from(
        "versioninfo\n{\n\t\"editorversion\" \"400\"\n\t\"editorbuild\" \"8000\"\n\t\"mapversion\" \"1\"\n\t\"formatversion\" \"100\"\n\t\"prefab\" \"0\"\n}\nworld\n{\n\t\"id\" \"1\"\n\t\"mapversion\" \"1\"\n\t\"classname\" \"worldspawn\"\n\t\"skyname\" \"sky_day01_01\"\n",
    );
    let mut id = 2;
    let m = "DEV/DEV_MEASUREGENERIC01";
    solid(&mut out, &mut id, [-256, -256, -16], [256, 256, 0], m);
    solid(&mut out, &mut id, [-256, -256, 256], [256, 256, 272], m);
    solid(&mut out, &mut id, [-272, -256, 0], [-256, 256, 256], m);
    solid(&mut out, &mut id, [-256, 256, 0], [256, 272, 256], m);
    solid(&mut out, &mut id, [-256, -272, 0], [256, -256, 256], m);
    solid(&mut out, &mut id, [256, -256, 0], [272, -32, 256], m);
    solid(&mut out, &mut id, [256, 32, 0], [272, 256, 256], m);
    solid(&mut out, &mut id, [256, -32, 0], [272, 32, 64], m);
    solid(&mut out, &mut id, [256, -32, 128], [272, 32, 256], m);
    if !hole {
        solid(&mut out, &mut id, [256, -32, 64], [272, 32, 128], m);
    }
    out.push_str("}\n");
    let _ = write!(
        out,
        "entity\n{{\n\t\"id\" \"{id}\"\n\t\"classname\" \"info_player_start\"\n\t\"angles\" \"0 0 0\"\n\t\"origin\" \"0 0 16\"\n}}\n"
    );
    let _ = write!(
        out,
        "entity\n{{\n\t\"id\" \"{}\"\n\t\"classname\" \"light\"\n\t\"_light\" \"255 255 255 200\"\n\t\"origin\" \"0 0 200\"\n}}\n",
        id + 1
    );
    out
}

#[test]
#[ignore = "needs Garry's Mod and a built hammerpro; run with --ignored"]
fn draft_compile_packs_and_copies() {
    let Some(mut tools) = tools() else { return };
    let dir = scratch("draft");
    tools.maps_dir = dir.join("maps");
    let vmf = dir.join("hp_room.vmf");
    std::fs::write(&vmf, room_vmf(false)).expect("write vmf");
    let content = dir.join("content/materials/hammerpro_test");
    std::fs::create_dir_all(&content).expect("content dir");
    std::fs::write(
        content.join("packed.vmt"),
        "\"UnlitGeneric\"\n{\n\t\"$basetexture\" \"dev/dev_measuregeneric01\"\n}\n",
    )
    .expect("write vmt");
    let pack_files = hp_compile::pack_files_from_dir(&dir.join("content")).expect("walk content");
    let opts = PlanOptions { pack_files, stop_on_leak: true, ..PlanOptions::default() };
    let plan = CompilePlan::preset(Preset::Draft, &tools, &vmf, &opts).expect("plan");
    assert_eq!(
        plan.steps.iter().map(|s| s.kind).collect::<Vec<_>>(),
        [StepKind::Bsp, StepKind::Rad, StepKind::Pack, StepKind::CopyToMaps]
    );
    for step in &plan.steps {
        eprintln!("plan: {}", step.command_line());
    }
    let plan_copy = plan.clone();
    let events = run(plan, &dir);
    step_summary(&plan_copy, &events);
    let (success, cancelled, leaked, total_ms) = finished(&events);
    eprintln!("total: {total_ms} ms");
    assert!(success && !cancelled && !leaked);
    let bsp = dir.join("hp_room.bsp");
    let read = hp_bsp::Bsp::read(&bsp).expect("bsp");
    assert!(read.pakfile().expect("pak").contains("materials/hammerpro_test/packed.vmt"));
    assert!(dir.join("maps/hp_room.bsp").is_file(), "copy to maps failed");
    assert!(
        events.iter().any(|(_, e)| matches!(e, CompileEvent::Notice { notice: Notice::Packed { count: 1, .. }, .. }))
    );
}

#[test]
#[ignore = "needs Garry's Mod and a built hammerpro; run with --ignored"]
fn leaking_map_is_detected() {
    let Some(mut tools) = tools() else { return };
    let dir = scratch("leak");
    tools.maps_dir = dir.join("maps");
    let vmf = dir.join("hp_leaktest.vmf");
    std::fs::write(&vmf, room_vmf(true)).expect("write vmf");
    let opts = PlanOptions { copy_to_maps: true, stop_on_leak: true, ..PlanOptions::default() };
    let plan = CompilePlan::preset(Preset::Draft, &tools, &vmf, &opts).expect("plan");
    assert!(plan.steps[0].args.contains(&"-leaktest".to_owned()));
    let events = run(plan, &dir);
    let (success, cancelled, leaked, _) = finished(&events);
    assert!(!success && !cancelled && leaked);
    assert!(events.iter().any(|(_, e)| matches!(e, CompileEvent::Notice { notice: Notice::StoppedOnLeak, .. })));
    assert!(!events.iter().any(|(_, e)| matches!(e, CompileEvent::StepStarted { index, .. } if *index > 0)));
    let pointfile = dir.join("hp_leaktest.lin");
    let points = read_pointfile(&pointfile).expect("read pointfile");
    assert!(points.len() >= 2);
    assert!(points.iter().any(|p| p[0] >= 256.0), "path does not reach the hole");
    assert!(events.iter().any(|(_, e)| matches!(
        e,
        CompileEvent::Log { severity: Severity::Error, location: Some(loc), .. }
            if matches!(loc.entity_class.as_deref(), Some("info_player_start" | "light"))
    )));
    assert!(!dir.join("maps").join("hp_leaktest.bsp").exists());
}

#[test]
#[ignore = "needs Garry's Mod and a built hammerpro; run with --ignored"]
fn only_entities_and_stage_cache() {
    let Some(mut tools) = tools() else { return };
    let dir = scratch("onlyents");
    tools.maps_dir = dir.join("maps");
    let vmf = dir.join("oe.vmf");
    std::fs::write(&vmf, room_vmf(false)).expect("write vmf");
    let opts = PlanOptions { copy_to_maps: false, ..PlanOptions::default() };
    let builds = dir.join("builds");
    let plan =
        CompilePlan::preset(Preset::Draft, &tools, &vmf, &opts).expect("plan").with_cache(5, builds.clone(), "oe", 0);
    let t0 = Instant::now();
    assert!(finished(&run(plan, &dir)).0);
    let full = t0.elapsed();
    let plan = CompilePlan::preset(Preset::Draft, &tools, &vmf, &opts).expect("plan").with_cache(5, builds, "oe", 0);
    let t0 = Instant::now();
    let events = run(plan, &dir);
    assert!(finished(&events).0);
    let cached = events
        .iter()
        .filter(|(_, e)| matches!(e, CompileEvent::Notice { notice: Notice::FromCache { .. }, .. }))
        .count();
    assert_eq!(cached, 2, "the BSP build and the lighting restored");
    eprintln!("full {full:?}, cached {:?}", t0.elapsed());
    let modes =
        hp_compile::StepModes::new(hp_compile::BspMode::OnlyEnts, hp_compile::VisMode::Off, hp_compile::RadMode::Off);
    let plan = CompilePlan::preset(Preset::Draft, &tools, &vmf, &PlanOptions { modes: Some(modes), ..opts.clone() })
        .expect("plan");
    assert_eq!(plan.steps.len(), 1);
    let events = run(plan, &dir);
    assert!(finished(&events).0, "-onlyents works on the earlier BSP");
}

#[test]
#[ignore = "needs Garry's Mod and a built hammerpro; run with --ignored"]
fn visibility_is_kept_between_compiles() {
    let Some(mut tools) = tools() else { return };
    let dir = scratch("viscache");
    tools.maps_dir = dir.join("maps");
    let vmf = dir.join("vc.vmf");
    std::fs::write(&vmf, room_vmf(false)).expect("write vmf");
    let builds = dir.join("builds");
    let modes =
        hp_compile::StepModes::new(hp_compile::BspMode::Full, hp_compile::VisMode::Full, hp_compile::RadMode::Off);
    let opts = PlanOptions { copy_to_maps: false, modes: Some(modes), ..PlanOptions::default() };
    let plan_for = |input: u64| {
        CompilePlan::preset(Preset::Normal, &tools, &vmf, &opts).expect("plan").with_cache(
            input,
            builds.clone(),
            "vc",
            0,
        )
    };
    let reused = |events: &[(Duration, CompileEvent)]| {
        events.iter().any(|(_, e)| matches!(e, CompileEvent::Log { line, .. } if line.contains("visibility reused")))
    };
    let vis_stats = |events: &[(Duration, CompileEvent)]| {
        let lines: Vec<&str> = events
            .iter()
            .filter_map(|(_, e)| match e {
                CompileEvent::Log { line, .. } => Some(line.as_str()),
                _ => None,
            })
            .collect();
        hp_compile::stats::parse_text(&lines.join("\n")).vis.expect("visibility statistics")
    };
    let kept = || {
        std::fs::read_dir(builds.join("vc"))
            .map(|r| r.flatten().filter(|e| e.path().extension().is_some_and(|x| x == "hpvis")).count())
            .unwrap_or(0)
    };
    let first = run(plan_for(1), &dir);
    assert!(finished(&first).0);
    assert!(!reused(&first), "nothing kept yet");
    let v = vis_stats(&first);
    assert_eq!((v.mode.as_str(), v.reused), ("full", false));
    assert_eq!(kept(), 1, "the visibility is kept in the build cache of the map");
    let bsp = std::fs::read(dir.join("vc.bsp")).expect("bsp");
    let second = run(plan_for(2), &dir);
    assert!(finished(&second).0);
    assert!(reused(&second), "a new input with the same portals reuses the visibility");
    let v = vis_stats(&second);
    assert_eq!((v.mode.as_str(), v.reused), ("full", true), "the statistics read a full visibility, reused");
    assert_eq!(v.times.iter().map(|t| t.name.as_str()).collect::<Vec<_>>(), ["reused"]);
    assert_eq!(std::fs::read(dir.join("vc.bsp")).expect("bsp"), bsp, "the same map");
}
