#![forbid(unsafe_code)]

mod common;
pub mod disp;
pub mod terrain;
mod tr;
pub mod vertex;

#[cfg(test)]
mod tests;

pub use disp::{AlphaEffect, BrushSettings, DisplacementTool, GeoEffect, Interaction, PaintAxis};
pub use tr::TR;
pub use vertex::VertexTool;

pub fn register_translations() {
    hp_app::i18n::register(TR);
    hp_app::i18n::register(disp::erode::TR);
    hp_app::i18n::register(terrain::tr::TR);
    hp_app::i18n::register(vertex::loops::TR);
    hp_app::i18n::register(disp::loops::TR);
    hp_app::i18n::register(disp::blend::TR);
    hp_app::i18n::register(disp::slope_ui::TR);
}

pub fn tools() -> Vec<Box<dyn hp_app::Tool>> {
    vec![Box::new(VertexTool::new()), Box::new(DisplacementTool::new())]
}
