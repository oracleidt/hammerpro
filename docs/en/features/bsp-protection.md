[Documentation](../README.md) > Features > Protect your BSP | [Français](../../fr/features/bsp-protection.md)

# Protect your BSP

Protect your BSP ships a protected copy of your Garry's Mod map to players and keeps the full map for your server. The client needs only the geometry and the rendering to draw the map, while every bit of gameplay (entity logic, inputs and outputs, triggers) runs on the server. So the copy players download can drop all of that, carry a watermark, and be much harder to decompile into something usable.

A map a client can draw is never fully theft proof: a decompiler can always recover some geometry. This makes a stolen copy far less useful and traceable, without breaking the map.

## Opening it

The protection is part of the release build. Open **File > Build and save as content...**, tab **Build**, and tick **Protect your BSP** (below the release options). Its options appear indented under it. The next **Build** produces the protected release. See [Build and save as content](build-content-release.md) for the rest of the window.

## Options

| Option | What it does | Default |
| --- | --- | --- |
| **Protect your BSP** | Ship a protected copy of the map: gameplay logic, entity wiring and decompile data removed, watermarked. Keeps the full map for your server. Key `content.protect`. | off |
| **Watermark** | Text written into the protected map (your name or server), traceable in a stolen copy. Stored on the world entity as `hammerpro_protected`; control characters are removed and the text is cut at 120 characters. Key `content.protect_watermark`. | empty |
| **Break decompilers (empty original faces)** | Empties the original faces lump: decompilers use it, the game does not. Leave on. Key `content.protect_strip_original_faces`. | on |
| **Material randomizer (break decompilers like bsp_iidefeat)** | Renames the map's materials to garbage names in the shipped copy, patched so it renders the same, so a recovered map has unusable material names. Leave on. Key `content.protect_randomize`. | on |
| **Shuffle the lump order in the file** | Every lump keeps its id but gets a random place in the file, different at each compile. Key `content.protect_shuffle_lumps`. | on |
| **Random junk gaps between lumps** | Gaps of random size filled with random bytes (4 byte aligned) between the lumps. Key `content.protect_junk_gaps`. | on |
| **Decoy data blocks** | Blocks no lump entry points to: random data and stale, slightly altered copies of real lumps. Key `content.protect_decoys`. | on |
| **Scramble header fields the game ignores** | Random map revision, random versions of lumps whose version the game ignores, random offset, version and marker of unused directory entries. Key `content.protect_scramble_header`. | on |
| **Shuffle the game lump directory** | Static props, detail props and the other game lumps in a random order. Key `content.protect_shuffle_game_lumps`. | on |
| **Pakfile at a random position** | The embedded content archive is no longer always the last lump. Key `content.protect_random_pak`. | on |
| **Reorder planes, vertices, edges and texture tables** | Permutes planes, vertexes, edges, texinfo and texdata and renumbers every reference. Key `content.protect_reorder_tables`. | on |
| **Reorder brushes, faces and leaf lists** | Permutes brushes with their sides, faces inside each node, and the face and brush lists of each leaf, renumbering every reference. Key `content.protect_reorder_world`. | on |
| **Layout seed** | A text that fixes the scrambling: the same seed on the same map gives the same file. Empty gives a new layout at each compile. Key `content.protect_layout_seed`. | empty |
| **Server lock** | The map plays normally only on a server that sets `hammerpro_key` to your key. Elsewhere the shipped guard shows a watermark (and blocks play when asked). A deterrent, not unbreakable. Key `content.protect_lock`. | off |
| **Server key** | Your secret key for the lock (hidden field). The key itself is never written into the map, only its hash. Key `content.protect_lock_key`. | empty |
| **Block play on an unauthorized server (not just a watermark)** | Freezes the local player and shows a notice when the server key is missing or wrong. Key `content.protect_lock_block`. | off |

Without a key, the lock does nothing and the window says *Set a key: you put the same value in your server.cfg (hammerpro_key).*

## What the protected copy loses

- **Logic entities** are removed from the entity lump: every class starting with `logic_`, `math_`, `filter_`, `game_`, `ai_`, `npc_`, `scripted_`, `phys_`, `comp_`, `trigger_`, `weapon_`, `item_`, and `point_template`, `point_servercommand`, `point_clientcommand`, `env_microphone`, `env_entity_maker`, `info_node`, `info_target`, `path_track`, `path_corner`, `func_instance`. Brush entities that draw something keep their geometry.
- **Every input/output connection** is stripped from the entities that stay.
- **Editor keys** that reveal how the map works are dropped: `hammerid`, `comment`, `_comment`, `parentname`, `classname_original`.
- **The original faces lump** is emptied (option above).
- **Material names** are scrambled (option above). Materials the engine recognizes by name (tool and sky textures, the cubemap patched materials under `maps/`) are never renamed.
- **The navigation mesh** is kept for the server only: it maps out the whole map and the client never reads it.

The build log reports *Protect your BSP: N logic entities removed, N connections stripped, size of decompile data emptied* and, with the randomizer, *Material randomizer: N materials renamed, patched to render the same.*

## The structure scrambler

Besides removing data, the protection can scramble how the file is built, so a tool that parses the BSP by position or by guessing lump boundaries finds a different layout at each compile. Every item below is its own option (under **Structure scrambler** in the window) and all of them are on by default. The game reads lumps through the header directory and renumbered references, so the map loads and plays the same.

