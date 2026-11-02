use std::collections::{BTreeSet, HashMap, HashSet};
use std::time::{Duration, Instant};

use egui::{Id, Key, Modifiers, RichText, ScrollArea, Ui};
use glam::DVec3;
use hp_app::state::ThumbState;
use hp_app::{Action, AppState, Severity, t, tf, theme};
use hp_editor::{Document, EntityQuery, Id as EntityId, ObjRef};
use hp_fgd::ResolvedFgd;

use crate::cx::Cx;
use crate::edit::Op;
use crate::names::MapNames;
use crate::smartedit::{SmartEdit, thumb_names};
use crate::view::{DocStamp, EntityView, Target};
use crate::widgets::{ROW_HEIGHT, SortBy, column_widths, empty_note, tab_bar, table_header, table_row};

#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash)]
pub enum FindMode {
    Auto,
    Name,
    Class,
    KeyValue,
}

impl FindMode {
    const ALL: [FindMode; 4] = [FindMode::Auto, FindMode::Name, FindMode::Class, FindMode::KeyValue];

    fn label(self) -> &'static str {
        match self {
            FindMode::Auto => t!("entities.find_auto"),
            FindMode::Name => t!("entities.find_name"),
            FindMode::Class => t!("entities.find_class"),
            FindMode::KeyValue => t!("entities.find_key_value"),
        }
    }
}

fn pattern(text: &str, exact: bool) -> String {
    let t = text.trim();
    if exact || t.contains(['*', '?']) { t.to_owned() } else { format!("*{t}*") }
}

pub(crate) fn find_queries(text: &str, mode: FindMode, exact: bool, visible_only: bool) -> Vec<EntityQuery> {
    let text = text.trim();
    if text.is_empty() {
        return Vec::new();
    }
    let base = EntityQuery { include_hidden: !visible_only, ..EntityQuery::default() };
    let mode = if mode == FindMode::Auto && text.contains('=') { FindMode::KeyValue } else { mode };
    match mode {
        FindMode::Auto => vec![
            EntityQuery { targetname: Some(pattern(text, exact)), ..base.clone() },
            EntityQuery { classname: Some(pattern(text, exact)), ..base },
        ],
        FindMode::Name => vec![EntityQuery { targetname: Some(pattern(text, exact)), ..base }],
        FindMode::Class => vec![EntityQuery { classname: Some(pattern(text, exact)), ..base }],
        FindMode::KeyValue => {
            let (k, v) = text.split_once('=').unwrap_or((text, ""));
            let key = (!k.trim().is_empty()).then(|| pattern(k, exact));
            let value = (!v.trim().is_empty()).then(|| pattern(v, exact));
            if key.is_none() && value.is_none() {
                return Vec::new();
            }
            vec![EntityQuery { key, value, ..base }]
        }
    }
}

pub(crate) fn run_find(doc: &Document, queries: &[EntityQuery]) -> Vec<EntityId> {
    match queries {
        [] => Vec::new(),
        [q] => doc.find_entities(q),
        many => {
            let found: HashSet<EntityId> = many.iter().flat_map(|q| doc.find_entities(q)).collect();
            doc.scene().entities().keys().copied().filter(|id| found.contains(id)).collect()
        }
    }
}

struct FindState {
    query: String,
    mode: FindMode,
    visible_only: bool,
    exact: bool,
    results: Vec<EntityId>,
    key: Option<(DocStamp, String, FindMode, bool, bool)>,
    highlight: Option<usize>,
    focus: bool,
    was_open: bool,
    built_at: Option<Instant>,
}

impl Default for FindState {
    fn default() -> Self {
        Self {
            query: String::new(),
            mode: FindMode::Auto,
            visible_only: true,
            exact: false,
            results: Vec::new(),
            key: None,
            highlight: None,
            focus: true,
            was_open: false,
            built_at: None,
        }
    }
}

#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash)]
pub enum KindFilter {
    All,
    Point,
    Brush,
}

#[derive(Clone, Debug, PartialEq)]
pub(crate) struct ReportRow {
    pub id: EntityId,
    pub class: String,
    pub name: String,
    pub position: Option<DVec3>,
    pub origin: String,
    pub brush: bool,
    pub visible: bool,
    pub details: String,
}

