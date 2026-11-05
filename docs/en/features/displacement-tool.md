[Documentation](../README.md) > Features > Displacement tool | [Français](../../fr/features/displacement-tool.md) | [Русский](../../ru/features/displacement-tool.md)

# Displacement tool

The displacement tool creates, destroys, selects, sculpts and paints displacements (terrain made from brush faces). It covers everything of the Hammer displacement panels (paint geometry, paint alpha, sculpt, tags, sew, subdivide, noise, masks) plus Hammer++ features (4-way blends, continuous painting) and some of its own (live sewing, line of sight, project onto brushes, cast shadows).

## Opening it

- **Tools > Displacement tool**, the tool palette, or `Shift+P`.
- Its options are in the **Displacement** tab of the right panel, which opens by itself when the tool is selected. The panel options are saved in the settings (`mesh.disp_tool`) and come back the next time you use the tool.
- The first line of the panel shows how many displacements the map has.

## Modes

The row of buttons at the top of the panel chooses what clicks and drags do in the views.

| Mode | What it does |
| --- | --- |
| **Select** | Click faces to select them (`Ctrl` toggles, `Shift` toggles every displacement of the solid). Hold and sweep to select every face the mouse passes over |
| **Paint geometry** (default) | Raise, lower, smooth or set the height of the surface with the brush |
| **Paint alpha** | Paint the blend between the textures of a blend material |
| **Sculpt** | Push the surface to a layer, carve a profile along the stroke, project it onto brushes, or pinch, grab, inflate and flatten it |
| **Tags** | Paint the triangle tags: forced walkable, forced buildable or removed |

## Mouse and keys

Painting, sculpting and tagging happen in the 3D view. A brush circle follows the surface under the mouse.

| Gesture | Effect |
| --- | --- |
| Hold the left button over displacements (3D view) | Paints. Painting starts once the mouse moves 3 points or the button is held 0.2 s. One stroke is one undo step |
| Short click (3D view) | Selects the face under the mouse (`Ctrl` adds or removes it) |
| Click (2D views) | Selects the displacements of the solid under the mouse |
| `Shift` while painting (geometry, alpha, tags) | Inverts the effect: Raise lowers, Lower raises, alpha Add removes, Force on becomes Force off |
| `Ctrl` while sculpting | Inverts: Push sinks, Carve raises a ridge |
| `Shift` while sculpting | Smooths instead (strength: **Smoothing**) |
| Right drag (3D view, painting modes) | Changes the brush radius: 150 points to the right doubles it, to the left halves it |
| `Alt` + left drag (3D view, painting modes) | Same radius change (Hammer gesture) |
| `Alt` + right click on a face (3D view) | Sets the **Custom** axis to the normal of the surface there and switches the axis to **Custom** |
| `Esc` | Cancels the current stroke (its changes are reverted); otherwise clears the face selection |

The brush circle changes color with the effect, and turns gray with the label "Not selected: not painted" over displacements it may not paint (see **Selected only**). The status bar always shows the gestures of the current mode.

## Edge loops of the grid

`Alt`+click a grid line of a displacement (in the 3D view or a 2D view) to select its row or column, the line of vertices that goes on straight through the grid. The loop continues across the sewn neighbouring displacements (up to 1024 connected ones), through the places where four displacements meet, and across displacements of other powers; it ends at the border of the terrain, and is closed when it comes back to its start. The loop is drawn in blue with a dot on each vertex and the status bar counts its vertices and the displacements it crosses. `Esc` clears it. In the paint modes of the 3D view a short `Alt`+click selects the loop and `Alt`+drag still changes the brush radius.

Section **Edge loop** of the panel (and the command palette and MCP):

| Option or button | What it does | Default or range |
| --- | --- | --- |
| **Strength**, **Passes** | How far, per pass, **Smooth displacement loop** moves each vertex toward the average of its two neighbours on the loop | 0.5 and 1 pass (0 to 1, 1 to 64) |
| **Smooth displacement loop** | Smooths the loop along its own line, along the face normal: the vertices at the ends of an open loop stay, the sewn neighbours follow and stay sewn, one undo step | |
| **Height**, **Set height of displacement loop** | Puts every vertex of the loop at that height above the face, like **Raise to** on the loop only (-16384 to 16384), one undo step | 0 |
| **Clear loop** | Forgets the loop (`Esc`) | |

MCP: `select_displacement_loop` (face, row and col of a vertex, direction `row` or `column`) answers the vertices of the loop, `smooth_displacement_loop` and `set_displacement_loop_height` change it, and the options `loop_height`, `loop_strength` and `loop_passes` of the displacement tool are listed by `get_tool_options`. The actions are `disp_smooth_loop` and `disp_loop_height`.

## Create

Section **Create** of the panel:

- **Power** buttons **2**, **3** and **4** turn the targets into displacements of that power: 5 x 5, 9 x 9 or 17 x 17 vertices.
- Targets: the selected faces, or, when no face is selected, the faces of the selected solids that face up (normal Z above 0.7). A line tells you which ("Applies to N selected face(s)" or "Applies to N top face(s) of the selected solids").
- Only four sided faces can become displacements; other faces are skipped with a message.

Faces selected with the [Texture tool](texture-tool.md) can also be turned into displacements from there.

## Selection

Section **Selection**: shows "N displacement(s) among M selected face(s)".

