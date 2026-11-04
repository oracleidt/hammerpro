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
| `Alt`+clic sur une arête | Sélectionne sa **boucle d'arêtes** (voir [Boucles et anneaux d'arêtes](#boucles-et-anneaux-darêtes)) ; `Maj` l'ajoute à la sélection. Fonctionne dans tous les modes de poignées et dans les vues 2D et 3D, l'arête est saisie sur toute sa longueur |
| `Ctrl+Alt`+clic sur une arête | Sélectionne son **anneau d'arêtes** ; `Maj` ajoute |
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
| **Faire tourner les sommets...** | Ouvre la rotation des sommets | |

Une ligne contient les commandes de sélection d'arêtes (voir [Boucles et anneaux d'arêtes](#boucles-et-anneaux-darêtes)) : **Sélectionner la boucle d'arêtes**, **Sélectionner l'anneau d'arêtes**, **Agrandir la sélection**, **Réduire la sélection**, **Sélectionner le contour**, **Faire glisser la boucle d'arêtes...** et **Faire tourner les sommets...**, avec le nombre d'arêtes et de sommets sélectionnés. Une seconde ligne contient les commandes de soudure (voir [Souder des sommets](#souder-des-sommets)) : **Souder** avec sa cible, **Souder les sommets sélectionnés**, **Distance de soudure** avec **Souder les sommets par distance**, et **Supprimer les faces et arêtes dégénérées**.

Les quatre derniers n'apparaissent que si des sommets sont sélectionnés.

## Scinder une face

Sélectionnez deux sommets (ou deux arêtes) d'une même face, puis appuyez sur `Ctrl+F` (**Outils > Scinder la face**). La face est coupée le long de la ligne qui les relie. Deux sommets déjà reliés par une arête ne donnent rien : prenez deux sommets opposés de la face.

L'outil garde la scission en attente jusqu'à ce que vous déplaciez un sommet : les deux moitiés deviennent alors deux faces, ou redeviennent une seule face si elles sont encore planes. Pratique pour créer une arête vive ou un pli dans un mur. Les scissions en attente sont dessinées dans leur propre couleur et comptées dans la barre d'options.

## Souder des sommets

Lâcher un sommet près d'un autre les fusionne (voir **Distance de fusion**). Les commandes de soudure le font volontairement, sur des sélections entières, depuis la seconde ligne de la barre d'options, le menu **Outils** (quand l'outil vertex est actif), le menu du clic droit des vues, la palette de commandes et MCP. Chaque commande forme une seule étape d'annulation.

| Commande | Effet |
| --- | --- |
| **Souder les sommets sélectionnés** | Fusionne les sommets sélectionnés de chaque solide en un seul sommet. La cible se choisit dans la liste à côté de **Souder** : **Au centre** (la moyenne des sommets, par défaut), **Au premier** ou **Au dernier** (le sommet d'indice le plus petit ou le plus grand, les sommets n'ont pas d'ordre de sélection) |
| **Souder les sommets par distance** | Fusionne chaque paire de sommets des solides édités plus proche que la **Distance de soudure** (0,001 à 64 unités, 1 par défaut), en répétant jusqu'à ce qu'aucune paire ne soit aussi proche. Les solides qui deviendraient invalides restent tels quels et un avertissement les compte |
| **Supprimer les faces et arêtes dégénérées** | Fusionne les sommets plus proches que 0,01 unité et supprime les faces et arêtes devenues dégénérées, ainsi que les faces qui ne font plus partie du solide. La soudure supprime déjà les faces qu'elle écrase, donc cette commande sert pour les solides qui arrivent avec de tels restes |

Une étiquette à côté de chaque commande prévisualise le résultat, par exemple « 12 sommets seront soudés ». Elle compte les sommets qui disparaissent : pour **Souder les sommets sélectionnés** avec la sélection et la cible actuelles, pour **Souder les sommets par distance** avec la distance actuelle.

Une soudure qui rendrait un solide concave ou plat (par exemple fusionner deux coins opposés d'une face) est refusée avec un message, et rien ne change. Les sommets sélectionnés restent sélectionnés sur le sommet soudé.

MCP : `weld_brush_vertices` (mode `vertices`, `distance` ou `cleanup`) fait la même chose sur le document, et les options `merge_distance`, `weld_distance` et `weld_target` de l'outil vertex sont listées par `get_tool_options` et modifiées avec `set_tool_options`. Les actions sont `vertex_weld`, `vertex_weld_distance` et `vertex_cleanup`.

## Mettre les sommets à l'échelle

`Alt+E` (**Outils > Mettre les sommets à l'échelle...**) met les sommets sélectionnés à l'échelle autour d'un centre, avec un aperçu en direct. La barre d'options affiche :

| Option | Effet | Par défaut |
| --- | --- | --- |
| **Facteur** | 1 garde la taille, 2 double les distances au centre, 0,5 les divise par deux (0 à 100). Les boutons `-` et `+` changent de 0,1 | 1 |
| **Centre** | Point dont les sommets se rapprochent ou s'éloignent. Vous pouvez aussi glisser sa poignée ronde dans les vues (elle s'aligne sur la grille comme les sommets, `Alt` pour s'en passer) | Centre de la boîte autour des sommets sélectionnés |
| **Appliquer** | Applique la mise à l'échelle en une étape d'annulation (`Entrée`, ou `Alt+E` de nouveau) | |
| **Annuler** | Ferme sans rien changer (`Échap`) | |

Au-dessus d'une vue, `Alt` + molette change le facteur de 0,1 par cran, `Ctrl+Alt` + molette de 0,01. Un résultat invalide (solide concave ou plat) garde la mise à l'échelle ouverte avec un message.

## Boucles et anneaux d'arêtes

Une *boucle d'arêtes* est une ligne d'arêtes qui continue d'elle-même, et un *anneau d'arêtes* est l'ensemble des arêtes parallèles à travers les faces à quatre côtés. Les brushes étant des solides convexes, les boucles se calculent sur les arêtes de tous les brushes édités ensemble : les brushes qui partagent un coin ou une arête forment un seul réseau, et une face qui touche exactement la face d'un autre brush compte comme un mur entre eux, pas comme une surface.

- **Boucle d'arêtes.** La boucle traverse tout droit un sommet où se rencontrent quatre arêtes (le milieu d'un mur fait de plusieurs brushes, une grille) et suit le contour d'une face là où trois arêtes se rencontrent (les coins d'un brush : la boucle d'une arête de boîte est l'anneau autour d'une de ses faces, et sur un prisme elle suit l'anneau du dessus). Elle s'arrête à un sommet où cinq arêtes ou plus se rencontrent et à un cul-de-sac. Quand deux faces sont possibles à un coin, la boucle suit la face tournée vers vous, puis celle qui a le plus de côtés. Le long du bord d'une surface, elle suit ce bord.
- **Anneau d'arêtes.** À partir de l'arête, il traverse chaque face à quatre côtés jusqu'à l'arête opposée et continue par la face suivante, dans les deux sens. Sur une boîte, il donne les quatre arêtes parallèles.
- **Alt+clic** sur une arête sélectionne sa boucle, **Ctrl+Alt+clic** son anneau, ajoutez `Maj` pour ajouter à la sélection au lieu de la remplacer. Maintenir `Alt` montre en bleu, dans toutes les vues, la boucle (ou l'anneau avec `Ctrl`) qu'un clic sélectionnerait. Les arêtes sélectionnées sont dessinées en jaune avec leurs sommets dans toutes les vues, et la barre d'état compte les arêtes et sommets sélectionnés.
- **Commandes de sélection.** **Édition > Sélection d'arêtes** (aussi dans la barre d'options, le menu contextuel et la palette de commandes) : **Sélectionner la boucle d'arêtes** et **Sélectionner l'anneau d'arêtes** étendent les arêtes sélectionnées (ou les arêtes entre sommets sélectionnés) à leurs boucles ou anneaux ; **Agrandir la sélection** ajoute tous les sommets voisins des sommets sélectionnés ; **Réduire la sélection** retire les sommets qui touchent un sommet non sélectionné ; **Sélectionner le contour** garde les arêtes du bord de la région sélectionnée (faces dont tous les sommets sont sélectionnés) ou, si aucune face n'est complète, les sommets sélectionnés qui ont un voisin non sélectionné.
- **Déplacer la boucle.** La boucle sélectionnée est une sélection de sommets ordinaire : glissez-la à la souris ou avec le gizmo, tapez sa **Position**, décalez-la avec les flèches, ou utilisez **Mettre les sommets à l'échelle...** (`Alt+E`). Chaque brush qui partage un sommet de la boucle la suit. La sélection d'arêtes reste après le déplacement, qui forme une seule étape d'annulation.
- **Faire tourner les sommets...** (**Outils > Faire tourner les sommets...**, barre d'options) fait tourner les sommets sélectionnés autour d'un centre avec aperçu en direct : l'angle (-360 à 360 degrés, `Alt` + molette le change de 5 degrés, `Ctrl+Alt` de 1), l'axe (**X**, **Y**, **Z** ou **Normale de la boucle**, la normale du plan des sommets sélectionnés, pour qu'une boucle tourne dans son propre plan) et le centre (le milieu des sommets sélectionnés, modifiable). **Appliquer** (`Entrée`) ou **Annuler** (`Échap`).
- **Faire glisser la boucle d'arêtes...** (**Outils > Faire glisser la boucle d'arêtes...**, barre d'options) fait glisser les boucles sélectionnées le long des faces voisines en gardant leur forme, avec aperçu en direct. Chaque sommet de la boucle se déplace le long de sa propre arête qui quitte la boucle : les valeurs positives vont d'un côté de la boucle, les négatives de l'autre, de cette fraction de l'arête (-1 à 1, `Alt` + molette le change de 0,05). Là où un sommet n'a pas d'arête d'un côté (le coin d'un brush), l'autre arête est prolongée : le dessus d'une boîte glisse vers le haut ou le bas le long de ses côtés. Les arêtes sélectionnées doivent former des boucles ou des lignes simples, sans ramification.
- Un résultat invalide (brush concave ou plat) s'affiche en rouge avec la raison, comme toute modification de sommets, et ne peut pas être appliqué.

MCP : `select_edge_loop`, `select_edge_ring` et `grow_edge_selection` (mode `grow`, `shrink` ou `boundary`) donnent les arêtes et sommets d'une boucle, d'un anneau ou d'une sélection pour des arêtes de départ, avec `toward` pour choisir la face et `select` pour montrer aussi le résultat dans l'outil vertex ; `move_edge_loop` déplace, met à l'échelle, fait tourner ou glisser une boucle en une étape d'annulation (voir la [référence des outils MCP](../../MCP.md)). Les actions sont `edge_select_loop`, `edge_select_ring`, `edge_grow`, `edge_shrink`, `edge_select_boundary`, `edge_slide` et `vertex_rotate`.

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

**Comment fusionner deux sommets ?** Glissez l'un sur l'autre. Tout sommet lâché à moins de la **Distance de fusion** (1 unité par défaut) d'un sommet immobile est soudé à lui. Pour en fusionner plusieurs d'un coup, sélectionnez-les et utilisez **Souder les sommets sélectionnés**, ou **Souder les sommets par distance** pour fusionner chaque paire proche.

**Mes textures s'étirent quand je bouge les sommets.** Le **Verrouillage UV** est activé : les textures suivent les sommets. Désactivez-le pour garder les textures fixes dans le monde.

**Peut-on éditer les sommets avec un assistant IA ?** Oui, les outils MCP `get_brush_vertices`, `set_brush_vertices`, `move_brush_vertices` et `snap_brush_vertices` font les mêmes modifications (voir la [référence des outils MCP](../../MCP.md), en anglais).

**Comment sélectionner toute la ligne d'arêtes autour d'une face de brush ou le long d'un mur ?** `Alt`+clic sur l'une de ses arêtes. `Ctrl+Alt`+clic sélectionne plutôt les arêtes parallèles.

**La boucle s'arrête à un brush, pourquoi ?** La boucle ne traverse que les brushes édités, et les brushes ne se joignent que là où leurs coins se rencontrent exactement. Sélectionnez d'abord tous les brushes du mur et vérifiez que les coins sont alignés.

## Pages liées

- [Outil de sélection](selection-tool.md)
- [Outil de découpe](clip-tool.md)
- [Outil displacement](displacement-tool.md)
- [Outil biseau](bevel-tool.md)
- [Grilles](grids.md)
- [Outil texture](texture-tool.md)
- [Éditeur UV](uv-editor.md)
- [Raccourcis clavier](keyboard-shortcuts.md)
- [Référence des paramètres](../reference/settings.md)

---
[Retour à l'index de la documentation](../README.md)
