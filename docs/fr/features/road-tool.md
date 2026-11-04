[Documentation](../README.md) > Fonctionnalités > Outil route | [English](../../en/features/road-tool.md)

# Outil route

L'outil route construit une route entière le long d'une spline : des voies d'une largeur choisie, des accotements, un matériau de voie, des lignes centrale, de voie et de rive (continues, pointillées ou doubles) avec leur propre matériau, un dévers, une montée et une descente en douceur le long des points, et au besoin une route qui suit le sol. La route est faite de brushes convexes, un par voie, par ligne et par accotement dans chaque segment, avec la texture qui suit la route sans raccord visible. Vous pouvez rouvrir une route plus tard et tout changer, la couper en deux, et relier l'extrémité d'une route au côté d'une autre avec des évasements arrondis.

Il s'appuie sur l'[outil courbe](curve-tool.md) : la même spline lisse, la même façon de mémoriser la route dans les clés `hammerpro_curve` des brushes (Hammer et Hammer++ les ignorent et les conservent), les mêmes joints en onglet sans trou ni chevauchement.

## L'ouvrir

- **Outils > Outil route** ou la palette d'outils (groupe création, après l'outil sprinkle). Il n'a pas de raccourci par défaut ; vous pouvez lui en donner un dans **Paramètres > Clavier**.
- **Outils > Modifier la route** rouvre la route du brush sélectionné (voir plus bas). **Outils > Modifier la courbe** fait la même chose quand le brush sélectionné appartient à une route.
- La palette de commandes (`Ctrl+P`) propose les deux commandes.

## Dessiner la route

1. Cliquez des points l'un après l'autre dans la **vue de dessus** ou dans la **vue 3D**. Dans la vue de dessus le point s'aimante à la grille (maintenez `Alt` pour ne pas aimanter) et prend la hauteur du point précédent, ou la hauteur du sol quand **Suivre le terrain** est activé. Dans la vue 3D le point est posé sur la surface sous le curseur, ou sur le plan du point précédent quand il n'y a pas de surface.
2. L'aperçu est en direct : les bords des voies et la ligne centrale dans les vues 2D, les brushes eux-mêmes dans la vue 3D. Le nombre de brushes et la longueur s'affichent près du dernier point et dans la barre d'options. En rouge, la raison quand la route ne peut pas être construite.
3. **Glissez un point** pour le déplacer. Dans la vue de dessus il se déplace en x et y, dans la vue 3D en x et y à sa propre hauteur. Dans les vues de **face** et de **côté** il se déplace sur son axe horizontal et en hauteur : c'est ainsi qu'on change la hauteur à la main.
4. **Cliquez un point** pour le sélectionner : son numéro et sa hauteur apparaissent dans la barre d'options, où vous pouvez taper la hauteur. `Ctrl+clic` sur un point le retire, `Maj+clic` dans la vue de dessus insère un point sur la route entre les deux points les plus proches (sa hauteur suit la route), `Retour arrière` retire le dernier point, `Suppr` retire le point sélectionné, **Inverser** retourne la route pour ajouter des points à l'autre bout. Une route compte au plus 512 points.
5. `Entrée`, un double clic ou le bouton **Créer** construit la route. `Échap` annule. Les nouveaux brushes sont sélectionnés et toute la route forme une seule étape d'annulation.

Les hauteurs sont lissées : entre les points la route monte et descend le long d'une courbe qui passe par la hauteur de chaque point sans la dépasser (une colline entre deux parties plates n'a jamais de bosse plus haute que ses points). Les pentes sont mesurées sur la route elle-même, la texture n'est donc pas étirée dans une pente.

## Réglages de la route

