[Documentation](../README.md) > Guides > Lighting | [Français](../../fr/guides/lighting.md) | [Русский](../../ru/guides/lighting.md)

# Lighting

This guide lights a map from the first light to the final compile: place lights, preview them in the editor (real time, then baked), compile in LDR and HDR and light the static props.

## Step 1: place the lights

Place light entities with the entity tool:

| Entity | Use |
|---|---|
| `light` | A point light that shines in every direction. |
| `light_spot` | A spot light with an inner and outer cone, aimed with its angles. |
| `light_environment` | The sun and the sky light, through the faces with the sky texture. One per map. |
| `light_dynamic` | A light computed by the game at run time (not baked). |

Texture lights (materials that emit light, listed in `lights.rad`) also light the map at compile time. The color and brightness of a light are its `_light` key: select the light and press `C` (**Edit > Display colors > Edit color...**) to pick the color.

## Step 2: real time preview (Lit mode)

Press `Alt+5` (**View > 3D render mode > Lighting preview**). The 3D view draws the light entities with the same falloff and spot cones as the compiler, `light_dynamic`, and the sun of the first `light_environment` with its soft shadows, lighting only what sees the sky like VRAD, props included. Its `_ambient` sky light only reaches what sees the sky (setting `render.sky_occlusion`), and the HDR light values are used like the game on an HDR map (setting `render.light_preview_hdr`). The point and spot lights in view cast shadows too, the brightest first: how many is the setting `render.light_shadows` (0 to 64, 16 by default, 0 for the sun only). The sky light and the light the surfaces bounce on each other come from a background bake of the map with the compiler (setting `render.lit_indirect`, on by default), like the ambient part of the game's lightmaps, so rooms lit by bounced light look like in game; the lights and the sun stay real time and update as you move them, the bounced part follows a moment after you edit the map.

## Step 3: baked preview

For the real look of the compiled map (bounced light, lightmaps, props lit like in game):

1. Press `Ctrl+F5` (**View > 3D render mode > Baked lighting preview**).
2. Bake: **Map > Lighting > Bake lighting** always calculates the whole map and keeps that scope while you navigate. **Map > Lighting > Bake around the camera** calculates only a fixed region within the **Radius around the camera** at its starting position. **Map > Lighting > Stop baking** stops it.
3. Open the **Lighting** tab (**View > Panels > Lighting**) to set the preview:
   - **Quality**: **Draft** (same light levels, noisier shadows), **Normal**, **Final** (slowest).
   - **Bounced light**: light reflected by surfaces (slower bakes).
   - **Dynamic updates** with an **Update delay**: bakes again once changes affecting lighting stop.
   - **Lightmap budget**: the estimated size of the lightmaps against the 2 GB a face can address in them, with the most expensive faces.
4. Wait for the quick and refined passes, then navigate using the calculated result. Camera movement and selection do not invalidate a global bake. The preview follows the dynamic range of the compile settings (LDR or HDR).
5. Edit the lights: moving a light or changing its color, brightness or cone relights only that light and the bounced light, usually in a fraction of a second, without compiling again. **Map > Lighting > Solo the selected lights** and **Mute the selected lights** compare lights one by one; **Measure the cost of each light** finds the lights that cost the most. On big maps the area around the camera is baked first (setting **Bake the visible area first**). Dragging a light with the selection tool relights it live while the mouse button is held, and the map is prepared in the background when edits pause, so turning the baked preview on shows it at once (setting **Prepare the baked preview in the background**).

The baking threads and the luxel budget of the preview are on the **Lighting** page of the settings. Automatic activation uses the whole map by default. **Bake only around the camera on big maps** is an opt-in regional mode for automatic activation whose region follows the camera; existing saved choices are preserved. Preview brightness, color correction and the update delay do not restart the lighting calculation. Details: [Lighting preview](../features/lighting-preview.md).

## Step 4: lightmap scale

The lightmap scale of a face is its number of units per luxel: lower is finer lighting and a bigger map. Set it per face in the texture tool, or per displacement in the displacement tool. **View > 3D render mode > Lightmap grid** (`Alt+8`) shows the luxels colored by scale, and **Map > Lighting > Select faces with a tiny lightmap scale** finds the faces that cost too much.

