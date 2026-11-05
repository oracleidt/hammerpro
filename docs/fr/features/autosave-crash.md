[Documentation](../README.md) > Fonctionnalités > Sauvegarde automatique et plantages | [English](../../en/features/autosave-crash.md) | [Русский](../../ru/features/autosave-crash.md)

# Sauvegarde automatique et plantages

Hammer Pro enregistre en arrière-plan des copies de vos maps modifiées, les sauvegarde en urgence quand quelque chose tourne mal, propose de récupérer une sauvegarde automatique plus récente que la map que vous ouvrez, et écrit un rapport de plantage à envoyer aux développeurs. Un fichier journal garde la trace de chaque session.

## Sauvegarde automatique

Toutes les quelques minutes, chaque map ouverte avec des modifications non enregistrées est écrite dans un nouveau fichier de sauvegarde, en arrière-plan (la console affiche **Sauvegarde automatique effectuée**). Le fichier de la map n'est jamais touché. La copie contient aussi les caméras de la map.

Réglez-la dans **Paramètres > Général**, section **Sauvegarde automatique** :

| Option | Effet | Défaut |
| --- | --- | --- |
| **Enregistrer toutes les** | Minutes entre deux sauvegardes, de 0 à 120. 0 coupe la sauvegarde automatique (**Désactivé**). Clé `edit.autosave_minutes`. | 5 min |
| **Copies conservées par map** | Sauvegardes gardées pour chaque map, de 1 à 100 ; les plus anciennes sont supprimées. Clé `edit.autosave_keep`. | 10 |

La page indique le dossier : *Les sauvegardes automatiques sont dans ...*. Sous Windows, c'est `%LOCALAPPDATA%\HammerPro\autosave`. Les fichiers s'appellent `<map>_<empreinte>_<heure>.vmf`, l'empreinte venant du chemin complet de la map : deux maps du même nom dans des dossiers différents ne partagent jamais leurs copies.

Les sauvegardes automatiques sont des fichiers VMF ordinaires : ouvrez-en une avec **Fichier > Ouvrir une map...** comme n'importe quelle map.

## Récupérer une sauvegarde automatique

