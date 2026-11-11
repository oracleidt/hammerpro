[Documentation](../README.md) > Fonctionnalités > Couleurs d'affichage | [English](../../en/features/display-colors.md) | [Русский](../../ru/features/display-colors.md)

# Couleurs d'affichage

Donnez à n'importe quel brush ou entité sa propre couleur dans les vues 2D, pour distinguer facilement les parties d'une map chargée : les sols d'une couleur, les murs d'une autre, les triggers, le détail et les props chacun dans la sienne. La couleur est enregistrée dans la map comme le font Valve Hammer et Hammer++, donc les autres éditeurs affichent les mêmes couleurs.

## Ouvrir

| Où | Quoi |
| --- | --- |
| **Panneau des propriétés** (dock de droite), section **Couleur d'affichage** | Affichée pour toute sélection : brushes du monde (sous **Lier à une entité**), entités de brushes, entités ponctuelles, groupes et sélections mixtes (au-dessus de **Commentaires de l'éditeur**). |
| Clic droit dans une vue 2D ou 3D > **Définir la couleur d'affichage...** | Une petite fenêtre **Couleur d'affichage** pour la sélection. |
| **Édition > Couleurs d'affichage > Définir la couleur d'affichage...** (`set_display_color`) | La même fenêtre. |
| **Édition > Couleurs d'affichage > Couleurs d'affichage aléatoires** (`random_display_colors`) | Une couleur différente pour chaque objet sélectionné, d'un coup. |
| **Édition > Couleurs d'affichage > Couleurs d'affichage par défaut** (`reset_display_colors`) | Les objets sélectionnés retrouvent leur aspect par défaut. |
| **Affichage > Options d'affichage > Couleurs des objets en 2D** (`toggle_object_colors_2d`) | Montre les couleurs d'affichage dans les vues 2D, ou l'aspect classique. |
| **Affichage > Options d'affichage > Couleurs des objets en 3D filaire et plat** (`toggle_object_colors_3d`) | Les utilise aussi dans la vue 3D en mode filaire et plat. |

Aucune de ces actions n'a de raccourci par défaut ; donnez-leur-en un dans **Outils > Options... > Clavier** si vous vous en servez souvent.

## La section Couleur d'affichage

| Contrôle | Effet |
| --- | --- |
| Pastille de couleur | Ouvre un sélecteur de couleur. Glisser dans le sélecteur recolore la sélection en direct ; tout le glissé compte pour une seule étape d'annulation. À côté : la valeur hexadécimale, **Par défaut** quand les objets n'ont pas de couleur choisie, ou **(valeurs multiples)** quand les objets sélectionnés ont des couleurs différentes. |
| Palette | 16 couleurs prédéfinies choisies pour bien se lire sur un fond 2D sombre comme clair, et pour rester loin du rouge de la sélection. Survolez une pastille pour voir sa valeur hexadécimale ; cliquez pour l'appliquer. |
| **Couleur aléatoire par objet** | Chaque brush et chaque entité sélectionnés reçoivent leur propre couleur, réparties sur le cercle des couleurs pour que les voisins diffèrent. |
| **Couleur par défaut** | Retour à l'aspect habituel : brushes du monde en blanc, entités dans la couleur de leurs données de jeu (FGD). Grisé quand rien n'a de couleur choisie. |
| **Activer** | Affiché seulement quand **Couleurs des objets en 2D** est désactivé, pour le réactiver. |

Chaque changement est une étape d'annulation (`Ctrl+Z`).

## Ce qui est coloré

- **Brushes du monde** : chaque brush sélectionné.
- **Entités de brushes** (`func_detail`, triggers...) : l'entité et tous ses brushes, comme un seul objet. En mode de sélection **Solides**, vous pouvez colorer un seul brush d'une entité ; ce brush garde alors sa propre couleur.
- **Entités ponctuelles** : la boîte, le fil de fer du modèle dans les vues 2D et l'étiquette du nom de l'entité.
- **Groupes** : sélectionner un groupe (mode de sélection **Groupes**) colore tous ses membres, et la couleur du groupe lui-même est mise à jour quand tout le groupe est sélectionné.
- **Sélections mixtes** : tout ce qui est sélectionné, brushes comme entités.

## Comment les vues 2D choisissent la couleur

Avec **Couleurs des objets en 2D** activé (par défaut), chaque objet est dessiné, dans cet ordre :

1. dans sa couleur d'affichage choisie ;
2. dans la couleur de son visgroup, quand **Couleurs des groupes de visibilité** est activé (voir [Visgroups](visgroups.md)) ;
3. dans la couleur classique : blanc pour les brushes du monde, couleur des données de jeu pour les entités.

