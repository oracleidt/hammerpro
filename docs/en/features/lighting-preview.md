[Documentation](../README.md) > Features > Lighting preview | [Français](../../fr/features/lighting-preview.md)

# Lighting preview

Hammer Pro shows lighting in the editor in two ways: the **Lit** mode, real-time preview lights with shadows, and the **baked lighting preview**, the map compiled and lit by the built-in compiler so you see the lighting the game will have, without leaving the editor. The **Lighting** tab, the lightmap grid and the lightmap budget help you keep the lightmaps in check, and the 3D view draws cubemap reflections.

## Opening it

| Action | Menu | Default shortcut |
| --- | --- | --- |
| **Lighting preview** (Lit mode) | **View > 3D render mode** | `Alt+5` |
| **Lightmap grid** | **View > 3D render mode** | `Alt+8` |
| **Baked lighting preview** | **View > 3D render mode** | `Ctrl+F5` |
| **Bake lighting** | **Map > Lighting** | none |
| **Bake around the camera** | **Map > Lighting** | none |
| **Stop baking** | **Map > Lighting** | none |
| **Select faces with a tiny lightmap scale** | **Map > Lighting** | none |
| **Rebuild cubemaps** | **Map > Lighting** | none |
| **Write cubemaps into the compiled map** | **Map > Lighting** | none |
| **Cubemap reflections** | **View > Display options** | none |
| Lighting tab | **View > Panels > Lighting** | none |

## Lit mode (Alt+5)

The `light`, `light_spot`, `light_environment` and `light_dynamic` entities light the 3D view in real time, with their colors, brightness and falloff. It updates as you move a light.

