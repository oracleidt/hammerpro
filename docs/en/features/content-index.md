[Documentation](../README.md) > Features > Content index and scan | [Français](../../fr/features/content-index.md)

# Content index and scan

At launch Hammer Pro lists every file of the game folders, the custom texture folders and the mounted games (a typical setup is more than 300 000 files). The first launch scans the disk. The next launches, even after a restart of the computer, read one small file instead and only look again at the folders that changed.

## The saved index

- The index is a single binary file, `content_index.bin`, in the cache folder of Hammer Pro (`%LOCALAPPDATA%\HammerPro` on Windows). It holds, for every scanned folder tree, the file names and the modification date of every folder.
- At launch every folder is compared with its saved modification date. A folder that did not change keeps its saved list; a changed folder is listed again; a new folder is scanned; a removed folder is dropped. Adding, deleting or renaming a file changes the date of the folder that holds it, so it is seen at the next launch (or at **Reload content from disk**).
- A folder modified in the last seconds before the scan is never trusted at the next launch, so a change made right while the scan ran cannot be missed.
- The index is keyed by the folder path, normalized (case, slashes, trailing slash, `..`, `\\?\` prefix, symbolic links and junctions). Changing the custom folders, the extra game folders, the Steam games or `mount.cfg` simply adds the new folders to the index; entries not used for 30 days are forgotten.
- VPK and GMA archives are opened directly every time (their own file table is read from the archive, which takes a few milliseconds), so a changed archive is always seen.
- The file is written to a temporary file and then renamed, so a crash cannot leave a half written index. A missing, truncated, corrupt or foreign file (another version of Hammer Pro, a checksum that does not match) is ignored with a line in the log: a full scan runs and a good index replaces it.

## Settings > Game > Content scan

| Control | What it does |
|---|---|
| **Keep a content index on disk** | Turns the saved index on or off (on by default). Off: every launch scans everything and the file is not written. |
| **Folder scan** | **Auto** (default), **Always in parallel** or **Always one at a time**. Applied at the next content load. |
| **Rebuild index** | Deletes the saved index and loads the game content again with a full scan. Also the action **Rebuild the content index** (`rebuild_content_index`, command palette or Tools menu). |

The line under the buttons shows the number of saved folder trees, how many folders were reused or scanned again since launch, the duration of the last folder scan and how many hard disks were seen.

## Hard disks and SSDs

On a hard disk, scanning many folders at the same time makes the head jump around and is slower than reading them one after the other. On an SSD or NVMe it is the opposite. In **Auto**, Windows is asked (the "seek penalty" property of the storage device of each volume) what kind of drive holds each folder:

- rotational drive (hard disk): its folders are scanned one at a time;
- SSD, NVMe or a drive that does not answer (for example some USB bridges or network shares): scanned in parallel.

The choice is made per volume, so a game on `D:` (hard disk) and custom folders on `C:` (NVMe) are scanned each in their best way, at the same time. **Always in parallel** and **Always one at a time** ignore the drive type.

## Duplicate folders

A folder declared twice is mounted once, at the priority of its first declaration. This covers a custom folder that is also a `mount.cfg` entry, an extra game folder that is also mounted by `mount.cfg` or by AppID, and the same folder written differently (`D:\Games\X`, `d:/games/x/`, `D:\Games\Y\..\X`, `\\?\D:\Games\X`, a junction or symbolic link to it). A custom folder of plain images (without a `materials` sub folder) is a different view of the folder and is kept next to a game mount of the same folder.

## MCP

- `get_content_index`: the state of the index (file, size, entries), the folders checked, reused and scanned again, the last scan duration, the scan mode and the type of each drive seen.
- `rebuild_content_index`: same as **Rebuild index**.
- `run_action` with `rebuild_content_index`.
- `set_setting` with `scan.use_index` (`true` or `false`) and `scan.mode` (`"auto"`, `"parallel"`, `"sequential"`), applied at the next content load.

## Questions

**The index seems out of date.** Click **Rebuild index**. Network shares and some file systems do not update folder dates reliably; turn the index off for them or rebuild after a change.

**Is the index shared between computers?** No, it lives in the cache folder of your user profile and can be deleted at any time.

## See also

- [Game setup](../getting-started/game-setup.md)
- [Custom texture folders](custom-texture-folders.md)
- [Settings dialog](settings-dialog.md)
- [MCP tool reference](../../MCP.md) (`get_content_index`, `rebuild_content_index`)

---
[Back to the documentation index](../README.md)
