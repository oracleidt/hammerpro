use std::path::PathBuf;
use std::sync::mpsc::{self, Receiver, TryRecvError};
use std::sync::{Arc, Mutex};
use std::time::{Duration, Instant};

use egui::{RichText, Ui};
use hp_app::settings::compilers::{
    self, CompilerProfile, CompilerStep, CompilerStepKind, MAX_COMPILER_NAME, MAX_COMPILER_STEPS, MAX_COMPILER_TEXT,
    MAX_COMPILERS,
};
use hp_app::settings::{CompilePresetSetting, LightingSetting};
use hp_app::{AppState, t, tf, theme};
use hp_compile::ToolPaths;
use hp_compile::template::{TOKENS, Tokens, expand};
use hp_compile::valve::{ValveFlag, ValveGroup, ValveValue, valve_flags};

use super::compilers::{display_name, find_program, kind_label, valve_tool};

const REFRESH: Duration = Duration::from_secs(10);
const LIST_WIDTH: f32 = 230.0;
const NARROW: f32 = 760.0;

type DetectKey = (Option<PathBuf>, String, Option<PathBuf>, Option<PathBuf>);

#[derive(Clone, Default)]
struct DetectCache {
    key: Option<DetectKey>,
    at: Option<Instant>,
    tools: Option<ToolPaths>,
    list: Vec<CompilerProfile>,
}

pub(crate) fn tools_of(state: &AppState) -> Option<ToolPaths> {
    let root = super::game_root(state).or_else(hp_fs::detect_gmod_root)?;
    super::tools::resolve(&root, &state.settings.compile).ok().map(|r| r.tools)
}

pub(crate) fn detection(ctx: &egui::Context, state: &AppState) -> (Option<ToolPaths>, Vec<CompilerProfile>) {
    let s = &state.settings.compile;
    let key: DetectKey = (super::game_root(state), s.game_key(), s.target_dir.clone(), s.game_exe_path.clone());
    let id = egui::Id::unique("hp_compilers_detected");
    let cached: DetectCache = ctx.data(|d| d.get_temp(id)).unwrap_or_default();
    if cached.key.as_ref() == Some(&key) && cached.at.is_some_and(|t| t.elapsed() < REFRESH) {
        return (cached.tools, cached.list);
    }
    let tools = tools_of(state);
    let list = tools.as_ref().map(super::compilers::detected).unwrap_or_default();
    let fresh = DetectCache { key: Some(key), at: Some(Instant::now()), tools: tools.clone(), list: list.clone() };
    ctx.data_mut(|d| d.insert_temp(id, fresh));
    (tools, list)
}

pub(crate) fn all_of(state: &AppState, detected: &[CompilerProfile]) -> Vec<CompilerProfile> {
    compilers::all_compilers(&state.settings.compile.compilers, detected)
}

pub(crate) fn active_of(state: &AppState, detected: &[CompilerProfile]) -> CompilerProfile {
    let all = all_of(state, detected);
    compilers::find_compiler(&all, state.settings.compile.active_compiler_id())
        .cloned()
        .unwrap_or_else(CompilerProfile::builtin)
}

pub(crate) fn picker(ui: &mut Ui, state: &mut AppState, detected: &[CompilerProfile], salt: &str) -> bool {
    let all = all_of(state, detected);
    let active = active_of(state, detected);
    let mut picked: Option<String> = None;
    egui::ComboBox::from_id_salt(("hp_compiler_pick", salt))
        .selected_text(display_name(&active))
        .width(260.0)
        .show_ui(ui, |ui| {
            for p in &all {
                if ui.selectable_label(p.id.eq_ignore_ascii_case(&active.id), display_name(p)).clicked() {
                    picked = Some(p.id.clone());
                }
            }
        })
        .response
        .on_hover_text(t!("compilers.label_tip"));
    match picked {
        Some(id) if !id.eq_ignore_ascii_case(&active.id) => {
            state.settings.compile.set_active_compiler(&id);
            state.settings_dirty = true;
            set_selected(ui.ctx(), &id);
            true
        }
        _ => false,
    }
}

type Browse = Arc<Mutex<Option<(String, usize, Receiver<Option<PathBuf>>)>>>;

fn browse_slot(ctx: &egui::Context) -> Browse {
    ctx.data_mut(|d| d.get_temp_mut_or_default::<Browse>(egui::Id::unique("hp_compilers_browse")).clone())
}

fn poll_browse(ctx: &egui::Context, state: &mut AppState, detected: &[CompilerProfile]) {
    let slot = browse_slot(ctx);
    let Ok(mut guard) = slot.lock() else { return };
    let Some((id, step, rx)) = guard.as_ref() else { return };
    match rx.try_recv() {
        Ok(picked) => {
            if let Some(path) = picked {
                let all = all_of(state, detected);
                if let Some(mut p) = compilers::find_compiler(&all, id).cloned()
                    && let Some(st) = p.steps.get_mut(*step)
                {
                    st.exe = path.display().to_string();
                    store(state, p);
                }
            }
            *guard = None;
        }
        Err(TryRecvError::Disconnected) => *guard = None,
        Err(TryRecvError::Empty) => {}
    }
}

fn start_browse(ctx: &egui::Context, id: &str, step: usize) {
    let slot = browse_slot(ctx);
    let Ok(mut guard) = slot.lock() else { return };
    if guard.is_some() {
        return;
    }
    let (tx, rx) = mpsc::channel();
    let repaint = ctx.clone();
    let title = t!("compilers.browse_exe").to_owned();
    let filter = t!("compilers.exe_filter").to_owned();
    let spawned = std::thread::Builder::new().name("hp-compilers-browse".into()).spawn(move || {
        let mut dialog = rfd::FileDialog::new().set_title(title);
        if cfg!(windows) {
            dialog = dialog.add_filter(filter, &["exe", "bat", "cmd"]);
        }
        let _ = tx.send(dialog.pick_file());
        repaint.request_repaint();
    });
    if spawned.is_ok() {
        *guard = Some((id.to_owned(), step, rx));
    }
}

fn store(state: &mut AppState, p: CompilerProfile) {
    let list = &mut state.settings.compile.compilers;
    match list.iter_mut().find(|q| q.id.eq_ignore_ascii_case(&p.id)) {
        Some(slot) => *slot = p,
        None if p.is_detected() || p.is_user() => list.push(p),
        None => return,
    }
    state.settings_dirty = true;
}

fn selected_id(ctx: &egui::Context) -> Option<String> {
    ctx.data(|d| d.get_temp::<String>(egui::Id::unique("hp_compilers_selected")))
}

fn set_selected(ctx: &egui::Context, id: &str) {
    ctx.data_mut(|d| d.insert_temp(egui::Id::unique("hp_compilers_selected"), id.to_owned()));
}

