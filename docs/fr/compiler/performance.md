[Documentation](../README.md) > Compilateur > Performances de compilation | [English](../../en/compiler/performance.md) | [Русский](../../ru/compiler/performance.md)

# Performances de compilation

Le compilateur de géométrie intégré recherche les fichiers de contenu à la demande au lieu d’indexer récursivement tous les dossiers à chaque lancement. La priorité des sources et la recherche sans distinction de casse sont conservées. Les index des archives restent à ouvrir et le premier accès à un addon compressé peut prendre du temps. Le journal indique le montage du contenu et le chargement des matériaux avant la géométrie ; une attente pendant ces phases ne correspond pas encore à la construction de l’arbre BSP.

Comment garder des compilations rapides pendant que vous itérez, et ce qui prend du temps dans une version finale. Pour la vitesse de l'éditeur lui-même, voir [Performances de l'éditeur](../troubleshooting/performance.md).

## Déjà fait pour vous

- **Recompilation intelligente** (Brouillon et Normal) : seul ce qui a changé est reconstruit. Déplacer une entité lance `-onlyents` (quelques secondes), modifier une lumière lance `-onlyents` plus l'éclairage (visibilité conservée), modifier des props statiques lance `-onlyprops` ; les brushes, displacements, overlays, entités de brushes, les réglages ou le jeu relancent toute la chaîne. Rien n'a changé : copie et lancement seulement.
- **Cache des étapes** : une étape de géométrie ou d'éclairage dont les entrées sont identiques à une précédente (une annulation puis une recompilation, la même map recompilée) est restaurée au lieu d'être relancée (« Mêmes entrées qu'une compilation précédente : résultat réutilisé »).
- **Visibilité conservée entre les compilations** : chaque visibilité est gardée dans le cache des étapes avec les portails dont elle vient. Quand une compilation suivante de la map a les mêmes portails (seuls des brushes de détail, des textures, des entités, des lumières ou des props ont changé : pas les brushes structurels, les areaportals, le ciel ni le rayon de visibilité), elle est reprise aussitôt au lieu d'être recalculée : « Full visibility reused: the portals are the same as in an earlier compile » dans le journal, `Vis mode: full (reused)` dans les statistiques du journal, « complète, reprise d'une compilation précédente aux mêmes portails » comme mode de visibilité de l'onglet Statistiques. La map est identique octet pour octet à celle d'un calcul complet. Six visibilités sont gardées par map, dans les 2 Go du cache des étapes.
- **Limites vérifiées avant la visibilité** : une map qui dépasse une limite du moteur s'arrête juste après la géométrie, pas après une longue visibilité.
- **Compiler pendant les pauses** : à partir de 8 threads processeur, la géométrie et la visibilité sont construites en arrière-plan 3 secondes après votre dernière modification ; F9 les réutilise et les fuites apparaissent avant même que vous compiliez. En pause sur batterie.
- **Une visibilité rapide par défaut** : le flux des portails de VVIS est borné (résultat identique ou plus prudent), et tourne sur tous les threads.
- **Un éclairage rapide** : la lumière du ciel est tracée une fois par luxel, le suréchantillonnage ne trace que les lumières qui changent sur un luxel, les échantillons d'ambiance des feuilles utilisent environ trois fois moins de rayons que VRAD. Depuis la 2.0, l'éclairage trace aussi ses rayons d'ombre plus vite (ordre des enfants moins coûteux dans l'arbre des rayons), saute les modèles de props dont aucun prop éclairé ou projetant une ombre n'a besoin (une map dont tous les props ont l'ombre coupée et pas d'éclairage par sommet n'en charge aucun), lit ses directions aléatoires dans des tables calculées une seule fois, raccorde les faces voisines et lisse les déplacements sans copier toute la lightmap, fait tourner les passes de rebond et les tests de ciel des feuilles sur tous les threads, range les luxels sur 12 octets au lieu de 16 et libère les arbres de rayons avant d'écrire le BSP. Sur les maps de test, l'éclairage prend en général 15 à 30 % de temps processeur en moins (jusqu'à 45 % sur les maps avec beaucoup de props sans ombre) et jusqu'à un tiers de mémoire en moins que la 1.2.4. Les lightmaps, les échantillons d'ambiance et les couleurs des props sont identiques, octet pour octet, à ceux d'avant ces changements.
- **Pas d'enregistrement, pas d'attente** : la compilation travaille sur une copie, la liste des fichiers à empaqueter se calcule pendant que le compilateur tourne, et le jeu peut démarrer pendant la compilation (**Démarrer le jeu : au début de la compilation**, voir [Lancer le jeu](../features/game-launch.md)).

## Gains mesurés de la 2.0

`hammerpro --bspc` de la version 1.2.4 contre la 2.0 sur le même ordinateur à 6 cœurs, occupé par d'autres travaux pendant les mesures (les temps sont plus courts sur un ordinateur au repos ; temps processeur entre parenthèses) :

| Map | Compilation | 1.2.4 | 2.0 |
|---|---|---|---|
| rp_archipel_v1 (4246 clusters) | `-vis` de nouveau après la suppression de trois `func_detail` | 434 s (1831 s) : toute la visibilité de nouveau | 14 s (4 s) : la visibilité est reprise en 0,01 s |
| gmc_op_v2 (5711 clusters) | `-vis` de nouveau après la suppression de trois `func_detail` | 54 s (177 s) | 18 s (10 s) |
| gmc_op_v2 | construction des portails (monde et fichier de portails) et écriture des lumps d'une compilation `-fastvis` | 0,91 s de temps processeur | 0,71 s |
| hokai_d (5850 displacements) | toutes | arrêt : MAX_MAP_PLANES dépassé, 67374 plans | la géométrie est reconstruite avec 46864 plans |

Les fichiers BSP sont identiques octet pour octet à ceux de la 1.2.4 (vérifié sur gm_construct, rp_archipel_v1 et gmc_op_v2, en visibilité rapide et complète), et une visibilité reprise donne les mêmes octets qu'un calcul complet.

## Pendant l'itération

| À faire | Pourquoi |
|---|---|
| Utiliser **Brouillon** | Visibilité rapide, un rebond, éclairage `-fast`, une seule plage dynamique, pas d'empaquetage. |
| Laisser **Compiler quand même les maps qui fuient** décoché | Une map qui fuit s'arrête après la géométrie au lieu d'être éclairée pour rien. |
| Utiliser **Entités seules** pour le travail sur les entités et les I/O | Quelques secondes au lieu d'une compilation complète (la recompilation intelligente le choisit toute seule). |
| Activer **Jouer tout de suite, finir l'éclairage ensuite** | Vous jouez tout de suite un brouillon ; l'éclairage Normal ou Final le remplace en arrière-plan. |
| Utiliser **Compiler uniquement la zone du cordon** | Seule la partie dans le cordon est compilée (sous `<map>_cordon`). |
| Décocher **Ombres des props statiques** sur les maps pleines de props | `-nostaticpropshadows` : les modèles des props ne sont même pas chargés. Recochez-la pour la version finale. |
| Garder **Addon de développement** coché | Les compilations d'itération copient le contenu personnalisé dans un addon au lieu de l'empaqueter dans le BSP. |

## Threads et priorité

- **Threads : Automatique** utilise tous les cœurs, ou tous sauf un avec **Garder un cœur pour l'éditeur** (activé par défaut). Vous pouvez indiquer un nombre de 1 à 256.
- `-low` (menu d'options, Vitesse) lance le compilateur en priorité basse : le reste de l'ordinateur reste réactif, la compilation peut durer plus longtemps si d'autres programmes travaillent.
- La visibilité et l'éclairage donnent le même résultat quel que soit le nombre de threads (les tests éclairent la même map sur 1, 3 et 7 threads et comparent chaque octet).
- Mémoire insuffisante sur une très grande map : baissez le nombre de threads (le journal l'explique).

## Ce qui coûte du temps dans une version finale

| Coût | Levier |
|---|---|
| Visibilité complète (`-vis`) sur les grandes maps ouvertes | Passez les petites brushes et les décors en `func_detail` (les statistiques les nomment), ajoutez des brushes hint et des areaportals, utilisez `-radius_override` ou un `farz` de brouillard en extérieur. `-fulldetail` la ralentit. |
| `-final` (plus de rayons du ciel et indirects) | Nécessaire pour la qualité finale ; `-extrasky` ajoute encore des rayons. |
| Les deux plages dynamiques | Final calcule toujours LDR et HDR (deux passes). |
| `-StaticPropLighting` | Chaque sommet de chaque prop statique est éclairé (ciel et lumière rebondie une fois par petit groupe de sommets) ; l'étape la plus longue sur les maps avec des milliers de props détaillés. Décocher **Éclairer les props statiques (par sommet)** l'évite. `-staticpropsamplescale` réduit ses rayons. |
| `-StaticPropPolys`, `-textureshadows` | Ombres de props plus fines, plus lentes. |
| Résolution des lightmaps | Augmentez l'échelle de lightmap des grandes faces ou de celles qu'on ne voit pas (16 ou plus) ; les statistiques listent les faces et matériaux avec le plus de luxels. `-luxeldensity` sous 1 rend toutes les lightmaps plus grossières. |
| Beaucoup de rebonds | `-bounce` (100 par défaut) s'arrête plus tôt ; les rebonds s'arrêtent d'eux-mêmes quand ils n'ajoutent presque plus rien. |

## Mesurer

- L'onglet **Historique** montre la durée de chaque étape de chaque compilation de la map ; une étape bien plus lente que d'habitude donne un avertissement dans le journal.
- L'onglet **Statistiques** montre la durée des phases de chaque étape (premier arbre, portails, flux, lumière directe, lumière rebondie...) et des conseils qui indiquent quoi optimiser.
- `-verbose` affiche les options réellement utilisées.
- `--rad` se termine par une ligne `Light phase times:` qui découpe l'éclairage en phases (arbres de la scène, lumière directe, déplacements, rayons et passes de rebond, raccords, ambiance des feuilles, props, écriture), voir [Ligne de commande du compilateur](command-line.md).

## FAQ

**Le journal dit que les plans dépassent la limite et que la géométrie est reconstruite.** Une map avec beaucoup de displacements peut dépasser MAX_MAP_PLANES (65536) avec les plans des brushes de displacements, qui ne vont jamais dans le BSP. Le compilateur reconstruit alors la géométrie sans eux : la map compile et la ligne du journal donne le nombre de plans.

**Pourquoi la deuxième compilation est si rapide quand je n'ai déplacé que des brushes de détail ?** Ses portails n'ont pas changé, la visibilité de la compilation précédente est donc reprise (voir Visibilité conservée entre les compilations).

**Pourquoi le deuxième F9 est-il bien plus rapide ?** La recompilation intelligente, le cache des étapes ou la précompilation en arrière-plan ont réutilisé le travail déjà fait.

**Une compilation Brouillon est-elle fausse ?** Non, c'est un aperçu grossier : la visibilité rapide s'affiche plus lentement en jeu et l'éclairage n'a qu'un rebond. Sortez la map en Final.

**`-low` ralentit-il ma compilation ?** Seulement quand d'autres programmes veulent le processeur : la priorité basse leur laisse la place.

## Pages liées
- [Options de compilation](compile-options.md)
- [Fenêtre Compiler et lancer](compile-window.md)
- [Limites et journaux](limits-and-logs.md)
- [Performances de l'éditeur](../troubleshooting/performance.md)

---
[Retour à l'index de la documentation](../README.md)
