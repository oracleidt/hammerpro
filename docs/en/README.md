[Hammer Pro](../../README.md) > Documentation | [Français](../fr/README.md) | [Русский](../ru/README.md)

# Hammer Pro documentation

[Team Mapping: collaborative editing, security and limitations](features/team-mapping.md)

Welcome to the knowledge base of Hammer Pro 1.2.0, the level editor for Source engine maps built for Garry's Mod first. Every feature has its own page: what it is, how to open it, every option with its default, tips and a short FAQ. Pages link to each other, so start anywhere.

New to Hammer Pro? Read [Installation](getting-started/installation.md), [Game setup](getting-started/game-setup.md) and [Your first map](getting-started/first-map.md), in that order.

## Getting started

| Page | What you will find |
| --- | --- |
| [Installation](getting-started/installation.md) | Requirements, download, first start, updating, uninstalling. |
| [Game setup](getting-started/game-setup.md) | First run wizard, choosing the game, mounting content (VPKs, addons, Workshop, `mount.cfg`, downloads), FGDs. |
| [Interface tour](getting-started/interface-tour.md) | Title bar, menus, toolbar, tool strip, the four views, dock tabs, status bar, console. |
| [Your first map](getting-started/first-map.md) | A room, a light, a player start, compiled and played in Garry's Mod, step by step. |
| [VMF compatibility](getting-started/vmf-compatibility.md) | Opening and saving, working side by side with Hammer and Hammer++, the sidecar file, import and export. |

## Features

### Creating and editing geometry

| Page | What you will find |
| --- | --- |
| [Selection tool](features/selection-tool.md) | Click and box selection, selection modes, 2D handles, 3D stretch handles, gizmo, modal transforms, nudging. |
| [Camera and navigation](features/camera-navigation.md) | Flying the 3D camera, keyboard layouts, mouselook, camera tool, 2D pan and zoom, control schemes. |
| [Trackpad](features/trackpad.md) | Trackpad mode, two finger scroll and pinch gestures, Space + drag instead of the middle button. |
| [Block tool](features/block-tool.md) | Brushes and every primitive with its options. |
| [Polygon tool](features/polygon-tool.md) | Convex brushes from an outline clicked point by point. |
| [Curve tool](features/curve-tool.md) | Curved walls and rails: box brushes with mitred joints along a stroke drawn in a 2D view. |
| [Road tool](features/road-tool.md) | Roads along a spline: lanes, shoulders, lines, banking, heights, terrain following, junctions with flares. |
| [Pipe tool](features/pipe-tool.md) | Pipes and ducts made of brushes: hollow or solid, square or round, 90 degree elbows, collars, routes on the grid. |
| [Scatter by material](features/scatter-by-material.md) | Props placed only where a texture shows on blend materials and displacements, with count, spacing, rotation, scale and seed. |
| [Road details](features/road-details.md) | Sidewalks, curbs, V and U gutters with drain slots, rounded junction corners, and terrain blending to a road with texture fade. |
| [Clip tool](features/clip-tool.md) | Cutting brushes with a plane. |
| [Slice tool](features/slice-tool.md) | Cutting brushes into several equal blocks, left to right or top to bottom, from the 3D view (`Shift+Y`, mouse for the position, wheel for the segments). |
| [Extrude / carve tool](features/extrude-tool.md) | Outlines clicked on a face, carved into the brush (recess or opening) or extruded out of it. |
| [Bevel tool](features/bevel-tool.md) | Round or chamfer brush edges in the 3D view (`Ctrl+B`, mouse for the size, wheel for the segments), or bevel them inward: a cove, or a concave round filling an inner corner (arches). |
| [Vertex tool](features/vertex-tool.md) | Moving, merging, splitting and scaling vertices. |
| [Displacement tool](features/displacement-tool.md) | Creating, sculpting, painting alpha, tags, sewing, noise. |
| [Terrain](features/terrain.md) | Import and export heightmaps, procedural terrain generation, erosion, automatic texturing. |
| [Brush operations](features/brush-operations.md) | Carve, hollow, merge, tie to entity, func_detail, nodraw, hint brushes, texture lock. |
| [Transform and align](features/transform-align.md) | Transform dialog, align, flip, snap, move to floor, physics placement. |
| [Physics simulator](features/physics-tool.md) | Play, pause, grab, shoot and explode brushes, props and ragdolls; keep their poses with one undo step. |
| [Clipboard](features/clipboard.md) | Cut, copy, paste, paste special, duplicate, between Hammer Pro windows. |
| [Grids](features/grids.md) | Global grid, one grid per view, custom grids, grids of objects, grid manager, 3D grid. |

