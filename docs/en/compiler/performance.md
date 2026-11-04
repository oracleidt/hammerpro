[Documentation](../README.md) > Compiler > Compile performance | [Français](../../fr/compiler/performance.md)

# Compile performance

How to make compiles fast while you iterate, and what costs time in a release build. For the speed of the editor itself, see [Editor performance](../troubleshooting/performance.md).

## Already done for you

- **Smart recompile** (Draft and Normal): only what changed is rebuilt. Moving an entity runs `-onlyents` (seconds), changing a light runs `-onlyents` plus the lighting (visibility kept), changing static props runs `-onlyprops`; brushes, displacements, overlays, brush entities, the settings or the game run the full chain. Nothing changed: copy and launch only.
- **Stage build cache**: a geometry or lighting step whose inputs are identical to an earlier one (an undo then a recompile, the same map compiled again) is restored instead of run ("Same input as an earlier compile: result reused").
- **Compile while idle**: with 8 or more processor threads, the geometry and visibility are built in the background 3 seconds after your last change; F9 then reuses them and leaks show up before you ask. Paused on battery.
- **Fast visibility by default**: the visibility bounds VVIS's portal flow (results are the same or more conservative), and runs on every thread.
- **Fast lighting**: the sky light is traced once per luxel, supersampling only traces the lights that change across a luxel, leaf ambient samples use about three times fewer rays than VRAD.
- **No save, no wait**: the compile works on a snapshot, the pack list is computed while the compiler already runs, and the game can start while the map compiles (**Start the game: when the compile starts**, see [Launch the game](../features/game-launch.md)).

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
- The visibility and the lighting give the same result on any number of threads.
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

## FAQ

**Why is the second F9 so much faster?** The smart recompile, the stage cache or the background pre-compile reused earlier work.

**Is a Draft compile wrong?** No, it is a coarse preview: fast visibility renders slower in game and the lighting has one bounce. Release with Final.

**Does `-low` make my compile slower?** Only when other programs want the processor; idle priority yields to them.

## Related pages
- [Compile options](compile-options.md)
- [Compile and run window](compile-window.md)
- [Limits and logs](limits-and-logs.md)
- [Editor performance](../troubleshooting/performance.md)

---
[Back to the documentation index](../README.md)
