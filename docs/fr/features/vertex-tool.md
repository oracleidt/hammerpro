[Documentation](../README.md) > Fonctionnalités > Outil vertex | [English](../../en/features/vertex-tool.md)

# Outil vertex

L'outil vertex modifie la forme des solides sélectionnés en déplaçant, fusionnant et scindant leurs sommets et leurs arêtes, avec un aperçu texturé en direct pendant le glissement. Il fonctionne comme l'outil vertex de Hammer, avec les ajouts de Hammer++ (verrouillage UV, aperçu en temps réel) et quelques extras : mise à l'échelle des sommets, scission de face et saisie précise de la position.

## Ouvrir l'outil

- **Outils > Outil vertex**, la palette d'outils, ou `Maj+V`.
- Appuyez de nouveau sur `Maj+V` quand l'outil est actif pour passer au mode de poignées suivant (voir plus bas). Un message indique le nouveau mode.
- L'outil édite les solides sélectionnés au moment où il démarre. Si rien n'est sélectionné, cliquez sur un solide pour l'éditer. Jusqu'à 2048 solides sont édités à la fois (un badge d'avertissement s'affiche au-delà).

Les options de l'outil sont dans la barre d'options au-dessus des vues.

## Poignées

- Les **poignées carrées** sont les sommets. Les **poignées rondes** sont les milieux des arêtes.
- Les sommets et arêtes sélectionnés sont dessinés en jaune. Pendant un glissement, les nouvelles arêtes sont en jaune, ou en rouge si le solide obtenu serait invalide.
- Dans les vues 2D, les poignées superposées selon l'axe de profondeur sont prises ensemble (comme dans Hammer) : cliquer sur un sommet d'un bloc en vue de dessus sélectionne le sommet du haut et celui du bas à cet endroit.
- Dans la vue 3D, seule la poignée cliquée est prise, et un gizmo à trois axes (X, Y, Z) est dessiné au centre des sommets sélectionnés.

## Modes de poignées

| Mode | Ce qui peut être pris | Glisser le milieu d'une arête |
| --- | --- | --- |
| **Sommets et arêtes** (par défaut) | Sommets et milieux d'arêtes | Coupe l'arête en deux : un nouveau sommet est créé au milieu et suit la souris |
| **Sommets** | Sommets seulement | Pas de poignées d'arête |
| **Arêtes** | Milieux d'arêtes seulement | Déplace toute l'arête (ses deux sommets) |

Choisissez le mode avec les trois boutons de la barre d'options, ou appuyez sur `Maj+V` (choisir l'outil à nouveau) pour passer de l'un à l'autre.

## Souris et clavier

