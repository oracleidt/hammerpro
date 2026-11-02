[Documentation](../README.md) > Troubleshooting > Editor performance | [Français](../../fr/troubleshooting/performance.md)

# Editor performance

Hammer Pro is built to stay smooth on huge maps, but heavy content, a weak GPU or a laptop on battery can still slow the views down. This page lists the settings that matter, from the cheapest to change. For compile times, see [Compile performance](../compiler/performance.md).

## Measuring first

- **Settings > General > Show frame rate** shows the frames per second in the status bar.
- **View > Display options > 3D view information** (`O`) shows the camera position and, for the 3D view, the draw calls, triangles and chunks drawn, and how many chunks culling skipped.

## Frame rate settings

Page **Settings > Rendering**, group **Frame rate**:

| Option | What it does | Default |
|---|---|---|
| **Frame rate limit** | Most frames per second (0: no limit; with vertical sync the display limits it). Lower it to save power and heat on laptops. | 0 |
| **In the background** | Frame rate limit while the window is not active (0: no limit). | 30 |
| **Vertical sync** | Waits for the screen refresh. Applies after a restart. | On |

## What the 3D view draws

| Setting | Where | Effect on speed | Default |
|---|---|---|---|
| **View distance** | Settings > Viewports | Farthest distance drawn. Shorter is faster. **View > Shorter draw distance** and **Longer draw distance** change it on the fly. | 32768 |
| **Antialiasing (MSAA)** | Settings > Viewports | Smoother edges, uses more GPU memory. Turn it off on weak GPUs. | On |
| **Radius culling** and **Culling radius** | Settings > Rendering, also **View > Display options > Radius culling** and the toolbar | Nothing farther than the radius from the camera is drawn (a circle shows it in the 2D views), like Hammer's radius culling. | Off, 4096 |
| **Skip hidden geometry** | Settings > Rendering | Parts of the map whose objects are all hidden are not drawn at all. | On |
| **Model distance** | Settings > Rendering | Models farther than this are drawn as boxes (0: no limit). A big help on maps with many props. | 0 |
| `lod_distances` | `settings.toml`, table `[render]` | Two distances beyond which heavy models use their LOD 1 and LOD 2 (0: always the full model). No UI yet. | `[0, 0]` |
| **Relief (normal maps)** | Settings > Rendering | Off: flat surfaces and less video memory. | On |
| **Cubemap reflections** and **Redraw the cubemaps after edits** | Settings > Rendering | Materials with `$envmap` reflect cubemaps drawn by the editor. Turning off the automatic redraw keeps them until **Map > Lighting > Rebuild cubemaps**. | On, On |
| **Shadowed lights** | Settings > Rendering | In the Lit render mode, how many point and spot lights near the camera cast shadows (0: only the sun, 16 at most). | 4 |
| **Sky light only under the sky** | Settings > Rendering | In the Lit render mode, the sky light is checked against a height map of the map around the camera. Off saves a little on slow graphics cards. | On |
| **Water preview** | Settings > Rendering | Refraction, reflection and waves of water. Off: the tool texture. | On |
| **Model fade**, **Render effects**, **Animated materials** | View > Display options (the first two also in Settings > Rendering) | Small costs; turn off when every frame counts. | Off, On, On |

The render mode matters too: **Flat shaded** (`Alt+2`) and **Wireframe** (`Alt+3`) are the cheapest; **Lighting preview** (`Alt+5`) and **Baked lighting preview** (`Ctrl+F5`) the most expensive. See [Render modes and display options](../features/render-modes-display.md) and [Lighting preview](../features/lighting-preview.md).

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
