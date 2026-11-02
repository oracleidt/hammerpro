[Documentation](../README.md) > Troubleshooting > Common errors | [Français](../../fr/troubleshooting/common-errors.md)

# Common errors

The problems mappers meet most often, what they mean and how to fix them. In the compile log, errors and warnings are clickable: a click goes to the brush, face, entity or position they name, and the compile window explains known messages. See [Compile limits and logs](../compiler/limits-and-logs.md).

## The map leaks

**What you see:** the compile log reports a leak, the status bar shows a **Leak** badge, and a line (the leak path) is drawn in the views from an entity to the outside.

**What it means:** an entity can reach the void through a hole in the world brushes. Visibility cannot be computed, and the map renders and lights badly.

**How to fix it:**

1. Follow the leak path from the entity to the hole. The path is loaded on its own after a leaking compile (**Map > Load leak pointfile...** loads one by hand).
2. Seal the hole with a **world** brush. `func_detail` and brush entities do not seal the map.
3. Check again without compiling: **Map > Check for leaks**. While you edit, the instant leak check (on by default) watches for leaks and shows the same badge.

Typical causes: a gap between two walls, a brush turned into `func_detail` on the outside of the map, an entity placed outside the walls, a skybox that does not close. See [Leaks and pointfiles](../features/leaks-pointfiles.md).

## Purple and black checkerboard textures in game

A checkerboard in game means the game cannot find a material.

| Cause | Fix |
|---|---|
| The material does not exist in the game or in your custom folders. | **Map > Check for problems** lists "missing material"; the **Map assets** tab shows what is missing. Replace the material (**Tools > Find and replace textures...**) or add its folder in **Settings > Game**. |
| A custom material was not packed. By default, custom content is packed into the map for **Final** compiles and releases only; test builds copy it to the development addon `garrysmod/addons/hammerpro_dev` instead. | After the first copy to the development addon, restart the game once. For a map you share, use **File > Build and save as content...** or set **Pack custom content** to **Always** in the compile window. |
| The material comes from a game mounted through `mount.cfg`, from a Workshop addon or from downloaded server content. | Players without that content see checkerboards too. Ship the files with the map, or replace them with base game content. See [Collect content for a release](../guides/collect-content-for-release.md). |

Entities and models have the same issue: "missing model" (the prop is left out of the map) and "missing sound" are also listed by Check for problems.

## Engine limits

**What you see:** "A hard engine limit was exceeded" in the log, or the limits gauge of the status bar close to its maximum. **Map > Map information** shows every limit with the amount used.

| Limit | What to do |
|---|---|
| Brushes | Merge brushes, turn decorative ones into props, delete hidden ones. |
| Planes | Keep brushes on the grid and simplify off-grid or rotated geometry. |
| Portals | Turn small or detailed brushes into `func_detail`. |
| Brush models (4096, the world included) | Turn brush entities that do not move into world brushes or `func_detail`. |
| Lightmaps | Raise the lightmap scale of large or unseen faces (32 or more), use nodraw where players never look. |
| Overlays (8192) | Remove some, or merge decals into fewer overlays. An overlay must cover fewer than 64 faces. |
| Named lights (96) | Give lights that switch together the same name, or remove the name of lights that never switch. |
| Leaf brushes | Turn small brushes in the area into `func_detail`. |

### "Exceeded max t-junctions fixup limit" (primindices)

The compiler fixes T-junctions (where an edge of a face meets the middle of another edge) with small extra triangles, stored as primitives with 16 bit indices: at most 65,536 indices (`MAX_MAP_PRIMINDICES`). Past that, the compiler writes this warning with the counts, and the faces left over keep their plain polygon, which can show thin cracks or sparkles at their edges. The compile still succeeds. To get under the limit, reduce the number of faces: turn detailed brushwork into props, remove faces players never see (nodraw), and keep geometry on the grid.

Limits differ between games and Source builds; **Settings > Map health > Limits of:** chooses the profile the gauges measure against. See [Check for problems](../features/check-problems.md) and [Compile limits and logs](../compiler/limits-and-logs.md).

## Other compile errors

| Message (meaning) | Fix |
|---|---|
| An entity is inside a solid brush | Move it into open space: lighting and the sky camera break otherwise. |
| A brush is too small or too thin | Delete it or make it at least 1 unit thick on the grid. |
| A brush has a face of zero area | Delete it or rebuild it with the block tool. |
| A brush has faces with materials of different contents | Use one kind of material (water, clip, nodraw...) on every face of the brush. |
| A face's texture is projected along the face itself | Reset the face alignment; Check for problems fixes it. |
| Displacements cannot be part of a brush entity | Move the displacement brush back to the world. |
| A model cannot be a `prop_static` | Use `prop_dynamic` or `prop_physics` for that model. |
| A static prop is in the void | Move it inside the world. |
| The file of a `func_instance` cannot be found | Check its path (relative to the map or the maps folder). |
| The compiler ran out of memory | Close other programs, lower the number of threads, or split very large maps. |
| The compiler does not know this option | Check the spelling of extra arguments in the compile settings. |

Most of these are also found before compiling by **Map > Check for problems** (`Alt+P`), which can fix several of them for you.

## The compile seems stuck in the lighting

The lighting step prints a progress line at least every 30 seconds (`Lighting 41%: direct lighting 60% (stage 2 of 7), 27 s elapsed, about 17 s left`), and the progress bar of the step shows the stage, its percentage and the time left. If the bar moves, the compile is working: the time left tells how long it needs.

| What you see | What to do |
|---|---|
| A long **Direct lighting** | Its time grows with the luxels, the number of lights each face sees and the sky over large outdoor areas. Raise the lightmap scale of big or unseen faces (the statistics list them), or compile with the Normal or Draft preset while iterating. |
| A long **Static props** stage | **Light static props (per vertex)** (`-StaticPropLighting`, part of Final) lights every vertex of every prop. Turn it off while iterating, or lower `-staticpropsamplescale`. |
| A long **Ambient occlusion**, **Bounce rays** or **Leaf ambient** | `-noao` (or `_ao 0`), `-bounce 1` and `-fastambient` cut them. |
| `Warning: the lighting stage "..." has not advanced for ...` | The stage is still working on something large (a huge face or a very detailed prop). The warning names what makes that stage slow. If it repeats for a long time, cancel and report it with the log ([Reporting bugs](reporting-bugs.md)). |

The log of the step also names the stages before they run (`Lighting stages: ...`), so you know what is left.

## The game is not found

| Message | Fix |
|---|---|
| "Garry's Mod was not found: choose its folder in Settings > Game" | Set the Garry's Mod folder (**Detect** or **Browse...**), see [Game setup](../getting-started/game-setup.md). |
| "The game of this map is not installed" | The map targets another game: install it, choose its folder in **Settings > Game**, or change the **Target game** in the compile window. |
| "The game executable was not found" | Check the game folder in **Settings > Game** (for a game other than Garry's Mod, the executable found is listed under the game). |
| "Save and compile the map first" / "*name*.bsp is not in the maps folder of the game" | Save the map, then compile it with **Copy to the maps folder** on before **Run the map in game**. |

## Related pages

- [Leaks and pointfiles](../features/leaks-pointfiles.md)
- [Check for problems](../features/check-problems.md)
- [Compile limits and logs](../compiler/limits-and-logs.md)
- [Map assets](../features/map-assets.md)
- [FAQ](faq.md)

---
[Back to the documentation index](../README.md)
