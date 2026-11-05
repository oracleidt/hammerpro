[Documentation](../README.md) > Features > Map assets | [Français](../../fr/features/map-assets.md) | [Русский](../../ru/features/map-assets.md)

# Map assets

The **Map assets** tab lists every file the open map uses (materials with their textures, models with their files and materials, sounds with their scripts, particle files), shows where each file comes from and what is missing, and copies them into a clean folder with **Collect**. A second section lists the files of a content folder (an addon, for example) that the map does not use.

## Opening it

- **Map > Map assets** shows the tab in the right dock. Run it again while the tab is shown to hide it.
- The tab stays in the dock until you hide it (setting `content.show_map_assets`, off by default).
- The game content must be loaded first. Without it the tab says *The game content is not loaded (configure the game in the settings first).* See [Game setup](../getting-started/game-setup.md).

The list is built in the background (*Looking for the files the map uses...*) from the same dependency search the release window uses: brush faces, entity keys, instances, and the editor textures of the materials. Click **Refresh** to search again after big changes.

## The list

The header shows a summary: number of entries, files, textures, model files and total size, plus the number of missing files in red.

| Control | What it does |
| --- | --- |
| **Custom only** | Default view. Files that are not part of the base game (addons, Workshop, custom folders, loose files) and the missing files. |
| **All** | Every file, base game included. |
| **Missing** | Only the missing files. |
| **Search a path...** | Filters the entries by path. |
| **Check all** / **Check none** | Checks or unchecks every listed entry for the collect. |
| **Collapse all** | Folds every expanded entry. |
| **Refresh** | Builds the list again. |

Entries are grouped in **Materials**, **Models**, **Sounds**, **Particles** and **Other**. Click a group name to fold it, click the arrow of an entry to see the files it needs (a model shows its model files and materials, a material its textures). The checkbox of an entry means *Collect this entry and everything it needs*.

Each file line shows a mark, the path, the size and the origin:

| Mark | Meaning |
| --- | --- |
| Filled dot (green) | Custom content: it will be collected. |
| Hollow dot (gray) | Part of the base game: only collected with **Also copy base game assets**. |
| Warning sign | The entry is present but something it needs is missing. |
| Cross (red) | Missing: no mounted content has this file. |

Origins: **Base game**, **Addon**, **Workshop**, **Custom folder**, **Loose file**, **Download**, **Mounted game**, **Extra**, **Generated**, **Missing**. Hover a line for the full path, the kind of file, where it comes from, its size and why the map needs it (which faces, entity keys or instances). Right click a line for **Copy the path**.

## Collect

The **Collect** section copies the checked entries and everything they need into an output folder, under `materials/`, `models/`, `sound/`... Files that live inside VPKs and GMAs are extracted. A report is written in the output folder at the end.

| Option | What it does | Default |
| --- | --- | --- |
| **Output** | Folder to copy the files into (full path). The folder button opens a folder picker. Remembered in `content.collect_folder`. | empty |
| **Also copy base game assets** | Every player already has them: off, only the other files are copied. | off |
| **Replace existing files** | Off: a file the folder already has with other content is kept and listed in the report. On: it is replaced, after a confirmation (**Replace files?**). | off |
| **Collect N files (size)** | Starts the copy. A progress bar and **Cancel** show while it runs. | |

The output folder cannot be a folder of mounted game content (you get an error asking for another folder). Files with the same content as the copy already there are counted as *already there* and not rewritten.

After a collect, the tab shows *Last collect: copied, replaced, already there, kept, failed, missing* with **Open the folder** and **Show the report**.

The report is `<map>_assets.txt` in the output folder (`<map>_assets_2.txt` and so on when a report already exists and **Replace existing files** is off). It lists the output folder, the counts, the bytes written, every file with its outcome (copied, replaced, already there, kept, failed) and the missing files.

## Unused files of a content folder

The collapsible section **Unused files of a content folder** lists the files under `materials`, `models`, `sound` and `particles` of a folder (an addon you are cleaning, for example) that this map does not use. It is a report only: nothing is deleted.

1. Type the folder in **Content folder (an addon...)** or pick it with the folder button.
2. Click **Find unused files** (**Cancel** stops it; the list is then partial).
3. The result reads *N of M files are not used by the map (size)*. Filter the list with **Search a path...**, copy it with **Copy the list**, or open the folder with **Open the folder**.

## MCP

Every part of the tab is reachable from AI apps through the MCP server: `list_map_assets`, `collect_map_assets`, `get_map_assets_status`, `cancel_collect_map_assets` and `find_unused_assets`. See the [MCP tool reference](../../MCP.md).

## Tips

- Keep **Custom only** to see what you must ship; switch to **Missing** before a release to fix broken names.
- To prepare a Workshop addon, the release window ([Build and save as content](build-content-release.md)) packs the files for you. Collect is for a hand made addon, a backup or to send the content to a teammate.
- The unused files report is handy before you publish an addon that grew over several versions of a map.

## FAQ

**Why is a file marked Missing when the map looks fine in the editor?** The editor may draw a placeholder (purple and black checkerboard) or the file comes from content that is no longer mounted. Check the game setup and the custom folders, then click **Refresh**.

**Does Collect change my map?** No. It only reads the map and copies files.

**Are files from Workshop items collected?** Yes, they are custom content (origin **Workshop**) and are extracted from their GMA.

**Can it delete the unused files?** No. The section only lists them. Delete them yourself after checking the list.

## Related pages

- [Build and save as content](build-content-release.md)
- [Collect content for a release](../guides/collect-content-for-release.md)
- [Check for problems](check-problems.md)
- [Custom texture folders](custom-texture-folders.md)
- [MCP tool reference](../../MCP.md)

---
[Back to the documentation index](../README.md)
