use std::sync::Arc;

use egui::{RichText, Ui};
use hp_app::theme;
use hp_app::{AppState, t, tf};
use hp_fs::SourceOrigin;
use hp_fs::deps::{DepKind, DepNode, DepReason, DependencyGraph, PackPolicy};

use super::ReleaseWindow;
use super::pipeline::fmt_bytes;
use super::placement::{Place, place};

#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub(crate) enum Section {
    #[default]
    Files,
    Origins,
    Missing,
    Sizes,
    Lint,
    Workshop,
}

#[derive(Default)]
struct Stats {
    key: Option<(usize, PackPolicy)>,
    shipped: usize,
    shipped_bytes: u64,
    workshop: Vec<(u64, String)>,
    origins: Vec<hp_fs::deps::OriginSummary>,
    mounted: usize,
}

#[derive(Default)]
pub(crate) struct ReportView {
    pub(crate) section: Section,
    filter: String,
    show_base: bool,
    filtered: Vec<usize>,
    key: Option<(usize, String, bool, PackPolicy)>,
    stats: Stats,
}

impl ReportView {
    pub(crate) fn invalidate(&mut self) {
        self.key = None;
        self.stats.key = None;
    }

    fn restat(&mut self, graph: &Arc<DependencyGraph>, policy: PackPolicy) {
        let key = (Arc::as_ptr(graph) as usize, policy);
        if self.stats.key == Some(key) {
            return;
        }
        let (shipped, shipped_bytes) = graph.shipped(policy).fold((0usize, 0u64), |(n, b), x| (n + 1, b + x.size));
        self.stats = Stats {
            key: Some(key),
            shipped,
            shipped_bytes,
            workshop: graph.workshop_items(),
            origins: graph.by_origin(),
            mounted: graph.nodes.iter().filter(|n| n.origin == Some(SourceOrigin::Mounted)).count(),
        };
    }

    fn refilter(&mut self, graph: &Arc<DependencyGraph>, policy: PackPolicy) {
        let key = (Arc::as_ptr(graph) as usize, self.filter.clone(), self.show_base, policy);
        if self.key.as_ref() == Some(&key) {
            return;
        }
        let needle = self.filter.trim().to_ascii_lowercase();
        self.filtered = graph
            .nodes
            .iter()
            .enumerate()
            .filter(|(_, n)| self.show_base || n.ships(policy))
            .filter(|(_, n)| {
                needle.is_empty() || n.vpath.contains(&needle) || n.source_label.to_ascii_lowercase().contains(&needle)
            })
            .map(|(i, _)| i)
            .collect();
        self.key = Some(key);
    }
}

pub(crate) fn origin_name(origin: Option<SourceOrigin>) -> &'static str {
    match origin {
        None => t!("release.origin_generated"),
        Some(SourceOrigin::Custom) => t!("release.origin_custom"),
        Some(SourceOrigin::GameLoose) => t!("release.origin_loose"),
        Some(SourceOrigin::Addon) => t!("release.origin_addon"),
        Some(SourceOrigin::BaseGame) => t!("release.origin_base"),
        Some(SourceOrigin::Mounted) => t!("release.origin_mounted"),
        Some(SourceOrigin::Extra) => t!("release.origin_extra"),
        Some(SourceOrigin::Workshop) => t!("release.origin_workshop"),
        Some(SourceOrigin::Download) => t!("release.origin_download"),
    }
}

pub(crate) fn reason_text(r: &DepReason) -> String {
    match r {
        DepReason::Root(why) => why.clone(),
        DepReason::UsedBy(v) => tf!("release.used_by", v),
        DepReason::Generated => t!("release.generated").to_owned(),
    }
}

pub(crate) fn used_by_chain(graph: &DependencyGraph, reasons: &[DepReason]) -> String {
    let mut parts: Vec<String> = Vec::new();
    let mut next = reasons.first().cloned();
    while let Some(r) = next.take() {
        if parts.len() >= 8 {
            break;
        }
        match r {
            DepReason::Root(why) => parts.push(why),
            DepReason::Generated => parts.push(t!("release.generated").to_owned()),
            DepReason::UsedBy(v) => {
                if parts.contains(&v) {
                    break;
                }
                next = graph.find(&v).and_then(|n| n.reasons.first().cloned());
                parts.push(v);
            }
        }
    }
    if parts.is_empty() {
        return t!("release.used_by_unknown").to_owned();
    }
    tf!("release.used_by", parts.join(" < "))
}

