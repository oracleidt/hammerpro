[Documentation](../README.md) > Troubleshooting > Editor performance | [Français](../../fr/troubleshooting/performance.md) | [Русский](../../ru/troubleshooting/performance.md)

# Editor performance

Hammer Pro is built to stay smooth on huge maps, but heavy content, a weak GPU or a laptop on battery can still slow the views down. This page lists the settings that matter, from the cheapest to change. For compile times, see [Compile performance](../compiler/performance.md).

## Measuring first

- **Settings > General > Show frame rate** shows the frames per second in the status bar.
- **View > Display options > 3D view information** (`O`) shows the camera position and, for the 3D view, the draw calls, triangles and chunks drawn, and how many chunks culling skipped.

- **Settings > Rendering > Performance > Show frame time and memory in the 3D view** (`render.perf_hud`) adds a readout over the 3D view: FPS and frame time, the CPU time of the render (prepare, record, submit), the GPU time per stage (shadows, scene, post effects, water reflection) when the graphics card supports timestamp queries, the RAM and graphics memory in use, the graphics card and the quality level.
- Through MCP, `get_performance` returns the same numbers, and `run_benchmark` turns the camera through a full circle and returns the average and worst frame times, the CPU and GPU milliseconds and the memory.
- `get_performance` also shows where the time of the window goes and why it draws: `frame_phases_ms` splits the processor time of the last frame (begin, panels, tool, sync, lighting, views, chrome, dialogs, end), `worst_frame_ms` holds the slowest frame since the previous call, `views_ms` gives the overlay, drawing and label time of each view, and `frame_index`, `frames_from_events`, `continuous_frames` and `repaint_causes` tell which parts of Hammer Pro asked for frames.
- Every frame of 30 ms or more is kept in `get_performance` `long_frames` (frame, time, phases and the subsystems that took the most time in it: a package tick, an action, `scene_sync`, `render_3d`, `render_2d`, `hover_pick`, `placement_checks`, `live_checks`...), and a frame at least twice as slow as usual is written to the log (`long frame ... ms`, see [Crashes and logs](crashes-and-logs.md)): when the editor stutters, the log tells what ran in that frame.
- `get_performance` `gpu_ms.passes` splits the graphics card time of the 3D view by pass: `light_cull` (the light lists of the lighting previews), `sun_shadows`, `local_shadows`, `main` (the view itself, with the depth pass of the Lit mode), `effects` (water, fog volumes, sun glow, refraction), `present` (tone mapping, bloom, display), `outline` and `views_2d` (the graphics card time of the 2D views drawn since the previous frame); `water_reflection_passes` splits the water reflection the same way. `run_benchmark` also returns the 99th percentile of the frame time, of the CPU time and of the GPU time, the average of each pass, and with `trace: true` every frame (frame time, CPU and GPU time, passes, the time of the views and of the whole frame), to find what makes a frame slow.

Keep the window active for frame rate comparisons: the background frame limit and Windows memory management can change the result when it loses focus. Use the same map, camera, view size, render mode and performance mode. Compare memory after loading, after entering Lit mode and after moving through the map; compare CPU and GPU milliseconds as well as FPS.

## 2D views and moving the 3D camera

A 2D view is drawn again only when something in it changes: the map, the selection, its own camera, its size. The 3D camera marker in the 2D views is painted over the kept image, so flying the 3D camera no longer redraws the three 2D views every frame (on big maps that cost more than the 3D view itself). Textures that load or change size while you fly (texture detail by distance) refresh the entity icons of the 2D views at most every 0.75 seconds, and lights, cubemaps, lightmaps, fog and tone mapping never redraw them. The selected edges are drawn only from the parts of the map that hold the selection. In the 3D view the `shadow_control` prop shadows keep their shadow map while the camera moves inside it, like the sun shadow, and are refined when the camera stops. Opening a big map no longer freezes on its entities: they are realized a few milliseconds per frame (props, icons and lights appear over the first frames), moving thousands of brushes re-uploads them over the next frames, the list of resources to load is built on a background thread, and the sky and fog settings are checked again only when the map or the display options change (at most every 0.25 s otherwise). `get_performance` `scene_sync_stages_ms` splits the scene sync of the last 120 frames by stage. `get_performance` `redraw_reasons` counts why each view was drawn again and `view_draws` gives the draw calls of each view.

## Full quality rendering and memory

The Lit renderer uses a depth pass for opaque world and brush geometry, then shades those surfaces at that depth. Since 2.0.5 the opaque parts of props use this depth pass too, so a pixel covered by many props is lit once instead of once per prop. Sky light occlusion reuses a per-view table of the same sample distances and mip levels. Light cluster lists discard lights that cannot contribute to the cluster while retaining the existing priority order. These optimizations keep the full quality settings.

