[Documentation](../README.md) > Fonctionnalités > Outil de découpe | [English](../../en/features/clip-tool.md) | [Русский](../../ru/features/clip-tool.md)

# Outil de découpe

L'outil de découpe (le clipping tool de Hammer) coupe les solides sélectionnés avec un plan. Vous définissez le plan par une ligne dans une vue 2D (le plan est perpendiculaire à cette vue, ou incliné par un troisième point), ou directement dans la vue 3D : deux points sur une face, trois points sur des surfaces, un plan d'axe ou le plan d'une face. Le résultat est prévisualisé en direct avant d'être appliqué.

## Ouvrir l'outil

- **Outils > Outil actif > Outil de découpe**, la palette d'outils, ou `Maj+X`.
- Sélectionnez d'abord les solides à découper : seuls les solides sélectionnés sont coupés (sinon **Sélectionnez d'abord les solides à découper**).

## Modes : les morceaux à conserver

| Mode | Résultat |
|---|---|
| **Garder les deux** | Chaque solide traversé devient deux solides. Le morceau arrière garde l'id du solide, le morceau avant est un nouveau solide (sélectionné si l'original l'était). |
| **Garder l'avant** | Seul le morceau du côté avant du plan reste. |
| **Garder l'arrière** | Seul le morceau du côté arrière reste. |

Le mode par défaut est **Garder les deux**. Changez-le avec les boutons de la barre d'options, ou faites-le tourner comme dans Hammer : choisissez à nouveau l'outil (`Maj+X` quand l'outil de découpe est actif), ou appuyez sur `Tab` (`Maj+Tab` tourne dans l'autre sens). L'ordre est : les deux, l'avant, l'arrière.

Dans l'aperçu, le morceau avant est vert, le morceau arrière bleu, et un morceau qui sera supprimé est rouge.

## Découper dans une vue 2D

