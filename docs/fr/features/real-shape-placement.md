[Documentation](../README.md) > Fonctionnalités > Placement sur les formes réelles | [English](../../en/features/real-shape-placement.md) | [Русский](../../ru/features/real-shape-placement.md)

# Placement sur les formes réelles

Hammer Pro mesure les props, les brushes, les displacements et les instances sur leur géométrie réelle : les triangles du maillage `.mdl` après ses angles, son `modelscale` et ses body groups, les plans des brushes, les surfaces déplacées. Rien n'utilise une boîte englobante quand la forme réelle est connue. Ces mêmes mesures servaient déjà aux outils IA du [serveur MCP](../reference/mcp-server.md) (`place_object`, `check_placement`, `find_floating_objects`, `get_object_shape`, `render_shape`) et à **Poser au sol** ; cette page explique comment s'en servir à la main.

## Commandes de placement

Elles sont dans **Outils > Placement et props** (premier groupe), dans le menu du clic droit des vues (sous-menu **Placer sur les formes réelles**, sous **Poser au sol**) et dans la palette de commandes (`Ctrl+P`).

| Commande | Raccourci | Ce qu'elle fait |
| --- | --- | --- |
| **Poser sur la cible** | `Alt+T` | Sélectionnez les objets à déplacer, puis la cible en dernier (`Ctrl+clic`). Les objets gardent leur disposition, sont centrés au-dessus de la cible et descendus dessus. |
| **Plaquer contre le mur** | `Alt+L` | Fait glisser chaque objet sélectionné jusqu'à ce qu'il touche la surface la plus proche dans la direction de la vue (voir plus bas), à fleur, sans le tourner. |
| **Plaquer contre le mur, dos au mur** | `Alt+Maj+L` | La même chose, mais tourne d'abord chaque prop pour que son avant (l'axe +X du modèle) regarde à l'opposé du mur, comme une étagère ou un distributeur. |
| **Placer à côté de la cible** | `Alt+X` | Met les objets à côté de la cible (sélectionnée en dernier), du côté d'où ils viennent, en la touchant sans la chevaucher, puis les descend sur le sol. |
| **Reposer les objets flottants** | `Alt+Maj+N` | Descend les objets sélectionnés qui flottent et remonte ceux qui s'enfoncent dans le sol (16 unités au plus). Sans sélection, il repose tous les props flottants ou enfoncés de la map, sauf ceux du skybox 3D et les props suspendus (voir plus bas). Les piles se reposent ensemble : une caisse sur une caisse flottante la suit vers le bas. |
| **Ajuster au sol (appuis)** | `Alt+G` | Pour les tables, les chaises, les véhicules : descend chaque objet et l'incline (jusqu'à 30 degrés) pour que ses points les plus bas (pieds, roues) touchent tous le sol irrégulier. |
| **Aperçu de la forme** | `Alt+Maj+M` | Ouvre la fenêtre d'aperçu de la forme (voir plus bas). |

Direction de **Plaquer contre le mur**, dans cet ordre :

1. la surface sur laquelle vous avez fait un clic droit dans la vue 3D, quand la commande vient du menu du clic droit ;
2. une face sélectionnée (sélectionnez la face du mur avec l'outil texture, puis les objets) ;
3. la vue sous la souris : dans la vue 3D, la direction où regarde la caméra (gardée horizontale), dans la vue de face vers -X, dans la vue de côté vers +Y ;
4. dans la vue de dessus ou sans vue, le mur le plus proche parmi les quatre directions horizontales.

Chaque commande est une étape d'annulation et dit ce qu'elle a fait dans un message en bas à droite, par exemple *3 objets déplacés, écart 0*. L'écart est la distance restante sous les objets (0 veut dire qu'ils touchent). Un avertissement suit quand le résultat chevauche encore un autre objet ou quand un objet s'est posé sur autre chose que la cible.

## Dimensions réelles

**Affichage > Options d'affichage > Dimensions réelles** (`Alt+M`, aussi dans le menu du panneau des propriétés de l'entité) affiche, pour la sélection :

