use hp_app::i18n::TrTable;
use hp_app::runtime::ErosionDialog;
use hp_app::{Action, Severity, ToolId, t};
use hp_tools_mesh::disp::{commands, terrain_windows};

use super::Package;
use crate::app::HammerProApp;

pub(crate) struct Mesh;

#[derive(Default)]
pub(crate) struct State {
    terrain: hp_tools_mesh::terrain::ui::TerrainUi,
}

const HANDLED: &[Action] = &[
    Action::VertexSplitFace,
    Action::VertexScale,
    Action::VertexWeld,
    Action::VertexWeldDistance,
    Action::VertexCleanup,
    Action::VertexRotate,
    Action::EdgeSelectLoop,
    Action::EdgeSelectRing,
    Action::EdgeGrow,
    Action::EdgeShrink,
    Action::EdgeSelectBoundary,
    Action::EdgeSlide,
    Action::DispSmoothLoop,
    Action::DispLoopHeight,
    Action::DispSubdivide,
    Action::DispInvertAlpha,
    Action::DispNoise,
    Action::DispFractalNoise,
    Action::DispThermalErosion,
    Action::DispHydraulicErosion,
    Action::ImportHeightmap,
    Action::ExportHeightmap,
    Action::GenerateTerrain,
    Action::DispBlendHeights,
    Action::DispLimitSlope,
    Action::DispUniformSlope,
    Action::DispSelectBySlope,
    Action::ToggleUvLock,
];

impl Package for Mesh {
    fn handled(&self) -> &'static [Action] {
        HANDLED
    }

    fn handle(&self, app: &mut HammerProApp, _ctx: &egui::Context, action: Action) -> bool {
        match action {
            Action::VertexSplitFace
            | Action::VertexScale
            | Action::VertexWeld
            | Action::VertexWeldDistance
            | Action::VertexCleanup
            | Action::VertexRotate
            | Action::EdgeSelectLoop
            | Action::EdgeSelectRing
            | Action::EdgeGrow
            | Action::EdgeShrink
            | Action::EdgeSelectBoundary
            | Action::EdgeSlide => {
                if app.state.tool != ToolId::Vertex && app.has_tool(ToolId::Vertex) {
                    app.switch_tool(ToolId::Vertex);
                }
                if !app.active_tool_action(action) {
                    app.state.notify(Severity::Info, t!("mesh.app.vertex_tool_only"));
                }
            }
            Action::DispSmoothLoop | Action::DispLoopHeight => {
                if app.state.tool != ToolId::Displacement && app.has_tool(ToolId::Displacement) {
                    app.switch_tool(ToolId::Displacement);
                }
                if !app.active_tool_action(action) {
                    app.state.notify(Severity::Info, t!("mesh.app.disp_tool_only"));
                }
            }
            Action::DispSubdivide => {
                commands::subdivide(&mut app.state);
            }
            Action::DispInvertAlpha => {
                commands::invert_alpha(&mut app.state);
            }
            Action::DispNoise => app.state.runtime.mesh.noise_dialog = true,
            Action::DispFractalNoise => {
                if let Some(tool) = app.tools.iter_mut().find(|t| t.id() == ToolId::Displacement) {
                    tool.set_option(&mut app.state, "noise_type", "fractal");
                }
                app.state.runtime.mesh.noise_dialog = true;
            }
            Action::DispThermalErosion => app.state.runtime.mesh.erosion_dialog = Some(ErosionDialog::Thermal),
            Action::DispHydraulicErosion => app.state.runtime.mesh.erosion_dialog = Some(ErosionDialog::Hydraulic),
            Action::ImportHeightmap => app.pkg.mesh.terrain.import_open = true,
            Action::ExportHeightmap => app.pkg.mesh.terrain.export_open = true,
            Action::GenerateTerrain => app.pkg.mesh.terrain.generate_open = true,
            Action::DispBlendHeights
            | Action::DispLimitSlope
            | Action::DispUniformSlope
            | Action::DispSelectBySlope => {
                if app.state.tool != ToolId::Displacement && app.has_tool(ToolId::Displacement) {
                    app.switch_tool(ToolId::Displacement);
                }
                app.active_tool_action(action);
            }
            Action::ToggleUvLock => {
                let s = &mut app.state.settings.mesh;
                s.uv_lock = !s.uv_lock;
                app.state.settings_dirty = true;
            }
            _ => return false,
        }
        true
    }

    fn dialogs(&self, app: &mut HammerProApp, ctx: &egui::Context) {
        terrain_windows::noise_window(ctx, &mut app.state, displacement_tool(&mut app.tools));
        terrain_windows::erosion_window(ctx, &mut app.state, displacement_tool(&mut app.tools));
        hp_tools_mesh::terrain::ui::show(ctx, &mut app.pkg.mesh.terrain, &mut app.state);
    }

    fn translations(&self) -> TrTable {
        TR
    }
}

fn displacement_tool(tools: &mut [Box<dyn hp_app::Tool>]) -> Option<&mut dyn hp_app::Tool> {
    for t in tools.iter_mut() {
        if t.id() == ToolId::Displacement {
            return Some(&mut **t);
        }
    }
    None
}

const TR: TrTable = &[
    (
        "mesh.app.vertex_tool_only",
        "This command works in the vertex tool.",
        "Cette commande fonctionne dans l'outil sommets.",
    ),
    (
        "mesh.app.disp_tool_only",
        "This command works in the displacement tool.",
        "Cette commande fonctionne dans l'outil displacement.",
    ),
];
