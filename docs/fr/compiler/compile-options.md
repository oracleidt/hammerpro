[Documentation](../README.md) > Compilateur > Options de compilation | [English](../../en/compiler/compile-options.md) | [Русский](../../ru/compiler/compile-options.md)

# Options de compilation

Le compilateur intégré active `-AllowDynamicPropsAsStatic` par défaut. Les modèles physiques placés en `prop_static` restent des décors statiques au lieu d'être supprimés ; ils ne deviennent ni animés ni physiques. Décochez cette option de géométrie pour rétablir les contrôles stricts. Les fichiers de modèle absents ou invalides restent refusés.

Tous les paramètres que comprend le compilateur de Hammer Pro, avec leur effet et leur valeur par défaut. L'étape de géométrie (`hammerpro --bspc`, qui calcule aussi la visibilité) et l'étape d'éclairage (`hammerpro --rad`) ont chacune leurs paramètres.

## Où les régler

- **Les menus d'options** de la [fenêtre Compiler et lancer](compile-window.md) (et de **Paramètres > Compilation**) : **Toutes les options de géométrie et de visibilité (hammerpro --bspc)** et **Toutes les options d'éclairage (hammerpro --rad)**. Un paramètre suit le préréglage jusqu'à ce que vous le forciez (avec sa valeur) ou le désactiviez. Les choix sont enregistrés dans `settings.toml` sous `[compile.bsp_flag_choices]` et `[compile.rad_flag_choices]`, par exemple :
  ```toml
  [compile.rad_flag_choices."-bounce"]
  on = true
  value = "8"
  ```
- **Arguments supplémentaires de la géométrie** et **Arguments supplémentaires de l'éclairage** (section Avancé) : du texte libre ajouté à la ligne de commande.
- Ce sont les options du compilateur de Hammer Pro. Les options de vbsp, vvis et vrad de Valve, pour les compilateurs que vous choisissez à la place, sont listées dans [Choisir un compilateur](compilers.md).
- Les profils du **mode expert**, et la [ligne de commande](command-line.md).
- L'outil MCP `set_compile_options` (`bsp_options`, `rad_options`).

Comment ils se combinent : les paramètres du préréglage d'abord ; un paramètre de vos arguments supplémentaires remplace celui du préréglage ; un choix du menu d'options remplace les deux (désactivé, il retire le paramètre partout). La casse ne compte pas. Les lignes de commande des étapes ressemblent à ceci :

```
hammerpro --bspc -gmod <GarrysMod> [paramètres du mode] [-threads N] <vos paramètres> [contenu] -game <dossier du jeu> <map>
hammerpro --rad <-ldr|-hdr|-both> [paramètres du mode] [-threads N] <vos paramètres> [contenu] -gmod <GarrysMod> -game <dossier du jeu> <map>
```

Pour un autre jeu cible, les deux étapes reçoivent `-target <id>` au lieu de `-gmod` (voir [Jeux cibles](target-games.md)).

## Paramètres écrits par d'autres réglages

Les menus d'options les montrent sous **Écrites d'après d'autres paramètres** sans les modifier.

| Paramètre | Étape | Effet | Réglé par |
|---|---|---|---|
| `-vis` | géométrie | Visibilité complète : la map s'affiche vite en jeu. | **Visibilité : Normal** (Normal, Final) |
| `-fastvis` | géométrie | Visibilité rapide et grossière : bien plus rapide à compiler, plus lente à afficher. | **Visibilité : Rapide** (Brouillon) |
| `-leaktest` | géométrie | Arrêt quand la map fuit : aucun BSP n'est écrit, aucun temps perdu avec l'éclairage. | **Compiler quand même les maps qui fuient** décoché (par défaut) |
| `-onlyents` | géométrie | Met à jour seulement les entités du BSP précédent (quelques secondes). Géométrie, visibilité et éclairage sont conservés. | **Géométrie : Entités seules**, recompilation intelligente |
| `-onlyprops` | géométrie | Met à jour seulement les props statiques et de détail du BSP précédent. | **Géométrie : Props seuls**, recompilation intelligente |
| `-threads N` | les deux | Nombre de threads du processeur. | **Threads** (automatique : tous les cœurs, ou tous sauf un) |
| `-game <dossier>` | les deux | Dossier de jeu dont le compilateur lit les matériaux, les modèles et `lights.rad`. | la configuration du jeu |
| `-gmod <dossier>` | les deux | Dossier de Garry's Mod dont le contenu est monté. | la configuration du jeu |
| `-ldr`, `-hdr`, `-both` | éclairage | Plage standard seule (le plus rapide), haute plage seule, ou les deux (deux passes, nécessaire pour la version finale). | **Plage dynamique** |
| `-nostaticpropshadows` | éclairage | Les props statiques ne projettent aucune ombre (voir plus bas). | **Ombres des props statiques** décoché |

