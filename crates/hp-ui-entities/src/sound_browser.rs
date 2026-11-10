use std::sync::Arc;

use egui::{Key, Modifiers, RichText, ScrollArea, Ui};
use hp_app::runtime::{AssetKind, AssetPick};
use hp_app::source_filter::SourceFilter;
use hp_app::{AppState, Severity, t, tf, theme};
use hp_fs::AssetEntry;

use crate::browse::{BannerAction, Sources, pick_banner, source_chips};
use crate::edit::{Op, apply};
use crate::names::WordSearch;
use crate::sounds::{self, GameSound};
use crate::values::sound_key_name;
use crate::widgets::{ROW_HEIGHT, RowLook, play_row, row_scroll_offset, search_box};

pub const TAB_ID: &str = "sounds";

#[derive(Clone, Copy, Debug, Default, PartialEq, Eq, Hash)]
pub enum SoundMode {
    #[default]
    GameSounds,
    Files,
}

type RowsKey = (SoundMode, SourceFilter, bool, usize, usize, usize, usize);

pub struct SoundBrowser {
    mode: SoundMode,
    search: String,
    source: SourceFilter,
    sources: Sources,
    game: Option<Arc<Vec<GameSound>>>,
    files: Arc<Vec<AssetEntry>>,
    rows: Vec<u32>,
    rows_key: Option<RowsKey>,
    rows_search: String,
    detail: String,
    selected: Option<String>,
    scroll_to_selected: bool,
    list_view: (f32, f32),
    script: Option<(String, String)>,
    last_pick: Option<(String, Vec<hp_editor::Id>)>,
    uses: crate::browse::UsesCache,
}

impl Default for SoundBrowser {
    fn default() -> Self {
        Self::new()
    }
}

impl SoundBrowser {
    pub fn new() -> Self {
        Self {
            mode: SoundMode::GameSounds,
            search: String::new(),
            source: SourceFilter::All,
            sources: Sources::default(),
            game: None,
            files: Arc::new(Vec::new()),
            rows: Vec::new(),
            rows_key: None,
            rows_search: String::new(),
            detail: String::new(),
            selected: None,
            scroll_to_selected: false,
            list_view: (0.0, 0.0),
            script: None,
            last_pick: None,
            uses: crate::browse::UsesCache::default(),
        }
    }

    pub fn set_search(&mut self, text: &str) {
        text.clone_into(&mut self.search);
    }

    pub fn set_mode(&mut self, mode: SoundMode) {
        self.mode = mode;
    }

    pub fn selected(&self) -> Option<&str> {
        self.selected.as_deref()
    }

    pub fn select(&mut self, value: &str) {
        self.selected = Some(value.to_owned());
        self.scroll_to_selected = true;
    }

    pub fn row_count(&self) -> usize {
        self.rows.len()
    }

    fn row(&self, i: usize) -> Option<(&str, &str, Option<u32>)> {
        let index = *self.rows.get(i)? as usize;
        match self.mode {
            SoundMode::GameSounds => {
                let g = self.game.as_ref()?.get(index)?;
                Some((&g.name, &g.wave, g.source))
            }
            SoundMode::Files => {
                let f = self.files.get(index)?;
                Some((sound_key_name(&f.name), "", Some(f.source)))
            }
        }
    }

