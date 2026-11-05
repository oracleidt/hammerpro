use std::ops::RangeInclusive;

use egui::{Color32, Pos2, Rect, Sense, Stroke, vec2};
use hp_app::state::AppState;
use hp_app::{Severity, t, tf};
use hp_geom::{FalloffShape, HeightField, NoiseBasis, NoiseShape, TerrainPreset};

use super::export::{ExportOptions, export};
use super::generate::{GenOptions, generate, preview};
use super::image_io::{is_raw, load_heightmap};
use super::import::{ImportOptions, import, shape_field};
use super::{TexMode, TileSetup};
use crate::disp::commands::targets;

const THUMB: usize = 48;

struct Thumb {
    key: String,
    result: Result<HeightField, String>,
    dims: (usize, usize),
}

#[derive(Default)]
pub struct TerrainUi {
    pub import_open: bool,
    pub generate_open: bool,
    pub export_open: bool,
    pub import: ImportOptions,
    pub generate: GenOptions,
    pub export: ExportOptions,
    pub live_preview: Option<bool>,
    thumb: Option<Thumb>,
    last_preview: Option<(GenOptions, u64)>,
    version: u64,
}

impl TerrainUi {
    pub fn close_generate(&mut self, state: &mut AppState) {
        self.generate_open = false;
        self.last_preview = None;
        state.runtime.mesh.terrain_preview = None;
    }
}

fn drag(ui: &mut egui::Ui, label: &str, tip: &str, v: &mut f64, range: RangeInclusive<f64>, speed: f64) -> bool {
    ui.label(label);
    let changed =
        ui.add(egui::DragValue::new(v).speed(speed).range(range).max_decimals(3)).on_hover_text(tip).changed();
    ui.end_row();
    changed
}

fn whole(ui: &mut egui::Ui, label: &str, tip: &str, v: &mut u32, range: RangeInclusive<u32>) -> bool {
    ui.label(label);
    let changed = ui.add(egui::DragValue::new(v).range(range)).on_hover_text(tip).changed();
    ui.end_row();
    changed
}

fn check(ui: &mut egui::Ui, label: &str, tip: &str, v: &mut bool) -> bool {
    ui.label("");
    let changed = ui.checkbox(v, label).on_hover_text(tip).changed();
    ui.end_row();
    changed
}

fn choice<T: Copy + PartialEq>(ui: &mut egui::Ui, value: &mut T, options: &[(T, &str)]) -> bool {
    let mut changed = false;
    ui.horizontal_wrapped(|ui| {
        for (v, label) in options {
            if ui.add(egui::Button::selectable(*value == *v, *label)).clicked() && *value != *v {
                *value = *v;
                changed = true;
            }
        }
    });
    changed
}

#[derive(Clone, Copy)]
enum Pick {
    OpenImage,
    SaveHeightmap,
    SaveSplat,
}

fn browse(kind: Pick, current: &str) -> Option<String> {
    let mut dialog = rfd::FileDialog::new();
    if let Some(dir) = std::path::Path::new(current).parent().filter(|p| p.is_dir()) {
        dialog = dialog.set_directory(dir);
    }
    let picked = match kind {
        Pick::OpenImage => dialog
            .add_filter(
                t!("terrain.filter_images"),
                &["png", "tif", "tiff", "exr", "hdr", "raw", "r16", "jpg", "jpeg", "bmp", "tga"],
            )
            .pick_file(),
        Pick::SaveHeightmap => dialog
            .add_filter(t!("terrain.filter_heightmap"), &["png", "tif", "tiff", "raw"])
            .set_file_name("heightmap.png")
            .save_file(),
        Pick::SaveSplat => dialog.add_filter("PNG", &["png"]).set_file_name("splat.png").save_file(),
    };
    picked.map(|p| p.to_string_lossy().into_owned())
}

fn path_row(ui: &mut egui::Ui, label: &str, tip: &str, value: &mut String, kind: Pick) -> bool {
    let mut changed = false;
    ui.label(label);
    ui.horizontal(|ui| {
        changed |= ui.add(egui::TextEdit::singleline(value).desired_width(220.0)).on_hover_text(tip).changed();
        if ui.button(t!("common.browse")).clicked()
            && let Some(p) = browse(kind, value)
        {
            *value = p;
            changed = true;
        }
    });
    ui.end_row();
    changed
}

