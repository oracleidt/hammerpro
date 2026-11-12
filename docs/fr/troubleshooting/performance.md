[Documentation](../README.md) > Dépannage > Performances de l'éditeur | [English](../../en/troubleshooting/performance.md) | [Русский](../../ru/troubleshooting/performance.md)

# Performances de l'éditeur

Hammer Pro est conçu pour rester fluide sur des maps énormes, mais un contenu lourd, un GPU modeste ou un portable sur batterie peuvent quand même ralentir les vues. Cette page liste les réglages qui comptent, du moins coûteux à changer au plus coûteux. Pour la durée des compilations, voir [Performances de compilation](../compiler/performance.md).

## Mesurer d'abord

- **Paramètres > Général > Afficher les images par seconde** affiche les images par seconde dans la barre d'état.
- **Affichage > Options d'affichage > Informations de la vue 3D** (`O`) affiche la position de la caméra et, pour la vue 3D, les appels de dessin, les triangles et les blocs dessinés, et le nombre de blocs écartés.

- **Paramètres > Rendu > Performance > Afficher le temps d'image et la mémoire dans la vue 3D** (`render.perf_hud`) ajoute un affichage sur la vue 3D : images par seconde et temps d'image, temps CPU du rendu (préparation, enregistrement, envoi), temps GPU par étape (ombres, scène, effets, reflet de l'eau) quand la carte graphique gère les requêtes d'horodatage, mémoire vive et graphique utilisées, carte graphique et niveau de qualité.
- Par MCP, `get_performance` renvoie les mêmes chiffres, et `run_benchmark` fait tourner la caméra sur un tour complet et renvoie les temps d'image moyen et pire, les millisecondes CPU et GPU et la mémoire.
- `get_performance` montre aussi où passe le temps de la fenêtre et pourquoi elle dessine : `frame_phases_ms` découpe le temps processeur de la dernière image (begin, panels, tool, sync, lighting, views, chrome, dialogs, end), `worst_frame_ms` garde l'image la plus lente depuis l'appel précédent, `views_ms` donne pour chaque vue le temps de l'overlay, du dessin et des étiquettes, et `frame_index`, `frames_from_events`, `continuous_frames` et `repaint_causes` indiquent quelles parties de Hammer Pro ont demandé des images.
- `get_performance` `gpu_ms.passes` découpe le temps de la carte graphique de la vue 3D par passe : `light_cull` (les listes de lumières des aperçus d'éclairage), `sun_shadows`, `local_shadows`, `main` (la vue elle-même, avec la passe de profondeur du mode éclairé), `effects` (eau, volumes de brouillard, halo du soleil, réfraction), `present` (tone mapping, bloom, affichage) et `outline` ; `water_reflection_passes` découpe de la même façon le reflet de l'eau. `run_benchmark` renvoie aussi le 99e centile du temps d'image, du temps CPU et du temps GPU, la moyenne de chaque passe et, avec `trace: true`, chaque image (temps d'image, temps CPU et GPU, passes, temps des vues et de toute l'image), pour trouver ce qui ralentit une image.

Gardez la fenêtre active pour comparer les images par seconde : la limite en arrière-plan et la gestion mémoire de Windows peuvent changer le résultat quand elle perd le focus. Utilisez la même map, caméra, taille de vue, mode de rendu et mode de performance. Comparez la mémoire après chargement, après passage en mode éclairé et après déplacement dans la map ; comparez les millisecondes CPU et GPU en plus des images par seconde.

## Rendu en qualité maximale et mémoire

Le rendu éclairé utilise une passe de profondeur pour la géométrie opaque du monde et des brushes, puis éclaire ces surfaces à cette profondeur. Depuis la 2.0.5, les parties opaques des props passent aussi par cette passe de profondeur, si bien qu'un pixel couvert par de nombreux props n'est éclairé qu'une fois au lieu d'une fois par prop. L'occlusion de la lumière du ciel réutilise une table par vue avec les mêmes distances d'échantillonnage et niveaux de mipmap. Les listes des clusters écartent les lumières qui ne peuvent pas contribuer au cluster, en conservant l'ordre de priorité existant. Ces optimisations gardent les réglages de qualité maximale.

**Niveaux de détail des modèles** (`render.model_lods`) suit les seuils Source du modèle et sa taille projetée. Le CPU évite d'envoyer les niveaux inutilisés quand il peut préserver l'ordre de dessin existant ; les modèles transparents et les seuils ambigus conservent la sélection par le shader. Désactiver ce réglage garde le niveau le plus fin.

Les textures DXT1, DXT3 et DXT5 sont envoyées dans leur format BC quand la carte graphique le gère. Les formats non gérés utilisent le repli RGBA existant. Les buffers de transfert décodés sont libérés après l'envoi. Le cache de géométrie des modèles utilise un budget de 128 Mio pour les données CPU sans référence, évince les moins récemment utilisées et conserve les informations des modèles. Les modèles de la map ouverte ne gardent leur géométrie complète en RAM que tant qu'ils servent : une fois la map chargée, leurs niveaux de détail et leurs coques d'ombre construits et le modèle inutilisé depuis environ cinq secondes (même quand l'éditeur reste immobile), il ne reste qu'une copie compacte pour une sélection précise (positions, triangles et côté visible, environ un tiers de la taille). Un nouveau skin ou bodygroup, le navigateur de modèles, un aperçu ou l'ouverture d'une autre map relit le modèle en arrière-plan ; pendant ce temps l'entité garde son apparence actuelle. Les fichiers d'un modèle sont lus une seule fois (le MDL n'est plus relu pour la liste des animations, et les niveaux de détail réutilisent les mêmes fichiers). Ce budget de cache est distinct de la RAM totale de l'éditeur et de la mémoire GPU.