fn step_tab_id(profile: &str) -> egui::Id {
    egui::Id::unique("hp_compilers_step_tab").with(profile.to_ascii_lowercase())
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
enum ListOp {
    New,
    Duplicate,
    Delete,
    Up,
    Down,
    Reset,
    Restore,
}

fn apply_list_op(
    ctx: &egui::Context,
    state: &mut AppState,
    op: ListOp,
    chosen: &CompilerProfile,
    all: &[CompilerProfile],
) {
    let s = &mut state.settings.compile;
    match op {
        ListOp::New | ListOp::Duplicate => {
            if s.compilers.len() >= MAX_COMPILERS {
                return;
            }
            let mut p = if op == ListOp::New || chosen.is_builtin() {
                CompilerProfile::blank(t!("compilers.new_name"))
            } else {
                chosen.clone()
            };
            p.id = compilers::new_user_id(&s.compilers);
            p.name = compilers::unique_name(all, &p.name);
            let id = p.id.clone();
            s.compilers.push(p);
            set_selected(ctx, &id);
        }
        ListOp::Delete => {
            s.remove_compiler(&chosen.id);
            set_selected(ctx, s.active_compiler_id());
        }
        ListOp::Up | ListOp::Down => {
            if let Some(i) = s.compilers.iter().position(|p| p.id == chosen.id) {
                let users: Vec<usize> =
                    s.compilers.iter().enumerate().filter(|(_, p)| p.is_user()).map(|(j, _)| j).collect();
                let at = users.iter().position(|j| *j == i);
                let other = at.and_then(|k| if op == ListOp::Up { k.checked_sub(1) } else { Some(k + 1) });
                if let Some(j) = other.and_then(|k| users.get(k).copied()) {
                    s.compilers.swap(i, j);
                }
            }
        }
        ListOp::Reset => {
            s.reset_compilers();
            set_selected(ctx, compilers::BUILTIN_COMPILER);
        }
        ListOp::Restore => s.compilers.retain(|p| !(p.is_detected() && p.id.eq_ignore_ascii_case(&chosen.id))),
    }
    state.settings_dirty = true;
}

fn game_name(state: &AppState) -> String {
    let id = state.settings.game.active_game();
    hp_app::games::catalogue(None)
        .and_then(|list| list.into_iter().find(|p| p.id.eq_ignore_ascii_case(id)).map(|p| p.name))
        .or_else(|| hp_fs::known_game(id).map(|g| g.name.to_owned()))
        .unwrap_or_else(|| id.to_owned())
}

fn list_name(p: &CompilerProfile) -> String {
    if p.is_builtin() { display_name(p) } else { p.name.clone() }
}

fn badge(p: &CompilerProfile) -> &'static str {
    if p.is_builtin() {
        t!("compilers.badge_builtin")
    } else if p.is_detected() {
        t!("compilers.badge_detected")
    } else {
        t!("compilers.badge_yours")
    }
}

fn list_panel(
    ui: &mut Ui,
    state: &mut AppState,
    all: &[CompilerProfile],
    chosen: &CompilerProfile,
    rename: &mut Option<String>,
) -> Option<ListOp> {
    let ctx = ui.ctx().clone();
    let mut op = None;
    let active = state.settings.compile.active_compiler_id().to_owned();
    let users: Vec<&CompilerProfile> = all.iter().filter(|p| p.is_user()).collect();
    let pos = users.iter().position(|p| p.id == chosen.id);
    ui.horizontal(|ui| {
        ui.label(RichText::new(t!("compilers.list_title")).strong());
        ui.with_layout(egui::Layout::right_to_left(egui::Align::Center), |ui| {
            if ui
                .add_enabled(pos.is_some_and(|i| i + 1 < users.len()), egui::Button::new("⏷").small())
                .on_hover_text(t!("compilers.move_down"))
                .clicked()
            {
                op = Some(ListOp::Down);
            }
            if ui
                .add_enabled(pos.is_some_and(|i| i > 0), egui::Button::new("⏶").small())
                .on_hover_text(t!("compilers.move_up"))
                .clicked()
            {
                op = Some(ListOp::Up);
            }
        });
    });
    egui::Frame::new().fill(ui.visuals().extreme_bg_color).corner_radius(4.0).inner_margin(4.0).show(ui, |ui| {
        ui.set_width(ui.available_width());
        for p in all {
            let is_chosen = p.id.eq_ignore_ascii_case(&chosen.id);
            if is_chosen && p.is_user() && rename.is_some() {
                let mut done: Option<bool> = None;
                ui.horizontal(|ui| {
                    if let Some(name) = rename.as_mut() {
                        let r =
                            ui.add(egui::TextEdit::singleline(name).desired_width(140.0).char_limit(MAX_COMPILER_NAME));
                        if r.lost_focus() && ui.input(|i| i.key_pressed(egui::Key::Enter)) {
                            done = Some(true);
                        }
                    }
                    if ui.small_button("✔").on_hover_text(t!("common.ok")).clicked() {
                        done = Some(true);
                    }
                    if ui.small_button("✖").on_hover_text(t!("common.cancel")).clicked() {
                        done = Some(false);
                    }
                });
                if let Some(ok) = done {
                    if ok
                        && let Some(name) = rename.take()
                        && !name.trim().is_empty()
                        && !all.iter().any(|q| q.id != p.id && q.name.trim().eq_ignore_ascii_case(name.trim()))
                        && let Some(q) = state.settings.compile.compilers.iter_mut().find(|q| q.id == p.id)
                    {
                        q.name = name.trim().chars().take(MAX_COMPILER_NAME).collect();
                        state.settings_dirty = true;
                    }
                    *rename = None;
                }
                continue;
            }
            let used = p.id.eq_ignore_ascii_case(&active);
            let mark = if used { "✔ " } else { "" };
            let text = RichText::new(format!("{mark}{}", list_name(p)));
            let button = egui::Button::selectable(is_chosen, text).right_text(RichText::new(badge(p)).weak().small());
            let r = ui.add_sized([ui.available_width(), 22.0], button);
            let r = if used { r.on_hover_text(tf!("compilers.used_for", game_name(state))) } else { r };
            if r.clicked() {
                set_selected(&ctx, &p.id);
                *rename = None;
            }
        }
    });
    ui.add_space(4.0);
    let room = state.settings.compile.compilers.len() < MAX_COMPILERS;
    ui.horizontal_wrapped(|ui| {
        if ui.add_enabled(room, egui::Button::new(t!("compilers.new"))).on_hover_text(t!("compilers.new_tip")).clicked()
        {
            op = Some(ListOp::New);
        }
        if ui
            .add_enabled(room && !chosen.is_builtin(), egui::Button::new(t!("compilers.duplicate")))
            .on_hover_text(t!("compilers.duplicate_tip"))
            .clicked()
        {
            op = Some(ListOp::Duplicate);
        }
        if ui
            .add_enabled(chosen.is_user(), egui::Button::new(t!("compilers.rename")))
            .on_hover_text(t!("compilers.rename_tip"))
            .clicked()
        {
            *rename = Some(chosen.name.clone());
        }
        if ui
            .add_enabled(chosen.is_user(), egui::Button::new(t!("compilers.delete")))
            .on_hover_text(t!("compilers.delete_tip"))
            .clicked()
        {
            op = Some(ListOp::Delete);
        }
    });
    let confirm_id = egui::Id::unique("hp_compilers_reset_confirm");
    let mut confirming = ctx.data(|d| d.get_temp::<bool>(confirm_id)).unwrap_or(false);
    ui.horizontal_wrapped(|ui| {
        if confirming {
            ui.label(RichText::new(t!("compilers.reset_confirm")).color(theme::warning()));
            if ui.button(t!("common.yes")).clicked() {
                op = Some(ListOp::Reset);
                confirming = false;
            }
            if ui.button(t!("common.no")).clicked() {
                confirming = false;
            }
        } else if ui.button(t!("compilers.reset")).on_hover_text(t!("compilers.reset_tip")).clicked() {
            confirming = true;
        }
    });
    ctx.data_mut(|d| d.insert_temp(confirm_id, confirming));
    op
}

