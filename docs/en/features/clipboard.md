[Documentation](../README.md) > Features > Clipboard | [Français](../../fr/features/clipboard.md) | [Русский](../../ru/features/clipboard.md)

# Clipboard

Cut, copy and paste map objects, paste many copies at once with Paste special, duplicate the selection, and copy keyvalues from one entity to others.

## The commands

All are in the **Edit** menu and in the right click menu of the views:

| Command | Shortcut | What it does |
|---|---|---|
| **Cut** | `Ctrl+X` | Copies the selected objects to the clipboard and deletes them |
| **Copy** | `Ctrl+C` | Copies the selected objects to the clipboard |
| **Paste** | `Ctrl+V` | Pastes the clipboard contents; the pasted objects become the selection |
| **Paste special...** | `Ctrl+Shift+V` | Opens the Paste special dialog (several copies, offset, rotation) |
| **Delete** | `Delete` | Deletes the selected objects |
| **Duplicate** | `Ctrl+D` | Copies the selection one grid step away on X and Y, without using the clipboard; the copies are selected |
| **Array duplicate...** | `Ctrl+Shift+D` | Opens the Array dialog: copies of the selection along one or several axes with a gap between them |
| **Copy keyvalues** | none | Copies the keyvalues of the selected entity |
| **Paste keyvalues** | none | Pastes them onto the selected entities |

## Where Paste puts the objects

- `Ctrl+V` with the pointer over a 2D view: the contents are centered on the cursor and keep their depth.
- `Ctrl+V` with the pointer over the 3D view: the contents sit on the surface under the cursor.
- The offset follows the grid, so objects on the grid stay on it.
- Anywhere else (menu, pointer outside the views): the objects are moved by one grid step on X and Y when **Offset pasted objects** is on (default), else pasted at their original place.

## What the clipboard holds

The clipboard is the system clipboard, as VMF text: top level `solid`, `entity` and `hidden` blocks. So:

- You can copy between two Hammer Pro windows, and between Hammer Pro and Hammer or Hammer++, both ways.
- Any VMF fragment, even a whole VMF, can be pasted. The visgroups of a whole VMF are merged by name.
- A leading `hammerpro_clipboard` block (ignored by other editors) carries the groups, so pasting keeps the grouping, even in another Hammer Pro instance.
- Pasted objects get new ids.

## Paste special

