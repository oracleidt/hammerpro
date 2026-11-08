[Documentation](../README.md) > Fonctionnalités > Aperçu de l'éclairage | [English](../../en/features/lighting-preview.md) | [Русский](../../ru/features/lighting-preview.md)

# Aperçu de l'éclairage

Hammer Pro montre l'éclairage dans l'éditeur de deux façons : le mode **éclairé**, des lumières d'aperçu en temps réel avec ombres, et l'**aperçu de l'éclairage précalculé**, la map compilée et éclairée par le compilateur intégré pour voir l'éclairage que le jeu aura, sans quitter l'éditeur. L'onglet **Éclairage**, la grille de lightmap et le budget des lightmaps vous aident à garder les lightmaps sous contrôle, et la vue 3D dessine les reflets des cubemaps.

## Accès

| Action | Menu | Raccourci par défaut |
| --- | --- | --- |
| **Aperçu de l'éclairage** (mode éclairé) | **Affichage > Mode de rendu 3D** | `Alt+5` |
| **Grille de lightmap** | **Affichage > Mode de rendu 3D** | `Alt+8` |
| **Aperçu de l'éclairage précalculé** | **Affichage > Mode de rendu 3D** | `Ctrl+F5` |
| **Précalculer l'éclairage** | **Map > Éclairage** | aucun |
| **Précalculer autour de la caméra** | **Map > Éclairage** | aucun |
| **Arrêter le précalcul** | **Map > Éclairage** | aucun |
| **Sélectionner les faces à petite échelle de lightmap** | **Map > Éclairage** | aucun |
| **Reconstruire les cubemaps** | **Map > Éclairage** | aucun |
| **Écrire les cubemaps dans la map compilée** | **Map > Éclairage** | aucun |
| **Reflets des cubemaps** | **Affichage > Options d'affichage** | aucun |
| Onglet Éclairage | **Affichage > Panneaux > Éclairage** | aucun |

## Mode éclairé (Alt+5)

Les entités `light`, `light_spot`, `light_environment` et `light_dynamic` éclairent la vue 3D en temps réel, avec leur couleur, leur luminosité et leur atténuation. L'aperçu suit vos déplacements de lumières.