L'eau masquée et l'eau hors du frustum de la caméra ne déclenchent pas de passe de reflet. Cette élimination existait déjà en 1.1.0. En 1.1.1, les mesures de performance affichent un temps de reflet nul quand cette passe est ignorée, au lieu de reprendre la mesure du reflet précédent. L'eau visible conserve la qualité de reflet existante.

## Aperçus d'éclairage à pleine vitesse

Depuis la 2.0.5, le mode éclairé, l'aperçu de l'éclairage calculé et la vue joueur restent proches de la fréquence d'images du mode texturé, avec la même image (captures de chaque mode comparées pixel par pixel avant et après) :

- **Props dans la passe de profondeur.** En mode éclairé et dans l'aperçu calculé, les parties opaques des props passent maintenant par la passe de profondeur avant d'être éclairées, comme le monde. Avant, chaque prop était éclairé avec jusqu'à 128 lumières même là où un autre prop le cachait : sur gmc_op_v2, regarder la ville de loin dessinait la vue 3D en 82 ms de carte graphique, maintenant 17 ms (le monde seul prend 2 ms). Une lumière qui ne peut pas atteindre un pixel est aussi écartée avant de lire le reste de ses données.
- **Listes de lumières.** Les lumières qui atteignent chaque cellule de la vue sont listées par la carte graphique. Là où plus de 128 lumières atteignent une cellule, les plus fortes sont maintenant choisies par tout un groupe de fils de la carte graphique en 4 passes, au lieu d'un seul fil en 31 passes. Sur hokai_d (1 785 lumières), cette étape est passée de 3,4 ms à 0,2 ms par image (et autant pour un reflet d'eau visible), ce qui supprime les images de 40 à 90 ms en se déplaçant en mode éclairé.
- **Lumières clignotantes.** Les styles de lumière (scintillement, pulsation, motifs des lumières nommées) ne mettent plus à jour que l'intensité de leurs lumières 10 fois par seconde. Avant, chaque pas reconstruisait toutes les lumières de la map, cherchait à nouveau les lumières enfouies dans les murs et redessinait les vues 2D.
- **Moins d'appels de dessin.** La passe de profondeur et le monde de la vue 3D sont envoyés en quelques dessins multiples au lieu d'un appel par morceau de la map, et les cartes d'ombre du soleil et des lumières fusionnent la géométrie voisine en un seul appel.
- **Rien n'est créé à chaque image.** L'histogramme d'exposition, le bloom et l'affichage final de chaque vue réutilisent leurs tampons et leurs liaisons, et la relecture de l'exposition part avec l'image au lieu d'un envoi à part.
- **Vue joueur.** Les props avec lesquels le joueur entre en collision, les triggers qu'il touche et les compteurs de la vue joueur (ce que dessine le jeu) ne sont plus recalculés à partir de toutes les entités et tous les brushes de la map à chaque image : les props et triggers sont listés une fois par modification de la map, un trigger ne teste que ses propres brushes, et les compteurs ne sont recalculés que quand la caméra ou la visibilité change. Sur rp_archipel_v1, cela prenait 0,6 à 0,9 ms de chaque image.
- **Tourner la caméra.** Pendant qu'une vue 3D bouge, sa carte d'ombre du soleil couvre 1,5 fois la zone nécessaire (ombres au plus deux fois plus douces pendant le mouvement), si bien que tourner la réutilise sur plusieurs images au lieu d'y redessiner tous les props à chaque image ; les vues dessinées dans la même image la partagent. Dès que la caméra s'arrête, la carte est redessinée en pleine netteté : une image immobile ne change pas. Chaque vue 3D garde son propre état, deux vues 3D ne se reprennent donc plus la carte.
- **Qualité de l'aperçu d'éclairage** (`render.lit_quality`, **Paramètres > Rendu > Performance**). `Auto` (par défaut) ne garde les 32 lumières les plus fortes de chaque partie de la vue que pendant que la caméra bouge et redessine toutes les lumières à l'arrêt, `High` dessine toujours jusqu'à 128, `Balanced` toujours 32. Une image immobile en `Auto` est celle de `High`.
- **Survol sous une caméra qui bouge.** Quand la caméra glisse sous un pointeur immobile, la surface et l'objet sous le pointeur ne sont recherchés qu'une fois la caméra arrêtée, au lieu de chaque image (3 à 20 ms par image sur gmc_op_v2).
- **Cubemaps.** Les cubemaps de l'éditeur redessinées après un changement de mode de rendu ne sont dessinées que caméra au repos (400 ms après son dernier mouvement) et prennent au plus 15 % du temps d'image, la navigation juste après un changement de mode reste donc fluide.
- **Seules les vues 3D se redessinent** pour un pas de style de lumière, et seule la vue concernée pour son exposition automatique ou pour l'ombre du soleil plus nette dessinée quand sa caméra s'arrête (les vues 2D et les autres vues 3D gardent leur image).

