[Documentation](../README.md) > Compilateur > Performances de compilation | [English](../../en/compiler/performance.md) | [Русский](../../ru/compiler/performance.md)

# Performances de compilation

Le compilateur de géométrie intégré recherche les fichiers de contenu à la demande au lieu d’indexer récursivement tous les dossiers à chaque lancement. La priorité des sources et la recherche sans distinction de casse sont conservées. Les index des archives restent à ouvrir et le premier accès à un addon compressé peut prendre du temps. Le journal indique le montage du contenu et le chargement des matériaux avant la géométrie ; une attente pendant ces phases ne correspond pas encore à la construction de l’arbre BSP.

Comment garder des compilations rapides pendant que vous itérez, et ce qui prend du temps dans une version finale. Pour la vitesse de l'éditeur lui-même, voir [Performances de l'éditeur](../troubleshooting/performance.md).

## Déjà fait pour vous

- **Recompilation intelligente** (Brouillon et Normal) : seul ce qui a changé est reconstruit. Déplacer une entité lance `-onlyents` (quelques secondes), modifier une lumière lance `-onlyents` plus l'éclairage (visibilité conservée), modifier des props statiques lance `-onlyprops` ; les brushes, displacements, overlays, entités de brushes, les réglages ou le jeu relancent toute la chaîne. Rien n'a changé : copie et lancement seulement.
- **Cache des étapes** : une étape de géométrie ou d'éclairage dont les entrées sont identiques à une précédente (une annulation puis une recompilation, la même map recompilée) est restaurée au lieu d'être relancée (« Mêmes entrées qu'une compilation précédente : résultat réutilisé »).
- **Compiler pendant les pauses** : à partir de 8 threads processeur, la géométrie et la visibilité sont construites en arrière-plan 3 secondes après votre dernière modification ; F9 les réutilise et les fuites apparaissent avant même que vous compiliez. En pause sur batterie.
- **Une visibilité rapide par défaut** : le flux des portails de VVIS est borné (résultat identique ou plus prudent), et tourne sur tous les threads.
- **Un éclairage rapide** : la lumière du ciel est tracée une fois par luxel, le suréchantillonnage ne trace que les lumières qui changent sur un luxel, les échantillons d'ambiance des feuilles utilisent environ trois fois moins de rayons que VRAD.
- **Pas d'enregistrement, pas d'attente** : la compilation travaille sur une copie, la liste des fichiers à empaqueter se calcule pendant que le compilateur tourne, et le jeu peut démarrer pendant la compilation (**Démarrer le jeu : au début de la compilation**, voir [Lancer le jeu](../features/game-launch.md)).

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
- La visibilité et l'éclairage donnent le même résultat quel que soit le nombre de threads.
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

## FAQ

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
