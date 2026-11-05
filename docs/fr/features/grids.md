[Documentation](../README.md) > Fonctionnalités > Grilles | [English](../../en/features/grids.md) | [Русский](../../ru/features/grids.md)

# Grilles

La grille est ce sur quoi les outils s'alignent et ce que dessinent les vues 2D. Hammer Pro garde la grille unique de Hammer (par défaut) et ajoute une grille par vue, des grilles personnalisées nommées avec n'importe quel pas, origine et orientation, et des grilles attachées aux objets.

## Accès

| Action | Menu | Raccourci par défaut |
| --- | --- | --- |
| **Grille plus fine** | **Map > Grille plus fine** | `[` |
| **Grille plus large** | **Map > Grille plus large** | `]` |
| **Afficher la grille** | **Map > Afficher la grille** | `Maj+G` (et `Maj+R`, la touche de Hammer) |
| **Magnétisme de grille** | **Map > Magnétisme de grille** | `Maj+W` |
| **Grille 3D sur les faces** | **Map > Grille 3D sur les faces** | `P` |
| **Grille globale**, **Une grille par vue**, **Grille personnalisée**, **Grille de la sélection** | **Map > Grilles** | aucun |
| **Grille depuis la sélection**, **Grille depuis la face** | **Map > Grilles** | aucun |
| **Attacher la grille à la sélection**, **Détacher la grille de la sélection** | **Map > Grilles** | aucun |
| **Gestionnaire de grilles...** | **Map > Grilles > Gestionnaire de grilles...** | aucun |
| **Pivoter la grille de 45 degrés**, **Aligner la grille sur l'arête ou la face sélectionnée** | **Map > Grilles**, palette de commandes | aucun |

Avec les préréglages de touches Hammer et Hammer++, **Afficher la grille** est sur `Maj+R`.

La barre d'outils a **Afficher la grille**, **Grille 3D sur les faces**, les commandes de taille et **Magnétisme de grille** :

- le **bouton de grille** nomme la grille sur laquelle agissent les commandes de taille (« Grille » pour la grille globale, « Grille (Dessus) » pour la grille d'une vue, ou le nom d'une grille personnalisée) et ouvre le menu des grilles. Son infobulle indique la grille active ;
- les boutons plus fine et plus large, et un champ de taille : saisissez n'importe quelle taille de 0,01 à 4096 (2,5 ou 80 par exemple, la virgule marche comme séparateur décimal) puis `Entrée`.

