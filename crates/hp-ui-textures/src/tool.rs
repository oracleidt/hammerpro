use std::sync::Arc;

use egui::RichText;
use hp_app::actions::{Action, shortcut_label};
use hp_app::state::RightTab;
use hp_app::{AppState, Severity, ToolFlow, ToolId, ViewInfo, ViewInput, t, tf, theme};
use hp_editor::{FaceEdit, FaceRef, ObjRef, PickOptions, SelectOp};
use hp_geom::{Justify, Ray, TextureAlign};
use hp_render::Overlay;

use crate::faces::{self, FaceSummary, Multi};
use crate::mask::{MaskCache, MaskStyle};
use crate::util::{material_sizes, normalize_material, size_lookup};

const SELECT_STYLE: MaskStyle = MaskStyle { fill: [1.0, 0.22, 0.18, 0.28], line: [1.0, 0.35, 0.3, 1.0] };
const HOVER_LINE: [f32; 4] = [1.0, 0.85, 0.25, 1.0];

pub fn pick_face(state: &AppState, view: &ViewInfo, pos: egui::Pos2) -> Option<FaceRef> {
    if view.is_2d() {
        return None;
    }
    let (origin, dir) = view.ray(pos);
    let ray = Ray::new(origin, dir);
    let opts = PickOptions { include_entities: false, ..PickOptions::default() };
    let hit = state.view_pick(&ray, &opts)?;
    match (hit.obj, hit.face) {
        (ObjRef::Solid(id), Some(side)) => Some(FaceRef::new(id, side)),
        _ => None,
    }
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum ClickKind {
    Select,
    Toggle,
    SelectSolid { add: bool },
    Eyedropper,
    Apply,
    Wrap,
}

pub fn click_kind(secondary: bool, m: egui::Modifiers) -> ClickKind {
    let ctrl = m.command || m.ctrl;
    match (secondary, m.alt, m.shift, ctrl) {
        (true, true, _, _) => ClickKind::Wrap,
        (true, false, _, _) => ClickKind::Apply,
        (false, true, _, _) => ClickKind::Eyedropper,
        (false, false, true, add) => ClickKind::SelectSolid { add },
        (false, false, false, true) => ClickKind::Toggle,
        (false, false, false, false) => ClickKind::Select,
    }
}

pub fn click_face(state: &mut AppState, face: FaceRef, kind: ClickKind) {
    match kind {
        ClickKind::Select => state.doc.select_faces(&[face], SelectOp::Replace),
        ClickKind::Toggle => state.doc.select_faces(&[face], SelectOp::Toggle),
        ClickKind::SelectSolid { add } => {
            let all = state.doc.all_faces_of(&[ObjRef::Solid(face.solid)]);
            state.doc.select_faces(&all, if add { SelectOp::Add } else { SelectOp::Replace });
        }
        ClickKind::Eyedropper => {
            if let Some(side) = state.doc.face_side(face) {
                let name = normalize_material(&side.material);
                state.use_material(&name);
            }
        }
        ClickKind::Apply => {
            let material = state.current_material.clone();
            if let Err(e) = state.doc.apply_material(&[face], &material) {
                state.notify(Severity::Error, tf!("tex.tool.apply_failed", e));
            }
        }
        ClickKind::Wrap => {
            let src = state.doc.selection().faces.iter().copied().find(|f| *f != face);
            match src {
                Some(src) => {
                    if let Err(e) = state.doc.wrap_texture(src, &[face]) {
                        state.notify(Severity::Error, tf!("tex.tool.wrap_failed", e));
                    }
                }
                None => state.notify(Severity::Warning, t!("tex.tool.wrap_needs_source")),
            }
        }
    }
}

#[derive(Default)]
struct SummaryCache {
    revision: u64,
    valid: bool,
    faces: Arc<[FaceRef]>,
    summary: Arc<FaceSummary>,
}

pub struct TextureTool {
    hover: Option<FaceRef>,
    hover_key: Option<(egui::Pos2, u64, usize)>,
    hover_view: Option<usize>,
    hover_frame: u64,
    hide_mask: bool,
    treat_as_one: bool,
    fit_u: f64,
    fit_v: f64,
    cache: SummaryCache,
    mask: MaskCache,
    drag_serial: u64,
    saved_objects: Vec<ObjRef>,
    uv: crate::uv_editor::UvEditor,
    painting: bool,
    uv_place: UvPlace,
    uv_native: hp_app::native_window::NativeWindow,
    uv_builder: Option<egui::ViewportBuilder>,
}

pub const UV_WINDOW: &str = "uv_editor";

#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
enum UvPlace {
    #[default]
    Panel,
    Window,
}

impl Default for TextureTool {
    fn default() -> Self {
        Self {
            hover: None,
            hover_key: None,
            hover_view: None,
            hover_frame: 0,
            hide_mask: false,
            treat_as_one: false,
            fit_u: 1.0,
            fit_v: 1.0,
            cache: SummaryCache::default(),
            mask: MaskCache::default(),
            drag_serial: 0,
            saved_objects: Vec::new(),
            uv: crate::uv_editor::UvEditor::default(),
            painting: false,
            uv_place: UvPlace::Panel,
            uv_native: hp_app::native_window::NativeWindow::default(),
            uv_builder: None,
        }
    }
}

impl TextureTool {
    pub fn new() -> Self {
        Self::default()
    }

    pub fn hovered_face(&self) -> Option<FaceRef> {
        self.hover
    }

    fn shows_highlight(&self, state: &AppState) -> bool {
        !self.hide_mask && !state.doc.selection().faces.is_empty()
    }

    fn highlight_version(&self, state: &AppState, kind: hp_app::ViewKind) -> u64 {
        const TAG: u64 = 0x7E70_0000_0000_0000;
        let generation = if self.shows_highlight(state) { self.mask.generation(state, SELECT_STYLE) } else { 0 };
        TAG ^ ((generation << 2) | crate::mask::slot(kind) as u64)
    }

    fn refresh(&mut self, state: &AppState) {
        let rev = state.doc.revision();
        if !self.cache.valid || self.cache.revision != rev {
            self.cache.faces = state.doc.selection().faces.iter().copied().collect();
            self.cache.summary = Arc::new(faces::summarize(&state.doc, &self.cache.faces));
            self.cache.revision = rev;
            self.cache.valid = true;
        }
    }

    fn report<T>(state: &mut AppState, r: hp_editor::EditResult<T>) -> Option<T> {
        match r {
            Ok(v) => Some(v),
            Err(e) => {
                state.notify(Severity::Error, tf!("tex.tool.edit_failed", e));
                None
            }
        }
    }

    fn merge_tag(&mut self, field: &str, r: &egui::Response) -> String {
        if r.drag_started() || r.gained_focus() {
            self.drag_serial += 1;
        }
        format!("hp_face_edit:{field}:{}", self.drag_serial)
    }

    fn edit(&mut self, state: &mut AppState, field: &str, r: &egui::Response, edit: FaceEdit) {
        let tag = self.merge_tag(field, r);
        let faces = Arc::clone(&self.cache.faces);
        let res = faces::apply_edit(&mut state.doc, &faces, &edit, Some(&tag));
        Self::report(state, res);
    }

    fn material_row(&mut self, ui: &mut egui::Ui, state: &mut AppState) {
        let summary = Arc::clone(&self.cache.summary);
        let name = summary.material.value().unwrap_or_default();
        let mixed = summary.material.is_mixed();
        let materials = &summary.materials;
        let faces = Arc::clone(&self.cache.faces);
        let frame = egui::Frame::group(ui.style()).inner_margin(4.0);
        let (inner, dropped) = ui.dnd_drop_zone::<hp_app::dnd::DragPayload, _>(frame, |ui| {
            ui.horizontal(|ui| {
                let size = 48.0;
                let (rect, _) = ui.allocate_exact_size(egui::vec2(size, size), egui::Sense::hover());
                ui.painter().rect_filled(rect, 3.0, ui.visuals().extreme_bg_color);
                if !mixed && !name.is_empty() {
                    let thumb = state.assets.thumbnail(ui.ctx(), &name);
                    crate::browser::paint_thumbnail(ui.painter(), rect, thumb, false);
                } else {
                    let half = size * 0.5;
                    for (i, m) in materials.iter().take(4).enumerate() {
                        let offset = egui::vec2((i % 2) as f32 * half, (i / 2) as f32 * half);
                        let cell = egui::Rect::from_min_size(rect.min + offset, egui::Vec2::splat(half)).shrink(1.0);
                        let thumb = state.assets.thumbnail(ui.ctx(), m);
                        crate::browser::paint_thumbnail(ui.painter(), cell, thumb, false);
                    }
                }
                ui.vertical(|ui| {
                    if mixed {
                        let list: Vec<&str> = materials.iter().take(20).map(String::as_str).collect();
                        ui.label(RichText::new(t!("common.multiple_values")).italics()).on_hover_text(list.join("\n"));
                        ui.weak(tf!("tex.face.materials_count", materials.len()));
                    } else {
                        ui.label(RichText::new(crate::util::leaf_name(&name)).strong()).on_hover_text(&name);
                        ui.weak(crate::util::folder_of(&name));
                    }
                    ui.horizontal(|ui| {
                        if ui.button(t!("tex.face.browse")).on_hover_text(t!("tex.face.browse_tip")).clicked() {
                            crate::shared::set_browse_for_faces(ui.ctx(), true);
                            state.ui.right_tab = RightTab::Textures;
                            state.ui.focus_texture_search = true;
                        }
                        let current = state.current_material.clone();
                        if ui
                            .button(t!("tex.face.apply_current"))
                            .on_hover_text(tf!("tex.face.apply_current_tip", current))
                            .clicked()
                        {
                            let r = state.doc.apply_material(&faces, &current);
                            Self::report(state, r);
                        }
                        if !mixed
                            && !name.is_empty()
                            && crate::icons::icon_button(ui, crate::icons::Icon::Image, t!("tex.face.make_current"))
                                .clicked()
                        {
                            state.use_material(&name);
                        }
                    });
                });
            });
        });
        let _ = inner;
        if let Some(payload) = dropped
            && let hp_app::dnd::DragPayload::Material(m) = payload.as_ref()
        {
            let r = state.doc.apply_material(&faces, m);
            if Self::report(state, r).is_some() {
                state.use_material(m);
            }
        }
    }

    fn values_ui(&mut self, ui: &mut egui::Ui, state: &mut AppState) {
        let s = Arc::clone(&self.cache.summary);
        let prefs = &state.settings.entities;
        let (decimals, shift_step, scale_step) = (prefs.decimals(), prefs.shift_step(), prefs.scale_step());
        egui::Grid::new("hp_face_values").num_columns(3).spacing([8.0, 4.0]).show(ui, |ui| {
            ui.label("");
            ui.label(RichText::new("X / U").color(theme::text_weak()));
            ui.label(RichText::new("Y / V").color(theme::text_weak()));
            ui.end_row();

            ui.label(t!("tex.face.scale"));
            if let Some((v, r)) = multi_f64(ui, &s.u_scale, scale_step, decimals)
                && v.abs() > 1e-4
            {
                self.edit(state, "u_scale", &r, FaceEdit { u_scale: Some(v), ..FaceEdit::default() });
            }
            if let Some((v, r)) = multi_f64(ui, &s.v_scale, scale_step, decimals)
                && v.abs() > 1e-4
            {
                self.edit(state, "v_scale", &r, FaceEdit { v_scale: Some(v), ..FaceEdit::default() });
            }
            ui.end_row();

            ui.label(t!("tex.face.shift"));
            if let Some((v, r)) = multi_f64(ui, &s.u_shift, shift_step, decimals) {
                self.edit(state, "u_shift", &r, FaceEdit { u_shift: Some(v), ..FaceEdit::default() });
            }
            if let Some((v, r)) = multi_f64(ui, &s.v_shift, shift_step, decimals) {
                self.edit(state, "v_shift", &r, FaceEdit { v_shift: Some(v), ..FaceEdit::default() });
            }
            ui.end_row();

            ui.label(t!("tex.face.rotation"));
            if let Some((v, r)) = multi_f64(ui, &s.rotation, 1.0, decimals) {
                self.edit(state, "rotation", &r, FaceEdit { rotation: Some(v), ..FaceEdit::default() });
            }
            ui.horizontal(|ui| {
                for (label, delta) in [("-90", -90.0), ("+90", 90.0)] {
                    let r = ui.small_button(label).on_hover_text(t!("tex.face.rotate_by"));
                    if r.clicked() {
                        let faces = Arc::clone(&self.cache.faces);
                        let res =
                            state.doc.edit_faces(&faces, &FaceEdit { rotate_by: Some(delta), ..FaceEdit::default() });
                        Self::report(state, res);
                    }
                }
            });
            ui.end_row();

            ui.label(t!("tex.face.lightmap"));
            if let Some((v, r)) = multi_i32(ui, &s.lightmap_scale) {
                self.edit(state, "lightmap", &r, FaceEdit { lightmap_scale: Some(v.max(1)), ..FaceEdit::default() });
            }
            ui.label("");
            ui.end_row();
        });
    }

    fn justify_ui(&mut self, ui: &mut egui::Ui, state: &mut AppState) {
        hp_app::widgets::section(ui, t!("tex.face.justify"));
        let modes = [
            (Justify::Left, "tex.face.j_left"),
            (Justify::Right, "tex.face.j_right"),
            (Justify::Top, "tex.face.j_top"),
            (Justify::Bottom, "tex.face.j_bottom"),
            (Justify::Center, "tex.face.j_center"),
            (Justify::Fit, "tex.face.j_fit"),
        ];
        ui.horizontal_wrapped(|ui| {
            for (mode, key) in modes {
                if ui.button(t!(key)).clicked() {
                    let faces = Arc::clone(&self.cache.faces);
                    let mats = Arc::clone(&self.cache.summary);
                    let sizes = material_sizes(state, mats.materials.iter().map(String::as_str));
                    let lookup = size_lookup(&sizes);
                    let r = state.doc.justify_faces(&faces, mode, self.treat_as_one, &lookup);
                    Self::report(state, r);
                }
            }
        });
        ui.checkbox(&mut self.treat_as_one, t!("tex.face.treat_as_one")).on_hover_text(t!("tex.face.treat_as_one_tip"));

        hp_app::widgets::section(ui, t!("tex.face.align"));
        ui.horizontal_wrapped(|ui| {
            for (align, key) in
                [(TextureAlign::World, "tex.face.align_world"), (TextureAlign::Face, "tex.face.align_face")]
            {
                if ui.button(t!(key)).clicked() {
                    let faces = Arc::clone(&self.cache.faces);
                    let r = state.doc.edit_faces(&faces, &FaceEdit { align: Some(align), ..FaceEdit::default() });
                    Self::report(state, r);
                }
            }
            if ui.button(t!("tex.face.align_view")).on_hover_text(t!("tex.face.align_view_tip")).clicked() {
                let faces = Arc::clone(&self.cache.faces);
                let cam = state.cameras.cam3d;
                let r = state.doc.align_faces_to_view(&faces, cam.right(), cam.up());
                Self::report(state, r);
            }
        });
        ui.horizontal(|ui| {
            ui.label(t!("tex.face.fit_repeats"));
            ui.add(egui::DragValue::new(&mut self.fit_u).speed(0.05).range(0.01..=256.0).max_decimals(2).prefix("U "));
            ui.add(egui::DragValue::new(&mut self.fit_v).speed(0.05).range(0.01..=256.0).max_decimals(2).prefix("V "));
            if ui.button(t!("tex.face.fit")).clicked() {
                let faces = Arc::clone(&self.cache.faces);
                let mats = Arc::clone(&self.cache.summary);
                let sizes = material_sizes(state, mats.materials.iter().map(String::as_str));
                let lookup = size_lookup(&sizes);
                let r = state.doc.fit_faces(&faces, self.fit_u, self.fit_v, self.treat_as_one, &lookup);
                Self::report(state, r);
            }
        });
    }

    fn smoothing_ui(&mut self, ui: &mut egui::Ui, state: &mut AppState) {
        hp_app::widgets::section(ui, t!("tex.face.smoothing"));
        let all = self.cache.summary.smoothing_all;
        let any = self.cache.summary.smoothing_any;
        let mut toggle: Option<(u32, bool)> = None;
        for row in 0..3u32 {
            ui.horizontal(|ui| {
                ui.spacing_mut().item_spacing.x = 2.0;
                for col in 0..8u32 {
                    let bit = row * 8 + col;
                    let mask = 1u32 << bit;
                    let on = all & mask != 0;
                    let partial = !on && any & mask != 0;
                    let mut text = RichText::new(format!("{}", bit + 1)).monospace();
                    if partial {
                        text = text.color(theme::warning());
                    }
                    let b =
                        egui::Button::selectable(on, text).frame_when_inactive(true).min_size(egui::vec2(26.0, 20.0));
                    let tip = if partial { t!("tex.face.smoothing_partial") } else { t!("tex.face.smoothing_tip") };
                    if ui.add(b).on_hover_text(tip).clicked() {
                        toggle = Some((bit, !on));
                    }
                }
            });
        }
        if let Some((bit, on)) = toggle {
            let faces = Arc::clone(&self.cache.faces);
            let r = faces::set_smoothing_bit(&mut state.doc, &faces, bit, on);
            Self::report(state, r);
        }
    }

    fn disp_ui(&mut self, ui: &mut egui::Ui, state: &mut AppState) {
        hp_app::widgets::section(ui, t!("tex.face.disp"));
        ui.horizontal_wrapped(|ui| {
            ui.label(t!("tex.face.disp_create"));
            for power in [2u32, 3, 4] {
                if ui.button(format!("{power}")).on_hover_text(tf!("tex.face.disp_power", power)).clicked() {
                    let faces = Arc::clone(&self.cache.faces);
                    match state.doc.create_displacements(&faces, power) {
                        Ok(0) => state.notify(Severity::Warning, t!("tex.face.disp_none")),
                        Ok(n) => state.notify(Severity::Success, tf!("tex.face.disp_created", n)),
                        Err(e) => state.notify(Severity::Error, tf!("tex.face.disp_failed", e)),
                    }
                }
            }
            let has = self.cache.summary.displacements > 0;
            if ui.add_enabled(has, egui::Button::new(t!("tex.face.disp_destroy"))).clicked() {
                let faces = Arc::clone(&self.cache.faces);
                let r = state.doc.destroy_displacements(&faces);
                if let Some(n) = Self::report(state, r) {
                    state.notify(Severity::Success, tf!("tex.face.disp_destroyed", n));
                }
            }
        });
    }

    fn tools_ui(&mut self, ui: &mut egui::Ui, state: &mut AppState) {
        ui.horizontal_wrapped(|ui| {
            if ui.button(t!("tex.face.mark")).on_hover_text(t!("tex.face.mark_tip")).clicked() {
                let mats = Arc::clone(&self.cache.summary);
                let list = faces::faces_with_materials(&state.doc, &mats.materials);
                state.doc.select_faces(&list, SelectOp::Replace);
                state.notify(Severity::Info, tf!("tex.face.marked", list.len()));
            }
            if ui
                .add(egui::Button::selectable(self.hide_mask, t!("tex.face.hide_mask")).frame_when_inactive(true))
                .on_hover_text(t!("tex.face.hide_mask_tip"))
                .clicked()
            {
                self.hide_mask = !self.hide_mask;
            }
            if ui.button(t!("tex.face.select_none")).clicked() {
                state.doc.clear_face_selection();
            }
        });
    }
}

fn multi_f64(ui: &mut egui::Ui, m: &Multi<f64>, speed: f64, decimals: usize) -> Option<(f64, egui::Response)> {
    let mut v = m.value()?;
    let mut dv = egui::DragValue::new(&mut v).speed(speed).max_decimals(decimals);
    if m.is_mixed() {
        dv = dv.custom_formatter(|_, _| t!("common.multiple_values").to_owned());
    }
    let r = ui.add_sized([96.0, ui.spacing().interact_size.y], dv);
    (r.changed() && v.is_finite()).then_some((v, r))
}

fn multi_i32(ui: &mut egui::Ui, m: &Multi<i32>) -> Option<(i32, egui::Response)> {
    let mut v = m.value()?;
    let mut dv = egui::DragValue::new(&mut v).speed(0.25).range(1..=1024);
    if m.is_mixed() {
        dv = dv.custom_formatter(|_, _| t!("common.multiple_values").to_owned());
    }
    let r = ui.add_sized([96.0, ui.spacing().interact_size.y], dv);
    r.changed().then_some((v, r))
}

impl hp_app::Tool for TextureTool {
    fn id(&self) -> ToolId {
        ToolId::Texture
    }

    fn set_option(&mut self, state: &mut AppState, key: &str, value: &str) -> bool {
        let flag = match value.to_ascii_lowercase().as_str() {
            "on" | "true" | "1" => Some(true),
            "off" | "false" | "0" => Some(false),
            _ => None,
        };
        match key {
            "uv_place" => {
                let place = match value.to_ascii_lowercase().as_str() {
                    "panel" => UvPlace::Panel,
                    "window" | "detached" => UvPlace::Window,
                    _ => return false,
                };
                self.uv_place = place;
                true
            }
            "uv_all_faces" => flag.map(|f| self.uv.all_faces = f).is_some(),
            _ => self.uv.set_option(state, key, value),
        }
    }

    fn option_values(&self) -> Vec<hp_app::tool::ToolOption> {
        use hp_app::tool::ToolOption;
        let place = match self.uv_place {
            UvPlace::Panel => "panel",
            UvPlace::Window => "window",
        };
        let mut out = vec![
            ToolOption::new(
                "uv_place",
                place,
                "where the UV editor is: panel (docked in the Face edit panel) or window (a window of its own, like \
                 the Compile and run window)",
            ),
            ToolOption::new(
                "uv_all_faces",
                if self.uv.all_faces { "on" } else { "off" },
                "UV editor: edit every selected face (on) or only the edited one (off)",
            ),
        ];
        out.extend(self.uv.option_values());
        out
    }

    fn activate(&mut self, state: &mut AppState) {
        self.saved_objects = state.doc.selected_objects();
        if state.doc.selection().faces.is_empty() && !self.saved_objects.is_empty() {
            let faces = faces::faces_of_selected_solids(&state.doc);
            state.doc.select_faces(&faces, SelectOp::Replace);
        }
        if !self.saved_objects.is_empty() {
            state.doc.select(&[], SelectOp::Replace);
        }
        self.cache.valid = false;
        state.ui.right_tab = RightTab::Tool;
        self.hover = None;
    }

    fn deactivate(&mut self, state: &mut AppState) {
        self.mask.clear();
        state.doc.clear_face_selection();
        let saved = std::mem::take(&mut self.saved_objects);
        if !saved.is_empty() {
            state.doc.select(&saved, SelectOp::Replace);
        }
        self.hover = None;
        self.hover_key = None;
        if state.ui.right_tab == RightTab::Tool {
            state.ui.right_tab = RightTab::Textures;
        }
    }

    fn view_input(&mut self, state: &mut AppState, input: &ViewInput<'_>) -> ToolFlow {
        let this_view = input.view.index;
        let pointer = input.pointer.filter(|_| !input.view.is_2d() && !input.navigating);
        let Some(pos) = pointer else {
            if self.hover_view == Some(this_view) {
                self.hover = None;
                self.hover_key = None;
                self.hover_view = None;
            }
            return ToolFlow::Ignored;
        };
        let key = (pos, state.doc.revision(), this_view);
        if self.hover_key != Some(key) {
            self.hover = pick_face(state, input.view, pos);
            self.hover_key = Some(key);
        }
        self.hover_view = Some(this_view);
        self.hover_frame = state.frame.index;
        let r = input.response;
        let plain = !input.modifiers.alt;
        if r.drag_started_by(egui::PointerButton::Primary) && plain {
            self.painting = true;
            if let Some(face) = self.hover {
                let add = input.modifiers.ctrl || input.modifiers.command || input.modifiers.shift;
                state.doc.select_faces(&[face], if add { SelectOp::Add } else { SelectOp::Replace });
            }
            return ToolFlow::Handled;
        }
        if self.painting {
            if r.dragged_by(egui::PointerButton::Primary) {
                if let Some(face) = self.hover
                    && !state.doc.selection().faces.contains(&face)
                {
                    state.doc.select_faces(&[face], SelectOp::Add);
                }
                return ToolFlow::Handled;
            }
            self.painting = false;
            if r.drag_stopped() {
                return ToolFlow::Handled;
            }
        }
        let kind = if r.clicked() {
            Some(click_kind(false, input.modifiers))
        } else if r.secondary_clicked() {
            Some(click_kind(true, input.modifiers))
        } else {
            None
        };
        let Some(kind) = kind else { return ToolFlow::Ignored };
        match self.hover {
            Some(face) => click_face(state, face, kind),
            None => {
                if kind == ClickKind::Select {
                    state.doc.clear_face_selection();
                }
            }
        }
        ToolFlow::Handled
    }

    fn overlay(&self, state: &AppState, view: &ViewInfo, out: &mut Overlay) {
        self.overlay_impl(state, view, out);
    }

    fn retained_overlay_version(&self, state: &AppState, view: &ViewInfo) -> Option<u64> {
        Some(self.highlight_version(state, view.kind))
    }

    fn retained_overlay(&self, state: &AppState, view: &ViewInfo, out: &mut Overlay) {
        if self.shows_highlight(state) {
            self.mask.get(state, view.kind, SELECT_STYLE).emit(out);
        }
    }

    fn options_ui(&mut self, ui: &mut egui::Ui, state: &mut AppState) {
        self.refresh(state);
        self.uv_window(ui.ctx(), state);
        let count = self.cache.summary.count;
        ui.horizontal(|ui| {
            ui.with_layout(egui::Layout::right_to_left(egui::Align::Center), |ui| {
                let text = if self.cache.summary.displacements > 0 {
                    tf!("tex.face.count_disp", count, self.cache.summary.displacements)
                } else {
                    tf!("tex.face.count", count)
                };
                ui.weak(text);
            });
        });
        ui.horizontal(|ui| {
            for (action, text, enabled) in [
                (Action::FaceSelectionBack, t!("tex.face.prev_selection"), state.doc.can_face_selection_back()),
                (Action::FaceSelectionForward, t!("tex.face.next_selection"), state.doc.can_face_selection_forward()),
            ] {
                let mut tip = action.label().to_owned();
                if let Some(sc) = state.keys.shortcut(action) {
                    tip.push_str(&format!(" ({})", shortcut_label(&sc)));
                }
                if ui.add_enabled(enabled, egui::Button::new(text).small()).on_hover_text(tip).clicked() {
                    if action == Action::FaceSelectionBack {
                        state.doc.face_selection_back();
                    } else {
                        state.doc.face_selection_forward();
                    }
                }
            }
        });
        ui.separator();
        egui::ScrollArea::vertical().id_salt("hp_face_edit_scroll").auto_shrink([false, false]).show(ui, |ui| {
            if count == 0 {
                ui.add_space(6.0);
                ui.label(RichText::new(t!("tex.face.none")).color(ui.visuals().weak_text_color()));
                return;
            }
            self.material_row(ui, state);
            ui.add_space(4.0);
            self.values_ui(ui, state);
            egui::CollapsingHeader::new(RichText::new(t!("tex.uv.title")).strong())
                .id_salt("hp_face_uv_editor")
                .default_open(true)
                .show(ui, |ui| {
                    if self.uv_place != UvPlace::Panel {
                        ui.horizontal_wrapped(|ui| {
                            ui.weak(t!("tex.uv.in_window"));
                            if ui.small_button(t!("tex.uv.back_to_panel")).clicked() {
                                self.uv_place = UvPlace::Panel;
                            }
                        });
                        return;
                    }
                    if ui.small_button(t!("tex.uv.detach")).on_hover_text(t!("tex.uv.detach_tip")).clicked() {
                        self.uv_place = UvPlace::Window;
                    }
                    self.uv.fill = false;
                    let faces = Arc::clone(&self.cache.faces);
                    self.uv.ui(ui, state, &faces);
                });
            self.justify_ui(ui, state);
            self.smoothing_ui(ui, state);
            self.disp_ui(ui, state);
            hp_app::widgets::section(ui, t!("tex.face.selection"));
            self.tools_ui(ui, state);
        });
    }

    fn wants_side_panel(&self) -> bool {
        true
    }

    fn panel_label_key(&self) -> &'static str {
        "tex.face.title"
    }

    fn cursor(&self, _state: &AppState, view: &ViewInfo, _pos: egui::Pos2) -> Option<egui::CursorIcon> {
        (!view.is_2d() && self.hover.is_some()).then_some(egui::CursorIcon::PointingHand)
    }

    fn status_hint(&self, _state: &AppState) -> Option<String> {
        Some(t!("tex.tool.hint").to_owned())
    }

    fn cancel(&mut self, state: &mut AppState) -> bool {
        if state.doc.selection().faces.is_empty() {
            return false;
        }
        state.doc.clear_face_selection();
        true
    }
}

