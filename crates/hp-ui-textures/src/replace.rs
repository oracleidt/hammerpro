use std::collections::HashMap;

use glam::DVec2;
use hp_app::{AppState, Severity, t, tf, theme};
use hp_editor::{Document, EditResult, FaceEdit, FaceRef, ReplaceScope};

use crate::icons::{Icon, icon_button};
use crate::picker::{MaterialPicker, PickerOutcome};
use crate::search::Category;
use crate::util::normalize_material_into;

#[derive(Clone, Copy, Debug, Default, PartialEq, Eq, Hash)]
pub enum MatchMode {
    #[default]
    Exact,
    Wildcard,
}

#[derive(Clone, Debug, Default, PartialEq, Eq)]
pub struct MatchCount {
    pub faces: Vec<FaceRef>,
    pub materials: Vec<(String, usize)>,
}

pub fn normalize_pattern(pattern: &str) -> String {
    crate::util::normalize_material(pattern)
}

pub fn matches(pattern: &str, mode: MatchMode, material: &str) -> bool {
    match mode {
        MatchMode::Exact => pattern == material,
        MatchMode::Wildcard => hp_editor::glob_match(pattern, material),
    }
}

fn faces_in_scope(doc: &Document, selection: bool) -> Vec<FaceRef> {
    if selection {
        let mut v = doc.all_faces_of(&doc.selected_objects());
        for f in doc.selected_faces() {
            if !v.contains(&f) {
                v.push(f);
            }
        }
        v
    } else {
        let mut v = Vec::new();
        for n in doc.scene().solids().values() {
            v.extend((0..n.solid.sides.len()).map(|i| FaceRef::new(n.solid.id, i)));
        }
        v
    }
}

pub fn count_matches(doc: &Document, pattern: &str, mode: MatchMode, selection: bool) -> MatchCount {
    let pattern = normalize_pattern(pattern);
    let mut out = MatchCount::default();
    if pattern.is_empty() {
        return out;
    }
    let mut counts: HashMap<String, usize> = HashMap::new();
    let mut buf = String::new();
    for f in faces_in_scope(doc, selection) {
        let Some(side) = doc.face_side(f) else { continue };
        normalize_material_into(&side.material, &mut buf);
        if matches(&pattern, mode, &buf) {
            out.faces.push(f);
            match counts.get_mut(buf.as_str()) {
                Some(c) => *c += 1,
                None => {
                    counts.insert(buf.clone(), 1);
                }
            }
        }
    }
    out.materials = counts.into_iter().collect();
    out.materials.sort();
    out
}

pub fn replacement_for(pattern: &str, replace: &str, mode: MatchMode, material: &str) -> Option<String> {
    let pattern = normalize_pattern(pattern);
    let replace = normalize_pattern(replace);
    match mode {
        MatchMode::Exact => (pattern == material).then_some(replace),
        MatchMode::Wildcard => hp_editor::glob_replace(&pattern, &replace, material),
    }
}

pub fn replace(
    doc: &mut Document,
    pattern: &str,
    replace: &str,
    mode: MatchMode,
    selection: bool,
    rescale: Option<&HashMap<String, (u32, u32)>>,
) -> EditResult<(usize, usize)> {
    let find = normalize_pattern(pattern);
    let repl = normalize_pattern(replace);
    if find.is_empty() || repl.is_empty() {
        return Err(hp_editor::EditError::Invalid("empty material name".to_owned()));
    }
    if mode == MatchMode::Exact && find.contains(['*', '?']) {
        return Err(hp_editor::EditError::Invalid("wildcards need the wildcard mode".to_owned()));
    }
    let mut groups: Vec<(DVec2, Vec<FaceRef>)> = Vec::new();
    let mut unknown = 0;
    if let Some(sizes) = rescale {
        let matched = count_matches(doc, &find, mode, selection);
        let mut buf = String::new();
        for f in matched.faces {
            let Some(side) = doc.face_side(f) else { continue };
            normalize_material_into(&side.material, &mut buf);
            let Some(new) = replacement_for(&find, &repl, mode, &buf) else { continue };
            match (sizes.get(buf.as_str()), sizes.get(new.as_str())) {
                (Some(&(ow, oh)), Some(&(nw, nh))) if ow > 0 && oh > 0 && nw > 0 && nh > 0 => {
                    let k = DVec2::new(ow as f64 / nw as f64, oh as f64 / nh as f64);
                    if (k - DVec2::ONE).abs().max_element() < 1e-9 {
                        continue;
                    }
                    match groups.iter_mut().find(|(g, _)| (*g - k).abs().max_element() < 1e-9) {
                        Some((_, list)) => list.push(f),
                        None => groups.push((k, vec![f])),
                    }
                }
                _ => unknown += 1,
            }
        }
    }
    let scope = if selection { ReplaceScope::Selection } else { ReplaceScope::All };
    let n = doc.transaction("Replace materials", |doc| {
        let n = doc.replace_materials(&find, &repl, &scope)?;
        for (k, faces) in &groups {
            doc.edit_faces(faces, &FaceEdit { scale_by: Some(*k), ..FaceEdit::default() })?;
        }
        Ok::<usize, hp_editor::EditError>(n)
    })?;
    Ok((n, unknown))
}

