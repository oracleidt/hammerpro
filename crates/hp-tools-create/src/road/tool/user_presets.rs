use super::*;
use hp_app::settings::RoadPreset;

fn valid_name(name: &str) -> bool {
    !name.is_empty() && name.chars().count() <= 128 && !name.chars().any(char::is_control)
}

fn profile(mut meta: RoadMeta) -> RoadMeta {
    meta.uid = 0;
    meta.points = vec![RoadPoint::new(0.0, 0.0, 0.0), RoadPoint::new(64.0, 0.0, 0.0)];
    meta.start_dir = None;
    meta.end_dir = None;
    meta.start_slope = None;
    meta.end_slope = None;
    meta.u0 = 0.0;
    meta.junction = None;
    meta.clamped()
}

impl RoadTool {
    pub(super) fn match_user_preset(&mut self, state: &AppState) {
        let road = self.user_preset().road;
        self.preset_name = state
            .settings
            .brush
            .road_presets
            .iter()
            .find(|(_, preset)| preset.road == road)
            .map(|(name, _)| name.clone())
            .unwrap_or_default();
    }

    fn user_preset(&self) -> RoadPreset {
        RoadPreset {
            road: profile(self.options.meta.clone()).encode(),
            entity_class: match &self.options.create_as {
                CreateAs::World => None,
                CreateAs::Entity(class) => Some(class.clone()),
            },
            keep_active: self.options.keep_active,
        }
    }

    pub(super) fn save_user_preset(&mut self, state: &mut AppState, name: &str) -> bool {
        let name = name.trim();
        if !valid_name(name) {
            return false;
        }
        state.settings.brush.road_presets.insert(name.to_owned(), self.user_preset());
        state.settings_dirty = true;
        self.preset_name = name.to_owned();
        true
    }

    pub(super) fn load_user_preset(&mut self, state: &mut AppState, name: &str) -> bool {
        let name = name.trim();
        let Some(preset) = state.settings.brush.road_presets.get(name).cloned() else { return false };
        let Some(meta) = RoadMeta::decode(&preset.road) else { return false };
        self.history_group = false;
        self.record_history();
        let before = self.options.clone();
        let followed = self.follow_material;
        self.options.meta = RoadMeta { points: Vec::new(), ..profile(meta) };
        self.options.create_as = preset.entity_class.map_or(CreateAs::World, CreateAs::Entity);
        self.options.keep_active = preset.keep_active;
        self.follow_material = false;
        self.preset_name = name.to_owned();
        if self.options != before || followed {
            self.history_active = true;
            self.bump();
        }
        self.sync_preview(state);
        true
    }

    pub(super) fn delete_user_preset(&mut self, state: &mut AppState, name: &str) -> bool {
        let name = name.trim();
        if state.settings.brush.road_presets.remove(name).is_none() {
            return false;
        }
        state.settings_dirty = true;
        if self.preset_name == name {
            self.preset_name.clear();
        }
        true
    }

    pub(super) fn preset_combo(&mut self, ui: &mut Ui, state: &mut AppState, salt: &str) {
        let selected = state.settings.brush.road_presets.get(&self.preset_name);
        let label = match selected {
            Some(preset) if *preset != self.user_preset() => format!("{} *", self.preset_name),
            Some(_) => self.preset_name.clone(),
            None => t!("road.presets").to_owned(),
        };
        egui::ComboBox::from_id_salt(("road_user_presets", salt))
            .selected_text(label)
            .show_ui(ui, |ui| {
                let names: Vec<_> = state.settings.brush.road_presets.keys().cloned().collect();
                if names.is_empty() {
                    ui.label(t!("road.preset_empty"));
                }
                for name in names {
                    if ui.selectable_label(self.preset_name == name, &name).clicked()
                        && !self.load_user_preset(state, &name)
                    {
                        state.notify(Severity::Warning, t!("road.preset_invalid").to_owned());
                    }
                }
                ui.separator();
                if ui.button(t!("road.preset_manage")).clicked() {
                    self.show_window = true;
                }
            })
            .response
            .on_hover_text(t!("road.preset_tip"));
    }

    pub(super) fn presets_ui(&mut self, ui: &mut Ui, state: &mut AppState) {
        ui.horizontal_wrapped(|ui| {
            ui.label(t!("road.presets"));
            self.preset_combo(ui, state, "settings");
            ui.add(
                egui::TextEdit::singleline(&mut self.preset_name)
                    .char_limit(128)
                    .desired_width(180.0)
                    .hint_text(t!("road.preset_name")),
            );
            let name = self.preset_name.trim().to_owned();
            let exists = state.settings.brush.road_presets.contains_key(&name);
            let save = if exists { t!("road.preset_update") } else { t!("road.preset_save") };
            if ui.add_enabled(valid_name(&name), egui::Button::new(save)).clicked() {
                self.save_user_preset(state, &name);
            }
            if ui.add_enabled(exists, egui::Button::new(t!("road.preset_delete"))).clicked() {
                self.delete_user_preset(state, &name);
            }
        });
        ui.label(t!("road.preset_tip"));
    }
}
