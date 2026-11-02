use super::*;
use crate::i18n::Lang;

const CORE: &[(&str, &str)] = &[
    ("new_map", "Ctrl+N"),
    ("open_map", "Ctrl+O"),
    ("save_map", "Ctrl+S"),
    ("save_map_as", "Ctrl+Shift+S"),
    ("import_map", ""),
    ("export_selection", ""),
    ("run_map", "Ctrl+F9"),
    ("exit", ""),
    ("undo", "Ctrl+Z"),
    ("redo", "Ctrl+Y"),
    ("cut", "Ctrl+X"),
    ("copy", "Ctrl+C"),
    ("paste", "Ctrl+V"),
    ("paste_special", "Ctrl+Shift+V"),
    ("delete", "Delete"),
    ("duplicate", "Ctrl+D"),
    ("select_all", "Ctrl+A"),
    ("select_none", "Ctrl+Shift+A"),
    ("invert_selection", "Ctrl+I"),
    ("select_mode_groups", ""),
    ("select_mode_objects", ""),
    ("select_mode_solids", ""),
    ("find_entities", "Ctrl+Shift+F"),
    ("replace_textures", "Ctrl+Shift+R"),
    ("properties", "Alt+Enter"),
    ("command_palette", "Ctrl+P"),
    ("settings", "Ctrl+Comma"),
    ("group", "Ctrl+G"),
    ("ungroup", "Ctrl+U"),
    ("tie_to_entity", "Ctrl+T"),
    ("move_to_world", "Ctrl+Shift+W"),
    ("hide_selected", "H"),
    ("hide_unselected", "Ctrl+Shift+H"),
    ("unhide_all", "U"),
    ("carve", "Ctrl+Shift+C"),
    ("make_hollow", "Ctrl+H"),
    ("merge_convex", ""),
    ("snap_to_grid", "Ctrl+B"),
    ("transform_dialog", "Ctrl+M"),
    ("flip_x", ""),
    ("flip_y", ""),
    ("flip_z", ""),
    ("align_left", ""),
    ("align_right", ""),
    ("align_top", ""),
    ("align_bottom", ""),
    ("check_problems", "Alt+P"),
    ("map_properties", ""),
    ("entity_report", ""),
    ("map_info", ""),
    ("go_to_brush", "Ctrl+Shift+G"),
    ("load_pointfile", ""),
    ("unload_pointfile", ""),
    ("grid_smaller", "OpenBracket"),
    ("grid_larger", "CloseBracket"),
    ("toggle_grid", "Shift+G"),
    ("toggle_snap", "Shift+W"),
    ("toggle_texture_lock", "Shift+L"),
    ("toggle_texture_scale_lock", "Shift+K"),
    ("center_2d", "Ctrl+E"),
    ("center_3d", "Ctrl+Shift+E"),
    ("maximize_view", "Shift+Z"),
    ("render_textured", "Alt+1"),
    ("render_flat", "Alt+2"),
    ("render_wireframe", "Alt+3"),
    ("render_textured_wire", "Alt+4"),
    ("render_lit", "Alt+5"),
    ("toggle_models", ""),
    ("toggle_helpers", ""),
    ("toggle_tool_textures", ""),
    ("show_texture_browser", ""),
    ("show_model_browser", ""),
    ("show_entity_properties", ""),
    ("show_visgroups", ""),
    ("show_outliner", ""),
    ("show_console", "Backtick"),
    ("tool_select", "Shift+S"),
    ("tool_camera", "Shift+C"),
    ("tool_entity", "Shift+E"),
    ("tool_block", "Shift+B"),
    ("tool_texture", "Shift+A"),
    ("apply_current_texture", "Shift+T"),
    ("tool_decal", "Shift+D"),
    ("tool_overlay", "Shift+O"),
    ("tool_clip", "Shift+X"),
    ("tool_vertex", "Shift+V"),
    ("tool_displacement", "Shift+P"),
    ("keyboard_shortcuts", "F1"),
    ("open_docs", ""),
    ("report_issue", ""),
    ("about", ""),
];

