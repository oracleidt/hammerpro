[Documentation](../README.md) > Features > Lighting preview | [Français](../../fr/features/lighting-preview.md) | [Русский](../../ru/features/lighting-preview.md)

# Lighting preview

Hammer Pro shows lighting in the editor in two ways: the **Lit** mode, real-time preview lights with shadows, and the **baked lighting preview**, the map compiled and lit by the built-in compiler so you see the lighting the game will have, without leaving the editor. The **Lighting** tab, the lightmap grid and the lightmap budget help you keep the lightmaps in check, and the 3D view draws cubemap reflections.

## Light Appearance

The **Appearance** property of a `light` or `light_spot` previews Source flicker and pulse styles at ten steps per second. A custom **Pattern** uses the same letter values as Source: `a` is dark, `m` is normal and `z` is brighter. Named lights retain their appearance when compilation assigns a switchable style number. **Initially dark** keeps a named light off in the initial preview.

A bake retains its separate light style layers, including bumped lightmaps. Those layers animate without running VRAD again. HDR layers mix in linear space; LDR conversion follows the mix. Animation does not rebuild static cubemaps every frame. Editing a light relights it in the background lighting process, bounced light included, usually in a fraction of a second; the live difference of the real time lights covers the moment in between.

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
| **Solo the selected lights** | **Map > Lighting** | none |
| **Mute the selected lights** | **Map > Lighting** | none |
| **Unmute all lights** | **Map > Lighting** | none |
| **Measure the cost of each light** | **Map > Lighting** | none |
| **Rebuild cubemaps** | **Map > Lighting** | none |
| **Write cubemaps into the compiled map** | **Map > Lighting** | none |
| **Cubemap reflections** | **View > Display options** | none |
| Lighting tab | **View > Panels > Lighting** | none |

## Lit mode (Alt+5)

