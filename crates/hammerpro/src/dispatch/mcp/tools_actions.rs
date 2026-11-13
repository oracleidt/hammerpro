use hp_app::actions::{Category, Check};
use hp_app::i18n::{Lang, tr_in};
use hp_app::{Action, AppState};
use hp_mcp::protocol::ToolResult;
use hp_mcp::tools::{ToolKind, ToolSpec};
use serde_json::{Value, json};

use super::automation::UiOp;
use super::schema::{boolean, integer, object, one_of, string};
use super::tools::{Domain, Outcome, R, args};

pub(crate) const DOMAIN: Domain = Domain { category: "actions", specs: SPECS, run };

pub(crate) const NEEDS_CONFIRM: &[(&str, &str)] = &[
    ("exit", "closes Hammer Pro"),
    ("toggle_mcp_server", "switches this MCP server off"),
    ("mcp_install", "changes the configuration of other apps and runs their programs"),
    ("publish_workshop", "publishes to the Steam Workshop"),
    ("launch_game", "starts the game"),
    ("reload_in_game", "talks to the running game"),
    ("import_from_game", "talks to the running game"),
    ("compile_now", "runs the compiler and may start the game (use compile_map)"),
    ("batch_compile", "runs the compiler on several maps"),
    ("edit_instance", "may start another Hammer Pro window"),
    ("install_update", "downloads an update, closes Hammer Pro and replaces the program (use apply_update)"),
    ("open_docs", "opens the web browser"),
    ("report_issue", "opens the web browser"),
    ("open_map", "shows a file dialog the user must answer (use the open_map tool)"),
    ("save_map_as", "shows a file dialog the user must answer (use save_map with a name)"),
    ("import_map", "shows a file dialog the user must answer"),
    ("export_selection", "shows a file dialog the user must answer"),
    ("export_copy", "shows a file dialog the user must answer"),
    ("export_obj", "shows a file dialog the user must answer"),
    ("export_again", "writes a file outside the map folder"),
    ("open_archive", "shows a file dialog the user must answer"),
    ("open_bsp", "shows a file dialog the user must answer"),
    ("compare_maps", "shows a file dialog the user must answer"),
    ("load_pointfile", "shows a file dialog the user must answer"),
    ("load_portal_file", "shows a file dialog the user must answer"),
    ("load_nav_mesh", "shows a file dialog the user must answer"),
    ("send_to_blender", "starts Blender (use the send_to_blender tool)"),
    ("install_blender_addon", "writes into Blender's folders (use blender_setup with install_addon)"),
    ("live_link_start", "writes the live link addon into the game folder (use live_link_start)"),
    (
        "live_link_remove_addon",
        "deletes the live link addon from the game folder (use live_link_stop with remove_addon)",
    ),
    ("live_link_teleport", "moves the player of the running game (use live_link_teleport)"),
    ("live_link_teleport_back", "moves the player of the running game (use live_link_teleport with back)"),
];

const STARTS_GAME: &[&str] = &["launch_game", "reload_in_game", "compile_now", "batch_compile"];

fn category_name(c: Category) -> &'static str {
    match c {
        Category::File => "file",
        Category::Edit => "edit",
        Category::Map => "map",
        Category::View => "view",
        Category::Tools => "tools",
        Category::Window => "window",
        Category::Help => "help",
        Category::Other => "other",
    }
}

const CATEGORIES: &[&str] = &["file", "edit", "map", "view", "tools", "window", "help", "other"];

