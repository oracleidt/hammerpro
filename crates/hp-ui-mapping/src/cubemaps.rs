use egui::{RichText, Ui};
use glam::DVec3;
use hp_app::{Action, AppState, Severity, t, tf, theme};
use hp_editor::optimize::cubemaps::{CubemapReport, Suggestion, size_pixels};
use hp_editor::optimize::{Finding, Impact};

use crate::dialogs::MapDialog;
use crate::optimize::{finding_text, go_to};
use crate::util::{escape_pressed, icon_title};

pub(crate) fn dialogs() -> Vec<Box<dyn MapDialog>> {
    vec![Box::new(CubemapWindow)]
}

pub(crate) struct CubemapWindow;

pub const SIZES: [i32; 5] = [0, 6, 7, 8, 9];

pub fn size_label(size: i32) -> String {
    if size <= 0 { tf!("cubemaps.size_default", size_pixels(0)) } else { format!("{0}x{0}", size_pixels(size)) }
}

pub fn suggestion_text(s: &Suggestion) -> String {
    let at = format!("{:.0} {:.0} {:.0}", s.origin.x, s.origin.y, s.origin.z);
    match s.probe {
        Some(id) => tf!("cubemaps.suggest_sides", s.faces, id),
        None if s.sides.is_empty() => tf!("cubemaps.suggest_new", at, s.faces),
        None => tf!("cubemaps.suggest_new_sides", at, s.faces, s.sides.len()),
    }
}

pub fn accept(state: &mut AppState, list: Vec<Suggestion>) -> usize {
    if list.is_empty() || state.doc.in_transaction() {
        return 0;
    }
    match state.doc.place_cubemaps(&list) {
        Ok(ids) => {
            state.notify(Severity::Success, tf!("cubemaps.accepted", ids.len()));
            state.runtime.cubemaps.request = true;
            ids.len()
        }
        Err(e) => {
            state.notify(Severity::Warning, tf!("cubemaps.accept_failed", e));
            0
        }
    }
}

fn with_size(state: &AppState, s: &Suggestion) -> Suggestion {
    let mut s = s.clone();
    if s.is_new() {
        s.size = state.runtime.cubemaps.size.unwrap_or(s.size).clamp(0, 12);
    }
    s
}

fn go_to_point(state: &mut AppState, p: DVec3) {
    let f =
        Finding::new(hp_editor::optimize::Topic::Cubemaps, "suggestion", Impact::Low, String::new()).points(vec![p]);
    go_to(state, &f);
}

impl MapDialog for CubemapWindow {
    fn show(&mut self, ctx: &egui::Context, state: &mut AppState) {
        if !state.runtime.cubemaps.show {
            return;
        }
        let mut open = true;
        let screen = ctx.content_rect();
        let response = hp_app::chrome::Window::new(icon_title("◈", t!("cubemaps.title")))
            .id(egui::Id::unique("hp_cubemaps_window"))
            .default_pos(egui::pos2(screen.right() - 540.0, screen.top() + 140.0))
            .default_size(egui::vec2(480.0, 560.0))
            .resizable(true)
            .collapsible(true)
            .open(&mut open)
            .show(ctx, |ui| body(ui, state));
        if let Some(r) = &response
            && escape_pressed(ctx, &r.response)
        {
            open = false;
        }
        if !open {
            state.runtime.cubemaps.show = false;
        }
    }
}

fn body(ui: &mut Ui, state: &mut AppState) {
    ui.label(RichText::new(t!("cubemaps.intro")).weak().small());
    ui.horizontal_wrapped(|ui| {
        if ui.button(t!("cubemaps.analyze")).on_hover_text(t!("cubemaps.analyze_tip")).clicked() {
            state.runtime.cubemaps.request = true;
        }
        let mut view = state.runtime.cubemaps.view;
        if ui
            .checkbox(&mut view, Action::ToggleCubemapAssignment.label())
            .on_hover_text(t!("cubemaps.view_tip"))
            .changed()
        {
            state.request(Action::ToggleCubemapAssignment);
        }
        let waiting = state.runtime.cubemaps.wait_vis || state.runtime.optimize.running;
        if waiting {
            ui.spinner();
            ui.label(RichText::new(t!("cubemaps.areas_running")).small());
        } else if ui.button(t!("cubemaps.areas")).on_hover_text(t!("cubemaps.areas_tip")).clicked() {
            state.runtime.cubemaps.wait_vis = true;
            state.request(Action::AnalyzeOptimization);
        }
    });
    if let Some(e) = &state.runtime.cubemaps.error {
        ui.label(RichText::new(e).color(theme::error()));
    }
    let Some(report) = state.runtime.cubemaps.report.clone() else {
        ui.label(RichText::new(t!("cubemaps.none")).weak());
        return;
    };
    if state.runtime.cubemaps.stale(state.doc.scene_revision()) {
        ui.label(RichText::new(t!("cubemaps.stale")).color(theme::warning()).small());
    }
    summary(ui, &report);
    ui.separator();
    suggestions(ui, state, &report);
    ui.separator();
    findings(ui, state, &report);
}

