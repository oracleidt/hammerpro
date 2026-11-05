use std::path::PathBuf;

use crossbeam_channel::Receiver;
use egui::{Color32, Pos2, Rect, RichText, Sense, Stroke, Ui, vec2};
use hp_app::{AppState, Severity, t, tf, theme};
use hp_audio::{Edit, Sound};

use crate::media;

const ROW_H: f32 = 20.0;
const MAX_ROWS: usize = 100_000;
const WAVE_H: f32 = 150.0;

type PeakCache = (usize, usize, Vec<(f32, f32)>);

#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
enum Scope {
    #[default]
    Game,
    Map,
}

struct Loaded {
    sound: Sound,
    label: String,
    bytes: usize,
}

#[derive(Clone, Copy, PartialEq)]
enum Handle {
    Start,
    End,
}

pub struct SoundEditorWindow {
    search: String,
    scope: Scope,
    rows: Vec<String>,
    rows_key: Option<(Scope, String, usize, u64, usize)>,
    loaded: Option<Loaded>,
    edit: Edit,
    processed: Option<(Edit, Sound)>,
    peaks: Option<PeakCache>,
    out_name: String,
    root: Option<PathBuf>,
    overwrite: bool,
    status: Option<(Severity, String)>,
    pick_file: Option<Receiver<Option<PathBuf>>>,
    pick_dir: Option<Receiver<Option<PathBuf>>>,
    volume: f32,
    normalize_db: f32,
    dragging: Option<Handle>,
}

impl Default for SoundEditorWindow {
    fn default() -> Self {
        Self {
            search: String::new(),
            scope: Scope::Game,
            rows: Vec::new(),
            rows_key: None,
            loaded: None,
            edit: Edit::default(),
            processed: None,
            peaks: None,
            out_name: String::new(),
            root: None,
            overwrite: false,
            status: None,
            pick_file: None,
            pick_dir: None,
            volume: 0.8,
            normalize_db: -1.0,
            dragging: None,
        }
    }
}

fn db(v: f32) -> f32 {
    if v <= 1e-6 { -120.0 } else { 20.0 * v.log10() }
}

impl SoundEditorWindow {
    pub fn load(&mut self, state: &AppState, src: &str) {
        let result = media::read_source(state, src, media::game_sound_path).and_then(|(bytes, label)| {
            let size = bytes.len();
            hp_audio::decode(&bytes, &label).map(|s| (s, label, size)).map_err(|e| e.to_string())
        });
        match result {
            Ok((sound, label, bytes)) => {
                hp_audio::stop();
                self.edit = Edit::from_sound(&sound);
                self.out_name = default_output(&label);
                self.loaded = Some(Loaded { sound, label, bytes });
                self.processed = None;
                self.peaks = None;
                self.status = None;
                self.overwrite = false;
            }
            Err(e) => self.status = Some((Severity::Error, tf!("snd.load_failed", src, e))),
        }
    }

    fn refresh_rows(&mut self, state: &AppState) {
        let sounds = state.assets.sounds();
        let map = if self.scope == Scope::Map { media::map_sounds(state) } else { Vec::new() };
        let key = (self.scope, self.search.clone(), sounds.len(), state.assets.generation(), map.len());
        if self.rows_key.as_ref() == Some(&key) {
            return;
        }
        let words: Vec<String> = self.search.split_whitespace().map(str::to_ascii_lowercase).collect();
        let hit = |n: &str| {
            let l = n.to_ascii_lowercase();
            words.iter().all(|w| l.contains(w))
        };
        self.rows = match self.scope {
            Scope::Game => sounds
                .iter()
                .filter(|e| media::is_sound_ext(&e.name) && hit(&e.name))
                .take(MAX_ROWS)
                .map(|e| media::game_sound_path(&e.name))
                .collect(),
            Scope::Map => map.into_iter().filter(|n| hit(n)).collect(),
        };
        self.rows_key = Some(key);
    }

    fn poll(&mut self, state: &mut AppState) {
        if let Some(Some(path)) = media::poll(&mut self.pick_file) {
            self.load(state, &path.to_string_lossy());
        }
        if let Some(Some(dir)) = media::poll(&mut self.pick_dir) {
            self.root = Some(dir);
        }
        if let Some(src) = state.runtime.lab.sound_color.open_sound.take() {
            self.load(state, &src);
        }
    }

