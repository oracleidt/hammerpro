[Documentation](../README.md) > Fonctionnalités > Stockage | [English](../../en/features/storage.md) | [Русский](../../ru/features/storage.md)

# Stockage (espace disque)

Hammer Pro garde des réglages, des caches, des sauvegardes automatiques et des fichiers temporaires sur votre disque. La page **Stockage** montre l'espace qu'ils occupent, ce qui en prend le plus, et permet de supprimer ce dont vous n'avez pas besoin. Seuls les dossiers propres à Hammer Pro sont listés et nettoyés : vos maps, vos dossiers de jeu et les fichiers à côté de vos maps n'en font jamais partie.

## Où la trouver

**Paramètres > Stockage** (**Outils > Options...**, `Ctrl+,`). La page analyse les dossiers dans un thread d'arrière-plan à son ouverture : l'éditeur n'attend jamais, et une barre de progression indique les emplacements et les fichiers comptés jusqu'ici.

## La liste

Le total est affiché en haut, puis une ligne par type de données, **du plus gros au plus petit**. Chaque ligne contient :

| Élément | Ce qu'il montre |
| --- | --- |
| Case à cocher | Sélectionne la ligne pour **Supprimer la sélection**. |
| Nom et description | Ce que sont les données et ce qui arrive si on les supprime. Une étiquette colorée signale les **Données de récupération** et **Vos données**. |
| Chemin | Le dossier (survolez pour voir tous les dossiers de la ligne). |
| Taille et fichiers | La taille sur le disque et le nombre de fichiers, et combien d'éléments sont en cours d'utilisation et ignorés. |
| Barre | La part du total, en pourcentage. |
| **Ouvrir le dossier** | Ouvre le dossier dans l'explorateur de fichiers. |
| **Supprimer** | Supprime la ligne après confirmation. |

## Ce qui est listé

| Ligne | Contenu | Risque |
| --- | --- | --- |
| Miniatures | Miniatures de matériaux, modèles et prefabs | sans risque, redessinées |
| Cache d'empaquetage, Contenu de compilation, Sorties de compilation, Fichiers temporaires de compilation, Historique de compilation, Builds de publication | Fichiers du compilateur et de l'outil de publication | sans risque, reconstruits à la prochaine compilation |
| Maps décompilées et assets extraits | Maps décompilées pour le labo, modèles extraits des jeux | sans risque, extraits de nouveau |
| Résultats du BSP Sniffer | VMF, BSP, contenu et rapport de chaque opération du [BSP Sniffer](bsp-sniffer.md) (dossier par défaut) | vos données |
| Journaux | Journaux de l'exécution précédente (le journal en cours d'écriture est conservé) | sans risque |
| Index du contenu | L'index enregistré du contenu des jeux | sans risque, reconstruit par la prochaine analyse |
| Téléchargements de mises à jour | Paquets de mise à jour téléchargés dans le dossier temporaire du système | sans risque |
| Fichiers temporaires | Éléments `hammerpro_*` du dossier temporaire du système | sans risque |
| Autres données en cache | Tout le reste du dossier de cache | sans risque |
| Sauvegardes automatiques | Copies de secours des maps en cours d'édition | données de récupération |
| Rapports de plantage | Rapports et sauvegardes d'urgence | données de récupération |
| État par map | Vue et disposition enregistrées de chaque map | vos données |
| Réglages | settings.toml (options, raccourcis, maps récentes) | vos données |
| Prefabs, Préréglages de saupoudrage | Ce que vous avez créé dans les dossiers par défaut | vos données |

L'ordre des lignes suit les tailles, pas ce tableau. Les fichiers en cours d'utilisation (le journal en cours d'écriture, les marqueurs des sessions actives, les fichiers temporaires de cette session) sont comptés mais ignorés à la suppression. Les dossiers que d'autres fonctions ajoutent au cache de Hammer Pro apparaissent dans la liste.

## Supprimer

- **Supprimer** sur une ligne, ou cochez des lignes puis **Supprimer la sélection (N, taille)**.
- Une fenêtre de confirmation liste exactement ce qui sera supprimé (nom, taille, dossiers) et l'espace libéré. Les lignes **Vos données** ou **Données de récupération** ajoutent un avertissement clair, car ces données ne peuvent pas être récupérées.
- La suppression s'exécute dans un thread d'arrière-plan avec une barre de progression. À la fin, un résumé indique l'espace libéré et, pour chaque élément qui n'a pas pu être supprimé (en cours d'utilisation, droits insuffisants), le chemin et la raison. La liste se met à jour toute seule.
- Les lignes liées à la compilation ne peuvent pas être supprimées pendant une compilation.
- Chaque chemin est vérifié avant suppression : il doit se résoudre à l'intérieur du dossier de sa ligne. Tout le reste est refusé. Les liens sont supprimés, jamais suivis.
- Les dossiers eux-mêmes restent, seul leur contenu est supprimé.

## Astuces

- Supprimer les **Miniatures** et les **Sorties de compilation** est toujours sans risque et libère souvent le plus.
- Gardez les **Sauvegardes automatiques** tant que vos maps ne sont pas enregistrées et compilées.
- Supprimer settings.toml ici retire le fichier enregistré, qui est réécrit quand vous changez une option ou quittez.

## FAQ

**Est-ce que ça supprime mes maps ?** Non. Seuls les dossiers propres à Hammer Pro sont nettoyés. Les dossiers `.hammerpro_build` à côté de vos maps n'en font pas partie.

**Pourquoi une taille n'est-elle pas à zéro après la suppression ?** Les fichiers en cours d'utilisation sont ignorés et listés dans le résumé. Fermez le programme qui les utilise puis appuyez sur **Actualiser**.

**Où sont les dossiers ?** Sous `%APPDATA%\HammerPro` (réglages, prefabs, rapports de plantage) et `%LOCALAPPDATA%\HammerPro` (caches, sauvegardes automatiques, journaux) sous Windows, et dans le dossier temporaire du système.

## MCP

Les assistants d'IA utilisent `get_storage_usage` (total et liste triée avec ids, tailles, risques et chemins) et `delete_storage` (ids, et `confirm` à true pour vraiment supprimer ; `allow_user_data` à true aussi pour les réglages, prefabs, préréglages et l'état par map). Sans `confirm`, rien n'est supprimé et l'outil indique ce qui le serait. Voir la [référence des outils MCP](../../MCP.md).

## Pages liées

- [Sauvegarde automatique et plantages](autosave-crash.md)
- [Fenêtre des paramètres](settings-dialog.md)
- [Index du contenu et analyse](content-index.md)

---
[Retour à l'index de la documentation](../README.md)
