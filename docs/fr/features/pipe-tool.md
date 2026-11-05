[Documentation](../README.md) > Fonctionnalités > Outil tuyau | [English](../../en/features/pipe-tool.md)

# Outil tuyau

L'outil tuyau construit des tuyaux et des conduits en brushes (pas en props) : des tronçons droits découpés en plusieurs brushes, des coudes à 90 degrés faits de tranches de brushes qui approchent un arc, et des colliers (brides) en option. Le profil est carré ou rond, creux (un tube fait de brushes de paroi) ou plein. Un parcours peut avoir autant de coudes que vous voulez, dans les six directions de la grille : formes en L, en S, en U, en J et parcours qui montent sont possibles. Le résultat est groupé et tient en une seule étape d'annulation.

## L'ouvrir

- **Outils > Outil tuyau**, ou son bouton dans la palette d'outils (groupe création, après l'outil route). Il n'a pas de raccourci par défaut.
- Tout suit la **grille active** : voir [Grilles](grids.md). Le départ se cale sur la grille, les directions sont les axes de la grille (une grille tournée donne donc un tuyau tourné), les longueurs sont arrondies au pas de la grille le long du tuyau, et le rayon et le rayon de coude sont arrondis au plus petit pas de la grille. L'épaisseur de paroi, la taille et la longueur des colliers sont arrondies à un demi pas de grille. Sans magnétisme (le bouton **Magnétisme**), rien n'est arrondi.

## Dessiner un parcours

1. **Cliquez** le départ dans une vue 2D (ou sur une surface dans la vue 3D).
2. **Cliquez** les coins suivants dans la même vue 2D. Le tuyau tourne à chaque clic, et chaque nouveau point est tenu sur un axe de la grille dans le plan de la vue : le segment est droit et sa longueur est un nombre entier de pas de grille. Le tuyau reste dans le plan de cette vue (à la profondeur du premier clic, qui vient de la sélection comme l'outil bloc) ; pour monter, saisissez le parcours (ci-dessous) ou dessinez-le dans la vue de face ou de côté.
3. L'aperçu montre le tuyau fini dans toutes les vues, avec une étiquette donnant la longueur du segment, les coudes et le nombre de brushes. Un segment rouge dit pourquoi le parcours est refusé (un tronçon trop court pour ses coudes, un demi-tour).
4. `Entrée` ou un double clic crée le tuyau. `Retour arrière` retire le dernier point, `Échap` annule.

Un tuyau tourne d'exactement 90 degrés : un demi-tour ou un coin oblique est refusé.

## Saisir un parcours

Le champ **Parcours** de la barre d'options accepte des mouvements après les points cliqués (ou après un seul clic, avec la direction de **Départ**) : un nombre avance de cette longueur, et `L`, `R`, `U`, `D` tournent à gauche, à droite, vers le haut ou vers le bas de 90 degrés. Exemple : `256 L 128 U 64` avance de 256, tourne à gauche, avance de 128, tourne vers le haut, avance de 64. Gauche et droite tournent autour de l'axe haut de la grille, haut et bas inclinent le tuyau. Séparez les mouvements par des espaces ou des virgules.

## Comment les longueurs fonctionnent

Les longueurs se mesurent de coin à coin. Un coude de rayon `Coude` retire ce rayon aux deux tronçons voisins (l'arc commence `Coude` avant le coin et finit `Coude` après). Un tronçon entre deux coudes doit donc faire au moins deux fois `Coude`, et un tronçon au bout du tuyau au moins `Coude`. L'outil indique quel tronçon est trop court.

## Options

| Option | Ce qu'elle fait | Plage | Défaut |
|---|---|---|---|
| **Forme** | Carré, ou rond (un polygone à **Côtés**). | | Carré |
| **Côtés** | Nombre de côtés d'un profil rond. | 5 à 48 | 12 |
| **Rayon** | Moitié de la largeur du tuyau (extérieur). | 1 à 2048 | 32 |
| **Creux**, **Paroi** | Un tube de brushes de paroi autour d'un intérieur vide, et son épaisseur (inférieure au rayon). Décoché : brushes pleins. | | oui, 8 |
| **Brushes** | Brushes le long de chaque tronçon droit. | 1 à 256 | 4 |
| **Coude** | Rayon des coudes à 90 degrés, sur l'axe du tuyau. Il est agrandi s'il est plus petit que le rayon. | | 64 |
| **Tranches** | Tranches de brushes par coude : plus de tranches, un coude plus rond. | 1 à 32 | 4 |
| **Collier tous les** | Un collier entre chaque groupe de N brushes d'un tronçon (0 : aucun). | 0 à 256 | 0 |
| **Taille du collier**, **Longueur du collier** | De combien un collier dépasse et sa longueur. Les colliers sont des anneaux autour du tuyau et ne chevauchent jamais ses brushes. | | 8, 8 |
| **Colliers aux extrémités**, **Colliers aux coudes** | Un collier aux deux bouts du tuyau, ou de chaque côté de chaque coude. | | non |
| **Parcours**, **Départ** | Les mouvements saisis, et la première direction quand le parcours part d'un seul point (`+x -x +y -y +z -z` selon la grille). | | |
| **Échelle de texture**, matériau | Le matériau est le matériau courant. | | |

Un tuyau rond creux avec des coudes a des pièces de paroi non planes ; elles sont construites en brushes convexes valides, avec quelques faces en plus sur les coudes.

## MCP

`create_pipe` construit le même tuyau : `start`, `direction` (`+x` à `-z` selon la grille) ou `direction_vector`, puis `moves` (des nombres et `left`, `right`, `up`, `down`), ou `path` en texte, ou `length` pour un tuyau droit ; et `shape`, `sides`, `radius`, `hollow`, `wall`, `segments`, `segment_length`, `bend_radius`, `bend_slices`, `collar_every`, `collar_size`, `collar_length`, `collar_ends`, `collar_bends`, `snap`, `material`, `collar_material`, `tex_scale`, `group`, `view` (la grille utilisée, top par défaut) et `preview` (décrire le tuyau sans le créer). La réponse donne les nombres de brushes, les tailles effectives après arrondi, et `end` et `end_direction` pour que le tuyau suivant continue depuis là.

`set_tool_options` avec l'outil `pipe` règle les options de l'outil (`shape`, `sides`, `radius`, `hollow`, `wall`, `segments`, `segment_length`, `bend_radius`, `bend_slices`, `collar_every`, `collar_size`, `collar_length`, `collar_ends`, `collar_bends`, `tex_scale`, `path`, `heading`, `depth`, `keep_active`). Voir la [référence des outils MCP](../../MCP.md).

## Astuces

- Gardez le rayon et la grille dans un rapport simple (grille 16, rayon 32) pour que toutes les faces du tuyau soient sur la grille.
- Peu de tranches pour les tuyaux lointains, 6 à 8 pour ceux que les joueurs voient de près.
- Les brushes d'un tuyau sont groupés : sélectionnez le groupe pour le déplacer, ou dégroupez pour modifier un brush.

## Pages liées

- [Grilles](grids.md)
- [Outil courbe](curve-tool.md)
- [Outil route](road-tool.md)
- [Outil bloc](block-tool.md)
