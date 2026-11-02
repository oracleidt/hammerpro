[Documentation](../README.md) > Features > Vertex tool | [Français](../../fr/features/vertex-tool.md)

# Vertex tool

The vertex tool reshapes the selected solids by moving, merging and splitting their vertices and edges, with a live textured preview while you drag. It works like the Hammer vertex tool, with the Hammer++ additions (UV lock, real time preview) and a few more: vertex scaling, face splitting and a precise position editor.

## Opening it

- **Tools > Vertex tool**, the tool palette, or `Shift+V`.
- Press `Shift+V` again while the tool is active to cycle the handle mode (see below). A toast tells you the new mode.
- The tool edits the solids that are selected when it starts. If nothing is selected, click a solid to edit it. Up to 2048 solids are edited at once (a warning badge appears beyond that).

The options of the tool are shown in the options bar above the views.

## Handles

- **Square handles** are vertices. **Round handles** are edge midpoints.
- Selected vertices and edges are drawn in yellow. While dragging, the new edges are drawn in yellow, or red when the result would be an invalid solid.
- In 2D views, handles that overlap along the depth axis are picked together (like Hammer): clicking one vertex of a box in the top view selects the top and bottom vertices at that spot.
- In the 3D view only the handle itself is picked, and a three arm gizmo (X, Y, Z) is drawn at the center of the selected vertices.

## Handle modes

| Mode | What can be picked | Edge midpoint drag |
| --- | --- | --- |
| **Vertices and edges** (default) | Vertices and edge midpoints | Splits the edge: a new vertex is created at the midpoint and follows the mouse |
| **Vertices** | Vertices only | No edge handles |
| **Edges** | Edge midpoints only | Moves the whole edge (both of its vertices) |

Pick the mode with the three buttons of the options bar, or press `Shift+V` (selecting the tool again) to cycle them.

## Mouse and keys

| Gesture | Effect |
| --- | --- |
| Click a handle | Selects that vertex or edge (and replaces the vertex selection) |
| `Ctrl`+click a handle | Adds it to, or removes it from, the vertex selection |
| Click a solid that is not edited | Edits that solid instead (`Ctrl`+click adds or removes it), unless **Selected solids only** is on |
| Click empty space | Clears the vertex selection |
| Drag from empty space | Rectangle selection of the vertices (and edge midpoints) inside it; hold `Ctrl` to add to the selection |
| Drag a vertex | Moves the selected vertices (the dragged one joins the selection if it was not selected) |
| Drag a gizmo arm (3D view) | Moves the selected vertices along that world axis |
| Drag in the 3D view | Moves on the plane facing the camera; hold `Shift` to move along Z only |
| Hold `Alt` while dragging | No grid snapping |
| Arrow keys | Nudge the selected vertices by one grid step, in the axes of the view under the mouse |
| `Alt`+arrow or `Shift`+arrow | Nudge by one unit |
| `Page Up` / `Page Down` | Nudge along the depth axis (toward the viewer in 2D views, up and down in the 3D view) |
| `Ctrl+A` | Selects every vertex of the edited solids (the binding of **Select all**) |
| `Ctrl+B` | Snaps the selected vertices to the grid (the binding of **Snap selection to grid**). With the cursor on a brush edge in the 3D view it bevels that edge instead (see [Bevel tool](bevel-tool.md)) |
| `Ctrl+F` | **Split face** |
| `Alt+E` | **Scale vertices...** (press again to apply) |
| `Esc` | Cancels the current drag or scale; otherwise deselects the vertices; otherwise removes pending face splits |

Arrow key nudges follow the **Nudge with arrow keys** option of the **Editing** settings page (on by default). Grid snapping follows the **Snap to grid** toggle of the toolbar (`Shift+W`). When the edited solids have a custom grid attached, vertex drags use that grid (see [Grids](grids.md)).

The right-click menu of the views also offers **Split face**, **Scale vertices...** and **Snap vertices**.

## How edits are applied

- A drag shows its result live and becomes one undo step when you release the button. Keyboard nudges in a row merge into one undo step.
- A solid must stay convex. If a move would make a vertex end up inside the solid or on one of its faces or edges, or would make it flat, the status bar explains why and the edit is cancelled on release.
- **Merging**: a moved vertex dropped closer than the **Merge distance** to a vertex that did not move snaps onto it, and the two become one vertex.
- **Displacements**: a face keeps its displacement as long as it stays the same four sided face. Otherwise the displacement is removed and a toast tells you how many were lost. A warning badge in the options bar reminds you when the edited solids have displacements.

## Options bar

