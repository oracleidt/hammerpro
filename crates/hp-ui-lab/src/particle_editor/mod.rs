mod catalog;
mod preview;
mod properties;
mod session;
mod tr;

use std::collections::HashMap;
use std::path::PathBuf;

use crossbeam_channel::Receiver;
use egui::{RichText, ScrollArea, Ui, vec2};
use hp_app::{AppState, t, tf};
use hp_pcf::dmx::{Element, Value};
use hp_pcf::editor::{Category, Document, Preset};

pub use preview::EDITOR_VIEW_ID;
pub use session::{EditorHandle, EditorSession, PreviewOptions, new_editor_handle};
pub const TR: hp_app::i18n::TrTable = tr::TR;

#[derive(Clone, Copy)]
struct OperatorDrag {
    system: usize,
    category: Category,
    index: usize,
}

#[derive(Clone, Copy)]
enum Pick {
    Open,
    Import,
    Save,
    Export,
}

#[derive(Clone)]
enum Pending {
    New,
    Open(String),
    Close,
}

pub struct ParticleEditorWindow {
    handle: EditorHandle,
    preview: preview::Preview,
    path: String,
    search: String,
    name: String,
    category: Category,
    function: String,
    parameter: String,
    parameter_type: usize,
    child: Option<usize>,
    child_delay: f32,
    material_search: String,
    material_rows: Vec<usize>,
    material_key: Option<(u64, String, usize)>,
    material_target: Option<(usize, String)>,
    status: Option<String>,
    pick: Option<(Pick, Receiver<Option<PathBuf>>)>,
    pending: Option<Pending>,
    templates: HashMap<Category, Vec<Element>>,
    drafts: HashMap<(usize, String), Value>,
    draft_revision: u64,
    validation: Vec<hp_pcf::editor::ValidationIssue>,
    validation_revision: Option<u64>,
    export_lua: Option<String>,
}

impl ParticleEditorWindow {
    pub fn new(particles: crate::particles::ParticleHandle, handle: EditorHandle) -> Self {
        Self {
            handle,
            preview: preview::Preview::new(particles),
            path: String::new(),
            search: String::new(),
            name: String::new(),
            category: Category::Emitters,
            function: "emit_continuously".to_owned(),
            parameter: String::new(),
            parameter_type: 0,
            child: None,
            child_delay: 0.0,
            material_search: String::new(),
            material_rows: Vec::new(),
            material_key: None,
            material_target: None,
            status: None,
            pick: None,
            pending: None,
            templates: HashMap::new(),
            drafts: HashMap::new(),
            draft_revision: 0,
            validation: Vec::new(),
            validation_revision: None,
            export_lua: None,
        }
    }

    pub fn flush_pending_edits(&mut self) {
        let handle = self.handle.clone();
        let Ok(mut session) = handle.lock() else { return };
        self.flush_drafts(&mut session);
    }

    pub fn show(&mut self, ctx: &egui::Context, state: &mut AppState) {
        let handle = self.handle.clone();
        let Ok(mut session) = handle.lock() else { return };
        if !session.open {
            return;
        }
        self.poll_pick(state, &mut session);
        if self.draft_revision != session.revision {
            self.drafts.clear();
            self.draft_revision = session.revision;
            self.remember_templates(&session.doc);
        }
        if ctx.input(|i| i.pointer.any_released()) {
            self.flush_drafts(&mut session);
        }
        let mut open = true;
        let title = format!("{}{}", t!("pe.title"), if session.dirty() { " *" } else { "" });
        hp_app::chrome::Window::new(title)
            .id(egui::Id::unique("particle_editor"))
            .open(&mut open)
            .default_size([1240.0, 780.0])
            .min_size([880.0, 560.0])
            .max_height((ctx.content_rect().height() - 32.0).clamp(560.0, 900.0))
            .resizable(true)
            .show(ctx, |ui| {
                self.toolbar(ui, state, &mut session);
                ui.separator();
                let total = ui.available_width();
                let height = (ui.available_height() - 70.0).clamp(320.0, 620.0);
                let tree_width = (total * 0.19).clamp(150.0, 250.0);
                let preview_width = (total * 0.34).clamp(260.0, 460.0);
                let gaps = ui.spacing().item_spacing.x * 4.0 + 2.0;
                let properties_width = (total - tree_width - preview_width - gaps).max(240.0);
                ui.allocate_ui_with_layout(vec2(total, height), egui::Layout::left_to_right(egui::Align::Min), |ui| {
                    ui.allocate_ui_with_layout(
                        vec2(tree_width, height),
                        egui::Layout::top_down(egui::Align::Min),
                        |ui| {
                            ui.set_max_width(tree_width);
                            self.tree(ui, &mut session, height);
                        },
                    );
                    ui.add(egui::Separator::default().spacing(1.0));
                    ui.allocate_ui_with_layout(
                        vec2(properties_width, height),
                        egui::Layout::top_down(egui::Align::Min),
                        |ui| {
                            ui.set_max_width(properties_width);
                            ScrollArea::vertical()
                                .id_salt("pe_properties")
                                .max_height(height)
                                .show(ui, |ui| self.inspector(ui, state, &mut session));
                        },
                    );
                    ui.add(egui::Separator::default().spacing(1.0));
                    ui.allocate_ui_with_layout(
                        vec2(preview_width, height),
                        egui::Layout::top_down(egui::Align::Min),
                        |ui| {
                            ui.set_max_width(preview_width);
                            ScrollArea::vertical().id_salt("pe_preview_scroll").max_height(height).show(ui, |ui| {
                                self.preview.show(ui, state, &mut session);
                                ui.separator();
                                self.presets(ui, &mut session);
                            });
                        },
                    );
                });
                ui.separator();
                self.validation_ui(ui, state, &mut session);
                if let Some(status) = &self.status {
                    ui.label(RichText::new(status).small().color(hp_app::theme::warning()));
                }
            });
        if !open {
            self.request(Pending::Close, &mut session, state);
        }
        self.confirm_discard(ctx, state, &mut session);
        self.material_picker(ctx, state, &mut session);
        self.lua_window(ctx);
    }

