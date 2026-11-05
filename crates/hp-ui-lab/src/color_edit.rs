use std::path::PathBuf;

use crossbeam_channel::Receiver;
use egui::{Color32, ColorImage, Pos2, RichText, Sense, Stroke, TextureHandle, TextureOptions, Ui, vec2};
use hp_app::{AppState, Severity, t, tf, theme};
use hp_colorgrade::{Curve, FOLDERS, Grade};

use crate::media;

const SAMPLE_W: usize = 360;
const SAMPLE_H: usize = 240;
const CURVE_SIZE: f32 = 190.0;
const GRAB: f32 = 10.0;

pub struct ColorEditorWindow {
    grade: Grade,
    base: Option<Vec<u8>>,
    base_label: String,
    lut: Vec<u8>,
    dirty: bool,
    sample: Vec<u8>,
    sample_size: [usize; 2],
    sample_label: String,
    after: Vec<u8>,
    tex_before: Option<TextureHandle>,
    tex_after: Option<TextureHandle>,
    textures_dirty: bool,
    name: String,
    folder: usize,
    root: Option<PathBuf>,
    overwrite: bool,
    status: Option<(Severity, String)>,
    levels_tab: usize,
    curves_tab: usize,
    curve_drag: Option<usize>,
    live: bool,
    import_name: String,
    pick_raw: Option<Receiver<Option<PathBuf>>>,
    pick_image: Option<Receiver<Option<PathBuf>>>,
    pick_dir: Option<Receiver<Option<PathBuf>>>,
}

impl Default for ColorEditorWindow {
    fn default() -> Self {
        Self {
            grade: Grade::default(),
            base: None,
            base_label: String::new(),
            lut: hp_colorgrade::identity_raw(),
            dirty: true,
            sample: hp_colorgrade::sample_image(SAMPLE_W, SAMPLE_H),
            sample_size: [SAMPLE_W, SAMPLE_H],
            sample_label: String::new(),
            after: Vec::new(),
            tex_before: None,
            tex_after: None,
            textures_dirty: true,
            name: "cc_custom".to_owned(),
            folder: 0,
            root: None,
            overwrite: false,
            status: None,
            levels_tab: 0,
            curves_tab: 0,
            curve_drag: None,
            live: false,
            import_name: String::new(),
            pick_raw: None,
            pick_image: None,
            pick_dir: None,
        }
    }
}

const CHANNEL_NAMES: [&str; 4] = ["cc.master", "cc.red", "cc.green", "cc.blue"];

fn channel_color(i: usize) -> Color32 {
    hp_app::data_colors::channel(i)
}

impl ColorEditorWindow {
    pub fn load_lut(&mut self, state: &AppState, name: &str) {
        let name = name.trim();
        let bytes = if media::is_external(name) {
            std::fs::read(name).ok().filter(|b| hp_colorgrade::is_raw(b))
        } else {
            media::read_lut(state, name)
        };
        match bytes {
            Some(b) => {
                self.import(b, name);
                self.status = Some((Severity::Info, tf!("cc.imported", name)));
            }
            None => {
                self.status = Some((Severity::Error, tf!("cc.import_failed", name, hp_colorgrade::RAW_BYTES)));
            }
        }
    }

    fn import(&mut self, bytes: Vec<u8>, label: &str) {
        self.base = Some(bytes);
        self.base_label = label.to_owned();
        self.grade = Grade::default();
        let stem = label.rsplit(['/', '\\']).next().unwrap_or(label);
        let stem = stem.strip_suffix(".raw").or_else(|| stem.strip_suffix(".RAW")).unwrap_or(stem);
        if !stem.is_empty() {
            self.name = format!("{stem}_edit");
        }
        self.dirty = true;
        self.overwrite = false;
    }

