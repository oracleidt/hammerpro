[Documentation](../README.md) > Fonctionnalités > Instances | [English](../../en/features/instances.md) | [Русский](../../ru/features/instances.md)

# Instances

Une `func_instance` place le contenu d'un autre fichier VMF dans votre map : construisez une porte, une lampe ou une pièce entière une fois, placez-la autant de fois que vous voulez, modifiez le fichier et toutes les copies suivent. Hammer Pro dessine le contenu de chaque instance, modifie les fichiers d'instance, fusionne les instances dans la map, et son compilateur les fusionne comme VBSP.

## Accès

**Outils > Instances** :

| Action | Effet | Raccourci par défaut |
| --- | --- | --- |
| **Créer une instance depuis la sélection...** | Enregistre la sélection dans un nouveau fichier d'instance et la remplace par une `func_instance`. | `Alt+S` |
| **Modifier l'instance** | Ouvre le fichier de l'instance sélectionnée. | aucun |
| **Fusionner les instances sélectionnées** | Remplace les instances sélectionnées par leur contenu. | aucun |
| **Fusionner toutes les instances...** | Pareil pour toutes les instances visibles de la map, après confirmation. | aucun |
| **Instances : masquées** / **Instances : teintées** / **Instances : normales** | Comment le contenu des instances est dessiné. | aucun |

L'onglet **Instance** du dock de droite montre le fichier et les paramètres de l'instance sélectionnée.

## Comment une instance est fusionnée

Les règles sont celles de VBSP, utilisées par les vues, par la fusion et par le compilateur :

- le contenu est déplacé par l'`origin` et tourné par les `angles` de la `func_instance` (brushes avec verrouillage des textures, clés des entités selon leur type FGD) ;
- chaque valeur de clé et de sortie reçoit les remplacements de l'instance (clés `replace01`... contenant `$variable valeur`, toutes les occurrences, sans tenir compte de la casse) ;
- les noms d'entités, les cibles et les cibles des sorties sont corrigés avec le nom de l'instance : `nom-x` (préfixe, par défaut), `x-nom` (suffixe) ou laissés tels quels (aucun). Les noms qui commencent par `@` (globaux) ou `!` (comme `!activator`) ne changent jamais ; une instance sans nom utilise `InstanceAuto<n>` ;
- les entités `func_instance_parms` sont retirées ;
- les instances imbriquées sont fusionnées aussi, avec une protection contre une instance qui se contient elle-même, une limite de profondeur et une limite de taille.

Les fichiers d'instance sont cherchés depuis le dossier de la map (la clé `file` est relative à ce dossier), puis dans le contenu du jeu (en lecture seule).

## Affichage

| Mode | Ce que vous voyez |
| --- | --- |
| **Instances : masquées** | Seulement l'entité `func_instance`. |
| **Instances : teintées** (défaut) | Le contenu, teinté, comme dans Hammer. |
| **Instances : normales** | Le contenu dessiné comme le reste de la map. |

Le contenu des instances sélectionnées est encadré (instances imbriquées comprises) et le nom de leur fichier est affiché dans toutes les vues. Une instance dont le fichier est introuvable ou illisible reçoit une croix rouge et une étiquette à son origine.

## Créer une instance depuis la sélection

`Alt+S` (la touche de Hammer++) ouvre **Créer une instance depuis la sélection** :

| Champ | Effet |
| --- | --- |
| **Nom du fichier :** | Nom du nouveau VMF (pas de séparateur de dossier, ni `: * ? " < > \|`). |
| **Dossier :** | Où l'enregistrer, avec **Parcourir...**. Par défaut le **Dossier par défaut** des paramètres (`instances`, relatif au dossier de la map). Une map pas encore enregistrée demande un chemin complet. |
| **Origine :** | X, Y, Z. La sélection est enregistrée autour de ce point, qui devient l'origine de la `func_instance`. Part du centre de la sélection, aligné sur la grille. |
| **Remplacer le fichier existant** | Affiché quand le fichier existe déjà. |
| Clé du fichier | La valeur `file` que recevra l'instance (relative au dossier de la map quand c'est possible). |

**Créer** écrit le fichier en arrière-plan (écriture atomique), puis remplace la sélection par une `func_instance` de ce fichier en une étape d'annulation. La sélection est décalée de moins l'origine (textures verrouillées) ; les groupes entièrement sélectionnés sont conservés, les visgroups non.

## Modifier l'instance

