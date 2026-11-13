[Documentation](../README.md) > Compiler > Overview | [Français](../../fr/compiler/overview.md) | [Русский](../../ru/compiler/overview.md)

# The built in compiler

Hammer Pro compiles maps itself. The geometry, portals, leak test and visibility are built by `hammerpro --bspc`, the lighting by `hammerpro --rad`, and the packing of custom content, the copy to the game and the launch run inside the editor. You do not need VBSP, VVIS, VRAD or bspzip, and there is no compiler path to set. It is the default compiler; Valve's tools found next to the game, or programs of your own, can be chosen instead for each game (see [Choosing a compiler](compilers.md)).

## The stages

A compile runs these steps in order (a step whose mode is **No** is left out):

| Step | What it does | Program |
|---|---|---|
| **Geometry** | Merges the `func_instance` files, reads the brushes, builds the BSP tree, the portals, finds leaks, then (when asked) computes the visibility. Writes `<map>.bsp`, `<map>.prt` (sealed maps) or `<map>.lin` (leaks). | `hammerpro --bspc` |
| **Lighting** | Lights the BSP in place: lightmaps, ambient cubes, world lights, detail props, optionally static prop vertex lighting. | `hammerpro --rad` |
| **Pack content** | Packs the custom materials, models and other files the map uses into the pakfile of the BSP, then reads the pakfile back and reports anything missing. | built in |
| **Copy to the maps folder** | Copies the BSP to `garrysmod/maps` (or the folder you chose), through a temporary file and a rename so the game never sees a half written map. Cubemaps already built in game are carried over. | built in |
| **Launch the game** | Starts the game on the map, or loads it in the game that is already running. | built in |

Inside the geometry step the compiler follows the steps of Valve's VBSP: planes, material contents (from the `%compile` keys of the VMT), brushes with collision bevels, texture and lightmap mappings, a first tree of the structural brushes to find the inside of the map and the leaks, the final tree with the detail, faces (merged, cut for their lightmaps, without T-junctions), areas and areaportals, `func_viscluster`, occluders, `env_cubemap` samples, detail props (`detail.vbsp`), water, then the lumps of the file. The visibility follows Valve's VVIS (base portal vis, portal flow, cluster merge, PAS).

## How F9 runs it

- **F9** (**File > Compile > Compile now**) compiles at once with the last settings, without opening any window. The status bar shows the progress.
- **Ctrl+F9** (**File > Compile > Compile and run...**) opens the [Compile and run window](compile-window.md), where you choose the preset and every option.
- **File > Compile > Cancel compile** stops a running compile (the whole process tree is killed).

The open map is never saved by a compile. Hammer Pro takes a snapshot of the document and writes it to `<map folder>/.hammerpro_build/`: your VMF is not touched (the option **Save the map first** only saves your work at the same time). A map that was never saved asks where to save it first.

## Files produced

| File | Where | When |
|---|---|---|
| `<map>.vmf` (the snapshot) | `.hammerpro_build/` next to your map | every compile |
| `<map>.bsp` | `.hammerpro_build/`, then copied to the maps folder | every successful geometry step |
| `<map>.prt` | `.hammerpro_build/` | sealed maps (no portal file for a leaking map) |
| `<map>.lin` | `.hammerpro_build/` | when the map leaks (loaded as the leak path on its own), or with `-trace` |
| `<map>.log` | `.hammerpro_build/` | every compile: each line of the compiler, appended, CRLF line ends like Valve's tools |
| `<map>_cordon.bsp` | as above | with **Compile only the cordoned area**: the full map is never overwritten |

Hammer Pro also keeps, in its cache folder (`%LOCALAPPDATA%\HammerPro` on Windows): the stage build cache (`builds`, with the kept visibilities `vis-*.hpvis` of each map, 2 GB at most, oldest first), the background pre-compiles (`spec`), the timing history (`compile_history/<map>.jsonl`) and the progressive lighting builds (`progressive`).

## Differences from VBSP, VVIS and VRAD

What the code and its comments state:

- **One program, two steps.** The visibility is part of the geometry step (`-vis` or `-fastvis` on `--bspc`). Command sequences written for VBSP, VVIS, VRAD and bspzip still run: their steps are mapped to the Hammer Pro steps (see [Compile and run window](compile-window.md)).
- **Same command line words.** `--bspc` reads the options of VBSP and VVIS, `--rad` those of VRAD. Valve options that this compiler does not do (VBSP debug output, VVIS MPI, `-glview`, `-nocsg`...) give a warning and are ignored; unknown options are never silent. See [Command line](command-line.md).
- **No entity in the open.** A map with no entity inside (for example a terrain just imported into a new map) has no inside to flood. It is compiled like a leaked map, with every face seen from the void, every displacement and no visibility, so the lighting preview and a test compile work; the log says so and asks for an info_player_start in a sealed map. A world made only of displacements (no solid brush) gets one split node so the BSP stays valid.
- **Same log wording.** The compiler prints VBSP's messages on purpose (`**** leaked ****`, `Entity X (x y z) leaked!`, `Brush N: ...`) so the editor can make them clickable and explain them.
- **Faster visibility.** VVIS's portal flow can take hours on large open maps. By default Hammer Pro bounds the flow (shared separating planes, merged flows, results reused in both directions). Each shortcut only skips work or makes windings larger, so the result stays conservative: a cluster is never hidden from a cluster that can see it. The result does not depend on the number of threads. `-fastvis` gives exactly VVIS `-fast`'s rows.
- **Faster lighting, same formulas.** Lights, attenuation, spot cones, texture lights and static prop lighting follow VRAD's code (Source SDK 2013, checked against Garry's Mod's tools). Hammer Pro traces the sky light once per luxel instead of once per supersample, tries each light at the corners and middle of a luxel before supersampling it, and picks leaf ambient samples with fewer rays. The sky light of a face is traced on every other luxel and filled in between where the traced luxels agree; a soft sun (`SunSpreadAngle`) first tries six of its rays and only traces the others in the penumbra; static props trace their sky and bounced light once per small group of vertices facing the same way. The sky is only seen through sky faces: a ray that leaves the map lights nothing. Like VRAD, `_distance` only goes to the world light the game reads; the lightmaps keep the light past it. Models packed in the map's pakfile are found for prop shadows and prop lighting. The lighting writes the same bytes on every run and on any number of threads.
- **Progress you can follow.** The lighting announces its stages, then prints `Lighting 41%: direct lighting 60% (stage 2 of 7), 27 s elapsed, about 17 s left` lines (each 5%, at least every 30 s), and a warning when a stage stops advancing for a minute or runs past ten minutes. The compile window and the MCP server show the stage, its percentage and the time left.
- **Additions.** `-nostaticpropshadows`, `-nodispshadows`, `-wsaddons`, `-serveraddons`, `-blocksize`, `-propdebug` (VBSP has no prop report), statistics and optimization hints at the end of the log, and a check of every written BSP (every index inside its lump, every count within the engine limits): a map that passes loads.
- **Content like the editor.** The compile mounts the same content sources the editor shows (Workshop items, server downloads), so a material visible in the editor is never compiled as missing.
- **Other games.** One compiler writes the BSP formats of several Source branches (`-target`), see [Target games](target-games.md).
- **Trains without an origin.** A `func_tracktrain`, `func_tanktrain` or `func_train` with no `origin` key and no origin brush gets the center of its brushes as origin, with a warning in the log (`has no origin key nor origin brush`). VBSP leaves such a train at the world origin, and the game then draws it far from its path. Maps saved by Hammer always have the key.
- **Nested instance parameters.** A nested `func_instance` that passes a parameter on (`replace01` `$color $color`) works: only the value of its replace key gets the parent's value, and a parameter declared with a default in the `func_instance_parms` of a file and not set gets that default. VBSP also replaces the variable name and mangles the nested file (see [Instances](../features/instances.md)).
- **Shorelines.** An `info_overlay_transition` becomes water overlays (lump 50) computed from its shore and water faces, as the editor draws it: one upright strip per shore face, chained along the shore with seamless texture coordinates (the game draws no water overlay with a mirrored axis). Garry's Mod's VBSP only writes the strips that Valve's Hammer saved in the VMF, and without their position (measured: a zero origin, normal and corners), so the game does not draw them. The decompiler reads the water overlays back.
- **Displacements in a func_detail.** A displacement whose brush is tied to a `func_detail` is compiled like a world displacement (the same faces, collision and lighting), with a warning in the log (`displacement found on a(n) func_detail entity ... compiled as a world displacement`). VBSP stops the compile on it. A displacement in any other brush entity still stops the compile, as the game draws displacements only in the world. The brush of a displacement never goes into the BSP (it does not seal the map, cut the visibility or block players: the displacement surface has its own collision), so the BSP is the same, lump for lump, as with the displacement in the world (checked by a test).
- **Switchable lights bounce.** VRAD bounces only the light of style 0: a named (switchable) light lights what it sees directly and nothing else, so a room looks darker as soon as its lamp gets a name. Hammer Pro bounces the light of each switchable style in its own lightmap layer, with the same rays and the same number of bounces as the rest, so the bounced light of the lamp switches with it. A face takes at most 4 styles: a bounce too faint to see (below half a unit of light) or a face that already has 4 styles gets no extra layer. A bounce layer never pushes out the direct light of a style, in a compile and in the live relights of the preview. Their direct light and falloff (`_fifty_percent_distance`, `_zero_percent_distance`, `_hardfalloff`) are the same as for a light without a name. `-nostylebounce` (or **No bounce for switchable lights** in the lighting parameters) gives VRAD's behaviour.
- **Bounced light measured against VRAD.** On the test maps (Garry's Mod VRAD, same BSP) the bounced light of brush faces is within about 5% of VRAD and the total light of displacements within 1%. The back of a displacement blocks bounced light like the back of a brush face (its lit top no longer shines through it), and a water surface reflects bounced light back to the side it comes from, like VRAD, so the floor of a pool gets the light the water sends back down. Displacements gather their bounced light like VRAD's displacement patches: from flat patches about 8 luxels wide (`-dispchop`), never from their own surface, only from the displacements whose plane they are in front of, and blocked by the props standing on them. On the test maps their bounced light is within about 2% of VRAD (it was about 22% too bright before 2.0.6, on bumpy terrain, rocks and walls with props on them).
- **Models stop where the light ends.** The game lights models (props, players, NPCs) at run time from the world lights with their three attenuation terms and their radius only, so a light with a `_zero_percent_distance` and `_hardfalloff` (or a falloff curve that turns back up) kept lighting models far past the point where its lightmap light ends. Hammer Pro writes the end of that falloff as the radius of the world light (a shorter `_distance` still wins). VRAD writes only `_distance`.
- **Long faces split without cracks.** A face with more than 32 corners (a long wall along a staircase, its corners added where other faces touch it) is cut into pieces. VBSP cuts it along its own edge, which leaves thin pieces with no area and T-junctions along the cut where the light shows through as sparkles; Hammer Pro cuts it along lines that cross the face from a corner, and gives a world piece that still has corners in a row a triangle list that uses every corner.
- **4-way blends.** The 4-way blend rows of displacements (Hammer++ `multiblend`, `alphablend`, `multiblend_color_N`) go to the displacement multiblend lump (63) with the multiblend flag of each displacement, byte for byte like Garry's Mod's VBSP, for Garry's Mod, Source 2013 and CS:GO targets. A Lightmapped_4WayBlend displacement without rows gets rows made from its vertex alpha (texture 2 where painted), as the editor shows it; VBSP writes none for it. The decompiler reads the lump back into the rows.
- **Not written.** The displacement lightmap sample positions lump (only VRAD reads it) and the per texel lightmaps of props with `generatelightmaps`.

## Compared to Hammer++

Hammer Pro reads what Hammer++ adds for its compiles: the worldspawn key `_lightsrad` (a per map light file, see [Worldspawn and light keys](worldspawn-keys.md)) and its command sequences file `hammerplusplus_sequences.cfg` (imported as Expert profiles). The **VBSP++** limit profile of the map information dialog shows the limits of those tools (see [Limits and logs](limits-and-logs.md)).

## FAQ

**Do I need the Source SDK or Hammer installed?** No. Only the game (for its content and to play the map).

**Can I still compile with VBSP, VVIS and VRAD?** Yes: pick **Valve (bin)** or **Valve (bin/win64)** in the **Compiler** row of the compile window, or make a compiler of your own. See [Choosing a compiler](compilers.md).

**Is the BSP byte for byte the one VBSP would write?** No, and it does not try to be. The goal is a correct map that looks right in game; the formats, limits and formulas follow Valve's tools.

**Can I run the compiler outside the editor?** Yes, `hammerpro --bspc` and `hammerpro --rad` work from a terminal or a script. See [Command line](command-line.md).

**Can an AI assistant compile?** Yes, through the MCP tools `compile_map`, `compile_with_options`, `get_compile_log` and others. See [MCP server](../reference/mcp-server.md).

## Related pages
- [Compile and run window](compile-window.md)
- [Choosing a compiler](compilers.md)
- [Compile options](compile-options.md)
- [Command line](command-line.md)
- [Worldspawn and light keys](worldspawn-keys.md)
- [Target games](target-games.md)
- [Limits and logs](limits-and-logs.md)
- [Compile performance](performance.md)
- [Compile and test in Garry's Mod](../guides/compile-and-test-gmod.md)
- [Leaks and pointfiles](../features/leaks-pointfiles.md)

---
[Back to the documentation index](../README.md)
