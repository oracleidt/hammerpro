use std::collections::hash_map::DefaultHasher;
use std::collections::{HashMap, HashSet};
use std::hash::{Hash, Hasher};
use std::path::{Path, PathBuf};
use std::sync::Arc;
use std::sync::mpsc::{Receiver, TryRecvError};
use std::time::Duration;

use egui::{Color32, Event, Key, Modifiers, Pos2, Rect, RichText, Sense, Vec2, pos2, vec2};
use hp_app::dnd::DragPayload;
use hp_app::state::{AssetStatus, MaterialInfo, RightTab, ThumbState};
use hp_app::{Action, AppState, Severity, ToolId, t, tf, theme};
use hp_editor::SelectOp;
use hp_fs::{AssetEntry, CustomLayout, CustomMode, CustomReport};

use crate::grid::{CELL_PADDING, GridLayout};
use crate::icons::{Icon, icon_button, icon_text_button, icon_toggle, paint_icon};
use crate::indexes::{FlagsIndex, KeywordIndex, MountKey};
use crate::search::{
    Category, FilterParams, Filtered, MISSING_SOURCE, MatFlag, Query, SearchIndex, SourceFilter, SourceGroup,
    SourceTable, filter_materials, normalize_exclusions, toggle_flag_word,
};
use crate::util::{PendingPick, folder_label, folder_of, leaf_name, same_path};
use hp_app::runtime::AssetKind;

const LABEL_HEIGHT: f32 = 15.0;
const GRID_SPACING: f32 = 4.0;

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum GridKey {
    Move { dx: isize, dy: isize, extend: bool },
    Page { dir: isize, extend: bool },
    Edge { end: bool, extend: bool },
    Apply,
}

pub fn take_grid_keys(events: &mut Vec<Event>) -> Vec<GridKey> {
    let mut out = Vec::new();
    events.retain(|e| {
        let Event::Key { key, pressed: true, modifiers, .. } = e else { return true };
        let plain = *modifiers == Modifiers::NONE;
        let shift = *modifiers == Modifiers::SHIFT;
        if !plain && !shift {
            return true;
        }
        let k = match key {
            Key::ArrowLeft => GridKey::Move { dx: -1, dy: 0, extend: shift },
            Key::ArrowRight => GridKey::Move { dx: 1, dy: 0, extend: shift },
            Key::ArrowUp => GridKey::Move { dx: 0, dy: -1, extend: shift },
            Key::ArrowDown => GridKey::Move { dx: 0, dy: 1, extend: shift },
            Key::PageUp => GridKey::Page { dir: -1, extend: shift },
            Key::PageDown => GridKey::Page { dir: 1, extend: shift },
            Key::Home => GridKey::Edge { end: false, extend: shift },
            Key::End => GridKey::Edge { end: true, extend: shift },
            Key::Enter if plain => GridKey::Apply,
            _ => return true,
        };
        out.push(k);
        false
    });
    out
}

#[derive(Default)]
struct FolderReports {
    key: Option<MountKey>,
    done: HashMap<u32, CustomReport>,
    pending: Option<Receiver<(u32, Option<CustomReport>)>>,
}

pub(crate) fn paint_thumbnail(painter: &egui::Painter, rect: Rect, thumb: ThumbState, missing: bool) {
    paint_thumbnail_alpha(painter, rect, thumb, missing, false);
}

pub(crate) fn paint_thumbnail_alpha(
    painter: &egui::Painter,
    rect: Rect,
    thumb: ThumbState,
    missing: bool,
    alpha: bool,
) {
    if missing {
        paint_checker(painter, rect);
        return;
    }
    match thumb {
        ThumbState::Ready { texture, size } => {
            let (w, h) = (size[0].max(1) as f32, size[1].max(1) as f32);
            let s = (rect.width() / w).min(rect.height() / h);
            let img = Rect::from_center_size(rect.center(), vec2(w * s, h * s));
            if alpha {
                paint_alpha_checker(painter, img);
            }
            painter.image(texture, img, Rect::from_min_max(pos2(0.0, 0.0), pos2(1.0, 1.0)), Color32::WHITE);
        }
        ThumbState::Loading => {
            painter.rect_filled(rect.shrink(1.0), 3.0, Color32::from_gray(44));
            let c = rect.center();
            for i in 0..3 {
                let x = c.x + (i as f32 - 1.0) * 7.0;
                painter.circle_filled(pos2(x, c.y), 2.0, Color32::from_gray(90));
            }
        }
        ThumbState::Missing => paint_checker(painter, rect),
    }
}

fn paint_alpha_checker(painter: &egui::Painter, rect: Rect) {
    let cell = 8.0;
    let cols = ((rect.width() / cell).ceil() as usize).min(64);
    let rows = ((rect.height() / cell).ceil() as usize).min(64);
    let painter = painter.with_clip_rect(rect.intersect(painter.clip_rect()));
    painter.rect_filled(rect, 0.0, Color32::from_gray(200));
    for y in 0..rows {
        for x in 0..cols {
            if (x + y) % 2 == 1 {
                let r = Rect::from_min_size(rect.min + vec2(x as f32 * cell, y as f32 * cell), Vec2::splat(cell));
                painter.rect_filled(r, 0.0, Color32::from_gray(140));
            }
        }
    }
}

fn paint_checker(painter: &egui::Painter, rect: Rect) {
    let n = 4;
    let cw = rect.width() / n as f32;
    let ch = rect.height() / n as f32;
    for y in 0..n {
        for x in 0..n {
            let color = if (x + y) % 2 == 0 { Color32::from_rgb(200, 0, 200) } else { Color32::from_gray(10) };
            let r = Rect::from_min_size(pos2(rect.left() + x as f32 * cw, rect.top() + y as f32 * ch), vec2(cw, ch));
            painter.rect_filled(r, 0.0, color);
        }
    }
}

pub fn apply_to_selection(state: &mut AppState, material: &str) -> Option<usize> {
    if material.trim().is_empty() {
        return None;
    }
    let faces = state.doc.selected_faces();
    let r = if !faces.is_empty() {
        state.doc.apply_material(&faces, material)
    } else {
        let objs = state.doc.selected_objects();
        if objs.is_empty() {
            state.notify(Severity::Warning, t!("tex.browser.nothing_selected"));
            return None;
        }
        state.doc.apply_material_to_objects(&objs, material)
    };
    match r {
        Ok(n) => {
            state.notify(Severity::Success, tf!("tex.browser.applied", n));
            Some(n)
        }
        Err(e) => {
            state.notify(Severity::Error, tf!("tex.browser.apply_failed", e));
            None
        }
    }
}

pub fn mark_material(state: &mut AppState, material: &str) -> usize {
    let n = state.doc.select_faces_by_material(material, SelectOp::Replace);
    if n > 0 {
        state.set_tool(ToolId::Texture);
    }
    state.notify(Severity::Info, tf!("tex.browser.marked", n));
    n
}

pub fn material_disk_file(state: &AppState, name: &str) -> Option<PathBuf> {
    let fs = state.assets.fs()?;
    ["vmt", "vtf", "png", "tga", "jpg", "jpeg", "bmp"]
        .iter()
        .find_map(|ext| fs.disk_path(&format!("materials/{name}.{ext}")))
}

pub fn toggle_favorite(state: &mut AppState, name: &str) {
    let favs = &mut state.settings.browser.favorites;
    if let Some(i) = favs.iter().position(|f| f == name) {
        favs.remove(i);
    } else {
        favs.push(name.to_owned());
    }
    state.settings_dirty = true;
}

#[derive(Clone, Debug)]
struct CustomSource {
    id: u32,
    path: PathBuf,
    layout: Option<CustomLayout>,
    models: usize,
}

#[derive(Default)]
struct SourcesCache {
    key: Option<(u64, u64, usize)>,
    table: SourceTable,
    customs: Vec<CustomSource>,
    has_game: bool,
    has_addons: bool,
    has_workshop: bool,
}

#[derive(Default)]
struct ListCache {
    key: Option<(usize, usize, u64)>,
    list: Arc<Vec<AssetEntry>>,
    index: SearchIndex,
    counts: HashMap<u32, usize>,
}

enum CellAction {
    Shader(String),
    Click { index: usize, name: String, ctrl: bool, shift: bool },
    Apply(String),
    Mark(String),
    Favorite(String),
    Copy(String),
    ReplaceWith(String),
    Reveal(PathBuf),
    Edit(String),
    Vtf(String),
    Exclude(String),
}

enum FolderAction {
    Enable(usize, bool),
    Models(usize, bool),
    Prefix(usize, String),
    Remove(usize),
    ShowOnly(u32),
    Reveal(PathBuf),
}

const MODEL_SHADER: &str = "vertexlitgeneric";

pub struct TextureBrowser {
    search: String,
    query: Query,
    query_text: Option<String>,
    source: SourceFilter,
    category: Category,
    list: ListCache,
    sources: SourcesCache,
    filtered: Filtered,
    filter_key: Option<u64>,
    infos: HashMap<String, Arc<MaterialInfo>>,
    keywords: KeywordIndex,
    flags: FlagsIndex,
    flags_wanted: bool,
    reports: FolderReports,
    used: (Option<u64>, Vec<String>),
    marked: Vec<String>,
    anchor: Option<usize>,
    scroll_offset: f32,
    viewport_height: f32,
    scroll_to: Option<usize>,
    scroll_top: bool,
    scroll_center: bool,
    last_current: String,
    layout: Option<GridLayout>,
    prefix_buf: HashMap<PathBuf, String>,
    pending_folder: Option<PendingPick<PathBuf>>,
    pending_images: Option<PendingPick<Vec<PathBuf>>>,
    select_new_folder: Option<PathBuf>,
    grid_hovered: bool,
    grid_rect: Option<Rect>,
    count_text: ((usize, usize), hp_app::i18n::Lang, String),
    favorites: (Vec<String>, HashSet<String>),
}

