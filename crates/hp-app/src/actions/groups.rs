use crate::i18n::TrTable;

#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash)]
pub enum MenuGroup {
    FileNew,
    FileClose,
    FileSave,
    FileImport,
    FileRun,
    FileExit,
    EditHistory,
    EditFind,
    EditClipboard,
    EditSelect,
    EditSelectMode,
    EditSelectEdges,
    EditProperties,
    MapGrid,
    MapGrids,
    MapInfo,
    MapPointfile,
    MapProperties,
    MapLighting,
    ViewType,
    ViewRender,
    ViewShow,
    ViewDisplay,
    ViewHide,
    ViewNav,
    ViewPanels,
    ToolsBrush,
    ToolsAlign,
    ToolsFlip,
    ToolsPlacement,
    ToolsInstancing,
    ToolsSwitch,
    ToolsMesh,
    ToolsContent,
    ToolsApp,
    WindowLayout,
    WindowMessages,
    WindowMaps,
    HelpDocs,
    HelpAbout,
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
            MenuGroup::FileImport => inline(30, 0),
            MenuGroup::FileRun => inline(40, 0),
            MenuGroup::FileExit => inline(90, 0),
            MenuGroup::EditHistory => inline(10, 0),
            MenuGroup::EditFind => inline(20, 0),
            MenuGroup::EditClipboard => inline(30, 0),
            MenuGroup::EditSelect => inline(40, 0),
            MenuGroup::EditSelectMode => sub(40, 10, "menu.selection_mode", 160.0),
            MenuGroup::EditSelectEdges => sub(40, 20, "menu.edge_selection", 220.0),
            MenuGroup::EditProperties => inline(50, 0),
            MenuGroup::MapGrid => inline(10, 0),
            MenuGroup::MapGrids => sub(10, 10, "menu.grids", 240.0),
            MenuGroup::MapInfo => inline(20, 0),
            MenuGroup::MapPointfile => inline(30, 0),
            MenuGroup::MapProperties => inline(40, 0),
            MenuGroup::MapLighting => sub(50, 0, "menu.lighting", 220.0),
            MenuGroup::ViewType => inline(10, 0),
            MenuGroup::ViewRender => sub(10, 10, "menu.render_mode", 200.0),
            MenuGroup::ViewShow => inline(30, 10),
            MenuGroup::ViewDisplay => sub(30, 20, "menu.display", 220.0),
            MenuGroup::ViewHide => inline(35, 0),
            MenuGroup::ViewNav => inline(40, 0),
            MenuGroup::ViewPanels => sub(50, 0, "menu.panels", 200.0),
            MenuGroup::ToolsBrush => inline(10, 0),
            MenuGroup::ToolsAlign => sub(10, 10, "menu.align", 160.0),
            MenuGroup::ToolsFlip => sub(10, 20, "menu.flip", 140.0),
            MenuGroup::ToolsPlacement => inline(20, 0),
            MenuGroup::ToolsInstancing => sub(20, 10, "menu.instancing", 220.0),
            MenuGroup::ToolsSwitch => inline(30, 0),
            MenuGroup::ToolsMesh => inline(40, 0),
            MenuGroup::ToolsContent => inline(50, 0),
            MenuGroup::ToolsApp => inline(90, 0),
            MenuGroup::WindowLayout => sub(10, 0, "menu.layout", 200.0),
            MenuGroup::WindowMessages => inline(20, 0),
            MenuGroup::WindowMaps => inline(30, 0),
            MenuGroup::HelpDocs => inline(10, 0),
            MenuGroup::HelpAbout => inline(90, 0),
            MenuGroup::Hidden => None,
        }
    }

    pub const fn category(self) -> Option<super::Category> {
        use super::Category as C;
        Some(match self {
            MenuGroup::FileNew
            | MenuGroup::FileClose
            | MenuGroup::FileSave
            | MenuGroup::FileImport
            | MenuGroup::FileRun
            | MenuGroup::FileExit => C::File,
            MenuGroup::EditHistory
            | MenuGroup::EditFind
            | MenuGroup::EditClipboard
            | MenuGroup::EditSelect
            | MenuGroup::EditSelectMode
            | MenuGroup::EditSelectEdges
            | MenuGroup::EditProperties => C::Edit,
            MenuGroup::MapGrid
            | MenuGroup::MapGrids
            | MenuGroup::MapInfo
            | MenuGroup::MapPointfile
            | MenuGroup::MapProperties
            | MenuGroup::MapLighting => C::Map,
            MenuGroup::ViewType
            | MenuGroup::ViewRender
            | MenuGroup::ViewShow
            | MenuGroup::ViewDisplay
            | MenuGroup::ViewHide
            | MenuGroup::ViewNav
            | MenuGroup::ViewPanels => C::View,
            MenuGroup::ToolsBrush
            | MenuGroup::ToolsAlign
            | MenuGroup::ToolsFlip
            | MenuGroup::ToolsPlacement
            | MenuGroup::ToolsInstancing
            | MenuGroup::ToolsSwitch
            | MenuGroup::ToolsMesh
            | MenuGroup::ToolsContent
            | MenuGroup::ToolsApp => C::Tools,
            MenuGroup::WindowLayout | MenuGroup::WindowMessages | MenuGroup::WindowMaps => C::Window,
            MenuGroup::HelpDocs | MenuGroup::HelpAbout => C::Help,
            MenuGroup::Hidden => return None,
        })
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
];
