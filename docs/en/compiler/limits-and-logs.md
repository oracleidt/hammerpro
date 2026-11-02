[Documentation](../README.md) > Compiler > Limits and logs | [Français](../../fr/compiler/limits-and-logs.md)

# Limits and logs

The engine limits a map must stay under, how Hammer Pro measures them, and how to read the compile log, its explanations and its statistics.

## Engine limits

Garry's Mod's own tools and engine raised many of Valve's limits. Hammer Pro keeps one table of both (where Garry's Mod's tools and engine differ, the smaller value is kept). The compiler checks the limits of the [target game](target-games.md): Garry's Mod's for Garry's Mod, Source SDK 2013's for the other games. 0 means no limit.

| Limit | Source SDK 2013 | Garry's Mod |
|---|---|---|
| Brush models (brush entities, the world included) | 1024 | 4096 |
| Brushes | 8192 | 65535 |
| Brush sides | 65536 | 655350 |
| Planes | 65536 | 65536 |
| Vertices | 65536 | 65536 |
| BSP nodes | 65536 | 65536 |
| Texture infos | 12288 | 32768 |
| Materials (texdata) | 2048 | 8192 |
| Displacements | 2048 | 16384 |
| Faces, original faces | 65536 | 65536 |
| Leaves, leaf faces, leaf brushes | 65536 | 65536 |
| Areas | 256 | 1024 |
| Areaportals | 1024 | 1024 |
| Surface edges | 512000 | 512000 |
| Edges | 256000 | 256000 |
| Cubemap samples | 1024 | 1024 |
| Overlays | 512 | 8192 |
| Faces of one overlay | 64 | 64 |
| Faces of one water overlay | 256 | 256 |
| World lights (per pass) | 8192 | 8192 |
| Entities | 8192 | 65535 |
| Entity text | 384 KB | 4 MB |
| Visibility data | 16 MB | 16 MB |
| Lightmap data (per pass) | 2 GB | 2 GB |
| Static props | 65535 | 16384 (the engine keeps the first 16384) |
| Switchable (named) lights | 32 | 96 |
| Primitives, primitive indices | 32768, 65536 | 32768, 65536 |
| Lightmap extent of a face (luxels) | 33 | 126 |
| Collision data, displacement collision | no limit | no limit |

Other rows (vertex normals and their indices, leaf water data, areaportal vertices, water overlays, displacement vertices and triangles) follow the same table and appear in the statistics.

### Limit profiles

The live checks and the **Map > Map information** dialog measure the map against a profile, chosen in its **Limits of:** list (saved as `limit_profile` in the `[health]` table of `settings.toml`):

| Profile | Limits |
|---|---|
| **Automatic** (default) | Garry's Mod 64-bit for Garry's Mod, Source SDK 2013 for the other games |
| **Source SDK 2013** | Valve's `bspfile.h` values |
| **Garry's Mod 32-bit** | the same as 64-bit today |
| **Garry's Mod 64-bit** | the Garry's Mod column above |
| **VBSP++** | Garry's Mod's limits with 65536 entities |

The status bar and [Check for problems](../features/check-problems.md) warn when a limit is close ("Brushes: 60000 of 65535 (91%)") and when it is exceeded ("..., the compile will fail"). After every compile, the compiler checks the BSP it wrote: every index inside its lump, every count within the limits. Any problem is a compile error, so a map that compiles loads.

## Reading the compile log

The **Log** tab of the [Compile and run window](compile-window.md) shows every line of every step as it comes, with a progress bar per step. The same text is appended to `<map>.log`.

- **Colors and filters**: information, warnings and errors; **Show information lines**, **Show warnings**, **Show errors**, **Search the log**, **Auto-scroll**, **Copy the whole log**. Click a step to see only its lines; **Show the log of every step** clears that.
- **Clickable problems**: a line that names a brush, a face (side), an entity or a position becomes a link: click it to select the object and center the views ("Click to go to brush 12"). A log that belongs to another map tells you to open it first.
- **Problems list**: "Problems: N errors, M warnings", each with **Go to**.
- **Explanations**: known messages get a plain explanation and a button: **Help** (the Valve Developer Community page), **Go to**, **Leak path** (loads the pointfile) or **Check** (opens Check for problems).
- **Pipeline notices**: lines written by Hammer Pro itself, such as "Compile stopped after the geometry: the map leaks", "Packed N files into the BSP", "Same input as an earlier compile: result reused", "... does not exist yet: a full compile runs instead of the partial one", "Could not replace ...: if the map is loaded in the game, change map or disconnect first".
- A crash of a step is named ("The program ran out of memory", "stack overflow"...), with its exit code.

The compiler prints VBSP's wording on purpose, so the same rules work on logs of Valve's tools too.

### Common messages

