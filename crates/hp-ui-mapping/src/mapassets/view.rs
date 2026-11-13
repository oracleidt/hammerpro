use egui::{RichText, Ui};
use hp_app::theme;
use hp_app::{AppState, t, tf};
use hp_fs::SourceOrigin;
use hp_fs::deps::DepKind;

use super::model::{AssetTree, Group, Item, Show, is_base};
use super::{MapAssetsPanel, PickFor, Row};
use crate::release::{filter_box, fmt_size, origin_name, reason_text};

const MAX_DEPTH: u8 = 4;

const LIST_MIN_HEIGHT: f32 = 160.0;

fn group_name(g: Group) -> &'static str {
    match g {
        Group::Materials => t!("mapassets.group_materials"),
        Group::Models => t!("mapassets.group_models"),
        Group::Sounds => t!("mapassets.group_sounds"),
        Group::Particles => t!("mapassets.group_particles"),
        Group::Other => t!("mapassets.group_other"),
    }
}

fn origin_short(origin: Option<SourceOrigin>) -> &'static str {
    match origin {
        None => t!("mapassets.from_generated"),
        Some(SourceOrigin::BaseGame) => t!("mapassets.from_base"),
        Some(SourceOrigin::Addon) => t!("mapassets.from_addon"),
        Some(SourceOrigin::Workshop) => t!("mapassets.from_workshop"),
        Some(SourceOrigin::Custom) => t!("mapassets.from_custom"),
        Some(SourceOrigin::GameLoose) => t!("mapassets.from_loose"),
        Some(SourceOrigin::Download) => t!("mapassets.from_download"),
        Some(SourceOrigin::Mounted) => t!("mapassets.from_mounted"),
        Some(SourceOrigin::Extra) => t!("mapassets.from_extra"),
    }
}

fn kind_name(kind: DepKind) -> &'static str {
    match kind {
        DepKind::Material => t!("mapassets.kind_material"),
        DepKind::Texture => t!("mapassets.kind_texture"),
        DepKind::Model => t!("mapassets.kind_model"),
        DepKind::ModelPart => t!("mapassets.kind_model_part"),
        DepKind::Sound => t!("mapassets.kind_sound"),
        DepKind::SoundScript => t!("mapassets.kind_soundscript"),
        DepKind::Soundscape => t!("mapassets.kind_soundscape"),
        DepKind::Particle => t!("mapassets.kind_particle"),
        DepKind::ParticleManifest => t!("mapassets.kind_manifest"),
        DepKind::File => t!("mapassets.kind_file"),
    }
}

fn tooltip(tree: &AssetTree, item: Item) -> String {
    let mut out = format!("{}\n{}", tree.name(item), kind_name(tree.kind(item)));
    if let Some(n) = tree.node(item) {
        let from = if n.source_label.is_empty() {
            origin_name(n.origin).to_owned()
        } else {
            format!("{} ({})", origin_name(n.origin), n.source_label)
        };
        out.push_str(&format!("\n{}\n{}", tf!("mapassets.tip_from", from), fmt_size(n.size)));
        if is_base(n) {
            out.push_str(&format!("\n{}", t!("mapassets.tip_base")));
        }
        for r in n.reasons.iter().take(4) {
            out.push_str(&format!("\n{}", reason_text(r)));
        }
        if n.reason_count > 4 {
            out.push_str(&format!("\n{}", tf!("mapassets.tip_more", n.reason_count - 4)));
        }
    } else if let Some(m) = tree.missing(item) {
        out.push_str(&format!("\n{}", t!("mapassets.tip_missing")));
        for r in m.reasons.iter().take(4) {
            out.push_str(&format!("\n{}", reason_text(r)));
        }
    }
    out
}

impl MapAssetsPanel {
    pub fn ui(&mut self, ui: &mut Ui, state: &mut AppState) {
        if !self.output_loaded {
            self.output_loaded = true;
            if self.output.is_empty() {
                self.output = state.settings.content.collect_folder.clone();
            }
        }
        self.header(ui);
        let list_h = (ui.available_height() - self.bottom_h - 10.0).max(LIST_MIN_HEIGHT);
        let width = ui.available_width();
        ui.allocate_ui_with_layout(egui::vec2(width, list_h), egui::Layout::top_down(egui::Align::Min), |ui| {
            ui.set_min_height(list_h);
            self.list(ui);
        });
        ui.separator();
        let out = egui::ScrollArea::vertical()
            .id_salt("hp_map_assets_bottom_scroll")
            .max_height(ui.available_height())
            .auto_shrink([false, true])
            .show(ui, |ui| {
                self.collect_section(ui, state);
                ui.separator();
                self.unused_section(ui);
            });
        self.bottom_h = out.content_size.y;
        self.confirm_replace(ui.ctx(), state);
    }