#[test]
fn ids_unique_and_translated() {
    let mut seen = std::collections::HashSet::new();
    for a in Action::DECLARED {
        assert!(seen.insert(a.id()), "duplicate id {}", a.id());
        assert!(crate::i18n::has_key(&a.label_key()), "missing label {}", a.label_key());
    }
}

#[test]
fn shortcuts_unique() {
    let mut seen = std::collections::HashMap::new();
    for a in Action::DECLARED {
        if let Some(s) = a.default_shortcut() {
            let k = format_shortcut(&s);
            if let Some(prev) = seen.insert(k.clone(), a.id()) {
                panic!("shortcut {k} used by {prev} and {}", a.id());
            }
        }
    }
}

#[test]
fn shortcut_text_round_trip() {
    for a in Action::DECLARED {
        if let Some(s) = a.default_shortcut() {
            assert_eq!(parse_shortcut(&format_shortcut(&s)), Some(s), "{}", a.id());
        }
    }
}

const CTRL_KEY: Modifiers = Modifiers { alt: false, ctrl: true, shift: false, mac_cmd: false, command: true };

fn press(key: Key, physical: Option<Key>, modifiers: Modifiers) -> egui::Event {
    egui::Event::Key { key, physical_key: physical, pressed: true, repeat: false, modifiers }
}

fn triggered(keys: &KeyBindings, events: Vec<egui::Event>) -> (Vec<Action>, Vec<egui::Event>) {
    let mut events = events;
    let actions = keys.take_triggered(&mut events);
    (actions, events)
}

#[test]
fn modifiers_match_exactly() {
    let keys = KeyBindings::default();
    let ctrl_shift = Modifiers { shift: true, ..CTRL_KEY };
    let ctrl_alt = Modifiers { alt: true, ..CTRL_KEY };
    let (a, left) = triggered(&keys, vec![press(Key::Z, Some(Key::Z), ctrl_shift)]);
    assert!(a.is_empty(), "{a:?}");
    assert_eq!(left.len(), 1, "unbound keys stay for the rest of the app");
    let (a, _) = triggered(&keys, vec![press(Key::S, Some(Key::S), ctrl_alt)]);
    assert!(a.is_empty(), "{a:?}");
    let (a, left) = triggered(&keys, vec![press(Key::S, Some(Key::S), ctrl_shift)]);
    assert_eq!(a, vec![Action::SaveMapAs]);
    assert!(left.is_empty());
    let (a, _) = triggered(&keys, vec![press(Key::Z, Some(Key::Z), CTRL_KEY)]);
    assert_eq!(a, vec![Action::Undo]);
    let (a, _) = triggered(&keys, vec![press(Key::H, Some(Key::H), Modifiers::SHIFT)]);
    assert!(a.is_empty(), "{a:?}");
}

#[test]
fn azerty_digits_use_the_physical_key() {
    let keys = KeyBindings::default();
    let (a, _) = triggered(&keys, vec![press(Key::Quote, Some(Key::Num4), Modifiers::ALT)]);
    assert_eq!(a, vec![Action::RenderTexturedWire]);
    let sc = shortcut_from_event(&press(Key::Minus, Some(Key::Num6), Modifiers::ALT)).unwrap();
    assert_eq!(sc, KeyboardShortcut::new(Modifiers::ALT, Key::Num6));
    assert_eq!(format_shortcut(&sc), "Alt+6");
    let (a, _) = triggered(&keys, vec![press(Key::Num1, Some(Key::Num1), Modifiers::ALT)]);
    assert_eq!(a, vec![Action::RenderTextured]);
    let (a, _) = triggered(&keys, vec![press(Key::Z, Some(Key::W), CTRL_KEY)]);
    assert_eq!(a, vec![Action::Undo]);
}

#[test]
fn altgr_symbols_match_plain_bindings() {
    let keys = KeyBindings::default();
    let altgr = Modifiers { alt: true, ..CTRL_KEY };
    let (a, _) = triggered(&keys, vec![press(Key::OpenBracket, Some(Key::Num5), altgr)]);
    assert_eq!(a, vec![Action::GridSmaller]);
    let (a, _) = triggered(&keys, vec![press(Key::CloseBracket, Some(Key::Minus), altgr)]);
    assert_eq!(a, vec![Action::GridLarger]);
}

