use std::path::PathBuf;
use std::sync::Arc;

use crossbeam_channel::Receiver;
use egui::{RichText, ScrollArea, Ui};
use hp_app::runtime::soundscape::PreviewRequest;
use hp_app::{AppState, Severity, t, tf, theme};
use hp_editor::ObjRef;
use hp_kv::Block;
use hp_soundscape::library::{Library, Origin};
use hp_soundscape::rules::EmitterKind;
use hp_soundscape::script::{self, Interval, RULE_LOOPING, RULE_NESTED, RULE_RANDOM, RULES};
use hp_soundscape::soundlevel;

use super::hearing::SoundCache;
use super::world::{self, MapFile};
use crate::media;

const MAX_ITEMS: usize = 4096;
const MAX_LIST: usize = 2000;
const LABEL_W: f32 = 120.0;

#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
enum Filter {
    #[default]
    All,
    Map,
    Game,
}

#[derive(Clone, Debug, PartialEq, Eq)]
enum Sel {
    Map(usize),
    Game(String),
}

#[derive(Default)]
pub struct SoundscapeEditor {
    key: Option<(u64, Option<String>, Option<PathBuf>)>,
    seen_serial: u64,
    game: Arc<Vec<(String, String)>>,
    game_lib: Arc<Library>,
    combined: Option<(u64, Arc<Library>)>,
    map: MapFile,
    root: Option<PathBuf>,
    dirty: bool,
    selected: Option<Sel>,
    search: String,
    filter: Filter,
    new_name: String,
    status: Option<(Severity, String)>,
    pick_dir: Option<Receiver<Option<PathBuf>>>,
    picker: Option<(usize, usize)>,
    wave_search: String,
    sounds: SoundCache,
}

fn interval_field(
    ui: &mut Ui,
    block: &mut Block,
    key: &str,
    label: &str,
    default: Interval,
    range: (f32, f32),
    speed: f32,
) -> bool {
    let present = block.get(key).is_some();
    let mut v = block.get(key).and_then(Interval::parse).unwrap_or(default);
    let mut ranged = !v.is_fixed();
    let mut changed = false;
    ui.horizontal(|ui| {
        ui.add_sized([LABEL_W, ui.spacing().interact_size.y], egui::Label::new(label).truncate());
        let decimals = if speed < 0.1 { 2 } else { 0 };
        changed |= ui
            .add(egui::DragValue::new(&mut v.min).range(range.0..=range.1).speed(speed).fixed_decimals(decimals))
            .changed();
        if ranged {
            ui.label(t!("soundscape.ed.to"));
            changed |= ui
                .add(egui::DragValue::new(&mut v.max).range(range.0..=range.1).speed(speed).fixed_decimals(decimals))
                .changed();
        }
        if ui
            .checkbox(&mut ranged, t!("soundscape.ed.random_range"))
            .on_hover_text(t!("soundscape.ed.random_range_tip"))
            .changed()
        {
            changed = true;
            if !ranged {
                v.max = v.min;
            }
        }
        if !present {
            ui.label(RichText::new(t!("soundscape.ed.default")).small().weak());
        }
    });
    if changed {
        if !ranged {
            v.max = v.min;
        }
        let fixed = Interval { min: v.min.min(v.max), max: v.min.max(v.max) };
        block.set(key, fixed.to_text());
    }
    changed
}

fn soundlevel_field(ui: &mut Ui, block: &mut Block, salt: usize) -> bool {
    let current = block.get("soundlevel").map(str::trim).unwrap_or("").to_owned();
    let label = if current.is_empty() { t!("soundscape.ed.level_default").to_owned() } else { current.clone() };
    let mut changed = false;
    ui.horizontal(|ui| {
        ui.add_sized(
            [LABEL_W, ui.spacing().interact_size.y],
            egui::Label::new(t!("soundscape.ed.soundlevel")).truncate(),
        );
        egui::ComboBox::from_id_salt(("ss_level", salt)).selected_text(label).width(170.0).show_ui(ui, |ui| {
            if ui.selectable_label(current.is_empty(), t!("soundscape.ed.level_default")).clicked() {
                block.remove("soundlevel");
                changed = true;
            }
            for (name, db) in soundlevel::names() {
                if ui.selectable_label(current.eq_ignore_ascii_case(name), format!("{name}  ({db:.0} dB)")).clicked() {
                    block.set("soundlevel", name);
                    changed = true;
                }
            }
        });
    })
    .response
    .on_hover_text(t!("soundscape.ed.soundlevel_tip"));
    changed
}

fn position_field(ui: &mut Ui, block: &mut Block, key: &str, label: &str, allow_random: bool, salt: usize) -> bool {
    let current = block.get(key).map(str::trim).unwrap_or("").to_owned();
    let shown = if current.is_empty() { t!("soundscape.ed.position_none").to_owned() } else { current.clone() };
    let mut changed = false;
    ui.horizontal(|ui| {
        ui.add_sized([LABEL_W, ui.spacing().interact_size.y], egui::Label::new(label).truncate());
        egui::ComboBox::from_id_salt(("ss_pos", key, salt)).selected_text(shown).width(170.0).show_ui(ui, |ui| {
            if ui.selectable_label(current.is_empty(), t!("soundscape.ed.position_none")).clicked() {
                block.remove(key);
                changed = true;
            }
            for i in 0..script::MAX_POSITIONS {
                let v = i.to_string();
                if ui.selectable_label(current == v, tf!("soundscape.ed.position_n", i)).clicked() {
                    block.set(key, v);
                    changed = true;
                }
            }
            if allow_random
                && ui
                    .selectable_label(current.eq_ignore_ascii_case("random"), t!("soundscape.ed.position_random"))
                    .clicked()
            {
                block.set(key, "random");
                changed = true;
            }
        });
    })
    .response
    .on_hover_text(t!("soundscape.ed.position_tip"));
    changed
}

