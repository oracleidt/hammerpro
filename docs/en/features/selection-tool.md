[Documentation](../README.md) > Features > Selection tool | [Français](../../fr/features/selection-tool.md) | [Русский](../../ru/features/selection-tool.md)

# Selection tool

The Selection tool picks objects in the 2D and 3D views and transforms them: Hammer style handles in the 2D views, a transform gizmo and stretch arrows in the 3D view, Blender style G/R/E keys, arrow key nudging and clone dragging.

## Brush centers

A small **x** marks the center of every visible brush in the Top, Front and Side views. It is enabled by default, including with older settings. World brushes and brushes belonging to entities use the center of their axis-aligned bounds. The marker follows edits, hidden objects and cordons; selected brushes use the selection color. Its screen size stays constant while zooming.

Toggle **View > Display options > Show brush centers in 2D**, or **Settings > Helpers**. The saved setting is `helpers.show_brush_centers`. This affects the editor display only.

## Opening it

- Shortcut `Shift+S`, or **Tools > Active tool > Selection tool**, or its button in the tool palette.
- While it is active, its options bar shows the box mode, the handle mode, the 3D stretch handles, the gizmo, the pivot and the "select by" field.

## Selecting

### Click selection

| Gesture | 2D views | 3D view |
|---|---|---|
| Click an object | Selects it (replaces the selection) | Selects the nearest object you see under the pointer: models by their real shape (not their box), and what the view hides is skipped (hidden objects, tool textures hidden with the View menu, hidden displacements) |
| Click the same spot again | Within 2 seconds, while the last pick is still the selection: the next overlapping object (up to 64 are cycled) | Same: the next object behind. Later, or after the selection changed, the click starts over with the nearest object |
| `Ctrl+click` | Adds the object to the selection, or removes it when it is already selected | Same; `Ctrl+click` also picks through the gizmo |
| Click on empty space | Clears the selection (a click inside the selection box switches the handle mode instead) | Clears the selection (not with `Ctrl`) |
| Click an object that is already the whole selection | Switches the handle mode (Scale, Rotate, Skew, then Scale again), like Hammer | |
| Middle click | The next object at the spot of the last click, or the first one under the pointer | Same |
| Double click | Selects the object; for an entity (or a brush entity) it also opens its properties | Same |
| `Page Down` / `Page Up` | For 2 seconds after a click: the object behind / in front | Same |

A plain click on the gizmo or on a stretch arrow does not select (it only starts drags). After that two second window, `Page Up` and `Page Down` nudge the selection again (see Nudging with the keyboard below).

### Box selection (2D views)

Drag on empty space in a 2D view to draw a box. By default the selection happens as soon as you release the mouse. Turn on **Adjustable box selection (Enter to confirm)** in **Tools > Options > Selection** (or the **Enter** checkbox of the options bar) to work like Hammer++: the box stays after you release the mouse so you can adjust it before anything is selected:

- Drag one of its 8 handles to resize it, or drag inside it to move it (both snap to the grid, `Alt`: no snap).
- Press `Enter` to select what the box takes, `Ctrl+Enter` to add it to the selection.
- Press `Esc` to drop the box. A click outside it or a new drag also drops it.

This adjustable box is off by default since 2.0.3 (2.0.2 turned it on for everyone; upgrading turns it off once, turn it back on if you liked it).

What the box takes:

| Box mode | What it takes |
|---|---|
| **Touching** (default) | Every object touching the box |
| **Inside** | Only objects fully inside the box. A group or brush entity only partly inside is not taken |
| **Centers** | Only objects whose center is in the box. Centers are drawn as small crosses, and click selection also uses them. Handy in crowded areas |

- Hold `Alt` while dragging to use the other mode (Touching becomes Inside and the other way).
- Hold `Ctrl` to add the boxed objects to the selection. `Ctrl+drag` always draws a box, even over an object or the selection.

### Paint selection (3D view)

`Ctrl+drag` in the 3D view, away from the gizmo, adds every object the pointer sweeps over. `Ctrl+Shift+drag` removes them.

### Selection modes

What a click on a solid selects depends on the selection mode, set in the main toolbar or in **Edit > Selection mode**:

| Mode | A click selects | Default |
|---|---|---|
| **Groups** | The whole outermost group, or the whole brush entity | Yes |
| **Objects** | Brush entities as a whole, ignoring groups | |
| **Solids** | The single solid, even inside a brush entity | |

**Ignore groups** (`Ctrl+W`, **Edit > Selection mode**) switches to Solids, and back to the mode you used before. The mode is saved in `settings.toml` (`[edit] select_mode`). See [Groups](groups.md).

