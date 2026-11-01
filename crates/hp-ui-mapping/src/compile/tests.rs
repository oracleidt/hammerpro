use std::path::PathBuf;

use hp_app::testing::{run_ctx, test_state};
use hp_compile::{CompileEvent, CompilePlan, LogLocation, Severity as LogSeverity, Step, StepKind, ToolPaths};
use hp_editor::ObjRef;

use super::*;

fn plan() -> CompilePlan {
    let mut plan = CompilePlan::new("C:/maps/gm_test.vmf");
    plan.steps.push(Step::new(StepKind::Bsp, "hammerpro.exe", vec!["--bspc".into(), "gm_test".into()]));
    plan.steps.push(Step::new(StepKind::Rad, "hammerpro.exe", vec!["--rad".into(), "gm_test".into()]));
    plan.steps.push(Step::pack(
        std::path::Path::new("C:/maps/gm_test.bsp"),
        hp_compile::StepAction::Pack { files: Vec::new() },
    ));
    plan
}

fn feed(d: &mut CompileDialog, state: &mut AppState, events: Vec<CompileEvent>) {
    for ev in events {
        d.handle_event(ev, state);
    }
}

#[test]
fn events_update_steps_log_and_console() {
    let mut state = test_state();
    let mut d = CompileDialog::new();
    d.begin_test_run(&plan());
    let console_before = state.console.len();
    feed(
        &mut d,
        &mut state,
        vec![
            CompileEvent::StepStarted {
                index: 0,
                kind: StepKind::Bsp,
                command_line: "hammerpro --bspc gm_test".into(),
            },
            CompileEvent::Progress { step: 0, fraction: 0.3, phase: Some("ProcessBlock".into()) },
            CompileEvent::Log {
                step: 0,
                line: "Brush 3: WARNING, Brush with win 0, Leaf".into(),
                severity: LogSeverity::Warning,
                location: Some(LogLocation { brush_id: Some(3), ..LogLocation::default() }),
            },
            CompileEvent::StepFinished { index: 0, success: true, exit_code: Some(0), duration_ms: 900 },
            CompileEvent::StepStarted { index: 1, kind: StepKind::Rad, command_line: "hammerpro --rad".into() },
            CompileEvent::Log { step: 1, line: "Error! oops".into(), severity: LogSeverity::Error, location: None },
            CompileEvent::StepFinished { index: 1, success: false, exit_code: Some(1), duration_ms: 50 },
            CompileEvent::Finished { success: false, cancelled: false, leaked: false, total_ms: 1000 },
        ],
    );
    let run = d.run.as_ref().expect("run");
    assert_eq!(run.steps[0].status, StepStatus::Done);
    assert_eq!(run.steps[1].status, StepStatus::Failed);
    assert_eq!(run.steps[2].status, StepStatus::Skipped);
    assert_eq!((run.warnings, run.errors), (1, 1));
    assert!(run.summary.is_some_and(|s| !s.success));
    assert_eq!(state.console.len(), console_before + 3);
    assert!(!d.is_running());

    let loc = run.log.iter().find_map(|l| l.location.clone()).expect("location");
    assert!(goto_location(&mut state, &loc));
    assert_eq!(state.doc.selected_objects(), vec![ObjRef::Solid(3)]);
    assert!(state.requested_actions.contains(&Action::Center2dOnSelection));
    assert!(state.requested_actions.contains(&Action::Center3dOnSelection));
}

