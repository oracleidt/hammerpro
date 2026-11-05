[Documentation](../README.md) > Fonctionnalités > Thèmes et langues | [English](../../en/features/themes-languages.md)

# Thèmes et langues

Hammer Pro a un thème sombre et un thème clair, une couleur d'accent facultative, une échelle d'interface, une image de fond facultative sur la fenêtre, et toute son interface en anglais et en français, que l'on change à tout moment sans redémarrer.

## Icônes de l'interface

Hammer Pro intègre un ensemble homogène d'icônes pour ses barres d'outils, outils, fenêtres et indicateurs. Leurs proportions et leur espacement restent identiques à chaque échelle de l'interface, indépendamment des polices de symboles installées sous Windows. Le texte conserve sa police habituelle. Le sous-ensemble intégré provient des [Microsoft Fluent UI System Icons](https://github.com/microsoft/fluentui-system-icons) ; sa licence MIT est incluse dans la police et dans `assets/fonts/LICENSE.txt`.

## Où le régler

Tout ce qui est sur cette page se trouve dans **Paramètres > Général** (**Outils > Options...**, `Ctrl+,`), sections **Apparence** et **Image de fond**. La langue est aussi dans **Affichage > Langue** et dans la palette de commandes.

## Apparence

| Option | Effet | Défaut |
| --- | --- | --- |
| **Langue** | Langue de l'interface : English ou Français. Appliquée tout de suite. | la langue du système au premier démarrage |
| **Thème** | **Sombre** ou **Clair**. Appliqué tout de suite. Clé `ui.theme`. | Sombre |
| **Couleur d'accent** | **Noir et blanc** (pas d'accent), une des huit pastilles (bleu, orange, vert, violet, rouge, rose, cyan, jaune), ou une **Couleur personnalisée** choisie au sélecteur. Elle colore les surlignages, l'outil actif et la sélection dans l'interface. Clé `ui.accent_color`. | Noir et blanc |
| **Échelle de l'interface** | Taille de toute l'interface, de 0,75x à 2x par pas de 0,05. Appliquée quand vous relâchez le curseur. Le bouton **1x** la remet à zéro. Clé `ui.ui_scale`. | 1x |
| **Afficher les images par seconde** | Affiche le nombre d'images par seconde. Clé `view.show_fps`. | désactivé |
| **Cadres de fenêtre natifs** | Voir [Cadres de fenêtre](window-frames.md). | désactivé |

Le look est noir et blanc par choix : les états et gravités se lisent par niveaux de gris, graisse, icônes et fonds inversés. La couleur reste dans le contenu (les vues, les miniatures, les couleurs d'entités et de visgroups) et dans l'accent que vous choisissez.

## Image de fond

Une image dessinée sur les panneaux de la fenêtre (sous les dialogues et les menus), en faible opacité, pour personnaliser l'éditeur.

| Option | Effet | Défaut |
| --- | --- | --- |
| **Image** | **Choisir...** prend un fichier PNG, JPG, BMP, TGA ou WebP ; **Retirer** l'enlève. Le nom du fichier s'affiche à côté des boutons (**Aucune image** sinon). Clé `ui.background_image`. | aucune |
| **Opacité** | De 0 à 50 %. | 15 % |
| **Position** | Décalage X et Y en pixels depuis la position centrée. | 0, 0 |
| **Zoom** | De 0,25x à 4x par rapport à la taille qui couvre juste la fenêtre. **Ajuster** revient à une image qui couvre toute la fenêtre, centrée. | 1x |
| **Masquer sur la vue 3D** | L'image n'est pas dessinée sur les vues 3D. | activé |

L'image couvre la fenêtre et se centre, puis se déplace et se zoome selon ces réglages. Ses options n'apparaissent qu'une fois une image choisie.

## Langues

L'interface existe en anglais et en français. Chaque libellé, infobulle, message et les remarques du journal de compilation suivent la langue. Pour en changer :

- **Paramètres > Général > Langue**.
- **Affichage > Langue**, puis **English** ou **Français**.
- La palette de commandes (`Ctrl+P`) : tapez `langue` et choisissez **Langue : English** ou **Langue : Français**.
- L'assistant du premier démarrage la demande (rouvrez-le avec **Aide > Assistant de configuration...**).

Le changement s'applique tout de suite, sans redémarrer, et il est enregistré dans settings.toml (`language`, `En` ou `Fr`). Au tout premier démarrage, la langue suit celle du système : le français si le système est en français, l'anglais sinon.

### Pour une seule session

Lancez Hammer Pro avec `--lang fr` ou `--lang en` pour utiliser une langue le temps de la session sans changer le choix enregistré :

```text
hammerpro.exe --lang fr
```

Voir [Ligne de commande](../reference/command-line.md).

## Astuces

- Sur un écran 4K, mettez **Échelle de l'interface** à 1,5x ou plus ; les vues gardent leur pleine résolution.
- Choisissez une couleur d'accent pour repérer plus vite l'outil actif et la sélection.
- Gardez une faible opacité pour l'image de fond (10 à 20 %) pour que les panneaux restent lisibles.

## FAQ

**Puis-je traduire Hammer Pro dans une autre langue ?** Seuls l'anglais et le français existent en 1.0.2. Tous les textes sont dans des tables de traduction du code source ; voir [Étendre Hammer Pro](../development/extending.md).

**L'interface est trop grande après un changement d'échelle.** Cliquez **1x** à côté du curseur, ou modifiez `ui_scale` dans settings.toml.

**La langue change-t-elle le nom des fichiers ou des maps ?** Non. Seul le texte de l'interface change ; les maps, les clés et les paramètres restent identiques.

## Pages liées

- [Fenêtre des paramètres](settings-dialog.md)
- [Cadres de fenêtre](window-frames.md)
- [Palette de commandes](command-palette.md)
- [Référence des paramètres](../reference/settings.md)

---
[Retour à l'index de la documentation](../README.md)
