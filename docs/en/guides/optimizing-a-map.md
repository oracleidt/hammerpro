[Documentation](../README.md) > Guides > Optimizing a map | [Français](../../fr/guides/optimizing-a-map.md) | [Русский](../../ru/guides/optimizing-a-map.md)

# Optimizing a map

A map runs well when the engine draws only what the player can see, and when its lights make sense. This guide explains the four things that decide it (visleaves, func_detail, hint brushes, areaportals), then lights, and how Hammer Pro checks all of them for you: the **Optimize the map** window for mappers, and the optimization tools of the MCP server for AI assistants.

## Visleaves and the PVS

When the map compiles, VBSP cuts the empty space of the map into convex pieces, the **visleaves** (also called leaves or clusters), along the faces of the world brushes. The portals are the openings between two visleaves. VVIS then computes, for every visleaf, which other visleaves can be seen from it: the **PVS** (potentially visible set). In game, the engine draws everything in the PVS of the visleaf the camera is in.

So:

- the fewer visleaves each visleaf sees, the less the engine draws;
- the fewer and simpler the visleaves, the faster VVIS runs;
- a leak breaks everything: without a sealed hull there is no visibility at all (see [Leaks and pointfiles](../features/leaks-pointfiles.md)).

Big open areas, long straight corridors and windows lined up with each other make big PVS. Walls, corners and doors that block the view make small ones.

## func_detail

Every world brush cuts visleaves, even a crate, a step, a trim or a pillar. Those small brushes cut the space into many tiny slivers: VVIS gets slow, and the culling gets worse because the slivers see around everything. Select them and press **Make func_detail** (`Ctrl+Alt+D`, it refuses brushes that seal the map; **Tools > Brush operations > Select func_detail candidates...** finds them for you, see [func_detail](../features/func-detail.md)): a detail brush still blocks players, bullets and light, but VBSP does not cut visleaves with it, so it does not block visibility either.

Rule of thumb: the hull (walls, floors, ceilings, the big shapes that separate rooms) stays world; everything small or decorative becomes func_detail. Never make func_detail a brush that seals the map or that closes a room: the map would leak or the rooms would see each other.

## Hint brushes

A hint brush tells VBSP where to cut visleaves. Give the cutting face the `tools/toolshint` material and every other face `tools/toolsskip`. Two classic places:

- **at the top of a doorway**, horizontally: the tall visleaf above a door sees over it into the next rooms; a hint at the height of the opening cuts it off;
- **along the edge of an opening at a corner**: the visleaf at a corridor corner sees down both corridors; a hint that extends the jamb of the opening splits it where the view through the opening ends.

A hint only helps when it lowers the PVS: always measure (compile and compare).

## Areaportals

A `func_areaportal` is one thin brush with `tools/toolsareaportal` that closes an opening completely. When every way between two parts of the map is closed by world brushes or areaportals, the compile splits the map into **areas**. Through an open areaportal the engine draws only what is seen through the opening; through a closed one, nothing. Link the portal to its door (`target`) so it closes with the door, and make `StartOpen` match the door.

An areaportal works only when:

- it touches the world on its four sides (no gap around it);
- it sits in a narrow opening: a door frame, a window, the end of a corridor;
- it does not touch another areaportal;
- the space does not go around it through another opening (else the map has one area and the portal culls nothing).

`func_areaportalwindow` does the same for windows, fading a `func_brush` with the distance (`FadeDist`, `FadeStartDist`).

## Lights

Lights do not change visibility, but incoherent lights show at once and cost compile time:

- every light should come from something the player sees: a lamp model, a light texture, a sprite. A light floating in the air with no fixture looks wrong;
- every lamp should give light: a lamp prop or a self illuminated texture with no light near it glows but lights nothing;
- a light inside a brush or a solid prop is blocked: put it just under or in front of the lamp;
- identical lamps should give the same colour and brightness;
- two lights at the same place with the same colour are one light with the summed brightness;
- every named light is switchable: it adds a lightmap page on each face it touches (4 at most per face) and the engine accepts 32 of them;
- a map with sky needs one `light_environment`.

## The Optimize the map window

**Map > Optimize the map...** opens an advisor: it tells you what is not optimized and what to do, in plain language. It never changes the map by itself.

- **Refresh** compiles the BSP, the portals and the visibility of the current map in the background (nothing is written next to the map; the editor stays usable) and checks it. The first opening runs it once. When the map changes, a line says the analysis is out of date.
- The summary gives the visleaves, portals, areas, and how many visleaves a visleaf sees on average and at most.
- The findings are sorted by impact (high, medium, low). The buttons at the top filter them by topic: visleaves, func_detail, hints, areaportals, lights.
- Each finding says what is wrong, why it costs performance and the concrete thing to do, for example *Turn these 12 brushes into func_detail: they are small, do not seal the map and cut 40 visleaves*, *Put an areaportal in the frame of door 214: when the door is closed nothing behind it is drawn*, or *Light 88 has no lamp near it: move it to a lamp or add a fixture*.
- **Go to** selects the objects of the finding and frames them in the 2D and 3D views; for a place without objects (a visleaf, a hint plane, an opening) it moves the cameras there. The box of the finding is drawn in the views while the window is open.
- The func_detail finding also has **Select these**: it selects the suggested brushes without moving the views, so you can review them and press **Make func_detail**.
- **Show visleaves and portals** draws the portals of the analysis in the views, coloured per visleaf (also **Map > Leaks and visibility > Show visleaves and portals**). **Heat map** colours them by the triangles drawn from each visleaf. The other display options (line width, X-ray, PVS from the camera) are those of the [Portals and visibility](../features/leaks-pointfiles.md) window.