### Selecting faces, edges or vertices (Pick)

The **Pick** buttons of the options bar (also **Edit > Selection element**) choose the type of element a click in a 3D view selects. The setting is saved in `settings.toml` (`[selection] element`).

| Pick | Key | A click selects | Used by |
|---|---|---|---|
| **Object** | `4` | The whole object, as the selection mode says (default) | Everything else |
| **Vertex** | `1` | The corner of a brush nearest to the cursor | The vertex tool opens with it selected |
| **Edge** | `2` | The brush edge nearest to the cursor | `Ctrl+B` bevels the selected edges; the vertex tool opens with them selected |
| **Face** | `3` | The face under the cursor (the face selection of the texture tool) | Inset `I` and extrude `E` |

`Shift` or `Ctrl` adds to the selection or removes from it, a plain click replaces it, and a click on nothing clears it. Edges and vertices are picked within a few pixels of the cursor and only when they are not hidden behind another brush; picking an edge or a vertex also selects its brush, so the vertex tool can edit it. What is selected is drawn in every render mode, in the 3D views and in the 2D views: selected faces get a translucent cyan fill with a thick outline (the fill also shows faintly through other geometry), selected edges are thick cyan lines that always show, and selected vertices are large cyan points with a white center. Selected objects keep the usual selection tint. With an element pick active, the arrow keys, G, direct dragging on a picked element in 3D, and the move gizmo (3D) or the move drag (2D) move the picked faces, edges or vertices themselves instead of the whole brush: the corners of the picked elements are moved by the offset (snapped like an object move, `Alt` for single units) and the brush is rebuilt, which can make a face split when the move bends it; a move that would make an invalid brush is refused with a message. The picked elements stay picked after the move, the whole move is one undo step, and the rotate and scale handles are not offered for them. Any other change of the map, and undo or redo, clears the selected edges and vertices (selected faces stay selected as long as they exist). Shift plus the digit keys (1 vertex, 2 edge, 3 face, 4 object) are shortcuts of the actions `select_element_vertex`, `select_element_edge`, `select_element_face` and `select_element_object`; they can be rebound in **Settings > Keyboard**. In the 2D views a click always selects objects. The MCP tool `select_elements` selects edges or vertices by number, `select_faces` selects faces, and `get_selection` and `set_tool_options` (tool `select`, option `element`) read and change the setting.

### Edit menu commands

| Command | Shortcut | What it does |
|---|---|---|
| **Select all** | `Ctrl+A` | Selects every visible object |
| **Clear selection** | `Ctrl+Shift+A` (also `Shift+Q`) | Deselects everything |
| **Invert selection** | `Ctrl+I` | Selects what was not selected |
| **Previous face selection** | `Alt+Z` | Goes back to the previous face selection (switches to the texture tool, where faces show) |
| **Next face selection** | `Alt+Y` | Goes forward again |

With the **Hammer** and **Hammer++** keyboard presets, Select all is `Ctrl+Shift+A`, Clear selection is `Shift+Q` and Invert selection is `Ctrl+Shift+I`. See [Keyboard shortcuts](keyboard-shortcuts.md).

`Escape` clears the selection (or cancels the drag in progress).

### Select by class or material

The right end of the options bar has a **Class** / **Material** switch, a text field and a **Select** button. Type a class (for example `prop_*`) or a material (for example `tools/*`) and press `Enter` or **Select**:

- **Class** selects every visible entity of that class, **Material** every object using that material.
- Wildcards `*` and `?` work. Without a wildcard, an exact match is tried first, then any name containing the text.
- Hold `Ctrl` while applying to add to the selection.
- The bar then shows **{n} found** or **No match**. A query that finds nothing keeps the current selection.

## Transforming in the 2D views

Drag inside the selection box to move it. Drag an object that is not selected to select it and move it at once. The handles on the box depend on the handle mode:

| Handle mode | Handles | Drag does |
|---|---|---|
| **Scale** (default) | Corners and edges | Resizes the selection. `Ctrl`: keep the proportions (corner) or scale from the center (edge) |
| **Rotate** | Corners | Rotates around the pivot (see Movable pivot below), snapped to the rotation step. Hold the free rotation key (`Ctrl` by default) for a free angle |
| **Skew** | Edges | Shears the selection along the edge, snapped to the grid |

Click the selection (or empty space inside its box) to cycle the mode. A new selection always starts in Scale mode.

Modifiers during any drag:

| Key | Effect |
|---|---|
| `Shift` | Clone: the drag makes a transformed copy and leaves the original (one "Duplicate" undo step). A cloned single path node links itself after its source |
| `Alt` | No grid snap for this drag |
| `Escape` | Cancels the drag |

