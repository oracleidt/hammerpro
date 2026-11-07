# Native particle texture addressing

The authored GMA contains four identical 8×8 four-quadrant textures with VTF
flags 0, CLAMPS, CLAMPT, and CLAMPS|CLAMPT. The generator and live Lua probe are
included. The probe renders all eleven screen-velocity systems under each of
these four settings (44 captures). This fixture retains the four `after` images
and their measured particle position/camera, which isolate sampler addressing.

The probe runs in an isolated Garry's Mod test mod, changes the test material's
base texture through public Lua, renders into point-sampled RGBA16161616F, and
copies through UnlitGeneric to an sRGB readback. VTF flags change the observed
border pixels: compared with repeating both axes, clamping either one changes
236 pixels and clamping both changes 440 pixels. No game assets were overwritten.

The test mounts the GMA through GameFs, analyzes each actual VMT/VTF combination,
prepares its texture and renders the material. It compares every RGB pixel with
the native capture (tolerance 3/255). Identical texture pixels share one GPU image
in this test, so changing only the material's addressing must refresh its sampler;
the sequence 0/3/1/2/0 also checks returning to repeat mode.

Coverage is limited to addressing with linear filtering at this magnification.
Point filtering, anisotropy and minified mip selection require separate native
references and are not claimed by this fixture. World/model samplers are unchanged.
