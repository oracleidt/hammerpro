[Documentation](../README.md) > Fonctionnalités > Outil de texture | [English](../../en/features/texture-tool.md) | [Русский](../../ru/features/texture-tool.md)

# Outil de texture

L'outil d'application de texture correspond au mode "Face Edit" de Hammer : vous choisissez des faces dans la vue 3D, puis le panneau **Édition des faces** change leur matériau, leur échelle, leur décalage, leur rotation, leur échelle de lightmap, leur justification, leur alignement, leurs groupes de lissage et leurs displacements, sur toutes les faces sélectionnées à la fois. Il contient aussi l'[éditeur UV](uv-editor.md).

## Activer l'outil

| Comment | Raccourci |
|---|---|
| **Outils > Application de texture**, ou son bouton dans la barre d'outils | `Maj+A` |

Si des objets sont sélectionnés quand vous prenez l'outil, leurs faces deviennent la sélection de faces ; quand vous le quittez, la sélection de faces est vidée et les objets sélectionnés avant reviennent. Le dock de droite affiche le panneau **Édition des faces** pendant que l'outil est actif, puis l'onglet **Textures**.

## Sélectionner des faces

La sélection se fait dans la vue 3D. La face sous la souris est soulignée ; les faces sélectionnées ont une surbrillance rouge dans la vue 3D et un contour dans les vues 2D.

| Souris dans la vue 3D | Résultat |
|---|---|
| Clic | Sélectionne la face (remplace la sélection). |
| `Ctrl+clic` | Ajoute ou retire la face. |
| `Maj+clic` | Sélectionne toutes les faces du solide. `Ctrl+Maj+clic` les ajoute. |
| Bouton gauche maintenu en balayant | Sélectionne les faces survolées l'une après l'autre (`Ctrl` ou `Maj` : ajoutées à la sélection). |
| Clic dans le vide | Vide la sélection de faces. |
| `Alt+clic` | Pipette : le matériau de la face devient le matériau actuel. |
| Clic droit | Applique le matériau actuel à la face. |
| `Alt+clic droit` | Enroule : copie le matériau de la première face sélectionnée sur la face cliquée et prolonge sa texture par-dessus l'arête commune. Sélectionnez d'abord la face source. |
| `Échap` | Vide la sélection de faces. |

La barre d'état rappelle ces touches. **Édition > Sélection de faces précédente** (`Alt+Z`) et **Édition > Sélection de faces suivante** (`Alt+Y`), ainsi que les boutons **◀ Sélection précédente** et **Sélection suivante ▶** en haut du panneau, naviguent dans vos sélections de faces précédentes.

## Le panneau Édition des faces

La première ligne indique le nombre de faces sélectionnées (et combien sont des displacements). Sans face sélectionnée, le panneau affiche seulement **Aucune face sélectionnée**.

### Matériau

Le cadre du matériau montre le matériau des faces sélectionnées. Si elles en utilisent plusieurs, il affiche **(valeurs multiples)** avec jusqu'à quatre vignettes et le nombre de matériaux (survolez pour la liste).

| Commande | Effet |
|---|---|
| **Parcourir...** | Ouvre le navigateur de textures : le prochain matériau choisi est appliqué aux faces sélectionnées, puis le panneau revient. |
| **Modifier le matériau** | Ouvre l'[éditeur de matériaux](material-editor.md) sur le matériau des faces sélectionnées (sur tous si les faces en utilisent plusieurs). |
| **Appliquer l'actuel** | Applique le matériau actuel du navigateur aux faces sélectionnées. |
| Bouton image (**En faire le matériau actuel**) | Fait du matériau des faces le matériau actuel (seulement si elles en partagent un). |
| Déposer un matériau sur le cadre | Applique le matériau glissé aux faces sélectionnées. |

### Valeurs

| Champ | Effet |
|---|---|
| **Échelle** X / U et Y / V | Échelle de la texture sur chaque axe (0 est refusé). |
| **Décalage** X / U et Y / V | Décalage de la texture en texels. |
| **Rotation** | Rotation de la texture en degrés. **-90** et **+90** font tourner la texture de chaque face d'un quart de tour. |
| **Échelle de lightmap** | Taille des luxels des faces (1 à 1024, plus petit est plus fin). |

Un champ qui diffère entre les faces affiche **(valeurs multiples)** ; saisir ou glisser impose cette valeur à toutes les faces. Un glissement continu fait une seule étape d'annulation. Le nombre de décimales et les pas de glissement se règlent dans **Paramètres > Entités et navigateurs > Application de textures (Face Edit)** :

| Paramètre | Clé dans `[entities]` | Par défaut |
|---|---|---|
| **Décimales affichées** (0 à 6) | `texture_decimals` | 4 |
| **Pas du décalage** (texels par point, 0,01 à 64) | `texture_shift_step` | 1 |
| **Pas de l'échelle** (par point, 0,0005 à 1) | `texture_scale_step` | 0,005 |

