[Documentation](../README.md) > Référence > Commandes de script | [English](../../en/reference/script-commands.md)

# Commandes de script

`hammerpro.exe --script fichier.txt` joue dans l'éditeur un fichier texte de commandes : mouvements et clics de souris, touches, actions, options d'outil, captures d'écran et vérifications. C'est fait pour les tests automatisés de bout en bout et pour reproduire un bug pas à pas. Une exécution de script est une exécution automatisée : paramètres par défaut, rien n'est écrit dans vos paramètres, vos logs ou votre jeu (voir [Ligne de commande](command-line.md)).

## Format du fichier

- Une commande par ligne. Les lignes vides sont ignorées.
- `#` commence un commentaire : tout ce qui suit sur la ligne est ignoré (un texte de `type` ne peut donc pas contenir `#`).
- Les arguments sont séparés par des espaces. Les commandes qui prennent un chemin ou un texte (`open`, `save`, `screenshot`, `type`, `log`...) lisent tout le reste de la ligne, espaces compris.
- Une ligne illisible arrête l'exécution avant son début avec `script line N: cannot parse ...` et le code de sortie 2.
- Chaque commande affiche `script: ...` sur stderr quand elle s'exécute, ce qui rend la sortie facile à suivre.
- La fenêtre ne se ferme pas à la fin du script : terminez par `exit`.

## Positions (POS)

| Forme | Signification |
|---|---|
| `X,Y` | Coordonnées de fenêtre en points, depuis le coin haut gauche de la fenêtre. |
| `@vN:FX,FY` | Une fraction (0 à 1) du rectangle de la vue N. Dans la disposition par défaut `@v0` est la vue 3D, `@v1` Dessus, `@v2` Face, `@v3` Côté. `@v1:0.5,0.5` est le centre de la vue Dessus. |
| `@top:A,B` | Coordonnées monde dans la première vue Dessus (X, Y). Aussi `@front:A,B` (Y, Z) et `@side:A,B` (X, Z). |

## Boutons et modificateurs

- Boutons de souris : `left` (par défaut), `right`, `middle`.
- Modificateurs (MODS) : `ctrl`, `shift`, `alt`, reliés par `+`, par exemple `ctrl+shift`.
- Combinaisons de touches (COMBO) : modificateurs et une touche reliés par `+`, par exemple `Ctrl+Shift+S`, `Enter`, `Escape`, `Delete`, `F9`, `Space`, `ArrowUp`, `A`. Les noms de touches sont ceux d'egui (en anglais).
- Les modificateurs maintenus avec `keydown` s'appliquent à tous les événements suivants jusqu'au `keyup`.

## Commandes

### Temps

| Commande | Effet |
|---|---|
| `wait N` | Attend N images. Des images, pas du temps : sous forte charge une exécution peut tomber à quelques images par seconde, soyez large. |
| `idle MS` | Laisse passer MS millisecondes de temps réel sans forcer d'images (l'éditeur ne dessine que les images qu'il demande lui-même, comme quand personne ne touche la souris), puis affiche combien d'images ont été dessinées. |

### Souris

| Commande | Effet |
|---|---|
| `move POS` | Déplace le pointeur. |
| `click POS [bouton] [mods]` | Appui et relâchement. |
| `dclick POS [bouton] [mods]` | Double clic. |
| `drag POS POS [bouton] [étapes] [mods]` | Appuie à la première position, va à la seconde en `étapes` mouvements (8 par défaut), relâche. Les modificateurs restent maintenus pendant tout le glisser. |
| `press POS [bouton] [mods]` | Appuie seulement sur un bouton de souris : il reste enfoncé pendant les commandes suivantes (`idle`, `move`...). |
| `release POS [bouton] [mods]` | Relâche un bouton enfoncé avec `press`. |
| `scroll POS DY [mods]` | Molette. Un DY positif zoome ou avance. `scroll @v1:0.5,0.5 1 ctrl` change la taille de la grille. |

### Clavier

