[Documentation](../README.md) > Fonctionnalités > Outil de sélection | [English](../../en/features/selection-tool.md) | [Русский](../../ru/features/selection-tool.md)

# Outil de sélection

L'outil de sélection sert à prendre des objets dans les vues 2D et 3D et à les transformer : poignées à la Hammer dans les vues 2D, gizmo de transformation et flèches d'étirement dans la vue 3D, touches G/R/E à la Blender, décalage aux flèches et clonage en glissant.

## Centre des brushes

Un petit **x** indique le centre de chaque brush visible dans les vues Dessus, Face et Côté. Il est activé par défaut, y compris avec les anciennes configurations. Les brushes World et ceux des entités utilisent le centre de leur boîte englobante alignée sur les axes. Le repère suit les modifications, la visibilité et les cordons. Les brushes sélectionnés utilisent la couleur de sélection. Sa taille à l’écran reste constante pendant le zoom. Pendant un déplacement, une rotation, une mise à l'échelle ou un cisaillement, l'aperçu montre aussi un **x** au nouveau centre de chaque brush et entité ponctuelle sélectionnés, dans la couleur de l'aperçu.

Utilisez **Affichage > Options d’affichage > Afficher le centre des brushes en 2D**, ou **Paramètres > Aides**. Le réglage sauvegardé est `helpers.show_brush_centers`. Il concerne uniquement l’affichage dans l’éditeur.

## Ouvrir l'outil

- Raccourci `Maj+S`, ou **Outils > Outil actif > Outil de sélection**, ou son bouton dans la palette d'outils.
- Quand il est actif, sa barre d'options affiche le mode du cadre, le mode des poignées, les poignées d'étirement 3D, le gizmo, le pivot et le champ de sélection par classe ou matériau.

## Sélectionner

### Sélection au clic

