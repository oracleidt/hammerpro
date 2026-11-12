[Documentation](../README.md) > Features > Terrain | [Français](../../fr/features/terrain.md) | [Русский](../../ru/features/terrain.md)

# Terrain: heightmaps and procedural generation

Five commands build, paint or export whole terrains made of displacements. They are in the **File** and **Tools** menus and in the command palette (`Ctrl+P`):

| Command | Action id | What it does |
| --- | --- | --- |
| **File > Import > Import heightmap...** | `import_heightmap` | Builds a terrain from a heightmap image (8, 16 or 32 bit float, tiled exports too) |
| **Tools > Displacements > Generate terrain...** | `generate_terrain` | Builds a terrain from fractal noise, with erosion and automatic texturing |
| **File > Export > Export heightmap...** | `export_heightmap` | Writes the selected displacements to a 16 bit or 32 bit float heightmap (and a splat map) |
| **File > Import > Import blend masks...** | `import_blend_masks` | Paints the blend of displacements (WorldVertexTransition alpha or 4-way blend rows) from splat or mask images |
| **File > Export > Export blend masks...** | `export_blend_masks` | Writes the blend of displacements as 16 bit or float images |

For a complete World Machine or Gaea round trip, read the [World Machine and Gaea guide](../guides/world-machine-gaea.md).

They create a grid of square tiles. Each tile is a brush with a displacement on its top face. The tiles are sewn: the vertices on a shared edge are identical, so there are no cracks. Everything one command creates is **one undo step**, and the new tiles are selected afterwards, ready for the [displacement tool](displacement-tool.md) (sculpt, paint alpha, erode).

