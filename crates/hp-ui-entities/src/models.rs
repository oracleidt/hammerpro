use std::collections::{HashMap, HashSet};
use std::sync::Arc;
use std::sync::mpsc::{Receiver, Sender, TryRecvError, channel};
use std::time::Duration;

use egui::{Color32, CornerRadius, Key, Modifiers, Rect, RichText, ScrollArea, Sense, Stroke, Ui, vec2};
use hp_app::dnd::DragPayload;
use hp_app::source_filter::SourceFilter;
use hp_app::state::{AssetStatus, ModelInfo, ModelState, PreviewCamera, RightTab, ThumbState};
use hp_app::{Action, AppState, Severity, ToolId, t, tf, theme};
use hp_editor::{Document, Id as EntityId};
use hp_fgd::{PropertyType, ResolvedFgd};
use hp_fs::AssetEntry;

use crate::browse::{Sources, source_chips};
use crate::cx::{model_pick, push_recent_model, recent_models, set_model_pick};
use crate::edit::{Op, apply};
use crate::names::WordSearch;
use crate::values::{model_file_stem, model_folder, model_top_folder};
use crate::view::{DocStamp, SelectionSummary};
use crate::widgets::{
    CLICK, ROW_HEIGHT, RowLook, icon, list_row, list_row_icon, list_row_look, row_scroll_offset, search_box,
};

#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
enum DetailTab {
    #[default]
    Info,
    BodyGroups,
    Sequences,
    Inspector,
}

pub(crate) fn body_value(groups: &[hp_app::state::BodyGroupInfo], choices: &[usize]) -> i64 {
    groups
        .iter()
        .zip(choices)
        .map(|(g, &c)| i64::from(g.base.max(0)) * (c.min(g.choices.len().saturating_sub(1)) as i64))
        .sum()
}

pub(crate) fn body_choices(groups: &[hp_app::state::BodyGroupInfo], body: i64) -> Vec<usize> {
    groups
        .iter()
        .map(|g| {
            let n = g.choices.len().max(1) as i64;
            let base = i64::from(g.base.max(1));
            ((body.max(0) / base) % n) as usize
        })
        .collect()
}

const MAX_TILE_MDL_BYTES: usize = 32 << 20;
const DETAILS_MIN_HEIGHT: f32 = 430.0;
const TAB_CONTENT_MIN_HEIGHT: f32 = 150.0;

const FOLDER_COMBO_MIN: f32 = 90.0;

#[derive(Default)]
struct ModelTiles {
    generation: u64,
    materials: HashMap<String, Option<String>>,
    pending: HashSet<String>,
    requests: Option<Sender<String>>,
    results: Option<Receiver<(String, Option<String>)>>,
}

impl ModelTiles {
    fn sync(&mut self, assets: &dyn hp_app::AssetsApi) {
        if self.generation != assets.generation() {
            *self = ModelTiles { generation: assets.generation(), ..ModelTiles::default() };
        }
        if let Some(rx) = &self.results {
            while let Ok((path, material)) = rx.try_recv() {
                self.pending.remove(&path);
                self.materials.insert(path, material);
            }
        }
    }

    fn material(&mut self, assets: &dyn hp_app::AssetsApi, ctx: &egui::Context, path: &str) -> Option<Option<String>> {
        if let Some(m) = self.materials.get(path) {
            return Some(m.clone());
        }
        let fs = assets.fs()?;
        if self.requests.is_none() {
            let (tx, rx) = channel::<String>();
            let (done_tx, done_rx) = channel();
            let (fs, repaint) = (fs.clone(), ctx.clone());
            let spawned = std::thread::Builder::new().name("hp-model-tiles".to_owned()).spawn(move || {
                while let Ok(path) = rx.recv() {
                    let material = tile_material(&fs, &path);
                    if done_tx.send((path, material)).is_err() {
                        break;
                    }
                    repaint.request_repaint();
                }
            });
            if spawned.is_err() {
                return Some(None);
            }
            self.requests = Some(tx);
            self.results = Some(done_rx);
        }
        if self.pending.insert(path.to_owned())
            && let Some(tx) = &self.requests
            && tx.send(path.to_owned()).is_err()
        {
            self.materials.insert(path.to_owned(), None);
        }
        None
    }
}

