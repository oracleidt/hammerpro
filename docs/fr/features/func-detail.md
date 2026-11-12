[Documentation](../README.md) > Fonctionnalités > func_detail | [English](../../en/features/func-detail.md) | [Русский](../../ru/features/func-detail.md)

# func_detail

Les brushes `func_detail` bloquent toujours les joueurs, les balles et la lumière, mais VBSP ne s'en sert pas pour découper la map en visleaves. Transformer le décor (marches, moulures, piliers, gravats, encadrements de fenêtres, cylindres) en `func_detail` garde une visibilité simple : VVIS va plus vite et chaque visleaf voit moins de choses. Seule la coque extérieure qui scelle la map contre le vide doit rester en brushes du monde.

Hammer Pro en fait une affaire d'une touche : sélectionner, appuyer sur `Ctrl+Alt+D`, c'est fait. Il peut aussi trouver pour vous les brushes qui devraient être du détail, et les teinter dans les vues pour voir d'un coup d'oeil ce qui est du détail.

## Transformer en func_detail

| Où | Comment |
|---|---|
| Clavier | `Ctrl+Alt+D` (modifiable dans **Paramètres > Clavier**) |
| Menu | **Outils > Opérations sur les brushes > Transformer en func_detail** |
| Barre d'outils | **Vers func_detail** dans le groupe **Déplacer la sélection**, à côté de **Vers le monde** et **Vers une entité** |
| Clic droit | **Transformer en func_detail** dans le menu contextuel des vues 2D et 3D |
| Palette de commandes | `Ctrl+Shift+P`, tapez `func_detail` |

Les brushes du monde sélectionnés deviennent des `func_detail` en **une seule étape d'annulation** :

- **Un func_detail par sélection** (par défaut) les met tous dans une entité. **Un par groupe** (réglage **Paramètres > Brushes > func_detail > Transformer en func_detail crée**, `brush.detail.split = "cluster"`) crée une entité pour chaque ensemble de brushes qui se touchent, pratique quand vous sélectionnez d'un coup les moulures de toute une rue.
- **Les groupes restent intacts** : des brushes de groupes différents vont toujours dans des `func_detail` différents, et chaque nouvelle entité rejoint le groupe de ses brushes. Les **visgroups** et les **textures** sont conservés tels quels.
- Si la sélection contient déjà un `func_detail`, les nouveaux brushes y sont ajoutés au lieu d'en créer un autre.
- Les brushes déjà dans un `func_detail` restent où ils sont. Les brushes d'une autre entité brush (une porte, un `func_brush`) et les displacements sont ignorés ; l'avis dit combien et pourquoi (faites d'abord **Renvoyer au monde** pour convertir une entité brush ; les displacements ne découpent jamais les visleaves).
- Les nouvelles entités sont sélectionnées.

### Vérification de l'étanchéité

Un `func_detail` ne scelle pas la map. Avant de convertir, Hammer Pro lance le même remplissage de fuite que **Map > Vérifier les fuites** et le conseiller d'optimisation : il retire les brushes sélectionnés de la coque et regarde si l'espace jouable atteint alors le vide.

- Les brushes à l'intérieur de la map (le cas habituel) sont convertis tout de suite. Quand la vérification de fuite en direct connaît déjà la coque, il n'y a même rien à calculer.
- Quand des brushes sélectionnés scellent la map, une fenêtre l'explique : **{n} des {total} brushes sélectionnés scellent la map**, car ils font partie de la coque extérieure. Choisissez **Convertir les {k} autres** (le choix sûr), **Tout convertir quand même** (la map fuira tant que la coque n'est pas fermée autrement), **Sélectionner les brushes qui scellent** pour les voir, ou **Annuler**.
- Quand la map fuit déjà, la vérification ne peut pas se faire : les brushes sont convertis et un avertissement vous demande de corriger la fuite (voir [Fuites et pointfiles](leaks-pointfiles.md)).

Désactivez la vérification avec **Paramètres > Brushes > func_detail > Garder dans le monde les brushes qui scellent la map** (`brush.detail.check_sealing`, activé par défaut).

## Renvoyer au monde

L'inverse est tout aussi rapide : **Outils > Renvoyer au monde** (`Ctrl+Shift+W`), le bouton **Vers le monde** de la barre d'outils ou le menu du clic droit renvoient au monde les brushes des entités brush sélectionnées, `func_detail` compris, en une étape d'annulation. Voir [Opérations sur les brushes](brush-operations.md).

## Sélectionner les candidats func_detail

**Outils > Opérations sur les brushes > Sélectionner les candidats func_detail...** ouvre une petite fenêtre qui sélectionne les brushes du monde qui devraient être des `func_detail`, avec les règles du conseiller **Map > Optimiser** :