fn text_row(ui: &mut egui::Ui, label: &str, tip: &str, value: &mut String) -> bool {
    ui.label(label);
    let changed = ui.add(egui::TextEdit::singleline(value).desired_width(220.0)).on_hover_text(tip).changed();
    ui.end_row();
    changed
}

fn setup_ui(ui: &mut egui::Ui, id: &str, s: &mut TileSetup, with_size: bool) {
    egui::Grid::new(format!("hp_terrain_setup_{id}")).num_columns(2).spacing([8.0, 4.0]).show(ui, |ui| {
        if with_size {
            drag(ui, t!("terrain.size_x"), t!("terrain.size_tip"), &mut s.size.x, 16.0..=32768.0, 16.0);
            drag(ui, t!("terrain.size_y"), t!("terrain.size_tip"), &mut s.size.y, 16.0..=32768.0, 16.0);
            drag(ui, t!("terrain.origin_x"), t!("terrain.origin_tip"), &mut s.origin.x, -16384.0..=16384.0, 8.0);
            drag(ui, t!("terrain.origin_y"), t!("terrain.origin_tip"), &mut s.origin.y, -16384.0..=16384.0, 8.0);
            check(ui, t!("terrain.centered"), t!("terrain.centered_tip"), &mut s.centered);
        }
        drag(ui, t!("terrain.origin_z"), t!("terrain.origin_z_tip"), &mut s.origin.z, -16384.0..=16384.0, 8.0);
        drag(ui, t!("terrain.h_min"), t!("terrain.h_min_tip"), &mut s.h_min, -16384.0..=16384.0, 4.0);
        drag(ui, t!("terrain.h_max"), t!("terrain.h_max_tip"), &mut s.h_max, -16384.0..=16384.0, 4.0);
        ui.label(t!("terrain.h_scale"));
        ui.label(format!("{:.1}", s.h_max - s.h_min));
        ui.end_row();
        if with_size {
            ui.label(t!("terrain.tile_power"));
            ui.horizontal(|ui| {
                for p in 2..=4u32 {
                    if ui
                        .add(egui::Button::selectable(s.tile_power == p, p.to_string()))
                        .on_hover_text(t!("terrain.tile_power_tip"))
                        .clicked()
                    {
                        s.tile_power = p;
                    }
                }
            });
            ui.end_row();
            drag(ui, t!("terrain.tile_size"), t!("terrain.tile_size_tip"), &mut s.tile_size, 16.0..=16384.0, 16.0);
        }
        drag(ui, t!("terrain.skirt"), t!("terrain.skirt_tip"), &mut s.skirt, 0.0..=4096.0, 1.0);
        drag(ui, t!("terrain.thickness"), t!("terrain.thickness_tip"), &mut s.thickness, 1.0..=4096.0, 1.0);
        text_row(ui, t!("terrain.material"), t!("terrain.material_tip"), &mut s.material);
        let mut lm = s.lightmap_scale.max(1) as u32;
        if whole(ui, t!("terrain.lightmap"), t!("terrain.lightmap_tip"), &mut lm, 1..=1024) {
            s.lightmap_scale = lm as i32;
        }
    });
    if with_size {
        let (tx, ty) = s.tiles();
        let sp = s.spacing();
        ui.label(egui::RichText::new(tf!("terrain.layout", tx, ty, tx * ty, format!("{:.1}", sp.x.min(sp.y)))).weak());
        if let Err(e) = s.validate() {
            ui.colored_label(hp_app::theme::error(), e);
        }
    }
}

fn auto_ui(ui: &mut egui::Ui, id: &str, a: &mut hp_geom::AutoTexture) {
    egui::Grid::new(format!("hp_terrain_auto_{id}")).num_columns(2).spacing([8.0, 4.0]).show(ui, |ui| {
        drag(ui, t!("terrain.rock_slope"), t!("terrain.rock_slope_tip"), &mut a.rock_slope, 0.0..=89.0, 0.5);
        drag(ui, t!("terrain.rock_blend"), t!("terrain.blend_tip"), &mut a.rock_blend, 0.0..=45.0, 0.5);
        drag(ui, t!("terrain.snow_height"), t!("terrain.snow_height_tip"), &mut a.snow_height, 0.0..=1.0, 0.01);
        drag(ui, t!("terrain.snow_blend"), t!("terrain.blend_tip"), &mut a.snow_blend, 0.0..=1.0, 0.01);
        drag(ui, t!("terrain.sand_height"), t!("terrain.sand_height_tip"), &mut a.sand_height, 0.0..=1.0, 0.01);
        drag(ui, t!("terrain.sand_blend"), t!("terrain.blend_tip"), &mut a.sand_blend, 0.0..=1.0, 0.01);
    });
    ui.label(egui::RichText::new(t!("terrain.auto_channels")).weak());
}

