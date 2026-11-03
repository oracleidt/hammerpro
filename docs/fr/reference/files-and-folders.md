[Documentation](../README.md) > Référence > Fichiers et dossiers | [English](../../en/reference/files-and-folders.md)

# Fichiers et dossiers

Tous les fichiers et dossiers que Hammer Pro lit ou écrit en dehors de vos maps, et les fichiers qu'il écrit à côté d'elles. Les chemins sont ceux de Windows : `%APPDATA%` correspond en général à `C:\Users\<vous>\AppData\Roaming` et `%LOCALAPPDATA%` à `C:\Users\<vous>\AppData\Local`.

Les exécutions automatisées (`--script`, `--screenshot`, `--bench-open`) n'écrivent aucun des fichiers utilisateur ci-dessous : voir [Ligne de commande](command-line.md).

## Fichiers utilisateur (itinérants) : `%APPDATA%\HammerPro`

| Chemin | Contenu |
|---|---|
| `settings.toml` | Tous les paramètres, en TOML : une table par domaine (`[view]`, `[edit]`, `[ui]`, `[game]`, `[compile]`, `[mcp]`...). Seules les tables différentes des valeurs par défaut sont écrites, le fichier reste donc court. Les clés inconnues ou manquantes sont tolérées, et un fichier invalide est remplacé par les valeurs par défaut (un avertissement est inscrit au log). Son absence signifie un premier lancement : l'assistant de premier lancement s'affiche. Clés : [Référence des paramètres](settings.md). |
| `crash\` | Rapports de plantage : `crash-<horodatage>.txt` pour un plantage de l'éditeur, `crash-<horodatage>-worker.txt` pour un fil de fond qui a échoué pendant que l'éditeur continuait. `pending.txt` désigne le rapport à afficher au prochain lancement, et `running-<pid>.txt` marque une session en cours (supprimé à la fermeture normale, ce qui permet de détecter au lancement suivant une session morte sans rapport). |
| `prefabs\` | La bibliothèque de prefabs, où sont écrits les nouveaux prefabs. Un autre dossier peut être choisi dans les paramètres (`placement.prefab_library`). |
| `sprinkle\` | Vos propres définitions de sprinkle. |

## Fichiers utilisateur (locaux) : `%LOCALAPPDATA%\HammerPro`

| Chemin | Contenu |
|---|---|
| `autosave\` | Sauvegardes automatiques et sauvegardes d'urgence : `<map>_<hash>_<secondes unix>.vmf`, où le hash identifie le chemin complet de la map (deux maps de même nom ne partagent jamais leur rotation). Par défaut toutes les 5 minutes, 10 copies gardées par map (paramètres `edit.autosave_minutes` et `edit.autosave_keep`, `0` minute coupe la sauvegarde automatique). |
| `logs\hammerpro.log` | Le log de la session en cours, écrit ligne par ligne. La session précédente est gardée sous `hammerpro.old.log`. |
| `mcp\instances\<pid>.json` | La fiche de chaque fenêtre dont le serveur MCP est actif (port et jeton secret). Supprimée quand le serveur s'arrête. Voir [Serveur MCP](mcp-server.md). |
| `maps\<clé de la map>.toml` | Le fichier annexe de chaque map (voir plus bas). |
| `thumbs\` | Miniatures des matériaux et modèles longues à produire, gardées d'une session à l'autre. |
| `prefab_thumbs\` | Miniatures de la bibliothèque de prefabs. |
| `pack\` | Cache des fichiers intégrés aux maps compilées (images converties, fichiers extraits). |
| `compile_content\` | Dossier de jeu généré qui rend vos matériaux personnalisés (dossiers de textures personnalisés, images simples) visibles du compilateur. |
| `builds\` | Cache d'étapes du compilateur : une étape de géométrie ou d'éclairage identique est restaurée au lieu d'être relancée. |
| `content_index.bin` | L'index enregistré des dossiers du jeu, personnalisés et montés, pour que les lancements n'analysent que ce qui a changé (voir [Index et analyse du contenu](../features/content-index.md)). |
| `compile_history\<clé de la map>.jsonl` | Historique des temps de compilation de chaque map (onglet Historique de la fenêtre de compilation). |
| `progressive\`, `spec\` | Builds de la compilation progressive (jouer tout de suite) et de la précompilation en arrière-plan. |
| `untitled\` | Dossier de compilation des maps jamais enregistrées. |
| `release\<map>\` | Fichiers de travail de **Construire et enregistrer en contenu**. |
| `propsrc\` | Fichiers sources des modèles créés par Propper. |
| `decompiled\` | Contenu extrait des maps compilées ouvertes avec **Décompilateur de BSP...**. |
| `sniffer\` | Dossier de sortie par défaut du [BSP Sniffer](../features/bsp-sniffer.md) (VMF, BSP, contenu et rapport de chaque opération ; réglage `sniffer.output_dir`). |

Tout ce dossier, sauf `autosave\` et `logs\`, est un cache : vous pouvez le supprimer pendant que Hammer Pro est fermé, il se reconstruit au besoin (les fichiers annexes ne contiennent que des commodités, voir plus bas).

## Le fichier annexe de chaque map

Les données d'éditeur qui n'ont pas leur place dans le VMF sont gardées par map dans `%LOCALAPPDATA%\HammerPro\maps\<clé de la map>.toml`, où la clé est le nom du fichier de la map suivi d'un hash de son chemin complet (`untitled` pour une map jamais enregistrée). Il est lu à l'ouverture de la map et écrit (de façon atomique) quand la map est enregistrée et quand elle est fermée. Un fichier annexe absent ou abîmé est simplement lu comme des valeurs par défaut : ce n'est jamais la seule copie de votre travail.

| Section | Ce qu'elle retient |
|---|---|
| `startup_view` | Les caméras 3D et 2D et l'outil actif, restaurés à l'ouverture de la map. |
| `workflow` | La taille exacte de la grille (le VMF ne stocke qu'un nombre entier pour Hammer). |
| `game` | Le jeu de la map (voir [Porter une map vers un autre jeu](../guides/port-to-another-game.md)). |
| `compile` | La dernière compilation réussie de chaque build de la map, pour les recompilations intelligentes. |
| `content` | Les options de Construire et enregistrer en contenu : titre de l'addon, tags, liste d'exclusions, description, dossier de sortie, identifiant Workshop, dernière note de mise à jour, résumé de la dernière version, empreintes du nav mesh et des cubemaps, et le sel secret du randomiseur de matériaux de Protect your BSP. |
| `health_ignored` | Les problèmes que vous avez choisi d'ignorer. |
| `placement` | Le compteur du jeton de prefab `&i`. |
| `startup_export` | Le dernier export, pour "Exporter à nouveau". |

## Fichiers à côté de votre map

| Chemin | Contenu |
|---|---|
| `<map>.vmf` | Votre map. Une compilation ne l'enregistre ni ne la modifie jamais. |
| `.hammerpro_build\` | Dossier de compilation : l'instantané de la map en cours de compilation (`<map>.vmf`, ou `<map>_cordon.vmf` quand seul le cordon est compilé) et ce que le compilateur écrit à côté : `<map>.bsp`, `<map>.log`, `<map>.lin` quand la map fuit, `<map>.prt` pour une map étanche. `.hammerpro_build\portals\` contient les builds géométrie seule de **Construire les portails (géométrie seule)**. |
| `<map>.nav` | Le nav mesh généré par Hammer Pro (là où le visualiseur de nav mesh et Construire et enregistrer en contenu le cherchent). |
| `<map>_content\` | Dossier de sortie par défaut de **Construire et enregistrer en contenu** (un autre peut être choisi). |

Quand vous lancez vous-même `hammerpro --bspc`, les fichiers `.bsp`, `.log`, `.lin` et `.prt` sont écrits à côté de la map donnée.

### Le dossier de Construire et enregistrer en contenu

```text
<out>/<addon>/addon.json
<out>/<addon>/maps/<map>.bsp, <map>.nav, thumb/<map>.png
<out>/<addon>/materials/..., models/..., sound/...   (addon Workshop)
<out>/<addon>.gma
<out>/<addon>_icon.jpg
<out>/fastdl/...                                     (kit serveur)
<out>/.hammerpro_release.toml
```

`.hammerpro_release.toml` est le manifeste de version : pour chaque fichier écrit par une version, la taille et la date de modification de sa source et de la copie écrite. La construction suivante saute les fichiers inchangés et ne réécrit le GMA que si ses entrées ont changé. Hammer Pro ne supprime jamais que des fichiers listés dans ce manifeste.

## Fichiers dans le dossier du jeu

| Chemin | Contenu |
|---|---|
| `garrysmod\maps\<map>.bsp` | La map compilée, copiée là après une compilation (option `compile.copy_to_maps`, activée par défaut ; un autre dossier peut être réglé dans `compile.maps_dir_override`). |
| `garrysmod\addons\hammerpro_dev\` | L'addon de développement : les compilations Draft et Normal n'intègrent pas vos matériaux personnalisés au BSP, ils sont copiés dans cet addon dossier pour que la map s'affiche correctement en jeu. La copie est incrémentale et n'écrit jamais en dehors de ce dossier. Avec la politique d'intégration par défaut, les compilations Final et les versions de publication intègrent le contenu. Paramètre `compile.dev_addon`, activé par défaut. |
| `garrysmod\addons\hammerpro_bridge\` | L'addon pont HammerPro, un petit fichier Lua installé seulement à la demande (jamais intégré à une map). Il place le joueur à la caméra quand la map se charge, déclenche des inputs et renvoie les props que vous avez placés en jeu pour **Importer les props depuis le jeu**. |
| `garrysmod\data\hammerpro\` | Fichiers échangés entre l'éditeur et le jeu via le pont : `cmd.json` (écrit par l'éditeur), `ack.json` et `props.json` (écrits par le jeu). |
| `garrysmod\console.log` | Écrit par le jeu quand l'option de démarrage rapide `-condebug` est active ; Hammer Pro le lit pour le rapport de chargement de la map. |

## Fichiers de la mise à jour

| Chemin | Ce que c'est |
|---|---|
| `hammerpro.exe.old` | À côté du programme : la version remplacée par la dernière mise à jour, gardée comme sauvegarde (voir [Mises à jour](../features/updates.md)). |
| `%TEMP%\HammerPro-update-<processus>-<tag>\` | L'archive de la version téléchargée. Supprimée à un démarrage ultérieur. |
| `%TEMP%\HammerPro-updater-<processus>\` | La copie du programme qui installe la mise à jour, et son `update.log`. Supprimée à un démarrage ultérieur. |

## Pages liées

- [Référence des paramètres](settings.md)
- [Sauvegarde automatique et plantages](../features/autosave-crash.md)
- [Plantages et logs](../troubleshooting/crashes-and-logs.md)
- [Lancement du jeu](../features/game-launch.md)
- [Construire et enregistrer en contenu](../features/build-content-release.md)
- [Compatibilité VMF](../getting-started/vmf-compatibility.md)
- [Fenêtre de compilation](../compiler/compile-window.md)

---
[Retour à l'index de la documentation](../README.md)
