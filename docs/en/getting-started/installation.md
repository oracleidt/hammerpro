[Documentation](../README.md) > Getting started > Installation | [Français](../../fr/getting-started/installation.md) | [Русский](../../ru/getting-started/installation.md)

# Installation

Hammer Pro 1.0.2 is a single program, `hammerpro.exe`, with no installer. It brings its own map compiler, so you do not need Valve's VBSP, VVIS or VRAD.

## Requirements

| What | Needed |
|---|---|
| System | Windows 10 or 11, 64-bit. Linux builds compile from source too. |
| Graphics card | Vulkan, DirectX 12 or Metal support. |
| Game | Garry's Mod installed through Steam (found automatically). Other Source games are supported as compile targets, see [Game setup](game-setup.md). |

## Download and install

1. Download the release of Hammer Pro (it contains `hammerpro.exe`), or build it yourself (see [Building from source](../development/building.md)).
2. Put `hammerpro.exe` in any folder you like, for example `C:\Tools\HammerPro\`. Nothing else has to be copied.
3. Run it. A shortcut on the desktop or in the Start menu is optional.

## First start

- With no settings file yet, Hammer Pro opens the **Hammer Pro setup** assistant: language, the editor you come from (Hammer, Hammer++ or the Hammer Pro defaults), the game folder and the content to load. See [Game setup](game-setup.md).
- The interface language starts as the language of your system (English, French or Russian). You can force it for one run with `hammerpro --lang en` or `--lang fr` (see [Command line reference](../reference/command-line.md)).
- The game content (VPKs, addons, Workshop) mounts in the background: the editor is usable while it loads, and a card over the views shows the progress.
- You can open a map directly: `hammerpro path\to\map.vmf`, or drag a `.vmf` onto the window.

## Updating

Replace `hammerpro.exe` with the new one. Your settings, autosaves and per map data stay where they are: unknown or missing settings keys are tolerated, so old and new versions can share one settings file.

## Where files go

| What | Windows location |
|---|---|
| Settings (`settings.toml`) | `%APPDATA%\HammerPro\` |
| Crash reports | `%APPDATA%\HammerPro\crash\` |
| Log file (`hammerpro.log`, previous run `hammerpro.old.log`) | `%LOCALAPPDATA%\HammerPro\logs\` |
| Autosaves and emergency saves | `%LOCALAPPDATA%\HammerPro\autosave\` |
| Caches (thumbnails, per map editor data in `maps\`) | `%LOCALAPPDATA%\HammerPro\` |

Hammer Pro may also write into the game: compiled maps go to `garrysmod/maps` (option **Copy to the maps folder** of the compile window), custom materials of test builds to the `garrysmod/addons/hammerpro_dev` addon, and the optional bridge addon to `garrysmod/addons/hammerpro_bridge`. The full list is in [Files and folders](../reference/files-and-folders.md).

## Uninstalling

1. Delete `hammerpro.exe`.
2. To remove your settings and data too, delete the `HammerPro` folders in `%APPDATA%` and `%LOCALAPPDATA%`.
3. If you used them, delete `garrysmod/addons/hammerpro_dev` and `garrysmod/addons/hammerpro_bridge` (the bridge can also be removed with **Remove** in **Settings > Game**).

Your `.vmf` maps are never stored inside these folders: they stay where you saved them.

## FAQ

**Do I need the Source SDK or Hammer installed?** No. Hammer Pro reads the game content directly and compiles maps itself. Hammer++ is not needed either, but when it is installed its FGD is loaded too.

**Can I keep using Hammer or Hammer++?** Yes. Hammer Pro reads and writes standard `.vmf` files, see [VMF compatibility](vmf-compatibility.md).

**The window opens but stays empty or crashes at once.** See [Crashes and logs](../troubleshooting/crashes-and-logs.md) (graphics backends and the native window frames option).

## Related pages

- [Game setup](game-setup.md)
- [Interface tour](interface-tour.md)
- [Your first map](first-map.md)
- [Files and folders](../reference/files-and-folders.md)
- [Building from source](../development/building.md)

---
[Back to the documentation index](../README.md)