fn program_status(ui: &mut Ui, st: &CompilerStep, tokens: Option<&Tokens>) {
    if !st.kind.runs_program() {
        return;
    }
    let raw = if st.kind == CompilerStepKind::Game && st.exe.trim().is_empty() { compilers::GAME_EXE } else { &st.exe };
    let text = match tokens {
        Some(t) => expand(raw.trim(), t),
        None if raw.contains('$') => return,
        None => raw.trim().to_owned(),
    };
    if text.trim().is_empty() {
        ui.label(RichText::new(format!("⚠ {}", t!("compilers.no_program"))).color(theme::warning()).small());
        return;
    }
    match find_program(&text) {
        Some(p) => {
            ui.label(RichText::new(format!("✔ {}", t!("compilers.found"))).color(theme::success()).small())
                .on_hover_text(p.display().to_string());
        }
        None => {
            ui.label(RichText::new(format!("⚠ {}", t!("compilers.not_found"))).color(theme::warning()).small())
                .on_hover_text(tf!("compilers.not_found_tip", text));
        }
    }
}

fn var_key(token: &str) -> &str {
    match token {
        "$game_exe" => "gameexe",
        other => other.trim_start_matches('$'),
    }
}

fn variable_menu(ui: &mut Ui, text: &mut String, enabled: bool) {
    ui.add_enabled_ui(enabled, |ui| {
        ui.menu_button("$", |ui| {
            for token in TOKENS.iter().filter(|t| **t != "$game_exe") {
                let label = format!("{token}  {}", hp_app::i18n::tr(&format!("compilers.var.{}", var_key(token))));
                if ui.button(label).clicked() {
                    if text.len() + token.len() < MAX_COMPILER_TEXT {
                        text.push_str(token);
                    }
                    ui.close();
                }
            }
        })
        .response
        .on_hover_text(t!("compilers.insert_variable"));
    });
}

fn flag_tip(f: &ValveFlag) -> String {
    let key = format!("valveflags.{}", f.key());
    let mut tip = format!("{}\n{}", f.flag, hp_app::i18n::tr(&key));
    if !f.default.is_empty() {
        tip.push_str(&format!("\n{}", tf!("compilers.option_default", f.default)));
    }
    tip
}

#[derive(Clone, Copy)]
struct WindowModes {
    modes: hp_compile::StepModes,
    lighting: hp_compile::Lighting,
    threads: Option<u32>,
}

impl WindowModes {
    fn of(state: &AppState) -> WindowModes {
        let s = &state.settings.compile;
        WindowModes {
            modes: super::plan::effective_modes(s),
            lighting: super::plan::effective_lighting(s),
            threads: super::plan::threads_of(s),
        }
    }
}

fn option_rows(ui: &mut Ui, st: &mut CompilerStep, mode_flags: bool, window: WindowModes, editable: bool, salt: &str) {
    let Some(tool) = valve_tool(st.kind) else { return };
    let forced = if mode_flags {
        super::compilers::mode_flags(st.kind, window.modes, window.lighting, window.threads)
    } else {
        Vec::new()
    };
    let on = st.options.len();
    let title = if on == 0 { t!("compilers.options").to_owned() } else { tf!("compilers.options_count", on) };
    ui.label(RichText::new(title).strong()).on_hover_text(t!("compilers.options_tip"));
    for group in ValveGroup::ALL {
        let flags: Vec<&ValveFlag> = valve_flags(tool).filter(|f| f.group == group).collect();
        if flags.is_empty() {
            continue;
        }
        let used = flags.iter().filter(|f| st.options.keys().any(|k| k.eq_ignore_ascii_case(f.flag))).count();
        let mut head = hp_app::i18n::tr(&format!("valveflags.group.{}", group.key())).to_owned();
        if used > 0 {
            head.push_str(&format!("  ({used})"));
        }
        egui::CollapsingHeader::new(head)
            .id_salt(("hp_compilers_group", salt, group.key()))
            .default_open(used > 0 || group == ValveGroup::Quality)
            .show(ui, |ui| {
                egui::Grid::new(("hp_compilers_flags", salt, group.key()))
                    .num_columns(3)
                    .spacing([10.0, 3.0])
                    .striped(true)
                    .show(ui, |ui| {
                        for f in flags {
                            flag_row(ui, st, f, mode_flags, &forced, editable);
                            ui.end_row();
                        }
                    });
            });
    }
}