    fn toolbar(&mut self, ui: &mut Ui, state: &mut AppState, session: &mut EditorSession) {
        ui.horizontal_wrapped(|ui| {
            if ui.button(t!("pe.new")).clicked() {
                self.request(Pending::New, session, state);
            }
            if ui.button(t!("pe.open")).clicked() {
                self.pick(Pick::Open, ui.ctx());
            }
            if ui.button(t!("pe.save")).clicked() {
                self.flush_drafts(session);
                if let Some(path) = session.source.as_ref().filter(|p| crate::media::is_external(p)).cloned() {
                    self.result(session.save(&PathBuf::from(path)));
                } else {
                    self.pick(Pick::Save, ui.ctx());
                }
            }
            if ui.button(t!("pe.save_as")).clicked() {
                self.flush_drafts(session);
                self.pick(Pick::Save, ui.ctx());
            }
            if ui.button(t!("pe.import")).clicked() {
                self.pick(Pick::Import, ui.ctx());
            }
            if ui.button(t!("pe.export")).clicked() {
                self.flush_drafts(session);
                self.pick(Pick::Export, ui.ctx());
            }
            ui.separator();
            if ui.add_enabled(session.doc.can_undo(), egui::Button::new(t!("pe.undo"))).clicked() {
                self.flush_drafts(session);
                if session.doc.undo() {
                    session.changed();
                }
            }
            if ui.add_enabled(session.doc.can_redo(), egui::Button::new(t!("pe.redo"))).clicked() && session.doc.redo()
            {
                session.changed();
            }
            let name = session.selected.and_then(|id| session.doc.system_name(id)).map(str::to_owned);
            if ui.add_enabled(name.is_some(), egui::Button::new(t!("pe.assign"))).clicked()
                && let Some(name) = name
                && !crate::particle_preview::assign(state, &name)
            {
                self.status = Some(t!("pe.select_entity").to_owned());
            }
            ui.separator();
            if ui.button(t!("chrome.close")).clicked() {
                self.request(Pending::Close, session, state);
            }
        });
        ui.horizontal(|ui| {
            ui.label(t!("pe.file"));
            ui.add(
                egui::TextEdit::singleline(&mut self.path)
                    .hint_text(t!("pe.path_hint"))
                    .desired_width((ui.available_width() - 190.0).max(150.0)),
            );
            if ui.button(t!("pe.load")).clicked() && !self.path.trim().is_empty() {
                self.request(Pending::Open(self.path.trim().to_owned()), session, state);
            }
            ui.menu_button(t!("pe.game_files"), |ui| {
                if let Some(catalog) = state.assets.particles() {
                    let filter = self.path.to_ascii_lowercase();
                    ScrollArea::vertical().max_height(300.0).show(ui, |ui| {
                        for file in &catalog.files {
                            if !file.to_ascii_lowercase().contains(&filter) {
                                continue;
                            }
                            if ui.button(file).clicked() {
                                self.request(Pending::Open(file.clone()), session, state);
                                ui.close();
                            }
                        }
                    });
                } else {
                    ui.label(t!("entities.sb_no_content"));
                }
            });
        });
        if let Some(source) = &session.source {
            ui.add(egui::Label::new(RichText::new(source).small().weak()).truncate()).on_hover_text(source);
        }
    }

    fn tree(&mut self, ui: &mut Ui, session: &mut EditorSession, height: f32) {
        ui.heading(t!("pe.systems"));
        ui.add(egui::TextEdit::singleline(&mut self.search).hint_text(t!("pe.search")));
        let systems = session.doc.systems();
        ScrollArea::vertical().id_salt("pe_tree").max_height((height - 230.0).max(160.0)).show(ui, |ui| {
            for id in systems {
                let Some(name) = session.doc.system_name(id).map(str::to_owned) else { continue };
                if !name.to_ascii_lowercase().contains(&self.search.to_ascii_lowercase()) {
                    continue;
                }
                ui.push_id(id, |ui| {
                    let response = ui
                        .selectable_label(session.selected == Some(id) && session.selected_element == Some(id), &name);
                    if response.clicked() {
                        self.flush_drafts(session);
                        session.select(id);
                        self.name = name.clone();
                    }
                    if session.selected != Some(id) {
                        return;
                    }
                    ui.indent("operators", |ui| {
                        for category in Category::ALL {
                            let ops = session.doc.operators(id, category);
                            egui::CollapsingHeader::new(format!("{} ({})", category_name(category), ops.len()))
                                .id_salt(category.as_str())
                                .default_open(true)
                                .show(ui, |ui| {
                                    for (index, op) in ops.into_iter().enumerate() {
                                        let label = session.doc.element(op).map_or_else(
                                            || op.to_string(),
                                            |e| e.str("functionName").unwrap_or(&e.name).to_owned(),
                                        );
                                        let response = ui.add(
                                            egui::Label::new(label)
                                                .selectable(false)
                                                .sense(egui::Sense::click_and_drag()),
                                        );
                                        if response.clicked() {
                                            self.flush_drafts(session);
                                            session.selected_element = Some(op);
                                            self.category = category;
                                        }
                                        let response = if session.selected_element == Some(op) {
                                            response.highlight()
                                        } else {
                                            response
                                        };
                                        if response.drag_started() {
                                            response.dnd_set_drag_payload(OperatorDrag { system: id, category, index });
                                        }
                                        if let Some(payload) = response.dnd_release_payload::<OperatorDrag>()
                                            && payload.system == id
                                            && payload.category == category
                                        {
                                            let result = session.doc.move_operator(id, category, payload.index, index);
                                            if self.result(result) {
                                                session.changed();
                                            }
                                        }
                                    }
                                });
                        }
                        egui::CollapsingHeader::new(t!("pe.children")).default_open(true).show(ui, |ui| {
                            for child in session.doc.children(id) {
                                if let Some(element) = session.doc.element(child) {
                                    let name = element
                                        .elements("child")
                                        .first()
                                        .and_then(|id| session.doc.system_name(*id))
                                        .unwrap_or(&element.name)
                                        .to_owned();
                                    if ui.selectable_label(session.selected_element == Some(child), name).clicked() {
                                        session.selected_element = Some(child);
                                    }
                                }
                            }
                        });
                    });
                });
            }
        });
        ui.separator();
        ui.add(egui::TextEdit::singleline(&mut self.name).hint_text(t!("pe.system_name")));
        ui.horizontal_wrapped(|ui| {
            if ui.button(t!("pe.create")).clicked() {
                let result = session.doc.create_system(self.name.trim());
                match result {
                    Ok(id) => {
                        session.changed();
                        session.select(id);
                        self.status = None;
                    }
                    Err(e) => self.status = Some(e.to_string()),
                }
            }
            if let Some(id) = session.selected {
                if ui.button(t!("pe.rename")).clicked() && self.result(session.doc.rename_system(id, self.name.trim()))
                {
                    session.changed();
                }
                if ui.button(t!("pe.duplicate")).clicked() {
                    let name = unique_name(&session.doc, session.doc.system_name(id).unwrap_or("system"));
                    match session.doc.duplicate_system(id, &name) {
                        Ok(id) => {
                            session.changed();
                            session.select(id);
                            self.name = name;
                        }
                        Err(e) => self.status = Some(e.to_string()),
                    }
                }
                if ui.button(t!("pe.delete")).clicked() && self.result(session.doc.delete_system(id)) {
                    session.changed();
                }
            }
        });
        ui.label(RichText::new(t!("pe.reorder_hint")).small().weak());
    }

