[Documentation](../README.md) > Features > Decals and overlays | [Français](../../fr/features/decals-overlays.md) | [Русский](../../ru/features/decals-overlays.md)

# Decals and overlays

The decal tool places `infodecal` entities and the overlay tool places `info_overlay` entities on brush faces, both with the current material. Decals and overlays are drawn on their faces in the views the way the engine applies them, and overlays can be reshaped and spread over several faces.

## Opening the tools

| Tool | Menu | Default shortcut |
| --- | --- | --- |
| **Decal tool** | **Tools > Decal tool** | `Shift+D` |
| **Overlay tool** | **Tools > Overlay tool** | `Shift+O` |

Both tools are also in the tool palette. Both work in the 3D view only: clicking in a 2D view shows the message **Click a face in the 3D view** and the cursor shows that the view is not usable.

The material placed is the current material (the one chosen in the [texture browser](texture-browser.md)). When none is chosen, the default material of the settings is used, else `dev/dev_measuregeneric01b`.

## Decal tool

Move the cursor over a face of the 3D view: a preview shows the decal with its real size and orientation, with the material name and size next to the cursor. Click to place an `infodecal` whose `texture` key is the current material.

- The decal is placed 1 unit in front of the surface, like Hammer, and gets the FGD default keyvalues of `infodecal`.
- Its size is the texture size in pixels times the `$decalscale` of the material (64 x 64 while the material is not loaded yet).
- Its orientation follows the engine rules: on floors and ceilings the texture right direction is +X, on walls the texture down direction is -Z.
- The new decal becomes the selection. Hold `Shift` while clicking to keep the current selection instead.
- Each decal is one undo step.

### Options

| Option | What it does | Default |
| --- | --- | --- |
| Material chip | Thumbnail and name of the current material. Click it to show the texture browser tab. | |
| **Size: W x H** | Size of the decal in world units (texture size). Read only. | |
| **Snap to grid** | Snap the decal position to the grid on the surface (`Alt` while clicking: free). | Off |

## Overlay tool

Move the cursor over a face of the 3D view: a preview shows the overlay with the material name and size. Then:

| Gesture | What it does |
| --- | --- |
| Click a bare face | Place an `info_overlay` with the current material, centered on the point clicked (on the face plane). It becomes the selection. |
| `Shift`+click a bare face | Place an overlay and keep the current selection. |
| Click an overlay | Select it. When overlays overlap, the last one in the file (the one drawn on top) is picked. |
| `Shift`+click an overlay | Add it to or remove it from the selection. |
| `Ctrl`+click a face | Add the face to, or remove it from, the faces of the selected overlays. An overlay always keeps at least one face (**An overlay keeps at least one face**). One undo step. |
| Drag a corner handle | Reshape the selected overlay: the corner moves on the overlay plane, snapped to the grid and to the edges and corners of its faces, and stays on those faces. A label shows the corner key (`uv0` to `uv3`) and its value. One undo step per drag. |
| `Alt`+drag a corner | Move the corner off the grid. |
| `Shift`+drag a corner | Stick the corner to a corner of another overlay within a few pixels, for seamless roads and trims. |
| `Escape` during a drag | Cancel the drag. |

A new overlay gets the keys Hammer writes so the compiler builds it: `material`, `sides` (the face clicked), `StartU`/`StartV` 0 and `EndU`/`EndV` 1, `BasisOrigin`, `BasisU`, `BasisV`, `BasisNormal` (the same basis as Hammer) and the four corners `uv0` to `uv3`, half the texture size around the center. The size is the texture size times `$decalscale`, like decals, and can be changed afterwards by dragging the corners. Corner handles are shown for up to 32 selected overlays at once.

### Options

| Option | What it does | Default |
| --- | --- | --- |
| Material chip | Thumbnail and name of the current material. Click it to show the texture browser tab. | |
| **Size: W x H** | Size of a new overlay in world units (texture size, editable afterwards). Read only. | |
| **Snap to grid** | Snap the center of a new overlay to the grid on the face (`Alt`: free). | Off |

