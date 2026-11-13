[Documentation](../README.md) > Fonctionnalités > Lien direct avec Garry's Mod | [English](../../en/features/live-link.md) | [Русский](../../ru/features/live-link.md)

# Lien direct avec Garry's Mod

Le lien direct est un outil que vous activez quand vous le voulez : vous compilez et lancez la map dans Garry's Mod une fois, puis, tant que le lien est actif, les modifications d'entités faites dans Hammer Pro s'affichent aussitôt dans le jeu lancé, sans recompiler. Déplacez un prop et il bouge dans le jeu, changez une lumière et vous en voyez un aperçu, déclenchez l'entrée `Open` d'une porte pour tester votre logique, envoyez le joueur à la caméra de l'éditeur et ramenez-le. Les changements de brushes et d'éclairage sont listés comme demandant une compilation.

Le lien est arrêté par défaut et rien de lui ne tourne tant que vous ne l'avez pas démarré. Il fonctionne avec Garry's Mod en solo et sur un serveur d'écoute que vous hébergez.

## Commandes

Toutes les commandes sont dans **Outils > Intégrations** (et dans la palette de commandes, `Ctrl+P`) :

| Commande | Effet |
| --- | --- |
| **Démarrer le lien direct avec Garry's Mod** | Installe l'addon du lien dans Garry's Mod, ouvre une session et ouvre le panneau d'état. |
| **Arrêter le lien direct** | Termine la session : le jeu n'obéit plus, ses aperçus et surlignages disparaissent. |
| **État du lien direct...** | Affiche ou masque le panneau d'état. |
| **Déclencher une entrée dans le jeu...** | Ouvre la fenêtre de déclenchement (cible, entrée, paramètre, délai). |
| **Téléporter le joueur à la caméra 3D** | Place l'œil du joueur à la caméra 3D, regardant dans la même direction, en noclip. |
| **Ramener le joueur** | Renvoie le joueur là où il était avant la première téléportation. |
| **Le joueur suit la caméra 3D** | Tant qu'il est actif, déplacer la caméra 3D déplace le joueur dans le jeu (synchronisation de caméra, de l'éditeur vers le jeu). |
| **La caméra 3D suit le joueur** | Tant qu'il est actif, la caméra 3D de l'éditeur suit l'œil du joueur (synchronisation de caméra, du jeu vers l'éditeur). |
| **Renvoyer toutes les modifications au jeu** | Oublie ce qui a été dit au jeu et renvoie toutes les modifications depuis la compilation. |
| **Retirer l'addon du lien direct de Garry's Mod** | Arrête le lien et supprime l'addon et ses fichiers du dossier de Garry's Mod. |

Les deux synchronisations de caméra s'excluent : en activer une désactive l'autre.

## Pour commencer

1. Ouvrez votre map, enregistrez-la et compilez-la (**Fichier > Compiler > Compiler maintenant**, `F9`).
2. **Outils > Intégrations > Démarrer le lien direct avec Garry's Mod**. Le panneau d'état s'ouvre et indique *En attente du jeu*.
3. Lancez la map dans Garry's Mod (**Fichier > Compiler > Lancer la map dans le jeu**, ou laissez la compilation lancer le jeu).
4. Quand le jeu répond, le panneau indique *Connecté* et une notification dit *Lien direct connecté à <map> dans Garry's Mod*. Le coin en haut à gauche du jeu affiche *Lien direct Hammer Pro*.
5. Modifiez : chaque changement d'entité arrive dans le jeu en une ou deux images. Annuler et rétablir sont envoyés aussi.

Si Garry's Mod tourne déjà quand vous démarrez le lien, le lien se charge tout seul dans le jeu lancé (réglage **Charger le lien dans un jeu déjà lancé**, actif par défaut) : inutile de redémarrer le jeu. Un jeu lancé après le lien charge l'addon avec le jeu.

## Ce qui s'affiche dans le jeu, et comment

