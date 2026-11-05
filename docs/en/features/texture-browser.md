[Documentation](../README.md) > Features > Texture browser | [Français](../../fr/features/texture-browser.md)

# Texture browser

The texture browser is the **Textures** tab of the right dock. It lists every material of the mounted content (game VPKs, addons, Workshop, custom texture folders) in a thumbnail grid with an instant search, filters, favorites and the current material: the one the texture tool, the block tool and **Apply current texture** put on faces.

## Opening it

- Click the **Textures** tab of the right dock.
- **View > Panels > Texture browser** shows the tab and puts the cursor in the search box. There is no default shortcut.
- In the texture tool, **Browse...** of the **Face edit** panel opens the browser: the next material you pick is applied to the selected faces, then the panel comes back (see [Texture tool](texture-tool.md)).
- When a material key of the entity panel asks for a material, a banner at the top of the browser says **Pick a material for ...**: double click a material (or press `Enter` in the search) to give it to the key, **Use the current material** gives the current one, **Cancel** goes back to the entity panel.

## The current material

The top of the tab shows the current material: a large thumbnail, its name, a star, its folder, its size and shader, and the content it comes from. **Missing from the game content** in red means no mounted file has that name. **Resized in game** means the material is a plain image whose sides are not powers of two: the game gets it resized (hover the label for the sizes), and texture scales use the game size.

| Control | What it does |
|---|---|
| Thumbnail | Drag it onto a face to apply the material. Double click: apply it to the selection. |
| Star (**Add to favorites** / **Remove from favorites**) | Adds or removes the material from the favorites. |
| **Apply to selection** | Applies the material to the selected faces, or to every face of the selected objects. Disabled when nothing is selected. |
| **Replace...** | Opens [Find and replace textures](find-replace-textures.md) with this material in the **Find** field. |
| **Edit material** | Opens the [material editor](material-editor.md) on this material, or on the multiple selection when you made one (`Ctrl+click`). |
| **Mark** | Selects every face that uses this material and switches to the texture tool. |

The current material changes when you click a thumbnail, pick a face material with `Alt+click` in the texture tool, or apply a material by drag and drop. Each material you use goes first in the **Recent** list (64 materials at most).

## Search

Type in **Search materials...**: the list is filtered on every key stroke, even with 40,000 materials. Every word must match, case and accents are ignored. Press `Enter` in the search box to make the first result the current material.

| Word | Keeps the materials... |
|---|---|
| `brick wall` | whose name contains `brick` and `wall`. |
| `-dev` | whose name does not contain `dev`. |
| `kw:urban` | whose VMT keywords (`%keywords`) contain `urban`. |
| `src:workshop` | whose source label contains the word (an addon name, `workshop`, a custom folder name...). |
| `is:translucent` | with `$translucent` or `$additive`. |
| `is:alpha` | whose texture alpha is see-through in game (blended or alpha tested, as the engine decides). |
| `is:selfillum` | with `$selfillum`. |
| `is:envmap` | with `$envmap` (reflections). |
| `is:blend` | that are blend materials (WorldVertexTransition). |
| `is:missing` | whose VMT or base texture cannot be found. |
| `-is:flag` | without that flag (for example `is:translucent -is:envmap`). |
| `shader:lightmapped` | whose shader contains the word. |
| `-shader:vertexlit` | whose shader does not contain the word. |

A word pasted as a file path is reduced to the material name: `c:/game/materials/brick/brickwall001a.vmt` searches `brick/brickwall001a`. The `kw:`, `is:` and `shader:` words need the keywords and properties of every material: they are read in the background the first time such a word is used (**Indexing keywords...**, **Reading material properties...** next to the count), then the search is instant.

The `X` button at the end of the search box clears it.

## Filters

| Filter | What it does | Default |
|---|---|---|
| Source chips **All**, **Game**, **Addons**, **Workshop** and one chip per custom folder | Shows only the materials of that source. A chip appears only when the content has that kind of source. Hover a folder chip for its path and material count. | **All** |
| Category chips **All**, **Used in map**, **Favorites (n)**, **Recent**, **Tools** | **Used in map**: the materials of the open map (including missing ones). **Favorites**: your starred materials. **Recent**: the last used, most recent first. **Tools**: tool textures only. | **All** |
| Property chips **Translucent**, **Alpha**, **Self-illum**, **Reflective**, **Blend**, **Missing** | Each chip adds or removes the matching `is:` word in the search box. | none |
| **Shader:** dropdown | **All**, or one shader with its material count (most used first). Writes `shader:name` in the search. | **All** |
| **Hide VertexLitGeneric** | Hides the model materials (VertexLitGeneric shader): they do not go on brushes. Tool textures stay. Saved in `hidden_shaders` of `[browser]`. | off |
| **Hide tool textures** | Hides the tool textures (except in the **Tools** category). Saved as `hide_tool_textures`. | off |
| Thumbnail size slider | Thumbnail size, 48 to 256 points by steps of 8. Saved as `thumbnail_size` in `[ui]`. | 96 |
| **Show alpha** | Draws the materials whose alpha is see-through in game over a checkerboard. Saved as `show_alpha`. | off |

