[Documentation](../README.md) > Fonctionnalités > Aperçu de l'éclairage | [English](../../en/features/lighting-preview.md)

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
- **Lumière du ciel** : la couleur `_ambient` de la `light_environment` n'éclaire que ce qui voit le ciel, comme VRAD : un sol à découvert la reçoit en entier, un mur à moitié, les intérieurs et les endroits couverts pas du tout. **Outils > Options... > Rendu > Lumière du ciel seulement sous le ciel** (`render.sky_occlusion`, activé). Désactivé, toute la map la reçoit.
- **Valeurs de lumière HDR** : avec **Outils > Options... > Rendu > Valeurs de lumière HDR** (`render.light_preview_hdr`, activé), les lumières prennent `_lightHDR` (ou `_light` quand elle vaut `-1 -1 -1`) multiplié par `_lightscaleHDR`, et le ciel `_ambientHDR` multiplié par `_AmbientScaleHDR`, comme le jeu sur une map compilée en HDR. Désactivé : `_light` et `_ambient`. Une valeur de lumière peut être `r g b`, `r g b luminosité`, un seul gris, ou les huit nombres `r g b i` du LDR puis du HDR.
- **Entités de lumière** : les cônes des spots sont limités à 90 degrés comme VRAD (180 et 180 donnent une lumière ponctuelle), `_fifty_percent_distance`, `_zero_percent_distance`, `_hardfalloff`, `_distance` et les termes `_constant_attn`, `_linear_attn`, `_quadratic_attn` suivent le compilateur, les lumières nommées avec **Initially dark** restent éteintes, et une lumière dans un brush opaque n'éclaire rien.
- **Grandes maps** : toutes les lumières sont dessinées, même par milliers. Là où plus de 128 lumières atteignent le même endroit de la vue, seules les 128 plus fortes à cet endroit l'éclairent. `get_lighting_status` (MCP) donne le nombre de lumières dessinées et d'endroits encombrés.
- **Ombres des lumières** : les lumières ponctuelles et spots les plus proches de la caméra projettent aussi des ombres. **Outils > Options... > Rendu > Lumières avec ombres** (`render.light_shadows`) en fixe le nombre : de 0 à 16, 4 par défaut, 0 pour le soleil seul. Les ombres ne sont redessinées que quand une lumière ou la scène change.
- Les props sont éclairés par les mêmes lumières, le même soleil et la même lumière du ciel que les brushes, au pixel près.
- Avec **Relief (normal maps)** activé (par défaut), les normal maps des matériaux donnent leur relief aux surfaces.

## Aperçu de l'éclairage précalculé (Ctrl+F5)

