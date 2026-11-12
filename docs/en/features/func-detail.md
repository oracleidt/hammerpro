[Documentation](../README.md) > Features > func_detail | [Français](../../fr/features/func-detail.md) | [Русский](../../ru/features/func-detail.md)

# func_detail

`func_detail` brushes still block players, bullets and light, but VBSP does not use them to cut the map into visleaves. Turning decorative brushwork (steps, trims, pillars, rubble, window frames, cylinders) into `func_detail` keeps the visibility simple: VVIS runs faster and each visleaf sees less. Only the outer shell that seals the map from the void must stay world brushes.

Hammer Pro makes this a one key job: select, press `Ctrl+Alt+D`, done. It can also find the brushes that should be detail for you, and tint them in the views so you see at a glance what is detail.

## Make func_detail

| Where | How |
|---|---|
| Keyboard | `Ctrl+Alt+D` (change it in **Settings > Keyboard**) |
| Menu | **Tools > Brush operations > Make func_detail** |
| Toolbar | **To func_detail** in the **Move selected** group, next to **To world** and **To entity** |
| Right click | **Make func_detail** in the context menu of the 2D and 3D views |
| Command palette | `Ctrl+Shift+P`, type `func_detail` |

It turns the selected world brushes into `func_detail` in **one undo step**:

- **One func_detail per selection** (default) puts them all in one entity. **One per cluster** (setting **Settings > Brushes > func_detail > Make func_detail creates**, `brush.detail.split = "cluster"`) makes one entity for each group of touching brushes, handy when you select the trims of a whole street at once.
- **Groups stay intact**: brushes of different groups always go to different `func_detail`, and each new entity joins the group of its brushes. **Visgroups** and **textures** are kept as they are.
- If the selection already contains a `func_detail`, the new brushes are added to it instead of creating another one.
- Brushes already in a `func_detail` stay where they are. Brushes of another brush entity (a door, a `func_brush`) and displacements are skipped; the notice says how many and why (use **Move to world** first to convert a brush entity, displacements never cut visleaves anyway).
- The new entities are selected.

### Sealing check

A `func_detail` does not seal the map. Before converting, Hammer Pro runs the same leak flood as **Map > Check for leaks** and the optimization advisor: it removes the selected brushes from the hull and checks whether the playable space now reaches the void.

- Brushes inside the map (the usual case) are converted at once. When the live leak check already knows the hull, nothing has to be computed at all.
- When some selected brushes seal the map, a window explains it: **{n} of the {total} selected brushes seal the map**, because they are part of the outer shell. Choose **Convert the other {k}** (the safe choice), **Convert all anyway** (the map will leak until you close the hull another way), **Select the sealing brushes** to look at them, or **Cancel**.
- When the map already leaks, the check cannot run: the brushes are converted and a warning asks you to fix the leak (see [Leaks and pointfiles](leaks-pointfiles.md)).

Turn the check off with **Settings > Brushes > func_detail > Keep the brushes that seal the map in the world** (`brush.detail.check_sealing`, on by default).

## Move to world

