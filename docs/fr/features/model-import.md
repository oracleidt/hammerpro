[Documentation](../README.md) > Fonctionnalités > Import de modèles | [English](../../en/features/model-import.md) | [Русский](../../ru/features/model-import.md)

# Import de modèles

L'import de modèles transforme un fichier 3D (FBX, OBJ, glTF ou GLB) en prop Garry's Mod sans Crowbar ni les compilateurs de Valve : Hammer Pro convertit ses textures en VTF et VMT, compile le modèle avec son compilateur intégré, en fait le modèle des nouveaux props et peut en poser un devant la caméra.

## Ouvrir l'import

- **Fichier > Importer un modèle (FBX, OBJ, glTF)...** ouvre une boîte de dialogue de fichier (filtre « Modèles 3D » : `.fbx`, `.obj`, `.gltf`, `.glb`).
- Le bouton **+** du [navigateur de modèles](model-browser.md) fait la même chose. Son bouton **++** ouvre l'[import par lot](#import-par-lot).
- **Fichier > Importer des modèles (lot)...** convertit beaucoup de fichiers d'un coup.
- Glissez-déposez un fichier `.fbx`, `.obj`, `.gltf` ou `.glb` sur la fenêtre de Hammer Pro. Un modèle à la fois : un second dépôt pendant un import est refusé avec un message. Déposer plusieurs fichiers de modèle d'un coup ouvre à la place l'[import par lot](#import-par-lot) avec eux.
- Les fichiers `--script` ont `import_model CHEMIN`, `import_model_now CHEMIN` (importe avec les options par défaut) et `import_model_scaled ÉCHELLE CHEMIN` (voir [Commandes de script](../reference/script-commands.md)).

Le fichier est lu en arrière-plan (« Lecture de ... »), puis la fenêtre **Importer un modèle** affiche ses options.

## Fichiers acceptés

| Format | Ce qui est lu |
|---|---|
| Wavefront OBJ (`.obj` avec son `.mtl`) | Maillages et matériaux. Les faces sans normales reçoivent des normales lissées par groupe de lissage (`s off` donne du plat), ou à travers les arêtes pas trop vives quand le fichier n'a pas de groupe de lissage. |
| glTF 2.0 (`.gltf`, `.glb`) | Les maillages de la scène par défaut avec les transformations de leurs nœuds (maillages skinnés en pose de référence) ; les matériaux (texture de couleur de base avec son jeu de coordonnées et `KHR_texture_transform`, facteur de couleur, mode d'alpha, double face, et les valeurs de normal map, de métal et rugosité, d'émissif des « Paramètres des matériaux »). Les maillages compressés Draco ou meshopt sont refusés avec un message. |
| FBX 7.x, binaire et ASCII | Les maillages placés par leurs modèles (translation, rotation et son ordre, pivots, décalages, échelle, transformation géométrique, parents), les matériaux (couleur, texture diffuse en fichier ou intégrée, textures de normales, de bump et émissive, valeurs de spéculaire et d'émissif), le système d'axes et l'unité du fichier. |

Seule la géométrie statique est importée : les animations sont ignorées. Les textures peuvent être en PNG, JPG, TGA ou BMP.

## Options

La fenêtre affiche d'abord le chemin du fichier et un résumé : triangles, matériaux et taille en unités. Des avertissements apparaissent au-delà de 65000 triangles (un prop lourd), et pour un modèle de moins de 2 unités ou de plus de 4096 unités (vérifiez l'échelle).

| Option | Effet | Défaut |
|---|---|---|
| **Nom** | `models/hammerpro/<nom>.mdl`. Nettoyé en lettres minuscules, chiffres et `_` (48 caractères au plus). Un avertissement signale qu'un modèle de ce nom existe : il est remplacé. | Le nom du fichier |
| **Type** | **prop_static**, ou **Prop physique** (`prop_physics`). | prop_static |
| **Échelle** | Échelle supplémentaire après la conversion d'unités (0,001 à 1000). | 1 |
| **Convertir les unités** | Les mètres du fichier deviennent des unités Source (pouces) : 1 m = 39,37 unités. Désactivé : une unité du fichier vaut une unité Source. | Activé |
| **Axe vertical** | **Celui du fichier**, **Y** ou **Z**. Les modèles en Y vertical sont tournés pour que Z soit vertical, l'avant restant devant. | Celui du fichier |
| **Origine** - **En bas au centre** | Met l'origine du modèle au centre de sa base. | Activé |
| **Propriété de surface** | Matériau physique et sonore : `default`, `concrete`, `metal`, `solidmetal`, `metal_box`, `metalpanel`, `wood`, `wood_crate`, `wood_furniture`, `plastic`, `plastic_barrel`, `glass`, `rock`, `dirt`, `grass`, `gravel`, `brick`, `tile`, `carpet`, `cardboard`, `paper`, `rubber`, `flesh`, `ice`, `porcelain`, `computer`, `pottery`. | default |
| **Collision** | **Morceaux** : chaque morceau séparé du modèle reçoit une enveloppe convexe. **Boîte** : la boîte du modèle. **Aucune** : pas de modèle de collision. | Morceaux |
| **Masse** (prop physique seulement) | 0,1 à 50000 kg. | 20 kg |
| **Niveaux de détail (LOD)** | Versions simplifiées du modèle que le jeu affiche de loin. Partagé avec [Propper](propper.md) et enregistré (`[lab.model_lod]`). | Activé |
| **Niveaux** | Niveaux sous le modèle complet, de 3 à 6 (les gros modèles en ont un ou deux de plus). | 4 |
| **Chacun garde** | Part des triangles du niveau précédent que garde chaque niveau, de 20 à 90 % (les petits modèles sont réduits plus doucement). | 50 % |
| **Garder les paramètres des matériaux** | Écrit plus que la couleur de base dans les VMT : normal map, brillant, auto-illumination, tiling et décalage des textures (voir « Paramètres des matériaux » plus bas). Désactivé : couleur, alpha et double face seulement, et le tiling est cuit dans les UV. Enregistré avec les réglages (`lab.model_keep_params`, aussi accessible avec le `set_setting` MCP). | Activé |
| **Le placer devant la caméra** | Après l'import, pose un prop du modèle là où regarde la caméra 3D. | Activé |

Un prop physique sans collision affiche un avertissement : il ne peut pas bouger en jeu.

**Matériaux** liste les matériaux utilisés par le modèle avec leur texture : le nom du fichier, « texture dans le fichier » (intégrée), ou « pas de texture (sa couleur) ».

Les options autres que le nom et les réglages de LOD restent comme vous les avez laissées jusqu'à la fermeture de Hammer Pro. **Importer** est grisé tant que le nom est vide.

## Ce qui est écrit

Le modèle va dans le dossier du jeu du contenu monté, ses textures à côté :

- `materials/models/hammerpro/<nom>/` : un VTF (avec mipmaps) et un VMT `VertexLitGeneric` par matériau utilisé. Un matériau sans texture (ou dont la texture est illisible) reçoit une petite texture de sa couleur, avec un avertissement dans la console. Les matériaux double face reçoivent `$nocull` ; l'alpha devient `$alphatest` ou `$translucent` (selon le mode d'alpha du matériau, ou selon l'alpha de la texture quand le fichier ne le précise pas) ; un matériau texturé teinté reçoit `$color2`, une couleur transparente `$alpha`.
- Avec **Garder les paramètres des matériaux** : les fichiers et clés en plus décrits dans « Paramètres des matériaux » plus bas.
- `models/hammerpro/<nom>.mdl` avec ses `.vvd`, `.dx90.vtx` et `.phy` (pas de `.phy` avec la collision **Aucune**). Les anciens fichiers `.dx80.vtx`, `.sw.vtx`, `.vtx` et `.ani` du même nom sont supprimés. Le MDL est écrit en dernier, pour que le jeu ne voie jamais un modèle à moitié écrit.

Le compilateur intégré produit ce que studiomdl fait d'un prop statique : un os, une séquence idle, un maillage par matériau, les niveaux de détail, et un LOD d'ombre pour les props physiques. Un modèle de plus de 65536 sommets est découpé en plusieurs parties (la console indique combien). Les enveloppes de collision sont réduites de la marge de collision comme celles de studiomdl ; les morceaux plats sont épaissis à une unité.

**Construire et enregistrer en contenu** embarque ces fichiers avec la map (voir [Construire et publier le contenu](build-content-release.md)).

## Paramètres des matériaux

Avec **Garder les paramètres des matériaux** (activé par défaut), chaque matériau reçoit aussi ce que son fichier dit en plus de la couleur de base :

| Source dans le fichier | Écrit dans le VMT |
|---|---|
| Normal map : `normalTexture` glTF (canal vert inversé vers la convention Source), `norm` OBJ, `NormalMap` FBX | `$bumpmap` avec une normal map `<matériau>_n.vtf`. `map_Bump` ou `bump` OBJ et `Bump` FBX sont des normal maps quand l'image est bleutée, des cartes de hauteur sinon (converties en normal map). |
| `metallicFactor`, `roughnessFactor` et `metallicRoughnessTexture` glTF (sa moyenne, multipliée par les facteurs), `KHR_materials_pbrSpecularGlossiness` ; `Pr`, `Pm` OBJ ; `Ks` OBJ avec `Ns` ; `SpecularColor` FBX avec `ShininessExponent` | `$phong 1` avec `$phongexponent` (d'après le brillant), `$phongboost`, `$phongfresnelranges`, et `$phongalbedotint 1` pour les métaux. Une normal map plate est ajoutée quand le fichier n'en a pas, car le phong exige un `$bumpmap`. Un matériau mat (rugosité au-dessus de 0,8) et un spéculaire faible (`Ks` OBJ sous 0,55 ou `Ns` sous 50, ce qui écarte la valeur par défaut de la plupart des exporteurs) n'ont pas de phong. |
| Un métal lisse (métallique 0,6 ou plus et rugosité sous 0,6) | `$envmap env_cubemap` avec `$envmaptint` d'après métallique fois brillant. |
| `emissiveFactor`, `emissiveTexture` et `KHR_materials_emissive_strength` glTF ; `Ke`, `map_Ke` OBJ ; `EmissiveColor` FBX | `$selfillum 1` avec `$selfillummask` (`<matériau>_illum.vtf`, la texture émissive ou une blanche) et `$selfillumtint` pour la couleur. |
| `KHR_texture_transform` glTF (échelle et décalage), `map_Kd -s -o` OBJ, `Scaling` et `Translation` de la texture FBX | `$basetexturetransform`, et `$bumptransform` avec une normal map. Une rotation glTF reste cuite dans les UV. |

La console liste, pour chaque matériau, ce qui a été converti et ce qui a été ignoré (une texture d'occlusion, une texture de spéculaire ou de brillance, une carte de hauteur, un fichier de texture introuvable, les extensions glTF comme `KHR_materials_clearcoat`, qui sont nommées). Les textures ne sont jamais mélangées ni réempaquetées : seules les cartes ci-dessus sont lues.

## Après l'import

- Le contenu est remonté et une notification annonce « *modèle* est prêt (N textures, N morceaux de collision) : c'est le modèle des nouveaux props (outil Entité) ». La console liste les niveaux de détail avec leur nombre de triangles.
- Le modèle devient le modèle des nouveaux props, avec la classe `prop_static` ou `prop_physics`, pour l'[outil entité](entity-tool.md).
- Avec **Le placer devant la caméra**, un prop est créé là où regarde la caméra 3D : posé sur la surface touchée (son origine est au centre de sa base), sinon en l'air devant la caméra. Il devient la sélection (une seule étape d'annulation).

## Import par lot

**Fichier > Importer des modèles (lot)...** (aussi le bouton **++** du [navigateur de modèles](model-browser.md), et `import_models_batch` dans la palette de commandes) convertit beaucoup de fichiers d'un coup, avec le même pipeline et les mêmes options qu'un import seul.

1. **Ajouter des fichiers...** choisit plusieurs fichiers de modèle, **Ajouter un dossier...** choisit des dossiers. **Inclure les sous-dossiers** les parcourt (24 niveaux au plus), et les cases **Formats** (OBJ, FBX, glTF / GLB) filtrent ce qui est pris dans les dossiers. Les fichiers choisis à la main sont gardés quel que soit leur format coché tant que l'extension est prise en charge. 5000 fichiers au plus sont pris, par ordre alphabétique, sans doublons.
2. Le **tableau d'aperçu** lit chaque fichier en arrière-plan et liste ses triangles, sa taille en unités, le nom de modèle qu'il recevra et ses avertissements (modèle lourd, trop petit ou trop grand, prop physique sans collision, fichier illisible en rouge). Un nom en orange signifie qu'un modèle de ce nom existe déjà. **Aperçu** relit les fichiers après un changement d'échelle, d'unités ou d'axe.
3. **Modèle de nom** construit chaque nom de modèle : `{name}` est le nom de fichier nettoyé, `{folder}` le nom de son dossier, `{index}` son numéro (001, 002...). Le **Préfixe** est placé devant. Le résultat est nettoyé comme le nom d'un import seul (lettres minuscules, chiffres et `_`, 48 caractères au plus) ; deux fichiers qui donnent le même nom reçoivent `_2`, `_3`... pour qu'aucun n'écrase un autre.
4. Les options sont celles de l'import seul : type (`prop_static` ou prop physique avec sa masse), échelle, **Convertir les unités**, axe vertical, origine en bas au centre, propriété de surface, collision, plus les réglages enregistrés **Niveaux de détail (LOD)** et **Garder les paramètres des matériaux**.
5. **Remplacer les modèles qui existent** est désactivé par défaut : un modèle dont le nom existe dans `models/hammerpro` est ignoré et signalé. **Conversions en parallèle** fixe le nombre de fichiers convertis en même temps (1 à 16 ; par défaut la moitié des threads du processeur, entre 1 et 4, car les textures et maillages prennent de la mémoire).
6. **Placer un prop par modèle dans une grille** met un prop (`prop_static` ou `prop_physics`) par modèle converti devant la caméra 3D : la grille commence là où la caméra regarde, a le nombre de **Colonnes** choisi, et l'**Écart** entre les cases (chaque case est aussi grande que le plus grand modèle). Les props reposent sur le niveau du sol de ce point et sont sélectionnés, en une seule étape d'annulation (« Import de modèles »).
7. **Convertir N modèles** lance l'exécution. Une barre de progression montre les fichiers faits ; **Annuler** s'arrête après les fichiers en cours de conversion (le reste est signalé comme non traité). La colonne **Résultat** et la console listent chaque fichier : OK, Avertissement, Erreur ou Ignoré avec la raison, puis une ligne de synthèse : fichiers, ok, avec avertissements, erreurs, ignorés, non traités, et la durée.

Un fichier illisible ou impossible à compiler est une erreur pour ce fichier seulement ; l'exécution continue. Le contenu du jeu est monté à nouveau à la fin pour que les nouveaux modèles soient dans le navigateur de modèles.

### En ligne de commande

```text
hammerpro --import-models FICHIER_OU_DOSSIER... --out DOSSIER [options]
```

Il n'a besoin ni de fenêtre ni de jeu : les modèles vont dans `DOSSIER/models/hammerpro/` et les textures dans `DOSSIER/materials/models/hammerpro/<nom>/`, un dossier que vous pouvez copier dans le jeu ou embarquer. Voir [Ligne de commande](../reference/command-line.md) pour toutes les options. Par MCP l'outil est `import_models_batch`.

## Astuces

- Si la taille du résumé paraît 39 fois trop grande ou trop petite, basculez **Convertir les unités** ou changez l'**Échelle**.
- Restez sous 32 matériaux : un modèle du jeu ne peut pas en avoir plus et l'import s'arrête avec un message.
- Pour vérifier le résultat, sélectionnez le modèle dans le navigateur de modèles : l'onglet **Inspecteur** liste ses fichiers et matériaux.
- Pour transformer un modèle de terrain (OBJ, FBX, glTF, GLB, ou un prop déjà dans la map) en displacements plutôt qu'en prop, utilisez [Maillage en displacement](displacement-tool.md#maillage-en-displacement) : il lit les mêmes fichiers, avec les mêmes options d'unités et d'axe haut, et construit une grille de dalles de displacement.

## FAQ

**« Le contenu du jeu n'est pas chargé » : que faire ?** L'import écrit dans le dossier du jeu du contenu monté. Indiquez le dossier du jeu dans les paramètres et attendez la fin du chargement (voir [Configuration du jeu](../getting-started/game-setup.md)).

**« Le modèle est trop gros pour le jeu ».** Réduisez ses détails dans votre logiciel 3D et importez-le à nouveau.

**Mon glTF est refusé.** Les maillages compressés Draco et meshopt ne sont pas lus : exportez-le sans compression de maillage.

**Puis-je importer des animations ou des squelettes ?** Non : seule la géométrie statique est lue.

## Pages liées

- [Navigateur de modèles](model-browser.md)
- [Lien avec Blender](blender.md)
- [Outil displacement : Maillage en displacement](displacement-tool.md#maillage-en-displacement)
- [Propper](propper.md)
- [Outil entité](entity-tool.md)
- [Construire et publier le contenu](build-content-release.md)
- [Dossiers de textures personnalisés](custom-texture-folders.md)
- [Commandes de script](../reference/script-commands.md)

---
[Retour à l'index de la documentation](../README.md)
