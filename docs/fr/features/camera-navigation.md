[Documentation](../README.md) > Fonctionnalités > Caméra et navigation | [English](../../en/features/camera-navigation.md) | [Русский](../../ru/features/camera-navigation.md)

# Caméra et navigation

Comment se déplacer dans la map : la caméra volante de la vue 3D, le défilement et le zoom des vues 2D, les trois schémas de contrôle (Hammer Pro, Hammer, Hammer++), l'outil caméra, les types de vue et l'agrandissement d'une vue.

## Vue 3D : la caméra volante

Ces contrôles marchent avec tous les outils et tous les schémas :

| Entrée | Effet |
|---|---|
| Maintenir le clic droit et bouger la souris | Regarder autour |
| Clic droit maintenu : `W` `A` `S` `D` (`Z` `Q` `S` `D` en AZERTY) | Voler en avant, à gauche, en arrière, à droite |
| `E` ou `Espace` / `Q` (`A` en AZERTY) | Monter / descendre (sur l'axe Z du monde) |
| `Maj` | Voler plus vite (multiplicateur de vitesse rapide) |
| Molette en vol | Change la vitesse de la caméra (x1,25 par cran, de 50 à 20000 unités/s) ; une notification affiche la nouvelle vitesse |
| Touche de vue souris (`W` en AZERTY, `Z` en QWERTY) | Vue souris permanente : regarder autour sans tenir de bouton. La même touche ou `Échap` en sort |
| Glisser avec le bouton du milieu | Faire défiler (la scène sous le pointeur le suit) |
| Molette | Avancer ou reculer, vers le curseur si **Zoomer vers le curseur** est activé. `Maj` : plus vite |
| Clic droit sans bouger | Ouvre le menu contextuel de la vue |

- Désactivez **Paramètres > Vues > Déplacements fluides de la caméra** pour des mouvements précis avec démarrage et arrêt immédiats. Avec l'option activée, la vitesse monte en **Temps d'accélération de la caméra** secondes (0 est également immédiat). Ce temps est conservé lorsque les déplacements fluides sont désactivés.
- `Ctrl` ne fait jamais voler : en vol ou en vue souris, un clic gauche sélectionne l'objet sous le viseur (le centre de la vue), et `Ctrl+clic` l'ajoute ou le retire de la sélection, comme dans Hammer. Le pinceau des displacements, lui, continue de peindre.
- Les raccourcis ne se déclenchent pas pendant que le clavier pilote la caméra.

### Texturer en vue souris

En vue souris (`Z`), vous pouvez texturer la face sous le viseur sans quitter le mode caméra, avec n'importe quel outil :

| Entrée | Effet |
|---|---|
| `T` (`fly_apply_texture`) ou clic droit | Applique la texture actuelle à la face visée |
| `F` (`fly_apply_texture_brush`) ou `Maj+clic droit` | Applique la texture actuelle à toutes les faces du brush visé |
| `V` (`fly_pick_texture`) ou clic du milieu | Prélève la texture de la face visée comme texture actuelle |
| `N` (`fly_align_texture`) | Aligne la texture de la face visée sur la vue (les directions droite et haut de la caméra) |

- La face visée (la première face de solide dans la direction du regard, jusqu'à 16384 unités) n'est pas surlignée : le viseur et son étiquette la désignent. Un viseur est dessiné au centre de la vue avec le solide, la face et la texture de la cible, et une ligne d'aide en bas de la vue liste les touches et la texture actuelle.
- Chaque modification est une étape d'annulation. Les touches sont des actions : elles se changent dans **Paramètres > Clavier** (`[keybindings]`) comme les autres, et ne fonctionnent qu'en vue souris.
- Le clic gauche continue de sélectionner l'objet sous le viseur.
- Par MCP : `ui_key` avec `z` entre en vue souris, puis `t`, `f`, `v` ou `n` agissent sur la face au centre de la vue (`set_camera` pour viser, `list_keybindings` pour les touches).
- Quand l'outil actif utilise le clic droit dans la vue 3D (l'outil de texture applique la texture actuelle), `Maj+clic droit` ouvre le menu contextuel.

### Disposition du clavier (touches de la caméra)

**Touches de la caméra**, dans la page **Vues** des paramètres, décide des touches de vol :

| Valeur | Touches de vol | Touche de vue souris |
|---|---|---|
| **Auto (clavier)** (par défaut) | Selon la position des touches : ZQSD sur un clavier AZERTY, WASD en QWERTY (descendre : A en AZERTY) | La touche à la place du Z QWERTY (W en AZERTY) ; la touche marquée Z y entre aussi |
| **AZERTY** | Selon les lettres : Z Q S D, A descend, E monte | W |
| **QWERTY** | Selon les lettres : W A S D, Q descend, E monte | Z |

## Schémas de contrôle

**Contrôles de la caméra**, dans la page **Vues** des paramètres (`[workflow] nav_scheme`) :

| Schéma | Ajoute |
|---|---|
| **Hammer Pro** (par défaut) | Les contrôles ci-dessus |
| **Hammer** | Dans la vue 3D : `Espace` + glisser clic gauche regarde autour, `Espace` + glisser clic droit fait défiler, clic gauche + clic droit + glisser avance (pointeur vers le haut) et décale sur le côté, les flèches font regarder autour (90° par seconde) quand rien n'est sélectionné |
| **Hammer++ (bouton du milieu)** | Les contrôles Hammer, mais le bouton du milieu fait regarder autour (et voler) dans la vue 3D au lieu de faire défiler. Un clic du milieu sans bouger arrive toujours à l'outil |

La page des paramètres liste sous la liste déroulante les contrôles du schéma choisi.

## Vues 2D

| Entrée | Effet |
|---|---|
| Glisser avec le bouton du milieu, ou `Espace` + glisser clic gauche | Faire défiler |
| Molette | Zoom (x1,2 par cran), vers le curseur si **Zoomer vers le curseur** est activé |
| `Ctrl+molette` | Change la taille de la grille (par défaut) ou zoome toutes les vues 2D, voir **Ctrl + molette en 2D** |
| `+` / `-` | **Zoom avant des vues 2D** / **Zoom arrière des vues 2D** (toutes les vues 2D) |
| `1` à `9` | Zooms prédéfinis de la vue sous la souris : `5` affiche une unité par point (100 %), chaque touche en dessous divise par deux, chaque touche au-dessus multiplie par deux |
| Flèches | Font défiler la vue sous la souris d'un huitième de sa taille, quand rien n'est sélectionné (ou si le décalage aux flèches est désactivé) |
| Clic droit sans bouger | Ouvre le menu contextuel |

Avec **Zoomer les vues 2D ensemble** activé, zoomer une vue 2D zoome les autres de la même façon. L'en-tête de chaque vue 2D affiche son zoom en pourcentage.

## Outil caméra

`Maj+C` ou **Outils > Outil caméra**. Il règle la caméra 3D depuis les vues 2D, comme l'outil caméra de Hammer.

Dans une vue 2D, la caméra est dessinée comme un œil relié par une ligne à sa cible :

| Geste | Effet |
|---|---|
| Glisser la caméra | La déplace ; elle continue de viser sa cible. `Maj` : déplace les deux |
| Glisser la cible | Déplace la cible, la caméra se tourne vers elle |
| Glisser dans le vide | Place la caméra au point de départ, tournée vers le pointeur |
| `Maj+glisser` dans le vide | Ajoute une nouvelle caméra, placée de la même façon |
| `Page préc.` / `Page suiv.` | Caméra précédente / suivante enregistrée dans la map |
| `Suppr` | Supprime la caméra active (une caméra par défaut est créée quand la dernière disparaît) |
| `Échap` | Annule le glisser |

Avec **Outil caméra : cliquer pour zoomer** (page **Sélection**, désactivé par défaut), un clic dans une vue 2D zoome par deux (`Ctrl+clic` dézoome), comme la loupe de Hammer.

Dans la vue 3D, glisser fait regarder autour et un clic sélectionne comme l'outil de sélection (`Ctrl+clic` ajoute ou retire, cliquer encore prend l'objet derrière).

