[Documentation](../README.md) > Features > Extrude / carve tool | [Français](../../fr/features/extrude-tool.md) | [Русский](../../ru/features/extrude-tool.md)

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
- The depth drag follows the active grid, so the new face lies on a grid plane (the step is kept between 1 and 1024 units). Hold `Alt` while dragging to use the alternate grid: an axis aligned grid of half the size that ignores the grid rotation (see [Grid for bevel, inset and extrude](#grid-for-bevel-inset-and-extrude)). Typing the value in the **Depth** field is never snapped.
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

## Inset face

**Tools > Inset face...** (also the **Inset face...** button of the options bar of this tool) works on the selected faces, selected with the texture tool or the face selection of the select tool. It moves the edges of each face inward by a distance and splits the brush into convex pieces: a frame of brushes around the face plus the inner block, so the result is still made of valid convex brushes. A box face inset gives 5 brushes (4 for the frame, 1 inner block) whose volumes add up to the original brush.

The window asks for:

- **Distance**: how far the edges move inward, in units (0.125 to 16384). It must be smaller than half of the narrowest part of the face.
- **Depth**: 0 keeps the inner block flush with the face. A positive value raises it out of the face, a negative one sinks it into the brush (inset then extrude, like Blender I then E). A sink must stay under the thickness of the brush.
- **Snap to grid**: rounds distance and depth to the grid (on by default when snapping is on).

The kept faces keep their texture and alignment. The new faces (the sides of the frame hole and of the inner block) take the material of the inset face and, when parallel to it, its alignment. Overlays and cubemaps that used the face follow its pieces. The first piece keeps the brush id, the new pieces belong to the same owner, group and visgroups, and the whole operation is one undo step (**Inset face**). All the pieces are selected.

Limits: the brush must be convex and valid, and displacement faces are refused. The frame is always made of quads, one per edge of the face (see below). When the distance is larger than the face allows, a warning explains it (**The distance is larger than the face**) and nothing is changed.

With several faces selected the window insets touching coplanar faces as one region, like the interactive inset (see below); the MCP tool `inset_face` offers `individual` to inset each face on its own.

## Interactive inset (I)

The interactive inset works like the Inset of Blender (`I`), on the same geometry code as **Inset face...**.

1. Choose the faces. Select one or several faces with the face selection (the **Face** element of the selection tool, the texture tool, or with the vertex tool all the corners of the faces to inset). With no face selected, the face under the mouse is used: put the cursor over a face of a brush in the 3D view and press `I`.
2. Put the mouse over the 3D view and press `I`. While a face is selected or hovered and a 3D view has the mouse, `I` starts the inset. You can also use **Tools > Interactive inset (I)**, the command palette, or the **Interactive inset (I)** button of the options bar of this tool: then move the mouse over a view to start.
3. Move the mouse. The thickness is the distance from the face center to the cursor, relative to where the mouse was when you started: moving toward the center of the face makes the frame thicker, moving away brings it back to zero. A live preview shows the new inner face (yellow, filled) and the cut lines to the corners of the outline (green) in the 3D and the 2D views. The value is shown at the cursor and in the status bar, with the grid in use, the number of brushes the result will have, or the reason it cannot be applied (in red).
4. The thickness and the depth follow the active grid by themselves (see [Grid](#grid-for-bevel-inset-and-extrude)). Hold `Ctrl` to force the grid even when the snap is off, `Alt` for a free value.
5. Type a number to set an exact value (digits, `.` or `,`, and `-` for the depth). `Backspace` edits it, and emptying it gives the control back to the mouse.
6. Press `E` to switch between **Inset** (thickness) and **Depth**. In the depth mode, moving the mouse the way the face normal points on screen raises the inner face out of the brush and the other way sinks it, so the inner face can be extruded or sunk in the same operation. The value shown in brackets is the one the mouse edits.
7. With several faces selected, press `I` again to switch between **region** (the default) and **individual**, the way Blender's `I` toggles individual faces. The label shows which one is active.
8. Left click or `Enter` applies, in one undo step (**Inset face**). `Escape` or right click cancels and nothing is changed. A thickness under 0.125 or larger than the face allows is refused with a message and the tool stays active.

The first `I` on one face is the classic inset. The result is the same as **Inset face...**: the frame around the new inner face is made only of quads, one convex brush per edge of the outline (a triangle gives 3 frame brushes, a quad 4, a pentagon 5, and so on), plus the inner block. The pieces are cut along the line from each corner of the outline to the matching corner of the inner face. Every piece is a valid convex brush, the volumes add up to the original brush, and the faces that stay visible keep their texture and alignment. A face with a short edge (a trapezoid, for example) cannot be inset farther than the point where that edge would vanish: the distance is then refused instead of making triangles in the frame.

### Several faces: region or individual

With several faces selected, `I` insets them together, as a region, like the Inset of Blender without individual faces:

- Faces that lie on the same plane and share an edge (the tops of brushes side by side, a floor or a wall made of several brushes) form one region. The region gets one outline, which is the outline of the union of the faces, and one inner block: there is no frame along the edges the faces share, the frame crosses the seams and each brush gets its part of it. The outline may be concave (an L shape of three faces, for example). The region must have no hole and its pieces must not touch only at a corner, otherwise a message says so (**The selected faces do not form one simple region**).
- Faces that are not on the same plane, or are far from each other, or touch only at a corner, are inset on their own with the same thickness and depth. The top and the front face of one brush are two such faces: the pieces tile the brush (the volumes add up) and the faces keep their own frame.
- **Individual** (`I` again, or `individual: true` in the MCP tool) insets every face on its own even when they touch, which gives one frame per face.

The result is always made of convex brushes. This is why a region is limited to coplanar faces: on a convex brush two adjacent faces on different planes meet at a crease, and a shared frame would have to fold around it, which cannot be split into convex brushes.

The `I` key is the shortcut of the action `inset_interactive`, and it only starts the inset while a face is selected or hovered and a 3D view has the mouse. You can rebind it in the keyboard settings.

## Interactive extrude (E)

The interactive extrude pulls a face out of its brush as a new brush, like the Extrude of Blender (`E`) but making a separate convex brush, or pushes it into its brush to carve material out of it (negative distance). It is also on **Tools > Interactive extrude (E)**, the action `extrude_interactive`, and the command palette.

1. Choose the faces as for the inset: the selected faces, or the face under the mouse when none is selected. Several faces work together: each one becomes its own new brush, pulled along its own normal, in one undo step.
2. Put the mouse over a 3D view and press `E` (the key is free in the keymap presets; in the selection tool `E` still starts the scale of the selected objects, but not while faces are selected). Move the mouse: moving the way the face normal points on screen makes the new brush higher (yellow preview of the top of each new brush); moving the other way past zero gives a negative distance, which carves: a red-orange preview shows the face moving into the brush and the label shows a negative distance. To dig a pocket in the middle of a face, use an inset with a negative depth or the carve mode of this tool.
3. The distance follows the active grid (see below), also when negative. Type a number for an exact value, which is not snapped; type `-` first for a negative one (`-16`). Left click or `Enter` applies (**Extrude face** for new brushes, **Carve face** when every distance is negative), `Escape` or right click cancels. A distance whose size is under 0.125 is refused.

The new brush is a separate convex brush that starts on the face and reuses its texture and alignment: it is never merged into the original brush, so it is a new object you can move or delete on its own. It belongs to the same entity as the brush it comes from.

A negative distance does not add a brush: it cuts the slab of material of that depth under the face out of its own brush, so the face moves into the brush and the brush stays one valid convex brush with its id (the new face reuses the texture of the old one). Several faces can be carved together, also on the same brush, and mixed with extruded faces in the same single undo step. A carve that would go through the whole brush is refused with a message, nothing changes.

## Grid for bevel, inset and extrude

The mouse distances of the [bevel](bevel-tool.md) radius, the inset thickness and depth and the extrude distance snap to the active grid on their own while **Snap to grid** is on:

- The grid size, a custom grid (step, origin and angles) and a rotated grid are honored. The distance of an extrude or of an inset depth is chosen so that the new top plane lies on a plane of the grid along the face direction (when the face normal follows a grid axis); in other directions it is a multiple of the grid size. The inset thickness is a multiple of the smallest grid step along the face, so the new edges of a face on the grid stay on the grid.
- The grid size used by these three tools is kept between 1 and 1024 units, whatever the grid is set to (a grid of 0.25 acts as 1 and a grid of 4096 as 1024). The grid setting itself accepts every size from 0.01 to 4096, so 1 to 1024 are all valid.
- Every brush these tools create has its new faces placed from the source geometry by those snapped distances, so a face that was on the grid gives new corners that are on the grid.
- `Ctrl` forces the grid even when **Snap to grid** is off. For the bevel and the inset, `Alt` gives a free value.
- For the extrude, `Alt` switches to the **alternate grid** and stops following the current grid alignment: an axis aligned grid at the origin, of half the current grid size (the double when the current size is 1, kept between 1 and 1024). With a grid of 16 it snaps to 8, with a grid of 1 to 2. A rotated or custom grid is ignored while `Alt` is held, which makes it the way to align a new brush to the world grid. The label shows `grid 16` or `alt grid 8`.

## MCP

The MCP tool `extrude_face` does the carve and extrude tool from an AI app: `solid`, `side` (the side index of the face, from `get_faces`), the outline `points` as world positions on the face, `mode` (`carve` or `extrude`), `depth`, `through`, and the texture of the new faces (`new_faces`: `face` or `current`, or a `material`). `get_tool_options` and `set_tool_options` read and change the options of the tool itself (tool `extrude`: `mode`, `depth`, `through`, `new_faces`). The MCP tool `extrude_faces` is the interactive extrude: `faces` (default the selected faces), `distance` (negative carves the brush instead of adding one, magnitude at least 0.125), `snap` (align the top planes to the active grid), `alt_grid` (the Alt grid), `material` and `dry_run`. The MCP tool `inset_face` does the inset: `faces` (`[{solid, side}]`, default the selected faces, else the faces of `ids`), `distance`, `depth` (positive raises, negative sinks the inner block), `individual` (every face on its own instead of the shared region outline), `snap` (the active grid, clamped to 1 to 1024), `material` and `dry_run` (answers the number of brushes, or why it fails, without changing the map). The actions `inset_interactive` and `extrude_interactive` (script command `action extrude_interactive`) start the interactive tools. See the [MCP tool reference](../../MCP.md).

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
