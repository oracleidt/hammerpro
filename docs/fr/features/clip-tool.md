[Documentation](../README.md) > Fonctionnalités > Outil de découpe | [English](../../en/features/clip-tool.md)

# Outil de découpe

L'outil de découpe (le clipping tool de Hammer) coupe les solides sélectionnés avec un plan. Vous définissez le plan par une ligne dans une vue 2D (le plan est perpendiculaire à cette vue, ou incliné par un troisième point), ou par trois points sur des surfaces dans la vue 3D. Le résultat est prévisualisé en direct avant d'être appliqué.

## Ouvrir l'outil

- **Outils > Outil de découpe**, la palette d'outils, ou `Maj+X`.
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

1. Cliquez trois points sur des faces de brushes. Les points s'alignent sur la grille dans le plan de la face. La barre d'état indique combien de points manquent.
2. Avec trois points, le plan est défini ; son côté avant est celui qui fait face à la caméra. Trois points alignés sont refusés (**Les trois points sont alignés : choisissez d'autres points**) et vous recommencez.
3. Les points se glissent ensuite : en 3D sur la surface sous la souris, ou dans les vues 2D. Le plan suit et garde son côté avant. Un déplacement qui alignerait les points est refusé.
4. Appuyez sur `Entrée` ou cliquez sur **Découper**. Un nouveau clic sur une surface après un plan complet commence une nouvelle série de points.

La vue 3D affiche les distances entre les points.

## Options et touches

| Option ou touche | Rôle | Par défaut |
|---|---|---|
| **Garder l'avant** / **Garder l'arrière** / **Garder les deux** | Morceaux à conserver. `Maj+X` (rechoisir l'outil) ou `Tab` pour alterner. | Garder les deux |
| **Découper** (`Entrée`) | Coupe les solides sélectionnés. Actif quand le plan traverse au moins un solide sélectionné. | |
| **Effacer** (`Échap`) | Oublie les points de découpe. | |
| **3 points** (`C`) | Mode trois points : après la ligne, cliquez dans une autre vue 2D pour incliner le plan. | désactivé |
| **Mesures** (`O`) | Affiche la longueur et l'angle de la ligne de découpe à côté d'elle. Enregistré dans `[brush] clip_measurements`. | activé |
| **Matériau actuel sur les coupes** | Les nouvelles faces de coupe reçoivent le matériau actuel au lieu de copier la face la plus proche du brush. Enregistré dans `[brush] clip_current_material`. | désactivé |
| **N solide(s) coupé(s)** | Nombre de solides sélectionnés que le plan traverse. | |

Ces deux réglages se trouvent aussi sur la page **Brushes** de la fenêtre Paramètres (**Afficher la longueur et l'angle de la ligne de découpe**, **Matériau actuel sur les faces de coupe**), ouverte par **Outils > Options...** (`Ctrl+,`).

## Ce que deviennent les solides

- Les solides que le plan ne traverse pas restent intacts ; les solides invalides sont ignorés. Si le plan ne traverse rien, vous obtenez **Le plan de découpe ne traverse pas les solides sélectionnés**.
- La découpe est une seule étape d'annulation, et la console affiche **N solide(s) découpé(s)**.
- Les solides d'une entité brush restent dans leur entité. Les overlays et les cubemaps qui listent des faces découpées les suivent comme dans Hammer : une face coupée en deux est remplacée par ses deux moitiés, une face supprimée est retirée de leur liste.
- Les faces de coupe copient la face la plus proche du solide, ou reçoivent le matériau actuel quand **Matériau actuel sur les coupes** est coché.

## Astuces

- Utilisez **Garder l'avant** ou **Garder l'arrière** pour rogner un brush d'un coup ; **Garder les deux** pour couper un mur à l'endroit d'une porte.
- Pour couper une pente précise, tracez la ligne en vue de face ou de côté avec les mesures affichées : l'angle s'affiche en degrés.
- L'outil MCP `clip_brushes` découpe avec un plan donné par trois points ou par une normale et une distance (voir la [référence des outils MCP](../../MCP.md)).

## FAQ

**Pourquoi le bouton Découper est grisé ?** Aucun plan n'est encore défini, ou le plan ne traverse aucun solide sélectionné.

**Tab déplace le focus clavier au lieu de changer de mode.** `Tab` ne change le mode que si aucun champ ou bouton n'avait le focus clavier. Cliquez d'abord dans une vue, ou appuyez à nouveau sur `Maj+X`.

**Quel côté est l'avant ?** En 2D, le côté vers lequel pointe la petite flèche de la ligne. En 3D, le côté qui faisait face à la caméra quand le troisième point a été posé.

## Pages liées

- [Outil bloc](block-tool.md)
- [Outil vertex](vertex-tool.md)
- [Outil extruder / creuser](extrude-tool.md)
- [Opérations sur les brushes](brush-operations.md)
- [Outil de sélection](selection-tool.md)
- [Grilles](grids.md)
- [Référence des paramètres](../reference/settings.md)

---
[Retour à l'index de la documentation](../README.md)