**Model levels of detail** (`render.model_lods`) follows the model's Source switch points and its projected size. The CPU avoids submitting unused model levels when it can preserve the existing drawing order; transparent models and ambiguous switch boundaries retain the shader's selection. Turning this setting off keeps the finest model level.

DXT1, DXT3 and DXT5 textures are uploaded in their BC formats when the graphics card supports them. Unsupported formats use the existing RGBA fallback. Decoded upload buffers are released after upload. The model geometry cache uses a 128 MiB budget for unreferenced CPU data, evicts the least recently used data and keeps model information available. Models of the open map keep their full geometry in RAM only while they are still in use: once the map is loaded, their levels of detail and shadow hulls are built and a model has not been needed for about five seconds (even when the editor stays still), only a compact copy for precise selection stays (positions, triangles and which side faces out, about a third of the size). A new skin or body group, the model browser, a preview or opening another map reads the model again in the background; meanwhile the entity keeps its current look. Model files are read once per model (the MDL is no longer read again for the animation list, and levels of detail reuse the same files). This cache budget is separate from total editor RAM and GPU memory.

Hidden water and water outside the camera frustum do not trigger a reflection pass. This culling already existed in 1.1.0. In 1.1.1, the performance readout reports zero reflection time when that pass is skipped, instead of carrying over the previous reflection's sample. Visible water retains its existing reflection quality.

## Lighting previews at full speed

Since 2.0.5 the Lit mode, the baked lighting preview and the player view stay close to the frame rate of the Textured mode, with the same picture (screenshots of every mode compared pixel by pixel before and after):

- **Props in the depth pass.** In the Lit mode and the baked preview, the opaque parts of props now go through the depth pass before they are lit, like the world. Before, every prop was lit with up to 128 lights even where another prop hid it: on gmc_op_v2, looking across the city drew the 3D view in 82 ms of graphics card time, now 17 ms (the world alone takes 2 ms). A light that cannot reach a pixel is also skipped before the rest of it is read.
- **Light lists.** The lights reaching each cell of the view are listed by the graphics card. Where more than 128 lights reach one cell, the strongest are now picked by a whole group of graphics card threads in 4 passes, instead of one thread in 31 passes. On hokai_d (1,785 lights) this step went from 3.4 ms to 0.2 ms a frame (and as much again for a visible water reflection), which removes the frames of 40 to 90 ms seen while moving around in Lit mode.
- **Flickering lights.** Light styles (flicker, pulse, the patterns of named lights) only update the brightness of their lights 10 times a second. Before, every step rebuilt every light of the map, looked again for the lights buried in walls and drew the 2D views again.
- **Fewer draw calls.** The depth pass and the world of the 3D view are sent as a few multi draws instead of one call per piece of the map, and the sun and light shadow maps merge neighbouring geometry into one call.
- **Nothing created per frame.** The exposure histogram, the bloom and the final display of each view reuse their buffers and bindings, and the exposure readback rides along with the frame instead of its own submission.
- **Player view.** The props the player collides with, the triggers it touches and the counters of the player view (what the game draws) are no longer recomputed from every entity and brush of the map each frame: the props and triggers are listed once per map change, a trigger only checks its own brushes, and the counters are computed again only when the camera or the visibility changes. On rp_archipel_v1 this took 0.6 to 0.9 ms of every frame.
- **Turning the camera.** While a 3D view moves, its sun shadow map covers 1.5 times the area it needs (shadows at most twice softer while moving), so turning reuses it for several frames instead of drawing every prop into it each frame; the views drawn in the same frame share it. As soon as the camera stops, the map is drawn again at full sharpness: a still picture is unchanged. Each 3D view keeps its own state, so two 3D views no longer take the map from each other.
- **Lighting preview quality** (`render.lit_quality`, **Settings > Rendering > Performance**). `Auto` (default) keeps the 32 strongest lights of each part of the view only while the camera moves and draws every light again at rest, `High` always draws up to 128, `Balanced` always 32. A still picture in `Auto` is the `High` one.
- **Hover under a moving camera.** When the camera glides under a still mouse pointer, the surface and the object under the pointer are looked up again only once the camera stops, instead of every frame (3 to 20 ms a frame on gmc_op_v2).
- **Cubemaps.** The editor cubemaps redrawn after a change of render mode are drawn only while the camera rests (400 ms after it last moved) and take at most 15% of the frame time, so navigating right after a mode switch stays smooth.
- **Only the 3D views redraw** for a light style step, and only the view concerned for its auto exposure or for the sharper sun shadow drawn once its camera stops (the 2D views and the other 3D views keep their image).

Each 3D view keeps its own light lists (they follow its camera); the shadow maps of the point and spot lights are drawn once and shared by every view, and every shadow map is kept while nothing it depends on changes. MCP `get_performance` and `run_benchmark` show the cost of each pass (see Measuring first).

### In 2.2.4

Measured on an RTX 3060 Ti at 1920x1080 with the four views, camera orbiting (MCP `run_benchmark`), the old and new build run one after the other:

