use std::sync::Arc;

use egui::{
    Align2, Color32, CornerRadius, FontId, Key, Modifiers, Rect, RichText, ScrollArea, Sense, Stroke, Ui, vec2,
};
use hp_app::runtime::{AssetKind, AssetPick};
use hp_app::source_filter::SourceFilter;
use hp_app::state::{ParticleCatalog, ThumbState};
use hp_app::{AppState, Severity, t, tf, theme};

use crate::browse::{BannerAction, Sources, pick_banner, source_chips};
use crate::edit::{Op, apply};
use crate::names::WordSearch;
use crate::view::{SelectionSummary, Target};
use crate::widgets::{ROW_HEIGHT, RowLook, list_row_look, search_box, thumbnail};

pub const TAB_ID: &str = "particles";

const EFFECT_KEY: &str = "effect_name";

type RowsKey = (usize, usize, String, Option<String>, SourceFilter, bool, u64);

pub struct ParticleBrowser {
    search: String,
    file: Option<String>,
    source: SourceFilter,
    sources: Sources,
    catalog: Option<Arc<ParticleCatalog>>,
    rows: Vec<u32>,
    rows_key: Option<RowsKey>,
    selected: Option<String>,
    grid: bool,
    scroll_to_selected: bool,
    last_pick: Option<(String, Vec<hp_editor::Id>)>,
}

impl Default for ParticleBrowser {
    fn default() -> Self {
        Self::new()
    }
}

impl ParticleBrowser {
    pub fn new() -> Self {
        Self {
            search: String::new(),
            file: None,
            source: SourceFilter::All,
            sources: Sources::default(),
            catalog: None,
            rows: Vec::new(),
            rows_key: None,
            selected: None,
            grid: true,
            scroll_to_selected: false,
            last_pick: None,
        }
    }

    pub fn set_search(&mut self, text: &str) {
        text.clone_into(&mut self.search);
    }

    pub fn set_file(&mut self, file: Option<&str>) {
        self.file = file.map(str::to_owned);
    }

    pub fn selected(&self) -> Option<&str> {
        self.selected.as_deref()
    }

    pub fn row_count(&self) -> usize {
        self.rows.len()
    }

    fn system(&self, row: usize) -> Option<&hp_app::state::ParticleSystemInfo> {
        let i = *self.rows.get(row)? as usize;
        self.catalog.as_ref()?.systems.get(i)
    }

    fn refresh(&mut self, state: &mut AppState) {
        self.sources.refresh(state.assets.as_ref());
        self.catalog = state.assets.particles();
        let gmod_only = state.settings.browser.gmod_only;
        let (ptr, len) = self.catalog.as_ref().map_or((0, 0), |c| (Arc::as_ptr(c) as usize, c.systems.len()));
        let key: RowsKey =
            (ptr, len, self.search.clone(), self.file.clone(), self.source, gmod_only, state.assets.generation());
        if self.rows_key.as_ref() == Some(&key) {
            return;
        }
        let search = WordSearch::parse(&self.search);
        self.rows = match &self.catalog {
            Some(c) => c
                .systems
                .iter()
                .enumerate()
                .filter(|(_, s)| {
                    self.file.as_deref().is_none_or(|f| s.file == f)
                        && self.sources.passes(self.source, gmod_only, Some(s.source))
                        && search.matches2(&s.name, &s.file)
                })
                .map(|(i, _)| i as u32)
                .collect(),
            None => Vec::new(),
        };
        self.rows_key = Some(key);
    }