impl TextureTool {
    fn overlay_impl(&self, state: &AppState, view: &ViewInfo, out: &mut Overlay) {
        let fresh = state.frame.index <= self.hover_frame.saturating_add(1);
        if !view.is_2d()
            && fresh
            && let Some(h) = self.hover
        {
            crate::mask::face_outline(state, h, HOVER_LINE, true, out);
        }
    }

    fn uv_window(&mut self, ctx: &egui::Context, state: &mut AppState) {
        if self.uv_place == UvPlace::Panel {
            self.uv_native.forget();
            self.uv_builder = None;
            return;
        }
        let faces = Arc::clone(&self.cache.faces);
        let mut open = true;
        let mut back = false;
        if ctx.embed_viewports() {
            hp_app::chrome::Window::new(t!("tex.uv.title"))
                .id(egui::Id::unique("hp_uv_editor_window"))
                .default_size([580.0, 800.0])
                .resizable(true)
                .open(&mut open)
                .show(ctx, |ui| {
                    if ui.small_button(t!("tex.uv.back_to_panel")).clicked() {
                        back = true;
                    }
                    self.uv_contents(ui, state, &faces);
                });
        } else {
            let native = state.settings.ui.native_frames;
            let builder = self.uv_builder.get_or_insert_with(|| {
                hp_app::native_window::builder(
                    ctx,
                    t!("tex.uv.title"),
                    egui::vec2(640.0, 860.0),
                    egui::vec2(320.0, 360.0),
                    state.settings.ui.uv_window.as_ref(),
                    native,
                )
            });
            *builder = hp_app::chrome::builder(builder.clone(), native);
            let builder = builder.clone();
            let title = builder.title.clone().unwrap_or_default();
            ctx.show_viewport_immediate(
                egui::ViewportId::from_hash_of("hp_uv_editor_viewport"),
                builder,
                |ui, _class| {
                    let ctx = ui.ctx().clone();
                    let restore = state.settings.ui.uv_window.clone();
                    if let Some(now) = self.uv_native.follow(&ctx, &title, restore.as_ref(), Some(UV_WINDOW))
                        && restore.as_ref() != Some(&now)
                    {
                        state.settings.ui.uv_window = Some(now);
                        state.settings_dirty = true;
                    }
                    let spec = hp_app::chrome::ChromeSpec::dialog(t!("tex.uv.title"), native);
                    let ((), frame) = hp_app::chrome::dialog(ui, spec, |ui| {
                        egui::CentralPanel::default().show(ui, |ui| {
                            if native && ui.small_button(t!("tex.uv.back_to_panel")).clicked() {
                                back = true;
                            }
                            self.uv_contents(ui, state, &faces);
                        });
                    });
                    match frame.clicked {
                        Some(hp_app::chrome::ChromeButton::Place) => back = true,
                        Some(hp_app::chrome::ChromeButton::Close) => open = false,
                        _ => {}
                    }
                    if ctx.input(|i| i.viewport().close_requested()) {
                        open = false;
                    }
                },
            );
        }
        if back || !open {
            self.uv_place = UvPlace::Panel;
            self.uv_native.forget();
            self.uv_builder = None;
        }
    }

