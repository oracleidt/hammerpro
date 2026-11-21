[Documentation](../README.md) > Fonctionnalités > Outil courbe | [English](../../en/features/curve-tool.md) | [Русский](../../ru/features/curve-tool.md)

# Outil courbe

L'outil courbe construit des murs courbes, des rambardes, des bordures, des tuyaux vus de dessus ou des rampes vues de côté avec de simples brushes en forme de boîte. Vous dessinez un tracé de n'importe quelle forme dans une vue 2D, vous choisissez en combien de morceaux il est coupé et leur épaisseur, et l'outil crée un brush convexe à 6 faces par morceau (plus de brushes par morceau avec un profil bordures ou fossé, voir plus bas), qui suit la courbe avec des joints en onglet : deux brushes voisins partagent exactement leur face de jonction, sans trou ni chevauchement.

## L'ouvrir

- **Outils > Outil actif > Outil courbe**, la palette d'outils (groupe création, à côté de l'outil polygone), ou `Maj+U`.
- L'outil dessine dans les vues 2D ; la vue 3D montre l'aperçu en direct, ses poignées de points se posent sur les surfaces et les sommets, et **Choisir sur les faces** y fonctionne aussi (voir plus bas).

## Dessiner la courbe

1. Dans une vue 2D, **cliquez** des points l'un après l'autre, ou **faites glisser** pour un tracé à main levée (un point est ajouté chaque fois que la souris a avancé d'environ 24 pixels, au moins deux pas de grille). Les points s'aimantent à la grille active : en grille 8, chaque point tombe sur un multiple de 8. Maintenez `Alt` pour placer des points sans aimantation.
2. La courbe est dans le plan de cette vue : tracée dans la vue de dessus elle est horizontale, tracée dans la vue de face ou de côté elle est verticale.
3. Pendant le tracé, l'aperçu montre déjà les brushes avec les subdivisions et l'épaisseur actuelles : dans la vue du tracé sous forme de contours le long de la courbe lissée, dans les autres vues 2D et dans la vue 3D sous forme de brushes.
4. **Faites glisser un point** pour le déplacer. `Retour arrière` (ou **Retirer le point**) retire le dernier point, `Échap` annule la courbe. Une courbe compte au plus 1024 points.
5. Pendant le tracé ou l'ajustement de la courbe, `Maj` + molette change les **Subdivisions** (d'un huitième de la valeur par cran, au moins 1, de 2 à 4096) ; le nombre de brushes de l'étiquette suit. La molette seule zoome toujours.

## Choisir sur les faces : arcs entre deux faces

Pour placer un arc, un encadrement de porte cintré ou un linteau de fenêtre exactement dans une ouverture, activez **Choisir sur les faces** dans la barre d'options. Au lieu de dessiner des points, vous choisissez les deux côtés de l'ouverture puis le sommet de l'arc :

1. **Cliquez la face de départ** : une face latérale de l'ouverture, par exemple la face intérieure du montant gauche de la porte, dans la vue 3D ou dans une vue 2D où la face est vue par la tranche (une ligne). La face survolée est entourée en bleu avec le point aimanté. La hauteur où l'arc commence s'aimante aux arêtes de la face (son arête haute est la ligne de naissance habituelle), sinon à la grille ; maintenez `Alt` pour la placer librement.
2. **Cliquez la face opposée**, où l'arc se termine. L'étiquette montre la portée entre les deux faces.
3. **Bougez la souris** pour régler le sommet de l'arc (l'étiquette montre sa flèche), dans la vue 3D ou une vue de face ou de côté, aimanté à la grille. **Cliquez** (ou relâchez un glissement) pour le fixer, ou appuyez sur `Entrée` pour garder le demi cercle par défaut.

L'arc est généré exactement entre les faces : sa portée est la distance entre les plans des deux faces, sa profondeur est celle du mur (la partie où les deux faces se recouvrent), et il se tient dans le plan de l'ouverture. Ses deux extrémités sont coupées à plat le long des faces, elles reposent donc sur les montants sans vide ni chevauchement. La liste **Ajustement** choisit quel bord de l'arc touche les faces :

| Ajustement | Résultat |
|---|---|
| **Bord intérieur sur les faces** | L'intérieur de l'arc prolonge les faces : l'ouverture garde sa largeur et l'arc repose sur les montants, comme un arc de pierre sur ses piliers. Par défaut. |
| **Centre sur les faces** | Le milieu de l'épaisseur de l'arc est sur les faces. |
| **Bord extérieur sur les faces** | L'extérieur de l'arc touche les faces : l'arc tient dans l'ouverture et arrondit son haut. |

