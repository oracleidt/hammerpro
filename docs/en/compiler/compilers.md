[Documentation](../README.md) > Compiler > Choosing a compiler | [Français](../../fr/compiler/compilers.md) | [Русский](../../ru/compiler/compilers.md)

# Choosing a compiler

When an external profile such as Hammer++ is selected, the compile window exposes its **VBSP options**, **VVIS options** and **VRAD options** directly. These controls edit the selected profile's own flags and values, separately from Hammer Pro's built-in compiler settings. The resulting command preview shows the exact arguments. Managed quality, lighting and thread options follow the profile's preset-flags setting.

Hammer Pro compiles maps with its own compiler by default (see [The built in compiler](overview.md)). You can also compile with Valve's tools (vbsp, vvis, vrad) found next to the game, or with a compiler of your own: any programs, with your own arguments. The choice is kept for each game.

## Where to choose

- The **Compiler** row at the top of the settings of the [Compile and run window](compile-window.md) (Ctrl+F9). **Edit compilers...** opens the **Compilers** tab.
- The **Compilers** tab of that window and **Settings > Compile > Compiler** show the same editor.
- F9, **File > Compile now**, the batch compile and the MCP tools use the compiler chosen for the game of the map.

## The compilers

The editor lists the compilers on the left. The one marked with ✔ builds the maps of the current game; click a compiler to see it on the right, and **Use for <game>** to choose it.