fn flag_row(ui: &mut Ui, st: &mut CompilerStep, f: &ValveFlag, mode_flags: bool, forced: &[String], editable: bool) {
    let tip = flag_tip(f);
    let locked = mode_flags && f.managed.is_some();
    let current = if locked {
        forced
            .iter()
            .position(|a| a.eq_ignore_ascii_case(f.flag))
            .map(|i| forced.iter().skip(i + 1).take(f.value.count()).cloned().collect::<Vec<_>>().join(" "))
    } else {
        st.options.iter().find(|(k, _)| k.eq_ignore_ascii_case(f.flag)).map(|(_, v)| v.clone())
    };
    let mut on = current.is_some();
    let label = RichText::new(f.flag).monospace();
    let r = ui.add_enabled(editable && !locked, egui::Checkbox::new(&mut on, label));
    let r = if locked {
        let why = f.managed.map(|m| hp_app::i18n::tr(&format!("valveflags.managed.{}", m.key()))).unwrap_or_default();
        r.on_disabled_hover_text(format!("{tip}\n{why}"))
    } else if editable {
        r.on_hover_text(&tip)
    } else {
        r.on_disabled_hover_text(&tip)
    };
    if r.changed() {
        st.options.retain(|k, _| !k.eq_ignore_ascii_case(f.flag));
        if on {
            st.options.insert(f.flag.to_owned(), f.default.to_owned());
        }
    }
    let mut value = current.clone().unwrap_or_default();
    if f.value == ValveValue::Switch {
        ui.label("");
    } else {
        let width = match f.value {
            ValveValue::File | ValveValue::Folder | ValveValue::Text => 180.0,
            ValveValue::Numbers(n) => 30.0 * n as f32 + 20.0,
            _ => 70.0,
        };
        let edit = egui::TextEdit::singleline(&mut value)
            .desired_width(width)
            .char_limit(MAX_COMPILER_TEXT)
            .hint_text(RichText::new(f.default).weak().italics());
        let resp = ui.add_enabled(editable && !locked && current.is_some(), edit).on_hover_text(&tip);
        if resp.changed() {
            st.options.retain(|k, _| !k.eq_ignore_ascii_case(f.flag));
            st.options.insert(f.flag.to_owned(), value.clone());
        }
    }
    let note = if locked {
        Some(RichText::new(t!("compilers.option_locked")).small().color(theme::text_weak()))
    } else if current.is_some() && f.value != ValveValue::Switch {
        hp_compile::valve::check_value(f, &value)
            .err()
            .map(|e| RichText::new(format!("⚠ {e}")).small().color(theme::warning()))
    } else {
        None
    };
    let desc = hp_app::i18n::tr(&format!("valveflags.{}", f.key()));
    ui.horizontal(|ui| {
        if let Some(n) = note {
            ui.label(n);
        }
        ui.add(egui::Label::new(RichText::new(desc).small().weak()).truncate().show_tooltip_when_elided(false))
            .on_hover_text(&tip);
    });
}

fn step_title(i: usize, st: &CompilerStep) -> String {
    format!("{} {}", i + 1, kind_label(st.kind))
}

fn field_width(ui: &Ui, room: f32) -> f32 {
    (ui.available_width() - room).clamp(160.0, 560.0)
}

fn step_panel(
    ui: &mut Ui,
    p: &mut CompilerProfile,
    index: usize,
    tokens: Option<&Tokens>,
    window: WindowModes,
    editable: bool,
) -> (Option<usize>, Option<(usize, i32)>) {
    let mut browse = None;
    let mut step_op = None;
    let count = p.steps.len();
    let mode_flags = p.mode_flags;
    let salt = format!("{}_{index}", p.id);
    let Some(st) = p.steps.get_mut(index) else { return (None, None) };
    egui::Frame::group(ui.style()).show(ui, |ui| {
        ui.set_width(ui.available_width());
        ui.horizontal_wrapped(|ui| {
            ui.add_enabled(editable, egui::Checkbox::new(&mut st.enabled, t!("compilers.run_step")))
                .on_hover_text(t!("compilers.run_step_tip"));
            ui.separator();
            ui.label(t!("compilers.col_step"));
            ui.add_enabled_ui(editable, |ui| {
                egui::ComboBox::from_id_salt(("hp_compilers_kind", &salt))
                    .selected_text(kind_label(st.kind))
                    .width(170.0)
                    .show_ui(ui, |ui| {
                        for k in CompilerStepKind::ALL {
                            let r = ui.selectable_value(&mut st.kind, k, kind_label(k));
                            if r.changed() && st.args.trim().is_empty() {
                                st.args = k.default_args().to_owned();
                            }
                        }
                    })
                    .response
                    .on_hover_text(t!("compilers.kind_tip"));
            });
            ui.separator();
            if ui
                .add_enabled(editable && index > 0, egui::Button::new("⏶"))
                .on_hover_text(t!("compilers.step_up"))
                .clicked()
            {
                step_op = Some((index, -1));
            }
            if ui
                .add_enabled(editable && index + 1 < count, egui::Button::new("⏷"))
                .on_hover_text(t!("compilers.step_down"))
                .clicked()
            {
                step_op = Some((index, 1));
            }
            if ui.add_enabled(editable, egui::Button::new("✖")).on_hover_text(t!("compilers.step_remove")).clicked() {
                step_op = Some((index, 0));
            }
        });
        ui.add_space(4.0);
        egui::Grid::new(("hp_compilers_step_grid", &salt)).num_columns(2).spacing([10.0, 6.0]).show(ui, |ui| {
            if st.kind.runs_program() {
                ui.label(t!("compilers.col_program")).on_hover_text(t!("compilers.program_tip"));
                ui.horizontal_wrapped(|ui| {
                    let hint = if st.kind == CompilerStepKind::Game { compilers::GAME_EXE } else { "" };
                    ui.add_enabled(
                        editable,
                        egui::TextEdit::singleline(&mut st.exe)
                            .desired_width(field_width(ui, 170.0))
                            .char_limit(MAX_COMPILER_TEXT)
                            .hint_text(hint),
                    )
                    .on_hover_text(t!("compilers.program_tip"));
                    if ui.add_enabled(editable, egui::Button::new(t!("common.browse"))).clicked() {
                        browse = Some(index);
                    }
                    program_status(ui, st, tokens);
                });
                ui.end_row();
            }
            let (label, tip) = if st.kind.is_compile() {
                (t!("compilers.base_args"), t!("compilers.base_args_tip"))
            } else if st.kind == CompilerStepKind::Copy {
                (t!("compilers.copy_args"), t!("compilers.copy_args_tip"))
            } else {
                (t!("compilers.col_args"), t!("compilers.args_tip"))
            };
            ui.label(label).on_hover_text(tip);
            ui.horizontal(|ui| {
                ui.add_enabled(
                    editable,
                    egui::TextEdit::singleline(&mut st.args)
                        .desired_width(field_width(ui, 40.0))
                        .char_limit(MAX_COMPILER_TEXT)
                        .font(egui::TextStyle::Monospace),
                )
                .on_hover_text(tip);
                variable_menu(ui, &mut st.args, editable);
            });
            ui.end_row();
            if st.kind.is_compile() {
                ui.label(t!("compilers.extra_args")).on_hover_text(t!("compilers.extra_args_tip"));
                ui.horizontal(|ui| {
                    ui.add_enabled(
                        editable,
                        egui::TextEdit::singleline(&mut st.extra)
                            .desired_width(field_width(ui, 40.0))
                            .char_limit(MAX_COMPILER_TEXT)
                            .font(egui::TextStyle::Monospace)
                            .hint_text(t!("compilers.extra_args_hint")),
                    )
                    .on_hover_text(t!("compilers.extra_args_tip"));
                    variable_menu(ui, &mut st.extra, editable);
                });
                ui.end_row();
            }
            if st.kind != CompilerStepKind::Copy {
                ui.label(t!("compilers.col_workdir")).on_hover_text(t!("compilers.workdir_tip"));
                ui.add_enabled(
                    editable,
                    egui::TextEdit::singleline(&mut st.workdir)
                        .desired_width(field_width(ui, 40.0))
                        .char_limit(MAX_COMPILER_TEXT)
                        .hint_text(t!("compilers.workdir_hint")),
                )
                .on_hover_text(t!("compilers.workdir_tip"));
                ui.end_row();
            }
            ui.label("");
            ui.add_enabled(editable, egui::Checkbox::new(&mut st.continue_on_error, t!("compilers.keep_going")))
                .on_hover_text(t!("compilers.keep_going_tip"));
            ui.end_row();
        });
        if st.kind.is_compile() {
            ui.add_space(6.0);
            option_rows(ui, st, mode_flags, window, editable, &salt);
        }
    });
    (browse, step_op)
}

