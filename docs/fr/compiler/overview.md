[Documentation](../README.md) > Compilateur > Vue d'ensemble | [English](../../en/compiler/overview.md)

# Le compilateur intégré

Hammer Pro compile lui-même les maps. La géométrie, les portails, la recherche de fuites et la visibilité sont construits par `hammerpro --bspc`, l'éclairage par `hammerpro --rad`, et l'empaquetage du contenu personnalisé, la copie vers le jeu et le lancement tournent dans l'éditeur. Pas besoin de VBSP, VVIS, VRAD ni bspzip, et aucun chemin de compilateur à régler. C'est le compilateur par défaut ; les outils de Valve trouvés à côté du jeu, ou vos propres programmes, peuvent être choisis à la place pour chaque jeu (voir [Choisir un compilateur](compilers.md)).

## Les étapes

Une compilation enchaîne ces étapes (une étape réglée sur **Non** est sautée) :

| Étape | Ce qu'elle fait | Programme |
|---|---|---|
| **Géométrie** | Fusionne les fichiers des `func_instance`, lit les brushes, construit l'arbre BSP et les portails, cherche les fuites, puis calcule la visibilité si on la demande. Écrit `<map>.bsp`, `<map>.prt` (map étanche) ou `<map>.lin` (fuite). | `hammerpro --bspc` |
| **Éclairage** | Éclaire le BSP sur place : lightmaps, cubes d'ambiance, lumières du monde, props de détail, et selon les options l'éclairage par sommet des props statiques et l'occlusion ambiante. | `hammerpro --rad` |
| **Empaquetage du contenu** | Intègre au pakfile du BSP les matériaux, modèles et autres fichiers personnalisés utilisés par la map, puis relit le pakfile et signale ce qui manque. | intégré |
| **Copie dans le dossier maps** | Copie le BSP dans `garrysmod/maps` (ou le dossier choisi) via un fichier temporaire puis un renommage, pour que le jeu ne voie jamais une map à moitié écrite. Les cubemaps déjà construites en jeu sont reprises. | intégré |
| **Lancement du jeu** | Lance le jeu sur la map, ou la charge dans le jeu déjà ouvert. | intégré |

Dans l'étape de géométrie, le compilateur suit les étapes du VBSP de Valve : plans, contenus des matériaux (d'après les clés `%compile` du VMT), brushes avec leurs biseaux de collision, projections de texture et de lightmap, un premier arbre des brushes structurels pour trouver l'intérieur de la map et les fuites, l'arbre final avec les détails, les faces (fusionnées, découpées pour leurs lightmaps, sans jonctions en T), les zones et areaportals, les `func_viscluster`, les occulteurs, les échantillons `env_cubemap`, les props de détail (`detail.vbsp`), l'eau, puis les lumps du fichier. La visibilité suit le VVIS de Valve (visibilité de base des portails, flux des portails, fusion des clusters, PAS).

## Ce que fait F9

- **F9** (**Fichier > Compiler maintenant**) compile tout de suite avec les derniers réglages, sans ouvrir de fenêtre. La barre d'état affiche la progression.
- **Ctrl+F9** (**Fichier > Compiler et lancer...**) ouvre la [fenêtre Compiler et lancer](compile-window.md), où se règlent le préréglage et toutes les options.
- **Fichier > Annuler la compilation** arrête une compilation en cours (tous les processus lancés sont tués).

Une compilation n'enregistre jamais la map ouverte. Hammer Pro prend une copie du document et l'écrit dans `<dossier de la map>/.hammerpro_build/` : votre VMF n'est pas modifié (l'option **Enregistrer d'abord la map** enregistre seulement votre travail en même temps). Une map jamais enregistrée demande d'abord où l'enregistrer.

## Fichiers produits

