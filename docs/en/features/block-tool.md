[Documentation](../README.md) > Features > Block tool | [Français](../../fr/features/block-tool.md)

# Block tool

The block tool creates brushes from primitives (block, wedge, cylinder, spike, sphere, arch, torus, stairs, spiral stairs, ramp, pillar, doorway). You draw a box in a 2D view or directly on a surface in the 3D view, and the primitive is built inside that box with the current material.

## Opening it

- **Tools > Block tool**, the tool palette, or `Shift+B`.
- The options of the tool appear in the options bar above the views.

## Drawing in the 2D views

1. Drag in a 2D view (Top, Front or Side) to draw the footprint of the box. The two corners snap to the grid (hold `Alt` to draw without snapping).
2. The depth of the box (along the axis the view looks at) is taken, like in Hammer, from:
   - the bounds of the current selection on that axis, if it has a thickness,
   - else the depth of the last object created by the block, polygon or curve tool,
   - else 64 units on top of a flat selection (or from 0).
3. The box stays **pending**: it shows its width and height next to its edges, and 8 resize handles (corners and edge middles) in every 2D view.
   - Drag a handle to resize the box (the moved edges snap to the grid).
   - Drag inside the box to move it (its minimum corner snaps to the grid).
   - Drag outside the box to start a new one.
4. Press `Enter`, click **Create** in the options bar, or right click and choose **Create object** to build the primitive. `Escape` cancels the pending box.

The right click menu also offers **Create as cylinder** and **Create as arch**: they build the pending box with that primitive without changing the primitive chosen in the options.

## Drawing in the 3D view

1. Drag on a brush face (or on the ground plane at height 0 when there is no surface under the cursor) to draw the base rectangle on that surface. The base snaps to the grid; if the brush carries its own grid, that grid is used (see [Grids](grids.md)). `Alt`: no snapping.
2. Release the button: the base is done and the box gets one grid step of height. Move the mouse up or down to set the height (snapped to whole grid steps).
3. Click to create the primitive. `Escape` cancels.

Drawn on a wall, the box grows away from the wall; drawn under a ceiling, it grows downward. A small marker shows the snapped surface point under the cursor before you start, and the size `x x y x z` is shown above the box while drawing.

## Primitives

The primitive buttons are at the left of the options bar.

| Primitive | Shape | Solids created |
|---|---|---|
| **Block** | Box filling the bounds. | 1 |
| **Wedge** | Ramp: full bottom, vertical back face at the maximum Y side, slope down to the minimum Y side. | 1 |
| **Cylinder** | Vertical prism with a regular polygon section inscribed in the box. | 1 |
| **Spike** | Pyramid with a regular polygon base, apex at the top center. | 1 |
| **Sphere** | Sphere (ellipsoid) inscribed in the box, one convex solid per stack (number of stacks: half the sides, at least 2). | several |
| **Arch** | Arch or ring of wall segments, one convex solid per segment. If the wall is as thick as the radius, the segments become pie slices. | one per side |
| **Torus** | Ring with a polygonal tube, one convex solid per segment. Can be a spiral with a rise. | one per side |
| **Stairs** | Straight stairs rising toward the chosen **Direction** (+Y by default). The box gives the width, the run and the total rise (Z); the last step reaches the top of the box. | one per step |
| **Spiral stairs** | Steps around the center of the box (the box height is the total rise), each a convex sector between the inner and the outer radius. | one per step |
| **Ramp** | Wedge with a given slope angle, rising toward the chosen **Direction** (+Y by default). When the slope would climb higher than the box, it stops at the top and the rest is flat. | 1 |
| **Pillar** | Column with a regular polygon section, with an optional base and capital that are wider than the shaft. | 1 to 3 |
| **Doorway** | Wall filling the box with a centered opening that goes down to the bottom of the box: two jambs and a lintel; with **Arch** the lintel gets a round top made of straight pieces. | 3, or 2 plus the arch segments |

The stairs, ramps, pillars and doorways are built from non overlapping convex solids with integer vertices when that keeps them valid (the steps then snap to whole units). They use the current material on every face. Cylinders and arches get integer vertices when that keeps the shape valid; spheres and tori are never rounded (their faces would stop being planar). When the parameters cannot make a shape in the box, the 2D views show **Invalid parameters for this size** under the box.

