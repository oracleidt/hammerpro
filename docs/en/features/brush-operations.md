[Documentation](../README.md) > Features > Brush operations | [Français](../../fr/features/brush-operations.md)

# Brush operations

Commands that change brushes as a whole: carve, make hollow, merge, tie brushes to an entity and back to the world, convert to `func_detail`, nodraw on hidden faces, hint brushes, and the texture locks that decide how textures follow transforms.

All of them are in the **Tools** menu (except **Make hint brush**, only in the [command palette](command-palette.md)) and work on the current selection. Each one is a single undo step.

## Command summary

| Command | Default shortcut | Needs |
|---|---|---|
| **Carve** | `Ctrl+Shift+C` | one or more brushes |
| **Make hollow** | `Ctrl+H` | one or more brushes |
| **Merge brushes** | `Ctrl+Shift+M` | at least two brushes |
| **Merge into convex solid** | none | at least two brushes |
| **Intersect brushes** | `Ctrl+Shift+K` | at least two brushes |
| **Union of brushes** | none | at least two brushes |
| **Tie to entity** | `Ctrl+T` | a selection |
| **Tie each to its own entity** | `Ctrl+Shift+T` | a selection with world brushes |
| **Move to world** | `Ctrl+Shift+W` | brushes of brush entities |
| **Convert to func_detail** | none | a selection |
| **Nodraw on hidden faces...** | none | nothing (scans the whole map) |
| **Make hint brush** | none (command palette) | faces selected with the texture tool |
| **Texture lock** | `Shift+L` | |
| **Texture scale lock** | `Shift+K` | |

When a command needs brushes and none are selected, the notice **Select one or more brushes first** (or **Select at least two brushes**) shows. Shortcuts can be changed in **Settings > Keyboard** (see [Keyboard shortcuts](keyboard-shortcuts.md)).

## Carve

**Tools > Carve** (`Ctrl+Shift+C`) subtracts the selected brushes from the world brushes they intersect, like Hammer's Carve.

- The selected (carving) brushes are not changed. Keep them or delete them afterwards.
- Only visible world brushes are cut: brushes of entities and hidden brushes are left alone.
- The cut brushes are split into several convex pieces with new face ids. Overlays and `env_cubemap` face lists follow: a face cut into pieces is replaced by all of them, a face carved away entirely is removed from the list.

Carve makes many small brushes. For simple openings, the [extrude / carve tool](extrude-tool.md) (an outline clicked on one face, carved into that brush only, with as few pieces as possible), the [clipping tool](clip-tool.md) or several block brushes often give cleaner geometry.

## Make hollow

**Tools > Make hollow** (`Ctrl+H`) turns each selected brush into walls. A small window asks for the **Wall thickness**:

| Field | What it does | Default |
|---|---|---|
| **Wall thickness** | Thickness of the walls in units, from -4096 to 4096. A negative value hollows outward: the walls are built outside the brush, so the original volume becomes the inside room. | 32 |
| **OK** / `Enter` | Hollows the selection. | |
| **Cancel** / `Esc` | Closes without change. | |

The thickness field has the keyboard focus when the window opens, so you can type the value and press `Enter`. The walls of each world brush are grouped (like Hammer) and selected. If a brush is too small for the thickness, nothing changes and an error is shown. Overlay and cubemap face lists move to the outer faces of the walls.

## Merge brushes