const SPECS: &[ToolSpec] = &[
    ToolSpec {
        name: "list_actions",
        title: "List the editor actions",
        description: "Every command of Hammer Pro (menus, toolbars, shortcuts, command palette): id, English \
            label, menu, menu_path (where it sits in the menu bar, e.g. \"File > Import\"), shortcut, whether it is enabled now and checked (toggles), and whether run_action needs \
            confirm. Filter by menu or text.",
        kind: ToolKind::Read,
        schema: || {
            object(
                json!({
                    "category": one_of("Only this menu.", CATEGORIES),
                    "query": string("Only ids or labels containing this text (case insensitive)."),
                    "enabled_only": boolean("Only the actions that can run now."),
                    "limit": integer("Most actions returned (default 500).", 1, 1000),
                    "offset": integer("Actions to skip.", 0, 100_000),
                }),
                &[],
            )
        },
    },
    ToolSpec {
        name: "run_action",
        title: "Run an editor action",
        description: "Run a command of Hammer Pro by id (from list_actions), like a click in its menu: tools, \
            selection, groups, hiding, carve, views, render modes, panels, dialogs... One undo step. Actions that \
            close the app, publish, start the game or a program, or show a file dialog need confirm: true, only \
            when the user asked for exactly that.",
        kind: ToolKind::Destructive,
        schema: || {
            object(
                json!({
                    "action": string("Action id, e.g. \"select_all\", \"tool_clip\", \"render_wireframe\", \"maximize_view\"."),
                    "confirm": boolean("Needed by the actions list_actions marks needs_confirm; only when the user explicitly asked."),
                }),
                &["action"],
            )
        },
    },
];

fn run(state: &mut AppState, name: &str, v: &Value) -> Option<Outcome> {
    let result = match name {
        "list_actions" => list_actions(state, v).map(Outcome::Done),
        "run_action" => run_action(state, v),
        _ => return None,
    };
    Some(result.unwrap_or_else(|e| Outcome::Done(ToolResult::error(format!("{name}: {e}")))))
}

pub(crate) fn confirm_reason(state: &AppState, action: Action) -> Option<&'static str> {
    if let Some((_, why)) = NEEDS_CONFIRM.iter().find(|(id, _)| *id == action.id()) {
        return Some(why);
    }
    (matches!(action, Action::SaveMap | Action::SaveAsNewVersion) && state.doc.path().is_none())
        .then_some("shows a file dialog the user must answer (the map has no file yet: use save_map with a name)")
}

fn check_json(c: Check) -> Value {
    match c {
        Check::None => Value::Null,
        Check::Toggle(on) | Check::Radio(on) => Value::Bool(on),
    }
}

fn list_actions(state: &AppState, v: &Value) -> R<ToolResult> {
    let a = args(v, &["category", "query", "enabled_only", "limit", "offset"])?;
    let category = a.opt_enum("category", CATEGORIES)?;
    let query = a.opt_str("query", 128)?.map(str::to_lowercase);
    let enabled_only = a.opt_bool("enabled_only")?.unwrap_or(false);
    let limit = a.opt_u64("limit", 1, 1000)?.unwrap_or(500) as usize;
    let offset = a.opt_u64("offset", 0, 100_000)?.unwrap_or(0) as usize;
    let shown_lang = hp_app::i18n::lang();
    let matching: Vec<Value> = Action::ALL
        .iter()
        .copied()
        .filter(|act| category.as_deref().is_none_or(|c| category_name(act.category()) == c))
        .filter(|act| !enabled_only || act.enabled(state))
        .filter(|act| {
            query.as_deref().is_none_or(|q| {
                act.id().contains(q)
                    || tr_in(&act.label_key(), Lang::En).to_lowercase().contains(q)
                    || act.label().to_lowercase().contains(q)
            })
        })
        .map(|act| {
            let mut entry = json!({
                "id": act.id(),
                "label": tr_in(&act.label_key(), Lang::En),
                "menu": category_name(act.category()),
                "menu_path": hp_ui_shell::menu::menu_path_text(act, Some(Lang::En)),
                "shortcut": state.keys.shortcut(act).map(|sc| hp_app::actions::format_shortcut(&sc)),
                "enabled": act.enabled(state),
                "checked": check_json(act.check(state)),
            });
            if shown_lang != Lang::En {
                entry["label_shown"] = json!(act.label());
            }
            if let Some(why) = confirm_reason(state, act) {
                entry["needs_confirm"] = json!(why);
            }
            entry
        })
        .collect();
    let total = matching.len();
    let actions: Vec<Value> = matching.into_iter().skip(offset).take(limit).collect();
    Ok(ToolResult::json(json!({ "total": total, "offset": offset, "actions": actions })))
}

