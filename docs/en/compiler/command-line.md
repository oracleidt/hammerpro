[Documentation](../README.md) > Compiler > Command line | [Français](../../fr/compiler/command-line.md)

# Compiler command line

The compiler is part of `hammerpro.exe`. `hammerpro --bspc` builds the geometry, portals, leak test and visibility of a VMF; `hammerpro --rad` lights the BSP. Both take Valve's option names (VBSP and VVIS for `--bspc`, VRAD for `--rad`), so scripts and Hammer command sequences translate directly. The other arguments of `hammerpro.exe` are in the [command line reference](../reference/command-line.md).

## Usage

```
hammerpro --bspc [options] <map>
hammerpro --rad  [options] <map>
```

- `<map>` is the map path, with or without extension. `--bspc` reads `<map>.vmf`; `--rad` lights `<map>.bsp` in place. The map is given last (any argument that does not start with `-` is the map).
- Options are case insensitive. An option that needs a value and has none, or a value that is not a number, stops with `Error: ...` and exit code 1.
- Each line of output is printed on stdout and appended to `<map>.log` next to the map (CRLF line ends, like Valve's tools).
- Exit code 0 on success, 1 on any error (and on a leak with `-leaktest`).

## Files written

| File | By | When |
|---|---|---|
| `<map>.bsp` | `--bspc`, rewritten by `--rad` | always, except a leak with `-leaktest` |
| `<map>.prt` | `--bspc` | sealed maps only |
| `<map>.lin` | `--bspc` | leak path; or the line of sight with `-trace` |
| `<map>.log` | both | appended |
| `materials/maps/<map>/...` in the pakfile | `--bspc` | cubemap patches and placeholders (see [Cubemaps](cubemaps.md)) |

## `--bspc` options

Geometry and visibility. Defaults are those of the command line alone; the editor adds its own (see [Compile options](compile-options.md)).

| Option | Value | Effect | Default |
|---|---|---|---|
| `-game`, `-vproject` | folder | Game folder: materials, models, `lights.rad`, `detail.vbsp`, instances. | Garry's Mod's `garrysmod` |
| `-gmod` | folder | Garry's Mod install whose content is mounted. | the detected install |
| `-target` | game id | The game the map is for: its BSP format and limits (see [Target games](target-games.md)). | Garry's Mod, or the game of `-game` |
| `-insert_search_path` | folder | Another game folder mounted like `-game` (may be repeated). | none |
| `-threads` | number | Worker threads (1 or more). | every core |
| `-low` | | Idle priority. | off |
| `-v`, `-verbose` | | Prints the options. | off |
| `-leaktest` | | Stops on a leak: writes `<map>.lin`, no BSP, exit code 1. | off: a leaking map still gets its BSP |
| `-vis` | | Full visibility. | off: no visibility |
| `-fastvis` | | Fast coarse visibility. | off |
| `-fast` | | VVIS's `-fast`: with `-vis`, the fast visibility. | off |
| `-forceskyvis`, `-skyvis` | | Visibility inside the 3D skybox too. | off |
| `-onlyents` | | Updates the entities of the existing `<map>.bsp` only. | off |
| `-onlyprops` | | Updates the static and detail props of the existing BSP only. | off |
| `-nodetail` | | Leaves out the `func_detail` brushes. | off |
| `-nowater` | | Leaves out the water brushes. | off |
| `-fulldetail` | | Detail brushes cut the visibility like structural ones. | off |
| `-micro` | volume | Microbrush warning threshold. | 1 |
| `-blocksize` | units | 0 (no blocks) or 128 to 32768; other values are ignored with a note. | 1024 |
| `-luxelscale` | factor | Multiplies every lightmap scale. | 1 |
| `-minluxelscale` | scale | Smallest lightmap scale (at least 1). | 1 |
| `-defaultluxelsize` | scale | Scale of faces whose lightmap scale is 0 (1 or more). | 16 |
| `-maxlightmapdim` | luxels | Largest face lightmap, 1 to 125 (other games cap it at 32, with a warning). | 32 |
| `-bumpall` | | Bumped lightmaps on every lit surface. | off |
| `-lightifmissing` | | Lightmaps for missing or unlit materials. | off |
| `-nodrawtriggers` | | Trigger faces are not drawn. | off |
| `-AllowDynamicPropsAsStatic` | | Keeps static props whose model has `prop_data`. | off |
| `-replacematerials` | | Applies `cfg/materialsub.cfg` of the game. | off |
| `-keepstalezip` | | Keeps the files packed in the previous BSP. | off |
| `-embed` | folder | Packs a whole folder into the BSP (full compiles only, refused with `-onlyents`/`-onlyprops`). | none |
| `-dxlevel` | level | Drops entities excluded by their `mindxlevel`/`maxdxlevel`. | none |
| `-radius_override` | units | Visibility radius. | fog `farz`, else none |
| `-nosort` | | Visibility without sorting the portals. | off |
| `-trace` | cluster cluster | Writes the line of sight between two clusters to `<map>.lin`. | none |
| `-nomerge`, `-nosubdiv`, `-notjunc`, `-noprune` | | Debug switches, see [Compile options](compile-options.md). | off |
| `-novirtualmesh` | | Displacement collision as triangle soups. | off |
| `-propdebug` | | Report of the props whose model misbehaves. | off |
| `-noaddons` | | No folder addons. | addons mounted |
| `-wsaddons` | | Also the subscribed Workshop items. | off |
| `-nowsaddons` | | Neither folder addons nor Workshop items. | off |
| `-serveraddons` | | Also the content downloaded from servers. | off |
| `-mount_appid` | AppID[,AppID] | Also mounts installed Steam games by AppID (repeatable, `-mount_appid 240,440`), found through the Steam libraries like **Settings > Game > Steam games by AppID**. | none |

Accepted for compatibility, no effect: `-novconfig`, `-allowdebug`, `-steam`, `-verboseentities`, `-tmpout`, `-tmpin`.

Known Valve options this compiler does not do (a warning, the compile goes on): `-glview`, `-nocsg`, `-noshare`, `-noweld`, `-noopt`, `-nomergewater`, `-snapaxial`, `-dumpcollide`, `-dumpstaticprop`, `-dumpstaticprops`, `-allowdetailcracks`, `-notjuncsdetail`, `-xbox`, `-fullminidumps`, `-block` (2 values), `-blocks` (4 values), and every `-mpi...` option. Any other unknown option gets `Warning: VBSP: Unknown option "..." (ignored)`.

Without game content (no Garry's Mod found and no `-game`), materials are guessed from their names and the log says so.

## `--rad` options

Lighting. With no range option, `--rad` computes LDR only.

| Option | Value | Effect | Default |
|---|---|---|---|
| `-ldr` / `-hdr` / `-both` | | Dynamic ranges to compute. | LDR |
| `-game` | folder | Game folder: `lights.rad`, materials and models. | `garrysmod` of the install |
| `-gmod` | folder | Garry's Mod install. | the detected install |
| `-target` | game id | The game the map is for (its limits and content). | Garry's Mod |
| `-insert_search_path` | folder | More game folders for the models. | none |
| `-threads` | number | Worker threads. | every core |
| `-low` | | Idle priority. | off |
| `-fast` | | Draft quality: few rays, no static prop vertex lighting, no texture light cutting. | off |
| `-final` | | Final quality (more sky and indirect rays, static prop rays x4). | off |
| `-bounce` | number | Maximum bounces. | 100 |
| `-noextra` | | No supersampling. | off |
| `-extrasky` | factor | More sky and indirect rays (lightmaps and props). | none |
| `-smooth` | degrees | Smoothing angle. | 45 |
| `-softsun` | degrees | Sun spread for softer shadows. | 0 |
| `-luxeldensity` | factor | Coarser lightmaps, only values below 1 count (others get a warning). | 1 |
| `-noskyboxrecurse` | | No shadow or light through the 3D skybox. | off |
| `-fastambient` | | Quicker prop ambient light. | off |
| `-centersamples` | | Accepted (always done). | |
| `-lights` | file | Extra texture light file. | none |
| `-chop` / `-maxchop` / `-dispchop` | luxels | Texture light patch sizes, at least 1. | 4 / 4 / 8 |
| `-StaticPropLighting` | | Per vertex static prop lighting. | off |
| `-StaticPropPolys` | | Props shadow with their drawn triangles. | off |
| `-textureshadows` | | Alpha tested prop shadows (with `-StaticPropPolys`). | off |
| `-nossprops` | | No prop self shadowing. | off |
| `-staticpropsamplescale` | factor | More or fewer prop rays (more than 0). | 1 |
| `-StaticPropNormals` | | Debug: props show their normals. | off |
| `-nostaticpropshadows` | | Hammer Pro: no static prop shadow at all. | off |
| `-nodispshadows` | | Hammer Pro: no displacement shadow. | off |
| `-ao` / `-noao` | | Hammer Pro: ambient occlusion on or off. | on, off with `-fast` |
| `-aoradius` / `-aostrength` / `-aosamples` | number | Ambient occlusion settings. | 96 / 0.75 / 32 (64 with `-final`) |
| `-noaddons`, `-wsaddons`, `-nowsaddons`, `-serveraddons`, `-mount_appid` | | Content mounted for the models, like `--bspc`. | addons and Workshop items mounted |
| `-radius_override` | units | Accepted and skipped (a VVIS option often passed to every tool). | |

Every other option gets `Warning: option ... is not supported by this lighting compiler: ignored`; the known VRAD options that take a value (`-vproject`, `-aoscale`, `-aofacesamples`, `-aopropsamples`, `-ambientocclusion`, `-sunsamplesarealight`, `-leafambientsamplereduction`, `-maxdispsamplesize`, `-disppatchradius`, `-mpi_workercount`, `-dumptrace`) skip their value too.

Texture light files are read in this order: `lights.rad` of the game (the game folder, then the folders of the Garry's Mod install, its `bin` folders last), the `-lights` file, the file named by the worldspawn key `_lightsrad`, then `<map>.rad` next to the map.

While it lights, `--rad` lists its stages (`Lighting stages: direct lighting, ambient occlusion, bounce rays, bounced light, leaf ambient, static props, writing`), then reports each stage that runs longer than a second:

```
Lighting 41%: direct lighting 60% (stage 2 of 7), 27 s elapsed, about 17 s left
```

The first percentage is the whole lighting, the second the stage. A line comes every 5% and at least every 30 seconds. A stage that has not advanced for a minute prints `Warning: the lighting stage "..." has not advanced for ...` with what makes that stage slow, and one that runs past ten minutes prints `Warning: the lighting stage "..." has run for ...`. The editor reads these lines for its progress bar. Problems found by the final check that were already in the BSP before the lighting (a map compiled by other tools) are warnings; only new ones stop the write.

## Examples

A quick test of a Garry's Mod map:

```
hammerpro --bspc -fastvis -leaktest -game "D:\Steam\steamapps\common\GarrysMod\garrysmod" C:\maps\gm_test
hammerpro --rad -ldr -fast -bounce 1 C:\maps\gm_test
```

A release build:

```
hammerpro --bspc -vis -leaktest C:\maps\gm_test.vmf
hammerpro --rad -both -final -StaticPropLighting C:\maps\gm_test
```

The same map for Team Fortress 2:

```
hammerpro --bspc -target tf -vis C:\maps\ctf_test
hammerpro --rad -target tf -both C:\maps\ctf_test
```

Only the entities changed:

```
hammerpro --bspc -onlyents C:\maps\gm_test
```

## FAQ

**My map has no visibility after `hammerpro --bspc`.** The command line computes the visibility only with `-vis` or `-fastvis` (there is no separate VVIS step).

**Can I keep my old VBSP/VVIS/VRAD batch file?** Replace `vbsp.exe` by `hammerpro --bspc`, drop the `vvis.exe` line and add `-vis` (or `-fastvis`) to the `--bspc` line, replace `vrad.exe` by `hammerpro --rad`.

**Where is the compile log?** `<map>.log` next to the map, appended at each run.

## Related pages
- [Compile options](compile-options.md)
- [Command line reference](../reference/command-line.md)
- [Target games](target-games.md)
- [Compiler overview](overview.md)

---
[Back to the documentation index](../README.md)
