[Documentation](../README.md) > Fonctionnalités > Outil biseau | [English](../../en/features/bevel-tool.md) | [Русский](../../ru/features/bevel-tool.md)

# Outil biseau

L'outil biseau arrondit ou chanfreine les arêtes des brushes directement dans la vue 3D, ce que Hammer++ ne sait pas faire. Pointez une arête, appuyez sur `Ctrl+B`, bougez la souris pour régler la taille et tournez la molette pour régler la finesse de l'arrondi. Appuyez sur `B` pendant l'opération pour le biseau inversé (concave) : une gorge creusée dans l'arête, ou un arrondi concave qui remplit un coin intérieur, ce qui transforme le haut d'une porte en arche.

Chaque résultat est fait de brushes convexes valides, la map se compile comme n'importe quelle autre.

## L'ouvrir

- **Outils > Outil actif > Outil biseau**, la barre d'outils (icône de cercle avec un quart plein, dans le groupe d'édition après l'outil extruder / creuser), ou `Maj+J`.
- Depuis l'outil **Sélection** ou **Vertex** : pointez une arête de brush dans la vue 3D et appuyez sur `Ctrl+B`. L'outil biseau s'ouvre et commence tout de suite le biseau de cette arête (voir [Ctrl+B](#ctrlb-et-lalignement-sur-la-grille) plus bas).

## Biseauter une arête

1. Passez la souris sur un brush dans la vue 3D. L'arête sous le curseur est surlignée en jaune, avec les deux faces qui l'entourent.
2. Appuyez sur `Ctrl+B`. Le biseau commence avec le dernier rayon utilisé (16 unités la première fois), limité par ce que permettent les faces.
3. Montez la souris pour agrandir le biseau, descendez-la pour le réduire. La taille suit d'elle-même la grille active (taille, grille personnalisée et rotation respectées, pas gardé entre 1 et 1024 unités, voir [Grille pour le biseau, le décalage et l'extrusion](extrude-tool.md#grille-pour-le-biseau-le-décalage-et-lextrusion)) ; maintenez `Ctrl` pour forcer la grille et `Alt` pour une taille libre. Vous pouvez aussi la taper dans le champ **Rayon** de la barre d'options.
4. Tournez la **molette** vers le haut pour plus de segments (un arrondi plus lisse) et vers le bas pour moins. Un seul segment donne un chanfrein plat. Le champ **Segments** fait la même chose.
5. Appuyez sur `B` (ou de nouveau `Ctrl+B`) pour passer du biseau normal au biseau inversé.
6. Appuyez sur `Entrée` ou cliquez pour appliquer. `Échap` ou un clic droit annule.

Pendant le biseau, les vues montrent le résultat en direct : les arêtes des brushes obtenus en vert, les nouvelles faces arrondies en jaune dans la vue 3D, et une étiquette près de l'arête avec le type (arrondi, chanfrein, gorge ou remplissage intérieur), le rayon et le nombre de segments. L'étiquette passe en rouge avec la raison quand le biseau est impossible.

Toute l'opération est une seule étape d'annulation (**Biseautage d'arêtes**). Les brushes obtenus sont sélectionnés.

## Plusieurs arêtes à la fois

- Cliquez une arête pour la choisir, `Ctrl+clic` pour en ajouter ou en retirer. Les arêtes choisies sont dessinées en bleu.
- Appuyez sur `Ctrl+B` ou `Entrée` (ou cliquez sur **Biseauter** dans la barre d'options) pour biseauter toutes les arêtes choisies avec le même rayon et les mêmes segments.
- Un double clic sur une arête commence tout de suite son biseau.
- Cliquez loin de toute arête, ou appuyez sur `Échap`, pour vider les arêtes choisies.

- `Alt`+clic sur une arête choisit toute sa **boucle d'arêtes** (`Ctrl+Alt`+clic : son anneau d'arêtes, `Maj` ajoute aux arêtes choisies au lieu de les remplacer). La boucle se calcule comme dans l'[outil vertex](vertex-tool.md#boucles-et-anneaux-darêtes), à travers les brushes sélectionnés et les brushes qui les touchent (jusqu'à 256), et les arêtes sans coin à biseauter sont écartées. Biseautez-la avec `Ctrl+B`, `Entrée` ou le bouton **Biseauter** : les quatre arêtes autour d'une face de boîte sont biseautées d'un coup, le contour d'un mur fait de plusieurs brushes l'est brush par brush, en une seule étape d'annulation.

Les arêtes de plusieurs brushes peuvent être biseautées ensemble. Quand deux arêtes choisies sont de part et d'autre de la même face (les deux arêtes du haut d'un mur), chaque rayon est limité à la moitié de la face pour que les arrondis ne se croisent jamais.

