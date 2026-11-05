[Documentation](../README.md) > Fonctionnalités > Mises à jour | [English](../../en/features/updates.md) | [Русский](../../ru/features/updates.md)

# Mises à jour

Hammer Pro cherche une version plus récente sur GitHub au démarrage, vous dit ce qui est nouveau et l'installe pour vous : il télécharge la version, se ferme, remplace `hammerpro.exe` là où il est installé, puis redémarre. Rien n'est téléchargé avant que vous cliquiez sur **Mettre à jour**.

## Ce qui se passe au démarrage

1. Juste après l'ouverture de la fenêtre, un thread en arrière-plan interroge l'API des versions GitHub du projet Hammer Pro (`oracleidt/hammerpro`, dernière version) pour connaître le tag le plus récent, par exemple `v1.0.3`.
2. Le tag est comparé à la version que vous utilisez, nombre par nombre (`1.0.10` est plus récente que `1.0.9`).
3. Si la version est plus récente, la fenêtre **Mise à jour disponible** s'ouvre. Sinon, rien ne s'affiche.

La recherche ne bloque jamais l'éditeur et n'affiche jamais d'erreur : sans connexion, derrière un pare-feu, ou si GitHub est lent, l'éditeur démarre simplement comme d'habitude. Les exécutions automatisées (`--script`, `--screenshot`, `--bench-open`) ne cherchent jamais.

## La fenêtre Mise à jour disponible

| Élément | Ce qu'il montre |
| --- | --- |
| Ligne de titre | *Hammer Pro 1.0.3 est disponible* et *Vous utilisez la version 1.0.2.* |
| **Nouveautés** | Les notes de la version (sa description sur GitHub), avec défilement. Les titres, les listes à puces et les paragraphes sont mis en forme. |
| **Mettre à jour** | Télécharge l'archive de la version et l'installe (voir plus bas). |
| **Ignorer** | Ferme la fenêtre. Avec **Ne plus afficher** coché, cette version est sautée : la fenêtre ne s'ouvre plus au démarrage pour elle, et revient pour la version suivante. |
| **Ne plus afficher** | Mémorise la version dans le réglage `update.skipped_version`. Une recherche manuelle (**Rechercher des mises à jour...**) l'affiche quand même. |

Quand une version n'a pas d'archive Windows, la fenêtre propose **Ouvrir la page de la version** à la place de **Mettre à jour**.

## L'installation

1. **Mettre à jour** télécharge `HammerPro-<version>-windows.zip` dans un dossier temporaire, avec une barre de progression et un bouton **Annuler**. La taille est vérifiée et l'archive doit contenir `hammerpro.exe` : un mauvais téléchargement est refusé avant que quoi que ce soit ne soit fermé.
2. Hammer Pro se ferme ensuite comme **Fichier > Quitter** : si une map a des modifications non enregistrées, on vous demande d'enregistrer, d'abandonner ou d'annuler d'abord. **Annuler** garde la session ouverte et la mise à jour téléchargée en attente (la fenêtre propose **Redémarrer et installer**). La mise à jour ne démarre pas pendant une compilation.
3. Un petit assistant (une copie de `hammerpro.exe` lancée depuis le dossier temporaire, sans fenêtre) attend la fin du programme et ferme tout processus `hammerpro.exe` qui retiendrait encore le fichier. Il extrait le nouveau `hammerpro.exe` à côté de l'ancien, renomme l'ancien en `hammerpro.exe.old` (gardé comme sauvegarde), met le nouveau à sa place en réessayant tant que Windows verrouille le fichier, puis lance le nouveau `hammerpro.exe`.
4. Si quelque chose échoue (le fichier reste verrouillé, disque plein, archive endommagée), l'ancien programme est remis en place, une fenêtre d'erreur explique ce qui s'est passé et l'ancien Hammer Pro redémarre. Vos réglages ne sont pas touchés.

Le programme est remplacé **au même endroit** que celui que vous avez lancé : une copie installée n'importe où (Bureau, `D:\Outils\HammerPro`...) est mise à jour sur place. Si ce dossier demande les droits d'administrateur (par exemple `C:\Program Files`), la mise à jour échoue sans dégât et le dit ; lancez Hammer Pro depuis un dossier où vous pouvez écrire.

## Paramètres > Mises à jour

