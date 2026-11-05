[Documentation](../README.md) > Fonctionnalités > Annulation et historique | [English](../../en/features/undo-history.md) | [Русский](../../ru/features/undo-history.md)

# Annulation et historique

Chaque modification dans Hammer Pro est une étape d'annulation nommée : modifications des outils, commandes des menus, réglages qui changent la map, et modifications faites par les assistants IA via MCP. Vous pouvez les annuler et les rétablir une par une, sauter à n'importe quel point de l'historique et, par défaut, annuler aussi les changements de sélection.

## Accès

| Action | Menu | Raccourci par défaut |
| --- | --- | --- |
| **Annuler** | **Édition > Annuler** | `Ctrl+Z` |
| **Rétablir** | **Édition > Rétablir** | `Ctrl+Y` |
| **Historique des modifications** | **Affichage > Panneaux > Historique des modifications** | aucun |

La barre d'outils a les boutons **Annuler** et **Rétablir**. Le menu Édition nomme l'étape : **Annuler : Creusement**, **Rétablir : Déplacement**...

## Étapes d'annulation

- Chaque étape porte le nom de ce qu'elle a fait (« Creusement », « Collage », « Insertion de prefab », « Modification des cordons », « MCP : déplacement d'objets »...), dans la langue de l'interface.
- Les changements successifs du même type faits en moins de 1,5 seconde fusionnent en une étape (par exemple les mouvements d'un sélecteur de couleur).
- Les cordons et les positions de caméra enregistrées dans la map sont annulés avec les modifications.
- L'historique est propre à chaque map : chaque map ouverte a le sien.

## Annulation de la sélection

Avec **Annuler restaure la sélection** activé (par défaut), les changements de sélection sont aussi des étapes : `Ctrl+Z` rétablit la sélection précédente (faces de l'outil texture comprises) et `Ctrl+Y` la refait. Ces étapes s'appellent **Sélection** ou **Sélection de faces**. Elles ne marquent jamais la map comme modifiée, les changements d'un même geste (un clic, une sélection par rectangle, une touche) fusionnent en une étape, et ce sont les premières étapes supprimées quand l'historique est plein. Sélectionner ce qu'une modification vient de créer rejoint l'étape de cette modification.

Désactivez-le pour n'annuler que les modifications, comme dans Hammer.

## L'onglet Historique

**Affichage > Panneaux > Historique des modifications** ajoute l'onglet **Historique** au dock de droite (masqué par défaut, `workflow.show_history`) :

- la première ligne, **Ouverture de la map**, est la map telle qu'elle a été chargée ;
- puis les étapes faites, de la plus ancienne à la plus récente, l'étape courante en surbrillance ;
- puis les étapes annulées, en italique.

Cliquez sur une ligne pour y revenir ou y avancer (« Cliquez sur une étape pour y revenir ou y avancer. ») : Hammer Pro annule ou rétablit toutes les étapes entre les deux.

## Limites

| Option | Où | Effet | Défaut |
| --- | --- | --- | --- |
| **Niveaux d'annulation** (`edit.undo_levels`) | **Outils > Options... > Édition** | Nombre maximal d'étapes gardées, de 10 à 10000. | 500 |
| **Annuler restaure la sélection** (`edit.undo_restores_selection`) | **Outils > Options... > Édition** | Les changements de sélection sont des étapes d'annulation. | Activé |

L'historique garde aussi sa mémoire sous environ 512 Mo : les plus anciennes étapes sont supprimées d'abord, la plus récente est toujours gardée.

## Astuces

- Utilisez l'onglet Historique pour comparer avant et après : cliquez sur **Ouverture de la map**, regardez, puis cliquez sur la dernière étape pour revenir.
- Outils MCP : `undo`, `redo`, `undo_steps`, `redo_steps`, `get_history` (voir la [référence des outils MCP](../../MCP.md), en anglais). Chaque modification MCP est une étape « MCP : ... » que vous pouvez annuler vous-même.

## FAQ

**Ctrl+Z n'a changé que la sélection.** **Annuler restaure la sélection** est activé : la dernière étape était un changement de sélection. Appuyez à nouveau sur `Ctrl+Z`, ou désactivez l'option.

**Peut-on annuler après avoir enregistré ?** Oui. L'enregistrement ne vide pas l'historique.

**Pourquoi une ancienne étape a disparu ?** L'historique a atteint les **Niveaux d'annulation** ou sa limite de mémoire ; les plus anciennes étapes partent en premier.

## Pages liées

- [Outil de sélection](selection-tool.md)
- [Presse-papiers](clipboard.md)
- [Sauvegarde automatique et récupération](autosave-crash.md)
- [Référence des paramètres](../reference/settings.md)

---
[Retour à l'index de la documentation](../README.md)