    fn set_sample(&mut self, path: &std::path::Path) {
        match image::open(path) {
            Ok(img) => {
                let img = img.thumbnail(480, 320).to_rgba8();
                self.sample_size = [img.width() as usize, img.height() as usize];
                self.sample = img.into_raw();
                self.sample_label = path.file_name().map_or_else(String::new, |n| n.to_string_lossy().into_owned());
                self.dirty = true;
                self.textures_dirty = true;
            }
            Err(e) => self.status = Some((Severity::Error, tf!("cc.image_failed", path.display(), e))),
        }
    }

    fn poll(&mut self, state: &mut AppState) {
        if let Some(Some(path)) = media::poll(&mut self.pick_raw) {
            match std::fs::read(&path) {
                Ok(b) if hp_colorgrade::is_raw(&b) => {
                    self.import(b, &path.to_string_lossy());
                    self.status = Some((Severity::Info, tf!("cc.imported", path.display())));
                }
                _ => {
                    self.status =
                        Some((Severity::Error, tf!("cc.import_failed", path.display(), hp_colorgrade::RAW_BYTES)));
                }
            }
        }
        if let Some(Some(path)) = media::poll(&mut self.pick_image) {
            self.set_sample(&path);
        }
        if let Some(Some(dir)) = media::poll(&mut self.pick_dir) {
            self.root = Some(dir);
        }
        if let Some(name) = state.runtime.lab.sound_color.open_lut.take() {
            self.load_lut(state, &name);
        }
    }

    fn refresh(&mut self, state: &mut AppState) {
        if !self.dirty {
            return;
        }
        self.dirty = false;
        self.lut = hp_colorgrade::generate(&self.grade, self.base.as_deref());
        self.after.clone_from(&self.sample);
        hp_colorgrade::apply_to_rgba(&self.lut, &mut self.after);
        self.textures_dirty = true;
        if self.live {
            self.push_live(state);
        }
    }

    fn push_live(&self, state: &mut AppState) {
        let rgba = hp_colorgrade::to_rgba(&self.lut);
        state.runtime.lab.sound_color.set_preview(rgba, &self.name);
    }

    fn textures(&mut self, ctx: &egui::Context) {
        if !self.textures_dirty && self.tex_before.is_some() {
            return;
        }
        self.textures_dirty = false;
        let size = self.sample_size;
        let before = ColorImage::from_rgba_unmultiplied(size, &self.sample);
        let after = ColorImage::from_rgba_unmultiplied(
            size,
            if self.after.len() == self.sample.len() { &self.after } else { &self.sample },
        );
        match &mut self.tex_before {
            Some(t) => t.set(before, TextureOptions::LINEAR),
            None => self.tex_before = Some(ctx.load_texture("cc_before", before, TextureOptions::LINEAR)),
        }
        match &mut self.tex_after {
            Some(t) => t.set(after, TextureOptions::LINEAR),
            None => self.tex_after = Some(ctx.load_texture("cc_after", after, TextureOptions::LINEAR)),
        }
    }

    pub fn show(&mut self, ctx: &egui::Context, state: &mut AppState) {
        self.poll(state);
        if !state.runtime.lab.sound_color.color_open {
            if self.live {
                self.live = false;
                state.runtime.lab.sound_color.set_preview(None, "");
            }
            return;
        }
        self.refresh(state);
        self.textures(ctx);
        let mut open = true;
        hp_app::chrome::Window::new(format!("🎨  {}", t!("cc.title")))
            .id(egui::Id::unique("lab_color_correction_editor"))
            .open(&mut open)
            .default_size([1020.0, 700.0])
            .min_size([760.0, 480.0])
            .resizable(true)
            .show(ctx, |ui| {
                ui.horizontal_top(|ui| {
                    ui.allocate_ui_with_layout(
                        vec2(400.0, ui.available_height()),
                        egui::Layout::top_down(egui::Align::Min),
                        |ui| {
                            egui::ScrollArea::vertical().id_salt("cc_controls").auto_shrink([false, false]).show(
                                ui,
                                |ui| {
                                    ui.set_width(380.0);
                                    self.controls_ui(ui);
                                },
                            );
                        },
                    );
                    ui.separator();
                    egui::ScrollArea::vertical().id_salt("cc_right").auto_shrink([false, false]).show(ui, |ui| {
                        ui.vertical(|ui| self.preview_ui(ui, ctx, state));
                    });
                });
            });
        if !open {
            state.runtime.lab.sound_color.color_open = false;
        }
    }