- **Le soleil** : la première `light_environment` de la map (comme VRAD, les autres sont ignorées) éclaire selon ses **angles**, ou selon sa clé **pitch** quand elle n'est pas 0, de la même façon que le compilateur : un pitch négatif pointe vers le bas, donc `pitch -90` éclaire tout droit vers le bas et un pitch au-delà de -90 retourne le soleil. Une clé `target` le dirige vers l'entité de ce nom, comme VRAD. Le soleil n'éclaire que ce qui voit une face de ciel dans sa direction, comme VRAD : ce qui est au-delà du ciel (la salle de la skybox 3D, le dos des brushes de ciel, les salles fermées hors de la zone jouable) ne projette pas d'ombre sur la map, et les endroits sans ciel au-dessus d'eux ne reçoivent pas de soleil. Chaque point n'est ombré que par ce qui se trouve entre lui et le ciel qu'il atteint vers le soleil, donc les ciels empilés les uns au-dessus des autres marchent comme en jeu : une salle fermée avec son propre plafond de ciel, suspendue au-dessus d'une zone sous skybox, n'ombre pas cette zone. `get_lighting_status` (MCP) indique combien de ciels sont empilés (`sky_layers`).
- **Ombres du soleil** : sur ce que voit la caméra, jusqu'à 8192 unités. **SunSpreadAngle** les adoucit comme le compilateur : plus l'ombre est loin de ce qui la projette, plus elle est douce. Les entités brush ne projettent pas d'ombre sauf avec `vrad_brush_cast_shadows` à 1 (comme VRAD), les props avec `disableshadows` à 1 non plus, et les aides de l'éditeur (départs de joueur, nodes, boîtes des entités ponctuelles) jamais.
- **Ombres des props statiques** : les `prop_static` projettent leurs ombres comme le fera la compilation. **Outils > Options... > Rendu > Ombres des props statiques** (`render.prop_shadows`) : **Auto** (par défaut) suit les options de compilation du compilateur choisi : les triangles que dessine le modèle quand l'étape d'éclairage tourne avec `-StaticPropPolys`, son modèle de collision sinon (réglages VRAD de base, et le compilateur Hammer Pro tant que vous n'ajoutez pas l'option), aucune avec `-nostaticpropshadows` (ou **Ombres des props statiques** décoché dans la fenêtre de compilation). **Triangles du modèle** et **Modèle de collision** imposent l'un ou l'autre. Un modèle sans modèle de collision projette l'enveloppe convexe de chacun de ses maillages, comme le compilateur. Les autres props (`prop_dynamic`, `prop_physics`) projettent avec leurs triangles, comme leurs ombres en jeu.
- **Lumière du ciel et lumière rebondie** : avec **Outils > Options... > Rendu > Lumière rebondie en mode éclairé** (`render.lit_indirect`, activé) le mode éclairé lance une cuisson de la map en arrière-plan avec le compilateur (la compilation de l'aperçu cuit, sans lightmaps dessinées) un instant après son activation et après les modifications affectant l'éclairage (le **Délai de mise à jour** et les **Mises à jour dynamiques** de l'onglet Éclairage s'appliquent). Son résultat, la lumière du ciel (`_ambient`) et la lumière que les surfaces se renvoient, stocké en sondes de lumière près des surfaces (cellules de 32 à 512 unités, moins nombreuses et plus grosses sur les très grandes maps), éclaire ce que les lumières en temps réel n'atteignent pas, comme la part ambiante des lightmaps du jeu ; les lumières et le soleil restent en temps réel et nets. Tant que la cuisson n'est pas finie, et quand elle échoue (une map qui fuit, par exemple), le mode éclairé utilise une lumière du ciel approchée : la couleur `_ambient` de la `light_environment` n'éclaire que ce qui voit le ciel, un sol à découvert la reçoit en entier, un mur à moitié, les intérieurs et les endroits couverts pas du tout, murs minces compris (**Outils > Options... > Rendu > Lumière du ciel seulement sous le ciel**, `render.sky_occlusion`). `get_lighting_status` (MCP) donne l'état dans `realtime.indirect`.
- **Valeurs de lumière HDR** : avec **Outils > Options... > Rendu > Valeurs de lumière HDR** (`render.light_preview_hdr`, activé), les lumières prennent `_lightHDR` (ou `_light` quand elle vaut `-1 -1 -1`) multiplié par `_lightscaleHDR`, et le ciel `_ambientHDR` multiplié par `_AmbientScaleHDR`, comme le jeu sur une map compilée en HDR. Désactivé : `_light` et `_ambient`. Une valeur de lumière peut être `r g b`, `r g b luminosité`, un seul gris, ou les huit nombres `r g b i` du LDR puis du HDR.
- **Entités de lumière** : elles suivent le compilateur (vérifié contre le vrai VRAD) : une `light` sans clés d'atténuation n'a aucune décroissance (constante 1), les termes sous 0,001 valent 0, `_fifty_percent_distance`, `_zero_percent_distance` et `_hardfalloff` donnent les courbes de VRAD, les cônes des spots sont limités à 90 degrés (180 et 180 font une lumière ponctuelle) et décroissent avec le cosinus de l'angle à l'axe, `_inner_cone`, `_cone` et `_exponent` façonnent la pénombre, un `angle` de -1 ou -2 sans `pitch` ne vise rien comme dans VRAD, `_distance` ne limite que l'éclairage des props (VRAD éclaire au-delà), les lumières nommées avec **Initially dark** restent éteintes, et une lumière à l'intérieur d'un brush opaque n'éclaire rien. Seule la première `light_environment` compte, où qu'elle soit.
- **Textures projetées** : les entités `env_projectedtexture` projettent leur texture comme le jeu : la texture (`texturename`, `effects/flashlight001` par défaut, image `textureframe`), `lightfov` (le jeu prend 45 quand la clé manque), `nearz`, `farz` (la lumière s'estompe sur les 40 derniers pour cent), pleine intensité jusqu'à 100 unités de la texture projetée puis baisse avec l'inverse de la distance (l'atténuation linéaire 100 du jeu), `lightcolor` multiplié par `lightstrength`, orientées par `angles` ou par l'entité `target`. Elles éclairent le mode éclairé, l'aperçu précalculé et la vue joueur (par-dessus les lightmaps compilés, comme la lumière dynamique du jeu), les 16 premières de la map ; **Start Disabled** et `lightworld 0` en écartent une. **Outils > Options... > Rendu > Textures projetées** (`render.projected_textures`, activé) ; `get_lighting_status` (MCP) les compte dans `realtime.projectors`. `enableshadows` (activé sauf si la clé vaut 0) donne aux quatre premières textures projetées les ombres du monde et des props. En vue joueur, la simulation des I/O joue leurs entrées `TurnOn`, `TurnOff`, `FOV`, `EnableShadows`, `SpotlightTexture` et `LightWorld` (une texture projetée Start Disabled s'allume quand `TurnOn` l'atteint) ; quitter la vue joueur rend les clés de la map.
- **Grandes maps** : toutes les lumières sont dessinées, même par milliers. Là où plus de 128 lumières atteignent le même endroit de la vue, seules les 128 plus fortes à cet endroit l'éclairent. `get_lighting_status` (MCP) donne le nombre de lumières dessinées et d'endroits encombrés.
- **Ombres des lumières** : toutes les lumières ponctuelles et spots en vue projettent des ombres, les plus brillantes d'abord, si bien qu'une lumière n'éclaire jamais à travers un mur (VRAD fait l'ombre de chaque lumière). **Outils > Options... > Rendu > Lumières avec ombres** (`render.light_shadows`) en fixe le nombre : de 0 à 64, 16 par défaut, 0 pour le soleil seul ; au-delà de 8 lumières les cartes d'ombre sont plus grossières (256 pixels par face au lieu de 512). Une lumière qui attend ses cartes d'ombre reste noire au lieu de fuir. Les cartes d'ombre ne sont redessinées que quand une lumière ou la scène change. Les faces de brush des textures d'outil qui bloquent la lumière (nodraw, `toolsblocklight`) projettent des ombres même masquées.
- Les props sont éclairés par les mêmes lumières, le même soleil et la même lumière du ciel que les brushes, au pixel près.
- Avec **Relief (normal maps)** activé (par défaut), les normal maps des matériaux donnent leur relief aux surfaces.
- Un matériau avec `$nodiffusebumplighting 1` conserve un éclairage diffus plat, tandis que sa normal map déforme toujours les reflets des cubemaps. Son éclairage précalculé utilise une lightmap plate.