fn add_step_menu(ui: &mut Ui, p: &mut CompilerProfile) -> Option<usize> {
    let mut added = None;
    if p.steps.len() >= MAX_COMPILER_STEPS {
        return None;
    }
    ui.menu_button(format!("➕ {}", t!("compilers.add_step")), |ui| {
        for k in CompilerStepKind::ALL {
            if ui.button(kind_label(k)).clicked() {
                let mut st = CompilerStep::of_kind(k);
                if k.is_compile()
                    && let Some(dir) = p
                        .steps
                        .iter()
                        .find(|s| s.kind.is_compile())
                        .and_then(|s| std::path::Path::new(s.exe.trim()).parent().map(std::path::Path::to_path_buf))
                        .filter(|d| !d.as_os_str().is_empty())
                {
                    let name = match k {
                        CompilerStepKind::Bsp => "vbsp",
                        CompilerStepKind::Vis => "vvis",
                        _ => "vrad",
                    };
                    let file = if cfg!(windows) { format!("{name}.exe") } else { name.to_owned() };
                    st.exe = dir.join(file).display().to_string();
                }
                p.steps.push(st);
                added = Some(p.steps.len() - 1);
                ui.close();
            }
        }
    })
    .response
    .on_hover_text(t!("compilers.add_step_tip"));
    added
}

fn quality_row(ui: &mut Ui, state: &mut AppState) {
    let s = &mut state.settings.compile;
    ui.horizontal_wrapped(|ui| {
        ui.label(t!("compile.preset")).on_hover_text(t!("compilers.preset_tip"));
        for (p, key, tip) in [
            (CompilePresetSetting::Draft, "compile.preset.draft", "compile.preset.draft_tip"),
            (CompilePresetSetting::Normal, "compile.preset.normal", "compile.preset.normal_tip"),
            (CompilePresetSetting::Final, "compile.preset.final", "compile.preset.final_tip"),
        ] {
            if ui.add(egui::Button::selectable(s.preset == p, t!(key))).on_hover_text(t!(tip)).clicked() {
                super::plan::apply_preset(s, p);
            }
        }
        if s.preset == CompilePresetSetting::Custom {
            ui.label(RichText::new(t!("compile.preset.custom")).weak());
        }
        ui.separator();
        ui.label(t!("compile.lighting")).on_hover_text(t!("compile.lighting_tip"));
        egui::ComboBox::from_id_salt("hp_compilers_lighting")
            .selected_text(match s.lighting {
                LightingSetting::Ldr => t!("compile.lighting.ldr"),
                LightingSetting::Hdr => t!("compile.lighting.hdr"),
                LightingSetting::Both => t!("compile.lighting.both"),
            })
            .show_ui(ui, |ui| {
                for (l, key) in [
                    (LightingSetting::Ldr, "compile.lighting.ldr"),
                    (LightingSetting::Hdr, "compile.lighting.hdr"),
                    (LightingSetting::Both, "compile.lighting.both"),
                ] {
                    ui.selectable_value(&mut s.lighting, l, t!(key));
                }
            });
    });
}

fn preview_ui(ui: &mut Ui, state: &AppState, p: &CompilerProfile, tools: Option<&ToolPaths>) {
    ui.horizontal(|ui| {
        ui.label(RichText::new(t!("compilers.preview")).strong()).on_hover_text(t!("compilers.preview_tip"));
    });
    let lines = preview_lines(state, p, tools);
    egui::Frame::new().fill(ui.visuals().extreme_bg_color).corner_radius(4.0).inner_margin(6.0).show(ui, |ui| {
        ui.set_width(ui.available_width());
        match &lines {
            Ok(lines) if lines.is_empty() => {
                ui.label(RichText::new(t!("compilers.preview_none")).weak());
            }
            Ok(lines) => {
                for (label, line) in lines {
                    ui.label(RichText::new(label).small().strong());
                    ui.add(egui::Label::new(RichText::new(line).monospace().small()).wrap());
                }
            }
            Err(e) => {
                ui.add(egui::Label::new(RichText::new(format!("⚠ {e}")).color(theme::warning())).wrap());
            }
        }
    });
    if let Ok(lines) = &lines
        && !lines.is_empty()
        && ui.small_button(t!("compilers.copy_commands")).clicked()
    {
        let text: Vec<&str> = lines.iter().map(|(_, l)| l.as_str()).collect();
        ui.ctx().copy_text(text.join("\n"));
    }
}

fn preview_lines(
    state: &AppState,
    p: &CompilerProfile,
    tools: Option<&ToolPaths>,
) -> Result<Vec<(String, String)>, String> {
    let tools = tools.ok_or_else(|| super::tools::no_game_text(super::game_root(state).as_deref()))?;
    let s = &state.settings.compile;
    let cordoned = s.compile_cordon_only && state.doc.cordons_in_use();
    let untitled = hp_app::settings::Settings::cache_dir().join("untitled");
    let target = super::snapshot::target_for(state.doc.path(), cordoned, &untitled)?;
    let vmf = target.vmf();
    let cx = super::compilers::Context {
        vmf: &vmf,
        tools,
        modes: super::plan::effective_modes(s),
        lighting: super::plan::effective_lighting(s),
        threads: super::plan::threads_of(s),
    };
    let steps = super::compilers::profile_steps(p, &cx)?;
    Ok(steps
        .iter()
        .zip(&p.steps)
        .filter(|(st, _)| st.enabled)
        .map(|(st, src)| (kind_label(src.kind).to_owned(), st.command_line()))
        .collect())
}