When moving, the point that lands on the grid is the bounds minimum of the brushes (an off-grid prop selected with them does not pull them off the grid), or the origin of a single point entity.

## Transforming in the 3D view

### Transform gizmo

The gizmo sits on the pivot of the selection (the movable pivot, the origin of a single point entity, else the bounds center) and keeps the same size on screen. Its mode is chosen in the options bar:

| Gizmo mode | Parts |
|---|---|
| **Move** (default) | Arrows move along an axis, squares move in a plane |
| **Rotate** | Rings rotate around an axis, snapped to the rotation step (free rotation key: no snap) |
| **Scale** | Arms with cube tips scale one axis (size snapped to the grid), squares scale two axes, the center square scales uniformly (factors snapped to 0.1). Scale goes from 0.01 to 100 |

`Shift` clones, `Alt` turns snapping off. **Local** (or `Ctrl+J`, **Gizmo in local space**, in the command palette) aligns the gizmo with the angles of a single selected point entity instead of the world axes. Untick **Show** to hide the gizmo.

### 3D stretch handles

With **3D stretch** on (default), each face of the selection bounds gets an arrow in the axis colors. Drag one to move that face along its axis while the opposite face stays, like the 2D scale handles (texture lock included, undo step "Stretch").

- `Ctrl`: stretch from the center (the opposite face moves the other way).
- `Shift`: clone. `Alt`: no snap.
- The arrows only appear when the selection holds brushes (solids, brush entities, groups of them). Point entities alone are only moved; point entities selected with brushes keep their place in the stretched box.
- Arrows of faces turned away are smaller and faded, arrows pointing at the camera are hidden, and an arrow that would cover the gizmo slides out along its axis.

### G, R and E modal transforms

With objects selected and the pointer over a view, press:

- `G` to move, `R` to rotate, `E` to scale, by moving the mouse without holding a button.
- `X`, `Y` or `Z` to lock an axis, `Shift+X`, `Shift+Y` or `Shift+Z` to lock the plane perpendicular to it. Press the same key again to free it.
- `G`, `R` or `E` again to switch the transform.
- A click or `Enter` applies, `Escape` or a right click cancels.
- `Alt`: no snap. For rotations, the free rotation key turns off the angle snap.

While a modal transform runs, `Z` locks the Z axis instead of entering mouselook.

### Alt + mouse wheel: model scale

`Alt+wheel` over a view multiplies the scale of the selected props by 1.1 per notch (0.01 to 100): `modelscale` for `prop_dynamic` and `prop_physics` (or any class whose FGD has it), `uniformscale` for `prop_static` when the FGD declares it. Consecutive notches are one undo step. Static props whose FGD has no `uniformscale` key are refused with a warning.

## Movable pivot

**Movable pivot** (**Tools** menu, the **Pivot** button of the options bar, or the viewport context menu) shows a cross that rotations, scales, the [Transform dialog](transform-align.md) and [Paste special](clipboard.md) turn around.

- Drag its cross in a 2D view to move it (snapped to the grid, `Alt`: no snap). In the 3D view grab it near the gizmo center (not in Scale mode).
- It moves with the objects, and it is dropped when the selected set changes.

## Entity helper points

When one entity is selected, its helper points (keys typed `origin`, `vecline` or `axis` in the FGD, such as the hinge axis of `phys_hinge` or the rotation origin of `func_rotating`) show as round handles. Drag one to write the key (one undo step, `Alt`: no snap, `Escape`: cancel).

## Nudging with the keyboard

With objects selected and the pointer over a view (and **Nudge with arrow keys** on):

| Keys | Effect |
|---|---|
| Arrow keys | Move by one grid step in the view plane. In the 3D view, along the world axes closest to the camera right and forward directions |
| `Page Up` / `Page Down` | Move toward / away from the viewer in a 2D view, up / down in the 3D view |
| `Alt+arrow` | Move by one unit |
| `Shift+arrow` | Clone one grid step away (or move one unit when **Shift+arrows clone** is off) |

With nothing selected, the arrow keys scroll the 2D view instead (see [Camera and navigation](camera-navigation.md)).

## Edge loops