    fn refresh(&mut self, ctx: &egui::Context, state: &mut AppState) {
        self.sources.refresh(state.assets.as_ref());
        self.files = state.assets.sounds();
        if self.mode == SoundMode::GameSounds || self.game.is_none() {
            self.game = sounds::game_sounds(ctx, state.assets.as_ref());
        }
        let gmod_only = state.settings.browser.gmod_only;
        let game_len = self.game.as_ref().map_or(0, |g| g.len());
        let game_ptr = self.game.as_ref().map_or(0, |g| Arc::as_ptr(g) as usize);
        let key: RowsKey = (
            self.mode,
            self.source,
            gmod_only,
            game_ptr,
            game_len,
            Arc::as_ptr(&self.files) as usize,
            self.files.len(),
        );
        if self.rows_key.as_ref() == Some(&key) && self.rows_search == self.search {
            return;
        }
        let refine = self.rows_key.as_ref() == Some(&key)
            && !self.search.contains('!')
            && !self.rows_search.contains('!')
            && self.search.starts_with(self.rows_search.as_str());
        self.rows_search.clone_from(&self.search);
        if refine {
            let search = WordSearch::parse(&self.search);
            let mut rows = std::mem::take(&mut self.rows);
            rows.retain(|&i| match self.mode {
                SoundMode::GameSounds => self
                    .game
                    .as_ref()
                    .and_then(|g| g.get(i as usize))
                    .is_some_and(|g| search.matches2(&g.name, &g.wave)),
                SoundMode::Files => self.files.get(i as usize).is_some_and(|f| search.matches(sound_key_name(&f.name))),
            });
            self.rows = rows;
            return;
        }
        let search = WordSearch::parse(&self.search);
        let sources = &self.sources;
        let source = self.source;
        let pass = |s: Option<u32>| sources.passes(source, gmod_only, s);
        self.rows = match self.mode {
            SoundMode::GameSounds => self
                .game
                .as_deref()
                .map(|list| {
                    list.iter()
                        .enumerate()
                        .filter(|(_, g)| pass(g.source) && search.matches2(&g.name, &g.wave))
                        .map(|(i, _)| i as u32)
                        .collect()
                })
                .unwrap_or_default(),
            SoundMode::Files => self
                .files
                .iter()
                .enumerate()
                .filter(|(_, f)| pass(Some(f.source)) && search.matches(sound_key_name(&f.name)))
                .map(|(i, _)| i as u32)
                .collect(),
        };
        self.rows_key = Some(key);
    }