fn find_field_id() -> egui::Id {
    egui::Id::unique("hp_textures_replace_find")
}

fn replace_field_id() -> egui::Id {
    egui::Id::unique("hp_textures_replace_with")
}

#[derive(Default)]
struct CountCache {
    valid: bool,
    revision: u64,
    find: String,
    mode: MatchMode,
    selection: bool,
    count: MatchCount,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum ReplaceField {
    Find,
    Replace,
}

pub struct ReplaceDialog {
    find: String,
    replace: String,
    mode: MatchMode,
    selection_only: bool,
    rescale: bool,
    cache: CountCache,
    focus_find: bool,
    focus_replace: bool,
    was_open: bool,
    message: Option<(Severity, String)>,
    escape: crate::util::DialogEscape,
    find_picker: MaterialPicker,
    replace_picker: MaterialPicker,
    pick_buttons: [Option<egui::Rect>; 2],
}

impl Default for ReplaceDialog {
    fn default() -> Self {
        Self {
            find: String::new(),
            replace: String::new(),
            mode: MatchMode::default(),
            selection_only: false,
            rescale: false,
            cache: CountCache::default(),
            focus_find: false,
            focus_replace: false,
            was_open: false,
            message: None,
            escape: crate::util::DialogEscape::default(),
            find_picker: MaterialPicker::new("replace_find"),
            replace_picker: MaterialPicker::new("replace_with"),
            pick_buttons: [None; 2],
        }
    }
}

impl ReplaceDialog {
    pub fn prefill(&mut self, find: &str, replace: &str) {
        if !find.is_empty() {
            self.find = find.to_owned();
        }
        if !replace.is_empty() {
            self.replace = replace.to_owned();
        }
        self.focus_find = true;
        self.message = None;
    }

    pub fn fields(&self) -> (&str, &str) {
        (&self.find, &self.replace)
    }

    pub fn set(&mut self, find: &str, replace: &str, mode: MatchMode, selection_only: bool, rescale: bool) {
        self.find = find.to_owned();
        self.replace = replace.to_owned();
        self.mode = mode;
        self.selection_only = selection_only;
        self.rescale = rescale;
    }

    pub fn picker(&mut self, field: ReplaceField) -> &mut MaterialPicker {
        match field {
            ReplaceField::Find => &mut self.find_picker,
            ReplaceField::Replace => &mut self.replace_picker,
        }
    }

    pub fn pick_button_rect(&self, field: ReplaceField) -> Option<egui::Rect> {
        self.pick_buttons[field as usize]
    }

    pub fn open_picker(&mut self, ctx: &egui::Context, state: &AppState, field: ReplaceField) {
        let default = match field {
            ReplaceField::Find => Category::Used,
            ReplaceField::Replace if state.settings.browser.recent_materials.is_empty() => Category::All,
            ReplaceField::Replace => Category::Recent,
        };
        self.picker(field).open(ctx, default);
    }

    fn pick_button(&mut self, ui: &mut egui::Ui, state: &AppState, field: ReplaceField) -> egui::Response {
        let r = icon_button(ui, Icon::Image, t!("tex.replace.pick"));
        self.pick_buttons[field as usize] = Some(r.rect);
        if r.clicked() {
            if self.picker(field).is_open(ui.ctx()) {
                self.picker(field).close(ui.ctx());
            } else {
                self.open_picker(ui.ctx(), state, field);
            }
        } else if r.secondary_clicked() {
            self.picker(field).close(ui.ctx());
            let current = state.current_material.clone();
            match field {
                ReplaceField::Find => self.find = current,
                ReplaceField::Replace => self.replace = current,
            }
        }
        r
    }

