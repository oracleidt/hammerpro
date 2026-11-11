[Documentation](../README.md) > Fonctionnalités > Pavé tactile | [English](../../en/features/trackpad.md) | [Русский](../../ru/features/trackpad.md)

# Pavé tactile

Hammer Pro s'utilise sur le pavé tactile d'un portable sans souris : le défilement à deux doigts déplace les vues au lieu de zoomer, le pincement zoome et `Espace` + glisser remplace le bouton du milieu. Les réglages sont sur la page **Saisie** des paramètres (**Outils > Options**, section Application).

## Mode pavé tactile

**Mode pavé tactile** (`[input] trackpad_mode`) a trois valeurs :

| Valeur | Comportement |
|---|---|
| **Auto** (par défaut) | Active les gestes du pavé tactile dès qu'un geste de pavé est vu (un défilement fin ou latéral, un pincement), et les désactive au premier cran d'une molette de souris. La page indique ce qui a été détecté. Une molette de souris ordinaire ne l'active jamais. |
| **Activé** | Tout défilement est lu comme un geste de pavé tactile, même une molette de souris. À utiliser quand Auto ne reconnaît pas votre pavé. |
| **Désactivé** | Le comportement souris, exactement comme avant : la molette zoome, le bouton du milieu déplace. |

Avec une molette haute résolution qui envoie des fractions latérales, Auto peut la prendre pour un pavé tactile : choisissez **Désactivé**.

## Gestes

| Geste | Vues 2D | Vue 3D |
|---|---|---|
| Défilement à deux doigts | Déplacer la vue (la map suit les doigts) | Faire orbiter la caméra autour du point au centre de la vue, ou la déplacer sur les côtés et vers le haut ou le bas (**Défilement à deux doigts en 3D**) |
| Pincer, ou `Ctrl` + défilement | Zoomer au curseur (vers le curseur quand **Zoomer vers le curseur** est actif) | Avancer ou reculer la caméra |
| `Alt` + défilement | Agit comme la molette : l'outil l'utilise d'abord (nombre de blocs, segments de courbe...), sinon zoom | Agit comme la molette (déplace la caméra) |
| `Maj` + défilement | Agit comme `Maj` + molette | Agit comme `Maj` + molette (déplacement plus rapide) |
| `Espace` + glisser | Déplacer la vue | Regarder autour |
| `Espace` + `Maj` + glisser | | Déplacer la caméra |
| Toucher | Clic ; un léger mouvement du doigt (jusqu'à 9 points) compte encore comme un clic | Pareil |
| Toucher à deux doigts | Clic droit : menu contextuel | Pareil |
| Défilement pendant le vol (bouton droit tenu ou vue souris) | | Change la vitesse de la caméra, comme la molette |

- `Ctrl` + défilement avec une vraie molette change toujours la taille de grille (ou zoome toutes les vues 2D, **Ctrl + molette en 2D**) quand les gestes du pavé sont désactivés. Avec **Activé**, `Ctrl` + défilement zoome ; les touches `[` et `]` changent la grille.
- Le défilement est fluide : les fractions s'additionnent, et une image ne déplace jamais de plus de 240 points ni ne zoome de plus de 3 crans, donc un geste rapide ne fait pas sauter la vue à travers la map.
- En 3D, les gestes marchent avec tous les schémas de contrôle. `Espace` + glisser en 3D regarde autour même avec le schéma Hammer Pro quand les gestes du pavé sont actifs.
- Les listes, panneaux et fenêtres défilent à deux doigts comme d'habitude dans tous les modes.

## Réglages

| Réglage | Défaut | Effet |
|---|---|---|
| **Mode pavé tactile** (`input.trackpad_mode`) | `Auto` | `Auto`, `On` ou `Off` |
| **Défilement à deux doigts en 3D** (`input.trackpad_scroll_3d`) | `Orbit` | `Orbit` tourne autour du point au centre de la vue, `Move` fait glisser la caméra |
| **Vitesse de défilement** (`input.trackpad_pan_speed`) | 1.00 | 0,25 à 4, multiplie le déplacement |
| **Vitesse du zoom par pincement** (`input.trackpad_zoom_speed`) | 1.00 | 0,25 à 4, multiplie le zoom du pincement et de `Ctrl` + défilement |
| **Vitesse d'orbite** (`input.trackpad_orbit_speed`) | 1.00 | 0,25 à 4, multiplie l'orbite 3D |

## Par MCP

- `set_setting` avec `input.trackpad_mode` (`"Auto"`, `"On"`, `"Off"`), `input.trackpad_scroll_3d` (`"Orbit"`, `"Move"`) et les trois vitesses ; `get_settings` les lit.
- `ui_scroll` avec `trackpad: true` envoie un défilement à deux doigts de `dx`, `dy` points ; avec `pinch` (par exemple 1.25) il envoie un pincement. Exemple : `{"view": "top", "trackpad": true, "dx": 40, "dy": 0}` déplace la vue de dessus quand les gestes du pavé sont actifs.
- Scripts : `trackpad <pos> <dx> <dy> [mods]` et `pinch <pos> <facteur>` (voir [Commandes de script](../reference/script-commands.md)).

Voir aussi : [Caméra et navigation](camera-navigation.md), [Raccourcis clavier](keyboard-shortcuts.md), [Fenêtre des paramètres](settings-dialog.md).