impl Default for TextureBrowser {
    fn default() -> Self {
        Self {
            search: String::new(),
            query: Query::default(),
            query_text: None,
            source: SourceFilter::All,
            category: Category::All,
            list: ListCache::default(),
            sources: SourcesCache::default(),
            filtered: Filtered::default(),
            filter_key: None,
            infos: HashMap::new(),
            keywords: KeywordIndex::default(),
            flags: FlagsIndex::default(),
            flags_wanted: false,
            reports: FolderReports::default(),
            used: (None, Vec::new()),
            marked: Vec::new(),
            anchor: None,
            scroll_offset: 0.0,
            viewport_height: 0.0,
            scroll_to: None,
            scroll_top: false,
            scroll_center: false,
            last_current: String::new(),
            layout: None,
            prefix_buf: HashMap::new(),
            pending_folder: None,
            pending_images: None,
            select_new_folder: None,
            grid_hovered: false,
            grid_rect: None,
            count_text: ((usize::MAX, 0), hp_app::i18n::Lang::En, String::new()),
            favorites: (Vec::new(), HashSet::new()),
        }
    }
}

impl TextureBrowser {
    pub fn new() -> Self {
        Self::default()
    }

    pub fn search(&self) -> &str {
        &self.search
    }

    pub fn set_search(&mut self, text: &str) {
        text.clone_into(&mut self.search);
        self.scroll_top = true;
    }

    pub fn source_filter(&self) -> SourceFilter {
        self.source
    }

    pub fn set_source_filter(&mut self, f: SourceFilter) {
        self.source = f;
        self.scroll_top = true;
    }

    pub fn category(&self) -> Category {
        self.category
    }

    pub fn set_category(&mut self, c: Category) {
        self.category = c;
        self.scroll_top = true;
    }

    pub fn shown_count(&self) -> usize {
        self.filtered.len()
    }

    pub fn shown_names(&self) -> Vec<String> {
        (0..self.filtered.len()).filter_map(|i| self.filtered.get(&self.list.list, i)).map(|e| e.name.clone()).collect()
    }

    pub fn marked(&self) -> &[String] {
        &self.marked
    }

    fn update_list(&mut self, state: &AppState) {
        let list = state.assets.materials();
        let key = (Arc::as_ptr(&list) as usize, list.len(), state.assets.generation());
        if self.list.key == Some(key) {
            return;
        }
        let mut counts: HashMap<u32, usize> = HashMap::new();
        for e in list.iter() {
            *counts.entry(e.source).or_default() += 1;
        }
        let list = if list.windows(2).all(|w| w[0].name <= w[1].name) {
            list
        } else {
            let mut v = (*list).clone();
            v.sort_by(|a, b| a.name.cmp(&b.name));
            Arc::new(v)
        };
        self.list = ListCache { key: Some(key), index: SearchIndex::build(&list), list, counts };
        self.infos.clear();
        self.filter_key = None;
    }

    fn update_sources(&mut self, state: &AppState) {
        let fs_gen = state.assets.fs().map_or(0, |f| f.generation());
        let key = (state.assets.generation(), fs_gen, self.list.key.map_or(0, |k| k.0));
        if self.sources.key == Some(key) {
            return;
        }
        let mut c = SourcesCache { key: Some(key), ..SourcesCache::default() };
        match state.assets.fs() {
            Some(fs) => {
                let mut model_counts: HashMap<u32, usize> = HashMap::new();
                for model in state.assets.models().iter() {
                    *model_counts.entry(model.source).or_default() += 1;
                }
                for s in fs.sources() {
                    let group = SourceGroup::of(s.origin);
                    c.table.insert(s.id, Some(group), &s.label);
                    let has_items = self.list.counts.get(&s.id).copied().unwrap_or(0) > 0;
                    match group {
                        SourceGroup::Game => c.has_game |= has_items,
                        SourceGroup::Addons => c.has_addons |= has_items,
                        SourceGroup::Workshop => c.has_workshop |= has_items,
                        SourceGroup::Custom => c.customs.push(CustomSource {
                            id: s.id,
                            path: s.path.clone(),
                            layout: fs.custom_dir_layout(s.id),
                            models: model_counts.get(&s.id).copied().unwrap_or(0),
                        }),
                    }
                }
            }
            None => {
                for &id in self.list.counts.keys() {
                    c.table.insert(id, None, &state.assets.source_label(id));
                }
            }
        }
        self.sources = c;
        self.filter_key = None;
        if let SourceFilter::Source(id) = self.source
            && !self.sources.customs.iter().any(|s| s.id == id)
        {
            self.source = SourceFilter::All;
        }
        if let Some(path) = &self.select_new_folder
            && let Some(cs) = self.sources.customs.iter().find(|c| same_path(&c.path, path))
        {
            self.source = SourceFilter::Source(cs.id);
            self.category = Category::All;
            self.select_new_folder = None;
            self.scroll_top = true;
        }
    }

    fn update_used(&mut self, state: &AppState) {
        if self.category != Category::Used {
            return;
        }
        let rev = state.doc.revision();
        if self.used.0 != Some(rev) {
            self.used = (Some(rev), crate::search::used_materials(&state.doc));
        }
    }

    fn members(&self, state: &AppState) -> Vec<String> {
        match self.category {
            Category::Used => self.used.1.clone(),
            Category::Favorites => {
                let mut v: Vec<String> =
                    state.settings.browser.favorites.iter().map(|f| crate::util::normalize_material(f)).collect();
                v.sort();
                v
            }
            Category::Recent => {
                state.settings.browser.recent_materials.iter().map(|f| crate::util::normalize_material(f)).collect()
            }
            Category::All | Category::Tools => Vec::new(),
        }
    }

