mod create;
mod library;
mod render;
mod thumbs;

use std::path::PathBuf;
use std::sync::Arc;
use std::sync::mpsc::{Receiver, TryRecvError};

use egui::{Align2, Color32, FontId, Rect, Sense, Stroke, StrokeKind, Ui, vec2};
use hp_app::dnd::DragPayload;
use hp_app::{Action, AppState, t, tf};

use crate::dialogs::MapDialog;
use library::{Library, PrefabEntry, PrefabRoot};
use thumbs::Thumbs;

pub(crate) fn dialogs() -> Vec<Box<dyn MapDialog>> {
    vec![Box::new(create::CreatePrefabDialog::default())]
}

const LABEL_HEIGHT: f32 = 18.0;
const GAP: f32 = 6.0;

type ShownKey = (String, Option<(usize, String)>, usize);

pub struct PrefabTab {
    library: Arc<Library>,
    scanning: Option<Receiver<Library>>,
    scanned: Option<(Vec<PrefabRoot>, u64)>,
    search: String,
    category: Option<(usize, String)>,
    shown: Vec<usize>,
    shown_key: Option<ShownKey>,
    thumbs: Thumbs,
}

impl Default for PrefabTab {
    fn default() -> Self {
        Self::new()
    }
}

impl PrefabTab {
    pub fn new() -> Self {
        Self::with_thumbs(Thumbs::default())
    }

    fn with_thumbs(thumbs: Thumbs) -> Self {
        Self {
            library: Arc::new(Library::default()),
            scanning: None,
            scanned: None,
            search: String::new(),
            category: None,
            shown: Vec::new(),
            shown_key: None,
            thumbs,
        }
    }

    pub fn is_scanning(&self) -> bool {
        self.scanning.is_some()
    }

    pub fn len(&self) -> usize {
        self.library.entries.len()
    }

    pub fn is_empty(&self) -> bool {
        self.library.entries.is_empty()
    }

    pub fn shown_paths(&self) -> Vec<PathBuf> {
        self.shown.iter().filter_map(|i| self.library.entries.get(*i)).map(|e| e.path.clone()).collect()
    }

    pub fn set_search(&mut self, text: &str) {
        text.clone_into(&mut self.search);
    }

    fn update_library(&mut self, ctx: &egui::Context, state: &mut AppState) {
        if let Some(rx) = &self.scanning {
            match rx.try_recv() {
                Ok(lib) => {
                    self.scanning = None;
                    for e in &lib.entries {
                        state.runtime.placement.prefabs.note_stamp(&e.path, e.stamp);
                    }
                    if let Some(sel) = &state.runtime.placement.selected_prefab
                        && !lib.entries.iter().any(|e| &e.path == sel)
                    {
                        state.runtime.placement.selected_prefab = None;
                    }
                    if self.category.as_ref().is_some_and(|c| !lib.categories.contains(c)) {
                        self.category = None;
                    }
                    self.library = Arc::new(lib);
                    self.shown_key = None;
                }
                Err(TryRecvError::Empty) => {}
                Err(TryRecvError::Disconnected) => self.scanning = None,
            }
        }
        let roots = library::roots(&state.settings);
        let serial = state.runtime.placement.library_serial;
        let current = self.scanned.as_ref().is_some_and(|(r, s)| *r == roots && *s == serial);
        if !current && self.scanning.is_none() {
            self.scanned = Some((roots.clone(), serial));
            let (tx, rx) = std::sync::mpsc::channel();
            let repaint = ctx.clone();
            let spawned = std::thread::Builder::new().name("hp-prefab-scan".to_owned()).spawn(move || {
                hp_sched::lower_current_thread(hp_sched::Level::BelowNormal);
                let _ = tx.send(library::scan(roots));
                repaint.request_repaint();
            });
            match spawned {
                Ok(_) => self.scanning = Some(rx),
                Err(e) => log::warn!("cannot scan the prefab library: {e}"),
            }
        }
    }

