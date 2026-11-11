use std::sync::atomic::{AtomicBool, Ordering};

use egui::{
    Color32, CornerRadius, CursorIcon, Id, Pos2, Rect, ResizeDirection, Response, Sense, Stroke, StrokeKind, Ui,
    UiBuilder, ViewportCommand, pos2, vec2,
};

use crate::i18n::TrTable;
use crate::t;

pub const MARGIN: f32 = 8.0;
pub const RADIUS: f32 = 8.0;
pub const INSET: f32 = 4.0;
const OPAQUE_INSET: f32 = 4.0;
const INNER_GRIP: f32 = 4.0;
const MIN_GRIP_PX: f32 = 4.0;
pub const CORNER_GRAB: f32 = 16.0;
pub const MAIN_TITLE_HEIGHT: f32 = 34.0;
pub const DIALOG_TITLE_HEIGHT: f32 = 32.0;
pub const BUTTON_WIDTH: f32 = 46.0;
pub const CLOSE_HOVER: Color32 = Color32::from_rgb(0xE8, 0x11, 0x23);
pub const CLOSE_PRESSED: Color32 = Color32::from_rgb(0xF1, 0x70, 0x7A);
pub const DIRTY_MARKER: &str = "\u{23FA}";

pub const TR: TrTable = &[
    ("chrome.minimize", "Minimize", "Réduire"),
    ("chrome.maximize", "Maximize", "Agrandir"),
    ("chrome.restore", "Restore", "Restaurer"),
    ("chrome.close", "Close", "Fermer"),
    ("chrome.attach", "Put back in Hammer Pro", "Remettre dans Hammer Pro"),
    ("chrome.detach", "Open as a separate window", "Ouvrir dans une fenêtre séparée"),
];

static TRANSPARENCY: AtomicBool = AtomicBool::new(false);

pub fn set_transparency_available(available: bool) {
    TRANSPARENCY.store(available, Ordering::Relaxed);
}

pub fn transparency_available() -> bool {
    TRANSPARENCY.load(Ordering::Relaxed)
}

#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash)]
pub enum ChromeButton {
    Place,
    Minimize,
    Maximize,
    Close,
}

impl ChromeButton {
    pub fn name(self, maximized: bool) -> &'static str {
        match self {
            Self::Place => "place",
            Self::Minimize => "minimize",
            Self::Maximize if maximized => "restore",
            Self::Maximize => "maximize",
            Self::Close => "close",
        }
    }
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Hit {
    Outside,
    Resize(ResizeDirection),
    Button(ChromeButton),
    Caption,
    Client,
}

#[derive(Clone, Debug, PartialEq)]
pub struct FrameSpec {
    pub title_height: f32,
    pub buttons: Vec<ChromeButton>,
    pub resizable: bool,
    pub maximized: bool,
    pub transparent: bool,
}

#[derive(Clone, Debug, PartialEq)]
pub struct Geometry {
    pub window: Rect,
    pub visible: Rect,
    pub title: Rect,
    pub buttons: Vec<(ChromeButton, Rect)>,
    pub content: Rect,
    pub radius: f32,
    pub margin: f32,
    pub grip: f32,
    pub resizable: bool,
    pub maximized: bool,
}

impl Geometry {
    pub fn new(window: Rect, pixels_per_point: f32, spec: &FrameSpec) -> Self {
        let ppp = if pixels_per_point.is_finite() && pixels_per_point > 0.0 { pixels_per_point } else { 1.0 };
        let framed = !spec.maximized;
        let round = framed && spec.transparent;
        let margin = if round { MARGIN.min(window.width() / 4.0).min(window.height() / 4.0).max(0.0) } else { 0.0 };
        let visible = window.shrink(margin);
        let radius = if round { RADIUS } else { 0.0 };
        let inset = match (framed, round) {
            (false, _) => 0.0,
            (true, true) => INSET,
            (true, false) => OPAQUE_INSET,
        };
        let grip = if framed && spec.resizable {
            let points = if round { INNER_GRIP } else { OPAQUE_INSET };
            points.max(MIN_GRIP_PX / ppp)
        } else {
            0.0
        };
        let title_height = spec.title_height.max(0.0).min(visible.height());
        let title = Rect::from_min_size(visible.min, vec2(visible.width(), title_height));
        let mut buttons = Vec::with_capacity(spec.buttons.len());
        if title_height > 0.0 {
            let mut right = visible.right();
            for &b in spec.buttons.iter().rev() {
                let left = (right - BUTTON_WIDTH).max(visible.left());
                buttons.push((b, Rect::from_min_max(pos2(left, title.top()), pos2(right, title.bottom()))));
                right = left;
            }
            buttons.reverse();
        }
        let top = if title_height > 0.0 { title.bottom() } else { visible.top() + inset };
        let content = Rect::from_min_max(
            pos2(visible.left() + inset, top),
            pos2(visible.right() - inset, visible.bottom() - inset),
        );
        let content = if content.is_positive() { content } else { Rect::from_min_size(content.min, egui::Vec2::ZERO) };
        Self {
            window,
            visible,
            title,
            buttons,
            content,
            radius,
            margin,
            grip,
            resizable: spec.resizable && framed,
            maximized: spec.maximized,
        }
    }

    pub fn caption(&self) -> Rect {
        let right = self.buttons.first().map_or(self.title.right(), |(_, r)| r.left());
        Rect::from_min_max(self.title.min, pos2(right.max(self.title.left()), self.title.bottom()))
    }

    pub fn hit(&self, p: Pos2) -> Hit {
        if !self.window.contains(p) {
            return Hit::Outside;
        }
        if let Some(dir) = self.resize_at(p) {
            return Hit::Resize(dir);
        }
        if self.title.contains(p) {
            if let Some((b, _)) = self.buttons.iter().find(|(_, r)| r.contains(p)) {
                return Hit::Button(*b);
            }
            return Hit::Caption;
        }
        if self.visible.contains(p) { Hit::Client } else { Hit::Outside }
    }