fn detail_panel(ui: &mut Ui, state: &mut AppState, chosen: &CompilerProfile, tools: Option<&ToolPaths>, running: bool) {
    let ctx = ui.ctx().clone();
    let game = game_name(state);
    let active = state.settings.compile.active_compiler_id().eq_ignore_ascii_case(&chosen.id);
    ui.horizontal_wrapped(|ui| {
        ui.label(RichText::new(list_name(chosen)).heading());
        ui.label(RichText::new(badge(chosen)).weak());
    });
    ui.horizontal_wrapped(|ui| {
        if active {
            ui.label(RichText::new(format!("✔ {}", tf!("compilers.used_for", game))).color(theme::success()));
        } else if ui
            .add_enabled(!running, egui::Button::new(tf!("compilers.use_for", game)))
            .on_hover_text(t!("compilers.use_for_tip"))
            .clicked()
        {
            state.settings.compile.set_active_compiler(&chosen.id);
            state.settings_dirty = true;
        }
    });
    ui.add_space(4.0);
    if chosen.is_builtin() {
        ui.add(egui::Label::new(RichText::new(t!("compilers.builtin_info")).weak()).wrap());
        ui.add_space(4.0);
        ui.add_enabled_ui(!running, |ui| {
            quality_row(ui, state);
            ui.add_space(4.0);
            ui.label(RichText::new(t!("compilers.builtin_options")).strong());
            super::options_ui::compile_options_ui(ui, &mut state.settings.compile);
        });
        return;
    }
    if chosen.is_detected() {
        ui.add(egui::Label::new(RichText::new(t!("compilers.detected_info")).weak()).wrap());
        if compilers::is_edited(&state.settings.compile.compilers, &chosen.id)
            && ui
                .add_enabled(!running, egui::Button::new(t!("compilers.restore")))
                .on_hover_text(t!("compilers.restore_tip"))
                .clicked()
        {
            apply_list_op(&ctx, state, ListOp::Restore, chosen, &[]);
            return;
        }
    }
    let editable = !running;
    let mut p = chosen.clone();
    ui.add_space(6.0);
    ui.label(RichText::new(t!("compilers.section_compile")).strong())
        .on_hover_text(t!("compilers.section_compile_tip"));
    egui::Frame::group(ui.style()).show(ui, |ui| {
        ui.set_width(ui.available_width());
        ui.add_enabled_ui(editable, |ui| quality_row(ui, state));
        ui.add_enabled(editable, egui::Checkbox::new(&mut p.mode_flags, t!("compilers.mode_flags")))
            .on_hover_text(t!("compilers.mode_flags_tip"));
    });
    ui.add_space(6.0);
    ui.label(RichText::new(t!("compilers.steps")).strong()).on_hover_text(t!("compilers.steps_tip"));
    let tab_id = step_tab_id(&p.id);
    let mut tab = ctx.data(|d| d.get_temp::<usize>(tab_id)).unwrap_or(0).min(p.steps.len().saturating_sub(1));
    ui.horizontal_wrapped(|ui| {
        for (i, st) in p.steps.iter().enumerate() {
            let mut text = RichText::new(step_title(i, st));
            if !st.enabled {
                text = text.weak().strikethrough();
            }
            if ui.add(egui::Button::selectable(tab == i, text)).clicked() {
                tab = i;
            }
        }
        if editable && let Some(i) = add_step_menu(ui, &mut p) {
            tab = i;
        }
    });
    let tokens = tools.map(|t| tokens_for(&p, t));
    let (browse, step_op) = if p.steps.is_empty() {
        ui.label(RichText::new(t!("compilers.no_steps")).weak());
        (None, None)
    } else {
        step_panel(ui, &mut p, tab, tokens.as_ref(), WindowModes::of(state), editable)
    };
    match step_op {
        Some((i, 0)) if i < p.steps.len() => {
            p.steps.remove(i);
            tab = tab.min(p.steps.len().saturating_sub(1));
        }
        Some((i, d)) => {
            let j = if d < 0 { i.saturating_sub(1) } else { i + 1 };
            if j < p.steps.len() {
                p.steps.swap(i, j);
                tab = j;
            }
        }
        None => {}
    }
    ctx.data_mut(|d| d.insert_temp(tab_id, tab));
    if let Some(i) = browse {
        start_browse(&ctx, &p.id, i);
    }
    let problems = super::compilers::step_problems(&p, tokens.as_ref());
    if !problems.is_empty() {
        ui.add_space(4.0);
        for pr in &problems {
            ui.add(egui::Label::new(RichText::new(format!("⚠ {pr}")).color(theme::warning()).small()).wrap());
        }
    }
    ui.add_space(6.0);
    preview_ui(ui, state, &p, tools);
    if p != *chosen {
        store(state, p);
    }
}

fn variables_help(ui: &mut Ui) {
    egui::CollapsingHeader::new(t!("compilers.variables")).id_salt("hp_compilers_vars").show(ui, |ui| {
        ui.add(egui::Label::new(RichText::new(t!("compilers.variables_tip")).small().weak()).wrap());
        egui::Grid::new("hp_compilers_vars_grid").num_columns(2).spacing([12.0, 2.0]).show(ui, |ui| {
            for token in [
                "$path",
                "$file",
                "$ext",
                "$bspdir",
                "$gamedir",
                "$gameexe",
                "$exedir",
                "$bsp_exe",
                "$vis_exe",
                "$light_exe",
            ] {
                ui.label(RichText::new(token).monospace());
                ui.label(hp_app::i18n::tr(&format!("compilers.var.{}", var_key(token))));
                ui.end_row();
            }
        });
    });
}

pub(crate) fn editor_ui(ui: &mut Ui, state: &mut AppState, running: bool) {
    let visible = ui.clip_rect().right() - ui.cursor().left();
    ui.scope(|ui| {
        if visible.is_finite() && visible > 200.0 && visible < ui.available_width() {
            ui.set_max_width(visible);
        }
        editor_body(ui, state, running);
    });
}

