[Documentation](../README.md) > Développement > Architecture | [English](../../en/development/architecture.md)

# Architecture

Hammer Pro est découpé en petites crates spécialisées dans `crates/`, avec des dépendances qui vont dans un seul sens (aucun cycle). Cette page présente chaque crate en une ligne et montre comment elles dépendent les unes des autres. Le document complet pour les développeurs (objectifs, conventions, systèmes de coordonnées, circulation des données) est [ARCHITECTURE.md](../../ARCHITECTURE.md), en anglais.

## Crates

### Formats de fichiers et bibliothèques

| Crate | Rôle |
|---|---|
| `hp-kv` | Lecture tolérante et écriture rapide des fichiers texte KeyValues de Valve (VMF, VMT, `gameinfo.txt`, `.vdf` de Steam). |
| `hp-vmf` | Modèle de document VMF, lecture et écriture : sans perte, rapide, compatible Hammer et Hammer++. |
| `hp-vmt` | Lecture et écriture des matériaux VMT (shader, paramètres, proxies, fallbacks). |
| `hp-vtf` | Décodeur et encodeur du format de texture VTF. |
| `hp-vpk` | Lecteur d'archives VPK utilisable depuis plusieurs threads. |
| `hp-gma` | Lecteur d'archives d'addons Garry's Mod (GMA), anciens `.bin` en LZMA compris. |
| `hp-fgd` | Lecture et résolution des fichiers de définitions d'entités FGD. |
| `hp-mdl` | Chargement des modèles studio (`.mdl` + `.vvd` + `.vtx`), versions MDL 44 à 49. |
| `hp-bsp` | Lecture et écriture robustes des fichiers BSP, versions 19 à 21 : lumps, pakfile, cubemaps, game lumps, Protect your BSP. |
| `hp-nav` | Lecture des nav meshes (`.nav`), versions 1 à 16. |
| `hp-pcf` | Systèmes de particules des fichiers `.pcf` (DMX binaire) et petite simulation d'aperçu. |
| `hp-meshio` | Lecture des modèles OBJ, glTF/GLB et FBX et compilation en fichiers de modèles du jeu. |

### Géométrie, contenu et compilation

| Crate | Rôle |
|---|---|
| `hp-geom` | Géométrie des brushes : plans, polygones, calcul des textures, CSG, primitives, sélection par rayon, displacements. |
| `hp-fs` | Système de fichiers du jeu et index des ressources : détection des jeux, `gameinfo.txt`, VPK, addons, Workshop, `mount.cfg`, dossiers de textures personnalisés. |
| `hp-light` | Outils d'éclairage partagés par le compilateur, l'aperçu précalculé et les lumières d'aperçu de l'éditeur (lecture des lumières, fichiers `.rad`, hiérarchie de rayons). |
| `hp-bspc` | Le compilateur de maps : du VMF au BSP, écrit de zéro en suivant les étapes de VBSP, plus la visibilité et l'éclairage. |
| `hp-compile` | La chaîne de compilation : lance `hammerpro --bspc` et `--rad`, empaquette le contenu personnalisé, copie la map, lance le jeu, transmet le journal. |
| `hp-decompile` | Reconstruit une map modifiable depuis un BSP compilé, dans l'esprit de BSPSource. |

### Cœur de l'éditeur et services de l'application

| Crate | Rôle |
|---|---|
| `hp-editor` | Le cœur d'édition : document, sélection, annulation, opérations d'édition, sélection par rayon. Sans code d'interface ni de GPU. |
| `hp-render` | Le moteur de rendu GPU (wgpu) : vues 3D et 2D, regroupement des dessins, surimpressions. |
| `hp-app` | Cœur partagé de l'application : état, paramètres, actions, traductions, thème, calcul des vues, interface des outils et points d'extension des modules. |
| `hp-assets` | Gestion des ressources : montage du contenu du jeu et des FGD en arrière-plan, chargement progressif des matériaux, modèles et sprites, synchronisation de la scène du rendu. |
| `hp-mcp` | Le serveur MCP d'une fenêtre, le pont stdio `--mcp` et l'installation dans les applications d'IA (voir [MCP.md](../../MCP.md), en anglais). |
| `hp-discord` | Statut Discord (Rich Presence) par le protocole IPC local de Discord. |