### Entities and gameplay

| Page | What you will find |
| --- | --- |
| [Entity tool](features/entity-tool.md) | Placing point entities and prefabs on surfaces and models. |
| [Entity properties](features/entity-properties.md) | The entity panel, SmartEdit, flags, color editor, keyvalues, find and replace, entity report, map properties. |
| [I/O and logical view](features/io-logical-view.md) | Outputs and inputs, connection lines, the logical view window and the 2D logical view. |
| [Path tool](features/path-tool.md) | Chains of path_corner, path_track and other path entities. |
| [Decals and overlays](features/decals-overlays.md) | Decal tool and overlay tool. |
| [Sprinkle tool](features/sprinkle-tool.md) | Painting entities on surfaces. |
| [Instances](features/instances.md) | func_instance: create, edit, collapse, parameters. |
| [Prefabs](features/prefabs.md) | The prefab library. |
| [Sky cameras](features/sky-cameras.md) | Several 3D skyboxes and the skybox switch entity. |
| [Soundscapes](features/soundscapes.md) | The soundscape editor. |
| [Navigation mesh](features/nav-mesh.md) | Generating and showing the `.nav` of the map. |

### Textures and materials

| Page | What you will find |
| --- | --- |
| [Texture browser](features/texture-browser.md) | Search syntax, filters, favorites, drag and drop. |
| [Texture tool](features/texture-tool.md) | Face Edit: apply, scale, shift, rotate, justify, align, lightmap scale. |
| [Find and replace textures](features/find-replace-textures.md) | Replacing materials in the selection or the whole map. |
| [Custom texture folders](features/custom-texture-folders.md) | Your own folders, plain images as materials, image import. |
| [Content index and scan](features/content-index.md) | The saved index of the game files, hard disk and SSD scanning, duplicate folders. |
| [UV editor](features/uv-editor.md) | Face, edge and vertex modes, layers, its own window. |
| [Material editor](features/material-editor.md) | Editing VMT files, one at a time or in batch. |
| [VTF editor](features/vtf-editor.md) | Edit textures in VTFEdit (or another installed VTF editor) or in the built in editor: channels, mips, frames, cubemaps, flags, formats, normal maps. |
| [Shader editor](features/shader-editor.md) | Node based image and material editor: noise, blur, color ramps, normal maps, tile blending, written as VTF and VMT. |

### Models, sounds and effects

| Page | What you will find |
| --- | --- |
| [Model browser](features/model-browser.md) | Model grid, 3D preview, model inspector and tools. |
| [Model import](features/model-import.md) | FBX, OBJ, glTF and GLB straight into the game. |
| [Blender link](features/blender.md) | Send brushes, displacements or a prop to Blender and get the edit back by itself. |
| [Propper](features/propper.md) | Brushes to a static prop (LODs, collision, displacement bake) and props back to brushes. |
| [Sound browser](features/sound-browser.md) | Game sounds, soundscripts and the sound picker. |
| [Sound editor](features/sound-editor.md) | Trim, gain, loop markers and WAV output for any game sound. |
| [Color correction](features/color-correction.md) | Color lookup table editor (.raw) and 3D preview of color_correction entities. |
| [Particles](features/particles.md) | PCF system editor, operator library, live preview, browser and addon export. |

