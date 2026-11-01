use std::path::{Component, Path, PathBuf};
use std::sync::mpsc::{self, Receiver, TryRecvError};

use egui::Ui;
use hp_app::settings::{CompileProfile, CompileSettings, ProfileStep};
use hp_app::{AppState, Severity, t, tf};
use hp_compile::cmdseq::{self, Sequence, Special};
use hp_compile::template::{TOKENS, Tokens, expand, expand_args};
use hp_compile::{BSPC_ARG, CompilePlan, RAD_ARG, Step, StepAction, StepKind, ToolPaths};

use super::snapshot::Target;

pub(crate) const ACTIONS: &[&str] = &["bsp", "rad", "pack", "copy", "delete", "rename", "game", "run"];

pub(crate) fn canonical_action(a: &str) -> &str {
    match a {
        "vbsp" => "bsp",
        "vvis" => "vis",
        "vrad" => "rad",
        "bspzip" => "pack",
        other => other,
    }
}

fn action_label(a: &str) -> &'static str {
    match canonical_action(a) {
        "bsp" => t!("compile.expert.action_bsp"),
        "vis" => t!("compile.expert.action_vis"),
        "rad" => t!("compile.expert.action_rad"),
        "pack" => t!("compile.expert.action_pack"),
        "copy" => t!("compile.expert.action_copy"),
        "delete" => t!("compile.expert.action_delete"),
        "rename" => t!("compile.expert.action_rename"),
        "game" => t!("compile.expert.action_game"),
        _ => t!("compile.expert.action_run"),
    }
}

fn step(action: &str, args: &str) -> ProfileStep {
    ProfileStep {
        enabled: true,
        action: action.to_owned(),
        program: String::new(),
        args: args.to_owned(),
        continue_on_error: false,
    }
}

pub(crate) fn default_profiles() -> Vec<CompileProfile> {
    let base = |name: &str, vis: &str, rad: &str| CompileProfile {
        name: name.to_owned(),
        steps: vec![
            step("bsp", &format!(r"{vis}-game $gamedir $path\$file")),
            step("rad", &format!(r"{rad}-game $gamedir $path\$file")),
            step("copy", r"$path\$file.bsp $bspdir\$file.bsp"),
            step("game", "-dev -console -allowdebug -hijack -game $gamedir +map $file"),
        ],
    };
    vec![
        base("Default", "-vis ", ""),
        base("Fast", "-fastvis ", "-bounce 2 -noextra "),
        base("LDR + HDR", "-vis ", "-both "),
        base("Final", "-vis ", "-both -final "),
        CompileProfile {
            name: "Only entities".to_owned(),
            steps: vec![step("bsp", r"-onlyents -game $gamedir $path\$file")],
        },
    ]
}

pub(crate) fn profile_of(seq: &Sequence) -> CompileProfile {
    let steps = seq
        .commands
        .iter()
        .filter(|c| c.special != Special::ChangeDir)
        .map(|c| {
            let (action, program) = match c.special {
                Special::Copy => ("copy", String::new()),
                Special::Delete => ("delete", String::new()),
                Special::Rename => ("rename", String::new()),
                Special::None | Special::ChangeDir => {
                    let run = c.run.trim().to_ascii_lowercase();
                    match compiler_token(&run) {
                        Some(action) => (action, String::new()),
                        None if run == "$game_exe" => ("game", String::new()),
                        None if run.ends_with("bspzip.exe") => ("pack", String::new()),
                        None => ("run", c.run.trim().to_owned()),
                    }
                }
            };
            ProfileStep {
                enabled: c.enabled,
                action: action.to_owned(),
                program,
                args: c.params.clone(),
                continue_on_error: false,
            }
        })
        .collect();
    CompileProfile { name: if seq.name.trim().is_empty() { "Imported".to_owned() } else { seq.name.clone() }, steps }
}

fn compiler_token(program: &str) -> Option<&'static str> {
    match program.trim().to_ascii_lowercase().as_str() {
        "$bsp_exe" => Some("bsp"),
        "$vis_exe" => Some("vis"),
        "$light_exe" => Some("rad"),
        _ => None,
    }
}

