[Documentation](../README.md) > Features > Live link with Garry's Mod | [Français](../../fr/features/live-link.md) | [Русский](../../ru/features/live-link.md)

# Live link with Garry's Mod

The live link is a tool you turn on when you want it: you compile and run the map in Garry's Mod once, then, while the link is on, the entity edits you make in Hammer Pro show in the running game at once, without a recompile. Move a prop and it moves in game, change a light and you see a preview of it, fire a door's `Open` input to test your logic, send the player to the editor camera and back. Brush and lighting changes are listed as needing a compile.

The link is off by default and nothing of it runs until you start it. It works with Garry's Mod in single player and on a listen server you host.

## Commands

All the commands are in **Tools > Integrations** (and in the command palette, `Ctrl+P`):

| Command | What it does |
| --- | --- |
| **Start the live link with Garry's Mod** | Installs the link addon in Garry's Mod, opens a session and opens the status panel. |
| **Stop the live link** | Ends the session: the game stops obeying, its previews and highlights go away. |
| **Live link status...** | Shows or hides the status panel. |
| **Fire input in game...** | Opens the Fire input window (target, input, parameter, delay). |
| **Teleport the player to the 3D camera** | Puts the player's eye where the 3D camera is, looking the same way, in noclip. |
| **Send the player back** | Returns the player where they were before the first teleport. |
| **Player follows the 3D camera** | While on, flying the 3D camera moves the player in game (camera sync, editor to game). |
| **3D camera follows the player** | While on, the 3D camera of the editor follows the player's eye (camera sync, game to editor). |
| **Send all changes to the game again** | Clears what the game was told and sends every change since the compile again. |
| **Remove the live link addon from Garry's Mod** | Stops the link and deletes the addon and its files from the Garry's Mod folder. |

The two camera syncs exclude each other: turning one on turns the other off.

## Getting started

1. Open your map, save it, and compile it (**File > Compile > Compile now**, `F9`).
2. **Tools > Integrations > Start the live link with Garry's Mod**. The status panel opens and says *Waiting for the game*.
3. Run the map in Garry's Mod (**File > Compile > Run the map in game**, or let the compile launch the game).
4. When the game answers, the panel says *Connected* and a toast says *Live link connected to <map> in Garry's Mod*. The top left corner of the game shows *Hammer Pro live link*.
5. Edit: every entity change you make is in the game within a frame or two. Undo and redo are sent too.

If Garry's Mod is already running when you start the link, the link loads itself into the running game (setting **Load the link into a game already running**, on by default), so you do not need to restart the game. A game started after the link loads the addon with the game.

## What shows in game, and how

| Edit in Hammer Pro | In game |
| --- | --- |
| Move or rotate a `prop_physics`, `prop_dynamic` or any point entity | Moved at once (physics props are woken up where you put them). |
| Delete an entity | Removed. Undo creates a copy again. |
| Create an entity (prop, light_dynamic, info_target, logic entities...) | Created and spawned with its keyvalues and outputs. |
| Change keyvalues | `rendercolor`, `renderamt`, `rendermode`, `renderfx`, `skin`, `model`, `modelscale`, `targetname`, `disableshadows`, and every other keyvalue the engine accepts at run time (sounds of `ambient_generic`, speeds, spawn flags of some entities...). |
| Add an output | Added to the entity in game, so the new logic works at once. |
| Change a `prop_static` | Shown as a client side model at the new place, with its model, skin, color and scale, labelled *Hammer Pro: until the next compile*. The compiled static prop cannot be hidden by the engine, so it stays visible until you compile. A deleted static prop is marked with a red box *Removed: compile to update*. |
| Change a `light` or `light_spot` | A preview: a dynamic light for `light`, a projected texture with shadows for `light_spot`, with the color, brightness, cone, pitch and distance of the entity. It is marked *Hammer Pro preview* in game. The baked lighting stays as it is until you compile. |
| Select objects | The selection is highlighted in game: an orange halo around the entities and an orange box around their bounds (brushes included). |
| Change brushes, displacements or `light_environment` | Listed under **Needs a compile** in the panel. |

Entities of the map are found in the game by their Hammer id (the compiler writes it into the map), then by target name, then by class and origin, and brush entities by the order of their brush models. Entities the compiler removes (static props, unnamed lights, `func_detail`, overlays, cubemaps) are not looked for.

## The status panel

**Tools > Integrations > Live link status...**:

| Row | Meaning |
| --- | --- |
| **State** | *Off*, *Waiting for the game*, *Connected*, *Another map is running* (the game runs a map with another name than the open VMF), *Connection lost* (the game stopped answering: closed, paused in the menu, or crashed). |
| **Map** | The map open here and the map running in game. |
| **Addon** | *loaded with the game* (the addon was in the addons folder when the game started), *loaded into the running game*, *installed* or *not installed*. |
| **Latency** | Time between an edit and its confirmation by the game, last value and average. Usually 30 to 100 ms. |
| **Waiting to be applied** | Operations sent but not confirmed yet. |
| **Entities changed since the compile** | How far the map is from the compiled one. |
| **Map entities found in game** | How many entities of the map were matched in the running game. |
| **In game** | Entities created by the link, static prop previews and light previews. |
| **Needs a compile** | Why a compile is needed: brushes or displacements changed, lights changed (previews only), static props changed, brush entities added or removed, outputs removed (the game keeps them), an entity changed class, changed entities that do not exist in the game. |
| **Not sent** | Changes that cannot be sent, with the reason. |
| **Errors** | The last errors reported by the game (for example *entity not found in game*). |

Buttons: **Start**, **Stop**, **Teleport to camera**, **Back**, **Fire input...**, **Send again**, **Game matches the map** (counts the map as it is now as what the game runs, after you compiled and reloaded outside Hammer Pro), **Remove addon**. Below them, the settings of the link.

## Fire input in game

**Tools > Integrations > Fire input in game...** (or **Fire input...** in the panel) tests logic without walking to a button:

- **Target**: a target name of the map (a name ending in `*` matches several), `!player`, or `#` and an entity id (`#42`), which uses the matching above. **Use the selection** fills it from the selected entity.
- **Input**: the input name. When the target is known, the arrow lists the inputs of its class from the FGD (`Open`, `Close`, `Toggle`, `TurnOn`, `Trigger`...).
- **Parameter** and **Delay (s)**.
- **Fire** sends it; the toast says *Open fired on door1 in game*.

## Compiling while the link is on

When you compile and the game reloads the new map, the link notices the new build, counts the map as it was when the compile started as the new state of the game, finds the entities again and sends the edits made since. You can keep the link on for a whole session.

## Settings

In the panel and in `settings.toml` (`[livelink]`), also reachable by MCP (`set_setting`, `get_settings`):

| Setting | Key | Default |
| --- | --- | --- |
| Player follows the 3D camera | `livelink.camera_to_game` | off |
| 3D camera follows the player | `livelink.camera_from_game` | off |
| Highlight the selection in game | `livelink.highlight_selection` | on |
| Preview light changes in game | `livelink.light_previews` | on |
| Load the link into a game already running | `livelink.load_into_running_game` | on |
| Remove the addon when the link stops | `livelink.remove_addon_on_stop` | off |

These settings only act while the link is on.

## How it works and security

- Starting the link writes one Lua file, `garrysmod/addons/hammerpro_livelink/lua/autorun/hammerpro_livelink.lua`, and a copy in `garrysmod/data/hammerpro_livelink/addon.txt` used to load it into a game already running (through the console command `lua_run`, sent to the game window like **Run the map in game** does). `sv_cheats` is not needed.
- Hammer Pro and the game talk through small files in `garrysmod/data/hammerpro_livelink` on this computer only: no network port is opened. The game checks the folder about 30 times per second.
- Every session has a random token. The addon only obeys commands that carry the token of a live session (Hammer Pro refreshes it every second; after 10 seconds without it the addon goes idle and clears its previews). Stopping the link, or closing Hammer Pro, ends the session.
- The addon never runs code it receives: it only moves, creates, removes and configures entities, fires inputs and moves the player. Entities that run console commands or code (`point_servercommand`, `point_clientcommand`, `point_broadcastclientcommand`, `lua_run`) cannot be created, changed or fired at, and the inputs `Command`, `RunCode` and `RunPassedCode` are refused, also inside new outputs.
- **Remove addon** deletes the addon folder and the files of the link; your other addons are never touched.

## MCP

AI assistants drive the link with `live_link_start`, `live_link_stop` (`remove_addon`), `live_link_status`, `live_link_push` (`resync`, `mark_compiled`), `live_link_fire` and `live_link_teleport` (`back`), and the settings with `set_setting` (`livelink.*`). See the [MCP tool reference](../../MCP.md#live-link).

## Tips

- Keep the game in a window (`-windowed` in the launch options) beside Hammer Pro to watch edits arrive.
- Turn on **3D camera follows the player**, walk to a spot in game, turn it off: the editor camera is there.
- After a big change (many entities pasted), **Send again** resends everything in one go.

## FAQ

**Do I need to restart Garry's Mod after starting the link?** No: the link loads itself into a game already running. Restarting is only needed if you turned off **Load the link into a game already running**.

**Why does my moved static prop show twice?** The engine cannot hide a compiled static prop. The link shows the new one; the old one goes away at the next compile.

**Is the light preview the final lighting?** No, it is a dynamic approximation marked as a preview. Compile to bake the real lighting.

**Does it work on a dedicated server?** No: single player and listen servers you host on this computer only.

Problems: see [Live link troubleshooting](../troubleshooting/live-link.md).
