[Documentation](../README.md) > Compilateur > Fenêtre Compiler et lancer | [English](../../en/compiler/compile-window.md)

# Fenêtre Compiler et lancer

La fenêtre **Compiler et lancer** règle la façon dont les maps sont compilées, lance la compilation et affiche en direct son journal, ses statistiques et l'historique des durées. Ses réglages sont aussi ceux qu'utilise **F9**.

## L'ouvrir

- **Fichier > Compiler et lancer...** (`Ctrl+F9`) ou le bouton **Compiler** de la barre d'outils.
- **F9** (**Fichier > Compiler maintenant**) compile avec les réglages actuels sans ouvrir la fenêtre. **Fichier > Annuler la compilation** l'arrête.
- Par défaut c'est une **fenêtre séparée** : vous pouvez la sortir de Hammer Pro ou la mettre sur un autre écran et continuer à travailler pendant la compilation. **Remettre dans la fenêtre principale** la réaffiche dans Hammer Pro, **Fenêtre séparée** l'en sort. Le choix et la position sont retenus (`separate_window`, voir [Cadres de fenêtre](../features/window-frames.md)).
- Les valeurs par défaut des mêmes réglages sont sur la page **Compilation** de la fenêtre Paramètres (**Outils > Options...**, `Ctrl+,`).

## Jeu cible

La ligne **Jeu cible** en haut indique pour quel jeu est la map ouverte (retenu dans le fichier compagnon de la map). Choisir un autre jeu monte son contenu et ses FGD s'il est installé, et la compilation suit son format BSP, ses limites et ses passes d'éclairage (la ligne indique sa version de BSP et le format de ses props statiques). Un jeu non installé compile quand même, avec le contenu monté à ce moment, mais rien n'est copié ni lancé. Voir [Jeux cibles](target-games.md).

## Paramètres

