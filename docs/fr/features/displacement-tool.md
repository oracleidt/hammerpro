[Documentation](../README.md) > Fonctionnalités > Outil displacement | [English](../../en/features/displacement-tool.md)

# Outil displacement

L'outil displacement crée, détruit, sélectionne, sculpte et peint les displacements (le terrain fait à partir de faces de brush). Il reprend tout ce que proposent les panneaux displacement de Hammer (sculpter, peindre l'alpha, modeler, marquages, couture, subdivision, bruit, masques), les ajouts de Hammer++ (mélanges à 4 textures, peinture continue) et quelques fonctions en plus (couture en direct, ligne de vue, projection sur des brushes, ombres projetées).

## Ouvrir l'outil

- **Outils > Outil displacement**, la palette d'outils, ou `Maj+P`.
- Ses options sont dans l'onglet **Displacement** du panneau de droite, qui s'ouvre tout seul quand l'outil est choisi. Les options du panneau sont enregistrées dans les paramètres (`mesh.disp_tool`) et reviennent à la prochaine utilisation.
- La première ligne du panneau indique combien de displacements contient la map.

## Modes

La rangée de boutons en haut du panneau choisit ce que font les clics et les glissements dans les vues.

| Mode | Effet |
| --- | --- |
| **Sélection** | Cliquez sur des faces pour les sélectionner (`Ctrl` inverse, `Maj` inverse tous les displacements du solide). Maintenez et balayez pour sélectionner toutes les faces survolées |
| **Sculpter** (par défaut) | Élève, abaisse, lisse ou fixe la hauteur de la surface au pinceau |
| **Peindre l'alpha** | Peint le mélange entre les textures d'un matériau de fusion |
| **Modeler** | Pousse la surface jusqu'à une couche, creuse un profil le long du trait, ou la projette sur des brushes |
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
| **Puissance** | Reconstruit les displacements sélectionnés à plat avec cette puissance (2, 3 ou 4). La sculpture est perdue | Puissance actuelle, ou « (valeurs multiples) » |
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
- **Déplacer et retourner** un displacement garde ses rangées à 4 textures à leur place, comme son alpha. Changer la puissance ou aplanir repart de la texture 1.

## Modeler

Section **Modelage** (mode **Modeler**), l'équivalent du panneau Sculpt de Hammer :

- **Pousser** : élève la surface jusqu'à une couche décalée de sa position au début du trait (`Ctrl` l'enfonce). Repasser au même endroit ne cumule pas.
- **Creuser** : le trait est un chemin, la surface s'enfonce le long de celui-ci selon un profil en travers : fossés, rivières, routes (`Ctrl` élève une crête).
- **Projeter** : déplace la surface sur les brushes sélectionnés le long de l'axe de modelage (une route ou des fondations que le terrain doit épouser). Sélectionnez les brushes en même temps que les displacements ; si aucun brush n'est sélectionné, l'outil vous le dit.

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
| **Ajouter du bruit** | Ajoute une hauteur aléatoire entre le minimum et le maximum à chaque sommet, le long de la normale de la face | |

Une case affiche un état mixte quand la sélection n'est pas homogène ; un clic l'active pour tous.

Les mêmes commandes sont dans le menu **Outils** et marchent quel que soit l'outil actif : **Subdiviser les displacements**, **Inverser l'alpha des displacements** et **Ajouter du bruit aux displacements...** (celle-ci ouvre une petite fenêtre **Bruit sur les displacements** avec la plage et un bouton **Appliquer**). Aucune n'a de raccourci par défaut.

## Options d'affichage

| Où | Option | Effet | Par défaut |
| --- | --- | --- | --- |
| **Affichage > Options d'affichage** | **Afficher les displacements** | Affiche ou masque les displacements | Activé |
| **Affichage > Options d'affichage** | **Maillage des displacements en vue 2D** | Fait passer les vues 2D du maillage complet au contour seul, et inversement | Maillage complet |
| **Affichage > Options d'affichage** | **Praticabilité des displacements** | Colore les triangles : praticable et constructible (normale Z de la pente d'au moins 0,8), praticable (au moins 0,7), non praticable. Les marquages forcés l'emportent, comme dans la map compilée. Une légende s'affiche dans la vue 3D (et dans une vue 2D agrandie qui montre le maillage) | Désactivé |
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

Chaque option du panneau a un nom pour les scripts (`tool_option CLÉ VALEUR`, voir [Commandes de script](../reference/script-commands.md)) et pour les outils MCP `get_tool_options` et `set_tool_options` : `mode`, `effect`, `axis`, `custom_axis`, `radius`, `strength`, `smooth`, `target_height`, `falloff`, `spatial`, `line_of_sight`, `autosew`, `selected_only`, `continuous`, `alpha_effect`, `alpha_value`, `alpha_strength`, `blend_channel`, `sculpt`, `push_mode`, `sculpt_bounds`, `falloff_position`, `falloff_final`, `push_distance`, `push_percent`, `carve_depth`, `carve_profile`, `project_range`, `scale_factor`, `tag_kind`, `tag_value`, `mask_grid`, `mask_select`. `axis` prend `normal`, `subdiv`, `center`, `view`, `view_flat` (Vue (caméra) horizontale), `x`, `y`, `z` ou `custom`. D'autres outils MCP créent, peignent, cousent, subdivisent, marquent et inspectent les displacements (`create_displacements`, `stroke_displacement`, `paint_displacement`, `sew_displacements`...) ; `get_displacement` renvoie les rangées du mélange à 4 textures (`multiblend`) et `set_displacement_data` les écrit (`multiblend_weights`, `alphablend`, `multiblend_colors`, `remove_multiblend`), voir la [référence des outils MCP](../../MCP.md) (en anglais).

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

## Pages liées

- [Sculpter des displacements (guide)](../guides/sculpt-displacements.md)
- [Outil vertex](vertex-tool.md)
- [Outil texture](texture-tool.md)
- [Modes de rendu et affichage](render-modes-display.md)
- [Aperçu de l'éclairage](lighting-preview.md)
- [Vérifier les problèmes](check-problems.md)
- [Référence des paramètres](../reference/settings.md)
- [Référence des outils MCP](../../MCP.md)

---
[Retour à l'index de la documentation](../README.md)
