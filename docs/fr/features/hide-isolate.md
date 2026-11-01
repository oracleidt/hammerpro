[Documentation](../README.md) > Fonctionnalités > Masquer et isoler | [English](../../en/features/hide-isolate.md)

# Masquer et isoler

Masquer des objets les écarte sans les supprimer : le masquage rapide de Hammer (`H`, `Ctrl+Maj+H`, `U`), le bouton d'isolement de la palette d'outils, et des interrupteurs d'affichage pour les objets propres à l'éditeur et les textures outils.

## Accès

| Action | Menu | Raccourci par défaut |
| --- | --- | --- |
| **Masquer la sélection** | **Affichage > Masquer la sélection** | `H` |
| **Masquer le reste** | **Affichage > Masquer le reste** | `Ctrl+Maj+H` |
| **Tout afficher** | **Affichage > Tout afficher** | `U` |
| **Créer un visgroup des objets masqués** | **Affichage > Créer un visgroup des objets masqués** | aucun |
| **Afficher les objets d'éditeur** | **Affichage > Options d'affichage > Afficher les objets d'éditeur** | aucun (`Alt+O` avec le préréglage de touches Hammer++) |
| **Afficher les textures outils** | **Affichage > Afficher les textures outils** | aucun (`Ctrl+Maj+F2` avec le préréglage de touches Hammer++) |
| Textures outils masquées | **Affichage > Textures outils masquées** (sous-menu) | aucun |

La barre d'outils a les trois boutons de masquage, et la palette d'outils se termine par l'œil d'isolement.

## Masquer des objets

- **Masquer la sélection** (`H`) masque les objets sélectionnés. Ils sont désélectionnés.
- **Masquer le reste** (`Ctrl+Maj+H`) masque tous les objets visibles qui ne sont pas sélectionnés.
- **Tout afficher** (`U`) réaffiche tous les objets masqués de cette façon.

Chacune est une étape d'annulation. Les objets masqués :

- ne sont pas dessinés et ne peuvent pas être cliqués ni sélectionnés, comme dans Hammer : une sélection par rectangle, **Tout sélectionner**, **Sélectionner les membres** d'un visgroup ou un clic dans l'[Arborescence](outliner.md) les ignore, donc Supprimer ou un déplacement ne touche jamais ce que vous ne voyez pas ;
- restent masqués à l'enregistrement (blocs `hidden` du VMF, comme Hammer) ;
- sont exclus de la compilation, sauf si **Compiler les objets masqués** est coché dans la [fenêtre Compiler et lancer](../compiler/compile-window.md) ou dans la page **Compilation** des paramètres. Le journal de compilation prévient quand des lumières masquées sont exclues.

La ligne **Objets masqués** des visgroups automatiques (onglet Visgroups) les compte et propose les boutons **Tout afficher** et **En visgroup**. **En visgroup** (comme **Affichage > Créer un visgroup des objets masqués**) les déplace dans un visgroup masqué `_FromQuickHide(N)`, pour pouvoir les réafficher par leur nom (voir [Visgroups](visgroups.md)).

L'[Arborescence](outliner.md) a un œil sur chaque ligne pour masquer ou afficher un objet, un groupe, une classe d'entités ou tout le monde.

## Isoler

L'œil au bout de la palette d'outils isole la sélection :

- avec des objets sélectionnés et rien de masqué, il lance **Masquer le reste** (infobulle « Isoler la sélection (masquer tout le reste) ») ;
- quand des objets sont masqués, il est allumé et lance **Tout afficher** (infobulle « Tout réafficher ») ;
- sans sélection et sans objet masqué, il est grisé (« Sélectionnez quelque chose pour l'isoler »).

## Objets d'éditeur

**Afficher les objets d'éditeur** (activé par défaut, `helpers.show_editor_objects`) affiche les icônes d'entités et les aides qui n'existent que dans l'éditeur, comme les « editor objects » de Hammer++. Désactivez-le pour voir la map plus proche du jeu. Les sprites du jeu (`env_sprite`) restent visibles.

**Affichage > Options d'affichage > Afficher les entités ponctuelles** affiche ou masque toutes les entités ponctuelles d'un coup.

## Textures outils

- **Afficher les textures outils** (menu Affichage et barre d'outils, activé par défaut, `view.show_tool_textures`) affiche ou masque toutes les faces à texture outil dans la vue 3D.
- **Affichage > Textures outils masquées** masque des familles de textures outils dans la vue 3D quand les textures outils sont affichées : **Nodraw**, **Clips**, **Déclencheurs**, **Hints et skips**. Les mêmes quatre cases sont dans la page **Rendu** des paramètres (**Textures outils masquées en 3D**), et **Afficher le nodraw** (Affichage > Options d'affichage et barre d'outils) bascule la case **Nodraw**.

| Paramètre (`[render]`) | Ce qu'il masque | Défaut |
| --- | --- | --- |
| `hide_nodraw` | Faces nodraw | Désactivé |
| `hide_clips` | Brushes clip | Désactivé |
| `hide_triggers` | Triggers | Désactivé |
| `hide_hints` | Hints et skips | Désactivé |

Une face cachée ainsi n'est pas cliquée non plus dans la vue 3D : les clics, la sélection de faces et le placement passent au travers vers ce qui est dessiné derrière. Les vues 2D montrent et sélectionnent toujours ces brushes. Il en va de même pour les brushes cachés par **Afficher les displacements** ou **Displacements seuls**.

## Astuces

- Les cordons estompent et verrouillent tout ce qui est hors d'une boîte sans le masquer, souvent mieux pour travailler sur une pièce (voir [Cordons](cordons.md)).
- La [vue joueur](player-view.md) masque les textures outils comme le jeu ; appuyez sur `N` pour les afficher.
- Outils MCP : `hide_objects` (avec `unselected` pour Masquer le reste), `isolate_objects`, `hidden_to_visgroup`, `get_visibility` (voir la [référence des outils MCP](../../MCP.md), en anglais).

## FAQ

**Il manque une partie de la map après la compilation.** Elle était masquée. Appuyez sur `U` pour tout afficher, ou cochez **Compiler les objets masqués**.

**Pourquoi je ne peux pas sélectionner un objet masqué depuis l'arborescence ?** Les objets masqués ne sont jamais sélectionnés. Affichez-le d'abord avec son œil.

**Masquer modifie-t-il la map ?** Masquer est une étape d'annulation et est enregistré avec la map, mais les objets eux-mêmes ne changent pas.

## Pages liées

- [Visgroups](visgroups.md)
- [Cordons](cordons.md)
- [Arborescence](outliner.md)
- [Modes de rendu et options d'affichage](render-modes-display.md)
- [Raccourcis clavier](keyboard-shortcuts.md)

---
[Retour à l'index de la documentation](../README.md)