fn editor_body(ui: &mut Ui, state: &mut AppState, running: bool) {
    let ctx = ui.ctx().clone();
    let (tools, detected) = detection(&ctx, state);
    poll_browse(&ctx, state, &detected);
    let before = state.settings.compile.clone();
    let all = all_of(state, &detected);
    let wanted = selected_id(&ctx).unwrap_or_else(|| state.settings.compile.active_compiler_id().to_owned());
    let chosen = compilers::find_compiler(&all, &wanted).cloned().unwrap_or_else(|| active_of(state, &detected));
    let rename_id = egui::Id::unique("hp_compilers_rename");
    let mut rename: Option<String> = ctx.data(|d| d.get_temp(rename_id)).flatten();
    let mut op = None;
    let narrow = ui.available_width() < NARROW;
    ui.add(egui::Label::new(RichText::new(t!("compilers.intro")).weak()).wrap());
    ui.add_space(4.0);
    if narrow {
        ui.add_enabled_ui(!running, |ui| op = list_panel(ui, state, &all, &chosen, &mut rename));
        ui.separator();
        detail_panel(ui, state, &chosen, tools.as_ref(), running);
    } else {
        ui.horizontal_top(|ui| {
            ui.allocate_ui_with_layout(
                egui::vec2(LIST_WIDTH, ui.available_height()),
                egui::Layout::top_down(egui::Align::Min),
                |ui| {
                    ui.set_width(LIST_WIDTH);
                    ui.add_enabled_ui(!running, |ui| op = list_panel(ui, state, &all, &chosen, &mut rename));
                },
            );
            ui.separator();
            ui.vertical(|ui| detail_panel(ui, state, &chosen, tools.as_ref(), running));
        });
    }
    ui.add_space(6.0);
    variables_help(ui);
    ctx.data_mut(|d| d.insert_temp(rename_id, rename));
    if let Some(op) = op {
        apply_list_op(&ctx, state, op, &chosen, &all);
    }
    if state.settings.compile != before {
        state.settings_dirty = true;
    }
}

fn tokens_for(p: &CompilerProfile, tools: &ToolPaths) -> Tokens {
    let vmf = std::env::temp_dir().join(super::snapshot::BUILD_DIR).join("map.vmf");
    super::compilers::tokens(p, &vmf, tools)
}

pub fn settings_page_ui(ui: &mut Ui, state: &mut AppState) {
    editor_ui(ui, state, false);
}

pub fn compiler_tools(state: &AppState) -> Option<ToolPaths> {
    tools_of(state)
}

pub fn compiler_list(state: &AppState, tools: Option<&ToolPaths>) -> Vec<CompilerProfile> {
    let detected = tools.map(super::compilers::detected).unwrap_or_default();
    all_of(state, &detected)
}

pub fn compiler_name(p: &CompilerProfile) -> String {
    display_name(p)
}

pub fn compiler_problems(p: &CompilerProfile, tools: Option<&ToolPaths>) -> Vec<String> {
    let tokens = tools.map(|t| tokens_for(p, t));
    super::compilers::step_problems(p, tokens.as_ref())
}

pub fn compiler_commands(
    state: &AppState,
    p: &CompilerProfile,
    tools: Option<&ToolPaths>,
) -> Result<Vec<String>, String> {
    if p.is_builtin() {
        return Ok(Vec::new());
    }
    Ok(preview_lines(state, p, tools)?.into_iter().map(|(_, l)| l).collect())
}

pub fn valve_flag_text(f: &ValveFlag, lang: hp_app::i18n::Lang) -> &'static str {
    hp_app::i18n::tr_in(&format!("valveflags.{}", f.key()), lang)
}

pub fn valve_group_text(g: ValveGroup, lang: hp_app::i18n::Lang) -> &'static str {
    hp_app::i18n::tr_in(&format!("valveflags.group.{}", g.key()), lang)
}

pub fn valve_managed_text(m: hp_compile::valve::ValveManaged, lang: hp_app::i18n::Lang) -> &'static str {
    hp_app::i18n::tr_in(&format!("valveflags.managed.{}", m.key()), lang)
}

#[cfg(test)]
mod tests {
    use super::*;
    use hp_compile::valve::{VALVE_FLAGS, ValveTool};

    #[test]
    fn list_operations() {
        let _lang = crate::lang_lock();
        crate::register_translations();
        let ctx = egui::Context::default();
        let mut state = hp_app::testing::test_state();
        let detected =
            vec![CompilerProfile { id: "detected:valve:bin".into(), ..CompilerProfile::blank("Valve (bin)") }];
        let all = all_of(&state, &detected);
        apply_list_op(&ctx, &mut state, ListOp::New, &CompilerProfile::builtin(), &all);
        assert_eq!(state.settings.compile.compilers.len(), 1);
        assert_eq!(selected_id(&ctx).as_deref(), Some("user:1"));
        assert!(state.settings.compile.uses_builtin_compiler(), "a new compiler is not used before Use for");
        let all = all_of(&state, &detected);
        apply_list_op(&ctx, &mut state, ListOp::Duplicate, &detected[0], &all);
        let copy = &state.settings.compile.compilers[1];
        assert_eq!((copy.id.as_str(), copy.name.as_str()), ("user:2", "Valve (bin) (2)"));
        let second = state.settings.compile.compilers[1].clone();
        let all = all_of(&state, &detected);
        apply_list_op(&ctx, &mut state, ListOp::Up, &second, &all);
        assert_eq!(state.settings.compile.compilers[0].id, "user:2");
        let all = all_of(&state, &detected);
        apply_list_op(&ctx, &mut state, ListOp::Down, &second, &all);
        assert_eq!(state.settings.compile.compilers[1].id, "user:2");
        state.settings.compile.set_active_compiler("user:2");
        let all = all_of(&state, &detected);
        apply_list_op(&ctx, &mut state, ListOp::Delete, &second, &all);
        assert_eq!(state.settings.compile.compilers.len(), 1);
        assert!(state.settings.compile.uses_builtin_compiler(), "the deleted compiler is not chosen any more");

        let mut edited = detected[0].clone();
        edited.steps[2].options.insert("-bounce".into(), "4".into());
        state.settings.compile.set_active_compiler(&edited.id);
        store(&mut state, edited.clone());
        assert_eq!(active_of(&state, &detected).steps[2].options.get("-bounce").map(String::as_str), Some("4"));
        apply_list_op(&ctx, &mut state, ListOp::Restore, &edited, &[]);
        assert!(active_of(&state, &detected).steps[2].options.is_empty());
        assert_eq!(state.settings.compile.active_compiler_id(), "detected:valve:bin", "restoring keeps the choice");
        apply_list_op(&ctx, &mut state, ListOp::Reset, &CompilerProfile::builtin(), &[]);
        assert!(state.settings.compile.compilers.is_empty());
        assert!(state.settings_dirty);
    }

    #[test]
    fn every_valve_flag_is_explained_in_both_languages() {
        let _lang = crate::lang_lock();
        crate::register_translations();
        for f in VALVE_FLAGS {
            for lang in hp_app::i18n::Lang::ALL {
                let text = valve_flag_text(f, lang);
                assert!(!text.starts_with("valveflags."), "{} {lang:?}", f.flag);
                if let Some(m) = f.managed {
                    assert!(!valve_managed_text(m, lang).starts_with("valveflags."), "{m:?}");
                }
            }
        }
        for g in ValveGroup::ALL {
            assert!(!valve_group_text(g, hp_app::i18n::Lang::Fr).starts_with("valveflags."), "{g:?}");
        }
        for tool in ValveTool::ALL {
            assert!(hp_compile::valve::valve_flags(tool).count() > 10);
        }
    }

