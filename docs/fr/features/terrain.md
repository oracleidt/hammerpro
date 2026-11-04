[Documentation](../README.md) > Fonctionnalités > Terrain | [English](../../en/features/terrain.md)

# Terrain : cartes de hauteur et génération procédurale

Trois commandes construisent ou exportent des terrains entiers faits de displacements. Elles sont dans le menu **Outils** et dans la palette de commandes (`Ctrl+P`) :

| Commande | Id d'action | Ce qu'elle fait |
| --- | --- | --- |
| **Importer une carte de hauteur...** | `import_heightmap` | Construit un terrain depuis une image de hauteur |
| **Générer un terrain...** | `generate_terrain` | Construit un terrain depuis du bruit fractal, avec érosion et texturage automatique |
| **Exporter une carte de hauteur...** | `export_heightmap` | Écrit les displacements sélectionnés dans une carte de hauteur 16 bits (et une splat map) |

Elles créent une grille de tuiles carrées. Chaque tuile est un brush dont la face du dessus est un displacement. Les tuiles sont cousues : les sommets d'une arête commune sont identiques, donc sans fissure. Tout ce qu'une commande crée forme **une seule étape d'annulation**, et les nouvelles tuiles sont sélectionnées ensuite, prêtes pour l'[outil displacement](displacement-tool.md) (sculpter, peindre l'alpha, éroder).

Les mêmes fonctions existent comme outils MCP `import_heightmap`, `export_heightmap` et `generate_terrain` (voir [Outils MCP](#outils-mcp)).

## Notions

- **Tuile.** Un displacement de puissance 2, 3 ou 4 (4 x 4, 8 x 8 ou 16 x 16 cases) sur un brush de **Épaisseur des brushes** unités. La taille du terrain est coupée en un nombre entier de tuiles égales, le plus proche de **Taille des tuiles**. L'**espacement des sommets** est la taille de la tuile divisée par le nombre de cases, et la fenêtre affiche en direct le nombre de tuiles et l'espacement (« 8 x 8 tuiles (64 displacements), espacement des sommets 64 unités »).
- **Hauteurs.** Une valeur de la carte va du noir (0) au blanc (1) et devient une hauteur entre **Hauteur minimale** et **Hauteur maximale**. **Échelle verticale** est la différence des deux. **Origine Z** est la hauteur monde où se trouve la hauteur 0.
- **Orientation.** X grandit vers la droite de l'image et Y (le nord) grandit vers le haut de l'image. **Retourner X** et **Retourner Y** la reflètent.
- **Limites.** 4096 tuiles au plus, une taille de 16 à 32768 unités par côté, et le terrain entier dans les limites de la carte (plus ou moins 16384 unités). La fenêtre affiche la raison en rouge quand un réglage est hors limites, et les boutons restent désactivés.

## Importer une carte de hauteur

Ouvrez **Outils > Importer une carte de hauteur...**. La fenêtre a ces sections.

### Fichier

| Option | Ce qu'elle fait | Défaut |
| --- | --- | --- |
| **Carte de hauteur** | L'image : PNG (gris 8 ou 16 bits ; les images en couleur sont lues en luminance), TIFF, EXR ou HDR (virgule flottante, remis à l'échelle de 0 à 1 quand les valeurs sortent de cette plage), BMP, TGA, JPEG, ou un fichier **RAW** (`.raw`, `.r16`, `.bin`, `.data`). **Parcourir...** ouvre un sélecteur de fichier. Un chemin relatif part du dossier de la carte | vide |
| **Largeur RAW**, **Hauteur RAW** | Pixels d'un fichier RAW (16 bits non signé). Les deux à 0 veulent dire une image carrée, déduite de la taille du fichier | 0, 0 |
| **RAW gros-boutiste** | Les valeurs 16 bits stockent l'octet de poids fort en premier | Désactivé |

Les images 16 bits gardent toute leur précision : un PNG 16 bits donne 65536 niveaux de hauteur sur la plage verticale. Un côté d'image est limité à 16384 pixels. La fenêtre affiche la taille de l'image et, si le fichier est illisible, la raison.

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

**Splat map** prend soit une image **Splat map** (RVB ou RVBA : rouge, vert, bleu et alpha sont les canaux 1 à 4 ; une image RVB n'a pas de canal 4), soit, quand ce champ est vide, quatre images en niveaux de gris **Canal 1** à **Canal 4** (un champ vide veut dire que ce canal vaut 0). Les poids sont normalisés par sommet. Les images de splat sont reflétées par **Retourner X** et **Retourner Y** comme la carte de hauteur, et peuvent avoir n'importe quelle taille.

### Aperçu et import

Le dessin **Disposition des tuiles** montre la carte de hauteur (après inversion, retournements et lissage) sur l'emprise du terrain, avec les limites des tuiles en jaune et `N` pour le bord nord. Cliquez sur **Importer** pour construire le terrain. Le bouton **Importer** reste désactivé tant qu'aucun fichier n'est donné et qu'un réglage est invalide. Les erreurs (fichier illisible, mauvaise taille RAW) s'affichent en notification et ne changent rien.

## Générer un terrain

Ouvrez **Outils > Générer un terrain...**. Le terrain est construit depuis du bruit fractal, puis éventuellement érodé et texturé. Tant que la fenêtre est ouverte, un **aperçu en direct** (65 x 65 sommets au plus) est dessiné dans la vue 3D : une surface ombrée avec une grille grossière et un contour jaune de l'emprise. L'aperçu suit chaque changement. Désactivez-le avec **Aperçu en direct dans la vue 3D**. Il disparaît à la fermeture de la fenêtre. L'érosion de l'aperçu fait 200 passes au plus et à une résolution plus grossière : elle n'approche donc que le résultat final.

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

Sélectionnez des displacements (ou des solides qui en ont) et ouvrez **Outils > Exporter une carte de hauteur...**.

| Option | Ce qu'elle fait | Défaut |
| --- | --- | --- |
| **Fichier de hauteur** | Fichier de sortie ; l'extension choisit le format : `.png` (gris 16 bits), `.tif` ou `.tiff` (16 bits), `.raw` (16 bits petit-boutiste) | vide |
| **Fichier splat map** | PNG facultatif avec les poids du blend 4 textures (l'alpha quand un displacement n'a pas de lignes de blend) : rouge, vert, bleu et alpha sont les canaux 1 à 4 | vide |
| **Hauteur minimale**, **Hauteur maximale** | **Fixe** donne la hauteur écrite en noir et en blanc. Désactivé : le point le plus bas et le plus haut | Désactivé |

L'image couvre l'emprise des displacements sélectionnés, avec son haut au bord nord (+Y), et un pixel par sommet du displacement le plus fin. Un terrain importé est réexporté avec exactement un pixel par sommet. Les surfaces qui se recouvrent gardent le point le plus haut, et les pixels sans displacement sont remplis depuis leurs voisins. La notification donne la taille de l'image et les hauteurs écrites en noir et en blanc : reprenez-les comme **Hauteur minimale** et **Hauteur maximale** pour réimporter le fichier, avec la même taille, et obtenir le même terrain (l'outil MCP renvoie aussi la taille et le coin).

## Outils MCP

Les paramètres complets sont dans la [référence des outils MCP](../MCP.md) ; les options de la fenêtre ont les mêmes noms. En bref :

- `import_heightmap` : `path`, options RAW, `size_x`, `size_y`, `origin`, `centered`, `min_height`, `max_height` (ou `height_scale`), `tile_power`, `tile_size`, `invert`, `flip_x`, `flip_y`, `smoothing`, `sampling`, `skirt`, `thickness`, `material`, `lightmap_scale`, `texturing` (`none`, `auto`, `splat`) avec ses seuils, `splat` et `splat_channels`. `preview` à vrai ne fait que rapporter la disposition des tuiles.
- `export_heightmap` : les displacements (`faces`, `ids`, `solid_ids`, ou la sélection), `path`, `splat_path`, `min_height`, `max_height`, `overwrite`. Il renvoie les hauteurs et l'emprise pour le réimporter.
- `generate_terrain` : `preset`, `seed`, les options de bruit, de taille, d'érosion et de texturage, `replace_selected`, et `preview` (`show` ou `hide`) pour l'aperçu 3D. Il renvoie la graine.

Chaque appel est une seule étape d'annulation nommée « MCP : import d'une carte de hauteur » ou « MCP : génération de terrain ».

## Conseils

- Pour un terrain à l'échelle d'une carte, une tuile de puissance 3 de 512 unités donne un sommet tous les 64 unités. Pour plus de détail, baissez **Taille des tuiles** ou montez **Puissance des tuiles**.
- Source veut que le terrain soit étanche vis-à-vis du vide comme toute autre géométrie : construisez des murs autour, ou utilisez la **Jupe de bordure** avec un anneau de brushes de skybox.
- Après un import, utilisez l'[outil displacement](displacement-tool.md) pour sculpter, **Peindre l'alpha**, coudre avec un autre terrain, ou changer la puissance avec **Garder la forme**.
- Les cartes de hauteur exportées s'ouvrent dans tout éditeur d'images et dans les outils de terrain comme World Machine ou Gaea (PNG 16 bits).
