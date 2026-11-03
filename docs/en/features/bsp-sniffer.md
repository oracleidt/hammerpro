[Documentation](../README.md) > Features > BSP Sniffer | [Français](../../fr/features/bsp-sniffer.md)

# BSP Sniffer

The BSP Sniffer joins a Garry's Mod server and recovers the map exactly as the game loaded it, then writes a VMF you can open in Hammer Pro, the recovered BSP, and the content of the map (models, materials, sounds, pakfile). The map is rebuilt from the memory of the running game: it never needs the map file. A map protected with [Protect your BSP](bsp-protection.md) is recovered too: the engine has to load the map to draw it, so the sniffer reads what the engine loaded and restores what the protector scrambled. You type an `ip:port`, click **Sniff**, and Hammer Pro does the rest: it starts Garry's Mod through Steam, joins the server, waits for the map to load, reads the game memory, rebuilds the map, decompiles it and closes the game.

**It works only for Garry's Mod.** It was tested with the x86-64 branch of Garry's Mod (build 2026.09.17, Steam build 26229091) on Windows. Other Source games and other builds are not supported.

## Authorized use only

Only use the BSP Sniffer on servers and maps you are authorized to test. You are responsible for what you do with it. The Hammer Pro authors are not responsible for how it is used, or for any consequence for your accounts, servers or maps.

The first time you open the panel a warning dialog asks you to confirm that you understood. The warning stays visible in the panel, and **Sniff** stays disabled until you tick **I am authorized to test this server and map**.

On a VAC secured server the game runs with VAC while Hammer Pro reads its memory. Hammer Pro only reads (it never writes, injects or hooks anything), but a server or a community that forbids this kind of tool may sanction you. Hammer Pro asks the server first and adds `-insecure` only when the server is not VAC secured (setting **-insecure**, automatic by default): a VAC secured server refuses an insecure game.

## Opening it

**File > BSP Sniffer...** (action `bsp_sniffer`) opens the panel. **File > BSP Decompiler...** is the other tool: it decompiles a `.bsp` file you already have.

## The panel

