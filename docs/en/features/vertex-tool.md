[Documentation](../README.md) > Features > Vertex tool | [Français](../../fr/features/vertex-tool.md) | [Русский](../../ru/features/vertex-tool.md)

# Vertex tool

The vertex tool reshapes the selected solids by moving, merging and splitting their vertices and edges, with a live textured preview while you drag. It works like the Hammer vertex tool, with the Hammer++ additions (UV lock, real time preview) and a few more: vertex scaling, face splitting and a precise position editor.

## Opening it

- **Tools > Vertex tool**, the tool palette, or `Shift+V`.
- Press `Shift+V` again while the tool is active to cycle the handle mode (see below). A toast tells you the new mode.
- The tool edits the solids that are selected when it starts. If nothing is selected, click a solid to edit it. Up to 2048 solids are edited at once (a warning badge appears beyond that).

The options of the tool are shown in the options bar above the views.

## Align selected vertices on an axis

In a 2D view, select at least two vertices, press `S`, then `X`, `Y` or `Z` (holding `S` while pressing the axis also works). The selected vertices receive one common coordinate on that world axis. The reference is the center of the bounds of the entire selected group, snapped to the active grid, including the current view's grid origin and spacing. Other coordinates stay unchanged. The shortcut is inactive in text fields and 3D views. Each alignment is one undo step and keeps the vertex selection. World brushes, brush entities and displacement base faces use the same operation; solids must remain valid and displacement faces must remain planar quads.


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
| `Alt`+click an edge | Selects its **edge loop** (see [Edge loops and rings](#edge-loops-and-rings)); `Shift` adds it to the selection. Works in every handle mode and in the 2D and 3D views, the edge is picked along its whole length |
| `Ctrl+Alt`+click an edge | Selects its **edge ring**; `Shift` adds |
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

The right-click menu of the views also offers **Split face**, **Scale vertices...**, **Snap vertices**, the welding commands and the edge selection commands.

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
| **Rotate vertices...** | Opens the vertex rotation | |

A row holds the edge selection commands (see [Edge loops and rings](#edge-loops-and-rings)): **Select edge loop**, **Select edge ring**, **Grow selection**, **Shrink selection**, **Select boundary**, **Slide edge loop...** and **Rotate vertices...**, with the number of selected edges and vertices. Another row holds the welding commands (see [Welding vertices](#welding-vertices)): **Weld** with its target, **Weld selected vertices**, **Weld distance** with **Weld vertices by distance**, and **Remove degenerate faces and edges**.

The last four appear when vertices are selected.

## Split face

Select two vertices (or two edges) of the same face, then press `Ctrl+F` (**Tools > Split face**). The face is split along the line between them. Selecting two vertices that are already joined by an edge does nothing: pick two vertices across the face.

The split is kept by the tool until you move a vertex: the two halves become two faces, or merge back into one face if they are still flat. Use it to put a ridge or a fold in a wall, for example. Pending splits are drawn in their own color and counted in the options bar.

## Welding vertices

Dropping a vertex near another one merges them (see **Merge distance**). The welding commands do it on purpose, on whole selections, from the second row of the options bar, the **Tools** menu (while the vertex tool is active), the right-click menu of the views, the command palette and MCP. Each command is one undo step.

| Command | What it does |
| --- | --- |
| **Weld selected vertices** | Merges the selected vertices of each solid into one vertex. The target is chosen in the list next to **Weld**: **To center** (the average of the vertices, default), **To first** or **To last** (the vertex with the lowest or highest index, the vertices have no selection order) |
| **Weld vertices by distance** | Merges every pair of vertices of the edited solids closer than **Weld distance** (0.001 to 64 units, default 1), repeated until no pair is that close. Solids that would become invalid are left as they are and a warning counts them |
| **Remove degenerate faces and edges** | Merges the vertices closer than 0.01 unit and removes the faces and edges that are left degenerate, and the faces that are no longer part of the solid. Welding already drops the faces it collapses, so this is for solids that came with such leftovers |

A label next to each command previews the result, for example "12 vertices will be welded". It counts the vertices that disappear: for **Weld selected vertices** with the current selection and target, for **Weld vertices by distance** with the current distance.

A weld that would make a solid concave or flat (for example merging two opposite corners of a face) is refused with a message, and nothing changes. The selected vertices stay selected on the welded vertex.

MCP: `weld_brush_vertices` (mode `vertices`, `distance` or `cleanup`) does the same on the document, and the options `merge_distance`, `weld_distance` and `weld_target` of the vertex tool are listed by `get_tool_options` and changed with `set_tool_options`. The actions are `vertex_weld`, `vertex_weld_distance` and `vertex_cleanup`.

## Scale vertices

`Alt+E` (**Tools > Scale vertices...**) scales the selected vertices around a center, with a live preview. The options bar shows:

| Option | What it does | Default |
| --- | --- | --- |
| **Factor** | 1 keeps the size, 2 doubles the distances to the center, 0.5 halves them (0 to 100). The `-` and `+` buttons step by 0.1 | 1 |
| **Center** | Point the vertices move toward or away from. You can also drag its round handle in the views (it snaps to the grid like vertices, `Alt` for no snap) | Center of the box around the selected vertices |
| **Apply** | Applies the scale in one undo step (`Enter`, or `Alt+E` again) | |
| **Cancel** | Closes the scale without change (`Esc`) | |

Over a view, `Alt` + mouse wheel changes the factor by 0.1 per notch, `Ctrl+Alt` + wheel by 0.01. An invalid result (a concave or flat solid) keeps the scale open with a message.

## Edge loops and rings

An *edge loop* is a line of edges that goes on by itself, and an *edge ring* is the set of parallel edges across the four sided faces. Brushes are convex solids, so the loops are worked out on the edges of all the edited brushes together: brushes that share a corner or an edge join into one network, and a face that lies exactly against the face of another brush counts as a wall between them, not as a surface.

- **Edge loop.** The loop goes straight through a vertex where four edges meet (the middle of a wall made of several brushes, a grid) and follows the boundary of a face where three edges meet (the corners of a brush, so the loop of a box edge is the ring around one of its faces, and on a prism it follows the ring around the end cap). It stops at a vertex where five or more edges meet and at a dead end. When two faces are possible at a corner, the loop follows the face that looks toward you, then the one with the most sides. Along the border of a surface it follows that border.
- **Edge ring.** From the edge, it crosses each four sided face to the opposite edge and goes on through the next face, in both directions. On a box it gives the four parallel edges.
- **Alt+click** an edge to select its loop, **Ctrl+Alt+click** to select its ring, add `Shift` to add to the selection instead of replacing it. Holding `Alt` shows the loop (or the ring with `Ctrl`) that a click would select, in blue, in every view. The selected edges are drawn in yellow with their vertices in all views, and the status bar counts the selected edges and vertices.
- **Selection commands.** **Edit > Edge selection** (also in the options bar, the right-click menu and the command palette): **Select edge loop** and **Select edge ring** extend the selected edges (or the edges between selected vertices) to their loops or rings; **Grow selection** adds every vertex next to the selected ones; **Shrink selection** removes the vertices that touch an unselected one; **Select boundary** keeps the edges on the border of the selected region (faces whose vertices are all selected) or, when no face is complete, the selected vertices that have an unselected neighbour.
- **Moving the loop.** The selected loop is a normal vertex selection: drag it with the mouse or the gizmo, type its **Position**, nudge it with the arrow keys, or use **Scale vertices...** (`Alt+E`). Every brush that shares a vertex of the loop follows it. The edge selection stays after the move, and the move is one undo step.
- **Rotate vertices...** (**Tools > Rotate vertices...**, options bar) rotates the selected vertices around a center with a live preview: the angle (-360 to 360 degrees, `Alt` + mouse wheel changes it by 5 degrees, `Ctrl+Alt` by 1), the axis (**X**, **Y**, **Z** or **Loop normal**, the normal of the plane of the selected vertices, so a loop turns in its own plane) and the center (the middle of the selected vertices, editable). **Apply** (`Enter`) or **Cancel** (`Esc`).
- **Slide edge loop...** (**Tools > Slide edge loop...**, options bar) slides the selected loops along the neighbouring faces, keeping their shape, with a live preview. Each vertex of the loop moves along its own edge that leaves the loop: positive values go toward one side of the loop, negative values toward the other, by that fraction of the edge (-1 to 1, `Alt` + mouse wheel changes it by 0.05). Where a vertex has no edge on one side (the corner of a brush) the other edge is extended, so the cap of a box slides up or down along its sides. The selected edges must form simple loops or lines, without branches.
- An invalid result (a concave or flat brush) is shown in red with the reason, like every vertex edit, and cannot be applied.

MCP: `select_edge_loop`, `select_edge_ring` and `grow_edge_selection` (mode `grow`, `shrink` or `boundary`) give the edges and vertices of a loop, a ring or a selection for seed edges, with `toward` to choose the face and `select` to also show the result in the vertex tool; `move_edge_loop` moves, scales, rotates or slides a loop in one undo step (see the [MCP tool reference](../../MCP.md)). The actions are `edge_select_loop`, `edge_select_ring`, `edge_grow`, `edge_shrink`, `edge_select_boundary`, `edge_slide` and `vertex_rotate`.

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

**How do I merge two vertices?** Drag one onto the other. Any vertex dropped closer than the **Merge distance** (1 unit by default) to a vertex that did not move is welded to it. To merge several at once, select them and use **Weld selected vertices**, or use **Weld vertices by distance** to merge every close pair.

**My textures stretch when I move vertices.** **UV lock** is on: textures follow the vertices. Turn it off to keep textures fixed in the world.

**Can I edit vertices with an AI assistant?** Yes, the MCP tools `get_brush_vertices`, `set_brush_vertices`, `move_brush_vertices` and `snap_brush_vertices` do the same edits (see the [MCP tool reference](../../MCP.md)).

**How do I select the whole line of edges around a brush face or along a wall?** `Alt`+click one of its edges. `Ctrl+Alt`+click selects the parallel edges instead.

**The loop stops at a brush, why?** The loop only runs through the edited brushes, and brushes join only where their corners meet exactly. Select all the brushes of the wall first, and check that the corners line up.

## Related pages

- [Selection tool](selection-tool.md)
- [Clip tool](clip-tool.md)
- [Displacement tool](displacement-tool.md)
- [Bevel tool](bevel-tool.md)
- [Grids](grids.md)
- [Texture tool](texture-tool.md)
- [UV editor](uv-editor.md)
- [Keyboard shortcuts](keyboard-shortcuts.md)
- [Settings reference](../reference/settings.md)

---
[Back to the documentation index](../README.md)