## Display in the views

- Decals (`infodecal` and any class with the FGD `decal()` helper) are projected on the brush faces under their origin, like static decals in game. They are drawn again when the entity or a brush under it changes. In the player view a named `infodecal` stays hidden until an `Activate` input reaches it, like in game (an unnamed one is applied when the map loads).
- Overlays (`info_overlay`, `info_overlay_transition` and any class with the FGD `overlay()` or `overlay_transition()` helper) are drawn with their material, projected on every face of their `sides` list (displacements included) and clipped to them. The 2D views show the clipped outline. An overlay is selected and hidden together with its entity. An `info_overlay_transition` (a shoreline) is compiled into the water overlays of the map, drawn by the game on the water surface: one strip per shore face (`sides`) that crosses the plane of a water face (`sides2`), `Width1` units onto the land side and `Width2` units over the water. The strips follow each other along the shore whatever the order of the faces in `sides`, so the length texture coordinates run on without a seam from one end of the shore to the other, and the editor draws exactly these strips. Every strip is written upright, its length axis keeping the water on its left: the game builds nothing for a water overlay with a mirrored axis (measured, even with `r_overlaywireframe 1`), so a mirrored strip saved by Valve's Hammer is turned upright with the same corners and texture coordinates, and a saved strip without axes is skipped with a warning. A transition saved by Valve's Hammer also carries the strips Hammer computed (its `overlaytransition` block): they are used when the shore faces give no strip, for example in a decompiled map. A transition whose shore does not cross its water gets a warning in the log.
- **View > Display options > Show overlays** turns the overlays on or off (on by default, no default shortcut). The same switch is in **Settings > Helpers > Overlays and ropes**, saved as `show_overlays` in the `[helpers]` table of `settings.toml`.

## Overlays follow your edits

The compiler builds an overlay from its basis keys, its corners and its face list, not from `origin`. Hammer Pro moves those keys with the entity when you move, rotate or flip it, and updates the face list when the overlay leaves its faces. Pasted and duplicated overlays point to the pasted faces, and carving or clipping a brush updates the face lists of the overlays (and `env_cubemap` faces) that used it.

To edit the face list by hand, use the eyedropper button next to the `sides` key in the [entity properties](entity-properties.md) (SmartEdit): pick faces in the 3D view (`Ctrl`+click several, `Enter` to finish), `Ctrl`+click the button to add to the list.

## Tips

- **Edit > Set maximum distance...** writes `fademaxdist` on overlays so they fade out with distance.
- [Check for problems](check-problems.md) finds overlays whose faces no longer exist (with a fix that removes them from the lists) and overlays on more than 64 faces. A map is limited to 8192 overlays.
- The auto visgroup **Overlays and decals** holds every `info_overlay` and `infodecal` (see [Visgroups](visgroups.md)).
- MCP tools: `list_decals`, `create_decal`, `list_overlays`, `create_overlay`, `set_overlay`. See the [MCP tool reference](../../MCP.md).
- `set_tool_options` with tool `decal` or `overlay` and the option `snap` (`on` or `off`) switches the grid snapping of the tool, like its checkbox.

## FAQ

**Clicking in the top view does nothing.** Both tools place on a face hit in the 3D view only.

**My decal is huge (or tiny).** The size comes from the texture and its `$decalscale`. Use a decal material, or change `$decalscale` in the material (see [Material editor](material-editor.md)).

**How do I make an overlay cover two brushes?** Select it and `Ctrl`+click the other face with the overlay tool, then drag the corners over it.

**My overlays disappeared from the views.** Check **View > Display options > Show overlays**.

## Related pages

- [Texture browser](texture-browser.md)
- [Entity properties](entity-properties.md)
- [Texture application](texture-tool.md)
- [Render modes and display options](render-modes-display.md)
- [Check for problems](check-problems.md)

---
[Back to the documentation index](../README.md)