    pub fn ui(&mut self, ui: &mut Ui, state: &mut AppState) {
        let ctx = ui.ctx().clone();
        self.refresh(&ctx, state);
        let pick =
            state.runtime.entities.asset_pick.clone().filter(|p| p.result.is_none() && p.kind == AssetKind::Sound);
        self.follow_pick(pick.as_ref());

        ui.horizontal(|ui| {
            for (mode, label) in
                [(SoundMode::GameSounds, t!("entities.sb_game_sounds")), (SoundMode::Files, t!("entities.sb_files"))]
            {
                if ui.add(egui::Button::selectable(self.mode == mode, label)).clicked() {
                    self.mode = mode;
                }
            }
            ui.with_layout(egui::Layout::right_to_left(egui::Align::Center), |ui| {
                let windowed = state.runtime.entities.sound_window;
                let (glyph, tip) =
                    if windowed { ("⊟", t!("entities.dock_back")) } else { ("🗗", t!("entities.pop_out")) };
                if ui.small_button(glyph).on_hover_text(tip).clicked() {
                    state.runtime.entities.sound_window = !windowed;
                    if windowed {
                        state.settings.browser.show_sound_browser = true;
                        state.settings_dirty = true;
                        state.ui.right_tab = hp_app::state::RightTab::Package(TAB_ID);
                    }
                }
                if ui.small_button("⟳").on_hover_text(t!("entities.sb_reload")).clicked() {
                    sounds::reload_game_sounds(&ctx);
                    self.game = None;
                    self.rows_key = None;
                }
            });
        });
        if let Some(p) = &pick {
            match pick_banner(ui, &p.label, self.selected.is_some()) {
                Some(BannerAction::Use) => self.choose(state),
                Some(BannerAction::Cancel) => {
                    state.runtime.entities.asset_pick = None;
                    state.ui.right_tab = hp_app::state::RightTab::Entity;
                }
                None => {}
            }
        }
        if search_box(ui, &mut self.search, t!("entities.sb_search")) {
            self.scroll_to_selected = false;
        }
        let mut gmod_only = state.settings.browser.gmod_only;
        if source_chips(ui, &self.sources, &mut self.source, &mut gmod_only)
            && gmod_only != state.settings.browser.gmod_only
        {
            state.settings.browser.gmod_only = gmod_only;
            state.settings_dirty = true;
        }
        self.refresh(&ctx, state);
        let loading = self.mode == SoundMode::GameSounds && self.game.is_none() && state.assets.fs().is_some();
        ui.horizontal(|ui| {
            if loading {
                ui.spinner();
                ui.label(RichText::new(t!("entities.sb_loading")).weak().small());
            } else {
                ui.label(RichText::new(tf!("entities.sb_count", self.rows.len())).weak().small());
            }
        });

        let list_h = (ui.available_height() - 4.0 * ui.spacing().interact_size.y - 24.0).max(ROW_HEIGHT * 5.0);
        let mut area =
            ScrollArea::vertical().id_salt("hp_ui_entities.sound_list").max_height(list_h).auto_shrink([false, false]);
        if std::mem::take(&mut self.scroll_to_selected)
            && let Some(sel) = self.selected.clone()
            && let Some(row) =
                (0..self.rows.len()).find(|&i| self.row(i).is_some_and(|r| r.0.eq_ignore_ascii_case(&sel)))
        {
            let (offset, view_h) = self.list_view;
            let view_h = if view_h > 0.0 { view_h } else { list_h };
            if let Some(y) = row_scroll_offset(row, ROW_HEIGHT + ui.spacing().item_spacing.y, offset, view_h, true) {
                area = area.vertical_scroll_offset(y);
            }
        }
        let mut clicked: Option<String> = None;
        let mut activated: Option<String> = None;
        let mut preview: Option<String> = None;
        let playing = sounds::now_playing();
        let mut detail = std::mem::take(&mut self.detail);
        let out = area.show_rows(ui, ROW_HEIGHT, self.rows.len(), |ui, range| {
            for i in range {
                let Some((name, wave, source)) = self.row(i) else { continue };
                let badge = self.sources.badge_str(source);
                detail.clear();
                if !wave.is_empty() {
                    detail.push_str(wave);
                    detail.push_str("  ·  ");
                }
                detail.push_str(badge);
                let selected = self.selected.as_deref().is_some_and(|s| s.eq_ignore_ascii_case(name));
                let is_playing = playing.as_deref().is_some_and(|p| p.eq_ignore_ascii_case(name));
                let look = RowLook { main: name, detail: &detail, selected, strong: is_playing, ..RowLook::default() };
                let (play, row) = play_row(ui, ui.scope_id().with(("sound_row", i)), look);
                if play.on_hover_text(t!("entities.play_sound")).clicked() {
                    preview = Some(name.to_owned());
                }
                let row = row.on_hover_text(self.sources.label(source));
                if row.clicked() {
                    clicked = Some(name.to_owned());
                }
                if row.double_clicked() {
                    activated = Some(name.to_owned());
                }
            }
        });
        self.detail = detail;
        self.list_view = (out.state.clamped_offset().y, out.inner_rect.height());
        let hovered = ui.rect_contains_pointer(out.inner_rect);
        if self.rows.is_empty() && !loading {
            ui.label(RichText::new(t!("entities.no_match")).weak());
        }
        let game = self.game.clone();
        let game: &[GameSound] = game.as_deref().map_or(&[], Vec::as_slice);
        let volume = state.settings.browser.volume();
        if let Some(name) = clicked {
            self.selected = Some(name.clone());
            if state.settings.browser.sound_autoplay {
                sounds::play(state.assets.as_ref(), &name, game, volume);
            }
        }
        if let Some(name) = preview {
            self.selected = Some(name.clone());
            if !sounds::play(state.assets.as_ref(), &name, game, volume) {
                state.notify(Severity::Warning, tf!("entities.play_failed", name));
            }
        }
        if hovered && !ctx.text_edit_focused() && self.keyboard(ui, state, game) {
            activated = self.selected.clone();
        }
        if let Some(name) = activated {
            self.selected = Some(name);
            if pick.is_some() {
                self.choose(state);
            } else if let Some(sel) = self.selected.clone() {
                sounds::play(state.assets.as_ref(), &sel, game, volume);
            }
        }
        ui.separator();
        self.footer(ui, state, game, pick.as_ref());
        self.script_window(&ctx);
        if playing.is_some() {
            ctx.request_repaint_after(std::time::Duration::from_millis(200));
        }
    }

