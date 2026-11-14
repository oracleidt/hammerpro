[Documentation](../README.md) > Référence > Ligne de commande | [English](../../en/reference/command-line.md) | [Русский](../../ru/reference/command-line.md)

# Ligne de commande

`hammerpro.exe` regroupe l'éditeur, le compilateur de maps et le pont MCP dans un seul programme. Cette page liste tous les arguments qu'il accepte.

## Syntaxe

```text
hammerpro [map.vmf] [--lang en|fr|ru] [--size LxH] [--screenshot sortie.png] [--frames N]
          [--maximize-view 0..3] [--tool NOM] [--script fichier.txt] [--bench-open map.vmf]
hammerpro --mcp
hammerpro --bspc [options du compilateur] <map.vmf>
hammerpro --rad [options de l'éclairage] <map.bsp>
hammerpro --import-models FICHIER_OU_DOSSIER... --out DOSSIER [options]
```

`--mcp`, `--bspc`, `--rad`, `--import-models` et `--light-preview` doivent être le **premier** argument : ils lancent un mode sans fenêtre. Tous les autres arguments concernent l'éditeur et peuvent venir dans n'importe quel ordre.

`--no-save-settings` charge les paramètres habituels mais conserve les changements de préférences en mémoire pour cette session. L'édition, l'annulation et l'enregistrement des maps restent disponibles. Cette option permet de tester temporairement l'échelle de l'interface ou les réglages du rendu.

## Arguments de l'éditeur

| Argument | Effet | Par défaut |
|---|---|---|
| `map.vmf` | Tout argument qui ne commence pas par `--` est la map à ouvrir au démarrage (comme **Fichier > Ouvrir une map...**). S'il y en a plusieurs, c'est le dernier qui compte. | aucune (map vide) |
| `--lang en` / `--lang fr` / `--lang ru` | Langue de l'interface pour cette session uniquement. La langue enregistrée dans les paramètres ne change pas. Toute autre valeur est ignorée. | la langue enregistrée |
| `--size LxH` | Taille du contenu de la fenêtre en points, par exemple `--size 1920x1080`. La marge du cadre de fenêtre s'ajoute autour, et la taille minimale est 960x600. | `1600x960` |
| `--screenshot sortie.png` | Exécution automatisée : dessine N images (voir `--frames`), enregistre un PNG de toute la fenêtre dans le fichier donné puis se ferme. | désactivé |
| `--frames N` | Nombre d'images dessinées avant la capture de `--screenshot` (au moins 2). | `90` |
| `--maximize-view N` | Démarre avec une vue agrandie. N est l'index de la vue : `0` 3D, `1` Dessus, `2` Face, `3` Côté dans la disposition par défaut. | aucune |
| `--tool NOM` | Outil actif au démarrage. Noms (majuscules ignorées) : `select`, `camera`, `entity`, `block`, `texture`, `decal`, `overlay`, `clip`, `vertex`, `path`, `polygon`, `curve`, `sprinkle`, `displacement`, `cordon`. Un nom inconnu est ignoré. | `select` |
| `--script fichier.txt` | Exécution automatisée : joue les commandes d'un fichier script (souris, touches, actions, captures, vérifications). Voir [Commandes de script](script-commands.md). | désactivé |
| `--bench-open map.vmf` | Exécution automatisée : ouvre la map, attend qu'elle soit dessinée et que le contenu du jeu soit chargé, affiche sur stdout les temps de démarrage et d'ouverture sous forme de paires `clé=valeur` puis se ferme. | désactivé |
| `--backend vulkan` / `--backend dx12` / `--backend auto` | API graphique pour cette session seulement, à la place du réglage `render.graphics_backend` (**Paramètres > Rendu > Performance > API graphique**). Si aucune carte graphique ne la propose, le choix automatique est utilisé. `WGPU_BACKEND` reste prioritaire. Voir [Performances de l'éditeur](../troubleshooting/performance.md#premier-démarrage-lent-shaders). | le réglage enregistré (`Auto`) |
| `--restore-session liste.txt` | Ouvre les maps d'une liste écrite par Hammer Pro quand il redémarre après la perte du périphérique graphique (un onglet par map, les maps modifiées depuis leurs modifications sauvegardées), puis supprime la liste. Voir [Périphérique graphique perdu](../troubleshooting/graphics-device-lost.md). | désactivé |

Un argument inconnu commençant par `--` affiche `unknown argument: ...` sur stderr et il est ignoré.

## Modes sans fenêtre

| Argument | Effet |
|---|---|
| `--mcp` | Le pont MCP que les applications d'IA lancent elles-mêmes. Il n'ouvre aucune fenêtre et transmet les appels d'outils à une fenêtre Hammer Pro dont le serveur MCP est actif. stdout ne transporte que des messages MCP. Voir [Serveur MCP](mcp-server.md). |
| `--bspc [options] <map>` | Le compilateur de maps de Hammer Pro (géométrie, portails, test de fuite et visibilité), avec les arguments de VBSP (`-game`, `-leaktest`...). Il écrit `<map>.bsp` à côté de la map, `<map>.lin` quand la map fuit, `<map>.prt` pour une map étanche, et un `<map>.log`. |
| `--rad [options] <map>` | L'éclairage de Hammer Pro, avec les arguments de VRAD (`-ldr`, `-hdr`, `-both`, `-final`, `-fast`, `-bounce N`...) plus les options propres à Hammer Pro (`-nostaticpropshadows`, `-nodispshadows`...). Il éclaire `<map>.bsp` sur place. |
| `--import-models FICHIER_OU_DOSSIER... --out DOSSIER [options]` | Conversion par lot de fichiers OBJ, FBX, glTF et GLB en modèles Garry's Mod, sans fenêtre ni jeu (voir « Import par lot » dans [Import de modèles](../features/model-import.md#import-par-lot)). |
| `--light-preview ...` | Interne : l'éclairage de l'aperçu précalculé, lancé par l'éditeur lui-même. Vous n'avez jamais besoin de le lancer. |
| `--apply-update <zip> <programme> <pid> [<sha256>]` | Interne : l'assistant de mise à jour que l'éditeur lance depuis une copie de lui-même quand vous cliquez sur **Mettre à jour**. Il n'a pas de fenêtre : il attend la fin du processus `<pid>`, vérifie que `<zip>` a toujours la somme de contrôle SHA-256 `<sha256>` du téléchargement vérifié (quand elle est donnée), extrait `hammerpro.exe` de `<zip>`, remplace `<programme>` (en gardant `<programme>.old`, avec retour en arrière en cas d'échec) et le lance. Voir [Mises à jour](../features/updates.md). |

