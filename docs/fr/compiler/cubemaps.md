[Documentation](../README.md) > Compilateur > Cubemaps | [English](../../en/compiler/cubemaps.md) | [Русский](../../ru/compiler/cubemaps.md)

# Cubemaps

Les entités `env_cubemap` donnent aux matériaux réfléchissants (`$envmap env_cubemap`) l'image qu'ils reflètent. Hammer Pro les prépare à la compilation comme VBSP, peut les construire en jeu pendant une sortie, et peut aussi écrire directement dans la map compilée les cubemaps qu'il dessine dans l'éditeur.

## Ce que fait le compilateur

Pendant l'étape de géométrie :

- Chaque `env_cubemap` devient un échantillon de cubemap du BSP (et quitte le lump des entités). Une map contient au plus 1024 échantillons.
- Chaque face dont le matériau utilise `$envmap env_cubemap` reçoit une copie patchée de son matériau, `materials/maps/<map>/<matériau>_<x>_<y>_<z>`, qui pointe vers la cubemap de l'échantillon le plus proche devant elle, ou celui qu'un `env_cubemap` a choisi dans sa clé **sides**. Les matériaux désignés par `$bottommaterial`, `$crackmaterial` et `$fallbackmaterial` sont patchés aussi s'ils utilisent la cubemap.
- `cubemapsize` : 0 donne la taille par défaut, sinon la cubemap fait 2^(taille-1) texels de côté.
- Le pakfile reçoit des cubemaps provisoires noires de 32x32 pour les reflets, en LDR et HDR, avant la construction des vraies cubemaps.
- Le pakfile reste stocké (non compressé) : le moteur ne construit les cubemaps que dans des pakfiles stockés.

## Construire les vraies cubemaps

Les matériaux de ciel LDR, HDR et HDR compressé sont reconnus, y compris les faces `_hdr`. Si les fichiers du ciel demandé sont absents ou invalides, le compilateur utilise un ciel standard disponible et indique son nom dans le journal. Seul le BSP reçoit ce `skyname` de remplacement ; le VMF source reste inchangé. Montez le contenu du ciel d'origine pour conserver son apparence. Les ciels procéduraux `painted` sont préservés.

Une cubemap de réflexion manquante ne coupe pas l'éclairage des surfaces. Une passe d'éclairage entièrement noire produit son propre avertissement : vérifiez les lumières, `light_environment`, les ouvertures vers le ciel et les intensités HDR.

Quatre façons :

