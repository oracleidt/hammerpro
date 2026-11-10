use egui::{RichText, Ui};
use hp_app::extend::{StatusContext, StatusSegment, StatusSide, status_order};
use hp_app::i18n::TrTable;
use hp_app::settings::DisplayUnits;
use hp_app::widgets::fmt_coord;
use hp_app::{Action, AppState, Severity, t, tf};
use hp_editor::describe::{DescKind, Description};
use hp_geom::Aabb;

use crate::common::{fmt_grid, status_toggle, with_action_tooltip};

pub(super) fn segments() -> Vec<Box<dyn StatusSegment>> {
    vec![
        Box::new(Hint),
        Box::new(Selection::default()),
        Box::new(Cursor),
        Box::new(Fps),
        Box::new(TextureLock),
        Box::new(Snap),
        Box::new(Grid),
    ]
}

const BOUNDS_REFRESH_SECS: f64 = 1.0;

#[derive(Debug, Default)]
pub struct StatusCache {
    key: Option<(u64, u64)>,
    computed_at: f64,
    bounds: Option<Aabb>,
    desc_key: Option<(u64, usize)>,
    desc_at: f64,
    description: Option<Description>,
}

impl StatusCache {
    pub fn description(&mut self, state: &AppState) -> Option<&Description> {
        let key = (state.doc.revision(), state.doc.selection().objects.len());
        let now = state.frame.time;
        let selection_changed = self.desc_key.is_none_or(|(_, n)| n != key.1);
        let due = (now - self.desc_at).abs() > BOUNDS_REFRESH_SECS / 4.0;
        if self.desc_key != Some(key) && (selection_changed || due || key.1 <= 1) {
            self.desc_key = Some(key);
            self.desc_at = now;
            self.description = state.doc.describe_selection();
        }
        self.description.as_ref()
    }

    pub fn selection_bounds(&mut self, state: &AppState) -> Option<Aabb> {
        let key = (state.doc.revision(), state.assets.generation());
        let now = state.frame.time;
        let stale = self.key != Some(key) || (now - self.computed_at).abs() > BOUNDS_REFRESH_SECS;
        if stale {
            self.bounds = if state.doc.selection().is_empty() {
                None
            } else {
                state.doc.selection_bounds(&|e| state.entity_bounds(e))
            };
            self.key = Some(key);
            self.computed_at = now;
        }
        self.bounds
    }
}

pub const SIZE_LIMIT: f64 = 1.0e6;

pub fn fmt_len(v: f64) -> String {
    fmt_coord(v)
}

pub fn fmt_point(p: glam::DVec3) -> String {
    format!("{} {} {}", fmt_len(p.x), fmt_len(p.y), fmt_len(p.z))
}

pub fn fmt_size(b: &Aabb) -> String {
    fmt_size_in(b, DisplayUnits::Units)
}

pub fn fmt_size_in(b: &Aabb, units: DisplayUnits) -> String {
    let s = b.size();
    if !s.to_array().iter().all(|v| v.is_finite() && v.abs() <= SIZE_LIMIT) {
        return t!("shell.status.size_out_of_range").to_owned();
    }
    format!("{} × {} × {}", units.fmt_length(s.x), units.fmt_length(s.y), units.fmt_length(s.z))
}

pub fn fmt_center(b: &Aabb) -> String {
    let c = b.center();
    if !c.to_array().iter().all(|v| v.is_finite() && v.abs() <= SIZE_LIMIT) {
        return String::new();
    }
    format!("@({})", fmt_point(c.round()))
}

pub fn fmt_description(d: &Description) -> String {
    match d.kind {
        DescKind::Solid => tf!("shell.status.desc_solid", d.id, d.faces),
        DescKind::EntitySolid => tf!("shell.status.desc_entity_solid", d.id, d.faces, d.classname),
        DescKind::BrushEntity | DescKind::PointEntity if d.targetname.is_empty() => {
            tf!("shell.status.desc_entity", d.classname, d.id)
        }
        DescKind::BrushEntity | DescKind::PointEntity => {
            tf!("shell.status.desc_named_entity", d.targetname, d.classname, d.id)
        }
        DescKind::Group => tf!("shell.status.desc_group", d.id, d.group_size),
    }
}

struct Hint;