#[test]
fn leak_loads_pointfile() {
    let dir = crate::test_scratch("compile_leak");
    let lin = dir.join("gm_test.lin");
    std::fs::write(&lin, "0 0 0\n64 0 0\n64 128 32\n").expect("write lin");
    let mut state = test_state();
    state.settings.compile.load_pointfile_on_leak = true;
    state.doc.set_path(Some(plan().vmf_path));
    let mut d = CompileDialog::new();
    d.begin_test_run(&plan());
    feed(
        &mut d,
        &mut state,
        vec![
            CompileEvent::StepStarted { index: 0, kind: StepKind::Bsp, command_line: "hammerpro --bspc".into() },
            CompileEvent::LeakDetected { pointfile: lin.clone() },
            CompileEvent::StepFinished { index: 0, success: true, exit_code: Some(0), duration_ms: 10 },
            CompileEvent::Finished { success: false, cancelled: false, leaked: true, total_ms: 20 },
        ],
    );
    let pf = state.pointfile.as_ref().expect("pointfile loaded");
    assert_eq!(pf.points.len(), 3);
    assert_eq!(pf.path, lin);
    assert_eq!(pf.points[2], DVec3::new(64.0, 128.0, 32.0));
    assert_eq!(d.run.as_ref().and_then(|r| r.leak_file.clone()), Some(lin));

    let mut state2 = test_state();
    state2.settings.compile.load_pointfile_on_leak = false;
    let mut d2 = CompileDialog::new();
    d2.begin_test_run(&plan());
    d2.handle_event(CompileEvent::LeakDetected { pointfile: dir.join("gm_test.lin") }, &mut state2);
    assert!(state2.pointfile.is_none());

    assert!(!load_pointfile(&mut state2, &dir.join("missing.lin")));
}

#[test]
fn log_locations() {
    let mut state = test_state();
    let loc = LogLocation { entity_id: Some(2), ..LogLocation::default() };
    assert!(goto_location(&mut state, &loc));
    assert_eq!(state.doc.selected_objects(), vec![ObjRef::Entity(5)]);
    let loc = LogLocation {
        entity_class: Some("info_player_start".into()),
        position: Some([0.0, -128.0, 0.0]),
        ..LogLocation::default()
    };
    assert!(goto_location(&mut state, &loc));
    assert_eq!(state.doc.selected_objects(), vec![ObjRef::Entity(4)]);
    let loc = LogLocation { side_id: Some(14), ..LogLocation::default() };
    assert!(goto_location(&mut state, &loc));
    assert_eq!(state.doc.selected_objects(), vec![ObjRef::Solid(7)]);
    assert_eq!(state.doc.selected_faces().len(), 1);
    let loc = LogLocation { position: Some([10.0, 20.0, 30.0]), ..LogLocation::default() };
    assert!(goto_location(&mut state, &loc));
    assert_eq!(state.cameras.top.center, glam::DVec2::new(10.0, 20.0));
    let loc = LogLocation { brush_id: Some(999), ..LogLocation::default() };
    assert!(!goto_location(&mut state, &loc));
    assert!(!location_text(&LogLocation { brush_id: Some(1), ..LogLocation::default() }).is_empty());
}

#[test]
fn a_map_never_saved_compiles_without_saving() {
    crate::register_translations();
    let mut state = test_state();
    state.settings.game.gmod_root = Some(crate::test_scratch("compile_untitled").join("no_game"));
    let ctx = egui::Context::default();
    let mut d = CompileDialog::new();
    assert!(state.doc.path().is_none());
    d.start(&ctx, &mut state);
    assert!(!state.requested_actions.contains(&Action::SaveMapAs), "no save needed: the compile uses a snapshot");
    assert!(d.is_running(), "the preparation runs in the background");
    assert_eq!(d.run.as_ref().map(|r| r.map_name.as_str()), Some("untitled"));
    d.start(&ctx, &mut state);
    d.cancel(&mut state);
    assert!(!d.is_running());

    let mut d = CompileDialog::new();
    state.settings.compile.save_before_compile = true;
    state.requested_actions.clear();
    d.start(&ctx, &mut state);
    assert!(state.requested_actions.contains(&Action::SaveMapAs));
    assert!(d.waiting_for_save());
    state.ui.show_compile = false;
    d.show(&ctx, &mut state);
    assert!(!d.waiting_for_save());

    let dir = crate::test_scratch("compile_start");
    state.doc.set_path(Some(dir.join("gm_start.vmf")));
    state.doc.mark_dirty();
    state.requested_actions.clear();
    let mut d = CompileDialog::new();
    d.start(&ctx, &mut state);
    assert!(state.requested_actions.contains(&Action::SaveMap));
    assert!(!d.waiting_for_save());
    assert!(d.is_running());
    d.cancel(&mut state);
}