- **Switching into the Lit mode.** The fingerprint that tells whether a lighting bake can be reused is computed on every core: on hokai_d 537 ms became 135 ms, and the first Lit picture comes 0.38 s after the switch instead of 0.78 s (gmc_op_v2: 0.72 s to 0.31 s). The baked preview counts the light map size of the map once per change instead of twice when it starts.
- **No more freeze when a lighting mode starts.** The shadow map and light list shaders are prepared on a background thread at startup, so the first Lit picture no longer waits for them (on hokai_d the first Lit frame took 163 ms, now 25 ms). After an edit, once a lighting preview was used, the bake fingerprint is computed ahead while the editor is idle, and the next switch to Lit no longer computes it. The copy of the map sent to the lighting process is only made when that process does not already hold the same geometry, and the bake around the camera copies only the world brushes near the camera: switching to Ray traced on hokai_d went from 674 ms to 21 ms of main thread time. When a bake result arrives, the lighting of every prop is computed on all cores: on hokai_d that step went from 390 ms to 55 ms.
- **Light shadows while moving.** While the camera moves, the Lit mode draws 3 shadow faces of point and spot lights per frame instead of 6, and 6 again at rest. The faces still missing when the camera stops are now drawn on their own, before a light could stay dark until the next redraw. On gmc_op_v2 in Lit mode: 31 to 52 FPS while flying, 14.5 to 9.4 ms of processor time and 12.5 to 7.1 ms of graphics card time per frame, 14,014 to 6,124 draw calls.
- **Overlays and decals in the Baked mode.** Their extra lighting pass only runs when an overlay or a decal is in view, and only over the lit geometry next to it. Each batch of light mapped faces is also tested against the view on its own instead of a whole block of 2,048 faces at a time.
- **Lower resolution only when needed.** With **Lighting preview quality** on `Auto`, the Lit, Baked, Ray traced and player views lower the 3D view resolution while the camera moves when the graphics card needs more than 10 ms for what depends on it (the view, its effects and display; shadow maps do not count), down to half,, and draw the full resolution again at rest. A fast graphics card never sees it; a weak one keeps its frame rate. MCP `get_performance` reports the share in use as `lighting_motion_scale`.
- **Diagnostics.** The log reports any 3D view that takes more than 150 ms to prepare with where the time went, and the time each lighting bake takes to prepare on the main thread; `get_performance` long frames name the player view steps and the lighting fingerprints.

The pictures of the Lit and Baked modes are unchanged (screenshots compared before and after: under 0.1% of the pixels differ by more than a rounding).

## Texture detail by distance

**Settings > Rendering > Graphics memory > Load texture detail by distance** (`render.texture_streaming`, on by default) keeps textures used only far from every view at a smaller size. A texture far enough away is only ever sampled from its smaller mip levels, so Hammer Pro drops the larger levels it would never show, one whole level at a time, and keeps one extra level as a margin. The look on screen does not change.

- Distances come from every textured 3D and 2D view (a 2D view only counts what its rectangle shows), from the texture scale of each brush face and from the texture density of each model. Views count as at least 1080 pixels tall, so MCP screenshots up to 1920 pixels stay sharp, and the cubemaps the editor draws for `env_cubemap` count as views too, so reflections keep their detail. Textures never go under 128 pixels.
- When the camera moves closer, the texture comes back to full size before its larger levels would be sampled: the closest ones load first. A texture is made smaller only after it stayed far for 2 seconds.
- While a map opens, the textures of brush faces and models are first read at 256 pixels at most, then every texture close to the opening camera is loaded at full size before the loading screen goes away: what you see first is at full quality, and far textures never take their full size in memory.
- Only the base textures of brush faces and models are reduced. Detail textures, normal maps, masks, water, sprites, decals, overlays, particles, the sky, the player view and textures without mip levels always keep their full size.
- Once textures use more than a comfort budget (an eighth of the texture budget, between 192 and 512 MB), each one is held at the resolution it is seen at on the screen, which looks the same. Animated props count like the other props, so their textures are reduced with distance too.
- On a graphics card with little memory, textures may use 60% of the graphics memory budget: when that is not enough, the farthest textures are reduced one more level until they fit, before Hammer Pro lowers every texture (see [Out of memory](out-of-memory.md)).

Through MCP, `get_gpu_memory` returns `texture_streaming`: the textures held smaller, the graphics memory they use against their full size, and the reduction level. `set_setting render.texture_streaming false` turns it off.

## Calculate lighting, then navigate

**Lighting preview** (`Alt+5`) draws direct lighting in real time. **Baked lighting preview** (`Ctrl+F5`) reuses calculated lightmaps. Choose **Map > Lighting > Bake lighting** and wait for the quick and refined passes to finish before evaluating navigation. This action always calculates the whole map; moving the camera or changing the selection does not invalidate its result.

