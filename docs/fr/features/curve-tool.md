[Documentation](../README.md) > Fonctionnalités > Outil courbe | [English](../../en/features/curve-tool.md)

# Outil courbe

L'outil courbe construit des murs courbes, des rambardes, des bordures, des tuyaux vus de dessus ou des rampes vues de côté avec de simples brushes en forme de boîte. Vous dessinez un tracé de n'importe quelle forme dans une vue 2D, vous choisissez en combien de morceaux il est coupé et leur épaisseur, et l'outil crée un brush convexe à 6 faces par morceau, qui suit la courbe avec des joints en onglet : deux brushes voisins partagent exactement leur face de jonction, sans trou ni chevauchement.

## L'ouvrir

- **Outils > Outil courbe**, la palette d'outils (groupe création, à côté de l'outil polygone), ou `Maj+U`.
- L'outil dessine dans les vues 2D seulement ; les clics dans la vue 3D sont ignorés. La vue 3D montre l'aperçu en direct.

## Dessiner la courbe

1. Dans une vue 2D, **cliquez** des points l'un après l'autre, ou **faites glisser** pour un tracé à main levée (un point est ajouté chaque fois que la souris a avancé d'environ 24 pixels, au moins deux pas de grille). Les points s'aimantent à la grille active : en grille 8, chaque point tombe sur un multiple de 8. Maintenez `Alt` pour placer des points sans aimantation.
2. La courbe est dans le plan de cette vue : tracée dans la vue de dessus elle est horizontale, tracée dans la vue de face ou de côté elle est verticale.
3. Pendant le tracé, l'aperçu montre déjà les brushes avec les subdivisions et l'épaisseur actuelles : dans la vue du tracé sous forme de contours le long de la courbe lissée, dans les autres vues 2D et dans la vue 3D sous forme de brushes.
4. **Faites glisser un point** pour le déplacer. `Retour arrière` (ou **Retirer le point**) retire le dernier point, `Échap` annule la courbe. Une courbe compte au plus 1024 points.

## Valider et choisir les morceaux

Appuyez sur `Entrée`, double-cliquez, ou cliquez sur **Valider** dès que la courbe a au moins 2 points. Une fenêtre **Courbe** s'ouvre (les mêmes champs restent dans la barre d'options) :

| Champ | Rôle | Plage | Par défaut |
|---|---|---|---|
| **Subdivisions** | Nombre de brushes le long de la courbe. N'importe quel nombre entier convient ; les boutons `÷2` et `×2` passent par les valeurs habituelles 2, 4, 8, 16... | 2 à 4096 | 16 |
| **Épaisseur** | Largeur des brushes en travers de la courbe, centrée sur le tracé. | 1 à 1024 unités | 16 |
| **Hauteur** | Taille des brushes dans la profondeur de la vue (Z pour une courbe tracée dans la vue de dessus). | 1 à 16384 unités | selon la sélection, voir plus bas |

L'aperçu suit chaque changement immédiatement. Les points se déplacent encore à cette étape. Appuyez sur `Entrée` (ou **Créer**) pour construire les brushes, sur `Échap` (ou **Retour**) pour revenir au tracé.

Comment la courbe est coupée : une spline lisse (Catmull-Rom centripète) passe par chaque point placé, puis elle est coupée en morceaux de même longueur. Avec 2 points, la courbe est une droite coupée en parts égales.

## Profondeur et hauteur

Comme pour l'outil bloc, les brushes prennent leur étendue dans la profondeur de la vue depuis les limites de la sélection, sinon depuis le dernier objet créé par l'outil bloc, polygone ou courbe, sinon de 0 à 64 unités. Dans les autres vues 2D la courbe apparaît comme une bande : faites glisser une de ses extrémités pour changer l'étendue, ou son intérieur pour la déplacer. Le champ **Hauteur** change le haut de l'étendue.

## Barre d'options

| Option | Rôle | Par défaut |
|---|---|---|
| **Subdivisions**, **Épaisseur**, **Hauteur** | Comme dans la fenêtre Courbe. | 16, 16, selon la sélection |
| **Échelle** | Échelle de texture des nouvelles faces (0,01 à 64). | réglage **Échelle de texture par défaut** |
| Créer comme | **Brush du monde**, **func_detail** ou toute classe d'entité brush du FGD. Avec une classe, tous les brushes vont dans une seule nouvelle entité brush. | Brush du monde |
| Matériau actuel | Matériau posé sur chaque face, comme l'outil bloc. Cliquez dessus pour afficher le navigateur de textures. | le matériau actuel |
| Punaise (rester actif) | Garder l'outil courbe actif après la création. Désactivée, l'outil de sélection revient. | activée |
| **Retirer le point**, **Valider** / **Créer** | Comme `Retour arrière`, puis `Entrée`. | |
| Résumé | **N point(s)**, puis **N brushes, longueur L**, ou en rouge pourquoi la courbe ne peut pas être construite (survolez-le pour les détails). | |

Les nouveaux brushes deviennent la sélection ; toute la courbe tient en une seule étape d'annulation.

## Quand la courbe est refusée

- **Des segments se replient dans un virage serré** (en rouge) : à l'intérieur d'un virage plus serré que la moitié de l'épaisseur, les morceaux se croiseraient. Baissez l'épaisseur ou les subdivisions, ou déplacez les points pour ouvrir le virage. Un changement de direction de plus de 165 degrés environ entre deux morceaux est refusé de la même façon.
- **Des segments sont trop courts** : un morceau ferait moins d'une unité. Baissez les subdivisions ou tracez une courbe plus longue.
- **La courbe sort de la carte** : un coin de brush dépasserait 16384 unités.

## Astuces

- Tracez les routes et les murs dans la vue de dessus, les arches et les rampes dans la vue de face ou de côté.
- Peu de clics donnent les courbes les plus douces : la spline se charge de l'arrondi. Un tracé à main levée sur une grille fine suit davantage la main, tremblements compris.
- 16 à 64 subdivisions suffisent pour la plupart des courbes ; un nombre élevé fait beaucoup de petits brushes, qui coûtent du temps de compilation. Mettez les courbes décoratives dans un `func_detail` avec **Créer comme**.
- Les joints sont en onglet, donc les coins extérieurs ne tombent pas sur la grille. Ce sont quand même des plans propres partagés par les deux voisins, et la carte compile sans fuite ni brush invalide.
- L'outil MCP `create_curve_brushes` construit les mêmes brushes à partir d'une liste de points, et `set_tool_options` avec l'outil `curve` règle `subdivisions`, `thickness`, `height`, `tex_scale`, `create_as` et `keep_active` (voir la [référence des outils MCP](../../MCP.md)).

## FAQ

**Pourquoi certains morceaux sont rouges ?** Ils se replient dans un virage trop serré pour l'épaisseur. Voir plus haut.

**Puis-je faire un anneau fermé ?** Pour un anneau parfait, utilisez le tore ou l'arche de l'outil bloc. Avec l'outil courbe, placez le dernier point à côté du premier.

**Puis-je obtenir des cylindres ou des coins ?** Non : l'outil courbe ne fait que des brushes en forme de boîte, à 6 faces chacun, les brushes les plus robustes pour le compilateur.

## Pages liées

- [Outil bloc](block-tool.md)
- [Outil polygone](polygon-tool.md)
- [Outil vertex](vertex-tool.md)
- [Grilles](grids.md)
- [Raccourcis clavier](keyboard-shortcuts.md)

---
[Retour à l'index de la documentation](../README.md)