    fn header(&mut self, ui: &mut Ui) {
        ui.horizontal_wrapped(|ui| {
            let busy = self.resolve.is_some();
            if ui.add_enabled(!busy, egui::Button::new(format!("🔄 {}", t!("mapassets.refresh")))).clicked() {
                self.refresh();
            }
            if busy {
                ui.spinner();
                ui.label(RichText::new(t!("mapassets.resolving")).weak());
            } else if let Some(e) = &self.error {
                ui.colored_label(theme::error(), e);
            }
        });
        let before = self.filter.clone();
        ui.horizontal_wrapped(|ui| {
            ui.selectable_value(&mut self.filter.show, Show::Custom, t!("mapassets.show_custom"))
                .on_hover_text(t!("mapassets.show_custom_tip"));
            ui.selectable_value(&mut self.filter.show, Show::All, t!("mapassets.show_all"));
            ui.selectable_value(&mut self.filter.show, Show::Missing, t!("mapassets.show_missing"));
        });
        ui.horizontal_wrapped(|ui| {
            for g in Group::ALL {
                ui.checkbox(&mut self.filter.groups[g.index()], group_name(g));
            }
        });
        ui.horizontal_wrapped(|ui| {
            let width = (ui.available_width() - 28.0).max(80.0);
            filter_box(ui, &mut self.filter.text, t!("mapassets.search"), width);
        });
        if before != self.filter {
            self.rows_dirty = true;
        }
        self.rebuild_rows();
        let t = self.totals;
        let entries: usize = t.entries.iter().sum();
        if self.tree.is_some() {
            ui.label(
                RichText::new(tf!("mapassets.summary", entries, t.files, t.textures, t.model_files, fmt_size(t.bytes)))
                    .small(),
            );
            if t.missing > 0 {
                ui.colored_label(theme::error(), RichText::new(tf!("mapassets.summary_missing", t.missing)).small());
            }
            if !self.warnings.is_empty() {
                ui.colored_label(theme::warning(), RichText::new(self.warnings.join("\n")).small());
            }
        }
        ui.horizontal_wrapped(|ui| {
            if ui.small_button(t!("mapassets.select_all")).clicked() {
                self.unchecked.clear();
                self.rows_dirty = true;
            }
            if ui.small_button(t!("mapassets.select_none")).clicked()
                && let Some(tree) = &self.tree
            {
                self.unchecked.extend(self.visible.iter().map(|&i| tree.name(tree.entries[i].item).to_owned()));
                self.rows_dirty = true;
            }
            if ui.small_button(t!("mapassets.expand_none")).clicked() {
                self.expanded.clear();
                self.rows_dirty = true;
            }
        });
        ui.separator();
    }

    pub(super) fn rebuild_rows(&mut self) {
        if !self.rows_dirty {
            return;
        }
        self.rows_dirty = false;
        self.rows.clear();
        let Some(tree) = &self.tree else {
            self.visible.clear();
            self.totals = Default::default();
            return;
        };
        self.visible = tree.visible(&self.filter);
        let selected: Vec<usize> = self
            .visible
            .iter()
            .copied()
            .filter(|&i| !self.unchecked.contains(tree.name(tree.entries[i].item)))
            .collect();
        self.totals = tree.totals(&selected, self.base_game);
        let mut current: Option<Group> = None;
        for &i in &self.visible {
            let e = &tree.entries[i];
            if current != Some(e.group) {
                current = Some(e.group);
                self.rows.push(Row::Group(e.group));
            }
            if self.collapsed[e.group.index()] {
                continue;
            }
            self.rows.push(Row::Entry(i));
            if self.expanded.contains(tree.name(e.item)) {
                let mut seen = std::collections::HashSet::from([e.item]);
                let mut stack: Vec<(Item, u8)> = tree.children(e.item).iter().rev().map(|&c| (c, 1)).collect();
                while let Some((item, depth)) = stack.pop() {
                    if !seen.insert(item) {
                        continue;
                    }
                    self.rows.push(Row::Child(item, depth));
                    if depth < MAX_DEPTH {
                        stack.extend(tree.children(item).iter().rev().map(|&c| (c, depth + 1)));
                    }
                }
            }
        }
    }