    fn inspector(&mut self, ui: &mut Ui, state: &mut AppState, session: &mut EditorSession) {
        let Some(system) = session.selected else {
            ui.label(t!("pe.empty"));
            return;
        };
        let selected = session.selected_element.unwrap_or(system);
        let Some(element) = session.doc.element(selected).cloned() else { return };
        ui.heading(if selected == system { t!("pe.properties") } else { t!("pe.parameters") });
        ui.label(RichText::new(element.str("functionName").unwrap_or(&element.name)).strong());
        ui.label(RichText::new(format!("{} · {selected}", element.kind)).weak().small());
        if selected != system {
            self.operator_buttons(ui, session, system, selected);
        }
        let mut curve_element = element.clone();
        for ((id, key), value) in &self.drafts {
            if *id == selected {
                if let Some((_, existing)) = curve_element.attrs.iter_mut().find(|(name, _)| name == key) {
                    existing.clone_from(value);
                } else {
                    curve_element.attrs.push((key.clone(), value.clone()));
                }
            }
        }
        let color = session.doc.element(system).and_then(|e| e.color("color")).unwrap_or([255; 4]);
        for (key, value) in properties::source_curves_ui(ui, &curve_element, color) {
            self.drafts.insert((selected, key), value);
        }
        let mut edits: Vec<(String, Option<Value>)> = Vec::new();
        let mut inspect_target = None;
        for (key, original) in &element.attrs {
            if selected == system && (Category::parse(key).is_some() || key == "children") {
                continue;
            }
            ui.push_id((selected, key), |ui| {
                let mut remove = false;
                ui.horizontal(|ui| {
                    ui.label(RichText::new(key).strong());
                    if ui.small_button(t!("pe.remove")).clicked() {
                        edits.push((key.clone(), None));
                        remove = true;
                    }
                    if key.to_ascii_lowercase().contains("material")
                        && matches!(original, Value::Str(_))
                        && ui.small_button(t!("pe.browse_materials")).clicked()
                    {
                        self.material_target = Some((selected, key.clone()));
                    }
                });
                if remove {
                    self.drafts.remove(&(selected, key.clone()));
                    return;
                }
                let draft_key = (selected, key.clone());
                let draft = self.drafts.entry(draft_key).or_insert_with(|| original.clone());
                properties::named_value_ui(ui, key, draft, session.doc.dmx());
                let references: Vec<_> = match draft {
                    Value::Element(Some(id)) => vec![*id],
                    Value::Elements(ids) => ids.iter().flatten().copied().collect(),
                    Value::References(ids) => ids
                        .iter()
                        .filter_map(|r| match r {
                            hp_pcf::dmx::Reference::Element(Some(id)) => Some(*id),
                            _ => None,
                        })
                        .collect(),
                    _ => Vec::new(),
                };
                if !references.is_empty() {
                    ui.menu_button(t!("pe.inspect_reference"), |ui| {
                        for id in references {
                            if let Some(element) = session.doc.element(id)
                                && ui.button(format!("{id}: {} ({})", element.name, element.kind)).clicked()
                            {
                                inspect_target = Some(id);
                                ui.close();
                            }
                        }
                    });
                }
                let active = ui.ctx().egui_wants_keyboard_input() || ui.input(|i| i.pointer.any_down());
                if !active && draft != original {
                    edits.push((key.clone(), Some(draft.clone())));
                }
                if let Value::Str(material) = draft
                    && key.to_ascii_lowercase().contains("material")
                    && !material.is_empty()
                {
                    let thumb = state.assets.thumbnail(ui.ctx(), material);
                    let (rect, _) = ui.allocate_exact_size(vec2(56.0, 56.0), egui::Sense::hover());
                    paint_thumb(ui, rect, thumb);
                }
                ui.add_space(5.0);
            });
        }
        for (key, value) in edits {
            let result = match value {
                Some(value) => session.doc.set_attr(selected, &key, value),
                None => session.doc.remove_attr(selected, &key),
            };
            if self.result(result) {
                session.changed();
                self.drafts.remove(&(selected, key));
            }
        }
        if let Some(target) = inspect_target {
            self.flush_drafts(session);
            session.selected_element = Some(target);
        }
        ui.separator();
        egui::CollapsingHeader::new(t!("pe.add_parameter")).show(ui, |ui| {
            ui.add(egui::TextEdit::singleline(&mut self.parameter).hint_text(t!("pe.parameter_name")));
            egui::ComboBox::from_id_salt("pe_parameter_type").selected_text(type_name(self.parameter_type)).show_ui(
                ui,
                |ui| {
                    for i in 0..18 {
                        ui.selectable_value(&mut self.parameter_type, i, type_name(i));
                    }
                },
            );
            if ui.add_enabled(!self.parameter.trim().is_empty(), egui::Button::new(t!("pe.add"))).clicked() {
                let key = self.parameter.trim().to_owned();
                if self.result(session.doc.set_attr(selected, &key, default_value(self.parameter_type))) {
                    self.parameter.clear();
                    session.changed();
                }
            }
        });
        ui.separator();
        self.add_operator_ui(ui, session, system);
        ui.separator();
        self.children_ui(ui, session, system);
    }

