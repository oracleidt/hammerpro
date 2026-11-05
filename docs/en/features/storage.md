[Documentation](../README.md) > Features > Storage | [Français](../../fr/features/storage.md) | [Русский](../../ru/features/storage.md)

# Storage (disk usage)

Hammer Pro keeps settings, caches, autosaves and temporary files on your disk. The **Storage** page shows how much space they use, what uses the most, and lets you delete what you do not need. Only Hammer Pro's own folders are listed and cleaned: your maps, your game folders and the files next to your maps are never part of it.

## Opening it

**Settings > Storage** (**Tools > Options...**, `Ctrl+,`). The page scans the folders on a background thread when it opens: the editor never waits, and a progress bar shows the locations and files counted so far.

## The list

The total is shown at the top, then one row per kind of data, **largest first**. Every row has:

| Part | What it shows |
| --- | --- |
| Checkbox | Selects the row for **Delete selected**. |
| Name and description | What the data is and what happens if it is deleted. A colored tag marks **Recovery data** and **Your data**. |
| Path | The folder (hover for all folders of the row). |
| Size and files | The size on disk and the number of files, and how many items are in use and skipped. |
| Bar | The share of the total, in percent. |
| **Open folder** | Opens the folder in the file manager. |
| **Delete** | Deletes the row after a confirmation. |

## What is listed

| Row | Content | Risk |
| --- | --- | --- |
| Thumbnails | Material, model and prefab thumbnails | safe, drawn again |
| Pack cache, Compile content, Compile output, Compile temporary files, Compile history, Release builds | Files of the compiler and the release packer | safe, rebuilt on the next compile |
| Decompiled maps and extracted assets | Maps decompiled for the lab, models extracted from games | safe, extracted again |
| BSP Sniffer results | VMF, BSP, content and report of each [BSP Sniffer](bsp-sniffer.md) run (default folder) | your data |
| Logs | Logs of the previous run (the log being written is kept) | safe |
| Content index | The saved index of game content | safe, rebuilt by the next content scan |
| Update downloads | Downloaded update packages in the system temporary folder | safe |
| Temporary files | `hammerpro_*` items in the system temporary folder | safe |
| Other cached data | Anything else in the cache folder | safe |
| Autosaves and backups | Recovery copies of maps being edited | recovery data |
| Crash reports | Reports and emergency saves | recovery data |
| Per map state | Saved view and layout of each map | your data |
| Settings | settings.toml (options, key bindings, recent maps) | your data |
| Prefabs, Sprinkle presets | What you created in the default folders | your data |

The row order follows the sizes, not this table. Files in use (the log being written, the markers of running sessions, temporary files of this session) are counted but skipped when deleting. Folders that other features add to Hammer Pro's cache are included in the list as they appear.

## Deleting

- **Delete** on a row, or tick rows and press **Delete selected (N, size)**.
- A confirmation window lists exactly what will be deleted (name, size, folders) and the space freed. Rows with **Your data** or **Recovery data** add a clear warning, because that data cannot be recovered.
- The deletion runs on a background thread with a progress bar. When it ends, a summary shows the space freed and, for each item that could not be deleted (in use, no permission), the path and the reason. The list refreshes by itself.
- Compile related rows cannot be deleted while a compile runs.
- Every path is checked before deletion: it must resolve inside the folder of its row. Anything else is refused. Links are removed, never followed.
- The folders themselves stay, only their content is deleted.

## Tips

- Deleting **Thumbnails** and **Compile output** is always safe and often frees the most.
- Keep **Autosaves and backups** until your maps are saved and compiled.
- Deleting settings.toml here removes the saved file, which is written again when you change an option or quit.

## FAQ

**Does it delete my maps?** No. Only Hammer Pro's own folders are cleaned. The `.hammerpro_build` folders next to your maps are not part of it.

**Why is a size not zero after deleting?** Files in use are skipped and listed in the summary. Close the program using them and press **Refresh**.

**Where are the folders?** Under `%APPDATA%\HammerPro` (settings, prefabs, crash reports) and `%LOCALAPPDATA%\HammerPro` (caches, autosaves, logs) on Windows, and in the system temporary folder.

## MCP

AI assistants use `get_storage_usage` (total and the sorted list with ids, sizes, risks and paths) and `delete_storage` (ids, and `confirm` true to really delete; `allow_user_data` true as well for settings, prefabs, presets and per map state). Without `confirm` nothing is deleted and the tool reports what would be. See the [MCP tool reference](../../MCP.md).

## Related pages

- [Autosave and crash recovery](autosave-crash.md)
- [Settings window](settings-dialog.md)
- [Content index and scan](content-index.md)

---
[Back to the documentation index](../README.md)