    pub fn resize_at(&self, p: Pos2) -> Option<ResizeDirection> {
        if !self.resizable || !self.window.contains(p) {
            return None;
        }
        let v = self.visible;
        let left = p.x < v.left() + self.grip;
        let right = p.x >= v.right() - self.grip;
        let top = p.y < v.top() + self.grip;
        let bottom = p.y >= v.bottom() - self.grip;
        if !(left || right || top || bottom) {
            return None;
        }
        let corner = CORNER_GRAB.max(self.grip);
        let near_left = p.x < v.left() + corner;
        let near_right = p.x >= v.right() - corner;
        let near_top = p.y < v.top() + corner;
        let near_bottom = p.y >= v.bottom() - corner;
        use ResizeDirection as D;
        let dir = if (top && near_left) || (left && near_top) {
            D::NorthWest
        } else if (top && near_right) || (right && near_top) {
            D::NorthEast
        } else if (bottom && near_left) || (left && near_bottom) {
            D::SouthWest
        } else if (bottom && near_right) || (right && near_bottom) {
            D::SouthEast
        } else if top {
            D::North
        } else if bottom {
            D::South
        } else if left {
            D::West
        } else {
            D::East
        };
        Some(dir)
    }

    pub fn grip_bands(&self) -> Vec<Rect> {
        if !self.resizable {
            return Vec::new();
        }
        let (w, v, g) = (self.window, self.visible, self.grip);
        let top = v.top() + g;
        let bottom = v.bottom() - g;
        [
            Rect::from_min_max(w.min, pos2(w.right(), top)),
            Rect::from_min_max(pos2(w.left(), bottom), w.max),
            Rect::from_min_max(pos2(w.left(), top), pos2(v.left() + g, bottom)),
            Rect::from_min_max(pos2(v.right() - g, top), pos2(w.right(), bottom)),
        ]
        .into_iter()
        .filter(|r| r.is_positive())
        .collect()
    }
}

pub fn resize_cursor(dir: ResizeDirection) -> CursorIcon {
    use ResizeDirection as D;
    match dir {
        D::North | D::South => CursorIcon::ResizeVertical,
        D::East | D::West => CursorIcon::ResizeHorizontal,
        D::NorthWest | D::SouthEast => CursorIcon::ResizeNwSe,
        D::NorthEast | D::SouthWest => CursorIcon::ResizeNeSw,
    }
}

#[derive(Clone, Debug)]
pub struct ChromeSpec {
    pub title: String,
    pub dirty: bool,
    pub title_tooltip: Option<String>,
    pub title_height: f32,
    pub buttons: Vec<ChromeButton>,
    pub resizable: bool,
    pub external: bool,
    pub native: bool,
    pub logo: bool,
}

impl ChromeSpec {
    pub fn main(title: String, native: bool) -> Self {
        Self {
            title,
            dirty: false,
            title_tooltip: None,
            title_height: MAIN_TITLE_HEIGHT,
            buttons: vec![ChromeButton::Minimize, ChromeButton::Maximize, ChromeButton::Close],
            resizable: true,
            external: false,
            native,
            logo: false,
        }
    }

    pub fn dialog(title: impl Into<String>, native: bool) -> Self {
        Self {
            title: title.into(),
            dirty: false,
            title_tooltip: None,
            title_height: DIALOG_TITLE_HEIGHT,
            buttons: vec![ChromeButton::Place, ChromeButton::Maximize, ChromeButton::Close],
            resizable: true,
            external: true,
            native,
            logo: true,
        }
    }
}

#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub struct ChromeOutput {
    pub clicked: Option<ChromeButton>,
}

pub struct Chrome {
    spec: ChromeSpec,
    geom: Geometry,
    focused: bool,
    caption: Option<Response>,
    clicked: Option<ChromeButton>,
    title_used: f32,
}

fn geometry_id(viewport: egui::ViewportId) -> Id {
    Id::unique("hp_chrome_geometry").with(viewport)
}

pub fn last_geometry(ctx: &egui::Context, viewport: egui::ViewportId) -> Option<Geometry> {
    ctx.data(|d| d.get_temp::<Option<Geometry>>(geometry_id(viewport))).flatten()
}

pub fn sync_decorations(ctx: &egui::Context, native: bool, initial: bool) {
    let id = Id::unique("hp_chrome_decorations").with(ctx.viewport_id());
    let current = ctx.data(|d| d.get_temp::<bool>(id)).unwrap_or(initial);
    if current != native {
        ctx.send_viewport_cmd(ViewportCommand::Decorations(native));
        ctx.data_mut(|d| d.insert_temp(id, native));
    }
}

pub fn builder(builder: egui::ViewportBuilder, native: bool) -> egui::ViewportBuilder {
    builder.with_decorations(native).with_transparent(!native && transparency_available())
}

struct Colors {
    fill: Color32,
    title: Color32,
    border: Color32,
    separator: Color32,
    text: Color32,
    text_weak: Color32,
    hover: Color32,
    pressed: Color32,
    shadow: Color32,
}

fn colors(focused: bool) -> Colors {
    let p = crate::theme::palette();
    let dark = crate::theme::is_dark();
    let border = match (focused, crate::theme::accent_color()) {
        (true, Some(a)) => a,
        (true, None) => p.border_strong,
        (false, _) => p.border,
    };
    Colors {
        fill: p.bg1,
        title: if dark { p.bg0 } else { p.bg3 },
        border,
        separator: p.border,
        text: if focused { p.text } else { p.text_weak },
        text_weak: if focused { p.text_weak } else { p.text_faint },
        hover: p.bg4,
        pressed: p.selection,
        shadow: Color32::from_black_alpha(match (dark, focused) {
            (true, true) => 150,
            (true, false) => 90,
            (false, true) => 60,
            (false, false) => 35,
        }),
    }
}

