use std::sync::Arc;

use egui::{Event, Modifiers};
use hp_app::i18n::Lang;
use hp_app::settings::{GridMode, GridSpec};
use hp_app::{Action, AppState};

use crate::Shell;
use crate::tests::lang_guard;

struct Bars {
    ctx: egui::Context,
    texts: Vec<(String, egui::Rect)>,
}

impl Bars {
    fn new() -> Self {
        let ctx = egui::Context::default();
        ctx.enable_accesskit();
        Self { ctx, texts: Vec::new() }
    }

    fn frame(&mut self, state: &mut AppState, shell: &mut Shell, events: Vec<Event>) {
        let input = egui::RawInput {
            screen_rect: Some(egui::Rect::from_min_size(egui::Pos2::ZERO, egui::vec2(1900.0, 1400.0))),
            events,
            ..Default::default()
        };
        let mut out = self.ctx.run_ui(input, |ui| {
            egui::Panel::top("top").show(ui, |ui| shell.toolbar(ui, state));
            egui::Panel::bottom("bottom").show(ui, |ui| shell.status_bar(ui, state, Some("hint".into())));
            let ctx = ui.ctx().clone();
            shell.dialogs(&ctx, state);
        });
        out.textures_delta.clear();
        self.texts.clear();
        if let Some(update) = out.platform_output.accesskit_update {
            for (_, node) in update.nodes {
                let text = [node.label(), node.value()].into_iter().flatten().collect::<Vec<_>>().join(" ");
                if let Some(b) = node.bounds()
                    && !text.trim().is_empty()
                {
                    let r = egui::Rect::from_min_max(
                        egui::pos2(b.x0 as f32, b.y0 as f32),
                        egui::pos2(b.x1 as f32, b.y1 as f32),
                    );
                    let text = text.trim().to_owned();
                    let same = |(t, q): &(String, egui::Rect)| *t == text && q.center().distance(r.center()) < 2.0;
                    if !self.texts.iter().any(same) {
                        self.texts.push((text, r));
                    }
                }
            }
        }
    }

    fn frames(&mut self, state: &mut AppState, shell: &mut Shell) {
        for _ in 0..3 {
            self.frame(state, shell, vec![]);
        }
    }

    fn count(&self, f: impl Fn(&str) -> bool) -> usize {
        self.texts.iter().filter(|(t, _)| f(t)).count()
    }

    fn find(&self, text: &str) -> Option<egui::Rect> {
        let by = |f: &dyn Fn(&str) -> bool| self.texts.iter().find(|(t, _)| f(t)).map(|(_, r)| *r);
        by(&|t| t == text).or_else(|| by(&|t| t.ends_with(&format!(" {text}"))))
    }

    fn click(&mut self, state: &mut AppState, shell: &mut Shell, text: &str) {
        let rect = self
            .find(text)
            .unwrap_or_else(|| panic!("no {text:?} in {:?}", self.texts.iter().map(|(t, _)| t).collect::<Vec<_>>()));
        let pos = rect.center();
        let button = egui::PointerButton::Primary;
        self.frame(state, shell, vec![Event::PointerMoved(pos)]);
        self.frame(state, shell, vec![Event::PointerButton { pos, button, pressed: true, modifiers: Modifiers::NONE }]);
        self.frame(
            state,
            shell,
            vec![Event::PointerButton { pos, button, pressed: false, modifiers: Modifiers::NONE }],
        );
        self.frame(state, shell, vec![]);
    }
}

fn state() -> AppState {
    let s = hp_app::testing::test_state();
    crate::register_translations();
    s
}

#[test]
fn grid_snap_and_texture_lock_are_shown_once() {
    let _g = lang_guard();
    for lang in [Lang::En, Lang::Fr] {
        hp_app::i18n::set_lang(lang);
        let mut state = state();
        let mut shell = Shell::new();
        let mut bars = Bars::new();
        bars.frames(&mut state, &mut shell);
        let grid_status = hp_app::tf!("status.grid", "16");
        let snap = hp_app::t!("status.snap_on");
        let texlock = hp_app::t!("status.texlock_on");
        assert_eq!(bars.count(|t| t == "16"), 1, "{lang:?}: the grid size field: {:?}", bars.texts);
        assert_eq!(bars.count(|t| t == grid_status), 1, "{lang:?}: status bar grid like Hammer");
        state.settings.workflow.show_status_grid = false;
        bars.frames(&mut state, &mut shell);
        assert_eq!(bars.count(|t| t == grid_status), 0, "{lang:?}: hidden by the setting");
        assert_eq!(bars.count(|t| t == snap), 0, "{lang:?}: snap pill");
        assert_eq!(bars.count(|t| t == texlock), 0, "{lang:?}: texture lock pill");
        state.settings.workflow.show_toolbar = false;
        bars.frames(&mut state, &mut shell);
        assert_eq!(bars.count(|t| t == grid_status), 1, "{lang:?}: {:?}", bars.texts);
        assert_eq!(bars.count(|t| t == snap), 1, "{lang:?}");
        assert_eq!(bars.count(|t| t == texlock), 1, "{lang:?}");
        assert_eq!(bars.count(|t| t == "16"), 0, "{lang:?}: no toolbar field");
        bars.click(&mut state, &mut shell, snap);
        assert!(state.requested_actions.contains(&Action::ToggleSnap));
    }
    hp_app::i18n::set_lang(Lang::En);
}

