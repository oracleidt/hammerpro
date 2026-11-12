[Documentation](../README.md) > Features > Transform and align | [Français](../../fr/features/transform-align.md) | [Русский](../../ru/features/transform-align.md)

# Transform and align

Commands that move, rotate, scale, mirror, line up or drop the selected objects by exact rules instead of dragging handles: the Transform dialog, Align, Flip, Snap to grid, Move to floor/ceiling, Center origins, Random Z rotation, the movable pivot, physics placement and arrays (Paste special).

Every command below works on the current selection and is one undo step. Most of them are in the **Tools** menu (Align, Flip, Move to floor, the origins and the random rotation share the submenu **Tools > Arrange**). To drag objects with the mouse, see the [Selection tool](selection-tool.md).

## Command summary

| Command | Menu | Default shortcut |
|---|---|---|
| **Transform...** | Tools | `Ctrl+M` |
| **Snap selection to grid** | Tools | `Ctrl+B` |
| **Snap each object to grid** | Tools | `Ctrl+Shift+B` |
| **Move to floor** | Tools | `Alt+N` |
| **Move to ceiling** | Tools | `Alt+U` |
| **Move to floor, aligned to the surface** | Tools | `Ctrl+Alt+N` |
| **Move to ceiling, aligned to the surface** | Tools | `Ctrl+Alt+U` |
| **Center origins** | Tools | none |
| **Random Z rotation** | Tools, and the right click menu of the views | none |
| **Random Z rotation options...** | Tools (opens the Transform window in **Random Z** mode) | none |
| **Movable pivot** | Tools | none |
| **Physics placement (drop and settle)** | Tools | none |
| **Flip X**, **Flip Y**, **Flip Z** | Tools > Arrange | none |
| **Flip horizontally** | Tools > Arrange | `Ctrl+L` |
| **Flip vertically** | Tools > Arrange | none (`Ctrl+I` with the Hammer and Hammer++ key presets) |
| **Align left**, **Align right**, **Align top**, **Align bottom** | Tools > Arrange | none |
| **Align centers horizontally**, **Align centers vertically** | Tools > Arrange | none |
| **Paste special...** | Edit | `Ctrl+Shift+V` |
| **Texture lock** | Tools | `Shift+L` |
| **Texture scale lock** | Tools | `Shift+K` |

Commands without a shortcut can get one in **Settings > Keyboard** (see [Keyboard shortcuts](keyboard-shortcuts.md)) or be run from the [command palette](command-palette.md) (`Ctrl+P`).

## Transform dialog

**Tools > Transform...** (`Ctrl+M`) opens the **Transform** window: move, rotate or scale the selection by typed values. The window keeps its values for the whole session, so you can apply the same transform several times.

