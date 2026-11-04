use hp_app::i18n::TrTable;
use hp_app::{Action, Severity, t, tf};
use hp_editor::Id;
use hp_geom::roadside::{AlphaMode, BlendShape, GutterShape, HeightMode, ShoulderFalloff, Sides, Spine};
use hp_tools_create::curve::apply::Target;
use hp_tools_create::roadside::{self, Kind, RoadsideMeta, Source};
use hp_tools_mesh::disp::roadblend;

use super::Package;
use crate::app::HammerProApp;

pub(crate) struct Roadside;

#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
enum Tab {
    #[default]
    Sidewalk,
    Curb,
    Terrain,
}

pub(crate) struct State {
    open: bool,
    tab: Tab,
    sidewalk: RoadsideMeta,
    curb: RoadsideMeta,
    shape: BlendShape,
    sources: Vec<Source>,
    editing: Option<Id>,
    in_detail: bool,
    status: String,
    show_preview: bool,
    preview_key: String,
}

impl Default for State {
    fn default() -> Self {
        Self {
            open: false,
            tab: Tab::Sidewalk,
            sidewalk: RoadsideMeta { kind: Kind::Sidewalk, ..RoadsideMeta::default() },
            curb: RoadsideMeta { kind: Kind::Curb, ..RoadsideMeta::default() },
            shape: BlendShape::default(),
            sources: Vec::new(),
            editing: None,
            in_detail: false,
            status: String::new(),
            show_preview: true,
            preview_key: String::new(),
        }
    }
}

const HANDLED: &[Action] = &[Action::RoadSidewalk, Action::RoadCurb, Action::RoadTerrain, Action::RoadsideEdit];
const DETAIL_CLASS: &str = "func_detail";

impl Package for Roadside {
    fn handled(&self) -> &'static [Action] {
        HANDLED
    }

    fn handle(&self, app: &mut HammerProApp, _ctx: &egui::Context, action: Action) -> bool {
        match action {
            Action::RoadSidewalk => app.open_roadside(Tab::Sidewalk),
            Action::RoadCurb => app.open_roadside(Tab::Curb),
            Action::RoadTerrain => app.open_roadside(Tab::Terrain),
            Action::RoadsideEdit => app.edit_roadside_set(),
            _ => return false,
        }
        true
    }

    fn dialogs(&self, app: &mut HammerProApp, ctx: &egui::Context) {
        app.roadside_dialog(ctx);
    }

    fn translations(&self) -> TrTable {
        TR
    }
}

impl HammerProApp {
    fn fill_materials(&mut self) {
        let current = self.state.current_material.clone();
        let st = &mut self.pkg.roadside;
        for m in [&mut st.sidewalk, &mut st.curb] {
            if m.materials.top.is_empty() {
                m.materials.top.clone_from(&current);
            }
        }
    }

    fn take_selected_roads(&mut self) -> bool {
        let found = roadside::sources_of_selection(&self.state.doc);
        if found.is_empty() {
            return false;
        }
        self.pkg.roadside.sources = found;
        true
    }

    fn open_roadside(&mut self, tab: Tab) {
        self.fill_materials();
        let took = self.take_selected_roads();
        let st = &mut self.pkg.roadside;
        if !st.open || st.tab != tab {
            st.editing = None;
        }
        st.open = true;
        st.tab = tab;
        st.status.clear();
        if !took && st.sources.is_empty() {
            st.status = t!("roadside.need_roads").to_owned();
        }
    }

    fn edit_roadside_set(&mut self) {
        let Some(piece) = self.state.doc.selected_roadside_piece() else {
            self.state.notify(Severity::Info, t!("roadside.edit_need"));
            return;
        };
        let Some((_, meta)) = roadside::read(&self.state.doc, piece) else {
            self.state.notify(Severity::Warning, t!("roadside.edit_unreadable"));
            return;
        };
        let st = &mut self.pkg.roadside;
        st.sources.clone_from(&meta.sources);
        st.tab = if meta.kind == Kind::Sidewalk { Tab::Sidewalk } else { Tab::Curb };
        if meta.kind == Kind::Sidewalk {
            st.sidewalk = meta;
        } else {
            st.curb = meta;
        }
        st.editing = Some(piece);
        st.open = true;
        st.status.clear();
    }

    fn lightmap_default(&self) -> i32 {
        match self.state.settings.edit.default_lightmap_scale {
            s if s > 0 => s,
            _ => 16,
        }
    }

