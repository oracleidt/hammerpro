use crate::i18n::TrTable;

#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash)]
pub enum MenuGroup {
    FileNew,
    FileClose,
    FileSave,
    FileImport,
    FileImportOpen,
    FileExport,
    FileRun,
    FileGame,
    FilePublish,
    FileExit,
    EditHistory,
    EditFind,
    EditClipboard,
    EditSelect,
    EditSelectMode,
    EditSelectGroups,
    EditSelectElement,
    EditSelectEdges,
    EditFaceHistory,
    EditProperties,
    EditColors,
    MapGrid,
    MapGrids,
    MapInfo,
    MapPointfile,
    MapPortals,
    MapOptimize,
    MapNavMesh,
    MapProperties,
    MapLighting,
    MapLights,
    MapCubemaps,
    ViewType,
    ViewRender,
    ViewPlayer,
    ViewShow,
    ViewDisplay,
    ViewDisplay2d,
    ViewDisplayInfo,
    ViewSky,
    ViewFog,
    ViewWater,
    ViewEnvironment,
    ViewEffects,
    ViewEffectsObjects,
    ViewDiagnostics,
    ViewHide,
    ViewNav,
    ViewZoom,
    ViewPanels,
    ToolsBrush,
    ToolsAlign,
    ToolsFlip,
    ToolsArrange,
    ToolsOrigin,
    ToolsGroup,
    ToolsCsg,
    ToolsBrushOps,
    ToolsFaceOps,
    ToolsBrushMisc,
    ToolsMesh,
    ToolsDisp,
    ToolsDispSlope,
    ToolsDispNoise,
    ToolsTerrain,
    ToolsCurves,
    ToolsRoads,
    ToolsTextures,
    ToolsPlaceObjects,
    ToolsPlacement,
    ToolsScatter,
    ToolsProps,
    ToolsPaths,
    ToolsInstancing,
    ToolsInstanceDisplay,
    ToolsSwitch,
    ToolsContent,
    ToolsContentIndex,
    ToolsBlender,
    ToolsMcp,
    ToolsLiveLink,
    ToolsApp,
    WindowLayout,
    WindowMessages,
    WindowMaps,
    HelpDocs,
    HelpSupport,
    HelpAbout,
    OtherEditors,
    Hidden,
}

#[derive(Clone, Copy, Debug, PartialEq)]
pub struct GroupInfo {
    pub section: u16,
    pub order: u16,
    pub submenu: Option<&'static str>,
    pub width: f32,
}

const fn inline(section: u16, order: u16) -> Option<GroupInfo> {
    Some(GroupInfo { section, order, submenu: None, width: 0.0 })
}

const fn sub(section: u16, order: u16, key: &'static str, width: f32) -> Option<GroupInfo> {
    Some(GroupInfo { section, order, submenu: Some(key), width })
}

