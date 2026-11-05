[Documentation](../README.md) > Features > Find and replace textures | [Français](../../fr/features/find-replace-textures.md) | [Русский](../../ru/features/find-replace-textures.md)

# Find and replace textures

The **Find and replace textures** window swaps a material for another on the faces of the whole map or of the selection, by exact name or with wildcards, with a live count of the faces it will change. It can also keep the texture size in the world when the new texture has another resolution.

## Opening it

| How | Result |
|---|---|
| **Tools > Find and replace textures...** (`Ctrl+Shift+R`) | Opens the window. When **Find** is empty, it is filled with the current material. |
| **Replace...** in the [texture browser](texture-browser.md) header | Opens it with the current material in **Find**. |
| Right click a thumbnail > **Replace with this...** | Opens it with the current material in **Find** and the clicked one in **Replace with**. |

With the Hammer and Hammer++ keymap presets, `Ctrl+Shift+R` is **Find and replace in entities...** and this window has no shortcut (see [Keyboard shortcuts](keyboard-shortcuts.md)).

## The window

| Control | What it does | Default |
|---|---|---|
| **Find** | Material name or pattern. Names are compared without case; slashes, a `materials/` prefix and a `.vmt` extension are cleaned up. | current material |
| Image button next to **Find** | Click: opens or closes the material picker (see below). Right click: puts the current material in the field. | |
| Cursor button (**Use the material of the selection**) | Puts the first material of the selection in **Find**. Disabled with no selection. | |
| **Replace with** | Replacement material. | empty |
| Image button next to **Replace with** | Same picker, for the replacement. | |
| **In**: **Whole map** / **Selection** | Every face of the map, or the faces of the selected objects plus the selected faces. | **Whole map** |
| **Match**: **Exact name** / **Wildcards** | Exact: the whole name. Wildcards: `*` matches any text, `?` one character. | **Exact name** |
| **Keep the texture size in the world** | Adjusts the texture scale of each face when the new texture has another resolution, so the texture keeps its size on the walls. | off |
| **Replace** | Runs the replacement (also `Enter` in a field). Disabled until a face matches and **Replace with** is filled. | |
| **Close** | Closes the window (also `Escape`). | |

`Tab` goes from **Find** to **Replace with**.

Below the options, the window shows **N matching face(s) (M material(s))**, in green when something matches. With several matching materials, or in wildcard mode, a list shows each material with its number of faces and, when **Replace with** is filled, the material it will become. Typing `*` or `?` in exact mode shows **The name contains * or ?: choose the wildcard mode** and blocks the replacement.

The whole replacement (and the rescale) is one undo step. After it, the window says **N face(s) replaced**. Faces that could not be rescaled because a texture size is unknown are listed in the console.

## Wildcards

In wildcard mode, a `*` in **Replace with** reuses the text the `*` of **Find** matched:

| Find | Replace with | Result |
|---|---|---|
| `dev/*` | `tools/*` | `dev/dev_measurewall01a` becomes `tools/dev_measurewall01a` |
| `concrete/concretefloor00?a` | `concrete/concretefloor001a` | every numbered variant becomes the same floor |
| `*` | `dev/dev_measuregeneric01b` | every face of the scope gets the same material |

## The material picker

The image buttons open a compact picker below the field:

- A **Current material** row at the top: click it to use the material chosen in the texture browser.
- A search box with the texture browser rules (words, `-word`, `src:`, `kw:`, `is:`, `shader:`, see [Texture browser](texture-browser.md)).
- Section chips **Used in map (n)**, **Recent (n)**, **Favorites (n)**, **All (n)**. **Find** opens on **Used in map**, **Replace with** on **Recent** (on **All** when you have no recent material); afterwards each picker keeps the section you chose last. When a section has no match, **Search all materials** switches to **All**.
- A grid of thumbnails, five per row: click one to fill the field.

Keyboard: the search box has the focus; `Down` enters the grid, the arrows move in it, `Enter` chooses (the first result when nothing is highlighted), `Escape` closes the picker only (a second `Escape` closes the window). A click outside the picker, or on its button again, closes it.

## Tips

- To retexture a whole area, select it, choose **Selection**, and replace `*` with your material.
- When you swap a 512 texture for a 1024 one, tick **Keep the texture size in the world** so the bricks keep their size.
- Press **Mark** in the texture browser first to see where a material is used.

## FAQ

**The count stays at 0.** Check **In** (with **Selection**, nothing selected means nothing matches) and the match mode: in exact mode the whole name must match.

**Can I undo a replacement?** Yes, `Ctrl+Z` undoes the whole replacement in one step.

**Does it change entities' materials (overlays, decals, props)?** No, only brush faces (including displacements). For texts in entity keys, use **Find and replace in entities...** (see [Entity properties](entity-properties.md)).

## Related pages

- [Texture browser](texture-browser.md)
- [Texture tool](texture-tool.md)
- [Entity properties](entity-properties.md)
- [Undo and history](undo-history.md)
- [MCP tool reference](../../MCP.md) (`replace_materials`, `find_material_usage`)

---
[Back to the documentation index](../README.md)
