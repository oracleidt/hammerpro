[Documentation](../README.md) > Compilateur > Limites et journaux | [English](../../en/compiler/limits-and-logs.md)

# Limites et journaux

Les limites du moteur qu'une map ne doit pas dépasser, comment Hammer Pro les mesure, et comment lire le journal de compilation, ses explications et ses statistiques.

## Limites du moteur

Les outils et le moteur de Garry's Mod ont relevé beaucoup de limites de Valve. Hammer Pro garde une seule table des deux (quand les outils et le moteur de Garry's Mod diffèrent, la plus petite valeur est gardée). Le compilateur vérifie les limites du [jeu cible](target-games.md) : celles de Garry's Mod pour Garry's Mod, celles du Source SDK 2013 pour les autres jeux. 0 veut dire sans limite.

| Limite | Source SDK 2013 | Garry's Mod |
|---|---|---|
| Modèles de brushes (entités de brushes, monde compris) | 1024 | 4096 |
| Brushes | 8192 | 65535 |
| Faces de brushes | 65536 | 655350 |
| Plans | 65536 | 65536 |
| Sommets | 65536 | 65536 |
| Nœuds BSP | 65536 | 65536 |
| Infos de texture | 12288 | 32768 |
| Matériaux (texdata) | 2048 | 8192 |
| Displacements | 2048 | 16384 |
| Faces, faces d'origine | 65536 | 65536 |
| Feuilles, faces des feuilles, brushes des feuilles | 65536 | 65536 |
| Zones | 256 | 1024 |
| Areaportals | 1024 | 1024 |
| Arêtes de surface | 512000 | 512000 |
| Arêtes | 256000 | 256000 |
| Échantillons de cubemap | 1024 | 1024 |
| Overlays | 512 | 8192 |
| Faces d'un overlay | 64 | 64 |
| Faces d'un overlay d'eau | 256 | 256 |
| Lumières du monde (par passe) | 8192 | 8192 |
| Entités | 8192 | 65535 |
| Texte des entités | 384 Ko | 4 Mo |
| Données de visibilité | 16 Mo | 16 Mo |
| Données de lightmap (par passe) | 2 Go | 2 Go |
| Props statiques | 65535 | 16384 (le moteur garde les 16384 premiers) |
| Lumières commutables (nommées) | 32 | 96 |
| Primitives, indices des primitives | 32768, 65536 | 32768, 65536 |
| Étendue de lightmap d'une face (luxels) | 33 | 126 |
| Données de collision, collision des displacements | sans limite | sans limite |

Les autres lignes (normales des sommets et leurs indices, données d'eau des feuilles, sommets des areaportals, overlays d'eau, sommets et triangles des displacements) suivent la même table et apparaissent dans les statistiques.

### Profils de limites

Les vérifications en direct et la fenêtre **Map > Informations sur la map** mesurent la map selon un profil, choisi dans sa liste **Limites de :** (enregistré sous `limit_profile` dans la table `[health]` de `settings.toml`) :

| Profil | Limites |
|---|---|
| **Automatique** (par défaut) | Garry's Mod 64 bits pour Garry's Mod, Source SDK 2013 pour les autres jeux |
| **Source SDK 2013** | les valeurs de `bspfile.h` de Valve |
| **Garry's Mod 32 bits** | les mêmes que 64 bits aujourd'hui |
| **Garry's Mod 64 bits** | la colonne Garry's Mod ci-dessus |
| **VBSP++** | les limites de Garry's Mod avec 65536 entités |

La barre d'état et [Vérifier les problèmes](../features/check-problems.md) préviennent quand une limite est proche (« Brushes : 60000 sur 65535 (91 %) ») et quand elle est dépassée (« ..., la compilation échouera »). Après chaque compilation, le compilateur vérifie le BSP qu'il a écrit : chaque index dans son lump, chaque compte sous les limites. Tout problème est une erreur de compilation : une map qui compile se charge.

## Lire le journal de compilation

L'onglet **Journal** de la [fenêtre Compiler et lancer](compile-window.md) affiche chaque ligne de chaque étape au fur et à mesure, avec une barre de progression par étape. Le même texte est ajouté à `<map>.log`.

