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

There are three ways:

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

## Parallax corrected cubemaps

Like Mapbase's VBSP: give an `env_cubemap` a `parallaxobb` key naming a `parallax_obb` brush entity. The compiler patches its materials with `$envMapParallax 1`, `$envMapParallaxOBB1` to `3` (the box of the brush) and `$envMapOrigin` (the sample's origin), then removes the `parallax_obb` entities and their brushes. The shaders of Mapbase (and of other games with parallax capable shaders) correct the reflection with them; the stock shaders of the other games ignore the keys. A Mapbase mod is recognized in **Settings > Game**.

## FAQ

**My reflections are black in game.** The placeholders are black until the cubemaps are built: run `buildcubemaps`, use the release game session, or write them from the editor. An `env_cubemap` inside a solid brush also gives a black cubemap ([Check for problems](../features/check-problems.md) reports it).

**I built the cubemaps in game, then recompiled, and they are gone.** Only `env_cubemap` entities that kept their origin and size keep their cubemap, and only through the copy to the maps folder (or the release option).

## Related pages
- [Lighting preview](../features/lighting-preview.md)
- [Player view](../features/player-view.md)
- [Build and save as content](../features/build-content-release.md)
- [Worldspawn and light keys](worldspawn-keys.md)

---
[Back to the documentation index](../README.md)
