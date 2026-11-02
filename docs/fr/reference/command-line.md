[Documentation](../README.md) > Référence > Ligne de commande | [English](../../en/reference/command-line.md)

# Ligne de commande

`hammerpro.exe` regroupe l'éditeur, le compilateur de maps et le pont MCP dans un seul programme. Cette page liste tous les arguments qu'il accepte.

## Syntaxe

```text
hammerpro [map.vmf] [--lang en|fr] [--size LxH] [--screenshot sortie.png] [--frames N]
          [--maximize-view 0..3] [--tool NOM] [--script fichier.txt] [--bench-open map.vmf]
hammerpro --mcp
hammerpro --bspc [options du compilateur] <map.vmf>
hammerpro --rad [options de l'éclairage] <map.bsp>
```

`--mcp`, `--bspc`, `--rad` et `--light-preview` doivent être le **premier** argument : ils lancent un mode sans fenêtre. Tous les autres arguments concernent l'éditeur et peuvent venir dans n'importe quel ordre.

## Arguments de l'éditeur

| Argument | Effet | Par défaut |
|---|---|---|
| `map.vmf` | Tout argument qui ne commence pas par `--` est la map à ouvrir au démarrage (comme **Fichier > Ouvrir une map...**). S'il y en a plusieurs, c'est le dernier qui compte. | aucune (map vide) |
| `--lang en` / `--lang fr` | Langue de l'interface pour cette session uniquement. La langue enregistrée dans les paramètres ne change pas. Toute autre valeur est ignorée. | la langue enregistrée |
| `--size LxH` | Taille du contenu de la fenêtre en points, par exemple `--size 1920x1080`. La marge du cadre de fenêtre s'ajoute autour, et la taille minimale est 960x600. | `1600x960` |
| `--screenshot sortie.png` | Exécution automatisée : dessine N images (voir `--frames`), enregistre un PNG de toute la fenêtre dans le fichier donné puis se ferme. | désactivé |
| `--frames N` | Nombre d'images dessinées avant la capture de `--screenshot` (au moins 2). | `90` |
| `--maximize-view N` | Démarre avec une vue agrandie. N est l'index de la vue : `0` 3D, `1` Dessus, `2` Face, `3` Côté dans la disposition par défaut. | aucune |
| `--tool NOM` | Outil actif au démarrage. Noms (majuscules ignorées) : `select`, `camera`, `entity`, `block`, `texture`, `decal`, `overlay`, `clip`, `vertex`, `path`, `polygon`, `curve`, `sprinkle`, `displacement`, `cordon`. Un nom inconnu est ignoré. | `select` |
| `--script fichier.txt` | Exécution automatisée : joue les commandes d'un fichier script (souris, touches, actions, captures, vérifications). Voir [Commandes de script](script-commands.md). | désactivé |
| `--bench-open map.vmf` | Exécution automatisée : ouvre la map, attend qu'elle soit dessinée et que le contenu du jeu soit chargé, affiche sur stdout les temps de démarrage et d'ouverture sous forme de paires `clé=valeur` puis se ferme. | désactivé |
| `--restore-session liste.txt` | Ouvre les maps d'une liste écrite par Hammer Pro quand il redémarre après la perte du périphérique graphique (un onglet par map, les maps modifiées depuis leurs modifications sauvegardées), puis supprime la liste. Voir [Périphérique graphique perdu](../troubleshooting/graphics-device-lost.md). | désactivé |

Un argument inconnu commençant par `--` affiche `unknown argument: ...` sur stderr et il est ignoré.

## Modes sans fenêtre

| Argument | Effet |
|---|---|
| `--mcp` | Le pont MCP que les applications d'IA lancent elles-mêmes. Il n'ouvre aucune fenêtre et transmet les appels d'outils à une fenêtre Hammer Pro dont le serveur MCP est actif. stdout ne transporte que des messages MCP. Voir [Serveur MCP](mcp-server.md). |
| `--bspc [options] <map>` | Le compilateur de maps de Hammer Pro (géométrie, portails, test de fuite et visibilité), avec les arguments de VBSP (`-game`, `-leaktest`...). Il écrit `<map>.bsp` à côté de la map, `<map>.lin` quand la map fuit, `<map>.prt` pour une map étanche, et un `<map>.log`. |
| `--rad [options] <map>` | L'éclairage de Hammer Pro, avec les arguments de VRAD (`-ldr`, `-hdr`, `-both`, `-final`, `-fast`, `-bounce N`...) plus les options propres à Hammer Pro (occlusion ambiante `-ao`, `-aoradius`...). Il éclaire `<map>.bsp` sur place. |
| `--light-preview ...` | Interne : l'éclairage de l'aperçu précalculé, lancé par l'éditeur lui-même. Vous n'avez jamais besoin de le lancer. |

Toutes les options de `--bspc` et `--rad`, avec leur effet et leur valeur par défaut, sont décrites dans [Ligne de commande du compilateur](../compiler/command-line.md).

## Exécutions automatisées

`--script`, `--screenshot` et `--bench-open` lancent une exécution automatisée. Elle est faite pour ne jamais toucher à vos fichiers ni à votre jeu :

- Elle part des **paramètres par défaut** et n'écrit jamais `settings.toml`. L'assistant de premier lancement n'apparaît pas.
- La sauvegarde automatique est coupée, la map compilée n'est pas copiée dans le dossier `maps` du jeu et le jeu n'est jamais lancé (compilation avec lancement, session de jeu, publication : refusés avec le message "skipped: automated runs never write into the game folder or start the game").
- Pas de fichier log, pas de rapport de plantage, pas de proposition de récupération et pas de présence Discord. Le cache des miniatures et les fichiers annexes de chaque map restent en mémoire.
- Les cadres de fenêtre natifs ne sont pas utilisés (le cadre de Hammer Pro est dessiné).

Codes de sortie : `0` fin normale, `2` le fichier script est illisible ou une ligne ne peut pas être lue (l'erreur indique la ligne), `3` une commande `assert_selected` ou `assert_tabs` a échoué.

## Variables d'environnement

| Variable | Effet |
|---|---|
| `WGPU_BACKEND` | Force l'API graphique, par exemple `dx12`, `vulkan` ou `gl`. Pratique quand celle par défaut pose problème avec votre carte graphique. |
| `HAMMERPRO_INSTANCE` | Pour `--mcp` : l'identifiant de processus de la fenêtre Hammer Pro que le pont doit utiliser. |
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