**Modifier l'instance** (ou le bouton du même nom dans l'onglet Instance) ouvre le fichier de l'instance dans un nouvel onglet, la map parente restant ouverte. Quand **Ouvrir les maps dans de nouveaux onglets** est désactivé, il s'ouvre dans une nouvelle fenêtre Hammer Pro. Avec **Afficher la map autour d'une instance modifiée**, la map parente est dessinée autour de l'instance, teintée et en lecture seule. Quand vous enregistrez le fichier de l'instance, la map parente redessine ses instances. Un fichier qui vient du contenu du jeu ne peut pas être modifié.

## Fusion

**Fusionner les instances sélectionnées** et **Fusionner toutes les instances...** remplacent les instances par leur contenu corrigé (instances imbriquées comprises). Le contenu rejoint les visgroups de l'instance qu'il remplace, les objets créés sont sélectionnés, et le tout tient en une étape d'annulation. Les instances dont le fichier est introuvable sont ignorées et signalées. **Fusionner toutes les instances...** demande d'abord (« Remplacer les N instances visibles de la map par leur contenu ? ») et précise « Les objets masqués dans les fichiers des instances sont ignorés. » quand ce réglage est activé.

## L'onglet Instance

Avec une `func_instance` sélectionnée (la première quand il y en a plusieurs), l'onglet affiche :

- le fichier (**Fichier :**), s'il a été trouvé, ou « Lu depuis le contenu du jeu (lecture seule) » ;
- les boutons **Modifier l'instance** et **Fusionner les instances sélectionnées** ;
- **Paramètres de l'instance** : un champ par paramètre déclaré par l'entité `func_instance_parms` du fichier (`parmN` = `$nom type [défaut]`), typé selon son type FGD (case à cocher, nombres, couleur, texte). Un champ écrit la clé `replaceNN` correspondante (une nouvelle valeur prend le premier emplacement libre). **Retirer la valeur** l'efface (« Non défini : l'instance garde {défaut} ») ;
- **Autres remplacements** : les remplacements qui ne correspondent à aucun paramètre déclaré, en lignes brutes, avec **Ajouter un remplacement ($variable valeur)**.

Les mêmes champs de paramètres apparaissent sous les clés d'une `func_instance` dans le panneau d'entité.

## Options

**Outils > Options... > Instances** (`[instances]` dans `settings.toml`) :

| Option | Effet | Défaut |
| --- | --- | --- |
| **Contenu des func_instance** (`display`) | **Masqué**, **Teinté** ou **Normal**. | Teinté |
| **Encadrer les instances sélectionnées** (`outline_selected`) | Dessine les limites du contenu des instances sélectionnées et le nom de leur fichier. | Activé |
| **Afficher la map autour d'une instance modifiée** (`show_context`) | Dessine la map parente autour d'une instance ouverte avec **Modifier l'instance**, teintée et en lecture seule. | Activé |
| **Dossier par défaut** (`folder`) | Où **Créer une instance depuis la sélection** enregistre les nouvelles instances : relatif au dossier de la map, ou un chemin complet. | `instances` |
| **Ignorer les objets masqués des fichiers d'instance** (`collapse_skips_hidden`) | Comme Hammer++ : les objets masqués dans un fichier d'instance ne sont pas ajoutés à la map lors de la fusion. | Activé |

## Astuces

- La compilation garde les `func_instance` et le compilateur intégré les fusionne avec les mêmes règles : inutile de fusionner avant de compiler.
- Utilisez des noms en `@` pour les entités qui doivent garder leur nom exact dans chaque copie (un relais global).
- Outils MCP : `list_func_instances`, `create_instance`, `set_instance_parameters`, `collapse_instances`, `make_instance`, `get_instance_file` (voir la [référence des outils MCP](../../MCP.md), en anglais).

## FAQ

**Une instance affiche une croix rouge.** Son fichier est absent ou illisible. Vérifiez la clé `file` : elle est relative au dossier de la map.

**La fusion dit que le fichier de l'instance est encore en chargement.** Les fichiers se chargent en arrière-plan ; Hammer Pro attend jusqu'à 30 secondes, puis abandonne sans rien changer. Réessayez quand le contenu s'affiche.

**Pourquoi je ne peux pas modifier une instance du jeu ?** Les fichiers lus dans le contenu du jeu sont en lecture seule. Copiez le fichier à côté de votre map et faites pointer la clé `file` sur la copie.

## Pages liées

- [Prefabs](prefabs.md)
- [Propriétés des entités](entity-properties.md)
- [Plusieurs maps](multiple-maps.md)
- [Présentation du compilateur](../compiler/overview.md)

---
[Retour à l'index de la documentation](../README.md)
