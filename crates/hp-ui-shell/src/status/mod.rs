use egui::Ui;
use hp_app::AppState;
use hp_app::extend::{StatusContext, StatusFit, StatusSegment, StatusSide};
use hp_app::i18n::TrTable;

mod compile;
mod health;
mod launch;
mod lighting;
pub(crate) mod mcp;
mod shell;
mod startup;

pub use shell::{CURSOR_WIDTH, SIZE_LIMIT, StatusCache, cursor_text, fmt_len, fmt_point, fmt_size};
#[cfg(test)]
pub(crate) use startup::late_text;

pub(crate) const TABLES: &[TrTable] =
    &[shell::TR, startup::TR, compile::TR, health::TR, launch::TR, lighting::TR, mcp::TR];

fn default_segments() -> Vec<Box<dyn StatusSegment>> {
    let mut out = Vec::new();
    out.extend(shell::segments());
    out.extend(startup::segments());
    out.extend(compile::segments());
    out.extend(health::segments());
    out.extend(launch::segments());
    out.extend(lighting::segments());
    out.extend(mcp::segments());
    out
}

pub const SEGMENT_GAP: f32 = 13.0;

pub fn fit_widths(avail: f32, gap: f32, fits: &[Option<StatusFit>]) -> Vec<Option<f32>> {
    let avail = if avail.is_finite() { avail.max(0.0) } else { 0.0 };
    let gap = if gap.is_finite() { gap.max(0.0) } else { 0.0 };
    let mut shown: Vec<usize> = (0..fits.len()).filter(|&i| fits[i].is_some()).collect();
    let fit = |i: usize| fits[i].unwrap_or(StatusFit::fixed(0.0));
    let need = |shown: &[usize]| -> f32 {
        shown.iter().map(|&i| fit(i).min).sum::<f32>() + gap * shown.len().saturating_sub(1) as f32
    };
    while !shown.is_empty() && need(&shown) > avail {
        let Some(pos) = (0..shown.len()).rev().min_by_key(|&k| fit(shown[k]).priority) else { break };
        shown.remove(pos);
    }
    let mut out = vec![None; fits.len()];
    if shown.is_empty() {
        return out;
    }
    let mut extra = (avail - need(&shown)).max(0.0);
    let growth: f32 = shown.iter().map(|&i| fit(i)).filter(|f| !f.flex).map(|f| f.preferred - f.min).sum();
    let share = if growth > 0.0 { (extra / growth).min(1.0) } else { 0.0 };
    extra = (extra - growth).max(0.0);
    let flexes = shown.iter().filter(|&&i| fit(i).flex).count();
    for &i in &shown {
        let f = fit(i);
        out[i] = Some(if f.flex { f.min + extra / flexes as f32 } else { f.min + (f.preferred - f.min) * share });
    }
    out
}

#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub struct StatusOutput {
    pub open_settings: Option<&'static str>,
}

pub struct StatusBar {
    segments: Vec<Box<dyn StatusSegment>>,
    placed: Vec<(&'static str, egui::Rect)>,
}

impl Default for StatusBar {
    fn default() -> Self {
        Self { segments: default_segments(), placed: Vec::new() }
    }
}

impl std::fmt::Debug for StatusBar {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("StatusBar").field("segments", &self.segment_ids()).finish()
    }
}

impl StatusBar {
    pub fn add(&mut self, segment: Box<dyn StatusSegment>) {
        if !self.segments.iter().any(|s| s.id() == segment.id()) {
            self.segments.push(segment);
        }
    }

    pub fn segment_ids(&self) -> Vec<&'static str> {
        self.segments.iter().map(|s| s.id()).collect()
    }

    pub fn placed(&self) -> &[(&'static str, egui::Rect)] {
        &self.placed
    }

    fn side(&self, side: StatusSide) -> Vec<usize> {
        let mut idx: Vec<usize> = (0..self.segments.len()).filter(|&i| self.segments[i].side() == side).collect();
        idx.sort_by_key(|&i| self.segments[i].order());
        idx
    }

    pub fn ui(&mut self, ui: &mut Ui, state: &mut AppState, tool_hint: Option<String>) -> StatusOutput {
        let mut cx = StatusContext { tool_hint, open_settings: None };
        let right = self.side(StatusSide::Right);
        let left = self.side(StatusSide::Left);
        let segments = &mut self.segments;
        let placed = &mut self.placed;
        placed.clear();
        ui.horizontal(|ui| {
            ui.spacing_mut().item_spacing.x = 6.0;
            ui.with_layout(egui::Layout::right_to_left(egui::Align::Center), |ui| {
                for &i in &right {
                    segments[i].ui(ui, state, &mut cx);
                }
                ui.separator();
                let area = ui.available_rect_before_wrap();
                if area.width() <= 0.0 || area.height() <= 0.0 {
                    return;
                }
                ui.allocate_rect(area, egui::Sense::hover());
                let fits: Vec<Option<StatusFit>> = left.iter().map(|&i| segments[i].fit(ui, state)).collect();
                let widths = fit_widths(area.width(), SEGMENT_GAP, &fits);
                let stroke = ui.visuals().widgets.noninteractive.bg_stroke;
                let clip = ui.clip_rect();
                let mut x = area.left();
                let mut first = true;
                for (n, &i) in left.iter().enumerate() {
                    let Some(width) = widths[n] else { continue };
                    if !first {
                        let mid = (x + SEGMENT_GAP * 0.5).round() + 0.5;
                        ui.painter().with_clip_rect(clip).vline(mid, area.y_range().shrink(2.0), stroke);
                        x += SEGMENT_GAP;
                    }
                    first = false;
                    let rect = egui::Rect::from_min_size(egui::pos2(x, area.top()), egui::vec2(width, area.height()));
                    let mut child = ui.new_child(
                        egui::UiBuilder::new()
                            .id_salt(("hp_status_left", segments[i].id()))
                            .max_rect(rect)
                            .layout(egui::Layout::left_to_right(egui::Align::Center)),
                    );
                    child.set_clip_rect(rect.intersect(clip));
                    child.spacing_mut().item_spacing.x = 6.0;
                    segments[i].ui(&mut child, state, &mut cx);
                    placed.push((segments[i].id(), rect));
                    x += width;
                }
            });
        });
        StatusOutput { open_settings: cx.open_settings }
    }
}
