[Documentation](../README.md) > Features > Placement on real shapes | [Français](../../fr/features/real-shape-placement.md) | [Русский](../../ru/features/real-shape-placement.md)

# Placement on real shapes

Hammer Pro measures props, brushes, displacements and instances on their real geometry: the triangles of the `.mdl` mesh after its angles, `modelscale` and body groups, the brush planes, the displaced surfaces. Nothing uses a bounding box when the real shape is known. The same measures were already used by the AI tools of the [MCP server](../reference/mcp-server.md) (`place_object`, `check_placement`, `find_floating_objects`, `get_object_shape`, `render_shape`) and by **Move to floor**; this page describes how to use them by hand.

## Placement commands

They are in **Tools > Placement and props** (first group), in the right click menu of the views (**Place on real shapes** submenu, under **Move to floor**) and in the command palette (`Ctrl+P`).

| Command | Shortcut | What it does |
| --- | --- | --- |
| **Place on top of target** | `Alt+T` | Select the objects to move, then the target last (`Ctrl+click`). The objects keep their layout, are centered over the target and dropped onto it. |
| **Place against wall** | `Alt+L` | Slides each selected object until it touches the nearest surface in the direction of the view (see below), flush, without turning it. |
| **Place against wall, facing away** | `Alt+Shift+L` | The same, but first turns each prop so its front (the model's +X axis) faces away from the wall, like a shelf or a vending machine. |
| **Place next to target** | `Alt+X` | Puts the objects beside the target (selected last) on the side they come from, touching it without overlapping, then drops them onto the floor. |
| **Settle floating objects** | `Alt+Shift+N` | Drops the selected objects that float and lifts the ones that sink into the floor (16 units at most). With nothing selected it settles every floating or sunk prop of the map, except the props of the 3D skybox and the hanging props (see below). Stacks settle together: a crate on a floating crate follows it down. |
| **Fit to ground (supports)** | `Alt+G` | For tables, chairs, vehicles: drops each object and tilts it (up to 30 degrees) so that its lowest points (legs, wheels) all touch the uneven ground. |
| **Shape preview** | `Alt+Shift+M` | Opens the shape preview window (see below). |

Direction of **Place against wall**, in this order:

1. the surface you right clicked in the 3D view, when the command comes from the right click menu;
2. a selected face (select the wall face with the texture tool, then the objects);
3. the view under the mouse: in the 3D view the direction the camera looks at (kept horizontal), in the front view toward -X, in the side view toward +Y;
4. in the top view or without a view, the nearest wall of the four horizontal directions.

Every command is one undo step and says what it did in a message at the bottom right, for example *Moved 3 objects, gap 0*. The gap is the distance left under the objects (0 means they touch). A warning follows when the result still overlaps another object or when an object landed on something else than the target.

## Real dimensions

**View > Display options > Real dimensions** (`Alt+M`, also in the menu of the entity properties panel) shows, for the selection:

- in the status bar: the real size **width × depth × height** of the object (the footprint is the smallest rectangle around the mesh seen from above, so a rotated prop gives its own length and width), then its gap to the floor (*floats 12 above the floor*, *sunk 3 into the floor*, *rests on the floor*) and the overlap depth with another object. Its tooltip lists every selected object; a click opens the shape preview;
- in the entity properties panel, the same line under the class name, with a **Shape preview** button;
- in the 2D and 3D views: the real footprint outline at the bottom of the object (cyan), its smallest rectangle, the lowest contact points (green crosses), a line from the bottom down to the floor when the object floats (orange) or up to the floor when it sinks (red), and a label above it.

Up to 16 objects are measured (the size covers the whole selection). The values follow edits a moment after the selection stops moving.

## Shape preview

**Tools > Placement and props > Shape preview** (`Alt+Shift+M`) draws the real shape of the selection from the top, the front, the side and in 3D, with the dimensions, the origin, the lowest point, the attachment points and the collision mesh (cyan wire) when the model has one. **Selection** draws the selected objects; in the [model browser](model-browser.md), the **Shape preview** button of a model draws that model before you place it (**Model:** tab). **Refresh** draws again.

## Check for problems

[Check for problems](check-problems.md) lists two more kinds of problems, found in the background like the others, and a third one when you turn it on:

| Problem | Severity | Found when | Fix |
| --- | --- | --- | --- |
| **floating object** | warning | a prop floats more than 0.5 unit above the surface below it and touches nothing on its sides or above (lamps on a wall, signs under a ceiling are not floating) | drops it onto the surface |
| **sunk object** | information | a prop sinks more than 0.5 unit into the surface it stands on | lifts it so it rests on the surface (when it sinks 16 units or less) |
| **overlapping objects** | information | only with **Also list props that overlap other objects** on: a prop goes more than 1 unit into another prop, a brush or a brush entity (the floor it stands on is the sunk case) | pushes it out by the shortest way (16 units at most); for two props the smaller one moves |

Only props are checked (entities whose `model` key is a `.mdl`, without brushes and without a `parentname`), on their render mesh. The editor models of point entities (`info_node`, `info_player_start`...) are neither checked nor used as surfaces. **Go to** selects the object (and the other object of an overlap). **Fix** moves it by the measured distance, **Fix all** fixes every listed placement problem together with the others in one undo step, and stacked props move together. A problem whose object moved since the check is not fixed again (the next check lists it with its new distance).

Left out of the check (the MCP tools `find_floating_objects` and `check_placement` still report them):

- the props of the 3D skybox: the room around each `sky_camera`, found by looking from the camera in the six directions until a brush is hit (a camera in an open area excludes nothing);
- the props meant to hang, for floating: a model whose file name contains `lamp`, `light`, `chain`, `hanging`, `sign`, `wire` or `cable`, a model with an attachment point in the top 15% of its height (a hook), or a prop that comes within 1 unit of a wall or a ceiling.

The check runs on worker threads a moment after each change; the main thread only prepares it in slices of a few milliseconds. It is turned off by **Check for floating and sunk props** in **Settings > Map health** or **Settings > Prefabs** (`placement.problem_checks`, on by default). The overlaps are off by default because mappers often clip props into each other on purpose and the check is slower on big maps: turn on **Also list props that overlap other objects** under it (`placement.overlap_check`).

## New props onto the surface

**Drop new props onto the surface below** (**Settings > Prefabs**, `placement.auto_settle_on_create`, off by default): a prop created with the entity tool, dropped from the model browser, or created by an AI through MCP (`create_entity`, `create_entity_fgd`) falls onto what is under it, measured on its real mesh. The drop belongs to the same undo step as the creation.

## For the AI (MCP)

- The commands are actions: `run_action` with `place_on_top`, `place_against_wall`, `place_against_wall_away`, `place_next_to`, `settle_floating`, `fit_to_ground`, `show_shape_preview`, `toggle_real_dimensions`.
- The settings are `placement.problem_checks`, `placement.overlap_check`, `placement.auto_settle_on_create` and `placement.show_real_dimensions` (`get_settings`, `set_setting`).
- `check_problems`, `get_problems` and `fix_problems` list and fix the codes `floating_object`, `sunk_object` and `overlapping_objects`.
- The tools that create or move objects (`create_entity`, `create_entity_fgd`, `move_objects`, `transform_objects`, `rotate_objects`, `resize_objects`, `flip_objects`, `align_objects`, `random_rotate_objects`, `snap_objects_to_grid`, `set_entity_keyvalues`, `set_keyvalue`, `paste_vmf_text`, `paste_clipboard`, `paste_special`, `duplicate_objects`, `array_duplicate`, `sprinkle`, `insert_prefab`, `import_models_batch`, `create_brush`, `create_primitive`) check the props and brushes they created or moved (32 at most) and add a `placement_note` to their result when one floats more than 0.5 unit, sinks into the floor or overlaps another object, naming the fix (`drop_to_surface`, `place_object`). When `placement.auto_settle_on_create` is on, `create_entity` and `create_entity_fgd` add `auto_settled` with the number of props dropped.

## Settings

| Setting | Key | Default |
| --- | --- | --- |
| Check for floating and sunk props | `placement.problem_checks` | on |
| Also list props that overlap other objects | `placement.overlap_check` | off |
| Drop new props onto the surface below | `placement.auto_settle_on_create` | off |
| Show the real dimensions of the selection | `placement.show_real_dimensions` | off |
