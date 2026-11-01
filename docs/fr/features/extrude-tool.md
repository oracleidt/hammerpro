[Documentation](../README.md) > Fonctionnalités > Outil extruder / creuser | [English](../../en/features/extrude-tool.md)

# Outil extruder / creuser

L'outil extruder / creuser découpe des formes plus facilement que l'outil de découpe : vous cliquez les points d'un contour sur une face de brush (une fenêtre, une porte, une niche, un logo...), puis vous l'enfoncez dans le brush pour creuser un renfoncement ou une ouverture, ou vous le tirez hors de la face pour l'extruder. Le contour peut avoir n'importe quelle forme, concave aussi : l'outil le découpe en morceaux convexes, si bien que chaque brush créé est un brush convexe valide.

## Ouvrir l'outil

- **Outils > Outil extruder / creuser**, la bande d'outils (icône pioche, dans le groupe d'édition à côté de l'outil de découpe), ou `Maj+I`.
- Appuyer de nouveau sur `Maj+I` quand l'outil est actif bascule entre **Creuser** et **Extruder**, comme `Tab`.

## Tracer le contour

1. Cliquez sur une face de brush, dans la vue 3D ou dans une vue 2D qui regarde la face (la vue de dessus pour un sol, la vue de face ou de côté pour un mur). La face sous la souris est entourée avant le clic. Ce clic pose le premier point et choisit la face ; les points suivants restent sur le plan de cette face.
2. Cliquez les points suivants autour de la forme. Les points s'alignent sur la grille dans le plan de la face (maintenez `Alt` pour les poser sans magnétisme). La longueur de l'arête en cours s'affiche près du curseur.
3. Un point qui ferait se croiser le contour est refusé : le repère devient rouge avec **Le contour se croise**. Un point déjà présent est ignoré. Un contour compte au plus 128 points.
4. Fermez le contour en cliquant de nouveau sur son premier point, ou en double-cliquant. Vous pouvez aussi appuyer sur `Entrée` dès 3 points : le contour est fermé et appliqué tout de suite avec la profondeur actuelle.

`Retour arrière` (ou **Retirer le point**) retire le dernier point, ou rouvre un contour fermé. `Échap` annule le contour.

Une vue 2D qui regarde la face par la tranche (un mur vu de dessus) ne peut pas y poser de points ; servez-vous-en pour régler la profondeur.

## Régler la profondeur

- Une fois le contour fermé, glissez n'importe où dans une vue (3D ou 2D) : vers l'intérieur du brush pour creuser, vers l'extérieur pour extruder. La profondeur suit la souris le long de la normale de la face et s'aligne sur la grille. En 2D, les vues qui montrent la face par la tranche sont les plus pratiques.
- Ou tapez la valeur dans le champ **Profondeur** de la barre d'options.
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

## MCP

L'outil MCP `extrude_face` fait la même chose depuis une application d'IA : `solid`, `side` (l'indice de la face dans le solide, donné par `get_faces`), les `points` du contour en positions du monde sur la face, `mode` (`carve` ou `extrude`), `depth`, `through`, et la texture des nouvelles faces (`new_faces` : `face` ou `current`, ou un `material`). `get_tool_options` et `set_tool_options` lisent et changent les options de l'outil lui-même (outil `extrude` : `mode`, `depth`, `through`, `new_faces`). Voir la [référence des outils MCP](../../MCP.md).

## FAQ

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
