[Documentation](../README.md) > Fonctionnalités > Cubemaps | [English](../../en/features/cubemaps.md) | [Русский](../../ru/features/cubemaps.md)

# Cubemaps

Les matériaux réfléchissants (sols brillants, verre, métal, eau : tout matériau avec `$envmap env_cubemap`) ne reflètent pas la vraie scène. À la compilation, VBSP donne à chacune de leurs faces un `env_cubemap` : celui qui liste la face dans sa keyvalue **sides**, sinon **le cubemap le plus proche devant elle**. En jeu, `buildcubemaps` rend une petite image depuis chaque cubemap. Une pièce sans son propre cubemap reflète une autre pièce (ou le ciel quand la map n'en a aucun) et ses sols brillent dans le noir.

L'outil **Cubemaps** montre d'où viennent les reflets et propose où placer des cubemaps. C'est un outil que vous ouvrez : rien ne tourne et rien ne change dans la map tant que vous n'acceptez pas une suggestion.

## L'ouvrir

| Où | Comment |
|---|---|
| Menu | **Outils > Placement et props > Cubemaps...** |
| Conseiller d'optimisation | **Outil Cubemaps** sur un constat de cubemap de **Map > Optimiser la map** |
| Palette de commandes | `Ctrl+P`, tapez `cubemaps` |

L'affichage **Afficher l'attribution des cubemaps** est aussi dans **Affichage > Diagnostics**.

## Ce qui est analysé

**Analyser** examine la map actuelle (une fois à l'ouverture de la fenêtre, puis après vos modifications quand l'analyse est rapide) :

- **Faces réfléchissantes** : chaque face de brush dont le matériau utilise `$envmap env_cubemap` (lu dans les matériaux du jeu ; sans jeu configuré, les noms avec water, glass, metal, tile, marble... comptent). Les faces cachées dans d'autres brushes ou collées contre un mur sont ignorées, comme le fait le compilateur. Chaque matériau reçoit une force, son `$envmaptint` multiplié par la moyenne de son masque (`$envmapmask`, ou l'alpha de la texture de base ou de la normal map avec `$basealphaenvmapmask` ou `$normalmapalphaenvmapmask`) : les matériaux masqués presque entièrement (moins de 5 %) ne comptent pas, et les pièces sont pesées par leur surface réfléchissante multipliée par cette force.
- **Pièces** : les faces réfléchissantes sont regroupées quand elles se voient sans passer par une ouverture étroite (une porte, un couloir, une fenêtre) ; les pièces qui se voient sont fusionnées, et quelques petits éléments brillants (une moulure, un bout de sol dans une porte) rejoignent la pièce la plus proche au lieu de former la leur. Avec **Utiliser les zones du compilateur**, la map est compilée en arrière-plan comme pour le conseiller d'optimisation, et les pièces ne traversent jamais les zones découpées par les areaportals ; la fenêtre indique comment les pièces ont été trouvées.
- **Quelle face prend quel cubemap**, exactement comme VBSP : d'abord les sides, puis le cubemap le plus proche devant la face.
- Pour chaque cubemap : sa taille (`cubemapsize`) et sa mémoire, sa pièce, sa hauteur au-dessus du sol, combien de faces réfléchissantes le prennent.

La ligne de résumé donne le nombre de cubemaps et leur mémoire, les faces réfléchissantes et les pièces, puis combien de faces prennent un mauvais cubemap : un qu'elles ne voient qu'à travers un mur, ou celui d'une autre pièce alors qu'un cubemap de leur pièce est en vue (dont combien à travers un mur) et combien sont loin de leur cubemap. Une dernière ligne compte les faces qui ne peuvent pas avoir un meilleur cubemap même en acceptant toutes les suggestions : aucun endroit libre proche ne les voit (une fenêtre collée contre un autre immeuble, une cour étroite), ou leur groupe a trop peu de surface réfléchissante pour valoir un cubemap.

## Constats

| Constat | Que faire |
|---|---|
| Aucun cubemap | Chaque face réfléchissante montre le cubemap du ciel par défaut. Placez des cubemaps (acceptez les suggestions). |
| Pièce sans cubemap | La pièce a des faces réfléchissantes et aucun cubemap qui la voit. Placez-en un à hauteur des yeux. |
| Faces qui prennent le cubemap d'une autre pièce | Elles prennent un cubemap d'une autre pièce ou derrière un mur. Ajoutez-les aux sides d'un cubemap de leur pièce (suggéré) ou déplacez le cubemap. |
| Faces loin de leur cubemap | À plus de 1024 unités : le reflet ne correspond pas. Ajoutez un cubemap plus proche. |
| Cubemap dans un brush | Il est rendu noir. Sortez-le dans la pièce. |
| Cubemap dans le vide | Hors de l'espace jouable (demande les zones du compilateur). Déplacez-le dans une pièce. |
| Cubemap au ras du sol | À moins de 32 unités du sol : les reflets semblent vus depuis le sol. Montez-le à hauteur des yeux, 64 à 72 unités au-dessus du sol. |
| Cubemaps trop proches | Deux cubemaps à moins de 128 unités dans la même pièce capturent deux fois la même image. Supprimez-en un. |
| Taille trop grande ou trop petite | 256x256 coûte 2 Mo par cubemap : gardez-le pour les miroirs et les grands sols brillants. 16x16 et moins sont flous sur de grandes surfaces réfléchissantes. 0 (par défaut, 128x128) convient à la plupart des pièces. |
| Sides vers des faces supprimées | VBSP le signale. Retirez ces ids de `sides`. |
| Trop de cubemaps | Plus de 128 (ou 64 Mo) : le chargement et la taille du BSP augmentent. Un par pièce avec des surfaces réfléchissantes suffit en général. |