Chaque vue 3D garde ses propres listes de lumières (elles suivent sa caméra) ; les cartes d'ombre des lumières ponctuelles et spots sont dessinées une fois et partagées par toutes les vues, et chaque carte d'ombre est gardée tant que rien de ce dont elle dépend ne change. Par MCP, `get_performance` et `run_benchmark` montrent le coût de chaque passe (voir Mesurer d'abord).

## Détail des textures selon la distance

**Paramètres > Rendu > Mémoire graphique > Charger le détail des textures selon la distance** (`render.texture_streaming`, activé par défaut) garde les textures utilisées seulement loin de toutes les vues à une taille réduite. Une texture assez lointaine n'est lue que dans ses petits niveaux de mip : Hammer Pro retire les grands niveaux qui ne seraient jamais affichés, un niveau entier à la fois, et garde un niveau de plus par sécurité. L'image à l'écran ne change pas.

- Les distances viennent de toutes les vues 3D et 2D texturées (une vue 2D ne compte que ce que son rectangle montre), de l'échelle de texture de chaque face de brush et de la densité de texture de chaque modèle. Les vues comptent pour au moins 1080 pixels de haut, pour que les captures MCP jusqu'à 1920 pixels restent nettes, et les cubemaps que l'éditeur dessine pour les `env_cubemap` comptent aussi comme des vues, pour que les reflets gardent leur détail. Les textures ne descendent jamais sous 128 pixels.
- Quand la caméra s'approche, la texture revient en pleine taille avant que ses grands niveaux ne soient lus : les plus proches se chargent d'abord. Une texture n'est réduite qu'après être restée loin pendant 2 secondes.
- Pendant l'ouverture d'une map, les textures des faces de brush et des modèles sont d'abord lues à 256 pixels au plus, puis chaque texture proche de la caméra d'ouverture est chargée en pleine taille avant que l'écran de chargement ne disparaisse : ce que vous voyez d'abord est en pleine qualité, et les textures lointaines ne prennent jamais leur pleine taille en mémoire.
- Seules les textures de base des faces de brush et des modèles sont réduites. Les textures de détail, normal maps, masques, l'eau, les sprites, decals, overlays, particules, le ciel, la vue joueur et les textures sans niveaux de mip gardent toujours leur pleine taille.
- Sur une carte graphique avec peu de mémoire, les textures peuvent utiliser 60 % du budget de mémoire graphique : si ce n'est pas assez, les textures les plus lointaines sont réduites d'un niveau de plus jusqu'à ce qu'elles tiennent, avant que Hammer Pro ne baisse toutes les textures (voir [Mémoire insuffisante](out-of-memory.md)).

Par MCP, `get_gpu_memory` renvoie `texture_streaming` : les textures gardées réduites, la mémoire graphique qu'elles utilisent face à leur pleine taille, et le niveau de réduction. `set_setting render.texture_streaming false` le désactive.

## Calculer l'éclairage, puis naviguer

**Aperçu de l'éclairage** (`Alt+5`) dessine les lumières directes en temps réel. **Aperçu de l'éclairage précalculé** (`Ctrl+F5`) réutilise les lightmaps calculées. Choisissez **Map > Éclairage > Précalculer l'éclairage** et attendez la fin des passes rapide et affinée avant d'évaluer la navigation. Cette action calcule toujours toute la carte ; déplacer la caméra ou changer la sélection n'invalide pas son résultat.

L'option **Précalculer autour de la caméra sur les grandes cartes** est désactivée par défaut. Si elle est activée, elle concerne le démarrage automatique de l'aperçu et suit la caméra : entrer dans une autre région peut donc lancer un nouveau précalcul. **Précalculer l'éclairage** revient à un résultat global. Les choix existants enregistrés sont conservés.

La géométrie, les lumières, les matériaux et les paramètres de calcul peuvent invalider le résultat. La luminosité de l'aperçu, la correction colorimétrique et le délai de mise à jour ne relancent pas le calcul d'éclairage. La progression, le dernier résultat et l'avertissement périmé de l'onglet Éclairage distinguent la préparation d'un résultat réutilisable. Avec les mises à jour dynamiques désactivées, utilisez **Précalculer à nouveau** après un changement pertinent. Voir [Aperçu de l'éclairage](../features/lighting-preview.md).