**Ctrl + molette** dans une vue 2D change la taille de la grille (réglage **Ctrl + molette en 2D** de la page **Vues** des paramètres ; l'autre choix zoome toutes les vues 2D).

## Touches de taille de grille

`[` et `]` divisent et multiplient la taille de la grille par le **Pas de la taille de grille** (2 par défaut, de 1,25 à 10). Avec le facteur 2, elles suivent les pas de Hammer, les puissances de deux de 0,125 à 4096, quelle que soit la taille saisie (80 puis `[` donne 64). Avec un autre facteur, la taille est multipliée ou divisée et reste entre 0,01 et 4096.

## Modes de grille

Le mode décide sur quelle grille les outils s'alignent et ce que les vues dessinent. Choisissez-le dans **Map > Grilles**, le menu des grilles de la barre d'outils, le gestionnaire de grilles, ou **Paramètres > Vues > Grille**.

| Mode | Effet |
| --- | --- |
| **Grille globale** (défaut) | Une taille pour toutes les vues, comme Hammer (`edit.grid_size`, 16 par défaut). |
| **Une grille par vue** | Chaque vue (3D, dessus, face, côté) a sa propre taille, son origine et son intervalle de lignes principales : une grille précise dans une vue, une grille large dans une autre. Les commandes de taille changent la grille de la vue sous la souris, ou de la dernière vue utilisée. Le premier passage dans ce mode donne à chaque vue la taille globale. |
| **Grille personnalisée** | Une grille personnalisée nommée (la dernière choisie, sinon la première). Sans grille personnalisée : « Aucune grille personnalisée : créez-en une dans le gestionnaire de grilles ou depuis la sélection. » |
| **Grille de la sélection** | La grille attachée aux objets sélectionnés ; la grille globale s'ils n'en ont pas. |

## Grilles personnalisées

Une grille personnalisée a :

| Champ | Sens |
| --- | --- |
| **Nom** | Unique, sans tenir compte de la casse. |
| **Pas X Y Z** | Pas le long des axes propres de la grille, n'importe quelle taille avec décimales (12, 24, 1,5...). Des pas égaux changent ensemble ; maintenez `Maj` pour n'en changer qu'un. |
| **Origine** | Un nœud de la grille se trouve ici (unités du monde). |
| **Tangage Lacet Roulis** | Orientation, comme les angles d'une entité : suivre un bâtiment tourné (lacet) ou une pente (tangage, roulis). |
| **Ligne principale toutes les** | Toutes les N lignes, une ligne principale (0 : le réglage des vues). |
| **Limites** | **Min** et **Max** : la grille ne s'applique que dans cette boîte ; en dehors, la grille globale s'applique. **Ajuster à la sélection** les remplit. |
| **Couleur** | Couleur de ses lignes (par défaut : la couleur de grille du thème). |

Les grilles personnalisées appartiennent à la map (enregistrées dans le VMF dans un bloc `hammerpro_grids`, chaque modification est une étape d'annulation) ou aux paramètres, comme préréglages proposés dans toutes les maps. Le menu des grilles les liste sous la forme « nom (pas) » et « nom (pas, préréglage) ».

Deux commandes créent une grille personnalisée à partir de ce que vous avez sélectionné, et l'activent :

- **Grille depuis la sélection** : pour une entité ponctuelle, son origine et ses angles ; pour des brushes, les axes du premier solide (haut : sa face la plus tournée vers le haut, X : sa plus grande autre face) avec un nœud sur le coin de ses limites. Le pas est la taille de la grille globale. Nommée « Grille de sélection ».
- **Grille depuis la face** : la première face sélectionnée (outil texture) devient le plan XY de la grille (Z selon la normale de la face), X le long de sa plus longue arête, un nœud sur son coin. Nommée « Grille de face ».

## Grilles attachées aux objets

**Attacher la grille à la sélection** attache la grille personnalisée active aux objets sélectionnés (un préréglage est d'abord copié dans la map, dans la même étape d'annulation). **Détacher la grille de la sélection** la retire. La grille attachée est enregistrée sur chaque objet (`hammerpro_grid` dans son bloc editor).

Avec **Les objets utilisent leur grille** activé, modifier un objet qui porte une grille (déplacer, redimensionner, étirer, sommets, découpe) ou dessiner dessus s'aligne sur sa grille, quel que soit le mode. **Grille de la sélection** fait de la grille des objets sélectionnés la grille active.

## Le gestionnaire de grilles

**Map > Grilles > Gestionnaire de grilles...** ouvre la fenêtre **Grilles** :

- **Grille active :** les quatre modes en boutons ;
- **Les objets utilisent leur grille** et **Afficher la grille personnalisée dans la vue 3D** (la vue 3D dessine le plan d'une grille personnalisée ou par vue à son origine et dans son orientation) ;
- **Grilles des vues** : pour **Dessus**, **Face**, **Côté** et **3D**, la **Taille**, l'**Origine X Y Z** et les **Lignes principales** utilisées par **Une grille par vue** ;
- **Grilles personnalisées** : **Nouvelle grille**, **Grille depuis la sélection**, **Grille depuis la face**, la liste des grilles de la map et des préréglages, et tous les champs de la grille choisie (« enregistrée avec la map » ou « enregistrée dans les paramètres (toutes les maps) »), avec **Utiliser**, **Attacher à la sélection**, **Copier dans la map** ou **Enregistrer comme préréglage**, et **Supprimer**.

## Faire tourner la grille

La ligne **Rotation de la grille** du menu des grilles (le bouton de grille de la barre d'outils) et du gestionnaire de grilles tourne la grille active dans une vue 2D, comme Hammer de Source 2 :

- saisissez n'importe quel angle en degrés (décimales et valeurs négatives comprises) ou faites glisser le champ ; **0** revient à la grille droite, **45** est le bouton rapide ;
- **Pivoter la grille de 45 degrés** ajoute 45 degrés à l'angle actuel (deux fois donne un quart de tour, qui paraît de nouveau droit) ;
- **Aligner la grille sur l'arête ou la face sélectionnée** tourne la grille sur l'arête la plus longue de la face sélectionnée, du brush sélectionné ou sur le lacet de l'entité sélectionnée, vus dans cette vue 2D.

La grille tourne autour de l'axe de la vue (Z en vue de dessus, Y en vue de face, X en vue de côté) et autour de son origine. La vue utilisée est celle sous la souris, sinon la dernière utilisée. Ce qui tourne dépend de la grille active :

- **Grille globale** et **Une grille par vue** : l'angle de cette vue, gardé dans le champ `angle` de `[grid.per_view.top]` (front, side, perspective) de `settings.toml`. La taille reste la taille globale. Passer à une grille par vue garde les angles ;
- **Grille personnalisée** (et grille de la sélection) : ses **Tangage Lacet Roulis** changent, donc l'angle est enregistré dans la map avec la grille et chaque changement est une étape d'annulation.

La vue 2D dessine les lignes mineures et principales selon les axes tournés, coupées à la vue. Les axes de l'origine et les lignes de 1024 unités restent droits, comme les limites de la map. Tous les outils (déplacement, poignées de mise à l'échelle et de redimensionnement, bloc, découpe, sommets, extrusion, chemin, placement d'entités) s'alignent sur le réseau tourné, et **Ctrl + molette** change sa taille comme d'habitude. La rotation est une rotation du réseau dans le plan de la vue : une poignée de redimensionnement de brush aligne son point sur les nœuds tournés.

## Dessin de la grille

| Option | Où | Effet | Défaut |
| --- | --- | --- | --- |
| **Afficher la grille** (`edit.show_grid`) | Menu Map, `Maj+G` | Dessine la grille 2D. | Activé |
| **Magnétisme de grille** (`edit.snap_to_grid`) | Menu Map, `Maj+W` | Les outils s'alignent sur la grille. | Activé |
| **Accentuer les lignes de grille toutes les** (`view.grid_highlight_every`) | Paramètres > Vues | Intervalle des lignes principales de la grille 2D (2 à 64). | 8 |
| **Afficher les lignes de 1024 unités** (`view.show_1024_grid`) | Paramètres > Vues | Lignes toutes les 1024 unités. | Activé |
| **Grille dans la vue 3D** (`view.show_3d_grid`) | Paramètres > Vues | Une grille sur le plan z = 0 autour de la caméra, l'axe X en rouge et l'axe Y en vert. | Désactivé |
| **Grille 3D sur les faces (P)** (`render.grid_3d`) | Menu Map, `P`, Paramètres > Rendu | Dessine la grille sur les faces des brushes, à chaque pas de grille, plus claire à chaque pas majeur. | Désactivé |
| **Opacité de la grille 3D** (`render.grid_3d_opacity`) | Paramètres > Rendu | 0,05 à 1. | 0,45 |
| `render.grid_3d_fade_cells` | `settings.toml` | La grille 3D s'estompe au-delà de ce nombre de cases depuis la caméra. | 64 |
| **Grille en points** (`render.grid_dotted`) | Paramètres > Rendu | Dessine la grille 2D en points aux croisements de ses lignes. | Désactivé |
| **Surligner tous les 64 unités** (`render.grid_highlight_64`) | Paramètres > Rendu | Les lignes tous les 64 unités sont plus claires. | Désactivé |
| **Densité maximale de la grille** (`render.grid_min_px`) | Paramètres > Rendu | Les lignes plus proches que cela (2 à 32 px) sont masquées ; le pas supérieur s'affiche à la place. | 4 px |
| **Pas de la taille de grille** (`workflow.grid_step_factor`) | Paramètres > Vues | Facteur de `[` et `]`. | 2 |

Les réglages de grille sont dans la table `[grid]` de `settings.toml` : `mode`, `custom`, `per_view`, `presets`, `object_grids` (activé), `plane_3d` (activé).

## Plage de la grille et outils biseau, décalage et extrusion

La taille de la grille accepte toutes les valeurs de 0,01 à 4096 unités. Les distances à la souris du [biseau](bevel-tool.md), du décalage interactif et de l'extrusion interactive suivent la grille active (taille, grille personnalisée et rotation) mais n'utilisent jamais un pas inférieur à 1 ou supérieur à 1024, et chaque brush qu'ils créent a ses nouvelles faces sur cette grille. Pendant l'extrusion, `Alt` passe à la grille alternative, une grille alignée sur les axes de la moitié de la taille. Voir [Grille pour le biseau, le décalage et l'extrusion](extrude-tool.md#grille-pour-le-biseau-le-décalage-et-lextrusion).

## Astuces

- Pour des marches de 12 x 8, créez une grille personnalisée de pas 12 12 8 et attachez-la à l'escalier (voir [Grilles personnalisées pour les escaliers](../guides/custom-grids-for-stairs.md)).
- **Aligner la sélection sur la grille** (`Ctrl+B`) et **Aligner chaque objet sur la grille** (`Ctrl+Maj+B`) sont décrits dans [Transformer et aligner](transform-align.md).
- Outils MCP : `list_grids`, `set_grid_mode`, `create_grid`, `edit_grid`, `delete_grid`, `attach_grid`, `rotate_grid` (angle, delta ou align, pour une vue ; voir la [référence des outils MCP](../../MCP.md), en anglais).

## FAQ

**Puis-je utiliser une grille de 12 ou de 1,5 ?** Oui. Saisissez-la dans le champ de taille de la barre d'outils, ou créez une grille personnalisée.

**`[` saute de 80 à 64.** Avec le facteur 2, les touches suivent les puissances de deux de Hammer. Mettez une autre valeur dans **Pas de la taille de grille** pour multiplier et diviser librement.

**Où sont enregistrées les grilles personnalisées ?** Les grilles de la map sont écrites dans un bloc `hammerpro_grids` du VMF et les grilles attachées dans le bloc editor de chaque objet : elles reviennent quand vous rouvrez la map. Les préréglages sont dans `settings.toml`.

**Je veux une grille tournée autour d'un seul bâtiment.** Donnez des **Limites** à la grille personnalisée : en dehors de la boîte, la grille globale s'applique.

**Comment obtenir une grille à 45 degrés ?** Ouvrez le menu des grilles, saisissez 45 dans **Rotation de la grille** (ou utilisez **Pivoter la grille de 45 degrés**). Pour suivre un mur, sélectionnez-le et utilisez **Aligner la grille sur l'arête ou la face sélectionnée**.

## Pages liées

- [Grilles personnalisées pour les escaliers](../guides/custom-grids-for-stairs.md)
- [Transformer et aligner](transform-align.md)
- [Caméra et navigation](camera-navigation.md)
- [Référence des paramètres](../reference/settings.md)

---
[Retour à l'index de la documentation](../README.md)
