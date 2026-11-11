mod browse;
mod catalog;
mod color;
mod cx;
mod dialogs;
mod display_color;
mod edit;
mod find_replace;
mod help;
mod inspect;
pub mod instance_ui;
mod io_ui;
mod keyvalues;
pub mod max_distance;
pub mod model_import;
pub mod model_tools;
mod models;
mod names;
mod panel;
mod panel_tabs;
mod particles;
mod pick;
mod smartedit;
mod sound_browser;
mod sounds;
mod soundscapes;
mod tr;
pub mod values;
mod view;
mod widgets;

#[cfg(test)]
mod test_fgd;
#[cfg(test)]
mod tests;

pub use cx::{MAX_RECENT_MODELS, ModelPick, model_pick, push_recent_model, recent_models, set_model_pick};
pub use dialogs::{EntityDialogs, FindMode, KindFilter};
pub use edit::{Applied, ConnField, Op, apply, entities_using};
pub use keyvalues::{can_paste_keyvalues, copy_keyvalues, paste_keyvalues};
pub use models::ModelBrowser;
pub use particles::ParticleBrowser;
pub use pick::{asset_value, entities_matching_target, finish_asset_pick, is_vrad_light};
pub use sound_browser::{SoundBrowser, SoundMode};

pub const SOUND_TAB: &str = sound_browser::TAB_ID;
pub const PARTICLE_TAB: &str = particles::TAB_ID;
pub use names::{MapNames, SPECIAL_TARGETS, match_score, rank, suggest};
pub use panel::{EntityPanel, EntityTab};
pub use tr::TR;
pub use view::{
    Agg, CommonConnection, ConnIssues, ConnRow, DocStamp, EditKind, EntityView, SelectionSummary, Target,
    compiler_world_props,
};

pub fn register_translations() {
    hp_app::i18n::register(TR);
    hp_app::i18n::register(model_import::batch::TR);
}
