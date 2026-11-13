[Documentation](../README.md) > Fonctionnalités > Outil de végétation | [English](../../en/features/foliage-tool.md) | [Русский](../../ru/features/foliage-tool.md)

# Outil de végétation

Peignez des arbres, buissons, rochers et autres props sur les déplacements, les brushes et les props avec un pinceau, et choisissez où poussent les detail props (l'herbe `%detailtype` de la compilation). L'outil est optionnel : rien ne se passe tant que vous ne le choisissez pas.

## L'ouvrir

- Le bouton **Végétation** de la barre d'outils (icône d'arbre, dans le groupe de création).
- **Outils > Placement et props > Outil de peinture de végétation**, action `tool_foliage`.
- MCP : `paint_foliage`, `erase_foliage`, `set_face_detail_type`, `preview_detail_props`, `list_detail_types`, et `set_tool_options` avec l'outil `foliage`.

Les options sont dans le panneau de l'outil sur le côté. Elles sont enregistrées avec les réglages (`placement.foliage`).

## Modes

| Mode | Glisser | Maj + glisser |
|---|---|---|
| Peindre | Peint des props de la palette | Efface les props du calque de la palette |
| Effacer | Efface les props du calque de la palette | Pareil |
| Alpha des détails | Monte l'alpha du déplacement sous le pinceau | Le baisse |
| Détails par face | Donne aux faces sous le curseur le type de détail choisi | Efface le type propre de la face |

Alt + molette change le rayon. Chaque trait (appui, glisser, relâcher) est une seule étape d'annulation. La peinture marche dans les vues 3D et dans la vue 2D de dessus (tracé vers le bas).

## Palettes

Une palette est une liste de modèles, chacun avec un poids : un modèle de poids 2 est tiré deux fois plus souvent qu'un modèle de poids 1. Chaque entrée a aussi une classe (`prop_static`, `prop_dynamic` ou `prop_physics`) et peut être désactivée par sa case.

- Glissez des modèles du navigateur de modèles sur la zone de dépôt du panneau, ou utilisez **Ajouter le modèle courant** (le modèle choisi dans le navigateur).
- Plusieurs palettes peuvent être gardées (+ et la corbeille) ; le champ du nom renomme la palette active.
- Chaque prop peint rejoint le visgroup **Foliage: <nom de la palette>**, créé au premier trait. Masquez, sélectionnez ou supprimez un calque entier depuis le panneau des visgroups.

## Options de peinture

- **Rayon** : rayon du pinceau en unités.
- **Densité** : props par 256 x 256 unités de surface. Une touche n'ajoute que ce qui manque dans son disque, repasser au même endroit garde la densité au lieu d'empiler des props.
- **Espacement** : distance minimale entre deux props du calque (disque de Poisson).
- **Échelle** : plage d'échelle aléatoire, écrite dans `modelscale`.
- **Lacet aléatoire**, **Skin aléatoire** (tire un des skins du modèle).
- **Alignement** : 0 garde les props droits, 1 les penche complètement sur la normale, entre les deux mélange.
- **Pente** : seulement sur les surfaces dont la pente (degrés depuis le plat) est dans la plage.
- **Limite de hauteur** : seulement entre deux hauteurs du monde.
- **Éviter les chevauchements** : l'empreinte réelle de chaque modèle, prise de son maillage (les formes du placement sur formes réelles), ne chevauche jamais un autre prop peint ou existant.
- **Poser sur la surface** : le maillage réel du modèle descend sur les surfaces réelles (déplacements, brushes, autres props) jusqu'au contact, le prop ne flotte pas et ne s'enfonce pas.
- Appui : avant de placer, le sol est sondé sous plusieurs points de la zone de contact du modèle (la partie la plus basse de son maillage réel). Un prop dont l'empreinte dépasse d'un bord, d'un trou ou d'une pente hors limites n'est pas placé.
- **Peindre sur les props** : les props des autres calques et de la carte reçoivent aussi des props (herbe sur rochers). Les props du calque de la palette elle-même jamais.
- **Enfoncer** : décalage vers le haut, négatif enfonce les props dans le sol.

Performances : le pinceau ne regarde que les props proches, par une grille spatiale construite une fois par trait, la peinture reste rapide avec des milliers de props dans la carte.

## Detail props : où pousse l'herbe

Les detail props viennent du `%detailtype` du matériau et des groupes de `detail.vbsp` (voir [Aperçu des detail props](detail-props-preview.md)). Deux modes les contrôlent :

- **Alpha des détails** peint l'alpha des sommets du déplacement. Sur les matériaux de mélange (WorldVertexTransition) l'alpha mélange les deux textures et choisit le groupe de détails par son `alpha`, exactement comme VBSP : peindre la couche d'herbe fait pousser l'herbe là. **Force** est l'alpha ajouté par touche.
- **Détails par face** donne aux faces leur propre type de détail : un nom de groupe de `detail.vbsp`, `none` pour laisser une face nue même si son matériau a un `%detailtype`, ou Maj + clic pour revenir au matériau. Le type est enregistré dans le VMF comme clé de face `hp_detailtype`. Le compilateur de Hammer Pro et l'aperçu la respectent ; Hammer et Hammer++ ouvrent le fichier mais ignorent la clé.

Activez l'aperçu (**Afficher l'aperçu des détails** dans le panneau, ou le bouton feuille de la barre d'outils) pour voir l'herbe suivre vos traits.

## MCP

- `paint_foliage` : points du trait, rayon, une palette (nom enregistré, ou `items` avec model, weight, classname) et toute option ci-dessus ; `preview` compte sans placer.
- `erase_foliage` : points du trait, rayon, un nom de palette (ou tous les calques de végétation, ou `any_prop`).
- `set_face_detail_type` : des faces et un type (`none`, un groupe, ou vide).
- `paint_displacement` en mode `alpha` peint l'alpha des détails.
- `preview_detail_props` et `list_detail_types` : voir [Aperçu des detail props](detail-props-preview.md).

Voir aussi [Outil sprinkle](sprinkle-tool.md), [Semis par matériau](scatter-by-material.md), [Placement sur formes réelles](real-shape-placement.md).
