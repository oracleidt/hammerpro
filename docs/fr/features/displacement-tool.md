[Documentation](../README.md) > Fonctionnalités > Outil displacement | [English](../../en/features/displacement-tool.md) | [Русский](../../ru/features/displacement-tool.md)

# Outil displacement

L'outil displacement crée, détruit, sélectionne, sculpte et peint les displacements (le terrain fait à partir de faces de brush). Il reprend tout ce que proposent les panneaux displacement de Hammer (sculpter, peindre l'alpha, modeler, marquages, couture, subdivision, bruit, masques), les ajouts de Hammer++ (mélanges à 4 textures, peinture continue) et quelques fonctions en plus (couture en direct, ligne de vue, projection sur des brushes, ombres projetées).

## Ouvrir l'outil

- **Outils > Outil displacement**, la palette d'outils, ou `Maj+P`.
- Ses options sont dans l'onglet **Displacement** du panneau de droite, qui s'ouvre tout seul quand l'outil est choisi. Les options du panneau sont enregistrées dans les paramètres (`mesh.disp_tool`) et reviennent à la prochaine utilisation.
- La première ligne du panneau indique combien de displacements contient la map.

## Opacité du surlignage de sélection

Dans la section **Sélection**, **Opacité de la sélection** règle le voile des faces sélectionnées de 0 à 100 %. Cliquez sur la valeur numérique pour saisir un pourcentage précis (par exemple 23,7 %). À 0 %, le voile disparaît et les arêtes de la topologie ainsi que les sommets restent visibles. Le surlignage suit la surface du displacement. Ce réglage de l’éditeur est conservé entre les sessions et ne modifie ni les matériaux, ni l’alpha de mélange du displacement, ni la map compilée.


## Modes

La rangée de boutons en haut du panneau choisit ce que font les clics et les glissements dans les vues.

| Mode | Effet |
| --- | --- |
| **Sélection** | Cliquez sur des faces pour les sélectionner (`Ctrl` inverse, `Maj` inverse tous les displacements du solide). Maintenez et balayez pour sélectionner toutes les faces survolées |
| **Sculpter** (par défaut) | Élève, abaisse, lisse ou fixe la hauteur de la surface au pinceau |
| **Peindre l'alpha** | Peint le mélange entre les textures d'un matériau de fusion |
| **Modeler** | Pousse la surface jusqu'à une couche, creuse un profil le long du trait, la projette sur des brushes, ou la pince, la saisit, la gonfle et l'aplatit |
| **Marquages** | Peint les marquages des triangles : praticable forcé, constructible forcé ou retiré |

## Souris et clavier

On peint, on modèle et on marque dans la vue 3D. Un cercle de pinceau suit la surface sous la souris.

| Geste | Effet |
| --- | --- |
| Maintenir le bouton gauche sur des displacements (vue 3D) | Peint. La peinture démarre dès que la souris bouge de 3 points ou que le bouton est maintenu 0,2 s. Un trait = une étape d'annulation |
| Clic bref (vue 3D) | Sélectionne la face sous la souris (`Ctrl` l'ajoute ou la retire) |
| Clic (vues 2D) | Sélectionne les displacements du solide sous la souris |
| `Maj` en peignant (sculpter, alpha, marquages) | Inverse l'effet : Élever abaisse, Abaisser élève, Ajouter l'alpha retire, Forcer oui devient Forcer non |
| `Ctrl` en modelant | Inverse : Pousser enfonce, Creuser élève une crête |
| `Maj` en modelant | Lisse à la place (intensité : **Lissage**) |
| Glisser avec le clic droit (vue 3D, modes de peinture) | Change le rayon du pinceau : 150 points vers la droite le doublent, vers la gauche le divisent par deux |
| `Alt` + glisser avec le clic gauche (vue 3D, modes de peinture) | Même changement de rayon (geste de Hammer) |
| `Alt` + clic droit sur une face (vue 3D) | Règle l'axe **Personnalisé** sur la normale de la surface à cet endroit et passe l'axe en **Personnalisé** |
| `Échap` | Annule le trait en cours (ses changements sont défaits) ; sinon vide la sélection de faces |

La couleur du cercle change selon l'effet, et il devient gris avec la mention « Non sélectionné : pas peint » au-dessus des displacements qu'il n'a pas le droit de peindre (voir **Sélection seulement**). La barre d'état rappelle toujours les gestes du mode actif.

## Boucles d'arêtes de la grille

`Alt`+clic sur une ligne de la grille d'un displacement (dans la vue 3D ou une vue 2D) sélectionne sa rangée ou sa colonne, la ligne de sommets qui continue tout droit dans la grille. La boucle continue à travers les displacements voisins soudés (jusqu'à 1024 connectés), aux endroits où quatre displacements se rencontrent et à travers des displacements d'autres puissances ; elle s'arrête au bord du terrain, et elle est fermée quand elle revient à son début. La boucle est dessinée en bleu avec un point sur chaque sommet, et la barre d'état compte ses sommets et les displacements qu'elle traverse. `Échap` l'efface. Dans les modes de peinture de la vue 3D, un `Alt`+clic court sélectionne la boucle et `Alt`+glisser change toujours le rayon du pinceau.

Section **Boucle d'arêtes** du panneau (et la palette de commandes et MCP) :

| Option ou bouton | Ce qu'il fait | Défaut ou plage |
| --- | --- | --- |
| **Force**, **Passes** | De combien, à chaque passe, **Lisser la boucle du displacement** rapproche chaque sommet de la moyenne de ses deux voisins sur la boucle | 0,5 et 1 passe (0 à 1, 1 à 64) |
| **Lisser la boucle du displacement** | Lisse la boucle le long de sa propre ligne, selon la normale de la face : les sommets aux extrémités d'une boucle ouverte restent, les voisins soudés suivent et restent soudés, une seule étape d'annulation | |
| **Hauteur**, **Fixer la hauteur de la boucle du displacement** | Met chaque sommet de la boucle à cette hauteur au-dessus de la face, comme **Élever à** sur la boucle seulement (-16384 à 16384), une seule étape d'annulation | 0 |
| **Effacer la boucle** | Oublie la boucle (`Échap`) | |

