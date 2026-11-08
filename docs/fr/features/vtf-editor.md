[Documentation](../README.md) > Fonctionnalités > Éditeur de VTF | [English](../../en/features/vtf-editor.md) | [Русский](../../ru/features/vtf-editor.md)

# Éditeur de VTF

Les textures du moteur Source sont des fichiers VTF. Hammer Pro les modifie de deux façons : dans un **éditeur VTF externe** déjà installé sur l'ordinateur (VTFEdit, VTFEdit++ ou VTFEdit Reloaded), ou, s'il n'y en a aucun, dans son propre **éditeur de VTF intégré**, inspiré de VTFEdit++.

## L'ouvrir

| Depuis | Effet |
|---|---|
| [Éditeur de matériaux](material-editor.md), section **Textures liées** et chaque paramètre de texture (`$basetexture`, `$bumpmap`...) | Le bouton **VTF** ouvre cette texture. Il utilise l'éditeur externe s'il en trouve un, l'éditeur intégré sinon. Clic droit sur le bouton pour choisir : **Ouvrir dans l'éditeur VTF intégré** ou **Ouvrir dans** le programme externe. |
| [Navigateur de textures](texture-browser.md) : clic droit sur une vignette > **Modifier la texture (éditeur VTF)...** | Ouvre la texture de base du matériau dans l'éditeur intégré, liée à ce matériau. |
| **Outils > Éditeur de VTF...** (action `vtf_editor`, aussi dans la palette de commandes `Ctrl+P`) | Ouvre la fenêtre vide : saisissez un nom de texture, ouvrez un fichier, ou partez d'une image. |
| Déposer un `.vtf` ou une image sur la fenêtre | Un `.vtf` est ouvert ; une image remplace l'image affichée, ou démarre une nouvelle texture si rien n'est ouvert. |
| MCP : `open_vtf_editor` | Pareil, avec `external: true` pour l'éditeur externe. |

## L'éditeur externe

Hammer Pro cherche VTFEdit, VTFEdit++ et VTFEdit Reloaded dans les dossiers d'installation habituels (Program Files, le dossier des programmes locaux, les dossiers de l'utilisateur, la racine des disques), dans le `PATH`, et prend le programme associé aux fichiers `.vtf`. **Paramètres > Éditeur de VTF** montre ce qu'il a trouvé et permet de **Parcourir...** vers n'importe quel programme, de **Détecter** à nouveau, ou de cocher **Toujours utiliser l'éditeur intégré**. Le réglage `vtf.editor_path` n'est modifié que par vous, dans cette fenêtre.

- Une texture qui est un fichier sur le disque est ouverte là où elle est (une copie `.bak` est gardée une fois).
- Une texture dans un VPK ou un GMA est extraite dans un fichier temporaire. Quand vous enregistrez dans l'éditeur externe, Hammer Pro copie le résultat dans votre premier [dossier de textures personnalisé](custom-texture-folders.md) activé. Un dossier sans préfixe sert la texture sous le même nom, ce qui remplace celle de l'archive ; avec un préfixe, la copie prend le nom préfixé et, quand l'éditeur a été ouvert depuis un paramètre de l'éditeur de matériaux, ce paramètre désigne la copie.
- Hammer Pro surveille le fichier. À chaque enregistrement de l'éditeur externe, la texture est rechargée : vignettes, vues 3D et éditeur intégré ouvert suivent, et une notification le signale. La liste **Édition externe** de l'onglet **Infos** de l'éditeur intégré montre les fichiers surveillés, avec un bouton **Arrêter**.

## L'éditeur intégré

Disposition : une barre d'outils en haut, l'image au centre, un panneau latéral à quatre onglets, et une ligne d'état.

### Ouvrir et enregistrer

