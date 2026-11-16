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
| `-raytracing`, `-rtquality`, `-rt...` | éclairage | Le ray tracing et chacun de ses paramètres (voir [Ray tracing](#ray-tracing)). Seulement pour le compilateur de Hammer Pro : le vrad de Valve ne les reçoit jamais. | ligne **Ray tracing** et options **Ray tracing** |

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

## Optimisation

Groupe de menu **Optimisation** (étape géométrie). Toutes ces options sont désactivées par défaut, dans tous les préréglages : activez celles que vous voulez. Elles changent la façon dont la map est stockée, pas son rendu ni son jeu, et peuvent être combinées.

| Option | Effet | Défaut |
|---|---|---|
| `-cullverts` | Moins de sommets (MAX_MAP_VERTS) : les faces ne sont découpées pour leurs lightmaps que là où la limite de taille de lightmap du jeu l'exige (125 luxels dans Garry's Mod au lieu de 32, donc les mêmes luxels avec moins de raccords), les sommets sur des arêtes droites dont aucune face n'a besoin sont retirés, les faces d'origine (un lump que le jeu ne lit pas) réutilisent les sommets des faces dessinées, les sommets inutilisés disparaissent et les entités de brushes partagent leurs sommets avec le monde. Les faces de plus de 32 sommets sont découpées le long de cordes qui ne laissent aucune jonction en T. | désactivé |
| `-culltjunc` | Les sommets de jonction en T ne sont ajoutés que là où une autre face dessinée continue vraiment le long de la même arête : aucun ne vient des faces nodraw, skip ou hint, les faces jamais dessinées ne sont pas corrigées, et la vérification recommence jusqu'à ce qu'il ne reste aucune fissure. Moins d'arêtes et d'arêtes de surface. | désactivé |
| `-cullbrushes` | Laisse de côté les brushes qui ne changent rien : doublons exacts et brushes à l'intérieur d'un autre brush solide (même contenu, même texture sur les côtés communs), brushes plats, brushes solides structurels qu'aucun espace jouable ne touche (enfouis dans d'autres brushes ou dans le vide, jamais sur une map qui fuit, jamais un brush qui contient une entité), et fusionne les boîtes nodraw côte à côte de même contenu et même texture en une seule. Moins de brushes, de côtés de brushes et de brushes de feuilles ; collision, étanchéité, éclairage et rendu restent identiques. Les brushes avec des déplacements, des overlays ou un contenu clip ne sont jamais touchés. | désactivé |
| `-autolightmapscale` | Échelle de lightmap automatique. Les faces jamais vues de près reçoivent une échelle plus grossière selon leur distance à la zone praticable la plus proche devant elles (sols et displacements, plus la hauteur d'un joueur) : 16 jusqu'à 256 unités, 32 jusqu'à 1024, 64 jusqu'à 2048, 128 au-delà (plafonds hauts, dessous des toits, faces loin au-dessus ou à l'écart de là où les joueurs marchent). Les faces de la skybox 3D (la coque de ciel autour de `sky_camera`) reçoivent 32. Les petites faces à moins de 64 unités d'une zone praticable et de 256 unités d'une lumière reçoivent 8, seul changement plus fin. Seules les faces du monde et des `func_detail` laissées à l'échelle par défaut (16) changent : une échelle réglée par le mappeur est gardée. Le journal donne la mémoire de lightmap estimée avant et après. | désactivé |
| `-cleanpak` | Retire du pakfile les matériaux, textures, modèles, sons et fichiers de particules auxquels rien ne fait référence (les entités, les faces, les props statiques, et les matériaux, modèles, scripts, fichiers Lua et manifestes de particules gardés dans le pakfile), puis compresse le pakfile en LZMA (Garry's Mod le lit ; les sons restent non compressés comme le moteur l'exige). Les cubemaps de la map (`materials/maps/<map>/`), les matériaux de skybox et tous les autres types de fichiers sont toujours gardés. Il s'exécute à la fin de l'étape de géométrie et de nouveau après l'étape d'empaquetage. Construisez les cubemaps en jeu avant : le moteur ne construit les cubemaps que dans un pakfile non compressé (les cubemaps déjà construits dans la copie du dossier maps sont repris comme avant). | désactivé |
| `-autocubemaps` | Chaque `env_cubemap` laissé à la taille par défaut reçoit une taille selon la surface des faces réfléchissantes qu'il sert : 16 pixels pour presque rien, 32 quand les faces qu'il sert s'étendent sur 512 unités au plus, 64 jusqu'à 1536, 128 au-delà. Une taille réglée par le mappeur est gardée. Les cubemaps qu'aucune face n'utilise et qui ne sont le plus proche d'aucun prop sont retirés de la map, avec leurs textures provisoires. | désactivé |
| `-mergestaticprops` | Les `prop_static` proches qui ont le même modèle ou les mêmes matériaux, le même skin et les mêmes clés d'éclairage (ombres, éclairage par sommet, fondu, couleur de rendu...) sont combinés en un seul modèle statique par groupe, construit avec le compilateur de modèles de **Réglages > Compilation > Compilateur de modèles** (studiomdl par défaut), comme [Combiner des props](../features/propper.md). Les props sont groupés par zone de pièce : seulement les props à moins de 384 unités les uns des autres, dans la même case de 512 unités et sans mur plein entre eux, pour que la visibilité continue de cacher chaque groupe. Les props nommés (targetname, parentname, sorties) ne sont jamais combinés. Les modèles vont dans `<map>_merged_props` à côté de la map et sont inclus dans le BSP. Jusqu'à 4 groupes sont construits en même temps (la moitié des cœurs du processeur au plus), chaque compilation de modèle est arrêtée après 3 minutes (ce groupe reste tel quel) et le journal montre l'avancement, par exemple `Optimizing 25%: Merging static props: 8 of 32 groups done, 12 s elapsed, about 36 s left`. | désactivé |
| `-autohints` | La compilation calcule la visibilité une fois, mesure les brushes hint que l'[optimiseur](../guides/optimizing-a-map.md) propose (8 au plus, en compilant avec chacun) et n'ajoute que ceux qui baissent les totaux de visibilité, vérifiés ensemble aussi. Les mesures utilisent toujours des compilations avec la visibilité rapide, même avec `-vis`, et s'arrêtent après 3 minutes (les hints déjà mesurés sont gardés). Le journal montre chaque mesure avec le temps restant. Plus lent : une compilation de plus par candidat. Demande `-vis` ou `-fastvis` : sans visibilité l'option est sautée avec une note dans le journal. | désactivé |
| `-autofade` | Les `prop_static` sans distance de fondu (`fademaxdist` vide ou 0, sans fondu à l'écran) en reçoivent une selon leur taille : à partir de 800 unités pour les petits props jusqu'à 4000 unités au plus, le fondu commençant à 75 % de cette distance. Les props de plus de 400 unités et ceux de la skybox 3D restent sans fondu. Quand la map a des props de détail et pas d'`env_detail_controller`, un est ajouté avec un fondu selon la taille des sprites et modèles de détail (400 à 1200 unités, jamais plus loin que le réglage du jeu). | désactivé |
| `-autooccluders` | Les grands murs en boîte droits (au moins 256 sur 128 unités et 8 d'épaisseur, faces texturées opaques des deux grands côtés) qui cachent au moins 3 props d'un côté où se trouvent des entités reçoivent un `func_occluder` fin juste devant leur face, pour que le moteur ne dessine pas les props derrière. 16 au plus, les plus grands d'abord, jamais dans la skybox 3D. | désactivé |

Pendant que ces options tournent, l'étape de géométrie montre leur étape et leur pourcentage dans la fenêtre de compilation (lignes qui commencent par `Optimizing`), avec une nouvelle ligne au moins toutes les 5 secondes, et le journal commence par la liste des options d'optimisation activées. Annuler arrête l'étape et les compilations de modèles qu'elle a lancées.

Comment elles se recoupent : `-culltjunc` empêche l'ajout des sommets de jonction en T inutiles, `-cullverts` retire ceux qui restent une fois toutes les faces connues et économise aussi les sommets que `-culltjunc` ne touche pas (découpes de lightmap, faces d'origine, sommets inutilisés) ; `-cullverts` seul donne donc l'essentiel du gain et les deux ensemble donnent le moins d'arêtes. `-cullbrushes` agit sur les brushes avant la construction des faces, donc un peu moins de géométrie arrive aux deux autres. Sur les maps de test, `-cullverts` a retiré de 18 à 50 pour cent des sommets.

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
## Ray tracing

Groupe de menu **Ray tracing** (étape d'éclairage), et la ligne et les options **Ray tracing** de la [fenêtre Compiler et lancer](compile-window.md). Désactivé par défaut, dans tous les préréglages.

Le ray tracing remplace la radiosité classique de l'étape d'éclairage par un path tracer physique. Il écrit les mêmes formats de lightmaps : la map se charge dans les mêmes jeux et rend bien en jeu et dans la [vue joueur](../features/player-view.md).

**Ce qu'il calcule.** Une illumination globale Monte Carlo sans biais : les chemins de lumière rebondissent de nombreuses fois (ceux qui touchent des surfaces sombres s'arrêtent tôt par roulette russe, sans biais). La lumière directe est une estimation du prochain événement vers chaque type de lumière Source, échantillonnée de nombreuses fois par luxel : `light`, `light_spot` avec ses cônes et son exposant, `light_environment` avec le soleil doté d'une taille angulaire pour des ombres douces et la lumière ambiante du ciel, les lumières de texture de `lights.rad` et les surfaces auto illuminées (les matériaux `$selfillum` deviennent des lumières de surface, d'après leur couleur lumineuse). À chaque rebond, le chemin lit cette lumière directe convergée dans la lightmap qu'il touche (les props statiques et les faces sans lightmap échantillonnent une lumière par estimation du prochain événement), donc de nombreux rebonds restent sans bruit. L'atténuation suit les règles de VRAD (`_constant_attn`, `_linear_attn`, `_quadratic_attn`, `_fifty_percent_distance`, `_zero_percent_distance`, `_hardfalloff`) : une lumière est aussi forte qu'avec VRAD. Les surfaces à alpha testé et translucides, la teinte du verre, l'eau, la skybox 3D, les func_detail, les displacements et les ombres des props statiques d'après leurs vrais maillages sont tous tracés. La lumière rebondie prend la couleur des surfaces qu'elle quitte, d'après la moyenne de leur texture de base comme la réflectivité de VRAD.

**Échantillonnage.** Des échantillons stratifiés à faible discrépance ; un nombre d'échantillons adaptatif, les luxels des pénombres et des coins sombres en prenant plus que les murs simplement éclairés ; plusieurs points d'échantillonnage par luxel, placés dans la partie ouverte du luxel, donc pas de fuite de lumière sous les murs ni d'acné d'ombre ; un débruiteur qui respecte les contours et garde les bords d'ombres nets ; les raccords entre faces sont recousus.

**Sorties.** Sur gm_construct (750 000 luxels, RTX 3060 Ti), la qualité Normal prend environ 35 s avec l'éclairage des props statiques, contre 10 s pour l'éclairage classique ; ses lightmaps sont à moins de 2 % du VRAD de Valve en luminosité médiane. Lightmaps LDR et HDR, lightmaps bumpées pour les surfaces avec normal map, styles de lumière (lumières commutables), cubes ambiants pour les modèles, éclairage par sommet des props statiques et displacements.

**GPU et CPU.** Sur le GPU il tourne en compute shaders wgpu sur un BVH construit sur le CPU avec l'heuristique de surface. Sur les grandes scènes (300 000 triangles et plus) et les cartes à requêtes de rayons matérielles, il chronomètre les deux sur un lot de test et garde le plus rapide (le journal dit lequel) ; sinon le BVH est parcouru en compute. Les points d'échantillonnage restent sur la carte quand ils tiennent, la scène est vérifiée contre le budget de VRAM, et les très grandes maps prennent moins de points par luxel pour que la mémoire reste bornée (24 millions de points au plus). Le travail est découpé en lots pour que le pilote graphique n'expire jamais, et il respecte le budget de VRAM (réglage `render.vram_budget_mb`, ou la variable d'environnement `HAMMERPRO_VRAM_BUDGET_MB`). La progression et l'annulation marchent comme pour les autres étapes. L'aperçu ray tracing de l'éditeur utilise le même moteur par tranches courtes d'environ 8 ms sur la carte, trois en vol, dimensionnées d'après les mesures de la carte elle même, et laisse à l'éditeur sa part du GPU (Part du GPU de l'aperçu), pour que la navigation garde sa fluidité. Sans GPU utilisable, il exécute le même algorithme sur le CPU en priorité inférieure à la normale, et le journal écrit `Aucun GPU détecté, la compilation avec Ray Tracing prendra plus de temps.` (dans la langue de l'éditeur) ; la fenêtre de compilation affiche cette ligne en avertissement.

**Niveaux de qualité.** **Brouillon** pour les aperçus rapides, **Normal** pour les compilations de tous les jours, **Final** pour la sortie, **Optimal** où le compilateur choisit le meilleur équilibre entre qualité, taille de la map et temps de compilation pour cette map et cet ordinateur, et écrit dans le journal ce qu'il a choisi et pourquoi : échantillons et rebonds d'après le nombre de luxels et le GPU ou le CPU, 3, 2 ou 1 points d'échantillonnage par côté de luxel jusqu'à 1,5 et 5 millions de luxels, et un budget de lightmaps de 48 Mo par passe (la densité de luxels baisse quand les lightmaps ne tiendraient pas). Le journal finit par la taille du lump d'éclairage face à la limite du moteur. La même graine avec les mêmes réglages donne le même résultat.

Le bouton **Optimal** des options **Ray tracing** met la qualité sur Optimal, remet chaque paramètre en automatique et active `-autolightmapscale` de l'étape de géométrie (lightmaps plus fines près des zones praticables et des lumières, plus grossières ailleurs). Les limites du moteur ne sont jamais dépassées.

| Option | Réglage de la fenêtre | Effet | Défaut (draft, normal, final) |
|---|---|---|---|
| `-raytracing [qualité]` | **Utiliser le ray tracing** | Active le ray tracing. | désactivé |
| `-rtquality <q>` | **Qualité** | `draft`, `normal`, `final` ou `optimal`. | `normal` |
| `-rtsamples <n>` | **Échantillons par luxel (min)** | Échantillons que chaque luxel prend au moins. | 16, 64, 256 |
| `-rtmaxsamples <n>` | **Échantillons par luxel (max)** | Nombre maximal d'échantillons d'un luxel là où le bruit reste fort. | 64, 256, 1024 |
| `-rtbounces <n>` | **Rebonds** | Nombre maximal de rebonds. | 3, 6, 12 |
| `-rtthreshold <f>` | **Seuil de bruit** | Un luxel arrête de prendre des échantillons quand son bruit passe sous ce niveau. | 0.08, 0.04, 0.02 |
| `-rtlightsize <f>` | **Échelle de taille des lumières** | Échelle de la taille de chaque lumière : les lumières ponctuelles et les spots font 2 unités de large par défaut, le soleil prend son `SunSpreadAngle`. 0 donne les ombres dures de VRAD. | 1 |
| `-rtsunangle <degrés>` | **Taille angulaire du soleil** | Largeur du soleil pour des ombres de soleil douces. | la `light_environment` |
| `-rtskysamples <n>` | **Échantillons du ciel** | Directions du ciel par échantillon de lumière directe. | 1, 1, 2 |
| `-rtdenoise <0 à 4>` | **Force du débruiteur** | Débruiteur qui respecte les contours, 0 le désactive. | 1 |
| `-rtsupersample <1 à 8>` | **Suréchantillonnage** | Points d'échantillonnage par côté de luxel. | 2, 3, 4 |
| `-rtnobump` | **Lightmaps bumpées** décoché | Pas de lightmaps bumpées (plus léger, plus plat). | bumpées |
| `-rtpropsamples <n>` | **Échantillons d'éclairage des props** | Rayons par sommet de prop statique, avec l'éclairage des props statiques. | selon la qualité |
| `-rtdevice <auto, gpu, cpu>` | **Périphérique** | Où tourne le path tracer. | `auto` |
| `-rtchunk <luxels>` | **Taille des lots GPU** | Luxels par lot GPU. | automatique |
| `-rtseed <n>` | **Graine** | Graine aléatoire. | 0 |
| `-rtgpushare <auto, 0.1 à 1>` | **Part du GPU de l'aperçu** | Part de la carte graphique que l'[aperçu en ray tracing](../features/lighting-preview.md) utilise pendant que vous éditez. Auto garde l'éditeur fluide (60 images par seconde ou plus) ; plus haut converge plus vite, plus bas laisse plus d'images par seconde à l'éditeur. Seul l'aperçu la reçoit : les compilations utilisent toujours toute la carte graphique. | Auto (60 %) |
| `-rtlang <code>` | (la langue de l'éditeur) | Langue des messages du journal. | `en` |
| `-rtnoao`, `-rtaostrength <0 à 1>`, `-rtaoradius <unités>` | **Occlusion ambiante**, **Force de l'occlusion**, **Rayon de l'occlusion** | Ombres de contact précalculées dans les coins, sous et à la base des props statiques, aussi dans l'éclairage par sommet des props. | activée, 0.5, 48 |
| `-rtnoshafts`, `-rtshaftintensity`, `-rtshaftlength`, `-rtshaftsoftness` | **Rayons de lumière**, **Luminosité des rayons**, **Longueur des rayons**, **Douceur des rayons** | Faisceaux précalculés en maillages additifs d'aspect volumétrique là où le soleil (et les cônes de spots puissants avec `sun_and_spots`) passent par les fenêtres, portes et trous. Dessinés par quelques sprites orientés additifs croisés, avec un dégradé doux qui s'estompe vers la surface éclairée, empaquetés dans le BSP avec leur matériau ; sans collision ni ombre, 96 sprites au plus. Le journal indique où sont les plus grands rayons. | activés, 1, 1, 1 |
| `-rtshaftwhite`, `-rtshaftsource <sun, sun_and_spots>`, `-rtshaftdust` | **Couleur de la lumière**, **Sources des rayons**, **Poussières** | Couleur des rayons, les lumières qui en font, des poussières qui y flottent. | couleur de la lumière, `sun_and_spots`, désactivé |
| `-rtglow`, `-rtglowintensity <0 à 8>` | **Halos lumineux**, **Luminosité des halos** | Halos doux autour des lumières puissantes. | désactivé, 1 |
| `-rtnocubemaps` | **Cubemaps automatiques** décoché | Quand la map n'a pas d'`env_cubemap` à elle, l'étape de géométrie place des `env_cubemap` là où les surfaces réfléchissantes en ont besoin, avec les tailles de l'analyse des cubemaps (comme `suggest_cubemaps`), dans la map compilée seulement (le VMF n'est jamais modifié), et dimensionne les cubemaps selon ce qu'ils reflètent (`-autocubemaps`). Une map qui a ses propres cubemaps les garde, seulement dimensionnés. Le journal dit combien ont été placés et pourquoi. `buildcubemaps` en jeu les rend toujours. Écrit `-rtautocubemaps` sur l'étape de géométrie. | activé |
| `-rtnotonemap` | **Réglages de tonemap** décoché | Un `env_tonemap_controller` avec un bloom et une exposition adaptés au HDR, seulement quand la map n'en a pas. | activé |
| `-rtcolorcorrection` | **Correction colorimétrique** | Correction colorimétrique automatique discrète. | désactivé |

Les options **Ray tracing** apparaissent sous les options d'éclairage seulement quand **Ray tracing** est coché. Leur **Configuration** est **Par défaut** quand le ray tracing est activé (qualité Normale, toutes les valeurs automatiques, les effets par défaut), **Optimale** (ce que fait le bouton **Optimal**) ou **Personnalisée**, choisie d'elle même dès que vous changez une valeur ou choisissez Brouillon ou Final. **Rétablir par défaut** revient à Par défaut. Dans la fenêtre, chaque paramètre a une case **Auto** : cochée, la valeur vient du niveau de qualité et s'affiche en gris. Les effets ne changent que la map compilée, jamais le VMF. L'échelle des lightmaps, la densité de luxels et l'occlusion ambiante gardent leurs propres options (`-luxelscale`, `-autolightmapscale`, `-luxeldensity`, `-ao`). Les outils MCP `set_compile_options` et `compile_with_options` prennent les mêmes paramètres (`raytracing`, `raytracing_quality`, `raytracing_optimal`, `rt_config`, `rt_reset`, `rt_samples`, `rt_ao`, `rt_shafts`, `rt_glow`...), voir [Serveur MCP](../reference/mcp-server.md).

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