fn wave_slots(block: &Block) -> Vec<usize> {
    block.props.iter().enumerate().filter(|(_, (k, _))| k.eq_ignore_ascii_case("wave")).map(|(i, _)| i).collect()
}

fn entities_using(state: &AppState, name: &str) -> Vec<(u32, EmitterKind)> {
    let mut out: Vec<(u32, EmitterKind)> = world::collect_emitters(state)
        .into_iter()
        .filter(|e| e.soundscape.eq_ignore_ascii_case(name))
        .map(|e| (e.id, e.kind))
        .collect();
    out.sort_by_key(|(id, _)| *id);
    out
}

pub fn selected_soundscape_entities(state: &AppState) -> Vec<u32> {
    let scene = state.doc.scene();
    state
        .doc
        .selection()
        .objects
        .iter()
        .filter_map(|o| match *o {
            ObjRef::Entity(id) => scene
                .entity(id)
                .filter(|n| {
                    matches!(
                        EmitterKind::from_class(&n.entity.classname),
                        Some(EmitterKind::Main | EmitterKind::Triggerable)
                    )
                })
                .map(|_| id),
            ObjRef::Solid(_) => None,
        })
        .collect()
}

pub fn assign(state: &mut AppState, ids: &[u32], name: &str) -> Result<usize, String> {
    if ids.is_empty() {
        return Err(t!("soundscape.ed.assign_none").to_owned());
    }
    let ids = ids.to_vec();
    let name = name.to_owned();
    state
        .doc
        .transaction("Assign soundscape", |doc| doc.set_entity_key(&ids, "soundscape", &name))
        .map_err(|e| e.to_string())
}

impl SoundscapeEditor {
    pub fn reset(&mut self) {
        *self = SoundscapeEditor { root: self.root.take(), ..SoundscapeEditor::default() };
    }

    fn sync(&mut self, state: &mut AppState) {
        let key = (state.assets.generation(), world::map_rel(state), self.root.clone());
        let serial = state.runtime.lab.soundscape.library_serial;
        let pending_gone =
            self.dirty && key.1.as_ref().is_none_or(|rel| !state.runtime.lab.soundscape.unsaved.contains_key(rel));
        let outside_change = serial != self.seen_serial && (!self.dirty || pending_gone);
        if self.key.as_ref() == Some(&key) && !outside_change {
            return;
        }
        let map_changed = self.key.as_ref().is_none_or(|k| k.1 != key.1 || k.2 != key.2);
        if self.key.as_ref().is_none_or(|k| k.0 != key.0) || self.game_lib.is_empty() && self.game.is_empty() {
            self.game = Arc::new(world::game_texts(state));
            let mut lib = Library::default();
            for (f, text) in self.game.iter() {
                lib.add_file(f, text, Origin::Game, false);
            }
            self.game_lib = Arc::new(lib);
        }
        if map_changed || outside_change {
            let pending = key.1.as_ref().is_some_and(|rel| state.runtime.lab.soundscape.unsaved.contains_key(rel));
            self.map = if pending && self.root.is_none() {
                world::current_map_file(state)
            } else {
                world::read_map_file(state, self.root.as_deref())
            };
            self.dirty = pending;
            if map_changed {
                self.selected = None;
            }
        }
        self.combined = None;
        self.seen_serial = serial;
        self.key = Some(key);
    }

    fn combined(&mut self) -> Arc<Library> {
        if let Some((s, lib)) = &self.combined
            && *s == self.seen_serial
        {
            return Arc::clone(lib);
        }
        let mut lib = (*self.game_lib).clone();
        let rel = self.key.as_ref().and_then(|k| k.1.clone()).unwrap_or_default();
        lib.add_blocks(&rel, self.map.blocks.clone(), Origin::Map, true);
        let lib = Arc::new(lib);
        self.combined = Some((self.seen_serial, Arc::clone(&lib)));
        lib
    }

    fn changed(&mut self, state: &mut AppState) {
        self.dirty = true;
        let rt = &mut state.runtime.lab.soundscape;
        if let Some(rel) = self.key.as_ref().and_then(|k| k.1.clone()) {
            rt.unsaved.insert(rel, script::write_file(&self.map.blocks));
        }
        rt.library_serial += 1;
        self.seen_serial = rt.library_serial;
        self.combined = None;
    }

    pub fn show(&mut self, ctx: &egui::Context, state: &mut AppState) {
        if let Some(Some(dir)) = media::poll(&mut self.pick_dir) {
            self.root = Some(dir);
        }
        if !state.runtime.entities.show_soundscapes {
            return;
        }
        self.sync(state);
        if let Some(name) = state.runtime.lab.soundscape.select.take() {
            self.select_name(&name);
        }
        let mut open = true;
        hp_app::chrome::Window::new(t!("soundscape.ed.title"))
            .id(egui::Id::unique("hp_ui_lab.soundscape_editor"))
            .open(&mut open)
            .default_size([860.0, 600.0])
            .resizable(true)
            .collapsible(true)
            .show(ctx, |ui| self.ui(ui, ctx, state));
        if !open {
            state.runtime.entities.show_soundscapes = false;
            if state.runtime.lab.soundscape.preview.is_some() {
                state.runtime.lab.soundscape.preview_request = Some(PreviewRequest::Stop);
            }
        }
    }

    fn select_name(&mut self, name: &str) {
        if let Some(i) = self.map.blocks.iter().position(|b| b.name.trim().eq_ignore_ascii_case(name.trim())) {
            self.selected = Some(Sel::Map(i));
        } else if self.game_lib.get(name).is_some() {
            self.selected = Some(Sel::Game(name.trim().to_owned()));
        }
    }

