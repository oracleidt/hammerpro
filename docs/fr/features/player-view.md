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
| `E` | Utiliser : ouvrir ou fermer les portes, appuyer sur les boutons, utiliser les props. |
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
- `L` éclaire le monde compilé avec les lightmaps de la map compilée et les props comme le jeu (réglage **Vue joueur : éclairage compilé**, `render.player_compiled_lighting`, activé par défaut). Une map compilée sans éclairage est en plein éclairage, comme en jeu.
- Les **styles de lumière** de la map compilée jouent comme en jeu (dix lettres de motif par seconde). Les lumières commutables nommées suivent leurs entrées et les touches `1` à `9`.
- Les reflets utilisent les cubemaps construites en jeu quand la map compilée en a, sinon celles que dessine Hammer Pro.
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

## Entrées et sorties

Les sorties des entités déclenchent des entrées sur leurs cibles après leur délai, comme en jeu. Les triggers se déclenchent quand vous entrez et sortez, `E` utilise les boutons, portes et props, `logic_auto` se déclenche au départ et `logic_timer` à l'heure. Les entrées connues de la simulation agissent sur la vue joueur : les portes s'ouvrent, des brushes et des props disparaissent, des area portals s'ouvrent, les triggers de téléportation vous envoient à leur cible. Les autres sont écrites dans le journal **Entrées et sorties :** de la vue. Rien ne change dans la map.

## Astuces

- Compilez une fois (`F9`) avant de jouer : les compteurs, les area portals et les lightmaps ont besoin de la map compilée.
- Après une compilation avec éclairage, **Map > Éclairage > Écrire les cubemaps dans la map compilée** peut enregistrer les cubemaps dessinées dans la vue joueur (voir [Aperçu de l'éclairage](lighting-preview.md)).
- Outil MCP : `player_view` (démarrer ou quitter, mode filaire et plus ; voir la [référence des outils MCP](../../MCP.md), en anglais).

## FAQ

**Le joueur passe à travers le sol.** Le sol utilise un matériau que les joueurs traversent (trigger, skip, hint, fog, area portal...). Vérifiez son matériau.

**Je vois les brushes de l'éditeur et pas le monde compilé.** Compilez la map, ou appuyez sur `B` si vous êtes passé aux brushes de l'éditeur.

**Ouvrir une porte modifie-t-il ma map ?** Non. Les portes bougent seulement dans la vue joueur et se referment quand vous la quittez.

## Pages liées

- [Aperçu de l'éclairage](lighting-preview.md)
- [Fuites et pointfiles](leaks-pointfiles.md)
- [E/S et vue logique](io-logical-view.md)
- [Lancer le jeu](game-launch.md)
- [Caméra et navigation](camera-navigation.md)

---
[Retour à l'index de la documentation](../README.md)