impl Chrome {
    pub fn begin(ui: &mut Ui, spec: ChromeSpec) -> Self {
        let ctx = ui.ctx().clone();
        let (maximized, fullscreen, focused) = ctx.input(|i| {
            let v = i.viewport();
            (v.maximized.unwrap_or(false), v.fullscreen.unwrap_or(false), v.focused.unwrap_or(true))
        });
        let frame = FrameSpec {
            title_height: if spec.native { 0.0 } else { spec.title_height },
            buttons: if spec.native { Vec::new() } else { spec.buttons.clone() },
            resizable: spec.resizable,
            maximized: maximized || fullscreen,
            transparent: transparency_available(),
        };
        let window = ui.max_rect();
        let geom = if spec.native {
            Geometry {
                window,
                visible: window,
                title: Rect::from_min_size(window.min, egui::Vec2::ZERO),
                buttons: Vec::new(),
                content: window,
                radius: 0.0,
                margin: 0.0,
                grip: 0.0,
                resizable: false,
                maximized,
            }
        } else {
            Geometry::new(window, ctx.pixels_per_point(), &frame)
        };
        let stored = (!spec.native).then(|| geom.clone());
        let key = geometry_id(ctx.viewport_id());
        ctx.data_mut(|d| d.insert_temp(key, stored));
        let mut chrome = Self { title_used: geom.title.left(), spec, geom, focused, caption: None, clicked: None };
        if chrome.spec.native {
            return chrome;
        }
        chrome.paint_window(ui);
        if !fullscreen {
            chrome.caption_input(ui);
        }
        chrome.buttons(ui);
        chrome
    }

    pub fn geometry(&self) -> &Geometry {
        &self.geom
    }

    pub fn is_native(&self) -> bool {
        self.spec.native
    }

    fn paint_window(&self, ui: &Ui) {
        let g = &self.geom;
        let c = colors(self.focused);
        let painter = ui.painter();
        let r = CornerRadius::same(g.radius.round() as u8);
        if g.margin > 0.0 {
            let blur = (g.margin * 1.5).round().clamp(0.0, 255.0) as u8;
            let shadow = egui::Shadow { offset: [0, 2], blur, spread: 0, color: c.shadow };
            painter.add(shadow.as_shape(g.visible, r));
        }
        painter.rect_filled(g.visible, r, c.fill);
        if g.title.height() > 0.0 {
            let tr = CornerRadius { nw: r.nw, ne: r.ne, sw: 0, se: 0 };
            painter.rect_filled(g.title, tr, c.title);
            painter.hline(g.title.x_range(), g.title.bottom() - 0.5, Stroke::new(1.0, c.separator));
        }
    }

    fn caption_input(&mut self, ui: &mut Ui) {
        let caption = self.geom.caption();
        if !caption.is_positive() {
            return;
        }
        let ctx = ui.ctx().clone();
        let resp = ui.interact(caption, ui.scope_id().with("hp_chrome_caption"), Sense::click_and_drag());
        if resp.double_clicked() && self.spec.resizable {
            ctx.send_viewport_cmd(ViewportCommand::Maximized(!self.geom.maximized));
        } else if resp.drag_started_by(egui::PointerButton::Primary) {
            ctx.send_viewport_cmd(ViewportCommand::StartDrag);
        }
        self.caption = Some(resp);
    }

    fn buttons(&mut self, ui: &mut Ui) {
        let ctx = ui.ctx().clone();
        let c = colors(self.focused);
        let last = self.geom.buttons.len().saturating_sub(1);
        for (i, (button, rect)) in self.geom.buttons.clone().into_iter().enumerate() {
            let label = self.button_label(button);
            let resp = ui.interact(rect, ui.scope_id().with(("hp_chrome_button", button)), Sense::click());
            resp.widget_info(|| egui::WidgetInfo::labeled(egui::Role::Button, true, &label));
            let resp = resp.on_hover_text(label.as_str());
            let close = button == ChromeButton::Close;
            let fill = if resp.is_pointer_button_down_on() {
                Some(if close { CLOSE_PRESSED } else { c.pressed })
            } else if resp.hovered() {
                Some(if close { CLOSE_HOVER } else { c.hover })
            } else {
                None
            };
            if let Some(fill) = fill {
                let ne = if i == last { self.geom.radius.round() as u8 } else { 0 };
                ui.painter().rect_filled(rect, CornerRadius { nw: 0, ne, sw: 0, se: 0 }, fill);
            }
            let ink = if close && fill.is_some() { Color32::WHITE } else { c.text };
            paint_button_icon(ui.painter(), rect, button, self.geom.maximized, self.spec.external, ink);
            if resp.clicked() {
                match button {
                    ChromeButton::Minimize => ctx.send_viewport_cmd(ViewportCommand::Minimized(true)),
                    ChromeButton::Maximize => ctx.send_viewport_cmd(ViewportCommand::Maximized(!self.geom.maximized)),
                    ChromeButton::Close => ctx.send_viewport_cmd(ViewportCommand::Close),
                    ChromeButton::Place => {}
                }
                self.clicked = Some(button);
            }
        }
    }

    fn button_label(&self, button: ChromeButton) -> String {
        match button {
            ChromeButton::Place if self.spec.external => t!("chrome.attach"),
            ChromeButton::Place => t!("chrome.detach"),
            ChromeButton::Minimize => t!("chrome.minimize"),
            ChromeButton::Maximize if self.geom.maximized => t!("chrome.restore"),
            ChromeButton::Maximize => t!("chrome.maximize"),
            ChromeButton::Close => t!("chrome.close"),
        }
        .to_owned()
    }

