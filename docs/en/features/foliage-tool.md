[Documentation](../README.md) > Features > Foliage tool | [Français](../../fr/features/foliage-tool.md) | [Русский](../../ru/features/foliage-tool.md)

# Foliage tool

Paint trees, bushes, rocks and other props on displacements, brushes and props with a brush, and choose where the detail props (the `%detailtype` grass of the compile) grow. The tool is opt in: nothing happens until you pick it.

## Opening it

- The **Foliage** button of the tool strip (tree icon, in the creation group).
- **Tools > Placement and props > Foliage paint tool**, action `tool_foliage`.
- MCP: `paint_foliage`, `erase_foliage`, `set_face_detail_type`, `preview_detail_props`, `list_detail_types`, and `set_tool_options` with tool `foliage`.

The options live in the tool panel on the side. They are saved with the settings (`placement.foliage`).

## Modes

| Mode | Drag | Shift + drag |
|---|---|---|
| Paint | Paints props of the palette | Erases the props of the palette's layer |
| Erase | Erases the props of the palette's layer | Same |
| Detail alpha | Raises the displacement alpha under the brush | Lowers it |
| Detail per face | Gives the faces under the cursor the chosen detail type | Clears the face's own type |

Alt + mouse wheel changes the radius. Each stroke (press, drag, release) is one undo step. Painting works in the 3D views and in the top 2D view (traced straight down).

## Palettes

A palette is a list of models with a weight each: a model with weight 2 is picked twice as often as one with weight 1. Each entry also has a class (`prop_static`, `prop_dynamic` or `prop_physics`) and can be turned off with its check box.

- Drag models from the model browser onto the drop zone of the panel, or use **Add the current model** (the model chosen in the browser).
- Several palettes can be kept (+ and the bin button); the name field renames the active one.
- Every painted prop joins the visgroup **Foliage: <palette name>**, created on the first stroke. Hide, select or delete a whole layer from the visgroups panel.

## Painting options

- **Radius**: brush radius in units.
- **Density**: props per 256 x 256 units of surface. A dab only adds what is missing in its disc, so going over the same place again keeps the density instead of piling props up.
- **Spacing**: least distance between two props of the layer (Poisson disk).
- **Scale**: random scale range, written to `modelscale`.
- **Random yaw**, **Random skin** (picks one of the model's skins).
- **Align**: 0 keeps the props upright, 1 leans them fully to the surface normal, in between mixes both.
- **Slope**: only on surfaces whose slope (degrees from flat) is in the range.
- **Height limit**: only between two world heights.
- **Avoid overlap**: the real footprint of each model, taken from its mesh (the shapes of placement on real shapes), never overlaps another painted or existing prop.
- **Drop to surface**: the real mesh of the model is lowered onto the real surfaces (displacements, brushes, other props) until it touches, so the prop neither floats nor sinks.
- Support: before placing, the ground is probed under several points of the model's contact patch (the lowest part of its real mesh). A prop whose footprint hangs over an edge, a hole or a slope outside the limits is not placed.
- **Paint on props**: props of other layers and of the map receive props too (grass on rocks). Props of the palette's own layer never do.
- **Sink**: offset along the up direction, negative sinks the props in the ground.

Performance: the brush only looks at the props near it, through a spatial grid built once per stroke, so painting stays fast with thousands of props in the map.

## Detail props: where grass grows

Detail props come from the `%detailtype` of the material and the groups of `detail.vbsp` (see [Detail props preview](detail-props-preview.md)). Two modes control them:

- **Detail alpha** paints the displacement vertex alpha. On blend materials (WorldVertexTransition) the alpha both blends the two textures and chooses the detail group by its `alpha`, exactly like VBSP, so painting the grass layer makes grass grow there. **Strength** is the alpha added per dab.
- **Detail per face** gives faces their own detail type: a group name of `detail.vbsp`, `none` to keep a face bare even when its material has a `%detailtype`, or Shift + click to go back to the material. The type is stored in the VMF as the side key `hp_detailtype`. The Hammer Pro compiler and the preview honour it; Hammer and Hammer++ open the file but ignore the key.

Turn on the preview (**Show the detail preview** in the panel, or the leaf button of the toolbar) to watch the grass follow your strokes.

## MCP

- `paint_foliage`: stroke points, radius, a palette (saved name, or `items` with model, weight, classname) and any option above; `preview` counts without placing.
- `erase_foliage`: stroke points, radius, a palette name (or every foliage layer, or `any_prop`).
- `set_face_detail_type`: faces and a type (`none`, a group, or empty).
- `paint_displacement` with mode `alpha` paints the detail alpha.
- `preview_detail_props` and `list_detail_types`: see [Detail props preview](detail-props-preview.md).

See also [Sprinkle tool](sprinkle-tool.md), [Scatter by material](scatter-by-material.md), [Real shape placement](real-shape-placement.md).
