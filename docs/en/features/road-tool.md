[Documentation](../README.md) > Features > Road tool | [Français](../../fr/features/road-tool.md) | [Русский](../../ru/features/road-tool.md)

# Road tool

The road tool builds a whole road along a spline: lanes of a chosen width, shoulders, a lane material, center, lane and edge lines (solid, dashed or double) with their own material, banking, a smooth rise and fall along the points, and optionally a road that follows the ground. The road is made of convex brushes, one per lane, line and shoulder in every segment, with the texture running along the road without seams. You can reopen a road later and change anything, cut it in two, and join the end of one road to the side of another with rounded flares.

It is built on the [curve tool](curve-tool.md): same smooth spline, same way of remembering the road in the `hammerpro_curve` keys of the brushes (Hammer and Hammer++ ignore them and keep them), same mitred joints with no gap and no overlap.

## Opening it

- **Tools > Road tool** or the tool palette (creation group, after the sprinkle tool). It has no default shortcut; you can give it one in **Settings > Keyboard**.
- **Tools > Edit road** reopens the road of the selected brush (see below). **Tools > Edit curve** does the same when the selected brush belongs to a road.
- The command palette (`Ctrl+P`) has both commands.

## Drawing the road

1. Click points one after the other in the **top view** or in the **3D view**. In the top view the point snaps to the grid (hold `Alt` for no snapping) and takes the height of the previous point, or the height of the ground when **Follow terrain** is on. In the 3D view the point is placed on the surface under the cursor, or on the plane of the previous point when there is no surface.
2. The preview is live and textured in 3D, with the smooth centerline and white control points. It uses the same materials, lane markings, width and height as the generated brushes. In 2D the lane edges and centerline remain visible. No brushes are changed until you confirm.
3. **Click a point** to select it. Its red X, green Y and blue Z arrows appear in 3D. **Drag the blue arrow up to make the road climb**, or down to lower it. The red and green arrows move it sideways; the squares between arrows constrain movement to a plane. A direct point drag moves it horizontally. `Ctrl+drag` on a point also changes only its height. `Alt` disables snapping. Front and side view drags still change height. Manual height edits turn **Follow terrain** off so the ground cannot override the slope.
4. **Shift+drag an endpoint** or its arrow to extend the road from that end, leaving the old point in place. The options bar also has editable **X, Y and Z** coordinates. `Ctrl+click` a point removes it; `Shift+click` in the top or 3D view inserts a point on the existing spline. `Backspace` removes the last point, `Delete` removes the selected one, and **Reverse** changes the end used by new clicks. A road holds at most 512 points.
5. `Enter`, a double click, or the **Create** button builds the road. `Escape` cancels. The new brushes are selected and the whole road is one undo step.

Heights are smooth: between points the road rises and falls along a curve that goes through every point height without overshooting (a hill between two flat parts never gets a bump higher than its points). Slopes are measured on the road itself, so the texture is not stretched on a slope.

With mouse look enabled, a left click places a point at the crosshair. Leave mouse look to use the arrows. **New road** clears the pending path and starts another road. It cancels unapplied changes when editing an existing road.

Press `Ctrl+Z` while drawing or editing to undo the last point, drag or setting change. `Ctrl+Y` restores it. An entire drag, including Shift endpoint extrusion, is one step. Local undo never changes other map objects. **Apply** or **Create** commits the whole road as one normal map undo step; cancelling discards the pending edits.

## Road settings

A single lane can be wider than the old 256-unit limit. For a simple wide surface, set one lane, no shoulder and no center, edge or divider lines: the tool builds one road brush per longitudinal segment. Widening that lane does not add another lane or extra surface bands. The old 2048-unit total-width cap is removed; generated corners still have to fit within Source map coordinates.

The options bar holds the lanes, lane width, shoulder, **Follow terrain**, the material of the new road, **Create as** (world brush, `func_detail` or any brush entity class: all the brushes go into one entity), the pin and the buttons. **Road settings** opens the window with everything else.

