use std::time::{Duration, Instant};

use egui::{RichText, Ui};
use glam::DVec3;
use hp_app::{AppState, Severity, t, tf, theme};
use hp_compile::LogLocation;
use hp_compile::stats::{self, CompileStats, Hint, LimitRow, NameCount, Timing};

use super::run::{RunState, StepStatus};
use super::{CompileDialog, step_name};
use crate::util::fmt_duration_ms;

const REPARSE: Duration = Duration::from_millis(500);

#[derive(Default)]
pub(crate) struct StatsView {
    key: Option<(u64, usize)>,
    parsed_at: Option<Instant>,
    pub(crate) stats: CompileStats,
    pub(crate) hints: Vec<Hint>,
}

pub(crate) fn hint_text(h: &Hint) -> String {
    let args = h.args();
    let refs: Vec<&dyn std::fmt::Display> = args.iter().map(|a| a as &dyn std::fmt::Display).collect();
    hp_app::i18n::tf(h.key(), &refs)
}

fn stage_text(name: &str) -> String {
    let key = format!("compilestats.stage.{}", name.replace(", ", "_").replace([' ', '-', '(', ')'], "_"));
    if hp_app::i18n::has_key(&key) { hp_app::i18n::tr(&key).to_owned() } else { name.to_owned() }
}

fn vis_mode_key(v: &stats::VisStats) -> Option<&'static str> {
    match (v.mode.as_str(), v.reused) {
        ("full", false) => Some("compilestats.mode_full"),
        ("fast", false) => Some("compilestats.mode_fast"),
        ("full", true) => Some("compilestats.mode_full_reused"),
        ("fast", true) => Some("compilestats.mode_fast_reused"),
        ("off", _) => Some("compilestats.mode_off"),
        (m, _) if m.starts_with("skipped") => Some("compilestats.mode_skipped"),
        _ => None,
    }
}

fn vis_mode_text(v: &stats::VisStats) -> String {
    vis_mode_key(v).map_or_else(|| v.mode.clone(), |key| hp_app::i18n::tr(key).to_owned())
}

fn n(v: u64) -> String {
    let s = v.to_string();
    let mut out = String::with_capacity(s.len() + s.len() / 3);
    for (i, c) in s.chars().enumerate() {
        if i > 0 && (s.len() - i).is_multiple_of(3) {
            out.push('\u{202f}');
        }
        out.push(c);
    }
    out
}

fn pos_text(p: [f64; 3]) -> String {
    hp_app::widgets::fmt_vec3(DVec3::from_array(p))
}

fn size_text(s: [f64; 3]) -> String {
    tf!("compilestats.size", s[0], s[1], s[2])
}

fn level_severity(level: stats::HintLevel) -> Severity {
    match stats::hint_severity(level) {
        hp_compile::Severity::Error => Severity::Error,
        hp_compile::Severity::Warning => Severity::Warning,
        _ => Severity::Info,
    }
}

fn checked_location(state: &AppState, loc: &LogLocation) -> LogLocation {
    let mut out = loc.clone();
    if let (Some(side), Some(p)) = (loc.side_id, loc.position) {
        let p = DVec3::from_array(p);
        let scene = state.doc.scene();
        let on_plane = scene.solids().values().any(|n| {
            n.solid.sides.iter().filter(|s| s.id == side).any(|s| {
                let [a, b, c] = s.plane;
                let normal = (b - a).cross(c - a).normalize_or_zero();
                normal != DVec3::ZERO && normal.dot(p - a).abs() < 4.0
            })
        });
        if !on_plane {
            out.side_id = None;
        }
    }
    out
}

fn limit_bar(ui: &mut Ui, fraction: f64, text: String, width: f32) {
    let severity = theme::fraction_severity(fraction, 0.75);
    ui.add(
        hp_app::widgets::progress_bar(fraction as f32)
            .desired_width(width)
            .desired_height(14.0)
            .fill(theme::bar_fill(severity))
            .text(RichText::new(text).small()),
    );
}

fn share_bar(ui: &mut Ui, fraction: f64, text: String) {
    ui.add(
        hp_app::widgets::progress_bar(fraction as f32)
            .desired_width(140.0)
            .desired_height(12.0)
            .fill(theme::bar_fill(Severity::Info))
            .text(RichText::new(text).small()),
    );
}

struct Ctx<'a> {
    same_map: bool,
    other_map_tip: &'a str,
    goto: Option<LogLocation>,
}

