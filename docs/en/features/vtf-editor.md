[Documentation](../README.md) > Features > VTF editor | [Français](../../fr/features/vtf-editor.md) | [Русский](../../ru/features/vtf-editor.md)

# VTF editor

Textures of the Source engine are VTF files. Hammer Pro edits them in two ways: in an **external VTF editor** already installed on the computer (VTFEdit, VTFEdit++ or VTFEdit Reloaded), or, when there is none, in its own **built in VTF editor**, inspired by VTFEdit++.

## Opening it

| From | What it does |
|---|---|
| [Material editor](material-editor.md), **Linked textures** section and every texture parameter (`$basetexture`, `$bumpmap`...) | The **VTF** button opens that texture. It uses the external editor when one is found and the built in one otherwise. Right click the button to choose: **Open in the built in VTF editor** or **Open in** the external program. |
| [Texture browser](texture-browser.md): right click a thumbnail > **Edit texture (VTF editor)...** | Opens the base texture of the material in the built in editor, linked to that material. |
| **Tools > VTF editor...** (action `vtf_editor`, also in the command palette `Ctrl+P`) | Opens the empty window: type a texture name, open a file, or start from an image. |
| Drop a `.vtf` or an image on the window | A `.vtf` is opened; an image replaces the image shown, or starts a new texture when none is open. |
| MCP: `open_vtf_editor` | Same as above, with `external: true` for the external editor. |

## The external editor

Hammer Pro looks for VTFEdit, VTFEdit++ and VTFEdit Reloaded in the usual install folders (Program Files, the local programs folder, the user folders, the root of the drives), in the `PATH`, and takes the program registered for `.vtf` files. **Settings > VTF editor** shows what it found and lets you **Browse...** to any program, **Detect** again, or tick **Always use the built in editor**. The setting `vtf.editor_path` is changed only by you in that window.

- A texture that is a file on disk is opened where it is (a `.bak` copy is kept once).
- A texture inside a VPK or a GMA is extracted to a temporary file. When you save in the external editor, Hammer Pro copies the result into your first enabled [custom texture folder](custom-texture-folders.md). A folder without a prefix serves the texture under the same name, so it overrides the archived one; with a prefix the copy gets the prefixed name and, when the editor was opened from a material editor parameter, that parameter is pointed at the copy.
- Hammer Pro watches the file. Each time the external editor saves, the texture is reloaded: thumbnails, the 3D views and the open built in editor follow, and a notification tells you. The list **External editing** in the **Info** tab of the built in editor shows the watched files, with a **Stop** button.

## The built in editor

Window layout: a toolbar on top, the picture in the middle, a side panel with four tabs, and a status line.

### Opening and saving

| Control | What it does |
|---|---|
| **Texture** + **Load** | Loads a game texture by name (`brick/brickwall001a`), from the folders, the VPKs and the GMAs. |
| **Open file...** | Opens a `.vtf` file, or an image (PNG, TGA, JPG, BMP) as a new texture. |
| **New from image...** | Makes a new texture from an image; the **Save as a texture** form opens. |
| **Save** (`Ctrl+S`) | Writes the texture where it came from. A texture inside an archive is written as a copy in the custom folder chosen in **Save as texture...**. Saving is done in a background thread; a `.bak` copy of an overwritten file is kept once. |
| **Save as texture...** | Writes into a custom texture folder under a name (`subfolder/name`), and with **Create the material (VMT)** also writes the VMT with `$basetexture` pointing at it (never over an existing one). When the editor was opened from a material editor parameter, that parameter is set to the new texture, ready to be saved there. |
| **Save as file...** | Writes a `.vtf` anywhere. |
| **Reload** | Reads the texture again. |
| **External editor** | Opens the same texture in the external program (see above). |

### Viewing

The view toolbar has the channel toggles **R**, **G**, **B** and **A**, **Alpha only** (the alpha as grayscale), **Colour only** and **All**; the **Mip** level (with its size); the **Frame** of an animated texture with **Play**; the **Face** of a cubemap (right, left, back, front, up, down, sphere map); the **Slice** of a volume texture; **Fit**, **100%**, **200%**; and the background (checker, black, white, gray). Mouse wheel zooms around the pointer, dragging moves the picture, double click fits it, and the pixel under the pointer is described at the bottom left (position, R, G, B, A).

### Info tab

