use egui::epaint::{PathShape, PathStroke};
use egui::text::{LayoutJob, TextWrapping};
use egui::{Align2, Color32, FontId, Id, Pos2, Rect, Sense, Stroke, StrokeKind, TextFormat, Ui, Vec2, pos2, vec2};
use hp_app::i18n::TrTable;
use hp_app::state::AssetStatus;
use hp_app::{AppState, t, tf, theme};
use hp_fs::{MountSnapshot, MountStage};

pub const SHOW_DELAY: f64 = 0.15;

const FADE_TIME: f32 = 0.22;

const CARD_WIDTH: f32 = 380.0;

const RING_SIZE: f32 = 68.0;

#[derive(Clone, Debug, Default, PartialEq)]
pub struct LoadingInfo {
    pub content: Option<Option<MountSnapshot>>,
    pub opening: Option<(String, f32)>,
}

impl LoadingInfo {
    pub fn from_state(state: &AppState) -> Option<Self> {
        let content = matches!(state.assets.status(), AssetStatus::Mounting).then(|| state.assets.mount_progress());
        let opening = state.runtime.startup.opening.as_ref().map(|o| (o.name.clone(), o.elapsed.as_secs_f32()));
        (content.is_some() || opening.is_some()).then_some(Self { content, opening })
    }

    pub fn title(&self) -> String {
        match (&self.content, &self.opening) {
            (None, Some((name, _))) => tf!("shell.loading.opening", name),
            _ => t!("status.loading_assets").to_owned(),
        }
    }

    pub fn step(&self) -> Option<String> {
        let p = self.content.as_ref()?.as_ref()?;
        Some(match p.stage {
            MountStage::Planning => t!("shell.loading.planning").to_owned(),
            MountStage::Opening if p.total == 0 => t!("shell.loading.planning").to_owned(),
            MountStage::Opening => tf!("shell.loading.sources", p.done.min(p.total), p.total),
            MountStage::Indexing => t!("shell.loading.indexing").to_owned(),
        })
    }

    pub fn fraction(&self) -> Option<f32> {
        self.content.as_ref()?.as_ref()?.fraction()
    }

    pub fn current(&self) -> &str {
        match self.content.as_ref().and_then(Option::as_ref) {
            Some(p) if p.stage == MountStage::Opening => &p.current,
            _ => "",
        }
    }

    pub fn opening_line(&self) -> Option<String> {
        let (name, secs) = self.opening.as_ref()?;
        Some(tf!("shell.status.opening", name, seconds(*secs)))
    }
}

fn seconds(secs: f32) -> String {
    let s = format!("{secs:.1}");
    if hp_app::i18n::lang() == hp_app::i18n::Lang::Fr { s.replace('.', ",") } else { s }
}

#[derive(Debug, Default)]
pub struct LoadingOverlay {
    busy_since: Option<f64>,
    last: Option<LoadingInfo>,
}

impl LoadingOverlay {
    pub fn update(&mut self, now: f64, info: Option<LoadingInfo>) -> bool {
        match info {
            Some(info) => {
                let since = *self.busy_since.get_or_insert(now);
                self.last = Some(info);
                now - since >= SHOW_DELAY
            }
            None => {
                self.busy_since = None;
                if let Some(Some(Some(p))) = self.last.as_mut().map(|l| l.content.as_mut()) {
                    p.stage = MountStage::Indexing;
                }
                false
            }
        }
    }

    pub fn delay_left(&self, now: f64) -> Option<f64> {
        self.busy_since.map(|since| (SHOW_DELAY - (now - since)).max(0.0))
    }

    pub fn ui(&mut self, ui: &mut Ui, state: &mut AppState) {
        let now = ui.input(|i| i.time);
        let show = self.update(now, LoadingInfo::from_state(state));
        let alpha = ui.ctx().animate_bool_with_time(Id::unique("hp_loading_overlay"), show, FADE_TIME);
        if !show && let Some(left) = self.delay_left(now) {
            ui.ctx().request_repaint_after(std::time::Duration::from_secs_f64(left.max(0.001)));
        }
        if alpha <= 0.0 {
            if !show {
                self.last = None;
            }
            return;
        }
        let Some(info) = self.last.clone() else { return };
        ui.ctx().request_repaint();
        if let Some(cancel) = paint(ui, ui.max_rect(), &info, now, alpha)
            && cancel
        {
            state.runtime.startup.cancel_open = true;
        }
    }
}

