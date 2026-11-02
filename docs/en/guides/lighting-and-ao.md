[Documentation](../README.md) > Guides > Lighting and ambient occlusion | [Français](../../fr/guides/lighting-and-ao.md)

# Lighting and ambient occlusion

This guide lights a map from the first light to the final compile: place lights, preview them in the editor (real time, then baked), compile in LDR and HDR, add ambient occlusion and light the static props.

## Step 1: place the lights

Place light entities with the entity tool:

| Entity | Use |
|---|---|
| `light` | A point light that shines in every direction. |
| `light_spot` | A spot light with an inner and outer cone, aimed with its angles. |
| `light_environment` | The sun and the sky light, through the faces with the sky texture. One per map. |
| `light_dynamic` | A light computed by the game at run time (not baked). |

Texture lights (materials that emit light, listed in `lights.rad`) also light the map at compile time. The color and brightness of a light are its `_light` key: select the light and press `C` (**Edit > Edit color...**) to pick the color.

## Step 2: real time preview (Lit mode)

Press `Alt+5` (**View > 3D render mode > Lighting preview**). The 3D view draws the light entities with the same falloff and spot cones as the compiler, `light_dynamic`, and the sun of the first `light_environment` with its soft shadows, lighting only what sees the sky like VRAD, props included. Its `_ambient` sky light only reaches what sees the sky (setting `render.sky_occlusion`), and the HDR light values are used like the game on an HDR map (setting `render.light_preview_hdr`). The point and spot lights nearest the camera cast shadows too: how many is the setting `render.light_shadows` (0 to 16, 0 for the sun only). It updates as you move lights, but it has no bounced light, so rooms lit only by bounced light look darker than in game.

## Step 3: baked preview

For the real look of the compiled map (bounced light, lightmaps, props lit like in game):

1. Press `Ctrl+F5` (**View > 3D render mode > Baked lighting preview**).
2. Bake: **Map > Lighting > Bake lighting** (whole map) or **Map > Lighting > Bake around the camera** (only what lies within the **Radius around the camera**, faster on big maps). **Map > Lighting > Stop baking** stops it.
3. Open the **Lighting** tab (**View > Panels > Lighting**) to set the preview:
   - **Quality**: **Draft** (same light levels, noisier shadows), **Normal**, **Final** (slowest).
   - **Bounced light**: light reflected by surfaces (slower bakes).
   - **Dynamic updates** with an **Update delay**: bakes again by itself once your edits stop.
   - **Lightmap budget**: the estimated size of the lightmaps against the 2 GB a face can address in them, with the most expensive faces.
4. The preview follows the dynamic range of the compile settings (LDR or HDR).

The baking threads and the luxel budget of the preview are on the **Lighting** page of the settings. Details: [Lighting preview](../features/lighting-preview.md).

## Step 4: lightmap scale

The lightmap scale of a face is its number of units per luxel: lower is finer lighting and a bigger map. Set it per face in the texture tool, or per displacement in the displacement tool. **View > 3D render mode > Lightmap grid** (`Alt+8`) shows the luxels colored by scale, and **Map > Lighting > Select faces with a tiny lightmap scale** finds the faces that cost too much.

## Step 5: compile with LDR and HDR

In **File > Compile and run...** (`Ctrl+F9`), the **Dynamic range** row sets what the lighting computes:

| Choice | Effect |
|---|---|
| **LDR** | Standard dynamic range only: the fastest, what most players see. The default. |
| **HDR** | High dynamic range only. |
| **LDR + HDR** | Both (two passes), needed for a release. |

**Final** always computes both, **Draft** computes one. The **Preset** sets the quality: Draft uses `-bounce 1 -fast`, Final uses `-final -StaticPropLighting`. For a quick look at the real lighting, compile with **Normal**; for the release, **Final**.

## Step 6: ambient occlusion

Hammer Pro's lighting adds ambient occlusion, which VRAD does not have for lightmaps: contact shadows in corners, creases, at the foot of walls and under props. It only darkens the bounced light near close geometry; direct light and sky light keep their own shadows.

By default it is **on for Normal and Final, off for Draft**. Control it per map in **Map > Map properties...** (these keys are shown whatever the FGD):

| Map property | Key | Effect | Default |
|---|---|---|---|
| **Ambient occlusion (Hammer Pro)** | `_ao` | **Compile default**, **On** or **Off**. | Compile default |
| **Ambient occlusion radius** | `_aoradius` | How far geometry darkens a surface, in units (4 to 1024). | 96 |
| **Ambient occlusion strength** | `_aostrength` | 0 (none) to 1 (a closed corner loses all its bounced light). | 0.75 |
| **Ambient occlusion rays** | `_aosamples` | Rays per luxel and per static prop vertex (4 to 512). More is smoother and slower. | 32, 64 with Final |

Or per compile, with the lighting options of the compile window (the menu **All lighting options (hammerpro --rad)** or the **Extra lighting arguments**): `-ao`, `-noao`, `-aoradius N`, `-aostrength N`, `-aosamples N`. These options win over the map keys.

Tips: a smaller radius keeps the shadows tight in the corners, a larger one darkens wider areas around big shapes. Lower the strength if corners look dirty.

## Step 7: light the static props

In the **Static props** section of the compile window:

- **Light static props (per vertex)**: when the quality asks for it (Final), the lighting bakes the light of every static prop vertex (`-StaticPropLighting`): shadows, bounced light and ambient occlusion on the props. Slow on big maps. Off: the game lights the props itself.
- **Static prop shadows**: props cast shadows in the lighting. Off: no prop shadow at all, and their models are not even loaded (`-nostaticpropshadows`), much faster on maps with many props.

You can also force `-StaticPropLighting` with a Normal compile through the lighting options menu. Each flag is described in [Compile options](../compiler/compile-options.md).

## FAQ

**The baked preview and the game do not look the same.** Check that the preview and the compile use the same dynamic range, and that the game shows HDR only if you compiled HDR.

**Corners look too dark.** Lower **Ambient occlusion strength**, or set **Ambient occlusion (Hammer Pro)** to **Off** for this map.

**Props are black or flat.** Compile with **Final** (or force `-StaticPropLighting`) so prop vertices get baked lighting, and keep **Static prop shadows** on.

## Related pages

- [Lighting preview](../features/lighting-preview.md)
- [Compile options](../compiler/compile-options.md)
- [Worldspawn keys](../compiler/worldspawn-keys.md)
- [Compile window](../compiler/compile-window.md)
- [Cubemaps](../compiler/cubemaps.md)
- [Player view](../features/player-view.md)

---
[Back to the documentation index](../README.md)
