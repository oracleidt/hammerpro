use std::collections::HashMap;

use std::sync::Arc;

use egui::{RichText, ScrollArea, Ui, vec2};
use hp_app::actions::{Action, shortcut_label};
use hp_app::state::{ModelState, ParticleCatalog, PreviewCamera, ThumbState};
use hp_app::{AppState, t, tf, theme};
use hp_editor::ObjRef;
use hp_fgd::PropertyType;
use hp_fgd::ResolvedFgd;

use crate::cx::Cx;
use crate::edit::Op;
use crate::io_ui::{IoAction, IoEditor};
use crate::names::MapNames;
use crate::smartedit::{SmartEdit, picker_is_open, thumb_names, view_has_kind};
use crate::view::{DocStamp, EditKind, EntityView, SelectionSummary, Target};
use crate::widgets::{CommitAction, FieldOpts, PickerItem, icon, picker_button, picker_popup, tab_bar};

const QUICK_TIE: [&str; 5] = ["func_detail", "func_brush", "func_illusionary", "trigger_multiple", "trigger_once"];

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum EntityTab {
    Properties,
    Flags,
    Outputs,
    Inputs,
    Raw,
    Model,
    Visgroups,
}

pub struct EntityPanel {
    smart: SmartEdit,
    io: IoEditor,
    tab: EntityTab,
    names: MapNames,
    names_pending: bool,
    summary: SelectionSummary,
    summary_rev: Option<DocStamp>,
    view: EntityView,
    view_key: Option<(DocStamp, usize, Target)>,
    thumbs: HashMap<String, ThumbState>,
    last_target: Option<Target>,
    tie_class: String,
    wants_particles: bool,
    particles: Option<Arc<ParticleCatalog>>,
    model_path: Option<String>,
    model_info: Option<ModelState>,
    model_camera: PreviewCamera,
    model_camera_path: Option<String>,
    model_autoplay: bool,
    model_preview: Option<egui::TextureId>,
}

impl Default for EntityPanel {
    fn default() -> Self {
        Self::new()
    }
}

impl EntityPanel {
    pub fn new() -> Self {
        Self {
            smart: SmartEdit::new("entity_panel"),
            io: IoEditor::new(),
            tab: EntityTab::Properties,
            names: MapNames::default(),
            names_pending: false,
            summary: SelectionSummary::default(),
            summary_rev: None,
            view: EntityView::empty(),
            view_key: None,
            thumbs: HashMap::new(),
            last_target: None,
            tie_class: String::new(),
            wants_particles: false,
            particles: None,
            model_path: None,
            model_info: None,
            model_camera: PreviewCamera::default(),
            model_camera_path: None,
            model_autoplay: false,
            model_preview: None,
        }
    }

    pub fn tab(&self) -> EntityTab {
        self.tab
    }

    pub fn set_tab(&mut self, tab: EntityTab) {
        self.tab = tab;
    }

    #[cfg(test)]
    pub(crate) fn io_mut(&mut self) -> &mut IoEditor {
        &mut self.io
    }

    #[cfg(test)]
    pub(crate) fn field_id(&self, key: &str) -> egui::Id {
        self.smart.test_field_id(key)
    }

    pub fn ui(&mut self, ui: &mut Ui, state: &mut AppState) {
        self.ui_with_fgd(ui, state, None);
    }

