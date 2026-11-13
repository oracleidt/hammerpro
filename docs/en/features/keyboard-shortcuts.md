[Documentation](../README.md) > Features > Keyboard shortcuts | [Français](../../fr/features/keyboard-shortcuts.md) | [Русский](../../ru/features/keyboard-shortcuts.md)

# Keyboard shortcuts

Every command of Hammer Pro can have a keyboard shortcut. The defaults come from a shortcut set (Hammer Pro, Hammer or Hammer++, so your fingers keep their habits), and you can change any shortcut in **Settings > Keyboard**. `F1` shows them all.

The full table of actions with their default shortcuts and the preset differences is in [Actions and shortcuts](../reference/actions-shortcuts.md).

## The shortcut reference (F1)

**Help > Keyboard shortcuts** (`F1`) opens a window listing every action by menu (File, Edit, Map, View, Tools, Window, Help) with its shortcut, plus extra shortcuts of the preset (shown as *also ...*). Actions without a shortcut show `-`.

- **Search actions or shortcuts** filters the list, by action name (in English or in the interface language), by action id, or by shortcut (`ctrl+shift+s` finds Save as; the French key names work too).
- The end of the list shows **Viewport controls** for your camera scheme (mouse buttons, fly keys, wheel, grid keys, clicks, stretch handles). See [Camera and navigation](camera-navigation.md).
- **Customize...** opens **Settings > Keyboard**.

## Settings > Keyboard

Open it with **Tools > Options...** (`Ctrl+,`), page **Keyboard**, or with **Customize...** in the F1 window.

| Control | What it does |
| --- | --- |
| **Shortcut set** | Chooses where the default shortcuts come from: **Hammer Pro**, **Hammer** or **Hammer++**. A confirmation follows (*Use the X shortcuts? The shortcuts you changed stay.*) with **Use these shortcuts** and **Cancel**. |
| **Search actions or shortcuts** | Filters the actions, like in the F1 window. |
| **Reset all shortcuts** | Restores every default shortcut of the current set (your changes are dropped). |
| Conflict line | *Shortcuts used by several actions: N*, shown in orange when some exist. |

Below, the actions are grouped by menu. Each row has:

- the action name (bold when you changed its shortcut; the extra shortcuts of the preset after it; hover it to see the action id);
- the shortcut button: click it, then press the new combination (**Press a key combination...**); `Esc` cancels and keeps the old one. A shortcut used by another action shows in red, its tooltip says *Also used by: ...*;
- **Remove the shortcut** (cross): the action has no shortcut any more;
- **Restore the default shortcut** (arrow): back to the default of the current set.

Changes apply at once and are saved automatically.

### Recording rules

- Holding Ctrl, Shift or Alt shows the modifiers (`Ctrl+Shift+...`); the shortcut is recorded only with the key that completes it.
- `Esc`, `Tab` and `Enter` cannot be recorded alone (they need a modifier).
- Digit keys are recorded by position, so `Alt+4` works on an AZERTY keyboard, where that key types an apostrophe.
- `Ctrl+C`, `Ctrl+X` and `Ctrl+V` can be recorded like any other shortcut.
- Pressing a key that is already a shortcut while recording does not run its action.

## Shortcut sets