### Éditeur UV

La section **Éditeur UV** dessine la face sur sa texture et permet de la déplacer, la mettre à l'échelle et la tourner à la souris, de modifier sommets et arêtes, et de déplacer les autres couches du matériau. **Ouvrir dans une fenêtre séparée** la met dans sa propre fenêtre. Voir [Éditeur UV](uv-editor.md).

### Justifier et aligner

| Commande | Effet |
|---|---|
| **Justifier** : **Gauche**, **Droite**, **Haut**, **Bas** | Place ce bord de la texture sur le même bord de la face. |
| **Centrer** | Place le centre de la texture au centre de la face. |
| **Ajuster** | Met la texture à l'échelle pour qu'elle couvre la face exactement une fois. |
| **Traiter comme une seule** | Justifie et ajuste toutes les faces sélectionnées comme une seule surface au lieu de chaque face séparément. Désactivé par défaut. |
| **Aligner** : **Monde** | Projette la texture depuis l'axe du monde le plus proche (alignement World de Hammer). |
| **Face** | Pose la texture dans le plan de la face (alignement Face de Hammer). |
| **Vue** | Aligne les textures sur la caméra 3D. |
| **Répéter** U, V et **Ajuster** | Met la texture à l'échelle pour qu'elle se répète U fois et V fois sur la face (0,01 à 256, 1 et 1 par défaut). Tient compte de **Traiter comme une seule**. |

### Groupes de lissage

Les boutons **1** à **24** activent ou désactivent les groupes de lissage des faces sélectionnées (l'éclairage lisse entre les faces d'un même groupe). Un numéro dans la couleur d'avertissement n'est défini que sur une partie des faces : cliquez-le pour l'appliquer à toutes.

### Displacement

| Commande | Effet |
|---|---|
| **Créer, puissance :** **2**, **3**, **4** | Transforme les faces sélectionnées en displacements de cette puissance (les faces doivent avoir 4 côtés). |
| **Supprimer** | Remet les displacements sélectionnés en faces normales. |

Pour sculpter, voir [Outil displacement](displacement-tool.md).

### Sélection

| Commande | Effet |
|---|---|
| **Marquer** | Sélectionne toutes les faces de la map qui utilisent les mêmes matériaux que la sélection. |
| **Masquer la surbrillance** | Masque la surbrillance rouge des faces sélectionnées (pour juger la texture). |
| **Tout désélectionner** | Vide la sélection de faces. |

## Commandes associées

| Commande | Menu | Raccourci |
|---|---|---|
| **Appliquer la texture actuelle** | **Outils** | `Maj+T` (avec n'importe quel outil : faces sélectionnées, ou toutes les faces des objets sélectionnés) |
| **Verrouillage des textures** | **Outils**, barre d'outils | `Maj+L` |
| **Verrouillage de l'échelle des textures** | **Outils**, barre d'outils | `Maj+K` |
| **Rechercher et remplacer des textures...** | **Outils** | `Ctrl+Maj+R` (aucune touche avec les préréglages Hammer et Hammer++) |

Les verrouillages de texture sont décrits dans [Opérations sur les brushes](brush-operations.md).

## Astuces

- Sélectionnez toute une pièce avec `Maj+clic` sur chaque mur, puis **Justifier > Ajuster** avec **Traiter comme une seule** pour étirer une seule texture sur l'ensemble.
- `Alt+clic` sur une face qui vous plaît, puis clic droit sur d'autres faces pour peindre son matériau.
- Pour prolonger une texture dans un angle, sélectionnez la face texturée, puis `Alt+clic droit` sur la suivante.
- **Marquer** sur une sélection montre toutes les autres faces qui utilisent les mêmes matériaux, prêtes pour une modification groupée.

## FAQ

**Les clics ne font rien dans les vues 2D.** Les faces se choisissent seulement dans la vue 3D ; les vues 2D montrent le contour des faces sélectionnées.

**Pourquoi l'échelle refuse-t-elle 0 ?** Une échelle nulle rendrait la texture infinie ; le champ l'ignore.

**La surbrillance rouge cache ma texture.** Activez **Masquer la surbrillance**.

**Comment revenir à ma sélection de faces précédente ?** `Alt+Z` (**Sélection de faces précédente**), et `Alt+Y` pour revenir.

## Pages liées

- [Navigateur de textures](texture-browser.md)
- [Éditeur UV](uv-editor.md)
- [Rechercher et remplacer des textures](find-replace-textures.md)
- [Outil displacement](displacement-tool.md)
- [Opérations sur les brushes](brush-operations.md)
- [Raccourcis clavier](keyboard-shortcuts.md)
- [Référence des outils MCP](../../MCP.md) (en anglais : `get_faces`, `select_faces`, `set_face_texture`, `justify_texture`, `fit_texture`, `align_texture_to_view`, `wrap_texture`, `select_by_material`)

---
[Retour à l'index de la documentation](../README.md)
