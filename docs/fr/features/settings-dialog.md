[Documentation](../README.md) > Fonctionnalités > Fenêtre des paramètres | [English](../../en/features/settings-dialog.md) | [Русский](../../ru/features/settings-dialog.md)

# Fenêtre des paramètres

La fenêtre **Paramètres** regroupe toutes les options de Hammer Pro, rangées en pages. Les changements s'appliquent tout de suite et sont enregistrés automatiquement. Cette page liste chaque page et son contenu ; le tableau complet des clés, valeurs par défaut et effets est dans la [référence des paramètres](../reference/settings.md).

## L'ouvrir

- **Outils > Options...** (`Ctrl+,`).
- **Personnaliser...** dans la référence des raccourcis (`F1`) ouvre la page **Clavier**.
- Certains segments de la barre d'état et certains messages ouvrent directement la page concernée.

## La fenêtre

La liste des pages est à gauche, rangée en sections : **Application** (Général, Mises à jour, Stockage, Clavier, Saisie), **Jeu et contenu** (Jeu, Entités et navigateurs), **Vues et rendu** (Vues, Aides, Rendu, Éclairage, Nav mesh et particules), **Édition** (Édition, Sélection, Brushes, Sommets et displacements, Instances, Cordons et visgroups, Prefabs), **Compilation et vérification** (Compilation, Santé de la map) et **Intégrations** (Discord, Blender, Éditeur de VTF, MCP). La page choisie est à droite.

**Chercher un paramètre**, le champ au-dessus de la liste, trouve une option par son nom : tapez quelques lettres et la liste ne montre plus que les pages qui contiennent une option correspondante, chacune avec le texte trouvé sous son nom (survolez-le pour le lire en entier). La casse et les accents sont ignorés ; la recherche utilise la langue de l'interface et aussi les noms anglais. Le texte affiché sous une page privilégie un nom d'option qui commence par ce que vous avez tapé. Videz le champ pour revoir toutes les pages. Le pied de la fenêtre contient :

| Contrôle | Effet |
| --- | --- |
| **Valeurs par défaut** | Réinitialise toutes les options de la page affichée. Un premier clic l'arme (**Cliquez à nouveau pour confirmer**, pendant 4 secondes), le second réinitialise. Toutes les pages l'ont sauf **Jeu**. |
| **Appliquer et recharger le contenu** | Apparaît après un changement des réglages du jeu : remonte le contenu du jeu et recharge les définitions d'entités avec les nouveaux réglages. Fermer la fenêtre les applique aussi. |
| *Les modifications sont enregistrées automatiquement* | Rappel : il n'y a pas de bouton OK. |
| **Fermer** | Ferme la fenêtre (`Échap` aussi). |

Les paramètres sont enregistrés dans `settings.toml`, dans `%APPDATA%\HammerPro` sous Windows (voir [Fichiers et dossiers](../reference/files-and-folders.md)). Le fichier est écrit de façon atomique ; s'il est illisible, Hammer Pro démarre avec les valeurs par défaut et note un avertissement dans le journal. Le thème et la langue s'appliquent immédiatement.

**Chercher un paramètre** cherche dans les options, les explications et les messages de toutes les pages sauf **Mises à jour** et **Stockage** (ouvrez-les directement) ; chercher le nom d'une action trouve la page **Clavier**, qui a son propre champ (**Rechercher une action ou un raccourci**) pour trouver le raccourci lui-même. La [référence des paramètres](../reference/settings.md) et l'outil MCP `get_settings` listent toutes les clés.

## Les pages

Ce que contient chaque page :