impl StatusSegment for Hint {
    fn id(&self) -> &'static str {
        "shell_hint"
    }

    fn side(&self) -> StatusSide {
        StatusSide::Left
    }

    fn order(&self) -> i32 {
        status_order::HINT
    }

    fn ui(&mut self, ui: &mut Ui, state: &mut AppState, cx: &mut StatusContext) {
        let hint = cx
            .tool_hint
            .clone()
            .filter(|h| !h.is_empty())
            .or_else(|| state.assets.status_line())
            .unwrap_or_else(|| t!("status.ready").to_owned());
        ui.add(egui::Label::new(RichText::new(hint).small()).truncate());
    }
}

#[derive(Default)]
struct Selection {
    cache: StatusCache,
}

impl StatusSegment for Selection {
    fn id(&self) -> &'static str {
        "shell_selection"
    }

    fn side(&self) -> StatusSide {
        StatusSide::Left
    }

    fn order(&self) -> i32 {
        status_order::SELECTION
    }

    fn width(&self) -> Option<f32> {
        Some(340.0)
    }

    fn ui(&mut self, ui: &mut Ui, state: &mut AppState, _cx: &mut StatusContext) {
        let weak = ui.visuals().weak_text_color();
        let sel = state.doc.selection();
        let objects = sel.objects.len();
        let faces = sel.faces.len();
        let description = if objects > 0 { self.cache.description(state).cloned() } else { None };
        let mut summary = String::new();
        if let Some(d) = &description {
            summary.push_str(&fmt_description(d));
        } else if objects > 0 {
            summary.push_str(&tf!("status.selected_count", objects));
        }
        if faces > 0 {
            if !summary.is_empty() {
                summary.push_str(", ");
            }
            summary.push_str(&tf!("shell.status.faces", faces));
        }
        let bounds = if objects > 0 || faces > 0 { self.cache.selection_bounds(state) } else { None };
        if summary.is_empty() {
            ui.label(RichText::new(t!("shell.status.nothing_selected")).small().color(weak));
            return;
        }
        let mut text = summary;
        if let Some(b) = bounds {
            text.push_str("  ");
            text.push_str(&fmt_size_in(&b, state.settings.workflow.units));
            let center = fmt_center(&b);
            if !center.is_empty() {
                text.push(' ');
                text.push_str(&center);
            }
        }
        let sense = if description.is_some() { egui::Sense::click() } else { egui::Sense::hover() };
        let r = ui.add(egui::Label::new(RichText::new(text).small()).truncate().sense(sense));
        let r = match bounds {
            Some(b) => {
                crate::common::lazy_hover_text(r, || tf!("shell.status.bounds_tip", fmt_point(b.min), fmt_point(b.max)))
            }
            None => r,
        };
        if let Some(d) = &description {
            let id = d.id;
            let r = crate::common::lazy_hover_text(r, || tf!("shell.status.copy_id_tip", id));
            if r.hovered() {
                ui.ctx().set_cursor_icon(egui::CursorIcon::PointingHand);
            }
            if r.clicked() {
                ui.ctx().copy_text(d.id.to_string());
                state.notify(Severity::Info, tf!("shell.status.id_copied", d.id));
            }
        }
    }
}

struct Cursor;

impl StatusSegment for Cursor {
    fn id(&self) -> &'static str {
        "shell_cursor"
    }

    fn side(&self) -> StatusSide {
        StatusSide::Left
    }

    fn order(&self) -> i32 {
        status_order::CURSOR
    }

    fn width(&self) -> Option<f32> {
        Some(170.0)
    }

    fn ui(&mut self, ui: &mut Ui, state: &mut AppState, _cx: &mut StatusContext) {
        match state.hover_world {
            Some(p) => {
                ui.label(RichText::new(format!("⌖ {}", fmt_point(p))).monospace().small())
                    .on_hover_text(t!("shell.status.cursor_tip"));
            }
            None => {
                let weak = ui.visuals().weak_text_color();
                ui.label(RichText::new("⌖ -").monospace().small().color(weak));
            }
        }
    }
}

struct Fps;

impl StatusSegment for Fps {
    fn id(&self) -> &'static str {
        "shell_fps"
    }

    fn side(&self) -> StatusSide {
        StatusSide::Right
    }

    fn order(&self) -> i32 {
        status_order::FPS
    }

    fn ui(&mut self, ui: &mut Ui, state: &mut AppState, _cx: &mut StatusContext) {
        if state.settings.view.show_fps {
            let weak = ui.visuals().weak_text_color();
            ui.label(
                RichText::new(tf!("status.fps", format!("{:.0}", state.frame.fps))).monospace().small().color(weak),
            );
            ui.separator();
        }
    }
}

