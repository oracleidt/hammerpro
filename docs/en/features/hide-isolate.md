[Documentation](../README.md) > Features > Hide and isolate | [Français](../../fr/features/hide-isolate.md) | [Русский](../../ru/features/hide-isolate.md)

# Hide and isolate

Hiding takes objects out of the way without deleting them: Hammer's quick hide (`H`, `Ctrl+Shift+H`, `U`), the isolate button of the tool palette, and display switches for editor-only objects and tool textures.

## Opening it

| Action | Menu | Default shortcut |
| --- | --- | --- |
| **Hide selected** | **View > Hide selected** | `H` |
| **Hide unselected** | **View > Hide unselected** | `Ctrl+Shift+H` |
| **Unhide all** | **View > Unhide all** | `U` |
| **Make a visgroup of hidden objects** | **View > Make a visgroup of hidden objects** | none |
| **Show editor objects** | **View > Display options > Show editor objects** | none (`Alt+O` with the Hammer++ keymap preset) |
| **Show tool textures** | **View > Show tool textures** | none (`Ctrl+Shift+F2` with the Hammer++ keymap preset) |
| Hidden tool textures | **View > Hidden tool textures** (submenu) | none |

The toolbar has the three hide buttons, and the tool palette ends with the isolate eye.

## Hiding objects

- **Hide selected** (`H`) hides the selected objects. They are deselected.
- **Hide unselected** (`Ctrl+Shift+H`) hides every visible object that is not selected.
- **Unhide all** (`U`) shows every object hidden this way again.

Each is one undo step. Hidden objects:

- are not drawn and cannot be picked or selected, like in Hammer: a box select, **Select all**, **Select members** of a visgroup or a click in the [Outliner](outliner.md) skips them, so Delete or a move never touches what you cannot see;
- stay hidden when you save (Hammer's `hidden` blocks of the VMF);
- are left out of the compile unless **Compile hidden objects** is checked in the [Compile and run window](../compiler/compile-window.md) or on the **Compile** page of the settings. The compile log warns when hidden lights are left out.

The **Hidden objects** row of the auto visgroups (Visgroups tab) counts them and has **Unhide all** and **To visgroup** buttons. **To visgroup** (same as **View > Make a visgroup of hidden objects**) moves them into a hidden visgroup `_FromQuickHide(N)`, so you can show them again by name (see [Visgroups](visgroups.md)).

The [Outliner](outliner.md) has an eye on every row to hide or show one object, a group, a class of entities or the whole world.

## Isolate

The eye at the end of the tool palette isolates the selection:

- with objects selected and nothing hidden, it runs **Hide unselected** (tooltip "Isolate the selection (hide everything else)");
- while objects are hidden, it is lit and runs **Unhide all** (tooltip "Show everything again");
- with nothing selected and nothing hidden, it is disabled ("Select something to isolate it").

## Editor objects

**Show editor objects** (on by default, `helpers.show_editor_objects`) shows the entity icons and helper geometry that only exist in the editor, like Hammer++'s "editor objects". Turn it off to see the map closer to the game. World sprites (`env_sprite`) stay visible.

**View > Display options > Show point entities** shows or hides every point entity at once.

## Tool textures

- **Show tool textures** (View menu and toolbar, on by default, `view.show_tool_textures`) shows or hides every face with a tool texture in the 3D view.
- **View > Hidden tool textures** hides groups of tool textures in the 3D view while tool textures are shown: **Nodraw**, **Clips**, **Triggers**, **Hints and skips**. The same four checkboxes are on the **Rendering** page of the settings (**Tool textures hidden in 3D**), and **Show nodraw faces** (View > Display options and toolbar) toggles the **Nodraw** one.

| Setting (`[render]`) | What it hides | Default |
| --- | --- | --- |
| `hide_nodraw` | Nodraw faces | Off |
| `hide_clips` | Clip brushes | Off |
| `hide_triggers` | Triggers | Off |
| `hide_hints` | Hints and skips | Off |

A face hidden this way is not clicked either in the 3D view: clicks, face selection and placement go through it to what is drawn behind. The 2D views still show and pick these brushes. The same goes for brushes hidden by **Show displacements** or **Displacements only**.

## Tips

- Cordons dim and lock everything outside a box without hiding it, which is often better for working on one room (see [Cordons](cordons.md)).
- The [player view](player-view.md) hides tool textures like the game; press `N` there to show them.
- MCP tools: `hide_objects` (with `unselected` for Hide unselected), `isolate_objects`, `hidden_to_visgroup`, `get_visibility` (see the [MCP tool reference](../../MCP.md)).

## FAQ

**My compile is missing a part of the map.** It was hidden. Press `U` to show everything, or check **Compile hidden objects**.

**Why can I not select a hidden object from the outliner?** Hidden objects are never selected. Show it with its eye first.

**Does hiding change the map?** Hiding is an undo step and is saved with the map, but the objects are untouched.

## Related pages

- [Visgroups](visgroups.md)
- [Cordons](cordons.md)
- [Outliner](outliner.md)
- [Render modes and display options](render-modes-display.md)
- [Keyboard shortcuts](keyboard-shortcuts.md)

---
[Back to the documentation index](../README.md)