    fn ui(&mut self, ui: &mut Ui, ctx: &egui::Context, state: &mut AppState) {
        self.header_ui(ui, ctx, state);
        ui.separator();
        egui::Panel::left(egui::Id::unique("hp_ui_lab.soundscape_list"))
            .resizable(true)
            .default_size(250.0)
            .show(ui, |ui| self.list_ui(ui, state));
        egui::CentralPanel::default().show(ui, |ui| self.details_ui(ui, state));
    }

    fn header_ui(&mut self, ui: &mut Ui, ctx: &egui::Context, state: &mut AppState) {
        let path = self.map.path.clone();
        match &path {
            Some(p) => {
                ui.horizontal_wrapped(|ui| {
                    ui.label(RichText::new(tf!("soundscape.ed.file", p.display())).small());
                    if self.dirty {
                        ui.label(RichText::new(t!("soundscape.ed.unsaved")).small().color(theme::warning()));
                    }
                });
                if self.map.legacy
                    && let Some(from) = &self.map.loaded_from
                {
                    ui.label(
                        RichText::new(tf!("soundscape.ed.legacy", from.display())).small().color(theme::warning()),
                    );
                }
            }
            None => {
                ui.label(RichText::new(t!("soundscape.ed.save_map_first")).color(theme::warning()));
            }
        }
        if let Some(e) = &self.map.error {
            ui.colored_label(theme::error(), tf!("soundscape.ed.read_failed", e));
        }
        ui.horizontal_wrapped(|ui| {
            let can_save = path.is_some() && self.map.error.is_none() && (self.dirty || self.map.legacy);
            if ui.add_enabled(can_save, egui::Button::new(format!("💾 {}", t!("soundscape.ed.save")))).clicked() {
                self.save(state);
            }
            if ui.button(t!("soundscape.ed.reload")).clicked() {
                if let Some(rel) = world::map_rel(state) {
                    state.runtime.lab.soundscape.unsaved.remove(&rel);
                }
                self.map = world::read_map_file(state, self.root.as_deref());
                self.dirty = false;
                self.selected = None;
                state.runtime.lab.soundscape.library_serial += 1;
                self.seen_serial = state.runtime.lab.soundscape.library_serial;
                self.combined = None;
            }
            if ui.button(t!("soundscape.ed.folder")).on_hover_text(t!("soundscape.ed.folder_tip")).clicked()
                && self.pick_dir.is_none()
            {
                let start = self.root.clone().or_else(|| media::default_root(state));
                self.pick_dir = Some(media::pick_folder(start, ctx));
            }
            if self.root.is_some() && ui.button(t!("soundscape.ed.folder_default")).clicked() {
                self.root = None;
            }
        });
        ui.horizontal_wrapped(|ui| {
            let s = &mut state.settings.lab.soundscape;
            let mut dirty = false;
            dirty |= ui
                .checkbox(&mut s.view, t!("soundscape.ed.show_view"))
                .on_hover_text(t!("soundscape.ed.show_view_tip"))
                .changed();
            ui.add_enabled_ui(s.view, |ui| {
                dirty |= ui
                    .checkbox(&mut s.areas, t!("soundscape.ed.show_areas"))
                    .on_hover_text(t!("soundscape.ed.show_areas_tip"))
                    .changed();
            });
            dirty |= ui
                .checkbox(&mut s.hearing, t!("soundscape.ed.hearing"))
                .on_hover_text(t!("soundscape.ed.hearing_tip"))
                .changed();
            dirty |= ui
                .checkbox(&mut s.ambient_generic, t!("soundscape.ed.ambient"))
                .on_hover_text(t!("soundscape.ed.ambient_tip"))
                .changed();
            ui.label(t!("soundscape.ed.volume"));
            dirty |= ui.add(egui::Slider::new(&mut s.volume, 0.0..=1.0).show_value(true).fixed_decimals(2)).changed();
            dirty |= ui.checkbox(&mut s.mute, t!("soundscape.ed.mute")).changed();
            if dirty {
                state.settings_dirty = true;
            }
        });
        if let Some(a) = &state.runtime.lab.soundscape.active {
            let text = if a.by_trigger {
                tf!("soundscape.ed.active_trigger", a.name, a.class, a.entity)
            } else if a.kept {
                tf!("soundscape.ed.active_kept", a.name, a.class, a.entity)
            } else {
                tf!("soundscape.ed.active", a.name, a.class, a.entity)
            };
            ui.label(RichText::new(text).small().weak());
        }
        if let Some(note) = &state.runtime.lab.soundscape.hearing_note {
            ui.label(RichText::new(note).small().color(theme::warning()));
        }
        if let Some((sev, text)) = &self.status {
            ui.colored_label(sev.color(), text);
        }
    }

    fn save(&mut self, state: &mut AppState) {
        let Some(path) = self.map.path.clone() else { return };
        let result =
            world::check_target(state, &self.game, &path).and_then(|()| world::write_map_file(&path, &self.map.blocks));
        match result {
            Ok(()) => {
                self.dirty = false;
                self.map.legacy = false;
                self.map.loaded_from = Some(path.clone());
                if let Some(rel) = world::map_rel(state) {
                    state.runtime.lab.soundscape.unsaved.remove(&rel);
                }
                hp_ui_entities::model_import::remount_after_write(state);
                state.runtime.lab.soundscape.library_serial += 1;
                self.seen_serial = state.runtime.lab.soundscape.library_serial;
                self.combined = None;
                let text = tf!("soundscape.ed.saved", path.display());
                state.notify(Severity::Success, text.clone());
                self.status = Some((Severity::Success, text));
            }
            Err(e) => {
                let text = tf!("soundscape.ed.save_failed", e);
                state.notify(Severity::Error, text.clone());
                self.status = Some((Severity::Error, text));
            }
        }
    }

