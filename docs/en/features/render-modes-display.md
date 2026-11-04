[Documentation](../README.md) > Features > Render modes and display options | [Français](../../fr/features/render-modes-display.md)

# Render modes and display options

How the views draw the map: the render mode of the 3D view, the many switches of **View > Display options**, background reference images in the 2D views, the draw distance and the frame rate limits.

## 3D render modes

Choose the mode in **View > 3D render mode**, with the render mode list of the toolbar, or with the keys:

| Mode | Shortcut | What you see |
| --- | --- | --- |
| **Textured** (default) | `Alt+1` | Materials with the editor's shading. |
| **Flat shaded** | `Alt+2` | Plain colors per object, shaded. |
| **Wireframe** | `Alt+3` | Edges only. |
| **Textured + mesh edges** | `Alt+4` | Materials with the triangle edges drawn over them. |
| **Lighting preview** | `Alt+5` | The Lit mode: the light entities light the map in real time, with shadows (see [Lighting preview](lighting-preview.md)). |
| **Textured, unshaded** | `Alt+6` | Materials without the editor shading. |
| **Smoothing groups** | `Alt+7` | Faces colored by smoothing group. |
| **Lightmap grid** | `Alt+8` | Hammer's 3D lightmap grid: the luxel grid of every lightmapped face (cells as wide as its lightmap scale, displacements included), grey at 16, red when finer, green when coarser; no bake needed. Faces without a lightmap keep their texture. |
| **Baked lighting preview** | `Ctrl+F5` | The lighting the game will have, computed by the built-in compiler (see [Lighting preview](lighting-preview.md)). |

**View > View type** also has **3D textured view** (`F5`) and **3D flat view** (`Shift+F5`). The mode is remembered between sessions (`view.render_mode`).

## Show menu entries

Directly in the **View** menu:

| Entry | What it does | Default |
| --- | --- | --- |
| **Show models** | Draws props as models (off: boxes). | On |
| **Show helpers and sprites** | Entity helpers (radii, cones, lines to targets) and sprites. | On |
| **Show tool textures** | Faces with tool textures in the 3D view (see [Hide and isolate](hide-isolate.md)). | On |
| **Background images...** | Reference images behind the 2D views (below). | |

## Display options

**View > Display options**. Each entry is a switch; most are also on the **Rendering** page of the settings (`[render]` table).

