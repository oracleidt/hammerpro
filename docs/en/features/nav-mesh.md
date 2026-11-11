[Documentation](../README.md) > Features > Navigation mesh | [Français](../../fr/features/nav-mesh.md) | [Русский](../../ru/features/nav-mesh.md)

# Navigation mesh

The navigation mesh (`.nav`) tells NPCs and bots where they can walk. Hammer Pro generates one from the map without starting the game, and draws any `.nav` file over the map so you can spot holes and areas cut off from the rest.

## Opening it

| Action | Menu | Default shortcut |
| --- | --- | --- |
| **Generate nav mesh** | **Tools > Placement and props > Generate nav mesh** | none |
| **Load nav mesh...** | **Map > Leaks and visibility > Load nav mesh...** | none |
| **Show nav mesh** | **View > Diagnostics > Show nav mesh** | none |

## Generating

**Tools > Placement and props > Generate nav mesh** builds the mesh in the background and writes `<map>.nav` next to the VMF (where the viewer and **Build and save as content** look for it). Save the map first. When it is done, "Nav mesh generated: N areas in N ms" and **Show nav mesh** turns on.

How it works, like Valve's `nav_generate`:

- the walk starts at the player starts (classes beginning with `info_player_`, and `gmod_player_start`). A map without one gets "No player start in the map: add an info_player_start where players walk.";
- the ground is sampled every 25 units. A sample is kept where a player fits (standing or crouching), and two samples are linked when a player can walk, step (up to 18 units), jump up or drop between them;
- the samples are merged into rectangles of nearly flat ground (the areas), connected along their edges.

What blocks players: the visible world brushes, the brush entities `func_detail`, `func_brush`, `func_wall`, `func_wall_toggle` and `func_lod`, the displacements, and solid static props (as boxes). Doors, breakables and moving brushes are left out (they open, break or move; the game's generator walks through them too). Players walk through trigger, hint, skip, area portal, occluder, NPC clip, fog, block LOS and block light textures, and water.

Warnings in the console: player starts with no ground under them are skipped, and a very large map stops at the sample limit.

## Showing a nav mesh

**Show nav mesh** draws the mesh in every view. The file is the one chosen with **Map > Leaks and visibility > Load nav mesh...**, else the `.nav` next to the map, else the one in the game's `maps` folder (also next to a decompiled BSP). It is read again when it changes on disk (a background thread looks at the candidate files every 1.5 seconds and wakes the editor only when one changes or appears, so showing the nav mesh costs no redraw while nothing changes). Hammer Pro reads nav versions 1 to 16 (Garry's Mod writes version 16).

| Drawing | Meaning |
| --- | --- |
| Area outline green | Walkable. |
| Blue | Crouch. |
| Yellow | Jump. |
| Red | Avoid. |
| Cyan | Stairs. |
| Magenta | Cut off from the main part of the mesh (where players spawn, else the largest part). |
| White line | Connection both ways. |
| Orange line with an arrow | One way connection. |
| Purple frame | Ladder. |

Areas are filled in the 3D view and outlined in the 2D views. The status bar shows "Nav areas: N" (and "cut off: N"); the console lists the areas, ladders, one way links and links to missing areas of the loaded mesh.

## Options

**Tools > Options... > Nav mesh and particles** (`[lab]` in `settings.toml`):

| Option | What it does | Default |
| --- | --- | --- |
| **Show nav mesh** (`show_nav_mesh`) | Draws the mesh. | Off |
| **Fill the areas in the 3D view** (`nav_fill`) | Without the fill, only the outlines of the areas are drawn. | On |
| **Connections between areas** (`nav_connections`) | White: both ways. Orange with an arrow: one way. | On |

## Tips

- Magenta areas are the ones bots never reach: fix the gap that cuts them off, or remove them in game (the Valve wiki's `nav_flood_select` tip finds them).
- The [release window](build-content-release.md) can also build the nav mesh in a game session, and its checklist checks that the map has one.
- MCP tool: `nav_mesh` (the mesh shown, `show` to show or hide it, `load` a `.nav` file of the map folder, `generate` to run **Generate nav mesh** in the background, then `generating` and `last_generation` with the areas and time or the error; see the [MCP tool reference](../../MCP.md)).

## FAQ

**Generation says no walkable ground was found.** The player starts float above nothing, or stand inside a solid. Put an `info_player_start` on the floor.

**The mesh goes through my doors.** Doors are left out on purpose, like the game's generator.

**Do I need the game to generate?** No. The generator reads the map in the editor.

## Related pages

- [Build and save as content](build-content-release.md)
- [Launch the game](game-launch.md)
- [Leaks and pointfiles](leaks-pointfiles.md)
- [Settings reference](../reference/settings.md)

---
[Back to the documentation index](../README.md)
