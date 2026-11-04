pub mod cordon;
pub mod fog;
pub mod helpers;
pub mod instances;
pub mod labels;
pub mod mesh_disp;
pub mod pointfile;
pub mod portals;
pub mod render;
pub mod roadside;
pub mod terrain;

use hp_app::AppState;
use hp_app::i18n::TrTable;
use hp_app::view::ViewInfo;
use hp_render::Overlay;

pub trait OverlayLayer {
    fn name(&self) -> &'static str;

    fn prepare(&mut self, _state: &AppState) {}

    fn add(&self, _state: &AppState, _view: &ViewInfo, _out: &mut Overlay) {}

    fn paint(&self, _state: &AppState, _view: &ViewInfo, _painter: &egui::Painter) {}

    fn retained_version(&self, _state: &AppState, _view: &ViewInfo) -> Option<u64> {
        None
    }

    fn retained(&self, _state: &AppState, _view: &ViewInfo, _out: &mut Overlay) {}
}

pub(crate) const TABLES: &[TrTable] =
    &[pointfile::TR, cordon::TR, portals::TR, instances::TR, helpers::TR, labels::TR, render::TR, fog::TR, terrain::TR];

pub fn default_layers() -> Vec<Box<dyn OverlayLayer>> {
    let mut list = pointfile::layers();
    list.extend(cordon::layers());
    list.extend(portals::layers());
    list.extend(roadside::layers());
    list.extend(instances::layers());
    list.extend(helpers::layers());
    list.extend(labels::layers());
    list.extend(fog::layers());
    list.extend(render::layers());
    list.extend(terrain::layers());
    list.extend(mesh_disp::layers());
    list
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn layer_tables_are_clean() {
        let mut seen: std::collections::HashSet<&str> = crate::TR.iter().map(|(k, _, _)| *k).collect();
        for table in TABLES {
            let problems = hp_app::i18n::lint_table(table);
            assert!(problems.is_empty(), "{problems:#?}");
            for (key, en, fr) in *table {
                assert!(key.starts_with("viewport."), "key {key} must use the viewport. prefix");
                assert!(seen.insert(key), "key {key} is defined twice");
                assert!(
                    !hp_app::strings::TR.iter().any(|(k, _, _)| k == key),
                    "key {key} collides with the core table"
                );
                for i in 0..4 {
                    let p = format!("{{{i}}}");
                    assert_eq!(en.contains(&p), fr.contains(&p), "{key}: placeholder {p} differs");
                }
            }
        }
    }

    #[test]
    fn layer_names_are_unique() {
        let layers = default_layers();
        let mut names: Vec<&str> = layers.iter().map(|l| l.name()).collect();
        assert!(names.contains(&"pointfile"));
        names.sort_unstable();
        let n = names.len();
        names.dedup();
        assert_eq!(names.len(), n, "duplicate layer names");
    }
}
