pub mod actions;
pub mod checks;
pub mod chrome;
pub mod console;
pub mod data_colors;
pub mod dialog_keys;
pub mod dnd;
pub mod extend;
pub mod foliage;
pub mod fonts;
pub mod games;
pub mod grid;
pub mod grid_rotate;
pub mod hover;
pub mod i18n;
pub mod model_compiler;
pub mod native_window;
pub mod os;
pub mod pick;
pub mod runtime;
pub mod scatter;
pub mod settings;
pub mod sidecar;
pub mod source_filter;
pub mod state;
pub mod storage;
pub mod strings;
pub mod testing;
pub mod theme;
pub mod tool;
pub mod view;
pub mod vtf_external;
pub mod widgets;

pub use actions::Action;
pub use console::Severity;
pub use state::{AppState, AssetsApi, FgdReport, Gfx};
pub use tool::{Tool, ToolFlow, ToolId, ToolMenuEntry, ViewInput};
pub use view::{QuadrantView, ViewInfo, ViewKind};

pub fn register_translations() {
    i18n::register(strings::TR);
    i18n::register(games::TR);
    i18n::register(chrome::TR);
    i18n::register(runtime::fog::TR);
    i18n::register(runtime::update::TR);
    i18n::register(storage::TR);
    for table in actions::TABLES {
        i18n::register(table);
    }
}
