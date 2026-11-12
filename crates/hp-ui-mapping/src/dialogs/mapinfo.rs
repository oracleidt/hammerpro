use std::sync::mpsc::{Receiver, TryRecvError};

use egui::{RichText, Ui};
use hp_app::actions::key_label;
use hp_app::runtime::HealthRuntime;
use hp_app::state::AssetStatus;
use hp_app::widgets::{fmt_vec3, grid_row};
use hp_app::{AppState, t, tf, theme};
use hp_editor::edicts::{EdictHint, KEEP_FREE, MAX_EDICTS};
use hp_editor::limits::{LIMIT_WARNING, Limit, LimitProfile, LimitUse, MAX_AREAS};
use hp_editor::{MapStats, ObjRef, SelectOp};

use crate::util::{contains_ci, dialog_body, dialog_bottom_bar, escape_pressed, icon_title, plural, request_center};

#[derive(Clone, Debug, Default, PartialEq)]
pub(crate) struct TextureMemory {
    pub(crate) bytes: u64,
    pub(crate) files: usize,
    pub(crate) largest: Vec<(String, u64)>,
}

#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub(crate) enum InfoTab {
    #[default]
    Overview,
    Limits,
}

#[derive(Default)]
pub(crate) struct MapInfoDialog {
    pub(crate) tab: InfoTab,
    doc_key: Option<crate::util::DocKey>,
    pub(crate) stats: Option<MapStats>,
    pub(crate) materials: Vec<(String, usize)>,
    pub(crate) filter: String,
    pub(crate) by_count: bool,
    applied: Option<(String, bool)>,
    pub(crate) rows: Vec<usize>,
    texture_job: Option<Receiver<TextureMemory>>,
    pub(crate) textures: Option<TextureMemory>,
}

const LIGHTMAP_TOP: usize = 8;

pub(crate) fn fmt_bytes(b: u64) -> String {
    let b = b as f64;
    if b >= 1024.0 * 1024.0 {
        format!("{:.1} MB", b / (1024.0 * 1024.0))
    } else if b >= 1024.0 {
        format!("{:.0} KB", b / 1024.0)
    } else {
        format!("{b} B")
    }
}

fn fmt_amount(limit: Limit, v: u64) -> String {
    if limit.is_bytes() { fmt_bytes(v) } else { v.to_string() }
}

pub(crate) fn fraction_color(f: f64) -> egui::Color32 {
    if f > 1.0 {
        theme::error()
    } else if f >= LIMIT_WARNING {
        theme::warning()
    } else {
        theme::success()
    }
}

pub(crate) fn profile_name(p: Option<LimitProfile>) -> &'static str {
    match p {
        None => t!("mapinfo.profile.auto"),
        Some(LimitProfile::Stock) => t!("mapinfo.profile.stock"),
        Some(LimitProfile::Gmod32) => t!("mapinfo.profile.gmod32"),
        Some(LimitProfile::Gmod64) => t!("mapinfo.profile.gmod64"),
        Some(LimitProfile::PlusPlus) => t!("mapinfo.profile.plusplus"),
    }
}

fn texture_memory(fs: &hp_fs::GameFs, materials: Vec<String>) -> TextureMemory {
    use hp_fs::deps::{DepKind, DepRoot, DepsCache, ResolveOptions, RootKind};
    let roots: Vec<DepRoot> =
        materials.into_iter().map(|name| DepRoot { kind: RootKind::Material, name, why: String::new() }).collect();
    let graph = fs.resolve_dependencies(&roots, &ResolveOptions::default(), &DepsCache::new());
    let mut textures: Vec<(String, u64)> =
        graph.nodes.iter().filter(|n| n.kind == DepKind::Texture).map(|n| (n.vpath.clone(), n.size)).collect();
    let bytes = textures.iter().map(|(_, s)| *s).sum();
    let files = textures.len();
    textures.sort_by_key(|t| std::cmp::Reverse(t.1));
    textures.truncate(8);
    TextureMemory { bytes, files, largest: textures }
}

