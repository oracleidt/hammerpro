[Documentation](../README.md) > Features > Curve tool | [Français](../../fr/features/curve-tool.md) | [Русский](../../ru/features/curve-tool.md)

# Curve tool

The curve tool builds curved walls, rails, kerbs, pipes seen from above or ramps seen from the side out of plain box brushes. You draw a stroke of any shape in a 2D view, choose how many pieces it is cut into and how thick they are, and the tool makes one convex 6 sided brush per piece (more pieces per step with a curb or ditch profile, see below), following the curve with mitred joints: neighbouring brushes share their joint face exactly, so there is no gap and no overlap.

## Opening it

- **Tools > Curve tool**, the tool palette (creation group, next to the polygon tool), or `Shift+U`.
- The tool draws in the 2D views only; clicks in the 3D view are ignored. The 3D view shows the live preview.

## Drawing the curve

1. In a 2D view, **click** points one after the other, or **drag** to draw a freehand stroke (a point is added each time the mouse has moved about 24 pixels, at least two grid steps). Points snap to the active grid: with grid 8 every point is on a multiple of 8. Hold `Alt` to place points without snapping.
2. The curve lies in the plane of that view: drawn in the Top view it is horizontal, drawn in the Front or Side view it is vertical.
3. While drawing, the preview already shows the brushes with the current subdivisions and thickness: in the drawing view as outlines along the smoothed curve, in the other 2D views and in the 3D view as the brushes themselves.
4. **Drag a point** to move it. `Backspace` (or **Remove point**) removes the last point, `Escape` cancels the curve. A curve holds at most 1024 points.
5. While the curve is drawn or adjusted, `Shift` + mouse wheel changes the **Subdivisions** (by one eighth of the value per notch, at least 1, from 2 to 4096); the brush count in the label follows. The plain wheel keeps zooming.

## Validating and choosing the pieces

Press `Enter`, double click, or click **Validate** once the curve has at least 2 points. A **Curve** window opens (the same fields stay in the options bar):

| Field | What it does | Range | Default |
|---|---|---|---|
| **Subdivisions** | Number of brushes along the curve. Any whole number works; the `÷2` and `×2` buttons jump between the usual 2, 4, 8, 16... | 2 to 4096 | 16 |
| **Thickness** | Width of the brushes across the curve, centered on the stroke. | 1 to 1024 units | 16 |
| **Height** | Size of the brushes along the depth of the view (Z for a curve drawn in the Top view). | 1 to 16384 units | from the selection, see below |

The preview follows every change at once. Points can still be dragged in this step. Press `Enter` (or **Create**) to build the brushes, `Escape` (or **Back**) to go back to drawing.

How the curve is cut: a smooth spline (centripetal Catmull-Rom) goes through every point you placed, and it is cut into pieces of equal length. With 2 points the curve is a straight line cut in equal parts.

## Depth and height

Like the block tool, the brushes take their range along the view depth from the selection bounds, else from the last object created by the block, polygon or curve tool, else 0 to 64 units. In the other 2D views the curve appears as a band: drag one of its ends to change the range, or drag its inside to move it. The **Height** field changes the top of the range.

## Options bar

| Option | What it does | Default |
|---|---|---|
| **Subdivisions**, **Thickness**, **Height** | As in the Curve window. | 16, 16, from the selection |
| **Profile**, curb and ditch sizes, **Roll start**, **Roll end**, **Auto bank**, **Auto limit** | Cross section and banking, see below. | Flat, no banking |
| **Scale** | Texture scale of the new faces (0.01 to 64). | **Default texture scale** setting |
| Create as | **World brush**, **func_detail** or any brush entity class of the FGD. With a class, all the brushes go into one new brush entity. | World brush |
| Current material | Material put on every face, like the block tool. Click it to show the texture browser. | the current material |
| Pin (keep active) | Keep the curve tool active after creating. When off, the selection tool comes back. | on |
| **Remove point**, **Validate** / **Create** | Same as `Backspace`, then `Enter`. | |
| Summary | **N point(s)**, then **N brushes, length L**, or in red why the curve cannot be built (hover it for the details). | |

The new brushes become the selection; the whole curve is one undo step.

## Cross section and banking

The same Curve window and options bar hold the cross section and the banking. They apply to the whole curve.