pub(crate) fn add_profiles(list: &mut Vec<CompileProfile>, new: Vec<CompileProfile>) -> usize {
    let mut added = 0;
    for mut p in new {
        let base = p.name.clone();
        let mut n = 2;
        while list.iter().any(|q| q.name.eq_ignore_ascii_case(&p.name)) {
            p.name = format!("{base} ({n})");
            n += 1;
        }
        list.push(p);
        added += 1;
    }
    added
}

pub(crate) fn tokens_of(target: &Target, tools: &ToolPaths) -> Tokens {
    let s = |p: &Path| p.to_string_lossy().into_owned();
    Tokens {
        file: target.name.clone(),
        ext: "vmf".to_owned(),
        path: s(&target.build_dir),
        bspdir: s(&tools.maps_dir),
        gamedir: s(&tools.game_dir),
        exedir: tools.game_exe.as_deref().and_then(Path::parent).map(s).unwrap_or_default(),
        bsp_exe: s(&tools.compiler),
        vis_exe: s(&tools.compiler),
        light_exe: s(&tools.compiler),
        game_exe: tools.game_exe.as_deref().map(s).unwrap_or_default(),
    }
}

fn checked_path(text: &str, roots: &[PathBuf]) -> Option<PathBuf> {
    let p = std::path::absolute(text.trim()).ok()?;
    if p.components().any(|c| matches!(c, Component::ParentDir)) || p.file_name().is_none() {
        return None;
    }
    let key = |p: &Path| {
        let s = p.to_string_lossy().replace('\\', "/");
        if cfg!(windows) { s.to_lowercase() } else { s }
    };
    let pk = key(&p);
    roots
        .iter()
        .filter_map(|r| std::path::absolute(r).ok())
        .any(|r| {
            let rk = key(&r);
            let rk = rk.trim_end_matches('/');
            pk.len() > rk.len() && pk.starts_with(rk) && pk.as_bytes().get(rk.len()) == Some(&b'/')
        })
        .then_some(p)
}

fn pack_files_of(args: &[String], n: usize) -> Result<Vec<(String, PathBuf)>, String> {
    let mut files = Vec::new();
    let mut i = 0;
    while let Some(a) = args.get(i) {
        let lower = a.to_ascii_lowercase();
        if lower == "-game" {
            i += 2;
            continue;
        }
        if lower == "-addlist" || lower == "-addorupdatelist" {
            let list = args.get(i + 2).ok_or_else(|| tf!("compile.expert.err_pack", n, a))?;
            let text = std::fs::read_to_string(list).map_err(|e| tf!("compile.expert.err_pack", n, e))?;
            let lines: Vec<&str> = text.lines().map(str::trim).filter(|l| !l.is_empty()).collect();
            files.extend(lines.chunks_exact(2).map(|pair| (pair[0].to_owned(), PathBuf::from(pair[1]))));
            i += 4;
            continue;
        }
        let dir = Path::new(a);
        let found = hp_compile::pack_files_from_dir(dir).map_err(|e| tf!("compile.expert.err_pack", n, e))?;
        files.extend(found);
        i += 1;
    }
    Ok(files)
}

fn vvis_options(args: &[String]) -> Vec<String> {
    let mut out = Vec::new();
    let mut i = 0;
    while let Some(a) = args.get(i) {
        let lower = a.to_ascii_lowercase();
        let values = match lower.as_str() {
            "-radius_override" | "-threads" | "-mpi_pw" => 1,
            "-trace" => 2,
            _ => 0,
        };
        if lower == "-game" || lower == "-vproject" {
            i += 2;
            continue;
        }
        if a.starts_with('-') && lower != "-fast" {
            out.extend(args.iter().skip(i).take(values + 1).cloned());
        }
        i += values + 1;
    }
    out
}

