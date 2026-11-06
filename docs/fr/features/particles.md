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
- Seuls les effets à moins de 4096 unités de la caméra jouent, au plus 24 à la fois (les plus proches en premier). Chaque système PCF conserve son `max_particles`, jusqu’à la limite du jeu de 5 000 ; les systèmes parents et enfants partagent le budget de l’effet. Le navigateur et l’éditeur de particules utilisent la même limite par système. Utilisez **Particules de la sélection uniquement** pour examiner un effet dense sur une map lourde.
- `Color Lit Per Particle` échantillonne l’éclairage compilé ou calculé de la map à la position de chaque particule ; `Color Random` peut utiliser la teinte de son point de contrôle. Sans calcul d’éclairage, l’éditeur utilise un éclairage maximal. Un nouveau calcul actualise les teintes en cache des nouvelles particules sans redémarrer celles qui existent.
- `Cull relative to Ray Trace Environment` utilise les volumes physiques des `func_precipitation` à particules et des `func_precipitation_blocker`, séparément des collisions ordinaires. L’éditeur calcule ces volumes depuis les brushes ; la vue joueur compilée lit leur physique dans le BSP. Les toits du monde ne sont pas ajoutés à ces environnements de rayons nommés.
- Le placement sur un modèle utilise le groupe de hitboxes de l’entité liée au point de contrôle choisi. `Movement Lock to Bone` suit sa pose affichée, y compris les poses du simulateur de physique ; le modèle est chargé avant le démarrage de l’effet. `Position In CP Hierarchy` utilise les liens `cpointN_parent`, la croissance, la courbure et les biais de distance. Le placement sur un chemin utilise une courbe de Bézier quadratique et l’orientation du point de contrôle choisi.
- `Noise Scalar` et `Noise Vector` échantillonnent le bruit spatial natif à la position de chaque particule dans le monde, avec l’échelle des coordonnées et le mode additif. Une particule immobile conserve la même valeur de bruit en vieillissant ; le bruit vectoriel utilise les décalages natifs des canaux. `Velocity Noise` utilise l’instant de création de la particule, les décalages spatiaux et temporels, les réglages de valeur absolue et d’inversion par axe, ainsi que l’orientation locale du point de contrôle choisi.
- `Remap Noise to Scalar` et `Lifetime Pre-Age Noise` utilisent un bruit spatial et temporel avec les options de valeur absolue et d’inversion. Les initialisateurs de valeurs s’exécutent avant les modificateurs : le prévieillissement utilise donc la durée de vie déjà définie et peut atteindre 100 %. Le premier initialisateur d’un attribut est prioritaire ; les modificateurs s’exécutent ensuite dans leur ordre d’origine.
- `ALPHA` et `ALPHA2` restent indépendants et se multiplient lors du rendu, avec la précision d’opacité et le seuil de visibilité du jeu. Modifier un attribut scalaire préserve les autres attributs ; les durées de vie configurées ne sont pas raccourcies par une limite de temps de l’éditeur.
- L’émission continue répartit les dates de naissance des particules dans chaque image. `emit noise` utilise le bruit temporel de Source et sa particule initiale ; les facteurs liés aux points de contrôle et au nombre de particules parentes sont pris en compte. Les naissances refusées lorsque le système est plein sont abandonnées, sans s’accumuler pour une rafale ultérieure. Les émissions instantanées conservent la date de naissance configurée lorsqu’une image franchit leur délai de démarrage.
- Les particules sont dessinées par le moteur de rendu avec l'image de leur feuille de sprites, face à l'écran, verticales, à plat ou étirées (traînées, cordes), en mélange ou en additif. L'aperçu reste une approximation : certains opérateurs des systèmes de particules ne sont pas simulés.
- Dans la [vue joueur](player-view.md) avec le monde compilé, ce sont les systèmes de particules de la map compilée qui sont joués.

