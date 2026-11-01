[Documentation](../README.md) > Fonctionnalités > Construire et enregistrer en contenu | [English](../../en/features/build-content-release.md)

# Construire et enregistrer en contenu

**Construire et enregistrer en contenu** transforme une map Garry's Mod en publication finie dans une seule fenêtre : une compilation Finale, les cubemaps et le maillage de navigation construits en jeu si vous le voulez, tous les fichiers nécessaires trouvés et empaquetés, un addon Workshop (`addon.json`, dossier de contenu et `.gma`), une miniature et une icône Workshop, une liste de vérifications, et la publication sur le Workshop avec le `gmpublish` du jeu.

C'est une fonction propre à Garry's Mod. Pour une map d'un autre jeu, la fenêtre le signale : compilez-la depuis la fenêtre de compilation (`F9`), la map compilée va dans le dossier maps de son jeu.

## Ouvrir la fenêtre

| Commande | Ouvre |
| --- | --- |
| **Fichier > Construire et enregistrer en contenu...** | La fenêtre de publication sur l'onglet **Construction**. |
| **Fichier > Publier sur le Workshop...** | La fenêtre de publication sur l'onglet **Publication**. |

La publication est construite à partir du fichier enregistré : si la map a des modifications non enregistrées, la fenêtre vous demande d'abord de l'enregistrer (**Enregistrer la carte...**). La fenêtre a cinq onglets : **Construction**, **Vérifications**, **Addon**, **Publication** et **Rapport**.

## Onglet Construction

