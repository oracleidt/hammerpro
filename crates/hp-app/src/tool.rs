use serde::{Deserialize, Serialize};

#[derive(Clone, Copy, Debug, Default, PartialEq, Eq, Hash, Serialize, Deserialize)]
pub enum ToolId {
    #[default]
    Select,
    Camera,
    Entity,
    Block,
    Texture,
    Decal,
    Overlay,
    Clip,
    Extrude,
    Bevel,
    Vertex,
    Path,
    Polygon,
    Curve,
    Sprinkle,
    Displacement,
    Cordon,
    Road,
    Pipe,
    Slice,
    Physics,
    Spline,
    Foliage,
}

impl ToolId {
    pub const ALL: [ToolId; 23] = [
        ToolId::Select,
        ToolId::Camera,
        ToolId::Entity,
        ToolId::Block,
        ToolId::Texture,
        ToolId::Decal,
        ToolId::Overlay,
        ToolId::Clip,
        ToolId::Slice,
        ToolId::Extrude,
        ToolId::Bevel,
        ToolId::Vertex,
        ToolId::Path,
        ToolId::Polygon,
        ToolId::Curve,
        ToolId::Sprinkle,
        ToolId::Displacement,
        ToolId::Cordon,
        ToolId::Road,
        ToolId::Pipe,
        ToolId::Physics,
        ToolId::Spline,
        ToolId::Foliage,
    ];

    pub fn label_key(self) -> &'static str {
        match self {
            ToolId::Select => "tool.select",
            ToolId::Camera => "tool.camera",
            ToolId::Entity => "tool.entity",
            ToolId::Block => "tool.block",
            ToolId::Texture => "tool.texture",
            ToolId::Decal => "tool.decal",
            ToolId::Overlay => "tool.overlay",
            ToolId::Clip => "tool.clip",
            ToolId::Extrude => "tool.extrude",
            ToolId::Bevel => "tool.bevel",
            ToolId::Vertex => "tool.vertex",
            ToolId::Path => "tool.path",
            ToolId::Displacement => "tool.displacement",
            ToolId::Polygon => "tool.polygon",
            ToolId::Curve => "tool.curve",
            ToolId::Cordon => "tool.cordon",
            ToolId::Sprinkle => "tool.sprinkle",
            ToolId::Road => "tool.road",
            ToolId::Pipe => "tool.pipe",
            ToolId::Slice => "tool.slice",
            ToolId::Physics => "tool.physics",
            ToolId::Spline => "tool.spline",
            ToolId::Foliage => "tool.foliage",
        }
    }

    pub fn is_ready(self) -> bool {
        self.action().is_ready()
    }

    pub fn action(self) -> crate::actions::Action {
        use crate::actions::Action;
        match self {
            ToolId::Select => Action::ToolSelect,
            ToolId::Camera => Action::ToolCamera,
            ToolId::Entity => Action::ToolEntity,
            ToolId::Block => Action::ToolBlock,
            ToolId::Texture => Action::ToolTexture,
            ToolId::Decal => Action::ToolDecal,
            ToolId::Overlay => Action::ToolOverlay,
            ToolId::Clip => Action::ToolClip,
            ToolId::Extrude => Action::ToolExtrude,
            ToolId::Bevel => Action::ToolBevel,
            ToolId::Vertex => Action::ToolVertex,
            ToolId::Path => Action::ToolPath,
            ToolId::Displacement => Action::ToolDisplacement,
            ToolId::Polygon => Action::ToolPolygon,
            ToolId::Curve => Action::ToolCurve,
            ToolId::Cordon => Action::ToolCordon,
            ToolId::Sprinkle => Action::ToolSprinkle,
            ToolId::Road => Action::ToolRoad,
            ToolId::Pipe => Action::ToolPipe,
            ToolId::Slice => Action::ToolSlice,
            ToolId::Physics => Action::ToolPhysics,
            ToolId::Spline => Action::ToolSpline,
            ToolId::Foliage => Action::ToolFoliage,
        }
    }

    pub fn icon(self) -> &'static str {
        match self {
            ToolId::Select => "⬉",
            ToolId::Camera => "📷",
            ToolId::Entity => "💡",
            ToolId::Block => "⬛",
            ToolId::Texture => "🕸",
            ToolId::Decal => "🎯",
            ToolId::Overlay => "🗺",
            ToolId::Clip => "✂",
            ToolId::Extrude => "⬆",
            ToolId::Bevel => "◔",
            ToolId::Vertex => "⬟",
            ToolId::Path => "〰",
            ToolId::Displacement => "🗻",
            ToolId::Polygon => "⬣",
            ToolId::Curve => "➰",
            ToolId::Cordon => "⛶",
            ToolId::Sprinkle => "✨",
            ToolId::Road => "☰",
            ToolId::Pipe => "⊟",
            ToolId::Slice => "⊞",
            ToolId::Physics => "⚖",
            ToolId::Spline => "〽",
            ToolId::Foliage => "🌳",
        }
    }
}

