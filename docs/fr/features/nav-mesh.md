[Documentation](../README.md) > Fonctionnalités > Nav mesh | [English](../../en/features/nav-mesh.md) | [Русский](../../ru/features/nav-mesh.md)

# Nav mesh

Le nav mesh (fichier `.nav`) indique aux PNJ et aux bots où ils peuvent marcher. Hammer Pro en génère un à partir de la map sans lancer le jeu, et dessine n'importe quel fichier `.nav` sur la map pour repérer les trous et les zones coupées du reste.

## Accès

| Action | Menu | Raccourci par défaut |
| --- | --- | --- |
| **Générer le nav mesh** | **Outils > Placement et props > Générer le nav mesh** | aucun |
| **Charger un nav mesh...** | **Map > Fuites et visibilité > Charger un nav mesh...** | aucun |
| **Afficher le nav mesh** | **Affichage > Diagnostics > Afficher le nav mesh** | aucun |

## Génération

**Outils > Placement et props > Générer le nav mesh** construit le maillage en arrière-plan et écrit `<map>.nav` à côté du VMF (là où l'affichage et **Construire et enregistrer en contenu** le cherchent). Enregistrez d'abord la map. À la fin, « Nav mesh généré : N zones en N ms » s'affiche et **Afficher le nav mesh** s'active.

Le fonctionnement suit celui de `nav_generate` de Valve :

- la marche part des points d'apparition (classes qui commencent par `info_player_`, et `gmod_player_start`). Une map sans point d'apparition donne « Aucun point d'apparition dans la map : ajoutez un info_player_start là où les joueurs marchent. » ;
- le sol est échantillonné toutes les 25 unités. Un échantillon est gardé là où un joueur tient (debout ou accroupi), et deux échantillons sont reliés quand un joueur peut marcher, monter une marche (jusqu'à 18 unités), sauter ou descendre de l'un à l'autre ;
- les échantillons sont regroupés en rectangles de sol presque plat (les zones), reliés le long de leurs bords.

Ce qui bloque les joueurs : les brushes du monde visibles, les entités de brush `func_detail`, `func_brush`, `func_wall`, `func_wall_toggle` et `func_lod`, les displacements, et les props statiques solides (sous forme de boîtes). Les portes, objets cassables et brushes mobiles sont ignorés (ils s'ouvrent, cassent ou bougent ; le générateur du jeu les traverse aussi). Les joueurs traversent les textures trigger, hint, skip, area portal, occluder, NPC clip, fog, block LOS et block light, ainsi que l'eau.

Avertissements dans la console : les points d'apparition sans sol en dessous sont ignorés, et une très grande map s'arrête à la limite d'échantillons.

## Afficher un nav mesh

**Afficher le nav mesh** dessine le maillage dans toutes les vues. Le fichier est celui choisi avec **Map > Fuites et visibilité > Charger un nav mesh...**, sinon le `.nav` à côté de la map, sinon celui du dossier `maps` du jeu (aussi à côté d'un BSP décompilé). Il est relu quand il change sur le disque (un fil en arrière-plan regarde les fichiers possibles toutes les 1,5 seconde et ne réveille l'éditeur que quand l'un d'eux change ou apparaît : afficher le nav mesh ne coûte aucun rafraîchissement tant que rien ne change). Hammer Pro lit les versions de nav 1 à 16 (Garry's Mod écrit la version 16).

| Dessin | Sens |
| --- | --- |
| Contour vert | Praticable. |
| Bleu | Accroupi. |
| Jaune | Saut. |
| Rouge | À éviter. |
| Cyan | Escalier. |
| Magenta | Coupée de la partie principale du maillage (là où les joueurs apparaissent, sinon la plus grande partie). |
| Ligne blanche | Connexion dans les deux sens. |
| Ligne orange avec une flèche | Connexion à sens unique. |
| Cadre violet | Échelle. |

Les zones sont remplies dans la vue 3D et en contour dans les vues 2D. La barre d'état affiche « Zones nav : N » (et « isolées : N ») ; la console liste les zones, échelles, liens à sens unique et liens vers des zones absentes du maillage chargé.

## Options

**Outils > Options... > Nav mesh et particules** (`[lab]` dans `settings.toml`) :

| Option | Effet | Défaut |
| --- | --- | --- |
| **Afficher le nav mesh** (`show_nav_mesh`) | Dessine le maillage. | Désactivé |
| **Remplir les zones dans la vue 3D** (`nav_fill`) | Sans remplissage, seuls les contours des zones sont dessinés. | Activé |
| **Connexions entre les zones** (`nav_connections`) | Blanc : dans les deux sens. Orange avec une flèche : à sens unique. | Activé |

## Astuces

- Les zones magenta sont coupées de là où les joueurs apparaissent : comblez l'écart qui les isole, ou retirez-les en jeu (l'astuce `nav_flood_select` du wiki Valve les trouve).
- La [fenêtre de publication](build-content-release.md) peut aussi construire le nav mesh dans une session de jeu, et sa liste de vérification contrôle que la map en a un.
- Outil MCP : `nav_mesh` (le maillage affiché, `show` pour l'afficher ou le masquer, `load` pour charger un fichier `.nav` du dossier de la map, `generate` pour lancer **Générer le nav mesh** en arrière-plan, puis `generating` et `last_generation` avec les zones et la durée ou l'erreur ; voir la [référence des outils MCP](../../MCP.md), en anglais).

## FAQ

**La génération dit qu'aucun sol praticable n'a été trouvé.** Les points d'apparition flottent au-dessus du vide, ou sont dans un solide. Posez un `info_player_start` sur le sol.

**Le maillage passe à travers mes portes.** Les portes sont ignorées exprès, comme dans le générateur du jeu.

**Faut-il le jeu pour générer ?** Non. Le générateur lit la map dans l'éditeur.

## Pages liées

- [Construire et enregistrer en contenu](build-content-release.md)
- [Lancer le jeu](game-launch.md)
- [Fuites et pointfiles](leaks-pointfiles.md)
- [Référence des paramètres](../reference/settings.md)

---
[Retour à l'index de la documentation](../README.md)
