[Documentation](../README.md) > Fonctionnalités > Particules | [English](../../en/features/particles.md)

# Particules

Hammer Pro a un navigateur de particules qui liste tous les systèmes de particules des fichiers de particules montés (`.pcf`), et un aperçu des particules qui joue les effets des entités `info_particle_system` dans les vues 3D, comme Hammer++.

## Navigateur de particules

### Ouvrir le navigateur

- **Affichage > Panneaux > Navigateur de particules** ajoute l'onglet **Particules** au panneau de droite (ou le retire). Enregistré dans `show_particle_browser` de `[browser]` (désactivé par défaut).
- Le bouton de parcours d'une clé de particules (`effect_name` de `info_particle_system`) dans le panneau d'entité l'ouvre en mode choix.
- Pas de raccourci par défaut. Avec le préréglage de touches Hammer++ (**Paramètres > Clavier**), c'est `Maj+F11`.
- Le bouton `⊞` (**Ouvrir dans une fenêtre**) sort le navigateur dans une fenêtre flottante ; `⊟` (**Remettre dans le panneau**) le remet en place.

### Options

| Contrôle | Effet | Défaut |
|---|---|---|
| **Grille** / **Liste** | Grille de vignettes, ou une ligne par système avec une petite vignette, son fichier de particules et son badge de source. Gardé pendant la session. | Grille |
| Curseur de taille (grille seulement) | **Taille des vignettes** des grilles de particules et de modèles, de 48 à 256 points par pas de 8. `tile_size` dans `[browser]`. | 96 |
| **Rechercher des systèmes de particules (!mot exclut)** | Garde les systèmes dont le nom ou le fichier contient tous les mots ; `!mot` exclut. | |
| Liste des fichiers de particules | **Tous les fichiers de particules**, ou les systèmes d'un seul fichier. | Tous |
| Pastilles de source **Tout**, **Jeu**, **Addons**, **Workshop** | Ne gardent que les systèmes d'un type de source (une pastille n'apparaît que si le contenu l'a). | Tout |
| **Contenu de Garry's Mod uniquement** | Seulement ce que tous les joueurs de Garry's Mod possèdent. Partagé avec les navigateurs de modèles et de sons (`gmod_only` dans `[browser]`). | Désactivé |
| **Systèmes : N** | Nombre de systèmes listés (« Lecture des fichiers de particules... » pendant le chargement). | |