    fn operator_buttons(&mut self, ui: &mut Ui, session: &mut EditorSession, system: usize, selected: usize) {
        let location = Category::ALL
            .into_iter()
            .find_map(|c| session.doc.operators(system, c).iter().position(|id| *id == selected).map(|i| (c, i)));
        let Some((category, index)) = location else {
            if let Some(index) = session.doc.children(system).iter().position(|id| *id == selected)
                && ui.button(t!("pe.remove_child")).clicked()
                && self.result(session.doc.remove_child(system, index))
            {
                session.changed();
                session.selected_element = Some(system);
            }
            return;
        };
        let len = session.doc.operators(system, category).len();
        ui.horizontal_wrapped(|ui| {
            if ui.button(t!("pe.copy")).clicked() {
                self.result(session.doc.copy_operator(selected));
            }
            if ui.button(t!("pe.paste")).clicked() {
                match session.doc.paste_operator(system, category) {
                    Ok(id) => {
                        session.changed();
                        session.selected_element = Some(id);
                    }
                    Err(e) => self.status = Some(e.to_string()),
                }
            }
            if ui.add_enabled(index > 0, egui::Button::new(t!("pe.up"))).clicked()
                && self.result(session.doc.move_operator(system, category, index, index - 1))
            {
                session.changed();
            }
            if ui.add_enabled(index + 1 < len, egui::Button::new(t!("pe.down"))).clicked()
                && self.result(session.doc.move_operator(system, category, index, index + 1))
            {
                session.changed();
            }
            if ui.button(t!("pe.delete")).clicked() && self.result(session.doc.remove_operator(system, category, index))
            {
                session.changed();
                session.selected_element = Some(system);
            }
        });
    }

    fn add_operator_ui(&mut self, ui: &mut Ui, session: &mut EditorSession, system: usize) {
        egui::CollapsingHeader::new(t!("pe.add_operator")).default_open(true).show(ui, |ui| {
            egui::ComboBox::from_id_salt("pe_category").selected_text(category_name(self.category)).show_ui(ui, |ui| {
                for category in Category::ALL {
                    ui.selectable_value(&mut self.category, category, category_name(category));
                }
            });
            let mut templates = catalog::templates(self.category);
            let known = self.templates.get(&self.category).cloned().unwrap_or_default();
            egui::ComboBox::from_id_salt("pe_operator_catalog").selected_text(t!("pe.library")).show_ui(ui, |ui| {
                for (name, _) in &templates {
                    if ui.selectable_label(self.function.eq_ignore_ascii_case(name), *name).clicked() {
                        self.function = (*name).to_owned();
                    }
                }
                if !known.is_empty() {
                    ui.separator();
                }
                for element in &known {
                    let name = element.str("functionName").unwrap_or(&element.name);
                    if ui.selectable_label(self.function.eq_ignore_ascii_case(name), name).clicked() {
                        self.function = name.to_owned();
                    }
                }
            });
            ui.add(egui::TextEdit::singleline(&mut self.function).hint_text(t!("pe.function_hint")));
            ui.label(RichText::new(t!("pe.generic_hint")).small().weak());
            ui.horizontal_wrapped(|ui| {
                if ui.add_enabled(!self.function.trim().is_empty(), egui::Button::new(t!("pe.add"))).clicked() {
                    let function = self.function.trim().to_owned();
                    let source =
                        known.iter().find(|e| e.str("functionName").unwrap_or(&e.name).eq_ignore_ascii_case(&function));
                    let defaults = templates
                        .iter_mut()
                        .find(|(name, _)| name.eq_ignore_ascii_case(&function))
                        .map(|(_, attrs)| std::mem::take(attrs));
                    let attrs = if let Some(source) = source {
                        source
                            .attrs
                            .iter()
                            .filter(|(key, value)| key != "functionName" && !contains_reference(value))
                            .cloned()
                            .collect()
                    } else {
                        defaults.unwrap_or_default().into_iter().map(|(key, value)| (key.to_owned(), value)).collect()
                    };
                    match session.doc.add_operator_with_params(system, self.category, &function, attrs) {
                        Ok(id) => {
                            session.changed();
                            session.selected_element = Some(id);
                        }
                        Err(e) => self.status = Some(e.to_string()),
                    }
                }
                if ui.button(t!("pe.paste")).clicked() {
                    match session.doc.paste_operator(system, self.category) {
                        Ok(id) => {
                            session.changed();
                            session.selected_element = Some(id);
                        }
                        Err(e) => self.status = Some(e.to_string()),
                    }
                }
            });
        });
    }