#[test]
fn f9_compiles_without_the_window() {
    let _lang = crate::lang_lock();
    crate::register_translations();
    let mut state = test_state();
    state.settings.game.gmod_root = Some(crate::test_scratch("compile_f9").join("no_game"));
    let ctx = egui::Context::default();
    let mut d = CompileDialog::new();
    d.start_now(&ctx, &mut state);
    assert!(!state.ui.show_compile);
    assert!(d.is_running());
    d.publish_runtime(&mut state);
    assert!(state.runtime.compile.progress.is_some(), "the status bar shows the compile");
    let toasts = state.console.len();
    d.start_now(&ctx, &mut state);
    assert!(state.console.len() > toasts, "a second F9 tells a compile runs");
    let t0 = std::time::Instant::now();
    while d.is_running() && t0.elapsed() < std::time::Duration::from_secs(20) {
        d.poll(&ctx, &mut state);
        std::thread::sleep(std::time::Duration::from_millis(10));
    }
    run_ctx(&mut state, |ctx, state| d.show(ctx, state));
    assert!(state.runtime.compile.progress.is_none());
    assert_eq!(state.runtime.compile.badge, Some(hp_app::runtime::CompileBadge::Failed));
}

#[test]
fn leak_selects_the_leaking_entity() {
    let dir = crate::test_scratch("compile_leak_select");
    let lin = dir.join("gm_test.lin");
    std::fs::write(&lin, "0 0 0\n64 0 0\n0 -120 4\n").expect("write lin");
    let mut state = test_state();
    state.settings.compile.load_pointfile_on_leak = true;
    state.doc.set_path(Some(plan().vmf_path));
    let mut d = CompileDialog::new();
    d.begin_test_run(&plan());
    feed(
        &mut d,
        &mut state,
        vec![
            CompileEvent::StepStarted { index: 0, kind: StepKind::Bsp, command_line: "hammerpro --bspc".into() },
            CompileEvent::Log {
                step: 0,
                line: "Entity info_player_start (0.00 -128.00 0.00) leaked!".into(),
                severity: LogSeverity::Error,
                location: Some(LogLocation {
                    entity_class: Some("info_player_start".into()),
                    position: Some([0.0, -128.0, 0.0]),
                    ..LogLocation::default()
                }),
            },
            CompileEvent::LeakDetected { pointfile: lin.clone() },
        ],
    );
    assert!(state.pointfile.is_some());
    assert_eq!(state.doc.selected_objects(), vec![ObjRef::Entity(4)]);
    assert_eq!(state.runtime.compile.badge, Some(hp_app::runtime::CompileBadge::Leak));
    state.doc.set_selection(&[]);
    assert_eq!(leaking_entity(&state, None, Some(DVec3::new(0.0, -120.0, 4.0))), Some(4));
    assert!(leaking_entity(&state, None, None).is_none());
}

#[test]
fn smoke_settings_and_history() {
    let _lang = crate::lang_lock();
    crate::register_translations();
    let mut state = test_state();
    state.ui.show_compile = true;
    let cordons = hp_vmf::Cordons {
        active: true,
        cordons: vec![hp_vmf::Cordon {
            name: "c".into(),
            active: true,
            mins: DVec3::splat(-64.0),
            maxs: DVec3::splat(64.0),
        }],
        ..hp_vmf::Cordons::default()
    };
    state.doc.set_cordons(cordons).expect("cordons");
    let mut d = CompileDialog::new();
    for lang in hp_app::i18n::Lang::ALL {
        hp_app::i18n::set_lang(lang);
        for preset in [
            hp_app::settings::CompilePresetSetting::Draft,
            hp_app::settings::CompilePresetSetting::Final,
            hp_app::settings::CompilePresetSetting::Custom,
        ] {
            plan::apply_preset(&mut state.settings.compile, preset);
            run_ctx(&mut state, |ctx, state| d.show(ctx, state));
        }
        d.tab = Tab::History;
        run_ctx(&mut state, |ctx, state| d.show(ctx, state));
        d.tab = Tab::Log;
    }
    hp_app::i18n::set_lang(hp_app::i18n::Lang::En);
    state.ui.show_compile = false;
    state.runtime.compile.show_history = true;
    run_ctx(&mut state, |ctx, state| d.show(ctx, state));
    assert!(state.ui.show_compile && d.tab == Tab::History);
}

