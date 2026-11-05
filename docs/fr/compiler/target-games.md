[Documentation](../README.md) > Compilateur > Jeux cibles | [English](../../en/compiler/target-games.md) | [Русский](../../ru/compiler/target-games.md)

# Jeux cibles

Hammer Pro est d'abord fait pour Garry's Mod, mais son compilateur écrit les formats BSP de plusieurs branches du moteur Source. Le jeu cible décide de la version du BSP, du format des props statiques et des lumières du monde, des limites vérifiées et des passes d'éclairage par défaut.

## Choisir le jeu

- **Paramètres > Jeu** choisit le jeu avec lequel travaille l'éditeur (contenu, FGD). Voir [Configuration du jeu](../getting-started/game-setup.md).
- La ligne **Jeu cible** de la [fenêtre Compiler et lancer](compile-window.md) choisit le jeu de la map ouverte (retenu dans son fichier compagnon). Choisir un jeu installé monte son contenu et ses FGD. Un jeu non installé compile quand même (avec le contenu monté à ce moment, dans son format) mais la map n'est ni copiée ni lancée.
- En ligne de commande : `-target <id>` sur `--bspc` et `--rad` (voir [Ligne de commande](command-line.md)). Sans `-target`, c'est le jeu du dossier `-game` (Garry's Mod pour `garrysmod` ou sans dossier).

Les jeux installés sont trouvés dans les bibliothèques Steam (`appmanifest_*.acf`, `steamapps/common`, `steamapps/sourcemods`).

## Jeux pris en charge

| Id | Jeu | Branche du moteur | Version BSP | Éclairage par défaut |
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
| `swarm` | Alien Swarm | Alien Swarm | 21 | HDR seul |
| `nucleardawn` | Nuclear Dawn | Alien Swarm | 21 | HDR seul |
| `portal2` | Portal 2 | Portal 2 | 21 | HDR seul |
| `csgo` | Counter-Strike: Global Offensive (legacy) | CS:GO | 21 | HDR seul |
| `insurgency` | Insurgency | Insurgency | 21 | HDR seul |
| `doi` | Day of Infamy | Insurgency | 21 | HDR seul |

« Éclairage par défaut » est ce que contiennent les maps du jeu lui-même. Pour les jeux uniquement HDR, choisir **LDR** dans la fenêtre de compilation calcule du HDR à la place (le jeu n'affiche que des lightmaps HDR).

## Ce qui change d'une branche à l'autre

| Branche | BSP | Props statiques (`sprp`) | Lumières du monde | Limites |
|---|---|---|---|---|
| Garry's Mod | 20 | v10 (72 octets) | v0 (88 octets) | Garry's Mod |
| Source 2013 | 20 | v10 (72) | v0 | Source SDK 2013 |
| Source 2007 | 20 | v6 (64) | v0 | Source SDK 2013 |
| Black Mesa | 20 | v11 (80) | v0 | Source SDK 2013 |
| The Ship | 20 | v5 + targetname (188) | v0 | Source SDK 2013 |
| Bloody Good Time | 20 | v6 + targetname (192) | v0 | Source SDK 2013 |
| Left 4 Dead | 20 | v8 (68) | v0 | Source SDK 2013 |
| Left 4 Dead 2 | 21 (table des lumps avec la version en premier) | v9 (72) | v0 | Source SDK 2013 |
| Alien Swarm, Nuclear Dawn | 21 | v9 (72) | v1 (100 octets) | Source SDK 2013 |
| Portal 2 | 21 | v9 (72) | v1 (100) | Source SDK 2013 |
| CS:GO | 21 | v11 CS:GO (80) | v1 (100) | Source SDK 2013 |
| Insurgency, Day of Infamy | 21 | v10 CS:GO (76) | v1 (100) | Source SDK 2013 |

Tout le reste (feuilles, faces, côtés de brushes, infos de texture, displacements, overlays, cubemaps, props de détail, lumps d'éclairage) est identique partout. Les branches d'Alien Swarm (Alien Swarm, Nuclear Dawn, Portal 2, CS:GO, Insurgency) reçoivent en plus le lump 61, les niveaux CPU et GPU des overlays. Garry's Mod accepte des lightmaps jusqu'à 125 luxels par côté de face (`-maxlightmapdim`), les autres jeux 32. Les limites de Garry's Mod sont plus hautes que celles de Valve (voir [Limites et journaux](limits-and-logs.md)).

## Mods et autres dossiers de jeu

- Un mod de `steamapps/sourcemods` reçoit l'id `mod:<dossier>` et la branche du jeu désigné par son `SteamAppId` (Source 2013 si ce jeu est inconnu).
- Tout autre dossier de jeu choisi reçoit l'id `dir:<dossier>`, lu de la même façon.
- Un mod Mapbase est reconnu d'après son `gameinfo.txt` : les cubemaps à correction de parallaxe (`parallax_obb`) y fonctionnent.
- Le contenu est monté exactement comme le moteur lit `gameinfo.txt` (chemins de recherche dans l'ordre, VPK, dossiers `custom/*`, et à partir de Left 4 Dead le `pak01_dir.vpk` de chaque dossier et les dossiers de DLC).

## Non pris en charge

Ces jeux sont connus mais le compilateur ne peut pas écrire de maps pour eux : Counter-Strike 2 et Dota 2 (Source 2, les maps ne sont pas des fichiers BSP), Contagion (BSP version 27, non documentée), Dark Messiah of Might and Magic, Dear Esther et Zeno Clash (props statiques ou autres enregistrements à eux, non documentés), Vampire: The Masquerade - Bloodlines (BSP version 17), Vindictus (un format BSP à lui), SiN Episodes: Emergence et Source SDK Base 2006 (lumps d'éclairage plus anciens que les formats écrits par ce compilateur). En choisir un arrête la compilation avec la raison.

## FAQ

**Peut-on faire une map TF2 avec Hammer Pro ?** Oui : choisissez Team Fortress 2 comme jeu cible. La map prend le format de TF2 et les limites de Valve, et elle est copiée dans le dossier `maps` de TF2 s'il est installé.

**Pourquoi Portal 2 compile-t-il en HDR même avec LDR choisi ?** Portal 2 et les jeux de sa branche n'ont pas de lightmaps LDR.

**La visibilité ou l'éclairage changent-ils selon le jeu ?** Non, seulement le format du fichier, les limites et les passes d'éclairage par défaut.

## Pages liées
- [Configuration du jeu](../getting-started/game-setup.md)
- [Porter une map vers un autre jeu](../guides/port-to-another-game.md)
- [Limites et journaux](limits-and-logs.md)
- [Ligne de commande](command-line.md)

---
[Retour à l'index de la documentation](../README.md)
