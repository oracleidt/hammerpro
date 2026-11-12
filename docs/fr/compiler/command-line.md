[Documentation](../README.md) > Compilateur > Ligne de commande | [English](../../en/compiler/command-line.md) | [Русский](../../ru/compiler/command-line.md)

# Ligne de commande du compilateur

Le compilateur fait partie de `hammerpro.exe`. `hammerpro --bspc` construit la géométrie, les portails, la recherche de fuites et la visibilité d'un VMF ; `hammerpro --rad` éclaire le BSP. Les deux reprennent les noms d'options de Valve (VBSP et VVIS pour `--bspc`, VRAD pour `--rad`), donc les scripts et les séquences de commandes de Hammer se transposent directement. Les autres arguments de `hammerpro.exe` sont dans la [référence de la ligne de commande](../reference/command-line.md).

## Utilisation

```
hammerpro --bspc [options] <map>
hammerpro --rad  [options] <map>
```

- `<map>` est le chemin de la map, avec ou sans extension. `--bspc` lit `<map>.vmf` ; `--rad` éclaire `<map>.bsp` sur place. La map vient en dernier (tout argument qui ne commence pas par `-` est la map).
- La casse des options ne compte pas. Une option qui attend une valeur et n'en a pas, ou une valeur qui n'est pas un nombre, arrête tout avec `Error: ...` et le code de sortie 1.
- Chaque ligne de sortie est affichée sur la sortie standard et ajoutée à `<map>.log` à côté de la map (fins de ligne CRLF, comme les outils de Valve).
- Code de sortie 0 en cas de succès, 1 pour toute erreur (et pour une fuite avec `-leaktest`).

## Fichiers écrits

| Fichier | Par | Quand |
|---|---|---|
| `<map>.bsp` | `--bspc`, réécrit par `--rad` | toujours, sauf une fuite avec `-leaktest` |
| `<map>.prt` | `--bspc` | maps étanches seulement |
| `<map>.lin` | `--bspc` | chemin de fuite ; ou la ligne de vue avec `-trace` |
| `<map>.log` | les deux | ajouté à la suite |
| `materials/maps/<map>/...` dans le pakfile | `--bspc` | matériaux patchés et cubemaps provisoires (voir [Cubemaps](cubemaps.md)) |

## Options de `--bspc`

Géométrie et visibilité. Les valeurs par défaut sont celles de la ligne de commande seule ; l'éditeur ajoute les siennes (voir [Options de compilation](compile-options.md)).

