use egui::{RichText, Ui};
use hp_app::{AppState, Severity, t, tf, theme};

use crate::vmt_params::{self, Category, Kind, VmtParam};

pub(crate) struct SingleEditor {
    pub name: Option<String>,
    pub text: String,
    pub saved: String,
    page: Category,
    show_all: bool,
    filter: String,
    new_key: String,
    new_value: String,
    pub error: Option<String>,
    pub file_preview: Option<crate::matedit_preview::FilePreview>,
}

impl Default for SingleEditor {
    fn default() -> Self {
        Self {
            name: None,
            text: String::new(),
            saved: String::new(),
            page: Category::Base,
            show_all: false,
            filter: String::new(),
            new_key: String::new(),
            new_value: String::new(),
            error: None,
            file_preview: None,
        }
    }
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub(crate) enum SingleAction {
    Save,
    Select(usize),
}

impl SingleEditor {
    pub fn load(&mut self, name: &str, text: Option<&str>) {
        let text = text.map_or_else(|| vmt_params::new_material_text("LightmappedGeneric"), str::to_owned);
        self.name = Some(name.to_owned());
        self.saved = text.clone();
        self.text = text;
        self.error = None;
    }

    pub fn is_dirty(&self) -> bool {
        self.name.is_some() && self.text != self.saved
    }

    pub fn set(&mut self, key: &str, value: Option<&str>) {
        let r = match value {
            Some(v) => hp_vmt::set_param_in_text(&self.text, key, v),
            None => hp_vmt::remove_param_in_text(&self.text, key),
        };
        match r {
            Ok(t) => {
                self.text = t;
                self.error = None;
            }
            Err(e) => self.error = Some(e.to_string()),
        }
    }

    pub fn set_shader(&mut self, shader: &str) {
        match vmt_params::set_shader_in_text(&self.text, shader) {
            Some(t) => {
                self.text = t;
                self.error = None;
            }
            None => self.error = Some(t!("tex.me1.no_shader").to_owned()),
        }
    }

