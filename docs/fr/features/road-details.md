[Documentation](../README.md) > Fonctionnalités > Détails de route | [English](../../en/features/road-details.md)

# Détails de route : trottoirs, bordures, caniveaux et fondu du terrain

Trois générateurs terminent une route : le **générateur de trottoirs** élève un trottoir le long de la route, le **générateur de bordures et caniveaux** ajoute une bordure et un caniveau en V ou en U avec une fente d'évacuation facultative, et **Fondre le terrain sur la route** fait suivre la route au terrain en displacements, avec un accotement progressif et un fondu de texture. Ils travaillent sur les routes de l'[outil route](road-tool.md) et sur les courbes de l'[outil courbe](curve-tool.md), construisent de simples brushes convexes (les plus sûrs pour le compilateur), et chaque génération est une seule étape d'annulation.

## Les ouvrir

Tous sont dans le menu **Outils** et dans la palette de commandes (`Ctrl+P`) :

- **Générateur de trottoirs...**, **Générateur de bordures et caniveaux...** et **Fondre le terrain sur la route...** ouvrent la fenêtre **Détails de route** sur l'onglet correspondant. Les trois onglets sont toujours à un clic l'un de l'autre.
- **Modifier les détails de route...** (actif quand un brush créé par un générateur est sélectionné) ouvre la fenêtre sur l'ensemble de ce brush.

Aucun n'a de raccourci par défaut.

## Les routes

Les générateurs suivent les routes que vous sélectionnez :

1. Sélectionnez les brushes d'une ou plusieurs routes (outil route) ou courbes (outil courbe, ou l'outil MCP `create_curve_brushes`). Un seul brush d'une route suffit, la route entière est retrouvée.
2. Ouvrez le générateur. La fenêtre liste les routes trouvées (**Routes : N**). Si vous changez la sélection ensuite, appuyez sur **Utiliser les routes sélectionnées**.

Une courbe dessinée en vue de face ou de côté fonctionne aussi : ses détails sont construits dans le plan de la courbe, avec les hauteurs mesurées le long de l'axe de profondeur de cette vue (un trottoir le long d'un mur courbe ou d'une rampe vue de côté). Un ensemble ne peut pas mélanger les vues, et le fondu du terrain exige des routes vues de dessus, il refuse donc une courbe de face ou de côté avec un message. Le dévers de la route est suivi : les détails restent à plat et se posent sur le bord de la route, plus haut ou plus bas sur une route inclinée.

Plusieurs routes sont traitées ensemble, ce qui permet les jonctions, voir plus bas.

## Générateur de trottoirs

Construit un trottoir surélevé d'un côté ou des deux côtés de chaque route.

| Champ | Effet | Plage | Défaut |
|---|---|---|---|
| **Côtés** | **Gauche**, **Droite** ou **Les deux**, dans le sens de la route (du premier au dernier point). | | Les deux |
| **Écart avec la route** | Distance entre le bord de la route et le premier brush. Le bord de la route comprend les bordures d'une route de l'outil courbe au profil Bordures, et les accotements d'une route de l'outil route. | 0 à 512 | 0 |
| **Rayon des angles** | Rayon des angles arrondis aux jonctions, mesuré au bord de la route. 0 donne des angles vifs. | 0 à 512 | 32 |
| **Largeur** | Largeur du trottoir. | 1 à 512 | 64 |
| **Hauteur** | Hauteur du dessus au-dessus du bord de la route. | 0 à 256 | 6 |
| **Profondeur sous la route** | Jusqu'où le brush descend sous le bord de la route. | 1 à 256 | 16 |
| **Chanfrein de la face** | Chanfrein de l'arête haute côté route, pour que la face de la bordure ne soit pas une arête vive. | 0 à la hauteur | 0 |
| **Pente transversale** | Montée du dessus entre le côté route et le côté opposé. Une valeur négative évacue vers la route. | -512 à 512 | 0 |
| **Matériau du dessus**, **Matériau de la face** | Matériau du trottoir et des faces tournées vers la route (la face de la bordure). Le matériau de la face est vide par défaut, ce qui garde celui du dessus. **Actuel** met le matériau actuel. | | le matériau actuel |
| **Créer en func_detail** | Mettre tous les brushes dans une seule entité `func_detail` au lieu du monde. | | désactivé |