| Setting | What it does | Range | Default |
|---|---|---|---|
| **Lanes** | Number of lanes. | 1 to 8 | 2 |
| **Lane width** | Width of one lane. | 16 to 32768 units, within the map | 128 |
| **Shoulder** | Width of the shoulder on each side of the lanes, 0 for none. | 0, or 1 to 256 | 16 |
| **Total width** | Lanes plus both shoulders (information, bounded by the Source world span). | | 288 |
| **Slab depth** | How far the brushes go down under the road surface. | 2 to 128 | 8 |
| **Segment length** | Target length of one segment. Shorter segments follow bends, slopes and terrain better but make more brushes (at most 8000 for a road). | 16 to 512 | 64 |
| **Texture scale** | Texture scale of the faces. | 0.01 to 64 | the editor default |
| **Road material** | Material of the lanes. The **Current** button takes the material of the texture browser. | | the current material |
| **Shoulder material** | Material of the shoulders; the `x` button makes it the same as the road. | | same as the road |
| **Line material** | Material of the lines and stripes. | | `dev/reflectivity_90` |
| **Center material** | Material of the center line; `x` makes it the same as the other lines. | | same as the lines |
| **Center line** | The line in the middle (roads with an even number of lanes). | none, solid, dashed, double | dashed |
| **Edge lines** | Lines on the outer edge of the lanes. | same | solid |
| **Lane lines** | Lines between the other lanes. | same | dashed |
| **Line width** | Width of a line. A double line is two lines with a gap of the same width. | 1 to 32 | 6 |
| **Dash length**, **Gap length** | Length of a dash and of the gap of the dashed lines. | 4 to 1024 | 96, 96 |
| **Roll start**, **Roll end** | Banking in degrees at the start and the end, changing evenly along the road. A positive angle raises the left side when you follow the road. | -60 to 60 | 0 |
| **Auto bank**, **Auto limit** | Banking proportional to the curvature, and its largest value, as in the [curve tool](curve-tool.md). | 0 to 90, 0 to 45 | 0, 8 |
| **Follow terrain** | See below. | on/off | off |
| **Terrain lift** | Height of the road above the ground when it follows the terrain. | -256 to 256 | 1 |
| **Start height** | Height of the first point when it is placed in a 2D view and there is no ground under it. | -16384 to 16384 | 0 |

The lines are thin convex brushes in the surface of the road, not overlays and not raised, so they never flicker against the road. A dashed line is one brush per segment that has the line material for the dash and the road material for the gap. Dashes follow the segments: their length is rounded to a whole number of segments (with 64 unit segments and 96 unit dashes, a dash is 2 segments). Lines must fit in the lane width: a lane at least as wide as a few lines.

## Junction and cul-de-sac presets

Choose **Shape** in the options bar or **Road settings**: **Free path**, **T junction**, **Four-way junction**, **Y junction**, or **Rounded cul-de-sac**. These are complete editable road pieces, with a live textured preview.

For a junction, place exactly two points: its center, then the end of one arm. Their direction rotates the entire junction; their distance sets every arm's length. A T has its stem toward the second point and its through road perpendicular to that stem. A Y has three arms 120 degrees apart. For a cul-de-sac, the first point is the road entrance and the second is the center of the turning bulb. The bulb extends beyond the second point.

Set **Radius** for rounded junction corners or the cul-de-sac bulb. The bulb radius must exceed half the combined lane width. If the preview says there is too little room, move the second point farther away or reduce the width or radius. **Segment length** controls arc detail and brush count. Shoulders and edge markings follow the perimeter and leave arm mouths open. Center and divider markings are omitted within presets.

The heights of the two points define a single graded plane. Terrain following, banking and automatic side-junctions must be off; **Use a planar surface** clears those options. Reopen a created shape with **Road tool** to move either handle or change its settings. Continue roads from the open arm ends manually. Automatic side-joining and splitting remain available for free paths.

## Following the terrain

