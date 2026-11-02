use hp_app::settings::Settings;
use hp_app::{Severity, t, tf};

use crate::app::HammerProApp;
use crate::files;

fn closed(response: &egui::ModalResponse<()>) -> bool {
    response.should_close()
}

fn enter_pressed(ctx: &egui::Context) -> bool {
    ctx.input(|i| {
        i.events.iter().any(|e| {
            matches!(e, egui::Event::Key { key: egui::Key::Enter, pressed: true, repeat: false, modifiers, .. }
                if !modifiers.ctrl && !modifiers.alt && !modifiers.command && !modifiers.mac_cmd)
        })
    })
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(crate) enum UnsavedAnswer {
    Save,
    DontSave,
    Cancel,
}

pub(crate) fn unsaved_modal(ctx: &egui::Context, question: &str) -> Option<UnsavedAnswer> {
    let mut answer = None;
    let modal = egui::Modal::new(egui::Id::unique("hp_unsaved")).show(ctx, |ui| {
        ui.set_width(380.0);
        ui.heading(t!("dialog.unsaved_title"));
        ui.add_space(6.0);
        ui.label(question);
        ui.add_space(10.0);
        ui.horizontal(|ui| {
            if ui.button(t!("common.save")).clicked() {
                answer = Some(UnsavedAnswer::Save);
            }
            if ui.button(t!("common.dont_save")).clicked() {
                answer = Some(UnsavedAnswer::DontSave);
            }
            if ui.button(t!("common.cancel")).clicked() {
                answer = Some(UnsavedAnswer::Cancel);
            }
        });
    });
    if answer.is_none() && closed(&modal) {
        answer = Some(UnsavedAnswer::Cancel);
    }
    if answer.is_none() && enter_pressed(ctx) {
        answer = Some(UnsavedAnswer::Save);
    }
    answer
}

#[derive(Clone, Debug, PartialEq)]
pub(crate) struct HollowPrompt {
    pub thickness: f64,
    focused: bool,
}

#[derive(Clone, Copy, Debug, PartialEq)]
pub(crate) enum HollowAnswer {
    Open,
    Apply(f64),
    Cancel,
}

impl HollowPrompt {
    pub fn new(thickness: f64) -> Self {
        Self { thickness, focused: false }
    }

    pub fn show(&mut self, ctx: &egui::Context) -> HollowAnswer {
        let mut ok = false;
        let mut cancel = false;
        let modal = egui::Modal::new(egui::Id::unique("hp_hollow")).show(ctx, |ui| {
            ui.heading(t!("action.make_hollow"));
            ui.horizontal(|ui| {
                ui.label(t!("hollow.thickness"));
                if !self.focused {
                    let id = ui.next_auto_id();
                    ui.memory_mut(|m| m.request_focus(id));
                }
                let r = ui.add(egui::DragValue::new(&mut self.thickness).speed(1.0).range(-4096.0..=4096.0));
                if r.has_focus() {
                    self.focused = true;
                }
            });
            ui.label(egui::RichText::new(t!("hollow.hint")).weak());
            ui.horizontal(|ui| {
                ok = ui.button(t!("common.ok")).clicked();
                cancel = ui.button(t!("common.cancel")).clicked();
            });
        });
        if cancel || (!ok && closed(&modal)) {
            HollowAnswer::Cancel
        } else if ok || enter_pressed(ctx) {
            HollowAnswer::Apply(self.thickness)
        } else {
            HollowAnswer::Open
        }
    }
}

impl HammerProApp {
    pub(crate) fn unsaved_prompt(&mut self, ctx: &egui::Context) {
        let Some(op) = self.pending.clone() else {
            self.unsaved_asked = None;
            return;
        };
        let asked = (op.clone(), self.tabs.active_id());
        if self.unsaved_asked.as_ref() != Some(&asked) {
            ctx.memory_mut(|m| {
                if let Some(id) = m.focused() {
                    m.surrender_focus(id);
                }
            });
            self.unsaved_asked = Some(asked);
        }
        let question = op.unsaved_question(&files::map_display_name(&self.state));
        match unsaved_modal(ctx, &question) {
            None => {}
            Some(UnsavedAnswer::Cancel) => {
                self.pending = None;
                self.opener.drop_held();
                self.cancel_closing();
            }
            Some(answer) => {
                let save = answer == UnsavedAnswer::Save;
                if save && !files::save_map(&mut self.state) {
                    return;
                }
                if !save
                    && self.persist_settings
                    && let Some(path) = self.state.doc.path()
                {
                    crate::autosave::mark_discarded(&Settings::autosave_dir(), path);
                }
                self.pending = None;
                self.run_pending(ctx, op);
            }
        }
    }

    pub(crate) fn hollow_dialog(&mut self, ctx: &egui::Context) {
        let Some(prompt) = self.hollow_prompt.as_mut() else { return };
        match prompt.show(ctx) {
            HollowAnswer::Open => {}
            HollowAnswer::Cancel => self.hollow_prompt = None,
            HollowAnswer::Apply(thickness) => {
                self.hollow_prompt = None;
                self.make_hollow(thickness);
            }
        }
    }

    pub(crate) fn compile_exit_dialog(&mut self, ctx: &egui::Context) {
        if !self.compile_exit_prompt {
            return;
        }
        if !self.compile.is_running() {
            self.compile_exit_prompt = false;
            self.guard_unsaved(ctx, files::PendingOp::Exit);
            return;
        }
        let mut quit = false;
        let mut cancel = false;
        let modal = egui::Modal::new(egui::Id::unique("hp_compile_exit")).show(ctx, |ui| {
            ui.set_width(420.0);
            ui.heading(t!("app.compile_exit_title"));
            ui.add_space(6.0);
            ui.label(t!("app.compile_exit_body"));
            ui.add_space(10.0);
            ui.horizontal(|ui| {
                quit = ui.button(t!("app.compile_exit_quit")).clicked();
                cancel = ui.button(t!("common.cancel")).clicked();
            });
        });
        if quit {
            self.confirm_exit_while_compiling(ctx);
        } else if cancel || closed(&modal) {
            self.compile_exit_prompt = false;
        }
    }

    pub(crate) fn recover_dialog(&mut self, ctx: &egui::Context) {
        let Some(prompt) = self.recover_prompt.clone() else { return };
        let mut open = false;
        let mut keep = false;
        let name = prompt.original.file_name().map(|n| n.to_string_lossy().into_owned()).unwrap_or_default();
        let autosave_name = prompt.autosave.file_name().map(|n| n.to_string_lossy().into_owned()).unwrap_or_default();
        let modal = egui::Modal::new(egui::Id::unique("hp_recover")).show(ctx, |ui| {
            ui.set_width(460.0);
            ui.heading(t!("dialog.recover_title"));
            ui.add_space(6.0);
            ui.label(tf!("dialog.recover_body", name));
            ui.label(egui::RichText::new(tf!("app.recover_detail", autosave_name)).weak());
            ui.add_space(10.0);
            ui.horizontal(|ui| {
                open = ui.button(t!("app.recover_open")).clicked();
                keep = ui.button(t!("app.recover_keep")).clicked();
                if ui.button(t!("app.autosave_folder")).clicked() {
                    crate::app::open_folder(&Settings::autosave_dir());
                }
            });
        });
        if open {
            self.recover_prompt = None;
            let kind = files::OpenKind::Recovered { original: Some(prompt.original.clone()) };
            let target = self.recover_target(Some(&prompt.original));
            self.start_open(ctx, &prompt.autosave, kind, target);
        } else if keep || closed(&modal) {
            self.recover_prompt = None;
        }
    }

    pub(crate) fn crash_dialog(&mut self, ctx: &egui::Context) {
        let Some(crash) = self.crash_prompt.clone() else { return };
        match crash_modal(ctx, &crash) {
            CrashAnswer::Open => {}
            CrashAnswer::OpenSave => {
                self.crash_prompt = None;
                if let Some(save) = &crash.emergency_save {
                    let kind = files::OpenKind::Recovered { original: crash.original.clone() };
                    let target = self.recover_target(crash.original.as_deref());
                    self.start_open(ctx, save, kind, target);
                }
            }
            CrashAnswer::Close => self.crash_prompt = None,
        }
    }
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(crate) enum CrashAnswer {
    Open,
    OpenSave,
    Close,
}

pub(crate) fn crash_modal(ctx: &egui::Context, crash: &crate::crash::PendingCrash) -> CrashAnswer {
    let mut close = false;
    let mut open_save = false;
    let copied_id = egui::Id::unique("hp_crash_report_copied");
    let modal = egui::Modal::new(egui::Id::unique("hp_crash")).show(ctx, |ui| {
        ui.set_width(520.0);
        ui.heading(egui::RichText::new(t!("app.crash_title")).color(Severity::Error.color()));
        ui.add_space(6.0);
        ui.label(t!("app.crash_body"));
        ui.add_space(4.0);
        ui.horizontal_wrapped(|ui| {
            ui.label(egui::RichText::new(t!("app.crash_report_label")).weak());
            ui.add(
                egui::Label::new(egui::RichText::new(crash.report.display().to_string()).monospace()).selectable(true),
            );
        });
        if let Some(save) = &crash.emergency_save {
            ui.add_space(6.0);
            ui.label(tf!("app.crash_saved", save.display()));
        }
        ui.add_space(10.0);
        ui.horizontal_wrapped(|ui| {
            if crash.emergency_save.is_some() {
                open_save = ui.button(t!("app.crash_open_save")).clicked();
            }
            if ui.button(t!("app.crash_open_folder")).on_hover_text(t!("app.crash_open_folder_tip")).clicked()
                && hp_app::os::reveal(&crash.report).is_err()
            {
                crate::app::open_folder(&Settings::crash_dir());
            }
            if ui.button(t!("app.crash_copy")).on_hover_text(t!("app.crash_copy_tip")).clicked() {
                let text = crate::crash::read_report(&crash.report);
                let ok = text.is_some();
                if let Some(text) = text {
                    ui.ctx().copy_text(text);
                }
                ui.ctx().data_mut(|d| d.insert_temp(copied_id, ok));
            }
            close = ui.button(t!("common.close")).clicked();
        });
        match ui.ctx().data(|d| d.get_temp::<bool>(copied_id)) {
            Some(true) => {
                ui.label(egui::RichText::new(t!("app.crash_copied")).color(Severity::Success.color()));
            }
            Some(false) => {
                ui.label(egui::RichText::new(t!("app.crash_copy_failed")).color(Severity::Warning.color()));
            }
            None => {}
        }
    });
    if open_save {
        CrashAnswer::OpenSave
    } else if close || closed(&modal) {
        ctx.data_mut(|d| d.remove::<bool>(copied_id));
        CrashAnswer::Close
    } else {
        CrashAnswer::Open
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use egui::{Event, Key, Modifiers};

    fn key(key: Key) -> Event {
        Event::Key { key, physical_key: Some(key), pressed: true, repeat: false, modifiers: Modifiers::NONE }
    }

    fn release(key: Key) -> Event {
        Event::Key { key, physical_key: Some(key), pressed: false, repeat: false, modifiers: Modifiers::NONE }
    }

    fn frame<T>(ctx: &egui::Context, events: Vec<Event>, mut show: impl FnMut(&egui::Context) -> T) -> Vec<T> {
        let raw = egui::RawInput {
            screen_rect: Some(egui::Rect::from_min_size(egui::Pos2::ZERO, egui::vec2(1200.0, 800.0))),
            events,
            ..Default::default()
        };
        let mut answers = Vec::new();
        let mut out = ctx.run_ui(raw, |ui| answers.push(show(ui.ctx())));
        out.textures_delta.clear();
        answers
    }

    fn setup() -> egui::Context {
        hp_app::register_translations();
        hp_app::i18n::register(crate::app::TR);
        egui::Context::default()
    }

    fn hollow(ctx: &egui::Context, prompt: &mut HollowPrompt, events: Vec<Event>) -> HollowAnswer {
        let answers = frame(ctx, events, |ctx| prompt.show(ctx));
        answers.into_iter().find(|a| *a != HollowAnswer::Open).unwrap_or(HollowAnswer::Open)
    }

    #[test]
    fn hollow_prompt_typing_replaces_the_thickness_and_enter_applies() {
        let ctx = setup();
        let mut prompt = HollowPrompt::new(32.0);
        for _ in 0..3 {
            assert_eq!(hollow(&ctx, &mut prompt, vec![]), HollowAnswer::Open);
        }
        assert!(ctx.text_edit_focused(), "the thickness field has the keyboard focus");
        assert_eq!(hollow(&ctx, &mut prompt, vec![Event::Text("4".into())]), HollowAnswer::Open);
        assert_eq!(hollow(&ctx, &mut prompt, vec![key(Key::Enter)]), HollowAnswer::Apply(4.0));
    }

    #[test]
    fn hollow_prompt_enter_applies_the_default_thickness() {
        let ctx = setup();
        let mut prompt = HollowPrompt::new(32.0);
        hollow(&ctx, &mut prompt, vec![]);
        hollow(&ctx, &mut prompt, vec![]);
        assert_eq!(hollow(&ctx, &mut prompt, vec![key(Key::Enter)]), HollowAnswer::Apply(32.0));
    }

    #[test]
    fn hollow_prompt_enter_applies_without_the_field_focus() {
        let ctx = setup();
        let mut prompt = HollowPrompt::new(16.0);
        hollow(&ctx, &mut prompt, vec![]);
        hollow(&ctx, &mut prompt, vec![]);
        ctx.memory_mut(|m| {
            if let Some(id) = m.focused() {
                m.surrender_focus(id);
            }
        });
        assert_eq!(hollow(&ctx, &mut prompt, vec![]), HollowAnswer::Open);
        assert!(!ctx.text_edit_focused(), "the field does not take the focus back");
        assert_eq!(hollow(&ctx, &mut prompt, vec![key(Key::Enter)]), HollowAnswer::Apply(16.0));
    }

    #[test]
    fn hollow_prompt_escape_cancels() {
        let ctx = setup();
        let mut prompt = HollowPrompt::new(32.0);
        hollow(&ctx, &mut prompt, vec![]);
        hollow(&ctx, &mut prompt, vec![]);
        let mut answer = HollowAnswer::Open;
        for _ in 0..2 {
            answer = hollow(&ctx, &mut prompt, vec![key(Key::Escape), release(Key::Escape)]);
            if answer != HollowAnswer::Open {
                break;
            }
        }
        assert_eq!(answer, HollowAnswer::Cancel);
    }

    fn crash_frame(
        ctx: &egui::Context,
        crash: &crate::crash::PendingCrash,
        events: Vec<Event>,
    ) -> (CrashAnswer, Vec<(String, egui::Pos2)>, Option<String>) {
        let raw = egui::RawInput {
            screen_rect: Some(egui::Rect::from_min_size(egui::Pos2::ZERO, egui::vec2(1200.0, 800.0))),
            events,
            ..Default::default()
        };
        let mut answer = CrashAnswer::Open;
        let out = ctx.run_ui(raw, |ui| answer = crash_modal(ui.ctx(), crash));
        let controls = out
            .platform_output
            .accesskit_update
            .iter()
            .flat_map(|u| u.nodes.iter())
            .filter_map(|(_, n)| {
                let b = n.bounds()?;
                let at = egui::pos2(((b.x0 + b.x1) * 0.5) as f32, ((b.y0 + b.y1) * 0.5) as f32);
                let text = n.label().or_else(|| n.value())?;
                Some((text.to_owned(), at))
            })
            .collect();
        let copied = out.platform_output.commands.iter().find_map(|c| match c {
            egui::OutputCommand::CopyText(t) => Some(t.clone()),
            _ => None,
        });
        (answer, controls, copied)
    }

    #[test]
    fn crash_report_shows_its_file_and_copies_its_text() {
        let ctx = setup();
        ctx.enable_accesskit();
        let is = |label: &str, key: &str| {
            [hp_app::i18n::Lang::En, hp_app::i18n::Lang::Fr]
                .into_iter()
                .any(|lang| label == hp_app::i18n::tr_in(key, lang))
        };
        struct Cleanup(std::path::PathBuf);
        impl Drop for Cleanup {
            fn drop(&mut self) {
                let _ = std::fs::remove_dir_all(&self.0);
            }
        }
        let dir = std::env::temp_dir().join(format!("hammerpro_crash_dialog_{}", std::process::id()));
        let _ = std::fs::remove_dir_all(&dir);
        let _cleanup = Cleanup(dir.clone());
        let report = crate::crash::write_report(&dir, "Hammer Pro crash report\nmessage: boom\n").unwrap();
        let crash = crate::crash::PendingCrash { report: report.clone(), emergency_save: None, original: None };
        let mut controls = Vec::new();
        for _ in 0..3 {
            let (answer, c, _) = crash_frame(&ctx, &crash, vec![]);
            assert_eq!(answer, CrashAnswer::Open);
            controls = c;
        }
        let path = report.display().to_string();
        assert!(controls.iter().any(|(l, _)| l == &path), "the path of the report is shown: {controls:?}");
        assert!(controls.iter().any(|(l, _)| is(l, "app.crash_open_folder")), "{controls:?}");
        let at = controls.iter().find(|(l, _)| is(l, "app.crash_copy")).map(|(_, p)| *p).expect("copy button");
        let press = |pressed| Event::PointerButton {
            pos: at,
            button: egui::PointerButton::Primary,
            pressed,
            modifiers: Modifiers::NONE,
        };
        crash_frame(&ctx, &crash, vec![Event::PointerMoved(at)]);
        let (answer, _, copied) = crash_frame(&ctx, &crash, vec![press(true), press(false)]);
        assert_eq!(answer, CrashAnswer::Open, "copying keeps the report shown");
        assert_eq!(copied.as_deref(), Some("Hammer Pro crash report\nmessage: boom\n"));
        let (_, controls, _) = crash_frame(&ctx, &crash, vec![]);
        assert!(controls.iter().any(|(l, _)| is(l, "app.crash_copied")), "{controls:?}");
        std::fs::remove_file(&report).unwrap();
        let (_, _, copied) = crash_frame(&ctx, &crash, vec![press(true), press(false)]);
        assert_eq!(copied, None);
        let (_, controls, _) = crash_frame(&ctx, &crash, vec![]);
        assert!(controls.iter().any(|(l, _)| is(l, "app.crash_copy_failed")), "{controls:?}");
        let close = controls.iter().find(|(l, _)| is(l, "common.close")).map(|(_, p)| *p).expect("close button");
        let click = |pressed| Event::PointerButton {
            pos: close,
            button: egui::PointerButton::Primary,
            pressed,
            modifiers: Modifiers::NONE,
        };
        crash_frame(&ctx, &crash, vec![Event::PointerMoved(close)]);
        let (answer, _, _) = crash_frame(&ctx, &crash, vec![click(true), click(false)]);
        assert_eq!(answer, CrashAnswer::Close);
    }

    fn unsaved(ctx: &egui::Context, events: Vec<Event>) -> Option<UnsavedAnswer> {
        frame(ctx, events, |ctx| unsaved_modal(ctx, "Save the changes?")).into_iter().flatten().next()
    }

    #[test]
    fn unsaved_prompt_enter_saves() {
        let ctx = setup();
        assert_eq!(unsaved(&ctx, vec![]), None);
        assert_eq!(unsaved(&ctx, vec![]), None);
        assert_eq!(unsaved(&ctx, vec![key(Key::Enter)]), Some(UnsavedAnswer::Save));
    }

    #[test]
    fn unsaved_prompt_enter_presses_the_button_focused_with_tab() {
        let ctx = setup();
        unsaved(&ctx, vec![]);
        unsaved(&ctx, vec![]);
        assert_eq!(unsaved(&ctx, vec![key(Key::Tab), release(Key::Tab)]), None);
        assert_eq!(unsaved(&ctx, vec![key(Key::Tab), release(Key::Tab)]), None);
        assert_eq!(unsaved(&ctx, vec![key(Key::Enter)]), Some(UnsavedAnswer::DontSave));
    }

    #[test]
    fn unsaved_prompt_escape_cancels() {
        let ctx = setup();
        unsaved(&ctx, vec![]);
        unsaved(&ctx, vec![]);
        assert_eq!(unsaved(&ctx, vec![key(Key::Escape)]), Some(UnsavedAnswer::Cancel));
    }
}
