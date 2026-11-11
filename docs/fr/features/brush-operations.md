[Documentation](../README.md) > Fonctionnalités > Opérations sur les brushes | [English](../../en/features/brush-operations.md) | [Русский](../../ru/features/brush-operations.md)

# Opérations sur les brushes

Les commandes qui modifient les brushes en entier : creuser (carve), rendre creux, fusionner, lier des brushes à une entité et les renvoyer au monde, convertir en `func_detail`, nodraw sur les faces cachées, brushes hint, et les verrouillages qui décident comment les textures suivent les transformations.

Toutes sont dans le menu **Outils** (sauf **Créer un brush hint**, disponible seulement dans la [palette de commandes](command-palette.md)) et agissent sur la sélection actuelle. Chacune compte pour une seule étape d'annulation.

## Résumé des commandes

| Commande | Raccourci par défaut | Il faut |
|---|---|---|
| **Creuser (carve)** | `Ctrl+Maj+C` | un ou plusieurs brushes |
| **Rendre creux** | `Ctrl+H` | un ou plusieurs brushes |
| **Fusionner les brushes** | `Ctrl+Maj+M` | au moins deux brushes |
| **Fusionner en un solide convexe** | aucun | au moins deux brushes |
| **Intersection de brushes** | `Ctrl+Maj+K` | au moins deux brushes |
| **Union de brushes** | aucun | au moins deux brushes |
| **Lier à une entité** | `Ctrl+T` | une sélection |
| **Lier chacun à sa propre entité** | `Ctrl+Maj+T` | une sélection avec des brushes du monde |
| **Renvoyer au monde** | `Ctrl+Maj+W` | des brushes d'entités de brush |
| **Convertir en func_detail** | aucun | une sélection |
| **Nodraw sur les faces cachées...** | aucun | rien (parcourt toute la map) |
| **Créer un brush hint** | aucun (palette de commandes) | des faces sélectionnées avec l'outil de texture |
| **Verrouillage des textures** | `Maj+L` | |
| **Verrouillage de l'échelle des textures** | `Maj+K` | |

Quand une commande demande des brushes et qu'aucun n'est sélectionné, l'avis **Sélectionnez d'abord un ou plusieurs brushes** (ou **Sélectionnez au moins deux brushes**) s'affiche. Les raccourcis se changent dans **Paramètres > Clavier** (voir [Raccourcis clavier](keyboard-shortcuts.md)).

## Creuser (carve)

**Outils > Creuser (carve)** (`Ctrl+Maj+C`) soustrait les brushes sélectionnés des brushes du monde qu'ils traversent, comme le Carve de Hammer.

- Les brushes sélectionnés (ceux qui creusent) ne sont pas modifiés. Gardez-les ou supprimez-les ensuite.
- Seuls les brushes du monde visibles sont découpés : les brushes d'entités et les brushes masqués ne sont pas touchés.
- Les brushes découpés sont divisés en plusieurs morceaux convexes avec de nouveaux ids de faces. Les listes de faces des overlays et des `env_cubemap` suivent : une face découpée en morceaux est remplacée par tous ses morceaux, une face entièrement creusée est retirée de la liste.
- Les brushes sont découpés en parallèle sur tous les cœurs du processeur : creuser à travers des centaines de brushes d'un coup reste rapide.

Le carve crée beaucoup de petits brushes. Pour une simple ouverture, l'[outil extruder / creuser](extrude-tool.md) (un contour cliqué sur une face, creusé dans ce seul brush, avec le moins de morceaux possible), l'[outil de découpe](clip-tool.md) ou quelques blocs donnent souvent une géométrie plus propre.

## Rendre creux

**Outils > Rendre creux** (`Ctrl+H`) transforme chaque brush sélectionné en murs. Une petite fenêtre demande l'**Épaisseur des murs** :

| Champ | Effet | Par défaut |
|---|---|---|
| **Épaisseur des murs** | Épaisseur des murs en unités, de -4096 à 4096. Une valeur négative creuse vers l'extérieur : les murs sont construits autour du brush, et le volume d'origine devient la pièce intérieure. | 32 |
| **OK** / `Entrée` | Rend la sélection creuse. | |
| **Annuler** / `Échap` | Ferme sans rien changer. | |