impl Ctx<'_> {
    fn goto_button(&mut self, ui: &mut Ui, loc: LogLocation, tip: String) {
        let button = egui::Button::new(RichText::new(t!("compile.goto")).small()).small();
        if self.same_map {
            if ui.add(button).on_hover_text(tip).clicked() {
                self.goto = Some(loc);
            }
        } else {
            ui.add_enabled(false, button).on_disabled_hover_text(self.other_map_tip);
        }
    }

    fn place(&mut self, ui: &mut Ui, p: Option<[f64; 3]>) {
        match p {
            Some(p) => {
                ui.horizontal(|ui| {
                    let tip = tf!("compile.goto_position", pos_text(p));
                    self.goto_button(ui, LogLocation { position: Some(p), ..LogLocation::default() }, tip);
                    ui.label(RichText::new(pos_text(p)).small().monospace().weak());
                });
            }
            None => {
                ui.label("");
            }
        }
    }
}

fn kv(ui: &mut Ui, label: &str, value: String) {
    ui.label(RichText::new(label).strong());
    ui.label(value);
    ui.end_row();
}

fn section(ui: &mut Ui, id: &str, run_id: u64, title: String, open: bool, body: impl FnOnce(&mut Ui)) {
    egui::CollapsingHeader::new(RichText::new(title).strong())
        .id_salt(("hp_compile_stats", id, run_id))
        .default_open(open)
        .show(ui, body);
}

fn sub_title(ui: &mut Ui, text: String) {
    ui.add_space(6.0);
    ui.label(RichText::new(text).strong().small());
}

impl StatsView {
    pub(crate) fn update(&mut self, run: &RunState, run_id: u64, running: bool) {
        let key = (run_id, run.log.len());
        if self.key == Some(key) {
            return;
        }
        let same_run = self.key.is_some_and(|k| k.0 == run_id);
        if running && same_run && self.parsed_at.is_some_and(|t| t.elapsed() < REPARSE) {
            return;
        }
        let lines: Vec<&str> = run.log.iter().map(|l| l.text.as_str()).collect();
        self.stats = stats::parse_lines(&lines);
        self.hints = stats::hints(&self.stats);
        self.key = Some(key);
        self.parsed_at = Some(Instant::now());
    }

    pub(crate) fn ui(&mut self, ui: &mut Ui, state: &mut AppState, run: &RunState, run_id: u64, same_map: bool) {
        let other_map_tip = tf!("compile.other_map", run.map_name);
        let mut cx = Ctx { same_map, other_map_tip: &other_map_tip, goto: None };
        egui::ScrollArea::vertical().id_salt("hp_compile_stats_scroll").auto_shrink([false, false]).show(ui, |ui| {
            if self.stats.is_empty() {
                let text = if run.summary.is_none() { t!("compilestats.running") } else { t!("compilestats.none") };
                ui.add_space(12.0);
                ui.label(RichText::new(text).weak());
                return;
            }
            self.summary_ui(ui, run);
            ui.add_space(4.0);
            self.hints_ui(ui, &mut cx, run_id);
            self.limits_ui(ui, run_id);
            self.steps_ui(ui, run, run_id);
            self.geometry_ui(ui, &mut cx, run_id);
            self.vis_ui(ui, &mut cx, run_id);
            self.lighting_ui(ui, &mut cx, run_id);
        });
        if let Some(loc) = cx.goto {
            let loc = checked_location(state, &loc);
            if !super::goto_location(state, &loc) {
                state.notify(Severity::Warning, t!("compile.not_found"));
            }
        }
    }

    fn summary_ui(&self, ui: &mut Ui, run: &RunState) {
        let st = &self.stats;
        let mut chips: Vec<(String, String)> = Vec::new();
        if let Some(g) = &st.geometry {
            if let Some(b) = g.bsp_bytes {
                chips.push((t!("compilestats.bsp_file").to_owned(), hp_compile::format_bytes(b)));
            }
            chips.push((t!("compilestats.brushes").to_owned(), n(g.brushes.total)));
            chips.push((t!("compilestats.faces").to_owned(), n(g.world_faces + g.entity_faces)));
            chips.push((t!("compilestats.triangles").to_owned(), n(g.triangles)));
        }
        if let Some(v) = &st.vis {
            chips.push((t!("compilestats.clusters").to_owned(), n(v.clusters)));
            if v.average > 0.0 {
                chips.push((t!("compilestats.visible_avg").to_owned(), format!("{:.1}%", v.average_percent)));
            }
        }
        if let Some(l) = &st.lighting {
            for p in &l.passes {
                chips.push((
                    tf!("compilestats.pass_lightmaps", p.pass),
                    format!("{} / {}", hp_compile::format_bytes(p.bytes), hp_compile::format_bytes(p.max_bytes)),
                ));
            }
        } else if let Some(b) = st.geometry.as_ref().and_then(|g| g.estimated_lightmap_bytes) {
            chips.push((t!("compilestats.lightmap_estimate").to_owned(), hp_compile::format_bytes(b)));
        }
        if let Some(s) = run.summary {
            chips.push((t!("compilestats.total_time").to_owned(), fmt_duration_ms(s.total_ms)));
        }
        ui.horizontal_wrapped(|ui| {
            for (label, value) in chips {
                egui::Frame::new()
                    .fill(ui.visuals().faint_bg_color)
                    .stroke(ui.visuals().widgets.noninteractive.bg_stroke)
                    .corner_radius(4.0)
                    .inner_margin(egui::Margin::symmetric(8, 4))
                    .show(ui, |ui| {
                        ui.vertical(|ui| {
                            ui.label(RichText::new(label).small().weak());
                            ui.label(RichText::new(value).strong());
                        });
                    });
            }
        });
    }