fn paint(ui: &mut Ui, area: Rect, info: &LoadingInfo, time: f64, alpha: f32) -> Option<bool> {
    let p = theme::palette();
    let painter = ui.painter_at(area);
    painter.rect_filled(area, 0.0, Color32::from_black_alpha((110.0 * alpha) as u8));

    let fade = |c: Color32| c.gamma_multiply(alpha);
    let width = CARD_WIDTH.min(area.width() - 32.0).max(220.0);
    let inner = width - 2.0 * 28.0;
    let fonts = |size: f32| FontId::proportional(size);

    let title = painter.layout(info.title(), fonts(16.0), fade(p.text), inner);
    let step = info.step().map(|s| one_line(&painter, &s, fonts(12.5), fade(p.text_weak), inner - 48.0));
    let percent = info.fraction().map(|f| format!("{} %", (f * 100.0).floor() as u32));
    let has_bar = info.content.is_some();
    let current = has_bar.then(|| one_line(&painter, info.current(), fonts(11.5), fade(p.text_faint), inner));
    let opening = if info.content.is_some() {
        info.opening_line().map(|s| one_line(&painter, &s, fonts(12.0), fade(p.text_weak), inner - 28.0))
    } else {
        info.opening.as_ref().map(|(_, secs)| {
            one_line(&painter, &format!("{} s", seconds(*secs)), fonts(12.5), fade(p.text_weak), inner)
        })
    };

    let pad = 28.0;
    let mut h = pad + RING_SIZE + 20.0 + title.size().y;
    if let Some(s) = &step {
        h += 8.0 + s.size().y;
    }
    if has_bar {
        h += 12.0 + 6.0;
        h += 8.0 + current.as_ref().map_or(0.0, |c| c.size().y.max(14.0));
    }
    let cancel_h = 24.0;
    match (&opening, info.content.is_some()) {
        (Some(o), true) => h += 18.0 + 1.0 + 12.0 + o.size().y.max(cancel_h),
        (Some(o), false) => h += 8.0 + o.size().y + 14.0 + cancel_h,
        (None, _) => {}
    }
    h += pad;

    let card = Rect::from_center_size(area.center(), vec2(width, h));
    let radius = 14u8;
    let mut shadow = ui.visuals().popup_shadow;
    shadow.color = shadow.color.gamma_multiply(alpha);
    painter.add(shadow.as_shape(card, radius));
    painter.rect(card, radius, fade(p.bg2), Stroke::new(1.0, fade(p.border)), StrokeKind::Inside);
    ui.interact(card, Id::unique("hp_loading_card"), Sense::click_and_drag());

    let mut y = card.top() + pad;
    let ring = Rect::from_center_size(pos2(card.center().x, y + RING_SIZE / 2.0), Vec2::splat(RING_SIZE));
    paint_ring(&painter, ring, time, alpha);
    y += RING_SIZE + 20.0;

    let left = card.left() + pad;
    let center_x = card.center().x;
    painter.galley(pos2(center_x - title.size().x / 2.0, y), title.clone(), fade(p.text));
    y += title.size().y;

    if let Some(s) = step {
        y += 8.0;
        if let Some(pc) = &percent {
            painter.galley(pos2(left, y), s.clone(), fade(p.text_weak));
            painter.text(pos2(card.right() - pad, y), Align2::RIGHT_TOP, pc, fonts(12.5), fade(p.text_weak));
        } else {
            painter.galley(pos2(center_x - s.size().x / 2.0, y), s.clone(), fade(p.text_weak));
        }
        y += s.size().y;
    }

    if has_bar {
        y += 12.0;
        let bar = Rect::from_min_size(pos2(left, y), vec2(inner, 6.0));
        let shown = info.fraction().map(|f| ui.ctx().animate_value_with_time(Id::unique("hp_loading_bar"), f, 0.25));
        paint_bar(&painter, bar, shown, time, alpha);
        y += 6.0 + 8.0;
        if let Some(c) = current {
            painter.galley(pos2(center_x - c.size().x / 2.0, y), c.clone(), fade(p.text_faint));
            y += c.size().y.max(14.0);
        }
    }

    let mut cancel = None;
    match (opening, info.content.is_some()) {
        (Some(o), true) => {
            y += 18.0;
            painter.hline(left..=card.right() - pad, y, Stroke::new(1.0, fade(p.border)));
            y += 1.0 + 12.0;
            let row_h = o.size().y.max(cancel_h);
            painter.galley(pos2(left, y + (row_h - o.size().y) / 2.0), o.clone(), fade(p.text_weak));
            let b = Rect::from_min_size(
                pos2(card.right() - pad - cancel_h, y + (row_h - cancel_h) / 2.0),
                Vec2::splat(cancel_h),
            );
            cancel = Some(cancel_button(ui, b, "✖", alpha));
        }
        (Some(o), false) => {
            y += 8.0;
            painter.galley(pos2(center_x - o.size().x / 2.0, y), o.clone(), fade(p.text_weak));
            y += o.size().y + 14.0;
            let label = t!("shell.loading.cancel");
            let w = painter.layout_no_wrap(label.to_owned(), fonts(12.5), p.text).size().x + 24.0;
            let b = Rect::from_min_size(pos2(center_x - w / 2.0, y), vec2(w, cancel_h));
            cancel = Some(cancel_button(ui, b, label, alpha));
        }
        (None, _) => {}
    }
    cancel
}

