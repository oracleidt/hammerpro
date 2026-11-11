[Documentation](../README.md) > Fonctionnalités > Lancement du jeu | [English](../../en/features/game-launch.md) | [Русский](../../ru/features/game-launch.md)

# Lancement du jeu

Hammer Pro teste votre map en jeu rapidement : il charge la map compilée dans le Garry's Mod déjà lancé au lieu de redémarrer le jeu, peut démarrer le jeu pendant la compilation, peut vous placer à la caméra de l'éditeur, et peut ramener dans la map les props que vous avez disposés en jeu. Les autres jeux Source sont aussi lancés sur la map, sans les bonus propres à Garry's Mod.

## Commandes

| Commande | Raccourci | Effet |
| --- | --- | --- |
| **Fichier > Compiler > Compiler et lancer...** | `Ctrl+F9` | Ouvre la fenêtre de compilation, où **Lancer le jeu** lance la map après la compilation. |
| **Fichier > Compiler > Compiler maintenant** | `F9` | Compile avec les réglages actuels (et lance le jeu si **Lancer le jeu** est coché). |
| **Fichier > Compiler > Lancer la map dans le jeu** | | Charge la map compilée du document ouvert dans le jeu lancé, ou démarre le jeu dessus. |
| **Fichier > Compiler > Recharger la map dans le jeu** | | Pareil, pour un jeu déjà lancé. |
| **Fichier > Importer > Importer les props depuis le jeu** | | Ramène dans la map les props placés en jeu (nécessite le pont). |

Avec le jeu de raccourcis Hammer ou Hammer++, `F9` ouvre la fenêtre de compilation et `Ctrl+F9` compile tout de suite (voir [Raccourcis clavier](keyboard-shortcuts.md)).

