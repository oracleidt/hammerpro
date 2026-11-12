use egui::{Color32, Pos2, Rect, Sense, vec2};
use glam::DVec2;
use hp_app::state::AppState;
use hp_app::{Severity, t, tf};
use hp_editor::FaceRef;

use super::PixelGrid;
use super::image_io::{Channel, Pixels};
use super::masks::{
    Levels, MAX_MASK_FILES, MaskExportOptions, MaskOptions, MaskSource, MaskTarget, Prepared, UNDO_MASKS, WeightMode,
    apply_prepared, export_masks, face_bounds, load_all, prepare, preview_colors,
};
use crate::disp::commands::targets;

const THUMB: usize = 48;
const SETTLE_MS: u64 = 250;

struct Loaded {
    key: (Vec<String>, super::image_io::RawSpec),
    files: Result<Vec<Option<Pixels>>, String>,
    generation: u64,
}

type Thumb = (MaskOptions, u64, Result<Vec<[f32; 3]>, String>);

struct Live {
    key: (MaskOptions, Vec<FaceRef>),
    revision: u64,
}

#[derive(Default)]
pub struct MasksUi {
    pub import_open: bool,
    pub export_open: bool,
    pub import: MaskOptions,
    pub export: MaskExportOptions,
    pub live: bool,
    pub use_extent: bool,
    pub extent_min: DVec2,
    pub extent_size: DVec2,
    loaded: Option<Loaded>,
    preview: Option<Live>,
    settle: Option<((MaskOptions, Vec<FaceRef>), std::time::Instant)>,
    thumb: Option<Thumb>,
    generation: u64,
    last_error: Option<String>,
}

impl MasksUi {
    pub fn set_extent(&mut self, min: DVec2, size: DVec2) {
        self.use_extent = true;
        self.extent_min = min;
        self.extent_size = size;
    }

    fn options(&self) -> MaskOptions {
        let mut o = self.import.clone();
        o.extent = self.use_extent.then_some((self.extent_min, self.extent_size));
        o
    }

    fn drop_preview(&mut self, state: &mut AppState) {
        if let Some(p) = self.preview.take()
            && state.doc.revision() == p.revision
            && state.doc.undo_label() == Some(UNDO_MASKS)
        {
            state.doc.undo();
        }
    }

    pub fn close(&mut self, state: &mut AppState) {
        self.drop_preview(state);
        self.import_open = false;
    }
}

fn channel_label(c: Channel) -> &'static str {
    match c {
        Channel::R => t!("masks.ch_r"),
        Channel::G => t!("masks.ch_g"),
        Channel::B => t!("masks.ch_b"),
        Channel::A => t!("masks.ch_a"),
        Channel::Luma => t!("masks.ch_l"),
    }
}

fn source_label(s: MaskSource) -> String {
    match s {
        MaskSource::None => t!("masks.src_none").to_owned(),
        MaskSource::One => t!("masks.src_one").to_owned(),
        MaskSource::Rest => t!("masks.src_rest").to_owned(),
        MaskSource::Image { file, channel } => tf!("masks.src_image", file + 1, channel_label(channel)),
    }
}

fn source_combo(ui: &mut egui::Ui, id: &str, value: &mut MaskSource, files: usize, rest: bool) -> bool {
    let mut changed = false;
    egui::ComboBox::from_id_salt(id).selected_text(source_label(*value)).width(170.0).show_ui(ui, |ui| {
        let mut options = vec![MaskSource::None, MaskSource::One];
        if rest {
            options.push(MaskSource::Rest);
        }
        for f in 0..files.max(1) {
            for c in Channel::ALL {
                options.push(MaskSource::image(f, c));
            }
        }
        for o in options {
            if ui.selectable_label(*value == o, source_label(o)).clicked() && *value != o {
                *value = o;
                changed = true;
            }
        }
    });
    changed
}