    fn filter_hash(&self, state: &AppState) -> u64 {
        let mut h = DefaultHasher::new();
        state.settings.browser.hidden_shaders.hash(&mut h);
        if !state.settings.browser.hidden_shaders.is_empty() {
            self.flags.index().map(Arc::as_ptr).hash(&mut h);
        }
        self.list.key.hash(&mut h);
        self.sources.key.hash(&mut h);
        self.search.hash(&mut h);
        self.source.hash(&mut h);
        self.category.hash(&mut h);
        state.settings.browser.hide_tool_textures.hash(&mut h);
        match self.category {
            Category::Used => self.used.0.hash(&mut h),
            Category::Favorites => state.settings.browser.favorites.hash(&mut h),
            Category::Recent => {
                let mut set: Vec<&String> = state.settings.browser.recent_materials.iter().collect();
                set.sort_unstable();
                set.hash(&mut h);
            }
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
        excluded_folders(state).hash(&mut h);
        h.finish()
    }

    fn update_query(&mut self) {
        if self.query_text.as_deref() != Some(self.search.as_str()) {
            self.query = Query::parse(&self.search);
            self.query_text = Some(self.search.clone());
        }
    }

    fn update_filter(&mut self, state: &AppState) {
        self.update_query();
        let key = self.filter_hash(state);
        if self.filter_key == Some(key) {
            return;
        }
        let members = self.members(state);
        let excluded = excluded_folders(state);
        let params = FilterParams {
            query: &self.query,
            source: self.source,
            category: self.category,
            hide_tools: state.settings.browser.hide_tool_textures,
            members: &members,
            infos: &self.infos,
            keywords: self.keywords.index().map(|i| &**i),
            sources: &self.sources.table,
            flags: self.flags.index().map(|i| &**i),
            excluded: &excluded,
            hidden_shaders: &state.settings.browser.hidden_shaders,
        };
        self.filtered = filter_materials(&self.list.list, &self.list.index, &params);
        self.filter_key = Some(key);
    }

    fn update_keywords(&mut self, state: &AppState, ctx: &egui::Context) {
        if self.keywords.update(state, ctx, self.query.needs_info()) {
            self.filter_key = None;
        }
    }

    pub fn indexing_keywords(&self) -> bool {
        self.query.needs_info() && self.keywords.pending()
    }

    fn update_flags(&mut self, state: &AppState, ctx: &egui::Context) {
        let wanted = self.query.needs_flags() || self.flags_wanted || !state.settings.browser.hidden_shaders.is_empty();
        if self.flags.update(state, ctx, self.list.key, &self.list.list, wanted) {
            self.filter_key = None;
        }
    }

    pub fn indexing_flags(&self) -> bool {
        (self.query.needs_flags() || self.flags_wanted) && self.flags.pending()
    }

    fn shader_ui(&mut self, ui: &mut egui::Ui, state: &mut AppState) {
        ui.horizontal_wrapped(|ui| {
            ui.label(RichText::new(t!("tex.browser.shader")).small());
            let current = self.query.shaders.first().cloned();
            let text = current.clone().unwrap_or_else(|| t!("tex.browser.shader_all").to_owned());
            let mut chosen: Option<Option<String>> = None;
            let combo = egui::ComboBox::from_id_salt("hp_tex_shader_filter")
                .selected_text(RichText::new(text).small())
                .width(150.0)
                .height(360.0)
                .show_ui(ui, |ui| {
                    self.flags_wanted = true;
                    if ui.selectable_label(current.is_none(), t!("tex.browser.shader_all")).clicked() {
                        chosen = Some(None);
                    }
                    match self.flags.index() {
                        Some(index) => {
                            for (name, n) in index.shader_counts() {
                                let on = current.as_deref() == Some(name.as_str());
                                if ui.selectable_label(on, format!("{name}  ({n})")).clicked() {
                                    chosen = Some(Some(name));
                                }
                            }
                        }
                        None => {
                            ui.horizontal(|ui| {
                                ui.spinner();
                                ui.label(RichText::new(t!("tex.browser.flags_index")).small().weak());
                            });
                        }
                    }
                });
            combo.response.on_hover_text(t!("tex.browser.shader_tip"));
            if let Some(choice) = chosen {
                let mut words: Vec<String> = self
                    .search
                    .split_whitespace()
                    .filter(|w| !w.to_ascii_lowercase().starts_with("shader:"))
                    .map(str::to_owned)
                    .collect();
                if let Some(name) = choice {
                    words.push(format!("shader:{name}"));
                }
                self.search = words.join(" ");
                self.scroll_top = true;
            }
            let hidden = &mut state.settings.browser.hidden_shaders;
            let mut hide_models = hidden.iter().any(|s| s == MODEL_SHADER);
            let resp = ui.checkbox(&mut hide_models, RichText::new(t!("tex.browser.hide_vertexlit")).small());
            if resp.on_hover_text(t!("tex.browser.hide_vertexlit_tip")).changed() {
                hidden.retain(|s| s != MODEL_SHADER);
                if hide_models {
                    hidden.push(MODEL_SHADER.to_owned());
                }
                state.settings_dirty = true;
                self.filter_key = None;
            }
        });
    }

    fn update_reports(&mut self, state: &AppState, ctx: &egui::Context) {
        if let Some(rx) = &self.reports.pending {
            loop {
                match rx.try_recv() {
                    Ok((id, report)) => {
                        if let Some(r) = report {
                            self.reports.done.insert(id, r);
                        }
                    }
                    Err(TryRecvError::Empty) => break,
                    Err(TryRecvError::Disconnected) => {
                        self.reports.pending = None;
                        break;
                    }
                }
            }
        }
        let Some(fs) = state.assets.fs() else { return };
        let key = (state.assets.generation(), fs.generation());
        if self.reports.key == Some(key) {
            return;
        }
        self.reports = FolderReports { key: Some(key), ..FolderReports::default() };
        let ids: Vec<u32> = self.sources.customs.iter().map(|c| c.id).collect();
        if ids.is_empty() {
            return;
        }
        let fs = fs.clone();
        let (tx, rx) = std::sync::mpsc::channel();
        let repaint = ctx.clone();
        let spawned = std::thread::Builder::new().name("hp-folder-report".into()).spawn(move || {
            for id in ids {
                if tx.send((id, fs.custom_dir_report(id))).is_err() {
                    return;
                }
            }
            repaint.request_repaint();
        });
        match spawned {
            Ok(_) => self.reports.pending = Some(rx),
            Err(e) => log::warn!("cannot check the custom folders: {e}"),
        }
    }

    fn note_info(&mut self, state: &mut AppState, name: &str) -> Option<Arc<MaterialInfo>> {
        if let Some(i) = self.infos.get(name) {
            return Some(i.clone());
        }
        let info = state.assets.material_info(name)?;
        self.infos.insert(name.to_owned(), info.clone());
        Some(info)
    }

    fn set_marked(&mut self, ctx: &egui::Context, marked: Vec<String>) {
        if marked != self.marked {
            self.marked = marked;
            crate::shared::set_selection(ctx, self.marked.clone());
        }
    }

    pub fn ui(&mut self, ui: &mut egui::Ui, state: &mut AppState) {
        let ctx = ui.ctx().clone();
        if state.tool != ToolId::Texture && crate::shared::browse_for_faces(&ctx) {
            crate::shared::set_browse_for_faces(&ctx, false);
        }
        self.poll_pickers(&ctx, state);
        self.update_list(state);
        self.update_sources(state);
        self.update_used(state);
        self.update_query();
        self.update_keywords(state, &ctx);
        self.update_flags(state, &ctx);
        self.update_reports(state, &ctx);
        self.update_filter(state);

        if state.current_material != self.last_current {
            self.last_current = state.current_material.clone();
            self.scroll_to = self.filtered.position(&self.list.list, &self.last_current);
            self.scroll_center = self.scroll_to.is_some();
        }

        ui.spacing_mut().item_spacing = vec2(6.0, 4.0);
        self.pick_banner(ui, state);
        self.header_ui(ui, state);
        ui.separator();
        self.search_ui(ui, state);
        self.category_row(ui, state);
        self.filters_ui(ui, state);
        self.count_row(ui);
        self.folders_ui(ui, state);
        self.status_ui(ui, state);
        ui.separator();
        self.grid_ui(ui, state);
    }

    fn pick_banner(&mut self, ui: &mut egui::Ui, state: &mut AppState) {
        let Some(pick) = pending_material_pick(state) else { return };
        let label = pick.label.clone();
        egui::Frame::new()
            .fill(theme::accent_bg())
            .stroke(egui::Stroke::new(1.0, theme::accent()))
            .corner_radius(egui::CornerRadius::same(4))
            .inner_margin(egui::Margin::same(6))
            .show(ui, |ui| {
                ui.add(egui::Label::new(RichText::new(tf!("tex.pick.banner", label)).color(theme::accent())).wrap());
                ui.horizontal_wrapped(|ui| {
                    let current = state.current_material.clone();
                    let use_btn = ui.add_enabled(!current.is_empty(), egui::Button::new(t!("tex.pick.use")));
                    if use_btn.on_hover_text(current.as_str()).clicked() {
                        choose_for_pick(state, &current);
                    }
                    if ui.button(t!("common.cancel")).clicked() {
                        state.runtime.entities.asset_pick = None;
                        state.ui.right_tab = RightTab::Entity;
                    }
                });
            });
        ui.add_space(4.0);
    }

    fn poll_pickers(&mut self, ctx: &egui::Context, state: &mut AppState) {
        if let Some(p) = &self.pending_folder
            && let Some(result) = p.poll()
        {
            self.pending_folder = None;
            if let Some(path) = result {
                self.add_folder(state, &path);
            }
        }
        if let Some(p) = &self.pending_images
            && let Some(result) = p.poll()
        {
            self.pending_images = None;
            if let Some(files) = result
                && !files.is_empty()
            {
                crate::shared::post_import(ctx, files);
            }
        }
    }

    pub fn add_folder(&mut self, state: &mut AppState, path: &Path) {
        let added = crate::folders::add_custom_dir(&mut state.settings, path);
        state.settings_dirty = true;
        state.assets.remount(&state.settings);
        let prefix = state.settings.game.custom_texture_dirs.get(added.index()).map(|d| {
            let layout = hp_fs::custom_layout(&hp_fs::CustomDir {
                path: d.path.clone(),
                prefix: d.prefix.clone(),
                enabled: true,
                include_models: true,
            });
            if layout.prefix.is_empty() { "materials/".to_owned() } else { layout.prefix }
        });
        state.notify(Severity::Success, tf!("tex.folders.added", folder_label(path), prefix.unwrap_or_default()));
        self.select_new_folder = Some(path.to_path_buf());
    }

    fn header_ui(&mut self, ui: &mut egui::Ui, state: &mut AppState) {
        let name = state.current_material.clone();
        let in_list = crate::search::find_in_list(&self.list.list, &name);
        let missing = in_list.is_none() && !self.list.list.is_empty();
        let info = if name.is_empty() { None } else { self.note_info(state, &name) };
        let is_fav = state.settings.browser.favorites.iter().any(|f| f == &name);
        ui.horizontal(|ui| {
            let size = 76.0;
            let (rect, resp) = ui.allocate_exact_size(vec2(size, size), Sense::CLICK | Sense::DRAG);
            ui.painter().rect_filled(rect, 4.0, ui.visuals().extreme_bg_color);
            if !name.is_empty() {
                let thumb = state.assets.thumbnail(ui.ctx(), &name);
                paint_thumbnail(ui.painter(), rect.shrink(2.0), thumb, missing);
            }
            ui.painter().rect_stroke(rect, 4.0, egui::Stroke::new(1.0, theme::accent()), egui::StrokeKind::Inside);
            if resp.drag_started() && !name.is_empty() {
                resp.dnd_set_drag_payload(DragPayload::Material(name.clone()));
            }
            let resp = resp.on_hover_text(t!("tex.browser.drag_hint"));
            if resp.double_clicked() {
                apply_to_selection(state, &name);
            }
            ui.vertical(|ui| {
                ui.horizontal(|ui| {
                    let star_w = ui.spacing().interact_size.y + ui.spacing().item_spacing.x;
                    let name_w = (ui.available_width() - star_w).max(20.0);
                    ui.allocate_ui_with_layout(
                        vec2(name_w, ui.spacing().interact_size.y),
                        egui::Layout::left_to_right(egui::Align::Center),
                        |ui| {
                            ui.set_max_width(name_w);
                            ui.add(egui::Label::new(RichText::new(leaf_name(&name)).strong().size(14.0)).truncate())
                                .on_hover_text(&name);
                        },
                    );
                    let star = if is_fav { Icon::Star } else { Icon::StarOutline };
                    let tip = if is_fav { t!("tex.browser.unfavorite") } else { t!("tex.browser.favorite") };
                    if icon_button(ui, star, tip).clicked() && !name.is_empty() {
                        toggle_favorite(state, &name);
                        self.filter_key = None;
                    }
                });
                let folder = folder_of(&name);
                if !folder.is_empty() {
                    ui.add(egui::Label::new(RichText::new(folder).weak()).truncate());
                }
                let mut line = String::new();
                if let Some(i) = &info {
                    if i.width > 0 && i.height > 0 {
                        line.push_str(&format!("{} × {}", i.width, i.height));
                    }
                    if !i.shader.is_empty() {
                        if !line.is_empty() {
                            line.push_str("  ·  ");
                        }
                        line.push_str(&i.shader);
                    }
                }
                if missing {
                    ui.colored_label(theme::error(), t!("tex.browser.missing"));
                } else if !line.is_empty() {
                    ui.horizontal_wrapped(|ui| {
                        ui.label(RichText::new(line).small());
                        if let Some(image) = info.as_deref().and_then(|i| resized_image(state, i)) {
                            ui.label(RichText::new(t!("tex.browser.resized")).small().color(theme::warning()))
                                .on_hover_text(resized_text(&image));
                        }
                    });
                }
                if let Some(idx) = in_list
                    && let Some(e) = self.list.list.get(idx)
                {
                    let label = state.assets.source_label(e.source);
                    ui.add(egui::Label::new(RichText::new(label).small().weak()).truncate());
                }
            });
        });
        ui.horizontal_wrapped(|ui| {
            let has_sel = !state.doc.selection().is_empty();
            let enabled = !name.is_empty();
            let apply_sc = state
                .keys
                .shortcut(Action::ApplyCurrentTexture)
                .map(|s| hp_app::actions::shortcut_label(&s))
                .unwrap_or_default();
            if ui
                .add_enabled(enabled && has_sel, egui::Button::new(t!("tex.browser.apply")))
                .on_hover_text(format!("{} {}", t!("tex.browser.apply_tip"), apply_sc))
                .on_disabled_hover_text(t!("tex.browser.nothing_selected"))
                .clicked()
            {
                apply_to_selection(state, &name);
            }
            if ui
                .add_enabled(enabled, egui::Button::new(t!("tex.browser.replace")))
                .on_hover_text(t!("tex.browser.replace_tip"))
                .clicked()
            {
                crate::shared::post_replace(
                    ui.ctx(),
                    crate::shared::ReplacePrefill { find: name.clone(), replace: String::new() },
                );
                state.ui.show_replace_textures = true;
            }
            if ui
                .add_enabled(enabled, egui::Button::new(t!("tex.browser.edit")))
                .on_hover_text(t!("tex.browser.edit_tip"))
                .clicked()
            {
                let list = if self.marked.is_empty() { vec![name.clone()] } else { self.marked.clone() };
                crate::shared::post_editor(ui.ctx(), list);
                state.ui.show_material_editor = true;
            }
            if ui
                .add_enabled(enabled, egui::Button::new(t!("tex.browser.mark")))
                .on_hover_text(t!("tex.browser.mark_tip"))
                .clicked()
            {
                mark_material(state, &name);
            }
        });
    }

    fn search_ui(&mut self, ui: &mut egui::Ui, state: &mut AppState) {
        let id = egui::Id::unique("hp_texture_search");
        ui.horizontal(|ui| {
            let width = (ui.available_width() - 28.0).max(60.0);
            let r = ui.add(
                egui::TextEdit::singleline(&mut self.search)
                    .id(id)
                    .hint_text(t!("tex.browser.search_hint"))
                    .desired_width(width),
            );
            let r = r.on_hover_text(tf!("tex.browser.search_tip", hp_app::actions::key_label(egui::Key::Enter)));
            if state.ui.focus_texture_search {
                r.request_focus();
                state.ui.focus_texture_search = false;
            }
            if r.changed() {
                self.scroll_top = true;
            }
            if r.lost_focus()
                && ui.input(|i| i.key_pressed(egui::Key::Enter))
                && let Some(e) = self.filtered.get(&self.list.list, 0)
            {
                let name = e.name.clone();
                self.pick(ui.ctx(), state, &name);
                if pending_material_pick(state).is_some() {
                    choose_for_pick(state, &name);
                }
            }
            if icon_button(ui, Icon::Close, t!("common.clear")).clicked() && !self.search.is_empty() {
                self.search.clear();
                self.scroll_top = true;
            }
        });
    }

    fn source_text(&self, source: SourceFilter) -> String {
        match source {
            SourceFilter::All => t!("tex.browser.src_all").to_owned(),
            SourceFilter::Game => t!("tex.browser.src_game").to_owned(),
            SourceFilter::Addons => t!("tex.browser.src_addons").to_owned(),
            SourceFilter::Workshop => t!("tex.browser.src_workshop").to_owned(),
            SourceFilter::Source(id) => self
                .sources
                .customs
                .iter()
                .find(|c| c.id == id)
                .map_or_else(|| t!("tex.browser.src_all").to_owned(), |c| folder_label(&c.path)),
        }
    }

    fn category_row(&mut self, ui: &mut egui::Ui, state: &mut AppState) {
        let chip = |ui: &mut egui::Ui, selected: bool, text: &str| {
            ui.add(egui::Button::selectable(selected, RichText::new(text).small()))
        };
        let mut new_cat = None;
        let mut new_source = None;
        ui.horizontal_wrapped(|ui| {
            ui.spacing_mut().item_spacing.x = 3.0;
            let favorites = state.settings.browser.favorites.len();
            let cats = [
                (Category::All, t!("common.all"), "tex.browser.cat_all_tip"),
                (Category::Used, t!("tex.browser.cat_used"), "tex.browser.cat_used_tip"),
                (Category::Favorites, t!("tex.browser.cat_favorites_short"), "tex.browser.cat_favorites_tip"),
                (Category::Recent, t!("tex.browser.cat_recent"), "tex.browser.cat_recent_tip"),
                (Category::Tools, t!("tex.browser.cat_tools"), "tex.browser.cat_tools_tip"),
            ];
            for (cat, label, tip) in cats {
                let r = lazy_hover_text(chip(ui, self.category == cat, label), || {
                    if cat == Category::Favorites {
                        format!("{}\n{}", t!(tip), tf!("tex.browser.cat_favorites", favorites))
                    } else {
                        t!(tip).to_owned()
                    }
                });
                if r.clicked() {
                    new_cat = Some(cat);
                }
            }
            let sources = [
                (SourceFilter::Game, self.sources.has_game),
                (SourceFilter::Addons, self.sources.has_addons),
                (SourceFilter::Workshop, self.sources.has_workshop),
            ];
            let any_source = sources.iter().any(|(_, has)| *has) || !self.sources.customs.is_empty();
            if !any_source && self.source == SourceFilter::All {
                return;
            }
            ui.separator();
            let current = self.source_text(self.source);
            egui::ComboBox::from_id_salt("hp_tex_source_filter")
                .selected_text(RichText::new(current).small())
                .width(110.0)
                .truncate()
                .height(360.0)
                .show_ui(ui, |ui| {
                    if ui.selectable_label(self.source == SourceFilter::All, t!("tex.browser.src_all")).clicked() {
                        new_source = Some(SourceFilter::All);
                    }
                    for (src, has) in sources {
                        if (has || self.source == src)
                            && ui.selectable_label(self.source == src, self.source_text(src)).clicked()
                        {
                            new_source = Some(src);
                        }
                    }
                    for cs in &self.sources.customs {
                        let count = self.list.counts.get(&cs.id).copied().unwrap_or(0);
                        let on = self.source == SourceFilter::Source(cs.id);
                        let item = ui.selectable_label(on, folder_label(&cs.path));
                        if lazy_hover_text(item, || tf!("tex.browser.src_custom_tip", cs.path.display(), count))
                            .clicked()
                        {
                            new_source = Some(SourceFilter::Source(cs.id));
                        }
                    }
                })
                .response
                .on_hover_text(t!("tex.browser.source_tip"));
        });
        if let Some(c) = new_cat {
            self.set_category(c);
        }
        if let Some(s) = new_source {
            self.set_source_filter(s);
        }
    }

    fn active_filters(&self, state: &AppState) -> usize {
        self.query.flags.len()
            + usize::from(!self.query.shaders.is_empty())
            + usize::from(!state.settings.browser.hidden_shaders.is_empty())
            + usize::from(state.settings.browser.hide_tool_textures)
    }

    fn filters_ui(&mut self, ui: &mut egui::Ui, state: &mut AppState) {
        let active = self.active_filters(state);
        let title = if active == 0 {
            RichText::new(t!("tex.browser.filters")).strong()
        } else {
            RichText::new(tf!("tex.browser.filters_active", active)).strong().color(theme::accent())
        };
        egui::CollapsingHeader::new(title)
            .id_salt("hp_tex_filters")
            .default_open(false)
            .show(ui, |ui| {
                let mut toggled = None;
                ui.horizontal_wrapped(|ui| {
                    ui.spacing_mut().item_spacing.x = 3.0;
                    for flag in MatFlag::ALL {
                        let on = self.query.flags.contains(&flag);
                        let r = ui.add(egui::Button::selectable(on, RichText::new(flag_label(flag)).small()));
                        let r = lazy_hover_text(r, || tf!("tex.browser.flag_tip", flag.word()));
                        if r.clicked() {
                            toggled = Some(flag);
                        }
                    }
                });
                if let Some(flag) = toggled {
                    self.search = toggle_flag_word(&self.search, flag);
                    self.scroll_top = true;
                }
                self.shader_ui(ui, state);
                self.display_ui(ui, state);
            })
            .header_response
            .on_hover_text(t!("tex.browser.filters_tip"));
    }

    fn display_ui(&mut self, ui: &mut egui::Ui, state: &mut AppState) {
        ui.horizontal_wrapped(|ui| {
            let mut hide = state.settings.browser.hide_tool_textures;
            if ui
                .checkbox(&mut hide, RichText::new(t!("tex.browser.hide_tools")).small())
                .on_hover_text(t!("tex.browser.hide_tools_tip"))
                .changed()
            {
                state.settings.browser.hide_tool_textures = hide;
                state.settings_dirty = true;
            }
            let mut alpha = state.settings.browser.show_alpha;
            let resp = ui.checkbox(&mut alpha, RichText::new(t!("tex.browser.show_alpha")).small());
            if resp.on_hover_text(t!("tex.browser.show_alpha_tip")).changed() {
                state.settings.browser.show_alpha = alpha;
                state.settings_dirty = true;
            }
            let mut size = state.settings.ui.thumbnail_size.clamp(48.0, 256.0);
            let side = ui.spacing().interact_size.y;
            let slider_w = ui.spacing().slider_width;
            let unit = vec2(side + ui.spacing().item_spacing.x + slider_w, side);
            let layout = egui::Layout::left_to_right(egui::Align::Center);
            let changed = ui
                .allocate_ui_with_layout(unit, layout, |ui| {
                    let (icon_rect, _) = ui.allocate_exact_size(Vec2::splat(side), Sense::hover());
                    paint_icon(ui.painter(), icon_rect.shrink(3.0), Icon::Image, ui.visuals().weak_text_color());
                    ui.add(egui::Slider::new(&mut size, 48.0..=256.0).show_value(false).step_by(8.0))
                        .on_hover_text(t!("tex.browser.size"))
                        .changed()
                })
                .inner;
            if changed {
                state.settings.ui.thumbnail_size = size;
                state.settings_dirty = true;
            }
        });
    }

    fn count_row(&mut self, ui: &mut egui::Ui) {
        let counts = (self.filtered.len(), self.list.list.len());
        if self.count_text.0 != counts || self.count_text.1 != hp_app::i18n::lang() {
            self.count_text = (counts, hp_app::i18n::lang(), tf!("tex.browser.count", counts.0, counts.1));
        }
        ui.horizontal_wrapped(|ui| {
            ui.label(RichText::new(self.count_text.2.as_str()).small().weak());
            if self.indexing_keywords() {
                ui.spinner();
                ui.label(RichText::new(t!("tex.browser.kw_index")).small().weak());
            }
            if self.indexing_flags() {
                ui.spinner();
                ui.label(RichText::new(t!("tex.browser.flags_index")).small().weak());
            }
            if !self.marked.is_empty() {
                ui.label(
                    RichText::new(tf!("tex.browser.marked_count", self.marked.len())).small().color(theme::accent()),
                );
                if icon_button(ui, Icon::Close, t!("tex.browser.clear_marks")).clicked() {
                    let ctx = ui.ctx().clone();
                    self.set_marked(&ctx, Vec::new());
                }
            }
        });
    }

    fn folders_ui(&mut self, ui: &mut egui::Ui, state: &mut AppState) {
        let n = state.settings.game.custom_texture_dirs.len();
        let mut action: Option<FolderAction> = None;
        egui::CollapsingHeader::new(RichText::new(tf!("tex.folders.title", n)).strong())
            .id_salt("hp_tex_custom_folders_closed")
            .default_open(false)
            .show(ui, |ui| {
                if n == 0 {
                    ui.label(RichText::new(t!("tex.folders.empty")).small().weak());
                }
                for i in 0..n {
                    let Some(dir) = state.settings.game.custom_texture_dirs.get(i) else { continue };
                    let mounted = self.sources.customs.iter().find(|c| same_path(&c.path, &dir.path)).cloned();
                    let count = mounted.as_ref().map(|m| self.mounted_count(m.id));
                    ui.push_id(("hp_custom_dir", i), |ui| {
                        ui.horizontal(|ui| {
                            let mut enabled = dir.enabled;
                            if ui.checkbox(&mut enabled, "").on_hover_text(t!("tex.folders.enable_tip")).changed() {
                                action = Some(FolderAction::Enable(i, enabled));
                            }
                            ui.with_layout(egui::Layout::right_to_left(egui::Align::Center), |ui| {
                                if icon_button(ui, Icon::Trash, t!("tex.folders.remove")).clicked() {
                                    action = Some(FolderAction::Remove(i));
                                }
                                if icon_button(ui, Icon::Folder, t!("tex.folders.open")).clicked() {
                                    action = Some(FolderAction::Reveal(dir.path.clone()));
                                }
                                if let Some(m) = &mounted {
                                    let selected = self.source == SourceFilter::Source(m.id);
                                    if icon_toggle(ui, Icon::Filter, selected, t!("tex.folders.show_only")).clicked() {
                                        action = Some(FolderAction::ShowOnly(m.id));
                                    }
                                }
                                let count_text = folder_count_text(dir.enabled, count);
                                ui.label(RichText::new(count_text).small().weak());
                                ui.with_layout(egui::Layout::left_to_right(egui::Align::Center), |ui| {
                                    let name = RichText::new(folder_label(&dir.path)).strong();
                                    let name = if dir.enabled { name } else { name.weak() };
                                    ui.add(egui::Label::new(name).truncate())
                                        .on_hover_text(dir.path.display().to_string());
                                });
                            });
                        });
                        ui.horizontal(|ui| {
                            ui.add_space(24.0);
                            let mut include_models = dir.include_models;
                            if ui
                                .checkbox(&mut include_models, t!("common.load_models"))
                                .on_hover_text(t!("common.load_models_tip"))
                                .changed()
                            {
                                action = Some(FolderAction::Models(i, include_models));
                            }
                            if let Some(source) = &mounted {
                                ui.label(RichText::new(tf!("tex.folders.models", source.models)).small().weak());
                            }
                        });
                        ui.horizontal(|ui| {
                            ui.add_space(24.0);
                            let layout = mounted.as_ref().and_then(|m| m.layout.clone());
                            let fixed = layout
                                .clone()
                                .filter(|l| matches!(l.mode, CustomMode::GameFolder | CustomMode::InsideMaterials));
                            let detected = layout.filter(|l| l.mode == CustomMode::Detected).map(|l| l.prefix);
                            let shown_prefix = detected.clone().unwrap_or_else(|| dir.prefix.clone());
                            ui.label(RichText::new(t!("tex.folders.prefix")).small());
                            match fixed {
                                Some(layout) => {
                                    let shown = if layout.prefix.is_empty() {
                                        "materials/".to_owned()
                                    } else {
                                        layout.prefix.clone()
                                    };
                                    let tip = match layout.mode {
                                        CustomMode::GameFolder => t!("tex.folders.mode_game"),
                                        _ => t!("tex.folders.mode_inside"),
                                    };
                                    ui.label(RichText::new(shown).small().monospace()).on_hover_text(tip);
                                }
                                None => {
                                    let buf =
                                        self.prefix_buf.entry(dir.path.clone()).or_insert_with(|| shown_prefix.clone());
                                    let id = ui.make_persistent_id("prefix");
                                    let focused = ui.memory(|m| m.has_focus(id));
                                    if !focused && *buf != shown_prefix {
                                        buf.clone_from(&shown_prefix);
                                    }
                                    let width = if detected.is_some() { 160.0 } else { 220.0 };
                                    let r = ui.add(
                                        egui::TextEdit::singleline(buf)
                                            .id(id)
                                            .font(egui::TextStyle::Monospace)
                                            .desired_width(ui.available_width().min(width)),
                                    );
                                    let enter = hp_app::actions::key_label(egui::Key::Enter);
                                    let r = r.on_hover_text(tf!("tex.folders.prefix_tip", enter));
                                    if r.lost_focus() && *buf != shown_prefix {
                                        action = Some(FolderAction::Prefix(i, buf.clone()));
                                    }
                                    if detected.is_some() {
                                        ui.label(
                                            RichText::new(t!("tex.folders.detected")).small().color(theme::accent()),
                                        )
                                        .on_hover_text(t!("tex.folders.detected_tip"));
                                    }
                                }
                            }
                        });
                        if let Some(report) = mounted.as_ref().and_then(|m| self.reports.done.get(&m.id)) {
                            folder_report_ui(ui, report);
                        }
                    });
                }
                ui.horizontal_wrapped(|ui| {
                    let picking = self.pending_folder.is_some();
                    if icon_text_button(ui, Icon::Plus, t!("tex.folders.add"), !picking)
                        .on_hover_text(t!("tex.folders.add_tip"))
                        .clicked()
                    {
                        let title = t!("tex.folders.pick_title").to_owned();
                        self.pending_folder = Some(PendingPick::spawn(ui.ctx(), move || {
                            rfd::FileDialog::new().set_title(title).pick_folder()
                        }));
                    }
                    if icon_text_button(ui, Icon::Refresh, t!("tex.folders.rescan"), true)
                        .on_hover_text(t!("tex.folders.rescan_tip"))
                        .clicked()
                    {
                        state.assets.remount(&state.settings);
                        state.notify(Severity::Info, t!("tex.folders.rescanning"));
                    }
                    let picking = self.pending_images.is_some();
                    if icon_text_button(ui, Icon::Import, t!("tex.import.button"), !picking)
                        .on_hover_text(t!("tex.import.button_tip"))
                        .clicked()
                    {
                        let title = t!("tex.import.pick_title").to_owned();
                        let filter = t!("tex.import.filter").to_owned();
                        self.pending_images = Some(PendingPick::spawn(ui.ctx(), move || {
                            rfd::FileDialog::new()
                                .set_title(title)
                                .add_filter(filter, crate::util::IMPORT_EXTENSIONS)
                                .pick_files()
                        }));
                    }
                });
            });
        if let Some(a) = action {
            self.apply_folder_action(state, a);
        }
    }

    fn mounted_count(&self, source: u32) -> usize {
        self.list.counts.get(&source).copied().unwrap_or(0)
    }

    fn apply_folder_action(&mut self, state: &mut AppState, action: FolderAction) {
        match action {
            FolderAction::Models(i, on) => {
                if let Some(dir) = state.settings.game.custom_texture_dirs.get_mut(i)
                    && dir.include_models != on
                {
                    dir.include_models = on;
                    state.settings_dirty = true;
                    state.assets.remount(&state.settings);
                }
            }
            FolderAction::Enable(i, on) => {
                if crate::folders::set_custom_enabled(&mut state.settings, i, on) {
                    state.settings_dirty = true;
                    if let Some(dir) = state.settings.game.custom_texture_dirs.get(i)
                        && let Some(cs) = self.sources.customs.iter().find(|c| same_path(&c.path, &dir.path))
                        && let Some(fs) = state.assets.fs()
                    {
                        fs.set_source_enabled(cs.id, on);
                    }
                    state.assets.remount(&state.settings);
                }
            }
            FolderAction::Prefix(i, prefix) => {
                if crate::folders::set_custom_prefix(&mut state.settings, i, &prefix) {
                    state.settings_dirty = true;
                    state.assets.remount(&state.settings);
                }
            }
            FolderAction::Remove(i) => {
                if let Some(removed) = crate::folders::remove_custom_dir(&mut state.settings, i) {
                    state.settings_dirty = true;
                    if let Some(cs) = self.sources.customs.iter().find(|c| same_path(&c.path, &removed.path))
                        && self.source == SourceFilter::Source(cs.id)
                    {
                        self.source = SourceFilter::All;
                    }
                    self.prefix_buf.remove(&removed.path);
                    state.assets.remount(&state.settings);
                    state.notify(Severity::Info, tf!("tex.folders.removed", folder_label(&removed.path)));
                }
            }
            FolderAction::ShowOnly(id) => {
                let f =
                    if self.source == SourceFilter::Source(id) { SourceFilter::All } else { SourceFilter::Source(id) };
                self.set_source_filter(f);
                self.category = Category::All;
            }
            FolderAction::Reveal(path) => crate::util::reveal_or_log(state, &path),
        }
    }

    fn status_ui(&mut self, ui: &mut egui::Ui, state: &mut AppState) {
        match state.assets.status() {
            AssetStatus::Ready => {
                let late = state.assets.late_mount();
                if late > 0 {
                    ui.horizontal(|ui| {
                        ui.spinner();
                        ui.label(RichText::new(tf!("tex.browser.loading_late", late)).small());
                    });
                }
            }
            AssetStatus::Mounting => {
                ui.horizontal(|ui| {
                    ui.spinner();
                    ui.label(RichText::new(t!("status.loading_assets")).small());
                });
            }
            AssetStatus::NotConfigured => {
                ui.colored_label(theme::warning(), t!("tex.browser.not_configured"));
                if ui.button(t!("action.settings")).clicked() {
                    state.request(Action::Settings);
                }
            }
            AssetStatus::Failed(msg) => {
                ui.colored_label(theme::error(), tf!("tex.browser.mount_failed", msg));
            }
        }
    }

    fn pick(&mut self, ctx: &egui::Context, state: &mut AppState, name: &str) {
        state.use_material(name);
        self.last_current = state.current_material.clone();
        if crate::shared::browse_for_faces(ctx) && state.tool == ToolId::Texture {
            crate::shared::set_browse_for_faces(ctx, false);
            let faces = state.doc.selected_faces();
            if !faces.is_empty() {
                match state.doc.apply_material(&faces, name) {
                    Ok(n) => state.notify(Severity::Success, tf!("tex.browser.applied", n)),
                    Err(e) => state.notify(Severity::Error, tf!("tex.browser.apply_failed", e)),
                }
            }
            state.ui.right_tab = RightTab::Tool;
        }
    }

    fn grid_ui(&mut self, ui: &mut egui::Ui, state: &mut AppState) {
        let ctx = ui.ctx().clone();
        self.grid_hovered = false;
        if self.filtered.is_empty() {
            ui.add_space(16.0);
            ui.vertical_centered(|ui| {
                if self.list.list.is_empty() {
                    ui.weak(t!("tex.browser.no_materials"));
                } else {
                    ui.weak(t!("tex.browser.no_match"));
                    if ui.button(t!("tex.browser.clear_filters")).clicked() {
                        self.search.clear();
                        self.source = SourceFilter::All;
                        self.category = Category::All;
                    }
                }
            });
            return;
        }
        let thumb = state.settings.ui.thumbnail_size.clamp(48.0, 256.0);
        let width = ui.available_width();
        let layout = GridLayout::new(width, thumb, LABEL_HEIGHT, GRID_SPACING, self.filtered.len());
        self.layout = Some(layout);

        let mut sa = egui::ScrollArea::vertical().id_salt("hp_texture_grid").auto_shrink([false, false]);
        let viewport = if self.viewport_height > 0.0 { self.viewport_height } else { ui.available_height() };
        if self.scroll_top {
            sa = sa.vertical_scroll_offset(0.0);
            self.scroll_top = false;
            self.scroll_to = None;
        } else if let Some(idx) = self.scroll_to.take()
            && let Some(off) = layout.scroll_to_show(idx, self.scroll_offset, viewport)
        {
            let off = if std::mem::take(&mut self.scroll_center) { layout.center_offset(idx, viewport) } else { off };
            sa = sa.vertical_scroll_offset(off);
        }

        let mut actions: Vec<CellAction> = Vec::new();
        let mut any_loading = false;
        let list = self.list.list.clone();
        if self.favorites.0 != state.settings.browser.favorites {
            self.favorites.0.clone_from(&state.settings.browser.favorites);
            self.favorites.1 = self.favorites.0.iter().cloned().collect();
        }
        let favorites = std::mem::take(&mut self.favorites.1);
        let marked: HashSet<&str> = self.marked.iter().map(String::as_str).collect();
        let mut visible_names: Vec<String> = Vec::new();
        let show_alpha = state.settings.browser.show_alpha;

        let out = sa.show_viewport(ui, |ui, viewport| {
            ui.set_height(layout.content_height());
            ui.set_width(width);
            let origin = ui.max_rect().min;
            let font = egui::FontId::proportional(11.0);
            for row in layout.visible_rows(viewport.min.y, viewport.max.y) {
                for index in layout.row_items(row) {
                    let Some(entry) = self.filtered.get(&list, index) else { continue };
                    let (x, y) = layout.cell_offset(index);
                    let rect = Rect::from_min_size(origin + vec2(x, y), vec2(layout.cell_width, layout.cell_height));
                    let id = egui::Id::unique(("hp_tex_cell", entry.name.as_str()));
                    let resp = ui.interact(rect, id, Sense::CLICK | Sense::DRAG);
                    let missing = entry.source == MISSING_SOURCE;
                    let is_current = entry.name == state.current_material;
                    let is_marked = marked.contains(entry.name.as_str());
                    let painter = ui.painter_at(rect.expand(1.0));
                    if is_current {
                        painter.rect_filled(rect, 4.0, theme::accent_bg());
                        painter.rect_stroke(
                            rect,
                            4.0,
                            egui::Stroke::new(1.5, theme::accent()),
                            egui::StrokeKind::Inside,
                        );
                    } else if is_marked {
                        painter.rect_filled(rect, 4.0, theme::palette().bg4);
                        painter.rect_stroke(
                            rect,
                            4.0,
                            egui::Stroke::new(1.0, theme::text_weak()),
                            egui::StrokeKind::Inside,
                        );
                    } else if resp.hovered() {
                        painter.rect_filled(rect, 4.0, ui.visuals().widgets.hovered.bg_fill);
                    }
                    let thumb_rect = Rect::from_center_size(
                        pos2(rect.center().x, rect.top() + CELL_PADDING + thumb * 0.5),
                        Vec2::splat(thumb.min(layout.cell_width - 2.0 * CELL_PADDING)),
                    );
                    let state_thumb =
                        if missing { ThumbState::Missing } else { state.assets.thumbnail(&ctx, &entry.name) };
                    any_loading |= state_thumb == ThumbState::Loading;
                    let alpha = show_alpha && self.infos.get(&entry.name).is_some_and(|i| i.shows_alpha);
                    paint_thumbnail_alpha(&painter, thumb_rect, state_thumb, missing, alpha);
                    if self.infos.get(&entry.name).is_some_and(|i| i.is_blend) {
                        let text = t!("tex.browser.blend_badge");
                        let galley =
                            painter.layout_no_wrap(text.to_owned(), egui::FontId::proportional(9.5), Color32::WHITE);
                        let pill = Rect::from_min_size(
                            thumb_rect.left_bottom() + vec2(2.0, -galley.size().y - 5.0),
                            galley.size() + vec2(8.0, 3.0),
                        );
                        painter.rect_filled(pill, 3.0, Color32::from_rgba_unmultiplied(120, 60, 170, 225));
                        painter.galley(pill.min + vec2(4.0, 1.5), galley, Color32::WHITE);
                    }
                    if favorites.contains(&entry.name) {
                        let star = Rect::from_min_size(thumb_rect.right_top() + vec2(-14.0, 2.0), Vec2::splat(12.0));
                        painter.circle_filled(star.center(), 7.5, Color32::from_black_alpha(160));
                        crate::icons::paint_star(&painter, star.shrink(1.0), theme::accent());
                    }
                    let label_rect = Rect::from_min_max(
                        pos2(rect.left() + 2.0, rect.bottom() - LABEL_HEIGHT),
                        pos2(rect.right() - 2.0, rect.bottom()),
                    );
                    let color = if missing {
                        theme::error()
                    } else if is_current {
                        ui.visuals().strong_text_color()
                    } else {
                        ui.visuals().text_color()
                    };
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
                    if !self.infos.contains_key(&entry.name) {
                        visible_names.push(entry.name.clone());
                    }

                    if resp.drag_started() {
                        resp.dnd_set_drag_payload(DragPayload::Material(entry.name.clone()));
                    }
                    if resp.dragged() {
                        paint_drag_preview(&ctx, state_thumb, &entry.name, missing);
                    }
                    if resp.clicked() {
                        let m = ui.input(|i| i.modifiers);
                        actions.push(CellAction::Click {
                            index,
                            name: entry.name.clone(),
                            ctrl: m.command || m.ctrl,
                            shift: m.shift,
                        });
                    }
                    if resp.double_clicked() {
                        actions.push(CellAction::Apply(entry.name.clone()));
                    }
                    let resp =
                        if resp.dragged() { resp } else { resp.on_hover_ui(|ui| cell_tooltip(ui, state, entry)) };
                    resp.context_menu(|ui| {
                        cell_menu(ui, state, entry, favorites.contains(&entry.name), &mut actions);
                    });
                }
            }
        });
        self.favorites.1 = favorites;
        self.scroll_offset = out.state.clamped_offset().y;
        self.viewport_height = out.inner_rect.height();
        self.grid_rect = Some(out.inner_rect);
        self.grid_hovered = ui.rect_contains_pointer(out.inner_rect);

        for name in &visible_names {
            self.note_info(state, name);
        }
        if any_loading {
            ctx.request_repaint_after(Duration::from_millis(100));
        }
        for a in actions {
            self.run_action(&ctx, state, a);
        }
        if self.grid_hovered && !ctx.text_edit_focused() && !egui::Popup::is_any_open(&ctx) {
            let keys = ctx.input_mut(|i| take_grid_keys(&mut i.events));
            for key in keys {
                self.grid_key(&ctx, state, layout, key);
            }
        }
    }

    pub fn grid_has_keyboard(&self) -> bool {
        self.grid_hovered
    }

    pub fn grid_rect(&self) -> Option<Rect> {
        self.grid_rect
    }

    pub fn folder_report(&self, source: u32) -> Option<&CustomReport> {
        self.reports.done.get(&source)
    }

    pub fn checking_folders(&self) -> bool {
        self.reports.pending.is_some()
    }

    pub fn grid_key(&mut self, ctx: &egui::Context, state: &mut AppState, layout: GridLayout, key: GridKey) {
        if self.filtered.is_empty() {
            return;
        }
        let last = self.filtered.len() - 1;
        let from = self.filtered.position(&self.list.list, &state.current_material);
        let page_rows = ((self.viewport_height / layout.row_stride().max(1.0)).floor() as isize).max(1);
        let (to, extend) = match key {
            GridKey::Apply => {
                let name = state.current_material.clone();
                apply_to_selection(state, &name);
                return;
            }
            GridKey::Move { dx, dy, extend } => (from.map_or(0, |f| layout.step(f, dx, dy)), extend),
            GridKey::Page { dir, extend } => (from.map_or(0, |f| layout.step(f, 0, dir * page_rows)), extend),
            GridKey::Edge { end, extend } => (if end { last } else { 0 }, extend),
        };
        let Some(name) = self.filtered.get(&self.list.list, to).map(|e| e.name.clone()) else { return };
        if extend {
            let anchor = self.anchor.or(from).unwrap_or(to);
            self.anchor = Some(anchor);
            let (a, b) = if anchor <= to { (anchor, to) } else { (to, anchor) };
            let names: Vec<String> =
                (a..=b).filter_map(|i| self.filtered.get(&self.list.list, i)).map(|e| e.name.clone()).collect();
            self.set_marked(ctx, names);
        } else {
            self.set_marked(ctx, Vec::new());
            self.anchor = Some(to);
        }
        self.pick(ctx, state, &name);
        self.scroll_to = Some(to);
    }

    fn run_action(&mut self, ctx: &egui::Context, state: &mut AppState, action: CellAction) {
        match action {
            CellAction::Click { index, name, ctrl, shift } => {
                if shift {
                    let anchor = self.anchor.unwrap_or(index);
                    let (a, b) = if anchor <= index { (anchor, index) } else { (index, anchor) };
                    let names: Vec<String> =
                        (a..=b).filter_map(|i| self.filtered.get(&self.list.list, i)).map(|e| e.name.clone()).collect();
                    self.set_marked(ctx, names);
                } else if ctrl {
                    let mut m = self.marked.clone();
                    if m.is_empty() && !state.current_material.is_empty() && state.current_material != name {
                        m.push(state.current_material.clone());
                    }
                    match m.iter().position(|x| x == &name) {
                        Some(i) => {
                            m.remove(i);
                        }
                        None => m.push(name.clone()),
                    }
                    self.set_marked(ctx, m);
                    self.anchor = Some(index);
                } else {
                    self.set_marked(ctx, Vec::new());
                    self.anchor = Some(index);
                    self.pick(ctx, state, &name);
                }
            }
            CellAction::Apply(name) => {
                state.use_material(&name);
                self.last_current = state.current_material.clone();
                if pending_material_pick(state).is_some() {
                    choose_for_pick(state, &name);
                } else {
                    apply_to_selection(state, &name);
                }
            }
            CellAction::Exclude(folder) => {
                let list = &mut state.settings.browser.excluded_folders;
                if !list.iter().any(|f| f.eq_ignore_ascii_case(&folder)) {
                    list.push(folder.clone());
                    state.settings_dirty = true;
                    state.notify(Severity::Info, tf!("tex.browser.excluded", folder));
                }
            }
            CellAction::Mark(name) => {
                mark_material(state, &name);
            }
            CellAction::Favorite(name) => {
                toggle_favorite(state, &name);
            }
            CellAction::Copy(name) => ctx.copy_text(name),
            CellAction::ReplaceWith(name) => {
                let find = if state.current_material != name { state.current_material.clone() } else { String::new() };
                crate::shared::post_replace(ctx, crate::shared::ReplacePrefill { find, replace: name });
                state.ui.show_replace_textures = true;
            }
            CellAction::Reveal(path) => crate::util::reveal_or_log(state, &path),
            CellAction::Shader(name) => {
                state.runtime.shader.open(hp_app::runtime::ShaderRequest { source: Some(name), ..Default::default() });
            }
            CellAction::Vtf(name) => match state.assets.material_info(&name).and_then(|i| i.base_texture.clone()) {
                Some(texture) => state.runtime.vtf.open(hp_app::runtime::VtfRequest {
                    texture: Some(texture),
                    file: None,
                    external: Some(false),
                    materials: vec![name],
                    param: Some("$basetexture".to_owned()),
                }),
                None => state.notify(Severity::Warning, tf!("tex.menu.vtf_none", name)),
            },
            CellAction::Edit(name) => {
                let list = if self.marked.contains(&name) { self.marked.clone() } else { vec![name] };
                crate::shared::post_editor(ctx, list);
                state.ui.show_material_editor = true;
            }
        }
    }
}

fn folder_count_text(enabled: bool, count: Option<usize>) -> String {
    match count {
        _ if !enabled => t!("common.disabled").to_owned(),
        Some(c) => tf!("tex.folders.count", c),
        None => t!("tex.folders.not_mounted").to_owned(),
    }
}

fn paint_drag_preview(ctx: &egui::Context, thumb: ThumbState, name: &str, missing: bool) {
    let Some(pos) = ctx.pointer_interact_pos() else { return };
    let painter = ctx.layer_painter(egui::LayerId::new(egui::Order::Tooltip, egui::Id::unique("hp_tex_drag_preview")));
    let rect = Rect::from_min_size(pos + vec2(12.0, 12.0), Vec2::splat(64.0));
    painter.rect_filled(rect.expand(3.0), 4.0, Color32::from_black_alpha(200));
    paint_thumbnail(&painter, rect, thumb, missing);
    painter.text(
        Pos2::new(rect.left(), rect.bottom() + 4.0),
        egui::Align2::LEFT_TOP,
        leaf_name(name),
        egui::FontId::proportional(11.0),
        Color32::WHITE,
    );
}

fn cell_tooltip(ui: &mut egui::Ui, state: &mut AppState, entry: &AssetEntry) {
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
            if let Some(image) = resized_image(state, &info) {
                ui.colored_label(theme::warning(), resized_text(&image));
            }
            if !info.shader.is_empty() {
                ui.label(tf!("tex.tip.shader", info.shader));
            }
            if let Some(sp) = &info.surface_prop {
                ui.label(tf!("tex.tip.surfaceprop", sp));
            }
            if !info.keywords.is_empty() {
                ui.label(tf!("tex.tip.keywords", info.keywords.join(", ")));
            }
            let mut flags = Vec::new();
            if info.tool.is_some() {
                flags.push(t!("tex.tip.tool"));
            }
            if info.translucent {
                flags.push(t!("tex.me.p.translucent"));
            }
            if info.alpha_test {
                flags.push(t!("tex.me.p.alphatest"));
            }
            if info.is_water {
                flags.push(t!("tex.tip.water"));
            }
            if info.is_blend {
                flags.push(t!("tex.tip.blend"));
            }
            if !flags.is_empty() {
                ui.label(RichText::new(flags.join(", ")).small());
            }
        }
        None => {
            ui.weak(t!("common.loading"));
        }
    }
    ui.label(RichText::new(tf!("tex.tip.source", state.assets.source_label(entry.source))).small().weak());
    ui.label(RichText::new(t!("tex.tip.hint")).small().weak());
}