    pub fn show(&mut self, ctx: &egui::Context, state: &mut AppState) {
        self.poll(state);
        if !state.runtime.lab.sound_color.sound_open {
            if hp_audio::is_playing() {
                hp_audio::stop();
            }
            return;
        }
        self.refresh_rows(state);
        let mut open = true;
        hp_app::chrome::Window::new(format!("🔊  {}", t!("snd.title")))
            .id(egui::Id::unique("lab_sound_editor"))
            .open(&mut open)
            .default_size([860.0, 600.0])
            .min_size([640.0, 420.0])
            .resizable(true)
            .show(ctx, |ui| {
                ui.horizontal_top(|ui| {
                    ui.allocate_ui_with_layout(
                        vec2(280.0, ui.available_height()),
                        egui::Layout::top_down(egui::Align::Min),
                        |ui| self.list_ui(ui, ctx, state),
                    );
                    ui.separator();
                    egui::ScrollArea::vertical()
                        .auto_shrink([false, false])
                        .show(ui, |ui| ui.vertical(|ui| self.editor_ui(ui, ctx, state)).inner);
                });
            });
        if !open {
            state.runtime.lab.sound_color.sound_open = false;
        }
        if hp_audio::is_playing() {
            ctx.request_repaint();
        }
    }

    fn list_ui(&mut self, ui: &mut Ui, ctx: &egui::Context, state: &mut AppState) {
        ui.set_min_width(260.0);
        ui.horizontal(|ui| {
            ui.selectable_value(&mut self.scope, Scope::Game, t!("snd.scope.game"));
            ui.selectable_value(&mut self.scope, Scope::Map, t!("snd.scope.map"));
        });
        ui.add(egui::TextEdit::singleline(&mut self.search).hint_text(t!("snd.search")).desired_width(f32::INFINITY));
        if ui.button(t!("snd.open_file")).clicked() && self.pick_file.is_none() {
            let filter = t!("snd.files").to_owned();
            self.pick_file = Some(media::pick_file(filter, &["wav", "mp3", "ogg"], media::default_root(state), ctx));
        }
        ui.label(RichText::new(tf!("snd.count", self.rows.len())).weak().small());
        let height = (ui.available_height() - 4.0).max(ROW_H * 6.0);
        let selected = self.loaded.as_ref().map(|l| l.label.clone());
        let mut clicked: Option<String> = None;
        egui::ScrollArea::vertical().max_height(height).auto_shrink([false, false]).show_rows(
            ui,
            ROW_H,
            self.rows.len(),
            |ui, range| {
                for i in range {
                    let name = &self.rows[i];
                    let is_sel = selected.as_deref().is_some_and(|s| s.eq_ignore_ascii_case(name));
                    let short = name.strip_prefix("sound/").unwrap_or(name);
                    if ui.selectable_label(is_sel, short).on_hover_text(name).clicked() {
                        clicked = Some(name.clone());
                    }
                }
            },
        );
        if let Some(name) = clicked {
            self.load(state, &name);
        }
    }

    fn processed(&mut self) -> Option<&Sound> {
        let loaded = self.loaded.as_ref()?;
        if self.processed.as_ref().is_none_or(|(e, _)| *e != self.edit) {
            let out = hp_audio::apply(&loaded.sound, &self.edit);
            self.processed = Some((self.edit.clone(), out));
        }
        self.processed.as_ref().map(|(_, s)| s)
    }

    fn editor_ui(&mut self, ui: &mut Ui, ctx: &egui::Context, state: &mut AppState) {
        let Some(loaded) = &self.loaded else {
            ui.add_space(20.0);
            ui.label(RichText::new(t!("snd.pick")).weak());
            self.status_ui(ui);
            return;
        };
        let (rate, channels, duration, peak, bytes, label) = (
            loaded.sound.rate,
            loaded.sound.channels,
            loaded.sound.duration(),
            loaded.sound.peak(),
            loaded.bytes,
            loaded.label.clone(),
        );
        let friendly = loaded.sound.is_source_friendly();
        ui.label(RichText::new(&label).strong());
        let layout = if channels == 1 { t!("snd.mono") } else { t!("snd.stereo") };
        ui.label(
            RichText::new(tf!(
                "snd.info",
                rate,
                layout,
                format!("{duration:.2}"),
                format!("{:.1}", db(peak)),
                bytes / 1024
            ))
            .weak(),
        );
        if !friendly {
            ui.label(RichText::new(t!("snd.rate_warn")).color(theme::warning()).small());
        }
        ui.add_space(4.0);
        self.waveform_ui(ui, duration);
        ui.add_space(4.0);
        self.transport_ui(ui);
        ui.separator();
        self.controls_ui(ui, duration);
        ui.separator();
        self.save_ui(ui, ctx, state);
        self.status_ui(ui);
    }