    fn controls_ui(&mut self, ui: &mut Ui) {
        let mut changed = false;
        ui.label(RichText::new(t!("cc.presets")).strong());
        ui.horizontal_wrapped(|ui| {
            for p in hp_colorgrade::PRESETS {
                if ui.button(hp_app::i18n::tr(&format!("cc.preset.{}", p.id))).clicked() {
                    self.grade = (p.build)();
                    changed = true;
                }
            }
        });
        ui.separator();
        egui::CollapsingHeader::new(RichText::new(t!("cc.basic")).strong()).default_open(true).show(ui, |ui| {
            let g = &mut self.grade;
            changed |= slider(ui, t!("cc.exposure"), &mut g.exposure, -3.0..=3.0, 0.0, " EV");
            changed |= slider(ui, t!("cc.brightness"), &mut g.brightness, -0.5..=0.5, 0.0, "");
            changed |= slider(ui, t!("cc.contrast"), &mut g.contrast, -0.9..=0.9, 0.0, "");
            changed |= slider(ui, t!("cc.gamma"), &mut g.gamma, 0.3..=3.0, 1.0, "");
            changed |= slider(ui, t!("cc.saturation"), &mut g.saturation, 0.0..=3.0, 1.0, "");
            changed |= slider(ui, t!("cc.vibrance"), &mut g.vibrance, -1.0..=1.0, 0.0, "");
            changed |= slider(ui, t!("cc.hue"), &mut g.hue, -180.0..=180.0, 0.0, "°");
            changed |= ui.checkbox(&mut g.invert, t!("cc.invert")).changed();
        });
        egui::CollapsingHeader::new(RichText::new(t!("cc.balance")).strong()).default_open(true).show(ui, |ui| {
            let g = &mut self.grade;
            changed |= slider(ui, t!("cc.temperature"), &mut g.temperature, -1.0..=1.0, 0.0, "");
            changed |= slider(ui, t!("cc.magenta"), &mut g.magenta, -1.0..=1.0, 0.0, "");
            changed |= slider(ui, t!("cc.shadows"), &mut g.shadows, -1.0..=1.0, 0.0, "");
            changed |= slider(ui, t!("cc.highlights"), &mut g.highlights, -1.0..=1.0, 0.0, "");
        });
        egui::CollapsingHeader::new(RichText::new(t!("cc.tint")).strong()).default_open(false).show(ui, |ui| {
            let g = &mut self.grade;
            ui.horizontal(|ui| {
                ui.label(t!("cc.tint_color"));
                changed |= ui.color_edit_button_rgb(&mut g.tint.color).changed();
            });
            changed |= slider(ui, t!("cc.tint_amount"), &mut g.tint.amount, 0.0..=1.0, 0.0, "");
            ui.horizontal(|ui| {
                ui.label(t!("cc.colorize_color"));
                changed |= ui.color_edit_button_rgb(&mut g.colorize.color).changed();
            });
            changed |= slider(ui, t!("cc.colorize_amount"), &mut g.colorize.amount, 0.0..=1.0, 0.0, "");
        });
        egui::CollapsingHeader::new(RichText::new(t!("cc.levels")).strong()).default_open(false).show(ui, |ui| {
            ui.horizontal(|ui| {
                for (i, key) in CHANNEL_NAMES.iter().enumerate() {
                    ui.selectable_value(&mut self.levels_tab, i, RichText::new(t!(key)).color(channel_color(i)));
                }
            });
            let l = &mut self.grade.levels[self.levels_tab];
            changed |= slider(ui, t!("cc.in_black"), &mut l.in_black, 0.0..=0.95, 0.0, "");
            changed |= slider(ui, t!("cc.in_white"), &mut l.in_white, 0.05..=1.0, 1.0, "");
            changed |= slider(ui, t!("cc.level_gamma"), &mut l.gamma, 0.2..=5.0, 1.0, "");
            changed |= slider(ui, t!("cc.out_black"), &mut l.out_black, 0.0..=1.0, 0.0, "");
            changed |= slider(ui, t!("cc.out_white"), &mut l.out_white, 0.0..=1.0, 1.0, "");
            if ui.small_button(t!("cc.reset_channel")).clicked() {
                *l = hp_colorgrade::Channel::default();
                changed = true;
            }
        });
        egui::CollapsingHeader::new(RichText::new(t!("cc.curves")).strong()).default_open(false).show(ui, |ui| {
            ui.horizontal(|ui| {
                for (i, key) in CHANNEL_NAMES.iter().enumerate() {
                    ui.selectable_value(&mut self.curves_tab, i, RichText::new(t!(key)).color(channel_color(i)));
                }
            });
            let tab = self.curves_tab;
            changed |= curve_editor(ui, &mut self.grade.curves[tab], channel_color(tab), &mut self.curve_drag);
            ui.label(RichText::new(t!("cc.curve_tip")).weak().small());
            if ui.small_button(t!("cc.reset_channel")).clicked() {
                self.grade.curves[tab] = Curve::default();
                changed = true;
            }
        });
        ui.add_space(6.0);
        if ui.button(t!("cc.reset_all")).clicked() {
            self.grade = Grade::default();
            changed = true;
        }
        if changed {
            self.dirty = true;
            self.overwrite = false;
        }
    }

