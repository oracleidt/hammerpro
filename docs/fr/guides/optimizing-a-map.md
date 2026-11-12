[Documentation](../README.md) > Guides > Optimiser une map | [English](../../en/guides/optimizing-a-map.md) | [Русский](../../ru/guides/optimizing-a-map.md)

# Optimiser une map

Une map tourne bien quand le moteur ne dessine que ce que le joueur peut voir, et quand ses lumières ont un sens. Ce guide explique les quatre choses qui en décident (visleaves, func_detail, brushes hint, areaportals), puis les lumières, et comment Hammer Pro vérifie tout cela pour vous : la fenêtre **Optimiser la map** pour les mappeurs, et les outils d'optimisation du serveur MCP pour les assistants IA.

## Visleaves et PVS

À la compilation, VBSP découpe l'espace vide de la map en morceaux convexes, les **visleaves** (aussi appelées leaves ou clusters), le long des faces des brushes du monde. Les portails sont les ouvertures entre deux visleaves. VVIS calcule ensuite, pour chaque visleaf, quelles autres visleaves sont visibles depuis elle : le **PVS** (potentially visible set). En jeu, le moteur dessine tout ce qui est dans le PVS de la visleaf où se trouve la caméra.

Donc :

- moins chaque visleaf en voit, moins le moteur dessine ;
- moins il y a de visleaves et plus elles sont simples, plus VVIS est rapide ;
- une fuite casse tout : sans coque étanche il n'y a aucune visibilité (voir [Fuites et pointfiles](../features/leaks-pointfiles.md)).

Les grands espaces ouverts, les longs couloirs droits et les fenêtres alignées donnent de gros PVS. Les murs, les angles et les portes qui bloquent la vue en donnent de petits.

## func_detail

Chaque brush du monde découpe des visleaves, même une caisse, une marche, une moulure ou un pilier. Ces petits brushes découpent l'espace en une foule de minces éclats : VVIS devient lent et le culling empire parce que ces éclats voient autour de tout. Sélectionnez-les et appuyez sur **Transformer en func_detail** (`Ctrl+Alt+D`, il refuse les brushes qui scellent la map ; **Outils > Opérations sur les brushes > Sélectionner les candidats func_detail...** les trouve pour vous, voir [func_detail](../features/func-detail.md)) : un brush de détail bloque toujours les joueurs, les balles et la lumière, mais VBSP ne découpe pas de visleaves avec, donc il ne bloque pas non plus la visibilité.

Règle simple : la coque (murs, sols, plafonds, les grandes formes qui séparent les pièces) reste dans le monde ; tout ce qui est petit ou décoratif devient func_detail. Ne faites jamais un func_detail d'un brush qui scelle la map ou qui ferme une pièce : la map fuirait ou les pièces se verraient.

## Brushes hint

Un brush hint indique à VBSP où découper les visleaves. Donnez à la face qui coupe le matériau `tools/toolshint` et à toutes les autres `tools/toolsskip`. Deux emplacements classiques :

- **en haut d'une porte**, à l'horizontale : la haute visleaf au-dessus d'une porte voit par-dessus dans les pièces suivantes ; un hint à la hauteur de l'ouverture la coupe ;
- **le long du bord d'une ouverture dans un angle** : la visleaf d'un angle de couloir voit dans les deux couloirs ; un hint qui prolonge le montant de l'ouverture la sépare là où la vue à travers l'ouverture s'arrête.

Un hint n'aide que s'il réduit le PVS : mesurez toujours (compilez et comparez).

## Areaportals

Un `func_areaportal` est un seul brush fin en `tools/toolsareaportal` qui ferme complètement une ouverture. Quand chaque passage entre deux parties de la map est fermé par des brushes du monde ou des areaportals, la compilation sépare la map en **zones** (areas). À travers un areaportal ouvert le moteur ne dessine que ce qui se voit par l'ouverture ; à travers un fermé, rien. Liez le portail à sa porte (`target`) pour qu'il se ferme avec elle, et accordez `StartOpen` à la porte.

Un areaportal ne fonctionne que si :

- il touche le monde sur ses quatre côtés (aucun trou autour) ;
- il est dans une ouverture étroite : un cadre de porte, une fenêtre, le bout d'un couloir ;
- il ne touche pas un autre areaportal ;
- l'espace ne le contourne pas par une autre ouverture (sinon la map n'a qu'une zone et le portail ne masque rien).

`func_areaportalwindow` fait de même pour les fenêtres, en estompant un `func_brush` avec la distance (`FadeDist`, `FadeStartDist`).