    fn roadside_generate(&mut self) {
        self.fill_materials();
        let lightmap = self.lightmap_default();
        let st = &mut self.pkg.roadside;
        let mut meta = if st.tab == Tab::Curb { st.curb.clone() } else { st.sidewalk.clone() };
        meta.sources.clone_from(&st.sources);
        if meta.sources.is_empty() {
            st.status = t!("roadside.need_roads").to_owned();
            self.state.notify(Severity::Info, t!("roadside.need_roads"));
            return;
        }
        let scale = self.state.settings.edit.default_texture_scale;
        if !(scale.is_finite() && scale.abs() > 1e-6) {
            meta.tex_scale = hp_geom::DEFAULT_TEXTURE_SCALE;
        }
        let doc = &mut self.state.doc;
        let existing = st.editing.and_then(|piece| roadside::read(doc, piece));
        let result = match existing {
            Some((record, _)) => {
                let keep = roadside::lightmap_of(doc, &record);
                roadside::replace(doc, &record, &meta, keep)
            }
            None => {
                let target = if st.in_detail {
                    Target::NewEntity { class: DETAIL_CLASS, keyvalues: &[] }
                } else {
                    Target::World
                };
                roadside::create(doc, &meta, lightmap, target)
            }
        };
        match result {
            Ok(made) => {
                st.editing = made.ids.iter().copied().min();
                st.status = tf!("roadside.done", made.ids.len());
                self.state.log(Severity::Info, st.status.clone());
            }
            Err(e) => {
                st.status = tf!("roadside.failed", e.to_string());
                self.state.notify(Severity::Warning, st.status.clone());
            }
        }
    }

    fn roadside_spines(&mut self) -> Option<Vec<Spine>> {
        let st = &mut self.pkg.roadside;
        if st.sources.is_empty() {
            st.status = t!("roadside.need_roads").to_owned();
            return None;
        }
        match roadside::resolve_all(&self.state.doc, &st.sources) {
            Ok(s) => Some(s),
            Err(e) => {
                st.status = tf!("roadside.failed", e.to_string());
                None
            }
        }
    }

    fn roadside_blend(&mut self) {
        let Some(spines) = self.roadside_spines() else { return };
        let shape = self.pkg.roadside.shape;
        match roadblend::blend_terrain(&mut self.state.doc, &spines, &shape) {
            Ok(r) => {
                let text = tf!("roadside.blended", r.faces_changed, r.vertices_moved, r.vertices_blended);
                self.pkg.roadside.status.clone_from(&text);
                self.state.notify(Severity::Success, text);
            }
            Err(e) => {
                let text = tf!("roadside.failed", super::edit_error_text(&e));
                self.pkg.roadside.status.clone_from(&text);
                self.state.notify(Severity::Warning, text);
            }
        }
        self.pkg.roadside.preview_key.clear();
    }

    fn roadside_sync_preview(&mut self) {
        let wanted = self.pkg.roadside.open && self.pkg.roadside.tab == Tab::Terrain && self.pkg.roadside.show_preview;
        if !wanted {
            self.pkg.roadside.preview_key.clear();
            self.state.runtime.roadside.clear_preview();
            return;
        }
        let key = format!(
            "{:?}|{}|{}",
            self.pkg.roadside.shape,
            self.state.doc.revision(),
            self.pkg.roadside.sources.iter().map(Source::encode).collect::<Vec<_>>().join(";")
        );
        if key == self.pkg.roadside.preview_key {
            return;
        }
        self.pkg.roadside.preview_key = key;
        let Some(spines) = self.roadside_spines() else {
            self.state.runtime.roadside.clear_preview();
            return;
        };
        let shape = self.pkg.roadside.shape;
        match roadblend::preview(&self.state.doc, &spines, &shape) {
            Ok(p) => self.state.runtime.roadside.set_preview(p),
            Err(_) => self.state.runtime.roadside.clear_preview(),
        }
    }