### Interface et outils

| Crate | Rôle |
|---|---|
| `hp-ui-shell` | Barre de menus, barre d'outils, palette d'outils, barre d'état, console, toasts, fenêtre des paramètres, palette de commandes, fenêtres des raccourcis et À propos, assistant du premier lancement. |
| `hp-ui-viewports` | Les vues en 2x2 : disposition, séparateurs, navigation de la caméra, rendu, envoi des entrées aux outils, menu contextuel, glisser-déposer. |
| `hp-ui-textures` | Navigateur de textures, outil d'application de texture (Face Edit), recherche et remplacement de textures, éditeur de matériaux (VMT), import d'images. |
| `hp-ui-entities` | Propriétés des entités (SmartEdit, flags, I/O, keyvalues), choix de classe, navigateur de modèles, recherche d'entités, rapport des entités, propriétés de la map. |
| `hp-ui-mapping` | Visgroups, arborescence, fenêtre de compilation avec journal en direct, vérification des problèmes, transformation, collage spécial, aller à un id, informations sur la map, pointfiles. |
| `hp-ui-lab` | Fonctions plus lourdes : vue logique, affichage du nav mesh, ouverture des maps compilées, aperçu des particules, comparaison de maps. |
| `hp-tools-select` | Outil de sélection (clic et cadre, poignées, clonage au glisser, gizmo 3D, déplacement au clavier) et outil caméra. |
| `hp-tools-create` | Outils de création : bloc (primitives), entité, polygone, chemin, découpe, décal, overlay, cordon, sprinkle. |
| `hp-tools-mesh` | Outil vertex et outil displacement. |
| `hammerpro` | L'application (eframe/egui) : modules, répartition des actions, outils MCP, scripts, ligne de commande, gestion des plantages. |

## Sens des dépendances

Les dépendances descendent toujours cette liste de couches, jamais l'inverse (dans la couche interface, `hp-ui-lab` utilise aussi `hp-ui-viewports`) :

```
hammerpro                      l'application, dépend de tout ce qui suit
hp-ui-*, hp-tools-*            interface et outils
hp-assets                      gestion des ressources
hp-app                         état partagé, paramètres, actions, traductions
hp-editor, hp-render           cœur d'édition, rendu
hp-bspc, hp-decompile          compilateur, décompilateur
hp-light, hp-compile           outils d'éclairage, chaîne de compilation
hp-fs, hp-geom                 système de fichiers du jeu, géométrie des brushes
hp-vmf, hp-vmt                 formats construits sur hp-kv
hp-kv, hp-bsp, hp-vpk, hp-gma, hp-vtf, hp-mdl, hp-fgd, hp-nav, hp-pcf, hp-meshio
hp-discord, hp-mcp             autonomes (aucune dépendance interne)
```

Quelques liens concrets :

```
hp-kv <- hp-vmf <- hp-geom <- hp-editor <- hp-app <- hp-ui-*, hp-tools-* <- hammerpro
hp-kv, hp-vpk, hp-gma, hp-vtf, hp-vmt, hp-mdl, hp-bsp <- hp-fs <- hp-light <- hp-bspc
hp-bsp, hp-kv <- hp-compile
hp-vmf, hp-geom, hp-bsp <- hp-decompile <- hp-ui-lab
hp-discord <- hp-ui-shell
```

Les bibliothèques renvoient des erreurs et ne paniquent jamais sur une entrée invalide ; les traitements lourds tournent dans des threads de fond et rendent compte par des canaux, pour que le thread de l'interface reste libre.

## Ajouter une fonctionnalité

Les modules de fonctionnalités se branchent sur des registres partagés de `hp-app` (actions, paramètres, menus, pages de paramètres, segments de la barre d'état, surimpressions, données annexes par map) au lieu de modifier le cœur. Voir [Étendre Hammer Pro](extending.md).

## Pages liées

- [ARCHITECTURE.md](../../ARCHITECTURE.md) (document complet pour les développeurs, en anglais)
- [Compiler depuis les sources](building.md)
- [Contribuer](contributing.md)
- [Étendre Hammer Pro](extending.md)

---
[Retour à l'index de la documentation](../README.md)