impl MenuGroup {
    pub const fn info(self) -> Option<GroupInfo> {
        match self {
            MenuGroup::FileNew => inline(10, 0),
            MenuGroup::FileClose => inline(10, 20),
            MenuGroup::FileSave => inline(20, 0),
            MenuGroup::FileImport => sub(30, 0, "menu.import", 260.0),
            MenuGroup::FileImportOpen => sub(30, 1, "menu.import", 260.0),
            MenuGroup::FileExport => sub(30, 10, "menu.export", 240.0),
            MenuGroup::FileRun => sub(40, 0, "menu.compile", 240.0),
            MenuGroup::FileGame => sub(40, 1, "menu.compile", 240.0),
            MenuGroup::FilePublish => sub(40, 10, "menu.publish", 240.0),
            MenuGroup::FileExit => inline(90, 0),
            MenuGroup::EditHistory => inline(10, 0),
            MenuGroup::EditClipboard => inline(20, 0),
            MenuGroup::EditSelect => inline(30, 0),
            MenuGroup::EditSelectMode => sub(30, 10, "menu.selection_mode", 180.0),
            MenuGroup::EditSelectGroups => sub(30, 11, "menu.selection_mode", 180.0),
            MenuGroup::EditSelectElement => sub(30, 20, "menu.select_element", 220.0),
            MenuGroup::EditSelectEdges => sub(30, 21, "menu.select_element", 220.0),
            MenuGroup::EditFaceHistory => sub(30, 22, "menu.select_element", 220.0),
            MenuGroup::EditFind => inline(40, 0),
            MenuGroup::EditProperties => inline(50, 0),
            MenuGroup::EditColors => sub(50, 10, "menu.colors", 200.0),
            MenuGroup::MapGrid => inline(10, 0),
            MenuGroup::MapGrids => sub(10, 10, "menu.grids", 240.0),
            MenuGroup::MapInfo => inline(20, 0),
            MenuGroup::MapPointfile => sub(20, 10, "menu.leaks_visibility", 240.0),
            MenuGroup::MapPortals => sub(20, 11, "menu.leaks_visibility", 240.0),
            MenuGroup::MapOptimize => sub(20, 12, "menu.leaks_visibility", 240.0),
            MenuGroup::MapNavMesh => sub(20, 13, "menu.leaks_visibility", 240.0),
            MenuGroup::MapProperties => inline(30, 0),
            MenuGroup::MapLighting => sub(40, 0, "menu.lighting", 240.0),
            MenuGroup::MapLights => sub(40, 1, "menu.lighting", 240.0),
            MenuGroup::MapCubemaps => sub(40, 2, "menu.lighting", 240.0),
            MenuGroup::ViewType => sub(10, 0, "menu.view_type", 200.0),
            MenuGroup::ViewRender => sub(10, 10, "menu.render_mode", 220.0),
            MenuGroup::ViewPlayer => inline(10, 20),
            MenuGroup::ViewShow => sub(20, 0, "menu.display", 240.0),
            MenuGroup::ViewDisplay => sub(20, 1, "menu.display", 240.0),
            MenuGroup::ViewDisplay2d => sub(20, 2, "menu.display", 240.0),
            MenuGroup::ViewDisplayInfo => sub(20, 3, "menu.display", 240.0),
            MenuGroup::ViewSky => sub(20, 10, "menu.environment", 240.0),
            MenuGroup::ViewFog => sub(20, 11, "menu.environment", 240.0),
            MenuGroup::ViewWater => sub(20, 12, "menu.environment", 240.0),
            MenuGroup::ViewEnvironment => sub(20, 13, "menu.environment", 240.0),
            MenuGroup::ViewEffects => sub(20, 20, "menu.effects", 220.0),
            MenuGroup::ViewEffectsObjects => sub(20, 21, "menu.effects", 220.0),
            MenuGroup::ViewDiagnostics => sub(20, 30, "menu.diagnostics", 220.0),
            MenuGroup::ViewHide => inline(30, 0),
            MenuGroup::ViewNav => inline(40, 0),
            MenuGroup::ViewZoom => sub(40, 10, "menu.zoom_distance", 220.0),
            MenuGroup::ViewPanels => sub(50, 0, "menu.panels", 200.0),
            MenuGroup::ToolsSwitch => sub(10, 0, "menu.switch_tool", 220.0),
            MenuGroup::ToolsBrush => inline(20, 0),
            MenuGroup::ToolsAlign => sub(20, 10, "menu.arrange", 260.0),
            MenuGroup::ToolsFlip => sub(20, 11, "menu.arrange", 260.0),
            MenuGroup::ToolsArrange => sub(20, 12, "menu.arrange", 260.0),
            MenuGroup::ToolsOrigin => sub(20, 13, "menu.arrange", 260.0),
            MenuGroup::ToolsGroup => inline(30, 0),
            MenuGroup::ToolsCsg => inline(40, 0),
            MenuGroup::ToolsBrushOps => sub(40, 10, "menu.brush_ops", 220.0),
            MenuGroup::ToolsFaceOps => sub(40, 11, "menu.brush_ops", 220.0),
            MenuGroup::ToolsBrushMisc => sub(40, 12, "menu.brush_ops", 220.0),
            MenuGroup::ToolsMesh => sub(40, 20, "menu.vertices", 240.0),
            MenuGroup::ToolsDisp => sub(40, 30, "menu.displacements", 260.0),
            MenuGroup::ToolsDispSlope => sub(40, 31, "menu.displacements", 260.0),
            MenuGroup::ToolsDispNoise => sub(40, 32, "menu.displacements", 260.0),
            MenuGroup::ToolsTerrain => sub(40, 33, "menu.displacements", 260.0),
            MenuGroup::ToolsCurves => sub(40, 40, "menu.curves_roads", 220.0),
            MenuGroup::ToolsRoads => sub(40, 41, "menu.curves_roads", 220.0),
            MenuGroup::ToolsTextures => sub(40, 50, "menu.textures", 220.0),
            MenuGroup::ToolsPlaceObjects => sub(60, 0, "menu.placement", 260.0),
            MenuGroup::ToolsPlacement => sub(60, 1, "menu.placement", 260.0),
            MenuGroup::ToolsScatter => sub(60, 2, "menu.placement", 260.0),
            MenuGroup::ToolsProps => sub(60, 3, "menu.placement", 260.0),
            MenuGroup::ToolsPaths => sub(60, 4, "menu.placement", 260.0),
            MenuGroup::ToolsInstancing => sub(60, 10, "menu.instancing", 220.0),
            MenuGroup::ToolsInstanceDisplay => sub(60, 11, "menu.instancing", 220.0),
            MenuGroup::ToolsContent => sub(60, 20, "menu.content", 220.0),
            MenuGroup::ToolsContentIndex => sub(60, 21, "menu.content", 220.0),
            MenuGroup::ToolsBlender => sub(60, 30, "menu.integrations", 260.0),
            MenuGroup::ToolsMcp => sub(60, 31, "menu.integrations", 260.0),
            MenuGroup::ToolsLiveLink => sub(60, 32, "menu.integrations", 260.0),
            MenuGroup::ToolsApp => inline(90, 0),
            MenuGroup::WindowLayout => sub(10, 0, "menu.layout", 200.0),
            MenuGroup::WindowMessages => inline(20, 0),
            MenuGroup::WindowMaps => inline(30, 0),
            MenuGroup::HelpDocs => inline(10, 0),
            MenuGroup::HelpSupport => inline(20, 0),
            MenuGroup::HelpAbout => inline(90, 0),
            MenuGroup::OtherEditors => inline(10, 0),
            MenuGroup::Hidden => None,
        }
    }

