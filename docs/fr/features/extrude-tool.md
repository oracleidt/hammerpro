[Documentation](../README.md) > Fonctionnalités > Outil extruder / creuser | [English](../../en/features/extrude-tool.md) | [Русский](../../ru/features/extrude-tool.md)

# Outil extruder / creuser

L'outil extruder / creuser découpe des formes plus facilement que l'outil de découpe : vous cliquez les points d'un contour sur une face de brush (une fenêtre, une porte, une niche, un logo...), puis vous l'enfoncez dans le brush pour creuser un renfoncement ou une ouverture, ou vous le tirez hors de la face pour l'extruder. Le contour peut avoir n'importe quelle forme, concave aussi : l'outil le découpe en morceaux convexes, si bien que chaque brush créé est un brush convexe valide.

## Ouvrir l'outil

- **Outils > Outil actif > Outil extruder / creuser**, la bande d'outils (icône pioche, dans le groupe d'édition à côté de l'outil de découpe), ou `Maj+I`.
- Appuyer de nouveau sur `Maj+I` quand l'outil est actif bascule entre **Creuser** et **Extruder**, comme `Tab`.

## Tracer le contour

1. Cliquez sur une face de brush, dans la vue 3D ou dans une vue 2D qui regarde la face (la vue de dessus pour un sol, la vue de face ou de côté pour un mur). La face sous la souris est entourée avant le clic. Quand des brushes sont sélectionnés, leurs faces l'emportent sur les autres brushes, même quand un autre brush (un trigger, un pilier) est devant : sélectionnez d'abord le brush à creuser. Ce clic pose le premier point et choisit la face ; les points suivants restent sur le plan de cette face.
2. Cliquez les points suivants autour de la forme. Dans la vue 3D, les points s'alignent sur un coin ou sur le milieu d'une arête de la face quand la souris en est à environ 10 pixels, sinon sur la grille dans le plan de la face. Près d'une arête qui n'est pas sur la grille, le point reste sur l'arête au lieu de sauter hors de la face. Maintenez `Alt` pour poser les points sans aucun magnétisme. La longueur de l'arête en cours s'affiche près du curseur.
3. Un point qui ferait se croiser le contour est refusé : le repère devient rouge avec **Le contour se croise**. Un point déjà présent est ignoré. Un contour compte au plus 128 points.
4. Fermez le contour en cliquant de nouveau sur son premier point, ou en double-cliquant. Vous pouvez aussi appuyer sur `Entrée` dès 3 points : le contour est fermé et appliqué tout de suite avec la profondeur actuelle.

`Retour arrière` (ou **Retirer le point**) retire le dernier point, ou rouvre un contour fermé. `Échap` annule le contour.

Une vue 2D qui regarde la face par la tranche (un mur vu de dessus) ne peut pas y poser de points ; servez-vous-en pour régler la profondeur.

## Régler la profondeur

