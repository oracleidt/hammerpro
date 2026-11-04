[Documentation](../README.md) > Features > Terrain | [Français](../../fr/features/terrain.md)

# Terrain: heightmaps and procedural generation

Three commands build or export whole terrains made of displacements. They are in the **Tools** menu and in the command palette (`Ctrl+P`):

| Command | Action id | What it does |
| --- | --- | --- |
| **Import heightmap...** | `import_heightmap` | Builds a terrain from a heightmap image |
| **Generate terrain...** | `generate_terrain` | Builds a terrain from fractal noise, with erosion and automatic texturing |
| **Export heightmap...** | `export_heightmap` | Writes the selected displacements to a 16 bit heightmap (and a splat map) |

They create a grid of square tiles. Each tile is a brush with a displacement on its top face. The tiles are sewn: the vertices on a shared edge are identical, so there are no cracks. Everything one command creates is **one undo step**, and the new tiles are selected afterwards, ready for the [displacement tool](displacement-tool.md) (sculpt, paint alpha, erode).

The same features exist as the MCP tools `import_heightmap`, `export_heightmap` and `generate_terrain` (see [MCP tools](#mcp-tools)).

## Concepts

- **Tile.** One displacement of power 2, 3 or 4 (4 x 4, 8 x 8 or 16 x 16 cells) on a brush of **Brush depth** units. The terrain size is cut into the whole number of equal tiles closest to **Tile size**. The **vertex spacing** is the tile size divided by the number of cells, and the window shows the tile count and the spacing live ("8 x 8 tiles (64 displacements), vertex spacing 64 units").
- **Heights.** A height field value goes from black (0) to white (1) and becomes a height between **Lowest height** and **Highest height**. **Vertical scale** is the difference of the two. **Origin Z** is the world height where a height of 0 sits.
- **Orientation.** X grows to the right of the image and Y (north) grows toward the image top. **Flip X** and **Flip Y** mirror it.
- **Limits.** At most 4096 tiles, a size from 16 to 32768 units per side, and the whole terrain inside the map limits (plus or minus 16384 units). The window shows the reason in red when a setting is out of range, and the buttons stay disabled.

## Import heightmap

Open **Tools > Import heightmap...**. The window has these sections.

### File

| Option | What it does | Default |
| --- | --- | --- |
| **Heightmap** | The image: PNG (8 or 16 bit gray; color images are read as luminance), TIFF, EXR or HDR (floating point, rescaled to 0 to 1 when the values are outside that range), BMP, TGA, JPEG, or a **RAW** file (`.raw`, `.r16`, `.bin`, `.data`). **Browse...** opens a file dialog. A relative path is relative to the map folder | empty |
| **RAW width**, **RAW height** | Pixels of a RAW file (16 bit unsigned). Both 0 means a square image, deduced from the file size | 0, 0 |
| **RAW big endian** | The 16 bit values store the most significant byte first | Off |

16 bit images keep their full precision: a 16 bit PNG gives 65536 height steps over the vertical range. An image side is limited to 16384 pixels. The window shows the image size and, if the file cannot be read, the reason.

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

**Splat map** takes either one **Splat map** image (RGB or RGBA: red, green, blue and alpha are the channels 1 to 4; an RGB image has no channel 4) or, when that field is empty, four grayscale images **Channel 1** to **Channel 4** (an empty field means that channel is 0). The weights are normalized per vertex. The splat images are mirrored by **Flip X** and **Flip Y** like the heightmap, and may have any size.

### Preview and import

The **Tile layout** drawing shows the heightmap (after invert, flips and smoothing) on the terrain footprint with the tile boundaries in yellow, and `N` for the north edge. Click **Import** to build the terrain. The **Import** button stays disabled until a file is given and every setting is valid. Errors (unreadable file, wrong RAW size) appear as a notification and change nothing.

## Generate terrain

Open **Tools > Generate terrain...**. The terrain is built from fractal noise, then optionally eroded and textured. While the window is open, a **live preview** (65 x 65 vertices at most) is drawn in the 3D view: a shaded surface with a coarse grid and a yellow outline of the footprint. The preview follows every change. Switch it off with **Live preview in the 3D view**. It is removed when the window closes. The erosion in the preview runs at most 200 passes and at a coarser resolution, so it only approximates the final result.

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

Select displacements (or solids that have some) and open **Tools > Export heightmap...**.

| Option | What it does | Default |
| --- | --- | --- |
| **Heightmap file** | Output file; the extension chooses the format: `.png` (16 bit gray), `.tif` or `.tiff` (16 bit), `.raw` (16 bit little endian) | empty |
| **Splat map file** | Optional PNG with the 4-way blend weights (the alpha when a displacement has no blend rows): red, green, blue and alpha are channels 1 to 4 | empty |
| **Lowest height**, **Highest height** | **Fixed** gives the height written as black and as white. Off: the lowest and the highest point | Off |

The image covers the footprint of the selected displacements, with its top at the north (+Y) edge, and one pixel per vertex of the finest displacement. An imported terrain is exported back with exactly one pixel per vertex. Overlapping surfaces keep the highest point, and pixels with no displacement are filled from their neighbours. The notification gives the image size and the heights written as black and white: use them as **Lowest height** and **Highest height** when you import the file again, with the same size, to get the same terrain (the MCP tool also returns the size and the corner).

## MCP tools

Full parameters are in the [MCP tools reference](../MCP.md); the window options have the same names. In short:

- `import_heightmap`: `path`, RAW options, `size_x`, `size_y`, `origin`, `centered`, `min_height`, `max_height` (or `height_scale`), `tile_power`, `tile_size`, `invert`, `flip_x`, `flip_y`, `smoothing`, `sampling`, `skirt`, `thickness`, `material`, `lightmap_scale`, `texturing` (`none`, `auto`, `splat`) with its thresholds, `splat` and `splat_channels`. `preview` true only reports the tile layout.
- `export_heightmap`: the displacements (`faces`, `ids`, `solid_ids`, or the selection), `path`, `splat_path`, `min_height`, `max_height`, `overwrite`. It returns the heights and the footprint to import it again.
- `generate_terrain`: `preset`, `seed`, the noise, size, erosion and texturing options, `replace_selected`, and `preview` (`show` or `hide`) for the 3D preview. It returns the seed.

Every call is one undo step labeled "MCP: import heightmap" or "MCP: generate terrain".

## Tips

- For a map-sized terrain, a power 3 tile of 512 units gives a vertex every 64 units. For finer detail, lower **Tile size** or raise **Tile power**.
- Source needs the terrain to be sealed from the void like any other geometry: build walls around it, or use **Border skirt** with a skybox brush ring.
- After an import, use the [displacement tool](displacement-tool.md) to sculpt, to **Paint alpha**, to sew with other terrain, or to change the power with **Keep the shape**.
- Exported heightmaps open in any image editor and in terrain tools such as World Machine or Gaea (16 bit PNG).
