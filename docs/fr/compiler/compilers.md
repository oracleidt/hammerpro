[Documentation](../README.md) > Compilateur > Choisir un compilateur | [English](../../en/compiler/compilers.md)

# Choisir un compilateur

Par défaut, Hammer Pro compile les maps avec son propre compilateur (voir [Le compilateur intégré](overview.md)). Vous pouvez aussi compiler avec les outils de Valve (vbsp, vvis, vrad) trouvés à côté du jeu, ou avec un compilateur à vous : n'importe quels programmes, avec vos propres arguments. Le choix est gardé pour chaque jeu.

## Où choisir

- La ligne **Compilateur** en haut des paramètres de la [fenêtre Compiler et lancer](compile-window.md) (Ctrl+F9). **Modifier les compilateurs...** ouvre l'onglet **Compilateurs**.
- L'onglet **Compilateurs** de cette fenêtre et **Paramètres > Compilation > Compilateur** affichent le même éditeur.
- F9, **Fichier > Compiler maintenant**, la compilation par lots et les outils MCP utilisent le compilateur choisi pour le jeu de la map.

## Les compilateurs

L'éditeur liste les compilateurs à gauche. Celui marqué d'un ✔ construit les maps du jeu actuel ; cliquez sur un compilateur pour le voir à droite, et sur **Utiliser pour <jeu>** pour le choisir.

