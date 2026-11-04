[Documentation](../README.md) > Features > Road details | [Français](../../fr/features/road-details.md)

# Road details: sidewalks, curbs, gutters and terrain blending

Three generators finish a road: the **sidewalk generator** raises a walk beside the road, the **curb and gutter generator** adds a curb and a V or U gutter channel with an optional drain slot, and **Blend terrain to road** makes the displacement terrain follow the road with a smooth shoulder and a texture fade. They work on the roads of the [road tool](road-tool.md) and on the curves of the [curve tool](curve-tool.md), build plain convex brushes (the safest brushes for the compiler), and every generation is one undo step.

## Opening them

All of them are in the **Tools** menu and in the command palette (`Ctrl+P`):

- **Sidewalk generator...**, **Curb and gutter generator...** and **Blend terrain to road...** open the **Road details** window on the matching tab. The three tabs are always one click apart.
- **Edit road details...** (enabled when a brush made by a generator is selected) opens the window on the set of that brush.

None of them has a default shortcut.

## The roads

The generators follow the roads you select:

1. Select the brushes of one or several roads (road tool) or curves (curve tool, or the `create_curve_brushes` MCP tool). Selecting one brush of a road is enough, the whole road is found.
2. Open the generator. The window lists the roads it found (**Roads: N**). If you change the selection afterwards, press **Use the selected roads**.

A curve drawn in the Front or Side view works too: its details are built in the plane of the curve, with the heights measured along the depth axis of that view (a sidewalk beside a curved wall or a ramp seen from the side). A set cannot mix views, and the terrain blend needs roads seen from above, so it refuses a Front or Side curve with a message. Banking (roll) of the road is followed: the details stay level and sit on the road edge, which is higher or lower on a banked road.

Several roads are handled together, which is what makes junctions possible, see below.

## Sidewalk generator

Builds a raised walk on one or both sides of every road.

| Field | What it does | Range | Default |
|---|---|---|---|
| **Sides** | **Left**, **Right** or **Both**, following the direction of the road (first to last point). | | Both |
| **Gap to the road** | Distance between the road edge and the first brush. The road edge includes the curbs of a road drawn with the curve tool Curb profile, and the shoulders of a road tool road. | 0 to 512 | 0 |
| **Corner radius** | Radius of the rounded corners at junctions, measured at the road edge. 0 gives sharp corners. | 0 to 512 | 32 |
| **Width** | Width of the walk. | 1 to 512 | 64 |
| **Height** | Height of the top above the road edge. | 0 to 256 | 6 |
| **Depth under the road** | How far the brush goes under the road edge. | 1 to 256 | 16 |
| **Curb face bevel** | Chamfer of the top edge on the road side, so the curb face is not a razor edge. | 0 to the height | 0 |
| **Cross slope** | Rise of the top from the road side to the far side. Negative values drain toward the road. | -512 to 512 | 0 |
| **Top material**, **Face material** | Material of the sidewalk and of the faces turned toward the road (the curb face). The face material is empty by default, which keeps the top material. **Current** puts the current material. | | the current material |
| **Create as func_detail** | Put all the brushes in one `func_detail` entity instead of the world. | | off |

Press **Create**. The new brushes are selected. The sidewalk is cut in pieces along the road with the same joints as the road, so a banked, rising or curved road gets a sidewalk that follows it without gaps. A piece that would fold on the inside of a tight bend makes the whole generation refuse with the number of pieces, lower the width or open the bend.

## Junctions

When a road **ends on the side of another road** (a street meeting a main road), the generators see a junction by themselves:

- the sidewalk of the main road stops on both sides of the mouth of the side road, which stays open;
- the corners are rounded with the **Corner radius**, as quarter rounds whose inside edge follows the road edge;
- the sidewalk of the side road ends on the corner.

T junctions, Y junctions and crossings work, at any angle of 20 degrees or more. A road that **crosses** another one without ending on it (two curves, or a curve and a road) is cut at the crossing, so the intersection is an X junction: the four mouths stay open and the four corners are rounded with the corner radius. Draw the side road so that its end is on the main road (the road tool junction does it for you).

The same rounding is used for curbs and gutters, with concentric arcs: the corner radius is always the one at the road edge, and each detail further out has the radius reduced by its distance.

When a road tool junction has a **Flare**, give the generator the same **Corner radius** as the flare so the walk follows the rounded pavement.