The `light`, `light_spot`, `light_environment` and `light_dynamic` entities light the 3D view in real time, with their colors, brightness and falloff. It updates as you move a light. `point_spotlight` entities are previewed too (see [Spotlights and dynamic lights](#spotlights-and-dynamic-lights)).

- **The sun**: the first `light_environment` of the map (like VRAD, the others are ignored) shines along its **angles**, or along its **pitch** key when that key is not 0, the same way as the compiler: a negative pitch points down, so `pitch -90` shines straight down and a pitch past -90 turns the sun around. A `target` key aims it at the entity of that name, like VRAD. The sun lights only what sees a sky face toward it, like VRAD: what lies beyond the sky (the 3D skybox room, the backs of the sky brushes, rooms sealed outside the playable area) casts no shadow on the map, and places with no sky above them get no sun. Each point is shadowed only by what lies between it and the sky it reaches toward the sun, so skies stacked one above the other work like in game: a sealed room with its own sky ceiling floating above a skyboxed area does not shadow that area. `get_lighting_status` (MCP) tells how many skies lie one above the other (`sky_layers`).
- **Sun shadows**: over what the camera sees, up to 8192 units away. **SunSpreadAngle** softens them like the compiler: the farther the shadow is from what casts it, the softer it gets. Brush entities cast no shadow unless `vrad_brush_cast_shadows` is 1 (like VRAD), props with `disableshadows` 1 neither, and the editor helpers (player starts, nodes, boxes of point entities) never do.
- **Static prop shadows**: `prop_static` entities cast shadows like the compile will. **Tools > Options... > Rendering > Static prop shadows** (`render.prop_shadows`): **Auto** (default) follows the compile options of the chosen compiler: the triangles the model draws when the lighting step runs with `-StaticPropPolys`, its collision model otherwise (plain VRAD settings, and the Hammer Pro compiler unless you add the flag), none with `-nostaticpropshadows` (or the compile window's **Static prop shadows** unchecked). **Model triangles** and **Collision model** force one. A model without a collision model casts the convex hull of each of its meshes, like the compiler. Other props (`prop_dynamic`, `prop_physics`) cast with their triangles, like their shadows in game.
- **Sky light and bounced light**: with **Tools > Options... > Rendering > Bounced light in Lit mode** (`render.lit_indirect`, on) the Lit mode runs a background bake of the map with the compiler (the baked preview's compile, no lightmaps drawn) a moment after you switch to it and after changes affecting lighting (the **Update delay** and **Dynamic updates** of the [Lighting tab](#the-lighting-tab) apply). Its result, the sky light (`_ambient`) and the light the surfaces bounce on each other, stored as light probes near the surfaces (cells of 32 to 512 units, fewer and coarser on huge maps), lights what the real time lights do not reach, like the ambient part of the game's lightmaps; the lights and the sun stay real time and sharp. Until the bake finishes, and when it fails (a leaking map, for example), the Lit mode uses an approximate sky light: the `_ambient` color of the `light_environment` lights only what sees the sky, a floor in the open gets all of it, a wall half of it, interiors and covered places none, thin walls included (**Tools > Options... > Rendering > Sky light only under the sky**, `render.sky_occlusion`). `get_lighting_status` (MCP) reports the state in `realtime.indirect`.
- **HDR light values**: with **Tools > Options... > Rendering > HDR light values** (`render.light_preview_hdr`, on) the lights use `_lightHDR` (or `_light` when it is `-1 -1 -1`) times `_lightscaleHDR`, and the sky `_ambientHDR` times `_AmbientScaleHDR`, like the game on a map compiled with HDR. Off: `_light` and `_ambient`. A light value can be `r g b`, `r g b brightness`, one gray value, or the eight numbers `r g b i` of LDR then HDR.
- **Projected textures**: `env_projectedtexture` entities project their texture like the game does: the texture (`texturename`, `effects/flashlight001` by default, frame `textureframe`), `lightfov` (the game uses 45 when the key is missing), `nearz`, `farz` (the light fades over the last 40 percent of it), full brightness up to 100 units from the projector then falling with the inverse of the distance (the linear attenuation 100 of the game), `lightcolor` times `lightstrength`, aimed by `angles` or by the `target` entity. They light the Lit mode, the baked preview and the player view (over the compiled lightmaps, like the dynamic light of the game), the first 16 of the map; **Start Disabled** and `lightworld 0` leave one out. **Tools > Options... > Rendering > Projected textures** (`render.projected_textures`, on); `get_lighting_status` (MCP) counts them in `realtime.projectors`. `enableshadows` (on unless the key is 0) gives the first four shadowed projectors the shadows of the world and the props. In the player view the I/O simulation plays their inputs `TurnOn`, `TurnOff`, `FOV`, `EnableShadows`, `SpotlightTexture` and `LightWorld` (a Start Disabled projector lights once `TurnOn` reaches it); leaving the player view restores the keys of the map.
- **Light entities**: they follow the compiler (checked against the real VRAD): a `light` without attenuation keys has no falloff (constant 1), terms below 0.001 read as 0, `_fifty_percent_distance`, `_zero_percent_distance` and `_hardfalloff` give VRAD's curves, spot cones are clamped to 90 degrees (180 and 180 make a point light) and fall off with the cosine of the angle to the axis, `_inner_cone`, `_cone` and `_exponent` shape the penumbra, an `angle` of -1 or -2 without a `pitch` points nowhere like in VRAD, `_distance` limits only the lighting of props (VRAD lights past it), named lights with **Initially dark** stay off, and a light inside an opaque brush lights nothing. Only the first `light_environment` counts, wherever it stands.
- **LDR lighting**: with **HDR light values** off (or a map compiled in LDR only when the preview follows the compile settings) the Lit mode shades the way the game draws LDR lightmaps: the light is turned into the gamma of the lightmaps, multiplied with the texture in gamma space and clipped there, so very bright light turns white like in game instead of changing its hue, and dim light keeps the level of the compiled map.
- **Water and glass let the light through**: brushes with a water or translucent face cast no shadow in the Lit mode, their nodraw faces included, like the compiler (their contents are not opaque), so the floor of a pool or the room behind a window gets the light of the lights and the sun.
- **Checked against the compiler**: on the test maps, the Lit mode matches the baked preview of the same map (the lightmaps of the compiler, HDR or LDR, bounced light included) within a few percent everywhere: same falloff, spot cones, sun, sky light, colors, normal maps and props.
- **Big maps**: every light is drawn, even thousands. Where more than 128 lights reach the same spot of the view, only the 128 strongest there light it. `get_lighting_status` (MCP) tells how many lights are drawn and how many spots are crowded. The graphics card sorts the lights into the cells of the view each time the camera moves (the processor only lists the lights in front of the camera), so flying through a map with hundreds of lights costs almost no processor time and the view stays smooth on slow processors.
- **Light shadows**: every point and spot light in view casts shadows, the brightest first, so a light never shines through a wall (VRAD shadows every light). **Tools > Options... > Rendering > Shadowed lights** (`render.light_shadows`) sets how many: 0 to 64, 16 by default, 0 for the sun only; past 8 lights the shadow maps are coarser (256 pixels a face instead of 512). A light still waiting for its shadow maps stays dark instead of leaking. Shadow maps are drawn again only when a light or the scene changes: selecting, hovering or recoloring objects does not draw them again. Brush faces of tool textures that block the light (nodraw, `toolsblocklight`) cast shadows even when hidden.
- **Speed**: the Lit mode, the baked preview and the player view stay close to the frame rate of the Textured mode, even with thousands of lights or flickering light styles, with the same picture. See [Editor performance](../troubleshooting/performance.md#lighting-previews-at-full-speed).
- **Slow graphics cards**: with the default performance mode (**Auto**), when the graphics card needs more than 14 ms a frame for the Lit view, the lighting work steps down one level at a time (shadow maps drawn over more frames, fewer soft shadow samples, then smaller shadow maps) and comes back once the card keeps up; a card that keeps up always gets the full look. See [Editor performance](../troubleshooting/performance.md#fine-adaptation-of-the-lighting-auto).
- Props are lit by the same lights, sun and sky light as the brushes, per pixel.
- With **Relief (normal maps)** on (default), the normal maps of the materials give the surfaces their relief.
- A material with `$nodiffusebumplighting 1` keeps a flat diffuse response while its normal map still shapes cubemap reflections. Baked lighting uses a flat lightmap for that material.

Material previews follow the active HDR or LDR mode, including their base texture, normal map and reflection parameters. The player view follows the compiled map, the baked preview follows its bake, and the other editor views follow **HDR light values**. Switching modes refreshes the used materials automatically.

## Baked lighting preview (Ctrl+F5)

Like Hammer++'s "3D Lighting Preview", but the numbers are the compiler's: the map is compiled by the Hammer Pro compiler (the geometry step) and lit by its lighting step (VRAD's) in a separate lighting process that stays warm between bakes: it keeps the game content mounted, the compiled geometry, the ray tracing structures and the last lighting in memory, so the next bake only redoes what changed. Nothing is written to disk: the map and the results travel through a pipe. The compiled faces are drawn with their lightmaps (the four bumped lightmaps of bump mapped faces, for the relief of the normal maps) and shaded exactly like the compiled lighting of the [player view](player-view.md) (the same bump weights, cubemap reflections, self illumination and texture scrolling, the LDR lightmaps of a map compiled without HDR the way the game draws them, the sky seen through the sky faces), so the preview gives the picture of the compiled map in game, and props are lit by the ambient cubes, world lights and static prop vertex lighting of the lit map. The static props follow the compile settings for prop shadows and prop lighting. A prop is lit at its lighting origin like in game: the `lightingorigin` entity when the key names one, else the center of the model.

- When the preview is turned on and nothing is baked yet, a whole-map bake starts by default (setting **Bake when the preview is turned on**). The optional **Bake only around the camera on big maps** setting can restrict this automatic start to a region; it is off by default.
- **Bake lighting** always calculates the whole map, including when the automatic regional option is enabled. Its result stays global as the camera moves.
- Every bake runs in two passes. A **quick pass** first, with the Hammer++ sampling of the map's options (**Sun Samples**, **Ambient Samples** and **Supersample** of the `light_plus` block, 6, 40 and off by default; Draft bounced light; no light probes), shown as soon as it is done: same light levels, a little more noise. Then a **refined pass** at the chosen **Quality** replaces it (progress kind **Refining**); it reuses the compiled geometry and the scene of the quick pass, so only the lighting runs again. The lightmaps come first; the lighting of the props (ambient cubes, world lights, static prop vertex lighting) and the light probes follow a moment later, once edits pause, without blocking the next edit.
- **Light edits are incremental.** Moving a light, changing its color, brightness, falloff or cone, adding, removing, muting or soloing lights, or editing the sun relights only what changed: the lighting process subtracts the old contribution of the changed lights and adds the new one on the faces they reach, then recomputes the bounced light from cached bounce rays, and only the lightmaps that changed are sent to the GPU. No map copy is made and nothing is compiled. The live difference of the real time lights shows the edit at once and the exact baked result replaces it, usually within a fraction of a second; while you drag a light the preview relights its latest position each time the mouse rests (setting **Relight a dragged light after**, 120 ms) and when you drop it. Once you stop, the faces touched by the edits are computed again exactly (the result of a full bake) before the props are lit.
- **Brush edits** compile the geometry again in the warm process (no new process, no disk files, the game content already mounted) and light it with the quick pass, then refine; the faces far from the edit keep the direct light already computed (only the lights whose rays can cross the edited area are traced again on them). Until then an edited solid hides its baked faces; its faces, like the surfaces without lightmaps (water, translucent glass, props without baked lighting), are lit by the sun and the sky color only, without shadows. Brush edits wait 0.3 s without further edits (or the **Update delay** when it is longer); light edits do not wait.
- **Big maps: the visible area first.** With **Bake the visible area first** (`lighting.visible_first`, on), on maps of 6000 solids and more, turning the preview on bakes the area the camera looks at first (around the camera, at least 2048 units and far enough to reach the first surface in front of it, sealed like a cordon) so lighting shows in seconds, then the whole map is compiled and lit in the background and replaces it. When that area holds no brush or most of the map (a camera far above the map, for example), the whole map is baked at once.
- **Prepared in the background.** With **Prepare the baked preview in the background** (`lighting.prepare`, on), once the map is open and its resources are loaded, and whenever edits pause (2 s after opening the map, then 15 s after the last edit, at most once a minute), the lighting process compiles and lights the whole map with the quick pass at low priority, without drawing anything. Turning the baked preview on then shows it at once and the refined pass follows. Light edits made since then are relit on top of it; brush edits made since then are prepared again at the next pause. Nothing is prepared while the baked preview or the Lit mode is on, while the window is minimized or another window is active, in a map without lights, or when **Bake when the preview is turned on** is off. The preparation runs at the lowest priority of the system, so it only uses processor time no other program wants. The lighting process keeps a prepared result for five minutes of inactivity, then closes and gives its memory back; the prepared lightmaps stay in the editor and still show at once. Turn it off on a slow computer to keep its processor and memory free. `get_lighting_status` (MCP) tells in `prepared` whether a result is being prepared or ready.
- **The first bake starts at the camera.** The first bake of a map shows the lightmaps closest to the camera first: a moment after the compile the nearest faces get their direct light, the lit area grows outward in larger and larger steps, and the bounced light then completes the whole picture. Faces still waiting keep their unlit look. Later bakes keep the picture shown until their result is complete.
- **Lights follow the mouse.** Dragging lights with the selection tool (moving any light, or rotating point lights and the sun around a pivot) relights them in the baked preview while the mouse button is held, each time the mouse rests for **Relight a dragged light after** (`lighting.drag_pause_ms`, 120 ms; 0 relights continuously like before 2.0.5), so a long drag does not keep the processor busy: the real time lights show the move in between. The map is not edited until you drop them, and dropping keeps the lighting already computed. Brushes and other entities in the drag wait for the drop.
- **A light edit goes first.** A light edit made while the refined pass runs stops that pass: the edit is shown at once at the quick pass quality, then the refined pass starts again.
- The baked view reuses the calculated lightmaps when the camera moves. Props retain ambient cubes, world lights and static vertex lighting from the bake; materials, models and reflections retain their drawing cost.
- **Solo and mute**: **Solo the selected lights** keeps only the selected light entities in the preview, **Mute the selected lights** leaves them out, **Unmute all lights** brings them back. They are saved as the Hammer++ excluded entities of the map (one undo step each) and relight at once.
- **Measure the cost of each light** reports, for every light, the faces and luxels it reaches in the lighting process (MCP `get_lighting_status` with `costs: true`, most expensive first): the lights that make bakes and relights slow.
- **Bake around the camera** compiles and lights only what lies within the **Radius around the camera** of the 3D camera; the area is sealed like a cordon. This manually chosen region stays at its original position. It is separate from the optional automatic region that follows the camera.
- **Stop baking** stops at once; the lighting process stays ready for the next bake. It is closed, and its memory goes back to the system, when the preview has been off for two minutes (five when it holds a result prepared in the background), when another map is opened, and when the video memory runs short.
- The preview compile only stops on what the preview really needs: limits of the game it cannot store are reported as warnings, the planes of the brushes under displacements (which the BSP does not keep) are left out so a map just over MAX_MAP_PLANES still bakes, and bad `env_cubemap` keys are ignored. A real compile still stops on them, like VBSP.
- The lighting process runs at a lower memory priority so Windows trims its pages first when the memory is short. Its CPU priority never competes with the editor or the other programs: the bakes you wait for (turning the preview on, the refined pass, light edits, lights being dragged, brush edits) run below normal priority (setting **Baking priority**, `lighting.priority`), the preparation in the background and the lighting of the props (ambient cubes, static prop vertex lighting) at the lowest priority, so they never slow down a relight. It uses every core but a quarter of them, and always leaves at least two cores free (setting **Baking threads**). Turning the baked preview off while a bake runs stops it (the lighting process keeps the compiled map): it starts again when the preview is turned back on. The lighting of the props that follows a bake stops too, and runs again when the preview comes back. With **Pause the lighting preview while the window is minimized** (`lighting.pause_minimized`, on) a minimized window stops the running bake and the lighting of the props and starts nothing until it comes back; **Pause the lighting preview while another window is active** (`lighting.pause_unfocused`, off) does the same while you work in another program. `get_lighting_status` (MCP) reports `paused`, `threads` and `priority`. The editor sends the lightmaps to the graphics card a few megabytes per frame (setting **Lightmap upload per frame**, 4 MB), so a big bake never freezes a frame, and while a bake runs it redraws at most ten times a second unless you work; once the result is shown and nothing animates, the editor and the lighting process use no processor time. Moving the camera or changing the selection never starts a bake. The compile of the preview leaves out the collision data and the statistics, frees what it no longer needs while it runs and drops the hidden objects before it starts, so its peak memory is lower than a normal compile of the same map. The lightmap texture is only as large as the lightmaps need (at least 256 by 256 pixels, at most 2048), which saves video memory on small bakes. Large maps are previewed with coarser lightmaps (setting **Full resolution luxels**), and a note says "Large map: previewed at N times the lightmap resolution".
- HDR or LDR comes from the compile settings: the tab shows **HDR** or **LDR** accordingly.

## Reusing a calculated result

Wait for the quick and refined passes to finish, then navigate in the baked view. A whole-map result remains valid when the camera moves or the selection changes. Preview brightness, color correction and the update delay affect presentation or scheduling and do not restart the lighting calculation.

Geometry, materials and physical calculation settings affect the global result. **Dynamic updates** starts a new calculation after relevant edits stop for the configured delay. Light edits (lights, their targets and the sun) are relit incrementally on the shown result, bounced light included. `get_lighting_status` exposes progress, the last result, its stale state and `relight`: the last update, full or incremental, how many lights changed, how many faces were relit and sent, the seconds of the lighting (direct and bounce parts), the turnaround from the edit to the picture, and the memory of the warm lighting process.

The regional option is an explicit choice for automatic preview activation on large maps. It follows the camera after it leaves the central half of the region and stays still for one second, so entering a different area can start another calculation. **Bake lighting** returns to a whole-map result with no camera following. Existing saved values of `lighting.auto_scope` are preserved; a missing value uses the new default, `false`.

The lighting process keeps the compiled geometry of the current map and scope, so light edits, quality changes and the refined pass never compile again. Only the first bake of a map, brush, material, prop and setting changes that affect the geometry compile it again.

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
| **VRAD > No bounce for switchable lights (like VRAD)** | In the VRAD parameters of the tab: named lights light only what they see directly, like VRAD (`-nostylebounce` on the compile); off, their light bounces in their own style and switches with them. MCP: `set_lighting_options switchable_bounce`. | Off |
| **Advanced > Show light reach** | Draws how far the selected lights reach. | Off |
| **Advanced > Restore defaults** | Resets these options. | |

**Color correction**: lookups (`.raw`) applied to the baked preview with a weight each (0 to 1), saved in the map like Hammer++. **From the map** uses the files and maximum weights of the `color_correction` entities; **Clear** removes them.

**Lightmap budget**: a gauge of the estimated size of the lightmaps in the BSP against the 2 GB a face can address in them (the only limit of the game, see [Limits and logs](../compiler/limits-and-logs.md)), with the number of faces and luxels, the faces with a tiny lightmap scale (**Select** selects them), the faces VBSP splits (wider than 32 luxels), displacements too large for VBSP, and **Most expensive faces** (face, scale, luxels, size; click a face to select it).

## Lightmap tools

- **Lightmap grid** (`Alt+8`) draws the luxel grid of every lightmapped face like Hammer's 3D lightmap grid, no bake needed: one checker cell per luxel, as wide as the face's lightmap scale and aligned to its texture axes, displacements included. The color tells the scale: grey at 16 (the default), redder the finer the scale (8, 4 and less stand out), greener the coarser. Yellow lines mark where VBSP splits a face wider than 32 luxels. Once a bake exists, the grid is shaded by its lighting. Faces without a lightmap (nodraw, tool textures, sky, water, unlit materials) and props keep their texture.
- **Select faces with a tiny lightmap scale** selects them, ready for the texture tool.
- The **status bar** shows the lightmap budget ("N / 2048 MB"). Click it to open the Lighting tab.

## Spotlights and dynamic lights

- **`point_spotlight`** works without any setting: with **Start On** checked (the default of the FGD, also used when `spawnflags` is missing) it shows what the game draws, with the formulas of the engine (the `point_spotlight` and beam drawing code of the Source SDK). Its beam is the engine beam: one strip textured with `sprites/glow_test02` (the spotlight material of the game, its cone shape included), drawn additively and turned around the beam axis to face the camera, **Spotlight Width** on each side of the axis (the engine keeps the width at 102.3 at most), from the entity along its angles for **Spotlight Length** or until it hits a brush of the world (triggers, clips, hints and other see-through tool brushes let it pass, like the game's trace), bright at the entity and fading out to nothing at its end. Like in game the beam fades away when you look along it (from four widths away from its axis, gone at one width). A halo (`sprites/light_glow03`, 60 units, up to twice as big near the axis) glows at the entity only when you look at the front of the spotlight, brighter the more you face it, and hides behind the world like the pixel visibility test of the engine. The beam takes the **Color (R G B)** (`rendercolor`) at the beam brightness of the engine (64 of 255), the halo the full color, both times **HDR color scale** (`HDRColorScale`) in HDR; `renderamt` changes nothing, like in game. They show in every 3D render mode and in the player view, and selecting or hovering the entity highlights them.
- Unless **No Dynamic Light** is checked, the spotlight also casts the dynamic light the game puts where the beam ends: a point light in the beam color, just in front of the spot it hits, with a radius that grows with the beam width there (1.8 times 3 times the end width, like the game). It lights the Lit mode, the baked preview and the player view, props included.
- In the player view the I/O simulation plays `LightOn` and `LightOff`: a spotlight without **Start On** lights once `LightOn` reaches it.
- **`light_dynamic`** is a dynamic light of the engine: the compiler never bakes it, so besides the Lit mode it lights the baked preview and the player view over the lightmaps, like in game. **Turn off light** (spawnflags 1 and 2 together) leaves it out.
- Nothing needs to be turned on for this. If a spotlight shows nothing: check **Start On**, that the entity is visible (visgroups, hidden objects), that it is not aimed into a wall right at its origin, and that **Spotlight Length** is not tiny. Its helper cone (shown with the helpers) draws the length and width.

## Cubemaps in the views

With **Cubemap reflections** on (off by default), materials with `$envmap` reflect their cubemap in the 3D view (textured, lit and baked modes) and in the player view. The editor draws a cubemap at every `env_cubemap` (the sky when there is none), a few faces per frame. Each face takes the nearest `env_cubemap` in front of it like the compiler, props the nearest one. The player view uses the cubemaps built in game when the compiled map has them.

- **Off by default**: reflections drawn before the cubemaps are right, or from the sky, make lit surfaces look washed out, so the lighting preview starts without them and shows what the lights do, like Hammer++. Turn them on in **View > Display options > Cubemap reflections**, **Tools > Options... > Rendering**, `set_setting render.cubemaps true` or `run_action toggle_cubemaps` (MCP). Settings saved by an older version are switched off once when this version first starts; turning them on again is kept.
- A face whose cubemap is missing (no `env_cubemap` in the map) or not drawn yet reflects the sky darkened by its own lighting, as if the material had `$envmaplightscale 1`, so dark rooms stay dark instead of showing a bright sky. Faces with a drawn cubemap reflect it like the game: along the bumped normal, times `$envmapmask` (or the base alpha with `$basealphaenvmapmask`, the normal map alpha with `$normalmapalphaenvmapmask`), `$envmaptint`, `$envmapcontrast`, `$envmapsaturation`, `$fresnelreflection` and `$envmaplightscale`.
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
| **Baking threads** (`threads`) | 0 uses every core but a quarter of them (9 of 12). At least two cores always stay free: a larger number is capped to every core but two. Used by the next bake. | 0 |
| **Baking priority** (`priority`) | CPU priority of the bakes you wait for: `Idle` (only when the computer is idle), `BelowNormal`, `Normal` (faster, can slow down other programs). The background preparation and the lighting of the props always run at `Idle`. | BelowNormal |
| **Pause the lighting preview while the window is minimized** (`pause_minimized`) | Stops the running bake and the relights while Hammer Pro is minimized and takes them up again when it comes back. | On |
| **Pause the lighting preview while another window is active** (`pause_unfocused`) | The same while another program is active. | Off |
| **Relight a dragged light after** (`drag_pause_ms`) | While a light is dragged, its baked lighting is computed again once the mouse rests this long (0 to 2000 ms; 0 relights continuously) and when it is dropped. | 120 |
| **Lightmap upload per frame** (`upload_mb_per_frame`) | Lightmap data sent to the graphics card per frame (1 to 64 MB). Lower keeps the editor smoother while a big bake is shown. | 4 |
| **Full resolution luxels (millions)** (`max_luxels_millions`) | Maps with more luxels are previewed with coarser lightmaps (1 to 64). | 16 |
| **Bake only around the camera on big maps** (`auto_scope`) | Opt-in region for automatic preview activation above the threshold. The area follows the camera after it leaves the central half and stays still for a second. **Bake lighting** always stays global. | Off |
| **Big map threshold (millions of luxels)** (`auto_scope_luxels_millions`) | Threshold for the optional automatic regional preview (1 to 64). | 2 |
| **Big map radius** (`auto_scope_radius`) | Radius in units of the optional automatic region (256 to 32768). The manual camera bake has its own radius in the Lighting tab. | 3072 |
| **Bake the visible area first** (`visible_first`) | On maps of 6000 solids and more, the area around the camera is baked first, then the whole map replaces it. | On |
| **Prepare the baked preview in the background** (`prepare`) | Once the map is open and edits pause, the whole map is compiled and lit at low priority so the baked preview shows at once when turned on. | On |
| **Show the lightmap budget in the status bar** (`show_budget`) | | On |
| **Tiny lightmap scale** (`tiny_scale`) | Faces at or under this scale count as tiny (1 to 64). | 4 |
| **Light styles** (`budget_styles`) | Lightmaps per face counted by the budget: 1 without switchable lights, up to 4. | 1 |
| Quality (`quality`) | Set in the Lighting tab: `Draft`, `Normal`, `Final`. | Draft |

Rendering options used here: **Shadowed lights** (`render.light_shadows`, 16), **Bounced light in Lit mode** (`render.lit_indirect`, on), **HDR light values** (`render.light_preview_hdr`, on), **Sky light only under the sky** (`render.sky_occlusion`, on), **Static prop shadows** (`render.prop_shadows`, Auto), **Relief (normal maps)** (`render.normal_maps`, on), **Cubemap reflections** (`render.cubemaps`, off), **Redraw the cubemaps after edits** (`render.cubemaps_auto`, on).

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

In the lit HDR preview, the sky also uses its HDR textures when available, including HDR parameters stored in the ordinary sky material files. LDR and ordinary textured views keep the LDR sky. The real-time sky occlusion includes the overhead surface at the shaded point, preventing the near sampling gap from lighting covered cave floors. The indirect lighting preview continues to provide light entering through openings and reflected light.

## VRAD configuration and retained bake

By default, Lit mode follows compilation HDR/LDR so light_environment values, lighting range and materials use the same range. Changing HDR light values in Settings > Rendering selects a manual comparison. Enable Match compilation HDR/LDR to return to automatic selection.

The VRAD tab inside the Lighting panel configures bounce count, static-prop lighting and shadows, mesh-based prop shadows, texture alpha shadows, displacement shadow disabling, fast ambient sampling and the existing sun/ambient sample settings. Checkboxes select detected light_environment, shadow_control and env_tonemap_controller entities for the preview. Choices are preserved with the map; Bake lighting remains the full-map calculation.

Keep the global bake while editing local lights is enabled by default: light edits update the shown bake incrementally (only the changed lights and the bounced light are computed again, see above), and a signed live direct-light difference of the real time lights shows the edit until the relit lightmaps arrive. Turned off, every light edit relights every light of the map again in the warm lighting process (exact at once, slower on big maps). Geometry and material changes and physical calculation settings still require a new calculation.

This retention also applies to bounced lighting in Lit mode: editing a local light updates its direct contribution immediately and keeps the last indirect-light volume. These bounces are marked stale until the next global calculation.

Local-light edits can also keep the initial calculation running when its geometry and calculation settings remain unchanged. The result keeps its original light snapshot, and the final quality pass still runs.