    fn uv_contents(&mut self, ui: &mut egui::Ui, state: &mut AppState, faces: &[FaceRef]) {
        if faces.is_empty() {
            ui.weak(t!("tex.face.none"));
            return;
        }
        self.uv.fill = true;
        self.uv.ui(ui, state, faces);
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use glam::DVec3;
    use hp_app::Tool;

    #[test]
    fn uv_editor_options() {
        use hp_app::Tool;
        let mut state = hp_app::testing::test_state();
        let mut tool = TextureTool::new();
        let get = |t: &TextureTool, k: &str| t.option_values().into_iter().find(|o| o.key == k).map(|o| o.value);
        assert!(tool.set_option(&mut state, "uv_place", "window"));
        assert!(tool.set_option(&mut state, "uv_mode", "edge"));
        assert!(tool.set_option(&mut state, "uv_layer", "bump2"));
        assert!(tool.set_option(&mut state, "uv_keep_layers", "off"));
        assert!(tool.set_option(&mut state, "uv_all_faces", "off"));
        assert!(tool.set_option(&mut state, "uv_fit_tiles", "2"));
        assert!(!tool.set_option(&mut state, "uv_mode", "corner"));
        assert!(!tool.set_option(&mut state, "uv_layer", "nope"));
        assert_eq!(get(&tool, "uv_place").as_deref(), Some("window"));
        assert!(tool.set_option(&mut state, "uv_place", "panel"));
        assert!(tool.set_option(&mut state, "uv_place", "detached"));
        assert_eq!(get(&tool, "uv_place").as_deref(), Some("window"));
        assert!(!tool.set_option(&mut state, "uv_place", "floating"));
        assert_eq!(get(&tool, "uv_mode").as_deref(), Some("edge"));
        assert_eq!(get(&tool, "uv_layer").as_deref(), Some("bump2"));
        assert_eq!(get(&tool, "uv_keep_layers").as_deref(), Some("off"));
        assert_eq!(get(&tool, "uv_all_faces").as_deref(), Some("off"));
        assert_eq!(get(&tool, "uv_fit_tiles").as_deref(), Some("2"));
    }

    #[test]
    fn click_mapping() {
        let none = egui::Modifiers::NONE;
        assert_eq!(click_kind(false, none), ClickKind::Select);
        assert_eq!(click_kind(false, egui::Modifiers::COMMAND), ClickKind::Toggle);
        assert_eq!(click_kind(false, egui::Modifiers::SHIFT), ClickKind::SelectSolid { add: false });
        assert_eq!(
            click_kind(false, egui::Modifiers { shift: true, command: true, ..none }),
            ClickKind::SelectSolid { add: true }
        );
        assert_eq!(click_kind(false, egui::Modifiers::ALT), ClickKind::Eyedropper);
        assert_eq!(click_kind(true, none), ClickKind::Apply);
        assert_eq!(click_kind(true, egui::Modifiers::ALT), ClickKind::Wrap);
    }

    #[test]
    fn click_actions() {
        let mut state = hp_app::testing::test_state();
        let f = FaceRef::new(2, 0);
        click_face(&mut state, f, ClickKind::Select);
        assert_eq!(state.doc.selected_faces(), [f]);
        click_face(&mut state, FaceRef::new(2, 1), ClickKind::Toggle);
        assert_eq!(state.doc.selected_faces().len(), 2);
        click_face(&mut state, FaceRef::new(2, 1), ClickKind::Toggle);
        assert_eq!(state.doc.selected_faces(), [f]);
        click_face(&mut state, FaceRef::new(3, 0), ClickKind::SelectSolid { add: false });
        assert_eq!(state.doc.selected_faces().len(), 6);
        click_face(&mut state, f, ClickKind::SelectSolid { add: true });
        assert_eq!(state.doc.selected_faces().len(), 12);

        click_face(&mut state, FaceRef::new(3, 2), ClickKind::Eyedropper);
        assert_eq!(state.current_material, "concrete/concretefloor001a");

        state.use_material("brick/brickwall001a");
        let before = state.doc.history_len().0;
        click_face(&mut state, f, ClickKind::Apply);
        assert_eq!(state.doc.face_side(f).map(|s| s.material.as_str()), Some("brick/brickwall001a"));
        assert_eq!(state.doc.history_len().0, before + 1);

        state.doc.select_faces(&[FaceRef::new(2, 0)], SelectOp::Replace);
        click_face(&mut state, FaceRef::new(2, 2), ClickKind::Wrap);
        assert_eq!(state.doc.face_side(FaceRef::new(2, 2)).map(|s| s.material.as_str()), Some("brick/brickwall001a"));
    }

    #[test]
    fn undo_brings_back_deselected_faces() {
        let mut state = hp_app::testing::test_state();
        state.apply_selection_undo_setting();
        assert!(state.doc.records_selection(), "on by default");
        let dirty = state.doc.is_dirty();
        let mut tool = TextureTool::new();
        tool.activate(&mut state);
        let (f, g) = (FaceRef::new(2, 0), FaceRef::new(3, 1));
        click_face(&mut state, f, ClickKind::Select);
        state.doc.end_selection_burst();
        click_face(&mut state, g, ClickKind::Toggle);
        state.doc.end_selection_burst();
        state.use_material("brick/brickwall001a");
        click_face(&mut state, g, ClickKind::Apply);
        state.doc.end_selection_burst();
        assert!(tool.cancel(&mut state));
        state.doc.end_selection_burst();
        assert!(state.doc.selected_faces().is_empty());
        assert_eq!(state.doc.undo_label(), Some(hp_editor::FACE_SELECTION_STEP));
        assert!(state.doc.undo());
        assert_eq!(state.doc.selected_faces(), [f, g], "the faces, in their order");
        assert!(state.doc.undo(), "the material");
        let material = state.doc.face_side(g).map(|s| s.material.to_ascii_lowercase());
        assert_eq!(material.as_deref(), Some("concrete/concretefloor001a"));
        assert_eq!(state.doc.selected_faces(), [f, g]);
        assert!(state.doc.undo());
        assert_eq!(state.doc.selected_faces(), [f]);
        assert_eq!(state.doc.is_dirty(), dirty, "selection steps are not modifications");
        assert!(state.doc.redo() && state.doc.redo() && state.doc.redo());
        assert!(state.doc.selected_faces().is_empty());
        state.settings.edit.undo_restores_selection = false;
        state.apply_selection_undo_setting();
        let steps = state.doc.history_len().0;
        click_face(&mut state, f, ClickKind::Select);
        state.doc.end_selection_burst();
        assert_eq!(state.doc.history_len().0, steps);
    }

    #[test]
    fn activation_converts_selection() {
        let mut state = hp_app::testing::test_state();
        state.doc.select(&[ObjRef::Solid(2)], SelectOp::Replace);
        let mut tool = TextureTool::new();
        tool.activate(&mut state);
        assert_eq!(state.doc.selected_faces().len(), 6);
        assert!(state.doc.selected_objects().is_empty());
        assert_eq!(state.ui.right_tab, RightTab::Tool);
        assert!(tool.cancel(&mut state));
        assert!(state.doc.selected_faces().is_empty());
        assert!(!tool.cancel(&mut state));
        tool.deactivate(&mut state);
        assert_eq!(state.doc.selected_objects(), [ObjRef::Solid(2)]);
        assert_eq!(state.ui.right_tab, RightTab::Textures);
    }

    fn retained(tool: &TextureTool, state: &AppState, view: &ViewInfo) -> Overlay {
        let mut out = Overlay::default();
        tool.retained_overlay(state, view, &mut out);
        out
    }

    #[test]
    fn overlay_of_selected_faces() {
        let mut state = hp_app::testing::test_state();
        state.doc.select_faces(&[FaceRef::new(2, 0), FaceRef::new(3, 1)], SelectOp::Replace);
        let tool = TextureTool::new();
        let (view, top) = views();
        let out = retained(&tool, &state, &view);
        assert_eq!(out.triangles.len(), 4);
        assert_eq!(out.lines.len(), 8);
        assert!(out.lines_on_top.is_empty());
        let out2 = retained(&tool, &state, &top);
        assert!(out2.triangles.is_empty());
        assert!(out2.lines.is_empty());
        assert_eq!(out2.lines_on_top.len(), 7);
        let mut frame = Overlay::default();
        tool.overlay(&state, &view, &mut frame);
        assert!(frame.is_empty());
    }

    fn views() -> (ViewInfo, ViewInfo) {
        let view = ViewInfo {
            index: 0,
            kind: hp_app::ViewKind::Perspective,
            rect: egui::Rect::from_min_size(egui::Pos2::ZERO, egui::vec2(800.0, 600.0)),
            cam3d: hp_app::view::Camera3d::default(),
            cam2d: hp_app::view::Camera2d::default(),
            pixels_per_point: 1.0,
            grid: 16.0,
        };
        (view, ViewInfo { index: 1, kind: hp_app::ViewKind::Top, ..view })
    }

    #[test]
    fn shared_edges_are_drawn_once() {
        let mut state = hp_app::testing::test_state();
        let all = state.doc.all_faces_of(&[ObjRef::Solid(2)]);
        state.doc.select_faces(&all, SelectOp::Replace);
        let tool = TextureTool::new();
        let (view, top) = views();
        let out = retained(&tool, &state, &view);
        assert_eq!(out.triangles.len(), 12);
        assert_eq!(out.lines.len(), 12);
        let top_z = state.doc.scene().solid(2).map(|n| n.bounds.max.z).unwrap_or_default() + crate::mask::LIFT;
        let on_top = |t: &&hp_render::OverlayTri| t.p.iter().all(|p| (f64::from(p[2]) - top_z).abs() < 1e-3);
        assert_eq!(out.triangles.iter().filter(on_top).count(), 2, "the top face is lifted above the box");
        let out = retained(&tool, &state, &top);
        assert_eq!(out.lines_on_top.len(), 4);
    }

    #[test]
    fn retained_highlight_is_rebuilt_only_when_it_changes() {
        let mut state = hp_app::testing::test_state();
        let all = state.doc.all_faces_of(&[ObjRef::Solid(2)]);
        state.doc.select_faces(&all, SelectOp::Replace);
        let mut tool = TextureTool::new();
        let (view, top) = views();
        let version = |tool: &TextureTool, state: &AppState, v: &ViewInfo| tool.retained_overlay_version(state, v);
        let v3d = version(&tool, &state, &view);
        let v2d = version(&tool, &state, &top);
        assert!(v3d.is_some());
        assert_ne!(v3d, v2d, "2D and 3D content differ");
        let mut moved = view;
        moved.index = 2;
        moved.cam3d.pos = glam::DVec3::new(-900.0, 300.0, -400.0);
        moved.cam3d.look_at(glam::DVec3::ZERO);
        assert_eq!(version(&tool, &state, &moved), v3d);
        let panned =
            ViewInfo { cam2d: hp_app::view::Camera2d { center: glam::DVec2::new(20000.0, 0.0), zoom: 0.05 }, ..top };
        assert_eq!(version(&tool, &state, &panned), v2d);
        assert_eq!(retained(&tool, &state, &view), retained(&tool, &state, &moved));
        assert_eq!(retained(&tool, &state, &top), retained(&tool, &state, &panned));

        let edit = FaceEdit { u_shift: Some(7.0), ..FaceEdit::default() };
        state.doc.edit_faces(&all, &edit).unwrap();
        state.doc.apply_material(&all, "brick/brickwall001a").unwrap();
        assert_eq!(version(&tool, &state, &view), v3d);

        state.doc.select_faces(&all[..2], SelectOp::Replace);
        let v_sel = version(&tool, &state, &view);
        assert_ne!(v_sel, v3d);
        assert_eq!(retained(&tool, &state, &view).triangles.len(), 4);

        state.doc.translate(&[ObjRef::Solid(2)], glam::DVec3::new(0.0, 0.0, 512.0)).unwrap();
        let v_moved = version(&tool, &state, &view);
        assert_ne!(v_moved, v_sel);
        let out = retained(&tool, &state, &view);
        assert!(out.lines.iter().all(|l| l.a[2] > 300.0), "{:?}", out.lines.first());

        tool.hide_mask = true;
        let hidden = version(&tool, &state, &view);
        assert_ne!(hidden, v_moved);
        assert!(retained(&tool, &state, &view).is_empty());
        tool.hide_mask = false;
        assert_eq!(version(&tool, &state, &view), v_moved);
        state.doc.clear_face_selection();
        assert_eq!(version(&tool, &state, &view), hidden);
        assert!(retained(&tool, &state, &view).is_empty());

        state.doc.select_faces(&all[2..4], SelectOp::Replace);
        let first = version(&TextureTool::new(), &state, &view);
        state.doc.select_faces(&all[..1], SelectOp::Replace);
        let second = version(&TextureTool::new(), &state, &view);
        assert_ne!(first, second);
        tool.deactivate(&mut state);
        tool.activate(&mut state);
        state.doc.select_faces(&all[4..], SelectOp::Replace);
        let third = version(&tool, &state, &view);
        assert!(third != first && third != second);
    }

    #[test]
    fn mask_is_built_once_per_selection_or_geometry_change() {
        let mut state = hp_app::testing::test_state();
        let all = state.doc.all_faces_of(&[ObjRef::Solid(2)]);
        state.doc.select_faces(&all, SelectOp::Replace);
        let tool = TextureTool::new();
        let (view, top) = views();
        let mut seen: [Option<u64>; 2] = [None; 2];
        let mut fills = 0;
        let mut draw = |tool: &TextureTool, state: &AppState| {
            for (i, v) in [&view, &top].into_iter().enumerate() {
                let version = tool.retained_overlay_version(state, v);
                if seen[i] != version {
                    seen[i] = version;
                    let _ = retained(tool, state, v);
                    fills += 1;
                }
            }
        };
        for _ in 0..5 {
            draw(&tool, &state);
        }
        assert_eq!(tool.mask.builds(), 2, "one build per view kind");
        let edit = FaceEdit { u_shift: Some(7.0), ..FaceEdit::default() };
        state.doc.edit_faces(&all, &edit).unwrap();
        draw(&tool, &state);
        assert_eq!(tool.mask.builds(), 2, "texture edits keep the geometry");
        state.doc.select_faces(&all[..2], SelectOp::Replace);
        draw(&tool, &state);
        assert_eq!(tool.mask.builds(), 4);
        assert_eq!(fills, 4, "the viewports refill only on change");
    }

    #[test]
    fn picking_in_3d() {
        let state = hp_app::testing::test_state();
        let mut cam = hp_app::view::Camera3d { pos: DVec3::new(0.0, 0.0, 300.0), ..Default::default() };
        cam.look_at(DVec3::new(0.0, 1.0, 0.0));
        cam.pitch_deg = -89.0;
        let view = ViewInfo {
            index: 0,
            kind: hp_app::ViewKind::Perspective,
            rect: egui::Rect::from_min_size(egui::Pos2::ZERO, egui::vec2(800.0, 600.0)),
            cam3d: cam,
            cam2d: hp_app::view::Camera2d::default(),
            pixels_per_point: 1.0,
            grid: 16.0,
        };
        let f = pick_face(&state, &view, view.rect.center());
        assert_eq!(f, Some(FaceRef::new(2, 0)));
        let top = ViewInfo { kind: hp_app::ViewKind::Top, ..view };
        assert_eq!(pick_face(&state, &top, top.rect.center()), None);
    }

    #[test]
    fn hover_survives_other_views() {
        let mut state = hp_app::testing::test_state();
        let mut cam = hp_app::view::Camera3d { pos: DVec3::new(0.0, 0.0, 300.0), ..Default::default() };
        cam.look_at(DVec3::new(0.0, 1.0, 0.0));
        cam.pitch_deg = -89.0;
        let rect = egui::Rect::from_min_size(egui::Pos2::ZERO, egui::vec2(800.0, 600.0));
        let view3d = ViewInfo {
            index: 0,
            kind: hp_app::ViewKind::Perspective,
            rect,
            cam3d: cam,
            cam2d: hp_app::view::Camera2d::default(),
            pixels_per_point: 1.0,
            grid: 16.0,
        };
        let top = ViewInfo { index: 1, kind: hp_app::ViewKind::Top, ..view3d };
        let mut tool = TextureTool::new();
        hp_app::testing::run_ui(&mut state, |ui, s| {
            let response = ui.allocate_rect(rect, egui::Sense::click());
            let input = ViewInput {
                view: &view3d,
                response: &response,
                pointer: Some(rect.center()),
                modifiers: egui::Modifiers::NONE,
                navigating: false,
            };
            assert_eq!(tool.view_input(s, &input), ToolFlow::Ignored);
            let input2 = ViewInput { view: &top, response: &response, pointer: None, ..input };
            tool.view_input(s, &input2);
        });
        assert_eq!(tool.hovered_face(), Some(FaceRef::new(2, 0)));
        let mut out = Overlay::default();
        tool.overlay(&state, &view3d, &mut out);
        assert_eq!(out.lines_on_top.len(), 4, "hover outline");
        hp_app::testing::run_ui(&mut state, |ui, s| {
            let response = ui.allocate_rect(rect, egui::Sense::click());
            let input = ViewInput {
                view: &view3d,
                response: &response,
                pointer: None,
                modifiers: egui::Modifiers::NONE,
                navigating: false,
            };
            tool.view_input(s, &input);
        });
        assert_eq!(tool.hovered_face(), None);
    }

    pub(crate) fn grid_map(n: i64) -> String {
        use std::fmt::Write as _;
        let mut s = String::from("world\n{\n\"id\" \"1\"\n\"classname\" \"worldspawn\"\n");
        let mut id = 2;
        let side = |s: &mut String, id: &mut i64, p: String| {
            let _ = write!(
                s,
                "side\n{{\n\"id\" \"{id}\"\n\"plane\" \"{p}\"\n\"material\" \"BRICK/BRICKWALL001A\"\n\
                 \"uaxis\" \"[1 0 0 0] 0.25\"\n\"vaxis\" \"[0 -1 0 0] 0.25\"\n\"rotation\" \"0\"\n\
                 \"lightmapscale\" \"16\"\n\"smoothing_groups\" \"0\"\n}}\n"
            );
            *id += 1;
        };
        for i in 0..n {
            for j in 0..n {
                let (x0, y0, z0) = (i * 96 - n * 48, j * 96 - n * 48, 0);
                let (x1, y1, z1) = (x0 + 64, y0 + 64, 64 * (1 + (i * 7 + j * 3) % 3));
                let _ = write!(s, "solid\n{{\n\"id\" \"{id}\"\n");
                id += 1;
                side(&mut s, &mut id, format!("({x0} {y1} {z1}) ({x1} {y1} {z1}) ({x1} {y0} {z1})"));
                side(&mut s, &mut id, format!("({x0} {y0} {z0}) ({x1} {y0} {z0}) ({x1} {y1} {z0})"));
                side(&mut s, &mut id, format!("({x0} {y1} {z1}) ({x0} {y0} {z1}) ({x0} {y0} {z0})"));
                side(&mut s, &mut id, format!("({x1} {y1} {z0}) ({x1} {y0} {z0}) ({x1} {y0} {z1})"));
                side(&mut s, &mut id, format!("({x1} {y1} {z1}) ({x0} {y1} {z1}) ({x0} {y1} {z0})"));
                side(&mut s, &mut id, format!("({x1} {y0} {z0}) ({x0} {y0} {z0}) ({x0} {y0} {z1})"));
                s.push_str("}\n");
            }
        }
        s.push_str("}\n");
        s
    }

    #[test]
    #[ignore]
    fn mask_timing_with_300k_faces() {
        let mut state = hp_app::testing::test_state();
        state.doc = hp_editor::Document::from_map(hp_vmf::parse(&grid_map(224)).unwrap());
        let objs = state.doc.scene().solids().keys().map(|id| ObjRef::Solid(*id)).collect::<Vec<_>>();
        let all = state.doc.all_faces_of(&objs);
        let t = std::time::Instant::now();
        state.doc.select_faces(&all, SelectOp::Replace);
        eprintln!("select {} faces: {:?}", all.len(), t.elapsed());
        let mut tool = TextureTool::new();
        let (mut view, top) = views();
        view.cam3d.pos = glam::DVec3::new(-12000.0, -12000.0, 3000.0);
        view.cam3d.look_at(glam::DVec3::ZERO);
        let front = ViewInfo { kind: hp_app::ViewKind::Front, ..top };
        let side = ViewInfo { kind: hp_app::ViewKind::Side, ..top };
        let zoomed_out =
            |v: ViewInfo| ViewInfo { cam2d: hp_app::view::Camera2d { center: glam::DVec2::ZERO, zoom: 0.02 }, ..v };
        let views = [view, zoomed_out(top), zoomed_out(front), zoomed_out(side)];
        let mut out = Overlay::default();
        for round in 0..3 {
            for v in &views {
                out.clear();
                let t = std::time::Instant::now();
                tool.retained_overlay(&state, v, &mut out);
                eprintln!(
                    "round {round} {:?}: {:?} ({} tris, {} lines, {} on top)",
                    v.kind,
                    t.elapsed(),
                    out.triangles.len(),
                    out.lines.len(),
                    out.lines_on_top.len()
                );
            }
        }
        let t = std::time::Instant::now();
        hp_app::testing::run_ui(&mut state, |ui, s| hp_app::Tool::options_ui(&mut tool, ui, s));
        eprintln!("panel (3 frames, summary once): {:?}", t.elapsed());
        let t = std::time::Instant::now();
        hp_app::testing::run_ui(&mut state, |ui, s| hp_app::Tool::options_ui(&mut tool, ui, s));
        eprintln!("panel (3 frames, cached): {:?}", t.elapsed());
    }
}