    fn hints_ui(&self, ui: &mut Ui, cx: &mut Ctx<'_>, run_id: u64) {
        section(ui, "hints", run_id, tf!("compilestats.hints", self.hints.len()), true, |ui| {
            if self.hints.is_empty() {
                ui.label(RichText::new(t!("compilestats.no_hints")).weak());
                return;
            }
            for h in &self.hints {
                let severity = level_severity(h.level);
                ui.horizontal_wrapped(|ui| {
                    ui.label(RichText::new(severity.icon()).color(severity.color()));
                    if let Some(loc) = &h.location {
                        let tip = super::location_text(loc);
                        cx.goto_button(ui, loc.clone(), tf!("compile.problem_goto", tip));
                    }
                    ui.label(hint_text(h));
                });
            }
        });
    }

    fn limits_ui(&self, ui: &mut Ui, run_id: u64) {
        if self.stats.limits.is_empty() {
            return;
        }
        let mut rows: Vec<&LimitRow> = self.stats.limits.iter().collect();
        rows.sort_by(|a, b| {
            let limited = |r: &LimitRow| r.max > 0 || r.max_bytes > 0;
            limited(b).cmp(&limited(a)).then(b.percent.total_cmp(&a.percent))
        });
        let full = rows.iter().filter(|r| r.percent >= 75.0).count();
        let title = if full > 0 { tf!("compilestats.limits_near", full) } else { t!("compilestats.limits").to_owned() };
        section(ui, "limits", run_id, title, true, |ui| {
            egui::Grid::new(("hp_compile_stats_limits", run_id))
                .num_columns(4)
                .spacing([12.0, 3.0])
                .striped(true)
                .show(ui, |ui| {
                    for h in [
                        "compilestats.col_object",
                        "compilestats.col_fullness",
                        "compilestats.col_objects",
                        "compilestats.col_memory",
                    ] {
                        ui.label(RichText::new(t!(h)).small().weak());
                    }
                    ui.end_row();
                    for r in rows {
                        let name = ui.label(RichText::new(&r.name).monospace());
                        if let Some(v) = r.valve_name() {
                            name.on_hover_text(tf!("compilestats.limit_tip", v));
                        }
                        let limited = r.max > 0 || r.max_bytes > 0;
                        if limited {
                            limit_bar(ui, r.percent / 100.0, format!("{:.1}%", r.percent), 180.0);
                        } else {
                            ui.label(RichText::new(t!("compilestats.no_limit")).small().weak());
                        }
                        let objects = match r.used {
                            Some(u) if r.max > 0 => format!("{} / {}", n(u), n(r.max)),
                            Some(u) => n(u),
                            None => t!("compilestats.variable").to_owned(),
                        };
                        ui.label(RichText::new(objects).monospace());
                        let bytes = if r.max_bytes > 0 {
                            format!("{} / {}", hp_compile::format_bytes(r.bytes), hp_compile::format_bytes(r.max_bytes))
                        } else {
                            hp_compile::format_bytes(r.bytes)
                        };
                        ui.label(RichText::new(bytes).monospace().weak());
                        ui.end_row();
                    }
                });
        });
    }

    fn steps_ui(&self, ui: &mut Ui, run: &RunState, run_id: u64) {
        let done: Vec<_> =
            run.steps.iter().filter(|s| s.duration_ms.is_some() && s.status != StepStatus::Skipped).collect();
        if done.is_empty() {
            return;
        }
        let total: u64 = done.iter().filter_map(|s| s.duration_ms).sum();
        section(ui, "steps", run_id, t!("compilestats.step_times").to_owned(), false, |ui| {
            egui::Grid::new(("hp_compile_stats_steps", run_id)).num_columns(2).spacing([12.0, 3.0]).show(ui, |ui| {
                for s in done {
                    let ms = s.duration_ms.unwrap_or(0);
                    ui.label(step_name(s.kind));
                    share_bar(ui, ms as f64 / total.max(1) as f64, fmt_duration_ms(ms));
                    ui.end_row();
                }
            });
        });
    }