**Aller à** cadre les objets ou l'endroit d'un constat ; sa boîte est dessinée dans les vues tant que la fenêtre est ouverte.

## Suggestions

- **Nouveau cubemap** : un par pièce avec des faces réfléchissantes et aucun cubemap qui la voit, au centre de ses faces réfléchissantes pondéré par leur surface, à **hauteur des yeux** (64 unités au-dessus du sol, à mi-hauteur sous un plafond bas), écarté des brushes quand l'endroit est dedans ou contre l'un d'eux. Une pièce longue (plus de 1024 unités) en reçoit plusieurs, répartis sur ses faces réfléchissantes. Les pièces avec moins de 128x128 unités de surface réfléchissante pondérée (quelques moulures brillantes) n'ont qu'un constat faible, sans suggestion. Dans une pièce qui a des cubemaps, là où une grande surface réfléchissante est à plus de 1024 unités de son cubemap, un cubemap est aussi suggéré (jusqu'à quatre par pièce).
- **Façades** : les faces qui ne verraient encore leur cubemap qu'à travers un mur, sans aucun cubemap visible à moins de 2048 unités (les fenêtres d'un immeuble caché derrière un autre), sont regroupées par façade (même côté, à moins de 1024 unités) ; chaque groupe avec assez de surface réfléchissante reçoit un cubemap dans l'espace libre devant lui, à 192 à 768 unités, à la hauteur du groupe (au moins à hauteur des yeux au-dessus du sol en dessous).
- **Sides** : les faces qui prendraient encore le cubemap d'une autre pièce sont listées dans les `sides` d'un cubemap de leur propre pièce, nouveau ou existant, pour refléter la bonne pièce.

Les suggestions apparaissent en **fantômes** dans les vues 2D et 3D (cyan pour les nouveaux cubemaps, jaune pour les changements de sides) ; **Aller à** en met une en évidence et trace des lignes vers les faces qu'elle ajoute à ses sides. Les nouveaux cubemaps sont suggérés à la taille que la plupart des cubemaps de la map utilisent déjà (128x128 par défaut sur une map qui n'en a aucun) ; **Taille des nouveaux cubemaps** la garde (**Comme suggéré**) ou impose par défaut, 32x32, 64x64, 128x128 ou 256x256. Une correction de `sides` n'utilise qu'un cubemap de la pièce de la face à moins de 2048 unités.

**Accepter** applique une suggestion, **Tout accepter** les applique toutes ; chaque clic est **une étape d'annulation** (*Placement de cubemaps*). **Écarter** cache une suggestion jusqu'à la prochaine analyse. Accepter place des entités, sans construire les images : **Map > Éclairage > Reconstruire les cubemaps** les dessine dans la vue 3D, et après une compilation `buildcubemaps` en jeu ou **Map > Éclairage > Écrire les cubemaps dans la map compilée** construit les vrais (voir [Cubemaps dans le compilateur](../compiler/cubemaps.md)).

## Affichage de l'attribution des cubemaps

**Afficher l'attribution des cubemaps** (désactivé par défaut, dans la fenêtre et **Affichage > Diagnostics**) colore chaque face de la vue 3D selon le cubemap qu'elle utilisera, une couleur par cubemap : fort pour les faces réfléchissantes, léger pour les autres (les props proches reçoivent le même cubemap). Une ligne va de chaque face réfléchissante à son cubemap ; une ligne rouge (et une face rouge) signifie le cubemap d'une autre pièce ou derrière un mur. Les cubemaps sont dessinés en boîtes de leur couleur, aussi dans les vues 2D.

## MCP

| Outil | Ce qu'il fait |
|---|---|
| `analyze_cubemaps` | Toute l'analyse : cubemaps, pièces, faces réfléchissantes et leur cubemap, constats, suggestions. |
| `suggest_cubemaps` | Les suggestions ; `apply` les place (toutes ou `indices`, `size` pour les nouveaux) en une étape d'annulation. |
| `show_cubemap_assignment` | Affiche ou masque l'attribution et ouvre ou ferme la fenêtre ; regardez avec `screenshot_view`. |

Les actions `show_cubemaps` et `toggle_cubemap_assignment` marchent avec `run_action`. Les constats de cubemaps font aussi partie de `optimize_map` (sujet `cubemaps`), qui ne modifie jamais les cubemaps de lui-même. Voir [MCP.md](../../MCP.md).

## Voir aussi

- [Optimiser une map](../guides/optimizing-a-map.md)
- [Aperçu de l'éclairage](lighting-preview.md) : les reflets des cubemaps dans la vue 3D
- [Cubemaps dans le compilateur](../compiler/cubemaps.md) : comment VBSP les attribue et comment les construire
