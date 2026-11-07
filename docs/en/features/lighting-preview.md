[Documentation](../README.md) > Features > Lighting preview | [Français](../../fr/features/lighting-preview.md) | [Русский](../../ru/features/lighting-preview.md)

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
- **Sky light and bounced light**: with **Tools > Options... > Rendering > Bounced light in Lit mode** (`render.lit_indirect`, on) the Lit mode runs a background bake of the map with the compiler (the baked preview's compile, no lightmaps drawn) a moment after you switch to it and after changes affecting lighting (the **Update delay** and **Dynamic updates** of the [Lighting tab](#the-lighting-tab) apply). Its result, the sky light (`_ambient`) and the light the surfaces bounce on each other, stored as light probes near the surfaces (cells of 32 to 512 units, fewer and coarser on huge maps), lights what the real time lights do not reach, like the ambient part of the game's lightmaps; the lights and the sun stay real time and sharp. Until the bake finishes, and when it fails (a leaking map, for example), the Lit mode uses an approximate sky light: the `_ambient` color of the `light_environment` lights only what sees the sky, a floor in the open gets all of it, a wall half of it, interiors and covered places none, thin walls included (**Tools > Options... > Rendering > Sky light only under the sky**, `render.sky_occlusion`). `get_lighting_status` (MCP) reports the state in `realtime.indirect`.
- **HDR light values**: with **Tools > Options... > Rendering > HDR light values** (`render.light_preview_hdr`, on) the lights use `_lightHDR` (or `_light` when it is `-1 -1 -1`) times `_lightscaleHDR`, and the sky `_ambientHDR` times `_AmbientScaleHDR`, like the game on a map compiled with HDR. Off: `_light` and `_ambient`. A light value can be `r g b`, `r g b brightness`, one gray value, or the eight numbers `r g b i` of LDR then HDR.
- **Projected textures**: `env_projectedtexture` entities project their texture like the game does: the texture (`texturename`, `effects/flashlight001` by default, frame `textureframe`), `lightfov` (the game uses 45 when the key is missing), `nearz`, `farz` (the light fades over the last 40 percent of it), full brightness up to 100 units from the projector then falling with the inverse of the distance (the linear attenuation 100 of the game), `lightcolor` times `lightstrength`, aimed by `angles` or by the `target` entity. They light the Lit mode, the baked preview and the player view (over the compiled lightmaps, like the dynamic light of the game), the first 16 of the map; **Start Disabled** and `lightworld 0` leave one out. **Tools > Options... > Rendering > Projected textures** (`render.projected_textures`, on); `get_lighting_status` (MCP) counts them in `realtime.projectors`. `enableshadows` (on unless the key is 0) gives the first four shadowed projectors the shadows of the world and the props. In the player view the I/O simulation plays their inputs `TurnOn`, `TurnOff`, `FOV`, `EnableShadows`, `SpotlightTexture` and `LightWorld` (a Start Disabled projector lights once `TurnOn` reaches it); leaving the player view restores the keys of the map.
- **Light entities**: they follow the compiler (checked against the real VRAD): a `light` without attenuation keys has no falloff (constant 1), terms below 0.001 read as 0, `_fifty_percent_distance`, `_zero_percent_distance` and `_hardfalloff` give VRAD's curves, spot cones are clamped to 90 degrees (180 and 180 make a point light) and fall off with the cosine of the angle to the axis, `_inner_cone`, `_cone` and `_exponent` shape the penumbra, an `angle` of -1 or -2 without a `pitch` points nowhere like in VRAD, `_distance` limits only the lighting of props (VRAD lights past it), named lights with **Initially dark** stay off, and a light inside an opaque brush lights nothing. Only the first `light_environment` counts, wherever it stands.
- **Big maps**: every light is drawn, even thousands. Where more than 128 lights reach the same spot of the view, only the 128 strongest there light it. `get_lighting_status` (MCP) tells how many lights are drawn and how many spots are crowded.
- **Light shadows**: every point and spot light in view casts shadows, the brightest first, so a light never shines through a wall (VRAD shadows every light). **Tools > Options... > Rendering > Shadowed lights** (`render.light_shadows`) sets how many: 0 to 64, 16 by default, 0 for the sun only; past 8 lights the shadow maps are coarser (256 pixels a face instead of 512). A light still waiting for its shadow maps stays dark instead of leaking. Shadow maps are drawn again only when a light or the scene changes. Brush faces of tool textures that block the light (nodraw, `toolsblocklight`) cast shadows even when hidden.
- Props are lit by the same lights, sun and sky light as the brushes, per pixel.
- With **Relief (normal maps)** on (default), the normal maps of the materials give the surfaces their relief.
- A material with `$nodiffusebumplighting 1` keeps a flat diffuse response while its normal map still shapes cubemap reflections. Baked lighting uses a flat lightmap for that material.

## Baked lighting preview (Ctrl+F5)

Like Hammer++'s "3D Lighting Preview", but the numbers are the compiler's: the map is compiled in memory by the Hammer Pro compiler (the geometry step) and lit by its lighting step (VRAD's) in a separate process. The compiled faces are drawn with their lightmaps (the four bumped lightmaps of bump mapped faces, for the relief of the normal maps) and shaded exactly like the compiled lighting of the [player view](player-view.md) (the same bump weights, cubemap reflections, self illumination and texture scrolling, the LDR lightmaps of a map compiled without HDR the way the game draws them, the sky seen through the sky faces), so the preview gives the picture of the compiled map in game, and props are lit by the ambient cubes, world lights and static prop vertex lighting of the lit map. The static props follow the compile settings for prop shadows and prop lighting. A prop is lit at its lighting origin like in game: the `lightingorigin` entity when the key names one, else the center of the model.

- When the preview is turned on and nothing is baked yet, a whole-map bake starts by default (setting **Bake when the preview is turned on**). The optional **Bake only around the camera on big maps** setting can restrict this automatic start to a region; it is off by default.
- **Bake lighting** always calculates the whole map, including when the automatic regional option is enabled. Its result stays global as the camera moves.
- Every bake runs in two passes. A **quick pass** first, with the Hammer++ sampling of the map's options (**Sun Samples**, **Ambient Samples** and **Supersample** of the `light_plus` block, 6, 40 and off by default; Draft bounced light; no light probes), shown as soon as it is done: same light levels, a little more noise. Then a **refined pass** at the chosen **Quality** replaces it when no lighting inputs changed in between (progress kind **Refining**). A change affecting the lighting cancels the running pass at once (with **Dynamic updates** on) and starts a new quick one once those edits stop for the update delay.
- An edited solid hides its baked faces until the next bake; until then its faces, like the surfaces without lightmaps (water, translucent glass, props without baked lighting), are lit by the sun and the sky color only, without shadows. With **Dynamic updates** on, the map is baked again once edits stop for the **Update delay**.
- The baked view uses the calculated lightmaps: no real time light, sun shadow map, sky visibility map or light clusters are computed while it shows a bake. Moving the camera reuses the result. Props use the bake's ambient cubes, world lights and static prop vertex lighting; materials, models and reflections still have their normal drawing cost.
- **Bake around the camera** compiles and lights only what lies within the **Radius around the camera** of the 3D camera; the area is sealed like a cordon. This manually chosen region stays at its original position. It is separate from the optional automatic region that follows the camera.
- **Stop baking** stops at once (the lighting process is ended and its memory goes back to the system).
- The preview compile only stops on what the preview really needs: limits of the game it cannot store are reported as warnings, the planes of the brushes under displacements (which the BSP does not keep) are left out so a map just over MAX_MAP_PLANES still bakes, and bad `env_cubemap` keys are ignored. A real compile still stops on them, like VBSP.
- The lighting process and the compile threads run at a lower CPU priority and a lower memory priority so the editor stays fluid and Windows trims their pages first when the memory is short. The compile of the preview leaves out the collision data and the statistics, frees what it no longer needs while it runs and drops the hidden objects before it starts, so its peak memory is lower than a normal compile of the same map. The lightmap texture is only as large as the lightmaps need (at least 256 by 256 pixels, at most 2048), which saves video memory on small bakes. Large maps are previewed with coarser lightmaps (setting **Full resolution luxels**), and a note says "Large map: previewed at N times the lightmap resolution".
- HDR or LDR comes from the compile settings: the tab shows **HDR** or **LDR** accordingly.

## Reusing a calculated result

Wait for the quick and refined passes to finish, then navigate in the baked view. A whole-map result remains valid when the camera moves or the selection changes. Preview brightness, color correction and the update delay affect presentation or scheduling and do not restart the lighting calculation.

Geometry, light and material changes, and calculation settings such as quality, HDR/LDR, sampling, bounced light and static-prop lighting/shadows affect the result's validity. **Dynamic updates** recalculates after relevant edits stop for the configured delay. With automatic updates disabled, the Lighting tab reports **Lighting is out of date** and **Bake again** starts a new whole-map calculation. The progress, last result and stale state are also available through `get_lighting_status`.

The regional option is an explicit choice for automatic preview activation on large maps. It follows the camera after it leaves the central half of the region and stays still for one second, so entering a different area can start another calculation. **Bake lighting** returns to a whole-map result with no camera following. Existing saved values of `lighting.auto_scope` are preserved; a missing value uses the new default, `false`.

## The Lighting tab

**View > Panels > Lighting** shows the tab.

**Lighting preview** section:

| Element | What it does |
| --- | --- |
| **Bake lighting** | Always bakes the whole map and keeps that scope while navigating. |
| **Bake around the camera** | Bakes a fixed area around the current 3D camera (tooltip gives the radius). |
| **Stop baking** | Enabled while a bake runs. |
| Progress | Kind (**Whole map**, **Around the camera**, **Update**, **Refining**), phase (**Preparing**, **Compiling the map**, **Direct light**, **Bounced light**, **Finishing**) and time. |
| Last result | "N faces baked in N s", or **Baking stopped**, or **Not baked yet**; video memory and shadow triangles. |
| Warnings | "The map has no light entity", **Lighting is out of date** with **Bake again**. |
| **Quality** | **Draft** (same light model and light levels as a compile, noisier shadows, the default), **Normal** (a normal compile), **Final** (a final compile, slowest). |

**Options (saved in the map)**: stored in the Hammer++ `light_plus` block of the VMF, so they travel with the map.

| Option | What it does | Default |
| --- | --- | --- |
| **Brightness** | Brightness of the preview, 0 to 4. | 1 |
| **Bounced light** | Light reflected by surfaces (slower bakes). | On |
| **Dynamic updates** | Bakes again automatically once edits stop for the update delay. | On |
| **Update delay** | 0 to 60 s. | 0 s |
| **Radius around the camera** | Radius of **Bake around the camera**, 64 to 65536. | 1024 |
| **Advanced > Sun samples** | Sun rays per luxel of the quick pass (Hammer++ **Sun Samples**), 1 to 256. | 6 |
| **Advanced > Ambient samples** | Sky light directions per luxel of the quick pass (Hammer++ **Ambient Samples**), 16 to 5184. | 40 |
| **Advanced > Supersample** | Smooths the shadow edges of the quick pass (Hammer++ **Supersample**), slower. | Off |
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

## Tone mapping, exposure and bloom

The lit views (the Lit mode, the baked preview and the player view with the HDR lightmaps of a compiled map) turn light into pixels the way the game does: the picture is drawn in HDR, the exposure scales it, and surfaces much brighter than the screen can show glow (bloom).

- **HDR tone mapping** (`render.tone_mapping`, **Tools > Options... > Rendering**): **Auto exposure** (default) measures the picture like the engine (the 2 percent brightest pixels aim at 60 percent, never under `mat_tonemap_min_avglum` and with the same adaptation speed) and keeps the exposure between **Lowest auto exposure** (`render.exposure_min`, 0.5) and **Highest auto exposure** (`render.exposure_max`, 2); **Fixed exposure** uses **Exposure** (`render.exposure`, 1 is the light of the lightmaps as it is, `mat_force_tonemap_scale`); **Off** shows the light as it is.
- **Bloom** (`render.bloom`, 1 like `mat_bloom_scalefactor_scalar`; 0 turns the glow off). It also follows the map: with **Follow the map's tone controller** (`render.map_tonemap`, on) the `env_tonemap_controller` of the map sets the exposure range and the bloom scale (`SetAutoExposureMin`, `SetAutoExposureMax`, `SetBloomScale`, also from a `logic_auto` at map start), like in game.
- LDR lighting (a map compiled without HDR) is shown without tone mapping, like the game. The sky of the map (`env_skypaint` or the skybox faces, drawn from their `$basetexture` like Garry's Mod does even for the `_hdr` skybox materials) glows through the same bloom.
- MCP: `set_setting` changes these options, `get_lighting_status` returns `tone` (mode, the exposure now, its range, the bloom, what the map's tone controller sets and the luminance histogram the auto exposure reads).

## Options

**Tools > Options... > Lighting** (`[lighting]` in `settings.toml`):

| Option | What it does | Default |
| --- | --- | --- |
| **Bake when the preview is turned on** (`auto_bake`) | Starts a bake when the preview is turned on and nothing is baked yet. Uses the whole map unless the optional automatic regional mode is enabled. | On |
| **Baking threads** (`threads`) | 0 uses every core (every core but two to compile the map). Used by the next bake. | 0 |
| **Full resolution luxels (millions)** (`max_luxels_millions`) | Maps with more luxels are previewed with coarser lightmaps (1 to 64). | 16 |
| **Bake only around the camera on big maps** (`auto_scope`) | Opt-in region for automatic preview activation above the threshold. The area follows the camera after it leaves the central half and stays still for a second. **Bake lighting** always stays global. | Off |
| **Big map threshold (millions of luxels)** (`auto_scope_luxels_millions`) | Threshold for the optional automatic regional preview (1 to 64). | 2 |
| **Big map radius** (`auto_scope_radius`) | Radius in units of the optional automatic region (256 to 32768). The manual camera bake has its own radius in the Lighting tab. | 3072 |
| **Show the lightmap budget in the status bar** (`show_budget`) | | On |
| **Tiny lightmap scale** (`tiny_scale`) | Faces at or under this scale count as tiny (1 to 64). | 4 |
| **Light styles** (`budget_styles`) | Lightmaps per face counted by the budget: 1 without switchable lights, up to 4. | 1 |
| Quality (`quality`) | Set in the Lighting tab: `Draft`, `Normal`, `Final`. | Draft |

Rendering options used here: **Shadowed lights** (`render.light_shadows`, 16), **Bounced light in Lit mode** (`render.lit_indirect`, on), **HDR light values** (`render.light_preview_hdr`, on), **Sky light only under the sky** (`render.sky_occlusion`, on), **Static prop shadows** (`render.prop_shadows`, Auto), **Relief (normal maps)** (`render.normal_maps`, on), **Cubemap reflections** (`render.cubemaps`, on), **Redraw the cubemaps after edits** (`render.cubemaps_auto`, on).

## Tips

- Use **Bake around the camera** while you tune one room: it is much faster than a full bake.
- Keep **Quality** on **Draft** while working; the light levels are the same, only the shadows are noisier.
- MCP tools: `bake_lighting`, `get_lighting_status`, `set_lighting_options` (the options of the tab, see the [MCP tool reference](../../MCP.md)).

## FAQ

**The baked preview stays dark.** The map has no light entity, or nothing is baked yet: press **Bake lighting**.

**Why does a brush I moved look unlit?** Edited solids hide their baked faces until the next bake and are lit by the sun and the sky color only meanwhile. With **Dynamic updates** on, it comes back once you stop editing.

**Why does the lighting change a little a few seconds after a bake?** The quick pass is shown first, then the refined pass at the chosen quality replaces it. The light levels are the same; the refined pass has smoother shadows and less noise.

**Does the preview match the game?** It uses the same compiler and lighting step as a real compile, at the quality you choose, and draws the result like the player view draws the compiled map. **Draft** differs from a compile only by its noise (fewer sky, sun and bounce rays); **Normal** matches a Normal compile and **Final** a final compile.

**Write cubemaps says the cubemaps are drawn with the editor shading.** Open the player view and press `L` so the compiled lighting is used, then try again.

## Related pages

- [Render modes and display options](render-modes-display.md)
- [Lighting guide](../guides/lighting.md)
- [Cubemaps](../compiler/cubemaps.md)
- [Player view](player-view.md)
- [Settings reference](../reference/settings.md)

---
[Back to the documentation index](../README.md)