- dans la barre d'état : la taille réelle **largeur × profondeur × hauteur** de l'objet (l'empreinte est le plus petit rectangle autour du maillage vu de dessus, donc un prop tourné donne sa propre longueur et sa propre largeur), puis son écart au sol (*flotte à 12 du sol*, *enfoncé de 3 dans le sol*, *posé au sol*) et la profondeur de chevauchement avec un autre objet. Son infobulle liste chaque objet sélectionné ; un clic ouvre l'aperçu de la forme ;
- dans le panneau des propriétés de l'entité, la même ligne sous le nom de la classe, avec un bouton **Aperçu de la forme** ;
- dans les vues 2D et 3D : le contour réel de l'empreinte au bas de l'objet (cyan), son plus petit rectangle, les points de contact les plus bas (croix vertes), une ligne du bas de l'objet jusqu'au sol quand l'objet flotte (orange) ou jusqu'au sol vers le haut quand il s'enfonce (rouge), et une étiquette au-dessus.

Jusqu'à 16 objets sont mesurés (la taille couvre toute la sélection). Les valeurs suivent les modifications un instant après que la sélection a cessé de bouger.

## Aperçu de la forme

**Outils > Placement et props > Aperçu de la forme** (`Alt+Maj+M`) dessine la forme réelle de la sélection de dessus, de face, de côté et en 3D, avec les dimensions, l'origine, le point le plus bas, les points d'attache et le maillage de collision (fil de fer cyan) quand le modèle en a un. **Sélection** dessine les objets sélectionnés ; dans le [navigateur de modèles](model-browser.md), le bouton **Aperçu de la forme** d'un modèle dessine ce modèle avant que vous le placiez (onglet **Modèle :**). **Actualiser** redessine.

## Vérifier les problèmes

[Vérifier les problèmes](check-problems.md) liste trois sortes de problèmes de plus, trouvées en arrière-plan comme les autres :

| Problème | Gravité | Trouvé quand | Correction |
| --- | --- | --- | --- |
| **objet flottant** | avertissement | un prop flotte à plus de 0,5 unité au-dessus de la surface en dessous et ne touche rien sur ses côtés ni au-dessus (les lampes sur un mur, les panneaux sous un plafond ne flottent pas) | le descend sur la surface |
| **objet enfoncé** | information | un prop s'enfonce de plus de 0,5 unité dans la surface sur laquelle il repose | le remonte pour qu'il repose sur la surface (quand il s'enfonce de 16 unités ou moins) |
| **objets qui se chevauchent** | information | seulement avec **Lister aussi les props qui chevauchent d'autres objets** activé : un prop entre de plus de 1 unité dans un autre prop, un brush ou une entité de brush (le sol sur lequel il repose est le cas de l'objet enfoncé) | le sort par le chemin le plus court (16 unités au plus) ; pour deux props, le plus petit bouge |

Seuls les props sont vérifiés (entités dont la clé `model` est un `.mdl`, sans brushes et sans `parentname`), sur leur maillage de rendu. Les modèles d'éditeur des entités ponctuelles (`info_node`, `info_player_start`...) ne sont ni vérifiés ni utilisés comme surfaces. **Aller à** sélectionne l'objet (et l'autre objet d'un chevauchement). **Corriger** le déplace de la distance mesurée, **Tout corriger** corrige chaque problème de placement listé en même temps que les autres en une seule étape d'annulation, et les props empilés bougent ensemble. Un problème dont l'objet a bougé depuis la vérification n'est pas corrigé de nouveau (la vérification suivante le liste avec sa nouvelle distance).

Exclus de la vérification (les outils MCP `find_floating_objects` et `check_placement` les signalent toujours) :

- les props du skybox 3D : la pièce autour de chaque `sky_camera`, trouvée en regardant depuis la caméra dans les six directions jusqu'à toucher un brush (une caméra dans une zone ouverte n'exclut rien) ;
- les props faits pour pendre, pour les objets flottants : un modèle dont le nom de fichier contient `lamp`, `light`, `chain`, `hanging`, `sign`, `wire` ou `cable`, un modèle avec un point d'attache dans les 15 % du haut de sa hauteur (un crochet), ou un prop qui arrive à moins de 1 unité d'un mur ou d'un plafond.

La vérification tourne sur des threads de travail un instant après chaque modification ; le thread principal ne la prépare que par tranches de quelques millisecondes. Elle se coupe avec **Vérifier les props flottants et enfoncés** dans **Paramètres > Santé de la map** ou **Paramètres > Prefabs** (`placement.problem_checks`, activé par défaut). Les chevauchements sont désactivés par défaut parce que les mappeurs enfoncent souvent des props les uns dans les autres exprès et que cette vérification est plus lente sur les grandes maps : activez **Lister aussi les props qui chevauchent d'autres objets** en dessous (`placement.overlap_check`).

## Nouveaux props sur la surface

**Poser les nouveaux props sur la surface en dessous** (**Paramètres > Prefabs**, `placement.auto_settle_on_create`, désactivé par défaut) : un prop créé avec l'outil entité, déposé depuis le navigateur de modèles ou créé par une IA via MCP (`create_entity`, `create_entity_fgd`) tombe sur ce qui est en dessous, mesuré sur son maillage réel. La descente fait partie de la même étape d'annulation que la création.

## Pour l'IA (MCP)

- Les commandes sont des actions : `run_action` avec `place_on_top`, `place_against_wall`, `place_against_wall_away`, `place_next_to`, `settle_floating`, `fit_to_ground`, `show_shape_preview`, `toggle_real_dimensions`.
- Les paramètres sont `placement.problem_checks`, `placement.overlap_check`, `placement.auto_settle_on_create` et `placement.show_real_dimensions` (`get_settings`, `set_setting`).
- `check_problems`, `get_problems` et `fix_problems` listent et corrigent les codes `floating_object`, `sunk_object` et `overlapping_objects`.
- Les outils qui créent ou déplacent des objets (`create_entity`, `create_entity_fgd`, `move_objects`, `transform_objects`, `rotate_objects`, `resize_objects`, `flip_objects`, `align_objects`, `random_rotate_objects`, `snap_objects_to_grid`, `set_entity_keyvalues`, `set_keyvalue`, `paste_vmf_text`, `paste_clipboard`, `paste_special`, `duplicate_objects`, `array_duplicate`, `sprinkle`, `insert_prefab`, `import_models_batch`, `create_brush`, `create_primitive`) vérifient les props et les brushes qu'ils ont créés ou déplacés (32 au plus) et ajoutent un `placement_note` à leur résultat quand l'un d'eux flotte à plus de 0,5 unité, s'enfonce dans le sol ou chevauche un autre objet, en nommant la correction (`drop_to_surface`, `place_object`). Quand `placement.auto_settle_on_create` est activé, `create_entity` et `create_entity_fgd` ajoutent `auto_settled` avec le nombre de props descendus.

## Paramètres

| Paramètre | Clé | Par défaut |
| --- | --- | --- |
| Vérifier les props flottants et enfoncés | `placement.problem_checks` | activé |
| Lister aussi les props qui chevauchent d'autres objets | `placement.overlap_check` | désactivé |
| Poser les nouveaux props sur la surface en dessous | `placement.auto_settle_on_create` | désactivé |
| Afficher les dimensions réelles de la sélection | `placement.show_real_dimensions` | désactivé |

---
[Retour à l'index de la documentation](../README.md)
