[Documentation](../README.md) > Compiler > Compile and run window | [Français](../../fr/compiler/compile-window.md) | [Русский](../../ru/compiler/compile-window.md)

# Compile and run window

The **Compile and run** window sets how maps are compiled, runs the compile and shows its live log, statistics and timing history. Its settings are also the ones **F9** uses.

**Save BSP beside VMF** in the compile settings writes the final BSP next to the source VMF. Intermediate files and the compile snapshot use the application cache instead of creating `.hammerpro_build` beside the map. The source VMF is preserved, and **Copy to the maps folder** remains independent. This setting takes effect on the next compile; existing build folders are left in place. Unsaved maps continue to use the cache.

## Opening it

- **File > Compile > Compile and run...** (`Ctrl+F9`) or the **Compile** button of the toolbar.
- **F9** (**File > Compile > Compile now**) compiles with the current settings without opening the window. **File > Compile > Cancel compile** stops it.
- The window is a **separate window** by default: you can move it out of Hammer Pro or to another screen and keep working while it compiles. **Put back in the main window** shows it inside Hammer Pro again; **Separate window** takes it out. The choice and the place are remembered (`separate_window`, see [Window frames](../features/window-frames.md)).
- The default values of the same settings are on the **Compile** page of the Settings window (**Tools > Options...**, `Ctrl+,`).

## Target game

