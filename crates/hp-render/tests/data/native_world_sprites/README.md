# Native env_sprite reference

The GMA contains an authored 64 by 32 RGBA texture and Sprite and UnlitGeneric materials. The capture scripts spawn actual env_sprite entities and render them using their native DrawModel path into a floating point target, then copy to an sRGB readback target. The PNGs preserve the complete silhouette and presented RGB values. No native sprite vertex data is injected into the editor regression.

Copy the GMA to a separate test game's data folder. Load native-sprites-server.lua serverside before loading the map, then run native-sprites.lua clientside. Use a private local game with sv_cheats 1 and r_drawworld 0 so the fixed test origin is unobstructed. The server publishes entity indices in data/hp_sprite118_entities.json; target names are not assumed to be networked to clients. The client saves hp_sprite118_reference.json and one PNG per case in data.

The capture uses a 128 square viewport, 90 degree field of view, and tone mapping scale one. The cases distinguish glow distance scaling, World Glow, No Dissipation, alpha/additive/frame blending, Alpha Add, render amount, scale, roll, sprite origin, and legacy color handling. HDR scale variants in this capture use an LDR map and therefore retain their LDR appearance. They do not establish HDR scale parity.

The regression compares every RGB pixel, allowing two byte values for native shader arithmetic. Source's sprite vertex shader uses gamma 2.2 for vertex color when sRGB is enabled, while texture reads use sRGB decoding. Source's render mode enumeration assigns Frame Blend to 7 and Alpha Add to 8.

Primary sources: [sprite vertex shader](https://github.com/ValveSoftware/source-sdk-2013/blob/master/src/materialsystem/stdshaders/sprite_vs20.fxc), [sprite drawing](https://github.com/ValveSoftware/source-sdk-2013/blob/master/src/game/client/c_sprite.cpp), [sprite material](https://github.com/ValveSoftware/source-sdk-2013/blob/master/src/materialsystem/stdshaders/sprite_dx9.cpp), [render modes](https://github.com/ValveSoftware/source-sdk-2013/blob/master/src/public/const.h).

The UnlitGeneric cases retain a centered origin, sRGB sampling and the material blend mode. Their vertex color and vertex alpha switches control entity modulation. Colored backgrounds distinguish alpha blending from additive blending.
