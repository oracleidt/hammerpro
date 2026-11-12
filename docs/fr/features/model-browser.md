[Documentation](../README.md) > Fonctionnalités > Navigateur de modèles | [English](../../en/features/model-browser.md) | [Русский](../../ru/features/model-browser.md)

# Navigateur de modèles

Le navigateur de modèles liste tous les modèles (`.mdl`) du contenu du jeu monté, rangés par dossier, avec un aperçu 3D en direct, les informations du modèle, ses bodygroups et ses séquences, un inspecteur qui repère les fichiers et matériaux manquants (un « Crowbar lite ») et des outils pour extraire, décompiler et compiler des modèles. C'est aussi là que vous choisissez le modèle d'un prop.

## Ouvrir le navigateur

- **Affichage > Panneaux > Navigateur de modèles** affiche ou masque l'onglet **Modèles** du panneau de droite. L'onglet est masqué par défaut (`show_model_browser = false` dans la table `[ui]` de `settings.toml`).
- Le bouton de parcours à côté d'une clé de modèle, dans le panneau d'entité, ouvre l'onglet tout seul (voir « Choisir un modèle pour une clé » plus bas).
- Pas de raccourci par défaut. Avec le préréglage de touches Hammer++ (**Paramètres > Clavier**), c'est `Maj+F10`.

Pendant le montage du contenu du jeu, l'onglet affiche un indicateur de chargement. Sans dossier de jeu, il vous le dit et propose un bouton **Options...** qui ouvre la fenêtre des paramètres (voir [Configuration du jeu](../getting-started/game-setup.md)).

## Trouver un modèle

| Contrôle | Effet |
|---|---|
| **Rechercher des modèles** | Garde les modèles dont le chemin contient tous les mots tapés (sans tenir compte de la casse). Un mot qui commence par `!` exclut les modèles qui le contiennent : `vent office !dynamic`. |
| Pastilles de source **Tout**, **Jeu**, **Addons**, **Workshop** | Ne gardent que les modèles de ce type de source. Une pastille n'apparaît que si le contenu monté a ce type de source. |
| **Contenu de Garry's Mod uniquement** | Ne liste que ce que tous les joueurs de Garry's Mod possèdent (ni jeux montés, ni addons, ni Workshop). Partagé avec les navigateurs de sons et de particules, et enregistré (`gmod_only` dans `[browser]`, désactivé par défaut). |
| Liste des dossiers | **Tous les dossiers**, ou un dossier de premier niveau sous `models/` (par exemple `props_c17`). |
| **Modèles : N** | Nombre de modèles listés. |
| Bouton grille / liste (`⊞` / `☰`) | Bascule entre la grille de vignettes et la liste. Enregistré (`model_grid_view` dans `[browser]`, grille par défaut). |
| **+** | **Fichier > Importer > Importer un modèle (FBX, OBJ, glTF)...**, voir [Import de modèles](model-import.md). |
| **++** | **Fichier > Importer > Importer des modèles (lot)...** : beaucoup de fichiers ou de dossiers d'un coup, voir [Import par lot](model-import.md#import-par-lot). |
| **Récents (N)** | Les derniers modèles utilisés (8 affichés, 16 mémorisés pendant la session), au-dessus de la liste. Clic pour sélectionner, double-clic pour utiliser. |

Dans la liste, les modèles sont regroupés sous des en-têtes de dossier avec leur nombre ; cliquez un en-tête pour le replier ou le déplier. Chaque ligne montre le nom du fichier et, quand le contenu a plusieurs sources, un badge qui indique d'où vient le modèle : **GMod**, **jeu : cstrike**, **dossier garrysmod**, **addon**, **Workshop 123456**, **perso** (un dossier de contenu personnalisé). Survolez une ligne pour voir le chemin complet.

