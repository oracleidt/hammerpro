[Documentation](../README.md) > Prise en main > Tour de l'interface | [English](../../en/getting-started/interface-tour.md) | [Русский](../../ru/getting-started/interface-tour.md)

# Tour de l'interface

La fenêtre de Hammer Pro reprend la disposition de Hammer et de Hammer++ (menus dans le même ordre, outils à gauche, quatre vues, propriétés à droite) : si vous venez de ces éditeurs, chaque bouton est là où vous l'attendez. L'interface est en noir et blanc (thème sombre ou clair) : la couleur est réservée à votre map, aux textures et aux modèles.

## Cadre de la fenêtre et barre de titre

Hammer Pro dessine son propre cadre de fenêtre : coins arrondis, ombre douce et barre de titre qui contient le logo Hammer Pro, les menus, le titre `<nom de la map> - Hammer Pro` et les boutons réduire, agrandir/restaurer et fermer. Le survol du titre affiche le chemin complet de la map (« Pas encore enregistrée » pour une nouvelle map) et « Modifications non enregistrées » quand la map a été modifiée.

Si ce cadre pose problème (bureau à distance, certains gestionnaires de fenêtres), activez **Paramètres > Général > Cadres de fenêtre natifs** : la barre de titre du système revient et les menus passent dans une barre de menus en dessous. Voir [Cadres de fenêtre](../features/window-frames.md).

## Barre de menus

