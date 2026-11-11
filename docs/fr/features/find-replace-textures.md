[Documentation](../README.md) > Fonctionnalités > Rechercher et remplacer des textures | [English](../../en/features/find-replace-textures.md) | [Русский](../../ru/features/find-replace-textures.md)

# Rechercher et remplacer des textures

La fenêtre **Rechercher et remplacer des textures** remplace un matériau par un autre sur les faces de toute la map ou de la sélection, par nom exact ou avec des jokers, avec le nombre de faces concernées affiché en direct. Elle peut aussi conserver la taille de la texture dans le monde quand la nouvelle texture a une autre résolution.

## Ouvrir la fenêtre

| Comment | Résultat |
|---|---|
| **Outils > Textures > Rechercher et remplacer des textures...** (`Ctrl+Maj+R`) | Ouvre la fenêtre. Si **Rechercher** est vide, il reçoit le matériau actuel. |
| **Remplacer...** dans l'en-tête du [navigateur de textures](texture-browser.md) | L'ouvre avec le matériau actuel dans **Rechercher**. |
| Clic droit sur une vignette > **Remplacer par celui-ci...** | L'ouvre avec le matériau actuel dans **Rechercher** et celui cliqué dans **Remplacer par**. |

Avec les préréglages de touches Hammer et Hammer++, `Ctrl+Maj+R` correspond à **Rechercher et remplacer dans les entités...** et cette fenêtre n'a pas de raccourci (voir [Raccourcis clavier](keyboard-shortcuts.md)).

## La fenêtre

| Commande | Effet | Par défaut |
|---|---|---|
| **Rechercher** | Nom ou motif de matériau. Les noms sont comparés sans tenir compte de la casse ; les barres obliques, un préfixe `materials/` et une extension `.vmt` sont nettoyés. | matériau actuel |
| Bouton image à côté de **Rechercher** | Clic : ouvre ou ferme le sélecteur de matériaux (voir plus bas). Clic droit : met le matériau actuel dans le champ. | |
| Bouton curseur (**Utiliser le matériau de la sélection**) | Met le premier matériau de la sélection dans **Rechercher**. Grisé sans sélection. | |
| **Remplacer par** | Matériau de remplacement. | vide |
| Bouton image à côté de **Remplacer par** | Même sélecteur, pour le remplacement. | |
| **Dans** : **Toute la map** / **Sélection** | Toutes les faces de la map, ou les faces des objets sélectionnés plus les faces sélectionnées. | **Toute la map** |
| **Correspondance** : **Nom exact** / **Jokers** | Exact : le nom complet. Jokers : `*` correspond à n'importe quel texte, `?` à un caractère. | **Nom exact** |
| **Conserver la taille de la texture dans le monde** | Ajuste l'échelle de texture de chaque face quand la nouvelle texture a une autre résolution, pour qu'elle garde sa taille sur les murs. | désactivé |
| **Remplacer** | Lance le remplacement (aussi `Entrée` dans un champ). Grisé tant qu'aucune face ne correspond ou que **Remplacer par** est vide. | |
| **Fermer** | Ferme la fenêtre (aussi `Échap`). | |

`Tab` passe de **Rechercher** à **Remplacer par**.

Sous les options, la fenêtre indique **N face(s) correspondante(s) (M matériau(x))**, en vert si quelque chose correspond. Avec plusieurs matériaux trouvés, ou en mode jokers, une liste montre chaque matériau avec son nombre de faces et, si **Remplacer par** est rempli, le matériau qu'il deviendra. Taper `*` ou `?` en mode exact affiche **Le nom contient * ou ? : choisissez le mode jokers** et bloque le remplacement.

Le remplacement complet (avec la remise à l'échelle) forme une seule étape d'annulation. La fenêtre affiche ensuite **N face(s) remplacée(s)**. Les faces non remises à l'échelle faute de taille de texture connue sont signalées dans la console.

## Jokers

En mode jokers, un `*` dans **Remplacer par** reprend le texte trouvé par le `*` de **Rechercher** :

| Rechercher | Remplacer par | Résultat |
|---|---|---|
| `dev/*` | `tools/*` | `dev/dev_measurewall01a` devient `tools/dev_measurewall01a` |
| `concrete/concretefloor00?a` | `concrete/concretefloor001a` | toutes les variantes numérotées deviennent le même sol |
| `*` | `dev/dev_measuregeneric01b` | toutes les faces de la portée reçoivent le même matériau |

## Le sélecteur de matériaux

Les boutons image ouvrent un petit sélecteur sous le champ :

- Une ligne **Matériau actuel** en haut : cliquez-la pour utiliser le matériau choisi dans le navigateur de textures.
- Une recherche avec les règles du navigateur (mots, `-mot`, `src:`, `kw:`, `is:`, `shader:`, voir [Navigateur de textures](texture-browser.md)).
- Les puces de section **Utilisés dans la map (n)**, **Récents (n)**, **Favoris (n)**, **Tous (n)**. **Rechercher** s'ouvre sur **Utilisés dans la map**, **Remplacer par** sur **Récents** (sur **Tous** si vous n'avez aucun matériau récent) ; ensuite chaque sélecteur garde la dernière section choisie. Quand une section ne donne rien, **Chercher dans tous les matériaux** passe à **Tous**.
- Une grille de vignettes, cinq par ligne : un clic remplit le champ.

Clavier : la recherche a le focus ; `Bas` entre dans la grille, les flèches s'y déplacent, `Entrée` choisit (le premier résultat si rien n'est surligné), `Échap` ferme seulement le sélecteur (un second `Échap` ferme la fenêtre). Un clic en dehors du sélecteur, ou de nouveau sur son bouton, le ferme.

## Astuces

- Pour retexturer toute une zone, sélectionnez-la, choisissez **Sélection** et remplacez `*` par votre matériau.
- Si vous remplacez une texture 512 par une 1024, cochez **Conserver la taille de la texture dans le monde** pour que les briques gardent leur taille.
- Utilisez d'abord **Marquer** dans le navigateur de textures pour voir où un matériau est utilisé.

## FAQ

**Le compteur reste à 0.** Vérifiez **Dans** (avec **Sélection**, rien de sélectionné signifie aucune face) et le mode de correspondance : en mode exact, le nom complet doit correspondre.

**Peut-on annuler un remplacement ?** Oui, `Ctrl+Z` annule tout le remplacement en une étape.

**Est-ce que ça change les matériaux des entités (overlays, décals, props) ?** Non, seulement les faces des brushes (displacements compris). Pour les textes des clés d'entités, utilisez **Rechercher et remplacer dans les entités...** (voir [Propriétés des entités](entity-properties.md)).

## Pages liées

- [Navigateur de textures](texture-browser.md)
- [Outil de texture](texture-tool.md)
- [Propriétés des entités](entity-properties.md)
- [Annuler et historique](undo-history.md)
- [Référence des outils MCP](../../MCP.md) (en anglais : `replace_materials`, `find_material_usage`)

---
[Retour à l'index de la documentation](../README.md)
