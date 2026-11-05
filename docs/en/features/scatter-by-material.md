[Documentation](../README.md) > Features > Scatter by material | [Français](../../fr/features/scatter-by-material.md)

# Scatter by material

Scatter by material places props (grass, rocks, bushes...) on the surfaces of brushes and displacements, and only where the texture is the one you chose. On a blend material (WorldVertexTransition and the other lightmapped blends with `$basetexture` and `$basetexture2`), the alpha painted on the displacement vertices says where each texture shows, so grass can grow only where `grass1` dominates and rocks only where `dirt1` does. It extends the [sprinkle](sprinkle-tool.md) placement: the same entities, the same surface search, but driven by a list of assets, a count and a spacing instead of a brush.

## Opening it

**Tools > Scatter by material...**, or the `scatter_by_material` action (command palette, MCP `run_action`). It is a window; it does not change the active tool.

## Using it

1. Select the brushes or displacements that receive the assets, or tick **Whole map**.
2. Add the assets one by one with **Add an asset**: the **Model** path (`models/.../name.mdl`) and the class (`prop_static`, `prop_dynamic`...). **From selected props** adds the models of the prop entities you selected.
3. For each asset, choose where it may appear:
   - **Texture**: a name matched against the material, its `$basetexture` (first layer) and its `$basetexture2` (second layer), with `*` and `?` allowed (`grass1`, `nature/*`). The small list next to the field offers the textures found on the selected surfaces. Empty means anywhere.
   - **Min blend**: how much of that texture must show at the point (default 0.5, "dominant"). On a displacement, the first layer shows `1 - alpha` and the second layer `alpha`, interpolated between the vertices under the point. A face of another material shows its texture fully.
   - **Weight**: how often the asset is picked compared to the others that are allowed at that point.
4. Choose the **Count**, the **Spacing** (least distance between two assets), the **Seed** (same seed, same result), the **Steepest slope**, and the random options: **Random rotation** (yaw), **Align to the surface**, **Scale** (a minimum and a maximum, written to the `modelscale` key, for games that read it) and **Offset** (a shift along the surface normal, negative sinks the asset into the ground).
5. Click **Preview** to see how many assets would be placed, how many per asset, and why some spots were skipped (no surface, too steep, too close, another texture). **Generate** places them as `prop_static` (or the chosen class) entities in one undo step.

Positions are random by default. Tick **Snap positions to the grid** to put them on the active grid, with its rotation and custom steps; this snaps the position on the surface, never its height.

While the materials are still loading, the preview says so and retries by itself.

## Notes

- Each placement drops from above the area onto the first targeted surface; surfaces that are not targeted are ignored, so props do not land on a roof above the terrain.
- The count is a goal: fewer assets are placed when the surface is small, steep, too far from a matching texture, or the spacing is large.
- Models are not checked against the game files; use the [model browser](model-browser.md) to find paths.

## MCP

The `sprinkle` tool does the same: `items` (each with `classname` or `model`, `keyvalues`, `weight`, `texture`, `min_blend`), `count`, `spacing`, `seed`, the area as `center` and `radius`, or `min` and `max`, or `solid_ids` (the targeted brushes and displacements, whose bounds become the area), `random_yaw`, `align`, `max_slope`, `scale_min`, `scale_max`, `offset_min`, `offset_max`, `snap_to_grid` with `view` (the grid used), and `preview` to count without placing. The answer lists what would be or was placed, per asset, and the skipped spots by reason. See the [MCP tool reference](../../MCP.md).

## Related pages

- [Sprinkle tool](sprinkle-tool.md)
- [Displacement tool](displacement-tool.md)
- [Grids](grids.md)
- [Model browser](model-browser.md)