## Vues 2D

Dans une vue 2D, une arête vue en bout (un point, comme le coin d'un mur dans la vue de dessus, ou l'arête d'une poutre vue de face) se pointe, se choisit et se biseaute de la même façon ; monter et descendre la souris change le rayon d'une unité par pixel au zoom 1. Les brushes sélectionnés sont cherchés en premier : sélectionnez le brush quand plusieurs arêtes sont au même endroit. L'aperçu s'affiche dans toutes les vues.

## Le biseau normal

Le biseau normal arrondit l'arête vers l'extérieur. Le brush est coupé par un plan par segment et reste **un seul brush convexe** : une boîte avec une arête arrondie en 4 segments a 10 faces. Avec 1 segment c'est un chanfrein plat.

## Le biseau inversé

Le biseau inversé est concave. Ce qu'il fait dépend de l'arête :

| Arête | Résultat |
|---|---|
| Une arête extérieure libre | Une **gorge** est creusée dans l'arête : un quart de rond concave, comme l'intérieur d'une arche ou une moulure en gorge. Le brush est découpé en morceaux convexes qui suivent la courbe (un par segment plus le reste du brush, refusionnés quand c'est possible). |
| Une arête où le brush rejoint un autre brush dans un coin intérieur (le coin du haut d'une porte, le bas d'un mur posé sur un sol) | Le coin intérieur est **rempli** d'un arrondi concave fait de nouveaux brushes, ajoutés au même propriétaire (monde ou entité de brush) que le brush biseauté. Les deux brushes ne changent pas. |

### Faire une arche

1. Faites un mur avec une porte rectangulaire : trois brushes (deux montants et le linteau au-dessus de l'ouverture), par exemple avec l'[outil extruder / creuser](extrude-tool.md) et **Traversant**.
2. Pointez un coin du haut de l'ouverture dans la vue 3D (l'arête où le linteau rejoint un montant), appuyez sur `Ctrl+B`, puis sur `B` pour le biseau inversé.
3. Montez la souris : le rayon s'arrête à ce que permet l'ouverture. Tournez la molette pour 8 à 12 segments et appuyez sur `Entrée`.
4. Faites de même pour l'autre coin du haut. Avec un rayon égal à la moitié de la largeur de l'ouverture, les deux arrondis se rejoignent au milieu et forment une arche en plein cintre.

Pour faire les deux coins en une étape, choisissez d'abord les deux arêtes avec `Ctrl+clic`.

## Textures

- Les faces gardées du brush gardent leur matériau et l'alignement de leur texture.
- Les faces arrondies prennent la texture d'une des deux faces autour de l'arête, **enroulée** sur tout l'arrondi : elle continue depuis cette face sur chaque segment sans raccord, la courbe se lit comme une seule surface. Le seul raccord est à l'autre ligne de tangence, où l'arrondi rejoint l'autre face. Avec **Nouvelles faces : Matériau courant**, elles prennent le matériau courant (toujours enroulé).
- Les faces arrondies d'un biseau d'au moins 2 segments reçoivent le groupe de lissage 1, l'éclairage compilé est donc lisse sur l'arrondi.
- L'arrondi d'un remplissage de coin prend la texture de la face du brush biseauté qui touche le coin, enroulée sur tout l'arrondi (une plinthe ou une arche dans le matériau du brush pointé). Les bouts du remplissage sont sur les faces du mur et prennent la texture et l'alignement de ces faces.

## Limites

- Le rayon est limité par ce que permettent les faces autour de l'arête : un arrondi ne peut pas dépasser le bout d'une face. La barre d'options affiche **Rayon maximal** pendant le biseau.
- Les segments de moins d'une demi-unité sont fusionnés (un tout petit rayon avec 32 segments en donne moins) ; l'étiquette affiche le nombre réellement utilisé.
- Les brushes avec un displacement ne peuvent pas être biseautés.
- Une arête entre deux faces à plat l'une avec l'autre n'a pas de coin à biseauter.

## Options

