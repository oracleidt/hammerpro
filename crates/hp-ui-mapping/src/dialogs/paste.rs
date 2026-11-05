use egui::{RichText, Ui};
use glam::DVec3;
use hp_app::actions::key_label;
use hp_app::{AppState, Severity, t, tf, theme};
use hp_editor::{Document, EditError, PasteSpecial};

use crate::util::{enter_pressed, escape_pressed, icon_title, plural};

pub(crate) const MAX_COPIES: u32 = 10_000;
const MANY_COPIES: u32 = 1_000;

fn read_clipboard() -> Result<String, String> {
    arboard::Clipboard::new().and_then(|mut c| c.get_text()).map_err(|e| e.to_string())
}

pub(crate) struct PasteSpecialDialog {
    pub(crate) opts: PasteSpecial,
    pub(crate) around_pivot: bool,
    pub(crate) error: Option<String>,
    size_cache: Option<(usize, u64, DVec3)>,
}

impl Default for PasteSpecialDialog {
    fn default() -> Self {
        Self {
            opts: PasteSpecial { offset: DVec3::new(64.0, 0.0, 0.0), ..PasteSpecial::default() },
            around_pivot: true,
            error: None,
            size_cache: None,
        }
    }
}

impl PasteSpecialDialog {
    pub(crate) fn paste_text(&mut self, state: &mut AppState, text: &str) -> Result<usize, String> {
        let mut opts = self.opts.clone();
        opts.copies = opts.copies.clamp(1, MAX_COPIES);
        opts.name_prefix = opts.name_prefix.trim().to_owned();
        opts.pivot = if self.around_pivot { state.pivot.filter(|p| p.is_finite()) } else { None };
        let fgd = state.assets.fgd().filter(|f| !f.is_empty());
        let targets =
            |class: &str, key: &str| fgd.and_then(|f| hp_app::runtime::helpers::fgd_target_key(f, class, key));
        let result = state.doc.paste_special_with(text, &opts, Some(&targets));
        match result {
            Ok(objs) => {
                self.error = None;
                Ok(objs.len())
            }
            Err(EditError::EmptyClipboard) => Err(t!("paste.err_empty").to_owned()),
            Err(e) => Err(tf!("paste.err_failed", e)),
        }
    }

    fn paste(&mut self, state: &mut AppState) -> bool {
        let text = match read_clipboard() {
            Ok(t) => t,
            Err(e) => {
                self.error = Some(tf!("paste.err_clipboard", e));
                return false;
            }
        };
        match self.paste_text(state, &text) {
            Ok(n) => {
                let copies = self.opts.copies.clamp(1, MAX_COPIES) as usize;
                let objects = tf!(plural(n, "paste.objects_one", "paste.objects"), n);
                let copies = tf!(plural(copies, "paste.copies_count_one", "paste.copies_count"), copies);
                state.log(Severity::Info, tf!("paste.done", objects, copies));
                true
            }
            Err(e) => {
                self.error = Some(e);
                false
            }
        }
    }

    pub(crate) fn clipboard_size(&mut self, text: &str) -> Result<DVec3, String> {
        use std::hash::{Hash, Hasher};
        let mut h = std::collections::hash_map::DefaultHasher::new();
        text.hash(&mut h);
        let key = (text.len(), h.finish());
        if let Some((len, hash, size)) = self.size_cache
            && (len, hash) == key
        {
            return Ok(size);
        }
        match Document::clipboard_bounds(text) {
            Ok(b) => {
                let size = b.size();
                self.size_cache = Some((key.0, key.1, size));
                Ok(size)
            }
            Err(EditError::EmptyClipboard) => Err(t!("paste.err_empty").to_owned()),
            Err(e) => Err(tf!("paste.err_failed", e)),
        }
    }

    fn offset_from_clipboard(&mut self, axis: usize) {
        let size = read_clipboard().map_err(|e| tf!("paste.err_clipboard", e)).and_then(|t| self.clipboard_size(&t));
        match size {
            Ok(s) => {
                self.opts.offset[axis] = s[axis];
                self.error = None;
            }
            Err(e) => self.error = Some(e),
        }
    }

