[Documentation](../README.md) > Fonctionnalités > Arborescence | [English](../../en/features/outliner.md) | [Русский](../../ru/features/outliner.md)

# Arborescence

L'onglet **Arborescence** montre la map sous forme d'arbre : le monde avec ses groupes et ses solides, et les entités rangées par classe. On peut y chercher, il suit la sélection, et il reste rapide même sur de très grosses maps (seules les lignes visibles sont dessinées).

## Accès

**Affichage > Panneaux > Arborescence** affiche ou masque l'onglet dans le dock de droite. Il est masqué par défaut (`ui.show_outliner`).

## L'arbre

| Nœud | Contient |
| --- | --- |
| **Monde** | Les groupes (**Groupe #id**, imbriqués comme dans la map) et les solides du monde (**Solide #id**). |
| **Entités** | Un nœud par classe (triées sans tenir compte de la casse), les entités de la classe dans l'ordre du fichier. Une entité de brush représente ses solides. |

La ligne du bas résume la map : « Solides du monde : N, entités : N, sélection : N ». Les nœuds **Monde** et **Entités** sont dépliés au départ.

## Commandes

| Commande | Effet |
| --- | --- |
| Champ de recherche | « Rechercher un nom, une classe, un id ou un matériau ». Les lignes restent quand elles ou un enfant correspondent, et les parents des résultats sont dépliés. Une classe qui correspond par son nom reste repliée. |
| Bouton flèche vers le bas | **Tout déplier**. |
| Bouton flèche vers le haut | **Tout replier** (garde **Monde** et **Entités** ouverts). |
| Œil d'une ligne | **Masquer** ou **Afficher** ce que la ligne représente (un objet, un groupe, une classe, le monde). Barré quand tout ce qu'il contient est masqué. |
| Flèche d'une ligne | La déplie ou la replie. |

## Sélectionner

| Geste | Effet |
| --- | --- |
| Clic sur une ligne | Sélectionne ses objets visibles (une ligne de groupe sélectionne ses membres, une ligne de classe ses entités). |
| `Ctrl`+clic | Ajoute la ligne à la sélection, ou la retire quand tous ses objets visibles sont déjà sélectionnés. |
| `Maj`+clic | Sélectionne la plage de lignes depuis la dernière ligne cliquée. |
| Double-clic | Sélectionne et centre les vues 2D et 3D sur la sélection. |
| `Haut` / `Bas` (pointeur sur la liste) | Sélectionne la ligne précédente ou suivante. |
| `Droite` | Déplie la ligne courante. |
| `Gauche` | Replie la ligne courante, ou remonte à son parent. |
| `Entrée` | Centre les vues sur la sélection. |

Les objets masqués ne sont jamais sélectionnés : une ligne qui ne contient que des objets masqués garde la sélection actuelle et affiche « Les objets masqués ne peuvent pas être sélectionnés : affichez-les d'abord (bouton œil) ». Cliquer sur une ligne ne prend pas le focus clavier : `Suppr`, `H` et les touches des outils continuent d'agir sur la sélection faite dans l'arborescence.

Quand vous sélectionnez des objets ailleurs, l'arborescence déplie les parents du premier objet sélectionné et défile jusqu'à lui.

## Astuces

- Cherchez un nom de matériau pour trouver les solides du monde qui l'utilisent.
- Cherchez un id (le numéro de **Solide #id**) pour aller au brush cité dans une erreur de compilation. **Map > Aller à un id de brush ou d'entité...** (`Ctrl+Maj+G`) le fait de partout (voir [Vérifier les problèmes](check-problems.md)).
- MCP : `list_entities`, `list_solids` et `list_groups` donnent les mêmes informations en texte (voir la [référence des outils MCP](../../MCP.md), en anglais).

## FAQ

**L'onglet n'est pas là.** Activez **Affichage > Panneaux > Arborescence**.

**Cliquer sur une ligne n'a rien sélectionné.** Tout ce qu'elle contient est masqué. Cliquez sur son œil pour l'afficher, puis recliquez sur la ligne.

**Pourquoi les flèches ne déplacent plus ma sélection dans la vue 2D ?** Quand le pointeur est sur l'arborescence, les flèches se déplacent dans la liste. Remettez le pointeur sur une vue.

## Pages liées

- [Groupes](groups.md)
- [Visgroups](visgroups.md)
- [Masquer et isoler](hide-isolate.md)
- [Tour de l'interface](../getting-started/interface-tour.md)

---
[Retour à l'index de la documentation](../README.md)