    fn list_ui(&mut self, ui: &mut Ui, state: &mut AppState) {
        ui.horizontal(|ui| {
            ui.add(
                egui::TextEdit::singleline(&mut self.search)
                    .hint_text(t!("soundscape.ed.search"))
                    .desired_width(f32::INFINITY),
            );
        });
        ui.horizontal(|ui| {
            for (f, label) in [
                (Filter::All, t!("soundscape.ed.filter_all")),
                (Filter::Map, t!("soundscape.ed.filter_map")),
                (Filter::Game, t!("soundscape.ed.filter_game")),
            ] {
                ui.selectable_value(&mut self.filter, f, label);
            }
        });
        let can_add = self.map.path.is_some();
        ui.horizontal(|ui| {
            let w = (ui.available_width() - 34.0).max(60.0);
            ui.add_enabled(
                can_add,
                egui::TextEdit::singleline(&mut self.new_name).hint_text(t!("soundscape.ed.new_name")).desired_width(w),
            );
            let ok = can_add
                && script::valid_name(&self.new_name)
                && !self.map.blocks.iter().any(|b| b.name.trim().eq_ignore_ascii_case(self.new_name.trim()))
                && self.map.blocks.len() < MAX_ITEMS;
            if ui.add_enabled(ok, egui::Button::new("+")).on_hover_text(t!("soundscape.ed.add")).clicked() {
                let mut b = Block::new(self.new_name.trim());
                b.set("dsp", "1");
                self.map.blocks.push(b);
                self.selected = Some(Sel::Map(self.map.blocks.len() - 1));
                self.new_name.clear();
                self.changed(state);
            }
        });
        let words: Vec<String> = self.search.split_whitespace().map(str::to_ascii_lowercase).collect();
        let hit = |n: &str| {
            let l = n.to_ascii_lowercase();
            words.iter().all(|w| l.contains(w))
        };
        ScrollArea::vertical().id_salt("ss_names").auto_shrink([false, false]).show(ui, |ui| {
            let mut clicked = None;
            let mut shown = 0;
            if self.filter != Filter::Game {
                for (i, b) in self.map.blocks.iter().enumerate() {
                    if !hit(&b.name) {
                        continue;
                    }
                    let collides = self.game_lib.get(&b.name).is_some();
                    let mut text = RichText::new(&b.name);
                    if collides {
                        text = text.color(theme::warning());
                    }
                    let resp = ui.selectable_label(self.selected == Some(Sel::Map(i)), text);
                    let resp = if collides {
                        resp.on_hover_text(t!("soundscape.ed.collision"))
                    } else {
                        resp.on_hover_text(t!("soundscape.ed.map_item"))
                    };
                    if resp.clicked() {
                        clicked = Some(Sel::Map(i));
                    }
                    shown += 1;
                }
                if self.map.blocks.is_empty() {
                    ui.label(RichText::new(t!("soundscape.ed.map_empty")).small().weak());
                }
            }
            if self.filter != Filter::Map {
                ui.separator();
                ui.label(RichText::new(t!("soundscape.ed.game_title")).small().strong());
                for e in self.game_lib.entries() {
                    if shown >= MAX_LIST {
                        break;
                    }
                    if !hit(&e.block.name) {
                        continue;
                    }
                    let name = e.block.name.trim().to_owned();
                    let sel = self.selected.as_ref() == Some(&Sel::Game(name.clone()));
                    if ui.selectable_label(sel, RichText::new(&name).weak()).on_hover_text(e.file.clone()).clicked() {
                        clicked = Some(Sel::Game(name));
                    }
                    shown += 1;
                }
                if self.game_lib.is_empty() {
                    ui.label(RichText::new(t!("soundscape.ed.game_empty")).small().weak());
                }
            }
            if let Some(c) = clicked {
                self.selected = Some(c);
                self.picker = None;
            }
        });
    }

