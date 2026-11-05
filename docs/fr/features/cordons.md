[Documentation](../README.md) > Fonctionnalités > Cordons | [English](../../en/features/cordons.md) | [Русский](../../ru/features/cordons.md)

# Cordons

Un cordon est une boîte nommée qui délimite la partie de la map sur laquelle vous travaillez. Quand les cordons sont utilisés, tout ce qui se trouve en dehors des cordons actifs est estompé dans les vues, ne peut plus être sélectionné et peut être exclu de la compilation : vous testez une pièce en quelques secondes au lieu de compiler toute la map.

## Accès

- **Outils > Outil cordon** (ou son bouton dans la palette d'outils) dessine et modifie les boîtes dans les vues 2D. Il n'a pas de raccourci par défaut : donnez-lui en un dans **Paramètres > Clavier** (voir [Raccourcis clavier](keyboard-shortcuts.md)).
- **Affichage > Utiliser les cordons** active ou désactive les cordons pour toute la map. Le bouton cordon de la barre d'outils fait la même chose.
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
| Clic sur une ligne | En fait le cordon modifié par l'outil cordon. L'infobulle donne ses coins et sa taille. |
| Double-clic | Renomme le cordon. |
| Bouton de cadrage | **Cadrer les vues sur ce cordon**. |
| Bouton boîte | **Modifier la boîte avec l'outil cordon**. |
| Bouton **x** | **Supprimer le cordon**. |
| Clic droit | **Modifier la boîte avec l'outil cordon**, **Cadrer les vues sur ce cordon**, **Renommer**, **Supprimer le cordon**. |

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
