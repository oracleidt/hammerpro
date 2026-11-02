[Documentation](../README.md) > Features > Bevel tool | [Français](../../fr/features/bevel-tool.md)

# Bevel tool

The bevel tool rounds or chamfers the edges of brushes straight in the 3D view, something Hammer++ does not have. Point at an edge, press `Ctrl+B`, move the mouse to set the size and turn the mouse wheel to set how smooth the round is. Press `B` during the operation for the inverse (concave) bevel: a cove carved into the edge, or a concave round that fills an inner corner, which turns the top of a doorway into an arch.

Every result is made of valid convex brushes, so the map compiles like any other.

## Opening it

- **Tools > Bevel tool**, the tool strip (quarter circle icon, in the editing group after the extrude / carve tool), or `Shift+J`.
- From the **Selection** or **Vertex** tool: point at a brush edge in the 3D view and press `Ctrl+B`. The bevel tool opens and starts bevelling that edge at once (see [Ctrl+B](#ctrlb-and-snap-to-grid) below).

## Bevelling an edge

1. Move the mouse over a brush in the 3D view. The edge under the cursor is highlighted in yellow, with the two faces around it.
2. Press `Ctrl+B`. The bevel starts with the last radius used (16 units the first time), limited to what the faces allow.
3. Move the mouse **up** to make the bevel bigger and **down** to make it smaller. The size snaps to the grid; hold `Alt` for a free size. You can also type it in the **Radius** field of the options bar.
4. Turn the **mouse wheel** up for more segments (a smoother round) and down for fewer. One segment is a flat chamfer. The **Segments** field does the same.
5. Press `B` (or `Ctrl+B` again) to switch between the normal and the inverse bevel.
6. Press `Enter` or click to apply. `Escape` or a right click cancels.

While you bevel, the views show the result live: the edges of the resulting brushes in green, the new rounded faces in yellow in the 3D view, and a label next to the edge with the kind (round, chamfer, cove or inner fill), the radius and the number of segments. The label turns red with the reason when the bevel cannot be made.

The whole operation is one undo step (**Bevel edges**). The resulting brushes are selected.

## Several edges at once

- Click an edge to pick it, `Ctrl+click` to add or remove more edges. Picked edges are drawn in blue.
- Press `Ctrl+B` or `Enter` (or click **Bevel** in the options bar) to bevel all the picked edges with the same radius and segments.
- A double click on an edge starts bevelling it at once.
- Click away from any edge, or press `Escape`, to drop the picked edges.

Edges of several brushes can be bevelled together. When two picked edges are on opposite sides of the same face (the two top edges of a wall), each radius is limited to half the face so the rounds never cross.

## 2D views

In a 2D view, an edge seen end-on (a point, like the corner of a wall in the Top view, or the edge of a beam seen from its end) can be pointed at, picked and bevelled the same way; moving the mouse up and down changes the radius by one unit per pixel at zoom 1. The selected brushes are searched first, so select the brush when several edges are at the same spot. The preview shows in every view.

## The normal bevel

The normal bevel rounds the edge outward. The brush is cut by one plane per segment and stays **one convex brush**: a box with a rounded edge and 4 segments has 10 faces. With 1 segment it is a flat chamfer.

## The inverse bevel

The inverse bevel is concave. What it does depends on the edge:

| Edge | Result |
|---|---|
| A free outer edge | A **cove** is carved into the edge: a concave quarter round, like the inside of an arch or a cove moulding. The brush is split into convex pieces that follow the curve (one per segment plus the rest of the brush, merged back when possible). |
| An edge where the brush meets another brush at an inner corner (the top corner of a doorway, the bottom of a wall that stands on a floor) | The inner corner is **filled** with a concave round made of new brushes, added to the same owner (world or brush entity) as the bevelled brush. The two brushes are not changed. |

### Making an arch

1. Make a wall with a rectangular doorway: three brushes (two jambs and the lintel above the opening), for example with the [extrude / carve tool](extrude-tool.md) and **Through**.
2. Point at one top corner of the opening in the 3D view (the edge where the lintel meets a jamb), press `Ctrl+B`, then `B` for the inverse bevel.
3. Move the mouse up: the radius stops at what the opening allows. Turn the wheel for 8 to 12 segments and press `Enter`.
4. Do the same for the other top corner. With a radius of half the opening width, the two rounds meet in the middle and make a round arch.

To do both corners in one step, pick the two edges with `Ctrl+click` first.

## Textures

- The faces kept from the brush keep their material and texture alignment.
- The rounded faces get the texture of one of the two faces around the edge, **wrapped** around the whole round: it continues from that face over every segment without a seam, so the curve reads as one surface. The only seam is at the far tangent line, where the round meets the other face. With **New faces: Current material** they get the current material instead (still wrapped).
- The rounded faces of a bevel with 2 segments or more get smoothing group 1, so the compiled lighting is smooth over the round.
- The round of a corner fill takes the texture of the face of the bevelled brush next to the corner, wrapped all along the round (a skirting or an arch in the material of the brush you pointed at). The ends of the fill are on the wall faces and take the texture and alignment of those faces.

## Limits

- The radius is limited to what the faces around the edge allow: a round cannot go past the end of a face. The options bar shows **Largest radius** while you bevel.
- Segments shorter than half a unit are merged (a tiny radius with 32 segments gives fewer segments); the label shows the number actually used.
- Brushes with a displacement cannot be bevelled.
- An edge between two faces that are flat with each other has no corner to bevel.

## Options

| Option | What it does | Default |
|---|---|---|
| **Radius** | Radius of the round in units (0.25 to 16384, limited by the faces). Moving the mouse during the bevel sets it. | 16 |
| **Segments** | Faces of the round, 1 (chamfer) to 32. The mouse wheel during the bevel sets it. | 4 |
| **Inverse (concave)** | Cove or inner corner fill instead of a round (`B` during the bevel). | off |
| **New faces** | **Edge faces** (the textures around the edge, wrapped) or **Current material**. | Edge faces |
| **Bevel** | Start bevelling the picked edges. | |
| **Apply** / **Cancel** | Apply (`Enter` or click) or cancel (`Escape` or right click) the bevel in progress. | |
| **Clear edges** | Drop the picked edges. | |

## Ctrl+B and snap to grid

`Ctrl+B` is also **Snap selection to grid** in every key preset (Hammer Pro, Hammer and Hammer++). The two share the key this way:

- In the **Bevel** tool, `Ctrl+B` starts the bevel when an edge is under the cursor or edges are picked, and switches to the inverse bevel during the operation. Otherwise it snaps the selection to the grid as usual.
- In the **Selection** and **Vertex** tools, `Ctrl+B` bevels the brush edge under the cursor when the cursor is on an edge in the **3D view**. Anywhere else (a 2D view, the middle of a face, empty space) it snaps the selection (or the selected vertices) to the grid as before.
- In every other tool `Ctrl+B` only snaps to grid.

If you rebind **Snap selection to grid** in **Settings > Keyboard**, the bevel follows the new key.

## MCP

The MCP tool `bevel_edges` does the same from an AI app: `solid` with `edge` (an index from `get_brush_edges` or two points `[[x, y, z], [x, y, z]]`) or `edges` (several, also as `{solid, edge}` or `{solid, a, b}` objects on several brushes), `radius`, `segments`, `inverse`, and the texture of the rounded faces (`new_faces`: `face` or `current`, or a `material`). `dry_run` answers the radius and segments that would be used, the largest radius and the kind of each edge (`round`, `cove` or `fill`) without changing the map. `get_brush_edges` lists the edges of a brush. `get_tool_options` and `set_tool_options` read and change the options of the tool itself (tool `bevel`: `radius`, `segments`, `inverse`, `new_faces`). See the [MCP tool reference](../../MCP.md).

## FAQ

**Can a bevel make invalid brushes?** No. A normal bevel keeps one convex brush; an inverse bevel makes convex pieces that are checked one by one, and pieces that can be merged back are merged. Nothing is changed when the result would not be valid.

**Why does the radius stop growing?** The round reached the end of a face around the edge, or half of a face shared with another picked edge.

**Why did the inverse bevel add brushes instead of carving?** The edge is an inner corner with another brush: the concave round fills the corner. To carve a cove instead, bevel an edge that is not against another brush.

**Can I bevel a func_detail or a brush entity?** Yes. The pieces and the fills stay in the same entity.

## Related pages

- [Extrude / carve tool](extrude-tool.md)
- [Clip tool](clip-tool.md)
- [Vertex tool](vertex-tool.md)
- [Brush operations](brush-operations.md)
- [Keyboard shortcuts](keyboard-shortcuts.md)

---
[Back to the documentation index](../README.md)