fn levels_ui(ui: &mut egui::Ui, id: &str, l: &mut Levels) {
    ui.horizontal(|ui| {
        ui.checkbox(&mut l.invert, t!("masks.invert")).on_hover_text(t!("masks.invert_tip"));
        egui::CollapsingHeader::new(t!("masks.levels")).id_salt(format!("hp_masks_levels_{id}")).show(ui, |ui| {
            egui::Grid::new(format!("hp_masks_levels_grid_{id}")).num_columns(2).show(ui, |ui| {
                for (label, tip, v, lo, hi) in [
                    (t!("masks.black"), t!("masks.black_tip"), &mut l.black, -1.0, 2.0),
                    (t!("masks.white"), t!("masks.white_tip"), &mut l.white, -1.0, 2.0),
                    (t!("masks.gamma"), t!("masks.gamma_tip"), &mut l.gamma, 0.05, 20.0),
                    (t!("masks.contrast"), t!("masks.contrast_tip"), &mut l.contrast, 0.0, 20.0),
                ] {
                    ui.label(label);
                    ui.add(egui::DragValue::new(v).speed(0.01).range(lo..=hi).max_decimals(3)).on_hover_text(tip);
                    ui.end_row();
                }
            });
            if ui.small_button(t!("masks.reset_levels")).clicked() {
                *l = Levels { invert: l.invert, ..Levels::default() };
            }
        });
    });
}

fn browse(current: &str, save: Option<&str>) -> Option<String> {
    let mut dialog = rfd::FileDialog::new();
    if let Some(dir) = std::path::Path::new(current).parent().filter(|p| p.is_dir()) {
        dialog = dialog.set_directory(dir);
    }
    let picked = match save {
        Some(name) => dialog
            .add_filter(t!("masks.filter"), &["png", "tif", "tiff", "exr", "pfm", "r16", "r32", "raw"])
            .set_file_name(name)
            .save_file(),
        None => dialog
            .add_filter(
                t!("masks.filter"),
                &["png", "tif", "tiff", "exr", "pfm", "hdr", "tga", "bmp", "jpg", "jpeg", "raw", "r8", "r16", "r32"],
            )
            .pick_file(),
    };
    picked.map(|p| p.to_string_lossy().into_owned())
}

fn path_row(ui: &mut egui::Ui, label: &str, tip: &str, value: &mut String, save: Option<&str>) {
    ui.label(label);
    ui.horizontal(|ui| {
        ui.add(egui::TextEdit::singleline(value).desired_width(230.0)).on_hover_text(tip);
        if ui.button(t!("common.browse")).clicked()
            && let Some(p) = browse(value, save)
        {
            *value = p;
        }
    });
    ui.end_row();
}

fn refresh(mui: &mut MasksUi, state: &AppState) {
    let key = (mui.import.files.clone(), mui.import.raw);
    if mui.loaded.as_ref().is_some_and(|l| l.key == key) {
        return;
    }
    let files = load_all(state, &mui.import.files, &mui.import.raw);
    mui.generation += 1;
    mui.loaded = Some(Loaded { key, files, generation: mui.generation });
}

fn paint_thumb(ui: &mut egui::Ui, colors: &[[f32; 3]]) {
    let side = 200.0f32;
    let (rect, _) = ui.allocate_exact_size(vec2(side, side), Sense::hover());
    let painter = ui.painter_at(rect);
    painter.rect_filled(rect, 0.0, Color32::from_gray(30));
    let cell = side / THUMB as f32;
    for (k, c) in colors.iter().enumerate() {
        let (i, j) = (k % THUMB, k / THUMB);
        let to = |v: f32| (v.clamp(0.0, 1.0) * 255.0) as u8;
        painter.rect_filled(
            Rect::from_min_size(
                Pos2::new(rect.left() + i as f32 * cell, rect.top() + j as f32 * cell),
                vec2(cell + 0.5, cell + 0.5),
            ),
            0.0,
            Color32::from_rgb(to(c[0]), to(c[1]), to(c[2])),
        );
    }
    painter.text(
        rect.center_top() + vec2(0.0, 2.0),
        egui::Align2::CENTER_TOP,
        "N",
        egui::FontId::proportional(12.0),
        Color32::WHITE,
    );
}

fn prepared_now(mui: &MasksUi, opts: &MaskOptions) -> Result<Prepared, String> {
    match mui.loaded.as_ref().map(|l| &l.files) {
        Some(Ok(files)) => prepare(files, opts),
        Some(Err(e)) => Err(e.clone()),
        None => Err(t!("masks.no_files").to_owned()),
    }
}

