[Documentation](../README.md) > Features > Cubemaps | [Français](../../fr/features/cubemaps.md) | [Русский](../../ru/features/cubemaps.md)

# Cubemaps

Reflective materials (shiny floors, glass, metal, water: every material with `$envmap env_cubemap`) do not reflect the real scene. At compile time VBSP gives each of their faces one `env_cubemap`: the face lists it in the cubemap's **sides** keyvalue, or else it takes the **nearest cubemap in front of it**. In game, `buildcubemaps` renders a small picture from each cubemap. A room without its own cubemap reflects another room (or the sky when the map has none at all) and its floors shine in the dark.

The **Cubemaps** tool shows where reflections come from and suggests where to put cubemaps. It is a tool you open: nothing runs and nothing changes in the map until you accept a suggestion.

## Open it

| Where | How |
|---|---|
| Menu | **Tools > Placement and props > Cubemaps...** |
| Optimization advisor | **Cubemaps tool** on a cubemap finding of **Map > Optimize the map** |
| Command palette | `Ctrl+P`, type `cubemaps` |

The view toggle **Show cubemap assignment** is also in **View > Diagnostics**.

## What it analyzes

**Analyze** looks at the current map (it runs once when the window opens, then again after you edit the map when it is fast):

- **Reflective faces**: every brush face whose material uses `$envmap env_cubemap` (read from the game materials; without a configured game, names with water, glass, metal, tile, marble... count). Faces hidden inside other brushes or pressed against a wall are left out, like the compiler does. Each material gets a strength, its `$envmaptint` times the average of its mask (`$envmapmask`, or the alpha of the base texture or normal map with `$basealphaenvmapmask` or `$normalmapalphaenvmapmask`): materials masked to almost nothing (under 5 %) do not count, and rooms are weighed by their reflective surface times that strength.
- **Rooms**: reflective faces are grouped when they see each other without going through a narrow opening (a doorway, a corridor, a window); rooms that see each other are merged, and a few small shiny pieces (a trim, a patch in a doorway) join the nearest room instead of making their own. With **Use compiler areas** the map is compiled in the background like the optimization advisor, and rooms never cross the areas cut by areaportals; the window says which way the rooms were found.
- **Which face takes which cubemap**, exactly like VBSP: sides first, then the nearest cubemap in front of the face.
- Per cubemap: its size (`cubemapsize`) and memory, its room, its height above the floor, how many reflective faces take it.

The summary line gives the cubemap count and their memory, the reflective faces and the rooms, then how many faces take a wrong cubemap: one they only see through a wall, or one of another room while a cubemap of their own room is in sight (and how many of those through a wall) and how many are far from their cubemap. A last line counts the faces that cannot get a better cubemap even with every suggestion accepted: no free spot near them sees them (a window pressed against another building, a narrow light well), or their group has too little reflective surface to be worth a cubemap.

## Findings

| Finding | What to do |
|---|---|
| No cubemap at all | Every reflective face shows the default sky cubemap. Place cubemaps (accept the suggestions). |
| Room without a cubemap | The room has reflective faces and no cubemap that sees it. Place one at eye height in it. |
| Faces taking another room's cubemap | They take a cubemap of another room or one behind a wall. Add them to the sides of a cubemap of their room (suggested) or move the cubemap. |
| Faces far from their cubemap | More than 1024 units away: the reflection does not match. Add a cubemap closer. |
| Cubemap inside a brush | It renders black. Move it into the room. |
| Cubemap in the void | Outside the playable space (needs the compiler areas). Move it into a room. |
| Cubemap at floor level | Less than 32 units above the floor: reflections look as seen from the ground. Raise it to eye height, 64 to 72 units above the floor. |
| Cubemaps too close | Two cubemaps under 128 units apart in the same room capture the same picture twice. Delete one. |
| Size too large or too small | 256x256 costs 2 MB per cubemap: keep it for mirrors and big shiny floors. 16x16 and below look blurry on large reflective surfaces. 0 (default, 128x128) fits most rooms. |
| Sides pointing at deleted faces | VBSP warns about it. Remove those ids from `sides`. |
| Too many cubemaps | More than 128 (or 64 MB): loading time and BSP size grow. One per room with reflective surfaces is usually enough. |

**Go to** frames the objects or the place of a finding; its box is drawn in the views while the window is open.

## Suggestions

- **New cubemap**: one per room with reflective faces and no cubemap that sees it, at the area weighted centre of its reflective faces, at **eye height** (64 units above the floor, halfway up under a low ceiling), moved away from brushes when the spot is inside or against one. A long room (more than 1024 units) gets several, spread over its reflective faces. Rooms with less than 128x128 units of weighted reflective surface (a few shiny trims) only get a low finding, no suggestion. In a room that has cubemaps, where a large reflective area is more than 1024 units from its cubemap, a cubemap is suggested there too (up to four per room).
- **Facades**: faces that would still see their cubemap only through a wall, with no cubemap they can see within 2048 units (the windows of a tall building hidden behind another one), are grouped by facade (same side, within 1024 units); each group with enough reflective surface gets a cubemap in the open space in front of it, 192 to 768 units out, at the height of the group (at least eye height above the ground below).
- **Sides**: faces that would still take the cubemap of another room get listed in the `sides` of a cubemap of their own room, new or existing, so they reflect the right room.

Suggestions show as **ghosts** in the 2D and 3D views (cyan for new cubemaps, yellow for sides changes); **Go to** highlights one and draws lines to the faces it adds to its sides. New cubemaps are suggested at the size most cubemaps of the map already use (default 128x128 on a map without any); **Size of new cubemaps** keeps that (**As suggested**) or forces default, 32x32, 64x64, 128x128 or 256x256. A `sides` fix only uses a cubemap of the face's own room less than 2048 units away.

**Accept** applies one suggestion, **Accept all** applies them all; each press is **one undo step** (*Place cubemaps*). **Dismiss** hides a suggestion until the next analysis. Accepting places entities, it does not build the pictures: **Map > Lighting > Rebuild cubemaps** draws them in the 3D view, and after a compile `buildcubemaps` in game or **Map > Lighting > Write cubemaps into the compiled map** builds the real ones (see [Cubemaps in the compiler](../compiler/cubemaps.md)).

## Cubemap assignment view

**Show cubemap assignment** (off by default, in the window and **View > Diagnostics**) colours each face in the 3D view by the cubemap it will use, one colour per cubemap: reflective faces strongly, the others faintly (props near them get the same cubemap). A line goes from each reflective face to its cubemap; a red line (and a red face) means the cubemap of another room or one behind a wall. Cubemaps are drawn as boxes of their colour, also in the 2D views.

## MCP

| Tool | What it does |
|---|---|
| `analyze_cubemaps` | The whole analysis: cubemaps, rooms, reflective faces and their cubemap, findings, suggestions. |
| `suggest_cubemaps` | The suggestions; `apply` places them (all or `indices`, `size` for new ones) in one undo step. |
| `show_cubemap_assignment` | Shows or hides the assignment view and opens or closes the window; look with `screenshot_view`. |

The actions `show_cubemaps` and `toggle_cubemap_assignment` work with `run_action`. The cubemap findings are also part of `optimize_map` (topic `cubemaps`), which never changes cubemaps by itself. See [MCP.md](../../MCP.md).

## See also

- [Optimizing a map](../guides/optimizing-a-map.md)
- [Lighting preview](lighting-preview.md): cubemap reflections in the 3D view
- [Cubemaps in the compiler](../compiler/cubemaps.md): how VBSP assigns them and how to build them
