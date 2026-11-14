[Documentation](../README.md) > Features > Texture tool | [Français](../../fr/features/texture-tool.md) | [Русский](../../ru/features/texture-tool.md)

# Texture tool

The texture application tool is Hammer's "Face Edit" mode: you pick faces in the 3D view, then the **Face edit** panel changes their material, scale, shift, rotation, lightmap scale, justification, alignment, smoothing groups and displacements, all selected faces at once. It also holds the [UV editor](uv-editor.md).

## Opening it

| How | Shortcut |
|---|---|
| **Tools > Active tool > Texture application**, or its button in the tool strip | `Shift+A` |

When you switch to the tool with objects selected, their faces become the face selection; when you leave it, the face selection is cleared and the objects you had selected come back. The right dock shows the **Face edit** panel while the tool is active, and the **Textures** tab after it.

## Selecting faces

Picking works in the 3D view. The faces under the mouse are outlined; the selected faces get a red highlight in the 3D view and an outline in the 2D views.

| Mouse in the 3D view | Result |
|---|---|
| Click | Selects the face (replaces the selection) and makes its material the current material. |
| `Ctrl+click` | Adds or removes the face. |
| `Shift+click` | Selects every face of the solid. `Ctrl+Shift+click` adds them. |
| Hold the left button and sweep | Selects the faces under the mouse one after the other (`Ctrl` or `Shift`: added to the selection). |
| Click in the void | Clears the face selection. |
| `Alt+click` | Eyedropper: the face material becomes the current material. |
| Right click | Applies the current material to the face. |
| `Ctrl+right click` | Applies `tools/toolsnodraw` to the clicked face, keeping the current material and face selection. |
| `Alt+right click` | Wraps: copies the material of the first selected face onto the clicked face and continues its texture across the shared edge. Select the source face first. |
| Hold the right button and sweep (Hammer++ camera scheme) | Paints: every face the mouse passes over gets the current material, like Hammer++. `Ctrl` paints `tools/toolsnodraw`, `Alt` wraps each face from the selected face. The whole stroke is one undo step and the face selection does not change. |
| `Escape` | Clears the face selection. |

The right drag paints only with the **Hammer++ (middle mouse)** camera scheme (**Settings > Viewports > Camera controls**), where the middle button moves the camera and the right button is left to the tools. With the **Hammer Pro** and **Hammer** schemes the right drag looks around and a right click applies, as before.

The status bar reminds these keys. **Edit > Selection element > Previous face selection** (`Alt+Z`) and **Edit > Selection element > Next face selection** (`Alt+Y`), also the **◀ Previous selection** and **Next selection ▶** buttons at the top of the panel, go back and forth through your earlier face selections.

## The Face edit panel

The first line shows the number of selected faces (and how many are displacements). With no face selected the panel only says **No face selected**.

### Material

The material box shows the material of the selected faces. When they use several materials, it says **(multiple values)** with up to four thumbnails and the number of materials (hover for the list).

| Control | What it does |
|---|---|
| **Browse...** | Opens the texture browser: the next material you pick there is applied to the selected faces, then the panel comes back. |
| **Edit material** | Opens the [material editor](material-editor.md) on the material of the selected faces (on all of them when the faces use several). |
| **Apply current** | Applies the current material of the texture browser to the selected faces. |
| Image button (**Make this the current material**) | Makes the material of the faces current (only when they share one). |
| Drop a material on the box | Applies the dragged material to the selected faces. |

### Values

| Field | What it does |
|---|---|
| **Scale** X / U and Y / V | Texture scale on each axis (0 is refused). |
| **Shift** X / U and Y / V | Texture shift in texels. |
| **Rotation** | Texture rotation in degrees. **-90** and **+90** turn the texture of every face by a quarter turn. |
| **Lightmap scale** | Luxel size of the faces (1 to 1024, smaller is finer). |

A field that differs between the faces shows **(multiple values)**; typing or dragging sets that value on every face. A continuous drag is one undo step. The number of decimals and the drag steps are set in **Settings > Entities and browsers > Texture application (Face Edit)**:

| Setting | Key in `[entities]` | Default |
|---|---|---|
| **Decimals shown** (0 to 6) | `texture_decimals` | 4 |
| **Shift drag step** (texels per point, 0.01 to 64) | `texture_shift_step` | 1 |
| **Scale drag step** (per point, 0.0005 to 1) | `texture_scale_step` | 0.005 |

### UV editor

The **UV editor** section draws the face over its texture and lets you move, scale and rotate it with the mouse, edit vertices and edges, and move the other layers of the material. **Open as a separate window** puts it in a window of its own. See [UV editor](uv-editor.md).

### Justify and align

| Control | What it does |
|---|---|
| **Justify**: **Left**, **Right**, **Top**, **Bottom** | Puts that edge of the texture on the same edge of the face. |
| **Center** | Puts the texture center on the face center. |
| **Fit** | Scales the texture so it covers the face exactly once. |
| **Treat as one** | Justify and fit all selected faces as one surface instead of each face alone. Off by default. |
| **Align**: **World** | Projects the texture from the closest world axis (Hammer's World alignment). |
| **Face** | Lays the texture in the face plane (Hammer's Face alignment). |
| **View** | Aligns the textures to the 3D camera. |
| **Repeat** U, V and **Fit** | Scales the texture so it repeats U times and V times on the face (0.01 to 256, default 1 and 1). Uses **Treat as one** too. |

### Smoothing groups

Buttons **1** to **24** toggle the smoothing groups of the selected faces (used by the lighting to smooth across faces of the same group). A number in the warning color is set on some faces only: click it to set it on all.

### Displacement

| Control | What it does |
|---|---|
| **Create, power:** **2**, **3**, **4** | Turns the selected faces into displacements of that power (faces must have 4 sides). |
| **Destroy** | Turns the selected displacements back into plain faces. |

See [Displacement tool](displacement-tool.md) for sculpting.

### Selection

| Control | What it does |
|---|---|
| **Mark** | Selects every face of the map using the same materials as the selection. |
| **Hide mask** | Hides the red highlight of the selected faces (to judge the texture). |
| **Select none** | Clears the face selection. |

## Related commands

| Command | Menu | Shortcut |
|---|---|---|
| **Apply current texture** | **Tools** | `Shift+T` (works with any tool: selected faces, or every face of the selected objects) |
| **Texture lock** | **Tools**, toolbar | `Shift+L` |
| **Texture scale lock** | **Tools**, toolbar | `Shift+K` |
| **Find and replace textures...** | **Tools** | `Ctrl+Shift+R` (no key with the Hammer and Hammer++ presets) |

The texture locks are described in [Brush operations](brush-operations.md).

## Tips

- Select a whole room with `Shift+click` on each wall, then **Justify > Fit** with **Treat as one** to stretch one texture over all of it.
- `Alt+click` a face you like, then right click other faces to paint its material.
- To continue a texture around a corner, select the textured face, then `Alt+right click` the next one.
- **Mark** on a selection shows every other face that uses the same materials, ready for one change.

## FAQ

**Clicks do nothing in the 2D views.** Faces are picked in the 3D view only; the 2D views show the outline of the selected faces.

**Why does Scale refuse 0?** A zero scale would make the texture infinite; the field ignores it.

**The red highlight hides my texture.** Toggle **Hide mask**.

**How do I go back to my previous face selection?** `Alt+Z` (**Previous face selection**), and `Alt+Y` to come back.

## Related pages

- [Texture browser](texture-browser.md)
- [UV editor](uv-editor.md)
- [Find and replace textures](find-replace-textures.md)
- [Displacement tool](displacement-tool.md)
- [Brush operations](brush-operations.md)
- [Keyboard shortcuts](keyboard-shortcuts.md)
- [MCP tool reference](../../MCP.md) (`get_faces`, `select_faces`, `set_face_texture`, `justify_texture`, `fit_texture`, `align_texture_to_view`, `wrap_texture`, `select_by_material`)

---
[Back to the documentation index](../README.md)
