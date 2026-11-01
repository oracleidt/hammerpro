[Documentation](../README.md) > Fonctionnalités > Protect your BSP | [English](../../en/features/bsp-protection.md)

# Protect your BSP

Protect your BSP livre aux joueurs une copie protégée de votre map Garry's Mod et garde la map complète pour votre serveur. Le client n'a besoin que de la géométrie et du rendu pour afficher la map, alors que tout le gameplay (logique des entités, entrées et sorties, triggers) tourne sur le serveur. La copie téléchargée par les joueurs peut donc perdre tout ça, porter un filigrane, et devenir bien plus dure à décompiler en quelque chose d'exploitable.

Une map que le client affiche n'est jamais totalement inviolable : un décompilateur récupère toujours une partie de la géométrie. La protection rend une copie volée beaucoup moins utile et traçable, sans casser la map.

## Où la trouver

La protection fait partie de la construction de la publication. Ouvrez **Fichier > Construire et enregistrer en contenu...**, onglet **Construction**, et cochez **Protect your BSP** (sous les options de publication). Ses options s'affichent en retrait dessous. La prochaine **Construire** produit la publication protégée. Voir [Construire et enregistrer en contenu](build-content-release.md) pour le reste de la fenêtre.

## Options

| Option | Effet | Défaut |
| --- | --- | --- |
| **Protect your BSP** | Livrer une copie protégée de la map : logique de jeu, câblage des entités et données de décompilation retirés, filigranée. Garde la map complète pour votre serveur. Clé `content.protect`. | désactivé |
| **Filigrane** | Texte écrit dans la map protégée (votre nom ou serveur), traçable dans une copie volée. Stocké sur l'entité du monde sous `hammerpro_protected` ; les caractères de contrôle sont retirés et le texte est coupé à 120 caractères. Clé `content.protect_watermark`. | vide |
| **Casser les décompilateurs (vider les faces d'origine)** | Vide le lump des faces d'origine : les décompilateurs l'utilisent, pas le jeu. Laissez activé. Clé `content.protect_strip_original_faces`. | activé |
| **Randomiseur de matériaux (casse les décompilateurs comme bsp_iidefeat)** | Renomme les matériaux de la map en noms poubelle dans la copie livrée (redirigés pour rendre pareil), pour qu'une map récupérée ait des matériaux inexploitables. Laissez activé. Clé `content.protect_randomize`. | activé |
| **Verrou serveur** | La map ne joue normalement que sur un serveur qui met `hammerpro_key` à votre clé. Ailleurs, le garde livré affiche un filigrane (et bloque si demandé). Une dissuasion, pas inviolable. Clé `content.protect_lock`. | désactivé |
| **Clé serveur** | Votre clé secrète pour le verrou (champ masqué). La clé elle-même n'est jamais écrite dans la map, seulement son empreinte. Clé `content.protect_lock_key`. | vide |
| **Bloquer le jeu sur un serveur non autorisé (pas juste un filigrane)** | Fige le joueur local et affiche un avis quand la clé serveur est absente ou fausse. Clé `content.protect_lock_block`. | désactivé |

Sans clé, le verrou ne fait rien et la fenêtre indique *Mettez une clé : vous mettez la même valeur dans votre server.cfg (hammerpro_key).*

## Ce que perd la copie protégée

- **Les entités logiques** sont retirées du lump des entités : toutes les classes qui commencent par `logic_`, `math_`, `filter_`, `game_`, `ai_`, `npc_`, `scripted_`, `phys_`, `comp_`, `trigger_`, `weapon_`, `item_`, ainsi que `point_template`, `point_servercommand`, `point_clientcommand`, `env_microphone`, `env_entity_maker`, `info_node`, `info_target`, `path_track`, `path_corner`, `func_instance`. Les entités de brush qui affichent quelque chose gardent leur géométrie.
- **Toutes les connexions entrées/sorties** sont retirées des entités qui restent.
- **Les clés d'éditeur** qui révèlent le fonctionnement de la map sont supprimées : `hammerid`, `comment`, `_comment`, `parentname`, `classname_original`.
- **Le lump des faces d'origine** est vidé (option ci-dessus).
- **Les noms de matériaux** sont brouillés (option ci-dessus). Les matériaux que le moteur reconnaît par leur nom (textures d'outil et de ciel, matériaux patchés des cubemaps sous `maps/`) ne sont jamais renommés.
- **Le maillage de navigation** est gardé pour le serveur seulement : il décrit toute la map et le client ne le lit jamais.

Le journal de construction indique *Protect your BSP : N entités logiques retirées, N connexions supprimées, taille de données de décompilation vidées* et, avec le randomiseur, *Randomiseur de matériaux : N matériaux renommés, redirigés pour rendre pareil.*

## La copie du serveur

La protection passe après l'empaquetage, les deux copies ont donc le même contenu. Le dossier de sortie reçoit un dossier `server` :

```text
<sortie>/server/README.txt
<sortie>/server/garrysmod/maps/<map>.bsp   (map complète : toutes les entités, clés et connexions)
<sortie>/server/garrysmod/maps/<map>.nav   (si la map a un maillage de navigation)
```

Copiez `server/garrysmod` dans votre serveur et installez-y aussi l'addon (le GMA ou l'objet du Workshop) : il contient le contenu de la map et ses scripts. Les deux copies ne diffèrent que par leurs entités, que le jeu ne compare pas quand un joueur rejoint : les joueurs qui ont la copie protégée peuvent donc rejoindre votre serveur. Ne redistribuez pas la copie du serveur. Le `README.txt` reprend ces étapes dans la langue de l'interface.

## Le verrou serveur

Avec **Verrou serveur** activé et une clé saisie :

1. La publication ajoute le garde `lua/autorun/hammerpro_lock.lua` à l'addon (*Le garde du verrou serveur (hammerpro_lock) est ajouté à l'addon.*).
2. La map ne stocke que l'empreinte de votre clé (`hammerpro_lock`) et s'il faut bloquer (`hammerpro_lock_block`).
3. Sur votre serveur, ajoutez cette ligne au `server.cfg`, avec la clé donnée à Hammer Pro :