## Options

The parameters shown depend on the primitive. Angles use Hammer's convention: 0 points to +Y and angles increase toward +X.

| Option | Primitives | What it does | Default |
|---|---|---|---|
| **Sides** | cylinder, spike, sphere, arch, torus, pillar | Number of sides of round shapes (3 to 128). | 8 |
| **Wall** | arch | Wall width of the arch (1 to 16384 units). | 16 |
| **Arc** | arch | Angle covered by the arch, 1 to 360 degrees (360 for a full ring). | 360 |
| **Start** | arch | Start angle, -360 to 360 degrees. | 0 |
| **Rise** | arch | Height added to each segment, -4096 to 4096 units (spiral stairs). | 0 |
| **Tube** | torus | Sides of the tube cross section (3 to 128). | 8 |
| **Ring** | torus | Width of the ring, that is the tube diameter (1 to 16384 units). It must stay smaller than the box. | 32 |
| **Arc** | torus | Angle swept by the ring: 1 to 360 degrees, or up to 3600 (ten turns) when **Rise** is not 0. | 360 |
| **Start** | torus | Start angle of the sweep, -360 to 360 degrees. | 0 |
| **Rise** | torus | Height gained over the whole sweep, -16384 to 16384 units (spiral ramps and pipes). | 0 |
| **Steps** | stairs, spiral stairs | Number of steps (1 to 256). The box height is divided evenly. | 8 |
| **Step height** | stairs, spiral stairs | Height of one step in units (0 to 1024). When above 0 it replaces **Steps**: the step count is the box height divided by it, rounded. | 0 |
| **Open** | stairs, spiral stairs | Open steps: slabs two steps thick instead of solid columns down to the bottom of the box. Needed for a spiral above 360 degrees. | off |
| **Inner radius** | spiral stairs | Inner radius of the steps, 0 for pie shaped steps. It must stay smaller than the box. | 16 |
| **Turn** | spiral stairs | Total angle, -3600 to 3600 degrees, negative turns the other way (limited to 360 unless **Open**). Each step must cover less than 180 degrees. | 270 |
| **Start** | spiral stairs | Start angle, -360 to 360 degrees. | 0 |
| **Slope** | ramp | Slope angle, 1 to 89 degrees. | 30 |
| **Direction** | stairs, spiral stairs, ramp | Side the stairs or the ramp rise toward: **+Y**, **-Y**, **+X**, **-X** or **Longest side** (the longer of the X and Y sizes of the box: +X when X is longer, else +Y). A spiral starts at that side, then turns by **Turn**. The sides are those of the shape's own frame, the same as the world axes unless the shape stands along another axis (see **Orient to view**). | +Y |
| **Base** | pillar | Height of the base block, 0 for none. | 16 |
| **Capital** | pillar | Height of the capital block, 0 for none. | 16 |
| **Inset** | pillar | How much narrower the shaft is than the base and the capital (used when there is a base or a capital). | 8 |
| **Opening W** | doorway | Width of the opening, centered. 0 is half the wall width. | 0 |
| **Opening H** | doorway | Height of the opening from the bottom of the box. 0 is three quarters of the wall height. | 0 |
| **Thickness** | doorway | Thickness of the wall, centered in the box depth. 0 fills the whole depth of the box. | 0 |
| **Arch** | doorway | Gives the opening a round top. The top of the opening is at **Opening H**; the arch must fit under it. | off |
| **Arch radius** | doorway | Radius of the round top. At least half the opening; 0 is exactly half (a semicircle). A larger radius gives a flatter, segmental arch. | 0 |
| **Arch segments** | doorway | Number of straight pieces that make the round top (2 to 64). | 8 |
| **Orient to view** | every primitive but the block | Shapes drawn in the Front or Side view (or on a wall in 3D) stand along the depth axis of that view: a cylinder drawn in the Front view lies along X. Saved as the setting `reorient_primitives`. | on |
| **Scale** | all | Texture scale of the new faces (world units per texel, 0.01 to 64). | **Default texture scale** setting (0.25) |
| Create as | all | **World brush**, **func_detail** or any brush entity class of the FGD (searchable list, a typed class name is accepted). With a class, every solid of the primitive goes into one new brush entity with the FGD default keyvalues. World primitives made of several solids are grouped. | World brush |
| Current material | all | Thumbnail and name of the current material used for every face (the default material when none is chosen). Click it to show the texture browser. | `dev/dev_measuregeneric01b` |
| Pin (keep active) | all | Keep the block tool active after creating an object. When off, the selection tool comes back after each creation. | on |
| **Create** | all | Create the pending object (`Enter`). | |