1. Glissez dans une vue 2D pour tracer la ligne de découpe. Les deux extrémités s'alignent sur la grille (sur la grille attachée à la sélection si elle en a une) ; maintenez `Alt` pour couper sans magnétisme. La ligne est placée à la profondeur du centre de la sélection.
2. Le plan passe par la ligne, perpendiculaire à la vue. Une petite flèche au milieu de la ligne indique le côté avant (à droite du sens dans lequel vous avez tracé, vu à l'écran).
3. Ajustez la ligne :
   - glissez une de ses deux poignées pour déplacer cette extrémité,
   - `Ctrl` + glisser une poignée ou la ligne elle-même déplace les deux extrémités ensemble (le premier point s'aligne, le second suit),
   - `Maj` + glisser commence toujours une nouvelle ligne, même sur une poignée.
4. Appuyez sur `Entrée` ou cliquez sur **Découper**.

### Mode trois points (plan incliné)

Appuyez sur `C` ou cliquez sur **3 points** pour activer le mode trois points (comme dans Hammer++). Après avoir tracé la ligne, cliquez dans une autre vue 2D pour poser un troisième point : le plan qui passe par la ligne s'incline pour passer par ce point. Le troisième point se glisse dans sa vue, et l'angle d'inclinaison (**Inclinaison N°**) s'affiche à côté. `Maj` + glisser dans l'autre vue y commence plutôt une nouvelle ligne. Si le troisième point est sur la ligne, le plan reste perpendiculaire. Appuyez à nouveau sur `C` pour revenir au mode deux points (le troisième point est alors ignoré).

## Découper dans la vue 3D

Vous pouvez couper directement dans la vue 3D, comme le découpage 3D de Hammer++. Les clics vont d'abord sur les surfaces des solides sélectionnés, même quand un autre brush est devant ; sans solide sélectionné sous la souris, la surface la plus proche est utilisée.

### Magnétisme

Le point sous la souris s'affiche avant le clic :

- **blanc** : aligné sur un coin de la face sous la souris,
- **cyan** : aligné sur le milieu d'une arête de cette face,
- **jaune** : aligné sur la grille dans le plan de la face (la grille attachée à la sélection si elle en a une).

Les coins et les milieux d'arêtes l'emportent quand la souris est à environ 10 pixels d'eux. Maintenez `Alt` pour couper sans aucun magnétisme.

### Deux points sur une face (la coupe habituelle)

1. Cliquez un premier point sur une face. Une ligne suit la souris et le contour du futur plan s'affiche.
2. Cliquez un deuxième point. Le plan passe par les deux points et est perpendiculaire à la première face : c'est la ligne de coupe tracée sur la face. L'avant est à droite du sens dans lequel vous avez tracé, vu depuis la caméra (comme la ligne 2D). Une petite flèche partant du plan montre le côté avant.
3. Appuyez sur `Entrée` ou cliquez sur **Découper**.

### Trois points (plan libre)

Après les deux points, cliquez un troisième point sur n'importe quelle surface (une autre face, un autre brush) : le plan passe par les trois points et garde son côté avant. Un troisième point aligné avec les deux premiers est refusé (**Les trois points sont alignés : choisissez d'autres points**) et les deux premiers restent. Un troisième point sur la même face que les deux premiers est refusé aussi, car le plan serait la face elle-même et ne couperait rien.

### Plans rapides

| Clic | Plan |
|---|---|
| `Maj` + clic | Plan d'axe passant par le point cliqué, perpendiculaire à un axe du monde. Choisissez l'axe avec les boutons **Plan d'axe** ou appuyez sur `X` pour alterner **Auto**, **X**, **Y**, **Z**. **Auto** prend l'axe le plus dans la direction de la vue, hors celui de la face cliquée, pour que la ligne de coupe traverse l'écran (une coupe horizontale sur un mur vu de face, une coupe en travers du sol que vous regardez). Changer d'axe met à jour un plan déjà posé de cette façon. |
| `Ctrl` + clic | Le plan de la face cliquée, pour couper les autres solides sélectionnés au ras d'une face existante. Son avant est l'extérieur de cette face. |

Tant que `Maj` ou `Ctrl` est maintenu, le plan candidat sous la souris est dessiné avant le clic. Le plan d'une coupe rapide a un point ; glissez-le (en 3D ou dans une vue 2D) pour déplacer le plan le long de sa normale.

### Ajuster et appliquer

- Glissez un point pour le déplacer : en 3D sur la surface sous la souris, ou dans les vues 2D. Le plan suit et garde son côté avant. Déplacer le premier point sur une autre face rend la coupe perpendiculaire à cette face. Un déplacement qui rendrait le plan impossible est refusé.
- Vous pouvez aussi appuyer sur une surface et glisser avant de relâcher : le nouveau point suit la souris jusqu'au relâchement.
- L'aperçu montre le plan en quadrilatère semi transparent et les deux moitiés de chaque solide traversé en couleur (avant vert, arrière bleu, supprimé rouge), dans les vues 3D et 2D.
- `Entrée` ou **Découper** coupe ; les points 3D sont alors effacés et le clic suivant commence de nouveaux points. `Échap` ou **Effacer** oublie les points.
- La vue 3D affiche les distances entre les points quand **Mesures** est coché.

### Synchronisé avec les vues 2D

Un plan créé en 3D s'affiche dans les vues 2D : ses points y ont des poignées que vous pouvez glisser, les solides traversés montrent leurs deux moitiés colorées, et quand le plan est perpendiculaire à une vue, sa trace est dessinée en pointillés avec une flèche vers le côté avant. Une ligne de découpe tracée dans une vue 2D s'affiche en 3D avec son plan et ses points.

## Options et touches

| Option ou touche | Rôle | Par défaut |
|---|---|---|
| **Garder l'avant** / **Garder l'arrière** / **Garder les deux** | Morceaux à conserver. `Maj+X` (rechoisir l'outil) ou `Tab` pour alterner. | Garder les deux |
| **Découper** (`Entrée`) | Coupe les solides sélectionnés. Actif quand le plan traverse au moins un solide sélectionné. | |
| **Effacer** (`Échap`) | Oublie les points de découpe. | |
| **3 points** (`C`) | Mode trois points : après la ligne, cliquez dans une autre vue 2D pour incliner le plan. | désactivé |
| **Plan d'axe : Auto / X / Y / Z** (`X`) | Axe du monde de la coupe `Maj` + clic dans la vue 3D. `X` pour alterner. | Auto |
| `Maj` + clic (3D) | Plan d'axe passant par le point cliqué. | |
| `Ctrl` + clic (3D) | Plan de la face cliquée. | |
| `Alt` | Sans magnétisme (grille, coins et milieux d'arêtes). | |
| **Mesures** (`O`) | Affiche la longueur et l'angle de la ligne de découpe à côté d'elle. Enregistré dans `[brush] clip_measurements`. | activé |
| **Matériau actuel sur les coupes** | Les nouvelles faces de coupe reçoivent le matériau actuel au lieu de copier la face la plus proche du brush. Enregistré dans `[brush] clip_current_material`. | désactivé |
| **N solide(s) coupé(s)** | Nombre de solides sélectionnés que le plan traverse. | |

Ces deux réglages se trouvent aussi sur la page **Brushes** de la fenêtre Paramètres (**Afficher la longueur et l'angle de la ligne de découpe**, **Matériau actuel sur les faces de coupe**), ouverte par **Outils > Options...** (`Ctrl+,`).

## Ce que deviennent les solides

- Les solides que le plan ne traverse pas restent intacts ; les solides invalides sont ignorés. Une coupe qui laisserait un morceau de moins de 0,25 unité d'épaisseur laisse le solide entier, pour ne jamais créer de brush lamelle. Si le plan ne traverse rien, vous obtenez **Le plan de découpe ne traverse pas les solides sélectionnés**.
- La découpe est une seule étape d'annulation, et la console affiche **N solide(s) découpé(s)**.
- Les solides d'une entité brush restent dans leur entité. Les overlays et les cubemaps qui listent des faces découpées les suivent comme dans Hammer : une face coupée en deux est remplacée par ses deux moitiés, une face supprimée est retirée de leur liste.
- Les faces de coupe copient la face la plus proche du solide, ou reçoivent le matériau actuel quand **Matériau actuel sur les coupes** est coché.

## Astuces

- Utilisez **Garder l'avant** ou **Garder l'arrière** pour rogner un brush d'un coup ; **Garder les deux** pour couper un mur à l'endroit d'une porte.
- Pour couper une pente précise, tracez la ligne en vue de face ou de côté avec les mesures affichées : l'angle s'affiche en degrés.
- Pour couper un mur à la hauteur d'une porte, `Maj` + clic sur le mur à cette hauteur avec l'axe sur **Auto** ou **Z**.
- Pour couper plusieurs brushes au ras d'une face, sélectionnez-les et `Ctrl` + clic sur cette face.
- L'outil MCP `clip_brushes` découpe avec un plan donné par trois points, par un point et une normale, ou par une normale et une distance, avec le mode de conservation et le matériau des faces de coupe (voir la [référence des outils MCP](../../MCP.md)). `set_tool_options` avec l'outil `clip` règle `keep`, `three_point` et `axis`, et le réglage `brush.clip_current_material` choisit le matériau des faces de coupe.

## FAQ

**Pourquoi le bouton Découper est grisé ?** Aucun plan n'est encore défini, ou le plan ne traverse aucun solide sélectionné.

**Tab déplace le focus clavier au lieu de changer de mode.** `Tab` ne change le mode que si aucun champ ou bouton n'avait le focus clavier. Cliquez d'abord dans une vue, ou appuyez à nouveau sur `Maj+X`.

**Quel côté est l'avant ?** En 2D, le côté vers lequel pointe la petite flèche de la ligne. En 3D, le côté vers lequel pointe la petite flèche partant du plan : à droite de la ligne pour deux points (vu depuis la caméra), le côté face à la caméra pour un plan d'axe ou trois points posés d'un coup, l'extérieur de la face pour un plan de face.

**Mon clic tombe sur le brush derrière.** Les clics privilégient les solides sélectionnés. Sélectionnez le brush à couper, ou celui sur lequel vous voulez cliquer.

**`X` ne change pas l'axe.** Seulement quand l'outil de découpe est actif et qu'aucun champ de texte n'a le focus clavier. Cliquez d'abord dans une vue.

## Pages liées

- [Outil tranche](slice-tool.md)
- [Outil bloc](block-tool.md)
- [Outil vertex](vertex-tool.md)
- [Outil extruder / creuser](extrude-tool.md)
- [Opérations sur les brushes](brush-operations.md)
- [Outil de sélection](selection-tool.md)
- [Grilles](grids.md)
- [Référence des paramètres](../reference/settings.md)

---
[Retour à l'index de la documentation](../README.md)