    fn preview_ui(&mut self, ui: &mut Ui, ctx: &egui::Context, state: &mut AppState) {
        ui.label(RichText::new(t!("cc.preview")).strong());
        let avail = ui.available_width();
        let each = ((avail - 12.0) / 2.0).clamp(160.0, 520.0);
        let aspect = self.sample_size[1] as f32 / self.sample_size[0].max(1) as f32;
        ui.horizontal(|ui| {
            for (label, tex) in [(t!("cc.before"), &self.tex_before), (t!("cc.after"), &self.tex_after)] {
                ui.vertical(|ui| {
                    ui.label(RichText::new(label).weak().small());
                    if let Some(tex) = tex {
                        ui.add(egui::Image::new((tex.id(), vec2(each, each * aspect))));
                    }
                });
            }
        });
        ui.horizontal(|ui| {
            if ui.button(t!("cc.sample_open")).clicked() && self.pick_image.is_none() {
                self.pick_image = Some(media::pick_file(
                    t!("cc.images").to_owned(),
                    &["png", "jpg", "jpeg", "bmp", "tga", "tif", "tiff"],
                    None,
                    ctx,
                ));
            }
            if ui.button(t!("cc.sample_default")).clicked() {
                self.sample = hp_colorgrade::sample_image(SAMPLE_W, SAMPLE_H);
                self.sample_size = [SAMPLE_W, SAMPLE_H];
                self.sample_label.clear();
                self.dirty = true;
            }
            if !self.sample_label.is_empty() {
                ui.label(RichText::new(&self.sample_label).weak().small());
            }
        });
        let mut live = self.live;
        if ui.checkbox(&mut live, t!("cc.live")).on_hover_text(t!("cc.live_tip")).changed() {
            self.live = live;
            if live {
                self.push_live(state);
            } else {
                state.runtime.lab.sound_color.set_preview(None, "");
            }
        }
        if let Some(s) = hp_colorgrade::stats(&self.lut) {
            let text = if s.identity {
                t!("cc.identity").to_owned()
            } else {
                tf!("cc.stats", format!("{:.1}", s.mean_shift), format!("{:.0}", s.max_shift))
            };
            ui.label(RichText::new(text).weak().small());
        }
        ui.separator();
        ui.label(RichText::new(t!("cc.import")).strong());
        ui.horizontal(|ui| {
            if ui.button(t!("cc.import_file")).clicked() && self.pick_raw.is_none() {
                self.pick_raw =
                    Some(media::pick_file(t!("cc.raw_files").to_owned(), &["raw"], media::default_root(state), ctx));
            }
            ui.add(
                egui::TextEdit::singleline(&mut self.import_name)
                    .hint_text("scripts/colorcorrection/name.raw")
                    .desired_width(210.0),
            );
            if ui.button(t!("cc.import_load")).clicked() && !self.import_name.trim().is_empty() {
                let name = self.import_name.clone();
                self.load_lut(state, &name);
            }
        });
        if self.base.is_some() {
            ui.horizontal(|ui| {
                ui.label(RichText::new(tf!("cc.base", self.base_label)).weak().small());
                if ui.small_button(t!("cc.base_remove")).clicked() {
                    self.base = None;
                    self.base_label.clear();
                    self.dirty = true;
                }
            });
        }
        ui.separator();
        self.save_ui(ui, ctx, state);
    }

