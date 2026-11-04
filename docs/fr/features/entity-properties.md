[Documentation](../README.md) > Fonctionnalités > Propriétés des entités | [English](../../en/features/entity-properties.md)

# Propriétés des entités

L'onglet **Entité** du panneau de droite, c'est l'Object Properties de Hammer : il modifie les keyvalues, les indicateurs (flags), les sorties, le modèle et les visgroups des entités sélectionnées, avec un éditeur typé (SmartEdit) piloté par les données du jeu (FGD). Autour de lui, Hammer Pro propose des outils qui travaillent sur les clés des entités : pipettes, éditeur de couleur, copier-coller des keyvalues, distance maximale, aide de l'entité, recherche d'entités, rechercher et remplacer dans les entités, rapport des entités et propriétés de la map.

## Ouverture

- **Édition > Propriétés** (`Alt+Enter`) ou **Affichage > Panneaux > Propriétés de l'entité** affiche l'onglet **Entité** du panneau de droite.
- L'onglet suit toujours la sélection :
  - une ou plusieurs entités sélectionnées : leurs clés (plusieurs entités se modifient ensemble) ;
  - uniquement des brushes du monde : une page pour les lier à une entité (voir plus bas) ;
  - rien de sélectionné : le worldspawn, c'est-à-dire les propriétés de la map.
- Chaque modification est une étape d'annulation. Les modifications continues (glisser un nombre, une couleur) sont fusionnées en une étape par clé.

## L'en-tête du panneau

| Élément | Rôle |
|---|---|
| Icône du type | Entité ponctuelle, entité brush, sélection mixte ou monde. Sa couleur est celle de la classe dans le FGD. |
| Bouton de classe | La classe de la sélection (ou « N classes »). Cliquez pour choisir une autre classe dans une liste avec recherche (classes ponctuelles pour les entités ponctuelles, classes brush pour les entités brush). Changer de classe ajoute les clés par défaut manquantes de la nouvelle classe, sur toutes les entités sélectionnées. Une entité de brush qui prend une classe à clé `origin` (`func_tracktrain`, `func_tanktrain`, `func_rotating`...) sans en avoir reçoit celle du centre de ses brushes, comme dans Hammer. |
| **?** | Ouvre l'aide de l'entité pour la classe. |
| **☰** (**Plus**) | Menu avec **Copier les keyvalues**, **Coller les keyvalues** (avec la classe copiée et son nombre de clés), la case **Coller aussi le nom (targetname)**, **Rechercher et remplacer dans les entités...**, **Définir la distance maximale...** et **Aide de l'entité**. |
| **Entités sélectionnées : N** | Affiché quand plusieurs entités sont modifiées. |
| **Renvoyer au monde** | Pour les entités brush : rend les brushes au monde et supprime les entités brush (`Ctrl+Maj+W`). |
| **Description** | Section repliée avec la description FGD de la classe. |

Des avertissements apparaissent sous l'en-tête quand aucun FGD n'est chargé (**Aucune donnée de jeu (FGD) chargée : les propriétés sont affichées en texte brut**) ou quand une classe n'est pas définie dans le FGD (ses clés sont alors en texte brut).