pub(crate) fn filter_box(ui: &mut Ui, text: &mut String, hint: &str, width: f32) -> bool {
    let mut changed = ui.add(egui::TextEdit::singleline(text).hint_text(hint).desired_width(width)).changed();
    if !text.is_empty() && ui.small_button("✖").on_hover_text(t!("release.clear_filter")).clicked() {
        text.clear();
        changed = true;
    }
    changed
}

pub(super) fn cell(ui: &mut Ui, width: f32, height: f32, widget: impl egui::Widget) -> egui::Response {
    ui.allocate_ui_with_layout(egui::vec2(width, height), egui::Layout::left_to_right(egui::Align::Center), |ui| {
        ui.set_width(width);
        ui.add(widget)
    })
    .inner
}

fn kind_name(k: DepKind) -> &'static str {
    match k {
        DepKind::Material => t!("release.kind_material"),
        DepKind::Texture => t!("release.kind_texture"),
        DepKind::Model => t!("release.kind_model"),
        DepKind::ModelPart => t!("release.kind_model_part"),
        DepKind::Sound => t!("release.kind_sound"),
        DepKind::SoundScript => t!("release.kind_soundscript"),
        DepKind::Soundscape => t!("release.kind_soundscape"),
        DepKind::Particle => t!("release.kind_particle"),
        DepKind::ParticleManifest => t!("release.kind_manifest"),
        DepKind::ParticleSystem => t!("release.kind_particle_system"),
        DepKind::File => t!("release.kind_file"),
    }
}

fn reasons_tooltip(n: &DepNode) -> String {
    let mut lines: Vec<String> = n.reasons.iter().map(reason_text).collect();
    if n.reason_count > n.reasons.len() {
        lines.push(tf!("release.more_reasons", n.reason_count - n.reasons.len()));
    }
    lines.join("\n")
}

impl ReleaseWindow {
    pub(super) fn report_tab(&mut self, ui: &mut Ui, state: &mut AppState) {
        let Some(graph) = self.graph.clone() else {
            ui.label(t!("release.report_empty"));
            if ui.add_enabled(self.job.is_none(), egui::Button::new(t!("release.analyze"))).clicked() {
                self.start(ui.ctx(), state, true);
            }
            return;
        };
        let s = &state.settings.content;
        let policy = PackPolicy { workshop: s.pack_workshop, mounted: s.pack_mounted };
        let mode = s.mode;
        self.report.restat(&graph, policy);
        let (shipped, shipped_bytes) = (self.report.stats.shipped, self.report.stats.shipped_bytes);
        ui.horizontal(|ui| {
            ui.label(tf!("release.report_totals", graph.nodes.len(), shipped, fmt_bytes(shipped_bytes)));
            if !graph.missing.is_empty() {
                ui.colored_label(theme::warning(), tf!("release.report_missing", graph.missing.len()));
            }
            if ui.add_enabled(self.job.is_none(), egui::Button::new(t!("release.analyze"))).clicked() {
                self.start(ui.ctx(), state, true);
            }
        });
        let tips = self.summary.as_ref().and_then(|r| r.as_ref().ok()).map(|s| s.tips.len()).unwrap_or(0);
        let lint = self.summary.as_ref().and_then(|r| r.as_ref().ok()).map(|s| s.lint.len()).unwrap_or(0);
        let workshop_count = self.report.stats.workshop.len();
        ui.horizontal(|ui| {
            let r = &mut self.report.section;
            ui.selectable_value(r, Section::Files, t!("release.section_files"));
            ui.selectable_value(r, Section::Origins, t!("release.section_origins"));
            ui.selectable_value(r, Section::Missing, tf!("release.section_missing", graph.missing.len()));
            ui.selectable_value(r, Section::Sizes, tf!("release.section_sizes", tips));
            ui.selectable_value(r, Section::Lint, tf!("release.section_lint", lint));
            ui.selectable_value(r, Section::Workshop, tf!("release.section_workshop", workshop_count));
        });
        ui.separator();
        match self.report.section {
            Section::Files => self.files_section(ui, &graph, policy, mode),
            Section::Origins => origins_section(ui, state, &self.report.stats.origins, self.report.stats.mounted),
            Section::Missing => missing_section(ui, state, &graph),
            Section::Sizes => self.sizes_section(ui),
            Section::Lint => self.lint_section(ui),
            Section::Workshop => workshop_section(ui, &self.report.stats.workshop),
        }
    }

