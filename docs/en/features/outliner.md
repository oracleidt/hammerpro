[Documentation](../README.md) > Features > Outliner | [Français](../../fr/features/outliner.md) | [Русский](../../ru/features/outliner.md)

# Outliner

The **Outliner** tab shows the map as a tree: the world with its groups and solids, and the entities sorted by class. It is searchable, synced with the selection, and fast even on very large maps (only the visible rows are drawn).

## Opening it

**View > Panels > Outliner** shows or hides the tab in the right dock. It is hidden by default (`ui.show_outliner`).

## The tree

| Node | Contains |
| --- | --- |
| **World** | The groups (**Group #id**, nested like in the map) and the world solids (**Solid #id**). |
| **Entities** | One node per class (sorted without case), the entities of that class in file order. A brush entity stands for its solids. |

The bottom line sums up the map: "World solids: N, entities: N, selected: N". The **World** and **Entities** nodes start expanded.

## Controls

| Control | What it does |
| --- | --- |
| Search box | "Search a name, class, id or material". Rows are kept when they or a child match, and the parents of matches are expanded. A class that matches by name stays collapsed. |
| Down arrow button | **Expand all**. |
| Up arrow button | **Collapse all** (keeps **World** and **Entities** open). |
| Eye of a row | **Hide** or **Show** what the row stands for (an object, a group, a class, the world). Shown crossed out when everything below it is hidden. |
| Arrow of a row | Expands or collapses it. |

## Selecting

| Gesture | What it does |
| --- | --- |
| Click a row | Selects its visible objects (a group row selects its members, a class row its entities). |
| `Ctrl`+click | Adds the row to the selection, or removes it when all its visible objects are already selected. |
| `Shift`+click | Selects the range of rows from the last clicked row. |
| Double click | Selects and centers the 2D and 3D views on the selection. |
| `Up` / `Down` (pointer over the list) | Selects the previous or next row. |
| `Right` | Expands the current row. |
| `Left` | Collapses the current row, or goes to its parent. |
| `Enter` | Centers the views on the selection. |

Hidden objects are never selected: a row holding only hidden objects keeps the current selection and tells you "Hidden objects cannot be selected: show them first (eye button)". Clicking a row does not take the keyboard focus, so `Delete`, `H` and the tool keys keep working on the selection you made in the outliner.

When you select objects elsewhere, the outliner expands the parents of the first selected object and scrolls to it.

## Tips

- Search a material name to find the world solids that use it.
- Search an id (the number of **Solid #id**) to jump to a brush named in a compile error. **Map > Go to brush or entity id...** (`Ctrl+Shift+G`) does it from anywhere (see [Check for problems](check-problems.md)).
- MCP: `list_entities`, `list_solids`, `list_groups` give the same information as text (see the [MCP tool reference](../../MCP.md)).

## FAQ

**The tab is not there.** Turn on **View > Panels > Outliner**.

**Clicking a row selected nothing.** Everything in that row is hidden. Click its eye to show it, then click the row again.

**Why are the arrow keys not moving my selection in the 2D view?** While the pointer is over the outliner, the arrow keys move in the list. Move the pointer back to a view.

## Related pages

- [Groups](groups.md)
- [Visgroups](visgroups.md)
- [Hide and isolate](hide-isolate.md)
- [Interface tour](../getting-started/interface-tour.md)

---
[Back to the documentation index](../README.md)
