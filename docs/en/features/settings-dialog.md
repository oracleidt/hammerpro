[Documentation](../README.md) > Features > Settings window | [Français](../../fr/features/settings-dialog.md)

# Settings window

The **Settings** window holds every option of Hammer Pro, sorted in pages. Changes apply at once and are saved automatically. This page lists every page and what it contains; the full table of keys, defaults and effects is in the [Settings reference](../reference/settings.md).

## Opening it

- **Tools > Options...** (`Ctrl+,`).
- **Customize...** in the shortcut reference (`F1`) opens the **Keyboard** page.
- Some status bar segments and messages open the page they are about.

## The window

The list of pages is on the left, the selected page on the right. The footer holds:

| Control | What it does |
| --- | --- |
| **Restore defaults** | Resets every option of the page shown. A first click arms it (**Click again to confirm**, for 4 seconds), the second click resets. Every page has it except **Game**. |
| **Apply and reload content** | Shown after you change the game settings: mounts the game content again and reloads the entity definitions with the new settings. Closing the window applies them too. |
| *Changes are saved automatically* | Reminder: there is no OK button. |
| **Close** | Closes the window (`Esc` too). |

Settings are saved in `settings.toml`, in `%APPDATA%\HammerPro` on Windows (see [Files and folders](../reference/files-and-folders.md)). The file is written atomically; if it cannot be read, Hammer Pro starts with the defaults and logs a warning. Theme and language apply immediately.

Only the **Keyboard** page has a search field (**Search actions or shortcuts**). To find an option by name, use the [Settings reference](../reference/settings.md), or the `get_settings` MCP tool.

## The pages

In the order of the list:

