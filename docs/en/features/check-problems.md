[Documentation](../README.md) > Features > Check for problems | [Français](../../fr/features/check-problems.md) | [Русский](../../ru/features/check-problems.md)

# Check for problems

Hammer Pro checks the map in the background while you edit: invalid brushes, broken entity links, missing content, engine limits, edicts and leaks. The results show in the status bar and in the **Check for problems** window, where most problems can be fixed in one click. The **Map information** window shows the statistics and limits of the map, and a few more tools help you find your way: func_detail suggestions, **Go to** and **Compare with another map**.

## Live checks and the status bar

A moment after each change, the map is checked on a worker thread (setting **Check the map as I edit**, on by default). The status bar shows two segments on the right:

| Segment | What it shows | Click |
| --- | --- | --- |
| Problem counts | Red cross and number of errors, warning sign and number of warnings, a check mark when there is nothing. `...` while a check runs. The tooltip also gives the number of notes. | Opens **Check for problems**. |
| Leak badge | **Leak** (red), only when the instant leak check found a way from an entity to the void. | Goes to the leak and draws its path (see [Leaks and pointfiles](leaks-pointfiles.md)). |
| Limit gauge | The engine limit closest to its maximum, in percent: green, yellow from 80%, red over 100%. The tooltip names the limit and the edicts used and left. | Opens **Map information**. |

## The Check for problems window

Open it with **Map > Check for problems** (`Alt+P`) or by clicking the problem counts. While it is open the list follows the live checks.

| Control | What it does |
| --- | --- |
| Error, warning and information buttons | Each shows its count and hides or shows that severity (**Show errors**, **Show warnings**, **Show information**). |
| **Ignored (N)** | Shown when you ignore problems: lists them again (dimmed) so you can stop ignoring them. |
| **Visible objects only** | Lists only the problems of objects shown in the views, like Hammer's "Check visible objects only". Saved as `health.visible_only` (off by default). Hidden objects stay counted in the status bar. |
| **Check again** | Runs the checks now. |
| **Fix all (N)** | Fixes every fixable problem that is not ignored, in one undo step. |

Each row shows the severity icon, the description (full text in the tooltip), the number of objects or faces involved, and the buttons **Go to** (selects the objects and centers the views on them) and **Fix** when the problem can be fixed automatically (the tooltip says what the fix does). A click or double click on the row also goes to it. Right click a row for **Go to**, **Fix**, **Ignore** (hide this problem for this map, remembered with the map), **Ignore every "kind"**, **Stop ignoring** and **Copy text**.

Messages at the top tell you when the map is being checked, when the leak check runs and when the game content is still loading (missing materials and models may not be listed yet).

### What is checked

