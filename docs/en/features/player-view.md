[Documentation](../README.md) > Features > Player view | [Français](../../fr/features/player-view.md) | [Русский](../../ru/features/player-view.md)

# Player view

The player view lets you walk the map in the 3D view like a Garry's Mod player: real movement, collisions, doors, trains, triggers, the compiled world with its lightmaps and counters of what the game would draw. Nothing in the map changes while you play.

## Opening it

- **View > Player view**, the player button of the toolbar, or `F6`.
- It takes the 3D view under the mouse (else the focused one, else the first 3D view). The layout needs a 3D view ("The player view needs a 3D view in the layout.").
- You start at the first `info_player_start` (or `info_player_deathmatch`, `info_player_combine`, `info_player_rebel`, `info_player_terrorist`, `info_player_counterterrorist`), facing its direction; without one, on the ground under the 3D camera.
- `Escape` or `F6` goes back to the editor, with the camera where it was before.

## Keys

| Key | What it does |
| --- | --- |
| Mouse | Look around. |
| `W` `A` `S` `D` (`Z` `Q` `S` `D` on AZERTY with the **Auto** camera keys) | Move. |
| `Space` | Jump. |
| `Ctrl` | Crouch. |
| `Shift` | Run. |
| `V` | Noclip: fly through walls. |
| `C` | Third person camera behind the player model. |
| Mouse wheel | Distance of the third person camera. |
| `M` | Next player model (switches to third person). |
| `X` | `mat_wireframe` 0, 1, 2, 3 in turn (below). |
| `E` | Use: open or close doors, press buttons, use props, start or stop the train you look at. |
| `T` | Start or stop every train of the map (below). |
| `N` | Shows the tool textures (nodraw, clips, triggers), hidden by default like in game. |
| `B` | Compiled world or editor brushes. |
| `L` | Lightmaps of the compiled map, or the editor shading. |
| `K` | Hearing: plays the active soundscape and the `ambient_generic` sounds with distance, like the engine (off by default, see [Soundscapes](soundscapes.md)). |
| `H` | Hides or shows the counters. |
| `1` to `9` | Turn a named light on or off. |
| `Escape`, `F6` | Leave. |

The keys line at the top of the view reminds you of them.

## Movement

The movement uses the Source values of Garry's Mod: 200 units per second walking, 400 running, gravity 600, jump speed 200, a 32 x 32 x 72 hull (36 crouched), eyes at 64 (28 crouched), steps up to 18 units. Collisions use the brushes (triggers, hints, skips, fog, area portals, occluders, invisible ladders and 2D sky faces are walked through) and the collision models of props (`.phy`, like the game: a tree only blocks at its trunk). The player pushes the physics props it walks into. The player model plays the Garry's Mod animations (idle, walk, run, crouch, jump, noclip).

**Displaced brushes** behave like in game: the compiler (like VBSP) keeps only the displacement surface of a brush that has one, so its other faces are neither drawn nor solid. The player view follows it with the compiled world and with the editor brushes alike: the sides of a displaced block are not drawn, walking and the use key go through them, and a slit between two displaced blocks stays open, exactly as in Garry's Mod. The editor views draw displaced brushes the same way. A brush whose displacement cannot be built (a face without exactly 4 corners after an edit, a power outside 1 to 4) is dropped entirely by the compiler: the player view hides it and walks through it, the editor views draw it in red and **Check problems** reports and fixes it.

## Compiled world and lighting