struct TextureLock;

impl StatusSegment for TextureLock {
    fn id(&self) -> &'static str {
        "shell_texture_lock"
    }

    fn side(&self) -> StatusSide {
        StatusSide::Right
    }

    fn order(&self) -> i32 {
        status_order::TEXTURE_LOCK
    }

    fn ui(&mut self, ui: &mut Ui, state: &mut AppState, _cx: &mut StatusContext) {
        if state.settings.workflow.show_toolbar {
            return;
        }
        let texlock = state.settings.edit.texture_lock;
        let r = status_toggle(ui, t!("status.texlock_on"), texlock);
        if with_action_tooltip(r, state, Action::ToggleTextureLock).clicked() {
            state.request(Action::ToggleTextureLock);
        }
    }
}

struct Snap;

impl StatusSegment for Snap {
    fn id(&self) -> &'static str {
        "shell_snap"
    }

    fn side(&self) -> StatusSide {
        StatusSide::Right
    }

    fn order(&self) -> i32 {
        status_order::SNAP
    }

    fn ui(&mut self, ui: &mut Ui, state: &mut AppState, _cx: &mut StatusContext) {
        if state.settings.workflow.show_toolbar {
            return;
        }
        let snap = state.settings.edit.snap_to_grid;
        let r = status_toggle(ui, t!("status.snap_on"), snap);
        if with_action_tooltip(r, state, Action::ToggleSnap).clicked() {
            state.request(Action::ToggleSnap);
        }
    }
}

struct Grid;

impl StatusSegment for Grid {
    fn id(&self) -> &'static str {
        "shell_grid"
    }

    fn side(&self) -> StatusSide {
        StatusSide::Right
    }

    fn order(&self) -> i32 {
        status_order::GRID
    }

    fn ui(&mut self, ui: &mut Ui, state: &mut AppState, _cx: &mut StatusContext) {
        if state.settings.workflow.show_toolbar {
            return;
        }
        let target = state.target_grid();
        let size = match target.source.custom_name() {
            Some(name) => format!("{name} {}", crate::grids::steps_text(target.frame.step.to_array())),
            None => fmt_grid(target.frame.nominal()),
        };
        ui.label(RichText::new(tf!("status.grid", size)).monospace().small())
            .on_hover_text(crate::grids::source_tip(state, &target));
    }
}

pub(crate) const TR: TrTable = &[
    ("shell.status.faces", "{0} faces", "{0} face(s)"),
    ("shell.status.nothing_selected", "No selection", "Aucune sélection"),
    ("shell.status.bounds_tip", "From {0}\nto {1}", "De {0}\nà {1}"),
    ("shell.status.size_out_of_range", "size out of range", "taille hors limites"),
    ("shell.status.cursor_tip", "World position under the mouse", "Position dans le monde sous la souris"),
    ("shell.status.desc_solid", "Solid n° {0}, {1} faces", "Solide n° {0}, {1} faces"),
    ("shell.status.desc_entity_solid", "Solid n° {0}, {1} faces, of {2}", "Solide n° {0}, {1} faces, de {2}"),
    ("shell.status.desc_entity", "{0} n° {1}", "{0} n° {1}"),
    ("shell.status.desc_named_entity", "{0} ({1}) n° {2}", "{0} ({1}) n° {2}"),
    ("shell.status.desc_group", "Group n° {0} of {1} objects", "Groupe n° {0} de {1} objets"),
    ("shell.status.copy_id_tip", "Click to copy the id {0}", "Cliquez pour copier l'identifiant {0}"),
    ("shell.status.id_copied", "Copied the id {0}", "Identifiant {0} copié"),
];

#[cfg(test)]
mod tests {
    use super::*;
    use glam::DVec3;

    #[test]
    fn size_format() {
        let b = Aabb::new(DVec3::new(-64.0, 0.0, 0.0), DVec3::new(64.0, 32.5, 16.0));
        assert_eq!(fmt_size(&b), "128 × 32.50 × 16");
        let b = Aabb::new(DVec3::splat(-40000.0), DVec3::splat(40000.0));
        assert_eq!(fmt_size(&b), "80000 × 80000 × 80000");
    }