Les aperçus des matériaux suivent le mode HDR ou LDR actif, y compris leurs textures de base, normal maps et paramètres de réflexion. La vue joueur suit la map compilée, l’aperçu précalculé suit son calcul, et les autres vues de l’éditeur suivent **Valeurs de lumière HDR**. Changer de mode actualise automatiquement les matériaux utilisés.

## Aperçu de l'éclairage précalculé (Ctrl+F5)

Comme le « 3D Lighting Preview » de Hammer++, mais avec les chiffres du compilateur : la map est compilée en mémoire par le compilateur de Hammer Pro (l'étape de géométrie) et éclairée par son étape d'éclairage (celle de VRAD) dans un processus séparé. Les faces compilées sont dessinées avec leurs lightmaps (les quatre lightmaps bumpées des faces à normal map, pour le relief) et ombrées exactement comme l'éclairage compilé de la [vue joueur](player-view.md) (mêmes poids de relief, reflets de cubemap, auto-illumination et défilement de texture, les lightmaps LDR d'une map compilée sans HDR dessinées comme le jeu, le ciel vu à travers les faces de ciel), si bien que l'aperçu donne l'image de la map compilée en jeu, et les props sont éclairés par les ambient cubes, les lumières du monde et l'éclairage par sommets des props statiques de la map éclairée. Les props statiques suivent les réglages de compilation pour leurs ombres et leur éclairage. Un prop est éclairé à son origine d'éclairage comme en jeu : l'entité `lightingorigin` quand la clé en nomme une, sinon le centre du modèle.

