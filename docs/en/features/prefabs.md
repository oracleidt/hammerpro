[Documentation](../README.md) > Features > Prefabs | [Français](../../fr/features/prefabs.md) | [Русский](../../ru/features/prefabs.md)

# Prefabs

A prefab is a piece of map saved as a VMF file (a staircase, a light fixture, a vending machine with its entities) that you insert as many times as you like. Unlike an [instance](instances.md), an inserted prefab becomes ordinary objects of your map. The **Prefabs** tab is your prefab library, with folders, search and thumbnails.

## Opening it

| Action | Menu | Default shortcut |
| --- | --- | --- |
| Prefabs tab | **View > Panels > Prefabs** | none |
| **Create prefab...** | **Tools > Placement and props > Create prefab...** | `Ctrl+R` |
| **Insert prefab at original size** | **Tools > Placement and props > Insert prefab at original size** | `Alt+Shift+C` |

## The library

The tab lists the `.vmf` files of the prefab folders, scanned in the background. Subfolders are categories. The folders are:

| Root | Where | Writable |
| --- | --- | --- |
| **My prefabs** | The library folder: `%APPDATA%\HammerPro\prefabs` by default, or the folder chosen in the settings. | Yes |
| **Prefab folder** | Each extra folder added in the settings. | No |
| The game (**Garry's Mod**) | The Hammer prefab folders of the game install (`bin/prefabs`, `bin/win64/prefabs`). | No |

Tab controls:

| Control | What it does |
| --- | --- |
| **+** | **Create a prefab from the selection...** (same as `Ctrl+R`). |
| Refresh | **Scan the prefab folders again**. |
| Folder | **Open my prefab folder**. |
| Slider | **Tile size** (56 to 192 points). |
| **Search prefabs...** | Filters the tiles by name. |
| Category list | **All prefabs** or one folder. |
| Count | "N of M prefabs" (very large folders show only their first files). |

Each tile shows a thumbnail drawn in the background and cached on disk, so the tab opens instantly next time. The tooltip gives the name, category and path.

| Gesture on a tile | What it does |
| --- | --- |
| Click | Selects the prefab (for **Insert prefab at original size**). |
| Double click | Arms the prefab for the [entity tool](entity-tool.md). |
| Drag into a view | Inserts it where you drop it. |
| Right-click | **Place with the entity tool**, **Insert at original size**, **Show in folder**. |

## Inserting a prefab

- **Insert prefab at original size** (`Alt+Shift+C`): the prefab selected in the tab goes at the center of the 2D view, at its original size (its depth kept).
- **Entity tool**: after a double click (or **Place with the entity tool**), the entity tool places the prefab until you press `Escape`. In the 3D view a click puts it on the surface under the cursor at its original size. In a 2D view a click centers it on the grid point, and a drag inserts it scaled to the box you draw. Offsets are snapped to the grid, so prefabs built on the grid stay on it. The options bar shows **Prefab:** with its size, a **Group** checkbox and **Back to entities** ("Place entities again instead of the prefab (Escape)").
- **Drag and drop** from the tab into a view.

Every insertion is one undo step "Insert prefab". The objects get new ids, are grouped when **Group inserted prefabs** is on (unless they already form one group), and are selected.

### The `&i` token

Write `&i` in names inside a prefab (for example `door_&i` and an output targeting `door_&i`). Each insertion replaces every `&i` in key values and outputs (names, parent names, targets, output parameters) with a number unique in the map, so every copy has its own names while names pointing outside the prefab stay as they are.

## Creating a prefab

Select the objects and press `Ctrl+R`. The **Create prefab** window asks:

| Field | What it does |
| --- | --- |
| **Name:** | File name of the prefab. |
| **Folder:** | A folder of your library, **(library)** for its root, **or a new folder** (subfolders with `/`). Created if needed. |
| **Replace the existing prefab** | Shown when the name exists. |

**Save** writes the VMF in the background (atomic write). Names and folders cannot contain `\ / : * ? " < > |` or reserved names.

## Options

**Tools > Options... > Prefabs** (`[placement]` in `settings.toml`):

| Option | What it does | Default |
| --- | --- | --- |
| **Show the Prefabs tab** (`show_prefabs`) | Shows the tab in the right dock. | On |
| **Library folder:** (`prefab_library`) | Where new prefabs are saved. **Browse...**, **Use the default folder**. | `%APPDATA%\HammerPro\prefabs` |
| **More prefab folders (read only):** (`prefab_dirs`) | Extra folders listed read only. **Add a folder...**, remove with the cross. | None |
| **List the prefabs of Garry's Mod** (`game_prefabs`) | Lists the Hammer prefab folders of the game install, read only. | On |
| **Group inserted prefabs** (`group_prefabs`) | The objects of each inserted prefab form a group. | On |
| Tile size (`prefab_tile`) | Size of the tiles, 56 to 192. | 96 |

## Tips

- Prefabs from Hammer (the game's `bin/prefabs`) work as they are.
- Build prefabs on the grid near the origin: insertions snap their offset to the grid.
- MCP tools: `list_prefabs`, `insert_prefab`, `create_prefab` (see the [MCP tool reference](../../MCP.md)).

## FAQ

**Prefab or instance?** A prefab is copied into your map and then lives on its own. An [instance](instances.md) stays linked to its file: edit the file and every copy changes.

**My new prefab does not show.** The library is scanned in the background; press the refresh button if you added files by hand.

**Two copies of my prefab fire each other's outputs.** Use `&i` in the names so each copy gets unique names.

## Related pages

- [Entity tool](entity-tool.md)
- [Instances](instances.md)
- [Sprinkle tool](sprinkle-tool.md)
- [Files and folders](../reference/files-and-folders.md)

---
[Back to the documentation index](../README.md)