The reverse is just as quick: **Tools > Move to world** (`Ctrl+Shift+W`), the **To world** toolbar button or the right click menu move the brushes of the selected brush entities, `func_detail` included, back to the world in one undo step. See [Brush operations](brush-operations.md#move-to-world).

## Select func_detail candidates

**Tools > Brush operations > Select func_detail candidates...** opens a small window that selects the world brushes that should be `func_detail`, with the rules of the **Map > Optimize** advisor:

| Option | Default | Effect |
|---|---|---|
| **Largest size** | 1024 units | brushes longer than this on any axis are left out |
| **Steps and stairs** | on | low and long brushes (at most 32 high, 64 deep) |
| **Trims and frames** | on | thin and long brushes (at most 16 thick) |
| **Pillars and posts** | on | narrow and tall brushes (at most 64 wide) |
| **Small pieces and rubble** | on | brushes at most 64 units on every axis |
| **Complex shapes (cylinders, arches)** | on | brushes with more than 6 faces or slanted faces, at most 256 units |
| **Only inside the current selection** | off | look only at the selected brushes (select a building, then refine) |

**Select candidates** runs in the background: the candidates are verified with the leak flood so converting them all never opens the map, and brushes that seal are left out (the result says how many). Nothing in the map changes: review the selection, deselect what you want to keep as world, then press **Make func_detail** (the button appears in the window once something is selected, or use `Ctrl+Alt+D`). The options are saved (`brush.detail.max_size`, `steps`, `trims`, `pillars`, `small`, `complex`, `in_selection`).

Only visible world brushes whose faces all seal (no tool textures, no sky, no displacement) are considered.

In the **Map > Optimize** window, the func_detail finding has a **Select these** button next to **Go to**: it selects the suggested brushes without moving the views. The advisor itself still never changes the map.

## Create brushes as func_detail

The [Block tool](block-tool.md), the [Polygon tool](polygon-tool.md), the [Curve tool](curve-tool.md) and the [Road tool](road-tool.md) have a **Detail** toggle next to their **Create as** picker: when it is on, new brushes are created directly as a `func_detail` (one undo step); click it again for world brushes. It is the same as choosing `func_detail` in the picker.

## Tint func_detail brushes

**View > Tint func_detail brushes** (also **Settings > Brushes > func_detail > Tint func_detail brushes in the views**, `brush.detail.tint`, off by default) gives `func_detail` brushes a blue tint in the 3D views and a blue outline in the 2D views. A color chosen with [Display colors](display-colors.md) or visgroup colors still wins for the outline.

## Settings

All in the `[brush.detail]` table of `settings.toml` and in **Settings > Brushes**:

| Setting | Default | Meaning |
|---|---|---|
| `split` | `"selection"` | `"selection"`: one func_detail per selection, `"cluster"`: one per group of touching brushes |
| `check_sealing` | `true` | run the sealing check before converting |
| `tint` | `false` | tint func_detail brushes in 2D and 3D |
| `max_size`, `steps`, `trims`, `pillars`, `small`, `complex`, `in_selection` | 1024, on, on, on, on, on, off | filters of **Select func_detail candidates** |

## MCP

For AI assistants:

- `make_func_detail`: `ids` / `solid_ids` (default: the selection), `split` (`selection` or `cluster`), `sealing` (`skip` converts the others and reports the sealing ones, `refuse` converts nothing when one seals, `allow` skips the check). Returns the `func_detail` ids, the converted and refused solids, what was skipped and whether the check was verified.
- `select_detail_candidates`: `max_size`, `kinds` (`step`, `trim`, `pillar`, `small`, `complex`), `within`, `within_selection`, `select` (default true), `limit`. Only selects.
- `run_action` with `convert_to_detail`, `select_detail_candidates` (opens the window) or `toggle_detail_tint`; `set_setting` with the `brush.detail.*` paths; `set_tool_options` with `create_as` `func_detail` for the creation tools.

See the [MCP tool reference](../../MCP.md).

## Tips

- Draw the hull first (floor, walls, ceiling, sky), then detail everything else with **Select func_detail candidates** and `Ctrl+Alt+D`.
- Keep the **Detail** toggle on while you dress a room: every new trim and step is detail from the start.
- Turn the tint on before a compile to check that no big wall was turned into detail by mistake.

## FAQ

**Why did some brushes stay in the world?** They seal the map: they separate the playable space from the void. A `func_detail` there would make the map leak, so Hammer Pro keeps them (or asks). **Select the sealing brushes** shows them.

**Can a func_detail touch the void?** Yes, as long as world brushes behind it seal the map. The check only refuses brushes that are the last barrier.

**Does func_detail change lighting or collisions?** No. Detail brushes cast shadows and block players like world brushes. They only stop cutting visleaves, and they no longer block visibility.

**Why are displacements skipped?** Displacements never cut visleaves, so turning them into detail brings nothing.

## Related pages

- [Brush operations](brush-operations.md)
- [Check for problems](check-problems.md)
- [Leaks and pointfiles](leaks-pointfiles.md)
- [Optimizing a map](../guides/optimizing-a-map.md)
- [Block tool](block-tool.md)
- [Render modes and display](render-modes-display.md)
- [Actions and shortcuts](../reference/actions-shortcuts.md)

---
[Back to the documentation index](../README.md)
