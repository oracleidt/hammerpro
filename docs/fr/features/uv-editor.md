[Documentation](../README.md) > Fonctionnalités > Éditeur UV | [English](../../en/features/uv-editor.md) | [Русский](../../ru/features/uv-editor.md)

# Éditeur UV

L'éditeur UV dessine la face sélectionnée sur sa texture (en texels, texture répétée) et permet de la déplacer, la mettre à l'échelle et la tourner à la souris, comme les éditeurs UV des logiciels 3D. Ses modes Arête et Sommet alignent une arête sur un axe, lui donnent un angle, y ajustent des tuiles ou font glisser des sommets sur la texture. Il déplace aussi les autres couches du matériau (bumpmap, détail, seconde texture de blend, masque d'envmap, masque de blend).

## Ouvrir l'éditeur

L'éditeur UV est la section **Éditeur UV** du panneau **Édition des faces** de l'[outil de texture](texture-tool.md) (`Maj+A`), ouverte par défaut. Sélectionnez une ou plusieurs faces dans la vue 3D pour la remplir.

- **Ouvrir dans une fenêtre séparée** le met dans sa propre fenêtre, comme la fenêtre Compiler et lancer : sortez-la de Hammer Pro ou mettez-la sur un autre écran, où elle profite de toute la place de la fenêtre. Le panneau indique alors **L'éditeur UV est dans sa propre fenêtre.**
- **Remettre dans le panneau** (dans le panneau, ou le bouton de la barre de titre de la fenêtre) le remet en place.

## Comment Source projette les textures

Une face de brush Source n'a pas d'UV par sommet : sa texture est une seule projection plane (axes U et V, échelles, décalages), la même pour un displacement et sa face de base. Chaque geste de l'éditeur change donc cette projection : un déplacement, une rotation, une mise à l'échelle ou une inclinaison, jamais une texture tordue. Survolez **Une projection par face** dans les modes Arête et Sommet pour un rappel.

## Barre d'outils

| Commande | Effet | Par défaut |
|---|---|---|
| **Toutes les faces (n)** | Déplace, met à l'échelle et tourne toutes les faces sélectionnées ensemble (chacune autour de son contour). Désactivé : seulement la face affichée. Visible avec plusieurs faces. | activé |
| `◀` **Face i sur n** `▶` | Affiche la face sélectionnée précédente ou suivante. | |
| **Recadrer** | Ajuste la vue sur la face. | |
| **Face** / **Arête** / **Sommet** | Le mode de sélection (voir plus bas). | **Face** |
| Puces **Couche :** | **Texture de base** et les autres couches du matériau : **Texture de base 2**, **Bumpmap**, **Bumpmap 2**, **Détail**, **Masque d'envmap**, **Masque de blend**. | **Texture de base** |

Navigation sur le canevas : glissez le fond (ou clic droit glissé, ou clic milieu glissé) pour vous déplacer, la molette zoome. Le survol montre ce qu'un clic attraperait (surbrillance, curseur et indication sur le canevas). Les autres faces sélectionnées qui ont la même texture sont dessinées aussi, chacune à la copie de son contour la plus proche de la face modifiée ; cliquez-en une pour la modifier. Chaque glissement fait une étape d'annulation.

## Mode Face

| Geste | Résultat |
|---|---|
| Glisser la face | Déplace son contour sur la texture (la texture reste en place). |
| Glisser un carré de coin ou de bord | La met à l'échelle. `Maj` : les deux axes pareil. |
| Glisser le rond | La tourne par pas de 15 degrés. `Alt` : libre. |
| Cliquer une autre face sélectionnée | En fait la face modifiée. |

Boutons sous le canevas : **Miroir U**, **Miroir V**, `⟲ 90`, `⟳ 90` (quarts de tour) et **Centrer** (met le milieu de la face au milieu de la texture). Avec **Toutes les faces**, ils s'appliquent à toutes les faces sélectionnées. Une ligne indique l'échelle, le décalage et la rotation de la face.

## Mode Arête

| Geste | Résultat |
|---|---|
| Cliquer une arête | La sélectionne. `Maj` : ajouter, `Ctrl` : basculer. Un glissement sur la face ou le fond trace un cadre de sélection. |
| Glisser une arête sélectionnée | Elle suit la souris, les sommets éloignés restent. |
| Glisser le rond | Tourne la texture autour du milieu de l'arête principale (la dernière choisie), par pas de 15 degrés (`Alt` : libre). |
| Double clic sur une arête | L'aligne sur l'axe le plus proche. |
| `X` / `Y` (souris sur le canevas), ou les petits boutons **X** et **Y** à côté de l'arête | Tourne la texture pour que l'arête soit sur cet axe (le plus petit tour, l'échelle gardée). |
| Flèches | Décalent la sélection d'un texel (`Maj` : 8 texels). |
| `Échap` | Vide la sélection. |

Avec une arête sélectionnée, le panneau propose **Aligner sur l'axe X**, **Aligner sur l'axe Y**, un champ **Angle** (direction de l'arête principale en degrés depuis l'axe X, sens horaire avec V vers le bas, de -180 à 180 ; le changer tourne la texture autour du milieu de l'arête), les boutons `⟲ 15`, `⟳ 15`, `⟲ 90`, `⟳ 90`, et **Ajuster le long de l'arête** avec un nombre de tuiles (0,25 à 64, 1 par défaut) : l'arête est alignée sur son axe le plus proche et la texture mise à l'échelle pour que ce nombre de tuiles la couvre, un coin de tuile à son début. Avec **Toutes les faces**, les autres faces utilisent leur arête la plus parallèle à l'arête principale.

## Mode Sommet

| Geste | Résultat |
|---|---|
| Cliquer un sommet | Le sélectionne. `Maj` : ajouter, `Ctrl` : basculer, un cadre en prend plusieurs. |
| Glisser la sélection | Les sommets choisis suivent la souris sur la texture (aimantés aux lignes des tuiles, `Alt` : libre) ; les sommets encadrés restent, les autres bougent le moins possible. |
| Double clic sur un sommet | Y place un coin de tuile. |
| Flèches | Décalent d'un texel (`Maj` : 8 texels). |
| `Échap` | Vide la sélection. `Suppr` ne fait rien (une face de brush garde ses sommets). |

**Coin de tuile sur le sommet** décale la texture pour qu'un coin de tuile tombe sur le dernier sommet choisi (avec **Toutes les faces**, sur les faces qui le partagent). Le panneau affiche **N sommets sélectionnés** (ou **N arêtes sélectionnées**) et **Vider**.

Comme toute la face partage une seule projection, trois coins d'une boîte ne peuvent pas rester en place pendant que le quatrième bouge : vous obtenez la projection la plus proche de votre geste.

## Les autres couches du matériau

Choisissez une couche dans **Couche :** pour la modifier sur le même canevas. Le canevas montre cette texture posée sur la face par la projection de la face et la transformation de la couche, avec **Texture de base dessous** (activé par défaut) et un curseur d'opacité de la texture de base. Les mêmes gestes changent alors la transformation de la couche (`$bumptransform`, `$detailtexturetransform`, `$envmapmasktransform`...) au lieu de la face.

| Commande | Effet |
|---|---|
| **Échelle**, **Décalage**, **Rotation**, **Centre** (U et V) | Les valeurs de la transformation de la couche. |
| **Échelle du détail** | `$detailscale`, pour la couche **Détail** : le détail se répète autant de fois par texture de base. |
| **Miroir U**, **Miroir V**, `⟲ 90`, `⟳ 90`, **Centrer**, **Ajuster à la face**, **Réinitialiser** | Opérations rapides sur la couche : miroir, quart de tour, milieu de la face au milieu d'une tuile, une tuile sur la face, ou aucune transformation. |
| **Annuler**, **Rétablir** | Les modifications de couche ont leur propre historique tant qu'elles ne sont pas appliquées. |
| **Revenir au fichier** | Revient aux valeurs du fichier du matériau. |
| **Appliquer à ces faces (n)** | Écrit une variante du matériau (un `patch`) dans votre premier dossier personnalisé et la donne à ces faces seulement (une étape d'annulation). Elle est empaquetée dans la map à la compilation. |
| **Appliquer au matériau** | Modifie le VMT du matériau : toutes les faces qui l'utilisent changent (une sauvegarde du fichier est gardée). |

**Pas encore appliqué** rappelle qu'une modification attend. Une ligne au-dessus des valeurs indique ce que le shader fait de la couche : si le jeu lit cette transformation, l'ignore, ou ne dessine pas du tout la couche (**Pas dessinée par le jeu avec ce matériau**). Les vues 3D ne dessinent pas ces couches (**Texture de base 2** y est dessinée avec la projection de la texture de base, comme dans le jeu) : vérifiez-les ici ou en jeu.

### Les couches que le shader ne déplace pas

Certains shaders dessinent une couche avec la projection de la texture de base et ignorent sa transformation (par exemple la bumpmap et le détail de LightmappedGeneric, ou `$basetexture2` de WorldVertexTransition). Hammer Pro les déplace quand même : appliquer écrit une copie de la texture aux pixels déplacés (normales tournées pour une bumpmap) dans votre premier dossier personnalisé, et le matériau l'utilise. Les gestes s'alignent alors sur ce qu'une texture peut contenir : quarts de tour, miroirs, nombres de répétitions entiers et tout décalage. Une bumpmap auto-ombrée (`$ssbump`) peut seulement être décalée, répétée et retournée en V.

Appliquer à des faces ou à un matériau demande un dossier de textures personnalisé (**Ajoutez un dossier personnalisé (onglet Textures) pour y mettre les variantes de matériaux.**). Un matériau dans une archive ne peut pas être modifié sur place : copiez-le dans un dossier personnalisé avec l'[éditeur de matériaux](material-editor.md), ou appliquez aux faces.

### Les couches déplacées restent en place

Sur la couche **Texture de base**, **Les couches déplacées restent en place** (activé par défaut) garde où elles sont sur la face les couches que vous avez déplacées ici quand vous bougez la texture de base : elles reçoivent le déplacement inverse, dans la même étape d'annulation. Les couches jamais déplacées suivent la texture de base, comme une bumpmap peinte pour elle.

## Astuces

- En mode **Arête**, `X` ou `Y` redresse une texture sur une face inclinée en une touche.
- **Ajuster le long de l'arête** avec 2 tuiles est le moyen le plus rapide de mettre exactement deux planches le long d'une poutre.
- Détachez l'éditeur sur un second écran et gardez une grande vue 3D.

## FAQ

**Pourquoi bouger un sommet fait bouger les autres ?** Une face Source n'a qu'une projection pour toute la face ; l'éditeur trouve la projection la plus proche de votre geste.

**Pourquoi ne puis-je pas tourner ma bumpmap de 30 degrés ?** Le shader ignore cette transformation, donc Hammer Pro écrit le déplacement dans une texture, qui ne peut contenir que des quarts de tour, des miroirs, des répétitions entières et des décalages.

**Où sont écrites les variantes ?** Dans votre premier dossier de textures personnalisé ; elles sont empaquetées dans la map à la compilation.

**Des scripts ou une IA peuvent-ils piloter l'éditeur UV ?** Oui : les options de l'outil de texture `uv_place`, `uv_all_faces`, `uv_mode`, `uv_layer`, `uv_keep_layers`, `uv_fit_tiles`, `uv_vertices` et `uv_edges` (MCP `set_tool_options`), et les outils MCP ci-dessous.

## Pages liées

- [Outil de texture](texture-tool.md)
- [Éditeur de matériaux](material-editor.md)
- [Dossiers de textures personnalisés](custom-texture-folders.md)
- [Cadres de fenêtres](window-frames.md)
- [Référence des outils MCP](../../MCP.md) (en anglais : `align_texture_edge`, `move_texture_vertices`, `set_texture_origin`, `get_uv_layers`, `edit_uv_layer`, `set_tool_options`, `screenshot_window` avec `uv_editor`)

---
[Retour à l'index de la documentation](../README.md)