pub(crate) fn plan_of(profile: &CompileProfile, target: &Target, tools: &ToolPaths) -> Result<CompilePlan, String> {
    let tokens = tokens_of(target, tools);
    let mut roots = vec![target.build_dir.clone(), tools.game_dir.clone(), tools.maps_dir.clone()];
    if let Some(dir) = target.source.as_deref().and_then(Path::parent) {
        roots.push(dir.to_path_buf());
    }
    let mut plan = CompilePlan::new(target.vmf());
    for (i, s) in profile.steps.iter().enumerate() {
        let args = expand_args(&s.args, &tokens);
        let n = i + 1;
        let file_args = |count: usize| -> Result<Vec<PathBuf>, String> {
            if args.len() != count {
                return Err(tf!("compile.expert.err_args", n, count));
            }
            args.iter().map(|a| checked_path(a, &roots).ok_or_else(|| tf!("compile.expert.err_path", n, a))).collect()
        };
        let compiler_args = |first: &str| -> Vec<String> {
            let mut out = vec![first.to_owned()];
            out.extend(args.iter().cloned());
            out
        };
        let action = match canonical_action(&s.action) {
            "run" => compiler_token(&s.program).unwrap_or("run"),
            other => other,
        };
        let mut st = match action {
            "bsp" => Step::new(StepKind::Bsp, &tools.compiler, compiler_args(BSPC_ARG)),
            "rad" => Step::new(StepKind::Rad, &tools.compiler, compiler_args(RAD_ARG)),
            "vis" => {
                if !s.enabled {
                    continue;
                }
                let flag = if args.iter().any(|a| a.eq_ignore_ascii_case("-fast")) { "-fastvis" } else { "-vis" };
                let bsp = plan
                    .steps
                    .iter_mut()
                    .rev()
                    .find(|st| st.kind == StepKind::Bsp && st.enabled)
                    .ok_or_else(|| tf!("compile.expert.err_vis", n))?;
                if !bsp.args.iter().any(|a| a.eq_ignore_ascii_case("-vis") || a.eq_ignore_ascii_case("-fastvis")) {
                    bsp.args.insert(1, flag.to_owned());
                }
                let extra = vvis_options(&args);
                let at = 2.min(bsp.args.len());
                bsp.args.splice(at..at, extra);
                continue;
            }
            "pack" => Step::pack(&plan.bsp_path(), StepAction::Pack { files: pack_files_of(&args, n)? }),
            "game" => {
                let exe = tools
                    .game_exe
                    .clone()
                    .ok_or_else(|| tf!("compile.expert.err_missing_tool", n, t!("compile.game_exe")))?;
                Step::new(StepKind::LaunchGame, exe, args.clone())
            }
            "copy" => {
                let p = file_args(2)?;
                let kind = if p[1].starts_with(&tools.maps_dir) { StepKind::CopyToMaps } else { StepKind::Custom };
                Step::copy(kind, p[0].clone(), p[1].clone())
            }
            "delete" => {
                let p = file_args(1)?;
                let mut st = Step::new(StepKind::Custom, PathBuf::new(), Vec::new());
                st.action = StepAction::Delete { path: p[0].clone() };
                st
            }
            "rename" => {
                let p = file_args(2)?;
                let mut st = Step::new(StepKind::Custom, PathBuf::new(), Vec::new());
                st.action = StepAction::Rename { from: p[0].clone(), to: p[1].clone() };
                st
            }
            _ => {
                let program = expand(&s.program, &tokens);
                if program.trim().is_empty() {
                    return Err(tf!("compile.expert.err_program", n));
                }
                Step::new(StepKind::Custom, program.trim(), args.clone())
            }
        };
        st.enabled = s.enabled;
        st.continue_on_error = s.continue_on_error;
        plan.steps.push(st);
    }
    if plan.enabled_steps().next().is_none() {
        return Err(t!("compile.err_no_steps").to_owned());
    }
    Ok(plan)
}

type ImportResult = Option<Result<Vec<CompileProfile>, String>>;

#[derive(Default)]
pub(crate) struct ExpertView {
    import: Option<Receiver<ImportResult>>,
    rename: Option<String>,
}

