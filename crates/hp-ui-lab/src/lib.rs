pub mod bsp;
pub mod color_edit;
pub mod color_zone;
pub mod compare;
pub mod entity_fx;
pub mod logic;
pub mod media;
pub mod nav;
pub mod nav_gen;
pub mod particle_editor;
pub mod particle_preview;
pub mod particles;
pub mod sniffer;
pub mod sound_edit;
mod tr;
mod tr_sniffer;

pub use tr::TR;
pub const SNIFFER_TR: hp_app::i18n::TrTable = tr_sniffer::TR;
pub const PARTICLE_PREVIEW_TR: hp_app::i18n::TrTable = particle_preview::TR;
pub const SOUND_TR: hp_app::i18n::TrTable = sound_edit::TR;
pub const COLOR_TR: hp_app::i18n::TrTable = color_edit::TR;

pub fn register_translations() {
    hp_app::register_translations();
    hp_app::i18n::register(TR);
    hp_app::i18n::register(tr_sniffer::TR);
    hp_app::i18n::register(SOUND_TR);
    hp_app::i18n::register(COLOR_TR);
    hp_app::i18n::register(particle_preview::TR);
    hp_app::i18n::register(particle_editor::TR);
}