The optional **Bake only around the camera on big maps** setting is off by default. When enabled, it applies to automatic preview activation and follows the camera, so entering another region can start another bake. Explicit **Bake lighting** returns to a global result. Existing saved choices are preserved.

Geometry, lights, materials and calculation settings can invalidate the result. Preview brightness, color correction and the update delay do not restart the lighting calculation. Use the Lighting tab's progress, last result and stale warning to distinguish preparation from a reusable result. With dynamic updates disabled, use **Bake again** after a relevant change. See [Lighting preview](../features/lighting-preview.md).

## Lighting preview memory

Geometry preparation for the lighting preview runs in a separate process. It receives a snapshot of the mounted folders, VPKs, GMAs and custom texture prefixes in their current priority order. The editor keeps the BSP and face bindings required to display the result, and reuses the prepared geometry for the refined pass. Cancelling the preview stops its worker process.

Bounce rays use a bounded cache: 48 MiB for the preview and 128 MiB with the compiler's default memory budget. Excess ray records are read from temporary storage in tiles of 128 gather points. Sampling positions, ray counts and accumulation order stay the same. Temporary files close and disappear when the lighting process finishes or is cancelled. This bounds the ray cache; lightmaps, geometry, probes and other compiler data use their own memory.

Use the existing lighting actions or MCP `bake_lighting` with `action: "start"`, `"around_camera"` or `"stop"` to start or cancel a preview, and `get_lighting_status` to inspect it. See [Lighting preview](../features/lighting-preview.md) and [Compile performance](../compiler/performance.md).

## Performance mode

**Settings > Rendering > Performance** (`render.performance_mode`, `render.render_scale`):

| Mode | What it does |
|---|---|
| **Auto** (default) | Full quality. Only if the 3D view stays under 20 FPS for several seconds (6 on integrated graphics, 10 otherwise, while nothing is loading) Hammer Pro lowers the quality one step, shows a notice, and offers **Restore full quality** over the 3D view and in the settings. After you restore, the automatic reduction stays paused until you change the mode. It never lowers anything on a graphics card that keeps up. Only frames drawn one after the other count: frames that Hammer Pro spaces out on purpose (an animation at its own pace, an idle window) never lower the quality. |
| **Off** | Always full quality, never lowered automatically. |
| **Lightweight (like Hammer)** | Draws like Hammer at full resolution: plain textures, no antialiasing, bloom, shadows (lights, sun, `shadow_control`), reflections, normal maps, fog volumes, sun glow, 3D skybox or animated textures, textures up to 1024 pixels. Every prop and the sky are still drawn. |
| **Low** | 75% render resolution, no antialiasing, no bloom, 4 shadowed lights, a 2048 sun shadow, textures up to 1024 pixels, water reflects the world only, no sky light occlusion. |
| **Very low** | 50% render resolution, no antialiasing or bloom, no shadows from lights, no reflections, normal maps or fog volumes, a 1024 sun shadow, textures up to 512 pixels, props drawn as boxes beyond 3072 units. |

On a weak computer **Auto** starts in **Lightweight** by itself: integrated graphics, a graphics card with less than 2.5 GB of its own memory, or a laptop on battery (checked every 10 seconds; plugging the laptop in goes back to full quality). A notice over the 3D view says why for 20 seconds, with **Restore full quality**. `get_performance` returns the reason in `low_end` (`integrated_graphics`, `small_graphics_memory`, `on_battery`). `HAMMERPRO_SIMULATE_BATTERY=1` simulates a laptop on battery for tests.

**Render scale** draws the 3D view at 25% to 100% of its size and stretches it over the window; 100% (the default) is full quality. It works with every mode (the lowest of the two applies).

Nothing here changes the default look: the speed of the default mode comes from drawing less that you cannot see (props and shadow casters outside a view are no longer sent to the graphics card, the sun shadow is kept while the camera moves a little), not from lowering the quality.

### Fine adaptation of the lighting (Auto)

In **Auto**, the lighting previews also adapt finely to the graphics card, long before the quality level above changes anything. Hammer Pro measures the time the graphics card spends on each frame of the 3D view (timestamp queries, or the time the frame takes to finish on a card without them). When that time stays above 14 ms for two seconds in a row, the lighting work steps down one level at a time:

| Level | What changes |
|---|---|
| 0 | Full quality. A graphics card that keeps up never leaves it. |
| 1 | The shadow maps of the lights are drawn over more frames (2 faces a frame instead of 6) and the sky light looks around in longer steps. |
| 2 | Soft sun shadows and light shadows take half as many samples. |
| 3 | The sun shadow map is at most 2048 pixels and the light shadow maps 256. |

It climbs back one level once the graphics card stays under 7 ms for four seconds, never sooner than 20 seconds after it stepped down from that level, so the picture does not switch back and forth. **Off**, **Low**, **Very low** and **Restore full quality** turn it off. The frame time readout (`render.perf_hud`) shows the level while it is above 0, and MCP `get_performance` returns `adaptive_lighting` (level, graphics card milliseconds, target, number of changes).

