use glam::DVec3;
use hp_app::Action;
use hp_app::actions::MenuGroup;
use hp_app::view::ViewInfo;

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum MenuEntry {
    Action(Action),
    Submenu(MenuGroup),
    CenterHere,
    CreateHere,
    Separator,
}

pub const ENTRIES: &[MenuEntry] = &[
    MenuEntry::Action(Action::Undo),
    MenuEntry::Action(Action::Redo),
    MenuEntry::Separator,
    MenuEntry::Action(Action::Cut),
    MenuEntry::Action(Action::Copy),
    MenuEntry::Action(Action::Paste),
    MenuEntry::Action(Action::Delete),
    MenuEntry::Action(Action::Duplicate),
    MenuEntry::Separator,
    MenuEntry::Action(Action::Group),
    MenuEntry::Action(Action::Ungroup),
    MenuEntry::Action(Action::TieToEntity),
    MenuEntry::Action(Action::MoveToWorld),
    MenuEntry::Action(Action::HideSelected),
    MenuEntry::Separator,
    MenuEntry::Action(Action::Carve),
    MenuEntry::Action(Action::MakeHollow),
    MenuEntry::Action(Action::SnapToGrid),
    MenuEntry::Action(Action::SnapToGridIndividually),
    MenuEntry::Action(Action::TransformDialog),
    MenuEntry::Submenu(MenuGroup::ToolsAlign),
    MenuEntry::Submenu(MenuGroup::ToolsFlip),
    MenuEntry::Action(Action::MoveToFloor),
    MenuEntry::Action(Action::CenterOrigins),
    MenuEntry::Action(Action::RandomRotate),
    MenuEntry::Separator,
    MenuEntry::CenterHere,
    MenuEntry::CreateHere,
    MenuEntry::Separator,
    MenuEntry::Action(Action::Properties),
];

pub fn submenu_actions(group: MenuGroup) -> Vec<Action> {
    Action::ALL.iter().copied().filter(|a| a.group() == group).collect()
}

pub fn submenu_title(group: MenuGroup) -> &'static str {
    group.info().and_then(|g| g.submenu).unwrap_or("menu.view")
}

#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub struct MenuContext {
    pub has_point: bool,
    pub has_entity_class: bool,
}

pub fn entry_enabled(entry: MenuEntry, cx: MenuContext, action_enabled: impl Fn(Action) -> bool) -> bool {
    match entry {
        MenuEntry::Action(action) => action_enabled(action),
        MenuEntry::Submenu(group) => submenu_actions(group).into_iter().any(action_enabled),
        MenuEntry::CenterHere => cx.has_point,
        MenuEntry::CreateHere => cx.has_point && cx.has_entity_class,
        MenuEntry::Separator => false,
    }
}

#[derive(Clone, Copy, Debug)]
pub struct OpenMenu {
    pub view: ViewInfo,
    pub pos: egui::Pos2,
    pub point: Option<DVec3>,
    pub normal: Option<DVec3>,
    pub opened_frame: u64,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum MenuChoice {
    Action(Action),
    CenterHere,
    CreateHere,
    Tool(u32),
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn layout_is_sane() {
        assert!(!matches!(ENTRIES.first(), Some(MenuEntry::Separator)));
        assert!(!matches!(ENTRIES.last(), Some(MenuEntry::Separator)));
        for w in ENTRIES.windows(2) {
            assert!(!(w[0] == MenuEntry::Separator && w[1] == MenuEntry::Separator));
        }
        for required in [
            Action::Undo,
            Action::Redo,
            Action::Cut,
            Action::Copy,
            Action::Paste,
            Action::Delete,
            Action::Duplicate,
            Action::Group,
            Action::Ungroup,
            Action::TieToEntity,
            Action::MoveToWorld,
            Action::HideSelected,
            Action::Carve,
            Action::MakeHollow,
            Action::SnapToGrid,
            Action::TransformDialog,
            Action::MoveToFloor,
            Action::CenterOrigins,
            Action::Properties,
        ] {
            assert!(ENTRIES.contains(&MenuEntry::Action(required)), "{required:?}");
        }
        assert!(ENTRIES.contains(&MenuEntry::CenterHere));
        assert!(ENTRIES.contains(&MenuEntry::CreateHere));
        let align = submenu_actions(MenuGroup::ToolsAlign);
        assert!(align.starts_with(&[Action::AlignLeft, Action::AlignRight, Action::AlignTop, Action::AlignBottom]));
        let flip = submenu_actions(MenuGroup::ToolsFlip);
        assert!(flip.starts_with(&[Action::FlipX, Action::FlipY, Action::FlipZ]));
        assert_eq!(submenu_title(MenuGroup::ToolsAlign), "menu.align");
        assert_eq!(submenu_title(MenuGroup::ToolsFlip), "menu.flip");
    }

    #[test]
    fn enabled_states() {
        let none = MenuContext::default();
        let selected = |a: Action| !matches!(a, Action::Cut | Action::Delete | Action::AlignLeft);
        assert!(entry_enabled(MenuEntry::Action(Action::Paste), none, selected));
        assert!(!entry_enabled(MenuEntry::Action(Action::Cut), none, selected));
        assert!(entry_enabled(MenuEntry::Submenu(MenuGroup::ToolsAlign), none, selected), "one align action works");
        assert!(!entry_enabled(MenuEntry::Submenu(MenuGroup::ToolsFlip), none, |_| false));
        assert!(!entry_enabled(MenuEntry::CenterHere, none, selected));
        assert!(!entry_enabled(MenuEntry::CreateHere, none, selected));
        let pt = MenuContext { has_point: true, ..none };
        assert!(entry_enabled(MenuEntry::CenterHere, pt, selected));
        assert!(!entry_enabled(MenuEntry::CreateHere, pt, selected));
        let all = MenuContext { has_point: true, has_entity_class: true };
        assert!(entry_enabled(MenuEntry::CreateHere, all, selected));
        assert!(!entry_enabled(MenuEntry::Separator, all, selected));
        let state = hp_app::testing::test_state();
        if !Action::MoveToFloor.is_ready() {
            assert!(!entry_enabled(MenuEntry::Action(Action::MoveToFloor), all, |a| a.enabled(&state)));
        }
        assert!(!entry_enabled(MenuEntry::Action(Action::Delete), all, |a| a.enabled(&state)), "nothing selected");
        assert!(entry_enabled(MenuEntry::Action(Action::Paste), all, |a| a.enabled(&state)));
    }
}