    pub fn title_bar<R>(&mut self, ui: &mut Ui, add: impl FnOnce(&mut Ui) -> R) -> Option<R> {
        if self.spec.native || self.geom.title.height() <= 0.0 {
            return None;
        }
        let caption = self.geom.caption();
        let area = Rect::from_min_max(
            pos2(caption.left() + 8.0, caption.top()),
            pos2(caption.right() - 8.0, caption.bottom()),
        );
        if !area.is_positive() {
            return None;
        }
        let mut child = ui.new_child(
            UiBuilder::new()
                .id_salt("hp_chrome_title_bar")
                .max_rect(area)
                .layout(egui::Layout::left_to_right(egui::Align::Center)),
        );
        child.set_clip_rect(area.intersect(ui.clip_rect()));
        if self.spec.logo {
            crate::widgets::logo(&mut child, 18.0);
            child.add_space(4.0);
        }
        let r = add(&mut child);
        self.title_used = child.min_rect().right().max(self.title_used);
        Some(r)
    }

    pub fn set_title_left(&mut self, x: f32) {
        self.title_used = x;
    }

    pub fn body<R>(&mut self, ui: &mut Ui, add: impl FnOnce(&mut Ui) -> R) -> R {
        if self.spec.native {
            return add(ui);
        }
        let content = self.geom.content;
        ui.scope_builder(UiBuilder::new().id_salt("hp_chrome_body").max_rect(content), |ui| {
            ui.set_clip_rect(content.intersect(ui.clip_rect()));
            add(ui)
        })
        .inner
    }

    pub fn end(mut self, ui: &mut Ui) -> ChromeOutput {
        if self.spec.native {
            return ChromeOutput { clicked: self.clicked };
        }
        if self.spec.logo && self.title_used <= self.geom.title.left() {
            let _ = self.title_bar(ui, |_| ());
        }
        self.paint_title(ui);
        self.grip(ui);
        let g = &self.geom;
        if !g.maximized {
            let c = colors(self.focused);
            ui.painter().rect_stroke(
                g.visible,
                CornerRadius::same(g.radius.round() as u8),
                Stroke::new(1.0, c.border),
                StrokeKind::Inside,
            );
        }
        ChromeOutput { clicked: self.clicked }
    }

    fn paint_title(&mut self, ui: &Ui) {
        let caption = self.geom.caption();
        if self.spec.title.is_empty() || !caption.is_positive() {
            return;
        }
        let c = colors(self.focused);
        let free_left = self.title_used + 16.0;
        let free_right = caption.right() - 12.0;
        if free_right - free_left < 24.0 {
            return;
        }
        let font = egui::TextStyle::Body.resolve(ui.style());
        let mut job = egui::text::LayoutJob::default();
        if self.spec.dirty {
            let accent = crate::theme::accent_color().unwrap_or(c.text);
            job.append(DIRTY_MARKER, 0.0, egui::TextFormat::simple(font.clone(), accent));
            job.append(" ", 0.0, egui::TextFormat::simple(font.clone(), c.text));
        }
        job.append(&self.spec.title, 0.0, egui::TextFormat::simple(font, c.text));
        job.wrap.max_width = free_right - free_left;
        job.wrap.max_rows = 1;
        job.wrap.break_anywhere = true;
        let galley = ui.painter().layout_job(job);
        let size = galley.size();
        let center = self.geom.title.center().x;
        let x = if center - size.x / 2.0 >= free_left && center + size.x / 2.0 <= free_right {
            center - size.x / 2.0
        } else {
            free_left
        };
        let pos = pos2(x, self.geom.title.center().y - size.y / 2.0);
        let text_rect = Rect::from_min_size(pos, size);
        ui.painter().galley(pos, galley, c.text_weak);
        if let (Some(caption), Some(tip)) = (self.caption.take(), self.spec.title_tooltip.as_ref())
            && ui.ctx().pointer_hover_pos().is_some_and(|p| text_rect.contains(p))
        {
            let _ = caption.on_hover_text(tip.as_str());
        }
    }

    fn grip(&self, ui: &mut Ui) {
        let ctx = ui.ctx().clone();
        for (i, band) in self.geom.grip_bands().into_iter().enumerate() {
            let resp = ui.interact(band, ui.scope_id().with(("hp_chrome_grip", i)), Sense::drag());
            let Some(pos) = ctx.pointer_interact_pos() else { continue };
            if !(resp.hovered() || resp.is_pointer_button_down_on()) {
                continue;
            }
            if let Some(dir) = self.geom.resize_at(pos) {
                ctx.set_cursor_icon(resize_cursor(dir));
                if resp.is_pointer_button_down_on() && ctx.input(|i| i.pointer.primary_pressed()) {
                    ctx.send_viewport_cmd(ViewportCommand::BeginResize(dir));
                }
            }
        }
    }
}

pub fn dialog<R>(ui: &mut Ui, spec: ChromeSpec, body: impl FnOnce(&mut Ui) -> R) -> (R, ChromeOutput) {
    let mut chrome = Chrome::begin(ui, spec);
    let r = chrome.body(ui, body);
    let out = chrome.end(ui);
    crate::native_window::upload_textures(ui.ctx());
    (r, out)
}

pub const FLOATING_TITLE_HEIGHT: f32 = 28.0;
const FLOATING_CLOSE: egui::Vec2 = egui::vec2(28.0, 22.0);
pub const FLOATING_CLOSE_NAME: &str = "Close window";

pub struct Window<'open> {
    inner: egui::Window<'open>,
    title: egui::WidgetText,
    open: Option<&'open mut bool>,
}

impl<'open> Window<'open> {
    pub fn new(title: impl Into<egui::WidgetText>) -> Self {
        let title: egui::WidgetText = title.into();
        Self { inner: egui::Window::new(title.clone()).title_bar(false), title, open: None }
    }

