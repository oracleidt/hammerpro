use std::collections::HashMap;

use egui::{CornerRadius, RichText, Stroke, Ui};
use hp_app::source_filter::{SourceBadge, SourceFilter, SourceGroup, SourceTable};
use hp_app::state::AssetsApi;
use hp_app::{t, tf, theme};

#[derive(Default)]
pub(crate) struct Sources {
    key: Option<(u64, u64)>,
    pub table: SourceTable,
    labels: HashMap<u32, String>,
    badges: HashMap<u32, String>,
    badge_lang: Option<hp_app::i18n::Lang>,
    missing: String,
}

impl Sources {
    pub fn refresh(&mut self, assets: &dyn AssetsApi) {
        let key = (assets.generation(), assets.fs().map_or(0, |f| f.generation()));
        let lang = hp_app::i18n::lang();
        if self.key == Some(key) && self.badge_lang == Some(lang) {
            return;
        }
        self.key = Some(key);
        self.labels.clear();
        match assets.fs() {
            Some(fs) => {
                self.table = SourceTable::of_fs(fs);
                for s in fs.sources() {
                    self.labels.insert(s.id, s.label);
                }
            }
            None => self.table = SourceTable::default(),
        }
        self.badge_lang = Some(lang);
        self.missing = t!("entities.badge_missing").to_owned();
        let ids: Vec<u32> = self.labels.keys().copied().collect();
        self.badges = ids.into_iter().map(|id| (id, self.badge_text(Some(id)))).collect();
    }

    pub fn badge_str(&self, source: Option<u32>) -> &str {
        match source {
            None => self.missing.as_str(),
            Some(s) => self.badges.get(&s).map_or("", String::as_str),
        }
    }

    pub fn passes(&self, filter: SourceFilter, gmod_only: bool, source: Option<u32>) -> bool {
        match source {
            Some(s) => self.table.passes(filter, gmod_only, s),
            None => filter == SourceFilter::All && !gmod_only,
        }
    }

    pub fn badge_text(&self, source: Option<u32>) -> String {
        let Some(source) = source else { return t!("entities.badge_missing").to_owned() };
        match self.table.badge(source) {
            Some(SourceBadge::Base) => t!("entities.badge_base").to_owned(),
            Some(SourceBadge::Mounted) => {
                let label = self.labels.get(&source).map_or("", String::as_str);
                tf!("entities.badge_mounted", short_label(label))
            }
            Some(SourceBadge::Loose) => t!("entities.badge_loose").to_owned(),
            Some(SourceBadge::Addon) => t!("entities.badge_addon").to_owned(),
            Some(SourceBadge::Workshop(Some(id))) => tf!("entities.badge_workshop_id", id),
            Some(SourceBadge::Workshop(None)) => t!("entities.badge_workshop").to_owned(),
            Some(SourceBadge::Custom) => t!("entities.badge_custom").to_owned(),
            Some(SourceBadge::Other) | None => self.labels.get(&source).cloned().unwrap_or_default(),
        }
    }

    pub fn label(&self, source: Option<u32>) -> &str {
        source.and_then(|s| self.labels.get(&s)).map_or("", String::as_str)
    }
}

fn short_label(label: &str) -> &str {
    label.split(" (").next().unwrap_or(label).trim()
}

pub(crate) fn source_chips(ui: &mut Ui, sources: &Sources, filter: &mut SourceFilter, gmod_only: &mut bool) -> bool {
    let current = *filter;
    let mut picked: Option<SourceFilter> = None;
    let mut toggled = false;
    ui.horizontal_wrapped(|ui| {
        ui.spacing_mut().item_spacing.x = 3.0;
        let mut chip = |ui: &mut Ui, f: SourceFilter, text: &str| {
            if ui.add(egui::Button::selectable(current == f, RichText::new(text).small())).clicked() && current != f {
                picked = Some(f);
            }
        };
        chip(ui, SourceFilter::All, t!("common.all"));
        for (group, f, label) in [
            (SourceGroup::Game, SourceFilter::Game, t!("entities.src_game")),
            (SourceGroup::Addons, SourceFilter::Addons, t!("entities.src_addons")),
            (SourceGroup::Workshop, SourceFilter::Workshop, t!("entities.src_workshop")),
        ] {
            if sources.table.has_group(group) || current == f {
                chip(ui, f, label);
            }
        }
        let resp = ui
            .checkbox(gmod_only, RichText::new(t!("entities.gmod_only")).small())
            .on_hover_text(t!("entities.gmod_only_tip"));
        toggled = resp.changed();
    });
    if let Some(f) = picked {
        *filter = f;
    }
    picked.is_some() || toggled
}

#[derive(Default)]
pub(crate) struct UsesCache {
    key: Option<(crate::view::DocStamp, String, String)>,
    count: usize,
}

impl UsesCache {
    pub fn count(&mut self, doc: &hp_editor::Document, key: &str, value: &str) -> usize {
        let k = (crate::view::DocStamp::of(doc), key.to_ascii_lowercase(), value.trim().to_ascii_lowercase());
        if self.key.as_ref() != Some(&k) {
            self.count = crate::edit::entities_using(doc, key, value).len();
            self.key = Some(k);
        }
        self.count
    }
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(crate) enum BannerAction {
    Use,
    Cancel,
}

pub(crate) fn pick_banner(ui: &mut Ui, label: &str, can_use: bool) -> Option<BannerAction> {
    let mut action = None;
    egui::Frame::new()
        .fill(theme::accent_bg())
        .stroke(Stroke::new(1.0, theme::accent()))
        .corner_radius(CornerRadius::same(4))
        .inner_margin(egui::Margin::same(6))
        .show(ui, |ui| {
            ui.add(egui::Label::new(RichText::new(tf!("entities.pick_for", label)).color(theme::accent())).wrap());
            ui.horizontal(|ui| {
                if ui.add_enabled(can_use, egui::Button::new(t!("entities.pick_use"))).clicked() {
                    action = Some(BannerAction::Use);
                }
                if ui.button(t!("common.cancel")).clicked() {
                    action = Some(BannerAction::Cancel);
                }
            });
        });
    ui.add_space(4.0);
    action
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn short_labels_and_unknown_sources() {
        assert_eq!(short_label("cstrike (cstrike_pak.vpk)"), "cstrike");
        assert_eq!(short_label("hl2"), "hl2");
        let s = Sources::default();
        assert!(s.passes(SourceFilter::All, false, None));
        assert!(!s.passes(SourceFilter::All, true, None));
        assert!(!s.passes(SourceFilter::Game, false, Some(3)));
    }

    #[test]
    fn banner_and_chips_draw() {
        crate::register_translations();
        let mut state = hp_app::testing::test_state();
        let mut sources = Sources::default();
        sources.refresh(state.assets.as_ref());
        let mut filter = SourceFilter::All;
        let mut gmod = false;
        hp_app::testing::run_ui(&mut state, |ui, _| {
            assert_eq!(pick_banner(ui, "light (model)", false), None);
            source_chips(ui, &sources, &mut filter, &mut gmod);
        });
        assert_eq!(filter, SourceFilter::All);
    }
}