## Lumières

Les lumières ne changent pas la visibilité, mais des lumières incohérentes se voient tout de suite et coûtent du temps de compilation :

- chaque lumière doit venir de quelque chose que le joueur voit : un modèle de lampe, une texture lumineuse, un sprite. Une lumière qui flotte en l'air sans source visible paraît fausse ;
- chaque lampe doit éclairer : un prop de lampe ou une texture auto-illuminée sans lumière à côté brille mais n'éclaire rien ;
- une lumière dans un brush ou un prop plein est bloquée : placez-la juste sous ou devant la lampe ;
- des lampes identiques doivent donner la même couleur et la même luminosité ;
- deux lumières au même endroit avec la même couleur sont une seule lumière avec la luminosité additionnée ;
- chaque lumière nommée est commutable : elle ajoute une page de lightmap sur chaque face qu'elle touche (4 au plus par face) et le moteur en accepte 32 ;
- une map avec du ciel a besoin d'un `light_environment`.

## La fenêtre Optimiser la map

**Map > Optimiser la map...** ouvre un conseiller : il vous dit ce qui n'est pas optimisé et quoi faire, en langage simple. Il ne modifie jamais la map de lui-même.

- **Actualiser** compile le BSP, les portails et la visibilité de la map actuelle en arrière-plan (rien n'est écrit à côté de la map ; l'éditeur reste utilisable) et la vérifie. La première ouverture le lance une fois. Quand la map change, une ligne indique que l'analyse n'est plus à jour.
- Le résumé donne les visleaves, les portails, les zones, et combien de visleaves une visleaf voit en moyenne et au plus.
- Les constats sont triés par impact (fort, moyen, faible). Les boutons du haut les filtrent par thème : visleaves, func_detail, hints, areaportals, lumières.
- Chaque constat dit ce qui ne va pas, pourquoi cela coûte des performances et la chose concrète à faire, par exemple *Transformez ces 12 brushes en func_detail : ils sont petits, ne scellent pas la map et coupent 40 visleaves*, *Mettez un areaportal dans le cadre de la porte 214 : porte fermée, rien derrière n'est dessiné*, ou *La lumière 88 n'a aucune lampe à côté : déplacez-la vers une lampe ou ajoutez une source visible*.
- **Aller à** sélectionne les objets du constat et les cadre dans les vues 2D et 3D ; pour un endroit sans objet (une visleaf, un plan de hint, une ouverture), il y déplace les caméras. La boîte du constat est dessinée dans les vues tant que la fenêtre est ouverte.
- Le constat func_detail a aussi **Les sélectionner** : il sélectionne les brushes suggérés sans déplacer les vues, pour les vérifier puis appuyer sur **Transformer en func_detail**.
- **Afficher les visleaves et les portails** dessine les portails de l'analyse dans les vues, colorés par visleaf (aussi **Map > Fuites et visibilité > Afficher les visleaves et les portails**). **Carte de chaleur** les colore selon les triangles dessinés depuis chaque visleaf. Les autres options d'affichage (épaisseur, rayons X, PVS depuis la caméra) sont celles de la fenêtre [Portails et visibilité](../features/leaks-pointfiles.md).

Les vérifications :

| Thème | Ce qui est vérifié |
| --- | --- |
| Visleaves | Le PVS moyen et maximal, les pires visleaves (le plus de leaves visibles, de triangles dessinés), les visleaves minuscules fragmentées, le coût de VVIS. |
| func_detail | Les brushes du monde qui devraient être func_detail : petits brushes, marches, moulures, piliers, gravats et formes complexes qui ne font pas partie de la coque. Ils sont vérifiés avec la détection de fuites : les convertir n'ouvre jamais la map. |
| Hints | Des plans de hint trouvés à partir des portails (haut des portes, bords des ouvertures dans les angles), chacun mesuré en compilant la map avec : seuls ceux qui réduisent le PVS sont listés, avec le gain. |
| Areaportals | Chaque areaportal : un seul brush, fin, ouverture étroite, scellé sur les quatre côtés, ne touchant pas un autre, lien et état de départ de la porte, séparant vraiment deux zones. Les portes sans areaportal et les ouvertures sans porte qui sont le seul passage entre deux groupes de visleaves reçoivent un areaportal suggéré, ajusté à l'ouverture. |
| Lumières | Lumières flottantes, lampes sans lumière, lumières bloquées, lampes du même modèle avec des lumières différentes, lumières redondantes, spots dirigés dans les murs, trop de lumières commutables, light_environment manquant, valeurs d'atténuation incorrectes. |