| Button or option | What it does |
| --- | --- |
| **Select all displacements** | Selects every displacement face of the map |
| **Clear** | Deselects all faces (`Esc`) |
| **Select adjacent** | Adds the displacements sharing an edge with the selected ones |
| **From solids** | Selects every displacement of the selected solids |
| **Masks: Grid** | Hides the triangle grid of the selected displacements (Hammer's Grid mask). Off by default |
| **Masks: Select** | Does not tint the selected displacements (Hammer's Select mask). Off by default |
| **Destroy** | Turns the selected displacements (or those of the selected solids) back into flat faces |
| **Sew** | Closes the cracks between the selected displacements (at least two): shared edges and corners are made identical, edges between different powers follow the coarser one |

When displacements are selected, per face options appear:

| Option | What it does | Default or range |
| --- | --- | --- |
| **Power** | Changes the power (2, 3 or 4) of the selected displacements. With **Keep the shape** on, the sculpting is kept (see below); off, they are rebuilt flat and the sculpting is lost | Current power, or "(multiple values)" |
| **Keep the shape** (next to **Power**) | Resamples the existing grid to the new power instead of rebuilding flat: offsets, distances, normals, alpha, 4-way blend rows and triangle tags. Going up interpolates (bicubic for heights and offsets, bilinear for the rest), going down keeps the vertices that stay on the coarser grid. Flags and allowed vertices are kept, and the selected displacements are sewn afterwards, all in one undo step (`mesh.disp_tool` option `keep_shape`) | On |
| **Elevation** | Offset of the whole surface along the face normal | -4096 to 4096 |
| **Lightmap scale** | Units per luxel of the selected displacements (lower: finer lighting, bigger map) | 1 to 1024, 16 shown for a mixed selection |
| **Flatten** | Resets the selected displacements to flat, keeping their power | |

Every selected displacement shows the selection tint and its triangle grid in the views (the masks hide either). Triangles tagged removed are tinted in the 3D view.

## Paint geometry

Section **Paint geometry** (mode **Paint geometry**):

| Option | What it does | Default |
| --- | --- | --- |
| **Raise** | Pushes the surface out along the paint axis | Selected |
| **Lower** | Pulls the surface in along the paint axis | |
| **Smooth** | Evens out the heights of neighbouring vertices along the paint axis (connected displacements are smoothed together and stay sewn) | |
| **Raise to** | Brings the surface to a target height above the face | |
| **Strength** (Raise, Lower) | Units moved per second at the brush center (1 to 8192) | 64 /s |
| **Strength** (Smooth) | How fast the surface is smoothed (0 to 1) | 0.5 |
| **Target height** (Raise to) | Height above the face along the paint axis (-16384 to 16384) | 0 |
| **Axis** | Direction the vertices move in (see below) | Face normal |

### Paint axis

| Axis | Direction |
| --- | --- |
| **Face normal** | The normal of the base face (default) |
| **Surface normal** | The normal of the surface at each vertex (geometry painting only) |
| **Brush center** | The surface normal under the brush center (Hammer's Brush Center) |
| **View (camera)** | Toward the 3D camera, along the view direction at the start of the stroke (Hammer's Screen direction). Inverted: away from it |
| **View (camera), horizontal** | Toward the 3D camera on X and Y only: the view direction at the start of the stroke, flattened onto the ground, so vertices never go up or down. Handy to push a cliff face or a wall back and forth from where you look. Looking straight down, the bottom of the view gives the direction. Inverted: away from the camera |
| **X**, **Y**, **Z (up)** | A world axis |
| **Custom** | A direction of your own: type X, Y, Z under the list, or `Alt` + right click a face in the 3D view to take its normal (Hammer's Selected direction). Default 0 0 1 |

## Paint alpha

Section **Paint alpha** (mode **Paint alpha**) paints the blend of blend materials (WorldVertexTransition: alpha 0 shows the first texture, 255 the second).

| Option | What it does | Default |
| --- | --- | --- |
| **Texture: 2-way** | Paints the alpha between the two textures | Selected |
| **Texture: 1**, **2**, **3**, **4** | 4-way blend materials (Lightmapped_4WayBlend): paints that texture, the others fade in proportion (`Shift` or **Remove** lowers it). Colored dots show the weights under the brush | |
| **Add** | Reveals the second texture | Selected |
| **Remove** | Reveals the first texture | |
| **Set value** | Blends toward a fixed alpha value | |
| **Smooth** | Evens out the blend of neighbouring vertices (connected displacements are smoothed together) | |
| **Strength** (Add, Remove) | Alpha units painted per second at the brush center (1 to 16384) | 255 /s |
| **Value** (Set value) | Target alpha, 0 to 255 | 255 |
| **Strength** (Smooth) | How fast the blend evens out (0 to 1) | 0.5 |

The **Texture** row only shows when **Paint 4-way blends** is on in the settings (on by default, like Hammer++ for Garry's Mod).

### 4-way blends (Lightmapped_4WayBlend)

- **Where the blend is stored.** The 4 texture buttons write the `multiblend`, `alphablend` and `multiblend_color_0` to `multiblend_color_3` rows of the `dispinfo`, like Hammer++ and CS:GO Hammer. A map made in Hammer++ opens with its blends, and saving keeps the rows as they were read until you paint them.
- **What you see.** The textured, lit and baked views and the player view draw the 4 textures like the game: texture 2, then 3, then 4 are laid over texture 1, each by its own value of the rows. The VMT parameters shape the edges: `$textureN_lumstart` and `$textureN_lumend` (luminance of each texture), `$lumblendfactorN`, `$textureN_blendstart` and `$textureN_blendend` (sharpness), `$textureN_uvscale` (the scale of each texture) and `$textureN_blendmode` (1: modulate 2x).
- **No rows yet.** A displacement with a Lightmapped_4WayBlend material and no 4-way rows shows its vertex alpha as texture 2 (the **2-way** painting), and the first stroke on a texture button starts from that alpha. The compiler writes the same thing, so the game matches the editor. Valve's VBSP ignores the alpha of such a displacement (the game then shows texture 1 only).
- **In the game.** The compiler writes the rows to the displacement multiblend lump of the BSP and flags each displacement that has them, byte for byte like Garry's Mod's VBSP, so the blends show in Garry's Mod and Source 2013 games.
- **Moving and mirroring** a displacement keeps its 4-way rows in place, like its alpha. Flattening starts over from texture 1; changing the power with **Keep the shape** resamples the rows.

## Sculpt

Section **Sculpt** (mode **Sculpt**), Hammer's Sculpt panel:

- **Push**: raises the surface to a layer at an offset from where it was when the stroke started (`Ctrl` sinks it). Going over the same place again does not pile up.
- **Carve**: the stroke is a path, the surface along it sinks following a cross profile: ditches, rivers, roads (`Ctrl` raises a ridge).
- **Project**: moves the surface onto the selected brushes along the sculpt axis (a road or a foundation the terrain should follow). Select the brushes together with the displacements; if no brush is selected the tool tells you.
- **Pinch**: pulls the vertices under the brush toward the stroke center, along the surface plane of the sculpt axis (`Ctrl` pushes them away). Speed: **Strength**, in units per second.
- **Grab**: takes the vertices under the brush when the stroke starts and drags them along the mouse movement, weighted by the falloff, for the whole stroke even if the mouse leaves the brush (`Ctrl` drags the opposite way). The drag follows the screen like Blender's grab: the vertices move in the plane facing the camera at the depth of the point you grabbed, so moving the mouse up lifts the terrain and moving it sideways drags it sideways, whatever the surface under the cursor. It also works when the mouse leaves the terrain.
- **Inflate**: moves every vertex along its own surface normal, proportional to the falloff (`Ctrl` deflates). Speed: **Strength**, in units per second.
- **Flatten**: moves the vertices toward the average plane of the vertices under the brush, a real plane fitted to them, not a height (`Ctrl` moves them away from the plane).

All four use the brush radius and falloff, count as one undo step per stroke, and follow **Sew while painting** like the other sculpt brushes.

| Option | Applies to | What it does | Default |
| --- | --- | --- | --- |
| **Bounds limit: Attenuated** | Push, Carve | Each vertex moves to its offset once per stroke | Selected |
| **Bounds limit: Additive** | Push, Carve | The offset keeps adding while the brush stays over the surface | |
| **Falloff position** | Push | Share of the radius, from the center, that gets the whole offset before the falloff starts (0 to 100 %) | 0 % |
| **Falloff final** | Push | Share of the offset the falloff never goes below inside the brush (0 to 100 %) | 0 % |
| **Offset: Absolute** | Push | The offset is a distance (**Distance**, 0 to 4096 units) | Selected, 32 |
| **Offset: Adaptive** | Push | The offset is a percentage of the brush radius (**Percent**, 0 to 400 %) | 25 % |
| **Depth** | Carve | Depth at the deepest point of the profile (0 to 4096) | 32 |
| **Profile** | Carve | Five sliders: depth across the path at 0, 25, 50, 75 and 100 % of the radius. Presets **V**, **U** and **\|_\|** | U |
| **Strength** | Pinch, Inflate | Units moved per second at the brush center (1 to 8192) | 64 /s |
| **Strength** | Flatten | Share of the distance to the plane closed at each step (0 to 1) | 0.5 |
| **Plane: Average** | Flatten | The plane is fitted again to the vertices under the brush at every step | Selected |
| **Plane: Fixed** | Flatten | The plane is taken once, at the stroke start, and the whole stroke flattens to it | |
| **Range** | Project | Brushes farther than this from the surface are ignored (1 to 16384) | 512 |
| **Axis** | All | Sculpt direction (same list as painting, without Surface normal) | Face normal |
| **Smoothing** | All | Strength of the smoothing done while `Shift` is held (0 to 1) | 0.5 |

## Tags

Section **Paint tags** (mode **Tags**), Hammer's Tag Walkable, Tag Buildable and Tag Remove. Tags are written to the map and used by the compiler.

| Option | What it does | Default |
| --- | --- | --- |
| **Walkable** | Whether players can walk on the triangles (normally from their slope) | Selected |
| **Buildable** | Whether objects can be built on the triangles (normally from their slope) | |
| **Remove** | Marks triangles removed (written to the map; the Garry's Mod engine ignores it) | |
| **Force on** / **Force off** | The tag is on (or off) whatever the slope; `Shift` paints the opposite | Force on |
| **From slope** | The tag follows the slope again | |
| **Remove** / **Restore** (with the Remove tag) | Marks the triangles removed, or clears the mark; `Shift` paints the opposite | |
| **Show walkability** | Colors displacement triangles by walkability (same setting as **View > Display options > Displacement walkability**) | Off |
| **Apply to selection** | Gives the chosen tag to every triangle of the selected displacements | |

## Brush

Section **Brush**, shown in every mode except **Select**:

| Option | What it does | Default |
| --- | --- | --- |
| **Radius** | Brush radius in world units (slider 1 to 2048; right drag or `Alt` drag go up to 8192) | 64 |
| **Falloff** | **Constant**, **Linear** or **Smooth** | Smooth |
| **Spatial** | Measures the brush distance in 3D on the surface instead of in the face plane (paints walls and across tiles). Off: only the displacement under the brush and the terrain tiles connected to it are painted | Off |
| **Line of sight** | Only paints the displacements the camera sees at the brush (not the terrain hidden behind a hill or under a bridge) | Off |
| **Sew while painting** | Neighbouring displacements follow the brush live, shared edges and corners move together, and the remaining cracks are closed when the stroke ends (same undo step). With displacements selected, only those change: their edges stay on the unselected ones | Off |
| **Selected only** | With displacements selected, only those are painted (Hammer). Off: every displacement under the brush | On |
| **Continuous** | Holding the button still keeps painting (Hammer++). Off: paints only while the mouse moves | On |

## Tools

Section **Tools**, at the bottom of the panel. These commands work on the selected displacements, or on every displacement of the selected solids. Each one is one undo step.

| Option | What it does | Default |
| --- | --- | --- |
| **No physics collision** | Physics objects (props, ragdolls) go through these displacements | Off |
| **No hull collision** | Players and NPCs walk through these displacements | Off |
| **No ray collision** | Bullets and traces go through these displacements | Off |
| **Cast shadows** | The displacements cast shadows when the map is lit. Off: they are still lit but light goes through them (Hammer Pro compiler) | On |
| **Scale** + **Apply** | Multiplies the heights by this factor (-16 to 16; a negative factor turns them over) | 2 |
| **Invert alpha** | Swaps the two textures of the blend: every alpha value a becomes 255 - a | |
| **Subdivide** | Smooths the displacements as one subdivision surface (Catmull-Clark), then sews them. The outer borders stay in place | |
| **Minimum** / **Maximum** | Range of the random heights added by **Add noise** (saved in `mesh.noise_min` and `mesh.noise_max`) | -4 / 4 |
| **Noise: Random** / **Fractal** | Kind of noise **Add noise** adds (see [Noise and erosion](#noise-and-erosion)) | Random |
| **Add noise** | Adds a height between the minimum and the maximum to every vertex, along the face normal: a random one, or a fractal Perlin one | |
| **Thermal erosion**, **Hydraulic erosion** | Wear the terrain down like weather; their options are under **Thermal** and **Hydraulic** (see [Noise and erosion](#noise-and-erosion)) | |
| **To mesh...** | Saves the displacements as an OBJ file, see [Displacement to mesh](#displacement-to-mesh) | |
| **Mesh to displacement...**, **Conform to mesh...**, **Tile selection...** | Build or shape displacements from a 3D mesh, see [Mesh to displacement](#mesh-to-displacement) | |
| **Meters** / **Z up** | Units and up axis of that file (saved in `mesh.mesh_export_meters` and `mesh.mesh_export_z_up`) | Off / Off |

A checkbox shows a mixed state when the selection disagrees; clicking it turns the flag on for all.

Whole terrains come from the [Terrain](terrain.md) page: **Tools > Import heightmap...**, **Generate terrain...** and **Export heightmap...** build, replace and export grids of sewn displacements.

The same commands are in the **Tools** menu and work whatever the active tool: **Subdivide displacements**, **Invert displacement alpha** and **Add noise to displacements...** (a small **Displacement noise** window with the **Random** or **Fractal** choice, the range, the fractal options and an **Apply** button), plus **Fractal noise on displacements...** (the same window with **Fractal** chosen), **Thermal erosion of displacements...** and **Hydraulic erosion of displacements...** (small windows with the options of that erosion and an **Apply** button). The windows use the same options as the **Tools** section of the panel: a value changed in one is changed in the other. None has a default shortcut, and all are in the command palette and in the MCP tool `run_action` (`disp_noise`, `disp_fractal_noise`, `disp_thermal_erosion`, `disp_hydraulic_erosion`).

### Displacement to mesh

Saves the selected displacements (or every displacement of the selected solids) as a mesh you can edit in Blender or any 3D tool. Use the **To mesh...** button of the **Tools** section, **Tools > Displacement to mesh...**, or the MCP tool `displacement_to_mesh`. It is available when at least one displacement is selected.

- A file dialog asks where to write the `.obj` (the suggested name is `<map>_displacements.obj`, next to the map). The `.mtl` is written beside it.
- The file holds the displaced vertices, their normals, UVs computed from the texture mapping of each face, and one material per texture. Tool textures are left out. It is written by the same code as **File > Export OBJ**.
- **Meters** writes meters (1 unit = 0.01905 m) instead of Source units. **Z up** keeps the editor axes; off, the file is Y up (the Blender OBJ importer default).
- Nothing is changed in the map and no undo step is made. File > Export OBJ and Export again are not affected.
- MCP: `displacement_to_mesh` takes `faces`, `ids` or `solid_ids` (default: the selected displacements), `path` (full path or relative to the map folder; default `<map>_displacements.obj`), `units` (`source` or `meters`), `up` (`y` or `z`) and `overwrite`.

### Mesh to displacement

Turns a 3D mesh into displacements: Hammer Pro lays a grid of displacement tiles over the mesh, measures the height of the mesh at every displacement vertex with a ray, and sets the vertices to that height. Use **Tools > Mesh to displacement...**, the **Mesh to displacement...** button of the **Tools** section, the command palette, or the MCP tool `mesh_to_displacement`.

The mesh can be:

- a file: OBJ, FBX, glTF or GLB (the same readers as [Model import](model-import.md)), chosen with a file dialog;
- a model prop of the map: select a `prop_static` or `prop_physics` and click **Use the selected prop**. Its position, angles and scale are applied;
- a game model: type a path like `models/props_c17/oildrum001.mdl` and click **Load .mdl**. It is placed at the origin.

The **Mesh to displacement** window shows the options and, in the 3D view and the 2D views, a live preview of the layout: cyan tile outlines on the base plane, yellow lines on the measured surface, green lines for the skirts and the bottom. Below the options it says how many tiles, which power and what vertex spacing the result has, and warns about overhangs. **Create the tiles** makes everything in one undo step (**Mesh to displacement**); the new brushes are grouped and selected.

| Option | What it does | Default |
| --- | --- | --- |
| **Projection** | Direction of the rays: **Z (top down)**, **X**, **Y**, or **Best fit**, which takes the axis the mesh faces the most. **From the other side** projects against the axis (a ceiling, or a wall seen from the back) | Z |
| **Tile size** | Width of a tile, 16 to 4096 units. Tiles sit on a grid of multiples of this size | 256 |
| **Power** | 2, 3 or 4, or **Auto**: the smallest power for which a vertex is about as close as the triangle edges of the mesh | Auto |
| **Surface** | **Top**: the highest surface under the ray. **First hit**: the highest surface that faces the ray, which ignores the back and inside of the mesh | Top |
| **Holes** | A vertex of a kept tile that the mesh does not cover takes the **Nearest height** (the border of the mesh is extended) or the **Lowest height** | Nearest height |
| **Margin tiles** | Extra tiles kept around the tiles that touch the mesh, 0 to 8. Tiles that the mesh barely touches (only its edge, or a thin sliver) are skipped | 0 |
| **Smoothing** | Iterations that average every height with its neighbours, 0 to 64 | 0 |
| **Scale** | Scale of the mesh after the unit conversion, 0.001 to 1000 | 1 |
| **Placement** | **Center**: the middle of the mesh and its lowest point go to **Position**. **Min corner**: the lowest corner goes to **Position**. **Keep position**: the mesh stays where the file puts it. **At the selection** sets the position to the center of the selection | Center (Keep position for a prop or a game model) |
| **Texture**, **Texture scale** | Material of the displacements (**Current** takes the texture picked in the texture browser) and its scale. The texture is continuous from one tile to the next | `DEV/DEV_MEASUREGENERIC01B`, 0.25 |
| **Brush thickness** | Thickness of the brush under each displacement | 16 |
| **Skirts on the border** | Closes the border of the tiles with walls that go down to the base height: one displacement per border edge, whose top follows the tile edge | Off |
| **Flat bottom brush** | A flat brush under the tiles at the base height (one brush per row of tiles) | Off |
| **Base height** | Height of the skirt bottoms and of the flat brush. Off: one thickness under the lowest point | Off |
| **Group the new brushes** | Puts every new brush in one group | On |
| **Up axis of the file**, **Convert the units** | For a file: Y up files are turned so that Z is up, and meters become Source units (1 m = 39.37 units) | From the file, On |

How the heights are made:

- Every tile uses the same power and the tiles share one lattice of samples, so the vertices on a shared edge have exactly the same height: the seams are closed without sewing. All tile faces lie on one plane, at the lowest height of the surface, so neighbouring tiles are real neighbours for the compiler.
- Each vertex is set with the displacement distance along the face normal. Skirts use the displacement offsets instead (their top row is the tile edge, their bottom row is flat at the base height).
- An overhang (a surface that faces up above another one, like a roof over a floor, further apart than two vertex spacings) keeps the top surface, and a warning gives the number of vertices. A solid mesh is not an overhang: its underside faces down.
- More than 2048 displacements triggers a warning: the engine accepts about that many in one map. More than 4096 tiles are refused: use a bigger tile size.

#### Conform to mesh

**Tools > Conform displacements to mesh...** (or **Conform to mesh...** in the **Tools** section) projects existing displacements onto a mesh. Place flat displacement tiles by hand first (or with **Tile selection**), select them, then choose a file, a prop or a game model. For every vertex, a ray along the axis finds the mesh surface and the displacement distance (or the offset, when the vertex does not move along its normal) is set so that the vertex lies on it. A box in the views shows where the mesh is.

| Option | What it does | Default |
| --- | --- | --- |
| **Projection**, **From the other side** | Axis of the rays. **Best fit** follows the normal of the selected displacements | Z |
| **Surface** | **Top** or **First hit**, as above | Top |
| **Scale**, **Placement**, **Position** | Where the mesh is. For a mesh made in the map coordinates (a prop, or a file exported from the map) use **Keep position** | Keep position |
| **Strength** | 0 keeps the current shape, 1 puts the vertices exactly on the mesh, in between blends | 1 |
| **Height offset** | Units added to the mesh height along the axis | 0 |

Vertices that the mesh does not cover stay as they are, and the toast counts them. One undo step (**Conform displacements to mesh**). Neighbouring displacements get the same height on their shared edge, so they stay sewn.

#### Tile selection

**Tools > Tile selection with displacements...** (or **Tile selection...** in the **Tools** section) converts the selected brushes, or the selected faces, into flat displacement tiles: draw the area of the terrain with ordinary blocks, convert it, then use **Conform to mesh**. Each face that is flat and square to the axis (the top face for Z) is cut into equal tiles of at most **Tile size** units, so a 512 by 300 face with a tile size of 256 gives 2 by 2 tiles; faces that are slanted or face another way are skipped and counted. **Delete the source brushes** (on by default) removes the brushes that were tiled; Undo brings them back. The options are the axis, tile size, power (**Auto** is 3), texture, texture scale, thickness and grouping. One undo step (**Tile displacements from selection**).

MCP: `mesh_to_displacement` (`path`, `prop` or `model`, `up`, `convert_units`, `axis`, `flip`, `tile_size`, `power`, `pick`, `fill`, `margin`, `smooth`, `scale`, `origin`, `position`, `material`, `uv_scale`, `skirt`, `bottom`, `base_height`, `thickness`, `group`, `dry_run`), `conform_displacements_to_mesh` (the same mesh arguments, `faces`, `ids` or `solid_ids`, `axis`, `flip`, `pick`, `scale`, `origin`, `position`, `strength`, `offset`) and `tile_displacements_from_selection` (`ids` or `solid_ids`, `axis`, `flip`, `tile_size`, `power`, `material`, `uv_scale`, `thickness`, `delete_source`, `group`). `dry_run` returns the layout (tiles, power, spacing, plane, warnings) without changing the map. A relative `path` is relative to the map folder.

### Sculpt in Blender

**Tools > Send to Blender** sends the selected displacements to Blender, one object per displacement, so you can sculpt them with Blender's brushes and modifiers; the result is written back into the same displacements (alpha, flags, tags, lightmap scale and untouched blend data kept, seams sewn, one undo step). See [Blender link](blender.md).

### Noise and erosion

These commands work on the selected displacements, or those of the selected solids, one undo step each, and the displacements are sewn afterwards. Heights are measured along the face normal. The values are saved with the panel options.

| Command | What it does |
| --- | --- |
| **Add noise**, **Random** | A random height per vertex between **Minimum** and **Maximum**. A different result each time |
| **Add noise**, **Fractal** | Smooth Perlin noise, summed over several octaves, scaled to the range **Minimum** to **Maximum**. It depends on the position in the map only, so neighbouring displacements match without sewing. The same **Seed** gives the same result |
| **Thermal erosion** | Material slides down the slopes steeper than the talus angle, pass after pass, until the terrain holds. The total height is kept |
| **Hydraulic erosion** | Rain falls on every vertex and runs downhill, digging where it runs fast and dropping its sediment where it slows down. The total height is kept |

Connected displacements (sewn, sharing edges) are eroded as one height field, so the erosion crosses their borders and they stay sewn. Displacements with no neighbour among the targets are eroded on their own. Neighbours that are not selected are not changed, and the sewing closes the cracks afterwards.

| Option | Applies to | What it does | Default or range |
| --- | --- | --- | --- |
| **Scale** | Fractal | Size of the biggest features, in units | 256 (1 to 65536) |
| **Octaves** | Fractal | Layers of detail, each twice finer than the previous | 4 (1 to 8) |
| **Persistence** | Fractal | How much each finer layer counts compared to the previous | 0.5 (0.05 to 1) |
| **Seed** | Fractal, Hydraulic | Same seed, same result (the rain of the hydraulic erosion is random but repeatable) | 1 |
| **Iterations** | Thermal, Hydraulic | Number of passes | 20 (1 to 1000) and 50 (1 to 2000) |
| **Talus angle** | Thermal | Steepest slope in degrees that material stays on | 35 (0 to 89) |
| **Strength** (Thermal) | Thermal | Share of the excess that slides down each pass | 0.5 (0 to 1) |
| **Rain** | Hydraulic | Water added to every vertex each pass | 1 (0 to 100) |
| **Evaporation** | Hydraulic | Share of the water that evaporates each pass | 0.1 (0 to 1) |
| **Capacity** | Hydraulic | Sediment the water carries per unit of water and slope | 1 (0 to 10) |
| **Deposition** | Hydraulic | Share of the sediment above the capacity that settles each pass | 0.3 (0 to 1) |
| **Strength** (Hydraulic) | Hydraulic | Share of the missing capacity dissolved from the ground each pass | 0.3 (0 to 1) |

Tips: a few passes of thermal erosion soften sharp noise into believable slopes; hydraulic erosion carves gullies on steep ground, so give it relief first (noise or sculpting).

### Height blending

Merges the heights of two or more sets of displacements across a transition zone, for terrain pieces that overlap or touch: a hill and a plain, two tiles built at different elevations, a road bed and the ground. Use the **Height blending** section of the **Tools** panel, **Tools > Blend displacement heights**, the command palette, or the MCP tool `blend_displacements`. Everything is one undo step (**Blend displacements**) and the displacements are sewn afterwards.

Sets:

- **Set A = selection** and **Set B = selection** remember the selected displacements as set A and set B. A set can hold displacements from several brushes or groups.
- Without both sets, the selected displacements are split by group (the top level group of each solid, or its brush entity); if they all share one group, by solid. The panel says how many sets it found.
- With more than two sets (select several groups), every set blends with all the others.

| Option | What it does | Default |
| --- | --- | --- |
| **Width** | Width of the transition zone in units, 0.01 to 65536. A vertex of a set changes when it is closer than this to another set (or inside it) | 128 |
| **Curve** | How the blend grows from the edge of the zone to the meeting line: **Linear**, **Smooth**, **Ease in** (slow start) or **Ease out** (slow end) | Smooth |
| **Mode** | **Average**: both sides meet in the middle. **A over B**: A does not move and B follows it with a soft edge (with several sets, a set follows the ones before it). **B over A**: the reverse. **Maximum** or **Minimum**: the higher or lower surface wins | Average |
| **Match edge heights** | Shifts each side by the height gap measured at the meeting line instead of copying the other side, so each side keeps its own relief and two pieces at different elevations meet without a step | Off |
| **Blend texture alpha** | Blends the vertex alpha, and the 4-way blend rows, across the same zone with the same weights | On |
| **Match powers first** | Resamples the lower power displacements of the sets to the highest power (**Keep the shape**), so the blend keeps all the detail. Off: each displacement keeps its own power | Off |
| **Show the zone in the 3D view** | Draws the vertices the blend will move, bigger and brighter where it acts more | On |

How it works: inside the zone a vertex goes from its own height toward the other set's height by the curve, from nothing at the edge of the zone to the full blend at the meeting line (and everywhere inside an overlap). On a shared edge both sides get the same height, so the pieces meet exactly. The height of the other set is read from its triangles, so different powers and unaligned grids blend correctly. Displacements of different powers that touch are sewn afterwards (the edge follows the coarser one). Neighbours outside the sets are not changed.

### Slope control

Measures, limits and shapes the slopes of the selected displacements. A slope is the angle of a triangle from flat: 0 degrees is horizontal, 90 degrees is a wall. Heights move along Z, and sewn displacements are handled as one surface. Each command is one undo step and the result is sewn. The **Slope control** section of the **Tools** panel has all of it; the Tools menu has **Limit displacement slope**, **Make displacement slope uniform**, **Blend displacement heights** and **Select displacements by slope** (they use the panel options and open the displacement tool); MCP: `control_slope` and `select_displacements_by_slope`.

#### Slope colors

**Show slopes in the 3D view** is the **Displacement walkability** display (**View > Display options**), with a mode and an angle:

| Mode | Colors |
| --- | --- |
| **Source** | The three engine classes: walkable and buildable, walkable, not walkable (default). Forced tags win |
| **Limit** | Green up to the angle, red above it. Tags are ignored: only the geometry counts |
| **Gradient** | Eight bands around the limit: green and lime far below, yellow, amber up to the limit, then orange, red and dark red above it, and violet for overhangs (surfaces that face down). Tags are ignored |

**Walkable angle** is the limit of the Limit and Gradient modes, 1 to 89 degrees, **45.57** by default: the Source walkable limit (a triangle normal with Z of 0.7). The **45.57** button puts it back. The 3D view draws a legend with the angles of each color. The same options are in the Settings, page **Rendering** (`render.disp_walk_mode` and `render.disp_walk_angle`).

#### Measure

**Measure** shows the smallest, largest and mean slope of the selected displacements, and the plane that fits them best (its angle and the direction it climbs toward). **Take measured** copies that plane into the ramp options.

#### Limit steepness

Relaxes the vertices until no triangle is steeper than **Angle** (1 to 89 degrees, 45 by default). The mean height is kept, so steep spots come down and the rest goes up a little. The result is guaranteed: no triangle is above the angle, whatever the starting shape. **Terraces** first cuts the heights into flat steps of **Step height** units (**Terrace strength** from 0 to 1 sets how much the heights snap), then limits the risers between them to the angle: flat treads and sloped risers.

#### Exact ramp

Turns the selection into a plane of exactly **Angle** degrees climbing toward **Direction** (degrees in the map plane: 0 is +X, 90 is +Y). **Keep** chooses what stays in place: the mean height, the lowest point or the highest point. **Strength** (0 to 1) is how far to go toward the plane, and **Keep relief** (0 to 1) keeps that share of the current bumps on top of the plane (0 is a perfect plane). Sewn displacements form one plane, so a patch of tiles becomes one ramp. A ramp of 0 degrees is a flat floor.

#### Uniform slope

Makes the slope constant over the whole selection: the best fit plane is measured and the surface is brought to it, keeping its mean angle and direction (**Strength** and **Keep relief** work as for the ramp).

#### Constant slope along a path

Makes the ground near a path climb at a constant grade, from the height of the first point to the height of the last: roads, ramps, rails. Give the path with **Pick points in the 3D view** (while it is on, a click on a displacement adds a point instead of selecting; the path is drawn in yellow), **From selected entities** (the origins of the selected entities in selection order, for example `path_corner` or `info_target`) or the MCP `path` argument. The surface within **Half width** units of the path is brought to the grade, fading out toward the edge by the **Curve** (**Strength** from 0 to 1). The heights of the first and last points set the grade, so pick them on the ground or move the entities first. **Clear path** empties it.

#### Smooth slopes

Evens out the slope changes of the selection by averaging each vertex with its neighbours: **Iterations** passes (1 to 1000) of **Strength** (0 to 1). Sewn displacements are smoothed together.

#### Select by slope

Selects the displacements that have triangles **Above** or **Below** an angle (0 to 90 degrees from flat, 45.57 by default). **Any triangle** counts a displacement when one of its triangles matches, **All triangles** only when every triangle does. **Selection** replaces, adds to or removes from the current selection. The candidates are the selected displacements, or all the displacements of the map when none is selected, so the command can also refine a selection. A steep selection followed by **Limit steepness** or **Make ramp** fixes only those pieces.

MCP: `blend_displacements` (`sets` as lists of solid ids, or `faces`, `ids`, `solid_ids`, `width`, `falloff`, `mode`, `match_edges`, `alpha`, `match_powers`), `control_slope` (`operation` `limit`, `ramp`, `uniform`, `path`, `smooth`, `measure` or `display`, the targets, `options` with the `slope_*` names, `path`; `display` takes `show`, `mode` and `walk_angle`) and `select_displacements_by_slope` (`angle`, `mode`, `match`, `op` and the targets). The answers give the slope range before and after.

## Display options

| Where | Option | What it does | Default |
| --- | --- | --- | --- |
| **View > Display options** | **Show displacements** | Shows or hides displacements | On |
| **View > Display options** | **Displacement mesh in 2D views** | Switches 2D views between the full triangle mesh and the outline only | Full mesh |
| **View > Display options** | **Displacement walkability** | Colors triangles: walkable and buildable (slope normal Z 0.8 or more), walkable (0.7 or more), not walkable. Forced tags win, like in the compiled map. A legend is drawn in the 3D view (and in a maximized 2D view showing the mesh). The mode (Source, Limit angle or Angle gradient) and the walkable angle are in the Settings and in the [Slope control](#slope-control) section | Off |
| Menu of a 2D view, **Displacements** | **Like the other views**, **Full mesh**, **Mesh of the selection**, **Outline only**, **Source brushes** | Mode of that view only; **Source brushes** draws the brush each displacement was built from as a faint outline (every 2D view) | Like the other views |
| Settings, page **Rendering** | **Displacements**, **Displacements in 2D**, **Displacements only**, **Displacements in 2D views** (mode, with **Use this mode in every 2D view**), **Source brushes in 2D views**, **Displacement walkability** | Same options, globally. **Displacements only** hides every brush that is not a displacement | On, On, Off, Full mesh, On, Off |

## Settings

**Tools > Options...** (`Ctrl+,`), page **Vertices and displacements**, section **Displacements**:

| Setting | What it does | Default |
| --- | --- | --- |
| **Paint 4-way blends** (`mesh.four_way_blends`) | Alpha painting can paint each of the 4 textures of a 4-way blend material, like Hammer++ | On |
| **Minimum noise height** (`mesh.noise_min`) | Lowest height added by the noise | -4 |
| **Maximum noise height** (`mesh.noise_max`) | Highest height added by the noise | 4 |

## Scripting and MCP

Every option of the panel has a name for scripts (`tool_option KEY VALUE`, see [Script commands](../reference/script-commands.md)) and for the MCP tools `get_tool_options` and `set_tool_options`: `mode`, `effect`, `axis`, `custom_axis`, `radius`, `strength`, `smooth`, `target_height`, `falloff`, `spatial`, `line_of_sight`, `autosew`, `selected_only`, `continuous`, `alpha_effect`, `alpha_value`, `alpha_strength`, `blend_channel`, `sculpt` (`push`, `carve`, `project`, `pinch`, `grab`, `inflate`, `flatten`), `push_mode`, `sculpt_bounds`, `falloff_position`, `falloff_final`, `push_distance`, `push_percent`, `carve_depth`, `carve_profile`, `project_range`, `flatten_plane`, `flatten_strength`, `scale_factor`, `noise_type`, `noise_scale`, `noise_octaves`, `noise_persistence`, `terrain_seed`, `thermal_iterations`, `thermal_talus`, `thermal_strength`, `hydraulic_iterations`, `hydraulic_rain`, `hydraulic_evaporation`, `hydraulic_capacity`, `hydraulic_deposition`, `hydraulic_strength`, `blend_width`, `blend_falloff`, `blend_mode`, `blend_match_edges`, `blend_alpha`, `blend_match_powers`, `slope_max_angle`, `slope_terrace`, `slope_terrace_step`, `slope_terrace_strength`, `slope_ramp_angle`, `slope_ramp_direction`, `slope_ramp_pivot`, `slope_ramp_strength`, `slope_ramp_detail`, `slope_uniform_strength`, `slope_uniform_detail`, `slope_path`, `slope_path_width`, `slope_path_falloff`, `slope_path_strength`, `slope_smooth_passes`, `slope_smooth_strength`, `slope_select_angle`, `slope_select_mode`, `slope_select_match`, `slope_select_op`, `tag_kind`, `tag_value`, `mask_grid`, `mask_select`, `keep_shape`. `axis` takes `normal`, `subdiv`, `center`, `view`, `view_flat` (View (camera), horizontal), `x`, `y`, `z` or `custom`. Other MCP tools create, paint, sew, subdivide, tag and inspect displacements (`displacement_to_mesh` writes them to an OBJ, `mesh_to_displacement`, `conform_displacements_to_mesh` and `tile_displacements_from_selection` build or shape them from a 3D mesh, `terrain_displacements` runs fractal noise, thermal erosion or hydraulic erosion with these options, `blend_displacements`, `control_slope` and `select_displacements_by_slope` blend heights and control or select by slope, `create_displacements`, `stroke_displacement`, `paint_displacement`, `sew_displacements`, `resample_displacements`...); `get_displacement` returns the 4-way blend rows (`multiblend`) and `set_displacement_data` writes them (`multiblend_weights`, `alphablend`, `multiblend_colors`, `remove_multiblend`), see the [MCP tool reference](../../MCP.md).

## Tips

- Power 3 is enough for most terrain: power 4 has about 3.5 times more vertices and makes the collision mesh slower (the compile statistics point it out).
- Turn on **Sew while painting** when you paint across several displacements: no crack appears and no Sew is needed afterwards.
- With displacements selected, only those are painted. Press `Esc` to clear the selection and paint everything under the brush.
- Use **Carve** with the **U** profile for paths and rivers, then **Smooth** to soften the banks.
- Use **Line of sight** to paint the front of a hill without touching the terrain behind it.

## FAQ

**Nothing is painted, the brush circle is gray.** Displacements are selected and the brush is over none of them. Click this displacement to select it (`Ctrl` adds it), or press `Esc` to clear the selection, or turn off **Selected only**.

**Create is grayed out.** Select faces, or solids (their top faces are used). Only four sided faces can become displacements.

**My displacements have cracks between them.** Select them and click **Sew**, or paint with **Sew while painting** on.

**I painted the wrong place.** Press `Esc` during the stroke to cancel it, or `Ctrl+Z` after: each stroke is one undo step.

**Why do my blend textures not change?** The material must be a blend material (WorldVertexTransition, or Lightmapped_4WayBlend for the 4 texture buttons).

**My 4-way blend shows only its first texture in the game.** Recompile with Hammer Pro: the blend rows are written to the BSP (see 4-way blends above). A texture that stays hidden while painted can also come from `$textureN_blendstart` set close to 1 in the VMT: paint the texture to its full weight, or lower that value.

## Related pages

- [Sculpt displacements (guide)](../guides/sculpt-displacements.md)
- [Vertex tool](vertex-tool.md)
- [Road details (terrain blending to a road)](road-details.md)
- [Texture tool](texture-tool.md)
- [Render modes and display](render-modes-display.md)
- [Lighting preview](lighting-preview.md)
- [Check for problems](check-problems.md)
- [Settings reference](../reference/settings.md)
- [MCP tool reference](../../MCP.md)

---
[Back to the documentation index](../README.md)