fn cancel_button(ui: &mut Ui, rect: Rect, label: &str, alpha: f32) -> bool {
    let p = theme::palette();
    let r = ui
        .interact(rect, Id::unique("hp_loading_cancel"), Sense::click())
        .on_hover_text(t!("shell.status.open_cancel"));
    let fill = if r.hovered() { p.bg4 } else { p.bg3 };
    let painter = ui.painter();
    painter.rect(
        rect,
        6u8,
        fill.gamma_multiply(alpha),
        Stroke::new(1.0, p.border.gamma_multiply(alpha)),
        StrokeKind::Inside,
    );
    painter.text(rect.center(), Align2::CENTER_CENTER, label, FontId::proportional(12.5), p.text.gamma_multiply(alpha));
    r.clicked() && alpha > 0.5
}

fn one_line(
    painter: &egui::Painter,
    text: &str,
    font: FontId,
    color: Color32,
    width: f32,
) -> std::sync::Arc<egui::Galley> {
    let mut job = LayoutJob::single_section(text.to_owned(), TextFormat::simple(font, color));
    job.wrap = TextWrapping::truncate_at_width(width.max(10.0));
    painter.layout_job(job)
}

fn ease(x: f64) -> f64 {
    let x = x.clamp(0.0, 1.0);
    if x < 0.5 { 4.0 * x * x * x } else { 1.0 - (-2.0 * x + 2.0).powi(3) / 2.0 }
}

const ARC_CYCLE: f64 = 1.4;
const ARC_GROW: f64 = 0.7;
const ARC_MIN: f64 = 0.06;
const ARC_SPIN: f64 = 0.3;

pub fn arc_at(time: f64) -> (f64, f64) {
    use std::f64::consts::TAU;
    let cycles = time.max(0.0) / ARC_CYCLE;
    let k = cycles.floor();
    let u = cycles - k;
    let head = k * ARC_GROW + ARC_GROW * ease(u * 2.0);
    let tail = k * ARC_GROW + ARC_GROW * ease(u * 2.0 - 1.0);
    let spin = time * ARC_SPIN;
    let start = (tail + spin) * TAU;
    let sweep = (head - tail + ARC_MIN) * TAU;
    (start.rem_euclid(TAU), sweep)
}

