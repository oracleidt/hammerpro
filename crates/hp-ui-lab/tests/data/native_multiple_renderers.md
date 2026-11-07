# Native multiple-renderer reference

The PCF/GMA, generator, Lua probe, JSON measurements and six PNGs are authored
fixtures from Garry's Mod 2000.00.01. Each system emits one stationary particle
at (10,0,64), with radius 24 and vertex RGBA (255,255,255,128). The SpriteCard
material uses the included four-quadrant 8×8 RGBA8888 texture, no depth blending
and no fog. The camera is at (10,-128,64), yaw 90, FOV 60, rendering 128×128.

The six cases contain these animated-sprite renderer orientation lists:

| Case | Orientations | Native result |
|---|---|---|
| single | 0 | 1681 colored pixels; interior red 188 |
| double | 0, 0 | Same silhouette; interior red 225 |
| triple | 0, 0, 0 | Same silhouette; interior red 241 |
| flat | 2 | Edge-on quad: zero colored pixels |
| sprite_flat | 0, 2 | Same image as single |
| flat_sprite | 2, 0 | Same image as single |

Capture uses a point-sampled RGBA16161616F target, tone mapping 1 and an
UnlitGeneric copy into sRGB readback, matching the earlier particle fixtures.
The reference does not infer opacity from a UI thumbnail or from an 8-bit blend.

The regression goes through PCF parsing, Effect simulation, the actual UI
`append_sprites` adapter, `instances_of`, and GPU drawing. It compares every
RGB pixel to the six native images with a tolerance of 3/255. The material look
is read from the authored GMA through GameFs; the GPU texture is the identical
four-quadrant pixel array. MockAssets provides only the material ID, so this
fixture does not replace the separate asset-loading/texture-addressing tests.

Before the correction, double/triple submitted one instance and differed by
37/53 RGB levels; flat_sprite was invisible and differed by 188. The first
renderer must not suppress subsequent renderers, nor supply their orientation.

All cases use one particle and one material. They do not establish the ordering
of overlapping multi-particle collections, all renderer combinations, model
renderers, or all PCF behavior. Those need their own native references.

To regenerate, compile the included fixture generator against hp-pcf, hp-gma
and hp-vtf. Run its output and the Lua probe in an isolated test mod, preserving
existing autorun files. Copy hp_multirender117_reference.json and the six
hp_multirender117_*.png files from that mod's data directory. No installed game
assets or binaries need to be changed.
