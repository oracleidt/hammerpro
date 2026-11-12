[Documentation](../README.md) > Features > Clip tool | [Français](../../fr/features/clip-tool.md) | [Русский](../../ru/features/clip-tool.md)

# Clip tool

The clipping tool splits the selected solids with a plane. You define the plane by a line in a 2D view (the plane is perpendicular to that view, or tilted by a third point), or directly in the 3D view: two points on a face, three points on surfaces, an axis plane or the plane of a face. The result is previewed live before you apply it.

## Opening it

- **Tools > Active tool > Clipping tool**, the tool palette, or `Shift+X`.
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

You can cut directly in the 3D view, like Hammer++'s 3D clipping. Clicks go to the surfaces of the selected solids first, even when another brush is in front; with nothing selected under the mouse, the closest surface is used.

### Snapping

The point under the mouse is shown as a dot before you click:

- **white**: snapped to a corner of the face under the mouse,
- **cyan**: snapped to the middle of an edge of that face,
- **yellow**: snapped to the grid in the plane of the face (the grid attached to the selection when it has one).

Corners and edge middles win when the mouse is within about 10 pixels of them. Hold `Alt` for no snapping at all.

### Two points on a face (the usual cut)

1. Click a first point on a face. A line follows the mouse and the outline of the future plane shows.
2. Click a second point. The plane goes through both points and is square to the first face: this is the cut line you drew on the face. The front is on the right of the direction you drew, as seen from the camera (like the 2D line). A short arrow from the plane shows the front side.
3. Press `Enter` or click **Clip**.

### Three points (free plane)

After the two points, click a third point on any surface (another face, another brush): the plane goes through the three points and keeps the front side it had. A third point aligned with the first two is refused (**The three points are aligned: pick other points**) and the first two stay. A third point on the same face as the first two is refused too, because the plane would be the face itself and cut nothing.

### Quick planes

| Click | Plane |
|---|---|
| `Shift` + click | Axis plane through the clicked point, square to a world axis. Choose the axis with the **Axis plane** buttons or press `X` to cycle **Auto**, **X**, **Y**, **Z**. **Auto** takes the axis most along the view direction, without the axis of the clicked face, so the cut line runs across the screen (a horizontal cut on a wall seen from the front, a cut across the floor you look along). Changing the axis updates a plane already placed this way. |
| `Ctrl` + click | The plane of the clicked face, to cut the other selected solids flush with an existing face. Its front is the outside of that face. |

While `Shift` or `Ctrl` is held, the candidate plane under the mouse is outlined before you click. The plane of a quick cut has one point; drag it (in 3D or in a 2D view) to move the plane along its normal.

### Adjusting and applying

- Drag a point to move it: in 3D on the surface under the mouse, or in the 2D views. The plane follows and keeps its front side. Moving the first point to another face makes the cut square to that face. A move that would make the plane impossible is refused.
- You can also press on a surface and drag before releasing: the new point follows the mouse until you let go.
- The preview shows the plane as a semi transparent quad and the two halves of every crossed solid in color (front green, back blue, removed red), in the 3D and 2D views.
- `Enter` or **Clip** cuts; the 3D points are then cleared and the next click starts new points. `Escape` or **Clear** forgets the points.
- The distances between the 3D points are shown in the 3D view when **Measurements** is on.

### In sync with the 2D views

A plane made in 3D shows in the 2D views: its points have handles you can drag there, the crossed solids show their two colored halves, and when the plane is square to a view, its trace is drawn as a dashed line with an arrow to the front side. A clip line drawn in a 2D view shows in 3D with its plane and points.

## Options and keys

