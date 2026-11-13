[Documentation](../README.md) > Features > Spline tool | [Français](../../fr/features/spline-tool.md) | [Русский](../../ru/features/spline-tool.md)

# Spline tool

The spline tool draws an editable 3D path and generates objects along it: fences and rails, cables, pipes, curbs and trims made of brushes, and lines of props. The spline is kept in the map, so you can select it later, move its points and the objects follow. Nothing is generated until you ask for it, and every generation is one undo step.

## Opening it

- **Tools > Curves and roads > Spline tool**, or its button in the tool palette (creation group, after the pipe tool). It has no default shortcut.
- **Tools > Curves and roads > Edit spline** opens the tool on the selected spline (select its anchor or any object generated along it). Opening the tool while a spline is selected does the same.
- **Tools > Curves and roads > Bake spline to plain objects** removes the spline and keeps its objects as ordinary objects.

## Drawing the path

1. **Click** in a 2D view to add a point (it snaps to the grid; its depth is the one of the previous point). In the 3D view a click puts the point on the surface under the cursor (brushes and displacements), with X and Y snapped to the grid when snapping is on.
2. New points go after the selected point (before it when the first point is selected), so you can extend either end or add points in the middle.
3. **Drag** a point to move it (in 3D it slides on the surfaces). **Ctrl + click** on the curve inserts a point there. `Delete` removes the selected point, `Backspace` the last one of a new spline, `Escape` deselects, then cancels.
4. `Shift` + wheel changes the **roll** of the selected point (5 degrees per notch), `Alt` + wheel its **width** (0.1 per notch).
5. `Enter`, a double click or **Generate** creates the spline and its objects. The right click menu has Insert a point here, Delete point, Close or Open the loop, Automatic handles, Generate, Bake and New spline.
6. After generating, the tool keeps editing that spline (changes regenerate it); **New spline** starts another one.

The preview shows the curve and what will be generated (wire brushes, boxes of the models with their real size, sagging cables) in every view, with a label giving the length and the counts.

## Path options

| Option | What it does |
|---|---|
| **Catmull-Rom** / **Bezier** | Catmull-Rom passes through every point. Bezier gives each selected point two handles to drag; **Automatic handles** puts them back to the smooth default. |
| **Closed loop** | Joins the last point to the first (3 points or more). |
| **Follow terrain** and its height | The path hugs the ground under it between the points (brushes and displacements, not the spline's own objects), at that height above it. |
| Point **Roll** | Banks the path at this point in degrees: the profile of a sweep, rails and fence models tilt with it. Interpolated between points. |
| Point **Width** | Scales the profile, the cable radius and the side offset at this point. Interpolated between points. |
| **Offset** x, z | Moves the generated objects sideways (positive to the right of the path) and up. |
| 🔄 auto regenerate | When you edit a spline that already exists, its objects are regenerated a moment after each change (one undo step per change). Off: use **Apply**. |

## Modes

**Spline only** keeps the path alone (useful to prepare it, or to keep a guide).

**Fence and rails**
- *Model fence*: a model repeated along the path. **Spacing** 0 uses the length of the model (along its longest horizontal axis, or the **axis** you choose); **Fit to length** stretches the spacing so the segments end exactly on the end of the path. Each segment goes from one point of the path to the next, so it follows the curve like a real fence. **Post model** adds posts **at each point** of the spline and/or **between segments**; **Start cap** and **End cap** go on both ends. **Follow the slope** tilts the segments along the slope of the ground. The class is prop_static by default.
- *Brush rails*: posts and rails made of brushes (count, post height and size, rail size, post spacing), func_detail by default.

**Cable**
- *Rope entities*: a move_rope at the first point and keyframe_rope entities after it, linked by name, with **Slack**, width, material and subdivision. A closed loop links the last rope back to the first. **Point every** places a rope point every N units instead of at the spline points.
- *Brush cable*: a round brush tube hanging between the points with the same sag as the slack.

**Pipe, curb, trim**: brushes swept along the path from a 2D profile: *Round pipe* (sides, radius), *Curb* (width, height, bevelled top), *Trim* (an L molding), *Rail tube* (a round tube raised to Height, for handrails) or *Custom* (a convex profile typed as `x:y` pairs, scaled by Width / 16). **Segment** is the length of one brush along the path. The texture runs on along the whole length with no seam between brushes. The brushes go in one func_detail by default.

**Prop line**: models placed every **Spacing** units (0: from the model size), on the path, on one side or both sides (with the offset as the distance), with a **random offset**, a **random rotation**, a random **scale** between two values (written to modelscale) and a **seed** so the same settings give the same line. **Turn with the path** orients them along it. **Drop onto surfaces** puts each prop on the surface under it with its real mesh, and **Avoid overlaps** skips the props that would cross another prop or object, measured on their real shapes (the console says how many were skipped).

## How the spline is stored

The generated objects and a hidden anchor (an `info_null`, removed by the game when the map starts) are grouped together. The anchor keeps the points and every option in its editor data, and each generated object is tagged with the spline number, so the spline stays editable after saving and reopening the map. Moving or rotating the group moves the spline with it. A copy of a spline group is a separate spline: editing one does not change the other.

Editing replaces the generated objects. **Bake** removes the anchor and the tags in one undo step: the objects become ordinary objects and stay grouped.

## MCP

- `create_spline` builds a spline from `points` (`[x, y, z]` or `{position, roll, width, handle_in, handle_out}`), with `mode`, `interp`, `closed`, `follow_terrain`, `terrain_offset`, every mode option in `options` and `preview`.
- `edit_spline` changes a spline (by its anchor or any generated object, default the selection): all `points`, `insert_at` with `insert_position`, `delete_point`, one `point` with `position`, `roll`, `width`, `handle_in`, `handle_out`, `auto_handles`, the settings and `options`; `regenerate` false only stores the new spline.
- `generate_along_spline` generates with a `mode` and `options`, or describes the result with `preview`.
- `bake_spline` turns the objects into plain objects (`keep_group`).
- `get_spline` lists the splines, or describes one with `id`.
- `set_tool_options` with tool `spline` sets the tool options: every option of `options` above, `auto_regenerate`, `keep_active`, `selected_point` (from 1), `point_x`, `point_y`, `point_z`, `point_roll`, `point_width` and `point_auto_handles`. The actions `tool_spline`, `spline_edit` and `spline_bake` work through `run_action`.

The option names are: mode, interp, closed, follow_terrain, terrain_offset, material, tex_scale, detail, seed, align_slope, offset_x, offset_z, profile, sides, radius, width, height, segment, custom, fence, model, spacing, fit_spacing, model_axis, post_model, posts_at_points, post_every_segment, start_cap, end_cap, prop_class, rail_count, post_height, post_size, rail_size, post_spacing, cable, slack, rope_width, rope_material, subdiv, rope_every, models, side, jitter, yaw_jitter, face_path, scale_min, scale_max, drop, avoid_overlap. See the [MCP tool reference](../../MCP.md).

## Tips

- For a fence on uneven ground, turn on **Follow terrain** and **Follow the slope**.
- Keep **Segment** short on tight curves: each brush is straight.
- A spline in mode *Spline only* is a handy guide to try several modes with `generate_along_spline` and `preview`.
