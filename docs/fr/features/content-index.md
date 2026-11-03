[Documentation](../README.md) > Fonctionnalités > Index et analyse du contenu | [English](../../en/features/content-index.md)

# Index et analyse du contenu

Au lancement, Hammer Pro liste tous les fichiers des dossiers du jeu, des dossiers de textures personnalisés et des jeux montés (une installation typique dépasse 300 000 fichiers). Le premier lancement analyse le disque. Les lancements suivants, même après un redémarrage de l'ordinateur, lisent un seul petit fichier et ne regardent à nouveau que les dossiers qui ont changé.

## L'index enregistré

- L'index est un seul fichier binaire, `content_index.bin`, dans le dossier de cache de Hammer Pro (`%LOCALAPPDATA%\HammerPro` sous Windows). Il contient, pour chaque arborescence analysée, les noms de fichiers et la date de modification de chaque dossier.
- Au lancement, chaque dossier est comparé à sa date enregistrée. Un dossier inchangé garde sa liste enregistrée ; un dossier modifié est listé à nouveau ; un nouveau dossier est analysé ; un dossier supprimé est oublié. Ajouter, supprimer ou renommer un fichier change la date du dossier qui le contient : le changement est vu au prochain lancement (ou avec **Recharger le contenu depuis le disque**).
- Un dossier modifié dans les dernières secondes avant l'analyse n'est jamais cru au lancement suivant : un changement fait pendant l'analyse ne peut donc pas être manqué.
- L'index est indexé par le chemin du dossier, normalisé (casse, barres, barre finale, `..`, préfixe `\\?\`, liens symboliques et jonctions). Changer les dossiers personnalisés, les dossiers de jeu supplémentaires, les jeux Steam ou `mount.cfg` ajoute simplement les nouveaux dossiers à l'index ; les entrées inutilisées depuis 30 jours sont oubliées.
- Les archives VPK et GMA sont ouvertes directement à chaque fois (leur table de fichiers est lue dans l'archive, quelques millisecondes) : une archive modifiée est donc toujours vue.
- Le fichier est écrit dans un fichier temporaire puis renommé : un plantage ne peut pas laisser un index à moitié écrit. Un fichier absent, tronqué, corrompu ou étranger (autre version de Hammer Pro, somme de contrôle incorrecte) est ignoré avec une ligne dans le journal : une analyse complète est faite et un bon index le remplace.

## Paramètres > Jeu > Analyse du contenu

| Contrôle | Effet |
|---|---|
| **Garder un index du contenu sur le disque** | Active ou désactive l'index enregistré (actif par défaut). Désactivé : chaque lancement analyse tout et le fichier n'est pas écrit. |
| **Analyse des dossiers** | **Auto** (par défaut), **Toujours en parallèle** ou **Toujours un à la fois**. Appliqué au prochain chargement du contenu. |
| **Reconstruire l'index** | Supprime l'index enregistré et recharge le contenu du jeu avec une analyse complète. C'est aussi l'action **Reconstruire l'index du contenu** (`rebuild_content_index`, palette de commandes ou menu Outils). |

La ligne sous les boutons indique le nombre d'arborescences enregistrées, combien de dossiers ont été réutilisés ou réanalysés depuis le lancement, la durée de la dernière analyse et le nombre de disques durs détectés.

## Disques durs et SSD

Sur un disque dur, analyser beaucoup de dossiers en même temps fait sauter la tête de lecture et c'est plus lent que de les lire l'un après l'autre. Sur SSD ou NVMe c'est l'inverse. En **Auto**, Windows est interrogé (la propriété « pénalité de recherche » du périphérique de stockage de chaque volume) pour savoir quel type de disque contient chaque dossier :

- disque rotatif (disque dur) : ses dossiers sont analysés un à la fois ;
- SSD, NVMe ou disque qui ne répond pas (par exemple certains adaptateurs USB ou partages réseau) : analyse en parallèle.

Le choix se fait par volume : un jeu sur `D:` (disque dur) et des dossiers personnalisés sur `C:` (NVMe) sont analysés chacun de la meilleure façon, en même temps. **Toujours en parallèle** et **Toujours un à la fois** ignorent le type de disque.

## Dossiers en double

Un dossier déclaré deux fois n'est monté qu'une fois, à la priorité de sa première déclaration. Cela couvre un dossier personnalisé qui est aussi une entrée de `mount.cfg`, un dossier de jeu supplémentaire aussi monté par `mount.cfg` ou par AppID, et le même dossier écrit autrement (`D:\Games\X`, `d:/games/x/`, `D:\Games\Y\..\X`, `\\?\D:\Games\X`, une jonction ou un lien symbolique vers lui). Un dossier personnalisé d'images simples (sans sous-dossier `materials`) est une autre vue du dossier et reste à côté d'un montage de jeu du même dossier.

## MCP

- `get_content_index` : l'état de l'index (fichier, taille, entrées), les dossiers vérifiés, réutilisés et réanalysés, la durée de la dernière analyse, le mode d'analyse et le type de chaque disque vu.
- `rebuild_content_index` : identique à **Reconstruire l'index**.
- `run_action` avec `rebuild_content_index`.
- `set_setting` avec `scan.use_index` (`true` ou `false`) et `scan.mode` (`"auto"`, `"parallel"`, `"sequential"`), appliqué au prochain chargement du contenu.

## Questions

**L'index semble périmé.** Cliquez sur **Reconstruire l'index**. Les partages réseau et certains systèmes de fichiers ne mettent pas toujours à jour la date des dossiers ; désactivez l'index pour eux ou reconstruisez-le après un changement.

**L'index est-il partagé entre ordinateurs ?** Non, il se trouve dans le dossier de cache de votre profil et peut être supprimé à tout moment.

## Voir aussi

- [Configuration du jeu](../getting-started/game-setup.md)
- [Dossiers de textures personnalisés](custom-texture-folders.md)
- [Fenêtre des paramètres](settings-dialog.md)
- [Référence des outils MCP](../../MCP.md) (en anglais : `get_content_index`, `rebuild_content_index`)

---
[Retour à l'index de la documentation](../README.md)