    fn save_ui(&mut self, ui: &mut Ui, ctx: &egui::Context, state: &mut AppState) {
        ui.label(RichText::new(t!("cc.save")).strong());
        let root = self.root.clone().or_else(|| media::default_root(state));
        ui.horizontal(|ui| {
            ui.label(t!("snd.root"));
            let text = root.as_ref().map_or_else(|| t!("snd.no_root").to_owned(), |r| r.display().to_string());
            ui.label(RichText::new(text).weak());
            if ui.button(t!("snd.browse")).clicked() && self.pick_dir.is_none() {
                self.pick_dir = Some(media::pick_folder(root.clone(), ctx));
            }
        });
        ui.horizontal(|ui| {
            ui.label(t!("cc.folder"));
            egui::ComboBox::from_id_salt("cc_folder").selected_text(FOLDERS[self.folder]).show_ui(ui, |ui| {
                for (i, f) in FOLDERS.iter().enumerate() {
                    ui.selectable_value(&mut self.folder, i, *f);
                }
            });
        });
        ui.horizontal(|ui| {
            ui.label(t!("cc.name"));
            ui.add(egui::TextEdit::singleline(&mut self.name).desired_width(220.0));
        });
        let folder = FOLDERS[self.folder.min(FOLDERS.len() - 1)];
        let rel = media::clean_rel(&self.name, folder, "raw");
        let exists = match (&root, &rel) {
            (Some(r), Ok(rel)) => r.join(rel).exists(),
            _ => false,
        };
        match &rel {
            Ok(rel) => {
                ui.horizontal(|ui| {
                    ui.label(RichText::new(tf!("cc.reference", rel)).weak().small());
                    if ui.small_button(t!("cc.copy")).clicked() {
                        ui.ctx().copy_text(rel.clone());
                    }
                });
            }
            Err(e) => {
                ui.label(RichText::new(e).color(theme::error()).small());
            }
        }
        ui.label(RichText::new(t!("cc.path_note")).weak().small());
        if exists {
            ui.checkbox(&mut self.overwrite, t!("snd.overwrite"));
        }
        let can = root.is_some() && rel.is_ok() && (!exists || self.overwrite);
        if ui.add_enabled(can, egui::Button::new(format!("💾 {}", t!("cc.save_raw")))).clicked()
            && let (Some(root), Ok(rel)) = (root, rel)
        {
            match media::write_file(&root, &rel, &self.lut, self.overwrite) {
                Ok(path) => {
                    hp_ui_entities::model_import::remount_after_write(state);
                    state.runtime.lab.sound_color.lut_serial += 1;
                    let text = tf!("cc.saved", path.display(), rel);
                    state.notify(Severity::Success, text.clone());
                    self.status = Some((Severity::Success, text));
                    self.overwrite = false;
                }
                Err(e) => self.status = Some((Severity::Error, tf!("snd.failed", e))),
            }
        }
        if let Some((sev, text)) = &self.status {
            ui.add_space(4.0);
            ui.label(RichText::new(text).color(sev.color()));
        }
    }
}