fn resized_image(state: &AppState, info: &MaterialInfo) -> Option<hp_fs::CustomImageInfo> {
    let texture = info.base_texture.as_deref().unwrap_or(&info.name);
    state.assets.fs()?.custom_image(texture).filter(hp_fs::CustomImageInfo::is_resized)
}

fn resized_text(image: &hp_fs::CustomImageInfo) -> String {
    tf!("tex.tip.resized", image.width, image.height, image.game_width, image.game_height)
}

fn folder_report_ui(ui: &mut egui::Ui, report: &CustomReport) {
    const SHOWN: usize = 12;
    let list = |items: Vec<String>, total: usize| {
        let mut text = items.join("\n");
        if total > SHOWN {
            text.push_str(&format!("\n... (+{})", total - SHOWN));
        }
        text
    };
    if !report.missing_textures.is_empty() {
        let n = report.missing_textures.len();
        let items =
            report.missing_textures.iter().take(SHOWN).map(|(m, tex)| format!("{m}  >  {tex}")).collect::<Vec<_>>();
        let tip = format!("{}\n\n{}", t!("tex.folders.missing_textures_tip"), list(items, n));
        ui.horizontal(|ui| {
            ui.add_space(24.0);
            ui.add(egui::Label::new(
                RichText::new(tf!("tex.folders.missing_textures", n)).small().color(theme::warning()),
            ))
            .on_hover_text(tip);
        });
    }
    if !report.unsafe_names.is_empty() {
        let n = report.unsafe_names.len();
        let items = report.unsafe_names.iter().take(SHOWN).cloned().collect::<Vec<_>>();
        let tip = format!("{}\n\n{}", t!("tex.folders.unsafe_names_tip"), list(items, n));
        ui.horizontal(|ui| {
            ui.add_space(24.0);
            ui.add(egui::Label::new(RichText::new(tf!("tex.folders.unsafe_names", n)).small().color(theme::warning())))
                .on_hover_text(tip);
        });
    }
}

