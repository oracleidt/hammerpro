[Documentation](../README.md) > Features > Extrude / carve tool | [Français](../../fr/features/extrude-tool.md)

# Extrude / carve tool

The extrude / carve tool is an easier way to cut shapes than the clip tool: you click the points of an outline on a brush face (a window, a door, a niche, a logo...), then you push it into the brush to dig a recess or an opening, or pull it out of the face to extrude it. The outline can have any shape, concave too: the tool splits it into convex pieces so every brush it makes is a valid convex brush.

## Opening it

- **Tools > Extrude / carve tool**, the tool strip (pick icon, in the editing group next to the clipping tool), or `Shift+I`.
- Pressing `Shift+I` again while the tool is active switches between **Carve** and **Extrude**, like `Tab`.

## Drawing the outline

1. Click a face of a brush, in the 3D view or in a 2D view that looks at the face (the Top view for floors, the Front or Side view for walls). The face under the mouse is outlined before you click. When brushes are selected, their faces win over other brushes, even when another brush (a trigger, a pillar) is in front: select the brush to carve first. That click places the first point and picks the face; the next points stay on the plane of that face.
2. Click the next points around the shape. In the 3D view, points snap to a corner or to the middle of an edge of the face when the mouse is within about 10 pixels of it, otherwise to the grid on the face plane. Near an edge that is not on the grid, a point is kept on the edge instead of jumping off the face. Hold `Alt` to place points without any snapping. The length of the edge being drawn is shown next to the cursor.
3. A point that would make the outline cross itself is refused: the marker turns red with **The outline crosses itself**. A point already in the outline is ignored. An outline holds at most 128 points.
4. Close the outline by clicking its first point again, or by double clicking. You can also press `Enter` as soon as there are 3 points: the outline is closed and applied at once with the current depth.

`Backspace` (or **Remove point**) removes the last point, or opens a closed outline again. `Escape` cancels the outline.

A 2D view that looks along the face (a wall seen from above) cannot place points on it; use it to set the depth instead.

## Setting the depth

- Once the outline is closed, drag anywhere in a view (3D or 2D). The drag changes the depth from its current value, so pressing the mouse never makes the depth jump. Move the mouse the way the face normal points on screen to pull the face out (extrude), the other way to push it in (carve); the label next to the outline shows the result. When you look straight at the face, drag up to pull it toward you and down to push it away. The depth snaps to the grid. In a 2D view, the views that show the face edge on are the easiest.
- Or type the value in the **Depth** field of the options bar.
- **Through** (carve only) cuts the whole thickness of the brush: an opening instead of a recess. The options bar shows the thickness of the brush.
- Drag a point of a closed outline to move it (a move that would make the outline cross itself is refused).

The 3D and 2D views show the result live while you outline and change the depth: the brushes that will remain in green, the carved volume in red, the extruded prism in yellow, and the depth next to the outline. An outline the tool cannot apply (outside the face, crossing itself) shows the reason in red.

Press `Enter` or click **Apply** to apply. The whole operation is one undo step (**Carve face** or **Extrude face**).

## What carving and extruding do

| Mode | Result |
|---|---|
| **Carve** | The brush is replaced by the convex brushes that make the brush minus the outline prism, like **Carve** restricted to that brush. The brush keeps its id (it becomes the first piece), the other pieces are new brushes of the same owner (world or brush entity), group and visgroups. Pieces that can be merged back into one convex brush are merged, so you get as few brushes as possible. If the outline covers the whole face and goes through, the brush is removed. |
| **Extrude** | The prism of the outline is added on the face as new brushes of the same owner. When the brush and the prism together are still convex (an outline that covers the whole face), the brush itself grows instead and keeps its id. |

Textures:

- The faces kept from the brush keep their material and texture alignment.
- The new faces (the walls and the bottom of the hole, or the faces of the extruded prism) get the texture chosen in **New faces**: **Face texture** (the material of the clicked face) or **Current material**. Faces parallel to the clicked face (the bottom of a recess, the front of an extrusion) take the alignment of the clicked face, so the texture continues; the other new faces are aligned to the world with the scale of the clicked face.
- Overlays and cubemaps that used the clicked face follow its pieces.

## Options

| Option | What it does | Default |
|---|---|---|
| **Carve** / **Extrude** | Dig into the brush, or pull out of the face (`Tab` or `Shift+I` switches). Dragging the depth also sets it. | Carve |
| **Depth** | Depth in units (0.125 to 16384). | 16 |
| **Through** | Carve the whole thickness of the brush. | off |
| **New faces** | **Face texture** or **Current material** (with the current material chip; click it to show the texture browser). | Face texture |
| **Remove point** | Remove the last point, or open the outline again (`Backspace`). | |
| **Apply** | Carve or extrude the outline (`Enter`). | |
| **Clear** | Drop the outline. | |
| Point count and result | **N point(s)** and **Result: N brush(es)**, or the reason the outline cannot be applied. | |

## Tips

- For a window, draw the outline on the wall in the Front or Side view (or straight in 3D), turn **Through** on and press `Enter`.
- For a niche or a sign recess, keep **Through** off and drag the depth in the Top view.
- An outline larger than the face is fine: only the part over the brush is carved.
- Carving only touches the clicked brush. To cut a shape through several brushes, use [Carve](brush-operations.md) with a brush, or repeat the tool on each brush.
- The faces of a carved hole are real faces: give them another texture later with the [texture tool](texture-tool.md).

## MCP

The MCP tool `extrude_face` does the same from an AI app: `solid`, `side` (the side index of the face, from `get_faces`), the outline `points` as world positions on the face, `mode` (`carve` or `extrude`), `depth`, `through`, and the texture of the new faces (`new_faces`: `face` or `current`, or a `material`). `get_tool_options` and `set_tool_options` read and change the options of the tool itself (tool `extrude`: `mode`, `depth`, `through`, `new_faces`). See the [MCP tool reference](../../MCP.md).

## FAQ

**Why is my point refused?** It would make the outline cross itself, or it is the same point as another one.

**The outline starts on the wrong brush.** Select the brush you want to carve: its faces win over the brushes in front of it.

**Why does nothing happen when I click in a 2D view?** The view looks along the face. Click the face in the 3D view or in a 2D view that faces it.

**Can the result have invalid brushes?** No. The outline is split into convex pieces, every resulting brush is checked, and brushes that can be merged back are merged. The compiler sees ordinary convex brushes.

**Can I carve a displacement?** No, displacement faces are refused. Carve the brush before turning the face into a displacement.

## Related pages

- [Clip tool](clip-tool.md)
- [Polygon tool](polygon-tool.md)
- [Brush operations](brush-operations.md)
- [Vertex tool](vertex-tool.md)
- [Grids](grids.md)
- [Keyboard shortcuts](keyboard-shortcuts.md)

---
[Back to the documentation index](../README.md)
