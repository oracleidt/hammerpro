[Documentation](../README.md) > Prise en main > Votre première map | [English](../../en/getting-started/first-map.md) | [Русский](../../ru/getting-started/first-map.md)

# Votre première map

Un guide pas à pas : une pièce fermée avec une lumière et un point d'apparition, une texture sur les murs, puis l'enregistrement, la compilation et le test dans Garry's Mod. Comptez environ cinq minutes. Les touches indiquées sont celles de Hammer Pro par défaut ; si vous avez choisi Hammer ou Hammer++ dans l'assistant de configuration, voyez la remarque de l'étape 8.

## Avant de commencer

- Garry's Mod est configuré (voir [Configuration du jeu](game-setup.md)). Attendez que la carte de chargement au-dessus des vues disparaisse : les textures et les définitions d'entités sont alors disponibles.
- Sachez vous déplacer dans les vues : dans la vue 3D, maintenez le clic droit et utilisez `Z` `Q` `S` `D` sur un clavier AZERTY (`W` `A` `S` `D` en QWERTY ; `Maj` pour aller plus vite, `A`/`E` pour descendre et monter). Dans les vues 2D, la molette zoome et le clic molette (ou `Espace` + glisser) fait défiler. Voir [Caméra et navigation](../features/camera-navigation.md).

## 1. Créer une nouvelle map

**Fichier > Nouvelle map** (`Ctrl+N`). Une map vide, sans titre, s'ouvre. Les quatre vues affichent la 3D (en haut à gauche), le Dessus, la Face et le Côté.

## 2. Tracer un bloc

1. Prenez l'**Outil bloc** : `Maj+B`, ou son icône dans la palette d'outils à gauche. La ligne d'options sous la barre d'outils affiche les primitives ; gardez **Bloc**.
2. Dans la vue **Dessus**, tracez un rectangle d'environ 512 sur 512 unités. La taille de la grille s'affiche dans la barre d'outils ; `]` élargit la grille et `[` l'affine.
3. Une boîte en attente apparaît dans toutes les vues 2D. Dans la vue **Face**, tirez sa poignée du haut pour lui donner environ 256 unités de haut (une nouvelle boîte démarre à 64 unités).
4. Appuyez sur `Entrée` pour la créer (`Échap` annule).

Le nouveau brush est sélectionné et ses faces portent le matériau actuel, `dev/dev_measuregeneric01b` par défaut (la texture grise de mesure du jeu dev). Vous pouvez aussi tracer la base directement sur une surface dans la vue 3D, puis bouger la souris pour régler la hauteur et cliquer. Plus de détails dans [Outil bloc](../features/block-tool.md).

## 3. L'évider en pièce

Le bloc étant sélectionné, choisissez **Outils > Rendre creux** (`Ctrl+H`). La fenêtre demande l'**Épaisseur des murs** : gardez 32 et appuyez sur `Entrée`. Le bloc devient six murs, groupés et sélectionnés. Une épaisseur négative construirait les murs à l'extérieur du bloc.

Une pièce étanche est importante : un trou vers le vide fait fuir la map (voir [Fuites et pointfiles](../features/leaks-pointfiles.md)).

## 4. Texturer les murs

Les murs étant toujours sélectionnés :

1. Ouvrez l'onglet **Textures** du panneau de droite.
2. Tapez un mot dans le champ de recherche, par exemple `concrete`.
3. Double-cliquez sur un matériau : il est appliqué à toutes les faces des objets sélectionnés.

Vous pouvez aussi glisser un matériau du navigateur sur une face précise dans la vue 3D, ou sélectionner des faces avec l'**Application de texture** (`Maj+A`) pour un travail plus fin. Voir [Navigateur de textures](../features/texture-browser.md) et [Application de texture](../features/texture-tool.md).

## 5. Ajouter le point d'apparition

1. Prenez l'**Outil entité** (`Maj+E`). Le champ **Classe** de la ligne d'options contient d'abord `info_player_start`.
2. Entrez dans la pièce avec la caméra 3D, puis cliquez sur le sol. L'entité se pose sur la surface cliquée ; une boîte fantôme avec le nom de la classe suit le curseur avant le clic.

Dans une vue 2D, un clic place plutôt l'entité sur le point de grille sous le curseur.

## 6. Ajouter une lumière

1. Toujours avec l'outil entité, cliquez dans le champ **Classe**, tapez `light` et choisissez **light** dans la liste.
2. Cliquez sur le plafond de la pièce dans la vue 3D : la lumière s'y accroche.

Sélectionnez la lumière avec l'**Outil de sélection** (`Maj+S`) et ouvrez l'onglet **Entité** pour régler sa luminosité et sa couleur (voir [Propriétés des entités](../features/entity-properties.md)).

## 7. Enregistrer

**Fichier > Enregistrer** (`Ctrl+S`). Choisissez un dossier (le **Dossier des maps** de **Paramètres > Jeu** est proposé s'il est réglé) et un nom en minuscules, chiffres et `_`, par exemple `gm_premierepiece.vmf` : la map compilée portera ce nom. À chaque enregistrement, une copie `.vmx` de la version précédente est gardée à côté de la map, comme dans Hammer.

## 8. Compiler et lancer

Appuyez sur `F9` (**Fichier > Compiler maintenant**). La map se compile avec les derniers réglages de compilation (le préréglage **Normal** par défaut) : géométrie, visibilité, éclairage, puis le `.bsp` est copié dans `garrysmod/maps` et Garry's Mod démarre sur votre map. La barre d'état affiche l'étape et la progression ; un clic dessus ouvre la fenêtre de compilation.

Pour voir les options d'abord, utilisez **Fichier > Compiler et lancer...** (`Ctrl+F9`) ou le bouton **Compiler** de la barre d'outils : la fenêtre **Compiler et lancer** s'ouvre avec les préréglages (**Brouillon**, **Normal**, **Final**), les étapes et **Lancer le jeu**. Cliquez alors sur **Compiler**.

Avec le jeu de raccourcis Hammer ou Hammer++, les touches sont inversées comme dans Hammer : `F9` ouvre la fenêtre de compilation et `Ctrl+F9` compile directement.

Si Garry's Mod est déjà lancé, la map se charge dans le jeu en cours (« Jeu prêt » dans la barre d'état) au lieu d'en démarrer un nouveau.

## 9. En cas de problème

- **Fuite** : la barre d'état affiche un badge **Fuite** et le chemin de la fuite est tracé dans les vues. Bouchez le trou avec un brush du monde et recompilez. Voir [Fuites et pointfiles](../features/leaks-pointfiles.md).
- **Erreurs dans le journal** : dans la fenêtre de compilation, cliquez sur une erreur pour aller au brush ou à l'entité concerné. Voir [Limites et journaux de compilation](../compiler/limits-and-logs.md) et [Erreurs courantes](../troubleshooting/common-errors.md).
- **Annuler** : `Ctrl+Z` annule n'importe quelle étape, `Ctrl+Y` la rétablit.

## Pour aller plus loin

- [Compiler et tester dans Garry's Mod](../guides/compile-and-test-gmod.md)
- [Éclairage](../guides/lighting.md)
- [Outil de sélection](../features/selection-tool.md)
- [Fenêtre Compiler et lancer](../compiler/compile-window.md)

## Pages liées

- [Tour de l'interface](interface-tour.md)
- [Outil bloc](../features/block-tool.md)
- [Outil entité](../features/entity-tool.md)
- [Opérations sur les brushes](../features/brush-operations.md)

---
[Retour à l'index de la documentation](../README.md)
