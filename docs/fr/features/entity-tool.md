[Documentation](../README.md) > Fonctionnalités > Outil entité | [English](../../en/features/entity-tool.md) | [Русский](../../ru/features/entity-tool.md)

# Outil entité

L'outil entité place des entités ponctuelles : sur les surfaces dans la vue 3D (posées sur les sols, les murs, les plafonds et sur les modèles des props), ou sur la grille dans les vues 2D. Il place aussi les prefabs choisis dans l'onglet Prefabs.

## Ouvrir l'outil

- **Outils > Outil entité**, la palette d'outils, ou `Maj+E`.
- Tant qu'aucune classe n'est choisie, l'outil utilise le paramètre **Entité ponctuelle par défaut** (`info_player_start`).

## Placer une entité

- **Vue 3D** : cliquez sur une surface. La boîte de l'entité s'y pose par l'extérieur : sur un sol elle tient debout, sous un plafond elle y est suspendue, contre un mur elle le touche. Sur un prop, l'entité se pose sur le vrai maillage du modèle (pas sur sa boîte) ; là où le modèle n'a pas de triangle, elle tombe jusqu'à la surface derrière. Sans surface sous le curseur, l'entité va sur le plan du sol (hauteur 0).
- **Vues 2D** : cliquez pour placer l'entité sur le point de grille sous le curseur. Sa profondeur (le long de l'axe de la vue) est l'origine de la dernière entité placée en 3D, sinon le centre de la sélection, sinon 0.
- Une boîte fantôme portant le nom de la classe suit le curseur avant le clic.

Modificateurs :

| Touche | Effet |
|---|---|
| `Maj` + clic | Garder la sélection actuelle (sinon la nouvelle entité devient la sélection). |
| `Alt` + clic | Sans magnétisme (le décalage de pose est alors arrondi à 0,01 unité). |

Les positions s'alignent sur la grille de la vue ; sur un brush qui porte sa propre grille, l'entité se pose sur cette grille (voir [Grilles](grids.md)). Le décalage de pose est arrondi à l'unité (ou à la grille quand elle est plus fine qu'une unité), pour garder des origines propres.

Chaque placement est une seule étape d'annulation. La nouvelle entité reçoit les keyvalues et spawnflags par défaut de sa classe dans le FGD.

### Taille de la boîte fantôme

La boîte utilisée pour poser l'entité est, dans l'ordre : la hull de collision du modèle (pour les props, et pour les classes à modèle fixe sans taille, comme `info_player_start` qui prend la hull joueur de 32 x 32 x 72), sinon l'encombrement du modèle, sinon la taille donnée dans le FGD, sinon un cube de 16 unités.

## Options

| Option | Rôle | Par défaut |
|---|---|---|
| **Classe** | Sélecteur de classe avec recherche (voir plus bas). Son info-bulle montre la description FGD de la classe. Sans définitions d'entités chargées, il devient un simple champ texte (**Nom de classe**). | paramètre **Entité ponctuelle par défaut** |
| **Modèle** | Seulement pour les classes de props (`prop_*` et les classes avec une aide de modèle studio sans modèle fixe) : le modèle choisi dans le navigateur de modèles, ou **Aucun modèle sélectionné**. | |
| **Skin N** | Skin choisi dans le navigateur de modèles, appliqué aux nouveaux props (affiché quand ce n'est pas le skin par défaut). | |
| **Modèles...** | Ouvre l'onglet Modèles du navigateur de modèles pour choisir le modèle. | |
| **Aligner sur la surface** | Classes de props seulement : oriente les props pour que leur axe vertical suive la normale de la surface (la keyvalue `angles` est remplie). | désactivé |

Un prop placé sans modèle sélectionné est créé sans modèle et un avertissement est écrit dans la console.

## Le sélecteur de classe

Cliquez sur le bouton de la classe pour ouvrir la liste :

- Tapez dans **Rechercher une classe...** : chaque mot doit apparaître dans le nom ; les noms qui commencent par le premier mot passent en tête.
- `Haut` et `Bas` se déplacent dans la liste, `Entrée` choisit la classe en surbrillance, `Échap` ferme la liste.
- **Récentes** : les dernières classes placées (6 au plus) sont listées en premier quand la recherche est vide.
- **Utiliser « nom »** : un nom de classe absent du FGD (les entités scriptées de Garry's Mod par exemple) reste utilisable.
- Chaque ligne affiche la couleur d'éditeur de la classe, et sa description en info-bulle. La ligne du bas compte les classes trouvées.

## Placer des prefabs

Double-cliquez sur un prefab dans l'onglet **Prefabs** : l'outil entité passe en placement de prefab. La barre d'options affiche alors **Prefab :** avec son nom, un bouton **Revenir aux entités** et la case **Grouper**.

| Geste | Résultat |
|---|---|
| Clic dans la vue 3D | Insère le prefab à sa taille d'origine, posé sur la surface sous le curseur. |
| Clic dans une vue 2D | L'insère centré sur le point de grille (à la profondeur utilisée par l'outil entité). |
| Rectangle tracé dans une vue 2D | L'insère mis à l'échelle de la boîte tracée. |
| `Échap` | Abandonne la boîte en cours, sinon revient au placement d'entités. |

Les décalages s'alignent sur la grille, donc un prefab construit sur la grille y reste. Chaque insertion remplace le jeton `&i` du prefab par le numéro suivant de la map. **Grouper** (paramètre `[placement] group_prefabs`, activé par défaut) groupe les objets de chaque prefab inséré. Voir [Prefabs](prefabs.md) pour la bibliothèque.

## Astuces

- Choisissez d'abord le modèle dans le [Navigateur de modèles](model-browser.md), puis cliquez avec l'outil entité : chaque clic place un nouveau prop avec ce modèle et ce skin.
- Utilisez `Maj` + clic pour poser une rangée de lumières sans perdre la sélection sur laquelle vous travaillez.
- Pour modifier les keyvalues après le placement, ouvrez les [Propriétés de l'entité](entity-properties.md) (`Alt+Enter`).
- Les outils MCP `create_entity` et `set_current_entity_class` font la même chose depuis un assistant IA (voir la [référence des outils MCP](../../MCP.md)).
- `set_tool_options` avec l'outil `entity` règle ce que le prochain clic pose : `class`, `model` (un chemin `.mdl`, vide pour aucun), `skin` (0 à 255) et `align` (les props suivent la pente de la surface).

## FAQ

**Pourquoi mon entité flotte au-dessus du sol ?** L'outil pose la boîte de l'entité sur la surface ; pour un modèle il utilise la hull de collision, qui peut être plus grande que le maillage visible. Utilisez `Alt` + clic ou déplacez-la ensuite.

**Mon entité Lua perso n'est pas dans la liste.** Tapez son nom dans le champ de recherche et choisissez **Utiliser « nom »**.

**Pourquoi un prop n'a pas de modèle ?** Aucun modèle n'était choisi dans le navigateur de modèles. Cliquez sur **Modèles...**, choisissez-en un, puis placez à nouveau.

## Pages liées

- [Propriétés de l'entité](entity-properties.md)
- [Navigateur de modèles](model-browser.md)
- [Prefabs](prefabs.md)
- [Outil chemin](path-tool.md)
- [Outil sprinkle](sprinkle-tool.md)
- [Décals et overlays](decals-overlays.md)
- [Grilles](grids.md)

---
[Retour à l'index de la documentation](../README.md)