fn run_action(state: &AppState, v: &Value) -> R<Outcome> {
    let a = args(v, &["action", "confirm"])?;
    let id = a.str("action", 64)?.trim().to_ascii_lowercase();
    let confirm = a.opt_bool("confirm")?.unwrap_or(false);
    let Some(action) = Action::from_id(&id).filter(|a| a.is_ready()) else {
        let close = super::tools_help::suggest(&id, Action::ALL.iter().map(|a| a.id()));
        return Err(if close.is_empty() {
            format!("unknown action \"{id}\" (list_actions lists them)")
        } else {
            format!("unknown action \"{id}\"; did you mean: {}?", close.join(", "))
        });
    };
    if let Some(why) = confirm_reason(state, action)
        && !confirm
    {
        return Err(format!("{id} {why}: run it with confirm: true only if the user explicitly asked for it"));
    }
    if STARTS_GAME.contains(&id.as_str()) && hp_compile::sandbox::is_sandboxed() {
        return Err(format!("{id}: {}", hp_compile::sandbox::REFUSED));
    }
    if !action.enabled(state) {
        return Err(format!("{id} is not available now (greyed out in the menus: nothing selected, no map...)"));
    }
    Ok(Outcome::Ui(UiOp::RunAction(action)))
}

#[cfg(test)]
mod tests {
    use super::*;

    const RISKY_WORDS: &[&str] = &[
        "exit",
        "quit",
        "launch",
        "publish",
        "workshop",
        "game",
        "install",
        "open",
        "import",
        "export",
        "load",
        "compile",
        "docs",
        "report",
        "archive",
        "bsp",
        "steam",
        "web",
        "url",
        "delete_file",
        "folder",
        "run",
    ];
    const REVIEWED_SAFE: &[&str] = &[
        "unload_pointfile",
        "unload_portal_file",
        "reload_content",
        "cancel_compile",
        "entity_report",
        "run_map",
        "use_sky_camera_in_game",
        "import_model",
        "import_models_batch",
        "import_heightmap",
        "export_heightmap",
        "import_blend_masks",
        "export_blend_masks",
        "bsp_sniffer",
        "live_link_camera_to_game",
        "live_link_camera_from_game",
    ];

    #[test]
    fn risky_looking_actions_are_reviewed() {
        let mut missing = Vec::new();
        for a in Action::ALL {
            let id = a.id();
            if RISKY_WORDS.iter().any(|w| id.contains(w))
                && !NEEDS_CONFIRM.iter().any(|(c, _)| *c == id)
                && !REVIEWED_SAFE.contains(&id)
            {
                missing.push(id);
            }
        }
        assert!(missing.is_empty(), "decide whether run_action needs confirm for: {missing:?}");
        for id in REVIEWED_SAFE {
            assert!(Action::from_id(id).is_some_and(Action::is_ready), "{id}");
        }
    }

    fn call(state: &mut AppState, name: &str, v: Value) -> Outcome {
        run(state, name, &v).expect("a tool of this domain")
    }

    fn err(state: &mut AppState, name: &str, v: Value) -> String {
        match call(state, name, v) {
            Outcome::Done(r) if r.is_error => r.first_text().to_owned(),
            other => panic!("{name} should fail: {other:?}"),
        }
    }