## Géométrie

Groupe **Géométrie** du menu (étape de géométrie).

| Paramètre | Effet | Par défaut |
|---|---|---|
| `-nodetail` | Retire toutes les brushes de détail (`func_detail`) : il reste ce qui découpe la visibilité. | désactivé |
| `-nowater` | Retire les brushes d'eau. | désactivé |
| `-micro <volume>` | Les brushes plus petites que ce volume donnent un avertissement microbrush. | 1 |
| `-nodrawtriggers` | Les textures de trigger ne sont pas dessinées du tout (map plus petite). | désactivé |
| `-novirtualmesh` | Collision des displacements en soupes de triangles au lieu du maillage de terrain virtuel. | désactivé |
| `-replacematerials` | Remplace les matériaux comme indiqué dans `cfg/materialsub.cfg` du jeu (son bloc `AllMaps`, puis celui de la map). | désactivé |
| `-AllowDynamicPropsAsStatic` | Garde les `prop_static` dont le modèle a des propriétés physiques (`prop_data`) au lieu de les supprimer. | désactivé |
| `-dxlevel <niveau>` | Retire les entités dont `mindxlevel`/`maxdxlevel` excluent ce niveau DirectX. | non utilisé |
| `-blocksize <unités>` | Taille des blocs dans lesquels le monde est découpé avant que ses brushes le divisent : 0 (aucun bloc) ou de 128 à 32768. Des blocs plus grands ou 0 découpent moins de faces et de feuilles (moins de plans et de sommets). | 1024 |

## Visibilité

Groupe **Visibilité** (étape de géométrie). Sans `-vis` ni `-fastvis`, aucune visibilité n'est calculée : chaque partie de la map voit toutes les autres.

| Paramètre | Effet | Par défaut |
|---|---|---|
| `-fulldetail` | Les brushes de détail deviennent normales : elles découpent aussi la visibilité (vis plus lente). | désactivé |
| `-forceskyvis` | Calcule aussi la visibilité dans la skybox 3D (un seul cluster sinon). | désactivé |
| `-radius_override <unités>` | Rayon de visibilité : les clusters plus éloignés ne se voient jamais, quel que soit le brouillard. | le `farz` du premier `env_fog_controller`, sinon aucun |
| `-nosort` | Visibilité sans trier les portails d'abord (même résultat). | désactivé |

## Lightmaps

Groupe **Lightmaps** (étape de géométrie : la taille des lightmaps se décide au découpage des faces).