fn cell_menu(
    ui: &mut egui::Ui,
    state: &mut AppState,
    entry: &AssetEntry,
    favorite: bool,
    actions: &mut Vec<CellAction>,
) {
    let name = &entry.name;
    let has_sel = !state.doc.selection().is_empty();
    if ui.add_enabled(has_sel, egui::Button::new(t!("tex.menu.apply"))).clicked() {
        actions.push(CellAction::Apply(name.clone()));
        ui.close();
    }
    if ui.button(t!("tex.menu.mark")).clicked() {
        actions.push(CellAction::Mark(name.clone()));
        ui.close();
    }
    ui.separator();
    let fav = if favorite { t!("tex.browser.unfavorite") } else { t!("tex.browser.favorite") };
    if ui.button(fav).clicked() {
        actions.push(CellAction::Favorite(name.clone()));
        ui.close();
    }
    if ui.button(t!("tex.menu.copy")).clicked() {
        actions.push(CellAction::Copy(name.clone()));
        ui.close();
    }
    if ui.button(t!("tex.menu.replace_with")).clicked() {
        actions.push(CellAction::ReplaceWith(name.clone()));
        ui.close();
    }
    ui.separator();
    let file = if entry.source == MISSING_SOURCE { None } else { material_disk_file(state, name) };
    let r = ui.add_enabled(file.is_some(), egui::Button::new(t!("tex.menu.reveal")));
    let r = r.on_disabled_hover_text(t!("tex.menu.reveal_archive"));
    if r.clicked()
        && let Some(f) = file
    {
        actions.push(CellAction::Reveal(f));
        ui.close();
    }
    if ui.add_enabled(entry.source != MISSING_SOURCE, egui::Button::new(t!("tex.menu.edit"))).clicked() {
        actions.push(CellAction::Edit(name.clone()));
        ui.close();
    }
    let vtf = ui.add_enabled(entry.source != MISSING_SOURCE, egui::Button::new(t!("tex.menu.vtf")));
    if vtf.on_hover_text(t!("tex.menu.vtf_tip")).clicked() {
        actions.push(CellAction::Vtf(name.clone()));
        ui.close();
    }
    if ui.add_enabled(entry.source != MISSING_SOURCE, egui::Button::new(t!("tex.menu.shader"))).clicked() {
        actions.push(CellAction::Shader(name.clone()));
        ui.close();
    }
    let folder = folder_of(name);
    if !folder.is_empty() {
        ui.separator();
        if ui.button(tf!("tex.menu.exclude", folder)).on_hover_text(t!("tex.menu.exclude_tip")).clicked() {
            actions.push(CellAction::Exclude(folder.to_owned()));
            ui.close();
        }
    }
}

