[Documentation](../README.md) > Fonctionnalités > Caméras de skybox | [English](../../en/features/sky-cameras.md) | [Русский](../../ru/features/sky-cameras.md)

# Caméras de skybox

Une `sky_camera` se trouve dans une petite pièce (la skybox 3D) que le jeu dessine autour de la map à son échelle. Hammer Pro affiche la skybox 3D dans la vue 3D, gère les maps à plusieurs sky_camera, et fournit une entité de changement de skybox, `point_hammerpro_skybox`, qui change la skybox 3D en jeu (jour et nuit, une skybox par zone).

## Accès

| Action | Menu | Raccourci par défaut |
| --- | --- | --- |
| **Skybox 3D** (aperçu) | **Affichage > Options d'affichage > Skybox 3D** | `Alt+V` |
| **Aperçu du ciel** (ciel 2D) | **Affichage > Options d'affichage > Aperçu du ciel** | aucun |
| **Caméra de la skybox 3D** | **Affichage > Caméra de la skybox 3D** (visible quand la map a au moins deux sky_camera) | aucun |
| **Utiliser en jeu la sky_camera affichée** | **Affichage > Options d'affichage**, ou le sous-menu **Caméra de la skybox 3D** | aucun |

## Aperçu de la skybox 3D

Avec **Skybox 3D** activé (par défaut, `render.skybox_3d`), la pièce autour de la sky_camera est dessinée autour de la map à son `scale`, comme en jeu. **Aperçu du ciel** (`render.sky_preview`, activé) dessine derrière le ciel 2D du `skyname` du worldspawn. Les faces de ciel (`toolsskybox`) gardent leur texture d'outil, sauf si **Voir le ciel à travers toolsskybox** est activé (**Paramètres > Rendu**). `Alt+V` affiche « Skybox 3D affichée » ou « Skybox 3D masquée ».