    fn update_shown(&mut self) {
        let key = (self.search.trim().to_lowercase(), self.category.clone(), Arc::as_ptr(&self.library) as usize);
        if self.shown_key.as_ref() == Some(&key) {
            return;
        }
        let words: Vec<&str> = key.0.split_whitespace().collect();
        self.shown = self
            .library
            .entries
            .iter()
            .enumerate()
            .filter(|(_, e)| self.category.as_ref().is_none_or(|(r, f)| e.root == *r && e.folder == *f))
            .filter(|(_, e)| words.iter().all(|w| e.search_key.contains(w)))
            .map(|(i, _)| i)
            .collect();
        self.shown_key = Some(key);
    }

    fn category_label(lib: &Library, root: usize, folder: &str) -> String {
        let root_label = lib.roots.get(root).map_or("?", |r| t!(r.label_key));
        if folder.is_empty() { root_label.to_owned() } else { format!("{root_label} / {folder}") }
    }

    pub fn ui(&mut self, ui: &mut Ui, state: &mut AppState) {
        let ctx = ui.ctx().clone();
        self.update_library(&ctx, state);
        self.thumbs.poll(&ctx);
        state.runtime.placement.prefabs.poll();
        self.toolbar(ui, state);
        self.update_shown();
        ui.separator();
        let lib = Arc::clone(&self.library);
        if lib.entries.is_empty() {
            self.empty_ui(ui, state);
            return;
        }
        if self.shown.is_empty() {
            ui.weak(t!("prefabs.no_match"));
            return;
        }
        self.grid(ui, state, &lib);
    }

    fn toolbar(&mut self, ui: &mut Ui, state: &mut AppState) {
        ui.horizontal(|ui| {
            if ui
                .add_enabled(!state.doc.selection().objects.is_empty(), egui::Button::new("➕").small())
                .on_hover_text(t!("prefabs.create_tip"))
                .on_disabled_hover_text(t!("prefabs.create_tip"))
                .clicked()
            {
                state.request(Action::CreatePrefab);
            }
            if ui.small_button("🔄").on_hover_text(t!("prefabs.refresh_tip")).clicked() {
                state.runtime.placement.library_serial = state.runtime.placement.library_serial.wrapping_add(1);
            }
            if ui.small_button("📂").on_hover_text(t!("prefabs.open_folder_tip")).clicked() {
                state.runtime.placement.open_path = Some(state.settings.placement.library_dir());
            }
            let mut tile = state.settings.placement.tile_size();
            let r = ui
                .add(
                    egui::Slider::new(&mut tile, hp_app::settings::PlacementSettings::TILE_RANGE)
                        .show_value(false)
                        .step_by(8.0),
                )
                .on_hover_text(t!("prefabs.tile_size_tip"));
            if r.changed() {
                state.settings.placement.prefab_tile = tile;
                state.settings_dirty = true;
            }
            if self.scanning.is_some() {
                hp_app::widgets::spinner(ui).on_hover_text(t!("prefabs.scanning"));
            }
        });
        ui.horizontal(|ui| {
            let width = (ui.available_width() - 16.0).max(80.0);
            ui.allocate_ui(vec2(width, ui.spacing().interact_size.y), |ui| {
                hp_app::widgets::search_box(ui, &mut self.search, t!("prefabs.search"));
            });
        });
        let lib = Arc::clone(&self.library);
        if !lib.categories.is_empty() {
            let shown = match &self.category {
                Some((r, f)) => Self::category_label(&lib, *r, f),
                None => t!("prefabs.all").to_owned(),
            };
            egui::ComboBox::from_id_salt("hp_prefabs_category")
                .selected_text(shown)
                .width(ui.available_width().min(320.0))
                .show_ui(ui, |ui| {
                    ui.selectable_value(&mut self.category, None, t!("prefabs.all"));
                    for (r, f) in &lib.categories {
                        let label = Self::category_label(&lib, *r, f);
                        ui.selectable_value(&mut self.category, Some((*r, f.clone())), label);
                    }
                });
        }
    }

    fn empty_ui(&mut self, ui: &mut Ui, state: &mut AppState) {
        if self.scanning.is_some() {
            ui.horizontal(|ui| {
                hp_app::widgets::spinner(ui);
                ui.weak(t!("prefabs.scanning"));
            });
            return;
        }
        ui.add_space(8.0);
        ui.label(t!("prefabs.empty"));
        match state.keys.shortcut(Action::CreatePrefab) {
            Some(sc) => ui.label(tf!("prefabs.empty_create", hp_app::actions::shortcut_label(&sc))),
            None => ui.label(t!("prefabs.empty_create_menu")),
        };
        ui.add_space(4.0);
        ui.label(egui::RichText::new(state.settings.placement.library_dir().display().to_string()).weak().small());
    }