pub struct ViewInput<'a> {
    pub view: &'a crate::view::ViewInfo,
    pub response: &'a egui::Response,
    pub pointer: Option<egui::Pos2>,
    pub modifiers: egui::Modifiers,
    pub navigating: bool,
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct ToolMenuEntry {
    pub label: String,
    pub id: u32,
    pub enabled: bool,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum ToolFlow {
    Ignored,
    Handled,
}

pub trait Tool {
    fn id(&self) -> ToolId;

    fn activate(&mut self, _state: &mut crate::state::AppState) {}

    fn update(&mut self, _state: &mut crate::state::AppState, _ctx: &egui::Context) {}

    fn context_entries(&self, _state: &crate::state::AppState) -> Vec<ToolMenuEntry> {
        Vec::new()
    }

    fn context_command(&mut self, _state: &mut crate::state::AppState, _id: u32) {}

    fn wheel(
        &mut self,
        _state: &mut crate::state::AppState,
        _view: &crate::view::ViewInfo,
        _notches: f32,
        _modifiers: egui::Modifiers,
    ) -> ToolFlow {
        ToolFlow::Ignored
    }

    fn reactivate(&mut self, _state: &mut crate::state::AppState) {}

    fn action(&mut self, _state: &mut crate::state::AppState, _action: crate::actions::Action) -> bool {
        false
    }

    fn deactivate(&mut self, _state: &mut crate::state::AppState) {}

    fn view_input(&mut self, state: &mut crate::state::AppState, input: &ViewInput<'_>) -> ToolFlow;

    fn click_while_flying(
        &mut self,
        _state: &mut crate::state::AppState,
        _view: &crate::view::ViewInfo,
        _pos: egui::Pos2,
        _modifiers: egui::Modifiers,
    ) -> ToolFlow {
        ToolFlow::Ignored
    }

    fn key_input(&mut self, _state: &mut crate::state::AppState, _ctx: &egui::Context) -> bool {
        false
    }

    fn overlay(&self, _state: &crate::state::AppState, _view: &crate::view::ViewInfo, _out: &mut hp_render::Overlay) {}

    fn retained_overlay_version(&self, _state: &crate::state::AppState, _view: &crate::view::ViewInfo) -> Option<u64> {
        None
    }

    fn retained_overlay(
        &self,
        _state: &crate::state::AppState,
        _view: &crate::view::ViewInfo,
        _out: &mut hp_render::Overlay,
    ) {
    }

    fn paint(&self, _state: &crate::state::AppState, _view: &crate::view::ViewInfo, _painter: &egui::Painter) {}

    fn options_ui(&mut self, _ui: &mut egui::Ui, _state: &mut crate::state::AppState) {}

    fn takes_right_drag(&self, _kind: crate::view::ViewKind, _modifiers: egui::Modifiers) -> bool {
        false
    }

    fn takes_middle_drag(&self, _kind: crate::view::ViewKind, _modifiers: egui::Modifiers) -> bool {
        false
    }

    fn wants_side_panel(&self) -> bool {
        false
    }

    fn panel_label_key(&self) -> &'static str {
        self.id().label_key()
    }

    fn cursor(
        &self,
        _state: &crate::state::AppState,
        _view: &crate::view::ViewInfo,
        _pos: egui::Pos2,
    ) -> Option<egui::CursorIcon> {
        None
    }

    fn status_hint(&self, _state: &crate::state::AppState) -> Option<String> {
        None
    }

    fn cancel(&mut self, _state: &mut crate::state::AppState) -> bool {
        false
    }

    fn face_selection(&self, _state: &crate::state::AppState) -> Vec<hp_editor::FaceRef> {
        Vec::new()
    }

    fn set_option(&mut self, _state: &mut crate::state::AppState, _key: &str, _value: &str) -> bool {
        false
    }

    fn option_values(&self) -> Vec<ToolOption> {
        Vec::new()
    }

    fn option_order(&self, _key: &str) -> i32 {
        0
    }

    fn refresh_options(&mut self, _state: &crate::state::AppState) {}

    fn scripted_stroke(
        &mut self,
        _state: &mut crate::state::AppState,
        _stroke: &ScriptedStroke,
    ) -> Option<Result<StrokeOutcome, String>> {
        None
    }
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct ToolOption {
    pub key: &'static str,
    pub value: String,
    pub takes: &'static str,
}

impl ToolOption {
    pub fn new(key: &'static str, value: impl Into<String>, takes: &'static str) -> Self {
        Self { key, value: value.into(), takes }
    }
}

#[derive(Clone, Debug, PartialEq)]
pub struct ScriptedStroke {
    pub points: Vec<glam::DVec3>,
    pub invert: bool,
    pub smooth: bool,
    pub boost: bool,
    pub seconds_per_sample: f32,
    pub eye: Option<glam::DVec3>,
    pub forward: Option<glam::DVec3>,
    pub faces: Option<Vec<hp_editor::FaceRef>>,
    pub undo_label: String,
}

#[derive(Clone, Debug, Default, PartialEq, Eq)]
pub struct StrokeOutcome {
    pub samples_painted: usize,
    pub vertices_changed: usize,
    pub faces: Vec<hp_editor::FaceRef>,
    pub note: Option<String>,
}