| Commande | Effet |
|---|---|
| **Texture** + **Charger** | Charge une texture du jeu par son nom (`brick/brickwall001a`), depuis les dossiers, les VPK et les GMA. |
| **Ouvrir un fichier...** | Ouvre un fichier `.vtf`, ou une image (PNG, TGA, JPG, BMP) comme nouvelle texture. |
| **Nouveau depuis une image...** | Crée une texture à partir d'une image ; le formulaire **Enregistrer comme texture** s'ouvre. |
| **Enregistrer** (`Ctrl+S`) | Écrit la texture là où elle vient. Une texture d'une archive est écrite en copie dans le dossier personnalisé choisi dans **Enregistrer comme texture...**. L'enregistrement se fait dans un thread en arrière-plan ; une copie `.bak` d'un fichier écrasé est gardée une fois. |
| **Enregistrer comme texture...** | Écrit dans un dossier de textures personnalisé sous un nom (`sous-dossier/nom`), et avec **Créer le matériau (VMT)** écrit aussi le VMT dont `$basetexture` désigne la texture (jamais par-dessus un existant). Quand l'éditeur a été ouvert depuis un paramètre de l'éditeur de matériaux, ce paramètre prend la nouvelle texture, prêt à y être enregistré. |
| **Enregistrer sous...** | Écrit un `.vtf` n'importe où. |
| **Recharger** | Relit la texture. |
| **Éditeur externe** | Ouvre la même texture dans le programme externe (voir plus haut). |

### Visualiser

La barre d'affichage a les boutons de canaux **R**, **G**, **B** et **A** (rouge, vert, bleu, alpha), **Alpha seul** (l'alpha en niveaux de gris), **Couleur seule** et **Tout** ; le niveau de **Mip** (avec sa taille) ; l'**Image** d'une texture animée avec **Lire** ; la **Face** d'une cubemap (droite, gauche, arrière, avant, haut, bas, sphere map) ; la **Tranche** d'une texture volumique ; **Ajuster**, **100%**, **200%** ; et le fond (damier, noir, blanc, gris). La molette zoome autour du pointeur, le glissement déplace l'image, le double clic l'ajuste, et le pixel sous le pointeur est décrit en bas à gauche (position, R, G, B, A).

### Onglet Infos

Source, **version du VTF** (7.1 à 7.5), taille (un avertissement si ce n'est pas une puissance de deux), format (et le format cible après une conversion), niveaux de mipmap, images, faces, tranches, miniature, taille du fichier, **Image de départ**, **Échelle du bumpmap**, **Réflectivité** (rouge, vert et bleu, ou calculée à l'enregistrement), et les drapeaux.

### Onglet Drapeaux

Une case pour chaque drapeau : échantillonnage au plus proche, trilinéaire, clamp S, clamp T, clamp U, anisotrope, indice DXT5, sRGB, normal map, sans mipmaps, sans niveau de détail, tous les mips, procédurale, alpha 1 bit, alpha 8 bits, environment map, cible de rendu, cible de rendu de profondeur, sans remplacement de débogage, copie unique, pré sRGB, sans tampon de profondeur, texture de sommets, bump auto ombré et bordure. Survolez un drapeau pour savoir ce qu'il fait. **Régler les drapeaux alpha d'après l'image** garde les drapeaux alpha 1 bit et 8 bits en accord avec les pixels et le format quand une texture est reconstruite. Le drapeau environment map suit la case cubemap de l'onglet **Édition**.

### Onglet Édition