**Tools > Merge brushes** (`Ctrl+Shift+M`) joins the selected brushes into one brush, only when their union is already convex (Hammer++'s "Merge brushes"). Every face keeps the texture of the face it comes from.

- If the union is not convex, nothing changes and a warning suggests **Merge into convex solid**.
- The brushes must all be world brushes or all belong to the same entity, else the warning **The brushes must all be world brushes or belong to the same entity** shows.

## Merge into convex solid

**Tools > Merge into convex solid** wraps the selected brushes in their convex hull, even when that adds volume. The result replaces the first brush (same id, owner and editor data) and the others are deleted.

## Intersect brushes

**Tools > Intersect brushes** (`Ctrl+Shift+K`) keeps only the volume shared by all the selected brushes. The result is one convex brush that replaces the first selected brush (same id, owner and editor data); the others are deleted.

- Faces that survive keep their texture. The new cut faces take the texture of the face that cut them, like Carve.
- If the brushes do not overlap (touching is not enough), nothing changes and the warning **The brushes do not overlap: there is no shared volume to keep** shows.
- The brushes must all be world brushes or all belong to the same entity, else the warning **The brushes must all be world brushes or belong to the same entity** shows.

## Union of brushes

**Tools > Union of brushes** joins the selected brushes into the fewest convex brushes that cover exactly their union, without adding any volume (unlike **Merge into convex solid**).

1. Overlapping brushes are cut so that they no longer overlap, and the parts hidden inside another brush are removed.
2. Touching neighbors are merged back into one brush whenever their union is convex.

So two overlapping boxes in line become one box, while an L shape stays two brushes that touch without overlapping. Each brush keeps its id when it still has a piece; brushes entirely inside others are deleted. The new pieces keep the owner and editor data of the brush they come from and are selected when it was. When nothing can be joined, the warning **Nothing to join** shows. The same owner rule as **Merge brushes** applies. One undo step.

## Tie to entity

**Tools > Tie to entity** (`Ctrl+T`) behaves like Hammer:

- With exactly one brush entity among the selection plus other brushes, those brushes are added to that entity, which keeps its class and keys.
- With a single brush entity selected alone, its properties open.
- Otherwise the selection is tied to a new brush entity of the **Default brush entity** class (setting `default_brush_entity`, default `func_detail`, in **Settings > Editing > New objects**), with the default keyvalues of that class from the FGD.
- When the FGD class has an `origin` key (`func_tracktrain`, `func_tanktrain`, `func_rotating`, `func_door_rotating`...), the new entity gets it at the center of its brushes, like Hammer. The game moves and turns the entity around that point: a train puts it on its `path_track` nodes (raised by its `height` key). **Tools > Center origins** moves it back to the center later.

Brushes taken from other brush entities move to the entity, and brush entities left without brushes are deleted. The entity is selected and the **Entity properties** panel opens (see [Entity properties](entity-properties.md)).

## Tie each to its own entity

**Tools > Tie each to its own entity** (`Ctrl+Shift+T`, Hammer++) puts every selected world brush into its own new entity of the default brush entity class. Brushes that already belong to an entity are left alone. The new entities are selected and the entity panel opens.

## Move to world

**Tools > Move to world** (`Ctrl+Shift+W`) moves the brushes of the selected brush entities back to the world. Brush entities left without brushes are deleted. The moved brushes are selected.

## Convert to func_detail

**Tools > Convert to func_detail** ties the selection to one new `func_detail`, whatever the default brush entity class is. If the selection is already a single `func_detail`, the notice **The selection is already a func_detail** shows. To find brushes worth converting, see **Map > func_detail suggestions...** in [Check for problems](check-problems.md).

## Nodraw on hidden faces

**Tools > Nodraw on hidden faces...** searches the whole map for brush faces entirely covered by other brushes, which can never be seen.

1. The search runs in the background (**Searching for hidden faces...** in the console).
2. The faces found are selected and the [texture application tool](texture-tool.md) becomes active so you can check them in the views.
3. A window says how many faces were found. **Apply nodraw** gives them `tools/toolsnodraw`, **Cancel** closes it.

Rules used by the search:

- Candidates are faces of visible world and `func_detail` brushes that are not displacements and do not already have a tool texture.
- Covering brushes are visible world brushes without displacements and without see-through materials. Tool textures other than nodraw, black and skybox count as see-through, as do material names containing glass, water, window, fence, grate, chainlink, translucent or alpha.
- A `func_detail` never hides anything, so faces between two `func_detail` brushes are left alone.

If a brush involved changed between the search and **Apply nodraw**, nothing is applied and **The map changed since the search: run it again** shows.

## Make hint brush

**Make hint brush** (command palette only) turns brushes into hint brushes: select the faces that must become hint faces with the [texture application tool](texture-tool.md), then run the command. Those faces get `TOOLS/TOOLSHINT` and every other face of their brushes `TOOLS/TOOLSSKIP`. Without selected faces, the notice **Select the faces that become hint faces with the texture tool first** shows. The console reports how many hint brushes were made.

## Texture lock and texture scale lock

| Option | What it does | Default |
|---|---|---|
| **Texture lock** (`Shift+L`, `texture_lock`) | Textures follow brushes when they are moved, rotated, scaled or flipped. Off, textures stay fixed in the world (and are reset to world alignment when a rotated face can no longer use them). | on |
| **Texture scale lock** (`Shift+K`, `texture_scale_lock`) | Scaling a brush also scales its texture. Only used with texture lock on. Without it, the texture keeps its scale and stays anchored at the brush center. | off |

Both are in the **Tools** menu, in **Settings > Editing > Behavior**, and the Transform dialog shows texture lock with a **Toggle** button (see [Transform and align](transform-align.md)). They are saved in the `[edit]` table of `settings.toml`.

## MCP

For AI assistants: `carve`, `hollow`, `merge_brushes` (`convex_hull` true for Merge into convex solid), `intersect_brushes`, `union_brushes`, `tie_to_entity`, `move_to_world`, `make_hint_brushes`, `find_hidden_faces`, `apply_nodraw` and `run_action` for the other commands. See the [MCP tool reference](../../MCP.md).

## Tips

- Use a negative thickness in **Make hollow** to turn a block drawn at the size of a room into that room with its walls outside.
- Run **Nodraw on hidden faces...** before a final compile: hidden faces cost rendering and lightmap space for nothing.
- `Ctrl+T` with an existing brush entity and new brushes selected adds them to that entity: handy to extend a door or a `func_detail`.
- Set **Default brush entity** to the class you use most (for example `func_brush`) so `Ctrl+T` and `Ctrl+Shift+T` create it directly.

## FAQ

**Carve did nothing on my func_detail.** Carve only cuts world brushes. Use **Move to world** first, carve, then tie the pieces again.

**Merge brushes says the shape is not convex.** Merged brushes must form a convex brush. Use **Merge into convex solid** if adding volume is fine, or keep them as separate brushes.

**Union of brushes leaves several brushes.** The result is made of convex brushes only, so a non convex union (an L, a T) needs at least two. They no longer overlap or share internal faces.

**Make hollow fails.** The brush is too thin for the wall thickness. Lower the thickness or make the brush bigger.

**Why are the nodraw faces selected in the texture tool?** So you can check them before applying. Nothing changes until you click **Apply nodraw**.

## Related pages

- [Clipping tool](clip-tool.md)
- [Vertex tool](vertex-tool.md)
- [Transform and align](transform-align.md)
- [Groups](groups.md)
- [Entity properties](entity-properties.md)
- [Texture application](texture-tool.md)
- [Check for problems](check-problems.md)
- [Actions and shortcuts](../reference/actions-shortcuts.md)

---
[Back to the documentation index](../README.md)