    pub(crate) fn roadside_dialog(&mut self, ctx: &egui::Context) {
        if !self.pkg.roadside.open {
            if !self.state.runtime.roadside.preview.is_empty() {
                self.pkg.roadside.preview_key.clear();
                self.state.runtime.roadside.clear_preview();
            }
            return;
        }
        let mut open = true;
        let mut act = UiAction::None;
        let current = self.state.current_material.clone();
        let selected_has_roads = !roadside::sources_of_selection(&self.state.doc).is_empty();
        let st = &mut self.pkg.roadside;
        hp_app::chrome::Window::new(t!("roadside.title"))
            .id(egui::Id::unique("hp_roadside_window"))
            .open(&mut open)
            .collapsible(false)
            .resizable(false)
            .anchor(egui::Align2::RIGHT_TOP, [-12.0, 60.0])
            .show(ctx, |ui| {
                ui.set_max_width(420.0);
                tabs_ui(ui, st);
                ui.separator();
                roads_ui(ui, st, selected_has_roads, &mut act);
                ui.add_space(4.0);
                match st.tab {
                    Tab::Sidewalk => sidewalk_ui(ui, &mut st.sidewalk, &current),
                    Tab::Curb => curb_ui(ui, &mut st.curb, &current),
                    Tab::Terrain => terrain_ui(ui, &mut st.shape),
                }
                ui.add_space(6.0);
                buttons_ui(ui, st, &mut act);
                if !st.status.is_empty() {
                    ui.add_space(4.0);
                    ui.label(egui::RichText::new(&st.status).weak());
                }
            });
        if !open {
            self.pkg.roadside.open = false;
        }
        match act {
            UiAction::None => {}
            UiAction::UseSelection => {
                if self.take_selected_roads() {
                    self.pkg.roadside.status.clear();
                } else {
                    self.pkg.roadside.status = t!("roadside.need_roads").to_owned();
                }
            }
            UiAction::Generate => self.roadside_generate(),
            UiAction::NewSet => self.pkg.roadside.editing = None,
            UiAction::Blend => self.roadside_blend(),
            UiAction::Close => self.pkg.roadside.open = false,
        }
        self.roadside_sync_preview();
    }
}

#[derive(Clone, Copy, PartialEq, Eq)]
enum UiAction {
    None,
    UseSelection,
    Generate,
    NewSet,
    Blend,
    Close,
}

fn tabs_ui(ui: &mut egui::Ui, st: &mut State) {
    ui.horizontal(|ui| {
        for (tab, key) in [
            (Tab::Sidewalk, "roadside.tab_sidewalk"),
            (Tab::Curb, "roadside.tab_curb"),
            (Tab::Terrain, "roadside.tab_terrain"),
        ] {
            if ui.add(egui::Button::selectable(st.tab == tab, t!(key))).clicked() && st.tab != tab {
                st.tab = tab;
                st.editing = None;
                st.status.clear();
            }
        }
    });
}

fn roads_ui(ui: &mut egui::Ui, st: &mut State, selected_has_roads: bool, act: &mut UiAction) {
    ui.horizontal(|ui| {
        ui.label(tf!("roadside.roads", st.sources.len())).on_hover_text(t!("roadside.roads_tip"));
        if ui
            .add_enabled(selected_has_roads, egui::Button::new(t!("roadside.use_selected")))
            .on_hover_text(t!("roadside.use_selected_tip"))
            .clicked()
        {
            *act = UiAction::UseSelection;
        }
    });
    for s in &st.sources {
        ui.label(egui::RichText::new(s.label()).weak().small());
    }
}

fn num(ui: &mut egui::Ui, key: &str, value: &mut f64, range: std::ops::RangeInclusive<f64>, speed: f64) {
    ui.label(t!(key));
    ui.add(egui::DragValue::new(value).speed(speed).range(range).max_decimals(2)).on_hover_text(tip(key));
    ui.end_row();
}

fn tip(key: &str) -> String {
    t!(format!("{key}_tip").as_str()).to_owned()
}

fn sides_ui(ui: &mut egui::Ui, sides: &mut Sides) {
    ui.horizontal(|ui| {
        ui.label(t!("roadside.sides"));
        for (value, key) in
            [(Sides::Left, "roadside.left"), (Sides::Right, "roadside.right"), (Sides::Both, "roadside.both")]
        {
            if ui
                .add(egui::Button::selectable(*sides == value, t!(key)))
                .on_hover_text(t!("roadside.sides_tip"))
                .clicked()
            {
                *sides = value;
            }
        }
    });
}

fn material_row(ui: &mut egui::Ui, key: &str, value: &mut String, current: &str) {
    ui.label(t!(key));
    ui.horizontal(|ui| {
        ui.add(egui::TextEdit::singleline(value).desired_width(170.0)).on_hover_text(tip(key));
        if ui.small_button(t!("roadside.current")).on_hover_text(t!("roadside.current_tip")).clicked() {
            *value = current.to_owned();
        }
    });
    ui.end_row();
}

fn common_ui(ui: &mut egui::Ui, m: &mut RoadsideMeta) {
    sides_ui(ui, &mut m.sides);
    egui::Grid::new("hp_roadside_common").num_columns(2).spacing([8.0, 4.0]).show(ui, |ui| {
        num(ui, "roadside.gap", &mut m.gap, 0.0..=512.0, 0.5);
        num(ui, "roadside.radius", &mut m.radius, 0.0..=512.0, 1.0);
    });
}

