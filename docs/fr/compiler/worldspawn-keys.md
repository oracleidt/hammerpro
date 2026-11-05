[Documentation](../README.md) > Compilateur > Clés worldspawn et lumières | [English](../../en/compiler/worldspawn-keys.md) | [Русский](../../ru/compiler/worldspawn-keys.md)

# Clés worldspawn et lumières

Les clés de la map (worldspawn) et des entités que le compilateur lit pour construire et éclairer la map. Les clés worldspawn se règlent dans **Map > Propriétés de la map...** ; les autres sont des clés des entités elles-mêmes, réglées dans le [panneau d'entité](../features/entity-properties.md).

## Clés worldspawn

| Clé | Lue par | Effet | Par défaut |
|---|---|---|---|
| `_lightsrad` | éclairage | Un fichier de lumières de texture propre à la map (la clé qu'utilise Hammer++). Un chemin complet, ou relatif au dossier du jeu, au dossier de la map ou au dossier de Garry's Mod. Lu après `lights.rad` et `-lights` (une seule fois si c'est le même fichier que `-lights`), avant `<map>.rad`. | aucun |
| `detailvbsp` | géométrie | Le fichier de définition des props de détail des matériaux `%detailtype`. | `detail.vbsp` |
| `skyname` | géométrie | Le ciel dont les textures servent aux cubemaps provisoires (voir [Cubemaps](cubemaps.md)). | |

L'ordre de priorité est toujours : le paramètre de la ligne de commande, puis la clé worldspawn, puis la valeur par défaut de la qualité.

## Lumières

Le compilateur lit les entités de lumière comme VRAD (`light`, `light_spot`, `light_environment` ; `light_dynamic` est ignorée).

| Clé | Entités | Effet |
|---|---|---|
| `_light` | toutes | Couleur et intensité `r g b intensité` (rendues linéaires comme VRAD). |
| `_lightHDR` | toutes | Couleur pour la passe HDR ; une valeur négative veut dire « comme `_light` ». |
| `_lightscaleHDR` | toutes | Multiplie l'intensité HDR. |
| `style` | toutes | Style de lumière (0 à 255). Les lumières nommées reçoivent un style commutable si la map n'en a pas. |
| `_constant_attn`, `_linear_attn`, `_quadratic_attn` | `light`, `light_spot` | Atténuation `1 / (c + l d + q d^2)`, l'intensité étant donnée à 100 unités. Toutes à 0 : constante. |
| `_fifty_percent_distance`, `_zero_percent_distance` | `light`, `light_spot` | Courbe d'atténuation passant par ces distances (utilisée à la place des trois atténuations quand la première est réglée). |
| `_hardfalloff` | `light`, `light_spot` | Avec la distance à 50 % : la lumière s'arrête à la distance à 0 %. |
| `_distance` | `light`, `light_spot` | La lumière s'arrête là, en s'estompant sur son dernier quart (Hammer Pro la lit avec les deux types d'atténuation). |
| `_inner_cone`, `_cone`, `_exponent` | `light_spot` | Cônes du spot en degrés (l'intérieur vaut 10 par défaut, l'extérieur vaut l'intérieur) et l'exposant de l'atténuation entre les deux. |
| `angles`, `pitch`, `target` | `light_spot`, `light_environment` | Direction ; une entité `target` la remplace. |
| `_castentityshadow` | toutes | Marque la lumière du monde pour que le moteur projette les ombres des entités depuis elle. |
| `_ambient`, `_ambientHDR`, `_AmbientScaleHDR` | `light_environment` | Lumière ambiante du ciel (la moitié du soleil si elle n'est pas réglée). |
| `SunSpreadAngle` | `light_environment` | Largeur du soleil en degrés : plusieurs rayons de soleil dans cet angle, des ombres plus douces. Seul le premier `light_environment` éclaire la map. |

Les lumières de texture viennent de `lights.rad` et des autres fichiers de lumières ci-dessus, pas de clés d'entité.

## Entités de brushes

| Clé | Effet |
|---|---|
| `vrad_brush_cast_shadows` | `1` : les brushes de cette entité projettent des ombres dans l'éclairage (sinon les entités de brushes n'en projettent pas). |
| `_minlight` | Lumière minimale des faces de l'entité (une fraction, multipliée par 128 en unités VRAD). |

## Props statiques

| Clé | Effet dans l'éclairage |
|---|---|
| `disableshadows` | Le prop ne projette aucune ombre. |
| `disablevertexlighting` | Pas d'éclairage par sommet pour ce prop avec `-StaticPropLighting`. |
| `disableselfshadowing` | Le prop ne se fait pas d'ombre à lui-même. |
| `ignorenormals` | Le prop est éclairé avec un cosinus constant. |
| `lightingorigin` | L'entité dont la position éclaire le prop (aussi utilisée pour les sommets dans le solide). |
| `fademindist`, `fademaxdist`, `screenspacefade` | Écrites dans le lump des props statiques ; les statistiques listent les props sans distance de disparition. |

## Autres entités traitées par le compilateur

| Entité ou clé | Effet |
|---|---|
| `func_detail` | Brushes de détail : elles ne découpent pas la visibilité (sauf avec `-fulldetail`). |
| `func_viscluster` | Force les feuilles qu'elle contient dans un seul cluster de visibilité. |
| `func_areaportal` | Sépare les zones (voir [Fuites et pointfiles](../features/leaks-pointfiles.md)). |
| `func_occluder` | Ses brushes deviennent des polygones d'occultation. |
| `env_fog_controller` `farz` | Le premier fixe le rayon de visibilité (comme `-radius_override`). |
| `env_cubemap` `cubemapsize`, `sides`, `parallaxobb` | Taille de la cubemap, les faces qui lui sont attribuées, la brush `parallax_obb` des cubemaps à correction de parallaxe. Voir [Cubemaps](cubemaps.md). |
| `func_instance` | Fusionnée dans la map avant tout le reste, instances imbriquées comprises. Voir [Instances](../features/instances.md). |
| `prop_detail`, `prop_detail_sprite`, `detail_prop` | Des objets de détail en plus de ceux des matériaux `%detailtype`. |
| `sky_camera` | Les rayons qui atteignent le ciel sont retracés à travers la skybox 3D depuis elle, comme VRAD. |
| `mindxlevel`, `maxdxlevel` | Avec `-dxlevel`, les entités hors du niveau sont retirées. |

## FAQ

**Ma map Hammer++ a une clé `_lightsrad`, est-ce que ça marche ?** Oui, le fichier est lu comme Hammer++ le prévoit, et le journal affiche `Map light file (_lightsrad): ...`.

## Pages liées
- [Options de compilation](compile-options.md)
- [Éclairage](../guides/lighting.md)
- [Propriétés des entités](../features/entity-properties.md)
- [Cubemaps](cubemaps.md)

---
[Retour à l'index de la documentation](../README.md)