impl ExpertView {
    pub(crate) fn ui(&mut self, ui: &mut Ui, state: &mut AppState, running: bool) {
        self.poll_import(state);
        let before = state.settings.compile.clone();
        let s = &mut state.settings.compile;
        if s.profiles.is_empty() {
            s.profiles = default_profiles();
        }
        s.active_profile = s.active_profile.min(s.profiles.len().saturating_sub(1));
        if !s.uses_builtin_compiler() {
            ui.label(egui::RichText::new(t!("compile.expert.needs_builtin")).color(hp_app::theme::warning()).small());
        }
        ui.add_enabled_ui(!running, |ui| {
            ui.horizontal(|ui| {
                ui.checkbox(&mut s.expert, t!("compile.expert.use")).on_hover_text(t!("compile.expert.use_tip"));
                ui.separator();
                let names: Vec<String> = s.profiles.iter().map(|p| p.name.clone()).collect();
                egui::ComboBox::from_id_salt("hp_compile_profile")
                    .selected_text(names.get(s.active_profile).cloned().unwrap_or_default())
                    .show_ui(ui, |ui| {
                        for (i, n) in names.iter().enumerate() {
                            ui.selectable_value(&mut s.active_profile, i, n);
                        }
                    });
                if ui.button(t!("compile.expert.new")).clicked() {
                    add_profiles(
                        &mut s.profiles,
                        vec![CompileProfile { name: t!("compile.expert.new_name").to_owned(), steps: Vec::new() }],
                    );
                    s.active_profile = s.profiles.len() - 1;
                }
                if ui.button(t!("compile.expert.duplicate")).clicked()
                    && let Some(p) = s.profiles.get(s.active_profile).cloned()
                {
                    add_profiles(&mut s.profiles, vec![p]);
                    s.active_profile = s.profiles.len() - 1;
                }
                if ui.button(t!("compile.expert.rename")).clicked() {
                    self.rename = s.profiles.get(s.active_profile).map(|p| p.name.clone());
                }
                if ui.add_enabled(s.profiles.len() > 1, egui::Button::new(t!("compile.expert.delete"))).clicked() {
                    s.profiles.remove(s.active_profile);
                    s.active_profile = s.active_profile.saturating_sub(1);
                }
                if ui
                    .add_enabled(self.import.is_none(), egui::Button::new(t!("compile.expert.import")))
                    .on_hover_text(t!("compile.expert.import_tip"))
                    .clicked()
                {
                    self.start_import(ui.ctx());
                }
            });
            if let Some(name) = &mut self.rename {
                let mut done = false;
                ui.horizontal(|ui| {
                    let r = ui.text_edit_singleline(name);
                    if ui.button(t!("common.ok")).clicked()
                        || (r.lost_focus() && ui.input(|i| i.key_pressed(egui::Key::Enter)))
                    {
                        if let Some(p) = s.profiles.get_mut(s.active_profile)
                            && !name.trim().is_empty()
                        {
                            p.name = name.trim().chars().take(64).collect();
                        }
                        done = true;
                    }
                    if ui.button(t!("common.cancel")).clicked() {
                        done = true;
                    }
                });
                if done {
                    self.rename = None;
                }
            }
            let Some(profile) = s.profiles.get_mut(s.active_profile) else { return };
            Self::steps_ui(ui, profile);
        });
        if state.settings.compile != before {
            state.settings_dirty = true;
        }
    }