impl MapInfoDialog {
    pub(crate) fn refresh(&mut self, state: &AppState) {
        let key = crate::util::DocKey::of(&state.doc);
        if self.doc_key != Some(key) {
            self.stats = Some(state.doc.stats());
            self.materials = state.doc.used_materials();
            self.doc_key = Some(key);
            self.applied = None;
        }
        let key = (self.filter.trim().to_ascii_lowercase(), self.by_count);
        if self.applied.as_ref() != Some(&key) {
            let mut rows: Vec<usize> =
                (0..self.materials.len()).filter(|&i| contains_ci(&self.materials[i].0, &key.0)).collect();
            if key.1 {
                rows.sort_by(|&a, &b| self.materials[b].1.cmp(&self.materials[a].1).then(a.cmp(&b)));
            }
            self.rows = rows;
            self.applied = Some(key);
        }
    }

    pub(crate) fn compute_textures(&mut self, state: &AppState) {
        let Some(fs) = state.assets.fs().cloned() else { return };
        let materials: Vec<String> = self.materials.iter().map(|(m, _)| m.clone()).collect();
        let (tx, rx) = std::sync::mpsc::channel();
        let spawned = std::thread::Builder::new().name("hp-texture-memory".into()).spawn(move || {
            let _ = tx.send(texture_memory(&fs, materials));
        });
        if spawned.is_ok() {
            self.texture_job = Some(rx);
        }
    }

    fn poll_textures(&mut self) {
        let Some(rx) = &self.texture_job else { return };
        match rx.try_recv() {
            Ok(m) => {
                self.textures = Some(m);
                self.texture_job = None;
            }
            Err(TryRecvError::Empty) => {}
            Err(TryRecvError::Disconnected) => self.texture_job = None,
        }
    }

    pub(crate) fn show(&mut self, ctx: &egui::Context, state: &mut AppState) {
        if !state.ui.show_map_info {
            return;
        }
        self.refresh(state);
        self.poll_textures();
        if self.texture_job.is_some() || state.runtime.health.checking {
            ctx.request_repaint_after(std::time::Duration::from_millis(150));
        }
        let mut open = true;
        let response = hp_app::chrome::Window::new(icon_title("ℹ", t!("mapinfo.title")))
            .id(egui::Id::unique("hp_map_info_window"))
            .default_pos(crate::util::centered_pos(ctx, egui::vec2(500.0, 600.0)))
            .open(&mut open)
            .default_size([500.0, 600.0])
            .min_size([340.0, 300.0])
            .resizable(true)
            .collapsible(true)
            .show(ctx, |ui| self.window_ui(ui, state));
        if let Some(r) = &response
            && escape_pressed(ctx, &r.response)
        {
            open = false;
        }
        if !open {
            state.ui.show_map_info = false;
        }
    }

    fn window_ui(&mut self, ui: &mut Ui, state: &mut AppState) {
        dialog_bottom_bar(ui, "hp_map_info_bottom", |ui| {
            ui.with_layout(egui::Layout::right_to_left(egui::Align::Center), |ui| {
                if ui.button(t!("common.close")).on_hover_text(key_label(egui::Key::Escape)).clicked() {
                    state.ui.show_map_info = false;
                }
            });
        });
        dialog_body(ui, |ui| self.body_ui(ui, state));
    }

    fn submaps_ui(ui: &mut Ui, state: &mut AppState) {
        let rows: Vec<(usize, String, bool, bool)> = state
            .doc
            .manifest_submaps()
            .iter()
            .enumerate()
            .map(|(i, l)| {
                let name = l.relative_vmf.rsplit(['/', '\\']).next().unwrap_or(&l.relative_vmf).to_owned();
                (i, name, l.is_primary, l.is_protected)
            })
            .collect();
        if rows.is_empty() {
            return;
        }
        let active = state.doc.active_submap();
        ui.add_space(6.0);
        hp_app::widgets::section(ui, &tf!("mapinfo.submaps_title", rows.len()));
        ui.label(RichText::new(t!("mapinfo.submaps_hint")).weak());
        let mut set_active = None;
        for (i, name, primary, protected) in rows {
            ui.horizontal(|ui| {
                let resp = ui.add_enabled(!protected, egui::RadioButton::new(active == Some(i), name));
                if resp.clicked() {
                    set_active = Some(i);
                }
                if primary {
                    ui.label(RichText::new(format!("({})", t!("mapinfo.submap_primary"))).weak());
                }
                if protected {
                    ui.label(RichText::new(format!("({})", t!("mapinfo.submap_protected"))).weak());
                }
            });
        }
        if let Some(i) = set_active {
            state.doc.set_active_submap(i);
        }
    }

