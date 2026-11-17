use egui::{RichText, Ui};
use hp_app::{AppState, Severity, t, tf};
use hp_editor::rename_refs::{RENAME_LABEL, RefsKept, RenameReport};
use hp_editor::{Document, EditError, EditResult, Id};
use hp_fgd::ResolvedFgd;

use crate::view::SelectionSummary;

pub fn report_text(r: &RenameReport) -> Option<(Severity, String)> {
    let mut parts: Vec<String> = Vec::new();
    if r.references > 0 {
        parts.push(tf!("entities.rename_refs_done", r.references));
    }
    if let Some(why) = r.kept {
        parts.push(match why {
            RefsKept::SharedName => tf!("entities.rename_refs_kept_shared", r.kept_references),
            RefsKept::ClassName => tf!("entities.rename_refs_kept_class", r.kept_references),
            RefsKept::Special => tf!("entities.rename_refs_kept_special", r.kept_references),
        });
    }
    if r.wildcards > 0 {
        parts.push(tf!("entities.rename_refs_wildcards", r.wildcards));
    }
    if parts.is_empty() {
        return None;
    }
    let severity = if r.kept.is_some() || r.wildcards > 0 { Severity::Warning } else { Severity::Success };
    Some((severity, parts.join(" ")))
}

pub fn merge_reports(total: &mut RenameReport, r: RenameReport) {
    total.renamed += r.renamed;
    total.references += r.references;
    total.wildcards = total.wildcards.max(r.wildcards);
    total.kept_references += r.kept_references;
    total.kept = total.kept.or(r.kept);
}

pub fn names_of(doc: &Document, ids: &[Id]) -> (Vec<String>, Vec<Id>) {
    let scene = doc.scene();
    let mut names: Vec<String> = Vec::new();
    let mut unnamed: Vec<Id> = Vec::new();
    for id in ids {
        let Some(n) = scene.entity(*id) else { continue };
        match n.entity.get("targetname").map(str::trim).filter(|t| !t.is_empty()) {
            Some(name) => {
                if !names.iter().any(|o| o.eq_ignore_ascii_case(name)) {
                    names.push(name.to_owned());
                }
            }
            None => unnamed.push(*id),
        }
    }
    (names, unnamed)
}

pub fn selected_names(state: &AppState) -> (Vec<Id>, Vec<String>) {
    let ids = SelectionSummary::of(&state.doc).entities;
    let names = names_of(&state.doc, &ids).0;
    (ids, names)
}

pub fn rename_ids(
    doc: &mut Document,
    fgd: Option<&ResolvedFgd>,
    ids: &[Id],
    new: &str,
    update_references: bool,
    name_unnamed: bool,
) -> EditResult<RenameReport> {
    let new = new.trim();
    if new.is_empty() || new.contains('"') {
        return Err(EditError::Invalid("invalid entity name".to_owned()));
    }
    let (olds, unnamed) = names_of(doc, ids);
    doc.begin(RENAME_LABEL);
    let mut run = || -> EditResult<RenameReport> {
        let mut total = RenameReport::default();
        for old in &olds {
            let r = hp_app::runtime::helpers::rename_entities(doc, fgd, Some(ids), old, new, update_references)?;
            merge_reports(&mut total, r);
        }
        if name_unnamed && !unnamed.is_empty() {
            total.renamed += doc.set_entity_key(&unnamed, "targetname", new)?;
        }
        Ok(total)
    };
    match run() {
        Ok(r) => {
            doc.commit();
            Ok(r)
        }
        Err(e) => {
            doc.cancel();
            Err(e)
        }
    }
}

pub fn rename_selection(doc: &mut Document, fgd: Option<&ResolvedFgd>, new: &str) -> EditResult<RenameReport> {
    if new.contains(',') {
        return Err(EditError::Invalid("an entity name cannot contain a comma".to_owned()));
    }
    let ids = SelectionSummary::of(doc).entities;
    rename_ids(doc, fgd, &ids, new, true, false)
}

#[derive(Default)]
pub(crate) struct RenameWindow {
    open: bool,
    name: String,
    focus: bool,
}

impl RenameWindow {
    pub fn show(
        &mut self,
        ctx: &egui::Context,
        state: &mut AppState,
        fgd_override: Option<&ResolvedFgd>,
        focus_before: bool,
    ) {
        if std::mem::take(&mut state.runtime.entities.rename_references) {
            self.open = true;
            self.focus = true;
            self.name = selected_names(state).1.first().cloned().unwrap_or_default();
        }
        if !self.open {
            return;
        }
        let fgd = fgd_override.or_else(|| state.assets.fgd());
        let mut open = true;
        let mut apply = false;
        let resp = hp_app::chrome::Window::new(t!("entities.rename_title"))
            .id(egui::Id::unique("hp_ui_entities.rename_window"))
            .open(&mut open)
            .resizable(false)
            .collapsible(false)
            .default_width(380.0)
            .show(ctx, |ui| apply = self.body(ui, state, fgd));
        if crate::dialogs::escape_closes(ctx, resp.as_ref().map(|r| r.response.layer_id), focus_before) {
            open = false;
        }
        if apply {
            let AppState { doc, assets, .. } = &mut *state;
            let fgd = fgd_override.or_else(|| assets.fgd());
            let done = rename_selection(doc, fgd, &self.name);
            match done {
                Ok(r) => {
                    let text = report_text(&r).unwrap_or((Severity::Info, t!("entities.rename_refs_none").to_owned()));
                    state.notify(text.0, text.1);
                    open = false;
                }
                Err(e) => state.notify(Severity::Error, tf!("entities.edit_failed", e)),
            }
        }
        self.open = open;
    }

    fn body(&mut self, ui: &mut Ui, state: &AppState, fgd: Option<&ResolvedFgd>) -> bool {
        ui.set_max_width(380.0);
        ui.label(t!("entities.rename_help"));
        ui.add_space(4.0);
        let (ids, olds) = selected_names(state);
        if olds.is_empty() {
            ui.label(RichText::new(t!("entities.rename_no_name")).weak());
            return false;
        }
        ui.label(tf!("entities.rename_from", olds.join(", ")));
        let mut enter = false;
        ui.horizontal(|ui| {
            ui.label(t!("entities.rename_new"));
            let r = ui.add(egui::TextEdit::singleline(&mut self.name).desired_width(220.0));
            if std::mem::take(&mut self.focus) {
                r.request_focus();
            }
            enter = r.lost_focus() && ui.input(|i| i.key_pressed(egui::Key::Enter));
        });
        let new = self.name.trim();
        let mut total = RenameReport::default();
        let valid = !new.is_empty() && !new.contains(['"', ',']);
        if valid {
            for old in &olds {
                if let Ok(r) = hp_app::runtime::helpers::rename_preview(&state.doc, fgd, Some(&ids), old, new) {
                    merge_reports(&mut total, r);
                }
            }
            ui.separator();
            ui.label(tf!("entities.rename_preview", total.renamed, total.references));
            if let Some((_, warn)) = report_text(&RenameReport { references: 0, ..total.clone() }) {
                ui.label(RichText::new(warn).color(hp_app::theme::warning()));
            }
        }
        let can = valid && (total.renamed > 0 || total.references > 0);
        ui.add_space(4.0);
        let clicked = ui.add_enabled(can, egui::Button::new(t!("entities.rename_apply"))).clicked();
        can && (clicked || enter)
    }
}
