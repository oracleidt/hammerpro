use std::collections::HashMap;
use std::collections::hash_map::DefaultHasher;
use std::hash::{Hash, Hasher};
use std::sync::Arc;
use std::time::Duration;

use egui::{Key, Modifiers, Rect, RichText, Sense, Vec2, pos2, vec2};
use hp_app::state::{MaterialInfo, ThumbState};
use hp_app::{AppState, t, tf, theme};
use hp_fs::AssetEntry;

use crate::browser::{excluded_folders, paint_thumbnail};
use crate::grid::{CELL_PADDING, GridLayout};
use crate::icons::{Icon, icon_button};
use crate::indexes::{FlagsIndex, KeywordIndex};
use crate::search::{
    Category, FilterParams, Filtered, MISSING_SOURCE, Query, SearchIndex, SourceFilter, SourceGroup, SourceTable,
    filter_materials,
};
use crate::util::{leaf_name, normalize_material};

const THUMB: f32 = 64.0;
const LABEL_HEIGHT: f32 = 14.0;
const SPACING: f32 = 4.0;
const COLUMNS: usize = 5;
const GRID_HEIGHT: f32 = 300.0;
const CURRENT_HEIGHT: f32 = 32.0;

pub const SECTIONS: [Category; 4] = [Category::Used, Category::Recent, Category::Favorites, Category::All];

