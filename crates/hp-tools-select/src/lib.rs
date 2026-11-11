pub mod camera;
pub mod gizmo;
pub mod handles;
pub mod input;
pub mod math;
pub mod modal;
pub mod preview;
pub mod screen;
pub mod select;
pub mod stretch;

#[cfg(test)]
mod tests;
#[cfg(test)]
mod tests_qa;
#[cfg(test)]
mod tests_sel;
#[cfg(test)]
mod tests_stretch;

pub use camera::CameraTool;
pub use gizmo::GizmoMode;
pub use math::{Handle, HandleMode};
pub use select::{QueryKind, STRETCH_LABEL, SelectTool, sync_options};

pub const TR: hp_app::i18n::TrTable = &[
    ("seltool.mode.scale", "Scale", "Échelle"),
    ("seltool.mode.rotate", "Rotate", "Rotation"),
    ("seltool.mode.skew", "Skew", "Inclinaison"),
    ("seltool.handles", "Handles:", "Poignées :"),
    (
        "seltool.handles_tip",
        "What the handles of the selection box do in 2D views. Clicking the selection switches mode.",
        "Rôle des poignées du cadre de sélection dans les vues 2D. Cliquer sur la sélection change de mode.",
    ),
    ("seltool.stretch", "3D stretch", "Étirement 3D"),
    (
        "seltool.stretch_tip",
        "Show an arrow on each face of the selected brushes in the 3D view: drag one to stretch the selection along its axis, the opposite face stays. Ctrl: from the center, Alt: no snap, Shift: clone.",
        "Afficher une flèche sur chaque face des brushes sélectionnés dans la vue 3D : glissez-en une pour étirer la sélection sur son axe, la face opposée reste en place. Ctrl : depuis le centre, Alt : sans magnétisme, Maj : cloner.",
    ),
    ("seltool.element", "Pick:", "Choisir :"),
    ("seltool.element.object", "Object", "Objet"),
    ("seltool.element.face", "Face", "Face"),
    ("seltool.element.edge", "Edge", "Arête"),
    ("seltool.element.vertex", "Vertex", "Sommet"),
    (
        "seltool.element_tip",
        "What a click in a 3D view selects: a whole object, a face (for inset I and extrude E), an edge (for bevel and the vertex tool) or a vertex (for the vertex tool). Shift or Ctrl adds or removes. Keys 4, 3, 2, 1.",
        "Ce que sélectionne un clic dans une vue 3D : un objet entier, une face (pour le décalage I et l'extrusion E), une arête (pour le biseau et l'outil de sommets) ou un sommet (pour l'outil de sommets). Maj ou Ctrl ajoute ou retire. Touches 4, 3, 2, 1.",
    ),
    ("seltool.box", "Box:", "Cadre :"),
    ("seltool.box.touching", "Touching", "Touchés"),
    ("seltool.box.inside", "Inside", "Contenus"),
    (
        "seltool.box_tip",
        "Box selection in 2D views takes the objects touching the box, or only the objects fully inside it. Hold Alt while dragging for the other mode.",
        "La sélection par cadre dans les vues 2D prend les objets touchés par le cadre, ou seulement ceux entièrement contenus. Maintenez Alt pendant le glisser pour l'autre mode.",
    ),
    ("seltool.box.centers", "Centers", "Centres"),
    ("seltool.box.confirm", "Enter", "Entrée"),
    (
        "seltool.box.confirm_tip",
        "Like Hammer++: the box stays after release, drag its handles to resize it or drag inside to move it, then Enter selects (Ctrl+Enter adds) and Esc cancels. Off (the default): the selection happens on release.",
        "Comme Hammer++ : le cadre reste après le relâchement, glissez ses poignées pour le redimensionner ou glissez à l'intérieur pour le déplacer, puis Entrée sélectionne (Ctrl+Entrée ajoute) et Échap annule. Désactivé (par défaut) : la sélection se fait au relâchement.",
    ),
    ("seltool.box.enter", "Enter: select", "Entrée : sélectionner"),
    (
        "seltool.hint.box_pending",
        "Enter: select the objects in the box (Ctrl+Enter: add). Drag a handle to resize it, drag inside to move it, Esc: cancel.",
        "Entrée : sélectionner les objets du cadre (Ctrl+Entrée : ajouter). Glissez une poignée pour le redimensionner, glissez à l'intérieur pour le déplacer, Échap : annuler.",
    ),
    (
        "seltool.hint.box_edit",
        "Release to keep the box, then Enter to select. Alt: no snap.",
        "Relâchez pour garder le cadre, puis Entrée pour sélectionner. Alt : sans magnétisme.",
    ),
    (
        "seltool.box.centers_tip",
        "Box and click selection in 2D views only take objects by their center (drawn as small crosses), handy in crowded areas.",
        "La sélection par cadre et par clic dans les vues 2D ne prend les objets que par leur centre (dessiné en petites croix), pratique dans les zones chargées.",
    ),
    ("seltool.gizmo", "Gizmo:", "Gizmo :"),
    ("seltool.gizmo.show", "Show", "Afficher"),
    (
        "seltool.gizmo.show_tip",
        "Show the transform gizmo in the 3D view",
        "Afficher le gizmo de transformation dans la vue 3D",
    ),
    ("seltool.gizmo.move", "Move", "Déplacer"),
    ("seltool.gizmo.rotate", "Rotate", "Pivoter"),
    ("seltool.gizmo.scale", "Scale", "Échelle"),
    ("seltool.gizmo.local", "Local", "Local"),
    (
        "seltool.gizmo.local_tip",
        "Align the gizmo with the angles of a single selected entity instead of the world axes",
        "Aligner le gizmo sur les angles d'une seule entité sélectionnée plutôt que sur les axes du monde",
    ),
    (
        "seltool.gizmo_tip",
        "Drag an arrow to move along an axis, a square to move in a plane, a ring to rotate, a cube to scale. With the pointer over a view, G, R and E move, rotate and scale without the gizmo.",
        "Glissez une flèche pour déplacer sur un axe, un carré pour déplacer dans un plan, un anneau pour pivoter, un cube pour mettre à l'échelle. Avec le pointeur sur une vue, G, R et E déplacent, pivotent et mettent à l'échelle sans le gizmo.",
    ),
    ("seltool.pivot", "Pivot", "Pivot"),
    (
        "seltool.pivot_tip",
        "Show a movable pivot: rotations, scales, the Transform dialog and Paste special turn around it. Drag its cross in a 2D view to move it.",
        "Afficher un pivot déplaçable : les rotations, les mises à l'échelle, la boîte Transformer et le collage spécial tournent autour de lui. Glissez sa croix dans une vue 2D pour le déplacer.",
    ),
    ("seltool.query.class", "Class", "Classe"),
    ("seltool.query.material", "Material", "Matériau"),
    ("seltool.query.hint_class", "Class, e.g. prop_*", "Classe, ex. prop_*"),
    ("seltool.query.hint_material", "Material, e.g. tools/*", "Matériau, ex. tools/*"),
    ("seltool.query.select", "Select", "Sélectionner"),
    (
        "seltool.query.tip",
        "Select every visible entity of this class, or every object using this material (wildcards * and ?). Enter applies, Ctrl adds to the selection.",
        "Sélectionne toutes les entités visibles de cette classe, ou tous les objets utilisant ce matériau (jokers * et ?). Entrée applique, Ctrl ajoute à la sélection.",
    ),
    ("seltool.query.found", "{0} found", "{0} trouvé(s)"),
    ("seltool.query.none", "No match", "Aucun résultat"),
    ("seltool.label.copy", "Copy", "Copie"),
    ("seltool.transform_failed", "Transform failed: {0}", "Échec de la transformation : {0}"),
    ("seltool.key.shift", "Shift", "Maj"),
    ("seltool.key.ctrl", "Ctrl", "Ctrl"),
    ("seltool.key.alt", "Alt", "Alt"),
    (
        "seltool.static_scale_unsupported",
        "Static props cannot be scaled here: the game definitions (FGD) have no uniformscale key.",
        "Les props statiques ne peuvent pas être mis à l'échelle ici : les définitions du jeu (FGD) n'ont pas de clé uniformscale.",
    ),
    (
        "seltool.hint.idle",
        "Click to select (click again: object behind), drag to box select. Ctrl: add, Alt: inside only.",
        "Cliquez pour sélectionner (cliquez à nouveau : objet derrière), glissez pour sélectionner par cadre. Ctrl : ajouter, Alt : objets contenus.",
    ),
    (
        "seltool.hint.selected",
        "Drag to move (Shift: clone, Alt: no snap). Click the selection to switch handles ({0}). Arrows: nudge, Shift+arrows: clone, G, R, E: move, rotate, scale, Esc: deselect.",
        "Glissez pour déplacer (Maj : cloner, Alt : sans magnétisme). Cliquez sur la sélection pour changer de poignées ({0}). Flèches : décaler, Maj+flèches : cloner, G, R, E : déplacer, pivoter, mettre à l'échelle, Échap : désélectionner.",
    ),
    (
        "seltool.hint.3d",
        "Click to select (Ctrl: toggle, click again: object behind), Ctrl+drag to paint a selection, double click for properties. G, R, E: move, rotate, scale.",
        "Cliquez pour sélectionner (Ctrl : inverser, cliquez à nouveau : objet derrière), Ctrl+glisser pour peindre une sélection, double-cliquez pour les propriétés. G, R, E : déplacer, pivoter, mettre à l'échelle.",
    ),
    (
        "seltool.hint.gizmo",
        "Drag to transform (Shift: clone, Alt: no snap).",
        "Glissez pour transformer (Maj : cloner, Alt : sans magnétisme).",
    ),
    (
        "seltool.hint.cycle",
        "Click again or press Page Down for the object behind ({0}/{1}), Page Up for the one in front.",
        "Cliquez à nouveau ou appuyez sur Pg. suiv. pour l'objet derrière ({0}/{1}), Pg. préc. pour celui de devant.",
    ),
    (
        "seltool.hint.band_touching",
        "Box of the objects touching it. Ctrl: add, Alt: inside only.",
        "Cadre des objets qu'il touche. Ctrl : ajouter, Alt : objets contenus.",
    ),
    (
        "seltool.hint.band_inside",
        "Box of the objects fully inside it. Ctrl: add, Alt: touching.",
        "Cadre des objets entièrement contenus. Ctrl : ajouter, Alt : objets touchés.",
    ),
    (
        "seltool.hint.band_centers",
        "Box of the objects whose center is in it. Ctrl: add.",
        "Cadre des objets dont le centre y est. Ctrl : ajouter.",
    ),
    (
        "seltool.hint.paint_add",
        "Paint selection: sweep over objects to add them (Ctrl+Shift+drag removes).",
        "Sélection au pinceau : survolez les objets pour les ajouter (Ctrl+Maj+glisser retire).",
    ),
    (
        "seltool.hint.paint_remove",
        "Paint selection: sweep over objects to remove them from the selection.",
        "Sélection au pinceau : survolez les objets pour les retirer de la sélection.",
    ),
    (
        "seltool.hint.pivot",
        "Drag the pivot to move it (Alt: no snap). Rotations and scales turn around it.",
        "Glissez le pivot pour le déplacer (Alt : sans magnétisme). Les rotations et mises à l'échelle tournent autour de lui.",
    ),
    (
        "seltool.hint.pivot_drag",
        "Pivot at {0}. Alt: no snap, Esc: cancel.",
        "Pivot en {0}. Alt : sans magnétisme, Échap : annuler.",
    ),
    (
        "seltool.hint.move",
        "Move by {0}. Shift: clone, Alt: no snap, Esc: cancel.",
        "Déplacement de {0}. Maj : cloner, Alt : sans magnétisme, Échap : annuler.",
    ),
    (
        "seltool.hint.clone",
        "Clone with offset {0}. Release to place the copy, Esc: cancel.",
        "Clonage avec un décalage de {0}. Relâchez pour placer la copie, Échap : annuler.",
    ),
    (
        "seltool.hint.scale",
        "Scale to {0}. Ctrl: keep proportions (corner) or scale from the center (side), Shift: clone, Alt: no snap, Esc: cancel.",
        "Mise à l'échelle : {0}. Ctrl : garder les proportions (coin) ou depuis le centre (côté), Maj : cloner, Alt : sans magnétisme, Échap : annuler.",
    ),
    (
        "seltool.hint.stretch",
        "Drag to stretch the selection along {0}: the opposite face stays. Ctrl: from the center, Shift: clone, Alt: no snap.",
        "Glissez pour étirer la sélection sur {0} : la face opposée reste en place. Ctrl : depuis le centre, Maj : cloner, Alt : sans magnétisme.",
    ),
    (
        "seltool.hint.stretch_drag",
        "Stretch to {0}. Ctrl: from the center, Shift: clone, Alt: no snap, Esc: cancel.",
        "Étirement : {0}. Ctrl : depuis le centre, Maj : cloner, Alt : sans magnétisme, Échap : annuler.",
    ),
    (
        "seltool.hint.stretch_clone",
        "Clone stretched to {0}. Release to place the copy, Esc: cancel.",
        "Clone étiré : {0}. Relâchez pour placer la copie, Échap : annuler.",
    ),
    (
        "seltool.hint.scale_clone",
        "Clone scaled to {0}. Release to place the copy, Esc: cancel.",
        "Clone mis à l'échelle : {0}. Relâchez pour placer la copie, Échap : annuler.",
    ),
    (
        "seltool.hint.rotate",
        "Rotate by {0}°. {1}: free rotation, Shift: clone, Esc: cancel.",
        "Rotation de {0}°. {1} : rotation libre, Maj : cloner, Échap : annuler.",
    ),
    (
        "seltool.hint.rotate_clone",
        "Clone rotated by {0}°. {1}: free rotation, release to place the copy, Esc: cancel.",
        "Clone pivoté de {0}°. {1} : rotation libre, relâchez pour placer la copie, Échap : annuler.",
    ),
    (
        "seltool.hint.skew",
        "Skew by {0}. Shift: clone, Alt: no snap, Esc: cancel.",
        "Inclinaison de {0}. Maj : cloner, Alt : sans magnétisme, Échap : annuler.",
    ),
    (
        "seltool.hint.skew_clone",
        "Clone skewed by {0}. Release to place the copy, Esc: cancel.",
        "Clone incliné de {0}. Relâchez pour placer la copie, Échap : annuler.",
    ),
    (
        "seltool.hint.modal_move",
        "Move {0}. X, Y, Z: lock an axis (Shift: a plane), Alt: no snap, click or Enter: apply, Esc: cancel.",
        "Déplacement {0}. X, Y, Z : bloquer un axe (Maj : un plan), Alt : sans magnétisme, clic ou Entrée : appliquer, Échap : annuler.",
    ),
    (
        "seltool.hint.modal_rotate",
        "Rotate {0}. X, Y, Z: rotate around an axis, Shift: free rotation, click or Enter: apply, Esc: cancel.",
        "Rotation {0}. X, Y, Z : tourner autour d'un axe, Maj : rotation libre, clic ou Entrée : appliquer, Échap : annuler.",
    ),
    (
        "seltool.hint.modal_scale",
        "Scale {0}. X, Y, Z: one axis (Shift: a plane), Alt: no snap, click or Enter: apply, Esc: cancel.",
        "Mise à l'échelle {0}. X, Y, Z : un seul axe (Maj : un plan), Alt : sans magnétisme, clic ou Entrée : appliquer, Échap : annuler.",
    ),
    (
        "seltool.hint.helper",
        "Drag the point to edit the {0} key (Alt: no snap).",
        "Glissez le point pour modifier la clé {0} (Alt : sans magnétisme).",
    ),
    (
        "seltool.hint.helper_drag",
        "{0}: {1}. Alt: no snap, Esc: cancel.",
        "{0} : {1}. Alt : sans magnétisme, Échap : annuler.",
    ),
    ("seltool.hint.model_scale", "Model scale: {0} (Alt+wheel).", "Échelle du modèle : {0} (Alt+molette)."),
    (
        "seltool.hint.static_scale",
        "Static props cannot be scaled: the FGD has no uniformscale key.",
        "Les props statiques ne peuvent pas être mis à l'échelle : le FGD n'a pas de clé uniformscale.",
    ),
    ("camtool.target", "Target", "Cible"),
    ("camtool.position", "Position", "Position"),
    ("camtool.yaw", "Yaw", "Lacet"),
    ("camtool.pitch", "Pitch", "Tangage"),
    ("camtool.look_at_selection", "Look at selection", "Viser la sélection"),
    (
        "camtool.look_at_selection_tip",
        "Turn the 3D camera toward the center of the selection without moving it",
        "Oriente la caméra 3D vers le centre de la sélection sans la déplacer",
    ),
    (
        "camtool.reset_tip",
        "Put the 3D camera back to its default position",
        "Remet la caméra 3D à sa position par défaut",
    ),
    (
        "camtool.hint.2d",
        "Drag the camera or its target (Shift: move both). Drag on empty space to place the camera, Shift+drag to add one. Page Up, Page Down: other cameras, Delete: remove.",
        "Glissez la caméra ou sa cible (Maj : déplacer les deux). Glissez dans le vide pour placer la caméra, Maj+glisser pour en ajouter une. Pg. préc., Pg. suiv. : autres caméras, Suppr : supprimer.",
    ),
    (
        "camtool.hint.2d_zoom",
        "Click to zoom in, Ctrl+click to zoom out. Drag the camera or its target, Shift+drag on empty space to add a camera.",
        "Cliquez pour zoomer, Ctrl+clic pour dézoomer. Glissez la caméra ou sa cible, Maj+glisser dans le vide pour ajouter une caméra.",
    ),
    ("camtool.camera_n", "Camera {0}/{1}", "Caméra {0}/{1}"),
    (
        "camtool.cameras_tip",
        "Cameras saved in the map. Shift+drag on empty space in a 2D view adds one.",
        "Caméras enregistrées dans la carte. Maj+glisser dans le vide d'une vue 2D en ajoute une.",
    ),
    ("camtool.previous", "Previous camera (Page Up)", "Caméra précédente (Pg. préc.)"),
    ("camtool.next", "Next camera (Page Down)", "Caméra suivante (Pg. suiv.)"),
    ("camtool.delete", "Delete this camera (Delete)", "Supprimer cette caméra (Suppr)"),
    (
        "camtool.hint.3d",
        "Drag to look around. Click to select (Ctrl + click: add or remove, click again: the object behind).",
        "Glissez pour orienter la vue. Cliquez pour sélectionner (Ctrl + clic : ajouter ou retirer, cliquer encore : l'objet derrière).",
    ),
    (
        "camtool.hint.drag",
        "Camera at {0}, yaw {1}°, pitch {2}°. Esc: cancel.",
        "Caméra en {0}, lacet {1}°, tangage {2}°. Échap : annuler.",
    ),
];

pub fn register_translations() {
    hp_app::i18n::register(TR);
}

pub fn tools() -> Vec<Box<dyn hp_app::Tool>> {
    vec![Box::new(SelectTool::new()), Box::new(CameraTool::new())]
}