    fn children_ui(&mut self, ui: &mut Ui, session: &mut EditorSession, system: usize) {
        egui::CollapsingHeader::new(t!("pe.children")).show(ui, |ui| {
            let current = self.child.and_then(|id| session.doc.system_name(id)).unwrap_or(t!("pe.none"));
            egui::ComboBox::from_id_salt("pe_add_child").selected_text(current).show_ui(ui, |ui| {
                for id in session.doc.systems().into_iter().filter(|id| *id != system) {
                    if let Some(name) = session.doc.system_name(id) {
                        ui.selectable_value(&mut self.child, Some(id), name);
                    }
                }
            });
            ui.horizontal(|ui| {
                ui.label(t!("pe.delay"));
                ui.add(egui::DragValue::new(&mut self.child_delay).speed(0.05).range(0.0..=3600.0).suffix(" s"));
            });
            if ui.add_enabled(self.child.is_some(), egui::Button::new(t!("pe.add_child"))).clicked()
                && let Some(child) = self.child
            {
                match session.doc.add_child(system, child, self.child_delay) {
                    Ok(id) => {
                        session.changed();
                        session.selected_element = Some(id);
                    }
                    Err(e) => self.status = Some(e.to_string()),
                }
            }
        });
    }

    fn presets(&mut self, ui: &mut Ui, session: &mut EditorSession) {
        ui.label(RichText::new(t!("pe.presets")).strong());
        ui.horizontal_wrapped(|ui| {
            for preset in Preset::ALL {
                if ui.button(hp_app::i18n::tr(&format!("pe.preset.{}", preset.name()))).clicked() {
                    match session.doc.import(&Document::preset(preset)) {
                        Ok(ids) => {
                            session.changed();
                            if let Some(id) = ids.first() {
                                session.select(*id);
                            }
                        }
                        Err(e) => self.status = Some(e.to_string()),
                    }
                }
            }
        });
    }

    fn validation_ui(&mut self, ui: &mut Ui, state: &AppState, session: &mut EditorSession) {
        if self.validation_revision != Some(session.revision) {
            self.validation = session.doc.validate();
            if let Some(fs) = state.assets.fs() {
                self.validation.extend(session.doc.validate_materials(|material| fs.material(material).is_some()));
            }
            self.validation_revision = Some(session.revision);
        }
        egui::CollapsingHeader::new(tf!("pe.validation", self.validation.len())).show(ui, |ui| {
            ScrollArea::vertical().id_salt("pe_validation").max_height(130.0).show(ui, |ui| {
                if self.validation.is_empty() {
                    ui.label(t!("pe.valid"));
                }
                for issue in &self.validation {
                    let line = format!(
                        "{}{}: {}",
                        issue.element,
                        issue.attribute.as_ref().map_or_else(String::new, |k| format!(" / {k}")),
                        issue.message
                    );
                    if ui.selectable_label(false, RichText::new(line).color(hp_app::theme::warning())).clicked() {
                        session.selected_element = Some(issue.element);
                        if session.doc.system_name(issue.element).is_some() {
                            session.selected = Some(issue.element);
                        }
                    }
                }
            });
        });
    }

    fn remember_templates(&mut self, doc: &Document) {
        for system in doc.systems() {
            for category in Category::ALL {
                let entries = self.templates.entry(category).or_default();
                for id in doc.operators(system, category) {
                    let Some(element) = doc.element(id) else { continue };
                    let name = element.str("functionName").unwrap_or(&element.name);
                    if !entries.iter().any(|e| e.str("functionName").unwrap_or(&e.name).eq_ignore_ascii_case(name)) {
                        entries.push(element.clone());
                    }
                }
            }
        }
    }

    fn result<T, E: std::fmt::Display>(&mut self, result: Result<T, E>) -> bool {
        match result {
            Ok(_) => {
                self.status = None;
                true
            }
            Err(e) => {
                self.status = Some(tf!("pe.operation_failed", e));
                false
            }
        }
    }

    fn request(&mut self, action: Pending, session: &mut EditorSession, state: &AppState) {
        self.flush_drafts(session);
        if session.dirty() {
            self.pending = Some(action);
        } else {
            self.perform(action, session, state);
        }
    }

    fn perform(&mut self, action: Pending, session: &mut EditorSession, state: &AppState) {
        match action {
            Pending::New => {
                session.new_document();
                self.path.clear();
                self.name.clear();
                self.status = None;
            }
            Pending::Close => {
                session.close_discarding_changes();
                self.drafts.clear();
                self.draft_revision = session.revision;
                self.material_target = None;
                self.export_lua = None;
            }
            Pending::Open(source) => {
                let result = crate::media::read_source(state, &source, |p| {
                    let p = p.replace('\\', "/");
                    if p.starts_with("particles/") { p } else { format!("particles/{p}") }
                })
                .and_then(|(bytes, _)| session.load_bytes(&source, &bytes));
                if self.result(result) {
                    self.path = source;
                    self.remember_templates(&session.doc);
                    self.name = session.selected.and_then(|id| session.doc.system_name(id)).unwrap_or("").to_owned();
                }
            }
        }
    }

    fn flush_drafts(&mut self, session: &mut EditorSession) {
        let drafts = std::mem::take(&mut self.drafts);
        let mut elements: HashMap<usize, Vec<(String, Value)>> = HashMap::new();
        for ((id, key), value) in drafts {
            if session.doc.element(id).and_then(|e| e.get(&key)) != Some(&value) {
                elements.entry(id).or_default().push((key, value));
            }
        }
        for (id, attrs) in elements {
            if self.result(session.doc.set_attrs(id, attrs)) {
                session.changed();
            }
        }
        self.draft_revision = session.revision;
    }

    fn confirm_discard(&mut self, ctx: &egui::Context, state: &AppState, session: &mut EditorSession) {
        if self.pending.is_none() {
            return;
        }
        let mut action = None;
        let modal = egui::Modal::new(egui::Id::unique("pe_discard")).show(ctx, |ui| {
            ui.set_max_width(420.0);
            ui.heading(t!("pe.unsaved"));
            ui.label(t!("pe.discard_question"));
            ui.horizontal_wrapped(|ui| {
                if ui.button(t!("pe.discard")).clicked() {
                    action = Some(true);
                }
                if ui.button(t!("pe.cancel")).clicked() {
                    action = Some(false);
                }
            });
        });
        if action.is_none() && modal.should_close() {
            action = Some(false);
        }
        if let Some(discard) = action
            && let Some(action) = self.pending.take()
            && discard
        {
            self.perform(action, session, state);
        }
    }