- Quand l'aperçu est activé et que rien n'est encore précalculé, un calcul de toute la carte démarre par défaut (réglage **Précalculer à l'activation de l'aperçu**). L'option **Précalculer autour de la caméra sur les grandes cartes** peut limiter ce démarrage automatique à une région ; elle est désactivée par défaut.
- **Précalculer l'éclairage** calcule toujours toute la carte, même si l'option régionale automatique est activée. Son résultat reste global pendant les déplacements de caméra.
- Chaque précalcul se fait en deux passes. D'abord une **passe rapide**, avec l'échantillonnage Hammer++ des options de la map (**Sun Samples**, **Ambient Samples** et **Supersample** du bloc `light_plus`, 6, 40 et désactivé par défaut ; lumière rebondie en Brouillon ; pas de sondes de lumière), affichée dès qu'elle est finie : mêmes niveaux de lumière, un peu plus de bruit. Puis une **passe affinée** à la **Qualité** choisie la remplace si les données du calcul d'éclairage n'ont pas changé entre-temps (type de progression **Affinage**). Une modification nécessitant un nouveau bake global annule la passe en cours avec les **Mises à jour dynamiques** activées, puis relance le calcul après le délai. Les changements de lumières locales peuvent conserver le dernier bake global, comme décrit ci-dessous.
- Un solide modifié masque ses faces précalculées jusqu'au prochain précalcul ; d'ici là ses faces, comme les surfaces sans lightmap (eau, vitres translucides, props sans éclairage précalculé), ne sont éclairées que par le soleil et la couleur du ciel, sans ombres. Avec **Mises à jour dynamiques**, la map est précalculée à nouveau quand les modifications s'arrêtent pendant le **Délai de mise à jour**.
- La vue précalculée réutilise les lightmaps calculées pendant les déplacements de caméra. Les changements de lumières locales ajoutent une différence signée d'éclairage direct avec des lumières et ombres temps réel lorsque **Conserver le bake global pendant l'édition des lumières locales** est activé. Les props conservent les cubes ambiants, lumières du monde et couleurs par sommet du bake ; les matériaux, modèles et reflets gardent leur coût de dessin.
- **Précalculer autour de la caméra** compile et éclaire seulement ce qui est à moins du **Rayon autour de la caméra** de la caméra 3D ; la zone est fermée comme un cordon. Cette région choisie manuellement reste à sa position d'origine. Elle est distincte de la région automatique optionnelle qui suit la caméra.
- **Arrêter le précalcul** arrête tout de suite (le processus d'éclairage est arrêté et sa mémoire rendue au système).
- La compilation de l'aperçu ne s'arrête que sur ce dont l'aperçu a vraiment besoin : les limites du jeu qu'elle n'a pas à stocker sont signalées comme avertissements, les plans des brushes sous les displacements (que le BSP ne garde pas) sont laissés de côté pour qu'une map juste au-dessus de MAX_MAP_PLANES se précalcule quand même, et les clés `env_cubemap` invalides sont ignorées. Une vraie compilation s'y arrête toujours, comme VBSP.
- Le processus d'éclairage et les threads de compilation tournent avec une priorité processeur et une priorité mémoire plus basses pour que l'éditeur reste fluide et que Windows reprenne leurs pages en premier quand la mémoire manque. La compilation de l'aperçu laisse de côté les données de collision et les statistiques, libère ce dont elle n'a plus besoin en cours de route et écarte les objets cachés avant de commencer : son pic de mémoire est plus bas que celui d'une compilation normale de la même map. La texture des lightmaps n'est pas plus grande que ce qu'il faut (au moins 256 sur 256 pixels, au plus 2048), ce qui économise de la mémoire vidéo sur les petits précalculs. Les grandes maps sont prévisualisées avec des lightmaps plus grossières (réglage **Luxels en pleine résolution**), et une note l'indique (« Grande carte : aperçu à N fois la résolution des lightmaps »).
- HDR ou LDR vient des réglages de compilation : l'onglet affiche **HDR** ou **LDR** en conséquence.

## Réutiliser un résultat calculé

Attendez la fin des passes rapide et affinée, puis naviguez dans la vue précalculée. Le résultat global reste valide quand la caméra ou la sélection change. La luminosité de l'aperçu, la correction colorimétrique et le délai de mise à jour concernent l'affichage ou la planification et ne relancent pas le calcul d'éclairage.

La géométrie, l'environnement, les matériaux et les paramètres physiques de calcul affectent le résultat global. Les **Mises à jour dynamiques** lancent un nouveau calcul après le délai choisi. Avec la conservation du bake global, les changements de lumières locales actualisent l'éclairage direct et conservent les rebonds précédents. L'onglet indique que le résultat global est périmé jusqu'au prochain bake manuel. `get_lighting_status` expose la progression, le dernier résultat et son état périmé.

Le mode régional est un choix explicite pour l'activation automatique de l'aperçu sur les grandes cartes. La région suit la caméra quand elle quitte sa moitié centrale et reste immobile une seconde ; entrer dans une autre zone peut donc lancer un nouveau calcul. **Précalculer l'éclairage** revient à un résultat global sans suivi de caméra. Les valeurs existantes enregistrées de `lighting.auto_scope` sont conservées ; un champ absent utilise le nouveau défaut, `false`.

Modifier uniquement les paramètres photométriques d'une lumière statique réutilise la géométrie BSP lors d'un nouveau calcul demandé. Le cache de géométrie conservée est limité à environ 128 Mio. Le premier bake et un calcul complet d'éclairage prennent toujours du temps.

## L'onglet Éclairage

**Affichage > Panneaux > Éclairage** affiche l'onglet.

Section **Aperçu de l'éclairage** :

| Élément | Effet |
| --- | --- |
| **Précalculer l'éclairage** | Précalcule toujours toute la carte, sans suivi de caméra. |
| **Précalculer autour de la caméra** | Précalcule une zone fixe autour de la position actuelle de la caméra 3D (l'infobulle donne le rayon). |
| **Arrêter le précalcul** | Actif pendant un précalcul. |
| Progression | Type (**Carte entière**, **Autour de la caméra**, **Mise à jour**, **Affinage**), phase (**Préparation**, **Compilation de la carte**, **Lumière directe**, **Lumière rebondie**, **Finalisation**) et durée. |
| Dernier résultat | « N faces précalculées en N s », **Précalcul arrêté** ou **Pas encore précalculé** ; mémoire vidéo et triangles d'ombre. |
| Avertissements | « La carte n'a aucune entité de lumière », **L'éclairage n'est plus à jour** avec **Précalculer à nouveau**. |
| **Qualité** | **Brouillon** (même modèle et mêmes niveaux de lumière qu'une compilation, ombres plus bruitées, par défaut), **Normal** (une compilation normale), **Finale** (une compilation finale, la plus lente). |

**Options (enregistrées dans la carte)** : stockées dans le bloc `light_plus` de Hammer++ du VMF, elles voyagent avec la map.

| Option | Effet | Défaut |
| --- | --- | --- |
| **Luminosité** | Luminosité de l'aperçu, de 0 à 4. | 1 |
| **Lumière rebondie** | Lumière réfléchie par les surfaces (précalcul plus lent). | Activé |
| **Mises à jour dynamiques** | Précalcule à nouveau automatiquement quand les modifications s'arrêtent pendant le délai. | Activé |
| **Délai de mise à jour** | De 0 à 60 s. | 0 s |
| **Rayon autour de la caméra** | Rayon de **Précalculer autour de la caméra**, de 64 à 65536. | 1024 |
| **Avancé > Échantillons du soleil** | Rayons de soleil par luxel de la passe rapide (**Sun Samples** de Hammer++), 1 à 256. | 6 |
| **Avancé > Échantillons ambiants** | Directions de lumière du ciel par luxel de la passe rapide (**Ambient Samples** de Hammer++), 16 à 5184. | 40 |
| **Avancé > Suréchantillonnage** | Adoucit les bords des ombres de la passe rapide (**Supersample** de Hammer++), plus lent. | Désactivé |
| **Avancé > Afficher la portée des lumières** | Affiche la portée des lumières sélectionnées. | Désactivé |
| **Avancé > Rétablir les valeurs par défaut** | Réinitialise ces options. | |

**Correction des couleurs** : des tables de correspondance (`.raw`) appliquées à l'aperçu précalculé, chacune avec un poids (0 à 1), enregistrées dans la map comme dans Hammer++. **Depuis la carte** reprend les fichiers et poids maximaux des entités `color_correction` ; **Effacer** les retire.

**Budget des lightmaps** : une jauge de la taille estimée des lightmaps dans le BSP face aux 2 Go qu'une face peut y adresser (la seule limite du jeu, voir [Limites et journaux](../compiler/limits-and-logs.md)), avec le nombre de faces et de luxels, les faces à très petite échelle de lightmap (**Sélectionner** les sélectionne), les faces découpées par VBSP (plus de 32 luxels de large), les displacements trop grands pour VBSP, et **Faces les plus coûteuses** (face, échelle, luxels, taille ; cliquez sur une face pour la sélectionner).

## Outils de lightmap

- **Grille de lightmap** (`Alt+8`) dessine la grille des luxels de chaque face avec lightmap comme la grille de lightmap 3D de Hammer, sans précalcul : un carreau par luxel, aussi large que l'échelle de lightmap de la face et aligné sur ses axes de texture, displacements compris. La couleur indique l'échelle : gris à 16 (la valeur par défaut), plus rouge quand l'échelle est plus fine (8, 4 et moins ressortent), plus vert quand elle est plus grossière. Des lignes jaunes marquent où VBSP découpe une face de plus de 32 luxels. Une fois un précalcul fait, la grille est ombrée par son éclairage. Les faces sans lightmap (nodraw, textures outils, ciel, eau, matériaux non éclairés) et les props gardent leur texture.
- **Sélectionner les faces à petite échelle de lightmap** les sélectionne, prêtes pour l'outil texture.
- La **barre d'état** affiche le budget des lightmaps (« N / 2048 Mo »). Un clic ouvre l'onglet Éclairage.

## Cubemaps dans les vues

Avec **Reflets des cubemaps** activé (par défaut), les matériaux avec `$envmap` reflètent leur cubemap dans la vue 3D (modes texturé, éclairé et précalculé) et dans la vue joueur. L'éditeur dessine une cubemap à chaque `env_cubemap` (le ciel s'il n'y en a aucun), quelques faces par image. Chaque face prend l'`env_cubemap` le plus proche devant elle comme le compilateur, les props le plus proche. La vue joueur utilise les cubemaps construites en jeu quand la map compilée en a.

- **Redessiner les cubemaps après les modifications** (`render.cubemaps_auto`, activé) : les cubemaps sont redessinées un instant après un changement de la map, de son éclairage ou du mode de la vue. Désactivé : seulement avec **Map > Éclairage > Reconstruire les cubemaps** (les `env_cubemap` nouveaux ou déplacés ont quand même la leur).
- **Cubemaps à correction de parallaxe** (Mapbase) : un `env_cubemap` dont le **Cubemap Bounds** (`parallaxobb`) nomme un brush boîte `parallax_obb` voit ses reflets corrigés selon cette boîte dans les vues. Les shaders d'origine de Garry's Mod ignorent cette correction en jeu.
- **Écrire les cubemaps dans la map compilée** écrit les cubemaps dessinées par l'éditeur dans le pakfile de la map compilée, comme `buildcubemaps` (LDR, et HDR quand la map a un éclairage HDR). Ouvrez d'abord la [vue joueur](player-view.md) sur la map compilée avec son éclairage compilé (`L`) pour qu'elles soient dessinées comme en jeu, et attendez qu'elles soient complètes. Les recompilations les conservent. Voir [Cubemaps](../compiler/cubemaps.md).

## Tone mapping, exposition et bloom

Les vues éclairées (le mode éclairé, l'aperçu précalculé et la vue joueur avec les lightmaps HDR d'une map compilée) transforment la lumière en pixels comme le jeu : l'image est dessinée en HDR, l'exposition la met à l'échelle, et les surfaces bien plus claires que ce que l'écran peut montrer rayonnent (bloom).

- **Tone mapping HDR** (`render.tone_mapping`, **Outils > Options... > Rendu**) : **Exposition automatique** (par défaut) mesure l'image comme le moteur (les 2 pour cent de pixels les plus clairs visent 60 pour cent, jamais sous `mat_tonemap_min_avglum` et avec la même vitesse d'adaptation) et garde l'exposition entre **Exposition auto minimale** (`render.exposure_min`, 0,5) et **Exposition auto maximale** (`render.exposure_max`, 2) ; **Exposition fixe** utilise **Exposition** (`render.exposure`, 1 est la lumière des lightmaps telle quelle, `mat_force_tonemap_scale`) ; **Aucun** montre la lumière telle quelle.
- **Bloom** (`render.bloom`, 1 comme `mat_bloom_scalefactor_scalar` ; 0 désactive la lueur). Il suit aussi la map : avec **Suivre le contrôleur de tonemap de la map** (`render.map_tonemap`, activé), l'`env_tonemap_controller` de la map règle la plage d'exposition et l'échelle du bloom (`SetAutoExposureMin`, `SetAutoExposureMax`, `SetBloomScale`, y compris depuis un `logic_auto` au démarrage), comme en jeu.
- L'éclairage LDR (une map compilée sans HDR) est montré sans tone mapping, comme le jeu. Le ciel de la map (`env_skypaint` ou les faces de la skybox, dessinées depuis leur `$basetexture` comme le fait Garry's Mod même pour les matériaux de skybox `_hdr`) rayonne par le même bloom.
- MCP : `set_setting` modifie ces options, `get_lighting_status` renvoie `tone` (mode, l'exposition actuelle, sa plage, le bloom, ce que règle le contrôleur de tonemap de la map et l'histogramme de luminance lu par l'exposition automatique).

## Options

**Outils > Options... > Éclairage** (`[lighting]` dans `settings.toml`) :

| Option | Effet | Défaut |
| --- | --- | --- |
| **Précalculer à l'activation de l'aperçu** (`auto_bake`) | Lance un précalcul quand l'aperçu précalculé est activé et que rien n'est encore précalculé. Toute la carte par défaut ; l'option régionale peut limiter ce démarrage automatique. | Activé |
| **Threads de précalcul** (`threads`) | 0 utilise tous les cœurs (tous sauf deux pour compiler la map). Utilisé par le prochain précalcul. | 0 |
| **Luxels en pleine résolution (millions)** (`max_luxels_millions`) | Les maps avec plus de luxels sont prévisualisées avec des lightmaps plus grossières (1 à 64). | 16 |
| **Précalculer autour de la caméra sur les grandes cartes** (`auto_scope`) | Mode régional optionnel au démarrage automatique de l'aperçu : au-dessus du seuil, la zone suit la caméra quand elle quitte la moitié centrale et reste immobile une seconde. **Précalculer l'éclairage** reste global. | Désactivé |
| **Seuil de grande carte (millions de luxels)** (`auto_scope_luxels_millions`) | Seuil du mode régional automatique optionnel (1 à 64). | 2 |
| **Rayon des grandes cartes** (`auto_scope_radius`) | Rayon de la région automatique optionnelle (256 à 32768), distinct du rayon manuel de l'onglet Éclairage. | 3072 |
| **Afficher le budget des lightmaps dans la barre d'état** (`show_budget`) | | Activé |
| **Très petite échelle de lightmap** (`tiny_scale`) | Les faces à cette échelle ou moins comptent comme très petites (1 à 64). | 4 |
| **Styles de lumière** (`budget_styles`) | Lightmaps par face comptées par le budget : 1 sans lumières commutables, jusqu'à 4. | 1 |
| Qualité (`quality`) | Réglée dans l'onglet Éclairage : `Draft`, `Normal`, `Final`. | Draft |

Réglages de rendu utilisés ici : **Lumières avec ombres** (`render.light_shadows`, 16), **Lumière rebondie en mode éclairé** (`render.lit_indirect`, activé), **Valeurs de lumière HDR** (`render.light_preview_hdr`, activé), **Lumière du ciel seulement sous le ciel** (`render.sky_occlusion`, activé), **Ombres des props statiques** (`render.prop_shadows`, Auto), **Relief (normal maps)** (`render.normal_maps`, activé), **Reflets des cubemaps** (`render.cubemaps`, activé), **Redessiner les cubemaps après les modifications** (`render.cubemaps_auto`, activé).

## Astuces

- Utilisez **Précalculer autour de la caméra** pendant que vous réglez une pièce : c'est bien plus rapide qu'un précalcul complet.
- Gardez la **Qualité** sur **Brouillon** pendant le travail ; les niveaux de lumière sont les mêmes, seules les ombres sont plus bruitées.
- Outils MCP : `bake_lighting`, `get_lighting_status`, `set_lighting_options` (les options de l'onglet, voir la [référence des outils MCP](../../MCP.md), en anglais).

## FAQ

**L'aperçu précalculé reste sombre.** La map n'a aucune entité de lumière, ou rien n'est encore précalculé : cliquez sur **Précalculer l'éclairage**.

**Pourquoi un brush que j'ai déplacé paraît sans éclairage ?** Les solides modifiés masquent leurs faces précalculées jusqu'au prochain précalcul et ne sont éclairés entre-temps que par le soleil et la couleur du ciel. Avec **Mises à jour dynamiques**, l'éclairage revient quand vous arrêtez de modifier.

**Pourquoi l'éclairage change-t-il un peu quelques secondes après un précalcul ?** La passe rapide s'affiche d'abord, puis la passe affinée à la qualité choisie la remplace. Les niveaux de lumière sont les mêmes ; la passe affinée a des ombres plus douces et moins de bruit.

**L'aperçu correspond-il au jeu ?** Il utilise le même compilateur et la même étape d'éclairage qu'une vraie compilation, à la qualité choisie, et dessine le résultat comme la vue joueur dessine la map compilée. **Brouillon** ne diffère d'une compilation que par son bruit (moins de rayons de ciel, de soleil et de rebond) ; **Normal** correspond à une compilation Normal et **Finale** à une compilation finale.

**L'écriture des cubemaps dit qu'elles sont dessinées avec l'ombrage de l'éditeur.** Ouvrez la vue joueur et appuyez sur `L` pour utiliser l'éclairage compilé, puis recommencez.

## Pages liées

- [Modes de rendu et options d'affichage](render-modes-display.md)
- [Guide de l'éclairage](../guides/lighting.md)
- [Cubemaps](../compiler/cubemaps.md)
- [Vue joueur](player-view.md)
- [Référence des paramètres](../reference/settings.md)

---
[Retour à l'index de la documentation](../README.md)

En aperçu éclairé HDR, le ciel utilise aussi ses textures HDR lorsqu’elles existent, y compris les paramètres HDR placés dans les fichiers de matériau ordinaires du ciel. Les vues LDR et texturées ordinaires conservent le ciel LDR. L’occlusion du ciel en temps réel prend en compte la surface directement au-dessus du point éclairé, pour éviter la fuite due à l’intervalle entre les premiers échantillons sous les plafonds des grottes. L’aperçu de l’éclairage indirect continue de fournir la lumière passant par les ouvertures et les rebonds.

## Configuration VRAD et conservation du bake

L'onglet VRAD du panneau Éclairage règle les rebonds, l'éclairage et les ombres des props statiques, les ombres selon leur maillage ou l'alpha des textures, la désactivation des ombres des displacements, l'échantillonnage ambiant rapide et les échantillons du soleil et de l'ambiance. Des cases sélectionnent les light_environment, shadow_control et env_tonemap_controller détectés pour l'aperçu. Ces choix sont conservés avec la map ; Précalculer l'éclairage reste le calcul de toute la map.

Conserver le bake global pendant l'édition des lumières locales est activé par défaut. Une lumière locale ajoutée, déplacée, supprimée ou recolorée contribue par une différence signée d'éclairage direct aux lightmaps et à l'éclairage précalculé des props. Une lumière inchangée n'ajoute aucun éclairage supplémentaire. Les rebonds du dernier bake sont conservés. Un changement d'environnement, de géométrie, de matériau ou de paramètre physique nécessite encore un calcul. Précalculer l'éclairage actualise l'illumination globale ; la différence dynamique ne calcule pas de nouveaux rebonds.
