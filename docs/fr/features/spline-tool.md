[Documentation](../README.md) > Fonctionnalités > Outil spline | [English](../../en/features/spline-tool.md) | [Русский](../../ru/features/spline-tool.md)

# Outil spline

L'outil spline trace un chemin 3D modifiable et génère des objets le long : clôtures et rambardes, câbles, tuyaux, bordures et moulures en brushes, et lignes de props. La spline est gardée dans la carte : vous pouvez la sélectionner plus tard, déplacer ses points et les objets suivent. Rien n'est généré tant que vous ne le demandez pas, et chaque génération est une seule étape d'annulation.

## Ouverture

- **Outils > Courbes et routes > Outil spline**, ou son bouton dans la palette d'outils (groupe création, après l'outil tuyau). Il n'a pas de raccourci par défaut.
- **Outils > Courbes et routes > Modifier la spline** ouvre l'outil sur la spline sélectionnée (sélectionnez son ancre ou un objet généré le long). Ouvrir l'outil pendant qu'une spline est sélectionnée fait de même.
- **Outils > Courbes et routes > Figer la spline en objets ordinaires** supprime la spline et garde ses objets comme des objets ordinaires.

## Tracer le chemin

1. **Cliquez** dans une vue 2D pour ajouter un point (il se cale sur la grille ; sa profondeur est celle du point précédent). Dans la vue 3D, un clic pose le point sur la surface sous le curseur (brushes et déplacements), X et Y calés sur la grille quand le calage est actif.
2. Les nouveaux points se placent après le point sélectionné (avant lui quand le premier point est sélectionné) : vous pouvez prolonger chaque bout ou ajouter des points au milieu.
3. **Glissez** un point pour le déplacer (en 3D il glisse sur les surfaces). **Ctrl + clic** sur la courbe y insère un point. `Suppr` supprime le point sélectionné, `Retour arrière` le dernier point d'une nouvelle spline, `Échap` désélectionne, puis annule.
4. `Maj` + molette change le **roulis** du point sélectionné (5 degrés par cran), `Alt` + molette sa **largeur** (0,1 par cran).
5. `Entrée`, un double clic ou **Générer** crée la spline et ses objets. Le menu du clic droit propose Insérer un point ici, Supprimer le point, Fermer ou Ouvrir la boucle, Poignées automatiques, Générer, Figer et Nouvelle spline.
6. Après la génération, l'outil continue de modifier cette spline (les changements la régénèrent) ; **Nouvelle spline** en commence une autre.

L'aperçu montre la courbe et ce qui sera généré (brushes en fil de fer, boîtes des modèles à leur taille réelle, câbles qui pendent) dans toutes les vues, avec une étiquette donnant la longueur et les nombres.

## Options du chemin

