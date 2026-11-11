[Documentation](../README.md) > Fonctionnalités > Fuites et pointfiles | [English](../../en/features/leaks-pointfiles.md) | [Русский](../../ru/features/leaks-pointfiles.md)

# Fuites et pointfiles

Une fuite est un chemin d'une entité vers le vide hors de la map : le compilateur ne distingue plus l'intérieur de l'extérieur, et la visibilité comme l'éclairage échouent. Hammer Pro trouve les fuites pendant que vous travaillez (la vérification instantanée), charge le pointfile d'une compilation, vous guide le long du chemin de fuite, et affiche les portails et la visibilité de la map pour l'optimiser. Il peut aussi fermer une ouverture de porte avec un areaportal.

## La vérification instantanée des fuites

Pendant l'édition, un remplissage comme celui du compilateur cherche les fuites en arrière-plan, sans compiler (paramètre **Chercher les fuites pendant l'édition**, activé par défaut). Quand il en trouve une, la barre d'état affiche un badge rouge **Fuite** et le chemin est tracé dans les vues (paramètre **Tracer le chemin de la fuite trouvée**, activé par défaut). La compilation reste la référence.

**Map > Vérifier les fuites** lance la vérification tout de suite, puis :

- *La map fuit : l'entité N (classe) atteint le vide (M entités en tout). Son chemin est tracé dans les vues.* L'entité qui fuit est sélectionnée et les vues sont centrées dessus.
- *Aucune fuite : la map est étanche (vérifiée en N ms)*.
- *Aucun brush du monde ne ferme encore la map : rien à vérifier*.
- *La map est trop complexe pour la vérification instantanée : compilez-la pour trouver les fuites*.

Cliquer le badge **Fuite** de la barre d'état fait la même chose.

## Les pointfiles d'une compilation

