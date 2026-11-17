[Documentation](../README.md) > Compiler > Cubemaps | [Français](../../fr/compiler/cubemaps.md) | [Русский](../../ru/compiler/cubemaps.md)

# Cubemaps

`env_cubemap` entities give reflective materials (`$envmap env_cubemap`) the picture they reflect. Hammer Pro prepares them at compile time like VBSP, can build them in game during a release, and can also write the cubemaps it draws in the editor straight into the compiled map.

## What the compiler does

During the geometry step:

- Every `env_cubemap` becomes a cubemap sample of the BSP (and leaves the entity lump). A map holds 1024 samples at most.
- Every face whose material uses `$envmap env_cubemap` gets a patched copy of its material, `materials/maps/<map>/<material>_<x>_<y>_<z>`, pointing at the cubemap of the nearest sample in front of it, or the one an `env_cubemap` picked in its **sides** key. Materials named by `$bottommaterial`, `$crackmaterial` and `$fallbackmaterial` are patched too when they use the cubemap.
- `cubemapsize`: 0 is the default size, otherwise the cubemap is 2^(size-1) texels wide.
- The pakfile gets a placeholder `c<x>_<y>_<z>.vtf` (and `.hdr.vtf`) per sample and `cubemapdefault.vtf`. Like Garry's Mod's VBSP, these are black 32x32 reflection placeholders until real cubemaps are built. LDR, HDR and compressed HDR sky materials are recognized, including `_hdr` faces.
- If the requested sky has missing or invalid files, the compiler uses an available standard sky and logs its name. The compiled BSP gets that fallback `skyname`; the source VMF is unchanged. Mount the original sky's content to keep its appearance. Procedural `painted` skies are preserved. If no fallback is available, the missing sky is reported instead of pretending its assets exist.
- Missing reflection cubemaps do not disable baked surface lighting. A completely dark lighting pass has its own warning: check lights, `light_environment`, sky openings and HDR brightness scales.
- The pakfile stays stored (not compressed): the engine only builds cubemaps into stored pakfiles.

## Building the real cubemaps

There are four ways:

0. **At compile time**: **Build cubemaps on compile** in the compile window (row **Cubemaps**). At the end of the compile every `env_cubemap` (yours, and the automatic ones of `-autocubemaps` and ray tracing) and the default cubemap are rendered from the lit map (final lightmaps, textures, static props, the 2D sky and the 3D skybox, light sources as bright spots, bounced light) and written where `buildcubemaps` would, LDR and HDR as the map has them. The map has correct reflections the first time it loads. It works with every lighting: Hammer Pro's classic lighting, its ray traced lighting (`-raytracing`), on the graphics card or on the processor, and with Valve's `vrad.exe` or any other external compiler (Hammer Pro then runs `hammerpro --cubemaps <map>` on the BSP right after the lighting step). **Auto** (default) turns it on with the **Final** preset and with ray tracing at the Final or Optimal quality; **Always** on every compile that computes the lighting; **Off** leaves the placeholders to `buildcubemaps`. On the command line: `-buildcubemaps` and `-nobuildcubemaps` on the lighting step (`-rtreflections` and `-rtnoreflections` are the same switches, kept for older scripts). This was **Ray traced reflections** before 2.2.2: the old setting is carried over once.

1. **In game, by hand**: load the map and run `buildcubemaps` in the console. The game writes them into the BSP in its `maps` folder.
2. **During a release**: in the [Build and save as content](../features/build-content-release.md) window, **Build cubemaps and navigation in game** starts Garry's Mod (after a confirmation), builds the cubemaps (LDR and HDR) and the navigation mesh, then closes it. The checklist shows how many are built. Off by default.
3. **From the editor**: open the [player view](../features/player-view.md) (`F6`) on the compiled map with its compiled lighting (`L`), let the cubemaps draw, then **Map > Lighting > Write cubemaps into the compiled map**. The cubemaps the editor drew are written into the pakfile of that BSP under the names `buildcubemaps` uses, so the map ships with them without starting the game. They then count as built.

## Built cubemaps survive recompiles

A compile writes placeholders again. To keep your built cubemaps:

- The **Copy to the maps folder** step carries the built cubemaps of the map it replaces into the new map, for every `env_cubemap` that kept its origin and size. The log says "Kept the N cubemap files built in game in ...".
- A release does the same with **Keep the built cubemaps of the previous BSP** (on by default, `keep_cubemaps` in `[content]`).

A cubemap counts as built when its file differs from the placeholder. Move or resize an `env_cubemap` and its cubemap must be built again.

## Cubemaps in the editor views

- **View > Environment preview > Cubemap reflections**: materials with `$envmap` reflect their cubemap in the 3D view (textured, lit and baked modes) and the player view. The editor draws a cubemap at every `env_cubemap` (the sky when there is none); each face takes the nearest `env_cubemap` in front of it like the compiler, props the nearest one. The player view uses the cubemaps built in game when the compiled map has them.
- **Redraw the cubemaps after edits** (`[render] cubemaps_auto`, on by default) draws them again a moment after the map, its lighting or the view mode changes, a few faces a frame. Off: only **Map > Lighting > Rebuild cubemaps** draws them again (new and moved `env_cubemap` entities still get theirs).