    pub fn ui(
        &mut self,
        ui: &mut Ui,
        state: &mut AppState,
        names: &[String],
        active: usize,
        location: &mut dyn FnMut(&mut Ui),
    ) -> Option<SingleAction> {
        let mut action = None;
        let fr = hp_app::i18n::lang() == hp_app::i18n::Lang::Fr;
        let parsed = hp_vmt::parse(&self.text).ok();
        let shader = parsed.as_ref().map(|m| m.shader.clone()).unwrap_or_default();
        let family = vmt_params::family_of(&shader);

        ui.horizontal_wrapped(|ui| {
            if names.len() > 1 {
                if ui.small_button("◀").clicked() {
                    action = Some(SingleAction::Select((active + names.len() - 1) % names.len()));
                }
                let current = names.get(active).cloned().unwrap_or_default();
                egui::ComboBox::from_id_salt("hp_me1_material").selected_text(&current).width(260.0).show_ui(
                    ui,
                    |ui| {
                        for (i, n) in names.iter().enumerate() {
                            if ui.selectable_label(i == active, n).clicked() {
                                action = Some(SingleAction::Select(i));
                            }
                        }
                    },
                );
                if ui.small_button("▶").clicked() {
                    action = Some(SingleAction::Select((active + 1) % names.len()));
                }
            } else if let Some(n) = &self.name {
                ui.label(RichText::new(n).strong());
            }
            location(ui);
            if self.is_dirty() {
                ui.colored_label(theme::warning(), t!("tex.me1.modified"));
            }
            let save = ui.add_enabled(self.is_dirty(), egui::Button::new(RichText::new(t!("tex.me1.save")).strong()));
            if save.on_hover_text(t!("tex.me1.save_tip")).clicked() {
                action = Some(SingleAction::Save);
            }
            if ui.add_enabled(self.is_dirty(), egui::Button::new(t!("tex.me1.revert"))).clicked() {
                self.text = self.saved.clone();
                self.error = None;
            }
        });
        ui.horizontal_wrapped(|ui| {
            ui.label(t!("tex.me.shader"));
            let mut chosen = None;
            egui::ComboBox::from_id_salt("hp_me1_shader").selected_text(&shader).width(200.0).height(420.0).show_ui(
                ui,
                |ui| {
                    for (s, _) in vmt_params::SHADERS {
                        if ui.selectable_label(s.eq_ignore_ascii_case(&shader), *s).clicked() {
                            chosen = Some(*s);
                        }
                    }
                },
            );
            if let Some(s) = chosen
                && !s.eq_ignore_ascii_case(&shader)
            {
                self.set_shader(s);
            }
            ui.checkbox(&mut self.show_all, t!("tex.me1.show_all")).on_hover_text(t!("tex.me1.show_all_tip"));
            ui.add(egui::TextEdit::singleline(&mut self.filter).hint_text(t!("tex.me1.filter")).desired_width(160.0));
        });
        if let Some(e) = &self.error {
            ui.colored_label(theme::error(), e);
        }
        ui.separator();

        let Some(mat) = parsed else {
            ui.colored_label(theme::error(), t!("tex.me1.unreadable"));
            egui::ScrollArea::vertical().id_salt("hp_me1_text_only").show(ui, |ui| {
                let r = ui.add(egui::TextEdit::multiline(&mut self.text).code_editor().desired_width(f32::INFINITY));
                hp_app::dialog_keys::keep_tab_for(ui.ctx(), r.id);
            });
            return action;
        };

        egui::Panel::left("hp_me1_pages").resizable(false).exact_size(190.0).show(ui, |ui| {
            egui::ScrollArea::vertical().id_salt("hp_me1_pages_scroll").show(ui, |ui| {
                for c in Category::ALL {
                    let params: Vec<&VmtParam> = vmt_params::params_of(c, family, self.show_all).collect();
                    let set = if c == Category::Other {
                        other_params(&mat).len()
                    } else {
                        params.iter().filter(|p| mat.has(p.key)).count()
                    };
                    if params.is_empty() && c != Category::Other && set == 0 {
                        continue;
                    }
                    let (en, frl) = c.label();
                    let label = if set > 0 {
                        format!("{} ({set})", if fr { frl } else { en })
                    } else {
                        (if fr { frl } else { en }).to_owned()
                    };
                    let text = if set > 0 { RichText::new(label).strong() } else { RichText::new(label) };
                    if ui.selectable_label(self.page == c, text).clicked() {
                        self.page = c;
                    }
                }
            });
        });
        egui::Panel::right("hp_me1_side").resizable(true).default_size(300.0).show(ui, |ui| {
            if let Some(name) = &self.name {
                let size = ui.available_width().min(220.0);
                let (rect, _) = ui.allocate_exact_size(egui::vec2(size, size), egui::Sense::hover());
                ui.painter().rect_filled(rect, 3.0, ui.visuals().extreme_bg_color);
                let thumb = if let Some(preview) = &mut self.file_preview {
                    preview.thumbnail(ui.ctx(), &self.saved, state.assets.fs())
                } else {
                    state.assets.thumbnail(ui.ctx(), name)
                };
                crate::browser::paint_thumbnail(ui.painter(), rect, thumb, false);
                ui.label(RichText::new(t!("tex.me1.preview_note")).small().weak());
            }
            ui.add_space(4.0);
            self.linked_textures_ui(ui, state, &mat);
            ui.add_space(4.0);
            ui.label(RichText::new(t!("tex.me1.text")).strong());
            egui::ScrollArea::vertical().id_salt("hp_me1_text").show(ui, |ui| {
                let r = ui.add(
                    egui::TextEdit::multiline(&mut self.text)
                        .code_editor()
                        .desired_rows(14)
                        .desired_width(f32::INFINITY),
                );
                hp_app::dialog_keys::keep_tab_for(ui.ctx(), r.id);
            });
        });

        egui::ScrollArea::vertical().id_salt(("hp_me1_params", self.page as u8)).auto_shrink([false, false]).show(
            ui,
            |ui| {
                let filter = self.filter.trim().to_ascii_lowercase();
                if self.page == Category::Other {
                    self.other_ui(ui, &mat);
                    return;
                }
                let params: Vec<&VmtParam> = vmt_params::params_of(self.page, family, self.show_all)
                    .filter(|p| {
                        filter.is_empty()
                            || p.key.contains(filter.as_str())
                            || p.english.to_ascii_lowercase().contains(filter.as_str())
                            || p.french.to_lowercase().contains(filter.as_str())
                    })
                    .collect();
                if params.is_empty() {
                    ui.weak(t!("tex.me1.nothing_here"));
                }
                for param in params {
                    self.param_row(ui, state, param, mat.get(param.key), fr);
                    ui.separator();
                }
            },
        );
        action
    }