pub(crate) fn entity_details(e: &hp_vmf::Entity) -> &str {
    ["model", "material", "texture", "file", "effect_name"]
        .iter()
        .find_map(|k| e.get(k).map(str::trim).filter(|v| !v.is_empty()))
        .unwrap_or("")
}

fn key_value_matches(word: &str, e: &hp_vmf::Entity) -> bool {
    let (k, v) = word.split_once('=').unwrap_or((word, ""));
    let (k, v) = (pattern(k, false), pattern(v, false));
    let key_ok = |pk: &str| k == "**" || hp_editor::glob_match(&k, pk);
    let value_ok = |pv: &str| v == "**" || hp_editor::glob_match(&v, pv);
    (key_ok("classname") && value_ok(&e.classname)) || e.props.iter().any(|(pk, pv)| key_ok(pk) && value_ok(pv))
}

pub(crate) fn display_origin(doc: &Document, id: EntityId) -> Option<DVec3> {
    let scene = doc.scene();
    if scene.is_brush_entity(id)
        && let Some(b) = scene.entity_solid_bounds(id)
    {
        return Some(b.center());
    }
    scene.entity(id)?.entity.origin()
}

fn cmp_positions(a: Option<DVec3>, b: Option<DVec3>) -> std::cmp::Ordering {
    match (a, b) {
        (Some(a), Some(b)) => a.x.total_cmp(&b.x).then(a.y.total_cmp(&b.y)).then(a.z.total_cmp(&b.z)),
        (Some(_), None) => std::cmp::Ordering::Less,
        (None, Some(_)) => std::cmp::Ordering::Greater,
        (None, None) => std::cmp::Ordering::Equal,
    }
}

pub(crate) fn report_rows(
    doc: &Document,
    class_filter: &str,
    kind: KindFilter,
    hide_invisible: bool,
    sort: SortBy,
) -> Vec<ReportRow> {
    let scene = doc.scene();
    let words: Vec<String> = class_filter.split_whitespace().map(str::to_ascii_lowercase).collect();
    let mut rows: Vec<ReportRow> = scene
        .entities()
        .values()
        .filter_map(|n| {
            let e = &n.entity;
            let brush = scene.is_brush_entity(e.id);
            match kind {
                KindFilter::Point if brush => return None,
                KindFilter::Brush if !brush => return None,
                _ => {}
            }
            let visible = doc.is_entity_visible(e.id);
            if hide_invisible && !visible {
                return None;
            }
            let name = e.get("targetname").unwrap_or("");
            let details = entity_details(e);
            if !words.is_empty() {
                let class = e.classname.to_ascii_lowercase();
                let lname = name.to_ascii_lowercase();
                let ldetails = details.to_ascii_lowercase();
                let ok = words.iter().all(|w| {
                    if w.contains('=') {
                        key_value_matches(w, e)
                    } else {
                        class.contains(w.as_str()) || lname.contains(w.as_str()) || ldetails.contains(w.as_str())
                    }
                });
                if !ok {
                    return None;
                }
            }
            let position = display_origin(doc, e.id);
            let origin = position.map(hp_app::widgets::fmt_vec3).unwrap_or_default();
            Some(ReportRow {
                id: e.id,
                class: e.classname.clone(),
                name: name.to_owned(),
                position,
                origin,
                brush,
                visible,
                details: details.to_owned(),
            })
        })
        .collect();
    let cmp = |a: &ReportRow, b: &ReportRow| match sort.column {
        0 => a.class.to_ascii_lowercase().cmp(&b.class.to_ascii_lowercase()),
        1 => a.name.to_ascii_lowercase().cmp(&b.name.to_ascii_lowercase()),
        2 => a.details.to_ascii_lowercase().cmp(&b.details.to_ascii_lowercase()),
        3 => cmp_positions(a.position, b.position),
        4 => a.brush.cmp(&b.brush),
        5 => a.visible.cmp(&b.visible),
        _ => a.id.cmp(&b.id),
    };
    rows.sort_by(|a, b| {
        let o = cmp(a, b).then(a.id.cmp(&b.id));
        if sort.ascending { o } else { o.reverse() }
    });
    rows
}

struct ReportState {
    class_filter: String,
    kind: KindFilter,
    hide_invisible: bool,
    sort: SortBy,
    rows: Vec<ReportRow>,
    key: Option<(DocStamp, String, KindFilter, bool, SortBy)>,
    selected: BTreeSet<EntityId>,
    anchor: Option<usize>,
    built_at: Option<Instant>,
}