    fn stats_ui(&self, ui: &mut Ui, state: &AppState) {
        let Some(s) = &self.stats else { return };
        egui::Grid::new("hp_map_info_grid").num_columns(2).spacing([24.0, 4.0]).striped(true).show(ui, |ui| {
            let num = |ui: &mut Ui, v: usize| {
                ui.label(RichText::new(v.to_string()).monospace());
            };
            grid_row(ui, t!("mapinfo.solids"), |ui| num(ui, s.solids));
            grid_row(ui, t!("mapinfo.world_solids"), |ui| num(ui, s.world_solids));
            grid_row(ui, t!("mapinfo.faces"), |ui| num(ui, s.faces));
            grid_row(ui, t!("mapinfo.entities"), |ui| {
                ui.label(
                    RichText::new(tf!("mapinfo.entities_detail", s.entities, s.point_entities, s.brush_entities))
                        .monospace(),
                )
            });
            grid_row(ui, t!("mapinfo.displacements"), |ui| num(ui, s.displacements));
            grid_row(ui, t!("mapinfo.materials"), |ui| num(ui, s.unique_materials));
            grid_row(ui, t!("mapinfo.groups"), |ui| num(ui, s.groups));
            grid_row(ui, t!("mapinfo.visgroups"), |ui| num(ui, s.visgroups));
            grid_row(ui, t!("mapinfo.invalid_solids"), |ui| {
                let text = RichText::new(s.invalid_solids.to_string()).monospace();
                ui.label(if s.invalid_solids > 0 { text.color(theme::error()) } else { text });
            });
            match s.bounds {
                Some(b) => {
                    grid_row(ui, t!("mapinfo.bounds_min"), |ui| ui.label(RichText::new(fmt_vec3(b.min)).monospace()));
                    grid_row(ui, t!("mapinfo.bounds_max"), |ui| ui.label(RichText::new(fmt_vec3(b.max)).monospace()));
                    grid_row(ui, t!("mapinfo.size"), |ui| ui.label(RichText::new(fmt_vec3(b.size())).monospace()));
                }
                None => {
                    grid_row(ui, t!("mapinfo.bounds_min"), |ui| ui.label(RichText::new(t!("common.none")).weak()));
                }
            }
            if let Some((path, areas, cut_off)) = &state.runtime.lab.nav_summary {
                grid_row(ui, t!("mapinfo.nav"), |ui| {
                    let name = path
                        .file_name()
                        .map_or_else(|| path.display().to_string(), |n| n.to_string_lossy().into_owned());
                    let text = tf!(plural(*areas, "mapinfo.nav_areas_one", "mapinfo.nav_areas"), name, areas);
                    ui.label(RichText::new(text).monospace()).on_hover_text(path.display().to_string());
                    if *cut_off > 0 {
                        let cut = tf!(plural(*cut_off, "mapinfo.nav_cut_off_one", "mapinfo.nav_cut_off"), cut_off);
                        ui.label(RichText::new(cut).color(theme::warning()))
                            .on_hover_text(t!("mapinfo.nav_cut_off_tip"));
                    }
                });
            }
        });
    }

