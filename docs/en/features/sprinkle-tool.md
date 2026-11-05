[Documentation](../README.md) > Features > Sprinkle tool | [Français](../../fr/features/sprinkle-tool.md)

# Sprinkle tool

The sprinkle tool paints entities (props, plants, rocks, lights...) on the surfaces of the map with a round brush, like the Sprinkle tool of Hammer++ and CS:GO Hammer. What it places comes from sprinkle definition files, in the same format as Hammer++, so existing definitions work as they are.

## Opening it

- **Tools > Sprinkle tool**, or its button in the tool palette. It has no default shortcut (you can give it one in **Settings > Keyboard**, see [Keyboard shortcuts](keyboard-shortcuts.md)).
- Choose a sprinkle type in the **Type:** list of the options bar. Without a type, painting shows **Choose a sprinkle type first**. Without any definition, the status bar tells you which folder to put `.txt` files in.

## Painting

The brush is a circle drawn under the cursor (green, red when erasing).

- In the 3D view it follows the surface under the cursor; its plane is the one of the main axis of the surface normal (a floor, a wall or a ceiling).
- In a 2D view it lies in the view plane, and the entities land on the first surface seen from the viewer.

The plane is split into grid cells (the grid of the type, or the editor grid with **Use grid**). Each cell gets at most one entity, placed on the surface under the cell.

| Gesture | What it does |
| --- | --- |
| Click or drag | Apply the current mode to the cells under the brush. Each cell is handled once per stroke. A whole stroke is one undo step. |
| `Ctrl`+drag | Erase every entity of the type under the brush, whatever the mode and density. |
| `Alt`+mouse wheel | Change the brush radius (about 15 % per notch). |
| `Escape` during a stroke | Drop the whole stroke. |

Entities only land on world brushes and on `func_detail`, `func_brush`, `func_wall` and `func_lod` brushes that have at least one face without a `tools/` material (triggers, clips and other volumes are skipped). Hidden entities are never touched.

An entity "belongs" to a type when its class is one of the classes of the type and, when the type sets models, its model is one of them too. Subtract, Replace, Overwrite and `Ctrl` erasing only act on those entities.

## Modes

| Mode | What it does |
| --- | --- |
| **Add** | Fill empty cells: each empty cell gets an entity with the density as chance. |
| **Subtract** | Remove entities of the type, each one with the density as chance. |
| **Replace** | Pick a new entry for the cells that hold an entity of the type. |
| **Overwrite** | Empty the cells, then fill them again by the density. |

## Options

The options are saved in `settings.toml` under `[placement.sprinkle]`.

| Option | What it does | Default |
| --- | --- | --- |
| **Type:** | The sprinkle type to paint. The tooltip of each type tells the file it comes from. | none |
| Refresh button | Read the definitions again. | |
| Edit button | Open source: the definition file of the type in the text editor (disabled when the file is inside an archive). | |
| Folder button | Open the Hammer Pro sprinkle folder. | |
| Mode | **Add**, **Subtract**, **Replace** or **Overwrite** (see above). | **Add** |
| **Density:** | Chance in percent that a cell gets an entity (or loses it when subtracting), 0 to 100 %. | 50 % |
| **Radius:** | Brush radius in units, 8 to 4096 (`Alt`+wheel). | 128 |
| **Use grid** | Use the editor grid instead of the grid of the definition (the tooltip shows the grid of the type). | Off |
| **Random yaw** | Turn every entity to a random yaw. | On |
| **Offset:** | Random move inside the cell, in percent of half a cell, 0 to 100 %. | 50 % |
| **Z offset:** | Distance from the surface, -4096 to 4096 units. | 0 |
| **Align to normal** | Tilt entities to follow the surface they rest on. The Z offset then goes along the surface normal. | Off |

When the grid is too small for the radius (more than 40,000 cells under the brush), nothing is placed and **The grid is too small for this radius: nothing was sprinkled** is shown: raise the grid or lower the radius.

## Definition files

A definition file is a `.txt` file holding one or more types:

```text
"Jungle foliage"
{
    base { "classname" "prop_static" grid "64 64" }
    10 { "model" "models/props/plant01.mdl" }
    30 { "model" "models/props/plant02.mdl" "skin" "1" }
}
```

- The block name (`"Jungle foliage"`) is the name shown in the **Type:** list (types are sorted by name).
- `base` holds the keyvalues every entity gets, and the `grid` of the type (`"64 64"` for X and Y in the brush plane, or `"64"` for both). Keyvalues written directly in the type block also count as base ones. Without a grid, the type uses 64 x 64.
- Each numbered block is an entry picked with that weight (here `plant02` comes out three times as often as `plant01`). Its keyvalues override the base ones.
- `grid`, `origin` and `id` are never written to the entities.
- A type without a usable entry (no positive weight, or no `classname`) is skipped. Problems found are counted in **{0} problems in the definitions (see the console)** and detailed in the console.

### Where definitions are read

| Place | Notes |
| --- | --- |
| `scripts/hammer/sprinkle/` of the game content | Read through the mounted game content (base game, addons, mounted folders). |
| `bin/hammerplusplus/scripts/sprinkle/` and `bin/win64/hammerplusplus/scripts/sprinkle/` of the Garry's Mod install | The Hammer++ folders, read only. |
| `bin/scripts/hammer/sprinkle/` and `bin/win64/scripts/hammer/sprinkle/` of the Garry's Mod install | The Hammer folders, read only. |
| `%APPDATA%\HammerPro\sprinkle` | The Hammer Pro folder (folder button of the options bar). |
| Extra folders | Added in **Settings > Prefabs > Sprinkle** with **Add a folder...**, saved as `extra_dirs`. |

Only `.txt` files are read, without subfolders, up to 1 MB per file and 512 files per place. Files are read on a worker thread and kept in a cache by modification time, so a refresh only parses changed files. Definitions are read again with the Refresh button, when you change the folder list, and when the game content is mounted again.

## Tips

- Use **Overwrite** with a low density to thin out and reshuffle an area in one pass.
- For grass on a slope, turn on **Align to normal** and give a small negative **Z offset:** so the base does not float.
- AI assistants can scatter entities through MCP with the `sprinkle` tool (weighted items, count, spacing, random yaw, alignment, maximum slope), and read or change the tool options with `get_tool_options` / `set_tool_options` (tool `sprinkle`). See the [MCP tool reference](../../MCP.md).

## FAQ

**The Type list is empty.** No definition was found. Put a `.txt` file in `%APPDATA%\HammerPro\sprinkle` (folder button), or add your folder in **Settings > Prefabs > Sprinkle**, then click Refresh.

**Nothing appears on my prop or trigger.** Entities land on brush surfaces only (world brushes and `func_detail`, `func_brush`, `func_wall`, `func_lod`); models and tool brushes are skipped.

**Ctrl erased entities I placed by hand.** Erasing removes every visible entity that belongs to the type (same class and, if the type sets models, same model). Hide the ones you want to keep.

**I edited a definition and nothing changed.** Click the Refresh button of the options bar.

## Related pages

- [Scatter by material](scatter-by-material.md)
- [Prefabs](prefabs.md)
- [Entity tool](entity-tool.md)
- [Model browser](model-browser.md)
- [Hide and isolate](hide-isolate.md)
- [Settings window](settings-dialog.md)

---
[Back to the documentation index](../README.md)