fn refresh_thumb(ui: &mut TerrainUi, state: &AppState) {
    let o = &ui.import;
    let key = format!("{}|{}|{}|{}", o.path, o.raw.width, o.raw.height, o.raw.big_endian);
    if ui.thumb.as_ref().is_some_and(|t| t.key == key) {
        return;
    }
    let result = super::resolve_path(state, &o.path).and_then(|p| load_heightmap(&p, &o.raw));
    let dims = result.as_ref().map_or((0, 0), |f| (f.width, f.height));
    let result = result.map(|f| f.resampled(THUMB, THUMB));
    ui.thumb = Some(Thumb { key, result, dims });
}

fn paint_layout(ui: &mut egui::Ui, thumb: Option<&HeightField>, o: &ImportOptions) {
    let size = o.setup.size;
    let side = 240.0f32;
    let aspect = (size.x / size.y.max(1.0)) as f32;
    let dims = if aspect >= 1.0 { vec2(side, side / aspect.max(0.05)) } else { vec2(side * aspect.max(0.05), side) };
    let (rect, _) = ui.allocate_exact_size(dims, Sense::hover());
    let painter = ui.painter_at(rect);
    painter.rect_filled(rect, 0.0, Color32::from_gray(30));
    if let Some(small) = thumb {
        let shaped = shape_field(small.clone(), o);
        let (cw, ch) = (rect.width() / THUMB as f32, rect.height() / THUMB as f32);
        for j in 0..THUMB {
            for i in 0..THUMB {
                let v = shaped.at(i as isize, j as isize).clamp(0.0, 1.0);
                let g = (v * 255.0) as u8;
                let x = rect.left() + i as f32 * cw;
                let y = rect.bottom() - (j + 1) as f32 * ch;
                painter.rect_filled(
                    Rect::from_min_size(Pos2::new(x, y), vec2(cw + 0.5, ch + 0.5)),
                    0.0,
                    Color32::from_gray(g),
                );
            }
        }
    }
    let (tx, ty) = o.setup.tiles();
    let line = Stroke::new(1.0, hp_app::data_colors::TILE_GRID);
    for i in 0..=tx {
        let x = rect.left() + rect.width() * i as f32 / tx as f32;
        painter.line_segment([Pos2::new(x, rect.top()), Pos2::new(x, rect.bottom())], line);
    }
    for j in 0..=ty {
        let y = rect.top() + rect.height() * j as f32 / ty as f32;
        painter.line_segment([Pos2::new(rect.left(), y), Pos2::new(rect.right(), y)], line);
    }
    painter.text(
        rect.center_top() + vec2(0.0, 2.0),
        egui::Align2::CENTER_TOP,
        "N",
        egui::FontId::proportional(12.0),
        Color32::WHITE,
    );
}