- **Couleurs et filtres** : information, avertissements et erreurs ; **Afficher les lignes d'information**, **Afficher les avertissements**, **Afficher les erreurs**, **Rechercher dans le journal**, **Défilement auto**, **Copier tout le journal**. Cliquez une étape pour ne voir que ses lignes ; **Afficher le journal de toutes les étapes** annule ce filtre.
- **Problèmes cliquables** : une ligne qui désigne un brush, une face, une entité ou une position devient un lien : cliquez pour sélectionner l'objet et centrer les vues (« Cliquez pour aller au brush 12 »). Un journal qui concerne une autre map vous demande de l'ouvrir d'abord.
- **Liste des problèmes** : « Problèmes : N erreurs, M avertissements », chacun avec **Aller à**.
- **Explications** : les messages connus reçoivent une explication claire et un bouton : **Aide** (la page du Valve Developer Community), **Aller à**, **Chemin de fuite** (charge le pointfile) ou **Vérifier** (ouvre Vérifier les problèmes).
- **Messages de la chaîne de compilation** : les lignes écrites par Hammer Pro lui-même, par exemple « Compilation arrêtée après la géométrie : la map fuit », « N fichiers intégrés au BSP », « Mêmes entrées qu'une compilation précédente : résultat réutilisé », « ... n'existe pas encore : une compilation complète remplace la compilation partielle », « Impossible de remplacer ... : si la map est chargée dans le jeu, changez de map ou déconnectez-vous d'abord ».
- Le plantage d'une étape est nommé (« Le programme a manqué de mémoire », « dépassement de pile »...), avec son code de sortie.

Le compilateur écrit volontairement les messages de VBSP, donc les mêmes règles fonctionnent aussi sur les journaux des outils de Valve.

### Messages courants

Les messages du compilateur sont en anglais, comme ceux de Valve.

