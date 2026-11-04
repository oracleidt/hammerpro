use hp_geom::Aabb;

use crate::view::ViewInfo;

#[derive(Debug, Default)]
pub struct BrushRuntime {
    pub last_created: Option<Aabb>,
    pub bevel_start: Option<(ViewInfo, egui::Pos2)>,
    pub curve_edit: Option<hp_editor::Id>,
    pub road_edit: Option<hp_editor::Id>,
}