#[test]
fn preparation_failure_is_reported() {
    let mut state = test_state();
    let dir = crate::test_scratch("compile_prep_fail");
    let vmf = dir.join("gm_prep.vmf");
    state.doc.save(&vmf).expect("save scratch map");
    state.settings.game.gmod_root = Some(dir.join("not_a_game"));
    let ctx = egui::Context::default();
    let mut d = CompileDialog::new();
    d.start(&ctx, &mut state);
    assert!(d.is_running(), "preparation runs in the background");
    let t0 = std::time::Instant::now();
    while d.is_running() && t0.elapsed() < std::time::Duration::from_secs(20) {
        d.poll(&ctx, &mut state);
        std::thread::sleep(std::time::Duration::from_millis(10));
    }
    assert!(!d.is_running());
    let run = d.run.as_ref().expect("run");
    assert!(run.summary.is_some_and(|s| !s.success));
    assert!(run.errors >= 1);
    assert!(d.job.is_none(), "no process was started");
}

#[test]
fn summaries() {
    let _lang = crate::lang_lock();
    crate::register_translations();
    let ok = Summary { success: true, cancelled: false, leaked: false, total_ms: 2000 };
    assert_eq!(summary_text(&ok, "gm_x").0, Severity::Success);
    let leak = Summary { success: false, cancelled: false, leaked: true, total_ms: 2000 };
    assert_eq!(summary_text(&leak, "gm_x").0, Severity::Error);
    let cancel = Summary { success: false, cancelled: true, leaked: false, total_ms: 2000 };
    assert_eq!(summary_text(&cancel, "gm_x").0, Severity::Warning);
    let failed = Summary { success: false, cancelled: false, leaked: false, total_ms: 2000 };
    assert_eq!(summary_text(&failed, "my_map").1, "Compiling my_map failed");
    assert_eq!(counts_text(0, 1), "0 warnings, 1 error");
    assert_eq!(counts_text(1, 2), "1 warning, 2 errors");
    hp_app::i18n::set_lang(hp_app::i18n::Lang::Fr);
    assert_eq!(counts_text(0, 1), "0 avertissement, 1 erreur");
    assert_eq!(counts_text(3, 2), "3 avertissements, 2 erreurs");
    hp_app::i18n::set_lang(hp_app::i18n::Lang::En);
    for k in
        [StepKind::Bsp, StepKind::Rad, StepKind::Pack, StepKind::CopyToMaps, StepKind::LaunchGame, StepKind::Custom]
    {
        assert!(!step_name(k).starts_with("compile."), "{k:?} is translated");
    }
}

#[test]
fn smoke_dialog_states() {
    let _lang = crate::lang_lock();
    crate::register_translations();
    let mut state = test_state();
    state.ui.show_compile = true;
    let mut d = CompileDialog::new();
    run_ctx(&mut state, |ctx, state| d.show(ctx, state));
    assert!(state.ui.show_compile);
    d.begin_test_run(&plan());
    feed(
        &mut d,
        &mut state,
        vec![
            CompileEvent::StepStarted { index: 0, kind: StepKind::Bsp, command_line: "hammerpro --bspc".into() },
            CompileEvent::Progress { step: 0, fraction: 0.4, phase: Some("ProcessWorld".into()) },
            CompileEvent::Log {
                step: 0,
                line: "Entity 2 (0 0 96) leaked!".into(),
                severity: LogSeverity::Error,
                location: Some(LogLocation {
                    entity_id: Some(2),
                    position: Some([0.0, 0.0, 96.0]),
                    ..LogLocation::default()
                }),
            },
        ],
    );
    for i in 0..300 {
        d.handle_event(
            CompileEvent::Log { step: 0, line: format!("line {i}"), severity: LogSeverity::Info, location: None },
            &mut state,
        );
    }
    d.log_view.filter.search = "line 1".into();
    run_ctx(&mut state, |ctx, state| d.show(ctx, state));
    assert!(d.log_view.rows.len() > 10);
    d.log_view.filter = run::LogFilter { info: false, ..run::LogFilter::default() };
    run_ctx(&mut state, |ctx, state| d.show(ctx, state));
    assert_eq!(d.log_view.rows.len(), 1);
    d.handle_event(CompileEvent::LeakDetected { pointfile: PathBuf::from("C:/nowhere/gm_test.lin") }, &mut state);
    d.handle_event(
        CompileEvent::Finished { success: false, cancelled: false, leaked: true, total_ms: 5000 },
        &mut state,
    );
    run_ctx(&mut state, |ctx, state| d.show(ctx, state));
    state.ui.show_compile = false;
    run_ctx(&mut state, |ctx, state| d.show(ctx, state));
    hp_app::i18n::set_lang(hp_app::i18n::Lang::Fr);
    state.ui.show_compile = true;
    run_ctx(&mut state, |ctx, state| d.show(ctx, state));
    hp_app::i18n::set_lang(hp_app::i18n::Lang::En);
}

