[Documentation](../README.md) > Features > Polygon tool | [Français](../../fr/features/polygon-tool.md) | [Русский](../../ru/features/polygon-tool.md)

# Polygon tool

The polygon tool (like the one of Hammer++) creates one brush from a convex outline: you click its points in a 2D view and the outline is extruded along the depth of that view.

## Opening it

- **Tools > Active tool > Polygon tool**, the tool palette, or `Shift+N`.
- The tool works in the 2D views only; clicks in the 3D view are ignored.

## Drawing a brush

1. Click in a 2D view to place the first point, then the next ones. Points snap to the grid (hold `Alt` to place them without snapping).
2. While you move the mouse, the length of the edge being drawn is shown next to the cursor.
3. A point that would make the outline concave is refused: the marker turns red with **Concave: point refused**. A point already in the outline is ignored. An outline holds at most 128 points (**Too many points**).
4. Once the outline has 3 points or more, press `Enter`, double click, or click **Create** to build the brush. The status bar tells how many points are still needed before that.

`Backspace` (or the **Remove point** button) removes the last point. `Escape` cancels the outline.

## Editing the outline before creating

- **Move a point**: drag it in the view where the outline is drawn. A move that would make the outline concave or degenerate is refused.
- **Depth**: the extrusion range along the view axis comes, like the block tool, from the selection bounds, else from the last object created by the block or polygon tool, else 64 units. In the other 2D views the outline appears as a rectangle: drag one of its ends to change the depth, or drag its inside to move the whole range. Depth changes snap to the grid.
- The outline belongs to the view kind it was started in (Top, Front or Side). Clicks in the other 2D views only edit the depth; press `Escape` to start over in another view.
- The 3D view shows the prism and its size.

## Options

| Option | What it does | Default |
|---|---|---|
| **Scale** | Texture scale of the new faces (world units per texel, 0.01 to 64). | **Default texture scale** setting (0.25) |
| Create as | **World brush**, **func_detail** or any brush entity class of the FGD (searchable list, a typed class name is accepted). With a class, the brush becomes a new brush entity with the FGD default keyvalues. | World brush |
| Current material | Material put on every face (the default material when none is chosen). Click it to show the texture browser. | `dev/dev_measuregeneric01b` |
| Pin (keep active) | Keep the polygon tool active after creating a brush. When off, the selection tool comes back. | on |
| **Remove point** | Remove the last point (`Backspace`). | |
| **Create** | Create the brush (`Enter` or double click). | |
| Point count | **N point(s)**: number of points of the outline. | |

The new brush becomes the selection; the creation is one undo step.

## Tips

- The tool only accepts convex outlines. For a concave shape, draw several convex brushes side by side.
- Select the floor first: the outline takes the height range of the selection, so walls and pillars drawn in the Top view get the right height at once.
- After creating, use the [Vertex tool](vertex-tool.md) if you need to tilt or bend the brush.
- The MCP tool `create_polygon_brush` builds the same kind of brush from a list of points (see [MCP tool reference](../../MCP.md)).
- `set_tool_options` with tool `polygon` sets the options of the tool itself: `tex_scale`, `create_as` (`world` or a brush entity class) and `keep_active`.

## FAQ

**Why is my point refused?** It would make the outline concave (or it is the same point as another one). Place the points in order around the shape, always turning the same way.

**Can I draw in the 3D view?** No. Use the block tool to draw on surfaces in 3D.

**How do I change the thickness after drawing the outline?** Drag the ends of the outline rectangle in one of the other two 2D views before creating the brush.

## Related pages

- [Block tool](block-tool.md)
- [Vertex tool](vertex-tool.md)
- [Clip tool](clip-tool.md)
- [Grids](grids.md)
- [Keyboard shortcuts](keyboard-shortcuts.md)

---
[Back to the documentation index](../README.md)