| Menu | Contenu |
|---|---|
| **Fichier** | Nouvelle map, Ouvrir une map..., **Maps récentes**, Fermer, Fermer les autres maps, Enregistrer, Enregistrer sous..., Enregistrer comme nouvelle version..., **Importer** (Importer une map dans la map actuelle..., Importer un modèle (FBX, OBJ, glTF)..., Importer des modèles (lot)..., Importer les props depuis le jeu, Importer une carte de hauteur..., Ouvrir une archive..., Décompilateur de BSP..., BSP Sniffer...), **Exporter** (Exporter la sélection en VMF..., Exporter une copie..., Exporter à nouveau, Exporter en OBJ..., Exporter une carte de hauteur...), **Compiler** (Compiler et lancer..., Compiler maintenant, Annuler la compilation, Compilation par lots..., Lancer la map dans le jeu, Recharger la map dans le jeu), **Publier** (Construire et enregistrer en contenu..., Publier sur le Workshop...), Quitter |
| **Édition** | Annuler, Rétablir, Couper, Copier, Coller, Collage spécial..., Supprimer, Dupliquer, Duplication en série..., Tout sélectionner, Effacer la sélection, Inverser la sélection, **Mode de sélection** (Groupes, Objets, Solides, Ignorer les groupes), **Élément de sélection** (Objets entiers, Sommets, Arêtes, Faces, Sélectionner la boucle d'arêtes, Sélectionner l'anneau d'arêtes, Agrandir la sélection, Réduire la sélection, Sélectionner le contour, Sélection de faces précédente, Sélection de faces suivante), Rechercher des entités..., Rechercher et remplacer dans les entités..., Propriétés, Copier les keyvalues, Coller les keyvalues, Définir la distance maximale..., **Couleurs d'affichage** (Modifier la couleur..., Définir la couleur d'affichage..., Couleurs d'affichage aléatoires, Couleurs d'affichage par défaut) |
| **Map** | Magnétisme de grille, Afficher la grille, Grille 3D sur les faces, Grille plus fine, Grille plus large, **Grilles** (Grille globale, Une grille par vue, Grille personnalisée, Grille de la sélection, Grille depuis la sélection, Grille depuis la face, Attacher la grille à la sélection, Détacher la grille de la sélection, Gestionnaire de grilles..., Pivoter la grille de 45 degrés, Aligner la grille sur l'arête ou la face sélectionnée), Vérifier les problèmes, Vérifier les fuites, Aller à un id de brush ou d'entité..., Rapport des entités, Ressources de la map, Comparer avec une autre map..., **Fuites et visibilité** (Charger le pointfile de fuite..., Décharger le pointfile, Portails et visibilité..., Construire les portails (géométrie seule), Charger le fichier de portails..., Décharger le fichier de portails, Suggestions de func_detail..., Fermer l'ouverture par un areaportal, Charger un nav mesh...), Informations sur la map, Propriétés de la map..., Soundscapes..., Insérer la galerie d'entités, **Éclairage** (Précalculer l'éclairage, Précalculer autour de la caméra, Arrêter le précalcul, Isoler les lumières sélectionnées, Couper les lumières sélectionnées, Réactiver toutes les lumières, Mesurer le coût de chaque lumière, Sélectionner les faces à petite échelle de lightmap, Reconstruire les cubemaps, Écrire les cubemaps dans la map compilée) |
| **Affichage** | **Type de vue** (Vue de dessus, Vue de côté, Vue de face, Vue 3D texturée, Vue 3D unie, Vue 2D suivante, Vue logique 2D, Vue logique), **Mode de rendu 3D** (Texturé, Couleurs unies, Filaire, Texturé + arêtes du mesh, Aperçu de l'éclairage, Aperçu de l'éclairage précalculé, Texturé sans ombrage, Groupes de lissage, Grille de lightmap, 3D Correction colorimétrique), Vue joueur, **Options d'affichage** (Afficher les modèles, Afficher les aides et sprites, Afficher les textures outils, **Textures outils masquées**, Afficher les entités ponctuelles, Afficher les objets d'éditeur, Aides de toutes les entités, Afficher les connexions, Afficher les overlays, Afficher les displacements, Afficher le nodraw, Boîtes englobantes, Afficher les noms des entités en 2D, Afficher le centre des brushes en 2D, Maillage des displacements en vue 2D, Couleurs des objets en 2D, Couleurs des objets en 3D filaire et plat, Surligner les brushes invalides, Informations de la vue 3D, Images de fond...), **Aperçu de l'environnement** (Aperçu du ciel, Skybox 3D, Utiliser en jeu la sky_camera affichée, **Caméra de la skybox 3D**, Aperçu du brouillard, Aperçu des fog_volume, Texture des fog_volume, **Contrôleur de brouillard prévisualisé**, Aperçu de l'eau, Aperçu des detail props (herbe), Aperçu des poissons, Poissons qui nagent, Prévisualiser les entités color_correction, Reflets des cubemaps), **Effets** (Effets de rendu, Aperçu des effets de volume, Matériaux animés, Fondu des modèles, Matériaux opaques, Limite de distance circulaire, Afficher les particules, Particules de la sélection uniquement, Afficher les cordes, Simuler les cordes), **Diagnostics** (Afficher les clusters visibles, Afficher le nav mesh, Modèles de collision, Pentes praticables, Praticabilité des displacements, Afficher les collisions des modèles), Masquer la sélection, Masquer le reste, Tout afficher, Utiliser les cordons, Déplacer vers un visgroup..., Créer un visgroup des objets masqués, Centrer les vues 2D sur la sélection, Centrer la vue 3D sur la sélection, Agrandir la vue, Agrandir la fenêtre, **Zoom et distance d'affichage** (Zoom avant des vues 2D, Zoom arrière des vues 2D, Distance d'affichage plus courte, Distance d'affichage plus longue, Réinitialiser le partage des vues), **Panneaux** (Navigateur de textures, Propriétés de l'entité, Visgroups, Arborescence, Navigateur de modèles, Navigateur de sons, Navigateur de particules, Aperçu des particules..., Éclairage, Prefabs, Historique des modifications, Barre d'outils, Palette d'outils, Barre d'état), **Langue** |
| **Outils** | **Outil actif** (Outil de sélection, Outil caméra, Outil entité, Outil bloc, Application de texture, Appliquer la texture actuelle, Outil décal, Outil overlay, Outil de découpe, Outil tranche, Outil extruder / creuser, Outil biseau, Outil vertex, Outil chemin, Outil polygone, Outil courbe, Outil sprinkle, Outil displacement, Outil cordon, Outil route, Outil tuyau, Simulateur physique), Transformer..., Aligner la sélection sur la grille, **Disposer** (Aligner à gauche, Aligner à droite, Aligner en haut, Aligner en bas, Centrer horizontalement, Centrer verticalement, Retourner horizontalement, Retourner verticalement, Retourner X, Retourner Y, Retourner Z, Poser au sol, Coller au plafond, Poser au sol, aligné sur la surface, Coller au plafond, aligné sur la surface, Aligner chaque objet sur la grille, Centrer les origines, Pivot déplaçable, Rotation Z aléatoire, Options de rotation Z aléatoire...), Grouper, Dégrouper, Lier à une entité, Renvoyer au monde, Creuser (carve), Rendre creux, **Opérations sur les brushes** (Fusionner les brushes, Intersection de brushes, Union de brushes, Fusionner en un solide convexe, Extrusion interactive (E), Décalage interactif (I), Décaler la face..., Convertir en func_detail, Lier chacun à sa propre entité, Nodraw sur les faces cachées...), **Sommets et arêtes** (Scinder la face, Mettre les sommets à l'échelle..., Souder les sommets sélectionnés, Souder les sommets par distance, Supprimer les faces et arêtes dégénérées, Faire glisser la boucle d'arêtes..., Faire tourner les sommets..., Verrouillage UV (outil sommets)), **Displacements** (Subdiviser les displacements, Lisser la boucle du displacement, Fixer la hauteur de la boucle du displacement, Inverser l'alpha des displacements, Sélectionner les displacements par pente, Fusionner les hauteurs des displacements, Limiter la pente des displacements, Uniformiser la pente des displacements, Ajouter du bruit aux displacements..., Bruit fractal sur les displacements..., Érosion thermique des displacements..., Érosion hydraulique des displacements..., Générer un terrain..., Daller la sélection avec des displacements..., Maillage en displacement..., Plaquer les displacements sur un maillage..., Displacement en maillage...), **Courbes et routes** (Modifier la courbe, Modifier la route, Modifier les détails de route..., Générateur de trottoirs..., Générateur de bordures et caniveaux..., Fondre le terrain sur la route...), **Textures** (Rechercher et remplacer des textures..., Verrouillage des textures, Verrouillage de l'échelle des textures), **Placement et props** (Créer un prefab..., Insérer le prefab à sa taille d'origine, Placement physique (chute et pose), Semis par matériau..., Convertir les brushes en prop..., Convertir le prop en brushes..., Options des props (Propper)..., Insérer un point de chemin après la sélection, Supprimer des points de chemin et relier, Générer le nav mesh), **Instances** (Créer une instance depuis la sélection..., Modifier l'instance, Fusionner les instances sélectionnées, Fusionner toutes les instances..., Instances : masquées, Instances : teintées, Instances : normales), **Contenu et éditeurs** (Éditeur de shaders..., Éditeur de VTF..., Éditeur de sons..., Éditeur de correction des couleurs..., Éditeur de particules..., Recharger le contenu depuis le disque, Reconstruire l'index du contenu), **Blender et MCP** (Envoyer vers Blender, Mettre à jour depuis Blender, Arrêter le lien direct avec Blender, Installer l'add-on Blender, Serveur MCP, MCP..., Installer MCP dans Claude, Codex et Gemini), Palette de commandes..., Options... |
| **Fenêtre** | **Disposition** (Enregistrer la disposition..., Réinitialiser la disposition, **Dispositions enregistrées**), Console, Map suivante, Map précédente, **Maps ouvertes** |
| **Aide** | Documentation, Raccourcis clavier, Aide de l'entité, Signaler un problème, Rechercher des mises à jour..., Assistant de configuration..., À propos de Hammer Pro |

Les noms en gras sont des sous-menus. Les commandes fréquentes sont en haut de chaque menu, les commandes liées sont regroupées dans des sous-menus et des séparateurs délimitent les groupes, si bien qu'aucun menu ne dépasse une vingtaine d'entrées. La [palette de commandes](../features/command-palette.md) affiche le chemin de menu de chaque commande (par exemple "Fichier > Importer").

Toutes les commandes, avec leur raccourci par défaut, sont listées dans [Actions et raccourcis](../reference/actions-shortcuts.md). La [palette de commandes](../features/command-palette.md) (`Ctrl+P`) les retrouve par leur nom.

## Barre d'outils

La barre d'outils sous les menus se lit de gauche à droite comme dans la plupart des éditeurs : les actions de fichier d'abord, puis les actions d'édition, puis les outils de la map, avec **Compiler**, l'action principale, tout à droite :

1. Nouvelle map, Ouvrir une map, Enregistrer.
2. Annuler, Rétablir, Couper, Copier, Coller.
3. Afficher la grille, Grille 3D sur les faces, la taille de la grille (plus fine, un champ où saisir n'importe quelle taille de 0,01 à 4096, plus large), Magnétisme de grille.
4. Creuser, Grouper, Dégrouper, Ignorer les groupes.
5. Masquer la sélection, Masquer le reste, Tout afficher.
6. Utiliser les cordons, Outil cordon, Limite de distance circulaire.
7. Maillage des displacements en vue 2D, Praticabilité des displacements, Afficher le nodraw, Afficher les collisions des modèles, Verrouillage des textures, Verrouillage de l'échelle des textures.
8. Afficher les aides et sprites, Afficher les modèles, Afficher les textures outils, Afficher les objets d'éditeur, Skybox 3D, Afficher les particules, Simuler les cordes.
9. La liste des modes de rendu 3D, Vue joueur, Vérifier les problèmes.
10. **Compiler** (avec son icône de marteau), seul tout à droite de la première ligne : ouvre la fenêtre Compiler et lancer (le Run Map de Hammer).

Par défaut, la barre d'outils reste sur une seule ligne. Quand la fenêtre est trop étroite pour tous les groupes (petit écran, grande échelle d'interface, libellés plus longs en français ou en russe), les groupes les moins utilisés passent dans le bouton **»** au bout de la ligne, dans cet ordre : les bascules d'affichage (8), les bascules de displacement et de verrou de texture (7), le groupe des cordons (6), les boutons de masquage et de groupe (5 et 4), le groupe du mode de rendu (9), puis Annuler, Rétablir, Couper, Copier, Coller (2). Les groupes restants gardent leur ordre, et le menu **»** montre les groupes cachés avec les mêmes boutons, infobulles et raccourcis ; il reste ouvert pendant que vous cliquez sur ses boutons. **Compiler** est toujours le dernier à partir.

**Personnaliser la barre d'outils** : clic droit sur la barre d'outils (sur un bouton ou entre les boutons) pour ouvrir son menu :

- **Passer sur deux lignes** : quand la fenêtre est trop étroite, la barre d'outils continue sur une deuxième ligne au lieu de ranger des groupes dans **»** (une fenêtre large reste sur une ligne). Aussi **Affichage > Panneaux > Barre d'outils sur deux lignes** et **Paramètres > Général > Barre d'outils**.
- **Personnaliser la barre d'outils** : un sous-menu par groupe. **Afficher ce groupe** masque ou affiche tout le groupe, une case par bouton masque ou affiche ce bouton, **Déplacer vers la gauche** et **Déplacer vers la droite** changent l'ordre des groupes. **Compiler** peut être masqué mais reste toujours tout à droite.
- **Réinitialiser la barre d'outils** : retour aux groupes, à l'ordre et à la ligne unique par défaut (aussi **Affichage > Panneaux > Réinitialiser la barre d'outils**).
- **Masquer la barre d'outils** (**Affichage > Panneaux > Barre d'outils** la fait revenir).

Les choix sont enregistrés dans les réglages (`workflow.toolbar_rows`, `workflow.toolbar_order`, `workflow.toolbar_hidden`). Quand la barre d'outils est masquée, ou que son groupe de grille ou son bouton de verrouillage des textures est masqué, la barre d'état affiche à la place la taille de la grille, le magnétisme et le verrouillage des textures.

Tout à droite de la barre du haut, la pastille **MCP** indique si le serveur MCP de cette fenêtre est actif, et son menu installe Hammer Pro dans les applications d'IA (voir [Serveur MCP](../reference/mcp-server.md)).

Sous la barre d'outils, la **ligne d'options** affiche le nom de l'outil actif et ses options. Les outils aux options volumineuses (Application de texture, Displacement...) les affichent plutôt dans l'onglet de l'outil du panneau de droite (« Les options de l'outil sont dans le panneau de droite. »).

## Palette d'outils

La bande verticale le long du bord gauche regroupe les outils sous forme d'icônes, séparés par un trait : se déplacer (Sélection, Caméra), créer (Entité, Bloc, Polygone, Courbe, Sprinkle, Route, Tuyau), modifier (Application de texture, Appliquer la texture actuelle, Décal, Overlay, Découpe, Tranches, Extruder, Biseau, Vertex, Displacement), outils spéciaux (Chemin, Cordon, Physique, puis Propper, Unpropper et Envoyer vers Blender), et l'œil d'isolement (isoler la sélection, ou tout réafficher). L'infobulle de chaque bouton donne son nom et son raccourci. Quand la fenêtre est basse, les boutons s'aplatissent, puis la bande défile.

Les icônes sont dessinées par Hammer Pro dans un même style au trait, nettes à toutes les échelles d'interface et dans les deux thèmes, pour que chacune montre ce que fait l'outil : un damier pour l'application de texture, une face avec une flèche vers l'extérieur pour Extruder, un coin de cube coupé pour Biseau, une courbe avec ses poignées pour Chemin, un bloc coupé par un plan pour la Découpe, un bloc coupé en tranches pour Tranches, un autocollant au coin décollé pour Décal, des calques empilés pour Overlay, une arche de blocs pour Courbe, un terrain pour Displacement, une balle qui tombe au sol pour Physique, un brush qui devient un tonneau pour Propper (et l'inverse pour Unpropper). Le bouton Creuser de la barre d'outils montre un bloc entamé par un autre bloc.

**Deux colonnes** : clic droit sur la palette puis **Deux colonnes** (ou **Affichage > Panneaux > Palette d'outils sur deux colonnes**, ou **Paramètres > Général > Palette d'outils**) pour une palette plus large avec les boutons côte à côte, comme dans Photoshop : chaque bouton garde sa taille sur les fenêtres basses. **Une colonne** revient en arrière. Le choix est enregistré (`workflow.tool_palette_columns`). Le même menu propose **Masquer la palette d'outils**.

## Les quatre vues

Le centre de la fenêtre est une grille de 2x2 vues : la vue 3D en haut à gauche, Dessus (X/Y) en haut à droite, Face (Y/Z) en bas à gauche et Côté (X/Z) en bas à droite. Faites glisser les séparateurs pour les redimensionner (**Affichage > Zoom et distance d'affichage > Réinitialiser le partage des vues** les remet en place). Chaque vue a un menu **Vue** dans son coin pour changer ce qu'elle affiche (n'importe quel quart peut aussi afficher la vue logique 2D). `Maj+Z` agrandit la vue sous la souris, et `F2` à `F5` changent son type. Voir [Caméra et navigation](../features/camera-navigation.md) et [Modes de rendu et affichage](../features/render-modes-display.md).

## Panneaux séparés

Faites un clic droit sur un onglet latéral puis **Détacher le panneau**, ou utilisez le bouton du même nom à côté des onglets. Le panneau s'ouvre dans sa propre fenêtre déplaçable et redimensionnable dans l'éditeur : propriétés, textures, modèles et autres panneaux peuvent rester visibles ensemble. Répétez pour les panneaux souhaités. **Rattacher le panneau**, ou fermer sa fenêtre flottante, le remet dans les onglets. Le choix et la position sont conservés au redémarrage ; la visibilité reste réglée dans **Affichage > Panneaux**.

## Onglets des maps

Au-dessus des vues, un onglet par map ouverte. Un `*` après le nom signale des modifications non enregistrées et l'infobulle donne le chemin complet. Cliquez sur un onglet pour l'afficher, clic molette ou croix pour le fermer, **+** pour une nouvelle map. Clic droit : **Fermer**, **Fermer les autres maps**, **Copier le chemin**, **Afficher dans le dossier**. `Ctrl+Tab` et `Ctrl+Maj+Tab` passent d'un onglet à l'autre. Voir [Plusieurs maps ouvertes](../features/multiple-maps.md).

## Panneau de droite

Le panneau de droite commence par la **barre de sélection** (comme la barre de droite de Hammer) : le mode de sélection (**Groupes**, **Objets**, **Solides**) et **Déplacer la sélection :** avec **Vers le monde** et **Vers une entité**. En dessous, des onglets :

| Onglet | Contenu |
|---|---|
| Outil | Options de l'outil actif, quand il utilise le panneau latéral. |
| **Textures** | Le [navigateur de textures](../features/texture-browser.md). |
| **Entité** | Les [propriétés de l'entité](../features/entity-properties.md) sélectionnée. |
| **Modèles** | Le [navigateur de modèles](../features/model-browser.md). |
| **Visgroups** | Les [visgroups](../features/visgroups.md). |
| **Arborescence** | L'[arborescence](../features/outliner.md). |
| **Prefabs**, **Instance**, **Éclairage**, **Ressources de la map**, **Historique**, **Sons**, **Particules** | Panneaux des modules : [prefabs](../features/prefabs.md), [instances](../features/instances.md), [éclairage](../features/lighting-preview.md), [ressources de la map](../features/map-assets.md), [historique](../features/undo-history.md), [sons](../features/sound-browser.md), [particules](../features/particles.md). |

Les onglets présents dépendent de **Affichage > Panneaux**. Par défaut, le navigateur de textures, les propriétés de l'entité, les visgroups et la console sont affichés ; l'arborescence et le navigateur de modèles ne le sont pas.

## Barre d'état

La barre du bas (masquable avec **Affichage > Panneaux > Barre d'état**) est composée de segments.

| Segment | Ce qu'il affiche |
|---|---|
| Aide de l'outil (à gauche) | Ce que l'outil actif attend, l'étape d'une compilation en cours, ou ce que demande un « choix dans les vues » en attente. |
| Sélection | « Aucune sélection », le nombre d'objets sélectionnés, ou la description d'un objet ou d'un groupe unique (« Solide n° 42, 6 faces », « Groupe n° 7 de 5 objets ») avec sa taille et son centre. Un clic copie l'identifiant. |
| Curseur | La position dans le monde sous la souris. |
| Contenu du jeu | Affiché tant que le Workshop et les addons téléchargés s'ouvrent (« Workshop N / M »), quand aucun contenu de jeu n'est configuré (« Aucun contenu de jeu », un clic ouvre les paramètres du jeu) ou quand le chargement a échoué. |
| Santé de la map | Les totaux de problèmes des vérifications en direct (un clic ouvre Vérifier les problèmes), le badge **Fuite** de la vérification instantanée (un clic va à la fuite et trace son chemin), et la jauge de la limite du moteur la plus proche de son maximum (un clic ouvre Informations sur la map). |
| Compilation | L'étape, la progression et un bouton Annuler pendant une compilation, la compilation en arrière-plan, et un badge quand la dernière compilation a fui ou échoué. Un clic ouvre la fenêtre de compilation. |
| Jeu | « Jeu prêt » quand Garry's Mod tourne et peut recevoir une map, « Jeu en démarrage » pendant son lancement. |
| Éclairage | La progression du précalcul avec un bouton Arrêter, ou le budget de lightmaps quand il est activé. Un clic ouvre l'onglet Éclairage. |
| MCP | Affiché quand le serveur MCP de cette fenêtre est actif. |
| Côté droit | Images par seconde (quand **Afficher les images par seconde** est activé), et le verrou de texture, le magnétisme et la taille de grille quand la barre d'outils, ou cette partie de la barre, est masquée. |

## Console

Le panneau console en bas (`` ` `` ou **Fenêtre > Console**) liste les messages de l'éditeur : chargement de la map, montage du contenu, résultats de compilation, erreurs des tâches de fond et de la carte graphique. Des boutons de filtre affichent ou masquent chaque type (info, succès, avertissement, erreur), un champ filtre par texte, et des boutons copient les messages visibles ou vident la console. Les mêmes avertissements et erreurs sont aussi écrits dans le fichier journal (voir [Plantages et journaux](../troubleshooting/crashes-and-logs.md)). Elle se place sous les vues uniquement, entre la palette d'outils et le dock de droite : elle ne recouvre jamais le dock de droite, qui garde sa hauteur complète jusqu'à la barre d'état. Tirez son bord supérieur pour la redimensionner ; sa hauteur est limitée pour laisser de la place aux vues.

## Notifications

Les messages courts (toasts) s'empilent dans le coin inférieur droit : « Enregistré », « Sauvegarde automatique effectuée », résultats d'opérations, avertissements. Cliquez sur l'un d'eux pour le fermer.

## Carte de chargement

Pendant le montage du contenu du jeu ou l'ouverture d'une grosse map, une carte apparaît au milieu des vues avec l'étape, une barre de progression avec le nombre d'archives et d'addons ouverts, et celui en cours d'ouverture. Elle ne s'affiche que si le chargement dure plus d'un instant, et les menus, les panneaux et le reste de la fenêtre restent utilisables. L'ouverture d'une map peut être interrompue avec **Annuler**.

## Pages liées

- [Votre première map](first-map.md)
- [Fenêtre des paramètres](../features/settings-dialog.md)
- [Thèmes et langues](../features/themes-languages.md)
- [Raccourcis clavier](../features/keyboard-shortcuts.md)
- [Palette de commandes](../features/command-palette.md)
- [Actions et raccourcis](../reference/actions-shortcuts.md)

---
[Retour à l'index de la documentation](../README.md)