    fn list(&mut self, ui: &mut Ui) {
        if self.tree.is_none() {
            if self.error.is_none() {
                ui.label(RichText::new(t!("mapassets.resolving")).weak());
            }
            return;
        }
        if self.rows.is_empty() {
            ui.label(RichText::new(t!("mapassets.empty")).weak());
            return;
        }
        let row_h = ui.spacing().interact_size.y;
        let (Some(tree), rows) = (self.tree.take(), std::mem::take(&mut self.rows)) else { return };
        let mut toggle_group: Option<Group> = None;
        let mut toggle_expand: Option<String> = None;
        let mut toggle_check: Option<String> = None;
        egui::ScrollArea::vertical().id_salt("hp_map_assets_list").auto_shrink([false, false]).show_rows(
            ui,
            row_h,
            rows.len(),
            |ui, range| {
                for row in &rows[range] {
                    ui.horizontal(|ui| {
                        ui.set_height(row_h);
                        match *row {
                            Row::Group(g) => {
                                let count = self.visible.iter().filter(|&&i| tree.entries[i].group == g).count();
                                let arrow = if self.collapsed[g.index()] { "⏵" } else { "⏷" };
                                let text = RichText::new(format!("{arrow} {} ({count})", group_name(g))).strong();
                                if ui.add(egui::Label::new(text).sense(egui::Sense::click())).clicked() {
                                    toggle_group = Some(g);
                                }
                            }
                            Row::Entry(i) => {
                                let e = &tree.entries[i];
                                let name = tree.name(e.item);
                                let mut on = !self.unchecked.contains(name);
                                if ui.checkbox(&mut on, "").on_hover_text(t!("mapassets.check_tip")).changed() {
                                    toggle_check = Some(name.to_owned());
                                }
                                let has_children = !tree.children(e.item).is_empty();
                                let open = self.expanded.contains(name);
                                let arrow = if !has_children {
                                    " "
                                } else if open {
                                    "⏷"
                                } else {
                                    "⏵"
                                };
                                if ui.add(egui::Label::new(arrow).sense(egui::Sense::click())).clicked() && has_children
                                {
                                    toggle_expand = Some(name.to_owned());
                                }
                                let bytes = if self.base_game { e.all_bytes } else { e.custom_bytes };
                                item_line(ui, &tree, e.item, Some(bytes), e.missing);
                            }
                            Row::Child(item, depth) => {
                                ui.add_space(18.0 + 14.0 * f32::from(depth));
                                item_line(ui, &tree, item, None, false);
                            }
                        }
                    });
                }
            },
        );
        self.tree = Some(tree);
        self.rows = rows;
        if let Some(g) = toggle_group {
            self.collapsed[g.index()] = !self.collapsed[g.index()];
            self.rows_dirty = true;
        }
        if let Some(name) = toggle_expand {
            if !self.expanded.remove(&name) {
                self.expanded.insert(name);
            }
            self.rows_dirty = true;
        }
        if let Some(name) = toggle_check {
            if !self.unchecked.remove(&name) {
                self.unchecked.insert(name);
            }
            self.rows_dirty = true;
        }
    }