    fn status_ui(&self, ui: &mut Ui) {
        if let Some((sev, text)) = &self.status {
            ui.add_space(4.0);
            ui.label(RichText::new(text).color(sev.color()));
        }
    }

    fn waveform_ui(&mut self, ui: &mut Ui, duration: f64) {
        let width = ui.available_width().max(200.0);
        let (rect, resp) = ui.allocate_exact_size(vec2(width, WAVE_H), Sense::click_and_drag());
        let Some(loaded) = &self.loaded else { return };
        let bins = (rect.width() as usize).max(1);
        let key = (bins, loaded.sound.samples.len());
        if self.peaks.as_ref().is_none_or(|(b, n, _)| (*b, *n) != key) {
            self.peaks = Some((bins, key.1, hp_audio::peaks(&loaded.sound, bins)));
        }
        let painter = ui.painter_at(rect);
        painter.rect_filled(rect, 4.0, ui.visuals().extreme_bg_color);
        let mid = rect.center().y;
        painter.line_segment(
            [Pos2::new(rect.left(), mid), Pos2::new(rect.right(), mid)],
            Stroke::new(1.0, theme::border()),
        );
        let x_of = |t: f64| rect.left() + (t / duration.max(1e-9)) as f32 * rect.width();
        let t_of = |x: f32| ((x - rect.left()) / rect.width()).clamp(0.0, 1.0) as f64 * duration;
        let start = self.edit.start.clamp(0.0, duration);
        let end = self.edit.end.unwrap_or(duration).clamp(start, duration);
        let (xs, xe) = (x_of(start), x_of(end));
        if let Some((_, _, peaks)) = &self.peaks {
            let half = rect.height() * 0.46;
            for (i, (lo, hi)) in peaks.iter().enumerate() {
                let x = rect.left() + i as f32 + 0.5;
                let inside = x >= xs && x <= xe;
                let color = if inside { theme::accent() } else { theme::text_weak().gamma_multiply(0.45) };
                painter.line_segment(
                    [Pos2::new(x, mid - hi.min(1.0) * half), Pos2::new(x, mid - lo.max(-1.0) * half)],
                    Stroke::new(1.0, color),
                );
            }
        }
        let shade = Color32::from_black_alpha(90);
        painter.rect_filled(Rect::from_min_max(rect.min, Pos2::new(xs, rect.bottom())), 0.0, shade);
        painter.rect_filled(Rect::from_min_max(Pos2::new(xe, rect.top()), rect.max), 0.0, shade);
        for (x, c) in [(xs, theme::success()), (xe, theme::error())] {
            painter.line_segment([Pos2::new(x, rect.top()), Pos2::new(x, rect.bottom())], Stroke::new(2.0, c));
        }
        if self.edit.looped {
            let lx = x_of((start + self.edit.loop_start).min(end));
            painter.line_segment(
                [Pos2::new(lx, rect.top()), Pos2::new(lx, rect.bottom())],
                Stroke::new(1.0, theme::info()),
            );
        }
        if let Some(pos) = hp_audio::position() {
            let px = x_of((start + pos).min(duration));
            painter.line_segment(
                [Pos2::new(px, rect.top()), Pos2::new(px, rect.bottom())],
                Stroke::new(1.5, theme::warning()),
            );
        }
        if resp.drag_started()
            && let Some(p) = resp.interact_pointer_pos()
        {
            self.dragging = Some(if (p.x - xs).abs() <= (p.x - xe).abs() { Handle::Start } else { Handle::End });
        }
        if resp.dragged()
            && let (Some(p), Some(h)) = (resp.interact_pointer_pos(), self.dragging)
        {
            let tm = t_of(p.x);
            match h {
                Handle::Start => self.edit.start = tm.min(end - 0.001).max(0.0),
                Handle::End => {
                    let e = tm.max(start + 0.001).min(duration);
                    self.edit.end = if (e - duration).abs() < 1e-4 { None } else { Some(e) };
                }
            }
        }
        if resp.drag_stopped() {
            self.dragging = None;
        }
        resp.on_hover_text(t!("snd.wave_tip"));
    }

