[Documentation](../README.md) > Fonctionnalités > Modes de rendu et options d'affichage | [English](../../en/features/render-modes-display.md)

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
| **Aperçu de l'éclairage précalculé** | `Ctrl+F5` | L'éclairage que le jeu aura, calculé par le compilateur intégré (voir [Aperçu de l'éclairage](lighting-preview.md)). |

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
| **Boîtes englobantes** (`I`) | Boîtes englobantes des objets visibles. | Désactivé |
| **Afficher les overlays** | Les overlays dessinés sur leurs faces (voir [Décals et overlays](decals-overlays.md)). | Activé |
| **Afficher les connexions** | Lignes des entrées/sorties : aucune, sélection, toutes (chaque clic passe à la suivante). Voir [E/S et vue logique](io-logical-view.md). | Sélection |
| **Afficher les objets d'éditeur** | Icônes d'entités et aides qui n'existent que dans l'éditeur. | Activé |
| **Afficher les entités ponctuelles** | Toutes les entités ponctuelles. | Activé |
| **Afficher les noms des entités** | Noms à côté des entités ponctuelles dans les vues 2D. | Activé |
| **Aides de toutes les entités** | Les aides de toutes les entités au lieu de la sélection seulement. | Désactivé |
| **Afficher les cordes** / **Simuler les cordes** | Les cordes telles qu'elles pendent en jeu ; simulées, elles se balancent avant de se stabiliser. | Activé / Désactivé |
| **Effets de rendu** | Opacité, mode et couleur de rendu, groupes de corps, animation par défaut et échelle des entités. | Activé |
| **Aperçu du ciel** | Le ciel 2D (`skyname` du worldspawn) derrière la vue 3D. | Activé |
| **Skybox 3D** (`Alt+V`) | La pièce autour de la `sky_camera`, à son échelle, autour de la map (voir [Caméras de skybox](sky-cameras.md)). | Activé |
| **Modèles de collision** | Les enveloppes de collision dessinées sur les props (aussi dans l'aperçu du navigateur de modèles). | Désactivé |
| **Afficher les collisions des modèles** | La vue 3D montre les modèles de collision à la place des modèles (un modèle sans collision montre sa boîte en orange). | Désactivé |
| **Aperçu du brouillard** | Sous-menu : **Désactivé**, ou le brouillard d'un `env_fog_controller` de la map. | Désactivé |
| **Matériaux animés** | Les matériaux animés et les sprites s'animent. | Activé |
| **Fondu des modèles** | Les props disparaissent entre leurs distances de fondu, comme en jeu. | Désactivé |
| **Matériaux opaques** | Les matériaux translucides sont dessinés opaques. | Désactivé |
| **Pentes praticables** | Teinte les surfaces trop pentues pour que les joueurs s'y tiennent. | Désactivé |
| **Afficher les displacements** | Les displacements sont dessinés. | Activé |
| **Maillage des displacements en vue 2D** | Fait passer les vues 2D de la grille de triangles des displacements à leur contour. | Maillage |
| **Praticabilité des displacements** | Colore les triangles des displacements selon leur praticabilité (voir [Outil displacement](displacement-tool.md)). | Désactivé |
| **Limite de distance circulaire** | Rien n'est dessiné au-delà du rayon de limite autour de la caméra (un cercle le montre dans les vues 2D). | Désactivé |
| **Surligner les brushes invalides** | Les brushes qui ne sont pas fermés et convexes sont dessinés en rouge dans toutes les vues. | Activé |
| **Afficher le nodraw** | Affiche ou masque les faces nodraw dans la vue 3D. | Affiché |
| **Aperçu de l'eau** | L'eau dessinée comme en jeu : réfraction, brouillard de l'eau, reflet de la scène et du ciel, vagues animées. Désactivé : sa texture d'outil. | Activé |
| **Aperçu des fog_volume** | Chaque `fog_volume` est rempli du brouillard de son contrôleur (voir [Volumes de brouillard](#volumes-de-brouillard)). | Activé |
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

## Volumes de brouillard

Un `fog_volume` change le brouillard des joueurs dont les yeux sont dedans : le jeu utilise alors l'`env_fog_controller` nommé par son **Fog Name** au lieu du brouillard de la map. Comme en jeu, le volume est la boîte autour de tous ses brushes, quelle que soit leur forme.

Avec **Aperçu des fog_volume** activé, la vue 3D remplit cette boîte du brouillard du contrôleur : sa couleur principale (et la secondaire avec **Fog Blend**, selon **Primary Fog Direction** ou les angles avec **Use Angles for Fog Dir**), **Fog Start**, **Fog End** et **Fog Max Density**. Le brouillard est aussi épais que la vue traverse la boîte, comme si vous vous teniez à son bord : ce qui est dans le volume ou derrière lui est embrumé comme vu de l'intérieur. Modifier les clés du volume ou de son contrôleur met l'aperçu à jour tout de suite.

- Le contrôleur doit avoir **Fog Enable** à Yes, comme en jeu : avec No, le volume enlève le brouillard et l'aperçu ne montre rien.
- **Fog Name** peut finir par `*` pour prendre le premier contrôleur dont le nom commence ainsi.
- Le **Color Correction Name** (une table `color_correction`) n'est pas prévisualisé.
- Dans la [vue joueur](player-view.md), le brouillard du volume remplace celui de la map tant que les yeux sont dedans, même avec **Aperçu du brouillard** désactivé ; un volume avec **Start Disabled** n'y fait rien, comme en jeu.
- **Texture des fog_volume** désactivé cache le matériau du brush dans la vue 3D (sélectionnez le volume dans une vue 2D).

## Bancs de poissons

Un `func_fish_pool` fait apparaître `fish_count` poissons (10 par défaut) de son **World model** (`models/Junkola.mdl` par défaut) qui nagent jusqu'à **Max Range** unités (150 par défaut, 255 au plus) de l'entité. Avec **Aperçu des poissons** activé, la vue 3D les dessine dispersés dans cette distance, toujours aux mêmes places pour le même banc. Quand l'entité est dans l'eau, les poissons restent dans cette eau, au moins 8 unités sous sa surface, comme le jeu les garde sous le niveau de l'eau. Avec **Poissons qui nagent** activé, ils nagent en cercles lents autour de leur place.

- Le banc lui-même apparaît comme une boîte ; sélectionnez-le pour voir une sphère et un cercle de sa **Max Range**.
- L'aperçu dessine au plus 256 poissons par banc.
- Un modèle absent du contenu du jeu (Garry's Mod n'a pas `models/Junkola.mdl`) montre chaque poisson comme une petite boîte de la couleur de l'entité.

## Images de fond

**Affichage > Images de fond...** ouvre la fenêtre **Images de fond** : des images de référence (un plan, une photo) dessinées derrière la grille des vues 2D.

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
| **Synchronisation verticale** (`render.vsync`) | Attend le rafraîchissement de l'écran (s'applique après un redémarrage). | Activé |

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

## Astuces

- Sur une grosse map, **Distance des modèles**, **Limite de distance circulaire** et une **Distance d'affichage** plus courte gardent un bon nombre d'images par seconde (voir [Performances de l'éditeur](../troubleshooting/performance.md)).
- La limite en arrière-plan (30) économise de l'énergie quand vous êtes dans un autre programme.
- Outils MCP : `set_render_mode`, `screenshot_view`, `get_views`, `set_setting` (voir la [référence des outils MCP](../../MCP.md), en anglais).

## FAQ

**Mes faces de ciel montrent la texture d'outil.** C'est le comportement par défaut. Activez **Voir le ciel à travers toolsskybox** pour voir le ciel à travers.

**Où sont passés mes props ?** Vérifiez **Afficher les modèles**, **Distance des modèles** et **Afficher les collisions des modèles**.

**L'aperçu du brouillard ne montre rien.** Choisissez un contrôleur dans **Affichage > Options d'affichage > Aperçu du brouillard** : la map doit avoir un `env_fog_controller`.

**Mon fog_volume ne montre aucun brouillard.** Son **Fog Name** doit être le nom d'un `env_fog_controller` avec **Fog Enable** à Yes et une **Fog Max Density** au-dessus de 0.

## Pages liées

- [Aperçu de l'éclairage](lighting-preview.md)
- [Caméras de skybox](sky-cameras.md)
- [Caméra et navigation](camera-navigation.md)
- [Performances de l'éditeur](../troubleshooting/performance.md)
- [Référence des paramètres](../reference/settings.md)

---
[Retour à l'index de la documentation](../README.md)