| Entry | What it does | Default |
| --- | --- | --- |
| **3D view information** (`O`) | Frame rate, camera position and angles in the 3D view. | Off |
| **Bounding boxes** (`I`) | Bounding boxes of the visible objects. | Off |
| **Show overlays** | Overlays drawn on their faces (see [Decals and overlays](decals-overlays.md)). | On |
| **Show connections** | Input/output lines: off, selected, all (each use goes to the next). See [I/O and logical view](io-logical-view.md). | Selected |
| **Show editor objects** | Entity icons and helper geometry that only exist in the editor. | On |
| **Show point entities** | Every point entity. | On |
| **Show entity names** | Names next to point entities in the 2D views. | On |
| **Helpers of every entity** | Helpers of every entity instead of the selected ones only. | Off |
| **Show ropes** / **Simulate ropes** | Ropes as they hang in game; simulated ropes swing while they settle. | On / Off |
| **Render effects** | Render amount, mode and color, body groups, default animation and scale of entities. | On |
| **Sky preview** | The 2D sky (worldspawn `skyname`) behind the 3D view. | On |
| **3D skybox** (`Alt+V`) | The room around the `sky_camera`, at its scale, around the map (see [Sky cameras](sky-cameras.md)). | On |
| **Collision models** | Collision hulls drawn over the props (also in the model browser preview). | Off |
| **Show model collisions** | The 3D view shows the collision models instead of the models (a model without one shows its box in orange). | Off |
| **Fog preview** | Submenu: **Off**, or the fog of one `env_fog_controller` of the map (by default the one the game uses: the last **Master** one, else the first; a controller with **Fog Enable** at No is marked). Also a toolbar button. See [Map fog](#map-fog). | On |
| **Animated materials** | Animated materials and sprites play. | On |
| **Model fade** | Props fade between their fade distances, like in game. | Off |
| **Opaque materials** | Translucent materials drawn opaque. | Off |
| **Walkable slopes** | Tints the surfaces too steep for players to stand on. | Off |
| **Show displacements** | Displacements are drawn. | On |
| **Displacement mesh in 2D views** | Switches the 2D views between the triangle grid of displacements and their outline. | Mesh |
| **Displacement walkability** | Colors displacement triangles by walkability (see [Displacement tool](displacement-tool.md)). | Off |
| **Radius culling** | Nothing farther than the culling radius from the camera is drawn (a circle shows it in the 2D views). | Off |
| **Highlight invalid brushes** | Brushes that are not closed and convex are drawn in red in every view. | On |
| **Show nodraw faces** | Shows or hides the nodraw faces in the 3D view. | Shown |
| **Water preview** | Water drawn like in game: refraction, water fog, reflection of the scene and the sky, moving waves. Off: its tool texture. | On |
| **Water detail** | Like the game's Water Detail option (`render.water_detail`): **Reflect all** draws expensive water at every distance, reflecting the world, props, brush entities, the sky and the 3D skybox drawn again mirrored about its surface; **Reflect world** keeps the expensive water near the camera and reflects props and brush entities only for materials with `$reflectentities`; **Simple reflections** draws cheap water. | Reflect all |
| **Fog volume preview** | Each `fog_volume` is outlined and filled with its fog, with a label of its links and problems (see [Fog volumes](#fog-volumes)). Also a toolbar button. | On |
| **Fog volume texture** | The material of the `fog_volume` brushes is drawn in the 3D view, like Hammer. Off: only the fog shows. | On |
| **Fish preview** | The fish of each `func_fish_pool` (see [Fish pools](#fish-pools)). | On |
| **Swimming fish** | The fish swim slow circles. Off: they stay still. | On |
| **Cubemap reflections** | Materials with `$envmap` reflect their cubemap (see [Lighting preview](lighting-preview.md)). | On |
| **Use the previewed sky camera in game** | See [Sky cameras](sky-cameras.md). | |
| **Show particles** / **Particles of the selection only** | See [Particles](particles.md). | On / Off |
| **Show visible clusters** | See [Leaks and pointfiles](leaks-pointfiles.md). | Off |
| **Show nav mesh** | See [Navigation mesh](nav-mesh.md). | Off |
| **Player view** (`F6`) | See [Player view](player-view.md). | |

The View menu also has **3D skybox camera** (when the map has several sky cameras) and **Hidden tool textures** (nodraw, clips, triggers, hints and skips hidden in the 3D view).

The toolbar repeats the most used switches: radius culling, displacement mesh, displacement walkability, nodraw, model collisions, helpers, models, tool textures, editor objects, 3D skybox, particles, simulate ropes, the render mode list and the player view.

## Map fog

**Fog preview** (on by default, toolbar button, **View > Fog preview**) draws the fog of an `env_fog_controller` in the 3D views and the [player view](player-view.md), like the map fog in game: **Primary Fog Color** (and **Secondary Fog Color** with **Fog Blend**, along **Primary Fog Direction** or the angles with **Use Angles for Fog Dir**), **Fog Start**, **Fog End**, **Fog Max Density** and **Far Z Clip Plane**. Editing a key updates the view at once. It works in the Textured, Lit, Flat and Unshaded modes, on baked lightmaps, water, particles, detail props and models, and in the player view.

- The fog is computed like the game: it grows linearly with the distance along the view direction, from 0 at **Fog Start** (which may be negative: the fog is already there at the eye) to **Fog Max Density** at **Fog End**, and it is mixed in gamma space with the drawn color. A max density of 0.25 never covers more than a quarter of the color.
- With **Fog Blend**, the fog color depends on where the camera looks, not on each pixel: looking along **Primary Fog Direction** gives the primary color, looking the other way the secondary one, half of each side on.
- Past **Far Z Clip Plane** nothing is drawn, like the far clip plane of the game: the 2D sky shows through (when **Sky preview** is on), else the fog color, which fills the background of the 3D views while the fog shows, like the clear color of the game. The 2D sky is never fogged, like in game.
- Additive materials and particles fade to black in the fog, modulate decals fade to no effect.
- The 3D skybox does not take the map fog: it takes the fog keys of its `sky_camera` (**Fog Enable**, colors, blend, direction or **Use Angles for Fog Dir**, start, end, max density), measured in skybox units from the camera, like in game. A `sky_camera` with **Fog Enable** at No leaves the 3D skybox clear. **Fog preview** off turns both off.
- MCP: `list_fog` gives `preview.fog`, `preview.sky_camera` and `preview.sky_fog`.

- The submenu lists the controllers of the map; without a choice the game's choice is used: the last one with the **Master** flag, else the first one.
- A controller with **Fog Enable** at No gives no fog, like in game, and the 3D view shows a warning at its bottom left.
- Selecting a controller shows in the entity properties whether it is the one previewed and how many `fog_volume` use it.

## Fog volumes

A `fog_volume` changes the fog of the players whose eyes are inside it: the game then uses the `env_fog_controller` named by its **Fog Name** instead of the map fog, and the `postprocess_controller` and `color_correction` named by **PostProcessName** and **Color Correction Name**. Like the game, the volume is the box around all its brushes, whatever their shape.

With **Fog volume preview** on (the default), every `fog_volume` is visible in the 3D view as soon as you place it:

- its brushes keep their material (`tools/toolsfog` or another, see **Fog volume texture**) and the box is outlined in the color of its fog;
- the box is filled with its fog, as thick as the view crosses it, so what is inside or behind the volume is fogged like from inside it;
- a label above the box names the volume and its controller (`fog_volume > cave_fog`) and lists in orange what is wrong. Select the volume to see every detail and lines to the controller, post process and color correction entities.

The fog drawn in the box:

| Fog Name | Fog in the 3D view | In game, eyes inside |
| --- | --- | --- |
| Names a controller with **Fog Enable** at Yes | That controller's fog: colors, blend, start, end, max density | That fog replaces the map fog |
| Names a controller with **Fog Enable** at No | The default look (light blue) and the warning *has its fog disabled: clears the fog in game* | No fog |
| Names nothing, or no **Fog Name** | The default look and *No controller "name"* or *No Fog Name* | The map fog stays |

- **Fog Name** can end with `*` to take the first controller whose name starts that way.
- **PostProcessName** and **Color Correction Name** are checked: a missing entity or one of another class is listed in orange; the effects themselves are not previewed.
- Editing the keys of the volume or of its controllers updates the preview and the label at once.
- In the [player view](player-view.md), the fog of the controller replaces the map fog while the eyes are inside the volume, even with **Fog preview** off; a controller with **Fog Enable** at No clears it; a volume with **Start Disabled** does nothing there, like in game. The labels and outlines are hidden there.
- **Fog volume texture** off hides the brush material in the 3D view (the outline and the fog stay).
- MCP: `list_fog` reports every volume with its resolved fog, `preview_fog` chooses the previewed controller.

## Fish pools

A `func_fish_pool` spawns `fish_count` fish (default 10) of its **World model** (`models/Junkola.mdl` by default) that swim up to **Max Range** units (default 150, at most 255) from the entity. With **Fish preview** on, the 3D view draws them scattered within that range, always at the same places for the same pool. When the entity is in water, the fish stay in that water, at least 8 units under its surface, like the game keeps them under the water level. With **Swimming fish** on they swim slow circles around their place.

- The pool itself shows as a box; select it to see a sphere and a circle of its **Max Range**.
- The preview draws at most 256 fish per pool.
- A model the game content does not have (Garry's Mod has no `models/Junkola.mdl`) is previewed with the goldfish of the game content, `models/props/de_inferno/goldfish.mdl`, so the pool shows fish out of the box. In game such a pool shows no fish at all, so the entity properties of the pool warn about it: set that model on the pool to get the same fish in game. When even that model is missing, each fish is a small box in the color of the entity.

## Background images

**View > Background images...** opens the **Background images** window: reference images (a floor plan, a photo) drawn behind the grid of the 2D views.

| Field | What it does |
| --- | --- |
| **Add an image...** | PNG, JPEG, BMP or TGA. |
| **View** | **Top**, **Front** or **Side**: the 2D views of that axis show it. |
| **Center** | Position of its center, in view coordinates. |
| **Units per pixel** | Scale (0.001 to 1000). |
| **Rotation** | Degrees. |
| **Opacity** | 0 to 1. |
| **Remove this image** | Removes it. |

The images are stored in the map (`bgimages_plus` block of the VMF), so they travel with it. Files are only read, never written.

## Draw distance and culling

| Option | Where | What it does | Default |
| --- | --- | --- | --- |
| **View distance** (`view.far_plane`) | Settings > Viewports | Farthest distance the 3D view draws (2048 to 262144). | 32768 |
| **Shorter draw distance** / **Longer draw distance** | **View** menu | Divide or multiply the draw distance by 1.5. | |
| **Near plane** (`render.near_plane`) | Settings > Rendering | Closest distance drawn, 0.5 to 16. Raise it if far surfaces flicker. | 1 |
| **Model distance** (`render.model_distance`) | Settings > Rendering | Models farther than this are drawn as boxes (0: no limit). | 0 |
| `render.lod_distances` | `settings.toml` | Heavy models use their LOD 1 and LOD 2 beyond these distances (0: always the full model). | 0, 0 |
| **Culling radius** (`render.cull_radius`) | Settings > Rendering | Radius of the radius culling (256 to 65536). | 4096 |
| **Skip hidden geometry** (`render.occlusion_culling`) | Settings > Rendering | Parts of the map hidden behind closer geometry are not drawn. | On |
| **Antialiasing (MSAA)** (`view.msaa`) | Settings > Viewports | Smoother edges, more GPU memory. | On |

## Frame rate

| Option | What it does | Default |
| --- | --- | --- |
| **Frame rate limit** (`render.max_fps`) | Most frames per second (0: no limit; with vertical sync the display limits it). | 0 |
| **In the background** (`render.unfocused_fps`) | Limit while the window is not active (0: no limit). | 30 |
| **Vertical sync** (`render.vsync`) | Waits for the screen refresh (applies after a restart). | On |

## Other Rendering settings

| Option | What it does | Default |
| --- | --- | --- |
| **See the sky through toolsskybox** (`sky_see_through`) | Hides the `toolsskybox` faces so the sky previews show through them. | Off |
| **Relief (normal maps)** (`normal_maps`) | Normal maps give the surfaces their relief in the 3D view. Off: flat surfaces, less video memory. | On |
| **Selection outline** (`selection_outline`) | Selected objects get an outline visible through walls. | Off |
| **Models** (2D views, `models_in_2d`) | **Boxes**, **Wireframe**, **Wireframe when selected**. | Wireframe when selected |
| **Fill selected brushes** / **Fill opacity** | Selected brushes filled in the 2D views (opacity 0.02 to 0.6). | On, 0.2 |
| **Vertex dots** | Brush vertices drawn as dots when zoomed in. | On |
| **Entity icons** / **Icon size** | Entity icons in the 2D views (8 to 48 px). | On, 16 px |
| **3D camera** | Shows the 3D camera in the 2D views. | On |
| **Visgroup colors** | Brushes drawn in the color of their visgroup. | Off |
| **Use object colors in 2D** (`object_colors_2d`) | Brushes and entities drawn in their display color (see [Display colors](display-colors.md)); off: the classic look. | On |
| **Object colors in 3D wireframe and flat** (`object_colors_3d`) | The 3D view uses the display colors in wireframe and flat mode. | Off |

## Tips

- On a big map, **Model distance**, **Radius culling** and a lower **View distance** keep the frame rate up (see [Editor performance](../troubleshooting/performance.md)).
- The background frame rate limit (30) saves power while you are in another program.
- MCP tools: `set_render_mode`, `screenshot_view`, `get_views`, `set_setting` (see the [MCP tool reference](../../MCP.md)).

## FAQ

**My sky faces show the tool texture.** That is the default. Turn on **See the sky through toolsskybox** to see the sky through them.

**Where did my props go?** Check **Show models**, **Model distance** and **Show model collisions**.

**The fog preview shows nothing.** The map needs an `env_fog_controller` with **Fog Enable** at Yes (a new one has No, like in Hammer); the 3D view says so at the bottom left. Choose another controller in **View > Fog preview**.

**My fog_volume shows the default blue look.** Its **Fog Name** does not name an `env_fog_controller` with **Fog Enable** at Yes: the label above the volume and the entity properties say which. In game such a volume changes nothing (no controller) or clears the fog (controller with its fog disabled).

## Related pages

- [Lighting preview](lighting-preview.md)
- [Sky cameras](sky-cameras.md)
- [Camera and navigation](camera-navigation.md)
- [Editor performance](../troubleshooting/performance.md)
- [Settings reference](../reference/settings.md)

---
[Back to the documentation index](../README.md)