    pub fn ui_with_fgd(&mut self, ui: &mut Ui, state: &mut AppState, fgd: Option<&ResolvedFgd>) {
        self.refresh(state, fgd);
        if self.names_pending {
            ui.ctx().request_repaint_after(std::time::Duration::from_secs_f64(crate::names::REFRESH_INTERVAL_S));
        }
        if let [id] = self.summary.entities.as_slice()
            && let Some(entity) = state.doc.scene().entity(*id).map(|n| &n.entity)
            && entity.classname.eq_ignore_ascii_case("env_fade")
        {
            let entity = entity.clone();
            ui.horizontal(|ui| {
                ui.label(t!("entities.fade_preview"));
                if ui.button(t!("entities.fade_play")).clicked() {
                    state.runtime.fade.play(&entity, state.sidecar.serial(), state.frame.time);
                    ui.ctx().request_repaint();
                }
                if state.runtime.fade.sample(state).is_some() && ui.button(t!("entities.fade_stop")).clicked() {
                    state.runtime.fade.stop();
                    ui.ctx().request_repaint();
                }
            });
            ui.separator();
        }
        for (name, thumb) in self.thumbs.iter_mut() {
            *thumb = state.assets.thumbnail(ui.ctx(), name);
        }
        self.model_info = None;
        self.model_preview = None;
        if self.tab == EntityTab::Model
            && let Some(path) = &self.model_path
        {
            self.model_info = Some(state.assets.model_info(path));
            if self.model_camera_path.as_deref() != Some(path.as_str()) {
                self.model_camera.anim = None;
                self.model_camera.anim_time = 0.0;
                self.model_camera_path = Some(path.clone());
                self.model_autoplay = true;
            }
            if self.model_camera.anim.is_some() {
                self.model_camera.anim_time += ui.input(|i| i.stable_dt).clamp(0.0, 0.1);
                ui.ctx().request_repaint();
            }
            let px = (crate::panel_tabs::PREVIEW_SIZE * ui.ctx().pixels_per_point()).round().max(16.0) as u32;
            if let Some(gfx) = state.gfx.as_mut() {
                self.model_preview = state.assets.model_preview(gfx, path, &self.model_camera, [px, px]);
            }
        }
        self.particles = if self.wants_particles { state.assets.particles() } else { None };
        if self.wants_particles && self.particles.is_none() && state.assets.fs().is_some() {
            ui.ctx().request_repaint_after(std::time::Duration::from_millis(200));
        }
        let tie_shortcut = if self.summary.entities.is_empty() && !self.summary.world_solids.is_empty() {
            state.keys.shortcut(Action::TieToEntity).map(|s| shortcut_label(&s))
        } else {
            None
        };
        let move_shortcut = if self.summary.brush_count > 0 {
            state.keys.shortcut(Action::MoveToWorld).map(|s| shortcut_label(&s))
        } else {
            None
        };
        if self.tie_class.is_empty() {
            self.tie_class = if state.settings.edit.default_brush_entity.trim().is_empty() {
                "func_detail".to_owned()
            } else {
                state.settings.edit.default_brush_entity.clone()
            };
        }

        let Self {
            smart,
            io,
            tab,
            names,
            summary,
            view,
            thumbs,
            tie_class,
            particles,
            model_path,
            model_info,
            model_camera,
            model_autoplay,
            model_preview,
            ..
        } = self;
        let mut cx = Cx::with_particles(state, fgd, thumbs, particles.as_deref());
        let mut ops: Vec<Op> = Vec::new();
        let mut go_to = None;
        smart.begin_frame();
        io.buffers.begin_frame();
        smart.poll_picks(&mut cx, &mut ops);
        smart.name_prompt_ui(ui, &mut ops);

        if summary.entities.is_empty() && !summary.world_solids.is_empty() {
            world_brushes_ui(ui, &mut cx, summary, tie_class, tie_shortcut.as_deref(), &mut ops);
        } else {
            header(ui, &mut cx, view, move_shortcut.as_deref(), &mut ops);
            let world = view.target.is_world();
            if !world && !matches!(tab, EntityTab::Properties | EntityTab::Raw) && view.count() == 0 {
                *tab = EntityTab::Properties;
            }
            if world && !matches!(tab, EntityTab::Properties | EntityTab::Raw) {
                *tab = EntityTab::Properties;
            }
            if *tab == EntityTab::Model && model_path.is_none() {
                *tab = EntityTab::Properties;
            }
            let invalid = view.conns.iter().any(|c| c.issues.any());
            let mut tabs = vec![(EntityTab::Properties, t!("entities.tab_properties").to_owned(), false)];
            if !world {
                tabs.push((EntityTab::Flags, t!("entities.tab_flags").to_owned(), false));
                tabs.push((EntityTab::Outputs, tf!("entities.tab_outputs", view.conns.len()), invalid));
                tabs.push((EntityTab::Inputs, tf!("entities.tab_inputs", view.inputs.len()), false));
                if model_path.is_some() {
                    tabs.push((EntityTab::Model, t!("entities.tab_model").to_owned(), false));
                }
                tabs.push((EntityTab::Visgroups, t!("entities.tab_visgroups").to_owned(), false));
            }
            tabs.push((EntityTab::Raw, t!("entities.tab_raw").to_owned(), false));
            tab_bar(ui, tab, &tabs);
            ui.separator();

            if !world {
                egui::Panel::bottom(egui::Id::unique("hp_ui_entities.editor_info"))
                    .resizable(false)
                    .show_separator_line(true)
                    .frame(egui::Frame::NONE.inner_margin(egui::Margin::symmetric(0, 4)))
                    .show(ui, |ui| {
                        let objs = cx.doc.selected_objects();
                        crate::display_color::section(ui, cx.doc, cx.settings, cx.settings_dirty, &objs, &mut ops);
                        editor_info_ui(ui, smart, view);
                    });
            }
            match *tab {
                EntityTab::Properties => {
                    ScrollArea::vertical()
                        .id_salt("hp_ui_entities.props_scroll")
                        .auto_shrink([false, false])
                        .show(ui, |ui| smart.properties(ui, &mut cx, view, names, &mut ops));
                }
                EntityTab::Flags => {
                    ScrollArea::vertical()
                        .id_salt("hp_ui_entities.flags_scroll")
                        .auto_shrink([false, false])
                        .show(ui, |ui| smart.flags(ui, view, &mut ops));
                }
                EntityTab::Outputs => io.outputs(ui, &mut cx, view, names, &mut ops),
                EntityTab::Inputs => go_to = io.inputs(ui, view),
                EntityTab::Raw => {
                    ScrollArea::vertical()
                        .id_salt("hp_ui_entities.raw_scroll")
                        .auto_shrink([false, false])
                        .show(ui, |ui| smart.raw(ui, view, &mut ops));
                }
                EntityTab::Model => {
                    if let (Some(path), Some(info)) = (model_path.as_deref(), model_info.as_ref()) {
                        ScrollArea::vertical()
                            .id_salt("hp_ui_entities.model_tab_scroll")
                            .auto_shrink([false, false])
                            .show(ui, |ui| {
                                crate::widgets::fit_scroll_content(ui);
                                crate::panel_tabs::model_tab(
                                    ui,
                                    view,
                                    path,
                                    info,
                                    *model_preview,
                                    model_camera,
                                    model_autoplay,
                                    &mut ops,
                                );
                            });
                    }
                }
                EntityTab::Visgroups => crate::panel_tabs::visgroups_tab(ui, cx.doc, view, &mut ops),
            }
        }

        smart.end_frame(ui.ctx(), cx.doc.path(), &mut ops);
        io.buffers.end_frame(ui.ctx());
        for (action, text) in io.buffers.take_commits() {
            if let Some(op) = action.into_op(text, smart.update_references) {
                ops.push(op);
            }
        }
        if !ops.is_empty() {
            cx.apply_ops(ops);
        }
        if let Some(IoAction::GoTo(id)) = go_to {
            cx.doc.set_selection(&[ObjRef::Entity(id)]);
            cx.request(Action::Center2dOnSelection);
            cx.request(Action::Center3dOnSelection);
        }
    }

