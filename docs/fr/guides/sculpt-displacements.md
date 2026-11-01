[Documentation](../README.md) > Guides > Sculpter des displacements | [English](../../en/guides/sculpt-displacements.md)

# Sculpter des displacements

Ce guide crée un morceau de terrain : transformer des faces de brush en displacements, sculpter des collines et un lit de rivière, mélanger deux textures, et garder les tuiles cousues entre elles. Toutes les options de l'outil sont listées dans [Outil displacement](../features/displacement-tool.md).

## Étape 1 : préparer la base

1. Avec l'outil bloc, dessinez un brush plat pour le sol, par exemple 1024 x 1024 unités sur 16 d'épaisseur. Pour un grand terrain, mettez plusieurs brushes côte à côte : chaque face du dessus devient une tuile.
2. Donnez-lui un matériau de mélange (un matériau `WorldVertexTransition` à deux textures, comme herbe et terre) si vous voulez peindre le mélange des textures ensuite.

## Étape 2 : créer les displacements

1. Sélectionnez les brushes (ou seulement les faces voulues avec l'outil texture).
2. Appuyez sur `Maj+P` (**Outils > Outil displacement**). Le panneau **Outil** du dock de droite affiche les options des displacements.
3. Dans la section **Créer**, la ligne en dessous indique ce qui sera utilisé : les faces sélectionnées, ou les faces du dessus des solides sélectionnés. Seules les faces à 4 côtés peuvent devenir des displacements.
4. Cliquez sur un bouton de **Puissance** : **2** (5 x 5 sommets), **3** (9 x 9) ou **4** (17 x 17). La puissance 3 est un bon départ ; prenez 4 pour un sol détaillé.

## Étape 3 : peindre la forme

1. Choisissez le mode **Sculpter**.
2. Choisissez l'effet : **Élever**, **Abaisser**, **Lisser** ou **Élever à** (une hauteur cible au-dessus de la face).
3. Dans la section **Pinceau**, réglez **Rayon** (unités du monde), **Intensité** (unités par seconde au centre du pinceau) et **Atténuation** (**Constante**, **Linéaire**, **Douce**).
4. Choisissez l'**Axe** : **Normale de la face**, **Normale de la surface**, **Centre du pinceau**, **Vue (caméra)**, **Vue (caméra) horizontale** (vers la caméra sur X et Y seulement), **X**, **Y**, **Z (haut)** ou **Personnalisé** (Alt et un clic droit sur une face dans la vue 3D prend sa normale).
5. Dans la vue 3D, maintenez le bouton gauche sur les displacements et déplacez la souris : un cercle de pinceau suit la surface.
   - `Maj` inverse (abaisser au lieu d'élever).
   - Un glisser avec le clic droit, ou `Alt` et un glissement horizontal, change le rayon.
   - Un clic bref sélectionne une face (`Ctrl` inverse).
   - `Échap` annule le trait en cours.
   - Chaque trait est une étape d'annulation.

Avec **Continu** activé, garder le bouton enfoncé sans bouger continue de peindre. Quand des displacements sont sélectionnés et que **Sélection seulement** est activé, seuls ceux-ci sont peints : appuyez sur la touche indiquée dans l'aide pour vider la sélection et tout peindre.

## Étape 4 : modeler des formes précises

Le mode **Modeler** façonne le sol à partir de sa position au début du trait :

- **Pousser** : élève la surface jusqu'à une couche décalée (**Distance** en unités avec **Absolu**, ou **Pourcentage** du rayon avec **Adaptatif**). Repasser au même endroit ne cumule pas avec **Atténuée** ; **Additive** continue d'ajouter. `Ctrl` enfonce au lieu d'élever.
- **Creuser** : creuse un **Profil** (V, U, plat, ou vos 5 points) d'une **Profondeur** donnée le long du trait : fossés, rivières, routes. `Ctrl` élève une crête.
- **Projeter** : déplace la surface sur les brushes sélectionnés le long de l'axe de modelage (les brushes plus loin que la **Portée** sont ignorés). Sélectionnez d'abord les brushes en même temps que les displacements.

`Maj` lisse pendant le modelage, avec l'intensité réglée dans **Lissage**.

## Étape 5 : garder les tuiles ensemble

- **Coudre en peignant** : les tuiles voisines suivent le pinceau en direct et les fissures restantes sont refermées à la fin du trait.
- **Spatial** mesure le pinceau en 3D sur la surface, il peint donc aussi les murs et à travers les tuiles.
- **Ligne de vue** ne peint que ce que la caméra voit sous le pinceau (pas le terrain derrière une colline).
- **Coudre** dans la section **Sélection** referme après coup les fissures entre les displacements sélectionnés ; **Sélectionner les voisins** ajoute les tuiles qui partagent une arête.

## Étape 6 : peindre le mélange des textures

1. Choisissez **Peindre l'alpha**.
2. **Texture** : **2 textures** peint l'alpha entre les deux textures ; avec un matériau à 4 textures, choisissez la texture (1 à 4) à peindre.
3. Choisissez **Ajouter** (révèle la seconde texture), **Retirer** (la première), **Valeur fixe** (0 affiche la première texture, 255 la seconde) ou **Lisser**.
4. Peignez dans la vue 3D comme pour la géométrie. **Inverser l'alpha** dans la section **Outils** échange les deux textures.

## Étape 7 : finitions

Dans la section **Outils** du panneau :

- **Ajouter du bruit** : une hauteur aléatoire entre un minimum et un maximum sur chaque sommet (aussi **Outils > Ajouter du bruit aux displacements...**).
- **Subdiviser** : lisse les displacements sélectionnés comme une seule surface, puis les coud.
- **Échelle** : multiplie les hauteurs (un facteur négatif les retourne).
- **Projeter les ombres**, **Échelle de lightmap** et les drapeaux de collision (**Sans collision des personnages**, **Sans collision physique**, **Sans collision des tirs**) s'appliquent aux displacements sélectionnés.

Vérifiez où les joueurs peuvent marcher avec **Affichage > Options d'affichage > Praticabilité des displacements**, et peignez en mode **Marquages** (praticable, constructible, retiré) là où la pente donne une mauvaise réponse.

## FAQ

**Rien ne se passe quand je peins.** Quand des displacements sont sélectionnés, seuls ceux-ci sont peints : cliquez sur la tuile pour la sélectionner, ou videz la sélection.

**Des faces ont été ignorées à la création.** Seules les faces à 4 côtés peuvent devenir des displacements. Découpez ou refaites le brush pour que sa face du dessus ait quatre côtés.

**Des fissures entre les tuiles.** Activez **Coudre en peignant**, ou sélectionnez les tuiles et cliquez sur **Coudre**.

## Pages liées

- [Outil displacement](../features/displacement-tool.md)
- [Outil bloc](../features/block-tool.md)
- [Modes de rendu et affichage](../features/render-modes-display.md)
- [Créer un prop avec Propper](make-a-prop-with-propper.md)
- [Commandes de script](../reference/script-commands.md)

---
[Retour à l'index de la documentation](../README.md)
