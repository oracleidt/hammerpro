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

Trois façons :

1. **En jeu, à la main** : chargez la map et tapez `buildcubemaps` dans la console. Le jeu les écrit dans le BSP de son dossier `maps`.
2. **Pendant une sortie** : dans la fenêtre [Construire et enregistrer en contenu](../features/build-content-release.md), **Construire les cubemaps et la navigation en jeu** lance Garry's Mod (après confirmation), construit les cubemaps (LDR et HDR) et le maillage de navigation, puis le ferme. La liste de vérification indique combien sont construites. Désactivé par défaut.
3. **Depuis l'éditeur** : ouvrez la [vue joueur](../features/player-view.md) (`F6`) sur la map compilée avec son éclairage compilé (`L`), laissez les cubemaps se dessiner, puis **Map > Éclairage > Écrire les cubemaps dans la map compilée**. Les cubemaps dessinées par l'éditeur sont écrites dans le pakfile de ce BSP sous les noms qu'utilise `buildcubemaps`, la map est donc livrée avec elles sans lancer le jeu. Elles comptent alors comme construites.

## Les cubemaps construites survivent aux recompilations

Une compilation réécrit les cubemaps provisoires. Pour garder les vôtres :

- L'étape **Copier dans le dossier maps** reprend les cubemaps construites de la map qu'elle remplace dans la nouvelle map, pour chaque `env_cubemap` qui a gardé sa position et sa taille. Le journal indique « Les N fichiers de cubemaps construits en jeu dans ... sont conservés ».
- Une sortie fait de même avec **Garder les cubemaps construites du BSP précédent** (activé par défaut, `keep_cubemaps` dans `[content]`).

Une cubemap compte comme construite quand son fichier diffère de la cubemap provisoire. Déplacez ou redimensionnez un `env_cubemap` et sa cubemap doit être reconstruite.

## Les cubemaps dans les vues de l'éditeur

- **Affichage > Options d'affichage > Reflets des cubemaps** : les matériaux avec `$envmap` reflètent leur cubemap dans la vue 3D (modes texturé, éclairé et précalculé) et la vue joueur. L'éditeur dessine une cubemap à chaque `env_cubemap` (le ciel s'il n'y en a aucun) ; chaque face prend l'`env_cubemap` le plus proche devant elle comme le compilateur, les props le plus proche. La vue joueur utilise les cubemaps construites en jeu quand la map compilée en a.
- **Redessiner les cubemaps après les modifications** (`[render] cubemaps_auto`, activé par défaut) les redessine un instant après un changement de la map, de son éclairage ou du mode de la vue, quelques faces par image. Désactivé : seulement avec **Map > Éclairage > Reconstruire les cubemaps** (les `env_cubemap` nouveaux ou déplacés ont quand même la leur).

Voir [Aperçu de l'éclairage](../features/lighting-preview.md) pour la partie aperçu.

## Cubemaps à correction de parallaxe

Comme le VBSP de Mapbase : donnez à un `env_cubemap` une clé `parallaxobb` qui désigne une entité de brush `parallax_obb`. Le compilateur patche ses matériaux avec `$envMapParallax 1`, `$envMapParallaxOBB1` à `3` (la boîte de la brush) et `$envMapOrigin` (la position de l'échantillon), puis retire les entités `parallax_obb` et leurs brushes. Les shaders de Mapbase (et des autres jeux aux shaders capables de parallaxe) corrigent le reflet avec ces clés ; les shaders d'origine des autres jeux les ignorent. Un mod Mapbase est reconnu dans **Paramètres > Jeu**.

## FAQ

**Mes reflets sont noirs en jeu.** Les cubemaps provisoires sont noires tant que les cubemaps ne sont pas construites : lancez `buildcubemaps`, utilisez la session de jeu de la sortie, ou écrivez-les depuis l'éditeur. Un `env_cubemap` dans une brush solide donne aussi une cubemap noire ([Vérifier les problèmes](../features/check-problems.md) le signale).

**J'ai construit les cubemaps en jeu, puis recompilé, et elles ont disparu.** Seuls les `env_cubemap` qui ont gardé leur position et leur taille gardent leur cubemap, et seulement par la copie dans le dossier maps (ou l'option de sortie).

## Pages liées
- [Aperçu de l'éclairage](../features/lighting-preview.md)
- [Vue joueur](../features/player-view.md)
- [Construire et enregistrer en contenu](../features/build-content-release.md)
- [Clés worldspawn et lumières](worldspawn-keys.md)

---
[Retour à l'index de la documentation](../README.md)