    fn details_ui(&mut self, ui: &mut Ui, state: &mut AppState) {
        let (mut block, editable, file) = match self.selected.clone() {
            Some(Sel::Map(i)) if i < self.map.blocks.len() => {
                let file = self.map.path.as_ref().map_or_else(String::new, |p| p.display().to_string());
                (self.map.blocks[i].clone(), true, file)
            }
            Some(Sel::Game(name)) => match self.game_lib.get(&name) {
                Some(e) => (e.block.clone(), false, e.file.clone()),
                None => {
                    self.selected = None;
                    return;
                }
            },
            _ => {
                ui.label(RichText::new(t!("soundscape.ed.select")).weak());
                return;
            }
        };
        let name = block.name.trim().to_owned();
        ui.horizontal_wrapped(|ui| {
            ui.label(RichText::new(&name).strong().size(15.0));
            ui.label(
                RichText::new(if editable { t!("soundscape.ed.from_map") } else { t!("soundscape.ed.from_game") })
                    .small()
                    .weak(),
            );
            ui.label(RichText::new(file).small().weak());
        });
        let mut delete = false;
        let mut copy = false;
        ui.horizontal_wrapped(|ui| {
            let previewing =
                state.runtime.lab.soundscape.preview.as_deref().is_some_and(|p| p.eq_ignore_ascii_case(&name));
            if previewing {
                if ui.button(format!("■ {}", t!("soundscape.ed.stop"))).clicked() {
                    state.runtime.lab.soundscape.preview_request = Some(PreviewRequest::Stop);
                }
            } else if ui
                .button(format!("▶ {}", t!("soundscape.ed.preview")))
                .on_hover_text(t!("soundscape.ed.preview_tip"))
                .clicked()
            {
                state.runtime.lab.soundscape.preview_request =
                    Some(PreviewRequest::Play { name: name.clone(), volume: 1.0 });
            }
            let targets = selected_soundscape_entities(state);
            let assign_resp =
                ui.add_enabled(!targets.is_empty(), egui::Button::new(tf!("soundscape.ed.assign", targets.len())));
            if assign_resp
                .on_hover_text(t!("soundscape.ed.assign_tip"))
                .on_disabled_hover_text(t!("soundscape.ed.assign_none"))
                .clicked()
            {
                match assign(state, &targets, &name) {
                    Ok(n) => self.status = Some((Severity::Success, tf!("soundscape.ed.assigned", name, n))),
                    Err(e) => self.status = Some((Severity::Error, e)),
                }
            }
            if editable {
                if ui.button(t!("soundscape.ed.delete")).clicked() {
                    delete = true;
                }
            } else if ui
                .add_enabled(self.map.path.is_some(), egui::Button::new(t!("soundscape.ed.copy")))
                .on_hover_text(t!("soundscape.ed.copy_tip"))
                .clicked()
            {
                copy = true;
            }
        });
        let users = entities_using(state, &name);
        ui.horizontal_wrapped(|ui| {
            if users.is_empty() {
                ui.label(RichText::new(t!("soundscape.ed.unused")).small().weak());
            } else {
                ui.label(RichText::new(t!("soundscape.ed.used_by")).small());
                for (id, kind) in &users {
                    if ui
                        .small_button(format!("{} {id}", kind.class()))
                        .on_hover_text(t!("soundscape.ed.select_entity"))
                        .clicked()
                    {
                        state.doc.set_selection(&[ObjRef::Entity(*id)]);
                    }
                }
            }
        });
        if editable && self.game_lib.get(&name).is_some() {
            ui.label(RichText::new(t!("soundscape.ed.collision")).small().color(theme::warning()));
        }
        self.summary_ui(ui, &name);
        ui.separator();
        let mut changed = false;
        let mut remove_rule = None;
        let lib = self.combined();
        ScrollArea::vertical().id_salt("ss_rules").auto_shrink([false, false]).show(ui, |ui| {
            ui.add_enabled_ui(editable, |ui| {
                changed |= self.props_ui(ui, &mut block);
                ui.horizontal_wrapped(|ui| {
                    for rule in RULES {
                        if ui.small_button(format!("+ {rule}")).clicked() && block.children.len() < MAX_ITEMS {
                            block.children.push(new_rule(rule));
                            changed = true;
                        }
                    }
                });
                for r in 0..block.children.len() {
                    ui.push_id(("rule", r), |ui| {
                        egui::Frame::group(ui.style()).show(ui, |ui| {
                            ui.horizontal(|ui| {
                                ui.label(RichText::new(&block.children[r].name).strong());
                                ui.with_layout(egui::Layout::right_to_left(egui::Align::Center), |ui| {
                                    if ui.small_button("×").on_hover_text(t!("soundscape.ed.remove_rule")).clicked() {
                                        remove_rule = Some(r);
                                    }
                                });
                            });
                            changed |= self.rule_ui(ui, state, &mut block.children[r], r, &lib);
                        });
                    });
                }
            });
        });
        if let Some(r) = remove_rule {
            block.children.remove(r);
            changed = true;
        }
        if let Some(Sel::Map(i)) = self.selected.clone() {
            if delete {
                self.map.blocks.remove(i);
                self.selected = None;
                self.changed(state);
            } else if changed && editable {
                self.map.blocks[i] = block;
                self.changed(state);
            }
        } else if copy {
            let base = match world::map_name(state) {
                Some(m) if !name.to_ascii_lowercase().starts_with(&format!("{m}.")) => format!("{m}.{name}"),
                _ => name.clone(),
            };
            let mut new_name = base.clone();
            let mut n = 2;
            while self.map.blocks.iter().any(|b| b.name.trim().eq_ignore_ascii_case(&new_name))
                || self.game_lib.get(&new_name).is_some()
            {
                new_name = format!("{base}_{n}");
                n += 1;
            }
            block.name = new_name;
            self.map.blocks.push(block);
            self.selected = Some(Sel::Map(self.map.blocks.len() - 1));
            self.changed(state);
        }
    }

    fn summary_ui(&mut self, ui: &mut Ui, name: &str) {
        let lib = self.combined();
        let flat = lib.flatten(name);
        let looping = flat.sounds.iter().filter(|s| s.kind == hp_soundscape::FlatKind::Looping).count();
        let random = flat.sounds.len() - looping;
        let dsp = flat.dsp.map_or_else(|| "-".to_owned(), |d| d.to_string());
        ui.label(RichText::new(tf!("soundscape.ed.summary", looping, random, dsp)).small());
        if !flat.missing.is_empty() {
            ui.label(
                RichText::new(tf!("soundscape.ed.missing", flat.missing.join(", "))).small().color(theme::warning()),
            );
        }
        if !flat.cycles.is_empty() {
            ui.label(RichText::new(tf!("soundscape.ed.cycle", flat.cycles.join(", "))).small().color(theme::warning()));
        }
    }