La sélection est toujours dessinée par-dessus dans la couleur de sélection (rouge par défaut), et l'objet survolé est éclairci, donc la sélection reste bien visible quelles que soient les couleurs choisies.

Sur un fond 2D clair (**Outils > Options... > Vues > Fond 2D**), les vues 2D assombrissent les couleurs claires (brushes du monde blancs compris) et renforcent leur teinte pour qu'elles gardent assez de contraste, et dessinent une grille plus douce pour que les objets ressortent ; sur le fond sombre par défaut, les couleurs sont dessinées telles quelles.

Avec **Couleurs des objets en 2D** désactivé, les vues montrent l'aspect classique et ignorent les couleurs choisies (elles restent dans la map). La vue 3D garde ses couleurs classiques sauf si **Couleurs des objets en 3D filaire et plat** est activé ; les modes texturé et éclairé ne changent jamais.

## Enregistré dans la map

La couleur est la clé standard `color` du bloc `editor` de chaque solide et entité (et groupe) du VMF :

```
editor
{
	"color" "40 165 225"
	"visgroupshown" "1"
	"visgroupautoshown" "1"
}
```

Valve Hammer et Hammer++ lisent et écrivent cette même clé : les couleurs survivent à l'aller-retour et apparaissent aussi dans leurs vues. Hammer donne à chaque nouveau brush une couleur aléatoire de la forme `0 x y` (x et y de 100 à 255) et à chaque entité `220 30 220` ; Hammer Pro fait de même et considère ces valeurs comme « pas de couleur choisie », c'est pourquoi les maps venues de Hammer gardent leur aspect classique. Une couleur choisie qui tomberait dans ce motif est décalée d'un cran (par exemple `0 180 255` est enregistrée `1 180 255`), ce qui ne se voit pas.

## Réglages

| Réglage | Défaut | Effet |
| --- | --- | --- |
| `render.object_colors_2d` | `true` | **Couleurs des objets en 2D** (**Outils > Options... > Rendu**). |
| `render.object_colors_3d` | `false` | **Couleurs des objets en 3D filaire et plat**. |
| `edit.new_brush_color` | aucune | **Couleur des nouveaux brushes** (**Outils > Options... > Édition**, **Personnalisée**) : la couleur d'affichage donnée aux brushes créés avec les outils. Désactivé : la couleur par défaut aléatoire habituelle de Hammer. |

## MCP

| Outil | Effet |
| --- | --- |
| `get_display_color` | Couleurs d'objets (par défaut : la sélection) : `color` par objet (`[r, g, b]` ou `null` pour l'aspect par défaut), l'état commun `state` (`none`, `default`, `{ "custom": [r, g, b] }` ou `mixed`) et les deux réglages de vue. |
| `set_display_color` | `color: [r, g, b]`, `random: true` ou `reset: true` sur `ids` / `solid_ids` ou la sélection. Une étape d'annulation. |
| `set_setting` | `render.object_colors_2d`, `render.object_colors_3d`, `edit.new_brush_color` (`[r, g, b]` ou `null`). |
| `run_action` | `set_display_color`, `random_display_colors`, `reset_display_colors`, `toggle_object_colors_2d`, `toggle_object_colors_3d`. |

Voir la [référence des outils MCP](../../MCP.md) (en anglais).

## Astuces

- Colorez par rôle plutôt que par pièce : une couleur pour les brushes de détail, une pour les clips et triggers, une pour la structure. Les vues 2D se lisent alors comme un plan.
- **Couleur aléatoire par objet** sur tout un étage d'un coup montre vite où commence et finit chaque brush.
- Couleurs des visgroups et couleurs d'affichage vont ensemble : laissez des objets sans couleur choisie et ils suivent leur visgroup.

## FAQ

**J'ai ouvert une map Hammer et tout est blanc, comme avant.** Les brushes faits dans Hammer portent les couleurs automatiques de Hammer, que Hammer Pro considère comme « pas de couleur choisie ». Choisissez des couleurs et elles s'affichent aussitôt.

**Mes couleurs ont disparu.** Vérifiez **Affichage > Options d'affichage > Couleurs des objets en 2D**. Les couleurs sont toujours dans la map.

**Hammer affichera-t-il mes couleurs ?** Oui, ce sont les couleurs d'éditeur standard. Hammer s'en sert pour les groupes, les entités et sa vue 3D filaire ; ce qui s'en voit dans ses vues 2D dépend de ses propres options.

## Pages liées

- [Propriétés des entités](entity-properties.md)
- [Visgroups](visgroups.md)
- [Groupes](groups.md)
- [Modes de rendu et options d'affichage](render-modes-display.md)
