[Documentation](../README.md) > Compiler > Worldspawn and light keys | [Français](../../fr/compiler/worldspawn-keys.md) | [Русский](../../ru/compiler/worldspawn-keys.md)

# Worldspawn and light keys

The keys of the map (worldspawn) and of the entities that the compiler reads to build and light the map. Set worldspawn keys in **Map > Map properties...**; the others are keys of the entities themselves, set in the [entity panel](../features/entity-properties.md).

## Worldspawn keys

| Key | Read by | Effect | Default |
|---|---|---|---|
| `_lightsrad` | lighting | A per map texture light file (the key Hammer++ uses). A full path, or a path relative to the game folder, the map's folder or the Garry's Mod folder. Read after `lights.rad` and `-lights` (once when it is the same file as `-lights`), before `<map>.rad`. | none |
| `detailvbsp` | geometry | The detail prop definition file for the `%detailtype` materials. | `detail.vbsp` |
| `skyname` | geometry | The sky whose textures are copied into the cubemap placeholders (see [Cubemaps](cubemaps.md)). | |

The priority is always: command line flag first, then the worldspawn key, then the default of the quality.

## Lights

The compiler reads the light entities like VRAD (`light`, `light_spot`, `light_environment`; `light_dynamic` is ignored).

| Key | Entities | Effect |
|---|---|---|
| `_light` | all | Color and brightness `r g b brightness` (made linear like VRAD). |
| `_lightHDR` | all | Color for the HDR pass; a negative value means "same as `_light`". |
| `_lightscaleHDR` | all | Multiplies the HDR brightness. |
| `style` | all | Light style (0 to 255). Named lights get a switchable style when the map has none. |
| `_constant_attn`, `_linear_attn`, `_quadratic_attn` | `light`, `light_spot` | Attenuation `1 / (c + l d + q d^2)`, the intensity given at 100 units. All 0 means constant. |
| `_fifty_percent_distance`, `_zero_percent_distance` | `light`, `light_spot` | Falloff curve through these distances (used instead of the three attenuations when the first is set). |
| `_hardfalloff` | `light`, `light_spot` | With the fifty percent distance: the light ends at the zero percent distance. |
| `_distance` | `light`, `light_spot` | The light ends there, fading over its last quarter (Hammer Pro reads it with both falloff kinds). |
| `_inner_cone`, `_cone`, `_exponent` | `light_spot` | Spot cones in degrees (inner defaults to 10, outer to the inner) and the falloff exponent between them. |
| `angles`, `pitch`, `target` | `light_spot`, `light_environment` | Direction; a `target` entity overrides it. |
| `_castentityshadow` | all | Flags the world light so the engine casts entity shadows from it. |
| `_ambient`, `_ambientHDR`, `_AmbientScaleHDR` | `light_environment` | Sky ambient light (half the sun when unset). |
| `SunSpreadAngle` | `light_environment` | Width of the sun in degrees: several sun rays inside it, softer shadows. Only the first `light_environment` lights the map. |

Texture lights come from `lights.rad` and the other light files listed above, not from entity keys.

## Brush entities

| Key | Effect |
|---|---|
| `vrad_brush_cast_shadows` | `1`: the brushes of this entity cast shadows in the lighting (brush entities cast none otherwise). |
| `_minlight` | Minimum light of the faces of the entity (a fraction, times 128 in VRAD units). |

## Static props

| Key | Effect in the lighting |
|---|---|
| `disableshadows` | The prop casts no shadow. |
| `disablevertexlighting` | No per vertex lighting for this prop with `-StaticPropLighting`. |
| `disableselfshadowing` | The prop does not shadow itself. |
| `ignorenormals` | The prop is lit with a constant cosine. |
| `lightingorigin` | The entity whose position lights the prop (also used for vertices inside solid). |
| `fademindist`, `fademaxdist`, `screenspacefade` | Written into the static prop lump; the statistics list props without fade distance. |

## Other entities the compiler handles

| Entity or key | Effect |
|---|---|
| `func_detail` | Detail brushes: they do not cut the visibility (unless `-fulldetail`). |
| `func_viscluster` | Forces the leaves inside it into one visibility cluster. |
| `func_areaportal` | Separates areas (see [Leaks and pointfiles](../features/leaks-pointfiles.md)). |
| `func_occluder` | Its brushes become occluder polygons. |
| `env_fog_controller` `farz` | The first one sets the visibility radius (like `-radius_override`). |
| `env_cubemap` `cubemapsize`, `sides`, `parallaxobb` | Cubemap size, the faces it is assigned to, the `parallax_obb` brush for parallax corrected cubemaps. See [Cubemaps](cubemaps.md). |
| `func_instance` | Merged into the map before anything else, nested instances included. See [Instances](../features/instances.md). |
| `prop_detail`, `prop_detail_sprite`, `detail_prop` | Detail objects of their own, added to the `%detailtype` ones. |
| `sky_camera` | Sky rays that reach the sky are traced again through the 3D skybox from it, like VRAD. |
| `mindxlevel`, `maxdxlevel` | With `-dxlevel`, entities outside the level are dropped. |

## FAQ

**My Hammer++ map has `_lightsrad`, does it work?** Yes, the file is read like Hammer++ intends, and the log says `Map light file (_lightsrad): ...`.

## Related pages
- [Compile options](compile-options.md)
- [Lighting](../guides/lighting.md)
- [Entity properties](../features/entity-properties.md)
- [Cubemaps](cubemaps.md)

---
[Back to the documentation index](../README.md)
