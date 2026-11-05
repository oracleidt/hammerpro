[Documentation](../README.md) > Development > Architecture | [Français](../../fr/development/architecture.md) | [Русский](../../ru/development/architecture.md)

# Architecture

Hammer Pro is split into small, focused crates under `crates/`, with dependencies flowing in one direction (no cycles). This page lists every crate in one line and shows how they depend on each other. The full developer document (goals, conventions, coordinate systems, data flow) is [ARCHITECTURE.md](../../ARCHITECTURE.md), in English.

## Crates

### File formats and libraries

| Crate | Role |
|---|---|
| `hp-kv` | Fast and lenient parser and writer for Valve KeyValues text (VMF, VMT, `gameinfo.txt`, Steam `.vdf`). |
| `hp-vmf` | VMF document model, loader and writer: lossless, fast, compatible with Hammer and Hammer++. |
| `hp-vmt` | VMT material parsing and writing (shader, parameters, proxies, fallbacks). |
| `hp-vtf` | Decoder and encoder of the Valve Texture Format. |
| `hp-vpk` | Thread safe reader of VPK archives. |
| `hp-gma` | Thread safe reader of Garry's Mod addon archives (GMA), legacy LZMA `.bin` included. |
| `hp-fgd` | Parser and resolver of FGD entity definition files. |
| `hp-mdl` | Loader of studio models (`.mdl` + `.vvd` + `.vtx`), MDL versions 44 to 49. |
| `hp-bsp` | Hardened reader and writer of BSP files, versions 19 to 21: lumps, pakfile, cubemaps, game lumps, Protect your BSP. |
| `hp-nav` | Reader of navigation meshes (`.nav`), versions 1 to 16. |
| `hp-pcf` | Particle systems of `.pcf` files (binary DMX) and a small preview simulation. |
| `hp-meshio` | Reading OBJ, glTF/GLB and FBX models and compiling them into game model files. |
| `hp-shader` | The engine of the shader editor: node graph (`.hpshader` JSON), node catalog, CPU evaluation, presets, and writing the outputs as VTF and VMT. No UI code. |

### Geometry, content and compiling

| Crate | Role |
|---|---|
| `hp-geom` | Brush geometry: planes, polygons, texture math, CSG, primitives, picking, displacements. |
| `hp-fs` | Game filesystem and asset index: game detection, `gameinfo.txt`, VPKs, addons, Workshop, `mount.cfg`, custom texture folders. |
| `hp-light` | Lighting helpers shared by the compiler, the baked preview and the editor's preview lights (light parsing, `.rad` files, ray hierarchy). |
| `hp-bspc` | The map compiler: VMF to BSP, written from scratch in the steps of Valve's VBSP, plus visibility and lighting. |
| `hp-compile` | The compile pipeline: runs `hammerpro --bspc` and `--rad`, packs custom content, copies the map and launches the game, streams the log. |
| `hp-decompile` | Rebuilds an editable map from a compiled BSP, in the spirit of BSPSource. |

### Editor core and application services

| Crate | Role |
|---|---|
| `hp-editor` | The editing core: document, selection, undo/redo, editing operations, picking. No UI or GPU code. |
| `hp-render` | The GPU renderer (wgpu): 3D and 2D views, batching, overlays. |
| `hp-app` | Shared application core: state, settings, actions, i18n, theme, view math, tool interface and the extension points of the packages. |
| `hp-assets` | Asset system: mounting game content and FGDs in the background, streaming materials, models and sprites, scene sync of the renderer. |
| `hp-mcp` | The MCP server of a window, the `--mcp` stdio bridge and the AI app installer (see [MCP.md](../../MCP.md)). |
| `hp-discord` | Discord Rich Presence over the local Discord IPC protocol. |
| `hp-update` | Auto update: GitHub release check, download (WinHTTP), zip reader, and the helper that replaces the program (see [Updates](../features/updates.md)). |

### User interface and tools

| Crate | Role |
|---|---|
| `hp-ui-shell` | Menu bar, toolbar, tool palette, status bar, console, toasts, settings window, command palette, shortcuts and About dialogs, first run wizard. |
| `hp-ui-viewports` | The 2x2 views: layout, splitters, camera navigation, rendering, input routing to the tools, context menu, drag and drop. |
| `hp-ui-textures` | Texture browser, texture application tool (Face Edit), find and replace textures, material (VMT) editor, image import, the shader editor window. |
| `hp-ui-entities` | Entity properties (SmartEdit, flags, I/O, keyvalues), class picker, model browser, find entities, entity report, map properties. |
| `hp-ui-mapping` | Visgroups, outliner, compile window with live log, check for problems, transform, paste special, go to id, map information, pointfiles. |
| `hp-ui-lab` | Larger features: logical view, nav mesh display, opening compiled maps, particle preview, map comparison. |
| `hp-tools-select` | Selection tool (click and box select, handles, clone drag, 3D gizmo, nudging) and camera tool. |
| `hp-tools-create` | Creation tools: block (primitives), entity, polygon, curve, road, path, clipping, decal, overlay, cordon, sprinkle. |
| `hp-tools-mesh` | Vertex tool and displacement tool. |
| `hammerpro` | The application (eframe/egui): packages, action dispatch, MCP tools, scripts, command line, crash handling. |

## Dependency direction

Dependencies always point down this list of layers, never up (inside the UI layer, `hp-ui-lab` also uses `hp-ui-viewports`):

```
hammerpro                      the application, depends on everything below
hp-ui-*, hp-tools-*            UI and tools
hp-assets                      asset streaming
hp-app                         shared state, settings, actions, i18n
hp-editor, hp-render           editing core, renderer
hp-bspc, hp-decompile          compiler, decompiler
hp-light, hp-compile           lighting helpers, compile pipeline
hp-fs, hp-geom                 game filesystem, brush geometry
hp-vmf, hp-vmt                 formats built on hp-kv
hp-kv, hp-bsp, hp-vpk, hp-gma, hp-vtf, hp-mdl, hp-fgd, hp-nav, hp-pcf, hp-meshio
hp-discord, hp-mcp, hp-update   standalone (no internal dependency)
```

Some concrete edges:

```
hp-kv <- hp-vmf <- hp-geom <- hp-editor <- hp-app <- hp-ui-*, hp-tools-* <- hammerpro
hp-kv, hp-vpk, hp-gma, hp-vtf, hp-vmt, hp-mdl, hp-bsp <- hp-fs <- hp-light <- hp-bspc
hp-bsp, hp-kv <- hp-compile
hp-vmf, hp-geom, hp-bsp <- hp-decompile <- hp-ui-lab
hp-discord <- hp-ui-shell
```

Libraries return errors and never panic on bad input; heavy work runs on worker threads and reports back through channels, so the UI thread stays free.

## Adding a feature

Feature packages plug into shared registries of `hp-app` (actions, settings, menus, settings pages, status bar segments, overlays, sidecar data) instead of editing the core. See [Extending Hammer Pro](extending.md).

## Related pages

- [ARCHITECTURE.md](../../ARCHITECTURE.md) (full developer document, English)
- [Building from source](building.md)
- [Contributing](contributing.md)
- [Extending Hammer Pro](extending.md)

---
[Back to the documentation index](../README.md)