    pub fn id(mut self, id: Id) -> Self {
        self.inner = self.inner.id(id);
        self
    }

    pub fn open(mut self, open: &'open mut bool) -> Self {
        self.open = Some(open);
        self
    }

    pub fn resizable(mut self, resizable: impl Into<egui::Vec2b>) -> Self {
        self.inner = self.inner.resizable(resizable);
        self
    }

    pub fn collapsible(mut self, collapsible: bool) -> Self {
        self.inner = self.inner.collapsible(collapsible);
        self
    }

    pub fn default_size(mut self, size: impl Into<egui::Vec2>) -> Self {
        self.inner = self.inner.default_size(size);
        self
    }

    pub fn default_pos(mut self, pos: impl Into<Pos2>) -> Self {
        self.inner = self.inner.default_pos(pos);
        self
    }

    pub fn default_width(mut self, width: f32) -> Self {
        self.inner = self.inner.default_width(width);
        self
    }

    pub fn default_height(mut self, height: f32) -> Self {
        self.inner = self.inner.default_height(height);
        self
    }

    pub fn min_size(mut self, size: impl Into<egui::Vec2>) -> Self {
        self.inner = self.inner.min_size(size);
        self
    }

    pub fn min_width(mut self, width: f32) -> Self {
        self.inner = self.inner.min_width(width);
        self
    }

    pub fn min_height(mut self, height: f32) -> Self {
        self.inner = self.inner.min_height(height);
        self
    }

    pub fn max_width(mut self, width: f32) -> Self {
        self.inner = self.inner.max_width(width);
        self
    }

    pub fn max_height(mut self, height: f32) -> Self {
        self.inner = self.inner.max_height(height);
        self
    }

    pub fn pivot(mut self, pivot: egui::Align2) -> Self {
        self.inner = self.inner.pivot(pivot);
        self
    }

    pub fn fixed_pos(mut self, pos: impl Into<Pos2>) -> Self {
        self.inner = self.inner.fixed_pos(pos);
        self
    }

    pub fn current_pos(mut self, pos: impl Into<Pos2>) -> Self {
        self.inner = self.inner.current_pos(pos);
        self
    }

    pub fn anchor(mut self, align: egui::Align2, offset: impl Into<egui::Vec2>) -> Self {
        self.inner = self.inner.anchor(align, offset);
        self
    }

    pub fn fixed_size(mut self, size: impl Into<egui::Vec2>) -> Self {
        self.inner = self.inner.fixed_size(size);
        self
    }

    pub fn auto_sized(mut self) -> Self {
        self.inner = self.inner.auto_sized();
        self
    }

    pub fn vscroll(mut self, vscroll: bool) -> Self {
        self.inner = self.inner.vscroll(vscroll);
        self
    }

    pub fn order(mut self, order: egui::Order) -> Self {
        self.inner = self.inner.order(order);
        self
    }

    pub fn constrain(mut self, constrain: bool) -> Self {
        self.inner = self.inner.constrain(constrain);
        self
    }

    pub fn movable(mut self, movable: bool) -> Self {
        self.inner = self.inner.movable(movable);
        self
    }

    pub fn show<R>(
        self,
        ctx: &egui::Context,
        add_contents: impl FnOnce(&mut Ui) -> R,
    ) -> Option<egui::InnerResponse<Option<R>>> {
        let Self { inner, title, open } = self;
        if open.as_deref() == Some(&false) {
            return None;
        }
        let closable = open.is_some();
        let mut close = false;
        let mut content = None;
        let response = inner.show(ctx, |ui| {
            content = Some(ui.unique_id());
            let galley =
                title.clone().into_galley(ui, Some(egui::TextWrapMode::Extend), f32::INFINITY, egui::TextStyle::Body);
            let close_w = if closable { FLOATING_CLOSE.x + 8.0 } else { 0.0 };
            let min_width = galley.size().x + close_w + 8.0;
            let (row, _) = ui.allocate_exact_size(vec2(min_width, FLOATING_TITLE_HEIGHT), Sense::hover());
            ui.add_space(4.0);
            let r = add_contents(ui);
            let m = ui.style().spacing.window_margin;
            let last = ui.ctx().memory(|mem| mem.area_rect(ui.layer_id().id));
            let mut span = ui.min_rect().x_range();
            if let Some(last) = last {
                span.max = span.max.max(last.right() - f32::from(m.right));
            }
            let row = Rect::from_x_y_ranges(span, row.y_range());
            close = floating_title_row(ui, row, galley, closable);
            r
        });
        if let (Some(r), Some(content)) = (&response, content) {
            crate::dialog_keys::window_shown(ctx, r.response.layer_id, content);
        }
        if close && let Some(open) = open {
            *open = false;
        }
        response
    }
}