    fn grid(&mut self, ui: &mut Ui, state: &mut AppState, lib: &Library) {
        let tile = state.settings.placement.tile_size();
        let bar = ui.spacing().scroll.allocated_width().max(ui.spacing().scroll.bar_width) + 2.0;
        let width = (ui.available_width() - bar).max(tile);
        let cols = (((width + GAP) / (tile + GAP)).floor() as usize).max(1);
        let rows = self.shown.len().div_ceil(cols);
        let row_h = tile + LABEL_HEIGHT + GAP;
        let ctx = ui.ctx().clone();
        egui::ScrollArea::vertical().id_salt("hp_prefabs_grid").auto_shrink([false, false]).show_rows(
            ui,
            row_h,
            rows,
            |ui, range| {
                for row in range {
                    ui.horizontal(|ui| {
                        ui.spacing_mut().item_spacing.x = GAP;
                        for col in 0..cols {
                            let Some(&i) = self.shown.get(row * cols + col) else { break };
                            let Some(entry) = lib.entries.get(i) else { continue };
                            self.tile(ui, &ctx, state, lib, entry, tile);
                        }
                    });
                }
            },
        );
        ui.horizontal(|ui| {
            let mut text = tf!("prefabs.count", self.shown.len(), lib.entries.len());
            if lib.truncated {
                text.push_str(&format!("  {}", tf!("prefabs.truncated", library::MAX_FILES)));
            }
            ui.weak(text);
        });
    }

    fn tile(
        &mut self,
        ui: &mut Ui,
        ctx: &egui::Context,
        state: &mut AppState,
        lib: &Library,
        e: &PrefabEntry,
        tile: f32,
    ) {
        let (rect, resp) = ui.allocate_exact_size(vec2(tile, tile + LABEL_HEIGHT), Sense::click_and_drag());
        let rt = &state.runtime.placement;
        let selected = rt.selected_prefab.as_ref() == Some(&e.path);
        let armed = rt.armed_prefab.as_ref() == Some(&e.path);
        if ui.is_rect_visible(rect) {
            let visuals = ui.visuals();
            let image_rect = Rect::from_min_size(rect.min, vec2(tile, tile));
            let bg = if selected { visuals.selection.bg_fill.gamma_multiply(0.45) } else { visuals.extreme_bg_color };
            let painter = ui.painter();
            painter.rect_filled(image_rect, 4.0, bg);
            match self.thumbs.get(ctx, &e.path, e.stamp) {
                Some(tex) => {
                    let uv = Rect::from_min_max(egui::pos2(0.0, 0.0), egui::pos2(1.0, 1.0));
                    painter.image(tex, image_rect.shrink(3.0), uv, Color32::WHITE);
                }
                None => {
                    let glyph = if self.thumbs.is_missing(&e.path) { "⊞" } else { "⟳" };
                    painter.text(
                        image_rect.center(),
                        Align2::CENTER_CENTER,
                        glyph,
                        FontId::proportional(tile * 0.28),
                        visuals.weak_text_color(),
                    );
                }
            }
            let stroke = if armed {
                Stroke::new(2.0, hp_app::theme::accent())
            } else if selected {
                Stroke::new(1.5, visuals.selection.stroke.color)
            } else if resp.hovered() {
                visuals.widgets.hovered.bg_stroke
            } else {
                Stroke::NONE
            };
            painter.rect_stroke(image_rect, 4.0, stroke, StrokeKind::Inside);
            let label_rect = Rect::from_min_size(egui::pos2(rect.min.x, rect.min.y + tile), vec2(tile, LABEL_HEIGHT));
            let galley = painter.layout(e.name.clone(), FontId::proportional(11.5), visuals.text_color(), tile - 2.0);
            let pos = egui::pos2(label_rect.center().x - galley.size().x.min(tile) * 0.5, label_rect.min.y + 2.0);
            painter.with_clip_rect(label_rect).galley(pos, galley, visuals.text_color());
        }
        if resp.drag_started() {
            egui::DragAndDrop::set_payload(ctx, DragPayload::Prefab(e.path.clone()));
            state.runtime.placement.selected_prefab = Some(e.path.clone());
        }
        if resp.dragged() {
            ctx.set_cursor_icon(egui::CursorIcon::Grabbing);
        }
        let category = Self::category_label(lib, e.root, &e.folder);
        let resp = resp.on_hover_text(format!("{}\n{}\n{}", e.name, category, e.path.display()));
        if resp.clicked() {
            state.runtime.placement.selected_prefab = Some(e.path.clone());
        }
        if resp.double_clicked() {
            arm(ctx, state, &e.path);
        }
        resp.context_menu(|ui| {
            if ui.button(t!("prefabs.menu.place")).clicked() {
                arm(ctx, state, &e.path);
                ui.close();
            }
            if ui.button(t!("prefabs.menu.insert")).clicked() {
                state.runtime.placement.selected_prefab = Some(e.path.clone());
                state.request(Action::InsertOriginalPrefab);
                ui.close();
            }
            if ui.button(t!("prefabs.menu.show")).clicked() {
                state.runtime.placement.open_path = e.path.parent().map(std::path::Path::to_path_buf);
                ui.close();
            }
        });
    }
}