#[test]
fn results_only_apply_to_the_compiled_map() {
    let dir = crate::test_scratch("compile_other_map");
    let lin = dir.join("gm_a.lin");
    std::fs::write(&lin, "0 0 0\n64 0 0\n").expect("write lin");
    let mut state = test_state();
    state.settings.compile.load_pointfile_on_leak = true;
    state.doc.set_path(Some(dir.join("gm_b.vmf")));
    let mut d = CompileDialog::new();
    let mut plan = plan();
    plan.vmf_path = dir.join("gm_a.vmf");
    d.begin_test_run(&plan);
    assert!(!d.compiled_map_is_open(&state));
    d.handle_event(CompileEvent::LeakDetected { pointfile: lin.clone() }, &mut state);
    assert!(state.pointfile.is_none(), "the leak of gm_a is not drawn over gm_b");
    let mut state2 = test_state();
    state2.settings.compile.load_pointfile_on_leak = true;
    let spelled = dir.join(".").join("GM_A.vmf");
    state2.doc.set_path(Some(if cfg!(windows) { spelled } else { dir.join("gm_a.vmf") }));
    assert!(d.compiled_map_is_open(&state2));
    d.handle_event(CompileEvent::LeakDetected { pointfile: lin }, &mut state2);
    assert!(state2.pointfile.is_some());
    d.handle_event(CompileEvent::Finished { success: false, cancelled: false, leaked: true, total_ms: 5 }, &mut state);
    state.ui.show_compile = true;
    run_ctx(&mut state, |ctx, state| d.show(ctx, state));
}

#[test]
fn notices_are_logged_translated() {
    let _lang = crate::lang_lock();
    crate::register_translations();
    let mut state = test_state();
    let mut d = CompileDialog::new();
    d.begin_test_run(&plan());
    hp_app::i18n::set_lang(hp_app::i18n::Lang::Fr);
    feed(
        &mut d,
        &mut state,
        vec![
            CompileEvent::StepStarted { index: 0, kind: StepKind::Bsp, command_line: "hammerpro --bspc".into() },
            CompileEvent::Notice {
                step: 0,
                notice: hp_compile::Notice::Leaked { pointfile: PathBuf::from("C:/maps/gm_test.lin") },
            },
            CompileEvent::StepFinished { index: 0, success: true, exit_code: Some(0), duration_ms: 10 },
            CompileEvent::StepSkipped { index: 1, kind: StepKind::Rad },
            CompileEvent::Notice { step: 1, notice: hp_compile::Notice::NothingToPack },
        ],
    );
    hp_app::i18n::set_lang(hp_app::i18n::Lang::En);
    let run = d.run.as_ref().expect("run");
    assert_eq!(run.steps[1].status, StepStatus::Skipped);
    assert!(run.log.iter().any(|l| l.text.starts_with("La map fuit")), "{:?}", run.log);
    assert!(run.log.iter().any(|l| l.text == "Rien à empaqueter."));
}

fn window_rects(d: &mut CompileDialog, state: &mut AppState, frames: usize) -> Vec<egui::Rect> {
    let ctx = egui::Context::default();
    let mut rects = Vec::new();
    for _ in 0..frames {
        let input = egui::RawInput {
            screen_rect: Some(egui::Rect::from_min_size(egui::Pos2::ZERO, egui::vec2(1600.0, 960.0))),
            ..Default::default()
        };
        let mut out = ctx.run_ui(input, |ui| d.show(ui.ctx(), state));
        out.textures_delta.clear();
        if let Some(r) = ctx.memory(|m| m.area_rect(window_id())) {
            rects.push(r);
        }
    }
    rects
}

