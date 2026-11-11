[Documentation](../README.md) > Features > Shader editor | [Français](../../fr/features/shader-editor.md) | [Русский](../../ru/features/shader-editor.md)

# Shader editor

The **Shader editor** is a node based editor for images and materials, in the spirit of the shader graph of s&box. You connect nodes with wires: an image, some noise, a blur, a color ramp, a normal map made from a height map... and the result appears live in a preview. When it looks right, **Write the textures** writes it to a custom texture folder as VTF files and the VMT of the material, through the same conversion as [image import](custom-texture-folders.md).

Everything is computed on the CPU, at a small preview size while you work and at the full size when you write the files, so it needs no special graphics card feature.

## Opening it

- **Tools > Content and editors > Shader editor...** (action `shader_editor`, also in the command palette `Ctrl+P`): opens the window on the graph you were editing, or an empty one.
- **Import images** window > **Shader editor...**: opens it on the first image of the list. If a graph named like the image (`brick.hpshader` next to `brick.png`) exists, that graph is opened instead.
- [Texture browser](texture-browser.md): right click a thumbnail > **Edit in the shader editor**. A material written by the editor with **Save the graph next to the textures** opens its own graph again; any other material opens as a Texture node on its base texture.
- MCP: `open_shader_editor`.

The window can be resized and moved like any dialog. Closing it keeps your graph in memory until you close Hammer Pro: open it again from the menu.

## The window

| Area | What it holds |
|---|---|
| Toolbar | **New** (presets), **Open...**, **Save**, **Save as...**, **Undo**, **Redo**, **Add node**, **Fit**, **Reload images**, the zoom, and the **Node previews** switch. |
| Canvas (left) | The graph: nodes and wires. |
| Side panel (right) | **Preview**, **Node** (the parameters of the selected node), **Write to the game**, and **Problems** when there are some. |

### Working in the canvas

| Gesture | What it does |
|---|---|
| Mouse wheel | Zooms around the pointer (25% to 250%). |
| Middle or right button drag on the background | Pans. |
| Right click on the background, double click on it, or `Space` | Opens the node library at the pointer. |
| Left drag on the background | Box selection (hold `Shift` or `Ctrl` to add). |
| Click a node | Selects it (`Shift` or `Ctrl` toggles). Its parameters appear in the **Node** section. |
| Drag a node | Moves it (with the other selected nodes). Positions snap to a 8 unit grid when you let go. |
| Drag from a pin to another pin | Makes a wire. An output can feed many inputs; an input takes one wire. Loops are refused. |
| Drag from a connected input pin | Picks the wire up: drop it on another input to move it, or on the background to delete it. |
| Drag a wire from an output and drop it on the background | Opens the node library; the node you choose is connected automatically. |
| Click a wire, then `Delete` | Deletes the wire. |
| `Delete` or `Backspace` | Deletes the selected nodes. |
| `Ctrl+A` | Selects all nodes. |
| `Ctrl+C`, `Ctrl+V`, `Ctrl+D` | Copies, pastes and duplicates the selected nodes with the wires between them. |
| `F` | Frames the whole graph (same as **Fit**). |
| `Ctrl+Z`, `Ctrl+Y` (or `Ctrl+Shift+Z`) | Undo and redo (100 steps; a drag or a slider move is one step). |
| Double click a node, or right click > **Preview this node** | Shows that node in the preview instead of the output. |
| Right click a node | **Preview this node**, **Duplicate**, **Disconnect**, **Delete**. |

A colored dot in the header of a node means a problem (red: error, orange: warning); hover or read the **Problems** list. A white ring means the node is the one shown in the preview. Pin colors: orange for colors, green for numbers, blue for coordinates. Any pin can be connected to any other: a number becomes a gray color, a color read as a number gives its brightness.

### The node library

Opened with `Space`, a right click or a double click on the background. Type to search by name, id or category (`Up`, `Down`, `Enter` to choose, `Escape` to close). Nodes are grouped by category.

### Preview

The **Preview** section shows the output (the first **Output** node) or the node you chose. Beside it: the channel shown (**Color and alpha** over a checkerboard, **Color only**, **Alpha**, **Red**, **Green**, **Blue**), a tiling of 1x1, 2x2 or 3x3 to check that a texture repeats well, and the preview resolution (128, 256 or 512 pixels on the long edge). The line under the image gives the size and the time of the last computation.

The preview is computed on a background thread. While you drag, it drops to 128 pixels; it never blocks the editor, and a computation that became out of date is stopped. **Node previews** puts a small thumbnail in each node (the output node shows what is plugged into it); turn it off for very big graphs.

## Nodes

All nodes work on four channels per pixel (red, green, blue, alpha, nominally 0 to 1). Math nodes work on the color channels and keep the alpha of their first input. Unconnected inputs use a default value that you can change in the **Node** section (the **Reset** button puts the default back). Unconnected coordinate inputs use the UV of the image. Names of inputs and parameters are as in the interface.

### Input