```text
hammerpro_key "votre-cle-secrete"
```

Sur un serveur sans la bonne clé, le garde affiche le filigrane, et avec **Bloquer le jeu sur un serveur non autorisé** il fige aussi le joueur et affiche un avis. L'empreinte cache la clé à un simple coup d'œil dans la map ; ce n'est pas de la cryptographie, prenez donc une clé longue que personne ne peut deviner.

## Astuces

- Laissez **Casser les décompilateurs** et **Randomiseur de matériaux** activés : ils ne coûtent rien en jeu.
- Mettez votre nom ou votre communauté dans **Filigrane** : c'est la preuve qu'une copie volée est la vôtre.
- Testez la copie livrée sur un serveur local et la copie serveur sur votre serveur dédié avant de publier.

## FAQ

**La map protégée s'affiche-t-elle et a-t-elle les bonnes collisions ?** Oui. Seules la logique côté serveur et les données de décompilation sont retirées ; géométrie, éclairage, props et contenu restent.

**Puis-je protéger une map d'un autre jeu ?** Non. Construire et enregistrer en contenu crée des addons Garry's Mod ; le garde du verrou est du Lua Garry's Mod.

**J'ai perdu ma clé serveur.** Mettez une nouvelle clé, reconstruisez, et mettez la nouvelle clé dans `server.cfg`. Les joueurs ont besoin de la nouvelle publication.

**Que se passe-t-il si quelqu'un ouvre le BSP protégé dans Hammer Pro ?** **Fichier > Ouvrir une map compilée (BSP)...** signale des brushes sans faces (*elle a été protégée contre la décompilation*) ; la logique et le câblage ont disparu.

## Pages liées

- [Construire et enregistrer en contenu](build-content-release.md)
- [Outils BSP](bsp-tools.md)
- [Collecter le contenu pour une publication](../guides/collect-content-for-release.md)

---
[Retour à l'index de la documentation](../README.md)