fn import_window(ctx: &egui::Context, mui: &mut MasksUi, state: &mut AppState, heightmap: Option<(DVec2, DVec2)>) {
    if !mui.import_open {
        return;
    }
    refresh(mui, state);
    let faces = targets(&state.doc);
    let mut open = true;
    let mut apply = false;
    let mut close = false;
    let file_count = mui.import.files.len();
    hp_app::chrome::Window::new(t!("masks.import_title"))
        .id(egui::Id::unique("hp_masks_import_window"))
        .default_pos(ctx.content_rect().center() - vec2(240.0, 320.0))
        .default_size([480.0, 660.0])
        .open(&mut open)
        .collapsible(false)
        .resizable(true)
        .show(ctx, |ui| {
            egui::ScrollArea::vertical().max_height(580.0).show(ui, |ui| {
                ui.label(egui::RichText::new(tf!("masks.targets", faces.len())).weak());
                ui.label(egui::RichText::new(t!("masks.files")).strong());
                egui::Grid::new("hp_masks_files").num_columns(2).spacing([8.0, 4.0]).show(ui, |ui| {
                    for (i, f) in mui.import.files.iter_mut().enumerate() {
                        path_row(ui, &tf!("masks.file_n", i + 1), t!("masks.file_tip"), f, None);
                    }
                });
                ui.horizontal(|ui| {
                    if mui.import.files.len() < MAX_MASK_FILES && ui.small_button(t!("masks.add_file")).clicked() {
                        mui.import.files.push(String::new());
                    }
                    if mui.import.files.len() > 1 && ui.small_button(t!("masks.remove_file")).clicked() {
                        mui.import.files.pop();
                    }
                });
                if let Some(l) = &mui.loaded {
                    match &l.files {
                        Ok(files) => {
                            for (i, p) in files.iter().enumerate() {
                                if let Some(p) = p {
                                    ui.label(egui::RichText::new(tf!("masks.file_info", i + 1, p.describe())).weak());
                                }
                            }
                        }
                        Err(e) => {
                            ui.colored_label(hp_app::theme::error(), e);
                        }
                    }
                }
                ui.separator();
                ui.label(egui::RichText::new(t!("masks.target")).strong());
                ui.horizontal_wrapped(|ui| {
                    for (v, label, tip) in [
                        (MaskTarget::Auto, t!("masks.target_auto"), t!("masks.target_auto_tip")),
                        (MaskTarget::Alpha, t!("masks.target_alpha"), t!("masks.target_alpha_tip")),
                        (MaskTarget::FourWay, t!("masks.target_four"), t!("masks.target_four_tip")),
                    ] {
                        if ui.add(egui::Button::selectable(mui.import.target == v, label)).on_hover_text(tip).clicked()
                        {
                            mui.import.target = v;
                        }
                    }
                });
                let o = &mut mui.import;
                if o.target != MaskTarget::FourWay {
                    ui.label(egui::RichText::new(t!("masks.alpha_section")).strong());
                    ui.horizontal(|ui| {
                        ui.label(t!("masks.alpha_source")).on_hover_text(t!("masks.alpha_source_tip"));
                        source_combo(ui, "hp_masks_alpha", &mut o.alpha.source, file_count, false);
                    });
                    levels_ui(ui, "alpha", &mut o.alpha.levels);
                }
                if o.target != MaskTarget::Alpha {
                    ui.label(egui::RichText::new(t!("masks.four_section")).strong());
                    for (i, layer) in o.layers.iter_mut().enumerate() {
                        ui.horizontal(|ui| {
                            ui.label(tf!("masks.texture_n", i + 1));
                            source_combo(ui, &format!("hp_masks_layer_{i}"), &mut layer.source, file_count, true);
                        });
                        levels_ui(ui, &format!("layer{i}"), &mut layer.levels);
                    }
                    ui.checkbox(&mut o.normalize, t!("masks.normalize")).on_hover_text(t!("masks.normalize_tip"));
                    ui.horizontal(|ui| {
                        for (v, label, tip) in [
                            (WeightMode::Coverage, t!("masks.weights_coverage"), t!("masks.weights_coverage_tip")),
                            (WeightMode::Direct, t!("masks.weights_direct"), t!("masks.weights_direct_tip")),
                        ] {
                            if ui.add(egui::Button::selectable(o.weights == v, label)).on_hover_text(tip).clicked() {
                                o.weights = v;
                            }
                        }
                    });
                }
                ui.separator();
                ui.label(egui::RichText::new(t!("masks.placement")).strong());
                if ui
                    .checkbox(&mut mui.use_extent, t!("masks.use_extent"))
                    .on_hover_text(t!("masks.use_extent_tip"))
                    .changed()
                    && mui.use_extent
                    && mui.extent_size.min_element() < 1.0
                {
                    let (min, size) =
                        face_bounds(&state.doc, &faces).unwrap_or((DVec2::splat(-512.0), DVec2::splat(1024.0)));
                    mui.set_extent(min, size);
                }
                if mui.use_extent {
                    egui::Grid::new("hp_masks_extent").num_columns(2).spacing([8.0, 4.0]).show(ui, |ui| {
                        for (label, v, lo, hi) in [
                            (t!("masks.min_x"), &mut mui.extent_min.x, -16384.0, 16384.0),
                            (t!("masks.min_y"), &mut mui.extent_min.y, -16384.0, 16384.0),
                            (t!("masks.size_x"), &mut mui.extent_size.x, 1.0, 32768.0),
                            (t!("masks.size_y"), &mut mui.extent_size.y, 1.0, 32768.0),
                        ] {
                            ui.label(label);
                            ui.add(egui::DragValue::new(v).speed(8.0).range(lo..=hi));
                            ui.end_row();
                        }
                    });
                }
                if let Some((min, size)) = heightmap
                    && ui.button(t!("masks.from_heightmap")).on_hover_text(t!("masks.from_heightmap_tip")).clicked()
                {
                    mui.set_extent(min, size);
                }
                let o = &mut mui.import;
                ui.horizontal_wrapped(|ui| {
                    ui.checkbox(&mut o.flip_x, t!("terrain.flip_x")).on_hover_text(t!("terrain.flip_tip"));
                    ui.checkbox(&mut o.flip_y, t!("terrain.flip_y")).on_hover_text(t!("terrain.flip_tip"));
                    ui.checkbox(&mut o.bilinear, t!("masks.bilinear")).on_hover_text(t!("masks.bilinear_tip"));
                });
                ui.horizontal(|ui| {
                    ui.label(t!("terrain.pixel_grid")).on_hover_text(t!("terrain.pixel_grid_tip"));
                    for (v, label) in
                        [(PixelGrid::Corner, t!("terrain.grid_corner")), (PixelGrid::Center, t!("terrain.grid_center"))]
                    {
                        if ui.add(egui::Button::selectable(o.grid == v, label)).clicked() {
                            o.grid = v;
                        }
                    }
                });
                ui.separator();
                ui.label(egui::RichText::new(t!("terrain.preview")).strong());
                let opts = mui.options();
                let generation = mui.loaded.as_ref().map_or(0, |l| l.generation);
                if mui.thumb.as_ref().is_none_or(|t| t.0 != opts || t.1 != generation) {
                    let four = opts.target != MaskTarget::Alpha;
                    let colors = prepared_now(mui, &opts).map(|p| preview_colors(&p, &opts, THUMB, four));
                    mui.thumb = Some((opts.clone(), generation, colors));
                }
                match mui.thumb.as_ref().map(|t| &t.2) {
                    Some(Ok(colors)) => {
                        paint_thumb(ui, colors);
                        if opts.target != MaskTarget::Alpha {
                            ui.label(egui::RichText::new(t!("masks.thumb_legend")).weak());
                        }
                    }
                    Some(Err(e)) => {
                        ui.colored_label(hp_app::theme::warning(), e);
                    }
                    None => {}
                }
                if let Some(e) = &mui.last_error {
                    ui.colored_label(hp_app::theme::error(), e);
                }
            });
            ui.separator();
            ui.checkbox(&mut mui.live, t!("masks.live")).on_hover_text(t!("masks.live_tip"));
            ui.horizontal(|ui| {
                let ready = !faces.is_empty() && mui.options().validate().is_ok();
                if ui
                    .add_enabled(ready, egui::Button::new(t!("masks.apply")))
                    .on_hover_text(t!("masks.apply_tip"))
                    .clicked()
                {
                    apply = true;
                }
                if ui.button(t!("common.close")).clicked() {
                    close = true;
                }
            });
        });
    let opts = mui.options();
    let key = (opts.clone(), faces.clone());
    if apply {
        let same = mui.preview.as_ref().is_some_and(|p| p.key == key && p.revision == state.doc.revision());
        if same {
            mui.preview = None;
            state.notify(Severity::Success, t!("masks.applied_preview"));
        } else {
            mui.drop_preview(state);
            match prepared_now(mui, &opts).and_then(|p| apply_prepared(state, &faces, &opts, &p, UNDO_MASKS)) {
                Ok(r) => {
                    mui.last_error = None;
                    state.notify(Severity::Success, tf!("masks.applied", r.faces, r.alpha_faces, r.four_way_faces));
                }
                Err(e) => state.notify(Severity::Warning, tf!("masks.failed", e)),
            }
        }
        return;
    }
    if !open || close {
        mui.close(state);
        return;
    }
    if mui.live && !faces.is_empty() && opts.validate().is_ok() {
        let fresh = mui.preview.as_ref().is_some_and(|p| p.key == key && p.revision == state.doc.revision());
        let waited = match &mui.settle {
            Some((k, since)) if *k == key => since.elapsed() >= std::time::Duration::from_millis(SETTLE_MS),
            _ => {
                mui.settle = Some((key.clone(), std::time::Instant::now()));
                false
            }
        };
        if !fresh && !waited {
            ctx.request_repaint_after(std::time::Duration::from_millis(SETTLE_MS));
        } else if !fresh {
            mui.drop_preview(state);
            let faces = targets(&state.doc);
            let key = (opts.clone(), faces.clone());
            match prepared_now(mui, &opts).and_then(|p| apply_prepared(state, &faces, &opts, &p, UNDO_MASKS)) {
                Ok(_) => {
                    mui.last_error = None;
                    mui.preview = Some(Live { key, revision: state.doc.revision() });
                }
                Err(e) => {
                    mui.last_error = Some(e);
                    mui.preview = None;
                    mui.live = false;
                }
            }
        }
    } else if !mui.live {
        mui.drop_preview(state);
    }
}