    fn pick(&mut self, kind: Pick, ctx: &egui::Context) {
        if self.pick.is_some() {
            return;
        }
        let (tx, rx) = crossbeam_channel::bounded(1);
        let ctx = ctx.clone();
        let filter = t!("pe.file_filter").to_owned();
        let spawned = std::thread::Builder::new().name("hp-pcf-dialog".to_owned()).spawn(move || {
            let dialog = rfd::FileDialog::new().add_filter(filter, &["pcf"]);
            let path = match kind {
                Pick::Open | Pick::Import => dialog.pick_file(),
                Pick::Save => dialog.set_file_name("particles.pcf").save_file(),
                Pick::Export => dialog.pick_folder(),
            };
            let _ = tx.send(path);
            ctx.request_repaint();
        });
        match spawned {
            Ok(_) => self.pick = Some((kind, rx)),
            Err(e) => self.status = Some(e.to_string()),
        }
    }

    fn poll_pick(&mut self, state: &AppState, session: &mut EditorSession) {
        let Some((kind, rx)) = self.pick.as_ref() else { return };
        let kind = *kind;
        let result = match rx.try_recv() {
            Ok(path) => path,
            Err(crossbeam_channel::TryRecvError::Empty) => return,
            Err(crossbeam_channel::TryRecvError::Disconnected) => {
                self.pick = None;
                return;
            }
        };
        self.pick = None;
        let Some(path) = result else { return };
        match kind {
            Pick::Open => self.request(Pending::Open(path.to_string_lossy().into_owned()), session, state),
            Pick::Save => {
                self.result(session.save(&path));
            }
            Pick::Import => {
                let result = crate::media::read_source(state, &path.to_string_lossy(), str::to_owned)
                    .and_then(|(bytes, _)| Document::parse(&bytes).map_err(|e| e.to_string()));
                match result {
                    Ok(source) => {
                        self.remember_templates(&source);
                        match session.doc.import(&source) {
                            Ok(ids) => {
                                session.changed();
                                if let Some(id) = ids.first() {
                                    session.select(*id);
                                }
                            }
                            Err(e) => self.status = Some(e.to_string()),
                        }
                    }
                    Err(e) => self.status = Some(e),
                }
            }
            Pick::Export => {
                let file = session
                    .source
                    .as_ref()
                    .and_then(|s| std::path::Path::new(s).file_name())
                    .map_or_else(|| "hammerpro_particles.pcf".to_owned(), |s| s.to_string_lossy().into_owned());
                match session.doc.export_addon(&path, &file) {
                    Ok(result) => {
                        self.export_lua = Some(result.lua);
                        self.status = Some(tf!("pe.exported", result.pcf.display()));
                    }
                    Err(e) => self.status = Some(e.to_string()),
                }
            }
        }
    }

    fn material_picker(&mut self, ctx: &egui::Context, state: &mut AppState, session: &mut EditorSession) {
        let Some((element, key)) = self.material_target.clone() else { return };
        let mut open = true;
        let mut selected = None;
        hp_app::chrome::Window::new(t!("pe.browse_materials"))
            .id(egui::Id::unique("pe_materials"))
            .open(&mut open)
            .default_size([620.0, 500.0])
            .show(ctx, |ui| {
                ui.add(
                    egui::TextEdit::singleline(&mut self.material_search)
                        .hint_text(t!("pe.material_search"))
                        .desired_width(f32::INFINITY),
                );
                let catalog = state.assets.materials();
                let key = (state.assets.generation(), self.material_search.clone(), catalog.len());
                if self.material_key.as_ref() != Some(&key) {
                    let query = self.material_search.to_ascii_lowercase();
                    self.material_rows = catalog
                        .iter()
                        .enumerate()
                        .filter(|(_, e)| e.name.to_ascii_lowercase().contains(&query))
                        .map(|(i, _)| i)
                        .collect();
                    self.material_key = Some(key);
                }
                let columns = (ui.available_width() / 108.0).floor().max(1.0) as usize;
                ScrollArea::vertical().id_salt("pe_material_grid").max_height(430.0).show_rows(
                    ui,
                    110.0,
                    self.material_rows.len().div_ceil(columns),
                    |ui, range| {
                        for row in range {
                            ui.horizontal(|ui| {
                                for index in self.material_rows.iter().skip(row * columns).take(columns) {
                                    let Some(item) = catalog.get(*index) else { continue };
                                    ui.allocate_ui_with_layout(
                                        vec2(100.0, 105.0),
                                        egui::Layout::top_down(egui::Align::Center),
                                        |ui| {
                                            ui.set_max_width(100.0);
                                            let (rect, response) =
                                                ui.allocate_exact_size(vec2(78.0, 78.0), egui::Sense::click());
                                            let thumb = state.assets.thumbnail(ctx, &item.name);
                                            paint_thumb(ui, rect, thumb);
                                            let response = response.on_hover_text(&item.name);
                                            if response.clicked() {
                                                selected = Some(item.name.clone());
                                            }
                                            ui.add(
                                                egui::Label::new(item.name.rsplit('/').next().unwrap_or(&item.name))
                                                    .truncate(),
                                            );
                                        },
                                    );
                                }
                            });
                        }
                    },
                );
            });
        if let Some(material) = selected {
            if self.result(session.doc.set_attr(element, &key, Value::Str(material))) {
                session.changed();
            }
            open = false;
        }
        if !open {
            self.material_target = None;
        }
    }