    #[test]
    fn editor_renders_in_both_languages_and_widths() {
        let _lang = crate::lang_lock();
        crate::register_translations();
        let mut state = hp_app::testing::test_state();
        let mut p = CompilerProfile::blank("Mine");
        p.id = "user:1".into();
        p.steps[2].options.insert("-bounce".into(), "x".into());
        p.steps.push(CompilerStep::of_kind(CompilerStepKind::Copy));
        p.steps.push(CompilerStep::of_kind(CompilerStepKind::Game));
        state.settings.compile.compilers.push(p);
        for lang in hp_app::i18n::Lang::ALL {
            hp_app::i18n::set_lang(lang);
            for width in [1200.0, 560.0] {
                for id in ["user:1", "builtin"] {
                    let ctx = egui::Context::default();
                    set_selected(&ctx, id);
                    ctx.data_mut(|d| d.insert_temp(step_tab_id("user:1"), 2usize));
                    for _ in 0..3 {
                        let input = egui::RawInput {
                            screen_rect: Some(egui::Rect::from_min_size(egui::Pos2::ZERO, egui::vec2(width, 900.0))),
                            ..Default::default()
                        };
                        let mut out = ctx.run_ui(input, |ui| {
                            egui::CentralPanel::default().show(ui, |ui| {
                                egui::ScrollArea::vertical().show(ui, |ui| editor_ui(ui, &mut state, false));
                            });
                        });
                        out.textures_delta.clear();
                    }
                }
            }
            for k in CompilerStepKind::ALL {
                assert!(!kind_label(k).starts_with("compilers."), "{k:?}");
            }
            for token in TOKENS {
                assert!(hp_app::i18n::has_key(&format!("compilers.var.{}", var_key(token))), "{token}");
            }
        }
        hp_app::i18n::set_lang(hp_app::i18n::Lang::En);
        let p = &state.settings.compile.compilers[0];
        assert_eq!(p.steps[2].options.get("-bounce").map(String::as_str), Some("x"));
    }

    const DOCS_BEGIN: &str = "<!-- valve flags begin -->";
    const DOCS_END: &str = "<!-- valve flags end -->";

    fn takes(f: &ValveFlag, lang: hp_app::i18n::Lang) -> String {
        let fr = lang == hp_app::i18n::Lang::Fr;
        match f.value {
            ValveValue::Switch => if fr { "case à cocher" } else { "switch" }.to_owned(),
            ValveValue::Int { min, max } => {
                if fr { format!("entier de {min} à {max}") } else { format!("integer {min} to {max}") }
            }
            ValveValue::Float { min, max } => {
                if fr { format!("nombre de {min} à {max}") } else { format!("number {min} to {max}") }
            }
            ValveValue::Text => if fr { "texte" } else { "text" }.to_owned(),
            ValveValue::File => if fr { "fichier" } else { "file" }.to_owned(),
            ValveValue::Folder => if fr { "dossier" } else { "folder" }.to_owned(),
            ValveValue::Numbers(n) => if fr { format!("{n} nombres") } else { format!("{n} numbers") },
        }
    }

    fn flag_tables(lang: hp_app::i18n::Lang) -> String {
        let fr = lang == hp_app::i18n::Lang::Fr;
        let mut out = String::new();
        out.push_str(DOCS_BEGIN);
        out.push('\n');
        for (tool, step) in [
            (ValveTool::Vbsp, if fr { "Géométrie (BSP)" } else { "Geometry (BSP)" }),
            (ValveTool::Vvis, if fr { "Visibilité (VIS)" } else { "Visibility (VIS)" }),
            (ValveTool::Vrad, if fr { "Éclairage (RAD)" } else { "Lighting (RAD)" }),
        ] {
            let colon = if fr { " :" } else { ":" };
            out.push_str(&format!("\n### {}{colon} {step}\n\n", tool.key()));
            if fr {
                out.push_str("| Option | Groupe | Valeur | Défaut | Ce qu'elle fait |\n|---|---|---|---|---|\n");
            } else {
                out.push_str("| Option | Group | Value | Default | What it does |\n|---|---|---|---|---|\n");
            }
            for f in hp_compile::valve::valve_flags(tool) {
                let mut what =
                    valve_flag_text(f, lang).replace('|', "\\|").replace('<', "&lt;").replace('>', "&gt;");
                if let Some(m) = f.managed {
                    what.push_str(&format!(" *{}.*", valve_managed_text(m, lang)));
                }
                out.push_str(&format!(
                    "| `{}` | {} | {} | {} | {} |\n",
                    f.flag,
                    valve_group_text(f.group, lang),
                    takes(f, lang),
                    if f.default.is_empty() { String::new() } else { format!("`{}`", f.default) },
                    what
                ));
            }
        }
        out.push('\n');
        out.push_str(DOCS_END);
        out
    }

    #[test]
    fn docs_list_every_valve_flag() {
        let _lang = crate::lang_lock();
        crate::register_translations();
        let root = std::path::Path::new(env!("CARGO_MANIFEST_DIR")).join("../../docs");
        for (lang, file) in [(hp_app::i18n::Lang::En, "en"), (hp_app::i18n::Lang::Fr, "fr")] {
            let generated = flag_tables(lang);
            for f in VALVE_FLAGS {
                assert!(generated.contains(&format!("| `{}` |", f.flag)), "{}", f.flag);
            }
            for c in ['\u{2013}', '\u{2014}'] {
                assert!(!generated.contains(c));
            }
            let path = root.join(file).join("compiler").join(["compilers", "md"].join("."));
            let doc = std::fs::read_to_string(&path).expect("the compilers page");
            let (Some(a), Some(b)) = (doc.find(DOCS_BEGIN), doc.find(DOCS_END)) else {
                panic!("{} has no flag list markers", path.display())
            };
            let updated = format!("{}{generated}{}", &doc[..a], &doc[b + DOCS_END.len()..]);
            if updated != doc {
                if std::env::var_os("HP_UPDATE_COMPILER_DOCS").is_some() {
                    std::fs::write(&path, updated).expect("write the compilers page");
                } else {
                    panic!(
                        "the flag list of {} is out of date: run `HP_UPDATE_COMPILER_DOCS=1 cargo test -p \
                         hp-ui-mapping docs_list_every_valve_flag` (with your --target-dir)",
                        path.display()
                    );
                }
            }
        }
    }
}
