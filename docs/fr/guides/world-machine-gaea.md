[Documentation](../README.md) > Guides > World Machine et Gaea | [English](../../en/guides/world-machine-gaea.md) | [Русский](../../ru/guides/world-machine-gaea.md)

# World Machine et Gaea : cartes de hauteur et masques de blend

Ce guide amène un terrain fait dans World Machine ou Gaea dans une map Source, avec toute la précision des hauteurs et ses couches de texture, puis le ramène. Chaque étape existe aussi comme outil MCP (nommé entre parenthèses).

## 1. Exporter depuis l'outil de terrain

- **Hauteurs.** Exportez une carte de hauteur en flottant 32 bits : R32 (`.r32`, flottant brut) ou EXR dans World Machine, EXR ou R32 dans Gaea. Un PNG ou RAW (`.r16`) 16 bits marche aussi (65536 niveaux). Évitez les images 8 bits : elles donnent des marches visibles.
- **Taille.** Préférez 2^n+1 pixels (513, 1025, 2049) : les pixels du bord sont alors exactement sur les bords du terrain, et **Un sommet par pixel** peut les utiliser sans rééchantillonnage. Un export 1024 ou 2048 marche aussi : choisissez **Grille des pixels > Centre** si votre outil traite les pixels comme des surfaces.
- **Exports en tuiles.** Les tuiles World Machine de 2^n+1 pixels partagent leurs pixels de bord. Gardez-les en fichiers séparés : Hammer Pro les assemble.
- **Couches.** Exportez les couches de texture en splat map (une image RVBA, une couche par canal) ou en un masque gris par couche, en PNG 8 ou 16 bits, TIFF ou EXR, avec la même taille et la même étendue que la carte de hauteur.

## 2. Importer les hauteurs

Ouvrez **Fichier > Importer > Importer une carte de hauteur...** (`import_heightmap`) :

1. **Carte de hauteur** : le fichier `.r32` ou `.exr`. Pour un `.r32` sans taille, la taille est déduite du fichier (carré). Pour un export en tuiles, ouvrez plutôt **Export en tuiles** et **Ajouter des tuiles...**.
2. **Valeurs des pixels** : **Absolues** si le fichier contient de vraies hauteurs. **Unités par valeur** : 1 si les hauteurs sont déjà en unités, 52,49 pour des mètres (1 unité vaut environ 1,905 cm). Avec **Étirer**, les valeurs minimale et maximale deviennent plutôt **Hauteur minimale** et **Hauteur maximale**.
3. **Taille X** et **Taille Y** : la taille du terrain en unités. **Un sommet par pixel** avec la **Puissance des tuiles** 4 change une image 1025 x 1025 en 64 x 64 tuiles, un sommet par pixel. Avec moins de sommets, l'échantillonnage bicubique garde une forme douce.
4. **Matériau** : un matériau de blend : `WorldVertexTransition` pour deux textures, `Lightmapped_4WayBlend` pour quatre.
5. **Importer**. La notification donne les hauteurs utilisées. Le terrain est une étape d'annulation et ses tuiles sont sélectionnées.

## 3. Importer les couches de texture

Les nouvelles tuiles toujours sélectionnées, ouvrez **Fichier > Importer > Importer des masques de blend...** (`import_blend_masks`) :

1. **Fichier 1** : la splat map (ou **Ajouter un fichier** pour un masque par couche).
2. **Même étendue que la carte de hauteur** : les masques collent alors exactement au terrain (MCP : donnez `size_x`, `size_y`, `origin` et `centered` à false renvoyés par `import_heightmap`).
3. **Écrire dans** : **Auto** peint les faces Lightmapped_4WayBlend avec les lignes 4 textures et les autres faces avec l'alpha.
4. Associez les canaux : **Texture 1** à **Texture 4** pour les quatre couches d'un matériau Lightmapped_4WayBlend (par exemple la couche de base sur **Le reste (1 moins les autres)**), ou **Alpha : Canal** pour un matériau WorldVertexTransition (le blanc montre `$basetexture2`). **Inverser** et **Niveaux** ajustent un canal.
5. Gardez **Normaliser à une somme de 1** et **Couverture** pour les splat maps : chaque texture couvre alors sa part en jeu.
6. Activez **Aperçu dans les vues 3D** pour voir le blend en direct dans la vue Éclairée ou Cuite, puis **Appliquer** (une étape d'annulation).

## 4. Vérifier et compiler

- Les vues Éclairée et Cuite dessinent le blend comme le jeu. L'[outil displacement](../features/displacement-tool.md) peut peindre par dessus (**Peindre l'alpha**, boutons de texture du blend 4 textures).
- Compilez comme d'habitude : l'alpha va dans les sommets des displacements, les lignes 4 textures dans le lump multiblend des displacements, que Garry's Mod, Source 2013 et CS:GO lisent.

## 5. Retour vers l'outil de terrain

- **Fichier > Exporter > Exporter une carte de hauteur...** (`export_heightmap`) avec un fichier `.r32` ou `.exr` écrit les vraies hauteurs (valeurs **Auto** ou **Absolues**), un pixel par sommet.
- **Fichier > Exporter > Exporter les masques de blend...** (`export_blend_masks`) écrit les couches en PNG RVBA 16 bits, en EXR flottant, l'alpha, ou une image grise par texture, sur la même grille de pixels.
- Chargez-les dans World Machine ou Gaea, modifiez, et réimportez avec les mêmes réglages : l'aller-retour garde exactement les hauteurs flottantes et le blend aux 6 décimales du VMF près.

## Voir aussi

- [Terrain](../features/terrain.md) : toutes les options des fenêtres d'import et d'export.
- [Sculpter des displacements](sculpt-displacements.md) : peindre et sculpter à la main.
- [Référence des outils MCP](../../MCP.md).