    fn lua_window(&mut self, ctx: &egui::Context) {
        let Some(lua) = self.export_lua.as_ref() else { return };
        let lua = lua.clone();
        let mut open = true;
        hp_app::chrome::Window::new(t!("pe.export_lua"))
            .id(egui::Id::unique("pe_lua"))
            .open(&mut open)
            .default_width(560.0)
            .show(ctx, |ui| {
                ui.label(t!("pe.lua_hint"));
                ScrollArea::both().max_height(300.0).show(ui, |ui| {
                    ui.label(RichText::new(&lua).monospace());
                });
                if ui.button(t!("pe.copy")).clicked() {
                    ui.ctx().copy_text(lua);
                }
            });
        if !open {
            self.export_lua = None;
        }
    }
}

pub type OperatorCatalogEntry = (String, String, Vec<(String, Value)>);

pub fn operator_catalog() -> Vec<OperatorCatalogEntry> {
    Category::ALL
        .into_iter()
        .flat_map(|category| {
            catalog::templates(category).into_iter().map(move |(name, attrs)| {
                (
                    category.as_str().to_owned(),
                    name.to_owned(),
                    attrs.into_iter().map(|(k, v)| (k.to_owned(), v)).collect(),
                )
            })
        })
        .collect()
}

pub fn operator_templates(category: Category) -> Vec<(&'static str, Vec<(&'static str, Value)>)> {
    catalog::templates(category)
}

fn category_name(category: Category) -> &'static str {
    hp_app::i18n::tr(match category {
        Category::Emitters => "pe.emitters",
        Category::Initializers => "pe.initializers",
        Category::Operators => "pe.operators",
        Category::Forces => "pe.forces",
        Category::Constraints => "pe.constraints",
        Category::Renderers => "pe.renderers",
    })
}

fn type_name(index: usize) -> &'static str {
    hp_app::i18n::tr(match index {
        0 => "pe.type.float",
        1 => "pe.type.int",
        2 => "pe.type.bool",
        3 => "pe.type.string",
        4 => "pe.type.vec2",
        5 => "pe.type.vec3",
        6 => "pe.type.vec4",
        7 => "pe.type.color",
        8 => "pe.type.floats",
        9 => "pe.type.vec3s",
        10 => "pe.type.colors",
        11 => "pe.type.element",
        12 => "pe.type.elements",
        13 => "pe.type.time",
        14 => "pe.type.angles",
        15 => "pe.type.quaternion",
        16 => "pe.type.matrix",
        _ => "pe.type.raw",
    })
}

fn default_value(index: usize) -> Value {
    match index {
        0 => Value::Float(0.0),
        1 => Value::Int(0),
        2 => Value::Bool(false),
        3 => Value::Str(String::new()),
        4 => Value::Vec2([0.0; 2]),
        5 => Value::Vec3([0.0; 3]),
        6 => Value::Vec4([0.0; 4]),
        7 => Value::Color([255; 4]),
        8 => Value::Floats(vec![0.0, 1.0]),
        9 => Value::Vec3s(Vec::new()),
        10 => Value::Raw { kind: 22, data: 2i32.to_le_bytes().into_iter().chain([255u8; 4]).chain([0u8; 4]).collect() },
        11 => Value::Element(None),
        12 => Value::Elements(Vec::new()),
        13 => Value::Raw { kind: 7, data: vec![0; 4] },
        14 => Value::Raw { kind: 12, data: vec![0; 12] },
        15 => Value::Raw { kind: 13, data: [0.0f32, 0.0, 0.0, 1.0].into_iter().flat_map(f32::to_le_bytes).collect() },
        16 => Value::Raw {
            kind: 14,
            data: (0..16).flat_map(|i| if i % 5 == 0 { 1.0f32 } else { 0.0f32 }.to_le_bytes()).collect(),
        },
        _ => Value::Raw { kind: 6, data: vec![0; 4] },
    }
}

fn contains_reference(value: &Value) -> bool {
    matches!(value, Value::Element(_) | Value::Elements(_) | Value::External(_) | Value::References(_))
}

fn unique_name(doc: &Document, base: &str) -> String {
    let mut suffix = 2;
    let mut candidate = format!("{base}_{suffix}");
    while doc.find_system(&candidate).is_some() {
        suffix += 1;
        candidate = format!("{base}_{suffix}");
    }
    candidate
}

