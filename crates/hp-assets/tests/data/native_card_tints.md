# Native particle material tint reference

`native_card_tints.json` contains 248 native material-modulation captures,
using the authored `native_card_color.gma` and its PCF generator. These extend
the sprite/trail reference to 568 cases and eight whole-image comparisons.

`native_card_tints.lua` captures 192 combinations: eight particle/material
setups, four textures, and six modulation variants. These exercise `$color`,
`$color2`, `$alpha`, their combination, and dark RGB tints.
`native_card_alpha_boundaries.lua` captures another 72 frames; the JSON keeps
the 56 additional cases after excluding repeated identity/half-alpha controls.
The extra cases cover zero RGB, zero alpha, alpha 0.0039/0.004, 0.01, 0.25 and
0.75. All rendering uses linear RGBA16F blending and point-sampled sRGB
readback, as documented in `native_card_color.md`.

Measured shader behavior:

- SpriteCard ignores the VMT `$color`, `$color2` and `$alpha` modulation,
  including zero color and zero alpha. Per-particle vertex RGBA and the
  overbright factor provide its modulation.
- UnlitGeneric particle materials apply the combined material RGB using
  gamma 2.2, together with the material alpha. A white particle over a white
  texture with RGB tint `[.04 .08 .12]` produces native RGB `[3,12,25]`;
  standard sRGB tint conversion produces visibly different dark colors.

The particle shader selects the native SpriteCard rule from its material
flag. The UnlitGeneric particle tint conversion is selected when constructing
sprite material descriptors, using the existing uniform layout. Other
material kinds retain their established tint behavior; their native tint
spaces require separate references.

The end-to-end material test reproduced 104 failures with the first 192 new
captures before the fix. It loads the actual VMT/VTF data, applies the same
material parameter changes, and compares final GPU pixels with native output.
Two additional native PNGs exercise dark UnlitGeneric tint and ignored
SpriteCard modulation across the entire image.

Run:

```text
cargo test -p hp-assets --lib --profile fast \
  native_particle_colors_survive_material_loading_and_texture_preparation
```

For regeneration, run both probes in an owned private mod with the authored
PCF/GMA in `data`. Restore existing probe files after each run. The first
report supplies the 192 tint cases; merge the seven additional boundary
variants per effect/texture from the second report. Record the actual
material-alpha float from the native report. The native game is authoritative
for each expected pixel; this fixture does not establish parity for unrelated
particle operators or other shader families.
