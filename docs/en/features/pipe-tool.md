[Documentation](../README.md) > Features > Pipe tool | [Français](../../fr/features/pipe-tool.md)

# Pipe tool

The pipe tool builds pipes and ducts out of brushes (not props): straight runs cut into several brushes, 90 degree elbows made of brush slices that approximate an arc, and optional collars (flanges). The profile is square or round, hollow (a tube made of wall brushes) or solid. A route can have as many bends as you want, in any of the six grid directions, so L, S, U and J shapes and climbing routes are all possible. The result is grouped and is one undo step.

## Opening it

- **Tools > Pipe tool**, or its button in the tool palette (creation group, after the road tool). It has no default shortcut.
- Everything follows the **active grid**: see [Grids](grids.md). The start snaps to the grid, the directions are the axes of the grid (so a rotated grid gives a rotated pipe), lengths round to the grid step along the pipe, and the radius and the bend radius round to the smallest step of the grid. Wall thickness, collar size and collar length round to half a grid step. With snapping off (the **Snap to grid** toggle) nothing is rounded.

## Drawing a route

1. **Click** the start in a 2D view (or on a surface in the 3D view).
2. **Click** the next corners in the same 2D view. The pipe turns at every click, and each new point is held on a grid axis of the view plane: the segment is straight and its length is a whole number of grid steps. The pipe lies in the plane of that view (at the depth of the first click, which comes from the selection like the block tool); to climb, type the route (below), or draw it in the Front or Side view.
3. The preview shows the finished pipe in every view, with a label giving the segment length, the bends and the brush count. A red segment says why the route is refused (a run too short for its bends, a U-turn).
4. `Enter` or a double click creates the pipe. `Backspace` removes the last point, `Escape` cancels.

A pipe turns by exactly 90 degrees: a U-turn or a slanted corner is refused.

## Typing a route

The **Route** field of the options bar takes moves after the clicked points (or after one click, with the **Start** direction): a number goes forward by that length, and `L`, `R`, `U`, `D` turn left, right, up or down by 90 degrees. Example: `256 L 128 U 64` goes 256 forward, turns left, goes 128, turns up, goes 64. Left and right turn around the up axis of the grid, up and down tilt the pipe. Separate the moves with spaces or commas.

## How lengths work

Lengths are measured from corner to corner. A bend of radius `Bend` takes that radius off both runs next to it (the arc starts `Bend` before the corner and ends `Bend` after it). So a run between two bends must be at least twice `Bend` long, and a run at the end of the pipe at least `Bend` long. The tool tells you which run is too short.

## Options

| Option | What it does | Range | Default |
|---|---|---|---|
| **Shape** | Square, or round (a polygon with **Sides**). | | Square |
| **Sides** | Side count of a round profile. | 5 to 48 | 12 |
| **Radius** | Half the width of the pipe (outside). | 1 to 2048 | 32 |
| **Hollow**, **Wall** | A tube of wall brushes around an empty inside, and its thickness (smaller than the radius). Unticked: solid brushes. | | on, 8 |
| **Brushes** | Brushes along each straight run. | 1 to 256 | 4 |
| **Bend** | Radius of the 90 degree bends, on the axis of the pipe. It is made larger than the radius if it is not. | | 64 |
| **Slices** | Brush slices per bend: more slices, a rounder elbow. | 1 to 32 | 4 |
| **Collar every** | A collar between every N brushes of a run (0: none). | 0 to 256 | 0 |
| **Collar size**, **Collar length** | How far a collar sticks out and how long it is. Collars are rings around the pipe and never overlap its brushes. | | 8, 8 |
| **Collars at the ends**, **Collars at bends** | A collar at both ends of the pipe, or on both sides of every bend. | | off |
| **Route**, **Start** | The typed moves, and the first direction when the route starts from a single point (`+x -x +y -y +z -z` along the grid). | | |
| **Texture scale**, material | The material is the current one. | | |

A hollow round pipe with bends has non planar wall pieces; they are built as convex brushes and are valid, with a few extra faces on the bends.

## MCP

`create_pipe` builds the same pipe: `start`, `direction` (`+x` to `-z` along the grid) or `direction_vector`, then `moves` (numbers and `left`, `right`, `up`, `down`), or `path` as text, or `length` for a straight pipe; and `shape`, `sides`, `radius`, `hollow`, `wall`, `segments`, `segment_length`, `bend_radius`, `bend_slices`, `collar_every`, `collar_size`, `collar_length`, `collar_ends`, `collar_bends`, `snap`, `material`, `collar_material`, `tex_scale`, `group`, `view` (the grid used, default top) and `preview` (describe the pipe without creating it). The answer gives the brush counts, the effective sizes after snapping, and the `end` and `end_direction` so the next pipe can continue from it.

`set_tool_options` with tool `pipe` sets the options of the tool (`shape`, `sides`, `radius`, `hollow`, `wall`, `segments`, `segment_length`, `bend_radius`, `bend_slices`, `collar_every`, `collar_size`, `collar_length`, `collar_ends`, `collar_bends`, `tex_scale`, `path`, `heading`, `depth`, `keep_active`). See the [MCP tool reference](../../MCP.md).

## Tips

- Keep the radius and the grid in a simple ratio (grid 16, radius 32) so every face of the pipe lies on the grid.
- Use few slices for far away pipes and 6 to 8 for pipes players see up close.
- Brushes of a pipe are grouped: select the group to move it, or ungroup to edit one brush.

## Related pages

- [Grids](grids.md)
- [Curve tool](curve-tool.md)
- [Road tool](road-tool.md)
- [Block tool](block-tool.md)
