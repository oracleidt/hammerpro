[Documentation](../README.md) > Fonctionnalités > Simulateur physique | [English](../../en/features/physics-tool.md) | [Русский](../../ru/features/physics-tool.md)

# Simulateur physique

Sélectionnez des brushes, des props ou des ragdolls, puis choisissez **Outils > Simulateur physique** ou l'icône de balance dans la barre des outils. Les commandes apparaissent dans le panneau de droite.

| Commande | Effet |
| --- | --- |
| Lecture / Espace | Démarrer ou reprendre la simulation. |
| Pause / Espace | Suspendre la simulation. |
| Un pas | Avancer d'un pas de physique pendant la pause. |
| Conserver les positions | Appliquer les positions et rotations à la carte en une seule opération annulable. |
| Réinitialiser / Échap | Annuler la simulation et retrouver les positions initiales. |
| Clic gauche en 3D | Saisir l'objet au point visé ; maintenir pour le déplacer, relâcher pour le lâcher. |
| Clic droit en 3D | Tirer sur l'objet avec une impulsion au point touché. |
| Clic du milieu en 3D | Produire une explosion au point visé. |

Les brushes, les displacements et les props non sélectionnés servent de surfaces de collision. Les modèles utilisent leurs formes de collision `.phy`. Les ragdolls utilisent leurs différents corps physiques et les limites de leurs articulations. Les volumes d'eau appliquent la flottabilité et la résistance au mouvement.

La carte reste intacte pendant la lecture. Changer d'outil conserve les positions ; **Réinitialiser** ou **Échap** les abandonne. **Ctrl+Z** pendant la lecture annule l'aperçu ; après conservation, il annule le placement complet. Modifier ou changer la carte annule un aperçu devenu obsolète.

## Réglages

| Réglage | Valeur par défaut |
| --- | --- |
| Gravité | 600 unités Source/s² |
| Densité de l'air | 2 kg/m³ |
| Vitesse de simulation | 1 |
| Vitesse de tir | 300 unités/s |
| Vitesse d'explosion | 500 unités/s |
| Rayon d'explosion | 256 unités |

Les réglages sont conservés entre les sessions. Les positions physiques sont continues et ne s'alignent pas sur la grille d'édition. Les poses des ragdolls utilisent la clé Source `angleOverride` pour rester enregistrées dans le VMF.

Les modèles sans fichier de collision exploitable sont signalés puis ignorés. Les displacements restent des surfaces de collision et ne deviennent pas des corps mobiles.

Pour une pose rapide sans lecture interactive, le [placement physique](transform-align.md#placement-physique) reste disponible.

## MCP et scripts

`set_tool_options` avec l'outil `physics` pilote le simulateur sans la souris (aussi depuis les fichiers `--script` : `tool physics`, puis `tool_option simulate 3`) :

| Option | Effet |
| --- | --- |
| `simulate` | Lance d'un coup autant de secondes de physique (0,01 à 120, multipliées par l'échelle de temps) sur les brushes, props et ragdolls sélectionnés, puis met en pause. Le premier appel construit la simulation depuis la sélection ; les suivants la prolongent. 0 ne fait rien (c'est la valeur relue). La carte n'est pas modifiée. |
| `simulated` | Lecture seule : les secondes déjà simulées. |
| `paused` | `off` laisse la simulation se jouer dans l'éditeur (l'outil doit être actif), `on` la fige. |
| `keep` | `on` applique les positions, rotations et poses de ragdolls à la carte en une seule étape d'annulation, comme **Garder les positions**. |
| `reset` | `on` abandonne la simulation, comme **Réinitialiser**. |
| `bodies` | Lecture seule : le nombre d'objets simulés. |

La gravité, la densité de l'air, l'échelle de temps et les valeurs de tir et d'explosion sont les réglages `physics.gravity`, `physics.air_density`, `physics.time_scale`, `physics.shoot_speed`, `physics.explosion_speed` et `physics.explosion_radius` (`set_setting`). Exemple : sélectionnez quelques props, puis `set_tool_options` `{"tool": "physics", "options": {"simulate": 3}}` et `{"tool": "physics", "options": {"keep": true}}` les fait tomber au sol en une seule étape d'annulation.