#[test]
fn clipboard_events_follow_the_bindings() {
    let mut keys = KeyBindings::default();
    let (a, left) = triggered(&keys, vec![egui::Event::Copy, egui::Event::Cut, egui::Event::Paste("x".into())]);
    assert_eq!(a, vec![Action::Copy, Action::Cut, Action::Paste]);
    assert!(left.is_empty());
    keys.set(Action::Copy, None);
    let (a, left) = triggered(&keys, vec![egui::Event::Copy]);
    assert!(a.is_empty());
    assert_eq!(left.len(), 1);
    keys.set(Action::Carve, Some(KeyboardShortcut::new(Modifiers::COMMAND, Key::C)));
    let (a, _) = triggered(&keys, vec![egui::Event::Copy]);
    assert_eq!(a, vec![Action::Carve]);
    keys.set(Action::Copy, Some(KeyboardShortcut::new(Modifiers::NONE, Key::F6)));
    let (a, _) = triggered(&keys, vec![press(Key::F6, Some(Key::F6), Modifiers::NONE)]);
    assert_eq!(a, vec![Action::Copy]);
}

#[test]
fn held_keys_only_repeat_repeatable_actions() {
    let keys = KeyBindings::default();
    let repeat =
        |key, mods| egui::Event::Key { key, physical_key: Some(key), pressed: true, repeat: true, modifiers: mods };
    let (a, left) = triggered(&keys, vec![repeat(Key::G, Modifiers::SHIFT)]);
    assert!(a.is_empty(), "toggles do not flicker: {a:?}");
    assert!(left.is_empty(), "the repeat of a bound key is swallowed");
    let (a, _) = triggered(&keys, vec![repeat(Key::Z, CTRL_KEY)]);
    assert_eq!(a, vec![Action::Undo]);
}

#[test]
fn poll_ignores_everything_while_typing() {
    let keys = KeyBindings::default();
    let input = egui::RawInput {
        events: vec![press(Key::A, Some(Key::A), CTRL_KEY), press(Key::Delete, Some(Key::Delete), Modifiers::NONE)],
        ..Default::default()
    };
    let mut typing = Vec::new();
    let mut free = Vec::new();
    let ctx = egui::Context::default();
    ctx.run_ui(input.clone(), |ui| typing = keys.poll(ui.ctx(), true)).textures_delta.clear();
    let ctx = egui::Context::default();
    ctx.run_ui(input, |ui| free = keys.poll(ui.ctx(), false)).textures_delta.clear();
    assert!(typing.is_empty(), "{typing:?}");
    assert_eq!(free, vec![Action::SelectAll, Action::Delete]);
}

#[test]
fn labels_are_readable_and_localized() {
    let raw_names = ["OpenBracket", "CloseBracket", "Comma", "Backtick", "Delete", "Shift", "Escape", "Quote"];
    for a in Action::DECLARED {
        let Some(sc) = a.default_shortcut() else { continue };
        for lang in Lang::ALL {
            let label = shortcut_label_in(&sc, lang);
            for raw in raw_names {
                if raw == "Shift" && lang == Lang::En {
                    continue;
                }
                assert!(!label.contains(raw), "{}: {label}", a.id());
            }
        }
    }
    let grid = Action::GridSmaller.default_shortcut().unwrap();
    assert_eq!(shortcut_label_in(&grid, Lang::En), "[");
    let save_as = Action::SaveMapAs.default_shortcut().unwrap();
    assert_eq!(shortcut_label_in(&save_as, Lang::Fr), "Ctrl+Maj+S");
    let delete = Action::Delete.default_shortcut().unwrap();
    assert_eq!(shortcut_label_in(&delete, Lang::Fr), "Suppr");
    assert_eq!(key_label_in(Key::Minus, Lang::En), "-");
    assert_eq!(key_label_in(Key::Num4, Lang::Fr), "4");
}