impl Default for ReportState {
    fn default() -> Self {
        Self {
            class_filter: String::new(),
            kind: KindFilter::All,
            hide_invisible: false,
            sort: SortBy { column: 0, ascending: true },
            rows: Vec::new(),
            key: None,
            selected: BTreeSet::new(),
            anchor: None,
            built_at: None,
        }
    }
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
enum MapTab {
    Properties,
    Raw,
}

struct MapPropsState {
    smart: SmartEdit,
    tab: MapTab,
    names: MapNames,
    view: EntityView,
    view_key: Option<(DocStamp, usize)>,
    thumbs: HashMap<String, ThumbState>,
}

impl Default for MapPropsState {
    fn default() -> Self {
        Self {
            smart: SmartEdit::new("map_properties"),
            tab: MapTab::Properties,
            names: MapNames::default(),
            view: EntityView::empty(),
            view_key: None,
            thumbs: HashMap::new(),
        }
    }
}

#[derive(Default)]
pub struct EntityDialogs {
    find: FindState,
    report: ReportState,
    map: MapPropsState,
    find_replace: crate::find_replace::FindReplace,
    help: crate::help::HelpWindow,
    color: crate::color::ColorWindow,
    display_color: crate::display_color::DisplayColorWindow,
    max_distance: crate::max_distance::MaxDistanceWindow,
    soundscapes: crate::soundscapes::SoundscapeEditor,
    sounds: crate::sound_browser::SoundBrowser,
    particles: crate::particles::ParticleBrowser,
    text_focus_before: bool,
}

impl EntityDialogs {
    pub fn new() -> Self {
        Self::default()
    }

    pub fn set_find_query(&mut self, text: &str) {
        text.clone_into(&mut self.find.query);
        self.find.key = None;
    }

    pub fn find_result_count(&self) -> usize {
        self.find.results.len()
    }

    pub fn report_row_count(&self) -> usize {
        self.report.rows.len()
    }

    pub fn sound_browser(&mut self) -> &mut crate::sound_browser::SoundBrowser {
        &mut self.sounds
    }

    pub fn particle_browser(&mut self) -> &mut crate::particles::ParticleBrowser {
        &mut self.particles
    }

    pub fn set_find_replace(&mut self, find: &str, replace: &str) {
        self.find_replace.set_find(find, replace);
    }

    pub fn show(&mut self, ctx: &egui::Context, state: &mut AppState) {
        self.show_with_fgd(ctx, state, None);
    }

    pub fn show_with_fgd(&mut self, ctx: &egui::Context, state: &mut AppState, fgd: Option<&ResolvedFgd>) {
        if state.ui.show_find_entities {
            let mut open = true;
            let resp = hp_app::chrome::Window::new(t!("entities.find_title"))
                .id(Id::unique("hp_ui_entities.find_window"))
                .open(&mut open)
                .default_size([420.0, 420.0])
                .min_width(300.0)
                .resizable(true)
                .collapsible(false)
                .show(ctx, |ui| self.find_ui(ui, state));
            if escape_closes(ctx, resp.as_ref().map(|r| r.response.layer_id), false) {
                open = false;
            }
            state.ui.show_find_entities = open;
            self.find.was_open = open;
        } else {
            self.find.was_open = false;
        }

        if state.ui.show_entity_report {
            let mut open = true;
            let resp = hp_app::chrome::Window::new(t!("action.entity_report"))
                .id(Id::unique("hp_ui_entities.report_window"))
                .open(&mut open)
                .default_size([560.0, 480.0])
                .min_width(360.0)
                .resizable(true)
                .collapsible(false)
                .show(ctx, |ui| self.report_ui(ui, state));
            if escape_closes(ctx, resp.as_ref().map(|r| r.response.layer_id), self.text_focus_before) {
                open = false;
            }
            state.ui.show_entity_report = open;
        }

        if state.ui.show_map_properties {
            let mut open = true;
            let resp = hp_app::chrome::Window::new(t!("entities.map_properties_title"))
                .id(Id::unique("hp_ui_entities.map_properties_window"))
                .open(&mut open)
                .default_size([440.0, 460.0])
                .min_width(320.0)
                .resizable(true)
                .collapsible(false)
                .show(ctx, |ui| self.map_ui(ui, state, fgd));
            if escape_closes(ctx, resp.as_ref().map(|r| r.response.layer_id), self.text_focus_before) {
                open = false;
            }
            state.ui.show_map_properties = open;
        }

        self.find_replace.show(ctx, state, self.text_focus_before);
        self.help.show(ctx, state, fgd, self.text_focus_before);
        self.color.show(ctx, state, fgd);
        self.display_color.show(ctx, state);
        self.max_distance.show(ctx, state, fgd, self.text_focus_before);
        self.soundscapes.show(ctx, state);
        if state.runtime.entities.sound_window {
            let mut open = true;
            hp_app::chrome::Window::new(t!("action.show_sound_browser"))
                .id(Id::unique("hp_ui_entities.sound_window"))
                .open(&mut open)
                .default_size([420.0, 560.0])
                .resizable(true)
                .collapsible(true)
                .show(ctx, |ui| self.sounds.ui(ui, state));
            if !open {
                state.runtime.entities.sound_window = false;
            }
        }
        if state.runtime.entities.particle_window {
            let mut open = true;
            hp_app::chrome::Window::new(t!("action.show_particle_browser"))
                .id(Id::unique("hp_ui_entities.particle_window"))
                .open(&mut open)
                .default_size([480.0, 600.0])
                .resizable(true)
                .collapsible(true)
                .show(ctx, |ui| self.particles.ui(ui, state));
            if !open {
                state.runtime.entities.particle_window = false;
            }
        }
        self.text_focus_before = ctx.text_edit_focused();
    }