## Mémoire de l'aperçu d'éclairage

La préparation de la géométrie de l'aperçu d'éclairage s'exécute dans un processus séparé. Il reçoit un instantané des dossiers, VPK, GMA et préfixes de textures personnalisées montés, dans leur ordre de priorité actuel. L'éditeur conserve le BSP et les correspondances de faces nécessaires à l'affichage, puis réutilise la géométrie préparée pour la passe affinée. Annuler l'aperçu arrête son processus de travail.

Les rayons de rebond utilisent un cache borné : 48 Mio pour l'aperçu et 128 Mio avec le budget mémoire par défaut du compilateur. Les enregistrements excédentaires sont lus dans le stockage temporaire par tuiles de 128 points de collecte. Les positions d'échantillonnage, le nombre de rayons et l'ordre d'accumulation restent identiques. Les fichiers temporaires se ferment et disparaissent à la fin ou à l'annulation du processus d'éclairage. Cela borne le cache des rayons ; les lightmaps, la géométrie, les sondes et les autres données du compilateur utilisent leur propre mémoire.

Utilisez les actions d'éclairage existantes ou l'outil MCP `bake_lighting` avec `action: "start"`, `"around_camera"` ou `"stop"` pour démarrer ou annuler un aperçu, et `get_lighting_status` pour l'inspecter. Voir [Aperçu de l'éclairage](../features/lighting-preview.md) et [Performances de compilation](../compiler/performance.md).

## Mode de performance

**Paramètres > Rendu > Performance** (`render.performance_mode`, `render.render_scale`) :

| Mode | Effet |
|---|---|
| **Auto** (défaut) | Qualité maximale. Seulement si la vue 3D reste sous 20 images par seconde pendant plusieurs secondes (6 sur une puce graphique intégrée, 10 sinon, hors chargement) Hammer Pro baisse la qualité d'un cran, affiche un avis, et propose **Rétablir la qualité maximale** sur la vue 3D et dans les réglages. Après un rétablissement, la réduction automatique reste suspendue jusqu'à ce que vous changiez de mode. Il ne baisse rien sur une carte graphique qui suit. Seules les images dessinées à la suite comptent : les images que Hammer Pro espace volontairement (une animation à son rythme, une fenêtre au repos) ne baissent jamais la qualité. |
| **Off** | Toujours la qualité maximale, jamais baissée automatiquement. |
| **Low** | Résolution de rendu à 75 %, sans anticrénelage ni bloom, 4 lumières avec ombres, ombre du soleil en 2048, textures jusqu'à 1024 pixels, l'eau reflète seulement le monde, sans occlusion de la lumière du ciel. |
| **Very low** | Résolution de rendu à 50 %, sans anticrénelage ni bloom, sans ombres des lumières, sans reflets, normal maps ni volumes de brouillard, ombre du soleil en 1024, textures jusqu'à 512 pixels, props dessinés en boîtes au-delà de 3072 unités. |