## Step 5: compile with LDR and HDR

In **File > Compile > Compile and run...** (`Ctrl+F9`), the **Dynamic range** row sets what the lighting computes:

| Choice | Effect |
|---|---|
| **LDR** | Standard dynamic range only: the fastest, what most players see. The default. |
| **HDR** | High dynamic range only. |
| **LDR + HDR** | Both (two passes), needed for a release. |

**Final** always computes both, **Draft** computes one. The **Preset** sets the quality: Draft uses `-bounce 1 -fast`, Final uses `-final -StaticPropLighting`. For a quick look at the real lighting, compile with **Normal**; for the release, **Final**.

## Step 6: light the static props

In the **Static props** section of the compile window:

- **Light static props (per vertex)**: when the quality asks for it (Final), the lighting bakes the light of every static prop vertex (`-StaticPropLighting`): shadows and bounced light on the props. Slow on big maps. Off: the game lights the props itself.
- **Static prop shadows**: props cast shadows in the lighting. Off: no prop shadow at all, and their models are not even loaded (`-nostaticpropshadows`), much faster on maps with many props.

You can also force `-StaticPropLighting` with a Normal compile through the lighting options menu. Each flag is described in [Compile options](../compiler/compile-options.md).

## How the lighting compares with VRAD

The lighting compiler follows the VRAD rules, so a map lit by Hammer Pro looks like the same map lit by Valve's VRAD in Garry's Mod. It is checked feature by feature against the VRAD of Garry's Mod on test maps (lightmaps, light styles, bump maps, ambient light, static prop vertex colors, detail props, world lights) and in game:

- `light`, `light_spot` and `light_environment`: colors and brightness, the HDR keys (`_lightHDR`, `_lightscaleHDR`, `_ambientHDR`, `_AmbientScaleHDR`), constant, linear and quadratic attenuation, `_fifty_percent_distance`, `_zero_percent_distance`, `_hardfalloff`, `_distance`, spot cones and `_exponent`, `target`, `SunSpreadAngle`, the 3D skybox. Same light within about 1%.
- Named lights and light styles: the same switchable styles (32 and up) and the same extra lightmaps.
- Texture lights from `lights.rad`, from the map's own `.rad` file and from `-lights`. The worldspawn key `_lightsrad` is a Hammer Pro addition that VRAD ignores.
- Bump mapped surfaces, displacements, `func_detail`, the lightmap scale of each face, `_minlight`, `vrad_brush_cast_shadows`, static prop shadows and vertex lighting (`-StaticPropLighting`, `-textureshadows`), detail props, `-final`, `-extrasky`, `-noextra`, `-softsun`, `-noskyboxrecurse`, `-fastambient`, `-bounce`.
- Translucent glass, alpha tested grates (`CONTENTS_WINDOW`, `CONTENTS_GRATE`) and water cast no shadow, like in VRAD.

Details and deliberate differences:

- Bounced light and the ambient light of props and players go through water, glass and grates, like the direct light. Before 2.0 a water surface stopped them: floors under water and the ambient light above a pool were too dark.
- Light bounced off a small, very bright spot (a light a few units from a wall) is spread over a patch of about 4 luxels, like a VRAD patch, so it leaves no bright speckles on the surfaces around it.
- The bounced light is smoothed over 4 to 8 luxels (wider when fewer rays are traced, so Draft and Normal look as smooth as Final), like VRAD's patches: no blotches in places lit only by bounced light, such as shadows under a low sun.
- Luxels on the edge of a face that no light sample reaches (often on brush entities such as doors) take the light of their neighbours instead of staying black, so the face has no dark rim.
- `-fast` keeps the normal light levels; VRAD's `-fast` makes the bounced light about 8% darker.
- In open places (yards, streets) the bounced light is about 10% darker than VRAD's, which overestimates the light bounced between surfaces close to each other; the direct light and the sky light are the same. Rooms with water get about 10% more bounced light than VRAD, which dims the light crossing a water surface.
- A map without visibility (a leak) still gets bounced light, where VRAD computes the direct light only.

## FAQ

**The baked preview and the game do not look the same.** Check that the preview and the compile use the same dynamic range, and that the game shows HDR only if you compiled HDR.

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