| Option | Effet | Par défaut |
|---|---|---|
| **Rayon** | Rayon de l'arrondi en unités (0,25 à 16384, limité par les faces). Bouger la souris pendant le biseau le règle. | 16 |
| **Segments** | Faces de l'arrondi, 1 (chanfrein) à 32. La molette pendant le biseau le règle. | 4 |
| **Inversé (concave)** | Gorge ou remplissage de coin intérieur au lieu d'un arrondi (`B` pendant le biseau). | non |
| **Nouvelles faces** | **Faces de l'arête** (les textures autour de l'arête, enroulées) ou **Matériau courant**. | Faces de l'arête |
| **Biseauter** | Commencer le biseau des arêtes choisies. | |
| **Appliquer** / **Annuler** | Appliquer (`Entrée` ou clic) ou annuler (`Échap` ou clic droit) le biseau en cours. | |
| **Vider les arêtes** | Oublier les arêtes choisies. | |

## Ctrl+B et l'alignement sur la grille

`Ctrl+B` est aussi **Aligner la sélection sur la grille** dans tous les jeux de raccourcis (Hammer Pro, Hammer et Hammer++). Les deux se partagent la touche ainsi :

- Dans l'outil **Biseau**, `Ctrl+B` commence le biseau quand une arête est sous le curseur ou que des arêtes sont choisies, et passe au biseau inversé pendant l'opération. Sinon il aligne la sélection sur la grille comme d'habitude.
- Dans les outils **Sélection** et **Vertex**, `Ctrl+B` biseaute les arêtes choisies avec **Arête** dans l'outil de sélection ([Sélectionner des faces, des arêtes ou des sommets](selection-tool.md#sélectionner-des-faces-des-arêtes-ou-des-sommets-choisir)) s'il y en a, sinon l'arête de brush sous le curseur quand le curseur est sur une arête dans la **vue 3D**. Partout ailleurs (une vue 2D, le milieu d'une face, le vide) il aligne la sélection (ou les vertex sélectionnés) sur la grille comme avant.
- Dans tous les autres outils, `Ctrl+B` aligne seulement sur la grille.

Si vous changez la touche de **Aligner la sélection sur la grille** dans **Paramètres > Clavier**, le biseau suit la nouvelle touche.

## MCP

L'outil MCP `bevel_edges` fait la même chose depuis une application d'IA : `solid` avec `edge` (un numéro donné par `get_brush_edges` ou deux points `[[x, y, z], [x, y, z]]`) ou `edges` (plusieurs, aussi sous forme d'objets `{solid, edge}` ou `{solid, a, b}` sur plusieurs brushes), `radius`, `segments`, `inverse`, et la texture des faces arrondies (`new_faces` : `face` ou `current`, ou un `material`). `dry_run` donne le rayon et les segments qui seraient utilisés, le rayon maximal et le type de chaque arête (`round`, `cove` ou `fill`) sans modifier la map. `get_brush_edges` liste les arêtes d'un brush. Avec `expand` (`loop` ou `ring`), chaque arête donnée est remplacée par toute sa boucle ou son anneau d'arêtes, à travers les brushes qui la partagent (`select_edge_loop` et `select_edge_ring` répondent les mêmes arêtes). `get_tool_options` et `set_tool_options` lisent et changent les options de l'outil lui-même (outil `bevel` : `radius`, `segments`, `inverse`, `new_faces`). Voir la [référence des outils MCP](../../MCP.md).

## FAQ

**Un biseau peut-il créer des brushes invalides ?** Non. Un biseau normal garde un seul brush convexe ; un biseau inversé crée des morceaux convexes vérifiés un par un, et les morceaux qui peuvent être refusionnés le sont. Rien ne change quand le résultat ne serait pas valide.

**Pourquoi le rayon arrête-t-il de grandir ?** L'arrondi a atteint le bout d'une face autour de l'arête, ou la moitié d'une face partagée avec une autre arête choisie.

**Pourquoi le biseau inversé a-t-il ajouté des brushes au lieu de creuser ?** L'arête est un coin intérieur avec un autre brush : l'arrondi concave remplit le coin. Pour creuser une gorge, biseautez une arête qui n'est pas contre un autre brush.

**Puis-je biseauter un func_detail ou une entité de brush ?** Oui. Les morceaux et les remplissages restent dans la même entité.

## Pages liées

- [Outil extruder / creuser](extrude-tool.md)
- [Outil de découpe](clip-tool.md)
- [Outil vertex](vertex-tool.md)
- [Opérations sur les brushes](brush-operations.md)
- [Raccourcis clavier](keyboard-shortcuts.md)

---
[Retour à l'index de la documentation](../README.md)