    fn props_ui(&mut self, ui: &mut Ui, block: &mut Block) -> bool {
        let mut changed = false;
        for (key, label, tip) in [
            ("dsp", t!("soundscape.ed.dsp"), t!("soundscape.ed.dsp_tip")),
            ("dsp_player", t!("soundscape.ed.dsp_player"), t!("soundscape.ed.dsp_player_tip")),
        ] {
            ui.horizontal(|ui| {
                ui.add_sized([LABEL_W, ui.spacing().interact_size.y], egui::Label::new(label).truncate())
                    .on_hover_text(tip);
                let mut dsp: i32 = block.get(key).and_then(|v| v.trim().parse::<f32>().ok()).map_or(0, |v| v as i32);
                let present = block.get(key).is_some();
                if ui.add(egui::DragValue::new(&mut dsp).range(0..=133)).changed() {
                    block.set(key, dsp.to_string());
                    changed = true;
                }
                if present && ui.small_button("×").on_hover_text(t!("soundscape.ed.clear")).clicked() {
                    block.remove(key);
                    changed = true;
                }
                if !present {
                    ui.label(RichText::new(t!("soundscape.ed.dsp_unchanged")).small().weak());
                } else if let Some(k) = dsp_key(dsp) {
                    ui.label(RichText::new(t!(k)).small().weak());
                }
            });
        }
        ui.horizontal(|ui| {
            ui.add_sized(
                [LABEL_W, ui.spacing().interact_size.y],
                egui::Label::new(t!("soundscape.ed.dsp_volume")).truncate(),
            )
            .on_hover_text(t!("soundscape.ed.dsp_volume_tip"));
            let present = block.get("dsp_volume").is_some();
            let mut v: f32 = block.get("dsp_volume").and_then(|v| v.trim().parse().ok()).unwrap_or(1.0);
            if ui.add(egui::DragValue::new(&mut v).range(0.0..=2.0).speed(0.01).fixed_decimals(2)).changed() {
                block.set("dsp_volume", script::num(v));
                changed = true;
            }
            if present && ui.small_button("×").on_hover_text(t!("soundscape.ed.clear")).clicked() {
                block.remove("dsp_volume");
                changed = true;
            }
        });
        changed
    }

    fn rule_ui(&mut self, ui: &mut Ui, state: &AppState, rule: &mut Block, r: usize, lib: &Library) -> bool {
        let mut changed = false;
        match rule.name.trim().to_ascii_lowercase().as_str() {
            RULE_LOOPING => {
                changed |= interval_field(
                    ui,
                    rule,
                    "volume",
                    t!("soundscape.ed.volume_rule"),
                    Interval::fixed(0.0),
                    (0.0, 1.0),
                    0.01,
                );
                if rule.get("volume").and_then(Interval::parse).is_none_or(|v| v.max <= 0.0) {
                    ui.label(RichText::new(t!("soundscape.ed.no_volume")).small().color(theme::warning()));
                }
                changed |= interval_field(
                    ui,
                    rule,
                    "pitch",
                    t!("soundscape.ed.pitch"),
                    Interval::fixed(100.0),
                    (1.0, 255.0),
                    1.0,
                );
                changed |= self.wave_rows(ui, state, rule, r, true);
                changed |= soundlevel_field(ui, rule, r);
                changed |= position_field(ui, rule, "position", t!("soundscape.ed.position"), false, r);
            }
            RULE_RANDOM => {
                changed |=
                    interval_field(ui, rule, "time", t!("soundscape.ed.time"), Interval::fixed(0.0), (0.0, 600.0), 0.1);
                changed |= interval_field(
                    ui,
                    rule,
                    "volume",
                    t!("soundscape.ed.volume_rule"),
                    Interval::fixed(0.0),
                    (0.0, 1.0),
                    0.01,
                );
                changed |= interval_field(
                    ui,
                    rule,
                    "pitch",
                    t!("soundscape.ed.pitch"),
                    Interval::fixed(0.0),
                    (0.0, 255.0),
                    1.0,
                );
                for (key, min, warn) in [
                    ("volume", 0.0, "soundscape.ed.no_volume"),
                    ("pitch", 0.99, "soundscape.ed.no_pitch"),
                    ("time", 0.0, "soundscape.ed.no_time"),
                ] {
                    if rule.get(key).and_then(Interval::parse).is_none_or(|v| v.max <= min) {
                        ui.label(RichText::new(t!(warn)).small().color(theme::warning()));
                    }
                }
                if rule.child("rndwave").is_none() {
                    rule.push_child(Block::new("rndwave"));
                }
                if let Some(waves) = rule.child_mut("rndwave") {
                    changed |= self.wave_rows(ui, state, waves, r, false);
                }
                changed |= soundlevel_field(ui, rule, r);
                changed |= position_field(ui, rule, "position", t!("soundscape.ed.position"), true, r);
            }
            RULE_NESTED => {
                let current = rule.get("name").unwrap_or("").trim().to_owned();
                ui.horizontal(|ui| {
                    ui.add_sized(
                        [LABEL_W, ui.spacing().interact_size.y],
                        egui::Label::new(t!("soundscape.ed.nested")).truncate(),
                    );
                    let mut text = current.clone();
                    if ui.add(egui::TextEdit::singleline(&mut text).desired_width(200.0)).changed()
                        && !text.contains('"')
                    {
                        rule.set("name", text.trim());
                        changed = true;
                    }
                    egui::ComboBox::from_id_salt(("ss_nested", r)).selected_text("⏷").width(30.0).show_ui(ui, |ui| {
                        ScrollArea::vertical().max_height(300.0).show(ui, |ui| {
                            for e in lib.entries().take(MAX_LIST) {
                                let n = e.block.name.trim();
                                if ui.selectable_label(n.eq_ignore_ascii_case(&current), n).clicked() {
                                    rule.set("name", n);
                                    changed = true;
                                }
                            }
                        });
                    });
                    if !current.is_empty() && ui.small_button(t!("soundscape.ed.open")).clicked() {
                        self.select_name(&current);
                    }
                });
                if !current.is_empty() && lib.get(&current).is_none() {
                    ui.label(RichText::new(tf!("soundscape.ed.missing", current)).small().color(theme::warning()));
                }
                changed |= interval_field(
                    ui,
                    rule,
                    "volume",
                    t!("soundscape.ed.volume_rule"),
                    Interval::fixed(1.0),
                    (0.0, 1.0),
                    0.01,
                );
                changed |= position_field(ui, rule, "position", t!("soundscape.ed.nested_position"), false, r);
                changed |=
                    position_field(ui, rule, "positionoverride", t!("soundscape.ed.position_override"), false, r);
                changed |=
                    position_field(ui, rule, "ambientpositionoverride", t!("soundscape.ed.ambient_override"), false, r);
            }
            _ => {
                ui.label(RichText::new(t!("soundscape.ed.unknown_rule")).small().weak());
            }
        }
        changed
    }