    fn find_ui(&mut self, ui: &mut Ui, state: &mut AppState) {
        let f = &mut self.find;
        if !f.was_open {
            f.focus = true;
        }
        ui.horizontal(|ui| {
            egui::ComboBox::from_id_salt("hp_ui_entities.find_mode")
                .width(110.0)
                .selected_text(f.mode.label())
                .show_ui(ui, |ui| {
                    for m in FindMode::ALL {
                        ui.selectable_value(&mut f.mode, m, m.label());
                    }
                });
            let edit = ui.add(
                egui::TextEdit::singleline(&mut f.query)
                    .hint_text(t!("entities.find_hint"))
                    .desired_width(f32::INFINITY),
            );
            if f.focus {
                edit.request_focus();
                f.focus = false;
            }
            if edit.lost_focus()
                && ui.input(|i| i.key_pressed(Key::Enter))
                && let Some(&id) = f.highlight.and_then(|h| f.results.get(h)).or(f.results.first())
            {
                go_to(state, &[id]);
            }
        });
        ui.horizontal(|ui| {
            ui.checkbox(&mut f.visible_only, t!("entities.visible_only"));
            ui.checkbox(&mut f.exact, t!("entities.exact_match")).on_hover_text(t!("entities.exact_match_tip"));
        });
        let key = (DocStamp::of(&state.doc), f.query.clone(), f.mode, f.exact, f.visible_only);
        if f.key.as_ref() != Some(&key) {
            let only_doc_changed =
                f.key.as_ref().is_some_and(|k| (&k.1, k.2, k.3, k.4) == (&key.1, key.2, key.3, key.4));
            if only_doc_changed && throttled(ui.ctx(), f.built_at, state.doc.scene().entities().len()) {
            } else {
                let queries = find_queries(&f.query, f.mode, f.exact, f.visible_only);
                f.results = run_find(&state.doc, &queries);
                f.key = Some(key);
                f.built_at = Some(Instant::now());
                f.highlight = f.highlight.filter(|h| *h < f.results.len());
            }
        }
        let n = f.results.len();
        ui.horizontal(|ui| {
            let label = if f.query.trim().is_empty() {
                t!("entities.find_empty").to_owned()
            } else {
                tf!("entities.find_results", n)
            };
            ui.label(RichText::new(label).weak());
            ui.with_layout(egui::Layout::right_to_left(egui::Align::Center), |ui| {
                if ui.add_enabled(n > 0, egui::Button::new(t!("entities.select_all_results"))).clicked() {
                    let objs: Vec<ObjRef> = f.results.iter().map(|id| ObjRef::Entity(*id)).collect();
                    state.doc.set_selection(&objs);
                }
            });
        });
        let widths = column_widths(ui.available_width(), &[1.2, 1.2, 1.0], 60.0);
        table_header(ui, &[t!("entities.col_class"), t!("common.name"), t!("entities.col_origin")], &widths, None);
        if n == 0 {
            empty_note(ui, t!("entities.no_match"));
            return;
        }
        let mut activate = None;
        ScrollArea::vertical().id_salt("hp_ui_entities.find_results").auto_shrink([false, false]).show_rows(
            ui,
            ROW_HEIGHT,
            n,
            |ui, range| {
                let scene = state.doc.scene();
                for i in range {
                    let id = f.results[i];
                    let Some(node) = scene.entity(id) else { continue };
                    let e = &node.entity;
                    let origin = display_origin(&state.doc, id).map(hp_app::widgets::fmt_vec3).unwrap_or_default();
                    let cells = [
                        (e.classname.as_str(), None),
                        (e.get("targetname").unwrap_or(""), None),
                        (origin.as_str(), None),
                    ];
                    let resp = table_row(ui, &cells, &widths, f.highlight == Some(i), i % 2 == 1);
                    if resp.clicked() {
                        f.highlight = Some(i);
                    }
                    if resp.double_clicked() {
                        activate = Some(id);
                    }
                }
            },
        );
        if let Some(id) = activate {
            go_to(state, &[id]);
        }
    }