#[test]
fn compile_window_keeps_its_size() {
    crate::register_translations();
    let mut state = test_state();
    state.ui.show_compile = true;
    let mut d = CompileDialog::new();
    let idle = window_rects(&mut d, &mut state, 60);
    let (a, b) = (idle[5], *idle.last().expect("rects"));
    assert!((a.height() - b.height()).abs() < 0.5, "idle: {a:?} -> {b:?}");
    assert!(b.bottom() <= 960.0, "{b:?}");

    d.begin_test_run(&plan());
    d.collapse_settings = true;
    feed(
        &mut d,
        &mut state,
        vec![CompileEvent::StepStarted { index: 0, kind: StepKind::Bsp, command_line: "hammerpro --bspc".into() }],
    );
    for i in 0..3000 {
        d.handle_event(
            CompileEvent::Log { step: 0, line: format!("line {i}"), severity: LogSeverity::Info, location: None },
            &mut state,
        );
    }
    let running = window_rects(&mut d, &mut state, 80);
    eprintln!("idle {:?} running {:?} -> {:?}", idle.last(), running[5], running.last());
    let (a, b) = (running[5], *running.last().expect("rects"));
    assert!((a.height() - b.height()).abs() < 0.5, "running: {a:?} -> {b:?}");
    assert!(b.bottom() <= 960.0 && b.height() < 900.0, "the window did not fill the screen: {b:?}");
    assert!(d.is_running() || d.run.is_some());
}

#[test]
fn compiler_line_is_translated() {
    let _lang = crate::lang_lock();
    crate::register_translations();
    let tools = ToolPaths {
        compiler: PathBuf::from("C:/HammerPro/hammerpro.exe"),
        game_exe: None,
        game_dir: PathBuf::from("C:/Games/GarrysMod/garrysmod"),
        maps_dir: PathBuf::from("C:/Games/GarrysMod/garrysmod/maps"),
    };
    let dir = tools.game_dir.display().to_string();
    let version = env!("CARGO_PKG_VERSION");
    hp_app::i18n::set_lang(hp_app::i18n::Lang::En);
    let en = tools::tools_text(&tools);
    hp_app::i18n::set_lang(hp_app::i18n::Lang::Fr);
    let fr = tools::tools_text(&tools);
    hp_app::i18n::set_lang(hp_app::i18n::Lang::En);
    assert_eq!(en, format!("Compiler: Hammer Pro {version} (built in), game: {dir}"));
    assert_eq!(fr, format!("Compilateur : Hammer Pro {version} (intégré), jeu : {dir}"));
}

#[test]
fn escape_closes_the_compile_dialog_with_the_pointer_over_a_view() {
    crate::register_translations();
    let mut state = test_state();
    let mut d = CompileDialog::new();
    let ctx = egui::Context::default();
    let frame = |d: &mut CompileDialog, state: &mut AppState, events: Vec<egui::Event>| {
        let input = egui::RawInput {
            screen_rect: Some(egui::Rect::from_min_size(egui::Pos2::ZERO, egui::vec2(1600.0, 960.0))),
            events,
            ..Default::default()
        };
        let mut left = false;
        let mut out = ctx.run_ui(input, |ui| {
            d.show(ui.ctx(), state);
            left = ui.input(|i| i.key_pressed(egui::Key::Escape));
        });
        out.textures_delta.clear();
        left
    };
    let escape = || egui::Event::Key {
        key: egui::Key::Escape,
        physical_key: None,
        pressed: true,
        repeat: false,
        modifiers: egui::Modifiers::NONE,
    };
    frame(&mut d, &mut state, vec![egui::Event::PointerMoved(egui::pos2(1590.0, 950.0))]);
    state.ui.show_compile = true;
    d.begin_test_run(&plan());
    for _ in 0..3 {
        frame(&mut d, &mut state, Vec::new());
    }
    assert!(!frame(&mut d, &mut state, vec![escape()]), "Escape is consumed");
    assert!(!state.ui.show_compile, "the dialog closed");
    assert!(d.run.as_ref().is_some_and(|r| r.summary.is_none()), "the compile was not cancelled");
    frame(&mut d, &mut state, Vec::new());
    assert!(frame(&mut d, &mut state, vec![escape()]), "closed: Escape goes to the tools");
}