fn paint_ring(painter: &egui::Painter, rect: Rect, time: f64, alpha: f32) {
    use std::f32::consts::{FRAC_PI_2, TAU};
    let p = theme::palette();
    let center = rect.center();
    let width = 4.0;
    let r = rect.width() / 2.0 - width / 2.0;
    painter.circle_stroke(center, r, Stroke::new(width, p.border.gamma_multiply(alpha)));

    let (start, sweep) = arc_at(time);
    let (start, sweep) = (start as f32 - FRAC_PI_2, sweep as f32);
    let at = |a: f32| center + vec2(a.cos(), a.sin()) * r;
    let n = ((sweep * r / 2.0).ceil() as usize).clamp(8, 160);
    let points: Vec<Pos2> = (0..=n).map(|i| at(start + sweep * i as f32 / n as f32)).collect();
    let color = theme::accent().gamma_multiply(alpha);
    let stroke = PathStroke::new_uv(width, move |_, pos| {
        let d = pos - center;
        let ang = (d.y.atan2(d.x) - start).rem_euclid(TAU);
        let f = if ang > sweep + (TAU - sweep) / 2.0 { 0.0 } else { (ang / sweep).min(1.0) };
        color.gamma_multiply(f.powf(1.4))
    });
    painter.add(PathShape::line(points, stroke));
    painter.circle_filled(at(start + sweep), width / 2.0, color);

    let mark = Rect::from_center_size(center, Vec2::splat(rect.width() * 0.5));
    hp_app::widgets::paint_hammer(painter, mark, p.text.gamma_multiply(alpha));
}

fn paint_bar(painter: &egui::Painter, bar: Rect, fraction: Option<f32>, time: f64, alpha: f32) {
    let p = theme::palette();
    let radius = (bar.height() / 2.0) as u8;
    painter.rect_filled(bar, radius, p.bg4.gamma_multiply(alpha));
    let fill = theme::accent().gamma_multiply(alpha);
    let part = match fraction {
        Some(f) => Rect::from_min_size(bar.min, vec2(bar.width() * f.clamp(0.0, 1.0), bar.height())),
        None => {
            let seg = bar.width() * 0.28;
            let u = ease(0.5 - 0.5 * (time * std::f64::consts::PI / 1.1).cos()) as f32;
            Rect::from_min_size(pos2(bar.left() + (bar.width() - seg) * u, bar.top()), vec2(seg, bar.height()))
        }
    };
    if part.width() >= 1.0 {
        painter.rect_filled(part, radius, fill);
    }
}

pub const TR: TrTable = &[
    ("shell.loading.planning", "Looking for the game content...", "Recherche du contenu du jeu..."),
    ("shell.loading.sources", "Archives and addons: {0} / {1}", "Archives et addons : {0} / {1}"),
    ("shell.loading.indexing", "Listing materials, models and sounds...", "Liste des matériaux, modèles et sons..."),
    ("shell.loading.opening", "Opening {0}...", "Ouverture de {0}..."),
    ("shell.loading.cancel", "Cancel", "Annuler"),
];

#[cfg(test)]
mod tests {
    use std::sync::Arc;

    use egui::{Event, PointerButton, RawInput};
    use hp_app::i18n::Lang;
    use hp_app::state::{AssetsApi, MaterialInfo, ModelState, ThumbState};
    use hp_app::testing::{MockAssets, lang_lock, test_state};

    use super::*;

    struct Assets {
        mock: MockAssets,
        status: AssetStatus,
        progress: Option<MountSnapshot>,
        late: usize,
    }

    fn assets(status: AssetStatus, progress: Option<MountSnapshot>, late: usize) -> Box<Assets> {
        Box::new(Assets { mock: MockAssets::default(), status, progress, late })
    }

