[Documentation](../README.md) > Features > Groups | [Français](../../fr/features/groups.md) | [Русский](../../ru/features/groups.md)

# Groups

A group ties objects together (brushes, entities, other groups) so one click selects them all and they move as one. Hammer Pro reads and writes Hammer's groups, so groups made in Hammer or Hammer++ survive a round trip.

## Opening it

| Action | Menu | Default shortcut |
| --- | --- | --- |
| **Group** | **Tools > Group** | `Ctrl+G` |
| **Ungroup** | **Tools > Ungroup** | `Ctrl+U` |
| **Ignore groups** | **Edit > Selection mode > Ignore groups** | `Ctrl+W` |
| **Groups** / **Objects** / **Solids** | **Edit > Selection mode** | none |

The toolbar has the **Group**, **Ungroup** and **Ignore groups** buttons next to **Carve**. The selection mode is also at the top of the right dock (**Select:**), like Hammer's right bar.

## Grouping and ungrouping

- **Group** (`Ctrl+G`) puts the selected objects in a new group. Groups that are entirely selected are nested in the new group; other selected objects leave their old group. The grouped objects stay selected. One undo step.
- **Ungroup** (`Ctrl+U`) removes the outermost group of each selected object: its direct members and the groups nested in it become top level. One undo step.

Groups can be nested as deep as you like. The [Outliner](outliner.md) shows them as a tree (**Group #id**).

## Selection modes

The selection mode decides what a click on a brush selects:

| Mode | A click selects | Tooltip |
| --- | --- | --- |
| **Groups** (default) | The whole outermost group, or the whole brush entity. | "Clicks select whole groups" |
| **Objects** | The whole brush entity, ignoring groups. | "Clicks select whole entities, ignoring groups" |
| **Solids** | The single solid, even inside a brush entity. | "Clicks select single solids, even inside entities" |

**Ignore groups** (`Ctrl+W`, Hammer's key) switches to **Solids**, and back to the mode you used before when you press it again. Pressed while **Solids** was chosen by hand, it goes back to **Groups**. The mode is remembered between sessions (`select_mode` in the `[edit]` table of `settings.toml`, default `Groups`).

## Other places that make groups

- Inserted prefabs are grouped when **Group inserted prefabs** is on (see [Prefabs](prefabs.md)).
- **Duplicate** and paste keep the groups that are fully copied (see [Clipboard](clipboard.md)).
- A custom grid can be attached to a group as well as to objects (see [Grids](grids.md)).

## Tips

- Group the pieces of a prop-like brush build (a lamp post, a desk) so a click picks it whole; press `Ctrl+W` for a moment when you need one brush of it.
- Hidden members are never selected with their group: Delete or a move only touches what you see (see [Hide and isolate](hide-isolate.md)).
- MCP tools: `list_groups`, `group_objects`, `ungroup_objects`, `select_group` (see the [MCP tool reference](../../MCP.md)).

## FAQ

**I click a brush and half the room gets selected.** It belongs to a group. Press `Ctrl+W` (or pick **Solids**) to select single brushes, or ungroup with `Ctrl+U`.

**What is the difference between groups and visgroups?** Groups change what a click selects; [visgroups](visgroups.md) change what is shown. An object can be in a group and in several visgroups.

**Can I group a brush entity with world brushes?** Yes. In **Groups** mode a click on either selects the whole group.

## Related pages

- [Selection tool](selection-tool.md)
- [Visgroups](visgroups.md)
- [Outliner](outliner.md)
- [Actions and shortcuts reference](../reference/actions-shortcuts.md)

---
[Back to the documentation index](../README.md)