    fn refresh(&mut self, state: &AppState, fgd_override: Option<&ResolvedFgd>) {
        let doc = &state.doc;
        let rev = DocStamp::of(doc);
        let fgd = fgd_override.or_else(|| state.assets.fgd());
        let fgd_key = fgd.map_or(0, |f| f as *const ResolvedFgd as usize);
        if !self.names_pending && self.view_key.as_ref().is_some_and(|(r, f, _)| *r == rev && *f == fgd_key) {
            return;
        }
        let was_pending = self.names_pending;
        self.names_pending = self.names.refresh_at(doc, state.frame.time);
        if was_pending && !self.names_pending {
            self.view_key = None;
        }
        if self.summary_rev != Some(rev) {
            self.summary = SelectionSummary::of(doc);
            self.summary_rev = Some(rev);
        }
        let target = if !self.summary.entities.is_empty() {
            Target::Entities(self.summary.entities.clone())
        } else if self.summary.world_solids.is_empty() {
            Target::World
        } else {
            self.view_key = Some((rev, fgd_key, Target::Entities(Vec::new())));
            self.view = EntityView::empty();
            return;
        };
        if self.last_target.as_ref() != Some(&target) {
            self.io.reset_selection();
            self.last_target = Some(target.clone());
        }
        let key = (rev, fgd_key, target);
        if self.view_key.as_ref() != Some(&key) {
            self.view = EntityView::build(doc, key.2.clone(), fgd, &self.names);
            self.view_key = Some(key);
            self.wants_particles = view_has_kind(&self.view, &PropertyType::ParticleSystem);
            self.model_path = crate::panel_tabs::studio_model(&self.view);
            let wanted = thumb_names(&self.view);
            self.thumbs.retain(|k, _| wanted.contains(k));
            for name in wanted {
                self.thumbs.entry(name).or_insert(ThumbState::Loading);
            }
        }
    }
}

