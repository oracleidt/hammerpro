[Documentation](../README.md) > Guides > Port a map to another game | [Français](../../fr/guides/port-to-another-game.md) | [Русский](../../ru/guides/port-to-another-game.md)

# Port a map to another game

Hammer Pro is made for Garry's Mod first, but its compiler also writes maps for other Source games: Half-Life 2 and its episodes, Counter-Strike: Source, Team Fortress 2, Portal, Portal 2, Left 4 Dead 2 and more (the full list is in [Target games](../compiler/target-games.md)). This guide moves a map from Garry's Mod to another game.

## How the game of a map works

- The **default game** is set in **Tools > Options...**, page **Game**, field **Game**. Every map without a game of its own uses it.
- Each map can have **its own game**, chosen in the compile window (**Target game**). It is kept with the map, in its sidecar file (section `game`), not in the VMF: Hammer and other editors are not affected.
- When the active game changes, its content is mounted and its entity definitions (FGDs) are loaded again. The compile writes its BSP format, checks its limits and runs its lighting passes.

## Step 1: install the game

Install the target game through Steam. Hammer Pro reads the Steam libraries and finds it by itself. In the game lists, installed games come first; the others are marked **(not installed)**. If the game is installed outside Steam's folders, choose its folder on the **Game** page of the settings (**Game folder**, with **Search again** to read the Steam libraries again).

## Step 2: make a copy of the map

Use **File > Save as new version...** or save a copy under a new name (for example `de_mymap.vmf` from `gm_mymap.vmf`) so the Garry's Mod version stays intact. The game of a map belongs to its file, so the copy can target another game.

## Step 3: choose the target game

1. Open the copy and press `Ctrl+F9` (**File > Compile and run...**).
2. In the **Target game** row at the top, pick the game. Hammer Pro mounts its content and loads its FGDs.
3. The line under the row tells what compiling for it means: its BSP version and static prop version, **Valve's limits** (Garry's Mod has raised limits), and **Not installed** when the game is missing (then the map compiles with the content mounted now, and is neither copied to a game nor played).

## Step 4: fix what does not exist in the new game

- **Content**: textures and models of Garry's Mod or of your addons may not exist in the new game. Open **Map > Map assets** and filter **Missing**, then replace them with **Tools > Find and replace textures...** (`Ctrl+Shift+R`) or by editing the entities.
- **Entities**: some classes are Garry's Mod only. Run **Map > Check for problems** (`Alt+P`): unknown classes and missing targets are listed with their objects.
- **Garry's Mod features**: addons, the Workshop, `mount.cfg`, the Lua skybox switch (`point_hammerpro_skybox`), testing in game with the bridge addon and Workshop publishing are turned off for other games. **Build and save as content** is Garry's Mod only: compile the other games from the compile window (`F9`).
- **Limits**: the map now has Valve's limits. Check **Map > Map information** and the compile log.

## Step 5: compile and test

Press `F9`. With the game installed, the BSP is copied to its `maps` folder and the game starts on it, like for Garry's Mod (see [Compile and test in Garry's Mod](compile-and-test-gmod.md)).

## Going back

Pick Garry's Mod again in **Target game**. A map whose game is the default game stores nothing: choosing the default game clears the map's own choice.

## FAQ

**The game I want is not in the list.** The list only shows the games the compiler supports. Source 2 games and a few engine forks (for example Counter-Strike 2, Dota 2, Vampire: The Masquerade - Bloodlines) are known but not supported.

**My map compiled but the textures are missing in the other game.** The map still uses Garry's Mod content. Ship the files with the map or replace them with the game's own content (see step 4).

## Related pages

- [Target games](../compiler/target-games.md)
- [Game setup](../getting-started/game-setup.md)
- [Compile window](../compiler/compile-window.md)
- [Check for problems](../features/check-problems.md)
- [Map assets](../features/map-assets.md)
- [Files and folders](../reference/files-and-folders.md)

---
[Back to the documentation index](../README.md)