0. **À la compilation** : **Construire les cubemaps à la compilation** dans la fenêtre de compilation (ligne **Cubemaps**). À la fin de la compilation, chaque `env_cubemap` (les vôtres, et les automatiques de `-autocubemaps` et du ray tracing) et la cubemap par défaut sont rendues à partir de la map éclairée (lightmaps finales, textures, props statiques, ciel 2D et skybox 3D, sources de lumière en points brillants, lumière rebondie) et écrites là où `buildcubemaps` le ferait, en LDR et HDR selon la map. La map a des reflets justes dès son premier chargement. Marche avec tous les éclairages : l'éclairage classique de Hammer Pro, son éclairage ray tracé (`-raytracing`), sur la carte graphique ou le processeur, et avec le `vrad.exe` de Valve ou tout autre compilateur externe (Hammer Pro lance alors `hammerpro --cubemaps <map>` sur le BSP juste après l'étape d'éclairage). **Auto** (par défaut) l'active avec le préréglage **Final** et avec le ray tracing en qualité Final ou Optimal ; **Toujours** à chaque compilation qui calcule l'éclairage ; **Désactivé** laisse les substituts à `buildcubemaps`. En ligne de commande : `-buildcubemaps` et `-nobuildcubemaps` sur l'étape d'éclairage (`-rtreflections` et `-rtnoreflections` sont les mêmes options, gardées pour les anciens scripts). C'était **Reflets ray tracés** avant la 2.2.2 : l'ancien réglage est repris une fois.

   Le journal de compilation montre l'avancement pendant le rendu des cubemaps : une ligne quand la scène est prête (triangles, lumières), puis le nombre de cubemaps faites toutes les 10 secondes. Le rendu ne bloque jamais : chaque cubemap reçoit d'abord une passe rapide, puis s'affine jusqu'à sa qualité complète pendant 15 secondes au plus (plus longtemps pour les grandes cubemaps). Une cubemap dont la passe rapide ne finit pas en 90 secondes est sautée et garde sa cubemap provisoire ; le journal la nomme avec son numéro et sa position, par exemple `env_cubemap 12 of 266 at (-2787 -5125 72) (64 px) skipped`, pour la retrouver dans la map ou lancer `buildcubemaps` en jeu pour elle. Après 20 minutes, les cubemaps restantes n'ont que la passe rapide, après 40 minutes elles sont sautées. La dernière ligne compte les cubemaps rendues avec moins d'échantillons et celles sautées. Les empilements de props translucides (rideaux d'eau, feuillages, vitres) ne multiplient plus le travail d'un rayon de reflet.

1. **En jeu, à la main** : chargez la map et tapez `buildcubemaps` dans la console. Le jeu les écrit dans le BSP de son dossier `maps`.
2. **Pendant une sortie** : dans la fenêtre [Construire et enregistrer en contenu](../features/build-content-release.md), **Construire les cubemaps et la navigation en jeu** lance Garry's Mod (après confirmation), construit les cubemaps (LDR et HDR) et le maillage de navigation, puis le ferme. La liste de vérification indique combien sont construites. Désactivé par défaut.
3. **Depuis l'éditeur** : ouvrez la [vue joueur](../features/player-view.md) (`F6`) sur la map compilée avec son éclairage compilé (`L`), laissez les cubemaps se dessiner, puis **Map > Éclairage > Écrire les cubemaps dans la map compilée**. Les cubemaps dessinées par l'éditeur sont écrites dans le pakfile de ce BSP sous les noms qu'utilise `buildcubemaps`, la map est donc livrée avec elles sans lancer le jeu. Elles comptent alors comme construites.

## Les cubemaps construites survivent aux recompilations

Une compilation réécrit les cubemaps provisoires. Pour garder les vôtres :

- L'étape **Copier dans le dossier maps** reprend les cubemaps construites de la map qu'elle remplace dans la nouvelle map, pour chaque `env_cubemap` qui a gardé sa position et sa taille. Le journal indique « Les N fichiers de cubemaps construits en jeu dans ... sont conservés ».
- Une sortie fait de même avec **Garder les cubemaps construites du BSP précédent** (activé par défaut, `keep_cubemaps` dans `[content]`).

Une cubemap compte comme construite quand son fichier diffère de la cubemap provisoire. Déplacez ou redimensionnez un `env_cubemap` et sa cubemap doit être reconstruite.

## Les cubemaps dans les vues de l'éditeur

- **Affichage > Aperçu de l'environnement > Reflets des cubemaps** : les matériaux avec `$envmap` reflètent leur cubemap dans la vue 3D (modes texturé, éclairé et précalculé) et la vue joueur. L'éditeur dessine une cubemap à chaque `env_cubemap` (le ciel s'il n'y en a aucun) ; chaque face prend l'`env_cubemap` le plus proche devant elle comme le compilateur, les props le plus proche. La vue joueur utilise les cubemaps construites en jeu quand la map compilée en a.
- **Redessiner les cubemaps après les modifications** (`[render] cubemaps_auto`, activé par défaut) les redessine un instant après un changement de la map, de son éclairage ou du mode de la vue, quelques faces par image. Désactivé : seulement avec **Map > Éclairage > Reconstruire les cubemaps** (les `env_cubemap` nouveaux ou déplacés ont quand même la leur).

Voir [Aperçu de l'éclairage](../features/lighting-preview.md) pour la partie aperçu.

## Cubemaps à correction de parallaxe dans Garry's Mod

Une cubemap simple est une photo prise d'un seul point : sur un grand sol brillant, le reflet des murs glisse avec la caméra et se trouve au mauvais endroit, d'autant plus qu'on s'éloigne de l'`env_cubemap`. La correction de parallaxe (CS:GO, Mapbase) plie le reflet avec la boîte de la pièce pour qu'il reste là où la pièce est vraiment. Les shaders d'origine de Garry's Mod ne savent pas le faire, alors Hammer Pro met son propre shader dans la map, sans addon.

Activez **Cubemaps à correction de parallaxe** (ligne **Cubemaps** de la fenêtre de compilation, `-parallaxcubemaps` sur l'étape de géométrie). **Désactivé** est la valeur par défaut pour l'instant ; **Auto** l'active avec le ray tracing en qualité Final ou Optimal, **Toujours** pour toute compilation avec le compilateur Hammer Pro. Gardez aussi **Construire les cubemaps à la compilation**, pour que les cubemaps montrent la vraie pièce.

Ce que fait le compilateur :

- Chaque `env_cubemap` reçoit la boîte de sa pièce : des rayons de la cubemap vers les murs, le sol et le plafond (la médiane de neuf rayons par côté, pour qu'un pilier ou une porte ne la réduise pas), ou la brush du `parallax_obb` que nomme sa clé `parallaxobb`. Une cubemap en extérieur ou dans une zone ouverte n'a pas de boîte et garde le reflet simple. Vous pouvez aussi donner la boîte vous même avec la clé `hp_pcbox` "minx miny minz maxx maxy maxz".
- Chaque face réfléchissante du monde (`LightmappedGeneric` avec `$envmap env_cubemap`, ni translucide, ni en alpha test, ni mélange, ni displacement) dont la cubemap a une boîte et qui est dans cette boîte garde son matériau pour la lumière diffuse et la lightmap (son éclairage ne change pas), avec le reflet d'origine coupé (`$envmaptint 0` dans son matériau patché), et reçoit un overlay dessiné avec `screenspace_general` et le shader de cubemap projetée sur boîte de Hammer Pro (`shaders/fxc/hp_boxcube_vs20.vcs` et `hp_boxcube_ps20b.vcs` dans le pakfile). L'overlay ajoute le reflet : le rayon de vue réfléchi sur la face est coupé par la boîte de la pièce et la cubemap est lue vers ce point depuis l'`env_cubemap`. Il garde `$envmaptint`, `$envmapcontrast`, `$fresnelreflection` et le masque du matériau (`$envmapmask`, `$basealphaenvmapmask`, `$normalmapalphaenvmapmask`), et s'efface dans le brouillard.
- Les faces coplanaires du même matériau et de la même cubemap partagent un overlay (64 faces au plus par overlay, la limite du moteur). Si la map dépassait la limite d'overlays (`MAX_MAP_OVERLAYS`), des matériaux entiers gardent le reflet simple et le journal le dit.
- Le journal dit combien de cubemaps ont reçu une boîte et combien de faces le reflet corrigé.

Ce que ça peut et ne peut pas faire (honnêtement) :

- Le reflet est exact pour ce qui est sur les murs, le sol et le plafond d'une pièce en forme de boîte : un sol brillant reflète les murs, les affiches et le mur du fond là où ils sont vraiment, depuis n'importe quelle position de caméra. Pour les props, piliers et meubles dans la pièce, et pour les pièces loin d'une boîte (en L, en pente, voûtées), le reflet est plus juste qu'une cubemap simple mais pas exact : c'est la même limite que CS:GO et Mapbase.
- Le reflet est plat : la normal map façonne toujours la lumière diffuse mais ne fait plus onduler le reflet comme le shader d'origine. `$envmapsaturation` n'est pas appliqué.
- Seulement les faces de brushes du monde : displacements, mélanges `WorldVertexTransition`, modèles et props, eau et verre translucide gardent le reflet d'origine.
- Il faut le compilateur Hammer Pro (les faces sont préparées par son étape de géométrie) ; avec le `vbsp.exe` de Valve seule **Construire les cubemaps à la compilation** s'applique.
- Il faut un Garry's Mod qui charge les shaders de la map (le Garry's Mod actuel le fait). Là où un shader personnalisé ne peut pas tourner (mode DirectX 8), ces faces n'ont pas de reflet.

### Mapbase et autres shaders capables de parallaxe

Comme le VBSP de Mapbase : donnez à un `env_cubemap` une clé `parallaxobb` qui désigne une entité de brush `parallax_obb`. Le compilateur patche ses matériaux avec `$envMapParallax 1`, `$envMapParallaxOBB1` à `3` (la boîte de la brush) et `$envMapOrigin` (la position de l'échantillon), puis retire les entités `parallax_obb` et leurs brushes. Les shaders de Mapbase (et des autres jeux aux shaders capables de parallaxe) corrigent le reflet avec ces clés ; les shaders d'origine des autres jeux les ignorent. Un mod Mapbase est reconnu dans **Paramètres > Jeu**.

## FAQ

**Mes reflets sont noirs en jeu.** Les cubemaps provisoires sont noires tant que les cubemaps ne sont pas construites : compilez avec **Construire les cubemaps à la compilation**, lancez `buildcubemaps`, utilisez la session de jeu de la sortie, ou écrivez-les depuis l'éditeur. Un `env_cubemap` dans une brush solide donne aussi une cubemap noire ([Vérifier les problèmes](../features/check-problems.md) le signale).

**La construction des cubemaps semble bloquée.** Regardez le journal de compilation : les cubemaps donnent leur avancement toutes les 10 secondes et toute l'étape est limitée dans le temps. Une cubemap très lente à rendre (dans un grand prop, derrière beaucoup de couches translucides, près de centaines de lumières) est moins affinée ou sautée, et le journal la nomme avec sa position.

**J'ai construit les cubemaps en jeu, puis recompilé, et elles ont disparu.** Seuls les `env_cubemap` qui ont gardé leur position et leur taille gardent leur cubemap, et seulement par la copie dans le dossier maps (ou l'option de sortie).

## Pages liées
- [Aperçu de l'éclairage](../features/lighting-preview.md)
- [Vue joueur](../features/player-view.md)
- [Construire et enregistrer en contenu](../features/build-content-release.md)
- [Clés worldspawn et lumières](worldspawn-keys.md)

---
[Retour à l'index de la documentation](../README.md)
