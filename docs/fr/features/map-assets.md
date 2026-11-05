[Documentation](../README.md) > Fonctionnalités > Ressources de la map | [English](../../en/features/map-assets.md) | [Русский](../../ru/features/map-assets.md)

# Ressources de la map

L'onglet **Ressources de la map** liste tous les fichiers utilisés par la map ouverte (matériaux avec leurs textures, modèles avec leurs fichiers et matériaux, sons avec leurs scripts, fichiers de particules), indique d'où vient chaque fichier et ce qui manque, puis les copie dans un dossier propre avec **Collecter**. Une deuxième section liste les fichiers d'un dossier de contenu (un addon par exemple) que la map n'utilise pas.

## Ouvrir l'onglet

- **Map > Ressources de la map** affiche l'onglet dans le dock de droite. Relancez la commande quand l'onglet est affiché pour le masquer.
- L'onglet reste dans le dock tant que vous ne le masquez pas (paramètre `content.show_map_assets`, désactivé par défaut).
- Le contenu du jeu doit être chargé. Sinon l'onglet affiche *Le contenu du jeu n'est pas chargé (configurez d'abord le jeu dans les paramètres).* Voir [Configuration du jeu](../getting-started/game-setup.md).

La liste est construite en arrière-plan (*Recherche des fichiers utilisés par la map...*) avec la même recherche de dépendances que la fenêtre de publication : faces des brushes, clés des entités, instances et textures d'éditeur des matériaux. Cliquez **Actualiser** pour relancer la recherche après de gros changements.

## La liste

L'en-tête résume : nombre d'entrées, de fichiers, de textures, de fichiers de modèles et taille totale, plus le nombre de fichiers manquants en rouge.

| Contrôle | Effet |
| --- | --- |
| **Personnalisé uniquement** | Vue par défaut. Les fichiers qui ne font pas partie du jeu de base (addons, Workshop, dossiers personnalisés, fichiers libres) et les fichiers manquants. |
| **Tout** | Tous les fichiers, jeu de base compris. |
| **Manquants** | Seulement les fichiers manquants. |
| **Rechercher un chemin...** | Filtre les entrées par chemin. |
| **Tout cocher** / **Tout décocher** | Coche ou décoche toutes les entrées listées pour la collecte. |
| **Tout replier** | Replie toutes les entrées ouvertes. |
| **Actualiser** | Reconstruit la liste. |

Les entrées sont rangées dans **Matériaux**, **Modèles**, **Sons**, **Particules** et **Autres**. Cliquez le nom d'un groupe pour le replier, la flèche d'une entrée pour voir les fichiers dont elle a besoin (un modèle montre ses fichiers et ses matériaux, un matériau ses textures). La case d'une entrée signifie *Collecter cette entrée et tout ce dont elle a besoin*.

Chaque ligne de fichier affiche une marque, le chemin, la taille et la provenance :

| Marque | Signification |
| --- | --- |
| Point plein (vert) | Contenu personnalisé : il sera collecté. |
| Point creux (gris) | Fait partie du jeu de base : collecté seulement avec **Copier aussi les ressources du jeu de base**. |
| Signe d'avertissement | L'entrée existe mais un fichier dont elle a besoin manque. |
| Croix (rouge) | Manquant : aucun contenu monté n'a ce fichier. |

Provenances : **Jeu de base**, **Addon**, **Workshop**, **Dossier perso**, **Fichier libre**, **Téléchargé**, **Jeu monté**, **Supplémentaire**, **Généré**, **Manquant**. Survolez une ligne pour voir le chemin complet, le type de fichier, sa provenance, sa taille et pourquoi la map en a besoin (quelles faces, clés d'entités ou instances). Clic droit sur une ligne pour **Copier le chemin**.

## Collecter

La section **Collecter** copie les entrées cochées et tout ce dont elles ont besoin dans un dossier de sortie, sous `materials/`, `models/`, `sound/`... Les fichiers qui se trouvent dans des VPK ou des GMA sont extraits. Un rapport est écrit dans le dossier de sortie à la fin.

| Option | Effet | Défaut |
| --- | --- | --- |
| **Sortie** | Dossier où copier les fichiers (chemin complet). Le bouton dossier ouvre un sélecteur. Mémorisé dans `content.collect_folder`. | vide |
| **Copier aussi les ressources du jeu de base** | Tous les joueurs les ont déjà : désactivé, seuls les autres fichiers sont copiés. | désactivé |
| **Remplacer les fichiers existants** | Désactivé : un fichier déjà présent dans le dossier avec un autre contenu est gardé et listé dans le rapport. Activé : il est remplacé, après confirmation (**Remplacer des fichiers ?**). | désactivé |
| **Collecter N fichiers (taille)** | Lance la copie. Une barre de progression et **Annuler** s'affichent pendant l'opération. | |

Le dossier de sortie ne peut pas être un dossier de contenu monté du jeu (une erreur vous demande d'en choisir un autre). Un fichier identique à celui déjà présent compte comme *déjà présent* et n'est pas réécrit.

Après une collecte, l'onglet affiche *Dernière collecte : copiés, remplacés, déjà présents, gardés, en échec, manquants* avec **Ouvrir le dossier** et **Afficher le rapport**.

Le rapport s'appelle `<map>_assets.txt` dans le dossier de sortie (`<map>_assets_2.txt` et ainsi de suite si un rapport existe déjà et que **Remplacer les fichiers existants** est désactivé). Il donne le dossier de sortie, les totaux, les octets écrits, chaque fichier avec son résultat (copié, remplacé, déjà présent, gardé, en échec) et les fichiers manquants.

## Fichiers inutilisés d'un dossier de contenu

La section repliable **Fichiers inutilisés d'un dossier de contenu** liste les fichiers sous `materials`, `models`, `sound` et `particles` d'un dossier (un addon que vous nettoyez, par exemple) que cette map n'utilise pas. C'est un rapport uniquement : rien n'est supprimé.

1. Tapez le dossier dans **Dossier de contenu (un addon...)** ou choisissez-le avec le bouton dossier.
2. Cliquez **Trouver les fichiers inutilisés** (**Annuler** l'arrête ; la liste est alors partielle).
3. Le résultat indique *N fichiers sur M ne sont pas utilisés par la map (taille)*. Filtrez la liste avec **Rechercher un chemin...**, copiez-la avec **Copier la liste** ou ouvrez le dossier avec **Ouvrir le dossier**.

## MCP

Tout l'onglet est accessible aux applications d'IA par le serveur MCP : `list_map_assets`, `collect_map_assets`, `get_map_assets_status`, `cancel_collect_map_assets` et `find_unused_assets`. Voir la [référence des outils MCP](../../MCP.md) (en anglais).

## Astuces

- Restez sur **Personnalisé uniquement** pour voir ce que vous devez livrer ; passez sur **Manquants** avant une publication pour corriger les noms cassés.
- Pour préparer un addon Workshop, la fenêtre de publication ([Construire et enregistrer en contenu](build-content-release.md)) range les fichiers pour vous. La collecte sert pour un addon fait à la main, une sauvegarde ou pour envoyer le contenu à un coéquipier.
- Le rapport des fichiers inutilisés est pratique avant de publier un addon qui a grossi au fil des versions d'une map.

## FAQ

**Pourquoi un fichier est-il marqué Manquant alors que la map s'affiche bien dans l'éditeur ?** L'éditeur affiche peut-être un remplaçant (damier violet et noir), ou le fichier vient d'un contenu qui n'est plus monté. Vérifiez la configuration du jeu et les dossiers personnalisés, puis cliquez **Actualiser**.

**La collecte modifie-t-elle ma map ?** Non. Elle lit la map et copie des fichiers, c'est tout.

**Les fichiers des objets du Workshop sont-ils collectés ?** Oui, c'est du contenu personnalisé (provenance **Workshop**), extrait de leur GMA.

**Peut-elle supprimer les fichiers inutilisés ?** Non. La section les liste seulement. Supprimez-les vous-même après avoir vérifié la liste.

## Pages liées

- [Construire et enregistrer en contenu](build-content-release.md)
- [Collecter le contenu pour une publication](../guides/collect-content-for-release.md)
- [Vérifier les problèmes](check-problems.md)
- [Dossiers de textures personnalisés](custom-texture-folders.md)
- [Référence des outils MCP](../../MCP.md)

---
[Retour à l'index de la documentation](../README.md)