MCP : `select_displacement_loop` (face, row et col d'un sommet, direction `row` ou `column`) donne les sommets de la boucle, `smooth_displacement_loop` et `set_displacement_loop_height` la modifient, et les options `loop_height`, `loop_strength` et `loop_passes` de l'outil displacement sont listées par `get_tool_options`. Les actions sont `disp_smooth_loop` et `disp_loop_height`.

## Créer

Section **Créer** du panneau :

- Les boutons de **Puissance** **2**, **3** et **4** transforment les cibles en displacements de cette puissance : 5 x 5, 9 x 9 ou 17 x 17 sommets.
- Cibles : les faces sélectionnées, ou, si aucune face n'est sélectionnée, les faces tournées vers le haut des solides sélectionnés (normale Z supérieure à 0,7). Une ligne indique lesquelles (« S'applique à N face(s) sélectionnée(s) » ou « S'applique à N face(s) supérieure(s) des solides sélectionnés »).
- Seules les faces à 4 côtés peuvent devenir des displacements ; les autres sont ignorées avec un message.

Les faces sélectionnées avec l'[outil texture](texture-tool.md) peuvent aussi être transformées en displacements depuis cet outil.

## Sélection

Section **Sélection** : affiche « N displacement(s) parmi M face(s) sélectionnée(s) ».

| Bouton ou option | Effet |
| --- | --- |
| **Sélectionner tous les displacements** | Sélectionne toutes les faces displacement de la map |
| **Effacer** | Désélectionne toutes les faces (`Échap`) |
| **Sélectionner les voisins** | Ajoute les displacements qui partagent une arête avec ceux sélectionnés |
| **Des solides** | Sélectionne tous les displacements des solides sélectionnés |
| **Masques : Grille** | Masque la grille de triangles des displacements sélectionnés (masque Grid de Hammer). Désactivé par défaut |
| **Masques : Sélection** | Ne teinte pas les displacements sélectionnés (masque Select de Hammer). Désactivé par défaut |
| **Détruire** | Retransforme les displacements sélectionnés (ou ceux des solides sélectionnés) en faces planes |
| **Coudre** | Referme les fissures entre les displacements sélectionnés (au moins deux) : les arêtes et coins partagés deviennent identiques, les arêtes entre puissances différentes suivent la plus grossière |

Quand des displacements sont sélectionnés, des options par face apparaissent :

| Option | Effet | Par défaut ou plage |
| --- | --- | --- |
| **Puissance** | Change la puissance (2, 3 ou 4) des displacements sélectionnés. Avec **Garder la forme** activé, la sculpture est conservée ; désactivé, ils sont reconstruits à plat et la sculpture est perdue | Puissance actuelle, ou « (valeurs multiples) » |
| **Garder la forme** (à côté de **Puissance**) | Rééchantillonne la grille existante à la nouvelle puissance au lieu de reconstruire à plat : décalages, distances, normales, alpha, rangées à 4 textures et marquages de triangles. Monter interpole (bicubique pour les hauteurs et décalages, bilinéaire pour le reste), descendre garde les sommets qui restent sur la grille plus grossière. Les drapeaux et sommets autorisés sont conservés, et les displacements sélectionnés sont cousus ensuite, en une seule étape d'annulation (option `keep_shape` de `mesh.disp_tool`) | Activé |
| **Élévation** | Décalage de toute la surface le long de la normale de la face | -4096 à 4096 |
| **Échelle de lightmap** | Unités par luxel des displacements sélectionnés (plus bas : éclairage plus fin, map plus lourde) | 1 à 1024, 16 affiché pour une sélection mixte |
| **Aplanir** | Remet à plat les displacements sélectionnés en gardant leur puissance | |

Chaque displacement sélectionné affiche la teinte de sélection et sa grille de triangles dans les vues (les masques cachent l'une ou l'autre). Les triangles marqués retirés sont teintés dans la vue 3D.

## Sculpter la géométrie

Section **Sculpter la géométrie** (mode **Sculpter**) :

| Option | Effet | Par défaut |
| --- | --- | --- |
| **Élever** | Pousse la surface le long de l'axe de peinture | Choisi |
| **Abaisser** | Enfonce la surface le long de l'axe de peinture | |
| **Lisser** | Égalise les hauteurs des sommets voisins le long de l'axe de peinture (les displacements connectés sont lissés ensemble et restent cousus) | |
| **Élever à** | Amène la surface à une hauteur cible au-dessus de la face | |
| **Intensité** (Élever, Abaisser) | Unités déplacées par seconde au centre du pinceau (1 à 8192) | 64 /s |
| **Intensité** (Lisser) | Vitesse de lissage (0 à 1) | 0,5 |
| **Hauteur cible** (Élever à) | Hauteur au-dessus de la face le long de l'axe de peinture (-16384 à 16384) | 0 |
| **Axe** | Direction de déplacement des sommets (voir ci-dessous) | Normale de la face |

### Axe de peinture

| Axe | Direction |
| --- | --- |
| **Normale de la face** | La normale de la face de base (par défaut) |
| **Normale de la surface** | La normale de la surface à chaque sommet (seulement pour sculpter la géométrie) |
| **Centre du pinceau** | La normale de la surface sous le centre du pinceau (Brush Center de Hammer) |
| **Vue (caméra)** | Vers la caméra 3D, selon la direction de vue au début du trait (direction Écran de Hammer). Inversé : en s'en éloignant |
| **Vue (caméra) horizontale** | Vers la caméra 3D sur X et Y seulement : la direction de vue au début du trait, mise à plat sur le sol, donc les sommets ne montent ni ne descendent jamais. Pratique pour pousser une falaise ou un mur en avant ou en arrière depuis l'endroit où vous regardez. En regardant droit vers le bas, le bas de la vue donne la direction. Inversé : en s'éloignant de la caméra |
| **X**, **Y**, **Z (haut)** | Un axe du monde |
| **Personnalisé** | Une direction choisie : saisissez X, Y, Z sous la liste, ou faites `Alt` + clic droit sur une face dans la vue 3D pour prendre sa normale (direction Selected de Hammer). Par défaut 0 0 1 |

## Peindre l'alpha

Section **Peindre l'alpha** (mode **Peindre l'alpha**) : peint le mélange des matériaux de fusion (WorldVertexTransition : l'alpha 0 montre la première texture, 255 la seconde).

| Option | Effet | Par défaut |
| --- | --- | --- |
| **Texture : 2 textures** | Peint l'alpha entre les deux textures | Choisi |
| **Texture : 1**, **2**, **3**, **4** | Matériaux de mélange à 4 textures (Lightmapped_4WayBlend) : peint cette texture, les autres s'effacent en proportion (`Maj` ou **Retirer** la diminue). Des points colorés montrent les poids sous le pinceau | |
| **Ajouter** | Révèle la seconde texture | Choisi |
| **Retirer** | Révèle la première texture | |
| **Valeur fixe** | Mélange vers une valeur d'alpha fixe | |
| **Lisser** | Égalise le mélange des sommets voisins (les displacements connectés sont lissés ensemble) | |
| **Intensité** (Ajouter, Retirer) | Unités d'alpha peintes par seconde au centre du pinceau (1 à 16384) | 255 /s |
| **Valeur** (Valeur fixe) | Alpha cible, de 0 à 255 | 255 |
| **Intensité** (Lisser) | Vitesse à laquelle le mélange s'égalise (0 à 1) | 0,5 |

La rangée **Texture** n'apparaît que si **Peindre les mélanges à 4 textures** est activé dans les paramètres (activé par défaut, comme Hammer++ pour Garry's Mod).

### Mélanges à 4 textures (Lightmapped_4WayBlend)