Pour un `fog_volume`, l'en-tête indique quel brouillard il reçoit et ce qui ne va pas, en orange : **Pas de Fog Name**, **Aucun env_fog_controller nommé "x"**, **Le brouillard de "x" est désactivé (Fog Enable : No) : en jeu ce volume enlève le brouillard**, **Aperçu : brouillard par défaut**, **Start Disabled**, et l'état de **PostProcessName** et **Color Correction Name** (trouvé, absent, ou une entité d'une autre classe). Pour un `env_fog_controller`, il signale quand **Fog Enable** vaut No, si la vue 3D affiche ce brouillard et combien de `fog_volume` l'utilisent. Voir [Volumes de brouillard](render-modes-display.md#volumes-de-brouillard).

### Uniquement des brushes du monde

Le panneau affiche **Brushes du monde sélectionnés : N** et propose de leur donner une classe : choisissez une classe d'entité brush (`func_detail` par défaut, ou l'entité brush par défaut des paramètres) puis cliquez **Lier à une entité** (`Ctrl+T`). Les boutons **Liaison rapide :** créent un `func_detail`, `func_brush`, `func_illusionary`, `trigger_multiple` ou `trigger_once` en un clic (seulement les classes présentes dans votre FGD). Voir [Opérations sur les brushes](brush-operations.md).

## Onglets

| Onglet | Contenu | Affiché |
|---|---|---|
| **Propriétés** | SmartEdit : un widget typé par clé. | Toujours |
| **Indicateurs** | Cases des spawnflags. | Entités |
| **Sorties (N)** | Les sorties des entités sélectionnées, avec un éditeur. Mis en évidence quand une sortie pose problème. | Entités |
| **Entrées (N)** | Les sorties des autres entités qui ciblent la sélection. | Entités |
| **Modèle** | Skin, groupes de corps et animation par défaut d'un modèle studio, avec aperçu. | Quand la clé `model` commune est un fichier `.mdl` |
| **Visgroups** | Appartenance de la sélection aux visgroups. | Entités |
| **Brut** | Toutes les clés et valeurs dans un tableau simple. | Toujours |

Les onglets **Sorties** et **Entrées** sont décrits dans [I/O et vue logique](io-logical-view.md).

En bas des onglets d'entité, la section **Couleur d'affichage** règle la couleur de la sélection dans les vues 2D (voir [Couleurs d'affichage](display-colors.md)), et la section repliée **Commentaires de l'éditeur** contient **Commentaires** (notes enregistrées dans la map). Une sélection de brushes du monde montre la section **Couleur d'affichage** sous **Lier à une entité**.

## Onglet Propriétés (SmartEdit)

Au-dessus de la liste :

- **Clés** bascule entre les libellés du FGD et les vrais noms de clés.
- **Filtrer les propriétés** garde les lignes dont la clé ou le libellé contient le texte.

Chaque ligne a un libellé et un widget. Un `*` devant le libellé signale une valeur différente de celle par défaut du FGD. Les clés que le FGD ne définit pas sont en italique. Avec plusieurs entités sélectionnées, une clé qui diffère affiche **(valeurs multiples)** ; les clés FGD affichées sont celles communes à toutes les classes sélectionnées.

Survolez un libellé pour voir le libellé, la clé, le type, la description, la valeur par défaut et, pour les clés de distance, une note sur l'effet de la distance maximale en jeu. Clic droit sur un libellé :

- **Rétablir la valeur par défaut** (la valeur du FGD, ou suppression de la clé s'il n'y en a pas) ;
- **Copier la valeur**, **Copier le nom de la clé** ;
- **Supprimer la clé** ;
- sur la clé du nom : **Renommer met à jour les sorties** (activé par défaut). Quand le nom change, les sorties des autres entités qui ciblent l'ancien nom suivent, seulement si aucune autre entité ne garde ce nom.

Les champs texte s'appliquent avec `Entrée` ou quand ils perdent le focus ; `Échap` annule la saisie. Les guillemets doubles sont refusés dans les valeurs (le fichier de la map deviendrait illisible).

### Widgets selon le type de clé

| Type de clé | Widget |
|---|---|
| Booléen | Case à cocher (état mixte si les entités diffèrent). |
| Choix | Liste déroulante des choix du FGD. |
| Entier, flottant | Champ texte, rouge avec une infobulle si le nombre est invalide (la valeur n'est alors pas appliquée). |
| Couleur (`color255`) | Bouton de couleur, champ **Luminosité** pour les valeurs de lumière (`r g b luminosité`), et le texte. Une valeur de lumière à `-1` (HDR non défini) indique que la valeur normale est utilisée. |
| Couleur (`color1`) | Bouton de couleur et texte. |
| Modèle studio | Texte, bouton parcourir qui ouvre l'onglet **Modèles** en mode « choix pour cette clé », et menu **Modèles récemment utilisés**. Un modèle absent du contenu monté reçoit une icône d'avertissement. Voir [Navigateur de modèles](model-browser.md). |
| Matériau, décal, sprite | Vignette (agrandie au survol), texte, bouton **Utiliser le matériau actuel** et bouton parcourir qui ouvre le navigateur de textures en mode choix (double-cliquez sur un matériau). Les sprites reçoivent l'extension `.vmt`. |
| Son | Texte, liste des sons du jeu (soundscripts) puis des fichiers sonores (avec boutons d'écoute), bouton écouter/arrêter et bouton parcourir vers le navigateur de sons. Seuls les fichiers WAV, MP3 et OGG s'écoutent. Voir [Navigateur de sons](sound-browser.md). |
| Système de particules | Texte, liste des systèmes de particules du contenu et bouton parcourir vers le navigateur de particules. Voir [Particules](particles.md). |
| Scène, script | Texte et liste des fichiers `scenes/*.vcd` ou `scripts/vscripts` du contenu. |
| Liste de scripts | Texte et bouton **+** qui ajoute un script à la liste. |
| Liste de faces (`sidelist`) | Texte (l'infobulle compte les faces) et pipette de faces. |
| Nom de cette entité | Texte avec suggestions des noms de la map. |
| Nom de cible, filtre | Texte avec complétion des noms, rouge si aucune entité ne porte ce nom, pipette d'entité et bouton **Marquer**. Les clés de filtre ont aussi une liste des filtres de la map. |
| Angles | Champs tangage, lacet et roulis, cadran de lacet (glissez, `Maj` pour des pas de 15 degrés) et pipette de visée. |
| Tangage (`light_spot`) | Un seul nombre de -90 (vers le bas) à 90 (vers le haut), et pipette de visée. |
| Origine, vecteur | Champs X, Y et Z. |
| Axe | Deux points, X Y Z chacun. |
| Classe de PNJ, classe ponctuelle | Texte et liste des classes. |
| Fichier d'instance | Texte et bouton parcourir (boîte de dialogue VMF ; le chemin est enregistré relatif au dossier de la map si possible). |
| `skyname` (worldspawn) | Texte, liste des ciels trouvés dans le contenu, et aperçu des six faces au survol avec **Faces HDR trouvées** ou **Pas de faces HDR**. |

Pour un seul `func_instance`, ses paramètres s'affichent en champs typés sous la liste, avec **Modifier l'instance** et **Fusionner les instances sélectionnées** : voir [Instances](instances.md).

Complétion des noms : dans les champs de cible, `Tab` complète avec le premier nom, les flèches en choisissent un, `Entrée` valide ; `Entrée` seule garde ce que vous avez tapé.

### Pipettes et Marquer

| Bouton | Utilisation |
|---|---|
| Pipette d'entité (clés de cible) | Cliquez une entité dans les vues : son nom est écrit dans la clé. Si elle n'a pas de nom, une invite propose un nom libre (`classe_1`, `classe_2`...), la nomme et règle la clé en une seule étape. `Échap` annule ; recliquer le bouton annule aussi. |
| Pipette de faces (clés `sidelist`) | Cliquez des faces dans la vue 3D (`Ctrl+clic` pour plusieurs, `Entrée` pour finir). `Ctrl+clic` sur le bouton ajoute les faces à la liste au lieu de la remplacer. |
| Pipette de visée (angles) | Cliquez un point dans les vues : chaque entité sélectionnée se tourne vers lui (le roulis est conservé). |
| **Marquer** | Sélectionne les entités désignées par la valeur (noms, jokers `*`, noms de classe) et centre les vues dessus. L'infobulle donne leur nombre. |

## Onglet Indicateurs

Un groupe par clé d'indicateurs (en général `spawnflags`) : la valeur brute (somme des bits cochés, modifiable) et une case par bit. Survolez une case pour voir la valeur du bit et s'il est activé par défaut. Les bits inconnus du FGD s'affichent en **Indicateur inconnu N**. Avec plusieurs entités, un bit activé sur une partie seulement affiche une case mixte.

## Onglet Brut

Un tableau de toutes les clés et valeurs. `classname` est en lecture seule (utilisez le bouton de classe). Modifiez un nom de clé pour la renommer, une valeur pour la changer, **×** supprime la clé. La dernière ligne ajoute une clé : tapez le nom et la valeur, puis **+** ou `Entrée`. `id` et `classname` ne peuvent pas être ajoutés ici.

## Onglet Modèle

Affiché quand les entités sélectionnées partagent un modèle `.mdl`.

- Le nom et le chemin du modèle, un aperçu (glissez pour tourner autour), le nombre de triangles et de sommets et une section **Statistiques**.
- **Affichage** : **Filaire**, aides superposées (boîte englobante, box de mouvement, hitboxes, os, modèle physique, attaches, normales, axes d'origine, sol, position d'éclairage, boîte de séquence) et **Champ de vision**. Les aides que le modèle n'a pas sont désactivées.
- **Skins** : règle la clé `skin`.
- Une liste par groupe de corps ayant au moins deux choix : règle la clé `body`.
- **Séquences (N)** : le bouton lecture joue une séquence dans l'aperçu (**Arrêter l'animation**, **Redémarrer l'animation**). Cliquer le nom d'une séquence règle `DefaultAnim` si la classe a cette clé. Un modèle nouvellement affiché joue son `DefaultAnim` (sinon sa première séquence).

## Onglet Visgroups

Une case par visgroup de la map, indentée comme l'arbre des visgroups, avec sa couleur. Une case est mixte quand seule une partie des entités sélectionnées appartient au visgroup. Un clic ajoute ou retire toutes les entités sélectionnées. Voir [Visgroups](visgroups.md).

## Éditeur de couleur

**Édition > Modifier la couleur...** (`C`) ouvre la fenêtre **Couleur : clé** sur la première clé de couleur de la première entité sélectionnée (la première clé `color255` de sa classe, sinon `_light`, `rendercolor`, `_ambient` ou `color`). S'il n'y en a pas, un message indique **La sélection n'a pas de clé de couleur**.

- Un sélecteur de couleur, des champs **R G B** (0 à 255) et **H S V**.
- Curseur **Luminosité** (0 à 2000, logarithmique) pour les valeurs de lumière.
- **Palette de la map** : 16 couleurs enregistrées dans la map (bloc `palette_plus` de Hammer++). Clic sur une case pour l'utiliser, clic droit pour y ranger la couleur actuelle.

Les changements s'appliquent en direct à toutes les entités sélectionnées et se fusionnent en une étape d'annulation par clé.

## Copier et coller les keyvalues

- **Édition > Copier les keyvalues** copie toutes les clés de la première entité sélectionnée sauf `id`, `classname`, `origin` et `targetname` (gardé à part). Les clés partent aussi dans le presse-papiers du système sous forme de lignes `"clé" "valeur"`.
- **Édition > Coller les keyvalues** les écrit sur toutes les entités sélectionnées en une étape. La classe ne change pas.
- **Coller aussi le nom (targetname)** (menu du panneau) ou **Paramètres > Entités et navigateurs > Coller les keyvalues colle aussi le nom (targetname)** colle aussi le nom. Désactivé par défaut (`[entities] paste_targetname`).

## Définir la distance maximale

**Édition > Définir la distance maximale...** ouvre la fenêtre **Distance maximale** : une distance (2048 unités au départ, jusqu'à 1 000 000 ; le bouton **0** veut dire sans limite) écrite dans la clé que chaque classe respecte en jeu.

| Classes | Clé écrite |
|---|---|
| `light`, `light_spot` | `_distance` : la lumière s'arrête là (les modèles en jeu ; le compilateur de Hammer Pro fait aussi disparaître les lightmaps sur le dernier quart). |
| `light_dynamic`, `point_spotlight` | `distance`, `SpotlightLength` : seulement raccourcies, jamais allongées. |
| Props, items, armes et autres classes avec des clés de fondu, `info_overlay`, `env_detail_controller` | `fademaxdist`, avec un début `fademindist` pour un fondu au lieu d'une disparition sèche. Pour `env_detail_controller`, cela vaut pour tous les props de détail de la map. |
| `env_lightglow` | `OuterMaxDist`. |
| `func_lod` | `DisappearMaxDist` et `DisappearMinDist`. |

Les entités logiques, les triggers et les autres classes que le jeu ne peut pas limiter sont ignorés. La fenêtre indique combien d'entités seront modifiées, sont déjà réglées et sont ignorées ; **Appliquer** n'est actif que si quelque chose change. Le résultat s'affiche dans une notification.

## Aide de l'entité

**Aide > Aide de l'entité**, ou **?** dans l'en-tête du panneau, ouvre **Aide : classe** pour la première entité sélectionnée (sans sélection, la classe de l'outil entité). Elle montre le type et la description de la classe, un lien **Page du Valve Developer Community**, un filtre et les sections **Clés (N)** (libellé, clé, type, défaut, description), **Indicateurs (N)**, **Entrées (N)** et **Sorties (N)**. `Échap` la ferme.

## Rechercher des entités

**Édition > Rechercher des entités...** (`Ctrl+Maj+F`) ouvre **Rechercher des entités**.

| Option | Rôle |
|---|---|
| Mode | **Nom ou classe** (un texte avec `=` cherche `clé=valeur`), **Nom**, **Classe**, **Clé=valeur**. |
| Champ de recherche | Texte avec jokers `*` et `?`. Sans joker, le texte peut apparaître n'importe où. |
| **Visibles uniquement** | Écarte les entités cachées. |
| **Correspondance exacte** | Désactivé : le texte peut apparaître n'importe où dans le nom. |
| **Sélectionner tous les résultats** | Sélectionne tous les résultats. |

Les résultats donnent la classe, le nom et l'origine. Double-cliquez une ligne (ou `Entrée` dans le champ) pour sélectionner l'entité et centrer les vues dessus. `Échap` ferme la fenêtre.

## Rechercher et remplacer dans les entités

**Édition > Rechercher et remplacer dans les entités...** (`Ctrl+Alt+R`, `Ctrl+Maj+R` avec les préréglages de touches Hammer et Hammer++) ouvre **Rechercher et remplacer dans les entités**. La recherche porte sur les valeurs des clés, les noms de classe et les sorties (sortie, cible, entrée, paramètre).

| Option | Par défaut |
|---|---|
| **Rechercher**, **Remplacer par** | vide |
| **Mots entiers uniquement** | désactivé |
| **Respecter la casse** | désactivé |
| **Noms des clés aussi** (chercher et renommer aussi les noms de clés) | désactivé |
| **Toute la map** / **Sélection** | Toute la map |
| **Visibles uniquement** | désactivé |

La ligne d'état compte les occurrences, les champs et les entités. **Suivant** (ou `Entrée` dans le champ de recherche) passe à l'occurrence suivante et sélectionne son entité ; **Remplacer** remplace dans le champ courant puis avance ; **Tout remplacer** remplace tout en une étape d'annulation. Double-cliquez un résultat pour sélectionner son entité. Les guillemets doubles sont refusés dans le remplacement.

## Rapport des entités

**Map > Rapport des entités** liste toutes les entités : **Classe**, **Nom**, **Détails** (modèle, matériau, fichier d'instance ou système de particules), **Origine**, **Type**, **Visible**. Cliquez un en-tête de colonne pour trier.

- Filtre : des mots cherchés dans la classe, le nom et les détails ; un mot `clé=valeur` cherche une clé (`*` et `?` acceptés). Tous les mots doivent correspondre.
- Filtre de type (**Tout**, **Point**, **Brush**) et **Masquer les invisibles**.
- Clic, `Ctrl+clic` et `Maj+clic` sélectionnent des lignes ; **Sélectionner**, **Aller à** (sélectionner et centrer les vues) et **Supprimer** agissent dessus. Double-cliquez une ligne pour y aller.

Les entités brush sont placées au centre de leurs brushes. Sur les maps de plus de 3000 entités, les listes se rafraîchissent quelques fois par seconde pendant que vous modifiez.

## Propriétés de la map

**Map > Propriétés de la map...** ouvre **Propriétés de la map** : le worldspawn avec les onglets **Propriétés** et **Brut** (le même éditeur que le panneau, qui l'affiche aussi quand rien n'est sélectionné). En plus des clés du FGD, il affiche toujours les clés du compilateur d'éclairage de Hammer Pro :

| Clé | Valeurs |
|---|---|
| **Fichier de lumières (lights.rad)** (`_lightsrad`) | Un fichier de lumières de textures pour cette map. |

Voir [Clés du worldspawn](../compiler/worldspawn-keys.md).

## Options

| Option | Où | Par défaut |
|---|---|---|
| Coller les keyvalues colle aussi le nom | **Paramètres > Entités et navigateurs**, `[entities] paste_targetname` | désactivé |
| Les nouvelles sorties utilisent le séparateur ESC | **Paramètres > Entités et navigateurs**, `[entities] io_esc_delimiter` | activé |
| Volume de l'écoute des sons | **Paramètres > Entités et navigateurs**, `[browser] sound_volume` | 80 % |
| Renommer met à jour les sorties | clic droit sur le libellé du nom | activé |

## Astuces

- Quand un fichier FGD change sur le disque, Hammer Pro recharge les définitions d'entités en quelques secondes (une ligne du journal le signale).
- Utilisez **Marquer** sur une clé de cible pour voir d'un coup quelles entités elle atteint.
- Tout ce qui est décrit ici passe aussi par MCP : `get_entity`, `set_entity_keyvalues`, `set_keyvalue`, `set_entity_class`, `set_spawnflags`, `copy_keyvalues`, `set_max_distance`, `find_entities`, `find_text`, `replace_text`, `entity_report`, `get_entity_class`, `get_map_properties`, `set_map_properties`, `set_visgroup_members`, et `run_action` pour les fenêtres. Voir la [référence des outils MCP](../../MCP.md) (en anglais).

## FAQ

**Pourquoi mes clés sont-elles en texte brut ?** Aucun FGD n'est chargé, ou la classe n'y figure pas. Vérifiez les données du jeu dans **Paramètres > Jeu** (voir [Configuration du jeu](../getting-started/game-setup.md)).

**J'ai sélectionné un prop et une lumière : où sont passées les autres clés ?** Avec plusieurs classes, le panneau n'affiche que les clés communes à toutes. Sélectionnez une seule classe pour voir toutes ses clés.

**Coller les keyvalues n'a pas changé la classe.** C'est normal : changez la classe avec le bouton de classe, puis collez.

**La distance maximale a ignoré des entités.** Leurs classes n'ont pas de distance respectée par le jeu (entités logiques, triggers, entités brush simples, sprites, décals).

**Comment supprimer une clé ?** Clic droit sur son libellé puis **Supprimer la clé**, ou **×** dans l'onglet **Brut**.

## Pages liées

- [Outil entité](entity-tool.md)
- [I/O et vue logique](io-logical-view.md)
- [Navigateur de modèles](model-browser.md)
- [Navigateur de sons](sound-browser.md)
- [Navigateur de textures](texture-browser.md)
- [Instances](instances.md)
- [Visgroups](visgroups.md)
- [Presse-papiers](clipboard.md)
- [Soundscapes](soundscapes.md)
- [Référence des paramètres](../reference/settings.md)

---
[Retour à l'index de la documentation](../README.md)
