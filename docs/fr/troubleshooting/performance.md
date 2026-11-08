[Documentation](../README.md) > Dépannage > Performances de l'éditeur | [English](../../en/troubleshooting/performance.md) | [Русский](../../ru/troubleshooting/performance.md)

# Performances de l'éditeur

Hammer Pro est conçu pour rester fluide sur des maps énormes, mais un contenu lourd, un GPU modeste ou un portable sur batterie peuvent quand même ralentir les vues. Cette page liste les réglages qui comptent, du moins coûteux à changer au plus coûteux. Pour la durée des compilations, voir [Performances de compilation](../compiler/performance.md).

## Mesurer d'abord

- **Paramètres > Général > Afficher les images par seconde** affiche les images par seconde dans la barre d'état.
- **Affichage > Options d'affichage > Informations de la vue 3D** (`O`) affiche la position de la caméra et, pour la vue 3D, les appels de dessin, les triangles et les blocs dessinés, et le nombre de blocs écartés.

- **Paramètres > Rendu > Performance > Afficher le temps d'image et la mémoire dans la vue 3D** (`render.perf_hud`) ajoute un affichage sur la vue 3D : images par seconde et temps d'image, temps CPU du rendu (préparation, enregistrement, envoi), temps GPU par étape (ombres, scène, effets, reflet de l'eau) quand la carte graphique gère les requêtes d'horodatage, mémoire vive et graphique utilisées, carte graphique et niveau de qualité.
- Par MCP, `get_performance` renvoie les mêmes chiffres, et `run_benchmark` fait tourner la caméra sur un tour complet et renvoie les temps d'image moyen et pire, les millisecondes CPU et GPU et la mémoire.

Gardez la fenêtre active pour comparer les images par seconde : la limite en arrière-plan et la gestion mémoire de Windows peuvent changer le résultat quand elle perd le focus. Utilisez la même map, caméra, taille de vue, mode de rendu et mode de performance. Comparez la mémoire après chargement, après passage en mode éclairé et après déplacement dans la map ; comparez les millisecondes CPU et GPU en plus des images par seconde.

## Rendu en qualité maximale et mémoire

Le rendu éclairé utilise une passe de profondeur pour la géométrie opaque du monde et des brushes, puis éclaire ces surfaces à cette profondeur. Les instances de modèles n'utilisent pas cette prépasse. L'occlusion de la lumière du ciel réutilise une table par vue avec les mêmes distances d'échantillonnage et niveaux de mipmap. Les listes des clusters écartent les lumières qui ne peuvent pas contribuer au cluster, en conservant l'ordre de priorité existant. Ces optimisations gardent les réglages de qualité maximale.

**Niveaux de détail des modèles** (`render.model_lods`) suit les seuils Source du modèle et sa taille projetée. Le CPU évite d'envoyer les niveaux inutilisés quand il peut préserver l'ordre de dessin existant ; les modèles transparents et les seuils ambigus conservent la sélection par le shader. Désactiver ce réglage garde le niveau le plus fin.

Les textures DXT1, DXT3 et DXT5 sont envoyées dans leur format BC quand la carte graphique le gère. Les formats non gérés utilisent le repli RGBA existant. Les buffers de transfert décodés sont libérés après l'envoi. Le cache de géométrie des modèles utilise un budget de 128 Mio pour les données CPU sans référence, évince les moins récemment utilisées et conserve les informations des modèles. Les maps actives, variantes de modèles et lecteurs en cours gardent les données nécessaires à la sélection et à l'édition ; ce budget de cache est distinct de la RAM totale de l'éditeur et de la mémoire GPU.

L'eau masquée et l'eau hors du frustum de la caméra ne déclenchent pas de passe de reflet. Cette élimination existait déjà en 1.1.0. En 1.1.1, les mesures de performance affichent un temps de reflet nul quand cette passe est ignorée, au lieu de reprendre la mesure du reflet précédent. L'eau visible conserve la qualité de reflet existante.

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
| **Auto** (défaut) | Qualité maximale. Seulement si la vue 3D reste sous 20 images par seconde pendant plusieurs secondes (6 sur une puce graphique intégrée, 10 sinon, hors chargement) Hammer Pro baisse la qualité d'un cran, affiche un avis, et propose **Rétablir la qualité maximale** sur la vue 3D et dans les réglages. Après un rétablissement, la réduction automatique reste suspendue jusqu'à ce que vous changiez de mode. Il ne baisse rien sur une carte graphique qui suit. |
| **Off** | Toujours la qualité maximale, jamais baissée automatiquement. |
| **Low** | Résolution de rendu à 75 %, sans anticrénelage ni bloom, 4 lumières avec ombres, ombre du soleil en 2048, textures jusqu'à 1024 pixels, l'eau reflète seulement le monde, sans occlusion de la lumière du ciel. |
| **Very low** | Résolution de rendu à 50 %, sans anticrénelage ni bloom, sans ombres des lumières, sans reflets, normal maps ni volumes de brouillard, ombre du soleil en 1024, textures jusqu'à 512 pixels, props dessinés en boîtes au-delà de 3072 unités. |

**L'échelle de rendu** dessine la vue 3D entre 25 % et 100 % de sa taille puis l'étire sur la fenêtre ; 100 % (défaut) est la qualité maximale. Elle fonctionne avec chaque mode (le plus bas des deux s'applique).

Rien de cela ne change l'aspect par défaut : la vitesse du mode par défaut vient du fait de dessiner moins de ce qu'on ne voit pas (les props et les occulteurs d'ombre hors d'une vue ne sont plus envoyés à la carte graphique, l'ombre du soleil est conservée quand la caméra bouge un peu), pas d'une baisse de qualité.

### Essayer une carte graphique plus faible

La variable d'environnement `HP_ADAPTER` choisit la carte graphique au démarrage : `integrated`, `cpu` (le rendu logiciel de Windows, bon substitut d'une carte très faible) ou une partie du nom d'une carte. `HP_PERF_BASELINE=all` désactive les accélérations de rendu par défaut pour les comparaisons. Une liste séparée par des virgules permet de les isoler : `instances`, `shadowfit`, `exposure`, `clusters`, `prepass`, `lods`, `skymarch`, `clusterbounds`. `lods` concerne seulement l'envoi par le CPU ; le niveau de détail Source sélectionné est conservé. Ces commutateurs de diagnostic ne changent pas les réglages de qualité enregistrés.

## Réglages d'images par seconde

Page **Paramètres > Rendu**, groupe **Images par seconde** :

| Option | Effet | Défaut |
|---|---|---|
| **Limite d'images par seconde** | Nombre maximal d'images par seconde (0 : aucune limite ; avec la synchronisation verticale, l'écran la limite). Baissez-la pour économiser batterie et chaleur sur un portable. | 0 |
| **En arrière-plan** | Limite d'images par seconde quand la fenêtre n'est pas active (0 : aucune limite). | 30 |
| **Synchronisation verticale** | Attend le rafraîchissement de l'écran. S'applique immédiatement. | Activé |

## Ce que dessine la vue 3D

| Réglage | Où | Effet sur la vitesse | Défaut |
|---|---|---|---|
| **Distance d'affichage** | Paramètres > Vues | Distance maximale dessinée. Plus courte, c'est plus rapide. **Affichage > Distance d'affichage plus courte** et **Distance d'affichage plus longue** la changent à la volée. | 32768 |
| **Anticrénelage (MSAA)** | Paramètres > Vues | Bords plus lisses, utilise plus de mémoire GPU. Désactivez-le sur un GPU modeste. | Activé |
| **Limite de distance circulaire** et **Rayon de limite** | Paramètres > Rendu, aussi **Affichage > Options d'affichage > Limite de distance circulaire** et la barre d'outils | Rien n'est dessiné au-delà du rayon autour de la caméra (un cercle le montre dans les vues 2D), comme le radius culling de Hammer. | Désactivé, 4096 |
| **Ignorer la géométrie cachée** | Paramètres > Rendu | Les parties de la map dont tous les objets sont masqués ne sont pas dessinées du tout. | Activé |
| **Distance des modèles** | Paramètres > Rendu | Les modèles plus loin que cela sont dessinés en boîtes (0 : aucune limite). Très utile sur les maps chargées en props. | 0 |
| **Niveaux de détail des modèles** | Paramètres > Rendu | Utilise les niveaux de détail Source du modèle selon sa taille à l'écran. Désactivé : chaque modèle utilise son niveau le plus fin. | Activé |
| **Relief (normal maps)** | Paramètres > Rendu | Désactivé : surfaces plates et moins de mémoire vidéo. | Activé |
| **Reflets des cubemaps** et **Redessiner les cubemaps après les modifications** | Paramètres > Rendu | Les matériaux avec `$envmap` reflètent des cubemaps dessinées par l'éditeur. Sans le redessin automatique, elles restent telles quelles jusqu'à **Map > Éclairage > Reconstruire les cubemaps**. | Activé, Activé |
| **Lumières avec ombres** | Paramètres > Rendu | En mode Aperçu de l'éclairage, nombre de lumières ponctuelles et de spots proches de la caméra qui projettent des ombres (0 : le soleil seulement, 16 au maximum). | 4 |
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

## Travail en arrière-plan

Les vérifications en direct (**Vérifier la map pendant l'édition**, **Chercher les fuites pendant l'édition**, Paramètres > Santé de la map, activées par défaut) tournent en arrière-plan peu après chaque modification. Sur une très grosse map avec un processeur lent, désactiver la recherche de fuites économise du travail ; la compilation trouve toujours les fuites. La précompilation en arrière-plan (dans les réglages de compilation) utilise le processeur libre pour préparer la compilation suivante.

## FAQ

**La vue 3D saccade quand je vole dans une grosse map.** Activez la **Limite de distance circulaire**, réglez une **Distance des modèles** (2048 par exemple) et vérifiez que **Ignorer la géométrie cachée** est activé.

**L'éditeur sollicite beaucoup le GPU quand je suis dans une autre fenêtre.** Baissez **En arrière-plan** (par exemple 10).

**Les ventilateurs tournent même quand rien ne bouge.** Réglez une **Limite d'images par seconde** comme 60, ou gardez la **Synchronisation verticale**.

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