    fn geometry_ui(&self, ui: &mut Ui, cx: &mut Ctx<'_>, run_id: u64) {
        let Some(g) = &self.stats.geometry else { return };
        section(ui, "geometry", run_id, t!("compilestats.geometry").to_owned(), true, |ui| {
            egui::Grid::new(("hp_compile_stats_geo", run_id)).num_columns(2).spacing([12.0, 3.0]).show(ui, |ui| {
                let b = &g.brushes;
                kv(
                    ui,
                    t!("compilestats.brushes"),
                    tf!(
                        "compilestats.brushes_value",
                        n(b.total),
                        n(b.structural),
                        n(b.detail),
                        n(b.water),
                        n(b.tool),
                        n(b.entity)
                    ),
                );
                kv(
                    ui,
                    t!("compilestats.entities"),
                    tf!(
                        "compilestats.entities_value",
                        n(g.entities_in_map),
                        n(g.brush_entities),
                        n(g.point_entities),
                        n(g.entities_in_lump)
                    ),
                );
                kv(
                    ui,
                    t!("compilestats.faces"),
                    tf!(
                        "compilestats.faces_value",
                        n(g.world_faces),
                        n(g.entity_faces),
                        n(g.displacement_faces),
                        n(g.triangles)
                    ),
                );
                let [p2, p3, p4] = g.displacement_powers;
                kv(
                    ui,
                    t!("compilestats.displacements"),
                    tf!("compilestats.displacements_value", n(g.displacements), p2, p3, p4),
                );
                if let Some(s) = g.map_size {
                    kv(ui, t!("compilestats.map_size"), size_text(s));
                }
                if let Some(b) = g.bsp_bytes {
                    kv(ui, t!("compilestats.bsp_file"), hp_compile::format_bytes(b));
                }
            });
            if let (Some(luxels), Some(bytes)) = (g.estimated_luxels, g.estimated_lightmap_bytes) {
                ui.horizontal(|ui| {
                    ui.label(RichText::new(t!("compilestats.lightmap_estimate")).strong());
                    let max = g.max_lightmap_bytes.max(1);
                    limit_bar(
                        ui,
                        bytes as f64 / max as f64,
                        tf!(
                            "compilestats.lightmap_estimate_value",
                            n(luxels),
                            hp_compile::format_bytes(bytes),
                            hp_compile::format_bytes(max)
                        ),
                        320.0,
                    );
                });
            }
            timings_ui(ui, &g.stages, ("geo_stages", run_id));
            if !g.materials.is_empty() {
                sub_title(
                    ui,
                    tf!("compilestats.materials", g.materials.len(), g.material_total.max(g.materials.len() as u64)),
                );
                counts_grid(ui, &g.materials, ("geo_mats", run_id), true);
            }
            if !g.entity_classes.is_empty() {
                sub_title(ui, t!("compilestats.classes").to_owned());
                counts_grid(ui, &g.entity_classes, ("geo_classes", run_id), false);
            }
            if !g.brush_entity_classes.is_empty() {
                sub_title(ui, t!("compilestats.brush_classes").to_owned());
                egui::Grid::new(("hp_compile_stats_bclasses", run_id))
                    .num_columns(3)
                    .spacing([12.0, 2.0])
                    .striped(true)
                    .show(ui, |ui| {
                        for c in &g.brush_entity_classes {
                            ui.label(RichText::new(n(c.count)).monospace());
                            ui.label(&c.name);
                            ui.label(RichText::new(tf!("compilestats.n_brushes", n(c.brushes.unwrap_or(0)))).weak());
                            ui.end_row();
                        }
                    });
            }
            if !g.areas.is_empty() {
                sub_title(ui, tf!("compilestats.areas", g.area_total.max(g.areas.len() as u64)));
                egui::Grid::new(("hp_compile_stats_areas", run_id))
                    .num_columns(5)
                    .spacing([12.0, 2.0])
                    .striped(true)
                    .show(ui, |ui| {
                        for h in [
                            "compilestats.col_area",
                            "compilestats.col_faces",
                            "compilestats.col_clusters",
                            "compilestats.col_leaves",
                            "compilestats.col_place",
                        ] {
                            ui.label(RichText::new(t!(h)).small().weak());
                        }
                        ui.end_row();
                        for a in &g.areas {
                            ui.label(RichText::new(a.area.to_string()).monospace());
                            ui.label(RichText::new(n(a.faces)).monospace());
                            ui.label(RichText::new(n(a.clusters)).monospace());
                            ui.label(RichText::new(n(a.leaves)).monospace());
                            cx.place(ui, a.position);
                            ui.end_row();
                        }
                    });
            }
            if !g.small_brushes.is_empty() {
                sub_title(
                    ui,
                    tf!("compilestats.small_brushes", n(g.small_brush_total.max(g.small_brushes.len() as u64))),
                );
                ui.label(RichText::new(t!("compilestats.small_brushes_tip")).small().weak());
                egui::Grid::new(("hp_compile_stats_small", run_id))
                    .num_columns(3)
                    .spacing([12.0, 2.0])
                    .striped(true)
                    .show(ui, |ui| {
                        for b in &g.small_brushes {
                            ui.horizontal(|ui| {
                                let loc = LogLocation {
                                    brush_id: Some(b.brush_id),
                                    position: b.position,
                                    ..LogLocation::default()
                                };
                                cx.goto_button(ui, loc, tf!("compile.goto_brush", b.brush_id));
                                ui.label(RichText::new(tf!("compilestats.brush_n", b.brush_id)).monospace());
                            });
                            ui.label(size_text(b.size));
                            ui.label(
                                RichText::new(b.position.map(pos_text).unwrap_or_default()).small().monospace().weak(),
                            );
                            ui.end_row();
                        }
                    });
            }
            if !g.fadeless_props.is_empty() {
                sub_title(ui, tf!("compilestats.fadeless", n(g.fadeless_total)));
                ui.label(RichText::new(t!("compilestats.fadeless_tip")).small().weak());
                egui::Grid::new(("hp_compile_stats_fade", run_id))
                    .num_columns(3)
                    .spacing([12.0, 2.0])
                    .striped(true)
                    .show(ui, |ui| {
                        for p in &g.fadeless_props {
                            ui.label(RichText::new(n(p.count)).monospace());
                            ui.label(&p.name);
                            cx.place(ui, p.position);
                            ui.end_row();
                        }
                    });
            }
        });
    }