| Compiler | Where it comes from | What you can change |
|---|---|---|
| **Hammer Pro (built-in)** | Hammer Pro itself, the default. It cannot be deleted. | Its options are the presets, modes and option menus of the compile window (also shown on its page). |
| **Valve (bin)**, **Valve (bin/win64)**, **Valve (bin/x64)** | `vbsp`, `vvis` and `vrad` found in those folders of the game (for Garry's Mod, `GarrysMod\bin` and `GarrysMod\bin\win64`). | Everything; your changes are kept and **Restore** goes back to what was found. |
| **Hammer++ (...)**, **Hammer (...)** | The compilers set in Hammer++'s `hammerplusplus\hammerplusplus_gameconfig.txt` or Hammer's `GameConfig.txt`, for the same game folder, when they differ from the Valve ones. | Same as above. |
| **Valve (Source SDK Base 2013 ...)** | For other games without tools of their own: the Source SDK Base 2013 Multiplayer or Singleplayer tools of the same Steam library. | Same as above. |
| Your compilers | **New** (geometry, visibility and lighting steps to fill in) or **Duplicate** of another one. | Everything, and **Rename**, **Delete**, ⏶ ⏷ to order the list. |

**Reset to defaults** removes your compilers and your changes to the detected ones, and goes back to Hammer Pro's compiler for every game. A game whose chosen compiler is gone (deleted, tools uninstalled) is compiled by Hammer Pro, with a warning in the log.

## Steps

A compiler is a list of steps run in order. The step buttons (**1 Geometry (BSP)**, **2 Visibility (VIS)**...) pick the step shown below them; **➕ Add a step** adds one.

| Step | What it runs | Notes |
|---|---|---|
| **Geometry (BSP)** | A program such as vbsp. | Checked like Hammer Pro's own: a new `.lin` file or VBSP's leak message counts as a leak (the pointfile loads, **Stop when the map leaks** applies), and the step fails when no BSP was written. |
| **Visibility (VIS)** | A program such as vvis. | |
| **Lighting (RAD)** | A program such as vrad. | |
| **Copy a file** | Copies a file, without a program. | Two full paths: the file and where to copy it. A copy into the maps folder keeps the cubemaps built in game, like the built in copy. |
| **Start the game** | A program started without waiting (default `$gameexe`). | Skipped in automated runs. |
| **Custom command** | Any program, with all its arguments. | |

Each step has:

- **Run this step**: unchecked, the step is kept but skipped.
- **Program**: a full path, or a name found on the `PATH`. **Browse...** picks it; ✔ **Found** or ⚠ **Not found** tells whether it exists.
- **Map arguments** (geometry, visibility, lighting): the last arguments, by default `-game $gamedir $path\$file`.
- **Extra arguments** (geometry, visibility, lighting): free arguments after the options, for flags that are not in the lists.
- **Arguments** (other steps): all the arguments of the program.
- **Working folder**: where the program runs. Empty: the build folder (`$path`); the game is started in its own folder.
- **Keep going on failure**: the next steps still run when this one fails.
- **Options** (geometry, visibility, lighting): every option of Valve's tool as a checkbox, with a value field when it takes one. Hover an option for what it does. The full list is below.

The command line of a geometry, visibility or lighting step is: the program, the flags of the compile window (see below), the checked options in the order of the lists, the extra arguments, then the map arguments. **Command lines** at the bottom shows exactly what runs for the open map, after the variables are replaced; **Copy the command lines** copies them.

### Quality, modes and Pass the modes of the compile window

**Quality** on the page of a compiler shows the preset (Draft, Normal, Final, like Hammer's run map dialog) and the dynamic range of the compile window: changing them there changes them in the window too.

With **Pass the modes of the compile window** checked (the default), the geometry, visibility and lighting steps get these flags before their options, and a step whose mode is **No** is skipped:

| Compile window | Flag |
|---|---|
| Geometry: Entities only, Props only | `-onlyents`, `-onlyprops` on the geometry step |
| Visibility: Fast | `-fast` on the visibility step |
| Lighting: Draft, Final | `-fast`, `-final` on the lighting step |
| Dynamic range: LDR, HDR, LDR + HDR | `-ldr`, `-hdr`, `-both` on the lighting step |
| Threads | `-threads N` on the three steps |

Those options are then shown as **set by the compile window** in the lists. Uncheck it for programs that do not read Valve's flags: the options then decide alone.

### Variables

Variables are replaced in the programs, the arguments, the option values and the working folders (any case). A value with spaces stays one argument, so paths like `C:\Program Files (x86)\Steam\...` need no quotes. Quotes group words of your own. `$$` writes a `$`. The **$** button next to a field inserts one.

| Variable | Value |
|---|---|
| `$path` | The folder of the compiled VMF (the `.hammerpro_build` folder next to the map) |
| `$file` | The map name, without folder or extension |
| `$ext` | The map extension (`vmf`) |
| `$bspdir` | The folder compiled maps are copied to |
| `$gamedir` | The game folder (with `gameinfo.txt`) |
| `$gameexe` (also `$game_exe`) | The game executable |
| `$exedir` | The folder of the game executable |
| `$bsp_exe`, `$vis_exe`, `$light_exe` | The programs of the geometry, visibility and lighting steps of this compiler |

## Running

- The steps run like the built in ones: their output streams into the log of the compile window (warnings and errors are explained and clickable like VBSP's), **Cancel** stops the running program and its child processes, and a failed step shows its exit code (or the crash it means).
- After the compiler's steps, the rest of the pipeline is the same as with Hammer Pro's compiler: packing the custom content into the BSP, the copy to the maps folder and starting the game, as set in the compile window. A copy or game step of the compiler runs in addition to them.
- Before anything runs, every enabled step is checked: a missing program, a missing working folder, a bad option value or a copy without two full paths stops the compile with a message naming the compiler and the step.
- The map is compiled from the snapshot in `.hammerpro_build`. For external compilers, the `func_instance` files of the map folder are pointed at from there (`../instances/...`), so VBSP finds them.
- The compile window shows **Compiled by ...** while another compiler is chosen: the option menus, the extra arguments and the smart recompile of the window belong to Hammer Pro's compiler and are not used. The compile while idle, play now (progressive lighting), expert mode and releases always use Hammer Pro's compiler.

## Through MCP

`list_compilers`, `get_compiler` (with the exact command lines), `get_compiler_flags` (every option with what it does), `create_compiler`, `set_compiler` (steps with their options as structured fields, `restore` for a detected compiler), `delete_compiler`, `select_compiler` (per game), `reset_compilers`, and `compile_with_options` with `compiler` for one compile. Creating or changing the programs of a compiler needs `confirm: true`. See [MCP server](../reference/mcp-server.md) and the [tool list](../../MCP.md).

## Options of Valve's tools

Every option the lists offer, with what it does (from the Valve Developer Community pages of VBSP, VVIS and VRAD and the tools of the Source SDK 2013). The options of the group **Other branches** are only read by the tools of CS:GO and newer branches; the others refuse them. Options that are in no list go in **Extra arguments**.

<!-- valve flags begin -->

### vbsp: Geometry (BSP)

| Option | Group | Value | Default | What it does |
|---|---|---|---|---|
| `-onlyents` | Quality | switch |  | Only import the entities of the VMF into the existing BSP: brushes, brush entities and the lighting stay as they are. Fast for changing entity keys. *Written by the Geometry mode of the compile window.* |
| `-onlyprops` | Quality | switch |  | Only update the static props and detail props of the existing BSP. *Written by the Geometry mode of the compile window.* |
| `-threads` | Speed | integer 1 to 256 |  | Number of threads (default: one per processor). *Written by the Threads setting of the compile window.* |
| `-low` | Speed | switch |  | Run as an idle priority process. |
| `-nodetail` | Geometry | switch |  | Remove all detail geometry (func_detail): what is left is what blocks visibility. |
| `-fulldetail` | Geometry | switch |  | Treat all detail geometry as world geometry, so it cuts visibility too. |
| `-nowater` | Geometry | switch |  | Remove the water brushes. |
| `-noweld` | Geometry | switch |  | Do not join the face vertices together. |
| `-nocsg` | Geometry | switch |  | Do not cut away the parts where brushes overlap. |
| `-noshare` | Geometry | switch |  | Write unique face edges instead of sharing them. |
| `-notjunc` | Geometry | switch |  | Do not fix the T-junctions (small cracks may show between faces). |
| `-allowdetailcracks` | Geometry | switch |  | Do not fix the T-junctions on func_detail brushes. |
| `-noopt` | Geometry | switch |  | Keep the outer shell of the map (the faces nobody can see from inside), which VBSP removes by default. |
| `-noprune` | Geometry | switch |  | Do not prune neighbouring solid nodes of the tree. |
| `-nomerge` | Geometry | switch |  | Do not merge the chopped faces back together on the nodes. |
| `-nomergewater` | Geometry | switch |  | Do not merge the chopped water faces back together. |
| `-nosubdiv` | Geometry | switch |  | Do not subdivide the faces for their lightmaps. |
| `-snapaxial` | Geometry | switch |  | Snap the axis aligned planes to whole coordinates. |
| `-micro` | Geometry | number 0 to 1000000 | `1` | Warn about brushes smaller than this volume (default 1). |
| `-block` | Geometry | 2 numbers | `0 0` | Grid block where VBSP starts chopping the level (two numbers). |
| `-blocks` | Geometry | 4 numbers | `0 0 0 0` | Minimum and maximum grid blocks VBSP chops the level on (four numbers). |
| `-leaktest` | Visibility | switch |  | Stop when the map leaks. The .lin leak file is written in any case. |
| `-forceskyvis` | Visibility | switch |  | Compute the visibility inside the 3D skybox too. |
| `-luxelscale` | Lightmaps | number 0.01 to 1000 | `1` | Scale every lightmap by this amount (default 1; 2 halves the lightmap density). |
| `-minluxelscale` | Lightmaps | number 0.01 to 1000 | `1` | No lightmap scale goes below this value (default 1). |
| `-lightifmissing` | Lightmaps | switch |  | Give a lightmap to every surface, even those that do not need one. |
| `-bumpall` | Lightmaps | switch |  | Treat every surface as bump mapped (bumped lightmaps everywhere). |
| `-allowdynamicpropsasstatic` | Props | switch |  | Allow models without the static flag in their prop data on prop_static (Garry's Mod and Team Fortress 2 tools). |
| `-virtualdispphysics` | Props | switch |  | Use virtual collision models for displacements instead of precomputed ones. |
| `-dumpstaticprops` | Props | switch |  | Write the static props to staticprop*.txt files. |
| `-embed` | Content and files | folder |  | Pack the files of this folder into the BSP pakfile. |
| `-keepstalezip` | Content and files | switch |  | Keep the pakfile of the existing BSP and rebuild everything else. |
| `-replacematerials` | Content and files | switch |  | Replace materials as listed in materialsub.txt of the maps content folder. |
| `-verbose` | Debugging | switch |  | Print more details in the log. |
| `-verboseentities` | Debugging | switch |  | With -verbose, leave out the details of the brush entities. |
| `-glview` | Debugging | switch |  | Write .gl files of the geometry for Valve's glview viewer. |
| `-dumpcollide` | Debugging | switch |  | Write files with the collision data. |
| `-novconfig` | Debugging | switch |  | Never open the graphical game picker on a game folder error. |
| `-FullMinidumps` | Debugging | switch |  | Write large crash dumps. |
| `-StaticPropCombine` | Other branches (CS:GO and newer tools) | switch |  | Merge static props into combined models (VBSP of CS:GO and newer branches only; others refuse it). |
| `-KeepSources` | Other branches (CS:GO and newer tools) | switch |  | With -StaticPropCombine, keep the source files of the combined models (CS:GO and newer branches only). |

### vvis: Visibility (VIS)

| Option | Group | Value | Default | What it does |
|---|---|---|---|---|
| `-fast` | Quality | switch |  | Only the first quick pass of the visibility: fast but coarse, more is drawn in game. *Written by the Visibility mode of the compile window.* |
| `-radius_override` | Quality | number 0 to 1000000 | `4096` | Force a visibility radius in units, whatever env_fog_controller says. |
| `-threads` | Speed | integer 1 to 256 |  | Number of threads (default: one per processor). *Written by the Threads setting of the compile window.* |
| `-low` | Speed | switch |  | Run as an idle priority process. |
| `-mpi` | Speed | switch |  | Spread the work over other computers with VMPI (needs Valve's VMPI setup). |
| `-mpi_pw` | Speed | text |  | VMPI password, to pick a given group of workers. |
| `-nosort` | Visibility | switch |  | Do not sort the portals (sorting only speeds things up). |
| `-tmpin` | Visibility | switch |  | Read the portal file from \tmp\&lt;map&gt;. |
| `-tmpout` | Visibility | switch |  | Write the results to \tmp\&lt;map&gt;. |
| `-trace` | Debugging | 2 numbers | `0 1` | Write a line file that traces the visibility from one cluster to another (two cluster numbers). |
| `-verbose` | Debugging | switch |  | Print more details in the log. |
| `-novconfig` | Debugging | switch |  | Never open the graphical game picker on a game folder error. |
| `-FullMinidumps` | Debugging | switch |  | Write large crash dumps. |

### vrad: Lighting (RAD)

| Option | Group | Value | Default | What it does |
|---|---|---|---|---|
| `-fast` | Quality | switch |  | Quick and dirty lighting, for testing. *Written by the Lighting mode of the compile window (Draft adds -fast, Final adds -final).* |
| `-final` | Quality | switch |  | High quality: more rays for the sky and the bounced light (like -extrasky 16). *Written by the Lighting mode of the compile window (Draft adds -fast, Final adds -final).* |
| `-ldr` | Quality | switch |  | Light only the low dynamic range. *Written by the Dynamic range of the compile window.* |
| `-hdr` | Quality | switch |  | Light only the high dynamic range. *Written by the Dynamic range of the compile window.* |
| `-both` | Quality | switch |  | Light both dynamic ranges. *Written by the Dynamic range of the compile window.* |
| `-extrasky` | Quality | integer 1 to 1024 | `16` | Trace this many times more rays for the sky light and the bounced light. |
| `-bounce` | Quality | integer 0 to 1000 | `100` | Most light bounces (default 100). |
| `-noextra` | Quality | switch |  | No supersampling: faster, with more jagged shadows. |
| `-fastambient` | Quality | switch |  | Lower quality ambient light per leaf, to save time. |
| `-softsun` | Quality | number 0 to 90 | `0` | Treat the sun as an area light this many degrees wide: soft shadows (0 to 5 recommended, default 0). |
| `-threads` | Speed | integer 1 to 256 |  | Number of threads (default: one per processor). *Written by the Threads setting of the compile window.* |
| `-low` | Speed | switch |  | Run as an idle priority process. |
| `-mpi` | Speed | switch |  | Spread the work over other computers with VMPI (needs Valve's VMPI setup). |
| `-mpi_pw` | Speed | text |  | VMPI password, to pick a given group of workers. |
| `-smooth` | Lightmaps | number 0 to 180 | `45` | Angle in degrees under which faces share smoothed lighting (default 45). |
| `-luxeldensity` | Lightmaps | number 0 to 1 | `1` | Scale the density of all luxels down by this amount (below 1, default 1). |
| `-chop` | Lightmaps | number 0.1 to 1024 | `4` | Smallest size of a bounce patch, in luxels, used on edges (default 4). |
| `-maxchop` | Lightmaps | number 0.1 to 1024 | `4` | Largest size of a bounce patch, in luxels, used inside faces (default 4). |
| `-dispchop` | Lightmaps | number 0.1 to 1024 | `8` | Largest size of a bounce patch on displacements, in luxels (default 8). |
| `-maxdispsamplesize` | Lightmaps | integer 1 to 65536 | `512` | Largest light sample size on displacements (default 512). |
| `-LargeDispSampleRadius` | Lightmaps | switch |  | Gather the light of displacements over a wider area: removes splotches of bounced light on terrain, slower. |
| `-centersamples` | Lightmaps | switch |  | Move the sample centers. |
| `-dlightmap` | Lightmaps | switch |  | Put the direct light in a lightmap of its own, apart from the bounced light. |
| `-noskyboxrecurse` | Lightmaps | switch |  | Do not cast shadows from the 3D skybox onto the world. |
| `-StaticPropLighting` | Props | switch |  | Bake the lighting of every static prop vertex (better looking props, slower). |
| `-StaticPropPolys` | Props | switch |  | Static props cast shadows from their exact polygons instead of their collision models. |
| `-TextureShadows` | Props | switch |  | Let the alpha of model textures block light (for props listed in lights.rad). |
| `-nossprops` | Props | switch |  | No static prop casts a shadow on itself. |
| `-nodetaillight` | Props | switch |  | Do not light the detail props. |
| `-onlydetail` | Props | switch |  | Only light the detail props and the ambient light of the leaves. |
| `-lights` | Content and files | file |  | A light file (.rad) to load in addition to lights.rad and the map's own file. |
| `-verbose` | Debugging | switch |  | Print more details in the log. |
| `-dump` | Debugging | switch |  | Write debugging .txt files. |
| `-dumpnormals` | Debugging | switch |  | Write the normals to the debugging files. |
| `-dumptrace` | Debugging | switch |  | Write the ray tracing scene to debugging files. |
| `-debugextra` | Debugging | switch |  | Draw the supersampling into the lightmaps to show where it happens. |
| `-loghash` | Debugging | switch |  | Log the sample hash table to samplehash.txt. |
| `-rederror` | Debugging | switch |  | Show the errors in red. |
| `-OnlyStaticProps` | Debugging | switch |  | Only the direct lighting of the static props (debugging). |
| `-StaticPropNormals` | Debugging | switch |  | Light the static props with their normals only, to check them (debugging). |
| `-stoponexit` | Debugging | switch |  | Wait for a key press at the end (the compile then never ends on its own). |
| `-novconfig` | Debugging | switch |  | Never open the graphical game picker on a game folder error. |
| `-FullMinidumps` | Debugging | switch |  | Write large crash dumps. |

<!-- valve flags end -->

## FAQ

**Do I need Valve's tools?** No. Hammer Pro's compiler is complete; the other compilers are there for those who prefer them or need a tool of their own.

**Why is my Valve compile slower than Hammer Pro's?** VVIS has no bound on the portal flow and VRAD traces more rays; see [Compile performance](performance.md).

**Where are the compilers saved?** In `settings.toml`, `[compile]`: `compilers` (your compilers and your changes to the detected ones) and `compiler_by_game` (the compiler of each game). See [Settings reference](../reference/settings.md).

## Related pages
- [Compile and run window](compile-window.md)
- [The built in compiler](overview.md)
- [Compile options](compile-options.md)
- [Target games](target-games.md)
- [MCP server](../reference/mcp-server.md)

---
[Back to the documentation index](../README.md)