La barre d'options contient les voies, la largeur de voie, l'accotement, **Suivre le terrain**, le matériau de la nouvelle route, **Créer en tant que** (brush du monde, `func_detail` ou toute classe d'entité brush : tous les brushes vont dans une seule entité), l'épingle et les boutons. **Réglages de la route** ouvre la fenêtre avec tout le reste.

| Réglage | Effet | Plage | Défaut |
|---|---|---|---|
| **Voies** | Nombre de voies. | 1 à 8 | 2 |
| **Largeur de voie** | Largeur d'une voie. | 16 à 256 unités | 128 |
| **Accotement** | Largeur de l'accotement de chaque côté des voies, 0 pour aucun. | 0, ou 1 à 256 | 16 |
| **Largeur totale** | Voies plus les deux accotements (information, 2048 au plus). | | 288 |
| **Épaisseur de dalle** | Profondeur des brushes sous la surface de la route. | 2 à 128 | 8 |
| **Longueur de segment** | Longueur visée d'un segment. Des segments plus courts suivent mieux virages, pentes et terrain mais font plus de brushes (8000 au plus pour une route). | 16 à 512 | 64 |
| **Échelle de texture** | Échelle de texture des faces. | 0,01 à 64 | celle de l'éditeur |
| **Matériau de la route** | Matériau des voies. Le bouton **Courant** prend le matériau du navigateur de textures. | | le matériau courant |
| **Matériau d'accotement** | Matériau des accotements ; le bouton `x` le rend identique à celui de la route. | | celui de la route |
| **Matériau des lignes** | Matériau des lignes et des bandes. | | `dev/reflectivity_90` |
| **Matériau de la ligne centrale** | Matériau de la ligne centrale ; `x` le rend identique à celui des autres lignes. | | celui des lignes |
| **Ligne centrale** | La ligne du milieu (routes avec un nombre pair de voies). | aucune, continue, pointillée, double | pointillée |
| **Lignes de rive** | Lignes sur le bord extérieur des voies. | idem | continue |
| **Lignes de voie** | Lignes entre les autres voies. | idem | pointillée |
| **Largeur des lignes** | Largeur d'une ligne. Une ligne double est faite de deux lignes séparées par un espace de même largeur. | 1 à 32 | 6 |
| **Longueur de tiret**, **Longueur d'espace** | Longueur d'un tiret et de l'espace des lignes pointillées. | 4 à 1024 | 96, 96 |
| **Roulis début**, **Roulis fin** | Dévers en degrés au début et à la fin, variant régulièrement le long de la route. Un angle positif relève le côté gauche quand on suit la route. | -60 à 60 | 0 |
| **Dévers auto**, **Limite auto** | Dévers proportionnel à la courbure, et sa valeur maximale, comme dans l'[outil courbe](curve-tool.md). | 0 à 90, 0 à 45 | 0, 8 |
| **Suivre le terrain** | Voir plus bas. | oui/non | non |
| **Décalage au-dessus du sol** | Hauteur de la route au-dessus du sol quand elle suit le terrain. | -256 à 256 | 1 |
| **Hauteur de départ** | Hauteur du premier point quand il est posé dans une vue 2D et qu'il n'y a pas de sol dessous. | -16384 à 16384 | 0 |

Les lignes sont de fins brushes convexes dans la surface de la route, ni des overlays ni des brushes surélevés : elles ne scintillent donc jamais contre la route. Une ligne pointillée est un brush par segment qui a le matériau de la ligne pour le tiret et le matériau de la route pour l'espace. Les tirets suivent les segments : leur longueur est arrondie à un nombre entier de segments (avec des segments de 64 unités et des tirets de 96 unités, un tiret fait 2 segments). Les lignes doivent tenir dans la largeur de voie : une voie au moins aussi large que quelques lignes.

## Suivre le terrain

Avec **Suivre le terrain**, les nouveaux points prennent la hauteur du sol sous eux (la surface de brush ou de displacement la plus haute trouvée sous le curseur, en partant de 1024 unités au-dessus de la hauteur précédente, sinon du haut de la carte). La route plaque aussi chaque segment sur le sol à son endroit, relevée du **décalage au-dessus du sol**, et les hauteurs sont lissées pour que la route ne copie pas chaque bosse. Le sol, ce sont tous les brushes et displacements sauf les brushes de la route elle-même : modifier une route ne la fait donc jamais grimper sur elle-même. À chaque reconstruction de la route (modification, application) elle suit de nouveau le sol. Option désactivée, les hauteurs des points sont celles que vous avez réglées.

## Alignement des textures

Sur la face supérieure de chaque brush la texture suit la route : l'axe U suit la direction de la route (dans la pente, donc sans étirement sur une colline) et l'axe V va en travers. La position U est la distance le long de la route : les segments voisins se prolongent exactement sur la ligne centrale, sans saut aux joints, et les deux moitiés d'une route coupée gardent la même suite. Dans un virage, le bord intérieur est un peu plus court que le bord extérieur : la texture suit la ligne centrale, et l'écart reste faible pour les longueurs de segment usuelles. Les côtés et le dessous gardent l'alignement habituel sur le monde.

## Modifier une route existante

Chaque brush créé par l'outil route se souvient de sa route dans les clés `hammerpro_curve` du VMF (`r1;...`, points avec leurs hauteurs, tous les réglages, la jonction).

1. Sélectionnez un brush de la route (ou son entité brush).
2. **Outils > Modifier la route**. L'outil s'ouvre sur les points mémorisés avec les réglages mémorisés, et la fenêtre Route s'ouvre.
3. Glissez, ajoutez, insérez, retirez des points, changez des hauteurs, n'importe quel réglage, matériau, ligne, dévers, jonction.
4. `Entrée` ou **Appliquer** remplace tous les brushes de la route par les nouveaux en une seule étape d'annulation. Ils restent dans la même entité brush, s'il y en a une, et sont sélectionnés. `Échap` annule sans rien changer.

Les réglages de l'outil dans la barre d'options sont remis en place après la modification. Les copies d'une route entière continuent de fonctionner. Un brush que vous avez coupé ou déplacé à la main garde les données, mais **Modifier la route** remet la forme de la route.

## Couper une route

Dans la fenêtre de modification, sélectionnez un point intérieur et appuyez sur **Couper au point sélectionné**. La route devient deux routes qui se rejoignent à ce point, en une seule étape d'annulation. Les faces de coupe sont dans le même plan (les deux moitiés gardent la direction, la pente et le dévers qu'elles avaient à ce point), la suite de la texture continue, et la première partie garde le numéro de route (uid) tandis que la seconde en reçoit un nouveau. Avec l'outil MCP `split_road` vous pouvez aussi couper à une distance le long de la route ou à une position ; un point est alors ajouté à cet endroit de la route. Les routes reliées à la route coupée restent reliées à la partie qu'elles touchent.