Source, **VTF version** (7.1 to 7.5), size (a warning when it is not a power of two), format (and the target format after a conversion), mip levels, frames, faces, slices, thumbnail, file size, **Start frame**, **Bumpmap scale**, **Reflectivity** (red, green and blue, or computed when saving), and the flags.

### Flags tab

A check box for every flag: point sample, trilinear, clamp S, clamp T, clamp U, anisotropic, hint DXT5, sRGB, normal map, no mipmaps, no level of detail, all mips, procedural, one bit alpha, eight bit alpha, environment map, render target, depth render target, no debug override, single copy, pre sRGB, no depth buffer, vertex texture, self shadowed bump and border. Hover a flag for what it does. **Set the alpha flags from the image** keeps the one bit and eight bit alpha flags in line with the pixels and the format when a texture is rebuilt. The environment map flag follows the cubemap box of the **Edit** tab.

### Edit tab

| Section | What it does |
|---|---|
| **Resize** | Width and height (powers of two in the lists, any value in the field), **Nearest power of two**, `/2`, `x2`, the filter (nearest, smooth for colour, smooth for data, renormalised normals) and **Apply**. **Round to a power of two when saving** (on by default) because the engine cannot use other sizes. |
| **Format** | Converts to DXT1, DXT3, DXT5, ATI1N, ATI2N, BGRA8888, RGBA8888, ABGR8888, ARGB8888, BGRX8888, BGR888, RGB888, BGR565, RGB565, BGRA4444, BGRA5551, IA88, I8, A8 or UV88, with the compression quality (fast, normal, best). |
| **Mipmaps** | Mipmaps on or off and **Regenerate**, with the filter (from the flags, colour with gamma, data, normal map). |
| **Normal map from a height map** | Height from brightness, a colour channel, alpha or the average; filter (simple, Sobel 3x3, Sobel 5x5); strength; alpha (opaque, kept, or the height); wrap at the edges (for tiling textures); OpenGL convention (leave it off for Source, which uses the DirectX one); **Mark as a normal map** sets the normal flag and clears sRGB. |
| **Alpha channel** | Fully opaque, a constant, from the brightness or one colour channel, inverted, or from a grayscale image file. |
| **Frames and faces** | **Cubemap (6 faces)** on and off, **Add a frame** (a copy of the last one), **Add a frame from an image...**, **Remove this frame**, **Replace this image...** (the frame and face shown). |

Changes that touch the pixels, the format or the mipmaps rebuild the texture when you save: the picture is compressed again from the full size image kept in memory. A change that only touches the header (flags, bumpmap scale, reflectivity, start frame) keeps the stored pixels and mip levels exactly as they were. Volume textures (slices) and the formats the editor cannot write (for example float formats) can be viewed and have their header edited, but not their pixels.

### Export tab

**Export the shown image...** writes the mip level, frame and face you are looking at as PNG, TGA, BMP or JPG, with the four channels. **Export every frame and face...** writes one file per frame and face (`name_f000_rt.png`). **Import an image...** replaces the frame and face shown (a single image texture takes the size of the new image; other textures resize the image to their size).

## MCP

Everything the window does is also available to [MCP](../../MCP.md) clients: `get_vtf_info` (details of a texture or a `.vtf` file), `preview_vtf` (a picture with the chosen mip, frame, face and channels), `export_vtf` (to PNG, TGA, BMP or JPG), `edit_vtf` (a list of operations: flags, format, resize, mipmaps, normal map, alpha, bumpmap scale, start frame, reflectivity, version, cubemap, frames; written into a custom texture folder with an optional VMT; `dry_run` to look first), `open_vtf_editor` and `list_vtf_editors`. MCP only reads and writes files in the map folder and the custom texture folders. The existing `get_material_info`, `get_material_preview` and `import_images` tools still cover materials and plain image import.

## Related pages

- [Material editor](material-editor.md)
- [Texture browser](texture-browser.md)
- [Custom texture folders and image import](custom-texture-folders.md)
- [Shader editor](shader-editor.md)
- [Settings window](settings-dialog.md)

## Creation destination and associated VMT

When creating a texture, choose a full VTF file path with Browse, or use a configured custom folder and a relative name. The VTF and its associated VMT are written in the background. An existing VMT is kept.

The associated VMT opens automatically in its own tab inside the VTF editor. It provides the material editor's shader selector, grouped parameters, texture controls and VMT text. Each material tab keeps its own unsaved changes when switching tabs or creating another texture. Ctrl+S saves the active material and keeps a backup. Return to Texture to edit the image.