    fn collect_section(&mut self, ui: &mut Ui, state: &mut AppState) {
        ui.label(RichText::new(t!("mapassets.collect_title")).strong());
        ui.horizontal_wrapped(|ui| {
            ui.label(t!("mapassets.output"));
            let width = (ui.available_width() - 44.0).max(40.0);
            ui.add(
                egui::TextEdit::singleline(&mut self.output)
                    .hint_text(t!("mapassets.output_hint"))
                    .desired_width(width),
            );
            if ui.button("🗁").on_hover_text(t!("mapassets.browse")).clicked() {
                let start = self.output.clone();
                self.open_picker(PickFor::Output, &start);
            }
        });
        ui.horizontal_wrapped(|ui| {
            if ui
                .checkbox(&mut self.base_game, t!("mapassets.base_game"))
                .on_hover_text(t!("mapassets.base_game_tip"))
                .changed()
            {
                self.rows_dirty = true;
            }
            ui.checkbox(&mut self.replace, t!("mapassets.replace")).on_hover_text(t!("mapassets.replace_tip"));
        });
        if let Some(c) = &self.collect {
            let done = c.done.load(std::sync::atomic::Ordering::Relaxed);
            let fraction = if c.total == 0 { 1.0 } else { done as f32 / c.total as f32 };
            ui.horizontal_wrapped(|ui| {
                if ui.button(t!("mapassets.cancel")).clicked() {
                    c.job.cancel.store(true, std::sync::atomic::Ordering::Relaxed);
                }
                ui.add(hp_app::widgets::progress_bar(fraction).text(tf!("mapassets.collecting", done, c.total)));
            });
        } else {
            let t = self.totals;
            let ready = self.tree.is_some() && self.resolve.is_none() && (t.files > 0 || t.missing > 0);
            let label = tf!("mapassets.collect", t.files, fmt_size(t.bytes));
            if ui
                .add_enabled(ready && !self.output.trim().is_empty(), egui::Button::new(label))
                .on_hover_text(t!("mapassets.collect_tip"))
                .clicked()
            {
                self.collect_clicked(state);
            }
        }
        if let Some(s) = &state.runtime.content.map_assets.last_collect {
            match &s.error {
                Some(e) => {
                    ui.colored_label(theme::error(), RichText::new(e).small());
                }
                None => {
                    let text =
                        tf!("mapassets.last", s.copied, s.replaced, s.same, s.skipped, s.failed.len(), s.missing.len());
                    let color = if s.failed.is_empty() { theme::success() } else { theme::warning() };
                    ui.colored_label(color, RichText::new(text).small());
                    ui.horizontal(|ui| {
                        if ui.small_button(t!("mapassets.open_folder")).clicked() {
                            hp_app::os::open_folder(&s.output, false);
                        }
                        if let Some(report) = &s.report
                            && ui.small_button(t!("mapassets.open_report")).clicked()
                            && let Err(e) = hp_app::os::reveal(report)
                        {
                            log::warn!("cannot show {}: {e}", report.display());
                        }
                    });
                }
            }
        }
    }

    fn unused_section(&mut self, ui: &mut Ui) {
        egui::CollapsingHeader::new(hp_app::widgets::header_text(
            ui,
            RichText::new(t!("mapassets.unused_title")).strong(),
        ))
        .id_salt("hp_map_assets_unused")
        .default_open(false)
        .show(ui, |ui| {
            ui.label(RichText::new(t!("mapassets.unused_hint")).small().weak());
            ui.horizontal_wrapped(|ui| {
                let width = (ui.available_width() - 44.0).max(40.0);
                ui.add(
                    egui::TextEdit::singleline(&mut self.unused_folder)
                        .hint_text(t!("mapassets.unused_folder_hint"))
                        .desired_width(width),
                );
                if ui.button("🗁").on_hover_text(t!("mapassets.browse")).clicked() {
                    let start = self.unused_folder.clone();
                    self.open_picker(PickFor::Unused, &start);
                }
            });
            let ready = self.tree.is_some() && self.unused_job.is_none() && !self.unused_folder.trim().is_empty();
            ui.horizontal_wrapped(|ui| {
                if ui.add_enabled(ready, egui::Button::new(t!("mapassets.unused_check"))).clicked() {
                    self.check_unused();
                }
                if let Some(job) = &self.unused_job {
                    ui.spinner();
                    if ui.small_button(t!("mapassets.cancel")).clicked() {
                        job.cancel.store(true, std::sync::atomic::Ordering::Relaxed);
                    }
                }
            });
            match &self.unused {
                None => {}
                Some(Err(e)) => {
                    ui.colored_label(theme::error(), RichText::new(e).small());
                }
                Some(Ok(r)) => {
                    let text = tf!("mapassets.unused_result", r.unused.len(), r.scanned, fmt_size(r.unused_bytes));
                    ui.label(RichText::new(text).small());
                    if r.cancelled {
                        ui.colored_label(theme::warning(), RichText::new(t!("mapassets.unused_partial")).small());
                    }
                    if !r.unused.is_empty() {
                        ui.horizontal(|ui| {
                            filter_box(ui, &mut self.unused_filter, t!("mapassets.search"), 160.0);
                            if ui.small_button(t!("mapassets.copy_list")).clicked() {
                                let list: Vec<&str> = r.unused.iter().map(|u| u.path.as_str()).collect();
                                ui.ctx().copy_text(list.join("\n"));
                            }
                            if ui.small_button(t!("mapassets.open_folder")).clicked() {
                                hp_app::os::open_folder(&r.root, false);
                            }
                        });
                        let needle = self.unused_filter.trim().to_ascii_lowercase();
                        let shown: Vec<&hp_fs::collect::UnusedFile> = r
                            .unused
                            .iter()
                            .filter(|u| needle.is_empty() || u.path.to_ascii_lowercase().contains(&needle))
                            .collect();
                        let row_h = ui.text_style_height(&egui::TextStyle::Small) + 4.0;
                        egui::ScrollArea::vertical().id_salt("hp_map_assets_unused_list").max_height(160.0).show_rows(
                            ui,
                            row_h,
                            shown.len(),
                            |ui, range| {
                                for u in &shown[range] {
                                    ui.horizontal(|ui| {
                                        ui.label(RichText::new(&u.path).small().monospace());
                                        ui.label(RichText::new(fmt_size(u.bytes)).small().weak());
                                    });
                                }
                            },
                        );
                    }
                }
            }
        });
    }

