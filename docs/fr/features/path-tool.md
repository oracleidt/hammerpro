[Documentation](../README.md) > Fonctionnalités > Outil chemin | [English](../../en/features/path-tool.md) | [Русский](../../ru/features/path-tool.md)

# Outil chemin

L'outil chemin trace des chaînes d'entités de chemin (`path_track`, `path_corner`, keyframes de cordes...) en cliquant dans les vues. Chaque nouveau point est nommé dans l'ordre et relié depuis le précédent, et toutes les chaînes de la map sont dessinées avec des flèches de direction pour que vous puissiez les modifier en déplaçant, insérant et supprimant des points.

## Ouvrir l'outil

- **Outils > Outil chemin**, ou son bouton dans la palette d'outils. Il n'a pas de raccourci par défaut : donnez-lui en un dans **Paramètres > Clavier** si vous vous en servez souvent (voir [Raccourcis clavier](keyboard-shortcuts.md)).
- Quand l'outil est actif, ses options s'affichent dans la barre d'options de l'outil et la barre d'état montre une courte aide.

## Tracer un chemin

1. Cliquez dans une vue pour poser le premier point.
   - Dans une vue 2D, le point tombe sur la grille. Sa profondeur (l'axe que la vue ne montre pas) vient du chemin en cours, sinon d'un point de chemin sélectionné, sinon du centre de la sélection, sinon du dernier point posé, sinon 0.
   - Dans la vue 3D, le point se pose sur la surface sous le curseur (le plan du sol s'il n'y en a pas), écarté de la surface par l'option **Hauteur** (le long de la normale de la surface).
   - Maintenez `Alt` pour poser le point hors grille.
2. Cliquez encore pour ajouter les points suivants. Un aperçu montre où le point va tomber et le nom qu'il recevra.
3. Terminez le chemin :
   - `Entrée` ou `Échap`, un double clic, le bouton **Terminer le chemin**, ou **Terminer le chemin** dans le menu du clic droit. Avec l'option **Boucle** cochée, terminer relie le dernier point au premier.
   - Cliquez sur un point de chemin existant pour y relier le dernier point et terminer le chemin. Cliquer sur le premier point du chemin le ferme en boucle.
   - **Fermer la boucle** (bouton ou menu du clic droit) ferme tout de suite le chemin en boucle, quelle que soit l'option **Boucle** (il faut au moins 2 points).
   - Passer à un autre outil termine aussi le chemin.

Chaque point nomme le suivant dans sa clé « suivant » : `target` pour `path_track`, `path_corner` et `path_corner_crash`, `NextKey` pour `keyframe_rope`, `move_rope`, `keyframe_track`, `move_track` et `move_keyframed`, `m_iszNextScript` pour `scripted_sequence`, `nextassaultpoint` pour `assault_assaultpoint`. Les nouveaux points reçoivent les keyvalues par défaut de leur classe dans le FGD. Mettez le nom du premier point dans le `target` d'un `func_tracktrain` pour que le train suive le chemin.

## Les noms

Les points sont nommés à partir du nom de base : `path` donne `path_1`, `path_2`... Chaque point prend le premier numéro libre après le précédent, les noms restent donc uniques dans la map. Si vous insérez un point après `path_2` et que les points suivants sont numérotés à la suite, le nouveau devient `path_3` et les suivants sont décalés d'un cran (`path_3` devient `path_4`...) : toutes les clés et sorties qui les nomment sont mises à jour.

## Modifier les chemins existants

Avec n'importe quel outil, les liens des entités de chemin dont la classe du FGD a une aide `line()` ou `cylinder()` (`path_track`, `path_corner`...) sont dessinés pour toutes les entités visibles dans les vues 2D et 3D, sélectionnées ou non, comme dans Hammer ; une aide `cylinder()` dessine aussi le cercle `radius` de chaque point sur ce point. Quand l'outil est actif, toutes les chaînes de la map sont dessinées avec des flèches de direction. Les chaînes de la sélection et du chemin en cours sont mises en évidence avec le nom des points. Les branches (`altpath` de `path_track`) sont dessinées d'une autre couleur.

| Geste | Effet |
| --- | --- |
| Clic sur un point | Le sélectionne (quand aucun chemin n'est en cours). |
| `Ctrl`+clic sur un point | L'ajoute à la sélection ou l'en retire. |
| Glisser un point | Le déplace. S'il fait partie de la sélection, tous les points sélectionnés bougent ensemble. Aligné sur la grille (`Alt` : libre). Une seule étape d'annulation. |
| `Maj`+glisser un point | Insère un nouveau point après lui, là où vous relâchez. Glisser depuis le dernier point d'une chaîne prolonge la chaîne, et l'outil continue le tracé depuis le nouveau point. |
| `Échap` pendant un glisser | Annule le déplacement. |
| `Suppr` | Avec **Relier à la suppression** coché, supprime les points de chemin sélectionnés et relie le point précédent de chacun au point suivant. |
| Clic droit | Menu contextuel : **Insérer un point de chemin après**, **Supprimer le point de chemin et relier**, **Terminer le chemin**, **Fermer la boucle du chemin**. |

Insérer un point sans glisser (bouton, menu ou menu contextuel) le place à mi-chemin entre le point sélectionné et son suivant, ou un segment plus loin si le point sélectionné termine sa chaîne. Si un point supprimé était le départ d'un train, le train part du point suivant. En bout de chaîne, le lien est simplement vidé.

## Options

Les options sont enregistrées dans `settings.toml`, table `[entities.path]`.

| Option | Effet | Par défaut |
| --- | --- | --- |
| **Classe** | Classe des nouveaux points. La liste contient les classes ponctuelles chaînables des FGD chargés (`path_track` et `path_corner` en premier), ou les classes de base citées plus haut si aucun FGD n'est chargé. L'infobulle indique la clé qui nomme le point suivant. | `path_corner` |
| **Nom** | Nom de base des points (120 caractères au plus). | `path` |
| **Boucle** | Terminer un chemin relie son dernier point au premier (un circuit fermé). | Décoché |
| **Relier à la suppression** | Supprimer des points avec cet outil relie le point précédent de chacun au point suivant. Décoché : `Suppr` supprime normalement. | Coché |
| **Hauteur** | Distance ajoutée au-dessus de la surface pour les points posés dans la vue 3D (-1024 à 1024 unités). | 0 |

La barre d'options contient aussi les boutons **Terminer le chemin**, **Fermer la boucle**, **Insérer un point** (il faut exactement un point de chemin sélectionné) et **Supprimer et relier** (il faut des points de chemin sélectionnés), et affiche le nombre de points du chemin en cours et le nombre de chemins de la map.

## Commandes des menus

Elles marchent depuis n'importe quel outil, sur les points de chemin sélectionnés :

| Commande | Menu | Raccourci par défaut |
| --- | --- | --- |
| **Outil chemin** | **Outils** | aucun |
| **Insérer un point de chemin après la sélection** | **Outils** | aucun |
| **Supprimer des points de chemin et relier** | **Outils** | aucun |

## Astuces

- Pour un circuit de train : choisissez `path_track`, cochez **Boucle** pour un circuit fermé, cliquez les points dans la vue de dessus, puis mettez `path_1` dans le `target` du `func_tracktrain`. Appuyez sur `F6` pour monter dessus dans la [vue joueur](player-view.md#trains) : il suit la voie comme en jeu (appuyez sur `T` pour le démarrer quand sa **Initial Speed** vaut `0`).
- Annuler retire les derniers points du chemin en cours, vous pouvez donc continuer à tracer après une annulation.
- Tout est aussi accessible aux assistants IA par MCP : `list_paths`, `create_path`, `insert_path_point`, `remove_path_points`, et `get_tool_options` / `set_tool_options` avec l'outil `path` (options `class`, `base_name`, `loop`, `relink`, `offset`). Voir la [référence des outils MCP](../../MCP.md) (en anglais).

## FAQ

**Ma classe n'est pas dans la liste Classe.** L'outil liste les classes ponctuelles dont le FGD a une clé « suivant » (`target`, `NextKey`, `m_iszNextScript` ou `nextassaultpoint`) vers laquelle la classe dessine une ligne, que le FGD décrit comme la suivante, ou que le type de la classe implique. Les entités de brush ne sont jamais listées.

**Pourquoi les noms de mes autres points ont changé ?** Vous avez inséré un point au milieu d'une suite numérotée. Les points suivants ont été renumérotés et toutes les références vers eux mises à jour, rien n'est cassé.

**Suppr a effacé mes points mais la chaîne est coupée.** **Relier à la suppression** était décoché, ou vous les avez supprimés avec un autre outil. Utilisez plutôt **Outils > Supprimer des points de chemin et relier**.

**Je peux poser des points sur un mur ?** Oui, dans la vue 3D le point se pose sur la surface sous le curseur, écarté d'elle par la **Hauteur** le long de la normale.

## Pages liées

- [Outil entité](entity-tool.md)
- [Propriétés des entités](entity-properties.md)
- [Entrées/sorties et vue logique](io-logical-view.md)
- [Grilles](grids.md)
- [Outil de sélection](selection-tool.md)

---
[Retour à l'index de la documentation](../README.md)