fn panel_menu(ui: &mut Ui, cx: &mut Cx, view: &EntityView) {
    let entry = |ui: &mut Ui, cx: &mut Cx, action: Action, enabled: bool| {
        let mut text = action.label().to_owned();
        if let Some(s) = cx.keys.shortcut(action) {
            text.push_str(&format!("  ({})", shortcut_label(&s)));
        }
        if ui.add_enabled(enabled, egui::Button::new(text)).clicked() {
            cx.request(action);
            ui.close();
        }
    };
    let entities = !view.target.is_world() && view.count() > 0;
    entry(ui, cx, Action::CopyKeyvalues, entities);
    let can_paste = entities && cx.runtime.entities.clipboard.is_some();
    entry(ui, cx, Action::PasteKeyvalues, can_paste);
    if let Some(clip) = &cx.runtime.entities.clipboard {
        ui.label(RichText::new(tf!("entities.kv_clip_info", clip.keys.len(), clip.class)).small().weak());
    }
    let mut with_name = cx.settings.entities.paste_targetname;
    if ui.checkbox(&mut with_name, t!("entities.kv_paste_name")).changed() {
        cx.settings.entities.paste_targetname = with_name;
        *cx.settings_dirty = true;
    }
    ui.separator();
    entry(ui, cx, Action::FindReplaceText, true);
    entry(ui, cx, Action::SetMaxDistance, entities);
    entry(ui, cx, Action::EntityHelp, view.single_class().is_some());
}