Survolez ou sélectionnez un `info_particle_system` dans une vue 3D pour voir le nom de son effet au-dessus, avec son état : rien de plus s'il est joué, « (à l'arrêt : trop loin ou trop nombreux) », « (chargement...) » ou « (introuvable dans les fichiers de particules) ».

## Effets de volume dans les vues

`func_smokevolume`, `env_embers`, `func_dustcloud` et `func_dustmotes` sont des entités brush dessinées par le moteur du jeu. **Aperçu des effets de volume** les reproduit approximativement dans les vues 3D avec les bornes de leurs brushes visibles et les matériaux montés. Une entité ponctuelle sans brushes ne possède pas de volume valide et ne produit pas cet aperçu. La sélection est surlignée normalement.

Ces quatre effets nécessitent des bornes en coordonnées du monde. La sauvegarde et l’export réparent une ancienne `origin` placée au centre du brush sans déplacer le volume ; les trains et les positions des entités ponctuelles conservent leur origine. Après la mise à jour d’une map affectée, sauvegardez-la et effectuez une **compilation complète de la géométrie**, même avec un compilateur externe. Une mise à jour des entités seules ne répare pas les bornes déjà inscrites dans un ancien BSP. Les matériaux des effets sont inclus dans les ressources préparées et à empaqueter.

| Entité | Clés utilisées | Sprite | Aspect |
|---|---|---|---|
| `func_smokevolume` | `Color1`, `Color2` (chaque volute prend une couleur entre les deux), `ParticleDrawWidth` (taille des volutes), `ParticleSpacingDistance` (les volutes sont sur une grille de cet écart), `Density` (opacité), `MovementSpeed` (balancement), `RotationSpeed` (rotation), `Material` | `particle/particle_smokegrenade` (ou la clé `Material`) | Volutes douces sur une grille, qui tournent et se balancent lentement |
| `env_embers` | `rendercolor`, `renderamt`, `density` (étincelles par seconde), `lifetime`, `speed` | `particle/fire` | Petites braises émises selon les angles, de taille 1 à 0 ; influence optionnelle de env_wind |
| `func_dustcloud` | `Color`, `SizeMin`, `SizeMax` (demi-taille du sprite), `SpawnRate`, `LifetimeMin`, `LifetimeMax`, `SpeedMax` (vitesse aléatoire sur chaque axe), `FallSpeed`, `Alpha`, `DistMax` | `particle/sparkles` | Grands sprites avec fondu selon la durée de vie et la distance ; peuvent quitter le brush |
| `func_dustmotes` | Mêmes clés | `particle/sparkles` | Sprites dont la taille suit la profondeur, avec les unités des motes de Source |

Les valeurs par défaut suivent le FGD. Un `env_embers` sans nom démarre activé ; avec un nom, il nécessite **Start On** (indicateur 1) ou une entrée en jeu. La poussière respecte `StartDisabled` et `Frozen` ; les entrées/sorties en jeu ne sont pas simulées. Un nom `Material` explicite est conservé même s’il manque, pour ne pas masquer le problème avec un matériau standard. Les sprites intégrés peuvent utiliser leur remplacement connu. L’aperçu est limité à 24 effets proches, 2048 particules chacun (2048 bouffées de fumée), 8192 au total, dans un rayon de 3072 unités ou le `DistMax` inférieur. Les brushes masqués ne contribuent pas au volume de l’aperçu. Le jeu utilise la densité et l’espacement d’origine : un espacement très petit peut donc coûter beaucoup plus que l’aperçu limité.

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

Les cordes de la map (`move_rope` et `keyframe_rope`) réagissent aussi lorsque **Afficher les cordes** et **Simuler les cordes** sont activés, sauf si **Disable Wind** est activé. Le vent n'est simulé que sur les nœuds de corde à moins de 1000 unités de la caméra. `func_smokevolume` conserve son mouvement propre. Les braises `env_embers` suivent le vent lorsque `AffectedByWind` vaut 1. Les systèmes de particules restent soumis aux opérateurs pris en charge par leur simulateur d'aperçu.

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