**Edit > Paste special...** (`Ctrl+Shift+V`) pastes the clipboard several times, each copy moved and rotated from the previous one (like Hammer's Paste Special):

| Option | What it does | Default |
|---|---|---|
| **Copies** | Number of copies (1 to 10000; above 1000 the dialog warns that the map gets much heavier) | 1 |
| **Offset per copy** | X, Y, Z offset added for each copy. The `<` button next to an axis takes the size of the clipboard contents on that axis (copies side by side) | 64, 0, 0 |
| **Rotation per copy** | Rotation in degrees around X, Y and Z added for each copy, around the center of the copied objects | 0, 0, 0 |
| **Name prefix** | Optional prefix added to the name of every named pasted entity | empty |
| **Start at the original position** | The first copy is placed exactly over the copied objects | Off |
| **Group each copy** | Each copy becomes a group | Off |
| **Make entity names unique** | Pasted entities whose name already exists are renamed (`name_2`, `name_3`...) | Off |
| **Rotate around the pivot** | The rotation turns around the movable pivot instead of the center of the copied objects. Only available while the pivot is shown (**Movable pivot**) | On |

- The dialog shows **Offset of the last copy** so you can check the total.
- While the dialog is open, every change (copies, offset, rotation, start at the original position, rotate around the pivot) immediately shows the result as a green wireframe ghost of all the copies in the 3D and 2D views, and the dialog says how many objects and copies the preview shows. Nothing is pasted until you click **Paste** or press `Enter`; **Cancel**, `Escape` or closing the window removes the ghost. The preview follows the clipboard if it changes while the dialog is open, and with a very large number of copies it shows the first ones (the dialog says how many).
- Renamed entities keep working together: the keys of the pasted entities that name another pasted entity (targets, filters) follow the new names.
- `Enter` pastes, `Escape` closes. The values stay for the session.
- The whole paste is one undo step.
- The MCP tool `paste_special` pastes text or the clipboard (`text` or `from_clipboard`) with `copies`, `offset`, `rotation`, `pivot`, `start_at_original`, `group`, `unique_names` and `name_prefix`. With `preview: true` it pastes nothing and shows the same green ghost in the views, answering the objects, copies, shown copies, number of lines and bounds of the ghost; `clear_preview: true` removes it, and a real paste clears it too.

## Array duplicate

**Edit > Array duplicate...** (`Ctrl+Shift+D`) duplicates the selected brushes and entities along one or several axes at once, in one undo step:

| Option | What it does | Default |
|---|---|---|
| **X, Y, Z** (checkboxes) | The axes to duplicate along. Check one for a row, two for a grid, three for a block | X |
| **Copies** | Copies along each checked axis (1 to 1024; at most 4096 copies in all) | 3 |
| **Gap** | Space between the bounding boxes of two neighbours on that axis (not the distance between centers). Negative values overlap the copies | 0 |
| **Step** | Read only: the distance used between two copies on that axis, after the grid alignment | |

- The step of each axis is the size of the selection on that axis plus the gap, rounded up to the active grid, and the copies are placed on the grid. This follows the **active grid**, including rotated and custom grids: X, Y and Z are then the axes of that grid. With **Snap to grid** off the gap is used exactly.
- With several axes the copies fill the whole grid (2 copies on X and 1 on Y make 5 new copies).
- It works on brushes, point entities, brush entities and groups. The new copies are selected. `Enter` applies, `Escape` closes.
- Entity names are numbered automatically, see below. The MCP tool `array_duplicate` does the same with `axes`, `count` or `counts`, `gap` or `gaps`, and `snap`.

## Names of duplicated entities

When a copy, paste, duplicate, clone or array contains named entities whose names already exist, the copies get new names so they never clash:

- A name ending with digits is incremented: a set made of `porte1` (a `func_door`) and `btn1` (a `func_button`) becomes `porte2` and `btn2`, the next copy `porte3` and `btn3`. The same number step is used for the whole set, and it is always the next number that is free in the whole map (the leading zeros are kept: `door01` gives `door02`).
- A name without digits gets a number only when it is needed to avoid a duplicate (`lamp` gives `lamp2`).
- Names that do not exist yet (cutting then pasting, pasting into another map) stay as they are.
- Everything in the copied set that points to a renamed entity follows: the targets of outputs, `parentname` and the keys of type target (`target`, filters...) are rewritten to the new names. References to entities outside the set are left alone.

## Duplicate and clone

**Duplicate** (`Ctrl+D`) copies the selection one grid step away on X and Y. With the [Selection tool](selection-tool.md) you can also clone by holding `Shift` while dragging (move, scale, rotate, skew, gizmo, stretch) or with `Shift+arrow keys`. All these copies stay in the visgroups of their originals when **Keep visgroups when cloning** is on (page **Selection**, default on). They do not touch the clipboard.

## Copy and paste keyvalues

Select one entity and use **Edit > Copy keyvalues**, then select other entities and use **Edit > Paste keyvalues**: every key is pasted except `id`, `classname`, `origin` and `targetname`, in one undo step. Copy keyvalues also puts the keys on the system clipboard as `"key" "value"` lines, readable in a text editor. The same two commands are in the menu of the entity panel. Turn on **Paste keyvalues also pastes the name (targetname)** (page **Entities and browsers**, off by default) to paste the name too. See [Entity properties](entity-properties.md).

## Settings

| Option | Page | What it does | Default |
|---|---|---|---|
| **Offset pasted objects** | **Editing** | Paste moves the objects by one grid step so they do not overlap (when the paste does not follow the cursor) | On |
| **Keep visgroups when cloning** | **Selection** | Clones stay in the visgroups of their originals | On |
| **Paste keyvalues also pastes the name (targetname)** | **Entities and browsers** | Paste keyvalues also copies `targetname` | Off |

## Tips

- To lay out a row of identical pieces with an exact gap, use **Array duplicate** and check one axis.
- To lay out a row of identical pieces, copy one, open Paste special, click `<` on the axis of the row and set the number of copies.
- A spiral staircase: one step, **Rotation per copy** of 15 on Z and an **Offset per copy** of 8 on Z, with the pivot on the axis of the stairs.
- **File > Export > Export selection as VMF...** saves the selection to a file instead of the clipboard (see [VMF compatibility](../getting-started/vmf-compatibility.md)).

## FAQ

**Can I paste into Hammer?** Yes. The clipboard holds plain VMF blocks that Hammer and Hammer++ read. The groups block is simply ignored there.

**Paste says the clipboard does not contain map objects.** The clipboard holds something else (text, an image). Copy objects first.

**My pasted entities lost their links.** Pasted entities are renamed automatically when their names exist, and the links inside the pasted set follow. Use a **Name prefix** in Paste special for your own naming.

## Related pages

- [Selection tool](selection-tool.md)
- [Transform and align](transform-align.md)
- [Entity properties](entity-properties.md)
- [Groups](groups.md)
- [Visgroups](visgroups.md)
- [Undo and history](undo-history.md)
- [MCP tool reference](../../MCP.md)

---
[Back to the documentation index](../README.md)