Comme le « 3D Lighting Preview » de Hammer++, mais avec les chiffres du compilateur : la map est compilée en mémoire par le compilateur de Hammer Pro (l'étape de géométrie) et éclairée par son étape d'éclairage (celle de VRAD) dans un processus séparé. Les faces compilées sont dessinées avec leurs lightmaps (les quatre lightmaps bumpées des faces à normal map, pour le relief), et les props sont éclairés par les ambient cubes, les lumières du monde et l'éclairage par sommets des props statiques de la map éclairée. Les props statiques suivent les réglages de compilation pour leurs ombres et leur éclairage. Un prop est éclairé à son origine d'éclairage comme en jeu : l'entité `lightingorigin` quand la clé en nomme une, sinon le centre du modèle.

- Quand l'aperçu est activé et que rien n'est encore précalculé, un précalcul complet démarre (réglage **Précalculer à l'activation de l'aperçu**).
- Un solide modifié masque ses faces précalculées jusqu'au prochain précalcul. Avec **Mises à jour dynamiques**, la map est précalculée à nouveau quand les modifications s'arrêtent pendant le **Délai de mise à jour**.
- **Précalculer autour de la caméra** compile et éclaire seulement ce qui est à moins du **Rayon autour de la caméra** de la caméra 3D ; la zone est fermée comme un cordon.
- **Arrêter le précalcul** arrête tout de suite (le processus d'éclairage est arrêté et sa mémoire rendue au système).
- L'éclairage tourne avec une priorité plus basse pour que l'éditeur reste fluide. Les grandes maps sont prévisualisées avec des lightmaps plus grossières (réglage **Luxels en pleine résolution**), et une note l'indique (« Grande carte : aperçu à N fois la résolution des lightmaps »).
- HDR ou LDR vient des réglages de compilation : l'onglet affiche **HDR** ou **LDR** en conséquence.

## L'onglet Éclairage

**Affichage > Panneaux > Éclairage** affiche l'onglet.

Section **Aperçu de l'éclairage** :

| Élément | Effet |
| --- | --- |
| **Précalculer l'éclairage** | Précalcule toute la map. |
| **Précalculer autour de la caméra** | Précalcule la zone autour de la caméra 3D (l'infobulle donne le rayon). |
| **Arrêter le précalcul** | Actif pendant un précalcul. |
| Progression | Type (**Carte entière**, **Autour de la caméra**, **Mise à jour**), phase (**Préparation**, **Compilation de la carte**, **Lumière directe**, **Lumière rebondie**, **Finalisation**) et durée. |
| Dernier résultat | « N faces précalculées en N s », **Précalcul arrêté** ou **Pas encore précalculé** ; mémoire vidéo et triangles d'ombre. |
| Avertissements | « La carte n'a aucune entité de lumière », **L'éclairage n'est plus à jour** avec **Précalculer à nouveau**. |
| **Qualité** | **Brouillon** (mêmes niveaux de lumière, ombres plus bruitées, par défaut), **Normal** (une compilation normale), **Finale** (une compilation finale, la plus lente). |

**Options (enregistrées dans la carte)** : stockées dans le bloc `light_plus` de Hammer++ du VMF, elles voyagent avec la map.

| Option | Effet | Défaut |
| --- | --- | --- |
| **Luminosité** | Luminosité de l'aperçu, de 0 à 4. | 1 |
| **Lumière rebondie** | Lumière réfléchie par les surfaces (précalcul plus lent). | Activé |
| **Mises à jour dynamiques** | Précalcule à nouveau automatiquement quand les modifications s'arrêtent pendant le délai. | Activé |
| **Délai de mise à jour** | De 0 à 60 s. | 0 s |
| **Rayon autour de la caméra** | Rayon de **Précalculer autour de la caméra**, de 64 à 65536. | 1024 |
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

## Options

**Outils > Options... > Éclairage** (`[lighting]` dans `settings.toml`) :

| Option | Effet | Défaut |
| --- | --- | --- |
| **Précalculer à l'activation de l'aperçu** (`auto_bake`) | Lance un précalcul complet quand l'aperçu précalculé est activé et que rien n'est encore précalculé. | Activé |
| **Threads de précalcul** (`threads`) | 0 utilise tous les cœurs (tous sauf deux pour compiler la map). Utilisé par le prochain précalcul. | 0 |
| **Luxels en pleine résolution (millions)** (`max_luxels_millions`) | Les maps avec plus de luxels sont prévisualisées avec des lightmaps plus grossières (1 à 64). | 16 |
| **Afficher le budget des lightmaps dans la barre d'état** (`show_budget`) | | Activé |
| **Très petite échelle de lightmap** (`tiny_scale`) | Les faces à cette échelle ou moins comptent comme très petites (1 à 64). | 4 |
| **Styles de lumière** (`budget_styles`) | Lightmaps par face comptées par le budget : 1 sans lumières commutables, jusqu'à 4. | 1 |
| Qualité (`quality`) | Réglée dans l'onglet Éclairage : `Draft`, `Normal`, `Final`. | Draft |

Réglages de rendu utilisés ici : **Lumières avec ombres** (`render.light_shadows`, 4), **Valeurs de lumière HDR** (`render.light_preview_hdr`, activé), **Lumière du ciel seulement sous le ciel** (`render.sky_occlusion`, activé), **Ombres des props statiques** (`render.prop_shadows`, Auto), **Relief (normal maps)** (`render.normal_maps`, activé), **Reflets des cubemaps** (`render.cubemaps`, activé), **Redessiner les cubemaps après les modifications** (`render.cubemaps_auto`, activé).

## Astuces

- Utilisez **Précalculer autour de la caméra** pendant que vous réglez une pièce : c'est bien plus rapide qu'un précalcul complet.
- Gardez la **Qualité** sur **Brouillon** pendant le travail ; les niveaux de lumière sont les mêmes, seules les ombres sont plus bruitées.
- Outils MCP : `bake_lighting`, `get_lighting_status` (voir la [référence des outils MCP](../../MCP.md), en anglais).

## FAQ

**L'aperçu précalculé reste sombre.** La map n'a aucune entité de lumière, ou rien n'est encore précalculé : cliquez sur **Précalculer l'éclairage**.

**Pourquoi un brush que j'ai déplacé paraît sans éclairage ?** Les solides modifiés masquent leurs faces précalculées jusqu'au prochain précalcul. Avec **Mises à jour dynamiques**, l'éclairage revient quand vous arrêtez de modifier.

**L'aperçu correspond-il au jeu ?** Il utilise le même compilateur et la même étape d'éclairage qu'une vraie compilation, à la qualité choisie. **Finale** correspond à une compilation finale.

**L'écriture des cubemaps dit qu'elles sont dessinées avec l'ombrage de l'éditeur.** Ouvrez la vue joueur et appuyez sur `L` pour utiliser l'éclairage compilé, puis recommencez.

## Pages liées

- [Modes de rendu et options d'affichage](render-modes-display.md)
- [Guide éclairage et occlusion ambiante](../guides/lighting-and-ao.md)
- [Cubemaps](../compiler/cubemaps.md)
- [Vue joueur](player-view.md)
- [Référence des paramètres](../reference/settings.md)

---
[Retour à l'index de la documentation](../README.md)
