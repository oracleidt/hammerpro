[Documentation](../README.md) > Features > UV editor | [Français](../../fr/features/uv-editor.md) | [Русский](../../ru/features/uv-editor.md)

# UV editor

The UV editor draws the selected face over its texture (in texels, the texture repeated) and lets you move, scale and rotate it with the mouse, like the UV editors of 3D modelers. Its Edge and Vertex modes align an edge on an axis, give it an angle, fit tiles along it or drag vertices over the texture. It also moves the other layers of the material (bump map, detail, second blend texture, envmap mask, blend mask).

## Opening it

The UV editor is the **UV editor** section of the **Face edit** panel of the [texture tool](texture-tool.md) (`Shift+A`), open by default. Select one or more faces in the 3D view to fill it.

- **Open as a separate window** puts it in a window of its own, like the Compile and run window: move it out of Hammer Pro or to another screen, where it uses all the room of the window. The panel then says **The UV editor is in its own window.**
- **Back in the panel** (in the panel, or the button of the window's title bar) puts it back.

## How Source maps textures

A Source brush face has no UV per vertex: its texture is one planar projection (U and V axes, scales, shifts), the same for a displacement and its base face. Every gesture of the editor is therefore a change of that projection: a move, a turn, a scale or a skew, never a bent texture. Hover **One projection per face** in the Edge and Vertex modes for a reminder.

## Toolbar

| Control | What it does | Default |
|---|---|---|
| **All faces (n)** | Move, scale and rotate every selected face together (each about its own outline). Off: only the face shown. Shown with several faces. | on |
| `◀` **Face i of n** `▶` | Shows the previous or next selected face. | |
| **Frame** | Fits the view on the face. | |
| **Face** / **Edge** / **Vertex** | The pick mode (see below). | **Face** |
| **Layer:** chips | **Base texture** and the other layers the material has: **Base texture 2**, **Bump map**, **Bump map 2**, **Detail**, **Envmap mask**, **Blend mask**. | **Base texture** |

Navigation on the canvas: drag the background (or right drag, or middle drag) to pan, the wheel zooms. The hover shows what a press would grab (highlight, cursor and a hint over the canvas). The other selected faces with the same texture are drawn too, each at the copy of its outline nearest to the edited face; click one to edit it. Each drag is one undo step.

## Face mode

| Gesture | Result |
|---|---|
| Drag the face | Moves its outline over the texture (the texture stays). |
| Drag a corner or edge square | Scales it. `Shift`: both axes alike. |
| Drag the round handle | Rotates it by 15 degree steps. `Alt`: free. |
| Click another selected face | Makes it the edited face. |

Buttons below the canvas: **Flip U**, **Flip V**, `⟲ 90`, `⟳ 90` (quarter turns) and **Center** (puts the middle of the face on the middle of the texture). With **All faces** they apply to every selected face. A line shows the scale, shift and rotation of the face.

## Edge mode

| Gesture | Result |
|---|---|
| Click an edge | Selects it. `Shift`: add, `Ctrl`: toggle. A drag on the face or the background draws a selection box. |
| Drag a selected edge | It follows the mouse, the far vertices stay. |
| Drag the round knob | Turns the texture about the middle of the main edge (the last one picked), by 15 degree steps (`Alt`: free). |
| Double click an edge | Aligns it on the nearest axis. |
| `X` / `Y` (mouse over the canvas), or the small **X** and **Y** buttons next to the edge | Turns the texture so the edge lies on that axis (the smaller turn, the scale kept). |
| Arrows | Nudge the selection by 1 texel (`Shift`: 8 texels). |
| `Escape` | Clears the selection. |

With an edge selected, the panel offers **Align on X axis**, **Align on Y axis**, an **Angle** field (direction of the main edge in degrees from the X axis, clockwise with V down, -180 to 180; changing it turns the texture about the middle of the edge), the buttons `⟲ 15`, `⟳ 15`, `⟲ 90`, `⟳ 90`, and **Fit along the edge** with a tile count (0.25 to 64, default 1): it aligns the edge on its nearest axis and scales the texture so that many tiles span it, a tile corner on its start. With **All faces**, the other faces use their edge most parallel to the main one.

## Vertex mode

| Gesture | Result |
|---|---|
| Click a vertex | Selects it. `Shift`: add, `Ctrl`: toggle, a box selects several. |
| Drag the selection | The selected vertices follow the mouse over the texture (snapping to the tile lines, `Alt`: free); the framed vertices stay, the others move as little as possible. |
| Double click a vertex | Puts a tile corner on it. |
| Arrows | Nudge by 1 texel (`Shift`: 8 texels). |
| `Escape` | Clears the selection. `Delete` does nothing (a brush face keeps its vertices). |

**Tile corner on the vertex** shifts the texture so a tile corner lies on the last selected vertex (with **All faces**, on the faces sharing it). The panel shows **N vertices selected** (or **N edges selected**) and **Clear**.

Since the whole face shares one projection, three corners of a box cannot stay while the fourth moves: you get the closest projection to your gesture.

## Other layers of the material

Pick a layer in **Layer:** to edit it on the same canvas. The canvas shows that texture mapped on the face through the face mapping and the layer transform, with **Base texture under it** (on by default) and an opacity slider for the base texture. The same gestures then change the layer transform (`$bumptransform`, `$detailtexturetransform`, `$envmapmasktransform`...) instead of the face.

| Control | What it does |
|---|---|
| **Scale**, **Shift**, **Rotation**, **Center** (U and V) | The values of the layer transform. |
| **Detail scale** | `$detailscale`, for the **Detail** layer: the detail repeats this many times per base texture. |
| **Flip U**, **Flip V**, `⟲ 90`, `⟳ 90`, **Center**, **Fit to face**, **Reset** | Quick operations on the layer: mirror, quarter turn, the middle of the face on the middle of a tile, one tile over the face, or no transform at all. |
| **Undo**, **Redo** | The layer edits have their own undo until they are applied. |
| **Revert** | Back to the values of the material file. |
| **Apply to these faces (n)** | Writes a variant of the material (a `patch` of it) in your first custom folder and gives it to these faces only (one undo step). It is packed into the map when compiling. |
| **Apply to the material** | Changes the VMT of the material: every face using it changes (a backup of the file is kept). |

**Not applied yet** reminds you that an edit waits. A line above the values tells what the shader does with the layer: whether the game reads that transform, ignores it, or does not draw the layer at all (**Not drawn by the game with this material**). The 3D views do not draw these layers (**Base texture 2** is drawn with the base texture mapping, like the game): check them here or in game.

### Layers the shader does not move

Some shaders draw a layer with the base texture mapping and ignore its transform (for example the bump map and the detail of LightmappedGeneric, or `$basetexture2` of WorldVertexTransition). Hammer Pro moves them anyway by baking: applying writes a copy of the texture with the pixels moved (normals turned for a bump map) in your first custom folder, and the material uses it. The gestures then snap to what a texture can hold: quarter turns, flips, whole repeat counts and any shift. A self-shadowed bump map (`$ssbump`) can only shift, repeat and mirror along V.

Applying to faces or to a material needs a custom texture folder (**Add a custom folder (Textures tab) to hold the material variants.**). A material inside an archive cannot be changed in place: copy it into a custom folder with the [material editor](material-editor.md), or apply to the faces.

### Moved layers stay in place

On the **Base texture** layer, **Moved layers stay in place** (on by default) keeps the layers you moved here where they are on the face when you move the base texture: they get the opposite move, in the same undo step. Layers you never moved follow the base texture, like a bump map painted for it.

## Tips

- Use **Edge** mode and `X` or `Y` to straighten a texture on a slanted face in one key.
- **Fit along the edge** with 2 tiles is the quickest way to fit exactly two planks along a beam.
- Detach the editor to a second screen and keep the 3D view large.

## FAQ

**Why does moving one vertex move the others?** A Source face has one projection for the whole face; the editor finds the projection closest to your gesture.

**Why can't I turn my bump map by 30 degrees?** The shader ignores that transform, so Hammer Pro bakes the move into a texture, which only holds quarter turns, flips, whole repeats and shifts.

**Where are the variants written?** In your first custom texture folder; they are packed into the map at compile time.

**Can scripts or an AI drive the UV editor?** Yes: the texture tool options `uv_place`, `uv_all_faces`, `uv_mode`, `uv_layer`, `uv_keep_layers`, `uv_fit_tiles`, `uv_vertices` and `uv_edges` (MCP `set_tool_options`), and the MCP tools below.

## Related pages

- [Texture tool](texture-tool.md)
- [Material editor](material-editor.md)
- [Custom texture folders](custom-texture-folders.md)
- [Window frames](window-frames.md)
- [MCP tool reference](../../MCP.md) (`align_texture_edge`, `move_texture_vertices`, `set_texture_origin`, `get_uv_layers`, `edit_uv_layer`, `set_tool_options`, `screenshot_window` with `uv_editor`)

---
[Back to the documentation index](../README.md)