fn flag_label(flag: MatFlag) -> &'static str {
    match flag {
        MatFlag::Translucent => t!("tex.flag.translucent"),
        MatFlag::Alpha => t!("tex.flag.alpha"),
        MatFlag::SelfIllum => t!("tex.flag.selfillum"),
        MatFlag::EnvMap => t!("tex.flag.envmap"),
        MatFlag::Blend => t!("tex.flag.blend"),
        MatFlag::Missing => t!("tex.flag.missing"),
    }
}

fn lazy_hover_text(r: egui::Response, text: impl FnOnce() -> String) -> egui::Response {
    if r.hovered() || r.ctx.has_accesskit_node(r.id) { r.on_hover_text(text()) } else { r }
}

pub(crate) fn excluded_folders(state: &AppState) -> Vec<String> {
    let b = &state.settings.browser;
    let user = b.excluded_folders.iter().map(String::as_str);
    let fgd: Vec<&str> = match (b.fgd_exclusions, state.assets.fgd()) {
        (true, Some(f)) => f.material_exclusions.iter().map(String::as_str).collect(),
        _ => Vec::new(),
    };
    normalize_exclusions(user.chain(fgd))
}

pub(crate) fn pending_material_pick(state: &AppState) -> Option<&hp_app::runtime::AssetPick> {
    state.runtime.entities.asset_pick.as_ref().filter(|p| p.result.is_none() && p.kind.is_material())
}

