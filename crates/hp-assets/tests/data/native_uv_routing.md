# Native material UV reference

Captured in Garry's Mod `2000.00.01` using the public Lua mesh/material/render APIs.
`native_uv_routing.lua` paints controlled color/normal/lightmap render targets,
draws each material with unchanged geometry, and samples 64 interior RGB pixels.
Every case has an identity render, a render changing only the named matrix, and
a render changing only `$basetexturetransform`. The script also reads the native
2x3 matrix used by the changed transform; floating point values are decimal strings.

`native_uv_routing.json` contains those raw observations. The `tested_layer` and
`sampled_transform` annotations identify the 22 informative cases. Unit tests
require visible reference variation and independently check that the native pixel
differences support each annotated route before checking the asset descriptions.
The two saturated white four-way cases are explicitly excluded; they establish
neither secondary-normal nor secondary-color routing for that shader.

The normal-mask cases establish coordinate selection only. This fixture does not
assert full `$bumpmask` compositing or pixel-identical lighting across renderers.
The separate GPU tests compare native matrix coordinates against texture/geometry
references, including independent secondary normals and secondary base textures.

To recapture, use a private Garry's Mod mod directory with a playable test BSP:

1. Copy the Lua file to its physical `lua/autorun/client` directory.
2. Copy the supplied self-authored GMA to physical
   `data/hp_uvrouting117_assets.gma`.
3. Launch that mod and load its test map. The probe mounts its own tiny white
   cubemap, runs once after `InitPostEntity`, then writes
   `data/hp_uv_routing117_reference.json` and the 72 PNG renders.
4. Verify `errors` is empty and both environment cases report the actual
   `hp_uvrouting117/white_cube` texture with width 4. A missing/error cubemap
   must not be accepted as the reflection reference.
5. Restore/remove the temporary autorun file and close the owned test instance.

The GMA contains only `materials/hp_uvrouting117/white_cube.vtf`: a white 4x4
RGBA8888 VTF 7.2 cubemap with the seventh legacy sphere-map face required by
the native loader. No installed game material is edited or replaced.