Each face also gets the **Default lightmap scale** of the settings (16).

Each new object becomes the selection, and each creation is one undo step.

### Live shape preview

For round primitives a small thumbnail next to the parameters shows the shape seen from its up axis. Click it to open the large preview window (**Arch preview**, **Torus preview**...): it draws every solid with the parameters next to it, the box size and the number of solids. The window opens by itself when you choose the arch, like Hammer's Arch Properties dialog. With no box drawn, the preview uses the size of the last created object (or 256 x 256 x 64) and says **No box drawn: shown at the last size**. In the views, the wireframe of the primitive is drawn inside the pending box.

## Related settings

| Setting | Where | Default |
|---|---|---|
| **Orient primitives to the drawing view** (`[brush] reorient_primitives`) | Settings > **Brushes** | on |
| **Default material** (`[edit] default_material`) | Settings > **Editing** | `dev/dev_measuregeneric01b` |
| **Default texture scale** (`[edit] default_texture_scale`) | Settings > **Editing** | 0.25 |
| **Default lightmap scale** (`[edit] default_lightmap_scale`) | Settings > **Editing** | 16 |

The Settings window opens with **Tools > Options...** (`Ctrl+,`).

## Tips

- Select a brush before drawing in the Top view: the new box takes the height range of the selection. Select a wall of the room first and the new box gets the full wall height.
- For stairs, draw the box with the width, the run and the total rise you want and set **Steps** or **Step height**. For a spiral staircase, use the spiral stairs primitive (or an arch with **Rise** set to the step height). Choose the side they rise toward with **Direction** (+Y, -Y, +X, -X or the longest side of the box). For a helical pipe, use a torus with **Rise** and an **Arc** above 360.
- Turn **Orient to view** on and draw a cylinder in the Front view to get a horizontal pipe along X without rotating it afterwards.
- The scripted name of each option (for `--script` files and the MCP tool `set_tool_options`): `kind`, `sides`, `wall`, `arc`, `start`, `rise`, `tube_sides`, `ring`, `torus_arc`, `torus_start`, `torus_rise`, `steps`, `step_height`, `open`, `inner`, `spiral_angle`, `spiral_start`, `slope`, `direction`, `base`, `capital`, `inset`, `door_width`, `door_height`, `thickness`, `door_arch`, `door_arch_radius`, `door_arch_segments`, `reorient`, `preview`, `tex_scale`, `keep_active`, `create_as`.

## FAQ

**Why does my box have the wrong height?** In a 2D view the depth comes from the selection, else from the last created object. Clear the selection (`Ctrl+Shift+A`) or resize the box in another 2D view with its handles.

**Why does the view say "Invalid parameters for this size"?** The box is too small for the parameters: for example a torus ring wider than the box, or an arc too large for the number of sides (each segment must cover less than 180 degrees). Change the parameters or enlarge the box.

**How do I make a trigger directly?** Choose `trigger_multiple` (or any brush entity class) in the create as list. The current material is used, so pick the trigger tool texture first.

**Can I turn the tool off after each brush?** Yes, turn off the pin button: the selection tool comes back after each creation.

## Related pages

- [Polygon tool](polygon-tool.md)
- [Curve tool](curve-tool.md)
- [Selection tool](selection-tool.md)
- [Brush operations](brush-operations.md)
- [Vertex tool](vertex-tool.md)
- [Grids](grids.md)
- [Texture browser](texture-browser.md)
- [Settings reference](../reference/settings.md)
- [MCP tool reference](../../MCP.md)

---
[Back to the documentation index](../README.md)