| Compilateur | D'où il vient | Ce que vous pouvez changer |
|---|---|---|
| **Hammer Pro (intégré)** | Hammer Pro lui-même, celui par défaut. Il ne peut pas être supprimé. | Ses options sont les préréglages, les modes et les menus d'options de la fenêtre de compilation (aussi affichés sur sa page). |
| **Valve (bin)**, **Valve (bin/win64)**, **Valve (bin/x64)** | `vbsp`, `vvis` et `vrad` trouvés dans ces dossiers du jeu (pour Garry's Mod, `GarrysMod\bin` et `GarrysMod\bin\win64`). | Tout ; vos changements sont gardés et **Restaurer** revient à ce qui a été trouvé. |
| **Hammer++ (...)**, **Hammer (...)** | Les compilateurs réglés dans `hammerplusplus\hammerplusplus_gameconfig.txt` de Hammer++ ou dans `GameConfig.txt` de Hammer, pour le même dossier de jeu, quand ils diffèrent de ceux de Valve. | Comme ci-dessus. |
| **Valve (Source SDK Base 2013 ...)** | Pour les autres jeux sans outils à eux : les outils de Source SDK Base 2013 Multiplayer ou Singleplayer de la même bibliothèque Steam. | Comme ci-dessus. |
| Vos compilateurs | **Nouveau** (des étapes de géométrie, de visibilité et d'éclairage à remplir) ou **Dupliquer** un autre. | Tout, plus **Renommer**, **Supprimer**, ⏶ ⏷ pour ordonner la liste. |

**Valeurs par défaut** supprime vos compilateurs et vos changements des compilateurs détectés, et revient au compilateur de Hammer Pro pour chaque jeu. Un jeu dont le compilateur choisi a disparu (supprimé, outils désinstallés) est compilé par Hammer Pro, avec un avertissement dans le journal.

## Étapes

Un compilateur est une liste d'étapes lancées dans l'ordre. Les boutons d'étape (**1 Géométrie (BSP)**, **2 Visibilité (VIS)**...) choisissent l'étape affichée en dessous ; **➕ Ajouter une étape** en ajoute une.

| Étape | Ce qu'elle lance | Remarques |
|---|---|---|
| **Géométrie (BSP)** | Un programme comme vbsp. | Vérifiée comme celle de Hammer Pro : un nouveau fichier `.lin` ou le message de fuite de VBSP compte comme une fuite (le pointfile se charge, **S'arrêter quand la map fuit** s'applique), et l'étape échoue quand aucun BSP n'a été écrit. |
| **Visibilité (VIS)** | Un programme comme vvis. | |
| **Éclairage (RAD)** | Un programme comme vrad. | |
| **Copier un fichier** | Copie un fichier, sans programme. | Deux chemins complets : le fichier et sa destination. Une copie dans le dossier des maps garde les cubemaps construites en jeu, comme la copie intégrée. |
| **Lancer le jeu** | Un programme lancé sans attendre (par défaut `$gameexe`). | Sautée dans les exécutions automatisées. |
| **Commande personnalisée** | N'importe quel programme, avec tous ses arguments. | |

Chaque étape a :

- **Lancer cette étape** : décochée, l'étape est gardée mais sautée.
- **Programme** : un chemin complet, ou un nom trouvé dans le `PATH`. **Parcourir...** le choisit ; ✔ **Trouvé** ou ⚠ **Introuvable** indique s'il existe.
- **Arguments de la map** (géométrie, visibilité, éclairage) : les derniers arguments, par défaut `-game $gamedir $path\$file`.
- **Arguments supplémentaires** (géométrie, visibilité, éclairage) : des arguments libres après les options, pour les options absentes des listes.
- **Arguments** (autres étapes) : tous les arguments du programme.
- **Dossier de travail** : où tourne le programme. Vide : le dossier de compilation (`$path`) ; le jeu est lancé dans son propre dossier.
- **Continuer en cas d'échec** : les étapes suivantes tournent quand même si celle-ci échoue.
- **Options** (géométrie, visibilité, éclairage) : chaque option de l'outil de Valve en case à cocher, avec un champ de valeur quand elle en prend une. Survolez une option pour savoir ce qu'elle fait. La liste complète est plus bas.

La ligne de commande d'une étape de géométrie, de visibilité ou d'éclairage est : le programme, les options de la fenêtre de compilation (voir ci-dessous), les options cochées dans l'ordre des listes, les arguments supplémentaires, puis les arguments de la map. **Lignes de commande** en bas montre exactement ce qui tourne pour la map ouverte, une fois les variables remplacées ; **Copier les lignes de commande** les copie.

### Qualité, modes et Transmettre les modes de la fenêtre de compilation

**Qualité** sur la page d'un compilateur montre le préréglage (Brouillon, Normal, Final, comme la fenêtre Run Map de Hammer) et la plage dynamique de la fenêtre de compilation : les changer ici les change aussi dans la fenêtre.

Avec **Transmettre les modes de la fenêtre de compilation** coché (par défaut), les étapes de géométrie, de visibilité et d'éclairage reçoivent ces options avant les leurs, et une étape dont le mode est **Non** est sautée :

| Fenêtre de compilation | Option |
|---|---|
| Géométrie : entités seules, props seuls | `-onlyents`, `-onlyprops` sur l'étape de géométrie |
| Visibilité : rapide | `-fast` sur l'étape de visibilité |
| Éclairage : brouillon, final | `-fast`, `-final` sur l'étape d'éclairage |
| Plage dynamique : LDR, HDR, LDR + HDR | `-ldr`, `-hdr`, `-both` sur l'étape d'éclairage |
| Threads | `-threads N` sur les trois étapes |

Ces options sont alors affichées comme **réglée par la fenêtre de compilation** dans les listes. Décochez pour les programmes qui ne lisent pas les options de Valve : les options décident alors seules.

### Variables

Les variables sont remplacées dans les programmes, les arguments, les valeurs des options et les dossiers de travail (majuscules ou minuscules). Une valeur avec des espaces reste un seul argument, donc des chemins comme `C:\Program Files (x86)\Steam\...` n'ont pas besoin de guillemets. Les guillemets regroupent vos propres mots. `$$` écrit un `$`. Le bouton **$** à côté d'un champ en insère une.

| Variable | Valeur |
|---|---|
| `$path` | Le dossier du VMF compilé (le dossier `.hammerpro_build` à côté de la map) |
| `$file` | Le nom de la map, sans dossier ni extension |
| `$ext` | L'extension de la map (`vmf`) |
| `$bspdir` | Le dossier où les maps compilées sont copiées |
| `$gamedir` | Le dossier du jeu (avec `gameinfo.txt`) |
| `$gameexe` (aussi `$game_exe`) | L'exécutable du jeu |
| `$exedir` | Le dossier de l'exécutable du jeu |
| `$bsp_exe`, `$vis_exe`, `$light_exe` | Les programmes des étapes de géométrie, de visibilité et d'éclairage de ce compilateur |

## Exécution

- Les étapes tournent comme celles du compilateur intégré : leur sortie arrive dans le journal de la fenêtre de compilation (les avertissements et erreurs sont expliqués et cliquables comme ceux de VBSP), **Annuler** arrête le programme en cours et ses processus enfants, et une étape en échec affiche son code de sortie (ou le plantage qu'il signifie).
- Après les étapes du compilateur, la suite est la même qu'avec le compilateur de Hammer Pro : l'intégration du contenu personnalisé dans le BSP, la copie dans le dossier des maps et le lancement du jeu, comme réglés dans la fenêtre de compilation. Une étape de copie ou de jeu du compilateur s'ajoute à celles-ci.
- Avant de lancer quoi que ce soit, chaque étape active est vérifiée : un programme introuvable, un dossier de travail absent, une mauvaise valeur d'option ou une copie sans deux chemins complets arrête la compilation avec un message qui nomme le compilateur et l'étape.
- La map est compilée depuis l'instantané de `.hammerpro_build`. Pour les compilateurs externes, les fichiers `func_instance` du dossier de la map sont désignés depuis ce dossier (`../instances/...`), pour que VBSP les trouve.
- La fenêtre de compilation affiche **Compilé par ...** quand un autre compilateur est choisi : les menus d'options, les arguments supplémentaires et la recompilation intelligente de la fenêtre appartiennent au compilateur de Hammer Pro et ne servent pas. La compilation pendant les pauses, Jouer tout de suite (éclairage progressif), le mode expert et les publications utilisent toujours le compilateur de Hammer Pro.

## Par MCP

`list_compilers`, `get_compiler` (avec les lignes de commande exactes), `get_compiler_flags` (chaque option avec ce qu'elle fait), `create_compiler`, `set_compiler` (les étapes avec leurs options en champs structurés, `restore` pour un compilateur détecté), `delete_compiler`, `select_compiler` (par jeu), `reset_compilers`, et `compile_with_options` avec `compiler` pour une seule compilation. Créer ou changer les programmes d'un compilateur demande `confirm: true`. Voir [Serveur MCP](../reference/mcp-server.md) et la [liste des outils](../../MCP.md).

## Options des outils de Valve

Toutes les options proposées par les listes, avec ce qu'elles font (d'après les pages de VBSP, VVIS et VRAD du Valve Developer Community et les outils du Source SDK 2013). Les options du groupe **Autres branches** ne sont lues que par les outils de CS:GO et des branches plus récentes ; les autres les refusent. Les options absentes des listes vont dans **Arguments supplémentaires**.

<!-- valve flags begin -->

### vbsp : Géométrie (BSP)

| Option | Groupe | Valeur | Défaut | Ce qu'elle fait |
|---|---|---|---|---|
| `-onlyents` | Qualité | case à cocher |  | N'importe que les entités du VMF dans le BSP existant : les brushes, les entités à brushes et l'éclairage restent tels quels. Rapide pour changer des clés d'entités. *Écrit par le mode Géométrie de la fenêtre de compilation.* |
| `-onlyprops` | Qualité | case à cocher |  | Ne met à jour que les props statiques et les props de détail du BSP existant. *Écrit par le mode Géométrie de la fenêtre de compilation.* |
| `-threads` | Vitesse | entier de 1 à 256 |  | Nombre de threads (par défaut : un par processeur). *Écrit par le réglage Threads de la fenêtre de compilation.* |
| `-low` | Vitesse | case à cocher |  | Tourne avec une priorité basse. |
| `-nodetail` | Géométrie | case à cocher |  | Retire toute la géométrie de détail (func_detail) : il reste ce qui bloque la visibilité. |
| `-fulldetail` | Géométrie | case à cocher |  | Traite toute la géométrie de détail comme du monde, elle découpe donc aussi la visibilité. |
| `-nowater` | Géométrie | case à cocher |  | Retire les brushes d'eau. |
| `-noweld` | Géométrie | case à cocher |  | Ne soude pas les sommets des faces entre eux. |
| `-nocsg` | Géométrie | case à cocher |  | Ne découpe pas les parties où les brushes se chevauchent. |
| `-noshare` | Géométrie | case à cocher |  | Écrit des arêtes propres à chaque face au lieu de les partager. |
| `-notjunc` | Géométrie | case à cocher |  | Ne corrige pas les jonctions en T (de petites fentes peuvent apparaître entre les faces). |
| `-allowdetailcracks` | Géométrie | case à cocher |  | Ne corrige pas les jonctions en T sur les brushes func_detail. |
| `-noopt` | Géométrie | case à cocher |  | Garde la coque extérieure de la map (les faces que personne ne voit de l'intérieur), que VBSP retire par défaut. |
| `-noprune` | Géométrie | case à cocher |  | N'élague pas les nœuds solides voisins de l'arbre. |
| `-nomerge` | Géométrie | case à cocher |  | Ne refusionne pas les faces découpées sur les nœuds. |
| `-nomergewater` | Géométrie | case à cocher |  | Ne refusionne pas les faces d'eau découpées. |
| `-nosubdiv` | Géométrie | case à cocher |  | Ne subdivise pas les faces pour leurs lightmaps. |
| `-snapaxial` | Géométrie | case à cocher |  | Aligne les plans parallèles aux axes sur des coordonnées entières. |
| `-micro` | Géométrie | nombre de 0 à 1000000 | `1` | Avertit des brushes plus petits que ce volume (1 par défaut). |
| `-block` | Géométrie | 2 nombres | `0 0` | Bloc de la grille où VBSP commence à découper le niveau (deux nombres). |
| `-blocks` | Géométrie | 4 nombres | `0 0 0 0` | Blocs minimum et maximum de la grille sur lesquels VBSP découpe le niveau (quatre nombres). |
| `-leaktest` | Visibilité | case à cocher |  | S'arrête quand la map fuit. Le fichier de fuite .lin est écrit dans tous les cas. |
| `-forceskyvis` | Visibilité | case à cocher |  | Calcule aussi la visibilité dans la skybox 3D. |
| `-luxelscale` | Lightmaps | nombre de 0.01 à 1000 | `1` | Multiplie l'échelle de chaque lightmap (1 par défaut ; 2 divise la densité par deux). |
| `-minluxelscale` | Lightmaps | nombre de 0.01 à 1000 | `1` | Aucune échelle de lightmap ne descend sous cette valeur (1 par défaut). |
| `-lightifmissing` | Lightmaps | case à cocher |  | Donne une lightmap à chaque surface, même celles qui n'en ont pas besoin. |
| `-bumpall` | Lightmaps | case à cocher |  | Traite chaque surface comme ayant du relief (lightmaps bumpées partout). |
| `-allowdynamicpropsasstatic` | Props | case à cocher |  | Autorise sur prop_static les modèles sans l'indicateur statique dans leurs données de prop (outils de Garry's Mod et de Team Fortress 2). |
| `-virtualdispphysics` | Props | case à cocher |  | Utilise des modèles de collision virtuels pour les déplacements au lieu de modèles précalculés. |
| `-dumpstaticprops` | Props | case à cocher |  | Écrit les props statiques dans des fichiers staticprop*.txt. |
| `-embed` | Contenu et fichiers | dossier |  | Intègre les fichiers de ce dossier au pakfile du BSP. |
| `-keepstalezip` | Contenu et fichiers | case à cocher |  | Garde le pakfile du BSP existant et reconstruit tout le reste. |
| `-replacematerials` | Contenu et fichiers | case à cocher |  | Remplace les matériaux comme indiqué dans materialsub.txt du dossier de contenu des maps. |
| `-verbose` | Débogage | case à cocher |  | Écrit plus de détails dans le journal. |
| `-verboseentities` | Débogage | case à cocher |  | Avec -verbose, laisse de côté les détails des entités à brushes. |
| `-glview` | Débogage | case à cocher |  | Écrit des fichiers .gl de la géométrie pour la visionneuse glview de Valve. |
| `-dumpcollide` | Débogage | case à cocher |  | Écrit des fichiers avec les données de collision. |
| `-novconfig` | Débogage | case à cocher |  | N'ouvre jamais le sélecteur de jeu graphique en cas d'erreur de dossier de jeu. |
| `-FullMinidumps` | Débogage | case à cocher |  | Écrit de gros fichiers de diagnostic en cas de plantage. |
| `-StaticPropCombine` | Autres branches (CS:GO et outils plus récents) | case à cocher |  | Fusionne les props statiques en modèles combinés (VBSP de CS:GO et des branches plus récentes seulement ; les autres le refusent). |
| `-KeepSources` | Autres branches (CS:GO et outils plus récents) | case à cocher |  | Avec -StaticPropCombine, garde les fichiers sources des modèles combinés (CS:GO et branches plus récentes seulement). |

### vvis : Visibilité (VIS)

| Option | Groupe | Valeur | Défaut | Ce qu'elle fait |
|---|---|---|---|---|
| `-fast` | Qualité | case à cocher |  | Seulement la première passe rapide de la visibilité : rapide mais grossière, le jeu dessine davantage. *Écrit par le mode Visibilité de la fenêtre de compilation.* |
| `-radius_override` | Qualité | nombre de 0 à 1000000 | `4096` | Impose un rayon de visibilité en unités, quoi que dise env_fog_controller. |
| `-threads` | Vitesse | entier de 1 à 256 |  | Nombre de threads (par défaut : un par processeur). *Écrit par le réglage Threads de la fenêtre de compilation.* |
| `-low` | Vitesse | case à cocher |  | Tourne avec une priorité basse. |
| `-mpi` | Vitesse | case à cocher |  | Répartit le travail sur d'autres ordinateurs avec VMPI (demande l'installation VMPI de Valve). |
| `-mpi_pw` | Vitesse | texte |  | Mot de passe VMPI, pour choisir un groupe précis de machines. |
| `-nosort` | Visibilité | case à cocher |  | Ne trie pas les portails (le tri ne sert qu'à accélérer). |
| `-tmpin` | Visibilité | case à cocher |  | Lit le fichier de portails dans \tmp\&lt;map&gt;. |
| `-tmpout` | Visibilité | case à cocher |  | Écrit les résultats dans \tmp\&lt;map&gt;. |
| `-trace` | Débogage | 2 nombres | `0 1` | Écrit un fichier de lignes qui suit la visibilité d'un cluster à un autre (deux numéros de cluster). |
| `-verbose` | Débogage | case à cocher |  | Écrit plus de détails dans le journal. |
| `-novconfig` | Débogage | case à cocher |  | N'ouvre jamais le sélecteur de jeu graphique en cas d'erreur de dossier de jeu. |
| `-FullMinidumps` | Débogage | case à cocher |  | Écrit de gros fichiers de diagnostic en cas de plantage. |

### vrad : Éclairage (RAD)

| Option | Groupe | Valeur | Défaut | Ce qu'elle fait |
|---|---|---|---|---|
| `-fast` | Qualité | case à cocher |  | Éclairage rapide et approximatif, pour tester. *Écrit par le mode Éclairage de la fenêtre de compilation (Brouillon ajoute -fast, Final ajoute -final).* |
| `-final` | Qualité | case à cocher |  | Haute qualité : plus de rayons pour le ciel et la lumière rebondie (comme -extrasky 16). *Écrit par le mode Éclairage de la fenêtre de compilation (Brouillon ajoute -fast, Final ajoute -final).* |
| `-ldr` | Qualité | case à cocher |  | N'éclaire que la plage dynamique basse (LDR). *Écrit par la Plage dynamique de la fenêtre de compilation.* |
| `-hdr` | Qualité | case à cocher |  | N'éclaire que la plage dynamique haute (HDR). *Écrit par la Plage dynamique de la fenêtre de compilation.* |
| `-both` | Qualité | case à cocher |  | Éclaire les deux plages dynamiques. *Écrit par la Plage dynamique de la fenêtre de compilation.* |
| `-extrasky` | Qualité | entier de 1 à 1024 | `16` | Trace ce nombre de fois plus de rayons pour la lumière du ciel et la lumière rebondie. |
| `-bounce` | Qualité | entier de 0 à 1000 | `100` | Nombre maximum de rebonds de la lumière (100 par défaut). |
| `-noextra` | Qualité | case à cocher |  | Pas de suréchantillonnage : plus rapide, ombres plus crénelées. |
| `-fastambient` | Qualité | case à cocher |  | Lumière ambiante par feuille de moindre qualité, pour gagner du temps. |
| `-softsun` | Qualité | nombre de 0 à 90 | `0` | Traite le soleil comme une source étendue de cette largeur en degrés : ombres douces (0 à 5 conseillé, 0 par défaut). |
| `-threads` | Vitesse | entier de 1 à 256 |  | Nombre de threads (par défaut : un par processeur). *Écrit par le réglage Threads de la fenêtre de compilation.* |
| `-low` | Vitesse | case à cocher |  | Tourne avec une priorité basse. |
| `-mpi` | Vitesse | case à cocher |  | Répartit le travail sur d'autres ordinateurs avec VMPI (demande l'installation VMPI de Valve). |
| `-mpi_pw` | Vitesse | texte |  | Mot de passe VMPI, pour choisir un groupe précis de machines. |
| `-smooth` | Lightmaps | nombre de 0 à 180 | `45` | Angle en degrés en dessous duquel les faces partagent un éclairage lissé (45 par défaut). |
| `-luxeldensity` | Lightmaps | nombre de 0 à 1 | `1` | Réduit la densité de tous les luxels de ce facteur (moins de 1, 1 par défaut). |
| `-chop` | Lightmaps | nombre de 0.1 à 1024 | `4` | Taille la plus petite d'un patch de rebond, en luxels, sur les bords (4 par défaut). |
| `-maxchop` | Lightmaps | nombre de 0.1 à 1024 | `4` | Taille la plus grande d'un patch de rebond, en luxels, à l'intérieur des faces (4 par défaut). |
| `-dispchop` | Lightmaps | nombre de 0.1 à 1024 | `8` | Taille la plus grande d'un patch de rebond sur les déplacements, en luxels (8 par défaut). |
| `-maxdispsamplesize` | Lightmaps | entier de 1 à 65536 | `512` | Taille maximale d'un échantillon de lumière sur les déplacements (512 par défaut). |
| `-LargeDispSampleRadius` | Lightmaps | case à cocher |  | Rassemble la lumière des déplacements sur une zone plus large : enlève les taches de lumière rebondie sur le terrain, plus lent. |
| `-centersamples` | Lightmaps | case à cocher |  | Déplace les centres des échantillons. |
| `-dlightmap` | Lightmaps | case à cocher |  | Met la lumière directe dans une lightmap à part, séparée de la lumière rebondie. |
| `-noskyboxrecurse` | Lightmaps | case à cocher |  | Ne projette pas d'ombres de la skybox 3D sur le monde. |
| `-StaticPropLighting` | Props | case à cocher |  | Précalcule l'éclairage de chaque sommet des props statiques (props plus beaux, plus lent). |
| `-StaticPropPolys` | Props | case à cocher |  | Les props statiques projettent leurs ombres depuis leurs polygones exacts au lieu de leurs modèles de collision. |
| `-TextureShadows` | Props | case à cocher |  | Laisse l'alpha des textures des modèles bloquer la lumière (pour les props listés dans lights.rad). |
| `-nossprops` | Props | case à cocher |  | Aucun prop statique ne projette d'ombre sur lui-même. |
| `-nodetaillight` | Props | case à cocher |  | N'éclaire pas les props de détail. |
| `-onlydetail` | Props | case à cocher |  | N'éclaire que les props de détail et la lumière ambiante des feuilles. |
| `-lights` | Contenu et fichiers | fichier |  | Un fichier de lumières (.rad) à charger en plus de lights.rad et du fichier propre à la map. |
| `-verbose` | Débogage | case à cocher |  | Écrit plus de détails dans le journal. |
| `-dump` | Débogage | case à cocher |  | Écrit des fichiers .txt de débogage. |
| `-dumpnormals` | Débogage | case à cocher |  | Écrit les normales dans les fichiers de débogage. |
| `-dumptrace` | Débogage | case à cocher |  | Écrit la scène de lancer de rayons dans des fichiers de débogage. |
| `-debugextra` | Débogage | case à cocher |  | Dessine le suréchantillonnage dans les lightmaps pour montrer où il a lieu. |
| `-loghash` | Débogage | case à cocher |  | Écrit la table de hachage des échantillons dans samplehash.txt. |
| `-rederror` | Débogage | case à cocher |  | Affiche les erreurs en rouge. |
| `-OnlyStaticProps` | Débogage | case à cocher |  | Seulement l'éclairage direct des props statiques (débogage). |
| `-StaticPropNormals` | Débogage | case à cocher |  | Éclaire les props statiques avec leurs seules normales, pour les vérifier (débogage). |
| `-stoponexit` | Débogage | case à cocher |  | Attend une touche à la fin (la compilation ne se termine alors jamais toute seule). |
| `-novconfig` | Débogage | case à cocher |  | N'ouvre jamais le sélecteur de jeu graphique en cas d'erreur de dossier de jeu. |
| `-FullMinidumps` | Débogage | case à cocher |  | Écrit de gros fichiers de diagnostic en cas de plantage. |

<!-- valve flags end -->

## FAQ

**Ai-je besoin des outils de Valve ?** Non. Le compilateur de Hammer Pro est complet ; les autres compilateurs sont là pour ceux qui les préfèrent ou qui ont besoin d'un outil à eux.

**Pourquoi ma compilation Valve est-elle plus lente que celle de Hammer Pro ?** VVIS ne borne pas le flux des portails et VRAD trace plus de rayons ; voir [Performances de compilation](performance.md).

**Où sont enregistrés les compilateurs ?** Dans `settings.toml`, `[compile]` : `compilers` (vos compilateurs et vos changements des compilateurs détectés) et `compiler_by_game` (le compilateur de chaque jeu). Voir [Référence des paramètres](../reference/settings.md).

## Pages liées
- [Fenêtre Compiler et lancer](compile-window.md)
- [Le compilateur intégré](overview.md)
- [Options de compilation](compile-options.md)
- [Jeux cibles](target-games.md)
- [Serveur MCP](../reference/mcp-server.md)

---
[Retour à l'index de la documentation](../README.md)
