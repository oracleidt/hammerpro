[Hammer Pro](../../README.fr.md) > Documentation | [English](../en/README.md)

# Documentation de Hammer Pro

Bienvenue dans la base de connaissances de Hammer Pro 1.1.1, l'éditeur de maps pour le moteur Source pensé d'abord pour Garry's Mod. Chaque fonctionnalité a sa propre page : ce que c'est, comment l'ouvrir, chaque option avec sa valeur par défaut, des astuces et une courte FAQ. Les pages renvoient les unes aux autres : commencez où vous voulez.

Vous découvrez Hammer Pro ? Lisez [Installation](getting-started/installation.md), [Configuration du jeu](getting-started/game-setup.md) puis [Votre première map](getting-started/first-map.md), dans cet ordre.

## Prise en main

| Page | Ce que vous y trouverez |
| --- | --- |
| [Installation](getting-started/installation.md) | Configuration requise, téléchargement, premier lancement, mise à jour, désinstallation. |
| [Configuration du jeu](getting-started/game-setup.md) | Assistant du premier lancement, choix du jeu, montage du contenu (VPK, addons, Workshop, `mount.cfg`, téléchargements), FGD. |
| [Tour de l'interface](getting-started/interface-tour.md) | Barre de titre, menus, barre d'outils, palette d'outils, les quatre vues, onglets du dock, barre d'état, console. |
| [Votre première map](getting-started/first-map.md) | Une pièce, une lumière, un point d'apparition, compilée et jouée dans Garry's Mod, pas à pas. |
| [Compatibilité VMF](getting-started/vmf-compatibility.md) | Ouvrir et enregistrer, travailler avec Hammer et Hammer++, le fichier annexe, import et export. |

## Fonctionnalités

### Créer et modifier la géométrie

| Page | Ce que vous y trouverez |
| --- | --- |
| [Outil de sélection](features/selection-tool.md) | Sélection au clic et au cadre, modes de sélection, poignées 2D, poignées d'étirement 3D, gizmo, transformations modales, décalages. |
| [Caméra et navigation](features/camera-navigation.md) | Voler avec la caméra 3D, dispositions de clavier, vue souris, outil caméra, défilement et zoom 2D, schémas de contrôle. |
| [Outil bloc](features/block-tool.md) | Les brushes et chaque primitive avec ses options. |
| [Outil polygone](features/polygon-tool.md) | Des brushes convexes à partir d'un contour cliqué point par point. |
| [Outil courbe](features/curve-tool.md) | Murs et rambardes courbes : des brushes en boîte à joints en onglet le long d'un tracé dans une vue 2D. |
| [Outil route](features/road-tool.md) | Routes le long d'une spline : voies, accotements, lignes, dévers, hauteurs, suivi du terrain, jonctions avec évasements. |
| [Outil tuyau](features/pipe-tool.md) | Tuyaux et conduits en brushes : creux ou pleins, carrés ou ronds, coudes à 90 degrés, colliers, parcours sur la grille. |
| [Semis par matériau](features/scatter-by-material.md) | Des props placés seulement là où une texture apparaît sur les matériaux de mélange et les displacements, avec nombre, écart, rotation, échelle et graine. |
| [Détails de route](features/road-details.md) | Trottoirs, bordures, caniveaux en V et en U avec fentes d'évacuation, angles de jonction arrondis, et fondu du terrain sur une route avec fondu de texture. |
| [Outil de découpe](features/clip-tool.md) | Couper des brushes avec un plan. |
| [Outil tranche](features/slice-tool.md) | Couper des brushes en plusieurs blocs égaux, de gauche à droite ou de haut en bas, depuis la vue 3D (`Maj+Y`, souris pour la position, molette pour les segments). |
| [Outil extruder / creuser](features/extrude-tool.md) | Des contours cliqués sur une face, creusés dans le brush (renfoncement ou ouverture) ou extrudés hors de la face. |
| [Outil biseau](features/bevel-tool.md) | Arrondir ou chanfreiner les arêtes des brushes dans la vue 3D (`Ctrl+B`, souris pour la taille, molette pour les segments), ou les biseauter vers l'intérieur : une gorge, ou un arrondi concave qui remplit un coin intérieur (arches). |
| [Outil vertex](features/vertex-tool.md) | Déplacer, fusionner, scinder et mettre à l'échelle des sommets. |
| [Outil displacement](features/displacement-tool.md) | Créer, sculpter, peindre l'alpha, tags, couture, bruit. |
| [Terrain](features/terrain.md) | Importer et exporter des cartes de hauteur, génération procédurale de terrain, érosion, texturage automatique. |
| [Opérations sur les brushes](features/brush-operations.md) | Carve, creuser, fusionner, lier à une entité, func_detail, nodraw, brushes hint, verrouillage des textures. |
| [Transformer et aligner](features/transform-align.md) | Fenêtre Transformer, aligner, retourner, magnétisme, poser au sol, placement physique. |
| [Presse-papiers](features/clipboard.md) | Couper, copier, coller, collage spécial, dupliquer, entre fenêtres Hammer Pro. |
| [Grilles](features/grids.md) | Grille globale, une grille par vue, grilles personnalisées, grilles des objets, gestionnaire de grilles, grille 3D. |

### Entités et gameplay

| Page | Ce que vous y trouverez |
| --- | --- |
| [Outil entité](features/entity-tool.md) | Poser des entités ponctuelles et des prefabs sur les surfaces et les modèles. |
| [Propriétés des entités](features/entity-properties.md) | Le panneau d'entité, SmartEdit, flags, éditeur de couleur, keyvalues, rechercher et remplacer, rapport des entités, propriétés de la map. |
| [I/O et vue logique](features/io-logical-view.md) | Sorties et entrées, lignes de connexion, la fenêtre de vue logique et la vue logique 2D. |
| [Outil chemin](features/path-tool.md) | Chaînes de path_corner, path_track et autres entités de chemin. |
| [Décals et overlays](features/decals-overlays.md) | Outil décal et outil overlay. |
| [Outil sprinkle](features/sprinkle-tool.md) | Peindre des entités sur les surfaces. |
| [Instances](features/instances.md) | func_instance : créer, modifier, fusionner, paramètres. |
| [Prefabs](features/prefabs.md) | La bibliothèque de prefabs. |
| [Caméras de skybox](features/sky-cameras.md) | Plusieurs skybox 3D et l'entité qui change de skybox. |
| [Soundscapes](features/soundscapes.md) | L'éditeur de soundscapes. |
| [Nav mesh](features/nav-mesh.md) | Générer et afficher le `.nav` de la map. |

### Textures et matériaux

| Page | Ce que vous y trouverez |
| --- | --- |
| [Navigateur de textures](features/texture-browser.md) | Syntaxe de recherche, filtres, favoris, glisser-déposer. |
| [Outil de texture](features/texture-tool.md) | Édition de faces : appliquer, échelle, décalage, rotation, justifier, aligner, échelle de lightmap. |
| [Rechercher et remplacer des textures](features/find-replace-textures.md) | Remplacer des matériaux dans la sélection ou toute la map. |
| [Dossiers de textures personnalisés](features/custom-texture-folders.md) | Vos propres dossiers, des images simples comme matériaux, import d'images. |
| [Index et analyse du contenu](features/content-index.md) | L'index enregistré des fichiers du jeu, analyse des disques durs et SSD, dossiers en double. |
| [Éditeur UV](features/uv-editor.md) | Modes face, arête et sommet, calques, fenêtre séparée. |
| [Éditeur de matériaux](features/material-editor.md) | Modifier des VMT, un par un ou par lots. |
| [Éditeur de VTF](features/vtf-editor.md) | Modifier des textures dans VTFEdit (ou un autre éditeur VTF installé) ou dans l'éditeur intégré : canaux, mips, images, cubemaps, drapeaux, formats, normal maps. |
| [Éditeur de shaders](features/shader-editor.md) | Éditeur d'images et de matériaux à nœuds : bruit, flou, rampes de couleurs, normal maps, mélange de tuiles, écrit en VTF et VMT. |

### Modèles, sons et effets

| Page | Ce que vous y trouverez |
| --- | --- |
| [Navigateur de modèles](features/model-browser.md) | Grille de modèles, aperçu 3D, inspecteur de modèles et outils. |
| [Import de modèles](features/model-import.md) | FBX, OBJ, glTF et GLB directement dans le jeu. |
| [Lien avec Blender](features/blender.md) | Envoyer des brushes, des displacements ou un prop vers Blender et récupérer la modification tout seul. |
| [Propper](features/propper.md) | Des brushes vers un prop statique (LOD, collision, displacements précalculés) et des props vers des brushes. |
| [Navigateur de sons](features/sound-browser.md) | Sons du jeu, soundscripts et sélecteur de sons. |
| [Éditeur de sons](features/sound-editor.md) | Coupe, gain, marqueurs de boucle et sortie WAV pour tout son du jeu. |
| [Correction des couleurs](features/color-correction.md) | Éditeur de tables de couleurs (.raw) et aperçu 3D des entités color_correction. |
| [Particules](features/particles.md) | Éditeur de systèmes PCF, bibliothèque d'opérateurs, aperçu, navigateur et export d'addon. |

### Vues et rendu

| Page | Ce que vous y trouverez |
| --- | --- |
| [Aperçu des detail props](features/detail-props-preview.md) | L'herbe et les objets de détail des matériaux `%detailtype` affichés dans la vue 3D sans compiler. |
| [Modes de rendu et options d'affichage](features/render-modes-display.md) | Modes de rendu 3D, chaque option d'affichage, images de fond, images par seconde. |
| [Aperçu de l'éclairage](features/lighting-preview.md) | Mode éclairé, éclairage précalculé, ombres, normal maps, reflets des cubemaps, outils de lightmap. |
| [Vue joueur](features/player-view.md) | Parcourir la map comme un joueur, avec le monde compilé, les lightmaps et les compteurs. |

### Organiser la map

| Page | Ce que vous y trouverez |
| --- | --- |
| [Groupes](features/groups.md) | Grouper et modes de sélection. |
| [Visgroups](features/visgroups.md) | Visgroups personnels et visgroups automatiques. |
| [Couleurs d'affichage](features/display-colors.md) | Une couleur par brush ou entité dans les vues 2D, enregistrée dans la map. |
| [Masquer et isoler](features/hide-isolate.md) | Masquer des objets, isoler la sélection. |
| [Cordons](features/cordons.md) | Outil cordon, liste des cordons, compiler une partie de la map. |
| [Arborescence](features/outliner.md) | La map sous forme d'arbre. |
| [Annulation et historique](features/undo-history.md) | Annuler, rétablir, annulation de la sélection, panneau d'historique. |

### Santé de la map et maps compilées

| Page | Ce que vous y trouverez |
| --- | --- |
| [Vérifier les problèmes](features/check-problems.md) | Problèmes et corrections, vérifications en direct, limites, informations sur la map, suggestions de func_detail. |
| [Fuites et pointfiles](features/leaks-pointfiles.md) | Vérification des fuites, chemin de fuite, fichiers de portails, visibilité, areaportals. |
| [Outils BSP](features/bsp-tools.md) | Le Décompilateur de BSP, les archives (BSP, GMA, VPK), export OBJ, comparer des maps. |
| [BSP Sniffer](features/bsp-sniffer.md) | Rejoindre un serveur Garry's Mod et récupérer la map telle que le jeu l'a chargée, en VMF, en BSP et avec son contenu (Garry's Mod uniquement, usage autorisé uniquement). |

### Tester et publier

| Page | Ce que vous y trouverez |
| --- | --- |
| [Lancement du jeu](features/game-launch.md) | Lancer et recharger la map en jeu, options de démarrage rapide, l'addon passerelle. |
| [Ressources de la map](features/map-assets.md) | Chaque fichier utilisé par la map, sa provenance, sa collecte. |
| [Construire et enregistrer en contenu](features/build-content-release.md) | Fenêtre de publication, liste de contrôle, addon, publication sur le Workshop. |
| [Protect your BSP](features/bsp-protection.md) | Livrer une copie protégée de la map, verrou serveur. |

### L'application

| Page | Ce que vous y trouverez |
| --- | --- |
| [Fenêtre des paramètres](features/settings-dialog.md) | Chaque page de la fenêtre des paramètres. |
| [Raccourcis clavier](features/keyboard-shortcuts.md) | Jeux de raccourcis, modifier un raccourci, conflits, référence `F1`. |
| [Palette de commandes](features/command-palette.md) | `Ctrl+P` : toutes les commandes par leur nom. |
| [Plusieurs maps](features/multiple-maps.md) | Onglets, changer de map, dispositions enregistrées. |
| [Thèmes et langues](features/themes-languages.md) | Thèmes sombre et clair, couleur d'accent, échelle, image de fond, anglais et français. |
| [Cadres de fenêtre et fenêtres séparées](features/window-frames.md) | Cadres de Hammer Pro, cadres natifs, fenêtres sur d'autres écrans. |
| [Sauvegarde automatique et plantages](features/autosave-crash.md) | Sauvegardes automatiques, sauvegardes d'urgence, rapports de plantage. |
| [Statut Discord (Rich Presence)](features/discord.md) | Ce que votre profil Discord affiche pendant que vous mappez. |
| [Mises à jour](features/updates.md) | La recherche d'une version plus récente au démarrage, la fenêtre de mise à jour et l'installation automatique. |
| [Stockage](features/storage.md) | L'espace disque utilisé par Hammer Pro, ce qui en prend le plus, et le nettoyage sûr des caches et fichiers temporaires. |

## Compilateur

| Page | Ce que vous y trouverez |
| --- | --- |
| [Le compilateur intégré](compiler/overview.md) | Géométrie, visibilité et éclairage, et leurs différences avec VBSP, VVIS et VRAD. |
| [Fenêtre Compiler et lancer](compiler/compile-window.md) | Préréglages, étapes, options, mode expert, compilation par lots, historique et statistiques. |
| [Options de compilation](compiler/compile-options.md) | Chaque option de compilation avec son effet et sa valeur par défaut. |
| [Choisir un compilateur](compiler/compilers.md) | vbsp, vvis et vrad de Valve ou vos propres programmes à la place du compilateur intégré, avec chaque option. |
| [Ligne de commande du compilateur](compiler/command-line.md) | `hammerpro --bspc` et `hammerpro --rad`, chaque argument. |
| [Clés worldspawn et lumières](compiler/worldspawn-keys.md) | `_lightsrad` et les autres clés lues par le compilateur. |
| [Jeux cibles](compiler/target-games.md) | Jeux Source pris en charge et branches du moteur. |
| [Cubemaps](compiler/cubemaps.md) | Construire les cubemaps, les écrire dans la map, cubemaps à correction de parallaxe. |
| [Limites et journaux](compiler/limits-and-logs.md) | Limites du moteur par jeu, lire le journal de compilation et les statistiques. |
| [Performances de compilation](compiler/performance.md) | Compiler plus vite. |

## Référence

| Page | Ce que vous y trouverez |
| --- | --- |
| [Référence des paramètres](reference/settings.md) | Chaque clé de `settings.toml` avec sa valeur par défaut. |
| [Actions et raccourcis clavier](reference/actions-shortcuts.md) | Chaque commande, son menu, son id et son raccourci dans chaque jeu de raccourcis. |
| [Ligne de commande](reference/command-line.md) | Chaque argument de `hammerpro.exe`. |
| [Commandes de script](reference/script-commands.md) | Les commandes des fichiers `--script`. |
| [Serveur MCP](reference/mcp-server.md) | Connecter un assistant IA (Claude, Codex, Gemini). |
| [Référence des outils MCP](../MCP.md) | Chaque outil MCP (générée, en anglais). |
| [Fichiers et dossiers](reference/files-and-folders.md) | Chaque fichier lu ou écrit par Hammer Pro. |

## Guides

| Guide | Ce que vous allez faire |
| --- | --- |
| [Compiler et tester dans Garry's Mod](guides/compile-and-test-gmod.md) | La boucle la plus rapide entre l'édition et le jeu. |
| [Créer un prop avec Propper](guides/make-a-prop-with-propper.md) | Transformer des brushes en prop statique avec LOD et collision. |
| [Sculpter des displacements](guides/sculpt-displacements.md) | Du brush au terrain peint avec des mélanges de textures. |
| [Éclairage](guides/lighting.md) | Lumières, aperçus, LDR et HDR, éclairage des props statiques. |
| [Grilles personnalisées pour les escaliers](guides/custom-grids-for-stairs.md) | Une grille avec un pas par axe pour des marches régulières. |
| [Rassembler le contenu pour une publication](guides/collect-content-for-release.md) | Réunir chaque fichier personnalisé dont la map a besoin. |
| [Porter une map vers un autre jeu](guides/port-to-another-game.md) | Changer de jeu cible et corriger ce qui diffère. |
| [Utiliser Hammer Pro avec un assistant IA](guides/mcp-with-ai-assistant.md) | Configurer MCP et travailler avec un assistant. |

## Dépannage

| Page | Ce que vous y trouverez |
| --- | --- |
| [FAQ](troubleshooting/faq.md) | Les questions que tout le monde se pose. |
| [Erreurs courantes](troubleshooting/common-errors.md) | Fuites, textures manquantes, limites, compilations ratées. |
| [Plantages et journaux](troubleshooting/crashes-and-logs.md) | Rapports de plantage, fichier journal, problèmes de carte graphique, cadres natifs. |
| [Mémoire insuffisante](troubleshooting/out-of-memory.md) | Budget de mémoire graphique, nombreuses grandes maps ouvertes, mémoire de l'ordinateur. |
| [Périphérique graphique perdu](troubleshooting/graphics-device-lost.md) | Réinitialisations du pilote, ce qui est sauvegardé, redémarrage avec vos onglets. |
| [Performances de l'éditeur](troubleshooting/performance.md) | Garder l'éditeur fluide sur les très grandes maps. |
| [Le statut Discord ne s'affiche pas](troubleshooting/discord-presence.md) | Réparer le Rich Presence. |
| [Signaler un bug](troubleshooting/reporting-bugs.md) | Ce qu'il faut envoyer pour qu'un bug soit corrigé. |

## Développement

| Page | Ce que vous y trouverez |
| --- | --- |
| [Compiler depuis les sources](development/building.md) | Chaîne d'outils Rust, profils de compilation, tests. |
| [Architecture](development/architecture.md) | Les crates et leurs dépendances. |
| [Contribuer](development/contributing.md) | Règles des pull requests. |
| [Étendre Hammer Pro](development/extending.md) | Les points d'extension du code. |

---
[Retour au README](../../README.fr.md)