With **Follow terrain** on, new points take the height of the ground under them (the highest brush or displacement surface found under the cursor, starting 1024 units above the previous height, else from the top of the map). The road also drapes every segment on the ground at its place, raised by **Terrain lift**, and the heights are smoothed so the road does not copy every bump. The ground is every brush and displacement except the brushes of the road itself, so editing a road never makes it climb on itself. Each time the road is rebuilt (edit, apply, a change of the terrain followed by an edit) it follows the ground again. With the option off, the heights of the points are the ones you set.

## Texture alignment

On the top face of every brush the texture runs along the road: the U axis follows the direction of the road (on the slope, so a texture is not stretched on a hill) and the V axis goes across. The U position is the distance along the road, so neighbouring segments continue each other exactly on the center line, with no jump at the joints, and the two halves of a split road keep the same run. On a bend the inside edge is a little shorter than the outside edge: the texture follows the center line, and the error stays small for the usual segment lengths. The sides and the bottom keep the normal world alignment.

## Saved road presets

In **Road settings**, enter a **Preset name** and choose **Save preset**. Save different road types under different names, then recall one from **Saved presets** in the tool bar or settings window. Presets are stored in the editor settings and remain available in other maps and after restarting. Change the settings and choose **Update preset** to replace that named preset, or **Delete preset** to remove it. An asterisk beside the name means the current settings differ from the saved version. Selecting the same preset again restores its saved settings.

A preset includes lane widths and count, materials, markings, segmentation, shape and radius, terrain following, banking, creation mode and keep-active mode. It leaves the current trace points, placement height and map connections unchanged. Loading a preset supports local `Ctrl+Z` and `Ctrl+Y`, including before placing a point. Saving or deleting a preset does not change any map brushes.

To recover settings from a road you already created, select one of its brushes and activate the Road tool, or use **Tools > Edit road**. Its stored points and settings are restored for editing. Save those settings as a named preset to use them for other roads.

## Editing an existing road

Every brush made by the road tool remembers its road in the `hammerpro_curve` keys of the VMF (`r1;...`, points with their heights, every setting, the junction).

1. Select one brush of the road (or its brush entity).
2. Activate **Road tool** to reopen the selected road directly, or choose **Tools > Edit road** to open it with the settings window.
3. Drag, add, insert, remove points, change heights, any setting, material, line, banking, junction.
4. `Enter` or **Apply** replaces all the brushes of the road by the new ones in one undo step. They stay in the same brush entity, if any, and are selected. `Escape` cancels without changing anything.

The settings of the tool in the options bar are put back after the edit. Copies of a whole road keep working. A brush you cut or move by hand keeps the data, but **Edit road** puts back the shape of the road.

## Splitting a road

In the editing window, select an inner point and press **Split at selected point**. The road becomes two roads that meet at that point, in one undo step. The cut faces are in the same plane (the two halves keep the direction, the slope and the banking they had at that point), the texture run goes on, and the first part keeps the road number (uid) while the second one gets a new one. With the MCP tool `split_road` you can also cut at a distance along the road or at a position; a point is added there on the road. Roads joined to the split road stay joined to the part they touch.

## Junctions

A junction joins the end of one road to the **side** of another road, like a street meeting a main road.

1. Edit the road whose end must join (the branch).
2. In the Road window, under **Junction**, choose the other road (listed as "Road N (brush M)"), which end of the branch is joined (start or end of this road), the **Attach to** choice (outer edge with shoulder, or lane edge) and the **Flare** length.
3. **Connect**, then **Apply**.

What is generated: the end of the branch is moved onto the outer edge of the other road (its shoulder included). With **Attach to: Lane edge**, the shoulder of the other road is cut away across the width of the branch and the branch road surface crosses it down to the lanes, so nothing overlaps. In both cases the end of the branch keeps the direction it had and is cut along that edge, so the two roads touch without gap and without overlap, even at an angle. A rounded flare, made of triangular convex brushes, fills each of the two corners between the branch and the side of the other road: it is a smooth quarter round of the chosen length that follows the real edge of the other road, with the surface at the right height, and it belongs to the branch. A flare of 0 gives a plain T. Flares are skipped on a corner that is too sharp (less than about 20 degrees).