    fn lightmaps_ui(ui: &mut Ui, state: &mut AppState) {
        state.runtime.lighting.budget_wanted = true;
        let Some(b) = &state.runtime.lighting.budget else {
            ui.horizontal(|ui| {
                ui.spinner();
                ui.label(RichText::new(t!("mapinfo.lightmaps_waiting")).weak());
            });
            return;
        };
        let f = b.fraction();
        let bar = hp_app::widgets::progress_bar(f.clamp(0.0, 1.0) as f32)
            .text(RichText::new(tf!("mapinfo.lightmaps", fmt_bytes(b.bytes), fmt_bytes(b.limit))).small())
            .fill(theme::bar_fill(theme::fraction_severity(f, LIMIT_WARNING)));
        ui.add(bar).on_hover_text(t!("mapinfo.lightmaps_tip"));
        ui.label(RichText::new(tf!("mapinfo.lightmap_faces", b.faces, b.luxels)).small());
        let notes = [
            (b.tiny_faces, "mapinfo.lightmap_tiny_one", "mapinfo.lightmap_tiny"),
            (b.wide_faces, "mapinfo.lightmap_wide_one", "mapinfo.lightmap_wide"),
            (b.oversized_displacements, "mapinfo.lightmap_oversized_one", "mapinfo.lightmap_oversized"),
        ];
        for (n, one, many) in notes {
            if n > 0 {
                ui.label(RichText::new(tf!(plural(n, one, many), n)).small().color(theme::warning()));
            }
        }
        let mut select: Option<(u32, usize)> = None;
        if !b.top.is_empty() {
            ui.collapsing(t!("mapinfo.lightmap_top"), |ui| {
                for e in b.top.iter().take(LIGHTMAP_TOP) {
                    let label = tf!("mapinfo.lightmap_face", e.solid, e.side, fmt_bytes(e.bytes), e.scale);
                    if ui.link(label).on_hover_text(t!("mapinfo.select_tip")).clicked() {
                        select = Some((e.solid, e.side));
                    }
                }
            });
        }
        if let Some((solid, _side)) = select
            && state.doc.scene().contains(ObjRef::Solid(solid))
        {
            state.doc.set_selection(&[ObjRef::Solid(solid)]);
            request_center(state);
        }
    }

    fn limit_row(ui: &mut Ui, limit: Limit, estimate: Option<&LimitUse>, exact: Option<&LimitUse>) {
        ui.label(crate::dialogs::problems::limit_name(limit.id()));
        match estimate {
            Some(u) => {
                let f = u.fraction();
                let bar = hp_app::widgets::progress_bar(f.clamp(0.0, 1.0) as f32)
                    .text(
                        RichText::new(format!("{} / {}", fmt_amount(limit, u.used), fmt_amount(limit, u.max))).small(),
                    )
                    .fill(theme::bar_fill(theme::fraction_severity(f, LIMIT_WARNING)))
                    .desired_width(190.0);
                ui.add(bar).on_hover_text(tf!("mapinfo.limit_tip", format!("{:.0}", f * 100.0)));
            }
            None => {
                ui.label(RichText::new("-").weak());
            }
        }
        match exact {
            Some(u) => {
                let f = u.fraction();
                ui.label(RichText::new(fmt_amount(limit, u.used)).monospace().color(fraction_color(f)))
                    .on_hover_text(tf!("mapinfo.limit_tip", format!("{:.0}", f * 100.0)));
            }
            None => {
                ui.label(RichText::new("-").weak());
            }
        }
        ui.end_row();
    }

