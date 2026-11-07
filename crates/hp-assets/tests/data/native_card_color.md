# Native sprite material and blending reference

These fixtures were captured from owned `render_animated_sprites` PCF effects
using the public Garry's Mod Lua rendering API. The generator, Lua probe and
authored GMA accompany the numerical reference. No installed game assets or
binaries are changed.

The JSON selects 112 distinct cases from 208 recorded renders: seven particle
colors/opacities, four RGBA8888 textures, and four material modes
(UnlitGeneric alpha, SpriteCard alpha, SpriteCard additive, SpriteCard AddSelf).
Names beginning with `spline` are shared fixture material names; these are
ordinary sprites with `$splinetype 0`, not spline trails. Each particle has
radius 24. Camera positions are measured from the particle's render bounds.

Rendering uses an RGBA16161616F target, followed by an UnlitGeneric copy into
an sRGB readback target, at 128-by-128 pixels and 60-degree field of view.
The HDR target uses point sampling (texture flag 1) so the copy does not blur
the effect's silhouette. The numeric flag is intentional: the corresponding
named constant was not exported in the tested game's Lua environment.
The JSON retains eight interior RGB samples per case. Direct 8-bit rendering
rounds low alpha blend factors differently and is not an HDR reference.

Ordinary SpriteCard vertex RGB uses gamma 2.2, unlike the linear vertex RGB
of spline cards. For the tested alpha and additive SpriteCards, combined
texture and vertex alpha at or below 0.01 is clipped by the pixel shader.
AddSelf disables that cutoff and uses
`texture_rgb * vertex_rgb * combined_alpha * (1 + addself * vertex_alpha)`
in linear space. Additive SpriteCards use the authored texture alpha.

`material_particle_tests::native_particle_colors_survive_material_loading_and_texture_preparation`
loads actual materials and textures from this GMA and the renderer's native
trail GMA, changes the base texture as the native probe does, and exercises
material analysis, texture key selection, texture decoding, descriptor
construction and GPU rendering. It compares these 112 sprites and 208 trails
against the recorded native pixels, with a 3/255 RGB tolerance. Six native
PNG references additionally check the entire 128-by-128 image, including
silhouette coverage, with the same tolerance.

The regression reproduced 46 failing material-to-pixel cases before fixing
SpriteCard alpha preservation and blend arithmetic. In particular, selecting
the legacy `|lum` texture variant altered colored additive SpriteCard alpha;
raw-texture GPU tests alone could not detect this asset-loading error.
The SpriteCard material flag carries the native shader distinction through
the existing material uniform without changing its layout. Legacy sprite
material texture handling is retained.

```text
cargo test -p hp-assets --lib --profile fast \
  native_particle_colors_survive_material_loading_and_texture_preparation
```

Regenerate with `native_card_color_fixture.rs` linked against this workspace's
`hp-pcf`, `hp-vtf` and `hp-gma`, passing an owned private mod's `data` directory.
Run `native_card_color.lua` in that private mod. Disable and restore other
probe autoruns around the run, and close only the child game process.
Select the `default`, `low`, `white`, `green`, `alpha1`, `alpha2byte` and
`alpha3` effects and row samples x=48,52,...,76 from its report.

This reference covers the described color and blending cases. It does not
establish parity for arbitrary particle operators, animation sheets, depth
blending, lighting, water interaction, or collisions.
