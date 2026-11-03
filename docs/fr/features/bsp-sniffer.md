[Documentation](../README.md) > Fonctionnalités > BSP Sniffer | [English](../../en/features/bsp-sniffer.md)

# BSP Sniffer

Le BSP Sniffer rejoint un serveur Garry's Mod et récupère la map exactement comme le jeu l'a chargée, puis écrit un VMF que vous pouvez ouvrir dans Hammer Pro, le BSP récupéré et le contenu de la map (modèles, matériaux, sons, pakfile). La map est reconstruite depuis la mémoire du jeu en cours : elle n'a jamais besoin du fichier de map. Une map protégée avec [Protect your BSP](bsp-protection.md) est récupérée aussi : le moteur doit charger la map pour l'afficher, donc le sniffer lit ce que le moteur a chargé et restaure ce que le protecteur a brouillé. Vous tapez une `ip:port`, vous cliquez sur **Sniffer**, et Hammer Pro fait le reste : il lance Garry's Mod via Steam, rejoint le serveur, attend le chargement de la map, lit la mémoire du jeu, reconstruit la map, la décompile et ferme le jeu.

**Il fonctionne uniquement avec Garry's Mod.** Il a été testé avec la branche x86-64 de Garry's Mod (build 2026.09.17, build Steam 26229091) sous Windows. Les autres jeux Source et les autres builds ne sont pas pris en charge.

## Usage autorisé uniquement

N'utilisez le BSP Sniffer que sur des serveurs et des maps que vous avez le droit de tester. Vous êtes responsable de ce que vous en faites. Les auteurs de Hammer Pro ne sont pas responsables de son utilisation, ni de ses conséquences pour vos comptes, serveurs ou maps.

À la première ouverture du panneau, une boîte de dialogue d'avertissement vous demande de confirmer que vous avez compris. L'avertissement reste visible dans le panneau, et **Sniffer** reste désactivé tant que vous n'avez pas coché **Je suis autorisé à tester ce serveur et cette map**.

Sur un serveur protégé par VAC, le jeu tourne avec VAC pendant que Hammer Pro lit sa mémoire. Hammer Pro lit seulement (il n'écrit, n'injecte et ne détourne rien), mais un serveur ou une communauté qui interdit ce genre d'outil peut vous sanctionner. Hammer Pro interroge d'abord le serveur et n'ajoute `-insecure` que si le serveur n'est pas protégé par VAC (réglage **-insecure**, automatique par défaut) : un serveur VAC refuse un jeu insecure.

## L'ouvrir

**Fichier > BSP Sniffer...** (action `bsp_sniffer`) ouvre le panneau. **Fichier > Décompilateur de BSP...** est l'autre outil : il décompile un fichier `.bsp` que vous avez déjà.

## Le panneau

| Contrôle | Ce qu'il fait |
| --- | --- |
| **Serveur (IP:port)** | Le serveur à rejoindre. Sans port, 27015 est utilisé. `map:<nom>` lance à la place la map `<nom>` de cet ordinateur dans un Garry's Mod local (voir [Jeu local et jeu déjà lancé](#jeu-local-et-jeu-déjà-lancé)). |
| **Je suis autorisé à tester ce serveur et cette map** | Obligatoire pour activer **Sniffer**. |
| **Sniffer** | Lance l'opération. |
| **Annuler** | Arrête l'opération. Le jeu lancé par Hammer Pro est fermé (si **Fermer Garry's Mod à la fin** est activé). |
| Statut | La phase, une barre de progression, le temps écoulé, le processus du jeu et la map. |
| **Résultat** | Les comptages de la map chargée et du VMF, et les boutons **Ouvrir le VMF**, **Afficher le dossier** et **Copier le rapport**. |
| **Réglages** | Toutes les options ci-dessous. Elles sont enregistrées. |
| **Journal** | Ce qui s'est passé, avec les temps, et **Copier le journal**. |

## Jeu local et jeu déjà lancé

Un second Garry's Mod ne peut pas rejoindre votre propre serveur sur le même ordinateur (Steam n'autorise qu'un jeu), donc le sniffer a deux équivalents qui donnent la mémoire client d'un vrai join :

