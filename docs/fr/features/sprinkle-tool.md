[Documentation](../README.md) > Fonctionnalités > Outil sprinkle | [English](../../en/features/sprinkle-tool.md)

# Outil sprinkle

L'outil sprinkle peint des entités (props, plantes, rochers, lumières...) sur les surfaces de la map avec un pinceau rond, comme l'outil Sprinkle de Hammer++ et du Hammer de CS:GO. Ce qu'il pose vient de fichiers de définitions sprinkle, au même format que Hammer++ : vos définitions existantes marchent telles quelles.

## Ouvrir l'outil

- **Outils > Outil sprinkle**, ou son bouton dans la palette d'outils. Il n'a pas de raccourci par défaut (vous pouvez lui en donner un dans **Paramètres > Clavier**, voir [Raccourcis clavier](keyboard-shortcuts.md)).
- Choisissez un type sprinkle dans la liste **Type :** de la barre d'options. Sans type, peindre affiche **Choisissez d'abord un type sprinkle**. Sans aucune définition, la barre d'état indique dans quel dossier mettre des fichiers `.txt`.

## Peindre

Le pinceau est un cercle dessiné sous le curseur (vert, rouge quand vous effacez).

- Dans la vue 3D, il suit la surface sous le curseur ; son plan est celui de l'axe principal de la normale de la surface (un sol, un mur ou un plafond).
- Dans une vue 2D, il est dans le plan de la vue, et les entités tombent sur la première surface vue depuis l'observateur.

Le plan est découpé en cases de grille (la grille du type, ou celle de l'éditeur avec **Utiliser la grille**). Chaque case reçoit au plus une entité, posée sur la surface sous la case.

| Geste | Effet |
| --- | --- |
| Clic ou glisser | Applique le mode actuel aux cases sous le pinceau. Chaque case est traitée une seule fois par coup de pinceau. Un coup de pinceau entier est une seule étape d'annulation. |
| `Ctrl`+glisser | Efface toutes les entités du type sous le pinceau, quels que soient le mode et la densité. |
| `Alt`+molette | Change le rayon du pinceau (environ 15 % par cran). |
| `Échap` pendant un coup de pinceau | Abandonne tout le coup de pinceau. |

Les entités ne se posent que sur les brushes du monde et sur les brushes `func_detail`, `func_brush`, `func_wall` et `func_lod` qui ont au moins une face sans matériau `tools/` (les triggers, clips et autres volumes sont ignorés). Les entités masquées ne sont jamais touchées.

Une entité « appartient » à un type quand sa classe est l'une des classes du type et, si le type définit des modèles, que son modèle en fait partie. Retirer, Remplacer, Écraser et l'effacement au `Ctrl` n'agissent que sur ces entités.

## Modes

| Mode | Effet |
| --- | --- |
| **Ajouter** | Remplit les cases vides : chaque case vide reçoit une entité avec la densité comme probabilité. |
| **Retirer** | Retire les entités du type, chacune avec la densité comme probabilité. |
| **Remplacer** | Tire une nouvelle entrée pour les cases qui contiennent une entité du type. |
| **Écraser** | Vide les cases, puis les remplit à nouveau selon la densité. |

## Options

Les options sont enregistrées dans `settings.toml`, table `[placement.sprinkle]`.

| Option | Effet | Par défaut |
| --- | --- | --- |
| **Type :** | Le type sprinkle à peindre. L'infobulle de chaque type indique le fichier d'où il vient. | aucun |
| Bouton Rafraîchir | Relire les définitions. | |
| Bouton Modifier | Ouvrir la source : le fichier de définition du type dans l'éditeur de texte (désactivé quand le fichier est dans une archive). | |
| Bouton Dossier | Ouvrir le dossier sprinkle de Hammer Pro. | |
| Mode | **Ajouter**, **Retirer**, **Remplacer** ou **Écraser** (voir plus haut). | **Ajouter** |
| **Densité :** | Probabilité en pourcentage qu'une case reçoive une entité (ou la perde en retirant), de 0 à 100 %. | 50 % |
| **Rayon :** | Rayon du pinceau en unités, de 8 à 4096 (`Alt`+molette). | 128 |
| **Utiliser la grille** | Utiliser la grille de l'éditeur au lieu de la grille de la définition (l'infobulle montre la grille du type). | Décoché |
| **Lacet aléatoire** | Tourner chaque entité selon un lacet (yaw) aléatoire. | Coché |
| **Décalage :** | Déplacement aléatoire dans la case, en pourcentage d'une demi-case, de 0 à 100 %. | 50 % |
| **Décalage Z :** | Distance à la surface, de -4096 à 4096 unités. | 0 |
| **Aligner sur la normale** | Incliner les entités pour suivre la surface sur laquelle elles reposent. Le décalage Z suit alors la normale de la surface. | Décoché |