| Page | Contenu |
| --- | --- |
| **Général** | **Apparence** : langue, thème, couleur d'accent, échelle de l'interface, images par seconde, cadres de fenêtre natifs, démarrage agrandi, palette d'outils sur une ou deux colonnes, barre d'outils sur une ligne ou jusqu'à deux lignes (avec **Réinitialiser la barre d'outils**). **Image de fond** : image, opacité, position, zoom, ajuster, masquer sur la vue 3D. **Sauvegarde automatique** : toutes les N minutes, copies conservées par map (et le dossier). **Maps** : ouvrir les maps dans de nouveaux onglets. **Maps récentes** : combien, **Vider la liste des maps récentes**. Voir [Thèmes et langues](themes-languages.md), [Sauvegarde automatique et plantages](autosave-crash.md), [Plusieurs maps](multiple-maps.md). |
| **Discord** | Statut Discord (Rich Presence) : afficher l'activité, le nom de la map, le temps écoulé, l'outil actif, ID de l'application (vide : celle de Hammer Pro), lien du bouton, et la ligne **État :** (connecté, Discord n'est pas lancé...). Voir [Statut Discord](discord.md). |
| **Mises à jour** | **Rechercher les mises à jour au démarrage**, **Rechercher maintenant**, **Afficher la mise à jour**, la ligne d'état et la version ignorée (**Ne plus ignorer**). Voir [Mises à jour](updates.md). |
| **Éditeur de VTF** | L'éditeur VTF externe (**Parcourir...**, **Détecter**, les éditeurs trouvés), et **Toujours utiliser l'éditeur intégré**. Voir [Éditeur de VTF](vtf-editor.md). |
| **Lien avec Blender** | Le programme Blender (**Parcourir...**, **Détecter**), le format de fichier (glTF, OBJ, FBX), le **Lien direct**, et les boutons **Envoyer vers Blender**, **Mettre à jour depuis Blender**, **Arrêter le lien direct avec Blender** et **Installer l'add-on Blender**. Voir [Lien avec Blender](blender.md). |
| **Stockage** | Espace disque utilisé par Hammer Pro, du plus gros au plus petit, avec **Actualiser**, **Ouvrir le dossier**, **Supprimer** par ligne et **Supprimer la sélection**, chacun après confirmation. Voir [Stockage](storage.md). |
| **Jeu** | Le jeu pour lequel les maps sont faites (avec **Rechercher à nouveau**), le **Dossier de Garry's Mod**, **Contenu** (options de montage, contenu téléchargé des serveurs), **Analyse du contenu** (l'index enregistré, le mode d'analyse, **Reconstruire l'index** ; voir [Index et analyse du contenu](content-index.md)), **Dossiers de textures personnalisés**, **Dossiers de jeu supplémentaires**, **Jeux Steam par AppID**, **Définitions d'entités (FGD)** (FGD principal et FGD supplémentaires), **Maps** (dossier des maps), **Tests en jeu** (le pont Hammer Pro et **Apparaître à la caméra de l'éditeur**). Voir [Configuration du jeu](../getting-started/game-setup.md) et [Lancement du jeu](game-launch.md). |
| **Vues** | **Contrôles** : contrôles de la caméra (Hammer Pro, Hammer, Hammer++), inverser l'axe X, temps d'accélération de la caméra, Ctrl + molette en 2D (taille de grille ou zoom), zoomer les vues 2D ensemble, pas de la taille de grille, unités des tailles. **Caméra** : champ de vision, vitesse, multiplicateur rapide, sensibilité de la souris, inverser l'axe Y, touches de la caméra (Auto, AZERTY, QWERTY), distance d'affichage, zoomer vers le curseur, contour des objets sous la souris. **Rendu** : anticrénelage (MSAA), grille dans la vue 3D, accentuer les lignes toutes les N, lignes de 1024 unités. **Couleurs** : fond 2D, fond 3D, sélection. Voir [Caméra et navigation](camera-navigation.md). |
| **Aides** | Aides des entités (afficher, pour les entités sélectionnées ou toutes), objets d'éditeur, connexions entrées/sorties (couleurs masquées, valides, cible introuvable, sortie inconnue), overlays et cordes (afficher, simuler), noms des entités. Voir [Modes de rendu et affichage](render-modes-display.md). |
| **Rendu** | Vue 3D (plan proche, distance des modèles, opacité de la grille, rayon de limite, lumières avec ombres, valeurs de lumière HDR, lumière du ciel seulement sous le ciel, ombres des props statiques, grille 3D sur les faces, limite circulaire, ignorer la géométrie cachée, ciel, skybox 3D, eau, normal maps, reflets des cubemaps, brouillard, fog_volume et leur texture, poissons et poissons qui nagent, éclairage de la vue joueur, effets de rendu, fondu des modèles, matériaux opaques, pentes praticables, contour de la sélection, modèles de collision), vues 2D (modèles, opacité du remplissage, taille des icônes, densité de la grille, points des sommets, icônes des entités, grille en points, couleurs des visgroups, couleurs des objets en 2D et en 3D filaire...), textures outils masquées en 3D, displacements en 2D, limites d'images par seconde et synchronisation verticale, mémoire graphique (budget, garder les autres onglets sur la carte graphique, charger le détail des textures selon la distance). Voir [Modes de rendu et affichage](render-modes-display.md) et [Performances](../troubleshooting/performance.md). |
| **Éclairage** | Aperçu de l'éclairage précalculé (précalculer à l'activation, précalculer d'abord la zone visible, préparer en arrière-plan, pause quand la fenêtre est réduite ou qu'une autre fenêtre est active, threads, priorité du précalcul, délai de recalcul d'une lumière déplacée, envoi de lightmaps par image, luxels en pleine résolution), budget des lightmaps (et sa jauge dans la barre d'état), très petite échelle de lightmap, styles de lumière. Voir [Aperçu de l'éclairage](lighting-preview.md). |
| **Nav mesh et particules** | Affichage du maillage de navigation (afficher, remplir les zones, connexions) et des particules (afficher, sélection uniquement). Voir [Nav mesh](nav-mesh.md) et [Particules](particles.md). |
| **Édition** | **Nouveaux objets** : matériau par défaut (avec **Utiliser l'actuel**), échelle de texture, échelle de lightmap, entité ponctuelle, entité de brush, couleur des nouveaux brushes. **Comportement** : niveaux d'annulation, verrouillage des textures, verrouillage de l'échelle des textures, décaler avec les flèches, décaler les objets collés, annuler restaure la sélection. |
| **Sélection** | Transformations (pas de rotation, touche de rotation libre, taille des poignées) et comportement (garder les visgroups au clonage, Maj+flèches clone, poignées d'étirement 3D, gizmo en repère local, outil caméra cliquer pour zoomer). Voir [Outil de sélection](selection-tool.md). |
| **Brushes** | Primitives orientées selon la vue de tracé ; mesures de l'outil de découpe et matériau des faces de coupe. Voir [Outil bloc](block-tool.md) et [Outil de découpe](clip-tool.md). |
| **Sommets et displacements** | Aperçu en direct et verrouillage UV de l'outil sommets ; peinture des mélanges à 4 textures et hauteurs du bruit des displacements. Voir [Outil sommets](vertex-tool.md) et [Outil displacement](displacement-tool.md). |
| **Instances** | Contenu des `func_instance` (encadrement, map autour d'une instance modifiée), dossier par défaut des nouvelles instances, options de fusion. Voir [Instances](instances.md). |
| **Entités et navigateurs** | Propriétés des entités (séparateur ESC, coller le nom), décimales et pas de l'application de textures, navigateur de textures (dossiers masqués, alpha sur damier), navigateurs de modèles, sons et particules (contenu Garry's Mod uniquement, taille des vignettes, volume d'écoute, lire à la sélection). Voir [Propriétés des entités](entity-properties.md) et [Navigateur de textures](texture-browser.md). |
| **Cordons et visgroups** | Matériau du cordon, ombre hors des cordons, noms des cordons ; options de Déplacer vers un visgroup. Voir [Cordons](cordons.md) et [Visgroups](visgroups.md). |
| **Prefabs** | Dossier de la bibliothèque de prefabs, autres dossiers, prefabs de Garry's Mod, grouper les prefabs insérés, onglet Prefabs ; dossiers des définitions de Sprinkle. Voir [Prefabs](prefabs.md) et [Outil sprinkle](sprinkle-tool.md). |
| **Compilation** | **Valeurs par défaut (F9 et fenêtre de compilation)** : préréglage, plage dynamique, étapes, éclairage des props statiques. **Vitesse** : threads, garder un cœur pour l'éditeur, recompilation intelligente, compiler pendant les pauses. **Options** : s'arrêter quand la map fuit, compiler les objets masqués, cordon seulement, matériau de scellement du cordon, intégrer le contenu personnalisé, addon de développement, exporter les matériaux personnalisés, jouer tout de suite, historique des durées, pointfile de fuite, lancer le jeu et arguments du jeu. **Arguments supplémentaires du compilateur**, **Options du compilateur**, **Compilateur** (jeu et dossier des maps). Voir [Fenêtre de compilation](../compiler/compile-window.md). |
| **Santé de la map** | Vérifications (pendant l'édition, fuites pendant l'édition, délai après une pause des modifications, pause quand la fenêtre est réduite, chemin de fuite, objets visibles seulement), profil de limites, options des fichiers de portails. Voir [Vérifier les problèmes](check-problems.md). |
| **Clavier** | Jeu de raccourcis, recherche, tout réinitialiser, chaque action avec son raccourci. Voir [Raccourcis clavier](keyboard-shortcuts.md). |
| **Saisie** | Mode pavé tactile (Auto, Activé, Désactivé) avec ce qui a été détecté, défilement à deux doigts en 3D (orbite ou déplacement), vitesses de défilement, de zoom par pincement et d'orbite, et la liste des gestes du pavé tactile. Voir [Pavé tactile](trackpad.md). |
| **MCP** | État et bouton du serveur MCP, démarrage avec Hammer Pro, **Installation et configuration...**, outils proposés aux applications d'IA. Voir [Serveur MCP](../reference/mcp-server.md). |

Les pages ajoutées par des extensions prennent leur place dans la même liste (voir [Étendre Hammer Pro](../development/extending.md)).

## MCP

`get_settings` lit tous les paramètres (ou une table, ou une clé) et `set_setting` en change un par son chemin, comme `health.live_checks`. Voir la [référence des outils MCP](../../MCP.md) (en anglais).

## Astuces

- **Valeurs par défaut** ne réinitialise que la page affichée : testez sur une page sans perdre le reste.
- Après avoir changé le dossier du jeu ou les options de contenu, cliquez **Appliquer et recharger le contenu** pour voir le résultat sans fermer la fenêtre.
- Sauvegardez `settings.toml` avant un gros changement ; le copier sur un autre ordinateur emporte toutes vos options (raccourcis compris).

## FAQ

**Y a-t-il un bouton OK ou Appliquer ?** Non. Chaque changement est appliqué et enregistré tout de suite. Seul le contenu du jeu attend **Appliquer et recharger le contenu** (ou la fermeture de la fenêtre).

**J'ai tout cassé. Comment tout remettre à zéro ?** Utilisez **Valeurs par défaut** sur chaque page, ou fermez Hammer Pro et supprimez `settings.toml` (l'assistant du premier démarrage reviendra).

**Pourquoi la page Jeu n'a-t-elle pas Valeurs par défaut ?** La réinitialiser oublierait vos dossiers de jeu et votre configuration du contenu ; changez ces options une par une.

## Pages liées

- [Référence des paramètres](../reference/settings.md)
- [Fichiers et dossiers](../reference/files-and-folders.md)
- [Raccourcis clavier](keyboard-shortcuts.md)
- [Thèmes et langues](themes-languages.md)

---
[Retour à l'index de la documentation](../README.md)