The selection tool picks whole brushes, not edges, so it has no edge mode. The edge loop and edge ring commands live in the [vertex tool](vertex-tool.md#edge-loops-and-rings) (`Alt`+click an edge) and in the [bevel tool](bevel-tool.md) (`Alt`+click an edge to pick its loop for a bevel). To work on a loop from here, select the brushes that form it, press `Shift+V` for the vertex tool and `Alt`+click an edge. The commands of **Edit > Selection element** and **Tools > Vertices and edges > Slide edge loop...** work in the vertex tool.

## Viewport context menu

With a selection, a right click in a view adds at the top: **Move to floor**, **Move to ceiling**, their "aligned to the surface" versions, **Center origins** and **Movable pivot**. These are described in [Transform and align](transform-align.md).

## Options bar

| Option | What it does | Default |
|---|---|---|
| **Pick:** Object / Vertex / Edge / Face | What a click in a 3D view selects (see above) | Object |
| **Box:** Touching / Inside / Centers | What a box selection takes (see above) | Touching |
| **Enter** | The box waits for `Enter` and can be resized and moved first (saved in the settings) | Off |
| **Handles:** Scale / Rotate / Skew | What the 2D handles do | Scale |
| **3D stretch** | Arrows on the faces of the selected brushes in the 3D view (saved in the settings) | On |
| **Gizmo:** Show | Shows the transform gizmo in the 3D view | On |
| Move / Rotate / Scale | What the gizmo does | Move |
| **Local** | Gizmo of a single point entity follows its angles (saved in the settings) | Off |
| **Pivot** | Shows the movable pivot | Off |
| **Class** / **Material**, field, **Select** | Select by class or material | Class |

The box, handle and gizmo choices last for the session. These options are also reachable by name through scripts (`tool_option`) and the MCP tools `get_tool_options` / `set_tool_options` (`handles`, `box`, `gizmo`, `gizmo_mode`, `gizmo_local`, `stretch_handles`, `box_confirm`, `pending_box`). `pending_box` reads the waiting box, takes `x1 y1 x2 y2` to draw one in a 2D view, `confirm`, `add` or `cancel`.

## Settings

In **Tools > Options...**, page **Selection** (`[selection]` in `settings.toml`):

| Option | What it does | Default |
|---|---|---|
| **Rotation step** | Angle the rotation handles, gizmo rings and the R key snap to (1° to 90°) | 15° |
| **Free rotation key** | Key held for a free angle: Ctrl or Alt | Ctrl |
| **Handle size** | Size of the 2D handles, 4 to 16 points | 7 |
| **Keep visgroups when cloning** | Copies made by clone drags and Shift+arrows stay in the visgroups of their originals | On |
| **Shift+arrows clone** | Shift+arrows copies the selection one grid step away, Alt+arrows then nudges by one unit | On |
| **Adjustable box selection (Enter to confirm)** | Same as the **Enter** checkbox of the options bar | Off |
| **3D stretch handles** | Same as the options bar checkbox | On |
| **Gizmo in local space** | Same as the Local button | Off |

Page **Editing**: **Nudge with arrow keys** (on) and **Undo restores the selection** (on: selection changes are undo steps, see [Undo and history](undo-history.md)). Page **Viewports**: **Outline objects under the mouse** (off) outlines what is under the pointer in the 3D view. Snapping follows **Map > Snap to grid** (`Shift+W`) and the active grid (see [Grids](grids.md)).

## Tips

- In a crowded area, switch the box to **Centers**, or click the same spot several times to dig through overlapping objects.
- `Shift+drag` a path node to lay out a path quickly: each clone links itself after its source.
- For exact values, use the [Transform dialog](transform-align.md) (`Ctrl+M`).

## FAQ

**Why does clicking my selection not select the object behind it?** A click on an object that is already the whole selection switches the handle mode. Click the same spot again within 2 seconds (or press `Page Down`) to reach the object behind.

**I click a wall and something else gets selected.** A quick second click on the same spot digs to the object behind: wait 2 seconds, or click once. Brushes with a hidden tool texture (nodraw, clips, triggers, hints, see the View menu) and objects the 3D view does not draw are never picked there, but the 2D views still show and pick them.

**My rotation jumps by 15 degrees.** That is the rotation step. Hold `Ctrl` (or `Alt`, depending on **Free rotation key**) for a free angle, or change the step on the **Selection** settings page.

**The stretch arrows do not appear.** They only show for selections that contain brushes, when **3D stretch** is on. Arrows pointing straight at the camera are hidden: turn the camera a little.

**Can I select a single brush inside a func_detail?** Yes: switch to **Solids** mode (or press `Ctrl+W`), then click it.

## Related pages

- [Camera and navigation](camera-navigation.md)
- [Transform and align](transform-align.md)
- [Clipboard](clipboard.md)
- [Groups](groups.md)
- [Grids](grids.md)
- [Texture tool](texture-tool.md)
- [Keyboard shortcuts](keyboard-shortcuts.md)
- [Settings reference](../reference/settings.md)
- [MCP tool reference](../../MCP.md)

---
[Back to the documentation index](../README.md)