Si la grille est trop fine pour le rayon (plus de 40 000 cases sous le pinceau), rien n'est posé et le message **La grille est trop petite pour ce rayon : rien n'a été répandu** s'affiche : agrandissez la grille ou réduisez le rayon.

## Fichiers de définitions

Un fichier de définitions est un fichier `.txt` qui contient un ou plusieurs types :

```text
"Jungle foliage"
{
    base { "classname" "prop_static" grid "64 64" }
    10 { "model" "models/props/plant01.mdl" }
    30 { "model" "models/props/plant02.mdl" "skin" "1" }
}
```

- Le nom du bloc (`"Jungle foliage"`) est le nom affiché dans la liste **Type :** (les types sont triés par nom).
- `base` contient les keyvalues que reçoit chaque entité, et la `grid` du type (`"64 64"` pour X et Y dans le plan du pinceau, ou `"64"` pour les deux). Les keyvalues écrites directement dans le bloc du type comptent aussi comme keyvalues de base. Sans grille, le type utilise 64 x 64.
- Chaque bloc numéroté est une entrée tirée avec ce poids (ici `plant02` sort trois fois plus souvent que `plant01`). Ses keyvalues remplacent celles de la base.
- `grid`, `origin` et `id` ne sont jamais écrits sur les entités.
- Un type sans entrée utilisable (pas de poids positif, ou pas de `classname`) est ignoré. Les problèmes trouvés sont comptés dans **{0} problèmes dans les définitions (voir la console)** et détaillés dans la console.

### Où les définitions sont lues

| Emplacement | Remarques |
| --- | --- |
| `scripts/hammer/sprinkle/` du contenu du jeu | Lu à travers le contenu monté (jeu de base, addons, dossiers montés). |
| `bin/hammerplusplus/scripts/sprinkle/` et `bin/win64/hammerplusplus/scripts/sprinkle/` de l'installation de Garry's Mod | Les dossiers de Hammer++, en lecture seule. |
| `bin/scripts/hammer/sprinkle/` et `bin/win64/scripts/hammer/sprinkle/` de l'installation de Garry's Mod | Les dossiers de Hammer, en lecture seule. |
| `%APPDATA%\HammerPro\sprinkle` | Le dossier de Hammer Pro (bouton Dossier de la barre d'options). |
| Dossiers supplémentaires | Ajoutés dans **Paramètres > Prefabs > Sprinkle** avec **Ajouter un dossier...**, enregistrés sous `extra_dirs`. |

Seuls les fichiers `.txt` sont lus, sans les sous-dossiers, jusqu'à 1 Mo par fichier et 512 fichiers par emplacement. Les fichiers sont lus sur un thread de travail et gardés en cache selon leur date de modification : un rafraîchissement ne relit que les fichiers modifiés. Les définitions sont relues avec le bouton Rafraîchir, quand vous changez la liste des dossiers, et quand le contenu du jeu est monté à nouveau.

## Astuces

- Utilisez **Écraser** avec une densité faible pour éclaircir et remélanger une zone en un seul passage.
- Pour de l'herbe sur une pente, cochez **Aligner sur la normale** et mettez un petit **Décalage Z :** négatif pour que la base ne flotte pas.
- Les assistants IA peuvent répandre des entités par MCP avec l'outil `sprinkle` (éléments pondérés, nombre, espacement, lacet aléatoire, alignement, pente maximale), et lire ou changer les options de l'outil avec `get_tool_options` / `set_tool_options` (outil `sprinkle`). Voir la [référence des outils MCP](../../MCP.md) (en anglais).

## FAQ

**La liste Type est vide.** Aucune définition n'a été trouvée. Mettez un fichier `.txt` dans `%APPDATA%\HammerPro\sprinkle` (bouton Dossier), ou ajoutez votre dossier dans **Paramètres > Prefabs > Sprinkle**, puis cliquez sur Rafraîchir.

**Rien n'apparaît sur mon prop ou mon trigger.** Les entités ne se posent que sur des surfaces de brush (brushes du monde et `func_detail`, `func_brush`, `func_wall`, `func_lod`) ; les modèles et les brushes outils sont ignorés.

**Ctrl a effacé des entités que j'avais posées à la main.** L'effacement retire toutes les entités visibles qui appartiennent au type (même classe et, si le type définit des modèles, même modèle). Masquez celles que vous voulez garder.

**J'ai modifié une définition et rien n'a changé.** Cliquez sur le bouton Rafraîchir de la barre d'options.

## Pages liées

- [Semis par matériau](scatter-by-material.md)
- [Prefabs](prefabs.md)
- [Outil entité](entity-tool.md)
- [Navigateur de modèles](model-browser.md)
- [Masquer et isoler](hide-isolate.md)
- [Fenêtre des paramètres](settings-dialog.md)

---
[Retour à l'index de la documentation](../README.md)