    fn wave_rows(&mut self, ui: &mut Ui, state: &AppState, block: &mut Block, r: usize, single: bool) -> bool {
        let mut changed = false;
        let mut slots = wave_slots(block);
        if single && slots.is_empty() {
            block.push_prop("wave", "");
            slots = wave_slots(block);
        }
        let mut remove = None;
        for (n, slot) in slots.iter().copied().enumerate() {
            ui.push_id(("wave", n), |ui| {
                ui.horizontal(|ui| {
                    ui.add_sized([LABEL_W, ui.spacing().interact_size.y], egui::Label::new(t!("soundscape.ed.wave")));
                    let w = (ui.available_width() - 110.0).max(80.0);
                    let value = &mut block.props[slot].1;
                    if ui.add(egui::TextEdit::singleline(value).desired_width(w)).changed() {
                        *value = value.replace('"', "");
                        changed = true;
                    }
                    let btn = ui.small_button("♪").on_hover_text(t!("soundscape.ed.pick_wave"));
                    let mut open = self.picker == Some((r, n));
                    if btn.clicked() {
                        open = !open;
                        self.wave_search.clear();
                    }
                    if open {
                        let mut picked = None;
                        egui::Popup::from_response(&btn)
                            .id(ui.make_persistent_id(("ss_wave_pick", r, n)))
                            .open_bool(&mut open)
                            .close_behavior(egui::PopupCloseBehavior::CloseOnClickOutside)
                            .width(380.0)
                            .show(|ui| {
                                picked = self.picker_ui(ui, state);
                                if picked.is_some() {
                                    ui.close();
                                }
                            });
                        if let Some(p) = picked {
                            block.props[slot].1 = p;
                            changed = true;
                            open = false;
                        }
                    }
                    self.picker = if open {
                        Some((r, n))
                    } else if self.picker == Some((r, n)) {
                        None
                    } else {
                        self.picker
                    };
                    let wave = block.props[slot].1.clone();
                    if ui.small_button("▶").on_hover_text(t!("soundscape.ed.play_wave")).clicked() {
                        play_wave(state, &mut self.sounds, &wave);
                    }
                    if (!single || slots.len() > 1)
                        && ui.small_button("×").on_hover_text(t!("soundscape.ed.remove_wave")).clicked()
                    {
                        remove = Some(slot);
                    }
                });
                let wave = block.props[slot].1.clone();
                if let Some(path) = script::wave_path(&wave) {
                    if self.sounds.is_missing(&path) {
                        ui.label(
                            RichText::new(tf!("soundscape.ed.wave_missing", path)).small().color(theme::warning()),
                        );
                    } else if let Some(s) = self.sounds.loaded(&path) {
                        if single && !s.looped {
                            ui.label(RichText::new(t!("soundscape.ed.no_loop")).small().color(theme::warning()));
                        }
                    } else {
                        let _ = self.sounds.get(state, &path);
                        ui.ctx().request_repaint_after(std::time::Duration::from_millis(100));
                    }
                }
            });
        }
        if let Some(slot) = remove {
            block.props.remove(slot);
            changed = true;
        }
        if !single && ui.small_button(t!("soundscape.ed.add_wave")).clicked() && slots.len() < MAX_ITEMS {
            block.push_prop("wave", "");
            changed = true;
        }
        changed
    }

    fn picker_ui(&mut self, ui: &mut Ui, state: &AppState) -> Option<String> {
        let r = ui.add(
            egui::TextEdit::singleline(&mut self.wave_search)
                .hint_text(t!("soundscape.ed.search"))
                .desired_width(f32::INFINITY),
        );
        r.request_focus();
        let words: Vec<String> = self.wave_search.split_whitespace().map(str::to_ascii_lowercase).collect();
        let sounds = state.assets.sounds();
        let mut picked = None;
        ScrollArea::vertical().max_height(320.0).show(ui, |ui| {
            let mut shown = 0;
            for e in sounds.iter() {
                if shown >= 400 {
                    ui.label(RichText::new(t!("soundscape.ed.more")).small().weak());
                    break;
                }
                let l = e.name.to_ascii_lowercase();
                if !media::is_sound_ext(&l) || !words.iter().all(|w| l.contains(w)) {
                    continue;
                }
                let rel = media::game_sound_path(&e.name);
                let wave = rel.strip_prefix("sound/").unwrap_or(&rel).to_owned();
                if ui.selectable_label(false, &wave).clicked() {
                    picked = Some(wave);
                }
                shown += 1;
            }
            if shown == 0 {
                ui.label(RichText::new(t!("soundscape.ed.no_sounds")).small().weak());
            }
        });
        picked
    }
}

fn play_wave(state: &AppState, cache: &mut SoundCache, wave: &str) {
    let Some(path) = script::wave_path(wave) else { return };
    let sound = match cache.loaded(&path) {
        Some(s) => Some(s),
        None => media::read_game(state, &path).and_then(|b| hp_audio::decode(&b, &path).ok()).map(Arc::new),
    };
    if let Some(s) = sound {
        let volume = state.settings.lab.soundscape.output_volume();
        hp_audio::play(&s, 0.0, None, false, volume);
    }
}

pub fn new_rule(kind: &str) -> Block {
    let mut b = Block::new(kind);
    match kind {
        RULE_LOOPING => {
            b.set("volume", "1");
            b.set("pitch", "100");
            b.push_prop("wave", "");
        }
        RULE_RANDOM => {
            b.set("time", "10,20");
            b.set("volume", "0.5,0.8");
            b.set("pitch", "95,105");
            let mut waves = Block::new("rndwave");
            waves.push_prop("wave", "");
            b.push_child(waves);
        }
        _ => {
            b.set("volume", "1");
            b.set("name", "");
        }
    }
    b
}