    fn follow_pick(&mut self, pick: Option<&AssetPick>) {
        let key = pick.map(|p| (p.key.clone(), p.entities.clone()));
        if key == self.last_pick {
            return;
        }
        self.last_pick = key;
        if let Some(p) = pick
            && !p.current.trim().is_empty()
        {
            let current = p.current.trim().to_owned();
            self.mode = if sounds::is_sound_file(&current) { SoundMode::Files } else { SoundMode::GameSounds };
            self.select(&current);
        }
    }

    fn keyboard(&mut self, ui: &Ui, state: &AppState, game: &[GameSound]) -> bool {
        let (down, up, enter) = ui.input_mut(|i| {
            (
                i.consume_key(Modifiers::NONE, Key::ArrowDown),
                i.consume_key(Modifiers::NONE, Key::ArrowUp),
                i.consume_key(Modifiers::NONE, Key::Enter),
            )
        });
        if enter && self.selected.is_some() {
            return true;
        }
        if !down && !up || self.rows.is_empty() {
            return false;
        }
        let current = self
            .selected
            .as_deref()
            .and_then(|s| (0..self.rows.len()).find(|&i| self.row(i).is_some_and(|r| r.0.eq_ignore_ascii_case(s))));
        let next = match (current, down) {
            (None, _) => 0,
            (Some(c), true) => (c + 1).min(self.rows.len() - 1),
            (Some(c), false) => c.saturating_sub(1),
        };
        if let Some((name, _, _)) = self.row(next) {
            let name = name.to_owned();
            if state.settings.browser.sound_autoplay {
                sounds::play(state.assets.as_ref(), &name, game, state.settings.browser.volume());
            }
            self.select(&name);
        }
        false
    }

    fn choose(&mut self, state: &mut AppState) {
        let Some(sel) = self.selected.clone() else { return };
        if let Some(p) = state.runtime.entities.asset_pick.as_mut()
            && p.kind == AssetKind::Sound
        {
            p.result = Some(sel);
        }
    }

    fn footer(&mut self, ui: &mut Ui, state: &mut AppState, game: &[GameSound], pick: Option<&AssetPick>) {
        let sel = self.selected.clone();
        ui.horizontal(|ui| {
            let text = sel.clone().unwrap_or_else(|| t!("entities.sb_select_hint").to_owned());
            ui.add(egui::Label::new(RichText::new(text).strong()).truncate());
        });
        ui.horizontal(|ui| {
            let playing = sel.as_deref().is_some_and(sounds::is_playing);
            let label = if playing {
                format!("■ {}", t!("entities.stop_sound"))
            } else {
                format!("▶ {}", t!("entities.play_sound"))
            };
            if ui.add_enabled(sel.is_some(), egui::Button::new(label)).clicked()
                && let Some(s) = &sel
            {
                if playing {
                    sounds::stop();
                } else if !sounds::play(state.assets.as_ref(), s, game, state.settings.browser.volume()) {
                    state.notify(Severity::Warning, tf!("entities.play_failed", s));
                }
            }
            let mut volume = state.settings.browser.volume() * 100.0;
            let slider = egui::Slider::new(&mut volume, 0.0..=100.0).suffix("%").show_value(true).max_decimals(0);
            if ui.add(slider).on_hover_text(t!("entities.sb_volume")).changed() {
                state.settings.browser.sound_volume = (volume / 100.0).clamp(0.0, 1.0);
                state.settings_dirty = true;
                sounds::set_volume(state.settings.browser.volume());
            }
        });
        ui.horizontal_wrapped(|ui| {
            if ui.checkbox(&mut state.settings.browser.sound_autoplay, t!("entities.sb_autoplay")).changed() {
                state.settings_dirty = true;
            }
            let game_sound = sel.as_deref().and_then(|s| game.iter().find(|g| g.name.eq_ignore_ascii_case(s)));
            let script = ui.add_enabled(game_sound.is_some(), egui::Button::new(t!("entities.sb_soundscript")));
            if script.on_hover_text(game_sound.map_or("", |g| g.script.as_str())).clicked()
                && let Some(g) = game_sound
            {
                let text = state
                    .assets
                    .fs()
                    .and_then(|fs| fs.read(&g.script))
                    .and_then(|b| sounds::soundscript_block(&b, &g.name))
                    .unwrap_or_else(|| tf!("entities.sb_script_missing", g.script));
                self.script = Some((g.name.clone(), format!("// {}\n{text}", g.script)));
            }
            if ui.add_enabled(sel.is_some(), egui::Button::new(t!("entities.sb_copy"))).clicked()
                && let Some(s) = &sel
            {
                ui.ctx().copy_text(s.clone());
            }
            if let (Some(p), Some(s)) = (pick, &sel)
                && !p.current.trim().is_empty()
                && !p.current.trim().eq_ignore_ascii_case(s)
            {
                let n = self.uses.count(&state.doc, &p.key, &p.current);
                let btn = ui.button(tf!("entities.replace_uses", n)).on_hover_text(tf!(
                    "entities.replace_uses_tip",
                    p.current.trim(),
                    s
                ));
                if btn.clicked() {
                    let op = Op::ReplaceAllUses { key: p.key.clone(), old: p.current.clone(), new: s.clone() };
                    match apply(&mut state.doc, None, op) {
                        Ok(_) => {
                            state.notify(Severity::Success, tf!("entities.replaced_uses", n));
                            state.runtime.entities.asset_pick = None;
                            state.ui.right_tab = hp_app::state::RightTab::Entity;
                        }
                        Err(e) => state.notify(Severity::Error, tf!("entities.edit_failed", e)),
                    }
                }
            }
        });
        if state.assets.fs().is_none() {
            ui.label(RichText::new(t!("entities.sb_no_content")).color(theme::warning()).small());
        } else if !cfg!(windows) {
            ui.label(RichText::new(t!("entities.play_unavailable")).weak().small());
        }
    }