- **`map:<nom>` comme serveur** : Hammer Pro lance Garry's Mod avec `-insecure +sv_lan 1 +maxplayers 2 +map <nom>`, attend que la console annonce `Map: <nom>`, lit la mémoire et ferme le jeu. Le fichier de map doit être dans `garrysmod/maps` (ou là où le jeu le trouve), exactement où atterrit une map téléchargée. L'interrogation du serveur est sautée.
- **`map:` seul avec *Utiliser Garry's Mod s'il tourne déjà*** : lit le jeu déjà lancé avec une map chargée (solo, serveur d'écoute ou serveur rejoint par vous). Le nom de la map vient du journal de la console. Hammer Pro ne ferme jamais un jeu qu'il n'a pas lancé. Désactivez **Se déconnecter du serveur à la fin** dans ce mode : se déconnecter décharge la map et laisse des données périmées en mémoire.

## Réglages

Il n'y a pas de mode à choisir : le sniffer lit toujours la mémoire du jeu et la fusionne avec le fichier de map que le client possède (`map_pack.bsp` du cache du jeu, `maps`, `download`, addons, archives Workshop) quand ce fichier correspond à la mémoire (mêmes entités, mêmes lumps vérifiés). Chaque champ pris dans le fichier est revérifié contre la mémoire d'abord, et un fichier modifié ou différent est ignoré. Le fichier est lu par son répertoire de lumps, donc un fichier protégé aux lumps mélangés, avec des trous, des blocs leurres, un en-tête brouillé ou un pakfile n'importe où est lu comme un autre.

| Réglage | Ce qu'il fait | Défaut |
| --- | --- | --- |
| **Dossier de sortie** (`output_dir`) | Où vont les fichiers. Vide : `%LOCALAPPDATA%\HammerPro\sniffer` (le dossier listé par le panneau Stockage). | vide |
| **Nom du fichier** (`file_name_template`) | Nom du VMF sans extension. Jetons : `{map}` `{ip}` `{port}` `{date}` `{time}` `{datetime}`. Les caractères dangereux sont remplacés ; un fichier existant n'est jamais écrasé (un numéro est ajouté). | `{map}_sniffed` |
| **Dossier de Garry's Mod** (`gmod_dir`) | Le dossier qui contient `garrysmod`. Vide : détecté via Steam. La branche 64 bits (`bin\win64\gmod.exe`) et la branche 32 bits sont gérées. | automatique |
| **Options de lancement** (`launch_options`) | Ajoutées au lancement Steam. L'adresse du serveur (`+connect`), `-condebug` et `-insecure` sont gérés pour vous : ce texte ne peut pas rediriger la connexion. | `-windowed -w 800 -h 450 -novid -nojoy -nosplash -console +mat_picmip 4 +fps_max 30 +volume 0` |
| **-insecure (sans VAC)** (`insecure`) | Automatique : seulement pour les serveurs non protégés par VAC. Toujours. Jamais. | automatique |
| **Utiliser Garry's Mod s'il tourne déjà** (`attach_running`) | Lit le jeu en cours au lieu d'en lancer un. Hammer Pro ne ferme jamais un jeu qu'il n'a pas lancé. Le jeu en cours doit déjà être connecté au serveur. | désactivé |
| **Attente du processus du jeu** (`process_timeout_secs`) | Combien de temps attendre l'apparition du jeu. | 180 s |
| **Attente de la map** (`load_timeout_secs`) | Combien de temps attendre la connexion, les téléchargements et la map. | 900 s |
| **Délai de stabilisation** (`settle_secs`) | La map doit rester identique en mémoire pendant ce temps avant d'être capturée. | 6 s |
| **Vérifier toutes les** (`poll_secs`) | L'intervalle de lecture. | 3 s |
| **Enregistrer les modèles** (`save_models`) | Copie les fichiers `.mdl`, `.vvd`, `.vtx` et `.phy` utilisés par la map. | activé |
| **Enregistrer les matériaux** (`save_materials`) | Copie les fichiers `.vmt` et `.vtf` utilisés par la map. | activé |
| **Enregistrer les sons** (`save_sounds`) | Copie les sons utilisés par la map. | activé |
| **Enregistrer le reste du contenu embarqué** (`save_other`) | Copie le reste du pakfile et du contenu (scripts, particules...). | activé |
| **Seulement le contenu absent du jeu de base** (`only_non_stock`) | Ignore les fichiers qui existent déjà dans le jeu de base et ses VPK. | activé |
| **Garder le BSP récupéré** (`keep_bsp`) | Écrit le BSP à côté du VMF. | activé |
| **Décompilation** (`decompile.*`) | Brushes, déplacements, entités, entités brush, props statiques, overlays, cubemaps, areaportals, occluders, grouper les `func_detail` : les mêmes interrupteurs que le décompilateur. | tout activé |
| **Fermer Garry's Mod à la fin** (`kill_game`) | Ferme le jeu lancé par Hammer Pro, aussi après un échec ou une annulation. | activé |
| **Se déconnecter du serveur à la fin** (`auto_disconnect`) | Quand le jeu reste ouvert, lui demande de se déconnecter. | activé |
| **Ouvrir le VMF dans un nouvel onglet à la fin** (`open_when_done`) | Ouvre le résultat. | activé |

## Ce que vous obtenez

Dans le dossier de sortie, pour une map nommée `rp_example` :

| Fichier | Contenu |
| --- | --- |
| `rp_example_sniffed.vmf` | La map : brushes (tous, y compris les textures nodraw, tool, clip, skybox et trigger), déplacements, entités avec tous leurs keyvalues et sorties, props statiques, overlays, cubemaps, areaportals. |
| `rp_example_sniffed.bsp` | Le BSP récupéré (si **Garder le BSP récupéré** est activé). |
| `rp_example_sniffed_content\` | `materials`, `models`, `sound` et les autres fichiers de la map (pakfile et contenu du jeu). |
| `rp_example_sniffed_sniff.txt` | Le rapport : ce qui a été confirmé en mémoire, les comptages, les temps. |

## Comment ça marche

1. **Vérifications.** Le serveur est interrogé avec le protocole de requête Source (map, joueurs, VAC). Garry's Mod est détecté (branches 64 bits et 32 bits).
2. **Lancement.** Steam lance le jeu avec vos options, `-condebug` et `+connect ip:port`. Hammer Pro trouve le processus du jeu par le chemin de son exécutable, donc les processus auxiliaires sont ignorés.
3. **Attente.** Le journal de la console indique quand la connexion est faite et quelle map est chargée. Le sniffer analyse alors la mémoire du jeu pour trouver le lump d'entités de la map (l'entité `worldspawn` et les autres). La map du menu, toujours chargée en premier, est ignorée, et la map doit rester identique pendant le délai de stabilisation. Le lump d'entités est en mémoire bien avant que le moteur ait fini de construire la map : le sniffer continue donc de lire jusqu'à ce que le moteur ait construit chaque déplacement (les faces portent un drapeau de déplacement, ce qui donne le nombre attendu), que chaque overlay ait ses faces, que les props statiques soient tous résolus et que deux analyses consécutives donnent les mêmes nombres. Cette attente est bornée par **Attente de la map** ; un serveur lent peut demander plusieurs minutes.
4. **Capture.** Tout ce qui suit est décodé depuis la mémoire du jeu, sans aucun fichier :
   - le lump d'entités, avec chaque keyvalue et chaque sortie ;
   - les données de collision (brushes, faces de brush, boîtes alignées sur les axes, plans, nodes, leafs, modèles, et le matériau de chaque face) ;
   - les données de rendu : les enregistrements de faces (plan, côté, indices de sommets, texture info, objet de déplacement), le tableau des sommets et celui des indices de sommets, et les enregistrements texture info (88 octets : les deux axes de texture, les axes de lightmap, les drapeaux et le matériau). Le texture info d'un matériau est donné par l'ordre des objets matériaux, qui suit l'ordre des noms de surfaces de collision ;
   - les déplacements : le moteur garde, pour chaque déplacement, sa position de départ, sa puissance et la position finale de chaque sommet. Le sniffer reconstruit la grille plate de la face et transforme le décalage de chaque sommet en la direction et la distance stockées dans un BSP ; les tags de triangles sont recalculés depuis les pentes ;
   - les props statiques : le moteur garde un objet par prop (modèle, origine, angles, skin, solidité, plage de leafs) ; la liste est trouvée comme une suite d'objets de même taille dont les noms de modèles se résolvent en fichiers `.mdl` ;
   - les overlays : le moteur garde la table avec le texture info, les faces, les étendues UV, l'origine et la base de chaque overlay ;
   - les cubemaps : les noms des textures de cubemap, `maps/<map>/c<x>_<y>_<z>`, portent l'origine de chaque `env_cubemap` ;
   - les area portals et leurs sommets : trouvés comme une chaîne d'enregistrements dont les sommets sont sur les plans des portails ;
   - quand un fichier de map est autorisé et trouvé, un recoupement : les lumps que le moteur garde tels quels (plans, sommets, visibilité, leaf brushes, areaportals, éclairage, éclairage ambiant, normales de sommets, noms de textures...) sont comparés octet pour octet, les données de collision brush par brush, et les nombres de tout sont comparés dans le rapport.
5. **Complément (hybride).** Un fichier de map qui a correspondu à la mémoire (mêmes entités, lumps confirmés octet pour octet) complète, enregistrement par enregistrement, ce que le moteur ne garde pas. Chaque enregistrement est d'abord validé contre la mémoire et ignoré s'il la contredit :
   - les alphas, tags de triangles et sommets autorisés des déplacements (un déplacement n'est pris dans le fichier que si son départ, sa puissance et le décalage de chaque sommet concordent avec la mémoire à 0,05 unité près) ;
   - les distances de fondu, drapeaux et solidité des props statiques (appariés par modèle, origine et angles) ;
   - les tailles de cubemaps (appariées par origine) et l'ordre de rendu des overlays (apparié par id, texture et origine) ;
   - les axes de texture des faces de brush qu'aucune face rendue ne couvre (appariés par brush et plan, et seulement si les axes de texture sont ceux de la mémoire) ;
   - le pakfile de la map.
   Le rapport liste, pour chaque catégorie, sa source : mémoire, fichier de map, contenu du jeu ou absent.
6. **Reconstruction.** Un BSP valide est assemblé depuis ce qui a été décodé et complété : la table des brushes, faces de brush, plans, nodes, leafs, modèles, faces, arêtes, sommets, texture info et texture data, infos de déplacements, sommets et tags de triangles, overlays, cubemaps, area portals, les props statiques comme game lump et le pakfile. Chaque face de brush prend les axes de texture de la face rendue qui est sur son plan et dans son brush (le plus grand recouvrement gagne) ; une face qu'aucune face ne couvre, parce qu'elle est cachée dans d'autres brushes, reçoit les axes du fichier s'il y en a un, sinon des axes alignés sur le monde. Quand la mémoire du jeu n'a pas les données de rendu et qu'un fichier existe, le fichier de map est utilisé à la place.
7. **Décompilation.** Le BSP passe dans le même décompilateur que **Fichier > Décompilateur de BSP...**, avec vos interrupteurs.
8. **Contenu.** Le contenu du jeu (addons Workshop téléchargés, addons, téléchargements du serveur) est cherché pour les modèles, matériaux, sons et particules que le VMF utilise. Le pakfile de la map n'existe que dans le fichier de map : il est extrait quand un fichier de map est autorisé, trouvé et correspondant. Sans lui, le `.mdl` de chaque modèle que le jeu a chargé et qui n'est pas sur cet ordinateur est récupéré depuis la mémoire du jeu ; les autres fichiers d'un pakfile (matériaux patchés et textures de cubemap faits par le compilateur, textures personnalisées, fichiers `.vvd`, `.vtx` et `.phy`) ne sont pas en mémoire sous une forme utilisable et sont listés comme manquants dans le rapport. Les fichiers qui existent dans le jeu de base sont ignorés si **Seulement le contenu absent du jeu de base** est activé.
9. **Fin.** Le jeu est fermé par son propre identifiant de processus (et ses processus auxiliaires). Rien d'autre n'est jamais fermé.

La mémoire est lue avec `ReadProcessMemory` après activation de `SeDebugPrivilege` quand c'est permis ; le jeu 64 bits demande un Hammer Pro 64 bits. Le jeu est seulement lu, jamais modifié.

## Maps protégées

[Protect your BSP](bsp-protection.md) livre une copie de la map qui retire les entités logiques et toutes les entrées et sorties, vide le lump des faces originales, brouille les noms de matériaux (`hp/xxxxxxxx`, avec un matériau patch qui inclut le vrai), marque l'entité monde d'un watermark et peut ajouter un verrou serveur. Le jeu charge et affiche quand même cette copie, donc le sniffer lit les mêmes données que pour n'importe quelle map puis défait ce qu'il peut :

| Ce que fait le protecteur | Ce que le sniffer récupère |
| --- | --- |
| Noms de matériaux brouillés (`hp/` puis huit chiffres hexadécimaux) | Avec un fichier de map qui correspond : chaque matériau d'origine du jeu, exactement, depuis l'`include` du matériau patch dans le pakfile de la map protégée (les fichiers patch ne sont alors pas gardés dans le contenu). Sans fichier de map : les objets matériaux chargés gardent la réflectivité de leur texture ; le sniffer indexe les en-têtes des textures du contenu du jeu (en cache dans le dossier du sniffer, quelques minutes la première fois) et nomme le matériau d'après la texture de même réflectivité. Une réflectivité partagée par plus de quatre textures n'est pas utilisée. Une devinette peut être fausse quand deux matériaux partagent une texture, et le rapport compte les noms devinés à part. |
| Matériaux et textures personnalisés renommés (fichiers `hp/` du pakfile) | Ils gardent leur nom brouillé, et les fichiers sont sauvés dans le contenu sous ce nom, donc le VMF s'affiche toujours. Le nom d'origine a été remplacé par un hash avec un sel aléatoire absent de la map : il est irrécupérable. |
| Logique, `info_node` et autres entités serveur retirées, entrées et sorties, `hammerid`, `parentname` et commentaires retirés | Irrécupérable : le client ne les reçoit jamais. Toutes les autres entités sont complètes. |
| Lump des faces originales vidé | Le moteur ne l'utilise pas. Un côté de brosse qu'aucune face affichée ne couvre garde les axes de texture de l'enregistrement de côté du fichier de map (mode hybride) ; sans le fichier de map, ces axes sont alignés sur le monde. Un très petit nombre de côtés dont la texture venait des faces originales gardent la texture de l'enregistrement de côté. |
| Clés de watermark et de verrou serveur sur l'entité monde | Lues, montrées dans le rapport et retirées du VMF. |

Le rapport a une ligne `map protégée` et une ligne `material names of a protected map` dans le tableau des sources avec les nombres de chaque cas.


## Tout ce que le client possède

Le BSP reconstruit part de la mémoire du jeu. Le fichier de map du client sert ensuite pour chaque lump que la mémoire ne contient pas, mais seulement quand les structures que ce lump désigne sont prouvées identiques dans les deux : la liste des leafs (contenus, cluster, zone et plage de brushes) pour les lumps indexés par leaf, la liste des faces (plage d'arêtes et texture info) pour ceux indexés par face, et la liste des modèles (origine et head node) pour la physique. Cela ramène octet pour octet environ 30 Mo sur la petite map de test et environ 140 Mo sur une map Workshop de 155 Mo : la visibilité (PVS), les lightmaps LDR et HDR avec les champs de lightmap de chaque face, l'éclairage ambiant des leafs (LDR et HDR, avec leurs index), les échantillons de lightmap des displacements, les maillages de physique, les identifiants de faces, les zones, la distance à l'eau de chaque leaf, et le game lump des detail props. Sur les deux maps de test, rien n'est refusé. Le rapport liste ce qui a été pris par nom avec le nombre d'octets, et nomme ce qu'il a refusé.

Un lump est refusé quand ce qu'il indexe diffère, ce qui est la réponse honnête : ses numéros pointeraient vers les mauvaises structures. Le sniffer ne prend jamais les lumps qu'il reconstruit lui même (faces, arêtes, surfedges, sommets, plans, texture info, brushes, nodes, leafs, displacements, overlays, cubemaps, area portals, props statiques, entités, pakfile), et il laisse de côté ceux dont la numérotation suit l'un d'eux : la liste des faces par leaf, les occluders, les normales de sommets et leurs index, les primitives (eau et autres surfaces à jonction en T), les lumières du monde, l'éclairage des detail props (il suit l'ordre des props statiques, que le sniffer reconstruit), les drapeaux de map et les fondus d'overlays.

La liste des leafs du jeu commence parfois un ou deux enregistrements avant le vrai premier leaf, parce que les octets qui la précèdent peuvent ressembler à un leaf. Le sniffer compare sa liste de leafs à celle du fichier champ par champ (contenus, cluster, zone et plage de brushes) et retire ces enregistrements, pour que ses numéros de leafs soient ceux qu'utilisent les nodes de la map. Sans cela, une map comme celle de 155 Mo avait tous ses leafs décalés de un et aucun lump indexé par leaf ne pouvait être pris.

## Limites

- Garry's Mod uniquement, Windows uniquement.
- Hammer Pro lit les structures mémoire de la branche x86-64 de Garry's Mod, build 2026.09.17. La branche 32 bits n'a pas été installée pour les tests et utilise d'autres tailles de structures : les analyses du lump d'entités et des données de collision indiquent ce qu'elles trouvent, les données de rendu sont signalées comme introuvables, et un fichier de map (s'il est autorisé) sert alors pour la géométrie. Sur un autre build, le rapport indique quelle structure n'a pas été reconnue.
- Ce que le moteur ne garde pas est pris dans le fichier de map correspondant du client quand il est cohérent, et approché quand aucun fichier de map utilisable n'est trouvé :
  - les alphas de sommets des déplacements (la peinture de mélange de textures) sont exportés à 0 ;
  - les distances de fondu des props statiques sont omises, et les quelques drapeaux non gardés (sans ombres, sans auto-ombrage) ne sont pas posés ; un prop que le moteur a rendu non solide est exporté avec cette solidité ;
  - la taille d'un cubemap est exportée à la valeur habituelle, et deux cubemaps à la même origine deviennent un seul ;
  - l'ordre de rendu des overlays est exporté à 0 ;
  - les sommets autorisés des déplacements sont exportés par défaut, et les tags sont ceux que donnent les pentes ;
  - les axes de texture des faces de brush qu'aucune face ne couvre (cachées dans d'autres brushes) sont alignés sur le monde ;
  - ce qui a été cherché dans un instantané complet du processus du jeu de la map de test, sans succès :
    - alphas des déplacements : les alphas d'un déplacement en flottants, octets, entiers 16 et 32 bits, inversés ou divisés par 255, à tous les pas jusqu'à 128 octets, ne sont nulle part ; les alphas sont des flottants libres (596 valeurs distinctes, certaines au-dessus de 255) et trois valeurs rares ne sont jamais contiguës ; trois niveaux de pointeurs depuis l'objet de déplacement ne contiennent aucun tableau égal. Le mélange est construit dans les vertex buffers du système de matériaux, que le processus du jeu ne garde pas lisibles ;
    - distances de fondu des props statiques : les paires (4000, 5000) et les autres n'existent nulle part en flottants, carrés, différences, entiers ou valeurs 16 bits, ni dans l'objet prop de 256 octets ni dans quatre niveaux de pointeurs depuis lui ;
    - drapeaux sans ombres et sans auto-ombrage : aucun octet de l'objet prop ne diffère de façon constante pour les props qui les ont ;
    - contenu du pakfile : textures 1 échantillon sur 1276 trouvé, texte des matériaux 0 sur 1266 (le moteur le transforme en key values et jette le texte), fichiers de sommets (.vvd) seulement la section des tangentes (10 échantillons sur 97), fichiers de strips (.vtx) 0 sur 26, fichiers de physique (.phy) ont leur corps mais pas leur zone d'en-tête, donc un fichier valide ne peut pas être reconstruit ; seul le .mdl est entier en mémoire (8 sur 8) et il est récupéré ;
    - taille des cubemaps et ordre de rendu des overlays : les objets texture des cubemaps se nomment par des handles de pool de chaînes sans taille lisible à côté, et l'ordre de rendu vaut 0 pour les 109 overlays de la map de test, donc le champ n'a pas pu être localisé.
- Ce que le serveur n'envoie jamais au client ne peut pas être récupéré : entités logiques retirées par [Protect your BSP](bsp-protection.md), scripts serveur, tout ce que le serveur garde pour lui. Le maillage de navigation est un fichier `.nav` séparé que le serveur n'envoie pas non plus.
- Les occluders n'ont pas pu être mesurés : aucune des maps utilisées pour les tests n'en a. Le lump d'occlusion du fichier est refusé parce qu'il désigne les listes de sommets et de faces que le sniffer reconstruit, donc un `func_occluder` de la map n'est pas dans le VMF.
- Maps protégées : les matériaux personnalisés renommés par le protecteur gardent leur nom brouillé (le sel est aléatoire et absent de la map) ; sans fichier de map, les noms de matériaux d'une map protégée sont devinés d'après la réflectivité des textures et la devinette peut être fausse ou absente ; les entités retirées par le protecteur sont perdues.
- Les instances, groupes, visgroups et brushes réservés au compilateur sont perdus, comme pour toute décompilation.
- Les modèles et matériaux sont copiés depuis le contenu de cet ordinateur (téléchargements Workshop, addons, téléchargements du serveur), et les modèles que le jeu a chargés sont récupérés depuis sa mémoire ; un fichier qui n'est nulle part est listé comme manquant dans le rapport.
- Le moteur peut déplacer ses structures pendant leur lecture (il finit de charger, ou la map est rechargée). Le sniffer le remarque, parce que ce qu'il trouve retombe à zéro, garde la meilleure lecture complète déjà obtenue, et cherche la map à nouveau depuis le début, jusqu'à trois fois. Une lecture moins bonne que la précédente ne la remplace jamais, donc une exécution donne soit la map complète, soit ce qui était incomplet dans le rapport.
- Un serveur lourd peut demander des minutes avant que le moteur ait construit les déplacements et les overlays ; le jeu ne répond à la déconnexion automatique que lorsqu'il est au repos, donc la confirmation peut prendre une minute.

## Précision mesurée

La map de test est `gm_construct` (2203 brushes, 110 faces déplacées, 182 props statiques, 11 overlays, 28 cubemaps, 607 fichiers de pakfile) avec un matériau et un modèle personnalisés ajoutés à son pakfile, protégée avec toutes les options de [Protect your BSP](bsp-protection.md) : logique et connexions retirées, faces originales vidées, noms de matériaux brouillés, watermark, verrou serveur, puis toutes les options de disposition (lumps mélangés, trous aléatoires, blocs leurres, en-tête brouillé, game lumps mélangés, pakfile à un endroit aléatoire, tables et monde réordonnés). Chaque variante a été copiée dans `garrysmod/maps`, lancée avec `map:<nom>`, sniffée, et le BSP et le VMF récupérés ont été comparés à la map non protégée. Le résultat est le même pour la map protégée et pour les dix variantes de disposition (toutes les options ensemble, toutes les options de disposition, et chaque option seule) :

| Quoi | Récupéré |
| --- | --- |
| Solides dans le VMF (nodraw, tool, clip, skybox et trigger compris), mêmes plans | 2313 sur 2313 |
| Brushes et modèles de brush (func_brush et autres), mêmes ensembles de brushes | 2203 sur 2203, 12 sur 12 |
| Côtés d'outil (nodraw, skybox, skip, hint, trigger), même matériau | 4708 sur 4708 |
| Côtés de brush avec le vrai nom de matériau (pas `hp/xxxxxxxx`) | 12714 sur 13798 ; les 1084 autres sont le matériau personnalisé renommé par le protecteur et 1 côté |
| Axes de texture de tous les côtés de brush | 13796 sur 13798 |
| Displacements : positions, normales, distances, alphas, tags de triangles, sommets autorisés | 91 sur 91 (110 faces déplacées) |
| Props statiques : modèle, origine, angles, skin, distances de fondu, drapeaux, solidité | 182 sur 182 |
| Overlays, chaque clé | 11 sur 11 |
| Cubemaps avec leur taille | 28 sur 28 |
| Entités gardées par le protecteur, chaque clé et valeur | 663 sur 663 |
| Pakfile | 607 sur 607 fichiers, plus les fichiers personnalisés renommés sous leurs noms `hp/` |

Ce qui reste perdu, et pourquoi : les 839 entités logiques et les connexions retirées par le protecteur (jamais envoyées au client) ; le vrai nom du matériau personnalisé (remplacé par un hash avec un sel aléatoire absent de la map) ; 2 côtés dont les axes de texture venaient du lump des faces originales vidé par le protecteur (l'enregistrement de côté a d'autres axes, et la décompilation du fichier protégé donne le même résultat).

Le même sniffer sur `union_blue_lock_v2` (map Workshop de 155 Mo, 8180 brushes, 211 faces déplacées, 4967 props statiques) protégée de la même façon : 8176 de ses 8180 brushes et les 812 modèles de brush avec les mêmes ensembles de brushes, 50624 des 50683 côtés de brush avec le vrai nom de matériau, 50549 sur 50683 avec les mêmes axes de texture, les 211 displacements, les 4967 props statiques, et 140 Mo de lumps pris dans le fichier de map. Les quatre brushes qui diffèrent sont des brushes hint et skip que le moteur garde comme boîte alignée sur les axes : les deux faces en biais qu'elles ont dans le fichier ne sont pas dans le jeu. Les 59 côtés dont le matériau diffère sont des côtés qu'aucune face ne couvre, où le nom de surface gardé par le moteur et le côté de brush du fichier ne sont pas d'accord ; prendre le nom du fichier coûte plus qu'il ne rapporte (50482 corrects au lieu de 50624), parce que les plages de côtés de brush de cette map sont abîmées, donc le nom du jeu est gardé.

## MCP

Chaque réglage et l'opération elle-même sont accessibles par les outils `sniffer_start`, `sniffer_cancel`, `sniffer_status`, `sniffer_log`, `sniffer_result`, `sniffer_get_settings` et `sniffer_set_settings` ; le panneau s'ouvre avec l'action `bsp_sniffer` (`run_action`). Voir la [référence des outils MCP](../../MCP.md) (en anglais).

## Astuces

- Lancez-le une première fois avec **Fermer Garry's Mod à la fin** désactivé pour tester vos options de lancement, puis réactivez-le.
- Gardez **Seulement le contenu absent du jeu de base** activé : le résultat reste léger.
- Le fichier de rapport liste ce qui a été décodé depuis la mémoire avec les adresses, le nombre de lectures de fichier de map et le recoupement optionnel : joignez-le à un rapport de bug.

## FAQ

**Ça marche sur un serveur qui n'est pas le mien ?** Seulement si vous avez le droit de le tester. Le panneau vous demande de le confirmer.

**Pourquoi lancer le jeu plutôt que capturer le réseau ?** Le jeu décode et charge la map lui-même ; lire ce qu'il a chargé marche quel que soit le chemin par lequel la map est arrivée chez le client.

**Peut-il récupérer la logique du serveur ?** Non : seulement ce que le client reçoit.

**Mon jeu ou mon compte est-il en danger ?** Hammer Pro ne fait que lire la mémoire du jeu. Sur les serveurs protégés par VAC, le jeu tourne avec VAC ; utilisez-le à vos risques et seulement là où les propriétaires l'autorisent.

## Pages liées

- [Outils BSP](bsp-tools.md) (le Décompilateur de BSP)
- [Protect your BSP](bsp-protection.md)
- [Assets de la map](map-assets.md)
