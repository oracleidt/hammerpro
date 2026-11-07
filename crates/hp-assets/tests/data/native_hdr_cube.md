# Native cubemap color reference

The Lua probe renders black surfaces reflecting self-authored constant cubemaps.
All raw samples are captured from Garry's Mod, with tone mapping scale 1 and
`mat_specular 1`. The fixture covers RGBA16F, RGBA16 UNORM, and RGBA8 files,
small and large tint values, generic world/model shaders, and Phong activation.

The 53 cases in `native_hdr_cube.json` include 47 supported reflection cases and
six `WorldTwoTextureBlend` cases. That shader does not load or sample the supplied
environment map; its black base/detail remain black. Its reported error texture
is not treated as a valid cubemap or used as a color reference.

The supported observations establish:

- World generic shaders and active Phong use linear environment tint.
- VertexLitGeneric and UnlitGeneric without active Phong use clamped gamma 2.2
  tint. A tint above 1 behaves as 1 in this path.
- Phong can activate with a bump map, or with `$basemapalphaphongmask 1` even
  without a bump map. Merely setting `$phong 1` without either does not activate
  the linear path in these captures.
- RGBA16F preserves radiance above white. RGBA16 UNORM supplies linear channels;
  its colors must not undergo the RGB8 sRGB decoding path.

`material_hdr_tests.rs` mounts the actual GMA through GameFs, prepares the texture,
analyzes the actual material parameters, and renders the resulting descriptors.
It compares the 47 supported cases in five paths: regular, baked HDR/LDR, and
compiled HDR/LDR. The allowed RGB difference is 3/255 to accommodate GPU rounding.
This is a bounded color reference, not a claim of universal bit-identical rendering.

For recapture in a private test mod, copy `native_hdr_cube.lua` into physical
`lua/autorun/client`, and copy the GMA to physical
`data/hp_hdrcube117_assets.gma`. Load a test map. The probe writes
`data/hp_hdr_cube117_reference.json` and its PNGs. Verify empty `errors`, real
texture names/width 4, and mounted assets for every supported case. Restore the
temporary autorun file afterward. The GMA contains only generated constant VTFs.
