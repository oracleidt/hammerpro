pub mod browser;
pub mod faces;
pub mod folders;
pub mod grid;
pub mod icons;
pub mod import;
pub(crate) mod indexes;
mod mask;
pub mod matedit;
mod matedit_preview;
mod matedit_single;
pub mod picker;
pub mod replace;
pub mod search;
pub mod shader;
pub mod shared;
pub mod tool;
mod tr;
pub mod util;
pub mod uv_editor;
pub mod uv_layers;
pub mod uv_ops;
pub(crate) mod uv_select;
mod uv_texture;
pub(crate) mod vmt_params;
pub mod vtfedit;

use hp_app::AppState;

pub use browser::TextureBrowser;
pub use tool::TextureTool;
pub use tr::TR;

pub fn register_translations() {
    hp_app::i18n::register(TR);
    hp_app::i18n::register(shader::tr::TR);
    hp_app::i18n::register(vtfedit::TR);
}

#[derive(Default)]
pub struct TextureDialogs {
    replace: replace::ReplaceDialog,
    editor: matedit::MaterialEditor,
    import: import::ImportDialog,
    shader: shader::ShaderEditor,
    vtf: vtfedit::VtfEditor,
}

impl TextureDialogs {
    pub fn new() -> Self {
        Self::default()
    }

    pub fn show(&mut self, ctx: &egui::Context, state: &mut AppState) {
        self.replace.show(ctx, state);
        self.editor.show(ctx, state);
        self.import.show(ctx, state);
        self.shader.show(ctx, state);
        self.vtf.show(ctx, state);
    }

    pub fn replace_dialog(&mut self) -> &mut replace::ReplaceDialog {
        &mut self.replace
    }

    pub fn material_editor(&mut self) -> &mut matedit::MaterialEditor {
        &mut self.editor
    }

    pub fn import_dialog(&mut self) -> &mut import::ImportDialog {
        &mut self.import
    }

    pub fn shader_editor(&mut self) -> &mut shader::ShaderEditor {
        &mut self.shader
    }

    pub fn vtf_editor(&mut self) -> &mut vtfedit::VtfEditor {
        &mut self.vtf
    }
}

pub fn texture_tool() -> Box<dyn hp_app::Tool> {
    Box::new(TextureTool::new())
}

#[cfg(test)]
mod tests;