    fn vis_ui(&self, ui: &mut Ui, cx: &mut Ctx<'_>, run_id: u64) {
        let Some(v) = &self.stats.vis else { return };
        section(ui, "vis", run_id, t!("compilestats.vis").to_owned(), true, |ui| {
            egui::Grid::new(("hp_compile_stats_vis", run_id)).num_columns(2).spacing([12.0, 3.0]).show(ui, |ui| {
                let mode = vis_mode_text(v);
                if !mode.is_empty() {
                    kv(ui, t!("compilestats.vis_mode"), mode);
                }
                kv(
                    ui,
                    t!("compilestats.clusters"),
                    tf!("compilestats.clusters_value", n(v.clusters), n(v.portals), n(v.leaves)),
                );
                let radius = match (v.radius, &v.radius_source) {
                    (Some(r), Some(s)) => format!("{r:.0} ({s})"),
                    (Some(r), None) => format!("{r:.0}"),
                    _ => t!("compilestats.radius_none").to_owned(),
                };
                kv(ui, t!("compilestats.radius"), radius);
                if v.average > 0.0 {
                    ui.label(RichText::new(t!("compilestats.visible")).strong());
                    limit_bar(
                        ui,
                        v.average_percent / 100.0,
                        tf!(
                            "compilestats.visible_value",
                            format!("{:.1}", v.average),
                            format!("{:.1}", v.average_percent),
                            n(v.max),
                            format!("{:.1}", v.max_percent)
                        ),
                        320.0,
                    );
                    ui.end_row();
                }
            });
            timings_ui(ui, &v.times, ("vis_times", run_id));
            if !v.worst.is_empty() {
                sub_title(ui, t!("compilestats.worst").to_owned());
                ui.label(RichText::new(t!("compilestats.worst_tip")).small().weak());
                egui::Grid::new(("hp_compile_stats_worst", run_id))
                    .num_columns(4)
                    .spacing([12.0, 2.0])
                    .striped(true)
                    .show(ui, |ui| {
                        for h in [
                            "compilestats.col_cluster",
                            "compilestats.col_sees",
                            "compilestats.col_faces",
                            "compilestats.col_place",
                        ] {
                            ui.label(RichText::new(t!(h)).small().weak());
                        }
                        ui.end_row();
                        for c in &v.worst {
                            ui.label(RichText::new(c.cluster.to_string()).monospace());
                            limit_bar(ui, c.percent / 100.0, format!("{} ({:.1}%)", n(c.sees), c.percent), 160.0);
                            ui.label(RichText::new(n(c.faces)).monospace());
                            cx.place(ui, c.position);
                            ui.end_row();
                        }
                    });
            }
        });
    }