fn header(ui: &mut Ui, cx: &mut Cx, view: &EntityView, move_shortcut: Option<&str>, ops: &mut Vec<Op>) {
    ui.horizontal(|ui| {
        let glyph = match view.kind {
            EditKind::World => "◆",
            EditKind::Point => "●",
            EditKind::Brush => "■",
            EditKind::Mixed => "◫",
        };
        let color = view
            .single_class()
            .and_then(|c| cx.fgd?.class(c)?.color())
            .map_or(theme::accent(), |[r, g, b]| egui::Color32::from_rgb(r, g, b));
        ui.label(icon(glyph).size(16.0).color(color)).on_hover_text(kind_label(view.kind));
        if view.target.is_world() {
            ui.label(RichText::new("worldspawn").strong());
            ui.label(RichText::new(t!("entities.map_properties_hint")).weak());
            return;
        }
        let class_text = match view.single_class() {
            Some(c) => c.to_owned(),
            None => tf!("entities.n_classes", view.classes.len()),
        };
        let buttons_w = 2.0 * (crate::widgets::SIDE_BUTTON_W + ui.spacing().item_spacing.x);
        let width = (ui.available_width() - buttons_w).max(80.0);
        let btn = picker_button(ui, RichText::new(class_text).strong(), width).on_hover_text(t!("entities.class_tip"));
        let help = crate::widgets::side_button(ui, "?", false).on_hover_text(t!("action.entity_help"));
        if help.clicked() {
            cx.request(Action::EntityHelp);
        }
        crate::widgets::side_menu_button(ui, icon("☰"), |ui| panel_menu(ui, cx, view))
            .on_hover_text(t!("entities.panel_menu"));
        if picker_is_open(&btn) {
            let classes: Vec<&str> = match cx.fgd {
                Some(f) => match view.kind {
                    EditKind::Brush => f.solid_classes().iter().map(String::as_str).collect(),
                    EditKind::Point => f.point_classes().iter().map(String::as_str).collect(),
                    _ => {
                        let mut all: Vec<&str> =
                            f.point_classes().iter().chain(f.solid_classes()).map(String::as_str).collect();
                        all.sort_unstable_by(|a, b| crate::names::cmp_ignore_case(a, b));
                        all
                    }
                },
                None => Vec::new(),
            };
            let fgd = cx.fgd;
            let picked = picker_popup(&btn, true, classes.len(), |i| {
                let name = classes.get(i).copied().unwrap_or("");
                PickerItem { name, detail: fgd.and_then(|f| f.class(name)).map_or("", |c| c.description.as_str()) }
            });
            if let Some(class) = picked {
                ops.push(Op::ChangeClass { ids: view.target.ids().to_vec(), class });
            }
        }
    });
    if view.count() > 1 || matches!(view.kind, EditKind::Brush | EditKind::Mixed) {
        ui.horizontal(|ui| {
            if view.count() > 1 {
                ui.label(RichText::new(tf!("entities.n_selected", view.count())).weak());
            }
            if matches!(view.kind, EditKind::Brush | EditKind::Mixed) {
                ui.with_layout(egui::Layout::right_to_left(egui::Align::Center), |ui| {
                    let mut tip = t!("entities.move_to_world_tip").to_owned();
                    if let Some(s) = move_shortcut {
                        tip.push_str(&format!(" ({s})"));
                    }
                    if ui.small_button(t!("action.move_to_world")).on_hover_text(tip).clicked() {
                        ops.push(Op::MoveToWorld { ids: view.target.ids().to_vec() });
                    }
                });
            }
        });
    }
    match cx.fgd {
        None => {
            ui.label(RichText::new(t!("entities.no_fgd")).color(theme::warning()).small());
        }
        Some(fgd) if !view.classes_known && !view.classes.is_empty() && !view.target.is_world() => {
            if let Some(text) = unknown_classes_warning(fgd, &view.classes) {
                ui.add(egui::Label::new(RichText::new(text).color(theme::warning()).small()).wrap());
            }
        }
        Some(_) => {}
    }
    for n in fog_hints(cx, view) {
        let color = if n.warning { theme::warning() } else { ui.visuals().weak_text_color() };
        ui.add(egui::Label::new(RichText::new(n.text).color(color).small()).wrap());
    }
    if !view.description.trim().is_empty() {
        egui::CollapsingHeader::new(RichText::new(t!("entities.description")).small())
            .id_salt("hp_ui_entities.description")
            .default_open(false)
            .show(ui, |ui| {
                ui.label(RichText::new(view.description.trim()).small());
            });
    }
}

pub(crate) fn fog_hints(cx: &Cx, view: &EntityView) -> Vec<hp_app::runtime::fog::Note> {
    use hp_app::runtime::fog;
    let ([id], Some(class)) = (view.target.ids(), view.single_class()) else { return Vec::new() };
    if fog::is_fog_volume(class) {
        fog::resolve_volume(cx.doc, *id).map(|v| fog::volume_notes(&v)).unwrap_or_default()
    } else if fog::is_fog_controller(class) {
        fog::controller_notes(cx.doc, *id, cx.runtime.render.fog_controller, cx.settings.render.fog_preview)
    } else if hp_app::runtime::helpers::fish::is_fish_pool(class) {
        fish_hints(cx, *id)
    } else {
        Vec::new()
    }
}

