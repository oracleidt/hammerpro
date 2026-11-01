[Documentation](../README.md) > Fonctionnalités > Transformer et aligner | [English](../../en/features/transform-align.md)

# Transformer et aligner

Les commandes qui déplacent, pivotent, mettent à l'échelle, retournent, alignent ou posent les objets sélectionnés selon des règles précises, sans glisser de poignées : la fenêtre Transformer, l'alignement, le retournement, l'alignement sur la grille, poser au sol / coller au plafond, centrer les origines, la rotation Z aléatoire, le pivot déplaçable, le placement physique et les copies en série (Collage spécial).

Chaque commande ci-dessous agit sur la sélection actuelle et compte pour une seule étape d'annulation. La plupart se trouvent dans le menu **Outils** (l'alignement et le retournement ont leurs sous-menus **Outils > Aligner** et **Outils > Retourner**). Pour déplacer les objets à la souris, voyez l'[outil de sélection](selection-tool.md).

## Résumé des commandes

| Commande | Menu | Raccourci par défaut |
|---|---|---|
| **Transformer...** | Outils | `Ctrl+M` |
| **Aligner la sélection sur la grille** | Outils | `Ctrl+B` |
| **Aligner chaque objet sur la grille** | Outils | `Ctrl+Maj+B` |
| **Poser au sol** | Outils | `Alt+N` |
| **Coller au plafond** | Outils | `Alt+U` |
| **Poser au sol, aligné sur la surface** | Outils | `Ctrl+Alt+N` |
| **Coller au plafond, aligné sur la surface** | Outils | `Ctrl+Alt+U` |
| **Centrer les origines** | Outils | aucun |
| **Rotation Z aléatoire** | Outils, et le menu du clic droit des vues | aucun |
| **Options de rotation Z aléatoire...** | Outils (ouvre la fenêtre Transformer en mode **Z aléatoire**) | aucun |
| **Pivot déplaçable** | Outils | aucun |
| **Placement physique (chute et pose)** | Outils | aucun |
| **Retourner X**, **Retourner Y**, **Retourner Z** | Outils > Retourner | aucun |
| **Retourner horizontalement** | Outils > Retourner | `Ctrl+L` |
| **Retourner verticalement** | Outils > Retourner | aucun (`Ctrl+I` avec les préréglages de touches Hammer et Hammer++) |
| **Aligner à gauche**, **Aligner à droite**, **Aligner en haut**, **Aligner en bas** | Outils > Aligner | aucun |
| **Centrer horizontalement**, **Centrer verticalement** | Outils > Aligner | aucun |
| **Collage spécial...** | Édition | `Ctrl+Maj+V` |
| **Verrouillage des textures** | Outils | `Maj+L` |
| **Verrouillage de l'échelle des textures** | Outils | `Maj+K` |

Les commandes sans raccourci peuvent en recevoir un dans **Paramètres > Clavier** (voir [Raccourcis clavier](keyboard-shortcuts.md)) ou se lancer depuis la [palette de commandes](command-palette.md) (`Ctrl+P`).

## La fenêtre Transformer

**Outils > Transformer...** (`Ctrl+M`) ouvre la fenêtre **Transformer** : déplacer, pivoter ou mettre à l'échelle la sélection avec des valeurs tapées au clavier. La fenêtre garde ses valeurs pendant toute la session, vous pouvez donc appliquer plusieurs fois la même transformation.

