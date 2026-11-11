[Documentation](../README.md) > Fonctionnalités > Outil polygone | [English](../../en/features/polygon-tool.md) | [Русский](../../ru/features/polygon-tool.md)

# Outil polygone

L'outil polygone (comme celui de Hammer++) crée un brush à partir d'un contour convexe : vous cliquez ses points dans une vue 2D et le contour est extrudé selon la profondeur de cette vue.

## Ouvrir l'outil

- **Outils > Outil actif > Outil polygone**, la palette d'outils, ou `Maj+N`.
- L'outil ne fonctionne que dans les vues 2D ; les clics dans la vue 3D sont ignorés.

## Créer un brush

1. Cliquez dans une vue 2D pour poser le premier point, puis les suivants. Les points s'alignent sur la grille (maintenez `Alt` pour les poser sans magnétisme).
2. Pendant que vous bougez la souris, la longueur de l'arête en cours s'affiche près du curseur.
3. Un point qui rendrait le contour concave est refusé : le repère devient rouge avec **Concave : point refusé**. Un point déjà présent dans le contour est ignoré. Un contour compte au plus 128 points (**Trop de points**).
4. Dès que le contour a 3 points ou plus, appuyez sur `Entrée`, double-cliquez ou cliquez sur **Créer** pour construire le brush. La barre d'état indique combien de points manquent avant cela.

`Retour arrière` (ou le bouton **Retirer le point**) retire le dernier point. `Échap` annule le contour.

## Modifier le contour avant la création

- **Déplacer un point** : glissez-le dans la vue où le contour est tracé. Un déplacement qui rendrait le contour concave ou dégénéré est refusé.
- **Profondeur** : la plage d'extrusion le long de l'axe de la vue vient, comme pour l'outil bloc, de l'encombrement de la sélection, sinon du dernier objet créé par l'outil bloc ou polygone, sinon de 64 unités. Dans les autres vues 2D, le contour apparaît comme un rectangle : glissez une de ses extrémités pour changer la profondeur, ou son intérieur pour déplacer toute la plage. Les changements de profondeur s'alignent sur la grille.
- Le contour appartient au type de vue où il a été commencé (dessus, face ou côté). Les clics dans les autres vues 2D ne font que modifier la profondeur ; appuyez sur `Échap` pour recommencer dans une autre vue.
- La vue 3D montre le prisme et sa taille.

## Options

| Option | Rôle | Par défaut |
|---|---|---|
| **Échelle** | Échelle de texture des nouvelles faces (unités par texel, de 0,01 à 64). | paramètre **Échelle de texture par défaut** (0,25) |
| Créer comme | **Brush du monde**, **func_detail** ou n'importe quelle classe d'entité brush du FGD (liste avec recherche, un nom saisi est accepté). Avec une classe, le brush devient une nouvelle entité brush avec les keyvalues par défaut du FGD. | Brush du monde |
| Matériau actuel | Matériau posé sur toutes les faces (le matériau par défaut si aucun n'est choisi). Cliquez dessus pour afficher le navigateur de textures. | `dev/dev_measuregeneric01b` |
| Punaise (garder actif) | Garder l'outil polygone actif après la création d'un brush. Désactivée, l'outil de sélection revient. | activée |
| **Retirer le point** | Retirer le dernier point (`Retour arrière`). | |
| **Créer** | Créer le brush (`Entrée` ou double clic). | |
| Nombre de points | **N point(s)** : nombre de points du contour. | |

Le nouveau brush devient la sélection ; la création est une seule étape d'annulation.

## Astuces

- L'outil n'accepte que des contours convexes. Pour une forme concave, tracez plusieurs brushes convexes côte à côte.
- Sélectionnez d'abord le sol : le contour reprend la plage de hauteur de la sélection, donc les murs et piliers tracés en vue de dessus ont tout de suite la bonne hauteur.
- Après la création, passez à l'[Outil vertex](vertex-tool.md) si vous devez incliner ou tordre le brush.
- L'outil MCP `create_polygon_brush` construit le même type de brush à partir d'une liste de points (voir la [référence des outils MCP](../../MCP.md)).
- `set_tool_options` avec l'outil `polygon` règle les options de l'outil lui-même : `tex_scale`, `create_as` (`world` ou une classe d'entité brush) et `keep_active`.

## FAQ

**Pourquoi mon point est refusé ?** Il rendrait le contour concave (ou c'est le même point qu'un autre). Posez les points dans l'ordre autour de la forme, en tournant toujours dans le même sens.

**Peut-on tracer dans la vue 3D ?** Non. Utilisez l'outil bloc pour tracer sur des surfaces en 3D.

**Comment changer l'épaisseur après avoir tracé le contour ?** Glissez les extrémités du rectangle du contour dans l'une des deux autres vues 2D avant de créer le brush.

## Pages liées

- [Outil bloc](block-tool.md)
- [Outil vertex](vertex-tool.md)
- [Outil de découpe](clip-tool.md)
- [Grilles](grids.md)
- [Raccourcis clavier](keyboard-shortcuts.md)

---
[Retour à l'index de la documentation](../README.md)