    fn lighting_ui(&self, ui: &mut Ui, cx: &mut Ctx<'_>, run_id: u64) {
        let Some(l) = &self.stats.lighting else { return };
        section(ui, "lighting", run_id, t!("compilestats.lighting").to_owned(), true, |ui| {
            egui::Grid::new(("hp_compile_stats_light", run_id)).num_columns(2).spacing([12.0, 3.0]).show(ui, |ui| {
                for p in &l.passes {
                    ui.label(RichText::new(tf!("compilestats.pass_lightmaps", p.pass)).strong());
                    let max = p.max_bytes.max(1);
                    limit_bar(
                        ui,
                        p.bytes as f64 / max as f64,
                        format!(
                            "{} / {} ({:.1}%)",
                            hp_compile::format_bytes(p.bytes),
                            hp_compile::format_bytes(max),
                            p.bytes as f64 * 100.0 / max as f64
                        ),
                        320.0,
                    );
                    ui.end_row();
                }
                kv(
                    ui,
                    t!("compilestats.faces"),
                    tf!("compilestats.lit_faces_value", n(l.faces), n(l.lit_faces), n(l.luxels)),
                );
                let k = &l.lights;
                kv(
                    ui,
                    t!("compilestats.lights"),
                    tf!(
                        "compilestats.lights_value",
                        n(k.total),
                        n(k.point),
                        n(k.spot),
                        n(k.sun),
                        n(k.sky_ambient),
                        n(k.surface)
                    ),
                );
                if l.max_switchable_styles > 0 {
                    ui.label(RichText::new(t!("compilestats.styles")).strong());
                    let f = l.switchable_styles as f64 / l.max_switchable_styles as f64;
                    limit_bar(ui, f, format!("{} / {}", l.switchable_styles, l.max_switchable_styles), 200.0);
                    ui.end_row();
                }
                if l.texture_light_faces > 0 {
                    kv(
                        ui,
                        t!("compilestats.texture_lights"),
                        tf!("compilestats.texture_lights_value", n(l.texture_light_faces), n(k.surface)),
                    );
                }
                kv(
                    ui,
                    t!("compilestats.static_props"),
                    tf!("compilestats.static_props_value", n(l.static_props), n(l.vertex_lit_props)),
                );
                kv(
                    ui,
                    t!("compilestats.detail_props"),
                    tf!("compilestats.detail_props_value", n(l.detail_props), n(l.detail_lit)),
                );
                kv(ui, t!("compilestats.bounces"), l.bounces.to_string());
                kv(
                    ui,
                    t!("compilestats.ambient"),
                    tf!("compilestats.ambient_value", n(l.ambient_samples), n(l.ambient_leaves)),
                );
            });
            timings_ui(ui, &l.stages, ("light_stages", run_id));
            if !l.top_faces.is_empty() {
                sub_title(ui, t!("compilestats.top_faces").to_owned());
                ui.label(RichText::new(t!("compilestats.top_faces_tip")).small().weak());
                let most = l.top_faces.first().map_or(1, |f| f.luxels.max(1));
                egui::Grid::new(("hp_compile_stats_luxels", run_id))
                    .num_columns(5)
                    .spacing([12.0, 2.0])
                    .striped(true)
                    .show(ui, |ui| {
                        for h in [
                            "compilestats.col_luxels",
                            "compilestats.col_scale",
                            "compilestats.col_size",
                            "compilestats.col_material",
                            "compilestats.col_place",
                        ] {
                            ui.label(RichText::new(t!(h)).small().weak());
                        }
                        ui.end_row();
                        for f in &l.top_faces {
                            share_bar(ui, f.luxels as f64 / most as f64, n(f.luxels));
                            let scale = RichText::new(format!("{}", (f.scale * 100.0).round() / 100.0)).monospace();
                            ui.label(if f.scale > 0.0 && f.scale < 16.0 { scale.strong() } else { scale });
                            ui.label(size_text(f.size));
                            ui.label(&f.material);
                            ui.horizontal(|ui| {
                                let loc = LogLocation {
                                    side_id: (f.side_id > 0).then_some(f.side_id),
                                    position: f.position,
                                    ..LogLocation::default()
                                };
                                let tip =
                                    f.position.map(|p| tf!("compile.goto_position", pos_text(p))).unwrap_or_default();
                                cx.goto_button(ui, loc, tip);
                                if let Some(p) = f.position {
                                    ui.label(RichText::new(pos_text(p)).small().monospace().weak());
                                }
                            });
                            ui.end_row();
                        }
                    });
            }
            if !l.top_materials.is_empty() {
                sub_title(ui, t!("compilestats.top_materials").to_owned());
                egui::Grid::new(("hp_compile_stats_luxmats", run_id))
                    .num_columns(3)
                    .spacing([12.0, 2.0])
                    .striped(true)
                    .show(ui, |ui| {
                        for m in &l.top_materials {
                            share_bar(
                                ui,
                                m.percent.unwrap_or(0.0) / 100.0,
                                format!("{} ({:.1}%)", n(m.count), m.percent.unwrap_or(0.0)),
                            );
                            ui.label(RichText::new(tf!("compilestats.n_faces", n(m.brushes.unwrap_or(0)))).weak());
                            ui.label(&m.name);
                            ui.end_row();
                        }
                    });
            }
        });
    }
}