    fn report_ui(&mut self, ui: &mut Ui, state: &mut AppState) {
        let r = &mut self.report;
        ui.horizontal(|ui| {
            ui.add(
                egui::TextEdit::singleline(&mut r.class_filter)
                    .hint_text(t!("entities.report_filter"))
                    .desired_width(180.0),
            );
            egui::ComboBox::from_id_salt("hp_ui_entities.report_kind")
                .width(90.0)
                .selected_text(kind_label(r.kind))
                .show_ui(ui, |ui| {
                    for k in [KindFilter::All, KindFilter::Point, KindFilter::Brush] {
                        ui.selectable_value(&mut r.kind, k, kind_label(k));
                    }
                });
            ui.checkbox(&mut r.hide_invisible, t!("entities.hide_invisible"));
        });
        let key = (DocStamp::of(&state.doc), r.class_filter.clone(), r.kind, r.hide_invisible, r.sort);
        if r.key.as_ref() != Some(&key) {
            let only_doc_changed =
                r.key.as_ref().is_some_and(|k| (&k.1, k.2, k.3, k.4) == (&key.1, key.2, key.3, key.4));
            if only_doc_changed && throttled(ui.ctx(), r.built_at, state.doc.scene().entities().len()) {
            } else {
                r.rows = report_rows(&state.doc, &r.class_filter, r.kind, r.hide_invisible, r.sort);
                let present: HashSet<EntityId> = r.rows.iter().map(|row| row.id).collect();
                r.selected.retain(|id| present.contains(id));
                r.key = Some(key);
                r.built_at = Some(Instant::now());
            }
        }
        let n = r.rows.len();
        let sel: Vec<EntityId> = r.rows.iter().filter(|row| r.selected.contains(&row.id)).map(|row| row.id).collect();
        ui.horizontal(|ui| {
            ui.label(RichText::new(tf!("entities.report_count", n, state.doc.scene().entities().len())).weak());
            ui.with_layout(egui::Layout::right_to_left(egui::Align::Center), |ui| {
                if ui.add_enabled(!sel.is_empty(), egui::Button::new(t!("action.delete"))).clicked() {
                    let objs: Vec<ObjRef> = sel.iter().map(|id| ObjRef::Entity(*id)).collect();
                    if let Err(e) = state.doc.delete(&objs) {
                        state.notify(Severity::Error, tf!("entities.edit_failed", e));
                    }
                }
                if ui.add_enabled(!sel.is_empty(), egui::Button::new(t!("entities.go_to"))).clicked() {
                    go_to(state, &sel);
                }
                if ui.add_enabled(!sel.is_empty(), egui::Button::new(t!("entities.select"))).clicked() {
                    let objs: Vec<ObjRef> = sel.iter().map(|id| ObjRef::Entity(*id)).collect();
                    state.doc.set_selection(&objs);
                }
            });
        });
        let widths = column_widths(ui.available_width(), &[1.2, 1.0, 1.4, 1.0, 0.45, 0.45], 40.0);
        let headers = [
            t!("entities.col_class"),
            t!("common.name"),
            t!("entities.col_details"),
            t!("entities.col_origin"),
            t!("entities.col_kind"),
            t!("entities.col_visible"),
        ];
        table_header(ui, &headers, &widths, Some(&mut r.sort));
        if n == 0 {
            empty_note(ui, t!("entities.no_match"));
            return;
        }
        let mut activate = None;
        ScrollArea::vertical().id_salt("hp_ui_entities.report_rows").auto_shrink([false, false]).show_rows(
            ui,
            ROW_HEIGHT,
            n,
            |ui, range| {
                for i in range {
                    let row = &r.rows[i];
                    let kind = if row.brush { t!("entities.kind_brush") } else { t!("entities.kind_point") };
                    let visible = if row.visible { t!("common.yes") } else { t!("common.no") };
                    let weak = (!row.visible).then(|| ui.visuals().weak_text_color());
                    let cells = [
                        (row.class.as_str(), weak),
                        (row.name.as_str(), weak),
                        (row.details.as_str(), weak),
                        (row.origin.as_str(), weak),
                        (kind, weak),
                        (visible, if row.visible { None } else { Some(theme::warning()) }),
                    ];
                    let row_id = row.id;
                    let resp = table_row(ui, &cells, &widths, r.selected.contains(&row_id), i % 2 == 1);
                    let resp = if row.details.is_empty() { resp } else { resp.on_hover_text(&row.details) };
                    if resp.clicked() {
                        let mods = ui.input(|inp| inp.modifiers);
                        select_report_row(r, i, mods);
                    }
                    if resp.double_clicked() {
                        activate = Some(row_id);
                    }
                }
            },
        );
        if let Some(id) = activate {
            go_to(state, &[id]);
        }
    }