La map doit être enregistrée et compilée : la map compilée porte le nom du VMF (*Enregistrez et compilez d'abord la map*). Pour Garry's Mod, `<map>.bsp` doit se trouver dans le dossier maps du jeu (*compilez d'abord la map*).

## Comment la map arrive dans le jeu

- **Réutiliser le jeu lancé** (activé par défaut) : quand Garry's Mod tourne, la map y est chargée, sans second processus et sans la fenêtre « Source Engine Not Running » pendant le démarrage. Sinon, le jeu démarre sur la map.
- Si le jeu est en train de démarrer, la map attend la fenêtre du jeu et se charge dès qu'il est prêt (*Garry's Mod démarre : la map se chargera dès qu'il sera prêt*).
- La barre d'état montre l'état du jeu : **Jeu prêt**, **Jeu en démarrage**, ou rien quand il n'est pas lancé.

## Options de lancement de la fenêtre de compilation

Sous les réglages de compilation, la ligne **Jeu** regroupe les options de lancement. Elles s'appliquent quand **Lancer le jeu** est coché et que le jeu n'est pas ouvert **Via Steam**.

| Option | Effet | Défaut |
| --- | --- | --- |
| **Lancer le jeu** | Lance la map après la compilation. | activé |
| **Via Steam** | Ouvre la map avec un lien `steam://` au lieu de lancer l'exécutable du jeu. | désactivé |
| **Arguments du jeu** | Arguments supplémentaires pour le jeu. `-game`, `-hijack` (réutilise un jeu déjà lancé) et `+map` sont ajoutés sauf si vous les indiquez. `{map}` est remplacé par le nom de la map. | `-console -dev` (plus `-novid`, ajouté une fois) |
| **Démarrer le jeu :** **après la compilation** | Démarre le jeu une fois la map compilée (comme Hammer). | choisi |
| **Démarrer le jeu :** **au début de la compilation** | Démarre le jeu dès le début de la compilation : il se lance pendant que le compilateur tourne, puis charge la map. Garry's Mod uniquement. | |
| **Réutiliser le jeu lancé** | Charge la map dans le jeu déjà lancé au lieu de le redémarrer. | activé |
| État du jeu | **Jeu prêt**, **Jeu en démarrage** ou **Jeu non lancé**. | |

### Démarrage rapide

Le bouton **Démarrage rapide** ouvre des options qui accélèrent le démarrage du jeu pour un test. Chaque option ajoute ou retire son paramètre dans **Arguments du jeu** : ce que vous lisez dans ce champ est ce qui est lancé.

| Option | Paramètre | Effet |
| --- | --- | --- |
| **Passer la vidéo d'introduction** | `-novid` | Fait gagner quelques secondes à chaque démarrage. Ajouté une fois aux arguments du jeu au premier lancement. |
| **Fenêtré** | `-windowed` | Lance le jeu dans une fenêtre. |
| **Sans bordure** | `-noborder` | Fenêtre sans bordure. |
| **Taille de la fenêtre** | `-w`, `-h` | Largeur et hauteur de la fenêtre du jeu (1600 x 900 à l'activation ; de 320 à 7680 sur 240 à 4320). |
| **Ouvrir la console** | `-console` | Affiche la console de développement au démarrage. |
| **Messages de développement** | `-dev` | Affiche les messages de développement ; évite aussi le chargement de la map de fond du menu. |
| **Partie locale privée** | `+sv_lan 1 +maxplayers 1` | Un serveur local rien que pour vous : pas de réseau, démarrage plus rapide. |
| **Ignorer le Workshop et les addons inutilisés** | `-noworkshop`, `-noaddons` | Quand la map n'en utilise rien, le jeu ne monte pas les téléchargements du Workshop ni les addons : démarrage plus rapide, mais les outils et armes des addons ne sont pas disponibles en jeu. Paramètre `launch.skip_unused_content`, désactivé par défaut. |
| **Indiquer le temps de chargement de la map** | | Indique combien de temps le jeu a mis à charger la map et où est passé le temps (fichiers et contenu, entités et scripts). Nécessite le pont. Paramètre `launch.load_report`, désactivé par défaut. Le dernier résultat s'affiche dessous : *Dernier chargement : map en N s*. |

## Le pont Hammer Pro

Le pont est un petit addon dans `garrysmod/addons/hammerpro_bridge` qui permet à Hammer Pro de vous placer à la caméra de l'éditeur et d'importer les props que vous disposez en jeu. Il n'est jamais inclus dans vos maps, et il n'agit que dans les parties locales (serveurs d'écoute).

Installez-le depuis **Paramètres > Jeu**, section **Tests en jeu** :

| Contrôle | Effet |
| --- | --- |
| **Pont Hammer Pro : installé / non installé / aucun dossier de jeu** | État de l'addon. |
| **Installer** / **Mettre à jour** / **Supprimer** | Installe, met à jour ou supprime l'addon. Redémarrez le jeu ensuite. |
| **Apparaître à la caméra de l'éditeur** | Quand la map se charge en jeu, place le joueur à l'emplacement de la caméra 3D de l'éditeur (noclip). Nécessite le pont. Paramètre `launch.spawn_at_camera`, désactivé par défaut. |

L'éditeur et le jeu échangent de petits fichiers dans `garrysmod/data/hammerpro/` (`cmd.json`, `ack.json`, `props.json`) ; tout ce qui est relu est limité en taille et vérifié.

### Importer les props depuis le jeu

Faites apparaître et disposez des props en jeu (pont installé, map lancée), puis lancez **Fichier > Importer > Importer les props depuis le jeu**. Les props arrivent dans la map en une seule étape d'annulation : `prop_physics` reste `prop_physics`, `prop_dynamic` et `prop_physics_multiplayer` deviennent `prop_static`, avec leur modèle, skin, position et angles. Sans le pont, vous obtenez *Installez d'abord le pont Hammer Pro (Paramètres > Jeu), puis redémarrez le jeu* ; sans réponse, *Le jeu n'a pas répondu : une map est-elle lancée avec le pont installé ?*

## Autres jeux

Pour un autre jeu que Garry's Mod, **Lancer la map dans le jeu** démarre son exécutable avec `-game <dossier>`, vos arguments du jeu et `+map`, une fois la map compilée dans son dossier maps (compilez-la avec **Copier dans le jeu**). La réutilisation du jeu lancé, l'option de démarrage anticipé, le pont et l'import des props sont propres à Garry's Mod.

## MCP

`launch_game` (avec `reload: true` pour recharger dans le jeu lancé) démarre ou recharge la map depuis les applications d'IA. Voir la [référence des outils MCP](../../MCP.md) (en anglais).

## Astuces

- Laissez le jeu ouvert entre deux tests : **Réutiliser le jeu lancé** charge la nouvelle map en quelques secondes.
- Choisissez **au début de la compilation** pour une longue compilation : le jeu démarre pendant l'éclairage.
- **Apparaître à la caméra de l'éditeur** vous pose exactement là où vous regardiez dans l'éditeur, en noclip.

## FAQ

**Le jeu lance une deuxième copie au lieu de charger la map.** Vérifiez que **Réutiliser le jeu lancé** est coché et que le jeu a été lancé depuis l'installation configurée dans **Paramètres > Jeu**.

**Apparaître à la caméra de l'éditeur est grisé.** Installez d'abord le pont et redémarrez le jeu.

**Le pont part-il dans mon addon publié ?** Non, il n'est jamais inclus dans une map ni dans une publication.

## Pages liées

- [Fenêtre de compilation](../compiler/compile-window.md)
- [Compiler et tester dans Garry's Mod](../guides/compile-and-test-gmod.md)
- [Configuration du jeu](../getting-started/game-setup.md)
- [Construire et enregistrer en contenu](build-content-release.md)

---
[Retour à l'index de la documentation](../README.md)
