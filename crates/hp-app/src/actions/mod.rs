use egui::{Key, KeyboardShortcut, Modifiers};
use serde::{Deserialize, Serialize};

use crate::i18n::TrTable;
use crate::state::AppState;

mod groups;
mod keys;

pub use groups::{GroupInfo, MenuGroup};
pub use keys::*;

#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash)]
pub enum Category {
    File,
    Edit,
    Map,
    View,
    Tools,
    Window,
    Help,
    Other,
}

impl Category {
    pub const ALL: [Category; 8] = [
        Category::File,
        Category::Edit,
        Category::Map,
        Category::View,
        Category::Tools,
        Category::Window,
        Category::Help,
        Category::Other,
    ];

    pub fn label_key(self) -> &'static str {
        match self {
            Category::File => "menu.file",
            Category::Edit => "menu.edit",
            Category::Map => "menu.map",
            Category::View => "menu.view",
            Category::Tools => "menu.tools",
            Category::Window => "menu.window",
            Category::Help => "menu.help",
            Category::Other => "menu.other",
        }
    }
}

#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash)]
pub enum ActionStatus {
    Ready,
    Planned,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Check {
    None,
    Toggle(bool),
    Radio(bool),
}

const NONE: Modifiers = Modifiers::NONE;
const CTRL: Modifiers = Modifiers::COMMAND;
const SHIFT: Modifiers = Modifiers::SHIFT;
const ALT: Modifiers = Modifiers::ALT;
const CTRL_SHIFT: Modifiers = Modifiers { alt: false, ctrl: false, shift: true, mac_cmd: false, command: true };
const CTRL_ALT: Modifiers = Modifiers { alt: true, ctrl: false, shift: false, mac_cmd: false, command: true };
const ALT_SHIFT: Modifiers = Modifiers { alt: true, ctrl: false, shift: true, mac_cmd: false, command: false };

const fn sc(m: Modifiers, k: Key) -> Option<KeyboardShortcut> {
    Some(KeyboardShortcut::new(m, k))
}

macro_rules! define_actions {
    ($( $variant:ident => $id:literal, $cat:ident, $group:ident, $shortcut:expr, $status:ident; )*) => {
        #[derive(Clone, Copy, Debug, PartialEq, Eq, Hash, Serialize, Deserialize)]
        pub enum Action { $( $variant, )* }

        impl Action {
            pub const DECLARED: &'static [Action] = &[ $( Action::$variant, )* ];

            pub const ALL: &'static [Action] = &READY;

            pub fn id(self) -> &'static str {
                match self { $( Action::$variant => $id, )* }
            }

            pub fn category(self) -> Category {
                match self { $( Action::$variant => Category::$cat, )* }
            }

            pub fn group(self) -> MenuGroup {
                match self { $( Action::$variant => MenuGroup::$group, )* }
            }

            pub fn default_shortcut(self) -> Option<KeyboardShortcut> {
                match self { $( Action::$variant => $shortcut, )* }
            }

            pub const fn status(self) -> ActionStatus {
                match self { $( Action::$variant => ActionStatus::$status, )* }
            }
        }
    };
}

macro_rules! collect_actions {
    (($($done:tt)*)) => {
        define_actions! { $($done)* }
    };
    (($($done:tt)*) $next:ident $($rest:ident)*) => {
        $next! { ($($done)*) $($rest)* }
    };
}

macro_rules! action_list {
    ($name:ident { $($entries:tt)* }) => {
        action_list! { @define ($) $name { $($entries)* } }
    };
    (@define ($d:tt) $name:ident { $($entries:tt)* }) => {
        macro_rules! $name {
            (($d($d done:tt)*) $d($d rest:ident)*) => {
                collect_actions! { ($d($d done)* $($entries)*) $d($d rest)* }
            };
        }
    };
}

#[macro_use]
mod shell;
#[macro_use]
mod selection;
#[macro_use]
mod brush;
#[macro_use]
mod workflow;
#[macro_use]
mod instances;
#[macro_use]
mod visibility;
#[macro_use]
mod helpers;
#[macro_use]
mod mesh;
#[macro_use]
mod entities;
#[macro_use]
mod render;
#[macro_use]
mod lighting;
#[macro_use]
mod placement;
#[macro_use]
mod compile;
#[macro_use]
mod startup;
#[macro_use]
mod launch;
#[macro_use]
mod content;
#[macro_use]
mod health;
#[macro_use]
mod lab;
pub use lab::{is_model_prop, selected_prop};
#[macro_use]
mod mcp;
mod mcp_tools;