    pub(crate) fn show(&mut self, ctx: &egui::Context, state: &mut AppState) {
        if !state.ui.show_paste_special {
            self.error = None;
            return;
        }
        let mut open = true;
        let mut do_paste = false;
        let response = hp_app::chrome::Window::new(icon_title("📋", t!("paste.title")))
            .id(egui::Id::unique("hp_paste_special_window"))
            .default_pos(crate::util::centered_pos(ctx, egui::vec2(400.0, 280.0)))
            .open(&mut open)
            .default_width(400.0)
            .resizable(false)
            .collapsible(false)
            .show(ctx, |ui| do_paste = self.window_ui(ui, state));
        if let Some(r) = &response {
            if escape_pressed(ctx, &r.response) {
                open = false;
            } else if enter_pressed(ctx, &r.response) {
                do_paste = true;
            }
        }
        if do_paste && self.paste(state) {
            open = false;
        }
        if !open {
            state.ui.show_paste_special = false;
        }
    }

    fn window_ui(&mut self, ui: &mut Ui, state: &mut AppState) -> bool {
        let mut from_clipboard = None;
        egui::Grid::new("hp_paste_grid").num_columns(2).spacing([12.0, 6.0]).show(ui, |ui| {
            let o = &mut self.opts;
            ui.label(t!("paste.copies"));
            ui.add(egui::DragValue::new(&mut o.copies).range(1..=MAX_COPIES).speed(0.1));
            ui.end_row();
            ui.label(t!("paste.offset"));
            ui.horizontal(|ui| {
                for (k, (name, color)) in
                    [("X", theme::text_weak()), ("Y", theme::text_weak()), ("Z", theme::text_weak())]
                        .into_iter()
                        .enumerate()
                {
                    ui.label(RichText::new(name).color(color).strong());
                    ui.add(egui::DragValue::new(&mut o.offset[k]).speed(1.0).max_decimals(3));
                    if ui.small_button("<").on_hover_text(t!("paste.size_tip")).clicked() {
                        from_clipboard = Some(k);
                    }
                }
            });
            ui.end_row();
            ui.label(t!("paste.rotation"));
            hp_app::widgets::vec3_edit(ui, &mut o.rotation, 1.0);
            ui.end_row();
            ui.label(t!("paste.name_prefix"));
            ui.add(
                egui::TextEdit::singleline(&mut o.name_prefix)
                    .hint_text(t!("paste.name_prefix_hint"))
                    .desired_width(160.0),
            );
            ui.end_row();
        });
        if let Some(k) = from_clipboard {
            self.offset_from_clipboard(k);
        }
        let o = &mut self.opts;
        ui.add_space(2.0);
        ui.checkbox(&mut o.start_at_original, t!("paste.start_at_original"))
            .on_hover_text(t!("paste.start_at_original_tip"));
        ui.checkbox(&mut o.group, t!("paste.group"));
        ui.checkbox(&mut o.unique_names, t!("paste.unique_names")).on_hover_text(t!("paste.unique_names_tip"));
        let pivot = state.pivot.filter(|p| p.is_finite());
        ui.add_enabled_ui(pivot.is_some(), |ui| {
            let tip = match pivot {
                Some(p) => tf!("paste.around_pivot_tip", hp_app::widgets::fmt_vec3(p)),
                None => t!("paste.around_pivot_off").to_owned(),
            };
            ui.checkbox(&mut self.around_pivot, t!("paste.around_pivot"))
                .on_hover_text(tip)
                .on_disabled_hover_text(t!("paste.around_pivot_off"));
        });
        let o = &self.opts;
        let copies = o.copies.clamp(1, MAX_COPIES);
        let total = o.offset * f64::from(copies);
        ui.label(RichText::new(tf!("paste.total_offset", hp_app::widgets::fmt_vec3(total))).weak());
        if copies > MANY_COPIES {
            ui.label(RichText::new(tf!("paste.many_copies", copies)).color(theme::warning()));
        }
        if let Some(e) = &self.error {
            ui.label(RichText::new(e).color(theme::error()));
        }
        ui.separator();
        let mut paste = false;
        ui.horizontal(|ui| {
            ui.with_layout(egui::Layout::right_to_left(egui::Align::Center), |ui| {
                if ui.button(t!("common.cancel")).on_hover_text(key_label(egui::Key::Escape)).clicked() {
                    state.ui.show_paste_special = false;
                }
                if ui.button(t!("paste.paste")).on_hover_text(key_label(egui::Key::Enter)).clicked() {
                    paste = true;
                }
            });
        });
        paste
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use hp_editor::ObjRef;

    #[test]
    fn paste_copies_with_offset() {
        crate::register_translations();
        let mut state = hp_app::testing::test_state();
        let text = state.doc.copy(&[ObjRef::Entity(5)]);
        let mut d = PasteSpecialDialog::default();
        d.opts.copies = 3;
        d.opts.offset = DVec3::new(0.0, 0.0, 32.0);
        d.opts.group = false;
        d.opts.unique_names = true;
        let before = state.doc.scene().entities().len();
        let n = d.paste_text(&mut state, &text).expect("paste");
        assert_eq!(n, 3);
        assert_eq!(state.doc.scene().entities().len(), before + 3);
        let zs: Vec<f64> = state
            .doc
            .selected_entity_ids()
            .iter()
            .filter_map(|id| state.doc.scene().entity(*id).and_then(|n| n.entity.origin()).map(|o| o.z))
            .collect();
        assert_eq!(zs, vec![128.0, 160.0, 192.0]);
        assert!(state.doc.undo());
        assert_eq!(state.doc.scene().entities().len(), before);
    }

    #[test]
    fn paste_errors() {
        crate::register_translations();
        let mut state = hp_app::testing::test_state();
        let mut d = PasteSpecialDialog::default();
        let err = d.paste_text(&mut state, "not a map").expect_err("error");
        assert!(!err.is_empty());
        assert!(d.clipboard_size("not a map").is_err());
    }

    #[test]
    fn clipboard_size_and_pivot() {
        crate::register_translations();
        let mut state = hp_app::testing::test_state();
        let text = state.doc.copy(&[ObjRef::Solid(3)]);
        let mut d = PasteSpecialDialog::default();
        assert_eq!(d.clipboard_size(&text).expect("size"), DVec3::new(512.0, 16.0, 128.0));
        assert_eq!(d.clipboard_size(&text).expect("cached"), DVec3::new(512.0, 16.0, 128.0));
        state.pivot = Some(DVec3::new(1000.0, 264.0, 64.0));
        d.opts = PasteSpecial { copies: 1, rotation: DVec3::new(0.0, 0.0, 90.0), ..PasteSpecial::default() };
        d.paste_text(&mut state, &text).expect("paste");
        let sel = state.doc.selected_objects();
        let b = state.doc.object_bounds(sel[0], &|_| None).expect("bounds");
        assert!((b.center() - DVec3::new(1000.0, -736.0, 64.0)).length() < 1e-6, "{b:?}");
        d.around_pivot = false;
        d.paste_text(&mut state, &text).expect("paste");
        let sel = state.doc.selected_objects();
        let b = state.doc.object_bounds(sel[0], &|_| None).expect("bounds");
        assert!((b.center() - DVec3::new(0.0, 264.0, 64.0)).length() < 1e-6, "{b:?}");
        const { assert!(MAX_COPIES >= 10_000) };
    }

    #[test]
    fn renames_follow_parent_and_target_keys() {
        crate::register_translations();
        let mut state = hp_app::testing::test_state();
        let text = r#"entity { "id" "1" "classname" "func_door" "targetname" "door" "origin" "0 0 0" }
entity { "id" "2" "classname" "prop_dynamic" "parentname" "door,handle" "origin" "8 0 0" }
entity { "id" "3" "classname" "info_target" "target" "door" "origin" "0 8 0" }
"#;
        let mut d = PasteSpecialDialog {
            opts: PasteSpecial { copies: 1, name_prefix: "a_".into(), ..PasteSpecial::default() },
            ..PasteSpecialDialog::default()
        };
        d.paste_text(&mut state, text).expect("paste");
        let get = |class: &str, key: &str| {
            state
                .doc
                .scene()
                .entities()
                .values()
                .find(|n| n.entity.classname == class && n.entity.get(key).is_some())
                .and_then(|n| n.entity.get(key).map(str::to_owned))
        };
        assert_eq!(get("func_door", "targetname").as_deref(), Some("a_door"));
        assert_eq!(get("prop_dynamic", "parentname").as_deref(), Some("a_door,handle"));
        assert_eq!(get("info_target", "target").as_deref(), Some("a_door"));
    }
}
