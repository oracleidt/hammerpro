[Documentation](../README.md) > Guides > Compile and test in Garry's Mod | [Français](../../fr/guides/compile-and-test-gmod.md) | [Русский](../../ru/guides/compile-and-test-gmod.md)

# Compile and test in Garry's Mod

This guide takes a map from the editor to Garry's Mod: compile it with Hammer Pro's built-in compiler, start the game on it, then iterate quickly. No VBSP, VVIS or VRAD is needed: Hammer Pro compiles the map itself.

## Before you start

- Garry's Mod is installed through Steam. Hammer Pro finds it by itself at startup. If it does not, open **Tools > Options...** (`Ctrl+,`), page **Game**, and set the **Garry's Mod folder**.
- The map has at least one `info_player_start` and is sealed (no hole to the void).
- The map file name has no spaces and no dots (`gm_my_map.vmf`, not `gm my.map.vmf`): the game cannot load such names and the compile refuses them.

## Step 1: compile with F9

1. Press `F9` (**File > Compile now**). The map is compiled at once with the last settings, without opening any window. The status bar shows the progress.
2. The compile works on a copy of the map written to `.hammerpro_build\` next to it: your `.vmf` is never saved or changed by a compile.
3. With the default settings, the BSP is copied to `garrysmod\maps` and Garry's Mod starts on the map with `-console -dev`.

If the map was never saved, Hammer Pro asks where to save it first.

## Step 2: the Compile and run window

Press `Ctrl+F9` (**File > Compile and run...**) to choose the settings. The window opens on its own (**Separate window**).

1. **Target game**: the game the map is for. Keep Garry's Mod (see [Port a map to another game](port-to-another-game.md) for the others).
2. **Preset**:
   - **Draft**: quick iteration. Stops on leaks, fast visibility, preview lighting (one bounce, one dynamic range), no packing. Playable in seconds.
   - **Normal**: standard quality, full visibility and default lighting. The default.
   - **Final**: release quality, full visibility and final lighting in both dynamic ranges. Slow.
   - **Custom**: Normal plus your extra arguments (see **Advanced**).
3. **Steps**: **Geometry**, **Visibility** and **Lighting**, each with its own mode. **Only entities** and **Only props** in Geometry update the BSP of the last compile in seconds, keeping its visibility and lighting.
4. **Dynamic range**: **LDR**, **HDR** or **LDR + HDR**. Final always computes both.
5. **After**: **Copy to the maps folder**, **Launch the game** (and **Through Steam**), **Load the leak path on leak**, **Game arguments**.
6. Click **Compile**. The **Log** tab follows each step; errors and warnings are clickable and take you to the brush, face, entity or place they are about.

Every option is described in [Compile window](../compiler/compile-window.md) and [Compile options](../compiler/compile-options.md).

## Step 3: test in the game

In the **Game** section of the window:

- **Start the game:** **after the compile** (like Hammer) or **when the compile starts** (the game boots while the compiler runs, then loads the map).
- **Reuse the running game**: the next test loads the map in the game already running instead of starting it again. The status bar shows **Game ready** when Garry's Mod is running.
- **Fast start**: options that make the game start faster for a test (skip the intro video, windowed, borderless, window size, console, developer messages, **Private local game**). Each one adds or removes its flag in the game arguments.
- **Skip unused Workshop and addons**: when the map uses nothing from them, the game does not mount them and starts faster (addon tools and weapons are then missing in game).

To start in game where your editor camera is, install the Hammer Pro bridge addon: **Tools > Options...**, page **Game**, section **Testing in game**, **Install**, then tick **Spawn at the editor camera**. Restart the game once after installing it. The bridge only acts in local games. It also enables **Report the map load time**.

## Step 4: iterate quickly

- Keep **Smart recompile** on (the default): Draft and Normal compiles only run what changed since the last compile (entities only, lighting only, props only, or everything).
- Use **Draft** while you build, **Normal** to check the look, **Final** for a release.
- **File > Reload the map in game** loads the map again in the running game.
- **Play now, finish the lighting later**: compiles a Draft, starts the game, then compiles the chosen quality in the background. When you leave the map in game, the fully lit BSP replaces the quick one.
- Custom content (materials, models, sounds, particles): Draft and Normal builds do not pack it into the BSP. With **Development addon** on (the default), it is copied to `garrysmod\addons\hammerpro_dev` so the map looks right in game. Restart the game once after the first copy.

## If the map leaks

With **Stop when the map leaks** (the default), the compile stops and reports **The map leaks**. The leak path is loaded at once (**Load the leak path on leak**): a line from an entity to the void through the hole. Follow it in the views, close the hole and compile again. You can also check without compiling with **Map > Check for leaks**. See [Leaks and pointfiles](../features/leaks-pointfiles.md).

## FAQ

**The game does not start.** Check **Launch the game** in the window and the game found on the **Game** page of the settings. Automated runs (`--script`) never start the game.

**My custom textures are purple and black in game.** Keep **Development addon** on and restart the game once, or use the **Final** preset or **Build and save as content**, which pack them into the map.

**The compile asks to save.** Only for a map that was never saved. **Save the map before compiling** is off by default: the compile never needs it.

## Related pages

- [Compile window](../compiler/compile-window.md)
- [Compile options](../compiler/compile-options.md)
- [Game launch](../features/game-launch.md)
- [Leaks and pointfiles](../features/leaks-pointfiles.md)
- [Common errors](../troubleshooting/common-errors.md)
- [Collect content for a release](collect-content-for-release.md)

---
[Back to the documentation index](../README.md)
