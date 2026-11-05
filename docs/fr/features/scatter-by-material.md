[Documentation](../README.md) > Fonctionnalités > Semis par matériau | [English](../../en/features/scatter-by-material.md)

# Semis par matériau

Le semis par matériau place des props (herbe, rochers, buissons...) sur les surfaces des brushes et des displacements, et seulement là où la texture est celle que vous avez choisie. Sur un matériau de mélange (WorldVertexTransition et les autres mélanges lightmappés avec `$basetexture` et `$basetexture2`), l'alpha peint sur les sommets du displacement dit où chaque texture apparaît : l'herbe ne pousse donc que là où `grass1` domine, et les rochers que là où `dirt1` domine. Il étend le placement du [sprinkle](sprinkle-tool.md) : les mêmes entités, la même recherche de surface, mais pilotés par une liste d'assets, un nombre et un écart au lieu d'un pinceau.

## L'ouvrir

**Outils > Semis par matériau...**, ou l'action `scatter_by_material` (palette de commandes, MCP `run_action`). C'est une fenêtre ; elle ne change pas l'outil actif.

## L'utiliser

1. Sélectionnez les brushes ou displacements qui reçoivent les assets, ou cochez **Toute la carte**.
2. Ajoutez les assets un par un avec **Ajouter un asset** : le chemin du **Modèle** (`models/.../nom.mdl`) et la classe (`prop_static`, `prop_dynamic`...). **Depuis les props sélectionnés** ajoute les modèles des entités prop sélectionnées.
3. Pour chaque asset, choisissez où il peut apparaître :
   - **Texture** : un nom comparé au matériau, à son `$basetexture` (première couche) et à son `$basetexture2` (seconde couche), avec `*` et `?` permis (`grass1`, `nature/*`). La petite liste à côté du champ propose les textures trouvées sur les surfaces sélectionnées. Vide : partout.
   - **Mélange min** : la part de cette texture qui doit apparaître au point (0,5 par défaut, « dominante »). Sur un displacement, la première couche apparaît à `1 - alpha` et la seconde à `alpha`, interpolés entre les sommets sous le point. Une face d'un autre matériau montre sa texture en entier.
   - **Poids** : la fréquence de l'asset par rapport aux autres permis à ce point.
4. Choisissez le **Nombre**, l'**Écart** (distance minimale entre deux assets), la **Graine** (même graine, même résultat), la **Pente maximale**, et les options aléatoires : **Rotation aléatoire** (lacet), **Aligner sur la surface**, **Échelle** (un minimum et un maximum, écrits dans la clé `modelscale`, pour les jeux qui la lisent) et **Décalage** (un décalage le long de la normale de la surface, négatif enfonce l'asset dans le sol).
5. Cliquez **Aperçu** pour voir combien d'assets seraient placés, combien par asset, et pourquoi des endroits ont été écartés (pas de surface, trop pentu, trop proche, autre texture). **Générer** les place comme entités `prop_static` (ou la classe choisie) en une seule étape d'annulation.

Les positions sont aléatoires par défaut. Cochez **Caler les positions sur la grille** pour les mettre sur la grille active, avec sa rotation et ses pas personnalisés ; cela cale la position sur la surface, jamais sa hauteur.

Tant que les matériaux se chargent, l'aperçu le dit et réessaie tout seul.

## Remarques

- Chaque placement tombe du dessus de la zone sur la première surface ciblée ; les surfaces non ciblées sont ignorées, donc les props n'atterrissent pas sur un toit au-dessus du terrain.
- Le nombre est un objectif : moins d'assets sont placés quand la surface est petite, pentue, loin d'une texture voulue, ou quand l'écart est grand.
- Les modèles ne sont pas vérifiés dans les fichiers du jeu ; utilisez le [navigateur de modèles](model-browser.md) pour trouver les chemins.

## MCP

L'outil `sprinkle` fait la même chose : `items` (chacun avec `classname` ou `model`, `keyvalues`, `weight`, `texture`, `min_blend`), `count`, `spacing`, `seed`, la zone en `center` et `radius`, ou `min` et `max`, ou `solid_ids` (les brushes et displacements ciblés, dont les limites deviennent la zone), `random_yaw`, `align`, `max_slope`, `scale_min`, `scale_max`, `offset_min`, `offset_max`, `snap_to_grid` avec `view` (la grille utilisée), et `preview` pour compter sans placer. La réponse liste ce qui serait ou a été placé, par asset, et les endroits écartés par raison. Voir la [référence des outils MCP](../../MCP.md).

## Pages liées

- [Outil sprinkle](sprinkle-tool.md)
- [Outil displacement](displacement-tool.md)
- [Grilles](grids.md)
- [Navigateur de modèles](model-browser.md)
