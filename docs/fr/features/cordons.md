[Documentation](../README.md) > Fonctionnalités > Cordons | [English](../../en/features/cordons.md) | [Русский](../../ru/features/cordons.md)

# Cordons

Un cordon est une boîte nommée qui délimite la partie de la map sur laquelle vous travaillez. Quand les cordons sont utilisés, tout ce qui se trouve en dehors des cordons actifs est estompé dans les vues, ne peut plus être sélectionné et peut être exclu de la compilation : vous testez une pièce en quelques secondes au lieu de compiler toute la map.

## Accès

- **Outils > Outil actif > Outil cordon** (ou son bouton dans la palette d'outils) dessine et modifie les boîtes dans les vues 2D. Il n'a pas de raccourci par défaut : donnez-lui en un dans **Paramètres > Clavier** (voir [Raccourcis clavier](keyboard-shortcuts.md)).
- **Affichage > Utiliser les cordons** active ou désactive les cordons pour toute la map. Le bouton cordon de la barre d'outils fait la même chose. Sans aucune boîte de cordon active, la map est enregistrée avec les cordons désactivés, pour que Hammer et Hammer++ affichent toujours toute la map.
- La section **Cordons** de l'onglet **Visgroups** (dock de droite) liste tous les cordons de la map.
- Quand les cordons sont utilisés, un badge rouge **Cordon** apparaît dans la barre d'état. Son infobulle donne le nombre de boîtes ; un clic arrête d'utiliser les cordons.

Les cordons sont enregistrés dans le VMF comme le fait Hammer : une map avec un seul cordon est écrite avec l'ancien bloc `cordon` de Hammer (sauf si le fichier utilisait déjà le nouveau bloc), une map avec plusieurs cordons avec le bloc `cordons` plus récent. Les cordons à plusieurs boîtes écrits par d'autres éditeurs gardent toutes leurs boîtes à l'enregistrement (l'outil modifie la première boîte de chaque cordon, les autres sont dessinées en discret).

## L'outil cordon

L'outil modifie un cordon à la fois, la **cible**. Choisissez-la dans la liste **Cordon :** de la barre d'options, en cliquant sur sa ligne dans la liste des cordons, avec **Modifier la boîte avec l'outil cordon** dans cette liste, ou en cliquant dans sa boîte dans une vue 2D (la plus petite boîte gagne quand des boîtes se chevauchent).

| Geste (vues 2D) | Effet |
| --- | --- |
| Glisser dans le vide | Redessine la boîte du cordon cible. Sa profondeur sur l'axe que vous ne voyez pas est conservée. |
| Glisser une poignée | Redimensionne la boîte. |
| Glisser à l'intérieur de la boîte | Déplace la boîte. |
| Cliquer dans un autre cordon | En fait la cible. |
| `Échap` | Annule le glissement en cours, puis quitte le mode **Nouveau cordon**. |

Chaque modification est appliquée au relâchement du bouton, en une seule étape d'annulation. Un glissement qui ne dessine aucune surface garde la boîte telle quelle. La vue 3D affiche les boîtes mais ne les modifie pas.

La profondeur d'une nouvelle boîte (l'axe invisible dans la vue où vous dessinez) vient du cordon cible, sinon de la sélection, sinon de la map avec un pas de grille de marge, sinon de la boîte par défaut de Hammer (-1024 à 1024).

### Barre d'options

