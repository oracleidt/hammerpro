[Documentation](../README.md) > Fonctionnalités > Terrain | [English](../../en/features/terrain.md) | [Русский](../../ru/features/terrain.md)

# Terrain : cartes de hauteur et génération procédurale

Cinq commandes construisent, peignent ou exportent des terrains entiers faits de displacements. Elles sont dans les menus **Fichier** et **Outils** et dans la palette de commandes (`Ctrl+P`) :

| Commande | Id d'action | Ce qu'elle fait |
| --- | --- | --- |
| **Fichier > Importer > Importer une carte de hauteur...** | `import_heightmap` | Construit un terrain depuis une image de hauteur (8, 16 ou 32 bits flottant, exports en tuiles compris) |
| **Outils > Displacements > Générer un terrain...** | `generate_terrain` | Construit un terrain depuis du bruit fractal, avec érosion et texturage automatique |
| **Fichier > Exporter > Exporter une carte de hauteur...** | `export_heightmap` | Écrit les displacements sélectionnés dans une carte de hauteur 16 bits ou 32 bits flottant (et une splat map) |
| **Fichier > Importer > Importer des masques de blend...** | `import_blend_masks` | Peint le blend des displacements (alpha WorldVertexTransition ou lignes du blend 4 textures) depuis des images de splat ou de masque |
| **Fichier > Exporter > Exporter les masques de blend...** | `export_blend_masks` | Écrit le blend des displacements en images 16 bits ou flottantes |

Pour un aller-retour complet avec World Machine ou Gaea, lisez le [guide World Machine et Gaea](../guides/world-machine-gaea.md).