fn sidewalk_ui(ui: &mut egui::Ui, m: &mut RoadsideMeta, current: &str) {
    common_ui(ui, m);
    egui::Grid::new("hp_roadside_sidewalk").num_columns(2).spacing([8.0, 4.0]).show(ui, |ui| {
        num(ui, "roadside.width", &mut m.sidewalk.width, 1.0..=512.0, 1.0);
        num(ui, "roadside.height", &mut m.sidewalk.height, 0.0..=256.0, 0.25);
        num(ui, "roadside.depth", &mut m.sidewalk.depth, 1.0..=256.0, 0.5);
        num(ui, "roadside.bevel", &mut m.sidewalk.bevel, 0.0..=256.0, 0.25);
        num(ui, "roadside.slope", &mut m.sidewalk.slope, -512.0..=512.0, 0.25);
        material_row(ui, "roadside.material", &mut m.materials.top, current);
        material_row(ui, "roadside.face_material", &mut m.materials.face, current);
    });
}

fn curb_ui(ui: &mut egui::Ui, m: &mut RoadsideMeta, current: &str) {
    common_ui(ui, m);
    ui.checkbox(&mut m.use_gutter, t!("roadside.gutter")).on_hover_text(t!("roadside.gutter_tip"));
    if m.use_gutter {
        egui::Grid::new("hp_roadside_gutter").num_columns(2).spacing([8.0, 4.0]).show(ui, |ui| {
            num(ui, "roadside.gutter_width", &mut m.gutter.width, 2.0..=512.0, 0.5);
            num(ui, "roadside.gutter_depth", &mut m.gutter.depth, 0.5..=256.0, 0.25);
            ui.label(t!("roadside.gutter_shape"));
            ui.horizontal(|ui| {
                for (shape, key) in [(GutterShape::V, "roadside.shape_v"), (GutterShape::U, "roadside.shape_u")] {
                    if ui
                        .add(egui::Button::selectable(m.gutter.shape == shape, t!(key)))
                        .on_hover_text(t!("roadside.gutter_shape_tip"))
                        .clicked()
                    {
                        m.gutter.shape = shape;
                    }
                }
            });
            ui.end_row();
            if m.gutter.shape == GutterShape::U {
                num(ui, "roadside.gutter_floor", &mut m.gutter.floor_width, 0.0..=512.0, 0.5);
            }
        });
        ui.checkbox(&mut m.gutter.slot, t!("roadside.slot")).on_hover_text(t!("roadside.slot_tip"));
        if m.gutter.slot {
            egui::Grid::new("hp_roadside_slot").num_columns(2).spacing([8.0, 4.0]).show(ui, |ui| {
                num(ui, "roadside.slot_width", &mut m.gutter.slot_width, 0.5..=512.0, 0.25);
                num(ui, "roadside.slot_depth", &mut m.gutter.slot_depth, 1.0..=256.0, 0.5);
                ui.label(t!("roadside.slot_every"));
                ui.add(egui::DragValue::new(&mut m.gutter.slot_every).range(1..=1024))
                    .on_hover_text(t!("roadside.slot_every_tip"));
                ui.end_row();
            });
        }
    }
    ui.checkbox(&mut m.use_curb, t!("roadside.curb")).on_hover_text(t!("roadside.curb_tip"));
    if m.use_curb {
        egui::Grid::new("hp_roadside_curb").num_columns(2).spacing([8.0, 4.0]).show(ui, |ui| {
            num(ui, "roadside.curb_width", &mut m.curb.width, 1.0..=512.0, 0.5);
            num(ui, "roadside.curb_height", &mut m.curb.height, 1.0..=256.0, 0.25);
            num(ui, "roadside.curb_depth", &mut m.curb.depth, 1.0..=256.0, 0.5);
            num(ui, "roadside.curb_bevel", &mut m.curb.bevel, 0.0..=256.0, 0.25);
        });
        ui.checkbox(&mut m.curb.rounded, t!("roadside.curb_rounded")).on_hover_text(t!("roadside.curb_rounded_tip"));
        ui.checkbox(&mut m.curb.both_edges, t!("roadside.curb_both")).on_hover_text(t!("roadside.curb_both_tip"));
    }
    egui::Grid::new("hp_roadside_curb_materials").num_columns(2).spacing([8.0, 4.0]).show(ui, |ui| {
        material_row(ui, "roadside.curb_material", &mut m.materials.top, current);
        material_row(ui, "roadside.gutter_material", &mut m.materials.gutter, current);
    });
}