**Outils > Options...** (`Ctrl+,`), page **Mises à jour** :

| Option | Effet | Défaut |
| --- | --- | --- |
| **Rechercher les mises à jour au démarrage** | Désactive ou active la recherche en arrière-plan au démarrage. Clé `update.check_on_startup`. | activé |
| **Afficher la fenêtre de mise à jour quand une nouvelle version sort** | L'interrupteur principal de la fenêtre. Réactivez-le pour annuler **Ne plus afficher**, quelle que soit la version masquée : il efface aussi la version ignorée. Clé `update.show_dialog`. | activé |
| **Rechercher maintenant** | Cherche tout de suite et ouvre la fenêtre si une version plus récente existe (ou vous dit que vous êtes à jour). | |
| **Afficher la mise à jour** | Rouvre la fenêtre de la mise à jour trouvée. | |
| Ligne d'état | *Hammer Pro est à jour (dernière version : 1.0.2)*, *La version 1.0.3 est disponible.*, ou la raison de l'échec de la dernière recherche. | |
| **Version ignorée** et **Ne plus ignorer** | Affichés quand vous avez coché **Ne plus afficher**. Clé `update.skipped_version`. | vide |

**Aide > Rechercher des mises à jour...** (et la palette de commandes) fait comme **Rechercher maintenant**.

## Pour tester : les variables HP_UPDATE

Deux variables d'environnement permettent d'essayer tout le déroulement sans vraie version :

| Variable | Effet |
| --- | --- |
| `HP_UPDATE_API_URL` | Remplace l'adresse GitHub. Pointez-la vers un serveur HTTP local qui répond avec le JSON d'une version (`tag_name`, `body`, `assets` avec `name`, `browser_download_url` et `size`). Les téléchargements sont alors acceptés depuis ce même serveur. La page Mises à jour indique que l'adresse est remplacée. Les exécutions automatisées ne cherchent que si elle est définie. |
| `HP_UPDATE_CURRENT_VERSION` | Fait comme si cette version était l'installée (par exemple `1.0.0`), pour que toute version paraisse plus récente. |

Sans `HP_UPDATE_API_URL`, les téléchargements ne sont acceptés que depuis `github.com` (HTTPS) et les serveurs de téléchargement de GitHub.

## FAQ

**Est-ce que quelque chose est envoyé à GitHub en plus de la requête ?** Non. C'est une seule requête anonyme pour la dernière version, avec un `User-Agent` `HammerPro/<version>`. Rien sur vous ni sur vos maps n'est envoyé.

**Où le téléchargement est-il gardé ?** Dans `%TEMP%\HammerPro-update-<processus>-<tag>`. Les anciens dossiers sont supprimés à un démarrage ultérieur.

**Comment revenir en arrière ?** Fermez Hammer Pro, supprimez `hammerpro.exe` et renommez `hammerpro.exe.old` en `hammerpro.exe`. Sautez la version avec **Ne plus afficher** pour qu'elle ne soit plus proposée.

**Je ne veux aucun message de mise à jour.** Décochez **Rechercher les mises à jour au démarrage**. Vous pouvez toujours chercher à la main.

**La mise à jour a échoué.** Lisez le message ; le journal de l'assistant est `update.log` dans le dossier temporaire `%TEMP%\HammerPro-updater-<processus>`. Les causes les plus courantes sont un antivirus qui retient le fichier et un dossier sans droit d'écriture.

## MCP

Les assistants IA lisent l'état avec `get_update_status` (version installée, état, version la plus récente avec ses notes, progression, erreurs, préférences), demandent une recherche avec `check_for_updates`, installent avec `apply_update` (demande `confirm: true`, une mise à jour trouvée et aucune compilation en cours ; il ferme Hammer Pro, donc seulement à la demande de l'utilisateur), et changent `update.check_on_startup`, `update.show_dialog` et `update.skipped_version` avec `set_setting`. L'action `install_update` de `run_action` demande aussi `confirm`. Voir la [référence des outils MCP](../../MCP.md).

## Pages liées

- [Fenêtre des paramètres](settings-dialog.md)
- [Référence des paramètres](../reference/settings.md)
- [Ligne de commande](../reference/command-line.md)
- [Fichiers et dossiers](../reference/files-and-folders.md)

---
[Retour à l'index de la documentation](../README.md)