| Contrôle | Effet | Défaut |
| --- | --- | --- |
| **Carte** | La map publiée. | |
| **Dossier de sortie** | Où la publication est écrite. **Par défaut** donne `<dossier de la map>/<map>_content` ; **Changer...** choisit un autre dossier ; **Ouvrir le dossier** l'ouvre. Mémorisé pour chaque map. | `<map>_content` |
| **Produire** | Le mode de publication (plus bas). | **Addon Workshop** |
| **Compilation finale d'abord (géométrie, visibilité, éclairage LDR et HDR)** | Compile la map avec le préréglage Final avant d'empaqueter. | activé |
| **Compresser le pakfile (LZMA)** | Fichier BSP plus petit. Garry's Mod le lit mais affiche un avertissement dans la console pour chaque fichier compressé chargé, et les téléchargements du Workshop et du FastDL sont déjà compressés : laissez désactivé sauf besoin d'un fichier plus petit. Les sons restent toujours non compressés. | désactivé |
| **Garder les cubemaps construites du BSP précédent** | Recopie les cubemaps déjà construites dans le nouveau BSP. | activé |
| **Écrire le .gma** | Écrit le fichier de l'addon. | activé |
| **Inclure le contenu des objets du Workshop** | Désactivé : les objets du Workshop sont listés comme objets requis (les joueurs s'y abonnent). | activé |
| **Inclure le contenu des jeux montés** | Contenu des jeux montés par `mount.cfg` (CS:S, épisodes de HL2...). Les joueurs qui ne les ont pas voient des erreurs ; le redistribuer est sous votre responsabilité. | désactivé |
| **Construire les cubemaps et la navigation en jeu** | Lance Garry's Mod, après confirmation, pour construire les cubemaps et générer le maillage de navigation, puis le ferme. | désactivé |
| **Ouvrir le dossier de sortie à la fin** | | désactivé |
| **Protect your BSP** | Livre une copie protégée et garde la map complète pour votre serveur. Voir [Protect your BSP](bsp-protection.md). | désactivé |
| **Construire** | Lance la publication. **Annuler** l'arrête. | |
| **Analyser** | Liste tous les fichiers nécessaires à la map, sans compiler ni rien écrire (remplit l'onglet Rapport). | |

Ces options sont enregistrées dans la table `[content]` de settings.toml (voir la [référence des paramètres](../reference/settings.md)).

### Modes de publication

| Mode | Produit |
| --- | --- |
| **Addon Workshop** | Le BSP, un dossier de contenu et un `.gma`. Les fichiers refusés par gmad (soundscapes, manifeste de particules) voyagent dans le pakfile du BSP. |
| **BSP autonome** | Un seul BSP contenant tous les fichiers nécessaires dans son pakfile. |
| **Kit serveur** | Un BSP autonome plus un dossier `fastdl` (le BSP et son `.nav`) et `lua/autorun/server/<map>_resources.lua`, qui ajoute l'objet du Workshop (`resource.AddWorkshop`, une fois publié) ou la map (`resource.AddFile`) aux téléchargements. |

### Ce que fait une construction

Les étapes s'affichent pendant l'opération : **Dépendances**, **Compilation**, **Session de jeu**, **Pakfile**, **Navigation**, **Dossier de contenu**, **GMA**, **Terminé**. Le journal en dessous liste ce qui s'est passé (fichiers empaquetés, cubemaps gardées, fichiers exclus du GMA, avertissements sur les jeux montés et les scripts sonores).

Sortie, où `<addon>` est le titre de l'addon en minuscules (le nom de la map si le titre est vide) :

```text
<sortie>/<addon>/addon.json
<sortie>/<addon>/maps/<map>.bsp, <map>.nav, thumb/<map>.png
<sortie>/<addon>/materials/..., models/..., sound/...   (addon Workshop)
<sortie>/<addon>.gma
<sortie>/<addon>_icon.jpg
<sortie>/fastdl/...                                     (kit serveur)
<sortie>/.hammerpro_release.toml                        (état incrémental)
```

Les reconstructions sont incrémentales : `.hammerpro_release.toml` note chaque fichier écrit par une publication, les fichiers inchangés sont sautés et le GMA n'est réécrit que si ses entrées ont changé. Seuls les fichiers écrits par une publication précédente peuvent être supprimés.

La publication ajoute aussi à l'addon le Lua des changements de skybox (`point_hammerpro_skybox`) quand la map en utilise, car le jeu n'exécute jamais le Lua du pakfile d'une map.

Une construction s'arrête avec un message clair si la map n'est pas enregistrée, si le jeu ou son contenu est introuvable, si la compilation échoue, si la map fuit, s'il manque des fichiers nécessaires (voir le rapport) ou si certains fichiers ne peuvent être chargés ni depuis une map ni depuis un addon.

### Session de jeu

Avec **Construire les cubemaps et la navigation en jeu**, la construction demande **Lancer Garry's Mod ?** (fermez d'abord le jeu s'il est lancé et ne l'utilisez pas avant la fin ; la map est copiée dans le dossier maps du jeu). Ensuite :

- **Cubemaps** : le jeu lance `-buildcubemaps` une fois par plage d'éclairage du BSP (LDR et HDR) et se ferme tout seul.
- **Navigation** : le jeu charge une copie de la map qui lance `nav_generate`, Hammer Pro attend que le maillage soit enregistré, puis ferme le jeu.

Vos réglages vidéo et `cfg/config.cfg`, que le jeu réécrit en quittant, sont sauvegardés avant la session et remis après, quoi qu'il arrive. Voir [Cubemaps](../compiler/cubemaps.md).

## Onglet Vérifications

Chaque ligne doit être verte avant de publier (**Actualiser** relit tout). Les lignes sont vertes, rouges, orange ou inconnues, avec un bouton de correction quand il y en a un :

| Ligne | Vérifie | Bouton |
| --- | --- | --- |
| **Nom de la carte** | Uniquement des minuscules, des chiffres, `_` et `-` ; aucun autre addon n'a une map de ce nom. | |
| **Compilation complète** | Enregistrée, compilée, à jour, avec visibilité. | **Construire** |
| **Éclairage final** | Éclairage LDR et HDR, pas fullbright. | **Construire** |
| **Aucune fuite** | La map est étanche. | **Construire** |
| **Cubemaps** | Cubemaps construites par rapport au nombre d'`env_cubemap`. | **Construire en jeu** |
| **Maillage de navigation** | Présent et à jour, absent, ou construit pour une autre géométrie. | **Construire en jeu** |
| **Dépendances** | Tous les fichiers trouvés, ou le nombre de manquants. | **Afficher** (onglet Rapport) |
| **Miniature de la carte** | `maps/thumb/<map>.png` existe. | **Capturer** |
| **Icône du Workshop** | L'icône existe (écrite à la prochaine construction à partir de la miniature). | **Capturer** |
| **addon.json** | Titre, type et étiquettes valides. | **Modifier** (onglet Addon) |
| **Limites du moteur** | La limite la plus haute et son pourcentage. | |
| **Mode de jeu** | Ce que promet le préfixe de la map (plus bas). | |

Profils de mode de jeu, choisis d'après le préfixe du nom de la map :

| Préfixe | Mode de jeu | Demande |
| --- | --- | --- |
| `gm_` | Sandbox | un point d'apparition (`info_player_start`, `info_player_deathmatch`, `info_player_combine` ou `info_player_rebel`) |
| `ttt_` | Trouble in Terrorist Town | `info_player_*`, et des armes ou munitions (`weapon_*`, `item_*`, `ttt_random_weapon`, `ttt_random_ammo`) |
| `ph_` | Prop Hunt | `info_player_terrorist` et `info_player_counterterrorist` |
| `rp_` | DarkRP | un point d'apparition |
| `zs_` | Zombie Survival | `info_player_*` et un maillage de navigation |
| `mu_` | Murder | `info_player_*` |
| `deathrun_` | Deathrun | `info_player_terrorist` et `info_player_counterterrorist` |
| `jb_` | Jailbreak | `info_player_terrorist` et `info_player_counterterrorist` |
| `bhop_` | Bunny Hop | `info_player_*` |
| `surf_` | Surf | `info_player_*` |

## Onglet Addon

| Champ | Remarques |
| --- | --- |
| **Titre** | ASCII simple, 128 caractères au plus. Vide : le nom de la map. |
| **Type** | Toujours `map`. |
| **Étiquettes (2 au plus)** | `fun`, `roleplay`, `scenic`, `movie`, `realism`, `cartoon`, `water`, `comic`, `build`. |
| **Ignorer** | Un motif gmad par ligne, par exemple `*.psd`. |
| **Description** | Texte de l'addon. |

**Miniature et icône du Workshop** : écrite en `maps/thumb/<map>.png` (256 x 256) et comme icône JPEG 512 x 512 du Workshop. **Capturer la vue 3D** utilise le carré central de la vue 3D (cadrez d'abord la map ; le cadre montre la zone sûre), **Choisir une image...** charge un fichier, **Effacer** l'enlève.

Le titre, la description, les étiquettes, le dossier de sortie et l'id Workshop sont gardés pour chaque map dans son fichier annexe.

## Onglet Publication

Affiche l'**Objet du Workshop** (son id, ou *Pas encore publié*), le **Fichier de l'addon** et l'**Icône**, ainsi qu'un champ **Note de version**.

- **Publier comme nouvel objet...** crée un nouvel objet public du Workshop avec votre compte Steam.
- **Mettre à jour l'objet...** envoie une nouvelle version de l'objet déjà publié.

Une confirmation (**Publier sur le Workshop ?**) passe d'abord ; Steam doit être lancé. Hammer Pro n'exécute que le `gmpublish` de votre installation de Garry's Mod (`bin/win64`, puis `bin`), jamais un programme trouvé ailleurs. Sa sortie s'affiche dans le journal et **Annuler** le ferme. Construisez la publication d'abord (*Construisez d'abord la publication.*).

## Onglet Rapport

Chaque fichier nécessaire à la map avec sa taille, son origine, sa destination (**Pakfile**, **Addon**, **Non inclus**, **Non chargeable**) et la raison (faces de brush, une clé d'entité, une sortie, une instance). Filtrez avec **Filtrer les fichiers...**, et cochez **Afficher les fichiers non inclus** pour voir aussi les fichiers du jeu de base. Sections :

- **Origines** : fichiers par origine (jeu de base, addons, Workshop, dossiers personnalisés, téléchargements, jeux montés, contenu supplémentaire, fichiers du jeu en vrac, généré), avec les choix d'inclusion pour le Workshop et les jeux montés.
- **Objets du Workshop (N)** : les objets utilisés par la map, avec **Copier les identifiants** (listez-les comme objets requis quand leur contenu n'est pas inclus).
- **Manquants (N)** : fichiers nommés par la map qu'aucun contenu monté n'a.
- **Chemins (N)** : fichiers inclus dans des dossiers génériques, avec des espaces dans le chemin ou aussi fournis par un autre addon.
- **Conseils de taille (N)** : textures qui pourraient être plus légères (DXT5 sans alpha, plus grandes que 1024, sans mipmaps, non compressées).

## MCP

`get_dependency_report`, `open_release_window`, `start_release`, `get_release_checklist`, `get_release_status`, `cancel_release`, `inspect_archive` et `publish_workshop` (qui ouvre l'onglet Publication : l'envoi demande toujours votre confirmation). Voir la [référence des outils MCP](../../MCP.md) (en anglais).

## Astuces

- Cliquez **Analyser** tôt : le rapport montre les fichiers manquants et les dépendances Workshop sans compiler.
- Prenez **BSP autonome** pour les serveurs qui n'utilisent pas le Workshop, **Kit serveur** quand vous gérez le serveur vous-même.
- Les boutons de correction vous emmènent au bon endroit : **Construire en jeu** coche l'option de session de jeu pour vous.

## FAQ

**Pourquoi les soundscapes sont-ils dans le BSP même en mode addon Workshop ?** gmad refuse ces fichiers dans un GMA, ils voyagent donc dans le pakfile, où le jeu les charge.

**Où est gardé l'id Workshop ?** Dans le fichier annexe de la map : la prochaine publication met à jour le même objet.

**La construction dit que des fichiers ne peuvent pas être chargés depuis une map ou un addon.** Certains fichiers (scripts sonores hors de `maps/<map>_level_sounds.txt`, soundscapes hors de `scripts/soundscapes_<map>.txt`) ne sont jamais chargés pour une map. Déplacez leurs entrées comme l'explique le journal.

## Pages liées

- [Protect your BSP](bsp-protection.md)
- [Ressources de la map](map-assets.md)
- [Collecter le contenu pour une publication](../guides/collect-content-for-release.md)
- [Cubemaps](../compiler/cubemaps.md)
- [Lancement du jeu](game-launch.md)

---
[Retour à l'index de la documentation](../README.md)