Avec **Aperçu du brouillard** activé, la skybox 3D prend le brouillard de sa sky_camera (**Fog Enable**, couleurs, mélange, direction, début, fin, densité max), mesuré en unités de la skybox, et jamais le brouillard de la map, comme en jeu ; voir [Brouillard de la map](render-modes-display.md#brouillard-de-la-map).

Une sky_camera sans `scale` valide (un nombre entier d'au moins 1) utilise 16, la valeur par défaut du FGD, et est marquée « non valide » dans les menus.

## Plusieurs sky_camera

Une map peut contenir plusieurs sky_camera, chacune dans sa pièce. Garry's Mod et les jeux Source 2013 affichent la skybox 3D de **la dernière sky_camera de la map** (celle créée en dernier). [Vérifier les problèmes](check-problems.md) vous le rappelle quand la map en a plusieurs.

Le sous-menu **Affichage > Caméra de la skybox 3D** liste :

- **Comme en jeu : <caméra>** : l'aperçu suit la caméra que le jeu utilise (par défaut) ;
- chaque sky_camera, nommée « nom (n°id, échelle N) » ou « sky_camera n°id (échelle N) », celle du jeu marquée « utilisée en jeu ». En choisir une l'affiche (et active l'aperçu de la skybox 3D) ;
- **Utiliser en jeu la sky_camera affichée** : fait de la caméra affichée celle du jeu en la plaçant après les autres entités de la map. Une étape d'annulation. Le message « Le jeu utilise maintenant <caméra> » le confirme.

## Le changement de skybox : point_hammerpro_skybox

Le jeu affiche une seule skybox 3D à la fois. `point_hammerpro_skybox` est une entité Lua de Garry's Mod que Hammer Pro ajoute à la liste des entités (son FGD est chargé après ceux du jeu) pour afficher la skybox 3D d'une autre `sky_camera` quand elle reçoit `Activate`.

Mise en place :

1. Donnez un **Name** à la `sky_camera` (Hammer Pro ajoute une clé `targetname` à `sky_camera` pour cela).
2. Placez un `point_hammerpro_skybox` et mettez ce nom dans son **Sky camera**. Une ligne les relie dans les vues.
3. Envoyez `Activate` depuis un trigger, un bouton, un `logic_timer`...

Clés et flags (les libellés viennent du FGD, en anglais) :

| Clé | Effet | Défaut |
| --- | --- | --- |
| **Name** | Nom du changement de skybox. | |
| **Sky camera** (`skycamera`) | Nom de la `sky_camera` dont ce changement affiche la skybox 3D. | |
| **Mode** (`mode`) | **Global (everyone)** : change la skybox 3D pour tous les joueurs. **Per player** : dessinée sur chaque client, pour le joueur qui a activé le changement, donc plusieurs joueurs peuvent voir des skybox différentes en même temps. Ce mode dessine la géométrie de la skybox 3D de la caméra choisie ; le ciel 2D lointain et le brouillard restent ceux de la map. | Global |
| **Start active (global)** (spawnflag 1) | Affiche cette skybox au démarrage de la map (mode global seulement). | Désactivé |

| Entrée | Effet |
| --- | --- |
| `Activate` | Affiche la skybox 3D de la sky_camera. |
| `Deactivate` | Réaffiche la skybox 3D d'origine de la map, quand ce changement est celui affiché. |
| `Toggle` | Active si inactif, sinon désactive. |

| Sortie | Quand |
| --- | --- |
| `OnActivated` | La skybox de ce changement est affichée. |
| `OnDeactivated` | La skybox de ce changement n'est plus affichée. |

À la compilation, chaque changement reçoit les clés de sa sky_camera (`sky_origin`, `sky_scale`, clés de brouillard...), pour fonctionner en jeu sans chercher la caméra. Les clés de brouillard de la caméra sont copiées en mode global.

### Livrer le script

Le changement a besoin de son Lua en jeu (`lua/entities/point_hammerpro_skybox.lua` et `lua/autorun/hammerpro_skybox.lua`). Hammer Pro le livre pour vous :

- les compilations de test le mettent dans l'**Addon de développement** (réglages de compilation) ;
- **Construire et enregistrer en contenu** l'ajoute à l'addon quand la map compilée a un changement de skybox.

Le journal de compilation vous prévient quand une map avec des changements de skybox est compilée sans l'addon de développement. [Vérifier les problèmes](check-problems.md) signale un changement dont la sky_camera n'existe pas, et les changements par joueur (si le serveur a un addon qui dessine aussi le ciel, un addon de météo par exemple, ils peuvent entrer en conflit).

## Astuces

- Une map, jour et nuit : deux pièces de skybox, deux sky_camera nommées, deux changements, déclenchés par un `logic_timer` ou un bouton.
- Pour changer la caméra que le jeu utilise sans changement de skybox, affichez-la et choisissez **Utiliser en jeu la sky_camera affichée**.
- Outils MCP : `list_sky_cameras`, `preview_sky_camera`, `use_sky_camera_in_game`, `create_skybox_switch` (voir la [référence des outils MCP](../../MCP.md), en anglais).

## FAQ

**Le jeu affiche la mauvaise skybox 3D.** Le jeu utilise la dernière `sky_camera` de la map. Affichez la bonne dans **Affichage > Caméra de la skybox 3D** et choisissez **Utiliser en jeu la sky_camera affichée**.

**Le changement de skybox ne fait rien en jeu.** Son Lua manque : activez **Addon de développement** pour les compilations de test, ou publiez avec **Construire et enregistrer en contenu**. Vérifiez aussi que **Sky camera** correspond au **Name** de la caméra.

**Le changement marche-t-il dans d'autres jeux ?** Non. C'est une entité Lua de Garry's Mod.

## Pages liées

- [Modes de rendu et options d'affichage](render-modes-display.md)
- [Construire et enregistrer en contenu](build-content-release.md)
- [Fenêtre Compiler et lancer](../compiler/compile-window.md)
- [Propriétés des entités](entity-properties.md)

---
[Retour à l'index de la documentation](../README.md)