| Fichier | Où | Quand |
|---|---|---|
| `<map>.vmf` (la copie) | `.hammerpro_build/` à côté de la map | à chaque compilation |
| `<map>.bsp` | `.hammerpro_build/`, puis copié dans le dossier maps | à chaque étape de géométrie réussie |
| `<map>.prt` | `.hammerpro_build/` | map étanche (pas de fichier de portails pour une map qui fuit) |
| `<map>.lin` | `.hammerpro_build/` | quand la map fuit (chargé tout seul comme chemin de fuite), ou avec `-trace` |
| `<map>.log` | `.hammerpro_build/` | à chaque compilation : chaque ligne du compilateur, ajoutée à la suite, fins de ligne CRLF comme les outils de Valve |
| `<map>_cordon.bsp` | comme ci-dessus | avec **Compiler uniquement la zone du cordon** : la map complète n'est jamais écrasée |

Hammer Pro garde aussi, dans son dossier de cache (`%LOCALAPPDATA%\HammerPro` sous Windows) : le cache des étapes (`builds`, 2 Go au plus, les plus anciennes entrées partent d'abord), les précompilations en arrière-plan (`spec`), l'historique des durées (`compile_history/<map>.jsonl`) et les éclairages progressifs (`progressive`).

## Différences avec VBSP, VVIS et VRAD

Ce que le code et ses commentaires indiquent :

- **Un programme, deux étapes.** La visibilité fait partie de l'étape de géométrie (`-vis` ou `-fastvis` sur `--bspc`). Les séquences de commandes écrites pour VBSP, VVIS, VRAD et bspzip fonctionnent toujours : leurs étapes deviennent celles de Hammer Pro (voir [fenêtre Compiler et lancer](compile-window.md)).
- **Les mêmes options.** `--bspc` lit les options de VBSP et VVIS, `--rad` celles de VRAD. Les options de Valve que ce compilateur ne fait pas (sorties de débogage de VBSP, MPI de VVIS, `-glview`, `-nocsg`...) donnent un avertissement et sont ignorées ; une option inconnue n'est jamais passée sous silence. Voir [Ligne de commande](command-line.md).
- **Les mêmes messages.** Le compilateur écrit volontairement les messages de VBSP (`**** leaked ****`, `Entity X (x y z) leaked!`, `Brush N: ...`), pour que l'éditeur les rende cliquables et les explique.
- **Une visibilité plus rapide.** Le flux des portails de VVIS peut prendre des heures sur les grandes maps ouvertes. Par défaut Hammer Pro borne ce flux (plans séparateurs partagés, flux fusionnés, résultats réutilisés dans les deux sens). Chaque raccourci ne fait que sauter du travail ou agrandir les polygones, donc le résultat reste prudent : un cluster n'est jamais caché à un cluster qui peut le voir. Le résultat ne dépend pas du nombre de threads. `-fastvis` donne exactement les lignes de VVIS `-fast`.
- **Un éclairage plus rapide, les mêmes formules.** Les lumières, l'atténuation, les cônes des spots, les lumières de texture et l'éclairage des props statiques suivent le code de VRAD (Source SDK 2013, vérifié avec les outils de Garry's Mod). Hammer Pro trace la lumière du ciel une fois par luxel au lieu d'une fois par sous-échantillon, essaie chaque lumière aux coins et au milieu d'un luxel avant de le suréchantillonner, et choisit les échantillons d'ambiance des feuilles avec moins de rayons. L'éclairage écrit les mêmes octets à chaque fois, quel que soit le nombre de threads.
- **Des ajouts.** L'occlusion ambiante (`-ao`, active par défaut en Normal et Final), `-nostaticpropshadows`, `-nodispshadows`, `-wsaddons`, `-serveraddons`, `-blocksize`, `-propdebug` (VBSP n'a pas de rapport sur les props), des statistiques et des conseils d'optimisation en fin de journal, et une vérification de chaque BSP écrit (chaque index dans son lump, chaque compte sous les limites du moteur) : une map qui passe se charge.
- **Le contenu de l'éditeur.** La compilation monte les mêmes sources de contenu que l'éditeur (objets du Workshop, téléchargements des serveurs), donc un matériau visible dans l'éditeur n'est jamais compilé comme manquant.
- **D'autres jeux.** Un seul compilateur écrit les formats BSP de plusieurs branches de Source (`-target`), voir [Jeux cibles](target-games.md).
- **Les trains sans origine.** Un `func_tracktrain`, `func_tanktrain` ou `func_train` sans clé `origin` ni brush d'origine reçoit le centre de ses brushes comme origine, avec un avertissement dans le journal (`has no origin key nor origin brush`). VBSP laisse un tel train à l'origine du monde, et le jeu le dessine alors loin de son chemin. Les maps enregistrées par Hammer ont toujours la clé.
- **Mélanges à 4 textures.** Les rangées de mélange à 4 textures des displacements (`multiblend`, `alphablend`, `multiblend_color_N` de Hammer++) vont dans le lump multiblend des displacements (63) avec le drapeau multiblend de chaque displacement, octet pour octet comme le VBSP de Garry's Mod, pour les cibles Garry's Mod, Source 2013 et CS:GO. Un displacement Lightmapped_4WayBlend sans rangées reçoit des rangées tirées de son alpha de sommet (texture 2 là où il est peint), comme l'éditeur le montre ; VBSP n'en écrit aucune. Le décompilateur relit le lump dans les rangées.
- **Non écrits.** Le lump des positions d'échantillons de lightmap des displacements (seul VRAD le lit) et les lightmaps par texel des props avec `generatelightmaps`.

## Par rapport à Hammer++

Hammer Pro lit ce que Hammer++ ajoute pour ses compilations : la clé worldspawn `_lightsrad` (un fichier de lumières propre à la map, voir [Clés worldspawn et lumières](worldspawn-keys.md)) et son fichier de séquences de commandes `hammerplusplus_sequences.cfg` (importé en profils experts). Le profil de limites **VBSP++** de la fenêtre d'informations sur la map montre les limites de ces outils (voir [Limites et journaux](limits-and-logs.md)).

## FAQ

**Faut-il installer le Source SDK ou Hammer ?** Non. Seulement le jeu (pour son contenu et pour jouer la map).

**Puis-je encore compiler avec VBSP, VVIS et VRAD ?** Oui : choisissez **Valve (bin)** ou **Valve (bin/win64)** dans la ligne **Compilateur** de la fenêtre de compilation, ou créez un compilateur à vous. Voir [Choisir un compilateur](compilers.md).

**Le BSP est-il identique octet pour octet à celui de VBSP ?** Non, et ce n'est pas le but. Le but est une map correcte qui rend bien en jeu ; les formats, les limites et les formules suivent les outils de Valve.

**Peut-on lancer le compilateur hors de l'éditeur ?** Oui, `hammerpro --bspc` et `hammerpro --rad` marchent depuis un terminal ou un script. Voir [Ligne de commande](command-line.md).

**Un assistant IA peut-il compiler ?** Oui, avec les outils MCP `compile_map`, `compile_with_options`, `get_compile_log` et d'autres. Voir [Serveur MCP](../reference/mcp-server.md).

## Pages liées
- [Fenêtre Compiler et lancer](compile-window.md)
- [Choisir un compilateur](compilers.md)
- [Options de compilation](compile-options.md)
- [Ligne de commande](command-line.md)
- [Clés worldspawn et lumières](worldspawn-keys.md)
- [Jeux cibles](target-games.md)
- [Limites et journaux](limits-and-logs.md)
- [Performances de compilation](performance.md)
- [Compiler et tester dans Garry's Mod](../guides/compile-and-test-gmod.md)
- [Fuites et pointfiles](../features/leaks-pointfiles.md)

---
[Retour à l'index de la documentation](../README.md)