Elles créent une grille de tuiles carrées. Chaque tuile est un brush dont la face du dessus est un displacement. Les tuiles sont cousues : les sommets d'une arête commune sont identiques, donc sans fissure. Tout ce qu'une commande crée forme **une seule étape d'annulation**, et les nouvelles tuiles sont sélectionnées ensuite, prêtes pour l'[outil displacement](displacement-tool.md) (sculpter, peindre l'alpha, éroder).

Les mêmes fonctions existent comme outils MCP `import_heightmap`, `export_heightmap`, `generate_terrain`, `import_blend_masks` et `export_blend_masks` (voir [Outils MCP](#outils-mcp)).

## Notions

- **Tuile.** Un displacement de puissance 2, 3 ou 4 (4 x 4, 8 x 8 ou 16 x 16 cases) sur un brush de **Épaisseur des brushes** unités. La taille du terrain est coupée en un nombre entier de tuiles égales, le plus proche de **Taille des tuiles**. L'**espacement des sommets** est la taille de la tuile divisée par le nombre de cases, et la fenêtre affiche en direct le nombre de tuiles et l'espacement (« 8 x 8 tuiles (64 displacements), espacement des sommets 64 unités »).
- **Hauteurs.** Une valeur de la carte va du noir (0) au blanc (1) et devient une hauteur entre **Hauteur minimale** et **Hauteur maximale**. **Échelle verticale** est la différence des deux. **Origine Z** est la hauteur monde où se trouve la hauteur 0. Les cartes flottantes peuvent aussi contenir de vraies hauteurs (voir **Valeurs des pixels**).
- **Précision.** Chaque image est lue dans sa pleine précision et gardée en double précision jusqu'à l'écriture des sommets : une image 16 bits donne 65536 niveaux, une image 32 bits flottante (R32F, EXR, TIFF flottant, PFM) garde chaque valeur exactement. Rien n'est réduit à 8 bits en route.
- **Orientation.** X grandit vers la droite de l'image et Y (le nord) grandit vers le haut de l'image. **Retourner X** et **Retourner Y** la reflètent.
- **Limites.** 4096 tuiles au plus, une taille de 16 à 32768 unités par côté, et le terrain entier dans les limites de la carte (plus ou moins 16384 unités). La fenêtre affiche la raison en rouge quand un réglage est hors limites, et les boutons restent désactivés.

## Importer une carte de hauteur

Ouvrez **Fichier > Importer > Importer une carte de hauteur...**. La fenêtre a ces sections.

### Fichier

| Option | Ce qu'elle fait | Défaut |
| --- | --- | --- |
| **Carte de hauteur** | L'image : PNG (gris 8 ou 16 bits ; les images en couleur sont lues en luminance, une image grise enregistrée en RVB garde ses valeurs exactes), TIFF (8, 16 ou 32 bits, entier ou flottant), EXR (demi ou pleine précision flottante, un seul canal `Y`, `R` ou autre, ou RVB), PFM (`Pf` gris ou `PF` couleur), HDR, BMP, TGA, JPEG, ou un fichier **RAW** sans en-tête (`.raw`, `.r8`, `.r16`, `.r32`, `.f32`, `.bin`, `.data`). **Parcourir...** ouvre un sélecteur de fichier. Un chemin relatif part du dossier de la carte | vide |
| **Largeur RAW**, **Hauteur RAW** | Pixels d'un fichier RAW. Les deux à 0 veulent dire une image carrée, déduite de la taille du fichier ; une seule à 0 est calculée depuis l'autre et la taille du fichier | 0, 0 |
| **RAW gros-boutiste** | Les valeurs stockent l'octet de poids fort en premier | Désactivé |
| **Format RAW** | **Auto** (`.r8` est en 8 bits, `.r16` en 16 bits, `.r32` et `.f32` en flottant 32 bits ; les autres extensions essaient 16 bits, puis flottant 32 bits, puis 8 bits, celui qui donne une image carrée), **8 bit**, **16 bit** ou **Flottant 32 bits** (R32 de World Machine) | Auto |
| **Export en tuiles** | Les tuiles d'un export en tuiles de World Machine ou Gaea au lieu d'un seul fichier : **Ajouter des tuiles...** choisit plusieurs fichiers (triés par nom), listés rangée par rangée depuis la tuile nord-ouest, et **Tuiles par rangée** (0 : une grille carrée). Les tuiles de 2^n+1 pixels (comme 513 ou 1025) partagent leurs pixels de bord avec leurs voisines, comme World Machine les écrit ; ces pixels partagés sont cousus (moyennés), le résultat n'a donc pas de couture | aucune |

Un côté d'image est limité à 16384 pixels. La fenêtre affiche la taille de l'image, le format (« 1025 x 1025, 32 bit float gray ») et les valeurs minimale et maximale du fichier et, si le fichier est illisible, la raison.

### Valeurs des pixels

Comment les valeurs du fichier deviennent des hauteurs :

| Mode | Ce qu'il fait |
| --- | --- |
| **Auto** (défaut) | Les images 8 et 16 bits, et les images flottantes dont les valeurs sont entre 0 et 1, utilisent **0 à 1** ; les autres images flottantes utilisent **Étirer**. C'est la lecture des versions précédentes |
| **0 à 1** | 0 est la **Hauteur minimale** et 1 la **Hauteur maximale**. Les valeurs flottantes hors de 0 à 1 vont en dessous ou au dessus |
| **Étirer** | La plus petite valeur du fichier devient la **Hauteur minimale** et la plus grande la **Hauteur maximale** |
| **Absolues** | Le fichier contient de vraies hauteurs : la hauteur d'un sommet est la valeur fois **Unités par valeur** au dessus de l'**Origine Z**. Les hauteurs minimale et maximale viennent du fichier (la notification les affiche). Utilisez-le pour les cartes R32F de World Machine ou Gaea : avec **Unités par valeur** à 1 les valeurs sont des unités, à 52,49 des mètres |

**Un sommet par pixel** choisit la **Taille des tuiles** (et la **Taille Y**, en gardant les proportions de l'image) pour que chaque pixel tombe sur un sommet, sans rééchantillonnage. Il demande des côtés d'image multiples de 2 puissance **Puissance des tuiles** plus un, comme 257, 513, 1025 ou 2049 (les tailles habituelles de World Machine). Par exemple une image 1025 x 1025 avec la puissance 4 donne 64 x 64 tuiles de 16 x 16 cases. La fenêtre dit pourquoi quand la taille ne convient pas. La limite de 4096 tuiles s'applique toujours : 2049 pixels demandent la puissance 4 et deux imports, ou une résolution plus basse.

### Taille et hauteur

| Option | Ce qu'elle fait | Défaut |
| --- | --- | --- |
| **Taille X**, **Taille Y** | Taille du terrain en unités (16 à 32768) | 4096, 4096 |
| **Origine X**, **Y**, **Z** | Position du terrain | 0, 0, 0 |
| **Centré sur l'origine** | Activé : l'origine est le centre du terrain. Désactivé : c'est le coin de plus petits X et Y | Activé |
| **Hauteur minimale** | Hauteur du noir | 0 |
| **Hauteur maximale** | Hauteur du blanc | 512 |
| **Puissance des tuiles** | 2, 3 ou 4 | 3 |
| **Taille des tuiles** | Taille voulue d'une tuile en unités (16 à 16384) | 512 |
| **Jupe de bordure** | Abaisse l'anneau extérieur de sommets de ce nombre d'unités, pour que le bord ouvert du terrain disparaisse. 0 désactive | 0 |
| **Épaisseur des brushes** | Épaisseur du brush sous chaque tuile (1 à 4096) | 16 |
| **Matériau** | Matériau des faces de displacement. Prenez un matériau de blend pour voir les textures de la section texturage | `DEV/DEV_BLENDMEASURE` |
| **Échelle de lightmap** | Unités par luxel des displacements | 16 |

Le brush sous chaque tuile a sa face du dessus à la hauteur minimale, et ses autres faces utilisent `TOOLS/TOOLSNODRAW`.

### Image

| Option | Ce qu'elle fait | Défaut |
| --- | --- | --- |
| **Inverser** | Le blanc devient bas et le noir devient haut | Désactivé |
| **Retourner X**, **Retourner Y** | Reflètent l'image | Désactivé |
| **Échantillonnage bicubique** | Échantillonne l'image en douceur entre les pixels (Catmull-Rom, sans dépasser les pixels voisins). Désactivé : échantillonnage bilinéaire. Quand l'image a exactement un pixel par sommet, les deux donnent les valeurs des pixels | Activé |
| **Passes de lissage** | Floute les hauteurs (un filtre 3 x 3 par passe, 0 à 64) avant de construire le terrain | 0 |
| **Grille des pixels** | **Coin** : le premier et le dernier pixel sont exactement sur les bords du terrain (pixel ponctuel, la convention des images 2^n+1 comme 1025). **Centre** : chaque pixel couvre son propre carré et les pixels extérieurs sont à un demi-pixel des bords (pixel surface, pour des images 1024 ou 2048 exportées ainsi) | Coin |

Les tuiles sont toujours sans couture : chaque sommet prend sa hauteur depuis sa position dans le monde, donc les sommets d'une arête commune ont la même hauteur.

### Texturage

Le texturage écrit les **lignes de blend 4 textures** (`multiblend`) et l'alpha des displacements : le matériau de blend montre la texture 1 sur le sol plat, la texture 2 sur la roche, la texture 3 sur la neige et la texture 4 sur le sable. Vous pouvez ensuite peindre par-dessus avec le mode **Peindre l'alpha**.

| Mode | Ce qu'il fait |
| --- | --- |
| **Aucun** | Pas de texturage (défaut) |
| **Pente et hauteur** | Texturage automatique : **roche** (canal 2) sur les pentes raides, **herbe** (canal 1) sur le sol plat, **neige** (canal 3) au-dessus d'une hauteur et **sable** (canal 4) sous une hauteur. La neige et le sable ne tiennent pas sur la roche |
| **Splat map** | Une seconde image pilote le blend |

Seuils de **Pente et hauteur** :

| Option | Ce qu'elle fait | Défaut |
| --- | --- | --- |
| **Roche à partir de la pente** | Pente en degrés à partir de laquelle la roche apparaît (0 à 89) | 35 |
| **Fondu de la roche** | Largeur de la transition en degrés | 10 |
| **Neige au-dessus de** | Part de la plage de hauteur (0 à 1) au-dessus de laquelle la neige apparaît | 0,8 |
| **Fondu de la neige** | Largeur de la transition, en part de la plage | 0,1 |
| **Sable sous** | Part de la plage de hauteur sous laquelle le sable apparaît. 0 désactive le sable | 0 |
| **Fondu du sable** | Largeur de la transition | 0,05 |

**Splat map** prend soit une image **Splat map** (RVB ou RVBA, 8 ou 16 bits ou flottante : rouge, vert, bleu et alpha sont la couverture des textures 1 à 4 ; une image RVB n'a pas de texture 4), soit, quand ce champ est vide, quatre images en niveaux de gris **Canal 1** à **Canal 4** (un champ vide veut dire que cette texture vaut 0). Les couvertures sont normalisées par sommet. Les images de splat couvrent la même étendue que la carte de hauteur, sont reflétées par **Retourner X** et **Retourner Y** comme elle, utilisent la même **Grille des pixels**, et peuvent avoir n'importe quelle taille. **Couverture** (défaut) convertit les couvertures en lignes de blend lues par le jeu pour que chaque texture montre sa part en jeu ; **Lignes brutes** les écrit telles quelles (voir [Masques de blend](#importer-des-masques-de-blend)). Pour choisir les canaux, régler les niveaux ou peindre l'alpha WorldVertexTransition, utilisez **Importer des masques de blend** après l'import.

### Aperçu et import

Le dessin **Disposition des tuiles** montre la carte de hauteur (après inversion, retournements et lissage) sur l'emprise du terrain, avec les limites des tuiles en jaune et `N` pour le bord nord. Cliquez sur **Importer** pour construire le terrain. Le bouton **Importer** reste désactivé tant qu'aucun fichier n'est donné et qu'un réglage est invalide. Les erreurs (fichier illisible, mauvaise taille RAW) s'affichent en notification et ne changent rien.

## Générer un terrain

Ouvrez **Outils > Displacements > Générer un terrain...**. Le terrain est construit depuis du bruit fractal, puis éventuellement érodé et texturé. Tant que la fenêtre est ouverte, un **aperçu en direct** (65 x 65 sommets au plus) est dessiné dans la vue 3D : une surface ombrée avec une grille grossière et un contour jaune de l'emprise. L'aperçu suit chaque changement. Désactivez-le avec **Aperçu en direct dans la vue 3D**. Il disparaît à la fermeture de la fenêtre. L'érosion de l'aperçu fait 200 passes au plus et à une résolution plus grossière : elle n'approche donc que le résultat final.

### Préréglages

| Préréglage | Aspect |
| --- | --- |
| **Collines** | Collines douces en bruit simplex avec une légère distorsion |
| **Montagnes** | Bruit de Perlin en crêtes sur 6 octaves, avec une courbe de hauteur relevée |
| **Canyon** | Bruit en crêtes inversé découpé en 4 terrasses : mesas et gorges |
| **Îles** | Bruit simplex avec un niveau de la mer et un masque d'île rond |
| **Dunes** | Bruit en crêtes étiré selon Y avec une forte distorsion |
| **Plateau** | Bruit de Perlin avec un niveau de la mer et deux terrasses nettes |

Un préréglage ne charge que les réglages de bruit (la graine, la taille et la plage de hauteur sont conservées). **Graine aléatoire** choisit une nouvelle graine ; la même graine avec les mêmes réglages donne toujours le même terrain (la graine est dans la notification et dans le résultat MCP).

### Bruit

| Option | Ce qu'elle fait | Défaut (collines) |
| --- | --- | --- |
| Base : **Perlin** ou **Simplex** | La fonction de bruit | Simplex |
| Forme : **Simple**, **En crêtes**, **En bosses** | Les crêtes font des arêtes vives, les bosses des rondeurs | Simple |
| **Reliefs** | Nombre des plus grands reliefs sur le plus long côté du terrain (0,1 à 64). Le terrain a le même aspect à toute taille | 3 |
| **Octaves** | Couches de détail (1 à 10) | 4 |
| **Persistance** | Poids de chaque couche plus fine (0,05 à 1) | 0,45 |
| **Distorsion** | Déforme le bruit avec un autre bruit (0 à 2). 0 désactive | 0,1 |
| **Étirement** | Allonge les reliefs selon Y (au-dessus de 1) ou X (en dessous de 1) | 1 |
| **Contraste** | Étale le bruit sur la plage de hauteur (0,1 à 4) | 1,4 |
| **Courbe de hauteur** | Exposant sur les hauteurs : au-dessus de 1 aplatit les plaines et accentue les sommets (0,2 à 5) | 1,2 |
| **Inverser le bruit** | Les sommets deviennent des vallées | Désactivé |
| **Niveau de la mer** | Part de la plage de hauteur (0 à 0,95) aplatie en bas | 0 |
| **Terrasses** | Nombre de paliers plats, 0 désactive | 0 |
| **Netteté des terrasses** | 0 donne des paliers doux, 1 des falaises entre les paliers | 0,5 |
| **Masque d'île** | Fait tomber les hauteurs à zéro vers les bords | Désactivé |
| Île : **Rond** ou **Carré**, **Largeur de la chute**, **Puissance de la chute** | Forme du masque, part du rayon utilisée par la chute, et durée pendant laquelle le centre reste haut | Rond, 0,5, 1 |

### Résultat

- **Nouveaux displacements** crée les tuiles avec les réglages de taille, origine, plage de hauteur, puissance et taille des tuiles, épaisseur des brushes, matériau, jupe et échelle de lightmap, identiques à ceux de la fenêtre d'import.
- **Remplacer les N sélectionnés** garde les displacements sélectionnés et remplace leurs hauteurs par le terrain généré (les réglages de taille, d'origine et de tuiles ne servent pas). Le terrain couvre l'emprise de la sélection, à l'espacement de sommets le plus fin des displacements sélectionnés (1025 sommets par côté au plus), donc les tuiles de plusieurs displacements restent cousues. Le point le plus bas de la plage de hauteur se trouve sur le plan de la face. Les décalages sont remis à zéro, les lignes de blend sont réécrites quand le texturage est actif, et la surface est cousue ensuite, en une seule étape d'annulation.

### Érosion

**Érosion thermique** (la matière glisse sur les pentes plus raides que l'**Angle d'éboulis**) et **Érosion hydraulique** (la pluie creuse où l'eau coule vite et dépose les sédiments où elle ralentit) utilisent les mêmes algorithmes et les mêmes paramètres que l'érosion de l'[outil displacement](displacement-tool.md) : passes, angle d'éboulis, force, pluie, évaporation, capacité, dépôt. Elles agissent sur la carte de hauteur générée avant la construction des tuiles, la thermique d'abord. La pluie hydraulique utilise la graine.

### Texturage

**Texturer selon la pente et la hauteur** (actif par défaut) utilise les mêmes seuils que l'import. Désactivez-le pour garder les lignes de blend des displacements remplacés.

### Appliquer

**Appliquer** construit le terrain (ou remplace les hauteurs) et affiche une notification avec la graine. **Fermer** retire l'aperçu.

## Exporter une carte de hauteur

Sélectionnez des displacements (ou des solides qui en ont) et ouvrez **Fichier > Exporter > Exporter une carte de hauteur...**.

| Option | Ce qu'elle fait | Défaut |
| --- | --- | --- |
| **Fichier de hauteur** | Fichier de sortie ; l'extension choisit le format : `.png` (gris 16 bits), `.tif` ou `.tiff` (16 bits, ou flottant 32 bits), `.exr` (flottant 32 bits), `.pfm` (flottant 32 bits), `.r32` (flottant 32 bits petit-boutiste, R32 de World Machine), `.r16` ou `.raw` (16 bits petit-boutiste, ou flottant 32 bits pour `.raw`) | vide |
| **Fichier splat map** | PNG ou TIFF 16 bits, ou EXR flottant, facultatif, avec les lignes brutes du blend 4 textures (l'alpha quand un displacement n'a pas de lignes de blend) : rouge, vert, bleu et alpha sont les canaux 1 à 4. **Exporter les masques de blend** écrit plutôt des masques de couverture et l'alpha | vide |
| **Flottant 32 bits (tif, raw)** | Écrit `.tif` et `.raw` en flottant 32 bits | Désactivé |
| **Valeurs** | **Auto** : les vraies hauteurs dans les fichiers flottants, la plage du plus bas au plus haut dans les fichiers 16 bits. **Du plus bas au plus haut** : le noir et le blanc sont les hauteurs minimale et maximale. **Absolues** (fichiers flottants seulement) : chaque pixel est la hauteur monde divisée par **Unités par valeur**, rien n'est perdu | Auto |
| **Hauteur minimale**, **Hauteur maximale** | **Fixe** donne la hauteur écrite en noir et en blanc. Désactivé : le point le plus bas et le plus haut | Désactivé |

L'image couvre l'emprise des displacements sélectionnés, avec son haut au bord nord (+Y), et un pixel par sommet du displacement le plus fin. Un terrain importé est réexporté avec exactement un pixel par sommet. Les surfaces qui se recouvrent gardent le point le plus haut, et les pixels sans displacement sont remplis depuis leurs voisins. La notification donne la taille de l'image et les hauteurs écrites en noir et en blanc : reprenez-les comme **Hauteur minimale** et **Hauteur maximale** pour réimporter le fichier, avec la même taille, et obtenir le même terrain (l'outil MCP renvoie aussi la taille et le coin). Un export flottant absolu réimporté avec les valeurs **Absolues** et **Unités par valeur** à 1 redonne les mêmes hauteurs au flottant près.

## Importer des masques de blend

Les outils de terrain donnent les masques de blend (splat) en **images**, mais Source mélange les textures des displacements **par sommet**. **Fichier > Importer > Importer des masques de blend...** convertit les images en données de sommet des displacements sélectionnés, en une étape d'annulation :

- **WorldVertexTransition** lit l'**alpha** des sommets du displacement (les lignes `alphas` du VMF, de 0 à 255) : 0 montre `$basetexture`, 255 montre `$basetexture2`. Le moteur le passe au shader comme alpha de la couleur de sommet (`COLOR0`).
- **Lightmapped_4WayBlend** lit les **lignes du blend 4 textures** du displacement : les blocs VMF `multiblend` (4 valeurs par sommet), `alphablend` (4 valeurs) et `multiblend_color_0` à `multiblend_color_3` (une couleur par texture), le format de Hammer++ et du Hammer de CS:GO et Portal 2. Le compilateur les écrit dans le lump multiblend des displacements (80 octets par sommet, comme le VBSP de Garry's Mod) et le moteur les passe au shader comme données de sommet supplémentaires (les données de peinture « COLOR1 »). Le shader part de la texture 1 et mélange par dessus la texture 2, puis 3, puis 4, avec les deuxième, troisième et quatrième valeurs `multiblend` comme quantités. L'import écrit les valeurs `multiblend` et garde `alphablend` et les couleurs (blanches quand elles sont nouvelles).

La fenêtre :

| Option | Ce qu'elle fait | Défaut |
| --- | --- | --- |
| **Fichier 1** à **Fichier 4** | Jusqu'à quatre images : PNG 8 ou 16 bits, TIFF (16 bits ou flottant), EXR, PFM, HDR, TGA, BMP, JPEG ou RAW, lues en pleine précision. **Ajouter un fichier** et **Retirer le dernier fichier** changent leur nombre. Chaque fichier affiche sa taille et son format | un fichier vide |
| **Écrire dans** | **Auto** : les faces avec un matériau Lightmapped_4WayBlend, et celles qui ont déjà des lignes 4 textures, reçoivent les lignes 4 textures, les autres l'alpha. **Alpha (WorldVertexTransition)** ou **Lignes 4 textures (Lightmapped_4WayBlend)** imposent l'un des deux | Auto |
| **Alpha : Canal** | Le canal d'image qui devient l'alpha : rouge, vert, bleu, alpha ou luminance d'un fichier, **Aucun (0)** ou **Plein (1)** | Fichier 1 luminance |
| **Texture 1** à **Texture 4** | Le canal qui donne la couverture de chaque texture du blend 4 textures, avec aussi **Le reste (1 moins les autres)** | Fichier 1 rouge, vert, bleu, alpha (une splat map RVBA) |
| **Inverser**, **Niveaux** | Par canal : inverser, puis **Noir** et **Blanc** (les valeurs d'entrée qui deviennent 0 et 1), **Gamma** (au dessus de 1 éclaircit les valeurs moyennes) et **Contraste** (au dessus de 1 durcit les transitions). **Réinitialiser** remet les niveaux | désactivé, 0, 1, 1, 1 |
| **Normaliser à une somme de 1** | Met les quatre couvertures de chaque sommet à l'échelle pour que leur somme fasse 1 (tout à zéro donne la texture 1) | Activé |
| **Couverture** / **Lignes brutes** | **Couverture** : les images disent quelle part de chaque texture se voit (une splat map de World Machine ou Gaea). Elles sont converties en valeurs lues par le shader : texture 4 = sa couverture, texture 3 = sa part de ce que laisse la texture 4, texture 2 = sa part de ce que laissent les textures 3 et 4. Le jeu montre alors ces parts (avant les réglages de luminance et de plage de blend du matériau). **Lignes brutes** : les valeurs sont écrites telles quelles, comme le pinceau 4 textures de l'outil displacement | Couverture |
| **Étendue fixe** | Le rectangle couvert par les images (**X minimal**, **Y minimal**, **Taille X**, **Taille Y**). Désactivé : les limites des displacements sélectionnés vus de dessus. **Même étendue que la carte de hauteur** copie l'étendue du dernier import de carte de hauteur, pour que des masques exportés avec la carte de hauteur collent exactement | Désactivé |
| **Retourner X**, **Retourner Y**, **Échantillonnage bilinéaire**, **Grille des pixels** | Comme pour l'import de carte de hauteur : le haut de l'image est le bord nord (+Y). Sans bilinéaire, l'échantillonnage est bicubique | désactivé, désactivé, activé, Coin |
| **Aperçu dans les vues 3D** | Applique les masques pendant que vous changez les réglages, pour que les vues Éclairée et Cuite montrent le blend. **Appliquer** le garde, **Fermer** le retire | Désactivé |

Chaque sommet prend la valeur du masque sous sa position vue de dessus. Un petit aperçu montre le résultat (textures 1 à 4 en vert, brun, gris et blanc, ou l'alpha en gris). Les faces 4 textures reçoivent aussi l'alpha (1 moins la couverture de la texture 1), le terrain reste donc correct avec un matériau WorldVertexTransition.

Les vues 3D (Éclairée, Cuite, Vue joueur) dessinent les matériaux WorldVertexTransition et Lightmapped_4WayBlend depuis ces données, et la carte compilée les contient : l'alpha dans les sommets des displacements, les lignes 4 textures dans le lump multiblend. Les lignes du VMF gardent 6 décimales.

## Exporter les masques de blend

**Fichier > Exporter > Exporter les masques de blend...** écrit le blend des displacements sélectionnés en images, sur la grille de pixels d'**Exporter une carte de hauteur** (un pixel par sommet, le nord en haut), pour qu'elles collent à une carte de hauteur exportée et retournent dans World Machine, Gaea ou un éditeur d'images :

| Option | Ce qu'elle fait |
| --- | --- |
| **4 textures (RGBA)** | Rouge, vert, bleu et alpha sont les textures 1 à 4 : PNG ou TIFF 16 bits, EXR flottant, ou TIFF flottant avec **Flottant 32 bits** |
| **Alpha (gris)** | L'alpha des sommets (WorldVertexTransition) en image grise : png, tif, exr, pfm, r16, r32 ou raw |
| **Texture 1** à **Texture 4 (gris)** | Une image grise par texture |
| **Couverture** / **Lignes brutes** | **Couverture** écrit quelle part de chaque texture se voit en jeu, calculée depuis les lignes comme le fait le shader (l'inverse de l'import), **Lignes brutes** les valeurs `multiblend` |

Les displacements sans lignes 4 textures utilisent leur alpha pour les textures 1 et 2. Réimporter les masques exportés avec la même étendue et le même mode redonne les mêmes lignes (aux 6 décimales du VMF près, et à 1/65535 près pour les fichiers 16 bits).

## Outils MCP

Les paramètres complets sont dans la [référence des outils MCP](../../MCP.md) ; les options de la fenêtre ont les mêmes noms. En bref :

- `import_heightmap` : `path` (ou `tiles` et `tile_columns`), options RAW (`raw_width`, `raw_height`, `raw_big_endian`, `raw_format`), `values` (`auto`, `unit`, `stretch`, `absolute`) et `value_scale`, `match_pixels`, `pixel_grid`, `size_x`, `size_y`, `origin`, `centered`, `min_height`, `max_height` (ou `height_scale`), `tile_power`, `tile_size`, `invert`, `flip_x`, `flip_y`, `smoothing`, `sampling`, `skirt`, `thickness`, `material`, `lightmap_scale`, `texturing` (`none`, `auto`, `splat`) avec ses seuils, `splat`, `splat_channels` et `splat_weights`. `preview` à vrai ne fait que rapporter la disposition des tuiles, le format et la plage des valeurs. Le résultat donne les hauteurs utilisées et l'étendue pour `import_blend_masks`.
- `export_heightmap` : les displacements (`faces`, `ids`, `solid_ids`, ou la sélection), `path`, `splat_path`, `float`, `values` (`auto`, `normalized`, `absolute`), `value_scale`, `min_height`, `max_height`, `overwrite`. Il renvoie les hauteurs et l'emprise pour le réimporter.
- `import_blend_masks` : les displacements, `files` (1 à 4), `target` (`auto`, `alpha`, `four_way`), les sources `alpha` et `texture1` à `texture4` (des textes comme `"1.r"`, `"2.a"`, `"l"`, `"none"`, `"one"`, `"rest"`), `adjust` (par canal `invert`, `black`, `white`, `gamma`, `contrast`), `normalize`, `weights` (`coverage`, `direct`), l'étendue (`size_x`, `size_y`, `origin`, `centered`), `flip_x`, `flip_y`, `pixel_grid`, `sampling` et les options RAW.
- `export_blend_masks` : les displacements, `path` (RGBA), `alpha_path`, `texture_paths`, `weights`, `float`, `overwrite`.
- `generate_terrain` : `preset`, `seed`, les options de bruit, de taille, d'érosion et de texturage, `replace_selected`, et `preview` (`show` ou `hide`) pour l'aperçu 3D. Il renvoie la graine.

Chaque appel est une seule étape d'annulation nommée « MCP : import d'une carte de hauteur », « MCP : import de masques de blend » ou « MCP : génération de terrain ».

## Conseils

- Pour un terrain à l'échelle d'une carte, une tuile de puissance 3 de 512 unités donne un sommet tous les 64 unités. Pour plus de détail, baissez **Taille des tuiles** ou montez **Puissance des tuiles**.
- Un terrain seul (une nouvelle map avec seulement les tuiles importées) se compile et se cuit comme une map qui fuit, la vue Cuite marche donc tout de suite ; elle reste sombre tant que la map n'a pas de lumière (un `light_environment` pour un ciel). Pour le jeu, ajoutez un `info_player_start` et rendez la map étanche.
- Source veut que le terrain soit étanche vis-à-vis du vide comme toute autre géométrie : construisez des murs autour, ou utilisez la **Jupe de bordure** avec un anneau de brushes de skybox.
- Après un import, utilisez l'[outil displacement](displacement-tool.md) pour sculpter, **Peindre l'alpha**, coudre avec un autre terrain, ou changer la puissance avec **Garder la forme**.
- Les cartes de hauteur exportées s'ouvrent dans tout éditeur d'images et dans les outils de terrain comme World Machine ou Gaea (PNG 16 bits, ou EXR et R32 flottants pour la pleine précision).
- Le [guide World Machine et Gaea](../guides/world-machine-gaea.md) décrit un aller-retour complet : hauteurs, splat maps, matériaux et compilation.