#[test]
fn core_actions_keep_their_ids_shortcuts_and_order() {
    let core: Vec<(&str, String)> = Action::DECLARED
        .iter()
        .take(CORE.len())
        .map(|a| (a.id(), a.default_shortcut().map(|s| format_shortcut(&s)).unwrap_or_default()))
        .collect();
    let expected: Vec<(&str, String)> = CORE.iter().map(|&(id, sc)| (id, sc.to_owned())).collect();
    assert_eq!(core, expected);
    assert!(Action::DECLARED[..CORE.len()].iter().all(|a| a.is_ready()));
}

#[test]
fn all_lists_the_ready_actions_in_registry_order() {
    let ready: Vec<Action> = Action::DECLARED.iter().copied().filter(|a| a.is_ready()).collect();
    assert_eq!(Action::ALL, ready.as_slice());
    assert!(Action::DECLARED.len() >= Action::ALL.len());
    if let Some(planned) = Action::DECLARED.iter().copied().find(|a| !a.is_ready()) {
        assert_eq!(Action::from_id(planned.id()), Some(planned));
        assert!(!Action::ALL.contains(&planned));
    }
}

#[test]
fn planned_shortcuts_are_inactive() {
    let keys = KeyBindings::default();
    let planned = Action::DECLARED.iter().copied().filter(|a| !a.is_ready()).find_map(|a| {
        let sc = a.default_shortcut()?;
        let shared = Action::ALL.iter().any(|r| r.default_shortcut() == Some(sc));
        (!shared).then_some((a, sc))
    });
    let Some((action, sc)) = planned else { return };
    let (a, left) = triggered(&keys, vec![press(sc.logical_key, Some(sc.logical_key), sc.modifiers)]);
    assert!(a.is_empty(), "{a:?}");
    assert_eq!(left.len(), 1, "the key stays for the rest of the app");
    assert_eq!(keys.shortcut(action), action.default_shortcut());
}

#[test]
fn groups_belong_to_one_menu() {
    let mut menus = std::collections::HashMap::new();
    for &a in Action::DECLARED.iter().filter(|a| a.group() != MenuGroup::Hidden) {
        if let Some(prev) = menus.insert(a.group(), a.category())
            && prev != a.category()
        {
            panic!("group {:?} is used in {prev:?} and {:?} ({})", a.group(), a.category(), a.id());
        }
    }
    for &a in Action::DECLARED {
        if let Some(menu) = a.group().category() {
            assert_eq!(menu, a.category(), "{} is in the {menu:?} group {:?}", a.id(), a.group());
        }
    }
    for &a in Action::DECLARED {
        if let Some(submenu) = a.group().info().and_then(|g| g.submenu) {
            assert!(crate::i18n::has_key(submenu), "missing submenu title {submenu}");
        }
    }
}

#[test]
fn tool_switches_follow_the_tools() {
    for tool in crate::tool::ToolId::ALL {
        assert_eq!(tool.action().tool(), Some(tool));
        assert_eq!(tool.action().group(), MenuGroup::ToolsSwitch, "{tool:?}");
    }
    assert_eq!(Action::ApplyCurrentTexture.tool(), None);
    assert_eq!(Action::ToolBlock.icon(), Some(crate::tool::ToolId::Block.icon()));
}

#[test]
fn tools_follow_the_hammer_order_and_keys() {
    use crate::tool::ToolId as T;
    assert_eq!(
        T::ALL,
        [
            T::Select,
            T::Camera,
            T::Entity,
            T::Block,
            T::Texture,
            T::Decal,
            T::Overlay,
            T::Clip,
            T::Extrude,
            T::Bevel,
            T::Vertex,
            T::Path,
            T::Polygon,
            T::Curve,
            T::Sprinkle,
            T::Displacement,
            T::Cordon,
        ]
    );
    let hammer_keys = [
        (T::Select, "Shift+S"),
        (T::Camera, "Shift+C"),
        (T::Entity, "Shift+E"),
        (T::Block, "Shift+B"),
        (T::Texture, "Shift+A"),
        (T::Decal, "Shift+D"),
        (T::Overlay, "Shift+O"),
        (T::Clip, "Shift+X"),
        (T::Vertex, "Shift+V"),
    ];
    assert_eq!(T::Path.action().default_shortcut(), None);
    for preset in KeymapPreset::ALL {
        let keys = KeyBindings::from_preset(preset, &Default::default());
        for (tool, key) in hammer_keys {
            let sc = keys.shortcut(tool.action()).map(|s| format_shortcut(&s));
            assert_eq!(sc.as_deref(), Some(key), "{preset:?} {tool:?}");
        }
        let sc = keys.shortcut(Action::ApplyCurrentTexture).map(|s| format_shortcut(&s));
        assert_eq!(sc.as_deref(), Some("Shift+T"), "{preset:?}");
    }
    assert_eq!(Action::ApplyCurrentTexture.group(), MenuGroup::ToolsSwitch);
}