fn paint_thumb(ui: &Ui, rect: egui::Rect, thumb: hp_app::state::ThumbState) {
    ui.painter().rect_filled(rect, 3.0, ui.visuals().extreme_bg_color);
    match thumb {
        hp_app::state::ThumbState::Ready { texture, .. } => {
            ui.painter().image(
                texture,
                rect,
                egui::Rect::from_min_max(egui::Pos2::ZERO, egui::pos2(1.0, 1.0)),
                egui::Color32::WHITE,
            );
        }
        hp_app::state::ThumbState::Loading => {
            ui.painter().text(
                rect.center(),
                egui::Align2::CENTER_CENTER,
                "...",
                egui::FontId::proportional(14.0),
                ui.visuals().weak_text_color(),
            );
        }
        hp_app::state::ThumbState::Missing => {
            ui.painter().text(
                rect.center(),
                egui::Align2::CENTER_CENTER,
                "?",
                egui::FontId::proportional(14.0),
                hp_app::theme::warning(),
            );
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn particle_editor_plays_presets_and_preserves_pause_and_control_points() {
        crate::register_translations();
        let mut state = hp_app::testing::test_state();
        let handle = new_editor_handle();
        {
            let mut session = handle.lock().unwrap();
            session.doc = Document::preset(Preset::Fire);
            session.changed();
            session.open = true;
            session.preview.points = vec![[0.0; 3], [10.0, 20.0, 30.0]];
        }
        let mut window = ParticleEditorWindow::new(crate::particles::new_handle(), handle.clone());
        hp_app::testing::run_ctx(&mut state, |ctx, state| window.show(ctx, state));
        let count = handle.lock().unwrap().preview.particles;
        assert!(count > 0);
        assert_eq!(handle.lock().unwrap().preview.points[1], [10.0, 20.0, 30.0]);
        handle.lock().unwrap().preview.paused = true;
        hp_app::testing::run_ctx(&mut state, |ctx, state| window.show(ctx, state));
        assert_eq!(handle.lock().unwrap().preview.particles, count);
        assert!(window.templates.get(&Category::Emitters).is_some_and(|entries| !entries.is_empty()));
    }

    #[test]
    fn particle_parameter_types_survive_binary_roundtrip_and_draw() {
        crate::register_translations();
        let mut state = hp_app::testing::test_state();
        let mut document = Document::new();
        let system = document.create_system("types").unwrap();
        for index in 0..18 {
            document.set_attr(system, &format!("type{index}"), default_value(index)).unwrap();
        }
        let bytes = document.to_bytes().unwrap();
        let parsed = Document::parse(&bytes).unwrap();
        assert_eq!(document.dmx(), parsed.dmx());
        hp_app::testing::run_ui(&mut state, |ui, _| {
            for index in 0..18 {
                ui.push_id(index, |ui| {
                    let mut value = default_value(index);
                    assert!(!properties::value_ui(ui, &mut value, parsed.dmx()));
                });
            }
            let mut retained = Value::Int(200_000);
            assert!(!properties::named_value_ui(ui, "max_particles", &mut retained, parsed.dmx()));
            assert_eq!(retained, Value::Int(200_000));
            let mut retained = Value::Float(1.0e30);
            assert!(!properties::value_ui(ui, &mut retained, parsed.dmx()));
            assert_eq!(retained, Value::Float(1.0e30));
            for function in ["radius scale", "alpha fade and decay", "color fade"] {
                ui.push_id(function, |ui| {
                    let element = Element {
                        attrs: vec![("functionName".to_owned(), Value::Str(function.to_owned()))],
                        ..Default::default()
                    };
                    assert!(properties::source_curves_ui(ui, &element, [255; 4]).is_empty());
                });
            }
        });
    }

    #[test]
    fn particle_session_saves_atomically_and_dirty_tracks_undo_to_saved() {
        let root = PathBuf::from("E:/hp-scratch/ui111");
        std::fs::create_dir_all(&root).unwrap();
        let path = root.join(format!("session-{}.pcf", std::process::id()));
        let mut session = EditorSession::default();
        let system = session.doc.create_system("saved").unwrap();
        session.changed();
        assert!(session.dirty());
        session.save(&path).unwrap();
        assert!(!session.dirty());
        session.doc.rename_system(system, "renamed").unwrap();
        session.changed();
        assert!(session.dirty());
        session.doc.undo();
        session.changed();
        assert!(!session.dirty());
        session.save(&path).unwrap();
        let mut loaded = EditorSession::default();
        loaded.load_bytes("saved.pcf", &std::fs::read(&path).unwrap()).unwrap();
        assert!(!loaded.dirty());
        assert_eq!(loaded.doc.system_name(system), Some("saved"));
        std::fs::remove_file(path).unwrap();
    }

    #[test]
    fn particle_drafts_are_committed_before_save_or_discard_checks() {
        crate::register_translations();
        let state = hp_app::testing::test_state();
        let handle = new_editor_handle();
        let mut session = EditorSession::default();
        let system = session.doc.create_system("draft").unwrap();
        session.doc.mark_saved();
        let mut window = ParticleEditorWindow::new(crate::particles::new_handle(), handle);
        window.drafts.insert((system, "radius".to_owned()), Value::Float(20.0));
        window.request(Pending::Close, &mut session, &state);
        assert!(window.pending.is_some());
        assert_eq!(session.doc.element(system).unwrap().float("radius"), Some(20.0));
        assert!(session.doc.undo());
        assert!(!session.dirty());
    }

    #[test]
    fn particle_pending_edits_are_visible_to_the_global_exit_guard() {
        let handle = new_editor_handle();
        let system = {
            let mut session = handle.lock().unwrap();
            let id = session.doc.create_system("exit_draft").unwrap();
            session.doc.mark_saved();
            id
        };
        let mut window = ParticleEditorWindow::new(crate::particles::new_handle(), handle.clone());
        window.drafts.insert((system, "radius".to_owned()), Value::Float(18.0));
        window.drafts.insert((system, "initial_particles".to_owned()), Value::Int(42));
        window.flush_pending_edits();
        let mut session = handle.lock().unwrap();
        assert!(session.dirty());
        assert_eq!(session.doc.element(system).unwrap().float("radius"), Some(18.0));
        assert_eq!(session.doc.element(system).unwrap().int("initial_particles"), Some(42));
        assert!(session.doc.undo());
        assert!(!session.dirty());
    }

    #[test]
    fn particle_window_layout_does_not_grow_on_every_frame() {
        crate::register_translations();
        let mut state = hp_app::testing::test_state();
        let handle = new_editor_handle();
        {
            let mut session = handle.lock().unwrap();
            session.doc = Document::preset(Preset::Smoke);
            session.open = true;
            session.changed();
        }
        let mut window = ParticleEditorWindow::new(crate::particles::new_handle(), handle);
        let ctx = egui::Context::default();
        let mut previous = egui::Rect::NOTHING;
        for index in 0..40 {
            let input = egui::RawInput {
                screen_rect: Some(egui::Rect::from_min_size(egui::Pos2::ZERO, vec2(1600.0, 900.0))),
                ..Default::default()
            };
            let _ = ctx.run_ui(input, |ui| window.show(ui.ctx(), &mut state));
            let bounds = ctx.memory(|memory| memory.area_rect(egui::Id::unique("particle_editor"))).unwrap();
            assert!(bounds.height() <= 900.0, "{bounds:?}");
            assert!(bounds.width() < 1450.0, "{bounds:?}");
            if index >= 20 {
                assert!((bounds.width() - previous.width()).abs() <= 1.0, "{previous:?} -> {bounds:?}");
            }
            previous = bounds;
        }
    }
}