| Option | Valeur | Effet | Par défaut |
|---|---|---|---|
| `-game`, `-vproject` | dossier | Dossier du jeu : matériaux, modèles, `lights.rad`, `detail.vbsp`, instances. | `garrysmod` de Garry's Mod |
| `-gmod` | dossier | Installation de Garry's Mod dont le contenu est monté. | l'installation détectée |
| `-target` | id de jeu | Le jeu de la map : son format BSP et ses limites (voir [Jeux cibles](target-games.md)). | Garry's Mod, ou le jeu de `-game` |
| `-insert_search_path` | dossier | Un autre dossier de jeu monté comme `-game` (répétable). | aucun |
| `-threads` | nombre | Threads de travail (1 ou plus). | tous les cœurs |
| `-low` | | Priorité basse. | désactivé |
| `-v`, `-verbose` | | Affiche les options. | désactivé |
| `-leaktest` | | Arrêt sur fuite : écrit `<map>.lin`, pas de BSP, code de sortie 1. | désactivé : une map qui fuit a quand même son BSP |
| `-vis` | | Visibilité complète. | désactivé : pas de visibilité |
| `-fastvis` | | Visibilité rapide et grossière. | désactivé |
| `-fast` | | Le `-fast` de VVIS : avec `-vis`, la visibilité rapide. | désactivé |
| `-forceskyvis`, `-skyvis` | | Visibilité aussi dans la skybox 3D. | désactivé |
| `-onlyents` | | Met à jour seulement les entités du `<map>.bsp` existant. | désactivé |
| `-onlyprops` | | Met à jour seulement les props statiques et de détail du BSP existant. | désactivé |
| `-nodetail` | | Retire les brushes `func_detail`. | désactivé |
| `-nowater` | | Retire les brushes d'eau. | désactivé |
| `-fulldetail` | | Les brushes de détail découpent la visibilité comme les structurelles. | désactivé |
| `-micro` | volume | Seuil de l'avertissement microbrush. | 1 |
| `-blocksize` | unités | 0 (aucun bloc) ou de 128 à 32768 ; une autre valeur est ignorée avec une note. | 1024 |
| `-luxelscale` | facteur | Multiplie chaque échelle de lightmap. | 1 |
| `-minluxelscale` | échelle | Plus petite échelle de lightmap (au moins 1). | 1 |
| `-defaultluxelsize` | échelle | Échelle des faces dont l'échelle de lightmap vaut 0 (1 ou plus). | 16 |
| `-maxlightmapdim` | luxels | Plus grande lightmap d'une face, de 1 à 125 (les autres jeux la limitent à 32, avec un avertissement). | 32 |
| `-bumpall` | | Lightmaps bumpées sur toutes les surfaces éclairées. | désactivé |
| `-lightifmissing` | | Lightmaps pour les matériaux manquants ou non éclairés. | désactivé |
| `-nodrawtriggers` | | Les faces des triggers ne sont pas dessinées. | désactivé |
| `-AllowDynamicPropsAsStatic` | | Garde les props statiques dont le modèle a un `prop_data`. | désactivé |
| `-replacematerials` | | Applique `cfg/materialsub.cfg` du jeu. | désactivé |
| `-keepstalezip` | | Garde les fichiers empaquetés dans le BSP précédent. | désactivé |
| `-embed` | dossier | Empaquette un dossier entier dans le BSP (compilations complètes seulement, refusé avec `-onlyents`/`-onlyprops`). | aucun |
| `-dxlevel` | niveau | Retire les entités exclues par leur `mindxlevel`/`maxdxlevel`. | aucun |
| `-radius_override` | unités | Rayon de visibilité. | `farz` du brouillard, sinon aucun |
| `-nosort` | | Visibilité sans trier les portails. | désactivé |
| `-viscache` | dossier | Garde la visibilité dans ce dossier et la reprend quand une compilation suivante a les mêmes portails et options de visibilité (l'éditeur passe son cache des étapes). | aucun |
| `-trace` | cluster cluster | Écrit la ligne de vue entre deux clusters dans `<map>.lin`. | aucun |
| `-nomerge`, `-nosubdiv`, `-notjunc`, `-noprune` | | Options de débogage, voir [Options de compilation](compile-options.md). | désactivé |
| `-novirtualmesh` | | Collision des displacements en soupes de triangles. | désactivé |
| `-propdebug` | | Rapport des props dont le modèle pose problème. | désactivé |
| `-noaddons` | | Pas d'addons en dossier. | addons montés |
| `-wsaddons` | | Aussi les objets du Workshop abonnés. | désactivé |
| `-nowsaddons` | | Ni addons en dossier ni objets du Workshop. | désactivé |
| `-serveraddons` | | Aussi le contenu téléchargé depuis les serveurs. | désactivé |
| `-mount_appid` | AppID[,AppID] | Monte aussi des jeux Steam installés par AppID (répétable, `-mount_appid 240,440`), trouvés par les bibliothèques Steam comme **Paramètres > Jeu > Jeux Steam par AppID**. | aucun |

Acceptées par compatibilité, sans effet : `-novconfig`, `-allowdebug`, `-steam`, `-verboseentities`, `-tmpout`, `-tmpin`.

Options de Valve connues que ce compilateur ne fait pas (un avertissement, la compilation continue) : `-glview`, `-nocsg`, `-noshare`, `-noweld`, `-noopt`, `-nomergewater`, `-snapaxial`, `-dumpcollide`, `-dumpstaticprop`, `-dumpstaticprops`, `-allowdetailcracks`, `-notjuncsdetail`, `-xbox`, `-fullminidumps`, `-block` (2 valeurs), `-blocks` (4 valeurs), et toutes les options `-mpi...`. Toute autre option inconnue donne `Warning: VBSP: Unknown option "..." (ignored)`.

Sans contenu de jeu (Garry's Mod introuvable et pas de `-game`), les matériaux sont devinés d'après leur nom et le journal le signale.

## Options de `--rad`

Éclairage. Sans option de plage, `--rad` ne calcule que le LDR.

| Option | Valeur | Effet | Par défaut |
|---|---|---|---|
| `-ldr` / `-hdr` / `-both` | | Plages dynamiques à calculer. | LDR |
| `-game` | dossier | Dossier du jeu : `lights.rad`, matériaux et modèles. | `garrysmod` de l'installation |
| `-gmod` | dossier | Installation de Garry's Mod. | l'installation détectée |
| `-target` | id de jeu | Le jeu de la map (ses limites et son contenu). | Garry's Mod |
| `-insert_search_path` | dossier | D'autres dossiers de jeu pour les modèles. | aucun |
| `-threads` | nombre | Threads de travail. | tous les cœurs |
| `-low` | | Priorité basse. | désactivé |
| `-fast` | | Qualité brouillon : peu de rayons, pas d'éclairage par sommet des props, pas de découpage des lumières de texture. | désactivé |
| `-final` | | Qualité finale (plus de rayons du ciel et indirects, rayons des props multipliés par 4). | désactivé |
| `-bounce` | nombre | Rebonds au maximum. | 100 |
| `-noextra` | | Pas de suréchantillonnage. | désactivé |
| `-extrasky` | facteur | Plus de rayons du ciel et indirects (lightmaps et props). | aucun |
| `-smooth` | degrés | Angle de lissage. | 45 |
| `-softsun` | degrés | Largeur du soleil pour des ombres plus douces. | 0 |
| `-luxeldensity` | facteur | Lightmaps plus grossières, seules les valeurs sous 1 comptent (les autres donnent un avertissement). | 1 |
| `-noskyboxrecurse` | | Ni ombre ni lumière à travers la skybox 3D. | désactivé |
| `-fastambient` | | Ambiance des props plus rapide. | désactivé |
| `-centersamples` | | Accepté (toujours fait). | |
| `-lights` | fichier | Fichier de lumières de texture en plus. | aucun |
| `-chop` / `-maxchop` / `-dispchop` | luxels | Tailles des morceaux de lumière de texture, au moins 1. | 4 / 4 / 8 |
| `-StaticPropLighting` | | Éclairage par sommet des props statiques. | désactivé |
| `-StaticPropPolys` | | Ombres des props avec leurs triangles affichés. | désactivé |
| `-textureshadows` | | Ombres des props à transparence (avec `-StaticPropPolys`). | désactivé |
| `-nossprops` | | Pas d'ombre d'un prop sur lui-même. | désactivé |
| `-staticpropsamplescale` | facteur | Plus ou moins de rayons pour les props (plus que 0). | 1 |
| `-StaticPropNormals` | | Débogage : les props montrent leurs normales. | désactivé |
| `-nostaticpropshadows` | | Hammer Pro : aucune ombre de prop statique. | désactivé |
| `-nodispshadows` | | Hammer Pro : aucune ombre de displacement. | désactivé |
| `-nostylebounce` | | Hammer Pro : les lumières nommées (commutables) ne rebondissent pas, comme dans VRAD. | désactivé |
| `-noaddons`, `-wsaddons`, `-nowsaddons`, `-serveraddons`, `-mount_appid` | | Contenu monté pour les modèles, comme `--bspc`. | addons et objets du Workshop montés |
| `-radius_override` | unités | Accepté et ignoré (une option de VVIS souvent passée à tous les outils). | |

Toute autre option donne `Warning: option ... is not supported by this lighting compiler: ignored` ; les options connues de VRAD qui prennent une valeur (`-vproject`, `-aoscale`, `-aofacesamples`, `-aopropsamples`, `-ambientocclusion`, `-sunsamplesarealight`, `-leafambientsamplereduction`, `-maxdispsamplesize`, `-disppatchradius`, `-mpi_workercount`, `-dumptrace`) sautent aussi leur valeur.

Les fichiers de lumières de texture sont lus dans cet ordre : `lights.rad` du jeu (le dossier du jeu, puis les dossiers de l'installation de Garry's Mod, ses dossiers `bin` en dernier), le fichier de `-lights`, le fichier désigné par la clé worldspawn `_lightsrad`, puis `<map>.rad` à côté de la map.

Pendant l'éclairage, `--rad` annonce ses étapes (`Lighting stages: direct lighting, bounce rays, bounced light, leaf ambient, static props, writing`), puis rend compte de chaque étape qui dure plus d'une seconde :

```
Lighting 41%: direct lighting 60% (stage 2 of 7), 27 s elapsed, about 17 s left
```

Le premier pourcentage est celui de tout l'éclairage, le second celui de l'étape. Une ligne arrive tous les 5 % et au moins toutes les 30 secondes. Une étape qui n'a pas avancé depuis une minute écrit `Warning: the lighting stage "..." has not advanced for ...` avec ce qui rend cette étape lente, et une étape qui dure plus de dix minutes écrit `Warning: the lighting stage "..." has run for ...`. L'éditeur lit ces lignes pour sa barre de progression. À la fin, `--rad` indique où est passé le temps, en secondes réelles (une phase qui n'a pas tourné affiche 0.00 s) :

```
Light phase times: setup 0.15 s, faces 0.63 s, static prop models 0.41 s, shadow tree 1.56 s, surface tree 0.19 s, blocker tree 0.04 s, direct light 13.50 s, displacement blend 1.24 s, displacement bounce blend 1.00 s, bounce rays 3.23 s, bounce passes 3.02 s, seams 0.31 s, leaf ambient 0.45 s, write 0.77 s
```

`direct light` comprend les lumières, le soleil et le ciel, `static prop models` la lecture des modèles des props (seuls les modèles des props qui projettent une ombre ou reçoivent l'éclairage par sommet sont lus), `seams` le raccord des luxels partagés par des faces voisines ; `light probes` et `detail props` apparaissent quand ils tournent. Les problèmes trouvés par la vérification finale qui étaient déjà dans le BSP avant l'éclairage (une map compilée par d'autres outils) sont des avertissements ; seuls les nouveaux empêchent l'écriture.

## Exemples

Un test rapide d'une map Garry's Mod :

```
hammerpro --bspc -fastvis -leaktest -game "D:\Steam\steamapps\common\GarrysMod\garrysmod" C:\maps\gm_test
hammerpro --rad -ldr -fast -bounce 1 C:\maps\gm_test
```

Une version finale :

```
hammerpro --bspc -vis -leaktest C:\maps\gm_test.vmf
hammerpro --rad -both -final -StaticPropLighting C:\maps\gm_test
```

La même map pour Team Fortress 2 :

```
hammerpro --bspc -target tf -vis C:\maps\ctf_test
hammerpro --rad -target tf -both C:\maps\ctf_test
```

Seules les entités ont changé :

```
hammerpro --bspc -onlyents C:\maps\gm_test
```

## FAQ

**Ma map n'a pas de visibilité après `hammerpro --bspc`.** La ligne de commande ne calcule la visibilité qu'avec `-vis` ou `-fastvis` (il n'y a pas d'étape VVIS séparée).

**Puis-je garder mon ancien fichier batch VBSP/VVIS/VRAD ?** Remplacez `vbsp.exe` par `hammerpro --bspc`, supprimez la ligne de `vvis.exe` et ajoutez `-vis` (ou `-fastvis`) à la ligne `--bspc`, remplacez `vrad.exe` par `hammerpro --rad`.

**Où est le journal de compilation ?** `<map>.log` à côté de la map, complété à chaque lancement.

## Pages liées
- [Options de compilation](compile-options.md)
- [Référence de la ligne de commande](../reference/command-line.md)
- [Jeux cibles](target-games.md)
- [Vue d'ensemble du compilateur](overview.md)

---
[Retour à l'index de la documentation](../README.md)