| Option | What it does | Default |
| --- | --- | --- |
| **Vertices and edges** / **Vertices** / **Edges** | Handle mode (see above) | Vertices and edges |
| **UV lock** | Textures follow the moved vertices (they may be skewed) instead of staying fixed in the world. Same setting as the **UV lock (vertex tool)** action of the **Tools** menu and the Settings page | Off |
| **Selected solids only** | Clicking another solid does not change the edited solids (only vertices are picked) | Off |
| **Merge distance** | A vertex dropped closer than this to another vertex merges with it (0 to 16 units) | 1 |
| Counter | "N of M vertices selected"; the tooltip gives the number of edited solids | |
| **N split(s)** | Face splits waiting for a vertex move; `Esc` removes them | |
| **All vertices** | Selects every vertex of the edited solids (`Ctrl+A`) | |
| **Split face** | Splits the face between the two selected vertices or edges (`Ctrl+F`) | |
| **Deselect** | Deselects all vertices (`Esc`) | |
| **Position** | X, Y, Z of the center of the selected vertices: type a value to move them precisely (one undo step) | |
| **Snap vertices** | Moves the selected vertices onto the nearest grid points (`Ctrl+B`) | |
| **Scale vertices...** | Opens the vertex scale (`Alt+E`) | |

The last four appear when vertices are selected.

## Split face

Select two vertices (or two edges) of the same face, then press `Ctrl+F` (**Tools > Split face**). The face is split along the line between them. Selecting two vertices that are already joined by an edge does nothing: pick two vertices across the face.

The split is kept by the tool until you move a vertex: the two halves become two faces, or merge back into one face if they are still flat. Use it to put a ridge or a fold in a wall, for example. Pending splits are drawn in their own color and counted in the options bar.

## Scale vertices

`Alt+E` (**Tools > Scale vertices...**) scales the selected vertices around a center, with a live preview. The options bar shows:

| Option | What it does | Default |
| --- | --- | --- |
| **Factor** | 1 keeps the size, 2 doubles the distances to the center, 0.5 halves them (0 to 100). The `-` and `+` buttons step by 0.1 | 1 |
| **Center** | Point the vertices move toward or away from. You can also drag its round handle in the views (it snaps to the grid like vertices, `Alt` for no snap) | Center of the box around the selected vertices |
| **Apply** | Applies the scale in one undo step (`Enter`, or `Alt+E` again) | |
| **Cancel** | Closes the scale without change (`Esc`) | |

Over a view, `Alt` + mouse wheel changes the factor by 0.1 per notch, `Ctrl+Alt` + wheel by 0.01. An invalid result (a concave or flat solid) keeps the scale open with a message.

## Settings

**Tools > Options...** (`Ctrl+,`), page **Vertices and displacements**, section **Vertex tool**:

| Setting | What it does | Default |
| --- | --- | --- |
| **Live preview while dragging** (`mesh.live_preview`) | Dragged solids show their new shape with their textures while the mouse button is held (up to 64 solids at once; larger drags only show the edges) | On |
| **UV lock** (`mesh.uv_lock`) | Same as the option of the options bar | Off |

## Tips

- To bevel a box corner, use **Vertices and edges** mode and drag an edge midpoint: the edge splits and you place the new vertex.
- Work in a 2D view to move a whole column of overlapping vertices at once, and in the 3D view to move a single vertex.
- If a drag is refused, try moving fewer vertices at once, or split the face first.
- Use **Position** to type exact coordinates, for example to align a slope to a given height.

## FAQ

**Why does my edit snap back when I release the mouse?** The solid would become concave or flat. The status bar shows the reason while you drag (edges turn red). Brushes must stay convex in Source.

**How do I merge two vertices?** Drag one onto the other. Any vertex dropped closer than the **Merge distance** (1 unit by default) to a vertex that did not move is welded to it.

**My textures stretch when I move vertices.** **UV lock** is on: textures follow the vertices. Turn it off to keep textures fixed in the world.

**Can I edit vertices with an AI assistant?** Yes, the MCP tools `get_brush_vertices`, `set_brush_vertices`, `move_brush_vertices` and `snap_brush_vertices` do the same edits (see the [MCP tool reference](../../MCP.md)).

## Related pages

- [Selection tool](selection-tool.md)
- [Clip tool](clip-tool.md)
- [Displacement tool](displacement-tool.md)
- [Grids](grids.md)
- [Texture tool](texture-tool.md)
- [UV editor](uv-editor.md)
- [Keyboard shortcuts](keyboard-shortcuts.md)
- [Settings reference](../reference/settings.md)

---
[Back to the documentation index](../README.md)