pub fn arm(ctx: &egui::Context, state: &mut AppState, path: &std::path::Path) {
    let rt = &mut state.runtime.placement;
    rt.selected_prefab = Some(path.to_path_buf());
    rt.armed_prefab = Some(path.to_path_buf());
    rt.prefabs.refresh(path, Some(ctx));
    state.request(Action::ToolEntity);
}

#[cfg(test)]
mod tests {
    use super::*;

    const BOX: &str = "solid { side { \"plane\" \"(0 64 64) (64 64 64) (64 0 64)\" \"material\" \"A\" } side { \"plane\" \"(0 0 0) (64 0 0) (64 64 0)\" \"material\" \"A\" } side { \"plane\" \"(0 64 64) (0 0 64) (0 0 0)\" \"material\" \"A\" } side { \"plane\" \"(64 64 0) (64 0 0) (64 0 64)\" \"material\" \"A\" } side { \"plane\" \"(64 64 64) (0 64 64) (0 64 0)\" \"material\" \"A\" } side { \"plane\" \"(64 0 0) (0 0 0) (0 0 64)\" \"material\" \"A\" } }";

    #[test]
    fn tab_scans_searches_and_arms() {
        let _lang = crate::lang_lock();
        crate::register_translations();
        let dir = crate::test_scratch("prefab_tab");
        std::fs::create_dir_all(dir.join("lib/doors")).expect("dir");
        std::fs::write(dir.join("lib/crate.vmf"), BOX).expect("w");
        std::fs::write(dir.join("lib/doors/Metal Door.vmf"), BOX).expect("w");
        let mut state = hp_app::testing::test_state();
        state.settings.placement.prefab_library = Some(dir.join("lib"));
        state.settings.placement.game_prefabs = false;
        let mut tab = PrefabTab::with_thumbs(Thumbs::new(Some(dir.join("cache"))));
        let deadline = std::time::Instant::now() + std::time::Duration::from_secs(20);
        loop {
            hp_app::testing::run_ui(&mut state, |ui, state| tab.ui(ui, state));
            if !tab.is_scanning() && tab.len() == 2 {
                break;
            }
            assert!(std::time::Instant::now() < deadline, "scan finishes");
            std::thread::sleep(std::time::Duration::from_millis(5));
        }
        assert_eq!(tab.shown_paths().len(), 2);
        tab.set_search("METAL");
        hp_app::testing::run_ui(&mut state, |ui, state| tab.ui(ui, state));
        assert_eq!(tab.shown_paths(), [dir.join("lib").join("doors").join("Metal Door.vmf")]);
        tab.set_search("nothing like this");
        hp_app::testing::run_ui(&mut state, |ui, state| tab.ui(ui, state));
        assert!(tab.shown_paths().is_empty());
        tab.set_search("");
        hp_app::i18n::set_lang(hp_app::i18n::Lang::Fr);
        hp_app::testing::run_ui(&mut state, |ui, state| tab.ui(ui, state));
        hp_app::i18n::set_lang(hp_app::i18n::Lang::En);
        let ctx = egui::Context::default();
        let path = dir.join("lib").join("crate.vmf");
        arm(&ctx, &mut state, &path);
        assert_eq!(state.runtime.placement.armed_prefab.as_ref(), Some(&path));
        assert_eq!(state.requested_actions, vec![Action::ToolEntity]);
        std::fs::write(dir.join("lib/new.vmf"), BOX).expect("w");
        state.runtime.placement.library_serial += 1;
        loop {
            hp_app::testing::run_ui(&mut state, |ui, state| tab.ui(ui, state));
            if !tab.is_scanning() && tab.len() == 3 {
                break;
            }
            assert!(std::time::Instant::now() < deadline, "rescan finishes");
            std::thread::sleep(std::time::Duration::from_millis(5));
        }
        let _ = std::fs::remove_dir_all(&dir);
    }

