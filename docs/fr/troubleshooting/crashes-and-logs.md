[Documentation](../README.md) > Dépannage > Plantages et journaux | [English](../../en/troubleshooting/crashes-and-logs.md) | [Русский](../../ru/troubleshooting/crashes-and-logs.md)

# Plantages et journaux

Ce que fait Hammer Pro quand quelque chose tourne mal, où il écrit ses rapports et ses journaux, et quoi essayer quand la carte graphique ou le cadre de la fenêtre pose problème.

## Après un plantage

Hammer Pro protège votre travail de plusieurs façons :

- **Sauvegarde d'urgence.** Quand l'éditeur intercepte un plantage de son image, il écrit une sauvegarde d'urgence de la map modifiée à côté des sauvegardes automatiques (`%LOCALAPPDATA%\HammerPro\autosave\`) et la note dans le rapport de plantage.
- **Rapport de plantage.** Un plantage du fil principal écrit un rapport (message, emplacement, fil, pile d'appels et dernières lignes du journal) dans `%APPDATA%\HammerPro\crash\`.
- **Plantages natifs.** Un plantage hors du code Rust (une violation d'accès dans le pilote graphique, un débordement de pile) écrit aussi un rapport, avec l'exception, le module où elle a eu lieu (par exemple `nvoglv64.dll+0x1234`) et les dernières lignes du journal. Quand il a lieu dans une tâche de fond, Hammer Pro sauvegarde d'abord les maps modifiées (il attend jusqu'à 10 secondes pour cela).
- **Mémoire insuffisante.** Hammer Pro garde une réserve de mémoire : quand l'ordinateur n'en a plus, la réserve est rendue, les maps modifiées sont sauvegardées et un message vous demande de libérer de la mémoire. Une allocation qui échoue malgré tout écrit un rapport (« memory allocation of N bytes failed »). Voir [Mémoire insuffisante](out-of-memory.md).
- **Arrêts sans un mot.** Un processus tué, ou un plantage si profond que même le rapport ne peut pas être écrit, ne laisse rien sur le moment. Chaque lancement laisse un marqueur dans le dossier des plantages qu'une fermeture normale supprime ; au lancement suivant, un marqueur laissé par un processus mort devient un rapport avec la fin du journal du lancement précédent.

Au lancement suivant, la fenêtre **Hammer Pro s'est fermé de façon inattendue** affiche le chemin du **Fichier du rapport :** et ces boutons :

| Bouton | Effet |
|---|---|
| **Afficher le fichier du rapport** | Ouvre son dossier avec le rapport sélectionné. |
| **Copier le rapport** | Copie le texte du rapport, pour le coller dans un message. |
| **Ouvrir les modifications sauvegardées** | Ouvre la sauvegarde d'urgence, quand elle existe (« Vos modifications non enregistrées ont été sauvegardées dans ... »). |

Les erreurs des tâches de fond n'arrêtent pas l'éditeur : elles sont notées dans le journal avec leur emplacement, et les premières d'un lancement ont aussi un fichier de rapport dont le chemin figure dans le journal (non proposé au lancement suivant).

Les lancements automatisés (`--script`, `--screenshot`) ne lisent ni n'écrivent jamais le dossier des plantages.

## Sauvegardes automatiques

Indépendamment des plantages, Hammer Pro enregistre en arrière-plan les maps modifiées toutes les 5 minutes par défaut et garde 10 copies par map (**Paramètres > Général > Sauvegarde automatique** : **Enregistrer toutes les**, **Copies conservées par map** ; 0 minute la désactive). Quand vous ouvrez une map qui a une sauvegarde automatique plus récente, **Récupérer la sauvegarde automatique ?** propose de l'ouvrir. Voir [Sauvegarde automatique et récupération](../features/autosave-crash.md).

## Le fichier journal

| Fichier | Contenu |
|---|---|
| `%LOCALAPPDATA%\HammerPro\logs\hammerpro.log` | Le lancement en cours, écrit ligne par ligne (il survit à un arrêt brutal). |
| `%LOCALAPPDATA%\HammerPro\logs\hammerpro.old.log` | Le lancement précédent. |

Le journal contient les diagnostics en anglais des bibliothèques (durées de montage, nombres de fichiers, carte graphique choisie, budget de mémoire graphique) ainsi que chaque avertissement et chaque erreur. Les avertissements et erreurs apparaissent aussi, traduits, dans le panneau **console** (`` ` ``).

Un message répété en rafale est écrit au plus 5 fois toutes les 10 secondes ; le suivant indique combien de messages semblables ont été masqués. Écrire dans le journal n'arrête jamais Hammer Pro, même quand sa sortie est fermée.

## Problèmes graphiques

### Erreurs graphiques et périphérique perdu

Une erreur graphique sur une image (validation, souvent un problème de pilote) ne ferme plus l'éditeur : l'image est sautée et la console affiche une fois « Erreur graphique (une image a été sautée) : ... » (les suivantes sont comptées, le détail est dans le journal). Une erreur de mémoire insuffisante fait libérer tout de suite de la mémoire graphique, voir [Mémoire insuffisante](out-of-memory.md). Quand le périphérique graphique est perdu (réinitialisation du pilote, GPU retiré), Hammer Pro écrit un rapport, enregistre les maps modifiées à côté des sauvegardes automatiques et propose de redémarrer en rouvrant tous vos onglets, voir [Périphérique graphique perdu](graphics-device-lost.md).

### Choisir l'API graphique

Hammer Pro dessine avec wgpu. Par défaut, il essaie Vulkan et DirectX 12 (et Metal sur macOS), en préférant un GPU et une API capables de dessiner des fenêtres transparentes (pour le cadre arrondi), puis une carte dédiée plutôt qu'une carte intégrée. La carte choisie est notée dans le journal (« graphics adapter: ... »).

Pour forcer une API, définissez la variable d'environnement `WGPU_BACKEND` avant de lancer Hammer Pro, par exemple dans une invite de commandes :

```bat
set WGPU_BACKEND=vulkan
hammerpro.exe
```

Valeurs : `vulkan`, `dx12`, `metal`, `gl` (plusieurs valeurs séparées par des virgules). Essayez une autre valeur quand la fenêtre reste noire, scintille ou que le pilote plante au démarrage.

Mettez d'abord à jour votre pilote graphique : la plupart des problèmes d'affichage viennent de pilotes anciens.

## Problèmes de cadre de fenêtre

Hammer Pro dessine sa propre barre de titre et son cadre arrondi. Si la fenêtre se déplace ou se redimensionne mal, reste invisible en bureau à distance, ou si votre gestionnaire de fenêtres a du mal avec elle, activez **Paramètres > Général > Cadres de fenêtre natifs** : les barres de titre et cadres du système sont utilisés à la place, tout de suite. Quand le GPU ne sait pas dessiner de fenêtres transparentes, le cadre est simplement carré.

Le réglage est enregistré sous la forme `native_frames = true` dans la table `[ui]` de `%APPDATA%\HammerPro\settings.toml`, que vous pouvez modifier à la main si la fenêtre est inutilisable.

## Repartir de paramètres propres

Si Hammer Pro se comporte mal après un changement de réglage, fermez-le et renommez `%APPDATA%\HammerPro\settings.toml`. Le lancement suivant utilise les valeurs par défaut et ouvre l'assistant de configuration. Un fichier de paramètres invalide est aussi ignoré (valeurs par défaut, avertissement dans le journal).

## FAQ

**Hammer Pro s'est fermé et j'ai perdu mes modifications.** Rouvrez la map : si une sauvegarde automatique ou d'urgence est plus récente, Hammer Pro la propose. Vous pouvez aussi regarder dans `%LOCALAPPDATA%\HammerPro\autosave\` (fichiers nommés `<map>_<hash>_<heure>.vmf`).

**Où trouver ce qui s'est mal passé ?** Dans le rapport de plantage (`%APPDATA%\HammerPro\crash\`) et le fichier journal (`%LOCALAPPDATA%\HammerPro\logs\hammerpro.log`). Joignez les deux à un rapport de bug, voir [Signaler un bug](reporting-bugs.md).

**La fenêtre est noire ou scintille.** Mettez à jour le pilote graphique, puis essayez `WGPU_BACKEND=vulkan` ou `WGPU_BACKEND=dx12`.

## Pages liées

- [Mémoire insuffisante](out-of-memory.md)
- [Périphérique graphique perdu](graphics-device-lost.md)
- [Sauvegarde automatique et récupération](../features/autosave-crash.md)
- [Cadres de fenêtre](../features/window-frames.md)
- [Signaler un bug](reporting-bugs.md)
- [Performances de l'éditeur](performance.md)
- [Fichiers et dossiers](../reference/files-and-folders.md)

---
[Retour à l'index de la documentation](../README.md)