#[test]
fn the_grid_button_names_the_grid_it_acts_on() {
    let _g = lang_guard();
    hp_app::i18n::set_lang(Lang::En);
    let mut state = state();
    let mut shell = Shell::new();
    let mut bars = Bars::new();
    bars.frames(&mut state, &mut shell);
    assert!(bars.find("Grid").is_some(), "{:?}", bars.texts);
    state.settings.edit.grid_size = 8.0;
    state.set_grid_mode(GridMode::PerView);
    state.settings.grid.per_view.front.size = 12.0;
    state.ui.focused_view = Some(2);
    bars.frames(&mut state, &mut shell);
    assert!(bars.find("Grid (Front)").is_some(), "{:?}", bars.texts);
    assert!(bars.find("12").is_some(), "the field shows the Front grid: {:?}", bars.texts);
    crate::toolbar::set_grid_size(&mut state, 24.0);
    assert_eq!(state.settings.grid.per_view.front.size, 24.0);
    assert_eq!(state.settings.grid.per_view.top.size, 8.0);
    state
        .put_custom_grid(
            "",
            GridSpec { name: "Stairs".into(), step: [12.0, 12.0, 8.0], ..GridSpec::default() },
            true,
            false,
        )
        .unwrap();
    state.use_custom_grid("Stairs").unwrap();
    bars.frames(&mut state, &mut shell);
    assert!(bars.find("Stairs").is_some(), "{:?}", bars.texts);
    bars.click(&mut state, &mut shell, "Stairs");
    assert!(bars.find("Stairs  (12 × 12 × 8)").is_some(), "the menu lists the grid: {:?}", bars.texts);
    bars.click(&mut state, &mut shell, "Global grid");
    assert!(state.requested_actions.contains(&Action::GridModeGlobal));
    crate::grids::run_grid_action(&mut state, Action::GridModeGlobal);
    assert_eq!(state.settings.grid.mode, GridMode::Global);
}

#[test]
fn the_leak_badge_is_shown_once() {
    let _g = lang_guard();
    hp_app::i18n::set_lang(Lang::En);
    let mut state = state();
    let mut shell = Shell::new();
    let mut bars = Bars::new();
    state.runtime.compile.badge = Some(hp_app::runtime::CompileBadge::Leak);
    bars.frames(&mut state, &mut shell);
    let leak = |t: &str| t.ends_with(hp_app::t!("shell.status.compile_leak"));
    assert_eq!(bars.count(leak), 1, "the compile leak alone: {:?}", bars.texts);
    let report = hp_editor::limits::leak::LeakReport {
        leak: Some(hp_editor::limits::leak::LeakPath { entity: 1, points: vec![glam::DVec3::ZERO, glam::DVec3::X] }),
        ..Default::default()
    };
    state.runtime.health.leak =
        Some(Arc::new(hp_app::runtime::LeakResult { serial: 0, report, problems: Vec::new(), ms: 0 }));
    state.runtime.health.checks = None;
    state.runtime.health.checking = true;
    bars.frames(&mut state, &mut shell);
    assert_eq!(bars.count(leak), 1, "{:?}", bars.texts);
}

#[test]
fn the_grid_manager_edits_custom_grids() {
    let _g = lang_guard();
    hp_app::i18n::set_lang(Lang::En);
    let mut state = state();
    let mut shell = Shell::new();
    let mut bars = Bars::new();
    state.ui.show_grid_manager = true;
    bars.frames(&mut state, &mut shell);
    assert!(bars.find("Grids").is_some(), "{:?}", bars.texts);
    bars.click(&mut state, &mut shell, "New grid");
    let grids = state.map_grids();
    assert_eq!(grids.len(), 1);
    assert_eq!(grids[0].name, "Grid");
    assert_eq!(state.doc.undo_label(), Some("Edit grid"));
    bars.click(&mut state, &mut shell, "Use");
    assert_eq!(state.settings.grid.mode, GridMode::Custom);
    bars.click(&mut state, &mut shell, "Save as preset");
    assert_eq!(state.settings.grid.presets.len(), 1);
    bars.click(&mut state, &mut shell, "Delete");
    assert_eq!(state.settings.grid.presets.len(), 0, "the preset was the one edited");
    assert_eq!(state.map_grids().len(), 1);
}

#[test]
fn status_grid_sits_at_the_right_end_and_survives_a_narrow_bar() {
    let _g = lang_guard();
    hp_app::i18n::set_lang(Lang::En);
    let mut state = state();
    let mut shell = Shell::new();
    let mut bars = Bars::new();
    bars.frames(&mut state, &mut shell);
    let grid = hp_app::tf!("status.grid", "16");
    let r = bars.find(&grid).expect("status grid");
    let rightmost = bars.texts.iter().filter(|(_, q)| q.center().y > 1000.0).map(|(_, q)| q.max.x).fold(0.0, f32::max);
    assert!((r.max.x - rightmost).abs() < 0.5, "grid is the last item: {r:?} vs {rightmost}");
    bars.click(&mut state, &mut shell, &grid);
    bars.frames(&mut state, &mut shell);
    assert!(bars.count(|t| t == "64") >= 1, "the click opens the grid size menu: {:?}", bars.texts);
    assert_eq!(crate::status::wheel_steps(0.25, 1.0), 0);
    assert_eq!(crate::status::wheel_steps(1.0, 1.0), 1);
    assert_eq!(crate::status::wheel_steps(-3.0, 1.0), -1);
}