Le champ d'épaisseur a le focus clavier à l'ouverture : tapez la valeur puis `Entrée`. Les murs de chaque brush du monde sont groupés (comme dans Hammer) et sélectionnés. Si un brush est trop petit pour l'épaisseur demandée, rien ne change et une erreur s'affiche. Les listes de faces des overlays et des cubemaps passent sur les faces extérieures des murs.

## Fusionner les brushes

**Outils > Opérations sur les brushes > Fusionner les brushes** (`Ctrl+Maj+M`) réunit les brushes sélectionnés en un seul, uniquement si leur union est déjà convexe (le « Merge brushes » de Hammer++). Chaque face garde la texture de la face dont elle vient.

- Si l'union n'est pas convexe, rien ne change et un avertissement propose **Fusionner en un solide convexe**. Le contrôle est exact à 0,02 unité près : une marche ou une encoche, même d'une seule unité, n'est jamais comblée.
- Les brushes doivent tous être des brushes du monde ou appartenir à la même entité, sinon l'avertissement **Les brushes doivent tous être des brushes du monde ou appartenir à la même entité** s'affiche.

## Fusionner en un solide convexe

**Outils > Opérations sur les brushes > Fusionner en un solide convexe** enveloppe les brushes sélectionnés dans leur enveloppe convexe, même si cela ajoute du volume. Le résultat remplace le premier brush (même id, même propriétaire, mêmes données d'éditeur) et les autres sont supprimés.

## Intersection de brushes

**Outils > Opérations sur les brushes > Intersection de brushes** (`Ctrl+Maj+K`) ne garde que le volume commun à tous les brushes sélectionnés. Le résultat est un brush convexe qui remplace le premier brush sélectionné (même id, même propriétaire, mêmes données d'éditeur) ; les autres sont supprimés.

- Les faces conservées gardent leur texture. Les nouvelles faces de coupe prennent la texture de la face qui les a coupées, comme Carve.
- Si les brushes ne se chevauchent pas (un simple contact ne suffit pas), rien ne change et l'avertissement **Les brushes ne se chevauchent pas : il n'y a aucun volume commun à conserver** s'affiche.
- Les brushes doivent tous être des brushes du monde ou appartenir à la même entité, sinon l'avertissement **Les brushes doivent tous être des brushes du monde ou appartenir à la même entité** s'affiche.

## Union de brushes

**Outils > Opérations sur les brushes > Union de brushes** réunit les brushes sélectionnés en le moins possible de brushes convexes qui couvrent exactement leur union, sans ajouter de volume (contrairement à **Fusionner en un solide convexe**).

1. Les brushes qui se chevauchent sont découpés pour ne plus se recouvrir, et les parties cachées à l'intérieur d'un autre brush sont retirées.
2. Les voisins en contact sont refusionnés en un seul brush dès que leur union est convexe (exactement, comme pour **Fusionner les brushes** : une marche d'une unité est conservée).

Deux boîtes qui se chevauchent dans l'axe donnent donc une seule boîte, tandis qu'un L reste deux brushes qui se touchent sans se chevaucher. Chaque brush garde son id tant qu'il a encore un morceau ; ceux entièrement à l'intérieur d'autres brushes sont supprimés. Les nouveaux morceaux gardent le propriétaire et les données d'éditeur du brush dont ils viennent, et sont sélectionnés si lui l'était. Quand rien ne peut être joint, l'avertissement **Rien à joindre** s'affiche. La même règle de propriétaire que **Fusionner les brushes** s'applique. Une seule étape d'annulation. Le découpage tourne sur tous les cœurs du processeur et les paires qui ne peuvent jamais former un brush convexe sont écartées tout de suite : l'union de plusieurs centaines de brushes prend une fraction de seconde.

## Lier à une entité

**Outils > Lier à une entité** (`Ctrl+T`) se comporte comme dans Hammer :

- Avec exactement une entité de brush dans la sélection plus d'autres brushes, ces brushes sont ajoutés à cette entité, qui garde sa classe et ses clés.
- Avec une seule entité de brush sélectionnée, ses propriétés s'ouvrent.
- Sinon, la sélection est liée à une nouvelle entité de brush de la classe **Entité de brush par défaut** (réglage `default_brush_entity`, `func_detail` par défaut, dans **Paramètres > Édition > Nouveaux objets**), avec les keyvalues par défaut de cette classe dans le FGD.
- Quand la classe du FGD a une clé `origin` (`func_tracktrain`, `func_tanktrain`, `func_rotating`, `func_door_rotating`...), la nouvelle entité la reçoit au centre de ses brushes, comme dans Hammer. Le jeu déplace et tourne l'entité autour de ce point : un train le pose sur ses points `path_track` (relevé de sa clé `height`). **Outils > Disposer > Centrer les origines** la remet au centre plus tard.

Les brushes pris à d'autres entités de brush passent dans l'entité, et les entités de brush qui n'ont plus de brush sont supprimées. L'entité est sélectionnée et le panneau **Propriétés de l'entité** s'ouvre (voir [Propriétés des entités](entity-properties.md)).

## Lier chacun à sa propre entité

**Outils > Opérations sur les brushes > Lier chacun à sa propre entité** (`Ctrl+Maj+T`, comme Hammer++) place chaque brush du monde sélectionné dans sa propre nouvelle entité, de la classe d'entité de brush par défaut. Les brushes qui appartiennent déjà à une entité ne sont pas touchés. Les nouvelles entités sont sélectionnées et le panneau d'entité s'ouvre.

## Renvoyer au monde

**Outils > Renvoyer au monde** (`Ctrl+Maj+W`) renvoie les brushes des entités de brush sélectionnées dans le monde. Les entités de brush qui n'ont plus de brush sont supprimées. Les brushes déplacés sont sélectionnés.

## Convertir en func_detail

**Outils > Opérations sur les brushes > Convertir en func_detail** lie la sélection à un nouveau `func_detail`, quelle que soit la classe d'entité de brush par défaut. Si la sélection est déjà un seul `func_detail`, l'avis **La sélection est déjà un func_detail** s'affiche. Pour trouver les brushes à convertir, voyez **Map > Fuites et visibilité > Suggestions de func_detail...** dans [Vérifier les problèmes](check-problems.md).

## Nodraw sur les faces cachées

**Outils > Opérations sur les brushes > Nodraw sur les faces cachées...** cherche dans toute la map les faces de brush entièrement couvertes par d'autres brushes, qui ne peuvent jamais être vues.

1. La recherche tourne en arrière-plan (**Recherche des faces cachées...** dans la console).
2. Les faces trouvées sont sélectionnées et l'outil d'[application de texture](texture-tool.md) devient actif pour que vous puissiez les vérifier dans les vues.
3. Une fenêtre indique combien de faces ont été trouvées. **Appliquer nodraw** leur donne `tools/toolsnodraw`, **Annuler** ferme la fenêtre.

Règles de la recherche :

- Les candidates sont les faces des brushes visibles du monde et des `func_detail` qui ne sont pas des displacements et n'ont pas déjà une texture outil.
- Les brushes qui cachent sont les brushes visibles du monde sans displacement et sans matériau transparent. Les textures outils autres que nodraw, black et skybox comptent comme transparentes, tout comme les matériaux dont le nom contient glass, water, window, fence, grate, chainlink, translucent ou alpha.
- Un `func_detail` ne cache jamais rien : les faces entre deux `func_detail` ne sont pas touchées.

Si un brush concerné a changé entre la recherche et **Appliquer nodraw**, rien n'est appliqué et le message **La map a changé depuis la recherche : relancez-la** s'affiche.

## Créer un brush hint

**Créer un brush hint** (palette de commandes uniquement) transforme des brushes en brushes hint : sélectionnez avec l'outil d'[application de texture](texture-tool.md) les faces qui doivent devenir des faces hint, puis lancez la commande. Ces faces reçoivent `TOOLS/TOOLSHINT` et toutes les autres faces de leurs brushes `TOOLS/TOOLSSKIP`. Sans face sélectionnée, l'avis **Sélectionnez d'abord les faces qui deviennent des faces hint avec l'outil de texture** s'affiche. La console indique combien de brushes hint ont été créés.

## Verrouillage des textures et de leur échelle

| Option | Effet | Par défaut |
|---|---|---|
| **Verrouillage des textures** (`Maj+L`, `texture_lock`) | Les textures suivent les brushes quand vous les déplacez, pivotez, mettez à l'échelle ou retournez. Désactivé, les textures restent fixes dans le monde (et sont remises dans l'alignement du monde quand une face tournée ne peut plus les utiliser). | actif |
| **Verrouillage de l'échelle des textures** (`Maj+K`, `texture_scale_lock`) | Mettre un brush à l'échelle met aussi sa texture à l'échelle. Ne sert qu'avec le verrouillage des textures actif. Sans lui, la texture garde son échelle et reste ancrée au centre du brush. | désactivé |

Les deux sont dans le menu **Outils**, dans **Paramètres > Édition > Comportement**, et la fenêtre Transformer affiche le verrouillage des textures avec un bouton **Basculer** (voir [Transformer et aligner](transform-align.md)). Ils sont enregistrés dans la table `[edit]` de `settings.toml`.

## MCP

Pour les assistants IA : `carve`, `hollow`, `merge_brushes` (`convex_hull` à true pour Fusionner en un solide convexe), `intersect_brushes`, `union_brushes`, `tie_to_entity`, `move_to_world`, `make_hint_brushes`, `find_hidden_faces`, `apply_nodraw` et `run_action` pour les autres commandes. Voir la [référence des outils MCP](../../MCP.md) (en anglais).

## Astuces

- Mettez une épaisseur négative dans **Rendre creux** pour transformer un bloc tracé à la taille d'une pièce en cette pièce, avec ses murs à l'extérieur.
- Lancez **Nodraw sur les faces cachées...** avant une compilation finale : les faces cachées coûtent du rendu et de la place de lightmap pour rien.
- `Ctrl+T` avec une entité de brush existante et de nouveaux brushes sélectionnés les ajoute à cette entité : pratique pour agrandir une porte ou un `func_detail`.
- Réglez **Entité de brush par défaut** sur la classe que vous utilisez le plus (par exemple `func_brush`) pour que `Ctrl+T` et `Ctrl+Maj+T` la créent directement.

## FAQ

**Le carve n'a rien fait sur mon func_detail.** Le carve ne découpe que les brushes du monde. Faites d'abord **Renvoyer au monde**, creusez, puis liez à nouveau les morceaux.

**Fusionner les brushes dit que la forme n'est pas convexe.** Les brushes fusionnés doivent former un brush convexe. Utilisez **Fusionner en un solide convexe** si l'ajout de volume ne vous gêne pas, ou gardez des brushes séparés.

**L'union de brushes laisse plusieurs brushes.** Le résultat n'est fait que de brushes convexes : une union non convexe (un L, un T) en demande au moins deux. Ils ne se chevauchent plus et n'ont plus de faces internes.

**Rendre creux échoue.** Le brush est trop fin pour l'épaisseur demandée. Réduisez l'épaisseur ou agrandissez le brush.

**Pourquoi les faces nodraw sont-elles sélectionnées dans l'outil de texture ?** Pour que vous puissiez les vérifier avant d'appliquer. Rien ne change tant que vous ne cliquez pas sur **Appliquer nodraw**.

## Pages liées

- [Outil de découpe](clip-tool.md)
- [Outil vertex](vertex-tool.md)
- [Transformer et aligner](transform-align.md)
- [Groupes](groups.md)
- [Propriétés des entités](entity-properties.md)
- [Application de texture](texture-tool.md)
- [Vérifier les problèmes](check-problems.md)
- [Actions et raccourcis](../reference/actions-shortcuts.md)

---
[Retour à l'index de la documentation](../README.md)