| Section | Effet |
|---|---|
| **Redimensionner** | Largeur et hauteur (puissances de deux dans les listes, n'importe quelle valeur dans le champ), **Puissance de deux la plus proche**, `/2`, `x2`, le filtre (plus proche, lisse pour la couleur, lisse pour les données, normales renormalisées) et **Appliquer**. **Arrondir à une puissance de deux à l'enregistrement** (activé par défaut) car le moteur ne sait pas utiliser d'autres tailles. |
| **Format** | Convertit en DXT1, DXT3, DXT5, ATI1N, ATI2N, BGRA8888, RGBA8888, ABGR8888, ARGB8888, BGRX8888, BGR888, RGB888, BGR565, RGB565, BGRA4444, BGRA5551, IA88, I8, A8 ou UV88, avec la qualité de compression (rapide, normale, maximale). |
| **Mipmaps** | Mipmaps activées ou non et **Régénérer**, avec le filtre (d'après les drapeaux, couleur avec gamma, données, normal map). |
| **Normal map depuis une height map** | Hauteur depuis la luminosité, un canal de couleur, l'alpha ou la moyenne ; filtre (simple, Sobel 3x3, Sobel 5x5) ; intensité ; alpha (opaque, gardé, ou la hauteur) ; boucler aux bords (pour les textures qui se répètent) ; convention OpenGL (laissez décoché pour Source, qui utilise celle de DirectX) ; **Marquer comme normal map** active le drapeau normal et retire le sRGB. |
| **Canal alpha** | Entièrement opaque, une constante, depuis la luminosité ou un canal de couleur, inversé, ou depuis un fichier image en niveaux de gris. |
| **Images et faces** | **Cubemap (6 faces)** activée ou non, **Ajouter une image** (copie de la dernière), **Ajouter une image depuis un fichier...**, **Retirer cette image**, **Remplacer cette image...** (l'image et la face affichées). |

Les changements qui touchent les pixels, le format ou les mipmaps reconstruisent la texture à l'enregistrement : l'image est compressée à nouveau à partir de l'image pleine taille gardée en mémoire. Un changement qui ne touche que l'en-tête (drapeaux, échelle du bumpmap, réflectivité, image de départ) garde exactement les pixels et niveaux de mip enregistrés. Les textures volumiques (tranches) et les formats que l'éditeur ne sait pas écrire (par exemple les formats flottants) peuvent être affichés et avoir leur en-tête modifié, mais pas leurs pixels.

### Onglet Export

**Exporter l'image affichée...** écrit le niveau de mip, l'image et la face que vous regardez en PNG, TGA, BMP ou JPG, avec les quatre canaux. **Exporter toutes les images et faces...** écrit un fichier par image et par face (`nom_f000_rt.png`). **Importer une image...** remplace l'image et la face affichées (une texture à image unique prend la taille de la nouvelle image ; les autres redimensionnent l'image à leur taille).

## MCP

Tout ce que fait la fenêtre est aussi accessible aux clients [MCP](../../MCP.md) (en anglais) : `get_vtf_info` (détails d'une texture ou d'un fichier `.vtf`), `preview_vtf` (une image avec le mip, l'image, la face et les canaux choisis), `export_vtf` (vers PNG, TGA, BMP ou JPG), `edit_vtf` (une liste d'opérations : drapeaux, format, taille, mipmaps, normal map, alpha, échelle du bumpmap, image de départ, réflectivité, version, cubemap, images ; écrit dans un dossier de textures personnalisé avec un VMT facultatif ; `dry_run` pour regarder d'abord), `open_vtf_editor` et `list_vtf_editors`. Le MCP ne lit et n'écrit des fichiers que dans le dossier de la map et les dossiers de textures personnalisés. Les outils existants `get_material_info`, `get_material_preview` et `import_images` couvrent toujours les matériaux et l'import simple d'images.

## Pages liées

- [Éditeur de matériaux](material-editor.md)
- [Navigateur de textures](texture-browser.md)
- [Dossiers de textures personnalisés et import d'images](custom-texture-folders.md)
- [Éditeur de shaders](shader-editor.md)
- [Fenêtre des paramètres](settings-dialog.md)

## Destination de création et VMT associé

Lors de la création, choisissez le chemin complet du fichier VTF avec Parcourir, ou utilisez un dossier personnalisé configuré et un nom relatif. Le VTF et son VMT associé sont écrits en arrière-plan. Un VMT existant est conservé.

Le VMT associé s’ouvre automatiquement dans son propre onglet de l’éditeur VTF. Il reprend le sélecteur de shader, les paramètres groupés, les contrôles de textures et le texte VMT de l’éditeur de matériaux. Chaque onglet conserve ses modifications non enregistrées lorsque vous changez d’onglet ou créez une autre texture. Ctrl+S enregistre le matériau actif avec une sauvegarde. Revenez dans Texture pour modifier l’image.
