[Documentation](../README.md) > Troubleshooting > Editor performance | [Français](../../fr/troubleshooting/performance.md) | [Русский](../../ru/troubleshooting/performance.md)

# Editor performance

Hammer Pro is built to stay smooth on huge maps, but heavy content, a weak GPU or a laptop on battery can still slow the views down. This page lists the settings that matter, from the cheapest to change. For compile times, see [Compile performance](../compiler/performance.md).

## Measuring first

- **Settings > General > Show frame rate** shows the frames per second in the status bar.
- **View > Display options > 3D view information** (`O`) shows the camera position and, for the 3D view, the draw calls, triangles and chunks drawn, and how many chunks culling skipped.

- **Settings > Rendering > Performance > Show frame time and memory in the 3D view** (`render.perf_hud`) adds a readout over the 3D view: FPS and frame time, the CPU time of the render (prepare, record, submit), the GPU time per stage (shadows, scene, post effects, water reflection) when the graphics card supports timestamp queries, the RAM and graphics memory in use, the graphics card and the quality level.
- Through MCP, `get_performance` returns the same numbers, and `run_benchmark` turns the camera through a full circle and returns the average and worst frame times, the CPU and GPU milliseconds and the memory.

Keep the window active for frame rate comparisons: the background frame limit and Windows memory management can change the result when it loses focus. Use the same map, camera, view size, render mode and performance mode. Compare memory after loading, after entering Lit mode and after moving through the map; compare CPU and GPU milliseconds as well as FPS.

## Full quality rendering and memory

The Lit renderer uses a depth pass for opaque world and brush geometry, then shades those surfaces at that depth. Model instances do not use this prepass. Sky light occlusion reuses a per-view table of the same sample distances and mip levels. Light cluster lists discard lights that cannot contribute to the cluster while retaining the existing priority order. These optimizations keep the full quality settings.

**Model levels of detail** (`render.model_lods`) follows the model's Source switch points and its projected size. The CPU avoids submitting unused model levels when it can preserve the existing drawing order; transparent models and ambiguous switch boundaries retain the shader's selection. Turning this setting off keeps the finest model level.

DXT1, DXT3 and DXT5 textures are uploaded in their BC formats when the graphics card supports them. Unsupported formats use the existing RGBA fallback. Decoded upload buffers are released after upload. The model geometry cache uses a 128 MiB budget for unreferenced CPU data, evicts the least recently used data and keeps model information available. Active maps, model variants and current readers keep the data required for selection and editing; this cache budget is separate from total editor RAM and GPU memory.

Hidden water and water outside the camera frustum do not trigger a reflection pass. This culling already existed in 1.1.0. In 1.1.1, the performance readout reports zero reflection time when that pass is skipped, instead of carrying over the previous reflection's sample. Visible water retains its existing reflection quality.

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
| **Auto** (default) | Full quality. Only if the 3D view stays under 20 FPS for several seconds (6 on integrated graphics, 10 otherwise, while nothing is loading) Hammer Pro lowers the quality one step, shows a notice, and offers **Restore full quality** over the 3D view and in the settings. After you restore, the automatic reduction stays paused until you change the mode. It never lowers anything on a graphics card that keeps up. |
| **Off** | Always full quality, never lowered automatically. |
| **Low** | 75% render resolution, no antialiasing, no bloom, 4 shadowed lights, a 2048 sun shadow, textures up to 1024 pixels, water reflects the world only, no sky light occlusion. |
| **Very low** | 50% render resolution, no antialiasing or bloom, no shadows from lights, no reflections, normal maps or fog volumes, a 1024 sun shadow, textures up to 512 pixels, props drawn as boxes beyond 3072 units. |

**Render scale** draws the 3D view at 25% to 100% of its size and stretches it over the window; 100% (the default) is full quality. It works with every mode (the lowest of the two applies).

Nothing here changes the default look: the speed of the default mode comes from drawing less that you cannot see (props and shadow casters outside a view are no longer sent to the graphics card, the sun shadow is kept while the camera moves a little), not from lowering the quality.

### Trying a weaker graphics card