fn combo<T: Copy + PartialEq>(ui: &mut egui::Ui, id: &str, value: &mut T, items: &[(T, &str)], tip_key: &str) {
    let shown = items.iter().find(|(v, _)| v == value).map_or("", |(_, k)| t!(*k));
    egui::ComboBox::from_id_salt(id)
        .selected_text(shown)
        .show_ui(ui, |ui| {
            for (v, key) in items {
                ui.selectable_value(value, *v, t!(*key));
            }
        })
        .response
        .on_hover_text(t!(tip_key));
}

fn terrain_ui(ui: &mut egui::Ui, s: &mut BlendShape) {
    egui::Grid::new("hp_roadside_terrain").num_columns(2).spacing([8.0, 4.0]).show(ui, |ui| {
        num(ui, "roadside.shoulder", &mut s.shoulder, 0.0..=4096.0, 2.0);
        num(ui, "roadside.margin", &mut s.margin, 0.0..=1024.0, 1.0);
        ui.label(t!("roadside.falloff"));
        combo(
            ui,
            "hp_roadside_falloff",
            &mut s.falloff,
            &[
                (ShoulderFalloff::Smooth, "roadside.falloff_smooth"),
                (ShoulderFalloff::Linear, "roadside.falloff_linear"),
                (ShoulderFalloff::EaseIn, "roadside.falloff_ease_in"),
                (ShoulderFalloff::EaseOut, "roadside.falloff_ease_out"),
            ],
            "roadside.falloff_tip",
        );
        ui.end_row();
        num(ui, "roadside.strength", &mut s.strength, 0.0..=1.0, 0.01);
        num(ui, "roadside.sink", &mut s.sink, -256.0..=256.0, 0.25);
        ui.label(t!("roadside.mode"));
        combo(
            ui,
            "hp_roadside_mode",
            &mut s.mode,
            &[
                (HeightMode::Both, "roadside.mode_both"),
                (HeightMode::SinkOnly, "roadside.mode_sink"),
                (HeightMode::RaiseOnly, "roadside.mode_raise"),
            ],
            "roadside.mode_tip",
        );
        ui.end_row();
        ui.label(t!("roadside.texture"));
        combo(
            ui,
            "hp_roadside_texture",
            &mut s.alpha,
            &[
                (AlphaMode::Off, "roadside.texture_off"),
                (AlphaMode::TwoWay, "roadside.texture_two"),
                (AlphaMode::FourWay, "roadside.texture_four"),
            ],
            "roadside.texture_tip",
        );
        ui.end_row();
    });
    if s.alpha != AlphaMode::Off {
        egui::Grid::new("hp_roadside_alpha").num_columns(2).spacing([8.0, 4.0]).show(ui, |ui| {
            num(ui, "roadside.texture_width", &mut s.alpha_shoulder, 0.0..=4096.0, 2.0);
            num(ui, "roadside.texture_strength", &mut s.alpha_strength, 0.0..=1.0, 0.01);
            ui.label(t!("roadside.channel"));
            let top = if s.alpha == AlphaMode::TwoWay { 1 } else { 3 };
            s.road_channel = s.road_channel.min(top);
            ui.add(egui::DragValue::new(&mut s.road_channel).range(0..=top)).on_hover_text(t!("roadside.channel_tip"));
            ui.end_row();
        });
    }
}

fn buttons_ui(ui: &mut egui::Ui, st: &mut State, act: &mut UiAction) {
    if st.tab == Tab::Terrain {
        ui.checkbox(&mut st.show_preview, t!("roadside.preview")).on_hover_text(t!("roadside.preview_tip"));
        ui.horizontal(|ui| {
            if ui
                .add_enabled(!st.sources.is_empty(), egui::Button::new(t!("roadside.blend")))
                .on_hover_text(t!("roadside.blend_tip"))
                .clicked()
            {
                *act = UiAction::Blend;
            }
            if ui.button(t!("common.close")).clicked() {
                *act = UiAction::Close;
            }
        });
        return;
    }
    ui.checkbox(&mut st.in_detail, t!("roadside.as_detail")).on_hover_text(t!("roadside.as_detail_tip"));
    ui.horizontal(|ui| {
        let label = if st.editing.is_some() { t!("roadside.apply") } else { t!("roadside.create") };
        if ui
            .add_enabled(!st.sources.is_empty(), egui::Button::new(label))
            .on_hover_text(t!("roadside.create_tip"))
            .clicked()
        {
            *act = UiAction::Generate;
        }
        if st.editing.is_some() && ui.button(t!("roadside.new_set")).on_hover_text(t!("roadside.new_set_tip")).clicked()
        {
            *act = UiAction::NewSet;
        }
        if ui.button(t!("common.close")).clicked() {
            *act = UiAction::Close;
        }
    });
}

