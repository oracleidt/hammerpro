<p align="center"><img src="assets/hammer-pro.png" width="96" alt="Hammer Pro logo: a white hammer on a black square"></p>

# Hammer Pro

**A modern, fast level editor for Source engine maps, built for Garry's Mod first.**

[Lire en français](README.fr.md) | [Documentation](docs/en/README.md) | [Documentation en français](docs/fr/README.md) | [Discord](https://discord.gg/eAEWWKRNAe)

Hammer Pro opens and saves standard `.vmf` files, so you can use it side by side with Hammer and Hammer++. It is written in Rust, renders with the GPU through wgpu, stays smooth on huge maps and compiles maps with its own built in compiler: no VBSP, VVIS or VRAD needed.

Version **1.0.3**.

## Highlights

Every feature has its own page in the [documentation](docs/en/README.md).

- **Instant loading.** A 100,000 brush VMF parses in about 0.3 s. Game content (VPKs, addons, Workshop) mounts in the background. See [Game setup](docs/en/getting-started/game-setup.md).
- **Lossless VMF round trip.** Maps written by Hammer come back out unchanged, Hammer++ data (`vertices_plus`, `*_plus` blocks) is kept. See [VMF compatibility](docs/en/getting-started/vmf-compatibility.md).
- **All your content.** Base game VPKs, addons and `.gma` files, Workshop addons (legacy `.bin` too), `mount.cfg` games, content downloaded from servers and [your own texture folders](docs/en/features/custom-texture-folders.md), where plain PNG, JPG, TGA and BMP images work as materials.
- **Every Hammer tool.** [Selection](docs/en/features/selection-tool.md) with handles, 3D stretch handles and a gizmo, [block tool and primitives](docs/en/features/block-tool.md), [polygon tool](docs/en/features/polygon-tool.md), [curve tool](docs/en/features/curve-tool.md), [entity tool](docs/en/features/entity-tool.md), [clipping](docs/en/features/clip-tool.md), [vertex tool](docs/en/features/vertex-tool.md), [displacements](docs/en/features/displacement-tool.md), [texture application](docs/en/features/texture-tool.md), [decals and overlays](docs/en/features/decals-overlays.md), [path tool](docs/en/features/path-tool.md), [sprinkle](docs/en/features/sprinkle-tool.md), [cordons](docs/en/features/cordons.md), [groups](docs/en/features/groups.md), [visgroups](docs/en/features/visgroups.md), [instances](docs/en/features/instances.md), [prefabs](docs/en/features/prefabs.md), [I/O editor](docs/en/features/io-logical-view.md), [check for problems](docs/en/features/check-problems.md).
- **More than Hammer.** An [edge bevel tool](docs/en/features/bevel-tool.md) (round, chamfer, cove or arch an edge with `Ctrl+B` in the 3D view), a [UV editor](docs/en/features/uv-editor.md), a [material editor](docs/en/features/material-editor.md), [custom grids](docs/en/features/grids.md) (a step per axis, rotated, attached to objects), a [command palette](docs/en/features/command-palette.md), an [outliner](docs/en/features/outliner.md), a [logical view](docs/en/features/io-logical-view.md), [selection undo](docs/en/features/undo-history.md), [autosave and crash recovery](docs/en/features/autosave-crash.md), [several maps in tabs](docs/en/features/multiple-maps.md).
- **See the map like the game.** [Real time lighting preview](docs/en/features/lighting-preview.md) with shadows, normal maps and cubemap reflections, a baked lighting preview, and a [player view](docs/en/features/player-view.md) (`F6`) that walks the compiled world with its lightmaps.
- **Props and models.** [Propper](docs/en/features/propper.md) turns brushes into a static prop (with LODs and collision) and props back into brushes, [model import](docs/en/features/model-import.md) brings FBX, OBJ and glTF into the game, the [model browser](docs/en/features/model-browser.md) previews every model in 3D.
- **Its own compiler.** Geometry, visibility and lighting (LDR and HDR, ambient occlusion, static prop lighting) for Garry's Mod and other Source games, with smart recompiles, background precompiles and a compile log with clickable errors. Valve's vbsp, vvis and vrad, or programs of your own, can be chosen instead for each game. See [The built in compiler](docs/en/compiler/overview.md), [Choosing a compiler](docs/en/compiler/compilers.md) and [Target games](docs/en/compiler/target-games.md).
- **Map health.** Live problem checks, an instant leak check with the leak path, engine limits per game, portal files and visibility. See [Leaks and pointfiles](docs/en/features/leaks-pointfiles.md).
- **Ship it.** [Map assets](docs/en/features/map-assets.md) lists and collects every file the map uses, [Build and save as content](docs/en/features/build-content-release.md) makes a ready to share addon with a release checklist and Workshop publishing, [Protect your BSP](docs/en/features/bsp-protection.md) ships a protected copy of the map.
- **Gameplay helpers.** [Navigation mesh generation](docs/en/features/nav-mesh.md), [several 3D skyboxes and a skybox switch entity](docs/en/features/sky-cameras.md), [soundscapes](docs/en/features/soundscapes.md), [particle preview](docs/en/features/particles.md), [game launch and reload](docs/en/features/game-launch.md).
- **AI assistants.** A built in [MCP server](docs/en/reference/mcp-server.md) lets Claude, Codex or Gemini read and edit the open map, check it, compile it and take screenshots, every edit being one undo step.
- **Your editor.** Black and white interface (dark or light), [English and French](docs/en/features/themes-languages.md), [Hammer and Hammer++ shortcut sets](docs/en/features/keyboard-shortcuts.md), [Discord Rich Presence](docs/en/features/discord.md), [automatic updates](docs/en/features/updates.md).

## Requirements

- Windows 10 or 11 (64-bit). Linux builds compile from source too.
- A GPU with Vulkan, DirectX 12 or Metal support.
- Garry's Mod installed through Steam (found automatically). Other Source games are supported as targets.

## Download and install

1. Download `hammerpro.exe` from the Releases page of this repository, or [build it from source](docs/en/development/building.md).
2. Put it in any folder and run it. There is no installer.
3. On the first start, the setup assistant asks for your language, the editor you come from (Hammer, Hammer++ or the Hammer Pro defaults) and your game content.

Details: [Installation](docs/en/getting-started/installation.md) and [Game setup](docs/en/getting-started/game-setup.md).

## Quick start

1. Open a `.vmf` (**File > Open map...**, `Ctrl+O`, or drag it onto the window) or start a new map (`Ctrl+N`).
2. Draw a room with the block tool (`Shift+B`), place entities with the entity tool (`Shift+E`), texture faces with `Shift+A`.
3. Press `F9` to compile and run the map in Garry's Mod.

Follow [Your first map](docs/en/getting-started/first-map.md) for a complete walk through.

| Action | Keys |
|---|---|
| Fly the 3D camera | hold the right mouse button + `W A S D` (`Z Q S D` on AZERTY), `Shift` faster |
| Toggle mouselook | `Z` |
| Pan / zoom 2D views | middle mouse or `Space` + drag / mouse wheel |
| Grid smaller / larger | `[` / `]` |
| Selection, block, entity, texture, clip, vertex tools | `Shift+S`, `Shift+B`, `Shift+E`, `Shift+A`, `Shift+X`, `Shift+V` |
| Command palette | `Ctrl+P` |
| Compile now | `F9` |
| Player view | `F6` |
| Every shortcut | `F1` |

All shortcuts are listed in [Actions and keyboard shortcuts](docs/en/reference/actions-shortcuts.md) and can be changed in **Settings > Keyboard**.

## Building from source

```bash
git clone <this repository>
cd HammerPro
cargo build --release -p hammerpro
```

The executable is `target/release/hammerpro.exe`. Rust 1.95 or newer is required. Run the tests with `cargo test --workspace` (tests that need Garry's Mod skip themselves when it is not installed). See [Building from source](docs/en/development/building.md) for build profiles and tips, and [Architecture](docs/en/development/architecture.md) for the crates.

## Documentation

- [English documentation](docs/en/README.md): getting started, one page per feature, the compiler, references (settings, shortcuts, command line, scripts, MCP), guides, troubleshooting and development.
- [Documentation en français](docs/fr/README.md).
- [MCP tool reference](docs/MCP.md) (generated) and [developer guide to extending Hammer Pro](docs/EXTENDING.md).

## Community and contact

- Discord server: [discord.gg/eAEWWKRNAe](https://discord.gg/eAEWWKRNAe) (help, feedback, bug reports, news)
- Author on Discord: `oracleidt`

## Contributing

Contributions are welcome. Please read [CONTRIBUTING.md](CONTRIBUTING.md). Translations live next to the code in each crate's `TR` table (English and French); adding a language means adding one column.

## License

MIT. See [LICENSE](LICENSE).

Hammer Pro is not affiliated with Valve or Facepunch Studios. Source, Hammer and Garry's Mod are trademarks of their respective owners. Hammer Pro does not include or redistribute any Valve code or game content.