    fn confirm_replace(&mut self, ctx: &egui::Context, state: &mut AppState) {
        let Some(pending) = &self.pending else { return };
        let mut answer = None;
        let modal = egui::Modal::new(egui::Id::unique("hp_map_assets_confirm")).show(ctx, |ui| {
            ui.set_max_width(420.0);
            ui.heading(t!("mapassets.confirm_title"));
            ui.label(tf!("mapassets.confirm_text", pending.existing, pending.input.output.display()));
            ui.add_space(8.0);
            ui.horizontal_wrapped(|ui| {
                if ui.button(t!("mapassets.confirm_replace")).clicked() {
                    answer = Some(true);
                }
                if ui.button(t!("mapassets.cancel")).clicked() {
                    answer = Some(false);
                }
            });
        });
        if answer.is_none() && modal.should_close() {
            answer = Some(false);
        }
        match answer {
            Some(true) => {
                if let Some(p) = self.pending.take() {
                    state.settings.content.collect_folder = p.input.output.display().to_string();
                    state.settings_dirty = true;
                    self.start(p.input);
                }
            }
            Some(false) => self.pending = None,
            None => {}
        }
    }
}

fn item_line(ui: &mut Ui, tree: &AssetTree, item: Item, bytes: Option<u64>, needs_missing: bool) {
    let node = tree.node(item);
    let (mark, color) = match node {
        None => ("❌", theme::error()),
        Some(_) if needs_missing => ("⚠", theme::warning()),
        Some(n) if is_base(n) => ("○", ui.visuals().weak_text_color()),
        Some(_) => ("⚫", theme::success()),
    };
    ui.colored_label(color, mark);
    let tag = match node {
        Some(n) => origin_short(n.origin),
        None => t!("mapassets.from_missing"),
    };
    let size = node.map(|n| bytes.unwrap_or(n.size)).map(fmt_size).unwrap_or_default();
    let right = format!("{size}  {tag}");
    let right_w = 8.0 * right.chars().count() as f32;
    let width = (ui.available_width() - right_w).max(60.0);
    let mut text = RichText::new(tree.name(item)).monospace();
    if node.is_some_and(is_base) {
        text = text.weak();
    } else if node.is_none() {
        text = text.color(theme::error());
    }
    let height = ui.spacing().interact_size.y;
    let resp = ui
        .allocate_ui_with_layout(egui::vec2(width, height), egui::Layout::left_to_right(egui::Align::Center), |ui| {
            ui.set_width(width);
            ui.add(egui::Label::new(text).truncate().sense(egui::Sense::click()))
        })
        .inner
        .on_hover_text(tooltip(tree, item));
    resp.context_menu(|ui| {
        if ui.button(t!("mapassets.copy_path")).clicked() {
            ui.ctx().copy_text(tree.name(item).to_owned());
            ui.close();
        }
    });
    ui.with_layout(egui::Layout::right_to_left(egui::Align::Center), |ui| {
        ui.label(RichText::new(tag).small().color(if node.is_none() { theme::error() } else { theme::info() }));
        ui.label(RichText::new(size).small().weak());
    });
}