The junction is remembered. When you edit or split the other road, the roads joined to it are rebuilt in the same undo step so they follow its new side; a joined road that can no longer reach it (for example the roads became parallel) is left as it was and the log says how many. **Disconnect** removes the junction. Refused: a road joined to itself, two roads joined to each other, roads nearly parallel, an end more than 2048 units from the other road, and an angle too shallow.

## When the road is refused

- **Segments fold inside a tight bend** (red): the half width exceeds the bend radius. Move the points farther apart, or click **Fit width to bend** to reduce lane, shoulder and marking widths together without moving the path. If the bend is still invalid at the minimum width, open the bend by moving its points.
- **Lines too wide**: the lines do not fit in the lanes. Lower the line width or widen the lanes.
- **Too short**, **too many brushes**, **wider than the Source world span**, **outside the map**, and the junction problems above.

## MCP

| Tool | What it does |
|---|---|
| `create_road` | Builds a road from `points` ([x, y] or [x, y, z]) and the settings above (`lanes`, `lane_width`, `shoulder`, `thickness`, `step`, `material`, `center_line`, `edge_line`, `divider_line`, `line_width`, `dash`, `gap`, `roll_start`, `roll_end`, `follow_terrain`, `lift`, `entity_class`...). Answers `road_uid` and the brush ids. |
| `get_road` | Reads a road (by `solid_id` of any of its brushes, `road_uid` or the selection): points with heights, settings, junction, brush ids, length, bounds, optionally the centerline `samples`. |
| `list_roads` | Lists every road of the map. |
| `edit_road` | Rebuilds a road with only the fields you give changed, in one undo step. Adding, removing, moving points or changing heights is done by sending the new `points` list. |
| `delete_road` | Deletes all the brushes of a road in one undo step. |
| `split_road` | Cuts a road in two with `point`, `at` or `distance`. |
| `add_road_junction` | Joins a road end (`end`) to the side of `target_uid` with a `flare` and an `attach` (`outer` or `lane`), or removes the junction with `remove`. |

`set_tool_options` with tool `road` sets the same settings for the next road drawn with the tool (`lanes`, `lane_width`, `shoulder`, `thickness`, `step`, `tex_scale`, `material`, `shoulder_material`, `line_material`, `center_material`, `center_line`, `edge_line`, `divider_line`, `line_width`, `dash`, `gap`, `roll_start`, `roll_end`, `auto_bank`, `auto_bank_limit`, `follow_terrain`, `lift`, `base_height`, `create_as`, `keep_active`). While a road is being drawn or edited, `selected_point` (numbered from 1) and `point_x`, `point_y`, `point_z` control its points. Setting `fit_width` to true fits the width to a tight bend. See the [MCP tool reference](../../MCP.md).

## Tips

- Draw the road with few points and let the spline do the rounding; add a point where the road must pass exactly.
- For a road on hills, switch on **Follow terrain** before the first click, draw in the top view, then fine tune the heights of single points in the front or side view.
- World roads automatically compile as detail geometry to prevent cracks and stray faces along curved or sloped segments. They remain world brushes, with the same brush count and collision. Existing roads receive this marking when saved or exported, including compilation. Keep structural ground underneath: detail roads do not seal the map. An explicit side `contents` override is preserved.
- Keep the segment length at 64 or more for long roads to keep the brush count and compile time low. **Create as** still supports `func_detail` and other brush entities.
- A road compiles as plain convex brushes; with banking the top of a segment can be slightly twisted, which the compiler splits into a few more faces.

## Related pages

- [Curve tool](curve-tool.md)
- [Road details](road-details.md)
- [Block tool](block-tool.md)
- [Displacement tool](displacement-tool.md)
- [Grids](grids.md)
- [Keyboard shortcuts](keyboard-shortcuts.md)

---
[Back to the documentation index](../README.md)
