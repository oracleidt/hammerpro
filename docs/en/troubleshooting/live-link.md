[Documentation](../README.md) > Troubleshooting > Live link | [Français](../../fr/troubleshooting/live-link.md) | [Русский](../../ru/troubleshooting/live-link.md)

# Live link troubleshooting

When the [live link with Garry's Mod](../features/live-link.md) does not behave, open **Tools > Integrations > Live link status...** first: the **State**, **Needs a compile**, **Not sent** and **Errors** rows usually say what is wrong. AI assistants read the same with the MCP tool `live_link_status`.

## The panel stays on "Waiting for the game"

1. **The map runs in Garry's Mod.** The link talks to a running map, not to the main menu. Run it with **File > Compile > Run the map in game**.
2. **The same map.** The game must run the map named like the open VMF (`gm_mymap.vmf` and `gm_mymap.bsp`). Otherwise the state is *Another map is running*.
3. **The game is not paused.** In single player the game stops while the Escape menu or the console is open, and so does the link. Close the menu.
4. **The game folder.** **Settings > Game** must point at the Garry's Mod you run (the panel and `live_link_status` show `game_folder`). The link writes into `garrysmod/data/hammerpro_livelink` of that folder.
5. **A game started before the link.** The link loads itself into it 3 seconds after starting (the log says *Live link: the addon was loaded into the running game*). If **Load the link into a game already running** is off, or the game window was not found, restart the map from Hammer Pro (**Run the map in game**) or restart the game: the addon is now in the addons folder and loads with the game.
6. **Lua errors in game.** Open the game console (`~`, with `-console` in the launch options): an error mentioning `hammerpro_livelink` means the addon could not run. Use **Remove addon**, then **Start** again to write a fresh copy.

## "Connection lost"

The game stopped answering for more than 3 seconds: it was closed, it crashed, it is paused, or a new map is loading. The link reconnects by itself when the game answers again; when the map was reloaded, it finds the entities again and sends the edits again.

## An edit does not show in game

- **Look at Needs a compile.** Brushes, displacements, `light_environment`, removed outputs and class changes cannot change at run time. Compile.
- **Look at Not sent.** *not_in_game* means the entity of the map was not found in the game (the compiler removed it, or it was renamed and moved since the compile). *brush_entities* means a brush entity was added: it needs a compile. Classes that run commands are never sent.
- **Look at Errors.** *entity not found in game* means the entity was removed in game (by the game logic, or broken). **Send again** resends everything.
- **Map entities found in game** is low: the game runs an older build than the one the link expects. Compile and reload, or press **Game matches the map** if the game already runs the map as it is open.
- **Some keyvalues only act at spawn.** The engine reads a few keyvalues only when the entity spawns (for example the model of some brush entities or a few spawn flags). Delete and undo the entity to recreate it with the new values, or compile.

## The static prop shows twice

Expected: the engine cannot hide a compiled `prop_static`. The link draws the new one as a client side model; the old one goes away at the next compile.

## The light preview does not look like the compiled light

The preview is a dynamic light (`light`) or a projected texture (`light_spot`), an approximation marked *Hammer Pro preview*. Baked lighting, bounces and shadows of the compiled map only change when you compile. Turn off **Preview light changes in game** if you do not want the previews.

## The player moves by itself

**Player follows the 3D camera** is on: every camera move teleports the player. Turn it off in **Tools > Integrations** or in the panel. **Send the player back** returns to where you were before the first teleport.

## The latency is high

Usually 30 to 100 ms. Higher values come from a game running very slowly (low frame rate), a very slow disk for the Garry's Mod folder, or an antivirus scanning `garrysmod/data`. Excluding `garrysmod/data/hammerpro_livelink` from real time scanning helps.

## Removing everything

**Tools > Integrations > Remove the live link addon from Garry's Mod** (or `live_link_stop` with `remove_addon` over MCP) deletes `garrysmod/addons/hammerpro_livelink` and the files in `garrysmod/data/hammerpro_livelink`. To remove it by hand, close Garry's Mod and delete those two folders. Nothing else is installed.