**L'échelle de rendu** dessine la vue 3D entre 25 % et 100 % de sa taille puis l'étire sur la fenêtre ; 100 % (défaut) est la qualité maximale. Elle fonctionne avec chaque mode (le plus bas des deux s'applique).

Rien de cela ne change l'aspect par défaut : la vitesse du mode par défaut vient du fait de dessiner moins de ce qu'on ne voit pas (les props et les occulteurs d'ombre hors d'une vue ne sont plus envoyés à la carte graphique, l'ombre du soleil est conservée quand la caméra bouge un peu), pas d'une baisse de qualité.

### Adaptation fine de l'éclairage (Auto)

En **Auto**, les aperçus d'éclairage s'adaptent aussi finement à la carte graphique, bien avant que le niveau de qualité ci-dessus ne change quoi que ce soit. Hammer Pro mesure le temps que la carte graphique passe sur chaque image de la vue 3D (requêtes d'horodatage, ou le temps que met l'image à se terminer sur une carte qui n'en a pas). Quand ce temps reste au-dessus de 14 ms deux secondes de suite, le travail d'éclairage descend d'un niveau à la fois :

| Niveau | Ce qui change |
|---|---|
| 0 | Qualité complète. Une carte graphique qui suit n'en sort jamais. |
| 1 | Les cartes d'ombre des lumières sont dessinées sur plus d'images (2 faces par image au lieu de 6) et la lumière du ciel regarde autour par pas plus longs. |
| 2 | Les ombres douces du soleil et les ombres des lumières prennent deux fois moins d'échantillons. |
| 3 | La carte d'ombre du soleil fait au plus 2048 pixels et celles des lumières 256. |

Elle remonte d'un niveau quand la carte graphique reste sous 7 ms pendant quatre secondes, jamais moins de 20 secondes après être descendue de ce niveau, pour que l'image ne change pas sans cesse. **Désactivé**, **Bas**, **Très bas** et **Restaurer la qualité complète** la coupent. L'affichage du temps d'image (`render.perf_hud`) montre le niveau tant qu'il est au-dessus de 0, et MCP `get_performance` renvoie `adaptive_lighting` (niveau, millisecondes de la carte graphique, cible, nombre de changements).

### Essayer une carte graphique plus faible

La variable d'environnement `HP_ADAPTER` choisit la carte graphique au démarrage : `integrated`, `cpu` (le rendu logiciel de Windows, bon substitut d'une carte très faible) ou une partie du nom d'une carte. `HP_PERF_BASELINE=all` désactive les accélérations de rendu par défaut pour les comparaisons. Une liste séparée par des virgules permet de les isoler : `instances`, `shadowfit`, `exposure`, `clusters`, `prepass`, `lods`, `skymarch`, `clusterbounds`, `gpuclusters` (les lumières sont rangées dans les cellules de la vue par le processeur au lieu de la carte graphique ; `clusters` et `clusterbounds` agissent sur ce chemin processeur), `casters` (tout changement de géométrie redessine les cartes d'ombre, même un assistant qui ne projette pas d'ombre). `lods` concerne seulement l'envoi par le CPU ; le niveau de détail Source sélectionné est conservé. Ces commutateurs de diagnostic ne changent pas les réglages de qualité enregistrés.

## Réglages d'images par seconde

Page **Paramètres > Rendu**, groupe **Images par seconde** :

| Option | Effet | Défaut |
|---|---|---|
| **Limite d'images par seconde** | Nombre maximal d'images par seconde (0 : aucune limite ; avec la synchronisation verticale, l'écran la limite). Baissez-la pour économiser batterie et chaleur sur un portable. | 0 |
| **En arrière-plan** | Limite d'images par seconde quand la fenêtre n'est pas active (0 : aucune limite). | 30 |
| **Synchronisation verticale** | Attend le rafraîchissement de l'écran. S'applique immédiatement. | Activé |
| **Animations** (`render.animation_fps`) | Images par seconde des matériaux animés, des sprites de détail qui ondulent, des effets de particules et des lumières qui clignotent quand rien d'autre ne bouge (0 : chaque image). | 30 |
| **Mettre les animations en pause après** (`render.animation_idle_pause_s`) | Les animations s'arrêtent après ce nombre de secondes sans souris ni clavier et reprennent à la prochaine action (0 : jamais). | 120 s |
| **Animer en arrière-plan** (`render.animate_in_background`) | Continuer les animations quand une autre fenêtre est active. | Désactivé |

## Au repos et animations

Quand rien ne bouge, Hammer Pro ne dessine rien : la fenêtre ne dessine une image que pour une action de la souris ou du clavier, un changement de la map ou du contenu chargé, une tâche en cours ou une animation (voir plus bas). Mesuré sur le même ordinateur, fenêtre sans y toucher pendant 30 secondes, en pourcentage d'un cœur du processeur :

| État de la fenêtre | gm_construct | hokai_d (73 Mo) |
|---|---|---|
| Version 1.2.4, fenêtre active (elle dessinait en permanence) | 65 à 87 % | 50 à 67 % |
| Animations arrêtées : fenêtre en arrière-plan, après **Mettre les animations en pause après**, ou aucune surface animée visible | 0,3 % | 0,4 à 0,6 % |
| Fenêtre active avec des surfaces animées visibles (eau, textures qui défilent), pendant les deux premières minutes après la dernière action | 15 à 18 % | 23 à 30 % |

Dans le dernier cas seule la vue 3D se redessine, à **Animations** images par seconde (30), pour environ 5 à 7 ms de processeur par image sur gm_construct et 10 à 12 ms sur hokai_d. Baissez **Animations**, raccourcissez **Mettre les animations en pause après** ou coupez **Matériaux animés** pour revenir à la deuxième ligne. `get_performance` par MCP indique le cas en cours (`window_focused`, `animations_running`) et le coût moyen des images dessinées depuis l'appel précédent (`average_frame_ms`).

Les matériaux animés, l'ondulation des sprites de détail, les effets de particules des entités et le clignotement des styles de lumière (dans les aperçus d'éclairage et d'éclairage calculé) sont les seules choses qui bougent seules. Ils suivent les trois réglages ci-dessus : ils avancent à **Animations** images par seconde, s'arrêtent quand une autre fenêtre est active (sauf avec **Animer en arrière-plan**) et s'arrêtent après **Mettre les animations en pause après** secondes sans action ; le prochain mouvement de souris ou la prochaine touche les relance. Une vue animée ne se redessine que quand l'animation avance : bouger la souris ne la redessine pas plus souvent, et l'animation de la vue 3D ne redessine pas les vues 2D. L'indicateur de la barre d'état pendant le démarrage de Garry's Mod tourne 12 fois par seconde (il redessinait avant toute la fenêtre à pleine vitesse, environ 25 % d'un cœur) et s'arrête avec les animations.

## Enregistrement, sauvegarde automatique et annulation sur les grosses maps

- **Enregistrer** (`Ctrl+S`) copie la map en mémoire et écrit le fichier en arrière-plan : la fenêtre reste réactive pendant l'écriture d'une grosse map. L'`*` du titre disparaît quand le fichier est sur le disque. Fermer la map, fermer Hammer Pro ou enregistrer à nouveau le même fichier attend d'abord la fin de cette écriture. **Enregistrer sous...**, les sous-maps d'un manifeste et `save_map` par MCP écrivent avant de rendre la main, et une écriture en arrière-plan qui se termine après eux ne ramène jamais la map vers l'ancien fichier.
- La **sauvegarde automatique** copie la map quelques millisecondes à la fois entre les images (par tranches plus longues quand vous ne touchez ni la souris ni le clavier), puis écrit la copie en arrière-plan : aucun à-coup, même sur des maps énormes. Une modification pendant la copie la fait recommencer, une sélection non.
- L'**historique d'annulation** ne garde que ce qui a changé. La dernière étape garde les objets entiers pour que son annulation reste immédiate ; dès qu'une autre étape la suit (les étapes de sélection ne comptent pas), elle ne garde que les champs modifiés de chaque face et de chaque clé, et l'annuler plus tard reconstruit la géométrie de ses objets. Un ancien déplacement des 26 419 objets de hokai_d garde 31 Mo au lieu de 171 : après trois transformations de toute la map, l'historique occupe 247 Mo au lieu de 506.

## Ce que dessine la vue 3D

| Réglage | Où | Effet sur la vitesse | Défaut |
|---|---|---|---|
| **Distance d'affichage** | Paramètres > Vues | Distance maximale dessinée. Plus courte, c'est plus rapide. **Affichage > Zoom et distance d'affichage > Distance d'affichage plus courte** et **Distance d'affichage plus longue** la changent à la volée. | 32768 |
| **Anticrénelage (MSAA)** | Paramètres > Vues | Bords plus lisses, utilise plus de mémoire GPU. Désactivez-le sur un GPU modeste. | Activé |
| **Limite de distance circulaire** et **Rayon de limite** | Paramètres > Rendu, aussi **Affichage > Effets > Limite de distance circulaire** et la barre d'outils | Rien n'est dessiné au-delà du rayon autour de la caméra (un cercle le montre dans les vues 2D), comme le radius culling de Hammer. | Désactivé, 4096 |
| **Ignorer la géométrie cachée** | Paramètres > Rendu | Les parties de la map dont tous les objets sont masqués ne sont pas dessinées du tout. | Activé |
| **Distance des modèles** | Paramètres > Rendu | Les modèles plus loin que cela sont dessinés en boîtes (0 : aucune limite). Très utile sur les maps chargées en props. | 0 |
| **Niveaux de détail des modèles** | Paramètres > Rendu | Utilise les niveaux de détail Source du modèle selon sa taille à l'écran. Désactivé : chaque modèle utilise son niveau le plus fin. | Activé |
| **Relief (normal maps)** | Paramètres > Rendu | Désactivé : surfaces plates et moins de mémoire vidéo. | Activé |
| **Reflets des cubemaps** et **Redessiner les cubemaps après les modifications** | Paramètres > Rendu | Les matériaux avec `$envmap` reflètent des cubemaps dessinées par l'éditeur. Sans le redessin automatique, elles restent telles quelles jusqu'à **Map > Éclairage > Reconstruire les cubemaps**. | Activé, Activé |
| **Lumières avec ombres** | Paramètres > Rendu | En mode Aperçu de l'éclairage, nombre de lumières ponctuelles et de spots en vue qui projettent des ombres, les plus brillantes d'abord (0 : le soleil seulement, 64 au maximum ; au-delà de 8 leurs cartes d'ombre sont plus grossières). | 16 |
| **Lumière du ciel seulement sous le ciel** | Paramètres > Rendu | En mode Aperçu de l'éclairage, la lumière du ciel est vérifiée avec une carte des hauteurs de la map autour de la caméra. Désactivé : un peu moins de travail pour les cartes graphiques lentes. | Activé |
| **Aperçu de l'eau** | Paramètres > Rendu | Réfraction, reflets et vagues de l'eau. Désactivé : la texture d'outil. | Activé |
| **Détail de l'eau** | Paramètres > Rendu | Reflets complets et Reflet du monde dessinent la scène une seconde fois (1024 pixels de large au plus) pour le miroir de l'eau coûteuse ; Reflets simples est le moins coûteux. | Reflets complets |
| **Fondu des modèles**, **Effets de rendu**, **Matériaux animés** | Affichage > Options d'affichage (les deux premiers aussi dans Paramètres > Rendu) | Coûts modestes ; à couper quand chaque image compte. | Désactivé, Activé, Activé |

Le mode de rendu compte aussi : **Couleurs unies** (`Alt+2`) et **Filaire** (`Alt+3`) évitent le travail d'éclairage. **Aperçu de l'éclairage** (`Alt+5`) calcule les lumières directes pendant la navigation ; **Aperçu de l'éclairage précalculé** (`Ctrl+F5`) prépare l'éclairage puis réutilise ce résultat. Les matériaux, modèles et reflets contribuent toujours au coût de dessin. Voir [Modes de rendu et affichage](../features/render-modes-display.md) et [Aperçu de l'éclairage](../features/lighting-preview.md).

## Ce que dessinent les vues 2D

| Réglage | Où | Effet | Défaut |
|---|---|---|---|
| **Modèles** | Paramètres > Rendu | **Boîtes** (le plus rapide), **Fil de fer** (tous les modèles), **Fil de fer si sélectionné**. | Fil de fer si sélectionné |
| Displacements en 2D | Paramètres > Rendu, **Affichage > Options d'affichage > Maillage des displacements en vue 2D** | **Maillage complet**, **Maillage de la sélection** ou **Contour seulement**. Les contours allègent les maps riches en terrain. | Maillage complet |

Les croix des centres de brushes sont dessinées par la carte graphique avec le reste de la vue et ne sont préparées à nouveau qu'après un zoom ou une modification, et les noms des entités ne sont mis en page qu'une fois tant qu'une vue ne bouge pas : sur hokai_d dézoomée, une image de l'éditeur pendant que la souris passe sur les vues est passée d'environ 36 ms de temps processeur à environ 10 ms, et une image pendant le déplacement ou le zoom de la vue de dessus d'environ 60 ms à 12 à 18 ms.

## Masquer ce sur quoi vous ne travaillez pas

L'objet le plus rapide est celui qu'on ne dessine pas :

- **Masquer la sélection** (`H`), **Masquer le reste** (`Ctrl+Maj+H`), l'œil d'isolement de la palette d'outils et **Tout afficher** (`U`) : voir [Masquer et isoler](../features/hide-isolate.md).
- Désactivez des [visgroups](../features/visgroups.md) entiers.
- Utilisez un [cordon](../features/cordons.md) pour travailler sur une zone.
- **Affichage > Options d'affichage** : coupez les modèles, les aides, les particules, les cordes, les overlays ou la skybox 3D quand vous n'en avez pas besoin.

## Temps de chargement

- **Monter les téléchargements du Workshop** (Paramètres > Jeu) ralentit le démarrage : laissez-le désactivé si vous n'avez pas besoin du contenu du Workshop.
- Les addons du Workshop et téléchargés se montent en arrière-plan après le reste ; l'éditeur reste utilisable pendant ce temps.
- Les très gros dossiers de textures personnalisés sont indexés à chaque montage : gardez-les ciblés.
- L'ouverture d'un VMF construit les brushes, faces, displacements et données d'éditeur des entités directement depuis le texte du fichier, sans copie intermédiaire de chaque clé et valeur : les grandes maps font de 50 % à 70 % d'allocations mémoire en moins pendant le chargement, pour le même résultat.
- Les textures et matériaux sont lus dans les archives VPK et GMA sans copier le fichier d'abord, et les fichiers MDL, VVD et VTX d'un modèle sont lus une seule fois par chargement.
- Le rechargement en direct des matériaux et modèles modifiés dort jusqu'à ce que Windows signale un changement dans un dossier de contenu non archivé, au lieu de vérifier chaque fichier surveillé toutes les 2 secondes.

## Travail en arrière-plan

Les vérifications en direct (**Vérifier la map pendant l'édition**, **Chercher les fuites pendant l'édition**, Paramètres > Santé de la map, activées par défaut) tournent en arrière-plan peu après chaque modification. Sur une très grosse map avec un processeur lent, désactiver la recherche de fuites économise du travail ; la compilation trouve toujours les fuites. La précompilation en arrière-plan (dans les réglages de compilation) utilise le processeur libre pour préparer la compilation suivante.

## Panneaux sur les grosses maps

Les panneaux du dock et les fenêtres ne dessinent que les lignes et les vignettes visibles : les longues listes (des dizaines de milliers de matériaux, de modèles ou de sons, toutes les entités d'une grosse map, des milliers de problèmes ou de lignes de console) coûtent à peu près autant que les courtes, bien moins d'une milliseconde par image chacune. Les recherches filtrent toute la liste à chaque touche sans allocation par nom (environ 3 ms pour 60 000 matériaux ou 40 000 modèles) ; dans le navigateur de sons, une recherche qui ne fait qu'ajouter des lettres filtre les résultats précédents (moins de 1 ms pour 50 000 sons). Sur les très grosses maps, l'outliner (quand construire son arbre prend plus de 2 ms) et les noms de cibles vérifiés par les propriétés d'entité (quand les vérifier prend plus de 1 ms) suivent les modifications continues comme un déplacement au plus quatre fois par seconde au lieu de chaque image, et sont à jour dès que la modification s'arrête : sur hokai_d (30 000 objets), une modification avec l'outliner ouvert coûte 0,3 ms au lieu de 10 ms par image.

## FAQ

**La vue 3D saccade quand je vole dans une grosse map.** Activez la **Limite de distance circulaire**, réglez une **Distance des modèles** (2048 par exemple) et vérifiez que **Ignorer la géométrie cachée** est activé.

**L'éditeur sollicite beaucoup le GPU quand je suis dans une autre fenêtre.** Baissez **En arrière-plan** (par exemple 10).

**Les ventilateurs tournent même quand rien ne bouge.** Réglez une **Limite d'images par seconde** comme 60, ou gardez la **Synchronisation verticale**. Si un matériau animé est en vue, baissez **Animations** ou **Mettre les animations en pause après**.

**Hammer Pro utilise le processeur alors que je ne fais rien.** Appelez `get_performance` par MCP deux fois à quelques secondes d'intervalle : si `frame_index` augmente, `repaint_causes` nomme la source qui continue de demander des images.

## Pages liées

- [Modes de rendu et affichage](../features/render-modes-display.md)
- [Masquer et isoler](../features/hide-isolate.md)
- [Référence des paramètres](../reference/settings.md)
- [Performances de compilation](../compiler/performance.md)
- [Plantages et journaux](crashes-and-logs.md)

---
[Retour à l'index de la documentation](../README.md)

## Overlay des primitives de la map

Le toggle Σ à côté de Vérifier les problèmes affiche dans la vue 3D les nombres de solides, matériaux du monde, matériaux des props et lumières de la map active. Les variantes des modèles utilisent leurs matériaux réellement sélectionnés. Les modèles en attente ou manquants sont indiqués séparément.

Sous Windows, les allocations CPU par catégorie sont mesurées dans les blocs du heap qui possèdent les ressources suivies de la map, avec la surcharge rapportée par Windows. Les blocs partagés sont comptés une seule fois dans une ligne distincte. Ces valeurs sont des octets alloués, pas une répartition de la RAM résidente. Le pilote, l'interface et l'historique d'annulation sont hors de cette répartition. La RAM résidente totale de l'éditeur est mesurée séparément par le système. Une mesure indisponible est signalée. L'instantané est actualisé lorsque les ressources changent ; laisser l'overlay actif ne parcourt pas continuellement le heap.

L'éviction des caches libère les données inutilisées. Le vidage forcé du working set Windows au repos ne sert plus à faire baisser la RAM affichée et provoquer ensuite des défauts de page. Les valeurs de 2 Go de RAM et 1 Go de VRAM sont des objectifs, pas des limites fixes ; la consommation dépend de la map. Une carte limitée à DirectX 9 ne satisfait pas les exigences de l'API graphique du moteur de rendu.

Les instances imbriquées sont incluses dans les totaux des solides, matériaux, props et lumières. Chaque placement est compté séparément ; les allocations partagées de la géométrie source et des modèles sont mesurées une seule fois. Les fichiers d’instance non résolus sont signalés.
