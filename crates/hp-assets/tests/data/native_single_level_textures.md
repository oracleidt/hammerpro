# Authored single-level particle textures

The initializer-order fixture in hp-ui-lab supplies an authored 8×8 RGBA8888
VTF with one mip level. Its count case draws particles with radii 1..4, ending
in a seven-pixel-wide textured quad at 128×128. The native whole-image reference
is `native_initializer_order_count.png` in that crate's test data.

The old preview differed by up to 106/255 in RGB around the texture's borders.
Uncompressed single-level VTFs acquired a synthetic mip chain during asset
preparation, and single-level GPU uploads acquired another generated chain
when necessary. Minification sampled reduced images that did not exist in
the native texture, mixing the four quadrant colors along repeated borders.

Authored VTFs now retain the levels supplied by their file. Main viewport and
asset preview uploads preserve this chain with `create_texture_exact`.
Procedural uploads through the original texture API can still generate mip
levels, and custom image preparation continues building its own chain.

The asset regression loads the actual authored GMA, resolves its VMT, prepares
its VTF and material descriptor, then draws the native reference geometry.
It checks that the prepared upload has exactly one level and compares every
GPU RGB pixel against the game's PNG (tolerance 3/255). The PCF regression
independently simulates the complete effect and checks the same image.

The native test process used `mat_forceaniso 8`, texture filtering and trilinear
filtering enabled, `mat_picmip 0`. A separate owned capture with the VTF's
anisotropic flag enabled produced the same native image. The regression does
not establish every texture flag or filtering configuration.
