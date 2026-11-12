[Documentation](../README.md) > Fonctionnalités > Modes de rendu et options d'affichage | [English](../../en/features/render-modes-display.md) | [Русский](../../ru/features/render-modes-display.md)

# Modes de rendu et options d'affichage

Comment les vues dessinent la map : le mode de rendu de la vue 3D, les nombreux interrupteurs de **Affichage > Options d'affichage**, les images de référence en fond des vues 2D, la distance d'affichage et les limites d'images par seconde.

## Modes de rendu 3D

Choisissez le mode dans **Affichage > Mode de rendu 3D**, dans la liste des modes de la barre d'outils, ou au clavier :

| Mode | Raccourci | Ce que vous voyez |
| --- | --- | --- |
| **Texturé** (défaut) | `Alt+1` | Les matériaux avec l'ombrage de l'éditeur. |
| **Couleurs unies** | `Alt+2` | Une couleur unie par objet, ombrée. |
| **Filaire** | `Alt+3` | Seulement les arêtes. |
| **Texturé + arêtes du mesh** | `Alt+4` | Les matériaux avec les arêtes des triangles par-dessus. |
| **Aperçu de l'éclairage** | `Alt+5` | Le mode éclairé : les entités de lumière éclairent la map en temps réel, avec des ombres (voir [Aperçu de l'éclairage](lighting-preview.md)). |
| **Texturé sans ombrage** | `Alt+6` | Les matériaux sans l'ombrage de l'éditeur. |
| **Groupes de lissage** | `Alt+7` | Les faces colorées par groupe de lissage. |
| **Grille de lightmap** | `Alt+8` | La grille de lightmap 3D de Hammer : la grille des luxels de chaque face avec lightmap (carreaux aussi larges que son échelle de lightmap, displacements compris), grise à 16, rouge si plus fine, verte si plus grossière ; sans précalcul. Les faces sans lightmap gardent leur texture. |
| **3D Correction colorimétrique** | `Alt+9` | La vue texturée avec les entités `color_correction` appliquées : leurs tables de couleurs se mélangent selon la distance de la caméra 3D (poids plein dans `minfalloff`, nul au-delà de `maxfalloff`, multiplié par `maxweight`), donc vous voyez l'étalonnage du jeu sans activer l'aperçu séparé de la correction colorimétrique (voir [Correction colorimétrique](color-correction.md)). Comme le réglage d'aperçu, mais lié au mode. |
| **Aperçu de l'éclairage précalculé** | `Ctrl+F5` | L'éclairage calculé par le compilateur intégré, puis réutilisé pendant la navigation (voir [Aperçu de l'éclairage](lighting-preview.md)). |

Le menu des modes dans le coin de la vue 3D utilise les mêmes noms. **Aperçu de l'éclairage** conserve les lumières directes en temps réel. **Aperçu de l'éclairage précalculé** démarre un calcul global par défaut ; attendez ses passes rapide et affinée. **Map > Éclairage > Précalculer l'éclairage** calcule toujours toute la carte sans suivi de caméra. L'option régionale est désactivée par défaut et concerne seulement l'activation automatique ; si elle est activée, sa région suit la caméra et peut déclencher un autre précalcul.

**Affichage > Type de vue** a aussi **Vue 3D texturée** (`F5`) et **Vue 3D unie** (`Maj+F5`). Le mode est conservé d'une session à l'autre (`view.render_mode`).

## Entrées du menu Affichage

Directement dans le menu **Affichage** :

| Entrée | Effet | Défaut |
| --- | --- | --- |
| **Afficher les modèles** | Dessine les props en modèles (désactivé : des boîtes). | Activé |
| **Afficher les aides et sprites** | Aides des entités (rayons, cônes, lignes vers les cibles) et sprites. | Activé |
| **Afficher les textures outils** | Les faces à texture outil dans la vue 3D (voir [Masquer et isoler](hide-isolate.md)). | Activé |
| **Images de fond...** | Images de référence derrière les vues 2D (plus bas). | |

## Options d'affichage

**Affichage > Options d'affichage**. Chaque entrée est un interrupteur ; la plupart sont aussi dans la page **Rendu** des paramètres (table `[render]`).

| Entrée | Effet | Défaut |
| --- | --- | --- |
| **Informations de la vue 3D** (`O`) | Images par seconde, position et angles de la caméra dans la vue 3D. | Désactivé |
| **Afficher les overlays** | Les overlays dessinés sur leurs faces (voir [Décals et overlays](decals-overlays.md)). | Activé |
| **Afficher les connexions** | Lignes des entrées/sorties : aucune, sélection, toutes (chaque clic passe à la suivante). Voir [E/S et vue logique](io-logical-view.md). | Sélection |
| **Afficher les objets d'éditeur** | Icônes d'entités et aides qui n'existent que dans l'éditeur. | Activé |
| **Afficher les entités ponctuelles** | Toutes les entités ponctuelles. | Activé |
| **Afficher les noms des entités en 2D** | Noms de classe et de cible à côté des entités ponctuelles en 2D. Le bouton **Aa**, à côté de **2D** dans la barre d'outils, les masque sans cacher les entités ni modifier le rendu 3D. | Activé |
| **Aides de toutes les entités** | Les aides de toutes les entités au lieu de la sélection seulement. | Désactivé |
| **Afficher les cordes** / **Simuler les cordes** | Les cordes telles qu'elles pendent en jeu ; simulées, elles se stabilisent et réagissent à env_wind sauf si Disable Wind est activé. | Activé / Désactivé |
| **Effets de rendu** | Opacité, mode et couleur de rendu, groupes de corps, animation par défaut et échelle des entités. | Activé |
| **Aperçu du ciel** | Le ciel 2D (`skyname` du worldspawn) derrière la vue 3D. | Activé |
| **Skybox 3D** (`Alt+V`) | La pièce autour de la `sky_camera`, à son échelle, autour de la map (voir [Caméras de skybox](sky-cameras.md)). | Activé |
| **Modèles de collision** | Les enveloppes de collision dessinées sur les props (aussi dans l'aperçu du navigateur de modèles). | Désactivé |
| **Afficher les collisions des modèles** | La vue 3D montre les modèles de collision à la place des modèles (un modèle sans collision montre sa boîte en orange). | Désactivé |
| **Aperçu du brouillard** | Sous-menu : **Désactivé**, ou le brouillard d'un `env_fog_controller` de la map (par défaut celui du jeu : le dernier marqué **Master**, sinon le premier ; un contrôleur avec **Fog Enable** à No est signalé). Aussi un bouton de la barre d'outils. Voir [Brouillard de la map](#brouillard-de-la-map). | Activé |
| **Matériaux animés** | Les matériaux animés et les sprites s'animent. | Activé |
| **Fondu des modèles** | Les props disparaissent entre leurs distances de fondu, comme en jeu. | Désactivé |
| **Matériaux opaques** | Les matériaux translucides sont dessinés opaques. | Désactivé |
| **Pentes praticables** | Teinte les surfaces trop pentues pour que les joueurs s'y tiennent. | Désactivé |
| **Afficher les displacements** | Les displacements sont dessinés. | Activé |
| **Maillage des displacements en vue 2D** | Fait passer les vues 2D de la grille de triangles des displacements à leur contour. | Maillage |
| **Praticabilité des displacements** | Colore les triangles des displacements selon leur praticabilité, ou selon l'angle de pente avec les modes Limite et Dégradé (voir [Outil displacement](displacement-tool.md#contrôle-des-pentes)). | Désactivé |
| **Limite de distance circulaire** | Rien n'est dessiné au-delà du rayon de limite autour de la caméra (un cercle le montre dans les vues 2D). | Désactivé |
| **Surligner les brushes invalides** | Les brushes qui ne sont pas fermés et convexes, et les brushes dont le displacement ne peut pas être construit (le jeu les supprime), sont dessinés en rouge dans toutes les vues. | Activé |
| **Afficher le nodraw** | Affiche ou masque les faces nodraw dans la vue 3D. | Affiché |
| **Aperçu de l'eau** | L'eau dessinée comme en jeu : réfraction, brouillard de l'eau, reflet de la scène et du ciel, vagues animées. Désactivé : sa texture d'outil. | Activé |
| **Boîtes englobantes** | Boîtes englobantes jaunes des objets visibles. Désactivé par défaut et sans raccourci (`I` lance seulement le [décalage interactif](extrude-tool.md)) ; activez-les ici ou avec l'action `toggle_bounds_overlay` depuis la palette de commandes. | Désactivé |
| **Aperçu des detail props (herbe)** | Répartit l'herbe et les objets de détail des matériaux `%detailtype` autour de la caméra 3D (voir [Aperçu des detail props](detail-props-preview.md)). Aussi un bouton de la barre d'outils. | Désactivé |
| **Détail de l'eau** | Comme l'option Détail de l'eau du jeu (`render.water_detail`) : **Reflets complets** dessine l'eau coûteuse à toute distance, qui reflète le monde, les props, les entités de brushes, le ciel et la skybox 3D dessinés une seconde fois en miroir ; **Reflet du monde** garde l'eau coûteuse près de la caméra et ne reflète props et entités de brushes que pour les matériaux avec `$reflectentities` ; **Reflets simples** dessine l'eau bon marché. | Reflets complets |
| **Aperçu des fog_volume** | Chaque `fog_volume` est entouré et rempli de son brouillard, avec une étiquette de ses liens et problèmes (voir [Volumes de brouillard](#volumes-de-brouillard)). Aussi un bouton de la barre d'outils. | Activé |
| **Texture des fog_volume** | Le matériau des brushes `fog_volume` est dessiné dans la vue 3D, comme Hammer. Désactivé : seul le brouillard apparaît. | Activé |
| **Aperçu des poissons** | Les poissons de chaque `func_fish_pool` (voir [Bancs de poissons](#bancs-de-poissons)). | Activé |
| **Poissons qui nagent** | Les poissons nagent en cercles lents. Désactivé : ils restent immobiles. | Activé |
| **Reflets des cubemaps** | Les matériaux avec `$envmap` reflètent leur cubemap (voir [Aperçu de l'éclairage](lighting-preview.md)). | Activé |
| **Utiliser en jeu la sky_camera affichée** | Voir [Caméras de skybox](sky-cameras.md). | |
| **Afficher les particules** / **Particules de la sélection uniquement** | Voir [Particules](particles.md). | Activé / Désactivé |
| **Afficher les clusters visibles** | Voir [Fuites et pointfiles](leaks-pointfiles.md). | Désactivé |
| **Afficher le nav mesh** | Voir [Nav mesh](nav-mesh.md). | Désactivé |
| **Vue joueur** (`F6`) | Voir [Vue joueur](player-view.md). | |

Le menu Affichage contient aussi **Caméra de la skybox 3D** (quand la map a plusieurs sky_camera) et **Textures outils masquées** (nodraw, clips, déclencheurs, hints et skips masqués dans la vue 3D).

La barre d'outils reprend les interrupteurs les plus utilisés : limite circulaire, maillage des displacements, praticabilité des displacements, nodraw, collisions des modèles, aides, modèles, textures outils, objets d'éditeur, skybox 3D, particules, simulation des cordes, la liste des modes de rendu et la vue joueur.

## Brouillard de la map

**Aperçu du brouillard** (activé par défaut, bouton de la barre d'outils, **Affichage > Aperçu de l'environnement > Aperçu du brouillard** ; le contrôleur se choisit dans **Affichage > Aperçu de l'environnement > Contrôleur de brouillard prévisualisé**) dessine le brouillard d'un `env_fog_controller` dans les vues 3D et la [vue joueur](player-view.md), comme le brouillard de la map en jeu : **Primary Fog Color** (et **Secondary Fog Color** avec **Fog Blend**, selon **Primary Fog Direction** ou les angles avec **Use Angles for Fog Dir**), **Fog Start**, **Fog End**, **Fog Max Density** et **Far Z Clip Plane**. Modifier une clé met la vue à jour tout de suite. Il marche dans les modes texturé, éclairé, plat et sans ombrage, sur les lightmaps précalculés, l'eau, les particules, les detail props et les modèles, et dans la vue joueur.

- Le brouillard est calculé comme le jeu : il croît linéairement avec la distance selon la direction de la vue, de 0 à **Fog Start** (qui peut être négatif : le brouillard est déjà là devant les yeux) à **Fog Max Density** à **Fog End**, et il est mélangé en espace gamma avec la couleur dessinée. Une densité max de 0.25 ne couvre jamais plus d'un quart de la couleur.
- Avec **Fog Blend**, la couleur du brouillard dépend de l'endroit où regarde la caméra, pas de chaque pixel : regarder selon **Primary Fog Direction** donne la couleur primaire, regarder à l'opposé la secondaire, moitié de chaque de côté.
- Au-delà de **Far Z Clip Plane** rien n'est dessiné, comme le plan de coupe lointain du jeu : le ciel 2D se voit à travers (avec **Aperçu du ciel**), sinon la couleur du brouillard, qui remplit le fond des vues 3D tant que le brouillard est affiché, comme la couleur de fond du jeu. Le ciel 2D n'est jamais dans le brouillard, comme en jeu.
- Les matériaux et particules additifs s'éteignent vers le noir dans le brouillard, les décales en modulate perdent leur effet.
- La skybox 3D ne prend pas le brouillard de la map : elle prend les clés de brouillard de son `sky_camera` (**Fog Enable**, couleurs, mélange, direction ou **Use Angles for Fog Dir**, début, fin, densité max), mesurées en unités de la skybox depuis la caméra, comme en jeu. Un `sky_camera` avec **Fog Enable** à No laisse la skybox 3D sans brouillard. **Aperçu du brouillard** désactivé coupe les deux.
- MCP : `list_fog` donne `preview.fog`, `preview.sky_camera` et `preview.sky_fog`.

- Le sous-menu liste les contrôleurs de la map ; sans choix, celui du jeu est utilisé : le dernier avec l'indicateur **Master**, sinon le premier.
- Un contrôleur avec **Fog Enable** à No ne donne pas de brouillard, comme en jeu, et la vue 3D affiche un avertissement en bas à gauche.
- Sélectionner un contrôleur indique dans les propriétés de l'entité s'il est celui de l'aperçu et combien de `fog_volume` l'utilisent.

## Volumes de brouillard

Un `fog_volume` change le brouillard des joueurs dont les yeux sont dedans : le jeu utilise alors l'`env_fog_controller` nommé par son **Fog Name** au lieu du brouillard de la map, ainsi que le `postprocess_controller` et le `color_correction` nommés par **PostProcessName** et **Color Correction Name**. Comme en jeu, le volume est la boîte autour de tous ses brushes, quelle que soit leur forme.

Avec **Aperçu des fog_volume** activé (par défaut), chaque `fog_volume` est visible dans la vue 3D dès que vous le placez :

- ses brushes gardent leur matériau (`tools/toolsfog` ou un autre, voir **Texture des fog_volume**) et la boîte est entourée de la couleur de son brouillard ;
- la boîte est remplie de son brouillard, aussi épais que la vue la traverse : ce qui est dans le volume ou derrière lui est embrumé comme vu de l'intérieur ;
- une étiquette au-dessus de la boîte nomme le volume et son contrôleur (`fog_volume > cave_fog`) et liste en orange ce qui ne va pas. Sélectionnez le volume pour voir tous les détails et des lignes vers le contrôleur, le post-traitement et la correction colorimétrique.

Le brouillard dessiné dans la boîte :

| Fog Name | Brouillard dans la vue 3D | En jeu, yeux dedans |
| --- | --- | --- |
| Nomme un contrôleur avec **Fog Enable** à Yes | Le brouillard de ce contrôleur : couleurs, mélange, début, fin, densité max | Ce brouillard remplace celui de la map |
| Nomme un contrôleur avec **Fog Enable** à No | L'aspect par défaut (bleu clair) et l'avertissement *a son brouillard désactivé : l'enlève en jeu* | Pas de brouillard |
| Ne nomme rien, ou pas de **Fog Name** | L'aspect par défaut et *Aucun contrôleur "nom"* ou *Pas de Fog Name* | Le brouillard de la map reste |

- **Fog Name** peut finir par `*` pour prendre le premier contrôleur dont le nom commence ainsi.
- **PostProcessName** et **Color Correction Name** sont vérifiés : une entité absente ou d'une autre classe est listée en orange ; les effets eux-mêmes ne sont pas prévisualisés.
- Modifier les clés du volume ou de ses contrôleurs met l'aperçu et l'étiquette à jour tout de suite.
- Dans la [vue joueur](player-view.md), le brouillard du contrôleur remplace celui de la map tant que les yeux sont dans le volume, même avec **Aperçu du brouillard** désactivé ; un contrôleur avec **Fog Enable** à No l'enlève ; un volume avec **Start Disabled** n'y fait rien, comme en jeu. Les étiquettes et contours y sont cachés.
- **Texture des fog_volume** désactivé cache le matériau des brushes dans la vue 3D (le contour et le brouillard restent).
- MCP : `list_fog` donne chaque volume avec son brouillard résolu, `preview_fog` choisit le contrôleur de l'aperçu.

## Bancs de poissons

Un `func_fish_pool` fait apparaître `fish_count` poissons (10 par défaut) de son **World model** (`models/Junkola.mdl` par défaut) qui nagent jusqu'à **Max Range** unités (150 par défaut, 255 au plus) de l'entité. Avec **Aperçu des poissons** activé, la vue 3D les dessine dispersés dans cette distance, toujours aux mêmes places pour le même banc. Quand l'entité est dans l'eau, les poissons restent dans cette eau, au moins 8 unités sous sa surface, comme le jeu les garde sous le niveau de l'eau. Avec **Poissons qui nagent** activé, ils nagent en cercles lents autour de leur place.

- Le banc lui-même apparaît comme une boîte ; sélectionnez-le pour voir une sphère et un cercle de sa **Max Range**.
- L'aperçu dessine au plus 256 poissons par banc.
- Un modèle absent du contenu du jeu (Garry's Mod n'a pas `models/Junkola.mdl`) est prévisualisé avec le poisson rouge du contenu du jeu, `models/props/de_inferno/goldfish.mdl`, pour que le banc montre des poissons dès sa pose. En jeu un tel banc ne montre aucun poisson, aussi les propriétés de l'entité le signalent : mettez ce modèle sur le banc pour avoir les mêmes poissons en jeu. Si même ce modèle manque, chaque poisson est une petite boîte de la couleur de l'entité.

## Images de fond

**Affichage > Options d'affichage > Images de fond...** ouvre la fenêtre **Images de fond** : des images de référence (un plan, une photo) dessinées derrière la grille des vues 2D.

| Champ | Effet |
| --- | --- |
| **Ajouter une image...** | PNG, JPEG, BMP ou TGA. |
| **Vue** | **Dessus**, **Face** ou **Côté** : les vues 2D de cet axe l'affichent. |
| **Centre** | Position de son centre, en coordonnées de la vue. |
| **Unités par pixel** | Échelle (0,001 à 1000). |
| **Rotation** | En degrés. |
| **Opacité** | De 0 à 1. |
| **Retirer cette image** | La retire. |

Les images sont enregistrées dans la map (bloc `bgimages_plus` du VMF) et voyagent avec elle. Les fichiers sont seulement lus, jamais écrits.

## Distance d'affichage et limites

| Option | Où | Effet | Défaut |
| --- | --- | --- | --- |
| **Distance d'affichage** (`view.far_plane`) | Paramètres > Vues | Distance maximale dessinée par la vue 3D (2048 à 262144). | 32768 |
| **Distance d'affichage plus courte** / **plus longue** | Menu **Affichage** | Divise ou multiplie la distance d'affichage par 1,5. | |
| **Plan proche** (`render.near_plane`) | Paramètres > Rendu | Distance la plus proche dessinée, de 0,5 à 16. Augmentez-la si des surfaces lointaines scintillent. | 1 |
| **Distance des modèles** (`render.model_distance`) | Paramètres > Rendu | Les modèles plus loin que cela sont dessinés en boîtes (0 : aucune limite). | 0 |
| `render.lod_distances` | `settings.toml` | Les modèles lourds passent à leurs LOD 1 et LOD 2 au-delà de ces distances (0 : toujours le modèle complet). | 0, 0 |
| **Rayon de limite** (`render.cull_radius`) | Paramètres > Rendu | Rayon de la limite circulaire (256 à 65536). | 4096 |
| **Ignorer la géométrie cachée** (`render.occlusion_culling`) | Paramètres > Rendu | Les parties de la map cachées derrière une géométrie plus proche ne sont pas dessinées. | Activé |
| **Anticrénelage (MSAA)** (`view.msaa`) | Paramètres > Vues | Bords plus lisses, plus de mémoire GPU. | Activé |

## Images par seconde

| Option | Effet | Défaut |
| --- | --- | --- |
| **Limite d'images par seconde** (`render.max_fps`) | Nombre maximal d'images par seconde (0 : aucune limite ; avec la synchronisation verticale, l'écran la limite). | 0 |
| **En arrière-plan** (`render.unfocused_fps`) | Limite quand la fenêtre n'est pas active (0 : aucune limite). | 30 |
| **Animations** (`render.animation_fps`) | Images par seconde des matériaux animés, des effets de particules et des lumières qui clignotent quand rien d'autre ne bouge (0 : chaque image). | 30 |
| **Mettre les animations en pause après** (`render.animation_idle_pause_s`) | Les animations s'arrêtent après ce nombre de secondes sans souris ni clavier (0 : jamais). | 120 s |
| **Animer en arrière-plan** (`render.animate_in_background`) | Continuer les animations quand une autre fenêtre est active. | Désactivé |
| **Synchronisation verticale** (`render.vsync`) | Attend le rafraîchissement de l'écran (s'applique immédiatement). | Activé |

## Autres réglages de rendu

| Option | Effet | Défaut |
| --- | --- | --- |
| **Voir le ciel à travers toolsskybox** (`sky_see_through`) | Masque les faces `toolsskybox` pour voir les aperçus du ciel à travers. | Désactivé |
| **Relief (normal maps)** (`normal_maps`) | Les normal maps donnent leur relief aux surfaces dans la vue 3D. Désactivé : surfaces plates, moins de mémoire vidéo. | Activé |
| **Contour de la sélection** (`selection_outline`) | Les objets sélectionnés ont un contour visible à travers les murs. | Désactivé |
| **Modèles** (vues 2D, `models_in_2d`) | **Boîtes**, **Fil de fer**, **Fil de fer si sélectionné**. | Fil de fer si sélectionné |
| **Remplir les brushes sélectionnés** / **Opacité du remplissage** | Brushes sélectionnés remplis dans les vues 2D (opacité 0,02 à 0,6). | Activé, 0,2 |
| **Points des sommets** | Sommets des brushes dessinés en points quand on zoome. | Activé |
| **Icônes des entités** / **Taille des icônes** | Icônes des entités dans les vues 2D (8 à 48 px). | Activé, 16 px |
| **Caméra 3D** | Montre la caméra 3D dans les vues 2D. | Activé |
| **Couleurs des groupes de visibilité** | Brushes dessinés de la couleur de leur visgroup. | Désactivé |
| **Couleurs des objets en 2D** (`object_colors_2d`) | Brushes et entités dessinés dans leur couleur d'affichage (voir [Couleurs d'affichage](display-colors.md)) ; désactivé : l'aspect classique. | Activé |
| **Couleurs des objets en 3D filaire et plat** (`object_colors_3d`) | La vue 3D utilise les couleurs d'affichage en mode filaire et plat. | Désactivé |
| **Teinter les brushes func_detail** (`brush.detail.tint`, **Affichage > Options d'affichage**) | Les brushes func_detail prennent une teinte bleue en 3D et un contour bleu en 2D (voir [func_detail](func-detail.md)). | Désactivé |

## Astuces

- Sur une grosse map, **Distance des modèles**, **Limite de distance circulaire** et une **Distance d'affichage** plus courte gardent un bon nombre d'images par seconde (voir [Performances de l'éditeur](../troubleshooting/performance.md)).
- La limite en arrière-plan (30) économise de l'énergie quand vous êtes dans un autre programme.
- Outils MCP : `set_render_mode`, `screenshot_view`, `get_views`, `set_setting` (voir la [référence des outils MCP](../../MCP.md), en anglais).

## FAQ

**Mes faces de ciel montrent la texture d'outil.** C'est le comportement par défaut dans les modes texturés ; le mode éclairé et l'aperçu précalculé montrent le ciel à travers elles, comme le jeu. Activez **Voir le ciel à travers toolsskybox** pour voir le ciel à travers.

**Où sont passés mes props ?** Vérifiez **Afficher les modèles**, **Distance des modèles** et **Afficher les collisions des modèles**.

**L'aperçu du brouillard ne montre rien.** La map doit avoir un `env_fog_controller` avec **Fog Enable** à Yes (un nouveau a No, comme dans Hammer) ; la vue 3D le signale en bas à gauche. Choisissez un autre contrôleur dans **Affichage > Aperçu de l'environnement > Contrôleur de brouillard prévisualisé**.

**Mon fog_volume montre l'aspect bleu par défaut.** Son **Fog Name** ne nomme pas un `env_fog_controller` avec **Fog Enable** à Yes : l'étiquette au-dessus du volume et les propriétés de l'entité disent lequel. En jeu un tel volume ne change rien (pas de contrôleur) ou enlève le brouillard (contrôleur au brouillard désactivé).

## Pages liées

- [Aperçu de l'éclairage](lighting-preview.md)
- [Caméras de skybox](sky-cameras.md)
- [Caméra et navigation](camera-navigation.md)
- [Performances de l'éditeur](../troubleshooting/performance.md)
- [Référence des paramètres](../reference/settings.md)

---
[Retour à l'index de la documentation](../README.md)

## Une grille bleue à la place du ciel

Dans les vues texturées, une grille bleue sur une face de ciel correspond à la texture d'édition `tools/toolsskybox` lorsque **Voir le ciel à travers toolsskybox** est désactivé. Activez cette option dans **Paramètres > Rendu** (`render.sky_see_through`) pour voir le ciel à travers ces faces. Le mode éclairé (Alt+5) et l'aperçu précalculé affichent automatiquement les ouvertures de ciel, pour que les faces de ciel ne soient jamais éclairées ni ombrées comme un mur.