    fn limits_ui(&mut self, ui: &mut Ui, state: &mut AppState) {
        ui.horizontal(|ui| {
            ui.label(t!("mapinfo.profile"));
            let current = state.settings.health.profile();
            let auto_label =
                format!("{} ({})", profile_name(None), profile_name(Some(HealthRuntime::profile_for(state))));
            let shown = if current.is_none() { auto_label.clone() } else { profile_name(current).to_owned() };
            egui::ComboBox::from_id_salt("hp_map_info_profile").selected_text(shown).show_ui(ui, |ui| {
                if ui.selectable_label(current.is_none(), auto_label).clicked() {
                    state.settings.health.set_profile(None);
                    state.settings_dirty = true;
                }
                for p in LimitProfile::ALL {
                    if ui.selectable_label(current == Some(p), profile_name(Some(p))).clicked() {
                        state.settings.health.set_profile(Some(p));
                        state.settings_dirty = true;
                    }
                }
            });
            if state.runtime.health.checking {
                ui.spinner();
            }
        });
        let rt = &state.runtime.health;
        let estimates: &[LimitUse] = rt.checks.as_ref().map_or(&[], |c| c.limits.as_slice());
        let exact: &[LimitUse] = rt.exact.as_ref().map_or(&[], |e| e.uses.as_slice());
        if estimates.is_empty() && exact.is_empty() {
            ui.label(RichText::new(t!("mapinfo.limits_waiting")).weak());
            return;
        }
        let mut select: Option<ObjRef> = None;
        egui::Grid::new("hp_map_info_limits").num_columns(3).spacing([16.0, 3.0]).striped(true).show(ui, |ui| {
            ui.label(RichText::new(t!("mapinfo.limit")).strong());
            ui.label(RichText::new(t!("mapinfo.estimate")).strong()).on_hover_text(t!("mapinfo.estimate_tip"));
            ui.label(RichText::new(t!("mapinfo.compiled")).strong()).on_hover_text(t!("mapinfo.compiled_tip"));
            ui.end_row();
            for limit in Limit::ALL {
                let e = estimates.iter().find(|u| u.limit == limit);
                let x = exact.iter().find(|u| u.limit == limit);
                if e.is_some() || x.is_some() {
                    Self::limit_row(ui, limit, e, x);
                }
            }
        });
        if let Some(ex) = &rt.exact {
            let color = if ex.areas > MAX_AREAS { theme::error() } else { ui.visuals().text_color() };
            ui.label(RichText::new(tf!("mapinfo.areas", ex.areas, MAX_AREAS, ex.clusters)).small().color(color));
        }
        for u in estimates.iter().filter(|u| !u.top.is_empty() && u.fraction() >= 0.25) {
            ui.collapsing(tf!("mapinfo.top", crate::dialogs::problems::limit_name(u.limit.id())), |ui| {
                for (obj, amount) in &u.top {
                    let label = format!("{} {}  {}", t!("mapinfo.solid"), obj.id(), fmt_amount(u.limit, *amount));
                    if ui.link(label).on_hover_text(t!("mapinfo.select_tip")).clicked() {
                        select = Some(*obj);
                    }
                }
            });
        }
        if let Some(o) = select
            && state.doc.scene().contains(o)
        {
            state.doc.set_selection(&[o]);
            request_center(state);
        }
    }

    fn edicts_ui(ui: &mut Ui, state: &AppState) {
        let Some(c) = &state.runtime.health.checks else {
            ui.label(RichText::new(t!("mapinfo.limits_waiting")).weak());
            return;
        };
        let e = &c.edicts;
        let f = e.used as f64 / MAX_EDICTS as f64;
        let severity = if e.tight() { hp_app::Severity::Warning } else { hp_app::Severity::Success };
        let bar = hp_app::widgets::progress_bar(f.clamp(0.0, 1.0) as f32)
            .text(RichText::new(tf!("mapinfo.edicts", e.used, MAX_EDICTS, e.left())).small())
            .fill(theme::bar_fill(severity));
        ui.add(bar).on_hover_text(tf!("mapinfo.edicts_tip", KEEP_FREE));
        for (class, n) in e.by_class.iter().take(8) {
            ui.label(RichText::new(format!("{class}  {n}")).monospace().small());
        }
        for hint in &e.hints {
            let text = match hint {
                EdictHint::StaticBrushes { count } => tf!("mapinfo.hint_static_brushes", count),
                EdictHint::DynamicProps { count } => tf!("mapinfo.hint_dynamic_props", count),
                EdictHint::NamedLights { count } => tf!("mapinfo.hint_named_lights", count),
            };
            ui.label(RichText::new(format!("💡 {text}")).small());
        }
    }

