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