    fn show_picker(&mut self, ui: &egui::Ui, state: &mut AppState, field: ReplaceField, anchor: egui::Rect) {
        let (picker, value) = match field {
            ReplaceField::Find => (&mut self.find_picker, &mut self.find),
            ReplaceField::Replace => (&mut self.replace_picker, &mut self.replace),
        };
        let outcome = picker.show(ui.ctx(), anchor, ui.layer_id(), state, value);
        if let Some(PickerOutcome::Picked(name)) = &outcome {
            name.clone_into(value);
            self.message = None;
        }
        if outcome.is_some() {
            match field {
                ReplaceField::Find => self.focus_find = true,
                ReplaceField::Replace => self.focus_replace = true,
            }
        }
    }

    pub fn match_count(&mut self, doc: &Document) -> &MatchCount {
        let rev = doc.revision();
        let c = &self.cache;
        let fresh = c.valid
            && c.revision == rev
            && c.find == self.find
            && c.mode == self.mode
            && c.selection == self.selection_only;
        if !fresh {
            self.cache = CountCache {
                valid: true,
                revision: rev,
                find: self.find.clone(),
                mode: self.mode,
                selection: self.selection_only,
                count: count_matches(doc, &self.find, self.mode, self.selection_only),
            };
        }
        &self.cache.count
    }

    pub fn run(&mut self, state: &mut AppState) -> Option<usize> {
        let sizes = if self.rescale {
            let matched = self.match_count(&state.doc).clone();
            let mut names: Vec<String> = matched.materials.iter().map(|(m, _)| m.clone()).collect();
            for (m, _) in &matched.materials {
                if let Some(r) = replacement_for(&self.find, &self.replace, self.mode, m) {
                    names.push(r);
                }
            }
            Some(crate::util::material_sizes(state, names.iter().map(String::as_str)))
        } else {
            None
        };
        match replace(&mut state.doc, &self.find, &self.replace, self.mode, self.selection_only, sizes.as_ref()) {
            Ok((n, unknown)) => {
                let text = tf!("tex.replace.done", n);
                state.notify(Severity::Success, text.clone());
                if unknown > 0 {
                    state.log(Severity::Warning, tf!("tex.replace.rescale_unknown", unknown));
                }
                self.message = Some((Severity::Success, text));
                Some(n)
            }
            Err(e) => {
                let text = tf!("tex.replace.failed", e);
                state.notify(Severity::Error, text.clone());
                self.message = Some((Severity::Error, text));
                None
            }
        }
    }

    pub fn show(&mut self, ctx: &egui::Context, state: &mut AppState) {
        if let Some(p) = crate::shared::take_replace(ctx) {
            self.prefill(&p.find, &p.replace);
            state.ui.show_replace_textures = true;
        }
        if !state.ui.show_replace_textures {
            self.was_open = false;
            return;
        }
        if !self.was_open {
            self.was_open = true;
            self.focus_find = true;
            if self.find.trim().is_empty() {
                self.find = state.current_material.clone();
            }
        }
        let mut open = true;
        let mut close = false;
        self.escape.begin(ctx);
        let response = hp_app::chrome::Window::new(t!("tex.replace.title"))
            .id(egui::Id::unique("hp_textures_replace"))
            .open(&mut open)
            .collapsible(false)
            .resizable(false)
            .default_width(460.0)
            .show(ctx, |ui| close = self.ui(ui, state));
        let escape = self.escape.update(ctx, response.as_ref().map(|r| &r.response));
        if !open || close || escape {
            state.ui.show_replace_textures = false;
        }
    }