fn log_text_positions(d: &mut CompileDialog, state: &mut AppState, frames: usize) -> Vec<Vec<(String, i32, i32)>> {
    let ctx = egui::Context::default();
    hp_app::theme::apply(&ctx, state.settings.ui.theme, state.settings.ui.ui_scale);
    let mut out_frames = Vec::new();
    for _ in 0..frames {
        let input = egui::RawInput {
            screen_rect: Some(egui::Rect::from_min_size(egui::Pos2::ZERO, egui::vec2(1600.0, 960.0))),
            ..Default::default()
        };
        let mut out = ctx.run_ui(input, |ui| d.show(ui.ctx(), state));
        out.textures_delta.clear();
        let mut texts = Vec::new();
        for clipped in &out.shapes {
            if let egui::Shape::Text(t) = &clipped.shape
                && t.galley.text().starts_with("log line")
            {
                texts.push((t.galley.text().to_owned(), (t.pos.x * 4.0) as i32, (t.pos.y * 4.0) as i32));
            }
        }
        out_frames.push(texts);
    }
    out_frames
}

#[test]
fn the_idle_log_stays_still() {
    let _lang = crate::lang_lock();
    crate::register_translations();
    let mut state = test_state();
    state.ui.show_compile = true;
    state.settings.compile.separate_window = false;
    let mut d = CompileDialog::new();
    d.begin_test_run(&plan());
    d.collapse_settings = true;
    let mut events =
        vec![CompileEvent::StepStarted { index: 0, kind: StepKind::Bsp, command_line: "hammerpro --bspc".into() }];
    for i in 0..400 {
        let (line, severity, location) = if i % 7 == 0 {
            (format!("log line {i} Brush {i}: WARNING, microbrush"), LogSeverity::Warning, None)
        } else {
            (format!("log line {i}"), LogSeverity::Info, None)
        };
        events.push(CompileEvent::Log { step: 0, line, severity, location });
    }
    events.push(CompileEvent::StepFinished { index: 0, success: true, exit_code: Some(0), duration_ms: 900 });
    events.push(CompileEvent::Finished { success: true, cancelled: false, leaked: false, total_ms: 1000 });
    feed(&mut d, &mut state, events);
    let frames = log_text_positions(&mut d, &mut state, 40);
    let last = frames.last().expect("frames");
    assert!(!last.is_empty(), "the log is drawn");
    let ctx = egui::Context::default();
    hp_app::theme::apply(&ctx, state.settings.ui.theme, state.settings.ui.ui_scale);
    let mut pitch = 0.0;
    let _ = ctx.run_ui(egui::RawInput::default(), |ui| {
        pitch = d.log_view.row_height + ui.spacing().item_spacing.y;
    });
    let mut rows: Vec<(usize, i32)> = last
        .iter()
        .filter_map(|(t, _, y)| t.split_whitespace().nth(2).and_then(|n| n.parse().ok()).map(|n| (n, *y)))
        .collect();
    rows.sort_unstable();
    assert!(rows.len() > 5, "{rows:?}");
    for w in rows.windows(2) {
        let step = (w[1].1 - w[0].1) as f32 / 4.0 / (w[1].0 - w[0].0) as f32;
        assert!((step - pitch).abs() < 0.5, "rows are {step} apart, show_rows counts {pitch}: {rows:?}");
    }
    assert!(last.iter().any(|t| t.0.starts_with("log line 399")), "auto scroll shows the last line: {last:?}");
    for (i, f) in frames.iter().enumerate().skip(20) {
        assert_eq!(f, last, "frame {i} moved");
    }
    d.no_auto_scroll = true;
    let frames = log_text_positions(&mut d, &mut state, 30);
    let last = frames.last().expect("frames");
    for (i, f) in frames.iter().enumerate().skip(10) {
        assert_eq!(f, last, "frame {i} moved without auto scroll");
    }
}
