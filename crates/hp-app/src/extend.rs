use crate::actions::Category;
use crate::state::AppState;

pub mod page_order {
    pub const GENERAL: i32 = 100;
    pub const GAME: i32 = 200;
    pub const VIEWPORT: i32 = 300;
    pub const EDITING: i32 = 400;
    pub const COMPILE: i32 = 500;
    pub const KEYBOARD: i32 = 600;
}

pub trait SettingsPage {
    fn id(&self) -> &'static str;

    fn label(&self) -> String;

    fn icon(&self) -> &'static str {
        "⚙"
    }

    fn order(&self) -> i32;

    fn ui(&mut self, ui: &mut egui::Ui, state: &mut AppState);

    fn can_reset(&self) -> bool {
        true
    }

    fn reset(&mut self, _state: &mut AppState) {}
}

pub type CompileOptionsUi = fn(&mut egui::Ui, &mut crate::settings::CompileSettings);

static COMPILE_OPTIONS_UI: std::sync::OnceLock<CompileOptionsUi> = std::sync::OnceLock::new();

pub fn set_compile_options_ui(ui: CompileOptionsUi) {
    let _ = COMPILE_OPTIONS_UI.set(ui);
}

pub fn compile_options_ui() -> Option<CompileOptionsUi> {
    COMPILE_OPTIONS_UI.get().copied()
}

pub type CompilersUi = fn(&mut egui::Ui, &mut AppState);

static COMPILERS_UI: std::sync::OnceLock<CompilersUi> = std::sync::OnceLock::new();

pub fn set_compilers_ui(ui: CompilersUi) {
    let _ = COMPILERS_UI.set(ui);
}

pub fn compilers_ui() -> Option<CompilersUi> {
    COMPILERS_UI.get().copied()
}

pub type FloodVerdict = Option<(Option<hp_editor::Id>, Vec<glam::DVec3>)>;

pub type LeakConfirm = fn(
    &hp_vmf::Map,
    Option<&hp_fs::GameFs>,
    &std::sync::Arc<std::sync::atomic::AtomicBool>,
) -> Result<Option<FloodVerdict>, String>;

static LEAK_CONFIRM: std::sync::OnceLock<LeakConfirm> = std::sync::OnceLock::new();

pub fn set_leak_confirm(confirm: LeakConfirm) {
    let _ = LEAK_CONFIRM.set(confirm);
}

pub fn leak_confirm() -> Option<LeakConfirm> {
    LEAK_CONFIRM.get().copied()
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum StatusSide {
    Left,
    Right,
}

pub mod status_order {
    pub const HINT: i32 = 100;
    pub const SELECTION: i32 = 200;
    pub const CURSOR: i32 = 300;
    pub const FPS: i32 = 100;
    pub const TEXTURE_LOCK: i32 = 200;
    pub const SNAP: i32 = 300;
    pub const GRID: i32 = 400;
    pub const CONTENT: i32 = 500;
}

pub mod status_priority {
    pub const REAL_DIMS: i32 = 10;
    pub const HINT: i32 = 20;
    pub const SELECTION: i32 = 30;
    pub const CURSOR: i32 = 40;
}

#[derive(Clone, Copy, Debug, PartialEq)]
pub struct StatusFit {
    pub min: f32,
    pub preferred: f32,
    pub flex: bool,
    pub priority: i32,
}

impl StatusFit {
    pub fn fixed(width: f32) -> Self {
        let width = finite_width(width);
        Self { min: width, preferred: width, flex: false, priority: 0 }
    }

    pub fn shrinkable(min: f32, preferred: f32) -> Self {
        let min = finite_width(min);
        Self { min, preferred: finite_width(preferred).max(min), flex: false, priority: 0 }
    }

    pub fn flex(min: f32) -> Self {
        let min = finite_width(min);
        Self { min, preferred: min, flex: true, priority: 0 }
    }

    pub fn with_priority(mut self, priority: i32) -> Self {
        self.priority = priority;
        self
    }
}

fn finite_width(w: f32) -> f32 {
    if w.is_finite() { w.max(0.0) } else { 0.0 }
}

#[derive(Debug, Default)]
pub struct StatusContext {
    pub tool_hint: Option<String>,
    pub open_settings: Option<&'static str>,
}

pub trait StatusSegment {
    fn id(&self) -> &'static str;

    fn side(&self) -> StatusSide;

    fn order(&self) -> i32;

    fn width(&self) -> Option<f32> {
        None
    }

    fn fit(&mut self, _ui: &egui::Ui, _state: &AppState) -> Option<StatusFit> {
        Some(match self.width() {
            Some(w) => StatusFit::fixed(w),
            None => StatusFit::flex(80.0),
        })
    }

    fn ui(&mut self, ui: &mut egui::Ui, state: &mut AppState, cx: &mut StatusContext);
}

pub type MenuUi = Box<dyn FnMut(&mut egui::Ui, &mut AppState)>;

pub struct MenuExtension {
    pub menu: Category,
    pub section: u16,
    pub order: u16,
    pub submenu: Option<&'static str>,
    pub ui: MenuUi,
}

impl MenuExtension {
    pub fn new(
        menu: Category,
        section: u16,
        order: u16,
        ui: impl FnMut(&mut egui::Ui, &mut AppState) + 'static,
    ) -> Self {
        Self { menu, section, order, submenu: None, ui: Box::new(ui) }
    }

    pub fn in_submenu(mut self, title_key: &'static str) -> Self {
        self.submenu = Some(title_key);
        self
    }
}

impl std::fmt::Debug for MenuExtension {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("MenuExtension")
            .field("menu", &self.menu)
            .field("section", &self.section)
            .field("order", &self.order)
            .field("submenu", &self.submenu)
            .finish_non_exhaustive()
    }
}
