[Documentation](../README.md) > Getting started > VMF compatibility | [Français](../../fr/getting-started/vmf-compatibility.md) | [Русский](../../ru/getting-started/vmf-compatibility.md)

# VMF compatibility

Hammer Pro opens and saves standard `.vmf` files, so you can use it side by side with Hammer and Hammer++ on the same maps. This page covers opening, saving, exporting and what is kept in the file.

## Opening maps

| How | What it does |
|---|---|
| **File > Open map...** (`Ctrl+O`) | Opens a `.vmf`, `.vmx` or `.vmm` file. A Quake style `.map` (Valve 220) can be imported too: it opens as a new document, use **Save as** to keep it as a VMF. |
| Drag and drop | Drop a `.vmf`, `.vmx` or `.vmm` on the window to open it. Other dropped files are routed too: pointfiles (`.lin`, `.pts`) load, portal files (`.prt`) and packs (`.bsp`, `.gma`, `.vpk`) open in their viewer, images open the texture import and models (`.fbx`, `.obj`, `.gltf`, `.glb`) the model import. |
| **File > Recent maps** | The last 12 maps, numbered. **Clear recent maps** empties the list (also in **Settings > General**). |
| Command line | `hammerpro path\to\map.vmf`. |
| [Command palette](../features/command-palette.md) | `Ctrl+P`, then type the name of a recent map. |

Maps open in the background: the current map stays usable and the status bar shows "Opening <name>... N s" with a button to stop. Each opened map gets its own tab; a map already open is shown instead of being loaded twice (see [Several open maps](../features/multiple-maps.md)). With **Settings > General > Open maps in new tabs** off, an opened map replaces the current one after asking to save its changes.

When a newer autosave of the map exists, Hammer Pro asks **Recover autosave?** with **Open the autosave** or **Keep the saved map** (see [Autosave and crash recovery](../features/autosave-crash.md)).

### Damaged files

Parsing is lenient. Keys without a value and stray closing braces are ignored, objects without a valid id get a new one, and values that cannot be read are kept unchanged; a toast says "Opened <name> with N problems" and the console lists them. A truncated file (unclosed blocks, a string cut in the middle) opens as a new document named "<name> (damaged)", so that saving cannot overwrite the damaged file: use **Save as** to keep it under another name.

## Saving

| Command | What it does |
|---|---|
| **File > Save** (`Ctrl+S`) | Saves the map. The save is atomic (a temporary file, then a rename) and, like Hammer, the previous file is first copied to `<name>.vmx`. |
| **File > Save as...** (`Ctrl+Shift+S`) | Saves under a new name. The dialog starts in the **Map folder** of **Settings > Game** when set. |
| **File > Save as new version...** | Asks for a new map name (proposed: `gm_x` gives `gm_x_v2`, `gm_x_v2` gives `gm_x_v3`; lowercase letters, digits, `_` and `-` only), saves the map under it, then copies the compiled map (with its generated paths renamed so built cubemaps keep working), its navigation mesh and its per map scripts (particles, level sounds, soundscapes) under the new name. Existing files are never replaced. A map that was never saved gets **Save as** instead. |

Like Hammer, saving writes the current 3D camera and the grid settings (`nGridSpacing`, `bSnapToGrid`, `bShowGrid` of `viewsettings`) into the map.

## Lossless round trip

- Anything Hammer Pro does not understand (unknown keys, unknown blocks, undecodable data) is kept and written back unchanged. Loading and saving a file never silently drops data.
- The text format found at load is kept: Windows-1252 text, CRLF line endings and Hammer's number spelling. A map written by Hammer and saved untouched comes back out as Hammer wrote it.
- Ids, groups, visgroups, cordons, cameras, hidden objects and entity connections are kept as in Hammer.

### Hammer++ data

The blocks Hammer++ adds are kept: the exact face vertices of `vertices_plus` in each side, and the map level `*_plus` blocks (`palette_plus`, `colorcorrection_plus`, `light_plus`, `bgimages_plus` and the others). A map can go back and forth between Hammer++ and Hammer Pro.

## The sidecar file

Editor data that does not belong in the VMF is stored apart, in one small TOML file per map: `%LOCALAPPDATA%\HammerPro\maps\<map key>.toml` (the key identifies the full path of the map). It holds conveniences such as the view state, ignored problems, the last compile fingerprint, the target game of the map, the last export path and a Workshop id. It is written when the map is saved and closed. A missing or broken sidecar simply reads as defaults: it is never the only copy of your work, and the VMF stays a plain VMF for Hammer. See [Files and folders](../reference/files-and-folders.md).

## Importing and exporting

| Command | What it does |
|---|---|
| **File > Import > Import map into current...** | Merges another VMF into the open map, as one undo step. Groups, visgroups and hidden objects are kept. |
| **File > Export > Export selection as VMF...** | Writes the selected objects to a new VMF (default name `prefab.vmf`), with the groups they belong to and their visgroups. |
| **File > Export > Export a copy...** | Writes the whole map to another VMF without renaming the open document. The path is remembered with the map. |
| **File > Export > Export again** (`Alt+B`) | Writes the copy again to the last export path of this map, without asking. |
| **File > Export > Export OBJ...** | Brush and displacement geometry with texture coordinates, for 3D tools (see [BSP tools](../features/bsp-tools.md)). |

Exports are written on a worker thread, atomically.

## FAQ

**Can I open my map in Hammer again after editing it in Hammer Pro?** Yes. The file is a standard VMF. Most data specific to Hammer Pro lives in the sidecar file. The exception is the custom grids of the map, written in a `hammerpro_grids` block of the VMF (see [Grids](../features/grids.md)).

**Will Hammer++ lose its extra data if I save in Hammer Pro?** No. `vertices_plus` and the `*_plus` blocks are written back.

**What is the `.vmx` file next to my map?** The previous version of the map, copied at each save like Hammer does. Open it with **File > Open map...** to go back.

**Can Hammer Pro open a compiled `.bsp`?** Yes, with **File > Import > BSP Decompiler...**, which decompiles it into a new map (see [BSP tools](../features/bsp-tools.md)).

## Related pages

- [Several open maps](../features/multiple-maps.md)
- [Autosave and crash recovery](../features/autosave-crash.md)
- [Clipboard](../features/clipboard.md)
- [BSP tools](../features/bsp-tools.md)
- [Files and folders](../reference/files-and-folders.md)

---
[Back to the documentation index](../README.md)
