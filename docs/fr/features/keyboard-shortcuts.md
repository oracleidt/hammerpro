[Documentation](../README.md) > Fonctionnalités > Raccourcis clavier | [English](../../en/features/keyboard-shortcuts.md)

# Raccourcis clavier

Chaque commande de Hammer Pro peut avoir un raccourci clavier. Les raccourcis par défaut viennent d'un jeu de raccourcis (Hammer Pro, Hammer ou Hammer++, pour que vos doigts gardent leurs habitudes), et vous pouvez changer n'importe quel raccourci dans **Paramètres > Clavier**. `F1` les affiche tous.

Le tableau complet des actions avec leurs raccourcis par défaut et les différences entre jeux est dans [Actions et raccourcis](../reference/actions-shortcuts.md).

## La référence des raccourcis (F1)

**Aide > Raccourcis clavier** (`F1`) ouvre une fenêtre qui liste chaque action par menu (Fichier, Édition, Map, Affichage, Outils, Fenêtre, Aide) avec son raccourci, plus les raccourcis supplémentaires du jeu choisi (affichés *aussi ...*). Les actions sans raccourci affichent `-`.

- **Rechercher une action ou un raccourci** filtre la liste, par nom d'action (en anglais ou dans la langue de l'interface), par id d'action, ou par raccourci (`ctrl+maj+s` comme `ctrl+shift+s` trouvent Enregistrer sous).
- La fin de la liste montre les **Contrôles des vues** de votre schéma de caméra (boutons de souris, touches de vol, molette, touches de grille, clics, poignées d'étirement). Voir [Caméra et navigation](camera-navigation.md).
- **Personnaliser...** ouvre **Paramètres > Clavier**.

## Paramètres > Clavier

Ouvrez-la avec **Outils > Options...** (`Ctrl+,`), page **Clavier**, ou avec **Personnaliser...** dans la fenêtre F1.

| Contrôle | Effet |
| --- | --- |
| **Jeu de raccourcis** | Choisit d'où viennent les raccourcis par défaut : **Hammer Pro**, **Hammer** ou **Hammer++**. Une confirmation suit (*Utiliser les raccourcis de X ? Les raccourcis que vous avez modifiés sont conservés.*) avec **Utiliser ces raccourcis** et **Annuler**. |
| **Rechercher une action ou un raccourci** | Filtre les actions, comme dans la fenêtre F1. |
| **Réinitialiser tous les raccourcis** | Restaure tous les raccourcis par défaut du jeu actuel (vos modifications sont perdues). |
| Ligne des conflits | *Raccourcis utilisés par plusieurs actions : N*, affichée en orange quand il y en a. |

Dessous, les actions sont rangées par menu. Chaque ligne contient :

- le nom de l'action (en gras quand vous avez changé son raccourci ; les raccourcis supplémentaires du jeu à la suite ; survolez-le pour voir l'id de l'action) ;
- le bouton du raccourci : cliquez-le, puis appuyez sur la nouvelle combinaison (**Appuyez sur une combinaison...**) ; `Échap` annule et garde l'ancien. Un raccourci utilisé par une autre action s'affiche en rouge, son infobulle indique *Aussi utilisé par : ...* ;
- **Supprimer le raccourci** (croix) : l'action n'a plus de raccourci ;
- **Restaurer le raccourci par défaut** (flèche) : retour au raccourci par défaut du jeu actuel.

Les changements s'appliquent tout de suite et sont enregistrés automatiquement.

### Règles d'enregistrement

- Maintenir Ctrl, Maj ou Alt affiche les modificateurs (`Ctrl+Maj+...`) ; le raccourci n'est enregistré qu'avec la touche qui le complète.
- `Échap`, `Tab` et `Entrée` ne peuvent pas être enregistrées seules (il leur faut un modificateur).
- Les chiffres sont enregistrés par position : `Alt+4` marche sur un clavier AZERTY, où cette touche tape une apostrophe.
- `Ctrl+C`, `Ctrl+X` et `Ctrl+V` s'enregistrent comme les autres raccourcis.
- Appuyer pendant l'enregistrement sur une touche qui est déjà un raccourci ne lance pas son action.

## Jeux de raccourcis

| Jeu | Pour qui | Raccourcis par défaut |
| --- | --- | --- |
| **Hammer Pro** | Par défaut. | Les raccourcis Hammer Pro, plus deux touches de Hammer qui ne gênent rien : `Maj+Q` efface aussi la sélection, `Maj+R` affiche ou masque aussi la grille. |
| **Hammer** | Les habitués du Hammer de Valve (versions Garry's Mod et Source SDK). | Les raccourcis Hammer Pro avec les différences ci-dessous. |
| **Hammer++** | Les habitués de Hammer++. | Les touches de Hammer plus celles de Hammer++ ci-dessous. |

Différences du jeu **Hammer** par rapport à Hammer Pro :

| Action | Hammer Pro | Hammer |
| --- | --- | --- |
| Afficher la grille | `Maj+G` | `Maj+R` |
| Tout sélectionner | `Ctrl+A` | `Ctrl+Maj+A` |
| Effacer la sélection | `Ctrl+Maj+A` | `Maj+Q` |
| Réinitialiser le partage des vues | aucun | `Ctrl+A` |
| Retourner verticalement | aucun | `Ctrl+I` |
| Inverser la sélection | `Ctrl+I` | `Ctrl+Maj+I` |
| Rechercher et remplacer dans les entités... | `Ctrl+Alt+R` | `Ctrl+Maj+R` |
| Rechercher et remplacer des textures... | `Ctrl+Maj+R` | aucun |
| Compiler et lancer... | `Ctrl+F9` | `F9` |
| Compiler maintenant | `F9` | `Ctrl+F9` |

Le jeu **Hammer++** ajoute :

| Action | Hammer++ |
| --- | --- |
| Navigateur de modèles | `Maj+F10` |
| Navigateur de particules | `Maj+F11` |
| Navigateur de sons | `Maj+F12` |
| Afficher les textures outils | `Ctrl+Maj+F2` |
| Afficher les objets d'éditeur | `Alt+O` |

Le jeu ne touche pas aux raccourcis que vous avez changés vous-même : ils restent prioritaires quel que soit le jeu. Les contrôles de la caméra (souris et touches de vol) se choisissent à part, avec **Contrôles de la caméra** dans **Paramètres > Vues** (voir [Caméra et navigation](camera-navigation.md)). L'assistant du premier démarrage règle les deux selon l'éditeur dont vous venez (rouvrez-le avec **Aide > Assistant de configuration...**).

### Ctrl+B : alignement sur la grille et biseau

`Ctrl+B` est **Aligner la sélection sur la grille** dans tous les jeux. Il sert aussi au biseau : dans l'[outil biseau](bevel-tool.md), et dans les outils Sélection et Vertex quand le curseur est sur une arête de brush dans la vue 3D, il commence le biseau de cette arête. Partout ailleurs il aligne sur la grille comme avant. Changer la touche de **Aligner la sélection sur la grille** déplace les deux.

## Où sont stockés les raccourcis

Dans settings.toml :

```toml
[workflow]
keymap_preset = "Hammer"        # "HammerPro", "Hammer" ou "HammerPlusPlus"

[keybindings]
save_map = "Ctrl+S"             # id de l'action = raccourci
show_console = ""               # vide : pas de raccourci
```

Seuls les raccourcis que vous avez changés sont écrits dans `[keybindings]`, avec les noms anglais des touches (`Shift`, pas `Maj`). Les ids des actions sont listés dans [Actions et raccourcis](../reference/actions-shortcuts.md).

## Quand un raccourci ne marche pas

Les raccourcis de la map ne se déclenchent jamais pendant la saisie dans un champ de texte, quand un menu, une fenêtre surgissante ou un dialogue modal est ouvert, pendant que la caméra 3D vole au clavier, ou pendant l'enregistrement d'un raccourci. Après avoir déplacé le focus sur un bouton ou une liste avec `Tab`, les touches simples (lettres, chiffres, Suppr, flèches, Entrée, Espace) appartiennent à l'interface : `Suppr` ou `H` après un champ de propriété ne suppriment ni ne masquent jamais l'objet en cours d'édition. Les raccourcis avec Ctrl ou Alt et les touches de fonction marchent toujours.

## MCP

`list_keybindings` et `set_keybinding` lisent et modifient les raccourcis depuis les applications d'IA. Voir la [référence des outils MCP](../../MCP.md) (en anglais).

## Astuces

- Cherchez par touche : tapez `alt+` dans la recherche pour voir tous les raccourcis avec Alt.
- Les commandes sans raccourci se lancent depuis la palette de commandes (`Ctrl+P`) ; donnez une touche ici à celles que vous utilisez souvent.
- Changer de jeu garde vos propres raccourcis : vous pouvez essayer **Hammer++** et revenir.

## FAQ

**Deux actions partagent un raccourci. Laquelle se lance ?** Réglez le conflit : la ligne est rouge et son infobulle nomme l'autre action. Supprimez ou changez l'un des deux.

**Comment retrouver le F9 de Hammer ?** Choisissez le jeu **Hammer** ou **Hammer++** : `F9` ouvre la fenêtre de compilation et `Ctrl+F9` compile.

**Puis-je partager mes raccourcis ?** Copiez la ligne `keymap_preset` de `[workflow]` et la table `[keybindings]` de votre settings.toml sur l'autre ordinateur.

## Pages liées

- [Actions et raccourcis](../reference/actions-shortcuts.md)
- [Palette de commandes](command-palette.md)
- [Caméra et navigation](camera-navigation.md)
- [Fenêtre des paramètres](settings-dialog.md)

---
[Retour à l'index de la documentation](../README.md)