fn fish_hints(cx: &Cx, id: hp_editor::Id) -> Vec<hp_app::runtime::fog::Note> {
    use hp_app::runtime::helpers::fish;
    let Some(pool) = cx.doc.scene().entity(id).and_then(|n| fish::fish_pool(&n.entity)) else { return Vec::new() };
    let models = cx.assets.models();
    let has = |m: &str| models.iter().any(|e| e.name.eq_ignore_ascii_case(m.trim()));
    if models.is_empty() || has(&pool.model) || !has(fish::FALLBACK_MODEL) {
        return Vec::new();
    }
    vec![hp_app::runtime::fog::Note {
        warning: true,
        text: tf!("fish.missing_model", pool.model, fish::FALLBACK_MODEL),
    }]
}

const MAX_LISTED_CLASSES: usize = 4;

pub(crate) fn unknown_classes_warning(fgd: &ResolvedFgd, classes: &[String]) -> Option<String> {
    let unknown: Vec<&str> = classes.iter().map(String::as_str).filter(|c| fgd.class(c).is_none()).collect();
    match (classes.len(), unknown.len()) {
        (_, 0) => None,
        (1, _) => Some(t!("entities.class_unknown").to_owned()),
        (_, 1) => Some(tf!("entities.class_unknown_named", unknown[0])),
        (_, n) => {
            let mut list = unknown.iter().take(MAX_LISTED_CLASSES).copied().collect::<Vec<_>>().join(", ");
            if n > MAX_LISTED_CLASSES {
                list.push_str(&tf!("entities.and_more", n - MAX_LISTED_CLASSES));
            }
            Some(tf!("entities.classes_unknown", n, list))
        }
    }
}

fn kind_label(kind: EditKind) -> &'static str {
    match kind {
        EditKind::World => t!("entities.kind_world"),
        EditKind::Point => t!("entities.kind_point"),
        EditKind::Brush => t!("entities.kind_brush"),
        EditKind::Mixed => t!("entities.kind_mixed"),
    }
}

fn editor_info_ui(ui: &mut Ui, smart: &mut SmartEdit, view: &EntityView) {
    let ids = view.target.ids();
    egui::CollapsingHeader::new(t!("entities.editor_section"))
        .id_salt("hp_ui_entities.editor_section")
        .default_open(false)
        .show(ui, |ui| {
            ui.label(t!("entities.comments"));
            let id = egui::Id::unique("hp_ui_entities.comments");
            let hint =
                if view.comments.is_mixed() { t!("common.multiple_values") } else { t!("entities.comments_hint") };
            let opts = FieldOpts { hint, multiline: true, ..FieldOpts::default() };
            smart.buffers.field(ui, id, view.comments.text(), opts, || CommitAction::Comments { ids: ids.to_vec() });
        });
}