L'**Épaisseur** est la largeur de la bande de l'arc ; la changer, ou changer l'ajustement, reconstruit l'arc sur les mêmes faces. Ensuite la fenêtre Courbe s'ouvre comme d'habitude : changez les subdivisions, le profil ou faites glisser des points, et `Entrée` crée les brushes en une seule étape d'annulation. `Retour arrière` revient d'une étape (choisir à nouveau la face de fin, puis la face de départ), `Échap` annule. Après la création, l'outil reste en mode Choisir sur les faces pour l'ouverture suivante ; désactivez le bouton pour dessiner des points à nouveau.

Les faces doivent être verticales et orientées selon X ou Y (les côtés d'une ouverture dans un mur non tourné) ; un sol, un plafond ou un mur tourné est refusé avec un message.

## Aimantation des points

- Dans les vues 2D, les points cliqués ou glissés s'aimantent aux sommets des brushes proches de la souris (à quelques pixels), sinon à la grille.
- Dans la vue 3D, les points de contrôle de la courbe sont dessinés comme des poignées. Faites en glisser un pour le poser sur une surface : il s'aimante aux sommets de la face sous la souris, sinon à la grille sur cette face ; hors de toute surface il se déplace dans le plan de la courbe. Un clic sur une surface ajoute un point pendant le tracé, `Maj+clic` en insère un et `Ctrl+clic` en retire un, comme en 2D.
- Maintenez `Alt` pour couper l'aimantation.

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
| **Profil**, tailles des bordures et du fossé, **Roulis début**, **Roulis fin**, **Dévers auto**, **Limite auto** | Section et dévers, voir plus bas. | Plat, sans dévers |
| **Échelle** | Échelle de texture des nouvelles faces (0,01 à 64). | réglage **Échelle de texture par défaut** |
| Créer comme | **Brush du monde**, **func_detail** ou toute classe d'entité brush du FGD. Avec une classe, tous les brushes vont dans une seule nouvelle entité brush. | Brush du monde |
| Matériau actuel | Matériau posé sur chaque face, comme l'outil bloc. Cliquez dessus pour afficher le navigateur de textures. | le matériau actuel |
| Punaise (rester actif) | Garder l'outil courbe actif après la création. Désactivée, l'outil de sélection revient. | activée |
| **Retirer le point**, **Valider** / **Créer** | Comme `Retour arrière`, puis `Entrée`. | |
| Résumé | **N point(s)**, puis **N brushes, longueur L**, ou en rouge pourquoi la courbe ne peut pas être construite (survolez-le pour les détails). | |

Les nouveaux brushes deviennent la sélection ; toute la courbe tient en une seule étape d'annulation.

## Section et dévers

La même fenêtre Courbe et la même barre d'options contiennent la section et le dévers. Ils s'appliquent à toute la courbe.

| Champ | Rôle | Plage | Par défaut |
|---|---|---|---|
| **Profil** | **Plat** : un brush par morceau. **Bordures** : la route plus une bordure surélevée de chaque côté, 3 brushes par morceau. **Fossé** : un canal en V, 2 brushes par morceau. | | Plat |
| **Largeur bordure** | Profil bordures : largeur de chaque bordure. Les bordures s'ajoutent à l'extérieur de l'épaisseur, la largeur totale est donc l'épaisseur plus deux bordures. | 1 à 256 unités | 8 |
| **Hauteur bordure** | Profil bordures : de combien les bordures dépassent la route. | 0 à 256 unités | 8 |
| **Profondeur fossé** | Profil fossé : de combien le milieu du V s'enfonce sous les deux bords. Elle doit rester au moins 1 unité sous la hauteur. | 0 à 512 unités | 8 |
| **Roulis début**, **Roulis fin** | Dévers : l'angle de roulis en degrés au début et à la fin de la courbe, qui varie régulièrement le long de la courbe. Un angle positif relève le côté gauche quand on suit la courbe du premier au dernier point. | -60 à 60 degrés | 0 |
| **Dévers auto** | Dévers automatique, proportionnel à la courbure : le roulis en degrés pour un virage de rayon 1000 unités (un virage de rayon 500 en reçoit le double). La route penche vers l'intérieur de chaque virage. 0 le désactive. Il s'ajoute au roulis de début et de fin. | 0 à 90 | 0 |
| **Limite auto** | Plus grand roulis que le dévers automatique peut ajouter. | 0 à 45 degrés | 8 |

Le roulis fait tourner la section autour du milieu de la route, donc les morceaux partagent toujours exactement leurs faces de jonction. Le fossé et les bordures gardent aussi les mêmes jonctions, morceau après morceau. Le roulis total est limité à 60 degrés. Chaque morceau reste convexe à faces planes : là où le dévers, un virage ou le profil fossé vrillerait le dessus d'un morceau, ce morceau est coupé en deux moitiés convexes le long d'une diagonale, donc une courbe avec dévers peut avoir plus de brushes que les morceaux fois les bandes du profil. Une route droite au dévers régulier reste un brush à 6 faces par bande.

## Modifier une courbe

Chaque brush créé par l'outil courbe se souvient de sa courbe : les points, la vue, la profondeur et toutes les options sont stockés dans le bloc `editor` des brushes du VMF, sous forme de clés `hammerpro_curve`. Hammer et Hammer++ ignorent ces clés et les conservent à l'enregistrement, la carte reste donc valide pour eux. Les copies de la courbe entière continuent de fonctionner. Un brush coupé ou déplacé à la main garde les données, mais **Modifier la courbe** remet la forme de la courbe.

1. Sélectionnez un brush de la courbe (ou son entité brush).
2. **Outils > Courbes et routes > Modifier la courbe**, ou la palette de commandes. L'outil courbe s'ouvre sur les points enregistrés, à l'étape d'ajustement, avec les options enregistrées.
3. Changez ce que vous voulez :
   - **Faites glisser un point** pour le déplacer.
   - **Maj+clic** insère un point sur la courbe, entre les deux points les plus proches.
   - **Ctrl+clic** sur un point le retire (une courbe garde au moins 2 points).
   - **Retour** revient au tracé pour cliquer de nouveaux points à la fin, `Retour arrière` retire le dernier point.
   - Changez les subdivisions, l'épaisseur, la hauteur, le profil et le dévers.
4. `Entrée` (ou **Appliquer**) remplace tous les brushes de la courbe par les nouveaux, en une seule étape d'annulation. Ils gardent le matériau et restent dans la même entité brush, s'il y en a une. `Échap` annule sans rien changer.

Si le brush n'a pas de données de courbe (il n'a pas été créé par l'outil courbe, ou les données sont perdues), la commande le dit.

## Quand la courbe est refusée

- **Des segments se replient dans un virage serré** (en rouge) : à l'intérieur d'un virage plus serré que la moitié de l'épaisseur, les morceaux se croiseraient. Baissez l'épaisseur ou les subdivisions, ou déplacez les points pour ouvrir le virage. Un changement de direction de plus de 165 degrés environ entre deux morceaux est refusé de la même façon.
- **Des segments sont trop courts** : un morceau ferait moins d'une unité. Baissez les subdivisions ou tracez une courbe plus longue.
- **La courbe sort de la carte** : un coin de brush dépasserait 16384 unités.

## Astuces

- Tracez les routes et les murs dans la vue de dessus, les arches et les rampes dans la vue de face ou de côté.
- Peu de clics donnent les courbes les plus douces : la spline se charge de l'arrondi. Un tracé à main levée sur une grille fine suit davantage la main, tremblements compris.
- 16 à 64 subdivisions suffisent pour la plupart des courbes ; un nombre élevé fait beaucoup de petits brushes, qui coûtent du temps de compilation. Mettez les courbes décoratives dans un `func_detail` avec **Créer comme**.
- Les joints sont en onglet, donc les coins extérieurs ne tombent pas sur la grille. Ce sont quand même des plans propres partagés par les deux voisins, et la carte compile sans fuite ni brush invalide.
- L'outil MCP `create_curve_brushes` construit les mêmes brushes à partir d'une liste de points, ou un arc entre deux faces avec `faces` (face de départ et de fin `{solid, side}`), `start_height`, `end_height`, `apex` et `align` (avec les options `profile`, `curb_width`, `curb_height`, `ditch_depth`, `roll_start`, `roll_end`, `auto_bank` et `auto_bank_limit`), `get_curve_brushes` lit la courbe enregistrée, `edit_curve_brushes` remplace les brushes d'une courbe avec des points ou des options modifiés en une seule étape d'annulation, et `set_tool_options` avec l'outil `curve` règle `subdivisions`, `thickness`, `height`, `tex_scale`, `create_as`, `keep_active`, `pick_faces`, `arch_align` et les mêmes options de section et de dévers (voir la [référence des outils MCP](../../MCP.md)).

## FAQ

**Pourquoi certains morceaux sont rouges ?** Ils se replient dans un virage trop serré pour l'épaisseur. Voir plus haut.

**Puis-je faire un anneau fermé ?** Pour un anneau parfait, utilisez le tore ou l'arche de l'outil bloc. Avec l'outil courbe, placez le dernier point à côté du premier.

**Puis-je obtenir des cylindres ou des coins ?** Non : l'outil courbe ne fait que des brushes en forme de boîte, les brushes les plus robustes pour le compilateur. Le profil fossé fait des morceaux en forme de coin et le dévers les incline, mais ils restent convexes à faces planes.

## Pages liées

- [Outil route](road-tool.md)
- [Détails de route](road-details.md)
- [Outil bloc](block-tool.md)
- [Outil polygone](polygon-tool.md)
- [Outil vertex](vertex-tool.md)
- [Grilles](grids.md)
- [Raccourcis clavier](keyboard-shortcuts.md)

---
[Retour à l'index de la documentation](../README.md)