Refused with a clear message: a junction angle under 20 degrees, a junction too close to the end of a road for the radius (lower the radius), and two junctions of the same side of a road closer than their corners.

## Curb and gutter generator

Builds separate brushes beside the road, in this order from the road edge outward: the **gutter** channel, then the **curb**. The sidewalk generator starts at the road edge too, so give the sidewalk a **Gap to the road** equal to the gutter width plus the curb width to put it after them.

| Field | What it does | Range | Default |
|---|---|---|---|
| **Sides**, **Gap to the road**, **Corner radius** | As for the sidewalk. | | Both, 0, 32 |
| **Curb** | Build the curb. | | on |
| **Curb width**, **Curb height**, **Curb depth** | Size of the curb above and under the road edge. | 1 to 512, 1 to 256, 1 to 256 | 8, 6, 8 |
| **Bevel** | Size of the chamfer of the top edge on the road side (0 for a sharp edge). | 0 to the height | 0 |
| **Rounded top** | Round that edge with a quarter circle (4 steps) instead of a chamfer. With a bevel of 0 the radius is half the curb width or height. | | off |
| **Both top edges** | Treat the top edge on the far side the same way. | | off |
| **Gutter channel** | Build a channel beside the road. | | off |
| **Gutter width**, **Gutter depth** | Width of the channel and how far it sinks under the road edge. | 2 to 512, 0.5 to 256 | 16, 3 |
| **Shape** | **V** (two sloped pieces) or **U** (two slopes and a flat bottom). | | V |
| **Flat bottom** | Width of the flat bottom of a U. | 0 to the width | 6 |
| **Drain slot** | Cut a pit in the middle of the channel every few pieces. | | off |
| **Slot width**, **Slot depth**, **One slot every** | Width of the slot across the channel (it stays narrower than the flat bottom of a U, or half the width of a V), how far it goes under the channel floor, and how many pieces along the road separate two slots. | 0.5 to 512, 1 to 256, 1 to 1024 | 4, 12, 4 |
| **Curb material**, **Gutter material** | Materials. An empty gutter material uses the curb one. | | the current material |

The gutter is made of convex pieces (a V is two wedges, a U is two wedges and a block) sitting on a foundation under the channel, so the map stays closed. A drain slot is a pit with its own floor, it does not open the map to the void.

## Editing a set

Every set remembers its roads and all its numbers in the `editor` block of its brushes (the `hammerpro_roadside` keys, which Hammer and Hammer++ ignore and keep), so it can be changed later even after saving the map.

1. Select one brush of the set and use **Edit road details...**, or keep the window open after **Create**: the next presses of **Apply changes** replace the same set.
2. Change any number, the sides, the materials, or press **Use the selected roads** to give the set other roads.
3. **Apply changes** replaces all the brushes in one undo step, in the same entity if there was one, and keeps them selected. **New set** stops editing and makes the next press create another set.

A set that follows a road tool road reads the road again every time it is applied: after you edit the road, open **Edit road details...** on the set and press **Apply changes** with nothing changed to rebuild it on the new road. A set that follows a curve keeps the curve as it was when the set was made, change it by picking the curve again with **Use the selected roads**.

Copies of the whole set keep working. The brushes of a set found by **Edit road details...** are the ones with the same tag within 512 units of each other, so a copy far away is not touched.

## Blend terrain to road

Makes displacement terrain follow a road: the vertices under and around the road go to the road height, with a smooth shoulder back to the terrain. It works on **every displacement within reach of the selected roads** at once, not only on the selected ones, it sews the neighbours so seams stay closed, and it is one undo step.

| Field | What it does | Range | Default |
|---|---|---|---|
| **Shoulder width** | Width of the fade from the road height back to the terrain, beyond the road edge and the flat margin. | 0 to 4096 | 128 |
| **Flat margin** | Extra width beside the road kept fully at the road height. | 0 to 1024 | 0 |
| **Falloff** | Shape of the fade: **Smooth** (smoothstep), **Linear**, **Steep near the road** (squared, drops fast) or **Steep far from the road** (stays high, then drops). | | Smooth |
| **Strength** | How far the terrain goes toward the road, 1 is fully. | 0 to 1 | 1 |
| **Sink under the road** | Put the terrain this many units under the road surface so the two do not overlap and flicker. | -256 to 256 | 1 |
| **Move** | **Up and down** (default), **Only sink** (never lifts the terrain) or **Only raise** (never lowers it). | | Up and down |
| **Texture blend** | **Off**, **2 way (alpha)** or **4 way (multiblend)**, see below. | | Off |
| **Texture fade width**, **Texture strength** | Width of the texture fade past the road edge, and how far it goes toward the road texture. | 0 to 4096, 0 to 1 | 64, 1 |
| **Road texture** | Which texture of the displacement is the road: 0 to 3 for 4 way, 0 or 1 for 2 way. | 0 to 3 | 1 |

