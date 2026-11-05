[Documentation](../README.md) > Fonctionnalités > Groupes | [English](../../en/features/groups.md) | [Русский](../../ru/features/groups.md)

# Groupes

Un groupe lie des objets entre eux (brushes, entités, autres groupes) : un clic les sélectionne tous et ils se déplacent ensemble. Hammer Pro lit et écrit les groupes de Hammer, donc les groupes faits dans Hammer ou Hammer++ survivent à l'aller-retour.

## Accès

| Action | Menu | Raccourci par défaut |
| --- | --- | --- |
| **Grouper** | **Outils > Grouper** | `Ctrl+G` |
| **Dégrouper** | **Outils > Dégrouper** | `Ctrl+U` |
| **Ignorer les groupes** | **Édition > Mode de sélection > Ignorer les groupes** | `Ctrl+W` |
| **Groupes** / **Objets** / **Solides** | **Édition > Mode de sélection** | aucun |

La barre d'outils a les boutons **Grouper**, **Dégrouper** et **Ignorer les groupes** à côté de **Creuser (carve)**. Le mode de sélection est aussi en haut du dock de droite (**Sélection :**), comme la barre de droite de Hammer.

## Grouper et dégrouper

- **Grouper** (`Ctrl+G`) place les objets sélectionnés dans un nouveau groupe. Les groupes entièrement sélectionnés sont imbriqués dans le nouveau groupe ; les autres objets sélectionnés quittent leur ancien groupe. Les objets groupés restent sélectionnés. Une étape d'annulation.
- **Dégrouper** (`Ctrl+U`) retire le groupe le plus extérieur de chaque objet sélectionné : ses membres directs et les groupes imbriqués passent au premier niveau. Une étape d'annulation.

Les groupes s'imbriquent aussi profondément que vous voulez. L'[Arborescence](outliner.md) les montre sous forme d'arbre (**Groupe #id**).

## Modes de sélection

Le mode de sélection décide ce qu'un clic sur un brush sélectionne :

| Mode | Un clic sélectionne | Infobulle |
| --- | --- | --- |
| **Groupes** (défaut) | Le groupe le plus extérieur en entier, ou l'entité de brush entière. | « Un clic sélectionne le groupe entier » |
| **Objets** | L'entité de brush entière, sans tenir compte des groupes. | « Un clic sélectionne l'entité entière, sans tenir compte des groupes » |
| **Solides** | Un seul solide, même dans une entité de brush. | « Un clic sélectionne un seul solide, même dans une entité » |

**Ignorer les groupes** (`Ctrl+W`, la touche de Hammer) passe en mode **Solides**, puis revient au mode d'avant quand vous appuyez à nouveau. Si vous aviez choisi **Solides** à la main, il revient à **Groupes**. Le mode est conservé d'une session à l'autre (`select_mode` dans la table `[edit]` de `settings.toml`, `Groups` par défaut).

## Autres endroits qui créent des groupes

- Les prefabs insérés sont groupés quand **Grouper les prefabs insérés** est activé (voir [Prefabs](prefabs.md)).
- **Dupliquer** et coller conservent les groupes entièrement copiés (voir [Presse-papiers](clipboard.md)).
- Une grille personnalisée peut être attachée à un groupe comme à des objets (voir [Grilles](grids.md)).

## Astuces

- Groupez les morceaux d'une construction en brushes qui fait office d'objet (un lampadaire, un bureau) pour la sélectionner d'un clic ; appuyez sur `Ctrl+W` un instant quand vous avez besoin d'un seul de ses brushes.
- Les membres masqués ne sont jamais sélectionnés avec leur groupe : Supprimer ou un déplacement ne touche que ce que vous voyez (voir [Masquer et isoler](hide-isolate.md)).
- Outils MCP : `list_groups`, `group_objects`, `ungroup_objects`, `select_group` (voir la [référence des outils MCP](../../MCP.md), en anglais).

## FAQ

**Je clique sur un brush et la moitié de la pièce est sélectionnée.** Il fait partie d'un groupe. Appuyez sur `Ctrl+W` (ou choisissez **Solides**) pour sélectionner les brushes un par un, ou dégroupez avec `Ctrl+U`.

**Quelle différence entre groupes et visgroups ?** Les groupes changent ce qu'un clic sélectionne ; les [visgroups](visgroups.md) changent ce qui est affiché. Un objet peut être dans un groupe et dans plusieurs visgroups.

**Puis-je grouper une entité de brush avec des brushes du monde ?** Oui. En mode **Groupes**, un clic sur l'un ou l'autre sélectionne tout le groupe.

## Pages liées

- [Outil de sélection](selection-tool.md)
- [Visgroups](visgroups.md)
- [Arborescence](outliner.md)
- [Référence des actions et raccourcis](../reference/actions-shortcuts.md)

---
[Retour à l'index de la documentation](../README.md)