    fn map_ui(&mut self, ui: &mut Ui, state: &mut AppState, fgd_override: Option<&ResolvedFgd>) {
        let m = &mut self.map;
        {
            let fgd = fgd_override.or_else(|| state.assets.fgd());
            m.names.refresh(&state.doc);
            let key = (DocStamp::of(&state.doc), fgd.map_or(0, |f| f as *const ResolvedFgd as usize));
            if m.view_key != Some(key) {
                m.view = EntityView::build(&state.doc, Target::World, fgd, &m.names);
                m.view_key = Some(key);
                let wanted = thumb_names(&m.view);
                m.thumbs.retain(|k, _| wanted.contains(k));
                for name in wanted {
                    m.thumbs.entry(name).or_insert(ThumbState::Loading);
                }
            }
        }
        for (name, thumb) in m.thumbs.iter_mut() {
            *thumb = state.assets.thumbnail(ui.ctx(), name);
        }
        let MapPropsState { smart, tab, names, view, thumbs, .. } = m;
        let mut cx = Cx::new(state, fgd_override, thumbs);
        let mut ops: Vec<Op> = Vec::new();
        smart.begin_frame();
        smart.poll_picks(&mut cx, &mut ops);
        smart.name_prompt_ui(ui, &mut ops);
        tab_bar(
            ui,
            tab,
            &[
                (MapTab::Properties, t!("entities.tab_properties").to_owned(), false),
                (MapTab::Raw, t!("entities.tab_raw").to_owned(), false),
            ],
        );
        ui.separator();
        ScrollArea::vertical().id_salt("hp_ui_entities.map_props_scroll").auto_shrink([false, false]).show(ui, |ui| {
            match tab {
                MapTab::Properties => smart.properties(ui, &mut cx, view, names, &mut ops),
                MapTab::Raw => smart.raw(ui, view, &mut ops),
            }
        });
        smart.end_frame(ui.ctx(), cx.doc.path(), &mut ops);
        if !ops.is_empty() {
            cx.apply_ops(ops);
        }
    }
}

const THROTTLE_ENTITIES: usize = 3000;
const THROTTLE: Duration = Duration::from_millis(250);

pub(crate) fn throttled(ctx: &egui::Context, built_at: Option<Instant>, entities: usize) -> bool {
    match built_at {
        Some(t) if entities > THROTTLE_ENTITIES && t.elapsed() < THROTTLE => {
            ctx.request_repaint_after(THROTTLE.saturating_sub(t.elapsed()));
            true
        }
        _ => false,
    }
}

fn kind_label(k: KindFilter) -> &'static str {
    match k {
        KindFilter::All => t!("common.all"),
        KindFilter::Point => t!("entities.kind_point"),
        KindFilter::Brush => t!("entities.kind_brush"),
    }
}