Toutes les options de `--bspc` et `--rad`, avec leur effet et leur valeur par défaut, sont décrites dans [Ligne de commande du compilateur](../compiler/command-line.md).

## Import de modèles par lot

```text
hammerpro --import-models FICHIER_OU_DOSSIER... --out DOSSIER [options]
```

Doit être le **premier** argument. Chaque fichier ou dossier est converti en `DOSSIER/models/hammerpro/<nom>.mdl` (avec `.vvd`, `.dx90.vtx` et `.phy`) et ses textures en `DOSSIER/materials/models/hammerpro/<nom>/`. Rien n'est lu ni écrit dans le jeu. Les réglages enregistrés donnent les valeurs par défaut des niveaux de détail et des paramètres de matériaux.

| Option | Effet | Défaut |
|---|---|---|
| `--out DOSSIER` | Dossier de sortie (obligatoire, sauf avec `--dry-run`). | aucun |
| `--recursive`, `-r` | Cherche dans les dossiers récursivement. | désactivé |
| `--formats LISTE` | Formats pris dans les dossiers : `obj`, `fbx`, `gltf` (aussi `glb`), séparés par des virgules. | tous |
| `--template T` | Modèle de nom avec `{name}`, `{folder}` et `{index}`. | `{name}` |
| `--prefix P` | Texte placé devant chaque nom. | aucun |
| `--scale S` | Échelle supplémentaire, 0,001 à 1000. | `1` |
| `--no-units` | Une unité du fichier vaut une unité Source (pas de conversion des mètres). | désactivé |
| `--up auto\|y\|z` | Axe vertical des fichiers. | `auto` |
| `--no-center` | Garde l'origine du fichier au lieu du bas au centre. | désactivé |
| `--kind static\|physics` | `prop_static` ou `prop_physics`. | `static` |
| `--surface NOM` | Propriété de surface. | `default` |
| `--mass KG` | Masse des props physiques, 0,1 à 50000. | `20` |
| `--collision pieces\|box\|none` | Modèle de collision. | `pieces` |
| `--no-lod`, `--lod-levels N`, `--lod-keep PCT` | Niveaux de détail : désactivés, 3 à 6 niveaux, 20 à 90 % gardés par niveau. | réglages enregistrés |
| `--no-material-params` | Couleur, alpha et double face seulement dans les VMT. | réglage enregistré |
| `--overwrite` | Remplace les modèles qui existent (ignorés par défaut). | désactivé |
| `--threads N` | Fichiers convertis en même temps. | moitié des threads du processeur, 1 à 4 |
| `--dry-run` | Lit seulement les fichiers et affiche triangles, taille, nom et avertissements. | désactivé |
| `--quiet`, `-q` | N'affiche que la synthèse. | désactivé |