| Paramètre | Effet | Par défaut |
|---|---|---|
| `-luxelscale <facteur>` | Multiplie chaque échelle de lightmap par ce facteur (2 divise la résolution par deux). | 1 |
| `-minluxelscale <échelle>` | Aucune échelle de lightmap sous cette valeur. | 1 |
| `-defaultluxelsize <échelle>` | Échelle de lightmap des faces dont l'échelle vaut 0. | 16 |
| `-maxlightmapdim <luxels>` | Plus grande lightmap d'une face en luxels : les grandes faces sont moins découpées. Garry's Mod accepte jusqu'à 125, les autres jeux 32. | 32 |
| `-bumpall` | Toutes les surfaces éclairées reçoivent des lightmaps bumpées (données d'éclairage plus grosses). | désactivé |
| `-lightifmissing` | Les surfaces aux matériaux manquants ou non éclairés reçoivent quand même une lightmap. | désactivé |

## Montage du contenu

Groupe **Contenu** (étape de géométrie ; l'étape d'éclairage lit les mêmes paramètres de montage pour les modèles).

| Paramètre | Effet | Par défaut |
|---|---|---|
| `-noaddons` | Ne monte pas les addons en dossier de `garrysmod/addons`. | addons montés |
| `-wsaddons` | Monte aussi les objets du Workshop abonnés (le VBSP de Valve ne le fait pas). | désactivé en ligne de commande ; l'éditeur l'ajoute quand il affiche du contenu du Workshop |
| `-nowsaddons` | Ne monte ni les addons en dossier ni les objets du Workshop. | désactivé |
| `-serveraddons` | Monte aussi le contenu téléchargé depuis les serveurs (`garrysmod/download`, `garrysmod/cache/workshop` ; le VBSP de Valve ne le fait pas). | désactivé en ligne de commande ; l'éditeur l'ajoute quand il affiche ce contenu |
| `-keepstalezip` | Garde les fichiers empaquetés dans le BSP précédent (le reste est reconstruit). | désactivé |
| `-embed <dossier>` | Dossier ajouté au contenu et empaqueté en entier dans le BSP. Compilations complètes seulement. | non utilisé |

La compilation de l'éditeur transmet les sources de contenu que l'éditeur affiche, donc un matériau visible dans l'éditeur n'est jamais compilé comme manquant. Vos dossiers de textures personnalisés sont toujours visibles par le compilateur : les matériaux et les modèles personnalisés utilisés par la map sont copiés pour lui dans un dossier de jeu généré, donc un `prop_static` avec un modèle d'un dossier personnalisé garde son prop.

## Qualité de l'éclairage

Groupe **Qualité** (étape d'éclairage).

| Paramètre | Effet | Par défaut |
|---|---|---|
| `-final` | Ciel et éclairage indirect de haute qualité (plus de rayons) : lent. Augmente aussi les rayons du ciel et des rebonds des props statiques. | préréglage Final |
| `-bounce <n>` | Nombre maximal de rebonds de la lumière : 1 est bien plus rapide. | 100 (Brouillon : 1) |
| `-noextra` | Pas de suréchantillonnage : bords d'ombre plus durs et pixelisés, plus rapide. | désactivé |
| `-extrasky <facteur>` | Trace ce nombre de fois plus de rayons du ciel et de lumière indirecte (ambiance plus douce, plus lent). | non utilisé |
| `-smooth <degrés>` | Les faces qui se rejoignent sous cet angle ont un ombrage lissé sur leur arête. | 45 |
| `-softsun <degrés>` | Traite le soleil comme une source étendue de cette largeur : bords d'ombre plus doux (0 à 5 en général). | 0 |
| `-centersamples` | Accepté comme par VRAD : les luxels sont toujours échantillonnés en leur milieu quand il est dégagé. | toujours |
| `-nodispshadows` | Option de Hammer Pro : les displacements ne projettent aucune ombre (ils restent éclairés). Un seul displacement peut aussi ne pas projeter d'ombre avec l'option **Projeter des ombres** de l'[outil displacement](../features/displacement-tool.md). | désactivé |
| `-nostylebounce` | Option de Hammer Pro : les lumières nommées (commutables) n'éclairent que ce qu'elles voient directement, comme le VRAD de Valve. Sans elle, leur lumière rebondit aussi, dans leur propre style de lumière, et s'allume et s'éteint avec elles. Leur lumière directe garde toujours sa place parmi les 4 styles d'une face : une couche de rebond ne prend qu'une place libre. | désactivé |

<a id="ambient-occlusion"></a>
## Occlusion ambiante

Groupe de menu **Occlusion ambiante** (étape d'éclairage). Occlusion ambiante précalculée comme dans VRAD++ : les coins, les recoins, le pied des murs et le sol sous les caisses s'assombrissent dans les lightmaps. Elle est désactivée par défaut, dans tous les préréglages (Final compris) : activez-la avec `-ao` (ou `-ambientocclusion`, comme dans VRAD++). Les aperçus de l'éditeur (Éclairé, Précalculé) ne la montrent jamais ; la map compilée oui, en jeu et dans la [vue joueur](../features/player-view.md).

| Option | Effet | Par défaut |
|---|---|---|
| `-ao` | Calcule l'occlusion ambiante. `-ambientocclusion` fait la même chose ; `-ao 0` et `-noao` la désactivent. | désactivé |
| `-aoradius <unités>` | Distance jusqu'à laquelle la géométrie assombrit une surface. Dans la skybox 3D elle est divisée par l'échelle du `sky_camera`, pour que la skybox ait la même occlusion que le monde en taille réelle. | 32 |
| `-aoscale <0 à 1>` | Force : 0 ne change rien, 1 laisse les luxels entièrement enfermés devenir noirs. Les valeurs hors limites sont ramenées dans l'intervalle avec un avertissement. | 0,5 |
| `-aofacesamples <rayons>` | Rayons par luxel. | 32, 16 avec `-fast`, 64 avec `-final` |
| `-aopropsamples <rayons>` | Rayons par sommet de prop statique, avec `-StaticPropLighting`. | 32, 16 avec `-fast`, 64 avec `-final` |
| `-aodebug` | Les lightmaps ne montrent que le terme d'occlusion (blanc là où c'est dégagé, sombre dans les coins) au lieu de la lumière, les sommets des props précalculés aussi. Active `-ao`. | désactivé |

Les options de rayon, d'échelle et de rayons ne font rien sans `-ao` (le journal le dit). Un nombre de rayons donné avec `-aofacesamples` ou `-aopropsamples` est utilisé tel quel (1 à 1024) ; seules les valeurs par défaut suivent `-fast` et `-final`. Les anciennes `-aostrength` et `-aosamples` sont lues comme `-aoscale` et `-aofacesamples`.

### Le calcul

1. Pour chaque luxel, depuis le point qu'utilise la lumière directe (1 unité au-dessus de la surface), N rayons (`-aofacesamples`) partent dans un hémisphère pondéré par le cosinus autour de la normale (la normale lissée sur les displacements) et s'arrêtent au rayon R. Ils touchent les brushes du monde, les displacements et les props statiques ; les faces de ciel, l'eau, le verre et les feuillages à alpha testé n'occultent pas.
2. Chaque rayon qui touche à la distance t occulte avec une atténuation douce : `AO = 1 - (1/N) x somme de (1 - (t / R)^2)` sur les rayons qui touchent. AO vaut 1 dans un endroit dégagé et baisse près de la géométrie.
3. L'AO de chaque luxel est moyennée avec les luxels voisins (à moins d'un luxel et demi, tournés du même côté, à travers les faces et les displacements) : pas de bruit, pas de raccord visible entre faces, pages de lightmap ou displacements.
4. Comme dans le VRAD de CS:GO et VRAD++, l'occlusion multiplie la lightmap finale, ramenée vers 1 par l'échelle : `lightmap = (directe + rebonds) x (1 - échelle x (1 - AO))`. Elle s'applique à chaque style de lumière, au LDR et au HDR et aux directions du bump map, après les rebonds (l'occlusion n'assombrit pas la lumière qui rebondit), avant la mise en accord des luxels partagés par des faces voisines.
5. La lumière ambiante des props et des joueurs (cubes ambiants des leafs) et les props de détail prennent leur lumière dans les lightmaps occultées. Avec `-StaticPropLighting`, chaque sommet de prop reçoit le même facteur avec `-aopropsamples` rayons (les props qui ne s'ombrent pas eux-mêmes, `-nossprops`, ne sont pas occultés par leur propre modèle).
6. Avec `-aodebug`, les lightmaps contiennent `128 x AO^2.2` dans leur premier style de lumière (les autres sont noirs) : le gris vu en jeu suit AO, les sols dégagés sont blancs, les coins gris.

Le coût : l'étape AO (`ambient occlusion` dans la liste des étapes et dans `Light phase times`) trace des rayons courts dans le BVH que l'éclairage a déjà construit. Sur les maps de test elle ajoute environ 10 % au temps d'éclairage : 2 secondes pour les 790 000 luxels de gm_construct sur 4 threads (32 rayons par luxel, 22 secondes d'éclairage sans elle), 30 à 200 ms sur les petites maps.

## Props statiques

Groupe **Props statiques** (étape d'éclairage).

| Paramètre | Effet | Par défaut |
|---|---|---|
| `-StaticPropLighting` | Précalcule l'éclairage de chaque sommet des props statiques (ombres et lumière rebondie) au lieu de laisser le jeu les éclairer en direct. | préréglage Final, si **Éclairer les props statiques (par sommet)** est coché |
| `-StaticPropPolys` | Les props statiques projettent leur ombre avec les triangles affichés au lieu de leur modèle de collision (ombres plus fines, plus lent). | désactivé |
| `-textureshadows` | Avec `-StaticPropPolys` : les textures à transparence (feuillages, grillages) des modèles compilés avec `$casttextureshadows` laissent passer la lumière. | désactivé |
| `-nossprops` | Les props statiques ne se font jamais d'ombre à eux-mêmes. | désactivé |
| `-staticpropsamplescale <facteur>` | Multiplie le nombre de rayons qui éclairent les sommets des props statiques (avec `-StaticPropLighting`). | 1 (4 avec `-final`) |
| `-nostaticpropshadows` | Option de Hammer Pro : les props statiques ne projettent aucune ombre (lightmaps, lumière rebondie, props de détail, lumière ambiante, éclairage des props) ; leurs modèles ne sont même pas chargés. Écrit par **Ombres des props statiques**. | désactivé |

## Lumières de texture et ciel

Groupe **Lumières de texture et ciel** (étape d'éclairage).

| Paramètre | Effet | Par défaut |
|---|---|---|
| `-lights <fichier>` | Fichier de lumières de texture en plus, lu après `lights.rad` et avant les fichiers propres à la map. | non utilisé |
| `-chop <luxels>` | Plus petit morceau de lumière de texture (au moins 1). | 4 |
| `-maxchop <luxels>` | Plus grand morceau de lumière de texture (au moins 1). | 4 |
| `-dispchop <luxels>` | Taille des patchs sur les displacements pour les lumières de texture et la lumière rebondie (au moins 1). | 8 |
| `-noskyboxrecurse` | La skybox 3D ne projette aucune ombre sur la map et n'en reçoit pas de lumière. | désactivé |

## Vitesse

Groupe **Vitesse**.

| Paramètre | Étape | Effet | Par défaut |
|---|---|---|---|
| `-low` | les deux | Tourne en priorité basse : l'ordinateur reste réactif. | désactivé |
| `-fast` | éclairage | Aperçu rapide : peu de rayons, pas de suréchantillonnage, un échantillon d'ambiance par feuille, pas d'éclairage par sommet des props, bien plus rapide. | préréglage Brouillon |
| `-fastambient` | éclairage | Lumière ambiante des props et des objets dynamiques plus rapide et plus grossière. | désactivé |
| `-luxeldensity <facteur>` | éclairage | Multiplie la résolution de chaque lightmap par ce facteur, inférieur à 1 seulement (éclairage plus grossier et plus rapide). | 1 |

**Garder un cœur pour l'éditeur** n'est pas `-low` : il baisse d'un le nombre automatique de threads.

## Débogage

Groupe **Débogage**.

| Paramètre | Étape | Effet |
|---|---|---|
| `-nomerge` | géométrie | Ne refusionne pas les faces découpées par le BSP. |
| `-nosubdiv` | géométrie | Ne découpe pas les faces pour leurs lightmaps (les grandes faces échouent). |
| `-notjunc` | géométrie | Ne corrige pas les jonctions en T (de petites fissures peuvent apparaître). |
| `-noprune` | géométrie | Ne fusionne pas les nœuds solides de l'arbre BSP. |
| `-propdebug` | géométrie | Liste dans le journal chaque prop dont le modèle pose problème (modèle manquant ou cassé, pas de modèle de collision, skin ou groupe de corps invalide, hors du monde, pas un prop statique, trop de sommets pour l'éclairage des props), avec son entité, son modèle et sa position. |
| `-trace <cluster> <cluster>` | géométrie | Écrit la ligne de vue entre deux clusters dans `<map>.lin`. |
| `-verbose` | géométrie | Affiche les options de la compilation. |
| `-StaticPropNormals` | éclairage | Les props statiques affichent les normales de leurs sommets au lieu de leur éclairage. |
| `-aodebug` | éclairage | Les lightmaps ne montrent que le terme d'occlusion ambiante (groupe de menu **Occlusion ambiante**, voir [plus haut](#ambient-occlusion)). |

## FAQ

**Quels paramètres changent le plus la durée de compilation ?** `-fastvis` contre `-vis`, `-fast` et `-bounce 1` pour l'éclairage, `-final`, `-StaticPropLighting` et `-nostaticpropshadows` sur les maps avec beaucoup de props. Voir [Performances de compilation](performance.md).

**J'ai tapé un paramètre dans les arguments supplémentaires et il ne fait rien.** Regardez le journal : un paramètre inconnu donne un avertissement. Vérifiez aussi que le menu d'options ne le désactive pas.

**Puis-je utiliser `-StaticPropLighting` en Normal ?** Oui : forcez-le dans le menu des options d'éclairage (il faut quand même que **Éclairer les props statiques (par sommet)** soit coché).

## Pages liées
- [Ligne de commande](command-line.md)
- [Fenêtre Compiler et lancer](compile-window.md)
- [Clés worldspawn et lumières](worldspawn-keys.md)
- [Éclairage](../guides/lighting.md)
- [Performances de compilation](performance.md)

---
[Retour à l'index de la documentation](../README.md)