| Champ | Effet | Par défaut |
|---|---|---|
| **Déplacer** / **Pivoter** / **Mettre à l'échelle** / **Z aléatoire** | Boutons de mode en haut de la fenêtre. **Z aléatoire** est décrit dans [Rotation Z aléatoire](#rotation-z-aléatoire). | **Déplacer** |
| **Décalage** (Déplacer) | Distance X, Y, Z en unités ajoutée à la sélection. | 0 0 0 |
| **Angles (degrés)** (Pivoter) | Rotation autour de X, Y et Z, appliquée d'abord sur Z, puis Y, puis X (le même ordre que le Collage spécial). | 0 0 0 |
| **Facteurs** (Mettre à l'échelle) | Facteur d'échelle sur X, Y et Z (pas de glissement 0,01). Un facteur nul est refusé (**Les facteurs d'échelle ne peuvent pas être nuls**). Un facteur négatif fait un miroir. | 1 1 1 |
| **Pivot** (Pivoter et Mettre à l'échelle seulement) | **Centre de la sélection** : centre de la boîte englobante de la sélection. **Origine du monde** : le point 0 0 0. **Pivot déplaçable** : la croix du pivot (voir plus bas) ; le bouton est grisé tant que le pivot est masqué, et son infobulle donne la position du pivot. | **Centre de la sélection** |
| **Verrouillage des textures** | Indique si le verrouillage des textures est actif ; **Basculer** l'active ou le coupe (comme `Maj+L`). | suit le réglage |
| **Réinitialiser** | Remet les valeurs du mode actuel à leur défaut (0 ou 1). | |
| **Appliquer** | Applique la transformation (aussi `Entrée`). Grisé si rien n'est sélectionné. | |
| **Fermer** | Ferme la fenêtre (aussi `Échap`). | |

La ligne sous les champs compte les objets sélectionnés. Les erreurs (rien de sélectionné, valeurs invalides, échec de la transformation) s'affichent en rouge dans la fenêtre.

## Aligner sur la grille

- **Aligner la sélection sur la grille** (`Ctrl+B`) : déplace la sélection d'un bloc pour que le coin minimum de sa boîte englobante tombe sur le point le plus proche de la grille actuelle.
- **Aligner chaque objet sur la grille** (`Ctrl+Maj+B`) : fait la même chose pour chaque objet sélectionné, séparément.

La taille utilisée est la taille de grille actuelle (voir [Grilles](grids.md)).

## Poser au sol et coller au plafond

Le « drop » façon Hammer++ :

- **Poser au sol** (`Alt+N`) descend chaque objet tout droit jusqu'à ce que sa boîte englobante touche la surface la plus proche.
- **Coller au plafond** (`Alt+U`) le monte tout droit.
- Les variantes **aligné sur la surface** (`Ctrl+Alt+N`, `Ctrl+Alt+U`) tournent en plus les entités ponctuelles pour que leur axe vertical suive la normale de la surface touchée, en gardant au mieux leur orientation.

En détail :

- Chaque groupe (son groupe le plus extérieur) ou objet hors groupe bouge séparément.
- Les rayons partent de la mi-hauteur de l'objet, au-dessus des quatre coins du bas et du centre : un objet enfoncé dans le sol remonte dessus au lieu de passer à travers.
- Les brushes et les props (entités avec un modèle `.mdl`) servent de surfaces. Les objets déplacés eux-mêmes sont ignorés.
- Un objet sans rien en dessous (ou au-dessus) reste en place. Si rien ne bouge, le message **Aucune surface sous la sélection** (ou au-dessus) s'affiche.

## Placement physique

**Outils > Placement physique (chute et pose)** fait tomber les objets sélectionnés l'un après l'autre, le plus bas en premier : chacun se pose sur le sol ou sur les objets déjà placés au lieu de les traverser, comme sous l'effet de la gravité. Un groupe tombe d'un bloc. Les props sont inclinés sur la surface où ils atterrissent. Quand tout repose déjà sur une surface, l'avis **Rien à faire tomber : la sélection repose déjà sur une surface.** s'affiche.

## Retourner

Le retournement fait un miroir de la sélection par rapport à un plan qui passe par le pivot déplaçable s'il est affiché, sinon par le centre de la sélection.

- **Retourner X**, **Retourner Y**, **Retourner Z** font le miroir sur cet axe du monde.
- **Retourner horizontalement** (`Ctrl+L`) et **Retourner verticalement** dépendent de la vue sous le pointeur de la souris (sinon la dernière vue utilisée). Dans une vue 2D, ils retournent sur l'axe horizontal ou vertical de cette vue. Dans la vue 3D, horizontalement retourne sur l'axe du monde (X ou Y) le plus proche de la droite de la caméra, et verticalement sur Z.

Les brushes gardent des faces tournées vers l'extérieur et leurs displacements sont retournés avec eux. Le verrouillage des textures s'applique.

## Aligner

L'alignement façon Hammer : les objets sélectionnés s'alignent sur le **dernier objet sélectionné**, qui ne bouge pas. Il faut au moins deux objets (**Sélectionnez au moins deux objets**).

| Commande | Aligne |
|---|---|
| **Aligner à gauche** | le minimum de la boîte englobante sur l'axe horizontal de la vue |
| **Aligner à droite** | le maximum sur l'axe horizontal |
| **Aligner en haut** | le maximum sur l'axe vertical |
| **Aligner en bas** | le minimum sur l'axe vertical |
| **Centrer horizontalement** | les centres sur l'axe horizontal |
| **Centrer verticalement** | les centres sur l'axe vertical |

Les axes sont ceux de la vue 2D sous le pointeur (ou de la dernière utilisée). Depuis la vue 3D, ce sont les axes de la vue de dessus. Les autres objets bougent ensemble, d'un seul bloc.

## Centrer les origines

**Outils > Centrer les origines** place la clé `origin` des entités de brush sélectionnées (`func_door_rotating`, `func_rotating`...) au centre de leurs brushes. Seules les entités de brush qui ont une clé `origin` changent ; sélectionner un de leurs brushes suffit. Si aucune n'a changé, l'avis **Aucune entité de brosses sélectionnée n'a d'origine à centrer** s'affiche.

## Rotation Z aléatoire

**Outils > Rotation Z aléatoire** tourne chaque entité ponctuelle sélectionnée qui a une clé `angles` (`prop_static`, `prop_dynamic`, `prop_physics`, lumières, toute entité ponctuelle avec des angles) autour de Z d'une quantité aléatoire qui lui est propre, autour de sa propre origine. C'est une seule étape d'annulation (**Rotation aléatoire**). Les brushes, les entités de brush et les entités sans clé `angles` sont ignorés ; l'avis indique combien ont tourné et combien ont été ignorés, par exemple **Rotation Z aléatoire : 12 tournés, 3 ignorés (brosses, entités de brosses ou sans clé angles)**.

Les options sont dans le mode **Z aléatoire** de la fenêtre Transformer (**Outils > Options de rotation Z aléatoire...**), gardées dans les paramètres :

| Champ | Effet | Par défaut |
|---|---|---|
| **Plage de lacet** | La rotation ajoutée au lacet de chaque objet est tirée entre ces deux valeurs, en degrés. | 0 à 360 |
| **Pas d'arrondi** | **Libre**, **15°**, **45°**, **90°** ou tout pas jusqu'à 180 : la rotation est un multiple du pas (avec 0 à 360 et 90, chaque objet regarde dans l'une des quatre directions). | Libre |
| **Variation de tangage et roulis** | Incline aussi le tangage et le roulis d'une quantité aléatoire jusqu'à ce nombre de degrés (0 à 45). | désactivé, 3° |
| **Réinitialiser** | Remet ces options à leurs valeurs par défaut. | |
| **Appliquer** | Tourne la sélection (aussi `Entrée`) ; la fenêtre indique combien ont tourné. | |

Les options sont les clés `random_yaw_min`, `random_yaw_max`, `random_yaw_snap`, `random_tilt` et `random_tilt_deg` de la table `[selection]` (voir la [référence des paramètres](../reference/settings.md)).

## Pivot déplaçable

**Outils > Pivot déplaçable** affiche une croix de pivot au centre de la sélection (à l'origine d'une entité ponctuelle seule), ou la masque si elle est déjà affichée.

- Avec l'[outil de sélection](selection-tool.md), glissez la croix dans une vue 2D pour la déplacer (elle s'aligne sur la grille quand le magnétisme est actif). Dans la vue 3D, le gizmo est posé sur le pivot : attrapez-le près du centre du gizmo.
- Les rotations et mises à l'échelle des poignées, du gizmo et des transformations modales tournent autour de lui. Le retournement passe par lui. La fenêtre Transformer l'utilise avec **Pivot > Pivot déplaçable**, et le Collage spécial avec **Pivoter autour du pivot**.
- Le pivot suit les objets quand vous les déplacez. Il disparaît quand l'ensemble sélectionné change (comme dans Hammer++).

## Copies en série (Collage spécial)

**Édition > Collage spécial...** (`Ctrl+Maj+V`) colle plusieurs copies du presse-papiers, chacune décalée et tournée un peu plus que la précédente : escaliers, colonnades, objets en cercle. Les champs sont **Copies** (jusqu'à 10 000, avec un avertissement au-delà de 1 000), **Décalage par copie** (64 0 0 par défaut, avec des boutons **<** qui prennent la taille du presse-papiers sur un axe), **Rotation par copie**, **Pivoter autour du pivot**, **Commencer à la position d'origine**, **Grouper chaque copie**, **Rendre les noms d'entités uniques** et **Préfixe de nom**. La description complète est sur la page [Couper, copier et coller](clipboard.md).

## Verrouillage des textures et de leur échelle

- **Verrouillage des textures** (`Maj+L`, réglage `texture_lock`, actif par défaut) : les textures suivent les brushes quand vous les déplacez, pivotez, mettez à l'échelle ou retournez. Désactivé, les textures restent fixes dans le monde.
- **Verrouillage de l'échelle des textures** (`Maj+K`, réglage `texture_scale_lock`, désactivé par défaut) : mettre un brush à l'échelle met aussi sa texture à l'échelle. Ne sert qu'avec le verrouillage des textures actif ; sans lui, la texture garde son échelle.

Les deux se trouvent aussi dans **Paramètres > Édition > Comportement** et sont enregistrés dans la table `[edit]` de `settings.toml`.

## MCP

Les mêmes opérations existent pour les assistants IA : `transform_objects`, `move_objects`, `rotate_objects`, `resize_objects`, `snap_objects_to_grid`, `drop_to_surface`, `flip_objects`, `align_objects`, `center_entity_origins`, `random_rotate_objects` (rotation Z aléatoire, avec sa plage, son pas, son inclinaison et sa graine), `array_duplicate` et `run_action` (par exemple `physics_placement` ou `toggle_pivot`). Voir la [référence des outils MCP](../../MCP.md) (en anglais).

## Astuces

- Pour faire pivoter une porte autour de ses gonds, affichez le **Pivot déplaçable**, glissez-le sur les gonds, puis tournez avec les poignées ou avec **Transformer...** réglé sur **Pivot déplaçable**.
- L'alignement prend le dernier objet sélectionné comme référence : faites Ctrl+clic sur la référence en dernier.
- **Poser au sol** sur un groupe de props le déplace d'un bloc : une table et les objets posés dessus restent assemblés.
- **Aligner chaque objet sur la grille** remet d'un coup beaucoup de props hors grille en place, sans tous les décaler de la même distance.
- Après avoir placé une rangée de rochers, de caisses ou d'arbres identiques, la **Rotation Z aléatoire** leur donne un air moins copié ; réglez le **Pas d'arrondi** sur 90° pour des caisses qui doivent rester d'équerre avec les murs.

## FAQ

**Pourquoi Pivot déplaçable est grisé dans la fenêtre Transformer ?** Le pivot est masqué. Utilisez d'abord **Outils > Pivot déplaçable**.

**Mon pivot a disparu.** Il disparaît quand la sélection change. Réaffichez-le après avoir sélectionné.

**L'alignement ne fait rien.** Sélectionnez au moins deux objets, et vérifiez que la référence (le dernier objet sélectionné) n'est pas déjà alignée avec les autres sur cet axe.

**Poser au sol n'a pas bougé un prop.** Il n'y avait rien en dessous. Seuls les brushes et les props (entités avec un modèle `.mdl`) comptent comme surfaces ; les autres entités ponctuelles non.

## Pages liées

- [Outil de sélection](selection-tool.md)
- [Couper, copier et coller](clipboard.md)
- [Opérations sur les brushes](brush-operations.md)
- [Grilles](grids.md)
- [Groupes](groups.md)
- [Application de texture](texture-tool.md)
- [Actions et raccourcis](../reference/actions-shortcuts.md)
- [Référence des paramètres](../reference/settings.md)

---
[Retour à l'index de la documentation](../README.md)
