mod array;
mod goto;
pub(crate) mod instance;
mod mapinfo;
mod paste;
mod pointfile;
mod problems;
mod transform;

use hp_app::AppState;

pub use pointfile::go_to_leak_start;
pub(crate) use pointfile::{leak_entity_index, window_id as pointfile_window_id};

pub(crate) trait MapDialog {
    fn show(&mut self, ctx: &egui::Context, state: &mut AppState);
}

pub struct MapDialogs {
    problems: problems::ProblemsDialog,
    transform: transform::TransformDialog,
    paste: paste::PasteSpecialDialog,
    array: array::ArrayDialog,
    goto: goto::GotoDialog,
    map_info: mapinfo::MapInfoDialog,
    packages: Vec<Box<dyn MapDialog>>,
}

impl Default for MapDialogs {
    fn default() -> Self {
        Self {
            problems: problems::ProblemsDialog::default(),
            transform: transform::TransformDialog::default(),
            paste: paste::PasteSpecialDialog::default(),
            array: array::ArrayDialog::default(),
            goto: goto::GotoDialog::default(),
            map_info: mapinfo::MapInfoDialog::default(),
            packages: crate::package_dialogs(),
        }
    }
}

impl MapDialogs {
    pub fn new() -> Self {
        Self::default()
    }