Barre d'options de l'outil :

| Contrôle | Effet |
|---|---|
| **Caméra {n}/{m}**, flèches, corbeille | Les caméras enregistrées dans la map (jusqu'à 256) : précédente, suivante, supprimer |
| **Position** | Position de la caméra (X, Y, Z) |
| **Lacet**, **Tangage** | Angles de la caméra (tangage de -89° à 89°) |
| **Viser la sélection** | Oriente la caméra vers le centre de la sélection sans la déplacer |
| **Centrer la vue 3D sur la sélection** | Déplace la caméra pour que la sélection remplisse la vue |
| **Réinitialiser** | Remet la caméra à sa position par défaut |

## Commandes des vues

| Commande | Raccourci | Effet |
|---|---|---|
| **Centrer les vues 2D sur la sélection** | `Ctrl+E` | Centre et zoome les vues 2D sur la sélection |
| **Centrer la vue 3D sur la sélection** | `Ctrl+Maj+E` | Pointe la caméra 3D sur la sélection pour qu'elle remplisse la vue |
| **Agrandir la vue** | `Maj+Z` | Agrandit la vue sous la souris, ou rétablit les quatre vues |
| **Vue de dessus** | `F2` | Affiche la vue de dessus dans la vue sous la souris |
| **Vue de côté** | `F3` | Vue de côté |
| **Vue de face** | `F4` | Vue de face |
| **Vue 3D texturée** | `F5` | Vue 3D en mode Texturé |
| **Vue 3D unie** | `Maj+F5` | Vue 3D en mode Couleurs unies |
| **Vue 2D suivante** | `Tab` | Dessus, Face, Côté à tour de rôle, dans la vue 2D sous la souris |
| **Distance d'affichage plus courte** / **plus longue** | aucun | Divise ou multiplie la distance d'affichage 3D par 1,5 |
| **Réinitialiser le partage des vues** | aucun | Remet les séparateurs des vues au milieu |
| **Informations de la vue 3D** | `O` | Images par seconde, position et angles de la caméra, statistiques du rendu dans la vue 3D. Un clic copie `setpos` et `setang` pour la console du jeu |

Elles se trouvent dans le menu **Affichage**. Le menu du clic droit d'une vue propose aussi **Centrer les vues ici**.

Chaque vue a un en-tête : cliquez sur le nom de la vue pour choisir un autre type de vue (et, en 3D, le mode de rendu), double-cliquez dessus pour agrandir la vue, ou utilisez le bouton d'agrandissement à sa droite. Glissez les séparateurs entre les vues pour les redimensionner, double-cliquez sur un séparateur pour le remettre en place.

## Paramètres

**Outils > Options...**, page **Vues** :

| Option | Effet | Par défaut |
|---|---|---|
| **Contrôles de la caméra** | Hammer Pro, Hammer ou Hammer++ (bouton du milieu) | Hammer Pro |
| **Inverser l'axe X de la souris** | Inverse l'axe horizontal pour regarder autour | Désactivé |
| **Déplacements fluides de la caméra** | Accélération et décélération ; désactiver pour démarrer et s'arrêter immédiatement | Activé |
| **Temps d'accélération de la caméra** | Temps que met la caméra en vol pour atteindre sa vitesse (0 à 0,5 s) | 0,07 s |
| **Ctrl + molette en 2D** | Change la taille de la grille, ou zoome toutes les vues 2D | Change la taille de la grille |
| **Zoomer les vues 2D ensemble** | Zoomer une vue 2D zoome les autres | Désactivé |
| **Champ de vision** | Champ de vision 3D (40° à 120°) | 75° |
| **Vitesse de la caméra** | Vitesse de vol (50 à 20000 unités/s) | 1000 |
| **Multiplicateur de vitesse rapide** | Appliqué tant que `Maj` est enfoncée (1 à 20) | 3 |
| **Sensibilité de la souris** | Vitesse de rotation (0,02 à 1) | 0,15 |
| **Inverser l'axe Y de la souris** | Inverse l'axe vertical | Désactivé |
| **Touches de la caméra** | Auto (clavier), AZERTY ou QWERTY | Auto |
| **Distance d'affichage** | Distance d'affichage 3D (2048 à 262144) | 32768 |
| **Zoomer vers le curseur** | Zoom à la molette vers le curseur plutôt que vers le centre de la vue | Activé |
| **Contour des objets sous la souris** | Entoure ce qui est sous le pointeur dans la vue 3D | Désactivé |

Les clés sont dans `settings.toml` sous `[view]` et `[workflow]`, voir la [Référence des paramètres](../reference/settings.md).

## Astuces

- Perdu dans la map ? Sélectionnez quelque chose et faites `Ctrl+Maj+E`, ou **Réinitialiser** dans l'outil caméra.
- Si vous venez de Hammer ou de Hammer++, choisissez ce schéma pour retrouver `Espace` et les glissers à deux boutons.
- Tournez la molette en vol pour régler une vitesse adaptée à la taille de la pièce.

## FAQ

**ZQSD ne fait rien.** Les touches de vol ne marchent que clic droit maintenu (ou en vue souris), avec le pointeur sur la vue 3D.

**Mon clavier AZERTY vole dans le mauvais sens.** Choisissez **Touches de la caméra : Auto (clavier)**, qui se base sur la position des touches (ZQSD), ou forcez **AZERTY**.

**La caméra est trop lente dans une grande map.** Tournez la molette vers l'avant en vol, montez **Vitesse de la caméra**, ou maintenez `Maj`.

**Ctrl+molette change ma grille au lieu de zoomer.** Réglez **Ctrl + molette en 2D** sur **Zoome toutes les vues 2D**.

## Pages liées

- [Outil de sélection](selection-tool.md)
- [Tour de l'interface](../getting-started/interface-tour.md)
- [Modes de rendu et affichage](render-modes-display.md)
- [Vue joueur](player-view.md)
- [Grilles](grids.md)
- [Raccourcis clavier](keyboard-shortcuts.md)
- [Référence des paramètres](../reference/settings.md)

---
[Retour à l'index de la documentation](../README.md)