    fn files_section(
        &mut self,
        ui: &mut Ui,
        graph: &Arc<DependencyGraph>,
        policy: PackPolicy,
        mode: hp_app::settings::ReleaseMode,
    ) {
        ui.horizontal(|ui| {
            if filter_box(ui, &mut self.report.filter, t!("release.filter"), 360.0) {
                self.report.key = None;
            }
            if ui.checkbox(&mut self.report.show_base, t!("release.show_base")).changed() {
                self.report.key = None;
            }
        });
        self.report.refilter(graph, policy);
        let row_h = ui.text_style_height(&egui::TextStyle::Body) + 6.0;
        let rows = self.report.filtered.len();
        egui::ScrollArea::vertical().id_salt("hp_release_files").auto_shrink([false, false]).show_rows(
            ui,
            row_h,
            rows,
            |ui, range| {
                for &i in &self.report.filtered[range] {
                    let Some(n) = graph.nodes.get(i) else { continue };
                    let ships = n.ships(policy);
                    let dest = if !ships {
                        RichText::new(t!("release.dest_none")).weak()
                    } else {
                        match place(&n.vpath, mode) {
                            Some(Place::Pakfile) => RichText::new(t!("release.dest_pakfile")).color(theme::warning()),
                            Some(Place::Addon) => RichText::new(t!("release.dest_addon")).color(theme::success()),
                            None => RichText::new(t!("release.dest_nowhere")).color(theme::error()),
                        }
                    };
                    ui.horizontal(|ui| {
                        let gap = ui.spacing().item_spacing.x;
                        let path_w = (ui.available_width() - 90.0 - 70.0 - 200.0 - 100.0 - 5.0 * gap).max(120.0);
                        cell(ui, path_w, row_h, egui::Label::new(RichText::new(&n.vpath).monospace()).truncate())
                            .on_hover_text(reasons_tooltip(n));
                        cell(ui, 90.0, row_h, egui::Label::new(kind_name(n.kind)));
                        cell(ui, 70.0, row_h, egui::Label::new(fmt_bytes(n.size)));
                        cell(
                            ui,
                            200.0,
                            row_h,
                            egui::Label::new(RichText::new(&n.source_label).color(theme::info())).truncate(),
                        )
                        .on_hover_text(origin_name(n.origin));
                        cell(ui, 100.0, row_h, egui::Label::new(dest).truncate());
                    });
                }
            },
        );
    }

    fn sizes_section(&mut self, ui: &mut Ui) {
        let Some(Ok(summary)) = &self.summary else {
            ui.label(t!("release.sizes_after_analysis"));
            return;
        };
        if summary.tips.is_empty() {
            ui.colored_label(theme::success(), t!("release.no_tips"));
            return;
        }
        egui::ScrollArea::vertical().id_salt("hp_release_tips").auto_shrink([false, false]).show(ui, |ui| {
            egui::Grid::new("hp_release_tips_grid").num_columns(3).striped(true).show(ui, |ui| {
                for tip in summary.tips.iter().take(500) {
                    ui.label(RichText::new(&tip.vpath).monospace());
                    ui.label(fmt_bytes(tip.bytes));
                    ui.label(t!(tip.tip));
                    ui.end_row();
                }
            });
        });
    }
}

impl ReleaseWindow {
    fn lint_section(&mut self, ui: &mut Ui) {
        let Some(Ok(summary)) = &self.summary else {
            ui.label(t!("release.sizes_after_analysis"));
            return;
        };
        if summary.lint.is_empty() {
            ui.colored_label(theme::success(), t!("release.no_lint"));
            return;
        }
        ui.label(RichText::new(t!("release.lint_hint")).weak());
        egui::ScrollArea::vertical().id_salt("hp_release_lint").auto_shrink([false, false]).show(ui, |ui| {
            egui::Grid::new("hp_release_lint_grid").num_columns(2).striped(true).show(ui, |ui| {
                for issue in summary.lint.iter().take(2000) {
                    ui.label(RichText::new(&issue.vpath).monospace());
                    ui.label(match &issue.kind {
                        hp_fs::lint::LintKind::GenericPath => t!("release.lint_generic").to_owned(),
                        hp_fs::lint::LintKind::Space => t!("release.lint_space").to_owned(),
                        hp_fs::lint::LintKind::Clash { other } => tf!("release.lint_clash", other),
                    });
                    ui.end_row();
                }
            });
        });
    }
}