| Node | Inputs | Parameters | What it does |
|---|---|---|---|
| Texture | UV | Image, Wrap (Repeat, Clamp, Mirror), Filter (Linear, Nearest) | Reads an image: a file path (absolute, or relative to the graph file or the map), or the name of a game texture (for example `brick/brickwall001a`). Outputs RGBA and R, G, B, A. Large images are sampled with mipmaps, so tiling stays clean. |
| Color | | Color | A constant color with alpha. |
| Number | | Value | A constant number. |
| Vector 2, Vector 3 | | Value or X, Y, Z | Constant vectors. |
| UV | | | The coordinates of the image, 0 to 1 across and down. |

### Coordinates

| Node | Inputs | Parameters | What it does |
|---|---|---|---|
| Tiling and offset | UV | Tiling, Offset | Repeats and moves the coordinates. |
| Rotate UV | UV | Angle, Pivot | Turns the coordinates. |
| Warp UV | UV, Map | Strength | Shifts the coordinates by the red and green of a map (0.5 is no shift): distortion. |

### Math

| Node | Inputs | Parameters | What it does |
|---|---|---|---|
| Add, Subtract, Multiply, Divide | A, B | | The four operations (dividing by 0 gives 0). |
| Minimum, Maximum | A, B | | The smaller or larger. |
| Lerp | A, B, T | | Mix: A when T is 0, B when T is 1. |
| Clamp | Value, Min, Max | | Keeps a value in a range. |
| Saturate | Color | | Keeps a value between 0 and 1. |
| Power | Value, Exponent | | Raises to a power. |
| Absolute, One minus | Color | | The absolute value; 1 minus the value. |
| Remap | Color | Input min and max, Output min and max | Moves a range to another range. |
| Smoothstep | Color | Edge 0, Edge 1 | A smooth step between two edges. |

### Color

| Node | Inputs | Parameters | What it does |
|---|---|---|---|
| Invert | Color | Invert alpha | Inverts the colors (alpha kept unless asked). |
| Brightness and contrast | Color | Brightness, Contrast | |
| Hue, saturation, value | Color | Hue (degrees), Saturation, Value | |
| Tint | Color, Tint | Amount | Multiplies by a tint color. |
| Grayscale | Color | Mode (Luminance, Average, Lightness), Amount | |
| Posterize | Color | Levels | Fewer tones. |
| Levels | Color | Black point, White point, Gamma | |
| Threshold | Value | Threshold, Softness | Black and white from the brightness. |
| Color ramp | Value | Ramp, Mode (Linear, Steps) | Gives a color to each brightness. The ramp has 2 to 16 stops with a position and a color each (**Add a stop**, the cross removes one). |
| Blend | Base, Layer, Opacity | Mode | Normal, Multiply, Screen, Overlay, Soft light, Hard light, Add, Subtract, Difference, Darken, Lighten, Color dodge, Color burn, Exclusion. The alpha of the layer also counts. |

### Filters

Radii are given for a 1024 pixel image and scale with the size, so the preview matches the written texture.

| Node | Inputs | Parameters | What it does |
|---|---|---|---|
| Blur | Color | Radius | Gaussian like blur that wraps around the edges, so it keeps tiling. |
| Sharpen | Color | Amount, Radius | |
| Edge detect | Color | Strength, Radius | Edges of the brightness. |
| Height to normal | Value | Strength, Format (DirectX (Source), OpenGL) | A normal map from a height map. Source uses the DirectX convention (green pointing down), which is the default. |
| Normal strength | Normal | Strength | Stronger or weaker relief. |
| Pixelate | Color | Cells X, Cells Y | Big square pixels. |
| Make seamless | Color | Width | Blends the image with a half shifted copy so it tiles without a seam. |

### Procedural

| Node | Inputs | Parameters | What it does |
|---|---|---|---|
| Noise | UV | Type (Perlin, Simplex, Voronoi, White noise), Scale, Octaves, Lacunarity, Gain, Seed, Voronoi output (Distance, Edges, Cells), Jitter | Perlin, Voronoi and white noise tile exactly; simplex is made to tile too. Scale is the number of cells across the image. |
| Checker | UV | Tiles X, Tiles Y, Color A, Color B | A checkerboard; outputs the color and a 0/1 mask. |
| Gradient | UV | Type (Linear, Radial, Angular, Diamond), Angle | A 0 to 1 gradient. |

### Channels

| Node | Inputs | What it does |
|---|---|---|
| Split channels | Color | Outputs R, G, B, A as numbers. |
| Merge channels | R, G, B, A | Builds a color (for example to pack three masks into one texture). |
| Set alpha | Color, Alpha | Replaces the alpha. |

### Output

Add one **Output** node per file to write. Inputs: **Color** (red, green, blue) and **Alpha** (if nothing is plugged into Alpha, the alpha of the color is used). Parameters:

