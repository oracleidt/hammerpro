[Documentation](../README.md) > Compiler > Target games | [Français](../../fr/compiler/target-games.md) | [Русский](../../ru/compiler/target-games.md)

# Target games

Hammer Pro is made for Garry's Mod first, but its compiler writes the BSP formats of several Source engine branches. The target game decides the BSP version, the static prop records, the world light records, the limits checked and the default lighting passes.

## Choosing the game

- **Settings > Game** chooses the game the editor works with (content, FGDs). See [Game setup](../getting-started/game-setup.md).
- The **Target game** row of the [Compile and run window](compile-window.md) chooses the game of the open map (kept in its sidecar file). Picking an installed game mounts its content and FGDs. A game that is not installed still compiles (with the content mounted now, in its format) but the map is not copied nor launched.
- On the command line: `-target <id>` on both `--bspc` and `--rad` (see [Command line](command-line.md)). Without `-target`, the game of the `-game` folder is used (Garry's Mod for `garrysmod` or no folder).

Installed games are found in the Steam libraries (`appmanifest_*.acf`, `steamapps/common`, `steamapps/sourcemods`).

## Supported games

| Id | Game | Engine branch | BSP version | Default lighting |
|---|---|---|---|---|
| `gmod` | Garry's Mod | Garry's Mod | 20 | LDR + HDR |
| `hl2` | Half-Life 2 | Source 2013 | 20 | LDR + HDR |
| `episodic` | Half-Life 2: Episode One | Source 2013 | 20 | LDR + HDR |
| `ep2` | Half-Life 2: Episode Two | Source 2013 | 20 | LDR + HDR |
| `lostcoast` | Half-Life 2: Lost Coast | Source 2013 | 20 | LDR + HDR |
| `hl1` | Half-Life: Source | Source 2013 | 20 | LDR + HDR |
| `portal` | Portal | Source 2013 | 20 | LDR + HDR |
| `sdk2013sp` | Source SDK Base 2013 Singleplayer | Source 2013 | 20 | LDR + HDR |
| `synergy` | Synergy | Source 2013 | 20 | LDR + HDR |
| `hl2mp` | Half-Life 2: Deathmatch | Source 2013 | 20 | LDR + HDR |
| `cstrike` | Counter-Strike: Source | Source 2013 | 20 | LDR + HDR |
| `dod` | Day of Defeat: Source | Source 2013 | 20 | LDR + HDR |
| `tf` | Team Fortress 2 | Source 2013 | 20 | LDR + HDR |
| `sdk2013mp` | Source SDK Base 2013 Multiplayer | Source 2013 | 20 | LDR + HDR |
| `nmrih` | No More Room in Hell | Source 2013 | 20 | LDR + HDR |
| `zps` | Zombie Panic! Source | Source 2013 | 20 | LDR + HDR |
| `fof` | Fistful of Frags | Source 2013 | 20 | LDR + HDR |
| `sdk2007` | Source SDK Base 2007 | Source 2007 | 20 | LDR + HDR |
| `bms` | Black Mesa | Black Mesa | 20 | LDR + HDR |
| `ship` | The Ship | The Ship | 20 | LDR + HDR |
| `pm` | Bloody Good Time | Bloody Good Time | 20 | LDR + HDR |
| `left4dead` | Left 4 Dead | Left 4 Dead | 20 | LDR + HDR |
| `left4dead2` | Left 4 Dead 2 | Left 4 Dead 2 | 21 | LDR + HDR |
| `swarm` | Alien Swarm | Alien Swarm | 21 | HDR only |
| `nucleardawn` | Nuclear Dawn | Alien Swarm | 21 | HDR only |
| `portal2` | Portal 2 | Portal 2 | 21 | HDR only |
| `csgo` | Counter-Strike: Global Offensive (legacy) | CS:GO | 21 | HDR only |
| `insurgency` | Insurgency | Insurgency | 21 | HDR only |
| `doi` | Day of Infamy | Insurgency | 21 | HDR only |

"Default lighting" is what the game's own maps hold. For the HDR only games, choosing **LDR** in the compile window computes HDR instead (the game draws HDR lightmaps only).

## What changes between branches

| Branch | BSP | Static props (`sprp`) | World lights | Limits |
|---|---|---|---|---|
| Garry's Mod | 20 | v10 (72 bytes) | v0 (88 bytes) | Garry's Mod |
| Source 2013 | 20 | v10 (72) | v0 | Source SDK 2013 |
| Source 2007 | 20 | v6 (64) | v0 | Source SDK 2013 |
| Black Mesa | 20 | v11 (80) | v0 | Source SDK 2013 |
| The Ship | 20 | v5 + targetname (188) | v0 | Source SDK 2013 |
| Bloody Good Time | 20 | v6 + targetname (192) | v0 | Source SDK 2013 |
| Left 4 Dead | 20 | v8 (68) | v0 | Source SDK 2013 |
| Left 4 Dead 2 | 21 (version first lump table) | v9 (72) | v0 | Source SDK 2013 |
| Alien Swarm, Nuclear Dawn | 21 | v9 (72) | v1 (100 bytes) | Source SDK 2013 |
| Portal 2 | 21 | v9 (72) | v1 (100) | Source SDK 2013 |
| CS:GO | 21 | v11 CS:GO (80) | v1 (100) | Source SDK 2013 |
| Insurgency, Day of Infamy | 21 | v10 CS:GO (76) | v1 (100) | Source SDK 2013 |

Everything else (leaves, faces, brush sides, texture infos, displacements, overlays, cubemaps, detail props, lighting lumps) is the same in all of them. The Alien Swarm branches (Alien Swarm, Nuclear Dawn, Portal 2, CS:GO, Insurgency) also get lump 61, the CPU and GPU levels of the overlays. Garry's Mod takes lightmaps up to 125 luxels per face side (`-maxlightmapdim`), the other games 32. The Garry's Mod limits are higher than Valve's (see [Limits and logs](limits-and-logs.md)).

## Mods and other game folders

- A mod in `steamapps/sourcemods` gets the id `mod:<folder>` and the engine branch of the game its `SteamAppId` names (Source 2013 when that game is unknown).
- Any other game folder you pick gets the id `dir:<folder>`, read the same way.
- A Mapbase mod is recognized from its `gameinfo.txt`: parallax corrected cubemaps (`parallax_obb`) work in it.
- Content is mounted exactly like the engine reads `gameinfo.txt` (search paths in order, VPKs, `custom/*` folders, and for Left 4 Dead and later the `pak01_dir.vpk` of each folder and the DLC folders).

## Not supported

These games are known but the compiler cannot write maps for them: Counter-Strike 2 and Dota 2 (Source 2, maps are not BSP files), Contagion (BSP version 27, not documented), Dark Messiah of Might and Magic, Dear Esther and Zeno Clash (static prop or other records of their own, not documented), Vampire: The Masquerade - Bloodlines (BSP version 17), Vindictus (a BSP format of its own), SiN Episodes: Emergence and Source SDK Base 2006 (lighting lumps older than the formats this compiler writes). Choosing one stops the compile with the reason.

## FAQ

**Can I make a TF2 map with Hammer Pro?** Yes: choose Team Fortress 2 as the target game. The map uses TF2's format and Valve's limits, and is copied to TF2's `maps` folder when it is installed.

**Why does Portal 2 compile HDR even with LDR chosen?** Portal 2 and the games of its branch ship no LDR lightmaps.

**Does the visibility or lighting differ per game?** No, only the file format, the limits and the default lighting passes.

## Related pages
- [Game setup](../getting-started/game-setup.md)
- [Port a map to another game](../guides/port-to-another-game.md)
- [Limits and logs](limits-and-logs.md)
- [Command line](command-line.md)

---
[Back to the documentation index](../README.md)