fn select_report_row(r: &mut ReportState, i: usize, mods: egui::Modifiers) {
    let Some(id) = r.rows.get(i).map(|row| row.id) else { return };
    if mods.shift
        && let Some(a) = r.anchor
    {
        let (lo, hi) = if a <= i { (a, i) } else { (i, a) };
        if !mods.command {
            r.selected.clear();
        }
        for row in r.rows.iter().take(hi + 1).skip(lo) {
            r.selected.insert(row.id);
        }
    } else if mods.command {
        if !r.selected.remove(&id) {
            r.selected.insert(id);
        }
        r.anchor = Some(i);
    } else {
        r.selected.clear();
        r.selected.insert(id);
        r.anchor = Some(i);
    }
}

fn go_to(state: &mut AppState, ids: &[EntityId]) {
    let objs: Vec<ObjRef> = ids.iter().map(|id| ObjRef::Entity(*id)).collect();
    state.doc.set_selection(&objs);
    state.request(Action::Center2dOnSelection);
    state.request(Action::Center3dOnSelection);
}

pub(crate) fn escape_closes(ctx: &egui::Context, layer: Option<egui::LayerId>, text_focus_before: bool) -> bool {
    let Some(layer) = layer else { return false };
    if text_focus_before || ctx.text_edit_focused() || egui::Popup::is_any_open(ctx) {
        return false;
    }
    let top = ctx.memory(|m| m.areas().top_layer_id(egui::Order::Middle));
    top == Some(layer) && ctx.input_mut(|i| i.consume_key(Modifiers::NONE, Key::Escape))
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn find_query_building() {
        assert!(find_queries("  ", FindMode::Auto, false, true).is_empty());
        let q = find_queries("light", FindMode::Auto, false, true);
        assert_eq!(q.len(), 2);
        assert_eq!(q[0].targetname.as_deref(), Some("*light*"));
        assert_eq!(q[1].classname.as_deref(), Some("*light*"));
        assert!(!q[0].include_hidden);
        let q = find_queries("light", FindMode::Class, true, false);
        assert_eq!(q[0].classname.as_deref(), Some("light"));
        assert!(q[0].include_hidden);
        let q = find_queries("main_*", FindMode::Name, false, true);
        assert_eq!(q[0].targetname.as_deref(), Some("main_*"));
        let q = find_queries("wait=1", FindMode::Auto, true, true);
        assert_eq!(q.len(), 1);
        assert_eq!(q[0].key.as_deref(), Some("wait"));
        assert_eq!(q[0].value.as_deref(), Some("1"));
        let q = find_queries("=main*", FindMode::KeyValue, false, true);
        assert_eq!(q[0].key, None);
        assert_eq!(q[0].value.as_deref(), Some("main*"));
        assert!(find_queries("=", FindMode::KeyValue, false, true).is_empty());
    }

    #[test]
    fn find_runs_on_test_map() {
        let state = hp_app::testing::test_state();
        let doc = &state.doc;
        assert_eq!(run_find(doc, &find_queries("light", FindMode::Auto, false, true)), [5]);
        assert_eq!(run_find(doc, &find_queries("trigger", FindMode::Class, false, true)), [6]);
        assert_eq!(run_find(doc, &find_queries("wait=1", FindMode::Auto, true, true)), [6]);
        assert_eq!(run_find(doc, &find_queries("_light=255*", FindMode::KeyValue, false, true)), [5]);
        assert!(run_find(doc, &find_queries("zzz", FindMode::Auto, false, true)).is_empty());
        let mut doc = state.doc;
        doc.set_entity_key(&[4], "targetname", "trigger_spawn").unwrap();
        assert_eq!(run_find(&doc, &find_queries("trigger", FindMode::Auto, false, true)), [4, 6]);
    }

    #[test]
    fn report_rows_filter_and_sort() {
        let mut state = hp_app::testing::test_state();
        let doc = &mut state.doc;
        let by_class = SortBy { column: 0, ascending: true };
        let rows = report_rows(doc, "", KindFilter::All, false, by_class);
        let classes: Vec<&str> = rows.iter().map(|r| r.class.as_str()).collect();
        assert_eq!(classes, ["info_player_start", "light", "trigger_multiple"]);
        assert!(rows[2].brush);
        assert_eq!(rows[1].origin, "0 0 96");
        assert_eq!(rows[2].origin, "96 32 32", "brush entities report their center");
        let desc = report_rows(doc, "", KindFilter::All, false, SortBy { column: 0, ascending: false });
        assert_eq!(desc[0].class, "trigger_multiple");
        assert_eq!(report_rows(doc, "", KindFilter::Brush, false, by_class).len(), 1);
        assert_eq!(report_rows(doc, "", KindFilter::Point, false, by_class).len(), 2);
        assert_eq!(report_rows(doc, "main", KindFilter::All, false, by_class).len(), 1, "names match too");
        doc.set_hidden(&[ObjRef::Entity(5)], true);
        assert_eq!(report_rows(doc, "", KindFilter::All, true, by_class).len(), 2);
        let all = report_rows(doc, "", KindFilter::All, false, SortBy { column: 5, ascending: true });
        assert!(!all[0].visible);
    }

    #[test]
    fn report_details_and_key_value_filter() {
        let mut state = hp_app::testing::test_state();
        let doc = &mut state.doc;
        let model = [("model".to_owned(), "models/props_c17/oildrum001.mdl".to_owned())];
        let prop = doc.create_point_entity("prop_static", DVec3::ZERO, &model).unwrap();
        let by_class = SortBy { column: 0, ascending: true };
        let rows = report_rows(doc, "oildrum", KindFilter::All, false, by_class);
        assert_eq!(rows.len(), 1, "details are searched");
        assert_eq!(rows[0].details, "models/props_c17/oildrum001.mdl");
        assert_eq!(rows[0].id, prop);
        assert_eq!(report_rows(doc, "wait=1", KindFilter::All, false, by_class).len(), 1);
        assert_eq!(report_rows(doc, "_light=255*", KindFilter::All, false, by_class).len(), 1);
        assert_eq!(report_rows(doc, "classname=light", KindFilter::All, false, by_class).len(), 1);
        assert_eq!(report_rows(doc, "=main", KindFilter::All, false, by_class).len(), 1, "any key");
        assert!(report_rows(doc, "wait=9", KindFilter::All, false, by_class).is_empty());
        let by_details = SortBy { column: 2, ascending: false };
        assert_eq!(report_rows(doc, "", KindFilter::All, false, by_details)[0].id, prop);
    }

    #[test]
    fn report_sorts_origins_numerically() {
        let mut state = hp_app::testing::test_state();
        let doc = &mut state.doc;
        for (i, x) in [-100.0, -23.76, -208.0, -29.0, 5.0, -230.5].iter().enumerate() {
            doc.create_point_entity("info_target", DVec3::new(*x, i as f64, 0.0), &[]).unwrap();
        }
        let rows = report_rows(doc, "info_target", KindFilter::All, false, SortBy { column: 3, ascending: true });
        let xs: Vec<f64> = rows.iter().map(|r| r.position.unwrap().x).collect();
        assert_eq!(xs, [-230.5, -208.0, -100.0, -29.0, -23.76, 5.0]);
        let rows = report_rows(doc, "", KindFilter::All, false, SortBy { column: 3, ascending: true });
        assert!(rows.windows(2).all(|w| cmp_positions(w[0].position, w[1].position).is_le()));
    }

    #[test]
    fn brush_entities_are_found_at_their_center() {
        let state = hp_app::testing::test_state();
        assert_eq!(display_origin(&state.doc, 6), Some(DVec3::new(96.0, 32.0, 32.0)));
        assert_eq!(display_origin(&state.doc, 5), Some(DVec3::new(0.0, 0.0, 96.0)));
        assert_eq!(display_origin(&state.doc, 999), None);
    }

    #[test]
    fn report_selection_modifiers() {
        let state = hp_app::testing::test_state();
        let mut r = ReportState::default();
        r.rows = report_rows(&state.doc, "", KindFilter::All, false, r.sort);
        select_report_row(&mut r, 0, egui::Modifiers::NONE);
        select_report_row(&mut r, 2, egui::Modifiers::SHIFT);
        assert_eq!(r.selected.len(), 3);
        select_report_row(&mut r, 1, egui::Modifiers::COMMAND);
        assert_eq!(r.selected.len(), 2);
        select_report_row(&mut r, 9, egui::Modifiers::NONE);
        assert_eq!(r.selected.len(), 2, "out of range rows are ignored");
    }
}
