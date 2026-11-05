[Documentation](../README.md) > Features > Slice tool | [Français](../../fr/features/slice-tool.md)

# Slice tool

The slice tool cuts the selected brushes into several blocks with evenly spaced parallel planes, from the 3D view alone. Move the mouse to place the cut, turn the wheel to choose how many cuts, click to apply. It also works in the 2D views.

## Opening it

- **Tools > Slice tool**, the tool palette, or `Shift+Y`.
- Select the brushes first. The tool works on one or several selected brushes at once, and on the brushes inside a selected group or brush entity.
- Selecting the tool again (`Shift+Y` while it is active) switches the orientation.

There is no panel and no dialog: the options bar and the status bar show one hint line with the orientation, the number of segments, the position, the number of cuts and blocks, and the keys.

## Orientation: left to right and top to bottom

| Orientation | The planes cut across | Result |
|---|---|---|
| **Left to right** (default) | the horizontal axis of the grid nearest the right of the screen | the object is split into blocks side by side, from left to right |
| **Top to bottom** | the vertical axis of the grid (the one nearest world Z) | the object is split into stacked layers, from top to bottom |

`Tab` or `O` toggles between the two. The orientation is relative to the view the mouse is in:

- In the 3D view, "left to right" follows the camera: the horizontal grid axis (X or Y) most along the right of the screen. Turn the camera and the axis follows.
- In a 2D view, "left to right" is the horizontal axis of that view and "top to bottom" its vertical axis (Top view: X and Y; Front and Side views: Y or X, and Z).
- On a rotated or custom grid the planes follow the axes of the active grid (see [Grids](grids.md)), not the world axes.

## Position, segments and blocks

- **Position.** The mouse sets the position of the cut along the axis. It snaps to the active grid step along that axis (the grid origin counts, so a shifted grid is respected); hold `Alt` for no snapping. The position never leaves the limits of the selection: with several brushes, the limits are those of the whole selection.
- **Segments.** The wheel changes the number of cuts, 1 to 64 (never fewer than 1). N cuts make N+1 blocks. In a 2D view the plain wheel keeps zooming, so use `Shift` + wheel there.
- **How the planes are spaced.** The first plane is at the position. The distance between the nearest face of the selection and the position is the step, and the planes repeat that step toward the other face. With the mouse 16 units from the left edge and 3 segments, the cuts are at 16, 32 and 48 units from that edge. A plane that would fall on or beyond the other face, or leave a piece thinner than 0.25 unit, is dropped, and the hint line shows the real number of cuts.
- **Center.** At the start, and after a right click, the position is the center of the object and the cuts divide it into N+1 equal blocks (one segment cuts it in half). The hint line shows **(center)**. It stays at the center until the mouse moves.

The preview draws every plane as a yellow quad with its coordinate written on it, inside a faint outline of the selection bounds, in the 3D and 2D views.

## Mouse and keys

| Input | What it does |
|---|---|
| Move the mouse | Place the cut (snapped to the grid). |
| Wheel (3D) / `Shift` + wheel (2D) | More or fewer segments (1 to 64). |
| Left click | Apply the cuts. |
| Right click | Put the position back at the center of the object. A right drag still looks around. |
| `Tab`, `O`, `Shift+Y` | Switch left to right / top to bottom. |
| `Enter` | Apply the cuts (like the left click). |
| `Escape` | Put the position back at the center. |
| `Alt` (held) | No snapping. |

## What happens to the brushes

- Every cut gives valid convex brushes. The pieces keep the original textures, alignment and lightmap scale of the brush; the new cut faces copy the most similar face of the brush, or get the current material when **Current material on cuts** is on (the clip tool setting `brush.clip_current_material`).
- The whole slice, all planes of all brushes, is **one undo step** (**Slice**). The new pieces are selected like the original brushes, so you can slice them again, along the other orientation for example, to cut a wall into bricks.
- Brush entity solids keep their entity. Brushes the planes do not cross are left alone. When nothing can be cut you get **No cut inside the selection**.

## MCP

The tool `slice_brushes` does the same without the tool: `ids` or `solid_ids` (default the selection), `orientation` (`left_right` or `top_bottom`) with `view` (`perspective`, `top`, `front`, `side`), or `axis` (`x`, `y`, `z` of the active grid, `world_axes` for the world axes), `position` (the plane distance along the axis, default the center), `segments` (1 to 64), `snap`, `cap_material` and `dry_run`, which answers the planes without editing. `set_tool_options` with tool `slice` sets `orientation` and `segments`, and `run_action` with `tool_slice` activates the tool. See the [MCP tool reference](../../MCP.md).

## Tips

- To cut a long wall into slabs of one grid width: put the mouse one grid width from the end, then turn the wheel until the last cut reaches the other end.
- To split a floor into four equal tiles: select it, one segment left to right, apply, then one segment top to bottom (or the other axis in the Top view), apply again.
- To cut layers out of a pillar, use top to bottom with several segments.

## FAQ

**The wheel zooms instead of changing the segments.** In the 2D views use `Shift` + wheel. In the 3D view the wheel belongs to the tool while it is active.

**Nothing happens when I click.** Select brushes first, and keep the mouse inside the object: a position on the edge gives no cut.

**Tab moves the keyboard focus.** `Tab` only toggles when no text field or button had the focus. Click in a view first, or press `O`.

## Related pages

- [Clip tool](clip-tool.md)
- [Brush operations](brush-operations.md)
- [Grids](grids.md)
- [Keyboard shortcuts](keyboard-shortcuts.md)

---
[Back to the documentation index](../README.md)