    fn ui(&mut self, ui: &mut egui::Ui, state: &mut AppState) -> bool {
        let mut enter = false;
        egui::Grid::new("hp_replace_grid").num_columns(2).spacing([8.0, 6.0]).show(ui, |ui| {
            ui.label(t!("tex.replace.find"));
            ui.horizontal(|ui| {
                let r = ui.add(
                    egui::TextEdit::singleline(&mut self.find)
                        .id(find_field_id())
                        .desired_width(260.0)
                        .hint_text(t!("tex.replace.find_hint")),
                );
                if self.focus_find {
                    r.request_focus();
                    self.focus_find = false;
                }
                enter |= r.lost_focus() && ui.input(|i| i.key_pressed(egui::Key::Enter));
                let pick = self.pick_button(ui, state, ReplaceField::Find);
                let has_selection = !state.doc.selection().is_empty();
                if ui
                    .add_enabled_ui(has_selection, |ui| icon_button(ui, Icon::Cursor, t!("tex.replace.use_selection")))
                    .inner
                    .clicked()
                    && let Some(first) = crate::matedit::selection_materials(&state.doc).into_iter().next()
                {
                    self.find = first;
                }
                self.show_picker(ui, state, ReplaceField::Find, r.rect.union(pick.rect));
            });
            ui.end_row();

            ui.label(t!("tex.replace.with"));
            ui.horizontal(|ui| {
                let r = ui.add(
                    egui::TextEdit::singleline(&mut self.replace)
                        .id(replace_field_id())
                        .desired_width(260.0)
                        .hint_text(t!("tex.replace.with_hint")),
                );
                if self.focus_replace {
                    r.request_focus();
                    self.focus_replace = false;
                }
                enter |= r.lost_focus() && ui.input(|i| i.key_pressed(egui::Key::Enter));
                let pick = self.pick_button(ui, state, ReplaceField::Replace);
                self.show_picker(ui, state, ReplaceField::Replace, r.rect.union(pick.rect));
            });
            ui.end_row();

            ui.label(t!("tex.replace.scope"));
            hp_app::widgets::choice_row(ui, |ui| {
                ui.radio_value(&mut self.selection_only, false, t!("tex.replace.scope_all"));
                ui.radio_value(&mut self.selection_only, true, t!("tex.replace.scope_selection"));
            });
            ui.end_row();

            ui.label(t!("tex.replace.mode"));
            hp_app::widgets::choice_row(ui, |ui| {
                ui.radio_value(&mut self.mode, MatchMode::Exact, t!("tex.replace.mode_exact"));
                ui.radio_value(&mut self.mode, MatchMode::Wildcard, t!("tex.replace.mode_wildcard"))
                    .on_hover_text(t!("tex.replace.mode_wildcard_tip"));
            });
            ui.end_row();

            ui.label("");
            ui.checkbox(&mut self.rescale, t!("tex.replace.rescale")).on_hover_text(t!("tex.replace.rescale_tip"));
            ui.end_row();
        });

        ui.separator();
        let wildcard_in_exact = self.mode == MatchMode::Exact && self.find.contains(['*', '?']);
        let (faces, materials) = {
            let c = self.match_count(&state.doc);
            (c.faces.len(), c.materials.clone())
        };
        if wildcard_in_exact {
            ui.colored_label(theme::warning(), t!("tex.replace.needs_wildcard"));
        } else if self.find.trim().is_empty() {
            ui.weak(t!("tex.replace.type_pattern"));
        } else {
            let color = if faces == 0 { theme::warning() } else { theme::success() };
            ui.colored_label(color, tf!("tex.replace.count", faces, materials.len()));
            if materials.len() > 1 || (self.mode == MatchMode::Wildcard && !materials.is_empty()) {
                egui::ScrollArea::vertical().max_height(110.0).id_salt("hp_replace_list").show(ui, |ui| {
                    for (m, n) in materials.iter().take(200) {
                        let target = replacement_for(&self.find, &self.replace, self.mode, m).unwrap_or_default();
                        ui.horizontal(|ui| {
                            ui.monospace(format!("{n:>5}"));
                            ui.label(m);
                            if !self.replace.trim().is_empty() {
                                ui.weak(">");
                                ui.label(target);
                            }
                        });
                    }
                });
            }
        }
        if let Some((sev, text)) = &self.message {
            ui.colored_label(sev.color(), text);
        }
        ui.add_space(4.0);
        let can_run = faces > 0 && !self.replace.trim().is_empty() && !wildcard_in_exact;
        let mut close = false;
        ui.horizontal(|ui| {
            let run = ui
                .add_enabled(can_run, egui::Button::new(egui::RichText::new(t!("tex.replace.run")).strong()))
                .on_hover_text(hp_app::actions::key_label(egui::Key::Enter));
            if run.clicked() || (enter && can_run) {
                self.run(state);
            }
            if ui.button(t!("common.close")).clicked() {
                close = true;
            }
        });
        close
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn counting() {
        let state = hp_app::testing::test_state();
        let c = count_matches(&state.doc, "DEV/DEV_MEASUREGENERIC01B", MatchMode::Exact, false);
        assert_eq!(c.faces.len(), 6);
        assert_eq!(c.materials, [("dev/dev_measuregeneric01b".to_owned(), 6)]);
        let c = count_matches(&state.doc, "*/*", MatchMode::Wildcard, false);
        assert_eq!(c.faces.len(), 18);
        assert_eq!(c.materials.len(), 3);
        let c = count_matches(&state.doc, "concrete/*", MatchMode::Exact, false);
        assert!(c.faces.is_empty());
        let mut state = state;
        state.doc.select(&[hp_editor::ObjRef::Solid(3)], hp_editor::SelectOp::Replace);
        let c = count_matches(&state.doc, "*", MatchMode::Wildcard, true);
        assert_eq!(c.faces.len(), 6);
        assert_eq!(c.materials, [("concrete/concretefloor001a".to_owned(), 6)]);
    }

    #[test]
    fn replacing_with_rescale() {
        let mut state = hp_app::testing::test_state();
        let mut sizes = HashMap::new();
        sizes.insert("dev/dev_measuregeneric01b".to_owned(), (512, 512));
        sizes.insert("brick/big".to_owned(), (1024, 256));
        let before = state.doc.history_len().0;
        let (n, unknown) =
            replace(&mut state.doc, "dev/dev_measuregeneric01b", "brick/big", MatchMode::Exact, false, Some(&sizes))
                .unwrap();
        assert_eq!(n, 6);
        assert_eq!(unknown, 0);
        assert_eq!(state.doc.history_len().0, before + 1);
        let side = state.doc.face_side(FaceRef::new(2, 0)).unwrap();
        assert_eq!(side.material, "brick/big");
        assert!((side.uaxis.scale - 0.125).abs() < 1e-9, "{}", side.uaxis.scale);
        assert!((side.vaxis.scale - 0.5).abs() < 1e-9, "{}", side.vaxis.scale);
        assert!(state.doc.undo());
        let side = state.doc.face_side(FaceRef::new(2, 0)).unwrap();
        assert_eq!(side.uaxis.scale, 0.25);
    }

    #[test]
    fn wildcard_replacement_names() {
        assert_eq!(
            replacement_for("dev/*", "tools/*", MatchMode::Wildcard, "dev/floor").as_deref(),
            Some("tools/floor")
        );
        assert_eq!(replacement_for("dev/floor", "x/y", MatchMode::Exact, "dev/floor").as_deref(), Some("x/y"));
        assert_eq!(replacement_for("dev/floor", "x/y", MatchMode::Exact, "dev/wall"), None);
        let mut state = hp_app::testing::test_state();
        assert!(replace(&mut state.doc, "dev/*", "x", MatchMode::Exact, false, None).is_err());
        assert!(replace(&mut state.doc, "", "x", MatchMode::Exact, false, None).is_err());
        let (n, _) = replace(&mut state.doc, "*/*01?", "custom/*_new", MatchMode::Wildcard, false, None).unwrap();
        assert_eq!(n, 12);
    }

    #[test]
    fn tab_goes_from_find_to_replace() {
        crate::register_translations();
        let mut state = hp_app::testing::test_state();
        state.ui.show_replace_textures = true;
        let mut d = ReplaceDialog::default();
        let ctx = egui::Context::default();
        let frame = |events: Vec<egui::Event>, state: &mut AppState, d: &mut ReplaceDialog| {
            let input = egui::RawInput {
                screen_rect: Some(egui::Rect::from_min_size(egui::Pos2::ZERO, egui::vec2(1200.0, 900.0))),
                events,
                ..Default::default()
            };
            let mut out = ctx.run_ui(input, |ui| {
                let ctx = ui.ctx().clone();
                d.show(&ctx, state);
            });
            out.textures_delta.clear();
        };
        frame(Vec::new(), &mut state, &mut d);
        frame(Vec::new(), &mut state, &mut d);
        assert_eq!(ctx.memory(|m| m.focused()), Some(find_field_id()), "the find field has the focus");
        let tab = egui::Event::Key {
            key: egui::Key::Tab,
            physical_key: None,
            pressed: true,
            repeat: false,
            modifiers: egui::Modifiers::NONE,
        };
        frame(vec![tab], &mut state, &mut d);
        frame(Vec::new(), &mut state, &mut d);
        assert_eq!(ctx.memory(|m| m.focused()), Some(replace_field_id()), "Tab skips the icon buttons");
    }

    struct Harness {
        ctx: egui::Context,
        state: AppState,
        d: ReplaceDialog,
    }

    impl Harness {
        fn new() -> Self {
            crate::register_translations();
            let mut state = hp_app::testing::test_state();
            state.ui.show_replace_textures = true;
            let mut h = Harness { ctx: egui::Context::default(), state, d: ReplaceDialog::default() };
            h.frame(Vec::new());
            h.frame(Vec::new());
            h
        }

        fn frame(&mut self, events: Vec<egui::Event>) -> Vec<egui::Event> {
            let input = egui::RawInput {
                screen_rect: Some(egui::Rect::from_min_size(egui::Pos2::ZERO, egui::vec2(1200.0, 900.0))),
                events,
                ..Default::default()
            };
            let (state, d) = (&mut self.state, &mut self.d);
            let mut left = Vec::new();
            let mut out = self.ctx.run_ui(input, |ui| {
                let ctx = ui.ctx().clone();
                d.show(&ctx, state);
                left = ctx.input(|i| i.events.clone());
            });
            out.textures_delta.clear();
            left
        }

        fn settle(&mut self) {
            for _ in 0..3 {
                self.frame(Vec::new());
            }
        }

        fn click_with(&mut self, pos: egui::Pos2, button: egui::PointerButton) {
            let b = |pressed| egui::Event::PointerButton { pos, button, pressed, modifiers: egui::Modifiers::NONE };
            self.frame(vec![egui::Event::PointerMoved(pos), b(true), b(false)]);
            self.settle();
        }

        fn click(&mut self, pos: egui::Pos2) {
            self.click_with(pos, egui::PointerButton::Primary);
        }

        fn key(&mut self, key: egui::Key) -> Vec<egui::Event> {
            let left = self.frame(vec![egui::Event::Key {
                key,
                physical_key: None,
                pressed: true,
                repeat: false,
                modifiers: egui::Modifiers::NONE,
            }]);
            self.settle();
            left
        }

        fn type_text(&mut self, text: &str) {
            self.frame(vec![egui::Event::Text(text.to_owned())]);
            self.settle();
        }

        fn open_picker(&mut self, field: ReplaceField) {
            let at = self.d.pick_button_rect(field).expect("picker button shown").center();
            self.click(at);
            assert!(self.d.picker(field).is_open(&self.ctx), "{field:?}: the button opens the picker");
        }

        fn focused(&self) -> Option<egui::Id> {
            self.ctx.memory(|m| m.focused())
        }
    }

    fn has_escape(events: &[egui::Event]) -> bool {
        events.iter().any(|e| matches!(e, egui::Event::Key { key: egui::Key::Escape, pressed: true, .. }))
    }

    #[test]
    fn picker_opens_filters_and_fills_the_find_field() {
        let mut h = Harness::new();
        h.open_picker(ReplaceField::Find);
        assert!(egui::Popup::is_any_open(&h.ctx));
        let search = h.d.picker(ReplaceField::Find).search_id();
        assert_eq!(h.focused(), Some(search), "the search box takes the focus");
        let p = h.d.picker(ReplaceField::Find);
        assert_eq!(p.category(), Category::Used);
        assert_eq!(p.shown_names(), ["concrete/concretefloor001a", "dev/dev_measuregeneric01b", "tools/toolstrigger"],);
        h.type_text("measure");
        assert_eq!(h.d.picker(ReplaceField::Find).shown_names(), ["dev/dev_measuregeneric01b"]);
        let cell = h.d.picker(ReplaceField::Find).cell_rect("dev/dev_measuregeneric01b").expect("cell shown");
        h.click(cell.center());
        assert_eq!(h.d.fields(), ("dev/dev_measuregeneric01b", ""));
        assert!(!h.d.picker(ReplaceField::Find).is_open(&h.ctx));
        assert!(h.state.ui.show_replace_textures, "the dialog stays open");
        assert_eq!(h.focused(), Some(find_field_id()), "the field gets the focus back");
        assert_eq!(h.d.match_count(&h.state.doc).faces.len(), 6);
        h.type_text("x");
        assert_eq!(h.d.fields().0, "dev/dev_measuregeneric01bx");
        assert_eq!(h.d.match_count(&h.state.doc).faces.len(), 0);
        h.key(egui::Key::Tab);
        assert_eq!(h.focused(), Some(replace_field_id()));
    }

    #[test]
    fn picker_fills_the_replace_field_with_the_keyboard() {
        let mut h = Harness::new();
        h.d.set("tools/toolstrigger", "", MatchMode::Exact, false, false);
        h.open_picker(ReplaceField::Replace);
        assert!(!h.d.picker(ReplaceField::Find).is_open(&h.ctx));
        let p = h.d.picker(ReplaceField::Replace);
        assert_eq!(p.category(), Category::All);
        assert_eq!(p.shown_names().len(), 5);
        h.key(egui::Key::ArrowDown);
        assert_eq!(h.d.picker(ReplaceField::Replace).cursor(), Some(0));
        h.key(egui::Key::ArrowRight);
        assert_eq!(h.d.picker(ReplaceField::Replace).cursor(), Some(1));
        h.key(egui::Key::ArrowUp);
        assert_eq!(h.d.picker(ReplaceField::Replace).cursor(), None);
        assert_eq!(h.focused(), Some(h.d.picker(ReplaceField::Replace).search_id()));
        h.type_text("brick");
        assert_eq!(h.d.picker(ReplaceField::Replace).shown_names(), ["custom/mytextures/brick01"]);
        let left = h.key(egui::Key::Enter);
        assert!(!left.iter().any(|e| matches!(e, egui::Event::Key { key: egui::Key::Enter, .. })));
        assert_eq!(h.d.fields(), ("tools/toolstrigger", "custom/mytextures/brick01"), "only its own field changes");
        assert!(!h.d.picker(ReplaceField::Replace).is_open(&h.ctx));
        assert_eq!(h.focused(), Some(replace_field_id()));
        assert_eq!(h.d.match_count(&h.state.doc).faces.len(), 6);
        h.key(egui::Key::Enter);
        assert_eq!(h.d.match_count(&h.state.doc).faces.len(), 0);
        assert_eq!(count_matches(&h.state.doc, "custom/mytextures/brick01", MatchMode::Exact, false).faces.len(), 6);
    }

    #[test]
    fn escape_closes_the_picker_then_the_dialog() {
        let mut h = Harness::new();
        h.open_picker(ReplaceField::Find);
        let left = h.key(egui::Key::Escape);
        assert!(!has_escape(&left), "the picker takes the Escape");
        assert!(!h.d.picker(ReplaceField::Find).is_open(&h.ctx));
        assert!(!egui::Popup::is_any_open(&h.ctx));
        assert!(h.state.ui.show_replace_textures, "the dialog stays open");
        assert_eq!(h.focused(), Some(find_field_id()));
        let left = h.key(egui::Key::Escape);
        assert!(!has_escape(&left));
        assert!(!h.state.ui.show_replace_textures, "the next Escape closes the dialog");
    }

    #[test]
    fn picker_closes_on_a_click_outside_and_on_its_button() {
        let mut h = Harness::new();
        h.open_picker(ReplaceField::Find);
        let at = h.d.pick_button_rect(ReplaceField::Find).expect("button").center();
        h.click(at);
        assert!(!h.d.picker(ReplaceField::Find).is_open(&h.ctx));
        h.open_picker(ReplaceField::Find);
        let window = h.ctx.memory(|m| m.area_rect(egui::Id::unique("hp_textures_replace"))).expect("window");
        h.click(window.left_top() + egui::vec2(20.0, 8.0));
        assert!(!h.d.picker(ReplaceField::Find).is_open(&h.ctx));
        assert!(h.state.ui.show_replace_textures);
    }

    #[test]
    fn current_material_shortcuts() {
        let mut h = Harness::new();
        h.state.current_material = "tools/toolsnodraw".to_owned();
        let at = h.d.pick_button_rect(ReplaceField::Replace).expect("button").center();
        h.click_with(at, egui::PointerButton::Secondary);
        assert_eq!(h.d.fields().1, "tools/toolsnodraw");
        assert!(!egui::Popup::is_any_open(&h.ctx));
        h.open_picker(ReplaceField::Find);
        let row = h.d.picker(ReplaceField::Find).current_rect().expect("current material row");
        h.click(row.center());
        assert_eq!(h.d.fields(), ("tools/toolsnodraw", "tools/toolsnodraw"));
        assert!(!h.d.picker(ReplaceField::Find).is_open(&h.ctx));
        h.state.doc.select(&[hp_editor::ObjRef::Solid(3)], hp_editor::SelectOp::Replace);
        h.settle();
        let find = h.d.pick_button_rect(ReplaceField::Find).expect("button");
        let cursor = find.center() + egui::vec2(find.width() + h.ctx.global_style().spacing.item_spacing.x, 0.0);
        h.click(cursor);
        assert_eq!(h.d.fields().0, "concrete/concretefloor001a");
    }

    #[test]
    fn picker_sections() {
        let mut h = Harness::new();
        h.state.settings.browser.recent_materials =
            vec!["tools/toolsnodraw".into(), "dev/dev_measuregeneric01b".into()];
        h.state.settings.browser.favorites = vec!["custom/mytextures/brick01".into()];
        h.open_picker(ReplaceField::Replace);
        assert_eq!(h.d.picker(ReplaceField::Replace).category(), Category::Recent);
        assert_eq!(h.d.picker(ReplaceField::Replace).shown_names(), ["tools/toolsnodraw", "dev/dev_measuregeneric01b"]);
        h.d.picker(ReplaceField::Replace).set_category(Category::Favorites);
        h.settle();
        assert_eq!(h.d.picker(ReplaceField::Replace).shown_names(), ["custom/mytextures/brick01"]);
        h.type_text("zzz");
        assert!(h.d.picker(ReplaceField::Replace).shown_names().is_empty());
        h.d.picker(ReplaceField::Replace).set_category(Category::All);
        h.d.picker(ReplaceField::Replace).set_search("tool");
        h.settle();
        assert_eq!(h.d.picker(ReplaceField::Replace).shown_names(), ["tools/toolsnodraw", "tools/toolstrigger"]);
        h.key(egui::Key::Escape);
        h.open_picker(ReplaceField::Replace);
        assert_eq!(h.d.picker(ReplaceField::Replace).category(), Category::All);
        assert_eq!(h.d.picker(ReplaceField::Replace).search(), "");
        h.key(egui::Key::Escape);
        let face = FaceRef::new(2, 0);
        h.state.doc.apply_material(&[face], "custom/gone").expect("apply");
        h.open_picker(ReplaceField::Find);
        assert!(h.d.picker(ReplaceField::Find).shown_names().contains(&"custom/gone".to_owned()));
    }

    #[test]
    fn picker_texts_in_both_languages() {
        let _lang = hp_app::testing::lang_lock();
        for lang in [hp_app::i18n::Lang::Fr, hp_app::i18n::Lang::En] {
            hp_app::i18n::set_lang(lang);
            let mut h = Harness::new();
            h.state.current_material = "tools/toolsnodraw".to_owned();
            h.open_picker(ReplaceField::Find);
            h.type_text("dev");
            h.key(egui::Key::ArrowDown);
            assert_eq!(h.d.picker(ReplaceField::Find).cursor(), Some(0));
        }
    }

    #[test]
    fn dialog_count_is_cached_and_runs() {
        let mut state = hp_app::testing::test_state();
        let mut d = ReplaceDialog::default();
        d.set("tools/toolstrigger", "tools/toolsclip", MatchMode::Exact, false, false);
        assert_eq!(d.match_count(&state.doc).faces.len(), 6);
        assert_eq!(d.run(&mut state), Some(6));
        assert_eq!(d.match_count(&state.doc).faces.len(), 0);
        let c = count_matches(&state.doc, "tools/toolsclip", MatchMode::Exact, false);
        assert_eq!(c.faces.len(), 6);
    }
}