### Views and rendering

| Page | What you will find |
| --- | --- |
| [Detail props preview](features/detail-props-preview.md) | The grass and detail objects of `%detailtype` materials shown in the 3D view without compiling. |
| [Render modes and display options](features/render-modes-display.md) | 3D render modes, every display toggle, background images, frame rate. |
| [Lighting preview](features/lighting-preview.md) | Lit mode, baked lighting, shadows, normal maps, cubemap reflections, lightmap tools. |
| [Player view](features/player-view.md) | Walking the map like a player, with the compiled world, lightmaps and counters. |

### Organizing the map

| Page | What you will find |
| --- | --- |
| [Groups](features/groups.md) | Grouping and selection modes. |
| [Visgroups](features/visgroups.md) | User visgroups and auto visgroups. |
| [Display colors](features/display-colors.md) | A color per brush or entity in the 2D views, saved in the map. |
| [Hide and isolate](features/hide-isolate.md) | Hiding objects, isolating the selection. |
| [Cordons](features/cordons.md) | Cordon tool, cordon list, compiling a part of the map. |
| [Outliner](features/outliner.md) | The map as a tree. |
| [Undo and history](features/undo-history.md) | Undo, redo, selection undo, the history panel. |

### Map health and compiled maps

| Page | What you will find |
| --- | --- |
| [Check for problems](features/check-problems.md) | Problems and fixes, live checks, limits, map information, func_detail suggestions. |
| [Leaks and pointfiles](features/leaks-pointfiles.md) | Leak checks, leak path, portal files, visibility, areaportals. |
| [BSP tools](features/bsp-tools.md) | The BSP Decompiler, archives (BSP, GMA, VPK), OBJ export, comparing maps. |
| [BSP Sniffer](features/bsp-sniffer.md) | Joining a Garry's Mod server and recovering the map as the game loaded it, as a VMF, a BSP and its content (Garry's Mod only, authorized use only). |

### Testing and releasing

| Page | What you will find |
| --- | --- |
| [Game launch](features/game-launch.md) | Running and reloading the map in game, fast start options, the bridge addon. |
| [Map assets](features/map-assets.md) | Every file the map uses, where it comes from, collecting it. |
| [Build and save as content](features/build-content-release.md) | Release window, checklist, addon, Workshop publishing. |
| [Protect your BSP](features/bsp-protection.md) | Shipping a protected copy of the map, server lock. |

### The application

| Page | What you will find |
| --- | --- |
| [Settings window](features/settings-dialog.md) | Every page of the settings window. |
| [Keyboard shortcuts](features/keyboard-shortcuts.md) | Shortcut sets, rebinding, conflicts, `F1` reference. |
| [Command palette](features/command-palette.md) | `Ctrl+P`: every command by name. |
| [Multiple maps](features/multiple-maps.md) | Tabs, switching maps, saved layouts. |
| [Themes and languages](features/themes-languages.md) | Dark and light themes, accent color, scale, background image, English, French and Russian. |
| [Window frames and separate windows](features/window-frames.md) | Hammer Pro frames, native frames, windows on other screens. |
| [Autosave and crash recovery](features/autosave-crash.md) | Autosaves, emergency saves, crash reports. |
| [Discord Rich Presence](features/discord.md) | What your Discord profile shows while you map. |
| [Updates](features/updates.md) | The startup check for a newer release, the update window and the automatic install. |
| [Storage](features/storage.md) | Disk space used by Hammer Pro, what uses the most, and cleaning caches and temporary files safely. |

## Compiler