    fn textures_ui(&mut self, ui: &mut Ui, state: &AppState) {
        let ready = state.assets.status() == AssetStatus::Ready;
        ui.horizontal(|ui| {
            match &self.textures {
                Some(m) => {
                    ui.label(tf!("mapinfo.texture_memory", fmt_bytes(m.bytes), m.files));
                }
                None => {
                    ui.label(RichText::new(t!("mapinfo.texture_memory_unknown")).weak());
                }
            }
            if self.texture_job.is_some() {
                ui.spinner();
            } else if ui
                .add_enabled(ready, egui::Button::new(t!("mapinfo.compute")))
                .on_hover_text(t!("mapinfo.compute_tip"))
                .on_disabled_hover_text(t!("mapinfo.needs_content"))
                .clicked()
            {
                self.compute_textures(state);
            }
        });
        if let Some(m) = &self.textures {
            for (path, size) in &m.largest {
                ui.label(RichText::new(format!("{path}  {}", fmt_bytes(*size))).monospace().small());
            }
        }
    }

    fn body_ui(&mut self, ui: &mut Ui, state: &mut AppState) {
        ui.horizontal(|ui| {
            ui.selectable_value(&mut self.tab, InfoTab::Overview, t!("mapinfo.tab_overview"));
            let worst = state.runtime.health.worst_limit().map(LimitUse::fraction);
            let label = match worst {
                Some(f) if f >= LIMIT_WARNING => RichText::new(t!("mapinfo.limits")).color(fraction_color(f)),
                _ => RichText::new(t!("mapinfo.limits")),
            };
            ui.selectable_value(&mut self.tab, InfoTab::Limits, label);
        });
        ui.separator();
        if self.tab == InfoTab::Limits {
            egui::ScrollArea::vertical().id_salt("hp_map_info_limits_scroll").auto_shrink([false, false]).show(
                ui,
                |ui| {
                    self.limits_ui(ui, state);
                    ui.add_space(8.0);
                    hp_app::widgets::section(ui, t!("mapinfo.edicts_title"));
                    Self::edicts_ui(ui, state);
                    ui.add_space(8.0);
                    hp_app::widgets::section(ui, t!("mapinfo.lightmaps_title"));
                    Self::lightmaps_ui(ui, state);
                    ui.add_space(8.0);
                    hp_app::widgets::section(ui, t!("mapinfo.textures_title"));
                    self.textures_ui(ui, state);
                },
            );
            return;
        }
        self.stats_ui(ui, state);
        Self::submaps_ui(ui, state);
        ui.add_space(6.0);
        hp_app::widgets::section(ui, &tf!("mapinfo.used_materials", self.materials.len()));
        let row = egui::vec2(ui.available_width(), ui.spacing().interact_size.y);
        ui.allocate_ui_with_layout(row, egui::Layout::right_to_left(egui::Align::Center), |ui| {
            ui.selectable_value(&mut self.by_count, true, t!("mapinfo.sort_count"));
            ui.selectable_value(&mut self.by_count, false, t!("mapinfo.sort_name"));
            let w = ui.available_width().max(60.0);
            ui.allocate_ui_with_layout(
                egui::vec2(w, ui.spacing().interact_size.y),
                egui::Layout::left_to_right(egui::Align::Center),
                |ui| hp_app::widgets::search_box(ui, &mut self.filter, t!("common.filter")),
            );
        });
        self.refresh(state);

        let row_h = ui.spacing().interact_size.y;
        let mut select: Option<String> = None;
        let mut make_current: Option<String> = None;
        let rows = &self.rows;
        let materials = &self.materials;
        egui::ScrollArea::vertical().id_salt("hp_map_info_materials").auto_shrink([false, false]).show_rows(
            ui,
            row_h,
            rows.len(),
            |ui, range| {
                for i in range {
                    let Some((name, count)) = rows.get(i).and_then(|&k| materials.get(k)) else { continue };
                    let resp = ui
                        .horizontal(|ui| {
                            let r = ui.add(
                                egui::Label::new(RichText::new(name.to_ascii_lowercase()).monospace())
                                    .truncate()
                                    .sense(egui::Sense::click()),
                            );
                            ui.with_layout(egui::Layout::right_to_left(egui::Align::Center), |ui| {
                                let n = *count;
                                let text = tf!(plural(n, "mapinfo.faces_count_one", "mapinfo.faces_count"), n);
                                ui.label(RichText::new(text).weak().small());
                            });
                            r
                        })
                        .inner;
                    let resp = resp.on_hover_text(t!("mapinfo.material_tip"));
                    if resp.double_clicked() {
                        select = Some(name.clone());
                    }
                    resp.context_menu(|ui| {
                        if ui.button(t!("mapinfo.select_objects")).clicked() {
                            select = Some(name.clone());
                            ui.close();
                        }
                        if ui.button(t!("mapinfo.make_current")).clicked() {
                            make_current = Some(name.clone());
                            ui.close();
                        }
                        if ui.button(t!("common.copy_text")).clicked() {
                            ui.ctx().copy_text(name.to_ascii_lowercase());
                            ui.close();
                        }
                    });
                }
            },
        );
        if let Some(m) = select {
            select_material_users(state, &m);
        }
        if let Some(m) = make_current {
            state.use_material(&crate::util::normalize_material(&m));
        }
    }
}

