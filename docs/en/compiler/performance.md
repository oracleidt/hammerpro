[Documentation](../README.md) > Compiler > Compile performance | [Français](../../fr/compiler/performance.md) | [Русский](../../ru/compiler/performance.md)

# Compile performance

The integrated geometry compiler resolves loose content files on demand instead of recursively indexing every content directory at each launch. Source priority and case-insensitive lookup are preserved. Archive indexes still need to be opened, and compressed addon content may take time on its first read. The journal reports content mounting and material loading before geometry processing; a long wait in these phases is not yet BSP tree generation.

How to make compiles fast while you iterate, and what costs time in a release build. For the speed of the editor itself, see [Editor performance](../troubleshooting/performance.md).

## Already done for you

- **Smart recompile** (Draft and Normal): only what changed is rebuilt. Moving an entity runs `-onlyents` (seconds), changing a light runs `-onlyents` plus the lighting (visibility kept), changing static props runs `-onlyprops`; brushes, displacements, overlays, brush entities, the settings or the game run the full chain. Nothing changed: copy and launch only.
- **Stage build cache**: a geometry or lighting step whose inputs are identical to an earlier one (an undo then a recompile, the same map compiled again) is restored instead of run ("Same input as an earlier compile: result reused").
- **Visibility kept between compiles**: each visibility is kept in the build cache with the portals it comes from. When a later compile of the map has the same portals (only detail brushes, textures, entities, lights or props changed: not the structural brushes, the areaportals, the sky or the visibility radius), it is used at once instead of being computed again: "Full visibility reused: the portals are the same as in an earlier compile" in the log, `Vis mode: full (reused)` in the statistics of the log, "full, reused from an earlier compile with the same portals" as the visibility mode of the Statistics tab. The map is byte for byte the one a full visibility pass gives. Six visibilities are kept per map, inside the 2 GB of the build cache.
- **Limits checked before the visibility**: a map over an engine limit stops right after the geometry, not after a long visibility pass.
- **Compile while idle**: with 8 or more processor threads, the geometry and visibility are built in the background 3 seconds after your last change; F9 then reuses them and leaks show up before you ask. Paused on battery.
- **Fast visibility by default**: the visibility bounds VVIS's portal flow (results are the same or more conservative), and runs on every thread.
- **Fast lighting**: the sky light is traced once per luxel, supersampling only traces the lights that change across a luxel, leaf ambient samples use about three times fewer rays than VRAD. Since 2.0 the lighting also traces its shadow rays faster (cheaper child ordering in the ray tree), skips the static prop models that no lit or shadow casting prop needs (a map whose props all have shadows off and no vertex lighting loads none), reads its random directions from tables computed once, matches the seams between faces and blends the displacements without copying the whole lightmap, runs the bounce passes and the leaf sky tests on every thread, stores the luxels in 12 bytes instead of 16 and frees the ray trees before it writes the BSP. On the benchmark maps the lighting usually takes 15 to 30% less processor time (up to 45% on maps with many shadowless props) and up to a third less memory than 1.2.4. The lightmaps, ambient samples and prop colors are the same, byte for byte, as before these changes.
- **No save, no wait**: the compile works on a snapshot, the pack list is computed while the compiler already runs, and the game can start while the map compiles (**Start the game: when the compile starts**, see [Launch the game](../features/game-launch.md)).

## Measured gains of 2.0

`hammerpro --bspc` of version 1.2.4 against 2.0 on the same 6 core computer, busy with other work during the runs (times on an idle computer are shorter; processor time in brackets):

| Map | Compile | 1.2.4 | 2.0 |
|---|---|---|---|
| rp_archipel_v1 (4246 clusters) | `-vis` again after three `func_detail` were removed | 434 s (1831 s): the whole visibility again | 14 s (4 s): the visibility is reused in 0.01 s |
| gmc_op_v2 (5711 clusters) | `-vis` again after three `func_detail` were removed | 54 s (177 s) | 18 s (10 s) |
| gmc_op_v2 | portal building (world and portal file) and lump writing of a `-fastvis` compile | 0.91 s of processor time | 0.71 s |
| hokai_d (5850 displacements) | any | stops: MAX_MAP_PLANES exceeded, 67374 planes | the geometry is built again with 46864 planes |