## Jonctions

Une jonction relie l'extrémité d'une route au **côté** d'une autre route, comme une rue qui rejoint une route principale.

1. Modifiez la route dont l'extrémité doit rejoindre (la branche).
2. Dans la fenêtre Route, sous **Jonction**, choisissez l'autre route (listée « Route N (brush M) »), quelle extrémité de la branche est reliée (début ou fin de cette route) le choix **Attacher à** (bord extérieur avec accotement, ou bord de voie) et la longueur d'**Évasement**.
3. **Relier**, puis **Appliquer**.

Ce qui est généré : l'extrémité de la branche est amenée sur le bord extérieur de l'autre route (accotement compris). Avec **Attacher à : Bord de voie**, l'accotement de l'autre route est coupé sur la largeur de la branche et la surface de la branche le traverse jusqu'aux voies, sans aucun chevauchement. Dans les deux cas l'extrémité de la branche garde sa direction et est coupée le long de ce bord : les deux routes se touchent sans trou ni chevauchement, même en biais. Un évasement arrondi, fait de brushes convexes triangulaires, comble chacun des deux coins entre la branche et le côté de l'autre route : c'est un quart de rond lisse de la longueur choisie qui suit le vrai bord de l'autre route, avec la surface à la bonne hauteur, et il appartient à la branche. Un évasement de 0 donne un simple T. Les évasements sont omis sur un coin trop aigu (moins de 20 degrés environ).