fn export_window(ctx: &egui::Context, mui: &mut MasksUi, state: &mut AppState) {
    if !mui.export_open {
        return;
    }
    let faces = targets(&state.doc);
    let mut open = true;
    let mut apply = false;
    let mut close = false;
    hp_app::chrome::Window::new(t!("masks.export_title"))
        .id(egui::Id::unique("hp_masks_export_window"))
        .default_pos(ctx.content_rect().center() - vec2(240.0, 160.0))
        .open(&mut open)
        .collapsible(false)
        .resizable(false)
        .show(ctx, |ui| {
            ui.label(egui::RichText::new(tf!("terrain.export_targets", faces.len())).weak());
            let o = &mut mui.export;
            egui::Grid::new("hp_masks_export").num_columns(2).spacing([8.0, 4.0]).show(ui, |ui| {
                path_row(ui, t!("masks.export_rgba"), t!("masks.export_rgba_tip"), &mut o.path, Some("splat.png"));
                path_row(
                    ui,
                    t!("masks.export_alpha"),
                    t!("masks.export_alpha_tip"),
                    &mut o.alpha_path,
                    Some("alpha.png"),
                );
                for (i, p) in o.layer_paths.iter_mut().enumerate() {
                    path_row(ui, &tf!("masks.export_layer", i + 1), t!("masks.export_layer_tip"), p, Some("layer.png"));
                }
            });
            ui.horizontal(|ui| {
                for (v, label, tip) in [
                    (WeightMode::Coverage, t!("masks.weights_coverage"), t!("masks.weights_coverage_tip")),
                    (WeightMode::Direct, t!("masks.weights_direct"), t!("masks.weights_direct_tip")),
                ] {
                    if ui.add(egui::Button::selectable(o.weights == v, label)).on_hover_text(tip).clicked() {
                        o.weights = v;
                    }
                }
            });
            ui.checkbox(&mut o.float, t!("masks.export_float")).on_hover_text(t!("masks.export_float_tip"));
            ui.horizontal(|ui| {
                let ready = !faces.is_empty() && !mui.export.paths().is_empty();
                if ui
                    .add_enabled(ready, egui::Button::new(t!("terrain.export_run")))
                    .on_hover_text(t!("masks.export_run_tip"))
                    .clicked()
                {
                    apply = true;
                }
                if ui.button(t!("common.close")).clicked() {
                    close = true;
                }
            });
        });
    if apply {
        match export_masks(state, &faces, &mui.export) {
            Ok(r) => state.notify(Severity::Success, tf!("masks.exported", r.files.len(), r.width, r.height)),
            Err(e) => state.notify(Severity::Warning, tf!("masks.failed", e)),
        }
    }
    if !open || close {
        mui.export_open = false;
    }
}

pub fn show(ctx: &egui::Context, mui: &mut MasksUi, state: &mut AppState, heightmap: Option<(DVec2, DVec2)>) {
    import_window(ctx, mui, state, heightmap);
    export_window(ctx, mui, state);
}