- **Lump order, gaps and decoys.** The lump blocks are written in a random physical order, with gaps of random bytes between them (always 4 byte aligned, as the engine requires) and with decoy blocks that no directory entry points to: random data, and stale copies of real lumps (planes, faces, entities...) with a few bytes altered. Decoys add at most about an eighth of the file size.
- **Header.** The map revision, the version of lumps whose version the game ignores (planes, vertexes, nodes, texinfo, edges, brushes, the pakfile and similar), and the offset, version and marker of unused directory entries are random. The versions that the game reads (leafs, lighting, occlusion, overlays, game lumps, the HDR lumps) and the compression markers of used lumps are never touched.
- **Game lumps and pakfile.** The game lump directory is shuffled, and the pakfile can sit anywhere in the file instead of at the end.
- **Order inside lumps.** Planes (in pairs, so the plane and its opposite stay together), vertexes, edges, texinfo and texdata are permuted and every reference to them is renumbered (nodes, faces, brush sides, area portals, occluders, overlays, displacements, water data). Brushes are permuted with their sides, faces are permuted inside the range of each node, and the face and brush lists of each leaf are shuffled. Before any of these changes the references are checked: if one cannot be renumbered with certainty (a value out of range, overlapping ranges, an unknown lump size), that permutation is skipped and the data stays as it was. The log counts the permutations done.
- **Seed.** An empty seed gives a new layout at each compile. A text in **Layout seed** makes the result repeatable: same map, same options and same seed give the same bytes.

The log reports *Structure scrambler: N tables reordered, N decoy blocks, size of decoys and junk, seed ...*. The server copy keeps the same lump contents as the protected copy (the game compares them), so players join normally; it is not scrambled itself because it is never shipped. Decoys make the file bigger by up to a few megabytes on large maps: turn **Decoy data blocks** off if the size matters more to you.

## The server copy

The protection runs after packing, so both copies hold the same content. The output folder gets a `server` folder:

```text
<output>/server/README.txt
<output>/server/garrysmod/maps/<map>.bsp   (full map, every entity, key and connection)
<output>/server/garrysmod/maps/<map>.nav   (when the map has a navigation mesh)
```

Copy `server/garrysmod` into your server and install the addon (the GMA or the Workshop item) there too: it carries the map content and its scripts. The two copies differ only in their entities, which the game does not compare when a player joins, so players with the protected copy can join your server. Do not redistribute the server copy. `README.txt` repeats these steps in the language of the interface.

## The server lock

With **Server lock** on and a key set:

1. The release adds the guard `lua/autorun/hammerpro_lock.lua` to the addon (*The server lock guard (hammerpro_lock) is added to the addon.*).
2. The map stores only the hash of your key (`hammerpro_lock`) and whether to block (`hammerpro_lock_block`).
3. On your server, add this line to `server.cfg`, with the key you gave Hammer Pro:

```text
hammerpro_key "your-secret-key"
```

On a server without the right key, the guard shows the watermark, and with **Block play on an unauthorized server** it also freezes the player and shows a notice. The hash hides the key from a glance at the map; it is not cryptography, so use a long key nobody can guess.

## What the protection does not hide

The game must load and draw the map, so everything it draws is in the memory of the player's game, whatever the file looks like. The [BSP Sniffer](bsp-sniffer.md) reads that memory and rebuilds the brushes, displacements (with alphas, when the protected file is also at hand), static props, overlays, cubemaps, area portals, the pakfile and the entities that stay, and it restores the scrambled material names of the stock materials from the patch materials in the pakfile. Tested on a map with every option on, the recovered VMF has all the solids, all the displacements and props, all the overlays and the stock material names. What stays lost: the logic entities and the connections that were removed, the editor keys, the custom material names (replaced by a hash with a random salt) and the texture axes of a handful of sides that only the original faces held.

So the protection stops the logic from being copied (it is not in the client) and makes a stolen copy traceable, but it does not make the geometry and the content theft proof. Keep what must stay secret on the server.

## Tips

- Keep **Break decompilers** and **Material randomizer** on: they cost nothing in game.
- Leave the **Structure scrambler** options on and test the shipped copy once in game: they cost nothing at run time.
- Put your name or your community in **Watermark**: it is the proof that a stolen copy is yours.
- Test the shipped copy on a listen server and the server copy on your dedicated server before publishing.

## FAQ

**Does the protected map still look and collide right?** Yes. Only server side logic and decompile data are removed; geometry, lighting, props and content stay.

**Can I protect a map for another game?** No. Build and save as content makes Garry's Mod addons; the lock guard is Garry's Mod Lua.

**I lost my server key.** Set a new key, build again, and put the new key in `server.cfg`. Players need the new release.

**Can someone rebuild my map with the BSP Sniffer?** The geometry and the content that the client draws, yes (see above); the logic, the wiring and the original names of your custom materials, no.

**What happens when someone opens the protected BSP in Hammer Pro?** **File > BSP Decompiler...** reports brushes without faces (*it was protected against decompiling*); the logic and the wiring are gone.

## Related pages

- [Build and save as content](build-content-release.md)
- [BSP tools](bsp-tools.md)
- [Collect content for a release](../guides/collect-content-for-release.md)

---
[Back to the documentation index](../README.md)
