[Documentation](../README.md) > Fonctionnalités > Éditeur de shaders | [English](../../en/features/shader-editor.md) | [Русский](../../ru/features/shader-editor.md)

# Éditeur de shaders

L'**Éditeur de shaders** est un éditeur d'images et de matériaux à nœuds, dans l'esprit du shader graph de s&box. Vous reliez des nœuds par des fils : une image, du bruit, un flou, une rampe de couleurs, une normal map fabriquée à partir d'une carte de hauteur... et le résultat apparaît en direct dans un aperçu. Quand le rendu vous plaît, **Écrire les textures** l'écrit dans un dossier de textures personnalisé sous forme de fichiers VTF et du VMT du matériau, avec la même conversion que l'[import d'images](custom-texture-folders.md).

Tout est calculé sur le processeur, à petite taille pendant que vous travaillez et à pleine taille quand vous écrivez les fichiers : aucune fonction particulière de carte graphique n'est nécessaire.

## L'ouvrir

- **Outils > Éditeur de shaders...** (action `shader_editor`, aussi dans la palette de commandes `Ctrl+P`) : ouvre la fenêtre sur le graphe que vous éditiez, ou sur un graphe vide.
- Fenêtre **Importer des images** > **Éditeur de shaders...** : l'ouvre sur la première image de la liste. Si un graphe portant le nom de l'image existe (`brique.hpshader` à côté de `brique.png`), c'est ce graphe qui s'ouvre.
- [Navigateur de textures](texture-browser.md) : clic droit sur une vignette > **Modifier dans l'éditeur de shaders**. Un matériau écrit par l'éditeur avec **Enregistrer le graphe à côté des textures** rouvre son propre graphe ; tout autre matériau s'ouvre comme un nœud Texture sur sa texture de base.
- MCP : `open_shader_editor`.

La fenêtre se redimensionne et se déplace comme n'importe quelle boîte de dialogue. La fermer garde votre graphe en mémoire jusqu'à la fermeture de Hammer Pro : rouvrez-la depuis le menu.

## La fenêtre

| Zone | Contenu |
|---|---|
| Barre d'outils | **Nouveau** (préréglages), **Ouvrir...**, **Enregistrer**, **Enregistrer sous...**, **Annuler**, **Rétablir**, **Ajouter un nœud**, **Cadrer**, **Recharger les images**, le zoom et l'interrupteur **Aperçus des nœuds**. |
| Canevas (à gauche) | Le graphe : nœuds et fils. |
| Panneau latéral (à droite) | **Aperçu**, **Nœud** (les paramètres du nœud sélectionné), **Écrire dans le jeu**, et **Problèmes** quand il y en a. |

### Travailler dans le canevas

| Geste | Effet |
|---|---|
| Molette | Zoome autour du pointeur (25 % à 250 %). |
| Glisser avec le bouton du milieu ou droit sur le fond | Déplace la vue. |
| Clic droit sur le fond, double clic dessus, ou `Espace` | Ouvre la bibliothèque de nœuds au pointeur. |
| Glisser avec le bouton gauche sur le fond | Sélection par rectangle (`Maj` ou `Ctrl` pour ajouter). |
| Clic sur un nœud | Le sélectionne (`Maj` ou `Ctrl` bascule). Ses paramètres apparaissent dans la section **Nœud**. |
| Glisser un nœud | Le déplace (avec les autres nœuds sélectionnés). Les positions s'alignent sur une grille de 8 unités au relâchement. |
| Glisser d'une broche à une autre | Crée un fil. Une sortie peut alimenter plusieurs entrées ; une entrée n'accepte qu'un fil. Les boucles sont refusées. |
| Glisser depuis une broche d'entrée connectée | Soulève le fil : déposez-le sur une autre entrée pour le déplacer, ou sur le fond pour le supprimer. |
| Glisser un fil depuis une sortie et le lâcher sur le fond | Ouvre la bibliothèque de nœuds ; le nœud choisi est connecté automatiquement. |
| Clic sur un fil, puis `Suppr` | Supprime le fil. |
| `Suppr` ou `Retour arrière` | Supprime les nœuds sélectionnés. |
| `Ctrl+A` | Sélectionne tous les nœuds. |
| `Ctrl+C`, `Ctrl+V`, `Ctrl+D` | Copie, colle et duplique les nœuds sélectionnés avec les fils qui les relient. |
| `F` | Cadre tout le graphe (comme **Cadrer**). |
| `Ctrl+Z`, `Ctrl+Y` (ou `Ctrl+Maj+Z`) | Annuler et rétablir (100 étapes ; un glissement ou un mouvement de curseur est une étape). |
| Double clic sur un nœud, ou clic droit > **Prévisualiser ce nœud** | Affiche ce nœud dans l'aperçu à la place de la sortie. |
| Clic droit sur un nœud | **Prévisualiser ce nœud**, **Dupliquer**, **Déconnecter**, **Supprimer**. |

Un point coloré dans l'en-tête d'un nœud signale un problème (rouge : erreur, orange : avertissement) ; survolez-le ou lisez la liste **Problèmes**. Un anneau blanc indique le nœud affiché dans l'aperçu. Couleurs des broches : orange pour les couleurs, vert pour les nombres, bleu pour les coordonnées. Toute broche peut se connecter à toute autre : un nombre devient une couleur grise, une couleur lue comme un nombre donne sa luminosité.

### La bibliothèque de nœuds

Ouverte avec `Espace`, un clic droit ou un double clic sur le fond. Tapez pour chercher par nom, identifiant ou catégorie (`Haut`, `Bas`, `Entrée` pour choisir, `Échap` pour fermer). Les nœuds sont rangés par catégorie.

### Aperçu

La section **Aperçu** montre la sortie (le premier nœud **Sortie**) ou le nœud que vous avez choisi. À côté : le canal affiché (**Couleur et alpha** sur un damier, **Couleur seule**, **Alpha**, **Rouge**, **Vert**, **Bleu**), une répétition 1x1, 2x2 ou 3x3 pour vérifier qu'une texture se répète bien, et la résolution de l'aperçu (128, 256 ou 512 pixels sur le grand côté). La ligne sous l'image donne la taille et la durée du dernier calcul.

L'aperçu est calculé dans un fil d'exécution séparé. Pendant un glissement il descend à 128 pixels ; il ne bloque jamais l'éditeur, et un calcul devenu obsolète est interrompu. **Aperçus des nœuds** place une petite vignette dans chaque nœud (le nœud de sortie montre ce qui est branché dessus) ; désactivez-le pour de très gros graphes.

## Les nœuds

Tous les nœuds travaillent sur quatre canaux par pixel (rouge, vert, bleu, alpha, de 0 à 1 en principe). Les nœuds de calcul agissent sur les canaux de couleur et gardent l'alpha de leur première entrée. Une entrée non connectée utilise une valeur par défaut que vous changez dans la section **Nœud** (le bouton **Réinitialiser** remet la valeur par défaut). Une entrée de coordonnées non connectée utilise les UV de l'image. Les noms d'entrées et de paramètres sont ceux de l'interface.

### Entrées

| Nœud | Entrées | Paramètres | Effet |
|---|---|---|---|
| Texture | UV | Image, Bordure (Répéter, Bloquer, Miroir), Filtre (Linéaire, Au plus proche) | Lit une image : un chemin de fichier (absolu, ou relatif au fichier du graphe ou à la map), ou le nom d'une texture du jeu (par exemple `brick/brickwall001a`). Sorties RVBA et R, V, B, A. Les grandes images sont échantillonnées avec des mipmaps, la répétition reste propre. |
| Couleur | | Couleur | Une couleur constante avec alpha. |
| Nombre | | Valeur | Un nombre constant. |
| Vecteur 2, Vecteur 3 | | Valeur ou X, Y, Z | Des vecteurs constants. |
| UV | | | Les coordonnées de l'image, de 0 à 1 en largeur et en hauteur. |

### Coordonnées

| Nœud | Entrées | Paramètres | Effet |
|---|---|---|---|
| Répétition et décalage | UV | Répétition, Décalage | Répète et déplace les coordonnées. |
| Rotation UV | UV | Angle, Pivot | Fait tourner les coordonnées. |
| Déformation UV | UV, Carte | Force | Décale les coordonnées selon le rouge et le vert d'une carte (0,5 ne décale pas) : distorsion. |

### Calcul

| Nœud | Entrées | Paramètres | Effet |
|---|---|---|---|
| Addition, Soustraction, Multiplication, Division | A, B | | Les quatre opérations (diviser par 0 donne 0). |
| Minimum, Maximum | A, B | | Le plus petit ou le plus grand. |
| Mélange (Lerp) | A, B, T | | A quand T vaut 0, B quand T vaut 1. |
| Limiter | Valeur, Min, Max | | Garde une valeur dans une plage. |
| Saturer | Couleur | | Garde une valeur entre 0 et 1. |
| Puissance | Valeur, Exposant | | Élève à une puissance. |
| Valeur absolue, Un moins | Couleur | | La valeur absolue ; 1 moins la valeur. |
| Remapper | Couleur | Entrée min et max, Sortie min et max | Fait passer une plage vers une autre. |
| Lissage | Couleur | Bord 0, Bord 1 | Une marche lissée entre deux bords. |

### Couleur

| Nœud | Entrées | Paramètres | Effet |
|---|---|---|---|
| Inverser | Couleur | Inverser l'alpha | Inverse les couleurs (alpha gardé sauf demande). |
| Luminosité et contraste | Couleur | Luminosité, Contraste | |
| Teinte, saturation, valeur | Couleur | Teinte (degrés), Saturation, Valeur | |
| Teinte (multiplication) | Couleur, Teinte | Quantité | Multiplie par une couleur de teinte. |
| Niveaux de gris | Couleur | Mode (Luminance, Moyenne, Clarté), Quantité | |
| Postérisation | Couleur | Niveaux | Moins de tons. |
| Niveaux | Couleur | Point noir, Point blanc, Gamma | |
| Seuil | Valeur | Seuil, Douceur | Noir et blanc selon la luminosité. |
| Rampe de couleurs | Valeur | Rampe, Mode (Linéaire, Paliers) | Donne une couleur à chaque luminosité. La rampe a 2 à 16 points avec une position et une couleur chacun (**Ajouter un point**, la croix en retire un). |
| Fusion | Base, Calque, Opacité | Mode | Normal, Multiplier, Écran, Incrustation, Lumière douce, Lumière dure, Ajouter, Soustraire, Différence, Assombrir, Éclaircir, Densité couleur -, Densité couleur +, Exclusion. L'alpha du calque compte aussi. |

### Filtres

Les rayons sont donnés pour une image de 1024 pixels et suivent la taille, donc l'aperçu correspond à la texture écrite.

| Nœud | Entrées | Paramètres | Effet |
|---|---|---|---|
| Flou | Couleur | Rayon | Flou de type gaussien qui se referme sur les bords, donc compatible avec la répétition. |
| Netteté | Couleur | Quantité, Rayon | |
| Détection de contours | Couleur | Force, Rayon | Contours de la luminosité. |
| Hauteur vers normale | Valeur | Force, Format (DirectX (Source), OpenGL) | Une normal map à partir d'une carte de hauteur. Source utilise la convention DirectX (vert vers le bas), c'est le réglage par défaut. |
| Force de la normale | Normale | Force | Relief plus ou moins marqué. |
| Pixelisation | Couleur | Cases X, Cases Y | De gros pixels carrés. |
| Rendre sans raccord | Couleur | Largeur | Mélange l'image avec une copie décalée de moitié pour qu'elle se répète sans raccord. |

### Procédural

| Nœud | Entrées | Paramètres | Effet |
|---|---|---|---|
| Bruit | UV | Type (Perlin, Simplex, Voronoi, Bruit blanc), Échelle, Octaves, Lacunarité, Gain, Graine, Sortie Voronoi (Distance, Bords, Cellules), Variation | Perlin, Voronoi et le bruit blanc se répètent exactement ; le simplex est aussi rendu répétable. L'échelle est le nombre de cellules sur la largeur de l'image. |
| Damier | UV | Cases X, Cases Y, Couleur A, Couleur B | Un damier ; sorties : la couleur et un masque 0/1. |
| Dégradé | UV | Type (Linéaire, Radial, Angulaire, Losange), Angle | Un dégradé de 0 à 1. |

### Canaux

| Nœud | Entrées | Effet |
|---|---|---|
| Séparer les canaux | Couleur | Sorties R, V, B, A sous forme de nombres. |
| Fusionner les canaux | R, V, B, A | Construit une couleur (par exemple pour ranger trois masques dans une texture). |
| Définir l'alpha | Couleur, Alpha | Remplace l'alpha. |

### Sortie

Ajoutez un nœud **Sortie** par fichier à écrire. Entrées : **Couleur** (rouge, vert, bleu) et **Alpha** (si rien n'est branché sur Alpha, l'alpha de la couleur est utilisé). Paramètres :

| Paramètre | Effet | Par défaut |
|---|---|---|
| **Écrit** | La nature du fichier : **Texture de base** (`$basetexture`), **Normal map** (`$bumpmap`, écrite avec l'indicateur de normal map et un filtre de mipmaps adapté), **Masque de reflets** (`$envmapmask`, et `$envmap env_cubemap` est ajouté), **Masque d'auto-illumination** (`$selfillum 1` et `$selfillummask`), **Autre fichier** (VTF seulement). | Texture de base |
| **Suffixe du fichier** | Ajouté au nom. Vide : la valeur du type, soit rien, `_normal`, `_envmask`, `_illum`, `_custom`. | vide |
| **Format** | **Automatique** (DXT1 si opaque, DXT5 sinon ; les normal maps sont écrites sans compression), **DXT1**, **DXT5**, **BGRA8888**, **BGR888**. | Automatique |
| **Mipmaps** | Écrit des mipmaps. | activé |
| **Bords bloqués** | Active les indicateurs clamp S et T (sprites et decals). | désactivé |

Deux sorties ne peuvent pas écrire le même fichier : c'est une erreur dans **Problèmes**.

## Écrire dans le jeu

La section **Écrire dans le jeu** :

| Option | Effet | Par défaut |
|---|---|---|
| **Destination** | Le dossier de textures personnalisé qui reçoit les fichiers (voir [Dossiers de textures personnalisés](custom-texture-folders.md)). | premier dossier |
| **Nom** | Nom de la texture : lettres, chiffres, `_` et `-` (le reste devient `_`). | nom de l'image |
| **Taille** | **Automatique** (la taille de la première image du graphe, 512 s'il n'y en a pas), ou de 64 à 2048 pixels sur le grand côté. Les côtés des VTF sont arrondis à des puissances de deux. | Automatique |
| **Propriété de surface** | `$surfaceprop` du VMT. | **Aucun** |
| **Écraser les fichiers existants** | Remplace les fichiers déjà présents (sinon l'écriture est refusée). | désactivé |
| **Enregistrer le graphe à côté des textures** | Écrit aussi `nom.hpshader` dans le même dossier, pour que l'ouverture du matériau depuis le navigateur de textures redonne le graphe. | activé |
| **Écrire les textures** | Calcule à pleine taille dans un fil d'exécution séparé et écrit. | |

La liste sous les options montre les fichiers VTF qui seront écrits. Le VMT est créé pour la sortie **Texture de base** : shader `LightmappedGeneric`, `$basetexture`, et `$alphatest 1` si son alpha est uniquement opaque ou transparent, ou `$translucent 1` s'il a de la transparence partielle, comme pour l'[import d'images](custom-texture-folders.md) ; plus `$bumpmap`, `$envmapmask` et `$selfillummask` quand de telles sorties existent. Sans sortie de base, seuls les VTF sont écrits. Quand l'écriture réussit, le matériau devient le matériau courant du navigateur de textures.

Les graphes à côté des textures sont de petits fichiers JSON. Ils ne sont pas intégrés au BSP, mais un outil qui copie un dossier personnalisé entier dans un addon Workshop peut refuser des fichiers qu'il ne connaît pas ; désactivez l'option, ou déplacez les fichiers `.hpshader` ailleurs, pour de tels dossiers.

## Enregistrer et charger des graphes

Les graphes sont des fichiers `.hpshader`, du JSON simple (format version 1) : les nœuds avec leurs paramètres et positions, et les fils. **Enregistrer sous...** choisit l'emplacement (le dossier du projet, à côté de la texture, où vous voulez) ; **Enregistrer** enregistre à nouveau. Les chemins d'images situées dans le dossier du graphe sont enregistrés en relatif, ce qui permet de déplacer un graphe et ses images ensemble. **Ouvrir...** charge un graphe ; un fichier abîmé est réparé autant que possible (nœuds inconnus, fils cassés et boucles sont retirés et listés dans la console) et un fichier qui n'est pas un graphe est refusé.

Le titre de la fenêtre indique le fichier et `*` quand des modifications ne sont pas enregistrées.

## Préréglages

**Nouveau** liste des graphes de départ (ils utilisent l'image de votre graphe actuel s'il en a une) :

| Préréglage | Ce qu'il fabrique |
|---|---|
| Image vers texture de base | Un nœud Texture vers une sortie de texture de base. |
| Teinte | Texture, Teinte, sortie. |
| Normal map depuis la hauteur | Une texture de base, et une normal map calculée depuis la luminosité floutée. |
| Mélange de tuiles | L'image mélangée avec une copie tournée, décalée et recolorée d'elle-même par un masque de bruit : casse la répétition. |
| Tuile sans raccord | Texture, Rendre sans raccord, sortie. |
| Étalonnage | Teinte et saturation, luminosité et contraste, niveaux. |
| Saleté | Bruit et bruit de Voronoi passés dans une rampe de couleurs, incrustés sur l'image. |
| Jeu de matériau | Une texture de base, une normal map et un masque de reflets depuis une image. |
| Graphe vide | Rien. |

Choisir un préréglage remplace le graphe ; **Annuler** rend l'ancien.

## Limites et robustesse

- 512 nœuds et 2048 fils au plus par graphe ; les images sont écrites à 2048 pixels au plus sur le grand côté.
- Une image absente ou illisible affiche un avertissement sur son nœud et compte comme grise ; un fil manquant utilise la valeur par défaut de l'entrée ; une boucle est coupée au fil qui la ferme et signalée. Le graphe ne cesse jamais d'être évalué.
- Les couleurs sont mélangées telles qu'elles sont stockées dans les images (sans conversion gamma), comme dans la plupart des éditeurs d'images.

## Astuces

- Branchez le nœud **Hauteur vers normale** sur une copie en niveaux de gris légèrement floutée de la texture ; augmentez **Force** jusqu'à voir le relief dans la répétition 3x3.
- Utilisez **Prévisualiser ce nœud** sur un nœud **Bruit** pour le régler avant de le mélanger.
- Gardez le grand côté à 256 pendant que vous travaillez et vérifiez le rendu final avec la répétition 2x2.
- Utilisez **Fusionner les canaux** pour mettre un masque dans l'alpha de la texture de base : le format DXT5 est choisi automatiquement.

## Pour les assistants IA

L'éditeur est entièrement accessible via [MCP](../../MCP.md) (en anglais) : `list_shader_nodes`, `list_shader_presets`, `create_shader_graph`, `read_shader_graph`, `edit_shader_graph`, `validate_shader_graph`, `render_shader_graph` (renvoie l'image), `bake_shader_graph` et `open_shader_editor`. `live: true` agit sur le graphe ouvert dans la fenêtre. Le MCP ne lit des images et des graphes que dans le dossier de la map et les dossiers de textures personnalisés.

## FAQ

**L'aperçu est gris.** Le nœud Texture n'a pas d'image ou elle ne peut pas être chargée : lisez le point orange et la liste **Problèmes**. Les chemins relatifs sont cherchés à côté du fichier du graphe et à côté de la map.

**Pourquoi la normal map est-elle plate ?** Augmentez **Force**, ou vérifiez que l'entrée de hauteur n'est pas une constante.

**Cela remplace-t-il VTFEdit ou Photoshop ?** Non : c'est fait pour des variations procédurales et des retouches rapides ; il écrit des fichiers VTF et VMT ordinaires.