- After a compile, the view draws the **compiled world** (the faces as the compiler cut them) instead of the editor brushes. `B` switches back to the editor brushes. Without a compile: "Editor brushes: compile the map to see the world as the game cuts it". When the map changed since the compile, a line asks you to compile again.
- `L` lights the compiled world with the lightmaps of the compiled map and the props like the game (setting **Player view: compiled lighting**, `render.player_compiled_lighting`, on by default): static props by their baked vertex lighting (`sp_*.vhv`) when the map has it, else by the ambient cube and world lights at their lighting origin, the other props at the `lightingorigin` entity when the key names one, else at the center of their model. A map compiled without lighting is fullbright, like in game.
- Starting the player view prepares the compiled world on several processor cores at once. Its lightmaps keep the precision of the BSP (a shared exponent per texel, 4 bytes) in an atlas only as large as they need, and the world is drawn material by material from a few large buffers: the faces the game would not draw from where you stand (visibility, area portals) are left out before they reach the graphics card.
- The **light styles** of the compiled map play like in game (ten pattern letters a second). Named switchable lights follow their inputs and the keys `1` to `9`.
- Cubemap reflections use the cubemaps built in game when the compiled map has them, else the ones Hammer Pro draws.
- With **Fog preview** on (the default), the map fog of the previewed `env_fog_controller` applies, like in game (none when its **Fog Enable** is No; see [Map fog](render-modes-display.md#map-fog)).
- While the eyes are inside a `fog_volume`, the fog of the `env_fog_controller` named by its **Fog Name** replaces the map fog, like in game (the box around its brushes, volumes with **Start Disabled** skipped; setting **Fog volume preview**, see [Fog volumes](render-modes-display.md#fog-volumes)).

## Counters

A corner of the view shows (hide with `H`):

- what is drawn: draw calls, triangles, material changes, world chunks in view and occluded, models, instances, sprites;
- the compiled world in view: faces, triangles, displacements, materials, back faces skipped;
- **In game**: the cluster you stand in, its area, how many clusters the game draws from there (PVS and area portals), and faces, triangles, displacements, props and overlays the game would draw. Without visibility data: "compile the map with VVIS";
- **Area portals**: how many are open, looked through, and the areas drawn. The same numbers without area portals are given for comparison;
- the lighting and cubemaps in use, the light styles in use;
- speed and height of the feet, frame rate and the camera mode (first person, third person, noclip);
- **Hearing**: off, or the soundscape playing with its looping sounds, the `ambient_generic` sounds and the volume (`K`, see [Soundscapes](soundscapes.md)).

## Wireframe (X)

| Value | What you see |
| --- | --- |
| `mat_wireframe 0` | Off. |
| `mat_wireframe 1` | World triangles seen through, models in wireframe. |
| `mat_wireframe 2` | World triangles on the visible surfaces, models in wireframe. |
| `mat_wireframe 3` | World polygon outlines seen through, models in wireframe. |

In wireframe, props show the level of detail the game would draw.

## Trains

The trains of the map move in the player view like in game, in the editor brushes and in the compiled world (the compiled model of the train, placed where the compiled map put it, moves the same way).

- **`func_tracktrain` and `func_tanktrain`** start on the `path_track` named by their **First Stop Target** (`target`): their origin goes to the track, raised by **Height above track** (`height`), facing the next tracks. **Initial Speed** (`speed`) is the speed at spawn: `0` waits to be started. **Max Speed** (`startspeed`) is the speed the train gets when it is started (`E`, `T`, `StartForward`, `Toggle`...) and clips `SetSpeed`, `SetSpeedDir` and `SetSpeedReal`. The train follows the chain of `target` keys, loops around closed circuits and stops at a dead end or before a disabled track. Like the engine, it steers toward the point of its path 0.1 second ahead, 66 times a second: its origin cuts the corners a little (about 12 units at 150 units per second) and it passes each track (`OnPass`, speed change) a moment before reaching it.
- **Turning**: with **Change angles** (`orientationtype`) on **Near path_tracks**, the front of the train looks at the point of the path **Distance Between the Wheels** (`wheels`) ahead, so it turns smoothly through the corners like the engine. **Linear blend** and **Ease in/ease out** blend from the orientation of one `path_track` to the next (its own **Orientation Type**: face the motion, its angles, or no change). **Never**, the **Fixed Orientation** spawnflag and the `LockOrientation` input keep the angles. **No Pitch** keeps the train level on slopes, **Bank Angle on Turns** (`bank`) tilts it in the turns, **Allow Roll** takes the roll of the track angles, **GoldSrc rotation** turns the train around like Half-Life 1 (built facing -X).
- **Speed changes**: like the engine, the **New Train Speed** of a `path_track` changes the speed of the trains with **No User Control** (spawnflag 2) or after a track with **Disable train**; a train the player drives keeps its speed. **Change Velocity** (`velocitytype`) blends the speed between two tracks.
- **Tracks**: a **Disabled** track (or `DisablePath`) stops the trains before it, `EnablePath` lets them go on. A fork with a **Branch Path** (`altpath`) takes the normal `target` until `EnableAlternatePath` or `ToggleAlternatePath`; **Branch Reverse** uses the branch when going backward. **Teleport to THIS path track** makes the train jump to that track (output `OnTeleport`). **Fire once** fires `OnPass` the first time only.
- **`func_train`** goes from `path_corner` (or `path_track`) to the next at its **speed**, waits the **wait** of each corner (`-1`: until it is started again), takes the **speed** of the corner it leaves and jumps to a corner with **Teleport**. A `func_train` without a name starts by itself; a named one waits for `Start` or `Toggle` (or `E` and `T`), like in game; **Wait for retrigger** stops it at that corner.
- **Riding**: standing on a train carries the player along (turning with it), and a train pushes the player out of its way. **Passable** trains do not collide.
- **Parented entities**: the entities whose **Parent** (`parentname`) is the train, or is parented to it, move and turn with it: props, sprites (`env_sprite`...), the lights of the lighting preview, brush entities (their brushes collide too), triggers (they fire where the train is), `trigger_teleport` with its destination, and `point_teleport`.
- **Inputs and outputs**: `StartForward`, `StartBackward`, `Stop`, `Toggle`, `Resume`, `Reverse`, `SetSpeed`, `SetSpeedDir`, `SetSpeedDirAccel`, `SetSpeedReal`, `SetMaxSpeed`, `TeleportToPathTrack`, `TeleportToPathNode`, `LockOrientation`, `UnlockOrientation` for the track trains; `Start`, `Stop`, `Toggle`, `SetSpeed` for `func_train`; `EnablePath`, `DisablePath`, `TogglePath`, `EnableAlternatePath`, `DisableAlternatePath`, `ToggleAlternatePath` for `path_track` and `SetNextPathCorner` for `path_corner`. A `logic_auto` with `OnMapSpawn` > `StartForward` starts its train at once. The tracks fire `OnPass` and the trains `OnStart`.
- `E` on a train starts it at its max speed or stops it (not a train with **No User Control**: the log says so); `T` starts every train when none moves, else stops them all. The counters show how many trains move.
- Leaving the player view puts every train back at its start.

## Inputs and outputs

The outputs of the entities fire inputs on their targets after their delay, like in game. Triggers fire when you walk in and out, `E` uses buttons, doors and props, `logic_auto` fires at the start and `logic_timer` on time. The inputs the simulation knows act on the player view: doors open, trains start and stop, brushes and props disappear, area portals open, teleport triggers send you to their target. The others are written in the **Inputs and outputs:** log of the view. Nothing changes in the map.

## Tips

- Compile once (`F9`) before playing: the counters, the area portals and the lightmaps need the compiled map.
- After a compile with lighting, **Map > Lighting > Write cubemaps into the compiled map** can store the cubemaps drawn in the player view (see [Lighting preview](lighting-preview.md)).
- MCP tool: `player_view` (start or leave, wireframe, start or stop trains with `train` and `start`, the state of every train in `trains`, and more; see the [MCP tool reference](../../MCP.md)).

## FAQ

**The player falls through the floor.** The floor uses a material players walk through (trigger, skip, hint, fog, area portal...). Check its material.

**I see the editor brushes, not the compiled world.** Compile the map, or press `B` if you switched to the editor brushes.

**Does opening a door change my map?** No. Doors move for the player view only and close again when you leave.

**My train does not move.** Its **Initial Speed** (`speed`) is `0`: press `T` or `E` on it, or start it from a `logic_auto`. Check that its `target` names a `path_track` (a `path_corner` for `func_train`) and that a named `func_train` gets a `Start` or `Toggle`.

**My train ignores the speeds of the tracks.** Like in game, the **New Train Speed** of a `path_track` only applies to a train with the **No User Control** spawnflag (or after a track with **Disable train**).

## Related pages

- [Lighting preview](lighting-preview.md)
- [Leaks and pointfiles](leaks-pointfiles.md)
- [I/O and logical view](io-logical-view.md)
- [Launch the game](game-launch.md)
- [Camera and navigation](camera-navigation.md)

---
[Back to the documentation index](../README.md)
