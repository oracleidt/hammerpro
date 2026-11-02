[Documentation](../README.md) > Fonctionnalités > Vue joueur | [English](../../en/features/player-view.md)

# Vue joueur

La vue joueur vous fait parcourir la map dans la vue 3D comme un joueur de Garry's Mod : vrais déplacements, collisions, portes, triggers, le monde compilé avec ses lightmaps et des compteurs de ce que le jeu dessinerait. Rien ne change dans la map pendant que vous jouez.

## Accès

- **Affichage > Options d'affichage > Vue joueur**, le bouton vue joueur de la barre d'outils, ou `F6`.
- Elle prend la vue 3D sous la souris (sinon celle qui a le focus, sinon la première vue 3D). La disposition doit contenir une vue 3D (« La vue joueur a besoin d'une vue 3D dans la disposition. »).
- Vous démarrez sur le premier `info_player_start` (ou `info_player_deathmatch`, `info_player_combine`, `info_player_rebel`, `info_player_terrorist`, `info_player_counterterrorist`), tourné dans sa direction ; sans point d'apparition, au sol sous la caméra 3D.
- `Échap` ou `F6` ramène à l'éditeur, avec la caméra là où elle était avant.

## Touches

| Touche | Effet |
| --- | --- |
| Souris | Regarder autour. |
| `Z` `Q` `S` `D` sur AZERTY avec les touches de caméra **Auto** (`W` `A` `S` `D` en QWERTY) | Se déplacer. |
| `Espace` | Sauter. |
| `Ctrl` | S'accroupir. |
| `Maj` | Courir. |
| `V` | Noclip : voler à travers les murs. |
| `C` | Caméra à la troisième personne derrière le modèle du joueur. |
| Molette | Distance de la caméra à la troisième personne. |
| `M` | Modèle de joueur suivant (passe à la troisième personne). |
| `X` | `mat_wireframe` 0, 1, 2, 3 à tour de rôle (plus bas). |
| `E` | Utiliser : ouvrir ou fermer les portes, appuyer sur les boutons, utiliser les props, démarrer ou arrêter le train regardé. |
| `T` | Démarrer ou arrêter tous les trains de la map (plus bas). |
| `N` | Affiche les textures outils (nodraw, clips, triggers), masquées par défaut comme en jeu. |
| `B` | Monde compilé ou brushes de l'éditeur. |
| `L` | Lightmaps de la map compilée, ou ombrage de l'éditeur. |
| `H` | Masque ou affiche les compteurs. |
| `1` à `9` | Allumer ou éteindre une lumière nommée. |
| `Échap`, `F6` | Quitter. |

La ligne de touches en haut de la vue les rappelle.

## Déplacements

Les déplacements utilisent les valeurs Source de Garry's Mod : 200 unités par seconde en marchant, 400 en courant, gravité 600, vitesse de saut 200, une boîte de 32 x 32 x 72 (36 accroupi), les yeux à 64 (28 accroupi), des marches jusqu'à 18 unités. Les collisions utilisent les brushes (on traverse les triggers, hints, skips, fog, area portals, occluders, échelles invisibles et faces de ciel 2D) et les modèles de collision des props (`.phy`, comme le jeu : un arbre ne bloque qu'à son tronc). Le joueur pousse les props physiques qu'il percute. Le modèle du joueur joue les animations de Garry's Mod (repos, marche, course, accroupi, saut, noclip).

## Monde compilé et éclairage

- Après une compilation, la vue dessine le **monde compilé** (les faces telles que le compilateur les a découpées) au lieu des brushes de l'éditeur. `B` revient aux brushes de l'éditeur. Sans compilation : « Brushes de l'éditeur : compilez la map pour voir le monde découpé comme en jeu ». Quand la map a changé depuis la compilation, une ligne demande de recompiler.
- `L` éclaire le monde compilé avec les lightmaps de la map compilée et les props comme le jeu (réglage **Vue joueur : éclairage compilé**, `render.player_compiled_lighting`, activé par défaut) : les props statiques par leur éclairage par sommets précalculé (`sp_*.vhv`) quand la map l'a, sinon par l'ambient cube et les lumières du monde à leur origine d'éclairage, les autres props à l'entité `lightingorigin` quand la clé en nomme une, sinon au centre de leur modèle. Une map compilée sans éclairage est en plein éclairage, comme en jeu.
- Les **styles de lumière** de la map compilée jouent comme en jeu (dix lettres de motif par seconde). Les lumières commutables nommées suivent leurs entrées et les touches `1` à `9`.
- Les reflets utilisent les cubemaps construites en jeu quand la map compilée en a, sinon celles que dessine Hammer Pro.
- Avec **Aperçu du brouillard** activé (par défaut), le brouillard de la map de l'`env_fog_controller` de l'aperçu s'applique, comme en jeu (aucun quand son **Fog Enable** vaut No ; voir [Brouillard de la map](render-modes-display.md#brouillard-de-la-map)).
- Tant que les yeux sont dans un `fog_volume`, le brouillard de l'`env_fog_controller` nommé par son **Fog Name** remplace celui de la map, comme en jeu (la boîte autour de ses brushes, les volumes avec **Start Disabled** ignorés ; réglage **Aperçu des fog_volume**, voir [Volumes de brouillard](render-modes-display.md#volumes-de-brouillard)).

## Compteurs

Un coin de la vue affiche (masquez-le avec `H`) :

- ce qui est dessiné : appels de dessin, triangles, changements de matériau, blocs du monde visibles et masqués, modèles, instances, sprites ;
- le monde compilé dans la vue : faces, triangles, displacements, matériaux, faces de dos ignorées ;
- **En jeu** : le cluster où vous êtes, sa zone, le nombre de clusters que le jeu dessine depuis là (PVS et area portals), et les faces, triangles, displacements, props et overlays que le jeu dessinerait. Sans données de visibilité : « compilez la map avec VVIS » ;
- **Area portals** : combien sont ouverts, traversés par le regard, et les zones dessinées. Les mêmes chiffres sans area portals sont donnés pour comparer ;
- l'éclairage et les cubemaps utilisés, les styles de lumière utilisés ;
- la vitesse et la hauteur des pieds, les images par seconde et le mode de caméra (1re personne, 3e personne, noclip).

## Filaire (X)

| Valeur | Ce que vous voyez |
| --- | --- |
| `mat_wireframe 0` | Désactivé. |
| `mat_wireframe 1` | Triangles du monde vus à travers, modèles en filaire. |
| `mat_wireframe 2` | Triangles du monde sur les surfaces visibles, modèles en filaire. |
| `mat_wireframe 3` | Contours des polygones du monde vus à travers, modèles en filaire. |

En filaire, les props montrent le niveau de détail que le jeu dessinerait.

## Trains

Les trains de la map roulent dans la vue joueur comme en jeu, avec les brushes de l'éditeur comme avec le monde compilé (le modèle compilé du train, placé là où la map compilée l'a mis, bouge de la même façon).

- **`func_tracktrain` et `func_tanktrain`** partent du `path_track` nommé par leur **First Stop Target** (`target`) : leur origine se place sur la voie, montée de **Height above track** (`height`), tournée vers les voies suivantes. **Initial Speed** (`speed`) est la vitesse au départ : `0` attend qu'on le démarre. **Max Speed** (`startspeed`) est la vitesse que prend le train quand on le démarre (`E`, `T`, `StartForward`, `Toggle`...) et limite `SetSpeed`, `SetSpeedDir` et `SetSpeedReal`. Le train suit la chaîne des clés `target`, tourne en boucle sur les circuits fermés et s'arrête à un cul-de-sac ou avant une voie désactivée. Comme le moteur, il vise le point de son chemin situé 0,1 seconde plus loin, 66 fois par seconde : son origine coupe un peu les virages (environ 12 unités à 150 unités par seconde) et il passe chaque voie (`OnPass`, changement de vitesse) un instant avant de l'atteindre.
- **Virages** : avec **Change angles** (`orientationtype`) sur **Near path_tracks**, l'avant du train regarde le point du chemin situé **Distance Between the Wheels** (`wheels`) plus loin, il prend donc les virages en douceur comme le moteur. **Linear blend** et **Ease in/ease out** passent de l'orientation d'un `path_track` à celle du suivant (son propre **Orientation Type** : suivre le mouvement, ses angles, ou pas de changement). **Never**, le spawnflag **Fixed Orientation** et l'entrée `LockOrientation` gardent les angles. **No Pitch** garde le train à plat dans les pentes, **Bank Angle on Turns** (`bank`) l'incline dans les virages, **Allow Roll** prend le roulis des angles des voies, **GoldSrc rotation** retourne le train comme dans Half-Life 1 (construit face à -X).
- **Changements de vitesse** : comme le moteur, la **New Train Speed** d'un `path_track` change la vitesse des trains avec **No User Control** (spawnflag 2) ou après une voie avec **Disable train** ; un train conduit par le joueur garde sa vitesse. **Change Velocity** (`velocitytype`) fait varier la vitesse en douceur entre deux voies.
- **Voies** : une voie **Disabled** (ou `DisablePath`) arrête les trains avant elle, `EnablePath` les laisse repartir. Un embranchement avec **Branch Path** (`altpath`) prend le `target` normal jusqu'à `EnableAlternatePath` ou `ToggleAlternatePath` ; **Branch Reverse** prend la branche en marche arrière. **Teleport to THIS path track** fait sauter le train sur cette voie (sortie `OnTeleport`). **Fire once** ne déclenche `OnPass` que la première fois.
- **`func_train`** va d'un `path_corner` (ou `path_track`) au suivant à sa **speed**, attend le **wait** de chaque coin (`-1` : jusqu'à ce qu'on le redémarre), prend la **speed** du coin qu'il quitte et saute sur un coin avec **Teleport**. Un `func_train` sans nom part tout seul ; un `func_train` nommé attend `Start` ou `Toggle` (ou `E` et `T`), comme en jeu ; **Wait for retrigger** l'arrête à ce coin.
- **Monter dessus** : le joueur debout sur un train est emporté avec lui (et tourne avec lui), et un train pousse le joueur hors de son chemin. Les trains **Passable** ne bloquent pas.
- **Entités attachées** : les entités dont le **Parent** (`parentname`) est le train, ou qui sont attachées à une entité attachée au train, bougent et tournent avec lui : props, sprites (`env_sprite`...), les lumières de l'aperçu de l'éclairage, entités de brushes (leurs brushes bloquent aussi), triggers (ils se déclenchent là où est le train), `trigger_teleport` avec sa destination, et `point_teleport`.
- **Entrées et sorties** : `StartForward`, `StartBackward`, `Stop`, `Toggle`, `Resume`, `Reverse`, `SetSpeed`, `SetSpeedDir`, `SetSpeedDirAccel`, `SetSpeedReal`, `SetMaxSpeed`, `TeleportToPathTrack`, `TeleportToPathNode`, `LockOrientation`, `UnlockOrientation` pour les trains sur voie ; `Start`, `Stop`, `Toggle`, `SetSpeed` pour `func_train` ; `EnablePath`, `DisablePath`, `TogglePath`, `EnableAlternatePath`, `DisableAlternatePath`, `ToggleAlternatePath` pour `path_track` et `SetNextPathCorner` pour `path_corner`. Un `logic_auto` avec `OnMapSpawn` > `StartForward` démarre son train tout de suite. Les voies déclenchent `OnPass` et les trains `OnStart`.
- `E` sur un train le démarre à sa vitesse maximale ou l'arrête (pas un train avec **No User Control** : le journal le dit) ; `T` démarre tous les trains quand aucun ne roule, sinon les arrête tous. Les compteurs montrent combien de trains roulent.
- Quitter la vue joueur remet chaque train à son départ.

## Entrées et sorties

Les sorties des entités déclenchent des entrées sur leurs cibles après leur délai, comme en jeu. Les triggers se déclenchent quand vous entrez et sortez, `E` utilise les boutons, portes et props, `logic_auto` se déclenche au départ et `logic_timer` à l'heure. Les entrées connues de la simulation agissent sur la vue joueur : les portes s'ouvrent, les trains démarrent et s'arrêtent, des brushes et des props disparaissent, des area portals s'ouvrent, les triggers de téléportation vous envoient à leur cible. Les autres sont écrites dans le journal **Entrées et sorties :** de la vue. Rien ne change dans la map.

## Astuces

- Compilez une fois (`F9`) avant de jouer : les compteurs, les area portals et les lightmaps ont besoin de la map compilée.
- Après une compilation avec éclairage, **Map > Éclairage > Écrire les cubemaps dans la map compilée** peut enregistrer les cubemaps dessinées dans la vue joueur (voir [Aperçu de l'éclairage](lighting-preview.md)).
- Outil MCP : `player_view` (démarrer ou quitter, mode filaire, démarrer ou arrêter les trains avec `train` et `start`, l'état de chaque train dans `trains`, et plus ; voir la [référence des outils MCP](../../MCP.md), en anglais).

## FAQ

**Le joueur passe à travers le sol.** Le sol utilise un matériau que les joueurs traversent (trigger, skip, hint, fog, area portal...). Vérifiez son matériau.

**Je vois les brushes de l'éditeur et pas le monde compilé.** Compilez la map, ou appuyez sur `B` si vous êtes passé aux brushes de l'éditeur.

**Ouvrir une porte modifie-t-il ma map ?** Non. Les portes bougent seulement dans la vue joueur et se referment quand vous la quittez.

**Mon train ne bouge pas.** Sa **Initial Speed** (`speed`) vaut `0` : appuyez sur `T` ou sur `E` en le regardant, ou démarrez-le depuis un `logic_auto`. Vérifiez que son `target` nomme un `path_track` (un `path_corner` pour `func_train`) et qu'un `func_train` nommé reçoit un `Start` ou un `Toggle`.

**Mon train ignore les vitesses des voies.** Comme en jeu, la **New Train Speed** d'un `path_track` ne s'applique qu'à un train avec le spawnflag **No User Control** (ou après une voie avec **Disable train**).

## Pages liées

- [Aperçu de l'éclairage](lighting-preview.md)
- [Fuites et pointfiles](leaks-pointfiles.md)
- [E/S et vue logique](io-logical-view.md)
- [Lancer le jeu](game-launch.md)
- [Caméra et navigation](camera-navigation.md)

---
[Retour à l'index de la documentation](../README.md)