| Message contains | Meaning | What to do |
|---|---|---|
| `**** leaked ****` | An entity can reach the void through a hole in the world brushes. | Follow the leak path and seal the hole with a world brush (`func_detail` and brush entities do not seal). See [Leaks and pointfiles](../features/leaks-pointfiles.md). |
| `leaked!` | This entity is the start of the leak path. | Same as above. |
| `areaportal leak` | An areaportal does not close its opening. | Make the areaportal fill its opening; separate the areas with world brushes. |
| `microbrush` | A brush is too small or too thin to be built. | Delete it or make it at least 1 unit thick on the grid. |
| `texture axis perpendicular` | A texture is projected along its own face. | Reset the face alignment (Check for problems fixes it). |
| `mixed face contents` | One brush mixes contents (water and solid, nodraw and clip...). | Use one kind of material on every face. |
| `displacement found on a(n)` | A displacement inside a brush entity. | Move the displacement brush back to the world. |
| `too many switched lights` | More than 96 named lights (32 outside Garry's Mod). | Give lights that switch together the same name; remove names that never switch. |
| `must be used on a dynamic entity` | A `prop_static` model has `prop_data`. | Use `prop_physics` or `prop_dynamic`, or `-AllowDynamicPropsAsStatic`. |
| `must be compiled with $staticprop` | The model is not a static prop model. | Use `prop_dynamic`. |
| `overlay touching too many faces` | An overlay covers 64 faces or more. | Make it smaller. |
| `too many overlays`, `too many brush models` | Limits exceeded. | Remove or merge overlays; turn brush entities that do not move into world brushes or `func_detail`. |
| `could not open instance file` | A `func_instance` file is missing. | Check its path (relative to the map, or to the maps folder). |
| `bad surface extents` | A face is too big for its lightmap and cannot be cut (`-nosubdiv`). | Raise its lightmap scale or split the brush. |
| `unknown option` | The compiler does not know an option. | Check the spelling in the extra arguments. |
| `material not found` | A material is neither in the game nor in your folders. | Replace it or add its folder in the settings. |
| `error loading studio model`, `outside the map` | A prop model is missing or broken, or a static prop is in the void: it is removed. | Fix the model path; move the prop inside the world. |
| `in solid volume` | An entity is inside a solid brush. | Move it into open space. |
| `findportalside`, `brush with win 0` | Overlapping or invalid brushes. | Clean up the brushes at that place. |
| `max_map_brushes`, `max_map_planes`, `max_map_portals` | A hard limit. | Merge brushes, keep geometry on the grid, turn small brushes into `func_detail`. |
| `ambient occlusion: radius ..., strength ..., N rays` | Information: the ambient occlusion settings used. | |
| `Warning: ... is not supported by the Hammer Pro compiler (ignored)` | A Valve option this compiler does not do. | Nothing; remove it to silence the warning. |
| `Warning: Couldn't open texlight file ...` | A light file was not found. | Check `-lights` or `_lightsrad`. |

## The Statistics tab

The **Statistics** tab (also the **Statistics: N hints** button of the log) reads the statistics the compiler prints at the end of each step. Every item with a place in the map (a brush, a face, a cluster, an area, a prop) has a **Go to** button.

- **Optimization hints**: limits at 75% or more (over 90% they read as warnings, over 100% the game may refuse the map), a map compiled without visibility (critical when it has water: the game then draws the water black, without refraction, so compile with at least the fast visibility), clusters that see 50% of the map or more, a weak average visibility, a big map without areaportals, small structural brushes that could be `func_detail` (the smallest named), static props without fade distance, power 4 displacements, lightmaps taking 90% or more of the 2 GB a face can address, very fine lightmap scales, switchable styles nearly used up.
- **Engine limits**: Valve's vbspinfo table (used, maximum, bytes, fullness), the fullest first.
- **Geometry**: map size, brushes (structural, detail, water, tool, in brush entities), entities by class, faces by material, displacements by power, areas, the small structural brushes and the props without fade distance.
- **Visibility**: mode (fast, full, not computed, or skipped because the map leaks), clusters, portals, leaves, visible clusters on average and at most, the vis radius, the clusters that see the most.
- **Lighting**: lightmap bytes of each pass against the 2 GB a face can address, luxels, lights by type, switchable styles, texture lights, static and detail props lit, leaf ambient samples, bounces, the faces and materials with the most luxels with their lightmap scale.
- **Stage times** and **Step times**.

With no Hammer Pro compile in the log, the tab says so. The MCP tool `get_compile_stats` gives the same data.

## FAQ

**The status bar says a limit will fail but the compile works.** The live checks are an estimate made before compiling; the compiler's own count is exact. Check the profile in **Map > Map information**: a profile with lower limits than your game is stricter.

**Where is the log of an older compile?** `<map>.log` in the `.hammerpro_build` folder next to your map keeps every run; the History tab keeps the times.

**Is there a 16 MB lightmap limit?** No. `MAX_MAP_LIGHTING` (16 MB) is still in Valve's `bspfile.h`, but neither the Source SDK 2013 tools nor Garry's Mod's VRAD check it (their size table prints the lightdata with no maximum), and the engine loads more: Valve compiled Garry's Mod maps load with 60 to 80 MB of lightmaps per pass. The real limit, for every target game, is the lightmap offset of a face (`lightofs` in `dface_t`), a signed 32 bit number: past 2 GB per pass the faces cannot point to their lightmaps and the game cannot load the map. Hammer Pro's lighting stops with an error there; the hints warn from 90% of it. Large lightmaps still cost memory and loading time, so keep coarse scales where shadows are soft.

## Related pages
- [Check for problems](../features/check-problems.md)
- [Leaks and pointfiles](../features/leaks-pointfiles.md)
- [Common errors](../troubleshooting/common-errors.md)
- [Compile and run window](compile-window.md)
- [Compile performance](performance.md)

---
[Back to the documentation index](../README.md)