The BSP files are the same bytes as those of 1.2.4 (checked on gm_construct, rp_archipel_v1 and gmc_op_v2, with fast and full visibility), and a reused visibility gives the same bytes as a full visibility pass.

## While iterating

| Do | Why |
|---|---|
| Use **Draft** | Fast visibility, one bounce, `-fast` lighting, one dynamic range, no packing. |
| Keep **Compile leaking maps anyway** off | A leaking map stops after the geometry instead of lighting for nothing. |
| Use **Only entities** for I/O and entity work | Seconds instead of a full compile (the smart recompile picks it on its own). |
| Turn on **Play now, finish the lighting later** | You play a Draft at once; Normal or Final lighting replaces it in the background. |
| Use **Compile only the cordoned area** | Only the part inside the cordon is compiled (as `<map>_cordon`). |
| Turn off **Static prop shadows** on prop heavy maps | `-nostaticpropshadows`: prop models are not even loaded. Turn it back on for the release. |
| Keep **Development addon** on | Iteration builds copy custom content to an addon instead of packing it into the BSP. |

## Threads and priority

- **Threads: Automatic** uses every core, or every core but one with **Keep a core for the editor** (on by default). You can set a number from 1 to 256.
- `-low` (options menu, Speed) runs the compiler at idle priority: the rest of the computer stays responsive, the compile may take longer when other programs are busy.
- The visibility and the lighting give the same result on any number of threads (the tests light the same map on 1, 3 and 7 threads and compare every byte).
- Out of memory on a huge map: lower the thread count (the log explains it).

## What costs time in a release

| Cost | Lever |
|---|---|
| Full visibility (`-vis`) on big open maps | Turn small and decorative brushes into `func_detail` (the statistics name them), add hint brushes and areaportals, use `-radius_override` or a fog `farz` for outdoor maps. `-fulldetail` makes it slower. |
| `-final` (more sky and indirect rays) | Needed for release quality; `-extrasky` adds more rays still. |
| Both dynamic ranges | Final always computes LDR and HDR (two passes). |
| `-StaticPropLighting` | Every vertex of every static prop is lit (sky and bounced light once per small group of vertices); the longest stage on maps with thousands of detailed props. **Light static props (per vertex)** off skips it. `-staticpropsamplescale` lowers its rays. |
| `-StaticPropPolys`, `-textureshadows` | Finer prop shadows, slower. |
| Lightmap resolution | Raise the lightmap scale of big or unseen faces (16 or more); the statistics list the faces and materials with the most luxels. `-luxeldensity` below 1 makes all lightmaps coarser. |
| Many bounces | `-bounce` (default 100) stops earlier; bounces stop on their own when they add almost nothing. |

## Measuring

- The **History** tab shows the time of every step of every compile of the map; a step much slower than usual gets a warning in the log.
- The **Statistics** tab shows the stage times of each step (first tree, portals, flow, direct light, bounced light...) and hints that point at what to optimize.
- `-verbose` prints the options actually used.
- `--rad` ends with a `Light phase times:` line that splits the lighting into its phases (scene trees, direct light, displacements, bounce rays and passes, seams, leaf ambient, props, write), see [Compiler command line](command-line.md).

## FAQ

**Why is the second F9 so much faster?** The smart recompile, the stage cache or the background pre-compile reused earlier work.

**Is a Draft compile wrong?** No, it is a coarse preview: fast visibility renders slower in game and the lighting has one bounce. Release with Final.

**The log says the planes are over the limit and the geometry is built again.** A map with many displacements can go over MAX_MAP_PLANES (65536) with the planes of the displacement brushes, which are never written to the BSP. The compiler then builds the geometry again without them: the map compiles and the log line gives the number of planes.

**Why is the second compile of a map so fast when I only moved detail brushes?** Its portals did not change, so the visibility of the earlier compile is used again (see Visibility kept between compiles).

**Does `-low` make my compile slower?** Only when other programs want the processor; idle priority yields to them.

## Related pages
- [Compile options](compile-options.md)
- [Compile and run window](compile-window.md)
- [Limits and logs](limits-and-logs.md)
- [Editor performance](../troubleshooting/performance.md)

---
[Back to the documentation index](../README.md)