| Modification dans Hammer Pro | Dans le jeu |
| --- | --- |
| Déplacer ou tourner un `prop_physics`, un `prop_dynamic` ou toute entité ponctuelle | Déplacé aussitôt (les props physiques sont réveillés là où vous les posez). |
| Supprimer une entité | Retirée. Annuler en recrée une copie. |
| Créer une entité (prop, light_dynamic, info_target, entités logiques...) | Créée et apparue avec ses valeurs et ses sorties. |
| Changer des valeurs | `rendercolor`, `renderamt`, `rendermode`, `renderfx`, `skin`, `model`, `modelscale`, `targetname`, `disableshadows`, et toute autre valeur que le moteur accepte en jeu (sons d'`ambient_generic`, vitesses, flags de certaines entités...). |
| Ajouter une sortie | Ajoutée à l'entité dans le jeu : la nouvelle logique fonctionne aussitôt. |
| Changer un `prop_static` | Affiché comme modèle côté client au nouvel endroit, avec son modèle, son skin, sa couleur et son échelle, marqué *Hammer Pro : jusqu'à la prochaine compilation*. Le moteur ne peut pas masquer le prop statique compilé, il reste donc visible jusqu'à la compilation. Un prop statique supprimé est marqué d'une boîte rouge *Supprimé : compilez pour mettre à jour*. |
| Changer une `light` ou une `light_spot` | Un aperçu : une lumière dynamique pour `light`, une texture projetée avec ombres pour `light_spot`, avec la couleur, la luminosité, le cône, le pitch et la distance de l'entité. Il est marqué *Aperçu Hammer Pro* dans le jeu. L'éclairage précalculé reste tel quel jusqu'à la compilation. |
| Sélectionner des objets | La sélection est surlignée dans le jeu : un halo orange autour des entités et une boîte orange autour de leurs limites (brushes compris). |
| Changer des brushes, des displacements ou `light_environment` | Listé sous **Demande une compilation** dans le panneau. |

Les entités de la map sont retrouvées dans le jeu par leur id Hammer (le compilateur l'écrit dans la map), puis par nom de cible, puis par classe et origine, et les entités brush par l'ordre de leurs modèles brush. Les entités que le compilateur retire (props statiques, lumières sans nom, `func_detail`, overlays, cubemaps) ne sont pas cherchées.

## Le panneau d'état

**Outils > Intégrations > État du lien direct...** :

| Ligne | Sens |
| --- | --- |
| **État** | *Arrêté*, *En attente du jeu*, *Connecté*, *Une autre map est lancée* (le jeu fait tourner une map d'un autre nom que le VMF ouvert), *Connexion perdue* (le jeu ne répond plus : fermé, en pause dans le menu, ou planté). |
| **Map** | La map ouverte ici et la map lancée dans le jeu. |
| **Addon** | *chargé avec le jeu* (l'addon était dans le dossier addons au démarrage du jeu), *chargé dans le jeu lancé*, *installé* ou *non installé*. |
| **Latence** | Temps entre une modification et sa confirmation par le jeu, dernière valeur et moyenne. En général 30 à 100 ms. |
| **En attente d'application** | Opérations envoyées pas encore confirmées. |
| **Entités modifiées depuis la compilation** | L'écart entre la map et la map compilée. |
| **Entités de la map trouvées dans le jeu** | Combien d'entités de la map ont été retrouvées dans le jeu. |
| **Dans le jeu** | Entités créées par le lien, aperçus de props statiques et de lumières. |
| **Demande une compilation** | Pourquoi une compilation est nécessaire : brushes ou displacements changés, lumières changées (aperçus seulement), props statiques changés, entités brush ajoutées ou supprimées, sorties supprimées (le jeu les garde), une entité a changé de classe, des entités modifiées n'existent pas dans le jeu. |
| **Non envoyé** | Les modifications qui ne peuvent pas être envoyées, avec la raison. |
| **Erreurs** | Les dernières erreurs signalées par le jeu (par exemple *entity not found in game*). |

Boutons : **Démarrer**, **Arrêter**, **Téléporter à la caméra**, **Retour**, **Déclencher une entrée...**, **Renvoyer**, **Le jeu correspond à la map** (considère la map actuelle comme celle que fait tourner le jeu, après une compilation et un rechargement faits hors de Hammer Pro), **Retirer l'addon**. En dessous, les réglages du lien.

## Déclencher une entrée dans le jeu

**Outils > Intégrations > Déclencher une entrée dans le jeu...** (ou **Déclencher une entrée...** dans le panneau) teste la logique sans marcher jusqu'à un bouton :

- **Cible** : un nom de cible de la map (un nom finissant par `*` en désigne plusieurs), `!player`, ou `#` suivi d'un id d'entité (`#42`), qui utilise la correspondance ci-dessus. **Utiliser la sélection** la remplit depuis l'entité sélectionnée.
- **Entrée** : le nom de l'entrée. Quand la cible est connue, la flèche liste les entrées de sa classe depuis le FGD (`Open`, `Close`, `Toggle`, `TurnOn`, `Trigger`...).
- **Paramètre** et **Délai (s)**.
- **Déclencher** l'envoie ; la notification dit *Open déclenché sur door1 dans le jeu*.

## Compiler avec le lien actif

Quand vous compilez et que le jeu recharge la nouvelle map, le lien remarque la nouvelle compilation, prend la map telle qu'elle était au début de la compilation comme nouvel état du jeu, retrouve les entités et renvoie les modifications faites depuis. Vous pouvez garder le lien actif toute une session.

## Réglages

Dans le panneau et dans `settings.toml` (`[livelink]`), accessibles aussi par MCP (`set_setting`, `get_settings`) :

| Réglage | Clé | Par défaut |
| --- | --- | --- |
| Le joueur suit la caméra 3D | `livelink.camera_to_game` | désactivé |
| La caméra 3D suit le joueur | `livelink.camera_from_game` | désactivé |
| Surligner la sélection dans le jeu | `livelink.highlight_selection` | activé |
| Prévisualiser les lumières dans le jeu | `livelink.light_previews` | activé |
| Charger le lien dans un jeu déjà lancé | `livelink.load_into_running_game` | activé |
| Retirer l'addon à l'arrêt du lien | `livelink.remove_addon_on_stop` | désactivé |

Ces réglages n'agissent que tant que le lien est actif.

## Fonctionnement et sécurité

- Démarrer le lien écrit un fichier Lua, `garrysmod/addons/hammerpro_livelink/lua/autorun/hammerpro_livelink.lua`, et une copie dans `garrysmod/data/hammerpro_livelink/addon.txt` qui sert à le charger dans un jeu déjà lancé (par la commande console `lua_run`, envoyée à la fenêtre du jeu comme le fait **Lancer la map dans le jeu**). `sv_cheats` n'est pas nécessaire.
- Hammer Pro et le jeu échangent par de petits fichiers dans `garrysmod/data/hammerpro_livelink`, sur cet ordinateur seulement : aucun port réseau n'est ouvert. Le jeu regarde le dossier environ 30 fois par seconde.
- Chaque session a un jeton aléatoire. L'addon n'obéit qu'aux commandes portant le jeton d'une session vivante (Hammer Pro le rafraîchit chaque seconde ; après 10 secondes sans lui, l'addon se met en veille et efface ses aperçus). Arrêter le lien, ou fermer Hammer Pro, termine la session.
- L'addon n'exécute jamais de code reçu : il ne fait que déplacer, créer, supprimer et configurer des entités, déclencher des entrées et déplacer le joueur. Les entités qui exécutent des commandes console ou du code (`point_servercommand`, `point_clientcommand`, `point_broadcastclientcommand`, `lua_run`) ne peuvent être ni créées, ni modifiées, ni visées, et les entrées `Command`, `RunCode` et `RunPassedCode` sont refusées, y compris dans les nouvelles sorties.
- **Retirer l'addon** supprime le dossier de l'addon et les fichiers du lien ; vos autres addons ne sont jamais touchés.

## MCP

Les assistants IA pilotent le lien avec `live_link_start`, `live_link_stop` (`remove_addon`), `live_link_status`, `live_link_push` (`resync`, `mark_compiled`), `live_link_fire` et `live_link_teleport` (`back`), et les réglages avec `set_setting` (`livelink.*`). Voir la [référence des outils MCP](../../MCP.md#live-link).

## Astuces

- Gardez le jeu en fenêtre (`-windowed` dans les options de lancement) à côté de Hammer Pro pour voir arriver les modifications.
- Activez **La caméra 3D suit le joueur**, marchez jusqu'à un endroit dans le jeu, désactivez-le : la caméra de l'éditeur y est.
- Après un gros changement (beaucoup d'entités collées), **Renvoyer** renvoie tout d'un coup.

## FAQ

**Faut-il redémarrer Garry's Mod après avoir démarré le lien ?** Non : le lien se charge tout seul dans un jeu déjà lancé. Redémarrer n'est utile que si vous avez désactivé **Charger le lien dans un jeu déjà lancé**.

**Pourquoi mon prop statique déplacé apparaît-il deux fois ?** Le moteur ne peut pas masquer un prop statique compilé. Le lien affiche le nouveau ; l'ancien disparaît à la prochaine compilation.

**L'aperçu de lumière est-il l'éclairage final ?** Non, c'est une approximation dynamique marquée comme aperçu. Compilez pour calculer le vrai éclairage.

**Cela marche-t-il sur un serveur dédié ?** Non : en solo et sur un serveur d'écoute hébergé sur cet ordinateur seulement.

En cas de problème : voir [Dépannage du lien direct](../troubleshooting/live-link.md).
