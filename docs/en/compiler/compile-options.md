[Documentation](../README.md) > Compiler > Compile options | [Français](../../fr/compiler/compile-options.md) | [Русский](../../ru/compiler/compile-options.md)

# Compile options

Every flag the Hammer Pro compiler understands, with its effect and its default. The geometry step (`hammerpro --bspc`, which also computes the visibility) and the lighting step (`hammerpro --rad`) each have their own flags.

## Where you set them

The built-in compiler enables `-AllowDynamicPropsAsStatic` by default. Physics-capable models placed as `prop_static` are kept as static scenery instead of being deleted. They do not gain physics or animation. Disable the flag in the geometry options to restore strict model checks; missing or broken model files are still rejected.

- **The options menus** of the [Compile and run window](compile-window.md) (and of **Settings > Compile**): **All geometry and visibility options (hammerpro --bspc)** and **All lighting options (hammerpro --rad)**. A flag follows the preset until you force it on (with its value) or off. The choices are saved in `settings.toml` under `[compile.bsp_flag_choices]` and `[compile.rad_flag_choices]`, for example:
  ```toml
  [compile.rad_flag_choices."-bounce"]
  on = true
  value = "8"
  ```
- **Extra geometry arguments** and **Extra lighting arguments** (Advanced): free text added to the command line.
- **Expert mode** profiles, and the [command line](command-line.md).
- These are the flags of Hammer Pro's compiler. The options of Valve's vbsp, vvis and vrad, for the compilers you choose instead, are listed in [Choosing a compiler](compilers.md).
- The MCP tool `set_compile_options` (`bsp_options`, `rad_options`).

How they combine: the preset's flags come first; a flag of your extra arguments replaces the preset's; a choice of the options menu replaces both (forced off removes the flag everywhere). Flags are case insensitive. The step command lines look like this:

```
hammerpro --bspc -gmod <GarrysMod> [mode flags] [-threads N] <your flags> [content] -game <game folder> <map>
hammerpro --rad <-ldr|-hdr|-both> [mode flags] [-threads N] <your flags> [content] -gmod <GarrysMod> -game <game folder> <map>
```

For another target game, both steps get `-target <id>` instead of `-gmod` (see [Target games](target-games.md)).

## Flags written by other settings

The options menus show these under **Written from other settings** but do not change them.

| Flag | Step | Effect | Set by |
|---|---|---|---|
| `-vis` | geometry | Full visibility: the map renders fast in game. | **Visibility: Normal** (Normal, Final) |
| `-fastvis` | geometry | Quick coarse visibility: much faster to compile, slower to render. | **Visibility: Fast** (Draft) |
| `-leaktest` | geometry | Stop when the map leaks: no BSP is written, no time is lost on the lighting. | **Compile leaking maps anyway** off (the default) |
| `-onlyents` | geometry | Only update the entities of the previous BSP (seconds). Geometry, visibility and lighting are kept. | **Geometry: Only entities**, smart recompile |
| `-onlyprops` | geometry | Only update the static and detail props of the previous BSP. | **Geometry: Only props**, smart recompile |
| `-threads N` | both | Processor threads to use. | **Threads** (automatic: every core, or every core but one) |
| `-game <folder>` | both | Game folder whose materials, models and `lights.rad` the compiler reads. | the game configuration |
| `-gmod <folder>` | both | Garry's Mod folder whose content the compiler mounts. | the game configuration |
| `-ldr`, `-hdr`, `-both` | lighting | Standard range only (fastest), high range only, or both (two passes, needed for release). | **Dynamic range** |
| `-nostaticpropshadows` | lighting | Static props cast no shadow at all (see below). | **Static prop shadows** off |

## Geometry

Menu group **Geometry** (geometry step).

| Flag | Effect | Default |
|---|---|---|
| `-nodetail` | Leaves out every detail brush (`func_detail`): what is left is what cuts the visibility. | off |
| `-nowater` | Leaves out the water brushes. | off |
| `-micro <volume>` | Brushes smaller than this volume get a microbrush warning. | 1 |
| `-nodrawtriggers` | Trigger textures are not drawn at all (smaller map). | off |
| `-novirtualmesh` | Displacement collision as triangle soups instead of the virtual terrain mesh. | off |
| `-replacematerials` | Replaces materials as listed in `cfg/materialsub.cfg` of the game (its `AllMaps` block, then the block of the map). | off |
| `-AllowDynamicPropsAsStatic` | Keeps `prop_static` props whose model has physics properties (`prop_data`) instead of deleting them. | off |
| `-dxlevel <level>` | Drops the entities whose `mindxlevel`/`maxdxlevel` exclude this DirectX level. | not used |
| `-blocksize <units>` | Size of the blocks the world is cut in before its brushes split it: 0 (no blocks) or 128 to 32768. Bigger blocks or 0 cut fewer faces and leaves (fewer planes and vertices). | 1024 |