    #[test]
    fn huge_selection_reads_out_of_range() {
        let _g = crate::tests::lang_guard();
        crate::register_translations();
        hp_app::i18n::set_lang(hp_app::i18n::Lang::En);
        let huge = Aabb::new(DVec3::new(-1e300, 0.0, 0.0), DVec3::new(1e300, 64.0, 64.0));
        let text = fmt_size(&huge);
        assert_eq!(text, "size out of range");
        hp_app::i18n::set_lang(hp_app::i18n::Lang::Fr);
        assert_eq!(fmt_size(&huge), "taille hors limites");
        hp_app::i18n::set_lang(hp_app::i18n::Lang::En);
        let inf = Aabb::new(DVec3::ZERO, DVec3::new(f64::INFINITY, 1.0, 1.0));
        assert_eq!(fmt_size(&inf), "size out of range");
        let overflow = Aabb::new(DVec3::splat(-1e308), DVec3::splat(1e308));
        assert_eq!(fmt_size(&overflow), "size out of range");
        assert_eq!(fmt_point(huge.min), "-1.00e300 0 0");
        assert_eq!(fmt_point(DVec3::new(16.0, -0.5, 2.0e9)), "16 -0.50 2.00e9");
        assert_eq!(fmt_len(f64::NEG_INFINITY), "-inf");
        assert_eq!(fmt_len(f64::NAN), "NaN");
        for v in [1e300, -1e300, 1e19, i64::MAX as f64] {
            assert!(!fmt_len(v).contains("9223372036854775807"), "{v}: {}", fmt_len(v));
        }
    }

    #[test]
    fn units_center_and_descriptions() {
        let _g = crate::tests::lang_guard();
        crate::register_translations();
        hp_app::i18n::set_lang(hp_app::i18n::Lang::En);
        let b = Aabb::new(DVec3::new(0.0, 0.0, 0.0), DVec3::new(128.0, 32.0, 16.0));
        assert_eq!(fmt_size_in(&b, DisplayUnits::FeetInches), "10' 8\" × 2' 8\" × 1' 4\"");
        assert_eq!(fmt_size_in(&b, DisplayUnits::Meters), "3.25 m × 0.81 m × 0.41 m");
        assert_eq!(fmt_center(&b), "@(64 16 8)");
        let huge = Aabb::new(DVec3::new(-1e300, 0.0, 0.0), DVec3::new(1e300, 64.0, 64.0));
        assert_eq!(fmt_size_in(&huge, DisplayUnits::Inches), "size out of range");
        let desc = |kind, targetname: &str| Description {
            kind,
            id: 42,
            classname: "light".into(),
            targetname: targetname.into(),
            faces: 6,
            solids: 1,
            group_size: 5,
        };
        assert_eq!(fmt_description(&desc(DescKind::PointEntity, "lamp")), "lamp (light) n° 42");
        assert_eq!(fmt_description(&desc(DescKind::PointEntity, "")), "light n° 42");
        assert_eq!(fmt_description(&desc(DescKind::Solid, "")), "Solid n° 42, 6 faces");
        assert_eq!(fmt_description(&desc(DescKind::Group, "")), "Group n° 42 of 5 objects");
        hp_app::i18n::set_lang(hp_app::i18n::Lang::Fr);
        assert_eq!(fmt_description(&desc(DescKind::Group, "")), "Groupe n° 42 de 5 objets");
        hp_app::i18n::set_lang(hp_app::i18n::Lang::En);
    }

    #[test]
    fn description_cache_follows_the_document() {
        let mut state = hp_app::testing::test_state();
        let mut cache = StatusCache::default();
        assert!(cache.description(&state).is_none());
        state.doc.select(&[hp_editor::ObjRef::Entity(5)], hp_editor::SelectOp::Replace);
        assert_eq!(cache.description(&state).map(|d| d.id), Some(5));
        state.doc.select_all();
        let many = cache.description(&state).cloned();
        assert!(many.is_none_or(|d| d.kind == DescKind::Group));
        state.doc.select(&[hp_editor::ObjRef::Entity(5)], hp_editor::SelectOp::Replace);
        let mut seg = Selection::default();
        hp_app::testing::run_ui(&mut state, |ui, state| {
            seg.ui(ui, state, &mut StatusContext::default());
        });
    }

    #[test]
    fn bounds_cache_follows_selection() {
        let mut state = hp_app::testing::test_state();
        let mut cache = StatusCache::default();
        assert!(cache.selection_bounds(&state).is_none());
        state.doc.select_all();
        let b = cache.selection_bounds(&state);
        assert!(b.is_some_and(|b| b.size().x >= 512.0), "{b:?}");
        state.doc.clear_selection();
        assert!(cache.selection_bounds(&state).is_none());
    }
}
