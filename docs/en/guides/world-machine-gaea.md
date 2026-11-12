[Documentation](../README.md) > Guides > World Machine and Gaea | [Français](../../fr/guides/world-machine-gaea.md) | [Русский](../../ru/guides/world-machine-gaea.md)

# World Machine and Gaea: heightmaps and blend masks

This guide takes a terrain made in World Machine or Gaea into a Source map with full height precision and its texture layers, and back. Every step also exists as an MCP tool (named in brackets).

## 1. Export from the terrain tool

- **Heights.** Export a 32 bit float heightmap: R32 (`.r32`, raw float) or EXR in World Machine, EXR or R32 in Gaea. 16 bit PNG or RAW (`.r16`) works too (65536 steps). Avoid 8 bit images: they give visible steps.
- **Size.** Prefer 2^n+1 pixels (513, 1025, 2049): the edge pixels then sit exactly on the terrain edges, and **One vertex per pixel** can use them without resampling. A 1024 or 2048 export works too: pick **Pixel grid > Center** if your tool treats pixels as areas.
- **Tiled builds.** World Machine tiles of 2^n+1 pixels share their edge pixels. Keep them as separate files: Hammer Pro stitches them.
- **Layers.** Export the texture layers as a splat map (an RGBA image, one layer per channel) or as one grayscale mask per layer, 8 or 16 bit PNG, TIFF or EXR, with the same size and extent as the heightmap.

## 2. Import the heights

Open **File > Import > Import heightmap...** (`import_heightmap`):

1. **Heightmap**: the `.r32` or `.exr` file. For a `.r32` without a size, the size is deduced from the file (square). For a tiled build, open **Tiled export** and **Add tiles...** instead.
2. **Pixel values**: **Absolute** if the file holds real heights. **Units per value**: 1 if the heights are already in units, 52.49 for meters (1 unit is about 1.905 cm). With **Stretch** the lowest and highest values become **Lowest height** and **Highest height** instead.
3. **Size X** and **Size Y**: the size of the terrain in units. **One vertex per pixel** with **Tile power** 4 turns a 1025 x 1025 image into 64 x 64 tiles, one vertex per pixel. With fewer vertices, the bicubic sampling keeps the shape smooth.
4. **Material**: a blend material: `WorldVertexTransition` for two textures, `Lightmapped_4WayBlend` for four.
5. **Import**. The notification gives the heights used. The terrain is one undo step and its tiles are selected.

## 3. Import the texture layers

With the new tiles still selected, open **File > Import > Import blend masks...** (`import_blend_masks`):

1. **File 1**: the splat map (or **Add a file** for one mask per layer).
2. **Same extent as the heightmap**: the masks then line up with the terrain exactly (MCP: give the `size_x`, `size_y`, `origin` and `centered` false returned by `import_heightmap`).
3. **Write to**: **Auto** paints Lightmapped_4WayBlend faces with the 4-way rows and the other faces with the alpha.
4. Map the channels: **Texture 1** to **Texture 4** for the four layers of a Lightmapped_4WayBlend material (for example the base layer on **The rest (1 minus the others)**), or **Alpha: Channel** for a WorldVertexTransition material (white shows `$basetexture2`). **Invert** and **Levels** adjust a channel.
5. Keep **Normalize to a sum of 1** and **Coverage** for splat maps: each texture then covers its share in game.
6. Turn on **Preview in the 3D views** to see the blend live in the Lit or Baked view, then **Apply** (one undo step).

## 4. Check and compile

- The Lit and Baked views draw the blend like the game. The [displacement tool](../features/displacement-tool.md) can paint over it (**Paint alpha**, 4-way texture buttons).
- Compile as usual: the alpha goes to the displacement vertices, the 4-way rows go to the displacement multiblend lump, which Garry's Mod, Source 2013 and CS:GO read.

## 5. Back to the terrain tool

- **File > Export > Export heightmap...** (`export_heightmap`) with an `.r32` or `.exr` file writes the real heights (values **Auto** or **Absolute**), one pixel per vertex.
- **File > Export > Export blend masks...** (`export_blend_masks`) writes the layers as a 16 bit RGBA PNG, a float EXR, the alpha, or one gray image per texture, on the same pixel grid.
- Load them in World Machine or Gaea, edit, and import again with the same settings: the round trip keeps the float heights exactly and the blend within the 6 decimals of the VMF.

## See also

- [Terrain](../features/terrain.md): every option of the import and export windows.
- [Sculpt displacements](sculpt-displacements.md): painting and sculpting by hand.
- [MCP tools reference](../../MCP.md).