The same features exist as the MCP tools `import_heightmap`, `export_heightmap`, `generate_terrain`, `import_blend_masks` and `export_blend_masks` (see [MCP tools](#mcp-tools)).

## Concepts

- **Tile.** One displacement of power 2, 3 or 4 (4 x 4, 8 x 8 or 16 x 16 cells) on a brush of **Brush depth** units. The terrain size is cut into the whole number of equal tiles closest to **Tile size**. The **vertex spacing** is the tile size divided by the number of cells, and the window shows the tile count and the spacing live ("8 x 8 tiles (64 displacements), vertex spacing 64 units").
- **Heights.** A height field value goes from black (0) to white (1) and becomes a height between **Lowest height** and **Highest height**. **Vertical scale** is the difference of the two. **Origin Z** is the world height where a height of 0 sits. Float heightmaps can also hold real heights (see **Pixel values**).
- **Precision.** Every image is read at its full precision and kept in double precision until the vertices are written: a 16 bit image gives 65536 steps, a 32 bit float image (R32F, EXR, float TIFF, PFM) keeps every value exactly. Nothing is reduced to 8 bit on the way.
- **Orientation.** X grows to the right of the image and Y (north) grows toward the image top. **Flip X** and **Flip Y** mirror it.
- **Limits.** At most 4096 tiles, a size from 16 to 32768 units per side, and the whole terrain inside the map limits (plus or minus 16384 units). The window shows the reason in red when a setting is out of range, and the buttons stay disabled.

## Import heightmap

Open **File > Import > Import heightmap...**. The window has these sections.

### File

| Option | What it does | Default |
| --- | --- | --- |
| **Heightmap** | The image: PNG (8 or 16 bit gray; color images are read as luminance, a gray image saved as RGB keeps its exact values), TIFF (8, 16 or 32 bit, integer or float), EXR (half or full float, a single `Y`, `R` or any one channel, or RGB), PFM (`Pf` gray or `PF` color float), HDR, BMP, TGA, JPEG, or a headerless **RAW** file (`.raw`, `.r8`, `.r16`, `.r32`, `.f32`, `.bin`, `.data`). **Browse...** opens a file dialog. A relative path is relative to the map folder | empty |
| **RAW width**, **RAW height** | Pixels of a RAW file. Both 0 means a square image, deduced from the file size; one of them 0 is computed from the other and the file size | 0, 0 |
| **RAW big endian** | The values store the most significant byte first | Off |
| **RAW format** | **Auto** (`.r8` is 8 bit, `.r16` 16 bit, `.r32` and `.f32` 32 bit float; other extensions try 16 bit, then 32 bit float, then 8 bit, whichever gives a square image), **8 bit**, **16 bit** or **32 bit float** (World Machine R32) | Auto |
| **Tiled export** | The tiles of a World Machine or Gaea tiled build instead of one file: **Add tiles...** picks several files (sorted by name), listed row by row from the north west tile, and **Tiles per row** (0: a square grid). Tiles of 2^n+1 pixels (like 513 or 1025) share their edge pixels with their neighbours, as World Machine writes them; the shared pixels are sewn (averaged) so the result has no seam | none |

An image side is limited to 16384 pixels. The window shows the image size, the format ("1025 x 1025, 32 bit float gray") and the lowest and highest value of the file, and, if the file cannot be read, the reason.

### Pixel values

How the values of the file become heights:

| Mode | What it does |
| --- | --- |
| **Auto** (default) | 8 and 16 bit images, and float images whose values are within 0 to 1, use **0 to 1**; other float images use **Stretch**. This is how earlier versions read heightmaps |
| **0 to 1** | 0 is the **Lowest height** and 1 the **Highest height**. Float values outside 0 to 1 go below or above |
| **Stretch** | The lowest value of the file becomes the **Lowest height** and the highest value the **Highest height** |
| **Absolute** | The file holds real heights: the height of a vertex is the value times **Units per value** above **Origin Z**. The lowest and highest heights come from the file (the notification shows them). Use it for R32F maps of World Machine or Gaea: with **Units per value** 1 the values are units, with 52.49 they are meters |

**One vertex per pixel** picks the **Tile size** (and **Size Y**, keeping the image proportions) so every pixel falls on a vertex and nothing is resampled. It needs image sides of a multiple of 2 to the power **Tile power** plus one, like 257, 513, 1025 or 2049 (World Machine's usual sizes). For example a 1025 x 1025 image with power 4 gives 64 x 64 tiles of 16 x 16 cells. The window tells why when the size does not fit. The tile count limit (4096) still applies: 2049 pixels need power 4 and two imports, or a lower resolution.

### Size and height

| Option | What it does | Default |
| --- | --- | --- |
| **Size X**, **Size Y** | Size of the terrain in units (16 to 32768) | 4096, 4096 |
| **Origin X**, **Y**, **Z** | Position of the terrain | 0, 0, 0 |
| **Centered on the origin** | On: the origin is the center of the terrain. Off: it is the corner with the lowest X and Y | On |
| **Lowest height** | Height of black | 0 |
| **Highest height** | Height of white | 512 |
| **Tile power** | 2, 3 or 4 | 3 |
| **Tile size** | Wanted tile size in units (16 to 16384) | 512 |
| **Border skirt** | Lowers the outermost ring of vertices by this many units, so the open edge of the terrain drops out of sight. 0 is off | 0 |
| **Brush depth** | Depth of the brush under each tile (1 to 4096) | 16 |
| **Material** | Material of the displacement faces. Use a blend material to see the textures of the texturing section | `DEV/DEV_BLENDMEASURE` |
| **Lightmap scale** | Units per luxel of the displacements | 16 |

The brush under each tile has its top face at the lowest height, and its other faces use `TOOLS/TOOLSNODRAW`.

### Image

| Option | What it does | Default |
| --- | --- | --- |
| **Invert** | White becomes low and black becomes high | Off |
| **Flip X**, **Flip Y** | Mirror the image | Off |
| **Bicubic sampling** | Samples the image smoothly between pixels (Catmull-Rom, never overshooting the neighbouring pixels). Off: bilinear sampling. When the image has exactly one pixel per vertex, both give the pixel values | On |
| **Smoothing passes** | Blurs the heights (a 3 x 3 filter per pass, 0 to 64) before the terrain is built | 0 |
| **Pixel grid** | **Corner**: the first and last pixels sit exactly on the terrain edges (pixel is point, the convention of 2^n+1 images such as 1025). **Center**: each pixel covers its own square and the outer pixels are half a pixel inside the edges (pixel is area, for 1024 or 2048 images exported that way) | Corner |

The tiles are always seamless: every vertex takes its height from its world position, so the vertices on a shared tile edge get the same height.

### Texturing

The texturing writes the **4-way blend rows** (`multiblend`) and the alpha of the displacements, so the blend material shows texture 1 on flat ground, texture 2 on rock, texture 3 on snow and texture 4 on sand. You can then paint over it with the **Paint alpha** mode.

| Mode | What it does |
| --- | --- |
| **None** | No texturing (default) |
| **Slope and height** | Automatic texturing: **rock** (channel 2) on steep slopes, **grass** (channel 1) on flat ground, **snow** (channel 3) above a height and **sand** (channel 4) below a height. Snow and sand do not stick to rock |
| **Splat map** | A second image drives the blend |

Thresholds of **Slope and height**:

| Option | What it does | Default |
| --- | --- | --- |
| **Rock from slope** | Slope in degrees from which rock appears (0 to 89) | 35 |
| **Rock blend** | Width of the transition in degrees | 10 |
| **Snow above** | Share of the height range (0 to 1) above which snow appears | 0.8 |
| **Snow blend** | Width of the transition, as a share of the range | 0.1 |
| **Sand below** | Share of the height range below which sand appears. 0 turns sand off | 0 |
| **Sand blend** | Width of the transition | 0.05 |

**Splat map** takes either one **Splat map** image (RGB or RGBA, 8 or 16 bit or float: red, green, blue and alpha are the coverage of textures 1 to 4; an RGB image has no texture 4) or, when that field is empty, four grayscale images **Channel 1** to **Channel 4** (an empty field means that texture is 0). The coverages are normalized per vertex. The splat images cover the same extent as the heightmap, are mirrored by **Flip X** and **Flip Y** like it, use the same **Pixel grid**, and may have any size. **Coverage** (default) turns the coverages into the blend rows the game reads so each texture shows its share in game; **Raw rows** writes them as they are (see [Blend masks](#import-blend-masks)). For channel choices, levels, or the WorldVertexTransition alpha, use **Import blend masks** after the import.

### Preview and import

The **Tile layout** drawing shows the heightmap (after invert, flips and smoothing) on the terrain footprint with the tile boundaries in yellow, and `N` for the north edge. Click **Import** to build the terrain. The **Import** button stays disabled until a file is given and every setting is valid. Errors (unreadable file, wrong RAW size) appear as a notification and change nothing.

## Generate terrain

Open **Tools > Displacements > Generate terrain...**. The terrain is built from fractal noise, then optionally eroded and textured. While the window is open, a **live preview** (65 x 65 vertices at most) is drawn in the 3D view: a shaded surface with a coarse grid and a yellow outline of the footprint. The preview follows every change. Switch it off with **Live preview in the 3D view**. It is removed when the window closes. The erosion in the preview runs at most 200 passes and at a coarser resolution, so it only approximates the final result.

### Presets

| Preset | Look |
| --- | --- |
| **Rolling hills** | Soft simplex hills with a slight warp |
| **Mountains** | Ridged Perlin noise with 6 octaves and a raised height curve |
| **Canyon** | Inverted ridged noise cut into 4 terraces: mesas and gorges |
| **Islands** | Simplex noise with a sea level and a round island mask |
| **Dunes** | Ridged noise stretched along Y with strong warping |
| **Plateau** | Perlin noise with a sea level and two sharp terraces |

A preset loads the noise settings only (the seed, the size and the height range are kept). **Randomize seed** picks a new seed; the same seed and settings always give the same terrain (the seed is in the notification and in the MCP result).

### Noise

| Option | What it does | Default (hills) |
| --- | --- | --- |
| Basis: **Perlin** or **Simplex** | The noise function | Simplex |
| Shape: **Plain**, **Ridged**, **Billow** | Ridged makes sharp crests, billow makes rounded bumps | Plain |
| **Features** | Number of biggest features across the longest side of the terrain (0.1 to 64). The terrain looks the same at any size | 3 |
| **Octaves** | Layers of detail (1 to 10) | 4 |
| **Persistence** | Weight of each finer layer (0.05 to 1) | 0.45 |
| **Domain warp** | Bends the noise with another noise (0 to 2). 0 is off | 0.1 |
| **Stretch** | Elongates the features along Y (above 1) or X (below 1) | 1 |
| **Contrast** | Spreads the noise over the height range (0.1 to 4) | 1.4 |
| **Height curve** | Exponent on the heights: above 1 flattens the lowlands and sharpens the peaks (0.2 to 5) | 1.2 |
| **Invert the noise** | Peaks become valleys | Off |
| **Sea level** | Share of the height range (0 to 0.95) cut flat at the bottom | 0 |
| **Terraces** | Number of flat steps, 0 is off | 0 |
| **Terrace sharpness** | 0 gives soft steps, 1 gives cliffs between steps | 0.5 |
| **Island mask** | Fades the heights to zero toward the edges | Off |
| Island: **Round** or **Square**, **Falloff width**, **Falloff power** | Shape of the mask, share of the radius used by the fade, and how long the center stays high | Round, 0.5, 1 |

### Result

- **New displacements** creates the tiles with the size, origin, height range, tile power and tile size, brush depth, material, skirt and lightmap scale settings, which are the same as in the import window.
- **Replace the N selected** keeps the selected displacements and replaces their heights with the generated terrain (the size, origin and tile settings are not used). The terrain covers the footprint of the selection, at the finest vertex spacing of the selected displacements (at most 1025 vertices per side), so tiles of several displacements stay sewn. The lowest point of the height range lies on the face plane. Offsets are reset, the blend rows are rewritten when texturing is on, and the surface is sewn afterwards, in one undo step.

### Erosion

**Thermal erosion** (material slides down the slopes steeper than the **Talus angle**) and **Hydraulic erosion** (rain digs where the water runs fast and drops sediment where it slows) use the same algorithms and the same parameters as the [displacement tool](displacement-tool.md) erosion: passes, talus angle, strength, rain, evaporation, capacity, deposition. They run on the generated height field before the tiles are built, thermal first. The hydraulic rain uses the seed.

### Texturing

**Texture by slope and height** (on by default) uses the same thresholds as the import. Turn it off to keep the blend rows of the replaced displacements.

### Apply

**Apply** builds the terrain (or replaces the heights) and shows a notification with the seed. **Close** removes the preview.

## Export heightmap

Select displacements (or solids that have some) and open **File > Export > Export heightmap...**.

| Option | What it does | Default |
| --- | --- | --- |
| **Heightmap file** | Output file; the extension chooses the format: `.png` (16 bit gray), `.tif` or `.tiff` (16 bit, or 32 bit float), `.exr` (32 bit float), `.pfm` (32 bit float), `.r32` (32 bit float little endian, World Machine R32), `.r16` or `.raw` (16 bit little endian, or 32 bit float for `.raw`) | empty |
| **Splat map file** | Optional 16 bit PNG or TIFF, or a float EXR, with the raw 4-way blend rows (the alpha when a displacement has no blend rows): red, green, blue and alpha are channels 1 to 4. **Export blend masks** writes coverage masks and the alpha instead | empty |
| **32 bit float (tif, raw)** | Writes `.tif` and `.raw` as 32 bit float | Off |
| **Values** | **Auto**: real heights in float files, the lowest to highest range in 16 bit files. **Lowest to highest**: black and white are the lowest and highest heights. **Absolute** (float files only): each pixel is the world height divided by **Units per value**, nothing is lost | Auto |
| **Lowest height**, **Highest height** | **Fixed** gives the height written as black and as white. Off: the lowest and the highest point | Off |

The image covers the footprint of the selected displacements, with its top at the north (+Y) edge, and one pixel per vertex of the finest displacement. An imported terrain is exported back with exactly one pixel per vertex. Overlapping surfaces keep the highest point, and pixels with no displacement are filled from their neighbours. The notification gives the image size and the heights written as black and white: use them as **Lowest height** and **Highest height** when you import the file again, with the same size, to get the same terrain (the MCP tool also returns the size and the corner). An absolute float export imported with **Absolute** values and **Units per value** 1 gives back the same heights to the float.

## Import blend masks

Terrain tools give blend (splat) maps as **images**, but Source blends displacement textures **per vertex**. **File > Import > Import blend masks...** converts the images into the vertex data of the selected displacements, in one undo step:

- **WorldVertexTransition** reads the vertex **alpha** of the displacement (the `alphas` rows of the VMF, 0 to 255): 0 shows `$basetexture`, 255 shows `$basetexture2`. The engine passes it to the shader as the alpha of the vertex color (`COLOR0`).
- **Lightmapped_4WayBlend** reads the **4-way blend rows** of the displacement: the VMF blocks `multiblend` (4 values per vertex), `alphablend` (4 values) and `multiblend_color_0` to `multiblend_color_3` (a color per texture), the format of Hammer++ and of the CS:GO and Portal 2 Hammer. The compiler writes them to the displacement multiblend lump (80 bytes per vertex, like Garry's Mod's VBSP) and the engine passes them to the shader as extra vertex data (the "COLOR1" painting data). The shader starts from texture 1 and blends texture 2, then 3, then 4 over it, using the second, third and fourth `multiblend` values as their blend amounts. The import writes the `multiblend` values and keeps `alphablend` and the colors (white when new).

The window:

| Option | What it does | Default |
| --- | --- | --- |
| **File 1** to **File 4** | Up to four images: PNG 8 or 16 bit, TIFF (16 bit or float), EXR, PFM, HDR, TGA, BMP, JPEG or RAW, read at full precision. **Add a file** and **Remove the last file** change the count. Each file shows its size and format | one empty file |
| **Write to** | **Auto**: faces with a Lightmapped_4WayBlend material, and faces that already have 4-way rows, get the 4-way rows, the others get the alpha. **Alpha (WorldVertexTransition)** or **4-way rows (Lightmapped_4WayBlend)** force one | Auto |
| **Alpha: Channel** | The image channel that becomes the alpha: red, green, blue, alpha or luminance of a file, **None (0)** or **Full (1)** | File 1 luminance |
| **Texture 1** to **Texture 4** | The channel that gives the coverage of each 4-way texture, also **The rest (1 minus the others)** | File 1 red, green, blue, alpha (an RGBA splat map) |
| **Invert**, **Levels** | Per channel: invert, then **Black** and **White** (the input values that become 0 and 1), **Gamma** (above 1 brightens the middle values) and **Contrast** (above 1 sharpens the transitions). **Reset** puts the levels back | off, 0, 1, 1, 1 |
| **Normalize to a sum of 1** | Scales the four coverages of each vertex so they add up to 1 (all zero gives texture 1) | On |
| **Coverage** / **Raw rows** | **Coverage**: the images say how much of each texture shows (a World Machine or Gaea splat map). They are converted to the values the shader reads: texture 4 = its coverage, texture 3 = its share of what texture 4 leaves, texture 2 = its share of what textures 3 and 4 leave. The game then shows these shares (before the luminance and blend range settings of the material). **Raw rows**: the values are written as they are, like the 4-way brush of the displacement tool | Coverage |
| **Fixed extent** | The rectangle the images cover (**Lowest X**, **Lowest Y**, **Size X**, **Size Y**). Off: the bounds of the selected displacements seen from above. **Same extent as the heightmap** copies the extent of the last heightmap import, so masks exported with the heightmap line up exactly | Off |
| **Flip X**, **Flip Y**, **Bilinear sampling**, **Pixel grid** | Like the heightmap import: the image top is the north (+Y) edge. Off for bilinear uses bicubic sampling | off, off, on, Corner |
| **Preview in the 3D views** | Applies the masks while you change the settings, so the Lit and Baked views show the blend. **Apply** keeps it, **Close** takes it back | Off |

Each vertex takes the mask value under its position seen from above. A small preview shows the result (textures 1 to 4 in green, brown, gray and white, or the alpha in gray). 4-way faces also get the alpha (1 minus the coverage of texture 1), so the terrain still looks right with a WorldVertexTransition material.

The 3D views (Lit, Baked, Player view) draw WorldVertexTransition and Lightmapped_4WayBlend materials from this data, and the compiled map carries it: the alpha in the displacement vertices, the 4-way rows in the multiblend lump. The VMF rows hold 6 decimals.

## Export blend masks

**File > Export > Export blend masks...** writes the blend of the selected displacements as images, on the pixel grid of **Export heightmap** (one pixel per vertex, north at the top), so they line up with an exported heightmap and go back into World Machine, Gaea or an image editor:

| Option | What it does |
| --- | --- |
| **4 textures (RGBA)** | Red, green, blue and alpha are textures 1 to 4: 16 bit PNG or TIFF, float EXR, or float TIFF with **32 bit float** |
| **Alpha (gray)** | The vertex alpha (WorldVertexTransition) as a gray image: png, tif, exr, pfm, r16, r32 or raw |
| **Texture 1** to **Texture 4 (gray)** | One gray image per texture |
| **Coverage** / **Raw rows** | **Coverage** writes how much of each texture shows in game, computed from the rows like the shader does (the reverse of the import), **Raw rows** the `multiblend` values |

Displacements without 4-way rows use their alpha for textures 1 and 2. Importing the exported masks back with the same extent and mode gives the same rows (within the 6 decimals of the VMF, and 1/65535 for 16 bit files).

## MCP tools

Full parameters are in the [MCP tools reference](../../MCP.md); the window options have the same names. In short:

- `import_heightmap`: `path` (or `tiles` and `tile_columns`), RAW options (`raw_width`, `raw_height`, `raw_big_endian`, `raw_format`), `values` (`auto`, `unit`, `stretch`, `absolute`) and `value_scale`, `match_pixels`, `pixel_grid`, `size_x`, `size_y`, `origin`, `centered`, `min_height`, `max_height` (or `height_scale`), `tile_power`, `tile_size`, `invert`, `flip_x`, `flip_y`, `smoothing`, `sampling`, `skirt`, `thickness`, `material`, `lightmap_scale`, `texturing` (`none`, `auto`, `splat`) with its thresholds, `splat`, `splat_channels` and `splat_weights`. `preview` true only reports the tile layout, the format and the value range. The result gives the heights used and the extent for `import_blend_masks`.
- `export_heightmap`: the displacements (`faces`, `ids`, `solid_ids`, or the selection), `path`, `splat_path`, `float`, `values` (`auto`, `normalized`, `absolute`), `value_scale`, `min_height`, `max_height`, `overwrite`. It returns the heights and the footprint to import it again.
- `import_blend_masks`: the displacements, `files` (1 to 4), `target` (`auto`, `alpha`, `four_way`), the sources `alpha` and `texture1` to `texture4` (texts like `"1.r"`, `"2.a"`, `"l"`, `"none"`, `"one"`, `"rest"`), `adjust` (per channel `invert`, `black`, `white`, `gamma`, `contrast`), `normalize`, `weights` (`coverage`, `direct`), the extent (`size_x`, `size_y`, `origin`, `centered`), `flip_x`, `flip_y`, `pixel_grid`, `sampling` and the RAW options.
- `export_blend_masks`: the displacements, `path` (RGBA), `alpha_path`, `texture_paths`, `weights`, `float`, `overwrite`.
- `generate_terrain`: `preset`, `seed`, the noise, size, erosion and texturing options, `replace_selected`, and `preview` (`show` or `hide`) for the 3D preview. It returns the seed.

Every call is one undo step labeled "MCP: import heightmap", "MCP: import blend masks" or "MCP: generate terrain".

## Tips

- For a map-sized terrain, a power 3 tile of 512 units gives a vertex every 64 units. For finer detail, lower **Tile size** or raise **Tile power**.
- A terrain alone (a new map with only the imported tiles) compiles and bakes like a leaked map, so the Baked view works at once; it stays dark until the map has a light (a `light_environment` for a sky). For the game, add an `info_player_start` and seal the map.
- Source needs the terrain to be sealed from the void like any other geometry: build walls around it, or use **Border skirt** with a skybox brush ring.
- After an import, use the [displacement tool](displacement-tool.md) to sculpt, to **Paint alpha**, to sew with other terrain, or to change the power with **Keep the shape**.
- Exported heightmaps open in any image editor and in terrain tools such as World Machine or Gaea (16 bit PNG, or EXR and R32 float for full precision).
- The [World Machine and Gaea guide](../guides/world-machine-gaea.md) walks through a whole round trip: heights, splat maps, materials and compile.