    pub fn show(&mut self, ctx: &egui::Context, state: &mut AppState) {
        self.problems.show(ctx, state);
        self.transform.show(ctx, state);
        self.paste.show(ctx, state);
        self.array.show(ctx, state);
        self.goto.show(ctx, state);
        self.map_info.show(ctx, state);
        for dialog in &mut self.packages {
            dialog.show(ctx, state);
        }
        pointfile::show(ctx, state);
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use glam::DVec3;
    use hp_app::state::Pointfile;
    use hp_app::testing::{run_ctx, test_state};

    #[test]
    fn smoke_all_dialogs() {
        let _lang = crate::lang_lock();
        crate::register_translations();
        let mut state = test_state();
        let mut d = MapDialogs::new();
        run_ctx(&mut state, |ctx, state| d.show(ctx, state));
        state.ui.show_problems = true;
        state.ui.show_transform = true;
        state.ui.show_paste_special = true;
        state.ui.show_goto = true;
        state.ui.show_map_info = true;
        state.ui.quick_input = "5".into();
        state.pointfile = Some(Pointfile {
            path: "C:/maps/gm_test.lin".into(),
            points: vec![DVec3::ZERO, DVec3::new(64.0, 0.0, 0.0), DVec3::new(64.0, 64.0, 0.0)],
            cursor: 1,
        });
        run_ctx(&mut state, |ctx, state| d.show(ctx, state));
        assert!(state.ui.show_problems && state.ui.show_transform && state.ui.show_paste_special);
        assert!(state.ui.show_goto && state.ui.show_map_info);
        assert_eq!(d.goto.text, "5", "quick input prefills the go to field");
        assert!(state.ui.quick_input.is_empty());
        assert!(d.map_info.stats.is_some());

        for mode in [transform::TransformMode::Move, transform::TransformMode::Rotate, transform::TransformMode::Scale]
        {
            d.transform.mode = mode;
            run_ctx(&mut state, |ctx, state| d.show(ctx, state));
        }
        state.doc.set_selection(&[hp_editor::ObjRef::Solid(2)]);
        state.doc.delete(&[hp_editor::ObjRef::Entity(4)]).ok();
        d.map_info.filter = "dev".into();
        d.goto.text = "0 0 64".into();
        run_ctx(&mut state, |ctx, state| d.show(ctx, state));
        assert_eq!(d.map_info.rows.len(), 1);
        hp_app::i18n::set_lang(hp_app::i18n::Lang::Fr);
        run_ctx(&mut state, |ctx, state| d.show(ctx, state));
        hp_app::i18n::set_lang(hp_app::i18n::Lang::En);
        state.ui = hp_app::state::UiState::default();
        state.pointfile = None;
        run_ctx(&mut state, |ctx, state| d.show(ctx, state));
    }

    #[test]
    fn package_windows_are_shown_every_frame() {
        struct Counting(std::rc::Rc<std::cell::Cell<usize>>);
        impl MapDialog for Counting {
            fn show(&mut self, _ctx: &egui::Context, _state: &mut AppState) {
                self.0.set(self.0.get() + 1);
            }
        }
        let mut state = test_state();
        let mut d = MapDialogs::new();
        let shown = std::rc::Rc::new(std::cell::Cell::new(0));
        d.packages.push(Box::new(Counting(shown.clone())));
        run_ctx(&mut state, |ctx, state| d.show(ctx, state));
        run_ctx(&mut state, |ctx, state| d.show(ctx, state));
        assert!(shown.get() >= 2, "{}", shown.get());
    }

    fn key_event(key: egui::Key) -> egui::Event {
        egui::Event::Key { key, physical_key: None, pressed: true, repeat: false, modifiers: egui::Modifiers::NONE }
    }

    fn dialogs_frame(
        ctx: &egui::Context,
        d: &mut MapDialogs,
        state: &mut hp_app::AppState,
        events: Vec<egui::Event>,
    ) -> bool {
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
    }

    #[test]
    fn escape_closes_the_top_dialog_with_the_pointer_over_a_view() {
        crate::register_translations();
        type Flag = fn(&mut hp_app::AppState) -> &mut bool;
        let flags: [(&str, Flag); 6] = [
            ("transform", |s| &mut s.ui.show_transform),
            ("paste special", |s| &mut s.ui.show_paste_special),
            ("array duplicate", |s| &mut s.ui.show_array_duplicate),
            ("problems", |s| &mut s.ui.show_problems),
            ("map info", |s| &mut s.ui.show_map_info),
            ("go to", |s| &mut s.ui.show_goto),
        ];
        let away = egui::Event::PointerMoved(egui::pos2(1590.0, 950.0));
        for (name, flag) in flags {
            let mut state = test_state();
            let mut d = MapDialogs::new();
            let ctx = egui::Context::default();
            dialogs_frame(&ctx, &mut d, &mut state, vec![away.clone()]);
            *flag(&mut state) = true;
            for _ in 0..3 {
                dialogs_frame(&ctx, &mut d, &mut state, Vec::new());
            }
            let left = dialogs_frame(&ctx, &mut d, &mut state, vec![key_event(egui::Key::Escape)]);
            assert!(!*flag(&mut state), "{name}: Escape closes the dialog");
            assert!(!left, "{name}: Escape is consumed");
            dialogs_frame(&ctx, &mut d, &mut state, Vec::new());
            assert!(dialogs_frame(&ctx, &mut d, &mut state, vec![key_event(egui::Key::Escape)]), "{name}");
        }

        let mut state = test_state();
        let mut d = MapDialogs::new();
        let ctx = egui::Context::default();
        state.pointfile = Some(Pointfile {
            path: "C:/maps/gm_test.lin".into(),
            points: vec![DVec3::ZERO, DVec3::new(64.0, 0.0, 0.0)],
            cursor: 0,
        });
        dialogs_frame(&ctx, &mut d, &mut state, vec![away]);
        state.ui.show_problems = true;
        dialogs_frame(&ctx, &mut d, &mut state, Vec::new());
        dialogs_frame(&ctx, &mut d, &mut state, Vec::new());
        state.ui.show_transform = true;
        for _ in 0..3 {
            dialogs_frame(&ctx, &mut d, &mut state, Vec::new());
        }
        assert!(!dialogs_frame(&ctx, &mut d, &mut state, vec![key_event(egui::Key::Escape)]));
        assert!(!state.ui.show_transform && state.ui.show_problems, "the top dialog closes first");
        dialogs_frame(&ctx, &mut d, &mut state, Vec::new());
        assert!(!dialogs_frame(&ctx, &mut d, &mut state, vec![key_event(egui::Key::Escape)]));
        assert!(!state.ui.show_problems);
        dialogs_frame(&ctx, &mut d, &mut state, Vec::new());
        assert!(dialogs_frame(&ctx, &mut d, &mut state, vec![key_event(egui::Key::Escape)]));
        assert!(state.pointfile.is_some(), "the navigator does not take Escape");
    }

    #[test]
    fn resizable_dialogs_keep_their_size() {
        crate::register_translations();
        let mut state = test_state();
        for i in 0..40 {
            let id = 10_000 + i;
            let text = format!(
                "entity {{ \"id\" \"{id}\" \"classname\" \"unknown_class_{i}\" \"origin\" \"{} 0 0\" }}",
                i * 32
            );
            let _ = state.doc.paste_special(&text, &hp_editor::PasteSpecial::default());
        }
        state.ui.show_problems = true;
        state.ui.show_map_info = true;
        let mut d = MapDialogs::new();
        let ctx = egui::Context::default();
        let ids = [egui::Id::unique("hp_problems_window"), egui::Id::unique("hp_map_info_window")];
        let mut first = Vec::new();
        for frame in 0..120 {
            let input = egui::RawInput {
                screen_rect: Some(egui::Rect::from_min_size(egui::Pos2::ZERO, egui::vec2(1600.0, 960.0))),
                ..Default::default()
            };
            let mut out = ctx.run_ui(input, |ui| d.show(ui.ctx(), &mut state));
            out.textures_delta.clear();
            let rects: Vec<egui::Rect> = ids.iter().filter_map(|id| ctx.memory(|m| m.area_rect(*id))).collect();
            assert_eq!(rects.len(), 2);
            if frame == 5 {
                first = rects.clone();
            }
            if frame > 5 {
                for (a, b) in first.iter().zip(&rects) {
                    assert!((a.height() - b.height()).abs() < 0.5, "frame {frame}: {a:?} -> {b:?}");
                    assert!(b.bottom() <= 960.0);
                }
            }
        }
        assert!(first[0].height() < 520.0 && first[1].height() < 640.0, "{first:?}");
    }
}
