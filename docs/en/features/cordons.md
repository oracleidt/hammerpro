[Documentation](../README.md) > Features > Cordons | [Français](../../fr/features/cordons.md) | [Русский](../../ru/features/cordons.md)

# Cordons

A cordon is a named box that limits the part of the map you work on. While cordons are in use, everything outside every active cordon is dimmed in the views, cannot be picked, and can be left out of a compile so you test one room in seconds instead of compiling the whole map.

## Opening it

- **Tools > Cordon tool** (or its button in the tool palette) draws and edits the boxes in the 2D views. It has no default shortcut: give it one in **Settings > Keyboard** (see [Keyboard shortcuts](keyboard-shortcuts.md)).
- **View > Use cordons** switches cordoning on or off for the whole map. The cordon button of the toolbar does the same.
- The **Cordons** section of the **Visgroups** tab (right dock) lists every cordon of the map.
- While cordons are in use, a red **Cordon** badge shows in the status bar. Its tooltip gives the number of boxes; a click stops using the cordons.

Cordons are saved in the VMF like Hammer does: a map with a single cordon is written with Hammer's old `cordon` block (unless the file already used the newer block), a map with several cordons with the newer `cordons` block. Multi-box cordons written by other editors keep all their boxes when you save (the tool edits the first box of each cordon, the others are drawn faintly).

## The cordon tool

The tool edits one cordon at a time, the **target**. Pick it in the **Cordon:** list of the options bar, by clicking its row in the cordon list, with **Edit the box with the cordon tool** in that list, or by clicking inside its box in a 2D view (the smallest box wins where boxes overlap).

| Gesture (2D views) | What it does |
| --- | --- |
| Drag in empty space | Redraws the box of the target cordon. Its depth along the axis you do not see is kept. |
| Drag a handle | Resizes the box. |
| Drag inside the box | Moves the box. |
| Click inside another cordon | Makes it the target. |
| `Escape` | Cancels the drag in progress, then leaves the **New cordon** mode. |

Each change is applied when you release the mouse button, as one undo step. A drag that draws no area keeps the box as it was. The 3D view shows the boxes but does not edit them.

The depth of a new box (the axis you do not see in the view where you draw it) comes from the target cordon, else from the selection, else from the map with one grid step of margin, else from Hammer's default box (-1024 to 1024).

### Options bar

| Option | What it does |
| --- | --- |
| **Cordon:** | The cordon the tool edits. Shows **New cordon** while the next box will be a new cordon, **(none)** when the map has no cordon. |
| **+** button | **New cordon**: the next box you draw becomes a new cordon. |
| **Active** | Includes this cordon in the cordoned area (tooltip: "Include this cordon in the cordoned area"). |
| Size | The size of the target box. |
| **x** button | Deletes this cordon. |
| **Use cordons** | Same as **View > Use cordons**. |

### Right-click menu

| Entry | What it does |
| --- | --- |
| **New cordon around the selection** | A new cordon fitting the selected objects. A flat selection gets one grid step of thickness. Needs a selection. |
| **Delete this cordon** | Deletes the target cordon. |
| **Use cordons** / **Stop using cordons** | Switches cordoning on or off. |

The status bar hint reminds you of the gestures and adds "(Cordons are not in use.)" when cordoning is off.

## The cordon list

The **Cordons** section of the [Visgroups](visgroups.md) tab shows:

| Element | What it does |
| --- | --- |
| **+** | New cordon around the selection, or a cube at the center of the 2D views when nothing is selected. The name starts in rename mode. |
| **Use** | Use cordons (same as **View > Use cordons**). |
| Count | Number of cordons. |
| Checkbox of a row | Active: includes the cordon in the cordoned area. |
| Click a row | Makes it the cordon the cordon tool edits. The tooltip gives its corners and size. |
| Double click a row | Renames it. |
| Frame button | **Frame the views on this cordon**. |
| Box button | **Edit the box with the cordon tool**. |
| **x** button | **Delete cordon**. |
| Right-click a row | **Edit the box with the cordon tool**, **Frame the views on this cordon**, **Rename**, **Delete cordon**. |

A cordon with no volume shows "This cordon has no volume: it is deleted at its next edit."

## What "in use" does

Cordons are in use when **Use cordons** is on and at least one active cordon has a usable box. Then:

- the boxes of the active cordons are drawn in red in every view;
- the 2D views are shaded outside every active cordon (setting below);
- objects lying entirely outside every active box are dimmed and cannot be picked. An object touching a box counts as inside;
- the name of each active cordon is shown at its corner when there are several.

Turning cordons on, off, adding, renaming, resizing or deleting a cordon are undo steps.

## Compiling only the cordon

In the [Compile and run window](../compiler/compile-window.md), **Compile only the cordoned area** keeps only what touches the active boxes. Each box is sealed by six brushes of the **Cordon seal material**, and the map is compiled as `<map>_cordon` so the full map is never overwritten. When no player start lies inside the cordon, one is added on the floor of the first box. The same option exists on the **Compile** page of the settings (**Compile only the active cordon**).

## Options

**Tools > Options... > Cordons and visgroups** (`[visibility]` in `settings.toml`):

| Option | What it does | Default |
| --- | --- | --- |
| **Cordon material** (`cordon_material`) | Material of the brushes that seal the cordoned area when only the cordons are compiled. **Use the current material** fills in the current material. A blank value uses the default. | `tools/toolsskybox` |
| **Shade outside the cordons in 2D** (`shade_outside_2d`) | Darkens the 2D views outside every active cordon while cordons are used. | On |
| **Shade opacity** (`shade_opacity`) | Opacity of that shade, 0 to 1. | 0.45 |
| **Cordon names in 2D** (`show_cordon_names`) | Shows the name of each active cordon at its corner when there are several. | On |

## Tips

- Keep one cordon per area you test often (a room, a vehicle course) and switch them with their **Active** checkbox instead of redrawing boxes.
- Lighting preview has its own local mode: **Bake around the camera** seals the area around the 3D camera like a cordon (see [Lighting preview](lighting-preview.md)).
- MCP tools: `list_cordons`, `add_cordon`, `edit_cordon`, `set_cordons_enabled`, `get_visibility` (see the [MCP tool reference](../../MCP.md)).

## FAQ

**I drew a cordon but nothing is dimmed.** Cordoning is off. Tick **Use cordons** (options bar, cordon list or View menu), and check that the cordon is **Active**.

**Why can I not select an object?** It lies outside every active cordon. Stop using cordons or enlarge the box.

**Does a cordon compile overwrite my map?** No. The cordon compile is named `<map>_cordon`.

**Can a cordon have several boxes?** Hammer Pro keeps the extra boxes of maps made elsewhere and uses them in views and compiles, but it creates one box per cordon. Make several cordons instead.

## Related pages

- [Visgroups](visgroups.md)
- [Hide and isolate](hide-isolate.md)
- [Compile and run window](../compiler/compile-window.md)
- [Settings reference](../reference/settings.md)

---
[Back to the documentation index](../README.md)
