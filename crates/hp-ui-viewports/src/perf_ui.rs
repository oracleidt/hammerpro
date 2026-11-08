use egui::{Align2, CursorIcon, Pos2, Rect, Sense, vec2};
use hp_app::AppState;
use hp_app::i18n::TrTable;
use hp_app::runtime::PerfLevel;
use hp_app::{t, tf};

use crate::paint;

pub const TR: TrTable = &[
    ("primitives.memory_error", "RAM measurement unavailable (error {0})", "Mesure RAM indisponible (erreur {0})"),
    ("primitives.title", "Map primitives (editor)", "Primitives de la map (éditeur)"),
    ("primitives.world", "World: {0} solids, {1} materials, RAM {2}", "World : {0} solides, {1} matériaux, RAM {2}"),
    ("primitives.props", "Props: {0} materials, RAM {1}", "Props : {0} matériaux, RAM {1}"),
    ("primitives.lighting", "Lighting: {0} lights, RAM {1}", "Lighting : {0} lumières, RAM {1}"),
    ("primitives.shared", "Shared allocations: {0}", "Allocations partagées : {0}"),
    ("primitives.process", "Editor resident RAM: {0}", "RAM résidente de l'éditeur : {0}"),
    ("primitives.loading", "Measuring resource allocations...", "Mesure des allocations des ressources..."),
    ("primitives.models", "Models pending: {0}, missing: {1}", "Modèles en attente : {0}, manquants : {1}"),
    (
        "primitives.partial",
        "Some references are outside the measured heap",
        "Certaines références sont hors du heap mesuré",
    ),
    (
        "primitives.tip",
        "CPU allocations of the active map resources, measured from allocated Windows heap blocks including their reported overhead. Shared blocks are counted once. These are allocation sizes, not per-category resident pages. Driver, framework and undo-history memory are outside this breakdown. The total resident RAM is measured separately by the OS.",
        "Allocations CPU des ressources de la map active, mesurées dans les blocs alloués du heap Windows, avec leur surcharge rapportée. Les blocs partagés sont comptés une seule fois. Ce sont des tailles d'allocation, pas les pages résidentes par catégorie. La mémoire du pilote, de l'interface et de l'historique est hors de cette répartition. La RAM résidente totale est mesurée séparément par le système.",
    ),
    ("perf.hud_fps", "{0} FPS  {1} ms", "{0} i/s  {1} ms"),
    (
        "perf.hud_cpu",
        "CPU {0} ms  (prepare {1}, record {2}, submit {3})",
        "CPU {0} ms  (préparation {1}, enregistrement {2}, envoi {3})",
    ),
    (
        "perf.hud_gpu",
        "GPU {0} ms  (shadows {1}, scene {2}, post {3}, water {4})",
        "GPU {0} ms  (ombres {1}, scène {2}, post {3}, eau {4})",
    ),
    (
        "perf.hud_gpu_none",
        "GPU time not available on this graphics card",
        "Temps GPU indisponible sur cette carte graphique",
    ),
    ("perf.hud_mem", "RAM {0} MB  VRAM {1} / {2} MB", "RAM {0} Mo  VRAM {1} / {2} Mo"),
    ("perf.hud_gpu_name", "{0}  quality: {1}", "{0}  qualité : {1}"),
    (
        "perf.banner",
        "Quality lowered automatically ({0}, {1} FPS measured)",
        "Qualité baissée automatiquement ({0}, {1} i/s mesurées)",
    ),
    ("perf.restore", "Restore full quality", "Rétablir la qualité maximale"),
    (
        "perf.restore_tip",
        "Go back to the full quality preview. Automatic reduction stays paused until you change the performance mode.",
        "Revenir à l'aperçu en pleine qualité. La réduction automatique reste suspendue jusqu'à ce que vous changiez le mode de performance.",
    ),
];

fn ms(v: f32) -> String {
    format!("{v:.1}")
}

fn memory_text(bytes: u64) -> String {
    match bytes {
        0..1024 => format!("{bytes} B"),
        1024..1048576 => format!("{:.2} KiB", bytes as f64 / 1024.0),
        _ => format!("{:.2} MiB", bytes as f64 / (1024.0 * 1024.0)),
    }
}

pub fn primitive_lines(state: &AppState) -> Vec<String> {
    let mut lines = vec![t!("primitives.title").to_owned()];
    let Some(stats) = state.runtime.primitives.stats.as_ref() else {
        lines.push(t!("primitives.loading").to_owned());
        return lines;
    };
    let ram = |index: usize| stats.memory.map_or_else(|| "-".to_owned(), |memory| memory_text(memory.bytes[index]));
    lines.push(tf!("primitives.world", stats.solids, stats.world_materials, ram(0)));
    lines.push(tf!("primitives.props", stats.prop_materials, ram(1)));
    lines.push(tf!("primitives.lighting", stats.lights, ram(2)));
    lines.push(tf!("primitives.shared", ram(3)));
    let resident = state.runtime.render.perf.sample.working_set;
    lines.push(tf!("primitives.process", if resident > 0 { memory_text(resident) } else { "-".to_owned() }));
    if stats.models_loading > 0 || stats.models_missing > 0 {
        lines.push(tf!("primitives.models", stats.models_loading, stats.models_missing));
    }
    if stats.collecting {
        lines.push(t!("primitives.loading").to_owned());
    } else if let Some(error) = &stats.memory_error {
        lines.push(tf!("primitives.memory_error", error));
    } else if stats.memory.is_some_and(|memory| memory.matched_probes < memory.total_probes) {
        lines.push(t!("primitives.partial").to_owned());
    }
    lines
}