pub(crate) fn choose_for_pick(state: &mut AppState, name: &str) {
    if let Some(p) = state.runtime.entities.asset_pick.as_mut()
        && p.result.is_none()
        && matches!(p.kind, AssetKind::Material | AssetKind::Decal | AssetKind::Sprite)
    {
        p.result = Some(name.to_owned());
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn ready(state: &mut AppState) -> (TextureBrowser, egui::Context) {
        crate::register_translations();
        let mut b = TextureBrowser::new();
        hp_app::testing::run_ui(state, |ui, s| b.ui(ui, s));
        (b, egui::Context::default())
    }

    #[test]
    fn browser_never_grows_wider_than_the_dock() {
        crate::register_translations();
        let mut state = hp_app::testing::test_state();
        let _lang = hp_app::testing::lang_lock();
        state.runtime.entities.asset_pick = Some(hp_app::runtime::AssetPick {
            kind: AssetKind::Material,
            entities: vec![5],
            world: false,
            key: "texture".into(),
            label: "infodecal (texture)".into(),
            current: String::new(),
            result: None,
        });
        let mut b = TextureBrowser::new();
        b.set_search("is:translucent");
        for lang in hp_app::i18n::Lang::ALL {
            hp_app::i18n::set_lang(lang);
            for width in [260.0f32, 380.0] {
                let mut used = 0.0f32;
                hp_app::testing::run_ui(&mut state, |ui, s| {
                    let r = ui.allocate_ui(egui::vec2(width, 800.0), |ui| {
                        ui.set_max_width(width);
                        b.ui(ui, s);
                    });
                    used = used.max(r.response.rect.width());
                });
                if used > width + 0.5 {
                    hp_app::i18n::set_lang(hp_app::i18n::Lang::En);
                    panic!("{lang:?}: {used} > {width}");
                }
            }
        }
        hp_app::i18n::set_lang(hp_app::i18n::Lang::En);
    }

    #[test]
    fn browser_never_grows_a_resizable_panel() {
        crate::register_translations();
        let mut state = hp_app::testing::test_state();
        let _lang = hp_app::testing::lang_lock();
        let mut b = TextureBrowser::new();
        for lang in [hp_app::i18n::Lang::En, hp_app::i18n::Lang::Fr] {
            hp_app::i18n::set_lang(lang);
            for search in ["", "is:translucent concrete"] {
                b.set_search(search);
                let ctx = egui::Context::default();
                hp_app::theme::apply(&ctx, hp_app::settings::ThemeChoice::Dark, 1.0);
                let mut widths = Vec::new();
                for i in 0..8 {
                    let input = egui::RawInput {
                        screen_rect: Some(egui::Rect::from_min_size(egui::Pos2::ZERO, egui::vec2(1400.0, 900.0))),
                        time: Some(f64::from(i) * 0.1),
                        ..Default::default()
                    };
                    let mut out = ctx.run_ui(input, |ui| {
                        let r = egui::Panel::right("hp_test_dock")
                            .resizable(true)
                            .default_size(260.0)
                            .min_size(260.0)
                            .show(ui, |ui| b.ui(ui, &mut state));
                        widths.push(r.response.rect.width());
                    });
                    out.textures_delta.clear();
                }
                let last = *widths.last().expect("frames");
                if last > 261.0 {
                    hp_app::i18n::set_lang(hp_app::i18n::Lang::En);
                    panic!("{lang:?} {search:?}: the panel grew: {widths:?}");
                }
            }
        }
        hp_app::i18n::set_lang(hp_app::i18n::Lang::En);
    }

    #[test]
    fn material_picks_take_the_double_clicked_material() {
        let mut state = hp_app::testing::test_state();
        let (mut b, ctx) = ready(&mut state);
        state.runtime.entities.asset_pick = Some(hp_app::runtime::AssetPick {
            kind: AssetKind::Decal,
            entities: vec![5],
            world: false,
            key: "texture".into(),
            label: "infodecal (texture)".into(),
            current: String::new(),
            result: None,
        });
        state.doc.select(&[hp_editor::ObjRef::Solid(3)], SelectOp::Replace);
        let before = state.doc.history_len().0;
        b.run_action(&ctx, &mut state, CellAction::Apply("custom/mytextures/brick01".into()));
        let pick = state.runtime.entities.asset_pick.clone().expect("pick");
        assert_eq!(pick.result.as_deref(), Some("custom/mytextures/brick01"));
        assert_eq!(state.doc.history_len().0, before, "the selected brush is not textured");
        state.runtime.entities.asset_pick = None;
        b.run_action(&ctx, &mut state, CellAction::Apply("custom/mytextures/brick01".into()));
        assert_eq!(state.doc.history_len().0, before + 1);
        b.run_action(&ctx, &mut state, CellAction::Exclude("custom/mytextures".into()));
        assert_eq!(state.settings.browser.excluded_folders, ["custom/mytextures"]);
        hp_app::testing::run_ui(&mut state, |ui, s| b.ui(ui, s));
        assert!(!b.shown_names().iter().any(|n| n.starts_with("custom/mytextures/")));
    }

    #[test]
    fn clicks_and_multi_selection() {
        let mut state = hp_app::testing::test_state();
        let (mut b, ctx) = ready(&mut state);
        let names = b.shown_names();
        assert_eq!(names.len(), 5);
        b.run_action(
            &ctx,
            &mut state,
            CellAction::Click { index: 1, name: names[1].clone(), ctrl: false, shift: false },
        );
        assert_eq!(state.current_material, names[1]);
        assert!(b.marked().is_empty());
        b.run_action(
            &ctx,
            &mut state,
            CellAction::Click { index: 3, name: names[3].clone(), ctrl: false, shift: true },
        );
        assert_eq!(b.marked(), &names[1..=3]);
        assert_eq!(crate::shared::selection(&ctx), &names[1..=3]);
        b.run_action(
            &ctx,
            &mut state,
            CellAction::Click { index: 2, name: names[2].clone(), ctrl: true, shift: false },
        );
        assert_eq!(b.marked(), [names[1].clone(), names[3].clone()]);
        b.run_action(&ctx, &mut state, CellAction::Edit(names[1].clone()));
        assert!(state.ui.show_material_editor);
        assert_eq!(crate::shared::take_editor(&ctx).map(|r| r.0.len()), Some(2));
        b.run_action(&ctx, &mut state, CellAction::ReplaceWith(names[4].clone()));
        let p = crate::shared::take_replace(&ctx).unwrap_or_default();
        assert_eq!(p.find, names[1]);
        assert_eq!(p.replace, names[4]);
        assert!(state.ui.show_replace_textures);
        b.run_action(&ctx, &mut state, CellAction::Favorite(names[0].clone()));
        assert_eq!(state.settings.browser.favorites, [names[0].clone()]);
        b.run_action(&ctx, &mut state, CellAction::Favorite(names[0].clone()));
        assert!(state.settings.browser.favorites.is_empty());
        assert!(state.settings_dirty);
    }

    #[test]
    fn apply_mark_and_browse_for_faces() {
        let mut state = hp_app::testing::test_state();
        let (mut b, ctx) = ready(&mut state);
        assert_eq!(apply_to_selection(&mut state, "brick/x"), None);
        state.doc.select(&[hp_editor::ObjRef::Solid(3)], SelectOp::Replace);
        b.run_action(&ctx, &mut state, CellAction::Apply("custom/mytextures/brick01".into()));
        assert_eq!(state.current_material, "custom/mytextures/brick01");
        let n = state.doc.select_faces_by_material("custom/mytextures/brick01", SelectOp::Replace);
        assert_eq!(n, 6);
        state.requested_tool = None;
        assert_eq!(mark_material(&mut state, "custom/mytextures/brick01"), 6);
        assert_eq!(state.requested_tool, Some(ToolId::Texture));
        state.tool = ToolId::Texture;
        state.doc.select_faces(&[hp_editor::FaceRef::new(2, 0)], SelectOp::Replace);
        crate::shared::set_browse_for_faces(&ctx, true);
        b.pick(&ctx, &mut state, "tools/toolsnodraw");
        assert_eq!(
            state.doc.face_side(hp_editor::FaceRef::new(2, 0)).map(|s| s.material.as_str()),
            Some("tools/toolsnodraw")
        );
        assert!(!crate::shared::browse_for_faces(&ctx));
        assert_eq!(state.ui.right_tab, RightTab::Tool);
        assert!(material_disk_file(&state, "tools/toolsnodraw").is_none());
    }
}