collect_actions! { ()
    shell_actions selection_actions brush_actions workflow_actions instances_actions visibility_actions
    helpers_actions mesh_actions entities_actions render_actions lighting_actions placement_actions
    compile_actions startup_actions launch_actions content_actions health_actions lab_actions
    mcp_actions
}

const READY_COUNT: usize = {
    let mut n = 0;
    let mut i = 0;
    while i < Action::DECLARED.len() {
        if matches!(Action::DECLARED[i].status(), ActionStatus::Ready) {
            n += 1;
        }
        i += 1;
    }
    n
};

const READY: [Action; READY_COUNT] = {
    let mut out = [Action::DECLARED[0]; READY_COUNT];
    let mut i = 0;
    let mut j = 0;
    while i < Action::DECLARED.len() {
        if matches!(Action::DECLARED[i].status(), ActionStatus::Ready) {
            out[j] = Action::DECLARED[i];
            j += 1;
        }
        i += 1;
    }
    out
};

macro_rules! first_answer {
    ($f:ident ( $($arg:expr),* )) => {
        shell::$f($($arg),*)
            .or_else(|| selection::$f($($arg),*))
            .or_else(|| brush::$f($($arg),*))
            .or_else(|| workflow::$f($($arg),*))
            .or_else(|| instances::$f($($arg),*))
            .or_else(|| visibility::$f($($arg),*))
            .or_else(|| helpers::$f($($arg),*))
            .or_else(|| mesh::$f($($arg),*))
            .or_else(|| entities::$f($($arg),*))
            .or_else(|| render::$f($($arg),*))
            .or_else(|| lighting::$f($($arg),*))
            .or_else(|| placement::$f($($arg),*))
            .or_else(|| compile::$f($($arg),*))
            .or_else(|| startup::$f($($arg),*))
            .or_else(|| launch::$f($($arg),*))
            .or_else(|| content::$f($($arg),*))
            .or_else(|| health::$f($($arg),*))
            .or_else(|| lab::$f($($arg),*))
            .or_else(|| mcp::$f($($arg),*))
    };
}

const REPEATS: [&[Action]; 19] = [
    shell::REPEATS,
    selection::REPEATS,
    brush::REPEATS,
    workflow::REPEATS,
    instances::REPEATS,
    visibility::REPEATS,
    helpers::REPEATS,
    mesh::REPEATS,
    entities::REPEATS,
    render::REPEATS,
    lighting::REPEATS,
    placement::REPEATS,
    compile::REPEATS,
    startup::REPEATS,
    launch::REPEATS,
    content::REPEATS,
    health::REPEATS,
    lab::REPEATS,
    mcp::REPEATS,
];

pub const TABLES: &[TrTable] = &[
    groups::TR,
    shell::TR,
    selection::TR,
    brush::TR,
    workflow::TR,
    instances::TR,
    visibility::TR,
    helpers::TR,
    mesh::TR,
    entities::TR,
    render::TR,
    lighting::TR,
    placement::TR,
    compile::TR,
    startup::TR,
    launch::TR,
    content::TR,
    health::TR,
    lab::TR,
    mcp::TR,
    mcp_tools::TR,
];

impl Action {
    pub fn label_key(self) -> String {
        format!("action.{}", self.id())
    }

    pub fn label(self) -> &'static str {
        crate::i18n::tr(&self.label_key())
    }

    pub const fn is_ready(self) -> bool {
        matches!(self.status(), ActionStatus::Ready)
    }

    pub fn from_id(id: &str) -> Option<Action> {
        Action::DECLARED.iter().copied().find(|a| a.id() == id)
    }

    pub fn is_plain_key_shortcut(sc: &KeyboardShortcut) -> bool {
        !sc.modifiers.command && !sc.modifiers.ctrl && !sc.modifiers.alt
    }

    pub fn repeats(self) -> bool {
        REPEATS.iter().any(|list| list.contains(&self))
    }

    pub fn tool(self) -> Option<crate::tool::ToolId> {
        crate::tool::ToolId::ALL.iter().copied().find(|t| t.action() == self)
    }

    pub fn enabled(self, state: &AppState) -> bool {
        self.is_ready() && first_answer!(enabled(state, self)).unwrap_or(true)
    }

    pub fn check(self, state: &AppState) -> Check {
        if let Some(check) = first_answer!(check(state, self)) {
            return check;
        }
        match self.tool() {
            Some(tool) => Check::Radio(state.tool == tool),
            None => Check::None,
        }
    }

    pub fn icon(self) -> Option<&'static str> {
        if let Some(tool) = self.tool() {
            return Some(tool.icon());
        }
        first_answer!(icon(self))
    }
}

#[cfg(test)]
mod tests;