    fn steps_ui(ui: &mut Ui, profile: &mut CompileProfile) {
        let mut action: Option<(usize, i32)> = None;
        egui::ScrollArea::vertical().id_salt("hp_compile_expert_steps").max_height(ui.available_height() - 30.0).show(
            ui,
            |ui| {
                egui::Grid::new("hp_compile_expert_grid").num_columns(5).striped(true).spacing([8.0, 4.0]).show(
                    ui,
                    |ui| {
                        let count = profile.steps.len();
                        for (i, st) in profile.steps.iter_mut().enumerate() {
                            ui.checkbox(&mut st.enabled, "");
                            egui::ComboBox::from_id_salt(("hp_compile_expert_action", i))
                                .selected_text(action_label(&st.action))
                                .width(150.0)
                                .show_ui(ui, |ui| {
                                    for a in ACTIONS {
                                        ui.selectable_value(&mut st.action, (*a).to_owned(), action_label(a));
                                    }
                                });
                            ui.horizontal(|ui| {
                                if st.action == "run" {
                                    ui.add(
                                        egui::TextEdit::singleline(&mut st.program)
                                            .desired_width(160.0)
                                            .hint_text(t!("compile.expert.program")),
                                    );
                                }
                                let r = ui.add(
                                    egui::TextEdit::singleline(&mut st.args)
                                        .desired_width(320.0)
                                        .font(egui::TextStyle::Monospace),
                                );
                                ui.menu_button("$", |ui| {
                                    for token in TOKENS {
                                        if ui.button(*token).clicked() {
                                            st.args.push_str(token);
                                            ui.close();
                                        }
                                    }
                                })
                                .response
                                .on_hover_text(t!("compile.expert.tokens"));
                                r
                            });
                            ui.checkbox(&mut st.continue_on_error, t!("compile.expert.keep_going"));
                            ui.horizontal(|ui| {
                                if ui.add_enabled(i > 0, egui::Button::new("⏶").small()).clicked() {
                                    action = Some((i, -1));
                                }
                                if ui.add_enabled(i + 1 < count, egui::Button::new("⏷").small()).clicked() {
                                    action = Some((i, 1));
                                }
                                if ui.small_button("✖").on_hover_text(t!("common.remove")).clicked() {
                                    action = Some((i, 0));
                                }
                            });
                            ui.end_row();
                        }
                    },
                );
            },
        );
        match action {
            Some((i, 0)) => {
                profile.steps.remove(i);
            }
            Some((i, d)) => {
                let j = if d < 0 { i - 1 } else { i + 1 };
                profile.steps.swap(i, j);
            }
            None => {}
        }
        if ui.button(t!("compile.expert.add_step")).clicked() {
            profile.steps.push(step("bsp", r"-vis -game $gamedir $path\$file"));
        }
    }

    fn start_import(&mut self, ctx: &egui::Context) {
        let (tx, rx) = mpsc::channel();
        let ctx = ctx.clone();
        let filter = t!("compile.expert.import_filter").to_owned();
        let spawned = std::thread::Builder::new().name("hp-compile-import".into()).spawn(move || {
            let picked = rfd::FileDialog::new().add_filter(filter, &["wc", "cfg"]).pick_file();
            let result = picked.map(|p| {
                let len = std::fs::metadata(&p).map(|m| m.len()).unwrap_or(u64::MAX);
                if len > cmdseq::MAX_FILE_BYTES as u64 {
                    return Err(t!("compile.expert.import_too_large").to_owned());
                }
                let bytes = std::fs::read(&p).map_err(|e| e.to_string())?;
                cmdseq::parse_any(&bytes).map(|seqs| seqs.iter().map(profile_of).collect()).map_err(|e| e.to_string())
            });
            let _ = tx.send(result);
            ctx.request_repaint();
        });
        if spawned.is_ok() {
            self.import = Some(rx);
        }
    }

    fn poll_import(&mut self, state: &mut AppState) {
        let Some(rx) = &self.import else { return };
        match rx.try_recv() {
            Ok(result) => {
                self.import = None;
                match result {
                    Some(Ok(profiles)) => {
                        let s = &mut state.settings.compile;
                        if s.profiles.is_empty() {
                            s.profiles = default_profiles();
                        }
                        let n = add_profiles(&mut s.profiles, profiles);
                        state.settings_dirty = true;
                        state.notify(Severity::Success, tf!("compile.expert.imported", n));
                    }
                    Some(Err(e)) => state.notify(Severity::Error, tf!("compile.expert.import_failed", e)),
                    None => {}
                }
            }
            Err(TryRecvError::Disconnected) => self.import = None,
            Err(TryRecvError::Empty) => {}
        }
    }
}

pub(crate) fn active_profile(s: &CompileSettings) -> Option<&CompileProfile> {
    if !s.expert {
        return None;
    }
    s.profiles.get(s.active_profile)
}

#[cfg(test)]
mod tests {
    use super::*;

    fn tools(root: &Path) -> ToolPaths {
        ToolPaths {
            compiler: root.join("hammerpro.exe"),
            game_exe: Some(root.join("gmod.exe")),
            game_dir: root.join("garrysmod"),
            maps_dir: root.join("garrysmod").join("maps"),
        }
    }

    fn target(root: &Path) -> Target {
        Target {
            name: "gm_x".into(),
            build_dir: root.join("maps").join(".hammerpro_build"),
            source: Some(root.join("maps").join("gm_x.vmf")),
        }
    }