The environment variable `HP_ADAPTER` picks the graphics adapter at startup: `integrated`, `cpu` (the Windows software renderer, a good stand-in for a very weak card) or a part of an adapter name. `HP_PERF_BASELINE=all` switches off the default rendering speed-ups for comparisons. A comma-separated list can isolate them: `instances`, `shadowfit`, `exposure`, `clusters`, `prepass`, `lods`, `skymarch`, `clusterbounds`. `lods` controls CPU submission only; it preserves the selected Source level of detail. These switches are diagnostic controls and do not change the saved quality settings.

## Frame rate settings

Page **Settings > Rendering**, group **Frame rate**:

| Option | What it does | Default |
|---|---|---|
| **Frame rate limit** | Most frames per second (0: no limit; with vertical sync the display limits it). Lower it to save power and heat on laptops. | 0 |
| **In the background** | Frame rate limit while the window is not active (0: no limit). | 30 |
| **Vertical sync** | Waits for the screen refresh. Applies immediately. | On |

## What the 3D view draws

| Setting | Where | Effect on speed | Default |
|---|---|---|---|
| **View distance** | Settings > Viewports | Farthest distance drawn. Shorter is faster. **View > Shorter draw distance** and **Longer draw distance** change it on the fly. | 32768 |
| **Antialiasing (MSAA)** | Settings > Viewports | Smoother edges, uses more GPU memory. Turn it off on weak GPUs. | On |
| **Radius culling** and **Culling radius** | Settings > Rendering, also **View > Display options > Radius culling** and the toolbar | Nothing farther than the radius from the camera is drawn (a circle shows it in the 2D views), like Hammer's radius culling. | Off, 4096 |
| **Skip hidden geometry** | Settings > Rendering | Parts of the map whose objects are all hidden are not drawn at all. | On |
| **Model distance** | Settings > Rendering | Models farther than this are drawn as boxes (0: no limit). A big help on maps with many props. | 0 |
| **Model levels of detail** | Settings > Rendering | Uses the model's Source levels of detail according to its size on screen. Off: every model uses its finest level. | On |
| **Relief (normal maps)** | Settings > Rendering | Off: flat surfaces and less video memory. | On |
| **Cubemap reflections** and **Redraw the cubemaps after edits** | Settings > Rendering | Materials with `$envmap` reflect cubemaps drawn by the editor. Turning off the automatic redraw keeps them until **Map > Lighting > Rebuild cubemaps**. | On, On |
| **Shadowed lights** | Settings > Rendering | In the Lit render mode, how many point and spot lights near the camera cast shadows (0: only the sun, 16 at most). | 4 |
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

## Hiding what you are not working on

The fastest object is one that is not drawn:

- **Hide selected** (`H`), **Hide unselected** (`Ctrl+Shift+H`), the isolate eye of the tool palette, and **Unhide all** (`U`): see [Hide and isolate](../features/hide-isolate.md).
- Turn whole [visgroups](../features/visgroups.md) off.
- Use a [cordon](../features/cordons.md) to work on one area.
- **View > Display options**: turn off models, helpers, particles, ropes, overlays or the 3D skybox when you do not need them.

## Loading time

- **Mount Workshop downloads** (Settings > Game) slows the startup: keep it off unless you need Workshop content.
- Workshop and downloaded addons mount in the background after the rest; the editor is usable meanwhile.
- Very large custom texture folders are indexed once per mount; keep them focused.

## Background work

The live checks (**Check the map as I edit**, **Look for leaks as I edit**, Settings > Map health, both on by default) run in the background a moment after each change. On very large maps on a slow CPU, turning the leak check off saves work; compiles still find leaks. Background precompile (in the compile settings) uses spare CPU to prepare the next compile.

## FAQ

**The 3D view stutters when I fly through a big map.** Turn on **Radius culling**, set a **Model distance** (2048 for example) and check that **Skip hidden geometry** is on.

**The editor uses a lot of GPU while I am in another window.** Lower **In the background** (for example 10).

**Fans spin up even when nothing moves.** Set a **Frame rate limit** such as 60, or keep **Vertical sync** on.

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