fn timings_ui(ui: &mut Ui, list: &[Timing], id: (&str, u64)) {
    if list.is_empty() {
        return;
    }
    let total: f64 = list.iter().map(|t| t.secs).sum();
    sub_title(ui, tf!("compilestats.stages", format!("{total:.2}")));
    let most = list.iter().map(|t| t.secs).fold(0.0f64, f64::max).max(1e-9);
    egui::Grid::new(("hp_compile_stats_times", id.0, id.1)).num_columns(2).spacing([12.0, 2.0]).show(ui, |ui| {
        for t in list {
            ui.label(stage_text(&t.name));
            share_bar(ui, t.secs / most, format!("{:.3} s", t.secs));
            ui.end_row();
        }
    });
}

fn counts_grid(ui: &mut Ui, list: &[NameCount], id: (&str, u64), shares: bool) {
    let most = list.first().map_or(1, |c| c.count.max(1));
    egui::Grid::new(("hp_compile_stats_counts", id.0, id.1)).num_columns(2).spacing([12.0, 2.0]).striped(true).show(
        ui,
        |ui| {
            for c in list {
                let text = match c.percent.filter(|_| shares) {
                    Some(p) => format!("{} ({p:.1}%)", n(c.count)),
                    None => n(c.count),
                };
                share_bar(ui, c.count as f64 / most as f64, text);
                ui.label(&c.name);
                ui.end_row();
            }
        },
    );
}