| Commande | Effet |
|---|---|
| `key COMBO` | Appuie et relâche une touche, par exemple `key Ctrl+Shift+S`, `key Enter`. `Ctrl+C` et `Ctrl+X` envoient aussi les événements copier et couper qu'écoutent les champs de texte. |
| `keydown COMBO` | Appuie sur des touches et modificateurs et les garde enfoncés, par exemple `keydown Shift`, `keydown Space` (déplacement avec un glisser ensuite). |
| `keyup COMBO` | Relâche les touches et modificateurs maintenus avec `keydown`. |
| `hold TOUCHE IMAGES` | Maintient une touche pendant quelques images (30 par défaut), par exemple pour faire voler la caméra 3D. |
| `type TEXTE` | Tape le texte dans le champ qui a le focus. |

### Éditeur

| Commande | Effet |
|---|---|
| `action ID` | Lance une action par son identifiant, comme son entrée de menu ou son raccourci (`compile_now`, `toggle_grid`...). Les identifiants sont listés dans [Actions et raccourcis](actions-shortcuts.md). Un identifiant inconnu affiche `unknown action`. |
| `tool NOM` | Change d'outil : `select`, `camera`, `entity`, `block`, `texture`, `decal`, `overlay`, `clip`, `extrude`, `bevel`, `vertex`, `path`, `polygon`, `curve`, `sprinkle`, `displacement`, `cordon`. |
| `tool_option CLÉ VALEUR` | Règle une option de l'outil actif (voir Options des outils plus bas). La valeur est un seul mot : utilisez des virgules dans les listes (`custom_axis 0,0,1`). Une clé inconnue ou une mauvaise valeur affiche `tool ... has no option ...`. |
| `open CHEMIN` | Ouvre une map et attend son chargement (jusqu'à 120 secondes). Si la map est déjà ouverte, son onglet est affiché. |
| `save CHEMIN` | Enregistre la map dans CHEMIN, sans boîte de dialogue. |
| `material NOM` | Règle le matériau courant (celui que reçoivent les nouveaux brushes et Appliquer). |
| `select_id ID` | Sélectionne l'entité qui porte cet identifiant, sinon le solide qui le porte. |
| `grid TAILLE` | Règle la taille de la grille. |
| `camera X Y Z LACET TANGAGE` | Place la caméra 3D. |
| `player X Y Z LACET TANGAGE` | Dans la vue joueur : place les yeux à cet endroit, regardant dans cette direction (tangage positif vers le haut), en noclip pour que le joueur reste en place. |
| `setting CHEMIN VALEUR` | Change un paramètre par son chemin pointé, comme l'outil MCP `set_setting`. VALEUR est du JSON ou une simple chaîne : `setting render.normal_maps false`. Les clés sont dans [Référence des paramètres](settings.md). |
| `custom_folder CHEMIN [\| préfixe]` | Ajoute un dossier de textures personnalisé (activé) et recharge le contenu. Sans préfixe, un préfixe est proposé à partir du nom du dossier. |
| `import_model CHEMIN` | Ouvre la fenêtre d'import de modèle avec un fichier (FBX, OBJ, glTF, GLB). |
| `import_model_now CHEMIN` | Importe le modèle avec les options par défaut. |
| `import_model_scaled ÉCHELLE CHEMIN` | Importe le modèle avec cette échelle (supérieure à 0). Les scripts écrivent le modèle dans un dossier de contenu temporaire, jamais dans le jeu. |

### Fenêtre et apparence

| Commande | Effet |
|---|---|
| `theme dark` / `theme light` | Change le thème de l'interface. |
| `background OPACITÉ CHEMIN` | Affiche une image par-dessus la fenêtre (comme l'image de fond de **Paramètres > Général**), opacité de 0 à 1. |
| `window_size L H` | Redimensionne la fenêtre principale, en points (sa taille minimale s'applique toujours). |
| `window_maximize on` / `off` | Agrandit ou restaure la fenêtre principale. |

### Sorties et vérifications

| Commande | Effet |
|---|---|
| `screenshot CHEMIN` | Enregistre une capture PNG de la fenêtre principale. |
| `screenshot_window NOM CHEMIN` | Enregistre une capture d'une fenêtre de dialogue affichée dans sa propre fenêtre : `compile`, `uv_editor` ou `portals` (comme l'outil MCP `screenshot_window`). |
| `log TEXTE` | Affiche `script: TEXTE` sur stderr. |
| `stats` | Affiche la fréquence d'images, le temps par image, la pire image, le nombre de solides, faces, entités et objets sélectionnés, l'état des ressources, et les statistiques du rendu, des particules et des cubemaps. Dans la vue joueur, affiche aussi les styles de lumière. |
| `vram` | Affiche l'utilisation de la mémoire graphique : utilisation et budget en Mo, d'où vient l'utilisation (`driver` pour le pilote ou `estimate` pour une estimation), la pression (`normal`, `high`, `critical`), la répartition estimée (géométrie, textures, modèles, lightmaps, monde compilé, autre, maps gardées pour les onglets en arrière-plan), la limite de taille des textures, les erreurs de mémoire insuffisante vues et le nombre de libérations de mémoire. |
| `history` | Affiche les étapes d'annulation et de rétablissement, si une modification est encore ouverte, et les limites de la sélection. |
| `assert_selected N` | Arrête l'exécution avec le code 3 si le nombre d'objets sélectionnés n'est pas exactement N. |
| `assert_tabs N` | Arrête l'exécution avec le code 3 si le nombre de maps ouvertes n'est pas exactement N. |
| `exit` | Ferme Hammer Pro. |

## Options des outils

`tool_option` utilise les mêmes noms d'options que les outils MCP `get_tool_options` et `set_tool_options`. Les interrupteurs acceptent `on`/`off`, `true`/`false` ou `1`/`0`.

### Outil de sélection (`tool select`)

| Clé | Valeurs |
|---|---|
| `handles` | `scale`, `rotate` ou `skew` : ce que font les poignées de la boîte de sélection dans les vues 2D. |
| `box` | `touching`, `inside` ou `centers` : quels objets prend une sélection par rectangle dans une vue 2D. |
| `gizmo` | on/off : le gizmo de transformation dans la vue 3D. |
| `gizmo_mode` | `move`, `rotate` ou `scale`. |
| `gizmo_local` | on/off : le gizmo d'une seule entité ponctuelle suit ses angles (enregistré dans les paramètres). |
| `stretch_handles` | on/off : flèches d'étirement sur les faces des brushes sélectionnés dans la vue 3D (enregistré dans les paramètres). |

### Outil bloc (`tool block`)

| Clé | Valeurs |
|---|---|
| `kind` | `block`, `wedge`, `cylinder`, `spike`, `sphere`, `arch` ou `torus`. |
| `sides` | Côtés des primitives rondes, 3 à 64. |
| `wall` | Épaisseur de mur de l'arche en unités (1 ou plus). |
| `arc` | Arc de l'arche en degrés, 1 à 360. |
| `start` | Angle de départ de l'arche en degrés. |
| `rise` | Hauteur ajoutée à l'arche en unités. |
| `tube_sides` | Côtés du tube du tore, 3 à 64. |
| `ring` | Largeur de l'anneau du tore en unités (1 ou plus). |
| `torus_arc` | Balayage du tore en degrés (au-delà de 360, une spirale avec une montée). |
| `torus_start` | Angle de départ du tore en degrés. |
| `torus_rise` | Hauteur gagnée sur le balayage du tore en unités. |
| `reorient` | on/off : les primitives dessinées dans la vue de face ou de côté ou sur un mur s'orientent selon la profondeur de la vue (enregistré). |
| `preview` | on/off : la grande fenêtre d'aperçu de la forme. |
| `tex_scale` | Échelle de texture des nouvelles faces, 0.01 à 64. |
| `create_as` | `world`, ou une classe d'entité brush (`func_detail`, `trigger_multiple`...). |
| `keep_active` | on/off : rester dans l'outil bloc après la création. |

### Outil courbe (`tool curve`)

| Clé | Valeurs |
|---|---|
| `subdivisions` | Nombre de brushes le long de la courbe, 2 à 4096. |
| `thickness` | Largeur des brushes en travers de la courbe, 1 à 1024 unités. |
| `height` | Hauteur dans la profondeur de la vue, 1 à 16384 unités (change aussi la courbe en cours), ou `auto` : selon la sélection comme l'outil bloc. |
| `tex_scale` | Échelle de texture des nouvelles faces, 0,01 à 64. |
| `create_as` | `world`, ou une classe d'entité brush (`func_detail`...) qui contient tous les brushes. |
| `keep_active` | on/off : rester dans l'outil courbe après la création. |

### Outil chemin (`tool path`)

| Clé | Valeurs |
|---|---|
| `class` | Classe des nouveaux points : `path_corner`, `path_track`, `path_corner_crash`, `keyframe_rope`... |
| `base_name` | Nom de base des points (`path` les nomme `path_1`, `path_2`...). |
| `loop` | on/off (aussi yes/no) : terminer un chemin relie son dernier point au premier. |
| `relink` | on/off : Suppr dans cet outil relie les voisins des points supprimés. |
| `offset` | Hauteur au-dessus de la surface des points placés dans la vue 3D, -1024 à 1024. |

### Outil extruder / creuser (`tool extrude`)

| Clé | Valeurs |
|---|---|
| `mode` | `carve` (creuser le contour dans le brush) ou `extrude` (le sortir de la face). |
| `depth` | Profondeur en unités, 0,125 à 16384. |
| `through` | on/off : creuser toute l'épaisseur du brush. |
| `new_faces` | `face` (la texture de la face cliquée) ou `current` (le matériau actuel) sur les nouvelles faces. |

### Outil biseau (`tool bevel`)

| Clé | Valeurs |
|---|---|
| `radius` | Rayon de l'arrondi en unités, 0,25 à 16384 (limité par les faces pendant le biseau). |
| `segments` | Faces de l'arrondi, 1 (chanfrein plat) à 32. |
| `inverse` | on/off : biseau concave (une gorge, ou un arrondi qui remplit un coin intérieur). |
| `new_faces` | `face` (les textures autour de l'arête, enroulées) ou `current` (le matériau courant) sur les faces arrondies. |

Pour biseauter depuis un script, pointez l'arête et envoyez sa touche : `move @v0:0.5,0.4` puis `key Ctrl+B`, `scroll @v0:0.5,0.4 2` pour 2 segments de plus, `key B` pour le biseau inversé, `key Enter` pour appliquer.

### Outil texture et éditeur d'UV (`tool texture`)

| Clé | Valeurs |
|---|---|
| `uv_place` | `panel` (ancré dans le panneau d'édition des faces) ou `window` (une fenêtre à part). |
| `uv_all_faces` | on/off : l'éditeur d'UV modifie toutes les faces sélectionnées, ou seulement celle en cours. |
| `uv_mode` | `face`, `edge` ou `vertex`. |
| `uv_layer` | `base`, `base2`, `bump`, `bump2`, `detail`, `envmask` ou `blendmod` (si le matériau l'a). |
| `uv_keep_layers` | on/off : déplacer la texture de base laisse les calques en place sur la face. |
| `uv_fit_tiles` | Nombre de répétitions le long de l'arête pour l'ajustement le long de l'arête, 0.25 à 64. |
| `uv_vertices` | Sommets sélectionnés de la face éditée, indices séparés par des virgules (vide ou `none` : aucun). Passe en mode sommet. |
| `uv_edges` | Arêtes sélectionnées de la face éditée (indice du premier sommet de chacune), séparées par des virgules. Passe en mode arête. |

### Outil displacement (`tool displacement`)

| Clé | Valeurs |
|---|---|
| `mode` | `select`, `geometry` (aussi `paint`), `alpha`, `sculpt` ou `tag`. |
| `effect` | Peinture de géométrie : `raise`, `lower`, `smooth` ou `set` (aussi `raise_to`). |
| `axis` | `normal`, `subdiv`, `center` (aussi `brush_center`), `view`, `x`, `y`, `z` ou `custom`. |
| `custom_axis` | Direction de l'axe personnalisé, `x,y,z`. |
| `radius` | Rayon du pinceau en unités, 1 à 8192. |
| `strength` | Vitesse de montée et de descente en unités par seconde, 0 à 8192. |
| `smooth` | Force du lissage, 0 à 1. |
| `target_height` | Hauteur de l'effet set, -16384 à 16384. |
| `falloff` | `constant`, `linear` ou `smooth`. |
| `spatial` | on/off : distance 3D sur la surface (peint les murs et à travers les tuiles). |
| `line_of_sight` | on/off : seulement ce que la caméra voit au niveau du pinceau. |
| `autosew` | on/off : couture en direct des voisins. |
| `selected_only` | on/off : quand des displacements sont sélectionnés, seuls ceux-là sont peints. |
| `continuous` | on/off : garder le bouton enfoncé sans bouger continue de peindre. |
| `alpha_effect` | `add`, `remove`, `set` ou `smooth`. |
| `alpha_value` | Alpha visé par l'effet set, 0 à 255. |
| `alpha_strength` | Vitesse d'ajout et de retrait d'alpha par seconde, 0 à 16384. |
| `blend_channel` | `alpha` (aussi `none`) pour un mélange de deux textures, ou `1` à `4` pour une texture d'un mélange à 4 textures. |
| `sculpt` | `push`, `carve` ou `project`. |
| `push_mode` | `absolute` (unités) ou `adaptive` (pourcentage du rayon). |
| `sculpt_bounds` | `attenuated` ou `additive`. |
| `falloff_position` | Push : pourcentage du rayon qui reçoit tout le décalage, 0 à 100. |
| `falloff_final` | Push : pourcentage du décalage sous lequel l'atténuation ne descend jamais, 0 à 100. |
| `push_distance` | Décalage de push en unités, 0 à 4096. |
| `push_percent` | Décalage de push en pourcentage du rayon, 0 à 400. |
| `carve_depth` | Profondeur de carve en unités, 0 à 4096. |
| `carve_profile` | `v`, `u`, `flat` ou 5 profondeurs de 0 à 1 séparées par des virgules. |
| `project_range` | Project : les brushes plus loin que cette distance sont ignorés, 1 à 16384. |
| `scale_factor` | Facteur du bouton Échelle, -16 à 16. |
| `tag_kind` | `walkable`, `buildable` ou `remove`. |
| `tag_value` | `on`, `off` ou `auto`. |
| `mask_grid` | on/off : cacher la grille de triangles des displacements sélectionnés. |
| `mask_select` | on/off : ne pas teinter les displacements sélectionnés. |
| `create` | `2`, `3` ou `4` : crée des displacements de cette puissance sur les faces sélectionnées (les boutons Créer). |
| `select` | `all`, `adjacent`, `solids` ou `none` : les boutons de sélection du panneau. |
| `cast_shadows` | on/off sur les displacements sélectionnés. |
| `lightmap_scale` | 1 à 1024 sur les displacements sélectionnés. |
| `tag_selection` | N'importe quelle valeur : applique le `tag_kind` et le `tag_value` actuels à toute la sélection. |

## Exemple

```text
# Dessiner un bloc de 256 unités dans la vue Dessus, vérifier, compiler, capturer
tool block
tool_option kind block
drag @top:0,0 @top:256,256
key Enter
wait 10
assert_selected 1
action compile_now
idle 3000
screenshot C:\temp\apres.png
exit
```

## Pages liées

- [Ligne de commande](command-line.md)
- [Actions et raccourcis](actions-shortcuts.md)
- [Référence des paramètres](settings.md)
- [Serveur MCP](mcp-server.md)
- [Outil displacement](../features/displacement-tool.md)
- [Outil bloc](../features/block-tool.md)

---
[Retour à l'index de la documentation](../README.md)
