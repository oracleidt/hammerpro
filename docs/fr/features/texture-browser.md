[Documentation](../README.md) > Fonctionnalités > Navigateur de textures | [English](../../en/features/texture-browser.md)

# Navigateur de textures

Le navigateur de textures est l'onglet **Textures** du dock de droite. Il affiche tous les matériaux du contenu monté (VPK du jeu, addons, Workshop, dossiers de textures personnalisés) dans une grille de vignettes, avec une recherche instantanée, des filtres, des favoris et le matériau actuel : celui que l'outil de texture, l'outil bloc et **Appliquer la texture actuelle** posent sur les faces.

## Ouvrir le navigateur

- Cliquez sur l'onglet **Textures** du dock de droite.
- **Affichage > Panneaux > Navigateur de textures** affiche l'onglet et place le curseur dans la recherche. Aucun raccourci par défaut.
- Avec l'outil de texture, **Parcourir...** du panneau **Édition des faces** ouvre le navigateur : le prochain matériau choisi est appliqué aux faces sélectionnées, puis le panneau revient (voir [Outil de texture](texture-tool.md)).
- Quand une clé matériau du panneau d'entité attend un matériau, un bandeau en haut du navigateur indique **Choisissez un matériau pour ...** : double-cliquez sur un matériau (ou Entrée dans la recherche) pour le donner à la clé, **Utiliser le matériau actuel** donne le matériau actuel, **Annuler** revient au panneau d'entité.

## Le matériau actuel

Le haut de l'onglet montre le matériau actuel : une grande vignette, son nom, une étoile, son dossier, sa taille et son shader, et le contenu d'où il vient. **Absent du contenu du jeu** en rouge signifie qu'aucun fichier monté ne porte ce nom. **Redimensionnée dans le jeu** signale une image dont les côtés ne sont pas des puissances de deux : le jeu la reçoit redimensionnée (survolez l'étiquette pour voir les tailles) et les échelles de texture utilisent la taille du jeu.

| Commande | Effet |
|---|---|
| Vignette | Glissez-la sur une face pour appliquer le matériau. Double clic : l'appliquer à la sélection. |
| Étoile (**Ajouter aux favoris** / **Retirer des favoris**) | Ajoute ou retire le matériau des favoris. |
| **Appliquer à la sélection** | Applique le matériau aux faces sélectionnées, ou à toutes les faces des objets sélectionnés. Grisé sans sélection. |
| **Remplacer...** | Ouvre [Rechercher et remplacer des textures](find-replace-textures.md) avec ce matériau dans le champ **Rechercher**. |
| **Modifier le matériau** | Ouvre l'[éditeur de matériaux](material-editor.md) sur ce matériau, ou sur la sélection multiple si vous en avez fait une (`Ctrl+clic`). |
| **Marquer** | Sélectionne toutes les faces qui utilisent ce matériau et passe à l'outil de texture. |

Le matériau actuel change quand vous cliquez une vignette, quand vous prenez le matériau d'une face avec `Alt+clic` dans l'outil de texture, ou quand vous appliquez un matériau par glisser-déposer. Chaque matériau utilisé passe en tête de la liste **Récents** (64 matériaux au plus).

## Recherche

Tapez dans **Rechercher des matériaux...** : la liste est filtrée à chaque touche, même avec 40 000 matériaux. Tous les mots doivent correspondre, sans tenir compte de la casse ni des accents. `Entrée` dans la recherche fait du premier résultat le matériau actuel.

| Mot | Garde les matériaux... |
|---|---|
| `brick wall` | dont le nom contient `brick` et `wall`. |
| `-dev` | dont le nom ne contient pas `dev`. |
| `kw:urban` | dont les mots-clés du VMT (`%keywords`) contiennent `urban`. |
| `src:workshop` | dont la source contient le mot (nom d'addon, `workshop`, nom d'un dossier personnalisé...). |
| `is:translucent` | avec `$translucent` ou `$additive`. |
| `is:alpha` | dont l'alpha de la texture est transparent en jeu (mélangé ou alpha testé, comme le décide le moteur). |
| `is:selfillum` | avec `$selfillum`. |
| `is:envmap` | avec `$envmap` (reflets). |
| `is:blend` | de type blend (WorldVertexTransition). |
| `is:missing` | dont le VMT ou la texture de base est introuvable. |
| `-is:propriété` | sans cette propriété (par exemple `is:translucent -is:envmap`). |
| `shader:lightmapped` | dont le shader contient le mot. |
| `-shader:vertexlit` | dont le shader ne contient pas le mot. |