    #[test]
    fn default_profiles_build_plans() {
        crate::register_translations();
        let root = std::path::absolute(crate::test_scratch("expert_plans")).expect("abs");
        let target = target(&root);
        let t = tools(&root);
        for p in default_profiles() {
            let plan = plan_of(&p, &target, &t).expect("plan");
            assert!(!plan.steps.is_empty());
        }
        let plan = plan_of(&default_profiles()[0], &target, &t).expect("plan");
        let kinds: Vec<StepKind> = plan.steps.iter().map(|s| s.kind).collect();
        assert_eq!(kinds, [StepKind::Bsp, StepKind::Rad, StepKind::CopyToMaps, StepKind::LaunchGame]);
        assert!(plan.steps[..2].iter().all(|s| s.exe == t.compiler));
        assert_eq!(plan.steps[0].args[..2], [BSPC_ARG, "-vis"]);
        assert_eq!(plan.steps[1].args[0], RAD_ARG);
        assert_eq!(
            plan.steps[0].args.last().map(String::as_str),
            Some(target.build_dir.join("gm_x").to_string_lossy().as_ref())
        );
        match &plan.steps[2].action {
            StepAction::Copy { from, to } => {
                assert_eq!(from, &target.build_dir.join("gm_x.bsp"));
                assert_eq!(to, &t.maps_dir.join("gm_x.bsp"));
            }
            other => panic!("{other:?}"),
        }
    }

    #[test]
    fn old_compiler_actions_run_the_built_in_compiler() {
        crate::register_translations();
        let root = std::path::absolute(crate::test_scratch("expert_old_actions")).expect("abs");
        let target = target(&root);
        let t = tools(&root);
        let content = root.join("content");
        std::fs::create_dir_all(content.join("materials")).expect("content");
        std::fs::write(content.join("materials").join("a.vmt"), "x").expect("vmt");
        let list = root.join("list.txt");
        std::fs::write(&list, format!("materials/b.vmt\n{}\n", content.join("materials").join("a.vmt").display()))
            .expect("list");
        let old = CompileProfile {
            name: "old".into(),
            steps: vec![
                step("vbsp", r"-game $gamedir $path\$file"),
                step("vvis", r"-fast -radius_override 512 -nosort -trace 1 2 -game $gamedir $path\$file"),
                step("vrad", r"-both -game $gamedir $path\$file"),
                step("bspzip", &format!(r"-addorupdatelist $path\$file.bsp {} $path\$file.bsp", list.display())),
                step("bspzip", &content.display().to_string()),
                ProfileStep {
                    action: "run".into(),
                    program: "$light_exe".into(),
                    ..step("run", "-final $path\\$file")
                },
            ],
        };
        let plan = plan_of(&old, &target, &t).expect("plan");
        let kinds: Vec<StepKind> = plan.steps.iter().map(|s| s.kind).collect();
        assert_eq!(kinds, [StepKind::Bsp, StepKind::Rad, StepKind::Pack, StepKind::Pack, StepKind::Rad]);
        assert_eq!(plan.steps[0].args[..2], [BSPC_ARG, "-fastvis"]);
        assert_eq!(plan.steps[0].args[2..8], ["-radius_override", "512", "-nosort", "-trace", "1", "2"]);
        assert_eq!(plan.steps[0].args.iter().filter(|a| a.eq_ignore_ascii_case("-game")).count(), 1);
        assert!(plan.steps.iter().filter(|s| s.kind != StepKind::Pack).all(|s| s.exe == t.compiler));
        assert!(matches!(&plan.steps[2].action, StepAction::Pack { files } if files[0].0 == "materials/b.vmt"));
        assert!(matches!(&plan.steps[3].action, StepAction::Pack { files } if files[0].0 == "materials/a.vmt"));
        assert_eq!(plan.steps[4].args[..2], [RAD_ARG, "-final"]);
        let lone = CompileProfile { name: "x".into(), steps: vec![step("vvis", "")] };
        assert!(plan_of(&lone, &target, &t).is_err());
        assert_eq!(canonical_action("vbsp"), "bsp");
        assert_eq!(canonical_action("bspzip"), "pack");
        assert_eq!(canonical_action("copy"), "copy");
        let tokens = tokens_of(&target, &t);
        assert_eq!(tokens.bsp_exe, t.compiler.to_string_lossy());
        assert_eq!(tokens.light_exe, tokens.vis_exe);
    }