    impl AssetsApi for Assets {
        fn status(&self) -> AssetStatus {
            self.status.clone()
        }
        fn mount_progress(&self) -> Option<MountSnapshot> {
            self.progress.clone()
        }
        fn late_mount(&self) -> usize {
            self.late
        }
        fn fs(&self) -> Option<&hp_fs::GameFs> {
            None
        }
        fn fgd(&self) -> Option<&hp_fgd::ResolvedFgd> {
            None
        }
        fn generation(&self) -> u64 {
            1
        }
        fn remount(&mut self, _: &hp_app::settings::Settings) {}
        fn reload_fgd(&mut self, _: &hp_app::settings::Settings) {}
        fn materials(&self) -> Arc<Vec<hp_fs::AssetEntry>> {
            self.mock.materials()
        }
        fn models(&self) -> Arc<Vec<hp_fs::AssetEntry>> {
            self.mock.models()
        }
        fn sounds(&self) -> Arc<Vec<hp_fs::AssetEntry>> {
            self.mock.sounds()
        }
        fn source_label(&self, s: u32) -> String {
            self.mock.source_label(s)
        }
        fn material_info(&mut self, name: &str) -> Option<Arc<MaterialInfo>> {
            self.mock.material_info(name)
        }
        fn thumbnail(&mut self, _: &egui::Context, _: &str) -> ThumbState {
            ThumbState::Missing
        }
        fn model_info(&mut self, path: &str) -> ModelState {
            self.mock.model_info(path)
        }
        fn model_preview(
            &mut self,
            _: &mut hp_app::Gfx,
            _: &str,
            _: &hp_app::state::PreviewCamera,
            _: [u32; 2],
        ) -> Option<egui::TextureId> {
            None
        }
        fn entity_bounds(&self, e: &hp_vmf::Entity) -> Option<hp_geom::Aabb> {
            self.mock.entity_bounds(e)
        }
        fn invalidate_material(&mut self, _: &str) {}
        fn import_images(
            &mut self,
            _: &[std::path::PathBuf],
            _: &hp_app::state::ImportOptions,
        ) -> Vec<Result<String, String>> {
            Vec::new()
        }
        fn begin_frame(&mut self, _: &egui::Context, _: Option<&mut hp_app::Gfx>) {}
        fn sync_scene(&mut self, _: &mut hp_editor::Document, _: &mut hp_app::Gfx, _: &hp_app::state::SceneOptions) {}
        fn render_material_id(&mut self, _: &str) -> hp_render::MaterialId {
            0
        }
        fn status_line(&self) -> Option<String> {
            None
        }
    }

    fn opening_snapshot() -> MountSnapshot {
        MountSnapshot { stage: MountStage::Opening, done: 12, total: 40, current: "hl2_textures.vpk".into() }
    }

    fn state_with(status: AssetStatus, progress: Option<MountSnapshot>) -> AppState {
        let mut state = test_state();
        crate::register_translations();
        state.assets = assets(status, progress, 0);
        state
    }

    fn frame(
        ctx: &egui::Context,
        overlay: &mut LoadingOverlay,
        state: &mut AppState,
        time: f64,
        events: Vec<Event>,
    ) -> (Vec<(String, Rect)>, std::time::Duration) {
        let input = RawInput {
            screen_rect: Some(Rect::from_min_size(Pos2::ZERO, vec2(1200.0, 800.0))),
            time: Some(time),
            events,
            ..Default::default()
        };
        let mut out = ctx.run_ui(input, |ui| {
            egui::CentralPanel::default().show(ui, |ui| overlay.ui(ui, state));
        });
        out.textures_delta.clear();
        let mut texts = Vec::new();
        for clipped in &out.shapes {
            if let egui::Shape::Text(t) = &clipped.shape {
                texts.push((t.galley.text().to_owned(), t.visual_bounding_rect()));
            }
        }
        let delay =
            out.viewport_output.get(&egui::ViewportId::ROOT).map_or(std::time::Duration::MAX, |v| v.repaint_delay);
        (texts, delay)
    }

    fn warm_up(ctx: &egui::Context, overlay: &mut LoadingOverlay, state: &mut AppState) {
        for i in 0..5 {
            frame(ctx, overlay, state, f64::from(i) * 0.02, Vec::new());
        }
    }

    fn has(texts: &[(String, Rect)], needle: &str) -> bool {
        texts.iter().any(|(t, _)| t.contains(needle))
    }