Un mot collé sous forme de chemin de fichier est réduit au nom du matériau : `c:/game/materials/brick/brickwall001a.vmt` cherche `brick/brickwall001a`. Les mots `kw:`, `is:` et `shader:` ont besoin des mots-clés et des propriétés de tous les matériaux : ils sont lus en arrière-plan la première fois qu'un tel mot est utilisé (**Indexation des mots-clés...**, **Lecture des propriétés des matériaux...** à côté du compteur), puis la recherche redevient instantanée.

Le bouton `X` au bout du champ efface la recherche.

## Filtres

| Filtre | Effet | Par défaut |
|---|---|---|
| Puces de source **Tous**, **Jeu**, **Addons**, **Workshop** et une puce par dossier personnalisé | N'affiche que les matériaux de cette source. Une puce n'apparaît que si le contenu a ce type de source. Survolez la puce d'un dossier pour son chemin et son nombre de matériaux. | **Tous** |
| Puces de catégorie **Tous**, **Utilisés dans la map**, **Favoris (n)**, **Récents**, **Outils** | **Utilisés dans la map** : les matériaux de la map ouverte (y compris les manquants). **Favoris** : vos matériaux étoilés. **Récents** : les derniers utilisés, le plus récent en premier. **Outils** : les textures outils seulement. | **Tous** |
| Puces de propriétés **Translucide**, **Alpha**, **Auto-illuminé**, **Réfléchissant**, **Mélange**, **Manquant** | Chaque puce ajoute ou retire le mot `is:` correspondant dans la recherche. | aucune |
| Liste **Shader :** | **Tous**, ou un shader avec son nombre de matériaux (les plus utilisés en premier). Écrit `shader:nom` dans la recherche. | **Tous** |
| **Masquer VertexLitGeneric** | Masque les matériaux de modèles (shader VertexLitGeneric) : ils ne vont pas sur les brushes. Les textures outils restent. Enregistré dans `hidden_shaders` de `[browser]`. | désactivé |
| **Masquer les textures outils** | Masque les textures outils (sauf dans la catégorie **Outils**). Enregistré dans `hide_tool_textures`. | désactivé |
| Curseur de taille des vignettes | Taille des vignettes, de 48 à 256 points par pas de 8. Enregistré dans `thumbnail_size` de `[ui]`. | 96 |
| **Afficher l'alpha** | Dessine sur un damier les matériaux dont l'alpha est transparent en jeu. Enregistré dans `show_alpha`. | désactivé |

La ligne sous les filtres indique **N sur M matériaux**, et **N sélectionnés** avec un `X` pour vider la sélection multiple.

### Dossiers masqués

Certains dossiers de matériaux peuvent être masqués dans le navigateur (les textures outils et les matériaux de la map restent toujours visibles) :

- Clic droit sur un matériau, puis **Masquer le dossier ...** masque son dossier. La liste est `excluded_folders` de `[browser]` ; modifiez-la dans **Paramètres > Entités et navigateurs > Navigateur de textures > Dossiers masqués :** (tapez `dossier` ou `dossier/sous-dossier`, puis **Ajouter** ; `×` en retire un).
- **Masquer les dossiers exclus par les données du jeu** (même page, activé par défaut, `fgd_exclusions`) masque aussi les dossiers listés par `@MaterialExclusion` dans les FGD.

## La grille