- Une fois le contour fermé, glissez n'importe où dans une vue (3D ou 2D). Le glissement modifie la profondeur à partir de sa valeur actuelle : appuyer sur la souris ne fait jamais sauter la profondeur. Déplacez la souris dans le sens où la normale de la face pointe à l'écran pour tirer la face (extruder), dans l'autre sens pour l'enfoncer (creuser) ; l'étiquette près du contour montre le résultat. Quand vous regardez la face de face, glissez vers le haut pour la tirer vers vous et vers le bas pour l'enfoncer. La profondeur s'aligne sur la grille. En 2D, les vues qui montrent la face par la tranche sont les plus pratiques.
- Ou tapez la valeur dans le champ **Profondeur** de la barre d'options.
- Le glisser de profondeur suit la grille active, donc la nouvelle face tombe sur un plan de la grille (le pas est gardé entre 1 et 1024 unités). Maintenez `Alt` pendant le glisser pour utiliser la grille alternative : une grille alignée sur les axes, de la moitié de la taille, qui ignore la rotation de la grille (voir [Grille pour le biseau, le décalage et l'extrusion](#grille-pour-le-biseau-le-décalage-et-lextrusion)). La valeur tapée dans le champ **Profondeur** n'est jamais aimantée.
- **Traversant** (creuser seulement) coupe toute l'épaisseur du brush : une ouverture au lieu d'un renfoncement. La barre d'options affiche l'épaisseur du brush.
- Glissez un point d'un contour fermé pour le déplacer (un déplacement qui ferait se croiser le contour est refusé).

Les vues 3D et 2D montrent le résultat en direct pendant le tracé et le réglage de la profondeur : en vert les brushes qui resteront, en rouge le volume creusé, en jaune le prisme extrudé, et la profondeur près du contour. Un contour que l'outil ne peut pas appliquer (hors de la face, qui se croise) affiche la raison en rouge.

Appuyez sur `Entrée` ou cliquez sur **Appliquer**. Toute l'opération est une seule étape d'annulation (**Creusement de face** ou **Extrusion de face**).

## Ce que font creuser et extruder

| Mode | Résultat |
|---|---|
| **Creuser** | Le brush est remplacé par les brushes convexes qui forment le brush moins le prisme du contour, comme **Creuser** limité à ce brush. Le brush garde son identifiant (il devient le premier morceau), les autres morceaux sont de nouveaux brushes du même propriétaire (monde ou entité de brush), groupe et visgroups. Les morceaux qui peuvent redevenir un seul brush convexe sont fusionnés, pour obtenir le moins de brushes possible. Si le contour couvre toute la face et traverse, le brush est supprimé. |
| **Extruder** | Le prisme du contour est ajouté sur la face sous forme de nouveaux brushes du même propriétaire. Quand le brush et le prisme restent convexes ensemble (un contour qui couvre toute la face), c'est le brush lui-même qui grandit et garde son identifiant. |

Textures :

- Les faces conservées du brush gardent leur matériau et l'alignement de leur texture.
- Les nouvelles faces (les parois et le fond du trou, ou les faces du prisme extrudé) reçoivent la texture choisie dans **Nouvelles faces** : **Texture de la face** (le matériau de la face cliquée) ou **Matériau actuel**. Les faces parallèles à la face cliquée (le fond d'un renfoncement, le devant d'une extrusion) prennent l'alignement de la face cliquée, la texture se prolonge donc ; les autres nouvelles faces sont alignées sur le monde avec l'échelle de la face cliquée.
- Les overlays et cubemaps qui utilisaient la face cliquée suivent ses morceaux.

## Options

| Option | Rôle | Par défaut |
|---|---|---|
| **Creuser** / **Extruder** | Creuser dans le brush, ou sortir de la face (`Tab` ou `Maj+I` pour alterner). Glisser la profondeur le règle aussi. | Creuser |
| **Profondeur** | Profondeur en unités (0,125 à 16384). | 16 |
| **Traversant** | Creuser toute l'épaisseur du brush. | désactivé |
| **Nouvelles faces** | **Texture de la face** ou **Matériau actuel** (avec la vignette du matériau actuel ; cliquez dessus pour afficher le navigateur de textures). | Texture de la face |
| **Retirer le point** | Retirer le dernier point, ou rouvrir le contour (`Retour arrière`). | |
| **Appliquer** | Creuser ou extruder le contour (`Entrée`). | |
| **Effacer** | Abandonner le contour. | |
| Nombre de points et résultat | **N point(s)** et **Résultat : N brush(es)**, ou la raison pour laquelle le contour ne peut pas être appliqué. | |

## Astuces

- Pour une fenêtre, tracez le contour sur le mur dans la vue de face ou de côté (ou directement en 3D), activez **Traversant** et appuyez sur `Entrée`.
- Pour une niche ou un renfoncement d'enseigne, laissez **Traversant** désactivé et glissez la profondeur dans la vue de dessus.
- Un contour plus grand que la face convient : seule la partie au-dessus du brush est creusée.
- Creuser ne touche que le brush cliqué. Pour découper une forme à travers plusieurs brushes, utilisez [Creuser](brush-operations.md) avec un brush, ou répétez l'outil sur chaque brush.
- Les faces d'un trou creusé sont de vraies faces : changez leur texture ensuite avec l'[outil texture](texture-tool.md).

## Décaler une face

**Outils > Opérations sur les brushes > Décaler la face...** (aussi le bouton **Décaler la face...** de la barre d'options de cet outil) agit sur les faces sélectionnées, avec l'outil texture ou la sélection de faces de l'outil de sélection. Il fait reculer les arêtes de chaque face vers l'intérieur d'une distance et découpe le brush en morceaux convexes : un cadre de brushes autour de la face plus le bloc intérieur, de sorte que le résultat reste composé de brushes convexes valides. Décaler la face d'une boîte donne 5 brushes (4 pour le cadre, 1 bloc intérieur) dont les volumes s'additionnent au volume du brush d'origine.

La fenêtre demande :

- **Distance** : de combien les arêtes reculent vers l'intérieur, en unités (0,125 à 16384). Elle doit rester inférieure à la moitié de la partie la plus étroite de la face.
- **Profondeur** : 0 laisse le bloc intérieur au ras de la face. Une valeur positive le surélève hors de la face, une valeur négative l'enfonce dans le brush (décalage puis extrusion, comme I puis E dans Blender). L'enfoncement doit rester sous l'épaisseur du brush.
- **Aligner sur la grille** : arrondit la distance et la profondeur à la grille (activé par défaut quand l'aimantation est active).

Les faces conservées gardent leur texture et leur alignement. Les nouvelles faces (les côtés du trou du cadre et du bloc intérieur) prennent le matériau de la face décalée et, quand elles lui sont parallèles, son alignement. Les overlays et cubemaps qui utilisaient la face suivent ses morceaux. Le premier morceau garde l'id du brush, les nouveaux morceaux ont le même propriétaire, groupe et visgroups, et toute l'opération est une seule étape d'annulation (**Décalage de face**). Tous les morceaux sont sélectionnés.

Limites : le brush doit être convexe et valide, et les faces de displacement sont refusées. Le cadre est toujours fait de quadrilatères, un par arête de la face (voir plus bas). Quand la distance dépasse ce que la face permet, un avertissement l'explique (**La distance dépasse la taille de la face**) et rien n'est modifié.

Avec plusieurs faces sélectionnées, la fenêtre décale en une seule région les faces coplanaires qui se touchent, comme le décalage interactif (voir plus bas) ; l'outil MCP `inset_face` propose `individual` pour décaler chaque face seule.

## Décalage interactif (I)

Le décalage interactif fonctionne comme l'Inset de Blender (`I`), sur le même code géométrique que **Décaler la face...**.

1. Choisissez les faces. Sélectionnez une ou plusieurs faces avec la sélection de faces (l'élément **Face** de l'outil de sélection, l'outil texture, ou, avec l'outil vertex, tous les coins des faces à décaler). Sans face sélectionnée, la face sous la souris est utilisée : placez le curseur sur une face de brush dans la vue 3D et appuyez sur `I`.
2. Placez la souris sur la vue 3D et appuyez sur `I`. Tant qu'une face est sélectionnée ou survolée et qu'une vue 3D est sous la souris, `I` lance le décalage. Vous pouvez aussi utiliser **Outils > Opérations sur les brushes > Décalage interactif (I)**, la palette de commandes, ou le bouton **Décalage interactif (I)** de la barre d'options de cet outil : déplacez ensuite la souris sur une vue pour commencer.
3. Bougez la souris. L'épaisseur est la distance entre le centre de la face et le curseur, relative à la position de la souris au départ : aller vers le centre de la face épaissit le cadre, s'en éloigner le ramène à zéro. Un aperçu en direct montre la nouvelle face intérieure (jaune, remplie) et les lignes de coupe vers les coins du contour (vert) dans les vues 3D et 2D. La valeur s'affiche au curseur et dans la barre d'état, avec la grille utilisée, le nombre de brushes du résultat, ou la raison pour laquelle il ne peut pas être appliqué (en rouge).
4. L'épaisseur et la profondeur suivent d'elles-mêmes la grille active (voir [Grille](#grille-pour-le-biseau-le-décalage-et-lextrusion)). Maintenez `Ctrl` pour forcer la grille même si l'aimantation est coupée, `Alt` pour une valeur libre.
5. Tapez un nombre pour fixer une valeur exacte (chiffres, `.` ou `,`, et `-` pour la profondeur). `Retour arrière` la corrige, et la vider rend le contrôle à la souris.
6. Appuyez sur `E` pour passer de **Décalage** (épaisseur) à **Profondeur**. En mode profondeur, bouger la souris dans le sens où pointe la normale de la face à l'écran surélève la face intérieure hors du brush et l'autre sens l'enfonce : la face intérieure peut donc être extrudée ou enfoncée dans la même opération. La valeur entre crochets est celle que la souris modifie.
7. Avec plusieurs faces sélectionnées, appuyez de nouveau sur `I` pour passer de **région** (par défaut) à **individuel**, comme le `I` de Blender bascule les faces individuelles. L'étiquette indique le mode actif.
8. Clic gauche ou `Entrée` applique, en une seule étape d'annulation (**Décalage de face**). `Échap` ou clic droit annule et rien n'est modifié. Une épaisseur inférieure à 0,125 ou supérieure à ce que la face permet est refusée avec un message et l'outil reste actif.

Le premier `I` sur une face est le décalage classique. Le résultat est le même que **Décaler la face...** : le cadre autour de la nouvelle face intérieure est fait uniquement de quadrilatères, un brush convexe par arête du contour (un triangle donne 3 brushes de cadre, un quadrilatère 4, un pentagone 5, etc.), plus le bloc intérieur. Les morceaux sont coupés le long de la ligne qui va de chaque coin du contour au coin correspondant de la face intérieure. Chaque morceau est un brush convexe valide, les volumes s'additionnent au volume du brush d'origine, et les faces qui restent visibles gardent leur texture et leur alignement. Une face avec une arête courte (un trapèze, par exemple) ne peut pas être décalée au-delà du point où cette arête disparaîtrait : la distance est alors refusée plutôt que de produire des triangles dans le cadre.

### Plusieurs faces : région ou individuel

Avec plusieurs faces sélectionnées, `I` les décale ensemble, en région, comme l'Inset de Blender sans faces individuelles :

- Des faces situées dans le même plan et qui partagent une arête (les dessus de brushes côte à côte, un sol ou un mur fait de plusieurs brushes) forment une région. La région a un seul contour, celui de l'union des faces, et un seul bloc intérieur : il n'y a pas de cadre le long des arêtes que les faces partagent, le cadre traverse les jointures et chaque brush en reçoit sa part. Le contour peut être concave (un L fait de trois faces, par exemple). La région ne doit avoir ni trou ni morceaux qui se touchent seulement par un coin, sinon un message l'indique (**Les faces sélectionnées ne forment pas une région simple**).
- Des faces qui ne sont pas dans le même plan, ou éloignées, ou qui se touchent seulement par un coin, sont décalées chacune seule avec la même épaisseur et la même profondeur. Le dessus et la face avant d'un même brush sont deux faces de ce genre : les morceaux pavent le brush (les volumes s'additionnent) et les faces gardent chacune leur cadre.
- **Individuel** (`I` de nouveau, ou `individual: true` dans l'outil MCP) décale chaque face seule même si elles se touchent, ce qui donne un cadre par face.

Le résultat est toujours fait de brushes convexes. C'est pourquoi une région est limitée aux faces coplanaires : sur un brush convexe, deux faces adjacentes dans des plans différents se rejoignent en un pli, et un cadre partagé devrait se replier autour, ce qui ne peut pas être découpé en brushes convexes.

La touche `I` est le raccourci de l'action `inset_interactive`, et elle ne lance le décalage que lorsqu'une face est sélectionnée ou survolée et qu'une vue 3D a la souris. Vous pouvez la réassigner dans les réglages du clavier.

## Extrusion interactive (E)

L'extrusion interactive tire une face hors de son brush sous la forme d'un nouveau brush, comme l'Extrude de Blender (`E`) mais en créant un brush convexe séparé, ou l'enfonce dans son brush pour y creuser de la matière (distance négative). Elle est aussi dans **Outils > Opérations sur les brushes > Extrusion interactive (E)**, l'action `extrude_interactive` et la palette de commandes.

1. Choisissez les faces comme pour le décalage : les faces sélectionnées, ou la face sous la souris quand aucune n'est sélectionnée. Plusieurs faces fonctionnent ensemble : chacune devient son propre nouveau brush, tiré le long de sa propre normale, en une seule étape d'annulation.
2. Placez la souris sur une vue 3D et appuyez sur `E` (la touche est libre dans les préréglages de clavier ; dans l'outil de sélection `E` lance toujours la mise à l'échelle des objets sélectionnés, mais pas tant que des faces sont sélectionnées). Bougez la souris : aller dans le sens où pointe la normale de la face à l'écran rend le nouveau brush plus haut (aperçu jaune du dessus de chaque nouveau brush) ; aller dans l'autre sens au-delà de zéro donne une distance négative, qui creuse : un aperçu rouge-orange montre la face qui entre dans le brush et l'étiquette affiche une distance négative. Pour creuser une poche au milieu d'une face, utilisez un décalage avec une profondeur négative ou le mode creuser de cet outil.
3. La distance suit la grille active (voir ci-dessous), même négative. Tapez un nombre pour une valeur exacte, qui n'est pas aimantée ; tapez d'abord `-` pour une valeur négative (`-16`). Clic gauche ou `Entrée` applique (**Extrusion de face** pour de nouveaux brushes, **Creusement de face** quand toutes les distances sont négatives), `Échap` ou clic droit annule. Une distance dont la taille est inférieure à 0,125 est refusée.

Le nouveau brush est un brush convexe séparé qui part de la face et reprend sa texture et son alignement : il n'est jamais fusionné au brush d'origine, c'est donc un nouvel objet que vous pouvez déplacer ou supprimer seul. Il appartient à la même entité que le brush dont il vient.

Une distance négative n'ajoute pas de brush : elle retire de son propre brush la tranche de matière de cette épaisseur sous la face, donc la face entre dans le brush et le brush reste un seul brush convexe valide avec son identifiant (la nouvelle face reprend la texture de l'ancienne). Plusieurs faces peuvent être creusées ensemble, même sur le même brush, et mélangées à des faces extrudées en une seule étape d'annulation. Une découpe qui traverserait tout le brush est refusée avec un message, rien ne change.

## Grille pour le biseau, le décalage et l'extrusion

Les distances à la souris du rayon du [biseau](bevel-tool.md), de l'épaisseur et de la profondeur du décalage et de la distance de l'extrusion s'aimantent d'elles-mêmes à la grille active tant que **Aimanter à la grille** est activé :

- La taille de la grille, une grille personnalisée (pas, origine et angles) et une grille pivotée sont respectées. La distance d'une extrusion ou d'une profondeur de décalage est choisie pour que le nouveau plan supérieur tombe sur un plan de la grille dans la direction de la face (quand la normale de la face suit un axe de la grille) ; dans les autres directions c'est un multiple de la taille de la grille. L'épaisseur du décalage est un multiple du plus petit pas de la grille le long de la face, donc les nouvelles arêtes d'une face située sur la grille restent sur la grille.
- La taille de grille utilisée par ces trois outils est gardée entre 1 et 1024 unités, quel que soit le réglage de la grille (une grille de 0,25 agit comme 1 et une grille de 4096 comme 1024). Le réglage de la grille lui-même accepte toutes les tailles de 0,01 à 4096, donc 1 à 1024 sont toutes valides.
- Chaque brush créé par ces outils a ses nouvelles faces placées à partir de la géométrie d'origine par ces distances aimantées : une face qui était sur la grille donne de nouveaux coins qui sont sur la grille.
- `Ctrl` force la grille même si **Aimanter à la grille** est coupé. Pour le biseau et le décalage, `Alt` donne une valeur libre.
- Pour l'extrusion, `Alt` passe à la **grille alternative** et cesse de suivre l'alignement de la grille actuelle : une grille alignée sur les axes, à l'origine, de la moitié de la taille de la grille actuelle (le double quand la taille actuelle est 1, gardée entre 1 et 1024). Avec une grille de 16 elle aimante à 8, avec une grille de 1 à 2. Une grille pivotée ou personnalisée est ignorée tant que `Alt` est maintenu, ce qui permet d'aligner un nouveau brush sur la grille du monde. L'étiquette affiche `grille 16` ou `grille alt. 8`.

## MCP

L'outil MCP `extrude_face` fait l'outil extruder / creuser depuis une application d'IA : `solid`, `side` (l'indice de la face dans le solide, donné par `get_faces`), les `points` du contour en positions du monde sur la face, `mode` (`carve` ou `extrude`), `depth`, `through`, et la texture des nouvelles faces (`new_faces` : `face` ou `current`, ou un `material`). `get_tool_options` et `set_tool_options` lisent et changent les options de l'outil lui-même (outil `extrude` : `mode`, `depth`, `through`, `new_faces`). L'outil MCP `extrude_faces` est l'extrusion interactive : `faces` (par défaut les faces sélectionnées), `distance` (négative : creuse le brush au lieu d'en ajouter un, taille d'au moins 0,125), `snap` (aligner les plans supérieurs sur la grille active), `alt_grid` (la grille d'Alt), `material` et `dry_run`. L'outil MCP `inset_face` décale des faces : `faces` (`[{solid, side}]`, par défaut les faces sélectionnées, sinon les faces de `ids`), `distance`, `depth` (positif surélève, négatif enfonce le bloc intérieur), `individual` (chaque face seule au lieu du contour de région partagé), `snap` (la grille active, bornée de 1 à 1024), `material` et `dry_run` (donne le nombre de brushes, ou pourquoi cela échoue, sans modifier la carte). Les actions `inset_interactive` et `extrude_interactive` (commande de script `action extrude_interactive`) lancent les outils interactifs. Voir la [référence des outils MCP](../../MCP.md).

## FAQ

**Le contour commence sur le mauvais brush.** Sélectionnez le brush à creuser : ses faces l'emportent sur les brushes placés devant.

**Pourquoi mon point est-il refusé ?** Il ferait se croiser le contour, ou c'est le même point qu'un autre.

**Pourquoi rien ne se passe quand je clique dans une vue 2D ?** La vue regarde la face par la tranche. Cliquez sur la face dans la vue 3D ou dans une vue 2D qui lui fait face.

**Le résultat peut-il contenir des brushes invalides ?** Non. Le contour est découpé en morceaux convexes, chaque brush obtenu est vérifié, et les brushes qui peuvent être refusionnés le sont. Le compilateur ne voit que des brushes convexes ordinaires.

**Puis-je creuser un displacement ?** Non, les faces displacement sont refusées. Creusez le brush avant de transformer la face en displacement.

## Pages liées

- [Outil de découpe](clip-tool.md)
- [Outil polygone](polygon-tool.md)
- [Opérations sur les brushes](brush-operations.md)
- [Outil vertex](vertex-tool.md)
- [Grilles](grids.md)
- [Raccourcis clavier](keyboard-shortcuts.md)

---
[Retour à l'index de la documentation](../README.md)