    #[test]
    fn shows_only_after_the_delay_and_hides_when_done() {
        let mut o = LoadingOverlay::default();
        let info = || Some(LoadingInfo { content: Some(None), opening: None });
        assert!(!o.update(10.0, info()), "not at once");
        assert!(!o.update(10.1, info()));
        assert!(o.update(10.0 + SHOW_DELAY, info()), "after the delay");
        assert!(!o.update(10.3, None), "hidden when done");
        assert_eq!(o.delay_left(10.3), None, "nothing waits once done");
        assert!(!o.update(20.0, info()));
        assert!(o.delay_left(20.05).is_some_and(|d| (d - 0.1).abs() < 1e-9));
        assert!(!o.update(20.05, None));
        assert!(!o.update(20.5, None));
    }

    #[test]
    fn the_overlay_draws_while_mounting_after_the_delay_only() {
        let _lang = lang_lock();
        let mut state = state_with(AssetStatus::Ready, None);
        let ctx = egui::Context::default();
        let mut o = LoadingOverlay::default();
        warm_up(&ctx, &mut o, &mut state);
        state.assets = assets(AssetStatus::Mounting, Some(opening_snapshot()), 0);
        let (texts, delay) = frame(&ctx, &mut o, &mut state, 1.0, Vec::new());
        assert!(texts.is_empty(), "nothing before the delay: {texts:?}");
        assert!(delay > std::time::Duration::ZERO && delay <= std::time::Duration::from_millis(150), "{delay:?}");
        for i in 0..20 {
            frame(&ctx, &mut o, &mut state, 1.2 + f64::from(i) * 0.02, Vec::new());
        }
        let (texts, delay) = frame(&ctx, &mut o, &mut state, 1.7, Vec::new());
        assert!(has(&texts, "Loading game content..."), "{texts:?}");
        assert!(has(&texts, "Archives and addons: 12 / 40"), "{texts:?}");
        assert!(has(&texts, "30 %"), "{texts:?}");
        assert!(has(&texts, "hl2_textures.vpk"), "{texts:?}");
        assert_eq!(delay, std::time::Duration::ZERO, "the ring turns while it shows");

        state.assets = assets(AssetStatus::Ready, None, 0);
        for i in 0..20 {
            frame(&ctx, &mut o, &mut state, 1.8 + f64::from(i) * 0.03, Vec::new());
        }
        let (texts, delay) = frame(&ctx, &mut o, &mut state, 3.0, Vec::new());
        assert!(texts.is_empty(), "{texts:?}");
        assert_eq!(delay, std::time::Duration::MAX, "idle: no repaint asked");
    }

    #[test]
    fn nothing_draws_when_the_content_is_ready() {
        let mut state = state_with(AssetStatus::Ready, None);
        let ctx = egui::Context::default();
        let mut o = LoadingOverlay::default();
        warm_up(&ctx, &mut o, &mut state);
        for i in 0..10 {
            let (texts, delay) = frame(&ctx, &mut o, &mut state, 0.5 + f64::from(i) * 0.1, Vec::new());
            assert!(texts.is_empty());
            assert_eq!(delay, std::time::Duration::MAX);
        }
        assert!(LoadingInfo::from_state(&state).is_none());
    }

    #[test]
    fn progress_texts_in_both_languages() {
        let _lang = lang_lock();
        let state = state_with(AssetStatus::Mounting, Some(opening_snapshot()));
        let info = LoadingInfo::from_state(&state).unwrap();
        assert_eq!(info.fraction(), Some(0.3));
        assert_eq!(info.current(), "hl2_textures.vpk");
        assert_eq!(info.title(), "Loading game content...");
        assert_eq!(info.step().as_deref(), Some("Archives and addons: 12 / 40"));
        hp_app::i18n::set_lang(Lang::Fr);
        assert_eq!(info.title(), "Chargement du contenu du jeu...");
        assert_eq!(info.step().as_deref(), Some("Archives et addons : 12 / 40"));
        hp_app::i18n::set_lang(Lang::En);

        let step = |stage, total| {
            let p = MountSnapshot { stage, done: 0, total, current: String::new() };
            LoadingInfo { content: Some(Some(p)), opening: None }
        };
        assert_eq!(step(MountStage::Planning, 0).step().as_deref(), Some("Looking for the game content..."));
        assert_eq!(step(MountStage::Planning, 0).fraction(), None);
        assert_eq!(step(MountStage::Indexing, 40).step().as_deref(), Some("Listing materials, models and sounds..."));
        assert_eq!(step(MountStage::Indexing, 40).current(), "");
        let unknown = LoadingInfo { content: Some(None), opening: None };
        assert_eq!((unknown.step(), unknown.fraction()), (None, None));
    }