- **Où le mélange est rangé.** Les 4 boutons de texture écrivent les rangées `multiblend`, `alphablend` et `multiblend_color_0` à `multiblend_color_3` du `dispinfo`, comme Hammer++ et le Hammer de CS:GO. Une map faite dans Hammer++ s'ouvre avec ses mélanges, et l'enregistrement garde les rangées telles qu'elles ont été lues tant que vous ne les peignez pas.
- **Ce que vous voyez.** Les vues texturée, éclairée et précalculée et la vue joueur dessinent les 4 textures comme le jeu : la texture 2, puis la 3, puis la 4 se posent sur la texture 1, chacune selon sa propre valeur des rangées. Les paramètres du VMT dessinent les bords : `$textureN_lumstart` et `$textureN_lumend` (luminance de chaque texture), `$lumblendfactorN`, `$textureN_blendstart` et `$textureN_blendend` (netteté), `$textureN_uvscale` (l'échelle de chaque texture) et `$textureN_blendmode` (1 : modulation 2x).
- **Pas encore de rangées.** Un displacement avec un matériau Lightmapped_4WayBlend et sans rangées à 4 textures montre son alpha de sommet comme texture 2 (la peinture **2 textures**), et le premier trait sur un bouton de texture part de cet alpha. Le compilateur écrit la même chose, donc le jeu montre ce que montre l'éditeur. Le VBSP de Valve ignore l'alpha d'un tel displacement (le jeu ne montre alors que la texture 1).
- **Dans le jeu.** Le compilateur écrit les rangées dans le lump multiblend des displacements du BSP et marque chaque displacement qui en a, octet pour octet comme le VBSP de Garry's Mod : les mélanges s'affichent dans Garry's Mod et les jeux Source 2013.
- **Déplacer et retourner** un displacement garde ses rangées à 4 textures à leur place, comme son alpha. Aplanir repart de la texture 1 ; changer la puissance avec **Garder la forme** rééchantillonne les rangées.

## Modeler

Section **Modelage** (mode **Modeler**), l'équivalent du panneau Sculpt de Hammer :

- **Pousser** : élève la surface jusqu'à une couche décalée de sa position au début du trait (`Ctrl` l'enfonce). Repasser au même endroit ne cumule pas.
- **Creuser** : le trait est un chemin, la surface s'enfonce le long de celui-ci selon un profil en travers : fossés, rivières, routes (`Ctrl` élève une crête).
- **Projeter** : déplace la surface sur les brushes sélectionnés le long de l'axe de modelage (une route ou des fondations que le terrain doit épouser). Sélectionnez les brushes en même temps que les displacements ; si aucun brush n'est sélectionné, l'outil vous le dit.
- **Pincer** : tire les sommets sous le pinceau vers le centre du trait, dans le plan de surface de l'axe de modelage (`Ctrl` les repousse). Vitesse : **Intensité**, en unités par seconde.
- **Saisir** : prend les sommets sous le pinceau au début du trait et les entraîne avec le mouvement de la souris, pondérés par l'atténuation, pendant tout le trait même si la souris quitte le pinceau (`Ctrl` entraîne à l'opposé). Le déplacement suit l'écran comme la saisie de Blender : les sommets bougent dans le plan face à la caméra à la profondeur du point saisi, donc monter la souris soulève le terrain et la bouger sur le côté le tire sur le côté, quelle que soit la surface sous le curseur. Cela marche aussi quand la souris quitte le terrain.
- **Gonfler** : déplace chaque sommet le long de sa propre normale de surface, proportionnellement à l'atténuation (`Ctrl` dégonfle). Vitesse : **Intensité**, en unités par seconde.
- **Aplanir** : rapproche les sommets du plan moyen des sommets sous le pinceau, un vrai plan ajusté sur eux et non une hauteur (`Ctrl` les éloigne du plan).

Les quatre utilisent le rayon et l'atténuation du pinceau, comptent pour une seule étape d'annulation par trait et suivent **Coudre en peignant** comme les autres pinceaux de modelage.

| Option | Pour | Effet | Par défaut |
| --- | --- | --- | --- |
| **Limite : Atténuée** | Pousser, Creuser | Chaque sommet va à son décalage une fois par trait | Choisi |
| **Limite : Additive** | Pousser, Creuser | Le décalage continue de s'ajouter tant que le pinceau reste sur la surface | |
| **Début de l'atténuation** | Pousser | Part du rayon, depuis le centre, qui reçoit tout le décalage avant le début de l'atténuation (0 à 100 %) | 0 % |
| **Fin de l'atténuation** | Pousser | Part du décalage sous laquelle l'atténuation ne descend jamais dans le pinceau (0 à 100 %) | 0 % |
| **Décalage : Absolu** | Pousser | Le décalage est une distance (**Distance**, 0 à 4096 unités) | Choisi, 32 |
| **Décalage : Adaptatif** | Pousser | Le décalage est un pourcentage du rayon du pinceau (**Pourcentage**, 0 à 400 %) | 25 % |
| **Profondeur** | Creuser | Profondeur au point le plus bas du profil (0 à 4096) | 32 |
| **Profil** | Creuser | Cinq curseurs : profondeur en travers du trait à 0, 25, 50, 75 et 100 % du rayon. Profils prédéfinis **V**, **U** et **\|_\|** | U |
| **Intensité** | Pincer, Gonfler | Unités déplacées par seconde au centre du pinceau (1 à 8192) | 64 /s |
| **Intensité** | Aplanir | Part de la distance au plan comblée à chaque pas (0 à 1) | 0,5 |
| **Plan : Moyen** | Aplanir | Le plan est ajusté de nouveau sur les sommets sous le pinceau à chaque pas | Choisi |
| **Plan : Fixe** | Aplanir | Le plan est pris une seule fois, au début du trait, et tout le trait aplanit vers lui | |
| **Portée** | Projeter | Les brushes plus éloignés que cette distance de la surface sont ignorés (1 à 16384) | 512 |
| **Axe** | Tous | Direction du modelage (même liste que pour sculpter, sans Normale de la surface) | Normale de la face |
| **Lissage** | Tous | Intensité du lissage appliqué en maintenant `Maj` (0 à 1) | 0,5 |

## Marquages

Section **Peindre les marquages** (mode **Marquages**), l'équivalent de Tag Walkable, Tag Buildable et Tag Remove de Hammer. Les marquages sont écrits dans la map et utilisés par le compilateur.