    fn transport_ui(&mut self, ui: &mut Ui) {
        let playing = hp_audio::is_playing();
        ui.horizontal(|ui| {
            let label = if playing { format!("⏹ {}", t!("snd.stop")) } else { format!("▶ {}", t!("snd.play")) };
            if ui.button(label).clicked() {
                if playing {
                    hp_audio::stop();
                } else {
                    self.play(true);
                }
            }
            if ui.button(format!("▶ {}", t!("snd.play_original"))).clicked() {
                self.play(false);
            }
            ui.label(t!("snd.volume"));
            if ui.add(egui::Slider::new(&mut self.volume, 0.0..=1.0).show_value(false)).changed() && playing {
                self.play(true);
            }
            if ui.button(t!("snd.reset")).clicked()
                && let Some(l) = &self.loaded
            {
                self.edit = Edit::from_sound(&l.sound);
                hp_audio::stop();
            }
        });
    }

    fn play(&mut self, edited: bool) {
        let started = if edited {
            let looped = self.edit.looped;
            let volume = self.volume;
            self.processed().is_some_and(|s| hp_audio::play(s, 0.0, None, looped, volume))
        } else {
            let volume = self.volume;
            self.loaded.as_ref().is_some_and(|l| hp_audio::play(&l.sound, 0.0, None, false, volume))
        };
        if !started {
            self.status = Some((Severity::Warning, t!("snd.no_audio").to_owned()));
        }
    }

    fn controls_ui(&mut self, ui: &mut Ui, duration: f64) {
        let Some(loaded) = &self.loaded else { return };
        let sound_peak = loaded.sound.peak();
        egui::Grid::new("sound_editor_controls").num_columns(2).spacing([10.0, 6.0]).show(ui, |ui| {
            ui.label(t!("snd.gain"));
            ui.horizontal(|ui| {
                ui.add(egui::Slider::new(&mut self.edit.gain_db, -40.0..=24.0).suffix(" dB"));
                ui.label(
                    RichText::new(format!("{:.1} dB", db(sound_peak * hp_audio::db_to_gain(self.edit.gain_db))))
                        .weak()
                        .small(),
                )
                .on_hover_text(t!("snd.peak_after"));
            });
            ui.end_row();
            ui.label(t!("snd.normalize"));
            ui.horizontal(|ui| {
                ui.add(egui::DragValue::new(&mut self.normalize_db).range(-30.0..=0.0).speed(0.1).suffix(" dB"));
                if ui.button(t!("snd.normalize_apply")).on_hover_text(t!("snd.normalize_tip")).clicked()
                    && let Some(l) = &self.loaded
                {
                    self.edit.gain_db =
                        hp_audio::normalize_gain_db(&l.sound, &self.edit, self.normalize_db).clamp(-40.0, 24.0);
                }
            });
            ui.end_row();
            let end = self.edit.end.unwrap_or(duration);
            let mut start = self.edit.start;
            let mut stop = end;
            ui.label(t!("snd.start"));
            if ui.add(egui::DragValue::new(&mut start).range(0.0..=duration).speed(0.005).suffix(" s")).changed() {
                self.edit.start = start.min(stop - 0.001).max(0.0);
            }
            ui.end_row();
            ui.label(t!("snd.end"));
            if ui.add(egui::DragValue::new(&mut stop).range(0.0..=duration).speed(0.005).suffix(" s")).changed() {
                let e = stop.max(self.edit.start + 0.001).min(duration);
                self.edit.end = if (e - duration).abs() < 1e-4 { None } else { Some(e) };
            }
            ui.end_row();
            ui.label(t!("snd.length"));
            let mut len = self.edit.end.unwrap_or(duration) - self.edit.start;
            if ui.add(egui::DragValue::new(&mut len).range(0.001..=duration).speed(0.005).suffix(" s")).changed() {
                let e = (self.edit.start + len).min(duration);
                self.edit.end = if (e - duration).abs() < 1e-4 { None } else { Some(e) };
            }
            ui.end_row();
            ui.label(t!("snd.fade_in"));
            ui.add(egui::DragValue::new(&mut self.edit.fade_in).range(0.0..=duration).speed(0.01).suffix(" s"));
            ui.end_row();
            ui.label(t!("snd.fade_out"));
            ui.add(egui::DragValue::new(&mut self.edit.fade_out).range(0.0..=duration).speed(0.01).suffix(" s"));
            ui.end_row();
            ui.label(t!("snd.loop"));
            ui.horizontal(|ui| {
                ui.checkbox(&mut self.edit.looped, t!("snd.loop_on")).on_hover_text(t!("snd.loop_tip"));
                ui.add_enabled(
                    self.edit.looped,
                    egui::DragValue::new(&mut self.edit.loop_start)
                        .range(0.0..=duration)
                        .speed(0.005)
                        .prefix(format!("{} ", t!("snd.loop_start")))
                        .suffix(" s"),
                );
            });
            ui.end_row();
            ui.label(t!("snd.channels"));
            let stereo = self.loaded.as_ref().is_some_and(|l| l.sound.channels == 2);
            ui.add_enabled(stereo, egui::Checkbox::new(&mut self.edit.mono, t!("snd.to_mono")));
            ui.end_row();
            ui.label(t!("snd.rate"));
            let current = self.edit.rate;
            egui::ComboBox::from_id_salt("sound_editor_rate")
                .selected_text(current.map_or_else(|| t!("snd.rate_keep").to_owned(), |r| format!("{r} Hz")))
                .show_ui(ui, |ui| {
                    ui.selectable_value(&mut self.edit.rate, None, t!("snd.rate_keep"));
                    for r in hp_audio::SOURCE_RATES {
                        ui.selectable_value(&mut self.edit.rate, Some(r), format!("{r} Hz"));
                    }
                });
            ui.end_row();
        });
    }

