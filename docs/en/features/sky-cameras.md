[Documentation](../README.md) > Features > Sky cameras | [Français](../../fr/features/sky-cameras.md) | [Русский](../../ru/features/sky-cameras.md)

# Sky cameras

A `sky_camera` sits in a small room (the 3D skybox) that the game draws around the map at its scale. Hammer Pro previews the 3D skybox in the 3D view, handles maps with several sky cameras, and ships a skybox switch entity, `point_hammerpro_skybox`, that changes the 3D skybox in game (day and night, one skybox per area).

## Opening it

| Action | Menu | Default shortcut |
| --- | --- | --- |
| **3D skybox** (preview) | **View > Display options > 3D skybox** | `Alt+V` |
| **Sky preview** (2D sky) | **View > Display options > Sky preview** | none |
| **3D skybox camera** | **View > 3D skybox camera** (shown when the map has two sky cameras or more) | none |
| **Use the previewed sky camera in game** | **View > Display options**, or the **3D skybox camera** submenu | none |

## Previewing the 3D skybox

With **3D skybox** on (default, `render.skybox_3d`), the room around the sky camera is drawn around the map at its `scale`, like in game. **Sky preview** (`render.sky_preview`, on) draws the 2D sky of the worldspawn `skyname` behind it. Sky faces (`toolsskybox`) keep their tool texture unless **See the sky through toolsskybox** is on (**Settings > Rendering**). `Alt+V` shows "3D skybox shown" or "3D skybox hidden".

With **Fog preview** on, the 3D skybox takes the fog of its sky camera (**Fog Enable**, colors, blend, direction, start, end, max density), measured in skybox units, and never the map fog, like in game; see [Map fog](render-modes-display.md#map-fog).

A sky camera without a valid `scale` (a whole number of at least 1) uses 16, the FGD default, and is labelled "invalid" in the menus.

## Several sky cameras

A map may hold several sky cameras, each in its own room. Garry's Mod and the Source 2013 games show the 3D skybox of **the last sky_camera of the map** (the one spawned last). [Check for problems](check-problems.md) reminds you of it when a map has several.

The **View > 3D skybox camera** submenu lists:

- **Like in game: <camera>**: the preview follows the camera the game uses (default);
- every sky camera, named "name (#id, scale N)" or "sky_camera #id (scale N)", the one the game uses marked "used in game". Choosing one previews it (and turns the 3D skybox preview on);
- **Use the previewed sky camera in game**: makes the previewed camera the one the game uses by moving it after the other entities of the map. One undo step. The message "The game now uses <camera>" confirms it.

## The skybox switch: point_hammerpro_skybox

The game shows one 3D skybox at a time. `point_hammerpro_skybox` is a Garry's Mod Lua entity that Hammer Pro adds to the entity list (its FGD is loaded after the game's) to show the 3D skybox of another `sky_camera` when it gets `Activate`.

Set up:

1. Give the `sky_camera` a **Name** (Hammer Pro adds a `targetname` key to `sky_camera` for this).
2. Place a `point_hammerpro_skybox` and set its **Sky camera** to that name. A line joins them in the views.
3. Fire `Activate` from a trigger, a button, a `logic_timer`...

Keys and flags:

| Key | What it does | Default |
| --- | --- | --- |
| **Name** | Name of the switch. | |
| **Sky camera** (`skycamera`) | Name of the `sky_camera` whose 3D skybox this switch shows. | |
| **Mode** (`mode`) | **Global (everyone)**: changes the 3D skybox for every player. **Per player**: drawn on each client, for the player who activated the switch, so several players can see different skyboxes at once. Per player draws the 3D skybox geometry of the chosen camera; the far 2D sky and the fog stay the map's own. | Global |
| **Start active (global)** (spawnflag 1) | Shows this skybox when the map starts (global mode only). | Off |

| Input | What it does |
| --- | --- |
| `Activate` | Shows the 3D skybox of the sky camera. |
| `Deactivate` | Shows the map's own 3D skybox again, when this switch is the one shown. |
| `Toggle` | Activate when inactive, else deactivate. |

| Output | When |
| --- | --- |
| `OnActivated` | The skybox of this switch is shown. |
| `OnDeactivated` | The skybox of this switch stops being shown. |

When the map compiles, each switch gets the keys of its sky camera (`sky_origin`, `sky_scale`, fog keys...), so it works in game without looking the camera up. The fog keys of the camera are copied in global mode.

### Shipping the script

The switch needs its Lua in game (`lua/entities/point_hammerpro_skybox.lua` and `lua/autorun/hammerpro_skybox.lua`). Hammer Pro ships it for you:

- test compiles put it in the **Development addon** (Compile settings);
- **Build and save as content** adds it to the addon when the compiled map has a switch.

The compile log tells you when a map with switches is compiled without the development addon. [Check for problems](check-problems.md) warns about a switch whose sky camera does not exist and about per player switches (if the server runs an addon that also draws the sky, a weather addon for example, they can conflict).

## Tips

- One map, day and night: two skybox rooms, two named sky cameras, two switches, fired by a `logic_timer` or a button.
- To change which camera the game uses without a switch, preview it and use **Use the previewed sky camera in game**.
- MCP tools: `list_sky_cameras`, `preview_sky_camera`, `use_sky_camera_in_game`, `create_skybox_switch` (see the [MCP tool reference](../../MCP.md)).

## FAQ

**My game shows the wrong 3D skybox.** The game uses the last `sky_camera` of the map. Preview the right one in **View > 3D skybox camera** and choose **Use the previewed sky camera in game**.

**The switch does nothing in game.** Its Lua is missing: turn on **Development addon** for test compiles, or release with **Build and save as content**. Also check that **Sky camera** matches the camera's **Name**.

**Does the switch work in other games?** No. It is a Garry's Mod Lua entity.

## Related pages

- [Render modes and display options](render-modes-display.md)
- [Build and save as content](build-content-release.md)
- [Compile and run window](../compiler/compile-window.md)
- [Entity properties](entity-properties.md)

---
[Back to the documentation index](../README.md)
