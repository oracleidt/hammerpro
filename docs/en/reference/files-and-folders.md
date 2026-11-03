[Documentation](../README.md) > Reference > Files and folders | [Français](../../fr/reference/files-and-folders.md)

# Files and folders

Every file and folder Hammer Pro reads or writes outside of your own maps, and the files it writes next to them. Paths are given for Windows: `%APPDATA%` is usually `C:\Users\<you>\AppData\Roaming` and `%LOCALAPPDATA%` is `C:\Users\<you>\AppData\Local`.

Automated runs (`--script`, `--screenshot`, `--bench-open`) write none of the per-user files below: see [Command line](command-line.md).

## Per-user files (roaming): `%APPDATA%\HammerPro`

| Path | What it is |
|---|---|
| `settings.toml` | Every setting, in TOML: one table per area (`[view]`, `[edit]`, `[ui]`, `[game]`, `[compile]`, `[mcp]`...). Only tables that differ from the defaults are written, so the file stays short. Unknown or missing keys are tolerated, and an invalid file is replaced by the defaults (a warning is logged). Its absence means first start: the first run wizard is shown. Keys: [Settings reference](settings.md). |
| `crash\` | Crash reports: `crash-<time>.txt` for a crash of the editor, `crash-<time>-worker.txt` for a background thread that failed while the editor kept running. `pending.txt` points at the report to show at the next start, and `running-<pid>.txt` marks a running session (removed on a normal close, so a session that died without a report is detected at the next start). |
| `prefabs\` | The prefab library, where new prefabs are written. Another folder can be chosen in the settings (`placement.prefab_library`). |
| `sprinkle\` | Your own sprinkle definitions. |

## Per-user files (local): `%LOCALAPPDATA%\HammerPro`

| Path | What it is |
|---|---|
| `autosave\` | Autosaves and emergency saves: `<map>_<hash>_<unix seconds>.vmf`, where the hash identifies the full path of the map (two maps with the same name never share a rotation). By default every 5 minutes, 10 copies kept per map (settings `edit.autosave_minutes` and `edit.autosave_keep`, `0` minutes turns autosave off). |
| `logs\hammerpro.log` | The log of the current session, written line by line. The previous session is kept as `hammerpro.old.log`. |
| `mcp\instances\<pid>.json` | The record of each window whose MCP server is on (port and secret token). Removed when the server stops. See [MCP server](mcp-server.md). |
| `maps\<map key>.toml` | The sidecar file of each map (see below). |
| `thumbs\` | Thumbnails of materials and models that are slow to make, kept between sessions. |
| `prefab_thumbs\` | Thumbnails of the prefab library. |
| `pack\` | Cache of the files packed into compiled maps (converted images, extracted files). |
| `compile_content\` | Generated game folder that makes your custom materials (custom texture folders, plain images) visible to the compiler. |
| `builds\` | Stage cache of the compiler: an identical geometry or lighting step is restored instead of run again. |
| `content_index.bin` | The saved index of the game, custom and mounted folders, so launches only scan what changed (see [Content index and scan](../features/content-index.md)). |
| `compile_history\<map key>.jsonl` | Timing history of the compiles of each map (History tab of the compile window). |
| `progressive\`, `spec\` | Builds of the progressive compile (play now) and of the background precompile. |
| `untitled\` | Compile folder of maps that were never saved. |
| `release\<map>\` | Working files of **Build and save as content**. |
| `propsrc\` | Source files of the models made by Propper. |
| `decompiled\` | Content copied out of compiled maps opened with **BSP Decompiler...**. |
| `sniffer\` | Default output folder of the [BSP Sniffer](../features/bsp-sniffer.md) (VMF, BSP, content and report of each run; setting `sniffer.output_dir`). |

Everything in this folder except `autosave\` and `logs\` is a cache: you can delete it while Hammer Pro is closed, it is rebuilt when needed (the sidecar files only hold conveniences, see below).

## The sidecar file of each map

Editor data that does not belong in the VMF is kept per map in `%LOCALAPPDATA%\HammerPro\maps\<map key>.toml`, where the key is the map file name followed by a hash of its full path (`untitled` for a map never saved). It is read when the map opens and written (atomically) when the map is saved and when it is closed. A missing or damaged sidecar simply reads as defaults: it is never the only copy of your work.

| Section | What it remembers |
|---|---|
| `startup_view` | The 3D and 2D cameras and the active tool, restored when the map opens. |
| `workflow` | The exact grid size (the VMF only stores a whole number for Hammer). |
| `game` | The game of the map (see [Port a map to another game](../guides/port-to-another-game.md)). |
| `compile` | The last successful compile of each build of the map, for smart recompiles. |
| `content` | The Build and save as content options: addon title, tags, ignore list, description, output folder, Workshop id, last change note, last release summary, nav mesh and cubemap fingerprints, and the secret salt of the material randomizer of Protect your BSP. |
| `health_ignored` | The problems you chose to ignore. |
| `placement` | The counter of the `&i` prefab token. |
| `startup_export` | The last export, for "Export again". |

## Files next to your map

| Path | What it is |
|---|---|
| `<map>.vmf` | Your map. A compile never saves it or changes it. |
| `.hammerpro_build\` | Compile folder: the snapshot of the map being compiled (`<map>.vmf`, or `<map>_cordon.vmf` when only the cordon is compiled) and what the compiler writes next to it: `<map>.bsp`, `<map>.log`, `<map>.lin` when the map leaks, `<map>.prt` for a sealed map. `.hammerpro_build\portals\` holds the geometry only builds of **Build portals (geometry only)**. |
| `<map>.nav` | The navigation mesh made by Hammer Pro (where the nav mesh viewer and Build and save as content look for it). |
| `<map>_content\` | Default output folder of **Build and save as content** (another one can be chosen). |

When you run `hammerpro --bspc` yourself, the `.bsp`, `.log`, `.lin` and `.prt` files are written next to the map you give it.

### The Build and save as content folder

```text
<out>/<addon>/addon.json
<out>/<addon>/maps/<map>.bsp, <map>.nav, thumb/<map>.png
<out>/<addon>/materials/..., models/..., sound/...   (Workshop addon)
<out>/<addon>.gma
<out>/<addon>_icon.jpg
<out>/fastdl/...                                     (server kit)
<out>/.hammerpro_release.toml
```

`.hammerpro_release.toml` is the release manifest: for every file a release wrote, the size and modification time of its source and of the written copy. The next build skips unchanged files and rewrites the GMA only when its inputs changed. Hammer Pro only ever deletes files listed in this manifest.

## Files in the game folder

| Path | What it is |
|---|---|
| `garrysmod\maps\<map>.bsp` | The compiled map, copied there after a compile (option `compile.copy_to_maps`, on by default; another folder can be set in `compile.maps_dir_override`). |
| `garrysmod\addons\hammerpro_dev\` | The development addon: Draft and Normal compiles do not pack your custom materials into the BSP, they are copied into this folder addon instead so the map looks right in game. The copy is incremental and never writes outside this folder. With the default pack policy, Final compiles and releases pack. Setting `compile.dev_addon`, on by default. |
| `garrysmod\addons\hammerpro_bridge\` | The HammerPro bridge addon, a small Lua file installed only on request (never packed into a map). It places the player at the camera once the map loads, fires inputs and sends back the props you placed in game for **Import props from the game**. |
| `garrysmod\data\hammerpro\` | Files the editor and the game exchange through the bridge: `cmd.json` (written by the editor), `ack.json` and `props.json` (written by the game). |
| `garrysmod\console.log` | Written by the game when the `-condebug` fast start option is on; Hammer Pro reads it for the map load report. |

## Files of the update

| Path | What it is |
|---|---|
| `hammerpro.exe.old` | Next to the program: the version replaced by the last update, kept as a backup (see [Updates](../features/updates.md)). |
| `%TEMP%\HammerPro-update-<process>-<tag>\` | The downloaded release archive. Removed at a later start. |
| `%TEMP%\HammerPro-updater-<process>\` | The copy of the program that installs the update, and its `update.log`. Removed at a later start. |

## Related pages

- [Settings reference](settings.md)
- [Autosave and crash recovery](../features/autosave-crash.md)
- [Crashes and logs](../troubleshooting/crashes-and-logs.md)
- [Game launch](../features/game-launch.md)
- [Build and save as content](../features/build-content-release.md)
- [VMF compatibility](../getting-started/vmf-compatibility.md)
- [Compile window](../compiler/compile-window.md)

---
[Back to the documentation index](../README.md)