pub(crate) fn select_material_users(state: &mut AppState, material: &str) -> usize {
    let n = state.doc.select_by_material(&crate::util::normalize_material(material), SelectOp::Replace);
    if n > 0 {
        request_center(state);
    }
    n
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn stats_and_materials() {
        let mut state = hp_app::testing::test_state();
        let mut d = MapInfoDialog::default();
        d.refresh(&state);
        let s = d.stats.as_ref().expect("stats");
        assert_eq!(s.solids, 3);
        assert_eq!(s.entities, 3);
        assert_eq!(d.materials.len(), 3);
        assert_eq!(d.rows.len(), 3);
        d.filter = "TOOLS".into();
        d.refresh(&state);
        assert_eq!(d.rows.len(), 1);
        d.filter.clear();
        d.by_count = true;
        d.refresh(&state);
        let counts: Vec<usize> = d.rows.iter().map(|&i| d.materials[i].1).collect();
        assert!(counts.windows(2).all(|w| w[0] >= w[1]));
        assert_eq!(select_material_users(&mut state, "concrete/concretefloor001a"), 1);
        assert_eq!(state.doc.selected_objects(), vec![hp_editor::ObjRef::Solid(3)]);
    }

    #[test]
    fn limits_edicts_and_formats() {
        let _lang = crate::lang_lock();
        crate::register_translations();
        assert_eq!(fmt_bytes(512), "512 B");
        assert_eq!(fmt_bytes(2048), "2 KB");
        assert_eq!(fmt_bytes(3 * 1024 * 1024 / 2), "1.5 MB");
        assert_eq!(fraction_color(1.2), theme::error());
        assert_eq!(fraction_color(0.85), theme::warning());
        assert_eq!(fraction_color(0.1), theme::success());
        let mut state = hp_app::testing::test_state();
        HealthRuntime::check_now(&mut state);
        state.runtime.health.set_exact("x.bsp".into(), 0, &[("brushes", 9000), ("planes", 10)], 300, 42);
        state.ui.show_map_info = true;
        let mut d = MapInfoDialog::default();
        hp_app::testing::run_ctx(&mut state, |ctx, state| d.show(ctx, state));
        d.tab = InfoTab::Limits;
        hp_app::testing::run_ctx(&mut state, |ctx, state| d.show(ctx, state));
        hp_app::i18n::set_lang(hp_app::i18n::Lang::Fr);
        hp_app::testing::run_ctx(&mut state, |ctx, state| d.show(ctx, state));
        hp_app::i18n::set_lang(hp_app::i18n::Lang::En);
        state.settings.health.set_profile(Some(LimitProfile::Stock));
        assert_eq!(HealthRuntime::profile_for(&state), LimitProfile::Stock);
        for p in [None, Some(LimitProfile::PlusPlus)] {
            assert!(!profile_name(p).starts_with("mapinfo."));
        }
    }

    #[test]
    fn lightmap_budget_and_navigation_mesh() {
        use hp_app::runtime::{BudgetEntry, BudgetSummary};
        let _lang = crate::lang_lock();
        crate::register_translations();
        let mut state = hp_app::testing::test_state();
        hp_app::testing::run_ui(&mut state, MapInfoDialog::lightmaps_ui);
        assert!(state.runtime.lighting.budget_wanted);
        let entry = BudgetEntry { solid: 2, side: 1, bytes: 4096, scale: 1.0, dims: [32, 32], displacement: false };
        state.runtime.lighting.budget = Some(BudgetSummary {
            faces: 12,
            luxels: 9000,
            bytes: 36_000,
            limit: 32 * 1024 * 1024,
            tiny_faces: 1,
            wide_faces: 2,
            oversized_displacements: 3,
            top: vec![entry; 20],
            revision: 0,
        });
        state.runtime.lab.nav_summary = Some(("C:/maps/gm_x.nav".into(), 120, 3));
        let mut d = MapInfoDialog::default();
        d.refresh(&state);
        assert!(d.stats.is_some());
        for lang in [hp_app::i18n::Lang::En, hp_app::i18n::Lang::Fr] {
            hp_app::i18n::set_lang(lang);
            hp_app::testing::run_ui(&mut state, |ui, state| {
                d.stats_ui(ui, state);
                MapInfoDialog::lightmaps_ui(ui, state);
            });
        }
        hp_app::i18n::set_lang(hp_app::i18n::Lang::En);
        for key in [
            "mapinfo.nav",
            "mapinfo.nav_areas_one",
            "mapinfo.nav_cut_off",
            "mapinfo.lightmaps",
            "mapinfo.lightmap_oversized_one",
            "mapinfo.lightmap_face",
        ] {
            assert!(hp_app::i18n::has_key(key), "{key}");
        }
        assert_eq!(tf!("mapinfo.nav_areas", "gm_x.nav", 120), "gm_x.nav: 120 areas");
        assert_eq!(
            tf!("mapinfo.lightmaps", fmt_bytes(36_000), fmt_bytes(32 * 1024 * 1024)),
            "35 KB of 32.0 MB of lighting data"
        );
    }

    #[test]
    fn texture_memory_of_mounted_materials() {
        let dir = crate::test_scratch("mapinfo_textures");
        let vmt = dir.join("materials/test/wall.vmt");
        std::fs::create_dir_all(vmt.parent().expect("parent")).expect("dir");
        std::fs::write(&vmt, b"\"LightmappedGeneric\" { \"$basetexture\" \"test/wall\" }").expect("vmt");
        std::fs::write(dir.join("materials/test/wall.vtf"), vec![0u8; 4096]).expect("vtf");
        let fs = hp_fs::GameFs::empty();
        fs.add_game_dir(&dir).expect("game dir");
        let m = texture_memory(&fs, vec!["TEST/WALL".to_owned(), "missing/thing".to_owned()]);
        assert_eq!(m.files, 1);
        assert_eq!(m.bytes, 4096);
        assert_eq!(m.largest[0].0, "materials/test/wall.vtf");
    }

    #[test]
    fn map_info_keeps_its_width() {
        let _lang = crate::lang_lock();
        crate::register_translations();
        hp_app::register_translations();
        let mut state = hp_app::testing::test_state();
        state.ui.show_map_info = true;
        for lang in [hp_app::i18n::Lang::En, hp_app::i18n::Lang::Fr] {
            hp_app::i18n::set_lang(lang);
            let mut d = MapInfoDialog::default();
            let ctx = egui::Context::default();
            hp_app::theme::apply(&ctx, hp_app::settings::ThemeChoice::Dark, 1.0);
            let mut widths = Vec::new();
            for _ in 0..40 {
                let input = egui::RawInput {
                    screen_rect: Some(egui::Rect::from_min_size(egui::Pos2::ZERO, egui::vec2(1920.0, 1017.0))),
                    ..Default::default()
                };
                let mut out = ctx.run_ui(input, |ui| d.show(ui.ctx(), &mut state));
                out.textures_delta.clear();
                if let Some(r) = ctx.memory(|m| m.area_rect(egui::Id::unique("hp_map_info_window"))) {
                    widths.push(r.width());
                }
            }
            let (first, last) = (widths[4], *widths.last().expect("shown"));
            hp_app::i18n::set_lang(hp_app::i18n::Lang::En);
            assert!(last <= first + 0.5, "{lang:?}: the window grew from {first} to {last}");
        }
    }
}