| Option | Effet | Par défaut |
| --- | --- | --- |
| **Praticable** | Si les joueurs peuvent marcher sur les triangles (normalement selon leur pente) | Choisi |
| **Constructible** | Si des objets peuvent être construits sur les triangles (normalement selon leur pente) | |
| **Retirer** | Marque des triangles comme retirés (écrit dans la map ; le moteur de Garry's Mod l'ignore) | |
| **Forcer oui** / **Forcer non** | Le marquage est actif (ou inactif) quelle que soit la pente ; `Maj` peint l'inverse | Forcer oui |
| **Selon la pente** | Le marquage suit de nouveau la pente | |
| **Retirer** / **Restaurer** (avec le marquage Retirer) | Marque les triangles comme retirés, ou efface la marque ; `Maj` peint l'inverse | |
| **Afficher la praticabilité** | Colore les triangles des displacements selon leur praticabilité (même réglage que **Affichage > Options d'affichage > Praticabilité des displacements**) | Désactivé |
| **Appliquer à la sélection** | Donne le marquage choisi à tous les triangles des displacements sélectionnés | |

## Pinceau

Section **Pinceau**, présente dans tous les modes sauf **Sélection** :

| Option | Effet | Par défaut |
| --- | --- | --- |
| **Rayon** | Rayon du pinceau en unités du monde (curseur de 1 à 2048 ; le glisser au clic droit ou avec `Alt` monte jusqu'à 8192) | 64 |
| **Atténuation** | **Constante**, **Linéaire** ou **Douce** | Douce |
| **Spatial** | Mesure la distance du pinceau en 3D sur la surface au lieu du plan de la face (peint les murs et d'une dalle à l'autre). Désactivé : seuls le displacement sous le pinceau et les dalles de terrain qui lui sont reliées sont peints | Désactivé |
| **Ligne de vue** | Ne peint que les displacements visibles par la caméra sous le pinceau (pas le terrain caché derrière une colline ou sous un pont) | Désactivé |
| **Coudre en peignant** | Les displacements voisins suivent le pinceau en direct, les arêtes et coins partagés bougent ensemble, et les fissures restantes sont refermées à la fin du trait (même étape d'annulation). Avec des displacements sélectionnés, seuls eux changent : leurs bords restent collés aux non sélectionnés | Désactivé |
| **Sélection seulement** | Avec des displacements sélectionnés, seuls ceux-ci sont peints (Hammer). Désactivé : tous ceux sous le pinceau | Activé |
| **Continu** | Maintenir le bouton immobile continue de peindre (Hammer++). Désactivé : peint seulement quand la souris bouge | Activé |

## Outils

Section **Outils**, en bas du panneau. Ces commandes agissent sur les displacements sélectionnés, ou sur tous ceux des solides sélectionnés. Chacune est une étape d'annulation.

| Option | Effet | Par défaut |
| --- | --- | --- |
| **Sans collision physique** | Les objets physiques (props, ragdolls) traversent ces displacements | Désactivé |
| **Sans collision des personnages** | Les joueurs et les PNJ traversent ces displacements | Désactivé |
| **Sans collision des tirs** | Les balles et les traces traversent ces displacements | Désactivé |
| **Projeter les ombres** | Les displacements projettent des ombres quand la map est éclairée. Désactivé : ils restent éclairés mais la lumière les traverse (compilateur de Hammer Pro) | Activé |
| **Échelle** + **Appliquer** | Multiplie les hauteurs par ce facteur (-16 à 16 ; un facteur négatif les retourne) | 2 |
| **Inverser l'alpha** | Échange les deux textures du mélange : chaque valeur d'alpha a devient 255 - a | |
| **Subdiviser** | Lisse les displacements comme une seule surface de subdivision (Catmull-Clark), puis les coud. Les bords extérieurs restent en place | |
| **Minimum** / **Maximum** | Plage des hauteurs aléatoires ajoutées par **Ajouter du bruit** (enregistrée dans `mesh.noise_min` et `mesh.noise_max`) | -4 / 4 |
| **Bruit : Aléatoire** / **Fractal** | Type de bruit qu'ajoute **Ajouter du bruit** (voir [Bruit et érosion](#bruit-et-érosion)) | Aléatoire |
| **Ajouter du bruit** | Ajoute une hauteur entre le minimum et le maximum à chaque sommet, le long de la normale de la face : aléatoire, ou de Perlin fractal | |
| **Érosion thermique**, **Érosion hydraulique** | Usent le terrain comme le temps ; leurs options sont sous **Thermique** et **Hydraulique** (voir [Bruit et érosion](#bruit-et-érosion)) | |
| **En maillage...** | Enregistre les displacements en fichier OBJ, voir [Displacement en maillage](#displacement-en-maillage) | |
| **Maillage en displacement...**, **Plaquer sur un maillage...**, **Daller la sélection...** | Créent ou modèlent des displacements à partir d'un maillage 3D, voir [Maillage en displacement](#maillage-en-displacement) | |
| **Mètres** / **Z haut** | Unités et axe vertical de ce fichier (enregistrés dans `mesh.mesh_export_meters` et `mesh.mesh_export_z_up`) | Désactivé / Désactivé |

Une case affiche un état mixte quand la sélection n'est pas homogène ; un clic l'active pour tous.

Les terrains entiers viennent de la page [Terrain](terrain.md) : **Outils > Importer une carte de hauteur...**, **Générer un terrain...** et **Exporter une carte de hauteur...** construisent, remplacent et exportent des grilles de displacements cousus.

Les mêmes commandes sont dans le menu **Outils** et marchent quel que soit l'outil actif : **Subdiviser les displacements**, **Inverser l'alpha des displacements** et **Ajouter du bruit aux displacements...** (une petite fenêtre **Bruit sur les displacements** avec le choix **Aléatoire** ou **Fractal**, la plage, les options fractales et un bouton **Appliquer**), ainsi que **Bruit fractal sur les displacements...** (la même fenêtre avec **Fractal** choisi), **Érosion thermique des displacements...** et **Érosion hydraulique des displacements...** (petites fenêtres avec les options de cette érosion et un bouton **Appliquer**). Les fenêtres utilisent les mêmes options que la section **Outils** du panneau : une valeur changée dans l'une l'est dans l'autre. Aucune n'a de raccourci par défaut ; toutes sont dans la palette de commandes et dans l'outil MCP `run_action` (`disp_noise`, `disp_fractal_noise`, `disp_thermal_erosion`, `disp_hydraulic_erosion`).

### Displacement en maillage

Enregistre les displacements sélectionnés (ou tous ceux des solides sélectionnés) en maillage modifiable dans Blender ou tout outil 3D. Utilisez le bouton **En maillage...** de la section **Outils**, **Outils > Displacement en maillage...**, ou l'outil MCP `displacement_to_mesh`. La commande est disponible quand au moins un displacement est sélectionné.

- Une boîte de dialogue demande où écrire le `.obj` (nom proposé : `<map>_displacements.obj`, à côté de la map). Le `.mtl` est écrit à côté.
- Le fichier contient les sommets déplacés, leurs normales, les UV calculés depuis le mappage de texture de chaque face, et un matériau par texture. Les textures d'outil sont ignorées. C'est le même code que **Fichier > Exporter en OBJ**.
- **Mètres** écrit des mètres (1 unité = 0,01905 m) au lieu des unités Source. **Z haut** garde les axes de l'éditeur ; désactivé, le fichier est Y vers le haut (réglage par défaut de l'import OBJ de Blender).
- Rien ne change dans la map et aucune étape d'annulation n'est créée. Exporter en OBJ et Exporter à nouveau ne sont pas modifiés.
- MCP : `displacement_to_mesh` accepte `faces`, `ids` ou `solid_ids` (par défaut les displacements sélectionnés), `path` (chemin complet ou relatif au dossier de la map ; par défaut `<map>_displacements.obj`), `units` (`source` ou `meters`), `up` (`y` ou `z`) et `overwrite`.

### Maillage en displacement

Transforme un maillage 3D en displacements : Hammer Pro pose une grille de dalles de displacement sur le maillage, mesure la hauteur du maillage à chaque sommet de displacement avec un rayon, et met les sommets à cette hauteur. Utilisez **Outils > Maillage en displacement...**, le bouton **Maillage en displacement...** de la section **Outils**, la palette de commandes, ou l'outil MCP `mesh_to_displacement`.

Le maillage peut être :

- un fichier : OBJ, FBX, glTF ou GLB (les mêmes lecteurs que l'[import de modèle](model-import.md)), choisi dans une boîte de dialogue ;
- un prop de la map : sélectionnez un `prop_static` ou `prop_physics` et cliquez sur **Utiliser le prop sélectionné**. Sa position, ses angles et son échelle sont appliqués ;
- un modèle du jeu : tapez un chemin comme `models/props_c17/oildrum001.mdl` et cliquez sur **Charger le .mdl**. Il est placé à l'origine.

La fenêtre **Maillage en displacement** montre les options et, dans la vue 3D et les vues 2D, un aperçu en direct de la disposition : contours cyan des dalles sur le plan de base, lignes jaunes sur la surface mesurée, lignes vertes pour les jupes et le fond. Sous les options, elle indique le nombre de dalles, la puissance et l'espacement des sommets du résultat, et avertit des surplombs. **Créer les dalles** fait tout en une seule étape d'annulation (**Maillage en displacement**) ; les nouveaux brushes sont groupés et sélectionnés.

| Option | Effet | Par défaut |
| --- | --- | --- |
| **Projection** | Direction des rayons : **Z (vue de dessus)**, **X**, **Y**, ou **Meilleur choix**, qui prend l'axe vers lequel le maillage est le plus tourné. **Depuis l'autre côté** projette contre l'axe (un plafond, ou un mur vu de l'arrière) | Z |
| **Taille des dalles** | Largeur d'une dalle, de 16 à 4096 unités. Les dalles sont posées sur une grille de multiples de cette taille | 256 |
| **Puissance** | 2, 3 ou 4, ou **Auto** : la plus petite puissance pour laquelle un sommet est à peu près aussi proche que les arêtes des triangles du maillage | Auto |
| **Surface** | **Dessus** : la surface la plus haute sous le rayon. **Premier impact** : la surface la plus haute tournée vers le rayon, ce qui ignore l'arrière et l'intérieur du maillage | Dessus |
| **Trous** | Un sommet d'une dalle conservée que le maillage ne couvre pas prend la **Hauteur la plus proche** (le bord du maillage est prolongé) ou la **Hauteur la plus basse** | Hauteur la plus proche |
| **Dalles de marge** | Dalles gardées en plus autour de celles qui touchent le maillage, de 0 à 8. Les dalles que le maillage touche à peine (seulement son bord, ou une mince bande) sont ignorées | 0 |
| **Lissage** | Itérations qui moyennent chaque hauteur avec ses voisines, de 0 à 64 | 0 |
| **Échelle** | Échelle du maillage après la conversion des unités, de 0,001 à 1000 | 1 |
| **Placement** | **Centre** : le milieu du maillage et son point le plus bas vont à la **Position**. **Coin min** : le coin le plus bas va à la **Position**. **Garder la position** : le maillage reste où le fichier le met. **Sur la sélection** place la position au centre de la sélection | Centre (Garder la position pour un prop ou un modèle du jeu) |
| **Texture**, **Échelle de texture** | Matériau des displacements (**Actuelle** prend la texture choisie dans le navigateur de textures) et son échelle. La texture se prolonge d'une dalle à l'autre | `DEV/DEV_MEASUREGENERIC01B`, 0,25 |
| **Épaisseur des brushes** | Épaisseur du brush sous chaque displacement | 16 |
| **Jupes sur le bord** | Ferme le bord des dalles avec des murs qui descendent jusqu'à la hauteur de base : un displacement par arête du bord, dont le haut suit l'arête de la dalle | Désactivé |
| **Brush plat en dessous** | Un brush plat sous les dalles à la hauteur de base (un brush par rangée de dalles) | Désactivé |
| **Hauteur de base** | Hauteur du bas des jupes et du brush plat. Désactivé : une épaisseur sous le point le plus bas | Désactivé |
| **Grouper les nouveaux brushes** | Met tous les nouveaux brushes dans un seul groupe | Activé |
| **Axe haut du fichier**, **Convertir les unités** | Pour un fichier : les fichiers Y haut sont tournés pour que Z soit en haut, et les mètres deviennent des unités Source (1 m = 39,37 unités) | Selon le fichier, Activé |

Comment les hauteurs sont faites :

- Toutes les dalles ont la même puissance et partagent un seul réseau d'échantillons : les sommets d'une arête commune ont exactement la même hauteur, donc les raccords sont fermés sans couture à faire. Toutes les faces des dalles sont dans un même plan, à la hauteur la plus basse de la surface, donc les dalles voisines sont de vraies voisines pour le compilateur.
- Chaque sommet est réglé avec la distance du displacement le long de la normale de la face. Les jupes utilisent les décalages du displacement (leur rangée du haut est l'arête de la dalle, leur rangée du bas est plate à la hauteur de base).
- Un surplomb (une surface tournée vers le haut au-dessus d'une autre, comme un toit au-dessus d'un sol, à plus de deux espacements de sommets) garde la surface du dessus, et un avertissement donne le nombre de sommets. Un maillage plein n'est pas un surplomb : son dessous est tourné vers le bas.
- Plus de 2048 displacements déclenche un avertissement : le moteur en accepte à peu près autant dans une map. Plus de 4096 dalles sont refusées : utilisez une plus grande taille de dalle.

#### Plaquer sur un maillage

**Outils > Plaquer les displacements sur un maillage...** (ou **Plaquer sur un maillage...** dans la section **Outils**) projette des displacements existants sur un maillage. Posez d'abord des dalles de displacement plates à la main (ou avec **Daller la sélection**), sélectionnez-les, puis choisissez un fichier, un prop ou un modèle du jeu. Pour chaque sommet, un rayon le long de l'axe trouve la surface du maillage et la distance du displacement (ou le décalage, quand le sommet ne bouge pas le long de sa normale) est réglée pour que le sommet soit dessus. Une boîte dans les vues montre où se trouve le maillage.

| Option | Effet | Par défaut |
| --- | --- | --- |
| **Projection**, **Depuis l'autre côté** | Axe des rayons. **Meilleur choix** suit la normale des displacements sélectionnés | Z |
| **Surface** | **Dessus** ou **Premier impact**, comme ci-dessus | Dessus |
| **Échelle**, **Placement**, **Position** | Où se trouve le maillage. Pour un maillage fait dans les coordonnées de la map (un prop, ou un fichier exporté depuis la map) utilisez **Garder la position** | Garder la position |
| **Force** | 0 garde la forme actuelle, 1 met les sommets exactement sur le maillage, entre les deux fait un mélange | 1 |
| **Décalage de hauteur** | Unités ajoutées à la hauteur du maillage le long de l'axe | 0 |

Les sommets que le maillage ne couvre pas restent comme ils sont, et la notification les compte. Une seule étape d'annulation (**Plaquage des displacements sur un maillage**). Les displacements voisins reçoivent la même hauteur sur leur arête commune, ils restent donc raccordés.

#### Daller la sélection

**Outils > Daller la sélection avec des displacements...** (ou **Daller la sélection...** dans la section **Outils**) convertit les brushes sélectionnés, ou les faces sélectionnées, en dalles de displacement plates : dessinez la zone du terrain avec des blocs ordinaires, convertissez-la, puis utilisez **Plaquer sur un maillage**. Chaque face plate et perpendiculaire à l'axe (la face du dessus pour Z) est découpée en dalles égales d'au plus **Taille des dalles** unités : une face de 512 sur 300 avec une taille de 256 donne 2 sur 2 dalles ; les faces inclinées ou tournées ailleurs sont ignorées et comptées. **Supprimer les brushes d'origine** (activé par défaut) retire les brushes qui ont été dallés ; Annuler les ramène. Les options sont l'axe, la taille des dalles, la puissance (**Auto** vaut 3), la texture, son échelle, l'épaisseur et le groupement. Une seule étape d'annulation (**Dallage de la sélection en displacements**).

MCP : `mesh_to_displacement` (`path`, `prop` ou `model`, `up`, `convert_units`, `axis`, `flip`, `tile_size`, `power`, `pick`, `fill`, `margin`, `smooth`, `scale`, `origin`, `position`, `material`, `uv_scale`, `skirt`, `bottom`, `base_height`, `thickness`, `group`, `dry_run`), `conform_displacements_to_mesh` (les mêmes arguments de maillage, `faces`, `ids` ou `solid_ids`, `axis`, `flip`, `pick`, `scale`, `origin`, `position`, `strength`, `offset`) et `tile_displacements_from_selection` (`ids` ou `solid_ids`, `axis`, `flip`, `tile_size`, `power`, `material`, `uv_scale`, `thickness`, `delete_source`, `group`). `dry_run` renvoie la disposition (dalles, puissance, espacement, plan, avertissements) sans modifier la map. Un `path` relatif part du dossier de la map.

### Sculpter dans Blender

**Outils > Envoyer vers Blender** envoie les displacements sélectionnés vers Blender, un objet par displacement, pour les sculpter avec les pinceaux et modificateurs de Blender ; le résultat est réécrit dans les mêmes displacements (alpha, flags, tags, échelle de lightmap et données de mélange non touchées gardés, raccords recousus, une seule étape d'annulation). Voir [Lien avec Blender](blender.md).

### Bruit et érosion

Ces commandes agissent sur les displacements sélectionnés, ou ceux des solides sélectionnés, en une étape d'annulation chacune, et les displacements sont cousus ensuite. Les hauteurs se mesurent le long de la normale de la face. Les valeurs sont enregistrées avec les options du panneau.

| Commande | Ce qu'elle fait |
| --- | --- |
| **Ajouter du bruit**, **Aléatoire** | Une hauteur aléatoire par sommet entre **Minimum** et **Maximum**. Un résultat différent à chaque fois |
| **Ajouter du bruit**, **Fractal** | Bruit de Perlin doux, additionné sur plusieurs octaves, mis à l'échelle de la plage **Minimum** à **Maximum**. Il ne dépend que de la position dans la carte : les displacements voisins se raccordent sans couture. La même **Graine** donne le même résultat |
| **Érosion thermique** | La matière glisse sur les pentes plus raides que l'angle d'éboulis, passe après passe, jusqu'à ce que le terrain tienne. La hauteur totale est conservée |
| **Érosion hydraulique** | La pluie tombe sur chaque sommet et ruisselle vers le bas : elle creuse où elle va vite et dépose ses sédiments où elle ralentit. La hauteur totale est conservée |

Les displacements connectés (cousus, qui partagent des arêtes) sont érodés comme un seul champ de hauteurs : l'érosion traverse leurs bords et ils restent cousus. Un displacement sans voisin parmi les cibles est érodé seul. Les voisins non sélectionnés ne changent pas, et la couture referme les fissures ensuite.

| Option | S'applique à | Ce qu'elle fait | Défaut ou plage |
| --- | --- | --- | --- |
| **Échelle** | Fractal | Taille des plus grands reliefs, en unités | 256 (1 à 65536) |
| **Octaves** | Fractal | Couches de détail, chacune deux fois plus fine que la précédente | 4 (1 à 8) |
| **Persistance** | Fractal | Poids de chaque couche plus fine par rapport à la précédente | 0,5 (0,05 à 1) |
| **Graine** | Fractal, Hydraulique | Même graine, même résultat (la pluie de l'érosion hydraulique est aléatoire mais répétable) | 1 |
| **Itérations** | Thermique, Hydraulique | Nombre de passes | 20 (1 à 1000) et 50 (1 à 2000) |
| **Angle d'éboulis** | Thermique | Pente la plus raide, en degrés, sur laquelle la matière tient | 35 (0 à 89) |
| **Intensité** (Thermique) | Thermique | Part de l'excédent qui glisse à chaque passe | 0,5 (0 à 1) |
| **Pluie** | Hydraulique | Eau ajoutée à chaque sommet à chaque passe | 1 (0 à 100) |
| **Évaporation** | Hydraulique | Part de l'eau qui s'évapore à chaque passe | 0,1 (0 à 1) |
| **Capacité** | Hydraulique | Sédiments que l'eau transporte par unité d'eau et de pente | 1 (0 à 10) |
| **Dépôt** | Hydraulique | Part des sédiments au-dessus de la capacité qui se déposent à chaque passe | 0,3 (0 à 1) |
| **Intensité** (Hydraulique) | Hydraulique | Part de la capacité manquante arrachée au sol à chaque passe | 0,3 (0 à 1) |

Conseils : quelques passes d'érosion thermique adoucissent un bruit anguleux en pentes crédibles ; l'érosion hydraulique creuse des ravines sur les pentes raides, donnez-lui donc du relief d'abord (bruit ou sculpture).

### Fusion des hauteurs

Fusionne les hauteurs de deux ensembles de displacements ou plus sur une zone de transition, pour des morceaux de terrain qui se chevauchent ou se touchent : une colline et une plaine, deux dalles construites à des altitudes différentes, un lit de route et le sol. Utilisez la section **Fusion des hauteurs** du panneau **Outils**, **Outils > Fusionner les hauteurs des displacements**, la palette de commandes ou l'outil MCP `blend_displacements`. Tout se fait en une étape d'annulation (**Fusion des displacements**) et les displacements sont cousus ensuite.

Les ensembles :

- **A = sélection** et **B = sélection** mémorisent les displacements sélectionnés comme ensemble A et ensemble B. Un ensemble peut contenir des displacements de plusieurs brushes ou groupes.
- Sans les deux ensembles, les displacements sélectionnés sont répartis par groupe (le groupe de plus haut niveau de chaque solide, ou son entité brush) ; s'ils partagent tous un groupe, par solide. Le panneau indique combien d'ensembles il a trouvés.
- Avec plus de deux ensembles (sélectionnez plusieurs groupes), chaque ensemble fusionne avec tous les autres.

| Option | Ce qu'elle fait | Défaut |
| --- | --- | --- |
| **Largeur** | Largeur de la zone de transition en unités, 0,01 à 65536. Un sommet d'un ensemble change quand il est plus près que cela d'un autre ensemble (ou dedans) | 128 |
| **Courbe** | Comment la fusion croît du bord de la zone jusqu'à la ligne de rencontre : **Linéaire**, **Douce**, **Entrée douce** (départ lent) ou **Sortie douce** (fin lente) | Douce |
| **Mode** | **Moyenne** : les deux côtés se rejoignent au milieu. **A sur B** : A ne bouge pas et B le suit avec un bord doux (avec plusieurs ensembles, un ensemble suit ceux qui le précèdent). **B sur A** : l'inverse. **Maximum** ou **Minimum** : la surface la plus haute ou la plus basse l'emporte | Moyenne |
| **Raccorder les hauteurs de bord** | Décale chaque côté de l'écart de hauteur mesuré à la ligne de rencontre au lieu de copier l'autre côté : chaque côté garde son relief et deux morceaux à des altitudes différentes se raccordent sans marche | Désactivé |
| **Fusionner l'alpha des textures** | Fusionne l'alpha des sommets, et les lignes du 4-way blend, sur la même zone avec les mêmes poids | Activé |
| **Aligner d'abord les puissances** | Rééchantillonne les displacements de puissance plus faible des ensembles à la puissance la plus haute (**Garder la forme**), pour que la fusion garde tout le détail. Désactivé : chaque displacement garde sa puissance | Désactivé |
| **Afficher la zone dans la vue 3D** | Dessine les sommets que la fusion va déplacer, plus gros et plus lumineux là où elle agit le plus | Activé |

Fonctionnement : dans la zone, un sommet passe de sa hauteur vers celle de l'autre ensemble selon la courbe, de rien au bord de la zone jusqu'à la fusion complète à la ligne de rencontre (et partout dans un chevauchement). Sur une arête commune, les deux côtés reçoivent la même hauteur : les morceaux se rejoignent exactement. La hauteur de l'autre ensemble est lue sur ses triangles, donc des puissances différentes et des grilles non alignées fusionnent correctement. Les displacements de puissances différentes qui se touchent sont cousus ensuite (l'arête suit la plus grossière). Les voisins hors des ensembles ne sont pas modifiés.

### Contrôle des pentes

Mesure, limite et façonne les pentes des displacements sélectionnés. Une pente est l'angle d'un triangle depuis l'horizontale : 0 degré est à plat, 90 degrés est un mur. Les hauteurs bougent selon Z et les displacements cousus sont traités comme une seule surface. Chaque commande est une étape d'annulation et le résultat est cousu. La section **Contrôle des pentes** du panneau **Outils** contient tout ; le menu Outils a **Limiter la pente des displacements**, **Uniformiser la pente des displacements**, **Fusionner les hauteurs des displacements** et **Sélectionner les displacements par pente** (ils utilisent les options du panneau et ouvrent l'outil displacement) ; MCP : `control_slope` et `select_displacements_by_slope`.

#### Couleurs des pentes

**Afficher les pentes dans la vue 3D** est l'affichage **Praticabilité des displacements** (**Affichage > Options d'affichage**), avec un mode et un angle :

| Mode | Couleurs |
| --- | --- |
| **Source** | Les trois classes du moteur : praticable et constructible, praticable, non praticable (par défaut). Les marquages forcés l'emportent |
| **Limite** | Vert jusqu'à l'angle, rouge au-dessus. Les marquages sont ignorés : seule la géométrie compte |
| **Dégradé** | Huit bandes autour de la limite : vert et citron très en dessous, jaune, ambre jusqu'à la limite, puis orange, rouge et rouge sombre au-dessus, et violet pour les surplombs (surfaces tournées vers le bas). Les marquages sont ignorés |

**Angle praticable** est la limite des modes Limite et Dégradé, de 1 à 89 degrés, **45,57** par défaut : la limite praticable de Source (une normale de triangle avec un Z de 0,7). Le bouton **45,57** la remet. La vue 3D dessine une légende avec les angles de chaque couleur. Les mêmes options sont dans les Paramètres, page **Rendu** (`render.disp_walk_mode` et `render.disp_walk_angle`).

#### Mesurer

**Mesurer** affiche la pente minimale, maximale et moyenne des displacements sélectionnés, et le plan qui les approche le mieux (son angle et la direction vers laquelle il monte). **Prendre la mesure** copie ce plan dans les options de rampe.

#### Limiter la pente

Relâche les sommets jusqu'à ce qu'aucun triangle ne soit plus raide que **Angle** (1 à 89 degrés, 45 par défaut). La hauteur moyenne est conservée : les endroits raides descendent et le reste monte un peu. Le résultat est garanti : aucun triangle ne dépasse l'angle, quelle que soit la forme de départ. **Terrasses** découpe d'abord les hauteurs en paliers plats de **Hauteur de palier** unités (**Force des terrasses** de 0 à 1 règle de combien les hauteurs se calent), puis limite les contremarches entre eux à l'angle : des paliers plats et des contremarches en pente.

#### Rampe exacte

Transforme la sélection en un plan d'exactement **Angle** degrés montant vers **Direction** (degrés dans le plan de la carte : 0 est +X, 90 est +Y). **Conserver** choisit ce qui reste en place : la hauteur moyenne, le point le plus bas ou le plus haut. **Force** (0 à 1) règle la part du chemin vers le plan, et **Garder le relief** (0 à 1) garde cette part des bosses actuelles au-dessus du plan (0 donne un plan parfait). Les displacements cousus forment un seul plan : un ensemble de dalles devient une seule rampe. Une rampe de 0 degré est un sol plat.

#### Pente uniforme

Rend la pente constante sur toute la sélection : le plan d'ajustement optimal est mesuré et la surface y est amenée, en gardant son angle et sa direction moyens (**Force** et **Garder le relief** fonctionnent comme pour la rampe).

#### Pente constante le long d'un chemin

Fait monter le sol près d'un chemin à pente constante, de la hauteur du premier point à celle du dernier : routes, rampes, rails. Donnez le chemin avec **Placer des points dans la vue 3D** (activé, un clic sur un displacement ajoute un point au lieu de sélectionner ; le chemin est dessiné en jaune), **Depuis les entités sélectionnées** (les origines des entités sélectionnées dans l'ordre de sélection, par exemple des `path_corner` ou `info_target`) ou l'argument MCP `path`. La surface à moins de **Demi-largeur** unités du chemin est amenée à la pente, en s'estompant vers le bord selon la **Courbe** (**Force** de 0 à 1). Les hauteurs du premier et du dernier point fixent la pente : placez-les sur le sol ou déplacez d'abord les entités. **Effacer le chemin** le vide.

#### Adoucir les pentes

Atténue les variations de pente de la sélection en moyennant chaque sommet avec ses voisins : **Itérations** passes (1 à 1000) de **Force** (0 à 1). Les displacements cousus sont adoucis ensemble.

#### Sélectionner par pente

Sélectionne les displacements dont des triangles sont **Au-dessus** ou **En dessous** d'un angle (0 à 90 degrés depuis l'horizontale, 45,57 par défaut). **Un triangle** compte un displacement quand un de ses triangles correspond, **Tous les triangles** seulement quand tous correspondent. **Sélection** remplace la sélection, s'y ajoute ou en retire. Les candidats sont les displacements sélectionnés, ou tous ceux de la carte quand aucun n'est sélectionné : la commande peut donc aussi affiner une sélection. Une sélection de pentes raides suivie de **Limiter la pente** ou **Faire une rampe** ne corrige que ces morceaux.

MCP : `blend_displacements` (`sets` en listes d'ids de solides, ou `faces`, `ids`, `solid_ids`, `width`, `falloff`, `mode`, `match_edges`, `alpha`, `match_powers`), `control_slope` (`operation` `limit`, `ramp`, `uniform`, `path`, `smooth`, `measure` ou `display`, les cibles, `options` avec les noms `slope_*`, `path` ; `display` prend `show`, `mode` et `walk_angle`) et `select_displacements_by_slope` (`angle`, `mode`, `match`, `op` et les cibles). Les réponses donnent la plage de pente avant et après.

## Options d'affichage

| Où | Option | Effet | Par défaut |
| --- | --- | --- | --- |
| **Affichage > Options d'affichage** | **Afficher les displacements** | Affiche ou masque les displacements | Activé |
| **Affichage > Options d'affichage** | **Maillage des displacements en vue 2D** | Fait passer les vues 2D du maillage complet au contour seul, et inversement | Maillage complet |
| **Affichage > Options d'affichage** | **Praticabilité des displacements** | Colore les triangles : praticable et constructible (normale Z de la pente d'au moins 0,8), praticable (au moins 0,7), non praticable. Les marquages forcés l'emportent, comme dans la map compilée. Une légende s'affiche dans la vue 3D (et dans une vue 2D agrandie qui montre le maillage). Le mode (Source, Angle limite ou Dégradé d'angle) et l'angle praticable sont dans les Paramètres et dans la section [Contrôle des pentes](#contrôle-des-pentes) | Désactivé |
| Menu d'une vue 2D, **Displacements** | **Comme les autres vues**, **Maillage complet**, **Maillage de la sélection**, **Contour seulement**, **Brushes d'origine** | Mode de cette vue seulement ; **Brushes d'origine** dessine en contour discret le brush à partir duquel chaque displacement a été créé (toutes les vues 2D) | Comme les autres vues |
| Paramètres, page **Rendu** | **Displacements**, **Displacements en 2D**, **Displacements seuls**, **Displacements dans les vues 2D** (mode, avec **Utiliser ce mode dans toutes les vues 2D**), **Brushes d'origine dans les vues 2D**, **Praticabilité des displacements** | Les mêmes options, pour toutes les vues. **Displacements seuls** masque tous les brushes qui ne sont pas des displacements | Activé, Activé, Désactivé, Maillage complet, Activé, Désactivé |

## Paramètres

**Outils > Options...** (`Ctrl+,`), page **Sommets et displacements**, section **Displacements** :

| Paramètre | Effet | Par défaut |
| --- | --- | --- |
| **Peindre les mélanges à 4 textures** (`mesh.four_way_blends`) | La peinture d'alpha peut peindre chacune des 4 textures d'un matériau de mélange à 4 textures, comme Hammer++ | Activé |
| **Hauteur minimale du bruit** (`mesh.noise_min`) | Plus petite hauteur ajoutée par le bruit | -4 |
| **Hauteur maximale du bruit** (`mesh.noise_max`) | Plus grande hauteur ajoutée par le bruit | 4 |

## Scripts et MCP

Chaque option du panneau a un nom pour les scripts (`tool_option CLÉ VALEUR`, voir [Commandes de script](../reference/script-commands.md)) et pour les outils MCP `get_tool_options` et `set_tool_options` : `mode`, `effect`, `axis`, `custom_axis`, `radius`, `strength`, `smooth`, `target_height`, `falloff`, `spatial`, `line_of_sight`, `autosew`, `selected_only`, `continuous`, `alpha_effect`, `alpha_value`, `alpha_strength`, `blend_channel`, `sculpt` (`push`, `carve`, `project`, `pinch`, `grab`, `inflate`, `flatten`, `relax`), `push_mode`, `sculpt_bounds`, `falloff_position`, `falloff_final`, `push_distance`, `push_percent`, `carve_depth`, `carve_profile`, `project_range`, `flatten_plane`, `flatten_strength`, `relax_strength`, `scale_factor`, `noise_type`, `noise_scale`, `noise_octaves`, `noise_persistence`, `terrain_seed`, `thermal_iterations`, `thermal_talus`, `thermal_strength`, `hydraulic_iterations`, `hydraulic_rain`, `hydraulic_evaporation`, `hydraulic_capacity`, `hydraulic_deposition`, `hydraulic_strength`, `blend_width`, `blend_falloff`, `blend_mode`, `blend_match_edges`, `blend_alpha`, `blend_match_powers`, `slope_max_angle`, `slope_terrace`, `slope_terrace_step`, `slope_terrace_strength`, `slope_ramp_angle`, `slope_ramp_direction`, `slope_ramp_pivot`, `slope_ramp_strength`, `slope_ramp_detail`, `slope_uniform_strength`, `slope_uniform_detail`, `slope_path`, `slope_path_width`, `slope_path_falloff`, `slope_path_strength`, `slope_smooth_passes`, `slope_smooth_strength`, `slope_select_angle`, `slope_select_mode`, `slope_select_match`, `slope_select_op`, `tag_kind`, `tag_value`, `mask_grid`, `mask_select`, `keep_shape`. `axis` prend `normal`, `subdiv`, `center`, `view`, `view_flat` (Vue (caméra) horizontale), `x`, `y`, `z` ou `custom`. D'autres outils MCP créent, peignent, cousent, subdivisent, marquent et inspectent les displacements (`displacement_to_mesh` les écrit en OBJ, `mesh_to_displacement`, `conform_displacements_to_mesh` et `tile_displacements_from_selection` les créent ou les modèlent à partir d'un maillage 3D, `terrain_displacements` lance le bruit fractal, l'érosion thermique ou hydraulique avec ces options, `blend_displacements`, `control_slope` et `select_displacements_by_slope` fusionnent les hauteurs et contrôlent ou sélectionnent par pente, `create_displacements`, `stroke_displacement`, `paint_displacement`, `sew_displacements`, `resample_displacements`...) ; `get_displacement` renvoie les rangées du mélange à 4 textures (`multiblend`) et `set_displacement_data` les écrit (`multiblend_weights`, `alphablend`, `multiblend_colors`, `remove_multiblend`), voir la [référence des outils MCP](../../MCP.md) (en anglais).

## Astuces

- La puissance 3 suffit pour la plupart des terrains : la puissance 4 a environ 3,5 fois plus de sommets et ralentit le maillage de collision (les statistiques de compilation le signalent).
- Activez **Coudre en peignant** quand vous peignez sur plusieurs displacements : aucune fissure n'apparaît et pas besoin de **Coudre** ensuite.
- Avec des displacements sélectionnés, seuls ceux-ci sont peints. Appuyez sur `Échap` pour vider la sélection et peindre tout ce qui est sous le pinceau.
- Utilisez **Creuser** avec le profil **U** pour les chemins et les rivières, puis **Lisser** pour adoucir les berges.
- Utilisez **Ligne de vue** pour peindre le flanc d'une colline sans toucher au terrain derrière.

## FAQ

**Rien n'est peint, le cercle du pinceau est gris.** Des displacements sont sélectionnés et le pinceau n'en touche aucun. Cliquez sur ce displacement pour le sélectionner (`Ctrl` l'ajoute), appuyez sur `Échap` pour vider la sélection, ou décochez **Sélection seulement**.

**Les boutons de création sont grisés.** Sélectionnez des faces, ou des solides (leurs faces supérieures sont utilisées). Seules les faces à 4 côtés peuvent devenir des displacements.

**Mes displacements ont des fissures entre eux.** Sélectionnez-les et cliquez sur **Coudre**, ou peignez avec **Coudre en peignant** activé.

**J'ai peint au mauvais endroit.** Appuyez sur `Échap` pendant le trait pour l'annuler, ou sur `Ctrl+Z` ensuite : chaque trait est une étape d'annulation.

**Pourquoi mes textures de mélange ne changent pas ?** Le matériau doit être un matériau de fusion (WorldVertexTransition, ou Lightmapped_4WayBlend pour les boutons des 4 textures).

**Mon mélange à 4 textures ne montre que sa première texture dans le jeu.** Recompilez avec Hammer Pro : les rangées du mélange sont écrites dans le BSP (voir Mélanges à 4 textures plus haut). Une texture qui reste cachée alors qu'elle est peinte peut aussi venir d'un `$textureN_blendstart` proche de 1 dans le VMT : peignez la texture jusqu'à son poids plein, ou baissez cette valeur.

## Pinceau Relax

Choisissez **Sculpter > Sculpter la géométrie > Relax**. Maintenez le clic gauche pour répartir les sommets sur la surface du displacement, même sans bouger la souris. Relax agit toujours en continu. L'**Intensité** (0 à 1) règle sa vitesse ; le rayon et l'atténuation définissent la zone. Ctrl n'inverse pas ce pinceau ; Shift conserve le lissage temporaire habituel.

Les sommets glissent tangentiellement et sont reprojetés sur la surface prise au début du trait. Les bords, les trous et les reliefs marqués sont protégés, notamment les raccords cousus. Le nombre de sommets et de triangles, les alphas de texture et la topologie sont conservés ; les mouvements qui retourneraient ou écraseraient des triangles sont réduits ou refusés. Le pinceau répartit la grille existante sans ajouter de triangles. Un trait maintenu correspond à une annulation. Une grille déjà régulière ou une force nulle produit peu ou pas de mouvement.

Scripts/MCP : `mode geometry`, `effect relax`, `relax_strength 0.5`.

## Pages liées

- [Sculpter des displacements (guide)](../guides/sculpt-displacements.md)
- [Outil vertex](vertex-tool.md)
- [Détails de route (fondu du terrain sur une route)](road-details.md)
- [Outil texture](texture-tool.md)
- [Modes de rendu et affichage](render-modes-display.md)
- [Aperçu de l'éclairage](lighting-preview.md)
- [Vérifier les problèmes](check-problems.md)
- [Référence des paramètres](../reference/settings.md)
- [Référence des outils MCP](../../MCP.md)

---
[Retour à l'index de la documentation](../README.md)