fn slider(
    ui: &mut Ui,
    label: &str,
    v: &mut f32,
    range: std::ops::RangeInclusive<f32>,
    default: f32,
    suffix: &str,
) -> bool {
    let mut changed = false;
    ui.horizontal(|ui| {
        ui.add_sized([110.0, 18.0], egui::Label::new(label));
        let r = ui.add(egui::Slider::new(v, range).suffix(suffix).fixed_decimals(2));
        changed |= r.changed();
        if r.secondary_clicked() && (*v - default).abs() > f32::EPSILON {
            *v = default;
            changed = true;
        }
        r.on_hover_text(t!("cc.reset_tip"));
    });
    changed
}

fn curve_editor(ui: &mut Ui, curve: &mut Curve, color: Color32, drag: &mut Option<usize>) -> bool {
    let (rect, resp) = ui.allocate_exact_size(vec2(CURVE_SIZE, CURVE_SIZE), Sense::click_and_drag());
    let painter = ui.painter_at(rect);
    painter.rect_filled(rect, 3.0, ui.visuals().extreme_bg_color);
    for k in 1..4 {
        let f = k as f32 / 4.0;
        let c = Stroke::new(1.0, theme::border().gamma_multiply(0.5));
        painter.line_segment(
            [
                Pos2::new(rect.left() + f * rect.width(), rect.top()),
                Pos2::new(rect.left() + f * rect.width(), rect.bottom()),
            ],
            c,
        );
        painter.line_segment(
            [
                Pos2::new(rect.left(), rect.top() + f * rect.height()),
                Pos2::new(rect.right(), rect.top() + f * rect.height()),
            ],
            c,
        );
    }
    let to_screen = |p: [f32; 2]| Pos2::new(rect.left() + p[0] * rect.width(), rect.bottom() - p[1] * rect.height());
    painter.line_segment(
        [to_screen([0.0, 0.0]), to_screen([1.0, 1.0])],
        Stroke::new(1.0, theme::text_weak().gamma_multiply(0.5)),
    );
    let table = curve.table();
    let line: Vec<Pos2> =
        table.iter().enumerate().map(|(i, y)| to_screen([i as f32 / (table.len() - 1) as f32, *y])).collect();
    painter.add(egui::Shape::line(line, Stroke::new(2.0, color)));
    for p in &curve.points {
        painter.circle(to_screen(*p), 4.5, Color32::from_gray(30), Stroke::new(1.5, color));
    }
    let from_screen = |pos: Pos2| -> [f32; 2] {
        [
            ((pos.x - rect.left()) / rect.width()).clamp(0.0, 1.0),
            ((rect.bottom() - pos.y) / rect.height()).clamp(0.0, 1.0),
        ]
    };
    let nearest = |curve: &Curve, pos: Pos2| -> Option<usize> {
        curve
            .points
            .iter()
            .enumerate()
            .map(|(i, p)| (i, to_screen(*p).distance(pos)))
            .filter(|(_, d)| *d <= GRAB)
            .min_by(|a, b| a.1.total_cmp(&b.1))
            .map(|(i, _)| i)
    };
    let mut changed = false;
    if resp.drag_started()
        && let Some(pos) = resp.interact_pointer_pos()
    {
        if curve.points.len() < 2 {
            curve.points = vec![[0.0, 0.0], [1.0, 1.0]];
        }
        *drag = nearest(curve, pos).or_else(|| {
            let p = from_screen(pos);
            let at = curve.points.iter().position(|q| q[0] > p[0]).unwrap_or(curve.points.len());
            curve.points.insert(at, p);
            changed = true;
            Some(at)
        });
    }
    if resp.dragged()
        && let (Some(pos), Some(i)) = (resp.interact_pointer_pos(), *drag)
        && i < curve.points.len()
    {
        let mut p = from_screen(pos);
        let lo = if i == 0 { 0.0 } else { curve.points[i - 1][0] + 0.01 };
        let hi = if i + 1 >= curve.points.len() { 1.0 } else { curve.points[i + 1][0] - 0.01 };
        p[0] = p[0].clamp(lo.min(hi), hi.max(lo));
        if i == 0 {
            p[0] = p[0].min(hi);
        }
        curve.points[i] = p;
        changed = true;
    }
    if resp.drag_stopped() {
        *drag = None;
    }
    if resp.clicked()
        && let Some(pos) = resp.interact_pointer_pos()
        && nearest(curve, pos).is_none()
    {
        if curve.points.len() < 2 {
            curve.points = vec![[0.0, 0.0], [1.0, 1.0]];
        }
        let p = from_screen(pos);
        let at = curve.points.iter().position(|q| q[0] > p[0]).unwrap_or(curve.points.len());
        curve.points.insert(at, p);
        changed = true;
    }
    if resp.secondary_clicked()
        && let Some(pos) = resp.interact_pointer_pos()
        && let Some(i) = nearest(curve, pos)
        && curve.points.len() > 2
    {
        curve.points.remove(i);
        changed = true;
    }
    changed
}

