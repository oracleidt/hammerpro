[Documentation](../README.md) > Fonctionnalités > Aperçu des detail props | [English](../../en/features/detail-props-preview.md) | [Русский](../../ru/features/detail-props-preview.md)

# Aperçu des detail props

Les matériaux qui ont la clé `%detailtype` (les textures d'herbe de Garry's Mod et semblables) reçoivent des objets de détail à la compilation de la map : sprites d'herbe, petits modèles. Cet aperçu répartit les mêmes objets dans la vue 3D pendant que vous éditez, pour voir l'herbe sur chaque texture sans compiler.

## L'activer

- Le bouton **Aperçu des detail props (herbe)** de la barre d'outils (icône de feuille), à côté des boutons de brouillard.
- **Réglages > Rendu > Aperçu des detail props (herbe)**, réglage `render.detail_preview` (désactivé par défaut).
- L'action `toggle_detail_preview`, ou l'outil MCP `detail_preview` avec `enabled` à true ou false. `detail_preview` sans argument rapporte seulement l'état.

## Ce qui est dessiné

- Chaque displacement et chaque face de brush (brushes du monde et `func_detail`) dont le matériau a `%detailtype`, quand le `detail.vbsp` de la map définit un groupe de ce nom. Le fichier est la propriété `detailvbsp` du worldspawn (Propriétés de la map), sinon `detail.vbsp`, sinon `scripts/detail.vbsp` du contenu du jeu.
- Le placement suit VBSP : la densité du groupe par surface, l'alpha peint sur les sommets d'un displacement choisissant entre les groupes par leur `alpha`, `minAngle` et `maxAngle` selon la pente de la surface, `upright`, les poids `amount`. La même surface reçoit toujours les mêmes objets.
- Les entrées sprite sont des billboards découpés dans la feuille de sprites du `detailmaterial` du worldspawn (par défaut `detail/detailsprites`) avec leur rectangle, `spritesize` et `spriterandomscale` ; `sprite_shape` cross et tri sont dessinés en deux et trois quads croisés ; `sway` fait balancer le haut du sprite dans le temps (il suit l'option **Animer les matériaux**) ; `detailOrientation` est respecté. Les entrées modèle sont dessinées comme des modèles.
- Seules les surfaces autour de la caméra 3D sont remplies, les plus proches d'abord, jusqu'à la distance d'affichage du jeu (1200 unités avec un fondu sur les 400 dernières, ou les `fademindist` et `fademaxdist` d'un `env_detail_controller`), au plus 60000 sprites et 1500 modèles. Les objets sont générés quelques surfaces par image, pour que la caméra reste fluide, et dessinés en instances.
- La vue joueur dessine les detail props de la map compilée et masque cet aperçu.

## Lire l'état

L'outil MCP `detail_preview` renvoie le fichier lu et son nombre de types, le matériau des sprites, combien de surfaces utilisent un `%detailtype`, combien sont à portée, les nombres de sprites et de modèles, si les limites en ont coupé, et pour chaque matériau le type de détail qu'il nomme (`known` est false quand le fichier n'a pas ce groupe).

## Identique à la compilation

L'aperçu et le compilateur de Hammer Pro utilisent le même code de placement, prop par prop. Comme Valve VBSP, la compilation émet les detail props par face compilée : les faces de brush coupées par d'autres brushes et par le BSP deviennent plusieurs morceaux, chacun avec la graine de l'id de la face, dans l'ordre des faces compilées, et l'échelle aléatoire des sprites continue d'une face à l'autre. Un instant après chaque modification, l'aperçu découpe donc les faces exactement comme la compilation (avec les options de compilation que vous utiliserez : options BSP et arguments supplémentaires de la fenêtre Compiler, profil expert actif, jeu cible, compilation des objets cachés et cordons ; sur un fil en arrière plan, à nouveau dès qu'elles changent) et place les props sur les mêmes morceaux dans le même ordre. Tant que ce découpage n'est pas prêt, les faces de brush montrent la face entière ; les outils MCP indiquent `exact` true quand l'aperçu égale la compilation. Avec un compilateur externe (Valve VBSP ou un autre choisi dans les réglages Compilateurs), c'est lui qui découpe les faces : l'aperçu reste proche mais `exact` vaut false.

## Choisir où pousse l'herbe

- Peindre l'alpha du déplacement : sur les matériaux de mélange l'alpha choisit le groupe de détails par son `alpha`, comme VBSP. L'[outil de végétation](foliage-tool.md) en mode **Alpha des détails** le fait avec l'aperçu mis à jour en direct, comme le pinceau alpha de l'outil déplacement.
- Donner aux faces leur propre type : l'outil de végétation en mode **Détails par face**, ou l'outil MCP `set_face_detail_type`, règle un groupe de `detail.vbsp` ou `none` (aucun detail prop sur cette face) à la place du `%detailtype` du matériau. Il est enregistré comme clé de face `hp_detailtype`, respecté par le compilateur et cet aperçu, ignoré par Hammer.

## Vérifier sans dessiner

- `preview_detail_props` calcule ce que la compilation émettra (objets, sprites, modèles, par type et par surface, au delà de la limite de 65535 ou non, positions d'exemple), pour toute la carte, des solides ou une zone.
- `list_detail_types` liste les groupes de `detail.vbsp` avec leurs entrées, les matériaux qui utilisent chaque type et les faces qui ont leur propre type.

Voir aussi [Modes de rendu et options d'affichage](render-modes-display.md), [Semis par matériau](scatter-by-material.md).