    fn save_ui(&mut self, ui: &mut Ui, ctx: &egui::Context, state: &mut AppState) {
        ui.label(RichText::new(t!("snd.output")).strong());
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
            ui.label(t!("snd.file"));
            ui.add(egui::TextEdit::singleline(&mut self.out_name).desired_width(f32::INFINITY));
        });
        let rel = media::clean_rel(&self.out_name, media::SOUND_PREFIX, "wav");
        let exists = match (&root, &rel) {
            (Some(r), Ok(rel)) => r.join(rel).exists(),
            _ => false,
        };
        match &rel {
            Ok(rel) => {
                ui.label(RichText::new(tf!("snd.will_write", rel)).weak().small());
            }
            Err(e) => {
                ui.label(RichText::new(e).color(theme::error()).small());
            }
        }
        if exists {
            ui.checkbox(&mut self.overwrite, t!("snd.overwrite"));
        }
        ui.label(RichText::new(t!("snd.wav_only")).weak().small());
        let can = root.is_some() && rel.is_ok() && (!exists || self.overwrite);
        if ui.add_enabled(can, egui::Button::new(format!("💾 {}", t!("snd.save")))).clicked()
            && let (Some(root), Ok(rel)) = (root, rel)
        {
            self.save(state, &root, &rel);
        }
    }

    fn save(&mut self, state: &mut AppState, root: &std::path::Path, rel: &str) {
        let Some(sound) = self.processed().cloned() else { return };
        let bytes = hp_audio::encode_wav(&sound);
        match media::write_file(root, rel, &bytes, self.overwrite) {
            Ok(path) => {
                hp_ui_entities::model_import::remount_after_write(state);
                let text = tf!("snd.saved", path.display(), bytes.len() / 1024);
                state.notify(Severity::Success, text.clone());
                self.status = Some((Severity::Success, text));
                self.overwrite = false;
            }
            Err(e) => self.status = Some((Severity::Error, tf!("snd.failed", e))),
        }
    }
}

fn default_output(label: &str) -> String {
    if media::is_external(label) {
        let stem = std::path::Path::new(label)
            .file_stem()
            .map_or_else(|| "sound".to_owned(), |s| s.to_string_lossy().into_owned());
        format!("sound/{stem}.wav")
    } else {
        media::clean_rel(&media::game_sound_path(label), media::SOUND_PREFIX, "wav")
            .unwrap_or_else(|_| "sound/edited.wav".to_owned())
    }
}