| Action | Résultat |
|---|---|
| Clic | Fait du matériau le matériau actuel. |
| `Ctrl+clic` | Ajoute ou retire le matériau de la sélection multiple (utilisée par **Modifier le matériau**). |
| `Maj+clic` | Sélectionne la plage depuis le dernier matériau cliqué. |
| Double clic | Applique le matériau à la sélection (ou le donne à la clé d'entité qui attend un matériau). |
| Glisser sur une face de la vue 3D | L'applique à cette face (la face est soulignée pendant le survol). |
| Glisser dans une vue 2D | L'applique à toutes les faces des objets sélectionnés (ou aux faces sélectionnées). |
| Survol | Infobulle : nom complet, taille, shader, propriété de surface, mots-clés, particularités (texture outil, translucide, test alpha, eau, mélange) et source. |
| Clic droit | Menu contextuel (voir plus bas). |

Quand la souris est sur la grille (et qu'aucun champ texte n'a le focus), le clavier s'y déplace : les flèches, `Page préc.`, `Page suiv.`, `Début` et `Fin` changent le matériau actuel, avec `Maj` ils étendent la sélection multiple, et `Entrée` applique le matériau actuel à la sélection. Dès que la souris quitte la grille, ces touches reviennent aux outils (pour décaler la sélection, par exemple).

Les vignettes au nom rouge sont des matériaux manquants (damier violet et noir).

### Menu contextuel

| Élément | Effet |
|---|---|
| **Appliquer à la sélection** | Applique le matériau à la sélection. |
| **Sélectionner les faces qui l'utilisent** | Sélectionne toutes les faces qui l'utilisent et passe à l'outil de texture. |
| **Ajouter aux favoris** / **Retirer des favoris** | Bascule l'étoile. Les favoris sont enregistrés dans `favorites` de `[browser]`. |
| **Copier le nom** | Copie le nom du matériau dans le presse-papiers. |
| **Remplacer par celui-ci...** | Ouvre [Rechercher et remplacer des textures](find-replace-textures.md) avec le matériau actuel dans **Rechercher** et celui-ci dans **Remplacer par**. |
| **Ouvrir le dossier** | Ouvre le dossier du VMT, du VTF ou de l'image dans l'explorateur (seulement pour les fichiers sur le disque, pas dans les archives VPK ou GMA). |
| **Modifier le matériau...** | Ouvre l'[éditeur de matériaux](material-editor.md) sur ce matériau (ou sur la sélection multiple s'il en fait partie). |
| **Modifier dans l'éditeur de shaders** | Ouvre l'[éditeur de shaders](shader-editor.md) sur le matériau : sur son graphe enregistré s'il en a un à côté de ses fichiers, sinon sur sa texture de base. |
| **Masquer le dossier ...** | Masque le dossier du matériau dans le navigateur. |

## Dossiers personnalisés

La section repliable **Dossiers personnalisés (n)** liste vos dossiers de textures personnalisés avec leur case d'activation, leur nombre de matériaux, leur préfixe et leurs avertissements, ainsi que les boutons **Ajouter un dossier...**, **Actualiser** et **Importer des images...**. Voir [Dossiers de textures personnalisés](custom-texture-folders.md). Elle est repliée par défaut et ne s'ouvre que si vous cliquez dessus.

## Astuces

- Tapez quelques lettres de chaque partie du nom : `conc flo` trouve `concrete/concretefloor001a`.
- `is:alpha -is:translucent` trouve les matériaux découpés (grillages, feuillages).
- Mettez une étoile sur les matériaux que vous utilisez sans arrêt et gardez **Favoris** ouvert pendant que vous texturez.
- **Marquer** sur un matériau montre où la map l'utilise ; changez-les ensuite tous d'un coup dans le panneau **Édition des faces**.

## FAQ

**Une texture que j'ai ajoutée n'apparaît pas.** Cliquez sur **Actualiser** dans **Dossiers personnalisés** : le contenu du jeu et les dossiers personnalisés sont rechargés. Vérifiez aussi que son dossier n'est pas masqué et que **Masquer les textures outils** ou une puce ne la filtre pas.

**Pourquoi des matériaux manquent-ils dans la liste ?** Regardez les puces de source et de catégorie, les puces de propriétés (elles écrivent des mots `is:`), la liste **Shader :**, **Masquer VertexLitGeneric** et les dossiers masqués.

**Que signifie le chargement du Workshop ?** Pendant la lecture des addons du Workshop, la liste affiche **Chargement du workshop (N addons), la liste se complète à la fin...** : leurs matériaux apparaissent à la fin de la lecture.

**Puis-je ne voir que ce que tous les joueurs de Garry's Mod possèdent ?** Le navigateur de textures n'a pas cette option ; utilisez la puce de source **Jeu**. L'option **Contenu de Garry's Mod uniquement** concerne les navigateurs de modèles, de sons et de particules.

## Pages liées

- [Outil de texture](texture-tool.md)
- [Rechercher et remplacer des textures](find-replace-textures.md)
- [Dossiers de textures personnalisés](custom-texture-folders.md)
- [Éditeur de matériaux](material-editor.md)
- [Éditeur UV](uv-editor.md)
- [Configuration du jeu](../getting-started/game-setup.md)
- [Référence des paramètres](../reference/settings.md)
- [Référence des outils MCP](../../MCP.md) (en anglais : `search_materials`, `set_current_material`, `apply_material`, `get_material_info`, `get_material_preview`, `list_used_materials`, `find_material_usage`)

---
[Retour à l'index de la documentation](../README.md)
