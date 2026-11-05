[Documentation](../README.md) > Fonctionnalités > Aperçu des detail props | [English](../../en/features/detail-props-preview.md)

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

Voir aussi [Modes de rendu et options d'affichage](render-modes-display.md), [Semis par matériau](scatter-by-material.md).