| Option | Effet |
|---|---|
| **Catmull-Rom** / **Bézier** | Catmull-Rom passe par chaque point. Bézier donne au point sélectionné deux poignées à glisser ; **Poignées automatiques** les remet à la forme lisse par défaut. |
| **Boucle fermée** | Relie le dernier point au premier (3 points ou plus). |
| **Suivre le terrain** et sa hauteur | Le chemin épouse le sol sous lui entre les points (brushes et déplacements, pas les objets de la spline), à cette hauteur au-dessus. |
| **Roulis** du point | Incline le chemin à ce point en degrés : le profil d'un balayage, les rambardes et les modèles de clôture s'inclinent avec. Interpolé entre les points. |
| **Largeur** du point | Agrandit le profil, le rayon du câble et le décalage latéral à ce point. Interpolée entre les points. |
| **Décalage** x, z | Déplace les objets générés sur le côté (positif à droite du chemin) et en hauteur. |
| 🔄 régénération auto | Quand vous modifiez une spline qui existe déjà, ses objets sont régénérés peu après chaque changement (une étape d'annulation par changement). Désactivée : utilisez **Appliquer**. |

## Modes

**Spline seule** garde le chemin seul (pratique pour le préparer ou garder un guide).

**Clôture et rambardes**
- *Clôture de modèles* : un modèle répété le long du chemin. **Espacement** 0 prend la longueur du modèle (le long de son plus grand axe horizontal, ou de l'**axe** choisi) ; **Ajuster à la longueur** étire l'espacement pour que les segments finissent pile au bout du chemin. Chaque segment va d'un point du chemin au suivant, il suit donc la courbe comme une vraie clôture. **Modèle de poteau** ajoute des poteaux **à chaque point** de la spline et/ou **entre les segments** ; **Embout de début** et **Embout de fin** vont aux deux bouts. **Suivre la pente** incline les segments selon la pente du sol. La classe est prop_static par défaut.
- *Rambardes en brushes* : poteaux et lisses en brushes (nombre, hauteur et taille des poteaux, taille des lisses, espacement des poteaux), en func_detail par défaut.

**Câble**
- *Entités corde* : un move_rope au premier point puis des keyframe_rope, reliés par nom, avec **Mou**, largeur, matériau et subdivision. Une boucle fermée relie la dernière corde à la première. **Point tous les** pose un point de corde tous les N unités au lieu des points de la spline.
- *Câble en brushes* : un tube rond en brushes qui pend entre les points avec la même flèche que le mou.

**Tuyau, bordure, moulure** : des brushes balayés le long du chemin depuis un profil 2D : *Tuyau rond* (côtés, rayon), *Bordure* (largeur, hauteur, haut biseauté), *Moulure* (un profil en L), *Tube de rambarde* (un tube rond surélevé à Hauteur, pour les mains courantes) ou *Personnalisé* (un profil convexe tapé en paires `x:y`, mis à l'échelle par Largeur / 16). **Segment** est la longueur d'un brush le long du chemin. La texture continue sur toute la longueur sans couture entre les brushes. Les brushes vont dans un func_detail par défaut.

**Ligne de props** : des modèles posés tous les **Espacement** unités (0 : selon la taille du modèle), sur le chemin, d'un côté ou des deux (le décalage donnant la distance), avec un **décalage aléatoire**, une **rotation aléatoire**, une **échelle** aléatoire entre deux valeurs (écrite dans modelscale) et une **graine** pour que les mêmes réglages donnent la même ligne. **Tourner avec le chemin** les oriente le long. **Poser sur les surfaces** pose chaque prop sur la surface sous lui avec son vrai maillage, et **Éviter les chevauchements** ignore les props qui traverseraient un autre prop ou objet, mesurés sur leurs formes réelles (la console dit combien ont été ignorés).

## Comment la spline est gardée

Les objets générés et une ancre cachée (un `info_null`, retiré par le jeu au démarrage de la carte) sont groupés. L'ancre garde les points et toutes les options dans ses données d'éditeur, et chaque objet généré porte le numéro de la spline : la spline reste modifiable après enregistrement et réouverture de la carte. Déplacer ou tourner le groupe déplace la spline avec. Une copie d'un groupe de spline est une spline à part : modifier l'une ne change pas l'autre.

La modification remplace les objets générés. **Figer** supprime l'ancre et les marques en une étape d'annulation : les objets deviennent ordinaires et restent groupés.

## MCP

- `create_spline` crée une spline à partir de `points` (`[x, y, z]` ou `{position, roll, width, handle_in, handle_out}`), avec `mode`, `interp`, `closed`, `follow_terrain`, `terrain_offset`, toutes les options du mode dans `options` et `preview`.
- `edit_spline` modifie une spline (par son ancre ou un objet généré, par défaut la sélection) : tous les `points`, `insert_at` avec `insert_position`, `delete_point`, un `point` avec `position`, `roll`, `width`, `handle_in`, `handle_out`, `auto_handles`, les réglages et `options` ; `regenerate` à false enregistre seulement la nouvelle spline.
- `generate_along_spline` génère avec un `mode` et des `options`, ou décrit le résultat avec `preview`.
- `bake_spline` transforme les objets en objets ordinaires (`keep_group`).
- `get_spline` liste les splines, ou en décrit une avec `id`.
- `set_tool_options` avec l'outil `spline` règle les options de l'outil : toutes les options de `options` ci-dessus, `auto_regenerate`, `keep_active`, `selected_point` (à partir de 1), `point_x`, `point_y`, `point_z`, `point_roll`, `point_width` et `point_auto_handles`. Les actions `tool_spline`, `spline_edit` et `spline_bake` passent par `run_action`.

Les noms d'options sont : mode, interp, closed, follow_terrain, terrain_offset, material, tex_scale, detail, seed, align_slope, offset_x, offset_z, profile, sides, radius, width, height, segment, custom, fence, model, spacing, fit_spacing, model_axis, post_model, posts_at_points, post_every_segment, start_cap, end_cap, prop_class, rail_count, post_height, post_size, rail_size, post_spacing, cable, slack, rope_width, rope_material, subdiv, rope_every, models, side, jitter, yaw_jitter, face_path, scale_min, scale_max, drop, avoid_overlap. Voir la [référence des outils MCP](../../MCP.md).

## Conseils

- Pour une clôture sur un sol inégal, activez **Suivre le terrain** et **Suivre la pente**.
- Gardez un **Segment** court dans les virages serrés : chaque brush est droit.
- Une spline en mode *Spline seule* est un guide pratique pour essayer plusieurs modes avec `generate_along_spline` et `preview`.