### Trying a weaker graphics card

The environment variable `HP_ADAPTER` picks the graphics adapter at startup: `integrated`, `cpu` (the Windows software renderer, a good stand-in for a very weak card) or a part of an adapter name. `HP_PERF_BASELINE=all` switches off the default rendering speed-ups for comparisons. A comma-separated list can isolate them: `instances`, `shadowfit`, `exposure`, `clusters`, `prepass`, `lods`, `skymarch`, `clusterbounds`, `gpuclusters` (the lights are sorted into the view's cells by the processor instead of the graphics card; `clusters` and `clusterbounds` act on that processor path), `casters` (every geometry change draws the shadow maps again, even a helper that casts no shadow). `lods` controls CPU submission only; it preserves the selected Source level of detail. These switches are diagnostic controls and do not change the saved quality settings.

### Integrated graphics and small cards

On integrated graphics (which share the system memory) and on cards with less than 3 GB, **Settings > Render > Graphics memory** set to **Auto** picks the **Low** mode: a graphics memory budget well below what the card holds, textures up to 1024 pixels from the start, smaller shadow maps, no multisampling, fewer lightmap layers and thumbnails, and uploads to the card in small steps. It keeps a big map with many props and the baked preview from running out of graphics memory. Choose **Normal** to get the usual budget back on such a card, or **Low** on any card to leave room for the game. See [Out of memory](out-of-memory.md#low-graphics-memory-mode).

## Frame rate settings

Page **Settings > Rendering**, group **Frame rate**:

| Option | What it does | Default |
|---|---|---|
| **Frame rate limit** | Most frames per second (0: no limit; with vertical sync the display limits it). Lower it to save power and heat on laptops. | 0 |
| **In the background** | Frame rate limit while the window is not active (0: no limit). | 30 |
| **Vertical sync** | Waits for the screen refresh. Applies immediately. | On |
| **Animations** (`render.animation_fps`) | Frames per second of animated materials, swaying detail sprites, particle effects and flickering lights while nothing else moves (0: every frame). | 30 |
| **Pause animations after** (`render.animation_idle_pause_s`) | Animations stop after this many seconds without mouse or keyboard input and start again at the next input (0: never). | 10 s |
| **Animate in the background** (`render.animate_in_background`) | Keep animations running while another window is active. | Off |

## Idle and animations

When nothing moves, Hammer Pro draws nothing: the window draws a frame only for input, for a change of the map or of the loaded content, for a task that is running, or for an animation (see below). Measured on the same computer with the window untouched for 30 seconds, in percent of one processor core:

| State of the window | gm_construct | hokai_d (73 MB) |
|---|---|---|
| Version 1.2.4, window active (it drew all the time) | 65 to 87% | 50 to 67% |
| Animations stopped: window in the background, after **Pause animations after**, or no animated surface in view | 0.3% | 0.4 to 0.6% |
| Window active with animated surfaces in view (water, scrolling textures), during the first ten seconds after the last input | 15 to 18% | 23 to 30% |

In the last case only the 3D view draws, at **Animations** frames per second (30), for about 5 to 7 ms of processor time per frame on gm_construct and 10 to 12 ms on hokai_d. Lower **Animations**, shorten **Pause animations after** or turn **Animated materials** off to come down to the second line. MCP `get_performance` tells which case applies (`window_focused`, `animations_running`) and the average cost of the frames drawn since the previous call (`average_frame_ms`).

Until version 2.2.3 animations kept running for two minutes after the last input, so a window left alone with water in view drew 30 frames a second for two minutes: about 15% of a core and 20% of the graphics card on gm_construct, 28% of a core on hokai_d. Since 2.2.4 **Pause animations after** is 10 seconds, and a settings file that still has the old default of 120 seconds is moved to 10 seconds once (a value you picked yourself is kept). Measured with the window active and untouched, from 12 to 32 seconds after the last input: gm_construct 0.05 frames a second, 0.5% of a core, 0% of the graphics card (2.2.3: 32 frames a second, 15% of a core, 20% of the graphics card); hokai_d 0.05 frames a second, 0.5% of a core (2.2.3: 30 frames a second, 28% of a core, 19% of the graphics card).

Animated materials, animated models, the sway of detail sprites, the particle effects of entities and the flicker of light styles (in the Lighting and Baked lighting previews) are the only things that move by themselves. They follow the three settings above: they advance at **Animations** frames per second, stop while another window is active (unless **Animate in the background** is on) and stop after **Pause animations after** seconds without input; the next mouse move or key starts them again. A view that animates draws again only when the animation advances, so moving the mouse does not redraw it more often, and the animation of the 3D view does not redraw the 2D views. The spinner of the status bar while Garry's Mod starts turns 12 times a second (it redrew the whole window at the full frame rate before, about 25% of a core) and stops with the animations.

**While a map compiles** the window no longer draws at full speed. Until 2.3.0 the spinners of the compile window redrew the whole window at every frame the graphics card could give, and every line written by the compiler asked for a frame at once: with the window untouched the editor drew 140 to 170 frames a second on rp_archipel_v1 during the visibility step (480 on a faster computer), taking processor and graphics card time from the compiler. Since 2.3.1 the compile log, the progress bars and the step list are updated 8 times a second at most and only when the compiler wrote something, the status bar and toolbar progress 4 times a second, and every spinner of Hammer Pro (compile, loading, model import, map checks, update download...) turns 12 times a second and stops with the other animations. Measured with the compile window open and the window untouched: 4 to 5 frames a second instead of 140 to 170, and still nothing at all when no compile runs. Mouse, keyboard, camera flight, previews and the animations you turned on draw exactly as before, and the frame rate limits of **Frame rate settings** still apply. The FPS shown in the 3D view, the status bar, the view information and the performance readout now counts the frames really drawn during the last second (an idle window reads 1 or 0), where it used to keep the speed of the last burst of frames, so a still window could show 480 FPS.

Animated models only wake the window when one of them is visible in the 3D view and its next pose is due: a model closer than 2048 units moves at up to 30 poses a second, farther ones at 15, 7.5 and 3.75, so a far away animated prop no longer redraws the view 30 times a second. Animations also stop while the window is minimized. Background work started by the editor itself (live map checks, background compiles, content and keyword indexes, thumbnails, placement checks, memory statistics, the content watcher) runs at below normal priority so that the window stays responsive while you work.

## Saving, autosave and undo on big maps

- **Save** (`Ctrl+S`) copies the map in memory and writes the file in the background, so the window stays responsive while a big map is written. The `*` of the title goes away when the file is on disk. Closing the map, closing Hammer Pro or saving the same file again waits for that write first. **Save As...**, the submaps of a manifest and MCP `save_map` write before they return, and a background write that ends after them never points the map back to the older file.
- **Autosave** copies the map a few milliseconds at a time between frames (longer slices while you do not touch the mouse or keyboard), then writes the copy in the background: there is no hitch, even on huge maps. An edit during the copy starts it again; selecting objects does not.
- The **undo history** keeps only what changed. The latest step keeps whole objects so that undoing it stays immediate; as soon as another step follows (selection steps do not count), it keeps only the changed fields of each face and keyvalue, and undoing it later rebuilds the geometry of its objects. An older move of the 26,419 objects of hokai_d keeps 31 MB instead of 171 MB: after three whole-map transforms the history holds 247 MB instead of 506 MB.

## What the 3D view draws

| Setting | Where | Effect on speed | Default |
|---|---|---|---|
| **View distance** | Settings > Viewports | Farthest distance drawn. Shorter is faster. **View > Zoom and draw distance > Shorter draw distance** and **Longer draw distance** change it on the fly. | 32768 |
| **Antialiasing (MSAA)** | Settings > Viewports | Smoother edges, uses more GPU memory. Turn it off on weak GPUs. | On |
| **Radius culling** and **Culling radius** | Settings > Rendering, also **View > Effects > Radius culling** and the toolbar | Nothing farther than the radius from the camera is drawn (a circle shows it in the 2D views), like Hammer's radius culling. | Off, 4096 |
| **Skip hidden geometry** | Settings > Rendering | Parts of the map whose objects are all hidden are not drawn at all. | On |
| **Model distance** | Settings > Rendering | Models farther than this are drawn as boxes (0: no limit). A big help on maps with many props. | 0 |
| **Model levels of detail** | Settings > Rendering | Uses the model's Source levels of detail according to its size on screen. Off: every model uses its finest level. | On |
| **Relief (normal maps)** | Settings > Rendering | Off: flat surfaces and less video memory. | On |
| **Cubemap reflections** and **Redraw the cubemaps after edits** | Settings > Rendering | Materials with `$envmap` reflect cubemaps drawn by the editor. Turning off the automatic redraw keeps them until **Map > Lighting > Rebuild cubemaps**. | On, On |
| **Shadowed lights** | Settings > Rendering | In the Lit render mode, how many point and spot lights in view cast shadows, the brightest first (0: only the sun, 64 at most; past 8 their shadow maps are coarser). | 16 |
| **Sky light only under the sky** | Settings > Rendering | In the Lit render mode, the sky light is checked against a height map of the map around the camera. Off saves a little on slow graphics cards. | On |
| **Water preview** | Settings > Rendering | Refraction, reflection and waves of water. Off: the tool texture. | On |
| **Water detail** | Settings > Rendering | Reflect all and Reflect world draw the scene a second time (at most 1024 pixels wide) for the mirror of expensive water; Simple reflections is the cheapest. | Reflect all |
| **Model fade**, **Render effects**, **Animated materials** | View > Display options (the first two also in Settings > Rendering) | Small costs; turn off when every frame counts. | Off, On, On |

The render mode matters too: **Flat shaded** (`Alt+2`) and **Wireframe** (`Alt+3`) avoid lighting work. **Lighting preview** (`Alt+5`) computes direct lighting during navigation; **Baked lighting preview** (`Ctrl+F5`) prepares lighting first and reuses that result. Materials, models and reflections still contribute to drawing cost. See [Render modes and display options](../features/render-modes-display.md) and [Lighting preview](../features/lighting-preview.md).

## What the 2D views draw

| Setting | Where | Effect | Default |
|---|---|---|---|
| **Models** | Settings > Rendering | **Boxes** (fastest), **Wireframe** (every model), **Wireframe when selected**. | Wireframe when selected |
| Displacements in 2D | Settings > Rendering, **View > Display options > Displacement mesh in 2D views** | The full triangle grid, the grid of the selection only, or the outline. Outlines are lighter on terrain heavy maps. | Full mesh |

The brush center crosses are drawn by the graphics card with the rest of the view and prepared again only after a zoom or an edit, and the entity names are laid out once while a view stays still: on hokai_d zoomed out, a frame of the editor while the mouse moves over the views went from about 36 ms of processor time to about 10 ms, and a frame while panning or zooming the top view from about 60 ms to 12 to 18 ms.

## Hiding what you are not working on

The fastest object is one that is not drawn:

- **Hide selected** (`H`), **Hide unselected** (`Ctrl+Shift+H`), the isolate eye of the tool palette, and **Unhide all** (`U`): see [Hide and isolate](../features/hide-isolate.md).
- Turn whole [visgroups](../features/visgroups.md) off.
- Use a [cordon](../features/cordons.md) to work on one area.
- **View > Display options**: turn off models, helpers, particles, ropes, overlays or the 3D skybox when you do not need them.

## Slow first start (shaders)

Before the views can draw, the graphics card needs the render pipelines of the editor (about 80 per anti-aliasing setting, most built from the large world shader). Since 2.0.8:

- **The window and the panels appear at once.** The pipelines are built on a background thread; until they are ready the 3D and 2D views show "Preparing shaders for this graphics card". Menus, panels, the texture browser and opening a map work meanwhile. Only the pipelines of the current anti-aliasing setting are built first, the others afterwards, so they are no longer built twice at the same time.
- **DirectX 12 keeps its compiled shaders between starts** in `%LOCALAPPDATA%\HammerPro\shaders-dx12`. On DirectX 12 every shader is translated to HLSL and compiled by Microsoft's FXC compiler, which is slow: on an RTX 3060 Ti the first frame took 50 to 58 seconds with DirectX 12 (and about 97 seconds was reported on an Intel UHD Graphics laptop), against 0.4 second with Vulkan. The next starts reuse the cache. Vulkan already kept its own pipeline cache in `pipelines\`.
- **DXC instead of FXC (experimental, off by default).** **Settings > Rendering > Performance > DirectX 12 shader compiler** (`render.dx12_compiler`, applied at the next start): `Fxc` (default) or `Dxc`. DXC needs `dxcompiler.dll` (version 1.8.2502 or newer, from Microsoft's [DirectX Shader Compiler releases](https://github.com/microsoft/DirectXShaderCompiler/releases) or the Windows SDK) next to `hammerpro.exe`; without it FXC is used. On the same machine the views were ready after 8.6 seconds instead of 42, but DXC was not reliable in every test, so it is never picked up on its own, even when the DLL is present. `WGPU_DX12_COMPILER=fxc` or `dxc` overrides the setting.
- **Automatic graphics API.** **Settings > Rendering > Performance > Graphics API** (`render.graphics_backend`, applied at the next start): `Auto` (default) picks the best graphics card, then prefers Vulkan; DirectX 12 is only chosen when the card has no Vulkan driver (the DirectX 12 shader compiler does not change this choice). Before 2.0.8 an integrated Intel card could end up on DirectX 12 with FXC because of the rounded window corners. `Vulkan` or `DirectX 12` forces one (if no graphics card offers it, the automatic choice is used and the log says so). The command line `--backend vulkan` or `--backend dx12` overrides the setting for one session, and `WGPU_BACKEND` overrides both.
- The log line `graphics adapter: ...` names the API and, on DirectX 12, the shader compiler; `startup: 3D and 2D views ready to draw after ... ms` tells when the views could draw and how long each set of pipelines took. MCP `get_performance` returns `graphics_api`, `graphics_api_setting`, `dx12_shader_compiler`, `startup_ms` and `pipelines` (sets with their build time, pipelines created and their total time, and on DirectX 12 the shaders compiled, their total time and the hits and misses of the shader cache).

The shader cache only holds compiled shaders (a few megabytes): delete `shaders-dx12\` or `pipelines\` while Hammer Pro is closed to rebuild them. A driver update needs a new driver compile but keeps the shader cache.

## Loading time

- **Mount Workshop downloads** (Settings > Game) slows the startup: keep it off unless you need Workshop content.
- Workshop and downloaded addons mount in the background after the rest; the editor is usable meanwhile.
- Very large custom texture folders are indexed once per mount; keep them focused.
- Opening a VMF builds brushes, faces, displacements and entity editor data straight from the file text, without an intermediate copy of every key and value: big maps make 50% to 70% fewer memory allocations while loading, for the same result.
- Textures and materials are read from VPK and GMA archives without copying the file first, and a model's MDL, VVD and VTX files are read once per load.
- Live reload of changed materials and models sleeps until Windows reports a change in a loose content folder, instead of checking every watched file every 2 seconds.

## Background work

Since 2.1.1, opening a map no longer loads every processor core. Background loading (materials, models, workshop mounts, content searches, scene building) runs at below normal priority on at most half of the logical processors, always leaving 2 free; the live checks run at the lowest priority on a third of them. The two jobs that used to start on their own after a map was opened are now off by default, and the 2.1.1 update turns them off for existing users too: **Compile while idle** (`compile.background_precompile`) and **Prepare the baked preview in the background** (`lighting.prepare`). The baked preview is then computed when you turn it on. Turn either back on in the settings if you want it; when on, the background compile also uses at most half of the logical processors.

The live checks (**Check the map as I edit**, **Look for leaks as I edit**, Settings > Map health, both on by default) run in the background a moment after each change. On very large maps on a slow CPU, turning the leak check off saves work; compiles still find leaks. Background precompile (in the compile settings) uses spare CPU to prepare the next compile.

The cursor position in the status bar comes from a ray cast into the map under the mouse. On huge maps, where one cast takes several milliseconds, it is cast again at most every four casts' worth of time (120 ms at most) while the mouse moves, and once more where the mouse stops, so moving the mouse over the 3D view does not slow the frames down.

## Panels on large maps

The dock panels and dialogs draw only the rows and thumbnails on screen, so long lists (tens of thousands of materials, models or sounds, every entity of a large map, thousands of problems or console lines) cost about the same as short ones: well under a millisecond per frame each. Searches filter the whole list on each key stroke without allocating per name (about 3 ms for 60,000 materials or 40,000 models); in the sound browser a search that only adds letters filters the previous results (under 1 ms for 50,000 sounds). On very large maps, the outliner (when building its tree takes more than 2 ms) and the target names checked by the entity properties (when checking them takes more than 1 ms) follow continuous edits such as dragging at most four times per second instead of every frame, and are up to date as soon as the edit stops: on hokai_d (30,000 objects) an edit with the outliner open costs 0.3 ms instead of 10 ms per frame.

## FAQ

**The 3D view stutters when I fly through a big map.** Turn on **Radius culling**, set a **Model distance** (2048 for example) and check that **Skip hidden geometry** is on.

**The editor uses a lot of GPU while I am in another window.** Lower **In the background** (for example 10).

**Fans spin up even when nothing moves.** Set a **Frame rate limit** such as 60, or keep **Vertical sync** on. If an animated material is in view, lower **Animations** or **Pause animations after**.

**Hammer Pro uses processor time while I do nothing.** Call MCP `get_performance` twice a few seconds apart: when `frame_index` grows, `repaint_causes` names the source that keeps asking for frames.

## Related pages

- [Render modes and display options](../features/render-modes-display.md)
- [Hide and isolate](../features/hide-isolate.md)
- [Settings reference](../reference/settings.md)
- [Compile performance](../compiler/performance.md)
- [Crashes and logs](crashes-and-logs.md)

---
[Back to the documentation index](../README.md)

## Map primitive overlay

The Σ toggle beside Check Problems shows the active map's solid, world-material, prop-material and light counts over the 3D view. Model variants use their actual selected materials. Pending and missing models are listed separately.

On Windows, category CPU allocations are measured from live heap blocks that own the map's tracked resources, including the overhead reported by Windows. Shared blocks are counted once in a separate row. These values are allocated bytes, not a partition of resident RAM. Driver allocations, the UI framework and undo history are outside the category breakdown. The total resident editor RAM is measured separately by the operating system. An unavailable measurement is shown explicitly. The snapshot updates when map resources change; leaving the overlay enabled does not continuously enumerate the heap.

Cache eviction frees unused data. Idle Windows working-set trimming is no longer used to force the displayed RAM down and cause later page faults. The 2 GB RAM and 1 GB VRAM figures are optimization targets, not fixed limits; usage depends on map content. A DirectX 9-only graphics card does not satisfy the renderer's graphics API requirements.

Nested instances are included in the global solid, material, prop and light counts. Each placement counts separately; shared source geometry and model allocations are measured once. Unresolved instance files are reported explicitly.