    #[test]
    fn file_steps_stay_inside_the_map_and_game_folders() {
        crate::register_translations();
        let root = std::path::absolute(crate::test_scratch("expert_paths")).expect("abs");
        let target = Target {
            name: "gm_x".into(),
            build_dir: root.join("maps").join(".b"),
            source: Some(root.join("maps").join("gm_x.vmf")),
        };
        let t = tools(&root);
        let bad = |args: &str, action: &str| {
            let p = CompileProfile { name: "x".into(), steps: vec![step(action, args)] };
            plan_of(&p, &target, &t).is_err()
        };
        assert!(bad(r"C:\Windows\System32\drivers\etc\hosts", "delete"));
        assert!(bad(r"$path\..\..\..\escape.txt", "delete"));
        assert!(bad(r"$path\$file.bsp", "copy"), "copy needs two paths");
        assert!(bad(r"$path\$file.bsp C:\elsewhere\x.bsp", "rename"));
        assert!(!bad(r"$path\$file.prt", "delete"));
        assert!(!bad(r"$path\$file.bsp $bspdir\$file_v2.bsp", "copy"));
        assert!(bad(r"$path\no_such_folder", "pack"));
        let run = CompileProfile {
            name: "x".into(),
            steps: vec![ProfileStep { action: "run".into(), ..ProfileStep::default() }],
        };
        assert!(plan_of(&run, &target, &t).is_err(), "a run step needs a program");
        assert!(plan_of(&CompileProfile::default(), &target, &t).is_err());
    }

    #[test]
    fn imports_and_names() {
        let seq = Sequence {
            name: "Hammer".into(),
            commands: vec![
                cmdseq::SeqCommand {
                    enabled: true,
                    special: Special::None,
                    run: "$bsp_exe".into(),
                    params: "-game $gamedir".into(),
                },
                cmdseq::SeqCommand {
                    enabled: false,
                    special: Special::ChangeDir,
                    run: String::new(),
                    params: "x".into(),
                },
                cmdseq::SeqCommand { enabled: true, special: Special::Copy, run: String::new(), params: "a b".into() },
                cmdseq::SeqCommand {
                    enabled: true,
                    special: Special::None,
                    run: "C:/tools/compilepal.exe".into(),
                    params: "--x".into(),
                },
                cmdseq::SeqCommand {
                    enabled: true,
                    special: Special::None,
                    run: "$VIS_EXE".into(),
                    params: "-fast".into(),
                },
            ],
        };
        let p = profile_of(&seq);
        let actions: Vec<&str> = p.steps.iter().map(|s| s.action.as_str()).collect();
        assert_eq!(actions, ["bsp", "copy", "run", "vis"]);
        assert_eq!(p.steps[2].program, "C:/tools/compilepal.exe");
        let mut list = default_profiles();
        let n = list.len();
        assert_eq!(add_profiles(&mut list, vec![p.clone(), p]), 2);
        assert_eq!(list[n].name, "Hammer");
        assert_eq!(list[n + 1].name, "Hammer (2)");
        assert!(active_profile(&CompileSettings::default()).is_none());
    }

    #[test]
    fn smoke_expert_tab() {
        let _lang = crate::lang_lock();
        crate::register_translations();
        let mut state = hp_app::testing::test_state();
        let mut view = ExpertView::default();
        state.settings.compile.profiles = vec![CompileProfile {
            name: "old".into(),
            steps: vec![step("vbsp", ""), step("vvis", ""), step("vrad", ""), step("bspzip", "")],
        }];
        for lang in hp_app::i18n::Lang::ALL {
            hp_app::i18n::set_lang(lang);
            hp_app::testing::run_ui(&mut state, |ui, state| view.ui(ui, state, false));
            for a in ["vbsp", "vvis", "vrad", "bspzip"].into_iter().chain(ACTIONS.iter().copied()) {
                assert!(!action_label(a).starts_with("compile."), "{a}");
            }
        }
        hp_app::i18n::set_lang(hp_app::i18n::Lang::En);
        assert!(!state.settings.compile.profiles.is_empty());
    }
}
