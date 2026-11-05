[Documentation](../README.md) > Guides > Grilles personnalisées pour les escaliers | [English](../../en/guides/custom-grids-for-stairs.md) | [Русский](../../ru/guides/custom-grids-for-stairs.md)

# Grilles personnalisées pour les escaliers

Hammer n'a qu'une grille, avec le même pas sur tous les axes. Les escaliers s'y prêtent rarement : une marche courante fait 8 unités de haut pour 12 de profondeur, et un escalier suit souvent l'orientation de son bâtiment. Les grilles personnalisées de Hammer Pro ont un pas par axe, une origine, une orientation et des limites facultatives, pour que chaque marche tombe pile. Ce guide construit un escalier avec l'une d'elles.

## Étape 1 : créer la grille

1. Ouvrez **Map > Grilles > Gestionnaire de grilles...**. La fenêtre **Grilles** s'ouvre.
2. Dans **Grilles personnalisées**, cliquez sur **Nouvelle grille**. Elle part de la taille de la grille globale.
3. Remplissez les champs :

| Champ | Pour l'escalier | Remarques |
|---|---|---|
| **Nom** | `Escalier` | Unique dans la map. |
| **Pas X Y Z** | `12`, `64`, `8` | N'importe quelle taille, décimales comprises. Des pas égaux changent ensemble : maintenez `Maj` pendant que vous en modifiez un pour le régler seul. |
| **Origine** | le coin bas de la première marche | Un nœud de la grille s'y trouve. |
| **Tangage Lacet Roulis** | `0 0 0` (ou le lacet d'un bâtiment tourné) | Orientation de la grille, comme les angles d'une entité. |
| **Ligne principale toutes les** | par ex. `4` | 0 : le réglage des vues. |
| **Limites** | la boîte de la cage d'escalier | Facultatif : la grille ne s'applique que dans cette boîte, la grille globale en dehors. **Ajuster à la sélection** prend les limites de la sélection. |
| **Couleur** | au choix | Couleur facultative de ses lignes. |

La grille est **enregistrée avec la map** (un bloc `hammerpro_grids` dans le VMF). **Enregistrer comme préréglage** la copie dans les paramètres pour que toutes les maps la proposent ; **Copier dans la map** fait l'inverse.

## Étape 2 : en faire la grille active

Cliquez sur **Utiliser** dans le gestionnaire, ou choisissez **Map > Grilles > Grille personnalisée**. La ligne **Grille active :** en haut de la fenêtre bascule entre **Grille globale**, **Une grille par vue**, **Grille personnalisée** et **Grille de la sélection**. Le bouton de grille de la barre d'outils propose les mêmes choix.

Avec la grille personnalisée active, les vues 2D dessinent ses lignes, et **Afficher la grille personnalisée dans la vue 3D** la dessine aussi dans la vue 3D. `[` et `]` mettent à l'échelle tous les pas de la grille personnalisée ensemble.

## Étape 3 : dessiner les marches

1. Prenez l'outil bloc (`Maj+B`).
2. Dans la vue de côté ou de face (celle qui montre la profondeur et la hauteur de l'escalier), dessinez la première marche : ses coins s'accrochent tous les 12 unités en X et tous les 8 unités en Z.
3. Appuyez sur `Entrée` pour la créer.
4. Clonez-la : avec l'outil de sélection, faites-la glisser en maintenant `Maj` (glisser-cloner) d'une marche vers l'avant et d'une marche vers le haut ; la copie s'aligne sur la grille. **Édition > Collage spécial...** (`Ctrl+Maj+V`) colle aussi plusieurs copies avec un décalage.
5. Recommencez, ou utilisez une série de copies (voir [Transformer et aligner](../features/transform-align.md)).

Pour un escalier tourné, réglez le **Lacet** de la grille sur l'angle du bâtiment : l'accrochage, les déplacements et les lignes de la grille suivent ses axes. Un **Tangage** suit une pente, par exemple pour poser les rampes le long de l'escalier.

## Étape 4 : garder la grille avec l'escalier

Sélectionnez l'escalier terminé et cliquez sur **Attacher à la sélection** (ou **Map > Grilles > Attacher la grille à la sélection**). Avec **Les objets utilisent leur grille** activé (par défaut), modifier ces objets ou dessiner dessus s'accroche à leur grille, quelle que soit la grille active. **Grille de la sélection** fait de la grille des objets sélectionnés la grille active. **Map > Grilles > Détacher la grille de la sélection** retire le lien. Le gestionnaire indique à combien d'objets ou de groupes chaque grille de la map est attachée.

## Raccourcis vers une grille

- **Map > Grilles > Grille depuis la sélection** : une grille calée sur la sélection (pour une entité ponctuelle : son origine et ses angles ; pour des brushes : les axes du premier solide, avec un nœud sur un coin de ses limites), avec le pas de la grille globale. Sélectionnez une marche existante, lancez la commande, puis réglez le **Pas X Y Z** dans le gestionnaire.
- **Map > Grilles > Grille depuis la face** : une grille sur la face sélectionnée (outil texture) : son plan est celui de la face, X le long de sa plus longue arête. Pratique pour une rampe ou un plafond en pente.

Les deux créent une nouvelle grille personnalisée et la rendent active.

## FAQ

**Mes blocs ne tournent pas avec la grille.** Les boîtes dessinées restent alignées sur les axes du monde : avec une grille tournée, chaque axe du monde prend le pas de l'axe de grille le plus proche. Construisez l'escalier aligné sur le monde, puis faites tourner l'escalier terminé.

**Le reste de la map s'accroche bizarrement.** Donnez des **Limites** à la grille pour qu'elle ne s'applique que dans la cage d'escalier, ou revenez à **Grille globale**.

## Pages liées

- [Grilles](../features/grids.md)
- [Outil bloc](../features/block-tool.md)
- [Transformer et aligner](../features/transform-align.md)
- [Presse-papiers](../features/clipboard.md)
- [Référence des paramètres](../reference/settings.md)

---
[Retour à l'index de la documentation](../README.md)