    #[test]
    fn every_ready_action_is_listed() {
        let _lang = hp_app::testing::lang_lock();
        let mut state = hp_app::testing::test_state();
        let Outcome::Done(r) = call(&mut state, "list_actions", json!({ "limit": 1000 })) else { panic!() };
        let v = r.structured.unwrap();
        assert_eq!(v["total"].as_u64().unwrap() as usize, Action::ALL.len());
        let ids: Vec<&str> = v["actions"].as_array().unwrap().iter().map(|a| a["id"].as_str().unwrap()).collect();
        assert_eq!(ids.len(), Action::ALL.len());
        let save = v["actions"].as_array().unwrap().iter().find(|a| a["id"] == "save_map").unwrap();
        assert_eq!(save["label"], tr_in("action.save_map", Lang::En));
        assert_eq!(save["menu"], "file");
        assert_eq!(save["shortcut"], "Ctrl+S");
        assert_eq!(save["menu_path"], "File");
        let obj = v["actions"].as_array().unwrap().iter().find(|a| a["id"] == "export_obj").unwrap();
        assert_eq!(obj["menu_path"], "File > Export");
        let flip = v["actions"].as_array().unwrap().iter().find(|a| a["id"] == "flip_x").unwrap();
        assert_eq!(flip["menu_path"], "Tools > Arrange");
        let exit = v["actions"].as_array().unwrap().iter().find(|a| a["id"] == "exit").unwrap();
        assert!(exit["needs_confirm"].is_string());
        for (id, _) in NEEDS_CONFIRM {
            assert!(Action::from_id(id).is_some_and(Action::is_ready), "{id}");
        }
        for id in STARTS_GAME {
            assert!(NEEDS_CONFIRM.iter().any(|(c, _)| c == id), "{id}");
        }

        let Outcome::Done(r) = call(&mut state, "list_actions", json!({ "category": "view", "query": "WIREFRAME" }))
        else {
            panic!()
        };
        let v = r.structured.unwrap();
        assert!(v["actions"].as_array().unwrap().iter().all(|a| a["menu"] == "view"));
        assert!(v["actions"].as_array().unwrap().iter().any(|a| a["id"] == "render_wireframe"));
        let Outcome::Done(r) =
            call(&mut state, "list_actions", json!({ "enabled_only": true, "limit": 1, "offset": 2 }))
        else {
            panic!()
        };
        assert_eq!(r.structured.unwrap()["actions"].as_array().unwrap().len(), 1);
        assert!(err(&mut state, "list_actions", json!({ "category": "nope" })).contains("category must be"));
        assert!(err(&mut state, "list_actions", json!({ "limit": 0 })).contains("limit"));

        hp_app::i18n::set_lang(Lang::Fr);
        let Outcome::Done(r) = call(&mut state, "list_actions", json!({ "query": "save_map_as" })) else { panic!() };
        let v = r.structured.unwrap();
        assert_eq!(v["actions"][0]["label_shown"], tr_in("action.save_map_as", Lang::Fr));
        hp_app::i18n::set_lang(Lang::En);
    }

    #[test]
    fn run_action_checks() {
        let mut state = hp_app::testing::test_state();
        assert!(matches!(
            call(&mut state, "run_action", json!({ "action": "select_all" })),
            Outcome::Ui(UiOp::RunAction(Action::SelectAll))
        ));
        assert!(matches!(
            call(&mut state, "run_action", json!({ "action": " Tool_Clip " })),
            Outcome::Ui(UiOp::RunAction(Action::ToolClip))
        ));
        let e = err(&mut state, "run_action", json!({ "action": "exit" }));
        assert!(e.contains("closes Hammer Pro") && e.contains("confirm"), "{e}");
        assert!(matches!(
            call(&mut state, "run_action", json!({ "action": "exit", "confirm": true })),
            Outcome::Ui(UiOp::RunAction(Action::Exit))
        ));
        assert!(err(&mut state, "run_action", json!({ "action": "publish_workshop" })).contains("publishes"));
        assert!(err(&mut state, "run_action", json!({ "action": "select_everything" })).contains("did you mean"));
        assert!(err(&mut state, "run_action", json!({ "action": "zzzz" })).contains("unknown action"));
        assert!(err(&mut state, "run_action", json!({})).contains("action is required"));
        assert!(err(&mut state, "run_action", json!({ "action": "undo", "confirm": "yes" })).contains("true or false"));
        assert!(state.doc.path().is_none());
        assert!(err(&mut state, "run_action", json!({ "action": "save_map" })).contains("file dialog"));
        if let Some(planned) = Action::DECLARED.iter().find(|a| !a.is_ready()) {
            assert!(err(&mut state, "run_action", json!({ "action": planned.id() })).contains("unknown action"));
        }
        assert!(!Action::Undo.enabled(&state) || state.doc.can_undo());
    }
}