| Option | Effet |
| --- | --- |
| **Cordon :** | Le cordon modifié par l'outil. Affiche **Nouveau cordon** quand la prochaine boîte sera un nouveau cordon, **(aucun)** quand la map n'a pas de cordon. |
| Bouton **+** | **Nouveau cordon** : la prochaine boîte dessinée devient un nouveau cordon. |
| **Actif** | Inclut ce cordon dans la zone cordonnée. |
| **Inclure** / **Exclure** | Le mode du cordon cible (voir [Cordons d'exclusion](#cordons-dexclusion)). |
| Taille | La taille de la boîte cible. |
| Bouton **x** | Supprime ce cordon. |
| **Utiliser les cordons** | Comme **Affichage > Utiliser les cordons**. |

### Menu du clic droit

| Entrée | Effet |
| --- | --- |
| **Nouveau cordon autour de la sélection** | Un nouveau cordon ajusté aux objets sélectionnés. Une sélection plate reçoit un pas de grille d'épaisseur. Demande une sélection. |
| **Supprimer ce cordon** | Supprime le cordon cible. |
| **Utiliser les cordons** / **Ne plus utiliser les cordons** | Active ou désactive les cordons. |

L'aide de la barre d'état rappelle les gestes et ajoute « (Les cordons ne sont pas utilisés.) » quand les cordons sont désactivés.

## La liste des cordons

La section **Cordons** de l'onglet [Visgroups](visgroups.md) contient :

| Élément | Effet |
| --- | --- |
| **+** | Nouveau cordon autour de la sélection, ou un cube au centre des vues 2D si rien n'est sélectionné. Le nom passe directement en mode renommage. |
| **Utiliser** | Utiliser les cordons (comme **Affichage > Utiliser les cordons**). |
| Compteur | Nombre de cordons. |
| Case d'une ligne | Actif : inclut le cordon dans la zone cordonnée. |
| Bouton **+** / **-** d'une ligne | Le mode du cordon : inclusion (+) ou exclusion (-). Un clic l'inverse. Le nom des cordons d'exclusion est en orange. |
| Clic sur une ligne | En fait le cordon modifié par l'outil cordon. L'infobulle donne ses coins et sa taille. |
| Double-clic | Renomme le cordon. |
| Bouton de cadrage | **Cadrer les vues sur ce cordon**. |
| Bouton boîte | **Modifier la boîte avec l'outil cordon**. |
| Bouton **x** | **Supprimer le cordon**. |
| Clic droit | **En faire un cordon d'exclusion** (ou **En faire un cordon d'inclusion**), **Modifier la boîte avec l'outil cordon**, **Cadrer les vues sur ce cordon**, **Renommer**, **Supprimer le cordon**. |

Un cordon sans volume affiche « Ce cordon n'a pas de volume : il sera supprimé à sa prochaine modification. »

## Ce que fait « utiliser les cordons »

Les cordons sont utilisés quand **Utiliser les cordons** est coché et qu'au moins un cordon actif a une boîte valide. Alors :

- les boîtes des cordons actifs sont dessinées en rouge dans toutes les vues ;
- les vues 2D sont ombrées en dehors des cordons actifs (réglage plus bas) ;
- les objets entièrement en dehors de toutes les boîtes actives sont estompés et ne peuvent pas être sélectionnés. Un objet qui touche une boîte compte comme à l'intérieur ;
- le nom de chaque cordon actif s'affiche dans son coin quand il y en a plusieurs.

Activer, désactiver, ajouter, renommer, redimensionner ou supprimer un cordon sont des étapes d'annulation.

## Compiler seulement le cordon

Dans la [fenêtre Compiler et lancer](../compiler/compile-window.md), **Compiler uniquement la zone du cordon** ne garde que ce qui touche les boîtes actives. Chaque boîte est fermée par six brushes du **Matériau de scellement du cordon**, et la map est compilée sous le nom `<map>_cordon` : la map complète n'est jamais écrasée. S'il n'y a aucun point d'apparition dans le cordon, un est ajouté au sol de la première boîte. La même option existe dans la page **Compilation** des paramètres (**Compiler seulement le cordon actif**).

## Cordons d'exclusion

Un cordon est par défaut un cordon d'**inclusion** : seul ce qui touche les cordons d'inclusion est gardé. Passez un cordon en **Exclure** (barre d'options de l'outil cordon, bouton **+** / **-** de sa ligne, ou son menu du clic droit) pour faire l'inverse : ce qui est entièrement dans la boîte est estompé dans les vues, ne peut pas être sélectionné et est retiré des compilations du cordon, tout le reste est gardé. Pratique pour cacher un bâtiment, une zone lourde ou une partie cassée pendant que vous testez le reste de la map.

- Les cordons d'exclusion sont dessinés en orange ; les vues 2D ombrent leur intérieur.
- Un objet qui traverse le bord d'une boîte d'exclusion est gardé (seul ce qui est entièrement dedans est retiré), comme un objet qui touche une boîte d'inclusion est gardé.
- Les ensembles mixtes fonctionnent : les cordons d'inclusion définissent la zone gardée, les cordons d'exclusion y découpent des trous. Avec seulement des cordons d'exclusion, toute la map est compilée moins ce qu'ils retirent, sans boîte de scellement ; retirer un mur peut faire fuir la map, la compilation le signale comme d'habitude.
- Le journal de compilation indique combien de boîtes d'exclusion ont été appliquées, et la compilation s'appelle toujours `<map>_cordon`.
- Enregistrement : un cordon d'exclusion est écrit dans le bloc `cordons` avec `"active" "0"` et deux clés en plus, `"hp_mode" "exclude"` et `"hp_active"`, que Hammer et Hammer++ ignorent. Ils ouvrent la map normalement et ne traitent jamais un cordon d'exclusion comme un cordon (la règle de la 2.0.9 reste : le fichier n'est enregistré comme cordonné que si un cordon d'inclusion est actif).

## Options

**Outils > Options... > Cordons et visgroups** (`[visibility]` dans `settings.toml`) :

| Option | Effet | Défaut |
| --- | --- | --- |
| **Matériau du cordon** (`cordon_material`) | Matériau des brushes qui ferment la zone cordonnée quand seuls les cordons sont compilés. **Utiliser le matériau actuel** reprend le matériau courant. Une valeur vide utilise le défaut. | `tools/toolsskybox` |
| **Ombrer l'extérieur des cordons en 2D** (`shade_outside_2d`) | Assombrit les vues 2D en dehors des cordons actifs quand les cordons sont utilisés. | Activé |
| **Opacité de l'ombre** (`shade_opacity`) | Opacité de cette ombre, de 0 à 1. | 0,45 |
| **Noms des cordons en 2D** (`show_cordon_names`) | Affiche le nom de chaque cordon actif dans son coin quand il y en a plusieurs. | Activé |

## Astuces

- Gardez un cordon par zone que vous testez souvent (une pièce, un parcours de véhicule) et passez de l'un à l'autre avec leur case **Actif** au lieu de redessiner les boîtes.
- L'aperçu de l'éclairage a son propre mode local : **Précalculer autour de la caméra** ferme une zone autour de la caméra 3D comme un cordon (voir [Aperçu de l'éclairage](lighting-preview.md)).
- Outils MCP : `list_cordons`, `add_cordon`, `edit_cordon`, `set_cordons_enabled`, `get_visibility` (voir la [référence des outils MCP](../../MCP.md), en anglais).

## FAQ

**Puis-je cacher une zone au lieu de tout le reste ?** Oui : faites de son cordon un cordon d'**exclusion**. Voir [Cordons d'exclusion](#cordons-dexclusion).

**Ma compilation du cordon est vide.** Le journal avertit « Les cordons ne gardent aucun brush » quand rien ne touche les cordons d'inclusion : la map reste une boîte scellée valide, mais vide. Déplacez ou agrandissez le cordon sur vos brushs et déplacements.

**J'ai dessiné un cordon mais rien n'est estompé.** Les cordons ne sont pas utilisés. Cochez **Utiliser les cordons** (barre d'options, liste des cordons ou menu Affichage) et vérifiez que le cordon est **Actif**.

**Pourquoi je n'arrive pas à sélectionner un objet ?** Il est en dehors de tous les cordons actifs. Arrêtez d'utiliser les cordons ou agrandissez la boîte.

**Une compilation de cordon écrase-t-elle ma map ?** Non. Elle s'appelle `<map>_cordon`.

**Un cordon peut-il avoir plusieurs boîtes ?** Hammer Pro garde les boîtes supplémentaires des maps faites ailleurs et les utilise dans les vues et les compilations, mais il crée une boîte par cordon. Faites plutôt plusieurs cordons.

## Pages liées

- [Visgroups](visgroups.md)
- [Masquer et isoler](hide-isolate.md)
- [Fenêtre Compiler et lancer](../compiler/compile-window.md)
- [Référence des paramètres](../reference/settings.md)

---
[Retour à l'index de la documentation](../README.md)
