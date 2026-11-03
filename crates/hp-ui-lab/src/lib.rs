pub mod bsp;
pub mod compare;
pub mod logic;
pub mod nav;
pub mod nav_gen;
pub mod particles;
pub mod sniffer;
mod tr;
mod tr_sniffer;

pub use tr::TR;
pub const SNIFFER_TR: hp_app::i18n::TrTable = tr_sniffer::TR;

pub fn register_translations() {
    hp_app::register_translations();
    hp_app::i18n::register(TR);
    hp_app::i18n::register(tr_sniffer::TR);
}