The road height is the height of the road surface at the nearest point of the road, so a rising, banked or ditch profile road is followed: the terrain under a banked road is tilted like the road. Beyond the end of a road the area is rounded. When several roads overlap, each vertex follows the road that influences it most. Only the height changes, along the vertical, and the vertices that two displacements share keep moving together.

### Texture blend

The alpha of a displacement tells the engine how to mix two textures, and the multiblend rows how to mix four. With **2 way**, the alpha moves toward the road texture (alpha 255 for texture 2, the default road texture, alpha 0 for texture 1) across the **Texture fade width**, and returns to whatever the terrain had at the end of the fade, so the terrain keeps its own painting. With **4 way**, the weight of the chosen **Road texture** goes toward 1 and the other weights shrink in proportion, written as multiblend rows like the blend brush of the displacement tool (use a Lightmapped_4WayBlend material). The painting of vertices outside the fade is never touched.

### Preview

With **Show the affected area** on (the default on this tab), the 3D view shows the displacements as they will be after the blend: a tinted surface over the affected cells (orange where the height changes, blue where only the texture does, more opaque where the effect is stronger), the road edge as white lines and the end of the shoulder as yellow lines, also in the 2D views. The preview follows every change of a number and of the map, and disappears when the window closes or the tab changes. It can show up to 60000 triangles.

## MCP

| Tool | What it does |
|---|---|
| `create_roadside` | Builds a sidewalk (`kind` sidewalk) or a curb and gutter (`kind` curb) with the fields above (`road_ids`, `sides`, `gap`, `corner_radius`, `sidewalk_*`, `curb`, `curb_*`, `gutter`, `gutter_*`, `drain_slot`, `slot_*`, `material`, `face_material`, `curb_material`, `gutter_material`, `tex_scale`, `entity_class`). |
| `edit_roadside` | Replaces a set in one undo step with only the fields that change (`solid_id` of any brush of the set, default the selected one). Without any field it rebuilds the set on the roads as they are now. |
| `get_roadside` | Reads the numbers, the roads and the brush ids of a set. |
| `blend_terrain_to_road` | Runs the terrain blend (`shoulder`, `margin`, `falloff`, `strength`, `sink`, `mode`, `texture_blend`, `texture_width`, `texture_strength`, `road_channel`) on the roads of `road_ids`. |
| `preview_terrain_blend` | Counts what the blend would change and shows the area in the 3D view (`show`, `clear`). |

The actions `road_sidewalk`, `road_curb`, `road_terrain` and `roadside_edit` open the windows with `run_action`. See the [MCP tool reference](../../MCP.md).

## Tips

- Draw the road first, then the sidewalk, then check the junctions: select all the roads of a block at once so their junctions are found.
- For a typical street: a V gutter of 12 units, a curb of 6, and a sidewalk with a gap of 18. Or skip the gutter and put the sidewalk at gap 0 with a bevel of 2 for a worn curb face.
- Put the sidewalk as `func_detail` when the walk is only decoration, it keeps the BSP tree simple.
- Blend the terrain after the road is final, since the terrain follows its height at that moment. Apply it again after a road edit, the already blended terrain is not a problem (it moves to the new height).
- A road on rough terrain: build it with **Follow terrain** of the road tool first, or blend the terrain to the road, depending on which one must keep its shape.

## FAQ

**Can I use a curve drawn in the Front or Side view?** Yes for sidewalks, curbs and gutters, in the plane of the curve. Not for the terrain blend, which needs roads seen from above.

**The sidewalk does not stop at my side road.** The side road must end on the main road (within the width of the main road) and both must be selected together.

**The terrain flickers on the road.** Use a sink of 1 or more, so the displacement stays under the road surface.

**Can I undo the terrain blend?** Yes, one `Ctrl+Z` restores the heights, the textures and the sewing.

## Related pages

- [Road tool](road-tool.md)
- [Curve tool](curve-tool.md)
- [Displacement tool](displacement-tool.md)
- [Brush operations](brush-operations.md)
- [Undo history](undo-history.md)

---
[Back to the documentation index](../README.md)
