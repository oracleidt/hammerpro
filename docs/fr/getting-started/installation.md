[Documentation](../README.md) > Prise en main > Installation | [English](../../en/getting-started/installation.md) | [Русский](../../ru/getting-started/installation.md)

# Installation

Hammer Pro 1.0.2 tient en un seul programme, `hammerpro.exe`, sans installateur. Il embarque son propre compilateur de maps : VBSP, VVIS et VRAD de Valve ne sont pas nécessaires.

## Configuration requise

| Élément | Nécessaire |
|---|---|
| Système | Windows 10 ou 11, 64 bits. Le code se compile aussi sous Linux. |
| Carte graphique | Prise en charge de Vulkan, DirectX 12 ou Metal. |
| Jeu | Garry's Mod installé via Steam (détecté automatiquement). D'autres jeux Source sont pris en charge comme cibles de compilation, voir [Configuration du jeu](game-setup.md). |

## Téléchargement et installation

1. Téléchargez la version de Hammer Pro (elle contient `hammerpro.exe`), ou compilez-la vous-même (voir [Compiler depuis les sources](../development/building.md)).
2. Placez `hammerpro.exe` dans le dossier de votre choix, par exemple `C:\Outils\HammerPro\`. Rien d'autre n'est à copier.
3. Lancez-le. Un raccourci sur le bureau ou dans le menu Démarrer est facultatif.

## Premier lancement

- Sans fichier de paramètres, Hammer Pro ouvre l'assistant **Configuration de Hammer Pro** : langue, éditeur dont vous venez (Hammer, Hammer++ ou les réglages Hammer Pro), dossier du jeu et contenu à charger. Voir [Configuration du jeu](game-setup.md).
- La langue de l'interface est d'abord celle de votre système (anglais, français ou russe). Vous pouvez la forcer pour un lancement avec `hammerpro --lang en` ou `--lang fr` (voir [Ligne de commande](../reference/command-line.md)).
- Le contenu du jeu (VPK, addons, Workshop) se monte en arrière-plan : l'éditeur reste utilisable pendant le chargement, et une carte au-dessus des vues affiche la progression.
- Vous pouvez ouvrir une map directement : `hammerpro chemin\vers\map.vmf`, ou glisser un `.vmf` sur la fenêtre.

## Mise à jour

Remplacez `hammerpro.exe` par le nouveau. Vos paramètres, sauvegardes automatiques et données par map restent en place : les clés de paramètres inconnues ou absentes sont tolérées, donc les anciennes et nouvelles versions peuvent partager le même fichier de paramètres.

## Emplacement des fichiers

| Quoi | Emplacement sous Windows |
|---|---|
| Paramètres (`settings.toml`) | `%APPDATA%\HammerPro\` |
| Rapports de plantage | `%APPDATA%\HammerPro\crash\` |
| Fichier journal (`hammerpro.log`, lancement précédent `hammerpro.old.log`) | `%LOCALAPPDATA%\HammerPro\logs\` |
| Sauvegardes automatiques et sauvegardes d'urgence | `%LOCALAPPDATA%\HammerPro\autosave\` |
| Caches (miniatures, données d'édition par map dans `maps\`) | `%LOCALAPPDATA%\HammerPro\` |

Hammer Pro peut aussi écrire dans le jeu : les maps compilées vont dans `garrysmod/maps` (option **Copier dans le dossier maps** de la fenêtre de compilation), le contenu personnalisé des compilations de test dans l'addon `garrysmod/addons/hammerpro_dev`, et l'addon pont facultatif dans `garrysmod/addons/hammerpro_bridge`. La liste complète se trouve dans [Fichiers et dossiers](../reference/files-and-folders.md).

## Désinstallation

1. Supprimez `hammerpro.exe`.
2. Pour effacer aussi vos paramètres et données, supprimez les dossiers `HammerPro` de `%APPDATA%` et de `%LOCALAPPDATA%`.
3. Si vous les avez utilisés, supprimez `garrysmod/addons/hammerpro_dev` et `garrysmod/addons/hammerpro_bridge` (le pont peut aussi être retiré avec **Supprimer** dans **Paramètres > Jeu**).

Vos maps `.vmf` ne sont jamais rangées dans ces dossiers : elles restent là où vous les avez enregistrées.

## FAQ

**Faut-il installer le Source SDK ou Hammer ?** Non. Hammer Pro lit directement le contenu du jeu et compile les maps lui-même. Hammer++ n'est pas nécessaire non plus, mais s'il est installé, son FGD est chargé en plus.

**Puis-je continuer à utiliser Hammer ou Hammer++ ?** Oui. Hammer Pro lit et écrit des fichiers `.vmf` standards, voir [Compatibilité VMF](vmf-compatibility.md).

**La fenêtre s'ouvre mais reste vide ou plante aussitôt.** Voir [Plantages et journaux](../troubleshooting/crashes-and-logs.md) (API graphiques et option des cadres de fenêtre natifs).

## Pages liées

- [Configuration du jeu](game-setup.md)
- [Tour de l'interface](interface-tour.md)
- [Votre première map](first-map.md)
- [Fichiers et dossiers](../reference/files-and-folders.md)
- [Compiler depuis les sources](../development/building.md)

---
[Retour à l'index de la documentation](../README.md)
