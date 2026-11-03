[Documentation](../README.md) > Guides > Éclairage et occlusion ambiante | [English](../../en/guides/lighting-and-ao.md)

# Éclairage et occlusion ambiante

Ce guide éclaire une map de la première lumière jusqu'à la compilation finale : placer les lumières, les prévisualiser dans l'éditeur (en temps réel puis précalculées), compiler en LDR et HDR, ajouter l'occlusion ambiante et éclairer les props statiques.

## Étape 1 : placer les lumières

Placez des entités de lumière avec l'outil entité :

| Entité | Usage |
|---|---|
| `light` | Une lumière ponctuelle qui éclaire dans toutes les directions. |
| `light_spot` | Un spot avec un cône intérieur et un cône extérieur, orienté par ses angles. |
| `light_environment` | Le soleil et la lumière du ciel, à travers les faces texturées en ciel. Une seule par map. |
| `light_dynamic` | Une lumière calculée par le jeu en temps réel (non précalculée). |

Les textures lumineuses (matériaux qui émettent de la lumière, listés dans `lights.rad`) éclairent aussi la map à la compilation. La couleur et l'intensité d'une lumière sont dans sa clé `_light` : sélectionnez la lumière et appuyez sur `C` (**Édition > Modifier la couleur...**) pour choisir la couleur.

## Étape 2 : aperçu en temps réel (mode éclairé)

