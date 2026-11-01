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

## Tips

- Keep **Break decompilers** and **Material randomizer** on: they cost nothing in game.
- Put your name or your community in **Watermark**: it is the proof that a stolen copy is yours.
- Test the shipped copy on a listen server and the server copy on your dedicated server before publishing.

## FAQ

**Does the protected map still look and collide right?** Yes. Only server side logic and decompile data are removed; geometry, lighting, props and content stay.

**Can I protect a map for another game?** No. Build and save as content makes Garry's Mod addons; the lock guard is Garry's Mod Lua.

**I lost my server key.** Set a new key, build again, and put the new key in `server.cfg`. Players need the new release.

**What happens when someone opens the protected BSP in Hammer Pro?** **File > Open compiled map (BSP)...** reports brushes without faces (*it was protected against decompiling*); the logic and the wiring are gone.

## Related pages

- [Build and save as content](build-content-release.md)
- [BSP tools](bsp-tools.md)
- [Collect content for a release](../guides/collect-content-for-release.md)

---
[Back to the documentation index](../README.md)
