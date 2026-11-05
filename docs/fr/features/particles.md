[Documentation](../README.md) > Fonctionnalités > Particules | [English](../../en/features/particles.md) | [Русский](../../ru/features/particles.md)

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

### Vent de env_wind

Avec **Prévisualiser les effets de volume**, `func_dustmotes` et `func_dustcloud` réagissent aux contrôleurs `env_wind` de la map. **Affected By Wind** doit être activé (par défaut), **Frozen** désactivé et **Start Disabled** désactivé pour afficher l'effet. Désactiver **Affected By Wind** conserve le mouvement propre à l'entité et sa vitesse de chute.

| Paramètre de env_wind | Aperçu |
|---|---|
| **Angles** | Direction horizontale initiale du vent, selon le yaw |
| **Min/Max normal speed** | Vitesse de base entre les rafales |
| **Min/Max gust speed** | Vitesse pendant les rafales |
| **Min/Max gust delay** | Délai aléatoire entre les rafales |
| **Gust Duration** | Durée de chaque rafale |
| **Max gust dir change** | Changement de direction au début des rafales |
| **Radius** | `-1` agit sur toute la map ; un rayon positif agit dans cette sphère |

Les contrôleurs locaux sont prioritaires dans leur rayon ; sinon le premier contrôleur global est utilisé. Les contrôleurs masqués sont exclus. Modifier un contrôleur ou les options d'une entité met directement l'aperçu à jour. Les poussières accélèrent vers le vent horizontal actuel en conservant leur vitesse de chute verticale, selon le comportement du [Source SDK](https://github.com/ValveSoftware/source-sdk-2013/blob/master/src/game/client/c_func_dust.cpp).

Sélectionner un `env_wind` affiche une flèche animée de direction et sa vitesse actuelle dans la vue 3D, avec **Vent normal** ou **Rafale**. Les aides de rayon existantes montrent sa zone d'action. L'aperçu utilise une séquence de rafales déterministe, distincte de la séquence aléatoire du jeu, et n'exécute pas les entrées/sorties des entités.

Les cordes de la map (`move_rope` et `keyframe_rope`) réagissent aussi lorsque **Afficher les cordes** et **Simuler les cordes** sont activés, sauf si **Disable Wind** est activé. Le vent n'est simulé que sur les nœuds de corde à moins de 1000 unités de la caméra. `func_smokevolume` et `env_embers` conservent leur mouvement propre ; ajouter un `env_wind` ne leur applique pas automatiquement du vent. Les systèmes de particules restent soumis aux opérateurs pris en charge par leur simulateur d'aperçu.

Via MCP, `get_entity` et `set_entity_keyvalues` lisent et modifient le contrôleur et les clés `AffectedByWind`, `Frozen` et `NoWind`. `set_setting` règle `lab.show_entity_fx`, `helpers.show_ropes` et `helpers.simulate_ropes`.

## Fenêtre d'aperçu des particules

Le bouton `▶` à côté du champ **Particle System Name** (`effect_name`) d'un `info_particle_system` (aussi **Affichage > Panneaux > Aperçu des particules...**, action `particle_preview`) ouvre une fenêtre avec :

- une zone de recherche (tous les mots doivent figurer dans le nom ou le fichier ; `!mot` exclut) et la liste de tous les systèmes de l'index du contenu et des fichiers de particules, avec leur fichier en infobulle ;
- une vue animée du système sélectionné, jouée depuis le vrai fichier de particules avec la même simulation, le même moteur de rendu et les mêmes sprites que les vues 3D (un rendu hors écran du moteur affiché comme une image, 400 particules au plus) : sprites texturés, mélange additif et alpha, images des planches de sprites, traînées et cordes. Glissez pour orbiter, molette pour zoomer, double-clic pour recentrer la caméra. Les points de contrôle sont à l'origine sans angles, comme un `info_particle_system` sans clés `cpoint`. **Pause**/**Lecture**, **Relancer** et **Tourner** (la rotation automatique) sont dessous, avec le nombre de particules et les opérateurs non simulés. L'ouvrir avec l'action `particle_preview` pendant qu'un `info_particle_system` est sélectionné démarre sur son effet ;
- **Assigner à N entités** (ou **Utiliser pour ce champ** quand la fenêtre vient d'un champ) : écrit le nom dans la clé (une étape d'annulation). Un double-clic sur un système fait pareil. **Copier le nom** le copie.

Fermer la fenêtre pendant qu'elle attend un choix annule ce choix.

## Éditeur de particules intégré

Ouvrez **Autres > Éditeur de particules** (action `particle_editor`). Cet éditeur crée et modifie les systèmes contenus dans un PCF. Le navigateur de particules et sa petite fenêtre d'aperçu restent disponibles pour choisir les effets existants du jeu.

### Fichiers et systèmes

**Nouveau** crée un document vide. **Ouvrir un PCF** lit un fichier externe. Le champ de chemin accepte un chemin absolu ou monté, comme `particles/fire_01.pcf` ; **Charger** l'ouvre et **PCF montés** liste les fichiers indexés du jeu, filtrés par ce champ. L'ouverture d'un fichier monté ne remplace pas l'archive du jeu : choisissez un chemin externe avec **Enregistrer sous**. Les dialogues de fichiers et dossiers fonctionnent indépendamment de la fenêtre de l'éditeur.

Le panneau gauche filtre les systèmes par nom. Sélectionnez un système pour afficher ses propriétés, opérateurs et enfants. Saisissez un nom puis utilisez **Créer** ou **Renommer** ; **Dupliquer** copie le système et son graphe d'opérateurs sous un nom unique. **Supprimer** retire le système et ses références enfants. Le document possède son historique **Annuler** et **Rétablir**, distinct de celui de la map. La croix du titre ou le bouton **Fermer** de la barre d'outils ferme l'éditeur. Fermer, remplacer ou vider un document modifié ouvre une confirmation modale au premier plan. **Annuler**, Échap ou un clic hors de la confirmation revient à l'édition sans perdre les modifications. Choisir **Abandonner les modifications** à la fermeture restaure le dernier document enregistré (ou le document vide pour un nouveau fichier), efface l'historique abandonné et ferme l'éditeur. Quitter Hammer Pro ne redemande pas confirmation pour ces modifications abandonnées et ne rouvre pas l'éditeur. Un astérisque dans le titre marque les modifications non enregistrées ; revenir par annulation à l'état enregistré le retire.

L'enregistrement produit un DMX binaire version 5. Le document conserve les identités d'éléments, références, ordre des opérateurs, attributs typés, références externes et données binaires au lieu de reconstruire un fichier depuis la seule partie comprise par l'aperçu. Un opérateur non simulé reste donc présent après ouverture et enregistrement. Les références incorrectes, valeurs invalides et fichiers dépassant la limite de taille PCF sont rejetés.

### Opérateurs et paramètres

Chaque système contient les groupes **Émetteurs**, **Initialisateurs**, **Opérateurs**, **Générateurs de forces**, **Contraintes** et **Rendus**. La bibliothèque propose les fonctions Source courantes avec leurs paramètres initiaux. Ouvrir ou importer un PCF ajoute aussi ses définitions d'opérateurs à la bibliothèque de la session. Il est possible de saisir directement le nom d'une autre fonction Source.

Sélectionnez un opérateur dans l'arbre pour modifier tous ses paramètres enregistrés, y compris `functionName`. **Modifier l'élément référencé** ouvre les propriétés des éléments DMX imbriqués. **Copier** et **Coller** transfèrent les opérateurs entre systèmes. **Monter**, **Descendre** ou glisser une ligne sur une autre de la même catégorie modifie l'ordre d'exécution. Chaque opération est annulable. Ajoutez ou retirez des paramètres avec leurs noms Source exacts ; les paramètres inconnus conservent leurs valeurs et types.

| Type de paramètre | Éditeur |
|---|---|
| Entier, décimal, booléen, texte | Valeur numérique glissable, case ou champ de texte |
| Couleur | Sélecteur de couleur et alpha |
| Vecteur 2, 3 ou 4 | Composantes numériques séparées |
| Référence d'élément | Liste des éléments du document, avec référence vide |
| Tableaux | Valeurs indexées, ajout et retrait |
| Tableaux de décimaux | Valeurs numériques et courbe glissable |
| Tableaux de couleurs | Étapes de couleur et aperçu du dégradé |
| Angles, quaternion, matrice | Composantes numériques avec conservation du type DMX |
| Autres données binaires | Éditeur hexadécimal conservant le type DMX |

Les champs numériques et textuels sont validés à la fin du geste ou de la saisie, pour que l'annulation revienne à la valeur précédente. L'enregistrement et le remplacement d'un document valident les saisies en attente avant de continuer. Les incohérences minimum/maximum, indices de points de contrôle invalides et autres problèmes apparaissent dans **Validation**.

Les opérateurs `radius scale` et `alpha fade and decay` proposent aussi des points glissables durée/valeur qui modifient leurs vrais champs Source de temps et de taille/alpha. L'opérateur `color fade` affiche un dégradé entre la couleur de base du système et sa couleur de fondu. Les paramètres Source individuels restent modifiables sous ces contrôles.

Les propriétés du système comprennent le matériau, les nombres maximal et initial de particules, le rayon, la couleur et la boîte englobante. **Choisir le matériau du sprite**, à côté d'un champ de matériau, permet de chercher dans les matériaux montés et leurs vignettes. L'aperçu utilise le shader Source et la planche de sprites existants du matériau.

**Systèmes enfants** lie un autre système du document avec un délai de démarrage. Sélectionnez sa ligne pour modifier la référence, le délai et la fin d'émission, ou retirer l'enfant. Les cycles d'enfants sont rejetés. Les autres paramètres de points de contrôle et d'opérateurs sont accessibles par leurs champs enregistrés.

### Aperçu animé

Le panneau droit utilise la même simulation et le même moteur GPU que la vue 3D de la map. Il affiche le système sélectionné et ses enfants avec les matériaux montés, les mélanges alpha/additif, les images d'animation, les traînées et les cordes pris en charge par ce moteur.

Glissez pour orbiter, utilisez la molette pour zoomer et double-cliquez pour réinitialiser la caméra. **Pause/Lecture**, **Relancer**, la vitesse de 0,05x à 4x, **Grille** et la couleur de fond affectent uniquement l'aperçu. Ajoutez des points de contrôle, saisissez leurs coordonnées ou glissez leurs repères CP dans la vue. Les statistiques indiquent le nombre de particules et le temps CPU mesuré de simulation. L'aperçu limite les particules actives à 30000 et affiche un avertissement si cette limite est atteinte.

L'éditeur PCF peut conserver et modifier davantage d'opérateurs que la simulation n'en implémente. L'aperçu et la validation listent explicitement les fonctions non simulées. Elles restent enregistrées pour Source ; vérifiez l'effet sauvegardé dans le jeu cible avant de considérer son aspect comme définitif. Un rendu par modèle ou dans l'espace écran n'est pas remplacé par une approximation en sprites.

### Modèles, import et export d'addon

Les modèles **Feu**, **Fumée**, **Étincelles**, **Pluie** et **Explosion** ajoutent des systèmes modifiables au document courant. Les imports répétés prennent des noms uniques. **Importer des systèmes** lit un autre PCF, copie ses systèmes et remappe les références de leur graphe.

**Exporter l'addon** choisit la racine d'un addon et écrit `particles/<fichier>.pcf` avec `particles/particles_manifest.txt`. Les entrées existantes du manifeste sont conservées. La fenêtre d'enregistrement fournit les appels `game.AddParticles` et `PrecacheParticleSystem` à copier dans le Lua de l'addon. L'export du PCF n'empaquette pas ses matériaux de sprites : ils doivent exister dans le contenu du jeu ou être inclus dans l'addon.

**Assigner à la sélection** écrit le nom du système sélectionné dans `effect_name` des entités `info_particle_system` sélectionnées, ou dans un champ de particules en attente de choix. La modification de map compte pour une étape d'annulation de la map. Enregistrez/exportez le PCF modifié et rendez-le disponible au jeu pour qu'il puisse charger l'effet nommé.

### Accès MCP

| Outil | Fonction |
|---|---|
| `get_particle_document` | Lire le document, ses systèmes, paramètres typés et l'état de l'éditeur |
| `list_particle_operators` | Découvrir les catégories, fonctions et paramètres initiaux |
| `particle_editor` | Effectuer les opérations de fichier, système, opérateur, paramètre, enfant, historique, aperçu et assignation |
| `capture_particle_preview` | Capturer l'aperçu animé via le moteur GPU partagé |

Les opérations de `particle_editor` comprennent `new`, `open`, `read`, `import`, `save`, `export`, `validate`, `create`, `rename`, `duplicate`, `delete`, `select`, `preset`, `add_operator`, `remove_operator`, `move_operator`, `copy_operator`, `paste_operator`, `set_parameter`, `remove_parameter`, `add_child`, `remove_child`, `undo`, `redo`, `preview` et `assign`. Elles utilisent le même document et le même historique que la fenêtre. La [référence MCP générée](../../MCP.md), en anglais, donne les arguments et types de valeurs actuellement acceptés. La fenêtre doit être ouverte et rendue avant la capture de son aperçu.

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