| Page | What it contains |
| --- | --- |
| **General** | **Appearance**: language, theme, accent color, interface scale, show frame rate, native window frames. **Background image**: image, opacity, position, zoom, fit, hide it over the 3D view. **Autosave**: save every N minutes, copies kept per map (and the folder). **Maps**: open maps in new tabs. **Recent maps**: how many, **Clear recent maps**. See [Themes and languages](themes-languages.md), [Autosave and crash recovery](autosave-crash.md), [Multiple maps](multiple-maps.md). |
| **Discord** | Discord Rich Presence: show activity, map name, elapsed time, active tool, application ID (empty: Hammer Pro's own), button link, and the **Status:** line (connected, Discord not running...). See [Discord Rich Presence](discord.md). |
| **Game** | The game maps are made for (with **Search again**), the **Garry's Mod folder**, **Content** (mounting options, content downloaded from servers), **Custom texture folders**, **Extra game folders**, **Steam games by AppID**, **Entity definitions (FGD)** (main FGD and extra FGD files), **Maps** (map folder), **Testing in game** (the Hammer Pro bridge and **Spawn at the editor camera**). See [Game setup](../getting-started/game-setup.md) and [Game launch](game-launch.md). |
| **Viewports** | **Controls**: camera controls (Hammer Pro, Hammer, Hammer++), invert mouse X, camera acceleration time, Ctrl + mouse wheel in 2D (grid size or zoom), zoom the 2D views together, grid size step, size units. **Camera**: field of view, speed, fast multiplier, mouse sensitivity, invert mouse Y, camera keys (Auto, AZERTY, QWERTY), view distance, zoom toward the cursor, outline objects under the mouse. **Rendering**: antialiasing (MSAA), grid in the 3D view, highlight grid lines every N, 1024 unit lines. **Colors**: 2D background, 3D background, selection. See [Camera and navigation](camera-navigation.md). |
| **Helpers** | Entity helpers (show, draw for selected or every entity), editor objects, input/output connections (hidden, valid, missing target, unknown output colors), overlays and ropes (show, simulate), entity names. See [Render modes and display](render-modes-display.md). |
| **Rendering** | 3D view (near plane, model distance, grid opacity, culling radius, shadowed lights, HDR light values, sky light only under the sky, static prop shadows, 3D grid on faces, radius culling, skip hidden geometry, sky, 3D skybox, water, normal maps, cubemap reflections, fog, fog volumes and their texture, fish and swimming fish, player view lighting, render effects, model fade, opaque materials, walkable slopes, selection outline, collision models), 2D views (models, fill opacity, icon size, grid density, vertex dots, entity icons, dotted grid, visgroup colors, object colors in 2D and in 3D wireframe...), tool textures hidden in 3D, displacements in 2D, frame rate limits and vertical sync. See [Render modes and display](render-modes-display.md) and [Performance](../troubleshooting/performance.md). |
| **Lighting** | Baked lighting preview (bake when turned on, baking threads, full resolution luxels), lightmap budget (and its status bar gauge), tiny lightmap scale, light styles. See [Lighting preview](lighting-preview.md). |
| **Nav mesh and particles** | Navigation mesh display (show, fill areas, connections) and particles (show, selection only). See [Navigation mesh](nav-mesh.md) and [Particles](particles.md). |
| **Editing** | **New objects**: default material (with **Use current**), texture scale, lightmap scale, point entity, brush entity, color of new brushes. **Behavior**: undo levels, texture lock, texture scale lock, nudge with arrow keys, offset pasted objects, undo restores the selection. |
| **Selection** | Transforms (rotation step, free rotation key, handle size) and behavior (keep visgroups when cloning, Shift+arrows clone, 3D stretch handles, gizmo in local space, camera tool click to zoom). See [Selection tool](selection-tool.md). |
| **Brushes** | Primitives oriented to the drawing view; clipping tool measurements and material of the cut faces. See [Block tool](block-tool.md) and [Clip tool](clip-tool.md). |
| **Vertices and displacements** | Vertex tool live preview and UV lock; displacement 4-way blend painting and noise heights. See [Vertex tool](vertex-tool.md) and [Displacement tool](displacement-tool.md). |
| **Instances** | Contents of `func_instance` (outline, map around an edited instance), default folder of new instances, collapse options. See [Instances](instances.md). |
| **Entities and browsers** | Entity properties (ESC separator, paste name), texture application decimals and drag steps, texture browser (hidden folders, alpha checkerboard), model, sound and particle browsers (Garry's Mod content only, tile size, preview volume, play when selected). See [Entity properties](entity-properties.md) and [Texture browser](texture-browser.md). |
| **Cordons and visgroups** | Cordon material, shading outside the cordons, cordon names; Move to visgroup options. See [Cordons](cordons.md) and [Visgroups](visgroups.md). |
| **Prefabs** | Prefab library folder, more prefab folders, Garry's Mod prefabs, group inserted prefabs, Prefabs tab; Sprinkle definition folders. See [Prefabs](prefabs.md) and [Sprinkle tool](sprinkle-tool.md). |
| **Compile** | **Defaults (F9 and the compile window)**: preset, dynamic range, steps, static props lighting. **Speed**: threads, keep a core for the editor, smart recompile, compile while idle. **Options**: stop when the map leaks, compile hidden objects, cordon only, cordon seal material, pack custom content, development addon, export custom materials, play now, timing history, leak pointfile, launch the game and game arguments. **Extra compiler arguments**, **Compiler options**, **Compiler** (game and maps folder). See [Compile window](../compiler/compile-window.md). |
| **Map health** | Checks (as I edit, leaks as I edit, leak path, visible objects only), limit profile, portal files options. See [Check for problems](check-problems.md). |
| **Keyboard** | Shortcut set, search, reset all, every action with its shortcut. See [Keyboard shortcuts](keyboard-shortcuts.md). |
| **MCP** | MCP server state and switch, start with Hammer Pro, **Install and setup...**, tools exposed to AI apps. See [MCP server](../reference/mcp-server.md). |

Pages added by extensions take their place in the same list (see [Extending Hammer Pro](../development/extending.md)).

## MCP

`get_settings` reads every setting (or a table, or one key) and `set_setting` changes one by dotted path, like `health.live_checks`. See the [MCP tool reference](../../MCP.md).

## Tips

- **Restore defaults** only resets the page shown: experiment on one page without losing the rest.
- After changing the game folder or the content options, click **Apply and reload content** to see the result without closing the window.
- Back up `settings.toml` before a big change; copying it to another computer brings all your options (shortcuts included).

## FAQ

**Is there an OK or Apply button?** No. Every change is applied and saved at once. Only the game content waits for **Apply and reload content** (or the window closing).

**I broke something. How do I reset everything?** Use **Restore defaults** on each page, or close Hammer Pro and delete `settings.toml` (you will see the first run wizard again).

**Why is there no Restore defaults on the Game page?** Resetting it would forget your game folders and content setup; change those options one by one.

## Related pages

- [Settings reference](../reference/settings.md)
- [Files and folders](../reference/files-and-folders.md)
- [Keyboard shortcuts](keyboard-shortcuts.md)
- [Themes and languages](themes-languages.md)

---
[Back to the documentation index](../README.md)
