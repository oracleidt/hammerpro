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
| `-raytracing`, `-rtquality`, `-rt...` | lighting | Ray tracing and each of its parameters (see [Ray tracing](#ray-tracing)). Only for Hammer Pro's compiler: Valve's vrad never gets them. | **Ray tracing** row and the **Ray tracing** options |

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

## Optimization

Menu group **Optimization** (geometry step). Every option here is off by default, in every preset: turn on the ones you want. They change how the map is stored, not how it looks or plays, and they can be combined.

| Flag | Effect | Default |
|---|---|---|
| `-cullverts` | Fewer vertices (MAX_MAP_VERTS): faces are cut for their lightmaps only where the game's lightmap size limit needs it (125 luxels in Garry's Mod instead of 32, so the same luxels with fewer seams), vertices on straight edges that no face needs are dropped, the original faces (a lump the game does not read) reuse the vertices of the drawn faces, unused vertices are removed and brush entities share vertices with the world. Faces longer than 32 vertices are cut along chords that leave no T-junction. | off |
| `-culltjunc` | T-junction vertices are only added where another drawn face really continues along the same edge: none come from nodraw, skip or hint faces, faces that are never drawn are not fixed, and the check runs again until no crack is left. Fewer edges and surface edges. | off |
| `-cullbrushes` | Leaves out the brushes that change nothing: exact duplicates and brushes inside another solid brush (same contents, same texture on shared sides), flat brushes, structural solid brushes no playable space touches (buried in other brushes or out in the void, never on a leaking map, never one holding an entity), and merges nodraw boxes side by side with the same contents and texture into one. Fewer brushes, brush sides and leaf brushes; collision, sealing, lighting and the look stay the same. Brushes with displacements, overlays or clip contents are never touched. | off |
| `-autolightmapscale` | Automatic lightmap scale. Faces never seen up close get a coarser scale from their distance to the nearest walkable area in front of them (floors and displacements, plus the height of a player): 16 up to 256 units, 32 up to 1024, 64 up to 2048, 128 beyond (high ceilings, roof undersides, faces far above or away from where players walk). Faces of the 3D skybox (the sky shell around `sky_camera`) get 32. Small faces within 64 units of a walkable area and 256 units of a light get 8, the only finer change. Only world and `func_detail` faces left at the default scale (16) change: a scale the mapper set is kept. The log gives the estimated lightmap memory before and after. | off |
| `-cleanpak` | Removes from the pakfile the materials, textures, models, sounds and particle files nothing refers to (the entities, the faces, the static props, and the materials, models, scripts, Lua files and particle manifests kept in the pakfile), then compresses the pakfile with LZMA (Garry's Mod reads it; sounds stay uncompressed as the engine needs). The cubemaps of the map (`materials/maps/<map>/`), the skybox materials and every other kind of file are always kept. It runs at the end of the geometry step and again after the packing step. Build the cubemaps in game before: the engine builds cubemaps only into uncompressed pakfiles (cubemaps already built in the copy of the maps folder are carried over as before). | off |
| `-autocubemaps` | Every `env_cubemap` left at the default size gets a size from the area of the reflective faces it serves: 16 pixels for almost nothing, 32 when the faces it serves span up to 512 units, 64 up to 1536, 128 beyond. A size set by the mapper is kept. Cubemaps no face uses and that are not the nearest to any prop are removed from the map, with their placeholder textures. | off |
| `-mergestaticprops` | Nearby `prop_static` props with the same model or the same materials, the same skin and the same lighting keys (shadows, vertex lighting, fade, render color...) are combined into one static model per group, built with the model compiler of **Settings > Compile > Model compiler** (studiomdl by default), the same way as [Combine props](../features/propper.md). Props are grouped by room area: only props less than 384 units apart, in the same 512 unit cell and with no solid wall between them, so the visibility still culls each group. Named props (targetname, parentname, outputs) are never merged. The models go to `<map>_merged_props` beside the map and are packed into the BSP. Up to 4 groups are built at the same time (half the processor cores at most), each model compile is stopped after 3 minutes (that group is left as is) and the log shows the progress, for example `Optimizing 25%: Merging static props: 8 of 32 groups done, 12 s elapsed, about 36 s left`. | off |
| `-autohints` | The compile builds the visibility once, measures the hint brushes the [optimizer](../guides/optimizing-a-map.md) suggests (up to 8, by compiling with each one) and adds only those that lower the visibility totals, checked together too. The measurements always use fast visibility compiles, even with `-vis`, and stop after 3 minutes (the hints measured so far are kept). The log shows each measurement with the time left. Slower: one extra compile per candidate. Needs `-vis` or `-fastvis`: without visibility the option is skipped with a note in the log. | off |
| `-autofade` | `prop_static` props without a fade distance (`fademaxdist` empty or 0, no screen space fade) get one from their size: from 800 units for small props up to 4000 units at most, the fade starting at 75% of it. Props bigger than 400 units across and props in the 3D skybox keep no fade. When the map has detail props and no `env_detail_controller`, one is added with a fade from the size of the detail sprites and models (400 to 1200 units, never farther than the game default). | off |
| `-autooccluders` | Big straight box walls (at least 256 by 128 units and 8 thick, opaque textured faces on both big sides) that hide at least 3 props from a side where entities stand get a thin `func_occluder` just off their face, so the engine skips drawing the props behind them. At most 16, the biggest first, never in the 3D skybox. | off |

While these options run, the geometry step shows their stage and percent in the compile window (lines starting with `Optimizing`), with a new line at least every 5 seconds, and the log starts with the list of the optimization options that are on. Cancel stops the step and the model compiles it started.

How they overlap: `-culltjunc` stops unneeded T-junction vertices from being added, `-cullverts` removes the ones that are left once every face is known and also saves the vertices `-culltjunc` does not touch (lightmap cuts, original faces, unused ones), so `-cullverts` alone gives most of the gain and both together give the fewest edges. `-cullbrushes` works on brushes before the faces are built, so a little less geometry reaches the other two. On the test maps `-cullverts` removed 18 to 50 percent of the vertices.

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

## Ray tracing

Menu group **Ray tracing** (lighting step), and the **Ray tracing** row and options of the [Compile and run window](compile-window.md). Off by default, in every preset.

Ray tracing replaces the classic radiosity of the lighting step with a physically based path tracer. It writes the same lightmap formats, so the map loads in the same games and looks right in game and in the [player view](../features/player-view.md).

**What it computes.** Unbiased Monte Carlo global illumination: light paths bounce many times (paths on dark surfaces stop early by Russian roulette, without bias). Direct light is next event estimation toward every Source light type, sampled many times per luxel: `light`, `light_spot` with its cones and exponent, `light_environment` with the sun given an angular size for soft shadows and the sky dome ambient light, texture lights from `lights.rad` and self illuminated surfaces (`$selfillum` materials become area lights, from their glowing color). At every bounce the path reads that converged direct light from the lightmap it hits (static props and unlit faces sample one light by next event estimation), so many bounces stay noise free. The falloff follows VRAD's rules (`_constant_attn`, `_linear_attn`, `_quadratic_attn`, `_fifty_percent_distance`, `_zero_percent_distance`, `_hardfalloff`) so a light is as bright as with VRAD. Alpha tested and translucent surfaces, glass tinting, water, the 3D skybox, func_detail, displacements and static prop shadows from their real meshes are all traced. Bounced light takes the color of the surfaces it leaves, from the average of their base texture like VRAD's reflectivity.

**Sampling.** Stratified low discrepancy samples; adaptive sample counts, so luxels in penumbras and dark corners take more samples than plain lit walls; several sample points per luxel, placed inside the open part of the luxel, so no light leaks under walls and there is no shadow acne; an edge aware denoiser that keeps shadow edges sharp; seams between faces are stitched.

**Outputs.** On gm_construct (750,000 luxels, RTX 3060 Ti) the Normal quality takes about 35 s with static prop lighting, against 10 s for the classic lighting; its lightmaps are within 2% of Valve's VRAD in median brightness. LDR and HDR lightmaps, bumped lightmaps for normal mapped surfaces, light styles (switchable lights), ambient cubes for models, static prop vertex lighting and displacements.

**GPU and CPU.** On the GPU it runs as wgpu compute shaders over a BVH built on the CPU with the surface area heuristic. On large scenes (300,000 triangles and more) and cards with hardware ray queries, it times both on a test batch and keeps the faster one (the log says which); otherwise the BVH is traversed in compute. Sample points stay on the card when they fit, the scene is checked against the VRAM budget, and very large maps get fewer sample points per luxel so memory stays bounded (at most 24 million points). The work goes in batches so the graphics driver never times out, and it respects the VRAM budget (setting `render.vram_budget_mb`, or the environment variable `HAMMERPRO_VRAM_BUDGET_MB`). Progress and cancel work like the other stages. The ray traced preview of the editor uses the same engine in short slices of about 8 ms on the card, three in flight, sized from the card's own timings, and leaves the editor its GPU share (Preview GPU share), so navigation keeps its frame rate. Without a usable GPU it runs the same algorithm on the CPU at below normal priority, and the log says `No GPU detected, compiling with Ray Tracing will take longer.` (in the editor's language); the compile window shows that line as a warning.

**Quality levels.** **Draft** for quick previews, **Normal** for everyday compiles, **Final** for release, **Optimal** where the compiler picks the best balance between quality, map size and compile time for this map and this computer and writes in the log what it chose and why: samples and bounces from the luxel count and from GPU or CPU, 3, 2 or 1 sample points per luxel side up to 1.5 and 5 million luxels, and a lightmap budget of 48 MB per pass (the luxel density is lowered when the lightmaps would not fit). The log ends with the lighting lump size against the engine limit. The same seed with the same settings gives the same result. In Final and Optimal the shadows are exact: static props cast from their render mesh (like `-StaticPropPolys`), every prop with an alpha tested or translucent material casts the pattern of its texture (foliage, fences, grates; like `-textureshadows` without needing `$casttextureshadows`), and a prop whose model has no VTX file falls back to its collision hull instead of stopping the shadows of the props after it.

The **Optimal** button of the **Ray tracing** options sets the quality to Optimal, puts every parameter back to automatic and turns on `-autolightmapscale` of the geometry step (finer lightmaps near walkable areas and lights, coarser elsewhere). The engine limits are never exceeded.

| Flag | Option in the window | Effect | Default (draft, normal, final) |
|---|---|---|---|
| `-raytracing [quality]` | **Use ray tracing** | Turns ray tracing on. | off |
| `-rtquality <q>` | **Quality** | `draft`, `normal`, `final` or `optimal`. | `normal` |
| `-rtsamples <n>` | **Samples per luxel (min)** | Samples every luxel takes at least. | 16, 64, 256 |
| `-rtmaxsamples <n>` | **Samples per luxel (max)** | Most samples a luxel takes where the noise stays high. | 64, 256, 1024 |
| `-rtbounces <n>` | **Bounces** | Most light bounces. | 3, 6, 12 |
| `-rtthreshold <f>` | **Noise threshold** | A luxel stops taking samples when its noise is below this level. | 0.08, 0.04, 0.02 |
| `-rtlightsize <f>` | **Light size scale** | Scale of the size of every light. Point and spot lights are area lights with a radius of 2 units in Draft and Normal, 6 in Optimal and 8 in Final (soft shadows with a real penumbra, sharp at the contact and softer farther away), shrunk around each light so the light never pokes into the wall, ceiling or lamp next to it. The sun uses its `SunSpreadAngle`, at least 1 degree in Optimal and 1.5 in Final. 0 gives VRAD's hard shadows. | 1 |
| `-rtsunangle <degrees>` | **Sun angular size** | Width of the sun for soft sun shadows. | the `light_environment` |
| `-rtluxels <1 to 4>` | **Lightmap detail** | Finer lightmaps for the ray traced bake: 2 puts twice as many luxels along each side of a face (the lightmap scale divided by 2), for sharp contact shadows under props and in corners and smooth soft penumbras. Each face keeps within the engine lightmap size limit (125 luxels per side in Garry's Mod, 32 in the other games; faces that would not fit get a smaller step or keep their size), and the lightmaps within a 32 MB per pass budget (the detail is lowered to fit). In Final the samples per luxel are divided by the detail squared (at least 64 and 256) and the supersampling by the detail (at least 2) so the samples per unit of surface and the compile time stay close to x1. The log gives the detail used, the faces refined and the lightmap size. Ignored with `-luxeldensity` below 1. | 1, 1, 2 (Optimal 1) |
| `-rtlook <classic, cinematic>` | **Look** | Cinematic: point and spot lights shine with a true inverse square falloff from an area source, scaled to keep their brightness at their reference distance (the `_fifty_percent_distance` or where the authored falloff halves, 64 to 512 units), and fade smoothly to zero at their authored reach: a bright pool under each lamp, smooth gradients on walls and floors, darker far areas; near the light the falloff is softened (at least 14 units) and each light is then limited so no lit point gets more than 1.6 times the classic direct light (2.2 straight under a ceiling fixture), beyond the light reach it fades smoothly: no white hot spots on walls next to a lamp. A point light with a ceiling less than 32 units above it is a ceiling fixture: it shines downward only (the ceiling above stays darker) and is 1.5 times bigger. Bounced light keeps more of the surface colours (energy conserving), and the ambient occlusion becomes deep contact shadows; the typical luxel keeps its exposure. Classic keeps the authored falloff of every light, like VRAD. | cinematic in Final and Optimal, classic otherwise |
| `-rtskysamples <n>` | **Sky samples** | Sky dome directions per direct light sample. | 1, 1, 2 |
| `-rtdenoise <0 to 4>` | **Denoiser strength** | Edge aware denoiser, 0 is off. | 1 |
| `-rtsupersample <1 to 8>` | **Supersampling** | Sample points per luxel side. | 2, 3, 4 |
| `-rtnobump` | **Bumped lightmaps** off | No bumped lightmaps (smaller, flatter). | bumped |
| `-rtpropsamples <n>` | **Prop lighting samples** | Rays per static prop vertex, with static prop lighting. | from the quality |
| `-rtdevice <auto, gpu, cpu>` | **Device** | Where the path tracer runs. | `auto` |
| `-rtchunk <luxels>` | **GPU batch size** | Luxels per GPU batch. | automatic |
| `-rtseed <n>` | **Seed** | Random seed. | 0 |
| `-rtgpushare <auto, 0.1 to 1>` | **Preview GPU share** | Share of the graphics card the [ray traced preview](../features/lighting-preview.md#ray-traced-lighting-preview-ctrlshiftf5) uses while you edit. Auto keeps the editor smooth (60 fps or more); higher converges faster, lower keeps more frame rate for the editor. Only the preview gets it: compiles always use the whole graphics card. | Auto (60%) |
| `-rtlang <code>` | (the editor's language) | Language of the messages in the log. | `en` |
| `-rtnoao`, `-rtaostrength <0 to 1>`, `-rtaoradius <units>` | **Ambient occlusion**, **Occlusion strength**, **Occlusion radius** | Baked contact shadows in corners, under and at the base of static props, also in the prop vertex lighting. In Draft and Normal the occlusion darkens the bounced light by the strength; with the cinematic look (`-rtlook`, automatic in Final and Optimal) it becomes a contact occlusion (an occluder weighs more the closer it is, out to 1.5 times the radius) that darkens the bounced light twice as much (up to fully) and the direct light by 1.2 times the strength, for the deep contact shadows of path traced lighting without dimming open areas. | on, 0.5, 48 |
| `-rtnoshafts`, `-rtshaftintensity`, `-rtshaftlength`, `-rtshaftsoftness` | **Light shafts**, **Shaft brightness**, **Shaft length**, **Shaft softness** | Baked volumetric light: for each framed opening (window, door, skylight, grate, gap in a roof; never a wide open side such as a cutaway wall) and each strong spot cone with `sun_and_spots`, the compiler traces the lit air of the room behind it on a 3D grid, so the beam has exactly the shape of the opening, is cut by every frame bar, grate slat, wall, slab, prop and floor, never shows in another room and fades along its length, at its edges and near the surface where it lands. The density is turned into a few nested soft shells drawn additive and unlit, packed in the BSP as one model per beam on a prop_dynamic without collision or shadows: the light grows with the depth of air you look through, like real volumetric light. Brightness, length, softness (edge blur) and the color of the light come from these options. The log tells how many beams, triangles and where they are. | on, 1, 1, 1 |
| `-rtshaftwhite`, `-rtshaftsource <sun, sun_and_spots>`, `-rtshaftdust` | **Color of the light**, **Shaft sources**, **Dust motes** | Shaft color, the lights that make shafts, dust motes drifting in them. | light color, `sun_and_spots`, off |
| `-rtglow`, `-rtglowintensity <0 to 8>` | **Glow halos**, **Glow brightness** | Soft halos around strong lights. | off, 1 |
| `-rtnocubemaps` | **Automatic cubemaps** off | When the map has no `env_cubemap` of its own, the geometry step places `env_cubemap` entities where reflective surfaces need them, with the sizes of the cubemap analysis (the same as `suggest_cubemaps`), in the compiled map only (the VMF is never changed), and sizes the cubemaps from what they reflect (`-autocubemaps`). A map that has its own cubemaps keeps them (sized) and gets new ones only in the reflective rooms and facades none of them covers, at least 256 units from its cubemaps, never past the engine limit of 1024. The log says how many were placed and why. Ray traced reflections (next row) render them in the compile, otherwise `buildcubemaps` in game does. Written as `-rtautocubemaps` on the geometry step. | on |
| `-rtreflections`, `-rtnoreflections` | **Ray traced reflections**: Auto (Final and Optimal), Always, Off | At the end of the ray traced lighting every `env_cubemap` of the map (the placed ones and the automatic ones) and the default cubemap are rendered with the path tracer from the lit map itself: the final lightmaps and textures, static props lit by path traced direct and bounced light, the 2D sky and the 3D skybox, unlit and self illuminated surfaces, glass, water, and light sources as bright spots. LDR and HDR as the map has them, at each cubemap's size with box filtered mipmaps, in the formats `buildcubemaps` writes (DXT1 and RGBA16161616F, VTF 7.4 with the sphere map face), into the pakfile under `materials/maps/<map>/c<x>_<y>_<z>.vtf` and `.hdr.vtf` and `cubemapdefault`, where the patched materials of the geometry step already point. Reflections are right the first time the map loads, no `buildcubemaps` needed, and they count as built cubemaps (a copy to the maps folder keeps them). The face layout is the engine's (right, left, back, front, up, down, as the game samples them). If all the cubemaps would take more than 96 MB the largest are rendered smaller. Auto turns it on with the Final and Optimal qualities. | Auto |
| `-rtnotonemap` | **Tonemap hints** off | An `env_tonemap_controller` with sensible bloom and exposure for HDR, only when the map has none. | on |
| `-rtcolorcorrection` | **Color correction** | Subtle automatic color correction. | off |

The **Ray tracing** options appear under the lighting options only when **Ray tracing** is ticked. Their **Configuration** is **Default** when ray tracing is turned on (quality Normal, every value automatic, the default effects), **Optimal** (what the **Optimal** button does) or **Custom**, picked by itself as soon as you change a value or choose Draft or Final. **Reset to default** goes back to Default. In the window each parameter has an **Auto** box: ticked, the value comes from the quality level and is shown in grey. The effects change the compiled map only, never the VMF. The lightmap scale, the luxel density and the ambient occlusion keep their own options (`-luxelscale`, `-autolightmapscale`, `-luxeldensity`, `-ao`). The MCP tools `set_compile_options` and `compile_with_options` take the same parameters (`raytracing`, `raytracing_quality`, `raytracing_optimal`, `rt_config`, `rt_reset`, `rt_samples`, `rt_ao`, `rt_shafts`, `rt_glow`...), see [MCP server](../reference/mcp-server.md).

## Ambient occlusion

Menu group **Ambient occlusion** (lighting step). Baked ambient occlusion like VRAD++: corners, crevices, the foot of walls and the ground under crates get darker in the lightmaps. It is off by default, in every preset (Final too): turn it on with `-ao` (or `-ambientocclusion`, like VRAD++). The editor previews (Lit, Baked) never show it; the compiled map does, in game and in the [player view](../features/player-view.md).

| Flag | Effect | Default |
|---|---|---|
| `-ao` | Bakes ambient occlusion. `-ambientocclusion` is the same; `-ao 0` and `-noao` turn it off again. | off |
| `-aoradius <units>` | How far geometry darkens a surface. In the 3D skybox it is divided by the `sky_camera` scale, so the skybox gets the same occlusion as the full size world. | 32 |
| `-aoscale <0 to 1>` | Strength: 0 changes nothing, 1 lets fully enclosed luxels go black. Values out of range are clamped with a warning. | 0.5 |
| `-aofacesamples <rays>` | Rays per luxel. | 32, 16 with `-fast`, 64 with `-final` |
| `-aopropsamples <rays>` | Rays per static prop vertex, with `-StaticPropLighting`. | 32, 16 with `-fast`, 64 with `-final` |
| `-aodebug` | The lightmaps show only the occlusion term (white where open, dark in corners) instead of the light, and baked prop vertices too. Turns `-ao` on. | off |

The radius, scale and sample options do nothing without `-ao` (the log says so). A number of rays given with `-aofacesamples` or `-aopropsamples` is used as is (1 to 1024); only the defaults follow `-fast` and `-final`. The older `-aostrength` and `-aosamples` are read as `-aoscale` and `-aofacesamples`.

### How it is computed

1. For each luxel, from the same point the direct light uses (1 unit above the surface), N rays (`-aofacesamples`) leave in a cosine weighted hemisphere around the surface normal (the smoothed normal on displacements) and stop at the radius R. They hit world brushes, displacements and static props; sky faces, water, glass and alpha tested leaves do not occlude.
2. Each ray that hits at distance t occludes with a smooth falloff: `AO = 1 - (1/N) x sum of (1 - (t / R)^2)` over the rays that hit. AO is 1 in the open and goes down near geometry.
3. The AO of each luxel is averaged with the luxels around it (within one and a half luxels, facing the same way, across faces and displacements): no noise, no seam between faces, lightmap pages or displacements.
4. Like CS:GO's VRAD and VRAD++, the occlusion multiplies the final lightmap, blended toward 1 by the scale: `lightmap = (direct + bounced) x (1 - scale x (1 - AO))`. It applies to every light style, to LDR and HDR and to the bump map directions, after the bounces (the occlusion does not darken the light that bounces), before the luxels shared by neighbor faces are matched.
5. The ambient light of props and players (leaf ambient cubes) and the detail props take their light from the occluded lightmaps. With `-StaticPropLighting`, each prop vertex gets the same factor with `-aopropsamples` rays (props that do not shadow themselves, `-nossprops`, are not occluded by their own model).
6. With `-aodebug` the lightmaps hold `128 x AO^2.2` in their first light style (the others are black): the gray you see in game follows AO, open floors are white and corners gray.

The cost: the AO stage (`ambient occlusion` in the stage list and in `Light phase times`) traces short rays on the BVH the lighting already built. On the test maps it adds about 10% to the lighting time: 2 seconds for the 790,000 luxels of gm_construct on 4 threads (32 rays per luxel, 22 seconds of lighting without it), 30 to 200 ms on small maps.

## Static props

Menu group **Static props** (lighting step).

| Flag | Effect | Default |
|---|---|---|
| `-StaticPropLighting` | Bakes the lighting of every static prop vertex (shadows and bounced light) instead of letting the game light props at run time. | Final preset, when **Light static props (per vertex)** is on |
| `-StaticPropPolys` | Static props cast shadows with the triangles they draw instead of their collision model (finer shadows, slower). Models listed with `forcetextureshadow` in a `.rad` file always do (see [Light file (.rad)](#light-file-rad)). | off |
| `-textureshadows` | With `-StaticPropPolys`: alpha tested prop textures (leaves, fences) of models compiled with `$casttextureshadows` let light through. | off |
| `-nossprops` | Static props never shadow themselves. | off |
| `-staticpropsamplescale <factor>` | Scales the number of rays that light static prop vertices (with `-StaticPropLighting`). | 1 (4 with `-final`) |
| `-nostaticpropshadows` | Hammer Pro option: static props cast no shadow at all (lightmaps, bounced light, detail props, ambient light, prop lighting); their models are not even loaded. Written by **Static prop shadows**. | off |

## Texture lights and sky

Menu group **Texture lights and sky** (lighting step).

| Flag | Effect | Default |
|---|---|---|
| `-lights <file>` | Extra texture light file, read after `lights.rad` and before the map's own light files. Written by **Use a .rad file** (see [Light file (.rad)](#light-file-rad)). | not used |
| `-chop <luxels>` | Smallest texture light patch (at least 1). | 4 |
| `-maxchop <luxels>` | Largest texture light patch (at least 1). | 4 |
| `-dispchop <luxels>` | Patch size on displacements for texture lights and bounced light (at least 1). | 8 |
| `-noskyboxrecurse` | The 3D skybox casts no shadow on the map and gets no light from it. Without it, when the scaled map lies inside or under the skybox geometry, the compile log warns that the 3D skybox hides the sun from a share of the sunlit surfaces. The baked preview follows this flag. | off |

## Light file (.rad)

The **Light file (.rad)** row of the compile window (also in the **Lighting** tab, **VRAD** page, and in **Tools > Options... > Compile**) chooses a `.rad` file for the lighting.

- **Use a .rad file** off (default): the lighting reads the game `lights.rad`, then `<map>.rad` beside the map, like before.
- On, with a file (**Browse...** or a path; a relative path starts at the map folder, `$path` works too): the file is read after the game `lights.rad` and before `<map>.rad`, so its lines win over `lights.rad`. Hammer Pro's compiler gets it as `-lights`, and so does Valve's `vrad.exe` (the `-lights` of the options menu or of a compiler profile is replaced). A missing file is shown in orange and the lighting reads the other files only.
- `<map>.rad` beside the VMF is copied next to the compiled map, so it is read even when the compile runs in the build folder.
- The Baked, Player view, Ray traced and Lit previews read the same files in the same order, and bake again within a second of a saved change to any of them.

What a `.rad` file holds:

| Line | Effect |
|---|---|
| `lights/white001 255 255 255 200` | Texture light: faces with this material emit light of this color (0 to 255) and brightness. |
| `lights/white001 255 255 255 200 255 240 220 800` | The same with HDR values: the HDR pass uses the last four numbers, the LDR pass the first four. |
| `hdr:lights/white001 255 255 255 800`, `ldr:...` | A line read by one pass only. |
| `noshadow foliage/leaves` | Parts of static prop render meshes with a material whose name contains this text cast no shadow. |
| `forcetextureshadow props/fence.mdl` | Static props of this model cast shadows from their render mesh, with the alpha of their textures (fences, trees, grates), never from their collision model. |

Hammer Pro's compiler (classic and ray traced) always shadows `forcetextureshadow` models with their render mesh and texture alpha, even without `-StaticPropPolys` or `-textureshadows`; Valve's `vrad.exe` needs both flags for it. Models compiled with `$casttextureshadows` shadow with their render mesh too when `-textureshadows` is on.

**Edit the .rad file** (Lighting tab, VRAD page, under the file) lists the texture lights with their color and brightness, adds the materials of the selected faces, and edits the `forcetextureshadow` models (also from the selected props) and the `noshadow` materials; **Save** writes the file, keeping its other lines and comments. Through MCP: `set_compile_options` `rad_file_enabled` and `rad_file`, and `edit_rad_file`.

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
| `-aodebug` | lighting | The lightmaps show only the ambient occlusion term (menu group **Ambient occlusion**, see [above](#ambient-occlusion)). |

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