    pub fn ui(&mut self, ui: &mut Ui, state: &mut AppState) {
        let ctx = ui.ctx().clone();
        self.refresh(state);
        let pick =
            state.runtime.entities.asset_pick.clone().filter(|p| p.result.is_none() && p.kind == AssetKind::Particle);
        self.follow_pick(pick.as_ref());

        ui.horizontal(|ui| {
            for (grid, label) in [(true, t!("entities.pb_grid")), (false, t!("entities.pb_list"))] {
                if ui.add(egui::Button::selectable(self.grid == grid, label)).clicked() {
                    self.grid = grid;
                }
            }
            ui.with_layout(egui::Layout::right_to_left(egui::Align::Center), |ui| {
                let windowed = state.runtime.entities.particle_window;
                let (glyph, tip) =
                    if windowed { ("⊟", t!("entities.dock_back")) } else { ("⊞", t!("entities.pop_out")) };
                if ui.small_button(glyph).on_hover_text(tip).clicked() {
                    state.runtime.entities.particle_window = !windowed;
                    if windowed {
                        state.settings.browser.show_particle_browser = true;
                        state.settings_dirty = true;
                        state.ui.right_tab = hp_app::state::RightTab::Package(TAB_ID);
                    }
                }
                if self.grid {
                    let mut tile = state.settings.browser.tile();
                    let slider = egui::Slider::new(&mut tile, 48.0..=256.0).show_value(false).step_by(8.0);
                    if ui.add(slider).on_hover_text(t!("entities.tile_size")).changed() {
                        state.settings.browser.tile_size = tile;
                        state.settings_dirty = true;
                    }
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
        search_box(ui, &mut self.search, t!("entities.pb_search"));
        ui.horizontal(|ui| {
            let files: &[String] = self.catalog.as_ref().map_or(&[][..], |c| c.files.as_slice());
            let current = self.file.clone().unwrap_or_else(|| t!("entities.pb_all_files").to_owned());
            egui::ComboBox::from_id_salt("hp_ui_entities.particle_file")
                .width((ui.available_width() - 4.0).max(80.0))
                .selected_text(current)
                .truncate()
                .height(400.0)
                .show_ui(ui, |ui| {
                    if ui.selectable_label(self.file.is_none(), t!("entities.pb_all_files")).clicked() {
                        self.file = None;
                    }
                    for f in files {
                        if ui.selectable_label(self.file.as_deref() == Some(f), f).clicked() {
                            self.file = Some(f.clone());
                        }
                    }
                });
        });
        let mut gmod_only = state.settings.browser.gmod_only;
        if source_chips(ui, &self.sources, &mut self.source, &mut gmod_only)
            && gmod_only != state.settings.browser.gmod_only
        {
            state.settings.browser.gmod_only = gmod_only;
            state.settings_dirty = true;
        }
        self.refresh(state);
        let indexing = self.catalog.is_none() && state.assets.fs().is_some();
        ui.horizontal(|ui| {
            if indexing {
                ui.spinner();
                ui.label(RichText::new(t!("entities.pb_indexing")).weak().small());
                ctx.request_repaint_after(std::time::Duration::from_millis(150));
            } else {
                ui.label(RichText::new(tf!("entities.pb_count", self.rows.len())).weak().small());
            }
        });

        let list_h = (ui.available_height() - 3.0 * ui.spacing().interact_size.y - 16.0).max(ROW_HEIGHT * 5.0);
        let (clicked, activated, hovered) =
            if self.grid { self.grid_ui(ui, state, list_h) } else { self.list_ui(ui, state, list_h) };
        if let Some(name) = clicked {
            self.selected = Some(name);
        }
        let mut activate = activated;
        if hovered && !ctx.text_edit_focused() && ui.input_mut(|i| i.consume_key(Modifiers::NONE, Key::Enter)) {
            activate = self.selected.clone();
        }
        if let Some(name) = activate {
            self.selected = Some(name);
            if pick.is_some() {
                self.choose(state);
            } else {
                self.apply_to_selection(state);
            }
        }
        if self.rows.is_empty() && !indexing {
            ui.label(RichText::new(t!("entities.no_match")).weak());
        }
        ui.separator();
        self.footer(ui, state);
    }

    fn grid_ui(&mut self, ui: &mut Ui, state: &mut AppState, height: f32) -> (Option<String>, Option<String>, bool) {
        let tile = state.settings.browser.tile();
        let gap = 4.0;
        let label_h = 15.0;
        let width = ui.available_width().max(tile);
        let cols = (((width + gap) / (tile + gap)).floor() as usize).max(1);
        let rows = self.rows.len().div_ceil(cols);
        let row_h = tile + label_h + gap;
        let mut area = ScrollArea::vertical()
            .id_salt("hp_ui_entities.particle_grid")
            .max_height(height)
            .auto_shrink([false, false]);
        if std::mem::take(&mut self.scroll_to_selected)
            && let Some(sel) = &self.selected
            && let Some(i) =
                (0..self.rows.len()).find(|&i| self.system(i).is_some_and(|s| s.name.eq_ignore_ascii_case(sel)))
        {
            area = area.vertical_scroll_offset(((i / cols) as f32 * row_h - row_h).max(0.0));
        }
        let (mut clicked, mut activated) = (None, None);
        let ctx = ui.ctx().clone();
        let out = area.show_rows(ui, row_h, rows, |ui, range| {
            for r in range {
                let (rect, _) = ui.allocate_exact_size(vec2(width, row_h), Sense::hover());
                for c in 0..cols {
                    let i = r * cols + c;
                    let Some(sys) = self.system(i) else { break };
                    let cell =
                        Rect::from_min_size(rect.min + vec2(c as f32 * (tile + gap), 0.0), vec2(tile, tile + label_h));
                    let resp = ui.interact(cell, ui.scope_id().with(("particle_tile", i)), Sense::CLICK);
                    let selected = self.selected.as_deref().is_some_and(|s| s.eq_ignore_ascii_case(&sys.name));
                    let painter = ui.painter_at(cell.expand(1.0));
                    if selected {
                        painter.rect_filled(cell, CornerRadius::same(4), theme::accent_bg());
                        painter.rect_stroke(
                            cell,
                            CornerRadius::same(4),
                            Stroke::new(1.5, theme::accent()),
                            egui::StrokeKind::Inside,
                        );
                    } else if resp.hovered() {
                        painter.rect_filled(cell, CornerRadius::same(4), ui.visuals().widgets.hovered.bg_fill);
                    }
                    let thumb_rect = Rect::from_min_size(cell.min + vec2(4.0, 4.0), vec2(tile - 8.0, tile - 8.0));
                    let thumb = if sys.material.is_empty() {
                        ThumbState::Missing
                    } else {
                        state.assets.thumbnail(&ctx, &sys.material)
                    };
                    paint_thumb(&painter, thumb_rect, thumb);
                    let mut job = egui::text::LayoutJob::simple_singleline(
                        sys.name.clone(),
                        FontId::proportional(11.0),
                        ui.visuals().text_color(),
                    );
                    job.wrap = egui::text::TextWrapping {
                        max_width: tile - 4.0,
                        max_rows: 1,
                        break_anywhere: true,
                        overflow_character: Some('…'),
                    };
                    let galley = painter.layout_job(job);
                    let x = cell.center().x - galley.size().x * 0.5;
                    painter.galley(egui::pos2(x, cell.bottom() - label_h), galley, ui.visuals().text_color());
                    let resp = resp.on_hover_ui(|ui| system_tooltip(ui, sys, &self.sources));
                    if resp.clicked() {
                        clicked = Some(sys.name.clone());
                    }
                    if resp.double_clicked() {
                        activated = Some(sys.name.clone());
                    }
                }
            }
        });
        let hovered = ui.rect_contains_pointer(out.inner_rect);
        (clicked, activated, hovered)
    }

    fn list_ui(&mut self, ui: &mut Ui, state: &mut AppState, height: f32) -> (Option<String>, Option<String>, bool) {
        let mut area = ScrollArea::vertical()
            .id_salt("hp_ui_entities.particle_list")
            .max_height(height)
            .auto_shrink([false, false]);
        if std::mem::take(&mut self.scroll_to_selected)
            && let Some(sel) = &self.selected
            && let Some(i) =
                (0..self.rows.len()).find(|&i| self.system(i).is_some_and(|s| s.name.eq_ignore_ascii_case(sel)))
        {
            area = area.vertical_scroll_offset((i as f32 * (ROW_HEIGHT + ui.spacing().item_spacing.y) - 60.0).max(0.0));
        }
        let (mut clicked, mut activated) = (None, None);
        let ctx = ui.ctx().clone();
        let out = area.show_rows(ui, ROW_HEIGHT, self.rows.len(), |ui, range| {
            for i in range {
                let Some(sys) = self.system(i) else { continue };
                let selected = self.selected.as_deref().is_some_and(|s| s.eq_ignore_ascii_case(&sys.name));
                let badge = self.sources.badge_text(Some(sys.source));
                let detail = format!("{}  ·  {badge}", sys.file);
                let look = RowLook {
                    main: &sys.name,
                    detail: &detail,
                    selected,
                    indent: ROW_HEIGHT + 2.0,
                    ..RowLook::default()
                };
                let resp = list_row_look(ui, look, crate::widgets::CLICK);
                let thumb = if sys.material.is_empty() {
                    ThumbState::Missing
                } else {
                    state.assets.thumbnail(&ctx, &sys.material)
                };
                let r = Rect::from_min_size(resp.rect.min + vec2(2.0, 1.0), vec2(ROW_HEIGHT - 2.0, ROW_HEIGHT - 2.0));
                paint_thumb(&ui.painter_at(r), r, thumb);
                let resp = resp.on_hover_ui(|ui| system_tooltip(ui, sys, &self.sources));
                if resp.clicked() {
                    clicked = Some(sys.name.clone());
                }
                if resp.double_clicked() {
                    activated = Some(sys.name.clone());
                }
            }
        });
        let hovered = ui.rect_contains_pointer(out.inner_rect);
        (clicked, activated, hovered)
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
            self.selected = Some(p.current.trim().to_owned());
            self.scroll_to_selected = true;
        }
    }

    fn choose(&mut self, state: &mut AppState) {
        let Some(sel) = self.selected.clone() else { return };
        if let Some(p) = state.runtime.entities.asset_pick.as_mut()
            && p.kind == AssetKind::Particle
        {
            p.result = Some(sel);
        }
    }

    fn particle_entities(state: &AppState) -> Vec<hp_editor::Id> {
        SelectionSummary::of(&state.doc)
            .entities
            .into_iter()
            .filter(|id| {
                state
                    .doc
                    .scene()
                    .entity(*id)
                    .is_some_and(|n| n.entity.classname.eq_ignore_ascii_case("info_particle_system"))
            })
            .collect()
    }

    fn apply_to_selection(&mut self, state: &mut AppState) {
        let Some(sel) = self.selected.clone() else { return };
        let ids = Self::particle_entities(state);
        if ids.is_empty() {
            return;
        }
        let op = Op::SetKey { target: Target::Entities(ids), key: EFFECT_KEY.to_owned(), value: sel, merge: false };
        if let Err(e) = apply(&mut state.doc, None, op) {
            state.notify(Severity::Error, tf!("entities.edit_failed", e));
        }
    }

    fn footer(&mut self, ui: &mut Ui, state: &mut AppState) {
        let sel = self.selected.clone();
        let info = sel
            .as_deref()
            .and_then(|s| self.catalog.as_ref()?.systems.iter().find(|x| x.name.eq_ignore_ascii_case(s)).cloned());
        ui.horizontal(|ui| {
            let text = sel.clone().unwrap_or_else(|| t!("entities.pb_select_hint").to_owned());
            ui.add(egui::Label::new(RichText::new(text).strong()).truncate());
        });
        if let Some(i) = &info {
            ui.label(
                RichText::new(format!("{}  ·  {}", i.file, self.sources.badge_text(Some(i.source)))).small().weak(),
            );
        }
        ui.horizontal_wrapped(|ui| {
            let n = Self::particle_entities(state).len();
            let apply_btn = ui.add_enabled(sel.is_some() && n > 0, egui::Button::new(tf!("entities.pb_apply", n)));
            if apply_btn.on_hover_text(t!("entities.pb_apply_tip")).clicked() {
                self.apply_to_selection(state);
            }
            if ui.add_enabled(sel.is_some(), egui::Button::new(t!("entities.sb_copy"))).clicked()
                && let Some(s) = &sel
            {
                ui.ctx().copy_text(s.clone());
            }
            if ui.add_enabled(sel.is_some(), egui::Button::new(t!("entities.pb_edit"))).clicked()
                && let Some(name) = &sel
            {
                state.runtime.lab.particle_preview.selected = Some(name.clone());
                state.request(hp_app::Action::ParticleEditor);
            }
        });
        if state.assets.fs().is_none() {
            ui.label(RichText::new(t!("entities.sb_no_content")).color(theme::warning()).small());
        }
    }
}

fn paint_thumb(painter: &egui::Painter, rect: Rect, thumb: ThumbState) {
    match thumb {
        ThumbState::Ready { texture, size } => {
            let (w, h) = (size[0].max(1) as f32, size[1].max(1) as f32);
            let s = (rect.width() / w).min(rect.height() / h);
            let img = Rect::from_center_size(rect.center(), vec2(w * s, h * s));
            painter.rect_filled(rect, CornerRadius::same(2), Color32::from_gray(20));
            painter.image(texture, img, Rect::from_min_max(egui::pos2(0.0, 0.0), egui::pos2(1.0, 1.0)), Color32::WHITE);
        }
        ThumbState::Loading => {
            painter.rect_filled(rect, CornerRadius::same(2), Color32::from_gray(44));
        }
        ThumbState::Missing => {
            painter.rect_filled(rect, CornerRadius::same(2), Color32::from_gray(30));
            painter.text(rect.center(), Align2::CENTER_CENTER, "?", FontId::proportional(14.0), theme::warning());
        }
    }
}

fn system_tooltip(ui: &mut Ui, sys: &hp_app::state::ParticleSystemInfo, sources: &Sources) {
    ui.label(RichText::new(&sys.name).strong());
    ui.label(tf!("entities.pb_file", sys.file));
    if !sys.material.is_empty() {
        ui.label(tf!("entities.pb_material", sys.material));
    }
    ui.label(RichText::new(sources.label(Some(sys.source))).small().weak());
    let _ = thumbnail;
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn search_file_filter_and_pick() {
        crate::register_translations();
        let mut state = hp_app::testing::test_state();
        let mut b = ParticleBrowser::new();
        hp_app::testing::run_ui(&mut state, |ui, state| b.ui(ui, state));
        assert_eq!(b.row_count(), 3, "the mock catalog");
        b.set_search("fire !large");
        hp_app::testing::run_ui(&mut state, |ui, state| b.ui(ui, state));
        assert_eq!(b.row_count(), 1);
        b.set_search("");
        b.set_file(Some("particles/smoke.pcf"));
        hp_app::testing::run_ui(&mut state, |ui, state| b.ui(ui, state));
        assert_eq!(b.row_count(), 1);
        b.set_file(None);
        b.grid = false;
        hp_app::testing::run_ui(&mut state, |ui, state| b.ui(ui, state));
        assert_eq!(b.row_count(), 3);

        let id = state.doc.create_point_entity("info_particle_system", glam::DVec3::ZERO, &[]).unwrap();
        let target = Target::Entities(vec![id]);
        crate::pick::start_asset_pick(
            &mut state.runtime.entities.asset_pick,
            AssetKind::Particle,
            &target,
            EFFECT_KEY,
            "info_particle_system (effect_name)".into(),
            "smoke_gib_01",
        );
        hp_app::testing::run_ui(&mut state, |ui, state| b.ui(ui, state));
        assert_eq!(b.selected(), Some("smoke_gib_01"));
        b.selected = Some("env_fire_large".into());
        b.choose(&mut state);
        assert!(crate::pick::finish_asset_pick(&mut state));
        assert_eq!(state.doc.scene().entity(id).unwrap().entity.get(EFFECT_KEY), Some("env_fire_large"));
        state.doc.set_selection(&[hp_editor::ObjRef::Entity(id), hp_editor::ObjRef::Entity(5)]);
        b.selected = Some("smoke_gib_01".into());
        b.apply_to_selection(&mut state);
        assert_eq!(state.doc.scene().entity(id).unwrap().entity.get(EFFECT_KEY), Some("smoke_gib_01"));
        assert_eq!(state.doc.scene().entity(5).unwrap().entity.get(EFFECT_KEY), None, "only particle entities");
    }
}