#[test]
fn menus_are_hammer_menus() {
    let labels: Vec<&str> = Category::ALL.iter().map(|c| crate::i18n::tr_in(c.label_key(), Lang::En)).collect();
    assert_eq!(labels, ["File", "Edit", "Map", "View", "Tools", "Window", "Help"]);
    assert_eq!(Action::ToggleSnap.category(), Category::Map, "Map > Snap to grid");
    assert_eq!(Action::ToggleGrid.category(), Category::Map, "Map > Show grid");
    assert_eq!(Action::Carve.category(), Category::Tools, "Tools > Carve");
    assert_eq!(Action::Group.category(), Category::Tools, "Tools > Group");
    assert_eq!(Action::ToggleTextureLock.category(), Category::Tools, "Tools > Texture lock");
    assert_eq!(Action::ReplaceTextures.category(), Category::Tools, "Tools > Replace textures");
    assert_eq!(Action::Settings.category(), Category::Tools, "Tools > Options");
    assert_eq!(Action::HideSelected.category(), Category::View, "View > Hide selected");
    assert_eq!(Action::ShowConsole.category(), Category::Window, "Window > Messages");
}

#[test]
fn repeats_and_states() {
    assert!(Action::Undo.repeats() && Action::GridLarger.repeats());
    assert!(!Action::ToggleGrid.repeats());
    let mut state = crate::testing::test_state();
    assert!(!Action::Copy.enabled(&state));
    assert!(Action::SaveMap.enabled(&state));
    assert!(!Action::MoveToFloor.enabled(&state), "planned actions are never enabled");
    state.doc.select_all();
    assert!(Action::Copy.enabled(&state));
    assert_eq!(Action::ToolSelect.check(&state), Check::Radio(true));
    assert_eq!(Action::SaveMap.check(&state), Check::None);
    state.settings.edit.snap_to_grid = false;
    assert_eq!(Action::ToggleSnap.check(&state), Check::Toggle(false));
}

#[test]
fn tables_are_clean() {
    let mut seen = std::collections::HashSet::new();
    for table in TABLES.iter().chain([&crate::strings::TR]) {
        let problems = crate::i18n::lint_table(table);
        assert!(problems.is_empty(), "{problems:#?}");
        for (k, _, _) in table.iter() {
            assert!(seen.insert(*k), "key {k} is in two tables");
        }
    }
}

#[test]
fn take_action_consumes_only_that_action() {
    let keys = KeyBindings::default();
    let ctrl = egui::Modifiers { ctrl: true, command: true, ..egui::Modifiers::NONE };
    let press = |key, modifiers, repeat| egui::Event::Key { key, physical_key: None, pressed: true, repeat, modifiers };
    let mut events = vec![press(egui::Key::A, ctrl, false), press(egui::Key::B, ctrl, false)];
    assert!(keys.take_action(&mut events, Action::SnapToGrid));
    assert_eq!(events.len(), 1, "only Ctrl+B was taken");
    assert!(!keys.take_action(&mut events, Action::SnapToGrid));
    let mut repeated = vec![press(egui::Key::B, ctrl, true)];
    assert!(!keys.take_action(&mut repeated, Action::SnapToGrid), "a held key does not start it again");
    assert_eq!(keys.take_triggered(&mut events), vec![Action::SelectAll]);
}