fn floating_title_row(ui: &mut Ui, row: Rect, galley: std::sync::Arc<egui::Galley>, closable: bool) -> bool {
    let visuals = ui.visuals().clone();
    let m = ui.style().spacing.window_margin;
    let band = Rect::from_min_max(
        pos2(row.left() - f32::from(m.left) + 1.0, row.top() - f32::from(m.top) + 1.0),
        pos2(row.right() + f32::from(m.right) - 1.0, row.bottom()),
    );
    let radius = visuals.window_corner_radius.nw.saturating_sub(1);
    let painter = ui.ctx().layer_painter(ui.layer_id());
    let c = colors(true);
    painter.rect_filled(band, CornerRadius { nw: radius, ne: radius, sw: 0, se: 0 }, c.title);
    painter.hline(band.x_range(), band.bottom() - 0.5, Stroke::new(1.0, c.separator));
    let text_pos = pos2(row.left() + 2.0, row.center().y - galley.size().y / 2.0);
    painter.galley(text_pos, galley, visuals.strong_text_color());
    if !closable {
        return false;
    }
    let button = Rect::from_center_size(pos2(row.right() - FLOATING_CLOSE.x / 2.0, row.center().y), FLOATING_CLOSE);
    let resp = ui.interact(button, ui.scope_id().with("hp_chrome_floating_close"), Sense::click());
    resp.widget_info(|| egui::WidgetInfo::labeled(egui::Role::Button, true, FLOATING_CLOSE_NAME));
    let resp = resp.on_hover_text(t!("chrome.close"));
    let hot = resp.hovered() || resp.is_pointer_button_down_on();
    if hot {
        let fill = if resp.is_pointer_button_down_on() { CLOSE_PRESSED } else { CLOSE_HOVER };
        painter.rect_filled(button, CornerRadius::same(crate::theme::RADIUS), fill);
    }
    let ink = if hot { Color32::WHITE } else { visuals.text_color() };
    let x = Rect::from_center_size(button.center(), vec2(8.0, 8.0));
    let stroke = Stroke::new(1.0, ink);
    painter.line_segment([x.left_top(), x.right_bottom()], stroke);
    painter.line_segment([x.right_top(), x.left_bottom()], stroke);
    resp.clicked()
}

