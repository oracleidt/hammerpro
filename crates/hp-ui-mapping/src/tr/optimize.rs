use hp_app::i18n::TrTable;

pub const TR: TrTable = &[
    ("optimize.title", "Optimize the map", "Optimiser la map"),
    (
        "optimize.intro",
        "What is not optimized in this map and what to do about it, sorted by impact. Nothing is changed for you: use Go to, then fix it yourself.",
        "Ce qui n'est pas optimisé dans cette map et quoi faire, trié par impact. Rien n'est modifié à votre place : utilisez Aller à, puis corrigez vous-même.",
    ),
    ("optimize.refresh", "Refresh", "Actualiser"),
    (
        "optimize.refresh_tip",
        "Compile the visibility of the current map in the background and check it again",
        "Compiler la visibilité de la map actuelle en arrière-plan et la vérifier à nouveau",
    ),
    ("optimize.running", "Analyzing the map...", "Analyse de la map..."),
    (
        "optimize.view_tip",
        "Draw the visleaves and the portals between them in the 2D and 3D views",
        "Dessiner les visleaves et les portails entre elles dans les vues 2D et 3D",
    ),
    ("optimize.heat", "Heat map", "Carte de chaleur"),
    ("optimize.error", "The analysis failed: {0}", "L'analyse a échoué : {0}"),
    ("optimize.none", "No analysis yet: press Refresh.", "Pas encore d'analyse : appuyez sur Actualiser."),
    (
        "optimize.stale",
        "The map changed since this analysis: press Refresh.",
        "La map a changé depuis cette analyse : appuyez sur Actualiser.",
    ),
    (
        "optimize.summary",
        "Visleaves: {0}, portals: {1}, areas: {2}. A visleaf sees {3} visleaves on average, {4} at most.",
        "Visleaves : {0}, portails : {1}, zones : {2}. Une visleaf voit {3} visleaves en moyenne, {4} au plus.",
    ),
    (
        "optimize.compile_failed",
        "The visibility compile failed ({0}): only the checks that need no compile are shown.",
        "La compilation de visibilité a échoué ({0}) : seules les vérifications sans compilation sont affichées.",
    ),
    ("optimize.counts", "High impact: {0}, medium: {1}, low: {2}", "Impact fort : {0}, moyen : {1}, faible : {2}"),
    ("optimize.all", "All", "Tout"),
    ("optimize.clean", "Nothing to optimize: well done.", "Rien à optimiser : bien joué."),
    ("optimize.go_to", "Go to", "Aller à"),
    (
        "optimize.go_to_tip",
        "Select the objects and frame them in the views",
        "Sélectionner les objets et les cadrer dans les vues",
    ),
    ("optimize.impact.high", "High", "Fort"),
    ("optimize.impact.medium", "Medium", "Moyen"),
    ("optimize.impact.low", "Low", "Faible"),
    ("optimize.topic.visleaves", "Visleaves", "Visleaves"),
    ("optimize.topic.func_detail", "func_detail", "func_detail"),
    ("optimize.topic.hints", "Hints", "Hints"),
    ("optimize.topic.areaportals", "Areaportals", "Areaportals"),
    ("optimize.topic.lights", "Lights", "Lumières"),
    (
        "optimize.why.visleaves",
        "Why: the engine draws everything the current visleaf can see (its PVS). The more each leaf sees, the more it draws.",
        "Pourquoi : le moteur dessine tout ce que la visleaf actuelle peut voir (son PVS). Plus chaque leaf voit, plus il dessine.",
    ),
    (
        "optimize.why.func_detail",
        "Why: world brushes cut the space into visleaves. Small brushes make many tiny leaves, VVIS gets slow and culling gets worse.",
        "Pourquoi : les brushes du monde découpent l'espace en visleaves. Les petits brushes créent beaucoup de leaves minuscules, VVIS devient lent et le culling empire.",
    ),
    (
        "optimize.why.hints",
        "Why: a hint brush tells VBSP where to cut the leaves, so a leaf at a corner or under a high ceiling stops seeing the next rooms.",
        "Pourquoi : un brush hint indique à VBSP où découper les leaves, pour qu'une leaf dans un angle ou sous un plafond haut ne voie plus les pièces suivantes.",
    ),
    (
        "optimize.why.areaportals",
        "Why: an areaportal splits the map into areas. Through it the engine only draws what is seen through the opening, and nothing at all when its door is closed.",
        "Pourquoi : un areaportal sépare la map en zones. À travers lui le moteur ne dessine que ce qui se voit par l'ouverture, et rien du tout quand sa porte est fermée.",
    ),
    (
        "optimize.why.lights",
        "Why: lights must come from something players see, match their lamp, and not be wasted (blocked, doubled, switchable for nothing).",
        "Pourquoi : la lumière doit venir de quelque chose que le joueur voit, correspondre à sa lampe et ne pas être gaspillée (bloquée, en double, commutable pour rien).",
    ),
    (
        "optimize.f.map_leaks",
        "The map leaks: without a sealed hull there is no visibility at all. Fix the leak first (Map > Check for leaks).",
        "La map fuit : sans coque étanche il n'y a aucune visibilité. Corrigez d'abord la fuite (Map > Vérifier les fuites).",
    ),
    (
        "optimize.f.vis_open_map",
        "An average visleaf sees {0}% of the map ({1} of {2} visleaves): almost nothing is culled. Block the view with walls and corners, use func_detail, hints and areaportals.",
        "Une visleaf moyenne voit {0} % de la map ({1} sur {2} visleaves) : presque rien n'est masqué. Bloquez la vue avec des murs et des angles, utilisez func_detail, hints et areaportals.",
    ),
    (
        "optimize.f.vis_worst_leaf",
        "Visleaf {0} near {1} sees {2} of {3} visleaves ({4} triangles drawn): block the view there or add a hint brush along the edge of the opening.",
        "La visleaf {0} près de {1} voit {2} visleaves sur {3} ({4} triangles dessinés) : bloquez la vue à cet endroit ou ajoutez un brush hint le long du bord de l'ouverture.",
    ),
    (
        "optimize.f.vis_fragmented",
        "{0} of {1} visleaves are tiny slivers cut by small world brushes: turn those brushes into func_detail.",
        "{0} visleaves sur {1} sont de minces éclats découpés par de petits brushes du monde : transformez ces brushes en func_detail.",
    ),
    (
        "optimize.f.vis_slow",
        "VVIS has a lot of work ({0} portals, estimated cost {1}): fewer, simpler visleaves make the compile much faster.",
        "VVIS a beaucoup de travail ({0} portails, coût estimé {1}) : des visleaves moins nombreuses et plus simples accélèrent beaucoup la compilation.",
    ),
    (
        "optimize.f.detail_candidates",
        "Turn these {0} brushes into func_detail ({1}): they are small, do not seal the map and cut {2} visleaves.",
        "Transformez ces {0} brushes en func_detail ({1}) : ils sont petits, ne scellent pas la map et coupent {2} visleaves.",
    ),
    (
        "optimize.f.hint_doorway_top",
        "Add a horizontal hint brush at the top of the opening near {0} ({1} = {2}): the tall leaf above the doorway sees over it. Measured gain: {3}.",
        "Ajoutez un brush hint horizontal en haut de l'ouverture près de {0} ({1} = {2}) : la haute leaf au-dessus de la porte voit par-dessus. Gain mesuré : {3}.",
    ),
    (
        "optimize.f.hint_opening_edge",
        "Add a hint brush that extends the edge of the opening near {0} ({1} = {2}): the leaf at this corner sees down both sides. Measured gain: {3}.",
        "Ajoutez un brush hint qui prolonge le bord de l'ouverture près de {0} ({1} = {2}) : la leaf de cet angle voit des deux côtés. Gain mesuré : {3}.",
    ),
    (
        "optimize.f.areaportals_no_areas",
        "The map has {0} areaportals but one area only: the space goes around them. Close every other way between the areas.",
        "La map a {0} areaportals mais une seule zone : l'espace passe autour. Fermez tous les autres passages entre les zones.",
    ),
    (
        "optimize.f.areaportal_empty",
        "Areaportal {0} has no brush: delete it.",
        "L'areaportal {0} n'a pas de brush : supprimez-le.",
    ),
    (
        "optimize.f.areaportal_brushes",
        "Areaportal {0} at {1} has several brushes: an areaportal must be exactly one brush.",
        "L'areaportal {0} en {1} a plusieurs brushes : un areaportal doit être un seul brush.",
    ),
    (
        "optimize.f.areaportal_thick",
        "Areaportal {0} at {1} is thick: make it a thin sheet in the middle of the opening.",
        "L'areaportal {0} en {1} est épais : faites-en une fine plaque au milieu de l'ouverture.",
    ),
    (
        "optimize.f.areaportal_large",
        "Areaportal {0} at {1} closes a very wide opening: areaportals work in narrow openings (doors, windows, corridor ends).",
        "L'areaportal {0} en {1} ferme une ouverture très large : les areaportals fonctionnent dans les ouvertures étroites (portes, fenêtres, bouts de couloir).",
    ),
    (
        "optimize.f.areaportal_not_sealed",
        "Areaportal {0} at {1} does not touch the world on every side: the areas leak around it. Stretch it into the walls, floor and ceiling.",
        "L'areaportal {0} en {1} ne touche pas le monde de tous les côtés : les zones fuient autour. Étirez-le dans les murs, le sol et le plafond.",
    ),
    (
        "optimize.f.areaportal_touching",
        "Areaportal {0} at {1} touches another areaportal: keep one, or leave real space between them.",
        "L'areaportal {0} en {1} touche un autre areaportal : gardez-en un, ou laissez un vrai espace entre eux.",
    ),
    (
        "optimize.f.areaportal_useless",
        "Areaportal {0} at {1} does not split two areas: delete it, or seal the opening it is in.",
        "L'areaportal {0} en {1} ne sépare pas deux zones : supprimez-le, ou scellez l'ouverture où il se trouve.",
    ),
    (
        "optimize.f.areaportalwindow_target",
        "Areaportal window {0} at {1}: its target (the func_brush that fades in) is missing.",
        "Fenêtre d'areaportal {0} en {1} : sa cible (le func_brush qui apparaît en fondu) est manquante.",
    ),
    (
        "optimize.f.areaportalwindow_fade",
        "Areaportal window {0} at {1} has no FadeDist: it never closes. Set FadeDist and FadeStartDist.",
        "La fenêtre d'areaportal {0} en {1} n'a pas de FadeDist : elle ne se ferme jamais. Réglez FadeDist et FadeStartDist.",
    ),
    (
        "optimize.f.areaportal_missing_door",
        "Areaportal {0} at {1} is linked to a door that does not exist: link it to the door of this opening.",
        "L'areaportal {0} en {1} est lié à une porte qui n'existe pas : liez-le à la porte de cette ouverture.",
    ),
    (
        "optimize.f.areaportal_door_state",
        "Areaportal {0} at {1} does not start in the same state as its door: set StartOpen like the door.",
        "L'areaportal {0} en {1} ne démarre pas dans le même état que sa porte : réglez StartOpen comme la porte.",
    ),
    (
        "optimize.f.areaportal_no_door_link",
        "Areaportal {0} at {1} sits in a door frame but is not linked to the door: set its target to the door.",
        "L'areaportal {0} en {1} est dans un cadre de porte mais n'est pas lié à la porte : mettez la porte en cible.",
    ),
    (
        "optimize.f.areaportal_door",
        "Put an areaportal in the frame of door {0} at {1}: when the door is closed nothing behind it is drawn ({2} and {3} visleaves on each side).",
        "Mettez un areaportal dans le cadre de la porte {0} en {1} : porte fermée, rien derrière n'est dessiné ({2} et {3} visleaves de chaque côté).",
    ),
    (
        "optimize.f.areaportal_opening",
        "Put an areaportal in the opening at {1}: it is the only way between {2} and {3} visleaves, so it separates two areas.",
        "Mettez un areaportal dans l'ouverture en {1} : c'est le seul passage entre {2} et {3} visleaves, il sépare donc deux zones.",
    ),
    (
        "optimize.f.light_blocked",
        "Light {0} at {1} is inside a brush or a prop: its light never leaves it. Move it just under or in front of its lamp.",
        "La lumière {0} en {1} est dans un brush ou un prop : sa lumière n'en sort jamais. Placez-la juste sous ou devant sa lampe.",
    ),
    (
        "optimize.f.light_no_fixture",
        "Light {0} at {1} has no lamp near it (floating light): move it to a lamp ({2}) or add a fixture where it is.",
        "La lumière {0} en {1} n'a aucune lampe à côté (lumière flottante) : déplacez-la vers une lampe ({2}) ou ajoutez une source visible à cet endroit.",
    ),
    (
        "optimize.f.spot_into_wall",
        "Spot light {0} at {1} points into a wall or floor a few units away: aim it at the space it lights.",
        "Le spot {0} en {1} pointe dans un mur ou le sol à quelques unités : orientez-le vers l'espace à éclairer.",
    ),
    (
        "optimize.f.light_dark",
        "Light {0} at {1} has brightness 0 and no name: it lights nothing, delete it.",
        "La lumière {0} en {1} a une luminosité de 0 et pas de nom : elle n'éclaire rien, supprimez-la.",
    ),
    (
        "optimize.f.light_falloff",
        "Light {0}: _zero_percent_distance {1} is not larger than _fifty_percent_distance {2}. Make the zero distance about twice the fifty distance.",
        "Lumière {0} : _zero_percent_distance {1} n'est pas plus grand que _fifty_percent_distance {2}. Mettez la distance zéro à environ deux fois la distance cinquante.",
    ),
    (
        "optimize.f.light_short_distance",
        "Light {0} stops at _distance {1}: the light ends with a hard edge. Use 0 or a larger distance.",
        "La lumière {0} s'arrête à _distance {1} : elle finit avec un bord net. Utilisez 0 ou une distance plus grande.",
    ),
    (
        "optimize.f.light_inconsistent",
        "Light {0} at {1} does not match the other {2} lights of the same lamp ({3}): it uses {4}, they use {5}.",
        "La lumière {0} en {1} ne correspond pas aux {2} autres lumières de la même lampe ({3}) : elle utilise {4}, elles utilisent {5}.",
    ),
    (
        "optimize.f.fixture_no_light",
        "The lamp {0} at {1} looks like a light source but no light is near it: add a light at {2} ({3}).",
        "La lampe {0} en {1} ressemble à une source de lumière mais aucune lumière n'est à côté : ajoutez une lumière en {2} ({3}).",
    ),
    (
        "optimize.f.light_redundant",
        "Lights {0} and {1} are on top of each other with the same colour: keep one with the summed brightness ({2}).",
        "Les lumières {0} et {1} sont l'une sur l'autre avec la même couleur : gardez-en une avec la luminosité additionnée ({2}).",
    ),
    (
        "optimize.f.lights_switchable",
        "{0} switchable (named) lights, the engine allows {1}: only name the lights that really switch.",
        "{0} lumières commutables (nommées), le moteur en accepte {1} : ne nommez que les lumières qui s'allument et s'éteignent vraiment.",
    ),
    (
        "optimize.f.no_light_environment",
        "The map has sky but no light_environment: outdoor areas get no sun and no sky light. Add one.",
        "La map a du ciel mais pas de light_environment : l'extérieur n'a ni soleil ni lumière du ciel. Ajoutez-en un.",
    ),
];