    fn param_row(&mut self, ui: &mut Ui, state: &mut AppState, param: &VmtParam, value: Option<&str>, fr: bool) {
        let set = value.is_some();
        ui.horizontal(|ui| {
            let key = RichText::new(param.key).monospace();
            ui.label(if set { key.strong() } else { key.weak() });
            ui.with_layout(egui::Layout::right_to_left(egui::Align::Center), |ui| {
                if set && ui.small_button("✖").on_hover_text(t!("tex.me1.reset")).clicked() {
                    self.set(param.key, None);
                }
                let shown = value.unwrap_or(param.default);
                let material = self.name.clone().unwrap_or_default();
                if let Some(new) = widget(ui, state, param, shown, set, &material) {
                    let new = new.trim().to_owned();
                    if new.is_empty() {
                        self.set(param.key, None);
                    } else {
                        self.set(param.key, Some(&new));
                    }
                }
            });
        });
        ui.label(RichText::new(if fr { param.french } else { param.english }).small().weak());
    }

    fn linked_textures_ui(&mut self, ui: &mut Ui, state: &mut AppState, mat: &hp_vmt::Material) {
        let linked: Vec<(&'static str, String)> = vmt_params::PARAMS
            .iter()
            .filter(|p| p.kind == Kind::Texture)
            .filter_map(|p| mat.get(p.key).map(str::trim).filter(|v| !v.is_empty()).map(|v| (p.key, v.to_owned())))
            .collect();
        ui.label(RichText::new(t!("tex.me1.linked")).strong());
        if linked.is_empty() {
            ui.label(RichText::new(t!("tex.me1.linked_none")).small().weak());
            return;
        }
        let material = self.name.clone().unwrap_or_default();
        egui::Grid::new("hp_me1_linked").num_columns(3).spacing([6.0, 3.0]).show(ui, |ui| {
            for (key, texture) in &linked {
                ui.label(RichText::new(*key).monospace().small());
                ui.add(egui::Label::new(RichText::new(texture).small()).truncate()).on_hover_text(texture);
                vtf_button(ui, state, texture, key, &material);
                ui.end_row();
            }
        });
        ui.label(RichText::new(t!("tex.me1.linked_tip")).small().weak());
    }

    fn other_ui(&mut self, ui: &mut Ui, mat: &hp_vmt::Material) {
        ui.label(RichText::new(t!("tex.me1.other_tip")).small().weak());
        let mut changes: Vec<(String, Option<String>)> = Vec::new();
        egui::Grid::new("hp_me1_other").num_columns(3).spacing([8.0, 4.0]).striped(true).show(ui, |ui| {
            for (key, value) in other_params(mat) {
                ui.label(RichText::new(&key).monospace());
                let mut v = value.clone();
                if ui.add(egui::TextEdit::singleline(&mut v).desired_width(220.0)).changed() {
                    changes.push((key.clone(), Some(v)));
                }
                if ui.small_button("✖").on_hover_text(t!("common.remove")).clicked() {
                    changes.push((key, None));
                }
                ui.end_row();
            }
        });
        for (k, v) in changes {
            self.set(&k, v.as_deref());
        }
        ui.add_space(6.0);
        ui.horizontal(|ui| {
            ui.add(egui::TextEdit::singleline(&mut self.new_key).hint_text("$parameter").desired_width(150.0));
            ui.add(egui::TextEdit::singleline(&mut self.new_value).hint_text(t!("tex.me1.value")).desired_width(160.0));
            let ok = !self.new_key.trim().is_empty() && !self.new_value.trim().is_empty();
            if ui.add_enabled(ok, egui::Button::new(t!("common.add"))).clicked() {
                let (k, v) = (self.new_key.trim().to_owned(), self.new_value.trim().to_owned());
                self.set(&k, Some(&v));
                if self.error.is_none() {
                    self.new_key.clear();
                    self.new_value.clear();
                }
            }
        });
    }
}

fn other_params(mat: &hp_vmt::Material) -> Vec<(String, String)> {
    mat.params.iter().filter(|(k, _)| vmt_params::find(k).is_none()).cloned().collect()
}

fn widget(
    ui: &mut Ui,
    state: &mut AppState,
    param: &VmtParam,
    shown: &str,
    set: bool,
    material: &str,
) -> Option<String> {
    let id = ui.scope_id().with(("hp_me1_param", param.key));
    match param.kind {
        Kind::Flag => {
            let mut on = set && shown.trim() != "0";
            ui.checkbox(&mut on, "").changed().then(|| if on { "1".to_owned() } else { String::new() })
        }
        Kind::Float { min, max } => {
            let mut v: f32 = shown.trim().parse().unwrap_or(0.0);
            let r = if max - min <= 2.0 {
                ui.add(egui::Slider::new(&mut v, min..=max).max_decimals(3))
            } else {
                ui.add(egui::DragValue::new(&mut v).range(min..=max).speed(f64::from((max - min) / 500.0).max(0.01)))
            };
            r.changed().then(|| vmt_params::format_number(v))
        }
        Kind::Int { min, max } => {
            let mut v: i32 = shown.trim().parse().unwrap_or(0);
            ui.add(egui::DragValue::new(&mut v).range(min..=max)).changed().then(|| v.to_string())
        }
        Kind::Color => {
            let c = vmt_params::parse_color(shown).unwrap_or([1.0, 1.0, 1.0]);
            let mut text = shown.to_owned();
            let mut out = None;
            if ui.add(egui::TextEdit::singleline(&mut text).desired_width(110.0)).changed() {
                out = Some(text);
            }
            let mut rgb = [c[0].clamp(0.0, 1.0), c[1].clamp(0.0, 1.0), c[2].clamp(0.0, 1.0)];
            if egui::color_picker::color_edit_button_rgb(ui, &mut rgb).changed() {
                out = Some(vmt_params::format_color(rgb));
            }
            out
        }
        Kind::Choice(list) => {
            let fr = hp_app::i18n::lang() == hp_app::i18n::Lang::Fr;
            let current = list.iter().find(|(v, _, _)| v.eq_ignore_ascii_case(shown.trim()));
            let text = current.map_or_else(|| shown.to_owned(), |(_, en, f)| (if fr { *f } else { *en }).to_owned());
            let mut out = None;
            egui::ComboBox::from_id_salt(id).selected_text(text).width(200.0).show_ui(ui, |ui| {
                for (v, en, f) in list {
                    if ui.selectable_label(current.is_some_and(|c| c.0 == *v), if fr { *f } else { *en }).clicked() {
                        out = Some((*v).to_owned());
                    }
                }
            });
            out
        }
        Kind::SurfaceProp => {
            let mut out = None;
            egui::ComboBox::from_id_salt(id).selected_text(shown).width(180.0).height(400.0).show_ui(ui, |ui| {
                for sp in crate::util::SURFACE_PROPS {
                    if ui.selectable_label(shown.eq_ignore_ascii_case(sp), *sp).clicked() {
                        out = Some((*sp).to_owned());
                    }
                }
            });
            out
        }
        Kind::Texture => {
            let mut text = if set { shown.to_owned() } else { String::new() };
            let mut out = None;
            if set {
                vtf_button(ui, state, shown, param.key, material);
            }
            if ui.button("…").on_hover_text(t!("tex.me1.pick_texture")).clicked()
                && let Some(path) = pick_texture(state)
            {
                out = Some(path);
            }
            if ui.add(egui::TextEdit::singleline(&mut text).desired_width(240.0).hint_text("folder/texture")).changed()
            {
                out = Some(text);
            }
            out
        }
        Kind::Vec3 | Kind::Text => {
            let mut text = if set { shown.to_owned() } else { String::new() };
            ui.add(egui::TextEdit::singleline(&mut text).desired_width(240.0).hint_text(param.default))
                .changed()
                .then_some(text)
        }
    }
}

fn vtf_button(ui: &mut Ui, state: &mut AppState, texture: &str, key: &str, material: &str) {
    let texture = hp_vmt::normalize_texture_path(texture);
    let external = crate::vtfedit::external_editor_ui(state);
    let tip = match &external {
        Some(e) if !state.settings.vtf.prefer_integrated => tf!("tex.me1.vtf_tip_external", e.label),
        _ => t!("tex.me1.vtf_tip_integrated").to_owned(),
    };
    let r = ui.add_enabled(!texture.is_empty(), egui::Button::new(t!("tex.me1.vtf_edit")).small()).on_hover_text(tip);
    let request = |external: Option<bool>| hp_app::runtime::VtfRequest {
        texture: Some(texture.clone()),
        file: None,
        external,
        materials: if material.is_empty() { Vec::new() } else { vec![material.to_owned()] },
        param: Some(key.to_owned()),
    };
    if r.clicked() {
        state.runtime.vtf.open(request(None));
    }
    r.context_menu(|ui| {
        if ui.button(t!("tex.me1.vtf_integrated")).clicked() {
            state.runtime.vtf.open(request(Some(false)));
            ui.close();
        }
        let label = match &external {
            Some(e) => tf!("tex.me1.vtf_external_named", e.label),
            None => t!("tex.me1.vtf_external").to_owned(),
        };
        if ui.add_enabled(external.is_some(), egui::Button::new(label)).clicked() {
            state.runtime.vtf.open(request(Some(true)));
            ui.close();
        }
    });
}

fn pick_texture(state: &mut AppState) -> Option<String> {
    let file = rfd::FileDialog::new().add_filter("VTF", &["vtf", "png", "tga", "jpg", "jpeg"]).pick_file()?;
    match texture_path_of(&file) {
        Some(p) => Some(p),
        None => {
            state.notify(Severity::Warning, tf!("tex.me1.not_in_materials", file.display()));
            None
        }
    }
}

pub(crate) fn texture_path_of(file: &std::path::Path) -> Option<String> {
    let parts: Vec<String> = file.components().map(|c| c.as_os_str().to_string_lossy().into_owned()).collect();
    let at = parts.iter().rposition(|p| p.eq_ignore_ascii_case("materials"))?;
    let rest = &parts[at + 1..];
    if rest.is_empty() {
        return None;
    }
    let mut out = rest.join("/");
    if let Some(dot) = out.rfind('.')
        && !out[dot..].contains('/')
    {
        out.truncate(dot);
    }
    Some(out.to_ascii_lowercase())
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn edits_keep_the_text_and_track_changes() {
        let mut e = SingleEditor::default();
        e.load("brick/wall", Some("\"LightmappedGeneric\"\n{\n\t$basetexture brick/wall // main\n}\n"));
        assert!(!e.is_dirty());
        e.set("$surfaceprop", Some("brick"));
        e.set("$basetexture", Some("brick/wall2"));
        assert!(e.is_dirty());
        assert!(e.text.contains("// main"), "comments stay: {}", e.text);
        let m = hp_vmt::parse(&e.text).expect("parse");
        assert_eq!(m.get("$surfaceprop"), Some("brick"));
        assert_eq!(m.get("$basetexture"), Some("brick/wall2"));
        e.set("$surfaceprop", None);
        assert!(!hp_vmt::parse(&e.text).expect("parse").has("$surfaceprop"));
        e.set_shader("VertexLitGeneric");
        assert_eq!(hp_vmt::parse(&e.text).expect("parse").shader, "VertexLitGeneric");
        e.set("bad\"key", Some("1"));
        assert!(e.error.is_some());
        e.load("new/one", None);
        assert_eq!(hp_vmt::parse(&e.text).expect("parse").shader, "LightmappedGeneric");
        let other = hp_vmt::parse("\"UnlitGeneric\" { $basetexture a $mycustom 3 }").expect("parse");
        assert_eq!(other_params(&other), vec![("$mycustom".to_owned(), "3".to_owned())]);
    }

    #[test]
    fn texture_paths_from_files() {
        let p = std::path::Path::new("C:/game/garrysmod/materials/Brick/Wall01.vtf");
        assert_eq!(texture_path_of(p).as_deref(), Some("brick/wall01"));
        assert_eq!(texture_path_of(std::path::Path::new("C:/x/wall.vtf")), None);
    }
}