| Option | Défaut | Effet |
|---|---|---|
| **Taille maximale** | 1024 unités | les brushes plus longs que cela sur un axe sont exclus |
| **Marches et escaliers** | activé | brushes bas et longs (au plus 32 de haut, 64 de profondeur) |
| **Moulures et encadrements** | activé | brushes fins et longs (au plus 16 d'épaisseur) |
| **Piliers et poteaux** | activé | brushes étroits et hauts (au plus 64 de large) |
| **Petites pièces et gravats** | activé | brushes d'au plus 64 unités sur chaque axe |
| **Formes complexes (cylindres, arches)** | activé | brushes de plus de 6 faces ou à faces inclinées, au plus 256 unités |
| **Seulement dans la sélection actuelle** | désactivé | ne regarder que les brushes sélectionnés (sélectionnez un bâtiment, puis affinez) |

**Sélectionner les candidats** travaille en arrière plan : les candidats sont vérifiés avec le remplissage de fuite, pour que les convertir tous n'ouvre jamais la map, et les brushes qui scellent sont exclus (le résultat dit combien). Rien ne change dans la map : vérifiez la sélection, désélectionnez ce que vous voulez garder dans le monde, puis appuyez sur **Transformer en func_detail** (le bouton apparaît dans la fenêtre dès que quelque chose est sélectionné, ou utilisez `Ctrl+Alt+D`). Les options sont enregistrées (`brush.detail.max_size`, `steps`, `trims`, `pillars`, `small`, `complex`, `in_selection`).

Seuls les brushes du monde visibles dont toutes les faces scellent (pas de texture outil, pas de ciel, pas de displacement) sont pris en compte.

Dans la fenêtre **Map > Optimiser**, la recommandation func_detail a un bouton **Les sélectionner** à côté de **Aller à** : il sélectionne les brushes suggérés sans déplacer les vues. Le conseiller lui même ne modifie toujours jamais la map.

## Créer des brushes en func_detail

L'[outil bloc](block-tool.md), l'[outil polygone](polygon-tool.md), l'[outil courbe](curve-tool.md) et l'[outil route](road-tool.md) ont un bouton **Détail** à côté de leur choix **Créer comme** : quand il est activé, les nouveaux brushes sont créés directement en `func_detail` (une étape d'annulation) ; cliquez à nouveau pour des brushes du monde. C'est la même chose que choisir `func_detail` dans la liste.

## Teinter les brushes func_detail

**Affichage > Teinter les brushes func_detail** (aussi **Paramètres > Brushes > func_detail > Teinter les brushes func_detail dans les vues**, `brush.detail.tint`, désactivé par défaut) donne aux brushes `func_detail` une teinte bleue dans les vues 3D et un contour bleu dans les vues 2D. Une couleur choisie avec les [couleurs d'affichage](display-colors.md) ou les couleurs de visgroup reste prioritaire pour le contour.

## Réglages

Tous dans la table `[brush.detail]` de `settings.toml` et dans **Paramètres > Brushes** :

| Réglage | Défaut | Rôle |
|---|---|---|
| `split` | `"selection"` | `"selection"` : un func_detail par sélection, `"cluster"` : un par ensemble de brushes qui se touchent |
| `check_sealing` | `true` | lancer la vérification d'étanchéité avant de convertir |
| `tint` | `false` | teinter les brushes func_detail en 2D et en 3D |
| `max_size`, `steps`, `trims`, `pillars`, `small`, `complex`, `in_selection` | 1024, activé, activé, activé, activé, activé, désactivé | filtres de **Sélectionner les candidats func_detail** |

## MCP

Pour les assistants IA :

- `make_func_detail` : `ids` / `solid_ids` (par défaut la sélection), `split` (`selection` ou `cluster`), `sealing` (`skip` convertit les autres et signale ceux qui scellent, `refuse` ne convertit rien si l'un scelle, `allow` saute la vérification). Renvoie les ids des `func_detail`, les solides convertis et refusés, ce qui a été ignoré et si la vérification a pu se faire.
- `select_detail_candidates` : `max_size`, `kinds` (`step`, `trim`, `pillar`, `small`, `complex`), `within`, `within_selection`, `select` (vrai par défaut), `limit`. Sélectionne seulement.
- `run_action` avec `convert_to_detail`, `select_detail_candidates` (ouvre la fenêtre) ou `toggle_detail_tint` ; `set_setting` avec les chemins `brush.detail.*` ; `set_tool_options` avec `create_as` `func_detail` pour les outils de création.

Voir la [référence des outils MCP](../../MCP.md).

## Astuces

- Construisez d'abord la coque (sol, murs, plafond, ciel), puis passez tout le reste en détail avec **Sélectionner les candidats func_detail** et `Ctrl+Alt+D`.
- Gardez le bouton **Détail** activé pendant que vous habillez une pièce : chaque nouvelle moulure ou marche est du détail dès le départ.
- Activez la teinte avant une compilation pour vérifier qu'aucun grand mur n'a été transformé en détail par erreur.

## FAQ

**Pourquoi certains brushes sont restés dans le monde ?** Ils scellent la map : ils séparent l'espace jouable du vide. Un `func_detail` à leur place ferait fuir la map, donc Hammer Pro les garde (ou demande). **Sélectionner les brushes qui scellent** les montre.

**Un func_detail peut il toucher le vide ?** Oui, tant que des brushes du monde derrière lui scellent la map. La vérification ne refuse que les brushes qui sont la dernière barrière.

**Le func_detail change t il l'éclairage ou les collisions ?** Non. Les brushes de détail projettent des ombres et bloquent les joueurs comme les brushes du monde. Ils cessent seulement de découper les visleaves et ne bloquent plus la visibilité.

**Pourquoi les displacements sont ignorés ?** Les displacements ne découpent jamais les visleaves, les passer en détail n'apporte rien.

## Pages liées

- [Opérations sur les brushes](brush-operations.md)
- [Vérifier les problèmes](check-problems.md)
- [Fuites et pointfiles](leaks-pointfiles.md)
- [Optimiser une map](../guides/optimizing-a-map.md)
- [Outil bloc](block-tool.md)
- [Modes de rendu et affichage](render-modes-display.md)
- [Actions et raccourcis](../reference/actions-shortcuts.md)

---
[Retour à l'index de la documentation](../README.md)