fn origins_section(ui: &mut Ui, state: &mut AppState, origins: &[hp_fs::deps::OriginSummary], mounted: usize) {
    egui::Grid::new("hp_release_origins").num_columns(4).striped(true).spacing([16.0, 6.0]).show(ui, |ui| {
        ui.label(RichText::new(t!("release.col_origin")).strong());
        ui.label(RichText::new(t!("release.col_files")).strong());
        ui.label(RichText::new(t!("release.col_size")).strong());
        ui.label(RichText::new(t!("release.col_shipped")).strong());
        ui.end_row();
        for o in origins {
            ui.label(origin_name(o.origin));
            ui.label(o.files.to_string());
            ui.label(fmt_bytes(o.bytes));
            let s = &mut state.settings.content;
            match o.origin {
                Some(SourceOrigin::Mounted) => {
                    if ui
                        .checkbox(&mut s.pack_mounted, t!("release.ship_mounted"))
                        .on_hover_text(t!("release.opt_mounted_tip"))
                        .changed()
                    {
                        state.settings_dirty = true;
                    }
                }
                Some(SourceOrigin::Workshop) => {
                    if ui
                        .checkbox(&mut s.pack_workshop, t!("release.ship_workshop"))
                        .on_hover_text(t!("release.opt_workshop_tip"))
                        .changed()
                    {
                        state.settings_dirty = true;
                    }
                }
                Some(SourceOrigin::BaseGame) | Some(SourceOrigin::Extra) => {
                    ui.label(RichText::new(t!("release.never_shipped")).weak());
                }
                _ => {
                    ui.label(RichText::new(t!("release.always_shipped")).color(theme::success()));
                }
            }
            ui.end_row();
        }
    });
    if mounted > 0 {
        ui.add_space(6.0);
        ui.colored_label(theme::warning(), tf!("release.mounted_warning", mounted));
    }
}

fn missing_section(ui: &mut Ui, state: &mut AppState, graph: &DependencyGraph) {
    if graph.missing.is_empty() {
        ui.colored_label(theme::success(), t!("release.nothing_missing"));
        return;
    }
    ui.label(RichText::new(t!("release.missing_hint")).weak());
    let mut unchecked = crate::mapassets::unchecked_entries(state);
    let mut toggle: Option<String> = None;
    let (systems, files): (Vec<_>, Vec<_>) = graph.missing.iter().partition(|m| m.kind == DepKind::ParticleSystem);
    egui::ScrollArea::vertical().id_salt("hp_release_missing").auto_shrink([false, false]).show(ui, |ui| {
        for (title, list) in
            [(t!("release.missing_files_title"), &files), (t!("release.missing_systems_title"), &systems)]
        {
            if list.is_empty() {
                continue;
            }
            ui.label(RichText::new(tf!("release.missing_group", title, list.len())).strong());
            for m in list.iter() {
                let left_out = unchecked.contains(&m.name);
                let chain = used_by_chain(graph, &m.reasons);
                let all: Vec<String> =
                    m.reasons.iter().map(|r| used_by_chain(graph, std::slice::from_ref(r))).collect();
                ui.horizontal(|ui| {
                    if left_out {
                        ui.label(RichText::new("-").weak());
                    } else {
                        ui.colored_label(theme::warning(), "⚠");
                    }
                    let name = RichText::new(&m.name).monospace();
                    let name = if left_out { name.weak().strikethrough() } else { name };
                    ui.label(name).on_hover_text(all.join(
                        "
",
                    ));
                    ui.label(RichText::new(kind_name(m.kind)).weak());
                    let label = if left_out { t!("release.include_again") } else { t!("release.leave_out") };
                    if ui.small_button(label).on_hover_text(t!("release.leave_out_tip")).clicked() {
                        toggle = Some(m.name.clone());
                    }
                    ui.label(RichText::new(chain).weak());
                });
            }
            ui.add_space(6.0);
        }
    });
    if let Some(name) = toggle {
        if !unchecked.remove(&name) {
            unchecked.insert(name);
        }
        crate::mapassets::set_unchecked_entries(state, &unchecked);
    }
}

fn workshop_section(ui: &mut Ui, items: &[(u64, String)]) {
    if items.is_empty() {
        ui.label(t!("release.no_workshop"));
        return;
    }
    ui.label(RichText::new(t!("release.workshop_hint")).weak());
    if ui.button(t!("release.copy_ids")).clicked() {
        let ids: Vec<String> = items.iter().map(|(id, _)| id.to_string()).collect();
        ui.ctx().copy_text(ids.join("\n"));
    }
    egui::ScrollArea::vertical().id_salt("hp_release_workshop").auto_shrink([false, false]).show(ui, |ui| {
        for (id, label) in items {
            ui.horizontal(|ui| {
                ui.label(RichText::new(id.to_string()).monospace());
                ui.label(label);
            });
        }
    });
}