The **Target game** row at the top tells which game the open map is for (kept in the map's sidecar file). Picking another game mounts its content and FGDs when it is installed, and the compile follows its BSP format, limits and lighting passes (the row gives its BSP version and static prop format). A game that is not installed still compiles, with the content mounted now, but nothing is copied or launched. See [Target games](target-games.md).

## Settings

Each row has its label on the left, aligned with the others, and its options on the right. When the window is narrow or the labels are longer (French, Russian), a row continues on the next line instead of cutting its options off, and the steps keep each label next to its list. Hover an option to read what it does.

| Option | What it does | Default |
|---|---|---|
| **Compiler** | The compiler of the game of the map: **Hammer Pro (built-in)**, the Valve tools found next to the game, or one of yours. **Edit compilers...** opens the **Compilers** tab. With another compiler, the window shows **Compiled by ...**: the preset, the modes and the dynamic range become its flags, and the option menus, the extra arguments and the smart recompile (which belong to Hammer Pro's compiler) are greyed out. See [Choosing a compiler](compilers.md). | Hammer Pro (built-in) |
| **Preset** | **Draft**, **Normal**, **Final** or **Custom** (see below). | Normal |
| **Smart recompile** | Runs only what the changes since the last compile need: entities only, lighting only, props only, or everything. Draft and Normal only. | on |
| **Steps: Geometry** | **No**, **Normal**, **Only entities** (`-onlyents`) or **Only props** (`-onlyprops`). The partial modes update the BSP of the last compile in seconds and keep its visibility and lighting; without an earlier BSP a full compile runs instead. | Normal |
| **Visibility** | **No**, **Fast** (`-fastvis`, coarse but quick) or **Normal** (`-vis`, what players need). Computed by the geometry step. | Normal |
| **Lighting** | **No**, **Draft**, **Normal** or **Final**. | Normal |
| **Dynamic range** | **LDR**, **HDR** or **LDR + HDR**. Final always computes both; Draft computes one (with LDR + HDR chosen, the one the game is set to: HDR when its `mat_hdr_level` is 2, else LDR). For Portal 2, CS:GO and the other HDR only games, LDR becomes HDR. | LDR |
| Command lines | Under the steps, the exact command line of each step. Hover a line for the meaning of every flag Hammer Pro added; right click > **Copy the command line**. | |
| **Compile only the cordoned area** | Shown when a cordon is active. Objects outside every box are left out, each box is sealed and the map is named `<map>_cordon`. | on |
| **Static props: Light static props (per vertex)** | When the quality asks for it (Final), bakes the light of every static prop vertex (`-StaticPropLighting`). Off: never. | on |
| **Static props: Static prop shadows** | Static props cast shadows in the lighting. Off: `-nostaticpropshadows`, much faster on maps with many props. | on |
| **Before: Save the map first** | Also saves your work. The compile itself uses a copy and never needs a save. | off |
| **Before: Compile hidden objects** | Like Hammer, objects hidden in the editor (visgroups, Hide) are left out unless this is checked. Hidden lights get a warning. | off |
| **Before: Compile leaking maps anyway** | Off: the compile stops right after the geometry when the map leaks (`-leaktest`). | off |
| **Content: Pack custom content** | Packs the custom materials, models, sounds, particle files (with the `maps/<map>_particles.txt` manifest the game needs to load them), the map's own soundscape and level sound scripts, sky, projected textures and other files the map uses into the BSP, with a policy: **Release builds only** (Final compiles and releases) or **Always**. | on, Release builds only |
| **Content: Development addon** | Builds that do not pack copy the same custom content (materials, models, sounds, particle files with their manifest, map scripts) into `garrysmod/addons/hammerpro_dev`, which the game loads (restart the game once after the first copy). Also holds the script of `point_hammerpro_skybox`. | on |
| **Content: Export custom materials** | Also copies the used materials of your custom texture folders into the game folder. Not needed to compile: the compiler always sees them. | off |
| **After: Copy to the maps folder** | Copies the BSP to `garrysmod/maps` (or the **Maps folder** of Advanced). | on |
| **After: Launch the game** | Starts the game on the map. **Through Steam** opens it with a `steam://` link instead of the game executable. | on, off |
| **After: Load the leak path on leak** | Loads `<map>.lin` in the views when the map leaks. | on |
| **Game arguments** | Extra game arguments. `-game`, `-hijack` (reuses a running game) and `+map` are added unless you set them; `{map}` is replaced by the map name. | `-console -dev` |
| **Threads** | **Automatic** (every core, or every core but one when a core is kept for the editor) or a number from 1 to 256 (`-threads`). | Automatic |
| **Keep a core for the editor** | The automatic thread count is one less than the cores, so the editor stays smooth. Needed by **Compile while idle**. | on |

Under the game arguments, the launch options (**Start the game:** after the compile or when the compile starts, **Reuse the running game**, **Fast start**, **Skip unused Workshop and addons**, **Report the map load time**) are described in [Launch the game](../features/game-launch.md).

### The presets

| Preset | Geometry | Visibility | Lighting | Packing |
|---|---|---|---|---|
| **Draft** | full, stops on leaks | Fast (`-fastvis`) | Draft (`-bounce 1 -fast`), one dynamic range | no (development addon) |
| **Normal** | full | Normal (`-vis`) | Normal (no extra flag) | no (development addon) |
| **Final** | full | Normal (`-vis`) | Final (`-final -StaticPropLighting`), always LDR + HDR | yes |
| **Custom** | your choices of the step lists | | | as the policy says |

Changing a step list makes the preset **Custom** unless the choices match a named preset.

### The options menus

Two menus list every flag of the compiler, grouped (Quality, Speed, Geometry, Visibility, Lightmaps, Static props, Texture lights and sky, Content, Debugging, Written from other settings): **All geometry and visibility options (hammerpro --bspc)** and **All lighting options (hammerpro --rad)**. Each flag follows the preset (or your extra arguments) until you check or uncheck it: it is then **forced on** or **forced off** for every compile, with its value when it takes one. **Follow the preset again** and **Follow the preset for every option** undo that. Flags written by other settings (visibility, threads, dynamic range, game, leak policy, static prop shadows) are shown but not changed here. Hover a flag for what it does. The full list is in [Compile options](compile-options.md).

## Advanced

| Option | What it does | Default |
|---|---|---|
| **Extra geometry arguments** | Added to `hammerpro --bspc`, for example `-fastvis`. A flag here replaces the preset's. | empty |
| **Extra lighting arguments** | Added to `hammerpro --rad`, for example `-bounce 4`. | empty |
| **Compile while idle** | When the map has not changed for 3 seconds, the geometry and visibility are built in the background (one core kept for the editor): F9 then reuses them and leaks show up early. Paused on battery and while another compile runs; any edit cancels it. | on with 8 or more processor threads |
| **Play now, finish the lighting later** | Compiles a Draft, starts the game, then compiles Normal (or Final with `progressive_preset = "Final"` in `settings.toml`) in the background and replaces the map in the maps folder when you leave it in game (it waits up to five minutes, retrying). | off |
| **Keep a timing history** | Records the time of every step (History tab, estimates, slow step warnings). | on |
| **Cordon seal material** | Material of the brushes that seal a cordon box (shared with the cordon settings). | `TOOLS/TOOLSSKYBOX` |
| **Game** | The game executable. Empty: detected. | detected |
| **Maps folder** | Where the BSP is copied. Empty: `garrysmod/maps` of the game. | empty |

## Compilers

The **Compilers** tab lists the compilers on the left (Hammer Pro's, the detected Valve tools, yours) and shows the chosen one on the right: its steps (program, map arguments, extra arguments, working folder, every option of Valve's tool as a checkbox with its value) and the exact command lines. See [Choosing a compiler](compilers.md).

## Expert mode

The **Expert** tab holds named compile profiles, like Hammer's Run Map Expert. Check **Use Expert mode** and the selected profile runs instead of the preset (the step lists are then greyed out). Expert profiles run with Hammer Pro's compiler only: while another compiler is chosen for the game, the tab says so and the profile is not used (a compiler of your own does the same with any program, see [Choosing a compiler](compilers.md)).

- Buttons: **New**, **Duplicate**, **Rename**, **Delete**, **Import...** (Hammer's `CmdSeq.wc` or Hammer++'s `hammerplusplus_sequences.cfg`), **Add a step**.
- Each step has a checkbox (enabled), an action, its arguments, **Keep going on failure**, and a **Program** for **Run a program**.
- Actions: **Geometry and visibility** (`hammerpro --bspc <args>`), **Visibility (added to the geometry step)** (a VVIS step: adds `-vis`, or `-fastvis` when its arguments have `-fast`, plus its other VVIS options, to the geometry step before it), **Lighting** (`hammerpro --rad <args>`), **Pack folders into the BSP** (the files of each folder given, or a bspzip `-addlist` list), **Copy file**, **Delete file**, **Rename file**, **Start the game**, **Run a program**.
- File steps only touch files inside the map's folders, the build folder, the game folder and the maps folder; any other path refuses the whole profile before anything runs.
- Profiles imported from Valve's tools keep working: `$bsp_exe`, `$vis_exe` and `$light_exe` all name Hammer Pro, VBSP and VRAD steps become its steps, bspzip packs in process.

Fresh install profiles: **Default**, **Fast** (`-fastvis`, `-bounce 2 -noextra`), **LDR + HDR**, **Final** (`-both -final`), **Only entities**.

### Tokens

Tokens are replaced in the arguments and the program (case insensitive; unknown `$words` stay as written). The arguments are split into words first, so a value with spaces (`C:\Program Files (x86)\...`) stays one argument:

| Token | Value |
|---|---|
| `$file` | The map name (no folder, no extension) |
| `$ext` | The map extension (`vmf`) |
| `$path` | The folder of the compiled VMF (the `.hammerpro_build` folder) |
| `$bspdir` | The folder compiled maps are copied to |
| `$gamedir` | The game folder (with `gameinfo.txt`) |
| `$exedir` | The folder of the game executable |
| `$bsp_exe`, `$vis_exe`, `$light_exe` | The Hammer Pro executable (all three) |
| `$game_exe` (also `$gameexe`) | The game executable |
| `$$` | A literal dollar sign |

## Log, Statistics, History

- **Log**: the live log of every step with progress bars (the lighting step shows its stage, the stage percentage and the time left) and step status (Waiting, Running, Done, Failed, Skipped, The map leaks). Click a step to show only its log. Filters **Show information lines**, **Show warnings**, **Show errors**, **Search the log**, **Auto-scroll**, **Copy the whole log**. Errors and warnings with a place are clickable (brush, face, entity, position) and known problems have an explanation and a button (**Help**, **Go to**, **Leak path**, **Check**). See [Limits and logs](limits-and-logs.md).
- **Statistics**: limits, brushes, visibility, lighting, stage times and optimization hints read from the log, each item with a **Go to** button. See [Limits and logs](limits-and-logs.md).
- **History**: the time of every step of the past compiles of the map (When, Preset, each step, Total, Clusters, portals) with sparklines. A step much slower than usual gets a warning in the log.

## Batch compile

**File > Compile > Batch compile...** compiles several maps one after the other, each with its own preset. **Add maps...**, **Remove finished**, **Copy to the maps folder**, **Compile all**, **Cancel**. Each VMF is copied into the `.hammerpro_build` folder next to it and compiled there, so your files are never written.

## Tips

- Iterate with **Draft** plus **Smart recompile**: moving an entity recompiles entities only, in seconds.
- The leak path loads on its own; the [Leaks and pointfiles](../features/leaks-pointfiles.md) page shows how to follow it.
- Every setting of this window is reachable through the MCP tools `get_compile_options` and `set_compile_options` ([MCP reference](../../MCP.md)).

## FAQ

**Why is the lighting step skipped?** The map leaks and **Compile leaking maps anyway** is off: the compile stops after the geometry.

**Why did my Draft compile only one dynamic range?** Draft always lights one range to stay fast. Use Normal with **LDR + HDR**, or Final.

**Smart recompile ran a full compile, why?** The brushes, the settings, the game or the last BSP changed, or it was the first compile. The **Compile** button label says what will run ("Auto: entities only"...).

**My custom textures are pink in game after a Normal compile.** Normal does not pack: keep **Development addon** on (and restart the game once), or set the policy to **Always**.

## Related pages
- [Compiler overview](overview.md)
- [Choosing a compiler](compilers.md)
- [Compile options](compile-options.md)
- [Launch the game](../features/game-launch.md)
- [Cordons](../features/cordons.md)
- [Build and save as content](../features/build-content-release.md)
- [Settings reference](../reference/settings.md)

---
[Back to the documentation index](../README.md)