    pub const fn category(self) -> Option<super::Category> {
        use super::Category as C;
        Some(match self {
            MenuGroup::OtherEditors => C::Other,
            MenuGroup::FileNew
            | MenuGroup::FileClose
            | MenuGroup::FileSave
            | MenuGroup::FileImport
            | MenuGroup::FileImportOpen
            | MenuGroup::FileExport
            | MenuGroup::FileRun
            | MenuGroup::FileGame
            | MenuGroup::FilePublish
            | MenuGroup::FileExit => C::File,
            MenuGroup::EditHistory
            | MenuGroup::EditFind
            | MenuGroup::EditClipboard
            | MenuGroup::EditSelect
            | MenuGroup::EditSelectMode
            | MenuGroup::EditSelectGroups
            | MenuGroup::EditSelectElement
            | MenuGroup::EditSelectEdges
            | MenuGroup::EditFaceHistory
            | MenuGroup::EditProperties
            | MenuGroup::EditColors => C::Edit,
            MenuGroup::MapGrid
            | MenuGroup::MapGrids
            | MenuGroup::MapInfo
            | MenuGroup::MapPointfile
            | MenuGroup::MapPortals
            | MenuGroup::MapOptimize
            | MenuGroup::MapNavMesh
            | MenuGroup::MapProperties
            | MenuGroup::MapLighting
            | MenuGroup::MapLights
            | MenuGroup::MapCubemaps => C::Map,
            MenuGroup::ViewType
            | MenuGroup::ViewRender
            | MenuGroup::ViewPlayer
            | MenuGroup::ViewShow
            | MenuGroup::ViewDisplay
            | MenuGroup::ViewDisplay2d
            | MenuGroup::ViewDisplayInfo
            | MenuGroup::ViewSky
            | MenuGroup::ViewFog
            | MenuGroup::ViewWater
            | MenuGroup::ViewEnvironment
            | MenuGroup::ViewEffects
            | MenuGroup::ViewEffectsObjects
            | MenuGroup::ViewDiagnostics
            | MenuGroup::ViewHide
            | MenuGroup::ViewNav
            | MenuGroup::ViewZoom
            | MenuGroup::ViewPanels => C::View,
            MenuGroup::ToolsBrush
            | MenuGroup::ToolsAlign
            | MenuGroup::ToolsFlip
            | MenuGroup::ToolsArrange
            | MenuGroup::ToolsOrigin
            | MenuGroup::ToolsGroup
            | MenuGroup::ToolsCsg
            | MenuGroup::ToolsBrushOps
            | MenuGroup::ToolsFaceOps
            | MenuGroup::ToolsBrushMisc
            | MenuGroup::ToolsMesh
            | MenuGroup::ToolsDisp
            | MenuGroup::ToolsDispSlope
            | MenuGroup::ToolsDispNoise
            | MenuGroup::ToolsTerrain
            | MenuGroup::ToolsCurves
            | MenuGroup::ToolsRoads
            | MenuGroup::ToolsTextures
            | MenuGroup::ToolsPlaceObjects
            | MenuGroup::ToolsPlacement
            | MenuGroup::ToolsScatter
            | MenuGroup::ToolsProps
            | MenuGroup::ToolsPaths
            | MenuGroup::ToolsInstancing
            | MenuGroup::ToolsInstanceDisplay
            | MenuGroup::ToolsSwitch
            | MenuGroup::ToolsContent
            | MenuGroup::ToolsContentIndex
            | MenuGroup::ToolsBlender
            | MenuGroup::ToolsMcp
            | MenuGroup::ToolsLiveLink
            | MenuGroup::ToolsApp => C::Tools,
            MenuGroup::WindowLayout | MenuGroup::WindowMessages | MenuGroup::WindowMaps => C::Window,
            MenuGroup::HelpDocs | MenuGroup::HelpSupport | MenuGroup::HelpAbout => C::Help,
            MenuGroup::Hidden => return None,
        })
    }