Quand une compilation trouve une fuite, le compilateur écrit un pointfile (`<map>.lin`). Avec **Charger automatiquement le pointfile de fuite** (Paramètres > Compilation, activé par défaut), il est chargé à la fin de la compilation et l'entité qui fuit est sélectionnée. La fenêtre de compilation peut aussi s'arrêter juste après la géométrie en cas de fuite (**S'arrêter quand la map fuit**, voir [Fenêtre de compilation](../compiler/compile-window.md)).

| Commande | Effet |
| --- | --- |
| **Map > Fuites et visibilité > Charger le pointfile de fuite...** | Charge `<map>.lin`, sinon `<map>.pts`, à côté de la map ou dans son dossier de construction (`.hammerpro_build`). S'il n'y en a pas, demande un fichier (filtre **Pointfile de fuite**, `.lin` et `.pts`). |
| **Map > Fuites et visibilité > Décharger le pointfile** | Retire le chemin des vues. |
| Déposer un fichier `.lin` ou `.pts` sur la fenêtre | Le charge. |

Un pointfile chargé remplace dans les vues le chemin de la vérification instantanée. Si l'entité qui fuit est supprimée, le chemin est déchargé (*L'entité qui fuyait n'est plus là : chemin de fuite déchargé. Recompile pour vérifier que la map est étanche.*).

## Le navigateur du chemin de fuite

Un pointfile chargé ouvre une petite fenêtre flottante, **Chemin de fuite : N points**, pour promener la caméra le long du chemin comme on corrige une fuite : le point 1 est l'entité qui fuit et **Suivant** avance vers le vide.

| Contrôle | Effet |
| --- | --- |
| **Point N sur M** | Votre position sur le chemin. |
| **Précédent** | Point précédent, vers l'entité qui fuit. |
| **Suivant** | Point suivant, vers le vide. |
| **Regarder le point actuel** | Déplace la caméra 3D pour regarder le point. |
| **Décharger** | Décharge le pointfile. |

Les vues marquent les extrémités du chemin : **Début de la fuite : entité** et **Fin de la fuite : vide**.

## Fichiers de portails et visibilité

Un fichier de portails (`<map>.prt`) contient les portails entre les clusters de visibilité construits par l'étape de géométrie. Les afficher montre où la visibilité découpe la map et comment les hints et les `func_detail` la changent.

| Commande | Raccourci | Effet |
| --- | --- | --- |
| **Map > Fuites et visibilité > Charger le fichier de portails...** | | Charge le `.prt` de la map (ou en demande un). Déposer un `.prt` sur la fenêtre marche aussi. |
| **Map > Fuites et visibilité > Décharger le fichier de portails** | | Retire les portails. |
| **Map > Fuites et visibilité > Construire les portails (géométrie seule)** | `Ctrl+Maj+P` | Construit seulement la géométrie de la map actuelle (quelques secondes, dans son dossier de construction) et charge ses portails : l'effet des hints et des `func_detail` se voit tout de suite. |
| **Map > Fuites et visibilité > Portails et visibilité...** | | Ouvre la fenêtre décrite plus bas. |
| **Affichage > Diagnostics > Afficher les clusters visibles** | | Colore les portails des clusters visibles depuis la caméra (nécessite une map compilée avec la visibilité). |

Après une compilation qui a écrit un nouveau fichier de portails, Hammer Pro vous le propose (*La compilation a écrit un nouveau fichier de portails.* **Le charger** / **Pas maintenant**), sauf si vous avez décoché **Proposer le nouveau fichier de portails après une compilation**. Les portails déjà affichés sont rechargés automatiquement.

### La fenêtre Portails et visibilité

| Contrôle | Effet | Défaut |
| --- | --- | --- |
| Résumé | *N portails entre M clusters*, ou **Aucun fichier de portails chargé**. | |
| **Épaisseur des lignes** | Épaisseur des lignes des portails, de 1 à 4 px. | 1 px |
| **Voir à travers les murs (3D)** | La vue 3D dessine les portails par-dessus la géométrie, comme les vues 2D. | activé |
| **Colorer selon le coût de rendu** | Les portails passent du vert au rouge selon les triangles dessinés depuis leurs clusters : les zones rouges sont celles à optimiser (hints, func_detail, areaportals). Nécessite la visibilité de la map compilée. | désactivé |
| **Grille des feuilles (1024 unités)** | Le compilateur découpe les feuilles toutes les 1024 unités : les hints marchent mieux le long de cette grille. **Épaisseur** règle ses lignes (1 à 6 px). | désactivé |
| **Afficher les clusters visibles** | Les portails des clusters visibles depuis la caméra (ou un point choisi) passent en vert, comme `mat_leafvis 3` en jeu. **Depuis la caméra** ou **Depuis un point** (cliquez un point dans une vue). Affiche *Cluster N : X clusters visibles sur Y* et *Dessiné d'ici : faces, triangles, displacements, props, overlays*. | désactivé |
| **Géométrie seule et recharger** | Comme **Construire les portails (géométrie seule)**. | |
| **Recharger** | Lit le fichier de portails le plus récent de la map ; après des modifications, il est d'abord reconstruit. | |
| **Charger le fichier de portails...** / **Décharger le fichier de portails** | Comme dans le menu Map. | |
| **Détacher la fenêtre** | Affiche la fenêtre dans sa propre fenêtre (sur un autre écran par exemple). **Remettre dans la fenêtre principale** la ramène. Sa position est mémorisée. | |

## Fermer une ouverture avec un areaportal

Sélectionnez une porte (`func_door`, `func_door_rotating`, `prop_door_rotating`, `func_movelinear`, ou un brush de l'une d'elles) et lancez **Map > Fuites et visibilité > Fermer l'ouverture par un areaportal**. Hammer Pro ajuste un brush `func_areaportal` à l'ouverture, le lie à la porte (la porte prend le nom `door_<id>` si elle n'en a pas), le fait démarrer ouvert ou fermé comme la porte, et le sélectionne. Une seule étape d'annulation. Sans porte sélectionnée, vous obtenez *Sélectionnez une porte (func_door, prop_door_rotating...) pour fermer son ouverture*.

Les vérifications d'areaportals de [Vérifier les problèmes](check-problems.md) vous préviennent quand un areaportal et sa porte sont désaccordés, traversent l'eau ou un occluder, ou ont plus d'un brush.

## Paramètres

**Paramètres > Santé de la map** (`[health]`) :

| Option | Clé | Défaut |
| --- | --- | --- |
| **Chercher les fuites pendant l'édition** | `live_leak_check` | activé |
| **Tracer le chemin de la fuite trouvée** | `show_leak_path` | activé |
| **Épaisseur des portails** | `portal_width` | 1 |
| **Afficher la grille des feuilles de 1024 unités** | `leaf_grid` | désactivé |
| **Proposer le nouveau fichier de portails après une compilation** | `offer_portals` | activé |
| Épaisseur de la grille des feuilles (fenêtre des portails) | `leaf_width` | 1 |
| Colorer selon le coût de rendu (fenêtre des portails) | `portal_heat` | désactivé |
| Voir à travers les murs (fenêtre des portails) | `portal_xray` | activé |

## MCP

`check_leaks`, `get_leak`, `load_pointfile` et `portal_file` donnent les mêmes outils aux applications d'IA. Voir la [référence des outils MCP](../../MCP.md) (en anglais).

## Astuces

- Corrigez une fuite en partant de l'entité : commencez au point 1 et appuyez sur **Suivant** jusqu'à voir le trou.
- Après avoir ajouté des hints ou des `func_detail`, appuyez sur `Ctrl+Maj+P` pour voir les nouveaux portails en quelques secondes sans compilation complète.
- Activez **Colorer selon le coût de rendu** après une compilation avec visibilité pour trouver les zones coûteuses.

## FAQ

**La vérification instantanée dit que la map est étanche mais la compilation fuit. Pourquoi ?** La compilation reste la référence : certains cas (entités mal placées, map très complexe) ne sont repérés que par le compilateur. Chargez son pointfile.

**Où est le pointfile ?** À côté de la map ou dans le dossier de construction `.hammerpro_build` voisin. **Charger le pointfile de fuite...** regarde aux deux endroits.

**Afficher les clusters visibles ne montre rien.** Il faut une map compilée avec la visibilité (*Pas encore de données de visibilité : compilez la map avec la visibilité*).

## Pages liées

- [Vérifier les problèmes](check-problems.md)
- [Fenêtre de compilation](../compiler/compile-window.md)
- [Erreurs courantes](../troubleshooting/common-errors.md)
- [Cadres de fenêtre et fenêtres séparées](window-frames.md)

---
[Retour à l'index de la documentation](../README.md)
