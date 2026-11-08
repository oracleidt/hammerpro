# Native Source mip selection

These authored VTFs use solid colors to identify every loaded mip level. The reference records actual Garry's Mod renders of a textured 3D quad at ten UV repetition rates. It includes the game's texture settings and executable hash. The uploaded user textures are excluded.

On the captured backend, the loaded chain stops when either dimension reaches 32 pixels. NOMIP keeps only the first level. ALL_MIPS does not extend the chain. The 1024 square, 64 square and 64 by 16 cases distinguish the size and aspect rules. BC and RGB fixtures distinguish upload and decoding paths.

Copy the supplied GMA into a separate test game's data folder, and run capture.lua clientside after loading a map. The Lua mounts that GMA and saves hp_render118_mips.json plus reference PNGs in the game's data folder. It uses native UnlitGeneric on a 3D mesh. It does not change installed assets or texture settings. Keep the recorded texture settings when comparing results.

The Rust regression uploads these files through hp-assets, both decoded and compressed, and compares the whole image with native RGB values. It also blends each texture with the 1024 texture at three weights, for 420 image comparisons. The raw VTF reader retains all authored mips for editing and export.