    #[test]
    fn the_dock_keeps_its_width() {
        let _lang = crate::lang_lock();
        crate::register_translations();
        let dir = crate::test_scratch("prefab_tab_width");
        std::fs::create_dir_all(dir.join("lib")).expect("dir");
        for i in 0..12 {
            std::fs::write(dir.join("lib").join(format!("prefab with a long name {i}.vmf")), BOX).expect("w");
        }
        let mut state = hp_app::testing::test_state();
        state.settings.placement.prefab_library = Some(dir.join("lib"));
        state.settings.placement.game_prefabs = false;
        let mut tab = PrefabTab::with_thumbs(Thumbs::new(None));
        let ctx = egui::Context::default();
        let mut widths = Vec::new();
        for frame in 0..400 {
            let input = egui::RawInput {
                screen_rect: Some(Rect::from_min_size(egui::Pos2::ZERO, vec2(1600.0, 900.0))),
                ..Default::default()
            };
            let mut out = ctx.run_ui(input, |ui| {
                let r = egui::Panel::right("hp_test_dock")
                    .resizable(true)
                    .default_size(380.0)
                    .show(ui, |ui| tab.ui(ui, &mut state));
                widths.push(r.response.rect.width());
            });
            out.textures_delta.clear();
            if frame > 20 && !tab.is_scanning() && tab.len() == 12 && widths.len() > 40 {
                break;
            }
            std::thread::sleep(std::time::Duration::from_millis(2));
        }
        assert_eq!(tab.len(), 12);
        let last = widths.iter().rev().take(20).copied().collect::<Vec<f32>>();
        assert!(last.iter().all(|w| (w - last[0]).abs() < 0.5), "the dock grows: {widths:?}");
        assert!(last[0] < 420.0, "{}", last[0]);
        let _ = std::fs::remove_dir_all(&dir);
    }

    #[test]
    fn glyphs_exist() {
        let ctx = egui::Context::default();
        let mut out = ctx.run_ui(egui::RawInput::default(), |_| {});
        out.textures_delta.clear();
        let font = egui::FontId::proportional(14.0);
        for g in ["➕", "🔄", "📂", "⊞", "⟳", "✖", "⌖"] {
            assert!(ctx.fonts_mut(|f| f.has_glyphs(&font, g)), "missing glyph {g}");
        }
    }

    #[test]
    fn empty_library_explains_how_to_start() {
        let _lang = crate::lang_lock();
        crate::register_translations();
        let dir = crate::test_scratch("prefab_tab_empty");
        let mut state = hp_app::testing::test_state();
        state.settings.placement.prefab_library = Some(dir.join("none"));
        state.settings.placement.game_prefabs = false;
        let mut tab = PrefabTab::with_thumbs(Thumbs::new(None));
        for _ in 0..200 {
            hp_app::testing::run_ui(&mut state, |ui, state| tab.ui(ui, state));
            if !tab.is_scanning() && tab.scanned.is_some() {
                break;
            }
            std::thread::sleep(std::time::Duration::from_millis(5));
        }
        assert!(tab.is_empty());
        let _ = std::fs::remove_dir_all(&dir);
    }
}