La jonction est mémorisée. Quand vous modifiez ou coupez l'autre route, les routes qui lui sont reliées sont reconstruites dans la même étape d'annulation pour suivre son nouveau côté ; une route reliée qui ne peut plus l'atteindre (par exemple si les routes sont devenues parallèles) reste telle quelle et le journal indique combien. **Détacher** retire la jonction. Refusés : une route reliée à elle-même, deux routes reliées l'une à l'autre, des routes presque parallèles, une extrémité à plus de 2048 unités de l'autre route, et un angle trop rasant.

## Quand la route est refusée

- **Segments repliés dans un virage serré** (rouge) : la demi-largeur de la route est plus grande que le rayon du virage. Réduisez la largeur de voie ou l'accotement, ou ouvrez le virage.
- **Lignes trop larges** : les lignes ne tiennent pas dans les voies. Réduisez la largeur des lignes ou élargissez les voies.
- **Trop courte**, **trop de brushes**, **plus large que 2048 unités**, **hors de la carte**, et les problèmes de jonction ci-dessus.

## MCP

| Outil | Effet |
|---|---|
| `create_road` | Construit une route à partir de `points` ([x, y] ou [x, y, z]) et des réglages ci-dessus (`lanes`, `lane_width`, `shoulder`, `thickness`, `step`, `material`, `center_line`, `edge_line`, `divider_line`, `line_width`, `dash`, `gap`, `roll_start`, `roll_end`, `follow_terrain`, `lift`, `entity_class`...). Répond `road_uid` et les identifiants des brushes. |
| `get_road` | Lit une route (par `solid_id` d'un de ses brushes, `road_uid` ou la sélection) : points avec hauteurs, réglages, jonction, identifiants des brushes, longueur, limites, et au besoin les `samples` de la ligne centrale. |
| `list_roads` | Liste toutes les routes de la carte. |
| `edit_road` | Reconstruit une route en ne changeant que les champs donnés, en une seule étape d'annulation. Ajouter, retirer, déplacer des points ou changer des hauteurs se fait en envoyant la nouvelle liste `points`. |
| `delete_road` | Supprime tous les brushes d'une route en une seule étape d'annulation. |
| `split_road` | Coupe une route en deux avec `point`, `at` ou `distance`. |
| `add_road_junction` | Relie une extrémité de route (`end`) au côté de `target_uid` avec un `flare` et un `attach` (`outer` ou `lane`), ou retire la jonction avec `remove`. |

`set_tool_options` avec l'outil `road` règle les mêmes paramètres pour la prochaine route dessinée avec l'outil (`lanes`, `lane_width`, `shoulder`, `thickness`, `step`, `tex_scale`, `material`, `shoulder_material`, `line_material`, `center_material`, `center_line`, `edge_line`, `divider_line`, `line_width`, `dash`, `gap`, `roll_start`, `roll_end`, `auto_bank`, `auto_bank_limit`, `follow_terrain`, `lift`, `base_height`, `create_as`, `keep_active`). Voir la [référence des outils MCP](../../MCP.md).

## Conseils

- Dessinez la route avec peu de points et laissez la spline arrondir ; ajoutez un point là où la route doit passer exactement.
- Pour une route sur des collines, activez **Suivre le terrain** avant le premier clic, dessinez dans la vue de dessus, puis affinez la hauteur de certains points dans la vue de face ou de côté.
- Mettez les longues routes dans un `func_detail` avec **Créer en tant que** : la route ne coupe alors pas la visibilité de la carte. Gardez une longueur de segment de 64 ou plus pour les longues routes afin de limiter le nombre de brushes et le temps de compilation.
- Une route compile comme de simples brushes convexes ; avec du dévers le dessus d'un segment peut être légèrement gauchi, ce que le compilateur découpe en quelques faces de plus.

## Pages liées

- [Outil courbe](curve-tool.md)
- [Détails de route](road-details.md)
- [Outil bloc](block-tool.md)
- [Outil displacement](displacement-tool.md)
- [Grilles](grids.md)
- [Raccourcis clavier](keyboard-shortcuts.md)

---
[Retour à l'index de la documentation](../README.md)
