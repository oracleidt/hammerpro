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
