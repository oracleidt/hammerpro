[Documentation](../README.md) > Fonctionnalités > Outil bloc | [English](../../en/features/block-tool.md) | [Русский](../../ru/features/block-tool.md)

# Outil bloc

L'outil bloc crée des brushes à partir de primitives (bloc, coin, cylindre, pointe, sphère, arche, tore, escalier, escalier en colimaçon, rampe, pilier, encadrement de porte). Vous tracez une boîte dans une vue 2D ou directement sur une surface dans la vue 3D, et la primitive est construite dans cette boîte avec le matériau actuel.

## Ouvrir l'outil

- **Outils > Outil actif > Outil bloc**, la palette d'outils, ou `Maj+B`.
- Les options de l'outil s'affichent dans la barre d'options au-dessus des vues.

## Tracer dans les vues 2D

1. Glissez dans une vue 2D (dessus, face ou côté) pour tracer l'emprise de la boîte. Les deux coins s'alignent sur la grille (maintenez `Alt` pour tracer sans magnétisme).
2. La profondeur de la boîte (le long de l'axe de la vue) vient, comme dans Hammer :
   - de l'encombrement de la sélection sur cet axe, si elle a une épaisseur,
   - sinon de la profondeur du dernier objet créé par l'outil bloc, polygone ou courbe,
   - sinon de 64 unités au-dessus d'une sélection plate (ou à partir de 0).
3. La boîte reste **en attente** : elle affiche sa largeur et sa hauteur le long de ses bords, et 8 poignées de redimensionnement (coins et milieux des bords) dans chaque vue 2D.
   - Glissez une poignée pour redimensionner la boîte (les bords déplacés s'alignent sur la grille).
   - Glissez l'intérieur pour la déplacer (son coin minimum s'aligne sur la grille).
   - Glissez en dehors pour en tracer une nouvelle.
4. Appuyez sur `Entrée`, cliquez sur **Créer** dans la barre d'options, ou faites un clic droit puis **Créer l'objet** pour construire la primitive. `Échap` annule la boîte en attente.

Le menu du clic droit propose aussi **Créer en cylindre** et **Créer en arche** : la boîte en attente est construite avec cette primitive sans changer celle choisie dans les options.

## Tracer dans la vue 3D

1. Glissez sur une face de brush (ou sur le plan du sol à la hauteur 0 quand il n'y a aucune surface sous le curseur) pour tracer le rectangle de base sur cette surface. La base s'aligne sur la grille ; si le brush porte sa propre grille, c'est elle qui est utilisée (voir [Grilles](grids.md)). `Alt` : sans magnétisme.
2. Relâchez le bouton : la base est posée et la boîte prend un pas de grille de hauteur. Déplacez la souris vers le haut ou le bas pour régler la hauteur (par pas de grille entiers).
3. Cliquez pour créer la primitive. `Échap` annule.

Tracée sur un mur, la boîte s'éloigne du mur ; tracée sous un plafond, elle pousse vers le bas. Avant de commencer, un petit repère montre le point de surface aligné sous le curseur, et la taille `x x y x z` s'affiche au-dessus de la boîte pendant le tracé.

## Changer le nombre avec la molette

Comme pour l'ajout d'un cylindre dans Blender, la molette change le nombre de la forme pendant sa création : pendant le glissement dans une vue 2D, tant que la boîte est en attente (avant `Entrée`), et pendant les étapes de base et de hauteur dans la vue 3D.

| Primitive | Ce que change la molette | Plage |
|---|---|---|
| Cylindre, pointe (cône), sphère, arche, tore, pilier | **Côtés** | 3 à 128 |
| Escalier, escalier en colimaçon | **Marches** | 1 à 256 |
| Encadrement de porte avec **Arche** cochée | **Segments de l'arc** | 2 à 64 |

- Molette vers le haut : un de plus, vers le bas : un de moins. `Maj` + molette avance de 4. La valeur s'arrête aux bornes de la plage.
- Le fil de fer des vues 2D et 3D et la fenêtre d'aperçu suivent aussitôt, et une étiquette sous la boîte (2D) ou au-dessus (3D) affiche le nombre actuel (**Côtés : 12**, **Marches : 8**...).
- Pendant la création d'une telle forme, la molette ne zoome pas et ne déplace pas la vue ; elle zoome de nouveau dès que la forme est créée ou annulée, et toujours pour le bloc, le coin, la rampe et un encadrement de porte sans arche. `Ctrl` + molette garde son action de grille ou de zoom 2D.
- La valeur reste dans les options pour la forme suivante. Pour un escalier réglé par **Hauteur de marche**, la molette passe à un nombre de marches en partant du nombre actuel (**Hauteur de marche** revient à 0).
- La barre d'état le rappelle dès que la primitive choisie a un nombre.

## Primitives

Les boutons des primitives sont à gauche de la barre d'options.

| Primitive | Forme | Solides créés |
|---|---|---|
| **Bloc** | Boîte qui remplit l'encombrement. | 1 |
| **Coin** | Rampe : dessous complet, face arrière verticale du côté Y maximum, pente qui descend vers le côté Y minimum. | 1 |
| **Cylindre** | Prisme vertical à section en polygone régulier inscrit dans la boîte. | 1 |
| **Pointe** | Pyramide à base en polygone régulier, sommet au centre du haut. | 1 |
| **Sphère** | Sphère (ellipsoïde) inscrite dans la boîte, un solide convexe par tranche (nombre de tranches : la moitié des côtés, au moins 2). | plusieurs |
| **Arche** | Arche ou anneau de segments de mur, un solide convexe par segment. Si le mur est aussi épais que le rayon, les segments deviennent des parts de tarte. | un par côté |
| **Tore** | Anneau à tube polygonal, un solide convexe par segment. Peut devenir une spirale avec une montée. | un par côté |
| **Escalier** | Escalier droit qui monte vers la **Direction** choisie (+Y par défaut). La boîte donne la largeur, la longueur et la hauteur totale (Z) ; la dernière marche atteint le haut de la boîte. | un par marche |
| **Escalier en colimaçon** | Marches autour du centre de la boîte (la hauteur de la boîte est la hauteur totale), chacune un secteur convexe entre le rayon intérieur et le rayon extérieur. | un par marche |
| **Rampe** | Coin avec un angle de pente donné, qui monte vers la **Direction** choisie (+Y par défaut). Quand la pente monterait plus haut que la boîte, elle s'arrête en haut et le reste est plat. | 1 |
| **Pilier** | Colonne à section en polygone régulier, avec un socle et un chapiteau facultatifs plus larges que le fût. | 1 à 3 |
| **Encadrement de porte** | Mur qui remplit la boîte avec une ouverture centrée descendant jusqu'au bas de la boîte : deux montants et un linteau ; avec **Arche** le linteau reçoit un haut arrondi fait de morceaux droits. | 3, ou 2 plus les segments de l'arche |

Les escaliers, rampes, piliers et encadrements sont faits de solides convexes qui ne se chevauchent pas, avec des sommets entiers quand la forme reste valide (les marches se calent alors sur des unités entières). Ils utilisent le matériau actuel sur toutes les faces. Les cylindres et les arches reçoivent des sommets entiers quand la forme reste valide ; les sphères et les tores ne sont jamais arrondis (leurs faces ne seraient plus planes). Quand les paramètres ne peuvent pas donner de forme dans la boîte, les vues 2D affichent **Paramètres invalides pour cette taille** sous la boîte.

## Options

Les paramètres affichés dépendent de la primitive. Les angles suivent la convention de Hammer : 0 pointe vers +Y et les angles croissent vers +X.

| Option | Primitives | Rôle | Par défaut |
|---|---|---|---|
| **Côtés** | cylindre, pointe, sphère, arche, tore | Nombre de côtés des formes rondes (3 à 128). La molette le change pendant la création de la forme. | 8 |
| **Mur** | arche | Épaisseur du mur de l'arche (1 à 16384 unités). | 16 |
| **Arc** | arche | Angle couvert par l'arche, de 1 à 360 degrés (360 pour un anneau complet). | 360 |
| **Départ** | arche | Angle de départ, de -360 à 360 degrés. | 0 |
| **Montée** | arche | Hauteur ajoutée à chaque segment, de -4096 à 4096 unités (escalier en colimaçon). | 0 |
| **Tube** | tore | Côtés de la section du tube (3 à 128). | 8 |
| **Anneau** | tore | Largeur de l'anneau, c'est-à-dire le diamètre du tube (1 à 16384 unités). Elle doit rester plus petite que la boîte. | 32 |
| **Arc** | tore | Angle parcouru par l'anneau : de 1 à 360 degrés, ou jusqu'à 3600 (dix tours) quand **Montée** n'est pas à 0. | 360 |
| **Départ** | tore | Angle de départ du balayage, de -360 à 360 degrés. | 0 |
| **Montée** | tore | Hauteur gagnée sur tout l'anneau, de -16384 à 16384 unités (rampes et tuyaux en spirale). | 0 |
| **Marches** | escalier, colimaçon | Nombre de marches (1 à 256). La hauteur de la boîte est divisée à parts égales. | 8 |
| **Haut. marche** | escalier, colimaçon | Hauteur d'une marche en unités (0 à 1024). Au-dessus de 0, elle remplace **Marches** : le nombre de marches est la hauteur de la boîte divisée par cette valeur, arrondi. | 0 |
| **Ouvert** | escalier, colimaçon | Marches ouvertes : dalles de deux marches d'épaisseur au lieu de colonnes pleines jusqu'au bas de la boîte. Nécessaire pour une spirale de plus de 360 degrés. | désactivé |
| **Rayon int.** | colimaçon | Rayon intérieur des marches, 0 pour des marches en part de tarte. Il doit rester plus petit que la boîte. | 16 |
| **Rotation** | colimaçon | Angle total, de -3600 à 3600 degrés, négatif pour tourner dans l'autre sens (limité à 360 sauf **Ouvert**). Chaque marche doit couvrir moins de 180 degrés. | 270 |
| **Départ** | colimaçon | Angle de départ, de -360 à 360 degrés. | 0 |
| **Pente** | rampe | Angle de pente, de 1 à 89 degrés. | 30 |
| **Direction** | escalier, colimaçon, rampe | Côté vers lequel montent l'escalier ou la rampe : **+Y**, **-Y**, **+X**, **-X** ou **Côté le plus long** (le plus long des côtés X et Y de la boîte : +X si X est plus long, sinon +Y). Un colimaçon démarre de ce côté puis tourne de **Rotation**. Les côtés sont ceux du repère de la forme, identiques aux axes du monde sauf si la forme se dresse selon un autre axe (voir **Orienter selon la vue**). | +Y |
| **Base** | pilier | Hauteur du socle, 0 pour aucun. | 16 |
| **Chapiteau** | pilier | Hauteur du chapiteau, 0 pour aucun. | 16 |
| **Retrait** | pilier | De combien le fût est plus étroit que le socle et le chapiteau (utilisé s'il y a un socle ou un chapiteau). | 8 |
| **Ouverture L** | encadrement | Largeur de l'ouverture, centrée. 0 vaut la moitié de la largeur du mur. | 0 |
| **Ouverture H** | encadrement | Hauteur de l'ouverture depuis le bas de la boîte. 0 vaut les trois quarts de la hauteur du mur. | 0 |
| **Épaisseur** | encadrement | Épaisseur du mur, centrée dans la profondeur de la boîte. 0 remplit toute la profondeur de la boîte. | 0 |
| **Arche** | encadrement | Donne un haut arrondi à l'ouverture. Le haut de l'ouverture est à **Ouverture H** ; l'arche doit tenir en dessous. | désactivé |
| **Rayon de l'arche** | encadrement | Rayon du haut arrondi. Au moins la moitié de l'ouverture ; 0 vaut exactement la moitié (demi-cercle). Un rayon plus grand donne une arche surbaissée. | 0 |
| **Segments de l'arche** | encadrement | Nombre de morceaux droits qui forment le haut arrondi (2 à 64). | 8 |
| **Orienter selon la vue** | toutes sauf le bloc | Les formes tracées dans la vue de face ou de côté (ou sur un mur en 3D) s'alignent sur l'axe de profondeur de la vue : un cylindre tracé dans la vue de face est couché le long de X. Enregistré dans le paramètre `reorient_primitives`. | activé |
| **Échelle** | toutes | Échelle de texture des nouvelles faces (unités par texel, de 0,01 à 64). | paramètre **Échelle de texture par défaut** (0,25) |
| Créer comme | toutes | **Brush du monde**, **func_detail** ou n'importe quelle classe d'entité brush du FGD (liste avec recherche, un nom de classe saisi est accepté). Avec une classe, tous les solides de la primitive vont dans une nouvelle entité brush avec les keyvalues par défaut du FGD. Les primitives du monde faites de plusieurs solides sont groupées. Le bouton **Détail** à côté bascule entre **Brush du monde** et **func_detail** d'un clic (voir [func_detail](func-detail.md)). | Brush du monde |
| Matériau actuel | toutes | Vignette et nom du matériau actuel, posé sur toutes les faces (le matériau par défaut si aucun n'est choisi). Cliquez dessus pour afficher le navigateur de textures. | `dev/dev_measuregeneric01b` |
| Punaise (garder actif) | toutes | Garder l'outil bloc actif après la création d'un objet. Désactivée, l'outil de sélection revient après chaque création. | activée |
| **Créer** | toutes | Créer l'objet en attente (`Entrée`). | |

Chaque face reçoit aussi l'**Échelle de lightmap par défaut** des paramètres (16).

Chaque nouvel objet devient la sélection, et chaque création est une seule étape d'annulation.

### Aperçu de la forme en direct

Pour les primitives rondes, une petite vignette à côté des paramètres montre la forme vue depuis son axe vertical. Cliquez dessus pour ouvrir la grande fenêtre d'aperçu (**Aperçu : Arche**, **Aperçu : Tore**...) : elle dessine chaque solide avec les paramètres à côté, la taille de la boîte et le nombre de solides. La fenêtre s'ouvre toute seule quand vous choisissez l'arche, comme la boîte Arch Properties de Hammer. Sans boîte tracée, l'aperçu prend la taille du dernier objet créé (ou 256 x 256 x 64) et affiche **Aucune boîte tracée : aperçu à la dernière taille**. Dans les vues, le fil de fer de la primitive est dessiné dans la boîte en attente.

## Paramètres liés

| Paramètre | Où | Par défaut |
|---|---|---|
| **Orienter les primitives selon la vue de tracé** (`[brush] reorient_primitives`) | Paramètres > **Brushes** | activé |
| **Matériau par défaut** (`[edit] default_material`) | Paramètres > **Édition** | `dev/dev_measuregeneric01b` |
| **Échelle de texture par défaut** (`[edit] default_texture_scale`) | Paramètres > **Édition** | 0,25 |
| **Échelle de lightmap par défaut** (`[edit] default_lightmap_scale`) | Paramètres > **Édition** | 16 |

La fenêtre Paramètres s'ouvre avec **Outils > Options...** (`Ctrl+,`).

## Astuces

- Sélectionnez un brush avant de tracer dans la vue de dessus : la nouvelle boîte reprend sa plage de hauteur. Sélectionnez d'abord un mur de la pièce et la boîte prend toute la hauteur du mur.
- Pour un escalier, tracez la boîte avec la largeur, la longueur et la hauteur totale voulues et réglez **Marches** ou **Haut. marche**. Pour un colimaçon, prenez la primitive escalier en colimaçon (ou une arche avec une **Montée** égale à la hauteur d'une marche). Choisissez le côté vers lequel ils montent avec **Direction** (+Y, -Y, +X, -X ou le côté le plus long de la boîte). Pour un tuyau en hélice, prenez un tore avec une **Montée** et un **Arc** supérieur à 360.
- Laissez **Orienter selon la vue** activé et tracez un cylindre dans la vue de face pour obtenir un tuyau couché le long de X sans devoir le tourner ensuite.
- Nom de chaque option dans les fichiers `--script` et pour l'outil MCP `set_tool_options` : `kind`, `sides`, `wall`, `arc`, `start`, `rise`, `tube_sides`, `ring`, `torus_arc`, `torus_start`, `torus_rise`, `steps`, `step_height`, `open`, `inner`, `spiral_angle`, `spiral_start`, `slope`, `direction`, `base`, `capital`, `inset`, `door_width`, `door_height`, `thickness`, `door_arch`, `door_arch_radius`, `door_arch_segments`, `reorient`, `preview`, `tex_scale`, `keep_active`, `create_as`.

## FAQ

**Pourquoi ma boîte n'a pas la bonne hauteur ?** Dans une vue 2D, la profondeur vient de la sélection, sinon du dernier objet créé. Videz la sélection (`Ctrl+Maj+A`) ou redimensionnez la boîte dans une autre vue 2D avec ses poignées.

**Pourquoi la vue affiche « Paramètres invalides pour cette taille » ?** La boîte est trop petite pour les paramètres : par exemple un anneau de tore plus large que la boîte, ou un arc trop grand pour le nombre de côtés (chaque segment doit couvrir moins de 180 degrés), ou plus de côtés ou de marches que la boîte ne peut en contenir (64 côtés dans une demi-unité, une seule marche de colimaçon tournant de presque 180 degrés : ces formes seraient des lamelles plates, elles sont donc refusées au lieu de créer des brushes cassés). Changez les paramètres (la molette baisse le nombre pendant le tracé) ou agrandissez la boîte.

**Comment créer directement un trigger ?** Choisissez `trigger_multiple` (ou une autre classe d'entité brush) dans la liste « créer comme ». Le matériau actuel est utilisé, choisissez donc d'abord la texture outil trigger.

**Peut-on quitter l'outil après chaque brush ?** Oui, désactivez la punaise : l'outil de sélection revient après chaque création.

## Pages liées

- [Outil polygone](polygon-tool.md)
- [Outil courbe](curve-tool.md)
- [Outil de sélection](selection-tool.md)
- [Opérations sur les brushes](brush-operations.md)
- [Outil vertex](vertex-tool.md)
- [Grilles](grids.md)
- [Navigateur de textures](texture-browser.md)
- [Référence des paramètres](../reference/settings.md)
- [Référence des outils MCP](../../MCP.md)

---
[Retour à l'index de la documentation](../README.md)
