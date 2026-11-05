[Documentation](../README.md) > Features > Material editor | [Français](../../fr/features/material-editor.md)

# Material editor

The **Material editor** window edits VMT files without leaving Hammer Pro. It has two modes: **One material**, a full editor like VMT Editor (shader, parameters by page with real widgets, preview and the VMT text side by side), and **Several materials**, a batch editor that sets or removes the same parameters in a whole list of materials at once. Edits keep the comments and formatting of the files, and every file on disk gets a `.vmt.bak` backup before its first change.

## Opening it

- **Edit material** in the header of the [texture browser](texture-browser.md): opens the editor on the current material, or on the multiple selection of the browser (`Ctrl+click`, `Shift+click`).
- Right click a thumbnail > **Edit material...**: opens it on that material (or on the multiple selection when the material is part of it).
- [Texture tool](texture-tool.md): **Edit material** next to **Browse...** opens it on the material of the selected faces.
- `Escape` or the window's close button closes it.

## Loading materials

The top row fills the list of materials the window works on:

| Control | What it does |
|---|---|
| **Load:** **Current material** | The current material of the texture browser. |
| **Browser selection (n)** | The materials selected with `Ctrl+click` or `Shift+click` in the texture browser. |
| **Used by the selection** | The materials of the selected faces and objects. |
| **Add a material by name** + **Add** | Adds one material (also `Enter`). |
| **Reload** | Reads the files again. |
| **Clear** | Empties the list. |

Then choose the mode: **One material** (default) or **Several materials**.

Each material has a location:

| Location | Meaning |
|---|---|
| **File** | A VMT file on disk (hover for its path): written in place. |
| **Image** | Generated from an image of a custom folder: saving creates a real VMT file next to the image. |
| **Archive** | Inside a VPK or GMA (read only): it must be copied into a custom folder to be edited. |
| **Missing** | Not found in the loaded content. |

## One material

| Control | What it does |
|---|---|
| `◀`, material list, `▶` | With several materials loaded, picks the edited one. Switching with unsaved changes is refused: **Save or undo the changes of this material first**. |
| Location label | See above. For an archived material, **Copy to:** chooses the custom folder that receives the copy. |
| **Modified** | Shown when the text differs from the file. |
| **Save** | Writes the VMT (the original is kept as `.vmt.bak`). An archived material is written as a copy in the chosen custom folder; a missing material is created in it. |
| **Undo the changes** | Back to the text of the file. |
| **Shader** | Changes the shader (the parameters stay): LightmappedGeneric, VertexLitGeneric, UnlitGeneric, UnlitTwoTexture, WorldVertexTransition, Lightmapped_4WayBlend, Water, Refract, Sprite, SpriteCard, DecalModulate, Cloud, Sky, Patch, Wireframe. |
| **Every parameter** | Also shows the parameters other shaders read. Off by default: only the parameters of the current shader family. |
| **Search a parameter** | Filters the parameters of the page by key or description. |

The left column lists the pages: **Base texture**, **Transparency**, **Bump map**, **Detail**, **Reflections**, **Phong**, **Rim light**, **Self-illumination**, **Lighting and surface**, **Tree sway**, **Water**, **Refraction**, **Sprite**, **Compile flags**, **Other parameters**. Pages with parameters set show their count in bold; pages the shader has nothing for are hidden.

With Lightmapped_4WayBlend, **Base texture** also lists the 4-way blend parameters: `$basetexture3`, `$basetexture4`, `$textureN_lumstart` and `$textureN_lumend`, `$textureN_blendstart` and `$textureN_blendend`, `$lumblendfactorN`, `$textureN_uvscale` and `$textureN_blendmode`; **Bump map** lists `$textureN_bumpblendfactor`. The 3D views follow them (see [4-way blends](displacement-tool.md#4-way-blends-lightmapped_4wayblend)).

Each parameter row shows its key (bold when set), a widget, a `✖` button (**Remove (back to the default)**) when it is set, and a line telling what it does. Widgets follow the parameter:

| Parameter kind | Widget |
|---|---|
| On or off | A check box (off removes the key). |
| Number | A slider for small ranges, a drag field otherwise. |
| Color | A text field (`[1 1 1]`, values above 1 can be typed) and a color picker. |
| Choice | A dropdown with named values. |
| Surface property | A dropdown of the common `$surfaceprop` values. |
| Texture | A **VTF** button (opens the texture in VTFEdit, VTFEdit++, VTFEdit Reloaded or another installed VTF editor, or in the built in [VTF editor](vtf-editor.md); right click for the choice), a path field (`folder/texture`) and a `…` button (**Choose a texture file**): the file (VTF, PNG, TGA, JPG) must be inside a `materials` folder, its path is written without extension. |
| Text or vector | A text field showing the default. |

**Other parameters** lists the parameters of the file the pages do not know, each editable with a remove button, and a row to add any parameter (`$parameter`, **value**, **Add**). Blocks like `Proxies` are edited in the text.

On the right, a preview of the saved material (**Preview of the saved material**) and the **VMT text**, editable: the widgets and the text change together. When the text is not a valid VMT, the window says **The text is not a valid VMT: fix it below** and only shows the text.

## Several materials

The batch mode shows a table of the loaded materials (**Material**, **Location**, **Shader**, a button to remove a row) and the **Changes** to make in all of them:

| Parameter | Choices |
|---|---|
| **Base texture** (`$basetexture`), **Normal map** (`$bumpmap`), **Environment map** (`$envmap`), **Keywords** (`%keywords`) | **Keep**, **Set** (with a value), **Remove** |
| **Surface property** (`$surfaceprop`) | **Keep**, **Set** (text field and list of common values), **Remove** |
| **Environment map tint** (`$envmaptint`) | **Keep**, **Set** (`[1 1 1]`), **Remove** |
| **Translucent** (`$translucent`), **Alpha test** (`$alphatest`), **Two sided** (`$nocull`), **Self illumination** (`$selfillum`) | **Keep**, **On**, **Off** |

Every row starts on **Keep**. **Other parameters** and **+ Add parameter** add rows with any key.

The footer shows **Files: N, from images: N, in archives: N, missing: N**. With archived materials, **Copy archived materials into** and a custom folder copy them before editing (a copy outside the folder prefix gets a new name: **... will be copied as ...**). **Apply to N material(s)** writes all the files in the background (**Writing files...**), then reports **N material(s) updated** (errors go to the console). It is disabled until at least one row is not **Keep** (**Choose at least one change**).

## Tips

- To make a set of materials double sided, select them in the browser with `Ctrl+click`, open **Edit material**, switch to **Several materials** and set **Two sided** to **On**.
- **Used by the selection** loads every material of a room in one click.
- The `.vmt.bak` backup is made only once, before the first change: it keeps the original file.

## FAQ

**Why can't I edit a material of the game?** It lives in a VPK or GMA archive. Choose a custom folder (**Copy to:** in the **One material** mode, **Copy archived materials into** in the **Several materials** mode): saving writes the edited copy there.

**I have no custom folder.** The window says **Add a custom folder in the texture browser first**. See [Custom texture folders](custom-texture-folders.md).

**Are my comments kept?** Yes, parameters are changed inside the existing text; comments and formatting stay.

**Does the 3D view update?** Yes, the material is reloaded after saving.

## Related pages

- [Texture browser](texture-browser.md)
- [Custom texture folders](custom-texture-folders.md)
- [UV editor](uv-editor.md)
- [Texture tool](texture-tool.md)
- [MCP tool reference](../../MCP.md) (`read_material`, `edit_material`, `get_material_info`, `get_material_preview`)

---
[Back to the documentation index](../README.md)