## Avec un assistant IA (MCP)

Les mêmes vérifications sont des outils MCP, et là elles peuvent appliquer les corrections (une étape d'annulation chacune, nommée `MCP: <outil>`) :

| Outil | Ce qu'il fait |
| --- | --- |
| `optimize_map` | Tout à la fois : un rapport trié par impact. `apply: "safe"` applique les corrections sûres (func_detail vérifiés, hints mesurés, areaportals des portes, réparations d'areaportals, lumières bloquées, redondantes ou cassées, light_environment manquant), `"all"` déplace aussi les lumières flottantes vers leurs lampes, ajoute des lumières aux lampes, harmonise les couleurs des lampes et met des areaportals dans les ouvertures sans porte. `measure` recompile et renvoie les visleaves et le PVS avant et après. |
| `analyze_visibility` | Le rapport de visibilité : leaves, visleaves, portails, zones avec leurs nombres de leaves, PVS total, moyen et maximal, coût de VVIS, pires visleaves, leaves minuscules, brushes qui découpent les leaves. `show` affiche les visleaves. |
| `suggest_func_detail` | Les candidats func_detail avec leur type, vérifiés contre les fuites ; `apply` les convertit. |
| `suggest_hint_brushes` | Des candidats hint mesurés en compilant ; `apply` ajoute ceux qui apportent un gain. |
| `show_visleaves` | Affiche ou masque les visleaves et portails de la dernière analyse (carte de chaleur, rayons X, grille des leaves) ; regardez-les avec `screenshot_view`. |
| `check_areaportals` | Valide chaque areaportal ; `fix` répare l'état de départ, le lien de porte, l'ajustement et supprime les inutiles. |
| `suggest_areaportals` | Les ouvertures qui devraient recevoir un areaportal, ajusté à l'ouverture ; `apply` les place. |
| `audit_lights` | L'audit de cohérence des lumières ; `fix` applique les corrections (toutes ou certains types). |

Sur une grande map l'analyse peut durer plus longtemps qu'un appel : l'outil répond alors `running: true` et l'analyse continue en arrière-plan ; rappeler l'outil avec les mêmes arguments (sans modifier la map) renvoie le résultat. `vis: "auto"` (par défaut) utilise la visibilité exacte jusqu'à 4000 brushes et la rapide au-delà.

Le serveur demande aux assistants de lancer `optimize_map` ou `analyze_visibility` avant de terminer une map. Voir [Utiliser Hammer Pro avec un assistant IA](mcp-with-ai-assistant.md) et [MCP.md](../../MCP.md).

## Exemple

Deux maps de test compilées par Hammer Pro, avant et après `optimize_map` avec `apply: "safe"` (les chiffres d'après viennent d'une nouvelle compilation de la map optimisée) : deux pièces reliées par un couloir avec des caisses, un pilier, des marches, une porte et quelques lumières ; et quatre pièces de ce genre à la suite.

| | Deux pièces, avant | après | Quatre pièces, avant | après |
| --- | --- | --- | --- | --- |
| Visleaves | 30 | 13 | 125 | 30 |
| Portails | 70 | 14 | 303 | 47 |
| Zones | 1 | 2 | 1 | 4 |
| PVS total (paires de visleaves visibles) | 726 | 169 | 6765 | 750 |
| Coût VVIS estimé | 16986 | 784 | 186830 | 7666 |

Dans les deux pièces, 8 brushes sont devenus un func_detail, 3 brushes hint mesurés ont été ajoutés, la porte a reçu un areaportal lié, la lumière dans une caisse en est sortie et les lumières en double ont été fusionnées ; dans les quatre pièces, 52 brushes sont devenus func_detail et les 3 portes ont reçu leurs areaportals.

## FAQ

**La fenêtre dit que la map fuit.** Corrigez d'abord la fuite : sans coque étanche il n'y a pas de visibilité, donc rien d'autre ne peut être mesuré. Voir [Fuites et pointfiles](../features/leaks-pointfiles.md).

**Pourquoi un brush n'est-il pas proposé en func_detail ?** Il fait partie de la coque (il touche l'extérieur), il est trop grand, il n'est pas fait de matériaux étanches, ou le transformer en func_detail ouvrirait la map.

**Pourquoi si peu de suggestions de hint ?** Seuls les hints qui réduisent le PVS quand la map est compilée avec sont listés. Beaucoup de maps n'en ont besoin d'aucun.

**L'analyse modifie-t-elle mes fichiers ?** Non. Elle compile en mémoire ; rien n'est écrit à côté de la map.