| Option or key | What it does | Default |
|---|---|---|
| **Keep front** / **Keep back** / **Keep both** | Pieces to keep. `Shift+X` (selecting the tool again) or `Tab` cycles. | Keep both |
| **Clip** (`Enter`) | Split the selected solids. Enabled when the plane crosses at least one selected solid. | |
| **Clear** (`Escape`) | Forget the clip points. | |
| **3 points** (`C`) | Three point mode: after the line, click in another 2D view to tilt the plane. | off |
| **Axis plane: Auto / X / Y / Z** (`X`) | World axis of the `Shift` + click cut in the 3D view. `X` cycles. | Auto |
| `Shift` + click (3D) | Axis plane through the clicked point. | |
| `Ctrl` + click (3D) | Plane of the clicked face. | |
| `Alt` | No snapping (grid, corners and edge middles). | |
| **Measurements** (`O`) | Show the length and angle of the clip line next to it. Saved as `[brush] clip_measurements`. | on |
| **Current material on cuts** | The new cut faces get the current material instead of copying the most similar face of the brush. Saved as `[brush] clip_current_material`. | off |
| **N solid(s) cut** | Number of selected solids the plane crosses. | |

The two settings are also on the **Brushes** page of the Settings window (**Show the length and angle of the clip line**, **Current material on the cut faces**), opened with **Tools > Options...** (`Ctrl+,`).

## What happens to the solids

- Solids that the plane does not cross are left alone; invalid solids are skipped. A cut that would leave a piece thinner than 0.25 unit leaves the solid whole, so no sliver brushes are made. If the plane crosses nothing, you get **The clip plane does not cross the selected solids**.
- The clip is one undo step, and the console says **Clipped N solid(s)**.
- Brush entity solids keep their owner entity. Overlays and cubemaps that list clipped faces follow them like in Hammer: a face split in two is replaced by both halves, a face cut away is removed from their list.
- The cut faces copy the most similar face of the solid, or get the current material when **Current material on cuts** is on.
- Displacements stay displacements (Hammer turned them back into flat brush faces). Where the cut follows the lines of the displacement grid, every original vertex is kept exactly; elsewhere the surface is resampled onto the new faces at a power that keeps the detail (up to 4). Alphas, 4-way blend data, normals, offsets, elevation, flags and tags follow, and the two halves meet with no crack. A piece whose displaced face would be a triangle gets a one unit chamfer (Source displacements need four corners) and a five sided face is split into two quads. Turn off **Settings > Editing > Keep displacements when cutting** (`edit.keep_displacements_on_cut`) for the old behavior.

## Tips

- Use **Keep front** or **Keep back** to trim a brush in one go; use **Keep both** to split a wall where a door goes.
- To cut a slope precisely, draw the line in the Front or Side view with measurements on: the angle is shown in degrees.
- To cut a wall at a door height, `Shift` + click the wall at that height with the axis on **Auto** or **Z**.
- To cut several brushes flush with a face, select them and `Ctrl` + click that face.
- The MCP tool `clip_brushes` clips with a plane given by three points, a point and a normal, or a normal and a distance, with the keep mode and the material of the cut faces (see [MCP tool reference](../../MCP.md)). `set_tool_options` with tool `clip` sets `keep`, `three_point` and `axis`, and the setting `brush.clip_current_material` chooses the material of the cut faces.

## FAQ

**Why is the Clip button grayed out?** No plane is defined yet, or the plane does not cross any selected solid.

**Tab moves the keyboard focus instead of changing the mode.** `Tab` only cycles the mode when no text field or button had the keyboard focus. Click in a view first, or press `Shift+X` again.

**Which side is the front?** In 2D, the side the small arrow on the line points to. In 3D, the side the short arrow from the plane points to: the right of the line for two points (seen from the camera), the side facing the camera for an axis plane or three points placed at once, the outside of the face for a face plane.

**My click lands on the brush behind.** Clicks prefer the selected solids. Select the brush you want to cut, or the one you want to click on.

**`X` does not change the axis.** It only does while the clipping tool is active and no text field has the keyboard focus. Click in a view first.

## Related pages

- [Slice tool](slice-tool.md)
- [Block tool](block-tool.md)
- [Vertex tool](vertex-tool.md)
- [Extrude / carve tool](extrude-tool.md)
- [Brush operations](brush-operations.md)
- [Selection tool](selection-tool.md)
- [Grids](grids.md)
- [Settings reference](../reference/settings.md)

---
[Back to the documentation index](../README.md)
