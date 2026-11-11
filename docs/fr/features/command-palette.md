[Documentation](../README.md) > Fonctionnalités > Palette de commandes | [English](../../en/features/command-palette.md) | [Русский](../../ru/features/command-palette.md)

# Palette de commandes

La palette de commandes lance n'importe quelle commande de Hammer Pro en tapant quelques lettres de son nom : toutes les actions des menus, tous les outils, les commandes qui n'ont pas de menu, vos maps récentes et les langues de l'interface.

## L'ouvrir

**Outils > Palette de commandes...** ou `Ctrl+P`. La palette s'ouvre par-dessus la fenêtre avec le champ de recherche prêt (*Tapez une commande, un outil ou une map...*).

## L'utiliser

| Touche | Effet |
| --- | --- |
| Taper | Filtre la liste par recherche approchée. |
| `Haut` / `Bas` | Déplace la surbrillance. |
| `Page préc.` / `Page suiv.` | Déplace la surbrillance d'une page. |
| `Entrée` | Lance l'entrée en surbrillance. |
| `Échap` | Ferme la palette. |

Un clic sur une entrée la lance aussi. Le pied de la palette rappelle les touches (*naviguer, exécuter, fermer*). Quand rien ne correspond, la palette affiche **Aucun résultat**. 200 résultats au plus sont listés.

Chaque ligne affiche une icône, le nom, l'endroit où se trouve la commande (son chemin de menu, par exemple **Fichier > Importer** ou **Outils > Disposer**, **Outil** pour les outils, **Maps récentes** pour une map, **Affichage** pour une langue) et son raccourci s'il en a un. Les commandes impossibles à lancer pour l'instant (rien de sélectionné, par exemple) sont grisées et ne font rien.

## Ce qu'on y trouve

| Entrée | Trouvée par |
| --- | --- |
| Toutes les actions de Hammer Pro | Leur nom dans la langue de l'interface, leur nom anglais, ou leur id avec des espaces (`toggle grid`). Y compris les commandes qui ne sont dans aucun menu, comme **Gizmo en repère local** ou **Créer un brush hint**. |
| Tous les outils | Leur nom (par exemple **Outil de découpe**). |
| **Ouvrir : nom de la map** | Une entrée par map récente : son nom de fichier, son chemin complet, ou `ouvrir` (`open` en anglais). |
| **Langue : English** / **Langue : Français** | `langue`, `language`, le nom de la langue, ou `lang en` / `lang fr`. |

Avec une recherche vide, la palette liste d'abord les six commandes lancées le plus récemment (pendant cette session), puis tout le reste.

## Astuces

- La palette est le moyen le plus rapide d'atteindre une commande sans raccourci : tapez 3 ou 4 lettres et appuyez sur `Entrée`.
- Les noms anglais marchent toujours, même avec l'interface en français : un tutoriel en anglais reste utile.
- Pour donner un raccourci à une commande, voir [Raccourcis clavier](keyboard-shortcuts.md).

## FAQ

**Une commande est grisée dans la palette.** Elle ne peut pas se lancer dans l'état actuel, comme son entrée de menu. Sélectionnez quelque chose ou ouvrez d'abord une map.

**Les commandes récentes sont-elles gardées après un redémarrage ?** Non, la liste des commandes récentes dure le temps de la session.

**Puis-je chercher par raccourci ?** Pas dans la palette ; la référence des raccourcis (`F1`) cherche par raccourci.

## Pages liées

- [Raccourcis clavier](keyboard-shortcuts.md)
- [Actions et raccourcis](../reference/actions-shortcuts.md)
- [Visite de l'interface](../getting-started/interface-tour.md)

---
[Retour à l'index de la documentation](../README.md)