pub fn dsp_key(dsp: i32) -> Option<&'static str> {
    Some(match dsp {
        0 => "soundscape.dsp.off",
        1 => "soundscape.dsp.generic",
        2..=4 => "soundscape.dsp.metal",
        5..=7 => "soundscape.dsp.tunnel",
        8..=10 => "soundscape.dsp.chamber",
        11..=13 => "soundscape.dsp.bright",
        14..=16 => "soundscape.dsp.water",
        17..=19 => "soundscape.dsp.concrete",
        20..=22 => "soundscape.dsp.outside",
        23..=25 => "soundscape.dsp.cavern",
        26..=28 => "soundscape.dsp.weird",
        _ => return None,
    })
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn new_rules_are_valid_and_audible() {
        for kind in RULES {
            let b = new_rule(kind);
            let rule = script::Rule::from_block(&b);
            assert_eq!(rule.kind(), kind);
        }
        let looping = script::Rule::from_block(&new_rule(RULE_LOOPING));
        let script::Rule::Looping(l) = looping else { panic!() };
        assert_eq!(l.volume, Interval::fixed(1.0), "a new looping rule is audible, unlike the engine default");
        assert_eq!(dsp_key(1), Some("soundscape.dsp.generic"));
        assert_eq!(dsp_key(500), None);
    }

    #[test]
    fn the_editor_draws_edits_and_never_writes_without_save() {
        crate::register_translations();
        let dir = std::env::temp_dir().join(format!("hp-ui-lab-ss-{}", std::process::id()));
        let _ = std::fs::remove_dir_all(&dir);
        std::fs::create_dir_all(dir.join("maps")).unwrap();
        let mut state = hp_app::testing::test_state();
        state.runtime.entities.show_soundscapes = true;
        let mut ed = SoundscapeEditor::default();
        hp_app::testing::run_ctx(&mut state, |ctx, s| ed.show(ctx, s));
        state.doc.set_path(Some(dir.join("maps").join("gm_ed.vmf")));
        hp_app::testing::run_ctx(&mut state, |ctx, s| ed.show(ctx, s));
        let target = dir.join("scripts").join("soundscapes_gm_ed.txt");
        assert_eq!(ed.map.path.as_deref(), Some(target.as_path()));
        ed.map.blocks.push(Block::new("gm_ed.a"));
        let mut a = new_rule(RULE_RANDOM);
        a.child_mut("rndwave").unwrap().props[0].1 = "ambient/x.wav".to_owned();
        ed.map.blocks[0].children.push(a);
        ed.map.blocks[0].children.push(new_rule(RULE_LOOPING));
        ed.map.blocks[0].children.push(new_rule(RULE_NESTED));
        ed.selected = Some(Sel::Map(0));
        ed.changed(&mut state);
        assert!(
            state
                .runtime
                .lab
                .soundscape
                .unsaved
                .get("scripts/soundscapes_gm_ed.txt")
                .is_some_and(|t| t.contains("gm_ed.a"))
        );
        for _ in 0..2 {
            hp_app::testing::run_ctx(&mut state, |ctx, s| ed.show(ctx, s));
        }
        assert!(!target.exists(), "nothing is written without Save");
        ed.save(&mut state);
        assert!(target.is_file());
        assert!(state.runtime.lab.soundscape.unsaved.is_empty());
        let back = world::read_map_file(&state, None);
        assert_eq!(back.blocks.len(), 1);
        assert_eq!(back.blocks[0].children.len(), 3);
        let _ = std::fs::remove_dir_all(&dir);
    }

    #[test]
    fn a_legacy_file_next_to_the_map_is_read_and_saved_into_scripts() {
        let dir = std::env::temp_dir().join(format!("hp-ui-lab-ss-legacy-{}", std::process::id()));
        let _ = std::fs::remove_dir_all(&dir);
        std::fs::create_dir_all(dir.join("maps")).unwrap();
        std::fs::write(dir.join("maps").join("soundscapes_gm_old.txt"), "\"gm_old.x\" { \"dsp\" \"2\" }").unwrap();
        let mut state = hp_app::testing::test_state();
        state.doc.set_path(Some(dir.join("maps").join("gm_old.vmf")));
        let m = world::read_map_file(&state, None);
        assert!(m.legacy);
        assert_eq!(m.blocks.len(), 1);
        let mut ed = SoundscapeEditor::default();
        ed.sync(&mut state);
        assert!(ed.map.legacy);
        ed.save(&mut state);
        assert!(dir.join("scripts").join("soundscapes_gm_old.txt").is_file());
        assert!(dir.join("maps").join("soundscapes_gm_old.txt").is_file(), "the old file is left alone");
        let _ = std::fs::remove_dir_all(&dir);
    }

    #[test]
    fn assigning_sets_the_key_in_one_undo_step() {
        crate::register_translations();
        let mut state = hp_app::testing::test_state();
        let id = state.doc.create_point_entity("env_soundscape", glam::DVec3::ZERO, &[]).unwrap();
        state.doc.set_selection(&[ObjRef::Entity(id)]);
        let ids = selected_soundscape_entities(&state);
        assert_eq!(ids, [id]);
        assert_eq!(assign(&mut state, &ids, "gm_x.cave"), Ok(1));
        assert_eq!(state.doc.scene().entity(id).and_then(|n| n.entity.get("soundscape")), Some("gm_x.cave"));
        assert!(state.doc.undo());
        assert_eq!(state.doc.scene().entity(id).and_then(|n| n.entity.get("soundscape")), None);
        assert!(assign(&mut state, &[], "x").is_err());
    }
}