    #[test]
    fn a_map_opening_shows_with_a_cancel_button() {
        let _lang = lang_lock();
        let mut state = state_with(AssetStatus::Ready, None);
        state.runtime.startup.opening =
            Some(hp_app::runtime::OpeningStatus { name: "big.vmf".into(), elapsed: std::time::Duration::from_secs(2) });
        let info = LoadingInfo::from_state(&state).unwrap();
        assert_eq!(info.title(), "Opening big.vmf...");
        assert_eq!(info.opening_line().as_deref(), Some("Opening big.vmf... 2.0 s"));
        let ctx = egui::Context::default();
        let mut o = LoadingOverlay::default();
        let mut texts = Vec::new();
        for i in 0..20 {
            texts = frame(&ctx, &mut o, &mut state, f64::from(i) * 0.03, Vec::new()).0;
        }
        assert!(has(&texts, "Opening big.vmf..."), "{texts:?}");
        let (_, cancel) = texts.iter().find(|(t, _)| t == "Cancel").expect("cancel button");
        let at = cancel.center();
        let click = |pressed| Event::PointerButton {
            pos: at,
            button: PointerButton::Primary,
            pressed,
            modifiers: Default::default(),
        };
        frame(&ctx, &mut o, &mut state, 0.7, vec![Event::PointerMoved(at)]);
        frame(&ctx, &mut o, &mut state, 0.72, vec![click(true)]);
        frame(&ctx, &mut o, &mut state, 0.74, vec![click(false)]);
        assert!(state.runtime.startup.cancel_open, "the click stops the opening");

        state.assets = assets(AssetStatus::Mounting, None, 0);
        let info = LoadingInfo::from_state(&state).unwrap();
        assert_eq!(info.title(), "Loading game content...");
        assert!(info.opening_line().is_some());
        let mut texts = Vec::new();
        for i in 0..10 {
            texts = frame(&ctx, &mut o, &mut state, 1.0 + f64::from(i) * 0.03, Vec::new()).0;
        }
        assert!(has(&texts, "Loading game content...") && has(&texts, "Opening big.vmf... 2.0 s"), "{texts:?}");
    }

    #[test]
    fn the_arc_turns_smoothly() {
        use std::f64::consts::TAU;
        let mut last: Option<(f64, f64)> = None;
        for i in 0..2000 {
            let t = f64::from(i) / 240.0;
            let (start, sweep) = arc_at(t);
            assert!((0.0..TAU).contains(&start), "{t}: {start}");
            assert!((ARC_MIN * TAU - 1e-9..=(ARC_MIN + ARC_GROW) * TAU + 1e-9).contains(&sweep), "{t}: {sweep}");
            if let Some((s0, w0)) = last {
                let ds = (start - s0).rem_euclid(TAU);
                assert!(ds < 0.15, "the arc start never jumps ({t}: {ds})");
                assert!((sweep - w0).abs() < 0.15, "the arc length never jumps ({t})");
            }
            last = Some((start, sweep));
        }
    }

    #[test]
    fn the_status_bar_counts_the_late_addons() {
        let _lang = lang_lock();
        let mut state = state_with(AssetStatus::Ready, None);
        assert!(crate::status::late_text(&state).is_none());
        let p = MountSnapshot { stage: MountStage::Opening, done: 120, total: 450, current: "2104.gma".into() };
        state.assets = assets(AssetStatus::Ready, Some(p), 450);
        let (text, tip) = crate::status::late_text(&state).unwrap();
        assert_eq!(text, "Workshop 120 / 450");
        assert!(tip.contains("120") && tip.contains("2104.gma"), "{tip}");
        state.assets = assets(AssetStatus::Mounting, Some(opening_snapshot()), 0);
        assert!(crate::status::late_text(&state).is_none());
    }
}