    pub const fn title(self) -> Option<&'static str> {
        match self {
            MenuGroup::ToolsAlign => Some("menu.align"),
            MenuGroup::ToolsFlip => Some("menu.flip"),
            MenuGroup::ToolsPlaceObjects => Some("menu.place_objects"),
            _ => match self.info() {
                Some(GroupInfo { submenu: Some(key), .. }) => Some(key),
                _ => None,
            },
        }
    }

    pub const fn is_visible(self) -> bool {
        self.info().is_some()
    }
}

pub const TR: TrTable = &[
    ("menu.instancing", "Instancing", "Instances"),
    ("menu.lighting", "Lighting", "Éclairage"),
    ("menu.grids", "Grids", "Grilles"),
    ("menu.edge_selection", "Edge selection", "Sélection d'arêtes"),
    ("menu.view_type", "View type", "Type de vue"),
    ("menu.display", "Display options", "Options d'affichage"),
    ("menu.layout", "Layout", "Disposition"),
    ("menu.import", "Import", "Importer"),
    ("menu.export", "Export", "Exporter"),
    ("menu.compile", "Compile", "Compiler"),
    ("menu.publish", "Publish", "Publier"),
    ("menu.colors", "Display colors", "Couleurs d'affichage"),
    ("menu.leaks_visibility", "Leaks and visibility", "Fuites et visibilité"),
    ("menu.environment", "Environment preview", "Aperçu de l'environnement"),
    ("menu.effects", "Effects", "Effets"),
    ("menu.diagnostics", "Diagnostics", "Diagnostics"),
    ("menu.zoom_distance", "Zoom and draw distance", "Zoom et distance d'affichage"),
    ("menu.switch_tool", "Active tool", "Outil actif"),
    ("menu.arrange", "Arrange", "Disposer"),
    ("menu.place_objects", "Place on real shapes", "Placer sur les formes réelles"),
    ("menu.brush_ops", "Brush operations", "Opérations sur les brushes"),
    ("menu.vertices", "Vertices and edges", "Sommets et arêtes"),
    ("menu.displacements", "Displacements", "Displacements"),
    ("menu.curves_roads", "Curves and roads", "Courbes et routes"),
    ("menu.textures", "Textures", "Textures"),
    ("menu.placement", "Placement and props", "Placement et props"),
    ("menu.content", "Content and editors", "Contenu et éditeurs"),
    ("menu.integrations", "Integrations", "Intégrations"),
];
