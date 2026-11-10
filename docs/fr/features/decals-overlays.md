[Documentation](../README.md) > Fonctionnalités > Décals et overlays | [English](../../en/features/decals-overlays.md) | [Русский](../../ru/features/decals-overlays.md)

# Décals et overlays

L'outil décal pose des entités `infodecal` et l'outil overlay des entités `info_overlay` sur les faces des brushes, toutes deux avec le matériau actuel. Les décals et overlays sont dessinés sur leurs faces dans les vues comme le moteur les applique, et les overlays peuvent être déformés et étalés sur plusieurs faces.

## Ouvrir les outils

| Outil | Menu | Raccourci par défaut |
| --- | --- | --- |
| **Outil décal** | **Outils > Outil décal** | `Maj+D` |
| **Outil overlay** | **Outils > Outil overlay** | `Maj+O` |

Les deux outils sont aussi dans la palette d'outils. Ils ne marchent que dans la vue 3D : cliquer dans une vue 2D affiche le message **Cliquez sur une face dans la vue 3D** et le curseur indique que la vue n'est pas utilisable.

Le matériau posé est le matériau actuel (celui choisi dans le [navigateur de textures](texture-browser.md)). S'il n'y en a pas, c'est le matériau par défaut des paramètres, sinon `dev/dev_measuregeneric01b`.

## Outil décal

Passez le curseur sur une face de la vue 3D : un aperçu montre le décal à sa vraie taille et dans sa vraie orientation, avec le nom du matériau et sa taille à côté du curseur. Cliquez pour poser un `infodecal` dont la clé `texture` est le matériau actuel.