| Option | Effet | Par défaut |
|---|---|---|
| **Compilateur** | Le compilateur du jeu de la map : **Hammer Pro (intégré)**, les outils de Valve trouvés à côté du jeu, ou l'un des vôtres. **Modifier les compilateurs...** ouvre l'onglet **Compilateurs**. Avec un autre compilateur, la fenêtre affiche **Compilé par ...** : le préréglage, les modes et la plage dynamique deviennent ses options, et les menus d'options, les arguments supplémentaires et la recompilation intelligente (qui appartiennent au compilateur de Hammer Pro) sont grisés. Voir [Choisir un compilateur](compilers.md). | Hammer Pro (intégré) |
| **Préréglage** | **Brouillon**, **Normal**, **Final** ou **Personnalisé** (voir plus bas). | Normal |
| **Recompilation intelligente** | N'exécute que ce que demandent les changements depuis la dernière compilation : entités seules, éclairage seul, props seuls ou tout. Brouillon et Normal seulement. | activée |
| **Étapes : Géométrie** | **Non**, **Normal**, **Entités seules** (`-onlyents`) ou **Props seuls** (`-onlyprops`). Les modes partiels mettent à jour le BSP de la dernière compilation en quelques secondes et gardent sa visibilité et son éclairage ; sans BSP précédent, une compilation complète a lieu à la place. | Normal |
| **Visibilité** | **Non**, **Rapide** (`-fastvis`, grossière mais vite faite) ou **Normal** (`-vis`, celle dont les joueurs ont besoin). Calculée par l'étape de géométrie. | Normal |
| **Éclairage** | **Non**, **Brouillon**, **Normal** ou **Final**. | Normal |
| **Plage dynamique** | **LDR**, **HDR** ou **LDR + HDR**. Final calcule toujours les deux ; Brouillon une seule (avec LDR + HDR choisi, celle réglée dans le jeu : HDR si son `mat_hdr_level` vaut 2, sinon LDR). Pour Portal 2, CS:GO et les autres jeux uniquement HDR, LDR devient HDR. | LDR |
| Lignes de commande | Sous les étapes, la ligne de commande exacte de chaque étape. Survolez une ligne pour le sens de chaque paramètre ajouté par Hammer Pro ; clic droit > **Copier la ligne de commande**. | |
| **Compiler uniquement la zone du cordon** | Affiché quand un cordon est actif. Les objets hors de toutes les boîtes sont exclus, chaque boîte est scellée et la map s'appelle `<map>_cordon`. | activée |
| **Props statiques : Éclairer les props statiques (par sommet)** | Quand la qualité le demande (Final), précalcule la lumière de chaque sommet des props statiques (`-StaticPropLighting`). Décoché : jamais. | activée |
| **Props statiques : Ombres des props statiques** | Les props statiques projettent des ombres dans l'éclairage. Décoché : `-nostaticpropshadows`, bien plus rapide sur les maps avec beaucoup de props. | activée |
| **Avant : Enregistrer d'abord la map** | Enregistre aussi votre travail. La compilation utilise une copie et n'a jamais besoin d'enregistrer. | désactivée |
| **Avant : Compiler les objets masqués** | Comme dans Hammer, les objets masqués dans l'éditeur (visgroups, Masquer) sont exclus sauf si cette case est cochée. Les lumières masquées donnent un avertissement. | désactivée |
| **Avant : Compiler quand même les maps qui fuient** | Décoché : la compilation s'arrête juste après la géométrie quand la map fuit (`-leaktest`). | désactivée |
| **Contenu : Empaqueter le contenu personnalisé** | Intègre au BSP les matériaux et modèles personnalisés de la map, selon une règle : **Versions finales uniquement** (compilations Final et sorties) ou **Toujours**. | activée, Versions finales uniquement |
| **Contenu : Addon de développement** | Les compilations qui n'empaquettent pas copient vos matériaux personnalisés dans `garrysmod/addons/hammerpro_dev`, que le jeu charge (redémarrez le jeu une fois après la première copie). Contient aussi le script de `point_hammerpro_skybox`. | activée |
| **Contenu : Exporter les matériaux personnalisés** | Copie aussi les matériaux utilisés de vos dossiers de textures personnalisés dans le dossier du jeu. Inutile pour compiler : le compilateur les voit toujours. | désactivée |
| **Après : Copier dans le dossier maps** | Copie le BSP dans `garrysmod/maps` (ou le **Dossier des maps** de la section Avancé). | activée |
| **Après : Lancer le jeu** | Lance le jeu sur la map. **Via Steam** l'ouvre avec un lien `steam://` au lieu de l'exécutable du jeu. | activée, désactivée |
| **Après : Charger le chemin de fuite en cas de fuite** | Charge `<map>.lin` dans les vues quand la map fuit. | activée |
| **Arguments du jeu** | Arguments supplémentaires du jeu. `-game`, `-hijack` (réutilise un jeu déjà lancé) et `+map` sont ajoutés sauf si vous les indiquez ; `{map}` est remplacé par le nom de la map. | `-console -dev` |
| **Threads** | **Automatique** (tous les cœurs, ou tous sauf un quand un cœur est gardé pour l'éditeur) ou un nombre de 1 à 256 (`-threads`). | Automatique |
| **Garder un cœur pour l'éditeur** | Le nombre automatique de threads est d'un de moins que les cœurs, l'éditeur reste fluide. Nécessaire pour **Compiler pendant les pauses**. | activée |

Sous les arguments du jeu, les options de lancement (**Démarrer le jeu :** après la compilation ou au début de la compilation, **Réutiliser le jeu lancé**, **Démarrage rapide**, **Ignorer le Workshop et les addons inutilisés**, **Indiquer le temps de chargement de la map**) sont décrites dans [Lancer le jeu](../features/game-launch.md).

### Les préréglages

| Préréglage | Géométrie | Visibilité | Éclairage | Empaquetage |
|---|---|---|---|---|
| **Brouillon** | complète, arrêt sur fuite | Rapide (`-fastvis`) | Brouillon (`-bounce 1 -fast`), une seule plage dynamique, sans occlusion ambiante | non (addon de développement) |
| **Normal** | complète | Normal (`-vis`) | Normal (aucun paramètre en plus), occlusion ambiante | non (addon de développement) |
| **Final** | complète | Normal (`-vis`) | Final (`-final -StaticPropLighting`), toujours LDR + HDR, occlusion ambiante à 64 rayons | oui |
| **Personnalisé** | vos choix dans les listes d'étapes | | | selon la règle |

Changer une liste d'étape passe le préréglage en **Personnalisé**, sauf si les choix correspondent à un préréglage nommé.

### Les menus d'options

Deux menus listent tous les paramètres du compilateur, par groupe (Qualité, Vitesse, Géométrie, Visibilité, Lightmaps, Props statiques, Lumières de texture et ciel, Contenu, Débogage, Écrites d'après d'autres paramètres) : **Toutes les options de géométrie et de visibilité (hammerpro --bspc)** et **Toutes les options d'éclairage (hammerpro --rad)**. Chaque paramètre suit le préréglage (ou vos arguments supplémentaires) tant que vous ne le cochez ou décochez pas : il est alors **forcé** ou **désactivé** pour chaque compilation, avec sa valeur s'il en prend une. **Suivre de nouveau le préréglage** et **Suivre le préréglage pour toutes les options** annulent cela. Les paramètres écrits par d'autres réglages (visibilité, threads, plage dynamique, jeu, arrêt sur fuite, ombres des props statiques) sont affichés mais pas modifiables ici. Survolez un paramètre pour savoir ce qu'il fait. La liste complète est dans [Options de compilation](compile-options.md).

## Avancé

| Option | Effet | Par défaut |
|---|---|---|
| **Arguments supplémentaires de la géométrie** | Ajoutés à `hammerpro --bspc`, par exemple `-fastvis`. Un paramètre ici remplace celui du préréglage. | vide |
| **Arguments supplémentaires de l'éclairage** | Ajoutés à `hammerpro --rad`, par exemple `-bounce 4`. | vide |
| **Compiler pendant les pauses** | Quand la map n'a pas changé depuis 3 secondes, la géométrie et la visibilité sont construites en arrière-plan (un cœur gardé pour l'éditeur) : F9 les réutilise et les fuites apparaissent tôt. En pause sur batterie et pendant une autre compilation ; toute modification l'annule. | activée à partir de 8 threads processeur |
| **Jouer tout de suite, finir l'éclairage ensuite** | Compile un brouillon, lance le jeu, puis compile en Normal (ou Final avec `progressive_preset = "Final"` dans `settings.toml`) en arrière-plan et remplace la map dans le dossier maps quand vous la quittez en jeu (il attend jusqu'à cinq minutes en réessayant). | désactivée |
| **Garder un historique des durées** | Enregistre la durée de chaque étape (onglet Historique, estimations, avertissement d'étape lente). | activée |
| **Matériau de scellement du cordon** | Matériau des brushes qui ferment une boîte de cordon (le même que dans les réglages des cordons). | `TOOLS/TOOLSSKYBOX` |
| **Jeu** | L'exécutable du jeu. Vide : détecté. | détecté |
| **Dossier des maps** | Où le BSP est copié. Vide : `garrysmod/maps` du jeu. | vide |

## Compilateurs

L'onglet **Compilateurs** liste les compilateurs à gauche (celui de Hammer Pro, les outils de Valve détectés, les vôtres) et montre celui choisi à droite : ses étapes (programme, arguments de la map, arguments supplémentaires, dossier de travail, chaque option de l'outil de Valve en case à cocher avec sa valeur) et les lignes de commande exactes. Voir [Choisir un compilateur](compilers.md).