| Page | What you will find |
| --- | --- |
| [The built in compiler](compiler/overview.md) | Geometry, visibility and lighting, and how they differ from VBSP, VVIS and VRAD. |
| [Compile and run window](compiler/compile-window.md) | Presets, steps, options, expert mode, batch compile, history and statistics. |
| [Compile options](compiler/compile-options.md) | Every compile flag with its effect and default. |
| [Choosing a compiler](compiler/compilers.md) | Valve's vbsp, vvis and vrad or your own programs instead of the built in compiler, with every option. |
| [Compiler command line](compiler/command-line.md) | `hammerpro --bspc` and `hammerpro --rad`, every argument. |
| [Worldspawn and light keys](compiler/worldspawn-keys.md) | `_lightsrad` and the other keys the compiler reads. |
| [Target games](compiler/target-games.md) | Supported Source games and engine branches. |
| [Cubemaps](compiler/cubemaps.md) | Building cubemaps, writing them into the map, parallax corrected cubemaps. |
| [Limits and logs](compiler/limits-and-logs.md) | Engine limits per game, reading the compile log and the statistics. |
| [Compile performance](compiler/performance.md) | Making compiles faster. |

## Reference

| Page | What you will find |
| --- | --- |
| [Settings reference](reference/settings.md) | Every key of `settings.toml` with its default. |
| [Actions and keyboard shortcuts](reference/actions-shortcuts.md) | Every command, its menu, its id and its shortcut in each shortcut set. |
| [Command line](reference/command-line.md) | Every argument of `hammerpro.exe`. |
| [Script commands](reference/script-commands.md) | The commands of `--script` files. |
| [MCP server](reference/mcp-server.md) | Connecting an AI assistant (Claude, Codex, Gemini). |
| [MCP tool reference](../MCP.md) | Every MCP tool (generated). |
| [Files and folders](reference/files-and-folders.md) | Every file Hammer Pro reads or writes. |

## Guides

| Guide | What you will do |
| --- | --- |
| [Compile and test in Garry's Mod](guides/compile-and-test-gmod.md) | The fastest loop from editing to playing. |
| [Make a prop with Propper](guides/make-a-prop-with-propper.md) | Turn brushes into a static prop with LODs and collision. |
| [Sculpt displacements](guides/sculpt-displacements.md) | Terrain from a brush to painted blends. |
| [Lighting](guides/lighting.md) | Lights, previews, LDR and HDR, static prop lighting. |
| [Custom grids for stairs](guides/custom-grids-for-stairs.md) | A grid with a step per axis for regular stairs. |
| [Collect content for a release](guides/collect-content-for-release.md) | Gather every custom file the map needs. |
| [Port a map to another game](guides/port-to-another-game.md) | Change the target game and fix what differs. |
| [Use Hammer Pro with an AI assistant](guides/mcp-with-ai-assistant.md) | Set up MCP and work with an assistant. |

## Troubleshooting

| Page | What you will find |
| --- | --- |
| [FAQ](troubleshooting/faq.md) | The questions everyone asks. |
| [Common errors](troubleshooting/common-errors.md) | Leaks, missing textures, limits, failed compiles. |
| [Crashes and logs](troubleshooting/crashes-and-logs.md) | Crash reports, log file, GPU problems, native frames. |
| [Out of memory](troubleshooting/out-of-memory.md) | Graphics memory budget, many big maps open, computer memory. |
| [Graphics device lost](troubleshooting/graphics-device-lost.md) | Driver resets, what is saved, restarting with your tabs. |
| [Editor performance](troubleshooting/performance.md) | Keeping the editor smooth on huge maps. |
| [Discord presence not showing](troubleshooting/discord-presence.md) | Fixing Rich Presence. |
| [Reporting bugs](troubleshooting/reporting-bugs.md) | What to send so a bug can be fixed. |

## Development

| Page | What you will find |
| --- | --- |
| [Building from source](development/building.md) | Rust toolchain, build profiles, tests. |
| [Architecture](development/architecture.md) | The crates and how they depend on each other. |
| [Contributing](development/contributing.md) | Pull request rules. |
| [Extending Hammer Pro](development/extending.md) | The extension points of the code. |

---
[Back to the README](../../README.md)
