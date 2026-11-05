[Documentation](../README.md) > Fonctionnalités > Outil tranche | [English](../../en/features/slice-tool.md) | [Русский](../../ru/features/slice-tool.md)

# Outil tranche

L'outil tranche coupe les brushes sélectionnés en plusieurs blocs avec des plans parallèles régulièrement espacés, directement depuis la vue 3D. Bougez la souris pour placer la coupe, tournez la molette pour choisir le nombre de coupes, cliquez pour appliquer. Il fonctionne aussi dans les vues 2D.

## L'ouvrir

- **Outils > Outil tranche**, la palette d'outils, ou `Maj+Y`.
- Sélectionnez d'abord les brushes. L'outil travaille sur un ou plusieurs brushes sélectionnés à la fois, y compris ceux d'un groupe ou d'une entité brush sélectionnés.
- Resélectionner l'outil (`Maj+Y` quand il est actif) alterne l'orientation.

Il n'y a ni panneau ni dialogue : la barre d'options et la barre d'état affichent une seule ligne d'aide avec l'orientation, le nombre de segments, la position, le nombre de coupes et de blocs, et les touches.

## Orientation : de gauche à droite et de haut en bas

| Orientation | Les plans coupent selon | Résultat |
|---|---|---|
| **De gauche à droite** (défaut) | l'axe horizontal de la grille le plus proche de la droite de l'écran | l'objet est découpé en blocs côte à côte, de gauche à droite |
| **De haut en bas** | l'axe vertical de la grille (celui le plus proche du Z du monde) | l'objet est découpé en couches empilées, de haut en bas |

`Tab` ou `O` alterne entre les deux. L'orientation est relative à la vue où se trouve la souris :

- Dans la vue 3D, « de gauche à droite » suit la caméra : l'axe horizontal de la grille (X ou Y) le plus aligné avec la droite de l'écran. Tournez la caméra et l'axe suit.
- Dans une vue 2D, « de gauche à droite » est l'axe horizontal de cette vue et « de haut en bas » son axe vertical (vue de dessus : X et Y ; vues de face et de côté : Y ou X, et Z).
- Sur une grille tournée ou personnalisée, les plans suivent les axes de la grille active (voir [Grilles](grids.md)), pas les axes du monde.

## Position, segments et blocs

- **Position.** La souris donne la position de la coupe le long de l'axe. Elle s'aimante au pas de la grille active sur cet axe (l'origine de la grille compte, donc une grille décalée est respectée) ; maintenez `Alt` pour désactiver l'aimantation. La position ne sort jamais des limites de la sélection : avec plusieurs brushes, ce sont les limites de toute la sélection.
- **Segments.** La molette change le nombre de coupes, de 1 à 64 (jamais moins de 1). N coupes font N+1 blocs. Dans une vue 2D la molette seule continue de zoomer : utilisez `Maj` + molette.
- **Espacement des plans.** Le premier plan est à la position. La distance entre la face la plus proche de la sélection et la position est le pas, et les plans répètent ce pas vers l'autre face. Avec la souris à 16 unités du bord gauche et 3 segments, les coupes sont à 16, 32 et 48 unités de ce bord. Un plan qui tomberait sur l'autre face ou au-delà, ou qui laisserait un morceau de moins de 0,25 unité, est abandonné, et la ligne d'aide montre le vrai nombre de coupes.
- **Centre.** Au départ, et après un clic droit, la position est le centre de l'objet et les coupes le divisent en N+1 blocs égaux (un segment le coupe en deux). La ligne d'aide affiche **(centre)**. La position reste au centre jusqu'à ce que la souris bouge.

L'aperçu dessine chaque plan comme un quad jaune avec sa coordonnée, dans un léger contour des limites de la sélection, dans les vues 3D et 2D.

## Souris et touches

| Entrée | Effet |
|---|---|
| Bouger la souris | Placer la coupe (aimantée à la grille). |
| Molette (3D) / `Maj` + molette (2D) | Plus ou moins de segments (1 à 64). |
| Clic gauche | Appliquer les coupes. |
| Clic droit | Remettre la position au centre de l'objet. Un glissement droit permet toujours de regarder autour. |
| `Tab`, `O`, `Maj+Y` | Alterner de gauche à droite / de haut en bas. |
| `Entrée` | Appliquer les coupes (comme le clic gauche). |
| `Échap` | Remettre la position au centre. |
| `Alt` (maintenu) | Pas d'aimantation. |

## Ce qui arrive aux brushes

- Chaque coupe donne des brushes convexes valides. Les morceaux gardent les textures, l'alignement et l'échelle de lightmap d'origine du brush ; les nouvelles faces de coupe copient la face la plus semblable du brush, ou prennent le matériau actuel quand **Matériau actuel sur les coupes** est activé (le réglage `brush.clip_current_material` de l'outil découpe).
- Toute la tranche, tous les plans de tous les brushes, est **une seule étape d'annulation** (**Tranche**). Les nouveaux morceaux sont sélectionnés comme les brushes d'origine : vous pouvez les trancher à nouveau, dans l'autre orientation par exemple, pour couper un mur en briques.
- Les solides d'entité brush gardent leur entité. Les brushes que les plans ne traversent pas sont laissés tels quels. Quand rien ne peut être coupé, vous obtenez **Aucune coupe dans la sélection**.

## MCP

L'outil `slice_brushes` fait la même chose sans l'outil : `ids` ou `solid_ids` (par défaut la sélection), `orientation` (`left_right` ou `top_bottom`) avec `view` (`perspective`, `top`, `front`, `side`), ou `axis` (`x`, `y`, `z` de la grille active, `world_axes` pour les axes du monde), `position` (la distance du plan le long de l'axe, par défaut le centre), `segments` (1 à 64), `snap`, `cap_material` et `dry_run`, qui répond avec les plans sans modifier la carte. `set_tool_options` avec l'outil `slice` règle `orientation` et `segments`, et `run_action` avec `tool_slice` active l'outil. Voir la [référence des outils MCP](../../MCP.md).

## Astuces

- Pour couper un long mur en dalles d'une largeur de grille : placez la souris à une largeur de grille du bout, puis tournez la molette jusqu'à ce que la dernière coupe atteigne l'autre bout.
- Pour diviser un sol en quatre carreaux égaux : sélectionnez-le, un segment de gauche à droite, appliquez, puis un segment de haut en bas (ou l'autre axe en vue de dessus), appliquez à nouveau.
- Pour tailler des couches dans un pilier, utilisez de haut en bas avec plusieurs segments.

## FAQ

**La molette zoome au lieu de changer les segments.** Dans les vues 2D, utilisez `Maj` + molette. Dans la vue 3D, la molette appartient à l'outil tant qu'il est actif.

**Rien ne se passe quand je clique.** Sélectionnez d'abord des brushes et gardez la souris à l'intérieur de l'objet : une position sur le bord ne donne aucune coupe.

**Tab déplace le focus du clavier.** `Tab` n'alterne que si aucun champ de texte ni bouton n'avait le focus. Cliquez d'abord dans une vue, ou appuyez sur `O`.

## Pages liées

- [Outil de découpe](clip-tool.md)
- [Opérations sur les brushes](brush-operations.md)
- [Grilles](grids.md)
- [Raccourcis clavier](keyboard-shortcuts.md)

---
[Retour à l'index de la documentation](../README.md)