fn tile_material(fs: &hp_fs::GameFs, path: &str) -> Option<String> {
    let deps = fs
        .read_with(path, |bytes| {
            (bytes.len() <= MAX_TILE_MDL_BYTES).then(|| hp_mdl::read_dependencies(bytes).ok()).flatten()
        })
        .flatten()?;
    let candidates = deps.material_candidates();
    candidates
        .iter()
        .find_map(|names| names.iter().find(|n| fs.exists(&format!("materials/{n}.vmt"))).cloned())
        .or_else(|| candidates.first().and_then(|n| n.first()).cloned())
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
enum Row {
    Folder { model: u32, count: u32 },
    Model(u32),
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
enum ScrollTo {
    Center,
    Visible,
}

struct ModelIndex {
    key: (usize, usize),
    order: Vec<u32>,
    top_folders: Vec<String>,
}

fn index_models(models: &[AssetEntry]) -> (Vec<u32>, Vec<String>) {
    let mut order: Vec<u32> = (0..models.len().min(u32::MAX as usize) as u32).collect();
    order.sort_by(|&a, &b| {
        let (ma, mb) = (&models[a as usize].name, &models[b as usize].name);
        model_folder(ma).cmp(model_folder(mb)).then_with(|| ma.cmp(mb))
    });
    let mut top: Vec<String> = Vec::new();
    let mut seen: HashSet<&str> = HashSet::new();
    for m in models {
        let t = model_top_folder(&m.name);
        if !t.is_empty() && seen.insert(t) {
            top.push(t.to_owned());
        }
    }
    top.sort_unstable();
    (order, top)
}

#[cfg(test)]
fn build_rows(
    models: &[AssetEntry],
    order: &[u32],
    search: &str,
    folder: Option<&str>,
    collapsed: &HashSet<String>,
) -> Vec<Row> {
    build_rows_where(models, order, search, folder, collapsed, &|_| true)
}

fn build_rows_where(
    models: &[AssetEntry],
    order: &[u32],
    search: &str,
    folder: Option<&str>,
    collapsed: &HashSet<String>,
    pass: &dyn Fn(u32) -> bool,
) -> Vec<Row> {
    let search = WordSearch::parse(search);
    let mut rows = Vec::new();
    let mut current_folder: Option<&str> = None;
    let mut header: Option<usize> = None;
    for &i in order {
        let Some(m) = models.get(i as usize) else { continue };
        let name = m.name.as_str();
        if let Some(f) = folder
            && model_top_folder(name) != f
        {
            continue;
        }
        if !search.terms.iter().all(|w| contains_ignore_case(name, w))
            || search.excluded.iter().any(|w| contains_ignore_case(name, w))
            || !pass(m.source)
        {
            continue;
        }
        let f = model_folder(name);
        if current_folder != Some(f) {
            current_folder = Some(f);
            header = Some(rows.len());
            rows.push(Row::Folder { model: i, count: 0 });
        }
        if let Some(Row::Folder { count, .. }) = header.and_then(|h| rows.get_mut(h)) {
            *count += 1;
        }
        if !collapsed.contains(f) {
            rows.push(Row::Model(i));
        }
    }
    rows
}

fn contains_ignore_case(haystack: &str, needle: &str) -> bool {
    crate::names::contains_lower(haystack, needle)
}

pub(crate) fn prop_entities(doc: &Document, fgd: Option<&ResolvedFgd>, ids: &[EntityId]) -> Vec<EntityId> {
    ids.iter()
        .copied()
        .filter(|id| {
            let Some(n) = doc.scene().entity(*id) else { return false };
            let class = &n.entity.classname;
            if class.to_ascii_lowercase().starts_with("prop_") {
                return true;
            }
            fgd.and_then(|f| f.class(class)).is_some_and(|c| {
                c.studio().is_some_and(|s| s.model.is_none())
                    || c.property("model").is_some_and(|p| p.kind == PropertyType::Studio)
            })
        })
        .collect()
}

pub struct ModelBrowser {
    search: String,
    folder: Option<String>,
    collapsed: HashSet<String>,
    show_recent: bool,
    models: Arc<Vec<AssetEntry>>,
    index: Option<ModelIndex>,
    indexing: Option<(usize, usize, Receiver<ModelIndex>)>,
    rows: Vec<Row>,
    rows_dirty: bool,
    grid_rows: Vec<u32>,
    count_text: (usize, hp_app::i18n::Lang, String),
    selected: Option<String>,
    scroll_to_selected: Option<ScrollTo>,
    list_view: (f32, f32),
    camera: PreviewCamera,
    props_cache: Option<(DocStamp, Vec<EntityId>)>,
    last_pick: Option<crate::cx::ModelPick>,
    sources: Sources,
    source: SourceFilter,
    rows_gmod_only: bool,
    tiles: ModelTiles,
    tab: DetailTab,
    body: Vec<usize>,
    sequence: Option<String>,
    playing: bool,
    anim_time: f32,
    autoplay: bool,
    uses: crate::browse::UsesCache,
    inspector: crate::inspect::Inspector,
    tools: crate::model_tools::ModelTools,
}

impl Default for ModelBrowser {
    fn default() -> Self {
        Self::new()
    }
}

impl ModelBrowser {
    pub fn new() -> Self {
        Self {
            search: String::new(),
            folder: None,
            collapsed: HashSet::new(),
            show_recent: true,
            models: Arc::new(Vec::new()),
            index: None,
            indexing: None,
            rows: Vec::new(),
            rows_dirty: true,
            grid_rows: Vec::new(),
            count_text: (usize::MAX, hp_app::i18n::Lang::En, String::new()),
            selected: None,
            scroll_to_selected: None,
            list_view: (0.0, 0.0),
            camera: PreviewCamera::default(),
            props_cache: None,
            last_pick: None,
            sources: Sources::default(),
            source: SourceFilter::All,
            rows_gmod_only: false,
            tiles: ModelTiles::default(),
            tab: DetailTab::Info,
            body: Vec::new(),
            sequence: None,
            playing: false,
            anim_time: 0.0,
            autoplay: false,
            uses: crate::browse::UsesCache::default(),
            inspector: crate::inspect::Inspector::default(),
            tools: crate::model_tools::ModelTools::default(),
        }
    }

    pub fn selected(&self) -> Option<&str> {
        self.selected.as_deref()
    }

    pub fn select(&mut self, path: &str) {
        self.select_with(path, ScrollTo::Center);
    }

    fn select_with(&mut self, path: &str, scroll: ScrollTo) {
        if self.selected.as_deref() != Some(path) {
            self.camera.skin = 0;
            self.body.clear();
            self.sequence = None;
            self.playing = false;
            self.anim_time = 0.0;
            self.autoplay = true;
        }
        self.selected = Some(path.to_owned());
        self.scroll_to_selected = Some(scroll);
        if self.collapsed.remove(model_folder(path)) {
            self.rows_dirty = true;
        }
    }

    fn drag_skin(&self, path: &str) -> u32 {
        if self.selected.as_deref() == Some(path) { self.camera.skin } else { 0 }
    }

    #[cfg(test)]
    pub(crate) fn list_offset(&self) -> f32 {
        self.list_view.0
    }

    #[cfg(test)]
    pub(crate) fn preview_skin(&self) -> u32 {
        self.camera.skin
    }

    #[cfg(test)]
    pub(crate) fn set_preview_skin(&mut self, skin: u32) {
        self.camera.skin = skin;
    }

    pub fn set_search(&mut self, text: &str) {
        text.clone_into(&mut self.search);
        self.rows_dirty = true;
    }

    pub fn row_count(&self) -> usize {
        self.rows.len()
    }

    pub fn ui(&mut self, ui: &mut Ui, state: &mut AppState) {
        self.sync_models(ui.ctx(), state);
        let pick = model_pick(ui.ctx());

        match state.assets.status() {
            AssetStatus::Ready => {}
            AssetStatus::Mounting => {
                ui.horizontal(|ui| {
                    ui.spinner();
                    ui.label(t!("status.loading_assets"));
                });
            }
            AssetStatus::NotConfigured => {
                ui.label(RichText::new(t!("entities.models_not_configured")).color(theme::warning()));
                if ui.button(t!("action.settings")).clicked() {
                    state.request(Action::Settings);
                }
            }
            AssetStatus::Failed(msg) => {
                ui.label(RichText::new(tf!("entities.models_failed", msg)).color(theme::error()));
            }
        }

        if pick != self.last_pick {
            if let Some(current) = pick.as_ref().and_then(|p| picked_model(&state.doc, p))
                && self.models.iter().any(|m| m.name == current)
            {
                self.select(&current);
            }
            self.last_pick.clone_from(&pick);
        }
        if let Some(p) = &pick {
            self.pick_banner(ui, state, p);
        }

        if search_box(ui, &mut self.search, t!("entities.search_models")) {
            self.rows_dirty = true;
        }
        self.sources.refresh(state.assets.as_ref());
        let mut gmod_only = state.settings.browser.gmod_only;
        if source_chips(ui, &self.sources, &mut self.source, &mut gmod_only) {
            self.rows_dirty = true;
            if gmod_only != state.settings.browser.gmod_only {
                state.settings.browser.gmod_only = gmod_only;
                state.settings_dirty = true;
            }
        }
        if self.rows_gmod_only != state.settings.browser.gmod_only {
            self.rows_gmod_only = state.settings.browser.gmod_only;
            self.rows_dirty = true;
        }
        let row_h = ui.spacing().interact_size.y;
        let layout = egui::Layout::right_to_left(egui::Align::Center);
        let mut folder_below = false;
        ui.allocate_ui_with_layout(vec2(ui.available_width(), row_h), layout, |ui| {
            if ui.small_button("+").on_hover_text(Action::ImportModel.label()).clicked() {
                state.request(Action::ImportModel);
            }
            if ui.small_button("++").on_hover_text(Action::ImportModelsBatch.label()).clicked() {
                state.request(Action::ImportModelsBatch);
            }
            let grid = state.settings.browser.model_grid;
            let (glyph, tip) = if grid { ("☰", t!("entities.pb_list")) } else { ("⊞", t!("entities.pb_grid")) };
            if ui.small_button(glyph).on_hover_text(tip).clicked() {
                state.settings.browser.model_grid = !grid;
                state.settings_dirty = true;
            }
            let count = self.grid_rows.len();
            let lang = hp_app::i18n::lang();
            if self.count_text.0 != count || self.count_text.1 != lang {
                self.count_text = (count, lang, tf!("entities.models_count", count));
            }
            ui.label(RichText::new(self.count_text.2.as_str()).weak().small());
            let width = ui.available_width() - 2.0;
            if width >= FOLDER_COMBO_MIN {
                self.folder_combo(ui, width);
            } else {
                folder_below = true;
            }
        });
        if folder_below {
            self.folder_combo(ui, ui.available_width() - 8.0);
        }
        if self.rows_dirty
            && let Some(index) = &self.index
        {
            let (sources, source, gmod_only) = (&self.sources, self.source, self.rows_gmod_only);
            let pass = |s: u32| sources.passes(source, gmod_only, Some(s));
            self.rows = build_rows_where(
                &self.models,
                &index.order,
                &self.search,
                self.folder.as_deref(),
                &self.collapsed,
                &pass,
            );
            self.grid_rows.clear();
            self.grid_rows.extend(self.rows.iter().filter_map(|r| match r {
                Row::Model(i) => Some(*i),
                Row::Folder { .. } => None,
            }));
            self.rows_dirty = false;
        }

        let recent_len = crate::cx::recent_models_len(ui.ctx());
        if recent_len > 0 {
            let header = egui::CollapsingHeader::new(tf!("entities.recent_count", recent_len))
                .id_salt("hp_ui_entities.recent_models")
                .default_open(self.show_recent)
                .show(ui, |ui| {
                    let recent = recent_models(ui.ctx());
                    for m in recent.iter().take(8) {
                        let resp = list_row(ui, model_file_stem(m), m, self.selected.as_deref() == Some(m), false);
                        if resp.clicked() {
                            self.select(m);
                        }
                        if resp.double_clicked() {
                            self.activate(ui.ctx(), state, m, pick.as_ref());
                        }
                    }
                });
            self.show_recent = header.fully_open();
        }

        let avail = ui.available_height();
        let list_h = if self.selected.is_none() {
            (avail - 40.0).max(ROW_HEIGHT * 5.0)
        } else if state.settings.browser.model_grid {
            (avail * 0.5).min(avail - DETAILS_MIN_HEIGHT).clamp(ROW_HEIGHT * 5.0, 900.0)
        } else {
            (avail * 0.38).min(avail - DETAILS_MIN_HEIGHT).clamp(ROW_HEIGHT * 5.0, 520.0)
        };
        let list_hovered = if state.settings.browser.model_grid {
            self.grid(ui, state, list_h, pick.as_ref())
        } else {
            let mut area = ScrollArea::vertical()
                .id_salt("hp_ui_entities.model_list")
                .max_height(list_h)
                .auto_shrink([false, false]);
            if let Some(y) = self.take_scroll_offset(ui.spacing().item_spacing.y, list_h) {
                area = area.vertical_scroll_offset(y);
            }
            let list = area.show_rows(ui, ROW_HEIGHT, self.rows.len(), |ui, range| {
                self.list_rows(ui, state, range, pick.as_ref());
            });
            self.list_view = (list.state.clamped_offset().y, list.inner_rect.height());
            ui.rect_contains_pointer(list.inner_rect)
        };
        if self.index.is_none() && !self.models.is_empty() {
            ui.horizontal(|ui| {
                ui.spinner();
                ui.label(t!("entities.indexing_models"));
            });
        } else if self.rows.is_empty() {
            ui.label(RichText::new(t!("entities.no_models")).weak());
        }
        if list_hovered
            && !ui.ctx().text_edit_focused()
            && self.keyboard_nav(ui)
            && let Some(path) = self.selected.clone()
        {
            self.activate(ui.ctx(), state, &path, pick.as_ref());
        }
        ui.separator();

        if self.selected.is_some()
            && ui.rect_contains_pointer(ui.max_rect())
            && !ui.ctx().text_edit_focused()
            && !egui::Popup::is_any_open(ui.ctx())
            && ui.input_mut(|i| i.consume_key(Modifiers::NONE, Key::Escape))
        {
            self.selected = None;
        }
        match self.selected.clone() {
            Some(path) => {
                let height = ui.available_height();
                ScrollArea::vertical()
                    .id_salt("hp_ui_entities.model_details")
                    .auto_shrink([false, false])
                    .show(ui, |ui| self.details(ui, state, &path, pick.as_ref(), height));
            }
            None => {
                ui.add(egui::Label::new(RichText::new(t!("entities.select_model_hint")).weak()).wrap());
            }
        }
    }

    fn sync_models(&mut self, ctx: &egui::Context, state: &AppState) {
        let models = state.assets.models();
        let key = (Arc::as_ptr(&models) as usize, models.len());
        let current = (Arc::as_ptr(&self.models) as usize, self.models.len());
        if key != current {
            self.models = models;
            self.index = None;
            self.rows.clear();
            self.grid_rows.clear();
            self.rows_dirty = true;
            let (tx, rx) = channel();
            let list = Arc::clone(&self.models);
            let repaint = ctx.clone();
            let spawned = std::thread::Builder::new().name("hp-model-index".to_owned()).spawn(move || {
                let (order, top_folders) = index_models(&list);
                let _ = tx.send(ModelIndex { key, order, top_folders });
                repaint.request_repaint();
            });
            match spawned {
                Ok(_) => self.indexing = Some((key.0, key.1, rx)),
                Err(e) => {
                    log::warn!("model index thread failed to start: {e}");
                    let (order, top_folders) = index_models(&self.models);
                    self.index = Some(ModelIndex { key, order, top_folders });
                }
            }
        }
        if let Some((p, n, rx)) = &self.indexing {
            match rx.try_recv() {
                Ok(index) => {
                    if index.key == (*p, *n) && index.key == key {
                        self.index = Some(index);
                        self.rows_dirty = true;
                    }
                    self.indexing = None;
                }
                Err(TryRecvError::Empty) => ctx.request_repaint_after(Duration::from_millis(100)),
                Err(TryRecvError::Disconnected) => self.indexing = None,
            }
        }
    }

    fn folder_combo(&mut self, ui: &mut Ui, width: f32) {
        let folder_text = self.folder.clone().unwrap_or_else(|| t!("entities.all_folders").to_owned());
        egui::ComboBox::from_id_salt("hp_ui_entities.model_folder")
            .width(width)
            .selected_text(folder_text)
            .truncate()
            .height(400.0)
            .show_ui(ui, |ui| {
                if ui.selectable_label(self.folder.is_none(), t!("entities.all_folders")).clicked() {
                    self.folder = None;
                    self.rows_dirty = true;
                }
                if let Some(index) = &self.index {
                    for f in &index.top_folders {
                        if ui.selectable_label(self.folder.as_deref() == Some(f), f).clicked() {
                            self.folder = Some(f.clone());
                            self.rows_dirty = true;
                        }
                    }
                }
            });
    }

    fn grid(&mut self, ui: &mut Ui, state: &mut AppState, height: f32, pick: Option<&crate::cx::ModelPick>) -> bool {
        self.tiles.sync(state.assets.as_ref());
        let models = std::mem::take(&mut self.grid_rows);
        let tile = state.settings.browser.tile();
        let (gap, label_h) = (4.0, 15.0);
        let width = (ui.available_width() - ui.spacing().scroll.allocated_width()).max(tile);
        let cols = (((width + gap) / (tile + gap)).floor() as usize).max(1);
        let rows = models.len().div_ceil(cols);
        let row_h = tile + label_h + gap;
        let mut area =
            ScrollArea::vertical().id_salt("hp_ui_entities.model_grid").max_height(height).auto_shrink([false, false]);
        if self.scroll_to_selected.take().is_some()
            && let Some(sel) = &self.selected
            && let Some(i) = models.iter().position(|&m| self.models.get(m as usize).is_some_and(|e| &e.name == sel))
        {
            area = area.vertical_scroll_offset(((i / cols) as f32 * row_h - row_h).max(0.0));
        }
        let all = Arc::clone(&self.models);
        let ctx = ui.ctx().clone();
        let (mut clicked, mut activated) = (None, None);
        let out = area.show_rows(ui, row_h, rows, |ui, range| {
            for r in range {
                let (rect, _) = ui.allocate_exact_size(vec2(width.min(ui.available_width()), row_h), Sense::hover());
                for c in 0..cols {
                    let Some(&m) = models.get(r * cols + c) else { break };
                    let Some(entry) = all.get(m as usize) else { continue };
                    let path = entry.name.as_str();
                    let cell =
                        Rect::from_min_size(rect.min + vec2(c as f32 * (tile + gap), 0.0), vec2(tile, tile + label_h));
                    let resp = ui.interact(cell, ui.scope_id().with(("model_tile", m)), CLICK | Sense::DRAG);
                    let selected = self.selected.as_deref() == Some(path);
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
                    let img = Rect::from_min_size(cell.min + vec2(4.0, 4.0), vec2(tile - 8.0, tile - 8.0));
                    let thumb = match state.assets.model_thumbnail(&ctx, path) {
                        ThumbState::Missing => match self.tiles.material(state.assets.as_ref(), &ctx, path) {
                            Some(Some(mat)) => state.assets.thumbnail(&ctx, &mat),
                            Some(None) => ThumbState::Missing,
                            None => ThumbState::Loading,
                        },
                        t => t,
                    };
                    paint_tile(&painter, img, thumb);
                    let mut job = egui::text::LayoutJob::simple_singleline(
                        model_file_stem(path).to_owned(),
                        egui::FontId::proportional(11.0),
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
                    if resp.drag_started() {
                        egui::DragAndDrop::set_payload(
                            ui.ctx(),
                            DragPayload::Model { path: path.to_owned(), skin: self.drag_skin(path) },
                        );
                        push_recent_model(ui.ctx(), path);
                    }
                    let sources = &self.sources;
                    let resp = if resp.hovered() || resp.ctx.has_accesskit_node(resp.id) {
                        resp.on_hover_text(format!("{path}\n{}", sources.badge_str(Some(entry.source))))
                    } else {
                        resp
                    };
                    if resp.clicked() {
                        clicked = Some(path.to_owned());
                    }
                    if resp.double_clicked() {
                        activated = Some(path.to_owned());
                    }
                }
            }
        });
        self.grid_rows = models;
        if let Some(p) = clicked {
            self.select_with(&p, ScrollTo::Visible);
            self.scroll_to_selected = None;
        }
        if let Some(p) = activated {
            self.activate(ui.ctx(), state, &p, pick);
        }
        ui.rect_contains_pointer(out.inner_rect)
    }

    fn take_scroll_offset(&mut self, spacing_y: f32, list_h: f32) -> Option<f32> {
        let scroll = self.scroll_to_selected?;
        if self.index.is_none() || self.rows_dirty {
            return None;
        }
        self.scroll_to_selected = None;
        let selected = self.selected.as_deref()?;
        let row = self.rows.iter().position(|r| match r {
            Row::Model(i) => self.models.get(*i as usize).is_some_and(|m| m.name == selected),
            Row::Folder { .. } => false,
        })?;
        let (offset, view_h) = self.list_view;
        let view_h = if view_h > 0.0 { view_h.min(list_h) } else { list_h };
        row_scroll_offset(row, ROW_HEIGHT + spacing_y, offset, view_h, scroll == ScrollTo::Center)
    }

    fn list_rows(
        &mut self,
        ui: &mut Ui,
        state: &mut AppState,
        range: std::ops::Range<usize>,
        pick: Option<&crate::cx::ModelPick>,
    ) {
        let models = Arc::clone(&self.models);
        for r in range {
            let Some(row) = self.rows.get(r).copied() else { continue };
            match row {
                Row::Folder { model, count } => {
                    let Some(m) = models.get(model as usize) else { continue };
                    let folder = model_folder(&m.name);
                    let collapsed = self.collapsed.contains(folder);
                    let arrow = if collapsed { "▸" } else { "▾" };
                    let label = folder.strip_prefix("models/").unwrap_or(folder);
                    let resp = list_row_icon(ui, arrow, label, &count.to_string(), false, true);
                    if resp.clicked() {
                        if collapsed {
                            self.collapsed.remove(folder);
                        } else {
                            self.collapsed.insert(folder.to_owned());
                        }
                        self.rows_dirty = true;
                    }
                }
                Row::Model(i) => {
                    let Some(m) = models.get(i as usize) else { continue };
                    let path = m.name.as_str();
                    let selected = self.selected.as_deref() == Some(path);
                    let detail =
                        if self.sources.table.is_empty() { "" } else { self.sources.badge_str(Some(m.source)) };
                    let look =
                        RowLook { main: model_file_stem(path), detail, selected, indent: 12.0, ..RowLook::default() };
                    let resp = list_row_look(ui, look, CLICK | Sense::DRAG);
                    if resp.drag_started() {
                        egui::DragAndDrop::set_payload(
                            ui.ctx(),
                            DragPayload::Model { path: path.to_owned(), skin: self.drag_skin(path) },
                        );
                        push_recent_model(ui.ctx(), path);
                    }
                    if resp.dragged() {
                        ui.ctx().set_cursor_icon(egui::CursorIcon::Grabbing);
                    }
                    let resp = resp.on_hover_text(path);
                    if resp.clicked() {
                        self.select_with(path, ScrollTo::Visible);
                    }
                    if resp.double_clicked() {
                        self.activate(ui.ctx(), state, path, pick);
                    }
                }
            }
        }
    }

    fn keyboard_nav(&mut self, ui: &Ui) -> bool {
        let (down, up) = ui.input_mut(|i| {
            (i.consume_key(Modifiers::NONE, Key::ArrowDown), i.consume_key(Modifiers::NONE, Key::ArrowUp))
        });
        if self.selected.is_some() && ui.input_mut(|i| i.consume_key(Modifiers::NONE, Key::Enter)) {
            return true;
        }
        if !down && !up {
            return false;
        }
        let model_rows: Vec<u32> = self
            .rows
            .iter()
            .filter_map(|r| match r {
                Row::Model(i) => Some(*i),
                Row::Folder { .. } => None,
            })
            .collect();
        if model_rows.is_empty() {
            return false;
        }
        let current = self
            .selected
            .as_deref()
            .and_then(|s| model_rows.iter().position(|&i| self.models.get(i as usize).is_some_and(|m| m.name == s)));
        let next = match (current, down) {
            (None, _) => 0,
            (Some(c), true) => (c + 1).min(model_rows.len() - 1),
            (Some(c), false) => c.saturating_sub(1),
        };
        if let Some(m) = model_rows.get(next).and_then(|&i| self.models.get(i as usize)) {
            let name = m.name.clone();
            self.select_with(&name, ScrollTo::Visible);
        }
        false
    }

    fn activate(&mut self, ctx: &egui::Context, state: &mut AppState, path: &str, pick: Option<&crate::cx::ModelPick>) {
        let skin = if self.selected.as_deref() == Some(path) { self.camera.skin } else { 0 };
        match pick {
            Some(p) => apply_pick(ctx, state, p, path, skin),
            None => use_for_new_props(ctx, state, path, skin),
        }
    }

    fn pick_banner(&mut self, ui: &mut Ui, state: &mut AppState, pick: &crate::cx::ModelPick) {
        egui::Frame::new()
            .fill(theme::accent_bg())
            .stroke(Stroke::new(1.0, theme::accent()))
            .corner_radius(CornerRadius::same(4))
            .inner_margin(egui::Margin::same(6))
            .show(ui, |ui| {
                ui.label(RichText::new(tf!("entities.pick_banner", pick.label)).color(theme::accent()));
                ui.horizontal(|ui| {
                    let selected = self.selected.clone();
                    if ui.add_enabled(selected.is_some(), egui::Button::new(t!("common.apply"))).clicked()
                        && let Some(path) = selected
                    {
                        apply_pick(ui.ctx(), state, pick, &path, self.camera.skin);
                    }
                    if ui.button(t!("common.cancel")).clicked() {
                        set_model_pick(ui.ctx(), None);
                    }
                });
            });
        ui.add_space(4.0);
    }

    fn details(
        &mut self,
        ui: &mut Ui,
        state: &mut AppState,
        path: &str,
        pick: Option<&crate::cx::ModelPick>,
        height: f32,
    ) {
        let mut close = false;
        ui.horizontal(|ui| {
            ui.label(RichText::new(model_file_stem(path)).strong()).on_hover_text(path);
            ui.with_layout(egui::Layout::right_to_left(egui::Align::Center), |ui| {
                if ui.small_button("{d7}").on_hover_text(t!("common.close")).clicked() {
                    close = true;
                }
            });
        });
        if close {
            self.selected = None;
            return;
        }
        ui.label(RichText::new(path).small().weak());
        let info = state.assets.model_info(path);

        let is_static = match &info {
            ModelState::Ready(i) => i.is_static_prop,
            _ => true,
        };
        let props = self.selected_props(state);
        ui.horizontal_wrapped(|ui| {
            let class = if is_static { "prop_static" } else { "prop_physics" };
            let use_btn = ui.button(format!("+ {}", t!("entities.use_for_new_props")));
            if use_btn.on_hover_text(tf!("entities.use_for_new_props_tip", class)).clicked() {
                use_for_new_props(ui.ctx(), state, path, self.camera.skin);
            }
            let apply_btn =
                ui.add_enabled(!props.is_empty(), egui::Button::new(tf!("entities.apply_to_props", props.len())));
            if apply_btn.on_hover_text(t!("entities.apply_to_props_tip")).clicked() {
                let op = Op::SetKeys {
                    target: crate::view::Target::Entities(props.clone()),
                    keys: model_keys("model", path, self.camera.skin),
                };
                match apply(&mut state.doc, None, op) {
                    Ok(_) => push_recent_model(ui.ctx(), path),
                    Err(e) => state.notify(Severity::Error, tf!("entities.edit_failed", e)),
                }
            }
            if pick.is_some()
                && ui.button(t!("entities.apply_pick")).clicked()
                && let Some(p) = pick
            {
                apply_pick(ui.ctx(), state, p, path, self.camera.skin);
            }
            if ui.small_button(t!("action.copy")).on_hover_text(t!("entities.copy_path")).clicked() {
                ui.ctx().copy_text(path.to_owned());
            }
        });
        ui.add_space(4.0);

        let used = ui.min_rect().height();
        let reserve = 3.0 * ui.spacing().interact_size.y + TAB_CONTENT_MIN_HEIGHT + 16.0;
        let room = height - used - reserve;
        let size = (ui.available_width() - 4.0).min(room).clamp(112.0, 256.0);
        let ppp = ui.ctx().pixels_per_point();
        let px = (size * ppp).round().max(1.0) as u32;
        if self.autoplay
            && let ModelState::Ready(i) = &info
        {
            self.autoplay = false;
            if self.sequence.is_none()
                && let Some(first) = i.preview_sequence(None).and_then(|s| i.sequences.get(s))
            {
                self.sequence = Some(first.clone());
                self.playing = true;
                self.anim_time = 0.0;
            }
        }
        let seq_index = self.playing.then(|| match &info {
            ModelState::Ready(i) => self.sequence.as_deref().and_then(|n| i.sequences.iter().position(|s| s == n)),
            _ => None,
        });
        self.camera.anim = seq_index.flatten();
        if self.camera.anim.is_some() {
            let dt = ui.input(|i| i.stable_dt).clamp(0.0, 0.1);
            self.anim_time += dt;
            self.camera.anim_time = self.anim_time;
            ui.ctx().request_repaint();
        } else {
            self.camera.anim_time = 0.0;
        }
        let texture = match state.gfx.as_mut() {
            Some(gfx) => state.assets.model_preview(gfx, path, &self.camera, [px, px]),
            None => None,
        };
        ui.vertical_centered(|ui| {
            let resp = match texture {
                Some(tex) => ui.add(egui::Image::new((tex, vec2(size, size))).sense(CLICK | Sense::DRAG)),
                None => {
                    let (rect, resp) = ui.allocate_exact_size(vec2(size, size), CLICK | Sense::DRAG);
                    ui.painter().rect_filled(rect, CornerRadius::same(4), ui.visuals().extreme_bg_color);
                    let text = if state.gfx.is_none() {
                        t!("entities.preview_unavailable")
                    } else if matches!(info, ModelState::Failed(_)) {
                        t!("entities.preview_failed")
                    } else {
                        t!("common.loading")
                    };
                    ui.painter().text(
                        rect.center(),
                        egui::Align2::CENTER_CENTER,
                        text,
                        egui::FontId::proportional(12.0),
                        ui.visuals().weak_text_color(),
                    );
                    resp
                }
            };
            if resp.dragged() {
                let d = resp.drag_delta();
                self.camera.yaw_deg = (self.camera.yaw_deg - d.x * 0.5).rem_euclid(360.0);
                self.camera.pitch_deg = (self.camera.pitch_deg + d.y * 0.5).clamp(-89.0, 89.0);
            }
            if resp.hovered() {
                let scroll = ui.input(|i| i.smooth_scroll_delta.y);
                if scroll != 0.0 {
                    self.camera.zoom = (self.camera.zoom * (1.0 - scroll * 0.002)).clamp(0.2, 8.0);
                }
            }
            if resp.double_clicked() {
                self.reset_camera();
            }
            resp.on_hover_text(t!("entities.preview_tip"));
        });
        if texture.is_none() && state.gfx.is_some() && !matches!(info, ModelState::Failed(_)) {
            ui.ctx().request_repaint_after(Duration::from_millis(100));
        }

        ui.horizontal(|ui| {
            let skins = match &info {
                ModelState::Ready(i) => i.skins.max(1),
                _ => 1,
            };
            ui.add_enabled_ui(skins > 1, |ui| {
                egui::ComboBox::from_id_salt("hp_ui_entities.model_skin")
                    .width(70.0)
                    .selected_text(tf!("entities.skin_n", self.camera.skin))
                    .show_ui(ui, |ui| {
                        for s in 0..skins {
                            ui.selectable_value(&mut self.camera.skin, s, tf!("entities.skin_n", s));
                        }
                    });
            });
            let wire = ui.add(egui::Button::selectable(self.camera.wireframe, icon("▦")));
            if wire.on_hover_text(t!("entities.wireframe")).clicked() {
                self.camera.wireframe = !self.camera.wireframe;
            }
            if ui.button(icon("↺")).on_hover_text(t!("entities.reset_view")).clicked() {
                self.reset_camera();
            }
        });
        preview_overlays(ui, &mut self.camera, None);

        match &info {
            ModelState::Loading => {
                ui.horizontal(|ui| {
                    ui.spinner();
                    ui.label(t!("common.loading"));
                });
                ui.ctx().request_repaint_after(Duration::from_millis(100));
            }
            ModelState::Failed(msg) => {
                ui.label(RichText::new(tf!("entities.model_failed", msg)).color(theme::error()));
            }
            ModelState::Ready(i) => {
                let tabs = [
                    (DetailTab::Info, t!("entities.model_tab_info").to_owned(), false),
                    (DetailTab::BodyGroups, tf!("entities.model_tab_bodygroups", i.body_groups.len()), false),
                    (DetailTab::Sequences, tf!("entities.model_tab_sequences", i.sequences.len()), false),
                    (DetailTab::Inspector, t!("entities.model_tab_inspector").to_owned(), false),
                ];
                crate::widgets::tab_bar(ui, &mut self.tab, &tabs);
                match self.tab {
                    DetailTab::Info => {
                        model_info_grid(ui, i);
                        self.replace_uses_ui(ui, state, path);
                    }
                    DetailTab::BodyGroups => self.body_groups_ui(ui, state, i),
                    DetailTab::Sequences => self.sequences_ui(ui, state, i),
                    DetailTab::Inspector => {
                        let inspection = self.inspector.get(ui.ctx(), state.assets.as_ref(), path);
                        crate::inspect::inspector_ui(ui, inspection.as_deref());
                        self.tools.ui(ui, state, path);
                    }
                }
            }
        }
        ui.add_space(4.0);
    }

    fn body_groups_ui(&mut self, ui: &mut Ui, state: &mut AppState, info: &ModelInfo) {
        if info.body_groups.iter().all(|g| g.choices.len() < 2) {
            ui.label(RichText::new(t!("entities.no_bodygroups")).weak());
            return;
        }
        if self.body.len() != info.body_groups.len() {
            let props = self.selected_props(state);
            let body = props
                .first()
                .and_then(|id| state.doc.scene().entity(*id))
                .filter(|n| n.entity.get("model").is_some_and(|m| m.trim().eq_ignore_ascii_case(&info.path)))
                .and_then(|n| n.entity.get("body"))
                .and_then(|b| b.trim().parse::<i64>().ok())
                .unwrap_or(0);
            self.body = body_choices(&info.body_groups, body);
        }
        egui::Grid::new("hp_ui_entities.model_bodygroups").num_columns(2).spacing([8.0, 4.0]).show(ui, |ui| {
            for (g, choice) in info.body_groups.iter().zip(self.body.iter_mut()) {
                if g.choices.len() < 2 {
                    continue;
                }
                ui.label(&g.name);
                let current = g.choices.get(*choice).map_or("", String::as_str);
                egui::ComboBox::from_id_salt(("hp_ui_entities.bodygroup", &g.name)).selected_text(current).show_ui(
                    ui,
                    |ui| {
                        for (i, name) in g.choices.iter().enumerate() {
                            ui.selectable_value(choice, i, name);
                        }
                    },
                );
                ui.end_row();
            }
        });
        let body = body_value(&info.body_groups, &self.body);
        ui.label(RichText::new(tf!("entities.body_value", body)).small().weak());
        let props = self.selected_props(state);
        let btn = ui.add_enabled(!props.is_empty(), egui::Button::new(tf!("entities.apply_body", props.len())));
        if btn.clicked() {
            let op = Op::SetKey {
                target: crate::view::Target::Entities(props),
                key: "body".to_owned(),
                value: body.to_string(),
                merge: false,
            };
            if let Err(e) = apply(&mut state.doc, None, op) {
                state.notify(Severity::Error, tf!("entities.edit_failed", e));
            }
        }
    }

    fn sequences_ui(&mut self, ui: &mut Ui, state: &mut AppState, info: &ModelInfo) {
        if info.sequences.is_empty() {
            ui.label(RichText::new(t!("entities.no_sequences")).weak());
            return;
        }
        let props = self.selected_props(state);
        let mut apply_now = None;
        ui.horizontal(|ui| {
            let can_play = self.sequence.is_some();
            let label = if self.playing { icon("⏸") } else { icon("▶") };
            if ui
                .add_enabled(can_play, egui::Button::selectable(self.playing, label))
                .on_hover_text(t!("entities.play_sequence_tip"))
                .clicked()
            {
                self.playing = !self.playing;
                self.anim_time = 0.0;
            }
            if ui.button(icon("↺")).on_hover_text(t!("entities.restart_sequence")).clicked() {
                self.anim_time = 0.0;
            }
            if let Some(seq) = self.sequence.as_deref() {
                ui.label(RichText::new(seq).weak());
            }
        });
        ScrollArea::vertical().id_salt("hp_ui_entities.model_sequences").max_height(160.0).show_rows(
            ui,
            ROW_HEIGHT,
            info.sequences.len(),
            |ui, range| {
                for i in range {
                    let name = &info.sequences[i];
                    let selected = self.sequence.as_deref() == Some(name.as_str());
                    let frames = info.sequence_frames.get(i).copied().unwrap_or(1);
                    let detail = if frames > 1 { tf!("entities.frames_n", frames) } else { String::new() };
                    let resp = list_row(ui, name, &detail, selected, false);
                    if resp.clicked() {
                        self.sequence = Some(name.clone());
                        self.playing = true;
                        self.anim_time = 0.0;
                    }
                    if resp.double_clicked() {
                        apply_now = Some(name.clone());
                    }
                }
            },
        );
        let can = self.sequence.is_some() && !props.is_empty();
        if ui.add_enabled(can, egui::Button::new(tf!("entities.apply_sequence", props.len()))).clicked() {
            apply_now = self.sequence.clone();
        }
        if let Some(seq) = apply_now.filter(|_| !props.is_empty()) {
            let op = Op::SetKey {
                target: crate::view::Target::Entities(props),
                key: "DefaultAnim".to_owned(),
                value: seq,
                merge: false,
            };
            if let Err(e) = apply(&mut state.doc, None, op) {
                state.notify(Severity::Error, tf!("entities.edit_failed", e));
            }
        }
    }

    fn replace_uses_ui(&mut self, ui: &mut Ui, state: &mut AppState, path: &str) {
        let props = self.selected_props(state);
        let Some(old) = props
            .first()
            .and_then(|id| state.doc.scene().entity(*id))
            .and_then(|n| n.entity.get("model"))
            .map(str::to_owned)
        else {
            return;
        };
        if old.trim().is_empty() || old.trim().replace('\\', "/").eq_ignore_ascii_case(path) {
            return;
        }
        let n = self.uses.count(&state.doc, "model", &old);
        let btn = ui.button(tf!("entities.replace_uses", n)).on_hover_text(tf!(
            "entities.replace_uses_tip",
            old.trim(),
            path
        ));
        if btn.clicked() {
            let op = Op::ReplaceAllUses { key: "model".to_owned(), old, new: path.to_owned() };
            match apply(&mut state.doc, None, op) {
                Ok(_) => state.notify(Severity::Success, tf!("entities.replaced_uses", n)),
                Err(e) => state.notify(Severity::Error, tf!("entities.edit_failed", e)),
            }
        }
    }

    fn reset_camera(&mut self) {
        let skin = self.camera.skin;
        let c = self.camera;
        self.camera = PreviewCamera {
            skin,
            wireframe: c.wireframe,
            overlays: c.overlays,
            fov_deg: c.fov_deg,
            ..PreviewCamera::default()
        };
    }

    fn selected_props(&mut self, state: &AppState) -> Vec<EntityId> {
        let rev = DocStamp::of(&state.doc);
        if let Some((r, ids)) = &self.props_cache
            && *r == rev
        {
            return ids.clone();
        }
        let summary = SelectionSummary::of(&state.doc);
        let ids = prop_entities(&state.doc, state.assets.fgd(), &summary.entities);
        self.props_cache = Some((rev, ids.clone()));
        ids
    }
}

fn paint_tile(painter: &egui::Painter, rect: Rect, thumb: ThumbState) {
    match thumb {
        ThumbState::Ready { texture, size } => {
            let (w, h) = (size[0].max(1) as f32, size[1].max(1) as f32);
            let s = (rect.width() / w).min(rect.height() / h);
            let img = Rect::from_center_size(rect.center(), vec2(w * s, h * s));
            painter.image(texture, img, Rect::from_min_max(egui::pos2(0.0, 0.0), egui::pos2(1.0, 1.0)), Color32::WHITE);
        }
        ThumbState::Loading => {
            painter.rect_filled(rect, CornerRadius::same(2), Color32::from_gray(44));
        }
        ThumbState::Missing => {
            painter.rect_filled(rect, CornerRadius::same(2), Color32::from_gray(30));
            painter.text(
                rect.center(),
                egui::Align2::CENTER_CENTER,
                "?",
                egui::FontId::proportional(14.0),
                theme::warning(),
            );
        }
    }
}

const OVERLAY_OPTIONS: [(u32, Option<[u8; 3]>, &str); 11] = {
    use hp_app::state::preview_overlay as po;
    [
        (po::BBOX, Some([110, 200, 255]), "bbox"),
        (po::HULL, Some([255, 230, 80]), "hull"),
        (po::HITBOXES, Some([255, 90, 90]), "hitboxes"),
        (po::SEQ_BOX, Some([190, 115, 255]), "seq_box"),
        (po::BONES, Some([242, 242, 242]), "bones"),
        (po::ATTACHMENTS, Some([76, 255, 153]), "attachments"),
        (po::NORMALS, Some([102, 153, 255]), "normals"),
        (po::ORIGIN, None, "origin"),
        (po::ILLUM, Some([255, 217, 51]), "illum"),
        (po::GROUND, Some([140, 140, 153]), "ground"),
        (po::PHYSICS, None, "physics"),
    ]
};

pub(crate) fn preview_overlays(ui: &mut Ui, camera: &mut PreviewCamera, info: Option<&ModelInfo>) {
    use hp_app::state::preview_overlay as po;
    let available = |bit: u32| match info {
        None => true,
        Some(i) => match bit {
            po::HULL => i.hull_min.is_some(),
            po::HITBOXES => i.stats.hitboxes > 0,
            po::BONES => i.stats.bones > 0,
            po::SEQ_BOX => !i.sequences.is_empty(),
            _ => true,
        },
    };
    egui::CollapsingHeader::new(t!("entities.preview_options"))
        .id_salt(ui.scope_id().with("hp_ui_entities.preview_options"))
        .default_open(true)
        .show(ui, |ui| {
            let cols = 2;
            egui::Grid::new(ui.scope_id().with("hp_ui_entities.preview_overlays"))
                .num_columns(cols)
                .spacing(vec2(10.0, 2.0))
                .show(ui, |ui| {
                    let mut col = 0;
                    let mut end_cell = |ui: &mut Ui| {
                        col += 1;
                        if col % cols == 0 {
                            ui.end_row();
                        }
                    };
                    ui.horizontal(|ui| {
                        ui.add_space(12.0);
                        ui.checkbox(&mut camera.wireframe, t!("entities.wireframe"));
                    });
                    end_cell(ui);
                    for (bit, color, key) in OVERLAY_OPTIONS {
                        ui.horizontal(|ui| {
                            let (r, _) = ui.allocate_exact_size(vec2(8.0, 8.0), Sense::hover());
                            if let Some(c) = color {
                                ui.painter().rect_filled(r, 1.0, egui::Color32::from_rgb(c[0], c[1], c[2]));
                            }
                            let mut on = camera.shows(bit);
                            let label = t!(&format!("entities.overlay.{key}"));
                            let tip = t!(&format!("entities.overlay.{key}_tip"));
                            let resp = ui.add_enabled(available(bit), egui::Checkbox::new(&mut on, label));
                            if resp.on_hover_text(tip).changed() {
                                camera.overlays = if on { camera.overlays | bit } else { camera.overlays & !bit };
                            }
                        });
                        end_cell(ui);
                    }
                });
            ui.horizontal(|ui| {
                ui.label(t!("entities.preview_fov"));
                ui.add(egui::Slider::new(&mut camera.fov_deg, 10.0..=120.0).step_by(1.0).suffix("°"));
            });
        });
}

pub(crate) fn model_info_grid(ui: &mut Ui, info: &ModelInfo) {
    let st = &info.stats;
    let size = info.bounds_max - info.bounds_min;
    let row = |ui: &mut Ui, label: &str, value: String| {
        ui.label(label);
        ui.label(value);
        ui.end_row();
    };
    let with_all = |n: u32, all: u32| {
        if all > n { tf!("entities.count_with_all", n, all) } else { n.to_string() }
    };
    egui::Grid::new("hp_ui_entities.model_info").num_columns(2).spacing(vec2(10.0, 2.0)).striped(true).show(ui, |ui| {
        row(ui, t!("common.size"), hp_app::widgets::fmt_vec3(size));
        row(ui, t!("entities.bbox_min"), hp_app::widgets::fmt_vec3(info.bounds_min));
        row(ui, t!("entities.bbox_max"), hp_app::widgets::fmt_vec3(info.bounds_max));
        if let (Some(lo), Some(hi)) = (info.hull_min, info.hull_max) {
            row(ui, t!("entities.hull"), hp_app::widgets::fmt_vec3(hi - lo));
        }
        row(ui, t!("entities.triangles"), with_all(info.triangles, st.triangles_all));
        row(ui, t!("entities.vertices"), with_all(st.vertices, st.vertices_all));
        row(ui, t!("entities.meshes"), st.meshes.to_string());
        row(ui, t!("entities.materials"), info.materials.len().to_string());
        row(ui, t!("entities.bones"), st.bones.to_string());
        row(ui, t!("entities.hitboxes"), st.hitboxes.to_string());
        row(ui, t!("entities.lods"), st.lods.max(1).to_string());
        row(ui, t!("entities.skins"), info.skins.to_string());
        row(ui, t!("entities.body_parts"), info.body_parts.to_string());
        row(ui, t!("entities.sequences"), info.sequences.len().to_string());
        row(
            ui,
            t!("entities.static_prop"),
            if info.is_static_prop { t!("common.yes") } else { t!("common.no") }.to_owned(),
        );
        row(
            ui,
            t!("entities.surface_prop"),
            if info.surface_prop.is_empty() { "-".to_owned() } else { info.surface_prop.clone() },
        );
        if st.mass > 0.0 && st.mass.is_finite() {
            row(ui, t!("entities.mass"), format!("{:.1} kg", st.mass));
        }
        if st.version > 0 {
            row(ui, t!("entities.mdl_version"), st.version.to_string());
        }
    });
    if !info.materials.is_empty() {
        egui::CollapsingHeader::new(tf!("entities.materials_count", info.materials.len()))
            .id_salt("hp_ui_entities.model_materials")
            .default_open(false)
            .show(ui, |ui| {
                for m in &info.materials {
                    ui.add(egui::Label::new(RichText::new(m).small()).truncate());
                }
            });
    }
}

fn use_for_new_props(ctx: &egui::Context, state: &mut AppState, path: &str, skin: u32) {
    let is_static = match state.assets.model_info(path) {
        ModelState::Ready(i) => i.is_static_prop,
        _ => true,
    };
    state.current_model = Some(path.to_owned());
    state.current_model_skin = skin;
    state.current_entity_class = if is_static { "prop_static" } else { "prop_physics" }.to_owned();
    state.set_tool(ToolId::Entity);
    push_recent_model(ctx, path);
}

fn picked_model(doc: &Document, pick: &crate::cx::ModelPick) -> Option<String> {
    let value = match &pick.target {
        crate::view::Target::World => doc.scene().world().get(&pick.key),
        crate::view::Target::Entities(ids) => doc.scene().entity(*ids.first()?)?.entity.get(&pick.key),
    }?;
    let path = value.trim().replace('\\', "/").to_ascii_lowercase();
    (!path.is_empty()).then_some(path)
}

pub(crate) fn model_keys(key: &str, path: &str, skin: u32) -> Vec<(String, String)> {
    let mut keys = vec![(key.to_owned(), path.to_owned())];
    if skin > 0 && key.eq_ignore_ascii_case("model") {
        keys.push(("skin".to_owned(), skin.to_string()));
    }
    keys
}

fn apply_pick(ctx: &egui::Context, state: &mut AppState, pick: &crate::cx::ModelPick, path: &str, skin: u32) {
    let op = Op::SetKeys { target: pick.target.clone(), keys: model_keys(&pick.key, path, skin) };
    match apply(&mut state.doc, None, op) {
        Ok(_) => {
            push_recent_model(ctx, path);
            set_model_pick(ctx, None);
            state.ui.right_tab = RightTab::Entity;
        }
        Err(e) => state.notify(Severity::Error, tf!("entities.edit_failed", e)),
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn entries(names: &[&str]) -> Vec<AssetEntry> {
        names.iter().map(|n| AssetEntry { name: (*n).to_owned(), source: 0 }).collect()
    }

    #[test]
    fn index_groups_by_folder() {
        let models = entries(&[
            "models/a/b.mdl",
            "models/a/c/d.mdl",
            "models/a/e.mdl",
            "models/z.mdl",
            "models/props_c17/oildrum001.mdl",
        ]);
        let (order, top) = index_models(&models);
        let names: Vec<&str> = order.iter().map(|&i| models[i as usize].name.as_str()).collect();
        assert_eq!(
            names,
            ["models/z.mdl", "models/a/b.mdl", "models/a/e.mdl", "models/a/c/d.mdl", "models/props_c17/oildrum001.mdl"]
        );
        assert_eq!(top, ["a", "props_c17"]);
    }

    #[test]
    fn rows_filter_and_collapse() {
        let models = entries(&["models/a/b.mdl", "models/a/barrel.mdl", "models/c/barrel2.mdl", "models/c/x.mdl"]);
        let (order, _) = index_models(&models);
        let none = HashSet::new();
        let rows = build_rows(&models, &order, "", None, &none);
        assert_eq!(rows.len(), 6);
        assert!(matches!(rows[0], Row::Folder { count: 2, .. }));
        let rows = build_rows(&models, &order, "BARREL", None, &none);
        assert_eq!(rows.len(), 4);
        let rows = build_rows(&models, &order, "barrel", Some("c"), &none);
        assert_eq!(rows.len(), 2);
        let mut collapsed = HashSet::new();
        collapsed.insert("models/a".to_owned());
        let rows = build_rows(&models, &order, "", None, &collapsed);
        assert_eq!(rows.len(), 4, "collapsed folder keeps only its header");
        assert!(matches!(rows[0], Row::Folder { count: 2, .. }));
        assert!(build_rows(&models, &order, "nothing", None, &none).is_empty());
    }

    #[test]
    fn negation_and_source_filters() {
        let models = entries(&["models/office/vent.mdl", "models/office/vent_dynamic.mdl", "models/x/vent.mdl"]);
        let (order, _) = index_models(&models);
        let none = HashSet::new();
        let rows = build_rows(&models, &order, "vent office !dynamic", None, &none);
        assert_eq!(rows.iter().filter(|r| matches!(r, Row::Model(_))).count(), 1);
        let rows = build_rows_where(&models, &order, "", None, &none, &|_| false);
        assert!(rows.is_empty(), "every source filtered out");
    }

    #[test]
    fn body_values_round_trip() {
        let g = |name: &str, base: i32, n: usize| hp_app::state::BodyGroupInfo {
            name: name.to_owned(),
            base,
            choices: (0..n).map(|i| format!("c{i}")).collect(),
        };
        let groups = [g("head", 1, 3), g("hat", 3, 2), g("single", 6, 1)];
        assert_eq!(body_value(&groups, &[2, 1, 0]), 5);
        assert_eq!(body_choices(&groups, 5), [2, 1, 0]);
        assert_eq!(body_value(&groups, &[9, 9, 9]), 2 + 3, "choices are clamped");
        assert_eq!(body_choices(&groups, -4), [0, 0, 0]);
        assert_eq!(body_value(&[], &[]), 0);
    }

    #[test]
    fn grid_mode_draws() {
        crate::register_translations();
        let mut state = hp_app::testing::test_state();
        state.settings.browser.model_grid = true;
        let mut b = ModelBrowser::new();
        for _ in 0..3 {
            hp_app::testing::run_ui(&mut state, |ui, state| b.ui(ui, state));
            std::thread::sleep(std::time::Duration::from_millis(20));
        }
        b.select("models/props_c17/oildrum001.mdl");
        for tab in [DetailTab::BodyGroups, DetailTab::Sequences, DetailTab::Inspector, DetailTab::Info] {
            b.tab = tab;
            hp_app::testing::run_ui(&mut state, |ui, state| b.ui(ui, state));
        }
        assert_eq!(state.doc.history_len().0, 0);
    }

    #[test]
    fn case_insensitive_contains() {
        assert!(contains_ignore_case("models/Props/Barrel.mdl", "barrel"));
        assert!(contains_ignore_case("abc", ""));
        assert!(!contains_ignore_case("ab", "abc"));
    }

    #[test]
    fn applying_a_model_writes_the_chosen_skin() {
        let model = |k: &str, v: &str| (k.to_owned(), v.to_owned());
        assert_eq!(model_keys("model", "models/a.mdl", 0), [model("model", "models/a.mdl")]);
        assert_eq!(model_keys("Model", "models/a.mdl", 3), [model("Model", "models/a.mdl"), model("skin", "3")]);
        assert_eq!(model_keys("gibmodel", "models/a.mdl", 3), [model("gibmodel", "models/a.mdl")]);

        let mut state = hp_app::testing::test_state();
        let ctx = egui::Context::default();
        let prop = state.doc.create_point_entity("prop_static", glam::DVec3::ZERO, &[]).unwrap();
        state.doc.clear_history();
        let pick = crate::cx::ModelPick {
            target: crate::view::Target::Entities(vec![prop]),
            key: "model".into(),
            label: "prop_static (model)".into(),
        };
        set_model_pick(&ctx, Some(pick.clone()));
        state.doc.set_entity_key(&[prop], "model", r"Models\Props_C17\OilDrum001.mdl").unwrap();
        assert_eq!(picked_model(&state.doc, &pick).as_deref(), Some("models/props_c17/oildrum001.mdl"));
        state.doc.clear_history();
        apply_pick(&ctx, &mut state, &pick, "models/props_c17/oildrum001.mdl", 2);
        let e = &state.doc.scene().entity(prop).unwrap().entity;
        assert_eq!(e.get("model"), Some("models/props_c17/oildrum001.mdl"));
        assert_eq!(e.get("skin"), Some("2"));
        assert_eq!(state.doc.history_len().0, 1, "model and skin are one undo step");
        assert_eq!(model_pick(&ctx), None, "the request is done");
        assert_eq!(state.ui.right_tab, RightTab::Entity);
    }

    #[test]
    fn use_for_new_props_sets_the_model_and_skin() {
        let mut state = hp_app::testing::test_state();
        let ctx = egui::Context::default();
        use_for_new_props(&ctx, &mut state, "models/props_c17/oildrum001.mdl", 2);
        assert_eq!(state.current_model.as_deref(), Some("models/props_c17/oildrum001.mdl"));
        assert_eq!(state.current_model_skin, 2);
        assert_eq!(state.current_entity_class, "prop_static");
        assert_eq!(state.requested_tool, Some(ToolId::Entity));
        use_for_new_props(&ctx, &mut state, "models/props_junk/wood_crate001a.mdl", 0);
        assert_eq!(state.current_model.as_deref(), Some("models/props_junk/wood_crate001a.mdl"));
        assert_eq!(state.current_model_skin, 0);
    }

    #[test]
    fn selecting_another_model_resets_the_preview_skin() {
        let mut browser = ModelBrowser::new();
        browser.select("models/a.mdl");
        browser.set_preview_skin(3);
        browser.select("models/a.mdl");
        assert_eq!(browser.preview_skin(), 3, "the first click of a double click keeps the skin");
        browser.select("models/b.mdl");
        assert_eq!(browser.preview_skin(), 0);
    }

    #[test]
    fn escape_and_the_close_button_dismiss_the_model_details() {
        crate::register_translations();
        let mut state = hp_app::testing::test_state();
        let mut browser = ModelBrowser::new();
        let ctx = egui::Context::default();
        let frame =
            |ctx: &egui::Context, browser: &mut ModelBrowser, state: &mut AppState, events: Vec<egui::Event>| {
                let input = egui::RawInput {
                    screen_rect: Some(Rect::from_min_size(egui::Pos2::ZERO, vec2(400.0, 900.0))),
                    events,
                    ..Default::default()
                };
                let mut out = ctx.run_ui(input, |ui| browser.ui(ui, state));
                out.textures_delta.clear();
            };
        browser.select("models/a.mdl");
        let hover = egui::Event::PointerMoved(egui::pos2(200.0, 450.0));
        frame(&ctx, &mut browser, &mut state, vec![hover]);
        assert!(browser.selected().is_some());
        let esc = egui::Event::Key {
            key: Key::Escape,
            physical_key: None,
            pressed: true,
            repeat: false,
            modifiers: Modifiers::NONE,
        };
        frame(&ctx, &mut browser, &mut state, vec![esc]);
        assert!(browser.selected().is_none(), "Escape dismisses the details");
    }

    #[test]
    fn prop_detection() {
        let state = hp_app::testing::test_state();
        let fgd = crate::test_fgd::test_fgd();
        let mut doc = state.doc;
        let prop = doc.create_point_entity("prop_static", glam::DVec3::ZERO, &[]).unwrap();
        let dynamic = doc.create_point_entity("prop_dynamic_override", glam::DVec3::ZERO, &[]).unwrap();
        let ids = [prop, dynamic, 5, 4];
        assert_eq!(prop_entities(&doc, Some(&fgd), &ids), [prop, dynamic]);
        assert_eq!(prop_entities(&doc, None, &ids), [prop, dynamic]);
    }
}
