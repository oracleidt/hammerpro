[Documentation](../README.md) > Features > Clip tool | [Français](../../fr/features/clip-tool.md)

# Clip tool

The clipping tool splits the selected solids with a plane. You define the plane by a line in a 2D view (the plane is perpendicular to that view, or tilted by a third point), or by three points on surfaces in the 3D view. The result is previewed live before you apply it.

## Opening it

- **Tools > Clipping tool**, the tool palette, or `Shift+X`.
- Select the solids to clip first: only selected solids are cut (**Select the solids to clip first** otherwise).

## Modes: which pieces to keep

| Mode | Result |
|---|---|
| **Keep both** | Each crossed solid becomes two solids. The back piece keeps the solid id, the front piece is a new solid (selected when the original was). |
| **Keep front** | Only the piece on the front side of the plane stays. |
| **Keep back** | Only the piece on the back side stays. |

The default mode is **Keep both**. Change it with the buttons of the options bar, or cycle it like in Hammer: select the tool again (`Shift+X` while the clipping tool is active), or press `Tab` (`Shift+Tab` cycles the other way). The cycle order is both, front, back.

In the preview, the front piece is green, the back piece blue, and a piece that will be removed is red.

## Clipping in a 2D view

1. Drag in a 2D view to draw the clip line. Both ends snap to the grid (to the grid attached to the selection when it has one); hold `Alt` for no snapping. The line is placed at the depth of the selection center.
2. The plane goes through the line, perpendicular to the view. A small arrow in the middle of the line points to the front side (the right of the direction you drew, as seen on screen).
3. Adjust the line:
   - drag one of its two handles to move that end,
   - `Ctrl` + drag a handle or the line itself to move both ends together (the first point snaps, the second follows),
   - `Shift` + drag always starts a new line, even over a handle.
4. Press `Enter` or click **Clip** to cut.

### Three point mode (tilted plane)

Press `C` or click **3 points** to turn the three point mode on (like Hammer++). After drawing the line, click in another 2D view to place a third point: the plane through the line is tilted to pass through it. The third point can be dragged in its view, and the tilt angle (**Tilt N°**) is shown next to it. `Shift` + drag in the other view starts a new line there instead. When the third point lies on the line, the plane stays perpendicular. Press `C` again to go back to two point mode (the third point is then ignored).

## Clipping in the 3D view

1. Click three points on brush faces. Points snap to the grid on the face plane. The status bar tells how many points are still needed.
2. With three points the plane is defined; its front is the side facing the camera. Three aligned points are refused (**The three points are aligned: pick other points**) and you start again.
3. The points can be dragged afterwards: in 3D on the surface under the mouse, or in the 2D views. The plane follows and keeps its front side. A move that would align the points is refused.
4. Press `Enter` or click **Clip**. A new click on a surface after a complete plane starts a new set of points.

The distances between the 3D points are shown in the 3D view.

## Options and keys

| Option or key | What it does | Default |
|---|---|---|
| **Keep front** / **Keep back** / **Keep both** | Pieces to keep. `Shift+X` (selecting the tool again) or `Tab` cycles. | Keep both |
| **Clip** (`Enter`) | Split the selected solids. Enabled when the plane crosses at least one selected solid. | |
| **Clear** (`Escape`) | Forget the clip points. | |
| **3 points** (`C`) | Three point mode: after the line, click in another 2D view to tilt the plane. | off |
| **Measurements** (`O`) | Show the length and angle of the clip line next to it. Saved as `[brush] clip_measurements`. | on |
| **Current material on cuts** | The new cut faces get the current material instead of copying the most similar face of the brush. Saved as `[brush] clip_current_material`. | off |
| **N solid(s) cut** | Number of selected solids the plane crosses. | |

The two settings are also on the **Brushes** page of the Settings window (**Show the length and angle of the clip line**, **Current material on the cut faces**), opened with **Tools > Options...** (`Ctrl+,`).

## What happens to the solids

- Solids that the plane does not cross are left alone; invalid solids are skipped. If the plane crosses nothing, you get **The clip plane does not cross the selected solids**.
- The clip is one undo step, and the console says **Clipped N solid(s)**.
- Brush entity solids keep their owner entity. Overlays and cubemaps that list clipped faces follow them like in Hammer: a face split in two is replaced by both halves, a face cut away is removed from their list.
- The cut faces copy the most similar face of the solid, or get the current material when **Current material on cuts** is on.

## Tips

- Use **Keep front** or **Keep back** to trim a brush in one go; use **Keep both** to split a wall where a door goes.
- To cut a slope precisely, draw the line in the Front or Side view with measurements on: the angle is shown in degrees.
- The MCP tool `clip_brushes` clips with a plane given by three points or a normal and a distance (see [MCP tool reference](../../MCP.md)).

## FAQ

**Why is the Clip button grayed out?** No plane is defined yet, or the plane does not cross any selected solid.

**Tab moves the keyboard focus instead of changing the mode.** `Tab` only cycles the mode when no text field or button had the keyboard focus. Click in a view first, or press `Shift+X` again.

**Which side is the front?** In 2D, the side the small arrow on the line points to. In 3D, the side facing the camera when the third point was placed.

## Related pages

- [Block tool](block-tool.md)
- [Vertex tool](vertex-tool.md)
- [Extrude / carve tool](extrude-tool.md)
- [Brush operations](brush-operations.md)
- [Selection tool](selection-tool.md)
- [Grids](grids.md)
- [Settings reference](../reference/settings.md)

---
[Back to the documentation index](../README.md)