| Area | Problems |
| --- | --- |
| Brushes | invalid solid, tiny brush (smaller than 1 cubic unit), duplicate plane, too many sides (more than 128), mixed contents (water and solid faces on one brush), invalid texture axes, solid out of bounds, origin brush in the world, tool texture on the world (textures that only work on brush entities) |
| Ids and references | duplicate object id, duplicate face id, ids renumbered when the map was loaded, missing group, missing visgroup |
| Entities | unknown class, missing target, missing output target, unknown input, entity in a brush, entity out of bounds, entity outside of the map (may leak), empty brush entity, duplicate unique entity, key too long (31 and 1023 characters), bad content path (spaces or special characters), no player spawn, empty map |
| Content | missing material, missing model, missing sound, missing particle system, bad material name |
| Displacements and overlays | broken displacement (face without exactly 4 corners, or a power outside 1 to 4, or a dispinfo block Hammer Pro cannot read and keeps as it is for Hammer, so the compiled brush stays whole; the fix removes it: the compiler and the game drop the whole brush, so the player view hides it and lets you walk through it too), displacement on a brush entity, overlay faces missing, overlay on too many faces (64 at most) |
| Lighting and sky | sky without sun (sky faces but no `light_environment`), too many named lights, several sky cameras, bad sky camera scale, skybox switch without sky camera, per player skybox |
| Areaportals | areaportal brushes (exactly one allowed), areaportal without a door, areaportal and door out of step, areaportal across water, areaportal across an occluder |
| Redundant objects | duplicate brush, brush inside a brush, stacked duplicate entity (see [Redundant brushwork](#redundant-brushwork)) |
| Limits and leaks | limit close, limit exceeded (the compile will fail), edict budget, leak, leak check incomplete, func_detail candidate |

### Automatic fixes

| Problem | Fix |
| --- | --- |
| invalid solid | Delete the invalid solids |
| duplicate object id, duplicate face id | Give new ids to the duplicates |
| missing output target | Remove the broken outputs |
| empty brush entity | Delete the empty brush entities |
| broken displacement | Turn the broken displacements back into plain faces |
| missing group, missing visgroup | Remove the references to missing groups |
| invalid texture axes | Realign the textures to the world |
| duplicate plane | Remove the duplicate faces |
| overlay faces missing | Remove the faces that do not exist from the lists (an overlay left on no face is deleted) |
| displacement on a brush entity | Move the solids to the world |
| tiny brush | Delete the tiny solids |
| origin brush in the world | Delete the origin brushes of the world |
| func_detail candidate | Tie them to one func_detail (one undo step) |
| areaportal and door out of step | Start the areaportal like its door |
| duplicate brush, brush inside a brush, stacked duplicate entity | Delete the redundant copies and keep the originals |

Every fix is an undo step: `Ctrl+Z` brings the map back.

## Redundant brushwork

The checks also find what you made twice by accident:

| Problem | What is found | Fix |
| --- | --- | --- |
| **duplicate brush** (warning) | A brush that is an exact copy of another brush of the same entity: same vertices within 1/16 unit, whatever the order of its faces and its textures. Displacement copies must also have the same surface. The world counts as one entity, and so do all the `func_detail` entities (they are merged when compiling). | Deletes the copies and keeps the oldest brush (lowest id) |
| **brush inside a brush** (information) | A solid brush fully inside another solid brush of the same entity (both made of opaque materials, nodraw and sky included, or both of the same clip texture). Brushes with displacements, hint, skip, trigger, areaportal or water faces are never reported. | Deletes the inner brushes |
| **stacked duplicate entity** (warning) | A point entity on another one at the same origin with the same class, model, skin, scale, angles, name, parent and outputs (entities without a model must have all their keyvalues equal, so two lights of different colors are kept), or a brush entity identical to another one with the same brushes. | Deletes the copies and keeps the oldest entity |

**Fix** (or **Fix all**) deletes the copies in one undo step, checking again first that each one is still redundant. **Go to** selects the copies so you can look at them before.

The search hashes the rounded sorted vertices of every brush once (the result is kept until the brush changes) and looks for brushes inside others with a grid, so it stays fast on big maps.

**Live warning**: when a new brush or entity (created, pasted, duplicated, cloned, made by an MCP tool) lies exactly on an identical one, a message says so at once, for example "Brush 812 is an exact copy of brush 640 at the same place". The setting **Warn when a new brush or entity is an exact copy of another** (`health.warn_duplicates`, on by default) turns it off.

MCP: `check_problems` with `category` `redundant` lists only these problems, and `find_duplicate_brushes` lists every copy with the object it duplicates (`kinds`: `brushes`, `contained`, `entities`), with `select` to select the copies or `delete` to delete them in one undo step.

## func_detail suggestions

**Map > Leaks and visibility > func_detail suggestions...** lists the world brushes that could be `func_detail`: small or complex brushes away from the hull of the map, verified to keep the map sealed. The visibility ignores detail brushes, so the map compiles faster. The suggestions come from the instant leak check, so the map must be sealed (*The map leaks: suggestions need a sealed map*, with **Show the leak**).

Each row (**Solid N**, its number of faces, *small* or *complex*) selects the brush and centers the views on it. Use **Check all**, **Check none**, **Select checked**, and **Convert N to func_detail** to tie the checked brushes to one `func_detail` in one undo step. The **List** button of the "func_detail candidate" problem opens the same window.

## Map information

**Map > Map information** opens a window with two tabs.

**Overview**: numbers of solids, world solids, faces, entities (point and brush), displacements, unique materials, groups, visgroups and invalid solids; the bounds (min, max, size); the navigation mesh (areas, areas cut off); the submaps of the map when it has some; and **Used materials (N)**, sortable **By use** or **By name** with a filter. Double click a material to select the objects using it; right click for **Select the objects using it**, **Use as the current texture** and **Copy text**.

**Engine limits** (the tab label turns yellow or red when a limit is close): every engine limit (brushes, brush sides, planes, vertices, edges, surface edges, faces, BSP nodes, texture infos, materials, brush models, entities, overlays, displacements, cubemaps, world lights, lightmaps and HDR lightmaps) with its **Limit**, the **Estimate** measured from the map as you edit, and the exact count of the **Last compile** read from the compiled map. It also shows the areas and visibility clusters, the largest users of a limit (click to select), and three sections:

- **Edicts (Garry's Mod)**: entities that still exist when the map runs, against the 8192 Garry's Mod networks, with advice (keep about 2000 free for what players spawn, unused `prop_dynamic`, named lights, unnamed `func_brush`/`func_wall`/`func_illusionary`).
- **Lightmaps**: the estimated lighting data against the engine limit, the most expensive faces, faces with a very fine lightmap scale, faces VBSP has to split and displacements too wide for VBSP.
- **Texture memory**: click **Compute** to add up the texture files of the used materials (needs the game content).

### Limit profiles

Limits differ between Source builds. **Limits of:** (in the Map information window, and in **Settings > Map health**) chooses the maxima:

| Profile | Use it for |
| --- | --- |
| **Automatic** | Default (`auto`): Garry's Mod 64-bit for Garry's Mod maps, Valve's limits for the other games. |
| **Source SDK 2013** | Stock Valve limits. |
| **Garry's Mod 32-bit** | The 32-bit build of Garry's Mod. |
| **Garry's Mod 64-bit** | The 64-bit build of Garry's Mod. |
| **VBSP++ (Tools++)** | Maps compiled with the Hammer++ tools. |

## Go to an id or a position

**Map > Go to brush or entity id...** (`Ctrl+Shift+G`) opens **Go to**. Type an object id (`1234`, `#1234`, `id 1234`) or three coordinates (`0 0 512`, `(1, 2, 3)`, `[1 2 3]`) and press **Go**. A line pasted from a compile log works too: the brush it names, else the first coordinates in parentheses, else its last three numbers. An id selects the solid or entity and centers the views on it (a hidden object is only centered); coordinates center the views.

## Compare with another map

**Map > Compare with another map...** asks for a `.vmf` or `.vmx` (an older version, an autosave, a copy) and lists the differences in the **Compare maps** window, values read as *other map > this map*:

- counters that also filter: **Only in this map**, **Only in the other map**, **Changed**;
- a filter box (class, id or change);
- each row: **Entity** or **Brush**, its id and what changed (class, keys, outputs, brushes of an entity, materials, faces moved, texture alignment, displacement, editor data), with **Go to** and **Select**;
- **Select the listed objects** (objects that are only in the other map cannot be selected).

## Map health settings

**Settings > Map health** (`[health]` in settings.toml):

| Option | Default |
| --- | --- |
| **Check the map as I edit** | on |
| **Look for leaks as I edit** | on |
| **Draw the path of the leak found** | on |
| **Check for problems lists visible objects only** | off |
| **Warn when a new brush or entity is an exact copy of another** | on |
| **Limits of:** | Automatic |
| Portal options | see [Leaks and pointfiles](leaks-pointfiles.md) |

## MCP

`check_problems`, `get_problems`, `fix_problems`, `recheck_map`, `get_map_limits`, `get_map_stats` and `find_hidden_faces` give AI apps the same checks. See the [MCP tool reference](../../MCP.md).

## Tips

- Leave the live checks on: the status bar tells you about a broken output or a missing model as soon as you make it.
- Ignore a problem you know about (a test entity) rather than switching the checks off: ignored problems are kept with the map.
- Before a big compile, look at the limit gauge; the exact numbers after a compile are in the **Last compile** column.

## FAQ

**Why does the estimate differ from the compiled count?** The compiler splits faces and adds vertices and nodes. Some numbers are only known after a compile; the **Last compile** column has them.

**Where are ignored problems stored?** With the map, in its sidecar file next to the VMF (see [VMF compatibility](../getting-started/vmf-compatibility.md)).

**The check takes time on a huge map. Can I turn it off?** Yes, untick **Check the map as I edit** in **Settings > Map health**. **Check again** still runs it on demand.

## Related pages

- [Leaks and pointfiles](leaks-pointfiles.md)
- [Limits and logs](../compiler/limits-and-logs.md)
- [Common errors](../troubleshooting/common-errors.md)
- [Settings reference](../reference/settings.md)

---
[Back to the documentation index](../README.md)