- **The sun**: the first `light_environment` of the map (like VRAD, the others are ignored) shines along its **angles**, or along its **pitch** key when that key is not 0, the same way as the compiler: a negative pitch points down, so `pitch -90` shines straight down and a pitch past -90 turns the sun around. A `target` key aims it at the entity of that name, like VRAD. The sun lights only what sees a sky face toward it, like VRAD: what lies beyond the sky (the 3D skybox room, the backs of the sky brushes, rooms sealed outside the playable area) casts no shadow on the map, and places with no sky above them get no sun. Each point is shadowed only by what lies between it and the sky it reaches toward the sun, so skies stacked one above the other work like in game: a sealed room with its own sky ceiling floating above a skyboxed area does not shadow that area. `get_lighting_status` (MCP) tells how many skies lie one above the other (`sky_layers`).
- **Sun shadows**: over what the camera sees, up to 8192 units away. **SunSpreadAngle** softens them like the compiler: the farther the shadow is from what casts it, the softer it gets. Brush entities cast no shadow unless `vrad_brush_cast_shadows` is 1 (like VRAD), props with `disableshadows` 1 neither, and the editor helpers (player starts, nodes, boxes of point entities) never do.
- **Static prop shadows**: `prop_static` entities cast shadows like the compile will. **Tools > Options... > Rendering > Static prop shadows** (`render.prop_shadows`): **Auto** (default) follows the compile options of the chosen compiler: the triangles the model draws when the lighting step runs with `-StaticPropPolys`, its collision model otherwise (plain VRAD settings, and the Hammer Pro compiler unless you add the flag), none with `-nostaticpropshadows` (or the compile window's **Static prop shadows** unchecked). **Model triangles** and **Collision model** force one. A model without a collision model casts the convex hull of each of its meshes, like the compiler. Other props (`prop_dynamic`, `prop_physics`) cast with their triangles, like their shadows in game.
- **Sky light**: the `_ambient` color of the `light_environment` lights only what sees the sky, like VRAD: a floor in the open gets all of it, a wall half of it, interiors and covered places none. **Tools > Options... > Rendering > Sky light only under the sky** (`render.sky_occlusion`, on). Off, the whole map gets it.
- **HDR light values**: with **Tools > Options... > Rendering > HDR light values** (`render.light_preview_hdr`, on) the lights use `_lightHDR` (or `_light` when it is `-1 -1 -1`) times `_lightscaleHDR`, and the sky `_ambientHDR` times `_AmbientScaleHDR`, like the game on a map compiled with HDR. Off: `_light` and `_ambient`. A light value can be `r g b`, `r g b brightness`, one gray value, or the eight numbers `r g b i` of LDR then HDR.
- **Light entities**: spot cones are clamped to 90 degrees like VRAD (180 and 180 make a point light), `_fifty_percent_distance`, `_zero_percent_distance`, `_hardfalloff`, `_distance` and the `_constant_attn`, `_linear_attn`, `_quadratic_attn` terms follow the compiler, named lights with **Initially dark** stay off, and a light inside an opaque brush lights nothing.
- **Big maps**: every light is drawn, even thousands. Where more than 128 lights reach the same spot of the view, only the 128 strongest there light it. `get_lighting_status` (MCP) tells how many lights are drawn and how many spots are crowded.
- **Light shadows**: the point and spot lights nearest the camera cast shadows too. **Tools > Options... > Rendering > Shadowed lights** (`render.light_shadows`) sets how many: 0 to 16, 4 by default, 0 for the sun only. Shadow maps are drawn again only when a light or the scene changes.
- Props are lit by the same lights, sun and sky light as the brushes, per pixel.
- With **Relief (normal maps)** on (default), the normal maps of the materials give the surfaces their relief.

## Baked lighting preview (Ctrl+F5)

Like Hammer++'s "3D Lighting Preview", but the numbers are the compiler's: the map is compiled in memory by the Hammer Pro compiler (the geometry step) and lit by its lighting step (VRAD's) in a separate process. The compiled faces are drawn with their lightmaps (the four bumped lightmaps of bump mapped faces, for the relief of the normal maps), and props are lit by the ambient cubes, world lights and static prop vertex lighting of the lit map. The static props follow the compile settings for prop shadows and prop lighting. A prop is lit at its lighting origin like in game: the `lightingorigin` entity when the key names one, else the center of the model.

- When the preview is turned on and nothing is baked yet, a full bake starts (setting **Bake when the preview is turned on**).
- An edited solid hides its baked faces until the next bake. With **Dynamic updates** on, the map is baked again once edits stop for the **Update delay**.
- **Bake around the camera** compiles and lights only what lies within the **Radius around the camera** of the 3D camera; the area is sealed like a cordon.
- **Stop baking** stops at once (the lighting process is ended and its memory goes back to the system).
- The lighting runs at a lower priority so the editor stays fluid. Large maps are previewed with coarser lightmaps (setting **Full resolution luxels**), and a note says "Large map: previewed at N times the lightmap resolution".
- HDR or LDR comes from the compile settings: the tab shows **HDR** or **LDR** accordingly.

## The Lighting tab

**View > Panels > Lighting** shows the tab.

**Lighting preview** section:

| Element | What it does |
| --- | --- |
| **Bake lighting** | Bakes the whole map. |
| **Bake around the camera** | Bakes the area around the 3D camera (tooltip gives the radius). |
| **Stop baking** | Enabled while a bake runs. |
| Progress | Kind (**Whole map**, **Around the camera**, **Update**), phase (**Preparing**, **Compiling the map**, **Direct light**, **Bounced light**, **Finishing**) and time. |
| Last result | "N faces baked in N s", or **Baking stopped**, or **Not baked yet**; video memory and shadow triangles. |
| Warnings | "The map has no light entity", **Lighting is out of date** with **Bake again**. |
| **Quality** | **Draft** (same light levels, noisier shadows, the default), **Normal** (a normal compile), **Final** (a final compile, slowest). |

**Options (saved in the map)**: stored in the Hammer++ `light_plus` block of the VMF, so they travel with the map.

| Option | What it does | Default |
| --- | --- | --- |
| **Brightness** | Brightness of the preview, 0 to 4. | 1 |
| **Bounced light** | Light reflected by surfaces (slower bakes). | On |
| **Dynamic updates** | Bakes again automatically once edits stop for the update delay. | On |
| **Update delay** | 0 to 60 s. | 0 s |
| **Radius around the camera** | Radius of **Bake around the camera**, 64 to 65536. | 1024 |
| **Advanced > Show light reach** | Draws how far the selected lights reach. | Off |
| **Advanced > Restore defaults** | Resets these options. | |

**Color correction**: lookups (`.raw`) applied to the baked preview with a weight each (0 to 1), saved in the map like Hammer++. **From the map** uses the files and maximum weights of the `color_correction` entities; **Clear** removes them.

**Lightmap budget**: a gauge of the estimated size of the lightmaps in the BSP against the 2 GB a face can address in them (the only limit of the game, see [Limits and logs](../compiler/limits-and-logs.md)), with the number of faces and luxels, the faces with a tiny lightmap scale (**Select** selects them), the faces VBSP splits (wider than 32 luxels), displacements too large for VBSP, and **Most expensive faces** (face, scale, luxels, size; click a face to select it).

## Lightmap tools

- **Lightmap grid** (`Alt+8`) draws the luxel grid of every lightmapped face like Hammer's 3D lightmap grid, no bake needed: one checker cell per luxel, as wide as the face's lightmap scale and aligned to its texture axes, displacements included. The color tells the scale: grey at 16 (the default), redder the finer the scale (8, 4 and less stand out), greener the coarser. Yellow lines mark where VBSP splits a face wider than 32 luxels. Once a bake exists, the grid is shaded by its lighting. Faces without a lightmap (nodraw, tool textures, sky, water, unlit materials) and props keep their texture.
- **Select faces with a tiny lightmap scale** selects them, ready for the texture tool.
- The **status bar** shows the lightmap budget ("N / 2048 MB"). Click it to open the Lighting tab.

## Cubemaps in the views

With **Cubemap reflections** on (default), materials with `$envmap` reflect their cubemap in the 3D view (textured, lit and baked modes) and in the player view. The editor draws a cubemap at every `env_cubemap` (the sky when there is none), a few faces per frame. Each face takes the nearest `env_cubemap` in front of it like the compiler, props the nearest one. The player view uses the cubemaps built in game when the compiled map has them.

- **Redraw the cubemaps after edits** (`render.cubemaps_auto`, on): the cubemaps are drawn again a moment after the map, its lighting or the view mode change. Off: only with **Map > Lighting > Rebuild cubemaps** (new and moved `env_cubemap` entities still get theirs).
- **Parallax corrected cubemaps** (Mapbase): an `env_cubemap` whose **Cubemap Bounds** (`parallaxobb`) names a `parallax_obb` box brush gets its reflections corrected to that box in the views. Garry's Mod's stock shaders ignore the correction in game.
- **Write cubemaps into the compiled map** writes the cubemaps the editor drew into the pakfile of the compiled map, like `buildcubemaps` (LDR, and HDR when the map has HDR lighting). Open the [player view](player-view.md) on the compiled map with its compiled lighting (`L`) first, so they are drawn like in game, and wait until they are complete. Recompiles keep them. See [Cubemaps](../compiler/cubemaps.md).

## Options

**Tools > Options... > Lighting** (`[lighting]` in `settings.toml`):

| Option | What it does | Default |
| --- | --- | --- |
| **Bake when the preview is turned on** (`auto_bake`) | Starts a full bake when the baked preview is turned on and nothing is baked yet. | On |
| **Baking threads** (`threads`) | 0 uses every core (every core but two to compile the map). Used by the next bake. | 0 |
| **Full resolution luxels (millions)** (`max_luxels_millions`) | Maps with more luxels are previewed with coarser lightmaps (1 to 64). | 16 |
| **Show the lightmap budget in the status bar** (`show_budget`) | | On |
| **Tiny lightmap scale** (`tiny_scale`) | Faces at or under this scale count as tiny (1 to 64). | 4 |
| **Light styles** (`budget_styles`) | Lightmaps per face counted by the budget: 1 without switchable lights, up to 4. | 1 |
| Quality (`quality`) | Set in the Lighting tab: `Draft`, `Normal`, `Final`. | Draft |

Rendering options used here: **Shadowed lights** (`render.light_shadows`, 4), **HDR light values** (`render.light_preview_hdr`, on), **Sky light only under the sky** (`render.sky_occlusion`, on), **Static prop shadows** (`render.prop_shadows`, Auto), **Relief (normal maps)** (`render.normal_maps`, on), **Cubemap reflections** (`render.cubemaps`, on), **Redraw the cubemaps after edits** (`render.cubemaps_auto`, on).

## Tips

- Use **Bake around the camera** while you tune one room: it is much faster than a full bake.
- Keep **Quality** on **Draft** while working; the light levels are the same, only the shadows are noisier.
- MCP tools: `bake_lighting`, `get_lighting_status` (see the [MCP tool reference](../../MCP.md)).

## FAQ

**The baked preview stays dark.** The map has no light entity, or nothing is baked yet: press **Bake lighting**.

**Why does a brush I moved look unlit?** Edited solids hide their baked faces until the next bake. With **Dynamic updates** on, it comes back once you stop editing.

**Does the preview match the game?** It uses the same compiler and lighting step as a real compile, at the quality you choose. **Final** matches a final compile.

**Write cubemaps says the cubemaps are drawn with the editor shading.** Open the player view and press `L` so the compiled lighting is used, then try again.

## Related pages

- [Render modes and display options](render-modes-display.md)
- [Lighting and ambient occlusion guide](../guides/lighting-and-ao.md)
- [Cubemaps](../compiler/cubemaps.md)
- [Player view](player-view.md)
- [Settings reference](../reference/settings.md)

---
[Back to the documentation index](../README.md)
