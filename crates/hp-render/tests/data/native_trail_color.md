# Native CPU and SpriteCard trail color reference

`native_trail_color.json` records 208 actual Garry's Mod particle renders,
using the accompanying authored PCF generator and public Lua rendering APIs.
The four solid RGBA8888 textures and four materials are in
`native_trail_color.gma`. No installed game assets or binaries are modified.

The fixture covers CPU ribbons and SpriteCard splines, alpha/additive/AddSelf
blending, six colors/opacities including alpha bytes 1, 2 and 3, and seven tail
parameter variants. Each render uses a 24-unit-wide, 64-unit-long horizontal
trail, a 128-by-128 viewport, 60-degree field of view, and tone mapping 1.
The measured head position comes from the effect's render bounds, so frame
timing does not introduce an assumed particle position into the reference.

Render into point-sampled RGBA16161616F, then copy through UnlitGeneric to an
sRGB readback target without filtering the silhouette. This matches the
preview's linear floating-point blending. Directly
rendering into an 8-bit target changes tiny source-alpha blend factors on the
tested native backend; those captures must not be used as HDR references.
The JSON retains 12 interior samples per render. Alpha from the copy is
irrelevant; the test compares the three presented RGB channels.

Observed behavior in this game build:

- CPU ribbons apply gamma 2.2 to their byte vertex RGB. Spline cards use the
  same byte RGB as linear color.
- Alpha and additive spline cards discard texture-alpha times vertex-alpha
  at or below 0.01. AddSelf disables that cutoff. CPU translucent ribbons
  retain positive sub-byte combined alpha during floating-point blending.
- Spline additive output uses texture alpha directly; it does not divide by
  the texture's maximum color channel.
- Spline AddSelf output over black is linear texture RGB times vertex RGB
  times combined alpha times `(1 + addself * vertex_alpha)`.
- All tested tail parameter values produce the same pixels, in both paths.
  Do not invent a tail gradient from the submitted vertex data: the native
  pixel reference takes precedence. This observation is limited to the
  tested materials and game build.

The official [SpriteCard implementation](https://github.com/ValveSoftware/source-sdk-2013/blob/master/src/materialsystem/stdshaders/spritecard.cpp)
documents `$splinetype`, selects the spline vertex shader, disables animated
frame blending for splines, and configures alpha testing according to AddSelf.
Numerical color behavior above was measured in the installed game, rather
than inferred from that source file.

To regenerate, build `native_trail_color_fixture.rs` with this workspace's
`hp-pcf`, `hp-vtf` and `hp-gma` libraries and pass an owned private mod's `data`
directory. Run `native_trail_color.lua` in that mod, with other probe autoruns
disabled temporarily. It writes a report and PNGs to `data`. Restore the
private mod's previous probe files and close only the child game process.
Extract each case's 12 row samples at x=40,44,...,84; retain its measured head,
camera, source texel, particle color, blend mode and authored tail parameter.

Validation:

```text
cargo test -p hp-render --test features --profile fast \
  trail_colors_and_alpha_cutoffs_match_native_cpu_and_spline_pixels
```

The initial test reproduced a visible low-alpha spline absent in the game.
After the fix, all 7,488 compared RGB channels differ by at most 1/255 in the
recorded run (test tolerance 3/255). The scope is trail color and blending;
this does not establish parity for all PCF operators, animated sheets, depth
blending, or particle collisions.
