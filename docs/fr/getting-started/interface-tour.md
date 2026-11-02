[Documentation](../README.md) > Prise en main > Tour de l'interface | [English](../../en/getting-started/interface-tour.md)

# Tour de l'interface

La fenêtre de Hammer Pro reprend la disposition de Hammer et de Hammer++ (menus dans le même ordre, outils à gauche, quatre vues, propriétés à droite) : si vous venez de ces éditeurs, chaque bouton est là où vous l'attendez. L'interface est en noir et blanc (thème sombre ou clair) : la couleur est réservée à votre map, aux textures et aux modèles.

## Cadre de la fenêtre et barre de titre

Hammer Pro dessine son propre cadre de fenêtre : coins arrondis, ombre douce et barre de titre qui contient le logo Hammer Pro, les menus, le titre `<nom de la map> - Hammer Pro` et les boutons réduire, agrandir/restaurer et fermer. Le survol du titre affiche le chemin complet de la map (« Pas encore enregistrée » pour une nouvelle map) et « Modifications non enregistrées » quand la map a été modifiée.

Si ce cadre pose problème (bureau à distance, certains gestionnaires de fenêtres), activez **Paramètres > Général > Cadres de fenêtre natifs** : la barre de titre du système revient et les menus passent dans une barre de menus en dessous. Voir [Cadres de fenêtre](../features/window-frames.md).

## Barre de menus

| Menu | Contenu |
|---|---|
| **Fichier** | Nouvelle map, Ouvrir une map..., Maps récentes, Enregistrer, Enregistrer sous..., Enregistrer comme nouvelle version..., Fermer, Fermer les autres maps, import et export (Importer une map dans la map actuelle..., Exporter la sélection en VMF..., Exporter une copie..., Exporter à nouveau, Exporter en OBJ..., Ouvrir une map compilée (BSP)..., Ouvrir une archive..., Importer un modèle (FBX, OBJ, glTF)..., Importer les props depuis le jeu), compilation et lancement (Compiler et lancer..., Compiler maintenant, Annuler la compilation, Compilation par lots..., Lancer la map dans le jeu, Recharger la map dans le jeu, Construire et enregistrer en contenu..., Publier sur le Workshop...), Quitter. |
| **Édition** | Annuler, Rétablir, presse-papiers (Couper, Copier, Coller, Collage spécial..., Supprimer, Dupliquer, Copier les keyvalues, Coller les keyvalues), sélection (Tout sélectionner, Effacer la sélection, Inverser la sélection, sélection de faces précédente et suivante), **Mode de sélection** (Groupes, Objets, Solides, Ignorer les groupes), Rechercher des entités..., Rechercher et remplacer dans les entités..., Propriétés, Modifier la couleur..., Définir la distance maximale... |
| **Map** | Grille (Afficher la grille, Magnétisme de grille, Grille plus fine, Grille plus large, Grille 3D sur les faces), **Grilles** (globale, par vue, grilles personnalisées, gestionnaire de grilles), Propriétés de la map..., Informations sur la map, Vérifier les problèmes, Rapport des entités, Ressources de la map, Vérifier les fuites, Aller à un id de brush ou d'entité..., Soundscapes..., comparaisons et suggestions, pointfiles, fichiers de portails, nav mesh, et **Éclairage** (précalcul, cubemaps). |
| **Affichage** | Masquer la sélection, Masquer le reste, Tout afficher, cordons et visgroups, centrage et zoom des vues, Agrandir la vue, **Type de vue** (dessus, côté, face, 3D, vues logiques), **Mode de rendu 3D**, **Options d'affichage**, Vue joueur, **Panneaux** (panneaux et barres affichés), **Langue**. |
| **Outils** | Les outils (Sélection, Caméra, Entité, Bloc, Polygone, Courbe, Application de texture, Décal, Overlay, Découpe, Extruder / creuser, Biseau, Vertex, Displacement, Chemin, Cordon, Sprinkle), opérations sur les brushes (Grouper, Dégrouper, Creuser, Rendre creux, Fusionner, Lier à une entité, Renvoyer au monde...), **Aligner**, **Retourner**, **Instances**, Transformer..., prefabs, placement et props, Rechercher et remplacer des textures..., Recharger le contenu depuis le disque, le serveur MCP, Palette de commandes..., Options... |
| **Fenêtre** | Console, **Disposition** (Enregistrer la disposition..., Réinitialiser la disposition), Map suivante, Map précédente. |
| **Aide** | Raccourcis clavier, Documentation, Aide de l'entité, Signaler un problème, Assistant de configuration..., À propos de Hammer Pro. |