| Field | What it does | Range | Default |
|---|---|---|---|
| **Profile** | **Flat**: one brush per piece. **Curb**: the road plus a raised curb on each side, 3 brushes per piece. **Ditch**: a V shaped channel, 2 brushes per piece. | | Flat |
| **Curb width** | Curb profile: width of each curb. The curbs are added outside the thickness, so the total width is the thickness plus two curbs. | 1 to 256 units | 8 |
| **Curb height** | Curb profile: how far the curbs rise above the road. | 0 to 256 units | 8 |
| **Ditch depth** | Ditch profile: how far the middle of the V sinks under the two edges. It must stay at least 1 unit under the height. | 0 to 512 units | 8 |
| **Roll start**, **Roll end** | Banking: the roll angle in degrees at the start and at the end of the curve, changing evenly along the curve. A positive angle raises the left side when you follow the curve from its first point to its last one. | -60 to 60 degrees | 0 |
| **Auto bank** | Automatic banking, proportional to the curvature: the roll in degrees for a bend of radius 1000 units (a bend of radius 500 gets twice as much). The road tilts toward the inside of each bend. 0 turns it off. It adds up to the roll start and end. | 0 to 90 | 0 |
| **Auto limit** | Largest roll the automatic banking may add. | 0 to 45 degrees | 8 |

The roll turns the cross section around the middle of the road, so the pieces still share their joint faces exactly. The ditch and the curbs also keep the same joints, piece after piece. The total roll is limited to 60 degrees. Every piece stays convex with flat faces: where banking, a bend or the ditch profile would twist the top of a piece, that piece is cut into two convex halves along a diagonal, so a banked curve can have more brushes than the pieces times the profile strips. A straight, evenly banked road stays one 6 sided brush per strip.

## Editing a curve

Every brush made by the curve tool remembers its curve: the points, the view, the depth and all the options are stored in the `editor` block of the brushes in the VMF, as `hammerpro_curve` keys. Hammer and Hammer++ ignore these keys and keep them when they save, so the map stays valid for them. Copies of the whole curve keep working. A brush you cut or move by hand keeps the data, but **Edit curve** puts back the shape of the curve.

1. Select one brush of the curve (or its brush entity).
2. **Tools > Edit curve**, or the command palette. The curve tool opens on the stored points, in the adjusting step, with the stored options.
3. Change what you want:
   - **Drag a point** to move it.
   - **Shift+click** inserts a point on the curve, between the two nearest points.
   - **Ctrl+click** a point removes it (a curve keeps at least 2 points).
   - **Back** goes to the drawing step to click new points at the end, `Backspace` removes the last point.
   - Change the subdivisions, thickness, height, profile and banking.
4. `Enter` (or **Apply**) replaces all the brushes of the curve by the new ones, in one undo step. They keep the material and stay in the same brush entity, if any. `Escape` cancels without changing anything.

If the brush has no curve data (it was not made by the curve tool, or the data was lost), the command says so.

## When the curve is refused

- **Segments fold inside a tight bend** (drawn in red): on the inside of a bend tighter than half the thickness, the pieces would cross each other. Lower the thickness or the subdivisions, or move the points to open the bend. A turn of more than about 165 degrees between two pieces is refused the same way.
- **Segments are too short**: a piece would be under 1 unit long. Lower the subdivisions or draw a longer curve.
- **The curve goes outside the map**: a corner of a brush would pass 16384 units.

## Tips

- Draw roads and walls in the Top view, arches and ramps in the Front or Side view.
- Few clicks give the smoothest curves: the spline does the rounding. A freehand stroke on a fine grid follows the hand more closely, including its wobbles.
- 16 to 64 subdivisions are enough for most curves; high counts make many small brushes, which cost compile time. Put decorative curves in a `func_detail` with **Create as**.
- The joints are mitred, so the outer corners are not on the grid. They are still clean planes shared by the two neighbours, and the map compiles without leaks or invalid brushes.
- The MCP tool `create_curve_brushes` builds the same brushes from a list of points (with the `profile`, `curb_width`, `curb_height`, `ditch_depth`, `roll_start`, `roll_end`, `auto_bank` and `auto_bank_limit` options), `get_curve_brushes` reads the stored curve, `edit_curve_brushes` replaces the brushes of a curve with changed points or options in one undo step, and `set_tool_options` with tool `curve` sets `subdivisions`, `thickness`, `height`, `tex_scale`, `create_as`, `keep_active` and the same cross section and banking options (see [MCP tool reference](../../MCP.md)).

## FAQ

**Why are some pieces red?** They fold inside a bend that is too tight for the thickness. See above.

**Can I make a closed ring?** Use the block tool's torus or arch for a perfect ring. With the curve tool, put the last point next to the first one.

**Can I get cylinders or wedges?** No: the curve tool only makes box like brushes, which are the most robust brushes for the compiler. The ditch profile makes wedge like pieces and banking tilts them, but they stay convex with flat faces.

## Related pages

- [Road tool](road-tool.md)
- [Road details](road-details.md)
- [Block tool](block-tool.md)
- [Polygon tool](polygon-tool.md)
- [Vertex tool](vertex-tool.md)
- [Grids](grids.md)
- [Keyboard shortcuts](keyboard-shortcuts.md)

---
[Back to the documentation index](../README.md)
