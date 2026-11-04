use hp_app::runtime::ErosionDialog;
use hp_app::state::AppState;
use hp_app::{Tool, t, tf};

use super::commands::{noise_range_ui, targets};
use super::erode::{NoiseKind, TerrainOp, TerrainOptions, apply, hydraulic_grid, noise_ui, thermal_grid};

fn read(tool: &Option<&mut dyn Tool>) -> TerrainOptions {
    tool.as_ref().map(|t| TerrainOptions::from_values(&t.option_values())).unwrap_or_default()
}

fn write(state: &mut AppState, tool: &mut Option<&mut dyn Tool>, before: &TerrainOptions, after: &TerrainOptions) {
    let Some(tool) = tool.as_mut() else { return };
    let old = before.values();
    for new in after.values() {
        if old.iter().any(|o| o.key == new.key && o.value == new.value) {
            continue;
        }
        tool.set_option(state, new.key, &new.value);
    }
}

fn kind_label(kind: NoiseKind) -> &'static str {
    match kind {
        NoiseKind::Random => "disp.noise_random",
        NoiseKind::Fractal => "disp.noise_fractal",
    }
}

fn shell(
    ctx: &egui::Context,
    id: &'static str,
    title: impl Into<egui::WidgetText>,
    count: usize,
    body: impl FnOnce(&mut egui::Ui),
) -> (bool, bool, bool) {
    let mut open = true;
    let (mut apply_now, mut close) = (false, false);
    hp_app::chrome::Window::new(title)
        .id(egui::Id::unique(id))
        .pivot(egui::Align2::CENTER_CENTER)
        .default_pos(ctx.content_rect().center())
        .open(&mut open)
        .collapsible(false)
        .resizable(false)
        .show(ctx, |ui| {
            ui.label(egui::RichText::new(tf!("disp.noise_targets", count)).weak());
            body(ui);
            ui.horizontal(|ui| {
                if ui.add_enabled(count > 0, egui::Button::new(t!("common.apply"))).clicked() {
                    apply_now = true;
                }
                if ui.button(t!("common.close")).clicked() {
                    close = true;
                }
            });
        });
    (apply_now, close, open)
}

pub fn noise_window(ctx: &egui::Context, state: &mut AppState, mut tool: Option<&mut dyn Tool>) {
    if !state.runtime.mesh.noise_dialog {
        return;
    }
    let before = read(&tool);
    let mut opts = before.clone();
    let count = targets(&state.doc).len();
    let mut range_changed = false;
    let (go, close, open) = shell(ctx, "hp_disp_noise_window", t!("disp.noise_title"), count, |ui| {
        ui.horizontal(|ui| {
            for kind in [NoiseKind::Random, NoiseKind::Fractal] {
                if ui.add(egui::Button::selectable(opts.noise_kind == kind, t!(kind_label(kind)))).clicked() {
                    opts.noise_kind = kind;
                }
            }
        });
        range_changed = noise_range_ui(ui, state);
        if opts.noise_kind == NoiseKind::Fractal {
            noise_ui(ui, &mut opts);
        }
    });
    if range_changed {
        state.settings_dirty = true;
    }
    if opts != before {
        write(state, &mut tool, &before, &opts);
    }
    if go {
        apply(state, TerrainOp::Noise, &opts);
    }
    if !open || close {
        state.runtime.mesh.noise_dialog = false;
    }
}

pub fn erosion_window(ctx: &egui::Context, state: &mut AppState, mut tool: Option<&mut dyn Tool>) {
    let Some(which) = state.runtime.mesh.erosion_dialog else { return };
    let before = read(&tool);
    let mut opts = before.clone();
    let count = targets(&state.doc).len();
    let (title, op) = match which {
        ErosionDialog::Thermal => (t!("disp.thermal_title"), TerrainOp::Thermal),
        ErosionDialog::Hydraulic => (t!("disp.hydraulic_title"), TerrainOp::Hydraulic),
    };
    let (go, close, open) = shell(ctx, "hp_disp_erosion_window", title, count, |ui| match which {
        ErosionDialog::Thermal => thermal_grid(ui, &mut opts),
        ErosionDialog::Hydraulic => hydraulic_grid(ui, &mut opts),
    });
    if opts != before {
        write(state, &mut tool, &before, &opts);
    }
    if go {
        apply(state, op, &opts);
    }
    if !open || close {
        state.runtime.mesh.erosion_dialog = None;
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn only_changed_options_are_written_back() {
        let a = TerrainOptions::default();
        let mut b = a.clone();
        b.thermal_iterations = 33;
        b.noise_kind = NoiseKind::Fractal;
        let changed: Vec<&str> = b
            .values()
            .into_iter()
            .filter(|n| !a.values().iter().any(|o| o.key == n.key && o.value == n.value))
            .map(|n| n.key)
            .collect();
        assert_eq!(changed, vec!["noise_type", "thermal_iterations"]);
    }
}
