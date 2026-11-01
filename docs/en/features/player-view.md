[Documentation](../README.md) > Features > Player view | [Français](../../fr/features/player-view.md)

# Player view

The player view lets you walk the map in the 3D view like a Garry's Mod player: real movement, collisions, doors, triggers, the compiled world with its lightmaps and counters of what the game would draw. Nothing in the map changes while you play.

## Opening it

- **View > Display options > Player view**, the player button of the toolbar, or `F6`.
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
| `E` | Use: open or close doors, press buttons, use props. |
| `N` | Shows the tool textures (nodraw, clips, triggers), hidden by default like in game. |
| `B` | Compiled world or editor brushes. |
| `L` | Lightmaps of the compiled map, or the editor shading. |
| `H` | Hides or shows the counters. |
| `1` to `9` | Turn a named light on or off. |
| `Escape`, `F6` | Leave. |

The keys line at the top of the view reminds you of them.

## Movement

The movement uses the Source values of Garry's Mod: 200 units per second walking, 400 running, gravity 600, jump speed 200, a 32 x 32 x 72 hull (36 crouched), eyes at 64 (28 crouched), steps up to 18 units. Collisions use the brushes (triggers, hints, skips, fog, area portals, occluders, invisible ladders and 2D sky faces are walked through) and the collision models of props (`.phy`, like the game: a tree only blocks at its trunk). The player pushes the physics props it walks into. The player model plays the Garry's Mod animations (idle, walk, run, crouch, jump, noclip).

## Compiled world and lighting

- After a compile, the view draws the **compiled world** (the faces as the compiler cut them) instead of the editor brushes. `B` switches back to the editor brushes. Without a compile: "Editor brushes: compile the map to see the world as the game cuts it". When the map changed since the compile, a line asks you to compile again.
- `L` lights the compiled world with the lightmaps of the compiled map and the props like the game (setting **Player view: compiled lighting**, `render.player_compiled_lighting`, on by default). A map compiled without lighting is fullbright, like in game.
- The **light styles** of the compiled map play like in game (ten pattern letters a second). Named switchable lights follow their inputs and the keys `1` to `9`.
- Cubemap reflections use the cubemaps built in game when the compiled map has them, else the ones Hammer Pro draws.
- While the eyes are inside a `fog_volume`, the fog of the `env_fog_controller` named by its **Fog Name** replaces the map fog, like in game (the box around its brushes, volumes with **Start Disabled** skipped; setting **Fog volume preview**, see [Fog volumes](render-modes-display.md#fog-volumes)).

## Counters

A corner of the view shows (hide with `H`):

- what is drawn: draw calls, triangles, material changes, world chunks in view and occluded, models, instances, sprites;
- the compiled world in view: faces, triangles, displacements, materials, back faces skipped;
- **In game**: the cluster you stand in, its area, how many clusters the game draws from there (PVS and area portals), and faces, triangles, displacements, props and overlays the game would draw. Without visibility data: "compile the map with VVIS";
- **Area portals**: how many are open, looked through, and the areas drawn. The same numbers without area portals are given for comparison;
- the lighting and cubemaps in use, the light styles in use;
- speed and height of the feet, frame rate and the camera mode (first person, third person, noclip).

## Wireframe (X)

| Value | What you see |
| --- | --- |
| `mat_wireframe 0` | Off. |
| `mat_wireframe 1` | World triangles seen through, models in wireframe. |
| `mat_wireframe 2` | World triangles on the visible surfaces, models in wireframe. |
| `mat_wireframe 3` | World polygon outlines seen through, models in wireframe. |

In wireframe, props show the level of detail the game would draw.

## Inputs and outputs

The outputs of the entities fire inputs on their targets after their delay, like in game. Triggers fire when you walk in and out, `E` uses buttons, doors and props, `logic_auto` fires at the start and `logic_timer` on time. The inputs the simulation knows act on the player view: doors open, brushes and props disappear, area portals open, teleport triggers send you to their target. The others are written in the **Inputs and outputs:** log of the view. Nothing changes in the map.

## Tips

- Compile once (`F9`) before playing: the counters, the area portals and the lightmaps need the compiled map.
- After a compile with lighting, **Map > Lighting > Write cubemaps into the compiled map** can store the cubemaps drawn in the player view (see [Lighting preview](lighting-preview.md)).
- MCP tool: `player_view` (start or leave, wireframe and more; see the [MCP tool reference](../../MCP.md)).

## FAQ

**The player falls through the floor.** The floor uses a material players walk through (trigger, skip, hint, fog, area portal...). Check its material.

**I see the editor brushes, not the compiled world.** Compile the map, or press `B` if you switched to the editor brushes.

**Does opening a door change my map?** No. Doors move for the player view only and close again when you leave.

## Related pages

- [Lighting preview](lighting-preview.md)
- [Leaks and pointfiles](leaks-pointfiles.md)
- [I/O and logical view](io-logical-view.md)
- [Launch the game](game-launch.md)
- [Camera and navigation](camera-navigation.md)

---
[Back to the documentation index](../README.md)