## Mode expert

L'onglet **Expert** contient des profils de compilation nommés, comme le Run Map Expert de Hammer. Cochez **Utiliser le mode expert** et le profil choisi est exécuté à la place du préréglage (les listes d'étapes sont alors grisées). Les profils experts ne tournent qu'avec le compilateur de Hammer Pro : quand un autre compilateur est choisi pour le jeu, l'onglet le signale et le profil ne sert pas (un compilateur à vous fait la même chose avec n'importe quel programme, voir [Choisir un compilateur](compilers.md)).

- Boutons : **Nouveau**, **Dupliquer**, **Renommer**, **Supprimer**, **Importer...** (le `CmdSeq.wc` de Hammer ou le `hammerplusplus_sequences.cfg` de Hammer++), **Ajouter une étape**.
- Chaque étape a une case (active), une action, ses arguments, **Continuer en cas d'échec**, et un **Programme** pour **Lancer un programme**.
- Actions : **Géométrie et visibilité** (`hammerpro --bspc <args>`), **Visibilité (ajoutée à l'étape de géométrie)** (une étape VVIS : ajoute `-vis`, ou `-fastvis` si ses arguments contiennent `-fast`, plus ses autres options VVIS, à l'étape de géométrie qui la précède), **Éclairage** (`hammerpro --rad <args>`), **Intégrer des dossiers au BSP** (les fichiers de chaque dossier donné, ou une liste `-addlist` de bspzip), **Copier un fichier**, **Supprimer un fichier**, **Renommer un fichier**, **Lancer le jeu**, **Lancer un programme**.
- Les étapes sur des fichiers ne touchent qu'aux fichiers des dossiers de la map, du dossier de compilation, du dossier du jeu et du dossier des maps ; tout autre chemin refuse le profil entier avant que quoi que ce soit ne tourne.
- Les profils importés des outils de Valve fonctionnent : `$bsp_exe`, `$vis_exe` et `$light_exe` désignent tous Hammer Pro, les étapes VBSP et VRAD deviennent ses étapes, bspzip empaquette dans l'éditeur.

Profils d'une installation neuve : **Default**, **Fast** (`-fastvis`, `-bounce 2 -noextra`), **LDR + HDR**, **Final** (`-both -final`), **Only entities**.

### Jetons

Les jetons sont remplacés dans les arguments et le programme (sans tenir compte de la casse ; un `$mot` inconnu reste tel quel). Les arguments sont d'abord découpés en mots, donc une valeur avec des espaces (`C:\Program Files (x86)\...`) reste un seul argument :

| Jeton | Valeur |
|---|---|
| `$file` | Le nom de la map (sans dossier ni extension) |
| `$ext` | L'extension de la map (`vmf`) |
| `$path` | Le dossier du VMF compilé (le dossier `.hammerpro_build`) |
| `$bspdir` | Le dossier où les maps compilées sont copiées |
| `$gamedir` | Le dossier du jeu (avec `gameinfo.txt`) |
| `$exedir` | Le dossier de l'exécutable du jeu |
| `$bsp_exe`, `$vis_exe`, `$light_exe` | L'exécutable de Hammer Pro (les trois) |
| `$game_exe` (aussi `$gameexe`) | L'exécutable du jeu |
| `$$` | Un signe dollar |

## Journal, Statistiques, Historique

- **Journal** : le journal en direct de chaque étape, avec barres de progression et état (En attente, En cours, Terminé, Échec, Ignoré, La map fuit). Cliquez une étape pour n'afficher que son journal. Filtres **Afficher les lignes d'information**, **Afficher les avertissements**, **Afficher les erreurs**, **Rechercher dans le journal**, **Défilement auto**, **Copier tout le journal**. Les erreurs et avertissements qui désignent un endroit sont cliquables (brush, face, entité, position) et les problèmes connus ont une explication et un bouton (**Aide**, **Aller à**, **Chemin de fuite**, **Vérifier**). Voir [Limites et journaux](limits-and-logs.md).
- **Statistiques** : limites, brushes, visibilité, éclairage, durée des phases et conseils d'optimisation lus dans le journal, chaque élément avec un bouton **Aller à**. Voir [Limites et journaux](limits-and-logs.md).
- **Historique** : la durée de chaque étape des compilations passées de la map (Quand, Préréglage, chaque étape, Total, Clusters, portails) avec des mini-courbes. Une étape bien plus lente que d'habitude donne un avertissement dans le journal.

## Compilation par lots

**Fichier > Compilation par lots...** compile plusieurs maps à la suite, chacune avec son préréglage. **Ajouter des maps...**, **Retirer les terminées**, **Copier dans le dossier maps**, **Tout compiler**, **Annuler**. Chaque VMF est copié dans le dossier `.hammerpro_build` à côté de lui et compilé là, vos fichiers ne sont jamais écrits.

## Astuces

- Itérez en **Brouillon** avec la **Recompilation intelligente** : déplacer une entité ne recompile que les entités, en quelques secondes.
- Le chemin de fuite se charge tout seul ; la page [Fuites et pointfiles](../features/leaks-pointfiles.md) explique comment le suivre.
- Tous les réglages de cette fenêtre sont accessibles par les outils MCP `get_compile_options` et `set_compile_options` ([référence MCP](../../MCP.md), en anglais).

## FAQ

**Pourquoi l'étape d'éclairage est-elle sautée ?** La map fuit et **Compiler quand même les maps qui fuient** est décoché : la compilation s'arrête après la géométrie.

**Pourquoi mon brouillon n'a-t-il qu'une plage dynamique ?** Brouillon n'en éclaire toujours qu'une pour rester rapide. Utilisez Normal avec **LDR + HDR**, ou Final.

**La recompilation intelligente a tout recompilé, pourquoi ?** Les brushes, les réglages, le jeu ou le dernier BSP ont changé, ou c'était la première compilation. Le libellé du bouton **Compiler** dit ce qui va tourner (« Auto : entités seules »...).

**Mes textures personnalisées sont roses en jeu après une compilation Normal.** Normal n'empaquette pas : gardez **Addon de développement** coché (et redémarrez le jeu une fois), ou passez la règle sur **Toujours**.

## Pages liées
- [Vue d'ensemble du compilateur](overview.md)
- [Choisir un compilateur](compilers.md)
- [Options de compilation](compile-options.md)
- [Lancer le jeu](../features/game-launch.md)
- [Cordons](../features/cordons.md)
- [Construire et enregistrer en contenu](../features/build-content-release.md)
- [Référence des paramètres](../reference/settings.md)

---
[Retour à l'index de la documentation](../README.md)