pub const TR: hp_app::i18n::TrTable = &[
    ("snd.title", "Sound editor", "Éditeur de sons"),
    ("snd.scope.game", "Game sounds", "Sons du jeu"),
    ("snd.scope.map", "Used by the map", "Utilisés par la map"),
    ("snd.search", "Search sounds...", "Rechercher des sons..."),
    ("snd.open_file", "Open a sound file...", "Ouvrir un fichier son..."),
    ("snd.files", "Sound files (WAV, MP3, OGG)", "Fichiers son (WAV, MP3, OGG)"),
    ("snd.count", "{0} sounds", "{0} sons"),
    (
        "snd.pick",
        "Pick a sound in the list, or open a file, to edit it.",
        "Choisissez un son dans la liste, ou ouvrez un fichier, pour le modifier.",
    ),
    ("snd.mono", "mono", "mono"),
    ("snd.stereo", "stereo", "stéréo"),
    ("snd.info", "{0} Hz, {1}, {2} s, peak {3} dB, {4} KB", "{0} Hz, {1}, {2} s, crête {3} dB, {4} Ko"),
    (
        "snd.rate_warn",
        "Source plays 11025, 22050 and 44100 Hz sounds correctly: pick one of these rates below.",
        "Source lit correctement les sons en 11025, 22050 et 44100 Hz : choisissez l'un de ces taux plus bas.",
    ),
    (
        "snd.wave_tip",
        "Drag near the green line to move the start, near the red line to move the end.",
        "Glissez près de la ligne verte pour déplacer le début, près de la rouge pour déplacer la fin.",
    ),
    ("snd.play", "Play", "Lire"),
    ("snd.stop", "Stop", "Arrêter"),
    ("snd.play_original", "Original", "Original"),
    ("snd.volume", "Volume", "Volume"),
    ("snd.reset", "Reset", "Réinitialiser"),
    ("snd.no_audio", "Playback is not available on this system.", "La lecture n'est pas disponible sur ce système."),
    ("snd.gain", "Volume (gain)", "Volume (gain)"),
    ("snd.peak_after", "Peak level after the gain", "Niveau de crête après le gain"),
    ("snd.normalize", "Normalize to", "Normaliser à"),
    ("snd.normalize_apply", "Set the gain", "Régler le gain"),
    (
        "snd.normalize_tip",
        "Sets the gain so the loudest sample of the kept part reaches this level",
        "Règle le gain pour que l'échantillon le plus fort de la partie gardée atteigne ce niveau",
    ),
    ("snd.start", "Start", "Début"),
    ("snd.end", "End", "Fin"),
    ("snd.length", "Length", "Durée"),
    ("snd.fade_in", "Fade in", "Fondu d'entrée"),
    ("snd.fade_out", "Fade out", "Fondu de sortie"),
    ("snd.loop", "Loop", "Boucle"),
    ("snd.loop_on", "Loop the sound", "Boucler le son"),
    (
        "snd.loop_tip",
        "Writes the loop markers (cue and sampler chunks) the Source engine reads to repeat the sound",
        "Écrit les marqueurs de boucle (blocs cue et sampler) que le moteur Source lit pour répéter le son",
    ),
    ("snd.loop_start", "from", "dès"),
    ("snd.channels", "Channels", "Canaux"),
    ("snd.to_mono", "Mix down to mono", "Mixer en mono"),
    ("snd.rate", "Sample rate", "Fréquence d'échantillonnage"),
    ("snd.rate_keep", "Keep", "Conserver"),
    ("snd.output", "Save", "Enregistrer"),
    ("snd.root", "Folder:", "Dossier :"),
    (
        "snd.no_root",
        "(none: set the game folder in the settings)",
        "(aucun : configurez le dossier du jeu dans les paramètres)",
    ),
    ("snd.browse", "Browse...", "Parcourir..."),
    ("snd.file", "File:", "Fichier :"),
    ("snd.will_write", "Writes {0}", "Écrit {0}"),
    ("snd.overwrite", "Replace the existing file", "Remplacer le fichier existant"),
    (
        "snd.wav_only",
        "Always saved as a 16 bit PCM WAV file, the format Source reads.",
        "Toujours enregistré en WAV PCM 16 bits, le format que Source lit.",
    ),
    ("snd.save", "Save as WAV", "Enregistrer en WAV"),
    ("snd.saved", "Saved {0} ({1} KB)", "Enregistré : {0} ({1} Ko)"),
    ("snd.failed", "Could not save: {0}", "Enregistrement impossible : {0}"),
    ("snd.load_failed", "Cannot open {0}: {1}", "Impossible d'ouvrir {0} : {1}"),
];