The line below the filters shows **N of M materials**, and **N selected** with an `X` to clear the multiple selection.

### Hidden folders

Some material folders are hidden from the browser (tool textures and the materials of the map always stay visible):

- Right click a material and choose **Hide the folder ...** to hide its folder. The list is `excluded_folders` of `[browser]`; edit it in **Settings > Entities and browsers > Texture browser > Hidden folders:** (type `folder` or `folder/sub`, then **Add**; `×` removes one).
- **Hide the folders the game data excludes** (same settings page, default on, `fgd_exclusions`) also hides the folders listed by `@MaterialExclusion` in the FGD files.

## The grid

| Action | Result |
|---|---|
| Click | Makes the material current. |
| `Ctrl+click` | Adds or removes the material from the multiple selection (used by **Edit material**). |
| `Shift+click` | Selects the range from the last clicked material. |
| Double click | Applies the material to the selection (or gives it to the entity key waiting for a material). |
| Drag onto a face of the 3D view | Applies it to that face (the face is outlined while you hover it). |
| Drag into a 2D view | Applies it to every face of the selected objects (or to the selected faces). |
| Hover | Tooltip: full name, size, shader, surface property, keywords, flags (tool texture, translucent, alpha test, water, blend) and source. |
| Right click | Context menu (see below). |

When the mouse is over the grid (and no text field has the focus), the keyboard moves in it: arrows, `Page Up`, `Page Down`, `Home`, `End` change the current material, with `Shift` they extend the multiple selection, and `Enter` applies the current material to the selection. When the mouse leaves the grid, those keys go back to the tools (nudging, for example).

Thumbnails with a red name are missing materials (purple and black checkerboard).

### Context menu

| Item | What it does |
|---|---|
| **Apply to selection** | Applies the material to the selection. |
| **Select faces using it** | Selects every face using the material and switches to the texture tool. |
| **Add to favorites** / **Remove from favorites** | Toggles the star. Favorites are saved in `favorites` of `[browser]`. |
| **Copy name** | Copies the material name to the clipboard. |
| **Replace with this...** | Opens [Find and replace textures](find-replace-textures.md) with the current material in **Find** and this one in **Replace with**. |
| **Open containing folder** | Opens the folder of the VMT, VTF or image in the file explorer (only for files on disk, not inside VPK or GMA archives). |
| **Edit material...** | Opens the [material editor](material-editor.md) on it (or on the multiple selection when it is part of it). |
| **Edit texture (VTF editor)...** | Opens the base texture of the material in the built in [VTF editor](vtf-editor.md). |
| **Edit in the shader editor** | Opens the [shader editor](shader-editor.md) on the material: on its saved graph when it has one next to its files, else on its base texture. |
| **Hide the folder ...** | Hides the folder of the material from the browser. |

## Custom folders

The collapsible **Custom folders (n)** section lists your custom texture folders with their switch, material count, prefix and warnings, and has the **Add folder...**, **Rescan** and **Import images...** buttons. See [Custom texture folders](custom-texture-folders.md). It is closed by default, and opens again only when you click it.

## Tips

- Type a few letters of each part of the name: `conc flo` finds `concrete/concretefloor001a`.
- `is:alpha -is:translucent` finds the cut out materials (fences, foliage).
- Star the materials you use all the time and keep **Favorites** open while you texture.
- Use **Mark** on a material to see where the map uses it, then change them all in the **Face edit** panel.

## FAQ

**A texture I added does not show up.** Click **Rescan** in **Custom folders**: it reloads the game content and the custom folders. Also check that its folder is not hidden and that **Hide tool textures** or a chip is not filtering it.

**Why are some materials missing from the list?** Check the source and category chips, the property chips (they write `is:` words), the **Shader:** dropdown, **Hide VertexLitGeneric** and the hidden folders.

**What does Workshop loading mean?** While the Workshop addons are read, the list shows **Loading the workshop (N addons), the list grows when it is done...**: their materials appear when the reading ends.

**Can I see only what every Garry's Mod player has?** The texture browser has no such switch; use the **Game** source chip. The **Garry's Mod content only** option belongs to the model, sound and particle browsers.

## Related pages

- [Texture tool](texture-tool.md)
- [Find and replace textures](find-replace-textures.md)
- [Custom texture folders](custom-texture-folders.md)
- [Material editor](material-editor.md)
- [UV editor](uv-editor.md)
- [Game setup](../getting-started/game-setup.md)
- [Settings reference](../reference/settings.md)
- [MCP tool reference](../../MCP.md) (`search_materials`, `set_current_material`, `apply_material`, `get_material_info`, `get_material_preview`, `list_used_materials`, `find_material_usage`)

---
[Back to the documentation index](../README.md)