- Le décal est posé 1 unité devant la surface, comme dans Hammer, et reçoit les keyvalues par défaut de `infodecal` dans le FGD.
- Sa taille est la taille de la texture en pixels multipliée par le `$decalscale` du matériau (64 x 64 tant que le matériau n'est pas encore chargé).
- Son orientation suit les règles du moteur : sur les sols et plafonds, la droite de la texture est +X, sur les murs le bas de la texture est -Z.
- Le nouveau décal devient la sélection. Maintenez `Maj` en cliquant pour garder la sélection actuelle.
- Chaque décal est une étape d'annulation.

### Options

| Option | Effet | Par défaut |
| --- | --- | --- |
| Pastille du matériau | Vignette et nom du matériau actuel. Cliquez dessus pour afficher l'onglet du navigateur de textures. | |
| **Taille : L x H** | Taille du décal en unités (taille de la texture). Lecture seule. | |
| **Magnétisme de grille** | Aligne la position du décal sur la grille de la surface (`Alt` en cliquant : libre). | Décoché |

## Outil overlay

Passez le curseur sur une face de la vue 3D : un aperçu montre l'overlay avec le nom du matériau et sa taille. Ensuite :

| Geste | Effet |
| --- | --- |
| Clic sur une face libre | Pose un `info_overlay` avec le matériau actuel, centré sur le point cliqué (sur le plan de la face). Il devient la sélection. |
| `Maj`+clic sur une face libre | Pose un overlay en gardant la sélection actuelle. |
| Clic sur un overlay | Le sélectionne. Si des overlays se superposent, c'est le dernier du fichier (celui dessiné au-dessus) qui est pris. |
| `Maj`+clic sur un overlay | L'ajoute à la sélection ou l'en retire. |
| `Ctrl`+clic sur une face | Ajoute la face aux faces des overlays sélectionnés, ou l'en retire. Un overlay garde toujours au moins une face (**Un overlay garde au moins une face**). Une étape d'annulation. |
| Glisser une poignée de coin | Déforme l'overlay sélectionné : le coin bouge sur le plan de l'overlay, aligné sur la grille et sur les arêtes et coins de ses faces, et reste sur ces faces. Une étiquette affiche la clé du coin (`uv0` à `uv3`) et sa valeur. Une étape d'annulation par glisser. |
| `Alt`+glisser un coin | Déplace le coin hors grille. |
| `Maj`+glisser un coin | Colle le coin à un coin d'un autre overlay à quelques pixels près, pour des routes et des bordures sans raccord visible. |
| `Échap` pendant un glisser | Annule le déplacement. |

Un nouvel overlay reçoit les clés qu'écrit Hammer pour que le compilateur le construise : `material`, `sides` (la face cliquée), `StartU`/`StartV` à 0 et `EndU`/`EndV` à 1, `BasisOrigin`, `BasisU`, `BasisV`, `BasisNormal` (la même base que Hammer) et les quatre coins `uv0` à `uv3`, à une demi-taille de texture autour du centre. La taille vaut la taille de la texture multipliée par `$decalscale`, comme pour les décals, et se change ensuite en glissant les coins. Les poignées de coin s'affichent pour 32 overlays sélectionnés au plus.

### Options

| Option | Effet | Par défaut |
| --- | --- | --- |
| Pastille du matériau | Vignette et nom du matériau actuel. Cliquez dessus pour afficher l'onglet du navigateur de textures. | |
| **Taille : L x H** | Taille d'un nouvel overlay en unités (taille de la texture, modifiable ensuite). Lecture seule. | |
| **Magnétisme de grille** | Aligne le centre d'un nouvel overlay sur la grille de la face (`Alt` : libre). | Décoché |

## Affichage dans les vues

- Les décals (`infodecal` et toute classe avec l'aide FGD `decal()`) sont projetés sur les faces de brush sous leur origine, comme les décals statiques en jeu. Ils sont redessinés quand l'entité ou un brush en dessous change. En vue joueur, un `infodecal` nommé reste caché jusqu'à ce qu'une entrée `Activate` l'atteigne, comme en jeu (un décal sans nom est appliqué au chargement de la map).
- Les overlays (`info_overlay`, `info_overlay_transition` et toute classe avec l'aide FGD `overlay()` ou `overlay_transition()`) sont dessinés avec leur matériau, projetés sur chaque face de leur liste `sides` (displacements compris) et découpés par elles. Les vues 2D montrent le contour découpé. Un overlay est sélectionné et masqué avec son entité. Un `info_overlay_transition` (une ligne de rivage) est compilé dans les overlays d'eau de la map, que le jeu dessine sur la surface de l'eau : une bande par face du rivage (`sides`) qui coupe le plan d'une face d'eau (`sides2`), `Width1` unités côté terre et `Width2` unités sur l'eau. Les bandes se suivent le long du rivage quel que soit l'ordre des faces dans `sides`, donc les coordonnées de texture de longueur courent sans raccord d'un bout à l'autre du rivage, et l'éditeur dessine exactement ces bandes. Chaque bande est écrite à l'endroit, son axe de longueur gardant l'eau à sa gauche : le jeu ne construit rien pour un overlay d'eau à l'axe inversé (mesuré, même avec `r_overlaywireframe 1`), donc une bande inversée enregistrée par le Hammer de Valve est remise à l'endroit avec les mêmes coins et coordonnées de texture, et une bande enregistrée sans axes est ignorée avec un avertissement. Une transition enregistrée par le Hammer de Valve contient aussi les bandes calculées par Hammer (son bloc `overlaytransition`) : elles servent quand les faces du rivage ne donnent aucune bande, par exemple dans une map décompilée. Une transition dont le rivage ne coupe pas son eau donne un avertissement dans le journal.
- **Affichage > Options d'affichage > Afficher les overlays** active ou coupe les overlays (activé par défaut, pas de raccourci par défaut). Le même interrupteur se trouve dans **Paramètres > Aides > Overlays et cordes**, enregistré sous `show_overlays` dans la table `[helpers]` de `settings.toml`.

## Les overlays suivent vos modifications

Le compilateur construit un overlay à partir de ses clés de base, de ses coins et de sa liste de faces, pas de `origin`. Hammer Pro déplace ces clés avec l'entité quand vous la déplacez, la tournez ou la retournez, et met à jour la liste de faces quand l'overlay quitte ses faces. Les overlays collés et dupliqués pointent vers les faces collées, et creuser ou découper un brush met à jour la liste de faces des overlays (et des `env_cubemap`) qui l'utilisaient.

Pour modifier la liste de faces à la main, utilisez le bouton pipette à côté de la clé `sides` dans les [propriétés de l'entité](entity-properties.md) (SmartEdit) : choisissez des faces dans la vue 3D (`Ctrl`+clic pour plusieurs, `Entrée` pour finir), `Ctrl`+clic sur le bouton pour ajouter à la liste.

## Astuces

- **Édition > Définir la distance maximale...** écrit `fademaxdist` sur les overlays pour qu'ils disparaissent en fondu avec la distance.
- [Vérifier les problèmes](check-problems.md) trouve les overlays dont les faces n'existent plus (avec une correction qui les retire des listes) et ceux posés sur plus de 64 faces. Une map est limitée à 8192 overlays.
- Le visgroup automatique **Overlays et décals** contient tous les `info_overlay` et `infodecal` (voir [Visgroups](visgroups.md)).
- Outils MCP : `list_decals`, `create_decal`, `list_overlays`, `create_overlay`, `set_overlay`. Voir la [référence des outils MCP](../../MCP.md) (en anglais).
- `set_tool_options` avec l'outil `decal` ou `overlay` et l'option `snap` (`on` ou `off`) active ou coupe l'alignement sur la grille de l'outil, comme sa case à cocher.

## FAQ

**Cliquer dans la vue de dessus ne fait rien.** Les deux outils ne posent que sur une face touchée dans la vue 3D.

**Mon décal est énorme (ou minuscule).** La taille vient de la texture et de son `$decalscale`. Utilisez un matériau de décal, ou changez `$decalscale` dans le matériau (voir [Éditeur de matériaux](material-editor.md)).

**Comment étaler un overlay sur deux brushes ?** Sélectionnez-le, faites `Ctrl`+clic sur l'autre face avec l'outil overlay, puis glissez les coins par-dessus.

**Mes overlays ont disparu des vues.** Vérifiez **Affichage > Options d'affichage > Afficher les overlays**.

## Pages liées

- [Navigateur de textures](texture-browser.md)
- [Propriétés des entités](entity-properties.md)
- [Application de texture](texture-tool.md)
- [Modes de rendu et options d'affichage](render-modes-display.md)
- [Vérifier les problèmes](check-problems.md)

---
[Retour à l'index de la documentation](../README.md)
