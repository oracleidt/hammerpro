[Documentation](../README.md) > Getting started > Game setup | [Français](../../fr/getting-started/game-setup.md)

# Game setup

Hammer Pro reads textures, models and entity definitions from a game folder and compiles maps for that game. Garry's Mod is the default. This page covers the first run assistant and the **Game** page of the settings.

## The setup assistant

The **Hammer Pro setup** assistant opens on the first start (when no settings file exists). Run it again at any time with **Help > Setup assistant...**. It has 3 steps; `Enter` goes to the next step, **Back** returns, **Skip** (or `Escape`) closes it and keeps the current settings.

| Step | What you choose |
|---|---|
| 1. Welcome | The interface language (English or French), and **Coming from Hammer or Hammer++?**: **No, Hammer Pro defaults**, **Hammer** or **Hammer++**. This sets the keyboard shortcut preset and the camera controls together; shortcuts you changed yourself stay. |
| 2. Which game do you map for? | The game (see below). For Garry's Mod, its folder is searched in your Steam libraries the first time this step shows; "Everything needed was found." confirms it. |
| 3. Content | **Mount addons**, **Mount Workshop downloads**, **Use mount.cfg games** (Garry's Mod only) and your **Custom texture folders**. |

**Start mapping** on the last step saves the choices, mounts the content again and loads the entity definitions ("Setup complete. Loading game content...").

The language can be changed later in **View > Language** or in **Settings > General**. The shortcut preset is in **Settings > Keyboard** and the camera controls in **Settings > Viewports** (see [Keyboard shortcuts](../features/keyboard-shortcuts.md) and [Camera and navigation](../features/camera-navigation.md)).

## The Game settings page

Open **Tools > Options...** (`Ctrl+,`), page **Game**. Every change is saved at once; content changes take effect when you click **Apply and reload content** at the bottom of the window, or when you close it.

### Game

| Option | What it does | Default |
|---|---|---|
| Game list | The game maps are made for: its content is mounted, its entity definitions loaded, and maps compile in its format and limits. Games found in the Steam libraries come first; games that are not installed are listed too (maps still compile in their format). Mods of `steamapps/sourcemods` are listed as well. | Garry's Mod |
| **Search again** | Searches the Steam libraries again (after installing or removing a game). | |
| **Game folder** (not Garry's Mod) | The folder holding `gameinfo.txt`. Empty: found in Steam. A summary below shows the folder found, the format ("Format: ..., BSP version ..., static props ..., limits"), the FGDs found and the game executable. | Empty |

A map can also have its own game, different from this default: the **Target game** of the compile window, kept with the map (see [Target games](../compiler/target-games.md)). Addons, the Workshop, `mount.cfg`, the Lua skybox switch, testing in game with the bridge addon and Workshop publishing are Garry's Mod features: they are off for other games.

### Garry's Mod folder

| Option | What it does | Default |
|---|---|---|
| **Garry's Mod folder** | The install folder (the one holding `garrysmod` and `bin`). Empty: detected at startup from Steam. Set it to use another install. | Empty (auto) |
| **Detect** | Searches your Steam libraries now. | |
| **Browse...** | Picks the folder by hand. | |
| Check result | "VPK archives: N" and "Addons: N", with **Check again**. A folder without `garrysmod/gameinfo.txt` is reported as not a Garry's Mod install. | |

### Content (Garry's Mod)

| Option | What it does | Default |
|---|---|---|
| **Mount addons** | Loads materials and models from `garrysmod/addons` (folder addons and `.gma` files). | On |
| **Mount Workshop downloads** | Loads the content of your subscribed Workshop addons, `.gma` and legacy `.bin` (slower startup). | Off |
| **Use mount.cfg games** | Mounts the other Source games listed in `garrysmod/cfg/mount.cfg`. | On |
| **Mount the content downloaded from servers** | Loads `garrysmod/download` and `garrysmod/cache/workshop`. Players may not have these files: the release checks pack what the map uses. | On |

Workshop and downloaded content open after the rest, in the background: the status bar shows "Workshop N / M" until they are ready.

Priority order, highest first: custom texture folders, addons, `garrysmod.vpk`, loose `garrysmod` files, the other VPKs of `gameinfo.txt`, gamemode `content` folders, loose `sourceengine` files, `mount.cfg` games, extra game folders, Steam games by AppID (in their list order), Workshop items, downloaded content. A file found in a higher source hides the same file below.

### Custom texture folders

Add any folder of VTF/VMT files or plain images (PNG, JPG, TGA, BMP) with **Add folder...**. Each row has an enable box, the path, a **Prefix** and a remove button. Images become materials named `prefix/file` and are converted to VTF + VMT when you compile. The prefix is not used when the folder has a `materials` sub folder or lies inside a `materials` folder; it is "detected" from the VMTs of a ready-made pack. Details: [Custom texture folders](../features/custom-texture-folders.md).

### Extra game folders

Mod folders (with a `gameinfo.txt` or loose content) to mount too. Add with **Add**, remove with the cross.

### Steam games by AppID

Mount the content of other installed Steam games by their AppID, for any target game (Garry's Mod or not): for example 220 Half-Life 2, 240 Counter-Strike: Source, 300 Day of Defeat: Source, 380 Half-Life 2: Episode One, 420 Half-Life 2: Episode Two, 400 Portal, 440 Team Fortress 2, 4000 Garry's Mod.

| Control | What it does |
|---|---|
| **AppID** field and **Add** | Adds one AppID or several (`240, 440`), also with `Enter`. A value that is not a number is refused with a message. |
| **Installed games** | Lists the Source games installed in your Steam libraries, with their AppIDs: click one to add it. |
| Search again (circular arrow) | Searches the Steam libraries again after installing or moving a game. |
| List rows | The AppID and the game name (hover: its game folders). *Not installed* in orange when Steam has no such game on this computer. Up and down arrows change the priority; the cross removes the game. |

Hammer Pro finds each game through Steam: the Steam folder (registry `SteamPath`, else the default install folder), every library of `steamapps/libraryfolders.vdf`, then `steamapps/appmanifest_<AppID>.acf` and its `installdir`. It then mounts what the game's `gameinfo.txt` lists (its VPKs and folders, the base games it builds on), after the content of the map's own game. The asset browsers, the 3D view and the compile (the editor passes the game folders to the compiler with `-insert_search_path`) all use it. Saved as `mount_app_ids` in the `[game]` table of `settings.toml`. With Garry's Mod, `garrysmod/cfg/mount.cfg` still works too (**Use mount.cfg games**). AI assistants use the MCP tools `list_app_mounts` and `set_app_mounts`; the compiler command line takes `-mount_appid 240,440` (see [Compiler command line](../compiler/command-line.md)).

### Entity definitions (FGD)

| Option | What it does | Default |
|---|---|---|
| **Main FGD** | Replaces the game FGD (`bin/garrysmod.fgd` for Garry's Mod, the FGD of the game otherwise). | Empty (game FGD) |
| **Extra FGD files (loaded after the main one)** | Gamemode FGDs, Mapbase, your own. | None |

Load order: the main FGD (or the game's own), then the Hammer++ FGD (`hammerplusplus_fgd.fgd`) when Hammer++ is installed in the game's `bin/win64/hammerplusplus` or `bin/hammerplusplus` folder, then the extra files. `@include`, `@mapsize`, material exclusions and auto visgroups are supported, and an FGD edited on disk is reloaded within seconds. A missing file is reported in the console.

### Maps

| Option | What it does | Default |
|---|---|---|
| **Map folder** | Start folder of **Open** and **Save as**. | Empty |

### Testing in game (Garry's Mod)

The **Hammer Pro bridge** is a small addon in `garrysmod/addons/hammerpro_bridge` that lets Hammer Pro place you at the editor camera and import the props you arrange in game. It is never packed into your maps and only acts in local games (listen servers). **Install**, **Update** and **Remove** manage it; restart the game afterwards. **Spawn at the editor camera** (needs the bridge) places the player where the 3D camera is when the map loads. See [Launching the game](../features/game-launch.md).

## Reloading content

- **Apply and reload content** (bottom of the settings window) mounts the game content again with the new settings.
- **Tools > Reload content from disk** reloads the content and the entity definitions without changing settings, for example after adding files to an addon.

## FAQ

**Garry's Mod is not found.** Click **Detect**, or **Browse...** to its folder (the one holding `garrysmod` and `bin`, usually `steamapps/common/GarrysMod`). The status bar shows "No game content" until it is set; clicking it opens this page.

**Can I map for Counter-Strike: Source or Team Fortress 2?** Yes: choose the game in the list. Its content and FGD are used and the map compiles in its format. See [Target games](../compiler/target-games.md) and [Port a map to another game](../guides/port-to-another-game.md).

**My Workshop textures are missing.** Turn on **Mount Workshop downloads**, then **Apply and reload content**.

**Where do I add a gamemode FGD?** In **Extra FGD files**, on this page.

## Related pages

- [Installation](installation.md)
- [Custom texture folders](../features/custom-texture-folders.md)
- [Settings window](../features/settings-dialog.md)
- [Target games](../compiler/target-games.md)
- [Launching the game](../features/game-launch.md)
- [Settings reference](../reference/settings.md)

---
[Back to the documentation index](../README.md)
