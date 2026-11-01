[Documentation](../README.md) > Features > Displacement tool | [Français](../../fr/features/displacement-tool.md)

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
| **Sculpt** | Push the surface to a layer, carve a profile along the stroke, or project it onto brushes |
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
| **Power** | Rebuilds the selected displacements flat with this power (2, 3 or 4). The sculpting is lost | Current power, or "(multiple values)" |
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
- **Moving and mirroring** a displacement keeps its 4-way rows in place, like its alpha. Changing the power or flattening starts over from texture 1.

## Sculpt

Section **Sculpt** (mode **Sculpt**), Hammer's Sculpt panel:

- **Push**: raises the surface to a layer at an offset from where it was when the stroke started (`Ctrl` sinks it). Going over the same place again does not pile up.
- **Carve**: the stroke is a path, the surface along it sinks following a cross profile: ditches, rivers, roads (`Ctrl` raises a ridge).
- **Project**: moves the surface onto the selected brushes along the sculpt axis (a road or a foundation the terrain should follow). Select the brushes together with the displacements; if no brush is selected the tool tells you.

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
| **Add noise** | Adds a random height between the minimum and the maximum to every vertex, along the face normal | |

A checkbox shows a mixed state when the selection disagrees; clicking it turns the flag on for all.

The same commands are in the **Tools** menu and work whatever the active tool: **Subdivide displacements**, **Invert displacement alpha** and **Add noise to displacements...** (this one opens a small **Displacement noise** window with the range and an **Apply** button). None has a default shortcut.

## Display options

| Where | Option | What it does | Default |
| --- | --- | --- | --- |
| **View > Display options** | **Show displacements** | Shows or hides displacements | On |
| **View > Display options** | **Displacement mesh in 2D views** | Switches 2D views between the full triangle mesh and the outline only | Full mesh |
| **View > Display options** | **Displacement walkability** | Colors triangles: walkable and buildable (slope normal Z 0.8 or more), walkable (0.7 or more), not walkable. Forced tags win, like in the compiled map. A legend is drawn in the 3D view (and in a maximized 2D view showing the mesh) | Off |
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

Every option of the panel has a name for scripts (`tool_option KEY VALUE`, see [Script commands](../reference/script-commands.md)) and for the MCP tools `get_tool_options` and `set_tool_options`: `mode`, `effect`, `axis`, `custom_axis`, `radius`, `strength`, `smooth`, `target_height`, `falloff`, `spatial`, `line_of_sight`, `autosew`, `selected_only`, `continuous`, `alpha_effect`, `alpha_value`, `alpha_strength`, `blend_channel`, `sculpt`, `push_mode`, `sculpt_bounds`, `falloff_position`, `falloff_final`, `push_distance`, `push_percent`, `carve_depth`, `carve_profile`, `project_range`, `scale_factor`, `tag_kind`, `tag_value`, `mask_grid`, `mask_select`. `axis` takes `normal`, `subdiv`, `center`, `view`, `view_flat` (View (camera), horizontal), `x`, `y`, `z` or `custom`. Other MCP tools create, paint, sew, subdivide, tag and inspect displacements (`create_displacements`, `stroke_displacement`, `paint_displacement`, `sew_displacements`...); `get_displacement` returns the 4-way blend rows (`multiblend`) and `set_displacement_data` writes them (`multiblend_weights`, `alphablend`, `multiblend_colors`, `remove_multiblend`), see the [MCP tool reference](../../MCP.md).

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
- [Texture tool](texture-tool.md)
- [Render modes and display](render-modes-display.md)
- [Lighting preview](lighting-preview.md)
- [Check for problems](check-problems.md)
- [Settings reference](../reference/settings.md)
- [MCP tool reference](../../MCP.md)

---
[Back to the documentation index](../README.md)
