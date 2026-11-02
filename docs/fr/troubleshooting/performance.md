[Documentation](../README.md) > Dépannage > Performances de l'éditeur | [English](../../en/troubleshooting/performance.md)

# Performances de l'éditeur

Hammer Pro est conçu pour rester fluide sur des maps énormes, mais un contenu lourd, un GPU modeste ou un portable sur batterie peuvent quand même ralentir les vues. Cette page liste les réglages qui comptent, du moins coûteux à changer au plus coûteux. Pour la durée des compilations, voir [Performances de compilation](../compiler/performance.md).

## Mesurer d'abord

- **Paramètres > Général > Afficher les images par seconde** affiche les images par seconde dans la barre d'état.
- **Affichage > Options d'affichage > Informations de la vue 3D** (`O`) affiche la position de la caméra et, pour la vue 3D, les appels de dessin, les triangles et les blocs dessinés, et le nombre de blocs écartés.

## Réglages d'images par seconde

Page **Paramètres > Rendu**, groupe **Images par seconde** :

| Option | Effet | Défaut |
|---|---|---|
| **Limite d'images par seconde** | Nombre maximal d'images par seconde (0 : aucune limite ; avec la synchronisation verticale, l'écran la limite). Baissez-la pour économiser batterie et chaleur sur un portable. | 0 |
| **En arrière-plan** | Limite d'images par seconde quand la fenêtre n'est pas active (0 : aucune limite). | 30 |
| **Synchronisation verticale** | Attend le rafraîchissement de l'écran. S'applique après un redémarrage. | Activé |

## Ce que dessine la vue 3D

| Réglage | Où | Effet sur la vitesse | Défaut |
|---|---|---|---|
| **Distance d'affichage** | Paramètres > Vues | Distance maximale dessinée. Plus courte, c'est plus rapide. **Affichage > Distance d'affichage plus courte** et **Distance d'affichage plus longue** la changent à la volée. | 32768 |
| **Anticrénelage (MSAA)** | Paramètres > Vues | Bords plus lisses, utilise plus de mémoire GPU. Désactivez-le sur un GPU modeste. | Activé |
| **Limite de distance circulaire** et **Rayon de limite** | Paramètres > Rendu, aussi **Affichage > Options d'affichage > Limite de distance circulaire** et la barre d'outils | Rien n'est dessiné au-delà du rayon autour de la caméra (un cercle le montre dans les vues 2D), comme le radius culling de Hammer. | Désactivé, 4096 |
| **Ignorer la géométrie cachée** | Paramètres > Rendu | Les parties de la map dont tous les objets sont masqués ne sont pas dessinées du tout. | Activé |
| **Distance des modèles** | Paramètres > Rendu | Les modèles plus loin que cela sont dessinés en boîtes (0 : aucune limite). Très utile sur les maps chargées en props. | 0 |
| `lod_distances` | `settings.toml`, table `[render]` | Deux distances au-delà desquelles les modèles lourds utilisent leur LOD 1 et LOD 2 (0 : toujours le modèle complet). Pas encore d'interface. | `[0, 0]` |
| **Relief (normal maps)** | Paramètres > Rendu | Désactivé : surfaces plates et moins de mémoire vidéo. | Activé |
| **Reflets des cubemaps** et **Redessiner les cubemaps après les modifications** | Paramètres > Rendu | Les matériaux avec `$envmap` reflètent des cubemaps dessinées par l'éditeur. Sans le redessin automatique, elles restent telles quelles jusqu'à **Map > Éclairage > Reconstruire les cubemaps**. | Activé, Activé |
| **Lumières avec ombres** | Paramètres > Rendu | En mode Aperçu de l'éclairage, nombre de lumières ponctuelles et de spots proches de la caméra qui projettent des ombres (0 : le soleil seulement, 16 au maximum). | 4 |
| **Lumière du ciel seulement sous le ciel** | Paramètres > Rendu | En mode Aperçu de l'éclairage, la lumière du ciel est vérifiée avec une carte des hauteurs de la map autour de la caméra. Désactivé : un peu moins de travail pour les cartes graphiques lentes. | Activé |
| **Aperçu de l'eau** | Paramètres > Rendu | Réfraction, reflets et vagues de l'eau. Désactivé : la texture d'outil. | Activé |
| **Fondu des modèles**, **Effets de rendu**, **Matériaux animés** | Affichage > Options d'affichage (les deux premiers aussi dans Paramètres > Rendu) | Coûts modestes ; à couper quand chaque image compte. | Désactivé, Activé, Activé |

Le mode de rendu compte aussi : **Couleurs unies** (`Alt+2`) et **Filaire** (`Alt+3`) sont les moins coûteux ; **Aperçu de l'éclairage** (`Alt+5`) et **Aperçu de l'éclairage précalculé** (`Ctrl+F5`) les plus coûteux. Voir [Modes de rendu et affichage](../features/render-modes-display.md) et [Aperçu de l'éclairage](../features/lighting-preview.md).

## Ce que dessinent les vues 2D

| Réglage | Où | Effet | Défaut |
|---|---|---|---|
| **Modèles** | Paramètres > Rendu | **Boîtes** (le plus rapide), **Fil de fer** (tous les modèles), **Fil de fer si sélectionné**. | Fil de fer si sélectionné |
| Displacements en 2D | Paramètres > Rendu, **Affichage > Options d'affichage > Maillage des displacements en vue 2D** | **Maillage complet**, **Maillage de la sélection** ou **Contour seulement**. Les contours allègent les maps riches en terrain. | Maillage complet |

## Masquer ce sur quoi vous ne travaillez pas

L'objet le plus rapide est celui qu'on ne dessine pas :

- **Masquer la sélection** (`H`), **Masquer le reste** (`Ctrl+Maj+H`), l'œil d'isolement de la palette d'outils et **Tout afficher** (`U`) : voir [Masquer et isoler](../features/hide-isolate.md).
- Désactivez des [visgroups](../features/visgroups.md) entiers.
- Utilisez un [cordon](../features/cordons.md) pour travailler sur une zone.
- **Affichage > Options d'affichage** : coupez les modèles, les aides, les particules, les cordes, les overlays ou la skybox 3D quand vous n'en avez pas besoin.

## Temps de chargement

- **Monter les téléchargements du Workshop** (Paramètres > Jeu) ralentit le démarrage : laissez-le désactivé si vous n'avez pas besoin du contenu du Workshop.
- Les addons du Workshop et téléchargés se montent en arrière-plan après le reste ; l'éditeur reste utilisable pendant ce temps.
- Les très gros dossiers de textures personnalisés sont indexés à chaque montage : gardez-les ciblés.

## Travail en arrière-plan

Les vérifications en direct (**Vérifier la map pendant l'édition**, **Chercher les fuites pendant l'édition**, Paramètres > Santé de la map, activées par défaut) tournent en arrière-plan peu après chaque modification. Sur une très grosse map avec un processeur lent, désactiver la recherche de fuites économise du travail ; la compilation trouve toujours les fuites. La précompilation en arrière-plan (dans les réglages de compilation) utilise le processeur libre pour préparer la compilation suivante.

## FAQ

**La vue 3D saccade quand je vole dans une grosse map.** Activez la **Limite de distance circulaire**, réglez une **Distance des modèles** (2048 par exemple) et vérifiez que **Ignorer la géométrie cachée** est activé.

**L'éditeur sollicite beaucoup le GPU quand je suis dans une autre fenêtre.** Baissez **En arrière-plan** (par exemple 10).

**Les ventilateurs tournent même quand rien ne bouge.** Réglez une **Limite d'images par seconde** comme 60, ou gardez la **Synchronisation verticale**.

## Pages liées

- [Modes de rendu et affichage](../features/render-modes-display.md)
- [Masquer et isoler](../features/hide-isolate.md)
- [Référence des paramètres](../reference/settings.md)
- [Performances de compilation](../compiler/performance.md)
- [Plantages et journaux](crashes-and-logs.md)

---
[Retour à l'index de la documentation](../README.md)