fn summary(ui: &mut Ui, r: &CubemapReport) {
    ui.label(tf!(
        "cubemaps.summary",
        r.probes.len(),
        format!("{:.1}", r.memory_bytes as f64 / (1024.0 * 1024.0)),
        r.reflective,
        r.rooms.len()
    ));
    ui.label(tf!("cubemaps.summary_wrong", r.wrong, r.blocked, r.far));
    if r.unfixable > 0 {
        ui.label(RichText::new(tf!("cubemaps.unfixable", r.unfixable)).weak().small());
    }
    let areas = if r.compiler_areas { t!("cubemaps.areas_used") } else { t!("cubemaps.areas_unused") };
    ui.label(RichText::new(areas).weak().small());
}

fn suggestions(ui: &mut Ui, state: &mut AppState, r: &CubemapReport) {
    let pending: Vec<(usize, Suggestion)> =
        state.runtime.cubemaps.pending().into_iter().map(|(i, s)| (i, s.clone())).collect();
    ui.horizontal_wrapped(|ui| {
        ui.label(RichText::new(tf!("cubemaps.suggestions", pending.len())).strong());
        ui.label(t!("cubemaps.size"));
        let mut size = state.runtime.cubemaps.size;
        let shown = size.map_or_else(|| t!("cubemaps.size_suggested").to_owned(), size_label);
        egui::ComboBox::from_id_salt("hp_cubemap_size").selected_text(shown).show_ui(ui, |ui| {
            ui.selectable_value(&mut size, None, t!("cubemaps.size_suggested"));
            for s in SIZES {
                ui.selectable_value(&mut size, Some(s), size_label(s));
            }
        });
        state.runtime.cubemaps.size = size;
    });
    if pending.is_empty() {
        let text = if r.suggestions.is_empty() { t!("cubemaps.no_suggestions") } else { t!("cubemaps.all_dismissed") };
        ui.label(RichText::new(text).color(theme::success()));
        return;
    }
    ui.horizontal(|ui| {
        if ui.button(t!("cubemaps.accept_all")).on_hover_text(t!("cubemaps.accept_all_tip")).clicked() {
            let list: Vec<Suggestion> = pending.iter().map(|(_, s)| with_size(state, s)).collect();
            accept(state, list);
        }
        if ui.button(t!("cubemaps.dismiss_all")).clicked() {
            state.runtime.cubemaps.dismissed.extend(pending.iter().map(|(i, _)| *i));
        }
    });
    egui::ScrollArea::vertical().id_salt("hp_cubemap_suggestions").max_height(200.0).auto_shrink([false, true]).show(
        ui,
        |ui| {
            for (i, s) in &pending {
                let focused = state.runtime.cubemaps.focus == Some(*i);
                egui::Frame::group(ui.style()).show(ui, |ui| {
                    ui.label(suggestion_text(s));
                    ui.horizontal(|ui| {
                        if ui.add(egui::Button::new(t!("optimize.go_to")).selected(focused)).clicked() {
                            state.runtime.cubemaps.focus = Some(*i);
                            go_to_point(state, s.origin);
                        }
                        if ui.button(t!("cubemaps.accept")).clicked() {
                            let one = with_size(state, s);
                            accept(state, vec![one]);
                        }
                        if ui.button(t!("cubemaps.dismiss")).clicked() {
                            state.runtime.cubemaps.dismissed.push(*i);
                        }
                    });
                });
            }
        },
    );
}

