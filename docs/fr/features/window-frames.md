[Documentation](../README.md) > Fonctionnalités > Cadres de fenêtre | [English](../../en/features/window-frames.md)

# Cadres de fenêtre et fenêtres séparées

Hammer Pro dessine son propre cadre de fenêtre : une barre de titre qui contient les menus, des coins arrondis, une fine bordure et une ombre douce. Si ce cadre vous pose problème (bureau à distance, certains gestionnaires de fenêtres), vous pouvez revenir aux cadres de votre système. Certains outils peuvent aussi sortir de la fenêtre principale et vivre dans leur propre fenêtre, sur un autre écran par exemple.

## Le cadre Hammer Pro

La fenêtre principale n'a pas de barre de titre système. Sa propre barre de titre affiche les menus, le titre de la fenêtre (`<map> - Hammer Pro`) et trois boutons : **Réduire**, **Agrandir** (ou **Restaurer**) et **Fermer**.

| Geste | Effet |
| --- | --- |
| Glisser la barre de titre | Déplace la fenêtre. L'ancrage aux bords de l'écran marche toujours (le système gère lui-même le déplacement). |
| Double-cliquer la barre de titre | Agrandit ou restaure la fenêtre. |
| Glisser un bord ou un coin | Redimensionne la fenêtre. |

Une fenêtre agrandie a des coins carrés et pas d'ombre. Quand la carte graphique ne sait pas dessiner une fenêtre transparente, le cadre est carré, sans ombre.

## Cadres de fenêtre natifs

**Paramètres > Général > Cadres de fenêtre natifs** utilise les barres de titre et les cadres du système au lieu de ceux de Hammer Pro, pour le bureau à distance ou les gestionnaires de fenêtres qui ont du mal avec eux. C'est appliqué tout de suite, dans les deux sens, à la fenêtre principale et aux fenêtres séparées. Clé `ui.native_frames`, désactivée par défaut.

## Démarrage agrandi et restauration

Hammer Pro démarre agrandi dans la zone utile Windows. La barre des tâches reste accessible et les boutons réduire, agrandir/restaurer et fermer restent visibles. `F11` (**Affichage > Agrandir la fenêtre**) bascule entre agrandi et restauré, en restaurant la position et la taille précédentes. Il s'agit d'un agrandissement classique dans la zone utile.

**Paramètres > Général > Démarrer agrandi** est activé par défaut et s'applique au prochain démarrage. Sa clé historique reste `ui.start_fullscreen` et l'identifiant d'action reste `toggle_fullscreen` pour compatibilité ; les deux contrôlent désormais l'agrandissement et la restauration. Une nouvelle vérification en exécution des dimensions dans la zone utile et de la restauration reste à effectuer.

## Fenêtres séparées

Ces fenêtres peuvent quitter la fenêtre principale et devenir des fenêtres du système, à placer sur un autre écran et à garder ouvertes pendant que vous travaillez :

| Fenêtre | Comment la détacher | Défaut |
| --- | --- | --- |
| **Compiler et lancer** | Bouton **Fenêtre séparée** de la fenêtre (« Afficher cette fenêtre dans sa propre fenêtre... Retenu pour les prochaines fois »). Paramètre `compile.separate_window`. | fenêtre séparée |
| **Éditeur UV** | **Ouvrir dans une fenêtre séparée** dans l'éditeur UV. | dans le panneau |
| **Portails et visibilité** | **Détacher la fenêtre** dans la fenêtre. | dans la fenêtre principale |

Une fenêtre séparée porte le cadre Hammer Pro avec la marque Hammer Pro, son titre et trois boutons : **Remettre dans Hammer Pro** (retour dans la fenêtre principale), **Agrandir** et **Fermer**. Les fenêtres Compiler et lancer et Portails ont aussi un bouton **Remettre dans la fenêtre principale** dans leur contenu.

Les fenêtres séparées appartiennent à la fenêtre principale : elles restent au-dessus d'elle (pas au-dessus de tous les autres programmes) et se masquent avec elle quand elle est réduite.

### Placement

La dernière position de chaque fenêtre séparée est mémorisée en pixels du bureau (`compile.window`, `ui.uv_window`, `ui.portals_window`). À la réouverture :

- elle revient sur le même écran, et suit cet écran si vous avez réorganisé vos écrans ;
- elle reste telle quelle tant que sa barre de titre est sur un écran (une fenêtre à cheval sur deux écrans le reste) ;
- si son écran a disparu ou qu'elle est hors de tout écran, elle est ramenée dans l'écran qu'elle chevauche le plus (sinon le plus proche) et rétrécie si elle ne tient pas.

Sur les systèmes autres que Windows, c'est le gestionnaire de fenêtres qui place les fenêtres ; Hammer Pro lit la position depuis l'interface et la restaure au mieux.

## Astuces

- Avec deux écrans, gardez la fenêtre Compiler et lancer sur le second et continuez à éditer pendant la compilation.
- Si la fenêtre affiche un cadre noir ou buggé en bureau à distance, activez **Cadres de fenêtre natifs**.

## FAQ

**Je ne trouve pas le bouton pour réduire.** Il est à droite de la barre de titre, à côté de **Agrandir** et **Fermer**. Avec les cadres natifs, ce sont les boutons du système.

**Une fenêtre séparée s'est ouverte hors de l'écran.** Sa position est ramenée automatiquement sur un écran. Si ça arrive encore, remettez-la dans la fenêtre principale avec son bouton **Remettre**, puis détachez-la de nouveau.

**Les dialogues comme Paramètres deviennent-ils des fenêtres séparées ?** Non. Seules les fenêtres listées plus haut peuvent être détachées ; les autres dialogues flottent dans la fenêtre principale.

## Pages liées

- [Thèmes et langues](themes-languages.md)
- [Fenêtre de compilation](../compiler/compile-window.md)
- [Éditeur UV](uv-editor.md)
- [Fuites et pointfiles](leaks-pointfiles.md)
- [Plantages et journaux](../troubleshooting/crashes-and-logs.md)

---
[Retour à l'index de la documentation](../README.md)