See [Lighting preview](../features/lighting-preview.md) for the preview side.

## Parallax corrected cubemaps in Garry's Mod

A plain cubemap is a picture taken from one point: on a big glossy floor the reflection of the walls slides with the camera and sits in the wrong place, the farther from the `env_cubemap` the worse. Parallax correction (CS:GO, Mapbase) bends the reflection with the box of the room so it stays where the room really is. Garry's Mod's stock shaders cannot do it, so Hammer Pro ships its own shader inside the map, no addon needed.

Turn on **Parallax corrected cubemaps** (row **Cubemaps** of the compile window, `-parallaxcubemaps` on the geometry step). **Off** is the default for now; **Auto** turns it on with ray tracing at the Final or Optimal quality, **Always** for every compile with the Hammer Pro compiler. Keep **Build cubemaps on compile** on too, so the cubemaps hold the real room.

What the compiler does:

- Every `env_cubemap` gets the box of its room: rays from the cubemap to the walls, the floor and the ceiling (the median of nine rays per side, so a pillar or a doorway does not shrink it), or the brush of the `parallax_obb` its `parallaxobb` key names. A cubemap outdoors or in an open area gets no box and keeps the plain reflection. You can also give the box yourself with the key `hp_pcbox` "minx miny minz maxx maxy maxz".
- Every reflective world face (`LightmappedGeneric` with `$envmap env_cubemap`, not translucent, not alpha tested, not a blend or a displacement) whose cubemap has a box and that lies in that box keeps its material for the diffuse and the lightmap (so its lighting does not change) with the stock reflection turned off (`$envmaptint 0` in its patched material), and gets an overlay drawn with `screenspace_general` and Hammer Pro's box projected cubemap shader (`shaders/fxc/hp_boxcube_vs20.vcs` and `hp_boxcube_ps20b.vcs` in the pakfile). The overlay adds the reflection: the view ray reflected on the face is cut by the room box and the cubemap is read toward that point from the `env_cubemap`. It keeps the material's `$envmaptint`, `$envmapcontrast`, `$fresnelreflection` and its mask (`$envmapmask`, `$basealphaenvmapmask`, `$normalmapalphaenvmapmask`), and fades in fog.
- Coplanar faces of the same material and cubemap share one overlay (64 faces at most per overlay, the engine limit). If the map would go over the overlay limit (`MAX_MAP_OVERLAYS`), whole materials keep the plain reflection and the log says so.
- The log says how many cubemaps got a box and how many faces got the corrected reflection.

What it can and cannot do (honestly):

- The reflection is exact for what lies on the walls, floor and ceiling of a box shaped room: a glossy floor reflects the walls, the posters and the far wall where they really are, at every camera position. For props, pillars and furniture inside the room, and for rooms that are far from a box (L shapes, slopes, vaults), the reflection is closer to right than a plain cubemap but still not exact: this is the same limit as CS:GO and Mapbase.
- The reflection is flat: the normal map still shapes the diffuse light, but it does not ripple the reflection like the stock shader does. `$envmapsaturation` is not applied.
- Only world brush faces: displacements, `WorldVertexTransition` blends, models and props, water and translucent glass keep the stock reflection.
- It needs the Hammer Pro compiler (the faces are prepared by its geometry step); with Valve's `vbsp.exe` only **Build cubemaps on compile** applies.
- It needs a Garry's Mod that loads shaders from the map (current Garry's Mod does). Where a custom shader cannot run (DirectX 8 mode), those faces show no reflection.

### Mapbase and other parallax capable shaders

Like Mapbase's VBSP: give an `env_cubemap` a `parallaxobb` key naming a `parallax_obb` brush entity. The compiler patches its materials with `$envMapParallax 1`, `$envMapParallaxOBB1` to `3` (the box of the brush) and `$envMapOrigin` (the sample's origin), then removes the `parallax_obb` entities and their brushes. The shaders of Mapbase (and of other games with parallax capable shaders) correct the reflection with them; the stock shaders of the other games ignore the keys. A Mapbase mod is recognized in **Settings > Game**.

## FAQ

**My reflections are black in game.** The placeholders are black until the cubemaps are built: compile with **Build cubemaps on compile**, run `buildcubemaps`, use the release game session, or write them from the editor. An `env_cubemap` inside a solid brush also gives a black cubemap ([Check for problems](../features/check-problems.md) reports it).

**I built the cubemaps in game, then recompiled, and they are gone.** Only `env_cubemap` entities that kept their origin and size keep their cubemap, and only through the copy to the maps folder (or the release option).

## Related pages
- [Lighting preview](../features/lighting-preview.md)
- [Player view](../features/player-view.md)
- [Build and save as content](../features/build-content-release.md)
- [Worldspawn and light keys](worldspawn-keys.md)

---
[Back to the documentation index](../README.md)
