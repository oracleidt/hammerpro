[Documentation](../README.md) > Fonctionnalités > I/O et vue logique | [English](../../en/features/io-logical-view.md) | [Русский](../../ru/features/io-logical-view.md)

# I/O et vue logique

Les sorties et les entrées relient les entités entre elles : « quand ceci se déclenche, envoyer telle entrée à telles entités ». Hammer Pro les modifie dans les onglets **Sorties** et **Entrées** du panneau d'entité (l'éditeur d'I/O de Hammer), les dessine en lignes dans les vues, et montre toute la logique de la map sous forme de schéma dans la vue logique.

## Ouverture

| Quoi | Comment |
|---|---|
| Onglets sorties et entrées | **Édition > Propriétés** (`Alt+Enter`), puis les onglets **Sorties (N)** et **Entrées (N)** de l'onglet **Entité**. |
| Lignes de connexion dans les vues | **Affichage > Options d'affichage > Afficher les connexions** fait tourner le mode ; **Paramètres > Aides > Connexions entrées/sorties** le règle. |
| Fenêtre de la vue logique | **Affichage > Type de vue > Vue logique** (l'ouvre et la ferme). |
| Vue logique 2D dans un quart d'écran | **Affichage > Type de vue > Vue logique 2D** l'applique à la vue sous la souris (sinon la dernière utilisée), ou choisissez **Logique 2D** dans le menu du type de vue, dans l'en-tête d'une vue. |
| Galerie d'entités | **Map > Insérer la galerie d'entités**. |

Aucune de ces commandes n'a de raccourci par défaut ; vous pouvez en attribuer dans **Paramètres > Clavier** (voir [Raccourcis clavier](keyboard-shortcuts.md)).

## Onglet Sorties

L'onglet liste les sorties des entités sélectionnées. Avec plusieurs entités sélectionnées, seules les sorties qu'elles ont toutes en commun sont listées (les modifications s'appliquent à la sortie correspondante de chaque entité), et une note indique le nombre de **Sorties masquées (non communes à toutes les entités sélectionnées)**. Le monde n'a pas de sorties ici.

### Barre d'outils

| Bouton | Touche | Rôle |
|---|---|---|
| **+ Ajouter** | `Inser` | Ajoute une sortie à chaque entité sélectionnée et la sélectionne. La nouvelle sortie est `OnTrigger` si la classe l'a, sinon la première sortie de la classe, sinon `OnUser1` ; elle se déclenche toujours. |
| **Copier** | `Ctrl+C` | Copie les sorties sélectionnées (aussi dans le presse-papiers du système, en lignes `"Sortie" "valeur"`). |
| **Coller** | `Ctrl+V` | Colle les sorties copiées sur chaque entité sélectionnée. |
| **Supprimer** | `Suppr` | Supprime les sorties sélectionnées. |
| (couper) | `Ctrl+X` | Copie puis supprime les sorties sélectionnées. |
| (tout sélectionner) | `Ctrl+A` | Sélectionne toutes les lignes. |
| **Marquer** | | Sélectionne les entités ciblées par les sorties sélectionnées (noms, jokers `*`, noms de classe) et centre les vues dessus. |

Ces touches sont vos raccourcis de l'application pour Copier, Couper, Coller, Supprimer et Tout sélectionner. Elles agissent sur les sorties tant que le pointeur de la souris est au-dessus de l'onglet et qu'aucun champ texte n'a le focus ; ailleurs, elles agissent sur la map comme d'habitude.

À droite, **Invalides : N** compte les sorties qui posent problème.

### Le tableau

Colonnes : une colonne d'état (**!** pour un problème, sinon le nombre de déclenchements d'une sortie limitée), **Sortie**, **Cible**, **Entrée**, **Paramètre**, **Délai**. Un problème s'affiche en rouge dans sa cellule :

- **Cette sortie n'existe pas pour cette classe** ;
- **Aucune entité ne porte ce nom de cible** ;
- **La cible n'accepte pas cette entrée**.

Survolez une ligne pour voir la connexion complète, son paramètre, son délai, ses déclenchements et ses problèmes. Un clic sélectionne une ligne, `Ctrl+clic` en ajoute ou retire une, `Maj+clic` sélectionne une plage.

### Modifier les sorties sélectionnées

Sous le tableau, l'éditeur modifie toutes les sorties sélectionnées d'un coup (**(valeurs multiples)** quand elles diffèrent). **Sélectionnez une sortie pour la modifier (Ctrl et Maj pour en sélectionner plusieurs).**

| Champ | Rôle |
|---|---|
| **Ma sortie nommée** | Liste des sorties de la classe (toutes les sorties si la classe est inconnue). |
| **Entités cibles nommées** | Texte avec complétion des noms de la map et des cibles spéciales (`!self`, `!activator`, `!caller`, `!player`, `!pvsplayer`, `!speechtarget`, `!picker`). En rouge si aucune entité ne porte ce nom. `Tab` complète, les flèches choisissent, `Entrée` valide. |
| **Via cette entrée** | Liste des entrées des classes visées par la cible (toutes les entrées si la cible ne correspond à rien de connu). |
| **Avec le paramètre** | Texte libre, **<aucun>** s'il est vide. Les guillemets doubles sont refusés ; si l'entrée est `RunScriptCode`, **Utiliser des accents graves** les remplace par des accents graves. |
| **Après un délai en secondes de** | 0 à 3600 secondes. Glissez ou tapez. |
| **Nombre de déclenchements** | **Toujours** (-1) ou un nombre (les refires de Hammer++). Glissez ou tapez un nombre ; 0 est sauté, puisqu'il ne se déclencherait jamais. |

## Onglet Entrées

Il liste les sorties des autres entités (et du monde) qui ciblent les entités sélectionnées : **Sorties ciblant cette entité : N**, avec les colonnes **Source**, **Sortie**, **Entrée**, **Paramètre**, **Délai**. Une entité sans nom ne peut pas être ciblée : l'onglet l'indique.

Double-cliquez une ligne, ou sélectionnez-la et cliquez **Aller à la source**, pour sélectionner l'entité source et centrer les vues dessus.

## Séparateur ESC

Dans le fichier de la map, les champs d'une sortie sont séparés par des virgules (ancien Hammer) ou par le caractère ESC (versions récentes de Hammer et Hammer++). Avec ESC, un paramètre peut contenir des virgules. Garry's Mod lit les deux.

**Paramètres > Entités et navigateurs > Les nouvelles sorties utilisent le séparateur ESC** (`[entities] io_esc_delimiter`, activé par défaut) choisit le séparateur des sorties ajoutées depuis l'onglet **Sorties**. Les sorties existantes gardent le leur. Les sorties ajoutées depuis la vue logique utilisent des virgules.

## Lignes de connexion dans les vues

Chaque sortie peut être dessinée en ligne de son entité vers chaque cible, avec une flèche côté cible, dans les vues 2D et 3D.

| Mode | Libellé dans les paramètres | Ce qui est dessiné |
|---|---|---|
| Désactivé | **Masquées** | Rien. |
| Sélection (par défaut) | **Entités sélectionnées** | Les sorties des entités sélectionnées et les sorties qui les ciblent. |
| Tout | **Toutes les entités** | Toutes les sorties de la map. |

**Affichage > Options d'affichage > Afficher les connexions** passe de Masquées à Sélection puis Tout, et une notification annonce le nouveau mode. Le mode est enregistré dans `[helpers] connections`.

Couleurs (la légende est dans **Paramètres > Aides**) :

- vert : **Valide** ;
- rouge : **Cible introuvable**, dessinée en croix sur la source ;
- jaune : **Sortie inconnue du FGD**.

Les cibles spéciales (`!activator`, `!player`...) ne sont résolues qu'en jeu : elles n'ont pas de ligne.

## Vue logique

La vue logique montre les entités de la map et leurs sorties sous forme de schéma. Elle existe en fenêtre flottante (**Affichage > Type de vue > Vue logique**, titre **Vue logique**) et comme type de vue des quatre vues (**Logique 2D**, comme dans Hammer). Chaque quart d'écran qui l'affiche a son propre déplacement, zoom, recherche et filtre. La sélection est celle de la map : elle suit les autres vues dans les deux sens.

### Ce qui est affiché

- Les nœuds : par défaut toutes les entités de la map, comme dans Hammer : entités ponctuelles (`light`, `light_spot`, `light_environment`, entités `info_`, logique...) et entités de brush, nommées ou non. Un nœud affiche l'icône d'éditeur de sa classe (l'aide `iconsprite()` ou `sprite()` du FGD, l'icône des vues 2D et 3D), le nom (**(sans nom)** s'il n'y en a pas) et la classe, avec une couleur selon la famille de classe (logique et filtres, triggers, `func_`, lumières et environnement, props et PNJ, info et chemins, autres). Les entités cachées par un visgroup sont dessinées estompées.
- Les liens : un par sortie, colorés comme les lignes de connexion (vert valide, rouge cible introuvable avec une croix et le nom, jaune sortie ou entrée inconnue du FGD). Une sortie vers sa propre entité fait une boucle au-dessus du nœud.
- Survolez un lien pour lire `Sortie > cible.Entrée (paramètre) après N s (une fois)` et, en cas de problème, la raison.
- En dézoomant, le texte et les ports disparaissent et les liens deviennent droits : les grosses maps restent fluides.

### Barre d'outils

| Contrôle | Rôle |
|---|---|
| **Chercher une entité...** | N'affiche que les nœuds dont le nom ou la classe contient le texte, et les cadre pendant la frappe. `Entrée` cadre les résultats. |
| Filtre | **Toutes les entités** (par défaut), **Nommées et connectées** (entités avec un nom ou une connexion) ou **Connectées seulement** (entités avec une connexion sortante ou entrante). Chaque vue garde son filtre. |
| **Tout cadrer** | Affiche toutes les entités (`Origine` avec la souris sur la vue). |
| **Cadrer la sélection** | Cadre les résultats de la recherche, sinon les entités sélectionnées. |
| **Organiser** | Place toutes les entités automatiquement et enregistre les positions dans la map (une étape d'annulation, « Organisation de la vue logique »). |

La ligne d'état donne **Entités : N, sorties : N**, le nombre de résultats de recherche, **Sorties à problème : N**, et la légende des couleurs.

### Souris

| Action | Résultat |
|---|---|
| Clic sur un nœud | Le sélectionne (`Ctrl` ajoute ou retire, `Maj` ajoute). |
| Clic sur un lien | Sélectionne son entité source. |
| Clic dans le vide | Vide la sélection. |
| Glisser dans le vide | Sélection par rectangle (`Ctrl` ou `Maj` ajoute). |
| Glisser un nœud | Déplace les nœuds sélectionnés ; une étape d'annulation (« Déplacement dans la vue logique ») au relâchement. |
| Glisser depuis le port de sortie d'un nœud vers un autre nœud | Ouvre le formulaire **Nouvelle sortie**. |
| Double-clic sur un nœud | Le sélectionne et ouvre ses propriétés. |
| Glisser avec le bouton du milieu ou le clic droit | Déplace la vue. |
| Molette | Zoome autour du curseur. |
| `Échap` | Annule le glissement en cours. |

Les entités cachées ne sont jamais sélectionnées depuis la vue logique.

Menu du clic droit : **Propriétés**, **Centrer les vues dessus**, et **Utiliser la position automatique** (pour les nœuds sélectionnés qui ont une position enregistrée).

### Positions

Les positions sont enregistrées dans la clé `logicalpos` de chaque entité, comme dans Hammer : elles sont donc conservées dans le VMF. Les nœuds sans position enregistrée sont placés par une disposition automatique en couches (sources à gauche, chaque sortie une colonne plus à droite), recalculée seulement quand les connexions changent. Déplacer un nœud enregistre sa position ; **Utiliser la position automatique** la retire.

### Formulaire Nouvelle sortie

Glisser depuis un port de sortie vers un autre nœud ouvre **Nouvelle sortie** :

| Champ | Contenu |
|---|---|
| **Sortie** | Une sortie de la classe source, tapée ou choisie. La première proposée est une sortie propre à la classe, pas une des bases communes (`OnUser1`...) ; `OnTrigger` si la classe est inconnue. |
| **Cible** | Le nom de la cible. Si elle n'a pas de nom, un champ propose `classe_id` : **La cible n'a pas de nom : elle recevra celui-ci**. |
| **Entrée** | Une entrée de la classe cible, tapée ou choisie. |
| **Paramètre** | Texte libre. |
| **Délai** | 0 à 3600 secondes. |
| **Une seule fois** | Se déclenche une fois au lieu de toujours. |

**Ajouter** (actif quand la sortie, l'entrée et le nom de la cible sont remplis) ajoute la sortie, en nommant d'abord la cible si besoin, en une étape d'annulation. **Annuler** ou `Échap` ferme le formulaire.

## Galerie d'entités

**Map > Insérer la galerie d'entités** place une entité de chaque classe ponctuelle du FGD sur une grille au centre de la vue de dessus, chacune avec les keyvalues par défaut de sa classe, en une étape d'annulation. L'espacement suit la plus grande taille d'entité (128 à 512 unités). Servez-vous-en pour vérifier d'un coup d'œil icônes, modèles et aides, puis annulez. Sans FGD, un message indique **Aucune définition d'entité chargée : la galerie a besoin du FGD**.

## Astuces

- Le titre de l'onglet Sorties est mis en évidence quand une sortie pose problème : cherchez les cellules rouges.
- Lancez **Organiser** une fois sur une map importée, puis déplacez les nœuds qui vous intéressent : les autres suivent la disposition automatique.
- Par MCP : `list_connections`, `add_connection`, `edit_connection`, `remove_connections`, `get_io_graph`, `list_targetnames`, `rename_targetname`, `check_entity`, `get_logical_graph` (les nœuds, icônes et liens de la vue logique, avec son filtre), et `get_views` (un quart de type `logical`). Voir la [référence des outils MCP](../../MCP.md) (en anglais).

## FAQ

**Pourquoi l'onglet Sorties montre-t-il moins de sorties que prévu ?** Plusieurs entités sont sélectionnées et seules les sorties communes sont listées. Sélectionnez une seule entité pour voir toutes ses sorties.

**Pourquoi ma sortie est-elle rouge alors que la cible existe ?** Regardez quelle cellule est rouge : la sortie n'existe peut-être pas pour la classe source, ou l'entrée pour la classe cible, dans votre FGD.

**Les positions de la vue logique changent-elles ma map en jeu ?** Non. `logicalpos` est une clé de l'éditeur, comme dans Hammer.

**Mes lignes de connexion ont disparu.** **Afficher les connexions** passe par trois modes : relancez-le jusqu'à ce que la notification annonce le mode voulu.

**Faut-il garder le séparateur ESC ?** Oui pour Garry's Mod : il lit les deux, et ESC permet des virgules dans les paramètres. Ne le désactivez que pour un outil qui ne lit pas ESC.

## Pages liées

- [Propriétés des entités](entity-properties.md)
- [Outil chemin](path-tool.md)
- [Vue joueur](player-view.md)
- [Modes de rendu et options d'affichage](render-modes-display.md)
- [Vérifier les problèmes](check-problems.md)
- [Caméra et navigation](camera-navigation.md)
- [Référence des paramètres](../reference/settings.md)
- [Serveur MCP](../reference/mcp-server.md)

---
[Retour à l'index de la documentation](../README.md)