## Visibility

Menu group **Visibility** (geometry step). Without `-vis` or `-fastvis` no visibility is computed: every part of the map sees every other.

| Flag | Effect | Default |
|---|---|---|
| `-fulldetail` | Detail brushes become normal brushes: they cut the visibility too (slower vis). | off |
| `-forceskyvis` | Computes the visibility inside the 3D skybox too (it is one cluster otherwise). | off |
| `-radius_override <units>` | Visibility radius: clusters farther apart never see each other, whatever the fog. | the `farz` of the first `env_fog_controller`, else none |
| `-nosort` | Visibility without sorting the portals first (same result). | off |

## Lightmaps

Menu group **Lightmaps** (geometry step: the lightmap sizes are decided when the faces are cut).

| Flag | Effect | Default |
|---|---|---|
| `-luxelscale <factor>` | Scales every lightmap scale by this factor (2 halves the lightmap resolution). | 1 |
| `-minluxelscale <scale>` | No lightmap scale below this value. | 1 |
| `-defaultluxelsize <scale>` | Lightmap scale of the faces whose lightmap scale is 0. | 16 |
| `-maxlightmapdim <luxels>` | Largest lightmap of a face in luxels: bigger faces are cut less. Garry's Mod takes up to 125, the other games 32. | 32 |
| `-bumpall` | Every lit surface gets bumped lightmaps (bigger lighting data). | off |
| `-lightifmissing` | Surfaces with missing or unlit materials get a lightmap anyway. | off |

## Content mounting

Menu group **Content** (geometry step; the lighting step reads the same mounting flags for the models).