pub(crate) const TR: TrTable = &[
    ("roadside.title", "Road details", "Détails de route"),
    ("roadside.tab_sidewalk", "Sidewalk", "Trottoir"),
    ("roadside.tab_curb", "Curb and gutter", "Bordure et caniveau"),
    ("roadside.tab_terrain", "Terrain", "Terrain"),
    ("roadside.roads", "Roads: {0}", "Routes : {0}"),
    (
        "roadside.roads_tip",
        "The roads the details follow: road tool roads and curves",
        "Les routes suivies : routes de l'outil route et courbes",
    ),
    ("roadside.use_selected", "Use the selected roads", "Utiliser les routes sélectionnées"),
    (
        "roadside.use_selected_tip",
        "Take the selected road or curve brushes as the roads to follow",
        "Prendre les brushes de route ou de courbe sélectionnés comme routes à suivre",
    ),
    (
        "roadside.need_roads",
        "Select roads or curves, then press Use the selected roads",
        "Sélectionnez des routes ou des courbes, puis appuyez sur Utiliser les routes sélectionnées",
    ),
    (
        "roadside.edit_need",
        "Select a brush made by the sidewalk or curb generator to edit its set",
        "Sélectionnez un brush créé par le générateur de trottoirs ou de bordures pour modifier son ensemble",
    ),
    (
        "roadside.edit_unreadable",
        "The road details data of this brush cannot be read",
        "Les données de détails de route de ce brush sont illisibles",
    ),
    ("roadside.sides", "Sides", "Côtés"),
    ("roadside.left", "Left", "Gauche"),
    ("roadside.right", "Right", "Droite"),
    ("roadside.both", "Both", "Les deux"),
    (
        "roadside.sides_tip",
        "Which side of the road, following its direction from the first to the last point",
        "Quel côté de la route, dans le sens du premier au dernier point",
    ),
    ("roadside.gap", "Gap to the road", "Écart avec la route"),
    (
        "roadside.gap_tip",
        "Distance between the road edge and the first brush",
        "Distance entre le bord de la route et le premier brush",
    ),
    ("roadside.radius", "Corner radius", "Rayon des angles"),
    (
        "roadside.radius_tip",
        "Radius of the rounded corners where a road ends on another one, measured at the road edge",
        "Rayon des angles arrondis là où une route débouche sur une autre, mesuré au bord de la route",
    ),
    ("roadside.width", "Width", "Largeur"),
    ("roadside.width_tip", "Width of the sidewalk", "Largeur du trottoir"),
    ("roadside.height", "Height", "Hauteur"),
    (
        "roadside.height_tip",
        "Height of the sidewalk above the road edge",
        "Hauteur du trottoir au-dessus du bord de la route",
    ),
    ("roadside.depth", "Depth under the road", "Profondeur sous la route"),
    (
        "roadside.depth_tip",
        "How far the brush goes under the road edge",
        "Jusqu'où le brush descend sous le bord de la route",
    ),
    ("roadside.bevel", "Curb face bevel", "Chanfrein de la face"),
    (
        "roadside.bevel_tip",
        "Chamfer of the top edge on the road side, the curb face (0 for none)",
        "Chanfrein de l'arête haute côté route, la face de la bordure (0 pour aucun)",
    ),
    ("roadside.slope", "Cross slope", "Pente transversale"),
    (
        "roadside.slope_tip",
        "Rise of the top from the road side to the far side, negative drains toward the road",
        "Montée du dessus entre le côté route et le côté opposé, négatif pour évacuer vers la route",
    ),
    ("roadside.material", "Top material", "Matériau du dessus"),
    ("roadside.material_tip", "Material of the sidewalk", "Matériau du trottoir"),
    ("roadside.face_material", "Face material", "Matériau de la face"),
    (
        "roadside.face_material_tip",
        "Material of the faces turned toward the road, the curb face (empty for the top material)",
        "Matériau des faces tournées vers la route, la face de la bordure (vide pour celui du dessus)",
    ),
    ("roadside.current", "Current", "Actuel"),
    ("roadside.current_tip", "Use the current material", "Utiliser le matériau actuel"),
    ("roadside.gutter", "Gutter channel", "Caniveau"),
    (
        "roadside.gutter_tip",
        "Build a V or U channel beside the road, before the curb",
        "Créer un caniveau en V ou en U le long de la route, avant la bordure",
    ),
    ("roadside.gutter_width", "Gutter width", "Largeur du caniveau"),
    ("roadside.gutter_width_tip", "Width of the channel", "Largeur du caniveau"),
    ("roadside.gutter_depth", "Gutter depth", "Profondeur du caniveau"),
    (
        "roadside.gutter_depth_tip",
        "How far the channel sinks under the road edge",
        "De combien le caniveau s'enfonce sous le bord de la route",
    ),
    ("roadside.gutter_shape", "Shape", "Forme"),
    ("roadside.shape_v", "V", "V"),
    ("roadside.shape_u", "U", "U"),
    (
        "roadside.gutter_shape_tip",
        "V section or U section with a flat bottom",
        "Section en V ou en U avec un fond plat",
    ),
    ("roadside.gutter_floor", "Flat bottom", "Fond plat"),
    ("roadside.gutter_floor_tip", "Width of the flat bottom of a U channel", "Largeur du fond plat d'un caniveau en U"),
    ("roadside.slot", "Drain slot", "Fente d'évacuation"),
    (
        "roadside.slot_tip",
        "Cut a drain slot into the channel floor every few pieces",
        "Percer une fente d'évacuation dans le fond du caniveau toutes les quelques pièces",
    ),
    ("roadside.slot_width", "Slot width", "Largeur de la fente"),
    ("roadside.slot_width_tip", "Width of the slot across the channel", "Largeur de la fente en travers du caniveau"),
    ("roadside.slot_depth", "Slot depth", "Profondeur de la fente"),
    (
        "roadside.slot_depth_tip",
        "How far the slot goes under the channel floor",
        "Jusqu'où la fente descend sous le fond du caniveau",
    ),
    ("roadside.slot_every", "One slot every", "Une fente toutes les"),
    (
        "roadside.slot_every_tip",
        "Number of pieces along the road between two slots",
        "Nombre de pièces le long de la route entre deux fentes",
    ),
    ("roadside.curb", "Curb", "Bordure"),
    ("roadside.curb_tip", "Build the curb brushes", "Créer les brushes de bordure"),
    ("roadside.curb_width", "Curb width", "Largeur de la bordure"),
    ("roadside.curb_width_tip", "Width of the curb", "Largeur de la bordure"),
    ("roadside.curb_height", "Curb height", "Hauteur de la bordure"),
    (
        "roadside.curb_height_tip",
        "Height of the curb above the road edge",
        "Hauteur de la bordure au-dessus du bord de la route",
    ),
    ("roadside.curb_depth", "Curb depth", "Profondeur de la bordure"),
    (
        "roadside.curb_depth_tip",
        "How far the curb goes under the road edge",
        "Jusqu'où la bordure descend sous le bord de la route",
    ),
    ("roadside.curb_bevel", "Bevel", "Chanfrein"),
    (
        "roadside.curb_bevel_tip",
        "Size of the chamfer or of the rounding of the top edge (0 for a sharp edge)",
        "Taille du chanfrein ou de l'arrondi de l'arête haute (0 pour une arête vive)",
    ),
    ("roadside.curb_rounded", "Rounded top", "Dessus arrondi"),
    (
        "roadside.curb_rounded_tip",
        "Round the top edge with a quarter circle instead of a chamfer",
        "Arrondir l'arête haute avec un quart de cercle au lieu d'un chanfrein",
    ),
    ("roadside.curb_both", "Both top edges", "Les deux arêtes hautes"),
    (
        "roadside.curb_both_tip",
        "Also bevel or round the top edge on the far side",
        "Chanfreiner ou arrondir aussi l'arête haute du côté opposé",
    ),
    ("roadside.curb_material", "Curb material", "Matériau de la bordure"),
    ("roadside.curb_material_tip", "Material of the curb", "Matériau de la bordure"),
    ("roadside.gutter_material", "Gutter material", "Matériau du caniveau"),
    (
        "roadside.gutter_material_tip",
        "Material of the gutter channel (empty for the curb material)",
        "Matériau du caniveau (vide pour celui de la bordure)",
    ),
    ("roadside.as_detail", "Create as func_detail", "Créer en func_detail"),
    (
        "roadside.as_detail_tip",
        "Put all the brushes in one func_detail entity instead of the world",
        "Mettre tous les brushes dans une seule entité func_detail au lieu du monde",
    ),
    ("roadside.create", "Create", "Créer"),
    ("roadside.apply", "Apply changes", "Appliquer les changements"),
    (
        "roadside.create_tip",
        "Build the brushes beside the roads, one undo step. After creating, the next presses replace the same set",
        "Créer les brushes le long des routes, en une étape d'annulation. Après la création, les appuis suivants remplacent le même ensemble",
    ),
    ("roadside.new_set", "New set", "Nouvel ensemble"),
    (
        "roadside.new_set_tip",
        "Stop editing this set: the next press creates another one",
        "Arrêter de modifier cet ensemble : le prochain appui en crée un autre",
    ),
    ("roadside.done", "{0} brush(es) built beside the roads", "{0} brush(es) créés le long des routes"),
    ("roadside.failed", "Road details failed: {0}", "Détails de route impossibles : {0}"),
    ("roadside.shoulder", "Shoulder width", "Largeur de l'accotement"),
    (
        "roadside.shoulder_tip",
        "Width of the fade from the road height back to the terrain",
        "Largeur du fondu entre la hauteur de la route et le terrain",
    ),
    ("roadside.margin", "Flat margin", "Marge plate"),
    (
        "roadside.margin_tip",
        "Extra width beside the road kept fully at the road height",
        "Largeur supplémentaire au bord de la route gardée entièrement à la hauteur de la route",
    ),
    ("roadside.falloff", "Falloff", "Atténuation"),
    ("roadside.falloff_smooth", "Smooth", "Douce"),
    ("roadside.falloff_linear", "Linear", "Linéaire"),
    ("roadside.falloff_ease_in", "Steep near the road", "Raide près de la route"),
    ("roadside.falloff_ease_out", "Steep far from the road", "Raide loin de la route"),
    ("roadside.falloff_tip", "Shape of the fade across the shoulder", "Forme du fondu sur l'accotement"),
    ("roadside.strength", "Strength", "Force"),
    (
        "roadside.strength_tip",
        "How far the terrain goes toward the road (1 is fully)",
        "Jusqu'où le terrain va vers la route (1 pour complètement)",
    ),
    ("roadside.sink", "Sink under the road", "Enfoncement sous la route"),
    (
        "roadside.sink_tip",
        "Put the terrain this many units under the road surface so they do not overlap",
        "Mettre le terrain à ces unités sous la surface de la route pour éviter qu'ils se chevauchent",
    ),
    ("roadside.mode", "Move", "Déplacement"),
    ("roadside.mode_both", "Up and down", "Haut et bas"),
    ("roadside.mode_sink", "Only sink", "Creuser seulement"),
    ("roadside.mode_raise", "Only raise", "Relever seulement"),
    (
        "roadside.mode_tip",
        "Which way the vertices may move to reach the road",
        "Dans quel sens les sommets peuvent bouger pour atteindre la route",
    ),
    ("roadside.texture", "Texture blend", "Fondu de texture"),
    ("roadside.texture_off", "Off", "Aucun"),
    ("roadside.texture_two", "2 way (alpha)", "2 textures (alpha)"),
    ("roadside.texture_four", "4 way (multiblend)", "4 textures (multiblend)"),
    (
        "roadside.texture_tip",
        "Blend the displacement texture toward the road texture, fading to the terrain texture across the shoulder",
        "Fondre la texture de la displacement vers celle de la route, en revenant à celle du terrain sur l'accotement",
    ),
    ("roadside.texture_width", "Texture fade width", "Largeur du fondu de texture"),
    (
        "roadside.texture_width_tip",
        "Width of the texture fade past the road edge",
        "Largeur du fondu de texture après le bord de la route",
    ),
    ("roadside.texture_strength", "Texture strength", "Force de la texture"),
    (
        "roadside.texture_strength_tip",
        "How far the texture goes toward the road texture",
        "Jusqu'où la texture va vers celle de la route",
    ),
    ("roadside.channel", "Road texture", "Texture de la route"),
    (
        "roadside.channel_tip",
        "Which texture of the displacement is the road: 0 to 3 for 4 way, 0 or 1 for 2 way",
        "Quelle texture de la displacement est la route : 0 à 3 pour 4 textures, 0 ou 1 pour 2 textures",
    ),
    ("roadside.preview", "Show the affected area", "Afficher la zone touchée"),
    (
        "roadside.preview_tip",
        "Show the new heights in the 3D view (orange the height, blue the texture only), with the road edge in white and the shoulder limit in yellow",
        "Afficher les nouvelles hauteurs dans la vue 3D (orange la hauteur, bleu la texture seule), avec le bord de la route en blanc et la limite de l'accotement en jaune",
    ),
    ("roadside.blend", "Blend terrain", "Fondre le terrain"),
    (
        "roadside.blend_tip",
        "Move every displacement within reach of the roads to follow them, one undo step",
        "Faire suivre les routes à toutes les displacements à portée, en une étape d'annulation",
    ),
    (
        "roadside.blended",
        "{0} displacement(s) changed, {1} vertices moved, {2} textures blended",
        "{0} displacement(s) modifiée(s), {1} sommets déplacés, {2} textures fondues",
    ),
];