Appuyez sur **Créer**. Les nouveaux brushes sont sélectionnés. Le trottoir est découpé en morceaux le long de la route avec les mêmes joints que la route, donc une route inclinée, qui monte ou qui tourne reçoit un trottoir qui la suit sans trou. Un morceau qui se replierait à l'intérieur d'un virage serré fait refuser toute la génération, avec le nombre de morceaux : réduisez la largeur ou ouvrez le virage.

## Jonctions

Quand une route **débouche sur le côté d'une autre route** (une rue qui rejoint une route principale), les générateurs voient la jonction tout seuls :

- le trottoir de la route principale s'arrête des deux côtés de l'entrée de la route secondaire, qui reste ouverte ;
- les angles sont arrondis avec le **Rayon des angles**, en quarts de rond dont le bord intérieur suit le bord de la route ;
- le trottoir de la route secondaire se termine sur l'angle.

Les jonctions en T, en Y et les croisements fonctionnent, à tout angle d'au moins 20 degrés. Une route qui en **croise** une autre sans y finir (deux courbes, ou une courbe et une route) est coupée au croisement, l'intersection est donc une jonction en X : les quatre entrées restent ouvertes et les quatre angles sont arrondis avec le rayon des angles. Dessinez la route secondaire pour que son extrémité soit sur la route principale (la jonction de l'outil route le fait pour vous).

Le même arrondi sert aux bordures et aux caniveaux, avec des arcs concentriques : le rayon des angles est toujours celui du bord de la route, et chaque détail plus loin a ce rayon réduit de sa distance.

Quand une jonction de l'outil route a un **Évasement**, donnez au générateur le même **Rayon des angles** que l'évasement pour que le trottoir suive le revêtement arrondi.

Refus avec un message clair : un angle de jonction sous 20 degrés, une jonction trop près de l'extrémité d'une route pour le rayon (réduisez le rayon), et deux jonctions du même côté d'une route plus proches que leurs angles.

## Générateur de bordures et caniveaux

Construit des brushes séparés le long de la route, dans cet ordre à partir du bord de la route : le **caniveau**, puis la **bordure**. Le générateur de trottoirs part aussi du bord de la route, donnez donc au trottoir un **Écart avec la route** égal à la largeur du caniveau plus celle de la bordure pour le placer après eux.

| Champ | Effet | Plage | Défaut |
|---|---|---|---|
| **Côtés**, **Écart avec la route**, **Rayon des angles** | Comme pour le trottoir. | | Les deux, 0, 32 |
| **Bordure** | Construire la bordure. | | activé |
| **Largeur**, **Hauteur**, **Profondeur de la bordure** | Taille de la bordure au-dessus et sous le bord de la route. | 1 à 512, 1 à 256, 1 à 256 | 8, 6, 8 |
| **Chanfrein** | Taille du chanfrein de l'arête haute côté route (0 pour une arête vive). | 0 à la hauteur | 0 |
| **Dessus arrondi** | Arrondir cette arête avec un quart de cercle (4 pas) au lieu d'un chanfrein. Avec un chanfrein de 0, le rayon est la moitié de la largeur ou de la hauteur de la bordure. | | désactivé |
| **Les deux arêtes hautes** | Traiter de la même façon l'arête haute du côté opposé. | | désactivé |
| **Caniveau** | Construire un caniveau le long de la route. | | désactivé |
| **Largeur du caniveau**, **Profondeur du caniveau** | Largeur du caniveau et de combien il s'enfonce sous le bord de la route. | 2 à 512, 0,5 à 256 | 16, 3 |
| **Forme** | **V** (deux pentes) ou **U** (deux pentes et un fond plat). | | V |
| **Fond plat** | Largeur du fond plat d'un U. | 0 à la largeur | 6 |
| **Fente d'évacuation** | Percer une fosse au milieu du caniveau toutes les quelques pièces. | | désactivé |
| **Largeur de la fente**, **Profondeur de la fente**, **Une fente toutes les** | Largeur de la fente en travers du caniveau (elle reste plus étroite que le fond plat d'un U, ou que la moitié de la largeur d'un V), jusqu'où elle descend sous le fond, et combien de pièces le long de la route séparent deux fentes. | 0,5 à 512, 1 à 256, 1 à 1024 | 4, 12, 4 |
| **Matériau de la bordure**, **Matériau du caniveau** | Matériaux. Un matériau de caniveau vide utilise celui de la bordure. | | le matériau actuel |

Le caniveau est fait de pièces convexes (un V est deux coins, un U est deux coins et un bloc) posées sur une fondation sous le caniveau, donc la map reste fermée. Une fente d'évacuation est une fosse avec son propre fond, elle n'ouvre pas la map sur le vide.

## Modifier un ensemble

Chaque ensemble mémorise ses routes et tous ses nombres dans le bloc `editor` de ses brushes (les clés `hammerpro_roadside`, que Hammer et Hammer++ ignorent et conservent), il peut donc être modifié plus tard, même après avoir enregistré la map.

1. Sélectionnez un brush de l'ensemble et utilisez **Modifier les détails de route...**, ou gardez la fenêtre ouverte après **Créer** : les appuis suivants sur **Appliquer les changements** remplacent le même ensemble.
2. Changez n'importe quel nombre, les côtés, les matériaux, ou appuyez sur **Utiliser les routes sélectionnées** pour donner d'autres routes à l'ensemble.
3. **Appliquer les changements** remplace tous les brushes en une étape d'annulation, dans la même entité s'il y en avait une, et les garde sélectionnés. **Nouvel ensemble** arrête la modification et fait créer un autre ensemble au prochain appui.

Un ensemble qui suit une route de l'outil route relit la route à chaque application : après avoir modifié la route, ouvrez **Modifier les détails de route...** sur l'ensemble et appuyez sur **Appliquer les changements** sans rien changer pour le reconstruire sur la nouvelle route. Un ensemble qui suit une courbe garde la courbe telle qu'elle était à sa création, changez-la en la reprenant avec **Utiliser les routes sélectionnées**.

Les copies de l'ensemble entier continuent de fonctionner. Les brushes d'un ensemble trouvés par **Modifier les détails de route...** sont ceux de même marque à moins de 512 unités les uns des autres, une copie lointaine n'est donc pas touchée.

## Fondre le terrain sur la route

Fait suivre la route au terrain en displacements : les sommets sous la route et autour vont à la hauteur de la route, avec un accotement progressif vers le terrain. L'opération agit sur **toutes les displacements à portée des routes sélectionnées** d'un coup, pas seulement les sélectionnées, recoud les voisines pour que les raccords restent fermés, et forme une seule étape d'annulation.

| Champ | Effet | Plage | Défaut |
|---|---|---|---|
| **Largeur de l'accotement** | Largeur du fondu entre la hauteur de la route et le terrain, après le bord de la route et la marge plate. | 0 à 4096 | 128 |
| **Marge plate** | Largeur supplémentaire au bord de la route gardée entièrement à la hauteur de la route. | 0 à 1024 | 0 |
| **Atténuation** | Forme du fondu : **Douce** (smoothstep), **Linéaire**, **Raide près de la route** (au carré, chute rapide) ou **Raide loin de la route** (reste haut, puis chute). | | Douce |
| **Force** | Jusqu'où le terrain va vers la route, 1 pour complètement. | 0 à 1 | 1 |
| **Enfoncement sous la route** | Mettre le terrain à ces unités sous la surface de la route pour qu'ils ne se chevauchent pas et ne clignotent pas. | -256 à 256 | 1 |
| **Déplacement** | **Haut et bas** (défaut), **Creuser seulement** (ne relève jamais le terrain) ou **Relever seulement** (ne l'abaisse jamais). | | Haut et bas |
| **Fondu de texture** | **Aucun**, **2 textures (alpha)** ou **4 textures (multiblend)**, voir plus bas. | | Aucun |
| **Largeur du fondu de texture**, **Force de la texture** | Largeur du fondu de texture après le bord de la route, et jusqu'où il va vers la texture de la route. | 0 à 4096, 0 à 1 | 64, 1 |
| **Texture de la route** | Quelle texture de la displacement est la route : 0 à 3 pour 4 textures, 0 ou 1 pour 2 textures. | 0 à 3 | 1 |

La hauteur de la route est celle de la surface de la route au point le plus proche de la route : une route qui monte, inclinée ou à profil de fossé est suivie, donc le terrain sous une route inclinée est incliné comme elle. Au-delà de l'extrémité d'une route la zone est arrondie. Quand plusieurs routes se chevauchent, chaque sommet suit la route qui l'influence le plus. Seule la hauteur change, à la verticale, et les sommets que deux displacements partagent continuent de bouger ensemble.

### Fondu de texture

L'alpha d'une displacement dit au moteur comment mélanger deux textures, et les lignes multiblend comment en mélanger quatre. Avec **2 textures**, l'alpha va vers la texture de la route (alpha 255 pour la texture 2, la texture de route par défaut, alpha 0 pour la texture 1) sur la **Largeur du fondu de texture**, et revient à ce que le terrain avait à la fin du fondu, le terrain garde donc sa propre peinture. Avec **4 textures**, le poids de la **Texture de la route** choisie va vers 1 et les autres poids diminuent en proportion, écrits en lignes multiblend comme le pinceau de fondu de l'outil displacement (utilisez un matériau Lightmapped_4WayBlend). La peinture des sommets hors du fondu n'est jamais touchée.

### Aperçu

Avec **Afficher la zone touchée** activé (par défaut sur cet onglet), la vue 3D montre les displacements comme elles seront après le fondu : une surface teintée sur les cases touchées (orange là où la hauteur change, bleu là où seule la texture change, plus opaque là où l'effet est plus fort), le bord de la route en lignes blanches et la fin de l'accotement en lignes jaunes, aussi dans les vues 2D. L'aperçu suit chaque changement de nombre et de la map, et disparaît quand la fenêtre se ferme ou que l'onglet change. Il peut montrer jusqu'à 60000 triangles.

## MCP

| Outil | Effet |
|---|---|
| `create_roadside` | Construit un trottoir (`kind` sidewalk) ou une bordure et un caniveau (`kind` curb) avec les champs ci-dessus (`road_ids`, `sides`, `gap`, `corner_radius`, `sidewalk_*`, `curb`, `curb_*`, `gutter`, `gutter_*`, `drain_slot`, `slot_*`, `material`, `face_material`, `curb_material`, `gutter_material`, `tex_scale`, `entity_class`). |
| `edit_roadside` | Remplace un ensemble en une étape d'annulation avec seulement les champs qui changent (`solid_id` de n'importe quel brush de l'ensemble, par défaut le sélectionné). Sans aucun champ il reconstruit l'ensemble sur les routes telles qu'elles sont maintenant. |
| `get_roadside` | Lit les nombres, les routes et les ids des brushes d'un ensemble. |
| `blend_terrain_to_road` | Lance le fondu du terrain (`shoulder`, `margin`, `falloff`, `strength`, `sink`, `mode`, `texture_blend`, `texture_width`, `texture_strength`, `road_channel`) sur les routes de `road_ids`. |
| `preview_terrain_blend` | Compte ce que le fondu changerait et montre la zone dans la vue 3D (`show`, `clear`). |

Les actions `road_sidewalk`, `road_curb`, `road_terrain` et `roadside_edit` ouvrent les fenêtres avec `run_action`. Voir la [référence des outils MCP](../../MCP.md).

## Astuces

- Dessinez d'abord la route, puis le trottoir, puis vérifiez les jonctions : sélectionnez d'un coup toutes les routes d'un bloc pour que leurs jonctions soient trouvées.
- Pour une rue classique : un caniveau en V de 12 unités, une bordure de 6, et un trottoir avec un écart de 18. Ou sans caniveau, le trottoir à l'écart 0 avec un chanfrein de 2 pour une bordure usée.
- Mettez le trottoir en `func_detail` quand il est purement décoratif, cela garde l'arbre BSP simple.
- Fondez le terrain quand la route est définitive, car le terrain suit sa hauteur à ce moment. Refaites-le après une modification de la route, le terrain déjà fondu ne gêne pas (il va à la nouvelle hauteur).
- Une route sur un terrain accidenté : construisez-la d'abord avec **Suivre le terrain** de l'outil route, ou fondez le terrain sur la route, selon ce qui doit garder sa forme.

## FAQ

**Puis-je utiliser une courbe dessinée en vue de face ou de côté ?** Oui pour les trottoirs, bordures et caniveaux, dans le plan de la courbe. Pas pour le fondu du terrain, qui exige des routes vues de dessus.

**Le trottoir ne s'arrête pas à ma route secondaire.** La route secondaire doit finir sur la route principale (dans la largeur de la route principale) et les deux doivent être sélectionnées ensemble.

**Le terrain clignote sur la route.** Utilisez un enfoncement de 1 ou plus, pour que la displacement reste sous la surface de la route.

**Puis-je annuler le fondu du terrain ?** Oui, un `Ctrl+Z` rétablit les hauteurs, les textures et la couture.

## Pages liées

- [Outil route](road-tool.md)
- [Outil courbe](curve-tool.md)
- [Outil displacement](displacement-tool.md)
- [Opérations sur les brushes](brush-operations.md)
- [Historique d'annulation](undo-history.md)

---
[Retour à l'index de la documentation](../README.md)