| Le message contient | Signification | Que faire |
|---|---|---|
| `**** leaked ****` | Une entité peut atteindre le vide par un trou dans les brushes du monde. | Suivez le chemin de fuite et bouchez le trou avec une brush du monde (`func_detail` et les entités de brushes ne bouchent pas). Voir [Fuites et pointfiles](../features/leaks-pointfiles.md). |
| `leaked!` | Cette entité est le début du chemin de fuite. | Comme ci-dessus. |
| `areaportal leak` | Un areaportal ne ferme pas son ouverture. | Faites remplir l'ouverture à l'areaportal ; séparez les zones par des brushes du monde. |
| `microbrush` | Une brush est trop petite ou trop fine pour être construite. | Supprimez-la ou donnez-lui au moins 1 unité d'épaisseur sur la grille. |
| `texture axis perpendicular` | Une texture est projetée dans le plan de sa propre face. | Réinitialisez l'alignement de la face (Vérifier les problèmes le corrige). |
| `mixed face contents` | Une brush mélange des contenus (eau et solide, nodraw et clip...). | Un seul type de matériau sur toutes les faces. |
| `displacement found on a(n)` | Un displacement dans une entité de brushes. | Remettez la brush à displacement dans le monde. |
| `too many switched lights` | Plus de 96 lumières nommées (32 hors Garry's Mod). | Même nom pour les lumières qui s'allument ensemble ; retirez le nom de celles qui ne changent jamais. |
| `must be used on a dynamic entity` | Le modèle d'un `prop_static` a un `prop_data`. | Utilisez `prop_physics` ou `prop_dynamic`, ou `-AllowDynamicPropsAsStatic`. |
| `must be compiled with $staticprop` | Le modèle n'est pas un modèle de prop statique. | Utilisez `prop_dynamic`. |
| `overlay touching too many faces` | Un overlay couvre 64 faces ou plus. | Réduisez-le. |
| `too many overlays`, `too many brush models` | Limites dépassées. | Supprimez ou regroupez des overlays ; transformez les entités de brushes immobiles en brushes du monde ou en `func_detail`. |
| `could not open instance file` | Le fichier d'un `func_instance` manque. | Vérifiez son chemin (relatif à la map ou au dossier maps). |
| `bad surface extents` | Une face est trop grande pour sa lightmap et ne peut pas être découpée (`-nosubdiv`). | Augmentez son échelle de lightmap ou coupez la brush. |
| `unknown option` | Le compilateur ne connaît pas une option. | Vérifiez l'orthographe dans les arguments supplémentaires. |
| `material not found` | Un matériau n'est ni dans le jeu ni dans vos dossiers. | Remplacez-le ou ajoutez son dossier dans les paramètres. |
| `error loading studio model`, `outside the map` | Le modèle d'un prop manque ou est abîmé, ou un prop statique est dans le vide : il est retiré. | Corrigez le chemin du modèle ; placez le prop dans le monde. |
| `in solid volume` | Une entité est dans une brush solide. | Placez-la dans un espace libre. |
| `findportalside`, `brush with win 0` | Des brushes qui se chevauchent ou invalides. | Nettoyez les brushes à cet endroit. |
| `max_map_brushes`, `max_map_planes`, `max_map_portals` | Une limite stricte. | Fusionnez des brushes, gardez la géométrie sur la grille, passez les petites brushes en `func_detail`. |
| `ambient occlusion: radius ..., strength ..., N rays` | Information : les réglages d'occlusion ambiante utilisés. | |
| `Warning: ... is not supported by the Hammer Pro compiler (ignored)` | Une option de Valve que ce compilateur ne fait pas. | Rien ; retirez-la pour ne plus avoir l'avertissement. |
| `Warning: Couldn't open texlight file ...` | Un fichier de lumières est introuvable. | Vérifiez `-lights` ou `_lightsrad`. |

## L'onglet Statistiques

L'onglet **Statistiques** (aussi le bouton **Statistiques : N conseils** du journal) lit les statistiques que le compilateur écrit à la fin de chaque étape. Chaque élément qui a une place dans la map (une brush, une face, un cluster, une zone, un prop) a un bouton **Aller à**.

- **Conseils d'optimisation** : limites à 75 % ou plus (au-delà de 90 % ce sont des avertissements, au-delà de 100 % le jeu peut refuser la map), une map compilée sans visibilité (critique quand elle a de l'eau : le jeu dessine alors l'eau en noir, sans réfraction, donc compilez au moins avec la visibilité rapide), des clusters qui voient 50 % de la map ou plus, une visibilité moyenne faible, une grande map sans areaportals, de petites brushes structurelles qui pourraient être des `func_detail` (la plus petite est nommée), des props statiques sans distance de disparition, des displacements de puissance 4, des lightmaps qui prennent 90 % ou plus des 2 Go qu'une face peut adresser, des échelles de lightmap très fines, des styles commutables presque tous utilisés.
- **Limites du moteur** : le tableau de vbspinfo de Valve (utilisé, maximum, octets, remplissage), les plus pleines d'abord.
- **Géométrie** : taille de la map, brushes (structurelles, de détail, d'eau, outils, dans des entités de brushes), entités par classe, faces par matériau, displacements par puissance, zones, les petites brushes structurelles et les props sans distance de disparition.
- **Visibilité** : mode (rapide, complète, non calculée, ou sautée parce que la map fuit), clusters, portails, feuilles, clusters visibles en moyenne et au plus, le rayon de visibilité, les clusters qui voient le plus.
- **Éclairage** : octets de lightmap de chaque passe sur les 2 Go qu'une face peut adresser, luxels, lumières par type, styles commutables, textures lumineuses, props statiques et de détail éclairés, échantillons d'ambiance, rebonds, les faces et matériaux avec le plus de luxels avec leur échelle de lightmap.
- **Durée des phases** et **Durée des étapes**.

Sans compilation Hammer Pro dans le journal, l'onglet le signale. L'outil MCP `get_compile_stats` donne les mêmes données.

## FAQ

**La barre d'état dit qu'une limite fera échouer la compilation, mais elle passe.** Les vérifications en direct sont une estimation faite avant de compiler ; le compte du compilateur est exact. Vérifiez le profil dans **Map > Informations sur la map** : un profil aux limites plus basses que votre jeu est plus strict.

**Où est le journal d'une ancienne compilation ?** `<map>.log` dans le dossier `.hammerpro_build` à côté de votre map garde chaque lancement ; l'onglet Historique garde les durées.

**Y a-t-il une limite de 16 Mo de lightmaps ?** Non. `MAX_MAP_LIGHTING` (16 Mo) est encore dans `bspfile.h` de Valve, mais ni les outils du Source SDK 2013 ni le VRAD de Garry's Mod ne le vérifient (leur tableau de tailles affiche les lightdata sans maximum), et le moteur charge davantage : des maps de Garry's Mod compilées par Valve se chargent avec 60 à 80 Mo de lightmaps par passe. La vraie limite, pour chaque jeu cible, est le décalage de lightmap d'une face (`lightofs` dans `dface_t`), un nombre signé sur 32 bits : au-delà de 2 Go par passe les faces ne peuvent plus désigner leurs lightmaps et le jeu ne peut pas charger la map. L'éclairage de Hammer Pro s'arrête alors avec une erreur ; les conseils préviennent dès 90 %. Les grandes lightmaps coûtent quand même de la mémoire et du temps de chargement, gardez donc des échelles grossières là où les ombres sont douces.

## Pages liées
- [Vérifier les problèmes](../features/check-problems.md)
- [Fuites et pointfiles](../features/leaks-pointfiles.md)
- [Erreurs courantes](../troubleshooting/common-errors.md)
- [Fenêtre Compiler et lancer](compile-window.md)
- [Performances de compilation](performance.md)

---
[Retour à l'index de la documentation](../README.md)