Dans la grille, chaque vignette est un rendu du modèle (ou, s'il ne peut pas être dessiné, la vignette de son premier matériau, ou un `?`). La taille des vignettes se règle avec le curseur **Taille des vignettes** du [navigateur de particules](particles.md) (48 à 256 points, 96 par défaut, `tile_size` dans `[browser]`). Survolez une vignette pour voir le chemin et la source.

Au clavier, pointeur sur la liste ou la grille : `Haut` / `Bas` déplacent la sélection, `Entrée` fait comme un double-clic.

## Utiliser un modèle

- **Faites glisser** un modèle de la liste ou de la grille dans une vue pour y poser un prop : une boîte montre où il atterrit (sur la surface sous le pointeur en 3D). Le prop est un `prop_static` si le modèle est un prop statique, sinon un `prop_dynamic`. Le skin choisi dans l'aperçu suit, et le modèle devient le modèle des nouveaux props.
- Le **double-clic** (ou `Entrée`) prend le modèle pour les nouveaux props, ou l'écrit dans la clé pour laquelle vous choisissez.
- Les boutons sous le nom du modèle :

| Bouton | Effet |
|---|---|
| **+ Utiliser pour les nouveaux props** | Active l'[outil entité](entity-tool.md), qui pose ce modèle en `prop_static` pour un modèle de prop statique, sinon en `prop_physics`, avec le skin de l'aperçu. |
| **Appliquer aux props sélectionnés (N)** | Met ce modèle (et le skin de l'aperçu s'il n'est pas le skin 0) sur les entités sélectionnées qui prennent un modèle (classes `prop_*`, et classes à qui le FGD donne un modèle studio). Une seule étape d'annulation. |
| **Appliquer à la propriété** | Seulement quand vous choisissez pour une clé : écrit le modèle dans cette clé. |
| **Copier** | Copie le chemin du modèle dans le presse-papiers. |

## Choisir un modèle pour une clé

Le bouton de parcours d'une clé de modèle dans le panneau d'entité (voir [Propriétés des entités](entity-properties.md)) ouvre l'onglet Modèles avec un bandeau : « Choisissez un modèle pour *classe (clé)*, puis double-cliquez dessus ou appuyez sur Appliquer. » Le modèle que la clé contient déjà est sélectionné. Double-cliquez un modèle, appuyez sur `Entrée` ou cliquez **Appliquer** pour l'écrire (avec le skin de l'aperçu quand la clé est `model` et que le skin n'est pas 0) ; le panneau revient sur l'onglet **Entité**. **Annuler** quitte ce mode. L'écriture tient en une seule étape d'annulation.

## Aperçu

L'aperçu montre le modèle sélectionné en 3D :

- Glisser pour tourner autour, molette pour zoomer, double-clic pour réinitialiser.
- Liste **Skin N** : le skin affiché (active si le modèle en a plusieurs). Elle revient au skin 0 quand vous sélectionnez un autre modèle.
- `▦` bascule le filaire, `↺` réinitialise la vue.
- Un modèle tout juste sélectionné joue sa première séquence, comme en jeu.

La section **Affichage** dessine des aides par-dessus le modèle (les options du Half-Life Model Viewer) :

| Option | Montre |
|---|---|
| **Filaire** | Le maillage en lignes. |
| **Boîte englobante** | La boîte autour du maillage visible. |
| **Box de mouvement** | Le hull de mouvement de l'en-tête du modèle (`$bbox`), la boîte que Hammer utilise pour placer les entités comme `info_player_start`. |
| **Hitboxes** | Les hitboxes du set par défaut, en pose de référence. |
| **Boîte de séquence** | La boîte englobante de la séquence jouée (sinon la première). |
| **Os** | Le squelette en pose de référence, articulations en orange. |
| **Attachments** | Les points d'attache (`$attachment`). |
| **Normales** | Les normales des sommets. |
| **Axes d'origine** | X en rouge, Y en vert, Z en bleu à l'origine du modèle. |
| **Position d'éclairage** | Le point depuis lequel le moteur éclaire le modèle (`$illumposition`). |
| **Sol** | Une grille à la hauteur de l'origine du modèle. |
| **Modèle physique** | Le modèle de collision du fichier `.phy`. |
| **Champ de vision** | 10 à 120 degrés, 40 par défaut. |

Ces aides et la caméra valent pour la session ; elles ne sont pas enregistrées.

## Onglets d'informations

**Informations** : taille, boîte minimum et maximum, taille du hull de collision (si le modèle en a un), triangles et sommets (avec le total sur tous les bodygroups s'il diffère), maillages (draw calls), matériaux, os, hitboxes, LODs, skins, parties du corps, séquences, prop statique (oui ou non), propriété de surface, masse (si connue) et version MDL. **Matériaux (N)** déplie la liste des matériaux.

Quand un prop sélectionné utilise un autre modèle, **Remplacer partout (N)** donne le modèle du navigateur à toutes les entités de la map qui utilisent le modèle de ce prop (une seule étape d'annulation).

**Groupes de corps (N)** : une liste par bodygroup qui offre un choix. Elle part de la valeur `body` du premier prop sélectionné qui utilise ce modèle. **Valeur de la clé body** affiche le nombre obtenu, et **Appliquer aux props sélectionnés (N)** l'écrit dans leur clé `body`.

**Séquences (N)** : les séquences avec leur nombre d'images. Cliquez-en une pour la jouer dans l'aperçu ; `▶` / `⏸` lance ou met en pause, `↺` redémarre. Double-cliquez une séquence, ou cliquez **Définir comme animation par défaut des props sélectionnés (N)**, pour l'écrire dans leur clé `DefaultAnim`.

## Inspecteur (« Crowbar lite »)

L'onglet **Inspecteur** lit l'en-tête du modèle (sans la géométrie) et vérifie les fichiers autour :

- Une ligne d'état : **Tous les fichiers et matériaux sont présents**, ou **Fichiers ou matériaux manquants : N**.
- Des compteurs : os, attaches, séquences, parties du corps, contrôleurs flex, niveaux de détail (lus dans le VVD), et **Noms des attaches (N)**.
- Les fichiers compagnons (`.vvd`, `.vtx`, `.phy` s'il existe) ; un fichier manquant est en rouge. Un modèle sans collision n'a pas de `.phy` exprès, ce n'est pas une erreur.
- **Modèles inclus (N)** (`$includemodel`), ceux qui manquent en rouge.
- Une section par skin avec ses matériaux : un VMT introuvable en rouge (« Matériau (VMT) introuvable »), un VMT dont la texture de base manque en orange (« Texture introuvable : ... »). Les skins à problème sont dépliés.

Les outils en dessous :

| Bouton | Effet |
|---|---|
| **Aperçu de la forme** | Ouvre la fenêtre d'[aperçu de la forme](real-shape-placement.md#aperçu-de-la-forme) sur ce modèle : sa forme réelle de dessus, de face, de côté et en 3D, avec ses dimensions, son origine, son point le plus bas et son maillage de collision. |
| **Extraire les fichiers...** | Copie le modèle et ses fichiers VVD, VTX, PHY et ANI depuis le contenu du jeu (VPK, GMA ou dossiers) vers le dossier de votre choix, en gardant leurs dossiers (`models/props/x.mdl`). |
| **Décompiler avec Crowbar** | Extrait le modèle dans un dossier temporaire (`hammerpro_models` dans le dossier temporaire du système) et l'ouvre dans Crowbar. Crowbar.exe est demandé la première fois puis mémorisé (`crowbar_exe` dans `[entities]`) ; clic droit sur le bouton puis **Choisir un autre Crowbar.exe...** pour en changer. |
| **Compiler un QC...** | Lance le studiomdl du jeu (`bin/win64/studiomdl.exe`, sinon `bin/studiomdl.exe` du dossier de Garry's Mod) sur le fichier QC choisi, avec `-game <dossier du jeu> -nop4 -verbose`. Sa sortie va dans la console ; un bouton **Annuler** l'arrête. Grisé si studiomdl est introuvable. |

## Modèles de collision dans les vues

Deux options de **Affichage > Options d'affichage** montrent les collisions des props :

| Option | Effet | Défaut |
|---|---|---|
| **Modèles de collision** | Dessine l'enveloppe de collision des props par-dessus dans la vue 3D, et dans l'aperçu du navigateur de modèles. Aussi dans **Paramètres > Rendu**. `collision_models` dans `[render]`. | Désactivé |
| **Afficher les collisions des modèles** | Les vues 3D montrent chaque modèle par son seul modèle de collision (ce que le joueur percute) ; une boîte orange signale un modèle sans modèle de collision. Aussi un bouton de la barre d'outils. `collision_only` dans `[render]`. | Désactivé |

## Paramètres

| Clé | Effet | Défaut |
|---|---|---|
| `[ui] show_model_browser` | L'onglet Modèles est dans le panneau. | `false` |
| `[browser] model_grid_view` | Grille au lieu de liste. | `true` |
| `[browser] tile_size` | Taille des vignettes de la grille (48 à 256). | `96` |
| `[browser] gmod_only` | Contenu de Garry's Mod uniquement (modèles, sons, particules). | `false` |
| `[entities] crowbar_exe` | Crowbar choisi depuis l'inspecteur. | aucun |
| `[render] collision_models` | Modèles de collision par-dessus les props. | `false` |
| `[render] collision_only` | Modèles de collision à la place des modèles. | `false` |

## Astuces

- Choisissez le skin dans l'aperçu avant de faire glisser : le prop posé le reçoit.
- L'onglet **Inspecteur** est le moyen le plus rapide de comprendre pourquoi un modèle s'affiche en damier violet et noir en jeu.
- Via MCP, un assistant IA peut utiliser `search_models`, `get_model_info` et `list_used_models` (voir la [référence des outils MCP](../../MCP.md), en anglais).

## FAQ

**Pourquoi l'onglet Modèles n'apparaît pas ?** Il est masqué par défaut : activez **Affichage > Panneaux > Navigateur de modèles**.

**La grille montre une texture au lieu du modèle, pourquoi ?** Le modèle n'a pas (encore) pu être rendu ; la vignette se rabat sur son premier matériau, ou sur `?` s'il n'en a pas.

**Pourquoi « Compiler un QC... » est grisé ?** studiomdl est introuvable dans le dossier `bin` de l'installation de Garry's Mod configurée.

**Un glisser donne un `prop_dynamic`, un double-clic un `prop_physics` : pourquoi ?** Un dépôt utilise `prop_dynamic` pour les modèles qui ne sont pas des props statiques, alors que **Utiliser pour les nouveaux props** utilise `prop_physics`. Changez la classe dans le panneau d'entité si vous voulez autre chose.

## Pages liées

- [Outil entité](entity-tool.md)
- [Propriétés des entités](entity-properties.md)
- [Import de modèles](model-import.md)
- [Propper](propper.md)
- [Modes de rendu et affichage](render-modes-display.md)
- [Assets de la map](map-assets.md)
- [Référence des paramètres](../reference/settings.md)

---
[Retour à l'index de la documentation](../README.md)