impl CompileDialog {
    pub fn mcp_stats(&self) -> Option<(String, bool, CompileStats, Vec<Hint>)> {
        let run = self.run.as_ref()?;
        let lines: Vec<&str> = run.log.iter().map(|l| l.text.as_str()).collect();
        let st = stats::parse_lines(&lines);
        let hints = stats::hints(&st);
        Some((run.map_name.clone(), self.is_running(), st, hints))
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    const LOG: &[&str] = &[
        "Hammer Pro map compiler (hp-bspc 0.1.0)",
        "---- Map statistics ----",
        "Object names            Objects/Maxobjs       Memory / Maxmem   Fullness",
        "brushsides                   60000/65536      480000/524288  ( 91.6%)",
        "---- Geometry details ----",
        "Brushes: 40 total, 30 structural, 5 detail, 1 water, 2 tool, 2 in brush entities",
        "Small structural brushes (top 1 of 25):",
        "#1  brush 77  16 x 16 x 16 units  at (8 8 8)",
        "---- Visibility statistics ----",
        "Clusters: 120, portals: 340, leaves: 800 (500 empty)",
        "Vis mode: full",
        "Visible clusters: average 48.0 (40.0%), max 90 (75.0%) in cluster 7",
        "Clusters that see the most (top 1 of 120):",
        "#1  cluster 7  sees 90 clusters (75.0%)  2500 faces  at (10 20 30)",
    ];

    fn run_with_log() -> RunState {
        let mut run = RunState::preparing("gm_x");
        for l in LOG {
            run.push_log(Some(0), hp_compile::Severity::Info, (*l).to_owned(), None);
        }
        run
    }

    #[test]
    fn the_view_reads_the_log_of_the_run() {
        crate::register_translations();
        let run = run_with_log();
        let mut view = StatsView::default();
        view.update(&run, 1, false);
        assert_eq!(view.stats.limit("brushsides").map(|r| r.percent), Some(91.6));
        assert_eq!(view.stats.geometry.as_ref().map(|g| g.small_brush_total), Some(25));
        assert_eq!(view.stats.vis.as_ref().map(|v| v.worst.len()), Some(1));
        assert!(view.hints.iter().any(|h| h.key() == "compilestats.hint.small_brushes"));
        assert!(view.hints.iter().any(|h| h.key() == "compilestats.hint.wide_view"));
        let mut run = run;
        run.push_log(Some(0), hp_compile::Severity::Info, "---- Lighting statistics ----".into(), None);
        view.update(&run, 1, false);
        assert!(view.stats.lighting.is_some());
    }

    #[test]
    fn every_hint_has_its_text_in_both_languages() {
        crate::register_translations();
        let table = crate::tr::compile_stats::TR;
        for (key, english) in stats::HINT_TEXTS {
            let row = table.iter().find(|(k, _, _)| k == key).unwrap_or_else(|| panic!("{key} missing"));
            assert_eq!(row.1, *english, "{key}: the English of the log and of the editor differ");
            for i in 0..4 {
                let p = format!("{{{i}}}");
                assert_eq!(row.1.contains(&p), row.2.contains(&p), "{key} {p}");
            }
        }
        let h = Hint {
            kind: stats::HintKind::SmallBrushes { count: 30, brush_id: 77 },
            level: stats::HintLevel::Tip,
            location: None,
        };
        assert!(hint_text(&h).contains("77"));
        assert!(stage_text("read brushes") != "compilestats.stage.read_brushes");
        assert_eq!(stage_text("something new"), "something new");
        assert_eq!(n(1_234_567), "1\u{202f}234\u{202f}567");
        assert_eq!(n(12), "12");
    }

    #[test]
    fn a_reused_visibility_is_not_shown_as_skipped() {
        crate::register_translations();
        let mut run = RunState::preparing("gm_x");
        for l in [
            "---- Visibility statistics ----",
            "Clusters: 120, portals: 340, leaves: 800 (500 empty)",
            "Vis mode: full (reused)",
            "Vis radius: none",
            "Vis times: reused 0.045 s, total 0.045 s",
            "Visible clusters: average 48.0 (40.0%), max 90 (75.0%) in cluster 7",
        ] {
            run.push_log(Some(0), hp_compile::Severity::Info, l.to_owned(), None);
        }
        let mut view = StatsView::default();
        view.update(&run, 1, false);
        let v = view.stats.vis.clone().expect("vis");
        assert_eq!((v.mode.as_str(), v.reused), ("full", true));
        assert_eq!(vis_mode_key(&v), Some("compilestats.mode_full_reused"));
        let fast = stats::VisStats { mode: "fast".into(), reused: true, ..stats::VisStats::default() };
        assert_eq!(vis_mode_key(&fast), Some("compilestats.mode_fast_reused"));
        let leak = stats::VisStats { mode: "skipped (leak)".into(), ..stats::VisStats::default() };
        assert_eq!(vis_mode_key(&leak), Some("compilestats.mode_skipped"));
        assert_eq!(vis_mode_text(&stats::VisStats::default()), "");
        let unknown = stats::VisStats { mode: "custom".into(), ..stats::VisStats::default() };
        assert_eq!(vis_mode_text(&unknown), "custom");
        for lang in hp_app::i18n::Lang::ALL {
            let tr = |key: &str| hp_app::i18n::tr_in(key, lang);
            for key in ["compilestats.mode_full_reused", "compilestats.mode_fast_reused", "compilestats.stage.reused"] {
                assert!(hp_app::i18n::has_translation(key, lang), "{key} {lang:?}");
            }
            let reused = tr("compilestats.mode_full_reused");
            for other in ["compilestats.mode_skipped", "compilestats.mode_full", "compilestats.mode_fast_reused"] {
                assert_ne!(reused, tr(other), "{other} {lang:?}");
            }
            for name in ["base", "flow", "cluster merge", "pas", "reused"] {
                let key = format!("compilestats.stage.{}", name.replace(' ', "_"));
                assert!(hp_app::i18n::has_translation(&key, lang), "{key} {lang:?}");
            }
        }
        assert_ne!(stage_text("reused"), "reused");
        let mut state = hp_app::testing::test_state();
        hp_app::testing::run_ui(&mut state, |ui, state| view.ui(ui, state, &run, 1, true));
    }

    #[test]
    fn the_tab_draws_and_goes_to_places() {
        crate::register_translations();
        let run = run_with_log();
        let mut view = StatsView::default();
        view.update(&run, 1, false);
        let mut state = hp_app::testing::test_state();
        hp_app::testing::run_ui(&mut state, |ui, state| view.ui(ui, state, &run, 1, true));
        let loc = LogLocation { side_id: Some(999_999), position: Some([1.0, 2.0, 3.0]), ..LogLocation::default() };
        assert_eq!(checked_location(&state, &loc).side_id, None);
    }
}