| Geste | Vues 2D | Vue 3D |
|---|---|---|
| Clic sur un objet | Le sélectionne (remplace la sélection) | Sélectionne l'objet le plus proche que vous voyez sous le pointeur : les modèles par leur vraie forme (pas leur boîte), et ce que la vue cache est ignoré (objets cachés, textures d'outils cachées par le menu Affichage, déplacements cachés) |
| Nouveau clic au même endroit | Dans les 2 secondes, tant que le dernier choix est encore la sélection : l'objet superposé suivant (jusqu'à 64 en boucle) | Pareil : l'objet suivant, derrière. Plus tard, ou si la sélection a changé, le clic repart de l'objet le plus proche |
| `Ctrl+clic` | Ajoute l'objet à la sélection, ou le retire s'il y est déjà | Pareil ; `Ctrl+clic` traverse aussi le gizmo |
| Clic dans le vide | Vide la sélection (un clic à l'intérieur du cadre de sélection change plutôt le mode des poignées) | Vide la sélection (pas avec `Ctrl`) |
| Clic sur un objet qui forme déjà toute la sélection | Change le mode des poignées (Échelle, Rotation, Inclinaison, puis Échelle), comme dans Hammer | |
| Clic du milieu | L'objet suivant à l'endroit du dernier clic, ou le premier sous le pointeur | Pareil |
| Double-clic | Sélectionne l'objet ; pour une entité (ou une entité de brush), ouvre aussi ses propriétés | Pareil |
| `Page suiv.` / `Page préc.` | Pendant 2 secondes après un clic : l'objet derrière / devant | Pareil |

Un simple clic sur le gizmo ou sur une flèche d'étirement ne sélectionne rien (il sert seulement à glisser). Passé ce délai de deux secondes, `Page préc.` et `Page suiv.` décalent de nouveau la sélection (voir Décaler au clavier plus bas).

### Sélection par cadre (vues 2D)

Glissez dans le vide d'une vue 2D pour tracer un cadre. Comme dans Hammer++, le cadre reste après le relâchement de la souris pour l'ajuster avant que rien ne soit sélectionné :

- Glissez une de ses 8 poignées pour le redimensionner, ou glissez à l'intérieur pour le déplacer (les deux suivent la grille, `Alt` : sans magnétisme).
- Appuyez sur `Entrée` pour sélectionner ce que prend le cadre, `Ctrl+Entrée` pour l'ajouter à la sélection.
- Appuyez sur `Échap` pour abandonner le cadre. Un clic en dehors ou un nouveau glisser l'abandonne aussi.

Ce cadre ajustable est désactivé par défaut depuis la 2.0.3 : la sélection se fait alors dès le relâchement. Activez **Sélection par cadre ajustable (Entrée pour valider)** dans **Outils > Options > Sélection** (ou la case **Entrée** de la barre d'options) pour retrouver le comportement de Hammer++ décrit ci-dessus. La 2.0.2 l'activait pour tout le monde ; la mise à jour le désactive une fois, réactivez-le si vous l'aimiez.

Ce que prend le cadre :

| Mode du cadre | Ce qu'il prend |
|---|---|
| **Touchés** (par défaut) | Tous les objets touchés par le cadre |
| **Contenus** | Seulement les objets entièrement dans le cadre. Un groupe ou une entité de brush à moitié dedans n'est pas pris |
| **Centres** | Seulement les objets dont le centre est dans le cadre. Les centres sont dessinés en petites croix, et la sélection au clic les utilise aussi. Pratique dans les zones chargées |

- Maintenez `Alt` pendant le glisser pour l'autre mode (Touchés devient Contenus et inversement).
- Maintenez `Ctrl` pour ajouter les objets du cadre à la sélection. `Ctrl+glisser` trace toujours un cadre, même au-dessus d'un objet ou de la sélection.

### Sélection au pinceau (vue 3D)

`Ctrl+glisser` dans la vue 3D, à l'écart du gizmo, ajoute chaque objet survolé. `Ctrl+Maj+glisser` les retire.

### Modes de sélection

Ce que sélectionne un clic sur un solide dépend du mode de sélection, réglé dans la barre d'outils principale ou dans **Édition > Mode de sélection** :

| Mode | Un clic sélectionne | Par défaut |
|---|---|---|
| **Groupes** | Tout le groupe le plus extérieur, ou toute l'entité de brush | Oui |
| **Objets** | Les entités de brush en entier, sans tenir compte des groupes | |
| **Solides** | Le solide seul, même dans une entité de brush | |

**Ignorer les groupes** (`Ctrl+W`, **Édition > Mode de sélection**) passe en Solides, puis revient au mode d'avant. Le mode est enregistré dans `settings.toml` (`[edit] select_mode`). Voir [Groupes](groups.md).

### Sélectionner des faces, des arêtes ou des sommets (Choisir)

Les boutons **Choisir** de la barre d'options (aussi **Édition > Élément de sélection**) fixent le type d'élément qu'un clic dans une vue 3D sélectionne. Le réglage est enregistré dans `settings.toml` (`[selection] element`).

| Choisir | Touche | Un clic sélectionne | Utilisé par |
|---|---|---|---|
| **Objet** | `4` | L'objet entier, selon le mode de sélection (par défaut) | Tout le reste |
| **Sommet** | `1` | Le coin de brush le plus proche du curseur | L'outil vertex s'ouvre avec lui sélectionné |
| **Arête** | `2` | L'arête de brush la plus proche du curseur | `Ctrl+B` biseaute les arêtes sélectionnées ; l'outil vertex s'ouvre avec elles sélectionnées |
| **Face** | `3` | La face sous le curseur (la sélection de faces de l'outil texture) | Décalage `I` et extrusion `E` |

`Maj` ou `Ctrl` ajoute à la sélection ou en retire, un clic simple la remplace, et un clic dans le vide l'efface. Les arêtes et les sommets sont pris à quelques pixels du curseur et seulement s'ils ne sont pas cachés derrière un autre brush ; choisir une arête ou un sommet sélectionne aussi son brush, pour que l'outil vertex puisse le modifier. Ce qui est sélectionné est dessiné dans tous les modes de rendu, dans les vues 3D comme dans les vues 2D : les faces sélectionnées ont un remplissage cyan translucide avec un contour épais (le remplissage se voit aussi faiblement à travers les autres géométries), les arêtes sélectionnées sont des lignes cyan épaisses toujours visibles, et les sommets sélectionnés de gros points cyan à centre blanc. Les objets sélectionnés gardent la teinte de sélection habituelle. Quand un choix d'élément est actif, les flèches, G, le glisser direct sur un élément choisi en 3D et le gizmo de déplacement (3D) ou le glisser de déplacement (2D) déplacent les faces, arêtes ou sommets choisis eux-mêmes au lieu du brush entier : les coins des éléments choisis sont déplacés du décalage (aimanté comme un déplacement d'objet, `Alt` pour des unités simples) et le brush est reconstruit, ce qui peut scinder une face quand le déplacement la plie ; un déplacement qui donnerait un brush invalide est refusé avec un message. Les éléments choisis le restent après le déplacement, tout le déplacement est une seule étape d'annulation, et les poignées de rotation et d'échelle ne sont pas proposées pour eux. Toute autre modification de la carte, ainsi qu'annuler ou rétablir, efface les arêtes et sommets sélectionnés (les faces sélectionnées restent sélectionnées tant qu'elles existent). Maj plus les chiffres (1 sommet, 2 arête, 3 face, 4 objet) sont les raccourcis des actions `select_element_vertex`, `select_element_edge`, `select_element_face` et `select_element_object` ; ils se changent dans **Réglages > Clavier**. Dans les vues 2D, un clic sélectionne toujours des objets. L'outil MCP `select_elements` sélectionne des arêtes ou des sommets par leur numéro, `select_faces` des faces, et `get_selection` et `set_tool_options` (outil `select`, option `element`) lisent et changent le réglage.

### Commandes du menu Édition

| Commande | Raccourci | Effet |
|---|---|---|
| **Tout sélectionner** | `Ctrl+A` | Sélectionne tous les objets visibles |
| **Effacer la sélection** | `Ctrl+Maj+A` (et aussi `Maj+Q`) | Désélectionne tout |
| **Inverser la sélection** | `Ctrl+I` | Sélectionne ce qui ne l'était pas |
| **Sélection de faces précédente** | `Alt+Z` | Revient à la sélection de faces précédente (passe à l'outil de texture, où les faces s'affichent) |
| **Sélection de faces suivante** | `Alt+Y` | Repart en avant |

Avec les préréglages clavier **Hammer** et **Hammer++**, Tout sélectionner devient `Ctrl+Maj+A`, Effacer la sélection `Maj+Q` et Inverser la sélection `Ctrl+Maj+I`. Voir [Raccourcis clavier](keyboard-shortcuts.md).

`Échap` vide la sélection (ou annule le glisser en cours).

### Sélectionner par classe ou par matériau

Au bout de la barre d'options se trouvent un choix **Classe** / **Matériau**, un champ texte et un bouton **Sélectionner**. Tapez une classe (par exemple `prop_*`) ou un matériau (par exemple `tools/*`), puis `Entrée` ou **Sélectionner** :

- **Classe** sélectionne toutes les entités visibles de cette classe, **Matériau** tous les objets qui utilisent ce matériau.
- Les jokers `*` et `?` marchent. Sans joker, Hammer Pro cherche d'abord le nom exact, puis tout nom qui contient le texte.
- Maintenez `Ctrl` en validant pour ajouter à la sélection.
- La barre affiche ensuite **{n} trouvé(s)** ou **Aucun résultat**. Une recherche sans résultat garde la sélection actuelle.

## Transformer dans les vues 2D

Glissez à l'intérieur du cadre de sélection pour le déplacer. Glissez un objet non sélectionné pour le sélectionner et le déplacer d'un coup. Les poignées du cadre dépendent du mode :

| Mode des poignées | Poignées | Le glisser |
|---|---|---|
| **Échelle** (par défaut) | Coins et côtés | Redimensionne la sélection. `Ctrl` : garde les proportions (coin) ou met à l'échelle depuis le centre (côté) |
| **Rotation** | Coins | Fait pivoter autour du pivot (voir Pivot déplaçable plus bas), par pas de rotation. Maintenez la touche de rotation libre (`Ctrl` par défaut) pour un angle libre |
| **Inclinaison** | Côtés | Incline la sélection le long du côté, calée sur la grille |

Cliquez sur la sélection (ou dans le vide à l'intérieur de son cadre) pour changer de mode. Une nouvelle sélection repart toujours en mode Échelle.

Touches pendant n'importe quel glisser :

| Touche | Effet |
|---|---|
| `Maj` | Clone : le glisser crée une copie transformée et laisse l'original (une étape d'annulation "Duplicate"). Un nœud de chemin cloné seul se relie à la suite de sa source |
| `Alt` | Pas de magnétisme de grille pour ce glisser |
| `Échap` | Annule le glisser |

Au déplacement, le point calé sur la grille est le coin minimum des brushes (un prop hors grille sélectionné avec eux ne les sort pas de la grille), ou l'origine d'une entité ponctuelle seule.

## Transformer dans la vue 3D

### Gizmo de transformation

Le gizmo se place sur le pivot de la sélection (le pivot déplaçable, l'origine d'une entité ponctuelle seule, sinon le centre de la boîte englobante) et garde la même taille à l'écran. Son mode se choisit dans la barre d'options :

| Mode du gizmo | Éléments |
|---|---|
| **Déplacer** (par défaut) | Les flèches déplacent sur un axe, les carrés dans un plan |
| **Pivoter** | Les anneaux font tourner autour d'un axe, par pas de rotation (touche de rotation libre : sans pas) |
| **Échelle** | Les bras à bout cubique mettent un axe à l'échelle (taille calée sur la grille), les carrés deux axes, le carré central tout uniformément (facteurs arrondis à 0,1). L'échelle va de 0,01 à 100 |

`Maj` clone, `Alt` coupe le magnétisme. **Local** (ou `Ctrl+J`, **Gizmo en repère local**, dans la palette de commandes) aligne le gizmo sur les angles d'une entité ponctuelle sélectionnée seule plutôt que sur les axes du monde. Décochez **Afficher** pour masquer le gizmo.

### Poignées d'étirement 3D

Avec **Étirement 3D** activé (par défaut), chaque face de la boîte de la sélection reçoit une flèche aux couleurs des axes. Glissez-en une pour déplacer cette face sur son axe pendant que la face opposée reste en place, comme les poignées d'échelle en 2D (verrouillage des textures compris, étape d'annulation "Stretch").

- `Ctrl` : étire depuis le centre (la face opposée part dans l'autre sens).
- `Maj` : clone. `Alt` : sans magnétisme.
- Les flèches n'apparaissent que si la sélection contient des brushes (solides, entités de brush, groupes de ceux-ci). Des entités ponctuelles seules sont seulement déplacées ; celles sélectionnées avec des brushes gardent leur place dans la boîte étirée.
- Les flèches des faces tournées vers l'arrière sont plus petites et estompées, celles qui pointent vers la caméra sont masquées, et une flèche qui couvrirait le gizmo glisse le long de son axe pour le dégager.

### Transformations G, R et E

Avec des objets sélectionnés et le pointeur sur une vue, appuyez sur :

- `G` pour déplacer, `R` pour pivoter, `E` pour mettre à l'échelle, en bougeant la souris sans tenir de bouton.
- `X`, `Y` ou `Z` pour bloquer un axe, `Maj+X`, `Maj+Y` ou `Maj+Z` pour bloquer le plan perpendiculaire. La même touche une seconde fois libère.
- `G`, `R` ou `E` de nouveau pour changer de transformation.
- Un clic ou `Entrée` applique, `Échap` ou un clic droit annule.
- `Alt` : sans magnétisme. Pour les rotations, la touche de rotation libre supprime le pas d'angle.

Pendant une transformation G/R/E, `Z` bloque l'axe Z au lieu d'entrer en vue souris.

### Alt + molette : échelle des modèles

`Alt+molette` sur une vue multiplie l'échelle des props sélectionnés par 1,1 par cran (de 0,01 à 100) : `modelscale` pour `prop_dynamic` et `prop_physics` (ou toute classe dont le FGD l'a), `uniformscale` pour `prop_static` quand le FGD la déclare. Les crans successifs forment une seule étape d'annulation. Les props statiques dont le FGD n'a pas de clé `uniformscale` sont refusés avec un avertissement.

## Pivot déplaçable

**Pivot déplaçable** (menu **Outils**, bouton **Pivot** de la barre d'options, ou menu contextuel des vues) affiche une croix autour de laquelle tournent les rotations, les mises à l'échelle, la boîte [Transformer](transform-align.md) et le [collage spécial](clipboard.md).

- Glissez sa croix dans une vue 2D pour la déplacer (calée sur la grille, `Alt` : sans magnétisme). Dans la vue 3D, attrapez-la près du centre du gizmo (pas en mode Échelle).
- Il suit les objets quand ils bougent, et disparaît quand l'ensemble sélectionné change.

## Points d'aide des entités

Quand une seule entité est sélectionnée, ses points d'aide (clés de type `origin`, `vecline` ou `axis` dans le FGD, comme l'axe de charnière d'un `phys_hinge` ou l'origine de rotation d'un `func_rotating`) s'affichent en poignées rondes. Glissez-en une pour écrire la clé (une étape d'annulation, `Alt` : sans magnétisme, `Échap` : annuler).

## Décaler au clavier

Avec des objets sélectionnés et le pointeur sur une vue (et **Décaler avec les flèches** activé) :

| Touches | Effet |
|---|---|
| Flèches | Déplacent d'un pas de grille dans le plan de la vue. Dans la vue 3D, sur les axes du monde les plus proches de la droite et de l'avant de la caméra |
| `Page préc.` / `Page suiv.` | Vers le spectateur / à l'opposé dans une vue 2D, vers le haut / le bas dans la vue 3D |
| `Alt+flèche` | Déplace d'une unité |
| `Maj+flèche` | Clone à un pas de grille (ou déplace d'une unité si **Maj+flèches clone** est désactivé) |

Sans sélection, les flèches font défiler la vue 2D (voir [Caméra et navigation](camera-navigation.md)).

## Boucles d'arêtes

L'outil de sélection choisit des brushes entiers, pas des arêtes : il n'a donc pas de mode arête. Les commandes de boucle et d'anneau d'arêtes sont dans l'[outil vertex](vertex-tool.md#boucles-et-anneaux-darêtes) (`Alt`+clic sur une arête) et dans l'[outil biseau](bevel-tool.md) (`Alt`+clic sur une arête pour choisir sa boucle avant un biseau). Pour travailler sur une boucle depuis ici, sélectionnez les brushes qui la forment, appuyez sur `Maj+V` pour l'outil vertex et faites `Alt`+clic sur une arête. Les commandes de **Édition > Élément de sélection** et **Outils > Sommets et arêtes > Faire glisser la boucle d'arêtes...** fonctionnent dans l'outil vertex.

## Menu contextuel des vues

Avec une sélection, un clic droit dans une vue ajoute en haut du menu : **Poser au sol**, **Coller au plafond**, leurs versions "aligné sur la surface", **Centrer les origines** et **Pivot déplaçable**. Ces commandes sont décrites dans [Transformer et aligner](transform-align.md).

## Barre d'options

| Option | Effet | Par défaut |
|---|---|---|
| **Choisir :** Objet / Sommet / Arête / Face | Ce qu'un clic dans une vue 3D sélectionne (voir plus haut) | Objet |
| **Cadre :** Touchés / Contenus / Centres | Ce que prend une sélection par cadre (voir plus haut) | Touchés |
| **Entrée** | Le cadre attend `Entrée` et peut d'abord être redimensionné et déplacé (enregistré dans les paramètres) | Désactivé |
| **Poignées :** Échelle / Rotation / Inclinaison | Rôle des poignées 2D | Échelle |
| **Étirement 3D** | Flèches sur les faces des brushes sélectionnés dans la vue 3D (enregistré dans les paramètres) | Activé |
| **Gizmo :** Afficher | Affiche le gizmo de transformation dans la vue 3D | Activé |
| Déplacer / Pivoter / Échelle | Rôle du gizmo | Déplacer |
| **Local** | Le gizmo d'une entité ponctuelle seule suit ses angles (enregistré dans les paramètres) | Désactivé |
| **Pivot** | Affiche le pivot déplaçable | Désactivé |
| **Classe** / **Matériau**, champ, **Sélectionner** | Sélection par classe ou par matériau | Classe |

Les choix du cadre, des poignées et du gizmo durent le temps de la session. Ces options sont aussi accessibles par leur nom dans les scripts (`tool_option`) et via les outils MCP `get_tool_options` / `set_tool_options` (`handles`, `box`, `gizmo`, `gizmo_mode`, `gizmo_local`, `stretch_handles`, `box_confirm`, `pending_box`). `pending_box` lit le cadre en attente, prend `x1 y1 x2 y2` pour en tracer un dans une vue 2D, `confirm`, `add` ou `cancel`.

## Paramètres

Dans **Outils > Options...**, page **Sélection** (`[selection]` dans `settings.toml`) :

| Option | Effet | Par défaut |
|---|---|---|
| **Pas de rotation** | Angle sur lequel s'arrêtent les poignées de rotation, les anneaux du gizmo et la touche R (1° à 90°) | 15° |
| **Touche de rotation libre** | Touche à maintenir pour un angle libre : Ctrl ou Alt | Ctrl |
| **Taille des poignées** | Taille des poignées 2D, de 4 à 16 points | 7 |
| **Garder les groupes de visibilité au clonage** | Les copies faites en glissant ou avec Maj+flèches restent dans les visgroups de leurs originaux | Activé |
| **Maj+flèches clone** | Maj+flèches copie la sélection à un pas de grille, Alt+flèches décale alors d'une unité | Activé |
| **Sélection par cadre ajustable (Entrée pour valider)** | Comme la case **Entrée** de la barre d'options | Désactivé |
| **Poignées d'étirement 3D** | Comme la case de la barre d'options | Activé |
| **Gizmo en repère local** | Comme le bouton Local | Désactivé |

Page **Édition** : **Décaler avec les flèches** (activé) et **Annuler restaure la sélection** (activé : les changements de sélection sont des étapes d'annulation, voir [Annulation et historique](undo-history.md)). Page **Vues** : **Contour des objets sous la souris** (désactivé) entoure ce qui est sous le pointeur dans la vue 3D. Le magnétisme suit **Map > Magnétisme de grille** (`Maj+W`) et la grille active (voir [Grilles](grids.md)).

## Astuces

- Dans une zone chargée, passez le cadre en **Centres**, ou cliquez plusieurs fois au même endroit pour descendre dans les objets superposés.
- `Maj+glisser` un nœud de chemin pour poser un chemin en vitesse : chaque clone se relie à la suite de sa source.
- Pour des valeurs exactes, utilisez la boîte [Transformer](transform-align.md) (`Ctrl+M`).

## FAQ

**Pourquoi un clic sur ma sélection ne prend pas l'objet derrière ?** Un clic sur un objet qui forme déjà toute la sélection change le mode des poignées. Recliquez au même endroit dans les 2 secondes (ou appuyez sur `Page suiv.`) pour atteindre l'objet derrière.

**Je clique sur un mur et autre chose est sélectionné.** Un second clic rapide au même endroit descend vers l'objet derrière : attendez 2 secondes, ou cliquez une seule fois. Les brushes à texture d'outil cachée (nodraw, clips, triggers, hints, voir le menu Affichage) et les objets que la vue 3D ne dessine pas n'y sont jamais choisis, mais les vues 2D les montrent et les sélectionnent toujours.

**Ma rotation saute de 15 degrés en 15 degrés.** C'est le pas de rotation. Maintenez `Ctrl` (ou `Alt`, selon **Touche de rotation libre**) pour un angle libre, ou changez le pas dans la page **Sélection** des paramètres.

**Les flèches d'étirement n'apparaissent pas.** Elles ne s'affichent que pour une sélection qui contient des brushes, avec **Étirement 3D** activé. Les flèches pointées droit sur la caméra sont masquées : tournez un peu la caméra.

**Je peux sélectionner un seul brush dans un func_detail ?** Oui : passez en mode **Solides** (ou `Ctrl+W`), puis cliquez dessus.

## Pages liées

- [Caméra et navigation](camera-navigation.md)
- [Transformer et aligner](transform-align.md)
- [Presse-papiers](clipboard.md)
- [Groupes](groups.md)
- [Grilles](grids.md)
- [Outil de texture](texture-tool.md)
- [Raccourcis clavier](keyboard-shortcuts.md)
- [Référence des paramètres](../reference/settings.md)
- [Référence des outils MCP (anglais)](../../MCP.md)

---
[Retour à l'index de la documentation](../README.md)
