[Documentation](../README.md) > Fonctionnalités > Outils BSP | [English](../../en/features/bsp-tools.md) | [Русский](../../ru/features/bsp-tools.md)

# Outils BSP

Des outils pour travailler avec les maps compilées et les archives : décompiler un BSP en map éditable, regarder dans le pakfile d'un BSP, un GMA ou un VPK (et modifier le pakfile et les entités d'un BSP), exporter la géométrie de la map en OBJ pour les logiciels 3D, et comparer deux maps.

## Décompilateur de BSP

**Fichier > Décompilateur de BSP...** demande un `.bsp` et reconstruit une map éditable, dans l'esprit de BSPSource. Le travail tourne en arrière-plan (*Décompilation de map...*) et le résultat s'ouvre dans un nouvel onglet (*map décompilée en N ms (brushes : N, entités : N). Enregistrez-la en VMF pour la garder.*).

Ce qui revient :

- les brushes du monde, avec leurs axes de texture et leurs échelles de lightmap (les noms de matériaux patchés par le compilateur retrouvent leur nom d'origine) ;
- les entités de brush avec leurs brushes ; les brushes de détail deviennent des entités `func_detail`, une par groupe de brushes qui se touchent ;
- les displacements, les props statiques (`prop_static`), les overlays (`info_overlay`, avec les faces où ils sont posés), les cubemaps (`env_cubemap`), les occluders et les brushes d'areaportal ;
- les entités avec leurs sorties, et les ids tirés de `hammerid`.

Les matériaux et modèles du pakfile de la map sont copiés dans un dossier de cache monté tant que la map est ouverte, pour que son propre contenu s'affiche (*Fichiers du contenu propre à la map, affichés tant qu'elle est ouverte*).

Une map compilée ne rend pas tout : les instances sont fusionnées, les visgroups, groupes et objets cachés ont disparu, et les brushes hint, skip et origin ont été retirés par le compilateur. Les brushes que le compilateur laisse dans le fichier sans s'en servir (le volume d'un `func_occluder`, qui revient plutôt depuis les données des occulteurs) sont ignorés, et les brushes `tools/toolstrigger` inutilisés des entités `func_viscluster` reviennent chacun en un `func_viscluster`. Les overlays d'eau (lignes de rivage) reviennent en entités `info_overlay_transition` avec les bandes de la map. La console liste les brushes, displacements et occluders impossibles à reconstruire, les overlays qui ont perdu leurs faces, et les brushes sans faces (map protégée contre la décompilation, voir [Protect your BSP](bsp-protection.md)).

Le Décompilateur de BSP travaille sur un fichier `.bsp` que vous avez déjà. Pour récupérer une map depuis un serveur Garry's Mod que vous rejoignez, voir le [BSP Sniffer](bsp-sniffer.md).

## Ouvrir une archive (inspecteur d'archives)

**Fichier > Ouvrir une archive...** ouvre la fenêtre **Inspecteur d'archives** sur un `.bsp`, `.gma` ou `.vpk` (**Ouvrir...** dans la fenêtre en choisit un autre). Déposer un de ces fichiers sur la fenêtre de Hammer Pro l'y ouvre aussi.

L'en-tête décrit l'archive : pour un BSP sa **Version**, sa **Révision**, son nombre d'**Entités**, la taille du **Pakfile** et les **Lumps compressés** ; pour un GMA son **Titre**, son **Type**, ses **Étiquettes** et sa **Version** ; pour un VPK sa **Version**.

| Contrôle | Effet |
| --- | --- |
| **Filtrer les entrées...** | Filtre la liste des fichiers. |
| **Extraire...** | Extrait l'entrée sélectionnée, ou toutes les entrées listées si aucune n'est sélectionnée, dans un dossier de votre choix. |
| **Ajouter des fichiers...** (BSP) | Ajoute des fichiers au pakfile. |
| **Remplacer...** (BSP) | Remplace l'entrée sélectionnée par un fichier. |
| **Retirer** (BSP) | Retire l'entrée sélectionnée. |
| **Lump des entités** (BSP) | Affiche le lump des entités sous forme de texte modifiable (il doit commencer par `worldspawn`). |
| **Enregistrer le BSP** (BSP) | Écrit vos modifications après confirmation (**Réécrire le BSP ?**) : un nouveau fichier remplace l'ancien une fois complet. Gardez une copie si vous risquez d'avoir besoin de l'ancienne version. |
| **Abandonner** | Annule les modifications en attente (*N à ajouter, N à retirer*). |

Sélectionnez une entrée pour la prévisualiser : fichiers texte (VMT, TXT, Lua, RES, CFG, JSON, VMF, RAD, FGD, VCD...), textures VTF (taille, format, mipmaps, images et l'aperçu), images PNG, JPG, TGA et BMP, en-têtes MDL (version, textures, skins, os), fichiers WAV (canaux, fréquence, bits) et fichiers de particules PCF (leurs systèmes). Les entrées utilisées par la map ouverte s'affichent en vert (d'après la dernière analyse de la fenêtre de publication).

## Exporter en OBJ

**Fichier > Exporter en OBJ...** écrit la géométrie des brushes et des displacements de la map, avec les coordonnées de texture, dans un fichier `.obj` et sa bibliothèque de matériaux `.mtl` à côté, pour les logiciels 3D (Blender, une visionneuse de modèles...). Les faces avec une texture `tools/` sont exclues. Pour faire l'aller-retour d'une sélection avec Blender et la retrouver dans la map, utilisez plutôt le [lien avec Blender](blender.md). Les coordonnées passent du Z vers le haut de Source au Y vers le haut attendu par la plupart des logiciels 3D. La taille des textures vient du contenu du jeu quand elle est connue (512 sinon). Les fichiers sont écrits de façon atomique.

## Comparer deux maps

**Map > Comparer avec une autre map...** liste les différences entre la map ouverte et un autre VMF (une ancienne version, une sauvegarde automatique, une copie). Voir [Vérifier les problèmes](check-problems.md) pour la fenêtre **Comparer des maps**.

## MCP

`decompile_bsp`, `inspect_archive` et `export_obj` donnent les mêmes outils aux applications d'IA. Voir la [référence des outils MCP](../../MCP.md) (en anglais).

## Astuces

- Passez votre GMA de publication dans l'inspecteur d'archives pour vérifier ce que vous livrez avant de publier.
- La décompilation est idéale pour récupérer le VMF perdu d'une de vos maps ; enregistrez-le en VMF tout de suite.
- Exportez en OBJ pour faire dans un logiciel 3D un décor ou un modèle qui colle exactement à votre map.

## FAQ

**Puis-je modifier un GMA ou un VPK dans l'inspecteur ?** Non, seuls le pakfile et le lump des entités d'un BSP sont modifiables. Les GMA et VPK sont en lecture seule (extraction et aperçu).

**La map décompilée est-elle identique à l'originale ?** Non. La géométrie et les entités reviennent, mais les instances, groupes, visgroups, objets cachés et brushes propres au compilateur sont perdus.

**D'où viennent les matériaux propres d'une map décompilée ?** De son pakfile, copié dans un dossier de cache monté tant que la map est ouverte.

## Pages liées

- [BSP Sniffer](bsp-sniffer.md)
- [Protect your BSP](bsp-protection.md)
- [Construire et enregistrer en contenu](build-content-release.md)
- [Compatibilité VMF](../getting-started/vmf-compatibility.md)
- [Fichiers et dossiers](../reference/files-and-folders.md)

---
[Retour à l'index de la documentation](../README.md)