fn findings(ui: &mut Ui, state: &mut AppState, r: &CubemapReport) {
    ui.label(RichText::new(tf!("cubemaps.findings", r.findings.len())).strong());
    if r.findings.is_empty() {
        ui.label(RichText::new(t!("cubemaps.clean")).color(theme::success()));
        return;
    }
    egui::ScrollArea::vertical().id_salt("hp_cubemap_findings").auto_shrink([false, false]).show(ui, |ui| {
        for (i, f) in r.findings.iter().enumerate() {
            let selected = state.runtime.cubemaps.selected == Some(i);
            egui::Frame::group(ui.style()).show(ui, |ui| {
                ui.horizontal(|ui| {
                    let (label, color) = match f.impact {
                        Impact::High => (t!("optimize.impact.high"), theme::error()),
                        Impact::Medium => (t!("optimize.impact.medium"), theme::warning()),
                        Impact::Low => (t!("optimize.impact.low"), theme::text_weak()),
                    };
                    ui.label(RichText::new(label).color(color).strong());
                    let can_go = !f.objects.is_empty() || !f.focus().is_empty();
                    ui.with_layout(egui::Layout::right_to_left(egui::Align::Center), |ui| {
                        if ui.add_enabled(can_go, egui::Button::new(t!("optimize.go_to")).selected(selected)).clicked()
                        {
                            state.runtime.cubemaps.selected = Some(i);
                            go_to(state, f);
                        }
                    });
                });
                ui.label(finding_text(f));
            });
        }
    });
}

#[cfg(test)]
mod tests {
    use hp_app::testing::{run_ctx, test_state};
    use hp_editor::optimize::cubemaps::CubemapReport;

    use super::*;

    #[test]
    fn window_draws_every_state_and_accepts_one_undo_step() {
        let _lang = crate::lang_lock();
        crate::register_translations();
        let mut state = test_state();
        let mut w = CubemapWindow;
        run_ctx(&mut state, |ctx, state| w.show(ctx, state));
        state.runtime.cubemaps.show = true;
        run_ctx(&mut state, |ctx, state| w.show(ctx, state));
        let new = Suggestion {
            room: Some(0),
            origin: DVec3::new(10.0, 20.0, 64.0),
            size: 0,
            sides: Vec::new(),
            faces: 4,
            probe: None,
            reason: "room_without_cubemap",
        };
        let sides = Suggestion { sides: vec![1, 2], ..new.clone() };
        let f = Finding::new(hp_editor::optimize::Topic::Cubemaps, "cubemap_far", Impact::Low, "m".to_owned())
            .points(vec![DVec3::ZERO])
            .args(vec!["3".to_owned(), "1024".to_owned()]);
        let report = CubemapReport {
            suggestions: vec![new.clone(), sides.clone()],
            findings: vec![f],
            revision: state.doc.scene_revision(),
            ..Default::default()
        };
        state.runtime.cubemaps.store(report, 1);
        for lang in [hp_app::i18n::Lang::En, hp_app::i18n::Lang::Fr, hp_app::i18n::Lang::Ru] {
            hp_app::i18n::set_lang(lang);
            run_ctx(&mut state, |ctx, state| w.show(ctx, state));
            assert!(!suggestion_text(&new).is_empty() && !suggestion_text(&sides).is_empty());
            assert!(!size_label(0).is_empty());
        }
        hp_app::i18n::set_lang(hp_app::i18n::Lang::En);
        assert!(suggestion_text(&new).contains("10 20 64"));
        let before = state.doc.scene().entities().len();
        state.runtime.cubemaps.size = Some(8);
        let list: Vec<Suggestion> = vec![with_size(&state, &new)];
        assert_eq!(accept(&mut state, list), 1);
        assert_eq!(state.doc.scene().entities().len(), before + 1);
        let probe = state.doc.scene().entities().values().find(|n| n.entity.classname == "env_cubemap").expect("probe");
        assert_eq!(probe.entity.get("cubemapsize"), Some("8"));
        assert!(state.runtime.cubemaps.request);
        assert!(state.doc.undo());
        assert_eq!(state.doc.scene().entities().len(), before);
        state.runtime.cubemaps.dismissed = vec![0, 1];
        run_ctx(&mut state, |ctx, state| w.show(ctx, state));
        state.runtime.cubemaps.error = Some("x".to_owned());
        run_ctx(&mut state, |ctx, state| w.show(ctx, state));
    }
}