fn paint_button_icon(
    painter: &egui::Painter,
    rect: Rect,
    button: ChromeButton,
    maximized: bool,
    external: bool,
    ink: Color32,
) {
    let stroke = Stroke::new(1.0, ink);
    let c = rect.center();
    let h = 5.0;
    let painter = painter.clone();
    let snap = |p: Pos2| pos2(p.x.round() + 0.5, p.y.round() + 0.5);
    let c = snap(c);
    match button {
        ChromeButton::Minimize => {
            painter.line_segment([pos2(c.x - h, c.y), pos2(c.x + h, c.y)], stroke);
        }
        ChromeButton::Maximize if maximized => {
            let back = Rect::from_min_max(pos2(c.x - h + 2.0, c.y - h), pos2(c.x + h, c.y + h - 2.0));
            let front = Rect::from_min_max(pos2(c.x - h, c.y - h + 2.0), pos2(c.x + h - 2.0, c.y + h));
            painter.line_segment([back.left_top(), back.right_top()], stroke);
            painter.line_segment([back.right_top(), back.right_bottom()], stroke);
            painter.line_segment([back.left_top(), pos2(back.left(), front.top())], stroke);
            painter.line_segment([pos2(front.right(), back.bottom()), back.right_bottom()], stroke);
            painter.rect_stroke(front, 0, stroke, StrokeKind::Middle);
        }
        ChromeButton::Maximize => {
            let r = Rect::from_center_size(c, vec2(2.0 * h, 2.0 * h));
            painter.rect_stroke(r, 0, stroke, StrokeKind::Middle);
        }
        ChromeButton::Close => {
            painter.line_segment([pos2(c.x - h, c.y - h), pos2(c.x + h, c.y + h)], stroke);
            painter.line_segment([pos2(c.x + h, c.y - h), pos2(c.x - h, c.y + h)], stroke);
        }
        ChromeButton::Place if external => {
            let r = Rect::from_center_size(c, vec2(2.0 * h + 2.0, 2.0 * h));
            painter.rect_stroke(r, 1, stroke, StrokeKind::Middle);
            let docked =
                Rect::from_min_max(pos2(r.center().x + 1.0, r.top() + 3.0), pos2(r.right() - 2.0, r.bottom() - 2.0));
            painter.rect_filled(docked, 0, ink);
        }
        ChromeButton::Place => {
            let r = Rect::from_min_max(pos2(c.x - h, c.y - h + 2.0), pos2(c.x + h - 2.0, c.y + h));
            painter.line_segment([r.left_top(), pos2(c.x - 1.0, r.top())], stroke);
            painter.line_segment([r.left_top(), r.left_bottom()], stroke);
            painter.line_segment([r.left_bottom(), r.right_bottom()], stroke);
            painter.line_segment([r.right_bottom(), pos2(r.right(), c.y + 1.0)], stroke);
            let tip = pos2(c.x + h, c.y - h);
            painter.line_segment([pos2(c.x - 1.0, c.y + 1.0), tip], stroke);
            painter.line_segment([tip, pos2(tip.x - 4.0, tip.y)], stroke);
            painter.line_segment([tip, pos2(tip.x, tip.y + 4.0)], stroke);
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use ResizeDirection as D;

    fn spec(transparent: bool, maximized: bool) -> FrameSpec {
        FrameSpec {
            title_height: MAIN_TITLE_HEIGHT,
            buttons: vec![ChromeButton::Minimize, ChromeButton::Maximize, ChromeButton::Close],
            resizable: true,
            maximized,
            transparent,
        }
    }

    fn window(w: f32, h: f32) -> Rect {
        Rect::from_min_size(Pos2::ZERO, vec2(w, h))
    }

    #[test]
    fn round_frame_layout() {
        let g = Geometry::new(window(1280.0, 800.0), 1.0, &spec(true, false));
        assert_eq!(g.margin, MARGIN);
        assert_eq!(g.radius, RADIUS);
        assert_eq!(g.visible, Rect::from_min_max(pos2(8.0, 8.0), pos2(1272.0, 792.0)));
        assert_eq!(g.title, Rect::from_min_max(pos2(8.0, 8.0), pos2(1272.0, 8.0 + MAIN_TITLE_HEIGHT)));
        let names: Vec<_> = g.buttons.iter().map(|(b, _)| *b).collect();
        assert_eq!(names, [ChromeButton::Minimize, ChromeButton::Maximize, ChromeButton::Close]);
        assert_eq!(g.buttons[2].1.right(), 1272.0);
        assert_eq!(g.buttons[2].1.width(), BUTTON_WIDTH);
        assert_eq!(g.buttons[0].1.left(), 1272.0 - 3.0 * BUTTON_WIDTH);
        assert_eq!(g.content, Rect::from_min_max(pos2(12.0, 42.0), pos2(1268.0, 788.0)));
        let corner = g.content.left_bottom();
        let center = pos2(g.visible.left() + g.radius, g.visible.bottom() - g.radius);
        assert!(corner.distance(center) <= g.radius - 1.0, "{}", corner.distance(center));
        assert_eq!(g.caption().right(), g.buttons[0].1.left());
    }

    #[test]
    fn hit_testing_at_several_sizes_and_scales() {
        for (w, h) in [(1280.0, 800.0), (640.0, 480.0), (2560.0, 1400.0), (360.0, 300.0)] {
            for ppp in [1.0, 1.25, 1.5, 2.0, 3.0] {
                let g = Geometry::new(window(w, h), ppp, &spec(true, false));
                let v = g.visible;
                let ctx = format!("{w}x{h} at {ppp}");
                assert_eq!(g.hit(pos2(w / 2.0, 1.0)), Hit::Resize(D::North), "{ctx}");
                assert_eq!(g.hit(pos2(w / 2.0, v.top() + 1.0)), Hit::Resize(D::North), "{ctx}");
                assert_eq!(g.hit(pos2(w / 2.0, h - 1.0)), Hit::Resize(D::South), "{ctx}");
                assert_eq!(g.hit(pos2(1.0, h / 2.0)), Hit::Resize(D::West), "{ctx}");
                assert_eq!(g.hit(pos2(w - 1.0, h / 2.0)), Hit::Resize(D::East), "{ctx}");
                assert_eq!(g.hit(pos2(1.0, 1.0)), Hit::Resize(D::NorthWest), "{ctx}");
                assert_eq!(g.hit(pos2(w - 1.0, 1.0)), Hit::Resize(D::NorthEast), "{ctx}");
                assert_eq!(g.hit(pos2(1.0, h - 1.0)), Hit::Resize(D::SouthWest), "{ctx}");
                assert_eq!(g.hit(pos2(w - 1.0, h - 1.0)), Hit::Resize(D::SouthEast), "{ctx}");
                assert_eq!(g.hit(pos2(v.left() + 10.0, 2.0)), Hit::Resize(D::NorthWest), "{ctx}");
                assert_eq!(g.hit(pos2(2.0, v.bottom() - 10.0)), Hit::Resize(D::SouthWest), "{ctx}");
                let grip_px = g.grip * ppp;
                assert!(grip_px >= MIN_GRIP_PX - 1e-3, "{ctx}: {grip_px}");
                let mid_title = pos2(w / 2.0 - 100.0, g.title.center().y);
                assert_eq!(g.hit(mid_title), Hit::Caption, "{ctx}");
                let close = g.buttons[2].1;
                assert_eq!(g.hit(close.center()), Hit::Button(ChromeButton::Close), "{ctx}");
                assert_eq!(g.hit(g.buttons[0].1.center()), Hit::Button(ChromeButton::Minimize), "{ctx}");
                assert_eq!(g.hit(g.buttons[1].1.center()), Hit::Button(ChromeButton::Maximize), "{ctx}");
                assert_eq!(g.hit(pos2(close.center().x, v.top() + 0.5)), Hit::Resize(D::North), "{ctx}");
                assert_eq!(g.hit(g.content.center()), Hit::Client, "{ctx}");
                assert_eq!(g.hit(pos2(-1.0, 5.0)), Hit::Outside, "{ctx}");
                assert_eq!(g.hit(pos2(w + 1.0, 5.0)), Hit::Outside, "{ctx}");
                let bands = g.grip_bands();
                for p in [pos2(1.0, 1.0), pos2(w / 2.0, h - 1.0), pos2(1.0, h / 2.0), pos2(w - 1.0, h / 2.0)] {
                    assert!(bands.iter().any(|b| b.contains(p)), "{ctx}: {p:?}");
                }
                assert!(!bands.iter().any(|b| b.contains(g.content.center())), "{ctx}");
                assert!(!bands.iter().any(|b| b.contains(close.center())), "{ctx}");
            }
        }
    }

    #[test]
    fn maximized_windows_are_square_without_margin() {
        let g = Geometry::new(window(1920.0, 1040.0), 1.0, &spec(true, true));
        assert_eq!(g.margin, 0.0);
        assert_eq!(g.radius, 0.0);
        assert_eq!(g.visible, g.window);
        assert_eq!(g.content.left(), 0.0);
        assert_eq!(g.content.right(), 1920.0);
        assert_eq!(g.content.bottom(), 1040.0);
        assert!(!g.resizable && g.grip_bands().is_empty());
        assert_eq!(g.hit(pos2(0.0, 500.0)), Hit::Client);
        assert_eq!(g.hit(pos2(960.0, 0.0)), Hit::Caption);
        assert_eq!(g.hit(pos2(1919.5, 0.0)), Hit::Button(ChromeButton::Close));
    }

    #[test]
    fn square_frame_without_transparency() {
        for ppp in [1.0, 1.5, 2.0] {
            let g = Geometry::new(window(800.0, 600.0), ppp, &spec(false, false));
            assert_eq!(g.margin, 0.0);
            assert_eq!(g.radius, 0.0);
            assert_eq!(g.visible, g.window);
            assert!(g.grip * ppp >= MIN_GRIP_PX - 1e-3);
            assert_eq!(g.hit(pos2(0.5, 300.0)), Hit::Resize(D::West));
            assert_eq!(g.hit(pos2(400.0, 599.5)), Hit::Resize(D::South));
            assert!(g.content.left() >= g.grip - 1e-3);
            assert_eq!(g.hit(g.content.center()), Hit::Client);
        }
    }

    #[test]
    fn fixed_size_windows_have_no_grip() {
        let mut s = spec(true, false);
        s.resizable = false;
        let g = Geometry::new(window(400.0, 300.0), 1.0, &s);
        assert!(g.grip_bands().is_empty());
        assert_eq!(g.hit(pos2(1.0, 150.0)), Hit::Outside, "the margin is not a grip");
        assert_eq!(g.hit(pos2(g.visible.left() + 1.0, 150.0)), Hit::Client);
    }

    #[test]
    fn tiny_and_odd_windows() {
        for (w, h, ppp) in [(10.0, 10.0, 1.0), (0.0, 0.0, 2.0), (50.0, 20.0, 0.0), (30.0, 400.0, f32::NAN)] {
            let g = Geometry::new(window(w, h), ppp, &spec(true, false));
            assert!(g.visible.width() >= 0.0 && g.visible.height() >= 0.0);
            assert!(g.content.width() >= 0.0 && g.content.height() >= 0.0);
            assert!(g.title.height() <= g.visible.height());
            let _ = g.hit(pos2(w / 2.0, h / 2.0));
        }
        let mut s = spec(true, false);
        s.title_height = 0.0;
        s.buttons.clear();
        let g = Geometry::new(window(300.0, 200.0), 1.0, &s);
        assert_eq!(g.content.top(), g.visible.top() + INSET);
        assert!(g.buttons.is_empty());
    }

    #[test]
    fn button_names() {
        assert_eq!(ChromeButton::Maximize.name(false), "maximize");
        assert_eq!(ChromeButton::Maximize.name(true), "restore");
        assert_eq!(ChromeButton::Close.name(false), "close");
        assert_eq!(ChromeButton::Place.name(true), "place");
        assert_eq!(resize_cursor(D::NorthEast), CursorIcon::ResizeNeSw);
        assert_eq!(resize_cursor(D::West), CursorIcon::ResizeHorizontal);
    }

    #[test]
    fn translations_are_clean() {
        assert!(crate::i18n::lint_table(TR).is_empty(), "{:?}", crate::i18n::lint_table(TR));
    }

    #[test]
    fn frames_in_a_pass() {
        let ctx = egui::Context::default();
        let input = || egui::RawInput {
            screen_rect: Some(Rect::from_min_size(Pos2::ZERO, vec2(900.0, 600.0))),
            ..Default::default()
        };
        set_transparency_available(true);
        let mut content = Rect::NOTHING;
        let _ = ctx.run_ui(input(), |ui| {
            let mut chrome = Chrome::begin(ui, ChromeSpec::main("map.vmf - Hammer Pro".into(), false));
            let used = chrome.title_bar(ui, |ui| ui.label("File")).is_some();
            assert!(used);
            content = chrome.body(ui, |ui| ui.max_rect());
            let out = chrome.end(ui);
            assert_eq!(out.clicked, None);
        });
        let g = last_geometry(&ctx, egui::ViewportId::ROOT).expect("geometry stored");
        assert_eq!(content, g.content);
        assert!(content.top() > MAIN_TITLE_HEIGHT);
        let _ = ctx.run_ui(input(), |ui| {
            let (r, out) = dialog(ui, ChromeSpec::dialog("Compile", true), |ui| ui.max_rect());
            content = r;
            assert_eq!(out.clicked, None);
        });
        assert_eq!(content, Rect::from_min_size(Pos2::ZERO, vec2(900.0, 600.0)));
        assert!(last_geometry(&ctx, egui::ViewportId::ROOT).is_none());
    }

    #[test]
    fn floating_windows_close_from_their_title_row() {
        let ctx = egui::Context::default();
        let mut open = true;
        let mut close_at = None;
        let mut content = Rect::NOTHING;
        for frame in 0..6 {
            let mut input = egui::RawInput {
                screen_rect: Some(Rect::from_min_size(Pos2::ZERO, vec2(800.0, 600.0))),
                ..Default::default()
            };
            if let Some(p) = close_at {
                let p: Pos2 = p;
                input.events.push(egui::Event::PointerMoved(p));
                if frame == 3 || frame == 4 {
                    input.events.push(egui::Event::PointerButton {
                        pos: p,
                        button: egui::PointerButton::Primary,
                        pressed: frame == 3,
                        modifiers: egui::Modifiers::NONE,
                    });
                }
            }
            let _ = ctx.run_ui(input, |ui| {
                let r = Window::new("Find")
                    .id(Id::unique("hp_chrome_test_window"))
                    .fixed_pos([100.0, 100.0])
                    .open(&mut open)
                    .show(ui.ctx(), |ui| {
                        content = ui.label("content").rect;
                    });
                if let Some(r) = r {
                    let rect = r.response.rect;
                    close_at = Some(pos2(
                        rect.right() - 8.0 - FLOATING_CLOSE.x / 2.0,
                        rect.top() + 8.0 + FLOATING_TITLE_HEIGHT / 2.0,
                    ));
                }
            });
            if frame == 1 {
                assert!(content.top() >= 100.0 + FLOATING_TITLE_HEIGHT, "{content:?}");
            }
        }
        assert!(!open, "the close button of the title row closed the window");
        let mut closed = false;
        let _ = ctx.run_ui(egui::RawInput::default(), |ui| {
            closed = Window::new("Find").open(&mut open).show(ui.ctx(), |_| ()).is_none();
        });
        assert!(closed, "nothing shown while closed");
    }

    #[test]
    fn marker_glyph_exists() {
        let ctx = egui::Context::default();
        let mut out = ctx.run_ui(egui::RawInput::default(), |_| {});
        out.textures_delta.clear();
        let font = egui::FontId::proportional(14.0);
        assert!(ctx.fonts_mut(|f| f.has_glyphs(&font, DIRTY_MARKER)));
    }
}
