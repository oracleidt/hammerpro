[Documentation](../README.md) > Guides > Rassembler le contenu pour une publication | [English](../../en/guides/collect-content-for-release.md) | [Русский](../../ru/guides/collect-content-for-release.md)

# Rassembler le contenu pour une publication

Une map qui utilise des textures, modèles ou sons personnalisés en a besoin en jeu, sinon les joueurs voient des damiers violets et des erreurs. Ce guide vérifie ce que la map utilise, puis le livre : soit une publication complète avec **Construire et enregistrer en contenu**, soit une simple copie des fichiers avec l'onglet **Ressources de la map**.

## Étape 1 : voir ce que la map utilise

1. Ouvrez **Map > Ressources de la map**. L'onglet **Ressources de la map** du dock de droite liste chaque fichier utilisé par la map : matériaux avec leurs textures, modèles avec leurs fichiers et matériaux, sons avec leurs scripts, fichiers de particules. Chaque entrée indique sa provenance (jeu de base, addon, Workshop, dossier personnalisé, fichier libre, jeu monté, généré) et si elle manque.
2. Utilisez les filtres : **Tout**, **Personnalisé uniquement** (tout ce qui ne fait pas partie du jeu de base, plus les fichiers manquants) et **Manquants**.
3. Corrigez chaque entrée **Manquant** : rectifiez le nom du matériau ou du modèle dans la map, ou ajoutez le contenu (un dossier de textures personnalisé, un addon), puis cliquez sur **Actualiser**.

Les fichiers du jeu de base ne sont jamais nécessaires : tous les joueurs les ont.

## Étape 2 (recommandée) : Construire et enregistrer en contenu

Pour publier sur Garry's Mod, **Fichier > Construire et enregistrer en contenu...** fait tout : compilation finale, cubemaps et nav mesh, intégration, dossier d'addon, GMA et envoi sur le Workshop.

1. Dans l'onglet **Construction**, choisissez quoi **Produire** :
   - **Addon Workshop** : le BSP, un dossier de contenu et un `.gma`.
   - **BSP autonome** : un seul BSP avec tous les fichiers nécessaires dans son pakfile.
   - **Kit serveur** : un BSP autonome plus un dossier FastDL et un fichier Lua qui ajoute la map aux téléchargements.
2. Vérifiez le **Dossier de sortie** (par défaut : `<map>_content` à côté de la map ; **Changer...** pour en choisir un autre).
3. Choisissez les options : **Compilation finale d'abord**, **Construire les cubemaps et la navigation en jeu** (lance Garry's Mod après confirmation), **Inclure le contenu des objets du Workshop** (décoché : ils sont listés comme objets requis), **Inclure le contenu des jeux montés**, **Écrire le .gma**, **Compresser le pakfile (LZMA)**.
4. Cliquez sur **Analyser** pour lister tous les fichiers dont la map a besoin sans rien compiler ni écrire, et lisez l'onglet **Rapport**.
5. Remplissez l'onglet **Addon** : **Titre**, **Type**, **Étiquettes (2 au plus)**, **Description**, **Ignorer** (jokers gmad), et la miniature (**Capturer la vue 3D** ou **Choisir une image...**).
6. Ouvrez l'onglet **Vérifications** : chaque ligne doit être verte avant de publier. La plupart des lignes ont un bouton qui les corrige.
7. Cliquez sur **Construire**. Les constructions suivantes sont incrémentales : les fichiers inchangés sont sautés (voir `.hammerpro_release.toml` dans [Fichiers et dossiers](../reference/files-and-folders.md)).
8. Pour envoyer, utilisez l'onglet **Publication** (**Publier comme nouvel objet...** ou **Mettre à jour l'objet...** avec une **Note de version**). Steam doit être lancé.

Toutes les options sont décrites dans [Construire et enregistrer en contenu](../features/build-content-release.md).

## Étape 2 (variante) : rassembler les fichiers dans un dossier

Pour obtenir une simple copie des fichiers (faire votre propre addon, envoyer le contenu à quelqu'un, l'archiver), utilisez la section **Collecter** de l'onglet **Ressources de la map** :

1. Cochez les entrées à rassembler (**Tout cocher**, **Tout décocher**, ou une par une : une entrée emporte tout ce dont elle a besoin).
2. Choisissez le dossier de **Sortie** (**Choisir un dossier...**). Ce ne doit pas être du contenu de jeu monté.
3. Options :
   - **Copier aussi les ressources du jeu de base** : décoché par défaut, tous les joueurs les ont.
   - **Remplacer les fichiers existants** : décoché, un fichier que le dossier possède déjà avec un autre contenu est gardé (et listé dans le rapport) ; coché, il est remplacé après confirmation.
4. Cliquez sur **Collecter N fichiers**. Les fichiers sont copiés sous `materials/`, `models/`, `sound/`... (les fichiers des VPK et GMA sont extraits), et un rapport `<map>_assets.txt` est écrit dans le dossier. **Afficher le rapport** et **Ouvrir le dossier** les ouvrent.

## Nettoyer un addon : fichiers inutilisés

La section **Fichiers inutilisés d'un dossier de contenu** liste les fichiers sous `materials`, `models`, `sound` et `particles` d'un dossier (un addon...) que cette map n'utilise pas. Choisissez le dossier et cliquez sur **Trouver les fichiers inutilisés**. C'est uniquement un rapport : rien n'est supprimé.

## FAQ

**Certains fichiers viennent de jeux montés (CS:S, épisodes de HL2).** Les joueurs qui n'ont pas ces jeux voient des erreurs. Incluez-les (**Inclure le contenu des jeux montés**, sous votre responsabilité) ou remplacez-les par du contenu du jeu de base.

**Un soundscript est "non chargeable".** Les fichiers soundscript ne peuvent pas accompagner une map : déplacez leurs entrées dans `maps/<map>_level_sounds.txt`.

**Faut-il rassembler les modèles créés par Propper ?** Aucune étape en plus : ils sont trouvés comme tous les modèles de la map et inclus par la construction.

## Pages liées

- [Ressources de la map](../features/map-assets.md)
- [Construire et enregistrer en contenu](../features/build-content-release.md)
- [Dossiers de textures personnalisés](../features/custom-texture-folders.md)
- [Compiler et tester dans Garry's Mod](compile-and-test-gmod.md)
- [Fichiers et dossiers](../reference/files-and-folders.md)

---
[Retour à l'index de la documentation](../README.md)