| Flag | Effect | Default |
|---|---|---|
| `-noaddons` | Does not mount the folder addons of `garrysmod/addons`. | addons mounted |
| `-wsaddons` | Also mounts the subscribed Workshop items (Valve's VBSP does not). | off on the command line; the editor adds it when it shows Workshop content |
| `-nowsaddons` | Mounts neither the folder addons nor the Workshop items. | off |
| `-serveraddons` | Also mounts the content downloaded from servers (`garrysmod/download`, `garrysmod/cache/workshop`; Valve's VBSP does not). | off on the command line; the editor adds it when it shows that content |
| `-keepstalezip` | Keeps the files packed in the previous BSP (the rest is rebuilt). | off |
| `-embed <folder>` | Folder added to the content and packed whole into the BSP. Full compiles only. | not used |

The editor's compile passes the content sources the editor itself shows, so a material visible in the editor is never compiled as missing. Your custom texture folders are always visible to the compiler: the custom materials and the custom models the map uses are copied into a generated game folder for it, so a `prop_static` with a model of a custom folder keeps its prop.

## Lighting quality

Menu group **Quality** (lighting step).

| Flag | Effect | Default |
|---|---|---|
| `-final` | High quality sky and indirect lighting (more rays): slow. Also raises the static prop sky and bounce rays. | Final preset |
| `-bounce <n>` | Maximum number of light bounces: 1 is much faster. | 100 (Draft: 1) |
| `-noextra` | No supersampling: harder, blockier shadow edges, faster. | off |
| `-extrasky <factor>` | Traces this many times more sky and indirect light rays (smoother ambient light, slower). | not used |
| `-smooth <degrees>` | Faces meeting at less than this angle get smooth shading across their edge. | 45 |
| `-softsun <degrees>` | Treats the sun as an area light this many degrees wide: softer shadow edges (0 to 5 is usual). | 0 |
| `-centersamples` | Accepted like VRAD: luxels are always sampled at their middle when it is open. | always |
| `-nodispshadows` | Hammer Pro option: displacements cast no shadow at all (they are still lit). One displacement can also be kept from casting shadows with the **Cast shadows** option of the [displacement tool](../features/displacement-tool.md). | off |
| `-nostylebounce` | Hammer Pro option: named (switchable) lights light only what they see directly, like Valve's VRAD. Without it their light also bounces, in their own light style, and switches with them. Their direct light always keeps its place among the 4 styles of a face: a bounce layer only takes a free slot. | off |

## Static props

Menu group **Static props** (lighting step).

| Flag | Effect | Default |
|---|---|---|
| `-StaticPropLighting` | Bakes the lighting of every static prop vertex (shadows and bounced light) instead of letting the game light props at run time. | Final preset, when **Light static props (per vertex)** is on |
| `-StaticPropPolys` | Static props cast shadows with the triangles they draw instead of their collision model (finer shadows, slower). | off |
| `-textureshadows` | With `-StaticPropPolys`: alpha tested prop textures (leaves, fences) of models compiled with `$casttextureshadows` let light through. | off |
| `-nossprops` | Static props never shadow themselves. | off |
| `-staticpropsamplescale <factor>` | Scales the number of rays that light static prop vertices (with `-StaticPropLighting`). | 1 (4 with `-final`) |
| `-nostaticpropshadows` | Hammer Pro option: static props cast no shadow at all (lightmaps, bounced light, detail props, ambient light, prop lighting); their models are not even loaded. Written by **Static prop shadows**. | off |

## Texture lights and sky

Menu group **Texture lights and sky** (lighting step).

| Flag | Effect | Default |
|---|---|---|
| `-lights <file>` | Extra texture light file, read after `lights.rad` and before the map's own light files. | not used |
| `-chop <luxels>` | Smallest texture light patch (at least 1). | 4 |
| `-maxchop <luxels>` | Largest texture light patch (at least 1). | 4 |
| `-dispchop <luxels>` | Patch size on displacements for texture lights and bounced light (at least 1). | 8 |
| `-noskyboxrecurse` | The 3D skybox casts no shadow on the map and gets no light from it. | off |

## Speed

Menu group **Speed**.

| Flag | Step | Effect | Default |
|---|---|---|---|
| `-low` | both | Runs at idle priority: the computer stays responsive. | off |
| `-fast` | lighting | Quick preview: few rays, no supersampling, one ambient sample per leaf, no static prop vertex lighting, much faster. | Draft preset |
| `-fastambient` | lighting | Quicker, coarser ambient light of the props and dynamic objects. | off |
| `-luxeldensity <factor>` | lighting | Scales every lightmap resolution by this factor, below 1 only (coarser and faster lighting). | 1 |

**Keep a core for the editor** is not `-low`: it lowers the automatic thread count by one.

## Debugging

Menu group **Debugging**.

| Flag | Step | Effect |
|---|---|---|
| `-nomerge` | geometry | Does not merge the faces cut by the BSP back together. |
| `-nosubdiv` | geometry | Does not cut the faces for their lightmaps (large faces fail). |
| `-notjunc` | geometry | Does not fix the T-junctions (small cracks may show). |
| `-noprune` | geometry | Does not merge the solid nodes of the BSP tree. |
| `-propdebug` | geometry | Lists in the log every prop whose model misbehaves (missing or broken model, no collision model, bad skin or body group, outside the world, not a static prop, too many vertices for the prop lighting), with its entity, model and position. |
| `-trace <cluster> <cluster>` | geometry | Writes the line of sight between two clusters to `<map>.lin`. |
| `-verbose` | geometry | Prints the options of the compile. |
| `-StaticPropNormals` | lighting | Static props show their vertex normals instead of their lighting. |

## FAQ

**Which flags make the biggest difference to compile time?** `-fastvis` against `-vis`, `-fast` and `-bounce 1` for the lighting, `-final`, `-StaticPropLighting` and `-nostaticpropshadows` on maps with many props. See [Compile performance](performance.md).

**I typed a flag in the extra arguments and it does nothing.** Check the log: an unknown flag gives a warning. Also check that the options menu does not force it off.

**Can I use the VRAD option `-StaticPropLighting` in Normal?** Yes: force it on in the lighting options menu (it still needs **Light static props (per vertex)** on).

## Related pages
- [Command line](command-line.md)
- [Compile and run window](compile-window.md)
- [Worldspawn and light keys](worldspawn-keys.md)
- [Lighting](../guides/lighting.md)
- [Compile performance](performance.md)

---
[Back to the documentation index](../README.md)