Les vignettes montrent le matériau du système (un `?` s'il n'en a pas ou s'il manque). Survolez un système pour voir son nom, son fichier, son matériau et sa source.

### Utiliser un système

- Un clic sélectionne un système.
- Double-clic (ou `Entrée` avec le pointeur sur la liste) : en mode choix, écrit le système dans la clé ; sinon l'applique aux entités `info_particle_system` sélectionnées.
- En bas : le nom sélectionné, son fichier et sa source, **Appliquer à la sélection (N)** (met le système dans `effect_name` des `info_particle_system` sélectionnées, une seule étape d'annulation) et **Copier le nom**.

### Choisir pour une clé

Une clé de particules du panneau d'entité (voir [Propriétés des entités](entity-properties.md)) a un bouton `▾` qui liste les systèmes de particules du contenu du jeu avec leur fichier (filtrable, et vous pouvez taper une valeur absente de la liste), et un bouton de parcours (« Choisir dans le navigateur de particules (double-cliquez sur un système) »).

En mode choix, le navigateur affiche « Choix pour *clé* : double-cliquez sur un élément, ou sélectionnez-le puis Utiliser. » avec la valeur actuelle sélectionnée. Double-clic, `Entrée` ou **Utiliser** écrit le système (une seule étape d'annulation) et revient sur l'onglet **Entité** ; **Annuler** quitte le mode choix.

## Aperçu des particules dans les vues

### L'activer

| Option | Où | Effet | Défaut |
|---|---|---|---|
| **Afficher les particules** | **Affichage > Options d'affichage**, et **Paramètres > Nav mesh et particules** | Joue les effets des `info_particle_system` proches de la caméra dans les vues 3D. `show_particles` dans `[lab]`. | Activé |
| **Particules de la sélection uniquement** | Aux mêmes endroits | Ne joue que les effets des systèmes de particules sélectionnés. `particles_selected_only` dans `[lab]`. | Désactivé |
| **Aperçu des effets de volume** | Aux mêmes endroits | Reproduction texturée et animée de `func_smokevolume`, `env_embers`, `func_dustcloud` et `func_dustmotes` (voir plus bas). `show_entity_fx` dans `[lab]` ; action `toggle_entity_fx`. | Activé |

### Ce qui est joué

- Les définitions viennent des fichiers de particules du contenu monté : d'abord les fichiers du manifeste de particules de la map (`maps/<map>_particles.txt`), puis le catalogue de particules du contenu. Ils sont lus en arrière-plan, avec l'aspect de leurs matériaux (`$additive`, `$overbrightfactor`, `$addself`, tailles de fondu, la feuille de sprites de la texture de base). Les systèmes enfants sont chargés aussi.
- Chaque effet est joué quel que soit son état de départ (`start_active`), à ses points de contrôle : l'entité elle-même avec ses angles, et les entités nommées par ses clés `cpoint1` à `cpoint63`. Les effets continus démarrent déjà lancés (1,5 seconde d'avance).
- Pour que le coût reste faible quelle que soit la taille de la map, seuls les effets à moins de 4096 unités de la caméra sont joués, 24 au plus en même temps (les plus proches d'abord), avec 3000 particules au plus en tout et 400 par effet.
- Les particules sont dessinées par le moteur de rendu avec l'image de leur feuille de sprites, face à l'écran, verticales, à plat ou étirées (traînées, cordes), en mélange ou en additif. L'aperçu reste une approximation : certains opérateurs des systèmes de particules ne sont pas simulés.
- Dans la [vue joueur](player-view.md) avec le monde compilé, ce sont les systèmes de particules de la map compilée qui sont joués.

Survolez ou sélectionnez un `info_particle_system` dans une vue 3D pour voir le nom de son effet au-dessus, avec son état : rien de plus s'il est joué, « (à l'arrêt : trop loin ou trop nombreux) », « (chargement...) » ou « (introuvable dans les fichiers de particules) ».

## Effets de volume dans les vues

`func_smokevolume`, `env_embers`, `func_dustcloud` et `func_dustmotes` sont dessinés par le moteur en jeu ; Hammer Pro les reproduit dans les vues 3D, activé par **Aperçu des effets de volume**. Ils sont dessinés comme les fichiers de particules : des billboards texturés avec le vrai sprite du contenu du jeu monté (son `.vmt` décide du mélange additif ou alpha, du facteur de surluminosité, de `$alpha` et de la taille de fondu ; les images des planches de sprites des matériaux `SpriteCard` sont fondues comme dans le jeu), par le même moteur de rendu. L'étendue est la boîte des brushes de l'entité (une entité ponctuelle a une boîte de 96 unités). Aucune boîte n'est tracée autour du volume : l'entité est surlignée comme n'importe quelle autre entité sélectionnée.

| Entité | Clés utilisées | Sprite | Aspect |
|---|---|---|---|
| `func_smokevolume` | `Color1`, `Color2` (chaque volute prend une couleur entre les deux), `ParticleDrawWidth` (taille des volutes), `ParticleSpacingDistance` (les volutes sont sur une grille de cet écart), `Density` (opacité), `MovementSpeed` (balancement), `RotationSpeed` (rotation), `Material` | `particle/particle_smokegrenade` (ou la clé `Material`) | Volutes douces sur une grille, qui tournent et se balancent lentement |
| `env_embers` | `rendercolor`, `renderamt`, `density` (étincelles par seconde), `lifetime`, `speed` | `effects/ember_swirling001` | Petites étincelles additives qui montent en ondulant, sortent du brush, scintillent et s'estompent |
| `func_dustcloud` | `Color`, `SizeMin`, `SizeMax` (demi-taille du sprite), `SpawnRate`, `LifetimeMin`, `LifetimeMax`, `SpeedMax` (vitesse aléatoire sur chaque axe), `FallSpeed`, `Alpha`, `DistMax` | `particle/particle_smokegrenade` | Grands grains ténus qui apparaissent et disparaissent en fondu, s'estompent avec la distance et disparaissent en quittant le brush |
| `func_dustmotes` | Mêmes clés | `particle/sparkles` | Le même mouvement avec le sprite d'étincelles |

Les valeurs par défaut suivent le FGD : `func_dustcloud` utilise `SizeMin` 100, `SizeMax` 200 et `Alpha` 30 (une brume blanche dense), `func_dustmotes` utilise 10, 20 et `Alpha` 255. Le jeu éclaire la poussière avec les lightmaps ; l'aperçu la montre non éclairée. L'état de départ est respecté : `env_embers` demande le drapeau `Start On` (1) et les entités de poussière sont masquées par `StartDisabled` (les entrées `TurnOn` et `TurnOff` n'existent qu'en jeu et ne sont pas simulées). La poussière `Frozen` est dessinée immobile. Quand un sprite manque dans le contenu, le sprite connu suivant est essayé, puis le nom est conservé (le matériau manquant s'affiche). L'aperçu est borné : seules les entités à moins de 3072 unités de la caméra (ou `DistMax` si plus petit) sont montrées, 24 au plus à la fois (les plus proches d'abord), 300 particules chacune (160 volutes pour la fumée) et 2400 en tout, avec moins de particules quand l'entité est loin. Les entités masquées ne sont pas montrées. Pour `info_particle_system`, ce sont les vrais fichiers de particules qui sont joués (voir plus haut).

## Fenêtre d'aperçu des particules

Le bouton `▶` à côté du champ **Particle System Name** (`effect_name`) d'un `info_particle_system` (aussi **Affichage > Panneaux > Aperçu des particules...**, action `particle_preview`) ouvre une fenêtre avec :

- une zone de recherche (tous les mots doivent figurer dans le nom ou le fichier ; `!mot` exclut) et la liste de tous les systèmes de l'index du contenu et des fichiers de particules, avec leur fichier en infobulle ;
- une vue animée du système sélectionné, jouée depuis le vrai fichier de particules avec la même simulation, le même moteur de rendu et les mêmes sprites que les vues 3D (un rendu hors écran du moteur affiché comme une image, 400 particules au plus) : sprites texturés, mélange additif et alpha, images des planches de sprites, traînées et cordes. Glissez pour orbiter, molette pour zoomer, double-clic pour recentrer la caméra. Les points de contrôle sont à l'origine sans angles, comme un `info_particle_system` sans clés `cpoint`. **Pause**/**Lecture**, **Relancer** et **Tourner** (la rotation automatique) sont dessous, avec le nombre de particules et les opérateurs non simulés. L'ouvrir avec l'action `particle_preview` pendant qu'un `info_particle_system` est sélectionné démarre sur son effet ;
- **Assigner à N entités** (ou **Utiliser pour ce champ** quand la fenêtre vient d'un champ) : écrit le nom dans la clé (une étape d'annulation). Un double-clic sur un système fait pareil. **Copier le nom** le copie.

Fermer la fenêtre pendant qu'elle attend un choix annule ce choix.

## Astuces

- Sur une map avec beaucoup d'effets, activez **Particules de la sélection uniquement** pour regarder un effet à la fois.
- Via MCP, `search_particles` cherche dans les systèmes du contenu et `list_particle_systems` donne l'état de chaque `info_particle_system` de la map (voir la [référence des outils MCP](../../MCP.md), en anglais).

## FAQ

**Un effet indique « introuvable dans les fichiers de particules ».** Aucun fichier de particules monté ne définit ce nom. Vérifiez l'orthographe, montez le jeu ou l'addon qui le contient, ou ajoutez son fichier au manifeste de particules de la map.

**Un effet n'est pas joué alors qu'il est dans le champ.** Il est à plus de 4096 unités, ou 24 effets plus proches sont déjà joués. Rapprochez la caméra.

**Pourquoi le navigateur ne liste rien ?** Le contenu du jeu n'est pas chargé, ou la recherche, le filtre de fichier, les pastilles de source ou **Contenu de Garry's Mod uniquement** masquent tout.

## Pages liées

- [Propriétés des entités](entity-properties.md)
- [Modes de rendu et affichage](render-modes-display.md)
- [Vue joueur](player-view.md)
- [Navigateur de modèles](model-browser.md)
- [Navigateur de sons](sound-browser.md)
- [Performances de l'éditeur](../troubleshooting/performance.md)

---
[Retour à l'index de la documentation](../README.md)
