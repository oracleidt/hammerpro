[Documentation](../README.md) > Prise en main > Configuration du jeu | [English](../../en/getting-started/game-setup.md) | [Русский](../../ru/getting-started/game-setup.md)

# Configuration du jeu

Hammer Pro lit les textures, les modèles et les définitions d'entités dans le dossier d'un jeu, et compile les maps pour ce jeu. Garry's Mod est le jeu par défaut. Cette page couvre l'assistant du premier lancement et la page **Jeu** des paramètres.

## L'assistant de configuration

L'assistant **Configuration de Hammer Pro** s'ouvre au premier lancement (quand aucun fichier de paramètres n'existe). Relancez-le quand vous voulez avec **Aide > Assistant de configuration...**. Il compte 3 étapes ; `Entrée` passe à l'étape suivante, **Retour** revient en arrière, **Passer** (ou `Échap`) le ferme en gardant les paramètres actuels.

| Étape | Ce que vous choisissez |
|---|---|
| 1. Bienvenue | La langue de l'interface (anglais, français ou russe), et **Vous venez de Hammer ou de Hammer++ ?** : **Non, réglages Hammer Pro**, **Hammer** ou **Hammer++**. Ce choix règle d'un coup le jeu de raccourcis clavier et les contrôles de caméra ; les raccourcis que vous avez modifiés vous-même sont conservés. |
| 2. Pour quel jeu faites-vous des maps ? | Le jeu (voir plus bas). Pour Garry's Mod, son dossier est recherché dans vos bibliothèques Steam la première fois que l'étape s'affiche ; « Tout le nécessaire a été trouvé. » le confirme. |
| 3. Contenu | **Monter les addons**, **Monter les téléchargements du Workshop**, **Utiliser les jeux de mount.cfg** (Garry's Mod uniquement) et vos **Dossiers de textures personnalisés**. |

**Commencer**, à la dernière étape, enregistre vos choix, remonte le contenu et charge les définitions d'entités (« Configuration terminée. Chargement du contenu du jeu... »).

La langue se change ensuite dans **Affichage > Langue** ou dans **Paramètres > Général**. Le jeu de raccourcis se trouve dans **Paramètres > Clavier** et les contrôles de caméra dans **Paramètres > Vues** (voir [Raccourcis clavier](../features/keyboard-shortcuts.md) et [Caméra et navigation](../features/camera-navigation.md)).

## La page Jeu des paramètres

Ouvrez **Outils > Options...** (`Ctrl+,`), page **Jeu**. Chaque modification est enregistrée aussitôt ; les changements de contenu s'appliquent quand vous cliquez sur **Appliquer et recharger le contenu** en bas de la fenêtre, ou quand vous la fermez.

### Jeu

| Option | Effet | Défaut |
|---|---|---|
| Liste des jeux | Le jeu pour lequel les maps sont faites : son contenu est monté, ses définitions d'entités chargées, et les maps se compilent dans son format et ses limites. Les jeux trouvés dans les bibliothèques Steam viennent en premier ; les jeux non installés sont aussi proposés (les maps se compilent quand même dans leur format). Les mods de `steamapps/sourcemods` apparaissent aussi. | Garry's Mod |
| **Rechercher à nouveau** | Relance la recherche dans les bibliothèques Steam (après l'installation ou la désinstallation d'un jeu). | |
| **Dossier du jeu** (hors Garry's Mod) | Le dossier qui contient `gameinfo.txt`. Vide : trouvé via Steam. Un résumé affiche ensuite le dossier trouvé, le format (« Format : ..., BSP version ..., props statiques ..., limites »), les FGD trouvés et l'exécutable du jeu. | Vide |

Une map peut aussi avoir son propre jeu, différent de ce jeu par défaut : le **Jeu cible** de la fenêtre de compilation, gardé avec la map (voir [Jeux cibles](../compiler/target-games.md)). Les addons, le Workshop, `mount.cfg`, le changement de skybox en Lua, le test en jeu avec l'addon pont et la publication sur le Workshop sont propres à Garry's Mod : ils sont désactivés pour les autres jeux.

### Dossier de Garry's Mod

| Option | Effet | Défaut |
|---|---|---|
| **Dossier de Garry's Mod** | Le dossier d'installation (celui qui contient `garrysmod` et `bin`). Vide : détecté au démarrage via Steam. Renseignez-le pour utiliser une autre installation. | Vide (auto) |
| **Détecter** | Lance la recherche dans vos bibliothèques Steam. | |
| **Parcourir...** | Choisit le dossier à la main. | |
| Résultat de la vérification | « Archives VPK : N » et « Addons : N », avec **Vérifier à nouveau**. Un dossier sans `garrysmod/gameinfo.txt` est signalé comme n'étant pas une installation de Garry's Mod. | |

### Contenu (Garry's Mod)

| Option | Effet | Défaut |
|---|---|---|
| **Monter les addons** | Charge les matériaux et modèles de `garrysmod/addons` (addons en dossier et fichiers `.gma`). | Activé |
| **Monter les téléchargements du Workshop** | Charge le contenu de vos addons Workshop abonnés, `.gma` et anciens `.bin` (démarrage plus lent). | Désactivé |
| **Utiliser les jeux de mount.cfg** | Monte les autres jeux Source listés dans `garrysmod/cfg/mount.cfg`. | Activé |
| **Monter le contenu téléchargé des serveurs** | Charge `garrysmod/download` et `garrysmod/cache/workshop`. Les joueurs n'ont pas forcément ces fichiers : les vérifications de publication empaquettent ce que la map utilise. | Activé |

Le contenu du Workshop et le contenu téléchargé s'ouvrent après le reste, en arrière-plan : la barre d'état affiche « Workshop N / M » jusqu'à ce qu'ils soient prêts.

Ordre de priorité, du plus fort au plus faible : dossiers de textures personnalisés, addons, `garrysmod.vpk`, fichiers libres de `garrysmod`, autres VPK de `gameinfo.txt`, dossiers `content` des gamemodes, fichiers libres de `sourceengine`, jeux de `mount.cfg`, dossiers de jeu supplémentaires, jeux Steam par AppID (dans l'ordre de leur liste), éléments du Workshop, contenu téléchargé. Un fichier présent dans une source plus haute masque le même fichier plus bas.

### Dossiers de textures personnalisés

Ajoutez n'importe quel dossier de fichiers VTF/VMT ou d'images (PNG, JPG, TGA, BMP) avec **Ajouter un dossier...**. Chaque ligne comporte une case d'activation, le chemin, un **Préfixe** et un bouton de suppression. Les images deviennent des matériaux nommés `préfixe/fichier` et sont converties en VTF + VMT à la compilation. Le préfixe n'est pas utilisé quand le dossier contient un sous-dossier `materials` ou se trouve dans un dossier `materials` ; il est « détecté » d'après les VMT d'un pack tout prêt. Détails : [Dossiers de textures personnalisés](../features/custom-texture-folders.md).

### Dossiers de jeu supplémentaires

Des dossiers de mod (avec un `gameinfo.txt` ou du contenu libre) à monter aussi. Ajout avec **Ajouter**, suppression avec la croix.

### Jeux Steam par AppID

Montez le contenu d'autres jeux Steam installés par leur AppID, quel que soit le jeu cible (Garry's Mod ou non) : par exemple 220 Half-Life 2, 240 Counter-Strike: Source, 300 Day of Defeat: Source, 380 Half-Life 2: Episode One, 420 Half-Life 2: Episode Two, 400 Portal, 440 Team Fortress 2, 4000 Garry's Mod.

| Contrôle | Effet |
|---|---|
| Champ **AppID** et **Ajouter** | Ajoute un AppID ou plusieurs (`240, 440`), aussi avec `Entrée`. Une valeur qui n'est pas un nombre est refusée avec un message. |
| **Jeux installés** | Liste les jeux Source installés dans vos bibliothèques Steam, avec leurs AppID : cliquez sur l'un d'eux pour l'ajouter. |
| Rechercher à nouveau (flèche circulaire) | Relance la recherche dans les bibliothèques Steam après l'installation ou le déplacement d'un jeu. |
| Lignes de la liste | L'AppID et le nom du jeu (au survol : ses dossiers de jeu). *Non installé* en orange quand Steam n'a pas ce jeu sur cet ordinateur. Les flèches haut et bas changent la priorité ; la croix retire le jeu. |

Hammer Pro trouve chaque jeu par Steam : le dossier de Steam (registre `SteamPath`, sinon le dossier d'installation par défaut), chaque bibliothèque de `steamapps/libraryfolders.vdf`, puis `steamapps/appmanifest_<AppID>.acf` et son `installdir`. Il monte ensuite ce que liste le `gameinfo.txt` du jeu (ses VPK et dossiers, les jeux de base dont il dépend), après le contenu du jeu de la map. Les navigateurs de ressources, la vue 3D et la compilation (l'éditeur passe les dossiers de jeu au compilateur avec `-insert_search_path`) l'utilisent tous. Enregistré dans `mount_app_ids` de la table `[game]` de `settings.toml`. Avec Garry's Mod, `garrysmod/cfg/mount.cfg` fonctionne toujours aussi (**Utiliser les jeux de mount.cfg**). Les assistants IA utilisent les outils MCP `list_app_mounts` et `set_app_mounts` ; la ligne de commande du compilateur accepte `-mount_appid 240,440` (voir [Ligne de commande du compilateur](../compiler/command-line.md)).

### Définitions d'entités (FGD)

| Option | Effet | Défaut |
|---|---|---|
| **FGD principal** | Remplace le FGD du jeu (`bin/garrysmod.fgd` pour Garry's Mod, sinon le FGD du jeu). | Vide (FGD du jeu) |
| **FGD supplémentaires (chargés après le principal)** | FGD de gamemodes, Mapbase, les vôtres. | Aucun |

Ordre de chargement : le FGD principal (ou celui du jeu), puis le FGD de Hammer++ (`hammerplusplus_fgd.fgd`) quand Hammer++ est installé dans le dossier `bin/win64/hammerplusplus` ou `bin/hammerplusplus` du jeu, puis les fichiers supplémentaires. `@include`, `@mapsize`, les exclusions de matériaux et les visgroups automatiques sont pris en charge, et un FGD modifié sur le disque est rechargé en quelques secondes. Un fichier introuvable est signalé dans la console.

### Maps

| Option | Effet | Défaut |
|---|---|---|
| **Dossier des maps** | Dossier proposé par **Ouvrir** et **Enregistrer sous**. | Vide |

### Tests en jeu (Garry's Mod)

Le **Pont Hammer Pro** est un petit addon dans `garrysmod/addons/hammerpro_bridge` qui permet à Hammer Pro de vous placer à la caméra de l'éditeur et d'importer les props que vous disposez en jeu. Il n'est jamais inclus dans vos maps et n'agit que dans les parties locales (serveurs d'écoute). **Installer**, **Mettre à jour** et **Supprimer** le gèrent ; redémarrez le jeu ensuite. **Apparaître à la caméra de l'éditeur** (nécessite le pont) place le joueur à l'emplacement de la caméra 3D quand la map se charge. Voir [Lancer le jeu](../features/game-launch.md).

## Recharger le contenu

- **Appliquer et recharger le contenu** (en bas de la fenêtre des paramètres) remonte le contenu du jeu avec les nouveaux paramètres.
- **Outils > Contenu et éditeurs > Recharger le contenu depuis le disque** recharge le contenu et les définitions d'entités sans toucher aux paramètres, par exemple après avoir ajouté des fichiers dans un addon.

## FAQ

**Garry's Mod est introuvable.** Cliquez sur **Détecter**, ou sur **Parcourir...** pour indiquer son dossier (celui qui contient `garrysmod` et `bin`, en général `steamapps/common/GarrysMod`). La barre d'état affiche « Aucun contenu de jeu » tant qu'il n'est pas réglé ; un clic dessus ouvre cette page.

**Puis-je mapper pour Counter-Strike: Source ou Team Fortress 2 ?** Oui : choisissez le jeu dans la liste. Son contenu et son FGD sont utilisés et la map se compile dans son format. Voir [Jeux cibles](../compiler/target-games.md) et [Porter une map vers un autre jeu](../guides/port-to-another-game.md).

**Mes textures du Workshop manquent.** Activez **Monter les téléchargements du Workshop**, puis **Appliquer et recharger le contenu**.

**Où ajouter le FGD d'un gamemode ?** Dans **FGD supplémentaires**, sur cette page.

## Pages liées

- [Installation](installation.md)
- [Dossiers de textures personnalisés](../features/custom-texture-folders.md)
- [Fenêtre des paramètres](../features/settings-dialog.md)
- [Jeux cibles](../compiler/target-games.md)
- [Lancer le jeu](../features/game-launch.md)
- [Référence des paramètres](../reference/settings.md)

---
[Retour à l'index de la documentation](../README.md)