| Parameter | What it does | Default |
|---|---|---|
| **Writes** | What the file is: **Base texture** (`$basetexture`), **Normal map** (`$bumpmap`, written with the normal map flag and a normal aware mip filter), **Reflection mask** (`$envmapmask`, and `$envmap env_cubemap` is added), **Self illumination mask** (`$selfillum 1` and `$selfillummask`), **Other file** (only the VTF). | Base texture |
| **File suffix** | Appended to the name. Empty uses the default of the type: nothing, `_normal`, `_envmask`, `_illum`, `_custom`. | empty |
| **Format** | **Automatic** (DXT1 when opaque, DXT5 otherwise; normal maps are written uncompressed), **DXT1**, **DXT5**, **BGRA8888**, **BGR888**. | Automatic |
| **Mipmaps** | Writes mipmaps. | on |
| **Clamp edges** | Sets the clamp S and T flags (for sprites and decals). | off |

Two outputs cannot write the same file: that is an error in **Problems**.

## Writing to the game

The **Write to the game** section:

| Option | What it does | Default |
|---|---|---|
| **Destination** | The custom texture folder that receives the files (see [Custom texture folders](custom-texture-folders.md)). | first folder |
| **Name** | Name of the texture: letters, digits, `_` and `-` (others become `_`). | name of the image |
| **Size** | **Automatic** (the size of the first image of the graph, 512 if there is none), or 64 to 2048 pixels on the long edge. VTF sides are rounded to powers of two. | Automatic |
| **Surface property** | `$surfaceprop` of the VMT. | **None** |
| **Overwrite existing files** | Replaces files that exist (else the write is refused). | off |
| **Save the graph next to the textures** | Also writes `name.hpshader` in the same folder, so that opening the material from the texture browser gives the graph back. | on |
| **Write the textures** | Computes at the full size on a background thread and writes. | |

The list under the options shows the VTF files that will be written. The VMT is made for the **Base texture** output: shader `LightmappedGeneric`, `$basetexture`, and `$alphatest 1` when its alpha is only fully opaque or transparent, or `$translucent 1` when it has partial transparency, like [image import](custom-texture-folders.md); plus `$bumpmap`, `$envmapmask` and `$selfillummask` when such outputs exist. Without a base output only the VTF files are written. When the write succeeds, the material becomes the current material of the texture browser.

Graphs next to the textures are small JSON files. They are not packed into the BSP, but a tool that copies a whole custom folder into a Workshop addon may refuse files it does not know; turn the option off, or move the `.hpshader` files elsewhere, for such folders.

## Saving and loading graphs

Graphs are `.hpshader` files, plain JSON (format version 1): nodes with their parameters and positions, and the wires. **Save as...** picks the place (the project folder, next to the texture, anywhere you like); **Save** saves again. Paths of images inside the folder of the graph are saved relative to it, so a graph and its images can be moved together. **Open...** loads a graph; a damaged file is repaired as far as possible (unknown nodes, broken wires and loops are dropped and listed in the console) and a file that is not a graph is refused.

The window title shows the file and `*` when there are unsaved changes.

## Presets

**New** lists starting graphs (they use the image of your current graph when it has one):

| Preset | What it makes |
|---|---|
| Image to base texture | One Texture node into one base texture output. |
| Tint | Texture, Tint, output. |
| Normal map from height | A base texture, and a normal map computed from the blurred brightness. |
| Tile blend | The image mixed with a rotated, shifted and recolored copy of itself through a noise mask: breaks repetition. |
| Seamless tile | Texture, Make seamless, output. |
| Color grade | Hue and saturation, brightness and contrast, levels. |
| Grunge | Noise and Voronoi noise through a color ramp, overlaid on the image. |
| Material set | A base texture, a normal map and a reflection mask from one image. |
| Empty graph | Nothing. |

Choosing a preset replaces the graph; **Undo** brings the old one back.

## Limits and robustness

- At most 512 nodes and 2048 wires per graph; images are written at 2048 pixels at most on the long edge.
- A missing or unreadable image shows a warning on its node and counts as gray; a missing wire uses the default of the input; a loop is cut at the wire that closes it and reported. The graph never stops evaluating.
- Colors are mixed as stored in the images (no gamma conversion), like most image editors.

## Tips

- Plug the **Height to normal** node into a slightly blurred, grayscale copy of the texture; raise **Strength** until the relief is visible in the 3x3 tiling.
- Use **Preview this node** on a **Noise** node to tune it before it is mixed in.
- Keep the long edge at 256 while you work and check the final look with the 2x2 tiling.
- Use **Merge channels** to put a roughness like mask in the alpha of the base texture: the DXT5 format is picked automatically.

## For AI assistants

The editor is fully reachable through [MCP](../../MCP.md): `list_shader_nodes`, `list_shader_presets`, `create_shader_graph`, `read_shader_graph`, `edit_shader_graph`, `validate_shader_graph`, `render_shader_graph` (returns the image), `bake_shader_graph` and `open_shader_editor`. `live: true` works on the graph open in the window. MCP only reads images and graphs from the map folder and the custom texture folders.

## FAQ

**The preview is gray.** The Texture node has no image or it cannot be loaded: read the orange dot and the **Problems** list. Relative paths are looked up next to the graph file and next to the map.

**Why is the normal map flat?** Raise **Strength**, or check that the height input is not a constant.

**Does it replace VTFEdit or Photoshop?** No: it is for procedural variations and quick texture work; it writes ordinary VTF and VMT files.
