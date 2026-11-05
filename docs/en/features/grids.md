[Documentation](../README.md) > Features > Grids | [Français](../../fr/features/grids.md)

# Grids

The grid is what the tools snap to and what the 2D views draw. Hammer Pro keeps Hammer's single grid (the default) and adds one grid per view, named custom grids with any step, origin and orientation, and grids attached to objects.

## Opening it

| Action | Menu | Default shortcut |
| --- | --- | --- |
| **Smaller grid** | **Map > Smaller grid** | `[` |
| **Larger grid** | **Map > Larger grid** | `]` |
| **Show grid** | **Map > Show grid** | `Shift+G` (also `Shift+R`, Hammer's key) |
| **Snap to grid** | **Map > Snap to grid** | `Shift+W` |
| **3D grid on faces** | **Map > 3D grid on faces** | `P` |
| **Global grid**, **One grid per view**, **Custom grid**, **Grid of the selection** | **Map > Grids** | none |
| **Grid from selection**, **Grid from face** | **Map > Grids** | none |
| **Attach the grid to the selection**, **Detach the grid of the selection** | **Map > Grids** | none |
| **Grid manager...** | **Map > Grids > Grid manager...** | none |
| **Rotate grid 45 degrees**, **Align grid to the selected edge or face** | **Map > Grids**, command palette | none |

With the Hammer and Hammer++ keymap presets, **Show grid** is `Shift+R`.

The toolbar has **Show grid**, **3D grid on faces**, the grid size controls and **Snap to grid**:

- the **grid button** names the grid the size controls act on ("Grid" for the global grid, "Grid (Top)" for a view grid, or the name of a custom grid) and opens the grid menu. Its tooltip says which grid is active;
- the smaller and larger buttons, and a size field: type any size from 0.01 to 4096 (2.5 or 80 for example, a comma works as decimal separator) and press `Enter`.

**Ctrl + mouse wheel** in a 2D view changes the grid size (setting **Ctrl + mouse wheel in 2D** on the **Viewports** page of the settings; the other choice zooms every 2D view).

## Grid size keys

`[` and `]` divide and multiply the grid size by the **Grid size step** (2 by default, from 1.25 to 10). With the factor 2 they follow Hammer's steps, the powers of two from 0.125 to 4096, whatever size you typed (80 then `[` gives 64). With another factor the size is multiplied or divided and kept from 0.01 to 4096.

## Grid modes

The mode decides which grid the tools snap to and the views draw. Choose it in **Map > Grids**, the grid menu of the toolbar, the grid manager, or **Settings > Viewports > Grid**.

| Mode | What it does |
| --- | --- |
| **Global grid** (default) | One size for every view, like Hammer (`edit.grid_size`, 16 by default). |
| **One grid per view** | Each view (3D, top, front, side) has its own size, origin and major line interval: a precise grid in one view, a coarse one in another. The size controls change the grid of the view under the mouse, or of the last view used. The first switch to this mode gives every view the global size. |
| **Custom grid** | A named custom grid (the one chosen last, else the first one). With no custom grid, you get "No custom grid yet: create one in the grid manager or from the selection." |
| **Grid of the selection** | The grid attached to the selected objects; the global grid when they have none. |

## Custom grids

A custom grid has:

| Field | Meaning |
| --- | --- |
| **Name** | Unique, without case. |
| **Step X Y Z** | Step along the grid's own axes, any size with decimals (12, 24, 1.5...). Equal steps change together; hold `Shift` to change one. |
| **Origin** | A grid node sits here (world units). |
| **Pitch Yaw Roll** | Orientation, like entity angles: follow a rotated building (yaw) or a slope (pitch, roll). |
| **Major line every** | Every N lines is a major line (0: the setting of the views). |
| **Limits** | **Min** and **Max**: the grid applies inside this box only; outside, the global grid applies. **Fit to the selection** fills them in. |
| **Colour** | Color of its lines (default: the theme's grid color). |

Custom grids live in the map (saved in the VMF in a `hammerpro_grids` block, every change is an undo step) or in the settings as presets offered in every map. The grid menu lists them as "name (step)" and "name (step, preset)".

Two commands make a custom grid from what you selected, and make it active:

- **Grid from selection**: for a point entity, its origin and angles; for brushes, the axes of the first solid (up: its face facing up the most, X: its largest other face) with a node on the corner of its bounds. The step is the global grid size. Named "Selection grid".
- **Grid from face**: the first selected face (texture tool) becomes the XY plane of the grid (Z along the face normal), X along its longest edge, a node on its corner. Named "Face grid".

## Grids attached to objects

**Attach the grid to the selection** attaches the active custom grid to the selected objects (a preset is first copied into the map, in the same undo step). **Detach the grid of the selection** removes it. The attached grid is stored on each object (`hammerpro_grid` in its editor block).

With **Objects use their grid** on, editing an object that carries a grid (moving, scaling, stretching, vertices, clipping) or drawing on it snaps to its grid, whatever the grid mode. **Grid of the selection** makes the grid of the selected objects the active grid.

## The grid manager

**Map > Grids > Grid manager...** opens the **Grids** window:

- **Active grid:** the four modes as buttons;
- **Objects use their grid** and **Draw the custom grid in the 3D view** (the 3D view draws the plane of a custom or per view grid at its origin and orientation);
- **Grids of the views**: for **Top**, **Front**, **Side** and **3D**, the **Size**, **Origin X Y Z** and **Major lines** used by **One grid per view**;
- **Custom grids**: **New grid**, **Grid from selection**, **Grid from face**, the list of the grids of the map and of the presets, and every field of the chosen grid ("saved with the map" or "saved in the settings (every map)"), with **Use**, **Attach to the selection**, **Copy to the map** or **Save as preset**, and **Delete**.

## Rotating the grid

The **Rotate grid** row of the grid menu (the toolbar grid button) and of the grid manager turns the active grid in a 2D view, like Source 2 Hammer:

- type any angle in degrees (decimals and negative values work) or drag the field; **0** goes back to the straight grid, **45** is the quick button;
- **Rotate grid 45 degrees** adds 45 degrees to the current angle (twice gives a quarter turn, which looks straight again);
- **Align grid to the selected edge or face** turns the grid to the longest edge of the selected face, of the selected brush or to the yaw of the selected entity, as seen in that 2D view.

The grid turns around the axis of the view (Z in the top view, Y in the front view, X in the side view) and around its origin. The view used is the one under the mouse, else the last one used. What turns depends on the active grid:

- **Global grid** and **One grid per view**: the angle of that view, kept in the `angle` field of `[grid.per_view.top]` (front, side, perspective) of `settings.toml`. The size stays the global one. Switching to one grid per view keeps the angles;
- **Custom grid** (and the grid of the selection): its **Pitch Yaw Roll** change, so the angle is saved in the map with the grid and every change is an undo step.

The 2D view draws the minor and major lines along the rotated axes, clipped to the view. The origin axes and the 1024 unit lines stay straight, like the map limits. Every tool (move, scale and resize handles, block, clip, vertex, extrude, path, entity placement) snaps to the rotated lattice, and **Ctrl + mouse wheel** changes its size as usual. Rotation is a lattice rotation in the plane of the view, so a brush resize handle snaps its point to the rotated nodes.

## How the grid is drawn

| Option | Where | What it does | Default |
| --- | --- | --- | --- |
| **Show grid** (`edit.show_grid`) | Map menu, `Shift+G` | Draws the 2D grid. | On |
| **Snap to grid** (`edit.snap_to_grid`) | Map menu, `Shift+W` | Tools snap to the grid. | On |
| **Highlight grid lines every** (`view.grid_highlight_every`) | Settings > Viewports | Major line interval of the 2D grid (2 to 64). | 8 |
| **Show 1024 unit lines** (`view.show_1024_grid`) | Settings > Viewports | Lines every 1024 units. | On |
| **Grid in the 3D view** (`view.show_3d_grid`) | Settings > Viewports | A grid on the z = 0 plane around the camera, the X axis red and the Y axis green. | Off |
| **3D grid on faces (P)** (`render.grid_3d`) | Map menu, `P`, Settings > Rendering | Draws the grid on the faces of brushes, every grid step, brighter every major step. | Off |
| **3D grid opacity** (`render.grid_3d_opacity`) | Settings > Rendering | 0.05 to 1. | 0.45 |
| `render.grid_3d_fade_cells` | `settings.toml` | The 3D grid fades out beyond this many cells from the camera. | 64 |
| **Dotted grid** (`render.grid_dotted`) | Settings > Rendering | Draws the 2D grid as dots where its lines cross. | Off |
| **Highlight every 64 units** (`render.grid_highlight_64`) | Settings > Rendering | Lines every 64 units are brighter. | Off |
| **Grid density limit** (`render.grid_min_px`) | Settings > Rendering | Grid lines closer than this (2 to 32 px) are hidden; the next larger step shows instead. | 4 px |
| **Grid size step** (`workflow.grid_step_factor`) | Settings > Viewports | Factor of `[` and `]`. | 2 |

Grid settings live in the `[grid]` table of `settings.toml`: `mode`, `custom`, `per_view`, `presets`, `object_grids` (on), `plane_3d` (on).

## Grid size range and the bevel, inset and extrude tools

The grid size accepts every value from 0.01 to 4096 units. The mouse distances of the [bevel](bevel-tool.md), the interactive inset and the interactive extrude follow the active grid (size, custom grid and rotation) but never use a step under 1 or over 1024, and every brush they create has its new faces on that grid. While extruding, `Alt` switches to the alternate grid, an axis aligned grid of half the size. See [Grid for bevel, inset and extrude](extrude-tool.md#grid-for-bevel-inset-and-extrude).

## Tips

- For stairs with 12 x 8 steps, make a custom grid with the step 12 12 8 and attach it to the staircase (see [Custom grids for stairs](../guides/custom-grids-for-stairs.md)).
- **Snap selection to grid** (`Ctrl+B`) and **Snap each object to grid** (`Ctrl+Shift+B`) are in [Transform and align](transform-align.md).
- MCP tools: `list_grids`, `set_grid_mode`, `create_grid`, `edit_grid`, `delete_grid`, `attach_grid`, `rotate_grid` (angle, delta or align, for a view; see the [MCP tool reference](../../MCP.md)).

## FAQ

**Can I use a grid of 12 or 1.5?** Yes. Type it in the size field of the toolbar, or make a custom grid.

**`[` jumps from 80 to 64.** With the factor 2, the keys follow Hammer's powers of two. Set **Grid size step** to another value to multiply and divide freely.

**Where are custom grids saved?** Grids of the map are written in a `hammerpro_grids` block of the VMF and attached grids in the editor block of each object, so they come back when you open the map. Presets are in `settings.toml`.

**I want a rotated grid around one building only.** Give the custom grid **Limits**: outside the box the global grid applies.

**How do I get a 45 degree grid?** Open the grid menu, type 45 in **Rotate grid** (or use **Rotate grid 45 degrees**). To follow a wall, select it and use **Align grid to the selected edge or face**.

## Related pages

- [Custom grids for stairs](../guides/custom-grids-for-stairs.md)
- [Transform and align](transform-align.md)
- [Camera and navigation](camera-navigation.md)
- [Settings reference](../reference/settings.md)

---
[Back to the documentation index](../README.md)