| Set | For whom | Defaults |
| --- | --- | --- |
| **Hammer Pro** | Default. | The Hammer Pro shortcuts, plus two extra Hammer keys that collide with nothing: `Shift+Q` also clears the selection, `Shift+R` also shows or hides the grid. |
| **Hammer** | Valve Hammer users (Garry's Mod and Source SDK builds). | The Hammer Pro shortcuts with the differences below. |
| **Hammer++** | Hammer++ users. | The Hammer keys plus the Hammer++ ones below. |

Differences of the **Hammer** set from Hammer Pro:

| Action | Hammer Pro | Hammer |
| --- | --- | --- |
| Show grid | `Shift+G` | `Shift+R` |
| Select all | `Ctrl+A` | `Ctrl+Shift+A` |
| Clear selection | `Ctrl+Shift+A` | `Shift+Q` |
| Reset view splits | none | `Ctrl+A` |
| Flip vertically | none | `Ctrl+I` |
| Invert selection | `Ctrl+I` | `Ctrl+Shift+I` |
| Find and replace in entities... | `Ctrl+Alt+R` | `Ctrl+Shift+R` |
| Find and replace textures... | `Ctrl+Shift+R` | none |
| Compile and run... | `Ctrl+F9` | `F9` |
| Compile now | `F9` | `Ctrl+F9` |

The **Hammer++** set adds:

| Action | Hammer++ |
| --- | --- |
| Model browser | `Shift+F10` |
| Particle browser | `Shift+F11` |
| Sound browser | `Shift+F12` |
| Show tool textures | `Ctrl+Shift+F2` |
| Show editor objects | `Alt+O` |

The set does not touch the shortcuts you changed yourself: they stay on top of any set. The camera controls (mouse and fly keys) are chosen separately, with **Camera controls** in **Settings > Viewports** (see [Camera and navigation](camera-navigation.md)). The first run wizard sets both from the editor you come from (reopen it with **Help > Setup assistant...**).

### I, E and the digit keys: inset, extrude and the pick element

`I` starts the interactive inset and `E` the interactive extrude on the selected faces, or on the face under the mouse when none is selected, while a 3D view has the mouse (see [Extrude tool](extrude-tool.md#interactive-inset-i)). `E` is the action `extrude_interactive`; `Alt+Shift+E` (`extrude_along_normal`) starts it along each face normal, and while it runs `Tab` changes the direction (each normal, average, axis), `X`, `Y`, `Z` pick an axis and `G` switches between a new brush and growing the brush; the selection tool keeps `E` for the scale of the selected objects, except while faces are selected. The digit keys `1`, `2`, `3` and `4` pick vertices, edges, faces or objects with the selection tool ([Pick](selection-tool.md#selecting-faces-edges-or-vertices-pick)). While the inset or extrude is running: `Ctrl` forces the grid, `Alt` frees the inset or switches the extrude to the alternate grid, `Enter` or a click applies, `Escape` or a right click cancels.

### Ctrl+B: snap to grid and bevel

`Ctrl+B` is **Snap selection to grid** in every set. It also bevels: in the [bevel tool](bevel-tool.md), and in the Selection and Vertex tools when the cursor is on a brush edge in the 3D view, it starts bevelling that edge. Anywhere else it snaps to the grid as before. Rebinding **Snap selection to grid** moves both.

## Where shortcuts are stored

In settings.toml:

```toml
[workflow]
keymap_preset = "Hammer"        # "HammerPro", "Hammer" or "HammerPlusPlus"

[keybindings]
save_map = "Ctrl+S"             # action id = shortcut
show_console = ""               # empty: no shortcut
```

Only the shortcuts you changed are written in `[keybindings]`. The action ids are listed in [Actions and shortcuts](../reference/actions-shortcuts.md).

## When shortcuts do not fire

Shortcuts of the map never run while you type in a text field, while a menu, popup or modal dialog is open, while the 3D camera flies with the keyboard, or while a shortcut is being recorded. After you move the focus to a button or a list with `Tab`, plain keys (letters, digits, Delete, arrows, Enter, Space) belong to the interface, so `Delete` or `H` after a property field never deletes or hides the object you edit. Shortcuts with Ctrl or Alt and the function keys still work.

A floating window (Transform, Paste special, the texture, entity and lab windows) owns `Tab` from the moment it opens or you click in it until you click in a view or close it: `Tab` and `Shift+Tab` then move between its fields and buttons, wrapping around, instead of cycling the 2D view or reaching the active tool. Code editors (VMT text, BSP entity text) and the property name completion keep `Tab` for themselves.

## Trackpad gestures

With the trackpad gestures on (**Settings > Input**, see [Trackpad](trackpad.md)):

| Gesture | Effect |
|---|---|
| Two finger scroll | Pan a 2D view; orbit or move the 3D camera |
| Pinch, or `Ctrl` + scroll | Zoom a 2D view at the cursor; move the 3D camera forward or back |
| `Alt` + scroll | The mouse wheel (tool options such as the block count, otherwise zoom) |
| `Shift` + scroll | `Shift` + mouse wheel |
| `Space` + drag | Pan a 2D view; look around in 3D |
| `Space` + `Shift` + drag | Pan the 3D camera |
| Tap / two finger tap | Click / right click (context menu) |

The shortcut reference (`F1`) lists them under the viewport controls.

## MCP

`list_keybindings` and `set_keybinding` read and change the shortcuts from AI apps. See the [MCP tool reference](../../MCP.md).

## Tips

- Search by key: type `alt+` in the search box to see every Alt shortcut.
- Commands without a shortcut can be run from the command palette (`Ctrl+P`); give the ones you use often a key here.
- Switching sets keeps your own shortcuts, so you can try **Hammer++** and come back.

## FAQ

**Two actions share a shortcut. Which one runs?** Fix the conflict: the row is red and its tooltip names the other action. Remove or change one of them.

**How do I get Hammer's F9 back?** Choose the **Hammer** or **Hammer++** set: `F9` opens the compile window and `Ctrl+F9` compiles.

**Can I share my shortcuts?** Copy the `[workflow] keymap_preset` line and the `[keybindings]` table of your settings.toml to the other computer.

## Related pages

- [Actions and shortcuts](../reference/actions-shortcuts.md)
- [Command palette](command-palette.md)
- [Camera and navigation](camera-navigation.md)
- [Settings window](settings-dialog.md)

---
[Back to the documentation index](../README.md)