Quand vous ouvrez une map qui a une sauvegarde automatique plus récente que le fichier (du travail non enregistré), Hammer Pro demande **Récupérer la sauvegarde automatique ?** (*Une sauvegarde automatique plus récente que map a été trouvée. L'ouvrir ?*), avec le nom de la sauvegarde :

| Bouton | Effet |
| --- | --- |
| **Ouvrir la sauvegarde automatique** | Ouvre la sauvegarde à la place (*Sauvegarde automatique ouverte. Enregistrez pour la conserver.*). |
| **Garder la map enregistrée** | Ouvre le fichier de la map tel quel. |
| **Ouvrir le dossier des sauvegardes automatiques** | Affiche le dossier des sauvegardes. |

Si vous avez fermé une map avec **Ne pas enregistrer**, ses sauvegardes jusque-là ne sont pas proposées : vous avez jeté ce travail exprès (les fichiers restent jusqu'à ce que la rotation les supprime).

## Sauvegardes d'urgence

- Quand une erreur interrompt une image de l'éditeur, les maps modifiées sont sauvegardées à côté des sauvegardes automatiques avant l'écriture du rapport de plantage, qui indique où.
- Quand le périphérique graphique est perdu (réinitialisation du pilote, GPU retiré), les maps modifiées de tous les onglets sont sauvegardées à côté des sauvegardes automatiques et Hammer Pro propose de redémarrer : **Oui** le relance avec tous vos onglets, les maps modifiées ouvertes depuis leurs modifications sauvegardées. Voir [Périphérique graphique perdu](../troubleshooting/graphics-device-lost.md).
- Quand l'ordinateur n'a plus de mémoire, Hammer Pro utilise une petite réserve pour continuer, sauvegarde tout de suite les maps modifiées et vous demande de libérer de la mémoire. Voir [Mémoire insuffisante](../troubleshooting/out-of-memory.md).
- Une erreur graphique qui ne gâche qu'une image s'affiche dans la console (*Erreur graphique (une image a été sautée)*) et Hammer Pro continue ; les suivantes vont dans le fichier journal.

## Rapports de plantage

Quand Hammer Pro plante, il écrit un rapport (message, emplacement, thread, pile d'appels et dernières lignes du journal) dans `crash-<date>.txt` du dossier des plantages, `%APPDATA%\HammerPro\crash` sous Windows. Les 10 rapports les plus récents sont gardés.

Au démarrage suivant, une fenêtre **Hammer Pro s'est fermé de façon inattendue** l'affiche une fois :

| Contrôle | Effet |
| --- | --- |
| **Fichier du rapport :** | Où se trouve le rapport. |
| **Afficher le fichier du rapport** | Ouvre son dossier avec le rapport sélectionné. |
| **Copier le rapport** | Copie son texte dans le presse-papiers, pour le coller dans un message. |
| **Ouvrir les modifications sauvegardées** | Ouvre la sauvegarde d'urgence s'il y en a une (*Vos modifications non enregistrées ont été sauvegardées dans ...*). |

Un plantage hors du code Rust (violation d'accès dans le pilote graphique, dépassement de pile) ou une allocation impossible écrit aussi un rapport, avec le module où il a eu lieu et les dernières lignes du journal ; quand il a lieu dans une tâche de fond, les maps modifiées sont d'abord sauvegardées. Les plantages qui ne laissent pas le temps d'écrire quoi que ce soit (processus tué) sont aussi repérés : chaque session laisse un marqueur qu'une fermeture normale efface, et le démarrage suivant transforme le marqueur d'un processus mort en rapport avec la fin du journal de la session précédente.

Les erreurs des threads de fond ne ferment pas Hammer Pro : elles sont journalisées, et les premières d'une session ont aussi un fichier de rapport (pas proposé au démarrage suivant) dont le chemin est dans le journal.

Les sessions lancées avec `--script` ou `--screenshot` ne lisent ni n'écrivent jamais le dossier des plantages.

## Fichier journal

Chaque session écrit `hammerpro.log` ligne par ligne dans le dossier des journaux, `%LOCALAPPDATA%\HammerPro\logs` sous Windows. La session précédente est gardée sous `hammerpro.old.log`. Les avertissements et les erreurs s'affichent aussi dans la console de l'éditeur. Joignez le journal à un rapport de bug (voir [Signaler un bug](../troubleshooting/reporting-bugs.md)).

## Astuces

- Descendez **Enregistrer toutes les** à 1 ou 2 minutes pendant une grosse session si votre machine est rapide ; l'enregistrement se fait en arrière-plan.
- Après un plantage, cliquez **Copier le rapport** et collez-le dans votre rapport de bug : il contient tout ce dont les développeurs ont besoin.
- Utilisez **Map > Comparer avec une autre map...** sur une sauvegarde automatique pour voir ce qui a changé avant de la récupérer.

## FAQ

**La sauvegarde automatique écrase-t-elle ma map ?** Non. Les sauvegardes vont dans leur propre dossier ; seul **Enregistrer** écrit le fichier de votre map.

**Les maps sans nom sont-elles sauvegardées ?** Oui, toute map ouverte avec des modifications non enregistrées est sauvegardée.

**J'ai coupé la sauvegarde automatique et perdu du travail.** Remettez **Enregistrer toutes les** au-dessus de 0 ; les sauvegardes d'urgence ont toujours lieu en cas de plantage, mais les sauvegardes régulières restent votre filet de sécurité.

## Pages liées

- [Plantages et journaux](../troubleshooting/crashes-and-logs.md)
- [Signaler un bug](../troubleshooting/reporting-bugs.md)
- [Fichiers et dossiers](../reference/files-and-folders.md)
- [Fenêtre des paramètres](settings-dialog.md)

---
[Retour à l'index de la documentation](../README.md)