| Control | What it does |
| --- | --- |
| **Server (IP:port)** | The server to join. Without a port, 27015 is used. `map:<name>` starts the map `<name>` of this computer in a local Garry's Mod instead (see [Local game and running game](#local-game-and-running-game)). |
| **I am authorized to test this server and map** | Required to enable **Sniff**. |
| **Sniff** | Starts the run. |
| **Cancel** | Stops the run. The game that Hammer Pro started is closed (when **Close Garry's Mod when done** is on). |
| Status | The phase, a progress bar, the elapsed time, the game process and the map. |
| **Result** | Counts of the loaded map and of the VMF, and the buttons **Open the VMF**, **Show the folder** and **Copy the report**. |
| **Settings** | Every option below. They are saved. |
| **Log** | What happened, with times, and **Copy the log**. |

## Local game and running game

A second Garry's Mod cannot join your own server on the same computer (Steam allows one game), so the sniffer has two equivalents that give the client memory of a real join:

- **`map:<name>` as the server**: Hammer Pro starts Garry's Mod with `-insecure +sv_lan 1 +maxplayers 2 +map <name>`, waits for the console to report `Map: <name>`, reads the memory and closes the game. The map file must be in `garrysmod/maps` (or where the game finds it), exactly where a downloaded map ends up. The server query is skipped.
- **`map:` alone with *Use Garry's Mod if it is already running***: reads the game that is already running with a map loaded (singleplayer, listen server or a server joined by you). The map name is taken from the console log. Hammer Pro never closes a game it did not start. Turn **Disconnect from the server when done** off in this mode: disconnecting unloads the map and leaves stale data in the memory.

## Settings

There is no mode to choose: the sniffer always reads the game memory and merges it with the map file that the client has (game cache `map_pack.bsp`, `maps`, `download`, addons, workshop archives) when that file matches the memory (same entities, same verified lumps). Every field taken from the file is checked again against the memory first, and a file that is altered or different is ignored. The file is read through its lump directory, so a protected file with shuffled lumps, gaps, decoy blocks, a scrambled header or a pakfile anywhere is read like any other.

| Setting | What it does | Default |
| --- | --- | --- |
| **Output folder** (`output_dir`) | Where the files go. Empty: `%LOCALAPPDATA%\HammerPro\sniffer` (the folder the Storage panel lists). | empty |
| **File name** (`file_name_template`) | Name of the VMF without extension. Tokens: `{map}` `{ip}` `{port}` `{date}` `{time}` `{datetime}`. Unsafe characters are replaced; an existing file is never overwritten (a number is added). | `{map}_sniffed` |
| **Garry's Mod folder** (`gmod_dir`) | The folder that holds `garrysmod`. Empty: detected from Steam. Both the 64 bit branch (`bin\win64\gmod.exe`) and the 32 bit one are handled. | automatic |
| **Launch options** (`launch_options`) | Added to the Steam launch. The server address (`+connect`), `-condebug` and `-insecure` are managed for you: the connection cannot be redirected by this text. | `-windowed -w 800 -h 450 -novid -nojoy -nosplash -console +mat_picmip 4 +fps_max 30 +volume 0` |
| **-insecure (no VAC)** (`insecure`) | Automatic: only for servers that are not VAC secured. Always. Never. | automatic |
| **Use Garry's Mod if it is already running** (`attach_running`) | Reads the running game instead of starting one. Hammer Pro never closes a game it did not start. The running game must already be connected to the server. | off |
| **Wait for the game process** (`process_timeout_secs`) | How long to wait for the game to appear. | 180 s |
| **Wait for the map** (`load_timeout_secs`) | How long to wait for the connection, the downloads and the map. | 900 s |
| **Settle time once found** (`settle_secs`) | The map must stay identical in memory for this long before it is captured. | 6 s |
| **Check every** (`poll_secs`) | The polling interval. | 3 s |
| **Save models** (`save_models`) | Copies the `.mdl`, `.vvd`, `.vtx` and `.phy` files the map uses. | on |
| **Save materials** (`save_materials`) | Copies the `.vmt` and `.vtf` files the map uses. | on |
| **Save sounds** (`save_sounds`) | Copies the sounds the map uses. | on |
| **Save other embedded content** (`save_other`) | Copies the rest of the pakfile and of the content (scripts, particles...). | on |
| **Only content that is not in the stock game** (`only_non_stock`) | Skips files that already exist in the base game and its VPKs. | on |
| **Keep the recovered BSP** (`keep_bsp`) | Writes the BSP next to the VMF. | on |
| **Decompile** (`decompile.*`) | Brushes, displacements, entities, brush entities, static props, overlays, cubemaps, areaportals, occluders, group `func_detail`: the same switches as the decompiler. | all on |
| **Close Garry's Mod when done** (`kill_game`) | Closes the game that Hammer Pro started, also after a failure or a cancel. | on |
| **Disconnect from the server when done** (`auto_disconnect`) | When the game is left open, asks it to disconnect. | on |
| **Open the VMF in a new tab when done** (`open_when_done`) | Opens the result. | on |

## What you get

In the output folder, for a map called `rp_example`:

| File | Content |
| --- | --- |
| `rp_example_sniffed.vmf` | The map: brushes (all of them, including nodraw, tool, clip, skybox and trigger textures), displacements, entities with all their keyvalues and outputs, static props, overlays, cubemaps, areaportals. |
| `rp_example_sniffed.bsp` | The recovered BSP (when **Keep the recovered BSP** is on). |
| `rp_example_sniffed_content\` | `materials`, `models`, `sound` and the other files of the map (pakfile and game content). |
| `rp_example_sniffed_sniff.txt` | The report: what was confirmed in memory, the counts, the timings. |

## How it works

1. **Preflight.** The server is asked with the Source query protocol (map, players, VAC). Garry's Mod is detected (64 bit and 32 bit branches).
2. **Launch.** Steam starts the game with your launch options, `-condebug` and `+connect ip:port`. Hammer Pro finds the game process by its image path, so Steam helper processes are ignored.
3. **Wait.** The console log tells when the connection is made and which map is loaded. The sniffer then scans the memory of the game for the entity lump of the map (the `worldspawn` entity and its siblings). The menu background map that is always loaded first is ignored, and the map must stay identical for the settle time. The entity lump is in memory well before the engine has finished building the map, so the sniffer keeps reading until the engine has built every displacement (the face records carry a displacement flag, which gives the expected count), every overlay has its faces, the static props are all resolved, and two consecutive scans give identical counts. This wait is bounded by **Wait for the map**; a slow server can take several minutes.
4. **Capture.** Everything below is decoded from the memory of the game, with no file involved:
   - the entity lump, with every keyvalue and every output;
   - the collision data (brushes, brush sides, axis aligned boxes, planes, nodes, leafs, models, and the material of every side);
   - the render data: the face records (plane, side, vertex indices, texture info, displacement object), the vertex array and the vertex index array, and the texture info records (88 bytes: the two texture axes, the lightmap axes, flags and the material). The texture info of a material is told by the order of the material objects, which follows the order of the collision surface names;
   - the displacements: the engine keeps, for every displacement, its start position, its power and the final position of every vertex. The sniffer rebuilds the flat grid of the face and turns the offset of every vertex into the direction and distance stored in a BSP; the triangle tags are recomputed from the slopes;
   - the static props: the engine keeps one object per prop (model, origin, angles, skin, solidity, leaf range); the list is found as a run of equal sized objects whose model names resolve to `.mdl` files;
   - the overlays: the engine keeps the table with the texture info, the faces, the UV extents and the origin and basis of every overlay;
   - the cubemaps: the names of the cubemap textures, `maps/<map>/c<x>_<y>_<z>`, carry the origin of every `env_cubemap`;
   - the area portals and their vertices: found as a chain of records whose vertices lie on the portal planes;
   - when a map file is allowed and found, a cross check: the lumps the engine keeps as they are (planes, vertexes, visibility, leaf brushes, areaportals, lighting, ambient lighting, vertex normals, texture names...) are compared byte for byte, the collision data brush by brush, and the counts of everything are compared in the report.
5. **Fill in (hybrid).** A map file that matched the memory (the same entities, lumps confirmed byte for byte) fills in, record by record, what the engine does not keep. Each record is validated against the memory first and ignored when it disagrees:
   - the displacement alphas, triangle tags and allowed vertices (a displacement is taken from the file only when its start, power and every vertex offset agree with the memory within 0.05 units);
   - the fade distances, flags and solidity of the static props (matched by model, origin and angles);
   - the cubemap sizes (matched by origin) and the overlay render order (matched by id, texture and origin);
   - the texture axes of the brush sides that no rendered face covers (matched by brush and plane, and only when the texture axes equal the ones in memory);
   - the pakfile of the map.
   The report lists, for every category, its source: memory, map file, game content or absent.
6. **Rebuild.** A valid BSP is assembled from what was decoded and filled in: the brush table, brush sides, planes, nodes, leafs, models, faces, edges, vertexes, texture info and texture data, displacement info, vertices and triangle tags, overlays, cubemaps, area portals, the static props as a game lump and the pakfile. Each brush side takes the texture axes of the rendered face that lies on its plane and inside its brush (the largest overlap wins); a side that no face covers, because it is hidden inside other brushes, gets the axes of the file when there is one, else world aligned axes. When the game memory has no render data and a file exists, the map file is used instead.
7. **Decompile.** The BSP goes through the same decompiler as **File > BSP Decompiler...**, with your switches.
8. **Content.** The game content (downloaded workshop addons, addons, the server downloads) is searched for the models, materials, sounds and particles the VMF uses. The pakfile of the map exists only in the map file: it is extracted when a map file is allowed, found and matching. Without it, the `.mdl` of every model that the game loaded and that is not on this computer is recovered from the memory of the game; the other files of a pakfile (the patched materials and cubemap textures made by the compiler, custom textures, `.vvd`, `.vtx` and `.phy` files) are not in memory in a usable form and are listed as missing in the report. Files that exist in the base game are skipped when **Only content that is not in the stock game** is on.
9. **Finish.** The game is closed by its own process id (and its helper processes). Nothing else is ever closed.

Memory is read with `ReadProcessMemory` after enabling `SeDebugPrivilege` when allowed; the 64 bit game needs the 64 bit Hammer Pro. The game is only read, never modified.

## Protected maps

[Protect your BSP](bsp-protection.md) ships a copy of the map that drops the logic entities and every input and output, empties the original faces lump, scrambles the material names (`hp/xxxxxxxx`, with a patch material that includes the real one), watermarks the world entity and can add a server lock. The game still loads and draws that copy, so the sniffer reads the same data as for any map and then undoes what it can:

| What the protector does | What the sniffer recovers |
| --- | --- |
| Scrambled material names (`hp/` plus eight hexadecimal digits) | With a matching map file: every stock material, exactly, from the `include` of the patch material in the pakfile of the protected map (the patch files are then left out of the saved content). Without a map file: the loaded material objects keep the reflectivity of their texture; the sniffer indexes the texture headers of the game content (cached in the sniffer folder, a few minutes the first time) and names the material after the texture with the same reflectivity. A reflectivity shared by more than four textures is not used. A guess can be wrong when two materials share a texture, and the report counts guessed names apart. |
| Custom materials and textures renamed (`hp/` files in the pakfile) | They are kept with their scrambled name, and the files are saved in the content under that name, so the VMF still renders. The original name was replaced by a hash with a random salt that is not in the map: it cannot be recovered. |
| Logic, `info_node` and other server entities removed, inputs and outputs, `hammerid`, `parentname` and comments removed | Not recoverable: the client never receives them. Every other entity is complete. |
| Original faces lump emptied | The engine does not use it. A brush side that no rendered face covers keeps the texture axes of the brush side record of the map file (hybrid mode); without the map file, those axes are world aligned. A very small number of sides whose texture came from the original faces keep the texture of the brush side record. |
| Watermark and server lock keys on the world entity | Read, shown in the report and removed from the VMF. |

The report has a line `protected map` and a row `material names of a protected map` in the table of sources with the numbers of each case.


## Everything the client has

The rebuilt BSP starts from the game memory. The map file of the client is then used for every lump that the memory does not hold, but only when the structures that lump points into are provably the same in both: the leaf list (contents, cluster, area and brush range) for the leaf indexed lumps, the face list (edge range and texture info) for the face indexed ones, and the model list (origin and head node) for the physics. That brings across, byte for byte, about 30 MB on the small test map and about 140 MB on a 155 MB workshop map: the visibility (PVS), the LDR and HDR lightmaps with the lightmap fields of every face, the leaf ambient lighting (LDR and HDR, with their indexes), the displacement lightmap samples, the physics meshes, the face ids, the areas, the distance to water of every leaf, and the detail prop game lump. On both test maps nothing is refused. The report lists what was taken by name with the number of bytes, and names anything it refused.

A lump is refused when what it indexes differs, which is the honest answer: its numbers would point at the wrong structures. The sniffer never takes the lumps it rebuilds itself (faces, edges, surfedges, vertexes, planes, texture info, brushes, nodes, leafs, displacements, overlays, cubemaps, area portals, static props, entities, pakfile), and it leaves out the ones whose numbering follows one of those: the leaf face list, the occluders, the vertex normals and their indexes, the primitives (water and other T junction surfaces), the world lights, the detail prop lighting (it follows the static prop order, which the sniffer rebuilds), the map flags and the overlay fades.

The leaf list of the game sometimes starts a record or two before the real first leaf, because the bytes in front of it can look like a leaf. The sniffer compares its leaf list with the one of the file field by field (contents, cluster, area and brush range) and drops those records, so its leaf numbers are the ones the nodes of the map use. Without that, a map like the 155 MB one had every leaf shifted by one and none of the leaf indexed lumps could be taken.

## Limits

- Garry's Mod only, Windows only.
- Hammer Pro reads the memory structures of the x86-64 branch of Garry's Mod, build 2026.09.17. The 32 bit branch was not installed for testing and uses other structure sizes: the entity lump and the collision data scans report what they find, the render data is reported as not found, and a map file (when allowed) is then used for the geometry. On another build the report shows which structure was not recognised.
- What the engine does not keep is taken from the matching map file of the client when it is consistent, and approximated when no usable map file is found:
  - the vertex alphas of displacements (the texture blend painting) are exported as 0;
  - the fade distances of static props are left out, and the few flags that are not kept (no shadows, no self shadowing) are not set; a prop that the engine made non solid is exported with that solidity;
  - the size of a cubemap is exported as the usual value, and two cubemaps at the same origin become one;
  - the render order of overlays is exported as 0;
  - the allowed vertices of displacements are exported as the default, and the tags are the ones the slopes give;
  - the texture axes of brush sides that no face covers (hidden inside other brushes) are world aligned;
  - what was searched in a full snapshot of the game process of the test map, and not found:
    - displacement alphas: the alphas of one displacement as floats, bytes, 16 bit and 32 bit integers, inverted or divided by 255, at every stride up to 128 bytes, are nowhere; the alphas are free floats (596 distinct values, some above 255) and three rare ones are never contiguous; three levels of pointers from the displacement object hold no array equal to them. The blend is built in the vertex buffers of the material system, which the game process does not keep readable;
    - fade distances of static props: the pairs (4000, 5000) and the others exist nowhere as floats, squares, differences, integers or 16 bit values, neither in the 256 byte prop object nor within four levels of pointers from it;
    - no shadow and no self shadowing flags: no byte of the prop object differs consistently for the props that have them;
    - pakfile content: textures 1 of 1276 samples found, material text 0 of 1266 (the engine parses it into key values and drops the text), vertex files (.vvd) only the tangent section (10 of 97 samples), strip files (.vtx) 0 of 26, physics files (.phy) have their body but not their header region, so a valid file cannot be rebuilt; only the .mdl is whole in memory (8 of 8) and is recovered;
    - cubemap size and overlay render order: the cubemap texture objects name themselves through string pool handles with no readable size next to them, and the render order is 0 for all 109 overlays of the test map, so the field could not be located.
- What the server never sends to the client cannot be recovered: logic entities removed by [Protect your BSP](bsp-protection.md), server only scripts, anything the server keeps to itself. The navigation mesh is a separate `.nav` file that the server never sends either.
- Occluders could not be measured: none of the maps used for the tests has one. The occlusion lump of the file is refused because it points into the vertex and face lists that the sniffer rebuilds, so an `func_occluder` of the map is not in the VMF.
- Protected maps: custom materials renamed by the protector keep their scrambled name (the salt is random and not in the map); without a map file the material names of a protected map are guessed from the texture reflectivity and the guess can be wrong or missing; the entities the protector removed are gone.
- Instances, groups, visgroups and compiler-only brushes are lost, like with any decompile.
- Models and materials are copied from the content on this computer (workshop downloads, addons, server downloads), and the models the game loaded are recovered from its memory; a file that is nowhere is listed as missing in the report.
- The engine can move its structures while they are being read (it finishes loading, or the map is reloaded). The sniffer notices it, because what it finds collapses to nothing, keeps the best complete reading it already had, and looks for the map again from scratch, up to three times. A reading that is worse than the one before never replaces it, so a run either gives the complete map or says in the report what was incomplete.
- A heavy server can need minutes before the engine has built the displacements and overlays; the game answers the automatic disconnect only once it is idle, so the confirmation can take a minute.

## Measured accuracy

The test map is `gm_construct` (2203 brushes, 110 displaced faces, 182 static props, 11 overlays, 28 cubemaps, 607 pakfile files) with a custom material and a custom model added to its pakfile, protected with every option of [Protect your BSP](bsp-protection.md): logic and connections removed, original faces emptied, material names scrambled, watermark, server lock, and then every layout option (shuffled lumps, junk gaps, decoy blocks, scrambled header, shuffled game lumps, pakfile at a random place, reordered tables and world). Each variant was copied to `garrysmod/maps`, started with `map:<name>`, sniffed, and the recovered BSP and VMF were compared with the unprotected map. The result is the same for the protected map and for the ten layout variants (all options together, all layout options, and each option alone):

| What | Recovered |
| --- | --- |
| Solids in the VMF (nodraw, tool, clip, skybox and trigger included), same planes | 2313 of 2313 |
| Brushes and brush models (func_brush and others), same brush sets | 2203 of 2203, 12 of 12 |
| Tool sides (nodraw, skybox, skip, hint, trigger), same material | 4708 of 4708 |
| Brush sides with the real material name (not `hp/xxxxxxxx`) | 12714 of 13798; the other 1084 are the custom material renamed by the protector and 1 side |
| Texture axes of all brush sides | 13796 of 13798 |
| Displacements: positions, normals, distances, alphas, triangle tags, allowed vertices | 91 of 91 (110 displaced faces) |
| Static props: model, origin, angles, skin, fade distances, flags, solidity | 182 of 182 |
| Overlays, every key | 11 of 11 |
| Cubemaps with their size | 28 of 28 |
| Entities that the protector kept, every key and value | 663 of 663 |
| Pakfile | 607 of 607 files, plus the renamed custom files under their `hp/` names |

What stays lost, with the reason: the 839 logic entities and the connections the protector removed (never sent to the client); the real name of the custom material (replaced by a hash with a random salt that is not in the map); 2 sides whose texture axes came from the original faces lump that the protector emptied (the brush side record has other axes, and the decompile of the protected file gives the same result).

The same sniffer on `union_blue_lock_v2` (a 155 MB workshop map, 8180 brushes, 211 displaced faces, 4967 static props) protected the same way: 8176 of its 8180 brushes and all 812 brush models with the same brush sets, 50624 of 50683 brush sides with the real material name, 50549 of 50683 with the same texture axes, all 211 displacements, all 4967 static props, and 140 MB of lumps taken from the map file. The four brushes that differ are hint and skip brushes that the engine keeps as an axis aligned box: the two slanted faces they have in the file are not in the game at all. The 59 sides whose material differs are sides that no face covers, where the surface name the engine keeps and the brush side of the file disagree; taking the name of the file instead costs more than it gains (50482 correct instead of 50624), because the brush side ranges of that map are damaged, so the name of the game is kept.

## MCP

Every setting and the run itself are reachable through the tools `sniffer_start`, `sniffer_cancel`, `sniffer_status`, `sniffer_log`, `sniffer_result`, `sniffer_get_settings` and `sniffer_set_settings`; the panel opens with the action `bsp_sniffer` (`run_action`). See the [MCP tool reference](../../MCP.md).

## Tips

- Run it once with **Close Garry's Mod when done** off while testing your launch options, then turn it on.
- Keep **Only content that is not in the stock game** on: the result stays small.
- The report file lists what was decoded from the memory with the addresses, the number of map file reads and the optional cross check: attach it to a bug report.

## FAQ

**Does it work with a server I do not own?** Only if you are authorized to test it. The panel asks you to confirm.

**Why does it start the game instead of capturing the network?** The game decodes and loads the map itself; reading what it loaded works whatever way the map reached the client.

**Can it get the server's logic?** No: only what the client receives.

**Is my game or account at risk?** Hammer Pro only reads the memory of the game. On VAC secured servers the game runs with VAC; use it at your own risk and only where the owners allow it.

## Related pages

- [BSP tools](bsp-tools.md) (the BSP Decompiler)
- [Protect your BSP](bsp-protection.md)
- [Map assets](map-assets.md)
