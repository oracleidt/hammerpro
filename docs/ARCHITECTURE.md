# Hammer Pro architecture

Hammer Pro is a modern level editor for Source engine maps, focused first on
Garry's Mod. It reads and writes `.vmf` files, so it works side by side with
Hammer and Hammer++.

## Goals

- Open anything fast. Big VMFs load in well under a second; assets stream in
  the background so the UI never blocks.
- Everything Hammer++ does, with a cleaner UI and more power tools.
- English, French and Russian UI, switchable live.
- Pure Rust, GPU rendering through wgpu, UI through egui.

## Crates

```
hp-kv        KeyValues text parser and writer (VMF, VMT, gameinfo, VDF)
hp-vmf       VMF document model, lossless load and save
hp-geom      Brush math: planes, polygons, UVs, CSG, primitives, picking, displacements
hp-vpk       VPK archive reader
hp-gma       GMA (Garry's Mod addon) archive reader, including legacy LZMA .bin
hp-vtf       VTF texture decoder and encoder
hp-vmt       VMT material parser and writer
hp-fgd       FGD entity definition parser and resolver
hp-mdl       MDL / VVD / VTX studio model loader
hp-ragdoll   PHY skeleton, constraints, saved poses and skinning
hp-physics   Rigid bodies, joints, convex water buoyancy and editor interaction
hp-bsp       BSP reader and writer: lumps, pakfile, cubemaps, game lumps, LZMA
hp-decompile BSP to VMF decompiler (brushes, displacements, entities, pakfile content)
hp-nav       Navigation mesh (.nav) reader
hp-pcf       Particle system (.pcf) reader and preview simulation
hp-audio     Sound decoding, editing and WAV output, playback, the multi voice soundscape mixer
hp-soundscape Soundscape scripts, library, nesting, active soundscape rules, sound levels, random scheduling
hp-fs        Game filesystem: gameinfo, VPKs, mount.cfg, addons, workshop, custom folders
hp-compile   Compile pipeline (the built in compiler, in process packing), log parsing, leak pointfiles
hp-render    wgpu renderer: 3D and 2D views, batching, overlays
hp-editor    Document, selection, undo/redo, editing operations, picking
hp-light     Lighting helpers: VRAD light parsing, .rad files, ray hierarchy, preview atlas
hp-mcp       MCP server of a window, `--mcp` stdio bridge, AI app installer (see MCP.md)
hp-update    Auto update: GitHub release check, download, zip reader, program replacement helper
hp-app       Shared state, settings, actions, i18n, theme, the extension points of the packages
hp-assets    Asset streaming (materials, models, sprites) and the scene sync of the renderer
hp-ui-*      User interface: shell, viewports, textures, entities, mapping, lab
hp-tools-*   Editing tools: create, select, mesh, physics
hammerpro    The application (eframe/egui): packages, dispatch, MCP tools, scripts
```

Dependency direction (no cycles):

```
hp-kv <- hp-vmf <- hp-geom <- hp-editor <- hammerpro
hp-kv <- hp-vmt, hp-fgd, hp-fs
hp-vpk, hp-gma, hp-vtf <- hp-fs <- hammerpro
hp-mdl, hp-compile, hp-render <- hammerpro
hp-vmf, hp-fs <- hp-light <- hp-bspc, hp-assets, hammerpro
```

## Conventions

- Coordinates are Source units, Z up, right handed like the engine.
- Documents use `f64` (`glam::DVec3`). GPU data uses `f32`.
- Plane convention (VMF `plane "(a) (b) (c)"`): outward normal is
  `normalize((a - b).cross(c - b))`, distance is `normal.dot(a)`.
- Texture coordinates are passed to the GPU in texel units:
  `u = dot(p, uaxis) / uscale + ushift`, `v = dot(p, vaxis) / vscale + vshift`.
  The shader divides by the texture size, so meshes never need a rebuild when
  a texture finishes loading.
- Material names are compared case insensitively, with forward slashes, no
  `materials/` prefix and no `.vmt` extension. The original case is kept when
  writing a VMF.
- Virtual filesystem paths (hp-fs) are lowercase, relative, forward slashes,
  with extension: `materials/concrete/concretefloor001a.vmt`.
- 2D views:
  - Top: +X right, +Y up, looking down -Z.
  - Front: +Y right, +Z up, camera on +X looking toward -X.
  - Side: +X right, +Z up, camera on -Y looking toward +Y.
- Libraries return errors (`thiserror`) and never panic on malformed files.
  The app uses `anyhow` at the edges.
- Never use the em dash or en dash characters anywhere (code, comments, UI
  strings, docs). Use a hyphen, a colon or parentheses.
- All UI text goes through the i18n layer (`hp_app::i18n`), with English,
  French and Russian translations. English and French entries are registered
  by each crate. The Russian catalog is bundled in `hp-app/src/i18n/ru.json`
  and loaded once when selected; the application tests validate coverage.

## Data flow

1. Startup: settings load, Garry's Mod install is detected, `hp-fs` mounts
   game content (VPKs, addons, workshop GMAs, custom texture folders) and
   builds an asset index on a background thread.
2. Open map: `hp-vmf` parses the VMF, `hp-editor` builds the scene and brush
   geometry in parallel with rayon.
3. Rendering: the app syncs document changes to `hp-render` (only changed
   objects are re-uploaded). Textures and models load on worker threads and
   are uploaded as they arrive.
4. Editing: tools in the app translate input into `hp-editor` operations.
   Every operation is an undoable transaction.
5. Compile: the document is saved to a VMF, `hp-compile` runs the pipeline on
   a worker thread and streams log lines, progress and leak info back to the UI.

## Extending

Feature packages plug into shared registries (actions, settings, dispatch,
menus, settings pages, status bar, overlay layers, scene producers, sidecar
state). See [EXTENDING.md](EXTENDING.md) before adding a feature.