fn world_brushes_ui(
    ui: &mut Ui,
    cx: &mut Cx,
    summary: &SelectionSummary,
    tie_class: &mut String,
    shortcut: Option<&str>,
    ops: &mut Vec<Op>,
) {
    let objs: Vec<ObjRef> = summary.world_solids.iter().map(|id| ObjRef::Solid(*id)).collect();
    ui.add_space(8.0);
    ui.vertical_centered(|ui| {
        ui.label(icon("■").size(28.0).color(ui.visuals().weak_text_color()));
        ui.label(RichText::new(tf!("entities.world_brushes", summary.world_solids.len())).strong());
        ui.label(RichText::new(t!("entities.world_brushes_hint")).weak());
    });
    ui.add_space(8.0);
    let row = vec2(ui.available_width(), ui.spacing().interact_size.y);
    ui.allocate_ui_with_layout(row, egui::Layout::right_to_left(egui::Align::Center), |ui| {
        let mut tip = t!("entities.tie_tip").to_owned();
        if let Some(s) = shortcut {
            tip.push_str(&format!(" ({s})"));
        }
        let tie = ui.button(t!("action.tie_to_entity"));
        if tie.on_hover_text(tip).clicked() && !tie_class.trim().is_empty() {
            ops.push(Op::Tie { objs: objs.clone(), class: tie_class.trim().to_owned() });
        }
        let picker_w = ui.available_width().max(40.0);
        let btn = picker_button(ui, tie_class.as_str(), picker_w).on_hover_text(t!("entities.tie_class_tip"));
        if picker_is_open(&btn) {
            let classes = tie_classes(cx.fgd);
            let fgd = cx.fgd;
            let picked = picker_popup(&btn, true, classes.len(), |i| {
                let name = classes.get(i).copied().unwrap_or("");
                PickerItem { name, detail: fgd.and_then(|f| f.class(name)).map_or("", |c| c.description.as_str()) }
            });
            if let Some(c) = picked {
                *tie_class = c;
            }
        }
    });
    ui.add_space(4.0);
    ui.label(RichText::new(t!("entities.quick_tie")).small().weak());
    ui.horizontal_wrapped(|ui| {
        for class in QUICK_TIE {
            if cx.fgd.is_some_and(|f| !f.contains(class)) {
                continue;
            }
            let desc = cx.fgd.and_then(|f| f.class(class)).map_or("", |c| c.description.as_str());
            if ui.small_button(class).on_hover_text(desc).clicked() {
                ops.push(Op::Tie { objs: objs.clone(), class: class.to_owned() });
            }
        }
    });
    ui.add_space(8.0);
    let selected = cx.doc.selected_objects();
    crate::display_color::section(ui, cx.doc, cx.settings, cx.settings_dirty, &selected, ops);
}

pub(crate) fn tie_classes(fgd: Option<&ResolvedFgd>) -> Vec<&str> {
    let mut out: Vec<&str> = vec!["func_detail"];
    if let Some(f) = fgd {
        out.extend(
            f.solid_classes()
                .iter()
                .map(String::as_str)
                .filter(|c| !c.eq_ignore_ascii_case("func_detail") && !c.eq_ignore_ascii_case("worldspawn")),
        );
    }
    out
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::test_fgd::test_fgd;

    #[test]
    fn tie_classes_put_func_detail_first() {
        let fgd = test_fgd();
        let list = tie_classes(Some(&fgd));
        assert_eq!(list[0], "func_detail");
        assert!(list.contains(&"trigger_multiple"));
        assert!(!list.contains(&"worldspawn"));
        assert_eq!(list.iter().filter(|c| **c == "func_detail").count(), 1);
        assert_eq!(tie_classes(None), ["func_detail"]);
    }

    #[test]
    fn unknown_class_warnings() {
        let _lang = hp_app::testing::lang_lock();
        crate::register_translations();
        let fgd = test_fgd();
        let names = |list: &[&str]| list.iter().map(|c| (*c).to_owned()).collect::<Vec<_>>();
        assert_eq!(unknown_classes_warning(&fgd, &names(&["light", "trigger_multiple"])), None);
        let one = unknown_classes_warning(&fgd, &names(&["ttt_thing"])).unwrap();
        assert_eq!(one, t!("entities.class_unknown"));
        let many = unknown_classes_warning(&fgd, &names(&["light", "ttt_a", "ttt_b"])).unwrap();
        assert!(many.starts_with('2') && many.contains("ttt_a, ttt_b"), "{many}");
        let named = unknown_classes_warning(&fgd, &names(&["light", "ttt_a"])).unwrap();
        assert!(named.contains("ttt_a") && named != one, "{named}");
        let list: Vec<String> = (0..7).map(|i| format!("custom_{i}")).collect();
        let long = unknown_classes_warning(&fgd, &list).unwrap();
        assert!(long.starts_with('7') && long.contains("custom_3") && !long.contains("custom_4"), "{long}");
    }
}
