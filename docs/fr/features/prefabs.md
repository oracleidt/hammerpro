[Documentation](../README.md) > Fonctionnalités > Prefabs | [English](../../en/features/prefabs.md) | [Русский](../../ru/features/prefabs.md)

# Prefabs

Un prefab est un morceau de map enregistré dans un fichier VMF (un escalier, un luminaire, un distributeur avec ses entités) que vous insérez autant de fois que vous voulez. Contrairement à une [instance](instances.md), un prefab inséré devient des objets ordinaires de votre map. L'onglet **Prefabs** est votre bibliothèque de prefabs, avec dossiers, recherche et vignettes.

## Accès

| Action | Menu | Raccourci par défaut |
| --- | --- | --- |
| Onglet Prefabs | **Affichage > Panneaux > Prefabs** | aucun |
| **Créer un prefab...** | **Outils > Créer un prefab...** | `Ctrl+R` |
| **Insérer le prefab à sa taille d'origine** | **Outils > Insérer le prefab à sa taille d'origine** | `Alt+Maj+C` |

## La bibliothèque

L'onglet liste les fichiers `.vmf` des dossiers de prefabs, analysés en arrière-plan. Les sous-dossiers sont des catégories. Les dossiers sont :

| Racine | Où | Modifiable |
| --- | --- | --- |
| **Mes prefabs** | Le dossier de la bibliothèque : `%APPDATA%\HammerPro\prefabs` par défaut, ou celui choisi dans les paramètres. | Oui |
| **Dossier de prefabs** | Chaque dossier supplémentaire ajouté dans les paramètres. | Non |
| Le jeu (**Garry's Mod**) | Les dossiers de prefabs de Hammer dans l'installation du jeu (`bin/prefabs`, `bin/win64/prefabs`). | Non |

Commandes de l'onglet :

| Commande | Effet |
| --- | --- |
| **+** | **Créer un prefab à partir de la sélection...** (comme `Ctrl+R`). |
| Actualiser | **Analyser à nouveau les dossiers de prefabs**. |
| Dossier | **Ouvrir mon dossier de prefabs**. |
| Curseur | **Taille des vignettes** (56 à 192 points). |
| **Rechercher un prefab...** | Filtre les vignettes par nom. |
| Liste des catégories | **Tous les prefabs** ou un dossier. |
| Compteur | « N prefabs sur M » (les très gros dossiers ne listent que leurs premiers fichiers). |

Chaque vignette est dessinée en arrière-plan et mise en cache sur le disque, l'onglet s'ouvre donc instantanément la fois suivante. L'infobulle donne le nom, la catégorie et le chemin.

| Geste sur une vignette | Effet |
| --- | --- |
| Clic | Sélectionne le prefab (pour **Insérer le prefab à sa taille d'origine**). |
| Double-clic | Arme le prefab pour l'[outil entité](entity-tool.md). |
| Glisser dans une vue | L'insère là où vous le déposez. |
| Clic droit | **Placer avec l'outil entité**, **Insérer à la taille d'origine**, **Afficher dans le dossier**. |

## Insérer un prefab

- **Insérer le prefab à sa taille d'origine** (`Alt+Maj+C`) : le prefab sélectionné dans l'onglet va au centre de la vue 2D, à sa taille d'origine (sa profondeur est conservée).
- **Outil entité** : après un double-clic (ou **Placer avec l'outil entité**), l'outil entité place le prefab jusqu'à ce que vous appuyiez sur `Échap`. Dans la vue 3D, un clic le pose sur la surface sous le curseur à sa taille d'origine. Dans une vue 2D, un clic le centre sur le point de grille, et un glisser l'insère mis à l'échelle du rectangle tracé. Les décalages sont alignés sur la grille : un prefab construit sur la grille y reste. La barre d'options affiche **Prefab :** avec sa taille, une case **Grouper** et **Revenir aux entités** (« Placer à nouveau des entités au lieu du prefab (Échap) »).
- **Glisser-déposer** depuis l'onglet dans une vue.

Chaque insertion est une étape d'annulation « Insertion de prefab ». Les objets reçoivent de nouveaux ids, sont groupés quand **Grouper les prefabs insérés** est activé (sauf s'ils forment déjà un seul groupe) et sont sélectionnés.

### Le jeton `&i`

Écrivez `&i` dans les noms à l'intérieur d'un prefab (par exemple `porte_&i` et une sortie qui vise `porte_&i`). Chaque insertion remplace tous les `&i` des valeurs de clés et des sorties (noms, noms de parents, cibles, paramètres de sortie) par un numéro unique dans la map : chaque copie a ses propres noms, et les noms qui visent l'extérieur du prefab restent tels quels.

## Créer un prefab

Sélectionnez les objets et appuyez sur `Ctrl+R`. La fenêtre **Créer un prefab** demande :

| Champ | Effet |
| --- | --- |
| **Nom :** | Nom du fichier du prefab. |
| **Dossier :** | Un dossier de votre bibliothèque, **(bibliothèque)** pour sa racine, **ou un nouveau dossier** (sous-dossiers avec `/`). Créé si besoin. |
| **Remplacer le prefab existant** | Affiché quand le nom existe déjà. |

**Enregistrer** écrit le VMF en arrière-plan (écriture atomique). Les noms et dossiers ne peuvent pas contenir `\ / : * ? " < > |` ni de nom réservé.

## Options

**Outils > Options... > Prefabs** (`[placement]` dans `settings.toml`) :

| Option | Effet | Défaut |
| --- | --- | --- |
| **Afficher l'onglet Prefabs** (`show_prefabs`) | Affiche l'onglet dans le dock de droite. | Activé |
| **Dossier de la bibliothèque :** (`prefab_library`) | Où les nouveaux prefabs sont enregistrés. **Parcourir...**, **Utiliser le dossier par défaut**. | `%APPDATA%\HammerPro\prefabs` |
| **Autres dossiers de prefabs (lecture seule) :** (`prefab_dirs`) | Dossiers supplémentaires listés en lecture seule. **Ajouter un dossier...**, retrait avec la croix. | Aucun |
| **Lister les prefabs de Garry's Mod** (`game_prefabs`) | Liste les dossiers de prefabs de Hammer dans l'installation du jeu, en lecture seule. | Activé |
| **Grouper les prefabs insérés** (`group_prefabs`) | Les objets de chaque prefab inséré forment un groupe. | Activé |
| Taille des vignettes (`prefab_tile`) | Taille des vignettes, de 56 à 192. | 96 |

## Astuces

- Les prefabs de Hammer (le dossier `bin/prefabs` du jeu) marchent tels quels.
- Construisez vos prefabs sur la grille près de l'origine : les insertions alignent leur décalage sur la grille.
- Outils MCP : `list_prefabs`, `insert_prefab`, `create_prefab` (voir la [référence des outils MCP](../../MCP.md), en anglais).

## FAQ

**Prefab ou instance ?** Un prefab est copié dans votre map puis vit sa vie. Une [instance](instances.md) reste liée à son fichier : modifiez le fichier et toutes les copies changent.

**Mon nouveau prefab n'apparaît pas.** La bibliothèque est analysée en arrière-plan ; cliquez sur le bouton d'actualisation si vous avez ajouté des fichiers à la main.

**Deux copies de mon prefab déclenchent les sorties l'une de l'autre.** Utilisez `&i` dans les noms pour que chaque copie ait des noms uniques.

## Pages liées

- [Outil entité](entity-tool.md)
- [Instances](instances.md)
- [Outil Sprinkle](sprinkle-tool.md)
- [Fichiers et dossiers](../reference/files-and-folders.md)

---
[Retour à l'index de la documentation](../README.md)