Appuyez sur `Alt+5` (**Affichage > Mode de rendu 3D > Aperçu de l'éclairage**). La vue 3D dessine les entités de lumière avec la même atténuation et les mêmes cônes que le compilateur, les `light_dynamic`, et le soleil de la première `light_environment` avec ses ombres douces, qui n'éclaire que ce qui voit le ciel comme VRAD, props compris. Sa lumière du ciel `_ambient` n'atteint que ce qui voit le ciel (paramètre `render.sky_occlusion`), et les valeurs de lumière HDR sont utilisées comme le jeu sur une map HDR (paramètre `render.light_preview_hdr`). Les lumières ponctuelles et spots en vue projettent aussi des ombres, les plus brillantes d'abord : leur nombre est le paramètre `render.light_shadows` (0 à 64, 16 par défaut, 0 pour le soleil seul). La lumière du ciel et celle que les surfaces se renvoient viennent d'une cuisson de la map en arrière-plan avec le compilateur (paramètre `render.lit_indirect`, activé par défaut), comme la part ambiante des lightmaps du jeu : les pièces éclairées par la lumière rebondie ressemblent donc au jeu ; les lumières et le soleil restent en temps réel et suivent vos déplacements, la part rebondie suit un instant après une modification de la map.

## Étape 3 : aperçu précalculé

Pour le vrai rendu de la map compilée (lumière rebondie, lightmaps, props éclairés comme en jeu) :

1. Appuyez sur `Ctrl+F5` (**Affichage > Mode de rendu 3D > Aperçu de l'éclairage précalculé**).
2. Lancez le précalcul : **Map > Éclairage > Précalculer l'éclairage** (toute la map) ou **Map > Éclairage > Précalculer autour de la caméra** (seulement ce qui se trouve dans le **Rayon autour de la caméra**, plus rapide sur les grandes maps). **Map > Éclairage > Arrêter le précalcul** l'arrête.
3. Ouvrez l'onglet **Éclairage** (**Affichage > Panneaux > Éclairage**) pour régler l'aperçu :
   - **Qualité** : **Brouillon** (mêmes niveaux de lumière, ombres plus bruitées), **Normal**, **Finale** (la plus lente).
   - **Lumière rebondie** : la lumière renvoyée par les surfaces (précalculs plus lents).
   - **Mises à jour dynamiques** avec un **Délai de mise à jour** : relance le précalcul tout seul quand vous arrêtez de modifier.
   - **Budget des lightmaps** : la taille estimée des lightmaps face aux 2 Go qu'une face peut y adresser, avec les faces les plus coûteuses.
4. L'aperçu suit la plage dynamique des réglages de compilation (LDR ou HDR).

Les fils de calcul et le budget de luxels de l'aperçu se règlent dans la page **Éclairage** des paramètres. Détails : [Aperçu de l'éclairage](../features/lighting-preview.md).

## Étape 4 : échelle de lightmap

L'échelle de lightmap d'une face est son nombre d'unités par luxel : plus bas donne un éclairage plus fin et une map plus lourde. Réglez-la par face dans l'outil texture, ou par displacement dans l'outil displacement. **Affichage > Mode de rendu 3D > Grille de lightmap** (`Alt+8`) montre les luxels colorés selon l'échelle, et **Map > Éclairage > Sélectionner les faces à petite échelle de lightmap** trouve les faces qui coûtent trop cher.

## Étape 5 : compiler en LDR et HDR

Dans **Fichier > Compiler et lancer...** (`Ctrl+F9`), la ligne **Plage dynamique** règle ce que calcule l'éclairage :

| Choix | Effet |
|---|---|
| **LDR** | Plage dynamique standard seule : la plus rapide, ce que voient la plupart des joueurs. Par défaut. |
| **HDR** | Haute plage dynamique seule. |
| **LDR + HDR** | Les deux (deux passes), nécessaire pour une publication. |

**Final** calcule toujours les deux, **Brouillon** une seule. Le **Préréglage** fixe la qualité : Brouillon utilise `-bounce 1 -fast`, Final utilise `-final -StaticPropLighting`. Pour juger rapidement le vrai éclairage, compilez en **Normal** ; pour la publication, en **Final**.

## Étape 6 : occlusion ambiante

L'éclairage de Hammer Pro ajoute une occlusion ambiante, que VRAD n'a pas pour les lightmaps : des ombres de contact dans les coins, les plis, au pied des murs et sous les props. Elle n'assombrit que la lumière rebondie près de la géométrie proche ; la lumière directe et celle du ciel gardent leurs propres ombres.

Par défaut, elle est **activée en Normal et Final, désactivée en Brouillon**. Réglez-la pour chaque map dans **Map > Propriétés de la map...** (ces clés s'affichent quel que soit le FGD) :

| Propriété de la map | Clé | Effet | Par défaut |
|---|---|---|---|
| **Occlusion ambiante (Hammer Pro)** | `_ao` | **Selon la compilation**, **Activée** ou **Désactivée**. | Selon la compilation |
| **Rayon de l'occlusion ambiante** | `_aoradius` | Distance jusqu'à laquelle la géométrie assombrit une surface, en unités (4 à 1024). | 96 |
| **Force de l'occlusion ambiante** | `_aostrength` | De 0 (aucune) à 1 (un coin fermé perd toute sa lumière rebondie). | 0.75 |
| **Rayons de l'occlusion ambiante** | `_aosamples` | Rayons par luxel et par sommet de prop statique (4 à 512). Plus est plus lisse et plus lent. | 32, 64 en Final |

Ou pour une compilation, avec les options d'éclairage de la fenêtre de compilation (le menu **Toutes les options d'éclairage (hammerpro --rad)** ou les **Arguments supplémentaires de l'éclairage**) : `-ao`, `-noao`, `-aoradius N`, `-aostrength N`, `-aosamples N`. Ces options priment sur les clés de la map.

Astuces : un rayon plus petit garde les ombres serrées dans les coins, un rayon plus grand assombrit des zones plus larges autour des grosses formes. Baissez la force si les coins paraissent sales.

## Étape 7 : éclairer les props statiques

Dans la section **Props statiques** de la fenêtre de compilation :

- **Éclairer les props statiques (par sommet)** : quand la qualité le demande (Final), l'éclairage précalcule la lumière de chaque sommet des props statiques (`-StaticPropLighting`) : ombres, lumière rebondie et occlusion ambiante sur les props. Lent sur les grandes maps. Décoché : le jeu éclaire lui-même les props.
- **Ombres des props statiques** : les props projettent des ombres dans l'éclairage. Décoché : aucune ombre de prop, et leurs modèles ne sont même pas chargés (`-nostaticpropshadows`), bien plus rapide sur les maps avec beaucoup de props.

Vous pouvez aussi forcer `-StaticPropLighting` avec une compilation Normal via le menu des options d'éclairage. Chaque option est décrite dans [Options de compilation](../compiler/compile-options.md).

## FAQ

**L'aperçu précalculé et le jeu ne se ressemblent pas.** Vérifiez que l'aperçu et la compilation utilisent la même plage dynamique, et que le jeu n'affiche du HDR que si vous avez compilé en HDR.

**Les coins sont trop sombres.** Baissez **Force de l'occlusion ambiante**, ou mettez **Occlusion ambiante (Hammer Pro)** sur **Désactivée** pour cette map.

**Les props sont noirs ou plats.** Compilez en **Final** (ou forcez `-StaticPropLighting`) pour que les sommets des props reçoivent un éclairage précalculé, et gardez **Ombres des props statiques** activé.

## Pages liées

- [Aperçu de l'éclairage](../features/lighting-preview.md)
- [Options de compilation](../compiler/compile-options.md)
- [Clés du worldspawn](../compiler/worldspawn-keys.md)
- [Fenêtre de compilation](../compiler/compile-window.md)
- [Cubemaps](../compiler/cubemaps.md)
- [Vue joueur](../features/player-view.md)

---
[Retour à l'index de la documentation](../README.md)
