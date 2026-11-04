# MCP: let AI apps edit your map

Hammer Pro has an MCP server built in. MCP (the Model Context Protocol) is
how AI apps such as Claude (Claude Code and Claude Desktop), Codex and
Gemini CLI use tools. With it, you can ask the AI to look at the map open in
Hammer Pro, add brushes and entities, change materials, check for problems,
save, compile, and take screenshots of the views to check its own work.

Every change the AI makes is one undo step named "MCP: ..." in the Edit
menu, so Ctrl+Z takes it back like any of your own edits.

## Quick start

1. In Hammer Pro, click the **MCP off** pill at the right end of the top
   bar. It turns into **MCP on · port · clients**. The server of that
   window is now on.
2. Click the small arrow next to the pill (or Tools > MCP...), then
   **Install in Claude, Codex, Gemini**. Hammer Pro adds itself to every
   app it finds on this computer, in this order: Claude Code, Claude
   Desktop, Codex, Gemini CLI, and checks each one.
3. Restart the AI app (or start a new session) and ask it, for example:
   "Look at my Hammer Pro map and add a 512x512 floor with a light above it."

The pill is off by default in every window. To have it on at start, tick
**Settings > MCP > Start the MCP server when Hammer Pro starts**.

## How it works

```text
AI app (Claude, Codex, Gemini)
    | starts "hammerpro.exe --mcp" and talks MCP on its stdin and stdout
hammerpro --mcp                     the bridge: no window
    | 127.0.0.1, random port, secret token
Hammer Pro window with MCP on       runs the tools on the map it shows
```

- The AI app starts `hammerpro.exe --mcp` itself. That process opens no
  window: it forwards the tool calls to a Hammer Pro window that has MCP on.
- With several windows, the bridge uses the one you focused last, and stays
  with it while it runs. The `list_instances` and `use_instance` tools let
  the AI choose another one; the `HAMMERPRO_INSTANCE` environment variable
  (a process id) names one in the app's configuration.
- When no window has MCP on, the AI still sees the tools, and every tool
  answers that you must open Hammer Pro and switch MCP on in the top bar.
- The bridge speaks the MCP protocol versions 2025-11-25, 2025-06-18,
  2025-03-26 and 2024-11-05 (newline delimited JSON-RPC 2.0 over stdio).

## Security

- **Off by default**, per window. Nothing listens until you click the pill.
- **Local only**: the server listens on `127.0.0.1` (never on the network)
  and refuses any connection that is not from this computer.
- **Secret token**: each time a server starts it makes a random 32 byte
  token. Every connection must send it first or is closed. The token is
  only written in the window's instance record,
  `%LOCALAPPDATA%\HammerPro\mcp\instances\<pid>.json`, in your own profile.
  The record is removed when the server stops or Hammer Pro closes;
  records of crashed windows are ignored and cleaned up.
- **Limits**: messages up to 8 MB, 8 connections at a time per window, a
  timeout on every tool call.
- **No programs unless you ask, no arbitrary files**: the tools run no
  command of their own. Files are only `.vmf` maps: `open_map` opens a
  `.vmf`, `save_map` saves the map to its own file or as a new `.vmf` in the
  same folder (replacing another file needs `overwrite: true`). `compile_map`
  compiles with the compiler chosen for the game (Hammer Pro's own by
  default), and starts the game only when the AI explicitly asks for it.
  Creating or changing a compiler that runs other programs
  (`create_compiler`, `set_compiler` with steps) needs `confirm: true`, which
  the AI passes only when you asked for it.
- `run_action` runs any command of the menus, except a few that need
  `confirm: true`, which the AI passes only when you asked for exactly that:
  Exit, switching MCP off, installing MCP in other apps, publishing,
  starting or talking to the game, compiling with the game, editing an
  instance in another window, opening the web browser, and the commands
  that show a file dialog. Automated runs (`--script`) never start the
  game, confirmed or not.
- `set_setting` never changes file paths, folders, programs, command lines,
  the game content folders or the MCP settings: you change those in
  Settings. Key bindings go through `set_keybinding`. The one exception is
  the list of extra entity definition files: `add_fgd` and `remove_fgd`
  take `.fgd` files only and need `confirm: true`; `validate_fgd` reads a
  `.fgd` file without loading it.
- The compile settings go through `set_compile_options` (the same settings
  as the compile window and the toolbar): starting the game after every
  compile (`launch_game: true`) and the folder the BSP is copied to
  (`maps_dir_override`, an existing folder) need `confirm: true`. The
  options of the compiler steps (`bsp_options`, `rad_options`) are checked
  against the compiler's own flags: `-lights` and `-embed` take files of the
  map folder only, and Hammer Pro writes `-game`, `-gmod`, `-threads`, the
  dynamic range and the mode flags itself (from the game configuration,
  `threads`, `lighting` and the modes).
- `edit_uv_layer` writes layer variants and baked textures in the first
  custom texture folder, and changes a material's own file only when it is
  in a custom texture folder (a backup is kept).
- "Build and save as content" (`start_release`) compiles the map and writes
  its content folder and GMA next to it: a build needs `confirm: true`, an
  analysis (`analyze_only`) writes nothing. Building the cubemaps and the
  navigation mesh in the game during a build (`game_session`) starts Garry's
  Mod: only when you asked, never in automated runs.
- The UI automation tools (`ui_click`, `ui_key`, `ui_type`...) act inside
  the Hammer Pro window only, like your mouse and keyboard there; positions
  outside the window are refused.

Tool calls run on the editor's own thread, between two frames, like a click
of yours.

## Install

The **Install in Claude, Codex, Gemini** button (Tools > Install MCP in
Claude, Codex and Gemini) and the per app buttons of the MCP window do the
following. Each app line shows its state: not installed on this computer,
installed but not configured, installed and configured, configured for
another copy of Hammer Pro (click Update), or the error. The apps' own
programs (the `claude` command) only run when you click Install, Verify or
Remove.

| App | What is changed |
|---|---|
| Claude Code | `claude mcp add --scope user hammer-pro -- "<hammerpro.exe>" --mcp` (checked with `claude mcp get hammer-pro`). `~/.claude.json` is never edited directly: running Claude Code sessions rewrite it. |
| Claude Desktop | `mcpServers` of `%APPDATA%\Claude\claude_desktop_config.json` (or the Microsoft Store copy under `%LOCALAPPDATA%\Packages\Claude_*`) |
| Codex | `[mcp_servers.hammer-pro]` of `~/.codex/config.toml` (or `%CODEX_HOME%`) |
| Gemini CLI | `mcpServers` of `~/.gemini/settings.json` |

Before the first change, each file is copied to `<file>.hammerpro.bak`.
Files are written through a temporary file, everything else in them is kept
(other servers, comments in the TOML file), and the result is read back to
verify it. A JSON file with comments is not touched: add the entry by hand.

Moved Hammer Pro to another folder? Open the MCP window: the apps show
"configured for another copy"; click Update (or Install in Claude, Codex,
Gemini).

## Manual setup

The MCP window shows these snippets with the real path of your
`hammerpro.exe` and a Copy button (Manual setup).

Claude Code:

```bash
claude mcp add --scope user hammer-pro -- "C:\Program Files\Hammer Pro\hammerpro.exe" --mcp
```

Claude Desktop (`claude_desktop_config.json`) and Gemini CLI
(`~/.gemini/settings.json`):

```json
{
  "mcpServers": {
    "hammer-pro": {
      "command": "C:\\Program Files\\Hammer Pro\\hammerpro.exe",
      "args": ["--mcp"]
    }
  }
}
```

Codex (`~/.codex/config.toml`):

```toml
[mcp_servers.hammer-pro]
command = 'C:\Program Files\Hammer Pro\hammerpro.exe'
args = ["--mcp"]
```

Any other MCP app: a stdio server with the command `hammerpro.exe` and the
argument `--mcp`.

## Tools

Coordinates are Source units (Z up). Ids are the VMF ids; when an entity and
a solid share an id, `ids` names the entity and `solid_ids` the solid.

<!-- BEGIN TOOL LIST: generated from the registry, see tools_help.rs -->

302 tools. Arguments marked * are required. `get_capabilities` gives the same list with an example call for each tool.

### Help

| Tool | What it does | Arguments |
|---|---|---|
| `get_capabilities` | Every Hammer Pro MCP tool by category with an example call, and tips; or one category, or one tool with its full description and argument schema. Start here. | `category`: text; `tool`: text |

### Map

| Tool | What it does | Arguments |
|---|---|---|
| `get_map_info` | Describe the map open in the Hammer Pro window: file, unsaved changes, numbers of solids, entities, faces and groups, map bounds, selection, undo and redo steps, game content and compile state. Call it first. | none |

### Files

| Tool | What it does | Arguments |
|---|---|---|
| `save_map` | Save the map to its file, or as a new .vmf in the folder of the current map file (a file name like "my_map_v2.vmf"). Replacing another existing file needs overwrite true. | `overwrite`: true or false; `path`: text |
| `open_map` | Open a .vmf file in the editor (in a new tab, or after the user answers the unsaved changes question). Returns at once: call get_map_info to see it. | `path`*: text |

### Bsp sniffer