pub fn content_width() -> f32 {
    COLUMNS as f32 * (THUMB + 2.0 * CELL_PADDING) + (COLUMNS as f32 - 1.0) * SPACING
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub enum PickerOutcome {
    Picked(String),
    Escaped,
}

#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
struct Keys {
    dx: isize,
    dy: isize,
    enter: bool,
}

fn take_keys(ctx: &egui::Context, grid: bool) -> Keys {
    ctx.input_mut(|i| {
        let mut k = Keys::default();
        let mut count = |key: Key| {
            let mut n = 0;
            while i.consume_key(Modifiers::NONE, key) {
                n += 1;
            }
            n
        };
        k.dy += count(Key::ArrowDown) - count(Key::ArrowUp);
        if grid {
            k.dx += count(Key::ArrowRight) - count(Key::ArrowLeft);
        }
        k.enter = count(Key::Enter) > 0;
        k
    })
}

pub struct MaterialPicker {
    id: egui::Id,
    search: String,
    query: Query,
    query_text: Option<String>,
    category: Category,
    opened: bool,
    list_key: Option<(usize, usize, u64)>,
    list: Arc<Vec<AssetEntry>>,
    index: SearchIndex,
    sources_key: Option<(u64, u64)>,
    sources: SourceTable,
    used: (Option<u64>, Vec<String>),
    filtered: Filtered,
    filter_key: Option<u64>,
    keywords: KeywordIndex,
    flags: FlagsIndex,
    infos: HashMap<String, Arc<MaterialInfo>>,
    cursor: Option<usize>,
    reveal: bool,
    scroll_top: bool,
    focus_search: bool,
    scroll_offset: f32,
    view_height: f32,
    layout: Option<GridLayout>,
    cells: Vec<(String, Rect)>,
    current_rect: Option<Rect>,
}

impl MaterialPicker {
    pub fn new(salt: &str) -> Self {
        Self {
            id: egui::Id::unique(("hp_material_picker", salt)),
            search: String::new(),
            query: Query::default(),
            query_text: None,
            category: Category::Used,
            opened: false,
            list_key: None,
            list: Arc::new(Vec::new()),
            index: SearchIndex::default(),
            sources_key: None,
            sources: SourceTable::default(),
            used: (None, Vec::new()),
            filtered: Filtered::default(),
            filter_key: None,
            keywords: KeywordIndex::default(),
            flags: FlagsIndex::default(),
            infos: HashMap::new(),
            cursor: None,
            reveal: false,
            scroll_top: false,
            focus_search: false,
            scroll_offset: 0.0,
            view_height: 0.0,
            layout: None,
            cells: Vec::new(),
            current_rect: None,
        }
    }

    pub fn popup_id(&self) -> egui::Id {
        self.id
    }

    pub fn search_id(&self) -> egui::Id {
        self.id.with("search")
    }

    pub fn is_open(&self, ctx: &egui::Context) -> bool {
        egui::Popup::is_id_open(ctx, self.id)
    }

    pub fn open(&mut self, ctx: &egui::Context, default: Category) {
        if !self.opened {
            self.category = default;
            self.opened = true;
        }
        self.search.clear();
        self.cursor = None;
        self.scroll_top = true;
        self.focus_search = true;
        egui::Popup::open_id(ctx, self.id);
    }

    pub fn close(&mut self, ctx: &egui::Context) {
        egui::Popup::close_id(ctx, self.id);
        self.cells.clear();
        self.current_rect = None;
    }

    pub fn category(&self) -> Category {
        self.category
    }

    pub fn set_category(&mut self, c: Category) {
        if self.category != c {
            self.category = c;
            self.cursor = None;
            self.scroll_top = true;
        }
        self.opened = true;
    }

    pub fn search(&self) -> &str {
        &self.search
    }

    pub fn set_search(&mut self, text: &str) {
        if self.search != text {
            text.clone_into(&mut self.search);
            self.cursor = None;
            self.scroll_top = true;
        }
    }

    pub fn shown_names(&self) -> Vec<String> {
        (0..self.filtered.len()).filter_map(|i| self.filtered.get(&self.list, i)).map(|e| e.name.clone()).collect()
    }

    pub fn indexing(&self) -> bool {
        (self.query.needs_info() && self.keywords.pending()) || (self.query.needs_flags() && self.flags.pending())
    }

    pub fn cursor(&self) -> Option<usize> {
        self.cursor
    }

    pub fn cell_rect(&self, name: &str) -> Option<Rect> {
        self.cells.iter().find(|(n, _)| n == name).map(|(_, r)| *r)
    }

    pub fn current_rect(&self) -> Option<Rect> {
        self.current_rect
    }

    fn refresh(&mut self, state: &AppState, ctx: &egui::Context) {
        let list = state.assets.materials();
        let key = (Arc::as_ptr(&list) as usize, list.len(), state.assets.generation());
        if self.list_key != Some(key) {
            let list = if list.windows(2).all(|w| w[0].name <= w[1].name) {
                list
            } else {
                let mut v = (*list).clone();
                v.sort_by(|a, b| a.name.cmp(&b.name));
                Arc::new(v)
            };
            self.index = SearchIndex::build(&list);
            self.list = list;
            self.list_key = Some(key);
            self.infos.clear();
            self.filter_key = None;
        }
        let fs_gen = state.assets.fs().map_or(0, |f| f.generation());
        let sources_key = (state.assets.generation(), fs_gen);
        if self.sources_key != Some(sources_key) {
            let mut table = SourceTable::default();
            if let Some(fs) = state.assets.fs() {
                for s in fs.sources() {
                    table.insert(s.id, Some(SourceGroup::of(s.origin)), &s.label);
                }
            }
            self.sources = table;
            self.sources_key = Some(sources_key);
            self.filter_key = None;
        }
        let rev = state.doc.revision();
        if self.used.0 != Some(rev) {
            self.used = (Some(rev), crate::search::used_materials(&state.doc));
        }
        if self.query_text.as_deref() != Some(self.search.as_str()) {
            self.query = Query::parse(&self.search);
            self.query_text = Some(self.search.clone());
        }
        if self.keywords.update(state, ctx, self.query.needs_info()) {
            self.filter_key = None;
        }
        if self.flags.update(state, ctx, self.list_key, &self.list, self.query.needs_flags()) {
            self.filter_key = None;
        }
        let excluded = excluded_folders(state);
        let browser = &state.settings.browser;
        let mut h = DefaultHasher::new();
        self.list_key.hash(&mut h);
        self.sources_key.hash(&mut h);
        self.search.hash(&mut h);
        self.category.hash(&mut h);
        excluded.hash(&mut h);
        match self.category {
            Category::Used => self.used.0.hash(&mut h),
            Category::Favorites => browser.favorites.hash(&mut h),
            Category::Recent => browser.recent_materials.hash(&mut h),
            Category::All | Category::Tools => {}
        }
        if self.query.needs_info() {
            self.keywords.index().map(Arc::as_ptr).hash(&mut h);
        }
        if self.query.needs_flags() {
            self.flags.index().map(Arc::as_ptr).hash(&mut h);
            if self.flags.index().is_none() {
                self.infos.len().hash(&mut h);
            }
        }
        let key = h.finish();
        if self.filter_key == Some(key) {
            return;
        }
        let members: Vec<String> = match self.category {
            Category::Used => self.used.1.clone(),
            Category::Favorites => {
                let mut v: Vec<String> = browser.favorites.iter().map(|f| normalize_material(f)).collect();
                v.sort();
                v
            }
            Category::Recent => browser.recent_materials.iter().map(|f| normalize_material(f)).collect(),
            Category::All | Category::Tools => Vec::new(),
        };
        let params = FilterParams {
            query: &self.query,
            source: SourceFilter::All,
            category: self.category,
            hide_tools: false,
            members: &members,
            infos: &self.infos,
            keywords: self.keywords.index().map(|i| &**i),
            sources: &self.sources,
            flags: self.flags.index().map(|i| &**i),
            excluded: &excluded,
            hidden_shaders: &[],
        };
        self.filtered = filter_materials(&self.list, &self.index, &params);
        self.filter_key = Some(key);
        if self.cursor.is_some_and(|c| c >= self.filtered.len()) {
            self.cursor = None;
        }
    }

    fn section_count(&self, c: Category, state: &AppState) -> usize {
        match c {
            Category::Used => self.used.1.len(),
            Category::Recent => state.settings.browser.recent_materials.len(),
            Category::Favorites => state.settings.browser.favorites.len(),
            Category::All | Category::Tools => self.list.len(),
        }
    }

    pub fn show(
        &mut self,
        ctx: &egui::Context,
        anchor: Rect,
        layer: egui::LayerId,
        state: &mut AppState,
        value: &str,
    ) -> Option<PickerOutcome> {
        if !self.is_open(ctx) {
            self.cells.clear();
            self.current_rect = None;
            return None;
        }
        if ctx.input_mut(|i| i.consume_key(Modifiers::NONE, Key::Escape)) {
            self.close(ctx);
            return Some(PickerOutcome::Escaped);
        }
        self.refresh(state, ctx);
        let keys = take_keys(ctx, self.cursor.is_some());
        let mut picked = self.apply_keys(keys);
        let width = content_width();
        let value = normalize_material(value);
        let shown = egui::Popup::new(self.id, ctx.clone(), anchor, layer)
            .open_memory(None)
            .close_behavior(egui::PopupCloseBehavior::CloseOnClickOutside)
            .gap(2.0)
            .width(width)
            .accessible_name(t!("tex.picker.title"))
            .show(|ui| {
                ui.set_width(width);
                self.contents(ui, state, &value)
            });
        match shown {
            Some(r) => {
                if picked.is_none() {
                    picked = r.inner;
                }
            }
            None => {
                self.cells.clear();
                self.current_rect = None;
            }
        }
        let picked = picked?;
        self.close(ctx);
        Some(PickerOutcome::Picked(picked))
    }

    fn apply_keys(&mut self, keys: Keys) -> Option<String> {
        let n = self.filtered.len();
        if keys.enter {
            let i = self.cursor.unwrap_or(0);
            return self.filtered.get(&self.list, i).map(|e| e.name.clone());
        }
        if n == 0 || (keys.dx == 0 && keys.dy == 0) {
            return None;
        }
        let layout = self.layout.unwrap_or_else(|| GridLayout::new(content_width(), THUMB, LABEL_HEIGHT, SPACING, n));
        let layout = GridLayout { count: n, rows: n.div_ceil(layout.columns), ..layout };
        self.cursor = match self.cursor {
            None if keys.dy > 0 => Some(layout.step(0, keys.dx, keys.dy - 1)),
            None => None,
            Some(c) if keys.dy < 0 && layout.cell_of(c).0 == 0 => {
                self.focus_search = true;
                None
            }
            Some(c) => Some(layout.step(c, keys.dx, keys.dy)),
        };
        self.reveal = self.cursor.is_some();
        None
    }

    fn contents(&mut self, ui: &mut egui::Ui, state: &mut AppState, value: &str) -> Option<String> {
        let mut picked = None;
        ui.spacing_mut().item_spacing = vec2(6.0, 5.0);
        let current = state.current_material.clone();
        if current.is_empty() {
            self.current_rect = None;
        } else if self.current_row(ui, state, &current).clicked() {
            picked = Some(current);
        }

        ui.horizontal(|ui| {
            let side = ui.spacing().interact_size.y;
            let width = (ui.available_width() - side - ui.spacing().item_spacing.x).max(60.0);
            let r = ui.add(
                egui::TextEdit::singleline(&mut self.search)
                    .id(self.id.with("search"))
                    .hint_text(t!("tex.browser.search_hint"))
                    .desired_width(width),
            );
            if std::mem::take(&mut self.focus_search) {
                r.request_focus();
            }
            if r.changed() {
                self.cursor = None;
                self.scroll_top = true;
            }
            if icon_button(ui, Icon::Close, t!("common.clear")).clicked() {
                self.set_search("");
                self.focus_search = true;
            }
        });

        let mut section = None;
        ui.horizontal_wrapped(|ui| {
            ui.spacing_mut().item_spacing.x = 3.0;
            for c in SECTIONS {
                let n = self.section_count(c, state);
                let label = match c {
                    Category::Used => tf!("tex.picker.used", n),
                    Category::Recent => tf!("tex.picker.recent", n),
                    Category::Favorites => tf!("tex.browser.cat_favorites", n),
                    Category::All | Category::Tools => tf!("tex.picker.all", n),
                };
                let chip = egui::Button::selectable(self.category == c, RichText::new(label).small());
                if ui.add(chip).clicked() {
                    section = Some(c);
                }
            }
        });
        if let Some(c) = section {
            self.set_category(c);
            self.focus_search = true;
        }
        let ctx = ui.ctx().clone();
        self.refresh(state, &ctx);

        if self.filtered.is_empty() {
            self.cells.clear();
            ui.allocate_ui(vec2(ui.available_width(), GRID_HEIGHT), |ui| {
                ui.set_min_size(vec2(ui.available_width(), GRID_HEIGHT));
                ui.add_space(24.0);
                ui.vertical_centered(|ui| {
                    let empty = if self.search.trim().is_empty() {
                        match self.category {
                            Category::Used => t!("tex.picker.empty_used"),
                            Category::Recent => t!("tex.picker.empty_recent"),
                            Category::Favorites => t!("tex.picker.empty_favorites"),
                            Category::All | Category::Tools => t!("tex.browser.no_materials"),
                        }
                    } else {
                        t!("tex.picker.no_match")
                    };
                    ui.weak(empty);
                    if self.category != Category::All && ui.button(t!("tex.picker.search_all")).clicked() {
                        self.set_category(Category::All);
                        self.focus_search = true;
                    }
                });
            });
        } else if let Some(name) = self.grid(ui, state, value) {
            picked = Some(name);
        }

        ui.horizontal_wrapped(|ui| {
            ui.label(RichText::new(tf!("tex.picker.count", self.filtered.len())).small().weak());
            if self.query.needs_info() && self.keywords.pending() {
                ui.spinner();
                ui.label(RichText::new(t!("tex.browser.kw_index")).small().weak());
            }
            if self.query.needs_flags() && self.flags.pending() {
                ui.spinner();
                ui.label(RichText::new(t!("tex.browser.flags_index")).small().weak());
            }
            let enter = hp_app::actions::key_label(Key::Enter);
            let escape = hp_app::actions::key_label(Key::Escape);
            ui.label(RichText::new(tf!("tex.picker.keys", enter, escape)).small().weak());
        });
        picked
    }

    fn current_row(&mut self, ui: &mut egui::Ui, state: &mut AppState, name: &str) -> egui::Response {
        let (rect, resp) = ui.allocate_exact_size(vec2(ui.available_width(), CURRENT_HEIGHT), Sense::CLICK);
        self.current_rect = Some(rect);
        let label = tf!("tex.picker.current", name);
        resp.widget_info(|| egui::WidgetInfo::labeled(egui::Role::Button, true, &label));
        if ui.is_rect_visible(rect) {
            let painter = ui.painter_at(rect);
            if resp.hovered() {
                painter.rect_filled(rect, 4.0, ui.visuals().widgets.hovered.bg_fill);
            }
            let thumb_rect = Rect::from_min_size(rect.min + vec2(3.0, 3.0), Vec2::splat(CURRENT_HEIGHT - 6.0));
            let thumb = state.assets.thumbnail(ui.ctx(), name);
            if thumb == ThumbState::Loading {
                ui.ctx().request_repaint_after(Duration::from_millis(100));
            }
            paint_thumbnail(&painter, thumb_rect, thumb, false);
            let text_x = thumb_rect.right() + 8.0;
            let max_width = (rect.right() - text_x - 4.0).max(10.0);
            painter.text(
                pos2(text_x, rect.top() + 3.0),
                egui::Align2::LEFT_TOP,
                t!("tex.picker.current_label"),
                egui::FontId::proportional(10.5),
                ui.visuals().weak_text_color(),
            );
            let color = ui.visuals().strong_text_color();
            let galley = elided(&painter, name, 12.5, max_width, color);
            painter.galley(pos2(text_x, rect.bottom() - galley.size().y - 2.0), galley, color);
        }
        resp.on_hover_text(t!("tex.picker.current_tip"))
    }

    fn grid(&mut self, ui: &mut egui::Ui, state: &mut AppState, value: &str) -> Option<String> {
        let ctx = ui.ctx().clone();
        let width = ui.available_width();
        let layout = GridLayout::new(width, THUMB, LABEL_HEIGHT, SPACING, self.filtered.len());
        self.layout = Some(layout);
        let mut sa = egui::ScrollArea::vertical()
            .id_salt(self.id.with("grid"))
            .max_height(GRID_HEIGHT)
            .min_scrolled_height(GRID_HEIGHT)
            .auto_shrink([false, false]);
        if std::mem::take(&mut self.scroll_top) {
            sa = sa.vertical_scroll_offset(0.0);
            self.reveal = false;
        } else if std::mem::take(&mut self.reveal)
            && let Some(c) = self.cursor
            && let Some(off) = layout.scroll_to_show(c, self.scroll_offset, self.view_height.max(layout.cell_height))
        {
            sa = sa.vertical_scroll_offset(off);
        }
        let list = Arc::clone(&self.list);
        let filtered = &self.filtered;
        let cursor = self.cursor;
        let id = self.id;
        let mut cells = Vec::new();
        let mut picked = None;
        let mut any_loading = false;
        let mut unknown: Vec<String> = Vec::new();
        let out = sa.show_viewport(ui, |ui, viewport| {
            ui.set_height(layout.content_height());
            ui.set_width(width);
            let origin = ui.max_rect().min;
            let font = egui::FontId::proportional(10.5);
            for row in layout.visible_rows(viewport.min.y, viewport.max.y) {
                for index in layout.row_items(row) {
                    let Some(entry) = filtered.get(&list, index) else { continue };
                    let (x, y) = layout.cell_offset(index);
                    let rect = Rect::from_min_size(origin + vec2(x, y), vec2(layout.cell_width, layout.cell_height));
                    let resp = ui.interact(rect, id.with(("cell", entry.name.as_str())), Sense::CLICK);
                    resp.widget_info(|| egui::WidgetInfo::labeled(egui::Role::Button, true, &entry.name));
                    cells.push((entry.name.clone(), rect));
                    if !self.infos.contains_key(&entry.name) {
                        unknown.push(entry.name.clone());
                    }
                    let missing = entry.source == MISSING_SOURCE;
                    let painter = ui.painter_at(rect.expand(1.0));
                    if cursor == Some(index) {
                        painter.rect_filled(rect, 4.0, theme::accent_bg());
                        painter.rect_stroke(
                            rect,
                            4.0,
                            egui::Stroke::new(1.5, theme::accent()),
                            egui::StrokeKind::Inside,
                        );
                    } else if resp.hovered() {
                        painter.rect_filled(rect, 4.0, ui.visuals().widgets.hovered.bg_fill);
                    }
                    if entry.name == value {
                        painter.rect_stroke(
                            rect,
                            4.0,
                            egui::Stroke::new(1.0, theme::text_weak()),
                            egui::StrokeKind::Inside,
                        );
                    }
                    let thumb_rect = Rect::from_center_size(
                        pos2(rect.center().x, rect.top() + CELL_PADDING + THUMB * 0.5),
                        Vec2::splat(THUMB.min(layout.cell_width - 2.0 * CELL_PADDING)),
                    );
                    let thumb = if missing { ThumbState::Missing } else { state.assets.thumbnail(&ctx, &entry.name) };
                    any_loading |= thumb == ThumbState::Loading;
                    paint_thumbnail(&painter, thumb_rect, thumb, missing);
                    let color = if missing {
                        theme::error()
                    } else if cursor == Some(index) {
                        ui.visuals().strong_text_color()
                    } else {
                        ui.visuals().text_color()
                    };
                    let label_rect = Rect::from_min_max(
                        pos2(rect.left() + 2.0, rect.bottom() - LABEL_HEIGHT),
                        pos2(rect.right() - 2.0, rect.bottom()),
                    );
                    let mut job = egui::text::LayoutJob::simple_singleline(
                        leaf_name(&entry.name).to_owned(),
                        font.clone(),
                        color,
                    );
                    job.wrap = egui::text::TextWrapping {
                        max_width: label_rect.width(),
                        max_rows: 1,
                        break_anywhere: true,
                        overflow_character: Some('…'),
                    };
                    let galley = painter.layout_job(job);
                    let text_x = label_rect.center().x - galley.size().x * 0.5;
                    painter.galley(pos2(text_x, label_rect.center().y - galley.size().y * 0.5), galley, color);
                    if resp.clicked() {
                        picked = Some(entry.name.clone());
                    }
                    resp.on_hover_ui(|ui| tooltip(ui, state, entry));
                }
            }
        });
        self.scroll_offset = out.state.clamped_offset().y;
        self.view_height = out.inner_rect.height();
        cells.retain(|(_, r)| out.inner_rect.intersects(*r));
        self.cells = cells;
        for name in unknown {
            if let Some(info) = state.assets.material_info(&name) {
                self.infos.insert(name, info);
            }
        }
        if any_loading {
            ctx.request_repaint_after(Duration::from_millis(100));
        }
        picked
    }
}

fn elided(painter: &egui::Painter, text: &str, size: f32, max_width: f32, color: egui::Color32) -> Arc<egui::Galley> {
    let mut job = egui::text::LayoutJob::simple_singleline(text.to_owned(), egui::FontId::proportional(size), color);
    job.wrap =
        egui::text::TextWrapping { max_width, max_rows: 1, break_anywhere: true, overflow_character: Some('…') };
    painter.layout_job(job)
}

fn tooltip(ui: &mut egui::Ui, state: &mut AppState, entry: &AssetEntry) {
    ui.label(RichText::new(&entry.name).strong());
    if entry.source == MISSING_SOURCE {
        ui.colored_label(theme::error(), t!("tex.browser.missing"));
        return;
    }
    match state.assets.material_info(&entry.name) {
        Some(info) => {
            if info.width > 0 && info.height > 0 {
                ui.label(format!("{} × {}", info.width, info.height));
            }
            if !info.shader.is_empty() {
                ui.label(RichText::new(tf!("tex.tip.shader", info.shader)).small());
            }
        }
        None => {
            ui.weak(t!("common.loading"));
        }
    }
    ui.label(RichText::new(tf!("tex.tip.source", state.assets.source_label(entry.source))).small().weak());
}