pub fn hud_lines(state: &AppState) -> Vec<String> {
    let rt = &state.runtime;
    let p = &rt.render.perf;
    let s = &p.sample;
    let mut lines = Vec::new();
    let fps = if s.frame_ms > 0.0 { 1000.0 / s.frame_ms } else { state.frame.fps };
    lines.push(tf!("perf.hud_fps", format!("{fps:.0}"), ms(s.frame_ms)));
    lines.push(tf!(
        "perf.hud_cpu",
        ms(s.prepare_ms + s.record_ms + s.submit_ms),
        ms(s.prepare_ms),
        ms(s.record_ms),
        ms(s.submit_ms)
    ));
    if s.gpu_supported {
        lines.push(tf!(
            "perf.hud_gpu",
            ms(s.gpu_total_ms + s.gpu_reflection_ms),
            ms(s.gpu_shadows_ms),
            ms(s.gpu_scene_ms),
            ms(s.gpu_post_ms),
            ms(s.gpu_reflection_ms)
        ));
    } else {
        lines.push(t!("perf.hud_gpu_none").to_owned());
    }
    lines.push(tf!("perf.hud_mem", s.working_set >> 20, rt.gpu.usage >> 20, rt.gpu.budget >> 20));
    let level = p.level(state.settings.render.performance_mode);
    lines.push(tf!("perf.hud_gpu_name", p.adapter, level.id()));
    lines
}

pub fn show(ui: &mut egui::Ui, state: &mut AppState, rect: Rect, top: f32, index: usize) -> Rect {
    let painter = ui.painter_at(rect);
    let mut taken = Rect::NOTHING;
    if state.settings.render.perf_hud {
        let galley = painter.layout_no_wrap(hud_lines(state).join("\n"), egui::FontId::monospace(11.0), paint::TEXT);
        let r = Align2::LEFT_TOP.anchor_size(Pos2::new(rect.left() + 6.0, top), galley.size() + vec2(10.0, 6.0));
        painter.rect_filled(r, 4, paint::PILL_BG);
        painter.galley(r.min + vec2(5.0, 3.0), galley, paint::TEXT);
        taken = r;
    }
    if state.settings.render.primitive_stats {
        ui.ctx().request_repaint_after(std::time::Duration::from_secs(1));
        let y = if taken.is_positive() { taken.bottom() + 6.0 } else { top };
        let galley =
            painter.layout_no_wrap(primitive_lines(state).join("\n"), egui::FontId::monospace(11.0), paint::TEXT);
        let r = Align2::LEFT_TOP.anchor_size(Pos2::new(rect.left() + 6.0, y), galley.size() + vec2(10.0, 6.0));
        painter.rect_filled(r, 4, paint::PILL_BG);
        painter.galley(r.min + vec2(5.0, 3.0), galley, paint::TEXT);
        ui.interact(r, ui.scope_id().with(("hp_primitives_hud", index)), Sense::HOVER)
            .on_hover_text(t!("primitives.tip"));
        taken = if taken.is_positive() { taken.union(r) } else { r };
    }
    let mode = state.settings.render.performance_mode;
    let perf = &state.runtime.render.perf;
    if perf.reduced_automatically(mode) && perf.auto_level != PerfLevel::Full {
        let text = tf!("perf.banner", perf.auto_level.id().replace('_', " "), format!("{:.0}", perf.auto_fps));
        let label = t!("perf.restore");
        let font = paint::small_font();
        let g1 = painter.layout_no_wrap(text, font.clone(), paint::TEXT);
        let g2 = painter.layout_no_wrap(label.to_owned(), font, egui::Color32::WHITE);
        let width = g1.size().x + g2.size().x + 36.0;
        let height = g1.size().y.max(g2.size().y) + 8.0;
        let r = Align2::CENTER_TOP
            .anchor_size(Pos2::new(rect.center().x, rect.bottom() - height - 8.0), vec2(width, height));
        painter.rect_filled(r, 4, paint::PILL_BG);
        painter.galley(r.min + vec2(6.0, 4.0), g1.clone(), paint::TEXT);
        let b = Rect::from_min_size(
            Pos2::new(r.min.x + 6.0 + g1.size().x + 12.0, r.min.y + 2.0),
            vec2(g2.size().x + 12.0, height - 4.0),
        );
        let response = ui.interact(b, ui.scope_id().with(("hp_perf_restore", index)), Sense::CLICK);
        let fill = if response.hovered() { hp_app::theme::ACCENT } else { hp_app::theme::ACCENT.gamma_multiply(0.7) };
        painter.rect_filled(b, 3, fill);
        painter.galley(b.min + vec2(6.0, 2.0), g2, egui::Color32::WHITE);
        let response = response.on_hover_text(t!("perf.restore_tip"));
        if response.hovered() {
            ui.ctx().set_cursor_icon(CursorIcon::PointingHand);
        }
        if response.clicked() {
            state.runtime.render.perf.restore_request = true;
        }
        taken = if taken.is_positive() { taken.union(r) } else { r };
    }
    taken
}