| Tool | What it does | Arguments |
|---|---|---|
| `sniffer_start` | Start a BSP Sniffer run (Garry's Mod only): launches Garry's Mod through Steam, joins the server, waits for the map to load and the engine to finish building it, and rebuilds the whole map from the game memory alone (entities, brushes, materials, planes, nodes, leaves, faces, texture axes and data, visibility, lightmaps, ambient lighting, world lights, physics collision, displacements and their lightmap samples, vertex normals, primitives, area portals, cubemaps and their sizes, static props with their fade distances and flags (five of the eight flag bits come from the memory; ignore normals, no shadow and no self shadowing exist only for the compiler and need the map file), overlays with their render order, occluders, displacement vertex alphas (blend painting) and allowed vertices, and the other structures the engine keeps; the engine world structure is found from its planes by signature and every lump of the result is listed with its fidelity), writes the BSP, decompiles it to a VMF and saves the content. No map file is read by default: the setting use_map_file (default off) lets a map file of the client (the game cache, maps, download, addons, workshop archives, read through its lump directory, so a protected, shuffled or padded file works) cross check the memory and complete what the memory does not keep, field by field after a consistency check. The game loads one lighting set only: read_ldr_lighting (default on) switches the game to LDR a second time to read the other one. The result lists the source of each category (memory, map file, game content, absent). The content step rebuilds, for the models the game loaded and that are not on this computer, the .mdl, the .vvd (found in the model cache), the .dx90.vtx (rebuilt from the hardware mesh groups of the model cache) and the .phy (rebuilt from the collision data and packed key values of the model cache), and the .vmt text of the loaded materials not on this computer (key values when the engine kept them, else the material variables and the decoded proxies; Lua proxies read from the Lua state); textures exist in video memory only, they are listed in textures_in_video_memory.txt with the measured residency in texture_residency.txt; the pakfile directory the game holds (names, sizes, CRC32) is listed in pakfile_listing.txt and every recovered file is checked against it. Runs in the background: poll sniffer_status and read sniffer_log. Needs the setting authorized = true (the user confirms they may test the server; set it with sniffer_set_settings only when the user said so). Takes the game lock rule of the machine: only one run at a time. Protected maps (Protect your BSP: logic removed, material names scrambled, original faces emptied, watermark, server lock, lumps shuffled) are recovered the same way, because the engine loads the lumps it needs whatever their order or padding: the scrambled material names are restored from the loaded materials; the report says how many of each. Entities: the entity lump the client holds is merged with the networked entities of the client (class name, origin, angles, model, spawn flags, render fields, skin, body, scale and parent names), so entities that a protector removed from the shipped file but that the server still sends to the client (weapons, items, npcs, props, brush entities) come back, and brush models that no entity uses become brush entities; what the server never sends (logic, nodes, paths, the inputs and outputs and hammer ids of removed entities) cannot be recovered, and sniffer_result gives the lower bound of how many entities are missing. The address can also be map:<name> to start a map of this computer in a local Garry's Mod (listen server), or map: alone with attach_running to read the map the running game has loaded. | `address`: text |
| `sniffer_cancel` | Stop the running BSP Sniffer; the game it started is closed when kill_game is on. | none |
| `sniffer_status` | Phase (idle, preflight, querying, launching, waiting_game, waiting_map, capturing, verifying, rebuilding, decompiling, extracting, finishing, done, failed, cancelled), progress 0 to 1, detail, elapsed seconds, game process, map, error, and when done the output files and the comparison counts. | none |
| `sniffer_log` | The log of the current or last run. Give from (the next value of the previous call) to read only new lines. | `from`: integer 0 to 9223372036854775807; `limit`: integer 1 to 2000 |
| `sniffer_result` | The full report of the last finished run: the source of the geometry, what was decoded from the game memory (faces, texture axes, displacements with their vertex alphas and allowed vertices, static props with fade distances and flags, overlays with render order and fade distances, cubemaps with sizes, area portals, occluders, the .mdl, .vvd, .dx90.vtx and .phy files of the loaded models, the material key values), the number of map file reads, the cross check against the map file, the lumps confirmed byte for byte, the counts of the recovered BSP and of the VMF (brushes, sides, nodraw, displacements, props, entities), the lumps copied from the map file as they are with their bytes and the refused ones, the content saved, what the client knew about entities (client_entities: networked entities read, matched to the entity lump, added, parent names restored, and the lower bound of what can never be recovered) and the timings. As text and as JSON. | none |
| `sniffer_get_settings` | Every BSP Sniffer setting with its value and the list of accepted keys. | none |
| `sniffer_set_settings` | Change BSP Sniffer settings (saved). Keys: address, authorized, output_dir, file_name_template ({map} {ip} {port} {date} {time} {datetime}), gmod_dir (empty: automatic), launch_options, insecure (auto, always, never), game_bits (auto, 64, 32), use_map_file (false: no map file is read), read_ldr_lighting, attach_running, process_timeout_secs, load_timeout_secs, settle_secs, poll_secs, save_models, save_materials, save_sounds, save_other, keep_bsp, only_non_stock, kill_game, auto_disconnect, open_when_done and decompile.brushes, decompile.displacements, decompile.entities, decompile.brush_entities, decompile.static_props, decompile.overlays, decompile.cubemaps, decompile.areaportals, decompile.occluders, decompile.group_details. The panel is the action bsp_sniffer (run_action). | `settings`*: object |

### Brushes and objects

| Tool | What it does | Arguments |
|---|---|---|
| `delete_objects` | Delete entities and solids by id. Deleting a brush entity deletes its solids. One undo step. | `ids`: list of integer; `solid_ids`: list of integer |
| `list_solids` | List solids (brushes) with id, owner entity, bounds and materials. Filter by material pattern, owner entity (0 for world brushes) or a box the bounds must touch. | `entity`: integer 0 to 4294967295; `limit`: integer 1 to 1000; `material`: text; `max`: 3 x number; `min`: 3 x number; `offset`: integer 0 to 4294967295 |
| `create_brush` | Create a brush (block, wedge, cylinder, spike, sphere, arch or torus) filling a box, with a material. With entity_class the brushes become a brush entity (func_detail, trigger_multiple...). The new objects become the selection. One undo step. | `entity_class`: text; `keyvalues`: object; `material`: text; `max`*: 3 x number; `min`*: 3 x number; `shape`: one of block, wedge, cylinder, spike, sphere, arch, torus; `sides`: integer 3 to 128 |
| `apply_material` | Apply a material to every face of objects, or to single faces (solid id and side index, as listed by list_solids). One undo step. | `faces`: list of object (side, solid); `ids`: list of integer; `material`*: text; `solid_ids`: list of integer |
| `move_objects` | Move objects by an offset (texture lock follows the editor setting). One undo step. | `ids`: list of integer; `offset`*: 3 x number; `solid_ids`: list of integer |
| `rotate_objects` | Rotate objects around an axis through a pivot (default: the center of their bounds). One undo step. | `axis`: one of x, y, z; `degrees`*: number -360 to 360; `ids`: list of integer; `pivot`: 3 x number; `solid_ids`: list of integer |

### Entities

| Tool | What it does | Arguments |
|---|---|---|
| `list_entities` | List the entities of the map (not the world) with id, class, targetname, origin and number of solids. Filters are case insensitive patterns with * and ?. | `classname`: text; `limit`: integer 1 to 1000; `offset`: integer 0 to 4294967295; `targetname`: text |
| `get_entity` | Every keyvalue, output, solid id and the bounds of one entity. | `id`*: integer 1 to 4294967295 |
| `create_entity` | Create a point entity (info_player_start, light, prop_static with keyvalues.model, ...) at a position. It becomes the selection. One undo step. | `angles`: 3 x number; `classname`*: text; `keyvalues`: object; `origin`*: 3 x number |
| `set_entity_keyvalues` | Set or remove keyvalues of one entity (the world uses its id from get_map_info: skyname, detailmaterial...). Setting "classname" changes the class of an entity. One undo step. | `id`*: integer 1 to 4294967295; `remove`: list of text; `set`: object |

### Selection

| Tool | What it does | Arguments |
|---|---|---|
| `select_objects` | Change the selection of the editor. Like a click, the change is one undo step named Selection when the setting edit.undo_restores_selection is on (the default). | `ids`: list of integer; `mode`: one of replace, add, remove; `solid_ids`: list of integer |
| `get_selection` | The selected objects (ids and kinds), selected faces and the selection bounds. | none |
| `clear_selection` | Deselect every object and every face. | none |

### History

| Tool | What it does | Arguments |
|---|---|---|
| `undo` | Undo the last edit (any edit, the user's included). With the setting edit.undo_restores_selection (on by default) a selection change is a step too: undo brings back the previous selection, faces included. | none |
| `redo` | Redo the last undone edit. | none |

### Clipboard

| Tool | What it does | Arguments |
|---|---|---|
| `get_vmf_text` | The VMF text of objects (default: the selection), like the editor clipboard. Edit it and give it back to paste_vmf_text. | `ids`: list of integer; `solid_ids`: list of integer |
| `paste_vmf_text` | Add the objects of a VMF text (world solids, entities, groups) to the map with new ids, moved by an optional offset. They become the selection. One undo step. | `offset`: 3 x number; `text`*: text |

### Assets

| Tool | What it does | Arguments |
|---|---|---|
| `search_materials` | Search the materials of the mounted game content by name (text or pattern with * and ?). | `limit`: integer 1 to 500; `query`*: text |
| `search_models` | Search the models (.mdl) of the mounted game content by path (text or pattern with * and ?). | `limit`: integer 1 to 500; `query`*: text |
| `search_sounds` | Search the sounds of the mounted game content by part of the path or a pattern with * and ?. | `limit`: integer 1 to 1000; `offset`: integer 0 to 4294967295; `query`*: text |
| `search_particles` | Search the particle systems of the game content (name, file). | `limit`: integer 1 to 1000; `offset`: integer 0 to 4294967295; `query`*: text |
| `list_particle_systems` | The info_particle_system entities of the map with their effect and how the particle preview of the 3D views does with it: playing, loaded (not playing: too far, too many or not selected with "selected only"), loading, not_found (no particle file of the content defines it), the particle file and the operators the preview does not simulate. Also the preview settings (lab.show_particles, lab.particles_selected_only: change them with set_setting or the toggle_particles action) and its caps. | `limit`: integer 1 to 1000; `offset`: integer 0 to 4294967295; `query`: text |
| `list_asset_sources` | The mounted content sources (game folders, VPKs, addons, workshop GMAs, custom folders) and the custom texture folders with their material prefixes. | none |
| `get_material_info` | A material's shader, texture size, base texture, surface property, tool kind, keywords and flags (translucent, alpha test, water, blend, self illumination...). | `material`*: text |
| `get_model_info` | A model's bounds, skins, body groups, sequences, materials, triangles, surface property and whether it can be a prop_static. Models load in the background: call again while loading is true. | `model`*: text |
| `get_material_preview` | A PNG image of a material's base texture (or of a texture), decoded at most size pixels on its longest side, to see what it looks like. | `material`: text; `size`: integer 16 to 1024; `texture`: text |
| `read_material` | The VMT text of a material and where it lives (file on disk, generated from an image, inside an archive). | `material`*: text |
| `edit_material` | The material editor: set or remove VMT parameters of materials (comments and formatting kept, a .vmt.bak backup first). Files must be in a custom texture folder; archived materials are copied into the custom folder copy_into first. | `copy_into`: integer 0 to 255; `materials`*: list of text; `remove`: list of text; `set`: object |
| `import_images` | Turn images (PNG, JPG, TGA, BMP...) from the map folder or a custom texture folder into game textures (VTF and VMT) of a custom texture folder. | `files`*: list of text; `folder`*: integer 0 to 255; `mipmaps`: true or false; `overwrite`: true or false; `subfolder`: text; `surface_prop`: text |
| `find_material_usage` | Where materials matching a pattern are used: the solids and faces, and the entities naming them in keys (overlays, decals, texture keys). | `limit`: integer 1 to 1000; `pattern`*: text |
| `set_current_material` | The material new brushes and Apply get (the texture browser's current material). | `material`*: text |
| `list_used_models` | The models of the map's entities with their counts and classes, most used first. | `limit`: integer 1 to 1000; `offset`: integer 0 to 4294967295 |
| `get_uv_layers` | The layers of a material the UV editor edits: base (the face mapping), base2 (blend), bump, bump2 (bump map of base2), detail, envmask and blendmod ($blendmodulatetexture), each with its texture, transform parameter and values (center, scale, rotate, translate, $detailscale), and what the shader does with the transform: honoured, follows_base (the shader ignores it: an edit is baked into a moved copy of the texture), not_drawn or unknown. Default material: the one of the first selected face, else the current material. | `material`: text |
| `edit_uv_layer` | Move, scale or turn the base2, bump, bump2, detail, envmask or blendmod layer of a material like the UV editor, and apply it to faces (apply_to faces: a variant of each face's material in the first custom folder, given to the faces, one undo step) or to the material itself (apply_to material: its VMT in a custom folder, a backup kept). Values not given keep the material's. When the shader ignores the layer transform (bump and detail of LightmappedGeneric, base2 and bump of WorldVertexTransition...), the move is baked into a new copy of the texture (quarter turns, flips, whole repeats, any shift). The base layer is the face mapping: set_face_texture, fit_texture, justify_texture. | `apply_to`: one of faces, material; `center`: 2 x number; `detail_scale`: 2 x number; `faces`: list of object (side, solid); `layer`*: one of base2, bump, bump2, detail, envmask, blendmod; `material`: text; `rotate`: number -3600.0 to 3600.0; `scale`: 2 x number; `translate`: 2 x number |
| `align_texture_edge` | Like the Edge mode of the UV editor: turn the texture of a face about the middle of its edge from vertex `edge` to the next (the vertex order of get_faces), the scale kept: axis x or y lays the edge on the X (U) or Y (V) axis of the texture (the smaller turn); angle gives the edge that direction on the texture (degrees from the X axis, clockwise with V down, as the UV editor shows it); rotate_by turns the texture by that many degrees about the middle of the edge. With fit_tiles (and an axis), scale it so that many tiles span the edge, a tile corner on its start. The other faces given use their edge most parallel to it (within 10 degrees). With a layer other than base (bump, detail...), the layer transform of the material turns instead, applied to the faces as a variant like edit_uv_layer. One undo step. Give one of axis, angle, rotate_by. | `angle`: number -360.0 to 360.0; `axis`: one of x, y; `edge`*: integer 0 to 255; `faces`: list of object (side, solid); `fit_tiles`: number 0.01 to 1024.0; `layer`: one of base, base2, bump, bump2, detail, envmask, blendmod; `rotate_by`: number -3600.0 to 3600.0 |
| `move_texture_vertices` | Like dragging vertices (or edges) in the Vertex and Edge modes of the UV editor: the given vertices of a face move by delta texels over its texture, the others stay. A Source brush face (and a displacement, through its base face) has one planar texture projection, not a UV per vertex, so this applies the affine map (move, turn, scale, skew) closest to it: the moved vertices land exactly, the kept vertex farthest from them (two with several moved) stays exactly when an affine map allows, the others move as little as possible; refused when it would flatten the texture. The other faces given move the vertices they share in the world with the moved ones. With a layer other than base, the layer under the vertices follows instead (turn and scale, no skew), applied to the faces as a variant. One undo step. The result lists the texels of every vertex of the first face before and after. | `delta`*: 2 x number; `edges`: list of integer 0 to 255; `faces`: list of object (side, solid); `layer`: one of base, base2, bump, bump2, detail, envmask, blendmod; `vertices`: list of integer 0 to 255 |
| `set_texture_origin` | Like the Vertex mode of the UV editor: shift the texture of a face so a corner of a texture tile lies on its vertex `vertex` (the vertex order of get_faces); the other faces given that share the point get the same. With a layer other than base, the layer transform moves instead (a variant of the faces). One undo step. | `faces`: list of object (side, solid); `layer`: one of base, base2, bump, bump2, detail, envmask, blendmod; `vertex`*: integer 0 to 255 |

### Map health

| Tool | What it does | Arguments |
|---|---|---|
| `check_problems` | Run the map checks (invalid solids, leaks prone entities, missing targets, unknown classes...) and list the problems. | `limit`: integer 1 to 1000 |
| `get_problems` | The problems of the map (like Check for problems, with the background checks: limits, leaks, entity budget when they ran): severity, code, message, objects, faces, fixable. Filters by severity, code pattern and object. | `code`: text; `limit`: integer 1 to 1000; `object`: integer 1 to 4294967295; `offset`: integer 0 to 4294967295; `severity`: one of info, warning, error |
| `fix_problems` | Fix the fixable problems (all, or those with these codes, or these indices of get_problems) in one step. | `all`: true or false; `codes`: list of text; `indices`: list of integer |
| `get_map_limits` | How much of each engine limit the map uses (brushes, sides, planes, entities, overlays, lightmaps...) with the biggest users, estimated by the background check or exact from the compiled BSP, and the entity (edict) budget. | `top`: integer 0 to 50 |
| `get_map_stats` | Counts of the map: solids, world solids, faces, entities (point and brush), displacements, materials, groups, visgroups, invalid solids, bounds, and the undo history size. | none |
| `find_hidden_faces` | Faces entirely covered by other brushes (they could get nodraw with apply_nodraw). Computed on a worker thread. | `limit`: integer 1 to 100000 |
| `recheck_map` | Run the background problem and limit checks again now (get_problems shows the result when checking is false). | none |

### Compile

| Tool | What it does | Arguments |
|---|---|---|
| `compile_map` | Start compiling the saved map with a preset (Hammer Pro compiles it itself: geometry and visibility, then lighting). Returns at once: poll get_compile_status. The game is only started when launch_game is true. | `launch_game`: true or false; `preset`: one of draft, normal, final |
| `get_compile_status` | Whether a compile runs, its step and progress (while the lighting runs, the step names its stage, the stage percent and the time left), and the result of the last compile. | none |

### Views

| Tool | What it does | Arguments |
|---|---|---|
| `set_camera` | Place the 3D camera (position, yaw and pitch in degrees, or a point to look at) and center or zoom the 2D views. | `center_2d`: 3 x number; `look_at`: 3 x number; `pitch`: number -89 to 89; `position`: 3 x number; `yaw`: number -360 to 360; `zoom_2d`: number 0.002 to 64 |
| `screenshot_view` | Render the 3D view or a 2D view (top, front, side) with the current cameras and return it as a PNG image. The 2D logical view (the I/O schematic a quadrant can show) is not a render: look at it with screenshot_window. | `height`: integer 64 to 1920; `view`: one of 3d, top, front, side; `width`: integer 64 to 1920 |
| `get_views` | The four views (kind: 3d, top, front, side or logical, the 2D logical view showing the entities and their outputs as a schematic; rectangle in window points, maximized, focused), the 3D camera, the 2D cameras (center, zoom), the 3D render mode, how the 2D views draw displacements (mesh or outline, per view) and whether walkability colours are on, the grid (mode and the grid of each view; list_grids has the details) and the saved layouts. | none |
| `get_logical_graph` | What the 2D logical view (and the logical view window) shows: one node per entity (id, class, name, the icon material from the FGD iconsprite or sprite helper, hidden when its visgroup is hidden, outputs, inputs, problems) and one edge per output and target (from, to or null when no entity has the name, output, target, input, parameter, delay, times, state: valid, missing_target, unknown_output or unknown_input). filter is the filter of the view: all (default, every entity, lights and info entities included, like Hammer), named (named or connected) or connected (with an outgoing or incoming output). | `filter`: one of all, named, connected; `limit`: integer 1 to 100000 |
| `set_view_layout` | Choose what each quadrant shows (quadrants: 4 of 3d, top, front, side, logical; top-left, top-right, bottom-left, bottom-right; logical is Hammer's "2D Logical" view: the entities and their outputs as a schematic, its selection shared with the other views; drive it with ui_click, ui_drag and ui_scroll with view "logical" and look at it with screenshot_window), the split positions, maximize one view (or none), how each 2D view draws displacements (displacements_2d: 4 of global, mesh, selected_mesh, outline; like the view menu) and whether 2D views draw the brushes displacements were built from as a faint outline (disp_source_brushes), or apply a saved layout by name ("default": the layout of a new installation). | `disp_source_brushes`: true or false; `displacements_2d`: 4 x one of global, mesh, selected_mesh, outline; `layout`: text; `maximize`: one of 0, 1, 2, 3, none; `quadrants`: 4 x one of 3d, top, front, side, logical; `split_x`: number 0.1 to 0.9; `split_y`: number 0.1 to 0.9 |
| `set_render_mode` | How the 3D view draws the map: textured (with the editor's shading), flat, wireframe, textured_wire, lit (lighting preview: the light entities with VRAD's falloff and spot cones, light_dynamic, the sun of the first light_environment (angles or pitch key, target, SunSpreadAngle soft shadows) lighting only what sees a sky face toward it like VRAD, each point shadowed only by what lies between it and the sky it reaches, skies stacked one above the other included (the 3D skybox, what lies beyond the sky and brush entities without vrad_brush_cast_shadows cast no shadow, editor helpers neither), its _ambient sky light only where the sky is seen (setting render.sky_occlusion), HDR or LDR light values (setting render.light_preview_hdr), every light of big maps kept (get_lighting_status realtime tells how many and how many crowded spots keep only their strongest lights), env_projectedtexture entities (setting              render.projected_textures: texture, lightfov, nearz, farz, lightcolor, lightstrength and target, the first 16;              get_lighting_status realtime projectors counts them), props included (prop_static shadows follow the compile like VRAD: the model triangles with -StaticPropPolys, the collision model otherwise, none with -nostaticpropshadows; setting render.prop_shadows: Auto, Mesh or Collision); the point and spot lights in view cast shadows too (the brightest first, so a light never shines through walls), how many set by the setting render.light_shadows (0 to 64, 16 by default, 0: the sun only; disableshadows props cast none); the sky light and the light bounced by the other surfaces come from a background bake of the map with the compiler while the setting render.lit_indirect is on (the default; get_lighting_status realtime.indirect tells its state), like the game's lightmaps, the lights and the sun staying real time; lights inside an opaque brush light nothing like in game; $selfillum glows by the base alpha, $selfillummask or the $envmapmask alpha of $selfillum_envmapmask_alpha, times $selfillumtint, also in baked and the player view), unshaded (textured without shading), smoothing_groups, baked (the baked lighting preview, see bake_lighting; props lit by the bake like the game: ambient cubes, world lights, static prop vertex lighting) or lightmap_grid (Hammer's 3D lightmap grid, no bake needed: every lightmapped face shows its luxel grid, one checker cell per lightmapscale units along its texture axes, displacements included, grey at scale 16, redder when finer, greener when coarser, yellow lines where VBSP splits a face wider than 32 luxels, shaded by the baked lighting once a bake exists; faces without a lightmap (nodraw, tool textures, sky, water, unlit) and props keep their texture). In the textured modes water surfaces are drawn like in game (refraction, water fog, reflection, moving waves) while the setting render.water_preview is on (set_setting, or run_action toggle_water_preview); off, they show their tool texture. Expensive water reflects the scene drawn again mirrored about its surface like the game; render.water_detail follows the Water Detail option of the game: ReflectAll (default, expensive at every distance, reflects the world, props, brush entities, the sky and the 3D skybox), ReflectWorld (expensive near the camera, props and brush entities only with $reflectentities) or Simple (cheap water). The map fog of an env_fog_controller (fogenable, fogcolor, fogcolor2 with fogblend, fogstart, fogend, fogmaxdensity, farz) covers the 3D views and the player view while render.fog_preview is on (the default; run_action toggle_fog_preview, preview_fog chooses the controller). Every fog_volume is outlined and filled with its fog, the box around its brushes like the game, as thick as the view crosses it, with a label naming its controller and its problems, while render.fog_volume_preview is on (run_action toggle_fog_volumes): the fog of the env_fog_controller its FogName names (fogcolor, fogcolor2 with fogblend along fogdir or the angles with use_angles, fogstart, fogend, fogmaxdensity), or a light blue default look when it names nothing or a controller with fogenable 0 (list_fog tells which). In the player view the controller fog replaces the map fog while the eyes are inside an enabled volume (none with fogenable 0). render.fog_volume_texture (run_action toggle_fog_volume_texture) draws or hides the brush material of the volumes in the 3D view. Every func_fish_pool shows fish_count copies of its model (models/Junkola.mdl by default; a model the game content lacks is previewed with models/props/de_inferno/goldfish.mdl) scattered within max_range (at most 255) of the entity, the same places each time, kept in the water around the entity 8 units under its surface, while render.fish_preview is on (run_action toggle_fish_preview); they swim slow circles while render.fish_swim is on (run_action toggle_fish_swim); selected, the pool shows a sphere and a circle of its max_range. Lightmapped_4WayBlend displacements show their 4 textures in every textured mode (textured, lit, baked, the player view) like the game: texture 2, 3 then 4 laid over texture 1 by the multiblend rows of the displacement (or its vertex alpha for texture 2 when it has none), shaped by $textureN_lumstart and lumend, $textureN_blendstart and blendend, $lumblendfactorN, $textureN_uvscale and $textureN_blendmode. The normal maps of the materials ($bumpmap, $bumpmap2 of WorldVertexTransition and of Lightmapped_4WayBlend, mixed by the same blend weights as the base textures, its $textureN_bumpblendfactor choosing the map of each texture; $ssbump) give the surfaces their relief in textured (the editor shading), lit (the preview lights), baked (the bumped lightmaps of the preview bake, like the game), the player view (the compiled bumped lightmaps like the game), on the props and in the model browser (preview and thumbnails) while the setting render.normal_maps is on (the default; set_setting); off, surfaces are flat and no normal map is loaded. Every prop is drawn at the level of detail the game picks for it, from the switch distances of              its model and the size it takes on the screen (setting render.model_lods, on by default; off,              the finest level is always drawn). The env_sun entity draws its sun sprite and its glow like the              game (direction, size, overlaysize, rendercolor, overlaycolor, material, overlaymaterial,              HDRColorScale, the fade over the 40 degrees around the middle of the view and the fade behind              what hides the sun) while the setting render.sun_glow is on (the default). The 3D views sample              their pixels on the game's pixel grid (setting render.game_pixel_grid, on by default: half a              pixel from the grid Vulkan and Direct3D 12 use, so a screenshot of the preview lines up with a              screenshot of the game).              The materials with $envmap reflect their cubemap (like the SDK 2013 shaders: along the bumped normal, times $envmapmask or the base or normal map alpha, $envmaptint, $envmapcontrast, $envmapsaturation, $fresnelreflection, $envmapfresnel with $phongfresnelranges on phong props, $envmaplightscale) in textured, lit and baked and in the player view while the setting render.cubemaps is on (the default; run_action toggle_cubemaps): the editor draws a cubemap at every env_cubemap (cubemapsize, the sky without any), each brush face reflects the nearest env_cubemap in front of it like the compiler (or the one whose sides key lists it), props the nearest one. Parallax corrected cubemaps (Mapbase): an env_cubemap whose parallaxobb key names a parallax_obb brush entity (in the player view, the keys of the materials the compiler patched) makes the LightmappedGeneric and WorldVertexTransition faces reflecting it look the cubemap up corrected to that box. The cubemaps are drawn like the first 3D view (its mode, the player view's compiled lighting), a few faces a frame, again a moment after the map changes while render.cubemaps_auto is on, or with run_action rebuild_cubemaps. In the player view the cubemaps built in game are used when the compiled map has them (see player_view); run_action write_cubemaps (from the player view) writes the drawn ones into the compiled map like buildcubemaps. Asking for the mode the view already has changes nothing (changed false). | `mode`*: one of textured, flat, wireframe, textured_wire, lit, unshaded, smoothing_groups, baked, lightmap_grid |
| `player_view` | Start or leave the player view (walk the map like in game, F6) and choose its wireframe, Garry's Mod's mat_wireframe (X in the view): 0 off; 1 the triangles of the world surfaces in pink seen through everything; 2 the same in green on the visible surfaces only; 3 the outlines of the world polygons in pink seen through everything. In 1 to 3 the props are drawn only as the triangles of the LOD the game picks (cyan, seen through), detail sprites as green lines, displacements are not drawn, the world keeps its textures, like in game. Without arguments it only reports. The answer gives the state and, with a compiled map, what the game draws through the area portals from the camera: the area of the eye, the areas drawn, the area portals open and looked through (closed doors hide what is behind them, an open one shows only what is seen through its opening). compiled_lighting turns on or off (L in the view, setting render.player_compiled_lighting) the lighting of the game: the compiled world drawn with the lightmaps of the compiled map (HDR when it has them, else LDR, fullbright when it was compiled without VRAD) and the props lit by their baked vertex lighting (static props) or the ambient cube and world lights at their lighting origin; the answer tells whether the compiled lightmaps are used and which range (lighting), and where the reflections come from (cubemaps: the samples of the map, how many reflect the cubemap built in game and how many the one the editor draws there because the map holds VBSP's placeholders only). Light styles play like the engine (ten pattern letters a second: the built-in styles 1 to 12 such as 1 flicker or 10 fluorescent flicker, the patterns of named lights, style 63 dark): the lightmaps with styles, the styled world lights on the models and the detail props follow them; light_styles reports the time, every style the lights use with its string and value now, and the named (switchable) lights with their state. light (a named light) with on (true or false; missing: toggle) switches it in the preview at the next frame of the player view, like the inputs TurnOn, TurnOff and Toggle (the keys 1 to 9 in the view do the same, the I/O simulation too); the map does not change. The I/O simulation also plays the env_projectedtexture inputs TurnOn, TurnOff, FOV, EnableShadows, SpotlightTexture and LightWorld on the projected light of the preview, and a named infodecal appears when an Activate input reaches it. Trains move like in game: func_tracktrain and func_tanktrain follow their path_track chain from their target (speed is the initial speed, startspeed the max speed, height above the track, wheels for turning, orientationtype, velocitytype, bank, the No Pitch, No User Control, Passable and Fixed Orientation spawnflags, path_track speed for trains without user control, disabled tracks, altpath branches, loops, dead ends), func_train goes from path_corner to path_corner waiting their wait; the player rides them, their parented entities move with them, their I/O inputs (StartForward, SetSpeed, Toggle, EnablePath...) and outputs (OnPass, OnStart) run in the I/O simulation. train (a train targetname, or * for every train) with start (true or false; missing: toggle) starts or stops trains in the running player view (T in the view does it for all, E on a train for one); trains reports each train (moving, speed, position, angles, the path node it passed and the next one). | `active`: true or false; `compiled_lighting`: true or false; `eye`: 3 x number; `feet`: 3 x number; `light`: text; `noclip`: true or false; `on`: true or false; `pitch`: number -89.0 to 89.0; `start`: true or false; `train`: text; `wireframe`: integer 0 to 3; `yaw`: number -360.0 to 360.0 |
| `zoom_2d` | Zoom the top, front or side view (or all): an absolute zoom in points per unit, or a factor (2 zooms in twice), optionally keeping a world point in place. | `anchor`: 3 x number; `factor`: number 0.01 to 100.0; `view`: one of top, front, side, all; `zoom`: number 0.002 to 64.0 |
| `pan_2d` | Move the top, front or side view (or all): center it on a world point, or move it by [right, up] world units of that view. | `by`: 2 x number; `center`: 3 x number; `view`: one of top, front, side, all |

### Actions

| Tool | What it does | Arguments |
|---|---|---|
| `list_actions` | Every command of Hammer Pro (menus, toolbars, shortcuts, command palette): id, English label, menu, shortcut, whether it is enabled now and checked (toggles), and whether run_action needs confirm. Filter by menu or text. | `category`: one of file, edit, map, view, tools, window, help; `enabled_only`: true or false; `limit`: integer 1 to 1000; `offset`: integer 0 to 100000; `query`: text |
| `run_action` | Run a command of Hammer Pro by id (from list_actions), like a click in its menu: tools, selection, groups, hiding, carve, views, render modes, panels, dialogs... One undo step. Actions that close the app, publish, start the game or a program, or show a file dialog need confirm: true, only when the user asked for exactly that. | `action`*: text; `confirm`: true or false |

### Settings

| Tool | What it does | Arguments |
|---|---|---|
| `get_settings` | Read Hammer Pro's settings: all of them, a table ("view", "edit", "ui", "render"...) or one setting by dotted path ("view.fov_deg", "ui.theme"). Lists the paths MCP cannot change. The "discord" table is Discord Rich Presence (enabled, show_map, show_elapsed, show_tool, application_id). The "update" table is the auto update (check_on_startup, show_dialog, skipped_version; see get_update_status). | `path`: text |
| `set_setting` | Change one setting by dotted path (from get_settings) to a value of the same type, e.g. path "view.fov_deg" value 90, "ui.theme" "Dark", "edit.snap_to_grid" false. Applied at once and saved. File paths, programs and game folders are read only. | `path`*: text; `value`*: any value |
| `list_keybindings` | The keyboard shortcut of every action (current, default of the preset, customized), the shortcut preset and the conflicts. | `customized_only`: true or false; `limit`: integer 1 to 1000; `query`: text |
| `set_keybinding` | Bind an action to a shortcut like "Ctrl+Shift+K", remove its shortcut (""), or give it back its default (reset: true). Answers with the actions that share the shortcut. | `action`*: text; `reset`: true or false; `shortcut`: text |

### Ui automation

| Tool | What it does | Arguments |
|---|---|---|
| `screenshot_window` | PNG of the whole Hammer Pro window as the user sees it (title bar with the menus, panels, dialogs, views), optionally a region in window points and downscaled so the longest side is at most max_size pixels. The Compile and run window is a separate window by default (setting compile.separate_window): it is not in the main window's image; window "compile" captures it, "uv_editor" and "portals" the UV editor and the portals window when they are separate windows (describe_ui lists them in separate_windows). | `max_size`: integer 64 to 4096; `region`: 4 x number; `settle_ms`: integer 0 to 2000; `window`: one of main, compile, uv_editor, portals |
| `describe_ui` | What is on screen: open maps, views and their rectangles, panels, open windows and dialogs with their visible texts, open menus, tooltips, the focused widget, the status bar, toasts, the end of the console, the dialogs open as separate windows of the system (separate_windows: the Compile and run window by default, the UV editor and the portals window when taken out) and the buttons of Hammer Pro's title bar (title_bar: minimize, maximize or restore, close, with their rectangles for ui_click; null with the system's frames, setting ui.native_frames). | `console_lines`: integer 0 to 200 |
| `ui_move` | Move the mouse pointer (hover: tooltips, highlights). Position: window x, y, or a view with fx, fy, x, y or a world point. | `fx`: number 0.0 to 1.0; `fy`: number 0.0 to 1.0; `mods`: list of text; `view`: one of 0, 1, 2, 3, 3d, top, front, side, logical; `world`: 3 x number; `x`: number -100000.0 to 100000.0; `y`: number -100000.0 to 100000.0 |
| `ui_click` | Click like the user: press and release a mouse button. Position: window x, y, or a view with fx, fy, x, y or a world point (e.g. view "top", world [0, 0, 0]). In the 3D view the Selection and Camera tools select the clicked object (mods ["ctrl"]: add or remove it); while flying in mouselook (ui_key "Z") a left click selects at the crosshair. | `button`: one of left, right, middle; `fx`: number 0.0 to 1.0; `fy`: number 0.0 to 1.0; `mods`: list of text; `view`: one of 0, 1, 2, 3, 3d, top, front, side, logical; `world`: 3 x number; `x`: number -100000.0 to 100000.0; `y`: number -100000.0 to 100000.0 |
| `ui_double_click` | Double click like the user. Position: window x, y, or a view with fx, fy, x, y or a world point. | `button`: one of left, right, middle; `fx`: number 0.0 to 1.0; `fy`: number 0.0 to 1.0; `mods`: list of text; `view`: one of 0, 1, 2, 3, 3d, top, front, side, logical; `world`: 3 x number; `x`: number -100000.0 to 100000.0; `y`: number -100000.0 to 100000.0 |
| `ui_drag` | Press a mouse button at from, move to to in steps, release (draw a block, move objects, pan with the right button, drag a slider). from and to take x, y, fx, fy or world; view applies to both. | `button`: one of left, right, middle; `from`*: object (fx, fy, world, x, y); `mods`: list of text; `steps`: integer 1 to 200; `to`*: object (fx, fy, world, x, y); `view`: one of 0, 1, 2, 3, 3d, top, front, side, logical |
| `ui_scroll` | Turn the mouse wheel at a position: positive dy zooms a view in (or scrolls a list up), with mods ctrl it changes the grid in a 2D view. | `dy`: number -50.0 to 50.0; `fx`: number 0.0 to 1.0; `fy`: number 0.0 to 1.0; `mods`: list of text; `view`: one of 0, 1, 2, 3, 3d, top, front, side, logical; `world`: 3 x number; `x`: number -100000.0 to 100000.0; `y`: number -100000.0 to 100000.0 |
| `ui_key` | Press and release a key or shortcut like "Ctrl+Shift+S", "Escape", "Enter", "F5", "Delete", "ArrowUp"; hold_frames keeps it down (camera flying with W A S D). | `hold_frames`: integer 0 to 600; `key`*: text |
| `ui_type` | Type text into the focused text field (click the field first; use ui_key for Enter, Tab and shortcuts). | `text`*: text |
| `wait_frames` | Wait until Hammer Pro drew this many frames (animations, loading, a dialog opening), then answer with the editor state. | `frames`: integer 1 to 600 |

### Windows

| Tool | What it does | Arguments |
|---|---|---|
| `list_instances` | List the Hammer Pro windows that have MCP switched on (process id, map, last focus) and the one the tools talk to. | none |
| `use_instance` | Send the next tool calls to the Hammer Pro window with this process id (from list_instances). | `pid`*: integer 1 to 4294967295 |

### Editing

| Tool | What it does | Arguments |
|---|---|---|
| `duplicate_objects` | Duplicate objects (groups they fully contain too) with new ids, moved by an offset (default: one grid step on X and Y). The copies are selected. | `ids`: list of integer; `offset`: 3 x number; `solid_ids`: list of integer |
| `array_duplicate` | Make several copies of objects, each one moved, rotated and scaled from the previous one (Hammer's Paste special): copy N gets N times the offset and the rotation. | `count`*: integer 1 to 256; `ids`: list of integer; `offset`: 3 x number; `pivot`: 3 x number; `rotate`: 3 x number; `scale`: number 0.01 to 100.0; `solid_ids`: list of integer |
| `transform_objects` | Move, rotate (degrees around X, then Y, then Z) and scale objects around a pivot (default: the center of their bounds), in one step. Entities turn with their angles. | `ids`: list of integer; `pivot`: 3 x number; `rotate`: 3 x number; `scale`: 3 x number; `solid_ids`: list of integer; `translate`: 3 x number |
| `resize_objects` | Scale objects so that their bounds become the box min..max (Hammer's scale handles). | `ids`: list of integer; `max`*: 3 x number; `min`*: 3 x number; `solid_ids`: list of integer |
| `flip_objects` | Mirror objects along an axis, around a center (default: the center of their bounds). | `axis`*: one of x, y, z; `center`: 3 x number; `ids`: list of integer; `solid_ids`: list of integer |
| `snap_objects_to_grid` | Move objects so the minimum corner of their bounds is on the grid (default: the editor's grid), together or each one on its own. | `grid`: number 0.125 to 4096.0; `ids`: list of integer; `individually`: true or false; `solid_ids`: list of integer |
| `align_objects` | Line up the minimum, maximum or center of objects on one axis with a reference: another object (reference_id), a value, or the bounds of all of them. | `axis`*: one of x, y, z; `ids`: list of integer; `individually`: true or false; `mode`*: one of min, max, center; `reference_id`: integer 1 to 4294967295; `solid_ids`: list of integer; `value`: number -131072.0 to 131072.0 |
| `drop_to_surface` | Move objects down (or along a direction) until they touch the surface below them, optionally turned to the surface normal. Objects with nothing below stay. | `align`: true or false; `direction`: 3 x number; `ids`: list of integer; `solid_ids`: list of integer |
| `clip_brushes` | Cut solids with a plane (the clip tool, Shift+X, in 2D or 3D) through three points (clockwise seen from the front side gives the outward normal like VMF planes), a point and a normal, or a normal and a distance. The front is the side the normal points to. Keep the front, the back or both pieces. Brush entity solids keep their entity, textures are kept, a cut that would leave a piece thinner than 0.25 unit leaves the solid alone, and the whole clip is one undo step. Answers the ids of the resulting solids. | `cap_material`: text; `distance`: number -262144.0 to 262144.0; `ids`: list of integer; `keep`: one of front, back, both; `normal`: 3 x number; `point`: 3 x number; `points`: 3 x 3 x number; `solid_ids`: list of integer |
| `carve` | Carve (subtract) the carving solids from every solid they touch (Hammer's Carve). The carvers stay. | `ids`: list of integer; `solid_ids`: list of integer |
| `extrude_face` | Outline a shape on a brush face, then dig it into the brush or pull it out of the face (the extrude / carve tool, Shift+I). points are world positions, projected on the face plane: any outline that does not cross itself, concave too (it is split into convex pieces). carve replaces the brush by the convex brushes left around the hole: a recess of depth units, or an opening with through. extrude adds the prism on the face as new brushes of the same owner, or grows the brush itself when the result stays convex (an outline covering the whole face). The kept faces keep their textures and alignment; the new faces get the clicked face's texture (new_faces face), the current material (new_faces current) or material. One undo step. Answers the ids of the brushes made or changed (the brush keeps its id when it is carved or grown). | `depth`: number 0.125 to 16384.0; `material`: text; `mode`: one of carve, extrude; `new_faces`: one of face, current; `points`*: list of 3 x number; `side`*: integer 0 to 4095; `solid`*: integer 1 to 4294967295; `through`: true or false |
| `get_brush_edges` | The edges of a solid as numbered [a, b] point pairs (the edge indices bevel_edges takes), with their length. | `solid`*: integer 1 to 4294967295 |
| `bevel_edges` | Round or chamfer brush edges (the bevel tool, Shift+J, or Ctrl+B on an edge in the 3D view). Each edge is an index from get_brush_edges or two points [[x, y, z], [x, y, z]] at its ends, on solid; several edges (up to 64, on one or several solids as {solid, edge} or {solid, a, b} objects) get the same radius. The outward bevel cuts the edge with segments planes and keeps one convex brush (segments 1: a flat chamfer). inverse makes it concave: it carves a cove into the edge (the brush is split into convex pieces), or, when the edge meets another brush at an inner corner (a doorway top, a wall on a floor), it fills that corner with a concave round made of new brushes in the same owner (two corners of a doorway make an arch). The radius is limited to what the faces around the edge allow and segments shorter than half a unit are merged; the answer gives the radius and segments used. The kept faces keep their textures and alignment; the rounded faces get the texture of a face next to the edge wrapped around the whole round (new_faces face), the current material (new_faces current) or material. One undo step, the result is selected. dry_run answers the radius, segments, largest radius and kinds without changing the map. | `dry_run`: true or false; `edge`: any value; `edges`: list of values; `inverse`: true or false; `material`: text; `new_faces`: one of face, current; `radius`: number 0.25 to 16384.0; `segments`: integer 1 to 32; `solid`: integer 1 to 4294967295 |
| `hollow` | Turn solids into rooms with walls of a thickness (negative: walls outside). The walls of each solid are grouped and selected. | `ids`: list of integer; `solid_ids`: list of integer; `thickness`: number -1024.0 to 1024.0 |
| `merge_brushes` | Merge solids into one. convex_hull false (default) merges only when their union is convex and keeps every texture; true takes their convex hull even when it adds volume. | `convex_hull`: true or false; `ids`: list of integer; `solid_ids`: list of integer |
| `get_brush_vertices` | The vertices of a solid and its faces as lists of vertex indices (for move_brush_vertices). | `solid`*: integer 1 to 4294967295 |
| `move_brush_vertices` | Move some vertices of a solid by an offset (vertex tool). The solid must stay convex; textures follow with uv_lock. | `offset`*: 3 x number; `solid`*: integer 1 to 4294967295; `uv_lock`: true or false; `vertices`*: list of integer |
| `set_brush_vertices` | Rebuild a solid as the convex hull of new vertex positions (textures from the closest original faces). | `points`*: list of 3 x number; `solid`*: integer 1 to 4294967295 |
| `create_brush_from_points` | Create a convex solid, the hull of points, with a material (world brush or brush entity class). | `entity_class`: text; `material`: text; `points`*: list of 3 x number |
| `create_primitive` | Create a block, wedge, cylinder, spike, sphere, arch or torus in a box with every option of the block tool (sides, arch wall width, arc, start angle, add height, torus ring width, tube sides, rotation, spiral rise, up axis like a primitive drawn in the front or side view, texture scale, brush entity class). With depth_from_selection, the box's extent along that axis comes from the selection like a box drawn in a 2D view (the selection bounds, else the last created object, else 64 units on top). The tool options of the block tool itself (its arch preview window...) are set with set_tool_options. | `arch_add_height`: number -65536.0 to 65536.0; `arch_arc`: number 1.0 to 360.0; `arch_start_angle`: number -360.0 to 360.0; `arch_wall_width`: number 1.0 to 65536.0; `depth_from_selection`: one of x, y, z; `entity_class`: text; `material`: text; `max`*: 3 x number; `min`*: 3 x number; `shape`: one of block, wedge, cylinder, spike, sphere, arch, torus; `sides`: integer 3 to 64; `tex_scale`: number 0.01 to 64.0; `torus_arc`: number 1.0 to 3600.0; `torus_ring_width`: number 1.0 to 65536.0; `torus_rise`: number -16384.0 to 16384.0; `torus_start_angle`: number -360.0 to 360.0; `torus_tube_sides`: integer 3 to 64; `up`: one of x, y, z; `upside_down`: true or false |
| `create_polygon_brush` | Create a brush from a convex outline drawn in a 2D view (Hammer++'s polygon tool): the outline points in the view's axes (top: x y, front: y z, side: x z), extruded between two depths along the view's depth axis. | `depth`*: 2 x number; `material`: text; `points`*: list of 2 x number; `view`*: one of top, front, side |
| `create_curve_brushes` | Build a curved wall, rail or road from box brushes like the curve tool (Shift+U): a smooth spline through the points (in the view's axes: top x y, front y z, side x z) cut in equal lengths into subdivisions brushes, each one a convex 6 sided block of a thickness across the curve and a height along the view's depth axis, with mitred joints (no gap, no overlap). The points snap to the editor's grid unless snap is false. Without base and height the depth comes from the selection like the block tool (else the last created object, else 0 to 64). The brushes are one undo step and are selected. Refused when a bend is too tight for the thickness or a segment would be under 1 unit. | `base`: number -16384.0 to 16384.0; `entity_class`: text; `height`: number 1.0 to 16384.0; `material`: text; `points`*: list of 2 x number; `snap`: true or false; `subdivisions`: integer 2 to 4096; `tex_scale`: number 0.01 to 64.0; `thickness`: number 1.0 to 1024.0; `view`*: one of top, front, side |
| `snap_brush_vertices` | Move every vertex of solids to the nearest grid point (default: the editor's grid), fixing off grid brushes (solids that would become invalid are left as they are). | `grid`: number 0.125 to 4096.0; `ids`: list of integer; `solid_ids`: list of integer |
| `create_room` | Create a closed room: six walls of a thickness around an inside box (the walls are grouped), with a material for the walls, the floor and the ceiling. | `ceiling_material`: text; `floor_material`: text; `material`: text; `max`*: 3 x number; `min`*: 3 x number; `thickness`: number 1.0 to 1024.0 |
| `create_stairs` | Create a flight of stairs filling a box: a number of steps rising toward a direction (each step a block from the bottom of the box), grouped. | `direction`*: one of +x, -x, +y, -y; `material`: text; `max`*: 3 x number; `min`*: 3 x number; `steps`*: integer 2 to 256 |
| `make_hint_brushes` | Turn the solids of faces into hint brushes: the faces get tools/toolshint, every other face tools/toolsskip. | `faces`*: list of object (side, solid) |
| `apply_nodraw` | Give faces the nodraw material (tools/toolsnodraw), for example the hidden faces found by find_hidden_faces. | `faces`*: list of object (side, solid) |
| `tie_to_entity` | Tie solids to a new brush entity (Ctrl+T), or each world solid to its own entity (each: true), or to an existing brush entity (entity). A new entity whose FGD class has an origin key (func_tracktrain, func_tanktrain, func_rotating...) gets it at the center of its solids unless keyvalues give one, like Hammer: the game moves and turns the entity around that point. | `each`: true or false; `entity`: integer 1 to 4294967295; `entity_class`: text; `ids`: list of integer; `keyvalues`: object; `solid_ids`: list of integer |
| `move_to_world` | Move the solids of brush entities back to the world; entities left without solids are deleted. | `ids`: list of integer; `solid_ids`: list of integer |
| `prop_to_brushes` | Rebuild a prop_static, prop_dynamic or prop_physics as brushes (the reverse of Convert brushes to a static prop): one brush per convex piece of its collision model (the exact brushes of a prop made with Hammer Pro) or of its render mesh, faces textured from the model and nodraw where the model draws nothing. The brushes go into a func_detail or the world and are selected; the prop is removed unless keep_prop. | `id`: integer 1 to 4294967295; `keep_prop`: true or false; `shape`: one of auto, collision, render; `target`: one of detail, world |
| `get_faces` | Every face of solids: material, normal, texture axes, scales, shifts, rotation, lightmap scale, smoothing groups, displacement power and vertices. | `ids`: list of integer; `solid_ids`: list of integer; `vertices`: true or false |
| `set_face_texture` | Change the texture of faces like the face edit sheet: material, world or face alignment, axes, rotation, scale, shift (absolute or added), lightmap scale, smoothing groups. | `align`: one of world, face; `faces`: list of object (side, solid); `ids`: list of integer; `lightmap_scale`: integer 1 to 1024; `material`: text; `rotate_by`: number -360.0 to 360.0; `rotation`: number -360.0 to 360.0; `scale_by`: 2 x number; `shift_by`: 2 x number; `smoothing_groups`: integer 0 to 4294967295; `solid_ids`: list of integer; `u_axis`: 3 x number; `u_scale`: number -1024.0 to 1024.0; `u_shift`: number -65536.0 to 65536.0; `v_axis`: 3 x number; `v_scale`: number -1024.0 to 1024.0; `v_shift`: number -65536.0 to 65536.0 |
| `justify_texture` | Justify the texture of faces: fit, left, right, top, bottom or center, each face or treated as one. | `faces`: list of object (side, solid); `ids`: list of integer; `mode`*: one of fit, left, right, top, bottom, center; `solid_ids`: list of integer; `treat_as_one`: true or false |
| `fit_texture` | Fit the texture of faces so it repeats a number of times on U and V. | `faces`: list of object (side, solid); `ids`: list of integer; `repeat_u`: number 0.01 to 1024.0; `repeat_v`: number 0.01 to 1024.0; `solid_ids`: list of integer; `treat_as_one`: true or false |
| `align_texture_to_view` | Project the texture of faces from a 2D view direction (top, front or side) or explicit right and up axes. | `faces`: list of object (side, solid); `ids`: list of integer; `right`: 3 x number; `solid_ids`: list of integer; `up`: 3 x number; `view`: one of top, front, side |
| `wrap_texture` | Wrap the texture of a source face onto other faces so it continues across edges (Hammer++ alt+right click). | `faces`*: list of object (side, solid); `source`*: list of object (side, solid) |
| `replace_materials` | Replace a material (pattern with * and ?) with another on every face of the map, of the selection, or of some objects. | `find`*: text; `ids`: list of integer; `replace`*: text; `scope`: one of all, selection, ids; `solid_ids`: list of integer |
| `list_used_materials` | Materials used by the faces of the map with their face counts, most used first. | `limit`: integer 1 to 1000; `offset`: integer 0 to 4294967295 |
| `random_rotate_objects` | Turn each point entity with an angles key (prop_static, prop_dynamic, prop_physics, lights...) by its own random yaw around Z, in one undo step, like Tools > Random Z rotation. Brushes, brush entities and entities without angles are skipped and counted. The options default to the selection.random_* settings. | `ids`: list of integer; `max_yaw`: number -360.0 to 720.0; `min_yaw`: number -360.0 to 360.0; `seed`: integer 0 to 4294967295; `snap`: number 0.0 to 180.0; `solid_ids`: list of integer; `tilt`: number 0.0 to 45.0 |

### Groups and visibility

| Tool | What it does | Arguments |
|---|---|---|
| `select_by_class` | Select entities whose class matches a pattern (* and ?, case insensitive). | `op`: one of replace, add, remove, toggle; `pattern`*: text |
| `select_by_targetname` | Select entities whose targetname matches a pattern (* and ?). | `op`: one of replace, add, remove, toggle; `pattern`*: text |
| `select_by_material` | Select the solids (or with faces: true, the faces) using a material pattern (* and ?). | `faces`: true or false; `op`: one of replace, add, remove, toggle; `pattern`*: text |
| `select_in_box` | Select the visible objects touching (or inside, or with their center inside) a box. | `max`*: 3 x number; `min`*: 3 x number; `mode`: one of touching, inside, center; `op`: one of replace, add, remove, toggle |
| `select_faces` | Select faces (texture application mode): [{solid, side}]. | `faces`*: list of object (side, solid); `op`: one of replace, add, remove, toggle |
| `select_all` | Select every visible object (or invert the selection with invert: true). | `invert`: true or false |
| `select_group` | Select the members of a group (group) or of a visgroup (visgroup). | `group`: integer 1 to 4294967295; `op`: one of replace, add, remove, toggle; `visgroup`: integer 1 to 4294967295 |
| `frame_objects` | Point the 3D camera at objects (default: the selection) so they fill the view, and center the 2D views on them. | `ids`: list of integer; `solid_ids`: list of integer; `views`: one of all, 3d, 2d |
| `list_groups` | The groups of the map: id, parent group, members and bounds. | `limit`: integer 1 to 1000; `offset`: integer 0 to 4294967295 |
| `group_objects` | Group objects (Ctrl+G); groups fully inside are nested. Returns the group id. | `ids`: list of integer; `solid_ids`: list of integer |
| `ungroup_objects` | Dissolve the groups of objects (Ctrl+U). | `groups`: list of integer; `ids`: list of integer; `solid_ids`: list of integer |
| `list_visgroups` | The visgroup tree: id, name, color, parent, visible (true, false or mixed) and member counts. | none |
| `create_visgroup` | Create a visgroup (under a parent), optionally with members and a color. | `color`: 3 x integer 0 to 255; `ids`: list of integer; `name`*: text; `parent`: integer 1 to 4294967295; `solid_ids`: list of integer |
| `edit_visgroup` | Rename a visgroup, change its color, move it under another parent (0: top level) or up and down. | `color`: 3 x integer 0 to 255; `id`*: integer 1 to 4294967295; `move`: integer -1000 to 1000; `name`: text; `parent`: integer 0 to 4294967295 |
| `delete_visgroup` | Delete a visgroup (its objects stay, they only leave it). | `id`*: integer 1 to 4294967295 |
| `set_visgroup_members` | Add objects to a visgroup, remove them from it, or set exactly the visgroups of objects. | `action`*: one of add, remove, set; `ids`: list of integer; `solid_ids`: list of integer; `visgroup`: integer 1 to 4294967295; `visgroups`: list of integer |
| `move_to_visgroup` | Put objects in a visgroup (existing, or new with new_name), optionally leaving their other visgroups and hiding the visgroup. | `hide`: true or false; `ids`: list of integer; `new_name`: text; `remove_others`: true or false; `solid_ids`: list of integer; `visgroup`: integer 1 to 4294967295 |
| `set_visgroup_visible` | Show or hide a visgroup (with its children), or every visgroup at once (all: true, with visible: false to hide them all). | `all`: true or false; `id`: integer 1 to 4294967295; `visible`: true or false |
| `hidden_to_visgroup` | Move every hidden object into a new visgroup (Hammer++). | none |
| `hide_objects` | Hide objects (hide: true, default) or show them again; unselected: true hides everything not selected; unhide_all: true shows every hidden object. | `hide`: true or false; `ids`: list of integer; `solid_ids`: list of integer; `unhide_all`: true or false; `unselected`: true or false |
| `isolate_objects` | Hide everything but some objects (default: the selection), to look at them alone; hide_objects with unhide_all shows everything again. | `ids`: list of integer; `solid_ids`: list of integer |
| `get_visibility` | Whether objects are visible, hidden, in a hidden visgroup or cordoned out, and the counts of hidden objects. | `ids`: list of integer; `solid_ids`: list of integer |
| `list_cordons` | The cordons (named boxes that limit what is shown and compiled), whether cordoning is on. | none |
| `add_cordon` | Add a cordon box (active by default) and switch cordoning on (enable false keeps it as it is). | `active`: true or false; `enable`: true or false; `max`*: 3 x number; `min`*: 3 x number; `name`: text |
| `edit_cordon` | Change a cordon by index (list_cordons): name, box, active; or delete it (delete: true). | `active`: true or false; `delete`: true or false; `index`*: integer 0 to 4095; `max`: 3 x number; `min`: 3 x number; `name`: text |
| `set_cordons_enabled` | Switch cordoning on or off for the whole map (views and compiles). | `enabled`*: true or false |
| `get_display_color` | The display color of objects (default: the selection) in the 2D views: color [r, g, b] when one was chosen, null for the default look (white world brushes, entities in their game data color), the common state (none, default, custom or mixed) and the render.object_colors_2d and render.object_colors_3d settings. | `ids`: list of integer; `solid_ids`: list of integer |
| `set_display_color` | Give objects (default: the selection; a selected group colors every member) a display color for the 2D views: color [r, g, b], random: true (each brush and entity its own color) or reset: true (default look). Saved in the map as the editor color, which Hammer and Hammer++ show too. One undo step. Toggle the colors with set_setting render.object_colors_2d and render.object_colors_3d; edit.new_brush_color ([r, g, b] or null) colors new brushes. | `color`: 3 x integer 0 to 255; `ids`: list of integer; `random`: true or false; `reset`: true or false; `solid_ids`: list of integer |

### Grids

| Tool | What it does | Arguments |
|---|---|---|
| `list_grids` | The editing grids: the grid mode (global: one size for every view like Hammer; per_view: each view has its own grid; custom: a named custom grid; selection: the grid attached to the selected objects), the active grid of each view (steps per axis, origin, angles, where it comes from), the grids of the views, the custom grids of the map (with how many objects carry each) and the presets of the settings, the grid of the selection, whether objects use their grid when edited or drawn on. | none |
| `set_grid_mode` | Choose the grid mode and change the global and per view grids (settings, no undo step): mode (global, per_view, custom, selection); custom: the name of a custom grid to make active (mode custom); size: the grid size, any number from 0.01 to 4096 (12, 1.5...) of the global grid, or with view of that view's grid; origin and major_every of a view's grid (view needed); object_grids: editing an object that carries a grid or drawing on it snaps to its grid; plane_3d: draw custom grids in the 3D view. | `custom`: text; `major_every`: integer 0 to 64; `mode`: one of global, per_view, custom, selection; `object_grids`: true or false; `origin`: 3 x number; `plane_3d`: true or false; `size`: number 0.01 to 4096.0; `view`: one of 3d, top, front, side |
| `create_grid` | Create a custom grid: name, step (one number, or [x, y, z] for a different step per axis, decimals allowed), origin, angles [pitch, yaw, roll] (to follow a rotated building or a slope), major_every, limits (bounds {min, max}: outside, the global grid applies), color [r, g, b]. With from "selection" or "face" the grid is fitted to the selected object (its axes, a node on its corner) or face (its plane), the other fields changing that. scope map (default, saved with the map, one undo step) or preset (saved in the settings, offered in every map). activate (default true) makes it the active grid. | `activate`: true or false; `angles`: 3 x number; `bounds`: any value; `color`: any value; `from`: one of selection, face; `major_every`: integer 0 to 64; `name`*: text; `origin`: 3 x number; `scope`: one of map, preset; `step`: any value |
| `edit_grid` | Change a custom grid (list_grids): new_name (objects carrying it follow), step, origin, angles, major_every, bounds (null: no limits), color (null: the theme colour). scope map (default) or preset. | `angles`: 3 x number; `bounds`: any value; `color`: any value; `major_every`: integer 0 to 64; `name`*: text; `new_name`: text; `origin`: 3 x number; `scope`: one of map, preset; `step`: any value |
| `delete_grid` | Delete a custom grid of the map (the objects carrying it lose it, in the same undo step) or a preset. | `name`*: text; `scope`: one of map, preset |
| `attach_grid` | Attach a custom grid to objects (default: the selection) or groups, so editing them or drawing on them snaps to it (a preset is copied into the map first); name null or absent detaches their grid. Saved in the editor block of the objects, which Hammer ignores. | `group_ids`: list of integer; `ids`: list of integer; `name`: text; `solid_ids`: list of integer |

### Displacements

| Tool | What it does | Arguments |
|---|---|---|
| `list_displacements` | Every displacement face of the map (or of some solids): face, power, elevation, flags and the bounds of its surface. | `limit`: integer 1 to 1000; `offset`: integer 0 to 4294967295; `solid_ids`: list of integer |
| `select_displacements` | Select displacement faces (every one, or those of some solids) for the displacement tools and Apply; op adjacent adds the displacements sharing an edge with the selected ones (the panel's Select adjacent). | `op`: one of replace, add, remove, adjacent; `solid_ids`: list of integer |
| `get_displacement` | The data of one displacement face: power, row length, start position, elevation, flags, and per vertex world positions, distances and alphas (row by row). multiblend holds the 4-way blend rows written by Hammer++ and CS:GO Hammer and painted with the blend channels (null when the face has none): weights (the amount of textures 1 to 4 per vertex; the engine lays texture 2, 3 then 4 over texture 1 by the second, third and fourth value), alphablend and colors (4 lists of RGB, read by the Multiblend shader only). A Lightmapped_4WayBlend displacement without multiblend shows its vertex alpha as texture 2, in the editor and in the compiled map. | `face`*: list of object (side, solid); `positions`: true or false |
| `create_displacements` | Turn quad faces into flat displacements of power 2, 3 or 4 (default 3). | `faces`: list of object (side, solid); `ids`: list of integer; `power`: integer 2 to 4; `solid_ids`: list of integer |
| `destroy_displacements` | Turn displacement faces back into plain faces. | `faces`: list of object (side, solid); `ids`: list of integer; `solid_ids`: list of integer |
| `paint_displacement` | Paint strokes on displacements like the displacement tool: raise, lower, smooth, set (height above the face) or alpha, with a radius, an amount and a falloff. Each stroke sample is a center; all samples of a call are one undo step. | `amount`*: number -16384.0 to 16384.0; `centers`*: list of 3 x number; `faces`: list of object (side, solid); `falloff`: one of constant, linear, smooth; `mode`*: one of raise, lower, smooth, set, alpha; `radius`*: number 1.0 to 16384.0; `spatial`: true or false |
| `stroke_displacement` | Paint a stroke through world points with the displacement tool's own brush, exactly like a drag in the 3D view: its mode (geometry, alpha, sculpt), effect, paint axis (normal, subdiv, z, custom, view: toward the 3D camera as it is now, or view_flat: toward it on X and Y only, never up or down), radius, intensity (strength), falloff, spatial, line of sight, live sewing (autosew), blend channel and sculpt settings, read and changed with get_tool_options and set_tool_options (tool displacement). Like the tool, only the selected displacements change when some are selected (faces limits it instead), their unselected neighbours are pinned. Each point is one brush sample; rate based effects (raise, lower, smooth, alpha) apply seconds_per_sample of their per second strength. One undo step. | `faces`: list of object (side, solid); `invert`: true or false; `points`*: list of 3 x number; `seconds_per_sample`: number 0.001 to 10.0; `smooth`: true or false |
| `displacement_noise` | Add random height noise between min and max units to displacements. | `faces`: list of object (side, solid); `ids`: list of integer; `max`*: number -4096.0 to 4096.0; `min`*: number -4096.0 to 4096.0; `seed`: integer 0 to 4294967295; `solid_ids`: list of integer |
| `scale_displacements` | Multiply the heights of displacements by a factor. | `faces`: list of object (side, solid); `factor`*: number -100.0 to 100.0; `ids`: list of integer; `solid_ids`: list of integer |
| `invert_displacement_alpha` | Invert the blend alpha of displacements (255 minus alpha). | `faces`: list of object (side, solid); `ids`: list of integer; `solid_ids`: list of integer |
| `set_displacement_flags` | Switch the collision flags of displacements (no physics, no hull (player) and no ray collisions, written to the map like Hammer and honoured by the compiler) and whether they cast shadows when the map is lit (cast_shadows, Hammer Pro's compiler; they are still lit). | `cast_shadows`: true or false; `faces`: list of object (side, solid); `ids`: list of integer; `no_hull`: true or false; `no_physics`: true or false; `no_ray`: true or false; `solid_ids`: list of integer |
| `tag_displacements` | Paint the triangle tags of displacements (Hammer's Tag Walkable, Tag Buildable and Tag Remove): force walkable or buildable on or off, give it back to the slope (auto), or mark triangles removed (on) or not (off). Every triangle of the faces, or those whose center is within radius of center. One undo step. | `center`: 3 x number; `faces`: list of object (side, solid); `ids`: list of integer; `radius`: number 0.1 to 16384.0; `solid_ids`: list of integer; `spatial`: true or false; `tag`*: one of walkable, buildable, remove; `value`*: one of on, off, auto |
| `set_displacement_data` | Set per vertex data of one displacement: distances along the normals (heights) and alphas, row by row ((2^power + 1) squared values each), the elevation, and the 4-way blend (multiblend) rows of Lightmapped_4WayBlend materials: multiblend_weights (4 numbers per vertex, the amounts of textures 1 to 4), alphablend (4 per vertex) and multiblend_colors (4 lists of RGB per vertex). Missing multiblend lists keep their values (a face without multiblend starts from its alphas: texture 2 where painted). remove_multiblend drops the rows. Written to the VMF like Hammer++ and compiled to the multiblend lump. | `alphablend`: list of 4 x number 0 to 1; `alphas`: list of number 0 to 255; `distances`: list of number; `elevation`: number -16384.0 to 16384.0; `face`*: list of object (side, solid); `multiblend_colors`: 4 x list of 3 x number 0 to 4; `multiblend_weights`: list of 4 x number 0 to 1; `remove_multiblend`: true or false |
| `subdivide_displacements` | Smooth the surface formed by displacements like a subdivision surface, then sew it (Hammer's Subdivide). | `faces`: list of object (side, solid); `ids`: list of integer; `solid_ids`: list of integer |
| `sew_displacements` | Make the shared edges and corners of neighbouring displacements identical (Hammer's Sew). | `faces`: list of object (side, solid); `ids`: list of integer; `solid_ids`: list of integer |

### Instances and prefabs

| Tool | What it does | Arguments |
|---|---|---|
| `list_func_instances` | The func_instance entities of the map: file, fixup name and style, origin, angles, replace parameters, and whether the file is found. | `limit`: integer 1 to 1000; `offset`: integer 0 to 4294967295 |
| `create_instance` | Place a func_instance of a VMF file (relative to the map's folder, like "instances/door.vmf") with a fixup name, style and replace parameters ($name value). | `angles`: 3 x number; `file`*: text; `fixup_style`: one of prefix, postfix, none; `origin`*: 3 x number; `replaces`: object; `targetname`: text |
| `set_instance_parameters` | Change the file, fixup name, fixup style or the replace parameters (the whole list) of a func_instance. | `file`: text; `fixup_style`: one of prefix, postfix, none; `id`*: integer 1 to 4294967295; `replaces`: object; `targetname`: text |
| `collapse_instances` | Replace func_instance entities by the contents of their files (fixed up like VBSP), or every instance (all: true). The files load in the background: when they are not loaded yet, call again. | `all`: true or false; `ids`: list of integer |
| `make_instance` | Write objects to a new instance file (relative to the map folder) and replace them with a func_instance of it at an origin (default: the center of their bounds, on the grid). | `file`*: text; `ids`: list of integer; `origin`: 3 x number; `overwrite`: true or false; `solid_ids`: list of integer |
| `get_instance_file` | What a VMF instance file (relative to the map folder) holds: its parameters (func_instance_parms: name, type, default), entity classes with counts, solids, bounds. | `file`*: text |
| `list_prefabs` | The prefab files of the prefab library (names relative to the library folder), with a name filter. | `filter`: text; `limit`: integer 1 to 1000; `offset`: integer 0 to 4294967295 |
| `insert_prefab` | Insert a prefab of the library: at its saved place moved by an offset, at a position (its bounds centered there), or fitted in a box. The &i token gets the next number unless given. | `group`: true or false; `max`: 3 x number; `min`: 3 x number; `name`*: text; `offset`: 3 x number; `position`: 3 x number; `token`: integer 0 to 1000000 |
| `create_prefab` | Save objects as a prefab file of the library (written in the background). The objects are selected. | `ids`: list of integer; `name`*: text; `overwrite`: true or false; `solid_ids`: list of integer |
| `sprinkle` | Scatter point entities (props, lights...) on the surfaces below an area, like the Sprinkle tool: a list of weighted items, a count, a minimum spacing, random yaw, optional alignment to the surface. | `align`: true or false; `center`: 3 x number; `count`*: integer 1 to 2000; `items`*: list of object (classname, keyvalues, weight); `max`: 3 x number; `max_slope`: number 0.0 to 90.0; `min`: 3 x number; `radius`: number 1.0 to 65536.0; `random_yaw`: true or false; `seed`: integer 0 to 4294967295; `spacing`: number 0.0 to 4096.0 |
| `trace_ray` | The first solid (and face) or point entity hit by a ray: object, face, point, normal, distance. | `direction`*: 3 x number; `entities`: true or false; `hidden`: true or false; `max_distance`: number 1.0 to 1000000.0; `origin`*: 3 x number |

### Entities and io

| Tool | What it does | Arguments |
|---|---|---|
| `list_entity_classes` | The entity classes of the game's FGD (point or brush), with a name pattern and their descriptions. | `kind`: one of point, brush; `limit`: integer 1 to 1000; `offset`: integer 0 to 4294967295; `pattern`: text |
| `get_entity_class` | Everything the FGD says about a class: description, bases, helpers (model, sprite, size...), every property with type, default, description, choices and flags, inputs and outputs. | `classname`*: text |
| `check_entity` | Check the keyvalues and outputs of an entity against the FGD: unknown class, unknown keys, values that are not one of the choices, outputs the class does not have, inputs the target classes do not have, targets that do not exist. | `id`*: integer 1 to 4294967295 |
| `set_keyvalue` | Set (or with value null, remove) one key on several entities at once, or rename a key (rename_to). | `ids`: list of integer; `key`*: text; `rename_to`: text; `value`: text |
| `set_entity_class` | Change the class of entities (keyvalues stay). A brush entity whose new FGD class has an origin key (func_tracktrain, func_tanktrain, func_rotating...) and has none gets it at the center of its solids, like Hammer. | `classname`*: text; `ids`: list of integer |
| `set_spawnflags` | Switch spawnflags of entities on or off, by bit value or by their FGD name. | `clear`: list of text; `ids`: list of integer; `set`: list of text |
| `list_connections` | The outputs of an entity (index, output, target, input, parameter, delay, times to fire) and the outputs of other entities that target it. | `id`*: integer 1 to 4294967295 |
| `add_connection` | Add an output (connection) to entities: output, target name, input, parameter, delay, times to fire (-1: always). | `delay`: number 0.0 to 100000.0; `ids`: list of integer; `input`*: text; `output`*: text; `parameter`: text; `target`*: text; `times`: integer -1 to 1000000 |
| `edit_connection` | Change fields of one output of an entity (index from list_connections). | `delay`: number 0.0 to 100000.0; `id`*: integer 1 to 4294967295; `index`*: integer 0 to 100000; `input`: text; `output`: text; `parameter`: text; `target`: text; `times`: integer -1 to 1000000 |
| `remove_connections` | Remove one output by index, or every output of entities matching output, target and input patterns. | `ids`: list of integer; `index`: integer 0 to 100000; `input`: text; `output`: text; `target`: text |
| `get_io_graph` | Every connection of the map (from entity, output, target, input, parameter, delay) with the entities the target name resolves to, and the broken ones (no entity has the target name). | `broken_only`: true or false; `limit`: integer 1 to 1000; `offset`: integer 0 to 4294967295 |
| `rename_targetname` | Rename a targetname on every entity that has it, and (default) in every output, parameter and key that refers to it. | `new`*: text; `old`*: text; `update_references`: true or false |
| `find_entities` | Find entities by class pattern, targetname pattern, and a key with a value pattern. | `classname`: text; `include_hidden`: true or false; `key`: text; `limit`: integer 1 to 1000; `offset`: integer 0 to 4294967295; `targetname`: text; `value`: text |
| `find_text` | Find text in keyvalues, key names and outputs of entities (Find and Replace). | `case_sensitive`: true or false; `find`*: text; `ids`: list of integer; `key_names`: true or false; `limit`: integer 1 to 1000; `offset`: integer 0 to 4294967295; `visible_only`: true or false; `whole_word`: true or false |
| `replace_text` | Replace text in keyvalues, key names and outputs of entities (every match of find_text). | `case_sensitive`: true or false; `find`*: text; `ids`: list of integer; `key_names`: true or false; `replace`*: text; `visible_only`: true or false; `whole_word`: true or false |
| `center_entity_origins` | Move the origin of brush entities (func_door_rotating...) to the center of their solids. | `ids`: list of integer |
| `entity_report` | How many entities of each class the map has (and brush entity solids). | none |
| `create_entity_fgd` | Create a point entity like the entity tool: the FGD defaults of its class (keyvalues and spawnflags), then the keyvalues given, at an origin with angles. | `angles`: 3 x number; `classname`*: text; `keyvalues`: object; `origin`*: 3 x number |
| `copy_keyvalues` | Copy keyvalues of one entity onto others (Copy and Paste keyvalues): every key but id, classname, origin and targetname, or only the keys listed. | `from`*: integer 1 to 4294967295; `ids`: list of integer; `keys`: list of text |
| `list_targetnames` | Every targetname of the map with its entities and how many outputs aim at it. | `limit`: integer 1 to 1000; `offset`: integer 0 to 4294967295; `pattern`: text |
| `query_box` | The solids and entities touching (or inside) a box, without changing the selection. | `inside`: true or false; `limit`: integer 1 to 1000; `max`*: 3 x number; `min`*: 3 x number |
| `aim_entities` | Turn entities (spot lights, cameras, props, info_player_start...) to face a point or another entity: their angles (and the pitch key of light_spot and light_environment, which VRAD reads with up positive). | `ids`: list of integer; `point`: 3 x number; `target`: integer 1 to 4294967295; `yaw_only`: true or false |
| `set_max_distance` | One maximum distance for entities, written to the key each class honours in Garry's Mod: light and light_spot _distance (the light stops there: models in game, lightmaps with Hammer Pro's compiler), props, info_overlay and env_detail_controller fademaxdist (with a fademindist start so they fade out), env_lightglow OuterMaxDist, func_lod DisappearMaxDist, light_dynamic distance and point_spotlight SpotlightLength (only shortened). 0 removes the limit. Classes the game cannot limit (logic entities, triggers) are skipped and listed. get_entity_class gives the key of a class (max_distance). | `distance`*: number 0.0 to 1000000.0; `ids`: list of integer |
| `set_current_entity_class` | The class the entity tool places next (and its default model for prop classes). | `classname`*: text; `model`: text |

### Compile and play

| Tool | What it does | Arguments |
|---|---|---|
| `get_compile_options` | The compile settings: preset, steps, geometry, visibility and lighting modes, dynamic ranges (LDR, HDR), the static prop options (static_prop_lighting, static_prop_shadows), the options of the geometry and lighting steps (bsp_options, rad_options: the flags forced on or off in the options menu of the compile settings, the flags of the extra arguments, the flags the preset adds and the resulting command_flags), cordon compile, smart pipeline, packing, cubemaps, threads, copy to the maps folder and its folder, starting the game, expert mode and its profiles, and the compiler chosen for the game (compiler: Hammer Pro's own by default, see list_compilers). set_compile_options changes them. | none |
| `compile_with_options` | Compile the saved map with options for this compile only (the settings are put back): preset, steps, modes (entities only, fast vis, final rad...), lighting, cordon only, smart pipeline, hidden objects, threads, an expert profile, or another compiler (compiler: an id or name of list_compilers, builtin for Hammer Pro's own). Never starts the game (compile_map does with launch_game). Poll get_compile_status and get_compile_log. | `bsp_mode`: one of normal, onlyents, onlyprops; `bsp_options`: object; `compile_hidden`: true or false; `compiler`: text; `copy_to_maps`: true or false; `cordon_only`: true or false; `lighting`: one of ldr, hdr, both; `preset`: one of draft, normal, final, custom; `profile`: text; `rad_mode`: one of draft, normal, final; `rad_options`: object; `run_bsp`: true or false; `run_rad`: true or false; `run_vis`: true or false; `smart`: true or false; `static_prop_lighting`: true or false; `static_prop_shadows`: true or false; `threads`: integer 0 to 256; `vis_mode`: one of fast, normal |
| `set_compile_options` | Change the saved compile settings (the compile window and the toolbar use them): preset, steps, modes, lighting, threads, priority, leaks, cordon, smart pipeline, packing, development addon, cubemaps, progressive play, history, the compile window as a separate window (separate_window), expert profile, copy to the maps folder, its folder, starting the game after a compile, the static prop options (static_prop_lighting, static_prop_shadows) and the options menu of the geometry step (bsp_options: nodetail, nowater, fulldetail, forceskyvis, luxelscale, radius_override, AllowDynamicPropsAsStatic, wsaddons, serveraddons, propdebug, blocksize (0: no blocks)...) and of the lighting step (rad_options: final, fast, bounce, StaticPropLighting, StaticPropPolys, textureshadows, lights, chop, smooth, softsun, extrasky...): each flag forced on (true or a value), forced off (false) or following the preset (null). Starting the game after compiles and the maps folder need confirm: true (only when the user asked). Answers the settings like get_compile_options. | `background_precompile`: true or false; `bsp_mode`: one of normal, onlyents, onlyprops; `bsp_options`: object; `compile_hidden`: true or false; `confirm`: true or false; `copy_to_maps`: true or false; `cordon_only`: true or false; `dev_addon`: true or false; `expert`: true or false; `history`: true or false; `keep_cubemaps`: true or false; `launch_game`: true or false; `launch_via_steam`: true or false; `lighting`: one of ldr, hdr, both; `load_pointfile_on_leak`: true or false; `low_priority`: true or false; `maps_dir_override`: text; `pack_custom_content`: true or false; `pack_policy`: one of release_only, always; `preset`: one of draft, normal, final, custom; `profile`: text; `progressive`: true or false; `progressive_preset`: one of normal, final; `rad_mode`: one of draft, normal, final; `rad_options`: object; `run_bsp`: true or false; `run_rad`: true or false; `run_vis`: true or false; `save_before_compile`: true or false; `separate_window`: true or false; `smart`: true or false; `static_prop_lighting`: true or false; `static_prop_shadows`: true or false; `stop_on_leak`: true or false; `threads`: integer 0 to 256; `vis_mode`: one of fast, normal |
| `cancel_compile` | Stop the running compile. | none |
| `get_compile_log` | The steps and log of the last compile (running or finished): last lines, only warnings and errors, or lines containing a text; counts, leak pointfile and result. Lines reporting a known problem (leak, microbrush, too many planes, missing material... the Valve messages) carry its code, explanation, fix and the brush, side, entity or position they name; problems sums them up. While Hammer Pro's lighting runs, its step carries light: the stage (scene, direct lighting, bounce rays, bounced light, leaf ambient, detail props, static props, writing), stage_percent, stage_index of stage_count, overall_percent, elapsed_s and left_s (estimated seconds left, null while estimating); the log shows these progress lines and a warning when a stage stops advancing. With from_file, the <map>.log the compiler writes next to the map is read instead (compiles of other windows or of the command line too). | `contains`: text; `from_file`: true or false; `problems_only`: true or false; `tail`: integer 1 to 5000 |
| `get_compile_stats` | The statistics of the last compile, read from its log, to optimize the map: the engine limits like vbspinfo (models, brushes, brushsides, planes, vertexes, nodes, texinfo, texdata, displacements, faces, leaves, areas, areaportals, edges, worldlights, lightmap bytes, visdata, entities, entdata, static and detail props, overlays, cubemaps, pakfile: used, maximum, bytes, percent), the brushes by kind (structural, detail, water, tool, in brush entities), entities and faces by class and material, displacements, the areas, the small structural brushes that could be func_detail, the static props without fade distance, the visibility (clusters, portals, visible clusters per cluster, radius, pass times, the clusters that see the most with their place), the lighting (lightmap bytes per pass of the 2 GB a face can address, luxels, lights by type, switchable styles, props, bounces, ambient samples, the faces and materials with the most luxels with lightmap scale and place), the time of each stage, and optimization hints in plain words with the brush, side or place they are about. With from_file, <map>.log next to the map is read (its last compile). | `from_file`: true or false |
| `get_compile_history` | The past compiles of the map: date, preset, step times, total time, success, leak, portal counts. | `limit`: integer 1 to 1000 |
| `get_compilers` | Hammer Pro's own compiler, the default one (geometry, portals, leak test and visibility with hammerpro --bspc, lighting with hammerpro --rad, packing built in): version, executable, the flags of each step with what they do, and the content arguments the editor adds to both steps so they mount the content it shows (content_args: -wsaddons, -serveraddons, -insert_search_path). Also the compiler chosen for the game (active_compiler): other compilers (Valve tools found next to the game, the user's own profiles) are listed by list_compilers and chosen with select_compiler. | none |
| `check_leaks` | Run the instant leak check (no compile): the result comes in the background, read it with get_leak. | none |
| `get_leak` | The result of the instant leak check (leak path from an entity to the void, leaking entities, entities in solids) and the pointfile of the last compile (load_pointfile). | `points`: integer 1 to 10000 |
| `load_pointfile` | Load (or with unload: true, remove) the leak pointfile of the last compile, drawn in the views. | `unload`: true or false |
| `portal_file` | Load the .prt portal file next to the map (or a name in the map folder), build one (build: true), or unload it; answers the clusters and portals loaded. | `action`: one of info, load, build, unload; `file`: text |
| `bake_lighting` | Bake the lighting preview (luxels on the CPU, like VRAD), around the camera only (around_camera), or stop (cancel) the bake, with a quality: draft (fast), normal or final (like the compile presets; saved as the preview quality). A bake shows a quick pass first (the Sun Samples, Ambient Samples and Supersample of the map's light_plus options), then a refined pass at the quality (kind refine in get_lighting_status) replaces it unless the map changed. Read the progress with get_lighting_status; set_render_mode baked shows the result. | `action`: one of start, around_camera, stop; `quality`: one of draft, normal, final |
| `set_lighting_options` | Change the lighting preview options saved in the map (the Hammer++ light_plus block, shown by get_lighting_status in options): brightness (0 to 4), bounced light, dynamic updates (bake again once edits stop) and their delay (0 to 60 s), the radius of a bake around the camera (64 to 65536), and the sampling of the quick pass every bake shows first: sun_samples (1 to 256), ambient_samples (sky light directions, 16 to 5184) and supersample. The refined pass that follows uses the bake quality. Answers the options. | `ambient_samples`: integer 16 to 5184; `bake_radius`: number 64.0 to 65536.0; `bounced`: true or false; `brightness`: number 0.0 to 4.0; `dynamic_updates`: true or false; `sun_samples`: integer 1 to 256; `supersample`: true or false; `update_delay`: number 0.0 to 60.0 |
| `get_lighting_status` | The lighting bake: options (the map's lighting preview options, see set_lighting_options), quality, progress, last result (time, faces, luxels, density), preview statistics (with what lights the props: the world lights, the static props and those with baked vertex lighting), tone (the HDR tone mapping of the focused 3D view: mode, the exposure scale now, its range, the bloom amount, what the env_tonemap_controller of the map sets and the luminance histogram the auto exposure reads), the lightmap budget, and while the lit mode is on the realtime preview: lights drawn, suns, lights left out, crowded spots keeping only their strongest lights, HDR or LDR values, sky light occlusion, projectors (the env_projectedtexture entities drawn, setting render.projected_textures), sky_layers (how many skies lie one above the other toward the sun and straight up: each point is shadowed only by what lies between it and the sky it reaches, like VRAD), prop_shadows (what static props cast shadows with: mesh, collision or none, from the setting render.prop_shadows: auto follows the compile options, -StaticPropPolys for mesh), and the sun (direction it shines along, color, _ambient sky color, SunSpreadAngle, its light_environment) and indirect (the background bake that gives the Lit mode its sky light and bounced light: the setting render.lit_indirect, state off, no_lights, waiting, baking, ready, stale or failed, the probe cells, their spacing, the GPU bytes and the bake time). | none |
| `launch_game` | Start Garry's Mod on the compiled map, or reload it in the running game (reload: true). Only when the user asked for it: set confirm to true. Refused in sandboxed (automated) runs. | `confirm`*: true or false; `reload`: true or false |
| `list_compilers` | The compilers that can build the map for the current game, the active one first in the answer: Hammer Pro's own compiler (id builtin, the default, cannot be deleted), the Valve tools detected next to the game (vbsp, vvis, vrad of bin, bin/win64 or bin/x64, the compilers set in Hammer's GameConfig.txt and Hammer++'s hammerplusplus_gameconfig.txt, or the Source SDK Base 2013 tools; ids detected:...; their steps can be changed, restore goes back to what was found) and the user's own compilers (ids user:N). Each with its steps (kind bsp, vis, rad, copy, game or custom, program, options, extra arguments, map arguments with variables, working folder, enabled, keep going), mode_flags and the problems found (missing programs, bad option values). Also lists the variables of the arguments. | none |
| `get_compiler` | One compiler (default: the active one) with its steps, the problems found, and the command lines it would run for the open map with the current compile settings (with mode_flags, the preset and modes of the compile window add -onlyents, -onlyprops, -fast, -final, -ldr, -hdr, -both and -threads). | `compiler`: text |
| `get_compiler_flags` | Every option of Valve's vbsp (step bsp), vvis (step vis) and vrad (step rad) that the steps of a compiler can set (steps[].options of create_compiler and set_compiler): the flag, what it does, the value it takes (switch, integer or number with its range, text, file, folder, numbers), its default, its group, and set_by when the compile window writes it itself while mode_flags is on. | `language`: one of en, fr; `step`: one of bsp, vis, rad |
| `create_compiler` | Create a compiler of the user: a blank one (geometry, visibility and lighting steps with no program yet), a copy of another (from: a detected Valve compiler or another of the user's), or with its steps (programs, options as structured fields, extra and map arguments). Compilers run programs on this computer: giving steps or from needs confirm: true (only when the user asked). select: true makes it the compiler of the current game. | `confirm`: true or false; `from`: text; `mode_flags`: true or false; `name`*: text; `select`: true or false; `steps`: list of object (args, continue_on_error, enabled, exe, extra, kind, options, workdir) |
| `set_compiler` | Change a compiler of the user (ids user:N) or a detected one (ids detected:..., the changes are kept and restore: true forgets them): its name (the user's only), mode_flags, or its whole list of steps with their options (steps need confirm: true, only when the user asked: they run programs). Hammer Pro's own compiler is set with set_compile_options. Answers the compiler like get_compiler. | `compiler`*: text; `confirm`: true or false; `mode_flags`: true or false; `name`: text; `restore`: true or false; `steps`: list of object (args, continue_on_error, enabled, exe, extra, kind, options, workdir) |
| `delete_compiler` | Delete a compiler of the user. The games that used it go back to Hammer Pro's compiler. Detected compilers cannot be deleted (set_compiler restore forgets their changes). | `compiler`*: text |
| `select_compiler` | Choose the compiler of the current game (or of another game id from list_games): builtin for Hammer Pro's compiler, a detected id or a compiler of the user. Saved in the settings, kept for each game; F9, compile_map and the compile window use it. | `compiler`*: text; `game`: text |
| `reset_compilers` | Delete every compiler of the user and the changes to the detected ones, and go back to Hammer Pro's compiler for every game. Needs confirm: true (only when the user asked). | `confirm`*: true or false |

### Content and release

| Tool | What it does | Arguments |
|---|---|---|
| `get_dependency_report` | The content the map uses (materials, models, sounds, particles, textures of materials, model materials...): each file with its source (base game, mounted game, Workshop item, custom folder, generated) and size, what a release ships, the Workshop items it needs, and the missing assets with the objects using them. | `filter`: text; `limit`: integer 1 to 1000; `mounted`: true or false; `offset`: integer 0 to 4294967295; `shipped_only`: true or false; `workshop`: true or false |
| `open_release_window` | Open the Build and save as content window on a tab: build (Final compile, content folder, GMA), report, checklist, addon (addon.json) or publish. The window runs the build on its own threads. | `tab`: one of build, report, checklist, addon, publish |
| `start_release` | Run Build and save as content: the final compile (when the content settings ask for it), the content folder with the BSP and every custom material, model, sound and particle the map uses, and the GMA. The map is saved first. It runs on its own threads: follow it with get_release_status. With analyze_only, only the dependencies are resolved (nothing is written). A build writes files: set confirm to true. With protect, Protect your BSP ships a protected copy of the map (gameplay logic and entity wiring stripped, watermarked) and keeps the full one for the server. | `analyze_only`: true or false; `confirm`: true or false; `game_session`: true or false; `keep_cubemaps`: true or false; `protect`: true or false; `watermark`: text |
| `get_protect_options` | Every option of Protect your BSP as saved in the content settings: protect, watermark, the decompile breakers (strip_original_faces, randomize_materials), the server lock, and the structure scrambler (shuffle_lumps, junk_gaps, decoys, scramble_header, shuffle_game_lumps, random_pak, reorder_tables, reorder_world, layout_seed). | none |
| `set_protect_options` | Change options of Protect your BSP (saved in the content settings, used by the next start_release). The structure scrambler makes the shipped BSP hard to parse: shuffle_lumps puts the lump blocks in a random physical order, junk_gaps fills random gaps between them with random bytes, decoys adds unreferenced blocks (random data and stale altered copies of lumps), scramble_header randomizes the map revision, the version of lumps the game ignores and the unused directory entries, shuffle_game_lumps shuffles the game lump directory, random_pak puts the pakfile anywhere, reorder_tables permutes planes, vertexes, edges, texinfo and texdata and renumbers every reference, reorder_world permutes brushes and faces the same way (the face and brush lists of the leaves are kept in order: shuffling them crashed the game). The map loads and plays the same. layout_seed fixes the randomness (empty: new at each compile). preset strongest turns every scrambler option on, none turns them off; individual keys apply after it. | `decoys`: true or false; `junk_gaps`: true or false; `layout_seed`: text; `lock`: true or false; `lock_block`: true or false; `lock_key`: text; `preset`: one of strongest, none; `protect`: true or false; `random_pak`: true or false; `randomize_materials`: true or false; `reorder_tables`: true or false; `reorder_world`: true or false; `scramble_header`: true or false; `shuffle_game_lumps`: true or false; `shuffle_lumps`: true or false; `strip_original_faces`: true or false; `watermark`: text |
| `get_release_checklist` | The release checklist of the map (saved, compiled, leaks, lighting, cubemaps, navigation mesh, missing content, thumbnail, addon.json, GMA...): each row with ok, warning, failed or unknown and a detail. The first call gathers it on a thread: call again while gathering is true. | `refresh`: true or false |
| `get_release_status` | Whether the release window is open, the progress of a running release (step and fraction), and the outcome of the last release of the map (folders, GMA, files, bytes, error, the end of its log). | none |
| `cancel_release` | Stop the running release build. | none |
| `inspect_archive` | Open a GMA, VPK or BSP of the map folder in the pack inspector window (files, sizes, contents). | `file`*: text |
| `publish_workshop` | Open the Publish tab of the release window, where the user reviews and confirms the upload. Only when the user asked for it: set confirm to true. Refused in sandboxed (automated) runs. | `confirm`*: true or false |
| `list_map_assets` | Every asset the map uses, as the Map assets tab lists it, by group: materials (brush faces, displacements with both blend textures, overlays, decals, sprites, the sky, keyvalues) with their textures (editor textures included), models with their files, skins, gibs and animation sounds, sounds (entity keyvalues, soundscripts resolved to their waves, soundscapes and their files), particle files, other files. Each entry with its files: where each one comes from (base game, addon, Workshop, custom folder, loose file...), its size, and the missing ones. Instances are included. Resolved on a worker thread. | `files`: true or false; `filter`: text; `groups`: list of text; `limit`: integer 1 to 1000; `offset`: integer 0 to 4294967295; `show`: one of custom, all, missing |
| `collect_map_assets` | Copy the assets the map uses into a clean folder, like the Collect button of the Map assets tab: every entry of the groups (with the filter) and everything it needs, under materials/, models/, sound/, particles/... (files of VPKs and GMAs are extracted), then a report <map>_assets.txt in the folder. By default only what is not part of the base game. A file the folder already has with other content is kept (and listed in the report) unless replace is true. Runs in the background in the tab: follow it with get_map_assets_status. | `base_game`: true or false; `filter`: text; `groups`: list of text; `output`*: text; `replace`: true or false |
| `get_map_assets_status` | Progress of the running collect of the Map assets tab (files done and total), and what the last collect gave: output folder, files copied, replaced, already there, kept, failed (with the error), missing, bytes written, the report file. | none |
| `cancel_collect_map_assets` | Stop the running collect of the Map assets tab (the files copied so far stay, the report says it was cancelled). | none |
| `get_content_index` | State of the persistent content index (the saved list of the files of the game folders, custom folders and mounts, validated by folder modification times at launch instead of scanning everything again) and of the scan: whether it is on, the index file, its size and entries, how many folders were checked, reused or scanned again, the duration of the last folder scan, the scan mode (auto, parallel, sequential) and the type of each drive seen (rotational drives are scanned one folder at a time in auto mode). Change it with set_setting: scan.use_index (true or false) and scan.mode ("auto", "parallel", "sequential"), applied at the next mount. | none |
| `rebuild_content_index` | Delete the saved content index and mount the game content again with a full scan of every folder, like the Rebuild index button of Settings > Game (and the rebuild_content_index action). Use it when files changed in a way the index missed. The mount runs on its own threads: follow it with get_content_index. | none |
| `find_unused_assets` | The files under materials, models, sound and particles of a content folder (an addon, a garrysmod folder, or one of those folders itself) that the open map does not use: what the map needs, the textures of its materials and the files of its models count as used, even when the folder is not mounted. Report only: nothing is deleted. | `folder`*: text; `limit`: integer 1 to 1000; `offset`: integer 0 to 4294967295 |

### Documents

| Tool | What it does | Arguments |
|---|---|---|
| `list_tabs` | The maps open in this window (tabs): id, title, file, unsaved changes, and the one shown. | none |
| `switch_tab` | Show another open map (tab id from list_tabs); the tools then work on it. | `id`*: integer 0 to 9223372036854775807 |
| `close_tab` | Close an open map that has no unsaved changes (save it first), or every other map (others: true). | `id`*: integer 0 to 9223372036854775807; `others`: true or false |
| `new_map` | Open a new empty map in a new tab. | none |
| `list_recent_maps` | The maps opened recently (most recent first) and whether the files still exist. | none |
| `import_vmf` | Merge another VMF (in the map folder) into the map: its solids, entities and groups get new ids, its visgroups are merged by name. The imported objects are selected. | `file`*: text |
| `export_objects` | Write objects (default: the selection) to a new VMF in the map folder (Export selection). | `file`*: text; `ids`: list of integer; `overwrite`: true or false; `solid_ids`: list of integer |
| `export_obj` | Write the visible brush and displacement geometry with texture coordinates to an OBJ (and MTL) file in the map folder, for 3D tools. | `file`*: text; `overwrite`: true or false |
| `decompile_bsp` | BSP Decompiler: decompile a compiled map (.bsp in the map folder) into a new map tab (brushes, entities, displacements, overlays), on a worker thread; list_tabs shows it when done. To recover the map of a Garry's Mod server you join, use the BSP Sniffer tools (sniffer_start). | `file`*: text |
| `nav_mesh` | The navigation mesh shown with the map (file, areas, isolated areas), or load a .nav file of the map folder. | `load`: text |
| `get_map_properties` | The worldspawn keyvalues (skyname, detail material, light settings...), the map file, its format version and the editor version that saved it, and compiler_keys: the worldspawn keys Hammer Pro's lighting compiler reads (_lightsrad: the map's texture light file) with their value, default and meaning. | none |
| `set_map_properties` | Set or remove worldspawn keyvalues (skyname, detailmaterial, detailvbsp, maxpropscreenwidth...). A value of "" removes the key. Lighting compiler key: _lightsrad (a texture light file read like -lights: relative to the game folder or the map folder, or a full path). | `keyvalues`*: object |

### History and clipboard

| Tool | What it does | Arguments |
|---|---|---|
| `get_history` | The undo steps (most recent last), the redo steps (next first), their names in the language of the interface, the memory they use and the limits. With the setting edit.undo_restores_selection (on by default) selection changes are steps too, named Selection or Face selection: they never mark the map as modified. | none |
| `undo_steps` | Undo several steps at once (stops when there is nothing left to undo). | `count`*: integer 1 to 1000 |
| `redo_steps` | Redo several steps at once. | `count`*: integer 1 to 1000 |
| `copy_objects` | Copy objects (default: the selection) to the system clipboard as VMF text, like Ctrl+C (Hammer and Hammer++ can paste it). | `ids`: list of integer; `solid_ids`: list of integer |
| `cut_objects` | Copy objects (default: the selection) to the system clipboard and delete them, like Ctrl+X. | `ids`: list of integer; `solid_ids`: list of integer |
| `paste_clipboard` | Paste the VMF text of the system clipboard (from Hammer Pro, Hammer or Hammer++), moved by an offset. | `offset`: 3 x number |
| `paste_special` | Paste VMF text (text, or the system clipboard with from_clipboard) several times, each copy moved and rotated from the previous one around a pivot, optionally grouped, with unique or prefixed entity names. | `copies`: integer 1 to 256; `from_clipboard`: true or false; `group`: true or false; `name_prefix`: text; `offset`: 3 x number; `pivot`: 3 x number; `rotation`: 3 x number; `start_at_original`: true or false; `text`: text; `unique_names`: true or false |

### Overlays and decals

| Tool | What it does | Arguments |
|---|---|---|
| `list_overlays` | The info_overlay entities: material, faces (solid and side), origin, basis, UV corners and render order. | `limit`: integer 1 to 1000; `offset`: integer 0 to 4294967295 |
| `create_overlay` | Place an info_overlay with a material on a face (and more faces it spreads over), centered on a point of the face (default: the face center), the size of the texture or a given size, rotated around the face normal. | `center`: 3 x number; `extra_faces`: list of object (side, solid); `face`*: list of object (side, solid); `material`: text; `render_order`: integer 0 to 3; `rotation`: number -360.0 to 360.0; `size`: 2 x number |
| `set_overlay` | Change an overlay: its faces (the whole list), material, size, rotation around its normal (added), or render order. | `faces`: list of object (side, solid); `id`*: integer 1 to 4294967295; `material`: text; `render_order`: integer 0 to 3; `rotate_by`: number -360.0 to 360.0; `size`: 2 x number |
| `list_decals` | The infodecal entities: id, material (texture), origin, targetname. | `limit`: integer 1 to 1000; `offset`: integer 0 to 4294967295 |
| `create_decal` | Place an infodecal with a decal material on the surface a ray hits (origin and direction), or at a point. | `direction`: 3 x number; `low_priority`: true or false; `material`: text; `origin`*: 3 x number |

### Paths

| Tool | What it does | Arguments |
|---|---|---|
| `list_paths` | The chains of path entities of the map (path_track, path_corner, path_corner_crash, keyframe_rope, scripted_sequence... any class whose next key names the next point): for each chain its points in walking order (id, name, class, origin, next key and value, branch), whether it is closed into a loop, and the classes the path tool can place with their next key. A chain starts at a point nothing links to (a loop at its lowest id). | `limit`: integer 1 to 500 |
| `create_path` | Create a chain of path entities through a list of points, like clicking them with the path tool: each point is named from the base name ("path" gives path_1, path_2..., numbers already used in the map are skipped) and names the next one in its next key (target for path_track and path_corner, NextKey for rope keyframes...). With loop the last point links back to the first (a closed track for a func_tracktrain). The points get the FGD defaults of the class, then keyvalues. One undo step; the new points become the selection. Point a func_tracktrain's target at the first name to make it follow the path. | `base_name`: text; `class`: text; `keyvalues`: object; `loop`: true or false; `points`*: list of 3 x number |
| `insert_path_point` | Insert a point after a path point: it takes that point's next point, which now links to it (the path tool's Shift+drag). Default position: halfway to the next point (one segment further at the end of a chain). When the points after it are numbered in sequence (path_3, path_4... after path_2) the new point takes the next number and they shift by one, with every key and output naming them. One undo step. | `after`*: integer 1 to 4294967295; `position`: 3 x number |
| `remove_path_points` | Delete path points. With relink (default true) the point before each removed one links to the point after it (and a train starting on it starts on the next one), so the chain stays connected; at the end of a chain the link is cleared. One undo step. | `ids`: list of integer; `relink`: true or false |

### Skybox

| Tool | What it does | Arguments |
|---|---|---|
| `list_sky_cameras` | The sky_camera entities of the map in file order: id, name, origin, scale (and whether the engine can use it), whether the game uses it (the last one of the map does), whether the 3D skybox preview shows it, and its skybox room (bounds and how many solids and entities are inside; null when the room is open). | none |
| `preview_sky_camera` | Choose the sky camera whose 3D skybox the 3D views show around the map (without id: the one the game uses), and show or hide the 3D skybox preview. | `id`: integer 1 to 4294967295; `show`: true or false |
| `use_sky_camera_in_game` | Make this sky camera the one the game uses: it moves after the other entities of the map (the game shows the 3D skybox of the last sky_camera). One undo step. | `id`*: integer 1 to 4294967295 |
| `create_skybox_switch` | Place a skybox switch (point_hammerpro_skybox) that shows the 3D skybox of this sky camera in game when it gets the Activate input (Deactivate shows the map's own skybox again, Toggle switches). The sky camera gets a name when it has none. Wire it with add_connection (for example a trigger's OnStartTouch to <switch name>,Activate). Hammer Pro ships the switch's Lua with the map (development addon of test compiles, Build and save as content). One undo step. | `name`: text; `origin`: 3 x number; `per_player`: true or false; `sky_camera`*: integer 1 to 4294967295; `start_active`: true or false |

### Fog

| Tool | What it does | Arguments |
|---|---|---|
| `list_fog` | The fog the 3D views preview and every fog_volume. preview: render.fog_preview (the global fog of an env_fog_controller in the 3D views and the player view), the controller it shows (chosen with preview_fog, else the one the game uses: the last with the Master flag, or the first of the map) and its resolved fog (null when that controller has Fog Enable at No), sky_camera (the sky_camera of the 3D skybox preview) and sky_fog (its own fog keys, drawn in the 3D         skybox only and measured in skybox units, null when its fogenable is off or the fog preview is off),         render.fog_volume_preview and render.fog_volume_texture. The preview works like the game: linear fog from         start to end capped by max_density and mixed in gamma space, color2 blended by the view direction when         blend is on, nothing drawn past farz (the sky or the fog color shows there) and the 2D sky never fogged.         controllers: id, name, fog_enable, the fog read from its keys (color and color2 as 0 to 255, dir, blend, start, end, max_density, farz: 0 when off) and the fog_volume ids that use it. fog_volumes: id, name, fog_name, bounds (the box around its brushes, like the game), status (controller, controller_disabled, missing_controller or no_fog_name), controller, in_game (fog: the controller fog replaces the map fog while the eyes are inside; clears_fog: Fog Enable at No, no fog inside; map_fog: no controller found, the map fog stays), start_disabled, the fog the 3D view draws in it (preview_fog; default_look true when it is the default look because there is no enabled controller), post_process and color_correction (PostProcessName and ColorCorrectionName: unset, found, missing or wrong_class with the entity), and the notes the 3D view and the entity properties show. | none |
| `preview_fog` | Choose the env_fog_controller whose fog the 3D views preview (without id: the one the game uses, the last Master one or else the first) and turn the global fog preview on or off (render.fog_preview). fog_volumes turns the fog_volume preview on or off (render.fog_volume_preview) and volume_texture the brush material of the volumes (render.fog_volume_texture). | `fog_volumes`: true or false; `id`: integer 1 to 4294967295; `show`: true or false; `volume_texture`: true or false |

### Entity definitions

| Tool | What it does | Arguments |
|---|---|---|
| `get_fgd_info` | The FGD files loaded (the game's, the Hammer++ one when installed, then the extra files), the override, how many classes they define (point and brush), @mapsize, material exclusions and auto visgroups, files that could not be read and the problems found inside them. | `notes`: integer 0 to 200 |
| `add_fgd` | Add an extra FGD file (a gamemode's, Mapbase, a custom one) after the others and load the definitions again. Saved in the settings: set confirm to true when the user asked for it. | `confirm`*: true or false; `path`*: text |
| `remove_fgd` | Remove an extra FGD file from the settings and load the definitions again. Set confirm to true when the user asked for it. | `confirm`*: true or false; `path`*: text |
| `reload_fgd` | Read the FGD files again (after editing one). Files that change on disk are also reloaded on their own within a few seconds. | none |
| `validate_fgd` | Parse an FGD file without loading it in the editor: the classes it defines, its @include files, and every problem found (file:line: message). Runs on a worker thread. | `notes`: integer 0 to 200; `path`*: text |

### Games

| Tool | What it does | Arguments |
|---|---|---|
| `list_games` | The Source games Hammer Pro compiles maps for: those found on this computer first (Steam libraries and sourcemods, with their folders), then the others (maps still compile in their format). Each with its id, engine branch, BSP version, static prop format, limits, FGD files and executable. | `installed_only`: true or false |
| `get_game` | The game the open map is for (its own one, kept with the map, else the default one of Settings > Game) with its profile: folders, BSP format, limits, FGD files, executable. | none |
| `set_game` | Make a game (an id from list_games) the game of the open map (scope "map", kept with the map), or the default game of every map (scope "default", saved in the settings: set confirm). Its content and entity definitions load when it is installed; the compile follows its BSP format, limits and lighting. | `confirm`: true or false; `id`*: text; `scope`: one of map, default |
| `list_app_mounts` | The Steam games whose content is mounted by AppID (Settings > Game > Steam games by AppID, setting game.mount_app_ids), in priority order, each with its name, install folder and Source game folders as found in the Steam libraries (libraryfolders.vdf, appmanifest_<AppID>.acf); and, with installed, every installed Steam game that has a Source game folder. | `installed`: true or false |
| `set_app_mounts` | Change the Steam games whose content is mounted (after the content of the map's game): add AppIDs at the end, remove some, or give the whole list in priority order. Saved in the settings (game.mount_app_ids); the content is mounted again for the browsers, the 3D view and the compile. Examples: 220 Half-Life 2, 240 Counter-Strike: Source, 300 Day of Defeat: Source, 380 and 420 the episodes, 400 Portal, 440 Team Fortress 2, 4000 Garry's Mod. | `add`: list of integer 1 to 99999999; `order`: list of integer 1 to 99999999; `refresh`: true or false; `remove`: list of integer 1 to 99999999 |

### Discord

| Tool | What it does | Arguments |
|---|---|---|
| `get_discord_status` | The state of the Discord Rich Presence: status (connected, connecting, discord_not_running, refused, disabled, no_application_id, bad_application_id, off in automated runs) with its text as shown in Settings > Discord, the application ID in use (Hammer Pro's own one unless the setting discord.application_id replaces it) and the details and state lines sent to Discord. The options themselves are the discord.* settings (get_settings, set_setting). | none |

### Updates

| Tool | What it does | Arguments |
|---|---|---|
| `get_update_status` | The state of the auto update: installed version, status (idle, checking, up_to_date, available, downloading, ready, failed), the newest release found (version, tag, title, release notes, page, archive name and size), the download progress, the last error, and the preferences update.check_on_startup (check in the background at startup), update.show_dialog (show the update window at all) and update.skipped_version (the version the user chose not to be asked about). The preferences are changed with set_setting on the update.* keys. | none |
| `check_for_updates` | Ask GitHub, in the background, whether a newer Hammer Pro release exists (same as Help > Check for updates...). Answers at once; read the result with get_update_status a few seconds later. When one is found, the update dialog opens for the user. Nothing is downloaded or installed. | none |
| `apply_update` | Download the available update, then close Hammer Pro, replace the program and start it again. It interrupts the session (the user is asked to save unsaved maps first) and the connection to this MCP server ends: run it only when the user explicitly asked to update now. Needs confirm: true, a found update (check_for_updates, get_update_status shows status available) and no running compile. | `confirm`*: true or false |

### Tool options

| Tool | What it does | Arguments |
|---|---|---|
| `get_tool_options` | The options of the editing tools by name with their current values and what each takes: the selection tool (handles: scale, rotate, skew; box: touching, inside, centers; gizmo on/off and gizmo_mode: move, rotate, scale; gizmo_local; stretch_handles: the arrows on the faces of the selected brushes in the 3D view that stretch the selection along an axis, saved with the settings), the block tool (primitive, sides, arch wall, arc, start, rise, torus, reorient, the shape preview window, texture scale, create as, keep active), the curve tool (subdivisions 2 to 4096, thickness 1 to 1024, height or auto, tex_scale, create_as, keep_active), the clip tool (keep: front, back, both; three_point; axis of the Shift+click cut in the 3D view: auto, x, y, z; the cut face material follows the brush.clip_current_material setting), the path tool (class, base_name, loop, relink, offset; saved with the settings), the extrude / carve tool (mode: carve, extrude; depth; through; new_faces: face, current), the bevel tool (radius, segments, inverse, new_faces: face, current) and the displacement tool's brush (mode: select, geometry, alpha, sculpt, tag; effect; axis: normal, subdiv, center, view, view_flat (toward the camera on X and Y only), x, y, z, custom; radius, strength (intensity), smooth, target height, falloff, spatial, line of sight, autosew (live sewing), selected only, continuous, alpha effect, value and strength, blend channel, sculpt push, carve and project settings with bounds limit and falloff position and final, tag kind and value, grid and select masks). The displacement options are saved with the settings. Default: every tool that has options, and which tool is active. | `tool`: one of select, camera, entity, block, texture, decal, overlay, clip, extrude, bevel, vertex, path, polygon, curve, sprinkle, displacement, cordon |
| `set_tool_options` | Change options of an editing tool like its panel does (names and values from get_tool_options), e.g. tool select, options {"stretch_handles": false}, or tool displacement, options {"axis": "view", "strength": 200, "autosew": true}, or tool block, options {"kind": "arch", "arc": 180, "preview": true}. Values are text, numbers, booleans or lists of numbers (custom_axis, carve_profile). With activate the tool becomes the active one. Answers the tool's options. | `activate`: true or false; `options`*: object; `tool`: one of select, camera, entity, block, texture, decal, overlay, clip, extrude, bevel, vertex, path, polygon, curve, sprinkle, displacement, cordon |

### Graphics memory

| Tool | What it does | Arguments |
|---|---|---|
| `get_gpu_memory` | How much graphics memory (VRAM) Hammer Pro uses and its budget: usage (from the driver when it reports it, else an estimate), budget (render.vram_budget_mb, 0 means automatic: 90% of what the system gives the process), pressure (normal, high above 85%, critical when over the budget or after an out of memory error), the estimated split (geometry, textures, models, lightmaps, compiled world, other, maps parked in background tabs), the texture size limit applied under pressure, and what was freed so far. | none |
| `free_gpu_memory` | Free graphics memory now: the maps parked in background tabs (they load again when shown), textures and models no open map uses, and the thumbnail caches. With lower_textures, textures are also loaded again at half their size limit (they come back to full size when memory is free for a while). | `lower_textures`: true or false |

### Storage

| Tool | What it does | Arguments |
|---|---|---|
| `get_storage_usage` | How much disk space Hammer Pro itself uses and what uses the most, scanned on a background thread (the answer comes when the scan is done). Returns total_bytes, total_files and items sorted by size, largest first: id (use it with delete_storage), name, description, risk (safe: caches and temporary files rebuilt on demand; recovery: autosaves and crash reports; user_data: settings, prefabs, sprinkle presets and per map state), bytes, human size, files, percent of the total, paths, in_use_items (kept while Hammer Pro uses them) and freeable_bytes. Only Hammer Pro's own settings, cache, autosave, log, crash and temporary folders are listed: maps and game folders are never part of it. | none |
| `delete_storage` | Delete the files of storage items listed by get_storage_usage, to free disk space. Nothing is deleted unless confirm is true: without it the tool deletes nothing and only reports what would be deleted and how much space it frees. Items of risk user_data (settings, prefabs, sprinkle presets, per map state) also need allow_user_data true. Files in use are skipped and errors are reported per item. Compile related items are refused while a compile runs. The folders of Hammer Pro are emptied, maps and game folders are never touched, and the result cannot be undone. | `allow_user_data`: true or false; `confirm`: true or false; `ids`*: list of text |

<!-- END TOOL LIST -->

## Seeing and driving the interface

Besides the semantic tools, the AI can reach everything you can:

- `get_capabilities` lists every tool by category with an example call
  (start here), or one tool with its schema.
- `list_actions` and `run_action` expose every command of the menus,
  toolbars and shortcuts automatically, with its enabled and checked state.
- `get_settings` and `set_setting` read and change any setting by path
  (`view.fov_deg`, `ui.theme`), validated like the Settings window;
  `list_keybindings` and `set_keybinding` the shortcuts.
- Propper: `run_action brushes_to_prop` converts the selected brushes and
  displacements into a static prop (displacements collide through slabs
  under their surface, blend materials are baked into a texture of the
  model), `prop_to_brushes` goes back. The levels of detail of every model
  Hammer Pro makes (Propper and the FBX, OBJ, glTF import) are the settings
  `lab.model_lod.enabled`, `lab.model_lod.levels` (3 to 6, default 4) and
  `lab.model_lod.reduction` (0.2 to 0.9, default 0.5); `run_action
  propper_options` opens their window.
- `get_views`, `set_view_layout`, `set_render_mode`, `zoom_2d`, `pan_2d`
  and `set_camera` arrange the views (render modes, the baked lighting
  preview, the 2D displacement mesh or outline of each view). The water
  preview (water drawn like in game in the textured 3D modes) is the
  setting `render.water_preview` and the action `toggle_water_preview`.
  The fog of the `fog_volume` brushes (from the `env_fog_controller` of
  their Fog Name, also applied in the player view while the eyes are
  inside) is `render.fog_volume_preview` (`toggle_fog_volumes`), their
  brush material `render.fog_volume_texture`
  (`toggle_fog_volume_texture`); the fish of `func_fish_pool` are
  `render.fish_preview` (`toggle_fish_preview`) and swim with
  `render.fish_swim` (`toggle_fish_swim`).
  The relief of the normal maps (`$bumpmap`, `$bumpmap2` of blends,
  `$ssbump`, on brushes, displacements and props; the compiled bumped
  lightmaps in the player view) is the setting `render.normal_maps` (on by
  default).
- `get_tool_options` and `set_tool_options` read and change the options of
  the editing tools like their panels (the block tool's primitives, arch
  and its preview window; the displacement brush: axis with the view
  direction, intensity, live sewing, line of sight, sculpting...), and
  `stroke_displacement` paints with that brush like a drag in the 3D view.
- `get_compile_options` and `set_compile_options` read and change every
  compile setting and every option of the compiler (`get_compilers` lists
  the flags with what they do); `list_compilers`, `get_compiler`,
  `get_compiler_flags`, `create_compiler`, `set_compiler`,
  `delete_compiler`, `select_compiler` and `reset_compilers` manage the
  other compilers (Valve's tools found next to the game, your own) with every
  option of vbsp, vvis and vrad as structured fields; `get_compile_log` explains the problems the
  compile reports, from this window or from `<map>.log`; `get_compile_stats`
  gives the statistics of that log to optimize the map (engine limits like
  vbspinfo, brushes, materials, areas, the clusters that see the most, the
  faces with the most luxels, stage times) and hints in plain words with
  the brush, side or place they are about.
- `screenshot_window` returns a PNG of the whole window (optionally a
  region, downscaled), `screenshot_view` one view rendered offscreen.
- `describe_ui` tells what is on screen: open maps, views and their
  rectangles, panels, open windows and dialogs with their texts and their
  controls (with the point to click), open menus, the focused widget, the
  status bar, notifications and the end of the console. The buttons of the
  tool palette (the strip along the left edge) are named after their tool
  ("Block", "Clip"...).
- `ui_move`, `ui_click`, `ui_double_click`, `ui_drag`, `ui_scroll`,
  `ui_key` and `ui_type` send mouse and keyboard input like the user, at
  window points, at fractions of a view, or at world points of a view
  (`{"view": "top", "world": [0, 0, 0]}`); `wait_frames` lets the interface
  settle. The input goes through the same path as the `--script` engine.

## For developers

- `crates/hp-mcp`: protocol (`protocol.rs`), tool list and schemas
  (`tools.rs`), instance records (`registry.rs`), the window server
  (`server.rs`), the bridge (`bridge.rs`), the installer (`install.rs`).
- `crates/hammerpro/src/dispatch/mcp.rs`: server life, request dispatch,
  compile and open; `dispatch/mcp/tools.rs`: the core tools and the
  registry (`DOMAINS`: one `tools_<area>.rs` per area, each with its
  category, tool descriptions and `run`); `dispatch/mcp/window.rs`: the MCP
  window.
- Generic tools: `tools_actions.rs` (every action), `tools_settings.rs`,
  `tools_views.rs`, `tools_ui.rs` (UI automation and vision),
  `tools_help.rs` (`get_capabilities` and the tool list of this page);
  `automation.rs` answers the calls that need frames (input injected from
  `raw_input_hook`, screenshots encoded on a worker thread) and
  `describe.rs` reads egui's AccessKit tree for `describe_ui`.
- The tool list above is generated: after adding or changing a tool, run
  `HP_UPDATE_MCP_DOCS=1 cargo test -p hammerpro docs_list_every_tool`.
  `harness.rs` calls every tool through the protocol with its example and
  with invalid arguments.
- Tool options: `tools_toolopts.rs` uses the tools' own
  `Tool::option_values` and `Tool::set_option` (the `tool_option` command
  of `--script` too) and `Tool::scripted_stroke` (`stroke_displacement`);
  a tool with options lists them there. The UV layers use
  `hp_ui_textures::uv_layers::{mcp_layers, mcp_apply_layer}`, the same
  model and writes as the UV editor.
- `crates/hp-app/src/{actions,settings,runtime}/mcp.rs`,
  `crates/hp-ui-shell/src/{status,settings_dialog}/mcp.rs` and
  `toolbar::mcp_pill`: actions, settings, shared state and UI.
- `HAMMERPRO_MCP_DIR` moves the instance records (tests and automated
  checks). `scripts/mcp_check.py` drives a real Hammer Pro through the
  bridge.