fn import_window(ctx: &egui::Context, tui: &mut TerrainUi, state: &mut AppState) {
    if !tui.import_open {
        return;
    }
    refresh_thumb(tui, state);
    let mut open = true;
    let mut apply = false;
    let mut close = false;
    hp_app::chrome::Window::new(t!("terrain.import_title"))
        .id(egui::Id::unique("hp_terrain_import_window"))
        .default_pos(ctx.content_rect().center() - vec2(230.0, 300.0))
        .default_size([460.0, 640.0])
        .open(&mut open)
        .collapsible(false)
        .resizable(true)
        .show(ctx, |ui| {
            egui::ScrollArea::vertical().max_height(560.0).show(ui, |ui| {
                let o = &mut tui.import;
                egui::Grid::new("hp_terrain_import_file").num_columns(2).spacing([8.0, 4.0]).show(ui, |ui| {
                    path_row(ui, t!("terrain.file"), t!("terrain.file_tip"), &mut o.path, Pick::OpenImage);
                    if is_raw(std::path::Path::new(o.path.trim())) {
                        whole(ui, t!("terrain.raw_width"), t!("terrain.raw_size_tip"), &mut o.raw.width, 0..=16384);
                        whole(ui, t!("terrain.raw_height"), t!("terrain.raw_size_tip"), &mut o.raw.height, 0..=16384);
                        check(ui, t!("terrain.raw_big"), t!("terrain.raw_big_tip"), &mut o.raw.big_endian);
                    }
                });
                match tui.thumb.as_ref().map(|t| (&t.result, t.dims)) {
                    Some((Ok(_), (w, h))) => {
                        ui.label(egui::RichText::new(tf!("terrain.image_info", w, h)).weak());
                    }
                    Some((Err(e), _)) if !tui.import.path.trim().is_empty() => {
                        ui.colored_label(hp_app::theme::error(), e);
                    }
                    _ => {}
                }
                ui.separator();
                let o = &mut tui.import;
                setup_ui(ui, "import", &mut o.setup, true);
                ui.separator();
                ui.label(egui::RichText::new(t!("terrain.image_section")).strong());
                egui::Grid::new("hp_terrain_import_image").num_columns(2).spacing([8.0, 4.0]).show(ui, |ui| {
                    check(ui, t!("terrain.invert"), t!("terrain.invert_tip"), &mut o.invert);
                    check(ui, t!("terrain.flip_x"), t!("terrain.flip_tip"), &mut o.flip_x);
                    check(ui, t!("terrain.flip_y"), t!("terrain.flip_tip"), &mut o.flip_y);
                    let mut cubic = !o.bilinear;
                    if check(ui, t!("terrain.bicubic"), t!("terrain.bicubic_tip"), &mut cubic) {
                        o.bilinear = !cubic;
                    }
                    whole(ui, t!("terrain.smoothing"), t!("terrain.smoothing_tip"), &mut o.smoothing, 0..=64);
                });
                ui.separator();
                ui.label(egui::RichText::new(t!("terrain.texturing")).strong());
                choice(
                    ui,
                    &mut o.tex.mode,
                    &[
                        (TexMode::None, t!("terrain.tex_none")),
                        (TexMode::Auto, t!("terrain.tex_auto")),
                        (TexMode::Splat, t!("terrain.tex_splat")),
                    ],
                );
                match o.tex.mode {
                    TexMode::None => {}
                    TexMode::Auto => auto_ui(ui, "import", &mut o.tex.auto),
                    TexMode::Splat => {
                        egui::Grid::new("hp_terrain_splat").num_columns(2).spacing([8.0, 4.0]).show(ui, |ui| {
                            path_row(
                                ui,
                                t!("terrain.splat_file"),
                                t!("terrain.splat_file_tip"),
                                &mut o.tex.splat,
                                Pick::OpenImage,
                            );
                            for (i, c) in o.tex.channels.iter_mut().enumerate() {
                                let label = tf!("terrain.splat_channel", i + 1);
                                path_row(ui, &label, t!("terrain.splat_channel_tip"), c, Pick::OpenImage);
                            }
                        });
                    }
                }
                ui.separator();
                ui.label(egui::RichText::new(t!("terrain.preview")).strong());
                let small = tui.thumb.as_ref().and_then(|t| t.result.as_ref().ok());
                paint_layout(ui, small, &tui.import);
            });
            ui.separator();
            ui.horizontal(|ui| {
                let ready = tui.import.setup.validate().is_ok() && !tui.import.path.trim().is_empty();
                if ui
                    .add_enabled(ready, egui::Button::new(t!("terrain.import_run")))
                    .on_hover_text(t!("terrain.import_run_tip"))
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
        match import(state, &tui.import) {
            Ok(r) => state.notify(Severity::Success, tf!("terrain.imported", r.tiles, r.image.0, r.image.1)),
            Err(e) => state.notify(Severity::Warning, tf!("terrain.failed", e)),
        }
    }
    if !open || close {
        tui.import_open = false;
    }
}

fn generate_ui(ui: &mut egui::Ui, g: &mut GenOptions, selected: usize) -> bool {
    let before = g.clone();
    ui.horizontal_wrapped(|ui| {
        for p in TerrainPreset::ALL {
            let label = t!(match p {
                TerrainPreset::RollingHills => "terrain.preset_hills",
                TerrainPreset::Mountains => "terrain.preset_mountains",
                TerrainPreset::Canyon => "terrain.preset_canyon",
                TerrainPreset::Islands => "terrain.preset_islands",
                TerrainPreset::Dunes => "terrain.preset_dunes",
                TerrainPreset::Plateau => "terrain.preset_plateau",
            });
            if ui.add(egui::Button::selectable(g.preset == p, label)).on_hover_text(t!("terrain.preset_tip")).clicked()
            {
                g.apply_preset(p);
            }
        }
    });
    ui.horizontal(|ui| {
        let mut seed = g.params.seed.min(u64::from(u32::MAX)) as u32;
        ui.label(t!("terrain.seed"));
        if ui.add(egui::DragValue::new(&mut seed)).on_hover_text(t!("terrain.seed_tip")).changed() {
            g.params.seed = u64::from(seed);
        }
        if ui.button(t!("terrain.randomize")).on_hover_text(t!("terrain.randomize_tip")).clicked() {
            let nanos = std::time::SystemTime::now()
                .duration_since(std::time::UNIX_EPOCH)
                .map_or(1, |d| d.subsec_nanos() ^ (d.as_secs() as u32).wrapping_mul(2_654_435_761));
            g.params.seed = u64::from(nanos);
        }
    });
    ui.separator();
    ui.label(egui::RichText::new(t!("terrain.noise_section")).strong());
    choice(ui, &mut g.params.basis, &[(NoiseBasis::Perlin, "Perlin"), (NoiseBasis::Simplex, "Simplex")]);
    choice(
        ui,
        &mut g.params.shape,
        &[
            (NoiseShape::Plain, t!("terrain.shape_plain")),
            (NoiseShape::Ridged, t!("terrain.shape_ridged")),
            (NoiseShape::Billow, t!("terrain.shape_billow")),
        ],
    );
    egui::Grid::new("hp_terrain_noise").num_columns(2).spacing([8.0, 4.0]).show(ui, |ui| {
        let p = &mut g.params;
        drag(ui, t!("terrain.frequency"), t!("terrain.frequency_tip"), &mut p.frequency, 0.1..=64.0, 0.05);
        whole(ui, t!("terrain.octaves"), t!("terrain.octaves_tip"), &mut p.octaves, 1..=10);
        drag(ui, t!("terrain.persistence"), t!("terrain.persistence_tip"), &mut p.persistence, 0.05..=1.0, 0.01);
        drag(ui, t!("terrain.warp"), t!("terrain.warp_tip"), &mut p.warp, 0.0..=2.0, 0.01);
        drag(ui, t!("terrain.stretch"), t!("terrain.stretch_tip"), &mut p.stretch, 0.1..=16.0, 0.05);
        drag(ui, t!("terrain.contrast"), t!("terrain.contrast_tip"), &mut p.contrast, 0.1..=4.0, 0.02);
        drag(ui, t!("terrain.exponent"), t!("terrain.exponent_tip"), &mut p.exponent, 0.2..=5.0, 0.02);
        check(ui, t!("terrain.invert_noise"), t!("terrain.invert_noise_tip"), &mut p.invert);
        drag(ui, t!("terrain.sea_level"), t!("terrain.sea_level_tip"), &mut p.sea_level, 0.0..=0.95, 0.01);
        whole(ui, t!("terrain.terraces"), t!("terrain.terraces_tip"), &mut p.terraces, 0..=64);
        drag(
            ui,
            t!("terrain.terrace_sharp"),
            t!("terrain.terrace_sharp_tip"),
            &mut p.terrace_sharpness,
            0.0..=1.0,
            0.01,
        );
        check(ui, t!("terrain.island"), t!("terrain.island_tip"), &mut p.island);
    });
    if g.params.island {
        choice(
            ui,
            &mut g.params.falloff_shape,
            &[(FalloffShape::Round, t!("terrain.falloff_round")), (FalloffShape::Square, t!("terrain.falloff_square"))],
        );
        egui::Grid::new("hp_terrain_island").num_columns(2).spacing([8.0, 4.0]).show(ui, |ui| {
            drag(ui, t!("terrain.falloff"), t!("terrain.falloff_tip"), &mut g.params.falloff, 0.0..=1.0, 0.01);
            drag(
                ui,
                t!("terrain.falloff_power"),
                t!("terrain.falloff_power_tip"),
                &mut g.params.falloff_power,
                0.1..=8.0,
                0.02,
            );
        });
    }
    ui.separator();
    ui.label(egui::RichText::new(t!("terrain.target")).strong());
    let mut replace = g.replace;
    ui.horizontal(|ui| {
        if ui.add(egui::Button::selectable(!replace, t!("terrain.target_new"))).clicked() {
            replace = false;
        }
        if ui
            .add(egui::Button::selectable(replace, tf!("terrain.target_replace", selected)))
            .on_hover_text(t!("terrain.target_replace_tip"))
            .clicked()
        {
            replace = true;
        }
    });
    g.replace = replace;
    setup_ui(ui, "generate", &mut g.setup, !g.replace);
    ui.separator();
    ui.label(egui::RichText::new(t!("terrain.erosion")).strong());
    ui.checkbox(&mut g.erosion.thermal, t!("terrain.thermal")).on_hover_text(t!("terrain.thermal_tip"));
    if g.erosion.thermal {
        egui::Grid::new("hp_terrain_thermal").num_columns(2).spacing([8.0, 4.0]).show(ui, |ui| {
            let p = &mut g.erosion.thermal_params;
            whole(ui, t!("terrain.passes"), t!("terrain.passes_tip"), &mut p.iterations, 1..=1000);
            drag(ui, t!("terrain.talus"), t!("terrain.talus_tip"), &mut p.talus_degrees, 0.0..=89.0, 0.5);
            drag(ui, t!("terrain.strength"), t!("terrain.strength_tip"), &mut p.strength, 0.0..=1.0, 0.01);
        });
    }
    ui.checkbox(&mut g.erosion.hydraulic, t!("terrain.hydraulic")).on_hover_text(t!("terrain.hydraulic_tip"));
    if g.erosion.hydraulic {
        egui::Grid::new("hp_terrain_hydraulic").num_columns(2).spacing([8.0, 4.0]).show(ui, |ui| {
            let p = &mut g.erosion.hydraulic_params;
            whole(ui, t!("terrain.passes"), t!("terrain.passes_tip"), &mut p.iterations, 1..=hp_geom::MAX_ITERATIONS);
            drag(ui, t!("terrain.rain"), t!("terrain.rain_tip"), &mut p.rain, 0.0..=100.0, 0.05);
            drag(ui, t!("terrain.evaporation"), t!("terrain.evaporation_tip"), &mut p.evaporation, 0.0..=1.0, 0.01);
            drag(ui, t!("terrain.capacity"), t!("terrain.capacity_tip"), &mut p.capacity, 0.0..=10.0, 0.05);
            drag(ui, t!("terrain.deposition"), t!("terrain.deposition_tip"), &mut p.deposition, 0.0..=1.0, 0.01);
            drag(ui, t!("terrain.strength"), t!("terrain.dissolve_tip"), &mut p.strength, 0.0..=1.0, 0.01);
        });
    }
    ui.separator();
    ui.label(egui::RichText::new(t!("terrain.texturing")).strong());
    ui.checkbox(&mut g.auto_texture, t!("terrain.auto_texture")).on_hover_text(t!("terrain.auto_texture_tip"));
    if g.auto_texture {
        auto_ui(ui, "generate", &mut g.auto);
    }
    *g != before
}

fn generate_window(ctx: &egui::Context, tui: &mut TerrainUi, state: &mut AppState) {
    if !tui.generate_open {
        return;
    }
    let mut open = true;
    let mut apply = false;
    let mut close = false;
    let selected = targets(&state.doc).len();
    hp_app::chrome::Window::new(t!("terrain.generate_title"))
        .id(egui::Id::unique("hp_terrain_generate_window"))
        .default_pos(ctx.content_rect().center() - vec2(230.0, 300.0))
        .default_size([460.0, 640.0])
        .open(&mut open)
        .collapsible(false)
        .resizable(true)
        .show(ctx, |ui| {
            egui::ScrollArea::vertical().max_height(560.0).show(ui, |ui| {
                generate_ui(ui, &mut tui.generate, selected);
            });
            ui.separator();
            let mut live = tui.live_preview.unwrap_or(true);
            if ui
                .checkbox(&mut live, t!("terrain.live_preview"))
                .on_hover_text(t!("terrain.live_preview_tip"))
                .changed()
            {
                tui.live_preview = Some(live);
                tui.last_preview = None;
            }
            ui.horizontal(|ui| {
                let ready = if tui.generate.replace { selected > 0 } else { tui.generate.setup.validate().is_ok() };
                if ui
                    .add_enabled(ready, egui::Button::new(t!("terrain.generate_run")))
                    .on_hover_text(t!("terrain.generate_run_tip"))
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
        match generate(state, &tui.generate) {
            Ok(r) if tui.generate.replace => state.notify(Severity::Success, tf!("terrain.replaced", r.faces)),
            Ok(r) => state.notify(Severity::Success, tf!("terrain.generated", r.tiles, r.seed)),
            Err(e) => state.notify(Severity::Warning, tf!("terrain.failed", e)),
        }
    }
    if !open || close {
        tui.close_generate(state);
        return;
    }
    if tui.live_preview.unwrap_or(true) {
        let key = (tui.generate.clone(), state.doc.revision());
        if tui.last_preview.as_ref() != Some(&key) {
            tui.version += 1;
            state.runtime.mesh.terrain_preview = preview(&state.doc, &tui.generate, tui.version);
            tui.last_preview = Some(key);
        }
    } else if state.runtime.mesh.terrain_preview.is_some() {
        state.runtime.mesh.terrain_preview = None;
        tui.last_preview = None;
    }
}

fn export_window(ctx: &egui::Context, tui: &mut TerrainUi, state: &mut AppState) {
    if !tui.export_open {
        return;
    }
    let mut open = true;
    let mut apply = false;
    let mut close = false;
    let faces = targets(&state.doc);
    hp_app::chrome::Window::new(t!("terrain.export_title"))
        .id(egui::Id::unique("hp_terrain_export_window"))
        .default_pos(ctx.content_rect().center() - vec2(230.0, 150.0))
        .open(&mut open)
        .collapsible(false)
        .resizable(false)
        .show(ctx, |ui| {
            ui.label(egui::RichText::new(tf!("terrain.export_targets", faces.len())).weak());
            let o = &mut tui.export;
            egui::Grid::new("hp_terrain_export").num_columns(2).spacing([8.0, 4.0]).show(ui, |ui| {
                path_row(
                    ui,
                    t!("terrain.export_file"),
                    t!("terrain.export_file_tip"),
                    &mut o.path,
                    Pick::SaveHeightmap,
                );
                path_row(ui, t!("terrain.export_splat"), t!("terrain.export_splat_tip"), &mut o.splat, Pick::SaveSplat);
                for (label, tip, field) in [
                    (t!("terrain.h_min"), t!("terrain.export_min_tip"), &mut o.min_height),
                    (t!("terrain.h_max"), t!("terrain.export_max_tip"), &mut o.max_height),
                ] {
                    ui.label(label);
                    ui.horizontal(|ui| {
                        let mut on = field.is_some();
                        if ui.checkbox(&mut on, t!("terrain.export_fixed")).on_hover_text(tip).changed() {
                            *field = on.then_some(0.0);
                        }
                        if let Some(v) = field {
                            ui.add(egui::DragValue::new(v).speed(4.0).range(-16384.0..=16384.0));
                        }
                    });
                    ui.end_row();
                }
            });
            ui.horizontal(|ui| {
                let ready = !faces.is_empty() && !tui.export.path.trim().is_empty();
                if ui
                    .add_enabled(ready, egui::Button::new(t!("terrain.export_run")))
                    .on_hover_text(t!("terrain.export_run_tip"))
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
        match export(state, &faces, &tui.export) {
            Ok(r) => state.notify(
                Severity::Success,
                tf!(
                    "terrain.exported",
                    r.width,
                    r.height,
                    format!("{:.2}", r.min_height),
                    format!("{:.2}", r.max_height)
                ),
            ),
            Err(e) => state.notify(Severity::Warning, tf!("terrain.failed", e)),
        }
    }
    if !open || close {
        tui.export_open = false;
    }
}

pub fn show(ctx: &egui::Context, tui: &mut TerrainUi, state: &mut AppState) {
    import_window(ctx, tui, state);
    generate_window(ctx, tui, state);
    export_window(ctx, tui, state);
}