The checks:

| Topic | What is checked |
| --- | --- |
| Visleaves | The average and largest PVS, the worst visleaves (most visible leaves, triangles drawn), tiny fragmented visleaves, the VVIS cost. |
| func_detail | World brushes that should be func_detail: small brushes, stair steps, trims, pillars, rubble and complex shapes that are not part of the hull. They are verified with the leak check, so converting them never opens the map. |
| Hints | Hint planes found from the portals (top of doorways, edges of openings at corners), each measured by compiling the map with it: only those that lower the PVS are listed, with the gain. |
| Areaportals | Every areaportal: one brush, thin, narrow opening, sealed on all four sides, not touching another, door link and start state, really separating two areas. Doors without areaportal and doorless openings that are the only way between two groups of visleaves get a suggested areaportal fitted to the opening. |
| Lights | Floating lights, lamps without light, blocked lights, lamps of the same model with different lights, redundant lights, spot lights aimed into walls, too many switchable lights, a missing light_environment, broken falloff values. |

## With an AI assistant (MCP)

The same checks are MCP tools, and there they can apply the fixes (one undo step each, named `MCP: <tool>`):

| Tool | What it does |
| --- | --- |
| `optimize_map` | Everything at once: one report sorted by impact. `apply: "safe"` applies the safe fixes (verified func_detail, measured hints, door areaportals, areaportal repairs, blocked, redundant and broken lights, a missing light_environment), `"all"` also moves floating lights to their lamps, adds lights at lamps, harmonizes lamp colours and puts areaportals in doorless openings. `measure` compiles again and returns the visleaves and the PVS before and after. |
| `analyze_visibility` | The visibility report: leaves, visleaves, portals, areas with their leaf counts, PVS total, average and largest, VVIS cost, worst visleaves, tiny leaves, the brushes that cut leaves. `show` displays the visleaves. |
| `suggest_func_detail` | The func_detail candidates with their kind, verified against leaks; `apply` converts them. |
| `suggest_hint_brushes` | Hint candidates measured by compiling; `apply` adds those with a gain. |
| `show_visleaves` | Shows or hides the visleaves and portals of the last analysis (heat map, X-ray, leaf grid); look at them with `screenshot_view`. |
| `check_areaportals` | Validates every areaportal; `fix` repairs the start state, the door link, the fit and deletes useless ones. |
| `suggest_areaportals` | Openings that should get an areaportal, fitted to the opening; `apply` places them. |
| `audit_lights` | The light coherence audit; `fix` applies the fixes (all or some kinds). |

On a big map the analysis can take longer than one call: the tool then answers `running: true` and the analysis goes on in the background; calling the tool again with the same arguments (without editing the map) returns the result. `vis: "auto"` (the default) uses the exact visibility on maps up to 4000 brushes and the fast one above.

The server tells assistants to run `optimize_map` or `analyze_visibility` before they finish a map. See [Use Hammer Pro with an AI assistant](mcp-with-ai-assistant.md) and [MCP.md](../../MCP.md).

## Example

Two test maps compiled by Hammer Pro, before and after `optimize_map` with `apply: "safe"` (the numbers after come from a new compile of the optimized map): two rooms joined by a corridor with crates, a pillar, steps, a door and a few lights; and four such rooms in a row.

| | Two rooms, before | after | Four rooms, before | after |
| --- | --- | --- | --- | --- |
| Visleaves | 30 | 13 | 125 | 30 |
| Portals | 70 | 14 | 303 | 47 |
| Areas | 1 | 2 | 1 | 4 |
| Total PVS (visible visleaf pairs) | 726 | 169 | 6765 | 750 |
| Estimated VVIS cost | 16986 | 784 | 186830 | 7666 |

In the two rooms, 8 brushes became one func_detail, 3 measured hint brushes were added, the door got an areaportal linked to it, the light inside a crate moved out and the doubled lights were merged; in the four rooms, 52 brushes became func_detail and the 3 doors got their areaportals.

## FAQ

**The window says the map leaks.** Fix the leak first: without a sealed hull there is no visibility, so nothing else can be measured. See [Leaks and pointfiles](../features/leaks-pointfiles.md).

**Why is a brush not offered as func_detail?** It is part of the hull (it touches the outside), it is too big, it is not made of sealing materials, or turning it into func_detail would open the map.

**Why so few hint suggestions?** Only hints that lower the PVS when the map is compiled with them are listed. Many maps need none.

**Does the analysis change my files?** No. It compiles in memory; nothing is written next to the map.