| Geste | Effet |
| --- | --- |
| Clic sur une poignée | Sélectionne ce sommet ou cette arête (remplace la sélection de sommets) |
| `Ctrl`+clic sur une poignée | L'ajoute à la sélection de sommets ou l'en retire |
| Clic sur un solide non édité | Édite ce solide à la place (`Ctrl`+clic l'ajoute ou le retire), sauf si **Solides sélectionnés seuls** est coché |
| Clic dans le vide | Vide la sélection de sommets |
| Glisser depuis le vide | Sélection rectangulaire des sommets (et milieux d'arêtes) qu'il contient ; avec `Ctrl`, ajoute à la sélection |
| Glisser un sommet | Déplace les sommets sélectionnés (le sommet glissé rejoint la sélection s'il n'en faisait pas partie) |
| Glisser un axe du gizmo (vue 3D) | Déplace les sommets sélectionnés le long de cet axe du monde |
| Glisser dans la vue 3D | Déplace sur le plan face à la caméra ; avec `Maj`, seulement selon Z |
| `Alt` pendant le glissement | Sans magnétisme de grille |
| Flèches | Décalent les sommets sélectionnés d'un pas de grille, selon les axes de la vue sous la souris |
| `Alt`+flèche ou `Maj`+flèche | Décalage d'une unité |
| `Page préc.` / `Page suiv.` | Décalage selon l'axe de profondeur (vers vous dans les vues 2D, vers le haut ou le bas dans la vue 3D) |
| `Ctrl+A` | Sélectionne tous les sommets des solides édités (le raccourci de **Tout sélectionner**) |
| `Ctrl+B` | Aligne les sommets sélectionnés sur la grille (le raccourci de **Aligner la sélection sur la grille**). Avec le curseur sur une arête de brush dans la vue 3D, il biseaute cette arête à la place (voir [Outil biseau](bevel-tool.md)) |
| `Ctrl+F` | **Scinder la face** |
| `Alt+E` | **Mettre les sommets à l'échelle...** (appuyez de nouveau pour appliquer) |
| `Échap` | Annule le glissement ou la mise à l'échelle en cours ; sinon désélectionne les sommets ; sinon retire les scissions en attente |

Les décalages aux flèches suivent l'option **Décaler avec les flèches** de la page **Édition** des paramètres (activée par défaut). Le magnétisme suit le bouton **Magnétisme de grille** de la barre d'outils (`Maj+W`). Si une grille personnalisée est attachée aux solides édités, les sommets s'alignent sur cette grille (voir [Grilles](grids.md)).

Le menu du clic droit des vues propose aussi **Scinder la face**, **Mettre les sommets à l'échelle...** et **Aligner les sommets**.

## Comment les modifications s'appliquent

- Un glissement montre son résultat en direct et devient une seule étape d'annulation au relâchement. Plusieurs décalages au clavier à la suite se fusionnent en une seule étape.
- Un solide doit rester convexe. Si un déplacement mettait un sommet à l'intérieur du solide, sur une de ses faces ou une de ses arêtes, ou rendait le solide plat, la barre d'état explique pourquoi et la modification est annulée au relâchement.
- **Fusion** : un sommet déplacé lâché à moins de la **Distance de fusion** d'un sommet resté immobile se colle à lui, et les deux ne forment plus qu'un.
- **Displacements** : une face garde son displacement tant qu'elle reste la même face à 4 côtés. Sinon le displacement est supprimé et un message indique combien ont été perdus. Un badge d'avertissement dans la barre d'options le rappelle quand les solides édités ont des displacements.

## Barre d'options

| Option | Effet | Par défaut |
| --- | --- | --- |
| **Sommets et arêtes** / **Sommets** / **Arêtes** | Mode de poignées (voir plus haut) | Sommets et arêtes |
| **Verrouillage UV** | Les textures suivent les sommets déplacés (elles peuvent être inclinées) au lieu de rester fixes dans le monde. Même réglage que l'action **Verrouillage UV (outil sommets)** du menu **Outils** et la page des paramètres | Désactivé |
| **Solides sélectionnés seuls** | Cliquer sur un autre solide ne change pas les solides édités (seuls les sommets sont pris) | Désactivé |
| **Distance de fusion** | Un sommet lâché plus près que cette distance d'un autre sommet fusionne avec lui (0 à 16 unités) | 1 |
| Compteur | « N sommets sur M sélectionnés » ; l'infobulle donne le nombre de solides édités | |
| **N scission(s)** | Scissions de face en attente d'un déplacement de sommet ; `Échap` les retire | |
| **Tous les sommets** | Sélectionne tous les sommets des solides édités (`Ctrl+A`) | |
| **Scinder la face** | Scinde la face entre les deux sommets ou arêtes sélectionnés (`Ctrl+F`) | |
| **Désélectionner** | Désélectionne tous les sommets (`Échap`) | |
| **Position** | X, Y, Z du centre des sommets sélectionnés : saisissez une valeur pour les déplacer avec précision (une étape d'annulation) | |
| **Aligner les sommets** | Place les sommets sélectionnés sur les points de grille les plus proches (`Ctrl+B`) | |
| **Mettre les sommets à l'échelle...** | Ouvre la mise à l'échelle des sommets (`Alt+E`) | |

Les quatre derniers n'apparaissent que si des sommets sont sélectionnés.

## Scinder une face

Sélectionnez deux sommets (ou deux arêtes) d'une même face, puis appuyez sur `Ctrl+F` (**Outils > Scinder la face**). La face est coupée le long de la ligne qui les relie. Deux sommets déjà reliés par une arête ne donnent rien : prenez deux sommets opposés de la face.

L'outil garde la scission en attente jusqu'à ce que vous déplaciez un sommet : les deux moitiés deviennent alors deux faces, ou redeviennent une seule face si elles sont encore planes. Pratique pour créer une arête vive ou un pli dans un mur. Les scissions en attente sont dessinées dans leur propre couleur et comptées dans la barre d'options.

## Mettre les sommets à l'échelle

`Alt+E` (**Outils > Mettre les sommets à l'échelle...**) met les sommets sélectionnés à l'échelle autour d'un centre, avec un aperçu en direct. La barre d'options affiche :

| Option | Effet | Par défaut |
| --- | --- | --- |
| **Facteur** | 1 garde la taille, 2 double les distances au centre, 0,5 les divise par deux (0 à 100). Les boutons `-` et `+` changent de 0,1 | 1 |
| **Centre** | Point dont les sommets se rapprochent ou s'éloignent. Vous pouvez aussi glisser sa poignée ronde dans les vues (elle s'aligne sur la grille comme les sommets, `Alt` pour s'en passer) | Centre de la boîte autour des sommets sélectionnés |
| **Appliquer** | Applique la mise à l'échelle en une étape d'annulation (`Entrée`, ou `Alt+E` de nouveau) | |
| **Annuler** | Ferme sans rien changer (`Échap`) | |

Au-dessus d'une vue, `Alt` + molette change le facteur de 0,1 par cran, `Ctrl+Alt` + molette de 0,01. Un résultat invalide (solide concave ou plat) garde la mise à l'échelle ouverte avec un message.

## Paramètres

**Outils > Options...** (`Ctrl+,`), page **Sommets et displacements**, section **Outil sommets** :

| Paramètre | Effet | Par défaut |
| --- | --- | --- |
| **Aperçu en direct pendant le glissement** (`mesh.live_preview`) | Les solides glissés montrent leur nouvelle forme avec leurs textures tant que le bouton est maintenu (jusqu'à 64 solides à la fois ; au-delà, seules les arêtes sont montrées) | Activé |
| **Verrouillage UV** (`mesh.uv_lock`) | Comme l'option de la barre d'options | Désactivé |

## Astuces

- Pour chanfreiner le coin d'un bloc, restez en mode **Sommets et arêtes** et glissez le milieu d'une arête : elle se coupe et vous placez le nouveau sommet.
- Travaillez en vue 2D pour déplacer d'un coup toute une colonne de sommets superposés, et en vue 3D pour déplacer un seul sommet.
- Si un déplacement est refusé, bougez moins de sommets à la fois, ou scindez d'abord la face.
- Utilisez **Position** pour saisir des coordonnées exactes, par exemple pour caler une pente à une hauteur précise.

## FAQ

**Pourquoi ma modification revient en arrière quand je relâche la souris ?** Le solide deviendrait concave ou plat. La barre d'état donne la raison pendant le glissement (les arêtes passent en rouge). Dans Source, un brush doit rester convexe.

**Comment fusionner deux sommets ?** Glissez l'un sur l'autre. Tout sommet lâché à moins de la **Distance de fusion** (1 unité par défaut) d'un sommet immobile est soudé à lui.

**Mes textures s'étirent quand je bouge les sommets.** Le **Verrouillage UV** est activé : les textures suivent les sommets. Désactivez-le pour garder les textures fixes dans le monde.

**Peut-on éditer les sommets avec un assistant IA ?** Oui, les outils MCP `get_brush_vertices`, `set_brush_vertices`, `move_brush_vertices` et `snap_brush_vertices` font les mêmes modifications (voir la [référence des outils MCP](../../MCP.md), en anglais).

## Pages liées

- [Outil de sélection](selection-tool.md)
- [Outil de découpe](clip-tool.md)
- [Outil displacement](displacement-tool.md)
- [Grilles](grids.md)
- [Outil texture](texture-tool.md)
- [Éditeur UV](uv-editor.md)
- [Raccourcis clavier](keyboard-shortcuts.md)
- [Référence des paramètres](../reference/settings.md)

---
[Retour à l'index de la documentation](../README.md)