Toutes les commandes, avec leur raccourci par défaut, sont listées dans [Actions et raccourcis](../reference/actions-shortcuts.md). La [palette de commandes](../features/command-palette.md) (`Ctrl+P`) les retrouve par leur nom.

## Barre d'outils

La barre d'outils sous les menus reprend les barres Map View et Map Operations de Hammer, groupe après groupe :

1. Nouvelle map, Ouvrir une map, Enregistrer.
2. Afficher la grille, Grille 3D sur les faces, la taille de la grille (plus fine, un champ où saisir n'importe quelle taille de 0,01 à 4096, plus large), Magnétisme de grille.
3. Annuler, Rétablir.
4. Creuser, Grouper, Dégrouper, Ignorer les groupes.
5. Masquer la sélection, Masquer le reste, Tout afficher.
6. Couper, Copier, Coller.
7. Utiliser les cordons, Outil cordon, Limite de distance circulaire.
8. Maillage des displacements en vue 2D, Praticabilité des displacements, Afficher le nodraw, Afficher les collisions des modèles, Verrouillage des textures, Verrouillage de l'échelle des textures.
9. **Compiler** : ouvre la fenêtre Compiler et lancer (le Run Map de Hammer).
10. Afficher les aides et sprites, Afficher les modèles, Afficher les textures outils, Afficher les objets d'éditeur, Skybox 3D, Afficher les particules, Simuler les cordes.
11. La liste des modes de rendu 3D, Vue joueur, Vérifier les problèmes.

Tout à droite de la barre du haut, la pastille **MCP** indique si le serveur MCP de cette fenêtre est actif, et son menu installe Hammer Pro dans les applications d'IA (voir [Serveur MCP](../reference/mcp-server.md)).

Sous la barre d'outils, la **ligne d'options** affiche le nom de l'outil actif et ses options. Les outils aux options volumineuses (Application de texture, Displacement...) les affichent plutôt dans l'onglet de l'outil du panneau de droite (« Les options de l'outil sont dans le panneau de droite. »).

## Palette d'outils

La bande verticale le long du bord gauche regroupe les outils sous forme d'icônes, séparés par un trait : se déplacer (Sélection, Caméra), créer (Entité, Bloc, Polygone, Courbe, Sprinkle), modifier (Application de texture, Appliquer la texture actuelle, Décal, Overlay, Découpe, Extruder / creuser, Biseau, Vertex, Displacement), outils spéciaux (Chemin, Cordon, puis Propper et Unpropper), et l'œil d'isolement (isoler la sélection, ou tout réafficher). L'infobulle de chaque bouton donne son nom et son raccourci. Quand la fenêtre est basse, les boutons s'aplatissent, puis la bande défile.

## Les quatre vues

Le centre de la fenêtre est une grille de 2x2 vues : la vue 3D en haut à gauche, Dessus (X/Y) en haut à droite, Face (Y/Z) en bas à gauche et Côté (X/Z) en bas à droite. Faites glisser les séparateurs pour les redimensionner (**Affichage > Réinitialiser le partage des vues** les remet en place). Chaque vue a un menu **Vue** dans son coin pour changer ce qu'elle affiche (n'importe quel quart peut aussi afficher la vue logique 2D). `Maj+Z` agrandit la vue sous la souris, et `F2` à `F5` changent son type. Voir [Caméra et navigation](../features/camera-navigation.md) et [Modes de rendu et affichage](../features/render-modes-display.md).

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
| Côté droit | Images par seconde (quand **Afficher les images par seconde** est activé), et le verrou de texture, le magnétisme et la taille de grille quand la barre d'outils est masquée. |

## Console

Le panneau console en bas (`` ` `` ou **Fenêtre > Console**) liste les messages de l'éditeur : chargement de la map, montage du contenu, résultats de compilation, erreurs des tâches de fond et de la carte graphique. Des boutons de filtre affichent ou masquent chaque type (info, succès, avertissement, erreur), un champ filtre par texte, et des boutons copient les messages visibles ou vident la console. Les mêmes avertissements et erreurs sont aussi écrits dans le fichier journal (voir [Plantages et journaux](../troubleshooting/crashes-and-logs.md)).

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