Une ligne est affichée par fichier (`OK`, `Warning`, `Error` ou `Skipped`, le fichier, le nom du modèle, la raison), puis une synthèse. Codes de sortie : `0` chaque fichier converti ou ignoré, `1` au moins un fichier en échec ou aucun fichier trouvé, `2` une option erronée (le message et une ligne d'usage sont affichés).

## Exécutions automatisées

`--script`, `--screenshot` et `--bench-open` lancent une exécution automatisée. Elle est faite pour ne jamais toucher à vos fichiers ni à votre jeu :

- Elle part des **paramètres par défaut** et n'écrit jamais `settings.toml`. L'assistant de premier lancement n'apparaît pas.
- La sauvegarde automatique est coupée, la map compilée n'est pas copiée dans le dossier `maps` du jeu et le jeu n'est jamais lancé (compilation avec lancement, session de jeu, publication : refusés avec le message "skipped: automated runs never write into the game folder or start the game").
- Pas de fichier log, pas de rapport de plantage, pas de proposition de récupération et pas de présence Discord et pas de recherche de mise à jour (sauf si `HP_UPDATE_API_URL` est définie). Le cache des miniatures et les fichiers annexes de chaque map restent en mémoire.
- Les cadres de fenêtre natifs ne sont pas utilisés (le cadre de Hammer Pro est dessiné).

Codes de sortie : `0` fin normale, `2` le fichier script est illisible ou une ligne ne peut pas être lue (l'erreur indique la ligne), `3` une commande `assert_selected` ou `assert_tabs` a échoué.

## Variables d'environnement

| Variable | Effet |
|---|---|
| `WGPU_BACKEND` | Force l'API graphique, par exemple `dx12`, `vulkan` ou `gl`. Pratique quand celle par défaut pose problème avec votre carte graphique. |
| `WGPU_DX12_COMPILER` | Compilateur de shaders DirectX 12 : `dxc` (demande `dxcompiler.dll`) ou `fxc`. Remplace le réglage `render.dx12_compiler` (FXC par défaut ; DXC seulement s'il est activé). |
| `HAMMERPRO_INSTANCE` | Pour `--mcp` : l'identifiant de processus de la fenêtre Hammer Pro que le pont doit utiliser. |
| `HP_UPDATE_API_URL` | Remplace l'adresse GitHub des versions pour la recherche de mise à jour (tests). Voir [Mises à jour](../features/updates.md). |
| `HP_UPDATE_CURRENT_VERSION` | Fait comme si une autre version était installée (tests de la mise à jour). |
| `HAMMERPRO_MCP_DIR` | Déplace les fiches d'instance MCP (tests et vérifications automatisées). |

## Exemples

```bat
rem Ouvrir une map en français
hammerpro.exe "C:\maps\gm_mamap.vmf" --lang fr

rem Capture de l'éditeur avec l'outil bloc, vue 3D agrandie
hammerpro.exe gm_mamap.vmf --tool block --maximize-view 0 --screenshot capture.png --frames 120

rem Compiler une map depuis un fichier batch
hammerpro.exe --bspc -game "C:\...\GarrysMod\garrysmod" "C:\maps\gm_mamap.vmf"
hammerpro.exe --rad -both -final -game "C:\...\GarrysMod\garrysmod" "C:\maps\gm_mamap.bsp"
```

## Pages liées

- [Commandes de script](script-commands.md)
- [Serveur MCP](mcp-server.md)
- [Ligne de commande du compilateur](../compiler/command-line.md)
- [Fichiers et dossiers](files-and-folders.md)
- [Thèmes et langues](../features/themes-languages.md)

---
[Retour à l'index de la documentation](../README.md)