Les opérateurs Source connus utilisent les types de paramètres attendus par le jeu. Par exemple, `use animation rate as FPS` est un booléen et `spin_rate_degrees` un entier. Les valeurs numériques sont converties uniquement si elles correspondent au type demandé ; les valeurs ambiguës sont refusées sans modifier le document. La validation signale les types incorrects des fichiers importés, sans réécrire leurs paramètres. Les paramètres inconnus sont conservés.

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

### Lecture du fondu

Sélectionnez un `env_fade` et cliquez sur **Lecture** au-dessus des propriétés. L'aperçu agit sur les vues 3D et utilise `rendercolor`, `renderamt`, `duration`, `holdtime`, **Fade In** et **Modulate**. Fade In maintient d'abord la couleur, puis la dissipe ; Fade Out atteint la couleur, puis la maintient. La lecture s'arrête après la durée et le maintien, même avec **Stay Out**. **Arrêter** retire immédiatement l'aperçu. La map et les sorties ne sont pas modifiées. Changer de map, supprimer ou masquer l'entité retire l'effet visible.

### Contrôle des ombres

Un `shadow_control` visible affiche les ombres projetées des modèles dans les vues 3D texturées et éclairées. L'aperçu utilise les angles (ou l'ancienne direction), la couleur, la distance et `disableallshadows`. Il ne nécessite pas de soleil et ne modifie pas l'environnement lumineux. Les modèles masqués ou dont les ombres sont désactivées ne projettent pas ces ombres. Cette projection par carte d'ombres reste une approximation de la méthode utilisée par Source.

### Jets de vapeur et fidélité

`env_steam` et l'ancien `env_steamjet` utilisent **Initial State** (`InitialState`) : **Off** (0) par défaut en jeu, **On** (1) pour émettre dès le démarrage. **Emissive** (indicateur 1) contrôle l'éclairage séparément. Les angles, la couleur/alpha, Speed, SpreadSpeed, JetLength, Rate, StartSize, EndSize et RollSpeed sont respectés.

**Heat Wave** (`type` 1) utilise `sprites/heatwave` pour réfracter la scène derrière le jet, avec la normale du matériau, `$refractamount` signé, `$bluramount` et `$nofog`. La profondeur des objets devant le jet est vérifiée avec et sans anticrénelage. Examinez la réfraction dans la vue 3D d'une map : le navigateur isolé ne contient qu'un fond uni à déformer.

La fumée et la vapeur normale utilisent l'éclairage de la scène en mode **Éclairé** et avec l'éclairage compilé ; **Emissive** ajoute leur couleur à cette lumière. Les autres modes gardent l'affichage d'édition. La poussière et les motes utilisent la profondeur de vue pour le fondu et la taille des motes. Les particules PCF et d'entités sont triées ensemble par profondeur ; l'alpha du matériau est appliqué une seule fois. Leur nombre est conservé tant que le budget le permet, sans réduction automatique liée à l'éloignement.

Les braises diminuent en couleur, alpha et taille, sans rotation et avec un ralentissement progressif. La fumée respecte largeur/opacité et fondu près de la caméra ; la poussière peut quitter son brush et respecte les tailles/débits/alphas nuls. Les budgets, les séquences aléatoires, les échanges de fumée, les collisions et les opérateurs PCF non pris en charge peuvent encore différer ; vérifiez le résultat final en jeu.

## Pages liées

- [Propriétés des entités](entity-properties.md)
- [Modes de rendu et affichage](render-modes-display.md)
- [Vue joueur](player-view.md)
- [Navigateur de modèles](model-browser.md)
- [Navigateur de sons](sound-browser.md)
- [Performances de l'éditeur](../troubleshooting/performance.md)

---
[Retour à l'index de la documentation](../README.md)
