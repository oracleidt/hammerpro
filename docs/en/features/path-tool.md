[Documentation](../README.md) > Features > Path tool | [Français](../../fr/features/path-tool.md)

# Path tool

The path tool draws chains of path entities (`path_track`, `path_corner`, rope keyframes...) by clicking in the views. Each new point is named in sequence and linked from the previous one, and every chain of the map is drawn with direction arrows so you can edit it by dragging, inserting and removing points.

## Opening it

- **Tools > Path tool**, or its button in the tool palette. It has no default shortcut: give it one in **Settings > Keyboard** if you use it often (see [Keyboard shortcuts](keyboard-shortcuts.md)).
- While the tool is active, its options appear in the tool options bar and the status bar shows a short hint.

## Drawing a path

1. Click in a view to place the first point.
   - In a 2D view the point lands on the grid. Its depth (the axis you do not see) is taken from the path being drawn, else from a selected path point, else from the center of the selection, else from the last point placed, else 0.
   - In the 3D view the point rests on the surface under the cursor (the ground plane when there is none), moved away from it by the **Height** option (along the surface normal).
   - Hold `Alt` to place the point off the grid.
2. Click again to add the next points. A preview shows where the point goes and the name it will get.
3. End the path:
   - `Enter` or `Escape`, a double click, the **End path** button, or **End path** in the right-click menu. With the **Loop** option on, ending links the last point back to the first one.
   - Click an existing path point to link the last point to it and end the path. Clicking the first point of the path closes it into a loop.
   - **Close loop** (button or right-click menu) closes the path into a loop now, whatever the **Loop** option says (needs at least 2 points).
   - Switching to another tool also ends the path.

Each point names the next one in its "next" key: `target` for `path_track`, `path_corner` and `path_corner_crash`, `NextKey` for `keyframe_rope`, `move_rope`, `keyframe_track`, `move_track` and `move_keyframed`, `m_iszNextScript` for `scripted_sequence`, `nextassaultpoint` for `assault_assaultpoint`. New points get the default keyvalues of their class from the FGD. Point a `func_tracktrain` `target` at the first point name to make the train follow the path.

## Names

Points are named from the base name: `path` gives `path_1`, `path_2`... Each point takes the first free number after the previous one, so names stay unique in the map. When you insert a point after `path_2` and the following points are numbered in sequence, the new one becomes `path_3` and the following points shift by one (`path_3` becomes `path_4`...): every key and output naming them is updated.

## Editing existing paths

With any tool, the links of the path entities whose FGD class has a `line()` or `cylinder()` helper (`path_track`, `path_corner`...) are drawn for every visible entity in the 2D and 3D views, selected or not, like Hammer; a `cylinder()` helper also draws the `radius` circle of each node at that node. While the tool is active, every chain of the map is drawn with direction arrows. The chains of the selection and of the path being drawn are highlighted with the point names. Branch links (`altpath` of `path_track`) are drawn in another color.

| Gesture | What it does |
| --- | --- |
| Click a point | Select it (when no path is being drawn). |
| `Ctrl`+click a point | Add it to or remove it from the selection. |
| Drag a point | Move it. When it is part of the selection, all the selected points move together. Snapped to the grid (`Alt`: free). One undo step. |
| `Shift`+drag a point | Insert a new point after it, at the place where you release. Dragging from the last point of a chain extends the chain, and the tool goes on drawing from the new point. |
| `Escape` during a drag | Cancel the drag. |
| `Delete` | With **Relink on delete** on, removes the selected path points and links the point before each one to the point after it. |
| Right click | Context menu: **Insert path point after**, **Remove path point and relink**, **End path**, **Close path loop**. |

Inserting a point without dragging (button, menu or context menu) puts it halfway between the selected point and its next point, or one segment further when the selected point ends its chain. When a removed point was where a train starts, the train starts on the next point. At the end of a chain the link is cleared.

## Options

The options are saved in `settings.toml` under `[entities.path]`.

| Option | What it does | Default |
| --- | --- | --- |
| **Class** | Class of the new points. The list holds the chaining point classes of the loaded FGDs (`path_track` and `path_corner` first), or the stock classes listed above when no FGD is loaded. The tooltip shows the key that names the next point. | `path_corner` |
| **Name** | Base name of the points (up to 120 characters). | `path` |
| **Loop** | Ending a path links its last point back to the first one (a closed track). | Off |
| **Relink on delete** | Deleting path points with this tool links the point before each one to the point after it. Off: `Delete` deletes normally. | On |
| **Height** | Extra distance above the surface for points placed in the 3D view (-1024 to 1024 units). | 0 |

The options bar also has the buttons **End path**, **Close loop**, **Insert point** (needs exactly one selected path point) and **Remove and relink** (needs selected path points), and shows the number of points of the path being drawn and the number of paths in the map.

## Menu commands

These work from any tool, on the selected path points:

| Command | Menu | Default shortcut |
| --- | --- | --- |
| **Path tool** | **Tools** | none |
| **Insert path point after selection** | **Tools** | none |
| **Remove path points and relink** | **Tools** | none |

## Tips

- To build a train track: choose `path_track`, turn **Loop** on for a closed circuit, click the points in the top view, then set the `target` of the `func_tracktrain` to `path_1`. Press `F6` to ride it in the [player view](player-view.md#trains): it follows the track like in game (press `T` to start it when its **Initial Speed** is `0`).
- Undoing removes the last points from the path being drawn, so you can keep drawing after an undo.
- Everything is also available to AI assistants through MCP: `list_paths`, `create_path`, `insert_path_point`, `remove_path_points`, and `get_tool_options` / `set_tool_options` with the tool `path` (options `class`, `base_name`, `loop`, `relink`, `offset`). See the [MCP tool reference](../../MCP.md).

## FAQ

**My class is not in the Class list.** The tool lists point classes whose FGD has a next key (`target`, `NextKey`, `m_iszNextScript` or `nextassaultpoint`) that the class draws a line to, describes as the "next" one, or that its kind implies. Brush entities are never listed.

**Why did the names of my other points change?** You inserted a point inside a numbered sequence. The following points were renumbered and every reference to them updated, so nothing breaks.

**Delete removed my points but the chain is broken.** **Relink on delete** was off, or you deleted them with another tool. Use **Tools > Remove path points and relink** instead.

**Can I place points on a wall?** Yes, in the 3D view the point rests on the surface under the cursor, pushed out of it by **Height** along the surface normal.

## Related pages

- [Entity tool](entity-tool.md)
- [Entity properties](entity-properties.md)
- [I/O and logical view](io-logical-view.md)
- [Grids](grids.md)
- [Selection tool](selection-tool.md)

---
[Back to the documentation index](../README.md)
