[Documentation](../README.md) > Guides > Custom grids for stairs | [Français](../../fr/guides/custom-grids-for-stairs.md) | [Русский](../../ru/guides/custom-grids-for-stairs.md)

# Custom grids for stairs

Hammer has one grid, the same step on every axis. Stairs rarely fit it: a common step is 8 units high and 12 deep, and a staircase is often turned with its building. Hammer Pro's custom grids have their own step per axis, an origin, an orientation and optional limits, so every step snaps exactly. This guide builds a staircase with one.

## Step 1: create the grid

1. Open **Map > Grids > Grid manager...**. The **Grids** window opens.
2. In **Custom grids**, click **New grid**. It starts from the size of the global grid.
3. Fill in the fields:

| Field | For the stairs | Notes |
|---|---|---|
| **Name** | `Stairs` | Unique in the map. |
| **Step X Y Z** | `12`, `64`, `8` | Any size, decimals allowed. Equal steps change together: hold `Shift` while you change one to set it alone. |
| **Origin** | the bottom corner of the first step | A node of the grid lies there. |
| **Pitch Yaw Roll** | `0 0 0` (or the yaw of a turned building) | Orientation of the grid, like entity angles. |
| **Major line every** | e.g. `4` | 0: the setting of the views. |
| **Limits** | the stairwell box | Optional: the grid applies only inside this box, the global grid outside. **Fit to the selection** takes the selection bounds. |
| **Colour** | any | Optional colour of its lines. |

The grid is **saved with the map** (a `hammerpro_grids` block in the VMF). **Save as preset** copies it to the settings so every map offers it; **Copy to the map** does the reverse.

## Step 2: make it the active grid

Click **Use** in the grid manager, or choose **Map > Grids > Custom grid**. The **Active grid:** row at the top of the window switches between **Global grid**, **One grid per view**, **Custom grid** and **Grid of the selection**. The grid button of the toolbar opens the same choices.

With the custom grid active, the 2D views draw its lines, and **Draw the custom grid in the 3D view** draws it in the 3D view too. `[` and `]` scale every step of the custom grid together.

## Step 3: draw the steps

1. Take the block tool (`Shift+B`).
2. In the Side or Front view (the one that shows the depth and the height of the stairs), draw the first step: its corners snap to 12 units along X and 8 units along Z.
3. Press `Enter` to create it.
4. Clone it: with the selection tool, drag it while holding `Shift` (clone drag) one step forward and one step up; the copy snaps to the grid. **Edit > Paste special...** (`Ctrl+Shift+V`) also pastes several copies with an offset.
5. Repeat, or use an array (see [Transform and align](../features/transform-align.md)).

For a turned staircase, set the **Yaw** of the grid to the angle of the building: snapping, moves and the grid lines follow its axes. A **Pitch** follows a slope, for example to place railings along the stairs.

## Step 4: keep the grid with the stairs

Select the finished stairs and click **Attach to the selection** (or **Map > Grids > Attach the grid to the selection**). With **Objects use their grid** on (the default), editing these objects or drawing on them snaps to their grid whatever the active grid is. **Grid of the selection** makes the grid of the selected objects the active grid. **Map > Grids > Detach the grid of the selection** removes the link. The grid manager shows how many objects or groups each map grid is attached to.

## Shortcuts to a grid

- **Map > Grids > Grid from selection**: a grid fitted to the selection (for a point entity: its origin and angles; for brushes: the axes of the first solid, with a node on a corner of its bounds), with the step of the global grid. Select an existing stair step, run it, then set the **Step X Y Z** in the manager.
- **Map > Grids > Grid from face**: a grid on the selected face (texture tool): its plane is the face, X along its longest edge. Handy for a ramp or a sloped ceiling.

Both create a new custom grid and make it active.

## FAQ

**My blocks are not rotated with the grid.** Drawn boxes stay aligned with the world axes: with a rotated grid, each world axis takes the step of the grid axis closest to it. Build the stairs aligned with the world, then rotate the finished staircase.

**The rest of the map snaps strangely.** Set **Limits** on the grid so it only applies in the stairwell, or go back to **Global grid**.

## Related pages

- [Grids](../features/grids.md)
- [Block tool](../features/block-tool.md)
- [Transform and align](../features/transform-align.md)
- [Clipboard](../features/clipboard.md)
- [Settings reference](../reference/settings.md)

---
[Back to the documentation index](../README.md)