pub const TR: hp_app::i18n::TrTable = &[
    ("cc.title", "Color correction editor", "Éditeur de correction des couleurs"),
    ("cc.presets", "Presets", "Préréglages"),
    ("cc.preset.neutral", "Neutral", "Neutre"),
    ("cc.preset.black_white", "Black and white", "Noir et blanc"),
    ("cc.preset.sepia", "Sepia", "Sépia"),
    ("cc.preset.warm", "Warm", "Chaud"),
    ("cc.preset.cool", "Cool", "Froid"),
    ("cc.preset.high_contrast", "High contrast", "Fort contraste"),
    ("cc.preset.faded", "Faded", "Délavé"),
    ("cc.preset.night", "Night", "Nuit"),
    ("cc.preset.vivid", "Vivid", "Éclatant"),
    ("cc.preset.cinematic", "Cinematic", "Cinéma"),
    ("cc.preset.horror", "Horror", "Horreur"),
    ("cc.preset.invert", "Invert", "Inversé"),
    ("cc.basic", "Basic", "Base"),
    ("cc.exposure", "Exposure", "Exposition"),
    ("cc.brightness", "Brightness", "Luminosité"),
    ("cc.contrast", "Contrast", "Contraste"),
    ("cc.gamma", "Gamma", "Gamma"),
    ("cc.saturation", "Saturation", "Saturation"),
    ("cc.vibrance", "Vibrance", "Vibrance"),
    ("cc.hue", "Hue", "Teinte"),
    ("cc.invert", "Invert the colors", "Inverser les couleurs"),
    ("cc.balance", "Color balance", "Équilibre des couleurs"),
    ("cc.temperature", "Temperature", "Température"),
    ("cc.magenta", "Green / magenta", "Vert / magenta"),
    ("cc.shadows", "Shadows", "Ombres"),
    ("cc.highlights", "Highlights", "Hautes lumières"),
    ("cc.tint", "Tint and colorize", "Teinte et colorisation"),
    ("cc.tint_color", "Tint color", "Couleur de teinte"),
    ("cc.tint_amount", "Tint amount", "Force de la teinte"),
    ("cc.colorize_color", "Colorize color", "Couleur de colorisation"),
    ("cc.colorize_amount", "Colorize amount", "Force de colorisation"),
    ("cc.levels", "Levels", "Niveaux"),
    ("cc.master", "RGB", "RVB"),
    ("cc.red", "Red", "Rouge"),
    ("cc.green", "Green", "Vert"),
    ("cc.blue", "Blue", "Bleu"),
    ("cc.in_black", "Input black", "Noir d'entrée"),
    ("cc.in_white", "Input white", "Blanc d'entrée"),
    ("cc.level_gamma", "Midtones", "Tons moyens"),
    ("cc.out_black", "Output black", "Noir de sortie"),
    ("cc.out_white", "Output white", "Blanc de sortie"),
    ("cc.reset_channel", "Reset this channel", "Réinitialiser ce canal"),
    ("cc.curves", "Curves", "Courbes"),
    (
        "cc.curve_tip",
        "Click to add a point, drag a point to move it, right click a point to remove it.",
        "Cliquez pour ajouter un point, glissez un point pour le déplacer, clic droit sur un point pour le supprimer.",
    ),
    ("cc.reset_all", "Reset everything", "Tout réinitialiser"),
    ("cc.reset_tip", "Right click to reset", "Clic droit pour réinitialiser"),
    ("cc.preview", "Preview", "Aperçu"),
    ("cc.before", "Before", "Avant"),
    ("cc.after", "After", "Après"),
    ("cc.sample_open", "Use an image...", "Utiliser une image..."),
    ("cc.sample_default", "Test pattern", "Mire de test"),
    ("cc.images", "Images", "Images"),
    ("cc.image_failed", "Cannot open {0}: {1}", "Impossible d'ouvrir {0} : {1}"),
    ("cc.live", "Preview in the 3D views", "Aperçu dans les vues 3D"),
    (
        "cc.live_tip",
        "Applies this correction to every 3D view while this window is open, whatever the map holds.",
        "Applique cette correction à toutes les vues 3D tant que cette fenêtre est ouverte, quoi que contienne la map.",
    ),
    ("cc.identity", "No change: the table is the identity.", "Aucun changement : la table est l'identité."),
    ("cc.stats", "Average shift {0} of 255, strongest shift {1}.", "Écart moyen {0} sur 255, écart maximal {1}."),
    ("cc.import", "Import", "Importer"),
    ("cc.import_file", "Import a .raw...", "Importer un .raw..."),
    ("cc.import_load", "Load", "Charger"),
    ("cc.raw_files", "Color lookup tables (.raw)", "Tables de correspondance (.raw)"),
    (
        "cc.imported",
        "Imported {0}: the corrections are applied on top of it.",
        "Importé : {0}. Les corrections s'appliquent par-dessus.",
    ),
    (
        "cc.import_failed",
        "{0} is not a color lookup table ({1} bytes expected).",
        "{0} n'est pas une table de correspondance ({1} octets attendus).",
    ),
    ("cc.base", "Based on {0}", "Basé sur {0}"),
    ("cc.base_remove", "Remove", "Retirer"),
    ("cc.save", "Save", "Enregistrer"),
    ("cc.folder", "Folder:", "Dossier :"),
    ("cc.name", "Name:", "Nom :"),
    ("cc.reference", "Lookup Filename: {0}", "Lookup Filename : {0}"),
    ("cc.copy", "Copy", "Copier"),
    (
        "cc.path_note",
        "Set this path as the Lookup Filename of a color_correction entity. Garry's Mod reads the table from this path in the game folder or the addon.",
        "Utilisez ce chemin comme Lookup Filename d'une entité color_correction. Garry's Mod lit la table à ce chemin dans le dossier du jeu ou l'addon.",
    ),
    ("cc.save_raw", "Save the .raw", "Enregistrer le .raw"),
    ("cc.saved", "Saved {0} (Lookup Filename: {1})", "Enregistré : {0} (Lookup Filename : {1})"),
];