    fn script_window(&mut self, ctx: &egui::Context) {
        let Some((name, text)) = &self.script else { return };
        let mut open = true;
        hp_app::chrome::Window::new(tf!("entities.sb_script_title", name))
            .id(egui::Id::unique("hp_ui_entities.soundscript_window"))
            .open(&mut open)
            .default_size([420.0, 300.0])
            .resizable(true)
            .collapsible(false)
            .show(ctx, |ui| {
                ScrollArea::both().show(ui, |ui| {
                    ui.add(egui::Label::new(RichText::new(text.as_str()).monospace()).selectable(true));
                });
            });
        if !open {
            self.script = None;
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn files_filter_with_negation_and_pick() {
        crate::register_translations();
        let mut state = hp_app::testing::test_state();
        let mut b = SoundBrowser::new();
        b.set_mode(SoundMode::Files);
        hp_app::testing::run_ui(&mut state, |ui, state| b.ui(ui, state));
        assert_eq!(b.row_count(), 1, "the mock content has one sound file");
        b.set_search("wind !gust");
        hp_app::testing::run_ui(&mut state, |ui, state| b.ui(ui, state));
        assert_eq!(b.row_count(), 1);
        b.set_search("wind !wind1");
        hp_app::testing::run_ui(&mut state, |ui, state| b.ui(ui, state));
        assert_eq!(b.row_count(), 0);
        b.set_search("");
        state.settings.browser.gmod_only = true;
        hp_app::testing::run_ui(&mut state, |ui, state| b.ui(ui, state));
        assert_eq!(b.row_count(), 0);
        state.settings.browser.gmod_only = false;

        let target = crate::view::Target::Entities(vec![5]);
        crate::pick::start_asset_pick(
            &mut state.runtime.entities.asset_pick,
            AssetKind::Sound,
            &target,
            "message",
            "light (message)".into(),
            "ambient/wind/wind1.wav",
        );
        hp_app::testing::run_ui(&mut state, |ui, state| b.ui(ui, state));
        assert_eq!(b.selected(), Some("ambient/wind/wind1.wav"));
        b.choose(&mut state);
        assert!(crate::pick::finish_asset_pick(&mut state));
        assert_eq!(state.doc.scene().entity(5).unwrap().entity.get("message"), Some("ambient/wind/wind1.wav"));
    }

    #[test]
    fn game_sounds_mode_without_content_draws() {
        crate::register_translations();
        let mut state = hp_app::testing::test_state();
        let mut b = SoundBrowser::new();
        state.runtime.entities.sound_window = true;
        hp_app::testing::run_ui(&mut state, |ui, state| b.ui(ui, state));
        assert_eq!(b.row_count(), 0);
    }
}