| Field | What it does | Default |
|---|---|---|
| **Move** / **Rotate** / **Scale** / **Random Z** | Mode buttons at the top. **Random Z** is described in [Random Z rotation](#random-z-rotation). | **Move** |
| **Offset** (Move) | X, Y, Z distance in units added to the selection. | 0 0 0 |
| **Angles (degrees)** (Rotate) | Rotation around X, Y and Z, applied Z first, then Y, then X (the same order as Paste special). | 0 0 0 |
| **Factors** (Scale) | Scale factor on X, Y and Z (drag step 0.01). A factor of 0 is refused (**Scale factors cannot be zero**). Negative factors mirror. | 1 1 1 |
| **Pivot** (Rotate and Scale only) | **Selection center**: center of the selection bounds. **World origin**: the point 0 0 0. **Movable pivot**: the pivot cross (see below); the button is greyed out while the pivot is hidden, and its tooltip gives the pivot position. | **Selection center** |
| **Texture lock** | Shows whether texture lock is enabled; **Toggle** switches it (same as `Shift+L`). | follows the setting |
| **Reset** | Puts the values of the current mode back to their default (0 or 1). | |
| **Apply** | Applies the transform (also `Enter`). Greyed out with nothing selected. | |
| **Close** | Closes the window (also `Esc`). | |

The line under the fields counts the selected objects. Errors (nothing selected, invalid values, transform failed) show in red in the window.

The cursor starts in the first field (X) when the window opens and when you change mode, with its text selected so you can type over it. `Tab` and `Shift+Tab` move between the fields and buttons of the window, wrapping around, and never reach the shortcuts of the views (such as `Tab`, which cycles the 2D view).

Rotating a mixed selection turns everything around the same pivot in the same direction, like Hammer: brushes, props, point entities and lights. For `light_spot`, `light_environment` and `light_directional` the light keeps shining the way the rotation turned it: the **Pitch** key gets the new pitch (positive is up, the way VRAD reads it) and the angles are written the Hammer way, so the spotlight model points along the beam. The rotate handles, the gizmo, Paste special and the MCP tools `rotate_objects` and `transform_objects` turn objects the same way.

## Snap to grid

- **Snap selection to grid** (`Ctrl+B`): moves the selection as a whole so the minimum corner of its bounds lands on the nearest point of the current grid. In the Selection, Vertex and Bevel tools, `Ctrl+B` with the cursor on a brush edge in the 3D view bevels that edge instead (see [Bevel tool](bevel-tool.md)).
- **Snap each object to grid** (`Ctrl+Shift+B`): does the same for every selected object on its own.

The grid size is the current grid size (see [Grids](grids.md)).

## Move to floor and ceiling

Hammer++ style drop:

- **Move to floor** (`Alt+N`) moves each object straight down until its real shape touches the real surface below: the lowest point of a prop's model mesh (after its angles and scale) lands exactly on brushes, displacements and other props' meshes, with no gap and no overlap. It is the same engine as the MCP `drop_to_surface` tool.
- **Move to ceiling** (`Alt+U`) moves it straight up.
- The **aligned to the surface** variants (`Ctrl+Alt+N`, `Ctrl+Alt+U`) also turn point entities so their up axis follows the normal of the surface they land on, keeping their heading as much as possible.

Details:

- Each group (its outermost group) or ungrouped object moves on its own.
- An object sunk into the floor is lifted out onto it instead of falling through. Invisible tool brushes (clip, trigger, hint...) are not surfaces.
- Brushes and props (entities with a `.mdl` model) are surfaces. The moved objects themselves are ignored.
- An object with nothing below (or above) it stays in place. When nothing moves, a message says **There is no surface below the selection** (or above).
- More commands on the real shapes (place on top of another object, against a wall, next to it, settle floating props, fit legs to the ground) are in **Tools > Placement and props**: see [Placement on real shapes](real-shape-placement.md).

## Physics placement

**Tools > Placement and props > Physics placement (drop and settle)** drops the selected objects one after another, lowest first, so each one rests on the ground or on the objects already placed instead of passing through them, like objects falling under gravity. A group falls as a whole. Props are tilted onto the surface they land on. Contacts use the real meshes of the props and the real brush and displacement shapes. When everything already rests on a surface, the notice **Nothing to drop: the selection is already resting on a surface.** shows.

## Flip

Flip mirrors the selection across a plane that goes through the movable pivot when it is shown, else through the center of the selection.

- **Flip X**, **Flip Y**, **Flip Z** mirror along that world axis.
- **Flip horizontally** (`Ctrl+L`) and **Flip vertically** depend on the view under the mouse pointer (else the last view used). In a 2D view they mirror along the horizontal or vertical axis of that view. In the 3D view, horizontally mirrors along the world axis (X or Y) closest to the camera's right direction, and vertically along Z.

Brushes keep outward faces and their displacements are mirrored with them. Texture lock applies.

## Align

Hammer style align: the selected objects line up with the **last selected object**, which does not move. At least two objects are needed (**Select at least two objects**).

| Command | Lines up |
|---|---|
| **Align left** | the minimum of the bounds on the horizontal axis of the view |
| **Align right** | the maximum on the horizontal axis |
| **Align top** | the maximum on the vertical axis |
| **Align bottom** | the minimum on the vertical axis |
| **Align centers horizontally** | the centers on the horizontal axis |
| **Align centers vertically** | the centers on the vertical axis |

The axes are those of the 2D view under the pointer (or the last one used). From the 3D view, the Top view axes are used. The other objects move together as one block.

## Center origins

**Tools > Arrange > Center origins** sets the `origin` key of the selected brush entities (`func_door_rotating`, `func_rotating`...) to the center of their brushes. Only brush entities that have an `origin` key change; selecting one of their brushes is enough. If none changed, the notice **No selected brush entity has an origin to center** shows.

## Random Z rotation

**Tools > Arrange > Random Z rotation** turns every selected point entity that has an `angles` key (`prop_static`, `prop_dynamic`, `prop_physics`, lights, any point entity with angles) around Z by its own random amount, around its own origin. It is one undo step (**Random rotation**). Brushes, brush entities and entities without an `angles` key are skipped; the notice says how many turned and how many were skipped, for example **Random Z rotation: 12 turned, 3 skipped (brushes, brush entities or no angles key)**.

The options are in the **Random Z** mode of the Transform window (**Tools > Arrange > Random Z rotation options...**), kept in the settings:

| Field | What it does | Default |
|---|---|---|
| **Yaw range** | The turn added to each object's yaw is picked between these two values, in degrees. | 0 to 360 |
| **Snap step** | **Free**, **15°**, **45°**, **90°** or any step up to 180: the turn is a multiple of the step (with 0 to 360 and 90, each object faces one of four directions). | Free |
| **Pitch and roll jitter** | Also tilts pitch and roll by a random amount up to this many degrees (0 to 45). | off, 3° |
| **Reset** | Puts these options back to their defaults. | |
| **Apply** | Turns the selection (also `Enter`); the window says how many turned. | |

The options are the `random_yaw_min`, `random_yaw_max`, `random_yaw_snap`, `random_tilt` and `random_tilt_deg` keys of the `[selection]` table (see [Settings reference](../reference/settings.md)).

## Movable pivot

**Tools > Arrange > Movable pivot** shows a pivot cross at the center of the selection (at the origin of a single point entity), or hides it when it is shown.

- With the [Selection tool](selection-tool.md), drag the cross in a 2D view to move it (it snaps to the grid when grid snapping is on). In the 3D view the gizmo sits on the pivot: grab it near the center of the gizmo.
- Rotations and scales of the handles, gizmo and modal transforms turn around it. Flip mirrors through it. The Transform dialog uses it with **Pivot > Movable pivot**, and Paste special with **Rotate around the pivot**.
- The pivot moves with the objects when you move them. It is dropped when the selected set changes (like Hammer++).

## Arrays (Paste special)

**Edit > Paste special...** (`Ctrl+Shift+V`) pastes several copies of the clipboard, each one moved and rotated a bit more than the previous one: stairs, columns, a circle of objects. The fields are **Copies** (up to 10,000, with a warning above 1,000), **Offset per copy** (default 64 0 0, with **<** buttons that take the clipboard size on an axis), **Rotation per copy**, **Rotate around the pivot**, **Start at the original position**, **Group each copy**, **Make entity names unique** and **Name prefix**. The full description is on the [Cut, copy and paste](clipboard.md) page.

## Texture lock and texture scale lock

- **Texture lock** (`Shift+L`, setting `texture_lock`, default on): textures follow brushes when they are moved, rotated, scaled or flipped. Off, textures stay fixed in the world.
- **Texture scale lock** (`Shift+K`, setting `texture_scale_lock`, default off): scaling a brush also scales its texture. Only used with texture lock on; without it the texture keeps its scale.

Both are also in **Settings > Editing > Behavior** and are saved in the `[edit]` table of `settings.toml`.

## MCP

The same operations exist for AI assistants: `transform_objects`, `move_objects`, `rotate_objects`, `resize_objects`, `snap_objects_to_grid`, `drop_to_surface`, `flip_objects`, `align_objects`, `center_entity_origins`, `random_rotate_objects` (Random Z rotation, with its range, snap, tilt and seed), `array_duplicate` and `run_action` (for example `physics_placement` or `toggle_pivot`). See the [MCP tool reference](../../MCP.md).

## Tips

- To rotate a door around its hinge, show the **Movable pivot**, drag it onto the hinge, then rotate with the handles or with **Transform...** set to **Movable pivot**.
- Align uses the last selected object as reference: Ctrl+click the reference last.
- **Move to floor** on a group of props moves the whole group as one, so a table with its objects stays assembled.
- **Snap each object to grid** fixes many off-grid props at once without moving them all by the same amount.
- After placing a row of identical rocks, crates or trees, **Random Z rotation** makes them look less copied; set **Snap step** to 90° for crates that should stay square to the walls.

## FAQ

**Why is Movable pivot greyed out in the Transform dialog?** The pivot is hidden. Use **Tools > Arrange > Movable pivot** first.

**My pivot disappeared.** It is dropped when the selection changes. Show it again after selecting.

**Align does nothing.** Select at least two objects, and check that the reference (the last selected object) is not already lined up with the others on that axis.

**Move to floor did not move one prop.** There was nothing below it. Only brushes and props (entities with a `.mdl` model) count as surfaces; other point entities do not.

## Related pages

- [Selection tool](selection-tool.md)
- [Cut, copy and paste](clipboard.md)
- [Brush operations](brush-operations.md)
- [Grids](grids.md)
- [Groups](groups.md)
- [Texture application](texture-tool.md)
- [Actions and shortcuts](../reference/actions-shortcuts.md)
- [Settings reference](../reference/settings.md)

---
[Back to the documentation index](../README.md)
